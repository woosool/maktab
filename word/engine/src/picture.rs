//! Picture Format edits (rotate, flip, crop, corrections, color, transparency, effects) applied to
//! the pixels, so the saved document needs no DrawingML effect support to show them.

use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PictureOp {
    RotateRight,
    RotateLeft,
    FlipHorizontal,
    FlipVertical,
    /// Brightness and contrast changes in percent (-100..=100).
    Corrections { brightness: i32, contrast: i32 },
    Grayscale,
    Sepia,
    Washout,
    /// Blend toward a color (Recolor).
    Tint(crate::Color),
    /// Map luminance from a dark to a light color (a:duotone).
    Duotone(crate::Color, crate::Color),
    /// Transparency in percent of the current alpha (0 opaque … 100 invisible).
    Transparency(u8),
    Blur(f32),
    Sharpen,
    /// Clear pixels close to the corner (background) colors.
    RemoveBackground,
    Sketch,
    Invert,
    Posterize,
    /// Downsample to 150 ppi at the displayed size.
    Compress,
    /// A border of 4% of the shorter side.
    Frame(crate::Color),
    Rounded,
    Oval,
    SoftEdges,
    /// Crop fractions of the width/height removed from the left, top, right and bottom.
    Crop { left: f32, top: f32, right: f32, bottom: f32 },
}

pub fn pixel_size(bytes: &[u8]) -> Option<(f32, f32)> {
    let img = image::load_from_memory(bytes).ok()?;
    Some((img.width() as f32, img.height() as f32))
}

pub fn to_png(bytes: &[u8]) -> Option<Vec<u8>> { encode(&image::load_from_memory(bytes).ok()?) }

