//! Export and print: pages of slides, handouts (1–9 per page), notes pages or the outline,
//! drawn as pictures (so every script, effect and 3-D look comes out as on screen) and written
//! to PDF; slide pictures (PNG/JPEG); the outline as text; and the file's properties
//! (docProps/core.xml: title, subject, author, keywords, comments…).

use crate::edit::RunFormat;
use crate::pres::Presentation;
use crate::scene::{Fill, Xfrm};
use crate::xml::{Element, Node};
use std::fmt::Write as _;
use tiny_skia::{Pixmap, PixmapPaint, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout { Slides, Handouts(usize), Notes, Outline }

#[derive(Clone, Debug)]
pub struct PrintOptions {
    pub layout: Layout,
    /// The slides to print (indexes), in order.
    pub slides: Vec<usize>,
    pub frame: bool,
    pub gray: bool,
    pub landscape: bool,
    pub header: String,
    pub footer: String,
    pub date: bool,
    pub numbers: bool,
    /// Pixels per inch of the pictures.
    pub dpi: f64,
}

impl Default for PrintOptions {
    fn default() -> Self { PrintOptions { layout: Layout::Slides, slides: vec![], frame: false, gray: false, landscape: false, header: String::new(), footer: String::new(), date: false, numbers: true, dpi: 150.0 } }
}

/// A rendered page: its picture and size in points.
pub struct Page { pub pixmap: Pixmap, pub w_pt: f64, pub h_pt: f64 }

const EMU_PER_PT: f64 = 12_700.0;
const LETTER: (f64, f64) = (612.0, 792.0);

fn slide_pixmap(pres: &Presentation, part: &str, width_px: u32) -> Option<Pixmap> {
    let scene = pres.scene(part);
    let mut images = crate::render::Images::default();
    crate::render::render(pres, &scene, &mut images, crate::render::Options { scale: width_px as f64 / scene.w, prompts: false, skip_text_of: None, translate: None })
}

/// Text in a box of `w`×`h` points at `size` pt, drawn by the slide engine (all scripts).
fn text_pixmap(text: &str, w: f64, h: f64, size: f64, bold: bool, align_right: bool, ppt: f64) -> Option<Pixmap> {
    if text.trim().is_empty() { return None; }
    let mut p = Presentation::new_blank(true);
    p.set_size((w * EMU_PER_PT) as i64, (h * EMU_PER_PT) as i64);
    let l = p.all_layouts().last().cloned()?;
    let s = p.add_slide(&l, 0);
    let loc = p.insert_shape(&s, "rect", &Xfrm { x: 0.0, y: 0.0, w: w * EMU_PER_PT, h: h * EMU_PER_PT, ..Default::default() }, true)?;
    p.insert_text(&loc, None, 0, text);
    let n = text.chars().count();
    p.format_range(&loc, None, 0, n, &RunFormat { size: Some(size), bold: Some(bold), color: Some("262626".into()), ..Default::default() });
    if align_right { p.set_align(&loc, None, None, "r"); }
    let mut scene = p.scene(&s);
    scene.background = Fill::None;
    let mut images = crate::render::Images::default();
    crate::render::render(&p, &scene, &mut images, crate::render::Options { scale: ppt / EMU_PER_PT, prompts: false, skip_text_of: None, translate: None })
}

fn place(page: &mut Pixmap, pic: &Pixmap, x: f64, y: f64, w: f64, ppt: f64) {
    let k = (w * ppt / pic.width() as f64) as f32;
    let paint = PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
    page.draw_pixmap(0, 0, pic.as_ref(), &paint, Transform::from_scale(k, k).post_translate((x * ppt) as f32, (y * ppt) as f32), None);
}

fn frame_rect(page: &mut Pixmap, x: f64, y: f64, w: f64, h: f64, ppt: f64) {
    let Some(r) = tiny_skia::Rect::from_xywh((x * ppt) as f32, (y * ppt) as f32, (w * ppt) as f32, (h * ppt) as f32) else { return };
    let path = tiny_skia::PathBuilder::from_rect(r);
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(110, 110, 110, 255);
    page.stroke_path(&path, &paint, &tiny_skia::Stroke { width: (0.75 * ppt) as f32, ..Default::default() }, Transform::identity(), None);
}

fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let z = secs / 86_400 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{m}/{d}/{}", yoe + era * 400 + if m <= 2 { 1 } else { 0 })
}

