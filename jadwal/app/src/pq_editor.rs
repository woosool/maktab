//! The Power Query Editor (query-editor.js, queries-pane.js): the workbook's queries on the
//! left, a preview of the selected step's table with a type mark and a filter/sort drop-down on
//! each column header, Applied Steps on the right (select a step to preview the table at it,
//! rename, delete, move, or change its settings with the gear), the formula bar with the step's
//! M, and a ribbon of Home, Transform and Add Column commands. The editor works on copies of the
//! queries; Close & Load saves them to the workbook (sheet_engine::powerquery) and loads the
//! selected one; Close discards the changes.

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{vec2, Color32, RichText, Ui};
use qalam_kit::widgets::*;
use sheet_engine::powerquery::eval::{Ctx, Table, V};
use sheet_engine::powerquery::{self as pq, mlang, Agg, AggOp, CondOp, Join, LoadTo, Op, Query, Split, SplitMode, Step, Ty};

/// The rows the preview shows (as the editor's preview is limited).
const PREVIEW_ROWS: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab { #[default] Home, Transform, AddColumn }

/// A command's dialog. `edit`: the step whose settings it changes (else it adds a step).
#[derive(Clone)]
pub enum Dlg {
    Rows { op: RowsOp, n: String, edit: Option<usize> },
    Replace { column: Vec<String>, find: String, replace: String, edit: Option<usize> },
    SplitDelim { column: String, delimiter: String, mode: SplitMode, edit: Option<usize> },
    SplitPos { column: String, positions: String, edit: Option<usize> },
    Group { keys: Vec<String>, aggs: Vec<(String, AggOp, String)>, edit: Option<usize> },
    Custom { name: String, formula: String, error: Option<String>, edit: Option<usize> },
    Index { name: String, start: String, step: String, edit: Option<usize> },
    Merge { other: String, left: String, right: String, kind: Join, edit: Option<usize> },
    Append { others: Vec<String>, edit: Option<usize> },
    Pivot { column: String, values: String, agg: Option<AggOp>, edit: Option<usize> },
    MergeCols { columns: Vec<String>, separator: String, name: String, edit: Option<usize> },
    Rename { from: String, to: String },
    Choose { columns: Vec<(String, bool)>, edit: Option<usize> },
    Filter { column: String, a: (CondOp, String), b: Option<(CondOp, String)>, and: bool, edit: Option<usize> },
    Advanced { text: String, error: Option<String> },
    RenameStep { index: usize, name: String },
    RenameQuery { index: usize, name: String },
    /// A step whose settings aren't edited in a dialog: its formula instead.
    StepFormula { index: usize, text: String, error: Option<String> },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RowsOp { KeepTop, KeepBottom, RemoveTop, RemoveBottom }

/// A column's filter drop-down: its distinct values and whether each is kept.
#[derive(Clone)]
pub struct ColumnMenu { pub column: String, pub values: Vec<(V, bool)>, pub pos: egui::Pos2 }

#[derive(Clone, Default)]
pub struct Editor {
    pub queries: Vec<Query>,
    /// The name each query had in the workbook (None for queries made in the editor).
    pub saved_names: Vec<Option<String>>,
    pub sel: usize,
    /// The selected step: the stage previewed (0 = the source, k = after k steps).
    pub stage: usize,
    pub tab: Tab,
    pub cols: Vec<String>,
    pub dlg: Option<Dlg>,
    pub menu: Option<ColumnMenu>,
    pub formula: String,
    formula_key: String,
    pub error: Option<String>,
}

fn id() -> egui::Id { egui::Id::unique("jadwal-pq-editor") }
pub fn state(app: &SheetApp) -> Option<Editor> { app.ctx.data(|d| d.get_temp::<Option<Editor>>(id())).flatten() }
fn put(app: &SheetApp, e: Option<Editor>) { app.ctx.data_mut(|d| d.insert_temp(id(), e)); }
/// Changes the open editor (tests and other modules).
pub fn with<R>(app: &SheetApp, f: impl FnOnce(&mut Editor) -> R) -> Option<R> { let mut s = state(app)?; let r = f(&mut s); put(app, Some(s)); Some(r) }

/// Opens the editor on the workbook's queries, with `select` chosen (or a new query added).
pub fn open(app: &mut SheetApp, select: Option<&str>, new: Option<Query>) {
    if !pq::editable(&app.book) { app.error = Some(tr("The workbook's queries are written in a form Jadwal can't change; they're kept as they are.")); return; }
    let queries = pq::list(&app.book);
    let mut e = Editor { saved_names: queries.iter().map(|q| Some(q.name.clone())).collect(), queries, ..Default::default() };
    if let Some(mut q) = new {
        let taken = |n: &str, qs: &[Query]| qs.iter().any(|x| x.name.eq_ignore_ascii_case(n));
        if taken(&q.name, &e.queries) { let base = q.name.clone(); q.name = (2..).map(|i| format!("{base} ({i})")).find(|n| !taken(n, &e.queries)).unwrap(); }
        e.queries.push(q);
        e.saved_names.push(None);
        e.sel = e.queries.len() - 1;
    } else if let Some(s) = select {
        e.sel = e.queries.iter().position(|q| q.name.eq_ignore_ascii_case(s)).unwrap_or(0);
    }
    e.stage = e.queries.get(e.sel).map_or(0, |q| q.steps.len());
    put(app, Some(e));
}

/// The previews of the selected query, cached by the queries' formulas.
#[derive(Clone, Default)]
struct Cache { key: String, stages: Vec<Result<Table, String>> }

fn stages(app: &SheetApp, e: &Editor) -> Vec<Result<Table, String>> {
    let Some(q) = e.queries.get(e.sel) else { return vec![] };
    let key = format!("{}\u{1}{}", q.name, e.queries.iter().map(|q| format!("{}={}", q.name, q.formula())).collect::<Vec<_>>().join("\u{2}"));
    let cid = egui::Id::unique("jadwal-pq-cache");
    if let Some(c) = app.ctx.data(|d| d.get_temp::<Cache>(cid)) { if c.key == key { return c.stages; } }
    let fetch = |u: &str| crate::data_tab::fetch(u);
    let out = Ctx::new(&app.book, &e.queries, &fetch).stages(q);
    app.ctx.data_mut(|d| d.insert_temp(cid, Cache { key, stages: out.clone() }));
    out
}

fn refresh_preview(app: &SheetApp) { app.ctx.data_mut(|d| d.remove_temp::<Cache>(egui::Id::unique("jadwal-pq-cache"))); }

// ---- commands -------------------------------------------------------------------------------

impl Editor {
    fn query(&mut self) -> Option<&mut Query> { self.queries.get_mut(self.sel) }
    /// Adds a step after the previewed one (as the editor inserts) and previews it.
    pub fn add(&mut self, op: Op) {
        let stage = self.stage;
        let Some(q) = self.queries.get_mut(self.sel) else { return };
        if q.read_only() { self.error = Some(tr("This query uses M that Jadwal doesn't model; edit it in the Advanced Editor.")); return; }
        let at = stage.min(q.steps.len());
        let name = q.unique_step_name(&op.default_name());
        q.steps.insert(at, Step { name, op });
        self.stage = at + 1;
    }
    /// Replaces a step's operation (a gear's settings) or adds a new step.
    fn put_op(&mut self, edit: Option<usize>, op: Op) {
        match edit { Some(i) => if let Some(q) = self.query() { if i < q.steps.len() { q.steps[i].op = op; } }, None => self.add(op) }
    }
    fn one_col(&self) -> Option<String> { self.cols.first().cloned() }
}

/// Opens the dialog of a step's settings (the gear).
fn settings(e: &mut Editor, i: usize) {
    let Some(q) = e.queries.get(e.sel) else { return };
    let Some(s) = q.steps.get(i) else { return };
    let edit = Some(i);
    e.dlg = Some(match &s.op {
        Op::FirstN(n) => Dlg::Rows { op: RowsOp::KeepTop, n: n.to_string(), edit },
        Op::LastN(n) => Dlg::Rows { op: RowsOp::KeepBottom, n: n.to_string(), edit },
        Op::Skip(n) => Dlg::Rows { op: RowsOp::RemoveTop, n: n.to_string(), edit },
        Op::RemoveLastN(n) => Dlg::Rows { op: RowsOp::RemoveBottom, n: n.to_string(), edit },
        Op::ReplaceValue { find, replace, columns, .. } => Dlg::Replace { column: columns.clone(), find: val_text(find), replace: val_text(replace), edit },
        Op::SplitColumn { column, by: Split::Delimiter { delimiter, mode }, .. } => Dlg::SplitDelim { column: column.clone(), delimiter: delimiter.clone(), mode: *mode, edit },
        Op::SplitColumn { column, by: Split::Positions(p), .. } => Dlg::SplitPos { column: column.clone(), positions: p.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "), edit },
        Op::Group { keys, aggs } => Dlg::Group { keys: keys.clone(), aggs: aggs.iter().map(|a| (a.name.clone(), a.op, a.column.clone())).collect(), edit },
        Op::AddColumn { name, expr, .. } => Dlg::Custom { name: name.clone(), formula: mlang::print(expr), error: None, edit },
        Op::AddIndex { name, start, step } => Dlg::Index { name: name.clone(), start: start.to_string(), step: step.to_string(), edit },
        Op::NestedJoin { keys, other, other_keys, kind, .. } => Dlg::Merge { other: other.clone(), left: keys.first().cloned().unwrap_or_default(), right: other_keys.first().cloned().unwrap_or_default(), kind: *kind, edit },
        Op::Append(o) => Dlg::Append { others: o.clone(), edit },
        Op::Pivot { column, values, agg } => Dlg::Pivot { column: column.clone(), values: values.clone(), agg: *agg, edit },
        Op::MergeColumns { columns, separator, name } => Dlg::MergeCols { columns: columns.clone(), separator: separator.clone(), name: name.clone(), edit },
        _ => Dlg::StepFormula { index: i, text: q.step_formula(Some(i)), error: None },
    });
}

fn val_text(v: &V) -> String { match v { V::Null => String::new(), V::Text(s) => s.clone(), v => v.display(Ty::Any) } }
/// Typed text as a value: a number when it reads as one, else text (empty: null).
fn text_val(s: &str, numeric: bool) -> V {
    if s.is_empty() { return V::Null; }
    if numeric { if let Ok(n) = s.trim().parse::<f64>() { return V::Num(n); } }
    V::Text(s.to_string())
}
fn is_numeric(t: Option<&Table>, col: &str) -> bool { t.and_then(|t| t.col(col).ok().map(|i| matches!(t.types[i], Ty::Int | Ty::Number | Ty::Currency | Ty::Percentage))).unwrap_or(false) }

// ---- the window -----------------------------------------------------------------------------

/// Draws the editor when it is open (each frame, after the app's dialogs).
pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut e) = state(app) else { return };
    if e.queries.is_empty() { e.sel = 0; }
    e.sel = e.sel.min(e.queries.len().saturating_sub(1));
    let all = stages(app, &e);
    if let Some(q) = e.queries.get(e.sel) { e.stage = e.stage.min(q.steps.len()); }
    let current: Option<Result<Table, String>> = all.get(e.stage).cloned().or_else(|| all.last().cloned());
    let table = current.as_ref().and_then(|r| r.as_ref().ok()).cloned();
    e.cols.retain(|c| table.as_ref().is_some_and(|t| t.columns.contains(c)));
    let mut close: Option<Option<LoadTo>> = None;
    let screen = ctx.content_rect();
    let w = (screen.width() - 40.0).clamp(600.0, 1240.0);
    let h = (screen.height() - 60.0).clamp(420.0, 760.0);
    // A fixed size from the screen: sizing panes from the space left made the window grow every frame.
    crate::theme::dialog(ctx, "Power Query Editor").default_width(w).show(ctx, |ui| {
        ui.set_width(w - 40.0);
        ui.set_max_width(w - 40.0);
        ui.set_height(h - 60.0);
        ui.set_max_height(h - 60.0);
        ribbon(app, ui, &mut e, table.as_ref(), &mut close);
        ui.separator();
        formula_bar(ui, &mut e);
        // The panes take what's left under the ribbon and formula bar (whose height changes with
        // the tab), less one footer line, so the window itself never changes size.
        let side_h = (ui.available_height() - 26.0).max(160.0);
        ui.horizontal_top(|ui| {
            ui.set_height(side_h);
            ui.set_max_height(side_h);
            ui.vertical(|ui| { ui.set_width(180.0); ui.set_min_height(side_h); queries_list(ui, &mut e); });
            ui.separator();
            let mid_w = (w - 40.0 - 180.0 - 210.0 - 40.0).max(200.0);
            ui.vertical(|ui| {
                ui.set_width(mid_w);
                ui.set_min_height(side_h);
                match &current {
                    Some(Ok(t)) => preview(ui, &mut e, t, side_h),
                    Some(Err(err)) => { ui.colored_label(Color32::from_rgb(196, 43, 28), format!("⚠ {}", tr("Expression.Error"))); ui.label(err); }
                    None => { ui.label(tr("Use Get Data or From Table/Range to make a query.")); }
                }
            });
            ui.separator();
            ui.vertical(|ui| { ui.set_width(210.0); steps_pane(ui, &mut e, &all); });
        });
        // One footer line, always there: the counts, or the last error in their place.
        ui.horizontal(|ui| {
            ui.set_height(18.0);
            if let Some(err) = e.error.clone() { ui.colored_label(Color32::from_rgb(196, 43, 28), err); }
            else if let Some(t) = &table { ui.label(RichText::new(crate::i18n::trf("{} columns, {} rows", &[&t.columns.len(), &t.rows.len()])).small()); }
        });
    });
    column_menu(ctx, &mut e, table.as_ref());
    dialogs(ctx, &mut e, table.as_ref(), &app.book);
    match close {
        Some(None) => { put(app, None); refresh_preview(app); }
        Some(Some(to)) => {
            put(app, Some(e.clone()));
            match close_and_load(app, &e, to) { Ok(()) => { put(app, None); refresh_preview(app); } Err(err) => { e.error = Some(err); put(app, Some(e)); } }
        }
        None => put(app, Some(e)),
    }
}

/// Close & Load: saves every query and loads the selected one as asked (and refreshes the other
/// loaded queries).
fn close_and_load(app: &mut SheetApp, e: &Editor, to: LoadTo) -> Result<(), String> {
    app.checkpoint("Close & Load");
    let before = app.book.clone();
    let r = (|| {
        // Queries renamed in the editor keep their connections (matched by their old names).
        let mut ordered: Vec<Query> = vec![];
        for old in pq::list(&app.book) {
            if let Some(i) = e.saved_names.iter().position(|n| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&old.name))) { ordered.push(e.queries[i].clone()); }
        }
        for (i, q) in e.queries.iter().enumerate() { if e.saved_names[i].is_none() || !ordered.iter().any(|x| x.name == q.name) && !pq::list(&app.book).iter().any(|o| e.saved_names[i].as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&o.name))) { ordered.push(q.clone()); } }
        pq::save_all(&mut app.book, &ordered)?;
        let fetch = |u: &str| crate::data_tab::fetch(u);
        let mut shown = None;
        if let Some(q) = e.queries.get(e.sel) {
            let cur = pq::find(&app.book, &q.name).ok_or("The query is gone.")?;
            if !cur.read_only() || to == LoadTo::ConnectionOnly { shown = pq::close_and_load(&mut app.book, &cur, None, to, &fetch)?; }
        }
        for q in pq::list(&app.book) {
            if Some(&q.name) == e.queries.get(e.sel).map(|x| &x.name) || q.read_only() || pq::loaded_table(&app.book, &q).is_none() { continue; }
            pq::refresh(&mut app.book, &q.name, &fetch)?;
        }
        Ok::<_, String>(shown)
    })();
    match r {
        Ok(shown) => {
            if let Some((si, ti)) = shown { app.book.active = si; let r = app.book.sheets[si].tables[ti].range; app.sel = crate::app::Selection::at(r.start()); }
            app.changed();
            Ok(())
        }
        Err(err) => { app.book = before; Err(err) }
    }
}

