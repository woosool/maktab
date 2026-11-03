//! PivotTables (pivot.js): a summary of a source range by row and column fields, with value
//! fields aggregated (Sum, Count, Average, Max, Min, Product, Count Numbers, StdDev/StdDevp,
//! Var/Varp) and shown as calculated or relative to totals (Show Values As), report filters,
//! date and number grouping, calculated fields, subtotals and grand totals, laid out in
//! Compact, Outline or Tabular form with a PivotTable style.
//! The result is written into the destination sheet as plain cells; the definition lives on that
//! sheet (`Sheet.pivots`) so Refresh and slicers can run it again. Files keep it as native pivot
//! cache and pivot table parts (xlsx/pivot_parts.rs); older Jadwal files kept it in a hidden
//! defined name, still read by `load`.
//!
//! Field numbering: the source columns come first (0..n), then the extra fields grouping adds
//! (`Pivot::extra`, e.g. Years and Quarters over a date field, or a discrete grouping), then the
//! calculated fields (`Pivot::calcs`) — the same order Excel's cache fields use.

use crate::addr::{parse_range, quote_sheet, Pos, Range};
use crate::book::{Input, Workbook};
use crate::style::{Border, LineStyle, Rgb};
use crate::value::{ErrKind, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

// ---- value fields -------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Agg { #[default] Sum, Count, Average, Max, Min, Product, CountNums, StdDev, StdDevp, Var, Varp }

impl Agg {
    pub const ALL: [Agg; 11] = [Agg::Sum, Agg::Count, Agg::Average, Agg::Max, Agg::Min, Agg::Product, Agg::CountNums, Agg::StdDev, Agg::StdDevp, Agg::Var, Agg::Varp];
    pub fn label(self) -> &'static str {
        match self { Agg::Sum => "Sum", Agg::Count => "Count", Agg::Average => "Average", Agg::Max => "Max", Agg::Min => "Min", Agg::Product => "Product", Agg::CountNums => "Count Numbers", Agg::StdDev => "StdDev", Agg::StdDevp => "StdDevp", Agg::Var => "Var", Agg::Varp => "Varp" }
    }
    /// The file's name (ST_DataConsolidateFunction).
    pub fn code(self) -> &'static str {
        match self { Agg::Sum => "sum", Agg::Count => "count", Agg::Average => "average", Agg::Max => "max", Agg::Min => "min", Agg::Product => "product", Agg::CountNums => "countNums", Agg::StdDev => "stdDev", Agg::StdDevp => "stdDevp", Agg::Var => "var", Agg::Varp => "varp" }
    }
    pub fn parse(s: &str) -> Option<Agg> { Agg::ALL.into_iter().find(|a| a.code() == s) }
}

/// Show Values As.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShowAs { #[default] Normal, PctGrand, PctCol, PctRow, PctParentRow, Difference, RunTotal, RankAsc, RankDesc }

impl ShowAs {
    pub const ALL: [ShowAs; 9] = [ShowAs::Normal, ShowAs::PctGrand, ShowAs::PctCol, ShowAs::PctRow, ShowAs::PctParentRow, ShowAs::Difference, ShowAs::RunTotal, ShowAs::RankAsc, ShowAs::RankDesc];
    pub fn label(self) -> &'static str {
        match self {
            ShowAs::Normal => "No Calculation", ShowAs::PctGrand => "% of Grand Total", ShowAs::PctCol => "% of Column Total", ShowAs::PctRow => "% of Row Total",
            ShowAs::PctParentRow => "% of Parent Row Total", ShowAs::Difference => "Difference From", ShowAs::RunTotal => "Running Total In",
            ShowAs::RankAsc => "Rank Smallest to Largest", ShowAs::RankDesc => "Rank Largest to Smallest",
        }
    }
    /// Whether the calculation needs a base field (and Difference From a base item).
    pub fn needs_base(self) -> bool { matches!(self, ShowAs::Difference | ShowAs::RunTotal | ShowAs::RankAsc | ShowAs::RankDesc) }
    pub fn percent(self) -> bool { matches!(self, ShowAs::PctGrand | ShowAs::PctCol | ShowAs::PctRow | ShowAs::PctParentRow) }
}

/// The base item of Difference From.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum BaseItem { #[default] Prev, Next, Item(String) }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ValueField {
    pub field: usize,
    pub agg: Agg,
    pub show: ShowAs,
    pub base_field: usize,
    pub base_item: BaseItem,
    /// A custom name ("" = "Sum of Sales").
    pub name: String,
    /// A number format for the values ("" = General, or a percentage for the % calculations).
    pub num_fmt: String,
}

impl ValueField {
    pub fn new(field: usize, agg: Agg) -> ValueField { ValueField { field, agg, ..Default::default() } }
    /// The format the values show with.
    pub fn format(&self) -> Option<String> {
        if !self.num_fmt.is_empty() && self.num_fmt != "General" { return Some(self.num_fmt.clone()); }
        if self.show.percent() { Some("0.00%".into()) } else { None }
    }
}

// ---- grouping -----------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DateLevel { Seconds, Minutes, Hours, Days, Months, Quarters, Years }

impl DateLevel {
    /// Coarsest first, as the Group dialog lists them... in Excel's order.
    pub const ALL: [DateLevel; 7] = [DateLevel::Seconds, DateLevel::Minutes, DateLevel::Hours, DateLevel::Days, DateLevel::Months, DateLevel::Quarters, DateLevel::Years];
    pub fn label(self) -> &'static str { match self { DateLevel::Seconds => "Seconds", DateLevel::Minutes => "Minutes", DateLevel::Hours => "Hours", DateLevel::Days => "Days", DateLevel::Months => "Months", DateLevel::Quarters => "Quarters", DateLevel::Years => "Years" } }
    pub fn code(self) -> &'static str { match self { DateLevel::Seconds => "seconds", DateLevel::Minutes => "minutes", DateLevel::Hours => "hours", DateLevel::Days => "days", DateLevel::Months => "months", DateLevel::Quarters => "quarters", DateLevel::Years => "years" } }
    pub fn parse(s: &str) -> Option<DateLevel> { DateLevel::ALL.into_iter().find(|l| l.code() == s) }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Grouping {
    /// Numbers in ranges of `by` from `start` to `end` (with "<start" and ">end" items).
    Range { start: f64, end: f64, by: f64, auto_start: bool, auto_end: bool },
    /// Dates by one level (Days may step `by` days).
    Date { level: DateLevel, start: f64, end: f64, by: f64, auto_start: bool, auto_end: bool },
    /// Named groups of the base field's items (items in no group stand for themselves).
    Discrete(Vec<(String, Vec<String>)>),
}

/// A field grouping adds over a source field: a coarser date level or a discrete grouping.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtraField { pub name: String, pub base: usize, pub group: Grouping }

/// A calculated field: a formula over the other fields' sums (`Sales*0.1`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CalcField { pub name: String, pub formula: String }

const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn num_text(x: f64) -> String { key_text(&Value::Num(x)) }
fn date_text(s: f64, d1904: bool) -> String { let (y, m, d) = crate::date::ymd(s, d1904); format!("{m}/{d}/{y}") }
fn hour_text(h: u32) -> String { format!("{} {}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" }) }

impl Grouping {
    /// The item a value falls in, with its sort rank (None: sorts as text, after ranked items).
    pub fn label(&self, v: &Value, d1904: bool) -> (String, Option<i64>) {
        match self {
            Grouping::Discrete(groups) => { let t = key_text(v); (groups.iter().find(|g| g.1.contains(&t)).map(|g| g.0.clone()).unwrap_or(t), None) }
            Grouping::Range { start, end, by, .. } => {
                let Value::Num(x) = v else { return (key_text(v), None) };
                if x < start { return (format!("<{}", num_text(*start)), Some(i64::MIN)); }
                if x > end { return (format!(">{}", num_text(*end)), Some(i64::MAX)); }
                let k = ((x - start) / by.max(1e-12)).floor();
                (range_text(start + k * by, *by), Some(k as i64))
            }
            Grouping::Date { level, start, end, by, .. } => {
                let Value::Num(x) = v else { return (key_text(v), None) };
                if *x < start.floor() { return (format!("<{}", date_text(*start, d1904)), Some(i64::MIN)); }
                if x.floor() > end.floor() { return (format!(">{}", date_text(*end, d1904)), Some(i64::MAX)); }
                let (y, m, d) = crate::date::ymd(*x, d1904);
                let (h, mi, s, _) = crate::date::hms(*x);
                match level {
                    DateLevel::Years => (y.to_string(), Some(y)),
                    DateLevel::Quarters => { let q = (m - 1) / 3 + 1; (format!("Qtr{q}"), Some(q as i64)) }
                    DateLevel::Months => (MON[m as usize - 1].into(), Some(m as i64)),
                    DateLevel::Days if *by > 1.0 => {
                        let k = ((x.floor() - start.floor()) / by).floor();
                        let lo = start.floor() + k * by;
                        (format!("{} - {}", date_text(lo, d1904), date_text(lo + by - 1.0, d1904)), Some(k as i64))
                    }
                    DateLevel::Days => (format!("{d}-{}", MON[m as usize - 1]), Some(m as i64 * 100 + d as i64)),
                    DateLevel::Hours => (hour_text(h), Some(h as i64)),
                    DateLevel::Minutes => (format!(":{mi:02}"), Some(mi as i64)),
                    DateLevel::Seconds => (format!(":{s:02}"), Some(s as i64)),
                }
            }
        }
    }