fn grayscale(pm: &mut Pixmap) {
    for p in pm.pixels_mut() {
        let l = (0.299 * p.red() as f32 + 0.587 * p.green() as f32 + 0.114 * p.blue() as f32) as u8;
        *p = tiny_skia::PremultipliedColorU8::from_rgba(l, l, l, p.alpha()).unwrap_or(*p);
    }
}

/// The printed pages.
pub fn pages(pres: &Presentation, o: &PrintOptions) -> Vec<Page> {
    let all = pres.slides();
    let parts: Vec<String> = o.slides.iter().filter_map(|&i| all.get(i).cloned()).collect();
    let (sw, sh) = pres.size();
    let ar = sh / sw;
    let ppt = o.dpi / 72.0;
    let mut out = vec![];
    let (pw, ph) = if o.landscape { (LETTER.1, LETTER.0) } else { LETTER };
    let new_page = |w: f64, h: f64| { let mut p = Pixmap::new((w * ppt).ceil() as u32, (h * ppt).ceil() as u32).unwrap(); p.fill(tiny_skia::Color::WHITE); p };
    // Header, date, footer and page number on paper pages.
    let decorate = |page: &mut Pixmap, n: usize, w: f64, h: f64| {
        let m = 36.0;
        if let Some(t) = text_pixmap(&o.header, w / 2.0 - m, 20.0, 10.0, false, false, ppt) { place(page, &t, m, 14.0, w / 2.0 - m, ppt); }
        if o.date { if let Some(t) = text_pixmap(&today(), w / 2.0 - m, 20.0, 10.0, false, true, ppt) { place(page, &t, w / 2.0, 14.0, w / 2.0 - m, ppt); } }
        if let Some(t) = text_pixmap(&o.footer, w / 2.0 - m, 20.0, 10.0, false, false, ppt) { place(page, &t, m, h - 30.0, w / 2.0 - m, ppt); }
        if o.numbers { if let Some(t) = text_pixmap(&n.to_string(), w / 2.0 - m, 20.0, 10.0, false, true, ppt) { place(page, &t, w / 2.0, h - 30.0, w / 2.0 - m, ppt); } }
    };
    match o.layout {
        Layout::Slides => {
            // A page the size of the slide.
            let (w, h) = (sw / EMU_PER_PT, sh / EMU_PER_PT);
            for part in &parts {
                let Some(pic) = slide_pixmap(pres, part, (w * ppt) as u32) else { continue };
                let mut page = new_page(w, h);
                place(&mut page, &pic, 0.0, 0.0, w, ppt);
                if o.frame { frame_rect(&mut page, 0.5, 0.5, w - 1.0, h - 1.0, ppt); }
                out.push(Page { pixmap: page, w_pt: w, h_pt: h });
            }
        }
        Layout::Handouts(n) => {
            let n = n.clamp(1, 9);
            let (cols, rows) = match (n, o.landscape) { (1, _) => (1, 1), (2, _) => (1, 2), (3, false) => (1, 3), (3, true) => (3, 1), (4, _) => (2, 2), (6, false) => (2, 3), (6, true) => (3, 2), _ => (3, 3) };
            let lines = n == 3 && !o.landscape;
            for (k, chunk) in parts.chunks(n).enumerate() {
                let mut page = new_page(pw, ph);
                let (mx, top, bottom) = (40.0, 50.0, 50.0);
                let cw = (pw - 2.0 * mx) / cols as f64;
                let rh = (ph - top - bottom) / rows as f64;
                for (j, part) in chunk.iter().enumerate() {
                    let (c, r) = (j % cols, j / cols);
                    let (cx, cy) = (mx + c as f64 * cw + 8.0, top + r as f64 * rh + 8.0);
                    let avail_w = if lines { cw * 0.5 } else { cw - 16.0 };
                    let fw = avail_w.min((rh - 16.0) / ar);
                    let fx = if lines { cx } else { cx + (cw - 16.0 - fw) / 2.0 };
                    if let Some(pic) = slide_pixmap(pres, part, (fw * ppt) as u32) { place(&mut page, &pic, fx, cy, fw, ppt); }
                    frame_rect(&mut page, fx, cy, fw, fw * ar, ppt);
                    if lines {
                        let mut y = cy + 14.0;
                        while y < cy + fw * ar {
                            let mut paint = tiny_skia::Paint::default();
                            paint.set_color_rgba8(170, 170, 170, 255);
                            let mut pb = tiny_skia::PathBuilder::new();
                            pb.move_to(((fx + fw + 16.0) * ppt) as f32, (y * ppt) as f32);
                            pb.line_to(((pw - mx) * ppt) as f32, (y * ppt) as f32);
                            if let Some(path) = pb.finish() { page.stroke_path(&path, &paint, &tiny_skia::Stroke { width: (0.5 * ppt) as f32, ..Default::default() }, Transform::identity(), None); }
                            y += 18.0;
                        }
                    }
                }
                decorate(&mut page, k + 1, pw, ph);
                out.push(Page { pixmap: page, w_pt: pw, h_pt: ph });
            }
        }
        Layout::Notes => {
            for (k, part) in parts.iter().enumerate() {
                let (pw, ph) = LETTER;
                let mut page = new_page(pw, ph);
                let fw = pw * 0.76;
                let (fx, fy) = ((pw - fw) / 2.0, 60.0);
                if let Some(pic) = slide_pixmap(pres, part, (fw * ppt) as u32) { place(&mut page, &pic, fx, fy, fw, ppt); }
                frame_rect(&mut page, fx, fy, fw, fw * ar, ppt);
                let notes = pres.notes_text(part);
                let body_top = fy + fw * ar + 24.0;
                if let Some(t) = text_pixmap(&notes, pw - 2.0 * 72.0, ph - body_top - 60.0, 12.0, false, false, ppt) { place(&mut page, &t, 72.0, body_top, pw - 2.0 * 72.0, ppt); }
                decorate(&mut page, o.slides.get(k).map(|i| i + 1).unwrap_or(k + 1), pw, ph);
                out.push(Page { pixmap: page, w_pt: pw, h_pt: ph });
            }
        }
        Layout::Outline => {
            let text = outline_text(pres, &o.slides);
            // About 46 lines a page.
            let lines: Vec<&str> = text.lines().collect();
            for (k, chunk) in lines.chunks(46).enumerate() {
                let mut page = new_page(pw, ph);
                if let Some(t) = text_pixmap(&chunk.join("\n"), pw - 2.0 * 60.0, ph - 120.0, 11.0, false, false, ppt) { place(&mut page, &t, 60.0, 56.0, pw - 120.0, ppt); }
                decorate(&mut page, k + 1, pw, ph);
                out.push(Page { pixmap: page, w_pt: pw, h_pt: ph });
            }
        }
    }
    if o.gray { for p in &mut out { grayscale(&mut p.pixmap); } }
    out
}

