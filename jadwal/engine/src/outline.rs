//! Outlines (xlsx-outline.js, outline-detail.js, subtotal.js): row and column groups with their
//! levels (1-7), collapsed groups hidden through `Sheet.hidden_rows`/`hidden_cols`, the summary
//! position (below/right by default), Show/Hide Detail, and Data ▸ Subtotal.
//!
//! In the file a row or column carries `outlineLevel` and, on the summary line of a collapsed
//! group, `collapsed="1"`; `<sheetPr><outlinePr summaryBelow summaryRight/>` gives the summary side.

use crate::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
use crate::book::{Input, Sheet, Workbook};
use crate::value::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_LEVEL: u8 = 7;

#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    /// Outline level by row and by column (absent = 0).
    pub rows: BTreeMap<u32, u8>,
    pub cols: BTreeMap<u32, u8>,
    /// Summary lines whose group is collapsed.
    pub collapsed_rows: BTreeSet<u32>,
    pub collapsed_cols: BTreeSet<u32>,
    pub summary_below: bool,
    pub summary_right: bool,
}

impl Default for Outline {
    fn default() -> Self { Outline { rows: BTreeMap::new(), cols: BTreeMap::new(), collapsed_rows: BTreeSet::new(), collapsed_cols: BTreeSet::new(), summary_below: true, summary_right: true } }
}

impl Outline {
    pub fn levels(&self, rows: bool) -> &BTreeMap<u32, u8> { if rows { &self.rows } else { &self.cols } }
    fn levels_mut(&mut self, rows: bool) -> &mut BTreeMap<u32, u8> { if rows { &mut self.rows } else { &mut self.cols } }
    pub fn collapsed(&self, rows: bool) -> &BTreeSet<u32> { if rows { &self.collapsed_rows } else { &self.collapsed_cols } }
    fn collapsed_mut(&mut self, rows: bool) -> &mut BTreeSet<u32> { if rows { &mut self.collapsed_rows } else { &mut self.collapsed_cols } }
    pub fn level(&self, rows: bool, i: u32) -> u8 { self.levels(rows).get(&i).copied().unwrap_or(0) }
    /// The deepest level in use (0: no outline in that direction).
    pub fn max_level(&self, rows: bool) -> u8 { self.levels(rows).values().copied().max().unwrap_or(0) }
    pub fn is_empty(&self) -> bool { self.rows.is_empty() && self.cols.is_empty() }
    pub fn below(&self, rows: bool) -> bool { if rows { self.summary_below } else { self.summary_right } }

    /// The groups at `level`: maximal runs of lines whose level is at least `level`.
    pub fn groups(&self, rows: bool, level: u8) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = vec![];
        for (&i, &l) in self.levels(rows) {
            if l < level { continue; }
            match out.last_mut() { Some(g) if g.1 + 1 == i => g.1 = i, _ => out.push((i, i)) }
        }
        out
    }
    /// The summary line of a group (the line after it, or before it when summaries are above).
    pub fn summary_of(&self, rows: bool, g: (u32, u32)) -> Option<u32> {
        if self.below(rows) { let max = if rows { MAX_ROWS } else { MAX_COLS }; (g.1 + 1 < max).then_some(g.1 + 1) } else { g.0.checked_sub(1) }
    }
    /// The innermost group at `level` containing line `i`.
    pub fn group_at(&self, rows: bool, level: u8, i: u32) -> Option<(u32, u32)> { self.groups(rows, level).into_iter().find(|g| g.0 <= i && i <= g.1) }

    /// Moves levels with inserted or deleted lines.
    pub fn on_insert(&mut self, rows: bool, at: u32, n: i64) {
        let max = if rows { MAX_ROWS } else { MAX_COLS } as i64;
        let map = |v: u32| -> Option<u32> {
            if v < at { return Some(v); }
            if n > 0 { let x = v as i64 + n; return (x < max).then_some(x as u32); }
            if (v as i64) < at as i64 - n { None } else { Some((v as i64 + n) as u32) }
        };
        let l = std::mem::take(self.levels_mut(rows));
        *self.levels_mut(rows) = l.into_iter().filter_map(|(k, v)| map(k).map(|k| (k, v))).collect();
        let c = std::mem::take(self.collapsed_mut(rows));
        *self.collapsed_mut(rows) = c.into_iter().filter_map(map).collect();
    }
}

fn hidden(s: &mut Sheet, rows: bool) -> &mut BTreeSet<u32> { if rows { &mut s.hidden_rows } else { &mut s.hidden_cols } }