fn ribbon(app: &mut SheetApp, ui: &mut Ui, e: &mut Editor, t: Option<&Table>, close: &mut Option<Option<LoadTo>>) {
    ui.horizontal(|ui| {
        for (tab, label) in [(Tab::Home, "Home"), (Tab::Transform, "Transform"), (Tab::AddColumn, "Add Column")] {
            if ui.selectable_label(e.tab == tab, tr(label)).clicked() { e.tab = tab; }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Close")).on_hover_text(tr("Close the editor without keeping changes")).clicked() { *close = Some(None); }
        });
    });
    let ro = e.queries.get(e.sel).is_none_or(|q| q.read_only());
    let has_t = t.is_some() && !ro;
    let col = e.one_col();
    ui.horizontal(|ui| {
        ui.set_min_height(GROUP_H);
        match e.tab {
            Tab::Home => {
                group(ui, "Close", |ui| {
                    let (b, m) = large_split(ui, "accept", "Close & Load", "Save the queries and load this one to a table");
                    if b.clicked() { *close = Some(Some(LoadTo::Table)); }
                    crate::theme::menu(&m, |ui| {
                        if menu_item(ui, "accept", "Close & Load") { *close = Some(Some(LoadTo::Table)); ui.close(); }
                        if menu_item(ui, "newSheet", "Close & Load to New Sheet") { *close = Some(Some(LoadTo::NewSheet)); ui.close(); }
                        if menu_item(ui, "link", "Only Create Connection") { *close = Some(Some(LoadTo::ConnectionOnly)); ui.close(); }
                    });
                });
                group(ui, "Query", |ui| {
                    if large(ui, "updateTable", "Refresh Preview", false, true, "Read the sources again").clicked() { refresh_preview(app); }
                    if large(ui, "formula", "Advanced Editor", false, !e.queries.is_empty(), "Edit the query's M formula").clicked() { if let Some(q) = e.queries.get(e.sel) { e.dlg = Some(Dlg::Advanced { text: q.formula(), error: None }); } }
                });
                group(ui, "Manage Columns", |ui| {
                    if large(ui, "columns", "Choose Columns", false, has_t, "Keep only the columns you choose").clicked() { if let Some(t) = t { e.dlg = Some(Dlg::Choose { columns: t.columns.iter().map(|c| (c.clone(), true)).collect(), edit: None }); } }
                    let r = large_menu(ui, "deleteColumn", "Remove Columns", "Remove the selected columns");
                    crate::theme::menu(&r, |ui| {
                        if menu_item(ui, "deleteColumn", "Remove Columns") { remove_columns(e, false); ui.close(); }
                        if menu_item(ui, "deleteColumn", "Remove Other Columns") { remove_columns(e, true); ui.close(); }
                    });
                });
                group(ui, "Reduce Rows", |ui| {
                    let r = large_menu(ui, "keepRow", "Keep Rows", "Keep the top or bottom rows");
                    crate::theme::menu(&r, |ui| {
                        if menu_item(ui, "insertAbove", "Keep Top Rows") { e.dlg = Some(Dlg::Rows { op: RowsOp::KeepTop, n: "5".into(), edit: None }); ui.close(); }
                        if menu_item(ui, "insertBelow", "Keep Bottom Rows") { e.dlg = Some(Dlg::Rows { op: RowsOp::KeepBottom, n: "5".into(), edit: None }); ui.close(); }
                    });
                    let r = large_menu(ui, "deleteRow", "Remove Rows", "Remove the top or bottom rows, or duplicates");
                    crate::theme::menu(&r, |ui| {
                        if menu_item(ui, "deleteRow", "Remove Top Rows") { e.dlg = Some(Dlg::Rows { op: RowsOp::RemoveTop, n: "1".into(), edit: None }); ui.close(); }
                        if menu_item(ui, "deleteRow", "Remove Bottom Rows") { e.dlg = Some(Dlg::Rows { op: RowsOp::RemoveBottom, n: "1".into(), edit: None }); ui.close(); }
                        if menu_item(ui, "removeDuplicates", "Remove Duplicates") { let c = e.cols.clone(); e.add(Op::Distinct(if c.is_empty() { None } else { Some(c) })); ui.close(); }
                    });
                });
                group(ui, "Sort", |ui| {
                    rows(ui, |ui| {
                        if small(ui, "sort", Some("Sort Ascending"), false, has_t && col.is_some(), "Sort the selected column A to Z").clicked() { if let Some(c) = &col { e.add(Op::Sort(vec![(c.clone(), false)])); } }
                        if small(ui, "sort", Some("Sort Descending"), false, has_t && col.is_some(), "Sort the selected column Z to A").clicked() { if let Some(c) = &col { e.add(Op::Sort(vec![(c.clone(), true)])); } }
                    });
                });
                transform_group(ui, e, t, has_t, col.as_deref());
                group(ui, "Combine", |ui| {
                    rows(ui, |ui| {
                        let others: Vec<String> = e.queries.iter().enumerate().filter(|(i, _)| *i != e.sel).map(|(_, q)| q.name.clone()).collect();
                        if small(ui, "combine", Some("Merge Queries"), false, has_t && !others.is_empty(), "Join another query's rows by matching columns").clicked() {
                            e.dlg = Some(Dlg::Merge { other: others[0].clone(), left: col.clone().or_else(|| t.and_then(|t| t.columns.first().cloned())).unwrap_or_default(), right: String::new(), kind: Join::LeftOuter, edit: None });
                        }
                        if small(ui, "insertBelow", Some("Append Queries"), false, has_t && !others.is_empty(), "Add another query's rows below these").clicked() { e.dlg = Some(Dlg::Append { others: vec![others[0].clone()], edit: None }); }
                    });
                });
            }
            Tab::Transform => {
                group(ui, "Table", |ui| {
                    if large(ui, "groupObjects", "Group By", false, has_t, "Group rows and summarize them").clicked() { open_group(e, t); }
                    rows(ui, |ui| {
                        if small(ui, "headerRow", Some("Use First Row as Headers"), false, has_t, "Promote the first row to column headers").clicked() { promote(app, e); }
                        if small(ui, "rotate", Some("Transpose"), false, has_t, "Turn rows into columns").clicked() { e.add(Op::Transpose); }
                        if small(ui, "headerRow", Some("Use Headers as First Row"), false, has_t, "Demote the headers to the first row").clicked() { e.add(Op::DemoteHeaders); }
                    });
                });
                transform_group(ui, e, t, has_t, col.as_deref());
                group(ui, "Any Column", |ui| {
                    rows(ui, |ui| {
                        if small(ui, "rename", Some("Rename"), false, has_t && col.is_some(), "Rename the selected column").clicked() { if let Some(c) = &col { e.dlg = Some(Dlg::Rename { from: c.clone(), to: c.clone() }); } }
                        let r = small_menu(ui, "fillDown", "Fill", "Fill empty cells from the cells above or below");
                        crate::theme::menu(&r, |ui| {
                            if menu_item(ui, "fillDown", "Down") { if !e.cols.is_empty() { let c = e.cols.clone(); e.add(Op::FillDown(c)); } ui.close(); }
                            if menu_item(ui, "fillDown", "Up") { if !e.cols.is_empty() { let c = e.cols.clone(); e.add(Op::FillUp(c)); } ui.close(); }
                        });
                        if small(ui, "pivotTable", Some("Pivot Column"), false, has_t && col.is_some(), "Make columns from the selected column's values").clicked() {
                            if let (Some(c), Some(t)) = (&col, t) { e.dlg = Some(Dlg::Pivot { column: c.clone(), values: t.columns.iter().find(|x| *x != c).cloned().unwrap_or_default(), agg: Some(AggOp::Sum), edit: None }); }
                        }
                    });
                    rows(ui, |ui| {
                        let r = small_menu(ui, "transpose", "Unpivot Columns", "Turn the selected columns into attribute-value rows");
                        crate::theme::menu(&r, |ui| {
                            if menu_item(ui, "transpose", "Unpivot Columns") { if !e.cols.is_empty() { let c = e.cols.clone(); e.add(Op::Unpivot { columns: c, other: false, attribute: "Attribute".into(), value: "Value".into() }); } ui.close(); }
                            if menu_item(ui, "transpose", "Unpivot Other Columns") { if !e.cols.is_empty() { let c = e.cols.clone(); e.add(Op::Unpivot { columns: c, other: true, attribute: "Attribute".into(), value: "Value".into() }); } ui.close(); }
                        });
                    });
                });
                group(ui, "Text Column", |ui| {
                    if large(ui, "mergeCells", "Merge Columns", false, has_t && e.cols.len() >= 2, "Join the selected columns' text into one column").clicked() { e.dlg = Some(Dlg::MergeCols { columns: e.cols.clone(), separator: String::new(), name: "Merged".into(), edit: None }); }
                });
            }
            Tab::AddColumn => {
                group(ui, "General", |ui| {
                    if large(ui, "function", "Custom Column", false, has_t, "Add a column calculated by a formula").clicked() { e.dlg = Some(Dlg::Custom { name: "Custom".into(), formula: String::new(), error: None, edit: None }); }
                    let r = large_menu(ui, "numbers", "Index Column", "Add a column numbering the rows");
                    crate::theme::menu(&r, |ui| {
                        if menu_item(ui, "numbers", "From 0") { e.add(Op::AddIndex { name: index_name(t), start: 0, step: 1 }); ui.close(); }
                        if menu_item(ui, "numbers", "From 1") { e.add(Op::AddIndex { name: index_name(t), start: 1, step: 1 }); ui.close(); }
                        if menu_item(ui, "numbers", "Custom…") { e.dlg = Some(Dlg::Index { name: index_name(t), start: "1".into(), step: "1".into(), edit: None }); ui.close(); }
                    });
                });
                group(ui, "From Text", |ui| {
                    if large(ui, "mergeCells", "Merge Columns", false, has_t && e.cols.len() >= 2, "Join the selected columns' text into a new column").clicked() { e.dlg = Some(Dlg::MergeCols { columns: e.cols.clone(), separator: String::new(), name: "Merged".into(), edit: None }); }
                });
            }
        }
    });
}

