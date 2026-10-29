//! AutoFill series (autofill-options.js): what dragging the fill handle continues — numbers and
//! dates by their step, text with a trailing number, weekday and month names — or copies.

use crate::book::{Cell, Input};
use crate::value::Value;

const LISTS: &[&[&str]] = &[
    &["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"],
    &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    &["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
    &["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    &["Q1", "Q2", "Q3", "Q4"],
];

pub enum Plan {
    Copy,
    Linear { first: f64, last: f64, step: f64 },
    /// Text around a number: prefix, first number, last number, step, digits (for zero padding), suffix.
    Numbered { prefix: String, first: i64, last: i64, step: i64, width: usize, suffix: String },
    List { list: usize, first: usize, last: usize, step: i64, case: u8 },
}

fn text_of(c: &Cell) -> Option<String> { if let Input::Value(Value::Str(s)) = &c.input { Some(s.to_string()) } else { None } }
fn num_of(c: &Cell) -> Option<f64> { if let Input::Value(Value::Num(n)) = &c.input { Some(*n) } else { None } }

fn split_number(s: &str) -> Option<(String, i64, usize, String)> {
    let b: Vec<char> = s.chars().collect();
    let end = b.iter().rposition(|c| c.is_ascii_digit())? + 1;
    let mut start = end;
    while start > 0 && b[start - 1].is_ascii_digit() { start -= 1; }
    let digits: String = b[start..end].iter().collect();
    Some((b[..start].iter().collect(), digits.parse().ok()?, digits.len(), b[end..].iter().collect()))
}

fn list_pos(s: &str) -> Option<(usize, usize, u8)> {
    for (li, l) in LISTS.iter().enumerate() {
        if let Some(i) = l.iter().position(|x| x.eq_ignore_ascii_case(s)) {
            let case = if s.chars().all(|c| !c.is_lowercase()) { 2 } else if s.chars().all(|c| !c.is_uppercase()) { 1 } else { 0 };
            return Some((li, i, case));
        }
    }
    None
}

pub fn plan(src: &[Cell], series: bool, _date1904: bool) -> Plan {
    if !series || src.is_empty() { return Plan::Copy; }
    let nums: Vec<f64> = src.iter().filter_map(num_of).collect();
    if nums.len() == src.len() {
        if nums.len() == 1 { return Plan::Copy; }
        let step = (nums[nums.len() - 1] - nums[0]) / (nums.len() - 1) as f64;
        return Plan::Linear { first: nums[0], last: nums[nums.len() - 1], step };
    }
    let texts: Vec<String> = src.iter().filter_map(text_of).collect();
    if texts.len() != src.len() { return Plan::Copy; }
    if let Some(ps) = texts.iter().map(|t| list_pos(t)).collect::<Option<Vec<_>>>() {
        if ps.iter().all(|p| p.0 == ps[0].0) {
            let step = if ps.len() > 1 { ps[1].1 as i64 - ps[0].1 as i64 } else { 1 };
            return Plan::List { list: ps[0].0, first: ps[0].1, last: ps[ps.len() - 1].1, step, case: ps[0].2 };
        }
    }
    if let Some(ps) = texts.iter().map(|t| split_number(t)).collect::<Option<Vec<_>>>() {
        if ps.iter().all(|p| p.0 == ps[0].0 && p.3 == ps[0].3) {
            let step = if ps.len() > 1 { (ps[ps.len() - 1].1 - ps[0].1) / (ps.len() as i64 - 1) } else { 1 };
            return Plan::Numbered { prefix: ps[0].0.clone(), first: ps[0].1, last: ps[ps.len() - 1].1, step, width: ps[0].2, suffix: ps[0].3.clone() };
        }
    }
    Plan::Copy
}

impl Plan {
    /// The value `k` places past the source (negative: before it); None copies the source cell.
    pub fn value(&self, k: i64, _idx: usize) -> Option<Value> {
        match self {
            Plan::Copy => None,
            Plan::Linear { first, last, step } => Some(Value::Num(if k > 0 { last + step * k as f64 } else { first + step * k as f64 })),
            Plan::Numbered { prefix, first, last, step, width, suffix } => {
                let n = if k > 0 { last + step * k } else { first + step * k };
                let body = if n < 0 { n.abs().to_string() } else { format!("{:0w$}", n, w = *width) };
                Some(Value::str(format!("{prefix}{body}{suffix}")))
            }
            Plan::List { list, first, last, step, case } => {
                let l = LISTS[*list]; let len = l.len() as i64;
                let i = if k > 0 { *last as i64 + step * k } else { *first as i64 + step * k }.rem_euclid(len) as usize;
                let s = l[i];
                Some(Value::str(match case { 2 => s.to_uppercase(), 1 => s.to_lowercase(), _ => s.to_string() }))
            }
        }
    }
}
