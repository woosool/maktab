//! Tables (ListObjects) and filters (tables.js, sheet-filters.js): a named range with a header
//! row, an optional totals row and a built-in table style; the AutoFilter criteria shared by
//! tables and the sheet's own AutoFilter range; structured references (Table1[Amount], [@Amount],
//! Table1[#Headers]) and the table parts of .xlsx files.
//!
//! Filtered rows are hidden through `Sheet.hidden_rows` (so drawing, SUBTOTAL 1xx and the
//! status bar skip them) and remembered in `Sheet.filtered_rows`, so clearing a filter shows
//! exactly the rows it hid and rows hidden by hand stay hidden.

use crate::addr::{col_name, parse_range, Pos, Range, MAX_COLS, MAX_ROWS};
use crate::book::{EditResult, Input, Sheet, Workbook};
use crate::formula::lexer::{tokenize, Tok};
use crate::value::Value;
use crate::xlsx::xml::{attr, text, Element};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TableColumn {
    pub name: String,
    /// sum, average, count, countNums, max, min, stdDev, var, custom or empty.
    pub totals_function: String,
    pub totals_label: String,
    /// The custom totals formula (totals_function "custom"), without =.
    pub totals_formula: String,
    /// The column's calculated formula (filled down every data row), without =.
    pub calculated_formula: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub name: String,
    pub display_name: String,
    /// The whole table: header row, data rows and totals row.
    pub range: Range,
    pub header_row: bool,
    pub totals_row: bool,
    pub columns: Vec<TableColumn>,
    /// A built-in style name (TableStyleMedium2), or empty for none.
    pub style: String,
    pub banded_rows: bool,
    pub banded_cols: bool,
    pub first_col: bool,
    pub last_col: bool,
    /// Some when the header shows filter buttons.
    pub filter: Option<Filter>,
}

/// The criteria of one filter range, by column offset in the range.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Filter { pub columns: Vec<ColumnFilter> }

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnFilter { pub col: u32, pub crit: Criteria }

