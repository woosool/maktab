//! Formula auditing (ui/evaluatedialog.js, error-smarttag.js, createnames.js): the
//! precedents and dependents of a cell for Trace Precedents/Dependents, error checking
//! (errors, numbers stored as text, inconsistent formulas), circular references, step-wise
//! evaluation for Evaluate Formula, Create Names from Selection, and iterative calculation.

use crate::addr::{col_name, quote_sheet, Pos, Range};
use crate::book::{DefinedName, Workbook};
use crate::formula::parser::{parse, BinOp, Expr, RefKind, Reference};
use crate::value::{ErrKind, Value};
use std::collections::{HashMap, HashSet};

// ---- iterative calculation ----------------------------------------------------------------

/// Runs recalculation passes (`run` takes the previous iteration's values and reports whether
/// a cycle closed) until the change is below the workbook's maximum change or the maximum
/// number of iterations is reached. Without iteration, cycles give #CYCLE!.
pub(crate) fn iterate(book: &Workbook, _now: f64, run: impl Fn(Option<HashMap<(usize, Pos), Value>>) -> (crate::formula::eval::Results, bool)) -> crate::formula::eval::Results {
    let Some((n, delta)) = book.iterate else { return run(None).0 };
    let mut prev: HashMap<(usize, Pos), Value> = book.sheets.iter().enumerate()
        .flat_map(|(si, s)| s.cells.iter().filter(|(_, c)| c.is_formula()).map(move |(p, c)| ((si, *p), match &c.value { Value::Err(_) => Value::Num(0.0), v => v.clone() })))
        .collect();
    let mut last = None;
    for _ in 0..n.max(1) {
        let (res, cyc) = run(Some(prev.clone()));
        if !cyc { return res; }
        let next: HashMap<(usize, Pos), Value> = res.0.iter().cloned().collect();
        let change = next.iter().map(|(k, v)| match (v, prev.get(k)) {
            (Value::Num(a), Some(Value::Num(b))) => (a - b).abs(),
            (Value::Num(_), _) => f64::INFINITY,
            _ => 0.0,
        }).fold(0.0, f64::max);
        prev = next;
        last = Some(res);
        if change < delta { break; }
    }
    last.unwrap_or_default()
}

// ---- precedents and dependents ------------------------------------------------------------

fn kids(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Call { args, .. } => args.iter().collect(),
        Expr::Invoke { callee, args } => std::iter::once(&**callee).chain(args.iter()).collect(),
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Implicit(x) => vec![x],
        Expr::Binary(_, a, b) | Expr::Span(a, b) | Expr::Intersect(a, b) => vec![a, b],
        Expr::Union(v) => v.iter().collect(),
        Expr::Array(rows) => rows.iter().flatten().collect(),
        _ => vec![],
    }
}

fn kids_mut(e: &mut Expr) -> Vec<&mut Expr> {
    match e {
        Expr::Call { args, .. } => args.iter_mut().collect(),
        Expr::Invoke { callee, args } => std::iter::once(&mut **callee).chain(args.iter_mut()).collect(),
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Implicit(x) => vec![x],
        Expr::Binary(_, a, b) | Expr::Span(a, b) | Expr::Intersect(a, b) => vec![a, b],
        Expr::Union(v) => v.iter_mut().collect(),
        Expr::Array(rows) => rows.iter_mut().flatten().collect(),
        _ => vec![],
    }
}

fn ref_sheets(book: &Workbook, sheet: usize, r: &Reference) -> Vec<usize> {
    let Some(s) = (match &r.sheet { Some(n) => book.sheet_index(n), None => Some(sheet) }) else { return vec![] };
    match r.sheet_end.as_ref().and_then(|e| book.sheet_index(e)) { Some(e) => (s.min(e)..=s.max(e)).collect(), None => vec![s] }
}

