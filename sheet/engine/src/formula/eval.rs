//! The evaluator (formula/evaluator.js): walks a formula's tree against the workbook, reading
//! cells on demand (so a formula's inputs calculate first), with Excel's coercions, array
//! broadcasting, spilled results, LET and LAMBDA, and cycle detection.

use super::parser::{BinOp, Expr, RefKind, Reference};
use crate::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
use crate::book::{Input, Workbook};
use crate::value::{ErrKind, Matrix, Value};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

/// A user function made with LAMBDA.
#[derive(Clone, Debug)]
pub struct Lambda { pub params: Vec<(String, bool)>, pub body: Expr, pub captured: Vec<(String, Arg)> }

/// What an argument evaluates to before a function sees it: a value, a reference (so ROW,
/// OFFSET and SUM can tell a range from an array), several areas, or a function.
#[derive(Clone, Debug)]
pub enum Arg {
    V(Value),
    R(usize, Range),
    Areas(Vec<(usize, Range)>),
    F(Arc<Lambda>),
    /// A built-in named without parentheses (BYROW(A1:C3, SUM)).
    Builtin(String),
    Missing,
}

impl Arg {
    pub fn is_missing(&self) -> bool { matches!(self, Arg::Missing) }
}

enum Slot { Busy, Done(Value) }

/// State shared by one recalculation.
pub struct Calc {
    slots: RefCell<HashMap<(usize, Pos), Slot>>,
    /// Spilled values found during this pass: (sheet, cell) → value, and the anchor of each.
    spill: RefCell<HashMap<(usize, Pos), (Value, Pos)>>,
    /// Spill anchors from the previous pass, to know which formula to calculate before reading
    /// an empty cell.
    prior: HashMap<(usize, Pos), Pos>,
    pub now: f64,
    pub cycles: RefCell<Vec<(usize, Pos)>>,
    /// Iterative calculation: the previous iteration's values, read where a cycle closes.
    pub iter: Option<HashMap<(usize, Pos), Value>>,
}

pub struct Ctx<'a> {
    pub book: &'a Workbook,
    pub sheet: usize,
    pub pos: Pos,
    pub calc: &'a Calc,
    pub scopes: Vec<Vec<(String, Arg)>>,
    pub depth: u32,
}

const MAX_DEPTH: u32 = 400;

impl<'a> Ctx<'a> {
    pub fn new(book: &'a Workbook, calc: &'a Calc, sheet: usize, pos: Pos) -> Ctx<'a> { Ctx { book, sheet, pos, calc, scopes: vec![], depth: 0 } }

    /// A cell's value, calculating its formula first if needed.
    pub fn cell(&self, sheet: usize, p: Pos) -> Value {
        let Some(s) = self.book.sheets.get(sheet) else { return Value::Err(ErrKind::Ref) };
        match s.cells.get(&p) {
            Some(c) if c.is_formula() => {
                if let Some(slot) = self.calc.slots.borrow().get(&(sheet, p)) {
                    return match slot { Slot::Busy => { self.calc.cycles.borrow_mut().push((sheet, p)); if let Some(m) = &self.calc.iter { return m.get(&(sheet, p)).map(|v| v.scalar()).unwrap_or(Value::Num(0.0)); } Value::Err(ErrKind::Cycle) } Slot::Done(v) => v.scalar() };
                }
                compute(self.book, self.calc, sheet, p).scalar()
            }
            Some(c) if !c.is_blank() => c.value.clone(),
            _ => {
                if let Some((v, _)) = self.calc.spill.borrow().get(&(sheet, p)) { return v.clone(); }
                if let Some(a) = self.calc.prior.get(&(sheet, p)) {
                    if !self.calc.slots.borrow().contains_key(&(sheet, *a)) && s.cells.get(a).is_some_and(|c| c.is_formula()) {
                        compute(self.book, self.calc, sheet, *a);
                        if let Some((v, _)) = self.calc.spill.borrow().get(&(sheet, p)) { return v.clone(); }
                    }
                }
                Value::Empty
            }
        }
    }

