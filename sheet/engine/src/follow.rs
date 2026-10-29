//! Floating objects, chart references, pivot sources and (for Insert/Delete Cells) tables and
//! rule ranges following structural edits. Each editing operation calls `follow` once, after it
//! has rewritten the formulas, as `rewrite_formulas` is called for cell formulas.

use crate::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
use crate::book::Workbook;
use crate::condformat::shift_range;
use crate::drawing::{Anchor, Mark, ObjKind};
use crate::formula::adjust;

/// A structural edit, as the operation saw it (sheet indexes are before the edit for
/// `DeleteSheet`, and the copy's index for `DuplicateSheet`).
pub enum Edit<'a> {
    /// `n` rows (or columns) inserted at `at` (n > 0) or deleted from it (n < 0).
    Insert { sheet: usize, rows: bool, at: u32, n: i64 },
    /// Insert/Delete Cells over `r`, shifting down (or right) / up (or left).
    ShiftCells { sheet: usize, r: Range, down: bool, delete: bool },
    /// Cells moved from `src` on `from` to `at` on `to`.
    Move { from: usize, src: Range, to: usize, at: Pos },
    Rename { old: &'a str, new: &'a str },
    /// The sheet named `gone` was deleted.
    DeleteSheet { gone: &'a str },
    /// Sheet `copy` was made from `orig`.
    DuplicateSheet { orig: usize, copy: usize },
}

/// Rewrites every chart's references with `f(chart's sheet, ref)`; changed charts are marked
/// dirty so writing regenerates their part.
fn rewrite_chart_refs(wb: &mut Workbook, f: impl Fn(&str, &str) -> String) {
    for s in &mut wb.sheets {
        let own = s.name.clone();
        for o in &mut s.objects {
            let ObjKind::Chart(c) = &mut o.kind else { continue };
            let mut changed = false;
            for se in &mut c.series {
                for r in [&mut se.name_ref, &mut se.cat_ref, &mut se.val_ref].into_iter().flatten() {
                    let t = f(&own, r);
                    if t != *r { *r = t; changed = true; }
                }
            }
            if let Some(r) = &mut c.title_ref { let t = f(&own, r); if t != *r { *r = t; changed = true; } }
            if changed { c.dirty = true; }
        }
    }
}

/// The source sheet name of a pivot whose source no longer exists (Refresh reports the
/// PivotTable as invalid).
pub const INVALID_SOURCE: &str = "#REF!";

/// Is the pivot's source still there?
pub fn pivot_source_valid(wb: &Workbook, p: &crate::pivot::Pivot) -> bool { p.source_sheet != INVALID_SOURCE && wb.sheet_index(&p.source_sheet).is_some() }

/// The message Refresh shows for a pivot whose source is gone.
pub const INVALID_PIVOT: &str = "The PivotTable report is invalid. The source data or the sheet it was on was deleted. Use Change Data Source to choose new data.";

/// The cells an anchor covers (a `to` mark at offset 0 ends in the cell before it); None for
/// absolute ones, which don't move with cells.
pub fn anchor_cells(a: &Anchor) -> Option<Range> {
    match a {
        Anchor::Two { edit_as: Some(e), .. } if e == "absolute" => None,
        Anchor::Two { from, to, .. } => {
            let c1 = if to.dx <= 0.0 && to.col > from.col { to.col - 1 } else { to.col.max(from.col) };
            let r1 = if to.dy <= 0.0 && to.row > from.row { to.row - 1 } else { to.row.max(from.row) };
            Some(Range { r0: from.row, c0: from.col, r1, c1 })
        }
        Anchor::One { from, .. } => Some(Range::cell(Pos::new(from.row, from.col))),
        Anchor::Abs { .. } => None,
    }
}

/// Moves an anchor by (dr, dc) cells (absolute anchors stay).
pub fn move_anchor(a: &mut Anchor, dr: i64, dc: i64) {
    let mv = |m: &mut Mark| { m.row = (m.row as i64 + dr).clamp(0, MAX_ROWS as i64 - 1) as u32; m.col = (m.col as i64 + dc).clamp(0, MAX_COLS as i64 - 1) as u32; };
    match a { Anchor::Two { from, to, .. } => { mv(from); mv(to); } Anchor::One { from, .. } => mv(from), Anchor::Abs { .. } => {} }
}

