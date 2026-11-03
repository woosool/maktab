//! The outline bar (xlsx-outline.js, outline-detail.js, outline-settings.js): beside the row
//! headers and above the column headers when the sheet has groups, the level buttons (1, 2, 3…)
//! in the corner, a bracket per group and its +/- button on the summary line. The grid makes room
//! with `reserve` and draws it with `draw`.

use crate::app::SheetApp;
use crate::grid::Layout;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use sheet_engine::outline;

const STEP: f32 = 16.0;
const BOX: f32 = 11.0;

fn size(levels: u8) -> f32 { if levels == 0 { 0.0 } else { STEP * (levels as f32 + 1.0) + 4.0 } }

/// The grid's rectangle once the outline bar has its strips.
pub fn reserve(app: &SheetApp, full: Rect) -> Rect {
    let s = &app.book.sheets[app.sheet()];
    if !s.headers { return full; }
    let (w, h) = (size(s.outline.max_level(true)), size(s.outline.max_level(false)));
    Rect::from_min_max(full.min + vec2(w, h), full.max)
}

fn button(ui: &Ui, painter: &egui::Painter, r: Rect, text: &str, id: egui::Id) -> bool {
    let resp = ui.interact(r, id, Sense::click());
    let fill = if resp.hovered() { c_hover() } else { c_card() };
    painter.rect(r, 2.0, fill, Stroke::new(1.0, c_ink3()), egui::StrokeKind::Inside);
    painter.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(10.0), c_ink());
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn minus_plus(painter: &egui::Painter, r: Rect, plus: bool, hovered: bool) {
    painter.rect(r, 1.0, if hovered { c_hover() } else { c_card() }, Stroke::new(1.0, c_ink2()), egui::StrokeKind::Inside);
    let c = r.center();
    let s = Stroke::new(1.2, c_ink());
    painter.line_segment([pos2(c.x - 3.0, c.y), pos2(c.x + 3.0, c.y)], s);
    if plus { painter.line_segment([pos2(c.x, c.y - 3.0), pos2(c.x, c.y + 3.0)], s); }
}