    /// Every item of a range or date grouping in order, "<start" first and ">end" last (the
    /// file's groupItems); a discrete grouping's group names.
    pub fn all_items(&self, d1904: bool) -> Vec<String> {
        let mut out = vec![];
        match self {
            Grouping::Discrete(g) => return g.iter().map(|x| x.0.clone()).collect(),
            Grouping::Range { start, end, by, .. } => {
                out.push(format!("<{}", num_text(*start)));
                let k = (((end - start) / by.max(1e-12)).floor().max(0.0) as usize).min(5000);
                for i in 0..=k { out.push(range_text(start + i as f64 * by, *by)); }
                out.push(format!(">{}", num_text(*end)));
            }
            Grouping::Date { level, start, end, by, .. } => {
                out.push(format!("<{}", date_text(*start, d1904)));
                match level {
                    DateLevel::Years => { let (a, _, _) = crate::date::ymd(*start, d1904); let (b, _, _) = crate::date::ymd(*end, d1904); for y in a..=b.max(a) { out.push(y.to_string()); } }
                    DateLevel::Quarters => for q in 1..=4 { out.push(format!("Qtr{q}")); },
                    DateLevel::Months => for m in MON { out.push(m.into()); },
                    DateLevel::Days if *by > 1.0 => {
                        let k = (((end.floor() - start.floor()) / by).floor().max(0.0) as usize).min(5000);
                        for i in 0..=k { let lo = start.floor() + i as f64 * by; out.push(format!("{} - {}", date_text(lo, d1904), date_text(lo + by - 1.0, d1904))); }
                    }
                    DateLevel::Days => for (mi, m) in MON.iter().enumerate() { for d in 1..=crate::date::days_in_month(2000, mi as u32 + 1) { out.push(format!("{d}-{m}")); } },
                    DateLevel::Hours => for h in 0..24 { out.push(hour_text(h)); },
                    DateLevel::Minutes | DateLevel::Seconds => for m in 0..60 { out.push(format!(":{m:02}")); },
                }
                out.push(format!(">{}", date_text(*end, d1904)));
            }
        }
        out
    }

    pub fn is_auto(&self) -> (bool, bool) {
        match self { Grouping::Range { auto_start, auto_end, .. } | Grouping::Date { auto_start, auto_end, .. } => (*auto_start, *auto_end), _ => (false, false) }
    }
    fn set_bounds(&mut self, lo: f64, hi: f64) {
        match self {
            Grouping::Range { start, end, auto_start, auto_end, .. } | Grouping::Date { start, end, auto_start, auto_end, .. } => { if *auto_start { *start = lo; } if *auto_end { *end = hi; } }
            _ => {}
        }
    }
}

fn range_text(lo: f64, by: f64) -> String {
    if lo.fract() == 0.0 && by.fract() == 0.0 { format!("{}-{}", num_text(lo), num_text(lo + by - 1.0)) } else { format!("{}-{}", num_text(lo), num_text(lo + by)) }
}

// ---- calculated field formulas -------------------------------------------------------------------------

/// A parsed calculated-field formula: numbers, source fields (by index), + - * / ^ and parentheses.
#[derive(Clone, Debug, PartialEq)]
pub enum Ex { Num(f64), Field(usize), Neg(Box<Ex>), Bin(char, Box<Ex>, Box<Ex>) }

/// Parses `=Sales*0.1` (or without "="), naming fields bare or 'quoted'.
pub fn parse_calc(formula: &str, names: &[String]) -> Result<Ex, String> {
    #[derive(Debug, Clone, PartialEq)]
    enum T { N(f64), Id(String), Op(char) }
    let s = formula.trim();
    let s = s.strip_prefix('=').unwrap_or(s);
    let cs: Vec<char> = s.chars().collect();
    let mut toks = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() { i += 1; continue; }
        if "+-*/^()".contains(c) { toks.push(T::Op(c)); i += 1; continue; }
        if c.is_ascii_digit() || (c == '.' && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.' || ((cs[i] == 'e' || cs[i] == 'E') && i + 1 < cs.len())) { i += 1; }
            let t: String = cs[st..i].iter().collect();
            toks.push(T::N(t.parse().map_err(|_| format!("Bad number {t}"))?));
            continue;
        }
        if c == '\'' {
            let mut t = String::new();
            i += 1;
            loop {
                match cs.get(i) { None => return Err("Missing '".into()), Some('\'') if cs.get(i + 1) == Some(&'\'') => { t.push('\''); i += 2; } Some('\'') => { i += 1; break; } Some(ch) => { t.push(*ch); i += 1; } }
            }
            toks.push(T::Id(t));
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let st = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_' || cs[i] == '.') { i += 1; }
            toks.push(T::Id(cs[st..i].iter().collect()));
            continue;
        }
        return Err(format!("Unexpected {c}"));
    }
    struct P<'a> { t: Vec<T>, i: usize, names: &'a [String] }
    impl P<'_> {
        fn peek(&self) -> Option<&T> { self.t.get(self.i) }
        fn bin(&mut self, lvl: u8) -> Result<Ex, String> {
            if lvl == 3 { return self.unary(); }
            let ops: &[char] = match lvl { 0 => &['+', '-'], 1 => &['*', '/'], _ => &['^'] };
            let mut l = self.bin(lvl + 1)?;
            while let Some(T::Op(o)) = self.peek().cloned() {
                if !ops.contains(&o) { break; }
                self.i += 1;
                let r = self.bin(lvl + 1)?;
                l = Ex::Bin(o, Box::new(l), Box::new(r));
            }
            Ok(l)
        }
        fn unary(&mut self) -> Result<Ex, String> {
            match self.peek().cloned() {
                Some(T::Op('-')) => { self.i += 1; Ok(Ex::Neg(Box::new(self.unary()?))) }
                Some(T::Op('+')) => { self.i += 1; self.unary() }
                Some(T::Op('(')) => { self.i += 1; let e = self.bin(0)?; if self.peek() != Some(&T::Op(')')) { return Err("Missing )".into()); } self.i += 1; Ok(e) }
                Some(T::N(n)) => { self.i += 1; Ok(Ex::Num(n)) }
                Some(T::Id(name)) => { self.i += 1; self.names.iter().position(|n| n.eq_ignore_ascii_case(name.trim())).map(Ex::Field).ok_or(format!("No field named {name}")) }
                _ => Err("Incomplete formula".into()),
            }
        }
    }
    let mut p = P { t: toks, i: 0, names };
    let e = p.bin(0)?;
    if p.i != p.t.len() { return Err("Unexpected text in the formula".into()); }
    Ok(e)
}

pub fn eval_calc(e: &Ex, sums: &[f64]) -> Result<f64, ErrKind> {
    Ok(match e {
        Ex::Num(n) => *n,
        Ex::Field(f) => sums.get(*f).copied().unwrap_or(0.0),
        Ex::Neg(a) => -eval_calc(a, sums)?,
        Ex::Bin(o, a, b) => {
            let (x, y) = (eval_calc(a, sums)?, eval_calc(b, sums)?);
            match o { '+' => x + y, '-' => x - y, '*' => x * y, '/' => { if y == 0.0 { return Err(ErrKind::Div0); } x / y } _ => { let r = x.powf(y); if !r.is_finite() { return Err(ErrKind::Num); } r } }
        }
    })
}

// ---- layout and style -------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Layout { #[default] Compact, Outline, Tabular }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Subtotals { #[default] Top, Bottom, Off }

#[derive(Clone, Debug, PartialEq)]
pub struct PivotStyle { pub name: String, pub row_headers: bool, pub col_headers: bool, pub banded_rows: bool, pub banded_cols: bool }

impl Default for PivotStyle {
    fn default() -> Self { PivotStyle { name: "PivotStyleLight16".into(), row_headers: true, col_headers: true, banded_rows: false, banded_cols: false } }
}

/// A built-in style name's family (0 Light, 1 Medium, 2 Dark) and number.
pub fn style_parts(name: &str) -> Option<(u8, u32)> {
    for (k, pre) in ["PivotStyleLight", "PivotStyleMedium", "PivotStyleDark"].iter().enumerate() {
        if let Some(n) = name.strip_prefix(pre).and_then(|n| n.parse().ok()) { return Some((k as u8, n)); }
    }
    None
}

fn tint(c: Rgb, t: f32) -> Rgb { c.map(|v| (v as f32 + (255.0 - v as f32) * t).round() as u8) }
fn shade(c: Rgb, t: f32) -> Rgb { c.map(|v| (v as f32 * t).round() as u8) }

/// The colors of a built-in style: (header fill, header text, band fill, total fill, data fill, data text, line).
pub fn style_colors(theme: &[[u8; 3]], name: &str) -> Option<[Option<Rgb>; 7]> {
    let (fam, n) = style_parts(name)?;
    let a = (n.saturating_sub(1) % 7) as usize;
    let c: Rgb = if a == 0 { [0x40, 0x40, 0x40] } else { theme.get(3 + a).copied().unwrap_or([0x44, 0x72, 0xC4]) };
    let white = Some([255, 255, 255]);
    Some(match fam {
        0 => [Some(tint(c, 0.8)), None, Some(tint(c, 0.9)), Some(tint(c, 0.8)), None, None, Some(c)],
        1 => [Some(c), white, Some(tint(c, 0.8)), Some(tint(c, 0.6)), None, None, Some(c)],
        _ => [Some(shade(c, 0.5)), white, Some(shade(c, 0.8)), Some(shade(c, 0.5)), Some(c), white, None],
    })
}

