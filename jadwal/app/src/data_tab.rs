//! The Data tab laid out as Excel's (get-data.js, queries.js, editlinks.js, sheet-group.js):
//! Get & Transform Data, Queries & Connections (with the Queries & Connections pane, Properties
//! and Edit Links), Sort & Filter, Data Tools, Forecast, Outline and Analyze. The dialogs live in
//! data_import.rs, data_tools.rs, whatif_ui.rs and analysis_ui.rs; their state is kept in egui's
//! memory like the Formulas tab's.

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{vec2, Ui};
use qalam_kit::widgets::*;
use sheet_engine::connections::{self, Connection};

#[derive(Clone, Default)]
pub struct State {
    pub dlg: Option<Dlg>,
    /// The Queries & Connections pane.
    pub pane: bool,
    pub pane_sel: Option<u32>,
    /// Recent Sources: files and addresses imported from.
    pub recent: Vec<String>,
}

#[derive(Clone)]
pub enum Dlg {
    TextImport(crate::data_import::TextWizard),
    Navigator(crate::data_import::Navigator),
    WebUrl { url: String },
    TableRange { range: String },
    TextToColumns(crate::data_import::TextWizard),
    RemoveDup(crate::data_tools::Dedupe),
    Advanced(crate::data_tools::Advanced),
    Consolidate(crate::data_tools::Consolidate),
    Subtotal(crate::data_tools::Subtotal),
    GroupAsk { ungroup: bool, rows: bool },
    OutlineSettings { below: bool, right: bool },
    DataForm(crate::data_tools::Form),
    GoalSeek(crate::whatif_ui::GoalSeek),
    DataTable { row: String, col: String },
    Scenarios(crate::whatif_ui::Scenarios),
    Forecast(crate::whatif_ui::Forecast),
    Solver(crate::whatif_ui::Solver),
    Analysis(crate::analysis_ui::Analysis),
    Links { sel: Option<usize> },
    Properties { id: u32, name: String, description: String, refresh_on_open: bool },
}

fn id() -> egui::Id { egui::Id::unique("data-tab-state") }
pub fn state(app: &SheetApp) -> State { app.ctx.data(|d| d.get_temp::<State>(id())).unwrap_or_default() }
fn put(app: &SheetApp, s: State) { app.ctx.data_mut(|d| d.insert_temp(id(), s)); }
pub fn with<R>(app: &SheetApp, f: impl FnOnce(&mut State) -> R) -> R { let mut s = state(app); let r = f(&mut s); put(app, s); r }
/// Opens a dialog of the tab.
pub fn open(app: &SheetApp, d: Dlg) { with(app, |s| s.dlg = Some(d)); }
pub fn note_recent(app: &SheetApp, src: &str) { with(app, |s| { s.recent.retain(|r| r != src); s.recent.insert(0, src.to_string()); s.recent.truncate(10); }); }

// ---- the ribbon -----------------------------------------------------------------------------

