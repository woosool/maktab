//! Data Tools and Outline dialogs (dedupedialog.js, advanced-filter.js, consolidatedialog.js,
//! subtotaldialog.js, dataform.js): Remove Duplicates with a choice of columns, Advanced Filter,
//! Consolidate, Subtotal and the Data Form.

use crate::app::SheetApp;
use crate::data_tab::{open, Dlg};
use crate::i18n::{tr, trf};
use egui::vec2;
use sheet_engine::addr::{parse_a1, parse_range, Pos, Range};

/// A reference typed in a dialog: `A1:B9`, `Sheet2!A1:B9` or `'My sheet'!$A$1`.
pub fn parse_ref(app: &SheetApp, t: &str) -> Option<(usize, Range)> {
    let t = t.trim().trim_start_matches('=');
    let (s, r) = match t.rsplit_once('!') { Some((s, r)) => (app.book.sheet_index(s.trim_matches('\''))?, r), None => (app.sheet(), t) };
    Some((s, parse_range(&r.replace('$', ""))?))
}

pub fn ref_text(app: &SheetApp, s: usize, r: Range) -> String { format!("{}!{}", sheet_engine::addr::quote_sheet(&app.book.sheets[s].name), crate::formulabar::absolute(r)) }

fn list_range(app: &SheetApp) -> Range {
    let s = app.sheet();
    let r = app.sel.range();
    if let Some(t) = app.book.table_at(s, app.sel.active) { return app.book.sheets[s].tables[t].range; }
    if r.is_cell() { app.book.current_region(s, r.start()) } else { r }
}

fn headers(app: &SheetApp, r: Range, header: bool) -> Vec<String> {
    let s = &app.book.sheets[app.sheet()];
    (r.c0..=r.c1).map(|c| if header { let t = s.value(Pos::new(r.r0, c)).display(); if t.is_empty() { format!("{} {}", tr("Column"), sheet_engine::addr::col_name(c)) } else { t } } else { format!("{} {}", tr("Column"), sheet_engine::addr::col_name(c)) }).collect()
}

// ---- Remove Duplicates --------------------------------------------------------------------------

#[derive(Clone)]
pub struct Dedupe { pub range: Range, pub header: bool, pub cols: Vec<bool> }

pub fn open_dedupe(app: &mut SheetApp) {
    let r = list_range(app);
    let header = app.book.looks_like_header(app.sheet(), r);
    open(app, Dlg::RemoveDup(Dedupe { range: r, header, cols: vec![true; r.cols() as usize] }));
}

