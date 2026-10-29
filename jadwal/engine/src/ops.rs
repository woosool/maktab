//! More editing operations (workbook.js): moving cells with their references, shifting cells to
//! insert or delete, the current region, and the status bar's selection totals.

use crate::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
use crate::book::{Cell, Formula, Input, Workbook};
use crate::formula::adjust;
use crate::value::Value;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats { pub count: usize, pub nums: usize, pub sum: f64, pub min: f64, pub max: f64 }

impl Workbook {
    /// Moves cells (cut and paste, or dragging the selection's border): formulas anywhere that
    /// pointed into the moved cells follow them.
    pub fn move_range(&mut self, from_sheet: usize, src: Range, to_sheet: usize, at: Pos) {
        let dr = at.row as i64 - src.r0 as i64; let dc = at.col as i64 - src.c0 as i64;
        if (dr == 0 && dc == 0 && from_sheet == to_sheet) || at.row + src.rows() > MAX_ROWS || at.col + src.cols() > MAX_COLS { return; }
        let src_name = self.sheets[from_sheet].name.clone();
        let dst_name = self.sheets[to_sheet].name.clone();
        // Take the cells out first.
        let taken: Vec<(Pos, Cell)> = {
            let s = &mut self.sheets[from_sheet];
            let keys: Vec<Pos> = s.cells.keys().copied().filter(|p| src.contains(*p)).collect();
            keys.into_iter().map(|p| (p, s.cells.remove(&p).unwrap())).collect()
        };
        let notes: Vec<(Pos, String)> = { let s = &mut self.sheets[from_sheet]; let k: Vec<Pos> = s.notes.keys().copied().filter(|p| src.contains(*p)).collect(); k.into_iter().map(|p| (p, s.notes.remove(&p).unwrap())).collect() };
        let merges: Vec<Range> = { let s = &mut self.sheets[from_sheet]; let (inside, rest): (Vec<Range>, Vec<Range>) = s.merges.iter().partition(|m| src.contains(m.start()) && src.contains(m.end())); s.merges = rest; inside };
        // References into the moved block follow it (across sheets: the sheet name is added).
        if from_sheet == to_sheet {
            self.rewrite_formulas(|own, f| adjust::move_area(f, own, &src_name, src, dr, dc));
        } else {
            let to = dst_name.clone();
            self.rewrite_formulas(|own, f| moved_to_sheet(f, own, &src_name, &to, src, dr, dc));
        }
        let dest = Range { r0: at.row, c0: at.col, r1: at.row + src.rows() - 1, c1: at.col + src.cols() - 1 };
        self.clear(to_sheet, dest, true, true, true);
        let s = &mut self.sheets[to_sheet];
        for (p, mut c) in taken {
            // The moved formulas were rewritten in place before the move only if on this sheet;
            // re-read the rewritten text from the cell itself.
            if let Input::Formula(f) = &c.input { if from_sheet != to_sheet { c.input = Input::Formula(Arc::new(Formula::new(&f.text))); } }
            s.cells.insert(Pos::new((p.row as i64 + dr) as u32, (p.col as i64 + dc) as u32), c);
        }
        for (p, n) in notes { s.notes.insert(Pos::new((p.row as i64 + dr) as u32, (p.col as i64 + dc) as u32), n); }
        for m in merges { s.merges.push(Range { r0: (m.r0 as i64 + dr) as u32, r1: (m.r1 as i64 + dr) as u32, c0: (m.c0 as i64 + dc) as u32, c1: (m.c1 as i64 + dc) as u32 }); }
        if from_sheet == to_sheet { s.move_rules(src, dr, dc); }
        crate::follow::follow(self, crate::follow::Edit::Move { from: from_sheet, src, to: to_sheet, at });
    }

