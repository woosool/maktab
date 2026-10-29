//! The Formulas tab (insertfunction.js, recent-functions.js, names.js, createnames.js,
//! evaluatedialog.js, watchwindow.js, error-smarttag.js), laid out as Excel's: Function
//! Library (Insert Function and the Function Arguments wizard, AutoSum, Recently Used and the
//! category menus), Defined Names (Name Manager, Define Name, Use in Formula, Create from
//! Selection), Formula Auditing (tracer arrows, Show Formulas, Error Checking with the green
//! triangles and their smart tag, Evaluate Formula, Watch Window) and Calculation.
//!
//! The tab's state (arrows, watches, ignored errors, open dialogs) lives in egui's memory so
//! the app's own structures stay untouched; the grid calls `draw_triangles` and `grid_hook`.

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, RichText, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use sheet_engine::addr::{quote_sheet, Pos, Range};
use sheet_engine::audit::{self, Arrow, Issue, Step};
use sheet_engine::book::DefinedName;
use sheet_engine::formula::eval::evaluate_in;
use sheet_engine::Value;
use std::collections::HashSet;

// ---- state -------------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct State {
    /// Tracer arrows on the grid, and whether each came from Trace Dependents.
    pub arrows: Vec<(Arrow, bool)>,
    /// The last traced cell and how many levels each kind of trace has gone.
    prec: Option<((usize, Pos), u32)>,
    dep: Option<((usize, Pos), u32)>,
    /// Sheets showing formulas instead of values.
    pub show_formulas: HashSet<usize>,
    pub ignored: HashSet<(usize, Pos)>,
    pub watches: Vec<(usize, Pos)>,
    pub watch_open: bool,
    watch_sel: Option<usize>,
    pub recent: Vec<String>,
    recent_loaded: bool,
    pub dlg: Option<Dlg>,
    pub name_manager: bool,
    nm_filter: u8,
    nm_sel: Vec<usize>,
    nm_refers: String,
    category: String,
    last_edit: Option<(usize, Pos)>,
}

#[derive(Clone, Debug)]
pub enum Dlg {
    /// Function Arguments: the function, one text per argument, the cell and whether the call
    /// goes into a formula being edited.
    Args { name: String, args: Vec<String>, target: (usize, Pos), into_edit: bool },
    NewName { name: String, scope: Option<usize>, comment: String, refers: String, editing: Option<usize> },
    CreateNames { top: bool, left: bool, bottom: bool, right: bool },
    PasteNames { picked: Option<usize> },
    ErrorCheck { at: Option<Pos> },
    /// Evaluate Formula: a stack of (sheet, cell, steps, current step); Step In pushes.
    Evaluate { stack: Vec<(usize, Pos, Vec<Step>, usize)> },
}

fn id() -> egui::Id { egui::Id::unique("formulas-tab-state") }
pub fn state(app: &SheetApp) -> State { app.ctx.data(|d| d.get_temp::<State>(id())).unwrap_or_default() }
fn put(app: &SheetApp, s: State) { app.ctx.data_mut(|d| d.insert_temp(id(), s)); }
fn with<R>(app: &SheetApp, f: impl FnOnce(&mut State) -> R) -> R { let mut s = state(app); let r = f(&mut s); put(app, s); r }

fn recent_file() -> std::path::PathBuf { crate::config_dir().join("recent-functions.json") }

fn load_recent(s: &mut State) {
    if s.recent_loaded { return; }
    s.recent_loaded = true;
    if cfg!(test) { return; }
    if let Ok(t) = std::fs::read_to_string(recent_file()) { if let Ok(v) = serde_json::from_str::<Vec<String>>(&t) { s.recent = v; } }
}

/// Remembers a function at the top of Recently Used (ten kept, saved with the settings).
pub fn note_function(app: &SheetApp, name: &str) {
    with(app, |s| {
        load_recent(s);
        let n = name.to_uppercase();
        s.recent.retain(|x| *x != n);
        s.recent.insert(0, n);
        s.recent.truncate(10);
        if !cfg!(test) { let _ = std::fs::create_dir_all(crate::config_dir()); let _ = std::fs::write(recent_file(), serde_json::to_string(&s.recent).unwrap_or_default()); }
    });
}

fn calls_in(e: &sheet_engine::formula::parser::Expr, out: &mut Vec<String>) {
    use sheet_engine::formula::parser::Expr;
    match e {
        Expr::Call { name, args } => { if sheet_engine::formula::functions::exists(name) { out.push(name.clone()); } for a in args { calls_in(a, out); } }
        Expr::Invoke { callee, args } => { calls_in(callee, out); for a in args { calls_in(a, out); } }
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Implicit(x) => calls_in(x, out),
        Expr::Binary(_, a, b) | Expr::Span(a, b) | Expr::Intersect(a, b) => { calls_in(a, out); calls_in(b, out); }
        Expr::Union(v) => for a in v { calls_in(a, out); },
        _ => {}
    }
}

pub fn showing_formulas(app: &SheetApp) -> bool { state(app).show_formulas.contains(&app.sheet()) }

// ---- per-frame hooks ----------------------------------------------------------------------

/// Keys the tab owns (run before the grid's shortcuts): ⇧F9 Calculate Sheet, ⌘` Show
/// Formulas, ⇧F3 Insert Function. Also notes the functions of each formula entered.
pub fn keys(app: &mut SheetApp, ctx: &egui::Context) {
    // Recently Used: when an edit ends, read the functions of the formula entered.
    let editing = app.edit.as_ref().map(|e| (e.sheet, e.pos));
    let last = with(app, |s| std::mem::replace(&mut s.last_edit, editing));
    if let (Some((si, p)), None) = (last, editing) {
        if let Some(f) = app.book.sheets.get(si).and_then(|s| s.cells.get(&p)).and_then(|c| c.formula()).map(String::from) {
            if let Ok(ast) = sheet_engine::formula::parser::parse(f.trim_start_matches('=')) { let mut v = vec![]; calls_in(&ast, &mut v); for n in v.iter().rev() { note_function(app, n); } }
        }
    }
    if app.dialog.is_some() || state(app).dlg.is_some() || app.edit.is_some() { return; }
    if ctx.memory(|m| m.focused().is_some()) { return; }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F9)) { sheet_engine::whatif::recalc_all(&mut app.book); }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Backtick)) { toggle_formulas(app); }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F3)) { open_insert_function(app); }
}

/// The tab's dialogs and the Watch Window (run after the app's dialogs).
pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let mut s = state(app);
    { let mut cleared = s.clone(); cleared.dlg = None; put(app, cleared); }
    if s.name_manager && !matches!(s.dlg, Some(Dlg::NewName { .. })) { name_manager(app, ctx, &mut s); }
    if let Some(mut d) = s.dlg.take() {
        let keep = match &mut d {
            Dlg::Args { .. } => args_dialog(app, ctx, &mut d, &mut s),
            Dlg::NewName { .. } => new_name_dialog(app, ctx, &mut d),
            Dlg::CreateNames { top, left, bottom, right } => create_names_dialog(app, ctx, top, left, bottom, right),
            Dlg::PasteNames { picked } => paste_names_dialog(app, ctx, picked),
            Dlg::ErrorCheck { at } => error_check_dialog(app, ctx, at, &mut s),
            Dlg::Evaluate { stack } => evaluate_dialog(app, ctx, stack),
        };
        // A dialog may have opened another one (Show Calculation Steps, Edit…).
        if keep && s.dlg.is_none() { s.dlg = Some(d); }
    }
    if s.watch_open { watch_window(app, ctx, &mut s); }
    // Dialogs opened from inside (with()) while `s` was out are merged back.
    let inner = state(app);
    if s.dlg.is_none() { s.dlg = inner.dlg; }
    if inner.recent.len() != s.recent.len() || inner.recent != s.recent { s.recent = inner.recent; s.recent_loaded = true; }
    put(app, s);
}

