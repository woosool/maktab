//! Renders an app icon SVG into icon.png (512 px), icon-full.png (1024 px) and icon.ico.
//! Run from saboura/: cargo run -p saboura-app --example render_icon [svg] [out dir]
//! (defaults: tools/icon.svg and assets/).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let svg_path = args.first().cloned().unwrap_or_else(|| "tools/icon.svg".into());
    let out = std::path::PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "assets".into()));
    std::fs::create_dir_all(&out).expect("output folder");
    let svg = std::fs::read(&svg_path).expect("icon SVG");
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_fonts_dir("../qalam/assets/fonts");
    let tree = resvg::usvg::Tree::from_data(&svg, &opt).expect("valid SVG");
    let render = |size: u32| {
        let mut pix = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
        let s = size as f32 / tree.size().width();
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut pix.as_mut());
        image::RgbaImage::from_raw(size, size, pix.take_demultiplied()).unwrap()
    };
    render(512).save(out.join("icon.png")).unwrap();
    render(1024).save(out.join("icon-full.png")).unwrap();
    let mut ico = image::codecs::ico::IcoEncoder::new(std::fs::File::create(out.join("icon.ico")).unwrap());
    let frames: Vec<_> = [16, 32, 48, 64, 128, 256].iter().map(|&s| image::codecs::ico::IcoFrame::as_png(render(s).as_raw(), s, s, image::ExtendedColorType::Rgba8).unwrap()).collect();
    ico.encode_images(&frames).unwrap();
}