fn encode(img: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = std::io::Cursor::new(vec![]);
    img.write_to(&mut out, ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

fn map_pixels(img: DynamicImage, f: impl Fn([f32; 4]) -> [f32; 4]) -> DynamicImage {
    let mut rgba: RgbaImage = img.into_rgba8();
    for p in rgba.pixels_mut() {
        let c = f([p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]);
        *p = Rgba(c.map(|v| v.round().clamp(0.0, 255.0) as u8));
    }
    DynamicImage::ImageRgba8(rgba)
}

/// Apply `op` to encoded picture bytes; returns PNG bytes and the new display size for a
/// picture shown at `size` px.
pub fn apply(bytes: &[u8], op: PictureOp, size: (f32, f32)) -> Option<(Vec<u8>, (f32, f32))> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = size;
    let (out, size) = match op {
        PictureOp::RotateRight => (img.rotate90(), (h, w)),
        PictureOp::RotateLeft => (img.rotate270(), (h, w)),
        PictureOp::FlipHorizontal => (img.fliph(), size),
        PictureOp::FlipVertical => (img.flipv(), size),
        PictureOp::Corrections { brightness, contrast } => {
            let b = brightness as f32 / 100.0 * 255.0;
            let k = (1.0 + contrast as f32 / 100.0).max(0.0);
            (map_pixels(img, |[r, g, bl, a]| [(r - 128.0) * k + 128.0 + b, (g - 128.0) * k + 128.0 + b, (bl - 128.0) * k + 128.0 + b, a]), size)
        }
        PictureOp::Grayscale => (map_pixels(img, |[r, g, b, a]| { let y = 0.299 * r + 0.587 * g + 0.114 * b; [y, y, y, a] }), size),
        PictureOp::Sepia | PictureOp::Tint(_) | PictureOp::Duotone(..) => {
            let (d, l) = duotone_colors(op).unwrap_or((crate::Color::BLACK, crate::Color::WHITE));
            (map_pixels(img, |[r, g, b, a]| {
                let y = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
                let m = |x: u8, z: u8| x as f32 + (z as f32 - x as f32) * y;
                [m(d.0, l.0), m(d.1, l.1), m(d.2, l.2), a]
            }), size)
        }
        PictureOp::Washout => (map_pixels(img, |[r, g, b, a]| { let y = 0.299 * r + 0.587 * g + 0.114 * b; let w = |c: f32| 255.0 - (255.0 - (c * 0.5 + y * 0.5)) * 0.3; [w(r), w(g), w(b), a] }), size),
        PictureOp::Transparency(t) => { let k = 1.0 - t.min(100) as f32 / 100.0; (map_pixels(img, |[r, g, b, a]| [r, g, b, a * k]), size) }
        PictureOp::Blur(sigma) => (img.blur(sigma.max(0.1)), size),
        PictureOp::Sharpen => (img.unsharpen(1.5, 2), size),
        PictureOp::RemoveBackground => {
            let mut rgba = img.into_rgba8();
            let (pw, ph) = rgba.dimensions();
            let corners = [*rgba.get_pixel(0, 0), *rgba.get_pixel(pw - 1, 0), *rgba.get_pixel(0, ph - 1), *rgba.get_pixel(pw - 1, ph - 1)];
            // Flood from the borders through pixels near a corner color, so inner areas of the
            // same color stay.
            let near = |p: &Rgba<u8>| corners.iter().any(|c| (0..3).map(|k| (p[k] as i32 - c[k] as i32).abs()).sum::<i32>() < 60);
            let mut seen = vec![false; (pw * ph) as usize];
            let mut stack: Vec<(u32, u32)> = (0..pw).flat_map(|x| [(x, 0), (x, ph - 1)]).chain((0..ph).flat_map(|y| [(0, y), (pw - 1, y)])).collect();
            while let Some((x, y)) = stack.pop() {
                let i = (y * pw + x) as usize;
                if seen[i] || !near(rgba.get_pixel(x, y)) { continue; }
                seen[i] = true;
                rgba.get_pixel_mut(x, y)[3] = 0;
                if x > 0 { stack.push((x - 1, y)); }
                if y > 0 { stack.push((x, y - 1)); }
                if x + 1 < pw { stack.push((x + 1, y)); }
                if y + 1 < ph { stack.push((x, y + 1)); }
            }
            (DynamicImage::ImageRgba8(rgba), size)
        }
        PictureOp::Sketch => {
            // Edges of the luminance as dark pencil lines on white.
            let gray = img.to_luma8();
            let (pw, ph) = gray.dimensions();
            let mut out = RgbaImage::new(pw, ph);
            for y in 0..ph { for x in 0..pw {
                let l = |dx: i32, dy: i32| gray.get_pixel((x as i32 + dx).clamp(0, pw as i32 - 1) as u32, (y as i32 + dy).clamp(0, ph as i32 - 1) as u32)[0] as f32;
                let g = ((l(1, 0) - l(-1, 0)).powi(2) + (l(0, 1) - l(0, -1)).powi(2)).sqrt();
                let v = (255.0 - g * 1.5).clamp(0.0, 255.0) as u8;
                out.put_pixel(x, y, Rgba([v, v, v, 255]));
            } }
            (DynamicImage::ImageRgba8(out), size)
        }
        PictureOp::Invert => (map_pixels(img, |[r, g, b, a]| [255.0 - r, 255.0 - g, 255.0 - b, a]), size),
        PictureOp::Posterize => (map_pixels(img, |[r, g, b, a]| [r, g, b].map(|c| (c / 64.0).floor() * 85.0).into_iter().chain([a]).collect::<Vec<_>>().try_into().unwrap()), size),
        PictureOp::Compress => {
            let (tw, th) = ((w * 150.0 / 96.0).round().max(1.0) as u32, (h * 150.0 / 96.0).round().max(1.0) as u32);
            if tw >= img.width() { return None; }
            (img.resize_exact(tw, th, image::imageops::FilterType::Lanczos3), size)
        }
        PictureOp::Frame(c) => {
            // The border follows the picture's visible edge (its box, or the shape it was clipped
            // to): opaque pixels within the border width of transparency or the image edge.
            let mut rgba = img.into_rgba8();
            let (pw, ph) = rgba.dimensions();
            let b = ((pw.min(ph) as f32) * 0.04).ceil() as i64;
            let (w, h) = (pw as i64, ph as i64);
            let solid: Vec<bool> = rgba.pixels().map(|p| p[3] >= 128).collect();
            // Minimum filter of `solid` over a (2b+1)² window, done as rows then columns.
            let mut rows = vec![false; solid.len()];
            for y in 0..h { for x in 0..w {
                let (a, z) = ((x - b).max(-1), (x + b).min(w));
                rows[(y * w + x) as usize] = a >= 0 && z < w && (a..=z).all(|k| solid[(y * w + k) as usize]);
            } }
            for y in 0..h { for x in 0..w {
                let i = (y * w + x) as usize;
                if !solid[i] { continue; }
                let (a, z) = ((y - b).max(-1), (y + b).min(h));
                let inner = a >= 0 && z < h && (a..=z).all(|k| rows[(k * w + x) as usize]);
                if !inner { let p = rgba.get_pixel_mut(x as u32, y as u32); *p = Rgba([c.0, c.1, c.2, p[3]]); }
            } }
            (DynamicImage::ImageRgba8(rgba), size)
        }
        PictureOp::Rounded | PictureOp::Oval | PictureOp::SoftEdges => {
            let mut rgba = img.into_rgba8();
            let (pw, ph) = rgba.dimensions();
            let (fw, fh) = (pw as f32, ph as f32);
            let r = fw.min(fh) * 0.16667; // roundRect's default adjustment
            let feather = fw.min(fh) * 0.08;
            for (x, y, p) in rgba.enumerate_pixels_mut() {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                // Coverage 0..1 from the distance inside the shape's edge.
                let k = match op {
                    PictureOp::Oval => {
                        let (dx, dy) = ((px - fw / 2.0) / (fw / 2.0), (py - fh / 2.0) / (fh / 2.0));
                        ((1.0 - (dx * dx + dy * dy).sqrt()) * fw.min(fh) / 2.0 + 0.5).clamp(0.0, 1.0)
                    }
                    PictureOp::Rounded => {
                        let cx = px.clamp(r, fw - r);
                        let cy = py.clamp(r, fh - r);
                        (r - ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0)
                    }
                    _ => (px.min(py).min(fw - px).min(fh - py) / feather).clamp(0.0, 1.0),
                };
                p[3] = (p[3] as f32 * k).round() as u8;
            }
            (DynamicImage::ImageRgba8(rgba), size)
        }
        PictureOp::Crop { left, top, right, bottom } => {
            let (l, t, r, b) = (left.clamp(0.0, 0.95), top.clamp(0.0, 0.95), right.clamp(0.0, 0.95), bottom.clamp(0.0, 0.95));
            if l + r >= 0.95 || t + b >= 0.95 { return None; }
            let (pw, ph) = img.dimensions();
            let x = (l * pw as f32).round() as u32;
            let y = (t * ph as f32).round() as u32;
            let cw = (((1.0 - l - r) * pw as f32).round() as u32).clamp(1, pw - x);
            let ch = (((1.0 - t - b) * ph as f32).round() as u32).clamp(1, ph - y);
            (img.crop_imm(x, y, cw, ch), (w * (1.0 - l - r), h * (1.0 - t - b)))
        }
    };
    Some((encode(&out)?, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut img = RgbaImage::new(w, h);
        for (x, _, p) in img.enumerate_pixels_mut() { *p = if x == 0 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 255]) }; }
        encode(&DynamicImage::ImageRgba8(img)).unwrap()
    }

    #[test]
    fn rotate_flip_crop_and_color() {
        let src = png(4, 2);
        let (out, size) = apply(&src, PictureOp::RotateRight, (40.0, 20.0)).unwrap();
        assert_eq!(size, (20.0, 40.0));
        assert_eq!(pixel_size(&out), Some((2.0, 4.0)));
        let (out, _) = apply(&src, PictureOp::FlipHorizontal, (40.0, 20.0)).unwrap();
        let img = image::load_from_memory(&out).unwrap().into_rgba8();
        assert_eq!(img.get_pixel(3, 0)[0], 255, "the red column moves to the right edge");
        let (out, size) = apply(&src, PictureOp::Crop { left: 0.25, top: 0.0, right: 0.0, bottom: 0.0 }, (40.0, 20.0)).unwrap();
        assert_eq!((pixel_size(&out), size), (Some((3.0, 2.0)), (30.0, 20.0)));
        let (out, _) = apply(&src, PictureOp::Grayscale, (4.0, 2.0)).unwrap();
        let p = *image::load_from_memory(&out).unwrap().into_rgba8().get_pixel(1, 0);
        assert!(p[0] == p[1] && p[1] == p[2]);
        let (out, _) = apply(&src, PictureOp::Transparency(50), (4.0, 2.0)).unwrap();
        assert_eq!(image::load_from_memory(&out).unwrap().into_rgba8().get_pixel(1, 0)[3], 128);
    }

    #[test]
    fn document_picture_edits_undo_and_reset() {
        let mut doc = crate::model::Document::new(vec![crate::model::Paragraph::new("")]);
        doc.insert_image(png(4, 2), "image/png", 40.0, 20.0);
        let pos = 0;
        let first = doc.image_at(pos).unwrap().asset.clone();
        assert!(doc.apply_picture_op(pos, PictureOp::RotateLeft));
        let img = doc.image_at(pos).unwrap();
        assert_eq!((img.width, img.height), (20.0, 40.0));
        assert_eq!(img.original.as_deref(), Some(first.as_str()));
        doc.reset_picture(pos);
        let img = doc.image_at(pos).unwrap();
        assert_eq!((img.asset.as_str(), img.width, img.height), (first.as_str(), 4.0, 2.0));
        doc.undo();
        assert_eq!(doc.image_at(pos).unwrap().width, 20.0);
    }
}