/// Qualifies a chart's unqualified references with `sheet` (before it moves to another sheet).
fn qualify_chart(o: &mut crate::drawing::Object, sheet: &str) {
    let ObjKind::Chart(c) = &mut o.kind else { return };
    for se in &mut c.series {
        for r in [&mut se.name_ref, &mut se.cat_ref, &mut se.val_ref].into_iter().flatten() {
            if let Some((None, rg)) = crate::chart::parse_ref(r) { *r = crate::chart::make_ref(sheet, rg); c.dirty = true; }
        }
    }
    if let Some(r) = &mut c.title_ref { if let Some((None, rg)) = crate::chart::parse_ref(r) { *r = crate::chart::make_ref(sheet, rg); c.dirty = true; } }
}

/// A copy of an object to place as a new one (copy and paste, duplicate): its generated XML
/// replaces the carried one so ids and parts are fresh.
pub fn copy_object(o: &crate::drawing::Object) -> crate::drawing::Object {
    let mut c = o.clone();
    c.name = String::new();
    match &mut c.kind {
        ObjKind::Chart(ch) => { ch.dirty = true; c.raw = None; }
        ObjKind::Other | ObjKind::Group(_) => {}
        _ => c.raw = None,
    }
    c
}

/// Copies the objects whose cells lie within `src` on `from` to the cells at `at` on `to`
/// (pasting copied cells brings their objects along, as Excel does for objects that move
/// with cells). Charts' references stay as they were (qualified with the source sheet).
pub fn copy_objects_with_cells(wb: &mut Workbook, from: usize, src: Range, to: usize, at: Pos) -> usize {
    let (dr, dc) = (at.row as i64 - src.r0 as i64, at.col as i64 - src.c0 as i64);
    let fname = wb.sheets[from].name.clone();
    let copies: Vec<crate::drawing::Object> = wb.sheets[from].objects.iter()
        .filter(|o| anchor_cells(&o.anchor).is_some_and(|c| src.contains(c.start()) && src.contains(c.end())))
        .map(|o| { let mut c = copy_object(o); qualify_chart(&mut c, &fname); move_anchor(&mut c.anchor, dr, dc); c }).collect();
    let n = copies.len();
    for c in copies { let base = base_name(&c); crate::drawing::add_object(&mut wb.sheets[to].objects, c, base); }
    n
}

/// The name stem new objects of this kind get ("Chart 3").
pub fn base_name(o: &crate::drawing::Object) -> &'static str {
    match o.kind { ObjKind::Chart(_) => "Chart", ObjKind::Picture { .. } => "Picture", ObjKind::Shape(ref s) if s.text_box => "TextBox", ObjKind::Shape(_) => "Shape", ObjKind::Slicer(_) => "Slicer", ObjKind::Timeline(_) => "Timeline", ObjKind::Group(_) => "Group", ObjKind::Other => "Object" }
}

/// A grid mark after rows (or columns) are inserted or deleted; a mark inside deleted lines
/// collapses to the first line after them, at offset 0.
fn shift_mark(m: Mark, rows: bool, at: u32, n: i64) -> Mark {
    let (v, max) = if rows { (m.row, MAX_ROWS) } else { (m.col, MAX_COLS) };
    let (nv, collapsed) = if v < at { (v, false) } else if n > 0 { ((v as i64 + n).min(max as i64 - 1) as u32, false) } else {
        let end = at as i64 - n;
        if (v as i64) < end { (at, true) } else { ((v as i64 + n) as u32, false) }
    };
    let mut m = m;
    if rows { m.row = nv; if collapsed { m.dy = 0.0; } } else { m.col = nv; if collapsed { m.dx = 0.0; } }
    m
}

fn shift_anchor(a: &mut Anchor, rows: bool, at: u32, n: i64) {
    match a {
        Anchor::Two { from, to, edit_as } => match edit_as.as_deref() {
            Some("absolute") => {}
            Some("oneCell") => {
                // Moves with its top-left cell, keeping its size in cells.
                let nf = shift_mark(*from, rows, at, n);
                let d = if rows { nf.row as i64 - from.row as i64 } else { nf.col as i64 - from.col as i64 };
                if rows { to.row = (to.row as i64 + d).clamp(0, MAX_ROWS as i64 - 1) as u32; } else { to.col = (to.col as i64 + d).clamp(0, MAX_COLS as i64 - 1) as u32; }
                *from = nf;
            }
            _ => { *from = shift_mark(*from, rows, at, n); *to = shift_mark(*to, rows, at, n); }
        },
        Anchor::One { from, .. } => *from = shift_mark(*from, rows, at, n),
        Anchor::Abs { .. } => {}
    }
}