fn index_name(t: Option<&Table>) -> String {
    let cols = t.map(|t| t.columns.clone()).unwrap_or_default();
    if !cols.iter().any(|c| c == "Index") { return "Index".into(); }
    (1..).map(|i| format!("Index.{i}")).find(|n| !cols.contains(n)).unwrap()
}

/// The Transform group shared by Home and Transform: Data Type, Use First Row as Headers,
/// Replace Values, Split Column, Group By.
fn transform_group(ui: &mut Ui, e: &mut Editor, t: Option<&Table>, has_t: bool, col: Option<&str>) {
    group(ui, "Transform", |ui| {
        rows(ui, |ui| {
            let r = small_menu(ui, "numbers", "Data Type", "Change the selected columns' type");
            crate::theme::menu(&r, |ui| {
                for ty in Ty::ALL { if ui.button(tr(ty.label())).clicked() { set_type(e, &e.cols.clone(), ty); ui.close(); } }
            });
            if small(ui, "headerRow", Some("Use First Row as Headers"), false, has_t, "Promote the first row to column headers").clicked() { e.add(Op::PromoteHeaders); }
            if small(ui, "replace", Some("Replace Values"), false, has_t && col.is_some(), "Replace a value in the selected columns").clicked() { e.dlg = Some(Dlg::Replace { column: e.cols.clone(), find: String::new(), replace: String::new(), edit: None }); }
        });
        rows(ui, |ui| {
            let r = small_menu(ui, "textToColumns", "Split Column", "Split the selected column into several");
            crate::theme::menu(&r, |ui| {
                if menu_item(ui, "textToColumns", "By Delimiter") { if let Some(c) = col { e.dlg = Some(Dlg::SplitDelim { column: c.into(), delimiter: ",".into(), mode: SplitMode::Each, edit: None }); } ui.close(); }
                if menu_item(ui, "textToColumns", "By Positions") { if let Some(c) = col { e.dlg = Some(Dlg::SplitPos { column: c.into(), positions: "0, 3".into(), edit: None }); } ui.close(); }
            });
            if small(ui, "groupObjects", Some("Group By"), false, has_t, "Group rows and summarize them").clicked() { open_group(e, t); }
        });
    });
}