/// The text of the slides as an outline: the title, then the body lines indented.
pub fn outline_text(pres: &Presentation, slides: &[usize]) -> String {
    let all = pres.slides();
    let mut out = String::new();
    for &i in slides {
        let Some(part) = all.get(i) else { continue };
        let scene = pres.scene(part);
        let mut title = String::new();
        let mut body = vec![];
        for it in scene.items.iter().filter(|i| i.own) {
            let crate::scene::Kind::Shape { text: Some(t), .. } = &it.kind else { continue };
            let is_title = it.placeholder.as_ref().is_some_and(|p| matches!(p.0.as_str(), "title" | "ctrTitle"));
            for para in &t.paras {
                let line: String = para.runs.iter().map(|r| r.text.as_str()).collect();
                if line.trim().is_empty() { continue; }
                if is_title { if !title.is_empty() { title.push(' '); } title.push_str(line.trim()); } else { body.push(format!("{}{}", "    ".repeat(para.level as usize + 1), line.trim())); }
            }
        }
        let _ = writeln!(out, "{}. {}", i + 1, if title.is_empty() { "(untitled)".into() } else { title });
        for b in body { let _ = writeln!(out, "{b}"); }
    }
    out
}

fn jpeg(pm: &Pixmap, quality: u8) -> Vec<u8> {
    let rgb: Vec<u8> = pm.pixels().iter().flat_map(|p| { let c = p.demultiply(); [c.red(), c.green(), c.blue()] }).collect();
    let mut out = vec![];
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    let _ = enc.encode(&rgb, pm.width(), pm.height(), image::ExtendedColorType::Rgb8);
    out
}