// ---- the ribbon tab -----------------------------------------------------------------------

const MORE: &[&str] = &["Statistical", "Engineering", "Cube", "Information", "Compatibility", "Web"];

pub fn tab(app: &mut SheetApp, ui: &mut Ui) {
    group(ui, "Function Library", |ui| {
        if large(ui, "function", "Insert\nFunction", false, true, "Insert Function (⇧F3)").clicked() { open_insert_function(app); }
        let (s, sa) = large_split(ui, "autoSum", "AutoSum", "AutoSum (⌘⇧T)");
        if s.clicked() { app.autosum("SUM"); }
        crate::theme::menu(&sa, |ui| {
            for (f, label) in [("SUM", "Sum"), ("AVERAGE", "Average"), ("COUNT", "Count Numbers"), ("MAX", "Max"), ("MIN", "Min")] { if ui.button(tr(label)).clicked() { app.autosum(f); note_function(app, f); ui.close(); } }
            ui.separator();
            if ui.button(tr("More Functions…")).clicked() { open_insert_function(app); ui.close(); }
        });
        let r = large_menu(ui, "recentlyUsed", "Recently Used", "Recently Used functions");
        crate::theme::menu(&r, |ui| {
            let recent = with(app, |s| { load_recent(s); s.recent.clone() });
            if recent.is_empty() { ui.add_enabled(false, egui::Button::new(tr("No functions used yet"))); }
            for n in recent { if ui.button(&n).on_hover_text(crate::dialogs::function_help(&n).map(|h| h.1).unwrap_or("")).clicked() { pick_function(app, &n); ui.close(); } }
            ui.separator();
            if ui.button(tr("Insert Function…")).clicked() { open_insert_function(app); ui.close(); }
        });
        for (label, cat, icon) in [("Financial", "financial", "financial"), ("Logical", "logical", "logical"), ("Text", "text", "text"), ("Date & Time", "date", "dateTime"), ("Lookup & Reference", "lookup", "lookup"), ("Math & Trig", "math", "mathTrig")] {
            let r = large_menu(ui, icon, label, label);
            crate::theme::menu(&r, |ui| function_menu(app, ui, cat));
        }
        let r = large_menu(ui, "moreFunctions", "More Functions", "More Functions");
        crate::theme::menu(&r, |ui| {
            for cat in MORE { crate::theme::submenu(ui, tr(cat), |ui| function_menu(app, ui, cat)); }
        });
    });
    group(ui, "Defined Names", |ui| {
        if large(ui, "nameManager", "Name\nManager", false, true, "Create, edit, delete and find the names in the workbook (⌘F3)").clicked() { open_name_manager(app); }
        rows(ui, |ui| {
            let r = small_menu(ui, "defineName", "Define Name", "Name cells so formulas can use the name");
            crate::theme::menu(&r, |ui| {
                if menu_item(ui, "defineName", "Define Name…") { open_new_name(app, None); ui.close(); }
            });
            let r = small_menu(ui, "useInFormula", "Use in Formula", "Insert a name into the formula");
            crate::theme::menu(&r, |ui| {
                let names: Vec<String> = usable_names(app);
                if names.is_empty() { ui.add_enabled(false, egui::Button::new(tr("No names defined"))); }
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| for n in names { if ui.button(&n).clicked() { insert_in_formula(app, &n); ui.close(); } });
                ui.separator();
                if ui.button(tr("Paste Names…")).clicked() { with(app, |s| s.dlg = Some(Dlg::PasteNames { picked: None })); ui.close(); }
            });
            if small(ui, "createFromSelection", Some("Create from Selection"), false, true, "Make names from the labels around the selection (⌘⇧F3)").clicked() { open_create_names(app); }
        });
    });
    group(ui, "Formula Auditing", |ui| {
        rows(ui, |ui| {
            if small(ui, "tracePrecedents", Some("Trace Precedents"), false, true, "Arrows from the cells this formula uses; click again for the next level").clicked() { trace(app, false); }
            if small(ui, "traceDependents", Some("Trace Dependents"), false, true, "Arrows to the formulas that use this cell; click again for the next level").clicked() { trace(app, true); }
            let r = small_menu(ui, "removeArrows", "Remove Arrows", "Remove the tracer arrows");
            crate::theme::menu(&r, |ui| {
                if menu_item(ui, "removeArrows", "Remove Arrows") { remove_arrows(app, None); ui.close(); }
                if menu_item(ui, "tracePrecedents", "Remove Precedent Arrows") { remove_arrows(app, Some(false)); ui.close(); }
                if menu_item(ui, "traceDependents", "Remove Dependent Arrows") { remove_arrows(app, Some(true)); ui.close(); }
            });
        });
        rows(ui, |ui| {
            let on = showing_formulas(app);
            if small(ui, "showFormulas", Some("Show Formulas"), on, true, "Show the formulas in cells instead of their results (⌘`)").clicked() { toggle_formulas(app); }
            let r = small_menu(ui, "errorChecking", "Error Checking", "Check the sheet for common formula errors");
            crate::theme::menu(&r, |ui| {
                if menu_item(ui, "errorChecking", "Error Checking…") { open_error_checking(app); ui.close(); }
                if menu_item(ui, "traceErrors", "Trace Error") { trace_error(app); ui.close(); }
                crate::theme::submenu(ui, tr("Circular References"), |ui| {
                    let c = audit::circular(&app.book);
                    if c.is_empty() { ui.add_enabled(false, egui::Button::new(tr("None"))); }
                    for (s, p) in c.into_iter().take(50) { if ui.button(format!("{}!{}", quote_sheet(&app.book.sheets[s].name), crate::formulabar::absolute(Range::cell(p)))).clicked() { go_to(app, s, p); ui.close(); } }
                });
            });
            if small(ui, "evaluateFormula", Some("Evaluate Formula"), false, true, "Evaluate the formula step by step").clicked() { open_evaluate(app, app.sheet(), app.sel.active); }
        });
        let on = state(app).watch_open;
        if large(ui, "watchWindow", "Watch\nWindow", on, true, "Watch cells' values while you work elsewhere").clicked() { with(app, |s| s.watch_open = !s.watch_open); }
    });
    group(ui, "Calculation", |ui| {
        let r = large_menu(ui, "calcOptions", "Calculation Options", "Calculate automatically or only when asked");
        crate::theme::menu(&r, |ui| {
            let mode = if app.book.manual { 2 } else if app.book.calc_no_tables { 1 } else { 0 };
            for (i, label) in ["Automatic", "Automatic Except for Data Tables", "Manual"].into_iter().enumerate() {
                if crate::theme::radio(ui, mode == i, tr(label)).clicked() { set_calc_mode(app, i); ui.close(); }
            }
        });
        rows(ui, |ui| {
            if small(ui, "calculateNow", Some("Calculate Now"), false, true, "Calculate the whole workbook (F9)").clicked() { sheet_engine::whatif::recalc_all(&mut app.book); }
            if small(ui, "calcSheet", Some("Calculate Sheet"), false, true, "Calculate the active sheet (⇧F9)").clicked() { sheet_engine::whatif::recalc_all(&mut app.book); }
        });
    });
}

fn function_menu(app: &mut SheetApp, ui: &mut Ui, cat: &str) {
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        for n in crate::dialogs::functions_in(cat) {
            let tip = crate::dialogs::function_help(n).map(|h| format!("{}\n{}", h.0, h.1)).unwrap_or_default();
            if ui.button(n).on_hover_text(tip).clicked() { pick_function(app, n); ui.close(); }
        }
    });
    ui.separator();
    if ui.button(tr("Insert Function…")).clicked() { open_insert_function(app); ui.close(); }
}

