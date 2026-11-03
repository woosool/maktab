//! Bundled fonts, text shaping and embedding as subset Type0 (CID) fonts.
//!
//! New text (page text, text boxes, typed signatures) is shaped with rustybuzz,
//! ordered with the Unicode bidi algorithm, and written with glyph ids into
//! subset TrueType fonts with a ToUnicode map, so any script the bundled fonts
//! cover (Latin, Arabic, …) renders, searches and copies correctly.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use lopdf::{Dictionary, Object, Stream, dictionary};
use rustybuzz::{Direction, Face, UnicodeBuffer};
use subsetter::GlyphRemapper;

/// Font faces offered for new text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontFace {
    Sans,
    SansBold,
    Serif,
    Script,
}

impl FontFace {
    pub const ALL: [FontFace; 4] = [
        FontFace::Sans,
        FontFace::SansBold,
        FontFace::Serif,
        FontFace::Script,
    ];
    pub fn label(self) -> &'static str {
        match self {
            FontFace::Sans => "Sans (Liberation Sans)",
            FontFace::SansBold => "Sans Bold",
            FontFace::Serif => "Serif (Liberation Serif)",
            FontFace::Script => "Script (Great Vibes)",
        }
    }
    pub(crate) fn key(self) -> &'static str {
        match self {
            FontFace::Sans => "Sans",
            FontFace::SansBold => "SansBold",
            FontFace::Serif => "Serif",
            FontFace::Script => "Script",
        }
    }
    pub(crate) fn from_key(k: &[u8]) -> Self {
        match k {
            b"SansBold" => FontFace::SansBold,
            b"Serif" => FontFace::Serif,
            b"Script" => FontFace::Script,
            _ => FontFace::Sans,
        }
    }
    fn index(self) -> usize {
        self as usize
    }
    /// The bundled font program (TrueType), e.g. for the UI to preview it.
    pub fn data(self) -> &'static [u8] {
        FONTS[self.index()].1
    }
}

/// Index 4 is the Arabic fallback used for characters the chosen face lacks.
const FONTS: [(&str, &[u8]); 5] = [
    (
        "LiberationSans",
        include_bytes!("../assets/fonts/LiberationSans-Regular.ttf"),
    ),
    (
        "LiberationSans-Bold",
        include_bytes!("../assets/fonts/LiberationSans-Bold.ttf"),
    ),
    (
        "LiberationSerif",
        include_bytes!("../assets/fonts/LiberationSerif-Regular.ttf"),
    ),
    (
        "GreatVibes-Regular",
        include_bytes!("../assets/fonts/GreatVibes-Regular.ttf"),
    ),
    (
        "NotoSansArabic",
        include_bytes!("../assets/fonts/NotoSansArabic.ttf"),
    ),
];
const FALLBACK: usize = 4;

/// The bundled Arabic font used when a face lacks a character.
pub fn fallback_font_data() -> &'static [u8] {
    FONTS[FALLBACK].1
}

fn faces() -> &'static [Face<'static>] {
    static FACES: OnceLock<Vec<Face<'static>>> = OnceLock::new();
    FACES.get_or_init(|| {
        FONTS
            .iter()
            .map(|(_, data)| Face::from_slice(data, 0).expect("bundled font parses"))
            .collect()
    })
}

/// One glyph placed on a line.
#[derive(Clone, Debug)]
struct Placed {
    font: usize,
    gid: u16,
    /// Pen position from the line start, in points.
    x: f32,
    y_off: f32,
    /// Text this glyph stands for (first glyph of a cluster carries it).
    text: String,
    /// Byte offset of its cluster in the line: the logical (reading) order.
    order: usize,
}

struct Shaped {
    glyphs: Vec<Placed>,
    width: f32,
}

fn covers(font: usize, c: char) -> bool {
    c.is_control() || faces()[font].glyph_index(c).is_some()
}