/// A PDF of the pages (each a JPEG picture filling its page).
pub fn pdf(pages: &[Page], title: &str) -> Vec<u8> {
    let mut objs: Vec<Vec<u8>> = vec![];
    // 1 catalog, 2 pages, 3 info; then per page: page, content, image.
    let n = pages.len();
    let kids: Vec<String> = (0..n).map(|k| format!("{} 0 R", 4 + k * 3)).collect();
    objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).into_bytes());
    let esc = |s: &str| -> String {
        // UTF-16BE with BOM, so any script survives in the title.
        let mut h = String::from("<FEFF");
        for u in s.encode_utf16() { let _ = write!(h, "{u:04X}"); }
        h.push('>');
        h
    };
    objs.push(format!("<< /Title {} /Producer (Sard) >>", esc(title)).into_bytes());
    for (k, p) in pages.iter().enumerate() {
        let (page, content, img) = (4 + k * 3, 5 + k * 3, 6 + k * 3);
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /XObject << /Im{k} {img} 0 R >> >> /Contents {content} 0 R >>", p.w_pt, p.h_pt).into_bytes());
        let _ = page;
        let draw = format!("q {:.2} 0 0 {:.2} 0 0 cm /Im{k} Do Q", p.w_pt, p.h_pt);
        objs.push(format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()).into_bytes());
        let data = jpeg(&p.pixmap, 88);
        let mut o = format!("<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", p.pixmap.width(), p.pixmap.height(), data.len()).into_bytes();
        o.extend_from_slice(&data);
        o.extend_from_slice(b"\nendstream");
        objs.push(o);
    }
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![];
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let mut x = format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1);
    for off in offsets { let _ = writeln!(x, "{off:010} 00000 n "); }
    let _ = write!(x, "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1);
    out.extend_from_slice(x.as_bytes());
    out
}

/// A slide as a JPEG of `width` px.
pub fn slide_jpeg(pres: &Presentation, slide: &str, width: u32) -> Option<Vec<u8>> { slide_pixmap(pres, slide, width).map(|p| jpeg(&p, 92)) }

/// A slide's picture (RGBA, not premultiplied) for other encoders.
pub fn slide_rgba(pres: &Presentation, slide: &str, width: u32) -> Option<(Vec<u8>, u32, u32)> {
    let p = slide_pixmap(pres, slide, width)?;
    Some((p.pixels().iter().flat_map(|c| { let c = c.demultiply(); [c.red(), c.green(), c.blue(), c.alpha()] }).collect(), p.width(), p.height()))
}

// ---- properties -----------------------------------------------------------------------------

const RT_CORE: &str = "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const CT_CORE: &str = "application/vnd.openxmlformats-package.core-properties+xml";

