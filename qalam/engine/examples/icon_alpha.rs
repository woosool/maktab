//! Make an icon's flat background transparent: `cargo run --example icon_alpha -- in.png out.png [size]`.
//! The background is the colour at the corners; it's flood-filled from the edges, and pixels
//! near that colour along the shape's edge get partial transparency for a smooth outline.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = image::open(&a[1]).expect("open").to_rgba8();
    let (w, h) = img.dimensions();
    let bg = *img.get_pixel(2, 2);
    println!("corner {:?} centre {:?}", bg, img.get_pixel(w / 2, h / 2));
    if a.len() < 3 { return; }
    let dist = |p: &image::Rgba<u8>| -> f32 { ((p[0] as f32 - bg[0] as f32).powi(2) + (p[1] as f32 - bg[1] as f32).powi(2) + (p[2] as f32 - bg[2] as f32).powi(2)).sqrt() };
    let mut out = img.clone();
    let mut seen = vec![false; (w * h) as usize];
    let mut stack: Vec<(u32, u32)> = vec![];
    for x in 0..w { stack.push((x, 0)); stack.push((x, h - 1)); }
    for y in 0..h { stack.push((0, y)); stack.push((w - 1, y)); }
    let (hard, soft) = (40.0f32, 140.0f32);
    while let Some((x, y)) = stack.pop() {
        let i = (y * w + x) as usize;
        if seen[i] { continue; }
        seen[i] = true;
        let p = *img.get_pixel(x, y);
        let d = dist(&p);
        if d >= soft { continue; }
        // Background: fully clear; the anti-aliased rim: partly clear.
        let alpha = if d < hard { 0.0 } else { (d - hard) / (soft - hard) };
        let px = out.get_pixel_mut(x, y);
        px[3] = (px[3] as f32 * alpha).round() as u8;
        if d < hard {
            if x > 0 { stack.push((x - 1, y)); }
            if y > 0 { stack.push((x, y - 1)); }
            if x + 1 < w { stack.push((x + 1, y)); }
            if y + 1 < h { stack.push((x, y + 1)); }
        }
    }
    let size: u32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(w);
    let out = if size != w { image::imageops::resize(&out, size, size, image::imageops::FilterType::Lanczos3) } else { out };
    out.save(&a[2]).expect("save");
    let clear = out.pixels().filter(|p| p[3] == 0).count();
    println!("saved {} ({}% transparent)", a[2], clear * 100 / out.pixels().len());
}