pub fn tab(app: &mut SheetApp, ui: &mut Ui) {
    group(ui, "Get & Transform Data", |ui| {
        let r = large_menu(ui, "download", "Get Data", "Import data from files, web pages and ranges");
        crate::theme::menu(&r, |ui| {
            crate::theme::submenu(ui, tr("From File"), |ui| {
                if menu_item(ui, "text", "From Text/CSV") { crate::data_import::pick_text(app); ui.close(); }
                if menu_item(ui, "folderOpen", "From Workbook") { crate::data_import::pick_workbook(app); ui.close(); }
            });
            if menu_item(ui, "link", "From Web") { open(app, Dlg::WebUrl { url: String::new() }); ui.close(); }
            if menu_item(ui, "table", "From Table/Range") { open_table_range(app); ui.close(); }
            ui.separator();
            if menu_item(ui, "formula", "Launch Power Query Editor") { crate::pq_editor::open(app, None, None); ui.close(); }
            if menu_item(ui, "info", "Queries & Connections") { with(app, |s| s.pane = true); ui.close(); }
        });
        rows(ui, |ui| {
            if small(ui, "text", Some("From Text/CSV"), false, true, "Import data from a text or CSV file").clicked() { crate::data_import::pick_text(app); }
            if small(ui, "link", Some("From Web"), false, true, "Import a table from a web page").clicked() { open(app, Dlg::WebUrl { url: String::new() }); }
            if small(ui, "formatAsTable", Some("From Table/Range"), false, true, "Make a query from the selected table or range").clicked() { open_table_range(app); }
        });
        rows(ui, |ui| {
            let r = small_menu(ui, "history", "Recent Sources", "Import again from a recently used source");
            crate::theme::menu(&r, |ui| {
                let recent = state(app).recent;
                if recent.is_empty() { ui.add_enabled(false, egui::Button::new(tr("No recent sources"))); }
                for src in recent { if ui.button(&src).clicked() { crate::data_import::open_source(app, &src); ui.close(); } }
            });
            if small(ui, "manageSources", Some("Existing Connections"), false, true, "Use a connection this workbook already has").clicked() { with(app, |s| s.pane = true); }
        });
    });
    group(ui, "Queries & Connections", |ui| {
        let (b, m) = large_split(ui, "updateTable", "Refresh All", "Refresh every connection in the workbook");
        if b.clicked() { refresh_all(app); }
        crate::theme::menu(&m, |ui| {
            if menu_item(ui, "updateTable", "Refresh All") { refresh_all(app); ui.close(); }
            let sel = state(app).pane_sel;
            if small(ui, "updateTable", Some("Refresh"), false, sel.is_some() || !connections::list(&app.book).is_empty(), "Refresh the selected connection").clicked() { refresh_one(app, sel); ui.close(); }
        });
        rows(ui, |ui| {
            let on = state(app).pane;
            if small(ui, "info", Some("Queries & Connections"), on, true, "Show the workbook's queries and connections").clicked() { with(app, |s| s.pane = !s.pane); }
            let any = !connections::list(&app.book).is_empty();
            if small(ui, "tableProperties", Some("Properties"), false, any, "Name, description and refresh settings of a connection").clicked() { open_properties(app, state(app).pane_sel); }
            let links = !connections::links(&app.book).is_empty();
            if small(ui, "unlink", Some("Edit Links"), false, links, "Change or break links to other workbooks").clicked() { open(app, Dlg::Links { sel: None }); }
        });
    });
    group(ui, "Sort & Filter", |ui| {
        rows(ui, |ui| {
            if small(ui, "sort", None, false, true, &format!("{}: {}", tr("Sort A to Z"), tr("Sort smallest to largest"))).clicked() { app.sort(true); }
            if small(ui, "sort", None, false, true, &format!("{}: {}", tr("Sort Z to A"), tr("Sort largest to smallest"))).clicked() { app.sort(false); }
        });
        if large(ui, "sortFilter", "Sort", false, true, "Sort by several columns").clicked() { crate::dialogs::open_sort(app); }
        if large(ui, "filter", "Filter", crate::filter_ui::filter_on(app), true, "Turn filtering on or off").clicked() { crate::filter_ui::toggle_filter(app); }
        rows(ui, |ui| {
            let s = app.sheet(); let any = app.book.has_filters(s) || !app.book.sheets[s].filtered_rows.is_empty();
            if small(ui, "clearFilter", Some("Clear"), false, any, "Clear the filters on this sheet").clicked() { clear(app); }
            if small(ui, "reapply", Some("Reapply"), false, any, "Filter again with the current values").clicked() { crate::filter_ui::reapply(app); }
            if small(ui, "filter", Some("Advanced"), false, true, "Filter with a criteria range, or copy the matching rows").clicked() { crate::data_tools::open_advanced(app); }
        });
    });
    group(ui, "Data Tools", |ui| {
        if large(ui, "textToColumns", "Text to Columns", false, true, "Split one column of text into several").clicked() { crate::data_import::open_text_to_columns(app); }
        rows(ui, |ui| {
            if small(ui, "flashFill", None, false, true, &format!("{}: {}", tr("Flash Fill"), tr("Fill the column from the pattern of your examples"))).clicked() { crate::ribbon::flash_fill(app); }
            if small(ui, "removeDuplicates", None, false, true, &format!("{}: {}", tr("Remove Duplicates"), tr("Delete rows that repeat"))).clicked() { crate::data_tools::open_dedupe(app); }
            let r = small_menu(ui, "dataValidation", "", &format!("{}: {}", tr("Data Validation"), tr("Choose what can be typed into the selected cells")));
            crate::theme::menu(&r, |ui| crate::validation_ui::validation_menu(app, ui));
        });
        rows(ui, |ui| {
            if small(ui, "combine", None, false, true, &format!("{}: {}", tr("Consolidate"), tr("Combine several ranges into one table"))).clicked() { crate::data_tools::open_consolidate(app); }
            if small(ui, "changeCase", None, false, true, &format!("{}: {}", tr("Trim Spaces"), tr("Remove extra spaces from text"))).clicked() { crate::ribbon::trim_spaces(app); }
            if small(ui, "selectRecipients", None, false, true, &format!("{}: {}", tr("Form"), tr("See, add and find the list's rows one record at a time"))).clicked() { crate::data_tools::open_form(app); }
        });
        rows(ui, |ui| {
            if small(ui, "matchFields", None, false, true, &format!("{}: {}", tr("Relationships"), tr("Relate tables of a Data Model"))).clicked() { app.error = Some(tr("Jadwal doesn't have a Data Model, so tables can't be related. Use XLOOKUP or a PivotTable over one table instead.")); }
            if small(ui, "pivotTable", None, false, true, &format!("{}: {}", tr("Manage Data Model"), tr("Manage the Data Model"))).clicked() { app.error = Some(tr("Jadwal doesn't have a Data Model, so tables can't be related. Use XLOOKUP or a PivotTable over one table instead.")); }
        });
    });
    group(ui, "Forecast", |ui| {
        let r = large_menu(ui, "formula", "What-If Analysis", "Try different values in formulas");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "rules", "Scenario Manager…") { crate::whatif_ui::open_scenarios(app); ui.close(); }
            if menu_item(ui, "goTo", "Goal Seek…") { crate::whatif_ui::open_goal_seek(app); ui.close(); }
            if menu_item(ui, "table", "Data Table…") { open(app, Dlg::DataTable { row: String::new(), col: String::new() }); ui.close(); }
        });
        if large(ui, "chartLine", "Forecast\nSheet", false, true, "Create a forecast from a time series").clicked() { crate::whatif_ui::open_forecast(app); }
    });
    let launched = group_launcher(ui, "Outline", |ui| {
        let (b, m) = large_split(ui, "groupObjects", "Group", "Group rows or columns (⌥⇧→)");
        if b.clicked() { ask_group(app, false); }
        crate::theme::menu(&m, |ui| {
            if menu_item(ui, "groupObjects", "Group…") { ask_group(app, false); ui.close(); }
            if menu_item(ui, "outlineLevel", "Auto Outline") { auto_outline(app); ui.close(); }
        });
        let (b, m) = large_split(ui, "ungroupObjects", "Ungroup", "Ungroup rows or columns (⌥⇧←)");
        if b.clicked() { ask_group(app, true); }
        crate::theme::menu(&m, |ui| {
            if menu_item(ui, "ungroupObjects", "Ungroup…") { ask_group(app, true); ui.close(); }
            if menu_item(ui, "clearFormat", "Clear Outline") { clear_outline(app); ui.close(); }
        });
        if large(ui, "autoSum", "Subtotal", false, true, "Insert subtotals and totals for groups of rows").clicked() { crate::data_tools::open_subtotal(app); }
        rows(ui, |ui| {
            if small(ui, "expand", Some("Show Detail"), false, true, "Expand the group under the selection").clicked() { detail(app, true); }
            if small(ui, "collapse", Some("Hide Detail"), false, true, "Collapse the group under the selection").clicked() { detail(app, false); }
        });
    }, Some("Outline settings"));
    if launched { let o = &app.book.sheets[app.sheet()].outline; open(app, Dlg::OutlineSettings { below: o.summary_below, right: o.summary_right }); }
    group(ui, "Analyze", |ui| {
        if large(ui, "mathTrig", "Data\nAnalysis", false, true, "Statistical and engineering analysis tools").clicked() { crate::analysis_ui::open(app); }
        if large(ui, "calcOptions", "Solver", false, true, "Find the best values for an objective under constraints").clicked() { crate::whatif_ui::open_solver(app); }
    });
}

