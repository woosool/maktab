//! PDF export with searchable text (backstage "Download PDF" in the JS app).
//!
//! Static faces are embedded as TrueType CID fonts (Identity-H, glyph ids as
//! character codes) with ToUnicode maps, so text can be selected and searched.
//! Variable-font instances (Noto Sans Bold) are drawn as outlines with an
//! invisible text layer on top for searching. Pictures are embedded as
//! Flate-compressed RGB with an alpha mask; hyperlinks become URI annotations.


use crate::layout::{DrawItem, Layout};
use crate::model::Document;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

const PT: f32 = 0.75; // pt per px

struct Pdf { objects: Vec<Vec<u8>> }

impl Pdf {
    fn reserve(&mut self) -> usize { self.objects.push(vec![]); self.objects.len() }
    fn set(&mut self, id: usize, body: impl Into<Vec<u8>>) { self.objects[id - 1] = body.into(); }
    fn add(&mut self, body: impl Into<Vec<u8>>) -> usize { let id = self.reserve(); self.set(id, body); id }
    fn stream(&mut self, dict: &str, data: &[u8]) -> usize {
        let z = miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
        let mut body = format!("<< {dict} /Filter /FlateDecode /Length {} >>\nstream\n", z.len()).into_bytes();
        body.extend(z);
        body.extend(b"\nendstream");
        self.add(body)
    }
    fn finish(self, root: usize, info: usize) -> Vec<u8> {
        let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = vec![];
        for (i, o) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n", i + 1).bytes());
            out.extend(o);
            out.extend(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", self.objects.len() + 1).bytes());
        for o in offsets { out.extend(format!("{o:010} 00000 n \n").bytes()); }
        out.extend(format!("trailer\n<< /Size {} /Root {root} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n", self.objects.len() + 1).bytes());
        out
    }
}

fn pdf_string(s: &str) -> String {
    // UTF-16BE with BOM, as a hex string.
    let mut h = String::from("<FEFF");
    for u in s.encode_utf16() { let _ = write!(h, "{u:04X}"); }
    h.push('>');
    h
}

fn rgb(c: crate::Color) -> String { format!("{:.3} {:.3} {:.3}", c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0) }

fn parse_face(index: usize) -> ttf_parser::Face<'static> {
    let data = crate::fonts::face(index as u16);
    let mut f = ttf_parser::Face::parse(data.bytes(), data.index).unwrap();
    if data.weight > 0.0 { f.set_variation(ttf_parser::Tag::from_bytes(b"wght"), data.weight); }
    f
}

/// Outline path with exact quadratic-to-cubic conversion.
fn glyph_path(face: &ttf_parser::Face, gid: ttf_parser::GlyphId) -> Option<String> {
    struct Exact { s: String, last: (f32, f32) }
    impl ttf_parser::OutlineBuilder for Exact {
        fn move_to(&mut self, x: f32, y: f32) { let _ = write!(self.s, "{x} {y} m "); self.last = (x, y); }
        fn line_to(&mut self, x: f32, y: f32) { let _ = write!(self.s, "{x} {y} l "); self.last = (x, y); }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            let (x0, y0) = self.last;
            let c1 = (x0 + 2.0 / 3.0 * (x1 - x0), y0 + 2.0 / 3.0 * (y1 - y0));
            let c2 = (x + 2.0 / 3.0 * (x1 - x), y + 2.0 / 3.0 * (y1 - y));
            let _ = write!(self.s, "{} {} {} {} {x} {y} c ", c1.0, c1.1, c2.0, c2.1);
            self.last = (x, y);
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { let _ = write!(self.s, "{x1} {y1} {x2} {y2} {x} {y} c "); self.last = (x, y); }
        fn close(&mut self) { self.s.push_str("h "); }
    }
    let mut e = Exact { s: String::new(), last: (0.0, 0.0) };
    face.outline_glyph(gid, &mut e)?;
    Some(e.s)
}

/// Export all pages to PDF bytes.
pub fn export(doc: &Document, layout: &Layout, title: &str) -> Vec<u8> {
    let all: Vec<usize> = (0..layout.pages.len()).collect();
    export_pages(doc, layout, title, &all)
}

