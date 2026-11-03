//! Timelines on screen (timeline.js, inserttimeline.js): Insert ▸ Timeline for the PivotTable
//! or table at the active cell, drawing a timeline, and picking periods — click a period, ⇧-click
//! or drag across periods for a range, the level menu (years, quarters, months, days), the
//! arrows to scroll the strip, and Clear Filter.

use crate::app::{rgb, SheetApp};
use crate::grid::SELECT;
use crate::i18n::tr;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind};
use sheet_engine::drawing::{Anchor, ObjKind, Object};
use sheet_engine::timeline::{self as tl, Level, Timeline};

const TILE: f32 = 38.0;

#[derive(Clone)]
struct InsertDlg { pivot: String, table: String, fields: Vec<(usize, String, bool)> }

fn dlg_id() -> egui::Id { egui::Id::new("jadwal-insert-timeline") }

/// Insert ▸ Timeline.
pub fn open_insert(app: &mut SheetApp) {
    let si = app.sheet();
    let p = app.sel.active;
    let (pivot, table, src) = if let Some(k) = crate::pivot_ui::pivot_at(app) {
        let pv = &app.book.sheets[si].pivots[k];
        (pv.name.clone(), String::new(), app.book.sheet_index(&pv.source_sheet).map(|s| (s, pv.source.c0, pv.source.c1, pv.source.r0 + 1, pv.source.r1)))
    } else if let Some(t) = app.book.sheets[si].tables.iter().find(|t| t.range.contains(p)) {
        (String::new(), t.name.clone(), Some((si, t.range.c0, t.range.c1, t.range.r0 + t.header_row as u32, t.range.r1 - t.totals_row as u32)))
    } else { (String::new(), String::new(), None) };
    let Some((ss, c0, c1, r0, r1)) = src else { app.notify(&tr("Select a cell in a PivotTable or table to insert a timeline.")); return };
    let fields: Vec<(usize, String, bool)> = (c0..=c1).filter(|&c| tl::is_date_column(&app.book, ss, c, r0, r1.max(r0)))
        .map(|c| (( c - c0) as usize, { let h = app.book.display(ss, sheet_engine::Pos::new(r0.saturating_sub(1), c)); if h.is_empty() { format!("Column{}", c - c0 + 1) } else { h } }, false)).collect();
    if fields.is_empty() { app.notify(&tr("There's no date field to filter by. A timeline needs a column of dates.")); return; }
    let mut d = InsertDlg { pivot, table, fields };
    if d.fields.len() == 1 { d.fields[0].2 = true; }
    app.ctx.data_mut(|m| m.insert_temp(dlg_id(), d));
}

