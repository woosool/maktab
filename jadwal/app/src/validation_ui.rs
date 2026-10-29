//! Data validation (ui/validation.js, validation-alert.js, listpicker.js): the Data Validation
//! dialog (Settings, Input Message, Error Alert), the alert when an entry breaks a rule, the
//! in-cell list's drop-down, the input message by the active cell, and Circle Invalid Data.

use crate::app::{Editor, Selection, SheetApp};
use crate::grid::Layout;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use qalam_kit::colors::*;
use sheet_engine::addr::Pos;
use sheet_engine::condformat::Op;
use sheet_engine::validation::{self, Alert, AlertStyle, DvKind, Validation};

#[derive(Clone)]
pub struct Dialog { tab: usize, v: Validation, f1: String, f2: String }

/// An entry that broke a rule, waiting for the alert's answer.
#[derive(Clone)]
pub struct Pending { editor: Editor, mv: Option<(i64, i64)>, alert: Alert }

#[derive(Clone, Default)]
pub struct State { dialog: Option<Dialog>, pending: Option<Pending>, bypass: bool, list: Option<(usize, Pos)>, circles: Vec<(usize, Pos)> }

fn id() -> egui::Id { egui::Id::new("jadwal-validation") }
fn state(ctx: &egui::Context) -> State { ctx.data(|d| d.get_temp::<State>(id()).unwrap_or_default()) }
fn set_state(ctx: &egui::Context, s: State) { ctx.data_mut(|d| d.insert_temp(id(), s)); }

/// A bound as the dialog shows it: a number (a date or time for those kinds) or =formula.
fn shown(book: &sheet_engine::Workbook, kind: DvKind, f: &str) -> String {
    match (kind, f.parse::<f64>()) {
        (DvKind::Date, Ok(n)) => sheet_engine::numfmt::format(&sheet_engine::Value::Num(n), "m/d/yyyy", book.date1904).text,
        (DvKind::Time, Ok(n)) => sheet_engine::numfmt::format(&sheet_engine::Value::Num(n), "h:mm:ss AM/PM", book.date1904).text,
        _ => crate::condformat_ui::formula_value(f),
    }
}

/// What the dialog's bound box holds, as the rule stores it.
fn stored(book: &sheet_engine::Workbook, t: &str) -> String {
    let t = t.trim();
    if !t.starts_with('=') { if let Some((n, _)) = sheet_engine::numfmt::parse_input(t, book.date1904) { return sheet_engine::value::fmt_num(n); } }
    crate::condformat_ui::value_formula(t)
}

/// Opens the Data Validation dialog for the selection.
pub fn open_validation(app: &mut SheetApp) {
    app.commit(None);
    let s = &app.book.sheets[app.sheet()];
    let v = validation::at(s, app.sel.active).cloned().unwrap_or_default();
    let f1 = if v.kind == DvKind::List { v.source_text() } else { shown(&app.book, v.kind, &v.f1) };
    let f2 = shown(&app.book, v.kind, &v.f2);
    let mut st = state(&app.ctx);
    st.dialog = Some(Dialog { tab: 0, v, f1, f2 });
    set_state(&app.ctx, st);
}

/// The Data Validation button's menu.
pub fn validation_menu(app: &mut SheetApp, ui: &mut Ui) {
    if ui.button(tr("Data Validation…")).clicked() { open_validation(app); ui.close(); }
    if ui.button(tr("Circle Invalid Data")).clicked() {
        let si = app.sheet();
        let cells = validation::invalid_cells(&app.book, si, 255);
        if cells.is_empty() { app.notify(&tr("No cells break their validation rules.")); }
        let mut st = state(&app.ctx);
        st.circles = cells.into_iter().map(|p| (si, p)).collect();
        set_state(&app.ctx, st);
        ui.close();
    }
    if ui.button(tr("Clear Validation Circles")).clicked() { let mut st = state(&app.ctx); st.circles.clear(); set_state(&app.ctx, st); ui.close(); }
}

/// Checks a committed entry; true when it broke a rule (the alert then decides what happens).
pub fn check_entry(app: &mut SheetApp, e: &Editor, text: &str, mv: Option<(i64, i64)>) -> bool {
    let mut st = state(&app.ctx);
    if std::mem::take(&mut st.bypass) { set_state(&app.ctx, st); return false; }
    let Err(alert) = validation::validate(&app.book, e.sheet, e.pos, text) else { return false };
    st.pending = Some(Pending { editor: Editor { request_focus: true, ..e.clone() }, mv, alert });
    set_state(&app.ctx, st);
    true
}