/// Draws the bar and handles its clicks.
pub fn draw(app: &mut SheetApp, ui: &mut Ui, lay: &Layout) {
    let si = app.sheet();
    let o = app.book.sheets[si].outline.clone();
    if !app.book.sheets[si].headers || o.is_empty() { return; }
    let (lr, lc) = (o.max_level(true), o.max_level(false));
    let (w, h) = (size(lr), size(lc));
    let top = lay.rect.min.y + lay.head_h;
    let left = lay.rect.min.x + lay.head_w;
    let mut action: Option<Box<dyn FnOnce(&mut SheetApp)>> = None;
    let bg = Color32::from_gray(if app.dark { 40 } else { 246 });
    for rows in [true, false] {
        let levels = if rows { lr } else { lc };
        if levels == 0 { continue; }
        // The strip, and the band of level buttons next to the other headers.
        let (strip, band) = if rows {
            (Rect::from_min_max(pos2(lay.rect.min.x - w, top), pos2(lay.rect.min.x, lay.rect.max.y)), Rect::from_min_max(pos2(lay.rect.min.x - w, lay.rect.min.y), pos2(lay.rect.min.x, top)))
        } else {
            (Rect::from_min_max(pos2(left, lay.rect.min.y - h), pos2(lay.rect.max.x, lay.rect.min.y)), Rect::from_min_max(pos2(lay.rect.min.x, lay.rect.min.y - h), pos2(left, lay.rect.min.y)))
        };
        let painter = ui.painter_at(strip.union(band));
        painter.rect_filled(strip.union(band), 0.0, bg);
        for k in 1..=levels + 1 {
            let c = if rows { pos2(band.min.x + 4.0 + STEP * (k as f32 - 0.5), band.center().y) } else { pos2(band.center().x, band.min.y + 4.0 + STEP * (k as f32 - 0.5)) };
            let r = Rect::from_center_size(c, vec2(13.0, 13.0));
            if button(ui, &painter, r, &k.to_string(), egui::Id::unique(("outline-level", rows, k))) {
                action = Some(Box::new(move |app: &mut SheetApp| { let s = app.sheet(); app.checkpoint("Show Level"); outline::show_level(&mut app.book.sheets[s], rows, k); app.dirty = true; }));
            }
        }
        let p = painter.with_clip_rect(strip);
        let stroke = Stroke::new(1.0, c_ink2());
        let sheet = &app.book.sheets[si];
        let hidden = |i: u32| if rows { sheet.hidden_rows.contains(&i) } else { sheet.hidden_cols.contains(&i) };
        let pos = |i: u32| if rows { lay.y(i) } else { lay.x(i) };
        let len = |i: u32| if rows { lay.rows.size(i) * lay.z } else { lay.cols.size(i) * lay.z };
        let (lo, hi) = if rows { (strip.min.y, strip.max.y) } else { (strip.min.x, strip.max.x) };
        for level in 1..=levels {
            let across = (if rows { strip.min.x } else { strip.min.y }) + 4.0 + STEP * (level as f32 - 0.5);
            for g in o.groups(rows, level) {
                let a = pos(g.0); let b = pos(g.1) + len(g.1);
                let summary = o.summary_of(rows, g);
                let shut = summary.is_some_and(|m| o.collapsed(rows).contains(&m));
                let all_hidden = (g.0..=g.1).all(hidden);
                // A group inside a collapsed parent isn't shown at all.
                if all_hidden && !shut { continue; }
                if b < lo - 50.0 || a > hi + 50.0 { continue; }
                if !all_hidden && b > a + 1.0 {
                    let (s0, s1) = if o.below(rows) { (a + 2.0, b) } else { (a, b - 2.0) };
                    if rows { p.line_segment([pos2(across, s0), pos2(across, s1)], stroke); } else { p.line_segment([pos2(s0, across), pos2(s1, across)], stroke); }
                    let tick = if o.below(rows) { a + 2.0 } else { b - 2.0 };
                    if rows { p.line_segment([pos2(across, tick), pos2(across + 4.0, tick)], stroke); } else { p.line_segment([pos2(tick, across), pos2(tick, across + 4.0)], stroke); }
                }
                let Some(m) = summary else { continue };
                if hidden(m) { continue; }
                let mid = pos(m) + len(m) / 2.0;
                let r = if rows { Rect::from_center_size(pos2(across, mid), vec2(BOX, BOX)) } else { Rect::from_center_size(pos2(mid, across), vec2(BOX, BOX)) };
                if !strip.intersects(r) { continue; }
                let resp = ui.interact(r, egui::Id::unique(("outline-box", rows, level, g.0)), Sense::click());
                minus_plus(&p, r, shut, resp.hovered());
                let tip = if shut { "Show Detail" } else { "Hide Detail" };
                if resp.on_hover_text(crate::i18n::tr(tip)).clicked() {
                    action = Some(Box::new(move |app: &mut SheetApp| { let s = app.sheet(); app.checkpoint(if shut { "Show Detail" } else { "Hide Detail" }); outline::set_collapsed(&mut app.book.sheets[s], rows, level, g, !shut); app.dirty = true; }));
                }
            }
        }
    }
    if let Some(f) = action { f(app); }
}

/// Whole rows (or whole columns) selected: what Group acts on without asking.
pub fn whole(app: &SheetApp) -> Option<bool> {
    let r = app.sel.range();
    if r.c0 == 0 && r.c1 >= sheet_engine::addr::MAX_COLS - 1 { Some(true) } else if r.r0 == 0 && r.r1 >= sheet_engine::addr::MAX_ROWS - 1 { Some(false) } else { None }
}

/// Group or Ungroup the selection's rows or columns.
pub fn group(app: &mut SheetApp, rows: bool, ungroup: bool) {
    let r = app.sel.range();
    let (a, b) = if rows { (r.r0, r.r1) } else { (r.c0, r.c1) };
    let s = app.sheet();
    app.checkpoint(if ungroup { "Ungroup" } else { "Group" });
    if ungroup {
        if !outline::ungroup(&mut app.book.sheets[s], rows, a, b) { app.notify(&crate::i18n::tr("Cannot ungroup. There are no groups in the selection.")); }
    } else if let Err(e) = outline::group(&mut app.book.sheets[s], rows, a, b) { app.error = Some(crate::i18n::tr(&e)); }
    app.dirty = true;
}