fn collect(book: &Workbook, sheet: usize, e: &Expr, out: &mut Vec<(usize, Range)>, depth: u32) {
    match e {
        Expr::Ref(r) => for s in ref_sheets(book, sheet, r) { out.push((s, r.range())); },
        Expr::Name { name, sheet: qual } if depth < 20 => {
            let scope = match qual { Some(q) => book.sheet_index(q), None => Some(sheet) };
            if let Some(i) = book.name_index(name, scope) {
                if let Ok(ast) = parse(book.names[i].formula.trim_start_matches('=')) { collect(book, scope.unwrap_or(sheet), &ast, out, depth + 1); }
            }
        }
        _ => for k in kids(e) { collect(book, sheet, k, out, depth); },
    }
}

/// The cells and ranges a formula cell reads directly (through names too), without repeats.
pub fn precedents(book: &Workbook, sheet: usize, p: Pos) -> Vec<(usize, Range)> {
    let Some(f) = book.sheets.get(sheet).and_then(|s| s.cells.get(&p)).and_then(|c| c.formula()) else { return vec![] };
    let Ok(ast) = parse(f.trim_start_matches('=')) else { return vec![] };
    let mut out = vec![];
    collect(book, sheet, &ast, &mut out, 0);
    let mut seen = HashSet::new();
    out.retain(|x| seen.insert(*x));
    out
}

/// The formula cells that read a cell directly.
pub fn dependents(book: &Workbook, sheet: usize, p: Pos) -> Vec<(usize, Pos)> {
    let mut out = vec![];
    for (si, s) in book.sheets.iter().enumerate() {
        for (q, c) in &s.cells {
            if c.is_formula() && precedents(book, si, *q).iter().any(|(ps, r)| *ps == sheet && r.contains(p)) { out.push((si, *q)); }
        }
    }
    out.sort_by_key(|(s, q)| (*s, q.row, q.col));
    out
}

/// One tracer arrow: from a precedent (a cell or a range, drawn boxed) to a formula cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrow { pub from: (usize, Range), pub to: (usize, Pos), pub error: bool }

fn has_error(book: &Workbook, sheet: usize, r: Range) -> bool {
    let s = &book.sheets[sheet];
    if (r.rows() as u64) * (r.cols() as u64) <= 4096 { r.iter().any(|q| s.value(q).is_err()) } else { s.cells.iter().any(|(q, c)| r.contains(*q) && c.value.is_err()) }
}

fn formulas_in(book: &Workbook, sheet: usize, r: Range) -> Vec<Pos> {
    let s = &book.sheets[sheet];
    let mut v: Vec<Pos> = if (r.rows() as u64) * (r.cols() as u64) <= 4096 { r.iter().filter(|q| s.cells.get(q).is_some_and(|c| c.is_formula())).collect() } else { s.cells.iter().filter(|(q, c)| r.contains(**q) && c.is_formula()).map(|(q, _)| *q).collect() };
    v.sort_by_key(|q| (q.row, q.col));
    v
}

/// The arrows Trace Precedents (or Dependents) draws after `levels` clicks on a cell: each click
/// goes one level further. Red arrows come from cells showing errors.
pub fn trace(book: &Workbook, sheet: usize, p: Pos, levels: u32, dependents_of: bool) -> Vec<Arrow> {
    let mut out = vec![];
    let mut frontier = vec![(sheet, p)];
    let mut seen: HashSet<(usize, Pos)> = HashSet::from([(sheet, p)]);
    for _ in 0..levels {
        let mut next = vec![];
        for (s, q) in frontier {
            if dependents_of {
                for (ds, dq) in dependents(book, s, q) {
                    out.push(Arrow { from: (s, Range::cell(q)), to: (ds, dq), error: book.sheets[s].value(q).is_err() });
                    if seen.insert((ds, dq)) { next.push((ds, dq)); }
                }
            } else {
                for (ps, r) in precedents(book, s, q) {
                    out.push(Arrow { from: (ps, r), to: (s, q), error: has_error(book, ps, r) });
                    for f in formulas_in(book, ps, r) { if seen.insert((ps, f)) { next.push((ps, f)); } }
                }
            }
        }
        if next.is_empty() && !out.is_empty() { break; }
        frontier = next;
    }
    let mut uniq = vec![];
    for a in out { if !uniq.contains(&a) { uniq.push(a); } }
    uniq
}

