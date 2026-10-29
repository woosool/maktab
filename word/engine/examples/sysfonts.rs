//! Load system fonts, lay out text in a few of them, and export a PDF: `cargo run --example sysfonts -- out.pdf`.
use word_engine::model::*;
fn main() {
    let out = std::env::args().nth(1).unwrap_or("sysfonts.pdf".into());
    let t = std::time::Instant::now();
    let n = word_engine::fonts::load_system_fonts(Some(&std::env::temp_dir().join("qalam-font-index.txt")));
    println!("{n} system faces in {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let n2 = word_engine::fonts::load_system_fonts(None);
    println!("again: {n2} ({:?})", t.elapsed());
    for name in ["Georgia", "Helvetica", "Arial", "Times New Roman", "Courier New", "Calibri"] {
        let k = word_engine::fonts::resolve(name, false, false);
        let f = word_engine::fonts::face(k.face);
        println!("{name:16} -> {} ({})", f.family, f.path.unwrap_or("bundled"));
    }
    let mut d = Document::new(vec![Paragraph::new("Georgia text here."), Paragraph::new("Helvetica text here."), Paragraph::new("Arial text here.")]);
    for (i, f) in ["Georgia", "Helvetica", "Arial"].iter().enumerate() {
        let s = d.paragraph_start(i); let e = s + d.paragraphs[i].len();
        d.select(s, e);
        d.set_run_props(word_engine::RunProps { font: Some(f.to_string()), ..Default::default() });
    }
    let l = word_engine::layout_document(&d);
    std::fs::write(&out, word_engine::pdf::export(&d, &l, "fonts")).unwrap();
    println!("families offered: {}", word_engine::fonts::family_names().len());
}
