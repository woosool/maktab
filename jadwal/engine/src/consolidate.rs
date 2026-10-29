//! Data ▸ Consolidate (consolidate.js): several ranges combined into one table, cell by cell by
//! position or by the labels in their top row and/or left column (the union of labels, in the
//! order first seen), with one of eleven summary functions.

use crate::addr::{Pos, Range};
use crate::book::{Input, Workbook};
use crate::value::Value;
use std::collections::HashMap;

pub const FUNCTIONS: &[&str] = &["Sum", "Count", "Average", "Max", "Min", "Product", "Count Numbers", "StdDev", "StdDevp", "Var", "Varp"];

fn variance(xs: &[f64], ddof: f64) -> f64 { let m = xs.iter().sum::<f64>() / xs.len() as f64; xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() as f64 - ddof) }

/// Applies function `f` (an index into FUNCTIONS) to the numbers of a cell (`all` counts every
/// non-blank value).
pub fn apply(f: usize, xs: &[f64], all: usize) -> Option<f64> {
    let n = xs.len();
    match f {
        0 => Some(xs.iter().sum()),
        1 => Some(all as f64),
        2 => (n > 0).then(|| xs.iter().sum::<f64>() / n as f64),
        3 => xs.iter().copied().reduce(f64::max),
        4 => xs.iter().copied().reduce(f64::min),
        5 => (n > 0).then(|| xs.iter().product()),
        6 => Some(n as f64),
        7 => (n > 1).then(|| variance(xs, 1.0).sqrt()),
        8 => (n > 0).then(|| variance(xs, 0.0).sqrt()),
        9 => (n > 1).then(|| variance(xs, 1.0)),
        10 => (n > 0).then(|| variance(xs, 0.0)),
        _ => None,
    }
}

/// The values of a source (errors as blanks), clipped to the sheet's used area.
pub fn source(wb: &Workbook, sheet: usize, r: Range) -> Vec<Vec<Value>> {
    let s = &wb.sheets[sheet];
    let Some(ext) = s.extent() else { return vec![] };
    let (r1, c1) = (r.r1.min(ext.row), r.c1.min(ext.col));
    (r.r0..=r1).map(|row| (r.c0..=c1).map(|c| { let v = s.value(Pos::new(row, c)); if v.is_err() { Value::Empty } else { v.scalar() } }).collect()).collect()
}

fn key(v: &Value) -> String { match v { Value::Str(s) => format!("s:{}", s.trim().to_lowercase()), v => format!("v:{}", v.display()) } }

/// Combines the sources; returns the result's rows (labels included where asked for).
pub fn consolidate(sources: &[Vec<Vec<Value>>], f: usize, top_row: bool, left_col: bool) -> Option<Vec<Vec<Value>>> {
    let grids: Vec<&Vec<Vec<Value>>> = sources.iter().filter(|g| !g.is_empty() && !g[0].is_empty()).collect();
    if grids.is_empty() { return None; }
    let (mut row_labels, mut col_labels): (Vec<Value>, Vec<Value>) = (vec![], vec![]);
    let (mut ri_map, mut ci_map): (HashMap<String, usize>, HashMap<String, usize>) = (HashMap::new(), HashMap::new());
    let mut cells: HashMap<(usize, usize), (Vec<f64>, usize)> = HashMap::new();
    let at = |l: &Value, list: &mut Vec<Value>, idx: &mut HashMap<String, usize>| -> usize { let k = key(l); *idx.entry(k).or_insert_with(|| { list.push(l.clone()); list.len() - 1 }) };
    for g in grids {
        let (r0, c0) = (top_row as usize, left_col as usize);
        for r in r0..g.len() {
            let rl = if left_col { g[r].first().cloned().unwrap_or(Value::Empty) } else { Value::Empty };
            if left_col && rl.is_empty() { continue; }
            let ri = if left_col { at(&rl, &mut row_labels, &mut ri_map) } else { r - r0 };
            for c in c0..g[r].len() {
                let cl = if top_row { g[0].get(c).cloned().unwrap_or(Value::Empty) } else { Value::Empty };
                if top_row && cl.is_empty() { continue; }
                let ci = if top_row { at(&cl, &mut col_labels, &mut ci_map) } else { c - c0 };
                let e = cells.entry((ri, ci)).or_default();
                match &g[r][c] { Value::Num(x) => { e.0.push(*x); e.1 += 1; } Value::Empty => {} Value::Str(s) if s.is_empty() => {} _ => e.1 += 1 }
            }
        }
    }
    let mut rows = if left_col { row_labels.len() } else { 0 };
    let mut cols = if top_row { col_labels.len() } else { 0 };
    for (ri, ci) in cells.keys() { rows = rows.max(ri + 1); cols = cols.max(ci + 1); }
    let (ro, co) = (top_row as usize, left_col as usize);
    let mut out = vec![vec![Value::Empty; cols + co]; rows + ro];
    if top_row { for (j, l) in col_labels.iter().enumerate() { out[0][j + co] = l.clone(); } }
    if left_col { for (i, l) in row_labels.iter().enumerate() { out[i + ro][0] = l.clone(); } }
    for ((ri, ci), (xs, all)) in cells {
        if let Some(v) = apply(f, &xs, all) { if !(xs.is_empty() && all == 0 && f != 1 && f != 6) { out[ri + ro][ci + co] = Value::Num(v); } }
    }
    Some(out)
}

/// Writes rows of values from `at` on a sheet; returns the area written.
pub fn put(wb: &mut Workbook, sheet: usize, at: Pos, rows: &[Vec<Value>]) -> Range {
    let mut end = at;
    for (i, row) in rows.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            let p = Pos::new(at.row + i as u32, at.col + j as u32);
            end = Pos::new(end.row.max(p.row), end.col.max(p.col));
            if v.is_empty() { wb.clear(sheet, Range::cell(p), true, false, false); continue; }
            let c = wb.sheets[sheet].cell_mut(p);
            c.input = Input::Value(v.clone()); c.value = v.clone(); c.array = None;
        }
    }
    Range::new(at, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(t: &str) -> Value { Value::str(t) }
    fn n(x: f64) -> Value { Value::Num(x) }

    #[test]
    fn by_labels_and_by_position() {
        let a = vec![vec![Value::Empty, s("Q1"), s("Q2")], vec![s("East"), n(1.0), n(2.0)], vec![s("West"), n(3.0), n(4.0)]];
        let b = vec![vec![Value::Empty, s("Q2"), s("Q3")], vec![s("west"), n(10.0), n(5.0)], vec![s("North"), n(7.0), n(8.0)]];
        let r = consolidate(&[a.clone(), b.clone()], 0, true, true).unwrap();
        assert_eq!(r[0], vec![Value::Empty, s("Q1"), s("Q2"), s("Q3")]);
        assert_eq!(r[2], vec![s("West"), n(3.0), n(14.0), n(5.0)]);
        assert_eq!(r[3], vec![s("North"), Value::Empty, n(7.0), n(8.0)]);
        let p = consolidate(&[vec![vec![n(1.0), n(2.0)]], vec![vec![n(3.0)], vec![n(5.0)]]], 2, false, false).unwrap();
        assert_eq!(p, vec![vec![n(2.0), n(2.0)], vec![n(5.0), Value::Empty]]);
        let mut wb = Workbook::new();
        put(&mut wb, 0, Pos::new(0, 0), &r);
        assert_eq!(wb.display(0, Pos::new(2, 2)), "14");
        assert_eq!(source(&wb, 0, crate::addr::parse_range("A1:Z99").unwrap()).len(), 4);
    }
}