/// Data ▸ Group: one level deeper for lines a..=b (up to seven levels).
pub fn group(s: &mut Sheet, rows: bool, a: u32, b: u32) -> Result<(), String> {
    if (a..=b).any(|i| s.outline.level(rows, i) >= MAX_LEVEL) { return Err("Cannot group: the outline already has eight levels.".into()); }
    for i in a..=b { *s.outline.levels_mut(rows).entry(i).or_insert(0) += 1; }
    Ok(())
}

/// Data ▸ Ungroup: one level less for lines a..=b; lines leaving a collapsed group show again.
pub fn ungroup(s: &mut Sheet, rows: bool, a: u32, b: u32) -> bool {
    let mut any = false;
    for i in a..=b {
        let m = s.outline.levels_mut(rows);
        if let Some(l) = m.get_mut(&i) { *l -= 1; any = true; if *l == 0 { m.remove(&i); hidden(s, rows).remove(&i); } }
    }
    if any {
        // A collapse mark whose group is gone goes too.
        let o = s.outline.clone();
        let keep: BTreeSet<u32> = o.collapsed(rows).iter().copied().filter(|&m| (1..=o.max_level(rows)).any(|l| o.groups(rows, l).iter().any(|g| o.summary_of(rows, *g) == Some(m)))).collect();
        *s.outline.collapsed_mut(rows) = keep;
    }
    any
}

/// Clear Outline: every level in the direction removed, collapsed lines shown.
pub fn clear(s: &mut Sheet, rows: bool) {
    let lines: Vec<u32> = s.outline.levels(rows).keys().copied().collect();
    for i in lines { hidden(s, rows).remove(&i); }
    s.outline.levels_mut(rows).clear();
    s.outline.collapsed_mut(rows).clear();
}

/// Collapses (Hide Detail) or expands (Show Detail) a group; expanding keeps the nested groups that
/// are collapsed hidden.
pub fn set_collapsed(s: &mut Sheet, rows: bool, level: u8, g: (u32, u32), collapse: bool) {
    let summary = s.outline.summary_of(rows, g);
    if collapse {
        for i in g.0..=g.1 { hidden(s, rows).insert(i); }
        if let Some(m) = summary { s.outline.collapsed_mut(rows).insert(m); }
    } else {
        if let Some(m) = summary { s.outline.collapsed_mut(rows).remove(&m); }
        for i in g.0..=g.1 { hidden(s, rows).remove(&i); }
        // Nested collapsed groups stay closed.
        let o = s.outline.clone();
        for l in level + 1..=o.max_level(rows) {
            for sub in o.groups(rows, l) {
                if sub.0 < g.0 || sub.1 > g.1 { continue; }
                if o.summary_of(rows, sub).is_some_and(|m| o.collapsed(rows).contains(&m)) { for i in sub.0..=sub.1 { hidden(s, rows).insert(i); } }
            }
        }
    }
}

/// The level buttons (1, 2, 3…): show levels up to `n`, hide the deeper detail.
pub fn show_level(s: &mut Sheet, rows: bool, n: u8) {
    let o = s.outline.clone();
    let mut col = BTreeSet::new();
    for (&i, &l) in o.levels(rows) { if l >= n { hidden(s, rows).insert(i); } else { hidden(s, rows).remove(&i); } }
    for l in n..=o.max_level(rows) { for g in o.groups(rows, l) { if let Some(m) = o.summary_of(rows, g) { col.insert(m); } } }
    *s.outline.collapsed_mut(rows) = col;
}

/// Show/Hide Detail for the selection: the innermost group under or summarized by line `i`.
pub fn detail(s: &mut Sheet, rows: bool, i: u32, show: bool) -> bool {
    let o = s.outline.clone();
    for l in (1..=o.max_level(rows)).rev() {
        for g in o.groups(rows, l) {
            if (g.0 <= i && i <= g.1) || o.summary_of(rows, g) == Some(i) {
                let shut = o.summary_of(rows, g).is_some_and(|m| o.collapsed(rows).contains(&m));
                if show && !shut { continue; }
                if !show && shut { continue; }
                set_collapsed(s, rows, l, g, !show);
                return true;
            }
        }
    }
    false
}

// ---- the file --------------------------------------------------------------------------------

/// Reads `<outlinePr>` from the sheet's `<sheetPr>`.
pub fn read_sheet_pr(e: &crate::xlsx::xml::Element, o: &mut Outline) {
    if let Some(p) = e.child("outlinePr") { o.summary_below = p.flag("summaryBelow", true); o.summary_right = p.flag("summaryRight", true); }
}

