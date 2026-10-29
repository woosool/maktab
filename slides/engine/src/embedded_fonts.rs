//! Fonts that come inside a presentation (p:embeddedFontLst → ppt/fonts/*.fntdata): read when
//! the file opens and made available by their family names, so text looks as its author saw
//! it even without the font installed. The data may be a plain TrueType/OpenType file or an
//! Embedded OpenType (EOT) wrapper, plain or XOR-scrambled; MicroType Express–compressed EOT
//! isn't unpacked (that text uses a stand-in font). Fonts can also be embedded when saving.

use crate::opc;
use crate::pres::Presentation;
use crate::xml::{Element, Node};

pub const RT_FONT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/font";
const CT_FONT: &str = "application/x-fontdata";

/// What became of one embedded face.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddedFace { pub family: String, pub bold: bool, pub italic: bool, pub loaded: bool, pub note: String }

/// The font inside an embedded font part, if it can be read.
pub fn font_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    let sfnt = |d: &[u8]| d.len() > 12 && matches!(&d[..4], [0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf");
    if sfnt(data) { return Ok(data.to_vec()); }
    // Embedded OpenType: a header, then the font at the end.
    if data.len() > 82 && u16::from_le_bytes([data[34], data[35]]) == 0x504C {
        let le = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]) as usize;
        let (size, font_size, flags) = (le(0).min(data.len()), le(4), le(12));
        if flags & 0x4 != 0 { return Err("compressed (MicroType Express), which Sard can't unpack yet".into()); }
        let start = size.checked_sub(font_size).ok_or("damaged EOT")?;
        let mut font = data.get(start..size).ok_or("damaged EOT")?.to_vec();
        if flags & 0x1000_0000 != 0 { for b in &mut font { *b ^= 0x50; } }
        return if sfnt(&font) { Ok(font) } else { Err("not a TrueType or OpenType font".into()) };
    }
    Err("not a font Sard can read".into())
}

impl Presentation {
    /// Read the presentation's embedded fonts and make them available (called on open).
    pub fn load_embedded_fonts(&self) -> Vec<EmbeddedFace> {
        let mut out = vec![];
        let main = self.main.clone();
        let Some(list) = self.root().child("p:embeddedFontLst") else { return out };
        for ef in list.children_named("p:embeddedFont") {
            let Some(family) = ef.child("p:font").and_then(|f| f.attr("typeface")).map(String::from) else { continue };
            for (tag, bold, italic) in [("p:regular", false, false), ("p:bold", true, false), ("p:italic", false, true), ("p:boldItalic", true, true)] {
                let Some(rid) = ef.child(tag).and_then(|r| r.attr("r:id")) else { continue };
                let Some(part) = self.pkg.rel_target(&main, rid) else { continue };
                let Some(bytes) = self.pkg.bytes(&part) else { continue };
                let (loaded, note) = match font_bytes(&bytes) {
                    Ok(font) => { let ok = word_engine::fonts::register_embedded(&family, bold, italic, font); (ok, if ok { String::new() } else { "couldn't be loaded".into() }) }
                    Err(e) => (false, e),
                };
                out.push(EmbeddedFace { family: family.clone(), bold, italic, loaded, note });
            }
        }
        out
    }

    /// The embedded font list as written (family names).
    pub fn embedded_font_families(&self) -> Vec<String> {
        self.root().child("p:embeddedFontLst").map(|l| l.children_named("p:embeddedFont").filter_map(|e| e.child("p:font").and_then(|f| f.attr("typeface")).map(String::from)).collect()).unwrap_or_default()
    }

    /// Embed a font face (TrueType/OpenType bytes) in the presentation.
    pub fn embed_font(&mut self, family: &str, bold: bool, italic: bool, font: &[u8]) {
        let main = self.main.clone();
        let part = self.pkg.free_name("ppt/fonts/font", ".fntdata");
        self.pkg.put_bytes(&part, font.to_vec());
        self.pkg.set_override(&format!("/{part}"), CT_FONT);
        let rid = self.pkg.add_rel(&main, RT_FONT, &part, false);
        let tag = match (bold, italic) { (false, false) => "p:regular", (true, false) => "p:bold", (false, true) => "p:italic", (true, true) => "p:boldItalic" };
        let root = self.root_mut();
        let list = root.ensure_at("p:embeddedFontLst", &["p:custShowLst", "p:photoAlbum", "p:custDataLst", "p:kinsoku", "p:defaultTextStyle", "p:modifyVerifier", "p:extLst"]);
        let pos = list.children.iter().position(|n| matches!(n, Node::Element(e) if e.child("p:font").and_then(|f| f.attr("typeface")) == Some(family)));
        let entry = match pos {
            Some(i) => match &mut list.children[i] { Node::Element(e) => e, _ => unreachable!() },
            None => {
                list.children.push(Node::Element(Element::new("p:embeddedFont").with_child(Element::new("p:font").with_attr("typeface", family))));
                match list.children.last_mut() { Some(Node::Element(e)) => e, _ => unreachable!() }
            }
        };
        entry.remove_children(tag);
        // Faces in their schema order: regular, bold, italic, boldItalic.
        let order = ["p:font", "p:regular", "p:bold", "p:italic", "p:boldItalic"];
        let at = entry.children.iter().position(|n| matches!(n, Node::Element(e) if order.iter().position(|o| *o == e.name) > order.iter().position(|o| *o == tag))).unwrap_or(entry.children.len());
        entry.children.insert(at, Node::Element(Element::new(tag).with_attr("r:id", rid)));
        let _ = opc::RT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caladea() -> Vec<u8> { std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../word/assets/fonts/Caladea-Regular.ttf")).unwrap() }

    #[test]
    fn embedded_fonts_are_read_and_used() {
        let ttf = caladea();
        // An EOT wrapper, XOR-scrambled.
        let mut eot = vec![0u8; 82];
        let total = (82 + ttf.len()) as u32;
        eot[0..4].copy_from_slice(&total.to_le_bytes());
        eot[4..8].copy_from_slice(&(ttf.len() as u32).to_le_bytes());
        eot[8..12].copy_from_slice(&0x0002_0001u32.to_le_bytes());
        eot[12..16].copy_from_slice(&0x1000_0000u32.to_le_bytes());
        eot[34..36].copy_from_slice(&0x504Cu16.to_le_bytes());
        eot.extend(ttf.iter().map(|b| b ^ 0x50));
        assert_eq!(font_bytes(&eot).unwrap(), ttf);
        let mut mtx = eot.clone();
        mtx[12..16].copy_from_slice(&0x4u32.to_le_bytes());
        assert!(font_bytes(&mtx).is_err());
        assert!(font_bytes(b"not a font at all, really").is_err());

        let mut p = Presentation::new_blank(true);
        p.embed_font("Sard Test Serif", false, false, &ttf);
        p.embed_font("Sard Test Serif", true, false, &ttf);
        let back = Presentation::open(&p.save().unwrap()).unwrap();
        assert_eq!(back.embedded_font_families(), vec!["Sard Test Serif".to_string()]);
        let faces = back.load_embedded_fonts();
        assert_eq!(faces.len(), 2);
        assert!(faces.iter().all(|f| f.loaded), "{faces:?}");
        // The family now resolves to the embedded face, not a stand-in.
        let k = word_engine::fonts::resolve("Sard Test Serif", false, false);
        assert_eq!(word_engine::fonts::face(k.face).family, "Sard Test Serif");
        assert_eq!(word_engine::fonts::face(k.face).file, "embedded");
    }
}