pub fn set_calc_mode(app: &mut SheetApp, mode: usize) {
    app.book.manual = mode == 2;
    app.book.calc_no_tables = mode == 1;
    app.dirty = true;
    if mode != 2 { app.book.recalc(); }
}

pub fn toggle_formulas(app: &mut SheetApp) {
    let si = app.sheet();
    with(app, |s| if !s.show_formulas.remove(&si) { s.show_formulas.insert(si); });
}

fn go_to(app: &mut SheetApp, s: usize, p: Pos) {
    app.book.active = s;
    app.select(p, false);
    app.reveal = true;
}

// ---- Function Library ---------------------------------------------------------------------

pub fn open_insert_function(app: &mut SheetApp) { app.dialog = Some(crate::app::Dialog::Function { query: String::new(), picked: None }); }

/// Picking a function opens Function Arguments for it.
pub fn pick_function(app: &mut SheetApp, name: &str) {
    let into_edit = app.edit.as_ref().is_some_and(|e| e.text.starts_with('='));
    let (si, p) = match &app.edit { Some(e) => (e.sheet, e.pos), None => (app.sheet(), app.sel.active) };
    let n_params = params(name).iter().filter(|p| !p.starts_with('[') && *p != "...").count().max(1);
    let mut args = vec![String::new(); n_params];
    // Editing an existing call of this function: its arguments fill the boxes.
    if !into_edit {
        if let Some(f) = app.book.sheets[si].cells.get(&p).and_then(|c| c.formula()) {
            if let Ok(sheet_engine::formula::parser::Expr::Call { name: n, args: a }) = sheet_engine::formula::parser::parse(f.trim_start_matches('=')) {
                if n.eq_ignore_ascii_case(name) { args = a.iter().map(audit::to_text).collect(); if args.is_empty() { args.push(String::new()); } }
            }
        }
    }
    if name.eq_ignore_ascii_case("SUM") && args.iter().all(|a| a.is_empty()) && !into_edit {
        // As AutoSum guesses: the numbers above or to the left.
        let r = app.book.current_region(si, p);
        if r.r1 > r.r0 && r.contains(p) && p.row == r.r1 { args[0] = Range::new(Pos::new(r.r0, p.col), Pos::new(p.row - 1, p.col)).a1(); }
    }
    with(app, |s| s.dlg = Some(Dlg::Args { name: name.to_uppercase(), args, target: (si, p), into_edit }));
}

/// A function's argument names, from its signature ("number1, [number2], ...").
pub fn params(name: &str) -> Vec<String> {
    crate::fnhelp::SIGNATURES.binary_search_by(|x| x.0.cmp(name)).ok().map(|i| crate::fnhelp::SIGNATURES[i].1.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}

/// The Insert Function dialog: search, a category, the functions, and the selected one's help.
/// OK opens Function Arguments.
pub fn insert_function_dialog(app: &mut SheetApp, ctx: &egui::Context, query: &mut String, picked: &mut Option<&'static str>, open: &mut bool) -> bool {
    let mut keep = true;
    let mut st = state(app);
    load_recent(&mut st);
    if st.category.is_empty() { st.category = if st.recent.is_empty() { "All".into() } else { "Most Recently Used".into() }; }
    let mut go: Option<&'static str> = None;
    crate::theme::dialog(ctx, "Insert Function").open(open).resizable(false).default_width(480.0).show(ctx, |ui| {
        ui.set_min_width(460.0);
        ui.label(tr("Search for a function:"));
        crate::theme::hrow(ui, |ui| {
            let r = ui.add(egui::TextEdit::singleline(query).desired_width(360.0).hint_text(tr("Type a brief description of what you want to do")));
            if !r.has_focus() && query.is_empty() && ui.ctx().memory(|m| m.focused().is_none()) { r.request_focus(); }
            if ui.button(tr("Go")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) { st.category = "All".into(); }
        });
        crate::theme::hrow(ui, |ui| {
            ui.label(tr("Or select a category:"));
            crate::theme::Combo::from_id_salt("fn-cat").selected_text(tr(&st.category)).show_ui(ui, |ui| {
                for c in ["Most Recently Used", "All"].into_iter().chain(crate::fnhelp::CATEGORIES.iter().map(|c| c.0)) { ui.selectable_value(&mut st.category, c.to_string(), tr(c)); }
            });
        });
        let q = query.trim().to_uppercase();
        let base: Vec<&'static str> = match st.category.as_str() {
            "All" => sheet_engine::formula::functions::names(),
            "Most Recently Used" => sheet_engine::formula::functions::names().into_iter().filter(|n| st.recent.iter().any(|r| r == n)).collect::<Vec<_>>().pipe_sort(&st.recent),
            c => crate::dialogs::functions_in(c),
        };
        let list: Vec<&'static str> = base.into_iter().filter(|n| q.is_empty() || n.contains(&q) || crate::dialogs::function_help(n).is_some_and(|h| h.1.to_uppercase().contains(&q))).collect();
        ui.label(tr("Select a function:"));
        egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(220.0).min_scrolled_height(220.0).show(ui, |ui| {
                ui.set_min_width(450.0);
                for n in list { let r = ui.selectable_label(*picked == Some(n), n); if r.clicked() { *picked = Some(n); } if r.double_clicked() { go = Some(n); } }
            });
        });
        match picked.and_then(crate::dialogs::function_help) {
            Some(h) => { ui.label(RichText::new(h.0).strong()); ui.label(h.1); }
            None => { ui.label(""); }
        }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, picked.is_some(), "OK").clicked() { go = *picked; }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    put(app, st);
    if let Some(n) = go { pick_function(app, n); keep = false; }
    keep
}

trait PipeSort { fn pipe_sort(self, order: &[String]) -> Self; }
impl PipeSort for Vec<&'static str> {
    fn pipe_sort(mut self, order: &[String]) -> Self { self.sort_by_key(|n| order.iter().position(|r| r == n).unwrap_or(usize::MAX)); self }
}

fn value_text(v: &Value) -> String {
    let one = |v: &Value| match v { Value::Str(s) => format!("\"{s}\""), v => v.display() };
    match v {
        Value::Array(m) => {
            let mut parts: Vec<String> = m.data.iter().take(8).map(one).collect();
            if m.data.len() > 8 { parts.push("…".into()); }
            format!("{{{}}}", parts.join(","))
        }
        Value::Empty => String::new(),
        v => one(v),
    }
}

fn call_text(name: &str, args: &[String]) -> String {
    let mut a: Vec<&str> = args.iter().map(|s| s.trim()).collect();
    while a.last().is_some_and(|s| s.is_empty()) { a.pop(); }
    format!("{name}({})", a.join(","))
}

