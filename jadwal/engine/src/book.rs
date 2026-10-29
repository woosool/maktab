//! The workbook model (workbook.js): sheets of cells, each holding what was typed (a value or a
//! formula), its calculated value and a style; defined names; and the editing operations the
//! app's commands use (enter, clear, fill, sort, insert and delete rows and columns, merge).

use crate::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
use crate::formula::{adjust, parser::Expr};
use crate::style::{Style, Styles};
use crate::value::{ErrKind, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct Formula { pub text: String, pub ast: Result<Expr, String> }

impl Formula {
    pub fn new(text: &str) -> Formula {
        Formula { text: text.to_string(), ast: crate::formula::parser::parse(text).map_err(|e| e.message) }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Input {
    #[default]
    Empty,
    Value(Value),
    Formula(Arc<Formula>),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Cell {
    pub input: Input,
    /// The calculated value (for a value cell, the value itself).
    pub value: Value,
    pub style: u32,
    /// A dynamic-array formula entered as a legacy array (CSE) over this range.
    pub array: Option<Range>,
}

impl Cell {
    pub fn is_formula(&self) -> bool { matches!(self.input, Input::Formula(_)) }
    pub fn formula(&self) -> Option<&str> { if let Input::Formula(f) = &self.input { Some(&f.text) } else { None } }
    pub fn is_blank(&self) -> bool { matches!(self.input, Input::Empty) }
    /// What the formula bar shows: =formula, or the value as typed.
    pub fn edit_text(&self, fmt: &str, date1904: bool) -> String {
        match &self.input {
            Input::Empty => String::new(),
            Input::Formula(f) => format!("={}", f.text),
            Input::Value(v) => crate::numfmt::edit_text(v, fmt, date1904),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hyperlink { pub target: String, pub tooltip: Option<String> }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Sheet {
    pub name: String,
    pub cells: BTreeMap<Pos, Cell>,
    /// Column widths and row heights in pixels, where they differ from the default.
    pub col_widths: BTreeMap<u32, f32>,
    pub row_heights: BTreeMap<u32, f32>,
    pub default_col_width: f32,
    pub default_row_height: f32,
    pub hidden_rows: BTreeSet<u32>,
    pub hidden_cols: BTreeSet<u32>,
    /// Styles for whole columns and rows (cells without their own style use them).
    pub col_styles: BTreeMap<u32, u32>,
    pub row_styles: BTreeMap<u32, u32>,
    pub merges: Vec<Range>,
    pub frozen_rows: u32,
    pub frozen_cols: u32,
    /// The first row and column the frozen panes show (rows scrolled away when freezing stay hidden, as in Excel).
    pub frozen_origin: Pos,
    pub notes: BTreeMap<Pos, String>,
    pub links: BTreeMap<Pos, Hyperlink>,
    pub tab_color: Option<[u8; 3]>,
    pub gridlines: bool,
    pub headers: bool,
    pub show_zeros: bool,
    pub right_to_left: bool,
    pub hidden: bool,
    pub zoom: u32,
    pub selection: Option<Range>,
    pub autofilter: Option<Range>,
    /// The AutoFilter's criteria, tables (ListObjects), and the rows filters hid (also in hidden_rows).
    pub filter: crate::tables::Filter,
    pub tables: Vec<crate::tables::Table>,
    pub filtered_rows: BTreeSet<u32>,
    pub protected: bool,
    /// How the sheet prints (page setup, margins, header and footer, breaks).
    pub page: crate::print::PageSetup,
    /// Parts of the file this model does not edit, kept so saving loses nothing (raw XML by kind).
    pub extra: Vec<(String, String)>,
    /// Spilled array values, by cell, and the formula each came from.
    pub spill: HashMap<Pos, Value>,
    pub spill_anchor: HashMap<Pos, Pos>,
    /// The last extent worked out, with what it was worked out from (see `extent`).
    pub extent_cache: ExtentCache,
    /// Conditional formats in priority order, and data validation rules.
    pub cond_formats: Vec<crate::condformat::CondFormat>,
    pub validations: Vec<crate::validation::Validation>,
    pub cf_cache: crate::condformat::Cache,
    /// Floating objects: charts, pictures, shapes, slicers (drawing.rs).
    pub objects: Vec<crate::drawing::Object>,
    /// PivotTables whose reports sit on this sheet (pivot.rs).
    pub pivots: Vec<crate::pivot::Pivot>,
    /// Sparkline groups drawn in this sheet's cells (sparkline.rs).
    pub sparklines: Vec<crate::sparkline::Group>,
    /// Threaded comments, protection details, edit ranges and split panes (review.rs).
    pub review: crate::review::SheetReview,
    /// Row and column groups (outline.rs).
    pub outline: crate::outline::Outline,
}

pub const DEFAULT_COL_WIDTH: f32 = 64.0;
pub const DEFAULT_ROW_HEIGHT: f32 = 20.0;

impl Sheet {
    pub fn new(name: &str) -> Sheet {
        Sheet { name: name.into(), default_col_width: DEFAULT_COL_WIDTH, default_row_height: DEFAULT_ROW_HEIGHT, gridlines: true, headers: true, show_zeros: true, zoom: 100, ..Default::default() }
    }
    pub fn col_width(&self, c: u32) -> f32 { if self.hidden_cols.contains(&c) { 0.0 } else { *self.col_widths.get(&c).unwrap_or(&self.default_col_width) } }
    pub fn row_height(&self, r: u32) -> f32 { if self.hidden_rows.contains(&r) { 0.0 } else { *self.row_heights.get(&r).unwrap_or(&self.default_row_height) } }
    /// The last row and column holding anything.
    pub fn extent(&self) -> Option<Pos> {
        // Walking millions of cells every frame froze big files, so the answer is kept until the
        // cells change: their count, first and last positions and the spill count mark a change.
        let key = (self.cells.len(), self.cells.keys().next().copied(), self.cells.keys().next_back().copied(), self.spill.len());
        if let Some((k, e)) = *self.extent_cache.0.lock().unwrap() { if k == key { return e; } }
        let e = self.extent_scan();
        *self.extent_cache.0.lock().unwrap() = Some((key, e));
        e
    }
    fn extent_scan(&self) -> Option<Pos> {
        let mut r = None; let mut c = 0;
        for (p, cell) in &self.cells { if !cell.is_blank() { r = Some(p.row); c = c.max(p.col); } }
        for p in self.spill.keys() { r = Some(r.unwrap_or(0).max(p.row)); c = c.max(p.col); }
        r.map(|r| Pos::new(r, c))
    }
    pub fn value(&self, p: Pos) -> Value {
        match self.cells.get(&p) { Some(c) if !c.is_blank() => c.value.clone(), _ => self.spill.get(&p).cloned().unwrap_or(Value::Empty) }
    }
    /// The merged area that starts at or covers `p`.
    pub fn merge_at(&self, p: Pos) -> Option<Range> { self.merges.iter().copied().find(|m| m.contains(p)) }
    pub fn style_index(&self, p: Pos) -> u32 {
        if let Some(c) = self.cells.get(&p) { return c.style; }
        self.row_styles.get(&p.row).or_else(|| self.col_styles.get(&p.col)).copied().unwrap_or(0)
    }
    pub fn cell_mut(&mut self, p: Pos) -> &mut Cell {
        let style = self.style_index(p);
        self.cells.entry(p).or_insert_with(|| Cell { style, ..Default::default() })
    }
    fn prune(&mut self, p: Pos) {
        if let Some(c) = self.cells.get(&p) { if c.is_blank() && c.style == 0 && c.array.is_none() { self.cells.remove(&p); } }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DefinedName { pub name: String, pub sheet: Option<usize>, pub formula: String, pub hidden: bool, pub comment: String }

#[derive(Clone, Debug)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub styles: Styles,
    pub names: Vec<DefinedName>,
    pub active: usize,
    pub date1904: bool,
    /// Manual calculation: recalc only when asked (F9).
    pub manual: bool,
    pub iterate: Option<(u32, f64)>,
    /// Automatic Except for Data Tables (calcMode="autoNoTable").
    pub calc_no_tables: bool,
    /// Package parts kept as they were read (themes, drawings and anything not modelled).
    pub parts: BTreeMap<String, Vec<u8>>,
    /// The theme's colors (dk1, lt1, dk2, lt2, accent1-6, hlink, folHlink).
    pub theme_colors: Vec<[u8; 3]>,
    /// The theme's names, fonts and effects (theme.rs).
    pub theme: crate::theme::Theme,
    /// People who comment, structure protection and custom views (review.rs).
    pub review: crate::review::BookReview,
}

impl Default for Workbook { fn default() -> Self { Workbook::new() } }

/// What went wrong when a name or edit is refused.
pub type EditResult = Result<(), String>;

impl Workbook {
    pub fn new() -> Workbook {
        Workbook { sheets: vec![Sheet::new("Sheet1")], styles: Styles::default(), names: vec![], active: 0, date1904: false, manual: false, iterate: None, calc_no_tables: false, parts: BTreeMap::new(), theme_colors: crate::numfmt::OFFICE_THEME.to_vec(), theme: Default::default(), review: Default::default() }
    }
    pub fn sheet(&self, i: usize) -> &Sheet { &self.sheets[i] }
    pub fn sheet_index(&self, name: &str) -> Option<usize> { self.sheets.iter().position(|s| s.name.eq_ignore_ascii_case(name)) }
    pub fn style_at(&self, sheet: usize, p: Pos) -> &Style { self.styles.get(self.sheets[sheet].style_index(p)) }

    /// The text a cell shows, formatted.
    pub fn display(&self, sheet: usize, p: Pos) -> String {
        let s = &self.sheets[sheet];
        let v = s.value(p);
        if v.is_empty() { return String::new(); }
        if !s.show_zeros && v == Value::Num(0.0) { return String::new(); }
        // A picture in cell shows as its alt text wherever text is needed (CSV, clipboard).
        if let Some((_, alt)) = crate::cellimage::parse(&v) { return alt.to_string(); }
        crate::numfmt::format(&v, &self.style_at(sheet, p).num_fmt, self.date1904).text
    }

    pub fn edit_text(&self, sheet: usize, p: Pos) -> String {
        self.sheets[sheet].cells.get(&p).map(|c| c.edit_text(&self.styles.get(c.style).num_fmt, self.date1904)).unwrap_or_default()
    }

    // ---- sheets ----

    pub fn valid_sheet_name(&self, name: &str, except: Option<usize>) -> EditResult {
        let n = name.trim();
        if n.is_empty() { return Err("A sheet name can't be empty.".into()); }
        if n.chars().count() > 31 { return Err("A sheet name can have at most 31 characters.".into()); }
        if n.contains(['\\', '/', '?', '*', '[', ']', ':']) || n.starts_with('\'') || n.ends_with('\'') { return Err("A sheet name can't contain \\ / ? * [ ] : or start or end with '.".into()); }
        if n.eq_ignore_ascii_case("History") { return Err("History is a reserved name.".into()); }
        if self.sheets.iter().enumerate().any(|(i, s)| Some(i) != except && s.name.eq_ignore_ascii_case(n)) { return Err("That name is already taken.".into()); }
        Ok(())
    }
    pub fn unique_sheet_name(&self, base: &str) -> String {
        (1..).map(|i| format!("{base}{i}")).find(|n| self.sheet_index(n).is_none()).unwrap()
    }
    pub fn add_sheet(&mut self, at: usize, name: Option<String>) -> usize {
        let name = name.filter(|n| self.valid_sheet_name(n, None).is_ok()).unwrap_or_else(|| self.unique_sheet_name("Sheet"));
        let at = at.min(self.sheets.len());
        self.sheets.insert(at, Sheet::new(&name));
        for n in &mut self.names { if let Some(s) = &mut n.sheet { if *s >= at { *s += 1; } } }
        at
    }
    pub fn rename_sheet(&mut self, i: usize, name: &str) -> EditResult {
        self.valid_sheet_name(name, Some(i))?;
        let old = std::mem::replace(&mut self.sheets[i].name, name.trim().to_string());
        let new = self.sheets[i].name.clone();
        self.rewrite_formulas(|_, f| adjust::rename_sheet(f, &old, &new));
        crate::follow::follow(self, crate::follow::Edit::Rename { old: &old, new: &new });
        Ok(())
    }
    pub fn delete_sheet(&mut self, i: usize) -> EditResult {
        if self.sheets.iter().filter(|s| !s.hidden).count() <= 1 && !self.sheets[i].hidden { return Err("A workbook must keep at least one visible sheet.".into()); }
        let gone = self.sheets.remove(i).name;
        self.names.retain(|n| n.sheet != Some(i));
        for n in &mut self.names { if let Some(s) = &mut n.sheet { if *s > i { *s -= 1; } } }
        self.rewrite_formulas(|_, f| adjust::drop_sheet(f, &gone));
        crate::follow::follow(self, crate::follow::Edit::DeleteSheet { gone: &gone });
        if self.active >= self.sheets.len() { self.active = self.sheets.len() - 1; }
        if self.sheets[self.active].hidden { self.active = self.sheets.iter().position(|s| !s.hidden).unwrap_or(0); }
        Ok(())
    }
    pub fn move_sheet(&mut self, from: usize, to: usize) {
        if from == to || from >= self.sheets.len() { return; }
        let s = self.sheets.remove(from);
        let to = to.min(self.sheets.len());
        self.sheets.insert(to, s);
        let map = |x: usize| if x == from { to } else if from < to && x > from && x <= to { x - 1 } else if to < from && x >= to && x < from { x + 1 } else { x };
        for n in &mut self.names { if let Some(s) = &mut n.sheet { *s = map(*s); } }
        self.active = map(self.active);
    }
    pub fn duplicate_sheet(&mut self, i: usize) -> usize {
        let mut s = self.sheets[i].clone();
        let base = s.name.clone();
        s.name = (2..).map(|k| format!("{base} ({k})")).find(|n| self.sheet_index(n).is_none() && n.chars().count() <= 31).unwrap_or_else(|| self.unique_sheet_name("Sheet"));
        s.hidden = false;
        self.sheets.insert(i + 1, s);
        for n in &mut self.names { if let Some(x) = &mut n.sheet { if *x > i { *x += 1; } } }
        crate::follow::follow(self, crate::follow::Edit::DuplicateSheet { orig: i, copy: i + 1 });
        i + 1
    }

    /// Rewrites every formula's text (cells and defined names) with `f(sheet name, text)`.
    pub fn rewrite_formulas(&mut self, f: impl Fn(&str, &str) -> String) {
        for s in &mut self.sheets {
            let own = s.name.clone();
            for c in s.cells.values_mut() {
                if let Input::Formula(fm) = &c.input {
                    let t = f(&own, &fm.text);
                    if t != fm.text { c.input = Input::Formula(Arc::new(Formula::new(&t))); }
                }
            }
            let mut g = |x: &mut String| if !x.is_empty() { *x = f(&own, x); };
            for cf in &mut s.cond_formats { cf.kind.each_formula(&mut g); }
            for v in &mut s.validations { v.each_formula(&mut g); }
        }
        let names: Vec<String> = self.sheets.iter().map(|s| s.name.clone()).collect();
        for n in &mut self.names {
            let own = n.sheet.and_then(|i| names.get(i)).cloned().unwrap_or_default();
            n.formula = f(&own, &n.formula);
        }
    }

    // ---- cells ----

    /// What typing `text` into a cell stores: a formula, a number (with the format its spelling
    /// implies, as 12% or 1/2/2024), a logical, an error or text.
    pub fn parse_input(&self, text: &str) -> (Input, Option<String>) {
        if text.is_empty() { return (Input::Empty, None); }
        if let Some(t) = text.strip_prefix('\'') { return (Input::Value(Value::str(t)), None); }
        if text.len() > 1 && text.starts_with('=') {
            return (Input::Formula(Arc::new(Formula::new(text[1..].trim_start()))), None);
        }
        if let Some((n, fmt)) = crate::numfmt::parse_input(text, self.date1904) { return (Input::Value(Value::Num(n)), fmt); }
        match text.trim().to_ascii_uppercase().as_str() { "TRUE" => return (Input::Value(Value::Bool(true)), None), "FALSE" => return (Input::Value(Value::Bool(false)), None), _ => {} }
        if let Some(e) = ErrKind::parse(text.trim()) { return (Input::Value(Value::Err(e)), None); }
        (Input::Value(Value::str(text)), None)
    }

    /// Enters text into a cell as typing does; the caller recalculates.
    pub fn set_input(&mut self, sheet: usize, p: Pos, text: &str) {
        let (input, fmt) = self.parse_input(text);
        self.set_cell_input(sheet, p, input, fmt);
    }

    pub fn set_cell_input(&mut self, sheet: usize, p: Pos, input: Input, fmt: Option<String>) {
        if let Some(fmt) = fmt {
            // The implied format applies only where the cell still shows General.
            let cur = self.style_at(sheet, p).clone();
            if cur.num_fmt == "General" { let mut s = cur; s.num_fmt = fmt; let i = self.styles.intern(s); self.sheets[sheet].cell_mut(p).style = i; }
        }
        let s = &mut self.sheets[sheet];
        let c = s.cell_mut(p);
        c.value = match &input { Input::Value(v) => v.clone(), _ => Value::Empty };
        c.input = input;
        c.array = None;
        s.prune(p);
    }

    /// Clears contents, formats or both (and notes and links with `all`).
    pub fn clear(&mut self, sheet: usize, r: Range, contents: bool, formats: bool, all: bool) {
        let s = &mut self.sheets[sheet];
        let keys: Vec<Pos> = s.cells.range(Pos::new(r.r0, 0)..=Pos::new(r.r1, MAX_COLS)).map(|(p, _)| *p).filter(|p| r.contains(*p)).collect();
        for p in keys {
            let c = s.cells.get_mut(&p).unwrap();
            if contents { c.input = Input::Empty; c.value = Value::Empty; c.array = None; }
            if formats { c.style = 0; }
            s.prune(p);
        }
        if all {
            s.notes.retain(|p, _| !r.contains(*p));
            s.links.retain(|p, _| !r.contains(*p));
            s.merges.retain(|m| !r.intersects(m));
        }
    }

    /// Changes the style of every cell in the ranges with `f`. Whole columns and rows are
    /// styled as columns and rows, so blank cells take the style without being stored.
    pub fn apply_style(&mut self, sheet: usize, ranges: &[Range], f: &dyn Fn(&mut Style)) {
        let mut memo: HashMap<u32, u32> = HashMap::new();
        let mut restyle = |styles: &mut Styles, i: u32| -> u32 {
            *memo.entry(i).or_insert_with(|| { let mut st = styles.get(i).clone(); f(&mut st); styles.intern(st) })
        };
        for r in ranges {
            let whole_cols = r.r0 == 0 && r.r1 == MAX_ROWS - 1;
            let whole_rows = r.c0 == 0 && r.c1 == MAX_COLS - 1;
            if whole_cols || whole_rows {
                if whole_cols && !whole_rows {
                    for c in r.c0..=r.c1 { let cur = self.sheets[sheet].col_styles.get(&c).copied().unwrap_or(0); let n = restyle(&mut self.styles, cur); self.sheets[sheet].col_styles.insert(c, n); }
                } else {
                    for row in r.r0..=r.r1 { let cur = self.sheets[sheet].row_styles.get(&row).copied().unwrap_or(0); let n = restyle(&mut self.styles, cur); self.sheets[sheet].row_styles.insert(row, n); }
                }
                let keys: Vec<Pos> = self.sheets[sheet].cells.keys().copied().filter(|p| r.contains(*p)).collect();
                for p in keys { let cur = self.sheets[sheet].cells[&p].style; let n = restyle(&mut self.styles, cur); self.sheets[sheet].cells.get_mut(&p).unwrap().style = n; }
                continue;
            }
            for p in r.iter() {
                let cur = self.sheets[sheet].style_index(p);
                let n = restyle(&mut self.styles, cur);
                let s = &mut self.sheets[sheet];
                s.cell_mut(p).style = n;
                s.prune(p);
            }
        }
    }

    /// Outer, inner or all borders over a range: `sides` picks which edges get `b`.
    pub fn apply_borders(&mut self, sheet: usize, r: Range, kind: BorderKind, b: crate::style::Border) {
        use crate::style::Border as B;
        let none = B::default();
        for p in r.iter() {
            let (top, bottom, left, right) = (p.row == r.r0, p.row == r.r1, p.col == r.c0, p.col == r.c1);
            let set = |st: &mut Style| match kind {
                BorderKind::All => { st.top = b; st.bottom = b; st.left = b; st.right = b; }
                BorderKind::None => { st.top = none; st.bottom = none; st.left = none; st.right = none; st.diag_up = false; st.diag_down = false; }
                BorderKind::Outside => { if top { st.top = b; } if bottom { st.bottom = b; } if left { st.left = b; } if right { st.right = b; } }
                BorderKind::Inside => { if !top { st.top = b; } if !bottom { st.bottom = b; } if !left { st.left = b; } if !right { st.right = b; } }
                BorderKind::InsideH => { if !top { st.top = b; } if !bottom { st.bottom = b; } }
                BorderKind::InsideV => { if !left { st.left = b; } if !right { st.right = b; } }
                BorderKind::Top => { if top { st.top = b; } }
                BorderKind::Bottom => { if bottom { st.bottom = b; } }
                BorderKind::Left => { if left { st.left = b; } }
                BorderKind::Right => { if right { st.right = b; } }
                BorderKind::ThickBox => { let t = B { style: crate::style::LineStyle::Thick, color: b.color }; if top { st.top = t; } if bottom { st.bottom = t; } if left { st.left = t; } if right { st.right = t; } }
                BorderKind::DiagUp => { st.diag_up = true; st.diagonal = b; }
                BorderKind::DiagDown => { st.diag_down = true; st.diagonal = b; }
            };
            self.apply_style(sheet, &[Range::cell(p)], &set);
        }
    }

    // ---- copy, paste, fill ----

    /// The cells of a range, for a later paste: (offset, cell).
    pub fn copy(&self, sheet: usize, r: Range) -> Clip {
        let s = &self.sheets[sheet];
        let mut cells = Vec::new();
        for (p, c) in s.cells.range(Pos::new(r.r0, 0)..=Pos::new(r.r1, MAX_COLS)) {
            if r.contains(*p) { cells.push(((p.row - r.r0, p.col - r.c0), c.clone(), self.styles.get(c.style).clone())); }
        }
        let merges = s.merges.iter().filter(|m| r.contains(m.start()) && r.contains(m.end())).map(|m| Range { r0: m.r0 - r.r0, r1: m.r1 - r.r0, c0: m.c0 - r.c0, c1: m.c1 - r.c0 }).collect();
        let values = (r.r0..=r.r1).map(|row| (r.c0..=r.c1).map(|col| s.value(Pos::new(row, col))).collect()).collect();
        let text = (r.r0..=r.r1).map(|row| (r.c0..=r.c1).map(|col| self.display(sheet, Pos::new(row, col))).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n");
        let widths = (r.c0..=r.c1).map(|c| s.col_width(c)).collect();
        Clip { rows: r.rows(), cols: r.cols(), cells, merges, values, text, widths, source: Some((sheet, r)) }
    }

    /// Pastes a clip with its top-left at `at`, repeated to fill `target` when that is a whole
    /// multiple of the clip; formulas move with their cells.
    pub fn paste(&mut self, sheet: usize, at: Range, clip: &Clip, how: Paste) -> Range {
        let (rows, cols) = if how.transpose { (clip.cols, clip.rows) } else { (clip.rows, clip.cols) };
        let reps_r = if at.rows() >= rows && at.rows() % rows == 0 { at.rows() / rows } else { 1 };
        let reps_c = if at.cols() >= cols && at.cols() % cols == 0 { at.cols() / cols } else { 1 };
        let dest = Range { r0: at.r0, c0: at.c0, r1: (at.r0 + rows * reps_r - 1).min(MAX_ROWS - 1), c1: (at.c0 + cols * reps_c - 1).min(MAX_COLS - 1) };
        if !how.skip_blanks && how.what != PasteWhat::Formats { self.clear(sheet, dest, true, how.what == PasteWhat::All, false); }
        for rr in 0..reps_r { for rc in 0..reps_c {
            let r0 = at.r0 + rr * rows; let c0 = at.c0 + rc * cols;
            for ((dr, dc), cell, style) in &clip.cells {
                let (dr, dc) = if how.transpose { (*dc, *dr) } else { (*dr, *dc) };
                let p = Pos::new(r0 + dr, c0 + dc);
                if p.row >= MAX_ROWS || p.col >= MAX_COLS { continue; }
                if how.skip_blanks && cell.is_blank() { continue; }
                if matches!(how.what, PasteWhat::All | PasteWhat::Formats | PasteWhat::NoBorders) {
                    let mut st = style.clone();
                    if how.what == PasteWhat::NoBorders { let d = Style::default(); st.left = d.left; st.right = d.right; st.top = d.top; st.bottom = d.bottom; }
                    let i = self.styles.intern(st);
                    let s = &mut self.sheets[sheet]; s.cell_mut(p).style = i; s.prune(p);
                }
                match how.what {
                    PasteWhat::Formats => {}
                    PasteWhat::Values => {
                        let v = cell.value.clone();
                        let input = if v.is_empty() { Input::Empty } else { Input::Value(v.scalar()) };
                        self.set_cell_input(sheet, p, input, None);
                    }
                    _ => {
                        let input = match (&cell.input, clip.source) {
                            (Input::Formula(f), Some(_)) if how.op.is_none() => {
                                let src_pos = clip.source.unwrap().1.start();
                                let (orow, ocol) = if how.transpose { (src_pos.row + dc, src_pos.col + dr) } else { (src_pos.row + dr, src_pos.col + dc) };
                                Input::Formula(Arc::new(Formula::new(&adjust::shift(&f.text, p.row as i64 - orow as i64, p.col as i64 - ocol as i64))))
                            }
                            (i, _) => i.clone(),
                        };
                        let input = match (how.op, &input) {
                            (Some(op), Input::Value(Value::Num(b))) => {
                                let a = self.sheets[sheet].value(p).to_num().unwrap_or(0.0);
                                Input::Value(Value::Num(match op { '+' => a + b, '-' => a - b, '*' => a * b, '/' => if *b == 0.0 { return dest } else { a / b }, _ => *b }))
                            }
                            _ => input,
                        };
                        let s = &mut self.sheets[sheet];
                        let c = s.cell_mut(p);
                        c.value = match &input { Input::Value(v) => v.clone(), _ => Value::Empty };
                        c.input = input;
                        s.prune(p);
                    }
                }
            }
            if how.what == PasteWhat::All {
                for m in &clip.merges {
                    let m = if how.transpose { Range { r0: m.c0, r1: m.c1, c0: m.r0, c1: m.r1 } } else { *m };
                    let m = Range { r0: m.r0 + r0, r1: m.r1 + r0, c0: m.c0 + c0, c1: m.c1 + c0 };
                    self.sheets[sheet].merges.retain(|x| !x.intersects(&m));
                    self.sheets[sheet].merges.push(m);
                }
            }
        }}
        if how.widths && !how.transpose { for (i, w) in clip.widths.iter().enumerate() { self.sheets[sheet].col_widths.insert(at.c0 + i as u32, *w); } }
        dest
    }

    /// Pastes plain text (tab-separated rows) as if typed.
    pub fn paste_text(&mut self, sheet: usize, at: Pos, text: &str) -> Range {
        let rows = crate::csv::parse(text, '\t');
        let mut r1 = at.row; let mut c1 = at.col;
        for (i, row) in rows.iter().enumerate() {
            for (j, t) in row.iter().enumerate() {
                let p = Pos::new(at.row + i as u32, at.col + j as u32);
                if p.row >= MAX_ROWS || p.col >= MAX_COLS { continue; }
                self.set_input(sheet, p, t);
                r1 = r1.max(p.row); c1 = c1.max(p.col);
            }
        }
        Range { r0: at.row, c0: at.col, r1, c1 }
    }

    /// Fills `target` (which contains `source` at one edge) from it, continuing number and date
    /// series, text with a trailing number, and weekday and month names; formulas move.
    pub fn fill(&mut self, sheet: usize, source: Range, target: Range, series: bool) {
        let down = target.c0 == source.c0 && target.c1 == source.c1;
        let lines: Vec<u32> = if down { (source.c0..=source.c1).collect() } else { (source.r0..=source.r1).collect() };
        let (s0, s1, t0, t1) = if down { (source.r0, source.r1, target.r0, target.r1) } else { (source.c0, source.c1, target.c0, target.c1) };
        let backward = t0 < s0;
        let len = s1 - s0 + 1;
        for line in lines {
            let at = |k: u32| if down { Pos::new(k, line) } else { Pos::new(line, k) };
            let src: Vec<Cell> = (s0..=s1).map(|k| self.sheets[sheet].cells.get(&at(k)).cloned().unwrap_or_default()).collect();
            let plan = crate::fill::plan(&src, series, self.date1904);
            let targets: Vec<u32> = if backward { (t0..s0).rev().collect() } else { (s1 + 1..=t1).collect() };
            for (n, k) in targets.into_iter().enumerate() {
                let step = n as i64 + 1;
                let idx = if backward { (len as i64 - 1 - (n as i64 % len as i64)) as usize } else { n % len as usize };
                let from = src[idx].clone();
                let p = at(k);
                let src_k = s0 + idx as u32;
                let input = match &from.input {
                    Input::Formula(f) => { let (dr, dc) = if down { (k as i64 - src_k as i64, 0) } else { (0, k as i64 - src_k as i64) }; Input::Formula(Arc::new(Formula::new(&adjust::shift(&f.text, dr, dc)))) }
                    _ => plan.value(if backward { -step } else { step }, idx).map(Input::Value).unwrap_or_else(|| from.input.clone()),
                };
                let s = &mut self.sheets[sheet];
                let c = s.cell_mut(p);
                c.style = from.style;
                c.value = match &input { Input::Value(v) => v.clone(), _ => Value::Empty };
                c.input = input;
                s.prune(p);
            }
        }
    }

    // ---- rows and columns ----

    /// Inserts `n` rows (or columns) before `at`, or deletes them when `n` is negative; formulas,
    /// merges, sizes and notes follow.
    pub fn insert(&mut self, sheet: usize, rows: bool, at: u32, n: i64) {
        let target = self.sheets[sheet].name.clone();
        let max = if rows { MAX_ROWS } else { MAX_COLS };
        let map = |v: u32| -> Option<u32> {
            if v < at { return Some(v); }
            if n > 0 { let x = v as i64 + n; return (x < max as i64).then_some(x as u32); }
            let end = at as i64 - n;
            if (v as i64) < end { None } else { Some((v as i64 + n) as u32) }
        };
        let mp = |p: Pos| -> Option<Pos> { if rows { map(p.row).map(|r| Pos::new(r, p.col)) } else { map(p.col).map(|c| Pos::new(p.row, c)) } };
        let s = &mut self.sheets[sheet];
        s.cells = std::mem::take(&mut s.cells).into_iter().filter_map(|(p, c)| mp(p).map(|q| (q, c))).collect();
        s.notes = std::mem::take(&mut s.notes).into_iter().filter_map(|(p, c)| mp(p).map(|q| (q, c))).collect();
        s.links = std::mem::take(&mut s.links).into_iter().filter_map(|(p, c)| mp(p).map(|q| (q, c))).collect();
        let remap_keys = |m: &mut BTreeMap<u32, f32>| { *m = std::mem::take(m).into_iter().filter_map(|(k, v)| map(k).map(|k| (k, v))).collect(); };
        let remap_set = |m: &mut BTreeSet<u32>| { *m = std::mem::take(m).into_iter().filter_map(map).collect(); };
        let remap_styles = |m: &mut BTreeMap<u32, u32>| { *m = std::mem::take(m).into_iter().filter_map(|(k, v)| map(k).map(|k| (k, v))).collect(); };
        if rows { remap_keys(&mut s.row_heights); remap_set(&mut s.hidden_rows); remap_styles(&mut s.row_styles); } else { remap_keys(&mut s.col_widths); remap_set(&mut s.hidden_cols); remap_styles(&mut s.col_styles); }
        // A new row or column takes the formatting of the one before it.
        if n > 0 && at > 0 {
            let prev: Vec<(Pos, u32)> = s.cells.iter().filter(|(p, c)| (if rows { p.row } else { p.col }) == at - 1 && c.style != 0).map(|(p, c)| (*p, c.style)).collect();
            for (p, st) in prev { for k in 0..n as u32 { let q = if rows { Pos::new(at + k, p.col) } else { Pos::new(p.row, at + k) }; s.cells.entry(q).or_default().style = st; } }
        }
        s.merges = s.merges.iter().filter_map(|m| {
            let (lo, hi) = if rows { (m.r0, m.r1) } else { (m.c0, m.c1) };
            let (a, b) = if n > 0 { (map(lo)?, if hi >= at { map(hi).unwrap_or(max - 1) } else { hi }) } else {
                let end = at as i64 - n;
                let f = |v: u32, is_hi: bool| -> i64 { if v < at { v as i64 } else if (v as i64) >= end { v as i64 + n } else if is_hi { at as i64 - 1 } else { at as i64 } };
                (f(lo, false).max(0) as u32, f(hi, true).max(0) as u32)
            };
            if b < a { return None; }
            let r = if rows { Range { r0: a, r1: b, ..*m } } else { Range { c0: a, c1: b, ..*m } };
            (!r.is_cell()).then_some(r)
        }).collect();
        crate::tables::on_insert(s, rows, at, n);
        s.outline.on_insert(rows, at, n);
        s.shift_rules(rows, at, n);
        self.rewrite_formulas(|own, f| adjust::insert(f, own, &target, rows, at, n));
        crate::follow::follow(self, crate::follow::Edit::Insert { sheet, rows, at, n });
    }

    /// Sorts the rows of a range by the given (column, ascending) keys; with `header` the first row stays.
    pub fn sort(&mut self, sheet: usize, r: Range, keys: &[(u32, bool)], header: bool, case_sensitive: bool) {
        let r0 = if header { r.r0 + 1 } else { r.r0 };
        if r0 > r.r1 { return; }
        let s = &self.sheets[sheet];
        let mut rows: Vec<u32> = (r0..=r.r1).collect();
        rows.sort_by(|&a, &b| {
            for &(col, asc) in keys {
                let (va, vb) = (s.value(Pos::new(a, col)), s.value(Pos::new(b, col)));
                // Blanks go last either way.
                let o = match (va.is_empty(), vb.is_empty()) {
                    (true, true) => std::cmp::Ordering::Equal, (true, false) => return std::cmp::Ordering::Greater, (false, true) => return std::cmp::Ordering::Less,
                    _ => crate::formula::eval::sort_cmp(&va, &vb, case_sensitive),
                };
                let o = if asc { o } else { o.reverse() };
                if o != std::cmp::Ordering::Equal { return o; }
            }
            std::cmp::Ordering::Equal
        });
        let mut moved: Vec<(u32, Vec<(u32, Cell)>)> = Vec::new();
        for (i, &from) in rows.iter().enumerate() {
            let cells: Vec<(u32, Cell)> = (r.c0..=r.c1).map(|c| (c, self.sheets[sheet].cells.get(&Pos::new(from, c)).cloned().unwrap_or_default())).collect();
            moved.push((r0 + i as u32, cells.into_iter().map(|(c, mut cell)| {
                if let Input::Formula(f) = &cell.input { cell.input = Input::Formula(Arc::new(Formula::new(&adjust::shift(&f.text, (r0 + i as u32) as i64 - from as i64, 0)))); }
                (c, cell)
            }).collect()));
        }
        let s = &mut self.sheets[sheet];
        for (row, cells) in moved { for (c, cell) in cells { let p = Pos::new(row, c); if cell == Cell::default() { s.cells.remove(&p); } else { s.cells.insert(p, cell); } } }
    }

    pub fn merge(&mut self, sheet: usize, r: Range, across: bool) {
        if r.is_cell() { return; }
        let parts: Vec<Range> = if across { (r.r0..=r.r1).map(|row| Range { r0: row, r1: row, ..r }).collect() } else { vec![r] };
        for m in parts {
            if m.is_cell() { continue; }
            // Only the top-left value stays.
            let keep = m.start();
            let s = &mut self.sheets[sheet];
            let others: Vec<Pos> = s.cells.keys().copied().filter(|p| m.contains(*p) && *p != keep).collect();
            for p in others { if let Some(c) = s.cells.get_mut(&p) { c.input = Input::Empty; c.value = Value::Empty; } s.prune(p); }
            s.merges.retain(|x| !x.intersects(&m));
            s.merges.push(m);
        }
    }
    pub fn unmerge(&mut self, sheet: usize, r: Range) { self.sheets[sheet].merges.retain(|m| !m.intersects(&r)); }

    /// The next cell (after `from`, in row order) whose text contains `query`.
    pub fn find(&self, sheet: usize, query: &str, from: Option<Pos>, opts: FindOptions) -> Option<Pos> {
        let s = &self.sheets[sheet];
        let q = if opts.match_case { query.to_string() } else { query.to_lowercase() };
        let hit = |p: &Pos| {
            let t = if opts.in_formulas { self.edit_text(sheet, *p) } else { self.display(sheet, *p) };
            let t = if opts.match_case { t } else { t.to_lowercase() };
            if opts.whole { t == q } else { !q.is_empty() && t.contains(&q) }
        };
        let after = |p: &&Pos| from.is_none_or(|f| **p > f);
        s.cells.keys().filter(after).find(|p| hit(p)).or_else(|| s.cells.keys().find(|p| hit(p))).copied()
    }

    /// Replaces `query` with `with` in every matching cell's input; returns how many changed.
    pub fn replace_all(&mut self, sheet: usize, query: &str, with: &str, opts: FindOptions) -> usize {
        if query.is_empty() { return 0; }
        let targets: Vec<(Pos, String)> = self.sheets[sheet].cells.keys().map(|p| (*p, self.edit_text(sheet, *p))).filter(|(_, t)| !t.is_empty()).collect();
        let mut n = 0;
        for (p, t) in targets {
            let new = if opts.whole { if (opts.match_case && t == query) || (!opts.match_case && t.eq_ignore_ascii_case(query)) { with.to_string() } else { continue } } else { replace_text(&t, query, with, opts.match_case) };
            if new != t { self.set_input(sheet, p, &new); n += 1; }
        }
        n
    }

    // ---- names ----
    pub fn name_index(&self, name: &str, sheet: Option<usize>) -> Option<usize> {
        let u = crate::formula::lexer::upper_name(name);
        self.names.iter().position(|n| n.sheet == sheet && crate::formula::lexer::upper_name(&n.name) == u)
            .or_else(|| sheet.and_then(|_| self.names.iter().position(|n| n.sheet.is_none() && crate::formula::lexer::upper_name(&n.name) == u)))
    }

    // ---- calculation ----

    /// Recalculates every formula.
    pub fn recalc(&mut self) { crate::formula::eval::recalc(self); self.cf_dirty(); }
}

fn replace_text(t: &str, q: &str, with: &str, case: bool) -> String {
    if case { return t.replace(q, with); }
    let lt = t.to_lowercase(); let lq = q.to_lowercase();
    if lt.len() != t.len() { return t.replace(q, with); }
    let mut out = String::new(); let mut i = 0;
    while let Some(k) = lt[i..].find(&lq) { out.push_str(&t[i..i + k]); out.push_str(with); i += k + lq.len(); }
    out.push_str(&t[i..]);
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderKind { All, None, Outside, Inside, InsideH, InsideV, Top, Bottom, Left, Right, ThickBox, DiagUp, DiagDown }

#[derive(Clone, Copy, Debug, Default)]
pub struct FindOptions { pub match_case: bool, pub whole: bool, pub in_formulas: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PasteWhat { #[default] All, Values, Formulas, Formats, NoBorders }

#[derive(Clone, Copy, Debug, Default)]
pub struct Paste { pub what: PasteWhat, pub transpose: bool, pub skip_blanks: bool, pub widths: bool, pub op: Option<char> }

/// Copied cells (Workbook::copy).
#[derive(Clone, Debug, Default)]
pub struct Clip {
    pub rows: u32,
    pub cols: u32,
    pub cells: Vec<((u32, u32), Cell, Style)>,
    pub merges: Vec<Range>,
    pub values: Vec<Vec<Value>>,
    /// Tab-separated display text, for other apps.
    pub text: String,
    pub widths: Vec<f32>,
    pub source: Option<(usize, Range)>,
}

type ExtentKey = (usize, Option<Pos>, Option<Pos>, usize);

/// A sheet's remembered extent; copies start empty and every cache compares equal.
#[derive(Default)]
pub struct ExtentCache(std::sync::Mutex<Option<(ExtentKey, Option<Pos>)>>);
impl Clone for ExtentCache { fn clone(&self) -> Self { ExtentCache::default() } }
impl PartialEq for ExtentCache { fn eq(&self, _: &Self) -> bool { true } }
impl std::fmt::Debug for ExtentCache { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("ExtentCache") } }