    /// The used part of a range (whole columns stop at the sheet's last row).
    pub fn clip(&self, sheet: usize, r: Range) -> Range {
        let Some(s) = self.book.sheets.get(sheet) else { return r };
        if r.r1 - r.r0 < 5000 && r.c1 - r.c0 < 500 { return r; }
        let e = s.extent().unwrap_or_default();
        let spill_max = self.calc.spill.borrow().keys().filter(|k| k.0 == sheet).fold(Pos::default(), |m, k| Pos::new(m.row.max(k.1.row), m.col.max(k.1.col)));
        Range { r0: r.r0, c0: r.c0, r1: r.r1.min(e.row.max(spill_max.row)).max(r.r0), c1: r.c1.min(e.col.max(spill_max.col)).max(r.c0) }
    }

    /// A range's values as a matrix (blank cells stay Empty).
    pub fn range_matrix(&self, sheet: usize, r: Range) -> Matrix {
        let r = self.clip(sheet, r);
        let mut data = Vec::with_capacity((r.rows() * r.cols()) as usize);
        for row in r.r0..=r.r1 { for col in r.c0..=r.c1 { data.push(self.cell(sheet, Pos::new(row, col))); } }
        Matrix::new(r.rows() as usize, r.cols() as usize, data)
    }

    /// Calls `f` with each non-blank value of a range, in row order.
    pub fn each_in(&self, sheet: usize, r: Range, f: &mut dyn FnMut(Value)) {
        let Some(s) = self.book.sheets.get(sheet) else { f(Value::Err(ErrKind::Ref)); return };
        let r = self.clip(sheet, r);
        let area = (r.rows() as u64) * (r.cols() as u64);
        if area > 4 * s.cells.len() as u64 + 64 {
            // Sparse: visit stored cells and spilled ones only.
            let mut ps: Vec<Pos> = s.cells.range(Pos::new(r.r0, 0)..=Pos::new(r.r1, MAX_COLS)).map(|(p, _)| *p).filter(|p| r.contains(*p)).collect();
            ps.extend(self.calc.spill.borrow().keys().filter(|k| k.0 == sheet && r.contains(k.1)).map(|k| k.1));
            ps.extend(self.calc.prior.keys().filter(|k| k.0 == sheet && r.contains(k.1)).map(|k| k.1));
            ps.sort(); ps.dedup();
            for p in ps { let v = self.cell(sheet, p); if !v.is_empty() { f(v); } }
            return;
        }
        for row in r.r0..=r.r1 { for col in r.c0..=r.c1 { let v = self.cell(sheet, Pos::new(row, col)); if !v.is_empty() { f(v); } } }
    }

    pub fn sheet_of(&self, r: &Reference) -> Option<usize> {
        match &r.sheet { None => Some(self.sheet), Some(n) => self.book.sheet_index(n) }
    }

    /// An argument's value: references read (a single cell as its value, larger as an array).
    pub fn value_of(&self, a: &Arg) -> Value {
        match a {
            Arg::V(v) => v.clone(),
            Arg::R(s, r) => if r.is_cell() { self.cell(*s, r.start()) } else { Value::array(self.range_matrix(*s, *r)) },
            Arg::Areas(list) => match list.as_slice() { [(s, r)] => self.value_of(&Arg::R(*s, *r)), _ => Value::Err(ErrKind::Value) },
            Arg::F(_) | Arg::Builtin(_) => Value::Err(ErrKind::Calc),
            Arg::Missing => Value::Empty,
        }
    }
    /// An argument as a matrix (a scalar is 1×1).
    pub fn matrix_of(&self, a: &Arg) -> Matrix {
        match a {
            Arg::R(s, r) => self.range_matrix(*s, *r),
            _ => match self.value_of(a) { Value::Array(m) => (*m).clone(), v => Matrix::new(1, 1, vec![v]) },
        }
    }
    /// A scalar from an argument (implicit intersection of a range with this cell's row or column).
    pub fn scalar_of(&self, a: &Arg) -> Value {
        match a {
            Arg::R(s, r) if !r.is_cell() => {
                if r.cols() == 1 && r.contains(Pos::new(self.pos.row, r.c0)) { return self.cell(*s, Pos::new(self.pos.row, r.c0)); }
                if r.rows() == 1 && r.contains(Pos::new(r.r0, self.pos.col)) { return self.cell(*s, Pos::new(r.r0, self.pos.col)); }
                Value::Err(ErrKind::Value)
            }
            _ => self.value_of(a).scalar(),
        }
    }

    pub fn lookup_var(&self, name: &str) -> Option<Arg> {
        for scope in self.scopes.iter().rev() { if let Some((_, a)) = scope.iter().rev().find(|(n, _)| n == name) { return Some(a.clone()); } }
        None
    }