pub fn dedupe_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut Dedupe) -> bool {
    let mut keep = true; let mut open_ = true;
    crate::theme::dialog(ctx, "Remove Duplicates").open(&mut open_).resizable(false).default_width(360.0).show(ctx, |ui| {
        ui.label(tr("To delete duplicate values, select one or more columns that contain duplicates."));
        ui.horizontal(|ui| {
            if ui.button(tr("Select All")).clicked() { d.cols.iter_mut().for_each(|c| *c = true); }
            if ui.button(tr("Unselect All")).clicked() { d.cols.iter_mut().for_each(|c| *c = false); }
            crate::theme::check(ui, &mut d.header, tr("My data has headers"));
        });
        let names = headers(app, d.range, d.header);
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| { for (i, n) in names.iter().enumerate() { if let Some(c) = d.cols.get_mut(i) { crate::theme::check(ui, c, n); } } });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, d.cols.iter().any(|c| *c), "OK").clicked() {
                let cols: Vec<u32> = d.cols.iter().enumerate().filter(|x| *x.1).map(|(i, _)| d.range.c0 + i as u32).collect();
                app.checkpoint("Remove Duplicates");
                let s = app.sheet();
                let (removed, left) = sheet_engine::import::remove_duplicates(&mut app.book, s, d.range, &cols, d.header);
                app.changed();
                app.notify(&if removed == 0 { tr("No duplicate values found.") } else { trf("{} duplicate values removed; {} unique values remain.", &[&removed, &left]) });
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

// ---- Advanced Filter --------------------------------------------------------------------------

#[derive(Clone)]
pub struct Advanced { pub copy: bool, pub list: String, pub criteria: String, pub to: String, pub unique: bool }

pub fn open_advanced(app: &mut SheetApp) {
    let r = list_range(app);
    let list = ref_text(app, app.sheet(), r);
    open(app, Dlg::Advanced(Advanced { copy: false, list, criteria: String::new(), to: String::new(), unique: false }));
}

pub fn advanced_dialog(app: &mut SheetApp, ctx: &egui::Context, a: &mut Advanced) -> bool {
    let mut keep = true; let mut open_ = true;
    crate::theme::dialog(ctx, "Advanced Filter").open(&mut open_).resizable(false).default_width(360.0).show(ctx, |ui| {
        crate::theme::section(ui, "Action");
        crate::theme::radio_value(ui, &mut a.copy, false, tr("Filter the list, in-place"));
        crate::theme::radio_value(ui, &mut a.copy, true, tr("Copy to another location"));
        crate::theme::grid(ui, egui::Grid::new("adv").num_columns(2).spacing(vec2(8.0, 6.0)), |ui| {
            ui.label(tr("List range:")); crate::refedit::field(ui, &mut a.list, "data_tools.rs:88", 200.0); ui.end_row();
            ui.label(tr("Criteria range:")); crate::refedit::field(ui, &mut a.criteria, "data_tools.rs:89", 200.0); ui.end_row();
            ui.add_enabled(a.copy, egui::Label::new(tr("Copy to:"))); ui.add_enabled_ui(a.copy, |ui| crate::refedit::field(ui, &mut a.to, "data_tools.rs:90", 200.0)).inner; ui.end_row();
        });
        crate::theme::check(ui, &mut a.unique, tr("Unique records only"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let Some((ls, lr)) = parse_ref(app, &a.list) else { app.error = Some(tr("The list range is not valid.")); return };
                let crit = if a.criteria.trim().is_empty() { None } else { match parse_ref(app, &a.criteria) { Some(c) => Some(c), None => { app.error = Some(tr("The criteria range is not valid.")); return; } } };
                if a.copy {
                    let Some((ts, tr_)) = parse_ref(app, &a.to) else { app.error = Some(tr("The extract range is not valid.")); return };
                    app.checkpoint("Advanced Filter");
                    sheet_engine::import::advanced_copy(&mut app.book, ls, lr, crit, a.unique, ts, tr_.start());
                } else {
                    app.checkpoint("Advanced Filter");
                    let n = sheet_engine::import::advanced_in_place(&mut app.book, ls, lr, crit, a.unique);
                    app.notify(&trf("{} of {} records found", &[&n, &(lr.rows() - 1)]));
                }
                app.changed();
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

// ---- Consolidate ------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Consolidate { pub func: usize, pub reference: String, pub refs: Vec<String>, pub sel: Option<usize>, pub top: bool, pub left: bool }

pub fn open_consolidate(app: &mut SheetApp) { open(app, Dlg::Consolidate(Consolidate { func: 0, reference: String::new(), refs: vec![], sel: None, top: false, left: false })); }

pub fn consolidate_dialog(app: &mut SheetApp, ctx: &egui::Context, c: &mut Consolidate) -> bool {
    let mut keep = true; let mut open_ = true;
    crate::theme::dialog(ctx, "Consolidate").open(&mut open_).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.set_min_width(400.0);
        ui.label(tr("Function:"));
        crate::theme::Combo::from_id_salt("cons-fn").selected_text(tr(sheet_engine::consolidate::FUNCTIONS[c.func])).show_ui(ui, |ui| for (i, f) in sheet_engine::consolidate::FUNCTIONS.iter().enumerate() { ui.selectable_value(&mut c.func, i, tr(f)); });
        ui.label(tr("Reference:"));
        ui.horizontal(|ui| {
            crate::refedit::field(ui, &mut c.reference, "data_tools.rs:130", 260.0);
            if ui.button(tr("Use Selection")).clicked() { c.reference = ref_text(app, app.sheet(), app.sel.range()); }
            if ui.button(tr("Browse…")).clicked() { if let Some(p) = rfd::FileDialog::new().add_filter(tr("Excel Workbook"), &["xlsx"]).pick_file() { c.reference = format!("[{}]Sheet1!A1:D20", p.to_string_lossy()); } }
        });
        ui.horizontal(|ui| {
            if ui.button(tr("Add")).clicked() && !c.reference.trim().is_empty() { if !c.refs.contains(&c.reference) { c.refs.push(c.reference.trim().to_string()); } }
            if ui.add_enabled(c.sel.is_some(), egui::Button::new(tr("Delete"))).clicked() { if let Some(i) = c.sel.take() { if i < c.refs.len() { c.refs.remove(i); } } }
        });
        ui.label(tr("All references:"));
        egui::Frame::group(ui.style()).show(ui, |ui| { ui.set_min_size(vec2(380.0, 80.0)); for (i, r) in c.refs.iter().enumerate() { if ui.selectable_label(c.sel == Some(i), r).clicked() { c.sel = Some(i); c.reference = r.clone(); } } });
        crate::theme::section(ui, "Use labels in");
        crate::theme::check(ui, &mut c.top, tr("Top row"));
        crate::theme::check(ui, &mut c.left, tr("Left column"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, !c.refs.is_empty(), "OK").clicked() {
                let mut sources = vec![];
                for r in &c.refs {
                    if let Some(rest) = r.strip_prefix('[') {
                        // Another workbook: [path]Sheet!range.
                        let Some((path, tail)) = rest.split_once(']') else { continue };
                        match sheet_engine::open(std::path::Path::new(path)) {
                            Ok(wb) => { let (sh, rg) = tail.rsplit_once('!').unwrap_or(("", tail)); let si = wb.sheet_index(sh.trim_matches('\'')).unwrap_or(0); if let Some(rg) = parse_range(&rg.replace('$', "")) { sources.push(sheet_engine::consolidate::source(&wb, si, rg)); } }
                            Err(e) => { app.error = Some(e); return; }
                        }
                    } else if let Some((s, rg)) = parse_ref(app, r) { sources.push(sheet_engine::consolidate::source(&app.book, s, rg)); } else { app.error = Some(trf("Reference isn't valid: {}", &[r])); return; }
                }
                match sheet_engine::consolidate::consolidate(&sources, c.func, c.top, c.left) {
                    Some(rows) => { app.checkpoint("Consolidate"); let s = app.sheet(); sheet_engine::consolidate::put(&mut app.book, s, app.sel.active, &rows); app.changed(); keep = false; }
                    None => app.error = Some(tr("No data consolidated.")),
                }
            }
            if crate::theme::secondary(ui, "Close").clicked() { keep = false; }
        });
    });
    keep && open_
}

// ---- Subtotal -----------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Subtotal { pub range: Range, pub group: usize, pub func: usize, pub cols: Vec<bool>, pub replace: bool, pub breaks: bool, pub below: bool }

pub fn open_subtotal(app: &mut SheetApp) {
    let r = list_range(app);
    if r.rows() < 2 { app.error = Some(tr("Select a list with a header row and at least one row of data.")); return; }
    let n = r.cols() as usize;
    let mut cols = vec![false; n];
    // The last numeric column is the default, as Excel does.
    let s = &app.book.sheets[app.sheet()];
    if let Some(j) = (0..n).rev().find(|j| matches!(s.value(Pos::new(r.r0 + 1, r.c0 + *j as u32)), sheet_engine::Value::Num(_))) { cols[j] = true; } else { cols[n - 1] = true; }
    open(app, Dlg::Subtotal(Subtotal { range: r, group: 0, func: 0, cols, replace: true, breaks: false, below: true }));
}

pub fn subtotal_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut Subtotal) -> bool {
    let mut keep = true; let mut open_ = true;
    let names = headers(app, d.range, true);
    crate::theme::dialog(ctx, "Subtotal").open(&mut open_).resizable(false).default_width(320.0).show(ctx, |ui| {
        ui.set_min_width(300.0);
        ui.label(tr("At each change in:"));
        crate::theme::Combo::from_id_salt("sub-group").selected_text(names.get(d.group).cloned().unwrap_or_default()).show_ui(ui, |ui| for (i, n) in names.iter().enumerate() { ui.selectable_value(&mut d.group, i, n); });
        ui.label(tr("Use function:"));
        let funcs = sheet_engine::outline::FUNCTIONS;
        crate::theme::Combo::from_id_salt("sub-fn").selected_text(tr(funcs[d.func].1)).show_ui(ui, |ui| for (i, f) in funcs.iter().enumerate() { ui.selectable_value(&mut d.func, i, tr(f.1)); });
        ui.label(tr("Add subtotal to:"));
        egui::Frame::group(ui.style()).show(ui, |ui| egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| { ui.set_min_width(270.0); for (i, n) in names.iter().enumerate() { if let Some(c) = d.cols.get_mut(i) { crate::theme::check(ui, c, n); } } }));
        crate::theme::check(ui, &mut d.replace, tr("Replace current subtotals"));
        crate::theme::check(ui, &mut d.breaks, tr("Page break between groups"));
        crate::theme::check(ui, &mut d.below, tr("Summary below data"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::secondary(ui, "Remove All").clicked() {
                app.checkpoint("Remove Subtotals");
                let s = app.sheet();
                let r = sheet_engine::outline::remove_subtotals(&mut app.book, s, d.range);
                app.sel = crate::app::Selection { anchor: r.start(), active: r.end(), extra: vec![] };
                app.changed(); keep = false;
            }
            if crate::theme::primary(ui, "OK").clicked() {
                let spec = sheet_engine::outline::SubtotalSpec { group_col: d.range.c0 + d.group as u32, func: funcs[d.func].0, cols: d.cols.iter().enumerate().filter(|x| *x.1).map(|(i, _)| d.range.c0 + i as u32).collect(), replace: d.replace, page_breaks: d.breaks, below: d.below };
                app.checkpoint("Subtotal");
                let s = app.sheet();
                match sheet_engine::outline::subtotal(&mut app.book, s, d.range, &spec) {
                    Ok(r) => { app.sel = crate::app::Selection { anchor: r.start(), active: r.end(), extra: vec![] }; app.changed(); keep = false; }
                    Err(e) => app.error = Some(tr(&e)),
                }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

// ---- Data Form -----------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Form { pub range: Range, pub record: u32, pub values: Vec<String>, pub criteria: Option<Vec<String>>, pub new: bool }

fn load_record(app: &SheetApp, f: &mut Form) {
    let s = app.sheet();
    f.values = (f.range.c0..=f.range.c1).map(|c| if f.new { String::new() } else { app.book.edit_text(s, Pos::new(f.range.r0 + 1 + f.record, c)) }).collect();
}

pub fn open_form(app: &mut SheetApp) {
    let r = list_range(app);
    if r.rows() < 1 || r.cols() > 32 { app.error = Some(tr("Select a list of up to 32 columns with a header row.")); return; }
    let mut f = Form { range: r, record: 0, values: vec![], criteria: None, new: r.rows() < 2 };
    load_record(app, &mut f);
    open(app, Dlg::DataForm(f));
}

fn matches(app: &SheetApp, f: &Form, rec: u32, crit: &[String]) -> bool {
    let s = &app.book.sheets[app.sheet()];
    crit.iter().enumerate().all(|(j, c)| c.trim().is_empty() || {
        let v = s.value(Pos::new(f.range.r0 + 1 + rec, f.range.c0 + j as u32));
        let t = v.display().to_lowercase();
        let c = c.trim().to_lowercase();
        match (c.trim_start_matches(['>', '<', '=']).trim().parse::<f64>(), v.to_num()) {
            (Ok(n), Ok(x)) if !matches!(v, sheet_engine::Value::Str(_)) => if c.starts_with(">=") { x >= n } else if c.starts_with("<=") { x <= n } else if c.starts_with('>') { x > n } else if c.starts_with('<') { x < n } else { x == n },
            _ => sheet_engine::tables::wildcard(&format!("{c}*"), &t),
        }
    })
}

pub fn form_dialog(app: &mut SheetApp, ctx: &egui::Context, f: &mut Form) -> bool {
    let mut keep = true; let mut open_ = true;
    let names = headers(app, f.range, true);
    let total = f.range.rows().saturating_sub(1);
    let title = app.book.sheets[app.sheet()].name.clone();
    crate::theme::dialog(ctx, title).open(&mut open_).resizable(false).default_width(440.0).show(ctx, |ui| {
        ui.set_min_width(420.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(300.0);
                let editing = f.criteria.is_none();
                crate::theme::grid(ui, egui::Grid::new("form").num_columns(2).spacing(vec2(8.0, 4.0)), |ui| {
                    for (j, n) in names.iter().enumerate() {
                        ui.label(format!("{n}:"));
                        let formula = !f.new && editing && app.book.sheets[app.sheet()].cells.get(&Pos::new(f.range.r0 + 1 + f.record, f.range.c0 + j as u32)).is_some_and(|c| c.is_formula());
                        if let Some(c) = &mut f.criteria { ui.text_edit_singleline(&mut c[j]); }
                        else if formula { ui.label(app.book.display(app.sheet(), Pos::new(f.range.r0 + 1 + f.record, f.range.c0 + j as u32))); }
                        else { ui.text_edit_singleline(&mut f.values[j]); }
                        ui.end_row();
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label(if f.criteria.is_some() { tr("Criteria") } else if f.new { tr("New Record") } else { trf("{} of {}", &[&(f.record + 1), &total]) });
                let commit = |app: &mut SheetApp, f: &Form| {
                    let s = app.sheet();
                    let row = if f.new { f.range.r1 + 1 } else { f.range.r0 + 1 + f.record };
                    app.checkpoint("Data Form");
                    for (j, v) in f.values.iter().enumerate() {
                        let p = Pos::new(row, f.range.c0 + j as u32);
                        if app.book.sheets[s].cells.get(&p).is_some_and(|c| c.is_formula()) && !f.new { continue; }
                        if app.book.edit_text(s, p) != *v { app.book.set_input(s, p, v); }
                    }
                    app.changed();
                };
                if ui.button(tr("New")).clicked() && f.criteria.is_none() {
                    if f.new && f.values.iter().any(|v| !v.is_empty()) { commit(app, f); f.range.r1 += 1; }
                    f.new = true; load_record(app, f);
                }
                if ui.add_enabled(!f.new && f.criteria.is_none() && total > 0, egui::Button::new(tr("Delete"))).clicked() {
                    app.checkpoint("Data Form");
                    let s = app.sheet();
                    let row = f.range.r0 + 1 + f.record;
                    let below = Range { r0: row + 1, r1: f.range.r1, ..f.range };
                    if below.r0 <= below.r1 { let c = app.book.copy(s, below); app.book.paste(s, Range::cell(Pos::new(row, f.range.c0)), &c, Default::default()); }
                    app.book.clear(s, Range { r0: f.range.r1, r1: f.range.r1, ..f.range }, true, true, false);
                    f.range.r1 -= 1;
                    f.record = f.record.min(f.range.rows().saturating_sub(2));
                    app.changed();
                    load_record(app, f);
                }
                if ui.add_enabled(f.criteria.is_none(), egui::Button::new(tr("Restore"))).clicked() { load_record(app, f); }
                let step = |app: &mut SheetApp, f: &mut Form, fwd: bool| {
                    let crit = f.criteria.take();
                    let mut k = f.record as i64;
                    loop {
                        k += if fwd { 1 } else { -1 };
                        if k < 0 || k >= total as i64 { break; }
                        if crit.as_ref().is_none_or(|c| matches(app, f, k as u32, c)) { f.record = k as u32; break; }
                    }
                    f.new = false;
                    load_record(app, f);
                };
                if ui.button(tr("Find Prev")).clicked() { step(app, f, false); }
                if ui.button(tr("Find Next")).clicked() { step(app, f, true); }
                if ui.button(if f.criteria.is_some() { tr("Form") } else { tr("Criteria") }).clicked() {
                    f.criteria = if f.criteria.is_some() { None } else { Some(vec![String::new(); names.len()]) };
                }
                if ui.button(tr("Close")).clicked() {
                    if f.criteria.is_none() && (f.new && f.values.iter().any(|v| !v.is_empty()) || !f.new) { commit(app, f); }
                    keep = false;
                }
            });
        });
    });
    keep && open_
}

#[allow(dead_code)]
fn _unused() { let _ = parse_a1; }