#[derive(Clone, Debug, PartialEq)]
pub enum Criteria {
    /// Rows whose shown text is one of `values` (compared without case), and blanks if `blanks`.
    Values { values: Vec<String>, blanks: bool },
    /// One or two conditions (with * and ? wildcards for equal / not equal).
    Custom { a: Cond, b: Option<Cond>, and: bool },
    Top10 { top: bool, percent: bool, n: f64 },
    /// today, yesterday, thisWeek, lastMonth, Q1, M3, aboveAverage...
    Dynamic(String),
    /// Cells filled with this color (None: no fill). Not saved to .xlsx.
    Color(Option<[u8; 3]>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp { Eq, Ne, Gt, Ge, Lt, Le }

#[derive(Clone, Debug, PartialEq)]
pub struct Cond { pub op: CmpOp, pub val: String }

impl CmpOp {
    pub const ALL: [CmpOp; 6] = [CmpOp::Eq, CmpOp::Ne, CmpOp::Gt, CmpOp::Ge, CmpOp::Lt, CmpOp::Le];
    pub fn xml(self) -> &'static str { match self { CmpOp::Eq => "equal", CmpOp::Ne => "notEqual", CmpOp::Gt => "greaterThan", CmpOp::Ge => "greaterThanOrEqual", CmpOp::Lt => "lessThan", CmpOp::Le => "lessThanOrEqual" } }
    pub fn from_xml(s: &str) -> CmpOp { CmpOp::ALL.into_iter().find(|o| o.xml() == s).unwrap_or(CmpOp::Eq) }
    pub fn label(self) -> &'static str { match self { CmpOp::Eq => "equals", CmpOp::Ne => "does not equal", CmpOp::Gt => "is greater than", CmpOp::Ge => "is greater than or equal to", CmpOp::Lt => "is less than", CmpOp::Le => "is less than or equal to" } }
}

impl Filter {
    pub fn get(&self, col: u32) -> Option<&Criteria> { self.columns.iter().find(|c| c.col == col).map(|c| &c.crit) }
    pub fn set(&mut self, col: u32, crit: Option<Criteria>) {
        self.columns.retain(|c| c.col != col);
        if let Some(crit) = crit { self.columns.push(ColumnFilter { col, crit }); self.columns.sort_by_key(|c| c.col); }
    }
    pub fn is_active(&self) -> bool { !self.columns.is_empty() }
}

impl Table {
    pub fn header(&self) -> Option<u32> { self.header_row.then_some(self.range.r0) }
    pub fn totals(&self) -> Option<u32> { self.totals_row.then_some(self.range.r1) }
    /// The data rows (may be empty: r0 > r1).
    pub fn data(&self) -> Range { Range { r0: self.range.r0 + self.header_row as u32, r1: self.range.r1.saturating_sub(self.totals_row as u32), ..self.range } }
    pub fn col_index(&self, name: &str) -> Option<u32> { self.columns.iter().position(|c| c.name.eq_ignore_ascii_case(name.trim())).map(|i| i as u32) }
    fn matches(&self, name: &str) -> bool { self.display_name.eq_ignore_ascii_case(name) || self.name.eq_ignore_ascii_case(name) }
}

/// A new name not used by the given ones: base, base2, base3...
fn unique_in(names: &[String], base: &str, skip: Option<usize>) -> String {
    let taken = |n: &str| names.iter().enumerate().any(|(i, x)| Some(i) != skip && x.eq_ignore_ascii_case(n));
    if !taken(base) { return base.to_string(); }
    (2..).map(|k| format!("{base}{k}")).find(|n| !taken(n)).unwrap()
}

/// Which filter a range of buttons belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope { Auto, Table(usize) }

// ---- the workbook's table and filter commands ----

impl Workbook {
    pub fn table_at(&self, sheet: usize, p: Pos) -> Option<usize> { self.sheets[sheet].tables.iter().position(|t| t.range.contains(p)) }
    pub fn find_table(&self, name: &str) -> Option<(usize, usize)> {
        self.sheets.iter().enumerate().find_map(|(s, sh)| sh.tables.iter().position(|t| t.matches(name)).map(|i| (s, i)))
    }
    pub fn unique_table_name(&self, base: &str) -> String {
        let taken = |n: &str| self.find_table(n).is_some() || self.names.iter().any(|d| d.name.eq_ignore_ascii_case(n));
        (1..).map(|k| format!("{base}{k}")).find(|n| !taken(n)).unwrap()
    }
    /// Checks a new name for table `i` of `sheet` (letters, digits, _ and ., starting with a letter or _).
    pub fn valid_table_name(&self, sheet: usize, i: usize, name: &str) -> EditResult {
        let n = name.trim();
        let mut ch = n.chars();
        if !ch.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '\\') || !n.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') { return Err("A table name must start with a letter or underscore and use only letters, numbers, periods and underscores.".into()); }
        if crate::addr::parse_range(n).is_some() || n.eq_ignore_ascii_case("R") || n.eq_ignore_ascii_case("C") { return Err("A table name can't look like a cell reference.".into()); }
        if let Some(x) = self.find_table(n) { if x != (sheet, i) { return Err("That name is already taken.".into()); } }
        if self.names.iter().any(|d| d.name.eq_ignore_ascii_case(n)) { return Err("That name is already taken.".into()); }
        Ok(())
    }
    pub fn rename_table(&mut self, sheet: usize, i: usize, name: &str) -> EditResult {
        self.valid_table_name(sheet, i, name)?;
        let old = self.sheets[sheet].tables[i].display_name.clone();
        let new = name.trim().to_string();
        { let t = &mut self.sheets[sheet].tables[i]; t.name = new.clone(); t.display_name = new.clone(); }
        self.rewrite_formulas(|_, f| rename_table_refs(f, &old, &new));
        Ok(())
    }

    /// Makes a table of `r` (Insert ▸ Table). Without headers a header row is inserted above the
    /// data, with Column1, Column2... Returns the table's index.
    pub fn create_table(&mut self, sheet: usize, r: Range, has_headers: bool, style: &str) -> Result<usize, String> {
        let mut r = r;
        if r.c1 - r.c0 >= 16_383 || r.rows() > 1_000_000 { return Err("A table can't be that large.".into()); }
        if self.sheets[sheet].tables.iter().any(|t| t.range.intersects(&r)) { return Err("A table can't overlap another table.".into()); }
        if self.sheets[sheet].merges.iter().any(|m| m.intersects(&r)) { return Err("A table can't contain merged cells.".into()); }
        if !has_headers {
            self.shift_cells(sheet, Range { r1: r.r0, ..r }, true, false);
            r.r1 += 1;
        }
        if r.r0 == r.r1 { r.r1 += 1; }
        let mut names: Vec<String> = vec![];
        for c in r.c0..=r.c1 {
            let t = if has_headers { self.display(sheet, Pos::new(r.r0, c)).trim().to_string() } else { String::new() };
            let base = if t.is_empty() { format!("Column{}", c - r.c0 + 1) } else { t };
            let n = unique_in(&names, &base, None);
            names.push(n);
        }
        for (k, n) in names.iter().enumerate() { self.set_cell_input(sheet, Pos::new(r.r0, r.c0 + k as u32), Input::Value(Value::str(n)), None); }
        let name = self.unique_table_name("Table");
        let s = &mut self.sheets[sheet];
        if s.autofilter.is_some_and(|a| a.intersects(&r)) { s.autofilter = None; s.filter = Filter::default(); }
        s.tables.push(Table {
            name: name.clone(), display_name: name, range: r, header_row: true, totals_row: false,
            columns: names.into_iter().map(|name| TableColumn { name, ..Default::default() }).collect(),
            style: if style.is_empty() { "TableStyleMedium2".into() } else { style.into() },
            banded_rows: true, banded_cols: false, first_col: false, last_col: false, filter: Some(Filter::default()),
        });
        self.apply_filters(sheet);
        Ok(self.sheets[sheet].tables.len() - 1)
    }

    /// Resizes a table (the header must stay in the same row); columns keep their names by position.
    pub fn resize_table(&mut self, sheet: usize, i: usize, r: Range) -> EditResult {
        let t = self.sheets[sheet].tables[i].clone();
        if t.header_row && r.r0 != t.range.r0 { return Err("The headers must remain in the same row, and the resulting table range must overlap the original table.".into()); }
        if !r.intersects(&t.range) { return Err("The resulting table range must overlap the original table.".into()); }
        if r.rows() < 1 + t.header_row as u32 + t.totals_row as u32 { return Err("A table needs at least one row of data.".into()); }
        if self.sheets[sheet].tables.iter().enumerate().any(|(k, o)| k != i && o.range.intersects(&r)) { return Err("A table can't overlap another table.".into()); }
        let mut cols: Vec<TableColumn> = vec![];
        for c in r.c0..=r.c1 {
            let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
            let col = if c >= t.range.c0 && c <= t.range.c1 { t.columns[(c - t.range.c0) as usize].clone() } else {
                let h = if t.header_row { self.display(sheet, Pos::new(r.r0, c)).trim().to_string() } else { String::new() };
                TableColumn { name: if h.is_empty() { format!("Column{}", c - r.c0 + 1) } else { h }, ..Default::default() }
            };
            cols.push(TableColumn { name: unique_in(&names, &col.name, None), ..col });
        }
        let (dc, totals_moved) = (r.c0 as i64 - t.range.c0 as i64, t.totals_row && r.r1 != t.range.r1);
        if totals_moved { self.clear(sheet, Range { r0: t.range.r1, r1: t.range.r1, ..t.range }, true, false, false); }
        {
            let tb = &mut self.sheets[sheet].tables[i];
            tb.range = r; tb.columns = cols;
            if let Some(f) = &mut tb.filter { f.columns = f.columns.iter().filter_map(|c| { let n = c.col as i64 - dc; (n >= 0 && n < r.cols() as i64).then(|| ColumnFilter { col: n as u32, crit: c.crit.clone() }) }).collect(); }
        }
        self.write_headers(sheet, i);
        if totals_moved { self.write_totals(sheet, i); }
        self.apply_filters(sheet);
        Ok(())
    }

    /// Table ▸ Convert to Range: the style's look is kept as cell formatting and structured
    /// references become ordinary references.
    pub fn convert_to_range(&mut self, sheet: usize, i: usize) {
        let t = self.sheets[sheet].tables[i].clone();
        let looks: Vec<(Pos, CellLook)> = t.range.iter().filter_map(|p| table_look(self, sheet, &t, p).map(|l| (p, l))).collect();
        for (p, l) in looks {
            let f = move |s: &mut crate::style::Style| {
                if s.fill.is_none() { if let Some(c) = l.fill { s.fill = Some(c); } }
                if s.color.is_none() { if let Some(c) = l.color { s.color = Some(c); } }
                if l.bold { s.bold = true; }
            };
            self.apply_style(sheet, &[Range::cell(p)], &f);
        }
        let mut edits = vec![];
        for (si, sh) in self.sheets.iter().enumerate() {
            for (q, c) in &sh.cells {
                let Some(f) = c.formula().filter(|f| f.contains('[')) else { continue };
                let n = table_refs_to_a1(self, &sh.name, f, &t.display_name, q.row);
                if n != f { edits.push((si, *q, n)); }
            }
        }
        for (si, q, f) in edits { self.set_input(si, q, &format!("={f}")); }
        self.sheets[sheet].tables.remove(i);
        self.apply_filters(sheet);
    }

    /// Shows or hides the header row (the row is cleared, or the row above takes the names).
    pub fn set_table_header(&mut self, sheet: usize, i: usize, on: bool) -> EditResult {
        let t = self.sheets[sheet].tables[i].clone();
        if t.header_row == on { return Ok(()); }
        if on {
            if t.range.r0 == 0 { return Err("There is no room for a header row above the table.".into()); }
            let above = Range { r0: t.range.r0 - 1, r1: t.range.r0 - 1, ..t.range };
            if above.iter().any(|p| !self.sheets[sheet].value(p).is_empty()) || self.sheets[sheet].tables.iter().any(|o| o.range.intersects(&above)) { self.shift_cells(sheet, Range { r0: t.range.r0, r1: t.range.r0, ..t.range }, true, false); let tb = &mut self.sheets[sheet].tables[i]; tb.range.r1 += 1; } else { self.sheets[sheet].tables[i].range.r0 -= 1; }
            self.sheets[sheet].tables[i].header_row = true;
            self.write_headers(sheet, i);
        } else {
            self.clear(sheet, Range { r0: t.range.r0, r1: t.range.r0, ..t.range }, true, false, false);
            let tb = &mut self.sheets[sheet].tables[i];
            tb.header_row = false; tb.range.r0 += 1;
            if tb.range.r0 > tb.range.r1 { tb.range.r1 = tb.range.r0; }
        }
        self.apply_filters(sheet);
        Ok(())
    }

    /// Adds or removes the totals row: Total in the first column and SUBTOTAL(109,…) in the last.
    pub fn set_table_totals(&mut self, sheet: usize, i: usize, on: bool) {
        let t = self.sheets[sheet].tables[i].clone();
        if t.totals_row == on { return; }
        if on {
            let below = Range { r0: t.range.r1 + 1, r1: t.range.r1 + 1, ..t.range };
            if below.iter().any(|p| !self.sheets[sheet].value(p).is_empty()) || self.sheets[sheet].tables.iter().any(|o| o.range.intersects(&below)) { self.shift_cells(sheet, below, true, false); }
            let tb = &mut self.sheets[sheet].tables[i];
            tb.range.r1 += 1; tb.totals_row = true;
            let n = tb.columns.len();
            if tb.columns.iter().all(|c| c.totals_function.is_empty() && c.totals_label.is_empty()) {
                if n > 1 { tb.columns[0].totals_label = "Total".into(); }
                tb.columns[n - 1].totals_function = "sum".into();
            }
            self.write_totals(sheet, i);
        } else {
            self.clear(sheet, Range { r0: t.range.r1, r1: t.range.r1, ..t.range }, true, false, false);
            let tb = &mut self.sheets[sheet].tables[i];
            tb.totals_row = false; tb.range.r1 -= 1;
        }
    }

    /// Sets a column's totals function and writes its cell.
    pub fn set_totals_function(&mut self, sheet: usize, i: usize, col: u32, f: &str) {
        let tb = &mut self.sheets[sheet].tables[i];
        if let Some(c) = tb.columns.get_mut(col as usize) { c.totals_function = f.into(); c.totals_label.clear(); }
        self.write_totals(sheet, i);
    }

    fn write_headers(&mut self, sheet: usize, i: usize) {
        let t = self.sheets[sheet].tables[i].clone();
        let Some(h) = t.header() else { return };
        for (k, c) in t.columns.iter().enumerate() {
            let p = Pos::new(h, t.range.c0 + k as u32);
            if self.display(sheet, p) != c.name { self.set_cell_input(sheet, p, Input::Value(Value::str(&c.name)), None); }
        }
    }

    fn write_totals(&mut self, sheet: usize, i: usize) {
        let t = self.sheets[sheet].tables[i].clone();
        let Some(row) = t.totals() else { return };
        for (k, c) in t.columns.iter().enumerate() {
            let p = Pos::new(row, t.range.c0 + k as u32);
            let col = struct_col(&c.name);
            let text = match subtotal_code(&c.totals_function) {
                Some(code) => format!("=SUBTOTAL({code},{}[{col}])", t.display_name),
                None if c.totals_function == "custom" && !c.totals_formula.is_empty() => format!("={}", c.totals_formula),
                None => c.totals_label.clone(),
            };
            self.set_input(sheet, p, &text);
        }
    }

    /// After typing into `p`: a table right above or left of it grows to take the cell, a header
    /// edit renames its column (kept unique), and a formula typed into an empty table column
    /// becomes a calculated column filled down every data row.
    pub fn table_after_input(&mut self, sheet: usize, p: Pos) {
        if self.sheets[sheet].cells.get(&p).is_none_or(|c| c.is_blank()) && self.table_at(sheet, p).is_none() { return; }
        if self.table_at(sheet, p).is_none() {
            let grow = self.sheets[sheet].tables.iter().position(|t| !t.totals_row && p.row == t.range.r1 + 1 && p.col >= t.range.c0 && p.col <= t.range.c1)
                .map(|i| (i, true))
                .or_else(|| self.sheets[sheet].tables.iter().position(|t| p.col == t.range.c1 + 1 && p.row >= t.range.r0 && p.row <= t.range.r1 && t.range.c1 + 1 < MAX_COLS).map(|i| (i, false)));
            let Some((i, down)) = grow else { return };
            let mut grown = self.sheets[sheet].tables[i].range;
            if down { grown.r1 += 1 } else { grown.c1 += 1 }
            if self.sheets[sheet].tables.iter().enumerate().any(|(k, o)| k != i && o.range.intersects(&grown)) { return; }
            if down {
                self.sheets[sheet].tables[i].range = grown;
                // The new row takes the calculated columns.
                let t = self.sheets[sheet].tables[i].clone();
                let data0 = t.data().r0;
                for (k, c) in t.columns.iter().enumerate() {
                    let q = Pos::new(p.row, t.range.c0 + k as u32);
                    if c.calculated_formula.is_empty() || q == p || !self.sheets[sheet].value(q).is_empty() { continue; }
                    let f = crate::formula::adjust::shift(&c.calculated_formula, p.row as i64 - data0 as i64, 0);
                    self.set_input(sheet, q, &format!("={f}"));
                }
            } else {
                let tb = &mut self.sheets[sheet].tables[i];
                tb.range = grown;
                let names: Vec<String> = tb.columns.iter().map(|c| c.name.clone()).collect();
                let n = unique_in(&names, &format!("Column{}", names.len() + 1), None);
                tb.columns.push(TableColumn { name: n, ..Default::default() });
                if tb.header() != Some(p.row) { self.write_headers(sheet, i); }
            }
        }
        let Some(i) = self.table_at(sheet, p) else { return };
        let t = self.sheets[sheet].tables[i].clone();
        let k = (p.col - t.range.c0) as usize;
        if t.header() == Some(p.row) {
            let typed = self.display(sheet, p).trim().to_string();
            let names: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
            let name = unique_in(&names, if typed.is_empty() { format!("Column{}", k + 1) } else { typed }.as_str(), Some(k));
            let old = std::mem::replace(&mut self.sheets[sheet].tables[i].columns[k].name, name.clone());
            if old != name { let tn = t.display_name.clone(); self.rewrite_formulas(|_, f| rename_column_refs(f, &tn, &old, &name)); }
            self.write_headers(sheet, i);
            return;
        }
        let data = t.data();
        if p.row < data.r0 || p.row > data.r1 { return; }
        let Some(text) = self.sheets[sheet].cells.get(&p).and_then(|c| c.formula()).map(String::from) else { return };
        let others_empty = (data.r0..=data.r1).filter(|r| *r != p.row).all(|r| self.sheets[sheet].cells.get(&Pos::new(r, p.col)).is_none_or(|c| c.is_blank()));
        if !others_empty || data.rows() > 100_000 { return; }
        let base = crate::formula::adjust::shift(&text, data.r0 as i64 - p.row as i64, 0);
        self.sheets[sheet].tables[i].columns[k].calculated_formula = base.clone();
        for r in data.r0..=data.r1 { if r != p.row { let f = crate::formula::adjust::shift(&base, (r - data.r0) as i64, 0); self.set_input(sheet, Pos::new(r, p.col), &format!("={f}")); } }
    }

    /// Sorts a table's data rows by one of its columns.
    pub fn sort_table(&mut self, sheet: usize, i: usize, col: u32, asc: bool) {
        let d = self.sheets[sheet].tables[i].data();
        if d.r0 <= d.r1 { self.sort(sheet, d, &[(d.c0 + col, asc)], false, false); }
        self.apply_filters(sheet);
    }

    /// Deletes repeated data rows of a table, comparing every column; returns how many went.
    pub fn table_remove_duplicates(&mut self, sheet: usize, i: usize) -> usize {
        let d = self.sheets[sheet].tables[i].data();
        if d.r0 > d.r1 { return 0; }
        let mut seen = std::collections::HashSet::new();
        let mut drop = vec![];
        for r in d.r0..=d.r1 {
            let key: Vec<String> = (d.c0..=d.c1).map(|c| self.display(sheet, Pos::new(r, c)).to_lowercase()).collect();
            if !seen.insert(key) { drop.push(r); }
        }
        if drop.len() as u32 >= d.rows() { return 0; }
        for r in drop.iter().rev() { self.shift_cells(sheet, Range { r0: *r, r1: *r, ..d }, true, true); }
        self.sheets[sheet].tables[i].range.r1 -= drop.len() as u32;
        drop.len()
    }

    /// The filter range whose header row holds `p`: its scope, whole range and button column offset.
    pub fn filter_button_at(&self, sheet: usize, p: Pos) -> Option<(Scope, Range, u32)> {
        let s = &self.sheets[sheet];
        if let Some(i) = s.tables.iter().position(|t| t.filter.is_some() && t.header() == Some(p.row) && t.range.contains(p)) { return Some((Scope::Table(i), s.tables[i].range, p.col - s.tables[i].range.c0)); }
        s.autofilter.filter(|a| a.r0 == p.row && a.contains(p)).map(|a| (Scope::Auto, a, p.col - a.c0))
    }
    pub fn filter_of(&self, sheet: usize, scope: Scope) -> Option<&Filter> {
        let s = &self.sheets[sheet];
        match scope { Scope::Auto => s.autofilter.map(|_| &s.filter), Scope::Table(i) => s.tables.get(i).and_then(|t| t.filter.as_ref()) }
    }
    /// The rows a filter's criteria test (below the header, above any totals row).
    pub fn filter_data(&self, sheet: usize, scope: Scope) -> Option<Range> {
        let s = &self.sheets[sheet];
        match scope {
            Scope::Auto => s.autofilter.map(|a| Range { r0: a.r0 + 1, r1: a.r1.max(a.r0 + 1), ..a }),
            Scope::Table(i) => s.tables.get(i).filter(|t| t.filter.is_some()).map(|t| t.data()),
        }
    }
    pub fn set_column_filter(&mut self, sheet: usize, scope: Scope, col: u32, crit: Option<Criteria>) {
        let s = &mut self.sheets[sheet];
        match scope {
            Scope::Auto => if s.autofilter.is_some() { s.filter.set(col, crit) },
            Scope::Table(i) => if let Some(f) = s.tables.get_mut(i).and_then(|t| t.filter.as_mut()) { f.set(col, crit) },
        }
        self.apply_filters(sheet);
    }
    /// Sorts the data of a filter range by one of its columns.
    pub fn sort_filter_range(&mut self, sheet: usize, scope: Scope, col: u32, asc: bool) {
        match scope {
            Scope::Table(i) => self.sort_table(sheet, i, col, asc),
            Scope::Auto => if let Some(d) = self.filter_data(sheet, Scope::Auto) { self.sort(sheet, d, &[(d.c0 + col, asc)], false, false); self.apply_filters(sheet); },
        }
    }

    /// Data ▸ Filter: in a table, shows or hides its buttons; elsewhere turns the sheet's
    /// AutoFilter off, or on over `r` (a single cell takes its current region).
    pub fn toggle_filter(&mut self, sheet: usize, r: Range, active: Pos) {
        if let Some(i) = self.table_at(sheet, active) {
            let t = &mut self.sheets[sheet].tables[i];
            t.filter = if t.filter.is_some() { None } else { Some(Filter::default()) };
        } else if self.sheets[sheet].autofilter.is_some() {
            let s = &mut self.sheets[sheet];
            s.autofilter = None; s.filter = Filter::default();
        } else {
            let r = self.filter_range(sheet, r, active);
            if self.sheets[sheet].tables.iter().any(|t| t.range.intersects(&r)) { return; }
            self.sheets[sheet].autofilter = Some(r);
        }
        self.apply_filters(sheet);
    }
    /// The range Filter covers, as Excel picks it: a single cell or a single row (the header,
    /// even selected as a whole row) grows into the data around and under it; whole rows or
    /// columns are trimmed to the data in them.
    pub fn filter_range(&self, sheet: usize, r: Range, active: Pos) -> Range {
        let whole_rows = r.c0 == 0 && r.c1 == MAX_COLS - 1;
        let whole_cols = r.r0 == 0 && r.r1 == MAX_ROWS - 1;
        if !(r.is_cell() || r.r0 == r.r1 || whole_rows || whole_cols) { return r; }
        // Grow from the active cell, or from the first filled cell in the selection.
        let s = &self.sheets[sheet];
        let start = if !s.value(active).is_empty() { active } else {
            let (c1, r1) = (r.c1.min(r.c0 + 16_384), r.r1.min(r.r0 + 1_048_576));
            s.cells.keys().find(|p| p.row >= r.r0 && p.row <= r1 && p.col >= r.c0 && p.col <= c1 && !s.value(**p).is_empty()).copied().unwrap_or(active)
        };
        let g = self.current_region(sheet, start);
        let mut out = if r.is_cell() { g } else {
            // Keep the columns picked (unless whole rows) and the rows picked (unless one row or whole columns).
            let (c0, c1) = if whole_rows { (g.c0, g.c1) } else { (r.c0.max(g.c0), r.c1.min(g.c1)) };
            let (r0, r1) = if whole_cols { (g.r0, g.r1) } else if r.r0 == r.r1 { (r.r0, g.r1.max(r.r0)) } else { (r.r0, r.r1) };
            if c0 > c1 { r } else { Range { r0, r1, c0, c1 } }
        };
        // A header with nothing under it still gets one row to filter.
        if out.r0 == out.r1 { out.r1 += 1; }
        out
    }
    /// Data ▸ Clear: every filter on the sheet shows all rows again (buttons stay).
    pub fn clear_filters(&mut self, sheet: usize) {
        let s = &mut self.sheets[sheet];
        s.filter.columns.clear();
        for t in &mut s.tables { if let Some(f) = &mut t.filter { f.columns.clear(); } }
        self.apply_filters(sheet);
    }
    pub fn has_filters(&self, sheet: usize) -> bool {
        let s = &self.sheets[sheet];
        (s.autofilter.is_some() && s.filter.is_active()) || s.tables.iter().any(|t| t.filter.as_ref().is_some_and(|f| f.is_active()))
    }

    /// Hides the rows that fail the sheet's filters and shows the ones that pass (Reapply).
    pub fn apply_filters(&mut self, sheet: usize) {
        let mut hide: BTreeSet<u32> = BTreeSet::new();
        let s = &self.sheets[sheet];
        let mut scopes: Vec<Scope> = (0..s.tables.len()).map(Scope::Table).collect();
        scopes.push(Scope::Auto);
        let last = s.extent().map_or(0, |e| e.row);
        for scope in scopes {
            let (Some(f), Some(d)) = (self.filter_of(sheet, scope), self.filter_data(sheet, scope)) else { continue };
            if !f.is_active() || d.r0 > d.r1 { continue; }
            let d = Range { r1: d.r1.min(last.max(d.r0)), ..d };
            for cf in &f.columns {
                if cf.col >= d.cols() { continue; }
                let c = d.c0 + cf.col;
                let ctx = ColumnCtx::new(self, sheet, d, c, &cf.crit);
                for r in d.r0..=d.r1 { if !hide.contains(&r) && !ctx.passes(self, sheet, Pos::new(r, c), &cf.crit) { hide.insert(r); } }
            }
        }
        let s = &mut self.sheets[sheet];
        let manual: BTreeSet<u32> = s.hidden_rows.difference(&s.filtered_rows).copied().collect();
        for r in std::mem::take(&mut s.filtered_rows) { if !hide.contains(&r) { s.hidden_rows.remove(&r); } }
        s.filtered_rows = hide.into_iter().filter(|r| !manual.contains(r)).collect();
        let f: Vec<u32> = s.filtered_rows.iter().copied().collect();
        s.hidden_rows.extend(f);
    }

    /// The distinct shown texts of a filter column, sorted (numbers by value), and whether it has blanks.
    pub fn filter_values(&self, sheet: usize, scope: Scope, col: u32) -> (Vec<String>, bool) {
        let Some(d) = self.filter_data(sheet, scope) else { return (vec![], false) };
        let s = &self.sheets[sheet];
        let last = s.extent().map_or(0, |e| e.row).min(d.r1);
        let c = d.c0 + col;
        let mut blanks = false;
        let mut seen = std::collections::HashSet::new();
        let mut list: Vec<(Value, String)> = vec![];
        // Only rows hidden by this column's own filter, or shown, count (as Excel lists them).
        let others: Option<Filter> = self.filter_of(sheet, scope).map(|f| Filter { columns: f.columns.iter().filter(|x| x.col != col).cloned().collect() });
        for r in d.r0..=last {
            if s.hidden_rows.contains(&r) && !s.filtered_rows.contains(&r) { continue; }
            if let Some(o) = &others { if o.columns.iter().any(|cf| { let ctx = ColumnCtx::new(self, sheet, d, d.c0 + cf.col, &cf.crit); !ctx.passes(self, sheet, Pos::new(r, d.c0 + cf.col), &cf.crit) }) { continue; } }
            let p = Pos::new(r, c);
            let v = s.value(p);
            if v.is_empty() { blanks = true; continue; }
            let t = self.display(sheet, p);
            if seen.insert(t.to_lowercase()) { list.push((v, t)); }
            if list.len() > 10_000 { break; }
        }
        list.sort_by(|a, b| match (&a.0, &b.0) { (Value::Num(x), Value::Num(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal), (Value::Num(_), _) => std::cmp::Ordering::Less, (_, Value::Num(_)) => std::cmp::Ordering::Greater, _ => a.1.to_lowercase().cmp(&b.1.to_lowercase()) });
        (list.into_iter().map(|x| x.1).collect(), blanks)
    }
}

/// What a column's criteria need computed once: the Top 10 threshold, the average, today.
struct ColumnCtx { threshold: f64, avg: f64, today: f64 }

impl ColumnCtx {
    fn new(wb: &Workbook, sheet: usize, d: Range, c: u32, crit: &Criteria) -> ColumnCtx {
        let mut out = ColumnCtx { threshold: 0.0, avg: 0.0, today: crate::numfmt::now_serial().floor() };
        let need = matches!(crit, Criteria::Top10 { .. }) || matches!(crit, Criteria::Dynamic(t) if t.ends_with("Average"));
        if !need { return out; }
        let s = &wb.sheets[sheet];
        let mut nums: Vec<f64> = (d.r0..=d.r1).filter_map(|r| if let Value::Num(n) = s.value(Pos::new(r, c)) { Some(n) } else { None }).collect();
        if nums.is_empty() { return out; }
        out.avg = nums.iter().sum::<f64>() / nums.len() as f64;
        if let Criteria::Top10 { top, percent, n } = crit {
            nums.sort_by(|a, b| if *top { b.total_cmp(a) } else { a.total_cmp(b) });
            let k = if *percent { (nums.len() as f64 * n / 100.0).ceil() } else { n.floor() }.clamp(1.0, nums.len() as f64) as usize;
            out.threshold = nums[k - 1];
        }
        out
    }

    fn passes(&self, wb: &Workbook, sheet: usize, p: Pos, crit: &Criteria) -> bool {
        let v = wb.sheets[sheet].value(p);
        match crit {
            Criteria::Values { values, blanks } => {
                if v.is_empty() { return *blanks; }
                let t = wb.display(sheet, p).to_lowercase();
                values.iter().any(|x| x.to_lowercase() == t)
            }
            Criteria::Custom { a, b, and } => {
                let text = wb.display(sheet, p);
                let x = cond_match(&v, &text, a);
                match b { None => x, Some(b) => { let y = cond_match(&v, &text, b); if *and { x && y } else { x || y } } }
            }
            Criteria::Top10 { top, .. } => match v { Value::Num(n) => if *top { n >= self.threshold } else { n <= self.threshold }, _ => false },
            Criteria::Dynamic(kind) => {
                let Value::Num(n) = v else { return false };
                match kind.as_str() {
                    "aboveAverage" => n > self.avg,
                    "belowAverage" => n < self.avg,
                    k => date_in(k, n.floor(), self.today, wb.date1904),
                }
            }
            Criteria::Color(c) => wb.style_at(sheet, p).fill == *c,
        }
    }
}

/// Whether a date serial falls in a dynamic period relative to `today`.
fn date_in(kind: &str, d: f64, today: f64, d1904: bool) -> bool {
    use crate::date::{serial, weekday, ymd};
    let (ty, tm, _) = ymd(today, d1904);
    let (dy, dm, _) = ymd(d, d1904);
    let month = |off: i64| -> (f64, f64) { let m0 = ty * 12 + tm as i64 - 1 + off; let (y, m) = (m0.div_euclid(12), m0.rem_euclid(12) + 1); (serial(y, m, 1, d1904), serial(y, m + 1, 1, d1904)) };
    let quarter = |off: i64| -> (f64, f64) { let q0 = ty * 4 + (tm as i64 - 1) / 3 + off; let (y, q) = (q0.div_euclid(4), q0.rem_euclid(4)); (serial(y, q * 3 + 1, 1, d1904), serial(y, q * 3 + 4, 1, d1904)) };
    let year = |off: i64| -> (f64, f64) { (serial(ty + off, 1, 1, d1904), serial(ty + off + 1, 1, 1, d1904)) };
    let week = |off: f64| -> (f64, f64) { let s = today - weekday(today, d1904) as f64 + 7.0 * off; (s, s + 7.0) };
    let (a, b) = match kind {
        "today" => (today, today + 1.0), "yesterday" => (today - 1.0, today), "tomorrow" => (today + 1.0, today + 2.0),
        "thisWeek" => week(0.0), "lastWeek" => week(-1.0), "nextWeek" => week(1.0),
        "thisMonth" => month(0), "lastMonth" => month(-1), "nextMonth" => month(1),
        "thisQuarter" => quarter(0), "lastQuarter" => quarter(-1), "nextQuarter" => quarter(1),
        "thisYear" => year(0), "lastYear" => year(-1), "nextYear" => year(1),
        "yearToDate" => (year(0).0, today + 1.0),
        k if k.starts_with('Q') => return k[1..].parse::<u32>().is_ok_and(|q| (dm - 1) / 3 + 1 == q),
        k if k.starts_with('M') => return k[1..].parse::<u32>().is_ok_and(|m| dm == m),
        _ => return true,
    };
    let _ = dy;
    d >= a && d < b
}

/// One custom condition against a cell (numbers compare as numbers; = and <> take wildcards).
pub fn cond_match(v: &Value, shown: &str, c: &Cond) -> bool {
    let num = c.val.trim().parse::<f64>().ok();
    if let (Value::Num(n), Some(x)) = (v, num) {
        return match c.op { CmpOp::Eq => *n == x, CmpOp::Ne => *n != x, CmpOp::Gt => *n > x, CmpOp::Ge => *n >= x, CmpOp::Lt => *n < x, CmpOp::Le => *n <= x };
    }
    let t = shown.to_lowercase();
    let pat = c.val.to_lowercase();
    match c.op {
        CmpOp::Eq => if pat.is_empty() { v.is_empty() } else { wildcard(&pat, &t) },
        CmpOp::Ne => if pat.is_empty() { !v.is_empty() } else { !wildcard(&pat, &t) },
        _ if num.is_some() || v.is_empty() => false,
        CmpOp::Gt => t > pat, CmpOp::Ge => t >= pat, CmpOp::Lt => t < pat, CmpOp::Le => t <= pat,
    }
}

/// * and ? wildcards, ~ escaping them.
pub fn wildcard(pat: &str, t: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let s: Vec<char> = t.chars().collect();
    fn go(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') => (0..=s.len()).any(|k| go(&p[1..], &s[k..])),
            Some('?') => !s.is_empty() && go(&p[1..], &s[1..]),
            Some('~') if p.len() > 1 => s.first() == Some(&p[1]) && go(&p[2..], &s[1..]),
            Some(c) => s.first() == Some(c) && go(&p[1..], &s[1..]),
        }
    }
    if p.len() * s.len() > 1_000_000 { return pat == t; }
    go(&p, &s)
}

pub fn subtotal_code(f: &str) -> Option<u32> {
    Some(match f { "average" => 101, "countNums" => 102, "count" => 103, "max" => 104, "min" => 105, "stdDev" => 107, "sum" => 109, "var" => 110, _ => return None })
}
pub const TOTALS_FUNCTIONS: [(&str, &str); 9] = [("", "None"), ("average", "Average"), ("count", "Count"), ("countNums", "Count Numbers"), ("max", "Max"), ("min", "Min"), ("sum", "Sum"), ("stdDev", "StdDev"), ("var", "Var")];

/// A column name as written inside [ ]: special characters escaped with '.
pub fn struct_col(name: &str) -> String {
    let mut o = String::new();
    for c in name.chars() { if matches!(c, '[' | ']' | '#' | '\'' | '@') { o.push('\''); } o.push(c); }
    if o.contains([' ', ',', ':', '+', '-', '*', '/', '&', '=', '<', '>', '(', ')', '{', '}', '"', '$', '^', '!', '\'']) { format!("[{o}]") } else { o }
}

fn unescape_col(s: &str) -> String {
    let mut o = String::new(); let mut it = s.chars();
    while let Some(c) = it.next() { if c == '\'' { if let Some(n) = it.next() { o.push(n); } } else { o.push(c); } }
    o.trim().to_string()
}

// ---- structured references ----

/// Splits a spec into its top-level items: "[#Headers],[Qty]:[Amount]" → ["#Headers", "Qty:Amount"].
fn spec_items(spec: &str) -> Vec<String> {
    let mut items = vec![]; let mut cur = String::new(); let mut depth = 0; let mut esc = false;
    for c in spec.chars() {
        if esc { cur.push('\''); cur.push(c); esc = false; continue; }
        match c {
            '\'' => esc = true,
            '[' => { depth += 1; if depth > 1 { cur.push(c); } }
            ']' => { depth -= 1; if depth > 0 { cur.push(c); } }
            ',' if depth == 0 => { items.push(std::mem::take(&mut cur)); }
            ':' if depth == 0 => cur.push('\u{1}'),
            _ => cur.push(c),
        }
    }
    items.push(cur);
    items.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Resolves a structured reference written in a cell at (`sheet`, `pos`) to a sheet and range.
pub fn resolve(wb: &Workbook, sheet: usize, pos: Pos, table: Option<&str>, spec: &str) -> Option<(usize, Range)> {
    let (si, ti) = match table { Some(n) => wb.find_table(n)?, None => (sheet, wb.table_at(sheet, pos)?) };
    let t = &wb.sheets[si].tables[ti];
    let spec = spec.trim();
    let items: Vec<String> = if let Some(rest) = spec.strip_prefix('@') {
        let rest = rest.trim();
        let mut v = vec!["#This Row".to_string()];
        if !rest.is_empty() { v.push(rest.trim_start_matches('[').trim_end_matches(']').to_string()); }
        v
    } else if spec.starts_with('[') { spec_items(spec) } else if spec.is_empty() { vec![] } else { vec![spec.to_string()] };
    let (mut rows, mut cols): (Vec<&str>, Option<(u32, u32)>) = (vec![], None);
    for it in &items {
        if it.starts_with('#') { rows.push(it.as_str()); continue; }
        let parts: Vec<&str> = it.split('\u{1}').collect();
        let a = t.col_index(&unescape_col(parts[0]))?;
        let b = parts.get(1).map(|x| t.col_index(&unescape_col(x))).unwrap_or(Some(a))?;
        cols = Some((a.min(b), a.max(b)));
    }
    let (c0, c1) = cols.map_or((t.range.c0, t.range.c1), |(a, b)| (t.range.c0 + a, t.range.c0 + b));
    let d = t.data();
    let (mut r0, mut r1) = (u32::MAX, 0u32);
    let mut take = |a: u32, b: u32| { r0 = r0.min(a); r1 = r1.max(b); };
    if rows.is_empty() { rows.push("#Data"); }
    for k in rows {
        match k.to_ascii_lowercase().as_str() {
            "#all" => take(t.range.r0, t.range.r1),
            "#data" => { if d.r0 > d.r1 { return None; } take(d.r0, d.r1) }
            "#headers" => { let h = t.header()?; take(h, h) }
            "#totals" => { let h = t.totals()?; take(h, h) }
            "#this row" => { if sheet != si || pos.row < d.r0 || pos.row > d.r1 { return None; } take(pos.row, pos.row) }
            _ => return None,
        }
    }
    Some((si, Range { r0, r1, c0, c1 }))
}

/// Rewrites structured references with `f(table name or None, spec, original text)`.
fn map_table_refs(src: &str, mut f: impl FnMut(Option<&str>, &str, &str) -> Option<String>) -> String {
    let Ok(tokens) = tokenize(src) else { return src.to_string() };
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::new(); let mut at = 0;
    for t in tokens {
        if t.start < at || t.end > chars.len() { continue; }
        out.extend(&chars[at..t.start]);
        at = t.end;
        let orig: String = chars[t.start..t.end].iter().collect();
        match &t.tok {
            Tok::TableRef { table, spec } => out.push_str(&f(table.as_deref(), spec, &orig).unwrap_or(orig)),
            _ => out.push_str(&orig),
        }
    }
    out.extend(&chars[at.min(chars.len())..]);
    out
}

fn rename_table_refs(src: &str, old: &str, new: &str) -> String {
    if !src.contains('[') { return src.to_string(); }
    map_table_refs(src, |t, spec, _| t.filter(|t| t.eq_ignore_ascii_case(old)).map(|_| format!("{new}[{spec}]")))
}

fn rename_column_refs(src: &str, table: &str, old: &str, new: &str) -> String {
    if !src.contains('[') || old.is_empty() { return src.to_string(); }
    let (o1, o2) = (format!("[{}]", struct_col(old).trim_start_matches('[').trim_end_matches(']')), struct_col(old));
    let n = struct_col(new);
    let nb = format!("[{}]", n.trim_start_matches('[').trim_end_matches(']'));
    map_table_refs(src, |t, spec, _| {
        if t.is_some_and(|t| !t.eq_ignore_ascii_case(table)) { return None; }
        let s = if spec.eq_ignore_ascii_case(&o2) || spec == old { n.clone() } else if spec.contains(&o1) { spec.replace(&o1, &nb) } else if let Some(r) = spec.strip_prefix('@') { if r == old || r == o1 { format!("@{nb}") } else { return None } } else { return None };
        Some(match t { Some(t) => format!("{t}[{s}]"), None => format!("[{s}]") })
    })
}

/// Structured references to `table` written as A1 references (Convert to Range).
fn table_refs_to_a1(wb: &Workbook, own: &str, src: &str, table: &str, row: u32) -> String {
    if !src.contains('[') { return src.to_string(); }
    let si = wb.sheet_index(own).unwrap_or(0);
    let (tsi, ti) = match wb.find_table(table) { Some(x) => x, None => return src.to_string() };
    let t = &wb.sheets[tsi].tables[ti];
    map_table_refs(src, |name, spec, _| {
        if name.is_some_and(|n| !t.matches(n)) { return None; }
        if spec.trim_start().starts_with('@') || spec.to_ascii_lowercase().contains("#this row") {
            // [@Col] becomes the column's cell on the formula's row: a relative row reference.
            let items = if let Some(r) = spec.trim().strip_prefix('@') { vec![r.trim().trim_start_matches('[').trim_end_matches(']').to_string()] } else { spec_items(spec).into_iter().filter(|x| !x.starts_with('#')).collect() };
            let c = items.first().and_then(|x| t.col_index(&unescape_col(x.split('\u{1}').next().unwrap_or(""))))?;
            let pre = if tsi != si { format!("{}!", crate::addr::quote_sheet(&wb.sheets[tsi].name)) } else { String::new() };
            return Some(format!("{pre}{}{}", col_name(t.range.c0 + c), row + 1));
        }
        let (rs, r) = resolve(wb, tsi, t.range.start(), Some(&t.display_name), spec)?;
        let pre = if rs != si { format!("{}!", crate::addr::quote_sheet(&wb.sheets[rs].name)) } else { String::new() };
        let a1 = |p: Pos| format!("${}${}", col_name(p.col), p.row + 1);
        Some(if r.is_cell() { format!("{pre}{}", a1(r.start())) } else { format!("{pre}{}:{}", a1(r.start()), a1(r.end())) })
    })
}

/// Follows inserted (n > 0) or deleted (n < 0) rows or columns at `at` (Workbook::insert).
/// Insert/Delete Cells sideways across a table's whole height: `n` columns inserted at `at`
/// (n > 0) or deleted from it (n < 0) become new table columns or drop table columns; false
/// when no column is left.
pub(crate) fn shift_table_cols(t: &mut Table, at: u32, n: i64) -> bool {
    let r = t.range;
    let Some(nr) = crate::condformat::shift_range(r, false, at, n) else { return false };
    if n > 0 && at > r.c0 && at <= r.c1 {
        let k = (at - r.c0) as usize;
        for j in 0..n as usize {
            let names: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
            let nm = unique_in(&names, &format!("Column{}", k + j + 1), None);
            t.columns.insert(k + j, TableColumn { name: nm, ..Default::default() });
        }
        if let Some(f) = &mut t.filter { for c in &mut f.columns { if c.col >= k as u32 { c.col += n as u32; } } }
    } else if n < 0 {
        let (d0, d1) = (at.max(r.c0), (at as i64 - n - 1).min(r.c1 as i64) as u32);
        if d0 <= d1 {
            t.columns.drain((d0 - r.c0) as usize..=(d1 - r.c0) as usize);
            if let Some(f) = &mut t.filter { f.columns = f.columns.iter().filter_map(|c| { let x = r.c0 + c.col; if x < d0 { Some(c.clone()) } else if x > d1 { Some(ColumnFilter { col: c.col - (d1 - d0 + 1), crit: c.crit.clone() }) } else { None } }).collect(); }
        }
    }
    t.range = nr;
    !t.columns.is_empty() && t.columns.len() as u32 == nr.cols()
}

/// Excel's message for Insert/Delete Cells that would split a table.
pub const SHIFT_SPLITS_TABLE: &str = "This operation is not allowed. The operation is attempting to shift cells in a table on your worksheet.";
/// Excel's message for Insert/Delete Cells that would split a merged cell.
pub const SHIFT_SPLITS_MERGE: &str = "This operation will cause some merged cells to unmerge. Cannot change part of a merged cell.";

impl Workbook {
    /// Why Insert/Delete Cells over `r` can't be done: the shifted cells would cut through a
    /// table (it reaches past the shifted columns, or rows, of the moving block).
    pub fn shift_cells_blocked(&self, sheet: usize, r: Range, down: bool) -> Option<&'static str> {
        let reach = if down { Range { r0: r.r0, r1: MAX_ROWS - 1, ..r } } else { Range { c0: r.c0, c1: MAX_COLS - 1, ..r } };
        self.sheets[sheet].tables.iter().any(|t| {
            let x = t.range;
            let inside = if down { x.c0 >= r.c0 && x.c1 <= r.c1 } else { x.r0 >= r.r0 && x.r1 <= r.r1 };
            x.intersects(&reach) && !inside
        }).then_some(SHIFT_SPLITS_TABLE).or_else(|| self.sheets[sheet].merges.iter().any(|x| {
            let inside = if down { x.c0 >= r.c0 && x.c1 <= r.c1 } else { x.r0 >= r.r0 && x.r1 <= r.r1 };
            x.intersects(&reach) && !inside
        }).then_some(SHIFT_SPLITS_MERGE))
    }
}

#[cfg(test)]
mod shift_merge_tests {
    use super::*;
    #[test]
    fn shift_refused_across_merge() {
        let mut b = Workbook::new();
        b.sheets[0].merges.push(Range { r0: 2, r1: 2, c0: 0, c1: 2 });
        assert_eq!(b.shift_cells_blocked(0, Range { r0: 0, r1: 0, c0: 1, c1: 1 }, true), Some(SHIFT_SPLITS_MERGE));
        assert_eq!(b.shift_cells_blocked(0, Range { r0: 0, r1: 0, c0: 0, c1: 3 }, true), None);
        assert_eq!(b.shift_cells_blocked(0, Range { r0: 0, r1: 0, c0: 5, c1: 5 }, true), None);
    }
}

pub fn on_insert(s: &mut Sheet, rows: bool, at: u32, n: i64) {
    let shift = |v: u32| -> Option<u32> { if v < at { Some(v) } else if n > 0 { Some(v + n as u32) } else if (v as i64) < at as i64 - n { None } else { Some((v as i64 + n) as u32) } };
    // A span [lo, hi] after the edit (None when all of it is deleted).
    let span = |lo: u32, hi: u32| -> Option<(u32, u32)> {
        if n > 0 { return Some((if lo >= at { lo + n as u32 } else { lo }, if hi >= at { hi + n as u32 } else { hi })); }
        let end = at as i64 - n;
        let f = |v: u32| -> i64 { if v < at { v as i64 } else if (v as i64) >= end { v as i64 + n } else { at as i64 } };
        let (a, b) = (f(lo), if (hi as i64) < end && hi >= at { at as i64 - 1 } else { f(hi) });
        (b >= a).then_some((a as u32, b as u32))
    };
    let mut keep = vec![];
    for mut t in std::mem::take(&mut s.tables) {
        let r = t.range;
        if rows {
            if let Some(h) = t.header() { if n < 0 && h >= at && (h as i64) < at as i64 - n { continue; } }
            let Some((a, b)) = span(r.r0, r.r1) else { continue };
            t.range.r0 = a; t.range.r1 = b;
            if t.range.rows() < 1 + t.header_row as u32 + t.totals_row as u32 { t.range.r1 = t.range.r0 + t.header_row as u32 + t.totals_row as u32; }
        } else {
            let Some((a, b)) = span(r.c0, r.c1) else { continue };
            if n > 0 && at > r.c0 && at <= r.c1 {
                let k = (at - r.c0) as usize;
                for j in 0..n as usize {
                    let names: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
                    let nm = unique_in(&names, &format!("Column{}", k + j + 1), None);
                    t.columns.insert(k + j, TableColumn { name: nm, ..Default::default() });
                }
                if let Some(f) = &mut t.filter { for c in &mut f.columns { if c.col >= k as u32 { c.col += n as u32; } } }
            } else if n < 0 {
                let (d0, d1) = (at.max(r.c0), (at as i64 - n - 1).min(r.c1 as i64) as u32);
                if d0 <= d1 {
                    t.columns.drain((d0 - r.c0) as usize..=(d1 - r.c0) as usize);
                    if let Some(f) = &mut t.filter { f.columns = f.columns.iter().filter_map(|c| { let x = r.c0 + c.col; if x < d0 { Some(c.clone()) } else if x > d1 { Some(ColumnFilter { col: c.col - (d1 - d0 + 1), crit: c.crit.clone() }) } else { None } }).collect(); }
                }
            }
            t.range.c0 = a; t.range.c1 = b;
            if t.columns.len() as u32 != t.range.cols() { continue; }
        }
        keep.push(t);
    }
    s.tables = keep;
    if let Some(af) = s.autofilter {
        s.autofilter = if rows { span(af.r0, af.r1).map(|(a, b)| Range { r0: a, r1: b.max(a + 1), ..af }) } else { span(af.c0, af.c1).map(|(a, b)| Range { c0: a, c1: b, ..af }) };
        if s.autofilter.is_none() { s.filter = Filter::default(); }
    }
    if rows { s.filtered_rows = std::mem::take(&mut s.filtered_rows).into_iter().filter_map(shift).collect(); }
}

// ---- table styles ----

/// What a table style adds to one cell: fill, font color, bold and borders (color, width).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellLook {
    pub fill: Option<[u8; 3]>,
    pub color: Option<[u8; 3]>,
    pub bold: bool,
    pub top: Option<([u8; 3], f32)>,
    pub bottom: Option<([u8; 3], f32)>,
    pub left: Option<([u8; 3], f32)>,
    pub right: Option<([u8; 3], f32)>,
}

