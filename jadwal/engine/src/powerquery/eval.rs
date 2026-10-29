//! Evaluating queries: values and tables, the expression language of custom columns and row
//! filters, each step's transformation, and the sources (text/CSV files, other workbooks, web
//! pages, this workbook's tables and names, other queries).

use super::mlang::{BOp, Expr, UOp};
use super::{Agg, AggOp, Join, Op, Query, Source, Split, SplitMode, Ty};
use crate::book::Workbook;
use crate::value::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A value of a query: a date, date-time or time is its serial (the column's type says which).
#[derive(Clone, Debug, PartialEq)]
pub enum V {
    Null,
    Num(f64),
    Text(String),
    Bool(bool),
    /// A cell error: the reason, as Power Query words it.
    Err(String),
    /// A nested table (Merged Queries before it is expanded).
    Table(Arc<Table>),
    List(Arc<Vec<V>>),
}

impl V {
    pub fn text(s: impl Into<String>) -> V { V::Text(s.into()) }
    pub fn is_null(&self) -> bool { matches!(self, V::Null) }
    /// The value as the editor's preview shows it.
    pub fn display(&self, ty: Ty) -> String {
        match self {
            V::Null => "null".into(),
            V::Num(n) => match ty {
                Ty::Date => date_text(*n),
                Ty::DateTime => format!("{} {}", date_text(*n), time_text(*n)),
                Ty::Time => time_text(*n),
                Ty::Percentage => format!("{}%", num_text(n * 100.0)),
                _ => num_text(*n),
            },
            V::Text(s) => s.clone(),
            V::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
            V::Err(_) => "Error".into(),
            V::Table(_) => "Table".into(),
            V::List(_) => "List".into(),
        }
    }
    /// As a cell value.
    pub fn to_cell(&self, ty: Ty) -> Value {
        match self {
            V::Null => Value::Empty,
            V::Num(n) => Value::Num(*n),
            V::Text(s) => Value::str(s),
            V::Bool(b) => Value::Bool(*b),
            V::Err(_) => Value::Err(crate::value::ErrKind::Value),
            v => Value::str(v.display(ty)),
        }
    }
    pub fn from_cell(v: &Value) -> V {
        match v {
            Value::Empty => V::Null,
            Value::Num(n) => V::Num(*n),
            Value::Str(s) => V::Text(s.to_string()),
            Value::Bool(b) => V::Bool(*b),
            Value::Err(e) => V::Err(format!("{e:?}")),
            Value::Array(m) => m.data.first().map(V::from_cell).unwrap_or(V::Null),
        }
    }
    fn truthy(&self) -> bool { matches!(self, V::Bool(true)) }
}

pub fn num_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { let s = format!("{:.10}", n); s.trim_end_matches('0').trim_end_matches('.').to_string() }
}

/// A date serial as the editor writes it (en-US): m/d/yyyy.
pub fn date_text(serial: f64) -> String { let (y, m, d) = crate::date::ymd(serial, false); format!("{m}/{d}/{y}") }
/// A time of day: h:mm:ss AM.
pub fn time_text(serial: f64) -> String {
    let secs = ((serial - serial.floor()) * 86400.0).round() as i64 % 86400;
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    format!("{}:{m:02}:{s:02} {}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" })
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Table { pub columns: Vec<String>, pub types: Vec<Ty>, pub rows: Vec<Vec<V>> }

impl Table {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<V>>) -> Table { let types = vec![Ty::Any; columns.len()]; Table { columns, types, rows } }
    pub fn col(&self, name: &str) -> Result<usize, String> {
        self.columns.iter().position(|c| c == name).ok_or_else(|| format!("The column '{name}' of the table wasn't found."))
    }
    fn cols(&self, names: &[String]) -> Result<Vec<usize>, String> { names.iter().map(|n| self.col(n)).collect() }
    /// Rows from cell values with Column1… names.
    pub fn from_rows(rows: Vec<Vec<V>>) -> Table {
        let n = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let rows = rows.into_iter().map(|mut r| { r.resize(n, V::Null); r }).collect();
        Table::new((1..=n).map(|i| format!("Column{i}")).collect(), rows)
    }
}

// ---- types ----------------------------------------------------------------------------------

fn bankers(n: f64) -> f64 { let r = n.round(); if (n - n.trunc()).abs() == 0.5 && r % 2.0 != 0.0 { r - n.signum() } else { r } }

fn parse_number(t: &str) -> Option<f64> {
    let t = t.trim();
    if let Ok(n) = t.parse::<f64>() { return Some(n); }
    crate::numfmt::parse_input(t, false).filter(|(_, f)| !f.as_deref().is_some_and(is_date_fmt)).map(|x| x.0)
}
fn is_date_fmt(f: &str) -> bool { let l = f.to_ascii_lowercase(); (l.contains('y') || l.contains('d') || l.contains('h')) && !l.contains('#') }
fn parse_date(t: &str) -> Option<f64> {
    let t = t.trim();
    crate::numfmt::parse_input(t, false).filter(|(_, f)| f.as_deref().is_some_and(is_date_fmt)).map(|x| x.0)
}

/// Converts a value as Table.TransformColumnTypes does.
pub fn convert(v: &V, from: Ty, to: Ty) -> V {
    match (v, to) {
        (V::Null | V::Err(_), _) | (_, Ty::Any) => v.clone(),
        (V::Table(_) | V::List(_), _) => V::Err("We cannot convert a value of type Table to that type.".into()),
        (V::Text(s), Ty::Text) => V::Text(s.clone()),
        (V::Num(n), Ty::Text) => V::Text(match from { Ty::Date | Ty::DateTime | Ty::Time => v.display(from), _ => num_text(*n) }),
        (V::Bool(b), Ty::Text) => V::text(if *b { "true" } else { "false" }),
        (_, Ty::Int | Ty::Number | Ty::Currency | Ty::Percentage) => {
            let n = match v {
                V::Num(n) => Some(*n),
                V::Bool(b) => Some(*b as u8 as f64),
                V::Text(s) if s.trim().is_empty() => return V::Null,
                V::Text(s) => match to { Ty::Percentage => s.trim().strip_suffix('%').and_then(parse_number).map(|x| x / 100.0).or_else(|| parse_number(s)), _ => parse_number(s) },
                _ => None,
            };
            match n {
                Some(n) => V::Num(match to { Ty::Int => bankers(n), Ty::Currency => (n * 10000.0).round() / 10000.0, _ => n }),
                None => V::Err("We couldn't convert to Number.".into()),
            }
        }
        (_, Ty::Date | Ty::DateTime | Ty::Time) => {
            let n = match v {
                V::Num(n) => Some(*n),
                V::Text(s) if s.trim().is_empty() => return V::Null,
                V::Text(s) => parse_date(s).or_else(|| (to == Ty::Time).then(|| parse_number(s)).flatten()),
                _ => None,
            };
            match n {
                Some(n) => V::Num(match to { Ty::Date => n.floor(), Ty::Time => n - n.floor(), _ => n }),
                None => V::Err(format!("We couldn't parse the input provided as a {} value.", match to { Ty::Date => "Date", Ty::Time => "Time", _ => "DateTime" })),
            }
        }
        (_, Ty::Logical) => match v {
            V::Bool(b) => V::Bool(*b),
            V::Num(n) => V::Bool(*n != 0.0),
            V::Text(s) => match s.trim().to_ascii_lowercase().as_str() { "" => V::Null, "true" => V::Bool(true), "false" => V::Bool(false), _ => V::Err("We couldn't convert to Logical.".into()) },
            _ => V::Err("We couldn't convert to Logical.".into()),
        },
    }
}

