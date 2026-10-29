//! Conditional formats (condformat.js): rules over ranges (cell value, formula, top/bottom,
//! average, duplicates, text, blanks, errors, dates, color scales, data bars, icon sets), kept
//! in priority order on the sheet, and the overlay each cell gets. Range-wide figures (ranks,
//! averages, duplicate counts, scale ends) are worked out once per rule per recalculation and
//! each cell's result is remembered until the next one.

use crate::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
use crate::book::{Sheet, Workbook};
use crate::formula::adjust;
use crate::formula::eval::{compare, evaluate_in};
use crate::style::{Border, Rgb, Style, Underline};
use crate::value::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Mutex;

/// A comparison (cellIs rules and data validation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum Op { #[default] Between, NotBetween, Equal, NotEqual, Greater, Less, GreaterEq, LessEq }

impl Op {
    pub const ALL: [(Op, &'static str, &'static str); 8] = [
        (Op::Between, "between", "between"), (Op::NotBetween, "notBetween", "not between"), (Op::Equal, "equal", "equal to"), (Op::NotEqual, "notEqual", "not equal to"),
        (Op::Greater, "greaterThan", "greater than"), (Op::Less, "lessThan", "less than"), (Op::GreaterEq, "greaterThanOrEqual", "greater than or equal to"), (Op::LessEq, "lessThanOrEqual", "less than or equal to"),
    ];
    pub fn name(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().1 }
    /// What the dialogs show ("greater than").
    pub fn label(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().2 }
    pub fn parse(s: &str) -> Op { Self::ALL.iter().find(|x| x.1 == s).map_or(Op::Between, |x| x.0) }
    /// Between and not between take two values.
    pub fn two(&self) -> bool { matches!(self, Op::Between | Op::NotBetween) }
    pub fn test(&self, v: &Value, a: &Value, b: &Value) -> bool {
        let ca = compare(v, a);
        match self {
            Op::Between | Op::NotBetween => {
                let (lo, hi) = if compare(a, b) == Ordering::Greater { (b, a) } else { (a, b) };
                let inside = compare(v, lo) != Ordering::Less && compare(v, hi) != Ordering::Greater;
                inside == (*self == Op::Between)
            }
            Op::Equal => ca == Ordering::Equal, Op::NotEqual => ca != Ordering::Equal,
            Op::Greater => ca == Ordering::Greater, Op::Less => ca == Ordering::Less,
            Op::GreaterEq => ca != Ordering::Less, Op::LessEq => ca != Ordering::Greater,
        }
    }
}

/// A differential format: only what is set changes the cell's own style.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Dxf {
    pub color: Option<Rgb>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<Underline>,
    pub strike: Option<bool>,
    pub fill: Option<Rgb>,
    /// The outline drawn on all four sides.
    pub border: Option<Border>,
    pub num_fmt: Option<String>,
}

impl Dxf {
    pub fn is_empty(&self) -> bool { *self == Dxf::default() }
    pub fn apply(&self, st: &mut Style) {
        if let Some(c) = self.color { st.color = Some(c); }
        if let Some(b) = self.bold { st.bold = b; }
        if let Some(i) = self.italic { st.italic = i; }
        if let Some(u) = self.underline { st.underline = u; }
        if let Some(s) = self.strike { st.strike = s; }
        if let Some(f) = self.fill { st.fill = Some(f); }
        if let Some(b) = self.border { st.left = b; st.right = b; st.top = b; st.bottom = b; }
        if let Some(n) = &self.num_fmt { st.num_fmt = n.clone(); }
    }
    /// Fills what this (higher priority) format leaves unset from `lower`.
    fn under(&mut self, lower: &Dxf) {
        macro_rules! take { ($($f:ident),*) => { $( if self.$f.is_none() { self.$f = lower.$f.clone(); } )* } }
        take!(color, bold, italic, underline, strike, fill, border, num_fmt);
    }
}

/// How a scale, bar or icon threshold is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VoKind { #[default] Min, Max, Num, Percent, Percentile, Formula, AutoMin, AutoMax }