/// Export the given pages (indices, in order, repeats allowed: printing copies) to PDF bytes.
pub fn export_pages(doc: &Document, layout: &Layout, title: &str, pages: &[usize]) -> Vec<u8> {
    let mut pdf = Pdf { objects: vec![] };
    let catalog = pdf.reserve();
    let pages_id = pdf.reserve();
    // Font resources: one per embedded static face.
    let mut font_ids: BTreeMap<u16, usize> = BTreeMap::new();
    let mut used: HashMap<u16, BTreeMap<u16, char>> = HashMap::new(); // face -> gid -> char
    let mut faces: HashMap<u16, ttf_parser::Face<'static>> = HashMap::new();
    let mut image_ids: HashMap<String, Option<usize>> = HashMap::new();
    let mut page_ids = vec![];

    for page in pages.iter().filter_map(|&k| layout.pages.get(k)) {
        let h = page.height * PT;
        let mut c = String::new();
        // Layout px with a top-left origin → PDF points.
        let _ = writeln!(c, "{PT} 0 0 -{PT} 0 {h} cm");
        let mut xobjects = BTreeSet::new();
        let mut paint = |item: &DrawItem, c: &mut String, pdf: &mut Pdf| match item {
            DrawItem::Frame { .. } => {}
            DrawItem::Path { points, closed, fill, stroke, .. } => {
                let d: String = points.iter().enumerate().map(|(i, p)| format!("{} {} {} ", p.0, p.1, if i == 0 { "m" } else { "l" })).collect();
                let close = if *closed { "h " } else { "" };
                if let Some(f) = fill { let _ = writeln!(c, "{} rg {d}{close}f", rgb(*f)); }
                if let Some((sc, w)) = stroke { let _ = writeln!(c, "{} RG {w} w 1 j {d}{close}S", rgb(*sc)); }
            }
            DrawItem::Rect { x, y, w, h, color } => { let _ = writeln!(c, "{} rg {x} {y} {w} {h} re f", rgb(*color)); }
            DrawItem::Line { x1, y1, x2, y2, width, color } => { let _ = writeln!(c, "{} RG {width} w {x1} {y1} m {x2} {y2} l S", rgb(*color)); }
            DrawItem::Image { asset, x, y, w, h, rot, flip, .. } => {
                let id = *image_ids.entry(asset.clone()).or_insert_with(|| doc.assets.get(asset).and_then(|a| image::load_from_memory(&a.bytes).ok()).map(|img| {
                    let rgba = img.to_rgba8();
                    let (iw, ih) = rgba.dimensions();
                    let rgb_bytes: Vec<u8> = rgba.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
                    let alpha: Vec<u8> = rgba.pixels().map(|p| p[3]).collect();
                    let smask = alpha.iter().any(|a| *a != 255).then(|| pdf.stream(&format!("/Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceGray /BitsPerComponent 8"), &alpha));
                    let sm = smask.map(|s| format!(" /SMask {s} 0 R")).unwrap_or_default();
                    pdf.stream(&format!("/Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceRGB /BitsPerComponent 8{sm}"), &rgb_bytes)
                }));
                if let Some(id) = id {
                    xobjects.insert(id);
                    // Image space is upside down relative to our flipped page.
                    let r = rotation_cm(*rot, x + w / 2.0, y + h / 2.0);
                    // Mirroring negates an axis of the image matrix and moves its origin.
                    let (a, ex) = if flip.0 { (-w, x + w) } else { (*w, *x) };
                    let (d, fy) = if flip.1 { (*h, *y) } else { (-h, y + h) };
                    let _ = writeln!(c, "q {r}{a} 0 0 {d} {ex} {fy} cm /Im{id} Do Q");
                }
            }
            DrawItem::Glyphs { glyphs, baseline, font, size, color, effects } => {
                let color = &effects.gradient.map(|g| g.0).unwrap_or(*color);
                if let Some((a, cx, cy)) = effects.rotate { let _ = writeln!(c, "q {}", rotation_cm(a, cx, cy)); }
                let face = faces.entry(font.face).or_insert_with(|| parse_face(font.face as usize));
                let upem = face.units_per_em() as f32;
                let variable = crate::fonts::face(font.face).weight > 0.0;
                let _ = writeln!(c, "{} rg", rgb(*color));
                let text_face = if variable { 0u16 } else { font.face };
                if effects.shadow || effects.outline {
                    // Shadow behind, then hollow (stroked) or filled outlines; text stays as an invisible layer.
                    let s = size / upem;
                    for (dxy, stroke) in [(size / 18.0, false), (0.0, effects.outline)] {
                        if dxy > 0.0 && !effects.shadow { continue; }
                        let _ = writeln!(c, "{}", if dxy > 0.0 { "0.6 0.6 0.6 rg".to_string() } else if stroke { format!("{} RG 1 1 1 rg {} w", rgb(*color), size / 30.0 / s) } else { format!("{} rg", rgb(*color)) });
                        for g in glyphs {
                            if let Some(path) = glyph_path(face, ttf_parser::GlyphId(g.gid)) {
                                let _ = writeln!(c, "q {s} 0 0 -{s} {} {} cm {path}{} Q", g.x + dxy, baseline - g.dy + dxy, if stroke { "B" } else { "f" });
                            }
                        }
                    }
                } else if variable || font.synthetic_bold {
                    // Outlines carry the visible weight.
                    let s = size / upem;
                    for g in glyphs {
                        if let Some(path) = glyph_path(face, ttf_parser::GlyphId(g.gid)) {
                            let y = baseline - g.dy;
                            let _ = writeln!(c, "q {s} 0 0 -{s} {} {y} cm {path}f Q", g.x);
                            if font.synthetic_bold { let _ = writeln!(c, "q {s} 0 0 -{s} {} {y} cm {path}f Q", g.x + size / 32.0); }
                        }
                    }
                }
                // Text layer: visible for static faces, invisible (render mode 3) over outlines.
                let tf = faces.entry(text_face).or_insert_with(|| parse_face(text_face as usize));
                let map = used.entry(text_face).or_default();
                let mode = if variable || font.synthetic_bold || effects.shadow || effects.outline { 3 } else { 0 };
                let _ = write!(c, "BT {mode} Tr /F{text_face} {size} Tf ");
                for g in glyphs {
                    // The visible layer uses the shaped glyph; the invisible one maps characters.
                    let gid = if mode == 0 { g.gid } else { tf.glyph_index(g.ch).map(|x| x.0).unwrap_or(0) };
                    map.entry(gid).or_insert(g.ch);
                    let _ = write!(c, "1 0 0 -1 {} {} Tm <{gid:04X}> Tj ", g.x, baseline - g.dy);
                }
                c.push_str("ET\n");
                if effects.rotate.is_some() { c.push_str("Q\n"); }
            }
        };
        for item in &page.under { paint(item, &mut c, &mut pdf); }
        for item in &page.items { paint(item, &mut c, &mut pdf); }
        for item in &page.over { paint(item, &mut c, &mut pdf); }
        let content = pdf.stream("", c.as_bytes());
        let page_id = pdf.reserve();
        let mut annots = vec![];
        for (x, y, w, lh, target) in &page.links {
            if target.starts_with('#') { continue; }
            let (x0, y0, x1, y1) = (x * PT, h - (y + lh) * PT, (x + w) * PT, h - y * PT);
            let mut uri = String::new();
            for ch in target.chars() { if matches!(ch, '(' | ')' | '\\') { uri.push('\\'); } uri.push(ch); }
            annots.push(pdf.add(format!("<< /Type /Annot /Subtype /Link /Rect [{x0} {y0} {x1} {y1}] /Border [0 0 0] /A << /S /URI /URI ({uri}) >> >>")));
        }
        page_ids.push((page_id, content, page.width * PT, h, xobjects, annots));
    }

    // Fonts are written after all pages so their used glyph sets are known.
    let mut faces_used: Vec<u16> = used.keys().copied().collect();
    faces_used.sort();
    for f in faces_used {
        let face = &faces[&f];
        let upem = face.units_per_em() as f32;
        let k = 1000.0 / upem;
        let fd = crate::fonts::face(f as u16);
        let base: String = fd.family.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let name = format!("WD{:02}+{}{}{}", f, if base.is_empty() { "Font".into() } else { base }, if fd.bold { "-Bold" } else { "" }, if fd.italic { "-Italic" } else { "" });
        // TrueType outlines embed as FontFile2; CFF (PostScript) outlines as an OpenType FontFile3.
        let (program, is_cff) = crate::fonts::embeddable(f);
        let file = if is_cff { pdf.stream("/Subtype /OpenType", &program) } else { pdf.stream(&format!("/Length1 {}", program.len()), &program) };
        let bbox = face.global_bounding_box();
        let descriptor = pdf.add(format!(
            "<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [{} {} {} {}] /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV 80 /{} {file} 0 R >>",
            bbox.x_min as f32 * k, bbox.y_min as f32 * k, bbox.x_max as f32 * k, bbox.y_max as f32 * k,
            face.italic_angle(), face.ascender() as f32 * k, face.descender() as f32 * k, face.capital_height().unwrap_or(face.ascender()) as f32 * k, if is_cff { "FontFile3" } else { "FontFile2" }));
        let glyphs = &used[&f];
        let mut widths = String::new();
        for gid in glyphs.keys() {
            let w = face.glyph_hor_advance(ttf_parser::GlyphId(*gid)).unwrap_or(0) as f32 * k;
            let _ = write!(widths, "{gid} [{w}] ");
        }
        let cid = if is_cff {
            pdf.add(format!("<< /Type /Font /Subtype /CIDFontType0 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /W [{widths}] >>"))
        } else {
            pdf.add(format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity /W [{widths}] >>"))
        };
        let mut cmap = String::from("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Adobe-Identity-UCS def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange\n");
        let entries: Vec<_> = glyphs.iter().collect();
        for chunk in entries.chunks(100) {
            let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
            for (gid, ch) in chunk {
                let mut u = String::new();
                for unit in ch.to_string().encode_utf16() { let _ = write!(u, "{unit:04X}"); }
                let _ = writeln!(cmap, "<{gid:04X}> <{u}>");
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap CMapName currentdict /CMap defineresource pop end end");
        let tounicode = pdf.stream("", cmap.as_bytes());
        let font = pdf.add(format!("<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {tounicode} 0 R >>"));
        font_ids.insert(f, font);
    }
    let fonts: String = font_ids.iter().map(|(f, id)| format!("/F{f} {id} 0 R ")).collect();
    for (page_id, content, w, h, xobjects, annots) in &page_ids {
        let xo: String = xobjects.iter().map(|id| format!("/Im{id} {id} 0 R ")).collect();
        let an: String = annots.iter().map(|id| format!("{id} 0 R ")).collect();
        pdf.set(*page_id, format!("<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {w} {h}] /Contents {content} 0 R /Resources << /Font << {fonts}>> /XObject << {xo}>> >> /Annots [{an}] >>"));
    }
    let kids: String = page_ids.iter().map(|p| format!("{} 0 R ", p.0)).collect();
    pdf.set(pages_id, format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", page_ids.len()));
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {pages_id} 0 R >>"));
    let info = pdf.add(format!("<< /Title {} /Producer (Word) >>", pdf_string(title)));
    pdf.finish(catalog, info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, Paragraph};

    #[test]
    fn writes_valid_structure() {
        let mut d = Document::new(vec![Paragraph::new("Hello PDF world"), Paragraph::new("bold line")]);
        d.select(16, 20);
        d.toggle_bold();
        d.select(0, 5);
        d.set_link(Some("https://example.com/a(b)".into()));
        let layout = crate::layout_document(&d);
        let bytes = export(&d, &layout, "Test");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.7") && text.trim_end().ends_with("%%EOF"));
        assert!(text.contains("/Type /Page ") && text.contains("/FontFile2") && text.contains("/ToUnicode"));
        assert!(text.contains("/URI (https://example.com/a\\(b\\))"), "links become escaped URI annotations");
        // xref offsets point at objects.
        let xref_at: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        assert!(bytes[xref_at..].starts_with(b"xref"));
    }
}

/// A `cm` operator rotating clockwise by `deg` about (cx, cy) in the flipped page space; empty for 0°.
fn rotation_cm(deg: f32, cx: f32, cy: f32) -> String {
    if deg.rem_euclid(360.0) == 0.0 { return String::new(); }
    let (sin, cos) = deg.to_radians().sin_cos();
    format!("{cos} {sin} {} {cos} {} {} cm ", -sin, cx - cx * cos + cy * sin, cy - cx * sin - cy * cos)
}