fn args_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut Dlg, _s: &mut State) -> bool {
    let Dlg::Args { name, args, target, into_edit } = d else { return false };
    let mut keep = true;
    let mut open = true;
    let ps = params(name);
    let variadic = ps.iter().any(|p| p == "...");
    let help = crate::dialogs::function_help(name).map(|h| h.1).unwrap_or("");
    let mut done = false;
    crate::theme::dialog(ctx, "Function Arguments").open(&mut open).resizable(false).default_width(520.0).show(ctx, |ui| {
        ui.set_min_width(500.0);
        ui.label(RichText::new(name.as_str()).strong());
        // Show the optional ones too, and one more box when the last of a list is filled.
        let fixed: Vec<String> = ps.iter().filter(|p| *p != "...").cloned().collect();
        while args.len() < fixed.len() { args.push(String::new()); }
        if variadic && args.last().is_some_and(|a| !a.trim().is_empty()) && args.len() < 255 { args.push(String::new()); }
        let mut focused_param = None;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            crate::theme::grid(ui, egui::Grid::new("fn-args").num_columns(3).spacing(vec2(8.0, 6.0)), |ui| {
                for (i, a) in args.iter_mut().enumerate() {
                    let label = fixed.get(i).cloned().unwrap_or_else(|| {
                        let last = fixed.last().cloned().unwrap_or_else(|| "value".into());
                        let stem = last.trim_matches(|c| c == '[' || c == ']').trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
                        format!("{stem}{}", i + 1)
                    });
                    let optional = label.starts_with('[');
                    let clean = label.trim_matches(|c| c == '[' || c == ']').to_string();
                    ui.label(if optional { RichText::new(&clean) } else { RichText::new(&clean).strong() });
                    let r = ui.add(egui::TextEdit::singleline(a).desired_width(220.0));
                    if r.has_focus() { focused_param = Some(clean.clone()); }
                    let v = if a.trim().is_empty() { String::new() } else { value_text(&evaluate_in(&app.book, target.0, target.1, a.trim())) };
                    ui.label(RichText::new(if v.is_empty() { String::new() } else { format!("= {}", v.chars().take(40).collect::<String>()) }).color(c_ink2()));
                    ui.end_row();
                }
            });
        });
        let result = evaluate_in(&app.book, target.0, target.1, &call_text(name, args));
        ui.separator();
        ui.label(help);
        if let Some(p) = focused_param { ui.label(RichText::new(format!("{}: {}", tr("Argument"), p)).color(c_ink2())); }
        ui.label(format!("{} = {}", tr("Formula result"), value_text(&result)));
        ui.hyperlink_to(tr("Help on this function"), format!("https://support.microsoft.com/search/results?query={}+function", name));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { done = true; }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    if done {
        let call = call_text(name, args);
        if *into_edit && app.edit.is_some() {
            crate::ribbon::insert_text(app, &call);
        } else {
            let (si, p) = *target;
            app.edit = None;
            app.checkpoint("Insert Function");
            app.book.set_input(si, p, &format!("={call}"));
            app.changed();
        }
        note_function(app, name);
        keep = false;
    }
    keep && open
}

// ---- Defined Names ------------------------------------------------------------------------

fn usable_names(app: &SheetApp) -> Vec<String> {
    let si = app.sheet();
    let mut v: Vec<String> = app.book.names.iter().filter(|n| !n.hidden && (n.sheet.is_none() || n.sheet == Some(si))).map(|n| n.name.clone()).collect();
    v.sort_by_key(|n| n.to_lowercase());
    v.dedup();
    v
}

/// Inserts text at the cursor of the formula being edited, or starts a formula with it.
pub fn insert_in_formula(app: &mut SheetApp, t: &str) {
    if app.edit.as_ref().is_some_and(|e| e.text.starts_with('=')) { crate::ribbon::insert_text(app, t); } else { app.commit(None); app.begin_edit(Some(format!("={t}")), false); }
}

fn selection_ref(app: &SheetApp) -> String { format!("={}!{}", quote_sheet(&app.book.sheets[app.sheet()].name), crate::formulabar::absolute(app.sel.range())) }

pub fn open_name_manager(app: &mut SheetApp) { let r = selection_ref(app); with(app, |s| { s.name_manager = true; s.nm_sel.clear(); s.nm_refers = r; }); }

pub fn open_new_name(app: &mut SheetApp, editing: Option<usize>) {
    let d = match editing.and_then(|i| app.book.names.get(i)) {
        Some(n) => Dlg::NewName { name: n.name.clone(), scope: n.sheet, comment: n.comment.clone(), refers: format!("={}", n.formula.trim_start_matches('=')), editing },
        None => {
            // Excel suggests the label to the left or above.
            let p = app.sel.active;
            let s = &app.book.sheets[app.sheet()];
            let guess = [p.col.checked_sub(1).map(|c| Pos::new(p.row, c)), p.row.checked_sub(1).map(|r| Pos::new(r, p.col))].into_iter().flatten().map(|q| s.value(q)).find_map(|v| if let Value::Str(t) = v { Some(audit::clean_name(&t)) } else { None }).unwrap_or_default();
            Dlg::NewName { name: guess, scope: None, comment: String::new(), refers: selection_ref(app), editing: None }
        }
    };
    with(app, |s| s.dlg = Some(d));
}

pub fn open_create_names(app: &mut SheetApp) {
    let r = app.sel.range();
    let s = &app.book.sheets[app.sheet()];
    let text = |p: Pos| matches!(s.value(p), Value::Str(_));
    let top = r.rows() > 1 && (r.c0..=r.c1).all(|c| text(Pos::new(r.r0, c)) || c == r.c0);
    let left = r.cols() > 1 && (r.r0..=r.r1).all(|rr| text(Pos::new(rr, r.c0)) || rr == r.r0);
    with(app, |s| s.dlg = Some(Dlg::CreateNames { top: top || !left, left, bottom: false, right: false }));
}

fn name_manager(app: &mut SheetApp, ctx: &egui::Context, s: &mut State) {
    let mut open = true;
    let filters = ["All Names", "Names Scoped to Worksheet", "Names Scoped to Workbook", "Names with Errors", "Names without Errors", "Defined Names"];
    let mut edit: Option<Option<usize>> = None;
    crate::theme::dialog(ctx, "Name Manager").open(&mut open).resizable(false).default_width(720.0).show(ctx, |ui| {
        ui.set_min_width(700.0);
        crate::theme::hrow(ui, |ui| {
            if ui.button(tr("New…")).clicked() { edit = Some(None); }
            if ui.add_enabled(s.nm_sel.len() == 1, egui::Button::new(tr("Edit…"))).clicked() { edit = Some(s.nm_sel.first().copied()); }
            if ui.add_enabled(!s.nm_sel.is_empty(), egui::Button::new(tr("Delete"))).clicked() {
                app.checkpoint("Delete Name");
                let mut del = s.nm_sel.clone();
                del.sort_unstable_by(|a, b| b.cmp(a));
                for i in del { if i < app.book.names.len() { app.book.names.remove(i); } }
                s.nm_sel.clear();
                app.changed();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                crate::theme::Combo::from_id_salt("nm-filter").selected_text(tr(filters[s.nm_filter as usize])).show_ui(ui, |ui| for (i, f) in filters.iter().enumerate() { ui.selectable_value(&mut s.nm_filter, i as u8, tr(f)); });
                ui.label(tr("Filter:"));
            });
        });
        let mods = ui.input(|i| i.modifiers);
        egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(240.0).min_scrolled_height(240.0).show(ui, |ui| {
                ui.set_min_width(690.0);
                crate::theme::grid(ui, egui::Grid::new("names-list").num_columns(5).striped(true).spacing(vec2(14.0, 4.0)), |ui| {
                    for h in ["Name", "Value", "Refers To", "Scope", "Comment"] { ui.label(RichText::new(tr(h)).strong()); }
                    ui.end_row();
                    for (i, n) in app.book.names.iter().enumerate() {
                        if n.hidden { continue; }
                        let v = evaluate_in(&app.book, n.sheet.unwrap_or(app.book.active), Pos::default(), &n.formula);
                        let show = match s.nm_filter { 1 => n.sheet.is_some(), 2 => n.sheet.is_none(), 3 => v.is_err(), 4 => !v.is_err(), _ => true };
                        if !show { continue; }
                        let on = s.nm_sel.contains(&i);
                        let r = ui.selectable_label(on, &n.name);
                        if r.clicked() {
                            if mods.command { if on { s.nm_sel.retain(|x| *x != i); } else { s.nm_sel.push(i); } } else { s.nm_sel = vec![i]; }
                            if s.nm_sel.len() == 1 { s.nm_refers = format!("={}", app.book.names[s.nm_sel[0]].formula.trim_start_matches('=')); }
                        }
                        if r.double_clicked() { edit = Some(Some(i)); }
                        ui.label(value_text(&v).chars().take(30).collect::<String>());
                        ui.label(format!("={}", n.formula.trim_start_matches('=')).chars().take(40).collect::<String>());
                        ui.label(n.sheet.and_then(|x| app.book.sheets.get(x)).map(|x| x.name.clone()).unwrap_or_else(|| tr("Workbook")));
                        ui.label(n.comment.chars().take(30).collect::<String>());
                        ui.end_row();
                    }
                });
            });
        });
        ui.label(tr("Refers to:"));
        crate::theme::hrow(ui, |ui| {
            let one = s.nm_sel.len() == 1;
            ui.add_enabled_ui(one, |ui| crate::refedit::field(ui, &mut s.nm_refers, "formulas_tab.rs:532", 560.0)).inner;
            if ui.add_enabled(one, egui::Button::new(tr("Use Selection"))).on_hover_text(tr("Refer to the cells selected on the sheet")).clicked() { s.nm_refers = selection_ref(app); }
            if ui.add_enabled(one, egui::Button::new("✓")).on_hover_text(tr("Apply")).clicked() {
                let i = s.nm_sel[0];
                if sheet_engine::formula::parser::parse(s.nm_refers.trim_start_matches('=')).is_err() { app.error = Some(tr("There's a problem with this formula.")); } else {
                    app.checkpoint("Edit Name");
                    app.book.names[i].formula = s.nm_refers.trim_start_matches('=').to_string();
                    app.changed();
                }
            }
        });
        crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Close").clicked() { s.name_manager = false; });
    });
    if !open { s.name_manager = false; }
    if let Some(e) = edit {
        let d = match e.and_then(|i| app.book.names.get(i)) {
            Some(n) => Dlg::NewName { name: n.name.clone(), scope: n.sheet, comment: n.comment.clone(), refers: format!("={}", n.formula.trim_start_matches('=')), editing: e },
            None => Dlg::NewName { name: String::new(), scope: None, comment: String::new(), refers: selection_ref(app), editing: None },
        };
        s.dlg = Some(d);
    }
}

