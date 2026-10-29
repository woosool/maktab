//! What-If Analysis, Forecast Sheet and Solver dialogs (goalseek, datatable.js,
//! scenario-manager.js, forecast-sheet.js, solver.js): Goal Seek with its status, Data Table,
//! the Scenario Manager (add, edit, delete, show, summary), Forecast Sheet (a table and a line
//! chart on a new sheet) and Solver Parameters with Add Constraint and Solver Results.

use crate::app::SheetApp;
use crate::data_tab::{open, Dlg};
use crate::data_tools::parse_ref;
use crate::i18n::{tr, trf};
use egui::vec2;
use sheet_engine::addr::{Pos, Range};
use sheet_engine::solver::{self, Constraint, Engine, Goal, Model, Rel};
use sheet_engine::whatif::{self, Scenario};

fn cell_ref(app: &SheetApp, t: &str) -> Option<Pos> { parse_ref(app, t).filter(|(s, r)| *s == app.sheet() && r.is_cell()).map(|(_, r)| r.start()) }
fn abs(p: Pos) -> String { crate::formulabar::absolute(Range::cell(p)) }

// ---- Goal Seek ------------------------------------------------------------------------------

#[derive(Clone)]
pub struct GoalSeek { pub set: String, pub to: String, pub by: String, pub status: Option<(bool, String, String)> }

pub fn open_goal_seek(app: &mut SheetApp) { let a = abs(app.sel.active); open(app, Dlg::GoalSeek(GoalSeek { set: a, to: String::new(), by: String::new(), status: None })); }