fn open_group(e: &mut Editor, t: Option<&Table>) {
    let key = e.one_col().or_else(|| t.and_then(|t| t.columns.first().cloned())).unwrap_or_default();
    e.dlg = Some(Dlg::Group { keys: vec![key], aggs: vec![("Count".into(), AggOp::CountRows, String::new())], edit: None });
}

fn promote(_app: &SheetApp, e: &mut Editor) { e.add(Op::PromoteHeaders); }

fn remove_columns(e: &mut Editor, others: bool) {
    if e.cols.is_empty() { e.error = Some(tr("Select one or more columns first.")); return; }
    let c = std::mem::take(&mut e.cols);
    e.add(if others { Op::SelectColumns(c) } else { Op::RemoveColumns(c) });
}

/// Data Type: changes the previous Changed Type step when it is the last one (as the editor
/// asks to), else adds one.
fn set_type(e: &mut Editor, cols: &[String], ty: Ty) {
    if cols.is_empty() { e.error = Some(tr("Select one or more columns first.")); return; }
    let stage = e.stage;
    if let Some(q) = e.queries.get_mut(e.sel) {
        if stage > 0 && stage == q.steps.len() {
            if let Op::ChangeType(list) = &mut q.steps[stage - 1].op {
                for c in cols { match list.iter_mut().find(|x| &x.0 == c) { Some(x) => x.1 = ty, None => list.push((c.clone(), ty)) } }
                return;
            }
        }
    }
    e.add(Op::ChangeType(cols.iter().map(|c| (c.clone(), ty)).collect()));
}

fn formula_bar(ui: &mut Ui, e: &mut Editor) {
    let Some(q) = e.queries.get(e.sel) else { return };
    let step = if e.stage == 0 { None } else { Some(e.stage - 1) };
    let key = format!("{}#{:?}#{}", q.name, step, q.formula());
    if key != e.formula_key { e.formula = format!("= {}", q.step_formula(step)); e.formula_key = key; }
    let editable = step.is_some() && !q.read_only();
    ui.horizontal(|ui| {
        ui.label(RichText::new("fx").italics());
        let r = ui.add_enabled(editable, egui::TextEdit::singleline(&mut e.formula).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
        if editable && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let text = e.formula.clone();
            let i = step.unwrap();
            if let Some(q) = e.queries.get_mut(e.sel) { match q.set_step_formula(i, &text) { Ok(()) => e.error = None, Err(err) => e.error = Some(format!("{}: {err}", tr("Expression.SyntaxError"))) } }
            e.formula_key.clear();
        }
    });
}

