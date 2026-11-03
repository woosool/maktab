//! Renders `pdf/assets/icon.svg` into the app's icon files:
//!
//!   cargo run --release --manifest-path pdf/tools/render-icon/Cargo.toml --target-dir pdf/target
//!
//! Writes `assets/icon.png` (512 px), `assets/icon-full.png` (1254 px), `assets/icon.ico`
//! (16/32/48/256) and `assets/Waraq.iconset/` (for `iconutil -c icns`), each size drawn from
//! the vector source rather than scaled down.

use std::path::{Path, PathBuf};

fn render(tree: &resvg::usvg::Tree, size: u32) -> image::RgbaImage {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let s = size as f32 / tree.size().width();
    resvg::render(tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut pixmap.as_mut());
    // tiny-skia stores premultiplied colour; PNG wants it straight.
    let mut img = image::RgbaImage::new(size, size);
    for (i, px) in pixmap.pixels().iter().enumerate() {
        let c = px.demultiply();
        img.as_mut()[i * 4..i * 4 + 4].copy_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    img
}

fn main() {
    let assets: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--compare") {
        compare(Path::new(&args[2]), &assets);
        return;
    }
    let svg = std::fs::read(assets.join("icon.svg")).expect("icon.svg");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default()).expect("valid SVG");
    render(&tree, 512).save(assets.join("icon.png")).unwrap();
    render(&tree, 1254).save(assets.join("icon-full.png")).unwrap();
    // Windows icon.
    let frames: Vec<image::codecs::ico::IcoFrame> = [16u32, 32, 48, 256]
        .iter()
        .map(|&s| {
            let img = render(&tree, s);
            image::codecs::ico::IcoFrame::as_png(img.as_raw(), s, s, image::ExtendedColorType::Rgba8).unwrap()
        })
        .collect();
    let file = std::fs::File::create(assets.join("icon.ico")).unwrap();
    image::codecs::ico::IcoEncoder::new(file).encode_images(&frames).unwrap();
    // macOS iconset (iconutil -c icns assets/Waraq.iconset).
    let set = assets.join("Waraq.iconset");
    std::fs::create_dir_all(&set).unwrap();
    for s in [16u32, 32, 128, 256, 512] {
        render(&tree, s).save(set.join(format!("icon_{s}x{s}.png"))).unwrap();
        render(&tree, s * 2).save(set.join(format!("icon_{s}x{s}@2x.png"))).unwrap();
    }
    println!("icons written to {}", assets.display());
}

/// `render-icon --compare out.png`: Qalam, Jadwal and Waraq side by side at 256 and 32 px.
fn compare(out: &Path, assets: &Path) {
    let load = |p: PathBuf| image::open(p).unwrap().to_rgba8();
    let icons = [load(assets.join("../../qalam/assets/icon.png")), load(assets.join("../../jadwal/assets/icon.png")), load(assets.join("icon.png"))];
    let mut canvas = image::RgbaImage::from_pixel(3 * 280 + 20, 340, image::Rgba([236, 235, 231, 255]));
    for (i, icon) in icons.iter().enumerate() {
        let big = image::imageops::resize(icon, 256, 256, image::imageops::FilterType::Lanczos3);
        image::imageops::overlay(&mut canvas, &big, 20 + i as i64 * 280, 12);
        let small = image::imageops::resize(icon, 32, 32, image::imageops::FilterType::Lanczos3);
        image::imageops::overlay(&mut canvas, &small, 20 + i as i64 * 280 + 112, 290);
    }
    canvas.save(out).unwrap();
}