// ---- the pivot ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Pivot {
    pub name: String,
    /// The source: a sheet and a range whose first row names the fields.
    pub source_sheet: String,
    pub source: Range,
    pub rows: Vec<usize>,
    pub cols: Vec<usize>,
    pub values: Vec<ValueField>,
    pub filters: Vec<usize>,
    /// Items left out, by field (report filters, slicers, row/column item filters).
    pub hidden: BTreeMap<usize, BTreeSet<String>>,
    /// The top-left cell of the report on the sheet holding the pivot.
    pub anchor: Pos,
    /// The cells the last run wrote, cleared before the next.
    pub output: Option<Range>,
    /// Range and date groupings of source fields.
    pub groups: BTreeMap<usize, Grouping>,
    /// Fields grouping added (numbered after the source fields).
    pub extra: Vec<ExtraField>,
    /// Calculated fields (numbered after the extra fields).
    pub calcs: Vec<CalcField>,
    pub layout: Layout,
    pub subtotals: Subtotals,
    /// The Grand Total column at the right (Excel's rowGrandTotals).
    pub row_grand: bool,
    /// The Grand Total row at the bottom (Excel's colGrandTotals).
    pub col_grand: bool,
    pub repeat_labels: bool,
    pub blank_rows: bool,
    pub style: PivotStyle,
    /// A timeline's selection: only records whose source field `.0` (a date) falls within
    /// serial days `.1..=.2` count. Set from the sheet's timelines (timeline.rs), not saved here.
    pub date_filter: Option<(usize, f64, f64)>,
}

impl Default for Pivot {
    fn default() -> Self {
        Pivot { name: String::new(), source_sheet: String::new(), source: Range::default(), rows: vec![], cols: vec![], values: vec![], filters: vec![], hidden: BTreeMap::new(),
            anchor: Pos::default(), output: None, groups: BTreeMap::new(), extra: vec![], calcs: vec![], layout: Layout::Compact, subtotals: Subtotals::Top,
            row_grand: true, col_grand: true, repeat_labels: false, blank_rows: false, style: PivotStyle::default(), date_filter: None }
    }
}

/// What a report row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role { #[default] Blank, Filter, Header, Data, Group, Subtotal, Grand }

/// A computed report: cells by (row, col) offset from the anchor, and what each row is.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Report {
    pub cells: Vec<Vec<Value>>,
    pub bold_rows: Vec<usize>,
    pub roles: Vec<Role>,
    /// For body rows: the row key (item texts of the row fields, a prefix for group and subtotal rows).
    pub keys: Vec<Vec<String>>,
    /// Indents of compact-form labels: (row, col, level).
    pub indents: Vec<(usize, usize, u8)>,
    /// Number formats by column.
    pub col_fmts: BTreeMap<usize, String>,
    /// Label columns before the values, and value fields per column item.
    pub lead: usize,
    pub nv: usize,
    /// The column keys in order (one empty key without column fields).
    pub col_keys: Vec<Vec<String>>,
    /// Header rows holding the items of each column field: (row, field).
    pub col_heads: Vec<(usize, usize)>,
    /// The header row naming the row fields.
    pub row_head: Option<usize>,
}

/// What a report cell belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit { Field(usize), Value(usize) }

pub(crate) fn key_text(v: &Value) -> String {
    match v { Value::Empty => "(blank)".into(), Value::Num(n) => crate::numfmt::format(&Value::Num(*n), "General", false).text, Value::Str(s) => s.to_string(), Value::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() }, Value::Err(e) => e.text().into(), Value::Array(_) => String::new() }
}

/// One item of a field: its key text, the value shown, and a sort rank for grouped items.
pub(crate) type Item = (String, Value, Option<i64>);

/// Items sort grouped ones by rank, then numbers (by value), then text (case-insensitively), blanks last.
pub(crate) fn cmp_item(a: &Item, b: &Item) -> Ordering {
    match (a.2, b.2) {
        (Some(x), Some(y)) => return x.cmp(&y),
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        _ => {}
    }
    let rank = |v: &Value| match v { Value::Num(_) => 0, Value::Empty => 2, _ => 1 };
    rank(&a.1).cmp(&rank(&b.1)).then_with(|| match (&a.1, &b.1) {
        (Value::Num(x), Value::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        _ => a.0.to_lowercase().cmp(&b.0.to_lowercase()),
    })
}

#[derive(Default, Clone)]
struct Acc { sum: f64, sumsq: f64, prod: f64, count: usize, nums: usize, max: f64, min: f64 }
impl Acc {
    fn add(&mut self, v: &Value) {
        if v.is_empty() { return; }
        self.count += 1;
        if let Value::Num(n) = v {
            let n = *n;
            if self.nums == 0 { self.max = n; self.min = n; self.prod = n; } else { self.max = self.max.max(n); self.min = self.min.min(n); self.prod *= n; }
            self.sum += n; self.sumsq += n * n; self.nums += 1;
        }
    }
    fn get(&self, a: Agg) -> Value {
        let k = self.nums as f64;
        let var = |pop: bool| -> Value {
            let d = if pop { k } else { k - 1.0 };
            if d <= 0.0 { return Value::Err(ErrKind::Div0); }
            Value::Num(((self.sumsq - self.sum * self.sum / k) / d).max(0.0))
        };
        match a {
            Agg::Sum => Value::Num(self.sum),
            Agg::Count => Value::Num(self.count as f64),
            Agg::CountNums => Value::Num(k),
            Agg::Average => if self.nums == 0 { Value::Err(ErrKind::Div0) } else { Value::Num(self.sum / k) },
            Agg::Max => Value::Num(if self.nums == 0 { 0.0 } else { self.max }),
            Agg::Min => Value::Num(if self.nums == 0 { 0.0 } else { self.min }),
            Agg::Product => Value::Num(if self.nums == 0 { 0.0 } else { self.prod }),
            Agg::Var => var(false), Agg::Varp => var(true),
            Agg::StdDev => match var(false) { Value::Num(v) => Value::Num(v.sqrt()), e => e },
            Agg::StdDevp => match var(true) { Value::Num(v) => Value::Num(v.sqrt()), e => e },
        }
    }
}

type CellKey = (Vec<String>, Option<Vec<String>>);

/// The accumulated values of a run, and Show Values As over them.
struct Grid<'a> {
    p: &'a Pivot,
    n: usize,
    cells: HashMap<CellKey, (Vec<Acc>, Vec<f64>)>,
    calc_ex: Vec<Option<Ex>>,
    row_items: Vec<Vec<String>>,
    col_items: Vec<Vec<String>>,
}

impl Grid<'_> {
    fn raw(&self, rk: &[String], ck: Option<&[String]>, i: usize) -> Value {
        let Some(c) = self.cells.get(&(rk.to_vec(), ck.map(|c| c.to_vec()))) else { return Value::Empty };
        let Some(vf) = self.p.values.get(i) else { return Value::Num(c.0[0].count as f64) };
        if let Some(ci) = self.p.calc_index(vf.field) {
            return match self.calc_ex.get(ci).and_then(|e| e.as_ref()) { Some(e) => eval_calc(e, &c.1).map(Value::Num).unwrap_or_else(Value::Err), None => Value::Err(ErrKind::Name) };
        }
        c.0[i].get(vf.agg)
    }
    fn num(&self, rk: &[String], ck: Option<&[String]>, i: usize) -> Option<f64> { if let Value::Num(x) = self.raw(rk, ck, i) { Some(x) } else { None } }

    fn shown(&self, rk: &[String], ck: Option<&[String]>, i: usize) -> Value {
        let raw = self.raw(rk, ck, i);
        let Some(vf) = self.p.values.get(i) else { return raw };
        if vf.show == ShowAs::Normal || raw.is_empty() { return raw; }
        let Value::Num(x) = raw else { return raw };
        let ratio = |d: Option<f64>| match d { Some(d) if d != 0.0 => Value::Num(x / d), _ => Value::Err(ErrKind::Div0) };
        match vf.show {
            ShowAs::Normal => raw,
            ShowAs::PctGrand => ratio(self.num(&[], None, i)),
            ShowAs::PctCol => ratio(self.num(&[], ck, i)),
            ShowAs::PctRow => ratio(self.num(rk, None, i)),
            ShowAs::PctParentRow => if rk.is_empty() { Value::Num(1.0) } else { ratio(self.num(&rk[..rk.len() - 1], ck, i)) },
            ShowAs::Difference | ShowAs::RunTotal | ShowAs::RankAsc | ShowAs::RankDesc => {
                let bf = vf.base_field;
                // The axis and depth of the base field; the key along it.
                let (on_rows, d) = match (self.p.rows.iter().position(|f| *f == bf), self.p.cols.iter().position(|f| *f == bf)) {
                    (Some(d), _) => (true, d), (None, Some(d)) => (false, d), _ => return Value::Err(ErrKind::NA),
                };
                let key: Vec<String> = match (on_rows, ck) { (true, _) => rk.to_vec(), (false, Some(c)) => c.to_vec(), (false, None) => return Value::Empty };
                if key.len() <= d { return Value::Empty; }
                let items = if on_rows { &self.row_items[d] } else { &self.col_items[d] };
                let with = |t: &str| -> Option<f64> {
                    let mut k = key.clone(); k[d] = t.to_string();
                    if on_rows { self.num(&k, ck, i) } else { self.num(rk, Some(&k), i) }
                };
                let at = items.iter().position(|t| *t == key[d]).unwrap_or(0);
                match vf.show {
                    ShowAs::Difference => {
                        let base = match &vf.base_item {
                            BaseItem::Item(t) => Some(t.clone()),
                            BaseItem::Prev => at.checked_sub(1).map(|j| items[j].clone()),
                            BaseItem::Next => items.get(at + 1).cloned(),
                        };
                        match base { None => Value::Empty, Some(b) if b == key[d] => Value::Empty, Some(b) => Value::Num(x - with(&b).unwrap_or(0.0)) }
                    }
                    ShowAs::RunTotal => Value::Num(items[..=at.min(items.len().saturating_sub(1))].iter().filter_map(|t| with(t)).sum()),
                    _ => {
                        let asc = vf.show == ShowAs::RankAsc;
                        let better = items.iter().filter_map(|t| with(t)).filter(|y| if asc { *y < x } else { *y > x }).count();
                        Value::Num(better as f64 + 1.0)
                    }
                }
            }
        }
    }
}