/// The dialogs: Data Validation and the alert for a broken rule.
pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let mut st = state(ctx);
    if let Some(p) = st.pending.take() {
        set_state(ctx, st);
        alert(app, ctx, p);
        return;
    }
    let Some(mut d) = st.dialog.take() else { return };
    if dialog(app, ctx, &mut d) { st.dialog = Some(d); }
    set_state(ctx, st);
}

fn alert(app: &mut SheetApp, ctx: &egui::Context, p: Pending) {
    let a = &p.alert;
    let title = if a.title.is_empty() { crate::app_name() } else { a.title.clone() };
    // 0 = keep waiting, 1 = retry, 2 = accept, 3 = drop the entry.
    let mut answer = 0;
    crate::theme::dialog(ctx, title).resizable(false).show(ctx, |ui| {
        ui.set_max_width(420.0);
        crate::theme::hrow(ui, |ui| {
            let (glyph, color) = match a.style { AlertStyle::Stop => ("✕", Color32::from_rgb(0xD1, 0x34, 0x38)), AlertStyle::Warning => ("!", Color32::from_rgb(0xE0, 0x9A, 0x00)), AlertStyle::Information => ("i", Color32::from_rgb(0x1E, 0x6F, 0xD9)) };
            let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 15.0, color);
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(18.0), Color32::WHITE);
            ui.vertical(|ui| {
                ui.label(tr(&a.message));
                if a.style == AlertStyle::Warning { ui.add_space(4.0); ui.label(tr("Continue?")); }
            });
        });
        crate::theme::footer(ui, |ui| match a.style {
            AlertStyle::Stop => {
                if crate::theme::primary(ui, "Retry").clicked() { answer = 1; }
                if crate::theme::secondary(ui, "Cancel").clicked() { answer = 3; }
            }
            AlertStyle::Warning => {
                if crate::theme::primary(ui, "Yes").clicked() { answer = 2; }
                if crate::theme::secondary(ui, "No").clicked() { answer = 1; }
                if crate::theme::secondary(ui, "Cancel").clicked() { answer = 3; }
            }
            AlertStyle::Information => {
                if crate::theme::primary(ui, "OK").clicked() { answer = 2; }
                if crate::theme::secondary(ui, "Cancel").clicked() { answer = 3; }
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { answer = 3; }
    let e = p.editor.clone();
    match answer {
        0 => { let mut st = state(ctx); st.pending = Some(p); set_state(ctx, st); }
        1 => { app.book.active = e.sheet; app.sel = Selection::at(e.pos); app.reveal = true; app.edit = Some(e); }
        2 => {
            let mut st = state(ctx); st.bypass = true; set_state(ctx, st);
            app.book.active = e.sheet;
            app.edit = Some(e);
            app.commit(p.mv);
        }
        _ => {}
    }
}

fn dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut Dialog) -> bool {
    let mut open = true;
    let mut keep = true;
    crate::theme::dialog(ctx, "Data Validation").open(&mut open).resizable(false).default_width(460.0).show(ctx, |ui| {
        ui.set_min_width(440.0);
        ui.set_min_height(260.0);
        crate::theme::hrow(ui, |ui| for (i, t) in ["Settings", "Input Message", "Error Alert"].iter().enumerate() { ui.selectable_value(&mut d.tab, i, tr(t)); });
        ui.separator();
        let v = &mut d.v;
        match d.tab {
            0 => {
                crate::theme::section(ui, "Validation criteria");
                crate::theme::hrow(ui, |ui| {
                    ui.label(tr("Allow:"));
                    let before = v.kind;
                    crate::theme::Combo::from_id_salt("dv-kind").width(160.0).selected_text(tr(v.kind.label())).show_ui(ui, |ui| for (k, _, l) in DvKind::ALL { ui.selectable_value(&mut v.kind, k, tr(l)); });
                    if v.kind != before { d.f1.clear(); d.f2.clear(); }
                    if v.kind != DvKind::Any { crate::theme::check(ui, &mut v.allow_blank, tr("Ignore blank")); }
                });
                if v.kind.has_op() {
                    crate::theme::hrow(ui, |ui| {
                        ui.label(tr("Data:"));
                        crate::theme::Combo::from_id_salt("dv-op").width(200.0).selected_text(tr(v.op.label())).show_ui(ui, |ui| for (o, _, l) in Op::ALL { ui.selectable_value(&mut v.op, o, tr(l)); });
                    });
                    let (a, b) = match (v.kind, v.op.two()) { (DvKind::Date, true) => ("Start date:", "End date:"), (DvKind::Time, true) => ("Start time:", "End time:"), (DvKind::Date, false) => ("Date:", ""), (DvKind::Time, false) => ("Time:", ""), (DvKind::TextLength, false) => ("Length:", ""), (_, true) => ("Minimum:", "Maximum:"), _ => ("Value:", "") };
                    ui.label(tr(a)); ui.add(egui::TextEdit::singleline(&mut d.f1).desired_width(300.0));
                    if v.op.two() { ui.label(tr(b)); ui.add(egui::TextEdit::singleline(&mut d.f2).desired_width(300.0)); }
                }
                match v.kind {
                    DvKind::List => {
                        crate::theme::check(ui, &mut v.dropdown, tr("In-cell dropdown"));
                        ui.label(tr("Source:"));
                        ui.add(egui::TextEdit::singleline(&mut d.f1).desired_width(300.0).hint_text(tr("Yes,No,Maybe or =$A$1:$A$5")));
                    }
                    DvKind::Custom => { ui.label(tr("Formula:")); if d.f1.is_empty() { d.f1.push('='); } ui.add(egui::TextEdit::singleline(&mut d.f1).desired_width(300.0).font(egui::TextStyle::Monospace)); }
                    _ => {}
                }
            }
            1 => {
                crate::theme::check(ui, &mut v.show_input, tr("Show input message when cell is selected"));
                crate::theme::section(ui, "When cell is selected, show this input message:");
                ui.label(tr("Title:")); ui.add(egui::TextEdit::singleline(&mut v.prompt_title).desired_width(300.0).char_limit(32));
                ui.label(tr("Input message:")); ui.add(egui::TextEdit::multiline(&mut v.prompt).desired_width(400.0).desired_rows(4).char_limit(255));
            }
            _ => {
                crate::theme::check(ui, &mut v.show_error, tr("Show error alert after invalid data is entered"));
                crate::theme::section(ui, "When user enters invalid data, show this error alert:");
                crate::theme::hrow(ui, |ui| {
                    ui.label(tr("Style:"));
                    let label = |s: AlertStyle| tr(match s { AlertStyle::Stop => "Stop", AlertStyle::Warning => "Warning", AlertStyle::Information => "Information" });
                    crate::theme::Combo::from_id_salt("dv-style").width(140.0).selected_text(label(v.style)).show_ui(ui, |ui| for s in [AlertStyle::Stop, AlertStyle::Warning, AlertStyle::Information] { ui.selectable_value(&mut v.style, s, label(s)); });
                });
                ui.label(tr("Title:")); ui.add(egui::TextEdit::singleline(&mut v.error_title).desired_width(300.0).char_limit(32));
                ui.label(tr("Error message:")); ui.add(egui::TextEdit::multiline(&mut v.error).desired_width(400.0).desired_rows(4).char_limit(255));
            }
        }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let mut v = d.v.clone();
                match v.kind {
                    DvKind::List => v.set_source(&d.f1),
                    DvKind::Custom => { v.f1 = d.f1.trim().trim_start_matches('=').to_string(); v.f2.clear(); }
                    DvKind::Any => { v.f1.clear(); v.f2.clear(); }
                    _ => { v.f1 = stored(&app.book, &d.f1); v.f2 = if v.op.two() { stored(&app.book, &d.f2) } else { String::new() }; }
                }
                let ranges = app.sel.ranges();
                let si = app.sheet();
                app.checkpoint("Data Validation");
                let s = &mut app.book.sheets[si];
                for r in &ranges { s.clear_validation_in(*r); }
                if v.kind != DvKind::Any || !v.prompt.is_empty() || !v.prompt_title.is_empty() { v.ranges = ranges; s.validations.push(v); }
                app.dirty = true;
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| if crate::theme::secondary(ui, "Clear All").clicked() { d.v = Validation::default(); d.f1.clear(); d.f2.clear(); });
        });
    });
    keep && open
}