fn clear(app: &mut SheetApp) {
    crate::filter_ui::clear_filters(app);
    let s = app.sheet();
    let rows: Vec<u32> = std::mem::take(&mut app.book.sheets[s].filtered_rows).into_iter().collect();
    for r in rows { app.book.sheets[s].hidden_rows.remove(&r); }
}

fn ask_group(app: &mut SheetApp, ungroup: bool) {
    match crate::outline_ui::whole(app) {
        Some(rows) => crate::outline_ui::group(app, rows, ungroup),
        None => open(app, Dlg::GroupAsk { ungroup, rows: true }),
    }
}

fn auto_outline(app: &mut SheetApp) {
    let s = app.sheet();
    let mut r = app.sel.range();
    if r.is_cell() { r = app.book.current_region(s, r.start()); }
    app.checkpoint("Auto Outline");
    if sheet_engine::outline::auto_outline(&mut app.book, s, r) == 0 { app.error = Some(tr("Cannot create an outline.")); }
    app.dirty = true;
}

fn clear_outline(app: &mut SheetApp) {
    let s = app.sheet();
    app.checkpoint("Clear Outline");
    sheet_engine::outline::clear(&mut app.book.sheets[s], true);
    sheet_engine::outline::clear(&mut app.book.sheets[s], false);
    app.dirty = true;
}