fn queries_list(ui: &mut Ui, e: &mut Editor) {
    ui.strong(crate::i18n::trf("Queries [{}]", &[&e.queries.len()]));
    egui::ScrollArea::vertical().id_salt("pq-queries").max_height((ui.max_rect().bottom() - ui.cursor().top() - 60.0).max(40.0)).show(ui, |ui| {
        for i in 0..e.queries.len() {
            let q = &e.queries[i];
            let mark = if q.read_only() { "ƒ " } else { "▦ " };
            if ui.selectable_label(e.sel == i, format!("{mark}{}", q.name)).on_hover_text(q.source.as_ref().map(|s| s.describe()).unwrap_or_default()).clicked() && e.sel != i {
                e.sel = i; e.stage = e.queries[i].steps.len(); e.cols.clear(); e.error = None;
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        let any = !e.queries.is_empty();
        if ui.add_enabled(any, egui::Button::new(tr("Rename"))).clicked() { e.dlg = Some(Dlg::RenameQuery { index: e.sel, name: e.queries[e.sel].name.clone() }); }
        if ui.add_enabled(any, egui::Button::new(tr("Duplicate"))).clicked() {
            let mut q = e.queries[e.sel].clone();
            q.load = pq::Load::ConnectionOnly; q.meta.clear();
            let base = q.name.clone();
            q.name = (2..).map(|k| format!("{base} ({k})")).find(|n| !e.queries.iter().any(|x| x.name.eq_ignore_ascii_case(n))).unwrap();
            e.queries.push(q); e.saved_names.push(None); e.sel = e.queries.len() - 1;
        }
        if ui.add_enabled(any, egui::Button::new(tr("Reference"))).on_hover_text(tr("A new query that starts from this one")).clicked() {
            let base = e.queries[e.sel].name.clone();
            let name = (2..).map(|k| format!("{base} ({k})")).find(|n| !e.queries.iter().any(|x| x.name.eq_ignore_ascii_case(n))).unwrap();
            e.queries.push(Query::new(&name, pq::Source::Query(base))); e.saved_names.push(None); e.sel = e.queries.len() - 1; e.stage = 0;
        }
        if ui.add_enabled(any, egui::Button::new(tr("Delete"))).clicked() {
            e.queries.remove(e.sel); e.saved_names.remove(e.sel);
            e.sel = e.sel.saturating_sub(1); e.stage = e.queries.get(e.sel).map_or(0, |q| q.steps.len());
        }
    });
}

fn preview(ui: &mut Ui, e: &mut Editor, t: &Table, height: f32) {
    const W: f32 = 130.0;
    egui::ScrollArea::both().id_salt("pq-preview").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("pq-grid").striped(true).spacing(vec2(4.0, 2.0)).min_col_width(W).max_col_width(W * 1.6).show(ui, |ui| {
            ui.label("");
            for (k, c) in t.columns.iter().enumerate() {
                ui.horizontal(|ui| {
                    // The type mark: a menu of types.
                    let ty = t.types[k];
                    let tr_ = ui.add(egui::Button::new(RichText::new(ty.icon().replace('\n', "")).small().monospace()).frame(false)).on_hover_text(tr(ty.label()));
                    let cols = vec![c.clone()];
                    crate::theme::menu(&tr_, |ui| { for t2 in Ty::ALL { if ui.button(tr(t2.label())).clicked() { set_type(e, &cols, t2); ui.close(); } } });
                    let selected = e.cols.contains(c);
                    let r = ui.add(egui::Button::selectable(selected, RichText::new(c).strong()));
                    if r.clicked() {
                        let add = ui.input(|i| i.modifiers.command || i.modifiers.ctrl || i.modifiers.shift);
                        if add { if selected { e.cols.retain(|x| x != c); } else { e.cols.push(c.clone()); } } else { e.cols = vec![c.clone()]; }
                    }
                    let d = ui.add(egui::Button::new("▾").frame(false)).on_hover_text(tr("Filter and sort this column"));
                    if d.clicked() {
                        let mut vals: Vec<V> = vec![];
                        for row in &t.rows { if !vals.contains(&row[k]) { vals.push(row[k].clone()); if vals.len() >= 1000 { break; } } }
                        vals.sort_by(sheet_engine::powerquery::eval::sort_cmp);
                        e.menu = Some(ColumnMenu { column: c.clone(), values: vals.into_iter().map(|v| (v, true)).collect(), pos: d.rect.left_bottom() });
                    }
                });
            }
            ui.end_row();
            for (i, row) in t.rows.iter().take(PREVIEW_ROWS).enumerate() {
                ui.label(RichText::new((i + 1).to_string()).weak());
                for (k, v) in row.iter().enumerate() {
                    let text = v.display(t.types[k]);
                    match v {
                        V::Null => { ui.label(RichText::new(text).italics().weak()); }
                        V::Err(m) => { ui.colored_label(Color32::from_rgb(196, 43, 28), text).on_hover_text(m); }
                        V::Table(_) | V::List(_) => { ui.colored_label(Color32::from_rgb(0, 102, 204), text); }
                        _ => { ui.label(text.chars().take(60).collect::<String>()); }
                    }
                }
                ui.end_row();
            }
        });
    });
}

fn steps_pane(ui: &mut Ui, e: &mut Editor, all: &[Result<Table, String>]) {
    ui.strong(tr("Query Settings"));
    let Some(q) = e.queries.get(e.sel).cloned() else { return };
    ui.label(tr("Name"));
    ui.label(RichText::new(&q.name).strong());
    ui.add_space(6.0);
    ui.strong(tr("Applied Steps"));
    if q.read_only() {
        ui.label(RichText::new(tr("This query's M uses functions Jadwal doesn't model. It's kept exactly as written and can be refreshed in Excel.")).small());
        return;
    }
    egui::ScrollArea::vertical().id_salt("pq-steps").max_height((ui.max_rect().bottom() - ui.cursor().top() - 70.0).max(40.0)).show(ui, |ui| {
        let src = q.source_steps();
        let src_label = src.join(" / ");
        if ui.selectable_label(e.stage == 0, format!("  {src_label}")).clicked() { e.stage = 0; }
        for (i, s) in q.steps.iter().enumerate() {
            let failed = matches!(all.get(i + 1), Some(Err(_)));
            ui.horizontal(|ui| {
                let label = if failed { RichText::new(format!("⚠ {}", s.name)).color(Color32::from_rgb(196, 43, 28)) } else { RichText::new(&s.name) };
                if ui.selectable_label(e.stage == i + 1, label).clicked() { e.stage = i + 1; }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").on_hover_text(tr("Delete the step")).clicked() { delete_step(e, i); }
                    if ui.small_button("⚙").on_hover_text(tr("Edit the step's settings")).clicked() { settings(e, i); }
                });
            });
        }
    });
    ui.separator();
    let i = e.stage.checked_sub(1);
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(i.is_some(), egui::Button::new(tr("Rename"))).clicked() { let k = i.unwrap(); e.dlg = Some(Dlg::RenameStep { index: k, name: q.steps[k].name.clone() }); }
        if ui.add_enabled(i.is_some(), egui::Button::new(tr("Delete"))).clicked() { delete_step(e, i.unwrap()); }
        if ui.add_enabled(i.is_some_and(|k| k > 0), egui::Button::new(tr("Move Up"))).clicked() { let k = i.unwrap(); if let Some(q) = e.query() { q.steps.swap(k, k - 1); } e.stage = k; }
        if ui.add_enabled(i.is_some_and(|k| k + 1 < q.steps.len()), egui::Button::new(tr("Move Down"))).clicked() { let k = i.unwrap(); if let Some(q) = e.query() { q.steps.swap(k, k + 1); } e.stage = k + 2; }
    });
}

