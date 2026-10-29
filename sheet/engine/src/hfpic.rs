//! Header and footer pictures (the &G field): a picture per header/footer section, sized and
//! cropped, printed where the section's &G stands, and saved as Excel does — a legacy VML part
//! (`vmlDrawingHF1.vml`) whose shapes are named by section (LH, CH, RH, LF, CF, RF, with FIRST or
//! EVEN for those pages' headers and footers), named by the sheet's `<legacyDrawingHF r:id>`.

use crate::xlsx::xml::{attr, Element};
use std::sync::Arc;

#[derive(Clone, PartialEq)]
pub struct HfPicture {
    /// The section it belongs to: "LH" "CH" "RH" "LF" "CF" "RF", plus "FIRST" or "EVEN".
    pub slot: String,
    pub data: Arc<Vec<u8>>,
    /// The picture's file extension (png, jpeg, gif, bmp).
    pub ext: String,
    /// Size on the page in points (of the cropped picture).
    pub width: f32,
    pub height: f32,
    /// The fractions cut off the left, top, right and bottom.
    pub crop: [f32; 4],
    pub title: String,
}

// Debug names the picture's bytes by address and length (page setups are compared as text).
impl std::fmt::Debug for HfPicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HfPicture({} {:x}/{} {} {}x{} {:?})", self.slot, Arc::as_ptr(&self.data) as usize, self.data.len(), self.ext, self.width, self.height, self.crop)
    }
}

/// Which pages a header/footer applies to: "" (odd/all), "FIRST" or "EVEN".
pub fn page_kind(hf: &crate::print::HeaderFooter, n: u32) -> &'static str {
    if hf.different_first && n == 1 { "FIRST" } else if hf.different_odd_even && n % 2 == 0 { "EVEN" } else { "" }
}

/// The slot name for a section (0 left, 1 center, 2 right) of a header or footer.
pub fn slot(section: usize, footer: bool, kind: &str) -> String {
    format!("{}{}{kind}", ["L", "C", "R"][section.min(2)], if footer { "F" } else { "H" })
}

impl HfPicture {
    /// A picture at its natural size (96 dpi pixels as points).
    pub fn new(slot: String, data: Vec<u8>, ext: &str) -> HfPicture {
        let (w, h) = crate::cellimage::image_size(&data).unwrap_or((96, 96));
        HfPicture { slot, data: Arc::new(data), ext: ext.to_ascii_lowercase(), width: w as f32 * 0.75, height: h as f32 * 0.75, crop: [0.0; 4], title: String::new() }
    }
    /// The full picture's size in points, before cropping.
    pub fn natural(&self) -> (f32, f32) {
        let (w, h) = crate::cellimage::image_size(&self.data).unwrap_or((96, 96));
        (w as f32 * 0.75, h as f32 * 0.75)
    }
    /// The picture's bytes with the crop applied (PNG), or the original when not cropped.
    pub fn cropped(&self) -> Vec<u8> {
        if self.crop.iter().all(|c| *c <= 0.0) { return self.data.to_vec(); }
        let Ok(img) = image::load_from_memory(&self.data) else { return self.data.to_vec() };
        let (w, h) = (img.width() as f32, img.height() as f32);
        let [l, t, r, b] = self.crop.map(|c| c.clamp(0.0, 0.99));
        let (x0, y0) = ((w * l).round() as u32, (h * t).round() as u32);
        let cw = ((w * (1.0 - l - r)).round() as u32).max(1).min(img.width().saturating_sub(x0).max(1));
        let ch = ((h * (1.0 - t - b)).round() as u32).max(1).min(img.height().saturating_sub(y0).max(1));
        let out = img.crop_imm(x0, y0, cw, ch);
        let mut buf = std::io::Cursor::new(Vec::new());
        if out.write_to(&mut buf, image::ImageFormat::Png).is_err() { return self.data.to_vec(); }
        buf.into_inner()
    }
    pub fn content_type(&self) -> &'static str {
        match self.ext.as_str() { "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", _ => "image/png" }
    }
}