impl Pivot {
    pub fn new(name: &str, source_sheet: &str, source: Range, anchor: Pos) -> Pivot {
        Pivot { name: name.into(), source_sheet: source_sheet.into(), source, anchor, ..Default::default() }
    }

    /// The number of source fields.
    pub fn n_source(&self) -> usize { (self.source.c1 - self.source.c0 + 1) as usize }
    /// Every field: source, extra and calculated.
    pub fn n_fields(&self) -> usize { self.n_source() + self.extra.len() + self.calcs.len() }
    /// The calculated field a field number names.
    pub fn calc_index(&self, f: usize) -> Option<usize> { f.checked_sub(self.n_source() + self.extra.len()).filter(|c| *c < self.calcs.len()) }
    /// The source field a field's values come from.
    pub fn base_of(&self, f: usize) -> usize { let n = self.n_source(); if f >= n && f < n + self.extra.len() { self.extra[f - n].base } else { f } }
    pub fn group_of(&self, f: usize) -> Option<&Grouping> { let n = self.n_source(); if f < n { self.groups.get(&f) } else { self.extra.get(f - n).map(|e| &e.group) } }

    fn source_names(&self, wb: &Workbook) -> Vec<String> {
        let Some(si) = wb.sheet_index(&self.source_sheet) else { return vec![] };
        (self.source.c0..=self.source.c1).map(|c| { let t = wb.display(si, Pos::new(self.source.r0, c)); if t.trim().is_empty() { format!("Column{}", c - self.source.c0 + 1) } else { t } }).collect()
    }

    /// The field names (the source's first row, blanks become Column1...; then extra and calculated fields).
    pub fn fields(&self, wb: &Workbook) -> Vec<String> {
        let mut v = self.source_names(wb);
        if v.is_empty() { return v; }
        v.extend(self.extra.iter().map(|e| e.name.clone()));
        v.extend(self.calcs.iter().map(|c| c.name.clone()));
        v
    }

    pub fn value_name(&self, wb: &Workbook, v: &ValueField) -> String {
        if !v.name.is_empty() { return v.name.clone(); }
        let f = self.fields(wb).get(v.field).cloned().unwrap_or_default();
        if self.calc_index(v.field).is_some() { format!("Sum of {f}") } else { format!("{} of {}", v.agg.label(), f) }
    }

    /// The source records (rows under the header that aren't blank).
    pub(crate) fn records(&self, wb: &Workbook) -> Vec<Vec<Value>> {
        let Some(si) = wb.sheet_index(&self.source_sheet) else { return vec![] };
        let s = &wb.sheets[si];
        let src = self.source;
        (src.r0 + 1..=src.r1).map(|r| (src.c0..=src.c1).map(|c| s.value(Pos::new(r, c))).collect::<Vec<Value>>()).filter(|r| !r.iter().all(|v| v.is_empty()))
            .filter(|r| match self.date_filter { Some((f, a, b)) => matches!(r.get(f), Some(Value::Num(n)) if n.floor() >= a && n.floor() <= b), None => true }).collect()
    }

    /// A field's item for a record.
    pub(crate) fn item(&self, rec: &[Value], f: usize, d1904: bool) -> Item {
        let v = rec.get(self.base_of(f)).cloned().unwrap_or_default();
        match self.group_of(f) { Some(g) => { let (l, r) = g.label(&v, d1904); (l.clone(), Value::str(l), r) } None => (key_text(&v), v, None) }
    }