/// Shapes one line of text in visual order.
fn shape_line(text: &str, face: FontFace, size: f32) -> Shaped {
    let mut glyphs = Vec::new();
    let mut pen = 0.0f32;
    if text.is_empty() {
        return Shaped { glyphs, width: 0.0 };
    }
    let bidi = unicode_bidi::BidiInfo::new(text, None);
    let para = &bidi.paragraphs[0];
    let (levels, runs) = bidi.visual_runs(para, para.range.clone());
    for run in runs {
        let rtl = levels[run.start].is_rtl();
        // Split the run where the font changes (logical order).
        let mut segments: Vec<(usize, std::ops::Range<usize>)> = Vec::new();
        for (i, c) in text[run.clone()].char_indices() {
            let i = i + run.start;
            let font = if covers(face.index(), c) || !covers(FALLBACK, c) {
                face.index()
            } else {
                FALLBACK
            };
            match segments.last_mut() {
                Some((f, r)) if *f == font || c.is_whitespace() => r.end = i + c.len_utf8(),
                _ => segments.push((font, i..i + c.len_utf8())),
            }
        }
        if rtl {
            segments.reverse();
        }
        for (font, range) in segments {
            let f = &faces()[font];
            let scale = size / f.units_per_em() as f32;
            let seg = &text[range.clone()];
            let mut buf = UnicodeBuffer::new();
            buf.push_str(seg);
            buf.set_direction(if rtl {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            });
            let out = rustybuzz::shape(f, &[], buf);
            let infos = out.glyph_infos();
            let positions = out.glyph_positions();
            // Cluster -> text, so ToUnicode maps each cluster once.
            let mut starts: Vec<usize> = infos.iter().map(|g| g.cluster as usize).collect();
            starts.sort_unstable();
            starts.dedup();
            let mut seen = std::collections::HashSet::new();
            for (info, pos) in infos.iter().zip(positions) {
                let cl = info.cluster as usize;
                let text_of = if seen.insert(cl) {
                    let end = starts
                        .iter()
                        .find(|&&s| s > cl)
                        .copied()
                        .unwrap_or(seg.len());
                    seg[cl..end].to_string()
                } else {
                    String::new()
                };
                glyphs.push(Placed {
                    font,
                    gid: info.glyph_id as u16,
                    x: pen + pos.x_offset as f32 * scale,
                    y_off: pos.y_offset as f32 * scale,
                    text: text_of,
                    order: range.start + cl,
                });
                pen += pos.x_advance as f32 * scale;
            }
        }
    }
    Shaped { glyphs, width: pen }
}

/// Width (1000 units per em) of `code` in WinAnsi encoding, using a bundled face.
pub(crate) fn win_ansi_width(face: FontFace, code: u32) -> Option<f32> {
    const HIGH: [u16; 32] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
        0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
        0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
    ];
    let ch = match code {
        0..=127 | 160..=255 => char::from_u32(code)?,
        128..=159 => char::from_u32(u32::from(HIGH[(code - 128) as usize]))?,
        _ => return None,
    };
    let f = &faces()[face.index()];
    let gid = f
        .glyph_index(ch)
        .unwrap_or(rustybuzz::ttf_parser::GlyphId(0));
    Some(f.glyph_hor_advance(gid)? as f32 * 1000.0 / f.units_per_em() as f32)
}

/// Whether a line's base direction is right to left (its first strong character is RTL).
pub(crate) fn is_rtl(line: &str) -> bool {
    let info = unicode_bidi::BidiInfo::new(line, None);
    info.paragraphs.first().is_some_and(|p| p.level.is_rtl())
}

/// Glyph ids of a shaped line in visual order (for tests of joining).
#[cfg(test)]
pub(crate) fn shaped_gids(text: &str, face: FontFace) -> Vec<u16> {
    shape_line(text, face, 10.0)
        .glyphs
        .iter()
        .map(|g| g.gid)
        .collect()
}

/// Width of the widest line and the line count.
pub(crate) fn measure(text: &str, face: FontFace, size: f32) -> (f32, usize) {
    let lines: Vec<&str> = text.split('\n').collect();
    let w = lines
        .iter()
        .map(|l| shape_line(l, face, size).width)
        .fold(0.0, f32::max);
    (w, lines.len())
}