    /// Evaluates to an Arg (keeping references).
    pub fn arg(&mut self, e: &Expr) -> Arg {
        match e {
            Expr::Ref(r) => {
                let Some(s) = self.sheet_of(r) else { return Arg::V(Value::Err(ErrKind::Ref)) };
                if let Some(end) = &r.sheet_end {
                    let Some(e) = self.book.sheet_index(end) else { return Arg::V(Value::Err(ErrKind::Ref)) };
                    let (a, b) = (s.min(e), s.max(e));
                    return Arg::Areas((a..=b).map(|i| (i, r.range())).collect());
                }
                if r.spill {
                    let anchor = Pos::new(r.a.row, r.a.col);
                    self.cell(s, anchor);
                    let sp = self.calc.spill.borrow();
                    let cells: Vec<Pos> = sp.iter().filter(|(k, (_, a))| k.0 == s && *a == anchor).map(|(k, _)| k.1).collect();
                    if cells.is_empty() { return Arg::V(Value::Err(ErrKind::Ref)); }
                    let r1 = cells.iter().map(|p| p.row).max().unwrap(); let c1 = cells.iter().map(|p| p.col).max().unwrap();
                    return Arg::R(s, Range::new(anchor, Pos::new(r1, c1)));
                }
                Arg::R(s, r.range())
            }
            Expr::Name { name, sheet } => self.name(name, sheet.as_deref()),
            Expr::Table { table, spec } => super::functions::table_ref(self, table.as_deref(), spec).unwrap_or(Arg::V(Value::Err(ErrKind::Ref))),
            Expr::Missing => Arg::Missing,
            Expr::Union(items) => {
                let mut areas = Vec::new();
                for it in items { match self.arg(it) { Arg::R(s, r) => areas.push((s, r)), Arg::Areas(a) => areas.extend(a), Arg::V(Value::Err(e)) => return Arg::V(Value::Err(e)), _ => return Arg::V(Value::Err(ErrKind::Value)) } }
                Arg::Areas(areas)
            }
            Expr::Span(a, b) => {
                let (x, y) = (self.arg(a), self.arg(b));
                match (x, y) {
                    (Arg::R(s1, r1), Arg::R(s2, r2)) if s1 == s2 => Arg::R(s1, r1.union(&r2)),
                    (Arg::V(Value::Err(e)), _) | (_, Arg::V(Value::Err(e))) => Arg::V(Value::Err(e)),
                    _ => Arg::V(Value::Err(ErrKind::Value)),
                }
            }
            Expr::Intersect(a, b) => {
                match (self.arg(a), self.arg(b)) {
                    (Arg::R(s1, r1), Arg::R(s2, r2)) if s1 == s2 => {
                        if !r1.intersects(&r2) { return Arg::V(Value::Err(ErrKind::Null)); }
                        Arg::R(s1, Range { r0: r1.r0.max(r2.r0), c0: r1.c0.max(r2.c0), r1: r1.r1.min(r2.r1), c1: r1.c1.min(r2.c1) })
                    }
                    _ => Arg::V(Value::Err(ErrKind::Value)),
                }
            }
            Expr::Call { name, args } => {
                // Functions that return references.
                match name.as_str() {
                    "LAMBDA" => return self.lambda(args),
                    _ => {}
                }
                if let Some(Arg::F(f)) = self.lookup_var(name) { let f = f.clone(); return self.invoke(&f, args); }
                if let Some(a) = super::functions::call_ref(self, name, args) { return a; }
                Arg::V(self.eval(e))
            }
            Expr::Invoke { callee, args } => {
                match self.arg(callee) { Arg::F(f) => self.invoke(&f, args), Arg::V(Value::Err(e)) => Arg::V(Value::Err(e)), _ => Arg::V(Value::Err(ErrKind::Value)) }
            }
            _ => Arg::V(self.eval(e)),
        }
    }