    /// The pivot with automatic group bounds set from the data.
    pub fn refreshed(&self, wb: &Workbook) -> Pivot {
        let mut p = self.clone();
        let recs = self.records(wb);
        let bounds = |f: usize| -> Option<(f64, f64)> {
            let nums: Vec<f64> = recs.iter().filter_map(|r| if let Some(Value::Num(x)) = r.get(f) { Some(*x) } else { None }).collect();
            if nums.is_empty() { return None; }
            Some((nums.iter().cloned().fold(f64::INFINITY, f64::min), nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max)))
        };
        for (f, g) in p.groups.iter_mut() { if let Some((lo, hi)) = bounds(*f) { g.set_bounds(lo, hi); } }
        for e in p.extra.iter_mut() { if let Some((lo, hi)) = bounds(e.base) { e.group.set_bounds(lo, hi); } }
        p
    }

    /// The (min, max) of a field's numbers.
    pub fn field_bounds(&self, wb: &Workbook, f: usize) -> Option<(f64, f64)> {
        let recs = self.records(wb);
        let f = self.base_of(f);
        let nums: Vec<f64> = recs.iter().filter_map(|r| if let Some(Value::Num(x)) = r.get(f) { Some(*x) } else { None }).collect();
        if nums.is_empty() { return None; }
        Some((nums.iter().cloned().fold(f64::INFINITY, f64::min), nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max)))
    }

    /// The distinct items of a field, sorted.
    pub fn items(&self, wb: &Workbook, field: usize) -> Vec<String> {
        if self.calc_index(field).is_some() { return vec![]; }
        let p = self.refreshed(wb);
        let mut seen: BTreeMap<String, Item> = BTreeMap::new();
        for r in p.records(wb) { let it = p.item(&r, field, wb.date1904); seen.entry(it.0.clone()).or_insert(it); }
        let mut v: Vec<Item> = seen.into_values().collect();
        v.sort_by(cmp_item);
        v.into_iter().map(|x| x.0).collect()
    }

    // ---- grouping and calculated fields ----

    /// Renumbers fields (None drops a field from the areas).
    fn remap(&mut self, map: &dyn Fn(usize) -> Option<usize>) {
        let m = |v: &mut Vec<usize>| *v = v.iter().filter_map(|f| map(*f)).collect();
        m(&mut self.rows); m(&mut self.cols); m(&mut self.filters);
        self.values = std::mem::take(&mut self.values).into_iter().filter_map(|mut v| { v.field = map(v.field)?; v.base_field = map(v.base_field).unwrap_or(0); Some(v) }).collect();
        self.hidden = std::mem::take(&mut self.hidden).into_iter().filter_map(|(f, h)| Some((map(f)?, h))).collect();
    }

    /// Removes a field's grouping (and the fields it added).
    pub fn ungroup(&mut self, f: usize) {
        let b = self.base_of(f);
        let n = self.n_source();
        self.groups.remove(&b);
        self.hidden.remove(&b);
        let gone: Vec<usize> = (0..self.extra.len()).filter(|j| self.extra[*j].base == b).collect();
        if gone.is_empty() { return; }
        let map = |x: usize| -> Option<usize> { if x < n { return Some(x); } let j = x - n; if gone.contains(&j) { return None; } Some(x - gone.iter().filter(|g| **g < j).count()) };
        self.remap(&map);
        let mut j = 0;
        self.extra.retain(|_| { let keep = !gone.contains(&j); j += 1; keep });
    }

    /// Adds extra fields grouping `base` (before the calculated fields), placed before `base` on its axis.
    fn add_extra(&mut self, base: usize, new: Vec<ExtraField>, wb_names: &[String]) {
        let n = self.n_source();
        let at = n + self.extra.len();
        let k = new.len();
        self.remap(&|x| Some(if x >= at { x + k } else { x }));
        let mut names: Vec<String> = wb_names.to_vec();
        let mut added = vec![];
        for (i, mut e) in new.into_iter().enumerate() {
            let stem = e.name.clone();
            let mut c = 2;
            while names.iter().any(|x| x.eq_ignore_ascii_case(&e.name)) { e.name = format!("{stem}{c}"); c += 1; }
            names.push(e.name.clone());
            self.extra.push(e);
            added.push(at + i);
        }
        for axis in [&mut self.rows, &mut self.cols, &mut self.filters] {
            if let Some(pos) = axis.iter().position(|f| *f == base) { for (i, f) in added.iter().enumerate() { axis.insert(pos + i, *f); } }
        }
    }

    /// Groups a source field by date levels (coarsest last in Excel's list; any order here).
    pub fn group_dates(&mut self, wb: &Workbook, f: usize, levels: &[DateLevel], start: f64, end: f64, by: f64, auto_start: bool, auto_end: bool) {
        let b = self.base_of(f);
        self.ungroup(b);
        let mut lv: Vec<DateLevel> = levels.to_vec();
        lv.sort(); lv.dedup();
        let Some(&finest) = lv.first() else { return };
        let g = |level| Grouping::Date { level, start, end, by: if level == DateLevel::Days && lv.len() == 1 { by.max(1.0) } else { 1.0 }, auto_start, auto_end };
        self.groups.insert(b, g(finest));
        // Coarse levels first on the axis: Years, Quarters, then the base field.
        let new: Vec<ExtraField> = lv[1..].iter().rev().map(|l| ExtraField { name: l.label().into(), base: b, group: g(*l) }).collect();
        let names = self.fields(wb);
        self.add_extra(b, new, &names);
    }

    /// Groups a source field's numbers in ranges.
    pub fn group_range(&mut self, f: usize, start: f64, end: f64, by: f64, auto_start: bool, auto_end: bool) {
        let b = self.base_of(f);
        self.ungroup(b);
        self.groups.insert(b, Grouping::Range { start, end, by: if by > 0.0 { by } else { 1.0 }, auto_start, auto_end });
    }

    /// Groups some items of a field under a name (a discrete grouping field over it).
    pub fn group_items(&mut self, wb: &Workbook, f: usize, name: &str, items: Vec<String>) {
        let b = self.base_of(f);
        let n = self.n_source();
        if let Some(j) = self.extra.iter().position(|e| e.base == b && matches!(e.group, Grouping::Discrete(_))) {
            if let Grouping::Discrete(gs) = &mut self.extra[j].group { for g in gs.iter_mut() { g.1.retain(|i| !items.contains(i)); } gs.retain(|g| !g.1.is_empty()); gs.push((name.into(), items)); }
            let _ = n;
            return;
        }
        if self.groups.contains_key(&b) { return; }
        let names = self.fields(wb);
        let stem = names.get(b).cloned().unwrap_or_default();
        self.add_extra(b, vec![ExtraField { name: format!("{stem}2"), base: b, group: Grouping::Discrete(vec![(name.into(), items)]) }], &names);
    }

    /// Adds a calculated field; returns its field number.
    pub fn add_calc(&mut self, name: &str, formula: &str) -> usize {
        let f = formula.trim();
        self.calcs.push(CalcField { name: name.into(), formula: f.strip_prefix('=').unwrap_or(f).trim().into() });
        self.n_fields() - 1
    }

    pub fn remove_calc(&mut self, c: usize) {
        if c >= self.calcs.len() { return; }
        let at = self.n_source() + self.extra.len() + c;
        self.remap(&|x| if x == at { None } else if x > at { Some(x - 1) } else { Some(x) });
        self.calcs.remove(c);
    }

    // ---- running ----

    /// Runs the pivot over the source.
    pub fn compute(&self, wb: &Workbook) -> Report {
        if wb.sheet_index(&self.source_sheet).is_none() { return Report::default(); }
        let p = self.refreshed(wb);
        p.run(wb)
    }

    fn run(&self, wb: &Workbook) -> Report {
        let d1904 = wb.date1904;
        let names = self.fields(wb);
        let n = self.n_source();
        let src_names: Vec<String> = names.iter().take(n).cloned().collect();
        let calc_ex: Vec<Option<Ex>> = self.calcs.iter().map(|c| parse_calc(&c.formula, &src_names).ok()).collect();
        let has_calc = self.values.iter().any(|v| self.calc_index(v.field).is_some());
        // Records that pass the filters.
        let recs: Vec<Vec<Value>> = self.records(wb).into_iter().filter(|rec| !self.hidden.iter().any(|(f, h)| !h.is_empty() && h.contains(&self.item(rec, *f, d1904).0))).collect();
        let keys_of = |fields: &[usize]| -> Vec<Vec<Item>> {
            let mut set: BTreeMap<Vec<String>, Vec<Item>> = BTreeMap::new();
            for rec in &recs { let k: Vec<Item> = fields.iter().map(|f| self.item(rec, *f, d1904)).collect(); set.entry(k.iter().map(|x| x.0.clone()).collect()).or_insert(k); }
            let mut v: Vec<Vec<Item>> = set.into_values().collect();
            v.sort_by(|a, b| a.iter().zip(b).map(|(x, y)| cmp_item(x, y)).find(|o| o.is_ne()).unwrap_or(Ordering::Equal));
            v
        };
        let items_at = |keys: &[Vec<Item>], d: usize| -> Vec<String> {
            let mut seen: BTreeMap<String, Item> = BTreeMap::new();
            for k in keys { if let Some(it) = k.get(d) { seen.entry(it.0.clone()).or_insert(it.clone()); } }
            let mut v: Vec<Item> = seen.into_values().collect();
            v.sort_by(cmp_item);
            v.into_iter().map(|x| x.0).collect()
        };
        let row_keys = keys_of(&self.rows);
        let mut col_keys = keys_of(&self.cols);
        if self.cols.is_empty() { col_keys = vec![vec![]]; }
        let nv = self.values.len().max(1);
        let text = |k: &[Item]| -> Vec<String> { k.iter().map(|x| x.0.clone()).collect() };
        // Accumulators by (row key prefix, column key) for every prefix length (subtotals).
        let mut cells: HashMap<CellKey, (Vec<Acc>, Vec<f64>)> = HashMap::new();
        for rec in &recs {
            let rk: Vec<String> = self.rows.iter().map(|f| self.item(rec, *f, d1904).0).collect();
            let ck: Vec<String> = self.cols.iter().map(|f| self.item(rec, *f, d1904).0).collect();
            for depth in 0..=rk.len() {
                for c in [Some(ck.clone()), None] {
                    let e = cells.entry((rk[..depth].to_vec(), c)).or_insert_with(|| (vec![Acc::default(); nv], vec![0.0; if has_calc { n } else { 0 }]));
                    for (i, vf) in self.values.iter().enumerate() { if vf.field < n + self.extra.len() { e.0[i].add(rec.get(self.base_of(vf.field)).unwrap_or(&Value::Empty)); } }
                    if self.values.is_empty() { e.0[0].count += 1; }
                    if has_calc { for (k, v) in rec.iter().enumerate().take(n) { if let Value::Num(x) = v { e.1[k] += x; } } }
                }
            }
        }
        let grid = Grid { p: self, n, cells, calc_ex, row_items: (0..self.rows.len()).map(|d| items_at(&row_keys, d)).collect(), col_items: (0..self.cols.len()).map(|d| items_at(&col_keys, d)).collect() };
        let _ = grid.n;
        let mut out = Report { nv, ..Default::default() };
        let push = |out: &mut Report, row: Vec<Value>, role: Role, key: Vec<String>| {
            out.cells.push(row); out.roles.push(role); out.keys.push(key);
            if matches!(role, Role::Header | Role::Group | Role::Subtotal | Role::Grand) { out.bold_rows.push(out.cells.len() - 1); }
        };
        // Report filters, then a blank row.
        for f in &self.filters {
            let h = self.hidden.get(f);
            let all = self.items(wb, *f);
            let shown: Vec<&String> = all.iter().filter(|i| !h.is_some_and(|h| h.contains(*i))).collect();
            let state = if h.is_none_or(|h| h.is_empty()) { "(All)".to_string() } else if shown.len() == 1 { shown[0].clone() } else { "(Multiple Items)".into() };
            push(&mut out, vec![Value::str(names.get(*f).cloned().unwrap_or_default()), Value::str(state)], Role::Filter, vec![]);
        }
        if !self.filters.is_empty() { push(&mut out, vec![], Role::Blank, vec![]); }
        let compact = self.layout == Layout::Compact;
        let tabular = self.layout == Layout::Tabular;
        let lead = if compact { 1 } else { self.rows.len().max(1) };
        out.lead = lead;
        let vname = |i: usize| self.values.get(i).map(|v| self.value_name(wb, v)).unwrap_or_else(|| "Count".into());
        let multi = self.values.len() > 1;
        let nck = col_keys.len();
        let grand_col = !self.cols.is_empty() && self.row_grand;
        let ncols = lead + nck * nv + if grand_col { nv } else { 0 };
        let row_names = |row: &mut Vec<Value>| {
            if compact { if !self.rows.is_empty() { row[0] = Value::str("Row Labels"); } } else { for (i, f) in self.rows.iter().enumerate() { row[i] = Value::str(names.get(*f).cloned().unwrap_or_default()); } }
        };
        // Header rows: one per column field (and one for the value names when several).
        if self.cols.is_empty() {
            let mut row = vec![Value::Empty; ncols];
            row_names(&mut row);
            for i in 0..nv { row[lead + i] = Value::str(vname(i)); }
            out.row_head = Some(out.cells.len());
            push(&mut out, row, Role::Header, vec![]);
        } else {
            let mut row = vec![Value::Empty; ncols];
            row[0] = Value::str(if multi { String::new() } else { vname(0) });
            if compact { row[lead] = Value::str("Column Labels"); } else { for (i, f) in self.cols.iter().enumerate() { if lead + i < ncols { row[lead + i] = Value::str(names.get(*f).cloned().unwrap_or_default()); } } }
            push(&mut out, row, Role::Header, vec![]);
            for h in 0..self.cols.len() {
                let mut row = vec![Value::Empty; ncols];
                if h + 1 == self.cols.len() && !multi { row_names(&mut row); out.row_head = Some(out.cells.len()); }
                let mut prev: Option<&Vec<Item>> = None;
                for (k, ck) in col_keys.iter().enumerate() {
                    if prev.is_none_or(|p| text(&p[..=h]) != text(&ck[..=h])) { row[lead + k * nv] = ck[h].1.clone().pipe_blank(); }
                    prev = Some(ck);
                }
                if h == 0 && grand_col { row[lead + nck * nv] = Value::str(if multi { format!("Total {}", vname(0)) } else { "Grand Total".into() }); }
                out.col_heads.push((out.cells.len(), self.cols[h]));
                push(&mut out, row, Role::Header, vec![]);
            }
            if multi {
                let mut row = vec![Value::Empty; ncols];
                row_names(&mut row);
                for k in 0..nck { for i in 0..nv { row[lead + k * nv + i] = Value::str(vname(i)); } }
                if grand_col { for i in 0..nv { row[lead + nck * nv + i] = Value::str(format!("Total {}", vname(i))); } }
                out.row_head = Some(out.cells.len());
                push(&mut out, row, Role::Header, vec![]);
            }
        }
        // Value columns and their formats.
        let col_texts: Vec<Vec<String>> = col_keys.iter().map(|k| text(k)).collect();
        for k in 0..nck + grand_col as usize { for i in 0..nv { if let Some(f) = self.values.get(i).and_then(|v| v.format()) { out.col_fmts.insert(lead + k * nv + i, f); } } }
        let put = |row: &mut Vec<Value>, rk: &[String]| {
            for (k, ck) in col_texts.iter().enumerate() { for i in 0..nv { row[lead + k * nv + i] = grid.shown(rk, Some(ck), i); } }
            if grand_col { for i in 0..nv { row[lead + nck * nv + i] = grid.shown(rk, None, i); } }
        };
        // Body rows.
        let depth = self.rows.len();
        let sub_top = self.subtotals == Subtotals::Top && !tabular;
        let sub_bottom = self.subtotals == Subtotals::Bottom || (self.subtotals == Subtotals::Top && tabular);
        let label_col = |d: usize| if compact { 0 } else { d };
        let mut prev: Option<&Vec<Item>> = None;
        for (ni, rk) in row_keys.iter().enumerate().filter(|_| depth > 0) {
            let texts = text(rk);
            let starts = |d: usize| prev.is_none_or(|p| text(&p[..=d]) != texts[..=d]);
            // Group header rows (Compact and Outline).
            if !tabular {
                for d in 0..depth.saturating_sub(1) {
                    if !starts(d) { continue; }
                    let mut row = vec![Value::Empty; ncols];
                    row[label_col(d)] = rk[d].1.clone().pipe_blank();
                    if self.repeat_labels && !compact { for e in 0..d { row[e] = rk[e].1.clone().pipe_blank(); } }
                    if sub_top { put(&mut row, &texts[..=d]); }
                    if compact { out.indents.push((out.cells.len(), 0, d as u8)); }
                    push(&mut out, row, Role::Group, texts[..=d].to_vec());
                }
            }
            let mut row = vec![Value::Empty; ncols];
            let last = depth - 1;
            row[label_col(last)] = rk[last].1.clone().pipe_blank();
            if compact { out.indents.push((out.cells.len(), 0, last as u8)); }
            if !compact { for d in 0..last { if self.repeat_labels || (tabular && starts(d)) { row[d] = rk[d].1.clone().pipe_blank(); } } }
            put(&mut row, &texts);
            push(&mut out, row, Role::Data, texts.clone());
            // Close the groups this row ends.
            let next = row_keys.get(ni + 1);
            for d in (0..depth.saturating_sub(1)).rev() {
                if next.is_none_or(|nx| text(&nx[..=d]) != texts[..=d]) {
                    if sub_bottom {
                        let mut row = vec![Value::Empty; ncols];
                        row[label_col(d)] = Value::str(format!("{} Total", rk[d].0));
                        put(&mut row, &texts[..=d]);
                        if compact { out.indents.push((out.cells.len(), 0, d as u8)); }
                        push(&mut out, row, Role::Subtotal, texts[..=d].to_vec());
                    }
                    if self.blank_rows { push(&mut out, vec![], Role::Blank, texts[..=d].to_vec()); }
                }
            }
            prev = Some(rk);
        }
        if self.col_grand || self.rows.is_empty() {
            let mut total = vec![Value::Empty; ncols];
            total[0] = Value::str("Grand Total");
            put(&mut total, &[]);
            push(&mut out, total, Role::Grand, vec![]);
        }
        out.col_keys = col_texts;
        out
    }

    /// The field or value field a report cell (offset from the anchor) belongs to.
    pub fn hit(&self, wb: &Workbook, r: usize, c: usize) -> Option<Hit> {
        let rep = self.compute(wb);
        let role = *rep.roles.get(r)?;
        if role == Role::Filter { return self.filters.get(r).map(|f| Hit::Field(*f)); }
        if let Some((_, f)) = rep.col_heads.iter().find(|(row, _)| *row == r) { if c >= rep.lead { return Some(Hit::Field(*f)); } }
        if rep.row_head == Some(r) && c < rep.lead { return self.rows.get(if self.layout == Layout::Compact { 0 } else { c }).map(|f| Hit::Field(*f)); }
        if c >= rep.lead { return matches!(role, Role::Data | Role::Group | Role::Subtotal | Role::Grand | Role::Header).then(|| Hit::Value((c - rep.lead) % rep.nv)); }
        if matches!(role, Role::Data | Role::Group | Role::Subtotal) {
            let d = if self.layout == Layout::Compact { rep.keys[r].len().saturating_sub(1) } else { c };
            return self.rows.get(d).map(|f| Hit::Field(*f));
        }
        None
    }

    /// Writes the report at the anchor of sheet `dest` (clearing the previous output); the
    /// caller recalculates.
    pub fn apply(&mut self, wb: &mut Workbook, dest: usize) {
        if let Some(o) = self.output.take() { wb.clear(dest, o, true, true, false); }
        if self.rows.is_empty() && self.cols.is_empty() && self.values.is_empty() && self.filters.is_empty() { return; }
        let rep = self.compute(wb);
        let width = rep.cells.iter().map(|r| r.len()).max().unwrap_or(1);
        let colors = style_colors(&wb.theme_colors, &self.style.name);
        let base = wb.styles.get(0).clone();
        let (mut r1, mut c1) = (self.anchor.row, self.anchor.col);
        let mut band = 0usize;
        for (i, row) in rep.cells.iter().enumerate() {
            let role = rep.roles[i];
            if role == Role::Data { band += 1; }
            let styled_row = !matches!(role, Role::Filter | Role::Blank);
            for j in 0..if styled_row { width } else { row.len() } {
                let v = row.get(j).cloned().unwrap_or_default();
                let p = Pos::new(self.anchor.row + i as u32, self.anchor.col + j as u32);
                if p.row >= crate::addr::MAX_ROWS || p.col >= crate::addr::MAX_COLS { continue; }
                let mut st = base.clone();
                if rep.bold_rows.contains(&i) || (role == Role::Filter && j == 0) { st.bold = true; }
                if let Some(&(_, _, lv)) = rep.indents.iter().find(|x| x.0 == i && x.1 == j) { st.indent = lv; }
                if matches!(role, Role::Data | Role::Group | Role::Subtotal | Role::Grand) && j >= rep.lead { if let Some(f) = rep.col_fmts.get(&j) { st.num_fmt = f.clone(); } }
                if let Some([hf, ht, bf, tf, df, dt, line]) = colors {
                    match role {
                        Role::Header if self.style.col_headers => { st.fill = hf; st.color = ht; st.bold = true; }
                        Role::Grand => { st.fill = tf; st.color = if df.is_some() { ht } else { None }; st.top = Border { style: LineStyle::Double, color: line }; }
                        Role::Group | Role::Subtotal => { st.fill = if df.is_some() { tf } else { bf.filter(|_| self.style.row_headers) }; st.color = dt; }
                        Role::Data => {
                            st.fill = df; st.color = dt;
                            if self.style.banded_rows && band % 2 == 0 { st.fill = bf; }
                            if self.style.banded_cols && j >= rep.lead && ((j - rep.lead) / rep.nv.max(1)) % 2 == 1 { st.fill = bf; }
                            if self.style.row_headers && j < rep.lead && df.is_none() { st.bold = true; }
                        }
                        _ => {}
                    }
                }
                if v.is_empty() && st == base { continue; }
                if !v.is_empty() { wb.set_cell_input(dest, p, Input::Value(v.clone()), None); }
                if st != base { let k = wb.styles.intern(st); wb.sheets[dest].cells.entry(p).or_default().style = k; }
                r1 = r1.max(p.row); c1 = c1.max(p.col);
            }
        }
        self.output = Some(Range { r0: self.anchor.row, c0: self.anchor.col, r1, c1 });
    }

    // ---- persistence (legacy defined-name specs: fields, areas, plain value fields and filters) ----

    pub fn to_spec(&self) -> String {
        let list = |v: &[usize]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
        let mut s = format!("v1|{}|{}!{}|{}|{}|{}|{}|", esc(&self.name), quote_sheet(&self.source_sheet), self.source.a1(), self.anchor.a1(), list(&self.rows), list(&self.cols), list(&self.filters));
        s.push_str(&self.values.iter().map(|v| format!("{}:{}", v.field, v.agg.code().to_lowercase())).collect::<Vec<_>>().join(","));
        s.push('|');
        s.push_str(&self.hidden.iter().filter(|(_, h)| !h.is_empty()).map(|(f, h)| format!("{f}:{}", h.iter().map(|x| esc(x)).collect::<Vec<_>>().join(";"))).collect::<Vec<_>>().join(","));
        s.push('|');
        if let Some(o) = self.output { s.push_str(&o.a1()); }
        s
    }

    pub fn from_spec(spec: &str) -> Option<Pivot> {
        let f: Vec<&str> = spec.split('|').collect();
        if f.len() < 10 || f[0] != "v1" { return None; }
        let (sheet, range) = crate::chart::parse_ref(f[2])?;
        let list = |s: &str| -> Vec<usize> { s.split(',').filter_map(|x| x.parse().ok()).collect() };
        // Old specs were written with the tabular layout and subtotals at the bottom.
        let mut p = Pivot { name: unesc(f[1]), source_sheet: sheet?, source: range, anchor: crate::addr::parse_a1(f[3])?, rows: list(f[4]), cols: list(f[5]), filters: list(f[6]), layout: Layout::Tabular, subtotals: Subtotals::Bottom, ..Default::default() };
        p.values = f[7].split(',').filter_map(|v| { let (a, b) = v.split_once(':')?; Some(ValueField::new(a.parse().ok()?, Agg::ALL.into_iter().find(|x| x.code().eq_ignore_ascii_case(b)).unwrap_or_default())) }).collect();
        for h in f[8].split(',').filter(|h| !h.is_empty()) {
            let Some((a, b)) = h.split_once(':') else { continue };
            let Ok(k) = a.parse() else { continue };
            p.hidden.insert(k, b.split(';').map(unesc).collect());
        }
        p.output = parse_range(f[9]);
        Some(p)
    }
}

