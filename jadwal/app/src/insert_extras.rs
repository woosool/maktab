//! Hooks the Insert tab's timeline, PivotChart, map, page view and scripts modules need from the
//! app: per-frame windows, side panels, and work after each change.

use crate::app::SheetApp;

/// Side panels (before the grid's central panel).
pub fn panels(app: &mut SheetApp, ui: &mut egui::Ui) { crate::scripts_ui::panel(app, ui); }

/// Windows and popups, and keeping pivots in step with their timelines (also after opening a
/// file or undoing: a derived refresh that doesn't mark the workbook changed).
pub fn frame(app: &mut SheetApp, ctx: &egui::Context) {
    crate::timeline_ui::frame(app, ctx);
    crate::pivot_chart_ui::frame(app, ctx);
    crate::maps_ui::frame(app, ctx);
    let rerun = sheet_engine::timeline::sync_pivots(&mut app.book);
    if !rerun.is_empty() {
        let dirty = app.dirty;
        for (si, k) in rerun { let mut p = app.book.sheets[si].pivots[k].clone(); p.apply(&mut app.book, si); app.book.sheets[si].pivots[k] = p; }
        app.book.recalc();
        sheet_engine::pivot_chart::sync(&mut app.book);
        app.dirty = dirty;
    }
}

/// After every change (SheetApp::changed).
pub fn after_change(app: &mut SheetApp) {
    sheet_engine::pivot_chart::sync(&mut app.book);
    crate::maps_ui::after_change(app);
    crate::scripts_ui::after_change(app);
}