/// The file's properties: (name, value) for title, subject, creator, keywords, description,
/// category, lastModifiedBy, created, modified.
pub const PROPS: &[(&str, &str)] = &[("dc:title", "Title"), ("dc:subject", "Subject"), ("dc:creator", "Author"), ("cp:keywords", "Keywords"), ("dc:description", "Comments"), ("cp:category", "Category"), ("cp:contentStatus", "Status"), ("cp:lastModifiedBy", "Last Modified By"), ("dcterms:created", "Created"), ("dcterms:modified", "Modified")];

impl Presentation {
    fn core_part(&self) -> Option<String> { self.pkg.related("", RT_CORE).or_else(|| self.pkg.has("docProps/core.xml").then(|| "docProps/core.xml".to_string())) }

    pub fn property(&self, name: &str) -> String {
        self.core_part().and_then(|p| self.pkg.xml(&p)).and_then(|r| r.child(name)).map(|e| e.text()).unwrap_or_default()
    }

    pub fn set_property(&mut self, name: &str, value: &str) {
        let part = match self.core_part() {
            Some(p) => p,
            None => {
                let p = "docProps/core.xml".to_string();
                let root = Element::new("cp:coreProperties")
                    .with_attr("xmlns:cp", "http://schemas.openxmlformats.org/package/2006/metadata/core-properties")
                    .with_attr("xmlns:dc", "http://purl.org/dc/elements/1.1/")
                    .with_attr("xmlns:dcterms", "http://purl.org/dc/terms/")
                    .with_attr("xmlns:dcmitype", "http://purl.org/dc/dcmitype/")
                    .with_attr("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance");
                self.pkg.put_xml(&p, root, Some(CT_CORE));
                self.pkg.add_rel("", RT_CORE, &p, false);
                p
            }
        };
        let Some(root) = self.pkg.xml_mut(&part) else { return };
        root.remove_children(name);
        if value.is_empty() { return; }
        let mut e = Element::new(name).with_text(value);
        if name.starts_with("dcterms:") { e.set_attr("xsi:type", "dcterms:W3CDTF"); }
        root.children.push(Node::Element(e));
    }
}

/// Now as W3CDTF (UTC).
pub fn w3c_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let d = today();
    let mut it = d.split('/');
    let (m, day, y) = (it.next().unwrap_or("1"), it.next().unwrap_or("1"), it.next().unwrap_or("1970"));
    let t = secs % 86_400;
    format!("{y}-{:0>2}-{:0>2}T{:02}:{:02}:{:02}Z", m, day, t / 3600, (t / 60) % 60, t % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_pages_and_properties() {
        let mut p = Presentation::new_blank(true);
        let l = p.all_layouts()[1].clone();
        for k in 0..5 {
            let s = p.add_slide(&l, k);
            let t = p.scene(&s).items.iter().find(|i| i.placeholder.is_some()).unwrap().loc.clone();
            p.insert_text(&t, None, 0, &format!("Slide {}", k + 1));
            p.set_notes(&s, "Speaker notes, and عربي too.");
        }
        let idx: Vec<usize> = (0..5).collect();
        for (layout, count) in [(Layout::Slides, 5), (Layout::Handouts(3), 2), (Layout::Handouts(6), 1), (Layout::Notes, 5), (Layout::Outline, 1)] {
            let pages = pages(&p, &PrintOptions { layout, slides: idx.clone(), dpi: 40.0, header: "Head".into(), date: true, ..Default::default() });
            assert_eq!(pages.len(), count, "{layout:?}");
            let bytes = pdf(&pages, "Deck");
            assert!(bytes.starts_with(b"%PDF-1.4") && bytes.ends_with(b"%%EOF\n"));
        }
        assert!(outline_text(&p, &idx).contains("2. Slide 2"));
        p.set_property("dc:title", "My Deck");
        p.set_property("dcterms:modified", &w3c_now());
        let again = Presentation::open(&p.save().unwrap()).unwrap();
        assert_eq!(again.property("dc:title"), "My Deck");
        assert!(again.property("dcterms:modified").ends_with('Z'));
    }
}