trait PipeBlank { fn pipe_blank(self) -> Value; }
impl PipeBlank for Value { fn pipe_blank(self) -> Value { if self.is_empty() { Value::str("(blank)") } else { self } } }

fn esc(s: &str) -> String { s.replace('%', "%25").replace('|', "%7C").replace(',', "%2C").replace(';', "%3B").replace(':', "%3A").replace('"', "%22") }
fn unesc(s: &str) -> String { s.replace("%22", "\"").replace("%3A", ":").replace("%3B", ";").replace("%2C", ",").replace("%7C", "|").replace("%25", "%") }

/// The name prefix of a stored pivot definition.
pub const NAME_PREFIX: &str = "_JadwalPivot";

/// A spec as a defined-name formula: string pieces of at most 250 characters joined with &.
pub fn spec_formula(spec: &str) -> String {
    let chars: Vec<char> = spec.chars().collect();
    let parts: Vec<String> = chars.chunks(250).map(|c| format!("\"{}\"", c.iter().collect::<String>())).collect();
    if parts.is_empty() { "\"\"".into() } else { parts.join("&") }
}

/// The spec back from a stored formula.
pub fn formula_spec(f: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    let mut chars = f.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' { if inside && chars.peek() == Some(&'"') { out.push('"'); chars.next(); } else { inside = !inside; } } else if inside { out.push(c); }
    }
    out
}