/// Detects a column's type from its first 200 values, as automatic type detection does.
pub fn detect(values: &[&V]) -> Ty {
    let sample: Vec<&V> = values.iter().take(200).copied().filter(|v| !v.is_null() && !matches!(v, V::Err(_)) && !matches!(v, V::Text(s) if s.is_empty())).collect();
    if sample.is_empty() { return Ty::Any; }
    if sample.iter().all(|v| matches!(v, V::Bool(_))) { return Ty::Logical; }
    if sample.iter().all(|v| matches!(v, V::Num(_))) { return if sample.iter().all(|v| matches!(v, V::Num(n) if n.fract() == 0.0)) { Ty::Int } else { Ty::Number }; }
    if sample.iter().all(|v| matches!(v, V::Text(_))) {
        let texts: Vec<&str> = sample.iter().filter_map(|v| if let V::Text(s) = v { Some(s.as_str()) } else { None }).collect();
        if texts.iter().all(|t| t.trim().parse::<i64>().is_ok()) { return Ty::Int; }
        if texts.iter().all(|t| t.trim().parse::<f64>().is_ok()) { return Ty::Number; }
        if texts.iter().all(|t| matches!(t.trim().to_ascii_lowercase().as_str(), "true" | "false")) { return Ty::Logical; }
        if texts.iter().all(|t| parse_date(t).is_some()) { return if texts.iter().all(|t| parse_date(t).is_some_and(|d| d.fract() == 0.0)) { Ty::Date } else { Ty::DateTime }; }
        return Ty::Text;
    }
    Ty::Any
}

// ---- expressions ----------------------------------------------------------------------------

/// What an expression sees: the current row (for [Column] and _), named values.
pub struct Scope<'a> { pub columns: &'a [String], pub types: &'a [Ty], pub row: &'a [V], pub vars: &'a HashMap<String, V> }

fn type_name(v: &V) -> &'static str {
    match v { V::Null => "Null", V::Num(_) => "Number", V::Text(_) => "Text", V::Bool(_) => "Logical", V::Err(_) => "Error", V::Table(_) => "Table", V::List(_) => "List" }
}

