//! Cell values: numbers, text, logicals, errors and arrays (what a formula produces).

use std::fmt;
use std::sync::Arc as Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrKind { Div0, Value, Ref, Name, NA, Num, Null, Cycle, Spill, Calc }

impl ErrKind {
    pub const ALL: [ErrKind; 10] = [ErrKind::Div0, ErrKind::Value, ErrKind::Ref, ErrKind::Name, ErrKind::NA, ErrKind::Num, ErrKind::Null, ErrKind::Cycle, ErrKind::Spill, ErrKind::Calc];
    pub fn text(&self) -> &'static str {
        match self {
            ErrKind::Div0 => "#DIV/0!", ErrKind::Value => "#VALUE!", ErrKind::Ref => "#REF!", ErrKind::Name => "#NAME?", ErrKind::NA => "#N/A",
            ErrKind::Num => "#NUM!", ErrKind::Null => "#NULL!", ErrKind::Cycle => "#CYCLE!", ErrKind::Spill => "#SPILL!", ErrKind::Calc => "#CALC!",
        }
    }
    pub fn parse(s: &str) -> Option<ErrKind> { let u = s.to_ascii_uppercase(); ErrKind::ALL.into_iter().find(|e| e.text() == u) }
    /// ERROR.TYPE's number.
    pub fn code(&self) -> f64 {
        match self { ErrKind::Null => 1.0, ErrKind::Div0 => 2.0, ErrKind::Value => 3.0, ErrKind::Ref => 4.0, ErrKind::Name => 5.0, ErrKind::Num => 6.0, ErrKind::NA => 7.0, ErrKind::Calc => 14.0, ErrKind::Spill => 9.0, ErrKind::Cycle => 4.0 }
    }
}

/// A rectangular block of values (an array constant, a range read, or a spilled result).
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix { pub rows: usize, pub cols: usize, pub data: Vec<Value> }

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<Value>) -> Matrix { debug_assert_eq!(rows * cols, data.len()); Matrix { rows, cols, data } }
    pub fn filled(rows: usize, cols: usize, v: Value) -> Matrix { Matrix { rows, cols, data: vec![v; rows * cols] } }
    pub fn get(&self, r: usize, c: usize) -> &Value {
        // A single row or column stretches across the other direction (broadcasting).
        let r = if self.rows == 1 { 0 } else { r };
        let c = if self.cols == 1 { 0 } else { c };
        self.data.get(r * self.cols + c).unwrap_or(&Value::Err(ErrKind::NA))
    }
    pub fn from_rows(rows: Vec<Vec<Value>>) -> Matrix {
        let r = rows.len(); let c = rows.iter().map(|x| x.len()).max().unwrap_or(0);
        let mut data = Vec::with_capacity(r * c);
        for mut row in rows { row.resize(c, Value::Err(ErrKind::NA)); data.extend(row); }
        Matrix { rows: r, cols: c, data }
    }
    pub fn row(&self, r: usize) -> &[Value] { &self.data[r * self.cols..(r + 1) * self.cols] }
    pub fn transpose(&self) -> Matrix {
        let mut d = Vec::with_capacity(self.data.len());
        for c in 0..self.cols { for r in 0..self.rows { d.push(self.data[r * self.cols + c].clone()); } }
        Matrix { rows: self.cols, cols: self.rows, data: d }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Value {
    #[default]
    Empty,
    Num(f64),
    Str(Rc<str>),
    Bool(bool),
    Err(ErrKind),
    Array(Rc<Matrix>),
}

impl Value {
    pub fn str(s: impl AsRef<str>) -> Value { Value::Str(Rc::from(s.as_ref())) }
    pub fn array(m: Matrix) -> Value { if m.rows == 1 && m.cols == 1 { m.data.into_iter().next().unwrap() } else { Value::Array(Rc::new(m)) } }
    pub fn is_empty(&self) -> bool { matches!(self, Value::Empty) }
    pub fn is_err(&self) -> bool { matches!(self, Value::Err(_)) }
    pub fn err(&self) -> Option<ErrKind> { if let Value::Err(e) = self { Some(*e) } else { None } }
    /// The top-left value of an array, or the value itself.
    pub fn scalar(&self) -> Value { match self { Value::Array(m) => m.data.first().cloned().unwrap_or(Value::Empty), v => v.clone() } }

    /// Number coercion as an operator does it ("3" → 3, TRUE → 1, "" → 0 for blanks, "x" → #VALUE!).
    pub fn to_num(&self) -> Result<f64, ErrKind> {
        match self {
            Value::Empty => Ok(0.0),
            Value::Num(n) => Ok(*n),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Str(s) => crate::numfmt::parse_number(s).ok_or(ErrKind::Value),
            Value::Err(e) => Err(*e),
            Value::Array(m) => m.data.first().map_or(Ok(0.0), |v| v.to_num()),
        }
    }
    pub fn to_bool(&self) -> Result<bool, ErrKind> {
        match self {
            Value::Empty => Ok(false),
            Value::Num(n) => Ok(*n != 0.0),
            Value::Bool(b) => Ok(*b),
            Value::Str(s) => match s.to_ascii_uppercase().as_str() { "TRUE" => Ok(true), "FALSE" => Ok(false), _ => Err(ErrKind::Value) },
            Value::Err(e) => Err(*e),
            Value::Array(m) => m.data.first().map_or(Ok(false), |v| v.to_bool()),
        }
    }
    /// Text coercion ("1.5", "TRUE", "").
    pub fn to_text(&self) -> Result<String, ErrKind> {
        match self {
            Value::Empty => Ok(String::new()),
            Value::Num(n) => Ok(fmt_num(*n)),
            Value::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.into()),
            Value::Str(s) => Ok(s.to_string()),
            Value::Err(e) => Err(*e),
            Value::Array(m) => m.data.first().map_or(Ok(String::new()), |v| v.to_text()),
        }
    }
    /// How a value shows with the General format.
    pub fn display(&self) -> String {
        match self { Value::Err(e) => e.text().into(), Value::Array(m) => m.data.first().map(|v| v.display()).unwrap_or_default(), v => v.to_text().unwrap_or_default() }
    }
}

impl fmt::Display for Value { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { f.write_str(&self.display()) } }

/// A number as General shows it: up to 15 significant digits, no trailing zeros, exponent past 1e11.
pub fn fmt_num(n: f64) -> String {
    if !n.is_finite() { return "#NUM!".into(); }
    if n == 0.0 { return "0".into(); }
    let a = n.abs();
    if a >= 1e15 || a < 1e-9 {
        let s = format!("{:.14E}", n);
        let (m, e) = s.split_once('E').unwrap();
        let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
        let e: i32 = e.parse().unwrap_or(0);
        return format!("{m}E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    // Round to 15 significant digits so 0.1+0.2 shows 0.3.
    let digits = 15 - (a.log10().floor() as i32 + 1);
    let s = if digits > 0 { format!("{:.*}", digits.min(20) as usize, n) } else { format!("{:.0}", n) };
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn general() {
        assert_eq!(fmt_num(0.1 + 0.2), "0.3"); assert_eq!(fmt_num(1234.5), "1234.5"); assert_eq!(fmt_num(-3.0), "-3");
        assert_eq!(fmt_num(1e20), "1E+20"); assert_eq!(fmt_num(1.0 / 3.0), "0.333333333333333");
    }
}
