//! Random editing to find panics: `cargo run --release --example fuzz -- [seeds] [steps]`.
use word_engine::model::*;
use word_engine::*;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    fn below(&mut self, n: usize) -> usize { if n == 0 { 0 } else { (self.next() % n as u64) as usize } }
}

fn step(d: &mut Document, r: &mut Rng, log: &mut Vec<String>) {
    let len = d.len();
    let a = r.below(len + 1);
    let b = if r.below(3) == 0 { a } else { r.below(len + 1) };
    d.select(a, b);
    let op = r.below(40);
    log.push(format!("select({a},{b}) op {op}"));
    match op {
        0..=7 => { let _ = d.replace(["x", "hello ", "مرحبا ", "😀", "\t", "a b c ", "שלום"][r.below(7)]); }
        8..=10 => { let _ = d.delete(r.below(2) == 0, r.below(4) == 0); }
        11 | 12 => { let _ = d.insert_paragraph(); }
        13 => { let _ = d.insert_table(1 + r.below(3), 1 + r.below(4)); }
        14 => { let _ = d.insert_table_row(r.below(2) == 0); }
        15 => { let _ = d.insert_table_column(r.below(2) == 0); }
        16 => { let _ = d.delete_table_part([word_engine::model::TablePart::Row, word_engine::model::TablePart::Column, word_engine::model::TablePart::Table][r.below(3)]); }
        17 => { d.undo(); }
        18 => { d.redo(); }
        19 => { let _ = d.insert_object(word_engine::objects::Object::Equation(word_engine::equation::Equation::new("\\frac{a}{b}+\\sum_{k=0}^{n} x"))); }
        20 => { d.add_comment("Me", "note", "2026-01-01T00:00:00Z"); }
        21 => { let _ = d.insert_page_break(); }
        22 => { let _ = d.insert_section_break(word_engine::page::SectionKind::Continuous); }
        23 => { let _ = d.set_columns(word_engine::page::Columns { count: 1 + r.below(3) as u8, ..Default::default() }); }
        24 => { let _ = d.toggle_bold(); }
        25 => { let _ = d.set_paragraph_style(["Heading1", "Heading2", "Normal"][r.below(3)]); }
        26 => { let _ = d.insert_toc(Default::default()); }
        27 => { let _ = d.insert_caption("Figure", "x", false); }
        28 => { let _ = d.update_tables(); }
        29 => { d.insert_note(NoteKind::Foot); }
        30 => { let _ = d.insert_line_break(); }
        31 => { let f = d.fragment(); let e = d.range().1; d.select(e, e); d.paste_fragment(f); }
        32 => { let _ = d.toggle_list(word_engine::numbering::ListKind::Bullet); }
        33 => { let _ = d.move_paragraphs(r.below(2) == 0); }
        34 => { let _ = d.set_track_changes(r.below(2) == 0); }
        35 => { let _ = d.insert_tab(); }
        36 => { let _ = d.move_cell(r.below(2) == 0); }
        37 => { let _ = d.insert_index(2); }
        38 => { let _ = d.set_alignment(Alignment::Center); }
        _ => { let _ = d.set_line_numbers(None); }
    }
    if r.below(6) == 0 { log.push("layout".into()); let _ = word_engine::layout_document(d); }
    if r.below(25) == 0 { log.push("docx roundtrip".into()); let bytes = word_engine::docx::write(d).unwrap(); *d = word_engine::docx::read(&bytes).unwrap(); }
}

fn main() {
    let args: Vec<usize> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let (seeds, steps) = (args.first().copied().unwrap_or(200), args.get(1).copied().unwrap_or(300));
    std::panic::set_hook(Box::new(|_| {}));
    let mut failures = std::collections::BTreeMap::new();
    for seed in 1..=seeds as u64 {
        let mut log = vec![];
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut d = Document::new(vec![Paragraph::new("Start")]);
            let mut r = Rng(seed * 0x9E37_79B9_7F4A_7C15 | 1);
            for _ in 0..steps { step(&mut d, &mut r, &mut log); }
            let _ = word_engine::layout_document(&d);
        }));
        if let Err(e) = res {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            failures.entry(msg).or_insert((seed, log.iter().rev().take(6).rev().cloned().collect::<Vec<_>>()));
        }
    }
    for (msg, (seed, tail)) in &failures { println!("seed {seed}: {msg}\n   {}", tail.join("\n   ")); }
    println!("{} distinct panics", failures.len());
}