    /// Inserts blank cells over `r`, pushing what was there down (or right), or deletes the cells
    /// of `r`, pulling the rest up (or left).
    pub fn shift_cells(&mut self, sheet: usize, r: Range, down: bool, delete: bool) {
        let name = self.sheets[sheet].name.clone();
        let (n, max) = if down { (r.rows() as i64, MAX_ROWS) } else { (r.cols() as i64, MAX_COLS) };
        // The block that moves: below (or right of) the range, within its columns (or rows).
        let block = if down { Range { r0: if delete { r.r1 + 1 } else { r.r0 }, r1: MAX_ROWS - 1, ..r } } else { Range { c0: if delete { r.c1 + 1 } else { r.c0 }, c1: MAX_COLS - 1, ..r } };
        if delete { self.clear(sheet, r, true, true, true); }
        let d = if delete { -n } else { n };
        let (dr, dc) = if down { (d, 0) } else { (0, d) };
        // Excel's adjustment: references spanning the insertion point grow, those past it move.
        self.rewrite_formulas(|own, f| adjust::shift_cells(f, own, &name, r, down, delete));
        let s = &mut self.sheets[sheet];
        let moving: Vec<(Pos, Cell)> = { let k: Vec<Pos> = s.cells.keys().copied().filter(|p| block.contains(*p)).collect(); k.into_iter().map(|p| (p, s.cells.remove(&p).unwrap())).collect() };
        for (p, c) in moving {
            let q = (p.row as i64 + dr, p.col as i64 + dc);
            if q.0 >= 0 && q.1 >= 0 && (if down { q.0 } else { q.1 }) < max as i64 { s.cells.insert(Pos::new(q.0 as u32, q.1 as u32), c); }
        }
        crate::follow::follow(self, crate::follow::Edit::ShiftCells { sheet, r, down, delete });
    }

    /// The block of filled cells around `p` bounded by blank rows and columns (Ctrl+A, Sort, Filter).
    pub fn current_region(&self, sheet: usize, p: Pos) -> Range {
        let s = &self.sheets[sheet];
        let filled = |r: u32, c: u32| !s.value(Pos::new(r, c)).is_empty();
        let mut g = Range::cell(p);
        loop {
            let mut grown = g;
            let (r0, r1, c0, c1) = (g.r0.saturating_sub(1), (g.r1 + 1).min(MAX_ROWS - 1), g.c0.saturating_sub(1), (g.c1 + 1).min(MAX_COLS - 1));
            if g.r0 > 0 && (c0..=c1).any(|c| filled(r0, c)) { grown.r0 = r0; }
            if g.r1 < MAX_ROWS - 1 && (c0..=c1).any(|c| filled(r1, c)) { grown.r1 = r1; }
            if g.c0 > 0 && (r0..=r1).any(|r| filled(r, c0)) { grown.c0 = c0; }
            if g.c1 < MAX_COLS - 1 && (r0..=r1).any(|r| filled(r, c1)) { grown.c1 = c1; }
            if grown == g { return g; }
            g = grown;
        }
    }

    /// Does the first row read as headers (text over columns with numbers or dates)?
    pub fn looks_like_header(&self, sheet: usize, r: Range) -> bool {
        let s = &self.sheets[sheet];
        let first_text = (r.c0..=r.c1).all(|c| matches!(s.value(Pos::new(r.r0, c)), Value::Str(_)));
        let below_differs = (r.c0..=r.c1).any(|c| !matches!(s.value(Pos::new(r.r0 + 1, c)), Value::Str(_)));
        let bold = (r.c0..=r.c1).any(|c| self.style_at(sheet, Pos::new(r.r0, c)).bold) && !(r.c0..=r.c1).any(|c| self.style_at(sheet, Pos::new(r.r0 + 1, c)).bold);
        first_text && (below_differs || bold)
    }

    /// Count, sum, min and max of the selected cells (each counted once).
    pub fn selection_stats(&self, sheet: usize, ranges: &[Range]) -> Stats {
        let s = &self.sheets[sheet];
        let mut st = Stats { min: f64::INFINITY, max: f64::NEG_INFINITY, ..Default::default() };
        let mut seen = std::collections::HashSet::new();
        let ext = s.extent().unwrap_or_default();
        for r in ranges {
            let r = Range { r1: r.r1.min(ext.row), c1: r.c1.min(ext.col), ..*r };
            if r.r0 > r.r1 || r.c0 > r.c1 { continue; }
            let mut visit = |p: Pos| {
                if !seen.insert(p) || s.hidden_rows.contains(&p.row) { return; }
                match s.value(p) { Value::Empty => {} Value::Num(n) => { st.count += 1; st.nums += 1; st.sum += n; st.min = st.min.min(n); st.max = st.max.max(n); } _ => st.count += 1 }
            };
            if (r.rows() as u64) * (r.cols() as u64) > 4 * (s.cells.len() + s.spill.len()) as u64 + 64 {
                let mut ps: Vec<Pos> = s.cells.keys().chain(s.spill.keys()).copied().filter(|p| r.contains(*p)).collect();
                ps.sort(); ps.dedup();
                for p in ps { visit(p); }
            } else { for p in r.iter() { visit(p); } }
        }
        st
    }
}

