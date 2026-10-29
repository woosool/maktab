//! Render a document's pages to SVG: `cargo run --example render -- in.docx out-dir [pages]`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, out) = (&args[1], std::path::Path::new(&args[2]));
    let pages: usize = args.get(3).and_then(|p| p.parse().ok()).unwrap_or(3);
    let bytes = std::fs::read(input).expect("read input");
    let doc = if input.ends_with(".txt") { word_engine::import::from_text(&String::from_utf8_lossy(&bytes)) } else if input.ends_with(".md") { word_engine::import::from_markdown(&String::from_utf8_lossy(&bytes)) } else { word_engine::docx::read(&bytes).expect("open docx") };
    let t = std::time::Instant::now();
    let layout = word_engine::layout_document(&doc);
    eprintln!("{} pages, {} lines laid out in {:?}", layout.page_count(), layout.lines.len(), t.elapsed());
    let mut cache = word_engine::MeasureCache::default();
    let _ = word_engine::layout_with_cache(&doc, &mut cache);
    let mut edited = doc;
    edited.select(10, 10);
    edited.replace("x");
    let t = std::time::Instant::now();
    let _ = word_engine::layout_with_cache(&edited, &mut cache);
    eprintln!("re-layout after typing one character: {:?} ({} reused, {} measured)", t.elapsed(), cache.hits, cache.misses);
    let doc = edited;
    std::fs::create_dir_all(out).unwrap();
    std::fs::write(out.join("doc.pdf"), word_engine::pdf::export(&doc, &layout, "Document")).unwrap();
    for i in 0..layout.page_count().min(pages) {
        std::fs::write(out.join(format!("page{}.svg", i + 1)), word_engine::svg::page_svg(&doc, &layout, i)).unwrap();
    }
}