pub fn goal_seek_dialog(app: &mut SheetApp, ctx: &egui::Context, g: &mut GoalSeek) -> bool {
    let mut keep = true; let mut open_ = true;
    if let Some((found, target, current)) = g.status.clone() {
        crate::theme::dialog(ctx, "Goal Seek Status").open(&mut open_).resizable(false).show(ctx, |ui| {
            ui.label(if found { trf("Goal Seeking with Cell {} found a solution.", &[&g.set]) } else { trf("Goal Seeking with Cell {} may not have found a solution.", &[&g.set]) });
            crate::theme::grid(ui, egui::Grid::new("gs-status").num_columns(2), |ui| {
                ui.label(tr("Target value:")); ui.label(&target); ui.end_row();
                ui.label(tr("Current value:")); ui.label(&current); ui.end_row();
            });
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "OK").clicked() { keep = false; }
                if crate::theme::secondary(ui, "Cancel").clicked() { app.undo(); keep = false; }
            });
        });
        return keep && open_;
    }
    crate::theme::dialog(ctx, "Goal Seek").open(&mut open_).resizable(false).show(ctx, |ui| {
        crate::theme::grid(ui, egui::Grid::new("gs").num_columns(2).spacing(vec2(8.0, 6.0)), |ui| {
            ui.label(tr("Set cell:")); crate::refedit::field(ui, &mut g.set, "whatif_ui.rs:43", 200.0); ui.end_row();
            ui.label(tr("To value:")); ui.text_edit_singleline(&mut g.to); ui.end_row();
            ui.label(tr("By changing cell:")); crate::refedit::field(ui, &mut g.by, "whatif_ui.rs:45", 200.0); ui.end_row();
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { run_goal_seek(app, g); }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

pub fn run_goal_seek(app: &mut SheetApp, g: &mut GoalSeek) {
    let (Some(set), Some(by)) = (cell_ref(app, &g.set), cell_ref(app, &g.by)) else { app.error = Some(tr("Reference must be to a single cell.")); return };
    let Ok(to) = g.to.trim().parse::<f64>() else { app.error = Some(tr("Invalid value.")); return };
    app.checkpoint("Goal Seek");
    let s = app.sheet();
    match whatif::goal_seek(&mut app.book, s, set, to, by, 100, 0.001) {
        Ok(r) => { app.dirty = true; g.status = Some((r.found, sheet_engine::value::fmt_num(to), app.book.display(s, set))); }
        Err(e) => { app.undo(); app.error = Some(tr(&e)); }
    }
}

// ---- Data Table -------------------------------------------------------------------------------

pub fn data_table_dialog(app: &mut SheetApp, ctx: &egui::Context, row: &mut String, col: &mut String) -> bool {
    let mut keep = true; let mut open_ = true;
    crate::theme::dialog(ctx, "Data Table").open(&mut open_).resizable(false).show(ctx, |ui| {
        crate::theme::grid(ui, egui::Grid::new("dt").num_columns(2).spacing(vec2(8.0, 6.0)), |ui| {
            ui.label(tr("Row input cell:")); crate::refedit::field(ui, row, "whatif_ui.rs:72", 200.0); ui.end_row();
            ui.label(tr("Column input cell:")); crate::refedit::field(ui, col, "whatif_ui.rs:73", 200.0); ui.end_row();
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let parse = |t: &str| if t.trim().is_empty() { Ok(None) } else { cell_ref(app, t).map(Some).ok_or(()) };
                match (parse(row), parse(col)) {
                    (Ok(r), Ok(c)) => {
                        app.checkpoint("Data Table");
                        let s = app.sheet();
                        match whatif::create_table(&mut app.book, s, app.sel.range(), r, c) {
                            Ok(_) => { app.dirty = true; app.book.cf_dirty(); keep = false; }
                            Err(e) => { app.undo(); app.error = Some(tr(&e)); }
                        }
                    }
                    _ => app.error = Some(tr("Input cell reference is not valid.")),
                }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

// ---- Scenario Manager -------------------------------------------------------------------------

#[derive(Clone)]
pub struct ScenarioEdit { pub index: Option<usize>, pub name: String, pub cells: String, pub comment: String, pub protect: bool, pub hide: bool, pub values: Option<Vec<(Pos, String)>> }

#[derive(Clone)]
pub struct Scenarios { pub sel: Option<usize>, pub edit: Option<ScenarioEdit>, pub summary: Option<String> }

pub fn open_scenarios(app: &mut SheetApp) { open(app, Dlg::Scenarios(Scenarios { sel: None, edit: None, summary: None })); }

pub fn scenarios_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut Scenarios) -> bool {
    let mut keep = true; let mut open_ = true;
    let s = app.sheet();
    let list = whatif::scenarios(&app.book, s);
    if let Some(e) = &mut d.edit {
        let mut close = false;
        if let Some(vals) = &mut e.values {
            crate::theme::dialog(ctx, "Scenario Values").open(&mut open_).resizable(false).show(ctx, |ui| {
                ui.label(tr("Enter values for each of the changing cells."));
                crate::theme::grid(ui, egui::Grid::new("sv").num_columns(2), |ui| { for (p, v) in vals.iter_mut() { ui.label(format!("{}:", abs(*p))); ui.text_edit_singleline(v); ui.end_row(); } });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() {
                        let mut all = list.clone();
                        let sc = Scenario { name: e.name.trim().to_string(), comment: e.comment.clone(), user: String::new(), locked: e.protect, hidden: e.hide, cells: vals.clone() };
                        match e.index { Some(i) if i < all.len() => all[i] = sc, _ => all.push(sc) }
                        app.checkpoint("Scenario");
                        whatif::set_scenarios(&mut app.book, s, &all, 0);
                        app.dirty = true;
                        close = true;
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { close = true; }
                });
            });
        } else {
            let title = if e.index.is_some() { "Edit Scenario" } else { "Add Scenario" };
            crate::theme::dialog(ctx, title).open(&mut open_).resizable(false).default_width(360.0).show(ctx, |ui| {
                ui.label(tr("Scenario name:")); ui.text_edit_singleline(&mut e.name);
                ui.label(tr("Changing cells:"));
                ui.horizontal(|ui| { crate::refedit::field(ui, &mut e.cells, "whatif_ui.rs:134", 200.0); if ui.button(tr("Use Selection")).clicked() { e.cells = app.sel.ranges().iter().map(|r| crate::formulabar::absolute(*r)).collect::<Vec<_>>().join(","); } });
                ui.label(tr("Comment:")); ui.text_edit_multiline(&mut e.comment);
                crate::theme::check(ui, &mut e.protect, tr("Prevent changes"));
                crate::theme::check(ui, &mut e.hide, tr("Hide"));
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() {
                        match solver::cells_of(&e.cells) {
                            _ if e.name.trim().is_empty() => app.error = Some(tr("Enter a name for the scenario.")),
                            _ if list.iter().enumerate().any(|(i, x)| Some(i) != e.index && x.name.eq_ignore_ascii_case(e.name.trim())) => app.error = Some(tr("Scenario name already exists. Enter another name.")),
                            Some(cells) if cells.len() <= 32 => {
                                let old: Vec<(Pos, String)> = e.index.and_then(|i| list.get(i)).map(|x| x.cells.clone()).unwrap_or_default();
                                e.values = Some(cells.iter().map(|p| (*p, old.iter().find(|o| o.0 == *p).map(|o| o.1.clone()).unwrap_or_else(|| app.book.edit_text(s, *p)))).collect());
                            }
                            Some(_) => app.error = Some(tr("Too many changing cells. A scenario can change up to 32 cells.")),
                            None => app.error = Some(tr("Changing cells reference is not valid.")),
                        }
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { close = true; }
                });
            });
        }
        if close { d.edit = None; }
        return keep && open_;
    }
    if let Some(res) = &mut d.summary {
        let mut close = false;
        crate::theme::dialog(ctx, "Scenario Summary").open(&mut open_).resizable(false).show(ctx, |ui| {
            ui.label(tr("Result cells:"));
            ui.text_edit_singleline(res);
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "OK").clicked() {
                    let cells = if res.trim().is_empty() { Some(vec![]) } else { solver::cells_of(res) };
                    match cells {
                        Some(cells) => { app.checkpoint("Scenario Summary"); let at = whatif::summary(&mut app.book, s, &cells); app.book.active = at; app.sel = crate::app::Selection::default(); app.changed(); keep = false; }
                        None => app.error = Some(tr("Result cells reference is not valid.")),
                    }
                }
                if crate::theme::secondary(ui, "Cancel").clicked() { close = true; }
            });
        });
        if close { d.summary = None; }
        return keep && open_;
    }
    crate::theme::dialog(ctx, "Scenario Manager").open(&mut open_).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.set_min_width(400.0);
        ui.horizontal_top(|ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_size(vec2(220.0, 160.0));
                ui.label(tr("Scenarios:"));
                if list.is_empty() { ui.label(tr("No Scenarios defined. Choose Add to add scenarios.")); }
                for (i, sc) in list.iter().enumerate() { if ui.selectable_label(d.sel == Some(i), &sc.name).clicked() { d.sel = Some(i); } }
            });
            ui.vertical(|ui| {
                if ui.button(tr("Add…")).clicked() { d.edit = Some(ScenarioEdit { index: None, name: String::new(), cells: app.sel.ranges().iter().map(|r| crate::formulabar::absolute(*r)).collect::<Vec<_>>().join(","), comment: String::new(), protect: true, hide: false, values: None }); }
                let one = d.sel.and_then(|i| list.get(i)).cloned();
                if ui.add_enabled(one.is_some(), egui::Button::new(tr("Delete"))).clicked() {
                    let mut all = list.clone(); all.remove(d.sel.unwrap());
                    app.checkpoint("Delete Scenario"); whatif::set_scenarios(&mut app.book, s, &all, 0); app.dirty = true; d.sel = None;
                }
                if ui.add_enabled(one.is_some(), egui::Button::new(tr("Edit…"))).clicked() {
                    let sc = one.clone().unwrap();
                    d.edit = Some(ScenarioEdit { index: d.sel, name: sc.name, cells: sc.cells.iter().map(|c| abs(c.0)).collect::<Vec<_>>().join(","), comment: sc.comment, protect: sc.locked, hide: sc.hidden, values: None });
                }
                if ui.add_enabled(!list.is_empty(), egui::Button::new(tr("Summary…"))).clicked() { d.summary = Some(String::new()); }
            });
        });
        if let Some(sc) = d.sel.and_then(|i| list.get(i)) {
            ui.label(format!("{} {}", tr("Changing cells:"), sc.cells.iter().map(|c| abs(c.0)).collect::<Vec<_>>().join(",")));
            if !sc.comment.is_empty() { ui.label(format!("{} {}", tr("Comment:"), sc.comment)); }
        }
        crate::theme::footer(ui, |ui| {
            let one = d.sel.and_then(|i| list.get(i)).cloned();
            if crate::theme::primary_enabled(ui, one.is_some(), "Show").clicked() { app.checkpoint("Show Scenario"); whatif::show_scenario(&mut app.book, s, one.as_ref().unwrap()); app.changed(); }
            if crate::theme::secondary(ui, "Close").clicked() { keep = false; }
        });
    });
    keep && open_
}

