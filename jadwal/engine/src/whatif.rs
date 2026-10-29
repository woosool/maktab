//! What-If Analysis (goalseek, datatable.js, scenarios.js, scenario-manager.js): Goal Seek, the
//! Scenario Manager with its summary report, and data tables (the TABLE function).
//!
//! A data table is a legacy array formula `{=TABLE(row_input, col_input)}` over the table's
//! results; each result is the table's formula worked out on a copy of the workbook with the
//! input cell(s) set to that row's and column's values. In the file it is `<f t="dataTable">`.
//! With "Automatic Except for Data Tables" (`calc_no_tables`) tables keep their values until a
//! full calculation (`recalc_all`, F9).

use crate::addr::{parse_a1, parse_range, Pos, Range};
use crate::book::{Cell, Formula, Input, Workbook};
use crate::formula::eval::{Arg, Ctx};
use crate::formula::functions::{l, Def};
use crate::formula::parser::Expr;
use crate::value::{ErrKind, Matrix, Value};
use std::cell::Cell as StdCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ---- Goal Seek ---------------------------------------------------------------------------------

/// Goal Seek's outcome: the changing cell's value found, and the formula's value there.
#[derive(Clone, Debug, PartialEq)]
pub struct Seek { pub found: bool, pub x: f64, pub value: f64, pub iterations: u32 }

fn set_num(wb: &mut Workbook, s: usize, p: Pos, x: f64) {
    let c = wb.sheets[s].cell_mut(p);
    c.input = Input::Value(Value::Num(x));
    c.value = Value::Num(x);
}

fn read_num(wb: &Workbook, s: usize, p: Pos) -> Option<f64> { wb.sheets[s].value(p).to_num().ok().filter(|v| v.is_finite()) }

/// Goal Seek: changes `changing` (a constant) until the formula in `set` equals `target`.
/// The workbook is left with the value found (or the closest one) in `changing`.
pub fn goal_seek(wb: &mut Workbook, sheet: usize, set: Pos, target: f64, changing: Pos, max_iter: u32, max_change: f64) -> Result<Seek, String> {
    if !wb.sheets[sheet].cells.get(&set).is_some_and(|c| c.is_formula()) { return Err("Set cell must contain a formula.".into()); }
    if wb.sheets[sheet].cells.get(&changing).is_some_and(|c| c.is_formula()) { return Err("By changing cell must contain a value.".into()); }
    let x0 = read_num(wb, sheet, changing).unwrap_or(0.0);
    let f = |wb: &mut Workbook, x: f64| -> Option<f64> { set_num(wb, sheet, changing, x); wb.recalc(); read_num(wb, sheet, set).map(|v| v - target) };
    let tol = max_change.abs().max(1e-12);
    let mut best = (x0, f(wb, x0));
    let mut it = 0;
    let done = |v: Option<f64>| v.is_some_and(|v| v.abs() <= tol);
    if done(best.1) { return Ok(Seek { found: true, x: x0, value: best.1.unwrap() + target, iterations: 0 }); }
    // Secant steps from two nearby points; when they stall, bracket and bisect.
    let mut a = x0;
    let mut fa = best.1;
    let mut b = if x0 == 0.0 { 0.01 } else { x0 * 1.01 };
    let mut fb = f(wb, b);
    let mut bracket: Option<(f64, f64, f64, f64)> = None;
    while it < max_iter {
        it += 1;
        if let (Some(va), Some(vb)) = (fa, fb) {
            if va.signum() != vb.signum() { bracket = Some((a, va, b, vb)); }
        }
        if let Some(vb) = fb { if best.1.is_none_or(|v| vb.abs() < v.abs()) { best = (b, fb); } }
        if done(fb) { break; }
        let next = match (fa, fb) {
            (Some(va), Some(vb)) if (vb - va).abs() > 1e-300 => b - vb * (b - a) / (vb - va),
            _ => b + (b - a) * 2.0 + 1.0,
        };
        let next = if next.is_finite() && (next - b).abs() < 1e12 * (1.0 + b.abs()) { next } else if let Some((l, _, h, _)) = bracket { (l + h) / 2.0 } else { b * 2.0 + 1.0 };
        a = b; fa = fb; b = next; fb = f(wb, b);
        // Inside a bracket the secant may wander: bisect when it leaves.
        if let Some((l, vl, h, vh)) = bracket {
            let (lo, hi) = (l.min(h), l.max(h));
            if !(lo..=hi).contains(&b) || fb.is_none() {
                let m = (l + h) / 2.0;
                let fm = f(wb, m);
                if let Some(v) = fm { if v.signum() == vl.signum() { bracket = Some((m, v, h, vh)); } else { bracket = Some((l, vl, m, v)); } }
                a = b; fa = fb; b = m; fb = fm;
            }
        }
    }
    let found = done(best.1);
    let (x, _) = best;
    set_num(wb, sheet, changing, x);
    wb.recalc();
    Ok(Seek { found, x, value: read_num(wb, sheet, set).unwrap_or(f64::NAN), iterations: it })
}