/// The built-in styles: Light 1–21, Medium 1–28, Dark 1–11.
pub fn style_names() -> Vec<String> {
    let mut v = vec![];
    for (fam, n) in [("Light", 21), ("Medium", 28), ("Dark", 11)] { for k in 1..=n { v.push(format!("TableStyle{fam}{k}")); } }
    v
}

fn tint(c: [u8; 3], t: f32) -> [u8; 3] { c.map(|x| if t >= 0.0 { (x as f32 + (255.0 - x as f32) * t).round() as u8 } else { (x as f32 * (1.0 + t)).round() as u8 }) }

const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];

/// The palette of a style: (family, number in it, main color, second color).
fn style_parts(wb: &Workbook, name: &str) -> Option<(&'static str, u32, [u8; 3], [u8; 3])> {
    let rest = name.strip_prefix("TableStyle")?;
    let (fam, num) = ["Light", "Medium", "Dark"].into_iter().find_map(|f| rest.strip_prefix(f).and_then(|n| n.parse::<u32>().ok()).map(|n| (f, n)))?;
    let accent = |k: u32| -> [u8; 3] { if k == 0 { BLACK } else { wb.theme_colors.get(3 + k as usize).copied().unwrap_or([0x44, 0x72, 0xC4]) } };
    if fam == "Dark" && num >= 8 {
        let (a, b) = match num { 8 => (0, 0), 9 => (2, 1), 10 => (4, 3), _ => (6, 5) };
        return Some((fam, num, accent(a), if num == 8 { BLACK } else { accent(b) }));
    }
    let c = accent((num - 1) % 7);
    Some((fam, num, c, c))
}