/// Trace Error: the arrows from the cells an error comes from, following error precedents back
/// to where the error starts.
pub fn trace_error(book: &Workbook, sheet: usize, p: Pos) -> Vec<Arrow> {
    let mut out = vec![];
    let mut stack = vec![(sheet, p)];
    let mut seen = HashSet::new();
    while let Some((s, q)) = stack.pop() {
        if !seen.insert((s, q)) || seen.len() > 500 { continue; }
        for (ps, r) in precedents(book, s, q) {
            let err = has_error(book, ps, r);
            if !err { continue; }
            out.push(Arrow { from: (ps, r), to: (s, q), error: true });
            for f in formulas_in(book, ps, r) { if book.sheets[ps].value(f).is_err() { stack.push((ps, f)); } }
        }
    }
    out
}

/// The cells that take part in a circular reference (each reaches itself through its
/// precedents), in sheet and row order.
pub fn circular(book: &Workbook) -> Vec<(usize, Pos)> {
    let nodes: Vec<(usize, Pos)> = book.sheets.iter().enumerate().flat_map(|(si, s)| s.cells.iter().filter(|(_, c)| c.is_formula()).map(move |(p, _)| (si, *p))).collect();
    let edges: HashMap<(usize, Pos), Vec<(usize, Pos)>> = nodes.iter().map(|&(s, p)| (((s, p)), precedents(book, s, p).into_iter().flat_map(|(ps, r)| formulas_in(book, ps, r).into_iter().map(move |f| (ps, f))).collect())).collect();
    // Tarjan's strongly connected components, iteratively.
    let mut index: HashMap<(usize, Pos), (usize, usize)> = HashMap::new();
    let mut on: HashSet<(usize, Pos)> = HashSet::new();
    let mut st: Vec<(usize, Pos)> = vec![];
    let mut out = vec![];
    let mut counter = 0;
    for &root in &nodes {
        if index.contains_key(&root) { continue; }
        let mut work: Vec<((usize, Pos), usize)> = vec![(root, 0)];
        index.insert(root, (counter, counter)); counter += 1; st.push(root); on.insert(root);
        while let Some((v, i)) = work.pop() {
            let es = &edges[&v];
            if i < es.len() {
                work.push((v, i + 1));
                let w = es[i];
                if !index.contains_key(&w) { index.insert(w, (counter, counter)); counter += 1; st.push(w); on.insert(w); work.push((w, 0)); }
                else if on.contains(&w) { let wi = index[&w].0; let e = index.get_mut(&v).unwrap(); e.1 = e.1.min(wi); }
            } else {
                let (vi, vl) = index[&v];
                if let Some(&(u, _)) = work.last() { let e = index.get_mut(&u).unwrap(); e.1 = e.1.min(vl); }
                if vi == vl {
                    let mut comp = vec![];
                    loop { let w = st.pop().unwrap(); on.remove(&w); comp.push(w); if w == v { break; } }
                    if comp.len() > 1 || es.contains(&v) { out.extend(comp); }
                }
            }
        }
    }
    out.sort_by_key(|(s, p)| (*s, p.row, p.col));
    out
}

// ---- error checking -----------------------------------------------------------------------

/// Why a cell gets the green error triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Issue { Error(ErrKind), NumberAsText, Inconsistent }

fn norm(f: &str) -> String { f.trim_start_matches('=').chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase() }