    fn name(&mut self, name: &str, sheet: Option<&str>) -> Arg {
        if sheet.is_none() { if let Some(a) = self.lookup_var(name) { return a; } }
        let scope = match sheet { Some(s) => match self.book.sheet_index(s) { Some(i) => Some(i), None => return Arg::V(Value::Err(ErrKind::Ref)) }, None => Some(self.sheet) };
        let Some(i) = self.book.name_index(name, scope) else {
            if super::functions::exists(name) { return Arg::Builtin(name.to_string()); }
            return Arg::V(Value::Err(ErrKind::Name));
        };
        let def = &self.book.names[i];
        if self.depth > MAX_DEPTH { return Arg::V(Value::Err(ErrKind::Cycle)); }
        let Ok(ast) = super::parser::parse(def.formula.trim_start_matches('=')) else { return Arg::V(Value::Err(ErrKind::Name)) };
        self.depth += 1;
        let a = self.arg(&ast);
        self.depth -= 1;
        a
    }

    fn lambda(&mut self, args: &[Expr]) -> Arg {
        let Some((body, params)) = args.split_last() else { return Arg::V(Value::Err(ErrKind::Value)) };
        let mut ps = Vec::new();
        for p in params {
            match p { Expr::Name { name, sheet: None } => ps.push((name.clone(), false)), Expr::Array(_) | Expr::Missing => return Arg::V(Value::Err(ErrKind::Value)),
                _ => return Arg::V(Value::Err(ErrKind::Value)) }
        }
        let captured = self.scopes.iter().flatten().cloned().collect();
        Arg::F(Arc::new(Lambda { params: ps, body: body.clone(), captured }))
    }

    /// Calls a LAMBDA with argument expressions.
    pub fn invoke(&mut self, f: &Arc<Lambda>, args: &[Expr]) -> Arg {
        let vals: Vec<Arg> = args.iter().map(|a| self.arg(a)).collect();
        self.call_lambda(f, vals)
    }
    pub fn call_lambda(&mut self, f: &Arc<Lambda>, vals: Vec<Arg>) -> Arg {
        if vals.len() > f.params.len() { return Arg::V(Value::Err(ErrKind::Value)); }
        if self.depth > MAX_DEPTH { return Arg::V(Value::Err(ErrKind::Num)); }
        let mut scope = f.captured.clone();
        for (i, (p, _)) in f.params.iter().enumerate() { scope.push((p.clone(), vals.get(i).cloned().unwrap_or(Arg::Missing))); }
        let saved = std::mem::replace(&mut self.scopes, vec![scope]);
        self.depth += 1;
        let out = self.arg(&f.body);
        self.depth -= 1;
        self.scopes = saved;
        out
    }
    /// Calls a function value (LAMBDA or a built-in's name) with values.
    pub fn apply(&mut self, f: &Arg, vals: Vec<Arg>) -> Value {
        match f {
            Arg::F(l) => { let a = self.call_lambda(l, vals); self.value_of(&a) }
            Arg::Builtin(name) => super::functions::call_with(self, name, vals),
            _ => Value::Err(ErrKind::Value),
        }
    }

    /// Evaluates to a value (a range becomes an array).
    pub fn eval(&mut self, e: &Expr) -> Value {
        match e {
            Expr::Num(n) => Value::Num(*n),
            Expr::Str(s) => Value::str(s),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Err(k) => Value::Err(*k),
            Expr::Missing => Value::Empty,
            Expr::Array(rows) => {
                let data: Vec<Vec<Value>> = rows.iter().map(|r| r.iter().map(|x| self.eval(x)).collect()).collect();
                Value::array(Matrix::from_rows(data))
            }
            Expr::Neg(x) => unary(self.eval(x), |n| -n),
            Expr::Plus(x) => self.eval(x),
            Expr::Percent(x) => unary(self.eval(x), |n| n / 100.0),
            Expr::Implicit(x) => { let a = self.arg(x); self.scalar_of(&a) }
            Expr::Binary(op, a, b) => {
                let (x, y) = (self.eval(a), self.eval(b));
                binary(*op, &x, &y)
            }
            Expr::Call { name, args } => {
                if self.depth > MAX_DEPTH { return Value::Err(ErrKind::Num); }
                if let Some(Arg::F(f)) = self.lookup_var(name) { let a = self.invoke(&f.clone(), args); return self.value_of(&a); }
                if name == "LAMBDA" { return Value::Err(ErrKind::Calc); }
                self.depth += 1;
                let v = super::functions::call(self, name, args);
                self.depth -= 1;
                v
            }
            Expr::Table { table, spec } => match super::functions::table_ref(self, table.as_deref(), spec) { Some(a) => self.value_of(&a), None => Value::Err(ErrKind::Ref) },
            _ => { let a = self.arg(e); match a { Arg::F(_) | Arg::Builtin(_) => Value::Err(ErrKind::Calc), a => self.value_of(&a) } }
        }
    }
}