/// Fill, header and line colors for a gallery thumbnail.
pub fn style_preview(wb: &Workbook, name: &str) -> Option<([u8; 3], [u8; 3], [u8; 3])> {
    let t = Table { name: String::new(), display_name: String::new(), range: Range { r0: 0, r1: 4, c0: 0, c1: 2 }, header_row: true, totals_row: false, columns: vec![], style: name.into(), banded_rows: true, banded_cols: false, first_col: false, last_col: false, filter: None };
    let h = look_of(wb, &t, Pos::new(0, 1))?;
    let b = look_of(wb, &t, Pos::new(1, 1)).unwrap_or_default();
    let line = h.bottom.or(b.top).or(b.bottom).map(|x| x.0).or(h.fill).unwrap_or([0x80, 0x80, 0x80]);
    Some((h.fill.unwrap_or(WHITE), b.fill.unwrap_or(WHITE), line))
}

/// The table style's look at `p` (None outside the table or with no style).
pub fn table_look(wb: &Workbook, _sheet: usize, t: &Table, p: Pos) -> Option<CellLook> { look_of(wb, t, p) }

fn look_of(wb: &Workbook, t: &Table, p: Pos) -> Option<CellLook> {
    if !t.range.contains(p) || t.style.is_empty() { return None; }
    let (fam, num, c, c2) = style_parts(wb, &t.style)?;
    let g = (num - 1) / 7;
    let r = t.range;
    let d = t.data();
    let header = t.header() == Some(p.row);
    let totals = t.totals() == Some(p.row);
    let body = !header && !totals;
    let band_row = t.banded_rows && body && (p.row - d.r0) % 2 == 0;
    let band_col = t.banded_cols && body && (p.col - r.c0) % 2 == 0;
    let edge_col = (t.first_col && p.col == r.c0) || (t.last_col && p.col == r.c1);
    let mut l = CellLook::default();
    let thin = 1.0; let thick = 2.0;
    let gray = c == BLACK;
    let light = tint(c, if gray { 0.85 } else { 0.8 });
    match (fam, g) {
        ("Light", 0) => {
            if header { l.bold = true; l.bottom = Some((c, thin)); }
            if p.row == r.r0 { l.top = Some((c, thin)); }
            if p.row == r.r1 { l.bottom = Some((c, thin)); }
            if band_row || band_col { l.fill = Some(light); }
            if !gray { l.color = Some(tint(c, -0.25)); }
            if totals { l.bold = true; l.top = Some((c, thin)); }
        }
        ("Light", 1) => {
            if header { l.fill = Some(c); l.color = Some(WHITE); l.bold = true; }
            if p.row == r.r0 { l.top = Some((c, thin)); }
            if p.row == r.r1 { l.bottom = Some((c, thin)); }
            if p.col == r.c0 { l.left = Some((c, thin)); }
            if p.col == r.c1 { l.right = Some((c, thin)); }
            if band_row { l.top = Some((c, thin)); l.bottom = Some((c, thin)); }
            if band_col { l.left = Some((c, thin)); l.right = Some((c, thin)); }
            if totals { l.bold = true; l.top = Some((c, thick)); }
        }
        ("Light", _) => {
            let all = Some((c, thin));
            l.top = all; l.bottom = all; l.left = all; l.right = all;
            if header { l.bold = true; l.bottom = Some((c, thick)); }
            if band_row || band_col { l.fill = Some(light); }
            if totals { l.bold = true; l.top = Some((c, thick)); }
        }
        ("Medium", 0) => {
            let line = Some((tint(c, 0.4), thin));
            l.top = line; l.bottom = line; l.left = line; l.right = line;
            if header { l.fill = Some(c); l.color = Some(WHITE); l.bold = true; l.top = Some((c, thin)); l.left = None; l.right = None; }
            if band_row || band_col { l.fill = Some(light); }
            if totals { l.bold = true; l.top = Some((c, thick)); }
        }
        ("Medium", 1) => {
            l.fill = Some(light);
            let w = Some((WHITE, thin));
            l.top = w; l.bottom = w; l.left = w; l.right = w;
            if band_row || band_col { l.fill = Some(tint(c, if gray { 0.65 } else { 0.6 })); }
            if header || totals { l.fill = Some(c); l.color = Some(WHITE); l.bold = true; }
            if totals { l.top = Some((WHITE, thick)); }
            if edge_col && body { l.fill = Some(c); l.color = Some(WHITE); }
        }
        ("Medium", 2) => {
            if header { l.fill = Some(c); l.color = Some(WHITE); l.bold = true; }
            if p.row == r.r0 { l.top = Some((BLACK, thick)); }
            if p.row == r.r1 { l.bottom = Some((BLACK, thick)); }
            if header { l.bottom = Some((BLACK, thick)); }
            if band_row || band_col { l.fill = Some(tint(if gray { BLACK } else { c }, 0.8)); }
            if totals { l.bold = true; l.top = Some((BLACK, 3.0)); }
        }
        ("Medium", _) => {
            l.fill = Some(light);
            let line = Some((tint(c, 0.4), thin));
            l.top = line; l.bottom = line; l.left = line; l.right = line;
            if band_row || band_col { l.fill = Some(tint(c, if gray { 0.65 } else { 0.6 })); }
            if header { l.bold = true; l.bottom = Some((c, thick)); }
            if totals { l.bold = true; l.top = Some((c, thick)); }
        }
        (_, _) if num >= 8 => {
            l.fill = Some(tint(c, if gray { 0.85 } else { 0.8 }));
            if band_row || band_col { l.fill = Some(tint(c, if gray { 0.65 } else { 0.6 })); }
            if header { l.fill = Some(c2); l.color = Some(WHITE); l.bold = true; }
            if totals { l.bold = true; l.top = Some((BLACK, 3.0)); }
        }
        _ => {
            let (body_c, band_c) = if gray { ([0x73, 0x73, 0x73], [0x40, 0x40, 0x40]) } else { (c, tint(c, -0.25)) };
            l.fill = Some(body_c); l.color = Some(WHITE);
            if band_row || band_col { l.fill = Some(band_c); }
            if header { l.fill = Some(BLACK); l.bold = true; l.bottom = Some((WHITE, thick)); }
            if totals { l.fill = Some(tint(body_c, -0.5)); l.bold = true; l.top = Some((WHITE, thick)); }
            if edge_col && body { l.fill = Some(tint(body_c, -0.25)); }
        }
    }
    if edge_col { l.bold = true; }
    Some(l)
}