/// A picture's opaque outline for tight wrapping, in 0..1 units of its width and height: the
/// left edge of each row band downward, then the right edge back up. None when the picture has
/// no transparent pixels (its box is the outline). Cached per asset.
pub fn contour(key: &str, bytes: &[u8]) -> Option<std::sync::Arc<Vec<(f32, f32)>>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<(String, usize), Option<Arc<Vec<(f32, f32)>>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let k = (key.to_string(), bytes.len());
    if let Some(v) = cache.lock().ok()?.get(&k) { return v.clone(); }
    let made = (|| {
        let img = image::load_from_memory(bytes).ok()?;
        let img = img.thumbnail(128, 128).to_rgba8();
        let (w, h) = img.dimensions();
        if img.pixels().all(|p| p[3] > 32) { return None; }
        let bands = h.min(48);
        let (mut left, mut right) = (vec![], vec![]);
        for b in 0..bands {
            let (r0, r1) = (b * h / bands, ((b + 1) * h / bands).max(b * h / bands + 1));
            let (mut lo, mut hi) = (u32::MAX, 0);
            for y in r0..r1.min(h) { for x in 0..w { if img.get_pixel(x, y)[3] > 32 { lo = lo.min(x); hi = hi.max(x + 1); } } }
            if lo > hi { continue; }
            let (y0, y1) = (r0 as f32 / h as f32, r1 as f32 / h as f32);
            left.push((lo as f32 / w as f32, y0)); left.push((lo as f32 / w as f32, y1));
            right.push((hi as f32 / w as f32, y0)); right.push((hi as f32 / w as f32, y1));
        }
        if left.is_empty() { return None; }
        left.extend(right.into_iter().rev());
        Some(Arc::new(left))
    })();
    if let Ok(mut c) = cache.lock() { c.insert(k, made.clone()); }
    made
}

