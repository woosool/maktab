//! Build the Windows icon (assets/icon.ico) from the artwork: 16–256 px in one file.
//!   cargo run -p word-engine --example make_ico -- assets/icon-full.png assets/icon.ico

use image::codecs::ico::{IcoEncoder, IcoFrame};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (src, dst) = (args.get(1).map(String::as_str).unwrap_or("assets/icon-full.png"), args.get(2).map(String::as_str).unwrap_or("assets/icon.ico"));
    let img = image::open(src).expect("artwork").to_rgba8();
    let frames: Vec<IcoFrame> = [16u32, 24, 32, 48, 64, 128, 256].iter().map(|&s| {
        let r = image::imageops::resize(&img, s, s, image::imageops::FilterType::Lanczos3);
        IcoFrame::as_png(r.as_raw(), s, s, image::ExtendedColorType::Rgba8).expect("frame")
    }).collect();
    IcoEncoder::new(std::fs::File::create(dst).expect("output")).encode_images(&frames).expect("encode");
    println!("wrote {dst}");
}