/// The table style's look at a cell of a sheet, if a table covers it.
pub fn look_at(wb: &Workbook, sheet: usize, p: Pos) -> Option<CellLook> {
    let s = &wb.sheets[sheet];
    s.tables.iter().find(|t| t.range.contains(p)).and_then(|t| look_of(wb, t, p))
}

// ---- .xlsx ----

/// Reads an autoFilter element's criteria (date group items and color filters are dropped).
pub fn parse_filter(e: &Element) -> Filter {
    let mut f = Filter::default();
    for fc in e.all("filterColumn") {
        let Some(col) = fc.attr_u32("colId") else { continue };
        let Some(k) = fc.elements().next() else { continue };
        let crit = match k.local() {
            "filters" => {
                if k.child("dateGroupItem").is_some() { continue; }
                Criteria::Values { values: k.all("filter").filter_map(|x| x.attr("val").map(String::from)).collect(), blanks: k.flag("blank", false) }
            }
            "customFilters" => {
                let cs: Vec<Cond> = k.all("customFilter").map(|c| Cond { op: CmpOp::from_xml(c.attr("operator").unwrap_or("equal")), val: c.attr("val").unwrap_or("").to_string() }).collect();
                let Some(a) = cs.first().cloned() else { continue };
                Criteria::Custom { a, b: cs.get(1).cloned(), and: k.flag("and", false) }
            }
            "top10" => Criteria::Top10 { top: k.flag("top", true), percent: k.flag("percent", false), n: k.attr_f64("val").unwrap_or(10.0) },
            "dynamicFilter" => Criteria::Dynamic(k.attr("type").unwrap_or("").to_string()),
            _ => continue,
        };
        f.columns.push(ColumnFilter { col, crit });
    }
    f
}

