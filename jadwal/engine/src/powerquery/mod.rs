//! Power Query (Get & Transform) as Excel keeps it (queries.js, query-editor.js): each query is a
//! source and an ordered list of applied steps, written in the Power Query formula language (M)
//! as Excel's editor writes it, and stored in the workbook's DataMashup part ([MS-QDEFF],
//! mashup.rs) with a `Microsoft.Mashup.OleDb.1` connection (Location = the query's name) and a
//! query table tying the connection to the table the query loads to.
//!
//! M that Jadwal doesn't model is kept exactly as written: the query is listed and preserved, but
//! read-only and not refreshed here.

pub mod eval;
pub mod mashup;
pub mod mlang;

use crate::book::Workbook;
use eval::{Ctx, Table, V};
use mlang::{ident, num_text, print, quote, BOp, Expr, UOp};

/// A column type, as Table.TransformColumnTypes names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty { Any, Text, Int, Number, Currency, Percentage, Date, DateTime, Time, Logical }

impl Ty {
    pub const ALL: [Ty; 10] = [Ty::Number, Ty::Currency, Ty::Int, Ty::Percentage, Ty::DateTime, Ty::Date, Ty::Time, Ty::Logical, Ty::Text, Ty::Any];
    pub fn m(self) -> &'static str {
        match self { Ty::Any => "type any", Ty::Text => "type text", Ty::Int => "Int64.Type", Ty::Number => "type number", Ty::Currency => "Currency.Type", Ty::Percentage => "Percentage.Type", Ty::Date => "type date", Ty::DateTime => "type datetime", Ty::Time => "type time", Ty::Logical => "type logical" }
    }
    /// The name in the Data Type menu.
    pub fn label(self) -> &'static str {
        match self { Ty::Any => "Any", Ty::Text => "Text", Ty::Int => "Whole Number", Ty::Number => "Decimal Number", Ty::Currency => "Fixed Decimal Number", Ty::Percentage => "Percentage", Ty::Date => "Date", Ty::DateTime => "Date/Time", Ty::Time => "Time", Ty::Logical => "True/False" }
    }
    /// The mark a column header shows for its type.
    pub fn icon(self) -> &'static str {
        match self { Ty::Any => "ABC\n123", Ty::Text => "ABC", Ty::Int => "123", Ty::Number => "1.2", Ty::Currency => "$", Ty::Percentage => "%", Ty::Date => "▦", Ty::DateTime => "▦◷", Ty::Time => "◷", Ty::Logical => "✓✗" }
    }
    fn from_expr(e: &Expr) -> Option<Ty> {
        match e {
            Expr::Type(t) => Some(match t.trim_start_matches("nullable ") { "any" | "anynonnull" => Ty::Any, "text" => Ty::Text, "number" => Ty::Number, "date" => Ty::Date, "datetime" => Ty::DateTime, "time" => Ty::Time, "logical" => Ty::Logical, _ => return None }),
            Expr::Ident(n) => Some(match n.as_str() { "Int64.Type" => Ty::Int, "Currency.Type" => Ty::Currency, "Percentage.Type" => Ty::Percentage, "Number.Type" => Ty::Number, "Text.Type" => Ty::Text, "Date.Type" => Ty::Date, "DateTime.Type" => Ty::DateTime, "Time.Type" => Ty::Time, "Logical.Type" => Ty::Logical, _ => return None }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggOp { Sum, Average, Median, Min, Max, CountRows, CountDistinct }

impl AggOp {
    pub const ALL: [AggOp; 7] = [AggOp::Sum, AggOp::Average, AggOp::Median, AggOp::Min, AggOp::Max, AggOp::CountRows, AggOp::CountDistinct];
    pub fn label(self) -> &'static str { match self { AggOp::Sum => "Sum", AggOp::Average => "Average", AggOp::Median => "Median", AggOp::Min => "Min", AggOp::Max => "Max", AggOp::CountRows => "Count Rows", AggOp::CountDistinct => "Count Distinct Rows" } }
    fn list_fn(self) -> &'static str { match self { AggOp::Sum => "List.Sum", AggOp::Average => "List.Average", AggOp::Median => "List.Median", AggOp::Min => "List.Min", AggOp::Max => "List.Max", AggOp::CountRows => "List.Count", AggOp::CountDistinct => "List.NonNullCount" } }
    fn from_list_fn(n: &str) -> Option<AggOp> { AggOp::ALL.into_iter().find(|a| a.list_fn() == n) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Agg { pub name: String, pub op: AggOp, pub column: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join { LeftOuter, RightOuter, FullOuter, Inner, LeftAnti, RightAnti }

impl Join {
    pub const ALL: [Join; 6] = [Join::LeftOuter, Join::RightOuter, Join::FullOuter, Join::Inner, Join::LeftAnti, Join::RightAnti];
    pub fn m(self) -> &'static str { match self { Join::LeftOuter => "JoinKind.LeftOuter", Join::RightOuter => "JoinKind.RightOuter", Join::FullOuter => "JoinKind.FullOuter", Join::Inner => "JoinKind.Inner", Join::LeftAnti => "JoinKind.LeftAnti", Join::RightAnti => "JoinKind.RightAnti" } }
    pub fn label(self) -> &'static str {
        match self { Join::LeftOuter => "Left Outer (all from first, matching from second)", Join::RightOuter => "Right Outer (all from second, matching from first)", Join::FullOuter => "Full Outer (all rows from both)", Join::Inner => "Inner (only matching rows)", Join::LeftAnti => "Left Anti (rows only in first)", Join::RightAnti => "Right Anti (rows only in second)" }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitMode { Each, Left, Right }

#[derive(Clone, Debug, PartialEq)]
pub enum Split { Delimiter { delimiter: String, mode: SplitMode }, Positions(Vec<usize>) }

/// What a step does.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    PromoteHeaders,
    DemoteHeaders,
    ChangeType(Vec<(String, Ty)>),
    RemoveColumns(Vec<String>),
    SelectColumns(Vec<String>),
    RenameColumns(Vec<(String, String)>),
    ReorderColumns(Vec<String>),
    /// Rows for which the condition (the body of `each`) is true.
    FilterRows(Expr),
    /// (column, descending)
    Sort(Vec<(String, bool)>),
    /// `text`: Replacer.ReplaceText (parts of text) rather than Replacer.ReplaceValue (whole values).
    ReplaceValue { find: V, replace: V, text: bool, columns: Vec<String> },
    SplitColumn { column: String, by: Split, into: Vec<String> },
    MergeColumns { columns: Vec<String>, separator: String, name: String },
    AddColumn { name: String, expr: Expr, ty: Option<Ty> },
    AddIndex { name: String, start: i64, step: i64 },
    Group { keys: Vec<String>, aggs: Vec<Agg> },
    Pivot { column: String, values: String, agg: Option<AggOp> },
    /// `other`: unpivot the columns not listed (Unpivot Other Columns).
    Unpivot { columns: Vec<String>, other: bool, attribute: String, value: String },
    NestedJoin { keys: Vec<String>, other: String, other_keys: Vec<String>, column: String, kind: Join },
    ExpandTable { column: String, fields: Vec<String>, names: Vec<String> },
    Append(Vec<String>),
    Distinct(Option<Vec<String>>),
    FirstN(usize),
    LastN(usize),
    Skip(usize),
    RemoveLastN(usize),
    FillDown(Vec<String>),
    FillUp(Vec<String>),
    Transpose,
}

impl Op {
    /// The name Excel's editor gives a step of this kind.
    pub fn default_name(&self) -> String {
        match self {
            Op::PromoteHeaders => "Promoted Headers".into(), Op::DemoteHeaders => "Demoted Headers".into(), Op::ChangeType(_) => "Changed Type".into(),
            Op::RemoveColumns(_) => "Removed Columns".into(), Op::SelectColumns(_) => "Removed Other Columns".into(), Op::RenameColumns(_) => "Renamed Columns".into(),
            Op::ReorderColumns(_) => "Reordered Columns".into(), Op::FilterRows(_) => "Filtered Rows".into(), Op::Sort(_) => "Sorted Rows".into(),
            Op::ReplaceValue { .. } => "Replaced Value".into(),
            Op::SplitColumn { by: Split::Delimiter { .. }, .. } => "Split Column by Delimiter".into(), Op::SplitColumn { .. } => "Split Column by Position".into(),
            Op::MergeColumns { .. } => "Merged Columns".into(), Op::AddColumn { .. } => "Added Custom".into(), Op::AddIndex { .. } => "Added Index".into(),
            Op::Group { .. } => "Grouped Rows".into(), Op::Pivot { .. } => "Pivoted Column".into(),
            Op::Unpivot { other: true, .. } => "Unpivoted Other Columns".into(), Op::Unpivot { .. } => "Unpivoted Columns".into(),
            Op::NestedJoin { .. } => "Merged Queries".into(), Op::ExpandTable { column, .. } => format!("Expanded {column}"), Op::Append(_) => "Appended Query".into(),
            Op::Distinct(_) => "Removed Duplicates".into(), Op::FirstN(_) => "Kept First Rows".into(), Op::LastN(_) => "Kept Last Rows".into(),
            Op::Skip(_) => "Removed Top Rows".into(), Op::RemoveLastN(_) => "Removed Bottom Rows".into(), Op::FillDown(_) => "Filled Down".into(),
            Op::FillUp(_) => "Filled Up".into(), Op::Transpose => "Transposed Table".into(),
        }
    }
    /// Whether the step has settings a gear opens.
    pub fn has_settings(&self) -> bool { !matches!(self, Op::PromoteHeaders | Op::DemoteHeaders | Op::Transpose) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step { pub name: String, pub op: Op }

/// Where a query's data comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Csv.Document(File.Contents(path), [Delimiter, Columns, Encoding, QuoteStyle]).
    Csv { path: String, delimiter: String, encoding: u32, columns: Option<usize> },
    /// Excel.Workbook(File.Contents(path)), then one sheet or table (kind "Sheet" / "Table").
    Workbook { path: String, item: String, kind: String },
    /// Web.Page(Web.Contents(url)), then the table at `index` (from 0).
    Web { url: String, index: usize },
    /// Excel.CurrentWorkbook(){[Name=…]}[Content]: a table or name of this workbook.
    Current { name: String },
    /// Another query of the workbook.
    Query(String),
}

impl Source {
    /// The steps the source takes in M: (name, expression).
    fn bindings(&self) -> Vec<(String, String)> {
        match self {
            Source::Csv { path, delimiter, encoding, columns } => {
                let cols = columns.map(|n| format!(" Columns={n},")).unwrap_or_default();
                vec![("Source".into(), format!("Csv.Document(File.Contents({}),[Delimiter={},{cols} Encoding={encoding}, QuoteStyle=QuoteStyle.None])", quote(path), quote(delimiter)))]
            }
            Source::Workbook { path, item, kind } => vec![
                ("Source".into(), format!("Excel.Workbook(File.Contents({}), null, true)", quote(path))),
                (format!("{item}_{kind}"), format!("Source{{[Item={},Kind={}]}}[Data]", quote(item), quote(kind))),
            ],
            Source::Web { url, index } => vec![("Source".into(), format!("Web.Page(Web.Contents({}))", quote(url))), (format!("Data{index}"), format!("Source{{{index}}}[Data]"))],
            Source::Current { name } => vec![("Source".into(), format!("Excel.CurrentWorkbook(){{[Name={}]}}[Content]", quote(name)))],
            Source::Query(q) => vec![("Source".into(), ident(q))],
        }
    }
    /// What the Queries & Connections pane says it is.
    pub fn describe(&self) -> String {
        match self { Source::Csv { path, .. } => path.clone(), Source::Workbook { path, item, .. } => format!("{path} [{item}]"), Source::Web { url, .. } => url.clone(), Source::Current { name } => name.clone(), Source::Query(q) => q.clone() }
    }
}

/// Where Close & Load puts a query's result.
#[derive(Clone, Debug, PartialEq)]
pub enum Load { /// A table of a sheet, by the table's name.
    Table(String), ConnectionOnly }

#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub name: String,
    pub description: String,
    /// None when the formula isn't one Jadwal models (then `raw` holds it).
    pub source: Option<Source>,
    pub steps: Vec<Step>,
    /// The formula as written, for queries kept read-only.
    pub raw: Option<String>,
    pub load: Load,
    /// The metadata entries read with the query ([MS-QDEFF] 2.5.1), kept on save.
    pub meta: Vec<(String, String)>,
}

impl Query {
    pub fn new(name: &str, source: Source) -> Query { Query { name: name.into(), description: String::new(), source: Some(source), steps: vec![], raw: None, load: Load::ConnectionOnly, meta: vec![] } }
    pub fn read_only(&self) -> bool { self.source.is_none() }
    /// The names of the source's own steps (Source, and a navigation step).
    pub fn source_steps(&self) -> Vec<String> { self.source.as_ref().map(|s| s.bindings().into_iter().map(|b| b.0).collect()).unwrap_or_default() }

    /// A step name that is free: base, base1, base2… (as the editor numbers them).
    pub fn unique_step_name(&self, base: &str) -> String {
        let taken = |n: &str| self.steps.iter().any(|s| s.name.eq_ignore_ascii_case(n)) || self.source_steps().iter().any(|s| s.eq_ignore_ascii_case(n));
        if !taken(base) { return base.to_string(); }
        (1..).map(|i| format!("{base}{i}")).find(|n| !taken(n)).unwrap()
    }
    /// Adds a step after `at` (None: at the end), named as the editor names it; returns its index.
    pub fn add_step(&mut self, op: Op, at: Option<usize>) -> usize {
        let name = self.unique_step_name(&op.default_name());
        let i = at.map_or(self.steps.len(), |a| (a + 1).min(self.steps.len()));
        self.steps.insert(i, Step { name, op });
        i
    }

    /// The query's formula: let … in ….
    pub fn formula(&self) -> String {
        if let Some(r) = &self.raw { return r.clone(); }
        let Some(src) = &self.source else { return String::new() };
        let mut binds = src.bindings();
        for s in &self.steps {
            let prev = ident(&binds.last().unwrap().0);
            binds.push((s.name.clone(), op_m(&s.op, &prev)));
        }
        let mut o = String::from("let\r\n");
        for (k, (n, x)) in binds.iter().enumerate() { o.push_str(&format!("    {} = {}{}\r\n", ident(n), x, if k + 1 < binds.len() { "," } else { "" })); }
        o.push_str(&format!("in\r\n    {}", ident(&binds.last().unwrap().0)));
        o
    }

    /// The M of one step (index into `steps`; None: the source).
    pub fn step_formula(&self, step: Option<usize>) -> String {
        let Some(src) = &self.source else { return self.raw.clone().unwrap_or_default() };
        let binds = src.bindings();
        match step {
            None => binds.last().map(|b| b.1.clone()).unwrap_or_default(),
            Some(i) => { let prev = if i == 0 { binds.last().unwrap().0.clone() } else { self.steps[i - 1].name.clone() }; op_m(&self.steps[i].op, &ident(&prev)) }
        }
    }

    /// Reads a formula: a modelled query, or one kept as written (read-only).
    pub fn from_formula(name: &str, text: &str) -> Query {
        let mut q = Query { name: name.into(), description: String::new(), source: None, steps: vec![], raw: None, load: Load::ConnectionOnly, meta: vec![] };
        match mlang::parse(text).and_then(|e| steps_from(&e)) {
            Ok((src, steps)) => { q.source = Some(src); q.steps = steps; }
            Err(_) => q.raw = Some(text.trim().to_string()),
        }
        q
    }

    /// Replaces the steps after the source with the M of one step (the formula bar), or errors.
    pub fn set_step_formula(&mut self, step: usize, text: &str) -> Result<(), String> {
        let e = mlang::parse(text.trim().trim_start_matches('=').trim())?;
        let prev = if step == 0 { self.source_steps().last().cloned().unwrap_or_default() } else { self.steps[step - 1].name.clone() };
        self.steps[step].op = op_from(&e, &prev)?;
        Ok(())
    }

    /// Renames a step, following references to it.
    pub fn rename_step(&mut self, i: usize, name: &str) -> Result<(), String> {
        let n = name.trim();
        if n.is_empty() { return Err("Enter a name for the step.".into()); }
        if self.steps.iter().enumerate().any(|(k, s)| k != i && s.name.eq_ignore_ascii_case(n)) || self.source_steps().iter().any(|s| s.eq_ignore_ascii_case(n)) { return Err("A step with that name already exists.".into()); }
        self.steps[i].name = n.to_string();
        Ok(())
    }
}

// ---- M for steps ----------------------------------------------------------------------------

fn texts(v: &[String]) -> String { format!("{{{}}}", v.iter().map(|s| quote(s)).collect::<Vec<_>>().join(", ")) }
fn val(v: &V) -> String { match v { V::Null => "null".into(), V::Num(n) => num_text(*n), V::Text(s) => quote(s), V::Bool(b) => b.to_string(), _ => "null".into() } }

/// One step's expression over the previous step `prev` (already an identifier).
pub fn op_m(op: &Op, prev: &str) -> String {
    match op {
        Op::PromoteHeaders => format!("Table.PromoteHeaders({prev}, [PromoteAllScalars=true])"),
        Op::DemoteHeaders => format!("Table.DemoteHeaders({prev})"),
        Op::ChangeType(v) => format!("Table.TransformColumnTypes({prev},{{{}}})", v.iter().map(|(c, t)| format!("{{{}, {}}}", quote(c), t.m())).collect::<Vec<_>>().join(", ")),
        Op::RemoveColumns(c) => format!("Table.RemoveColumns({prev},{})", texts(c)),
        Op::SelectColumns(c) => format!("Table.SelectColumns({prev},{})", texts(c)),
        Op::ReorderColumns(c) => format!("Table.ReorderColumns({prev},{})", texts(c)),
        Op::RenameColumns(v) => format!("Table.RenameColumns({prev},{{{}}})", v.iter().map(|(a, b)| format!("{{{}, {}}}", quote(a), quote(b))).collect::<Vec<_>>().join(", ")),
        Op::FilterRows(e) => format!("Table.SelectRows({prev}, each {})", print(e)),
        Op::Sort(k) => format!("Table.Sort({prev},{{{}}})", k.iter().map(|(c, d)| format!("{{{}, {}}}", quote(c), if *d { "Order.Descending" } else { "Order.Ascending" })).collect::<Vec<_>>().join(", ")),
        Op::ReplaceValue { find, replace, text, columns } => format!("Table.ReplaceValue({prev},{},{},{},{})", val(find), val(replace), if *text { "Replacer.ReplaceText" } else { "Replacer.ReplaceValue" }, texts(columns)),
        Op::SplitColumn { column, by, into } => {
            let sp = match by {
                Split::Delimiter { delimiter, mode: SplitMode::Each } => format!("Splitter.SplitTextByDelimiter({}, QuoteStyle.Csv)", quote(delimiter)),
                Split::Delimiter { delimiter, mode } => format!("Splitter.SplitTextByEachDelimiter({{{}}}, QuoteStyle.Csv, {})", quote(delimiter), *mode == SplitMode::Right),
                Split::Positions(p) => format!("Splitter.SplitTextByPositions({{{}}})", p.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")),
            };
            format!("Table.SplitColumn({prev}, {}, {sp}, {})", quote(column), texts(into))
        }
        Op::MergeColumns { columns, separator, name } => format!("Table.CombineColumns({prev},{},Combiner.CombineTextByDelimiter({}, QuoteStyle.None),{})", texts(columns), quote(separator), quote(name)),
        Op::AddColumn { name, expr, ty } => format!("Table.AddColumn({prev}, {}, each {}{})", quote(name), print(expr), ty.map(|t| format!(", {}", t.m())).unwrap_or_default()),
        Op::AddIndex { name, start, step } => format!("Table.AddIndexColumn({prev}, {}, {start}, {step}, Int64.Type)", quote(name)),
        Op::Group { keys, aggs } => {
            let a: Vec<String> = aggs.iter().map(|a| {
                let (body, ty) = match a.op {
                    AggOp::CountRows => ("Table.RowCount(_)".to_string(), "Int64.Type"),
                    AggOp::CountDistinct if a.column.is_empty() => ("Table.RowCount(Table.Distinct(_))".to_string(), "Int64.Type"),
                    AggOp::CountDistinct => (format!("List.Count(List.Distinct([{}]))", mlang::field(&a.column)), "Int64.Type"),
                    op => (format!("{}([{}])", op.list_fn(), mlang::field(&a.column)), "type nullable number"),
                };
                format!("{{{}, each {body}, {ty}}}", quote(&a.name))
            }).collect();
            format!("Table.Group({prev}, {}, {{{}}})", texts(keys), a.join(", "))
        }
        Op::Pivot { column, values, agg } => format!("Table.Pivot({prev}, List.Distinct({prev}[{}]), {}, {}{})", mlang::field(column), quote(column), quote(values), agg.map(|a| format!(", {}", a.list_fn())).unwrap_or_default()),
        Op::Unpivot { columns, other, attribute, value } => format!("Table.{}({prev}, {}, {}, {})", if *other { "UnpivotOtherColumns" } else { "Unpivot" }, texts(columns), quote(attribute), quote(value)),
        Op::NestedJoin { keys, other, other_keys, column, kind } => format!("Table.NestedJoin({prev}, {}, {}, {}, {}, {})", texts(keys), ident(other), texts(other_keys), quote(column), kind.m()),
        Op::ExpandTable { column, fields, names } => format!("Table.ExpandTableColumn({prev}, {}, {}, {})", quote(column), texts(fields), texts(names)),
        Op::Append(o) => format!("Table.Combine({{{prev}, {}}})", o.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", ")),
        Op::Distinct(None) => format!("Table.Distinct({prev})"),
        Op::Distinct(Some(c)) => format!("Table.Distinct({prev}, {})", texts(c)),
        Op::FirstN(n) => format!("Table.FirstN({prev},{n})"),
        Op::LastN(n) => format!("Table.LastN({prev},{n})"),
        Op::Skip(n) => format!("Table.Skip({prev},{n})"),
        Op::RemoveLastN(n) => format!("Table.RemoveLastN({prev},{n})"),
        Op::FillDown(c) => format!("Table.FillDown({prev},{})", texts(c)),
        Op::FillUp(c) => format!("Table.FillUp({prev},{})", texts(c)),
        Op::Transpose => format!("Table.Transpose({prev})"),
    }
}

// ---- reading M back into steps ------------------------------------------------------------------

fn text_of(e: &Expr) -> Result<String, String> { if let Expr::Text(s) = e { Ok(s.clone()) } else { Err(format!("Expected text, found {}.", print(e))) } }
fn int_of(e: &Expr) -> Result<i64, String> { if let Expr::Num(n) = e { if n.fract() == 0.0 { return Ok(*n as i64); } } Err(format!("Expected a whole number, found {}.", print(e))) }
fn list_of(e: &Expr) -> Result<&[Expr], String> { if let Expr::List(v) = e { Ok(v) } else { Err(format!("Expected a list, found {}.", print(e))) } }
fn texts_of(e: &Expr) -> Result<Vec<String>, String> { match e { Expr::Text(s) => Ok(vec![s.clone()]), _ => list_of(e)?.iter().map(text_of).collect() } }
fn pairs_of(e: &Expr) -> Result<Vec<(Expr, Expr)>, String> {
    let v = list_of(e)?;
    // A single pair may be written without the outer list: {"a", type text}.
    if v.len() == 2 && !matches!(v[0], Expr::List(_)) { return Ok(vec![(v[0].clone(), v[1].clone())]); }
    v.iter().map(|p| { let p = list_of(p)?; if p.len() == 2 { Ok((p[0].clone(), p[1].clone())) } else { Err("Expected pairs.".into()) } }).collect()
}
fn value_of(e: &Expr) -> Result<V, String> { Ok(match e { Expr::Null => V::Null, Expr::Num(n) => V::Num(*n), Expr::Text(s) => V::Text(s.clone()), Expr::Bool(b) => V::Bool(*b), _ => return Err("Expected a value.".into()) }) }
fn each_of(e: &Expr) -> Result<Expr, String> {
    match e {
        Expr::Each(b) => Ok((**b).clone()),
        // (_) => body is the same as each.
        Expr::Lambda(p, b) if p.len() == 1 && p[0] == "_" => Ok((**b).clone()),
        _ => Err("Expected an each expression.".into()),
    }
}
fn is_ident(e: &Expr, n: &str) -> bool { matches!(e, Expr::Ident(x) if x == n) }

/// The source and steps of a let expression, or why it isn't one Jadwal models.
pub fn steps_from(e: &Expr) -> Result<(Source, Vec<Step>), String> {
    let (binds, body) = match e { Expr::Let(b, body) => (b.clone(), body), _ => return Err("The formula isn't a let expression.".into()) };
    if binds.is_empty() { return Err("The formula has no steps.".into()); }
    let (source, used) = source_from(&binds)?;
    let mut prev = binds[used - 1].0.clone();
    let mut steps = vec![];
    for (name, x) in &binds[used..] {
        let op = op_from(x, &prev).map_err(|e| format!("{name}: {e}"))?;
        steps.push(Step { name: name.clone(), op });
        prev = name.clone();
    }
    if !is_ident(body, &prev) { return Err("The formula doesn't end with its last step.".into()); }
    Ok((source, steps))
}

fn source_from(b: &[(String, Expr)]) -> Result<(Source, usize), String> {
    let (_, e) = &b[0];
    if let Expr::Ident(q) = e { if !q.contains('.') || q.starts_with('#') { return Ok((Source::Query(q.clone()), 1)); } }
    // Excel.CurrentWorkbook(){[Name="T"]}[Content]
    if let Expr::FieldOf(x, f) = e {
        if f == "Content" {
            if let Expr::Item(t, key) = &**x {
                if matches!(&**t, Expr::Call(f, a) if is_ident(f, "Excel.CurrentWorkbook") && a.is_empty()) {
                    if let Expr::Record(r) = &**key { if let Some((_, Expr::Text(n))) = r.iter().find(|(k, _)| k == "Name") { return Ok((Source::Current { name: n.clone() }, 1)); } }
                }
            }
        }
    }
    if let Expr::Call(f, a) = e {
        let file = |x: &Expr| -> Result<String, String> { match x { Expr::Call(g, p) if is_ident(g, "File.Contents") && !p.is_empty() => text_of(&p[0]), _ => Err("Expected File.Contents.".into()) } };
        if is_ident(f, "Csv.Document") && !a.is_empty() {
            let path = file(&a[0])?;
            let (mut delimiter, mut encoding, mut columns) = (",".to_string(), 65001u32, None);
            if let Some(Expr::Record(opts)) = a.get(1) {
                for (k, v) in opts {
                    match k.as_str() {
                        "Delimiter" => delimiter = text_of(v)?,
                        "Encoding" => encoding = int_of(v)? as u32,
                        "Columns" => columns = Some(int_of(v)? as usize),
                        "QuoteStyle" => {}
                        _ => return Err(format!("Csv.Document option {k} isn't modelled.")),
                    }
                }
            } else if a.len() > 1 { return Err("Csv.Document options aren't modelled.".into()); }
            return Ok((Source::Csv { path, delimiter, encoding, columns }, 1));
        }
        if is_ident(f, "Excel.Workbook") && !a.is_empty() && b.len() > 1 {
            let path = file(&a[0])?;
            // Nav = Source{[Item="Sheet1",Kind="Sheet"]}[Data]
            if let Expr::FieldOf(x, fd) = &b[1].1 {
                if let (true, Expr::Item(t, key)) = (fd == "Data", &**x) {
                    if let (true, Expr::Record(r)) = (is_ident(t, &b[0].0), &**key) {
                        let get = |k: &str| r.iter().find(|(n, _)| n == k).and_then(|(_, v)| if let Expr::Text(s) = v { Some(s.clone()) } else { None });
                        if let (Some(item), Some(kind)) = (get("Item"), get("Kind")) {
                            if b[1].0 == format!("{item}_{kind}") { return Ok((Source::Workbook { path, item, kind }, 2)); }
                        }
                    }
                }
            }
        }
        if is_ident(f, "Web.Page") && a.len() == 1 && b.len() > 1 {
            if let Expr::Call(g, p) = &a[0] {
                if is_ident(g, "Web.Contents") && p.len() == 1 {
                    let url = text_of(&p[0])?;
                    if let Expr::FieldOf(x, fd) = &b[1].1 {
                        if let (true, Expr::Item(t, i)) = (fd == "Data", &**x) {
                            if is_ident(t, &b[0].0) { let index = int_of(i)? as usize; if b[1].0 == format!("Data{index}") { return Ok((Source::Web { url, index }, 2)); } }
                        }
                    }
                }
            }
        }
    }
    Err(format!("The source {} isn't one Jadwal models.", print(e)))
}

/// One step from its expression over the previous step `prev`.
pub fn op_from(e: &Expr, prev: &str) -> Result<Op, String> {
    let Expr::Call(f, a) = e else { return Err(format!("{} isn't a step Jadwal models.", print(e))) };
    let Expr::Ident(name) = &**f else { return Err("Expected a function.".into()) };
    let first = |a: &[Expr]| -> Result<(), String> { if a.first().is_some_and(|x| is_ident(x, prev)) { Ok(()) } else { Err(format!("The step doesn't take the previous step ({prev}).")) } };
    let n = a.len();
    let op = match (name.as_str(), n) {
        ("Table.Combine", 1) => {
            let l = list_of(&a[0])?;
            if !l.first().is_some_and(|x| is_ident(x, prev)) { return Err("The append doesn't start with the previous step.".into()); }
            return l[1..].iter().map(|x| if let Expr::Ident(q) = x { Ok(q.clone()) } else { Err("Appended tables must be queries.".to_string()) }).collect::<Result<Vec<_>, _>>().map(Op::Append);
        }
        _ => { first(a)?; name.as_str() }
    };
    Ok(match (op, n) {
        ("Table.PromoteHeaders", 1 | 2) => Op::PromoteHeaders,
        ("Table.DemoteHeaders", 1) => Op::DemoteHeaders,
        ("Table.TransformColumnTypes", 2 | 3) => Op::ChangeType(pairs_of(&a[1])?.iter().map(|(c, t)| Ok((text_of(c)?, Ty::from_expr(t).ok_or_else(|| format!("The type {} isn't modelled.", print(t)))?))).collect::<Result<_, String>>()?),
        ("Table.RemoveColumns", 2) => Op::RemoveColumns(texts_of(&a[1])?),
        ("Table.SelectColumns", 2) => Op::SelectColumns(texts_of(&a[1])?),
        ("Table.ReorderColumns", 2) => Op::ReorderColumns(texts_of(&a[1])?),
        ("Table.RenameColumns", 2) => Op::RenameColumns(pairs_of(&a[1])?.iter().map(|(x, y)| Ok((text_of(x)?, text_of(y)?))).collect::<Result<_, String>>()?),
        ("Table.SelectRows", 2) => Op::FilterRows(each_of(&a[1])?),
        ("Table.Sort", 2) => Op::Sort(match &a[1] {
            Expr::Text(c) => vec![(c.clone(), false)],
            l => list_of(l)?.iter().map(|k| match k {
                Expr::Text(c) => Ok((c.clone(), false)),
                Expr::List(p) if p.len() == 2 => Ok((text_of(&p[0])?, match &p[1] { Expr::Ident(o) if o == "Order.Descending" => true, Expr::Ident(o) if o == "Order.Ascending" => false, Expr::Num(n) => *n == 1.0, _ => return Err("Unknown sort order.".to_string()) })),
                _ => Err("Unknown sort key.".to_string()),
            }).collect::<Result<_, String>>()?,
        }),
        ("Table.ReplaceValue", 5) => Op::ReplaceValue {
            find: value_of(&a[1])?, replace: value_of(&a[2])?,
            text: match &a[3] { Expr::Ident(r) if r == "Replacer.ReplaceText" => true, Expr::Ident(r) if r == "Replacer.ReplaceValue" => false, _ => return Err("Unknown replacer.".into()) },
            columns: texts_of(&a[4])?,
        },
        ("Table.SplitColumn", 3 | 4) => {
            let column = text_of(&a[1])?;
            let Expr::Call(sf, sa) = &a[2] else { return Err("Unknown splitter.".into()) };
            let by = match (sf.as_ref(), sa.len()) {
                (Expr::Ident(s), 1 | 2) if s == "Splitter.SplitTextByDelimiter" => Split::Delimiter { delimiter: text_of(&sa[0])?, mode: SplitMode::Each },
                (Expr::Ident(s), 3) if s == "Splitter.SplitTextByEachDelimiter" => {
                    let d = texts_of(&sa[0])?;
                    if d.len() != 1 { return Err("Only one delimiter is modelled.".into()); }
                    Split::Delimiter { delimiter: d[0].clone(), mode: if matches!(sa[2], Expr::Bool(true)) { SplitMode::Right } else { SplitMode::Left } }
                }
                (Expr::Ident(s), 1) if s == "Splitter.SplitTextByPositions" => Split::Positions(list_of(&sa[0])?.iter().map(|x| int_of(x).map(|v| v as usize)).collect::<Result<_, _>>()?),
                _ => return Err("Unknown splitter.".into()),
            };
            let into = match a.get(3) { Some(Expr::Num(k)) => (1..=*k as usize).map(|i| format!("{column}.{i}")).collect(), Some(x) => texts_of(x)?, None => vec![format!("{column}.1"), format!("{column}.2")] };
            Op::SplitColumn { column, by, into }
        }
        ("Table.CombineColumns", 4) => {
            let Expr::Call(cf, ca) = &a[2] else { return Err("Unknown combiner.".into()) };
            if !is_ident(cf, "Combiner.CombineTextByDelimiter") || ca.is_empty() { return Err("Unknown combiner.".into()); }
            Op::MergeColumns { columns: texts_of(&a[1])?, separator: text_of(&ca[0])?, name: text_of(&a[3])? }
        }
        ("Table.AddColumn", 3 | 4) => Op::AddColumn { name: text_of(&a[1])?, expr: each_of(&a[2])?, ty: match a.get(3) { Some(t) => Some(Ty::from_expr(t).ok_or("The column type isn't modelled.")?), None => None } },
        ("Table.AddIndexColumn", 2..=5) => Op::AddIndex { name: text_of(&a[1])?, start: a.get(2).map(int_of).transpose()?.unwrap_or(0), step: a.get(3).map(int_of).transpose()?.unwrap_or(1) },
        ("Table.Group", 3) => {
            let keys = texts_of(&a[1])?;
            let l = list_of(&a[2])?;
            let one = l.len() >= 2 && matches!(l[0], Expr::Text(_)) && matches!(l[1], Expr::Each(_));
            let items: Vec<&Expr> = if one { vec![&a[2]] } else { l.iter().collect() };
            let aggs = items.into_iter().map(|it| {
                let p = list_of(it)?;
                if p.len() < 2 { return Err("Unknown aggregation.".to_string()); }
                let name = text_of(&p[0])?;
                let body = each_of(&p[1])?;
                let (op, column) = match &body {
                    Expr::Call(g, x) if is_ident(g, "Table.RowCount") && x.len() == 1 && is_ident(&x[0], "_") => (AggOp::CountRows, String::new()),
                    Expr::Call(g, x) if is_ident(g, "Table.RowCount") && x.len() == 1 && matches!(&x[0], Expr::Call(h, y) if is_ident(h, "Table.Distinct") && y.len() == 1 && is_ident(&y[0], "_")) => (AggOp::CountDistinct, String::new()),
                    Expr::Call(g, x) if is_ident(g, "List.Count") && x.len() == 1 && matches!(&x[0], Expr::Call(h, y) if is_ident(h, "List.Distinct") && y.len() == 1 && matches!(y[0], Expr::Field(_))) => {
                        let Expr::Call(_, y) = &x[0] else { unreachable!() };
                        let Expr::Field(c) = &y[0] else { unreachable!() };
                        (AggOp::CountDistinct, c.clone())
                    }
                    Expr::Call(g, x) if x.len() == 1 && matches!(x[0], Expr::Field(_)) => {
                        let Expr::Ident(gn) = &**g else { return Err("Unknown aggregation.".into()) };
                        let op = AggOp::from_list_fn(gn).filter(|o| !matches!(o, AggOp::CountRows | AggOp::CountDistinct)).ok_or("Unknown aggregation.")?;
                        let Expr::Field(c) = &x[0] else { unreachable!() };
                        (op, c.clone())
                    }
                    _ => return Err("Unknown aggregation.".into()),
                };
                Ok(Agg { name, op, column })
            }).collect::<Result<Vec<_>, String>>()?;
            Op::Group { keys, aggs }
        }
        ("Table.Pivot", 4 | 5) => {
            let column = text_of(&a[2])?;
            let ok = matches!(&a[1], Expr::Call(g, x) if is_ident(g, "List.Distinct") && x.len() == 1 && matches!(&x[0], Expr::FieldOf(t, c) if is_ident(t, prev) && *c == column));
            if !ok { return Err("Only pivots over the column's distinct values are modelled.".into()); }
            let agg = match a.get(4) { Some(Expr::Ident(g)) => Some(AggOp::from_list_fn(g).ok_or("Unknown aggregation.")?), Some(_) => return Err("Unknown aggregation.".into()), None => None };
            Op::Pivot { column, values: text_of(&a[3])?, agg }
        }
        ("Table.UnpivotOtherColumns" | "Table.Unpivot", 4) => Op::Unpivot { columns: texts_of(&a[1])?, other: op == "Table.UnpivotOtherColumns", attribute: text_of(&a[2])?, value: text_of(&a[3])? },
        ("Table.NestedJoin", 6 | 7) => {
            let Expr::Ident(other) = &a[2] else { return Err("Merged tables must be queries.".into()) };
            let kind = match &a[5] { Expr::Ident(k) => Join::ALL.into_iter().find(|j| j.m() == k).ok_or("Unknown join kind.")?, _ => return Err("Unknown join kind.".into()) };
            Op::NestedJoin { keys: texts_of(&a[1])?, other: other.clone(), other_keys: texts_of(&a[3])?, column: text_of(&a[4])?, kind }
        }
        ("Table.ExpandTableColumn", 3 | 4) => { let fields = texts_of(&a[2])?; Op::ExpandTable { column: text_of(&a[1])?, names: a.get(3).map(texts_of).transpose()?.unwrap_or_else(|| fields.clone()), fields } }
        ("Table.Distinct", 1) => Op::Distinct(None),
        ("Table.Distinct", 2) => Op::Distinct(Some(texts_of(&a[1])?)),
        ("Table.FirstN", 2) => Op::FirstN(int_of(&a[1])? as usize),
        ("Table.LastN", 2) => Op::LastN(int_of(&a[1])? as usize),
        ("Table.Skip", 2) => Op::Skip(int_of(&a[1])? as usize),
        ("Table.RemoveLastN", 2) => Op::RemoveLastN(int_of(&a[1])? as usize),
        ("Table.RemoveFirstN", 2) => Op::Skip(int_of(&a[1])? as usize),
        ("Table.FillDown", 2) => Op::FillDown(texts_of(&a[1])?),
        ("Table.FillUp", 2) => Op::FillUp(texts_of(&a[1])?),
        ("Table.Transpose", 1) => Op::Transpose,
        _ => return Err(format!("{name} isn't a step Jadwal models.")),
    })
}

// ---- row filters ---------------------------------------------------------------------------

/// One condition of the Filter Rows dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CondOp { Equals, NotEquals, Greater, GreaterEq, Less, LessEq, BeginsWith, NotBeginsWith, EndsWith, NotEndsWith, Contains, NotContains }

impl CondOp {
    pub const ALL: [CondOp; 12] = [CondOp::Equals, CondOp::NotEquals, CondOp::Greater, CondOp::GreaterEq, CondOp::Less, CondOp::LessEq, CondOp::BeginsWith, CondOp::NotBeginsWith, CondOp::EndsWith, CondOp::NotEndsWith, CondOp::Contains, CondOp::NotContains];
    pub fn label(self) -> &'static str {
        match self { CondOp::Equals => "equals", CondOp::NotEquals => "does not equal", CondOp::Greater => "is greater than", CondOp::GreaterEq => "is greater than or equal to", CondOp::Less => "is less than", CondOp::LessEq => "is less than or equal to", CondOp::BeginsWith => "begins with", CondOp::NotBeginsWith => "does not begin with", CondOp::EndsWith => "ends with", CondOp::NotEndsWith => "does not end with", CondOp::Contains => "contains", CondOp::NotContains => "does not contain" }
    }
}

fn lit(v: &V) -> Expr { match v { V::Null => Expr::Null, V::Num(n) => Expr::Num(*n), V::Text(s) => Expr::Text(s.clone()), V::Bool(b) => Expr::Bool(*b), _ => Expr::Null } }

/// One column condition as M: [Col] > 5, Text.StartsWith([Col], "a")…
pub fn cond_expr(column: &str, op: CondOp, value: &V) -> Expr {
    let f = || Box::new(Expr::Field(column.into()));
    let call = |n: &str| Expr::Call(Box::new(Expr::Ident(n.into())), vec![Expr::Field(column.into()), lit(value)]);
    let not = |e: Expr| Expr::Unary(UOp::Not, Box::new(e));
    match op {
        CondOp::Equals => Expr::Binary(BOp::Eq, f(), Box::new(lit(value))),
        CondOp::NotEquals => Expr::Binary(BOp::Ne, f(), Box::new(lit(value))),
        CondOp::Greater => Expr::Binary(BOp::Gt, f(), Box::new(lit(value))),
        CondOp::GreaterEq => Expr::Binary(BOp::Ge, f(), Box::new(lit(value))),
        CondOp::Less => Expr::Binary(BOp::Lt, f(), Box::new(lit(value))),
        CondOp::LessEq => Expr::Binary(BOp::Le, f(), Box::new(lit(value))),
        CondOp::BeginsWith => call("Text.StartsWith"), CondOp::NotBeginsWith => not(call("Text.StartsWith")),
        CondOp::EndsWith => call("Text.EndsWith"), CondOp::NotEndsWith => not(call("Text.EndsWith")),
        CondOp::Contains => call("Text.Contains"), CondOp::NotContains => not(call("Text.Contains")),
    }
}

/// Two conditions joined by and / or (the second optional).
pub fn conds_expr(column: &str, a: (CondOp, V), b: Option<(CondOp, V)>, and: bool) -> Expr {
    let x = cond_expr(column, a.0, &a.1);
    match b { Some((op, v)) => Expr::Binary(if and { BOp::And } else { BOp::Or }, Box::new(x), Box::new(cond_expr(column, op, &v))), None => x }
}

/// The filter of a column's value list: keep these values ([Col] = "a" or [Col] = "b"), or, when
/// fewer are left out, drop those ([Col] <> "c" and …).
pub fn values_expr(column: &str, keep: &[V], drop: &[V]) -> Expr {
    let f = || Box::new(Expr::Field(column.into()));
    let chain = |vals: &[V], eq: bool| -> Expr {
        vals.iter().map(|v| Expr::Binary(if eq { BOp::Eq } else { BOp::Ne }, f(), Box::new(lit(v)))).reduce(|a, b| Expr::Binary(if eq { BOp::Or } else { BOp::And }, Box::new(a), Box::new(b))).unwrap_or(Expr::Bool(eq == false))
    };
    Expr::Paren(Box::new(if !drop.is_empty() && drop.len() <= keep.len() { chain(drop, false) } else { chain(keep, true) }))
}

// ---- the workbook's queries ------------------------------------------------------------------

/// The workbook's queries (from its DataMashup part).
pub fn list(wb: &Workbook) -> Vec<Query> { mashup::read_queries(wb).map(|m| m.queries).unwrap_or_default() }

/// Whether the workbook's queries can be changed here (its DataMashup part, if any, was read).
pub fn editable(wb: &Workbook) -> bool { mashup::read_queries(wb).is_ok_and(|m| m.section_ok) }

pub fn find(wb: &Workbook, name: &str) -> Option<Query> { list(wb).into_iter().find(|q| q.name.eq_ignore_ascii_case(name)) }

/// A query name that is free: base, else base (2), base (3)…
pub fn unique_name(wb: &Workbook, base: &str) -> String {
    let clean: String = base.chars().filter(|c| !c.is_control() && *c != '"' && *c != '.').collect::<String>().trim().to_string();
    let clean = if clean.is_empty() { "Query1".to_string() } else { clean };
    let all = list(wb);
    let taken = |n: &str| all.iter().any(|q| q.name.eq_ignore_ascii_case(n));
    if !taken(&clean) { return clean; }
    (2..).map(|i| format!("{clean} ({i})")).find(|n| !taken(n)).unwrap()
}

/// Checks a query name ([MS-QDEFF] 2.3.2: no periods, quotation marks, tabs or line breaks).
pub fn name_problem(wb: &Workbook, name: &str, except: Option<&str>) -> Option<&'static str> {
    if name.trim().is_empty() { return Some("Enter a name for the query."); }
    if name.trim() != name || name.chars().any(|c| c == '.' || c == '"' || c.is_control()) { return Some("A query name can't contain periods, quotation marks, tabs or line breaks, or start or end with a space."); }
    if list(wb).iter().any(|q| q.name.eq_ignore_ascii_case(name) && !except.is_some_and(|e| e.eq_ignore_ascii_case(&q.name))) { return Some("A query with that name already exists."); }
    None
}

/// Replaces the workbook's queries: the DataMashup part and each query's connection.
pub fn save_all(wb: &mut Workbook, queries: &[Query]) -> Result<(), String> {
    let old = list(wb);
    mashup::write_queries(wb, queries)?;
    sync_connections(wb, &old, queries);
    Ok(())
}

/// Adds or replaces one query (matched by `old_name`, or its own name); renames follow into
/// other queries' references and the connection.
pub fn save(wb: &mut Workbook, q: &Query, old_name: Option<&str>) -> Result<(), String> {
    let mut all = list(wb);
    let key = old_name.unwrap_or(&q.name).to_lowercase();
    if let Some(old) = old_name.filter(|o| !o.eq_ignore_ascii_case(&q.name)) {
        for other in all.iter_mut() {
            if let Some(Source::Query(s)) = &mut other.source { if s.eq_ignore_ascii_case(old) { *s = q.name.clone(); } }
            for st in &mut other.steps {
                match &mut st.op { Op::NestedJoin { other: o, .. } if o.eq_ignore_ascii_case(old) => *o = q.name.clone(), Op::Append(v) => for o in v.iter_mut() { if o.eq_ignore_ascii_case(old) { *o = q.name.clone(); } }, _ => {} }
            }
        }
    }
    match all.iter().position(|x| x.name.to_lowercase() == key) { Some(i) => all[i] = q.clone(), None => all.push(q.clone()) }
    save_all(wb, &all)
}

/// Deletes a query and its connection (loaded data stays as a plain table).
pub fn delete(wb: &mut Workbook, name: &str) -> Result<(), String> {
    let all: Vec<Query> = list(wb).into_iter().filter(|q| !q.name.eq_ignore_ascii_case(name)).collect();
    save_all(wb, &all)
}

/// Each query gets a `Query - Name` connection (Microsoft.Mashup.OleDb.1, Location = its name);
/// connections of deleted queries go, renamed ones follow.
fn sync_connections(wb: &mut Workbook, old: &[Query], new: &[Query]) {
    use crate::connections::{self, Connection, Source as CS};
    let mut conns = connections::list(wb);
    // A renamed query (same position, or the only change) keeps its connection.
    for (o, n) in old.iter().zip(new.iter()) {
        if !o.name.eq_ignore_ascii_case(&n.name) && !new.iter().any(|x| x.name.eq_ignore_ascii_case(&o.name)) {
            for c in conns.iter_mut() { if matches!(&c.source, CS::Mashup { query } if query.eq_ignore_ascii_case(&o.name)) { c.source = CS::Mashup { query: n.name.clone() }; c.name = format!("Query - {}", n.name); c.description = format!("Connection to the '{}' query in the workbook.", n.name); } }
        }
    }
    conns.retain(|c| match &c.source { CS::Mashup { query } => new.iter().any(|q| q.name.eq_ignore_ascii_case(query)), _ => true });
    for q in new {
        if conns.iter().any(|c| matches!(&c.source, CS::Mashup { query } if query.eq_ignore_ascii_case(&q.name))) { continue; }
        let id = conns.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        conns.push(Connection { id, name: format!("Query - {}", q.name), description: format!("Connection to the '{}' query in the workbook.", q.name), source: CS::Mashup { query: q.name.clone() }, refresh_on_open: false, raw: None });
    }
    connections::save(wb, &conns);
}

/// The connection id of a query.
pub fn connection_id(wb: &Workbook, query: &str) -> Option<u32> {
    crate::connections::list(wb).into_iter().find(|c| matches!(&c.source, crate::connections::Source::Mashup { query: q } if q.eq_ignore_ascii_case(query))).map(|c| c.id)
}

/// Where a query's table is: (sheet, table index).
pub fn loaded_table(wb: &Workbook, q: &Query) -> Option<(usize, usize)> {
    match &q.load { Load::Table(t) => wb.find_table(t), Load::ConnectionOnly => None }
}

/// The range a query's data occupies.
pub fn destination(wb: &Workbook, query: &str) -> Option<(usize, crate::addr::Range)> {
    let q = find(wb, query)?;
    let (si, ti) = loaded_table(wb, &q)?;
    Some((si, wb.sheets[si].tables[ti].range))
}

/// The query a table is the output of (for the table's query table part on save).
pub fn query_for_table(wb: &Workbook, table: &str) -> Option<Query> {
    list(wb).into_iter().find(|q| matches!(&q.load, Load::Table(t) if t.eq_ignore_ascii_case(table)))
}

/// Evaluates a query by name.
pub fn evaluate(wb: &Workbook, name: &str, fetch: eval::Fetch) -> Result<Table, String> {
    let all = list(wb);
    let q = all.iter().find(|q| q.name.eq_ignore_ascii_case(name)).ok_or("The query is gone.")?;
    Ctx::new(wb, &all, fetch).run(q, None)
}

/// Where Close & Load sends a query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadTo { /// Its table if it has one, else a table on a new sheet.
    Table, NewSheet, ConnectionOnly }

/// Saves `q` and loads it as asked: returns the (sheet, table index) it was loaded to.
pub fn close_and_load(wb: &mut Workbook, q: &Query, old_name: Option<&str>, to: LoadTo, fetch: eval::Fetch) -> Result<Option<(usize, usize)>, String> {
    let mut q = q.clone();
    if to == LoadTo::ConnectionOnly { q.load = Load::ConnectionOnly; save(wb, &q, old_name)?; return Ok(None); }
    if to == LoadTo::NewSheet || loaded_table(wb, &q).is_none() { q.load = Load::ConnectionOnly; }
    save(wb, &q, old_name)?;
    refresh(wb, &q.name, fetch).map(Some)
}

/// Refresh: evaluates the query and writes its result to its table (making a table on a new
/// sheet when it has none and is meant to load).
pub fn refresh(wb: &mut Workbook, name: &str, fetch: eval::Fetch) -> Result<(usize, usize), String> {
    let mut q = find(wb, name).ok_or("The query is gone.")?;
    if q.read_only() { return Err("This query uses M that Jadwal doesn't evaluate; it can be refreshed in Excel.".into()); }
    let t = evaluate(wb, name, fetch)?;
    let (si, ti) = put_table(wb, &q, &t)?;
    let tname = wb.sheets[si].tables[ti].name.clone();
    q.load = Load::Table(tname);
    set_meta(&mut q.meta, "FillColumnNames", &format!("s{}", serde_json::to_string(&t.columns).unwrap_or_default()));
    set_meta(&mut q.meta, "FillCount", &format!("l{}", t.rows.len()));
    set_meta(&mut q.meta, "FillErrorCount", &format!("l{}", t.rows.iter().flatten().filter(|v| matches!(v, V::Err(_))).count()));
    set_meta(&mut q.meta, "FillLastUpdated", &format!("d{}", mashup::now_stamp()));
    set_meta(&mut q.meta, "FillStatus", "sComplete");
    set_meta(&mut q.meta, "RecoveryTargetSheet", &format!("s{}", wb.sheets[si].name));
    save(wb, &q, None)?;
    Ok((si, ti))
}

fn set_meta(meta: &mut Vec<(String, String)>, k: &str, v: &str) {
    match meta.iter_mut().find(|e| e.0 == k) { Some(e) => e.1 = v.into(), None => meta.push((k.into(), v.into())) }
}

/// Refreshes every query that loads to a table; returns errors by query name.
pub fn refresh_all(wb: &mut Workbook, fetch: eval::Fetch) -> Vec<(String, String)> {
    let mut errs = vec![];
    for q in list(wb) {
        if q.read_only() || loaded_table(wb, &q).is_none() { continue; }
        if let Err(e) = refresh(wb, &q.name, fetch) { errs.push((q.name, e)); }
    }
    errs
}

/// A table name made from a query name (spaces and other characters become _), free in the book.
fn table_name_for(wb: &Workbook, query: &str) -> String {
    let mut base: String = query.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '.' { c } else { '_' }).collect();
    if !base.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') { base.insert(0, '_'); }
    if crate::addr::parse_range(&base).is_some() { base.push('_'); }
    let taken = |n: &str| wb.find_table(n).is_some() || wb.names.iter().any(|d| d.name.eq_ignore_ascii_case(n));
    if !taken(&base) { return base; }
    (2..).map(|i| format!("{base}_{i}")).find(|n| !taken(n)).unwrap()
}

/// Writes a result table into the query's table (or a new table on a new sheet).
fn put_table(wb: &mut Workbook, q: &Query, t: &Table) -> Result<(usize, usize), String> {
    use crate::addr::{Pos, Range};
    let ncols = t.columns.len().max(1) as u32;
    let (si, at, existing) = match loaded_table(wb, q) {
        Some((si, ti)) => { let r = wb.sheets[si].tables[ti].range; wb.clear(si, r, true, false, false); (si, r.start(), Some(ti)) }
        None => {
            let name = wb.unique_sheet_name("Sheet");
            (wb.add_sheet(wb.sheets.len(), Some(name)), Pos::new(0, 0), None)
        }
    };
    let mut rows: Vec<Vec<crate::value::Value>> = vec![t.columns.iter().map(crate::value::Value::str).collect()];
    for r in &t.rows { rows.push(r.iter().zip(&t.types).map(|(v, ty)| v.to_cell(*ty)).collect()); }
    let data_rows = t.rows.len().max(1) as u32;
    let r = Range::new(at, Pos::new(at.row + data_rows, at.col + ncols - 1));
    // Other content in the way of a grown table moves no further than Excel's: it is overwritten.
    crate::consolidate::put(wb, si, at, &rows);
    // Dates and times keep a date format.
    for (k, ty) in t.types.iter().enumerate() {
        let fmt = match ty { Ty::Date => "m/d/yyyy", Ty::DateTime => "m/d/yyyy h:mm", Ty::Time => "h:mm:ss AM/PM", Ty::Percentage => "0.00%", _ => continue };
        for row in 0..t.rows.len() as u32 {
            let p = Pos::new(at.row + 1 + row, at.col + k as u32);
            let mut st = wb.style_at(si, p).clone();
            st.num_fmt = fmt.into();
            let id = wb.styles.intern(st);
            wb.sheets[si].cell_mut(p).style = id;
        }
    }
    let ti = match existing {
        Some(ti) => {
            let tb = &mut wb.sheets[si].tables[ti];
            tb.range = r;
            tb.columns = t.columns.iter().map(|c| crate::tables::TableColumn { name: c.clone(), ..Default::default() }).collect();
            if tb.columns.is_empty() { tb.columns.push(crate::tables::TableColumn { name: "Column1".into(), ..Default::default() }); }
            if let Some(f) = &mut tb.filter { f.columns.retain(|c| c.col < ncols); }
            wb.apply_filters(si);
            ti
        }
        None => {
            if wb.sheets[si].tables.iter().any(|x| x.range.intersects(&r)) { return Err("A table can't overlap another table.".into()); }
            let ti = wb.create_table(si, r, true, "TableStyleMedium2")?;
            let name = table_name_for(wb, &q.name);
            let _ = wb.rename_table(si, ti, &name);
            ti
        }
    };
    Ok((si, ti))
}

/// After a range is chosen for From Table/Range: the table over it (made when it isn't one, as
/// Excel's Create Table step does), and a new query from it with Changed Type detected.
pub fn from_table_range(wb: &mut Workbook, sheet: usize, r: crate::addr::Range, headers: bool) -> Result<Query, String> {
    let ti = match wb.sheets[sheet].tables.iter().position(|t| t.range.intersects(&r)) { Some(t) => t, None => wb.create_table(sheet, r, headers, "TableStyleMedium2")? };
    let tname = wb.sheets[sheet].tables[ti].name.clone();
    let mut q = Query::new(&unique_name(wb, &tname), Source::Current { name: tname });
    detect_types(wb, &mut q);
    Ok(q)
}

/// Adds a Changed Type step for the types detected in the query's current result (as the editor
/// does after a source or Use First Row as Headers).
pub fn detect_types(wb: &Workbook, q: &mut Query) {
    let all = list(wb);
    let Ok(t) = Ctx::new(wb, &all, &|_| Err("offline".into())).run(q, None) else { return };
    let types: Vec<(String, Ty)> = t.columns.iter().enumerate().map(|(i, c)| (c.clone(), eval::detect(&t.rows.iter().map(|r| &r[i]).collect::<Vec<_>>()))).collect();
    if !types.is_empty() { q.add_step(Op::ChangeType(types), None); }
}

// ---- saving -----------------------------------------------------------------------------------

/// For a table a query loads to: marks the table part as a query table's (tableType, and a
/// query table field id per column) and returns the query table part tying it to the query's
/// connection.
pub fn query_table_xml(wb: &Workbook, t: &crate::tables::Table, table_xml: &mut String) -> Option<String> {
    if !wb.parts.keys().any(|k| k.starts_with("customXml/")) { return None; }
    let q = query_for_table(wb, &t.name)?;
    let id = connection_id(wb, &q.name)?;
    let x = std::mem::take(table_xml);
    let mut x = x.replacen(&format!(" ref=\"{}\"", t.range.a1()), &format!(" ref=\"{}\" tableType=\"queryTable\"", t.range.a1()), 1);
    for k in 1..=t.columns.len() { x = x.replacen(&format!("<tableColumn id=\"{k}\" "), &format!("<tableColumn id=\"{k}\" uniqueName=\"{k}\" queryTableFieldId=\"{k}\" "), 1); }
    *table_xml = x;
    let n = t.columns.len();
    let fields: String = t.columns.iter().enumerate().map(|(k, c)| format!("<queryTableField id=\"{}\" name=\"{}\" tableColumnId=\"{}\"/>", k + 1, crate::xlsx::xml::attr(&c.name), k + 1)).collect();
    Some(format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<queryTable xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" name=\"ExternalData_{id}\" connectionId=\"{id}\" autoFormatId=\"16\" applyNumberFormats=\"0\" applyBorderFormats=\"0\" applyFontFormats=\"0\" applyPatternFormats=\"0\" applyAlignmentFormats=\"0\" applyWidthHeightFormats=\"0\"><queryTableRefresh nextId=\"{}\"><queryTableFields count=\"{n}\">{fields}</queryTableFields></queryTableRefresh></queryTable>", n + 1))
}

/// A copy of the workbook without the query table parts of its Power Query connections read
/// from the file (they are written afresh from the tables).
pub fn stage(wb: &Workbook) -> Option<Workbook> {
    let ids: Vec<u32> = crate::connections::list(wb).into_iter().filter(|c| matches!(c.source, crate::connections::Source::Mashup { .. })).map(|c| c.id).collect();
    if ids.is_empty() { return None; }
    let stale: Vec<String> = wb.parts.iter().filter(|(k, _)| k.starts_with("xl/queryTables/") && k.ends_with(".xml") && !k.contains("_rels"))
        .filter(|(_, b)| crate::xlsx::xml::Element::parse(&crate::xlsx::xml::part_text(b)).ok().and_then(|e| e.attr_u32("connectionId")).is_some_and(|id| ids.contains(&id)))
        .map(|(k, _)| k.clone()).collect();
    if stale.is_empty() { return None; }
    let mut out = wb.clone();
    for k in stale { out.parts.remove(&crate::xlsx::rels_name_of(&k)); out.parts.remove(&k); }
    Some(out)
}

#[cfg(test)]
mod tests;