/// Puts `<outlinePr>` into `<sheetPr>` (only when a summary side differs from the default).
pub fn write_sheet_pr(o: &Outline, pr: &mut crate::xlsx::xml::Element) {
    use crate::xlsx::xml::{Element, Node};
    pr.children.retain(|c| !matches!(c, Node::Element(e) if e.local() == "outlinePr"));
    if o.summary_below && o.summary_right { return; }
    let mut attrs = vec![];
    if !o.summary_below { attrs.push(("summaryBelow".to_string(), "0".to_string())); }
    if !o.summary_right { attrs.push(("summaryRight".to_string(), "0".to_string())); }
    // tabColor comes first, then outlinePr, then pageSetUpPr.
    let at = pr.children.iter().position(|c| matches!(c, Node::Element(e) if e.local() == "pageSetUpPr")).unwrap_or(pr.children.len());
    pr.children.insert(at, Node::Element(Element { name: "outlinePr".into(), attrs, children: vec![] }));
}

/// The attributes a `<row>` or `<col>` gets: ` outlineLevel="n" collapsed="1"`.
pub fn line_attrs(o: &Outline, rows: bool, i: u32) -> String {
    let mut s = String::new();
    let l = o.level(rows, i);
    if l > 0 { s.push_str(&format!(" outlineLevel=\"{l}\"")); }
    if o.collapsed(rows).contains(&i) { s.push_str(" collapsed=\"1\""); }
    s
}

/// `outlineLevelRow`/`outlineLevelCol` for `<sheetFormatPr>`.
pub fn format_attrs(o: &Outline) -> String {
    let mut s = String::new();
    if o.max_level(true) > 0 { s.push_str(&format!(" outlineLevelRow=\"{}\"", o.max_level(true))); }
    if o.max_level(false) > 0 { s.push_str(&format!(" outlineLevelCol=\"{}\"", o.max_level(false))); }
    s
}

/// Records a level and collapse mark read from a `<row>` or `<col>`.
pub fn read_line(o: &mut Outline, rows: bool, i: u32, level: Option<u32>, collapsed: bool) {
    if let Some(l) = level.filter(|l| *l > 0) { o.levels_mut(rows).insert(i, l.min(MAX_LEVEL as u32) as u8); }
    if collapsed { o.collapsed_mut(rows).insert(i); }
}

// ---- Subtotal ----------------------------------------------------------------------------------

/// The SUBTOTAL function numbers in the dialog's order with their labels.
pub const FUNCTIONS: &[(u32, &str, &str)] = &[(9, "Sum", "Total"), (3, "Count", "Count"), (1, "Average", "Average"), (4, "Max", "Max"), (5, "Min", "Min"), (6, "Product", "Product"), (2, "Count Numbers", "Count"), (7, "StdDev", "StdDev"), (8, "StdDevp", "StdDevp"), (10, "Var", "Var"), (11, "Varp", "Varp")];

#[derive(Clone, Debug, PartialEq)]
pub struct SubtotalSpec {
    /// "At each change in": the column (absolute) whose value starts a new group.
    pub group_col: u32,
    /// SUBTOTAL function number (9 = SUM).
    pub func: u32,
    /// "Add subtotal to": absolute columns.
    pub cols: Vec<u32>,
    pub replace: bool,
    pub page_breaks: bool,
    pub below: bool,
}

fn has_subtotal(wb: &Workbook, sheet: usize, row: u32, r: Range) -> bool {
    (r.c0..=r.c1).any(|c| wb.sheets[sheet].cells.get(&Pos::new(row, c)).and_then(|x| x.formula()).is_some_and(|f| f.to_ascii_uppercase().contains("SUBTOTAL(")))
}

/// Remove All: deletes the subtotal rows of a list (header in its first row) and its row outline.
/// Returns the list's range afterwards.
pub fn remove_subtotals(wb: &mut Workbook, sheet: usize, r: Range) -> Range {
    let mut r1 = r.r1;
    let mut row = r.r1;
    while row > r.r0 {
        if has_subtotal(wb, sheet, row, r) { wb.insert(sheet, true, row, -1); r1 -= 1; }
        row -= 1;
    }
    let s = &mut wb.sheets[sheet];
    for i in r.r0..=r1.max(r.r0) { s.outline.rows.remove(&i); s.outline.collapsed_rows.remove(&i); s.hidden_rows.remove(&i); s.page.row_breaks.remove(&i); }
    Range { r1, ..r }
}