// ---- Scenarios -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Scenario {
    pub name: String,
    pub comment: String,
    pub user: String,
    pub locked: bool,
    pub hidden: bool,
    /// Changing cells with the values (as typed) this scenario gives them.
    pub cells: Vec<(Pos, String)>,
}

/// The sheet's scenarios (its `<scenarios>` element).
pub fn scenarios(wb: &Workbook, sheet: usize) -> Vec<Scenario> {
    use crate::xlsx::xml::Element;
    let Some(raw) = wb.sheets[sheet].extra.iter().find(|(k, _)| k == "scenarios").map(|(_, v)| v.clone()) else { return vec![] };
    let Ok(e) = Element::parse(&raw) else { return vec![] };
    e.all("scenario").map(|s| Scenario {
        name: s.attr("name").unwrap_or("").to_string(), comment: s.attr("comment").unwrap_or("").to_string(), user: s.attr("user").unwrap_or("").to_string(),
        locked: s.flag("locked", false), hidden: s.flag("hidden", false),
        cells: s.all("inputCells").filter_map(|c| Some((parse_a1(c.attr("r")?)?, c.attr("val").unwrap_or("").to_string()))).collect(),
    }).collect()
}

/// Replaces the sheet's scenarios.
pub fn set_scenarios(wb: &mut Workbook, sheet: usize, list: &[Scenario], current: usize) {
    use crate::xlsx::xml::attr;
    let s = &mut wb.sheets[sheet];
    s.extra.retain(|(k, _)| k != "scenarios");
    if list.is_empty() { return; }
    let mut cells: Vec<Pos> = list.iter().flat_map(|x| x.cells.iter().map(|c| c.0)).collect();
    cells.sort(); cells.dedup();
    let sqref: Vec<String> = cells.iter().map(|p| p.a1()).collect();
    let mut x = format!("<scenarios current=\"{}\" show=\"{}\" sqref=\"{}\">", current.min(list.len() - 1), current.min(list.len() - 1), sqref.join(" "));
    for sc in list {
        x.push_str(&format!("<scenario name=\"{}\"", attr(&sc.name)));
        if sc.locked { x.push_str(" locked=\"1\""); }
        if sc.hidden { x.push_str(" hidden=\"1\""); }
        x.push_str(&format!(" count=\"{}\"", sc.cells.len()));
        if !sc.user.is_empty() { x.push_str(&format!(" user=\"{}\"", attr(&sc.user))); }
        if !sc.comment.is_empty() { x.push_str(&format!(" comment=\"{}\"", attr(&sc.comment))); }
        x.push('>');
        for (p, v) in &sc.cells { x.push_str(&format!("<inputCells r=\"{}\" val=\"{}\"/>", p.a1(), attr(v))); }
        x.push_str("</scenario>");
    }
    x.push_str("</scenarios>");
    s.extra.push(("scenarios".into(), x));
}

/// Show: puts a scenario's values into its changing cells.
pub fn show_scenario(wb: &mut Workbook, sheet: usize, sc: &Scenario) {
    for (p, v) in &sc.cells { wb.set_input(sheet, *p, v); }
    wb.recalc();
}