/// A VML length ("93.75pt", "1in", "20px", "2cm", "12mm") in points.
fn length_pt(v: &str) -> Option<f32> {
    let v = v.trim();
    let split = v.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(v.len());
    let n: f32 = v[..split].trim().parse().ok()?;
    Some(match &v[split..] { "in" => n * 72.0, "px" => n * 0.75, "cm" => n * 72.0 / 2.54, "mm" => n * 72.0 / 25.4, "emu" => n / 12700.0, _ => n })
}

/// A VML fraction: "0.25" or in 1/65536ths ("16384f").
fn fraction(v: &str) -> f32 {
    let v = v.trim();
    if let Some(f) = v.strip_suffix('f') { f.trim().parse::<f32>().map_or(0.0, |x| x / 65536.0) } else { v.parse().unwrap_or(0.0) }
}

/// Reads the pictures of a header/footer VML part; `images` are its relationships' targets as
/// (relationship id, part bytes, extension).
pub fn read_vml(vml: &str, images: &[(String, Vec<u8>, String)]) -> Vec<HfPicture> {
    let Ok(root) = Element::parse(vml) else { return vec![] };
    let mut out = vec![];
    for sh in root.all("shape") {
        let Some(slot) = sh.attr("id").map(|s| s.to_ascii_uppercase()) else { continue };
        let Some(img) = sh.child("imagedata") else { continue };
        let Some(rel) = img.attr("o:relid").or(img.attr("relid")).or(img.attr("r:id")) else { continue };
        let Some((_, data, ext)) = images.iter().find(|i| i.0 == rel) else { continue };
        let mut p = HfPicture::new(slot, data.clone(), ext);
        for kv in sh.attr("style").unwrap_or("").split(';') {
            let Some((k, v)) = kv.split_once(':') else { continue };
            match k.trim() { "width" => if let Some(w) = length_pt(v) { p.width = w; }, "height" => if let Some(h) = length_pt(v) { p.height = h; }, _ => {} }
        }
        p.crop = ["cropleft", "croptop", "cropright", "cropbottom"].map(|k| img.attr(k).map_or(0.0, fraction));
        p.title = img.attr("o:title").or(img.attr("title")).unwrap_or("").to_string();
        out.push(p);
    }
    out
}