/// What the grid adds for validation: the active cell's list arrow and its list, the input
/// message beside it, and circles around invalid data.
pub fn overlay(app: &mut SheetApp, ui: &mut Ui, lay: &Layout) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    let mut st = state(ui.ctx());
    if s.validations.is_empty() && st.circles.is_empty() && st.list.is_none() { return; }
    let p = ui.painter_at(lay.view());
    let z = lay.z;
    // Circles, while the cell is still invalid.
    let before = st.circles.len();
    st.circles.retain(|(sh, q)| !validation::is_valid(&app.book, *sh, *q));
    for (_, q) in st.circles.iter().filter(|c| c.0 == si) {
        let r = lay.cell_rect(*q).expand(2.0);
        let pts: Vec<egui::Pos2> = (0..32).map(|k| { let a = k as f32 / 32.0 * std::f32::consts::TAU; r.center() + vec2(a.cos() * r.width() / 2.0, a.sin() * r.height() / 2.0) }).collect();
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.5 * z.max(1.0), Color32::from_rgb(0xE0, 0x10, 0x10))));
    }
    let changed = before != st.circles.len();
    let at = app.sel.active;
    if st.list.is_some_and(|l| l != (si, at)) { st.list = None; }
    let Some(v) = validation::at(s, at).cloned() else { if changed || st.list.is_some() { st.list = None; set_state(ui.ctx(), st); } return };
    let rc = s.merge_at(at).map(|m| lay.range_rect(m)).unwrap_or_else(|| lay.cell_rect(at));
    let editing_elsewhere = app.edit.as_ref().is_some_and(|e| e.pos != at || e.sheet != si);
    // The drop-down arrow, inside the cell's right edge.
    let mut arrow = Rect::NOTHING;
    if v.kind == DvKind::List && v.dropdown && !editing_elsewhere && lay.view().contains(rc.center()) {
        let h = rc.height().min(18.0 * z).max(10.0);
        arrow = Rect::from_min_size(pos2(if lay.rtl { rc.min.x } else { rc.max.x - h }, rc.max.y - h), vec2(h, h));
        let hot = ui.rect_contains_pointer(arrow);
        p.rect(arrow, 2.0, if hot { c_hover() } else { c_card() }, Stroke::new(1.0, c_line()), StrokeKind::Inside);
        let c = arrow.center();
        let k = h * 0.18;
        p.add(egui::Shape::convex_polygon(vec![c + vec2(-k * 1.4, -k * 0.6), c + vec2(k * 1.4, -k * 0.6), c + vec2(0.0, k * 1.0)], c_ink2(), Stroke::NONE));
        if ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| arrow.contains(q))) {
            st.list = if st.list.is_some() { None } else { Some((si, at)) };
        }
        if ui.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowDown)) && app.edit.is_none() { st.list = Some((si, at)); }
    }
    // The list.
    let mut pick = None;
    if st.list == Some((si, at)) {
        let items = validation::list_items(&app.book, si, at).unwrap_or_default();
        let current = app.book.display(si, at);
        let area = egui::Area::new(egui::Id::new("dv-list")).order(egui::Order::Foreground).fixed_pos(pos2(if lay.rtl { rc.max.x - rc.width().max(120.0) } else { rc.min.x }, rc.max.y + 1.0)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).fill(c_card()).stroke(Stroke::new(1.0, c_line())).inner_margin(2).show(ui, |ui| {
                ui.set_min_width(rc.width().max(120.0));
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for it in &items { if ui.selectable_label(*it == current, it).clicked() { pick = Some(it.clone()); } }
                    if items.is_empty() { ui.label(RichText::new(tr("The list is empty.")).color(c_ink3())); }
                });
            });
        });
        let elsewhere = area.response.clicked_elsewhere() && !ui.input(|i| i.pointer.interact_pos().is_some_and(|q| arrow.contains(q)));
        if elsewhere || ui.input(|i| i.key_pressed(egui::Key::Escape)) { st.list = None; }
    } else if v.show_input && (!v.prompt.is_empty() || !v.prompt_title.is_empty()) && !lay.view().is_negative() && lay.view().contains(rc.center()) {
        // The input message, below and beside the cell.
        let w = 220.0;
        let font = egui::FontId::proportional(12.0);
        let title = ui.painter().layout(v.prompt_title.clone(), egui::FontId::new(12.0, egui::FontFamily::Name("ui-bold".into())), Color32::BLACK, w - 12.0);
        let body = ui.painter().layout(v.prompt.clone(), font, Color32::from_gray(0x20), w - 12.0);
        let h = title.size().y + body.size().y + 14.0;
        let x = if lay.rtl { rc.min.x - w + rc.width() * 0.5 } else { rc.min.x + rc.width() * 0.5 };
        let r = Rect::from_min_size(pos2(x, rc.max.y + 6.0), vec2(w, h));
        let fp = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("dv-input")));
        fp.rect(r, 2.0, Color32::from_rgb(0xFF, 0xFF, 0xE1), Stroke::new(1.0, Color32::from_gray(0x90)), StrokeKind::Inside);
        let ty = r.min.y + 6.0;
        fp.galley(pos2(r.min.x + 6.0, ty), title.clone(), Color32::BLACK);
        fp.galley(pos2(r.min.x + 6.0, ty + title.size().y + 2.0), body, Color32::BLACK);
    }
    set_state(ui.ctx(), st);
    if let Some(t) = pick {
        let mut st = state(ui.ctx()); st.list = None; set_state(ui.ctx(), st);
        app.begin_edit(Some(t), false);
        app.commit(None);
    }
}