/// The error-checking rule a cell breaks, if any.
pub fn issue(book: &Workbook, sheet: usize, p: Pos) -> Option<Issue> {
    let s = book.sheets.get(sheet)?;
    let c = s.cells.get(&p)?;
    if let Value::Err(e) = &c.value { return Some(Issue::Error(*e)); }
    if let Some(f) = c.formula() {
        let fo = |q: Option<Pos>| q.and_then(|q| s.cells.get(&q)).and_then(|c| c.formula()).map(|t| t.to_string());
        let up = |n: u32| p.row.checked_sub(n).map(|r| Pos::new(r, p.col));
        let left = |n: u32| p.col.checked_sub(n).map(|c| Pos::new(p.row, c));
        let pairs = [(fo(up(1)), fo(Some(Pos::new(p.row + 1, p.col))), (1i64, 0i64)), (fo(left(1)), fo(Some(Pos::new(p.row, p.col + 1))), (0, 1))];
        for (a, b, (dr, dc)) in pairs {
            if let (Some(a), Some(b)) = (a, b) {
                let (a, b) = (norm(&crate::formula::adjust::shift(a.trim_start_matches('='), dr, dc)), norm(&crate::formula::adjust::shift(b.trim_start_matches('='), -dr, -dc)));
                if a == b && a != norm(f) { return Some(Issue::Inconsistent); }
            }
        }
        return None;
    }
    if let crate::book::Input::Value(Value::Str(t)) = &c.input {
        let t = t.trim();
        if !t.is_empty() && t.replace(',', "").parse::<f64>().is_ok() { return Some(Issue::NumberAsText); }
    }
    None
}

/// Every cell of a sheet that breaks an error-checking rule, row by row.
pub fn issues(book: &Workbook, sheet: usize) -> Vec<(Pos, Issue)> {
    let Some(s) = book.sheets.get(sheet) else { return vec![] };
    let mut v: Vec<(Pos, Issue)> = s.cells.keys().filter_map(|p| issue(book, sheet, *p).map(|i| (*p, i))).collect();
    v.sort_by_key(|(p, _)| (p.row, p.col));
    v
}

/// A line about an error value, as the Error Checking dialog shows it.
pub fn describe(i: Issue) -> &'static str {
    match i {
        Issue::Error(ErrKind::Div0) => "Divide by Zero Error: the formula or function divides by zero or by an empty cell.",
        Issue::Error(ErrKind::Name) => "Invalid Name Error: the formula contains unrecognized text (a misspelled function or name).",
        Issue::Error(ErrKind::Ref) => "Invalid Cell Reference Error: the formula refers to cells that were deleted or don't exist.",
        Issue::Error(ErrKind::Value) => "Error in Value: a value used in the formula is of the wrong data type.",
        Issue::Error(ErrKind::NA) => "Value Not Available Error: a value is not available to the formula or function.",
        Issue::Error(ErrKind::Num) => "Invalid Number Error: the formula contains invalid numeric values.",
        Issue::Error(ErrKind::Null) => "Null Error: the ranges in the formula don't intersect.",
        Issue::Error(ErrKind::Cycle) => "Circular Reference: the formula refers to its own cell, directly or indirectly.",
        Issue::Error(ErrKind::Spill) => "Spill Error: the formula's results would overwrite data in the spill range.",
        Issue::Error(_) => "Error: the formula's result is an error.",
        Issue::NumberAsText => "Number Stored as Text: the number in this cell is formatted as text or preceded by an apostrophe.",
        Issue::Inconsistent => "Inconsistent Formula: the formula in this cell differs from the formulas in this area of the sheet.",
    }
}

// ---- Create Names from Selection ----------------------------------------------------------

