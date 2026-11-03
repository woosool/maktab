//! Pictures in cells (Excel's Place in Cell and IMAGE()): an image held as a cell's value and
//! drawn fitted inside the cell.
//!
//! The value model has no image variant, so an image value is a text value in a reserved form:
//! U+E000, the source (a `data:image/...;base64,` URL or an http(s) URL), U+E001, the alt text.
//! `parse` recognizes it; the grid and printing draw it instead of its text.
//!
//! Files: placed pictures are saved as Excel saves them, rich values (xl/richData, cell `vm`,
//! xlsx/richdata.rs), and Excel's own placed pictures are read back. IMAGE() formulas are
//! written as formulas (Excel computes them itself) with the alt text as the cached value.
//! Older Jadwal files carried a placed picture as a floating picture named `CELL_PICTURE`
//! over the cell; reading still turns those back into cell values.

use crate::addr::Pos;
use crate::book::{Input, Workbook};
use crate::drawing::{Anchor, ObjKind};
use crate::value::Value;

const START: char = '\u{E000}';
const SEP: char = '\u{E001}';
/// The name prefix of the floating pictures that carry a picture in cell in a saved file.
pub const CELL_PICTURE: &str = "Picture in Cell";

/// An image value from its source and alt text.
pub fn value(src: &str, alt: &str) -> Value { Value::str(format!("{START}{src}{SEP}{alt}")) }

/// The (source, alt text) of an image value.
pub fn parse(v: &Value) -> Option<(&str, &str)> {
    let Value::Str(s) = v else { return None };
    let rest = s.strip_prefix(START)?;
    Some(rest.split_once(SEP).unwrap_or((rest, "")))
}

pub fn is_image(v: &Value) -> bool { parse(v).is_some() }

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 { if i <= c.len() { s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char); } else { s.push('='); } }
    }
    s
}

pub fn unbase64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for b in s.bytes() {
        let v = match b { b'A'..=b'Z' => b - b'A', b'a'..=b'z' => b - b'a' + 26, b'0'..=b'9' => b - b'0' + 52, b'+' | b'-' => 62, b'/' | b'_' => 63, b'=' | b'\n' | b'\r' | b' ' => continue, _ => return None };
        acc = acc << 6 | v as u32; bits += 6;
        if bits >= 8 { bits -= 8; out.push((acc >> bits) as u8); }
    }
    Some(out)
}

/// A data URL holding a picture file's bytes.
pub fn data_url(data: &[u8], ext: &str) -> String { format!("data:image/{};base64,{}", mime_ext(ext), base64(data)) }

fn mime_ext(ext: &str) -> &str { match ext { "jpg" | "jpeg" => "jpeg", "gif" => "gif", "bmp" => "bmp", _ => "png" } }

/// The picture bytes and file extension of a source, if it holds them (a data URL; web
/// addresses aren't fetched).
pub fn bytes(src: &str) -> Option<(Vec<u8>, String)> {
    let rest = src.strip_prefix("data:image/")?;
    let (kind, data) = rest.split_once(',')?;
    let ext = kind.split(';').next().unwrap_or("png");
    let ext = if ext == "jpeg" { "jpeg" } else { ext };
    if !kind.contains(";base64") { return None; }
    Some((unbase64(data)?, ext.to_string()))
}

/// Puts a picture into a cell as its value (the caller recalculates).
pub fn place(wb: &mut Workbook, sheet: usize, p: Pos, data: &[u8], ext: &str, alt: &str) {
    wb.set_cell_input(sheet, p, Input::Value(value(&data_url(data, ext), alt)), None);
}

/// Where a picture of `iw` x `ih` sits fitted (aspect kept, centered) in `r` [x0, y0, x1, y1].
pub fn fit(r: [f32; 4], iw: f32, ih: f32) -> [f32; 4] {
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    if iw <= 0.0 || ih <= 0.0 || w <= 0.0 || h <= 0.0 { return r; }
    let k = (w / iw).min(h / ih);
    let (fw, fh) = (iw * k, ih * k);
    [r[0] + (w - fw) / 2.0, r[1] + (h - fh) / 2.0, r[0] + (w + fw) / 2.0, r[1] + (h + fh) / 2.0]
}

/// The pixel size of a picture's bytes.
pub fn image_size(data: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().ok()?.into_dimensions().ok()
}

