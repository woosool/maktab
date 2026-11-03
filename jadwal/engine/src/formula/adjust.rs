//! Reference rewriting (refs.js): moving a formula's relative references when it is filled or
//! pasted, following inserted and deleted rows and columns, and renamed sheets. Works on the
//! token list so the text around references keeps its spacing and case.

use super::lexer::{tokenize, Tok};
use super::parser::{parse_ref_body, CellRef, RefKind};
use crate::addr::{col_name, quote_sheet, MAX_COLS, MAX_ROWS};

fn cell_text(c: &CellRef, kind: RefKind) -> String {
    let col = format!("{}{}", if c.col_abs { "$" } else { "" }, col_name(c.col));
    let row = format!("{}{}", if c.row_abs { "$" } else { "" }, c.row + 1);
    match kind { RefKind::Cols => col, RefKind::Rows => row, _ => format!("{col}{row}") }
}

fn body_text(a: &CellRef, b: &CellRef, kind: RefKind) -> String {
    match kind { RefKind::Cell => cell_text(a, kind), _ => format!("{}:{}", cell_text(a, kind), cell_text(b, kind)) }
}

fn sheet_text(sheet: &Option<String>, end: &Option<String>) -> String {
    match (sheet, end) {
        (Some(s), Some(e)) => format!("{}:{}!", quote_sheet(s), quote_sheet(e)),
        (Some(s), None) => format!("{}!", quote_sheet(s)),
        _ => String::new(),
    }
}

/// Rewrites each reference with `f(sheet, a, b, kind)`, which returns the new corners or None for
/// a reference that no longer exists (written #REF!).
pub fn map_refs(src: &str, mut f: impl FnMut(&Option<String>, &mut CellRef, &mut CellRef, RefKind) -> bool, mut sheet_map: impl FnMut(&str) -> Option<String>) -> String {
    let Ok(tokens) = tokenize(src) else { return src.to_string() };
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    for t in tokens {
        out.extend(&chars[at..t.start]);
        at = t.end;
        match t.tok {
            Tok::Ref { mut sheet, mut sheet_end, body, spill } => {
                if let Some(s) = &sheet { if let Some(n) = sheet_map(s) { sheet = Some(n); } }
                if let Some(s) = &sheet_end { if let Some(n) = sheet_map(s) { sheet_end = Some(n); } }
                let Some((mut a, mut b, kind)) = parse_ref_body(&body) else { out.extend(&chars[t.start..t.end]); continue };
                if sheet.as_deref() != Some("#REF") && f(&sheet, &mut a, &mut b, kind) {
                    out.push_str(&sheet_text(&sheet, &sheet_end));
                    out.push_str(&body_text(&a, &b, kind));
                    if spill { out.push('#'); }
                } else {
                    out.push_str("#REF!");
                }
            }
            Tok::Name { value, sheet: Some(s) } => {
                let s = sheet_map(&s).unwrap_or(s);
                let orig: String = chars[t.start..t.end].iter().collect();
                let name = orig.rsplit_once('!').map(|x| x.1.to_string()).unwrap_or(value);
                out.push_str(&format!("{}!{}", quote_sheet(&s), name));
            }
            _ => out.extend(&chars[t.start..t.end]),
        }
    }
    out.extend(&chars[at.min(chars.len())..]);
    out
}

fn moved(v: u32, d: i64, max: u32) -> Option<u32> { let n = v as i64 + d; (n >= 0 && n < max as i64).then_some(n as u32) }

/// The formula as it reads `dr` rows and `dc` columns away (fill, copy and paste).
pub fn shift(src: &str, dr: i64, dc: i64) -> String {
    if dr == 0 && dc == 0 { return src.to_string(); }
    map_refs(src, |_, a, b, kind| {
        for c in [a, b] {
            if !c.row_abs && kind != RefKind::Cols { match moved(c.row, dr, MAX_ROWS) { Some(r) => c.row = r, None => return false } }
            if !c.col_abs && kind != RefKind::Rows { match moved(c.col, dc, MAX_COLS) { Some(x) => c.col = x, None => return false } }
        }
        true
    }, |_| None)
}

/// Follows `n` rows (or columns) inserted at `at` (n > 0) or deleted from `at` (n < 0) on the
/// sheet `target`; `own` is the name of the sheet the formula lives on.
pub fn insert(src: &str, own: &str, target: &str, rows: bool, at: u32, n: i64) -> String {
    map_refs(src, |sheet, a, b, kind| {
        let on = sheet.as_deref().unwrap_or(own);
        if !on.eq_ignore_ascii_case(target) { return true; }
        if (rows && kind == RefKind::Cols) || (!rows && kind == RefKind::Rows) { return true; }
        let get = |c: &CellRef| if rows { c.row } else { c.col };
        let max = if rows { MAX_ROWS } else { MAX_COLS };
        let (lo, hi) = (get(a), get(b));
        let (lo2, hi2) = if n > 0 {
            let lo2 = if lo >= at { lo as i64 + n } else { lo as i64 };
            let hi2 = if hi >= at { hi as i64 + n } else { hi as i64 };
            (lo2, hi2.min(max as i64 - 1))
        } else {
            let d = -n as u32; let end = at + d; // [at, end) is removed
            if lo >= at && hi < end { return false; }
            let f = |v: u32, is_hi: bool| -> i64 { if v < at { v as i64 } else if v >= end { v as i64 - d as i64 } else if is_hi { at as i64 - 1 } else { at as i64 } };
            (f(lo, false), f(hi, true))
        };
        if lo2 >= max as i64 { return false; }
        let set = |c: &mut CellRef, v: i64| if rows { c.row = v as u32 } else { c.col = v as u32 };
        set(a, lo2); set(b, hi2);
        true
    }, |_| None)
}