/// A label made into a valid name: spaces become underscores, other characters are dropped,
/// and a name starting with a digit gets an underscore.
pub fn clean_name(label: &str) -> String {
    let mut s: String = label.trim().chars().map(|c| if c.is_whitespace() || c == '-' { '_' } else { c }).filter(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\')).collect();
    if s.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.') { s.insert(0, '_'); }
    if crate::addr::parse_a1(&s).is_some() || s.eq_ignore_ascii_case("R") || s.eq_ignore_ascii_case("C") { s.push('_'); }
    s
}

fn abs_ref(sheet: &str, r: Range) -> String {
    let c = |p: Pos| format!("${}${}", col_name(p.col), p.row + 1);
    if r.is_cell() { format!("{}!{}", quote_sheet(sheet), c(r.start())) } else { format!("{}!{}:{}", quote_sheet(sheet), c(r.start()), c(r.end())) }
}

/// Create Names from Selection: names from the labels in the top row, left column, bottom
/// row and/or right column of a range, each referring to its row or column of values.
/// Existing workbook names are replaced. Returns how many names were made.
pub fn create_names(book: &mut Workbook, sheet: usize, r: Range, top: bool, left: bool, bottom: bool, right: bool) -> usize {
    let name = book.sheets[sheet].name.clone();
    // The body: the range without the label rows and columns.
    let body = Range { r0: r.r0 + top as u32, r1: r.r1.saturating_sub(bottom as u32), c0: r.c0 + left as u32, c1: r.c1.saturating_sub(right as u32) };
    if body.r0 > body.r1 || body.c0 > body.c1 { return 0; }
    let mut made: Vec<(String, String)> = vec![];
    let label = |book: &Workbook, p: Pos| book.sheets[sheet].value(p).display();
    for (on, row) in [(top, Some(r.r0)), (bottom, Some(r.r1))] { if on { for c in body.c0..=body.c1 { made.push((label(book, Pos::new(row.unwrap(), c)), abs_ref(&name, Range { c0: c, c1: c, ..body }))); } } }
    for (on, col) in [(left, r.c0), (right, r.c1)] { if on { for rr in body.r0..=body.r1 { made.push((label(book, Pos::new(rr, col)), abs_ref(&name, Range { r0: rr, r1: rr, ..body }))); } } }
    let mut n = 0;
    for (label, formula) in made {
        let nm = clean_name(&label);
        if nm.is_empty() || book.valid_name(&nm).is_err() { continue; }
        let def = DefinedName { name: nm.clone(), sheet: None, formula, hidden: false, comment: String::new() };
        match book.names.iter().position(|d| d.sheet.is_none() && d.name.eq_ignore_ascii_case(&nm)) { Some(i) => book.names[i] = def, None => book.names.push(def) }
        n += 1;
    }
    n
}

// ---- Evaluate Formula ---------------------------------------------------------------------

/// One stage of Evaluate Formula: the formula as reduced so far, the part evaluated next
/// (char offsets, None once fully evaluated), and the cell Step In opens for that part.
#[derive(Clone, Debug, PartialEq)]
pub struct Step { pub text: String, pub span: Option<(usize, usize)>, pub step_in: Option<(usize, Pos)> }

fn is_literal(e: &Expr) -> bool {
    match e { Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Err(_) | Expr::Missing => true, Expr::Array(rows) => rows.iter().flatten().all(is_literal), _ => false }
}

const LAZY: &[&str] = &["IF", "IFS", "SWITCH", "IFERROR", "IFNA", "CHOOSE"];

/// The path (child indices) to the next part to evaluate: the first part, left to right and
/// innermost first, whose inputs are all values. References passed straight to a function stay
/// as they are (the function reads the range).
fn next(e: &Expr, arg_of_call: bool) -> Option<Vec<usize>> {
    if is_literal(e) { return None; }
    let here = || Some(vec![]);
    match e {
        Expr::Ref(_) | Expr::Table { .. } | Expr::Name { .. } | Expr::Span(..) | Expr::Union(_) | Expr::Intersect(..) => if arg_of_call { None } else { here() },
        Expr::Call { name, args } => {
            if matches!(name.as_str(), "LET" | "LAMBDA") { return here(); }
            let n = if LAZY.contains(&name.as_str()) { 1 } else { args.len() };
            for (i, a) in args.iter().enumerate().take(n) { if let Some(mut p) = next(a, true) { p.insert(0, i); return Some(p); } }
            here()
        }
        Expr::Invoke { .. } => here(),
        Expr::Implicit(_) => here(),
        _ => {
            for (i, k) in kids(e).into_iter().enumerate() { if let Some(mut p) = next(k, false) { p.insert(0, i); return Some(p); } }
            here()
        }
    }
}

fn at_path<'a>(e: &'a Expr, path: &[usize]) -> &'a Expr { match path.split_first() { None => e, Some((i, rest)) => at_path(kids(e)[*i], rest) } }
fn at_path_mut<'a>(e: &'a mut Expr, path: &[usize]) -> &'a mut Expr { match path.split_first() { None => e, Some((i, rest)) => at_path_mut(kids_mut(e).swap_remove(*i), rest) } }