/// The Insert Timelines dialog, when open.
pub fn frame(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = ctx.data(|m| m.get_temp::<InsertDlg>(dlg_id())) else { return };
    let mut keep = true;
    let mut open = true;
    crate::theme::dialog(ctx, "Insert Timelines").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(260.0);
        for (_, n, on) in &mut d.fields { ui.checkbox(on, n.as_str()); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let chosen: Vec<(usize, String)> = d.fields.iter().filter(|f| f.2).map(|f| (f.0, f.1.clone())).collect();
                for (n, (field, caption)) in chosen.into_iter().enumerate() { insert(app, &d.pivot, &d.table, field, &caption, n); }
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    if keep && open { ctx.data_mut(|m| m.insert_temp(dlg_id(), d)); } else { ctx.data_mut(|m| m.remove::<InsertDlg>(dlg_id())); }
}

/// Adds a timeline for `field` of the pivot (or table).
pub fn insert(app: &mut SheetApp, pivot: &str, table: &str, field: usize, caption: &str, n: usize) -> usize {
    let t = Timeline { pivot: pivot.into(), table: table.into(), field, caption: caption.into(), level: Level::Months, selection: None };
    let mut a = crate::objects::anchor_at_cell(app, 340.0, 120.0);
    if let Anchor::Two { from, to, .. } = &mut a { from.row += 2 + 7 * n as u32; to.row += 2 + 7 * n as u32; }
    if !table.is_empty() { if let Some((ts, ti)) = app.book.find_table(table) { if app.book.sheets[ts].tables[ti].filter.is_none() { app.book.sheets[ts].tables[ti].filter = Some(Default::default()); } } }
    crate::objects::insert(app, Object::new(ObjKind::Timeline(t), a), "Timeline", "Insert Timeline")
}

// ---- geometry ----

fn clear_rect(r: Rect, z: f32) -> Rect { Rect::from_min_size(pos2(r.max.x - 26.0 * z, r.min.y + 5.0 * z), vec2(18.0 * z, 18.0 * z)) }
fn level_rect(r: Rect, z: f32) -> Rect { Rect::from_min_max(pos2(r.max.x - 96.0 * z, r.min.y + 26.0 * z), pos2(r.max.x - 8.0 * z, r.min.y + 42.0 * z)) }
fn strip(r: Rect, z: f32) -> Rect { Rect::from_min_max(pos2(r.min.x + 22.0 * z, r.min.y + 46.0 * z), pos2(r.max.x - 22.0 * z, r.min.y + 100.0 * z)) }
fn arrows(r: Rect, z: f32) -> (Rect, Rect) {
    let s = strip(r, z);
    (Rect::from_min_max(pos2(r.min.x + 4.0 * z, s.min.y + 12.0 * z), pos2(s.min.x, s.min.y + 34.0 * z)), Rect::from_min_max(pos2(s.max.x, s.min.y + 12.0 * z), pos2(r.max.x - 4.0 * z, s.min.y + 34.0 * z)))
}
fn visible(r: Rect, z: f32) -> usize { (strip(r, z).width() / (TILE * z)).floor().max(1.0) as usize }

fn scroll_id(si: usize, i: usize) -> egui::Id { egui::Id::new(("timeline-scroll", si, i)) }

/// The first period shown: the stored scroll, else so the selection (or the end) shows.
fn first_shown(ctx: &egui::Context, app: &SheetApp, t: &Timeline, ps: &[tl::Period], vis: usize, key: Option<(usize, usize)>) -> usize {
    let max = ps.len().saturating_sub(vis);
    if let Some(v) = key.and_then(|(si, i)| ctx.data(|d| d.get_temp::<usize>(scroll_id(si, i)))) { return v.min(max); }
    let _ = app;
    match t.selection { Some((a, _)) => ps.iter().position(|p| p.end >= a).unwrap_or(0).min(max), None => 0 }
}

fn tile_rect(r: Rect, z: f32, k: usize) -> Rect { let s = strip(r, z); Rect::from_min_size(pos2(s.min.x + k as f32 * TILE * z, s.min.y + 12.0 * z), vec2(TILE * z, 22.0 * z)) }

// ---- drawing ----

fn with_key(app: &SheetApp, t: &Timeline) -> Option<(usize, usize)> {
    app.book.sheets.iter().enumerate().find_map(|(si, s)| s.objects.iter().position(|o| matches!(&o.kind, ObjKind::Timeline(x) if std::ptr::eq(x, t))).map(|i| (si, i)))
}

pub fn paint(app: &SheetApp, p: &egui::Painter, t: &Timeline, r: Rect, z: f32) {
    p.rect_filled(r, 2.0, Color32::WHITE);
    p.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_gray(0xBF)), StrokeKind::Inside);
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    let bold = FontId::new(13.0 * z, egui::FontFamily::Name("ui-bold".into()));
    let small = FontId::proportional(10.0 * z);
    let grey = Color32::from_gray(0x60);
    p.text(pos2(r.min.x + 8.0 * z, r.min.y + 14.0 * z), Align2::LEFT_CENTER, &t.caption, bold, Color32::from_gray(0x40));
    let cr = clear_rect(r, z);
    p.text(cr.center(), Align2::CENTER_CENTER, "⊘", FontId::proportional(13.0 * z), if t.selection.is_some() { SELECT } else { Color32::from_gray(0xC8) });
    p.text(pos2(r.min.x + 8.0 * z, r.min.y + 34.0 * z), Align2::LEFT_CENTER, tl::selection_text(&app.book, t), small.clone(), grey);
    let lr = level_rect(r, z);
    p.text(pos2(lr.max.x, lr.center().y), Align2::RIGHT_CENTER, format!("{} ▾", tr(t.level.label())), small.clone(), SELECT);
    let accent = app.book.theme_colors.get(4).copied().map(rgb).unwrap_or(SELECT);
    let ps = tl::periods(&app.book, t);
    if ps.is_empty() { p.text(strip(r, z).center(), Align2::CENTER_CENTER, tr("No dates"), small, grey); return; }
    let vis = visible(r, z);
    let first = first_shown(p.ctx(), app, t, &ps, vis, with_key(app, t));
    for k in 0..vis.min(ps.len() - first) {
        let per = &ps[first + k];
        let tr_ = tile_rect(r, z, k);
        let on = t.selection.is_none_or(|(a, b)| per.start <= b && per.end >= a);
        p.rect_filled(tr_.shrink2(vec2(1.0 * z, 0.0)), 0.0, if on && t.selection.is_some() { accent } else if on { accent.gamma_multiply(0.55) } else { Color32::from_gray(0xE6) });
        if k == 0 || ps[first + k - 1].year != per.year { p.text(pos2(tr_.min.x + 2.0 * z, tr_.min.y - 6.0 * z), Align2::LEFT_CENTER, per.year.to_string(), small.clone(), grey); }
        p.text(pos2(tr_.center().x, tr_.max.y + 9.0 * z), Align2::CENTER_CENTER, &per.label, FontId::proportional(9.0 * z), grey);
    }
    let (la, ra) = arrows(r, z);
    let can_l = first > 0;
    let can_r = first + vis < ps.len();
    p.text(la.center(), Align2::CENTER_CENTER, "‹", FontId::proportional(16.0 * z), if can_l { grey } else { Color32::from_gray(0xD8) });
    p.text(ra.center(), Align2::CENTER_CENTER, "›", FontId::proportional(16.0 * z), if can_r { grey } else { Color32::from_gray(0xD8) });
}