pub fn filter_xml(r: Range, f: &Filter) -> String {
    let mut o = format!("<autoFilter ref=\"{}\"", r.a1());
    let cols: Vec<&ColumnFilter> = f.columns.iter().filter(|c| !matches!(c.crit, Criteria::Color(_))).collect();
    if cols.is_empty() { o.push_str("/>"); return o; }
    o.push('>');
    for c in cols {
        o.push_str(&format!("<filterColumn colId=\"{}\">", c.col));
        match &c.crit {
            Criteria::Values { values, blanks } => {
                o.push_str(if *blanks { "<filters blank=\"1\">" } else { "<filters>" });
                for v in values { o.push_str(&format!("<filter val=\"{}\"/>", attr(v))); }
                o.push_str("</filters>");
            }
            Criteria::Custom { a, b, and } => {
                o.push_str(if *and && b.is_some() { "<customFilters and=\"1\">" } else { "<customFilters>" });
                for x in std::iter::once(a).chain(b.iter()) { o.push_str(&format!("<customFilter operator=\"{}\" val=\"{}\"/>", x.op.xml(), attr(&x.val))); }
                o.push_str("</customFilters>");
            }
            Criteria::Top10 { top, percent, n } => o.push_str(&format!("<top10{}{} val=\"{n}\"/>", if *top { "" } else { " top=\"0\"" }, if *percent { " percent=\"1\"" } else { "" })),
            Criteria::Dynamic(t) => o.push_str(&format!("<dynamicFilter type=\"{}\"/>", attr(t))),
            Criteria::Color(_) => {}
        }
        o.push_str("</filterColumn>");
    }
    o.push_str("</autoFilter>");
    o
}