/// Scenario Summary: a new sheet listing the changing and result cells under Current Values and
/// each scenario. Returns the new sheet's index.
pub fn summary(wb: &mut Workbook, sheet: usize, results: &[Pos]) -> usize {
    let list = scenarios(wb, sheet);
    let mut cells: Vec<Pos> = list.iter().flat_map(|x| x.cells.iter().map(|c| c.0)).collect();
    cells.sort(); cells.dedup();
    let mut cols: Vec<(String, Vec<Value>, Vec<Value>)> = vec![];
    let read = |b: &Workbook| -> (Vec<Value>, Vec<Value>) { (cells.iter().map(|p| b.sheets[sheet].value(*p)).collect(), results.iter().map(|p| b.sheets[sheet].value(*p)).collect()) };
    let cur = read(wb);
    cols.push(("Current Values:".into(), cur.0, cur.1));
    for sc in &list {
        let mut b = light_copy(wb);
        show_scenario(&mut b, sheet, sc);
        let (a, r) = read(&b);
        cols.push((sc.name.clone(), a, r));
    }
    let at = wb.add_sheet(wb.sheets.len(), Some(wb.unique_sheet_name("Scenario Summary")));
    let put = |wb: &mut Workbook, r: u32, c: u32, v: Value| { let cell = wb.sheets[at].cell_mut(Pos::new(r, c)); cell.input = Input::Value(v.clone()); cell.value = v; };
    put(wb, 1, 1, Value::str("Scenario Summary"));
    for (j, (name, _, _)) in cols.iter().enumerate() { put(wb, 2, 3 + j as u32, Value::str(name)); }
    put(wb, 3, 1, Value::str("Changing Cells:"));
    for (i, p) in cells.iter().enumerate() {
        put(wb, 4 + i as u32, 2, Value::str(format!("${}${}", crate::addr::col_name(p.col), p.row + 1)));
        for (j, c) in cols.iter().enumerate() { put(wb, 4 + i as u32, 3 + j as u32, c.1[i].clone()); }
    }
    let r0 = 4 + cells.len() as u32;
    put(wb, r0, 1, Value::str("Result Cells:"));
    for (i, p) in results.iter().enumerate() {
        put(wb, r0 + 1 + i as u32, 2, Value::str(format!("${}${}", crate::addr::col_name(p.col), p.row + 1)));
        for (j, c) in cols.iter().enumerate() { put(wb, r0 + 1 + i as u32, 3 + j as u32, c.2[i].clone()); }
    }
    let note = r0 + 2 + results.len() as u32;
    put(wb, note, 1, Value::str("Notes: Current Values column represents values of changing cells at"));
    put(wb, note + 1, 1, Value::str("time Scenario Summary Report was created. Changing cells for each"));
    put(wb, note + 2, 1, Value::str("scenario are highlighted in gray."));
    let bold = |s: &mut crate::style::Style| s.bold = true;
    wb.apply_style(at, &[Range { r0: 1, c0: 1, r1: 2, c1: 3 + cols.len() as u32 }], &bold);
    wb.sheets[at].col_widths.insert(1, 110.0);
    wb.sheets[at].col_widths.insert(2, 70.0);
    for j in 0..cols.len() as u32 { wb.sheets[at].col_widths.insert(3 + j, 100.0); }
    at
}

/// A copy of the workbook without its package parts, for evaluating changed inputs.
pub fn light_copy(wb: &Workbook) -> Workbook {
    let mut b = Workbook::new();
    b.sheets = wb.sheets.clone();
    b.names = wb.names.clone();
    b.date1904 = wb.date1904;
    b.styles = wb.styles.clone();
    b.iterate = wb.iterate;
    b
}

// ---- Data tables -----------------------------------------------------------------------------

static FORCE: AtomicUsize = AtomicUsize::new(0);
thread_local!(static INSIDE: StdCell<bool> = const { StdCell::new(false) });