/// The workbook as saved: image values become their alt text, except placed pictures (constant
/// image cells), which become Excel rich values (xlsx/richdata.rs). None when nothing changes.
pub fn stage(wb: &Workbook) -> Option<Workbook> {
    let has = |s: &crate::book::Sheet| s.cells.values().any(|c| is_image(&c.value)) || s.spill.values().any(is_image);
    if !wb.sheets.iter().any(has) && !wb.parts.keys().any(|k| k.starts_with("xl/richData/")) { return None; }
    let mut w = wb.clone();
    let mut pics = vec![];
    for (si, s) in w.sheets.iter_mut().enumerate() {
        for (p, c) in s.cells.iter_mut() {
            let Some((src, alt)) = parse(&c.value).map(|(a, b)| (a.to_string(), b.to_string())) else { continue };
            let constant = matches!(c.input, Input::Value(_));
            c.value = Value::str(&alt);
            if constant {
                c.input = Input::Value(Value::str(&alt));
                if let Some((data, ext)) = bytes(&src) { pics.push((si, *p, data, ext, alt)); }
            }
        }
        for v in s.spill.values_mut() { if let Some((_, alt)) = parse(v) { *v = Value::str(alt); } }
    }
    crate::xlsx::richdata::stage(&mut w, pics);
    Some(w)
}

/// After reading: floating pictures saved for pictures in cells go back into their cells.
pub fn after_read(wb: &mut Workbook) {
    for si in 0..wb.sheets.len() {
        let mut placed = vec![];
        wb.sheets[si].objects.retain(|o| {
            let (ObjKind::Picture { data, ext }, Anchor::Two { from, .. }) = (&o.kind, &o.anchor) else { return true };
            let Some(rest) = o.name.strip_prefix(CELL_PICTURE) else { return true };
            let p = Pos::new(from.row, from.col);
            if rest.trim() != p.a1() { return true; }
            placed.push((p, data.clone(), ext.clone(), o.descr.clone()));
            false
        });
        for (p, data, ext, alt) in placed {
            place(wb, si, p, &data, &ext, &alt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0xF4, 0x22, 0x7F, 0x8A, 0, 0, 0, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x08, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x03, 0x00, 0x09, 0xFB, 0x03, 0xFD, 0xA3, 0x5B, 0xB4, 0x5E, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82];

    #[test]
    fn base64_round_trip() {
        for n in 0..10 { let d: Vec<u8> = (0..n).map(|i| (i * 37) as u8).collect(); assert_eq!(unbase64(&base64(&d)).unwrap(), d); }
        assert_eq!(base64(b"Man"), "TWFu");
    }

    #[test]
    fn image_value_and_function() {
        let v = value("data:image/png;base64,AAAA", "Logo");
        assert_eq!(parse(&v), Some(("data:image/png;base64,AAAA", "Logo")));
        assert!(parse(&Value::str("plain")).is_none());
        let mut wb = Workbook::new();
        wb.set_input(0, Pos::new(0, 0), "=IMAGE(\"https://example.com/a.png\",\"A picture\")");
        wb.recalc();
        assert_eq!(parse(&wb.sheets[0].value(Pos::new(0, 0))), Some(("https://example.com/a.png", "A picture")));
    }

    #[test]
    fn placed_picture_round_trips() {
        let mut wb = Workbook::new();
        place(&mut wb, 0, Pos::new(1, 1), PNG, "png", "Dot");
        wb.set_input(0, Pos::new(0, 0), "=IMAGE(\"https://example.com/a.png\",\"Web\")");
        wb.recalc();
        assert_eq!(image_size(&bytes(parse(&wb.sheets[0].value(Pos::new(1, 1))).unwrap().0).unwrap().0), Some((2, 1)));
        let out = crate::xlsx::write(&wb).unwrap();
        let back = crate::xlsx::read(&out).unwrap();
        assert!(back.sheets[0].objects.is_empty(), "the carrying picture went back into the cell");
        let (src, alt) = parse(&back.sheets[0].value(Pos::new(1, 1))).map(|(a, b)| (a.to_string(), b.to_string())).unwrap();
        assert_eq!(alt, "Dot");
        assert_eq!(bytes(&src).unwrap().0, PNG);
        assert!(is_image(&back.sheets[0].value(Pos::new(0, 0))), "IMAGE() is recalculated");
    }

    #[test]
    fn fit_keeps_aspect() { assert_eq!(fit([0.0, 0.0, 100.0, 20.0], 2.0, 1.0), [30.0, 0.0, 70.0, 20.0]); }
}