fn delete_step(e: &mut Editor, i: usize) {
    if let Some(q) = e.queries.get_mut(e.sel) { if i < q.steps.len() { q.steps.remove(i); } }
    e.stage = i;
}

/// A column header's drop-down: sort, remove, text/number filters and the value list.
fn column_menu(ctx: &egui::Context, e: &mut Editor, t: Option<&Table>) {
    let Some(mut m) = e.menu.take() else { return };
    let mut keep = true;
    let numeric = is_numeric(t, &m.column);
    egui::Area::new(egui::Id::unique("pq-colmenu")).order(egui::Order::Tooltip).fixed_pos(m.pos).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(240.0);
            let c = m.column.clone();
            if ui.button(tr("Sort Ascending")).clicked() { e.add(Op::Sort(vec![(c.clone(), false)])); keep = false; }
            if ui.button(tr("Sort Descending")).clicked() { e.add(Op::Sort(vec![(c.clone(), true)])); keep = false; }
            if ui.button(tr("Remove Column")).clicked() { e.add(Op::RemoveColumns(vec![c.clone()])); keep = false; }
            if ui.button(tr("Remove Empty")).clicked() { e.add(Op::FilterRows(mlang::Expr::Binary(mlang::BOp::And, Box::new(pq::cond_expr(&c, CondOp::NotEquals, &V::Null)), Box::new(pq::cond_expr(&c, CondOp::NotEquals, &V::text("")))))); keep = false; }
            if ui.button(tr(if numeric { "Number Filters…" } else { "Text Filters…" })).clicked() {
                e.dlg = Some(Dlg::Filter { column: c.clone(), a: (if numeric { CondOp::Greater } else { CondOp::Equals }, String::new()), b: None, and: true, edit: None });
                keep = false;
            }
            ui.separator();
            let mut all = m.values.iter().all(|v| v.1);
            if crate::theme::check(ui, &mut all, tr("(Select All)")).changed() { for v in &mut m.values { v.1 = all; } }
            egui::ScrollArea::vertical().id_salt("pq-colvals").max_height(220.0).show(ui, |ui| {
                let ty = t.and_then(|t| t.col(&c).ok().map(|i| t.types[i])).unwrap_or(Ty::Any);
                for (v, on) in &mut m.values { ui.checkbox(on, v.display(ty)); }
            });
            ui.horizontal(|ui| {
                if crate::theme::primary(ui, "OK").clicked() {
                    let keep_v: Vec<V> = m.values.iter().filter(|v| v.1).map(|v| v.0.clone()).collect();
                    let drop_v: Vec<V> = m.values.iter().filter(|v| !v.1).map(|v| v.0.clone()).collect();
                    if !drop_v.is_empty() { e.add(Op::FilterRows(pq::values_expr(&c, &keep_v, &drop_v))); }
                    keep = false;
                }
                if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
            });
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { keep = false; }
    if keep { e.menu = Some(m); }
}

fn col_combo(ui: &mut Ui, salt: &str, value: &mut String, cols: &[String]) {
    crate::theme::Combo::from_id_salt(salt).selected_text(value.clone()).show_ui(ui, |ui| { for c in cols { ui.selectable_value(value, c.clone(), c); } });
}

fn modal<R>(ctx: &egui::Context, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Modal::new(egui::Id::unique(("pq-modal", title))).show(ctx, |ui| {
        ui.set_min_width(380.0);
        ui.heading(tr(title));
        ui.add_space(6.0);
        add(ui)
    }).inner
}

/// OK / Cancel; returns Some(true) for OK, Some(false) for Cancel.
fn ok_cancel(ui: &mut Ui) -> Option<bool> {
    crate::theme::footer(ui, |ui| {
        if crate::theme::primary(ui, "OK").clicked() { return Some(true); }
        if crate::theme::secondary(ui, "Cancel").clicked() { return Some(false); }
        None
    })
}