/// Line height used for multi-line text.
pub(crate) fn leading(size: f32) -> f32 {
    size * 1.25
}

/// Glyph remapper for one font plus new gid -> (original gid, text).
type UsedFont = (GlyphRemapper, BTreeMap<u16, (u16, String)>);

/// Collects shaped text into content-stream operators and the fonts they need.
#[derive(Default)]
pub(crate) struct TextPainter {
    /// Write glyphs left to right instead (as many other producers do); used by tests.
    pub(crate) visual_order: bool,
    content: String,
    /// Per font: remapper and new gid -> (old gid, text).
    used: BTreeMap<usize, UsedFont>,
}

impl TextPainter {
    /// Paints `text` (may contain newlines) with the first baseline at `(x, y)` in
    /// a y-down space.
    pub(crate) fn text(
        &mut self,
        text: &str,
        face: FontFace,
        size: f32,
        x: f32,
        y: f32,
        rgb: [f32; 3],
    ) {
        self.paint(text, face, size, x, None, y, rgb);
    }

    /// Like `text`, but lines whose base direction is right to left (Arabic) are aligned to
    /// `right` instead of starting at `left`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_in_box(
        &mut self,
        text: &str,
        face: FontFace,
        size: f32,
        left: f32,
        right: f32,
        y: f32,
        rgb: [f32; 3],
    ) {
        self.paint(text, face, size, left, Some(right), y, rgb);
    }

    #[allow(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        text: &str,
        face: FontFace,
        size: f32,
        x: f32,
        right: Option<f32>,
        y: f32,
        rgb: [f32; 3],
    ) {
        let _ = writeln!(
            self.content,
            "{} {} {} rg",
            num(rgb[0]),
            num(rgb[1]),
            num(rgb[2])
        );
        self.content.push_str("BT\n");
        for (i, line) in text.split('\n').enumerate() {
            let shaped = shape_line(line, face, size);
            let base = y + i as f32 * leading(size);
            let x = match right {
                Some(r) if is_rtl(line) => r - shaped.width,
                _ => x,
            };
            let mut current: Option<usize> = None;
            // Write glyphs in reading order (each has its own position), so text extraction
            // and search see Arabic in logical order like any other text.
            let mut glyphs = shaped.glyphs;
            if self.visual_order {
                glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
            } else {
                glyphs.sort_by_key(|g| g.order);
            }
            for g in glyphs {
                let (remap, map) = self.used.entry(g.font).or_insert_with(|| {
                    let mut r = GlyphRemapper::new();
                    r.remap(0);
                    (r, BTreeMap::new())
                });
                let new = remap.remap(g.gid);
                let entry = map.entry(new).or_insert((g.gid, String::new()));
                if entry.1.is_empty() {
                    entry.1 = g.text.clone();
                }
                if current != Some(g.font) {
                    let _ = writeln!(self.content, "/MkF{} {} Tf", g.font, num(size));
                    current = Some(g.font);
                }
                let _ = writeln!(
                    self.content,
                    "1 0 0 -1 {} {} Tm <{new:04X}> Tj",
                    num(x + g.x),
                    num(base - g.y_off)
                );
            }
        }
        self.content.push_str("ET\n");
    }

    /// The operators written so far.
    pub(crate) fn take_content(&mut self) -> String {
        std::mem::take(&mut self.content)
    }

    /// Embeds the subset fonts and returns the /Font resource dictionary.
    pub(crate) fn fonts(self, doc: &mut lopdf::Document) -> Dictionary {
        let mut dict = Dictionary::new();
        for (font, (remap, map)) in self.used {
            let (name, data) = FONTS[font];
            let face = &faces()[font];
            let Ok(subset) = subsetter::subset(data, 0, &remap) else {
                continue;
            };
            let upem = face.units_per_em() as f32;
            let k = 1000.0 / upem;
            let mut file = Stream::new(dictionary! { "Length1" => subset.len() as i64 }, subset);
            let _ = file.compress();
            let file = doc.add_object(file);
            // A stable six-letter subset tag from the glyph set.
            let mut h: u32 = 2166136261;
            for g in map.keys() {
                h = (h ^ u32::from(*g)).wrapping_mul(16777619);
            }
            let tag: String = (0..6)
                .map(|i| (b'A' + ((h >> (i * 5)) % 26) as u8) as char)
                .collect();
            let base = format!("{tag}+{name}");
            let bbox = face.global_bounding_box();
            let descriptor = doc.add_object(dictionary! {
                "Type" => "FontDescriptor",
                "FontName" => Object::Name(base.clone().into_bytes()),
                "Flags" => 4,
                "FontBBox" => vec![
                    Object::Real(bbox.x_min as f32 * k), Object::Real(bbox.y_min as f32 * k),
                    Object::Real(bbox.x_max as f32 * k), Object::Real(bbox.y_max as f32 * k),
                ],
                "ItalicAngle" => 0,
                "Ascent" => Object::Real(face.ascender() as f32 * k),
                "Descent" => Object::Real(face.descender() as f32 * k),
                "CapHeight" => Object::Real(face.capital_height().unwrap_or(700) as f32 * k),
                "StemV" => 80,
                "FontFile2" => file,
            });
            let mut widths = Vec::new();
            for (new, (old, _)) in &map {
                let adv = face
                    .glyph_hor_advance(rustybuzz::ttf_parser::GlyphId(*old))
                    .unwrap_or(0);
                widths.push(Object::Integer(i64::from(*new)));
                widths.push(Object::Array(vec![Object::Real(adv as f32 * k)]));
            }
            let cid = doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "CIDFontType2",
                "BaseFont" => Object::Name(base.clone().into_bytes()),
                "CIDSystemInfo" => dictionary! {
                    "Registry" => Object::string_literal("Adobe"),
                    "Ordering" => Object::string_literal("Identity"),
                    "Supplement" => 0,
                },
                "FontDescriptor" => descriptor,
                "DW" => 1000,
                "W" => widths,
                "CIDToGIDMap" => "Identity",
            });
            let mut to_unicode = Stream::new(Dictionary::new(), to_unicode_cmap(&map).into_bytes());
            let _ = to_unicode.compress();
            let to_unicode = doc.add_object(to_unicode);
            let type0 = doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type0",
                "BaseFont" => Object::Name(base.into_bytes()),
                "Encoding" => "Identity-H",
                "DescendantFonts" => vec![Object::Reference(cid)],
                "ToUnicode" => to_unicode,
            });
            dict.set(format!("MkF{font}"), type0);
        }
        dict
    }
}

fn to_unicode_cmap(map: &BTreeMap<u16, (u16, String)>) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(u16, &String)> = map
        .iter()
        .filter(|(_, (_, t))| !t.is_empty())
        .map(|(k, (_, t))| (*k, t))
        .collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (cid, text) in chunk {
            let hex: String = text.encode_utf16().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(s, "<{cid:04X}> <{hex}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

pub(crate) fn num(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_latin_and_arabic() {
        let latin = shape_line("Hello", FontFace::Sans, 10.0);
        assert_eq!(latin.glyphs.len(), 5);
        assert!(latin.width > 20.0 && latin.width < 30.0, "{}", latin.width);
        let arabic = shape_line("مرحبا", FontFace::Sans, 10.0);
        assert!(
            arabic
                .glyphs
                .iter()
                .all(|g| g.font == FALLBACK && g.gid != 0)
        );
        // Visual order: the first logical letter (meem) ends up rightmost.
        let first = arabic
            .glyphs
            .iter()
            .find(|g| g.text.starts_with('م'))
            .unwrap();
        assert!(first.x >= arabic.glyphs.iter().map(|g| g.x).fold(0.0, f32::max) - 0.01);
    }
}