/// The dark and light colors of a recolor effect (Sepia, Tint, Duotone).
pub fn duotone_colors(op: PictureOp) -> Option<(crate::Color, crate::Color)> {
    use crate::Color;
    match op {
        PictureOp::Sepia => Some((Color(0x3B, 0x2A, 0x17), Color(0xF4, 0xE6, 0xC8))),
        PictureOp::Tint(c) => Some((Color(c.0 / 3, c.1 / 3, c.2 / 3), Color(255 - (255 - c.0) / 6, 255 - (255 - c.1) / 6, 255 - (255 - c.2) / 6))),
        PictureOp::Duotone(d, l) => Some((d, l)),
        _ => None,
    }
}

/// Whether Word can express the edit as DrawingML over the original picture.
pub fn representable(op: &PictureOp) -> bool {
    matches!(op, PictureOp::Corrections { .. } | PictureOp::Grayscale | PictureOp::Washout | PictureOp::Sepia | PictureOp::Tint(_) | PictureOp::Duotone(..)
        | PictureOp::Transparency(_) | PictureOp::Crop { .. } | PictureOp::Frame(_) | PictureOp::Rounded | PictureOp::Oval | PictureOp::SoftEdges)
}

/// A picture's edits as DrawingML (§20.1.8): effects inside `a:blip`, the crop (`a:srcRect`),
/// the shape (`a:prstGeom`), a border (`a:ln`) and soft edges (`a:effectLst`). None when an
/// edit has no DrawingML form (the picture is then saved with its pixels baked).
pub struct PictureXml { pub blip: String, pub src_rect: String, pub geom: &'static str, pub ln: String, pub effects: String }

pub fn drawingml(ops: &[PictureOp], w: f32, h: f32) -> Option<PictureXml> {
    if ops.iter().any(|o| !representable(o)) { return None; }
    let emu = |px: f32| (px * 9525.0).round() as i64;
    let (mut bright, mut contrast, mut gray, mut alpha) = (0i32, 0i32, false, 1.0f32);
    let mut duo: Option<(crate::Color, crate::Color)> = None;
    let (mut l, mut t, mut r, mut b) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let (mut geom, mut ln, mut soft) = ("rect", String::new(), String::new());
    for op in ops {
        match *op {
            PictureOp::Corrections { brightness, contrast: c } => { bright += brightness; contrast += c; }
            PictureOp::Washout => { bright += 70; contrast -= 70; }
            PictureOp::Grayscale => gray = true,
            PictureOp::Transparency(p) => alpha *= 1.0 - p.min(100) as f32 / 100.0,
            PictureOp::Crop { left, top, right, bottom } => {
                // Later crops are fractions of what earlier ones kept.
                let (kw, kh) = (1.0 - l - r, 1.0 - t - b);
                l += left * kw; r += right * kw; t += top * kh; b += bottom * kh;
            }
            PictureOp::Frame(c) => ln = format!(r#"<a:ln w="{}"><a:solidFill><a:srgbClr val="{}"/></a:solidFill></a:ln>"#, emu(w.min(h) * 0.04), c.hex()),
            PictureOp::Rounded => geom = "roundRect",
            PictureOp::Oval => geom = "ellipse",
            PictureOp::SoftEdges => soft = format!(r#"<a:effectLst><a:softEdge rad="{}"/></a:effectLst>"#, emu(w.min(h) * 0.08)),
            o => duo = duotone_colors(o).or(duo),
        }
    }
    let mut blip = String::new();
    if alpha < 1.0 { blip += &format!(r#"<a:alphaModFix amt="{}"/>"#, (alpha * 100000.0).round() as i64); }
    if let Some((d, lt)) = duo { blip += &format!(r#"<a:duotone><a:srgbClr val="{}"/><a:srgbClr val="{}"/></a:duotone>"#, d.hex(), lt.hex()); }
    if gray { blip += "<a:grayscl/>"; }
    if bright != 0 || contrast != 0 { blip += &format!(r#"<a:lum bright="{}" contrast="{}"/>"#, bright.clamp(-100, 100) * 1000, contrast.clamp(-100, 100) * 1000); }
    let pct = |v: f32| (v * 100000.0).round() as i64;
    let src_rect = if l + t + r + b > 0.0 { format!(r#"<a:srcRect l="{}" t="{}" r="{}" b="{}"/>"#, pct(l), pct(t), pct(r), pct(b)) } else { String::new() };
    Some(PictureXml { blip, src_rect, geom, ln, effects: soft })
}

/// Apply edits in Word's order (crop, color, transparency, then the picture's shape), for
/// showing a picture read with DrawingML effects. Returns PNG bytes.
pub fn bake(bytes: &[u8], ops: &[PictureOp]) -> Option<Vec<u8>> {
    // The border comes after the picture's shape, so it traces the shape's edge as Word does.
    let rank = |o: &PictureOp| match o { PictureOp::Crop { .. } => 0, PictureOp::Transparency(_) => 2, PictureOp::Rounded | PictureOp::Oval | PictureOp::SoftEdges => 3, PictureOp::Frame(_) => 4, _ => 1 };
    let mut sorted = ops.to_vec();
    sorted.sort_by_key(rank);
    let mut cur = bytes.to_vec();
    for op in sorted { cur = apply(&cur, op, (100.0, 100.0))?.0; }
    Some(cur)
}