/// A full calculation, data tables included (Calculate Now under "Automatic Except for Data
/// Tables").
pub fn recalc_all(wb: &mut Workbook) {
    FORCE.fetch_add(1, Ordering::SeqCst);
    wb.recalc();
    FORCE.fetch_sub(1, Ordering::SeqCst);
}

pub const DEFS: &[Def] = &[l("TABLE", 2, 2, table_fn)];

fn table_ref(c: &mut Ctx, e: &Expr) -> Result<Option<(usize, Pos)>, ErrKind> {
    match c.arg(e) {
        Arg::Missing => Ok(None),
        Arg::R(s, r) if r.is_cell() => Ok(Some((s, r.start()))),
        Arg::V(Value::Empty) => Ok(None),
        _ => Err(ErrKind::Ref),
    }
}

fn stored(c: &Ctx, area: Range) -> Value {
    let s = &c.book.sheets[c.sheet];
    let anchor = s.cells.get(&c.pos).map(|x| x.value.clone()).unwrap_or(Value::Empty);
    if let Value::Array(_) = anchor { return anchor; }
    let rows: Vec<Vec<Value>> = (area.r0..=area.r1).map(|r| (area.c0..=area.c1).map(|col| if Pos::new(r, col) == c.pos { anchor.clone() } else { s.spill.get(&Pos::new(r, col)).cloned().unwrap_or(Value::Empty) }).collect()).collect();
    Value::array(Matrix::from_rows(rows))
}

fn table_fn(c: &mut Ctx, a: &[Expr]) -> Value {
    let Some(area) = c.book.sheets[c.sheet].cells.get(&c.pos).and_then(|x| x.array) else { return Value::Err(ErrKind::Ref) };
    if INSIDE.with(|i| i.get()) { return stored(c, area); }
    let (row_in, col_in) = match (table_ref(c, &a[0]), table_ref(c, &a[1])) { (Ok(r), Ok(cl)) => (r, cl), _ => return Value::Err(ErrKind::Ref) };
    if row_in.is_none() && col_in.is_none() { return Value::Err(ErrKind::Ref); }
    if c.book.calc_no_tables && FORCE.load(Ordering::SeqCst) == 0 {
        let st = stored(c, area);
        let empty = match &st { Value::Array(m) => m.data.iter().all(|v| v.is_empty()), v => v.is_empty() };
        if !empty { return st; }
    }
    if area.r0 == 0 || area.c0 == 0 { return Value::Err(ErrKind::Ref); }
    let s = c.sheet;
    let val = |c: &Ctx, r: u32, col: u32| c.cell(s, Pos::new(r, col));
    let mut b = light_copy(c.book);
    let mut out = Vec::with_capacity(area.rows() as usize);
    INSIDE.with(|i| i.set(true));
    for r in area.r0..=area.r1 {
        let mut row = Vec::with_capacity(area.cols() as usize);
        for col in area.c0..=area.c1 {
            let (formula, inputs): (Pos, Vec<((usize, Pos), Value)>) = match (row_in, col_in) {
                (Some(ri), Some(ci)) => (Pos::new(area.r0 - 1, area.c0 - 1), vec![(ri, val(c, area.r0 - 1, col)), (ci, val(c, r, area.c0 - 1))]),
                (None, Some(ci)) => (Pos::new(area.r0 - 1, col), vec![(ci, val(c, r, area.c0 - 1))]),
                (Some(ri), None) => (Pos::new(r, area.c0 - 1), vec![(ri, val(c, area.r0 - 1, col))]),
                (None, None) => unreachable!(),
            };
            let saved: Vec<((usize, Pos), Option<Cell>)> = inputs.iter().map(|(k, _)| (*k, b.sheets[k.0].cells.get(&k.1).cloned())).collect();
            for ((si, p), v) in &inputs { let cell = b.sheets[*si].cell_mut(*p); cell.input = Input::Value(v.clone()); cell.value = v.clone(); }
            let v = crate::formula::eval::compute_fresh(&b, s, formula).scalar();
            row.push(if v.is_empty() { Value::Num(0.0) } else { v });
            for ((si, p), old) in saved { match old { Some(o) => { b.sheets[si].cells.insert(p, o); } None => { b.sheets[si].cells.remove(&p); } } }
        }
        out.push(row);
    }
    INSIDE.with(|i| i.set(false));
    Value::array(Matrix::from_rows(out))
}

