//! Where the Home tab reaches the features that live in their own modules (conditional
//! formatting, tables and filters, the selection pane), and Fill ▸ Series.

use crate::app::{Dialog, SheetApp};
use crate::i18n::tr;
use sheet_engine::addr::{Pos, Range};
use sheet_engine::Workbook;

pub fn cond_format_menu(app: &mut SheetApp, ui: &mut egui::Ui) { crate::condformat_ui::cond_format_menu(app, ui); }
pub fn format_as_table_menu(app: &mut SheetApp, ui: &mut egui::Ui) { crate::filter_ui::format_as_table_menu(app, ui); }
pub fn toggle_filter(app: &mut SheetApp) { crate::filter_ui::toggle_filter(app); }
pub fn clear_filters(app: &mut SheetApp) { crate::filter_ui::clear_filters(app); }
pub fn reapply_filters(app: &mut SheetApp) { crate::filter_ui::reapply(app); }
pub fn selection_pane(app: &mut SheetApp) { crate::arrange::toggle_pane(app); }
/// Find & Select ▸ Select Objects: dragging on the sheet selects objects with a rubber band.
pub fn select_objects(app: &mut SheetApp) { let mut st = crate::objects::state(&app.ctx); st.select_mode = !st.select_mode; crate::objects::set_state(&app.ctx, st); }

/// Cells with conditional formats (4) or data validation (5) inside `area`.
pub fn special_cells(book: &Workbook, sheet: usize, area: Range, kind: u8) -> Vec<Pos> {
    let s = &book.sheets[sheet];
    let ranges: Vec<Range> = match kind { 4 => s.cond_formats.iter().flat_map(|c| c.ranges.clone()).collect(), 5 => s.validations.iter().flat_map(|v| v.ranges.clone()).collect(), _ => return vec![] };
    let mut out: Vec<Pos> = ranges.iter().filter_map(|r| { let x = Range { r0: r.r0.max(area.r0), c0: r.c0.max(area.c0), r1: r.r1.min(area.r1), c1: r.c1.min(area.c1) }; (x.r0 <= x.r1 && x.c0 <= x.c1).then_some(x) })
        .flat_map(|x| { let x = Range { r1: x.r1.min(x.r0 + 10_000), c1: x.c1.min(x.c0 + 500), ..x }; x.iter().collect::<Vec<_>>() }).collect();
    out.sort(); out.dedup();
    out
}

/// The Series dialog's body; returns false when it closes.
pub fn series_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut Dialog, open: &mut bool) -> bool {
    let Dialog::Series { rows, kind, step, stop, unit, trend } = d else { return false };
    let mut keep = true;
    crate::theme::dialog(ctx, "Series").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| { crate::theme::section(ui, "Series in"); crate::theme::radio_value(ui, rows, true, tr("Rows")); crate::theme::radio_value(ui, rows, false, tr("Columns")); });
            ui.vertical(|ui| { crate::theme::section(ui, "Type"); for (i, l) in ["Linear", "Growth", "Date", "AutoFill"].iter().enumerate() { crate::theme::radio_value(ui, kind, i as u8, tr(l)); } });
            ui.add_enabled_ui(*kind == 2, |ui| { ui.vertical(|ui| { crate::theme::section(ui, "Date unit"); for (i, l) in ["Day", "Weekday", "Month", "Year"].iter().enumerate() { crate::theme::radio_value(ui, unit, i as u8, tr(l)); } }); });
        });
        crate::theme::check(ui, trend, tr("Trend"));
        crate::theme::hrow(ui, |ui| { ui.label(tr("Step value:")); ui.add(egui::DragValue::new(step).speed(0.1)); ui.label(tr("Stop value:")); ui.add(egui::TextEdit::singleline(stop).desired_width(80.0)); });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { fill_series(app, *rows, *kind, *step, stop.trim().parse().ok(), *unit, *trend); keep = false; }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep
}

fn fill_series(app: &mut SheetApp, rows: bool, kind: u8, step: f64, stop: Option<f64>, unit: u8, trend: bool) {
    let s = app.sheet();
    let r = app.sel.range();
    app.checkpoint("Series");
    if kind == 3 { let src = if rows { Range { c1: r.c0, ..r } } else { Range { r1: r.r0, ..r } }; app.book.fill(s, src, r, true); app.changed(); return; }
    let lines: Vec<(Pos, Vec<Pos>)> = if rows { (r.r0..=r.r1).map(|row| (Pos::new(row, r.c0), (r.c0 + 1..=r.c1).map(|c| Pos::new(row, c)).collect())).collect() } else { (r.c0..=r.c1).map(|c| (Pos::new(r.r0, c), (r.r0 + 1..=r.r1).map(|row| Pos::new(row, c)).collect())).collect() };
    for (first, rest) in lines {
        let sheet_engine::Value::Num(mut v) = app.book.sheets[s].value(first) else { continue };
        // A trend continues the step the first two cells show.
        let step = if trend { match app.book.sheets[s].value(*rest.first().unwrap_or(&first)) { sheet_engine::Value::Num(b) => b - v, _ => step } } else { step };
        for p in rest {
            v = match kind {
                1 => v * step,
                2 => match unit { 0 => v + step, 1 => { let mut x = v; let mut k = step.abs() as i64; while k > 0 { x += step.signum(); if sheet_engine::date::weekday(x, false) % 6 != 0 { k -= 1; } } x } 2 => sheet_engine::formula::functions::time::add_months(v, step as i64, false), _ => sheet_engine::formula::functions::time::add_months(v, 12 * step as i64, false) },
                _ => v + step,
            };
            if stop.is_some_and(|t| (step >= 0.0 && v > t) || (step < 0.0 && v < t)) { break; }
            app.book.set_cell_input(s, p, sheet_engine::Input::Value(sheet_engine::Value::Num(v)), None);
        }
    }
    app.changed();
}
