//! Opens a spreadsheet and times what an edit costs on it (undo copy, recalculation, save).
fn main() {
    let p = std::env::args().nth(1).expect("path");
    let t = std::time::Instant::now();
    let mut b = sheet_engine::open(std::path::Path::new(&p)).expect("open");
    let formulas: usize = b.sheets.iter().map(|s| s.cells.values().filter(|c| c.is_formula()).count()).sum();
    eprintln!("opened in {:?}; {} formulas", t.elapsed(), formulas);
    let t = std::time::Instant::now(); let copy = b.clone(); eprintln!("clone {:?}", t.elapsed()); drop(copy);
    let t = std::time::Instant::now(); b.set_input(0, sheet_engine::Pos::new(0, 0), "x"); b.recalc(); eprintln!("edit+recalc {:?}", t.elapsed());
    let t = std::time::Instant::now(); let _ = b.sheets.iter().map(|s| s.extent()).count(); eprintln!("extent {:?}", t.elapsed());
}