fn detail(app: &mut SheetApp, show: bool) {
    let s = app.sheet();
    let r = app.sel.range();
    let rows = crate::outline_ui::whole(app).unwrap_or(true);
    let at = if rows { r.r0 } else { r.c0 };
    app.checkpoint(if show { "Show Detail" } else { "Hide Detail" });
    let mut done = sheet_engine::outline::detail(&mut app.book.sheets[s], rows, at, show);
    if !done && crate::outline_ui::whole(app).is_none() { done = sheet_engine::outline::detail(&mut app.book.sheets[s], false, r.c0, show); }
    if !done { app.notify(&tr("There is no group here to show or hide.")); }
    app.dirty = true;
}

fn open_table_range(app: &mut SheetApp) {
    let s = app.sheet();
    let mut r = app.sel.range();
    if let Some(t) = app.book.table_at(s, app.sel.active) { r = app.book.sheets[s].tables[t].range; } else if r.is_cell() { r = app.book.current_region(s, r.start()); }
    let reference = format!("{}!{}", sheet_engine::addr::quote_sheet(&app.book.sheets[s].name), r.a1());
    open(app, Dlg::TableRange { range: reference });
}

// ---- connections ----------------------------------------------------------------------------

/// Fetches a web page (From Web, refreshing web connections).
pub fn fetch(url: &str) -> Result<String, String> {
    let resp = ureq::get(url).header("User-Agent", "Jadwal").call().map_err(|e| e.to_string())?;
    resp.into_body().read_to_string().map_err(|e| e.to_string())
}

fn refresh_all(app: &mut SheetApp) {
    if connections::list(&app.book).is_empty() { app.notify(&tr("This workbook has no connections to refresh.")); return; }
    app.checkpoint("Refresh All");
    let errs = connections::refresh_all(&mut app.book, &fetch);
    app.changed();
    if errs.is_empty() { app.notify(&tr("All connections refreshed.")); } else { app.error = Some(errs.iter().map(|(n, e)| format!("{n}: {e}")).collect::<Vec<_>>().join("\n")); }
}

fn refresh_one(app: &mut SheetApp, id: Option<u32>) {
    let Some(id) = id.or_else(|| connections::list(&app.book).first().map(|c| c.id)) else { return };
    app.checkpoint("Refresh");
    match connections::refresh(&mut app.book, id, &fetch) { Ok(_) => { app.changed(); } Err(e) => app.error = Some(e) }
}

fn open_properties(app: &mut SheetApp, id: Option<u32>) {
    let list = connections::list(&app.book);
    let Some(c) = id.and_then(|i| list.iter().find(|c| c.id == i)).or(list.first()) else { return };
    open(app, Dlg::Properties { id: c.id, name: c.name.clone(), description: c.description.clone(), refresh_on_open: c.refresh_on_open });
}