/// A table part (xl/tables/tableN.xml).
pub fn table_from_xml(src: &str) -> Option<Table> {
    let e = Element::parse(src).ok()?;
    let range = e.attr("ref").and_then(parse_range)?;
    let header_row = e.attr_u32("headerRowCount").unwrap_or(1) > 0;
    let totals_row = e.attr_u32("totalsRowCount").unwrap_or(0) > 0;
    let from = crate::xlsx::from_file_formula;
    let columns: Vec<TableColumn> = e.child("tableColumns").map(|c| c.all("tableColumn").map(|tc| TableColumn {
        name: crate::xlsx::xml::decode_x(tc.attr("name").unwrap_or("")),
        totals_function: tc.attr("totalsRowFunction").unwrap_or("").replace("none", ""),
        totals_label: tc.attr("totalsRowLabel").unwrap_or("").into(),
        totals_formula: tc.child("totalsRowFormula").map(|x| from(x.text().trim())).unwrap_or_default(),
        calculated_formula: tc.child("calculatedColumnFormula").map(|x| from(x.text().trim())).unwrap_or_default(),
    }).collect()).unwrap_or_default();
    if columns.len() as u32 != range.cols() { return None; }
    let si = e.child("tableStyleInfo");
    let name = e.attr("name").or(e.attr("displayName")).unwrap_or("Table1").to_string();
    Some(Table {
        display_name: e.attr("displayName").map(String::from).unwrap_or_else(|| name.clone()), name, range, header_row, totals_row, columns,
        style: si.and_then(|s| s.attr("name")).unwrap_or("").into(),
        banded_rows: si.is_some_and(|s| s.flag("showRowStripes", false)), banded_cols: si.is_some_and(|s| s.flag("showColumnStripes", false)),
        first_col: si.is_some_and(|s| s.flag("showFirstColumn", false)), last_col: si.is_some_and(|s| s.flag("showLastColumn", false)),
        filter: e.child("autoFilter").map(parse_filter),
    })
}

pub fn table_xml(t: &Table, id: u32) -> String {
    let to = crate::xlsx::to_file_formula;
    let mut o = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" id=\"{id}\" name=\"{}\" displayName=\"{}\" ref=\"{}\"", attr(&t.name), attr(&t.display_name), t.range.a1());
    if !t.header_row { o.push_str(" headerRowCount=\"0\""); }
    if t.totals_row { o.push_str(" totalsRowCount=\"1\""); } else { o.push_str(" totalsRowShown=\"0\""); }
    o.push('>');
    if let Some(f) = &t.filter { if t.header_row { o.push_str(&filter_xml(Range { r1: t.range.r1 - t.totals_row as u32, ..t.range }, f)); } }
    o.push_str(&format!("<tableColumns count=\"{}\">", t.columns.len()));
    for (k, c) in t.columns.iter().enumerate() {
        o.push_str(&format!("<tableColumn id=\"{}\" name=\"{}\"", k + 1, attr(&c.name)));
        if !c.totals_function.is_empty() { o.push_str(&format!(" totalsRowFunction=\"{}\"", attr(&c.totals_function))); }
        if !c.totals_label.is_empty() { o.push_str(&format!(" totalsRowLabel=\"{}\"", attr(&c.totals_label))); }
        if c.calculated_formula.is_empty() && !(c.totals_function == "custom" && !c.totals_formula.is_empty()) { o.push_str("/>"); continue; }
        o.push('>');
        if !c.calculated_formula.is_empty() { o.push_str(&format!("<calculatedColumnFormula>{}</calculatedColumnFormula>", text(&to(&c.calculated_formula)))); }
        if c.totals_function == "custom" && !c.totals_formula.is_empty() { o.push_str(&format!("<totalsRowFormula>{}</totalsRowFormula>", text(&to(&c.totals_formula)))); }
        o.push_str("</tableColumn>");
    }
    o.push_str("</tableColumns>");
    if !t.style.is_empty() { o.push_str(&format!("<tableStyleInfo name=\"{}\" showFirstColumn=\"{}\" showLastColumn=\"{}\" showRowStripes=\"{}\" showColumnStripes=\"{}\"/>", attr(&t.style), t.first_col as u8, t.last_col as u8, t.banded_rows as u8, t.banded_cols as u8)); }
    o.push_str("</table>");
    o
}