/// Follows a renamed sheet.
pub fn rename_sheet(src: &str, old: &str, new: &str) -> String {
    map_refs(src, |_, _, _, _| true, |s| s.eq_ignore_ascii_case(old).then(|| new.to_string()))
}

/// References to a deleted sheet become #REF!.
pub fn drop_sheet(src: &str, gone: &str) -> String {
    map_refs(src, |s, _, _, _| !s.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(gone)), |_| None)
}

/// Follows Insert Cells (or Delete Cells) over `r` on sheet `target`, as Excel adjusts
/// references: those whose cross extent (columns when shifting down, rows when shifting right)
/// lies within `r`'s grow when the insertion point falls inside them, move when they start at
/// or past it, and shrink (or become #REF! when wholly deleted) on deletion; others stay.
pub fn shift_cells(src: &str, own: &str, target: &str, r: crate::addr::Range, down: bool, delete: bool) -> String {
    map_refs(src, |sheet, a, b, kind| {
        let on = sheet.as_deref().unwrap_or(own);
        if !on.eq_ignore_ascii_case(target) { return true; }
        if (down && kind == RefKind::Cols) || (!down && kind == RefKind::Rows) { return true; }
        let (x0, x1, rc0, rc1) = if down { (a.col.min(b.col), a.col.max(b.col), r.c0, r.c1) } else { (a.row.min(b.row), a.row.max(b.row), r.r0, r.r1) };
        if x0 < rc0 || x1 > rc1 { return true; }
        let (at, n, max) = if down { (r.r0, r.rows(), MAX_ROWS) } else { (r.c0, r.cols(), MAX_COLS) };
        let get = |c: &CellRef| if down { c.row } else { c.col };
        let (lo, hi) = (get(a).min(get(b)), get(a).max(get(b)));
        let (lo2, hi2) = if !delete {
            let lo2 = if lo >= at { lo as i64 + n as i64 } else { lo as i64 };
            let hi2 = if hi >= at { hi as i64 + n as i64 } else { hi as i64 };
            if lo2 >= max as i64 { return false; }
            (lo2, hi2.min(max as i64 - 1))
        } else {
            let end = at + n;
            if lo >= at && hi < end { return false; }
            let f = |v: u32, is_hi: bool| -> i64 { if v < at { v as i64 } else if v >= end { v as i64 - n as i64 } else if is_hi { at as i64 - 1 } else { at as i64 } };
            (f(lo, false), f(hi, true))
        };
        let set = |c: &mut CellRef, v: i64| if down { c.row = v as u32 } else { c.col = v as u32 };
        set(a, lo2); set(b, hi2);
        true
    }, |_| None)
}

/// Moves references inside `from` (on sheet `target`) by (dr, dc), as dragging cells does.
pub fn move_area(src: &str, own: &str, target: &str, from: crate::addr::Range, dr: i64, dc: i64) -> String {
    map_refs(src, |sheet, a, b, _| {
        let on = sheet.as_deref().unwrap_or(own);
        if !on.eq_ignore_ascii_case(target) { return true; }
        let r = crate::addr::Range::new(crate::addr::Pos::new(a.row, a.col), crate::addr::Pos::new(b.row, b.col));
        if r.r0 >= from.r0 && r.r1 <= from.r1 && r.c0 >= from.c0 && r.c1 <= from.c1 {
            for c in [a, b] {
                match (moved(c.row, dr, MAX_ROWS), moved(c.col, dc, MAX_COLS)) { (Some(x), Some(y)) => { c.row = x; c.col = y; } _ => return false }
            }
        }
        true
    }, |_| None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shifting() {
        assert_eq!(shift("SUM(A1:B2)+$C$1+C$1", 1, 1), "SUM(B2:C3)+$C$1+D$1");
        assert_eq!(shift("A1", -1, 0), "#REF!");
        assert_eq!(insert("SUM(A1:A10)", "S", "S", true, 4, 2), "SUM(A1:A12)");
        assert_eq!(insert("A5+A1", "S", "S", true, 4, -1), "#REF!+A1");
        assert_eq!(insert("SUM(A1:A10)", "S", "S", true, 2, -3), "SUM(A1:A7)");
        assert_eq!(shift_cells("SUM(A1:A10)+B5+A1:B4", "S", "S", crate::addr::parse_range("A3:A4").unwrap(), true, false), "SUM(A1:A12)+B5+A1:B4");
        assert_eq!(shift_cells("A5+SUM(A1:A10)+A3", "S", "S", crate::addr::parse_range("A3:A4").unwrap(), true, true), "A3+SUM(A1:A8)+#REF!");
        assert_eq!(shift_cells("SUM(A1:D1)", "S", "S", crate::addr::parse_range("B1:B3").unwrap(), false, false), "SUM(A1:E1)");
        assert_eq!(rename_sheet("Old!A1+'Old'!B2", "Old", "New Name"), "'New Name'!A1+'New Name'!B2");
    }
}