fn new_name_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut Dlg) -> bool {
    let Dlg::NewName { name, scope, comment, refers, editing } = d else { return false };
    let mut keep = true;
    let mut open = true;
    let title = if editing.is_some() { "Edit Name" } else { "New Name" };
    crate::theme::dialog(ctx, title).open(&mut open).resizable(false).default_width(440.0).show(ctx, |ui| {
        ui.set_min_width(420.0);
        crate::theme::grid(ui, egui::Grid::new("new-name").num_columns(2).spacing(vec2(8.0, 8.0)), |ui| {
            ui.label(tr("Name:")); ui.add(egui::TextEdit::singleline(name).desired_width(300.0)); ui.end_row();
            ui.label(tr("Scope:"));
            let label = scope.and_then(|i| app.book.sheets.get(i)).map(|s| s.name.clone()).unwrap_or_else(|| tr("Workbook"));
            ui.add_enabled_ui(editing.is_none(), |ui| {
                crate::theme::Combo::from_id_salt("name-scope").selected_text(label).show_ui(ui, |ui| {
                    ui.selectable_value(scope, None, tr("Workbook"));
                    for (i, s) in app.book.sheets.iter().enumerate() { ui.selectable_value(scope, Some(i), &s.name); }
                });
            });
            ui.end_row();
            ui.label(tr("Comment:")); ui.add(egui::TextEdit::multiline(comment).desired_rows(3).desired_width(300.0)); ui.end_row();
            ui.label(tr("Refers to:"));
            ui.horizontal(|ui| {
                crate::refedit::field(ui, refers, "formulas_tab.rs:576", 220.0);
                if ui.button(tr("Use Selection")).on_hover_text(tr("Refer to the cells selected on the sheet")).clicked() { *refers = selection_ref(app); }
            });
            ui.end_row();
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let nm = name.trim().to_string();
                let clash = app.book.names.iter().enumerate().any(|(i, n)| Some(i) != *editing && n.sheet == *scope && n.name.eq_ignore_ascii_case(&nm));
                if let Err(e) = app.book.valid_name(&nm) { app.error = Some(tr(&e)); }
                else if clash { app.error = Some(tr("The name that you entered already exists. Enter a unique name.")); }
                else if sheet_engine::formula::parser::parse(refers.trim_start_matches('=')).is_err() { app.error = Some(tr("There's a problem with this formula.")); }
                else {
                    app.checkpoint("Define Name");
                    let def = DefinedName { name: nm, sheet: *scope, formula: refers.trim().trim_start_matches('=').to_string(), hidden: false, comment: comment.clone() };
                    match *editing { Some(i) if i < app.book.names.len() => app.book.names[i] = def, _ => app.book.names.push(def) }
                    app.changed();
                    keep = false;
                }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

fn create_names_dialog(app: &mut SheetApp, ctx: &egui::Context, top: &mut bool, left: &mut bool, bottom: &mut bool, right: &mut bool) -> bool {
    let mut keep = true;
    let mut open = true;
    crate::theme::dialog(ctx, "Create Names from Selection").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.label(tr("Create names from values in the:"));
        crate::theme::check(ui, top, tr("Top row"));
        crate::theme::check(ui, left, tr("Left column"));
        crate::theme::check(ui, bottom, tr("Bottom row"));
        crate::theme::check(ui, right, tr("Right column"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, *top || *left || *bottom || *right, "OK").clicked() {
                app.checkpoint("Create Names");
                let si = app.sheet();
                let n = audit::create_names(&mut app.book, si, app.sel.range(), *top, *left, *bottom, *right);
                app.changed();
                if n == 0 { app.error = Some(tr("No names were created: the labels aren't valid names.")); }
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

fn paste_names_dialog(app: &mut SheetApp, ctx: &egui::Context, picked: &mut Option<usize>) -> bool {
    let mut keep = true;
    let mut open = true;
    let names = usable_names(app);
    crate::theme::dialog(ctx, "Paste Name").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.label(tr("Paste name:"));
        egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(200.0).min_scrolled_height(200.0).show(ui, |ui| {
                ui.set_min_width(320.0);
                for (i, n) in names.iter().enumerate() { let r = ui.selectable_label(*picked == Some(i), n); if r.clicked() { *picked = Some(i); } if r.double_clicked() { *picked = Some(i); insert_in_formula(app, n); keep = false; } }
            });
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, picked.is_some(), "OK").clicked() { if let Some(n) = picked.and_then(|i| names.get(i)) { insert_in_formula(app, n); } keep = false; }
            if crate::theme::secondary(ui, "Paste List").on_hover_text(tr("Write every name and what it refers to on the sheet")).clicked() { paste_list(app); keep = false; }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

/// Paste List: each name and its formula, in two columns from the active cell.
pub fn paste_list(app: &mut SheetApp) {
    app.edit = None;
    let si = app.sheet();
    let p = app.sel.active;
    let mut list: Vec<(String, String)> = app.book.names.iter().filter(|n| !n.hidden && (n.sheet.is_none() || n.sheet == Some(si))).map(|n| (n.name.clone(), format!("'={}", n.formula.trim_start_matches('=')))).collect();
    list.sort_by_key(|x| x.0.to_lowercase());
    app.checkpoint("Paste Names");
    for (i, (n, f)) in list.iter().enumerate() {
        app.book.set_input(si, Pos::new(p.row + i as u32, p.col), &format!("'{n}"));
        app.book.set_input(si, Pos::new(p.row + i as u32, p.col + 1), f);
    }
    app.changed();
}

// ---- Formula Auditing: arrows -------------------------------------------------------------

pub fn trace(app: &mut SheetApp, dependents: bool) {
    let cell = (app.sheet(), app.sel.active);
    let mut s = state(app);
    let slot = if dependents { &mut s.dep } else { &mut s.prec };
    let level = match slot { Some((c, n)) if *c == cell => *n + 1, _ => 1 };
    let arrows = audit::trace(&app.book, cell.0, cell.1, level, dependents);
    if arrows.is_empty() {
        app.error = Some(tr(if dependents { "The traced cell has no dependent formulas." } else { "The active cell doesn't contain a formula that refers to other cells." }));
        return;
    }
    *slot = Some((cell, level));
    for a in arrows { if !s.arrows.iter().any(|(x, d)| *x == a && *d == dependents) { s.arrows.push((a, dependents)); } }
    put(app, s);
}

pub fn trace_error(app: &mut SheetApp) {
    let (si, p) = (app.sheet(), app.sel.active);
    if !app.book.sheets[si].value(p).is_err() { app.error = Some(tr("The active cell doesn't contain an error value.")); return; }
    let arrows = audit::trace_error(&app.book, si, p);
    with(app, |s| for a in arrows { if !s.arrows.iter().any(|(x, _)| *x == a) { s.arrows.push((a, false)); } });
}

pub fn remove_arrows(app: &mut SheetApp, which: Option<bool>) {
    with(app, |s| {
        s.arrows.retain(|(_, d)| which.is_some_and(|w| w != *d));
        if which != Some(true) { s.prec = None; }
        if which != Some(false) { s.dep = None; }
    });
}

const BLUE: Color32 = Color32::from_rgb(0x1f, 0x4e, 0xd8);
const RED: Color32 = Color32::from_rgb(0xd9, 0x2b, 0x2b);
const GREEN: Color32 = Color32::from_rgb(0x1e, 0x8e, 0x3e);

fn arrow_head(p: &egui::Painter, from: egui::Pos2, to: egui::Pos2, color: Color32, z: f32) {
    let d = (to - from).normalized();
    if !d.x.is_finite() { return; }
    let n = vec2(-d.y, d.x);
    let len = 8.0 * z.max(0.6);
    let base = to - d * len;
    p.add(egui::Shape::convex_polygon(vec![to, base + n * len * 0.45, base - n * len * 0.45], color, Stroke::NONE));
}

fn sheet_icon(p: &egui::Painter, c: egui::Pos2) {
    let r = egui::Rect::from_center_size(c, vec2(14.0, 12.0));
    p.rect_filled(r, 1.0, Color32::WHITE);
    p.rect_stroke(r, 1.0, Stroke::new(1.0, Color32::from_gray(60)), egui::StrokeKind::Inside);
    for k in 1..3 { let y = r.min.y + r.height() * k as f32 / 3.0; p.line_segment([pos2(r.min.x, y), pos2(r.max.x, y)], Stroke::new(0.7, Color32::from_gray(110))); }
    let x = r.center().x; p.line_segment([pos2(x, r.min.y), pos2(x, r.max.y)], Stroke::new(0.7, Color32::from_gray(110)));
}

fn dashed(p: &egui::Painter, a: egui::Pos2, b: egui::Pos2, stroke: Stroke) {
    let len = (b - a).length();
    if len < 1.0 { return; }
    let d = (b - a) / len;
    let mut t = 0.0;
    while t < len { let e = (t + 5.0).min(len); p.line_segment([a + d * t, a + d * e], stroke); t += 9.0; }
}

/// Green triangles in the corner of cells that break an error-checking rule (one pane).
pub fn draw_triangles(app: &SheetApp, p: &egui::Painter, lay: &crate::grid::Layout, rows: std::ops::RangeInclusive<u32>, cols: std::ops::RangeInclusive<u32>) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    if s.cells.is_empty() { return; }
    let ignored = app.ctx.data(|d| d.get_temp::<State>(id()).map(|x| x.ignored)).unwrap_or_default();
    let (r0, r1, c0, c1) = (*rows.start(), *rows.end(), *cols.start(), *cols.end());
    if (r1 - r0 + 1) as u64 * (c1 - c0 + 1) as u64 > 20_000 { return; }
    for r in r0..=r1 { for c in c0..=c1 {
        let q = Pos::new(r, c);
        if !s.cells.contains_key(&q) || ignored.contains(&(si, q)) { continue; }
        if audit::issue(&app.book, si, q).is_none() { continue; }
        let rect = lay.cell_rect(q);
        let k = 6.0 * lay.z.max(0.6);
        let (a, b, cc) = if lay.rtl { (rect.right_top(), rect.right_top() - vec2(k, 0.0), rect.right_top() + vec2(0.0, k)) } else { (rect.left_top(), rect.left_top() + vec2(k, 0.0), rect.left_top() + vec2(0.0, k)) };
        p.add(egui::Shape::convex_polygon(vec![a, b, cc], GREEN, Stroke::NONE));
    } }
}

/// Tracer arrows and the error smart tag beside the active cell.
pub fn grid_hook(app: &mut SheetApp, ui: &mut Ui, painter: &egui::Painter, lay: &crate::grid::Layout) {
    let si = app.sheet();
    let st = state(app);
    let p = painter.with_clip_rect(lay.view());
    let center = |q: Pos| lay.cell_rect(q).center();
    for (a, _) in &st.arrows {
        let color = if a.error { RED } else { BLUE };
        let stroke = Stroke::new(1.6 * lay.z.max(0.6), color);
        let (fs, fr) = a.from;
        let (ts, tq) = a.to;
        if fs == si && ts == si {
            let start = center(fr.start());
            if !fr.is_cell() { p.rect_stroke(lay.range_rect(fr), 0.0, Stroke::new(1.6, color), egui::StrokeKind::Inside); }
            let end = center(tq);
            p.circle_filled(start, 3.0 * lay.z.max(0.6), color);
            p.line_segment([start, end], stroke);
            arrow_head(&p, start, end, color, lay.z);
        } else if ts == si {
            // From another sheet: a dashed black arrow from a sheet icon.
            let end = center(tq);
            let icon = end - vec2(70.0, 34.0);
            dashed(&p, icon, end, Stroke::new(1.2, Color32::from_gray(30)));
            arrow_head(&p, icon, end, Color32::from_gray(30), lay.z);
            sheet_icon(&p, icon);
        } else if fs == si {
            let start = center(fr.start());
            let icon = start + vec2(70.0, -34.0);
            p.circle_filled(start, 3.0, Color32::from_gray(30));
            dashed(&p, start, icon, Stroke::new(1.2, Color32::from_gray(30)));
            arrow_head(&p, start, icon, Color32::from_gray(30), lay.z);
            sheet_icon(&p, icon);
        }
    }
    // The smart tag: a button beside the active cell when it has an issue.
    if app.edit.is_some() { return; }
    let q = app.sel.active;
    if st.ignored.contains(&(si, q)) { return; }
    let Some(issue) = audit::issue(&app.book, si, q) else { return };
    let rect = lay.cell_rect(q);
    let size = 18.0;
    let tag = if lay.rtl { egui::Rect::from_min_size(pos2(rect.max.x + 2.0, rect.min.y), vec2(size, size)) } else { egui::Rect::from_min_size(pos2(rect.min.x - size - 2.0, rect.min.y), vec2(size, size)) };
    if !lay.view().contains_rect(tag) { return; }
    let r = ui.interact(tag, egui::Id::unique("error-smart-tag"), egui::Sense::click()).on_hover_text(tr(audit::describe(issue)));
    p.rect_filled(tag, 3.0, if r.hovered() { c_hover() } else { c_card() });
    p.rect_stroke(tag, 3.0, Stroke::new(1.0, c_line()), egui::StrokeKind::Inside);
    let c = tag.center();
    p.add(egui::Shape::convex_polygon(vec![c + vec2(0.0, -6.0), c + vec2(6.0, 0.0), c + vec2(0.0, 6.0), c + vec2(-6.0, 0.0)], Color32::from_rgb(0xf2, 0xb6, 0x00), Stroke::NONE));
    p.text(c, egui::Align2::CENTER_CENTER, "!", egui::FontId::proportional(10.0), Color32::BLACK);
    crate::theme::menu(&r, |ui| {
        ui.label(RichText::new(tr(match issue { Issue::Error(_) => "Error in Value", Issue::NumberAsText => "Number Stored as Text", Issue::Inconsistent => "Inconsistent Formula" })).strong());
        ui.separator();
        smart_tag_items(app, ui, issue, si, q);
    });
}

fn smart_tag_items(app: &mut SheetApp, ui: &mut Ui, issue: Issue, si: usize, q: Pos) {
    match issue {
        Issue::NumberAsText => if ui.button(tr("Convert to Number")).clicked() { convert_to_number(app); ui.close(); },
        Issue::Inconsistent => if q.row > 0 && ui.button(tr("Copy Formula from Above")).clicked() {
            if let Some(f) = app.book.sheets[si].cells.get(&Pos::new(q.row - 1, q.col)).and_then(|c| c.formula()).map(String::from) {
                app.checkpoint("Copy Formula from Above");
                app.book.set_input(si, q, &format!("={}", sheet_engine::formula::adjust::shift(f.trim_start_matches('='), 1, 0)));
                app.changed();
            }
            ui.close();
        },
        Issue::Error(_) => {
            if ui.button(tr("Help on this Error")).clicked() { help_on(issue); ui.close(); }
            if ui.button(tr("Show Calculation Steps…")).clicked() { open_evaluate(app, si, q); ui.close(); }
            if ui.button(tr("Trace Error")).clicked() { trace_error(app); ui.close(); }
        }
    }
    if ui.button(tr("Ignore Error")).clicked() { with(app, |s| { s.ignored.insert((si, q)); }); ui.close(); }
    if ui.button(tr("Edit in Formula Bar")).clicked() { app.begin_edit(None, true); ui.close(); }
    ui.separator();
    if ui.button(tr("Error Checking Options…")).clicked() { app.options = Some(app.prefs.clone()); ui.close(); }
}

fn help_on(issue: Issue) {
    let q = match issue { Issue::Error(e) => format!("{} error", e.text()), Issue::NumberAsText => "number stored as text".into(), Issue::Inconsistent => "inconsistent formula".into() };
    let _ = crate::grid::open_url(&format!("https://support.microsoft.com/search/results?query={}", q.replace(' ', "+").replace('#', "%23").replace('!', "").replace('/', "%2F")));
}

/// Convert to Number: text that looks like a number in the selection becomes the number.
pub fn convert_to_number(app: &mut SheetApp) {
    let si = app.sheet();
    let cells: Vec<(Pos, String)> = app.sel.ranges().iter().flat_map(|r| {
        let s = &app.book.sheets[si];
        s.cells.iter().filter(|(q, _)| r.contains(**q)).filter_map(|(q, c)| if let sheet_engine::Input::Value(Value::Str(t)) = &c.input { t.trim().replace(',', "").parse::<f64>().ok().map(|_| (*q, t.trim().to_string())) } else { None }).collect::<Vec<_>>()
    }).collect();
    if cells.is_empty() { return; }
    app.checkpoint("Convert to Number");
    for (q, t) in cells {
        app.book.set_input(si, q, &t.replace(',', ""));
        // A text format (@) would keep it text.
        if app.book.style_at(si, q).num_fmt == "@" { app.book.apply_style(si, &[Range::cell(q)], &|s| s.num_fmt = "General".into()); app.book.set_input(si, q, &t.replace(',', "")); }
    }
    app.changed();
}

// ---- Error Checking -----------------------------------------------------------------------

pub fn open_error_checking(app: &mut SheetApp) {
    let first = next_issue(app, None, true);
    if first.is_none() { app.notify(&tr("The error check of the entire sheet is complete.")); return; }
    if let Some(p) = first { app.select(p, false); app.reveal = true; }
    with(app, |s| s.dlg = Some(Dlg::ErrorCheck { at: first }));
}

fn next_issue(app: &SheetApp, from: Option<Pos>, forward: bool) -> Option<Pos> {
    let si = app.sheet();
    let ignored = state(app).ignored;
    let list: Vec<Pos> = audit::issues(&app.book, si).into_iter().map(|x| x.0).filter(|p| !ignored.contains(&(si, *p))).collect();
    let key = |p: &Pos| (p.row, p.col);
    match (from, forward) {
        (None, _) => list.first().copied(),
        (Some(f), true) => list.iter().find(|p| key(p) > key(&f)).copied(),
        (Some(f), false) => list.iter().rev().find(|p| key(p) < key(&f)).copied(),
    }
}

fn error_check_dialog(app: &mut SheetApp, ctx: &egui::Context, at: &mut Option<Pos>, s: &mut State) -> bool {
    let mut keep = true;
    let mut open = true;
    let si = app.sheet();
    let issue = at.and_then(|p| audit::issue(&app.book, si, p));
    let mut step: Option<bool> = None;
    crate::theme::dialog(ctx, "Error Checking").open(&mut open).resizable(false).default_width(520.0).show(ctx, |ui| {
        ui.set_min_width(500.0);
        match (at.as_ref(), issue) {
            (Some(p), Some(i)) => {
                ui.label(RichText::new(format!("{} {}!{}", tr("Error in cell"), quote_sheet(&app.book.sheets[si].name), p.a1())).strong());
                ui.label(RichText::new(app.book.edit_text(si, *p)).monospace());
                ui.label(tr(audit::describe(i)));
                ui.add_space(6.0);
                crate::theme::hrow(ui, |ui| {
                    if ui.button(tr("Help on this Error")).clicked() { help_on(i); }
                    if ui.add_enabled(app.book.sheets[si].cells.get(p).is_some_and(|c| c.is_formula()), egui::Button::new(tr("Show Calculation Steps…"))).clicked() { s.dlg = Some(Dlg::Evaluate { stack: vec![(si, *p, steps_of(app, si, *p), 0)] }); keep = false; }
                    if ui.button(tr("Ignore Error")).clicked() { s.ignored.insert((si, *p)); step = Some(true); }
                    if ui.button(tr("Edit in Formula Bar")).clicked() { app.select(*p, false); app.begin_edit(None, true); keep = false; }
                    if i == Issue::NumberAsText && ui.button(tr("Convert to Number")).clicked() { app.select(*p, false); convert_to_number(app); step = Some(true); }
                });
            }
            _ => { ui.label(tr("The error check of the entire sheet is complete.")); }
        }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Next").clicked() { step = Some(true); }
            if crate::theme::secondary(ui, "Previous").clicked() { step = Some(false); }
            if crate::theme::secondary(ui, "Close").clicked() { keep = false; }
        });
    });
    if let Some(fwd) = step {
        put(app, s.clone());
        let n = next_issue(app, *at, fwd);
        if let Some(p) = n { *at = Some(p); app.select(p, false); app.reveal = true; } else if fwd { *at = None; }
    }
    keep && open
}

// ---- Evaluate Formula ---------------------------------------------------------------------

fn steps_of(app: &SheetApp, si: usize, p: Pos) -> Vec<Step> {
    let f = app.book.sheets[si].cells.get(&p).and_then(|c| c.formula()).unwrap_or("").to_string();
    audit::evaluation_steps(&app.book, si, p, &f)
}

pub fn open_evaluate(app: &mut SheetApp, si: usize, p: Pos) {
    if !app.book.sheets[si].cells.get(&p).is_some_and(|c| c.is_formula()) { app.error = Some(tr("The active cell doesn't contain a formula.")); return; }
    let steps = steps_of(app, si, p);
    with(app, |s| s.dlg = Some(Dlg::Evaluate { stack: vec![(si, p, steps, 0)] }));
}

fn underlined(text: &str, span: Option<(usize, usize)>, ink: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::monospace(13.0);
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = span.unwrap_or((chars.len(), chars.len()));
    let plain = egui::TextFormat { font_id: font.clone(), color: ink, ..Default::default() };
    let under = egui::TextFormat { font_id: font, color: ink, underline: Stroke::new(1.5, ink), background: Color32::from_rgba_unmultiplied(0x1f, 0x4e, 0xd8, 40), ..Default::default() };
    job.append(&chars[..a.min(chars.len())].iter().collect::<String>(), 0.0, plain.clone());
    job.append(&chars[a.min(chars.len())..b.min(chars.len())].iter().collect::<String>(), 0.0, under);
    job.append(&chars[b.min(chars.len())..].iter().collect::<String>(), 0.0, plain);
    job.wrap.max_width = 520.0;
    job
}

fn evaluate_dialog(app: &mut SheetApp, ctx: &egui::Context, stack: &mut Vec<(usize, Pos, Vec<Step>, usize)>) -> bool {
    let mut keep = true;
    let mut open = true;
    let mut action = 0u8;
    crate::theme::dialog(ctx, "Evaluate Formula").open(&mut open).resizable(false).default_width(580.0).show(ctx, |ui| {
        ui.set_min_width(560.0);
        crate::theme::grid(ui, egui::Grid::new("eval").num_columns(2).spacing(vec2(12.0, 6.0)), |ui| {
            ui.label(RichText::new(tr("Reference:")).strong());
            ui.label(RichText::new(tr("Evaluation:")).strong());
            ui.end_row();
            for (si, p, steps, i) in stack.iter() {
                ui.label(format!("{}!{}", quote_sheet(&app.book.sheets[*si].name), crate::formulabar::absolute(Range::cell(*p))));
                let st = &steps[(*i).min(steps.len() - 1)];
                egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(6.0).show(ui, |ui| { ui.set_min_width(440.0); ui.label(underlined(&st.text, st.span, c_ink())); });
                ui.end_row();
            }
        });
        let (_, _, steps, i) = stack.last().unwrap();
        let at_end = *i + 1 >= steps.len();
        ui.label(RichText::new(tr("To show the result of the underlined expression, click Evaluate. The most recent result appears in italics.")).size(11.5).color(c_ink2()));
        if stack.len() > 1 { ui.label(RichText::new(tr("The cell being evaluated contains a constant or a formula.")).size(11.5).color(c_ink2())); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, if at_end { "Restart" } else { "Evaluate" }).clicked() { action = 1; }
            if crate::theme::secondary(ui, "Close").clicked() { keep = false; }
            if ui.add_enabled(stack.len() > 1, egui::Button::new(tr("Step Out"))).clicked() { action = 3; }
            if ui.add_enabled(steps.get(*i).is_some_and(|s| s.step_in.is_some()), egui::Button::new(tr("Step In"))).clicked() { action = 2; }
        });
    });
    match action {
        1 => { let top = stack.last_mut().unwrap(); if top.3 + 1 >= top.2.len() { top.3 = 0; top.2 = steps_of(app, top.0, top.1); } else { top.3 += 1; } }
        2 => { let (s0, p0) = stack.last().and_then(|t| t.2[t.3].step_in).unwrap(); let st = steps_of(app, s0, p0); stack.push((s0, p0, st, 0)); }
        3 => { stack.pop(); if let Some(t) = stack.last_mut() { if t.3 + 1 < t.2.len() { t.3 += 1; } } }
        _ => {}
    }
    keep && open
}

// ---- Watch Window -------------------------------------------------------------------------

fn watch_window(app: &mut SheetApp, ctx: &egui::Context, s: &mut State) {
    let mut open = true;
    let book_name = app.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| tr("Book1"));
    egui::Window::new(tr("Watch Window")).id(egui::Id::unique("watch-window")).open(&mut open).default_size(vec2(640.0, 200.0)).resizable(true).collapsible(false).order(egui::Order::Foreground).show(ctx, |ui| {
        crate::theme::hrow(ui, |ui| {
            if ui.button(tr("Add Watch…")).clicked() {
                let si = app.sheet();
                for r in app.sel.ranges() { for q in r.iter().take(200) { if !s.watches.contains(&(si, q)) { s.watches.push((si, q)); } } }
            }
            if ui.add_enabled(s.watch_sel.is_some(), egui::Button::new(tr("Delete Watch"))).clicked() { if let Some(i) = s.watch_sel.take() { if i < s.watches.len() { s.watches.remove(i); } } }
        });
        s.watches.retain(|(si, _)| *si < app.book.sheets.len());
        egui::ScrollArea::both().show(ui, |ui| {
            crate::theme::grid(ui, egui::Grid::new("watches").num_columns(6).striped(true).spacing(vec2(14.0, 4.0)), |ui| {
                for h in ["Book", "Sheet", "Name", "Cell", "Value", "Formula"] { ui.label(RichText::new(tr(h)).strong()); }
                ui.end_row();
                let mut go = None;
                for (i, (si, q)) in s.watches.iter().enumerate() {
                    let sheet = &app.book.sheets[*si];
                    let abs = format!("{}!{}", quote_sheet(&sheet.name), crate::formulabar::absolute(Range::cell(*q)));
                    let name = app.book.names.iter().find(|n| n.formula.trim_start_matches('=').eq_ignore_ascii_case(&abs)).map(|n| n.name.clone()).unwrap_or_default();
                    let r = ui.selectable_label(s.watch_sel == Some(i), &book_name);
                    if r.clicked() { s.watch_sel = Some(i); }
                    if r.double_clicked() { go = Some((*si, *q)); }
                    ui.label(&sheet.name);
                    ui.label(name);
                    ui.label(q.a1());
                    ui.label(app.book.display(*si, *q));
                    ui.label(sheet.cells.get(q).and_then(|c| c.formula()).map(|f| format!("={}", f.trim_start_matches('='))).unwrap_or_default());
                    ui.end_row();
                }
                if let Some((si, q)) = go { go_to(app, si, q); }
            });
        });
    });
    if !open { s.watch_open = false; }
}

// ---- File ▸ Options: iterative calculation ------------------------------------------------

/// The Formulas section of File ▸ Options: iterative calculation (saved with the workbook).
pub fn options_section(app: &mut SheetApp, ui: &mut Ui) {
    crate::theme::section(ui, "Formulas");
    let (mut on, (mut n, mut d)) = (app.book.iterate.is_some(), app.book.iterate.unwrap_or((100, 0.001)));
    let mut changed = crate::theme::check(ui, &mut on, tr("Enable iterative calculation")).changed();
    ui.add_enabled_ui(on, |ui| {
        crate::theme::grid(ui, egui::Grid::new("iter-opts").num_columns(2).spacing(vec2(12.0, 6.0)), |ui| {
            ui.label(tr("Maximum Iterations:")); changed |= ui.add(egui::DragValue::new(&mut n).range(1..=32767)).changed(); ui.end_row();
            ui.label(tr("Maximum Change:")); changed |= ui.add(egui::DragValue::new(&mut d).range(0.0..=1.0).speed(0.0001).max_decimals(5)).changed(); ui.end_row();
        });
    });
    if changed {
        app.book.iterate = on.then_some((n, d));
        app.dirty = true;
        if !app.book.manual { app.book.recalc(); }
    }
}