/// The Queries & Connections pane.
pub fn panels(app: &mut SheetApp, ui: &mut Ui) {
    if !state(app).pane { return; }
    egui::Panel::right("jadwal-queries-pane").resizable(true).default_size(300.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading(tr("Queries & Connections"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if ui.button("✕").on_hover_text(tr("Close")).clicked() { with(app, |s| s.pane = false); } });
        });
        ui.separator();
        let list = connections::list(&app.book);
        let sel = state(app).pane_sel;
        if list.is_empty() { ui.label(tr("This workbook has no queries or connections. Use Get Data to add one.")); }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for c in &list {
                let dest = connections::destination(&app.book, c.id);
                let sub = match dest { Some((s, r)) => format!("{} · {}!{}", tr(c.kind()), app.book.sheets[s].name, r.a1()), None => format!("{} · {}", tr(c.kind()), tr("Connection only.")) };
                let r = ui.selectable_label(sel == Some(c.id), format!("{}\n{}", c.name, sub)).on_hover_text(c.location());
                if r.clicked() { with(app, |s| s.pane_sel = Some(c.id)); }
                if r.double_clicked() { if let Some((s, rg)) = dest { app.book.active = s; app.sel = crate::app::Selection { active: rg.start(), anchor: rg.end(), extra: vec![] }; app.reveal = true; } }
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let has = sel.is_some_and(|i| list.iter().any(|c| c.id == i));
            if ui.add_enabled(has, egui::Button::new(tr("Refresh"))).clicked() { refresh_one(app, sel); }
            if ui.add_enabled(!list.is_empty(), egui::Button::new(tr("Refresh All"))).clicked() { refresh_all(app); }
            let query = sel.and_then(|i| list.iter().find(|c| c.id == i)).and_then(|c| if let connections::Source::Mashup { query } = &c.source { Some(query.clone()) } else { None });
            if ui.add_enabled(query.is_some(), egui::Button::new(tr("Edit"))).on_hover_text(tr("Open the query in the Power Query Editor")).clicked() { crate::pq_editor::open(app, query.as_deref(), None); }
            if ui.add_enabled(has, egui::Button::new(tr("Properties…"))).clicked() { open_properties(app, sel); }
            if ui.add_enabled(has, egui::Button::new(tr("Delete"))).clicked() {
                app.checkpoint("Delete Connection");
                // A query's connection goes with the query.
                match &query { Some(q) => { if let Err(e) = sheet_engine::powerquery::delete(&mut app.book, q) { app.error = Some(e); } } None => connections::remove(&mut app.book, sel.unwrap()) }
                app.dirty = true;
                with(app, |s| s.pane_sel = None);
            }
        });
    });
}

// ---- dialogs --------------------------------------------------------------------------------

/// The tab's dialogs (run after the app's dialogs).
pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let mut s = state(app);
    let Some(mut d) = s.dlg.take() else { return };
    put(app, s);
    let keep = match &mut d {
        Dlg::TextImport(w) => crate::data_import::text_wizard(app, ctx, w, false),
        Dlg::TextToColumns(w) => crate::data_import::text_wizard(app, ctx, w, true),
        Dlg::Navigator(n) => crate::data_import::navigator(app, ctx, n),
        Dlg::WebUrl { url } => crate::data_import::web_dialog(app, ctx, url),
        Dlg::TableRange { range } => crate::data_import::table_range_dialog(app, ctx, range),
        Dlg::RemoveDup(x) => crate::data_tools::dedupe_dialog(app, ctx, x),
        Dlg::Advanced(x) => crate::data_tools::advanced_dialog(app, ctx, x),
        Dlg::Consolidate(x) => crate::data_tools::consolidate_dialog(app, ctx, x),
        Dlg::Subtotal(x) => crate::data_tools::subtotal_dialog(app, ctx, x),
        Dlg::DataForm(x) => crate::data_tools::form_dialog(app, ctx, x),
        Dlg::GroupAsk { ungroup, rows } => group_dialog(app, ctx, *ungroup, rows),
        Dlg::OutlineSettings { below, right } => outline_settings(app, ctx, below, right),
        Dlg::GoalSeek(x) => crate::whatif_ui::goal_seek_dialog(app, ctx, x),
        Dlg::DataTable { row, col } => crate::whatif_ui::data_table_dialog(app, ctx, row, col),
        Dlg::Scenarios(x) => crate::whatif_ui::scenarios_dialog(app, ctx, x),
        Dlg::Forecast(x) => crate::whatif_ui::forecast_dialog(app, ctx, x),
        Dlg::Solver(x) => crate::whatif_ui::solver_dialog(app, ctx, x),
        Dlg::Analysis(x) => crate::analysis_ui::dialog(app, ctx, x),
        Dlg::Links { sel } => links_dialog(app, ctx, sel),
        Dlg::Properties { id, name, description, refresh_on_open } => properties_dialog(app, ctx, *id, name, description, refresh_on_open),
    };
    // A dialog may have opened another one.
    if keep { with(app, |s| if s.dlg.is_none() { s.dlg = Some(d); }); }
}