/// After a worksheet is read: the AutoFilter's criteria become the model, the carried
/// tableParts element goes (tables are written from the model), and rows the file hid inside
/// filtered ranges count as filtered.
pub fn after_read(wb: &mut Workbook, sheet: usize) {
    let s = &mut wb.sheets[sheet];
    if let Some(i) = s.extra.iter().position(|(k, _)| k == "autoFilter") {
        let raw = s.extra.remove(i).1;
        if let Ok(e) = Element::parse(&raw) { s.filter = parse_filter(&e); }
    }
    s.extra.retain(|(k, _)| k != "tableParts");
    let mut scopes: Vec<Scope> = (0..s.tables.len()).map(Scope::Table).collect();
    scopes.push(Scope::Auto);
    let mut rows = BTreeSet::new();
    for sc in scopes {
        let (Some(f), Some(d)) = (wb.filter_of(sheet, sc), wb.filter_data(sheet, sc)) else { continue };
        if !f.is_active() { continue; }
        rows.extend(wb.sheets[sheet].hidden_rows.range(d.r0..=d.r1).copied());
    }
    wb.sheets[sheet].filtered_rows = rows;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::parse_a1;
    fn p(a: &str) -> Pos { parse_a1(a).unwrap() }

    fn sample() -> Workbook {
        let mut wb = Workbook::new();
        for (i, row) in [["Name", "Qty", "Price"], ["Apple", "3", "1.5"], ["Pear", "5", "2"], ["Fig", "1", "4"], ["Apple", "2", "1.5"]].iter().enumerate() {
            for (j, v) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, j as u32), v); }
        }
        wb
    }

    #[test]
    fn create_and_structured_refs() {
        let mut wb = sample();
        let i = wb.create_table(0, parse_range("A1:C5").unwrap(), true, "").unwrap();
        assert_eq!(wb.sheets[0].tables[i].name, "Table1");
        wb.set_input(0, p("E1"), "=SUM(Table1[Qty])");
        wb.set_input(0, p("E2"), "=Table1[[#Headers],[Price]]");
        wb.set_input(0, p("E3"), "=ROWS(Table1[#All])");
        wb.set_input(0, p("D2"), "=[@Qty]*[@Price]");
        wb.recalc();
        assert_eq!(wb.sheets[0].value(p("E1")), Value::Num(11.0));
        assert_eq!(wb.sheets[0].value(p("E2")), Value::str("Price"));
        assert_eq!(wb.sheets[0].value(p("E3")), Value::Num(5.0));
        // D is right of the table: typing there grows it; the formula is a calculated column.
        wb.table_after_input(0, p("D2"));
        let t = &wb.sheets[0].tables[0];
        assert_eq!(t.range, parse_range("A1:D5").unwrap());
        wb.recalc();
        assert_eq!(wb.sheets[0].value(p("D3")), Value::Num(10.0));
        assert_eq!(wb.display(0, p("D1")), "Column4");
        // Typing below adds a row and the calculated column follows.
        wb.set_input(0, p("A6"), "Kiwi"); wb.table_after_input(0, p("A6"));
        assert_eq!(wb.sheets[0].tables[0].range.r1, 5);
        assert!(wb.sheets[0].cells[&p("D6")].is_formula());
    }

    #[test]
    fn headers_unique_totals_and_filters() {
        let mut wb = sample();
        wb.create_table(0, parse_range("A1:C5").unwrap(), true, "").unwrap();
        wb.set_input(0, p("B1"), "Name"); wb.table_after_input(0, p("B1"));
        assert_eq!(wb.display(0, p("B1")), "Name2");
        wb.set_table_totals(0, 0, true);
        wb.recalc();
        assert_eq!(wb.display(0, p("A6")), "Total");
        assert_eq!(wb.sheets[0].value(p("C6")), Value::Num(9.0));
        wb.set_column_filter(0, Scope::Table(0), 0, Some(Criteria::Values { values: vec!["apple".into()], blanks: false }));
        assert_eq!(wb.sheets[0].filtered_rows.iter().copied().collect::<Vec<_>>(), vec![2, 3]);
        wb.recalc();
        assert_eq!(wb.sheets[0].value(p("C6")), Value::Num(3.0));
        wb.set_column_filter(0, Scope::Table(0), 1, Some(Criteria::Custom { a: Cond { op: CmpOp::Gt, val: "2".into() }, b: None, and: false }));
        assert_eq!(wb.sheets[0].filtered_rows.len(), 3);
        wb.clear_filters(0);
        assert!(wb.sheets[0].hidden_rows.is_empty());
        wb.set_column_filter(0, Scope::Table(0), 1, Some(Criteria::Top10 { top: true, percent: false, n: 2.0 }));
        assert_eq!(wb.sheets[0].filtered_rows.iter().copied().collect::<Vec<_>>(), vec![3, 4]);
    }

    #[test]
    fn insert_rows_cols_and_convert() {
        let mut wb = sample();
        wb.create_table(0, parse_range("A1:C5").unwrap(), true, "TableStyleLight9").unwrap();
        wb.insert(0, true, 2, 1);
        assert_eq!(wb.sheets[0].tables[0].range, parse_range("A1:C6").unwrap());
        wb.insert(0, false, 1, 1);
        assert_eq!(wb.sheets[0].tables[0].columns.len(), 4);
        wb.insert(0, false, 1, -1);
        assert_eq!(wb.sheets[0].tables[0].columns[1].name, "Qty");
        wb.set_input(0, p("E1"), "=SUM(Table1[Qty])");
        wb.convert_to_range(0, 0);
        assert!(wb.sheets[0].tables.is_empty());
        assert_eq!(wb.edit_text(0, p("E1")), "=SUM($B$2:$B$6)");
        assert!(wb.style_at(0, p("A1")).bold);
    }

    #[test]
    fn no_headers_and_autofilter() {
        let mut wb = sample();
        wb.create_table(0, parse_range("A2:C5").unwrap(), false, "").unwrap();
        assert_eq!(wb.display(0, p("A2")), "Column1");
        assert_eq!(wb.display(0, p("A3")), "Apple");
        let mut wb = sample();
        wb.toggle_filter(0, Range::cell(p("B2")), p("B2"));
        assert_eq!(wb.sheets[0].autofilter, parse_range("A1:C5"));
        wb.set_column_filter(0, Scope::Auto, 2, Some(Criteria::Values { values: vec!["1.5".into()], blanks: false }));
        assert_eq!(wb.sheets[0].filtered_rows.len(), 2);
        wb.sort_filter_range(0, Scope::Auto, 1, false);
        assert_eq!(wb.display(0, p("A2")), "Pear");
        assert!(look_at(&wb, 0, p("A1")).is_none());
        assert_eq!(style_names().len(), 60);
    }

    #[test]
    fn wildcards() {
        assert!(wildcard("a*e", "apple"));
        assert!(!wildcard("a?e", "apple"));
        assert!(wildcard("~*", "*"));
    }
}

#[cfg(test)]
mod filter_range_tests {
    use crate::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
    use crate::book::Workbook;

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        for (r, row) in [["Region", "Item", "Sales"], ["East", "Pen", "10"], ["West", "Pen", ""], ["East", "Ink", "5"]].iter().enumerate() {
            for (c, v) in row.iter().enumerate() { if !v.is_empty() { wb.set_input(0, Pos::new(r as u32, c as u32), v); } }
        }
        wb.recalc();
        wb
    }

    #[test]
    fn header_row_covers_the_data_under_it() {
        let wb = book();
        let full = Range { r0: 0, r1: 3, c0: 0, c1: 2 };
        // The header cells, the whole first row, one header cell, and the whole columns.
        assert_eq!(wb.filter_range(0, Range { r0: 0, r1: 0, c0: 0, c1: 2 }, Pos::new(0, 0)), full);
        assert_eq!(wb.filter_range(0, Range { r0: 0, r1: 0, c0: 0, c1: MAX_COLS - 1 }, Pos::new(0, 0)), full);
        assert_eq!(wb.filter_range(0, Range::cell(Pos::new(0, 1)), Pos::new(0, 1)), full);
        assert_eq!(wb.filter_range(0, Range { r0: 0, r1: MAX_ROWS - 1, c0: 0, c1: 2 }, Pos::new(0, 0)), full);
        // Part of the header keeps just those columns; a picked block stays as picked.
        assert_eq!(wb.filter_range(0, Range { r0: 0, r1: 0, c0: 1, c1: 2 }, Pos::new(0, 1)), Range { r0: 0, r1: 3, c0: 1, c1: 2 });
        assert_eq!(wb.filter_range(0, Range { r0: 0, r1: 2, c0: 0, c1: 1 }, Pos::new(0, 0)), Range { r0: 0, r1: 2, c0: 0, c1: 1 });
    }

    #[test]
    fn filtering_from_the_whole_header_row_filters_the_rows_below() {
        let mut wb = book();
        wb.toggle_filter(0, Range { r0: 0, r1: 0, c0: 0, c1: MAX_COLS - 1 }, Pos::new(0, 0));
        assert_eq!(wb.sheets[0].autofilter, Some(Range { r0: 0, r1: 3, c0: 0, c1: 2 }));
    }
}
