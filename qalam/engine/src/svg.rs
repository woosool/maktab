//! SVG rendering of laid-out pages with glyph outlines from the bundled
//! fonts, so the output matches layout exactly (used for previews, tests and
//! as the basis for print export).

use crate::fonts::{FontKey};
use crate::layout::{DrawItem, Layout};
use crate::model::Document;
use std::collections::HashMap;
use std::fmt::Write;

struct PathSink(String);

impl ttf_parser::OutlineBuilder for PathSink {
    fn move_to(&mut self, x: f32, y: f32) { let _ = write!(self.0, "M{x} {y}"); }
    fn line_to(&mut self, x: f32, y: f32) { let _ = write!(self.0, "L{x} {y}"); }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { let _ = write!(self.0, "Q{x1} {y1} {x} {y}"); }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { let _ = write!(self.0, "C{x1} {y1} {x2} {y2} {x} {y}"); }
    fn close(&mut self) { self.0.push('Z'); }
}

fn hex(c: crate::Color) -> String { format!("#{}", c.hex()) }

/// One page as a standalone SVG document. Pictures are embedded as data URLs.
pub fn page_svg(doc: &Document, layout: &Layout, page: usize) -> String {
    let p = &layout.pages[page];
    let items: Vec<&DrawItem> = p.under.iter().chain(p.items.iter()).chain(p.over.iter()).collect();
    items_svg(Some(doc), &items, p.width, p.height, 0.0, 0.0)
}

/// Draw items as an SVG of size w×h whose origin is (ox, oy) in item coordinates.
pub fn items_svg(doc: Option<&Document>, items: &[&DrawItem], w: f32, h: f32, ox: f32, oy: f32) -> String {
    let p = crate::layout::PageBox { width: w, height: h, items: vec![], under: vec![], links: vec![], over: vec![] };
    let mut defs = String::new();
    let mut body = String::new();
    let mut glyph_ids: HashMap<(FontKey, u16), Option<String>> = HashMap::new();
    let mut faces: HashMap<u16, ttf_parser::Face<'static>> = HashMap::new();
    let mut draw = |item: &DrawItem, body: &mut String, defs: &mut String| match item {
        DrawItem::Frame { .. } => {}
        DrawItem::Path { points, closed, fill, stroke, .. } => {
            let d: String = points.iter().enumerate().map(|(i, p)| format!("{}{} {} ", if i == 0 { "M" } else { "L" }, p.0, p.1)).collect::<String>() + if *closed { "Z" } else { "" };
            let fill = fill.map(hex).unwrap_or("none".into());
            let stroke = stroke.map(|(c, w)| format!(r#" stroke="{}" stroke-width="{w}" stroke-linejoin="round""#, hex(c))).unwrap_or_default();
            let _ = write!(body, r#"<path d="{d}" fill="{fill}"{stroke}/>"#);
        }
        DrawItem::Rect { x, y, w, h, color } => { let _ = write!(body, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{}"/>"#, hex(*color)); }
        DrawItem::Line { x1, y1, x2, y2, width, color } => { let _ = write!(body, r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{}" stroke-width="{width}"/>"#, hex(*color)); }
        DrawItem::Image { asset, x, y, w, h, rot, flip, .. } => {
            if let Some(a) = doc.and_then(|d| d.assets.get(asset)) {
                let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                let mirror = if flip.0 || flip.1 { format!(" translate({cx} {cy}) scale({} {}) translate({} {})", if flip.0 { -1 } else { 1 }, if flip.1 { -1 } else { 1 }, -cx, -cy) } else { String::new() };
                let t = if *rot != 0.0 || !mirror.is_empty() { format!(r#" transform="rotate({rot} {cx} {cy}){mirror}""#) } else { String::new() };
                let _ = write!(body, r#"<image x="{x}" y="{y}" width="{w}" height="{h}"{t} preserveAspectRatio="none" href="data:{};base64,{}"/>"#, a.content_type, base64(&a.bytes));
            }
        }
        DrawItem::Glyphs { glyphs, baseline, font, size, color, effects } => {
            let face = faces.entry(font.face).or_insert_with(|| {
                let data = crate::fonts::face(font.face);
                let mut f = ttf_parser::Face::parse(data.bytes(), data.index).unwrap();
                if data.weight > 0.0 { f.set_variation(ttf_parser::Tag::from_bytes(b"wght"), data.weight); }
                f
            });
            let scale = size / face.units_per_em() as f32;
            if let Some((a, cx, cy)) = effects.rotate { let _ = write!(body, r#"<g transform="rotate({a} {cx} {cy})">"#); }
            let color = &effects.gradient.map(|g| g.0).unwrap_or(*color);
            let paint = if effects.outline { format!(r#"fill="white" stroke="{}" stroke-width="{}""#, hex(*color), size / 30.0 / scale) } else { format!(r#"fill="{}""#, hex(*color)) };
            if effects.shadow {
                let _ = write!(body, r##"<g fill="#9a9a9a" transform="translate({} {})">"##, size / 18.0, size / 18.0);
                for g in glyphs { if face.outline_glyph(ttf_parser::GlyphId(g.gid), &mut PathSink(String::new())).is_some() { let _ = write!(body, "<use href=\"#g{}_{}\" transform=\"translate({} {}) scale({scale} {})\"/>", font.face, g.gid, g.x, baseline - g.dy, -scale); } }
                body.push_str("</g>");
            }
            let _ = write!(body, "<g {paint}>");
            for g in glyphs {
                let id = glyph_ids.entry((*font, g.gid)).or_insert_with(|| {
                    let mut sink = PathSink(String::new());
                    face.outline_glyph(ttf_parser::GlyphId(g.gid), &mut sink)?;
                    let id = format!("g{}_{}", font.face, g.gid);
                    let _ = write!(defs, r#"<path id="{id}" d="{}"/>"#, sink.0);
                    Some(id)
                });
                if let Some(id) = id {
                    let _ = write!(body, "<use href=\"#{id}\" transform=\"translate({} {}) scale({scale} {})\"/>", g.x, baseline - g.dy, -scale);
                }
            }
            body.push_str("</g>");
            if effects.rotate.is_some() { body.push_str("</g>"); }
        }
    };
    for item in items { draw(item, &mut body, &mut defs); }
    let bg = if doc.is_some() { r#"<rect width="100%" height="100%" fill="white"/>"# } else { "" };
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{ox} {oy} {w} {h}">{bg}<defs>{defs}</defs>{body}</svg>"#, w = p.width, h = p.height)
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() { out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char); } else { out.push('='); }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(super::base64(b"fo"), "Zm8=");
    }
}