fn unary(v: Value, f: impl Fn(f64) -> f64 + Copy) -> Value {
    match v {
        Value::Array(m) => Value::array(Matrix::new(m.rows, m.cols, m.data.iter().map(|x| unary(x.clone(), f)).collect())),
        v => match v.to_num() { Ok(n) => num(f(n)), Err(e) => Value::Err(e) },
    }
}

/// A number result, #NUM! when it is not finite.
pub fn num(n: f64) -> Value { if n.is_finite() { Value::Num(if n == 0.0 { 0.0 } else { n }) } else { Value::Err(ErrKind::Num) } }

/// Broadcasts `f` over two values (arrays pair up; a single row or column stretches).
pub fn broadcast(a: &Value, b: &Value, f: &dyn Fn(&Value, &Value) -> Value) -> Value {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            let rows = if x.rows == 1 { y.rows } else if y.rows == 1 { x.rows } else { x.rows.max(y.rows) };
            let cols = if x.cols == 1 { y.cols } else if y.cols == 1 { x.cols } else { x.cols.max(y.cols) };
            let mut d = Vec::with_capacity(rows * cols);
            for r in 0..rows { for c in 0..cols {
                let xa = if (x.rows > 1 && r >= x.rows) || (x.cols > 1 && c >= x.cols) { Value::Err(ErrKind::NA) } else { x.get(r, c).clone() };
                let yb = if (y.rows > 1 && r >= y.rows) || (y.cols > 1 && c >= y.cols) { Value::Err(ErrKind::NA) } else { y.get(r, c).clone() };
                d.push(f(&xa, &yb));
            } }
            Value::array(Matrix::new(rows, cols, d))
        }
        (Value::Array(x), y) => Value::array(Matrix::new(x.rows, x.cols, x.data.iter().map(|v| f(v, y)).collect())),
        (x, Value::Array(y)) => Value::array(Matrix::new(y.rows, y.cols, y.data.iter().map(|v| f(x, v)).collect())),
        _ => f(a, b),
    }
}

pub fn binary(op: BinOp, a: &Value, b: &Value) -> Value {
    broadcast(a, b, &|x, y| scalar_binary(op, x, y))
}

fn scalar_binary(op: BinOp, a: &Value, b: &Value) -> Value {
    if let Value::Err(e) = a { return Value::Err(*e); }
    if let Value::Err(e) = b { return Value::Err(*e); }
    match op {
        BinOp::Concat => match (a.to_text(), b.to_text()) { (Ok(x), Ok(y)) => Value::str(x + y.as_str()), (Err(e), _) | (_, Err(e)) => Value::Err(e) },
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
            let o = compare(a, b);
            Value::Bool(match op { BinOp::Eq => o == Ordering::Equal, BinOp::Ne => o != Ordering::Equal, BinOp::Lt => o == Ordering::Less, BinOp::Gt => o == Ordering::Greater, BinOp::Le => o != Ordering::Greater, _ => o != Ordering::Less })
        }
        _ => {
            let (x, y) = match (a.to_num(), b.to_num()) { (Ok(x), Ok(y)) => (x, y), (Err(e), _) | (_, Err(e)) => return Value::Err(e) };
            match op {
                BinOp::Add => num(x + y), BinOp::Sub => num(x - y), BinOp::Mul => num(x * y),
                BinOp::Div => if y == 0.0 { Value::Err(ErrKind::Div0) } else { num(x / y) },
                BinOp::Pow => if x == 0.0 && y == 0.0 { Value::Err(ErrKind::Num) } else if x == 0.0 && y < 0.0 { Value::Err(ErrKind::Div0) } else { num(x.powf(y)) },
                _ => unreachable!(),
            }
        }
    }
}

fn rank(v: &Value) -> u8 { match v { Value::Num(_) | Value::Empty => 0, Value::Str(_) => 1, Value::Bool(_) => 2, _ => 3 } }

