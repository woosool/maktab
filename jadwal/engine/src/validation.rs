//! Data validation (validation.js): what a cell may hold (a list, whole or decimal numbers,
//! dates, times, text length or a custom formula), the input message shown when it's selected
//! and the alert shown when an entry breaks the rule.

use crate::addr::{Pos, Range};
use crate::book::{Input, Sheet, Workbook};
use crate::condformat::Op;
use crate::formula::adjust;
use crate::formula::eval::evaluate_in;
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum DvKind { #[default] Any, Whole, Decimal, List, Date, Time, TextLength, Custom }

impl DvKind {
    pub const ALL: [(DvKind, &'static str, &'static str); 8] = [
        (DvKind::Any, "none", "Any value"), (DvKind::Whole, "whole", "Whole number"), (DvKind::Decimal, "decimal", "Decimal"), (DvKind::List, "list", "List"),
        (DvKind::Date, "date", "Date"), (DvKind::Time, "time", "Time"), (DvKind::TextLength, "textLength", "Text length"), (DvKind::Custom, "custom", "Custom"),
    ];
    pub fn name(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().1 }
    pub fn label(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().2 }
    pub fn parse(s: &str) -> DvKind { Self::ALL.iter().find(|x| x.1 == s).map_or(DvKind::Any, |x| x.0) }
    /// Kinds compared with an operator.
    pub fn has_op(&self) -> bool { matches!(self, DvKind::Whole | DvKind::Decimal | DvKind::Date | DvKind::Time | DvKind::TextLength) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum AlertStyle { #[default] Stop, Warning, Information }

impl AlertStyle {
    pub fn name(&self) -> &'static str { match self { AlertStyle::Stop => "stop", AlertStyle::Warning => "warning", AlertStyle::Information => "information" } }
    pub fn parse(s: &str) -> AlertStyle { match s { "warning" => AlertStyle::Warning, "information" => AlertStyle::Information, _ => AlertStyle::Stop } }
}

/// A validation rule over its ranges. Formulas are stored as files hold them, without "=": a
/// literal list is a quoted string ("a,b,c").
#[derive(Clone, Debug, PartialEq)]
pub struct Validation {
    pub ranges: Vec<Range>,
    pub kind: DvKind,
    pub op: Op,
    pub f1: String,
    pub f2: String,
    pub allow_blank: bool,
    /// Show the in-cell drop-down for a list (files store the opposite, showDropDown="1" hides it).
    pub dropdown: bool,
    pub show_input: bool,
    pub prompt_title: String,
    pub prompt: String,
    pub show_error: bool,
    pub style: AlertStyle,
    pub error_title: String,
    pub error: String,
}

impl Default for Validation {
    fn default() -> Self {
        Validation { ranges: vec![], kind: DvKind::Any, op: Op::Between, f1: String::new(), f2: String::new(), allow_blank: true, dropdown: true, show_input: true,
            prompt_title: String::new(), prompt: String::new(), show_error: true, style: AlertStyle::Stop, error_title: String::new(), error: String::new() }
    }
}

impl Validation {
    pub fn contains(&self, p: Pos) -> bool { self.ranges.iter().any(|r| r.contains(p)) }
    pub fn origin(&self) -> Pos { self.ranges.first().map(|r| r.start()).unwrap_or_default() }
    pub fn sqref(&self) -> String { self.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(" ") }
    /// A literal list's items, when the source is one ("a,b,c").
    pub fn literal_items(&self) -> Option<Vec<String>> {
        let f = self.f1.trim();
        let inner = f.strip_prefix('"')?.strip_suffix('"')?;
        Some(inner.replace("\"\"", "\"").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
    }
    /// The list source as the dialog shows it: "a,b,c" or "=$A$1:$A$5".
    pub fn source_text(&self) -> String { match self.literal_items() { Some(items) if self.kind == DvKind::List => items.join(","), _ => if self.f1.is_empty() { String::new() } else { format!("={}", self.f1) } } }
    /// Sets f1 from what the dialog's Source box holds.
    pub fn set_source(&mut self, text: &str) {
        let t = text.trim();
        self.f1 = match t.strip_prefix('=') { Some(f) => f.trim().to_string(), None => format!("\"{}\"", t.replace('"', "\"\"")) };
    }
    /// Visits the formulas, to rewrite them when references move.
    pub fn each_formula(&mut self, f: &mut dyn FnMut(&mut String)) { f(&mut self.f1); f(&mut self.f2); }
}

/// What to tell someone whose entry breaks a rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Alert { pub style: AlertStyle, pub title: String, pub message: String }

pub const DEFAULT_ERROR: &str = "This value doesn't match the data validation restrictions defined for this cell.";

/// The rule over a cell (the first one listed when they overlap).
pub fn at(s: &Sheet, p: Pos) -> Option<&Validation> { s.validations.iter().find(|v| v.contains(p)) }

fn eval(book: &Workbook, sheet: usize, v: &Validation, p: Pos, f: &str) -> Value {
    let f = f.trim().trim_start_matches('=');
    if f.is_empty() { return Value::Empty; }
    let o = v.origin();
    evaluate_in(book, sheet, p, &adjust::shift(f, p.row as i64 - o.row as i64, p.col as i64 - o.col as i64))
}

/// The items a list rule offers at `p` (None when the cell has no list rule).
pub fn list_items(book: &Workbook, sheet: usize, p: Pos) -> Option<Vec<String>> {
    let v = at(book.sheets.get(sheet)?, p).filter(|v| v.kind == DvKind::List)?;
    if let Some(items) = v.literal_items() { return Some(items); }
    let r = eval(book, sheet, v, p, &v.f1);
    let mut out = vec![];
    let mut push = |x: &Value| if !x.is_empty() { out.push(x.display()); };
    match &r { Value::Array(m) => m.data.iter().for_each(&mut push), x => push(x) }
    Some(out)
}

fn as_num(v: &Value) -> Option<f64> { match v.scalar() { Value::Num(n) => Some(n), Value::Str(s) => s.trim().parse().ok(), Value::Bool(b) => Some(b as u8 as f64), _ => None } }

/// Whether `value` passes the rule at `p`; `book` holds the value already for custom formulas.
fn passes(book: &Workbook, sheet: usize, v: &Validation, p: Pos, value: &Value) -> bool {
    let value = value.scalar();
    if value.is_empty() { return true; }
    let bound = |f: &str| { let x = eval(book, sheet, v, p, f).scalar(); as_num(&x) };
    let cmp = |n: f64| -> bool {
        let (Some(a), b) = (bound(&v.f1), if v.op.two() { bound(&v.f2) } else { Some(0.0) }) else { return false };
        let Some(b) = b else { return false };
        v.op.test(&Value::Num(n), &Value::Num(a), &Value::Num(b))
    };
    match v.kind {
        DvKind::Any => true,
        DvKind::Whole => matches!(value, Value::Num(n) if n.fract() == 0.0 && cmp(n)),
        DvKind::Decimal | DvKind::Date => matches!(value, Value::Num(n) if cmp(n)),
        DvKind::Time => matches!(value, Value::Num(n) if cmp(n.fract())),
        DvKind::TextLength => { let t = value.to_text().unwrap_or_default(); cmp(t.chars().count() as f64) }
        DvKind::List => {
            let Some(items) = list_items(book, sheet, p) else { return true };
            let t = value.display();
            items.iter().any(|i| i.eq_ignore_ascii_case(&t) || (as_num(&Value::str(i)).is_some() && as_num(&Value::str(i)) == as_num(&value)))
        }
        DvKind::Custom => match eval(book, sheet, v, p, &v.f1).scalar() { Value::Bool(b) => b, Value::Num(n) => n != 0.0, _ => false },
    }
}

fn alert(v: &Validation) -> Alert {
    Alert { style: v.style, title: v.error_title.clone(), message: if v.error.is_empty() { DEFAULT_ERROR.into() } else { v.error.clone() } }
}

/// Checks what someone typed into a cell against its rule.
pub fn validate(book: &Workbook, sheet: usize, p: Pos, input: &str) -> Result<(), Alert> {
    let Some(v) = book.sheets.get(sheet).and_then(|s| at(s, p)) else { return Ok(()) };
    if !v.show_error || v.kind == DvKind::Any || input.is_empty() { return Ok(()); }
    let (inp, _) = book.parse_input(input);
    let value = match &inp {
        Input::Empty => return Ok(()),
        Input::Value(x) => x.clone(),
        Input::Formula(f) => evaluate_in(book, sheet, p, &f.text),
    };
    let ok = if v.kind == DvKind::Custom {
        // The formula sees the new value in the cell.
        let mut tmp = Workbook { sheets: book.sheets.clone(), styles: book.styles.clone(), names: book.names.clone(), parts: Default::default(), theme_colors: vec![], ..Workbook::new() };
        tmp.date1904 = book.date1904;
        let c = tmp.sheets[sheet].cells.entry(p).or_default();
        c.input = Input::Value(value.clone()); c.value = value.clone();
        passes(&tmp, sheet, v, p, &value)
    } else { passes(book, sheet, v, p, &value) };
    if ok { Ok(()) } else { Err(alert(v)) }
}

/// Whether a cell's current value passes its rule (true without one).
pub fn is_valid(book: &Workbook, sheet: usize, p: Pos) -> bool {
    let Some(s) = book.sheets.get(sheet) else { return true };
    at(s, p).is_none_or(|v| v.kind == DvKind::Any || passes(book, sheet, v, p, &s.value(p)))
}

/// Cells whose current values break their rules (Circle Invalid Data), at most `limit`.
pub fn invalid_cells(book: &Workbook, sheet: usize, limit: usize) -> Vec<Pos> {
    let s = &book.sheets[sheet];
    let mut out = vec![];
    for v in &s.validations {
        if v.kind == DvKind::Any { continue; }
        for (p, c) in &s.cells {
            if out.len() >= limit { return out; }
            if !c.is_blank() && v.contains(*p) && at(s, *p) == Some(v) && !passes(book, sheet, v, *p, &c.value) { out.push(*p); }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::{parse_a1, parse_range};

    fn p(s: &str) -> Pos { parse_a1(s).unwrap() }
    fn dv(r: &str, kind: DvKind, op: Op, f1: &str, f2: &str) -> Validation { Validation { ranges: vec![parse_range(r).unwrap()], kind, op, f1: f1.into(), f2: f2.into(), ..Default::default() } }

    #[test]
    fn kinds() {
        let mut b = Workbook::new();
        for (a, t) in [("D1", "red"), ("D2", "green"), ("D3", "10")] { b.set_input(0, p(a), t); }
        b.recalc();
        let s = &mut b.sheets[0];
        s.validations.push(dv("A1:A5", DvKind::Whole, Op::Between, "1", "10"));
        s.validations.push(dv("B1", DvKind::List, Op::Between, "\"a, b,c\"", ""));
        s.validations.push(dv("B2", DvKind::List, Op::Between, "$D$1:$D$3", ""));
        s.validations.push(dv("C1", DvKind::TextLength, Op::LessEq, "3", ""));
        s.validations.push(dv("C2:C3", DvKind::Custom, Op::Between, "C2>D3", ""));
        s.validations.push(Validation { style: AlertStyle::Warning, error: "Too big".into(), ..dv("E1", DvKind::Decimal, Op::Less, "D3", "") });
        assert!(validate(&b, 0, p("A2"), "5").is_ok());
        assert!(validate(&b, 0, p("A2"), "5.5").is_err());
        assert!(validate(&b, 0, p("A2"), "11").is_err());
        assert!(validate(&b, 0, p("A2"), "").is_ok());
        assert_eq!(list_items(&b, 0, p("B1")), Some(vec!["a".into(), "b".into(), "c".into()]));
        assert!(validate(&b, 0, p("B1"), "B").is_ok() && validate(&b, 0, p("B1"), "d").is_err());
        assert_eq!(list_items(&b, 0, p("B2")).unwrap(), vec!["red", "green", "10"]);
        assert!(validate(&b, 0, p("B2"), "10").is_ok() && validate(&b, 0, p("B2"), "blue").is_err());
        assert!(validate(&b, 0, p("C1"), "abc").is_ok() && validate(&b, 0, p("C1"), "abcd").is_err());
        // The custom formula sees the typed value; its references move per cell.
        assert!(validate(&b, 0, p("C2"), "11").is_ok() && validate(&b, 0, p("C2"), "9").is_err());
        assert!(validate(&b, 0, p("C3"), "1").is_ok());
        let e = validate(&b, 0, p("E1"), "12").unwrap_err();
        assert_eq!((e.style, e.message.as_str()), (AlertStyle::Warning, "Too big"));
        b.set_input(0, p("A1"), "99"); b.recalc();
        assert_eq!(invalid_cells(&b, 0, 10), vec![p("A1")]);
        let mut v = Validation::default(); v.kind = DvKind::List; v.set_source("x, \"y\"");
        assert_eq!(v.source_text(), "x,\"y\"");
    }
}