/// References from anywhere into `src` on sheet `from` now point at the moved cells on `to`.
pub(crate) fn moved_to_sheet(f: &str, own: &str, from: &str, to: &str, src: Range, dr: i64, dc: i64) -> String {
    let mut hit = false;
    let shifted = adjust::map_refs(f, |sheet, a, b, _| {
        let on = sheet.as_deref().unwrap_or(own);
        if !on.eq_ignore_ascii_case(from) { return true; }
        let x = Range::new(Pos::new(a.row, a.col), Pos::new(b.row, b.col));
        if src.contains(x.start()) && src.contains(x.end()) {
            hit = true;
            for c in [a, b] { c.row = (c.row as i64 + dr) as u32; c.col = (c.col as i64 + dc) as u32; }
        }
        true
    }, |_| None);
    if !hit { return f.to_string(); }
    // Name the destination sheet on the moved references (simple form: rewrite the unqualified
    // ones that now point into the destination block).
    let dest = Range { r0: (src.r0 as i64 + dr) as u32, r1: (src.r1 as i64 + dr) as u32, c0: (src.c0 as i64 + dc) as u32, c1: (src.c1 as i64 + dc) as u32 };
    let tokens = crate::formula::lexer::tokenize(&shifted).unwrap_or_default();
    let chars: Vec<char> = shifted.chars().collect();
    let mut out = String::new(); let mut at = 0;
    for t in tokens {
        out.extend(&chars[at..t.start]); at = t.end;
        let text: String = chars[t.start..t.end].iter().collect();
        if let crate::formula::lexer::Tok::Ref { sheet, body, .. } = &t.tok {
            let on = sheet.as_deref().unwrap_or(own);
            if on.eq_ignore_ascii_case(from) && crate::addr::parse_range(body).is_some_and(|r| dest.contains(r.start()) && dest.contains(r.end())) {
                out.push_str(&format!("{}!{}", crate::addr::quote_sheet(to), body));
                continue;
            }
        }
        out.push_str(&text);
    }
    out.extend(&chars[at.min(chars.len())..]);
    out
}

#[cfg(test)]
mod tests {
    use crate::addr::{parse_a1, parse_range};
    use crate::book::Workbook;
    #[test]
    fn moving() {
        let mut wb = Workbook::new();
        wb.set_input(0, parse_a1("A1").unwrap(), "5");
        wb.set_input(0, parse_a1("B1").unwrap(), "=A1*2");
        wb.move_range(0, parse_range("A1").unwrap(), 0, parse_a1("C3").unwrap());
        wb.recalc();
        assert_eq!(wb.edit_text(0, parse_a1("B1").unwrap()), "=C3*2");
        assert_eq!(wb.display(0, parse_a1("B1").unwrap()), "10");
        wb.shift_cells(0, parse_range("B1").unwrap(), true, false);
        assert_eq!(wb.edit_text(0, parse_a1("B2").unwrap()), "=C3*2");
        assert_eq!(wb.current_region(0, parse_a1("C3").unwrap()), parse_range("B2:C3").unwrap());
    }
}

impl Workbook {
    /// Can `name` be a defined name? (Starts with a letter, _ or \, no spaces, not a cell address.)
    pub fn valid_name(&self, name: &str) -> Result<(), String> {
        let n = name.trim();
        let first_ok = n.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '\\');
        if !first_ok || n.chars().any(|c| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?'))) { return Err("A name must start with a letter or underscore and can't contain spaces.".into()); }
        if n.chars().count() > 255 { return Err("A name can have at most 255 characters.".into()); }
        if crate::addr::parse_a1(n).is_some() || crate::addr::col_index(n).is_some() && n.len() <= 3 || n.eq_ignore_ascii_case("R") || n.eq_ignore_ascii_case("C") || (n.to_ascii_uppercase().starts_with('R') && n[1..].contains('C') && n.chars().skip(1).all(|c| c.is_ascii_digit() || c == 'C' || c == 'c')) {
            return Err("A name can't look like a cell reference.".into());
        }
        Ok(())
    }
}