/// Excel's comparison: numbers < text < logicals; text without case; a blank equals 0, "" and FALSE.
pub fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Empty, Value::Str(s)) => if s.is_empty() { Ordering::Equal } else { Ordering::Less },
        (Value::Str(s), Value::Empty) => if s.is_empty() { Ordering::Equal } else { Ordering::Greater },
        (Value::Empty, Value::Bool(x)) => false.cmp(x),
        (Value::Bool(x), Value::Empty) => x.cmp(&false),
        (Value::Empty, Value::Empty) => Ordering::Equal,
        (Value::Num(x), Value::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Num(x), Value::Empty) => x.partial_cmp(&0.0).unwrap_or(Ordering::Equal),
        (Value::Empty, Value::Num(y)) => 0.0.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Str(x), Value::Str(y)) => text_cmp(x, y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

pub fn text_cmp(a: &str, b: &str) -> Ordering { a.to_lowercase().cmp(&b.to_lowercase()) }

/// The order SORT and Sort A→Z use (numbers, then text, then logicals, then errors).
pub fn sort_cmp(a: &Value, b: &Value, case_sensitive: bool) -> Ordering {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) if case_sensitive => { let o = text_cmp(x, y); if o == Ordering::Equal { y.cmp(x) } else { o } }
        (Value::Err(_), Value::Err(_)) => Ordering::Equal,
        _ => compare(a, b),
    }
}

/// Calculates one formula cell, records its result (and spill) and returns it.
fn compute(book: &Workbook, calc: &Calc, sheet: usize, p: Pos) -> Value {
    let Some(cell) = book.sheets[sheet].cells.get(&p) else { return Value::Empty };
    let Input::Formula(f) = &cell.input else { return cell.value.clone() };
    calc.slots.borrow_mut().insert((sheet, p), Slot::Busy);
    let mut ctx = Ctx::new(book, calc, sheet, p);
    let mut v = match &f.ast {
        Ok(e) => {
            let a = ctx.arg(e);
            match a { Arg::F(_) | Arg::Builtin(_) => Value::Err(ErrKind::Calc), Arg::R(s, r) if r.is_cell() => ctx.cell(s, r.start()), a => ctx.value_of(&a) }
        }
        Err(_) => Value::Err(ErrKind::Name),
    };
    if let Value::Num(n) = v { if !n.is_finite() { v = Value::Err(ErrKind::Num); } }
    if v == Value::Empty { v = Value::Num(0.0); }
    // A legacy array formula (CSE) shows its result over its range instead of spilling.
    if let (Value::Array(m), Some(area)) = (&v, cell.array) {
        let mut sp = calc.spill.borrow_mut();
        for row in area.r0..=area.r1 { for col in area.c0..=area.c1 {
            let (i, j) = ((row - area.r0) as usize, (col - area.c0) as usize);
            let x = if (m.rows > 1 && i >= m.rows) || (m.cols > 1 && j >= m.cols) { Value::Err(ErrKind::NA) } else { m.get(i, j).clone() };
            if (row, col) != (p.row, p.col) { sp.insert((sheet, Pos::new(row, col)), (x, p)); }
        } }
    } else if let Value::Array(m) = &v {
        let s = &book.sheets[sheet];
        let (rows, cols) = (m.rows as u32, m.cols as u32);
        let fits = p.row + rows <= MAX_ROWS && p.col + cols <= MAX_COLS;
        let blocked = !fits || (0..rows).any(|i| (0..cols).any(|j| (i, j) != (0, 0) && {
            let q = Pos::new(p.row + i, p.col + j);
            s.cells.get(&q).is_some_and(|c| !c.is_blank()) || calc.spill.borrow().get(&(sheet, q)).is_some_and(|x| x.1 != p) || s.merge_at(q).is_some()
        }));
        if blocked { v = Value::Err(ErrKind::Spill); } else {
            let mut sp = calc.spill.borrow_mut();
            for i in 0..rows { for j in 0..cols { if (i, j) != (0, 0) {
                let x = m.get(i as usize, j as usize).clone();
                sp.insert((sheet, Pos::new(p.row + i, p.col + j)), (if x.is_empty() { Value::Num(0.0) } else { x }, p));
            } } }
        }
    }
    calc.slots.borrow_mut().insert((sheet, p), Slot::Done(v.clone()));
    v
}

/// Recalculates every formula in the workbook and stores the results in the cells.
pub fn recalc(book: &mut Workbook) {
    let now = crate::numfmt::now_serial();
    // Deep chains of formulas recurse; a large stack keeps them safe.
    let results = std::thread::scope(|sc| {
        let b: &Workbook = book;
        std::thread::Builder::new().stack_size(512 << 20).spawn_scoped(sc, move || crate::audit::iterate(b, now, |iter| run(b, now, iter))).unwrap().join().unwrap_or_default()
    });
    let (values, spills) = results;
    for s in &mut book.sheets { s.spill.clear(); s.spill_anchor.clear(); }
    for ((si, p), v) in values { if let Some(c) = book.sheets[si].cells.get_mut(&p) { c.value = v; } }
    for ((si, p), (v, a)) in spills { book.sheets[si].spill.insert(p, v); book.sheets[si].spill_anchor.insert(p, a); }
}