fn group_dialog(app: &mut SheetApp, ctx: &egui::Context, ungroup: bool, rows: &mut bool) -> bool {
    let mut keep = true; let mut open = true;
    crate::theme::dialog(ctx, if ungroup { "Ungroup" } else { "Group" }).open(&mut open).resizable(false).show(ctx, |ui| {
        crate::theme::radio_value(ui, rows, true, tr("Rows"));
        crate::theme::radio_value(ui, rows, false, tr("Columns"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { crate::outline_ui::group(app, *rows, ungroup); keep = false; }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

fn outline_settings(app: &mut SheetApp, ctx: &egui::Context, below: &mut bool, right: &mut bool) -> bool {
    let mut keep = true; let mut open = true;
    crate::theme::dialog(ctx, "Settings").open(&mut open).resizable(false).show(ctx, |ui| {
        crate::theme::section(ui, "Direction");
        crate::theme::check(ui, below, tr("Summary rows below detail"));
        crate::theme::check(ui, right, tr("Summary columns to right of detail"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let s = app.sheet();
                app.checkpoint("Outline Settings");
                let o = &mut app.book.sheets[s].outline;
                o.summary_below = *below; o.summary_right = *right;
                app.dirty = true; keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

fn links_dialog(app: &mut SheetApp, ctx: &egui::Context, sel: &mut Option<usize>) -> bool {
    let mut keep = true; let mut open = true;
    let links = connections::links(&app.book);
    crate::theme::dialog(ctx, "Edit Links").open(&mut open).resizable(false).default_width(520.0).show(ctx, |ui| {
        ui.set_min_width(500.0);
        crate::theme::grid(ui, egui::Grid::new("links").num_columns(3).striped(true), |ui| {
            ui.strong(tr("Source")); ui.strong(tr("Type")); ui.strong(tr("Status")); ui.end_row();
            for (i, l) in links.iter().enumerate() {
                let exists = std::path::Path::new(&l.target.trim_start_matches("file:///")).exists();
                if ui.selectable_label(*sel == Some(i), if l.target.is_empty() { l.part.clone() } else { l.target.clone() }).clicked() { *sel = Some(i); }
                ui.label(tr("Worksheet"));
                ui.label(if exists { tr("OK") } else { tr("Error: Source not found") });
                ui.end_row();
            }
        });
        if links.is_empty() { ui.label(tr("This workbook has no links to other workbooks.")); }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let l = sel.and_then(|i| links.get(i)).cloned();
            if ui.add_enabled(l.is_some(), egui::Button::new(tr("Change Source…"))).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter(tr("Excel Workbook"), &["xlsx", "xlsm"]).pick_file() {
                    app.checkpoint("Change Source");
                    connections::change_link(&mut app.book, l.as_ref().unwrap(), &format!("file:///{}", p.to_string_lossy().trim_start_matches('/')));
                    app.dirty = true;
                }
            }
            if ui.add_enabled(l.is_some(), egui::Button::new(tr("Break Link"))).on_hover_text(tr("Replace formulas that use this link with their values")).clicked() {
                app.checkpoint("Break Link");
                let n = connections::break_link(&mut app.book, l.as_ref().unwrap());
                app.changed();
                app.notify(&crate::i18n::trf("{} formulas replaced by their values.", &[&n]));
            }
            if ui.add_enabled(l.is_some(), egui::Button::new(tr("Open Source"))).clicked() {
                let t = l.as_ref().unwrap().target.trim_start_matches("file:///").to_string();
                let p = std::path::PathBuf::from(if t.starts_with('/') { t } else { format!("/{t}") });
                if p.exists() { app.open_path(p); keep = false; } else { app.error = Some(tr("Error: Source not found")); }
            }
        });
        crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Close").clicked() { keep = false; });
    });
    keep && open
}

fn properties_dialog(app: &mut SheetApp, ctx: &egui::Context, id: u32, name: &mut String, description: &mut String, refresh_on_open: &mut bool) -> bool {
    let mut keep = true; let mut open = true;
    let Some(c) = connections::list(&app.book).into_iter().find(|c| c.id == id) else { return false };
    crate::theme::dialog(ctx, "Connection Properties").open(&mut open).resizable(false).default_width(440.0).show(ctx, |ui| {
        ui.set_min_width(420.0);
        crate::theme::grid(ui, egui::Grid::new("conn-props").num_columns(2).spacing(vec2(8.0, 8.0)), |ui| {
            ui.label(tr("Connection name:")); ui.text_edit_singleline(name); ui.end_row();
            ui.label(tr("Description:")); ui.text_edit_multiline(description); ui.end_row();
            ui.label(tr("Source:")); ui.label(c.location()); ui.end_row();
            ui.label(tr("Type:")); ui.label(tr(c.kind())); ui.end_row();
        });
        crate::theme::check(ui, refresh_on_open, tr("Refresh data when opening the file"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                app.checkpoint("Connection Properties");
                connections::update(&mut app.book, &Connection { name: name.trim().to_string(), description: description.clone(), refresh_on_open: *refresh_on_open, ..c.clone() });
                app.dirty = true; keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}