// ---- interaction ----

#[derive(Clone, Copy)]
struct DragSel { si: usize, i: usize, from: usize }
fn drag_id() -> egui::Id { egui::Id::new("timeline-drag") }

/// Sets object (si, i)'s selection and applies it (one undo step unless `quiet`).
pub fn set_selection(app: &mut SheetApp, si: usize, i: usize, sel: Option<(f64, f64)>, level: Option<Level>, quiet: bool) {
    let ObjKind::Timeline(mut t) = app.book.sheets[si].objects[i].kind.clone() else { return };
    if t.selection == sel && level.is_none_or(|l| l == t.level) { return; }
    if !quiet { app.checkpoint("Timeline"); }
    t.selection = sel;
    if let Some(l) = level { t.level = l; }
    app.book.sheets[si].objects[i].kind = ObjKind::Timeline(t.clone());
    app.book.sheets[si].objects[i].touch();
    let rerun = tl::apply(&mut app.book, &t);
    for (ps, pk) in rerun { crate::pivot_ui::rerun(app, ps, pk); }
    app.changed();
}

fn period_at(app: &SheetApp, t: &Timeline, r: Rect, z: f32, pt: Pos2, key: (usize, usize)) -> Option<usize> {
    let ps = tl::periods(&app.book, t);
    let vis = visible(r, z);
    let first = first_shown(&app.ctx, app, t, &ps, vis, Some(key));
    let s = strip(r, z);
    if !s.contains(pt) { return None; }
    let k = ((pt.x - s.min.x) / (TILE * z)).floor().max(0.0) as usize;
    (first + k < ps.len() && k < vis).then_some(first + k)
}

/// Clicks and drags on a timeline's controls; true when it used them (else the object moves).
pub fn interact(app: &mut SheetApp, ui: &egui::Ui, resp: &egui::Response, si: usize, i: usize, r: Rect, z: f32) -> bool {
    let ObjKind::Timeline(t) = app.book.sheets[si].objects[i].kind.clone() else { return false };
    let ctx = ui.ctx().clone();
    let ps = tl::periods(&app.book, &t);
    if let Some(d) = ctx.data(|m| m.get_temp::<DragSel>(drag_id())).filter(|d| d.si == si && d.i == i) {
        if let (Some(pt), Some(_)) = (resp.interact_pointer_pos(), ps.get(d.from)) {
            let s = strip(r, z);
            let pt = pos2(pt.x.clamp(s.min.x, s.max.x - 1.0), s.center().y);
            if let Some(k) = period_at(app, &t, r, z, pt, (si, i)) { let (a, b) = (d.from.min(k), d.from.max(k)); set_selection(app, si, i, Some((ps[a].start, ps[b].end)), None, true); }
        }
        if resp.drag_stopped() || !resp.dragged() { ctx.data_mut(|m| m.remove::<DragSel>(drag_id())); }
        return true;
    }
    let Some(pt) = resp.interact_pointer_pos().or_else(|| resp.hover_pos()) else { return false };
    let on_control = clear_rect(r, z).contains(pt) || level_rect(r, z).contains(pt) || strip(r, z).contains(pt) || arrows(r, z).0.contains(pt) || arrows(r, z).1.contains(pt);
    if !on_control { return false; }
    if resp.hovered() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }
    if resp.drag_started() {
        if let Some(k) = period_at(app, &t, r, z, pt, (si, i)) {
            ctx.data_mut(|m| m.insert_temp(drag_id(), DragSel { si, i, from: k }));
            app.checkpoint("Timeline");
            set_selection(app, si, i, Some((ps[k].start, ps[k].end)), None, true);
        }
        return true;
    }
    if resp.clicked() {
        if clear_rect(r, z).contains(pt) { set_selection(app, si, i, None, None, false); }
        else if level_rect(r, z).contains(pt) {
            let next = Level::ALL[(t.level as usize + 1) % 4];
            ctx.data_mut(|m| m.remove::<usize>(scroll_id(si, i)));
            set_selection(app, si, i, t.selection, Some(next), false);
        } else if arrows(r, z).0.contains(pt) || arrows(r, z).1.contains(pt) {
            let vis = visible(r, z);
            let first = first_shown(&ctx, app, &t, &ps, vis, Some((si, i)));
            let next = if arrows(r, z).0.contains(pt) { first.saturating_sub(vis.max(2) / 2) } else { (first + vis.max(2) / 2).min(ps.len().saturating_sub(vis)) };
            ctx.data_mut(|m| m.insert_temp(scroll_id(si, i), next));
        } else if let Some(k) = period_at(app, &t, r, z, pt, (si, i)) {
            let shift = ui.input(|x| x.modifiers.shift);
            let sel = match (shift, t.selection) { (true, Some((a, b))) => Some((a.min(ps[k].start), b.max(ps[k].end))), _ => Some((ps[k].start, ps[k].end)) };
            set_selection(app, si, i, sel, None, false);
        }
        return true;
    }
    false
}