pub(crate) type Results = (Vec<((usize, Pos), Value)>, Vec<((usize, Pos), (Value, Pos))>);

fn run(book: &Workbook, now: f64, iter: Option<HashMap<(usize, Pos), Value>>) -> (Results, bool) {
    let mut prior: HashMap<(usize, Pos), Pos> = book.sheets.iter().enumerate().flat_map(|(i, s)| s.spill_anchor.iter().map(move |(p, a)| ((i, *p), *a))).collect();
    for pass in 0..3 {
        let calc = Calc { slots: RefCell::new(HashMap::new()), spill: RefCell::new(HashMap::new()), prior: prior.clone(), now, cycles: RefCell::new(vec![]), iter: iter.clone() };
        for (si, s) in book.sheets.iter().enumerate() {
            for (p, c) in &s.cells {
                if c.is_formula() && !calc.slots.borrow().contains_key(&(si, *p)) { compute(book, &calc, si, *p); }
            }
        }
        let spills: HashMap<(usize, Pos), (Value, Pos)> = calc.spill.into_inner();
        let anchors: HashMap<(usize, Pos), Pos> = spills.iter().map(|(k, v)| (*k, v.1)).collect();
        // A spill that appeared where an earlier formula already read blanks needs another pass.
        if anchors == prior || pass == 2 {
            let cyc = !calc.cycles.borrow().is_empty();
            let values = calc.slots.into_inner().into_iter().filter_map(|(k, s)| if let Slot::Done(v) = s { Some((k, v)) } else { None }).collect();
            return ((values, spills.into_iter().collect()), cyc);
        }
        prior = anchors;
    }
    unreachable!()
}

/// Evaluates a formula text in the context of a cell without storing anything (the Name
/// Manager, conditional formats, data validation and Evaluate Formula use it).
pub fn evaluate_in(book: &Workbook, sheet: usize, p: Pos, formula: &str) -> Value {
    let Ok(e) = super::parser::parse(formula.trim_start_matches('=')) else { return Value::Err(ErrKind::Name) };
    let prior = book.sheets.iter().enumerate().flat_map(|(i, s)| s.spill_anchor.iter().map(move |(p, a)| ((i, *p), *a))).collect();
    let calc = Calc { slots: RefCell::new(HashMap::new()), spill: RefCell::new(HashMap::new()), prior, now: crate::numfmt::now_serial(), cycles: RefCell::new(vec![]), iter: None };
    // Formulas already calculated keep their values.
    for (si, s) in book.sheets.iter().enumerate() { for (q, c) in &s.cells { if c.is_formula() { calc.slots.borrow_mut().insert((si, *q), Slot::Done(c.value.clone())); } } }
    for (si, s) in book.sheets.iter().enumerate() { for (q, v) in &s.spill { calc.spill.borrow_mut().insert((si, *q), (v.clone(), s.spill_anchor[q])); } }
    let mut ctx = Ctx::new(book, &calc, sheet, p);
    let a = ctx.arg(&e);
    match a { Arg::R(s, r) if r.is_cell() => ctx.cell(s, r.start()), a => ctx.value_of(&a) }
}

/// Calculates one cell from scratch (every formula it depends on recalculated), without storing
/// anything: what-if analysis evaluates a workbook copy with changed inputs this way.
pub fn compute_fresh(book: &Workbook, sheet: usize, p: Pos) -> Value {
    let prior = book.sheets.iter().enumerate().flat_map(|(i, s)| s.spill_anchor.iter().map(move |(p, a)| ((i, *p), *a))).collect();
    let calc = Calc { slots: RefCell::new(HashMap::new()), spill: RefCell::new(HashMap::new()), prior, now: crate::numfmt::now_serial(), cycles: RefCell::new(vec![]), iter: None };
    let c = Ctx::new(book, &calc, sheet, p);
    c.cell(sheet, p)
}

#[allow(dead_code)]
fn kind_of(r: &Reference) -> RefKind { r.kind }