fn dialogs(ctx: &egui::Context, e: &mut Editor, t: Option<&Table>, book: &sheet_engine::Workbook) {
    let Some(mut d) = e.dlg.take() else { return };
    let cols: Vec<String> = t.map(|t| t.columns.clone()).unwrap_or_default();
    let others: Vec<String> = e.queries.iter().enumerate().filter(|(i, _)| *i != e.sel).map(|(_, q)| q.name.clone()).collect();
    let mut keep = true;
    match &mut d {
        Dlg::Rows { op, n, edit } => {
            let title = match op { RowsOp::KeepTop => "Keep Top Rows", RowsOp::KeepBottom => "Keep Bottom Rows", RowsOp::RemoveTop => "Remove Top Rows", RowsOp::RemoveBottom => "Remove Bottom Rows" };
            let r = modal(ctx, title, |ui| { ui.horizontal(|ui| { ui.label(tr("Number of rows")); ui.text_edit_singleline(n); }); ok_cancel(ui) });
            match r {
                Some(true) => match n.trim().parse::<usize>() {
                    Ok(k) => { let o = match op { RowsOp::KeepTop => Op::FirstN(k), RowsOp::KeepBottom => Op::LastN(k), RowsOp::RemoveTop => Op::Skip(k), RowsOp::RemoveBottom => Op::RemoveLastN(k) }; e.put_op(*edit, o); keep = false; }
                    Err(_) => e.error = Some(tr("Enter a whole number.")),
                },
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::Replace { column, find, replace, edit } => {
            let r = modal(ctx, "Replace Values", |ui| {
                ui.label(tr("Replace one value with another in the selected columns."));
                ui.label(tr("Value To Find")); ui.text_edit_singleline(find);
                ui.label(tr("Replace With")); ui.text_edit_singleline(replace);
                ok_cancel(ui)
            });
            match r {
                Some(true) => {
                    let numeric = column.iter().all(|c| is_numeric(t, c));
                    // Text columns replace parts of the text; others whole values.
                    e.put_op(*edit, Op::ReplaceValue { find: text_val(find, numeric), replace: text_val(replace, numeric), text: !numeric && !find.is_empty(), columns: column.clone() });
                    keep = false;
                }
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::SplitDelim { column, delimiter, mode, edit } => {
            let r = modal(ctx, "Split Column by Delimiter", |ui| {
                ui.label(tr("Select or enter delimiter"));
                ui.text_edit_singleline(delimiter);
                ui.label(tr("Split at"));
                crate::theme::radio_value(ui, mode, SplitMode::Left, tr("Left-most delimiter"));
                crate::theme::radio_value(ui, mode, SplitMode::Right, tr("Right-most delimiter"));
                crate::theme::radio_value(ui, mode, SplitMode::Each, tr("Each occurrence of the delimiter"));
                ok_cancel(ui)
            });
            match r {
                Some(true) if !delimiter.is_empty() => {
                    let n = match mode { SplitMode::Each => t.and_then(|t| t.col(column).ok().map(|i| t.rows.iter().take(PREVIEW_ROWS).map(|r| match &r[i] { V::Text(s) => s.matches(delimiter.as_str()).count() + 1, _ => 1 }).max().unwrap_or(1))).unwrap_or(2).max(1), _ => 2 };
                    e.put_op(*edit, Op::SplitColumn { column: column.clone(), by: Split::Delimiter { delimiter: delimiter.clone(), mode: *mode }, into: (1..=n).map(|k| format!("{column}.{k}")).collect() });
                    keep = false;
                }
                Some(false) => keep = false,
                _ => {}
            }
        }
        Dlg::SplitPos { column, positions, edit } => {
            let r = modal(ctx, "Split Column by Positions", |ui| { ui.label(tr("Positions")); ui.text_edit_singleline(positions); ok_cancel(ui) });
            match r {
                Some(true) => {
                    let p: Result<Vec<usize>, _> = positions.split(',').map(|x| x.trim().parse::<usize>()).collect();
                    match p { Ok(p) if !p.is_empty() => { let n = p.len(); e.put_op(*edit, Op::SplitColumn { column: column.clone(), by: Split::Positions(p), into: (1..=n).map(|k| format!("{column}.{k}")).collect() }); keep = false; } _ => e.error = Some(tr("Enter positions as numbers separated by commas.")) }
                }
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::Group { keys, aggs, edit } => {
            let r = modal(ctx, "Group By", |ui| {
                ui.label(tr("Specify the columns to group by and the desired outputs."));
                let mut drop_key = None;
                for (k, key) in keys.iter_mut().enumerate() { ui.horizontal(|ui| { col_combo(ui, &format!("pq-gk{k}"), key, &cols); if ui.small_button("✕").clicked() { drop_key = Some(k); } }); }
                if let Some(k) = drop_key { if keys.len() > 1 { keys.remove(k); } }
                if ui.button(tr("Add grouping")).clicked() { keys.push(cols.first().cloned().unwrap_or_default()); }
                ui.separator();
                crate::theme::grid(ui, egui::Grid::new("pq-aggs").num_columns(4), |ui| {
                    ui.label(tr("New column name")); ui.label(tr("Operation")); ui.label(tr("Column")); ui.label(""); ui.end_row();
                    let mut drop = None;
                    for (k, (name, op, col)) in aggs.iter_mut().enumerate() {
                        ui.text_edit_singleline(name);
                        crate::theme::Combo::from_id_salt(format!("pq-ao{k}")).selected_text(tr(op.label())).show_ui(ui, |ui| { for o in AggOp::ALL { ui.selectable_value(op, o, tr(o.label())); } });
                        if matches!(op, AggOp::CountRows) { ui.label(""); } else { col_combo(ui, &format!("pq-ac{k}"), col, &cols); }
                        if ui.small_button("✕").clicked() { drop = Some(k); }
                        ui.end_row();
                    }
                    if let Some(k) = drop { aggs.remove(k); }
                });
                if ui.button(tr("Add aggregation")).clicked() { aggs.push((format!("Count{}", aggs.len()), AggOp::Sum, cols.last().cloned().unwrap_or_default())); }
                ok_cancel(ui)
            });
            match r {
                Some(true) => { e.put_op(*edit, Op::Group { keys: keys.clone(), aggs: aggs.iter().map(|(n, o, c)| Agg { name: n.clone(), op: *o, column: if matches!(o, AggOp::CountRows) { String::new() } else { c.clone() } }).collect() }); keep = false; }
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::Custom { name, formula, error, edit } => {
            let r = modal(ctx, "Custom Column", |ui| {
                ui.label(tr("New column name")); ui.text_edit_singleline(name);
                ui.label(tr("Custom column formula"));
                ui.horizontal_top(|ui| {
                    ui.add(egui::TextEdit::multiline(formula).desired_rows(5).desired_width(320.0).font(egui::TextStyle::Monospace));
                    ui.vertical(|ui| {
                        ui.label(tr("Available columns"));
                        egui::ScrollArea::vertical().id_salt("pq-cc-cols").max_height(110.0).show(ui, |ui| { for c in &cols { if ui.small_button(c).clicked() { formula.push_str(&format!("[{}]", mlang::field(c))); } } });
                    });
                });
                if let Some(err) = error { ui.colored_label(Color32::from_rgb(196, 43, 28), err.as_str()); } else { ui.label(RichText::new(format!("✓ {}", tr("No syntax errors have been detected."))).small()); }
                ok_cancel(ui)
            });
            *error = mlang::parse(formula.trim().trim_start_matches('=')).err();
            match r {
                Some(true) => match mlang::parse(formula.trim().trim_start_matches('=')) { Ok(x) => { e.put_op(*edit, Op::AddColumn { name: name.trim().to_string(), expr: x, ty: None }); keep = false; } Err(err) => *error = Some(err) },
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::Index { name, start, step, edit } => {
            let r = modal(ctx, "Add Index Column", |ui| { ui.label(tr("Starting Index")); ui.text_edit_singleline(start); ui.label(tr("Increment")); ui.text_edit_singleline(step); ok_cancel(ui) });
            match r {
                Some(true) => match (start.trim().parse::<i64>(), step.trim().parse::<i64>()) { (Ok(s), Ok(k)) => { e.put_op(*edit, Op::AddIndex { name: name.clone(), start: s, step: k }); keep = false; } _ => e.error = Some(tr("Enter a whole number.")) },
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::Merge { other, left, right, kind, edit } => {
            let other_cols: Vec<String> = other_columns(book, e, other);
            if right.is_empty() || !other_cols.contains(right) { *right = other_cols.iter().find(|c| *c == left).or(other_cols.first()).cloned().unwrap_or_default(); }
            let r = modal(ctx, "Merge", |ui| {
                ui.label(tr("Select a table and matching columns to create a merged table."));
                ui.horizontal(|ui| { ui.label(tr("Column")); col_combo(ui, "pq-ml", left, &cols); });
                ui.horizontal(|ui| { ui.label(tr("Query")); col_combo(ui, "pq-mq", other, &others); });
                ui.horizontal(|ui| { ui.label(tr("Matching column")); col_combo(ui, "pq-mr", right, &other_cols); });
                ui.label(tr("Join Kind"));
                crate::theme::Combo::from_id_salt("pq-mk").selected_text(tr(kind.label())).show_ui(ui, |ui| { for k in Join::ALL { ui.selectable_value(kind, k, tr(k.label())); } });
                ok_cancel(ui)
            });
            match r {
                Some(true) if !right.is_empty() && !left.is_empty() => {
                    let column = { let mut n = other.clone(); while cols.contains(&n) { n.push('1'); } n };
                    e.put_op(*edit, Op::NestedJoin { keys: vec![left.clone()], other: other.clone(), other_keys: vec![right.clone()], column: column.clone(), kind: *kind });
                    // The merged table column is expanded to the other query's columns (except the key).
                    if edit.is_none() && !matches!(kind, Join::LeftAnti | Join::RightAnti) {
                        let fields: Vec<String> = other_cols.iter().filter(|c| *c != right).cloned().collect();
                        let names = fields.iter().map(|f| format!("{column}.{f}")).collect();
                        e.add(Op::ExpandTable { column, fields, names });
                    }
                    keep = false;
                }
                Some(false) => keep = false,
                _ => {}
            }
        }
        Dlg::Append { others: sel, edit } => {
            let r = modal(ctx, "Append", |ui| {
                ui.label(tr("Concatenate rows from these tables into one table."));
                for o in &others { let mut on = sel.contains(o); if ui.checkbox(&mut on, o).changed() { if on { sel.push(o.clone()); } else { sel.retain(|x| x != o); } } }
                ok_cancel(ui)
            });
            match r { Some(true) if !sel.is_empty() => { e.put_op(*edit, Op::Append(sel.clone())); keep = false; } Some(false) => keep = false, _ => {} }
        }
        Dlg::Pivot { column, values, agg, edit } => {
            let r = modal(ctx, "Pivot Column", |ui| {
                ui.label(crate::i18n::trf("Use the names in column \"{}\" to create new columns.", &[column]));
                ui.horizontal(|ui| { ui.label(tr("Values Column")); col_combo(ui, "pq-pv", values, &cols); });
                let label = agg.map_or(tr("Don't Aggregate"), |a| tr(a.label()));
                crate::theme::Combo::from_id_salt("pq-pa").selected_text(label).show_ui(ui, |ui| {
                    ui.selectable_value(agg, None, tr("Don't Aggregate"));
                    for a in AggOp::ALL { ui.selectable_value(agg, Some(a), tr(a.label())); }
                });
                ok_cancel(ui)
            });
            match r { Some(true) => { e.put_op(*edit, Op::Pivot { column: column.clone(), values: values.clone(), agg: *agg }); keep = false; } Some(false) => keep = false, None => {} }
        }
        Dlg::MergeCols { columns, separator, name, edit } => {
            let r = modal(ctx, "Merge Columns", |ui| {
                ui.label(tr("Choose how to merge the selected columns."));
                ui.horizontal(|ui| { ui.label(tr("Separator")); ui.text_edit_singleline(separator); });
                ui.horizontal(|ui| { ui.label(tr("New column name (optional)")); ui.text_edit_singleline(name); });
                ok_cancel(ui)
            });
            match r { Some(true) => { e.put_op(*edit, Op::MergeColumns { columns: columns.clone(), separator: separator.clone(), name: if name.trim().is_empty() { "Merged".into() } else { name.trim().into() } }); keep = false; } Some(false) => keep = false, None => {} }
        }
        Dlg::Rename { from, to } => {
            let r = modal(ctx, "Rename", |ui| { ui.text_edit_singleline(to); ok_cancel(ui) });
            match r {
                Some(true) if !to.trim().is_empty() => {
                    let stage = e.stage;
                    // A rename right after a Renamed Columns step joins it.
                    let joined = e.queries.get_mut(e.sel).is_some_and(|q| { if stage > 0 && stage == q.steps.len() { if let Op::RenameColumns(v) = &mut q.steps[stage - 1].op { v.push((from.clone(), to.trim().to_string())); return true; } } false });
                    if !joined { e.add(Op::RenameColumns(vec![(from.clone(), to.trim().to_string())])); }
                    e.cols = vec![to.trim().to_string()];
                    keep = false;
                }
                Some(false) => keep = false,
                _ => {}
            }
        }
        Dlg::Choose { columns, edit } => {
            let r = modal(ctx, "Choose Columns", |ui| {
                egui::ScrollArea::vertical().id_salt("pq-choose").max_height(300.0).show(ui, |ui| { for (c, on) in columns.iter_mut() { ui.checkbox(on, c.as_str()); } });
                ok_cancel(ui)
            });
            match r { Some(true) => { e.put_op(*edit, Op::SelectColumns(columns.iter().filter(|c| c.1).map(|c| c.0.clone()).collect())); keep = false; } Some(false) => keep = false, None => {} }
        }
        Dlg::Filter { column, a, b, and, edit } => {
            let numeric = is_numeric(t, column);
            let r = modal(ctx, "Filter Rows", |ui| {
                ui.label(crate::i18n::trf("Keep rows where '{}'", &[column]));
                let row = |ui: &mut Ui, salt: &str, c: &mut (CondOp, String)| {
                    ui.horizontal(|ui| {
                        crate::theme::Combo::from_id_salt(salt).selected_text(tr(c.0.label())).show_ui(ui, |ui| { for o in CondOp::ALL { ui.selectable_value(&mut c.0, o, tr(o.label())); } });
                        ui.text_edit_singleline(&mut c.1);
                    });
                };
                row(ui, "pq-f1", a);
                let mut two = b.is_some();
                ui.horizontal(|ui| {
                    crate::theme::radio_value(ui, and, true, tr("And"));
                    crate::theme::radio_value(ui, and, false, tr("Or"));
                    ui.checkbox(&mut two, tr("Second condition"));
                });
                if two && b.is_none() { *b = Some((CondOp::Equals, String::new())); }
                if !two { *b = None; }
                if let Some(bb) = b { row(ui, "pq-f2", bb); }
                ok_cancel(ui)
            });
            match r {
                Some(true) => {
                    let x = pq::conds_expr(column, (a.0, text_val(&a.1, numeric)), b.as_ref().map(|b| (b.0, text_val(&b.1, numeric))), *and);
                    e.put_op(*edit, Op::FilterRows(x));
                    keep = false;
                }
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::Advanced { text, error } => {
            let r = modal(ctx, "Advanced Editor", |ui| {
                ui.set_min_width(640.0);
                egui::ScrollArea::vertical().id_salt("pq-adv").max_height(380.0).show(ui, |ui| { ui.add(egui::TextEdit::multiline(text).code_editor().desired_rows(16).desired_width(620.0)); });
                if let Some(err) = error { ui.colored_label(Color32::from_rgb(196, 43, 28), err.as_str()); } else { ui.label(RichText::new(format!("✓ {}", tr("No syntax errors have been detected."))).small()); }
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "Done").clicked() { return Some(true); }
                    if crate::theme::secondary(ui, "Cancel").clicked() { return Some(false); }
                    None
                })
            });
            *error = mlang::parse(text).err();
            match r {
                Some(true) if error.is_none() => {
                    if let Some(q) = e.queries.get_mut(e.sel) {
                        let mut n = Query::from_formula(&q.name, text);
                        n.load = q.load.clone(); n.meta = q.meta.clone(); n.description = q.description.clone();
                        *q = n;
                        e.stage = q.steps.len();
                    }
                    keep = false;
                }
                Some(false) => keep = false,
                _ => {}
            }
        }
        Dlg::RenameStep { index, name } => {
            let r = modal(ctx, "Rename Step", |ui| { ui.text_edit_singleline(name); ok_cancel(ui) });
            match r {
                Some(true) => if let Some(q) = e.queries.get_mut(e.sel) { match q.rename_step(*index, name) { Ok(()) => keep = false, Err(err) => e.error = Some(tr(&err)) } },
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::RenameQuery { index, name } => {
            let r = modal(ctx, "Rename Query", |ui| { ui.text_edit_singleline(name); ok_cancel(ui) });
            match r {
                Some(true) => {
                    let n = name.trim().to_string();
                    let bad = n.is_empty() || n.chars().any(|c| c == '.' || c == '"' || c.is_control());
                    if bad { e.error = Some(tr("A query name can't contain periods, quotation marks, tabs or line breaks, or start or end with a space.")); }
                    else if e.queries.iter().enumerate().any(|(i, q)| i != *index && q.name.eq_ignore_ascii_case(&n)) { e.error = Some(tr("A query with that name already exists.")); }
                    else {
                        let old = e.queries[*index].name.clone();
                        // References from other queries follow the new name.
                        for q in e.queries.iter_mut() {
                            if let Some(pq::Source::Query(s)) = &mut q.source { if s.eq_ignore_ascii_case(&old) { *s = n.clone(); } }
                            for s in &mut q.steps { match &mut s.op { Op::NestedJoin { other, .. } if other.eq_ignore_ascii_case(&old) => *other = n.clone(), Op::Append(v) => for o in v.iter_mut() { if o.eq_ignore_ascii_case(&old) { *o = n.clone(); } }, _ => {} } }
                        }
                        e.queries[*index].name = n;
                        keep = false;
                    }
                }
                Some(false) => keep = false,
                None => {}
            }
        }
        Dlg::StepFormula { index, text, error } => {
            let r = modal(ctx, "Edit Step", |ui| {
                ui.add(egui::TextEdit::multiline(text).code_editor().desired_rows(4).desired_width(520.0));
                if let Some(err) = error { ui.colored_label(Color32::from_rgb(196, 43, 28), err.as_str()); }
                ok_cancel(ui)
            });
            match r {
                Some(true) => if let Some(q) = e.queries.get_mut(e.sel) { match q.set_step_formula(*index, text) { Ok(()) => keep = false, Err(err) => *error = Some(err) } },
                Some(false) => keep = false,
                None => {}
            }
        }
    }
    if keep { e.dlg = Some(d); }
}

/// The columns another query gives (for the Merge dialog).
fn other_columns(book: &sheet_engine::Workbook, e: &Editor, name: &str) -> Vec<String> {
    let fetch = |u: &str| crate::data_tab::fetch(u);
    Ctx::new(book, &e.queries, &fetch).query_table(name).map(|t| t.columns).unwrap_or_default()
}
