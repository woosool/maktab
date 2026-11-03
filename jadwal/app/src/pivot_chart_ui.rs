//! PivotCharts on screen: Insert ▸ PivotChart (for the PivotTable at the active cell, or a new
//! PivotTable and its chart), the field buttons drawn on the chart, and the item filter a
//! field button opens. The series follow the pivot through `sheet_engine::pivot_chart::sync`.

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Stroke, StrokeKind};
use sheet_engine::chart::{Chart, ChartKind};
use sheet_engine::drawing::{Anchor, ObjKind, Object};

pub fn menu(app: &mut SheetApp, ui: &mut egui::Ui) {
    if qalam_kit::widgets::menu_item(ui, "chartColumn", "PivotChart") { insert_for_active(app); ui.close(); }
    if qalam_kit::widgets::menu_item(ui, "pivotTable", "PivotChart & PivotTable") { with_new_pivot(app); ui.close(); }
}

/// A PivotChart of the pivot at the active cell.
pub fn insert_for_active(app: &mut SheetApp) {
    match crate::pivot_ui::pivot_at(app) {
        Some(k) => { let name = app.book.sheets[app.sheet()].pivots[k].name.clone(); insert(app, &name, ChartKind::Column); }
        None => app.notify(&tr("Select a cell in a PivotTable to insert a PivotChart, or use PivotChart & PivotTable.")),
    }
}

/// Inserts a PivotChart of pivot `name` beside it on the pivot's sheet.
pub fn insert(app: &mut SheetApp, name: &str, kind: ChartKind) -> Option<usize> {
    let (si, k) = app.book.sheets.iter().enumerate().find_map(|(si, s)| s.pivots.iter().position(|p| p.name == name).map(|k| (si, k)))?;
    app.book.active = si;
    let c = sheet_engine::pivot_chart::new_chart(&app.book, name, kind);
    let p = &app.book.sheets[si].pivots[k];
    let col = p.output.map_or(p.anchor.col + 3, |o| o.c1 + 2);
    let mut a = crate::objects::anchor_at_cell(app, 480.0, 288.0);
    if let Anchor::Two { from, to, .. } = &mut a { let w = to.col - from.col; from.col = col; to.col = col + w; let h = to.row - from.row; from.row = p.anchor.row; to.row = p.anchor.row + h; }
    Some(crate::objects::insert(app, Object::new(ObjKind::Chart(Box::new(c)), a), "Chart", "Insert PivotChart"))
}

#[derive(Clone)]
struct Pending { names: Vec<String>, since: f64 }
fn pending_id() -> egui::Id { egui::Id::new("pivot-chart-pending") }

/// PivotChart & PivotTable: Create PivotTable first; the chart follows when the pivot appears.
fn with_new_pivot(app: &mut SheetApp) {
    let names = app.book.sheets.iter().flat_map(|s| s.pivots.iter().map(|p| p.name.clone())).collect();
    let since = app.ctx.input(|i| i.time);
    app.ctx.data_mut(|m| m.insert_temp(pending_id(), Pending { names, since }));
    crate::pivot_ui::open_insert(app);
}

#[derive(Clone)]
struct Filter { si: usize, k: usize, field: usize, at: egui::Pos2, items: Vec<(String, bool)>, opened: u64 }
fn filter_id() -> egui::Id { egui::Id::new("pivot-chart-filter") }

pub fn frame(app: &mut SheetApp, ctx: &egui::Context) {
    if let Some(p) = ctx.data(|m| m.get_temp::<Pending>(pending_id())) {
        let new = app.book.sheets.iter().flat_map(|s| s.pivots.iter().map(|p| p.name.clone())).find(|n| !p.names.contains(n));
        if let Some(n) = new { ctx.data_mut(|m| m.remove::<Pending>(pending_id())); insert(app, &n, ChartKind::Column); }
        else if ctx.input(|i| i.time) - p.since > 600.0 { ctx.data_mut(|m| m.remove::<Pending>(pending_id())); }
    }
    let Some(mut f) = ctx.data(|m| m.get_temp::<Filter>(filter_id())) else { return };
    let mut keep = true;
    let area = egui::Area::new(egui::Id::new("pivot-chart-filter-area")).fixed_pos(f.at).order(egui::Order::Foreground).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_min_width(180.0);
            let all = f.items.iter().all(|i| i.1);
            let mut a = all;
            if ui.checkbox(&mut a, tr("(Select All)")).changed() { for i in &mut f.items { i.1 = a; } }
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| for (n, on) in &mut f.items { ui.checkbox(on, n.as_str()); });
            ui.horizontal(|ui| {
                if crate::theme::primary(ui, "OK").clicked() {
                    if f.items.iter().any(|i| i.1) && f.si < app.book.sheets.len() && f.k < app.book.sheets[f.si].pivots.len() {
                        let hidden: std::collections::BTreeSet<String> = f.items.iter().filter(|i| !i.1).map(|i| i.0.clone()).collect();
                        app.checkpoint("Filter");
                        let p = &mut app.book.sheets[f.si].pivots[f.k];
                        if hidden.is_empty() { p.hidden.remove(&f.field); } else { p.hidden.insert(f.field, hidden); }
                        crate::pivot_ui::rerun(app, f.si, f.k);
                    }
                    keep = false;
                }
                if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
            });
        });
    });
    let outside = ctx.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p)));
    if outside && ctx.cumulative_pass_nr() > f.opened + 1 { keep = false; }
    if keep { ctx.data_mut(|m| m.insert_temp(filter_id(), f)); } else { ctx.data_mut(|m| m.remove::<Filter>(filter_id())); }
}

