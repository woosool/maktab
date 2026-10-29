//! The function library (formula/functions.js). Each module lists its functions in a `DEFS`
//! table; a function is one of:
//! - `Scalar`: takes plain values and is applied to each element when given arrays or ranges
//!   (=LEN(A1:A9) spills nine lengths);
//! - `Eager`: takes its arguments evaluated, references kept (SUM, VLOOKUP);
//! - `Lazy`: takes the unevaluated expressions (IF, IFERROR, LET);
//! - `Refs`: returns a reference (OFFSET, INDEX, INDIRECT), usable where a range is expected.

use super::eval::{Arg, Ctx};
use super::parser::Expr;
use crate::value::{ErrKind, Matrix, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

pub mod criteria;
pub mod database;
pub mod dynamic;
pub mod engineering;
pub mod financial;
pub mod matrix;
pub mod misc;
pub mod regex;
pub mod stats;
pub mod info;
pub mod logic;
pub mod lookup;
pub mod math;
pub mod text;
pub mod time;

pub type ScalarFn = fn(&[Value]) -> Value;
pub type EagerFn = fn(&mut Ctx, &[Arg]) -> Value;
pub type LazyFn = fn(&mut Ctx, &[Expr]) -> Value;
pub type RefFn = fn(&mut Ctx, &[Expr]) -> Arg;

#[derive(Clone, Copy)]
pub enum Kind { Scalar(ScalarFn), Eager(EagerFn), Lazy(LazyFn), Refs(RefFn) }

#[derive(Clone, Copy)]
pub struct Def { pub name: &'static str, pub min: usize, pub max: usize, pub kind: Kind }

/// Shorthands for the tables.
pub const fn s(name: &'static str, min: usize, max: usize, f: ScalarFn) -> Def { Def { name, min, max, kind: Kind::Scalar(f) } }
pub const fn e(name: &'static str, min: usize, max: usize, f: EagerFn) -> Def { Def { name, min, max, kind: Kind::Eager(f) } }
pub const fn l(name: &'static str, min: usize, max: usize, f: LazyFn) -> Def { Def { name, min, max, kind: Kind::Lazy(f) } }
pub const fn r(name: &'static str, min: usize, max: usize, f: RefFn) -> Def { Def { name, min, max, kind: Kind::Refs(f) } }
pub const MANY: usize = 255;

fn tables() -> &'static [&'static [Def]] {
    &[math::DEFS, logic::DEFS, text::DEFS, lookup::DEFS, time::DEFS, info::DEFS,
      stats::DEFS, financial::DEFS, engineering::DEFS, matrix::DEFS, database::DEFS, misc::DEFS, dynamic::DEFS, crate::whatif::DEFS]
}

fn registry() -> &'static HashMap<&'static str, Def> {
    static R: OnceLock<HashMap<&'static str, Def>> = OnceLock::new();
    R.get_or_init(|| tables().iter().flat_map(|t| t.iter()).map(|d| (d.name, *d)).collect())
}

pub fn exists(name: &str) -> bool { registry().contains_key(name) }
pub fn get(name: &str) -> Option<Def> { registry().get(name).copied() }
/// Every function name, sorted (for autocomplete and Insert Function).
pub fn names() -> Vec<&'static str> { let mut v: Vec<_> = registry().keys().copied().collect(); v.sort(); v }

/// Volatile functions recalculate on every change.
pub fn is_volatile(name: &str) -> bool { matches!(name, "NOW" | "TODAY" | "RAND" | "RANDBETWEEN" | "RANDARRAY" | "OFFSET" | "INDIRECT" | "INFO" | "CELL") }

pub fn call(ctx: &mut Ctx, name: &str, args: &[Expr]) -> Value {
    let Some(d) = get(name) else { return Value::Err(ErrKind::Name) };
    if args.len() < d.min || args.len() > d.max { return Value::Err(ErrKind::Value); }
    match d.kind {
        Kind::Lazy(f) => f(ctx, args),
        Kind::Refs(f) => { let a = f(ctx, args); ctx.value_of(&a) }
        Kind::Eager(f) => { let a: Vec<Arg> = args.iter().map(|x| ctx.arg(x)).collect(); f(ctx, &a) }
        Kind::Scalar(f) => { let vals: Vec<Arg> = args.iter().map(|x| ctx.arg(x)).collect(); lift(ctx, f, &vals) }
    }
}

/// For functions passed by name (BYROW(A1:C3, SUM)).
pub fn call_with(ctx: &mut Ctx, name: &str, vals: Vec<Arg>) -> Value {
    let Some(d) = get(name) else { return Value::Err(ErrKind::Name) };
    if vals.len() < d.min || vals.len() > d.max { return Value::Err(ErrKind::Value); }
    match d.kind {
        Kind::Eager(f) => f(ctx, &vals),
        Kind::Scalar(f) => lift(ctx, f, &vals),
        Kind::Lazy(_) | Kind::Refs(_) => {
            // Bind the values to names and call with name expressions.
            let names: Vec<String> = (0..vals.len()).map(|i| format!("\u{1}ARG{i}")).collect();
            ctx.scopes.push(names.iter().cloned().zip(vals).collect());
            let exprs: Vec<Expr> = names.into_iter().map(|n| Expr::Name { name: n, sheet: None }).collect();
            let v = call(ctx, name, &exprs);
            ctx.scopes.pop();
            v
        }
    }
}

pub fn call_ref(ctx: &mut Ctx, name: &str, args: &[Expr]) -> Option<Arg> {
    let d = get(name)?;
    if let Kind::Refs(f) = d.kind { if args.len() >= d.min && args.len() <= d.max { return Some(f(ctx, args)); } }
    None
}

/// Applies a scalar function, element by element when any argument is an array.
fn lift(ctx: &mut Ctx, f: ScalarFn, args: &[Arg]) -> Value {
    let vals: Vec<Value> = args.iter().map(|a| if a.is_missing() { Value::Empty } else { ctx.value_of(a) }).collect();
    let (mut rows, mut cols) = (0usize, 0usize);
    for v in &vals { if let Value::Array(m) = v { rows = rows.max(m.rows); cols = cols.max(m.cols); } }
    if rows == 0 { return f(&vals); }
    let mut out = Vec::with_capacity(rows * cols);
    let mut buf = vals.clone();
    for r in 0..rows { for c in 0..cols {
        for (i, v) in vals.iter().enumerate() {
            buf[i] = match v { Value::Array(m) => if (m.rows > 1 && r >= m.rows) || (m.cols > 1 && c >= m.cols) { Value::Err(ErrKind::NA) } else { m.get(r, c).clone() }, v => v.clone() };
        }
        out.push(f(&buf));
    } }
    Value::array(Matrix::new(rows, cols, out))
}

pub fn table_ref(ctx: &mut Ctx, table: Option<&str>, spec: &str) -> Option<Arg> { lookup::table_ref(ctx, table, spec) }

// ---- helpers shared by the modules ----

pub fn num(n: f64) -> Value { super::eval::num(n) }
pub fn err(e: ErrKind) -> Value { Value::Err(e) }
pub fn boolv(b: bool) -> Value { Value::Bool(b) }

/// The number in a scalar argument, or its error.
macro_rules! n { ($v:expr) => { match $v.to_num() { Ok(x) => x, Err(e) => return Value::Err(e) } }; }
macro_rules! t { ($v:expr) => { match $v.to_text() { Ok(x) => x, Err(e) => return Value::Err(e) } }; }
macro_rules! b { ($v:expr) => { match $v.to_bool() { Ok(x) => x, Err(e) => return Value::Err(e) } }; }
macro_rules! tryv { ($v:expr) => { match $v { Ok(x) => x, Err(e) => return Value::Err(e) } }; }
pub(crate) use {b, n, t, tryv};

/// An optional scalar argument with a default.
pub fn opt(args: &[Value], i: usize, d: f64) -> Result<f64, ErrKind> {
    match args.get(i) { None | Some(Value::Empty) => Ok(d), Some(v) => v.to_num() }
}

/// Visits every value an argument holds; `direct` tells a value typed into the call (which
/// coerces) from one read through a range or array (which skips text and logicals).
pub fn each(ctx: &Ctx, a: &Arg, f: &mut dyn FnMut(&Value, bool)) {
    match a {
        Arg::R(s, r) => ctx.each_in(*s, *r, &mut |v| f(&v, false)),
        Arg::Areas(list) => for (s, r) in list { ctx.each_in(*s, *r, &mut |v| f(&v, false)); },
        Arg::V(Value::Array(m)) => for v in &m.data { f(v, false); },
        Arg::V(v) => f(v, true),
        Arg::Missing => {}
        _ => f(&Value::Err(ErrKind::Value), true),
    }
}

/// The numbers among the arguments, as SUM reads them.
pub fn numbers(ctx: &Ctx, args: &[Arg]) -> Result<Vec<f64>, ErrKind> {
    let mut out = Vec::new(); let mut bad = None;
    for a in args {
        each(ctx, a, &mut |v, direct| {
            if bad.is_some() { return; }
            match v {
                Value::Num(n) => out.push(*n),
                Value::Err(e) => bad = Some(*e),
                Value::Bool(b) if direct => out.push(if *b { 1.0 } else { 0.0 }),
                Value::Str(_) if direct => match v.to_num() { Ok(n) => out.push(n), Err(e) => bad = Some(e) },
                _ => {}
            }
        });
    }
    match bad { Some(e) => Err(e), None => Ok(out) }
}

/// As `numbers`, but text and logicals in ranges count (AVERAGEA, MAXA): text as 0, TRUE as 1.
pub fn numbers_a(ctx: &Ctx, args: &[Arg]) -> Result<Vec<f64>, ErrKind> {
    let mut out = Vec::new(); let mut bad = None;
    for a in args {
        each(ctx, a, &mut |v, direct| {
            if bad.is_some() { return; }
            match v {
                Value::Num(n) => out.push(*n),
                Value::Err(e) => bad = Some(*e),
                Value::Bool(b) => out.push(if *b { 1.0 } else { 0.0 }),
                Value::Str(_) => if direct { match v.to_num() { Ok(n) => out.push(n), Err(e) => bad = Some(e) } } else { out.push(0.0) },
                Value::Empty => {}
                _ => {}
            }
        });
    }
    match bad { Some(e) => Err(e), None => Ok(out) }
}

/// The values of an argument flattened in row order (blanks included for ranges).
pub fn flat(ctx: &Ctx, a: &Arg) -> Vec<Value> {
    match a {
        Arg::Areas(list) => list.iter().flat_map(|(s, r)| ctx.range_matrix(*s, *r).data).collect(),
        _ => ctx.matrix_of(a).data,
    }
}

/// A scalar argument from an Eager function's arguments.
pub fn arg_val(ctx: &Ctx, args: &[Arg], i: usize) -> Value { args.get(i).map(|a| if a.is_missing() { Value::Empty } else { ctx.scalar_of(a) }).unwrap_or(Value::Empty) }
pub fn arg_num(ctx: &Ctx, args: &[Arg], i: usize, d: f64) -> Result<f64, ErrKind> {
    match args.get(i) { None | Some(Arg::Missing) => Ok(d), Some(a) => match ctx.scalar_of(a) { Value::Empty => Ok(d), v => v.to_num() } }
}
pub fn arg_bool(ctx: &Ctx, args: &[Arg], i: usize, d: bool) -> Result<bool, ErrKind> {
    match args.get(i) { None | Some(Arg::Missing) => Ok(d), Some(a) => match ctx.scalar_of(a) { Value::Empty => Ok(d), v => v.to_bool() } }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_stats;
#[cfg(test)]
mod tests_financial;
#[cfg(test)]
mod tests_engineering;
#[cfg(test)]
mod tests_regex;
#[cfg(test)]
mod tests_matrix;
#[cfg(test)]
mod tests_database;
#[cfg(test)]
mod tests_misc;
#[cfg(test)]
mod tests_dynamic;