fn compare(a: &V, b: &V) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (V::Num(x), V::Num(y)) => x.partial_cmp(y),
        (V::Text(x), V::Text(y)) => Some(x.cmp(y)),
        (V::Bool(x), V::Bool(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

/// Order for sorting: nulls first, then values by kind.
pub fn sort_cmp(a: &V, b: &V) -> std::cmp::Ordering {
    let rank = |v: &V| match v { V::Null => 0, V::Bool(_) => 1, V::Num(_) => 2, V::Text(_) => 3, _ => 4 };
    compare(a, b).unwrap_or_else(|| rank(a).cmp(&rank(b)))
}

fn text_arg(v: &V) -> Result<Option<String>, V> {
    match v { V::Null => Ok(None), V::Text(s) => Ok(Some(s.clone())), V::Err(_) => Err(v.clone()), o => Err(V::Err(format!("We cannot convert the value {} to type Text.", o.display(Ty::Any)))) }
}
fn num_arg(v: &V) -> Result<Option<f64>, V> {
    match v { V::Null => Ok(None), V::Num(n) => Ok(Some(*n)), V::Err(_) => Err(v.clone()), o => Err(V::Err(format!("We cannot convert the value {} to type Number.", quote_val(o)))) }
}
fn quote_val(v: &V) -> String { match v { V::Text(s) => format!("\"{s}\""), o => o.display(Ty::Any) } }

pub fn eval(e: &Expr, s: &Scope) -> V {
    match e {
        Expr::Null => V::Null,
        Expr::Bool(b) => V::Bool(*b),
        Expr::Num(n) => V::Num(*n),
        Expr::Text(t) => V::Text(t.clone()),
        Expr::Field(f) => match s.columns.iter().position(|c| c == f) { Some(i) => s.row.get(i).cloned().unwrap_or(V::Null), None => V::Err(format!("The field '{f}' of the record wasn't found.")) },
        Expr::Ident(n) if n == "_" => V::List(Arc::new(s.row.to_vec())),
        Expr::Ident(n) => s.vars.get(n).cloned().unwrap_or_else(|| V::Err(format!("The name '{n}' wasn't recognized."))),
        Expr::List(v) => V::List(Arc::new(v.iter().map(|x| eval(x, s)).collect())),
        Expr::Unary(UOp::Neg, x) => match eval(x, s) { V::Num(n) => V::Num(-n), V::Null => V::Null, v @ V::Err(_) => v, v => V::Err(format!("We cannot apply operator - to type {}.", type_name(&v))) },
        Expr::Unary(UOp::Not, x) => match eval(x, s) { V::Bool(b) => V::Bool(!b), V::Null => V::Null, v @ V::Err(_) => v, v => V::Err(format!("We cannot apply operator not to type {}.", type_name(&v))) },
        Expr::Binary(op, a, b) => binary(*op, a, b, s),
        Expr::If(c, a, b) => match eval(c, s) {
            V::Bool(true) => eval(a, s),
            V::Bool(false) => eval(b, s),
            v @ V::Err(_) => v,
            v => V::Err(format!("We cannot convert the value {} to type Logical.", v.display(Ty::Any))),
        },
        Expr::Let(binds, body) => {
            let mut vars = s.vars.clone();
            for (n, x) in binds { let v = eval(x, &Scope { vars: &vars, ..*s }); vars.insert(n.clone(), v); }
            eval(body, &Scope { vars: &vars, ..*s })
        }
        Expr::Each(b) | Expr::Paren(b) => eval(b, s),
        Expr::Call(f, args) => match &**f { Expr::Ident(name) => call(name, args, s), _ => V::Err("Only built-in functions can be called here.".into()) },
        Expr::FieldOf(x, f) => match eval(x, s) { V::Table(t) => match t.col(f) { Ok(i) => V::List(Arc::new(t.rows.iter().map(|r| r[i].clone()).collect())), Err(e) => V::Err(e) }, v => V::Err(format!("We cannot apply field access to the type {}.", type_name(&v))) },
        Expr::Item(x, i) => match (eval(x, s), eval(i, s)) { (V::List(l), V::Num(n)) => l.get(n as usize).cloned().unwrap_or_else(|| V::Err("There weren't enough elements in the enumeration to complete the operation.".into())), _ => V::Err("Item access isn't supported here.".into()) },
        Expr::Record(_) | Expr::Lambda(..) | Expr::Type(_) => V::Err("This kind of value isn't supported in a column formula.".into()),
    }
}

fn binary(op: BOp, a: &Expr, b: &Expr, s: &Scope) -> V {
    // and / or short-circuit.
    if matches!(op, BOp::And | BOp::Or) {
        let x = eval(a, s);
        match (&x, op) { (V::Bool(false), BOp::And) => return V::Bool(false), (V::Bool(true), BOp::Or) => return V::Bool(true), (V::Err(_), _) => return x, _ => {} }
        let y = eval(b, s);
        return match (x, y) {
            (_, e @ V::Err(_)) => e,
            (V::Bool(_), V::Bool(q)) => V::Bool(q),
            (V::Null, V::Bool(q)) => if (op == BOp::And) == q { V::Null } else { V::Bool(q) },
            (V::Bool(_) | V::Null, V::Null) => V::Null,
            (x, y) => V::Err(format!("We cannot apply operator {} to types {} and {}.", if op == BOp::And { "and" } else { "or" }, type_name(&x), type_name(&y))),
        };
    }
    let (x, y) = (eval(a, s), eval(b, s));
    if let V::Err(_) = x { return x; }
    if let V::Err(_) = y { return y; }
    use std::cmp::Ordering::*;
    match op {
        BOp::Eq => V::Bool(x == y),
        BOp::Ne => V::Bool(x != y),
        BOp::Lt | BOp::Le | BOp::Gt | BOp::Ge => {
            if x.is_null() || y.is_null() { return V::Null; }
            match compare(&x, &y) {
                Some(o) => V::Bool(match op { BOp::Lt => o == Less, BOp::Le => o != Greater, BOp::Gt => o == Greater, _ => o != Less }),
                None => V::Err(format!("We cannot apply operator {} to types {} and {}.", match op { BOp::Lt => "<", BOp::Le => "<=", BOp::Gt => ">", _ => ">=" }, type_name(&x), type_name(&y))),
            }
        }
        BOp::Concat => match (&x, &y) {
            (V::Null, _) | (_, V::Null) => V::Null,
            (V::Text(p), V::Text(q)) => V::Text(format!("{p}{q}")),
            _ => V::Err(format!("We cannot apply operator & to types {} and {}.", type_name(&x), type_name(&y))),
        },
        BOp::Add | BOp::Sub | BOp::Mul | BOp::Div => match (&x, &y) {
            (V::Null, _) | (_, V::Null) => V::Null,
            (V::Num(p), V::Num(q)) => V::Num(match op { BOp::Add => p + q, BOp::Sub => p - q, BOp::Mul => p * q, _ => if *q == 0.0 { if *p == 0.0 { f64::NAN } else { f64::INFINITY * p.signum() } } else { p / q } }),
            _ => V::Err(format!("We cannot apply operator {} to types {} and {}.", match op { BOp::Add => "+", BOp::Sub => "-", BOp::Mul => "*", _ => "/" }, type_name(&x), type_name(&y))),
        },
        BOp::And | BOp::Or => unreachable!(),
    }
}

/// The functions a custom column or row filter can call.
pub const FUNCTIONS: &[&str] = &["Text.Upper", "Text.Lower", "Text.Proper", "Text.Trim", "Text.Clean", "Text.Length", "Text.Start", "Text.End", "Text.Middle", "Text.Contains", "Text.StartsWith", "Text.EndsWith", "Text.Replace", "Text.From", "Number.Round", "Number.RoundUp", "Number.RoundDown", "Number.Abs", "Number.From", "Number.Mod", "Date.Year", "Date.Month", "Date.Day", "Date.From", "#date", "#datetime", "#time"];

fn call(name: &str, args: &[Expr], s: &Scope) -> V {
    let a: Vec<V> = args.iter().map(|x| eval(x, s)).collect();
    if let Some(e) = a.iter().find(|v| matches!(v, V::Err(_))) { return e.clone(); }
    let arity = |lo: usize, hi: usize| -> Result<(), V> { if a.len() < lo || a.len() > hi { Err(V::Err(format!("{name} was called with {} arguments, but expects {}.", a.len(), if lo == hi { lo.to_string() } else { format!("{lo} to {hi}") }))) } else { Ok(()) } };
    let r: Result<V, V> = (|| {
        Ok(match name {
            "Text.Upper" | "Text.Lower" | "Text.Trim" | "Text.Proper" | "Text.Clean" => {
                arity(1, 2)?;
                match text_arg(&a[0])? {
                    None => V::Null,
                    Some(t) => V::Text(match name {
                        "Text.Upper" => t.to_uppercase(), "Text.Lower" => t.to_lowercase(), "Text.Trim" => t.trim().to_string(),
                        "Text.Clean" => t.chars().filter(|c| !c.is_control()).collect(),
                        _ => { let mut out = String::new(); let mut start = true; for c in t.chars() { if c.is_alphabetic() { if start { out.extend(c.to_uppercase()); } else { out.extend(c.to_lowercase()); } start = false; } else { out.push(c); start = true; } } out }
                    }),
                }
            }
            "Text.Length" => { arity(1, 1)?; text_arg(&a[0])?.map_or(V::Null, |t| V::Num(t.chars().count() as f64)) }
            "Text.Start" | "Text.End" => {
                arity(2, 2)?;
                let (Some(t), Some(n)) = (text_arg(&a[0])?, num_arg(&a[1])?) else { return Ok(V::Null) };
                let c: Vec<char> = t.chars().collect();
                let n = (n.max(0.0) as usize).min(c.len());
                V::Text(if name == "Text.Start" { c[..n].iter().collect() } else { c[c.len() - n..].iter().collect() })
            }
            "Text.Middle" => {
                arity(2, 3)?;
                let (Some(t), Some(st)) = (text_arg(&a[0])?, num_arg(&a[1])?) else { return Ok(V::Null) };
                let c: Vec<char> = t.chars().collect();
                let st = (st.max(0.0) as usize).min(c.len());
                let n = a.get(2).map(num_arg).transpose()?.flatten().map_or(c.len() - st, |n| (n.max(0.0) as usize).min(c.len() - st));
                V::Text(c[st..st + n].iter().collect())
            }
            "Text.Contains" | "Text.StartsWith" | "Text.EndsWith" => {
                arity(2, 3)?;
                let (Some(t), Some(p)) = (text_arg(&a[0])?, text_arg(&a[1])?) else { return Ok(V::Null) };
                let ci = matches!(args.get(2), Some(Expr::Ident(c)) if c == "Comparer.OrdinalIgnoreCase");
                let (t, p) = if ci { (t.to_lowercase(), p.to_lowercase()) } else { (t, p) };
                V::Bool(match name { "Text.Contains" => t.contains(&p), "Text.StartsWith" => t.starts_with(&p), _ => t.ends_with(&p) })
            }
            "Text.Replace" => {
                arity(3, 3)?;
                let (Some(t), Some(o), Some(n)) = (text_arg(&a[0])?, text_arg(&a[1])?, text_arg(&a[2])?) else { return Ok(V::Null) };
                V::Text(if o.is_empty() { t } else { t.replace(&o, &n) })
            }
            "Text.From" => { arity(1, 2)?; match &a[0] { V::Null => V::Null, v => convert(v, Ty::Any, Ty::Text) } }
            "Number.From" => { arity(1, 2)?; match &a[0] { V::Null => V::Null, v => convert(v, Ty::Any, Ty::Number) } }
            "Number.Abs" => { arity(1, 1)?; num_arg(&a[0])?.map_or(V::Null, |n| V::Num(n.abs())) }
            "Number.Mod" => { arity(2, 2)?; match (num_arg(&a[0])?, num_arg(&a[1])?) { (Some(x), Some(y)) => V::Num(x % y), _ => V::Null } }
            "Number.Round" | "Number.RoundUp" | "Number.RoundDown" => {
                arity(1, 3)?;
                let Some(n) = num_arg(&a[0])? else { return Ok(V::Null) };
                let d = a.get(1).map(num_arg).transpose()?.flatten().unwrap_or(0.0);
                let f = 10f64.powi(d as i32);
                V::Num(match name {
                    "Number.RoundUp" => (n * f).ceil() / f,
                    "Number.RoundDown" => (n * f).floor() / f,
                    _ => if matches!(args.get(2), Some(Expr::Ident(m)) if m == "RoundingMode.AwayFromZero") { (n * f).round() / f } else { bankers(n * f) / f },
                })
            }
            "Date.Year" | "Date.Month" | "Date.Day" => {
                arity(1, 1)?;
                let Some(n) = num_arg(&a[0])? else { return Ok(V::Null) };
                let (y, m, d) = crate::date::ymd(n, false);
                V::Num(match name { "Date.Year" => y as f64, "Date.Month" => m as f64, _ => d as f64 })
            }
            "Date.From" => { arity(1, 2)?; match &a[0] { V::Null => V::Null, v => convert(v, Ty::Any, Ty::Date) } }
            "#date" => { arity(3, 3)?; match (num_arg(&a[0])?, num_arg(&a[1])?, num_arg(&a[2])?) { (Some(y), Some(m), Some(d)) => V::Num(crate::date::serial(y as i64, m as i64, d as i64, false)), _ => return Err(V::Err("#date needs numbers.".into())) } }
            "#time" => { arity(3, 3)?; match (num_arg(&a[0])?, num_arg(&a[1])?, num_arg(&a[2])?) { (Some(h), Some(m), Some(sec)) => V::Num((h * 3600.0 + m * 60.0 + sec) / 86400.0), _ => return Err(V::Err("#time needs numbers.".into())) } }
            "#datetime" => {
                arity(6, 6)?;
                let n: Vec<f64> = a.iter().map(|v| num_arg(v).ok().flatten().unwrap_or(0.0)).collect();
                V::Num(crate::date::serial(n[0] as i64, n[1] as i64, n[2] as i64, false) + (n[3] * 3600.0 + n[4] * 60.0 + n[5]) / 86400.0)
            }
            "List.Sum" | "List.Average" | "List.Min" | "List.Max" | "List.Count" => {
                arity(1, 1)?;
                let V::List(l) = &a[0] else { return Err(V::Err(format!("{name} expects a list."))) };
                aggregate(match name { "List.Sum" => AggOp::Sum, "List.Average" => AggOp::Average, "List.Min" => AggOp::Min, "List.Max" => AggOp::Max, _ => AggOp::CountRows }, &l.iter().collect::<Vec<_>>())
            }
            _ => return Err(V::Err(format!("The function '{name}' isn't supported in Jadwal's Power Query."))),
        })
    })();
    r.unwrap_or_else(|e| e)
}

// ---- aggregates -----------------------------------------------------------------------------

pub fn aggregate(op: AggOp, vals: &[&V]) -> V {
    let nums: Vec<f64> = vals.iter().filter_map(|v| if let V::Num(n) = v { Some(*n) } else { None }).collect();
    match op {
        AggOp::CountRows => V::Num(vals.len() as f64),
        AggOp::CountDistinct => { let mut seen: Vec<&V> = vec![]; for v in vals { if !seen.contains(v) { seen.push(v); } } V::Num(seen.len() as f64) }
        AggOp::Sum => if nums.is_empty() { if vals.iter().all(|v| v.is_null()) { V::Null } else { V::Num(0.0) } } else { V::Num(nums.iter().sum()) },
        AggOp::Average => if nums.is_empty() { V::Null } else { V::Num(nums.iter().sum::<f64>() / nums.len() as f64) },
        AggOp::Median => if nums.is_empty() { V::Null } else { let mut v = nums.clone(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); let m = v.len() / 2; V::Num(if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 }) },
        AggOp::Min | AggOp::Max => {
            let mut best: Option<&V> = None;
            for v in vals.iter().copied().filter(|v| !v.is_null()) {
                best = Some(match best { None => v, Some(b) => { let o = sort_cmp(v, b); if (op == AggOp::Min) == (o == std::cmp::Ordering::Less) && o != std::cmp::Ordering::Equal { v } else { b } } });
            }
            best.cloned().unwrap_or(V::Null)
        }
    }
}

// ---- steps ----------------------------------------------------------------------------------

fn unique_name(cols: &[String], base: &str) -> String {
    if !cols.iter().any(|c| c == base) { return base.to_string(); }
    (1..).map(|i| format!("{base}{i}")).find(|n| !cols.iter().any(|c| c == n)).unwrap()
}

/// The result of one step over the previous step's table.
pub fn apply(op: &Op, t: Table, ctx: &mut Ctx) -> Result<Table, String> {
    let mut t = t;
    let empty_vars = HashMap::new();
    match op {
        Op::PromoteHeaders => {
            if t.rows.is_empty() { return Ok(t); }
            let first = t.rows.remove(0);
            let mut names: Vec<String> = vec![];
            for (i, v) in first.iter().enumerate() {
                let n = match v { V::Null => format!("Column{}", i + 1), V::Text(s) if s.is_empty() => format!("Column{}", i + 1), v => v.display(t.types[i]) };
                let n = if names.contains(&n) { unique_name(&names, &format!("{n}_")) } else { n };
                names.push(n);
            }
            t.columns = names;
            t.types = vec![Ty::Any; t.columns.len()];
        }
        Op::DemoteHeaders => {
            let header: Vec<V> = t.columns.iter().map(|c| V::Text(c.clone())).collect();
            t.rows.insert(0, header);
            t.columns = (1..=t.columns.len()).map(|i| format!("Column{i}")).collect();
            t.types = vec![Ty::Any; t.columns.len()];
        }
        Op::ChangeType(list) => {
            for (c, ty) in list {
                let i = t.col(c)?;
                let from = t.types[i];
                for r in &mut t.rows { r[i] = convert(&r[i], from, *ty); }
                t.types[i] = *ty;
            }
        }
        Op::RemoveColumns(cols) => {
            let idx = t.cols(cols)?;
            let keep: Vec<usize> = (0..t.columns.len()).filter(|i| !idx.contains(i)).collect();
            t = pick(t, &keep);
        }
        Op::SelectColumns(cols) | Op::ReorderColumns(cols) => {
            let idx = t.cols(cols)?;
            let keep: Vec<usize> = if matches!(op, Op::ReorderColumns(_)) {
                // Listed columns take the places the listed columns had, in the listed order.
                let mut places: Vec<usize> = idx.clone(); places.sort();
                let mut order: Vec<usize> = (0..t.columns.len()).collect();
                for (p, i) in places.iter().zip(idx.iter()) { order[*p] = *i; }
                order
            } else { idx };
            t = pick(t, &keep);
        }
        Op::RenameColumns(pairs) => {
            for (a, b) in pairs {
                let i = t.col(a)?;
                if t.columns.iter().enumerate().any(|(k, c)| k != i && c == b) { return Err(format!("The field '{b}' already exists in the record.")); }
                t.columns[i] = b.clone();
            }
        }
        Op::FilterRows(e) => {
            let (cols, types) = (t.columns.clone(), t.types.clone());
            let mut keep = vec![];
            for r in t.rows {
                match eval(e, &Scope { columns: &cols, types: &types, row: &r, vars: &empty_vars }) {
                    V::Err(m) => return Err(m),
                    v if v.truthy() => keep.push(r),
                    _ => {}
                }
            }
            t.rows = keep;
        }
        Op::Sort(keys) => {
            let idx: Vec<(usize, bool)> = keys.iter().map(|(c, d)| Ok((t.col(c)?, *d))).collect::<Result<_, String>>()?;
            t.rows.sort_by(|a, b| {
                for (i, desc) in &idx { let o = sort_cmp(&a[*i], &b[*i]); if o != std::cmp::Ordering::Equal { return if *desc { o.reverse() } else { o }; } }
                std::cmp::Ordering::Equal
            });
        }
        Op::ReplaceValue { find, replace, text, columns } => {
            let idx = t.cols(columns)?;
            for r in &mut t.rows {
                for &i in &idx {
                    if *text {
                        if let (V::Text(s), V::Text(f)) = (&r[i], find) {
                            if !f.is_empty() { let rep = match replace { V::Text(x) => x.clone(), V::Null => String::new(), v => v.display(Ty::Any) }; r[i] = V::Text(s.replace(f.as_str(), &rep)); }
                        }
                    } else if &r[i] == find { r[i] = replace.clone(); }
                }
            }
        }
        Op::SplitColumn { column, by, into } => {
            let i = t.col(column)?;
            let n = into.len().max(1);
            let parts: Vec<Vec<V>> = t.rows.iter().map(|r| {
                let s = match &r[i] { V::Null => None, V::Text(s) => Some(s.clone()), v => Some(v.display(t.types[i])) };
                let mut out: Vec<V> = match &s {
                    None => vec![V::Null],
                    Some(s) => match by {
                        Split::Delimiter { delimiter, mode } => {
                            let d = delimiter.as_str();
                            let pieces: Vec<String> = if d.is_empty() { vec![s.clone()] } else {
                                match mode {
                                    SplitMode::Each => s.split(d).map(String::from).collect(),
                                    SplitMode::Left => match s.split_once(d) { Some((a, b)) => vec![a.into(), b.into()], None => vec![s.clone()] },
                                    SplitMode::Right => match s.rsplit_once(d) { Some((a, b)) => vec![a.into(), b.into()], None => vec![s.clone()] },
                                }
                            };
                            pieces.into_iter().map(V::Text).collect()
                        }
                        Split::Positions(pos) => {
                            let c: Vec<char> = s.chars().collect();
                            pos.iter().enumerate().map(|(k, &p)| { let end = pos.get(k + 1).copied().unwrap_or(c.len()).min(c.len()); let p = p.min(c.len()); V::Text(c[p..end.max(p)].iter().collect()) }).collect()
                        }
                    },
                };
                out.resize(n, V::Null);
                // Extra pieces join the last column when splitting once.
                out
            }).collect();
            let mut cols = t.columns.clone();
            cols.splice(i..=i, into.iter().cloned());
            let mut types = t.types.clone();
            types.splice(i..=i, std::iter::repeat_n(Ty::Text, n));
            for (r, p) in t.rows.iter_mut().zip(parts) { r.splice(i..=i, p); }
            t.columns = cols; t.types = types;
        }
        Op::MergeColumns { columns, separator, name } => {
            let idx = t.cols(columns)?;
            let at = *idx.iter().min().unwrap_or(&0);
            for r in &mut t.rows {
                let parts: Vec<String> = idx.iter().filter(|&&i| !r[i].is_null()).map(|&i| match &r[i] { V::Text(s) => s.clone(), v => v.display(Ty::Any) }).collect();
                let merged = V::Text(parts.join(separator));
                let mut nr: Vec<V> = r.iter().enumerate().filter(|(k, _)| !idx.contains(k)).map(|x| x.1.clone()).collect();
                nr.insert(at.min(nr.len()), merged);
                *r = nr;
            }
            let mut cols: Vec<String> = t.columns.iter().enumerate().filter(|(k, _)| !idx.contains(k)).map(|x| x.1.clone()).collect();
            let mut types: Vec<Ty> = t.types.iter().enumerate().filter(|(k, _)| !idx.contains(k)).map(|x| *x.1).collect();
            cols.insert(at.min(cols.len()), name.clone());
            types.insert(at.min(types.len()), Ty::Text);
            t.columns = cols; t.types = types;
        }
        Op::AddColumn { name, expr, ty } => {
            if t.columns.contains(name) { return Err(format!("The field '{name}' already exists in the record.")); }
            let vals: Vec<V> = t.rows.iter().map(|r| eval(expr, &Scope { columns: &t.columns, types: &t.types, row: r, vars: &ctx.vars })).collect();
            for (r, v) in t.rows.iter_mut().zip(vals) { r.push(match ty { Some(ty) => convert(&v, Ty::Any, *ty), None => v }); }
            t.columns.push(name.clone());
            t.types.push(ty.unwrap_or(Ty::Any));
        }
        Op::AddIndex { name, start, step } => {
            if t.columns.contains(name) { return Err(format!("The field '{name}' already exists in the record.")); }
            for (k, r) in t.rows.iter_mut().enumerate() { r.push(V::Num((*start + *step * k as i64) as f64)); }
            t.columns.push(name.clone());
            t.types.push(Ty::Int);
        }
        Op::Group { keys, aggs } => {
            let ki = t.cols(keys)?;
            let ai: Vec<Option<usize>> = aggs.iter().map(|a| if matches!(a.op, AggOp::CountRows | AggOp::CountDistinct) && a.column.is_empty() { Ok(None) } else { t.col(&a.column).map(Some) }).collect::<Result<_, String>>()?;
            let mut groups: Vec<(Vec<V>, Vec<usize>)> = vec![];
            for (r, row) in t.rows.iter().enumerate() {
                let key: Vec<V> = ki.iter().map(|&i| row[i].clone()).collect();
                match groups.iter_mut().find(|g| g.0 == key) { Some(g) => g.1.push(r), None => groups.push((key, vec![r])) }
            }
            let rows: Vec<Vec<V>> = groups.into_iter().map(|(key, members)| {
                let mut out = key;
                for (a, i) in aggs.iter().zip(&ai) {
                    let v = match (a.op, i) {
                        (AggOp::CountRows, _) => V::Num(members.len() as f64),
                        (AggOp::CountDistinct, None) => { let mut seen: Vec<&Vec<V>> = vec![]; for &m in &members { if !seen.contains(&&t.rows[m]) { seen.push(&t.rows[m]); } } V::Num(seen.len() as f64) }
                        (op, Some(i)) => aggregate(op, &members.iter().map(|&m| &t.rows[m][*i]).collect::<Vec<_>>()),
                        (_, None) => V::Null,
                    };
                    out.push(v);
                }
                out
            }).collect();
            let mut cols: Vec<String> = keys.clone();
            let mut types: Vec<Ty> = ki.iter().map(|&i| t.types[i]).collect();
            for (a, i) in aggs.iter().zip(&ai) { cols.push(a.name.clone()); types.push(match a.op { AggOp::CountRows | AggOp::CountDistinct => Ty::Int, AggOp::Min | AggOp::Max => i.map_or(Ty::Any, |i| t.types[i]), _ => Ty::Number }); }
            t = Table { columns: cols, types, rows };
        }
        Op::Pivot { column, values, agg } => {
            let ci = t.col(column)?;
            let vi = t.col(values)?;
            let mut heads: Vec<V> = vec![];
            for r in &t.rows { if !heads.contains(&r[ci]) { heads.push(r[ci].clone()); } }
            let others: Vec<usize> = (0..t.columns.len()).filter(|&i| i != ci && i != vi).collect();
            let mut groups: Vec<(Vec<V>, Vec<Vec<&V>>)> = vec![];
            for r in &t.rows {
                let key: Vec<V> = others.iter().map(|&i| r[i].clone()).collect();
                let h = heads.iter().position(|x| x == &r[ci]).unwrap();
                let g = match groups.iter().position(|g| g.0 == key) { Some(p) => p, None => { groups.push((key, vec![vec![]; heads.len()])); groups.len() - 1 } };
                groups[g].1[h].push(&r[vi]);
            }
            let mut rows = vec![];
            for (key, cells) in groups {
                let mut out = key;
                for c in cells {
                    out.push(match agg {
                        Some(op) => if c.is_empty() { V::Null } else { aggregate(*op, &c) },
                        None => match c.len() { 0 => V::Null, 1 => c[0].clone(), _ => V::Err("There were too many elements in the enumeration to complete the operation.".into()) },
                    });
                }
                rows.push(out);
            }
            let mut cols: Vec<String> = others.iter().map(|&i| t.columns[i].clone()).collect();
            let mut types: Vec<Ty> = others.iter().map(|&i| t.types[i]).collect();
            for h in &heads { cols.push(h.display(t.types[ci])); types.push(if agg.is_some_and(|a| matches!(a, AggOp::CountRows | AggOp::CountDistinct)) { Ty::Int } else { t.types[vi] }); }
            t = Table { columns: cols, types, rows };
        }
        Op::Unpivot { columns, other, attribute, value } => {
            let listed = t.cols(columns)?;
            let piv: Vec<usize> = if *other { (0..t.columns.len()).filter(|i| !listed.contains(i)).collect() } else { listed };
            let keep: Vec<usize> = (0..t.columns.len()).filter(|i| !piv.contains(i)).collect();
            let mut rows = vec![];
            for r in &t.rows {
                for &p in &piv {
                    if r[p].is_null() { continue; }
                    let mut out: Vec<V> = keep.iter().map(|&i| r[i].clone()).collect();
                    out.push(V::Text(t.columns[p].clone()));
                    out.push(r[p].clone());
                    rows.push(out);
                }
            }
            let mut cols: Vec<String> = keep.iter().map(|&i| t.columns[i].clone()).collect();
            let mut types: Vec<Ty> = keep.iter().map(|&i| t.types[i]).collect();
            let vt = piv.iter().map(|&i| t.types[i]).reduce(|a, b| if a == b { a } else { Ty::Any }).unwrap_or(Ty::Any);
            cols.push(attribute.clone()); types.push(Ty::Text);
            cols.push(value.clone()); types.push(vt);
            t = Table { columns: cols, types, rows };
        }
        Op::NestedJoin { keys, other, other_keys, column, kind } => {
            let o = ctx.query_table(other)?;
            let li = t.cols(keys)?;
            let ri = o.cols(other_keys)?;
            if li.len() != ri.len() { return Err("The number of key columns of the two tables must match.".into()); }
            let empty = Table { columns: o.columns.clone(), types: o.types.clone(), rows: vec![] };
            let matches = |row: &Vec<V>| -> Vec<usize> { o.rows.iter().enumerate().filter(|(_, r)| li.iter().zip(&ri).all(|(&a, &b)| row[a] == r[b] && !row[a].is_null())).map(|x| x.0).collect() };
            let mut rows = vec![];
            let mut used = vec![false; o.rows.len()];
            for row in &t.rows {
                let m = matches(row);
                for &k in &m { used[k] = true; }
                let keep = match kind { Join::LeftOuter | Join::FullOuter => true, Join::Inner | Join::RightOuter => !m.is_empty(), Join::LeftAnti => m.is_empty(), Join::RightAnti => false };
                if keep {
                    let mut out = row.clone();
                    out.push(V::Table(Arc::new(Table { rows: if matches!(kind, Join::LeftAnti) { vec![] } else { m.iter().map(|&k| o.rows[k].clone()).collect() }, ..empty.clone() })));
                    rows.push(out);
                }
            }
            if matches!(kind, Join::RightOuter | Join::FullOuter | Join::RightAnti) {
                for (k, r) in o.rows.iter().enumerate() {
                    if used[k] { continue; }
                    let mut out: Vec<V> = vec![V::Null; t.columns.len()];
                    out.push(V::Table(Arc::new(Table { rows: vec![r.clone()], ..empty.clone() })));
                    rows.push(out);
                }
            }
            if matches!(kind, Join::RightAnti) { rows.retain(|r: &Vec<V>| r[..t.columns.len()].iter().all(|v| v.is_null())); }
            t.columns.push(unique_name(&t.columns, column));
            t.types.push(Ty::Any);
            t.rows = rows;
        }
        Op::ExpandTable { column, fields, names } => {
            let ci = t.col(column)?;
            let mut rows = vec![];
            let mut types = vec![Ty::Any; fields.len()];
            for r in &t.rows {
                let base: Vec<V> = r.iter().enumerate().filter(|(k, _)| *k != ci).map(|x| x.1.clone()).collect();
                let nested: Vec<Vec<V>> = match &r[ci] {
                    V::Table(nt) => {
                        let idx: Vec<Option<usize>> = fields.iter().map(|f| nt.columns.iter().position(|c| c == f)).collect();
                        for (k, i) in idx.iter().enumerate() { if let Some(i) = i { types[k] = nt.types[*i]; } }
                        nt.rows.iter().map(|nr| idx.iter().map(|i| i.map_or(V::Null, |i| nr[i].clone())).collect()).collect()
                    }
                    _ => vec![],
                };
                if nested.is_empty() {
                    let mut out = base.clone();
                    for (k, _) in fields.iter().enumerate() { out.insert(ci + k, V::Null); }
                    rows.push(out);
                } else {
                    for n in nested { let mut out = base.clone(); for (k, v) in n.into_iter().enumerate() { out.insert(ci + k, v); } rows.push(out); }
                }
            }
            let mut cols = t.columns.clone();
            cols.splice(ci..=ci, names.iter().cloned());
            let mut ty = t.types.clone();
            ty.splice(ci..=ci, types);
            t = Table { columns: cols, types: ty, rows };
        }
        Op::Append(others) => {
            for o in others {
                let o = ctx.query_table(o)?;
                for (k, c) in o.columns.iter().enumerate() { if !t.columns.contains(c) { t.columns.push(c.clone()); t.types.push(o.types[k]); for r in &mut t.rows { r.push(V::Null); } } else { let i = t.col(c)?; if t.types[i] != o.types[k] { t.types[i] = Ty::Any; } } }
                for r in &o.rows {
                    let out: Vec<V> = t.columns.iter().map(|c| o.columns.iter().position(|x| x == c).map_or(V::Null, |i| r[i].clone())).collect();
                    t.rows.push(out);
                }
            }
        }
        Op::Distinct(cols) => {
            let idx: Vec<usize> = match cols { Some(c) => t.cols(c)?, None => (0..t.columns.len()).collect() };
            let mut seen: Vec<Vec<V>> = vec![];
            t.rows.retain(|r| {
                let key: Vec<V> = idx.iter().map(|&i| match &r[i] { V::Text(s) => V::Text(s.clone()), v => v.clone() }).collect();
                if seen.contains(&key) { false } else { seen.push(key); true }
            });
        }
        Op::FirstN(n) => t.rows.truncate(*n),
        Op::LastN(n) => { let k = t.rows.len().saturating_sub(*n); t.rows.drain(..k); }
        Op::Skip(n) => { let k = (*n).min(t.rows.len()); t.rows.drain(..k); }
        Op::RemoveLastN(n) => { let k = t.rows.len().saturating_sub(*n); t.rows.truncate(k); }
        Op::FillDown(cols) | Op::FillUp(cols) => {
            let idx = t.cols(cols)?;
            let down = matches!(op, Op::FillDown(_));
            let n = t.rows.len();
            for &i in &idx {
                let mut last = V::Null;
                for k in 0..n {
                    let r = if down { k } else { n - 1 - k };
                    if t.rows[r][i].is_null() { t.rows[r][i] = last.clone(); } else { last = t.rows[r][i].clone(); }
                }
            }
        }
        Op::Transpose => {
            let n = t.rows.len();
            let rows: Vec<Vec<V>> = (0..t.columns.len()).map(|c| (0..n).map(|r| t.rows[r][c].clone()).collect()).collect();
            t = Table::new((1..=n).map(|i| format!("Column{i}")).collect(), rows);
        }
    }
    Ok(t)
}

fn pick(t: Table, keep: &[usize]) -> Table {
    Table {
        columns: keep.iter().map(|&i| t.columns[i].clone()).collect(),
        types: keep.iter().map(|&i| t.types[i]).collect(),
        rows: t.rows.into_iter().map(|r| keep.iter().map(|&i| r[i].clone()).collect()).collect(),
    }
}

// ---- sources and the evaluation context ---------------------------------------------------------

pub type Fetch<'a> = crate::connections::Fetch<'a>;

/// Evaluation state: the workbook (for its tables and names), the queries (for references),
/// a page fetcher, and a cache of evaluated queries.
pub struct Ctx<'a> {
    pub wb: &'a Workbook,
    pub queries: &'a [Query],
    pub fetch: Fetch<'a>,
    pub vars: HashMap<String, V>,
    cache: HashMap<String, Result<Table, String>>,
    stack: Vec<String>,
}

