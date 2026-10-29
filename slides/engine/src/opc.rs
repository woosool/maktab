//! The Open Packaging Convention container: a zip of parts (XML or binary), their content
//! types, and the relationships between them. XML parts are kept parsed; everything else is
//! kept as bytes, so unknown parts (charts' workbooks, VBA, custom XML) save back unchanged.

use crate::xml::{Document, Element, Node};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::Arc;

pub const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const RT_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
pub const RT_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
pub const RT_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
pub const RT_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
pub const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub const RT_NOTES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";
pub const RT_NOTES_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster";
pub const RT_HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
pub const RT_OFFICE_DOC: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const PKG_RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

pub const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
pub const CT_NOTES: &str = "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml";

#[derive(Clone, Debug)]
pub enum PartData {
    /// Shared until changed (undo steps share every part they didn't touch).
    Xml(Arc<Document>),
    Bytes(Arc<Vec<u8>>),
}

#[derive(Clone, Debug, Default)]
pub struct Package {
    /// Part names without the leading slash ("ppt/slides/slide1.xml"), in zip order.
    pub parts: BTreeMap<String, PartData>,
    order: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rel { pub id: String, pub kind: String, pub target: String, pub external: bool }

/// The relationships part for a part ("ppt/slides/_rels/slide1.xml.rels"); "" is the package.
pub fn rels_name(part: &str) -> String {
    match part.rfind('/') { Some(i) => format!("{}/_rels/{}.rels", &part[..i], &part[i + 1..]), None => format!("_rels/{part}.rels") }
}

/// Resolve a relationship target against the part that holds it.
pub fn resolve(source: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') { return abs.to_string(); }
    let mut segs: Vec<&str> = source.split('/').collect();
    segs.pop();
    for s in target.split('/') { match s { ".." => { segs.pop(); } "." | "" => {} s => segs.push(s) } }
    segs.join("/")
}

/// A target for `to`, relative to the part `from`.
pub fn relative(from: &str, to: &str) -> String {
    let a: Vec<&str> = from.split('/').collect();
    let b: Vec<&str> = to.split('/').collect();
    let dir = &a[..a.len() - 1];
    let common = dir.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let mut out: Vec<String> = std::iter::repeat_n("..".to_string(), dir.len() - common).collect();
    out.extend(b[common..].iter().map(|s| s.to_string()));
    out.join("/")
}

fn is_xml(name: &str) -> bool { name.ends_with(".xml") || name.ends_with(".rels") || name.ends_with(".vml") }

impl Package {
    pub fn open(bytes: &[u8]) -> Result<Package, String> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("Not a .pptx file ({e})"))?;
        let mut pkg = Package::default();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            if f.is_dir() { continue; }
            let name = f.name().trim_start_matches('/').to_string();
            let mut data = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut data).map_err(|e| e.to_string())?;
            let part = if is_xml(&name) { match Document::parse(&data) { Ok(d) => PartData::Xml(Arc::new(d)), Err(_) => PartData::Bytes(Arc::new(data)) } } else { PartData::Bytes(Arc::new(data)) };
            pkg.order.push(name.clone());
            pkg.parts.insert(name, part);
        }
        if !pkg.parts.contains_key("[Content_Types].xml") { return Err("Not a .pptx file (no content types)".into()); }
        Ok(pkg)
    }

    pub fn save(&self) -> Result<Vec<u8>, String> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            // [Content_Types].xml first, as Office expects; then the original order, then new parts.
            let mut names: Vec<&String> = vec![];
            if self.parts.contains_key("[Content_Types].xml") { names.push(self.order.iter().find(|n| *n == "[Content_Types].xml").unwrap_or(&self.order[0])); }
            for n in &self.order { if n != "[Content_Types].xml" && self.parts.contains_key(n) { names.push(n); } }
            let extra: Vec<&String> = self.parts.keys().filter(|k| !self.order.contains(k)).collect();
            names.extend(extra);
            for name in names {
                let Some(part) = self.parts.get(name) else { continue };
                let bytes: std::borrow::Cow<[u8]> = match part { PartData::Xml(d) => d.to_bytes().into(), PartData::Bytes(b) => b.as_slice().into() };
                // Media is already compressed.
                let o = if name.starts_with("ppt/media/") { opts.compression_method(zip::CompressionMethod::Stored) } else { opts };
                zip.start_file(name.as_str(), o).map_err(|e| e.to_string())?;
                zip.write_all(&bytes).map_err(|e| e.to_string())?;
            }
            zip.finish().map_err(|e| e.to_string())?;
        }
        Ok(buf.into_inner())
    }

    /// Whether two packages hold the same copy of a part (an undo step shares it).
    pub fn shares_part(&self, other: &Package, name: &str) -> bool {
        match (self.parts.get(name), other.parts.get(name)) { (Some(PartData::Xml(a)), Some(PartData::Xml(b))) => Arc::ptr_eq(a, b), (Some(PartData::Bytes(a)), Some(PartData::Bytes(b))) => Arc::ptr_eq(a, b), _ => false }
    }
    pub fn xml(&self, name: &str) -> Option<&Element> { match self.parts.get(name)? { PartData::Xml(d) => Some(&d.root), _ => None } }
    pub fn xml_mut(&mut self, name: &str) -> Option<&mut Element> { match self.parts.get_mut(name)? { PartData::Xml(d) => Some(&mut Arc::make_mut(d).root), _ => None } }
    pub fn bytes(&self, name: &str) -> Option<Arc<Vec<u8>>> {
        match self.parts.get(name)? { PartData::Bytes(b) => Some(b.clone()), PartData::Xml(d) => Some(Arc::new(d.to_bytes())) }
    }
    pub fn has(&self, name: &str) -> bool { self.parts.contains_key(name) }

    pub fn put_xml(&mut self, name: &str, root: Element, content_type: Option<&str>) {
        self.parts.insert(name.to_string(), PartData::Xml(Arc::new(Document::new(root))));
        if let Some(ct) = content_type { self.set_override(name, ct); }
    }
    pub fn put_bytes(&mut self, name: &str, data: Vec<u8>) {
        self.parts.insert(name.to_string(), PartData::Bytes(Arc::new(data)));
        if let Some(ext) = name.rsplit('.').next() { self.ensure_default(ext); }
    }
    pub fn remove(&mut self, name: &str) {
        self.parts.remove(name);
        self.parts.remove(&rels_name(name));
        if let Some(ct) = self.xml_mut("[Content_Types].xml") { ct.children.retain(|n| !matches!(n, Node::Element(e) if e.attr("PartName") == Some(&format!("/{name}")))); }
    }

    // ---- content types --------------------------------------------------------

    pub fn content_type(&self, name: &str) -> Option<String> {
        let ct = self.xml("[Content_Types].xml")?;
        if let Some(o) = ct.elements().find(|e| e.local() == "Override" && e.attr("PartName").map(|p| p.trim_start_matches('/')) == Some(name)) { return o.attr("ContentType").map(String::from); }
        let ext = name.rsplit('.').next()?.to_ascii_lowercase();
        ct.elements().find(|e| e.local() == "Default" && e.attr("Extension").map(|x| x.to_ascii_lowercase()) == Some(ext.clone())).and_then(|d| d.attr("ContentType").map(String::from))
    }
    pub fn set_override(&mut self, name: &str, content_type: &str) {
        let Some(ct) = self.xml_mut("[Content_Types].xml") else { return };
        let pn = format!("/{name}");
        if let Some(o) = ct.elements_mut().find(|e| e.local() == "Override" && e.attr("PartName") == Some(&pn)) { o.set_attr("ContentType", content_type); return; }
        ct.children.push(Node::Element(Element::new("Override").with_attr("PartName", pn).with_attr("ContentType", content_type)));
    }
    pub fn ensure_default(&mut self, ext: &str) {
        let ext = ext.to_ascii_lowercase();
        let mime = match ext.as_str() { "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", "svg" => "image/svg+xml", "tif" | "tiff" => "image/tiff", "emf" => "image/x-emf", "wmf" => "image/x-wmf", "mp4" => "video/mp4", "m4a" => "audio/mp4", "mp3" => "audio/mpeg", "wav" => "audio/wav", "xml" => "application/xml", "rels" => "application/vnd.openxmlformats-package.relationships+xml", _ => return };
        let Some(ct) = self.xml_mut("[Content_Types].xml") else { return };
        if ct.elements().any(|e| e.local() == "Default" && e.attr("Extension").map(|x| x.eq_ignore_ascii_case(&ext)) == Some(true)) { return; }
        let pos = ct.children.iter().position(|n| matches!(n, Node::Element(e) if e.local() == "Override")).unwrap_or(ct.children.len());
        ct.children.insert(pos, Node::Element(Element::new("Default").with_attr("Extension", ext).with_attr("ContentType", mime)));
    }

    // ---- relationships --------------------------------------------------------

    pub fn rels(&self, part: &str) -> Vec<Rel> {
        let Some(r) = self.xml(&rels_name(part)) else { return vec![] };
        r.elements().filter(|e| e.local() == "Relationship").map(|e| Rel {
            id: e.attr("Id").unwrap_or("").to_string(),
            kind: e.attr("Type").unwrap_or("").to_string(),
            target: e.attr("Target").unwrap_or("").to_string(),
            external: e.attr("TargetMode") == Some("External"),
        }).collect()
    }
    /// The part a relationship id points to.
    pub fn rel_target(&self, part: &str, id: &str) -> Option<String> {
        self.rels(part).into_iter().find(|r| r.id == id && !r.external).map(|r| resolve(part, &r.target))
    }
    pub fn rel_external(&self, part: &str, id: &str) -> Option<String> { self.rels(part).into_iter().find(|r| r.id == id).map(|r| r.target) }
    /// The first part related by type.
    pub fn related(&self, part: &str, kind: &str) -> Option<String> {
        self.rels(part).into_iter().find(|r| r.kind == kind && !r.external).map(|r| resolve(part, &r.target))
    }
    /// Add a relationship (reusing an equal one); returns its id.
    pub fn add_rel(&mut self, part: &str, kind: &str, target: &str, external: bool) -> String {
        let rn = rels_name(part);
        let existing = self.rels(part);
        let tgt = if external { target.to_string() } else { relative(part, target) };
        if let Some(r) = existing.iter().find(|r| r.kind == kind && r.target == tgt && r.external == external) { return r.id.clone(); }
        let mut n = existing.len() + 1;
        while existing.iter().any(|r| r.id == format!("rId{n}")) { n += 1; }
        let id = format!("rId{n}");
        if !self.has(&rn) { self.put_xml(&rn, Element::new("Relationships").with_attr("xmlns", PKG_RELS_NS), None); }
        let mut el = Element::new("Relationship").with_attr("Id", id.clone()).with_attr("Type", kind).with_attr("Target", tgt);
        if external { el.set_attr("TargetMode", "External"); }
        self.xml_mut(&rn).unwrap().children.push(Node::Element(el));
        id
    }
    pub fn remove_rel(&mut self, part: &str, id: &str) {
        if let Some(r) = self.xml_mut(&rels_name(part)) { r.children.retain(|n| !matches!(n, Node::Element(e) if e.attr("Id") == Some(id))); }
    }

    /// A free part name like "ppt/slides/slide7.xml" for the pattern ("ppt/slides/slide", ".xml").
    pub fn free_name(&self, prefix: &str, ext: &str) -> String {
        (1..).map(|i| format!("{prefix}{i}{ext}")).find(|n| !self.has(n)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(rels_name("ppt/slides/slide1.xml"), "ppt/slides/_rels/slide1.xml.rels");
        assert_eq!(resolve("ppt/slides/slide1.xml", "../slideLayouts/slideLayout2.xml"), "ppt/slideLayouts/slideLayout2.xml");
        assert_eq!(resolve("ppt/presentation.xml", "slides/slide1.xml"), "ppt/slides/slide1.xml");
        assert_eq!(relative("ppt/slides/slide1.xml", "ppt/media/image1.png"), "../media/image1.png");
        assert_eq!(relative("ppt/presentation.xml", "ppt/slides/slide2.xml"), "slides/slide2.xml");
    }
}