/// Data ▸ Subtotal over a list whose first row holds the headers: a summary row after (or before)
/// each run of equal values in the group column, a grand total, and a three-level row outline.
/// Returns the list's range afterwards.
pub fn subtotal(wb: &mut Workbook, sheet: usize, r: Range, spec: &SubtotalSpec) -> Result<Range, String> {
    if r.rows() < 2 { return Err("Select a list with a header row and at least one row of data.".into()); }
    if spec.cols.is_empty() { return Err("Choose at least one column to add a subtotal to.".into()); }
    let mut r = if spec.replace { remove_subtotals(wb, sheet, r) } else { r };
    let label = FUNCTIONS.iter().find(|f| f.0 == spec.func).map_or("Total", |f| f.2);
    let key = |wb: &Workbook, row: u32| wb.sheets[sheet].value(Pos::new(row, spec.group_col)).display();
    // The runs, found before anything moves.
    let mut runs: Vec<(u32, u32, String)> = vec![];
    for row in r.r0 + 1..=r.r1 {
        let k = key(wb, row);
        match runs.last_mut() { Some(g) if g.2.eq_ignore_ascii_case(&k) && g.1 + 1 == row => g.1 = row, _ => runs.push((row, row, k)) }
    }
    let sum_formula = |a: u32, b: u32, c: u32| format!("=SUBTOTAL({},{}{}:{}{})", spec.func, crate::addr::col_name(c), a + 1, crate::addr::col_name(c), b + 1);
    let mut shift = 0u32;
    let mut detail: Vec<(u32, u32)> = vec![];
    let mut summaries: Vec<u32> = vec![];
    for (a, b, k) in runs {
        let (a, b) = (a + shift, b + shift);
        let at = if spec.below { b + 1 } else { a };
        wb.insert(sheet, true, at, 1);
        shift += 1;
        let (da, db) = if spec.below { (a, b) } else { (a + 1, b + 1) };
        wb.set_cell_input(sheet, Pos::new(at, spec.group_col), Input::Value(Value::str(format!("{k} {label}"))), None);
        for &c in &spec.cols { wb.set_input(sheet, Pos::new(at, c), &sum_formula(da, db, c)); }
        let bold = |st: &mut crate::style::Style| st.bold = true;
        wb.apply_style(sheet, &[Range { r0: at, r1: at, c0: r.c0, c1: r.c1 }], &bold);
        if spec.page_breaks && spec.below { wb.sheets[sheet].page.row_breaks.insert(at + 1); }
        detail.push((da, db));
        summaries.push(at);
    }
    r.r1 += shift;
    // The grand total.
    let (first, last) = (r.r0 + 1, r.r1);
    let at = if spec.below { r.r1 + 1 } else { r.r0 + 1 };
    wb.insert(sheet, true, at, 1);
    let (first, last) = if spec.below { (first, last) } else { (first + 1, last + 1) };
    let grand = if label == "Total" { "Grand Total".to_string() } else { format!("Grand {label}") };
    wb.set_cell_input(sheet, Pos::new(at, spec.group_col), Input::Value(Value::str(grand)), None);
    for &c in &spec.cols { wb.set_input(sheet, Pos::new(at, c), &sum_formula(first, last, c)); }
    let bold = |st: &mut crate::style::Style| st.bold = true;
    wb.apply_style(sheet, &[Range { r0: at, r1: at, c0: r.c0, c1: r.c1 }], &bold);
    r.r1 += 1;
    // The outline: everything but the grand total at level 1, the detail at level 2.
    let s = &mut wb.sheets[sheet];
    s.outline.summary_below = spec.below;
    let shiftd = if spec.below { 0 } else { 1 };
    for i in first..=last { let e = s.outline.rows.entry(i).or_insert(0); *e = (*e).max(1); }
    for (a, b) in detail { for i in a + shiftd..=b + shiftd { let e = s.outline.rows.entry(i).or_insert(0); *e = (*e).max(2); } }
    let _ = summaries;
    Ok(r)
}