impl VoKind {
    pub const ALL: [(VoKind, &'static str); 8] = [(VoKind::Min, "min"), (VoKind::Max, "max"), (VoKind::Num, "num"), (VoKind::Percent, "percent"), (VoKind::Percentile, "percentile"), (VoKind::Formula, "formula"), (VoKind::AutoMin, "autoMin"), (VoKind::AutoMax, "autoMax")];
    pub fn name(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().1 }
    pub fn parse(s: &str) -> VoKind { Self::ALL.iter().find(|x| x.1 == s).map_or(VoKind::Num, |x| x.0) }
}

/// A threshold: its kind, its value (a number or formula text) and whether it includes equality.
#[derive(Clone, Debug, PartialEq)]
pub struct Cfvo { pub kind: VoKind, pub val: String, pub gte: bool }

impl Cfvo {
    pub fn new(kind: VoKind, val: &str) -> Cfvo { Cfvo { kind, val: val.into(), gte: true } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextOp { #[default] Contains, NotContains, BeginsWith, EndsWith }

impl TextOp {
    pub const ALL: [(TextOp, &'static str, &'static str, &'static str); 4] = [
        (TextOp::Contains, "containsText", "containsText", "containing"), (TextOp::NotContains, "notContainsText", "notContains", "not containing"),
        (TextOp::BeginsWith, "beginsWith", "beginsWith", "beginning with"), (TextOp::EndsWith, "endsWith", "endsWith", "ending with"),
    ];
    /// The rule type in a file.
    pub fn name(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().1 }
    /// The operator attribute in a file.
    pub fn operator(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().2 }
    pub fn label(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().3 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DatePeriod { #[default] Today, Yesterday, Tomorrow, Last7Days, ThisWeek, LastWeek, NextWeek, ThisMonth, LastMonth, NextMonth }

impl DatePeriod {
    pub const ALL: [(DatePeriod, &'static str, &'static str); 10] = [
        (DatePeriod::Yesterday, "yesterday", "Yesterday"), (DatePeriod::Today, "today", "Today"), (DatePeriod::Tomorrow, "tomorrow", "Tomorrow"), (DatePeriod::Last7Days, "last7Days", "In the last 7 days"),
        (DatePeriod::LastWeek, "lastWeek", "Last week"), (DatePeriod::ThisWeek, "thisWeek", "This week"), (DatePeriod::NextWeek, "nextWeek", "Next week"),
        (DatePeriod::LastMonth, "lastMonth", "Last month"), (DatePeriod::ThisMonth, "thisMonth", "This month"), (DatePeriod::NextMonth, "nextMonth", "Next month"),
    ];
    pub fn name(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().1 }
    pub fn label(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().2 }
    pub fn parse(s: &str) -> DatePeriod { Self::ALL.iter().find(|x| x.1 == s).map_or(DatePeriod::Today, |x| x.0) }
    /// Whether the day `d` (a serial) falls in the period around `today`.
    pub fn contains(&self, d: f64, today: f64, date1904: bool) -> bool {
        let (d, t) = (d.floor(), today.floor());
        let week = t - crate::date::weekday(t, date1904) as f64;
        let month = |off: i64| { let (y, m, _) = crate::date::ymd(t, date1904); let k = y * 12 + m as i64 - 1 + off; (k.div_euclid(12), k.rem_euclid(12) as u32 + 1) };
        let (dy, dm, _) = crate::date::ymd(d, date1904);
        match self {
            DatePeriod::Today => d == t, DatePeriod::Yesterday => d == t - 1.0, DatePeriod::Tomorrow => d == t + 1.0,
            DatePeriod::Last7Days => d >= t - 6.0 && d <= t,
            DatePeriod::ThisWeek => d >= week && d < week + 7.0, DatePeriod::LastWeek => d >= week - 7.0 && d < week, DatePeriod::NextWeek => d >= week + 7.0 && d < week + 14.0,
            DatePeriod::ThisMonth => (dy, dm) == month(0), DatePeriod::LastMonth => (dy, dm) == month(-1), DatePeriod::NextMonth => (dy, dm) == month(1),
        }
    }
}

/// The icon sets, by their names in files, with how many icons each has.
pub const ICON_SETS: &[&str] = &["3Arrows", "3ArrowsGray", "3Flags", "3TrafficLights1", "3TrafficLights2", "3Signs", "3Symbols", "3Symbols2", "3Stars", "3Triangles",
    "4Arrows", "4ArrowsGray", "4RedToBlack", "4Rating", "4TrafficLights", "5Arrows", "5ArrowsGray", "5Rating", "5Quarters", "5Boxes"];

pub fn icon_count(set: &str) -> usize { set.chars().next().and_then(|c| c.to_digit(10)).map_or(3, |n| n as usize).clamp(3, 5) }

/// The default thresholds of an icon set: equal percent steps.
pub fn icon_cfvos(set: &str) -> Vec<Cfvo> {
    let n = icon_count(set);
    (0..n).map(|i| Cfvo::new(VoKind::Percent, &crate::value::fmt_num((100.0 * i as f64 / n as f64).round()))).collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum CfKind {
    CellIs { op: Op, f1: String, f2: String },
    Expression(String),
    Top { bottom: bool, percent: bool, rank: u32 },
    /// Above (or below) the average, `std_dev` standard deviations away when not 0.
    Average { below: bool, equal: bool, std_dev: u32 },
    Duplicate { unique: bool },
    Text { op: TextOp, text: String },
    Blanks(bool),
    Errors(bool),
    Date(DatePeriod),
    ColorScale { vos: Vec<Cfvo>, colors: Vec<Rgb> },
    DataBar { min: Cfvo, max: Cfvo, color: Rgb, negative: Option<Rgb>, gradient: bool, border: Option<Rgb>, show_value: bool },
    IconSet { set: String, vos: Vec<Cfvo>, reverse: bool, show_value: bool },
}

impl CfKind {
    pub fn color_scale(colors: &[Rgb]) -> CfKind {
        let vos = if colors.len() == 3 { vec![Cfvo::new(VoKind::Min, ""), Cfvo::new(VoKind::Percentile, "50"), Cfvo::new(VoKind::Max, "")] } else { vec![Cfvo::new(VoKind::Min, ""), Cfvo::new(VoKind::Max, "")] };
        CfKind::ColorScale { vos, colors: colors.to_vec() }
    }
    pub fn data_bar(color: Rgb, gradient: bool) -> CfKind {
        CfKind::DataBar { min: Cfvo::new(VoKind::AutoMin, ""), max: Cfvo::new(VoKind::AutoMax, ""), color, negative: Some([0xFF, 0, 0]), gradient, border: None, show_value: true }
    }
    pub fn icon_set(set: &str) -> CfKind { CfKind::IconSet { set: set.into(), vos: icon_cfvos(set), reverse: false, show_value: true } }
    /// Whether the rule applies a differential format (the others draw scales, bars or icons).
    pub fn uses_dxf(&self) -> bool { !matches!(self, CfKind::ColorScale { .. } | CfKind::DataBar { .. } | CfKind::IconSet { .. }) }
    /// Visits the formulas the rule holds, to rewrite them when references move.
    pub fn each_formula(&mut self, f: &mut dyn FnMut(&mut String)) {
        let vo = |v: &mut Cfvo, f: &mut dyn FnMut(&mut String)| if matches!(v.kind, VoKind::Formula | VoKind::Num | VoKind::Percent | VoKind::Percentile) { f(&mut v.val) };
        match self {
            CfKind::CellIs { f1, f2, .. } => { f(f1); f(f2); }
            CfKind::Expression(x) => f(x),
            CfKind::ColorScale { vos, .. } | CfKind::IconSet { vos, .. } => for v in vos { vo(v, f) },
            CfKind::DataBar { min, max, .. } => { vo(min, f); vo(max, f); }
            _ => {}
        }
    }
    /// A short description for the rules manager.
    pub fn describe(&self) -> String {
        match self {
            CfKind::CellIs { op, f1, f2 } => if op.two() { format!("Cell value {} {} and {}", op.label(), f1, f2) } else { format!("Cell value {} {}", op.label(), f1) },
            CfKind::Expression(f) => format!("Formula: ={f}"),
            CfKind::Top { bottom, percent, rank } => format!("{} {}{}", if *bottom { "Bottom" } else { "Top" }, rank, if *percent { "%" } else { "" }),
            CfKind::Average { below, equal, std_dev } => format!("{}{} average{}", if *below { "Below" } else { "Above" }, if *equal { " or equal to" } else { "" }, if *std_dev > 0 { format!(" ({std_dev} std dev)") } else { String::new() }),
            CfKind::Duplicate { unique } => (if *unique { "Unique values" } else { "Duplicate values" }).into(),
            CfKind::Text { op, text } => format!("Cell value {} \"{}\"", op.label(), text),
            CfKind::Blanks(b) => (if *b { "Blanks" } else { "No blanks" }).into(),
            CfKind::Errors(b) => (if *b { "Errors" } else { "No errors" }).into(),
            CfKind::Date(d) => d.label().into(),
            CfKind::ColorScale { colors, .. } => format!("{}-color scale", colors.len()),
            CfKind::DataBar { .. } => "Data bar".into(),
            CfKind::IconSet { set, .. } => format!("Icon set {set}"),
        }
    }
}

/// One rule over its ranges. The sheet's list is in priority order (first wins).
#[derive(Clone, Debug, PartialEq)]
pub struct CondFormat { pub ranges: Vec<Range>, pub kind: CfKind, pub dxf: Dxf, pub stop: bool }

impl CondFormat {
    pub fn new(ranges: Vec<Range>, kind: CfKind, dxf: Dxf) -> CondFormat { CondFormat { ranges, kind, dxf, stop: false } }
    pub fn contains(&self, p: Pos) -> bool { self.ranges.iter().any(|r| r.contains(p)) }
    /// The cell relative references are written from (the first range's top-left).
    pub fn origin(&self) -> Pos { self.ranges.first().map(|r| r.start()).unwrap_or_default() }
    pub fn sqref(&self) -> String { self.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(" ") }
}

/// A data bar as drawn: the part of the cell's width it covers (0..1), and the axis between
/// negative and positive values when there are both.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar { pub lo: f32, pub hi: f32, pub color: Rgb, pub gradient: bool, pub border: Option<Rgb>, pub axis: Option<f32> }

/// What conditional formatting does to one cell.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CfResult {
    pub dxf: Dxf,
    /// A color scale's fill.
    pub fill: Option<Rgb>,
    pub bar: Option<Bar>,
    /// The icon set and the icon's index in it (0 = the lowest values' icon).
    pub icon: Option<(String, u8)>,
    /// False when a bar or icon rule shows only the bar or icon.
    pub hide_value: bool,
}

impl CfResult {
    /// The cell's style with the overlay's format and fill.
    pub fn apply(&self, st: &mut Style) { self.dxf.apply(st); if let Some(f) = self.fill { st.fill = Some(f); } }
}

// ---- the per-recalculation cache --------------------------------------------------------------

/// Results remembered between recalculations (cleared by `Workbook::recalc` and `cf_dirty`).
#[derive(Default)]
pub struct Cache(Mutex<Option<SheetCache>>);
impl Clone for Cache { fn clone(&self) -> Self { Cache::default() } }
impl PartialEq for Cache { fn eq(&self, _: &Self) -> bool { true } }
impl std::fmt::Debug for Cache { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("Cache") } }
impl Cache { pub fn clear(&self) { if let Ok(mut g) = self.0.lock() { *g = None; } } }

#[derive(Default)]
pub struct SheetCache { rules: Vec<Option<Prep>>, cells: HashMap<Pos, Option<CfResult>> }

/// A value that can live in the cache (cell values share strings and can't cross threads).
#[derive(Clone, Debug)]
enum Lit { Num(f64), Text(String), Bool(bool), Err, Empty }

impl Lit {
    fn of(v: &Value) -> Lit { match v.scalar() { Value::Num(n) => Lit::Num(n), Value::Str(s) => Lit::Text(s.to_string()), Value::Bool(b) => Lit::Bool(b), Value::Err(_) => Lit::Err, _ => Lit::Empty } }
    fn value(&self) -> Value { match self { Lit::Num(n) => Value::Num(*n), Lit::Text(s) => Value::str(s), Lit::Bool(b) => Value::Bool(*b), Lit::Err => Value::Err(crate::value::ErrKind::Value), Lit::Empty => Value::Empty } }
}

/// What a rule worked out over its whole range.
enum Prep {
    None,
    /// cellIs operands that don't depend on the cell.
    Fixed(Lit, Lit),
    Threshold(Option<f64>),
    Avg { mean: f64, sd: f64 },
    Counts(HashMap<String, u32>),
    /// Scale, bar or icon thresholds, resolved to numbers.
    Points(Vec<f64>),
}

impl Workbook {
    /// Forgets cached conditional formatting (after edits that don't recalculate).
    pub fn cf_dirty(&self) { for s in &self.sheets { s.cf_cache.clear(); } }
}

/// The numbers in a rule's ranges.
fn numbers(s: &Sheet, cf: &CondFormat) -> Vec<f64> {
    let mut out = vec![];
    each_value(s, cf, &mut |v| if let Value::Num(n) = v { out.push(*n) });
    out
}

/// Every non-blank value in a rule's ranges (each cell once).
fn each_value(s: &Sheet, cf: &CondFormat, f: &mut dyn FnMut(&Value)) {
    let inside = |p: &Pos| { let mut it = cf.ranges.iter().filter(|r| r.contains(*p)); it.next().is_some() };
    for (p, c) in &s.cells { if !c.is_blank() && inside(p) { f(&c.value.scalar()); } }
    for (p, v) in &s.spill { if inside(p) && s.cells.get(p).is_none_or(|c| c.is_blank()) { f(&v.scalar()); } }
}

fn key(v: &Value) -> Option<String> {
    match v { Value::Num(n) => Some(crate::value::fmt_num(*n)), Value::Str(s) if !s.is_empty() => Some(s.to_lowercase()), Value::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.into()), Value::Err(e) => Some(e.text().into()), _ => None }
}

/// A formula evaluated for cell `at` of a rule written from `origin`.
fn eval_at(book: &Workbook, sheet: usize, origin: Pos, at: Pos, f: &str) -> Value {
    let f = f.trim().trim_start_matches('=');
    if f.is_empty() { return Value::Empty; }
    let shifted = adjust::shift(f, at.row as i64 - origin.row as i64, at.col as i64 - origin.col as i64);
    evaluate_in(book, sheet, at, &shifted).scalar()
}

/// Whether a formula reads relative to the cell (so it's evaluated per cell).
fn relative(f: &str) -> bool { let f = f.trim().trim_start_matches('='); adjust::shift(f, 1, 1) != f }

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let k = (p / 100.0).clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let (i, frac) = (k.floor() as usize, k - k.floor());
    sorted[i] + (sorted[(i + 1).min(sorted.len() - 1)] - sorted[i]) * frac
}

fn resolve(book: &Workbook, sheet: usize, cf: &CondFormat, v: &Cfvo, sorted: &[f64], bar: bool) -> f64 {
    let (lo, hi) = (sorted.first().copied().unwrap_or(0.0), sorted.last().copied().unwrap_or(0.0));
    let num = || -> f64 { match eval_at(book, sheet, cf.origin(), cf.origin(), &v.val) { Value::Num(n) => n, _ => v.val.trim().parse().unwrap_or(0.0) } };
    match v.kind {
        VoKind::Min => lo, VoKind::Max => hi,
        VoKind::AutoMin => if bar { lo.min(0.0) } else { lo },
        VoKind::AutoMax => if bar { hi.max(0.0) } else { hi },
        VoKind::Num | VoKind::Formula => num(),
        VoKind::Percent => lo + (hi - lo) * num() / 100.0,
        VoKind::Percentile => percentile(sorted, num()),
    }
}

fn prep(book: &Workbook, sheet: usize, cf: &CondFormat) -> Prep {
    let s = &book.sheets[sheet];
    let sorted = || { let mut v = numbers(s, cf); v.sort_by(|a, b| a.total_cmp(b)); v };
    match &cf.kind {
        CfKind::CellIs { f1, f2, .. } if !relative(f1) && !relative(f2) => {
            let o = cf.origin();
            Prep::Fixed(Lit::of(&eval_at(book, sheet, o, o, f1)), Lit::of(&eval_at(book, sheet, o, o, f2)))
        }
        CfKind::Top { bottom, percent, rank } => {
            let v = sorted();
            if v.is_empty() { return Prep::Threshold(None); }
            let n = if *percent { ((v.len() as f64 * *rank as f64 / 100.0).floor() as usize).max(1) } else { (*rank as usize).max(1) }.min(v.len());
            Prep::Threshold(Some(if *bottom { v[n - 1] } else { v[v.len() - n] }))
        }
        CfKind::Average { .. } => {
            let v = numbers(s, cf);
            if v.is_empty() { return Prep::None; }
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            let sd = if v.len() > 1 { (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt() } else { 0.0 };
            Prep::Avg { mean, sd }
        }
        CfKind::Duplicate { .. } => {
            let mut m: HashMap<String, u32> = HashMap::new();
            each_value(s, cf, &mut |v| if let Some(k) = key(v) { *m.entry(k).or_default() += 1; });
            Prep::Counts(m)
        }
        CfKind::ColorScale { vos, .. } | CfKind::IconSet { vos, .. } => {
            let v = sorted();
            Prep::Points(vos.iter().map(|x| resolve(book, sheet, cf, x, &v, false)).collect())
        }
        CfKind::DataBar { min, max, .. } => {
            let v = sorted();
            Prep::Points(vec![resolve(book, sheet, cf, min, &v, true), resolve(book, sheet, cf, max, &v, true)])
        }
        _ => Prep::None,
    }
}

fn truthy(v: &Value) -> bool { match v { Value::Bool(b) => *b, Value::Num(n) => *n != 0.0, Value::Str(s) => s.eq_ignore_ascii_case("TRUE"), _ => false } }

fn lerp(a: Rgb, b: Rgb, t: f64) -> Rgb { let t = t.clamp(0.0, 1.0); [0, 1, 2].map(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8) }

/// What one rule does to a cell; None when it doesn't apply.
enum Hit { Format, Fill(Rgb), Bar(Bar, bool), Icon(String, u8, bool) }

fn apply_rule(book: &Workbook, sheet: usize, cf: &CondFormat, prep: &Prep, p: Pos) -> Option<Hit> {
    let v = book.sheets[sheet].value(p).scalar();
    let num = if let Value::Num(n) = v { Some(n) } else { None };
    let yes = |b: bool| b.then_some(Hit::Format);
    match (&cf.kind, prep) {
        (CfKind::CellIs { op, .. }, Prep::Fixed(a, b)) => { if v.is_empty() && !matches!(a, Lit::Empty | Lit::Num(_)) { return None; } yes(op.test(&if v.is_empty() { Value::Num(0.0) } else { v.clone() }, &a.value(), &b.value())) }
        (CfKind::CellIs { op, f1, f2 }, _) => {
            let (a, b) = (eval_at(book, sheet, cf.origin(), p, f1), eval_at(book, sheet, cf.origin(), p, f2));
            yes(op.test(&if v.is_empty() { Value::Num(0.0) } else { v.clone() }, &a, &b))
        }
        (CfKind::Expression(f), _) => yes(truthy(&eval_at(book, sheet, cf.origin(), p, f))),
        (CfKind::Top { bottom, .. }, Prep::Threshold(Some(t))) => yes(num.is_some_and(|n| if *bottom { n <= *t } else { n >= *t })),
        (CfKind::Average { below, equal, std_dev }, Prep::Avg { mean, sd }) => {
            let n = num?;
            let k = *std_dev as f64 * sd;
            yes(match (below, equal) { (false, false) => n > mean + k, (false, true) => n >= mean + k, (true, false) => n < mean - k, (true, true) => n <= mean - k })
        }
        (CfKind::Duplicate { unique }, Prep::Counts(m)) => { let c = key(&v).and_then(|k| m.get(&k).copied())?; yes((c > 1) != *unique) }
        (CfKind::Text { op, text }, _) => {
            if v.is_empty() || v.is_err() { return yes(*op == TextOp::NotContains && !v.is_err()); }
            let t = v.to_text().unwrap_or_default().to_lowercase(); let q = text.to_lowercase();
            yes(match op { TextOp::Contains => t.contains(&q), TextOp::NotContains => !t.contains(&q), TextOp::BeginsWith => t.starts_with(&q), TextOp::EndsWith => t.ends_with(&q) })
        }
        (CfKind::Blanks(b), _) => { let blank = match &v { Value::Empty => true, Value::Str(s) => s.trim().is_empty(), _ => false }; yes(blank == *b) }
        (CfKind::Errors(b), _) => yes(v.is_err() == *b),
        (CfKind::Date(d), _) => yes(num.is_some_and(|n| d.contains(n, crate::numfmt::now_serial(), book.date1904))),
        (CfKind::ColorScale { colors, .. }, Prep::Points(pts)) => {
            let n = num?;
            if colors.len() >= 3 && pts.len() >= 3 {
                let (lo, mid, hi) = (pts[0], pts[1], pts[2]);
                Some(Hit::Fill(if n <= mid { lerp(colors[0], colors[1], if mid > lo { (n - lo) / (mid - lo) } else { 1.0 }) } else { lerp(colors[1], colors[2], if hi > mid { (n - mid) / (hi - mid) } else { 1.0 }) }))
            } else if colors.len() >= 2 && pts.len() >= 2 {
                let (lo, hi) = (pts[0], pts[pts.len() - 1]);
                Some(Hit::Fill(lerp(colors[0], colors[colors.len() - 1], if hi > lo { (n - lo) / (hi - lo) } else { 1.0 })))
            } else { None }
        }
        (CfKind::DataBar { color, negative, gradient, border, show_value, .. }, Prep::Points(pts)) => {
            let n = num?;
            let (lo, hi) = (pts[0].min(pts[1]), pts[0].max(pts[1]));
            let span = (hi - lo).max(1e-300);
            let pos = |x: f64| (((x.clamp(lo, hi) - lo) / span) as f32).clamp(0.0, 1.0);
            let bar = if lo < 0.0 && hi > 0.0 {
                let axis = pos(0.0);
                let (a, b) = if n < 0.0 { (pos(n), axis) } else { (axis, pos(n)) };
                Bar { lo: a, hi: b, color: if n < 0.0 { negative.unwrap_or(*color) } else { *color }, gradient: *gradient, border: *border, axis: Some(axis) }
            } else if hi <= 0.0 {
                // All negative: bars grow leftwards from the right edge.
                let f = if hi > lo { ((hi - n.clamp(lo, hi)) / span) as f32 } else { 1.0 };
                Bar { lo: 1.0 - (0.1 + 0.9 * f), hi: 1.0, color: negative.unwrap_or(*color), gradient: *gradient, border: *border, axis: None }
            } else {
                Bar { lo: 0.0, hi: 0.1 + 0.9 * pos(n), color: *color, gradient: *gradient, border: *border, axis: None }
            };
            Some(Hit::Bar(bar, !*show_value))
        }
        (CfKind::IconSet { set, vos, reverse, show_value }, Prep::Points(pts)) => {
            let n = num?;
            let count = icon_count(set).min(pts.len());
            let mut idx = 0;
            for i in 1..count { if if vos[i].gte { n >= pts[i] } else { n > pts[i] } { idx = i; } }
            if *reverse { idx = count - 1 - idx; }
            Some(Hit::Icon(set.clone(), idx as u8, !*show_value))
        }
        _ => None,
    }
}

/// The conditional formatting overlay of a cell, or None when no rule applies.
pub fn cf_at(book: &Workbook, sheet: usize, p: Pos) -> Option<CfResult> {
    let s = book.sheets.get(sheet)?;
    if s.cond_formats.is_empty() || !s.cond_formats.iter().any(|c| c.contains(p)) { return None; }
    let mut guard = s.cf_cache.0.lock().ok()?;
    let cache = guard.get_or_insert_with(SheetCache::default);
    if cache.rules.len() != s.cond_formats.len() { cache.rules = (0..s.cond_formats.len()).map(|_| None).collect(); cache.cells.clear(); }
    if let Some(r) = cache.cells.get(&p) { return r.clone(); }
    let mut out = CfResult::default();
    let mut any = false;
    for (i, cf) in s.cond_formats.iter().enumerate() {
        if !cf.contains(p) { continue; }
        if cache.rules[i].is_none() { cache.rules[i] = Some(prep(book, sheet, cf)); }
        let Some(hit) = apply_rule(book, sheet, cf, cache.rules[i].as_ref().unwrap(), p) else { continue };
        any = true;
        match hit {
            Hit::Format => out.dxf.under(&cf.dxf),
            Hit::Fill(c) => { if out.fill.is_none() { out.fill = Some(c); } }
            Hit::Bar(b, hide) => if out.bar.is_none() { out.bar = Some(b); out.hide_value |= hide; },
            Hit::Icon(set, k, hide) => if out.icon.is_none() { out.icon = Some((set, k)); out.hide_value |= hide; },
        }
        if cf.stop && cf.kind.uses_dxf() { break; }
    }
    let r = any.then_some(out);
    cache.cells.insert(p, r.clone());
    r
}

// ---- following edits ----------------------------------------------------------------------------

/// A range after `n` rows (or columns) are inserted at `at` (n > 0) or deleted from it (n < 0);
/// None when it's deleted entirely.
pub fn shift_range(r: Range, rows: bool, at: u32, n: i64) -> Option<Range> {
    let (lo, hi, max) = if rows { (r.r0, r.r1, MAX_ROWS) } else { (r.c0, r.c1, MAX_COLS) };
    let (a, b) = if n > 0 {
        let f = |v: u32| if v >= at { (v as i64 + n).min(max as i64 - 1) } else { v as i64 };
        if lo as i64 + if lo >= at { n } else { 0 } >= max as i64 { return None; }
        (f(lo), f(hi))
    } else {
        let end = at as i64 - n;
        let f = |v: u32, is_hi: bool| -> i64 { if v < at { v as i64 } else if (v as i64) >= end { v as i64 + n } else if is_hi { at as i64 - 1 } else { at as i64 } };
        (f(lo, false), f(hi, true))
    };
    if b < a || a < 0 { return None; }
    Some(if rows { Range { r0: a as u32, r1: b as u32, ..r } } else { Range { c0: a as u32, c1: b as u32, ..r } })
}

impl Sheet {
    /// Conditional formats and validations follow inserted or deleted rows and columns.
    pub fn shift_rules(&mut self, rows: bool, at: u32, n: i64) {
        for cf in &mut self.cond_formats { cf.ranges = cf.ranges.iter().filter_map(|r| shift_range(*r, rows, at, n)).collect(); }
        self.cond_formats.retain(|c| !c.ranges.is_empty());
        for v in &mut self.validations { v.ranges = v.ranges.iter().filter_map(|r| shift_range(*r, rows, at, n)).collect(); }
        self.validations.retain(|v| !v.ranges.is_empty());
        self.cf_cache.clear();
    }
    /// Rules whose ranges lie inside moved cells move with them.
    pub fn move_rules(&mut self, src: Range, dr: i64, dc: i64) {
        let mv = |r: &mut Range| if src.contains(r.start()) && src.contains(r.end()) { *r = Range { r0: (r.r0 as i64 + dr) as u32, r1: (r.r1 as i64 + dr) as u32, c0: (r.c0 as i64 + dc) as u32, c1: (r.c1 as i64 + dc) as u32 }; };
        for cf in &mut self.cond_formats { cf.ranges.iter_mut().for_each(mv); }
        for v in &mut self.validations { v.ranges.iter_mut().for_each(mv); }
        self.cf_cache.clear();
    }
    /// Takes `r` out of every rule's ranges (Clear Rules from Selected Cells), splitting ranges
    /// around it; rules left with nothing are deleted.
    pub fn clear_rules_in(&mut self, r: Range) { for cf in &mut self.cond_formats { cf.ranges = subtract(&cf.ranges, r); } self.cond_formats.retain(|c| !c.ranges.is_empty()); self.cf_cache.clear(); }
    pub fn clear_validation_in(&mut self, r: Range) { for v in &mut self.validations { v.ranges = subtract(&v.ranges, r); } self.validations.retain(|v| !v.ranges.is_empty()); }
}

/// Ranges with `cut` removed (each split into up to four pieces).
pub fn subtract(ranges: &[Range], cut: Range) -> Vec<Range> {
    let mut out = vec![];
    for r in ranges {
        if !r.intersects(&cut) { out.push(*r); continue; }
        if cut.r0 > r.r0 { out.push(Range { r1: cut.r0 - 1, ..*r }); }
        if cut.r1 < r.r1 { out.push(Range { r0: cut.r1 + 1, ..*r }); }
        let (r0, r1) = (r.r0.max(cut.r0), r.r1.min(cut.r1));
        if cut.c0 > r.c0 { out.push(Range { r0, r1, c1: cut.c0 - 1, ..*r }); }
        if cut.c1 < r.c1 { out.push(Range { r0, r1, c0: cut.c1 + 1, ..*r }); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::parse_range;

    fn rng(s: &str) -> Range { parse_range(s).unwrap() }
    fn p(s: &str) -> Pos { crate::addr::parse_a1(s).unwrap() }
    fn book(vals: &[(&str, &str)]) -> Workbook { let mut b = Workbook::new(); for (a, t) in vals { b.set_input(0, p(a), t); } b.recalc(); b }
    fn red() -> Dxf { Dxf { fill: Some([255, 0, 0]), ..Default::default() } }
    fn hit(b: &Workbook, a: &str) -> bool { cf_at(b, 0, p(a)).is_some() }

    #[test]
    fn cell_is_and_expression() {
        let mut b = book(&[("A1", "5"), ("A2", "15"), ("A3", "25"), ("B1", "10")]);
        b.sheets[0].cond_formats.push(CondFormat::new(vec![rng("A1:A3")], CfKind::CellIs { op: Op::Greater, f1: "$B$1".into(), f2: String::new() }, red()));
        assert!(!hit(&b, "A1") && hit(&b, "A2") && hit(&b, "A3"));
        assert_eq!(cf_at(&b, 0, p("A2")).unwrap().dxf.fill, Some([255, 0, 0]));
        b.sheets[0].cond_formats.clear(); b.cf_dirty();
        // Relative references move with each cell.
        b.sheets[0].cond_formats.push(CondFormat::new(vec![rng("A1:A3")], CfKind::Expression("A1>B$1*2".into()), red()));
        assert!(!hit(&b, "A1") && !hit(&b, "A2") && hit(&b, "A3"));
        b.sheets[0].cond_formats[0].kind = CfKind::CellIs { op: Op::Between, f1: "10".into(), f2: "20".into() }; b.cf_dirty();
        assert!(!hit(&b, "A1") && hit(&b, "A2") && !hit(&b, "A3"));
    }

    #[test]
    fn ranks_averages_duplicates() {
        let vals: Vec<(String, String)> = (1..=10).map(|i| (format!("A{i}"), i.to_string())).collect();
        let v: Vec<(&str, &str)> = vals.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let mut b = book(&v);
        b.sheets[0].cond_formats.push(CondFormat::new(vec![rng("A1:A10")], CfKind::Top { bottom: false, percent: false, rank: 3 }, red()));
        assert!(hit(&b, "A8") && hit(&b, "A10") && !hit(&b, "A7"));
        b.sheets[0].cond_formats[0].kind = CfKind::Top { bottom: true, percent: true, rank: 20 }; b.cf_dirty();
        assert!(hit(&b, "A1") && hit(&b, "A2") && !hit(&b, "A3"));
        b.sheets[0].cond_formats[0].kind = CfKind::Average { below: false, equal: false, std_dev: 0 }; b.cf_dirty();
        assert!(hit(&b, "A6") && !hit(&b, "A5"));
        let mut b = book(&[("A1", "x"), ("A2", "X"), ("A3", "y"), ("A4", "3"), ("A5", "3")]);
        b.sheets[0].cond_formats.push(CondFormat::new(vec![rng("A1:A5")], CfKind::Duplicate { unique: false }, red()));
        assert!(hit(&b, "A1") && hit(&b, "A2") && !hit(&b, "A3") && hit(&b, "A5"));
    }

    #[test]
    fn text_blanks_dates_and_priority() {
        let today = crate::numfmt::now_serial().floor();
        let mut b = book(&[("A1", "apple pie"), ("A3", "#N/A"), ("B1", &crate::value::fmt_num(today)), ("B2", &crate::value::fmt_num(today - 1.0)), ("B3", &crate::value::fmt_num(today - 40.0))]);
        let s = &mut b.sheets[0];
        s.cond_formats.push(CondFormat::new(vec![rng("A1:A3")], CfKind::Text { op: TextOp::BeginsWith, text: "APP".into() }, Dxf { bold: Some(true), ..Default::default() }));
        s.cond_formats.push(CondFormat::new(vec![rng("A1:A3")], CfKind::Blanks(true), red()));
        s.cond_formats.push(CondFormat::new(vec![rng("A1:A3")], CfKind::Errors(true), Dxf { italic: Some(true), ..Default::default() }));
        s.cond_formats.push(CondFormat::new(vec![rng("B1:B3")], CfKind::Date(DatePeriod::Last7Days), red()));
        assert_eq!(cf_at(&b, 0, p("A1")).unwrap().dxf.bold, Some(true));
        assert_eq!(cf_at(&b, 0, p("A2")).unwrap().dxf.fill, Some([255, 0, 0]));
        assert_eq!(cf_at(&b, 0, p("A3")).unwrap().dxf.italic, Some(true));
        assert!(hit(&b, "B1") && hit(&b, "B2") && !hit(&b, "B3"));
        // Higher priority wins a conflict; stop-if-true ends the list.
        let mut b = book(&[("A1", "1")]);
        b.sheets[0].cond_formats.push(CondFormat::new(vec![rng("A1")], CfKind::Expression("TRUE".into()), Dxf { fill: Some([1, 1, 1]), ..Default::default() }));
        b.sheets[0].cond_formats.push(CondFormat::new(vec![rng("A1")], CfKind::Expression("TRUE".into()), Dxf { fill: Some([2, 2, 2]), bold: Some(true), ..Default::default() }));
        let r = cf_at(&b, 0, p("A1")).unwrap();
        assert_eq!((r.dxf.fill, r.dxf.bold), (Some([1, 1, 1]), Some(true)));
        b.sheets[0].cond_formats[0].stop = true; b.cf_dirty();
        assert_eq!(cf_at(&b, 0, p("A1")).unwrap().dxf.bold, None);
    }

    #[test]
    fn scales_bars_icons() {
        let mut b = book(&[("A1", "0"), ("A2", "50"), ("A3", "100"), ("B1", "-10"), ("B2", "10")]);
        let s = &mut b.sheets[0];
        s.cond_formats.push(CondFormat::new(vec![rng("A1:A3")], CfKind::color_scale(&[[0, 0, 0], [255, 255, 255]]), Dxf::default()));
        assert_eq!(cf_at(&b, 0, p("A2")).unwrap().fill, Some([128, 128, 128]));
        let s = &mut b.sheets[0];
        s.cond_formats.clear();
        s.cond_formats.push(CondFormat::new(vec![rng("A1:A3")], CfKind::icon_set("3Arrows"), Dxf::default()));
        s.cond_formats.push(CondFormat::new(vec![rng("A1:A3")], CfKind::data_bar([0, 0, 255], true), Dxf::default()));
        s.cond_formats.push(CondFormat::new(vec![rng("B1:B2")], CfKind::data_bar([0, 0, 255], false), Dxf::default()));
        b.cf_dirty();
        let icon = |a: &str| cf_at(&b, 0, p(a)).unwrap().icon.unwrap().1;
        assert_eq!((icon("A1"), icon("A2"), icon("A3")), (0, 1, 2));
        let bar = cf_at(&b, 0, p("A3")).unwrap().bar.unwrap();
        assert_eq!((bar.lo, bar.hi), (0.0, 1.0));
        assert!((cf_at(&b, 0, p("A1")).unwrap().bar.unwrap().hi - 0.1).abs() < 1e-6);
        let neg = cf_at(&b, 0, p("B1")).unwrap().bar.unwrap();
        assert_eq!((neg.lo, neg.hi, neg.axis, neg.color), (0.0, 0.5, Some(0.5), [255, 0, 0]));
    }

    #[test]
    fn ranges_follow_rows() {
        let mut b = book(&[("A1", "1")]);
        b.sheets[0].cond_formats.push(CondFormat::new(vec![rng("B2:B5")], CfKind::Expression("B2>$A$1".into()), red()));
        b.insert(0, true, 0, 2);
        assert_eq!(b.sheets[0].cond_formats[0].ranges, vec![rng("B4:B7")]);
        assert_eq!(b.sheets[0].cond_formats[0].kind, CfKind::Expression("B4>$A$3".into()));
        b.insert(0, true, 4, -2);
        assert_eq!(b.sheets[0].cond_formats[0].ranges, vec![rng("B4:B5")]);
        assert_eq!(subtract(&[rng("A1:C3")], rng("B2")).len(), 4);
    }
}