/// The field buttons' rectangles: (label, field, rect).
fn buttons(app: &SheetApp, c: &Chart, r: Rect, z: f32) -> Vec<(String, Option<usize>, Rect)> {
    let Some(pv) = &c.pivot_source else { return vec![] };
    let list = sheet_engine::pivot_chart::field_buttons(&app.book, pv);
    let p = app.book.sheets.iter().flat_map(|s| s.pivots.iter()).find(|p| &p.name == pv);
    let axis: Vec<usize> = p.map(|p| p.rows.iter().chain(p.cols.iter()).copied().collect()).unwrap_or_default();
    let font = FontId::proportional(10.0 * z);
    let (mut xt, mut xb) = (r.min.x + 4.0 * z, r.min.x + 4.0 * z);
    let mut out = vec![];
    for (label, f) in list {
        let w = (label.chars().count() as f32 * 5.6 + 22.0) * z * font.size / (10.0 * z);
        let bottom = f.is_some_and(|f| axis.contains(&f));
        let (x, y) = if bottom { (&mut xb, r.max.y - 20.0 * z) } else { (&mut xt, r.min.y + 4.0 * z) };
        let rect = Rect::from_min_size(pos2(*x, y), vec2(w, 16.0 * z));
        *x += w + 4.0 * z;
        out.push((label, f, rect));
    }
    out
}

pub fn paint_buttons(app: &SheetApp, p: &egui::Painter, c: &Chart, r: Rect, z: f32) {
    if c.pivot_source.is_none() { return; }
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    for (label, f, b) in buttons(app, c, r, z) {
        p.rect_filled(b, 2.0, Color32::from_gray(0xEE));
        p.rect_stroke(b, 2.0, Stroke::new(1.0, Color32::from_gray(0xB0)), StrokeKind::Inside);
        let t = if f.is_some() { format!("{label} ▾") } else { label };
        p.text(pos2(b.min.x + 5.0 * z, b.center().y), Align2::LEFT_CENTER, t, FontId::proportional(10.0 * z), Color32::from_gray(0x30));
    }
}

/// A click on a field button opens its filter.
pub fn interact(app: &mut SheetApp, resp: &egui::Response, si: usize, i: usize, r: Rect, z: f32) -> bool {
    let Some(c) = app.book.sheets[si].objects[i].chart() else { return false };
    let Some(pv) = c.pivot_source.clone() else { return false };
    let Some(pt) = resp.interact_pointer_pos().or_else(|| resp.hover_pos()) else { return false };
    let Some((_, f, b)) = buttons(app, c, r, z).into_iter().find(|b| b.2.contains(pt)) else { return false };
    if resp.hovered() { resp.ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }
    if !resp.clicked() { return resp.drag_started() || resp.dragged(); }
    let Some(field) = f else { return true };
    let Some((ps, k)) = app.book.sheets.iter().enumerate().find_map(|(s, sh)| sh.pivots.iter().position(|p| p.name == pv).map(|k| (s, k))) else { return true };
    let p = &app.book.sheets[ps].pivots[k];
    let hidden = p.hidden.get(&field).cloned().unwrap_or_default();
    let mut q = p.clone();
    q.hidden.remove(&field);
    let items = q.items(&app.book, field).into_iter().map(|it| { let on = !hidden.contains(&it); (it, on) }).collect();
    app.ctx.data_mut(|m| m.insert_temp(filter_id(), Filter { si: ps, k, field, at: b.left_bottom(), items, opened: app.ctx.cumulative_pass_nr() }));
    true
}