/// Data ▸ What-If Analysis ▸ Data Table over `r` (formulas and input values included, as Excel
/// selects it): the results go into the interior as `{=TABLE(row_input, col_input)}`.
pub fn create_table(wb: &mut Workbook, sheet: usize, r: Range, row_in: Option<Pos>, col_in: Option<Pos>) -> Result<Range, String> {
    if row_in.is_none() && col_in.is_none() { return Err("Input cell reference is not valid.".into()); }
    if r.rows() < 2 || r.cols() < 2 { return Err("Select the formulas and the input values together (at least two rows and two columns).".into()); }
    let area = Range { r0: r.r0 + 1, c0: r.c0 + 1, ..r };
    if let Some(p) = row_in.into_iter().chain(col_in).find(|p| area.contains(*p)) { return Err(format!("Input cell {} is inside the table.", p.a1())); }
    let text = format!("TABLE({},{})", row_in.map(|p| p.a1()).unwrap_or_default(), col_in.map(|p| p.a1()).unwrap_or_default());
    wb.clear(sheet, area, true, false, false);
    let cell = wb.sheets[sheet].cell_mut(area.start());
    cell.input = Input::Formula(Arc::new(Formula::new(&text)));
    cell.array = Some(area);
    recalc_all(wb);
    Ok(area)
}

/// `<f t="dataTable" r1 r2 dt2D dtr>` attributes as a TABLE formula.
pub fn table_formula_from_xml(a: &[(String, String)]) -> String {
    let get = |k: &str| a.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let flag = |k: &str| get(k).is_some_and(|v| v == "1" || v == "true");
    let (r1, r2) = (get("r1").unwrap_or(""), get("r2").unwrap_or(""));
    if flag("dt2D") { format!("TABLE({r1},{r2})") } else if flag("dtr") { format!("TABLE({r1},)") } else { format!("TABLE(,{r1})") }
}

/// The `<f t="dataTable">` attributes for a TABLE formula (None for other formulas).
pub fn table_xml(f: &str) -> Option<String> {
    let t = f.trim().trim_start_matches('=');
    let inner = t.strip_prefix("TABLE(").or_else(|| t.strip_prefix("table("))?.strip_suffix(')')?;
    let (a, b) = inner.split_once(',')?;
    let (a, b) = (a.trim(), b.trim());
    let ok = |s: &str| s.is_empty() || parse_a1(s).is_some();
    if !ok(a) || !ok(b) { return None; }
    Some(match (a.is_empty(), b.is_empty()) {
        (false, false) => format!(" dt2D=\"1\" dtr=\"1\" r1=\"{a}\" r2=\"{b}\""),
        (false, true) => format!(" dt2D=\"0\" dtr=\"1\" r1=\"{a}\""),
        (true, false) => format!(" dt2D=\"0\" dtr=\"0\" r1=\"{b}\""),
        _ => return None,
    })
}

