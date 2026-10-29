//! Timelines (timeline.js, inserttimeline.js): a floating filter for a PivotTable or a table on
//! a date field. It shows the field's dates as a strip of periods (years, quarters, months or
//! days); selecting a run of periods filters the pivot (its records, through
//! `Pivot::date_filter`) or the table (a between-dates custom filter on the column).
//!
//! In the file a timeline is Excel's timeline parts when its pivot can be written natively
//! (xlsx/timeline_parts.rs), else a text box tagged so Jadwal reads it back.

use crate::book::Workbook;
use crate::date::{serial, ymd};
use crate::drawing::{ObjKind, Shape};
use crate::value::Value;
use crate::Pos;

/// The period size a timeline shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Level { Years, Quarters, #[default] Months, Days }

impl Level {
    pub const ALL: [Level; 4] = [Level::Years, Level::Quarters, Level::Months, Level::Days];
    pub fn label(self) -> &'static str { match self { Level::Years => "YEARS", Level::Quarters => "QUARTERS", Level::Months => "MONTHS", Level::Days => "DAYS" } }
    /// Excel's timeline level number (x15:timelineState/@level... and timeline/@level): 0 years … 3 days.
    pub fn code(self) -> u8 { self as u8 }
    pub fn from_code(c: u8) -> Level { Level::ALL.get(c as usize).copied().unwrap_or_default() }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Timeline {
    /// The PivotTable it filters (by name), or empty for a table's.
    pub pivot: String,
    /// The table it filters when `pivot` is empty.
    pub table: String,
    /// The date field: a pivot source column, or a table column, by offset.
    pub field: usize,
    pub caption: String,
    pub level: Level,
    /// The selected days (serial dates, inclusive); None shows everything.
    pub selection: Option<(f64, f64)>,
}

/// One period of the strip: its label (with the year it starts, for headings) and its days.
#[derive(Clone, Debug, PartialEq)]
pub struct Period { pub label: String, pub year: i64, pub start: f64, pub end: f64 }

const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/// The source column: (sheet, column, first data row, last data row).
fn column(wb: &Workbook, t: &Timeline) -> Option<(usize, u32, u32, u32)> {
    if t.pivot.is_empty() {
        let (si, ti) = wb.find_table(&t.table)?;
        let tb = &wb.sheets[si].tables[ti];
        let r0 = tb.range.r0 + tb.header_row as u32;
        let r1 = tb.range.r1 - tb.totals_row as u32;
        return Some((si, tb.range.c0 + t.field as u32, r0, r1));
    }
    let p = wb.sheets.iter().flat_map(|s| s.pivots.iter()).find(|p| p.name == t.pivot)?;
    let si = wb.sheet_index(&p.source_sheet)?;
    Some((si, p.source.c0 + t.field as u32, p.source.r0 + 1, p.source.r1))
}

/// The field's dates (whole days), in sheet order.
pub fn dates(wb: &Workbook, t: &Timeline) -> Vec<f64> {
    let Some((si, c, r0, r1)) = column(wb, t) else { return vec![] };
    let s = &wb.sheets[si];
    (r0..=r1.max(r0)).filter_map(|r| match s.value(Pos::new(r, c)) { Value::Num(n) if n >= 1.0 => Some(n.floor()), _ => None }).collect()
}

/// Whether a column holds dates: numbers shown with a date format.
pub fn is_date_column(wb: &Workbook, si: usize, c: u32, r0: u32, r1: u32) -> bool {
    let s = &wb.sheets[si];
    let mut any = false;
    for r in r0..=r1.min(r0 + 200) {
        let p = Pos::new(r, c);
        match s.value(p) {
            Value::Num(_) => { let f = wb.styles.get(s.style_index(p)).num_fmt.to_ascii_lowercase(); if !(f.contains('d') || f.contains('y') || f.contains("mmm")) || f.contains('0') && !f.contains('y') { return false; } any = true; }
            Value::Empty => {}
            _ => return false,
        }
    }
    any
}

fn start_of(d: f64, level: Level, d1904: bool) -> (f64, f64, String, i64) {
    let (y, m, day) = ymd(d, d1904);
    match level {
        Level::Years => (serial(y, 1, 1, d1904), serial(y + 1, 1, 1, d1904) - 1.0, y.to_string(), y),
        Level::Quarters => { let q = (m - 1) / 3; let s = serial(y, q as i64 * 3 + 1, 1, d1904); (s, serial(y, q as i64 * 3 + 4, 1, d1904) - 1.0, format!("Q{}", q + 1), y) }
        Level::Months => (serial(y, m as i64, 1, d1904), serial(y, m as i64 + 1, 1, d1904) - 1.0, MONTHS[m as usize - 1].into(), y),
        Level::Days => (d, d, day.to_string(), y),
    }
}

/// The periods from the earliest date's to the latest's (Excel shows whole years).
pub fn periods(wb: &Workbook, t: &Timeline) -> Vec<Period> {
    let ds = dates(wb, t);
    let (Some(lo), Some(hi)) = (ds.iter().copied().reduce(f64::min), ds.iter().copied().reduce(f64::max)) else { return vec![] };
    let d1904 = wb.date1904;
    let (ylo, _, _) = ymd(lo, d1904);
    let (yhi, _, _) = ymd(hi, d1904);
    let (mut d, end) = if t.level == Level::Days { (lo, hi) } else { (serial(ylo, 1, 1, d1904), serial(yhi + 1, 1, 1, d1904) - 1.0) };
    let mut out = vec![];
    while d <= end && out.len() < 5000 {
        let (s, e, label, year) = start_of(d, t.level, d1904);
        out.push(Period { label, year, start: s, end: e });
        d = e + 1.0;
    }
    out
}

/// The selection's caption: "Jan 2024 - Mar 2024", "2023", "All Periods".
pub fn selection_text(wb: &Workbook, t: &Timeline) -> String {
    let Some((a, b)) = t.selection else { return "All Periods".into() };
    let f = |d: f64| { let (y, m, day) = ymd(d, wb.date1904); match t.level { Level::Years => y.to_string(), Level::Quarters => format!("Q{} {y}", (m - 1) / 3 + 1), Level::Months => format!("{} {y}", title_case(MONTHS[m as usize - 1])), Level::Days => format!("{} {day}, {y}", title_case(MONTHS[m as usize - 1])) } };
    let (x, y) = (f(a), f(b));
    if x == y { x } else { format!("{x} - {y}") }
}

fn title_case(s: &str) -> String { let mut c = s.chars(); c.next().map(|f| format!("{f}{}", c.as_str().to_lowercase())).unwrap_or_default() }

/// Makes every pivot's date filter match the timelines that point at it (call after a timeline,
/// pivot or load changes). Returns the pivots whose filter changed, as (sheet, index).
pub fn sync_pivots(wb: &mut Workbook) -> Vec<(usize, usize)> {
    let mut want: Vec<(String, Option<(usize, f64, f64)>)> = vec![];
    for s in &wb.sheets { for o in &s.objects { if let ObjKind::Timeline(t) = &o.kind { if !t.pivot.is_empty() { want.push((t.pivot.clone(), t.selection.map(|(a, b)| (t.field, a, b)))); } } } }
    let mut changed = vec![];
    for (si, s) in wb.sheets.iter_mut().enumerate() {
        for (k, p) in s.pivots.iter_mut().enumerate() {
            let f = want.iter().filter(|w| w.0 == p.name).find_map(|w| w.1);
            if p.date_filter != f { p.date_filter = f; changed.push((si, k)); }
        }
    }
    changed
}

/// Applies a table timeline's selection as the column's filter.
pub fn apply_table(wb: &mut Workbook, t: &Timeline) {
    let Some((si, ti)) = wb.find_table(&t.table) else { return };
    use crate::tables::{CmpOp, Cond, Criteria, Scope};
    let crit = t.selection.map(|(a, b)| Criteria::Custom { a: Cond { op: CmpOp::Ge, val: a.to_string() }, b: Some(Cond { op: CmpOp::Lt, val: (b + 1.0).to_string() }), and: true });
    if wb.sheets[si].tables[ti].filter.is_none() { wb.sheets[si].tables[ti].filter = Some(Default::default()); }
    wb.set_column_filter(si, Scope::Table(ti), t.field as u32, crit);
}

/// Applies a timeline: a table's filter at once; pivots through `sync_pivots`, then each changed
/// pivot is rerun. Returns the pivots to rerun.
pub fn apply(wb: &mut Workbook, t: &Timeline) -> Vec<(usize, usize)> {
    if t.pivot.is_empty() { apply_table(wb, t); vec![] } else { sync_pivots(wb) }
}

// ---- the tagged text box ----

const TAG: &str = "jadwal:timeline";

pub fn tag(t: &Timeline) -> String {
    let sel = t.selection.map(|(a, b)| format!("{a}\t{b}")).unwrap_or_else(|| "\t".into());
    format!("{TAG}\t{}\t{}\t{}\t{}\t{}\t{sel}", t.pivot, t.table, t.field, t.caption, t.level.code())
}

pub fn from_tag(descr: &str) -> Option<Timeline> {
    let rest = descr.strip_prefix(TAG)?.strip_prefix('\t')?;
    let f: Vec<&str> = rest.split('\t').collect();
    let sel = match (f.get(5).and_then(|x| x.parse::<f64>().ok()), f.get(6).and_then(|x| x.parse::<f64>().ok())) { (Some(a), Some(b)) => Some((a, b)), _ => None };
    Some(Timeline { pivot: f.first()?.to_string(), table: f.get(1).copied().unwrap_or("").into(), field: f.get(2).and_then(|x| x.parse().ok()).unwrap_or(0), caption: f.get(3).copied().unwrap_or("").into(), level: Level::from_code(f.get(4).and_then(|x| x.parse().ok()).unwrap_or(2)), selection: sel })
}

/// What a timeline looks like to other programs when it isn't written natively.
pub fn fallback_shape(wb: &Workbook, t: &Timeline) -> Shape {
    Shape { text: format!("{}\n{}", t.caption, selection_text(wb, t)), preset: "rect".into(), text_box: true, fill: Some([255, 255, 255]), line: Some([0xBF, 0xBF, 0xBF]), size: 10.0, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{add_object, Anchor, Object};

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        wb.set_input(0, Pos::new(0, 0), "Date");
        wb.set_input(0, Pos::new(0, 1), "Amount");
        for (i, d) in ["1/15/2024", "2/10/2024", "3/5/2024", "7/1/2024", "1/2/2025"].iter().enumerate() {
            wb.set_input(0, Pos::new(i as u32 + 1, 0), d);
            wb.set_input(0, Pos::new(i as u32 + 1, 1), &((i + 1) * 10).to_string());
        }
        wb
    }

    #[test]
    fn periods_and_pivot_filter() {
        let mut wb = book();
        assert!(is_date_column(&wb, 0, 0, 1, 5));
        let mut p = crate::pivot::Pivot::new("PivotTable1", "Sheet1", crate::addr::parse_range("A1:B6").unwrap(), Pos::new(0, 4));
        p.values.push(crate::pivot::ValueField { field: 1, ..Default::default() });
        wb.sheets[0].pivots.push(p);
        let mut t = Timeline { pivot: "PivotTable1".into(), field: 0, caption: "Date".into(), ..Default::default() };
        let ps = periods(&wb, &t);
        assert_eq!(ps.len(), 24);
        assert_eq!(ps[0].label, "JAN");
        t.level = Level::Years;
        assert_eq!(periods(&wb, &t).len(), 2);
        t.level = Level::Months;
        t.selection = Some((ps[0].start, ps[1].end));
        assert_eq!(selection_text(&wb, &t), "Jan 2024 - Feb 2024");
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Timeline(t.clone()), Anchor::default()), "Timeline");
        assert_eq!(sync_pivots(&mut wb), vec![(0, 0)]);
        let rep = wb.sheets[0].pivots[0].compute(&wb);
        let total = rep.cells.iter().flatten().filter_map(|v| if let Value::Num(n) = v { Some(*n) } else { None }).fold(0.0, f64::max);
        assert_eq!(total, 30.0);
        // Saved and read back as the tagged text box.
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let tl = back.sheets[0].objects.iter().find_map(|o| if let ObjKind::Timeline(t) = &o.kind { Some(t.clone()) } else { None }).expect("timeline");
        assert_eq!(tl, t);
        // Printed: its caption and the period labels are on the page.
        wb.sheets[0].objects[0].anchor = Anchor::Two { from: crate::drawing::Mark { col: 0, row: 7, ..Default::default() }, to: crate::drawing::Mark { col: 5, row: 14, ..Default::default() }, edit_as: None };
        let pages = crate::print::sheet_pages(&wb, 0, None);
        let pb = crate::print::draw_page(&wb, &pages[0], &Default::default());
        let dbg = format!("{:?}", pb);
        assert!(dbg.contains("Path"), "the timeline's frame is drawn");
    }

    #[test]
    fn table_filter() {
        let mut wb = book();
        let r = crate::addr::parse_range("A1:B6").unwrap();
        wb.create_table(0, r, true, "TableStyleMedium2").unwrap();
        let name = wb.sheets[0].tables[0].name.clone();
        let mut t = Timeline { table: name, field: 0, caption: "Date".into(), level: Level::Years, ..Default::default() };
        let ps = periods(&wb, &t);
        t.selection = Some((ps[1].start, ps[1].end));
        apply(&mut wb, &t);
        let hidden = (1..=5).filter(|r| wb.sheets[0].hidden_rows.contains(r)).count();
        assert_eq!(hidden, 4);
    }
}