/// Moves pivot definitions from the workbook's hidden names onto their sheets (after reading).
pub fn load(wb: &mut Workbook) {
    let mut keep = vec![];
    for n in std::mem::take(&mut wb.names) {
        if n.name.starts_with(NAME_PREFIX) {
            if let (Some(si), Some(p)) = (n.sheet, Pivot::from_spec(&formula_spec(&n.formula))) { if si < wb.sheets.len() { wb.sheets[si].pivots.push(p); continue; } }
        }
        keep.push(n);
    }
    wb.names = keep;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        let rows = [["Region", "Product", "Sales", "Qty"], ["East", "A", "10", "1"], ["West", "A", "20", "2"], ["East", "B", "5", "3"], ["East", "A", "7", "4"], ["West", "B", "", "5"]];
        for (i, row) in rows.iter().enumerate() { for (j, t) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, j as u32), t); } }
        wb.recalc();
        wb
    }
    fn n(v: &Value) -> f64 { if let Value::Num(x) = v { *x } else { f64::NAN } }
    fn s(v: &str) -> Value { Value::str(v) }
    fn tabular(mut p: Pivot) -> Pivot { p.layout = Layout::Tabular; p.subtotals = Subtotals::Bottom; p }

    #[test]
    fn rows_and_values() {
        let wb = book();
        let mut p = Pivot::new("P1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(0, 0));
        p.rows = vec![0];
        p.values = vec![ValueField::new(2, Agg::Sum), ValueField::new(3, Agg::Count)];
        let r = p.compute(&wb);
        assert_eq!(r.cells[0][0], s("Row Labels"));
        assert_eq!(r.cells[0][1], s("Sum of Sales"));
        assert_eq!(r.cells[1][0], s("East"));
        assert_eq!(n(&r.cells[1][1]), 22.0);
        assert_eq!(n(&r.cells[1][2]), 3.0);
        assert_eq!(n(&r.cells[2][1]), 20.0);
        assert_eq!(r.cells[3][0], s("Grand Total"));
        assert_eq!(n(&r.cells[3][1]), 42.0);
        assert_eq!(n(&r.cells[3][2]), 5.0);
        let r = tabular(p).compute(&wb);
        assert_eq!(r.cells[0][0], s("Region"));
    }

    #[test]
    fn columns_subtotals_filters() {
        let wb = book();
        let mut p = tabular(Pivot::new("P1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(0, 0)));
        p.rows = vec![0, 1];
        p.cols = vec![1];
        p.values = vec![ValueField::new(2, Agg::Sum)];
        let r = p.compute(&wb);
        assert_eq!(r.cells[1][2], s("A"));
        assert_eq!(r.cells[1][4], s("Grand Total"));
        assert_eq!(r.cells[2][0], s("East"));
        assert_eq!(n(&r.cells[2][2]), 17.0);
        let tot = r.cells.iter().find(|row| row[0] == s("East Total")).unwrap();
        assert_eq!(n(&tot[4]), 22.0);
        assert_eq!(n(&r.cells.last().unwrap()[4]), 42.0);
        p.filters = vec![0];
        p.rows = vec![1];
        p.cols = vec![];
        p.hidden.insert(0, ["West".to_string()].into());
        let r = p.compute(&wb);
        assert_eq!(r.cells[0], vec![s("Region"), s("East")]);
        assert_eq!(n(&r.cells.last().unwrap()[1]), 22.0);
        p.values = vec![ValueField::new(2, Agg::Average), ValueField::new(2, Agg::Max), ValueField::new(2, Agg::Min)];
        let r = p.compute(&wb);
        let g = r.cells.last().unwrap();
        assert!((n(&g[1]) - 22.0 / 3.0).abs() < 1e-9);
        assert_eq!((n(&g[2]), n(&g[3])), (10.0, 5.0));
    }

    #[test]
    fn more_summaries() {
        let wb = book();
        let mut p = Pivot::new("P1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(0, 0));
        p.values = [Agg::Product, Agg::CountNums, Agg::StdDev, Agg::StdDevp, Agg::Var, Agg::Varp, Agg::Count].iter().map(|a| ValueField::new(2, *a)).collect();
        let r = p.compute(&wb);
        let g = r.cells.last().unwrap();
        // Sales: 10, 20, 5, 7 (one blank).
        assert_eq!(n(&g[1]), 7000.0);
        assert_eq!(n(&g[2]), 4.0);
        let mean = 10.5; let ss: f64 = [10.0, 20.0, 5.0, 7.0].iter().map(|x: &f64| (x - mean).powi(2)).sum();
        assert!((n(&g[3]) - (ss / 3.0).sqrt()).abs() < 1e-9);
        assert!((n(&g[4]) - (ss / 4.0).sqrt()).abs() < 1e-9);
        assert!((n(&g[5]) - ss / 3.0).abs() < 1e-9);
        assert!((n(&g[6]) - ss / 4.0).abs() < 1e-9);
        assert_eq!(n(&g[7]), 4.0);
    }

    #[test]
    fn layouts() {
        let wb = book();
        let mut p = Pivot::new("P1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(0, 0));
        p.rows = vec![0, 1];
        p.values = vec![ValueField::new(2, Agg::Sum)];
        // Compact: one label column, group rows carrying subtotals, indented items.
        let r = p.compute(&wb);
        let col0: Vec<Value> = r.cells.iter().map(|x| x[0].clone()).collect();
        assert_eq!(col0, vec![s("Row Labels"), s("East"), s("A"), s("B"), s("West"), s("A"), s("B"), s("Grand Total")]);
        assert_eq!(n(&r.cells[1][1]), 22.0);
        assert_eq!(r.roles[1], Role::Group);
        assert!(r.indents.contains(&(2, 0, 1)));
        assert_eq!(r.cells[0].len(), 2);
        // Outline: a column per field, group rows on their own row.
        p.layout = Layout::Outline;
        let r = p.compute(&wb);
        assert_eq!(r.cells[0], vec![s("Region"), s("Product"), s("Sum of Sales")]);
        assert_eq!(r.cells[1], vec![s("East"), Value::Empty, Value::Num(22.0)]);
        assert_eq!(r.cells[2], vec![Value::Empty, s("A"), Value::Num(17.0)]);
        // Repeat item labels fill the outer column.
        p.repeat_labels = true;
        let r = p.compute(&wb);
        assert_eq!(r.cells[2][0], s("East"));
        // Tabular with subtotals at the bottom, a blank row after each item.
        p.layout = Layout::Tabular;
        p.repeat_labels = false;
        p.blank_rows = true;
        let r = p.compute(&wb);
        assert_eq!(r.cells[1], vec![s("East"), s("A"), Value::Num(17.0)]);
        assert_eq!(r.cells[2], vec![Value::Empty, s("B"), Value::Num(5.0)]);
        assert_eq!(r.cells[3], vec![s("East Total"), Value::Empty, Value::Num(22.0)]);
        assert!(r.cells[4].is_empty());
        // Subtotals off, no grand totals.
        p.blank_rows = false;
        p.subtotals = Subtotals::Off;
        p.col_grand = false;
        let r = p.compute(&wb);
        assert_eq!(r.cells.len(), 5);
        assert!(r.cells.iter().all(|x| x[0] != s("East Total") && x[0] != s("Grand Total")));
        // Bottom subtotals in compact form.
        p.layout = Layout::Compact;
        p.subtotals = Subtotals::Bottom;
        let r = p.compute(&wb);
        assert_eq!(r.cells[1], vec![s("East"), Value::Empty]);
        assert_eq!(r.cells[4], vec![s("East Total"), Value::Num(22.0)]);
        // The grand total column goes with row_grand.
        p.cols = vec![1];
        p.rows = vec![0];
        p.col_grand = true;
        let r = p.compute(&wb);
        assert_eq!(r.cells[1], vec![s("Row Labels"), s("A"), s("B"), s("Grand Total")]);
        assert_eq!(r.cells[0][1], s("Column Labels"));
        p.row_grand = false;
        let r = p.compute(&wb);
        assert_eq!(r.cells[1].len(), 3);
    }

    #[test]
    fn show_values_as() {
        let wb = book();
        let mut p = Pivot::new("P1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(0, 0));
        p.rows = vec![0];
        p.cols = vec![1];
        let mut v = ValueField::new(2, Agg::Sum);
        v.show = ShowAs::PctGrand;
        p.values = vec![v.clone()];
        // East: A 17, B 5; West: A 20, B 0 (blank sales); totals 22 / 20, 42.
        let r = p.compute(&wb);
        assert!((n(&r.cells[2][1]) - 17.0 / 42.0).abs() < 1e-9);
        assert_eq!(r.col_fmts.get(&1).map(|s| s.as_str()), Some("0.00%"));
        v.show = ShowAs::PctRow; p.values = vec![v.clone()];
        let r = p.compute(&wb);
        assert!((n(&r.cells[2][1]) - 17.0 / 22.0).abs() < 1e-9);
        v.show = ShowAs::PctCol; p.values = vec![v.clone()];
        let r = p.compute(&wb);
        assert!((n(&r.cells[2][1]) - 17.0 / 37.0).abs() < 1e-9);
        v.show = ShowAs::Difference; v.base_field = 0; v.base_item = BaseItem::Item("East".into()); p.values = vec![v.clone()];
        let r = p.compute(&wb);
        assert_eq!(r.cells[2][1], Value::Empty);
        assert_eq!(n(&r.cells[3][1]), 3.0);
        v.show = ShowAs::RunTotal; p.values = vec![v.clone()];
        let r = p.compute(&wb);
        assert_eq!(n(&r.cells[3][1]), 37.0);
        assert_eq!(n(&r.cells[3][3]), 42.0);
        v.show = ShowAs::RankDesc; v.base_field = 1; p.values = vec![v.clone()];
        let r = p.compute(&wb);
        assert_eq!((n(&r.cells[2][1]), n(&r.cells[2][2])), (1.0, 2.0));
        v.show = ShowAs::RankAsc; p.values = vec![v.clone()];
        let r = p.compute(&wb);
        assert_eq!((n(&r.cells[2][1]), n(&r.cells[2][2])), (2.0, 1.0));
        // % of parent row total over two row levels.
        p.cols = vec![];
        p.rows = vec![0, 1];
        v.show = ShowAs::PctParentRow; p.values = vec![v];
        let r = p.compute(&wb);
        assert!((n(&r.cells[1][1]) - 22.0 / 42.0).abs() < 1e-9);
        assert!((n(&r.cells[2][1]) - 17.0 / 22.0).abs() < 1e-9);
        assert_eq!(n(&r.cells.last().unwrap()[1]), 1.0);
    }

    #[test]
    fn calculated_fields() {
        let wb = book();
        let mut p = Pivot::new("P1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(0, 0));
        p.rows = vec![0];
        let f = p.add_calc("Bonus", "=Sales*0.1+Qty");
        assert_eq!(f, 4);
        p.values = vec![ValueField::new(f, Agg::Sum)];
        let names = p.fields(&wb);
        assert_eq!(names[4], "Bonus");
        let r = p.compute(&wb);
        assert_eq!(r.cells[0][1], s("Sum of Bonus"));
        assert!((n(&r.cells[1][1]) - (2.2 + 8.0)).abs() < 1e-9);
        assert!((n(&r.cells.last().unwrap()[1]) - (4.2 + 15.0)).abs() < 1e-9);
        // Per group on the sums: a ratio is not the sum of ratios.
        p.calcs[0].formula = "Sales/Qty".into();
        let r = p.compute(&wb);
        assert!((n(&r.cells[1][1]) - 22.0 / 8.0).abs() < 1e-9);
        assert!(parse_calc("'Sales' * (2 + Qty) ^ 2", &names).is_ok());
        assert!(parse_calc("Nope*2", &names).is_err());
        p.remove_calc(0);
        assert!(p.values.is_empty() && p.calcs.is_empty());
    }

    fn dates() -> Workbook {
        let mut wb = Workbook::new();
        let rows = [["Date", "Amount"], ["2024-01-15", "10"], ["2024-02-03", "20"], ["2024-05-20", "5"], ["2025-01-02", "7"], ["2025-11-30", "1"]];
        for (i, row) in rows.iter().enumerate() { for (j, t) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, j as u32), t); } }
        wb.recalc();
        wb
    }

    #[test]
    fn date_grouping() {
        let wb = dates();
        assert!(matches!(wb.sheets[0].value(Pos::new(1, 0)), Value::Num(_)));
        let mut p = tabular(Pivot::new("P1", "Sheet1", parse_range("A1:B6").unwrap(), Pos::new(0, 0)));
        p.rows = vec![0];
        p.values = vec![ValueField::new(1, Agg::Sum)];
        p.group_dates(&wb, 0, &[DateLevel::Years, DateLevel::Quarters, DateLevel::Months], 0.0, 0.0, 1.0, true, true);
        assert_eq!(p.extra.len(), 2);
        assert_eq!(p.rows, vec![2, 3, 0]);
        assert_eq!(p.fields(&wb), vec!["Date", "Amount", "Years", "Quarters"]);
        let r = p.compute(&wb);
        assert_eq!(r.cells[0], vec![s("Years"), s("Quarters"), s("Date"), s("Sum of Amount")]);
        assert_eq!(r.cells[1], vec![s("2024"), s("Qtr1"), s("Jan"), Value::Num(10.0)]);
        assert_eq!(r.cells[2], vec![Value::Empty, Value::Empty, s("Feb"), Value::Num(20.0)]);
        assert!(r.cells.iter().any(|x| x[1] == s("Qtr1 Total") && x[3] == Value::Num(30.0)));
        assert!(r.cells.iter().any(|x| x[0] == s("2025 Total") && x[3] == Value::Num(8.0)));
        assert_eq!(p.items(&wb, 0), vec!["Jan", "Feb", "May", "Nov"]);
        // Ungrouping removes the extra fields.
        p.ungroup(2);
        assert_eq!((p.extra.len(), p.rows.clone()), (0, vec![0]));
        // Days in steps of 7.
        p.group_dates(&wb, 0, &[DateLevel::Days], 45306.0, 46000.0, 7.0, false, false);
        let items = p.items(&wb, 0);
        assert_eq!(items[0], "1/15/2024 - 1/21/2024");
        // Months alone merge years.
        p.group_dates(&wb, 0, &[DateLevel::Months], 0.0, 0.0, 1.0, true, true);
        let r = p.compute(&wb);
        assert_eq!(r.cells[1], vec![s("Jan"), Value::Num(17.0)]);
    }

    #[test]
    fn number_grouping() {
        let wb = book();
        let mut p = tabular(Pivot::new("P1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(0, 0)));
        p.rows = vec![3];
        p.values = vec![ValueField::new(2, Agg::Sum)];
        p.group_range(3, 1.0, 4.0, 2.0, false, false);
        let r = p.compute(&wb);
        // Qty 1..5: 1-2 (10+20), 3-4 (5+7), >4 (blank sales).
        assert_eq!(r.cells[1], vec![s("1-2"), Value::Num(30.0)]);
        assert_eq!(r.cells[2], vec![s("3-4"), Value::Num(12.0)]);
        assert_eq!(r.cells[3], vec![s(">4"), Value::Num(0.0)]);
        assert_eq!(Grouping::Range { start: 1.0, end: 4.0, by: 2.0, auto_start: false, auto_end: false }.all_items(false), vec!["<1", "1-2", "3-4", ">4"]);
        // Discrete groups.
        p.ungroup(3);
        p.rows = vec![0];
        p.group_items(&wb, 0, "Everywhere", vec!["East".into(), "West".into()]);
        assert_eq!(p.rows, vec![4, 0]);
        let r = p.compute(&wb);
        assert_eq!(r.cells[1][0], s("Everywhere"));
    }

    #[test]
    fn apply_and_spec() {
        let mut wb = book();
        wb.add_sheet(1, Some("Pivot".into()));
        let mut p = tabular(Pivot::new("PivotTable1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(2, 0)));
        p.rows = vec![1];
        p.values = vec![ValueField::new(2, Agg::Sum)];
        p.hidden.insert(0, ["We|st, x".to_string()].into());
        p.apply(&mut wb, 1);
        assert_eq!(wb.display(1, Pos::new(3, 0)), "A");
        assert_eq!(wb.display(1, Pos::new(5, 1)), "42");
        // The style fills the header.
        assert!(wb.style_at(1, Pos::new(2, 1)).fill.is_some());
        let back = Pivot::from_spec(&formula_spec(&spec_formula(&p.to_spec()))).unwrap();
        assert_eq!(back, p);
        wb.sheets[1].pivots.push(p);
        let bytes = crate::xlsx::write(&wb).unwrap();
        let wb2 = crate::xlsx::read(&bytes).unwrap();
        assert_eq!(wb2.sheets[1].pivots.len(), 1);
        assert!(wb2.names.iter().all(|n| !n.name.starts_with(NAME_PREFIX)));
    }
}