/// Auto Outline: groups the rows above (or below) each row of SUM/SUBTOTAL formulas that refer to
/// the rows next to them.
pub fn auto_outline(wb: &mut Workbook, sheet: usize, r: Range) -> usize {
    let mut n = 0;
    let s = &wb.sheets[sheet];
    let mut groups = vec![];
    for row in r.r0..=r.r1 {
        for c in r.c0..=r.c1 {
            let Some(f) = s.cells.get(&Pos::new(row, c)).and_then(|x| x.formula()) else { continue };
            let up = f.to_ascii_uppercase();
            if !(up.contains("SUM(") || up.contains("SUBTOTAL(")) { continue; }
            let inner = up.split('(').last().unwrap_or("").trim_end_matches(')');
            let Some(rg) = inner.split(',').last().and_then(|t| crate::addr::parse_range(t.trim().trim_matches('$').replace('$', "").as_str())) else { continue };
            if rg.c0 == c && rg.c1 == c && rg.r1 + 1 == row && rg.r0 < rg.r1 + 1 { groups.push((rg.r0, rg.r1)); break; }
        }
    }
    for (a, b) in groups { if group(&mut wb.sheets[sheet], true, a, b).is_ok() { n += 1; } }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::parse_a1;

    fn list() -> Workbook {
        let mut wb = Workbook::new();
        for (r, row) in [["Region", "Sales"], ["East", "10"], ["East", "5"], ["West", "20"], ["West", "7"], ["North", "1"]].iter().enumerate() {
            for (c, t) in row.iter().enumerate() { wb.set_input(0, Pos::new(r as u32, c as u32), t); }
        }
        wb.recalc();
        wb
    }

    #[test]
    fn group_collapse_and_levels() {
        let mut wb = Workbook::new();
        let s = &mut wb.sheets[0];
        group(s, true, 1, 4).unwrap();
        group(s, true, 2, 3).unwrap();
        assert_eq!(s.outline.max_level(true), 2);
        assert_eq!(s.outline.groups(true, 2), vec![(2, 3)]);
        set_collapsed(s, true, 2, (2, 3), true);
        assert!(s.hidden_rows.contains(&2) && s.outline.collapsed_rows.contains(&4));
        set_collapsed(s, true, 1, (1, 4), true);
        set_collapsed(s, true, 1, (1, 4), false);
        // The nested collapsed group stays hidden.
        assert!(!s.hidden_rows.contains(&1) && s.hidden_rows.contains(&2));
        show_level(s, true, 1);
        assert!((1..=4).all(|i| s.hidden_rows.contains(&i)));
        show_level(s, true, 3);
        assert!(s.hidden_rows.is_empty());
        assert!(detail(s, true, 5, false));
        assert!(s.hidden_rows.contains(&1));
        ungroup(s, true, 1, 4);
        ungroup(s, true, 1, 4);
        assert!(s.outline.rows.is_empty() && s.hidden_rows.is_empty());
    }

    #[test]
    fn subtotals_and_remove() {
        let mut wb = list();
        let r = subtotal(&mut wb, 0, parse_range_("A1:B6"), &SubtotalSpec { group_col: 0, func: 9, cols: vec![1], replace: true, page_breaks: false, below: true }).unwrap();
        wb.recalc();
        assert_eq!(r, parse_range_("A1:B10"));
        assert_eq!(wb.display(0, parse_a1("A4").unwrap()), "East Total");
        assert_eq!(wb.display(0, parse_a1("B4").unwrap()), "15");
        assert_eq!(wb.display(0, parse_a1("B7").unwrap()), "27");
        assert_eq!(wb.display(0, parse_a1("A10").unwrap()), "Grand Total");
        assert_eq!(wb.display(0, parse_a1("B10").unwrap()), "43");
        let o = &wb.sheets[0].outline;
        assert_eq!(o.max_level(true), 2);
        assert_eq!(o.level(true, 1), 2);
        assert_eq!(o.level(true, 3), 1);
        show_level(&mut wb.sheets[0], true, 2);
        assert!(wb.sheets[0].hidden_rows.contains(&1) && !wb.sheets[0].hidden_rows.contains(&3));
        let back = remove_subtotals(&mut wb, 0, r);
        assert_eq!(back, parse_range_("A1:B6"));
        assert_eq!(wb.display(0, parse_a1("A4").unwrap()), "West");
        assert!(wb.sheets[0].outline.rows.is_empty());
    }

    fn parse_range_(s: &str) -> Range { crate::addr::parse_range(s).unwrap() }

    #[test]
    fn outline_round_trip() {
        let mut wb = list();
        group(&mut wb.sheets[0], true, 1, 2).unwrap();
        group(&mut wb.sheets[0], false, 1, 1).unwrap();
        wb.sheets[0].outline.summary_below = false;
        set_collapsed(&mut wb.sheets[0], true, 1, (1, 2), true);
        let bytes = crate::xlsx::write(&wb).unwrap();
        let back = crate::xlsx::read(&bytes).unwrap();
        let o = &back.sheets[0].outline;
        assert_eq!(o.level(true, 1), 1);
        assert_eq!(o.level(true, 2), 1);
        assert_eq!(o.level(false, 1), 1);
        assert!(!o.summary_below && o.summary_right);
        assert!(back.sheets[0].hidden_rows.contains(&1));
        // Collapsed mark sits on the summary line (above, here: row 0).
        assert!(o.collapsed_rows.contains(&0));
    }
}