/// The VML part for a sheet's header/footer pictures, naming picture `k` as relationship
/// `rId{k+1}` (the caller writes those relationships to the media parts).
pub fn write_vml(pics: &[HfPicture], si: usize) -> String {
    let mut x = String::from("<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">");
    x.push_str(&format!("<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"{}\"/></o:shapelayout>", si + 1));
    x.push_str("<v:shapetype id=\"_x0000_t75\" coordsize=\"21600,21600\" o:spt=\"75\" o:preferrelative=\"t\" path=\"m@4@5l@4@11@9@11@9@5xe\" filled=\"f\" stroked=\"f\"><v:stroke joinstyle=\"miter\"/><v:formulas><v:f eqn=\"if lineDrawn pixelLineWidth 0\"/><v:f eqn=\"sum @0 1 0\"/><v:f eqn=\"sum 0 0 @1\"/><v:f eqn=\"prod @2 1 2\"/><v:f eqn=\"prod @3 21600 pixelWidth\"/><v:f eqn=\"prod @3 21600 pixelHeight\"/><v:f eqn=\"sum @0 0 1\"/><v:f eqn=\"prod @6 1 2\"/><v:f eqn=\"prod @7 21600 pixelWidth\"/><v:f eqn=\"sum @8 21600 0\"/><v:f eqn=\"prod @7 21600 pixelHeight\"/><v:f eqn=\"sum @10 21600 0\"/></v:formulas><v:path o:extrusionok=\"f\" gradientshapeok=\"t\" o:connecttype=\"rect\"/><o:lock v:ext=\"edit\" aspectratio=\"t\"/></v:shapetype>");
    for (k, p) in pics.iter().enumerate() {
        x.push_str(&format!("<v:shape id=\"{}\" o:spid=\"_x0000_s{}\" type=\"#_x0000_t75\" style=\"position:absolute;margin-left:0;margin-top:0;width:{}pt;height:{}pt;z-index:{}\">", attr(&p.slot), 1024 * (si + 1) + k + 1, p.width, p.height, k + 1));
        x.push_str(&format!("<v:imagedata o:relid=\"rId{}\" o:title=\"{}\"", k + 1, attr(&p.title)));
        for (name, v) in ["cropleft", "croptop", "cropright", "cropbottom"].iter().zip(p.crop) { if v > 0.0 { x.push_str(&format!(" {name}=\"{}f\"", (v * 65536.0).round() as i64)); } }
        x.push_str("/><o:lock v:ext=\"edit\" rotation=\"t\"/></v:shape>");
    }
    x.push_str("</xml>");
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn vml_round_trip_and_crop() {
        let mut p = HfPicture::new("CH".into(), png(40, 20), "png");
        assert_eq!((p.width, p.height), (30.0, 15.0));
        p.crop = [0.25, 0.0, 0.25, 0.0];
        p.width = 45.0;
        let vml = write_vml(std::slice::from_ref(&p), 0);
        let back = read_vml(&vml, &[("rId1".into(), p.data.to_vec(), "png".into())]);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].slot, "CH");
        assert_eq!(back[0].width, 45.0);
        assert!((back[0].crop[0] - 0.25).abs() < 1e-4 && back[0].crop[1] == 0.0);
        assert_eq!(crate::cellimage::image_size(&p.cropped()), Some((20, 20)));
        assert_eq!(length_pt("1in"), Some(72.0));
        assert!((fraction("16384f") - 0.25).abs() < 1e-6);
    }

    #[test]
    fn saved_read_back_and_printed() {
        let mut wb = crate::book::Workbook::new();
        wb.set_input(0, crate::addr::Pos::new(0, 0), "x");
        let hf = &mut wb.sheets[0].page.header_footer;
        hf.odd_header = "&LTitle&C&G".into();
        let mut p = HfPicture::new("CH".into(), png(64, 32), "png");
        p.crop = [0.0, 0.0, 0.5, 0.0];
        hf.pictures.push(p);
        let bytes = crate::xlsx::write(&wb).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("vmlDrawingHF1.vml") && text.contains("hfimage1_1.png"));
        let back = crate::xlsx::read(&bytes).unwrap();
        let pics = &back.sheets[0].page.header_footer.pictures;
        assert_eq!(pics.len(), 1);
        assert_eq!((pics[0].slot.as_str(), pics[0].width, pics[0].height), ("CH", 48.0, 24.0));
        assert!((pics[0].crop[2] - 0.5).abs() < 1e-4);
        assert!(!back.sheets[0].extra.iter().any(|(k, _)| k == "legacyDrawingHF"));
        // Saved again, it is still one picture (not carried twice).
        let again = crate::xlsx::read(&crate::xlsx::write(&back).unwrap()).unwrap();
        assert_eq!(again.sheets[0].page.header_footer.pictures.len(), 1);
        // Printed: the header draws the picture.
        let pages = crate::print::sheet_pages(&back, 0, None);
        let info = crate::print::JobInfo { file_name: "a.xlsx".into(), folder: String::new(), now: Some(45000.0) };
        let (lay, doc) = crate::print::render(&back, &pages, &info);
        let img = lay.pages[0].items.iter().find_map(|i| if let word_engine::layout::DrawItem::Image { asset, w, h, .. } = i { Some((asset.clone(), *w, *h)) } else { None }).expect("header picture");
        assert_eq!((img.1, img.2), (64.0, 32.0));
        assert!(doc.assets.contains_key(&img.0));
        assert!(crate::print::pdf(&back, &pages, &info, "t", 1).len() > 1000);
    }
}