#[allow(dead_code)]
fn _unused(_: Option<Range>) { let _ = parse_range; }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::parse_a1;
    fn p(s: &str) -> Pos { parse_a1(s).unwrap() }

    #[test]
    fn goal_seek_finds_input() {
        let mut wb = Workbook::new();
        wb.set_input(0, p("A1"), "2");
        wb.set_input(0, p("B1"), "=A1^2+3*A1");
        wb.recalc();
        let r = goal_seek(&mut wb, 0, p("B1"), 40.0, p("A1"), 100, 0.001).unwrap();
        assert!(r.found, "{r:?}");
        assert!((r.x - 5.0).abs() < 1e-3 || (r.x + 8.0).abs() < 1e-3, "{r:?}");
        assert!((wb.sheets[0].value(p("B1")).to_num().unwrap() - 40.0).abs() < 0.001);
        // A loan payment: the rate giving a payment of -500.
        wb.set_input(0, p("C1"), "0.01");
        wb.set_input(0, p("D1"), "=PMT(C1/12,60,25000)");
        wb.recalc();
        let r = goal_seek(&mut wb, 0, p("D1"), -500.0, p("C1"), 100, 1e-6).unwrap();
        assert!(r.found, "{r:?}");
        assert!((r.x - 0.07420).abs() < 1e-3, "{r:?}");
        assert!(goal_seek(&mut wb, 0, p("A1"), 1.0, p("C1"), 10, 0.001).is_err());
    }

    #[test]
    fn one_and_two_variable_tables() {
        let mut wb = Workbook::new();
        wb.set_input(0, p("A1"), "3"); // input
        wb.set_input(0, p("B1"), "4"); // second input
        // One variable, column oriented: values in C3:C5, formulas in D2:E2.
        for (i, v) in ["1", "2", "3"].iter().enumerate() { wb.set_input(0, Pos::new(2 + i as u32, 2), v); }
        wb.set_input(0, p("D2"), "=A1*10");
        wb.set_input(0, p("E2"), "=A1+B1");
        wb.recalc();
        let area = create_table(&mut wb, 0, crate::addr::parse_range("C2:E5").unwrap(), None, Some(p("A1"))).unwrap();
        assert_eq!(area, crate::addr::parse_range("D3:E5").unwrap());
        assert_eq!(wb.display(0, p("D3")), "10");
        assert_eq!(wb.display(0, p("D5")), "30");
        assert_eq!(wb.display(0, p("E4")), "6");
        // The input cell keeps its own value.
        assert_eq!(wb.display(0, p("D2")), "30");
        // Two variables: formula at G2, row values H2:I2 (B1), column values G3:G4 (A1).
        wb.set_input(0, p("G2"), "=A1*B1");
        wb.set_input(0, p("H2"), "10"); wb.set_input(0, p("I2"), "20");
        wb.set_input(0, p("G3"), "1"); wb.set_input(0, p("G4"), "2");
        wb.recalc();
        create_table(&mut wb, 0, crate::addr::parse_range("G2:I4").unwrap(), Some(p("B1")), Some(p("A1"))).unwrap();
        assert_eq!(wb.display(0, p("I4")), "40");
        assert_eq!(wb.display(0, p("H3")), "10");
        // Automatic except tables: a change leaves the table alone until a full calculation.
        wb.calc_no_tables = true;
        wb.set_input(0, p("G3"), "5");
        wb.recalc();
        assert_eq!(wb.display(0, p("H3")), "10");
        recalc_all(&mut wb);
        assert_eq!(wb.display(0, p("H3")), "50");
        // Round trip as <f t="dataTable">.
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let c = &back.sheets[0].cells[&p("H3")];
        assert_eq!(c.formula(), Some("TABLE(B1,A1)"));
        assert_eq!(c.array, crate::addr::parse_range("H3:I4"));
        assert_eq!(table_xml("TABLE(,A1)").unwrap(), " dt2D=\"0\" dtr=\"0\" r1=\"A1\"");
    }

    #[test]
    fn scenarios_round_trip_and_summary() {
        let mut wb = Workbook::new();
        wb.set_input(0, p("A1"), "1");
        wb.set_input(0, p("B1"), "=A1*2");
        wb.recalc();
        let list = vec![Scenario { name: "High".into(), cells: vec![(p("A1"), "10".into())], ..Default::default() }, Scenario { name: "Low".into(), cells: vec![(p("A1"), "-1".into())], locked: true, ..Default::default() }];
        set_scenarios(&mut wb, 0, &list, 0);
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        assert_eq!(scenarios(&back, 0), list);
        show_scenario(&mut wb, 0, &list[0]);
        assert_eq!(wb.display(0, p("B1")), "20");
        let s = summary(&mut wb, 0, &[p("B1")]);
        assert_eq!(wb.display(s, p("E3")), "High");
        assert_eq!(wb.display(s, p("F7")), "-2");
    }
}