fn move_range_by(r: Range, dr: i64, dc: i64) -> Range {
    Range { r0: (r.r0 as i64 + dr) as u32, r1: (r.r1 as i64 + dr) as u32, c0: (r.c0 as i64 + dc) as u32, c1: (r.c1 as i64 + dc) as u32 }
}

/// A range after Insert/Delete Cells over `r`: ranges spanning the shifted lines' cross extent
/// grow, shrink or move; others stay. None when deleted entirely.
fn shift_cells_range(x: Range, r: Range, down: bool, delete: bool) -> Option<Range> {
    let inside_cross = if down { x.c0 >= r.c0 && x.c1 <= r.c1 } else { x.r0 >= r.r0 && x.r1 <= r.r1 };
    if !inside_cross { return Some(x); }
    let (at, n) = if down { (r.r0, r.rows() as i64) } else { (r.c0, r.cols() as i64) };
    shift_range(x, down, at, if delete { -n } else { n })
}

pub fn follow(wb: &mut Workbook, e: Edit) {
    crate::sparkline::follow(wb, &e);
    match e {
        Edit::Insert { sheet, rows, at, n } => {
            let target = wb.sheets[sheet].name.clone();
            rewrite_chart_refs(wb, |own, f| adjust::insert(f, own, &target, rows, at, n));
            for o in &mut wb.sheets[sheet].objects { shift_anchor(&mut o.anchor, rows, at, n); }
            for s in &mut wb.sheets {
                let here = s.name.eq_ignore_ascii_case(&target);
                for p in &mut s.pivots {
                    if p.source_sheet.eq_ignore_ascii_case(&target) { match shift_range(p.source, rows, at, n) { Some(r) => p.source = r, None => p.source_sheet = INVALID_SOURCE.into() } }
                    if here {
                        let a = shift_range(Range::cell(p.anchor), rows, at, n).map(|r| r.start());
                        match a { Some(a) => p.anchor = a, None => p.anchor = if rows { Pos::new(at, p.anchor.col) } else { Pos::new(p.anchor.row, at) } }
                        p.output = p.output.and_then(|r| shift_range(r, rows, at, n));
                    }
                }
            }
        }
        Edit::ShiftCells { sheet, r, down, delete } => {
            let name = wb.sheets[sheet].name.clone();
            let n = if down { r.rows() as i64 } else { r.cols() as i64 };
            let block = if down { Range { r0: if delete { r.r1 + 1 } else { r.r0 }, r1: MAX_ROWS - 1, ..r } } else { Range { c0: if delete { r.c1 + 1 } else { r.c0 }, c1: MAX_COLS - 1, ..r } };
            let d = if delete { -n } else { n };
            let (dr, dc) = if down { (d, 0) } else { (0, d) };
            // The same rewriting Workbook::shift_cells applies to formulas.
            rewrite_chart_refs(wb, |own, f| adjust::shift_cells(f, own, &name, r, down, delete));
            for s in &mut wb.sheets {
                let here = s.name.eq_ignore_ascii_case(&name);
                for p in &mut s.pivots {
                    if p.source_sheet.eq_ignore_ascii_case(&name) { match shift_cells_range(p.source, r, down, delete) { Some(x) => p.source = x, None => p.source_sheet = INVALID_SOURCE.into() } }
                    if here && block.contains(p.anchor) { p.anchor = move_range_by(Range::cell(p.anchor), dr, dc).start(); p.output = None; }
                }
            }
            let s = &mut wb.sheets[sheet];
            // Objects over the shifted cells (their cells within the shifted columns, or rows)
            // move with them; twoCell ones the insertion point crosses grow, deleted cells
            // shrink them; absolute ones stay.
            let at = if down { r.r0 } else { r.c0 };
            for o in &mut s.objects {
                let Some(c) = anchor_cells(&o.anchor) else { continue };
                if if down { c.c0 >= r.c0 && c.c1 <= r.c1 } else { c.r0 >= r.r0 && c.r1 <= r.r1 } { shift_anchor(&mut o.anchor, down, at, d); }
            }
            for cf in &mut s.cond_formats { cf.ranges = cf.ranges.iter().filter_map(|x| shift_cells_range(*x, r, down, delete)).collect(); }
            s.cond_formats.retain(|c| !c.ranges.is_empty());
            for v in &mut s.validations { v.ranges = v.ranges.iter().filter_map(|x| shift_cells_range(*x, r, down, delete)).collect(); }
            s.validations.retain(|v| !v.ranges.is_empty());
            s.cf_cache.clear();
            // Tables: moved whole when they sit in the shifted block; rows (or columns) grow or
            // shrink when the shift crosses them over their whole width (or height). Shifts that
            // would split a table are refused beforehand (Workbook::shift_cells_blocked).
            s.tables.retain_mut(|t| {
                let x = t.range;
                if block.contains(x.start()) && block.contains(x.end()) {
                    let m = move_range_by(x, dr, dc);
                    if m.r1 >= MAX_ROWS || m.c1 >= MAX_COLS { return false; }
                    t.range = m; return true;
                }
                if down && x.c0 >= r.c0 && x.c1 <= r.c1 && x.intersects(&Range { r0: r.r0, r1: MAX_ROWS - 1, ..r }) {
                    if delete && t.header_row && r.r0 <= x.r0 && x.r0 <= r.r1 { return false; }
                    match shift_range(x, true, r.r0, d) { Some(y) if y.rows() >= 1 + t.header_row as u32 + t.totals_row as u32 => { t.range = y; true } _ => false }
                } else if !down && x.r0 >= r.r0 && x.r1 <= r.r1 && x.intersects(&Range { c0: r.c0, c1: MAX_COLS - 1, ..r }) {
                    crate::tables::shift_table_cols(t, r.c0, d)
                } else { true }
            });
        }
        Edit::Move { from, src, to, at } => {
            let (dr, dc) = (at.row as i64 - src.r0 as i64, at.col as i64 - src.c0 as i64);
            let (fname, tname) = (wb.sheets[from].name.clone(), wb.sheets[to].name.clone());
            rewrite_chart_refs(wb, |own, f| if from == to { adjust::move_area(f, own, &fname, src, dr, dc) } else { crate::ops::moved_to_sheet(f, own, &fname, &tname, src, dr, dc) });
            for s in &mut wb.sheets {
                for p in &mut s.pivots {
                    if p.source_sheet.eq_ignore_ascii_case(&fname) && src.contains(p.source.start()) && src.contains(p.source.end()) {
                        p.source = move_range_by(p.source, dr, dc); p.source_sheet = tname.clone();
                    }
                }
            }
            // Objects whose cells lie within the moved range go with them (to the destination
            // sheet when moved across sheets, their charts' references qualified first).
            let inside: Vec<usize> = (0..wb.sheets[from].objects.len()).filter(|&i| anchor_cells(&wb.sheets[from].objects[i].anchor).is_some_and(|c| src.contains(c.start()) && src.contains(c.end()))).collect();
            if from == to {
                for i in inside { move_anchor(&mut wb.sheets[from].objects[i].anchor, dr, dc); }
            } else {
                for i in inside.into_iter().rev() {
                    let mut o = wb.sheets[from].objects.remove(i);
                    qualify_chart(&mut o, &fname);
                    move_anchor(&mut o.anchor, dr, dc);
                    o.id = 0;
                    let base = base_name(&o);
                    crate::drawing::add_object(&mut wb.sheets[to].objects, o, base);
                }
            }
        }
        Edit::Rename { old, new } => {
            rewrite_chart_refs(wb, |_, f| adjust::rename_sheet(f, old, new));
            for s in &mut wb.sheets { for p in &mut s.pivots { if p.source_sheet.eq_ignore_ascii_case(old) { p.source_sheet = new.to_string(); } } }
        }
        Edit::DeleteSheet { gone } => {
            rewrite_chart_refs(wb, |_, f| adjust::drop_sheet(f, gone));
            for s in &mut wb.sheets { for p in &mut s.pivots { if p.source_sheet.eq_ignore_ascii_case(gone) { p.source_sheet = INVALID_SOURCE.into(); } } }
            // Slicers of pivots (or tables) that went with the sheet are removed.
            let pivots: Vec<String> = wb.sheets.iter().flat_map(|s| s.pivots.iter().map(|p| p.name.to_lowercase())).collect();
            let tables: Vec<String> = wb.sheets.iter().flat_map(|s| s.tables.iter().flat_map(|t| [t.name.to_lowercase(), t.display_name.to_lowercase()])).collect();
            for s in &mut wb.sheets {
                s.objects.retain(|o| match &o.kind {
                    ObjKind::Slicer(sl) if !sl.pivot.is_empty() => pivots.contains(&sl.pivot.to_lowercase()),
                    ObjKind::Slicer(sl) if !sl.table.is_empty() => tables.contains(&sl.table.to_lowercase()),
                    _ => true,
                });
            }
        }
        Edit::DuplicateSheet { orig, copy } => {
            // Copied charts point where the originals did; unqualified references named the
            // original sheet, so they are qualified with it.
            let oname = wb.sheets[orig].name.clone();
            for o in &mut wb.sheets[copy].objects {
                let ObjKind::Chart(c) = &mut o.kind else { continue };
                for se in &mut c.series {
                    for r in [&mut se.name_ref, &mut se.cat_ref, &mut se.val_ref].into_iter().flatten() {
                        if let Some((None, rg)) = crate::chart::parse_ref(r) { *r = crate::chart::make_ref(&oname, rg); c.dirty = true; }
                    }
                }
            }
            // Copied pivots get unique names; the copy's slicers follow them.
            let mut taken: Vec<String> = wb.sheets.iter().enumerate().filter(|(i, _)| *i != copy).flat_map(|(_, s)| s.pivots.iter().map(|p| p.name.to_lowercase())).collect();
            let mut renames = vec![];
            for p in &mut wb.sheets[copy].pivots {
                let base = p.name.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
                let nn = (2..).map(|k| format!("{base}{k}")).find(|n| !taken.contains(&n.to_lowercase())).unwrap();
                taken.push(nn.to_lowercase());
                renames.push((std::mem::replace(&mut p.name, nn.clone()), nn));
            }
            for o in &mut wb.sheets[copy].objects {
                if let ObjKind::Slicer(sl) = &mut o.kind { if let Some((_, nn)) = renames.iter().find(|(old, _)| old.eq_ignore_ascii_case(&sl.pivot)) { sl.pivot = nn.clone(); o.raw = None; } }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::addr::{parse_a1, parse_range};
    use crate::book::Workbook;
    use crate::chart::{Chart, Series};
    use crate::drawing::{add_object, Anchor, Mark, Object, ObjKind};

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        let data = [("A1", "Name"), ("B1", "Val"), ("A2", "x"), ("B2", "1"), ("A3", "y"), ("B3", "2"), ("A4", "z"), ("B4", "3")];
        for (a, t) in data { wb.set_input(0, parse_a1(a).unwrap(), t); }
        wb.recalc();
        let chart = Chart { series: vec![Series { name_ref: Some("Sheet1!$B$1".into()), cat_ref: Some("Sheet1!$A$2:$A$4".into()), val_ref: Some("Sheet1!$B$2:$B$4".into()), ..Default::default() }], ..Default::default() };
        let anchor = Anchor::Two { from: Mark { col: 3, row: 5, ..Default::default() }, to: Mark { col: 8, row: 15, ..Default::default() }, edit_as: None };
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Chart(Box::new(chart)), anchor), "Chart");
        wb
    }
    fn chart(wb: &Workbook) -> &Chart { wb.sheets[0].objects[0].chart().unwrap() }

    #[test]
    fn insert_rows_above_chart() {
        let mut wb = book();
        wb.insert(0, true, 0, 2);
        let c = chart(&wb);
        assert_eq!(c.series[0].val_ref.as_deref(), Some("Sheet1!$B$4:$B$6"));
        assert_eq!(c.series[0].name_ref.as_deref(), Some("Sheet1!$B$3"));
        assert!(c.dirty);
        let Anchor::Two { from, to, .. } = &wb.sheets[0].objects[0].anchor else { panic!() };
        assert_eq!((from.row, to.row), (7, 17));
        // Deleting rows the chart sits in collapses it (move and size with cells).
        wb.insert(0, true, 10, -20);
        let Anchor::Two { from, to, .. } = &wb.sheets[0].objects[0].anchor else { panic!() };
        assert_eq!((from.row, to.row), (7, 10));
    }

    #[test]
    fn edit_as_modes() {
        let mut wb = book();
        for e in ["oneCell", "absolute"] {
            let o = Object::new(ObjKind::Other, Anchor::Two { from: Mark { row: 5, ..Default::default() }, to: Mark { row: 8, ..Default::default() }, edit_as: Some(e.into()) });
            wb.sheets[0].objects.push(o);
        }
        wb.insert(0, true, 6, 3);
        let rows: Vec<(u32, u32)> = wb.sheets[0].objects[1..].iter().map(|o| match &o.anchor { Anchor::Two { from, to, .. } => (from.row, to.row), _ => (0, 0) }).collect();
        assert_eq!(rows, vec![(5, 8), (5, 8)]);
        wb.insert(0, true, 0, 1);
        let rows: Vec<(u32, u32)> = wb.sheets[0].objects[1..].iter().map(|o| match &o.anchor { Anchor::Two { from, to, .. } => (from.row, to.row), _ => (0, 0) }).collect();
        assert_eq!(rows, vec![(6, 9), (5, 8)]);
    }

    #[test]
    fn delete_data_rows_gives_ref() {
        let mut wb = book();
        wb.insert(0, true, 0, -4);
        let c = chart(&wb);
        assert_eq!(c.series[0].val_ref.as_deref(), Some("#REF!"));
        assert_eq!(c.series[0].cat_ref.as_deref(), Some("#REF!"));
    }

    #[test]
    fn rename_and_delete_sheet() {
        let mut wb = book();
        wb.rename_sheet(0, "Data Set").unwrap();
        assert_eq!(chart(&wb).series[0].val_ref.as_deref(), Some("'Data Set'!$B$2:$B$4"));
        let i = wb.add_sheet(1, None);
        wb.sheets[i].objects = wb.sheets[0].objects.clone();
        wb.delete_sheet(0).unwrap();
        assert_eq!(wb.sheets[0].objects[0].chart().unwrap().series[0].val_ref.as_deref(), Some("#REF!"));
    }

    #[test]
    fn pivot_source_follows() {
        let mut wb = book();
        let p = crate::pivot::Pivot::new("PivotTable1", "Sheet1", parse_range("A1:B4").unwrap(), parse_a1("E1").unwrap());
        wb.sheets[0].pivots.push(p);
        wb.insert(0, true, 2, 2);
        assert_eq!(wb.sheets[0].pivots[0].source, parse_range("A1:B6").unwrap());
        wb.insert(0, false, 0, 1);
        assert_eq!(wb.sheets[0].pivots[0].source, parse_range("B1:C6").unwrap());
        assert_eq!(wb.sheets[0].pivots[0].anchor, parse_a1("F1").unwrap());
        wb.rename_sheet(0, "Src").unwrap();
        assert_eq!(wb.sheets[0].pivots[0].source_sheet, "Src");
        let k = wb.duplicate_sheet(0);
        assert_eq!(wb.sheets[k].pivots[0].name, "PivotTable2");
    }

    #[test]
    fn shift_cells_and_move() {
        let mut wb = book();
        wb.set_input(0, parse_a1("C1").unwrap(), "1");
        wb.sheets[0].validations.push(crate::validation::Validation { ranges: vec![parse_range("B2:B4").unwrap()], ..Default::default() });
        wb.shift_cells(0, parse_range("A2:B2").unwrap(), true, false);
        assert_eq!(chart(&wb).series[0].val_ref.as_deref(), Some("Sheet1!$B$3:$B$5"));
        assert_eq!(wb.sheets[0].validations[0].ranges, vec![parse_range("B3:B5").unwrap()]);
        wb.move_range(0, parse_range("A1:B5").unwrap(), 0, parse_a1("A11").unwrap());
        assert_eq!(chart(&wb).series[0].val_ref.as_deref(), Some("Sheet1!$B$13:$B$15"));
    }

    fn two(wb: &Workbook, i: usize) -> (u32, u32, u32, u32) {
        let Anchor::Two { from, to, .. } = &wb.sheets[0].objects[i].anchor else { panic!() };
        (from.row, from.col, to.row, to.col)
    }

    #[test]
    fn shift_cells_moves_objects() {
        let mut wb = book(); // chart over D6:H15 (rows 5..15, cols 3..8)
        // Insert cells over D2:H3 shifting down: the chart moves down 2 rows.
        wb.shift_cells(0, parse_range("D2:H3").unwrap(), true, false);
        assert_eq!(two(&wb, 0), (7, 3, 17, 8));
        // Shifted cells narrower than the chart: it stays.
        wb.shift_cells(0, parse_range("D2:E3").unwrap(), true, false);
        assert_eq!(two(&wb, 0), (7, 3, 17, 8));
        // Insertion point inside the chart: a twoCell anchor grows.
        wb.shift_cells(0, parse_range("A10:J10").unwrap(), true, false);
        assert_eq!(two(&wb, 0), (7, 3, 18, 8));
        // Delete cells sideways over rows covering it, left of it: it moves left.
        wb.shift_cells(0, parse_range("A1:B30").unwrap(), false, true);
        assert_eq!(two(&wb, 0), (7, 1, 18, 6));
        // Absolute objects stay.
        wb.sheets[0].objects.push(Object::new(ObjKind::Other, Anchor::Two { from: Mark { row: 5, ..Default::default() }, to: Mark { row: 8, col: 2, ..Default::default() }, edit_as: Some("absolute".into()) }));
        wb.shift_cells(0, parse_range("A1:C1").unwrap(), true, false);
        assert_eq!(two(&wb, 1), (5, 0, 8, 2));
    }

    #[test]
    fn insert_cells_grows_crossing_refs() {
        let mut wb = book();
        wb.set_input(0, parse_a1("D1").unwrap(), "=SUM(B2:B4)");
        // Insert cells A3:B3 down: B2:B4 crosses the insertion point and grows.
        wb.shift_cells(0, parse_range("A3:B3").unwrap(), true, false);
        assert_eq!(chart(&wb).series[0].val_ref.as_deref(), Some("Sheet1!$B$2:$B$5"));
        assert_eq!(chart(&wb).series[0].cat_ref.as_deref(), Some("Sheet1!$A$2:$A$5"));
        assert_eq!(wb.edit_text(0, parse_a1("D1").unwrap()), "=SUM(B2:B5)");
        wb.shift_cells(0, parse_range("A3:B4").unwrap(), true, true);
        assert_eq!(chart(&wb).series[0].val_ref.as_deref(), Some("Sheet1!$B$2:$B$3"));
        assert_eq!(wb.edit_text(0, parse_a1("D1").unwrap()), "=SUM(B2:B3)");
    }

    #[test]
    fn move_carries_objects() {
        let mut wb = book();
        let pic = Object::new(ObjKind::Picture { data: std::sync::Arc::new(vec![1, 2, 3]), ext: "png".into() }, Anchor::One { from: Mark { row: 1, col: 1, ..Default::default() }, w: 20.0, h: 20.0 });
        add_object(&mut wb.sheets[0].objects, pic, "Picture");
        // Moving A1:C3 takes the picture (B2) but not the chart.
        wb.move_range(0, parse_range("A1:C3").unwrap(), 0, parse_a1("E21").unwrap());
        let Anchor::One { from, .. } = &wb.sheets[0].objects[1].anchor else { panic!() };
        assert_eq!((from.row, from.col), (21, 5));
        assert_eq!(two(&wb, 0), (5, 3, 15, 8));
        // Across sheets: the chart (D6:H15, its data A1:B4) moves with its cells.
        let k = wb.add_sheet(1, None);
        wb.move_range(0, parse_range("A1:J20").unwrap(), k, parse_a1("A1").unwrap());
        assert_eq!(wb.sheets[0].objects.len(), 1);
        let c = wb.sheets[k].objects[0].chart().unwrap();
        let other = wb.sheets[k].name.clone();
        assert_eq!(c.series[0].val_ref.as_deref(), Some(format!("{other}!$B$2:$B$4").as_str()));
    }

    #[test]
    fn copy_objects_with_cells_and_copy() {
        let mut wb = book();
        let n = super::copy_objects_with_cells(&mut wb, 0, parse_range("A1:J20").unwrap(), 0, parse_a1("A31").unwrap());
        assert_eq!(n, 1);
        assert_eq!(wb.sheets[0].objects.len(), 2);
        assert_eq!(two(&wb, 1), (35, 3, 45, 8));
        assert_ne!(wb.sheets[0].objects[1].id, wb.sheets[0].objects[0].id);
        assert_eq!(wb.sheets[0].objects[1].chart().unwrap().series[0].val_ref.as_deref(), Some("Sheet1!$B$2:$B$4"));
        // Both charts are written as their own parts.
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        assert_eq!(back.sheets[0].objects.iter().filter(|o| o.is_chart()).count(), 2);
    }

    #[test]
    fn sideways_shift_through_table() {
        let mut wb = book();
        wb.create_table(0, parse_range("A1:B4").unwrap(), true, "").unwrap();
        // Insert cells B1:B4 to the right: the table gains a column.
        assert!(wb.shift_cells_blocked(0, parse_range("B1:B4").unwrap(), false).is_none());
        wb.shift_cells(0, parse_range("B1:B4").unwrap(), false, false);
        let t = &wb.sheets[0].tables[0];
        assert_eq!((t.range, t.columns.len()), (parse_range("A1:C4").unwrap(), 3));
        wb.shift_cells(0, parse_range("B1:B4").unwrap(), false, true);
        let t = &wb.sheets[0].tables[0];
        assert_eq!((t.range, t.columns.len()), (parse_range("A1:B4").unwrap(), 2));
        // A shift over only some of its rows would split it.
        assert_eq!(wb.shift_cells_blocked(0, parse_range("A2:A3").unwrap(), false), Some(crate::tables::SHIFT_SPLITS_TABLE));
        assert!(wb.shift_cells_blocked(0, parse_range("A2:A3").unwrap(), true).is_some());
        assert!(wb.shift_cells_blocked(0, parse_range("A5:B5").unwrap(), true).is_none());
    }

    #[test]
    fn pivot_source_deleted_and_slicers() {
        let mut wb = book();
        let k = wb.add_sheet(1, None);
        let p = crate::pivot::Pivot::new("PivotTable1", "Sheet1", parse_range("A1:B4").unwrap(), parse_a1("A1").unwrap());
        wb.sheets[k].pivots.push(p);
        assert!(super::pivot_source_valid(&wb, &wb.sheets[k].pivots[0]));
        wb.insert(0, true, 0, -10);
        assert!(!super::pivot_source_valid(&wb, &wb.sheets[k].pivots[0]));
        // Deleting the sheet holding a pivot removes its slicers elsewhere.
        let sl = crate::drawing::Slicer { pivot: "PivotTable1".into(), caption: "Name".into(), ..Default::default() };
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Slicer(sl), Anchor::default()), "Slicer");
        assert_eq!(wb.sheets[0].objects.len(), 2);
        wb.delete_sheet(k).unwrap();
        assert_eq!(wb.sheets[0].objects.len(), 1);
    }

    #[test]
    fn linked_chart_title_follows() {
        let mut wb = book();
        wb.sheets[0].objects[0].chart_mut().unwrap().title_ref = Some("Sheet1!$B$1".into());
        wb.insert(0, true, 0, 1);
        assert_eq!(chart(&wb).title_ref.as_deref(), Some("Sheet1!$B$2"));
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let c = back.sheets[0].objects[0].chart().unwrap();
        assert_eq!((c.title_ref.as_deref(), c.title.as_deref()), (Some("Sheet1!$B$2"), Some("Val")));
    }

    #[test]
    fn round_trip_after_edit() {
        let wb = book();
        let bytes = crate::xlsx::write(&wb).unwrap();
        let mut back = crate::xlsx::read(&bytes).unwrap();
        assert!(!back.sheets[0].objects[0].chart().unwrap().dirty);
        back.insert(0, true, 0, 1);
        let bytes = crate::xlsx::write(&back).unwrap();
        let again = crate::xlsx::read(&bytes).unwrap();
        let c = again.sheets[0].objects[0].chart().unwrap();
        assert_eq!(c.series[0].val_ref.as_deref(), Some("Sheet1!$B$3:$B$5"));
        let Anchor::Two { from, .. } = &again.sheets[0].objects[0].anchor else { panic!() };
        assert_eq!(from.row, 6);
    }
}