impl<'a> Ctx<'a> {
    pub fn new(wb: &'a Workbook, queries: &'a [Query], fetch: Fetch<'a>) -> Ctx<'a> { Ctx { wb, queries, fetch, vars: HashMap::new(), cache: HashMap::new(), stack: vec![] } }

    /// The final table of another query.
    pub fn query_table(&mut self, name: &str) -> Result<Table, String> {
        let key = name.to_lowercase();
        if let Some(r) = self.cache.get(&key) { return r.clone(); }
        if self.stack.contains(&key) { return Err(format!("A cyclic reference was encountered: the query '{name}' refers to itself.")); }
        let q = self.queries.iter().find(|q| q.name.eq_ignore_ascii_case(name)).ok_or_else(|| format!("The import {name} matches no exports. Did you miss a module reference?"))?.clone();
        self.stack.push(key.clone());
        let r = self.run(&q, None);
        self.stack.pop();
        self.cache.insert(key, r.clone());
        r
    }

    /// A query's table after `upto` steps (None: all of them; Some(0): the source alone).
    pub fn run(&mut self, q: &Query, upto: Option<usize>) -> Result<Table, String> {
        let src = q.source.as_ref().ok_or("This query's formula uses steps Jadwal can't evaluate; it's kept as written.")?;
        let mut t = self.source(src)?;
        let n = upto.unwrap_or(q.steps.len()).min(q.steps.len());
        for s in &q.steps[..n] { t = apply(&s.op, t, self).map_err(|e| format!("{}: {e}", s.name))?; }
        Ok(t)
    }

    /// Every stage of a query: the source, then the table after each step.
    pub fn stages(&mut self, q: &Query) -> Vec<Result<Table, String>> {
        let Some(src) = &q.source else { return vec![Err("This query's formula uses steps Jadwal can't evaluate; it's kept as written.".into())] };
        let mut out = vec![self.source(src)];
        for s in &q.steps {
            let next = match out.last().unwrap() { Ok(t) => apply(&s.op, t.clone(), self), Err(_) => Err("An earlier step has an error.".into()) };
            out.push(next);
        }
        out
    }

    pub fn source(&mut self, s: &Source) -> Result<Table, String> {
        match s {
            Source::Csv { path, delimiter, encoding, columns } => {
                let b = std::fs::read(path).map_err(|e| format!("We couldn't find the file '{path}'. ({e})"))?;
                let enc = match encoding { 1200 => crate::import::Encoding::Utf16Le, 1201 => crate::import::Encoding::Utf16Be, 1252 | 437 => crate::import::Encoding::Windows1252, 28591 => crate::import::Encoding::Latin1, _ => crate::import::Encoding::Utf8 };
                let text = crate::import::decode(&b, enc);
                let d = delimiter.chars().next().unwrap_or(',');
                let o = crate::import::TextOptions { tab: d == '\t', comma: d == ',', semicolon: d == ';', space: d == ' ', other: (!['\t', ',', ';', ' '].contains(&d)).then_some(d), qualifier: Some('"'), ..Default::default() };
                let rows: Vec<Vec<V>> = crate::import::split(&text, &o).into_iter().map(|r| r.into_iter().map(V::Text).collect()).collect();
                let mut t = Table::from_rows(rows);
                if let Some(n) = columns { let n = *n; t.columns = (1..=n).map(|i| format!("Column{i}")).collect(); t.types = vec![Ty::Any; n]; for r in &mut t.rows { r.resize(n, V::Null); } }
                Ok(t)
            }
            Source::Workbook { path, item, kind } => {
                let other = crate::open(std::path::Path::new(path)).map_err(|e| format!("We couldn't open the workbook '{path}'. ({e})"))?;
                let found = crate::import::workbook_items(&other).into_iter().find(|(n, table, _)| n.eq_ignore_ascii_case(item) && (*table == (kind == "Table")))
                    .ok_or_else(|| format!("The key didn't match any rows in the table. ('{item}' of kind {kind} wasn't found in {path}.)"))?;
                let rows: Vec<Vec<V>> = found.2.iter().map(|r| r.iter().map(V::from_cell).collect()).collect();
                Ok(if kind == "Table" { headed(rows) } else { Table::from_rows(rows) })
            }
            Source::Web { url, index } => {
                let html = (self.fetch)(url)?;
                let tables = crate::import::html_tables(&html);
                let t = tables.get(*index).ok_or("There weren't enough elements in the enumeration to complete the operation. (The page has no table at that position.)")?;
                Ok(headed(t.rows.iter().map(|r| r.iter().map(|x| V::Text(x.clone())).collect()).collect()))
            }
            Source::Current { name } => {
                let wb = self.wb;
                if let Some((si, ti)) = wb.find_table(name) {
                    let tb = &wb.sheets[si].tables[ti];
                    let r = tb.range;
                    let data = crate::addr::Range { r0: r.r0 + tb.header_row as u32, r1: r.r1.saturating_sub(tb.totals_row as u32), ..r };
                    let rows: Vec<Vec<V>> = if data.r0 > data.r1 { vec![] } else { crate::consolidate::source(wb, si, data).iter().map(|r| r.iter().map(V::from_cell).collect()).collect() };
                    let mut t = Table::new(tb.columns.iter().map(|c| c.name.clone()).collect(), rows);
                    for r in &mut t.rows { r.resize(t.columns.len(), V::Null); }
                    return Ok(t);
                }
                let n = wb.names.iter().find(|d| d.name.eq_ignore_ascii_case(name)).ok_or_else(|| format!("The key didn't match any rows in the table. (There's no table or name '{name}' in this workbook.)"))?;
                let (sheet, r) = n.formula.trim_start_matches('=').rsplit_once('!').ok_or("The name doesn't refer to a range.")?;
                let si = wb.sheet_index(sheet.trim_matches('\'')).ok_or("The name's sheet is gone.")?;
                let r = crate::addr::parse_range(&r.replace('$', "")).ok_or("The name doesn't refer to a range.")?;
                Ok(Table::from_rows(crate::consolidate::source(wb, si, r).iter().map(|r| r.iter().map(V::from_cell).collect()).collect()))
            }
            Source::Query(name) => self.query_table(name),
        }
    }
}

/// Rows whose first row is the header.
fn headed(mut rows: Vec<Vec<V>>) -> Table {
    if rows.is_empty() { return Table::default(); }
    let head = rows.remove(0);
    let mut names: Vec<String> = vec![];
    for (i, v) in head.iter().enumerate() {
        let n = match v { V::Null => format!("Column{}", i + 1), v => v.display(Ty::Any) };
        names.push(if names.contains(&n) { unique_name(&names, &n) } else { n });
    }
    let n = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(names.len());
    while names.len() < n { names.push(format!("Column{}", names.len() + 1)); }
    for r in &mut rows { r.resize(n, V::Null); }
    Table::new(names, rows)
}

/// Used by Group By's default names and the Merge dialog.
pub fn agg_default_name(a: &Agg) -> String { a.op.label().to_string() }