// ---- Forecast Sheet ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Forecast { pub timeline: String, pub values: String, pub periods: u32, pub confidence: f64, pub season: Option<u32>, pub stats: bool }

pub fn open_forecast(app: &mut SheetApp) {
    let s = app.sheet();
    let mut r = app.sel.range();
    if r.is_cell() { r = app.book.current_region(s, r.start()); }
    if r.cols() < 2 { app.error = Some(tr("Select a timeline column and a values column next to it.")); return; }
    let t = crate::data_tools::ref_text(app, s, Range { c1: r.c0, ..r });
    let v = crate::data_tools::ref_text(app, s, Range { c0: r.c0 + 1, c1: r.c0 + 1, ..r });
    open(app, Dlg::Forecast(Forecast { timeline: t, values: v, periods: (r.rows() / 3).max(3), confidence: 0.95, season: None, stats: false }));
}

pub fn forecast_dialog(app: &mut SheetApp, ctx: &egui::Context, f: &mut Forecast) -> bool {
    let mut keep = true; let mut open_ = true;
    crate::theme::dialog(ctx, "Create Forecast Worksheet").open(&mut open_).resizable(false).default_width(380.0).show(ctx, |ui| {
        crate::theme::grid(ui, egui::Grid::new("fc").num_columns(2).spacing(vec2(8.0, 6.0)), |ui| {
            ui.label(tr("Timeline Range")); crate::refedit::field(ui, &mut f.timeline, "whatif_ui.rs:232", 200.0); ui.end_row();
            ui.label(tr("Values Range")); crate::refedit::field(ui, &mut f.values, "whatif_ui.rs:233", 200.0); ui.end_row();
            ui.label(tr("Forecast periods")); ui.add(egui::DragValue::new(&mut f.periods).range(1..=1000)); ui.end_row();
            ui.label(tr("Confidence Interval")); let mut pc = f.confidence * 100.0; ui.add(egui::DragValue::new(&mut pc).range(1.0..=99.9).suffix("%")); f.confidence = pc / 100.0; ui.end_row();
            ui.label(tr("Seasonality"));
            ui.horizontal(|ui| {
                let mut auto = f.season.is_none();
                crate::theme::radio_value(ui, &mut auto, true, tr("Detect Automatically"));
                crate::theme::radio_value(ui, &mut auto, false, tr("Set Manually"));
                if auto { f.season = None; } else { let mut v = f.season.unwrap_or(12); ui.add(egui::DragValue::new(&mut v).range(0..=8760)); f.season = Some(v); }
            });
            ui.end_row();
        });
        crate::theme::check(ui, &mut f.stats, tr("Include forecast statistics"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Create").clicked() {
                match (parse_ref(app, &f.timeline), parse_ref(app, &f.values)) {
                    (Some((ts, tr_)), Some((vs, vr))) if ts == vs => {
                        app.checkpoint("Forecast Sheet");
                        let o = sheet_engine::forecast::Options { periods: f.periods, confidence: f.confidence, seasonality: f.season, include_stats: f.stats };
                        match sheet_engine::forecast::create(&mut app.book, ts, tr_, vr, &o) {
                            Ok((at, table)) => {
                                app.book.active = at;
                                app.sel = crate::app::Selection { anchor: Pos::new(0, 0), active: Pos::new(0, 0), extra: vec![] };
                                forecast_chart(app, at, table);
                                app.changed();
                                keep = false;
                            }
                            Err(e) => { app.undo(); app.error = Some(tr(&e)); }
                        }
                    }
                    _ => app.error = Some(tr("The timeline and values ranges must be on the same sheet.")),
                }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

/// A line chart of values, forecast and bounds beside the forecast table.
fn forecast_chart(app: &mut SheetApp, at: usize, table: Range) {
    use sheet_engine::chart::{Chart, ChartKind};
    use sheet_engine::drawing::{Anchor, ObjKind, Object};
    let mut c = Chart::from_range(&app.book, at, table, ChartKind::Line, Some(false));
    // The timeline is the categories, not a series.
    if c.series.len() == 5 { c.series.remove(0); }
    c.markers = false;
    let s = &app.book.sheets[at];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    let (x, y) = (cols.pos(table.c1 + 2), rows.pos(table.r0 + 1));
    let mut a = Anchor::default();
    a.set_rect([x, y, x + 520.0, y + 300.0], |x| cols.at(x), |c| cols.pos(c), |y| rows.at(y), |r| rows.pos(r));
    app.book.sheets[at].objects.push(Object::new(ObjKind::Chart(Box::new(c)), a));
}

// ---- Solver ---------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Solver {
    pub objective: String,
    pub goal: u8,
    pub value: String,
    pub vars: String,
    pub constraints: Vec<Constraint>,
    pub sel: Option<usize>,
    pub nonneg: bool,
    pub engine: Engine,
    /// Add/Change Constraint: (index being changed, lhs, relation, rhs).
    pub adding: Option<(Option<usize>, String, Rel, String)>,
    /// Solver Results: (status message, found, keep the solution).
    pub result: Option<(String, bool, bool)>,
}

pub fn open_solver(app: &mut SheetApp) {
    let m = solver::load(&app.book, app.sheet());
    let (goal, value) = match m.goal { Goal::Max => (0, String::new()), Goal::Min => (1, String::new()), Goal::Value(v) => (2, sheet_engine::value::fmt_num(v)) };
    let objective = if m.objective.is_empty() { abs(app.sel.active) } else { m.objective.clone() };
    open(app, Dlg::Solver(Solver { objective, goal, value, vars: m.vars.clone(), constraints: m.constraints.clone(), sel: None, nonneg: m.nonneg, engine: m.engine, adding: None, result: None }));
}

fn model_of(d: &Solver) -> Model {
    let goal = match d.goal { 1 => Goal::Min, 2 => Goal::Value(d.value.trim().parse().unwrap_or(0.0)), _ => Goal::Max };
    let tidy = |t: &str| t.replace('$', "").trim().to_string();
    Model { objective: tidy(&d.objective), goal, vars: tidy(&d.vars), constraints: d.constraints.clone(), nonneg: d.nonneg, engine: d.engine, ..Default::default() }
}

pub fn solver_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut Solver) -> bool {
    let mut keep = true; let mut open_ = true;
    if let Some((msg, found, keep_sol)) = &mut d.result {
        let mut done = false;
        crate::theme::dialog(ctx, "Solver Results").open(&mut open_).resizable(false).default_width(420.0).show(ctx, |ui| {
            ui.label(tr(msg));
            crate::theme::radio_value(ui, keep_sol, true, tr("Keep Solver Solution"));
            crate::theme::radio_value(ui, keep_sol, false, tr("Restore Original Values"));
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "OK").clicked() { if !*keep_sol || !*found { app.undo(); } done = true; }
                if crate::theme::secondary(ui, "Cancel").clicked() { app.undo(); done = true; }
            });
        });
        if done { keep = false; }
        return keep && open_;
    }
    if let Some((idx, lhs, rel, rhs)) = &mut d.adding {
        let mut close = None;
        crate::theme::dialog(ctx, if idx.is_some() { "Change Constraint" } else { "Add Constraint" }).open(&mut open_).resizable(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| { ui.label(tr("Cell Reference:")); crate::refedit::field(ui, lhs, "whatif_ui.rs:339", 120.0); });
                crate::theme::Combo::from_id_salt("solver-rel").width(60.0).selected_text(rel.label()).show_ui(ui, |ui| for r in Rel::ALL { ui.selectable_value(rel, r, r.label()); });
                ui.vertical(|ui| {
                    ui.label(tr("Constraint:"));
                    let word = match rel { Rel::Int => Some("integer"), Rel::Bin => Some("binary"), Rel::Dif => Some("AllDifferent"), _ => None };
                    if let Some(w) = word { *rhs = w.to_string(); ui.label(tr(w)); } else { ui.add(egui::TextEdit::singleline(rhs).desired_width(120.0)); }
                });
            });
            crate::theme::footer(ui, |ui| {
                let ok = !lhs.trim().is_empty() && !rhs.trim().is_empty();
                if crate::theme::primary_enabled(ui, ok, "OK").clicked() { close = Some(true); }
                if crate::theme::secondary(ui, "Add").clicked() && ok { close = Some(false); }
                if crate::theme::secondary(ui, "Cancel").clicked() { close = Some(true); *lhs = String::new(); }
            });
        });
        if let Some(done) = close {
            let (idx, lhs, rel, rhs) = d.adding.take().unwrap();
            if !lhs.trim().is_empty() {
                let c = Constraint { lhs: lhs.replace('$', "").trim().to_string(), rel, rhs: rhs.replace('$', "").trim().to_string() };
                match idx { Some(i) if i < d.constraints.len() => d.constraints[i] = c, _ => d.constraints.push(c) }
            }
            if !done { d.adding = Some((None, String::new(), Rel::Le, String::new())); }
        }
        return keep && open_;
    }
    crate::theme::dialog(ctx, "Solver Parameters").open(&mut open_).resizable(false).default_width(520.0).show(ctx, |ui| {
        ui.set_min_width(500.0);
        crate::theme::grid(ui, egui::Grid::new("solver").num_columns(2).spacing(vec2(8.0, 6.0)), |ui| {
            ui.label(tr("Set Objective:")); crate::refedit::field(ui, &mut d.objective, "whatif_ui.rs:367", 200.0); ui.end_row();
            ui.label(tr("To:"));
            ui.horizontal(|ui| {
                crate::theme::radio_value(ui, &mut d.goal, 0, tr("Max"));
                crate::theme::radio_value(ui, &mut d.goal, 1, tr("Min"));
                crate::theme::radio_value(ui, &mut d.goal, 2, tr("Value Of:"));
                ui.add_enabled(d.goal == 2, egui::TextEdit::singleline(&mut d.value).desired_width(70.0));
            });
            ui.end_row();
            ui.label(tr("By Changing Variable Cells:")); crate::refedit::field(ui, &mut d.vars, "whatif_ui.rs:376", 200.0); ui.end_row();
        });
        ui.label(tr("Subject to the Constraints:"));
        ui.horizontal_top(|ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_size(vec2(340.0, 110.0));
                for (i, c) in d.constraints.iter().enumerate() { if ui.selectable_label(d.sel == Some(i), format!("{} {} {}", c.lhs, c.rel.label(), c.rhs)).clicked() { d.sel = Some(i); } }
            });
            ui.vertical(|ui| {
                if ui.button(tr("Add")).clicked() { d.adding = Some((None, String::new(), Rel::Le, String::new())); }
                if ui.add_enabled(d.sel.is_some(), egui::Button::new(tr("Change"))).clicked() { let c = d.constraints[d.sel.unwrap()].clone(); d.adding = Some((d.sel, c.lhs, c.rel, c.rhs)); }
                if ui.add_enabled(d.sel.is_some(), egui::Button::new(tr("Delete"))).clicked() { d.constraints.remove(d.sel.take().unwrap()); }
                if ui.button(tr("Reset All")).clicked() { d.constraints.clear(); d.vars.clear(); d.sel = None; }
            });
        });
        crate::theme::check(ui, &mut d.nonneg, tr("Make Unconstrained Variables Non-Negative"));
        ui.horizontal(|ui| {
            ui.label(tr("Select a Solving Method:"));
            let cur = Engine::ALL.iter().find(|e| e.0 == d.engine).map_or("", |e| e.1);
            crate::theme::Combo::from_id_salt("solver-eng").selected_text(tr(cur)).show_ui(ui, |ui| for (e, l) in Engine::ALL { ui.selectable_value(&mut d.engine, e, tr(l)); });
        });
        ui.label(egui::RichText::new(tr("Select the GRG Nonlinear engine for smooth nonlinear problems, the LP Simplex engine for linear problems, and the Evolutionary engine for problems that are non-smooth.")).small());
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Solve").clicked() {
                let m = model_of(d);
                let s = app.sheet();
                app.checkpoint("Solver");
                solver::save(&mut app.book, s, &m);
                match solver::solve(&mut app.book, s, &m, true) {
                    Ok(r) => { app.changed(); d.result = Some((r.status.message().to_string(), r.status.found(), true)); }
                    Err(e) => { app.undo(); app.error = Some(tr(&e)); }
                }
            }
            if crate::theme::secondary(ui, "Close").clicked() {
                let m = model_of(d);
                let s = app.sheet();
                if solver::load(&app.book, s) != m { app.checkpoint("Solver"); solver::save(&mut app.book, s, &m); app.dirty = true; }
                keep = false;
            }
        });
    });
    keep && open_
}