fn num_text(n: f64) -> String { crate::value::fmt_num(n) }

fn literal(v: &Value) -> Expr {
    match v {
        Value::Empty => Expr::Num(0.0),
        Value::Num(n) => Expr::Num(*n),
        Value::Str(s) => Expr::Str(s.to_string()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Err(e) => Expr::Err(*e),
        Value::Array(m) => Expr::Array((0..m.rows).map(|r| (0..m.cols).map(|c| literal(m.get(r, c))).collect()).collect()),
    }
}

fn prec(op: BinOp) -> u8 {
    match op { BinOp::Pow => 5, BinOp::Mul | BinOp::Div => 4, BinOp::Add | BinOp::Sub => 3, BinOp::Concat => 2, _ => 1 }
}

fn cell_txt(c: &crate::formula::parser::CellRef, kind: RefKind) -> String {
    let col = format!("{}{}", if c.col_abs { "$" } else { "" }, col_name(c.col));
    let row = format!("{}{}", if c.row_abs { "$" } else { "" }, c.row + 1);
    match kind { RefKind::Cols => col, RefKind::Rows => row, _ => format!("{col}{row}") }
}

/// Writes an expression as formula text, recording where `target` lands.
fn print(e: &Expr, target: Option<&Expr>, out: &mut String, span: &mut Option<(usize, usize)>) {
    let start = out.chars().count();
    match e {
        Expr::Num(n) => out.push_str(&num_text(*n)),
        Expr::Str(s) => { out.push('"'); out.push_str(&s.replace('"', "\"\"")); out.push('"'); }
        Expr::Bool(b) => out.push_str(if *b { "TRUE" } else { "FALSE" }),
        Expr::Err(k) => out.push_str(k.text()),
        Expr::Missing => {}
        Expr::Ref(r) => {
            match (&r.sheet, &r.sheet_end) { (Some(s), Some(t)) => { out.push_str(&format!("{}:{}!", quote_sheet(s), quote_sheet(t))); } (Some(s), None) => { out.push_str(&quote_sheet(s)); out.push('!'); } _ => {} }
            out.push_str(&cell_txt(&r.a, r.kind));
            if r.kind != RefKind::Cell { out.push(':'); out.push_str(&cell_txt(&r.b, r.kind)); }
            if r.spill { out.push('#'); }
        }
        Expr::Name { name, sheet } => { if let Some(s) = sheet { out.push_str(&quote_sheet(s)); out.push('!'); } out.push_str(name); }
        Expr::Table { table, spec } => { out.push_str(table.as_deref().unwrap_or("")); if spec.starts_with('[') { out.push_str(spec); } else { out.push('['); out.push_str(spec); out.push(']'); } }
        Expr::Call { name, args } => {
            out.push_str(name); out.push('(');
            for (i, a) in args.iter().enumerate() { if i > 0 { out.push(','); } print(a, target, out, span); }
            out.push(')');
        }
        Expr::Invoke { callee, args } => {
            out.push('('); print(callee, target, out, span); out.push_str(")(");
            for (i, a) in args.iter().enumerate() { if i > 0 { out.push(','); } print(a, target, out, span); }
            out.push(')');
        }
        Expr::Neg(x) | Expr::Plus(x) | Expr::Implicit(x) => {
            out.push(match e { Expr::Neg(_) => '-', Expr::Plus(_) => '+', _ => '@' });
            let paren = matches!(**x, Expr::Binary(..));
            if paren { out.push('('); } print(x, target, out, span); if paren { out.push(')'); }
        }
        Expr::Percent(x) => { let paren = matches!(**x, Expr::Binary(..)); if paren { out.push('('); } print(x, target, out, span); if paren { out.push(')'); } out.push('%'); }
        Expr::Binary(op, a, b) => {
            let p = prec(*op);
            let pa = matches!(&**a, Expr::Binary(o, ..) if prec(*o) < p);
            let pb = matches!(&**b, Expr::Binary(o, ..) if prec(*o) <= p);
            if pa { out.push('('); } print(a, target, out, span); if pa { out.push(')'); }
            out.push_str(op.text());
            if pb { out.push('('); } print(b, target, out, span); if pb { out.push(')'); }
        }
        Expr::Span(a, b) => { print(a, target, out, span); out.push(':'); print(b, target, out, span); }
        Expr::Intersect(a, b) => { print(a, target, out, span); out.push(' '); print(b, target, out, span); }
        Expr::Union(v) => { out.push('('); for (i, a) in v.iter().enumerate() { if i > 0 { out.push(','); } print(a, target, out, span); } out.push(')'); }
        Expr::Array(rows) => {
            out.push('{');
            for (i, r) in rows.iter().enumerate() { if i > 0 { out.push(';'); } for (j, x) in r.iter().enumerate() { if j > 0 { out.push(','); } print(x, target, out, span); } }
            out.push('}');
        }
    }
    if target.is_some_and(|t| std::ptr::eq(t, e)) { *span = Some((start, out.chars().count())); }
}

/// Formula text for an expression.
pub fn to_text(e: &Expr) -> String { let mut s = String::new(); print(e, None, &mut s, &mut None); s }

/// Every stage Evaluate Formula steps through for a formula in a cell: each shows the formula
/// with the next part underlined; the last shows the result.
pub fn evaluation_steps(book: &Workbook, sheet: usize, p: Pos, formula: &str) -> Vec<Step> {
    let Ok(mut tree) = parse(formula.trim_start_matches('=')) else { return vec![Step { text: formula.to_string(), span: None, step_in: None }] };
    let mut steps = vec![];
    for _ in 0..500 {
        let Some(path) = next(&tree, false) else { break };
        let target = at_path(&tree, &path);
        let mut text = String::from("=");
        let mut span = None;
        print(&tree, Some(target), &mut text, &mut span);
        let step_in = match target {
            Expr::Ref(r) if r.kind == RefKind::Cell && r.sheet_end.is_none() => ref_sheets(book, sheet, r).first().copied().map(|s| (s, Pos::new(r.a.row, r.a.col))).filter(|(s, q)| book.sheets[*s].cells.get(q).is_some_and(|c| c.is_formula())),
            _ => None,
        };
        let src = to_text(target);
        let v = crate::formula::eval::evaluate_in(book, sheet, p, &src);
        steps.push(Step { text, span, step_in });
        *at_path_mut(&mut tree, &path) = literal(&v);
    }
    steps.push(Step { text: format!("={}", to_text(&tree)), span: None, step_in: None });
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(cells: &[(&str, &str)]) -> Workbook {
        let mut b = Workbook::new();
        for (a, t) in cells { b.set_input(0, crate::addr::parse_a1(a).unwrap(), t); }
        b.recalc();
        b
    }
    fn a(s: &str) -> Pos { crate::addr::parse_a1(s).unwrap() }

    #[test]
    fn precedents_and_dependents() {
        let mut b = book(&[("A1", "1"), ("A2", "2"), ("B1", "=A1+SUM(A1:A2)"), ("C1", "=B1*2"), ("D1", "=Total")]);
        b.names.push(DefinedName { name: "Total".into(), sheet: None, formula: "Sheet1!$C$1".into(), hidden: false, comment: String::new() });
        assert_eq!(precedents(&b, 0, a("B1")), vec![(0, Range::cell(a("A1"))), (0, crate::addr::parse_range("A1:A2").unwrap())]);
        assert_eq!(precedents(&b, 0, a("D1")), vec![(0, Range::cell(a("C1")))]);
        assert_eq!(dependents(&b, 0, a("A1")), vec![(0, a("B1"))]);
        assert_eq!(dependents(&b, 0, a("C1")), vec![(0, a("D1"))]);
        // One level, then two.
        assert_eq!(trace(&b, 0, a("C1"), 1, false).len(), 1);
        assert_eq!(trace(&b, 0, a("C1"), 2, false).len(), 3);
        assert_eq!(trace(&b, 0, a("A1"), 2, true).iter().map(|x| x.to).collect::<Vec<_>>(), vec![(0, a("B1")), (0, a("C1"))]);
    }

    #[test]
    fn errors_circles_and_issues() {
        let b = book(&[("A1", "0"), ("B1", "=1/A1"), ("C1", "=B1+1"), ("D1", "'12"), ("E1", "=F1"), ("F1", "=E1"), ("A3", "1"), ("A4", "2"), ("A5", "3"), ("B3", "=A3*2"), ("B4", "=A4+7"), ("B5", "=A5*2")]);
        assert_eq!(issue(&b, 0, a("B1")), Some(Issue::Error(ErrKind::Div0)));
        assert_eq!(issue(&b, 0, a("D1")), Some(Issue::NumberAsText));
        assert_eq!(issue(&b, 0, a("B4")), Some(Issue::Inconsistent));
        assert_eq!(issue(&b, 0, a("B3")), None);
        let t = trace_error(&b, 0, a("C1"));
        assert_eq!(t.len(), 1);
        assert!(t[0].error);
        assert_eq!(circular(&b), vec![(0, a("E1")), (0, a("F1"))]);
    }

    #[test]
    fn iterative_calculation() {
        let mut b = book(&[("A1", "=B1+1"), ("B1", "=MIN(A1,10)")]);
        assert_eq!(b.sheets[0].value(a("A1")), Value::Err(ErrKind::Cycle));
        b.iterate = Some((100, 0.001));
        b.recalc();
        assert_eq!(b.sheets[0].value(a("A1")), Value::Num(11.0));
        // Converging: x = x/2 + 1 → 2.
        let mut c = book(&[("A1", "=A1/2+1")]);
        c.iterate = Some((100, 0.0001));
        c.recalc();
        let Value::Num(x) = c.sheets[0].value(a("A1")) else { panic!() };
        assert!((x - 2.0).abs() < 0.001);
    }

    #[test]
    fn create_names_from_selection() {
        let mut b = book(&[("A1", "Name"), ("B1", "Unit Price"), ("A2", "alpha"), ("B2", "3"), ("A3", "beta"), ("B3", "4")]);
        let n = create_names(&mut b, 0, crate::addr::parse_range("A1:B3").unwrap(), true, true, false, false);
        assert_eq!(n, 3);
        let f = |n: &str| b.names.iter().find(|d| d.name == n).map(|d| d.formula.clone());
        assert_eq!(f("Unit_Price").as_deref(), Some("Sheet1!$B$2:$B$3"));
        assert_eq!(f("alpha").as_deref(), Some("Sheet1!$B$2"));
        assert_eq!(clean_name("2024 sales"), "_2024_sales");
    }

    #[test]
    fn evaluation_steps_reduce_left_to_right() {
        let b = book(&[("A1", "2"), ("A2", "3"), ("B1", "=A1*2")]);
        let s = evaluation_steps(&b, 0, a("C1"), "=B1+SUM(A1:A2)*2");
        let texts: Vec<&str> = s.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, vec!["=B1+SUM(A1:A2)*2", "=4+SUM(A1:A2)*2", "=4+5*2", "=4+10", "=14"]);
        assert_eq!(s[0].span, Some((1, 3)));
        assert_eq!(s[0].step_in, Some((0, a("B1"))));
        assert_eq!(s.last().unwrap().span, None);
        let s = evaluation_steps(&b, 0, a("C1"), "=IF(A1>1,\"big\",1/0)");
        assert_eq!(s.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(), vec!["=IF(A1>1,\"big\",1/0)", "=IF(2>1,\"big\",1/0)", "=IF(TRUE,\"big\",1/0)", "=\"big\""]);
    }
}
