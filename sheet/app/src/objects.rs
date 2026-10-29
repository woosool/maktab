//! Floating objects over the grid (pictures.js, chart selection in grid.js): charts, pictures,
//! shapes and text boxes, slicers — drawn at their anchors after the cells, selected with a
//! click, moved by dragging, resized with eight handles, deleted, reordered; plus Insert
//! Picture, Text Box and Shapes, and the hooks the window calls (side panel and dialogs).

use crate::app::{rgb, SheetApp};
use crate::grid::{Layout, SELECT};
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, CursorIcon, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui};
use qalam_kit::colors::*;
use sheet_engine::drawing::{add_object, Anchor, ObjKind, Object, Shape};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// The object layer's interface state, kept in egui's memory (not in the workbook).
#[derive(Clone, Default)]
pub struct State {
    /// The selected object: (sheet, index into `Sheet.objects`).
    pub selected: Option<(usize, usize)>,
    /// A copied (or cut) object, pasted by ⌘V until cells are copied.
    pub clip: Option<Object>,
    drag: Option<Drag>,
    pub dialog: Option<ObjDialog>,
    /// The PivotTable Fields pane: Some(false) when closed by hand for the pivot at the cursor.
    pub pane_closed: Option<String>,
    /// More selected objects (with `selected`, on its sheet): Shift/⌘-click and rubber bands.
    pub multi: Vec<usize>,
    /// A rubber band being dragged: where it started, and the cell selection to put back.
    pub band: Option<(Pos2, crate::app::Selection)>,
    /// Find & Select ▸ Select Objects: dragging on the sheet draws a rubber band.
    pub select_mode: bool,
    /// The Selection Pane, Snap to Grid, Snap to Shape, and what Regroup puts back together (ids).
    pub pane: bool,
    pub snap_grid: bool,
    pub snap_shape: bool,
    pub regroup: Vec<u32>,
}

#[derive(Clone)]
struct Drag { handle: u8, start: Rect, size: (f32, f32), grab: Pos2, others: Vec<(usize, [f32; 4])>, origin: [f32; 4] }

#[derive(Clone)]
pub enum ObjDialog {
    EditText { sheet: usize, idx: usize, text: String },
    SelectData(crate::chart_ui::SelectData),
    ChartType(crate::chart_ui::ChartTypeDlg),
    FormatSeries(crate::chart_ui::FormatSeries),
    InsertPivot(crate::pivot_ui::InsertPivot),
    InsertSlicer(crate::pivot_ui::InsertSlicer),
}

fn sid() -> Id { Id::new("jadwal-objects") }
pub fn state(ctx: &egui::Context) -> State { ctx.data(|d| d.get_temp::<State>(sid())).unwrap_or_default() }
pub fn set_state(ctx: &egui::Context, s: State) { ctx.data_mut(|d| d.insert_temp(sid(), s)); }

/// The selected object on the active sheet.
pub fn selected(app: &SheetApp) -> Option<usize> {
    let s = app.sheet();
    state(&app.ctx).selected.filter(|(si, i)| *si == s && *i < app.book.sheets[s].objects.len()).map(|x| x.1)
}
pub fn select(app: &SheetApp, i: Option<usize>) { let mut st = state(&app.ctx); st.selected = i.map(|i| (app.sheet(), i)); st.multi.clear(); st.drag = None; set_state(&app.ctx, st); }
pub fn open_dialog(app: &SheetApp, d: ObjDialog) { let mut st = state(&app.ctx); st.dialog = Some(d); set_state(&app.ctx, st); }

// ---- geometry ------------------------------------------------------------------------------------

/// Sheet pixels to the screen.
pub fn to_screen(lay: &Layout, x: f32, y: f32) -> Pos2 {
    let c = lay.cols.at(x.max(0.0));
    let r = lay.rows.at(y.max(0.0));
    let ox = (x - lay.cols.pos(c)) * lay.z;
    let sx = if lay.rtl { lay.x(c) + lay.cols.size(c) * lay.z - ox } else { lay.x(c) + ox };
    pos2(sx, lay.y(r) + (y - lay.rows.pos(r)) * lay.z)
}
/// The screen to sheet pixels.
pub fn to_sheet(lay: &Layout, p: Pos2) -> (f32, f32) {
    let c = lay.col_at(p.x);
    let r = lay.row_at(p.y);
    let ox = (p.x - lay.x(c)) / lay.z;
    let x = if lay.rtl { lay.cols.pos(c) + lay.cols.size(c) - ox } else { lay.cols.pos(c) + ox };
    (x.max(0.0), (lay.rows.pos(r) + (p.y - lay.y(r)) / lay.z).max(0.0))
}
pub fn sheet_rect(lay: &Layout, o: &Object) -> [f32; 4] { o.anchor.rect(|c| lay.cols.pos(c), |r| lay.rows.pos(r)) }
pub fn screen_rect(lay: &Layout, o: &Object) -> Rect {
    let r = sheet_rect(lay, o);
    Rect::from_two_pos(to_screen(lay, r[0], r[1]), to_screen(lay, r[2], r[3]))
}
pub(crate) fn place(lay: &Layout, o: &mut Object, r: [f32; 4]) {
    o.anchor.set_rect(r, |x| lay.cols.at(x), |c| lay.cols.pos(c), |y| lay.rows.at(y), |r| lay.rows.pos(r));
}

/// An anchor for a new object of `w`×`h` pixels at the active cell.
pub fn anchor_at_cell(app: &SheetApp, w: f32, h: f32) -> Anchor {
    let s = &app.book.sheets[app.sheet()];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    let p = app.sel.range().start();
    let (x, y) = (cols.pos(p.col), rows.pos(p.row));
    let mut a = Anchor::default();
    a.set_rect([x, y, x + w, y + h], |x| cols.at(x), |c| cols.pos(c), |y| rows.at(y), |r| rows.pos(r));
    a
}

/// Adds an object to the active sheet (with undo) and selects it.
pub fn insert(app: &mut SheetApp, o: Object, base: &str, label: &'static str) -> usize {
    app.commit(None);
    app.checkpoint(label);
    let s = app.sheet();
    let i = add_object(&mut app.book.sheets[s].objects, o, base);
    app.dirty = true;
    select(app, Some(i));
    i
}

// ---- drawing -------------------------------------------------------------------------------------

thread_local! { static TEXTURES: RefCell<HashMap<(usize, usize), Option<TextureHandle>>> = RefCell::new(HashMap::new()); }

fn texture(ctx: &egui::Context, data: &Arc<Vec<u8>>) -> Option<TextureHandle> {
    let key = (Arc::as_ptr(data) as usize, data.len());
    TEXTURES.with(|t| t.borrow_mut().entry(key).or_insert_with(|| {
        let img = image::load_from_memory(data).ok()?.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        Some(ctx.load_texture(format!("picture-{}-{}", key.0, key.1), ci, egui::TextureOptions::LINEAR))
    }).clone())
}

fn paint_shape(p: &egui::Painter, r: Rect, s: &Shape, z: f32) {
    let fill = s.fill.map(rgb).unwrap_or(Color32::TRANSPARENT);
    let stroke = s.line.map(|c| Stroke::new(z.max(0.5), rgb(c))).unwrap_or(Stroke::NONE);
    let geom = !matches!(s.preset.as_str(), "" | "rect" | "ellipse" | "line" | "straightConnector1") && crate::ins_shapes::paint_geom(p.ctx(), p, r, s, z);
    if !geom { match s.preset.as_str() {
        "ellipse" => { p.add(egui::Shape::ellipse_filled(r.center(), r.size() / 2.0, fill)); if stroke.width > 0.0 { p.add(egui::Shape::ellipse_stroke(r.center(), r.size() / 2.0, stroke)); } }
        "roundRect" => { p.rect_filled(r, r.height().min(r.width()) * 0.16, fill); p.rect_stroke(r, r.height().min(r.width()) * 0.16, stroke, StrokeKind::Inside); }
        "triangle" => { let pts = vec![pos2(r.center().x, r.min.y), r.right_bottom(), r.left_bottom()]; p.add(egui::Shape::convex_polygon(pts, fill, stroke)); }
        "line" | "straightConnector1" => { p.line_segment([r.left_top(), r.right_bottom()], if stroke.width > 0.0 { stroke } else { Stroke::new(z, Color32::BLACK) }); }
        _ => { p.rect_filled(r, 0.0, fill); p.rect_stroke(r, 0.0, stroke, StrokeKind::Inside); }
    } }
    if !s.text.is_empty() {
        let color = s.color.map(rgb).unwrap_or(if s.fill.is_some_and(|f| (f[0] as u32 + f[1] as u32 + f[2] as u32) < 300) { Color32::WHITE } else { Color32::BLACK });
        let font = FontId::new((s.size * 4.0 / 3.0 * z).max(4.0), if s.bold { egui::FontFamily::Name("ui-bold".into()) } else { egui::FontFamily::Proportional });
        let inner = crate::ins_shapes::text_rect(s, r).shrink(4.0 * z);
        let g = p.layout(s.text.clone(), font, color, inner.width().max(4.0));
        let x = if s.center { inner.center().x - g.size().x / 2.0 } else { inner.min.x };
        let y = if s.text_box { inner.min.y } else { inner.center().y - g.size().y / 2.0 };
        crate::ins_shapes::text_effects(&p.with_clip_rect(r.expand(4.0 * z).intersect(p.clip_rect())), s, &g, pos2(x, y), z);
        p.with_clip_rect(r.intersect(p.clip_rect())).galley(pos2(x, y), g, color);
    }
}

pub(crate) fn paint(app: &SheetApp, p: &egui::Painter, o: &Object, r: Rect, z: f32) {
    match &o.kind {
        ObjKind::Chart(c) => { crate::chart_ui::paint(app, p, c, r, z); crate::pivot_chart_ui::paint_buttons(app, p, c, r, z); }
        ObjKind::Picture { data, .. } => match texture(&app.ctx, data) {
            Some(t) => { p.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
            None => { p.rect_filled(r, 0.0, c_hover()); p.rect_stroke(r, 0.0, Stroke::new(1.0, c_line()), StrokeKind::Inside); }
        },
        ObjKind::Shape(s) => paint_shape(p, r, s, z),
        ObjKind::Slicer(s) => crate::pivot_ui::paint_slicer(app, p, s, r, z),
        ObjKind::Timeline(t) => crate::timeline_ui::paint(app, p, t, r, z),
        // Groups paint their members (arrange.rs).
        ObjKind::Group(_) | ObjKind::Other => {}
    }
}

fn handles(r: Rect) -> [(u8, Pos2); 8] {
    [(1, r.left_top()), (2, r.center_top()), (3, r.right_top()), (4, r.right_center()), (5, r.right_bottom()), (6, r.center_bottom()), (7, r.left_bottom()), (8, r.left_center())]
}

/// Draws the active sheet's objects and handles their mouse work; called by the grid after the
/// cells, with the grid's own response (a click there deselects).
pub fn grid_hook(app: &mut SheetApp, ui: &mut Ui, painter: &egui::Painter, lay: &Layout, grid: &egui::Response) {
    let si = app.sheet();
    let view = lay.view();
    let mut st = state(ui.ctx());
    crate::ins_shapes::draw_hook(app, ui, painter, lay);
    if st.selected.is_some_and(|(s, i)| s != si || i >= app.book.sheets[si].objects.len()) { st.selected = None; st.drag = None; }
    // A drag on the sheet with Shift/⌘ (or in Select Objects mode) draws a rubber band.
    let band_mod = ui.input(|i| i.modifiers.command || i.modifiers.shift) || st.select_mode;
    if grid.drag_started() && band_mod && !app.book.sheets[si].objects.is_empty() {
        if let Some(pt) = grid.interact_pointer_pos() { st.band = Some((pt, app.sel.clone())); }
    }
    if (grid.clicked() || grid.drag_started() || grid.secondary_clicked()) && st.band.is_none() { st.selected = None; st.multi.clear(); st.drag = None; }
    st.multi.retain(|&k| k < app.book.sheets[si].objects.len());
    let n = app.book.sheets[si].objects.len();
    if n == 0 { set_state(ui.ctx(), st); return; }
    let p = painter.with_clip_rect(view);
    let rects: Vec<Option<Rect>> = app.book.sheets[si].objects.iter().map(|o| { let r = screen_rect(lay, o); (!o.hidden && r.intersects(view) && r.is_finite()).then_some(r) }).collect();
    for i in 0..n { if let Some(r) = rects[i] { crate::arrange::paint_object(app, &p, &app.book.sheets[si].objects[i], r, lay.z); } }
    if crate::arrange::band(app, ui, &p, lay, &mut st) { set_state(ui.ctx(), st); return; }
    let cmd = ui.input(|i| i.modifiers.command);
    let toggle = ui.input(|i| i.modifiers.command || i.modifiers.shift);
    let mut menu_for = None;
    for i in 0..n {
        let Some(r) = rects[i] else { continue };
        let r = if r.width() < 6.0 || r.height() < 6.0 { r.expand(3.0) } else { r };
        let resp = ui.interact(r.intersect(view), Id::new(("sheet-object", si, i)), Sense::click_and_drag());
        let is_slicer = matches!(app.book.sheets[si].objects[i].kind, ObjKind::Slicer(_));
        if resp.hovered() { ui.ctx().set_cursor_icon(if is_slicer && st.selected != Some((si, i)) { CursorIcon::PointingHand } else { CursorIcon::Move }); }
        if resp.clicked() || resp.drag_started() || resp.secondary_clicked() {
            if app.edit.is_some() { app.commit(None); }
            if resp.clicked() && is_slicer {
                if let Some(pt) = resp.interact_pointer_pos() { crate::pivot_ui::slicer_click(app, si, i, r, pt, lay.z, cmd); }
            }
            // Shift/⌘-click adds to (or takes from) the selection; dragging a selected object
            // keeps the selection (they move together); otherwise it alone is selected.
            let mine = st.selected.is_some_and(|(s, _)| s == si);
            let cur = st.selected.filter(|_| mine).map(|x| x.1);
            if toggle && resp.clicked() && mine {
                if cur == Some(i) { st.selected = st.multi.pop().map(|k| (si, k)); }
                else if let Some(k) = st.multi.iter().position(|&k| k == i) { st.multi.remove(k); }
                else { st.multi.extend(cur); st.selected = Some((si, i)); }
            } else if resp.drag_started() && (cur == Some(i) || st.multi.contains(&i)) {
                st.multi.retain(|&k| k != i);
                if let Some(c) = cur { if c != i { st.multi.push(c); } }
                st.selected = Some((si, i));
            } else {
                st.multi.clear();
                st.selected = Some((si, i));
            }
        }
        // Timelines' period strips and PivotCharts' field buttons take their own clicks and drags.
        if crate::timeline_ui::interact(app, ui, &resp, si, i, r, lay.z) || crate::pivot_chart_ui::interact(app, &resp, si, i, r, lay.z) { continue; }
        if resp.drag_started() {
            app.checkpoint("Move Object");
            let sr = sheet_rect(lay, &app.book.sheets[si].objects[i]);
            let others = st.multi.iter().filter(|&&k| k != i).filter_map(|&k| app.book.sheets[si].objects.get(k).map(|o| (k, sheet_rect(lay, o)))).collect();
            st.drag = Some(Drag { handle: 0, start: r, size: (sr[2] - sr[0], sr[3] - sr[1]), grab: ui.input(|x| x.pointer.press_origin()).or(resp.interact_pointer_pos()).unwrap_or(r.center()), others, origin: sr });
        }
        if resp.double_clicked() {
            st.selected = Some((si, i));
            // On a chart's series: Format Data Series; elsewhere the editor (Select Data, text).
            let hit = app.book.sheets[si].objects[i].chart().zip(resp.interact_pointer_pos()).and_then(|(c, at)| crate::chart_ui::series_at(app, &p, c, r, lay.z, at));
            // In a group (a SmartArt graphic...): edit the member under the pointer.
            if matches!(app.book.sheets[si].objects[i].kind, ObjKind::Group(_)) { if let Some(at) = resp.interact_pointer_pos() { crate::arrange::edit_member(app, si, i, r, at); } } else {
            match hit { Some(k) => st.dialog = Some(ObjDialog::FormatSeries(crate::chart_ui::FormatSeries::new(app, si, i, k))), None => open_editor(app, &mut st, si, i) } }
        }
        if resp.secondary_clicked() { menu_for = Some(i); }
        resp.context_menu(|ui| { st.selected = Some((si, i)); context_menu(app, ui, si, i, &mut st); });
    }
    let _ = menu_for;
    for &k in &st.multi { if let Some(r) = rects.get(k).copied().flatten() { p.rect_stroke(r, 0.0, Stroke::new(1.0, SELECT), StrokeKind::Outside); } }
    if st.multi.is_empty() { if let Some((_, i)) = st.selected { if let Some(r) = rects.get(i).copied().flatten() { crate::arrange::rotate_handle(app, ui, &p, lay, si, i, r); } } }
    // The selection frame and its handles.
    if let Some((_, i)) = st.selected {
        if let Some(r) = rects.get(i).copied().flatten() {
            p.rect_stroke(r, 0.0, Stroke::new(1.0, SELECT), StrokeKind::Outside);
            crate::ins_shapes::adjust_handles(app, ui, &p, si, i, r);
            for (h, c) in handles(r) {
                let hr = Rect::from_center_size(c, vec2(9.0, 9.0));
                p.circle_filled(c, 4.0, Color32::WHITE);
                p.circle_stroke(c, 4.0, Stroke::new(1.2, SELECT));
                let hresp = ui.interact(hr, Id::new(("sheet-object-handle", si, i, h)), Sense::drag());
                if hresp.hovered() || hresp.dragged() { ui.ctx().set_cursor_icon(match h { 1 | 5 => CursorIcon::ResizeNwSe, 3 | 7 => CursorIcon::ResizeNeSw, 2 | 6 => CursorIcon::ResizeVertical, _ => CursorIcon::ResizeHorizontal }); }
                if hresp.drag_started() {
                    app.checkpoint("Resize Object");
                    st.drag = Some(Drag { handle: h, start: r, size: (0.0, 0.0), grab: hresp.interact_pointer_pos().unwrap_or(c), others: vec![], origin: [0.0; 4] });
                }
            }
        }
    }
    // An active move or resize follows the pointer.
    if let (Some(d), Some((_, i))) = (st.drag.clone(), st.selected) {
        let down = ui.input(|x| x.pointer.primary_down());
        match ui.input(|x| x.pointer.interact_pos()) {
            Some(pt) if down => {
                let dv = pt - d.grab;
                let mut r = d.start;
                let keep = ui.input(|x| x.modifiers.shift) || matches!(app.book.sheets[si].objects[i].kind, ObjKind::Picture { .. }) && [1, 3, 5, 7].contains(&d.handle);
                match d.handle {
                    0 => r = r.translate(dv),
                    h => {
                        if matches!(h, 1 | 7 | 8) { r.min.x = (r.min.x + dv.x).min(r.max.x - 4.0); }
                        if matches!(h, 3 | 4 | 5) { r.max.x = (r.max.x + dv.x).max(r.min.x + 4.0); }
                        if matches!(h, 1 | 2 | 3) { r.min.y = (r.min.y + dv.y).min(r.max.y - 4.0); }
                        if matches!(h, 5 | 6 | 7) { r.max.y = (r.max.y + dv.y).max(r.min.y + 4.0); }
                        if keep && d.start.height() > 0.0 {
                            let ratio = d.start.width() / d.start.height();
                            let h2 = r.width() / ratio;
                            if matches!(h, 1 | 3) { r.min.y = r.max.y - h2; } else { r.max.y = r.min.y + h2; }
                        }
                    }
                }
                let a = to_sheet(lay, r.min);
                let b = to_sheet(lay, r.max);
                let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                let sr = if d.handle == 0 {
                    let x0 = if lay.rtl { x1 - d.size.0 } else { x0 };
                    [x0.max(0.0), y0, x0.max(0.0) + d.size.0, y0 + d.size.1]
                } else { [x0, y0, x1, y1] };
                let mut sr = sr;
                crate::arrange::snap(app, &st, lay, si, i, d.handle, &mut sr);
                if let Some(o) = app.book.sheets[si].objects.get_mut(i) { place(lay, o, sr); }
                // The other selected objects move along.
                let (dx, dy) = (sr[0] - d.origin[0], sr[1] - d.origin[1]);
                for (k, r0) in &d.others { if let Some(o) = app.book.sheets[si].objects.get_mut(*k) { place(lay, o, [r0[0] + dx, r0[1] + dy, r0[2] + dx, r0[3] + dy]); } }
                app.dirty = true;
            }
            _ => st.drag = None,
        }
    }
    set_state(ui.ctx(), st);
}

/// Double-click: a chart's data, a shape's text.
fn open_editor(app: &mut SheetApp, st: &mut State, si: usize, i: usize) {
    match &app.book.sheets[si].objects[i].kind {
        ObjKind::Chart(_) => st.dialog = Some(ObjDialog::SelectData(crate::chart_ui::SelectData::new(app, si, i))),
        ObjKind::Shape(s) => st.dialog = Some(ObjDialog::EditText { sheet: si, idx: i, text: s.text.clone() }),
        _ => { crate::ins_pictures::open_object(app, si, i); }
    }
}

fn context_menu(app: &mut SheetApp, ui: &mut Ui, si: usize, i: usize, st: &mut State) {
    let is_chart = app.book.sheets[si].objects.get(i).is_some_and(|o| o.is_chart());
    let is_shape = matches!(app.book.sheets[si].objects.get(i).map(|o| &o.kind), Some(ObjKind::Shape(_)));
    if is_chart && ui.button(tr("Format Data Series…")).clicked() { st.dialog = Some(ObjDialog::FormatSeries(crate::chart_ui::FormatSeries::new(app, si, i, 0))); ui.close(); }
    if is_chart && ui.button(tr("Select Data…")).clicked() { open_editor(app, st, si, i); ui.close(); }
    if is_shape && ui.button(tr("Edit Text…")).clicked() { open_editor(app, st, si, i); ui.close(); }
    if ui.button(tr("Bring to Front")).clicked() { st.selected = order(app, si, i, 2).map(|k| (si, k)); ui.close(); }
    if ui.button(tr("Send to Back")).clicked() { st.selected = order(app, si, i, -2).map(|k| (si, k)); ui.close(); }
    ui.separator();
    if ui.button(tr("Delete")).clicked() { delete(app, si, i); st.selected = None; st.drag = None; ui.close(); }
}

/// Changes the stacking order: ±1 one step, ±2 to the front or back. Returns the new index.
pub fn order(app: &mut SheetApp, si: usize, i: usize, how: i32) -> Option<usize> {
    let n = app.book.sheets[si].objects.len();
    if i >= n { return None; }
    let to = match how { 2 => n - 1, -2 => 0, 1 => (i + 1).min(n - 1), _ => i.saturating_sub(1) };
    if to == i { return Some(i); }
    app.checkpoint(if how > 0 { "Bring Forward" } else { "Send Backward" });
    let o = app.book.sheets[si].objects.remove(i);
    app.book.sheets[si].objects.insert(to, o);
    app.dirty = true;
    Some(to)
}

pub fn delete(app: &mut SheetApp, si: usize, i: usize) {
    if i >= app.book.sheets[si].objects.len() { return; }
    app.checkpoint("Delete Object");
    app.book.sheets[si].objects.remove(i);
    app.dirty = true;
}

/// Object keys (before the grid's): Delete, Escape, arrows nudge. True when consumed.
pub fn keys(app: &mut SheetApp, ctx: &egui::Context) -> bool {
    let Some(i) = selected(app) else { return false };
    if crate::arrange::keys(app, ctx) { return true; }
    let si = app.sheet();
    let mut st = state(ctx);
    let mut done = false;
    let mut act: Option<char> = None;
    ctx.input_mut(|inp| {
        use egui::{Key, Modifiers};
        if inp.consume_key(Modifiers::NONE, Key::Delete) || inp.consume_key(Modifiers::NONE, Key::Backspace) { done = true; st.selected = None; app.book.sheets[si].objects.get(i).map(|_| ()).inspect(|_| {}); }
        else if inp.consume_key(Modifiers::NONE, Key::Escape) { st.selected = None; }
        else if inp.consume_key(Modifiers::COMMAND, Key::C) || inp.events.iter().any(|e| matches!(e, egui::Event::Copy)) {
            inp.events.retain(|e| !matches!(e, egui::Event::Copy));
            act = Some('c');
        }
        else if inp.consume_key(Modifiers::COMMAND, Key::X) || inp.events.iter().any(|e| matches!(e, egui::Event::Cut)) {
            inp.events.retain(|e| !matches!(e, egui::Event::Cut));
            act = Some('x');
        }
        else if inp.consume_key(Modifiers::COMMAND, Key::V) || inp.events.iter().any(|e| matches!(e, egui::Event::Paste(_))) {
            inp.events.retain(|e| !matches!(e, egui::Event::Paste(_)));
            act = Some('v');
        }
        else if inp.consume_key(Modifiers::COMMAND, Key::D) { act = Some('d'); }
        else {
            for (k, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
                if inp.consume_key(Modifiers::NONE, k) { st.drag = None; nudge(app, si, i, dx, dy); }
                if inp.consume_key(Modifiers::SHIFT, k) { nudge(app, si, i, dx * 10.0, dy * 10.0); }
            }
        }
    });
    if done { delete(app, si, i); }
    let keep = st.selected.is_some();
    set_state(ctx, st);
    match act {
        Some('c') | Some('x') => {
            copy_object(app, si, i);
            if act == Some('x') { delete(app, si, i); select(app, None); }
            return true;
        }
        Some('v') => { paste_object(app, true); return true; }
        Some('d') => { duplicate_object(app, si, i); return true; }
        _ => {}
    }
    keep || done
}

/// ⌘C on an object: it goes on the object clipboard (cells copied earlier are dropped).
pub fn copy_object(app: &mut SheetApp, si: usize, i: usize) {
    let Some(o) = app.book.sheets[si].objects.get(i) else { return };
    let mut c = sheet_engine::follow::copy_object(o);
    // Unqualified chart references named the copied chart's sheet.
    let own = app.book.sheets[si].name.clone();
    if let ObjKind::Chart(ch) = &mut c.kind {
        for se in &mut ch.series { for r in [&mut se.name_ref, &mut se.cat_ref, &mut se.val_ref].into_iter().flatten() { if let Some((None, rg)) = sheet_engine::chart::parse_ref(r) { *r = sheet_engine::chart::make_ref(&own, rg); } } }
    }
    let mut st = state(&app.ctx); st.clip = Some(c); set_state(&app.ctx, st);
    app.clip = None; app.cut = None; app.grid.marching = None;
}

/// ⌘V with an object on the clipboard: a new copy on the active sheet, a cell down and right of
/// the selected object (`offset`) or at the active cell. False when there is none.
pub fn paste_object(app: &mut SheetApp, offset: bool) -> bool {
    let Some(mut o) = state(&app.ctx).clip else { return false };
    let s = app.sheet();
    let sel = selected(app).filter(|_| offset).and_then(|i| app.book.sheets[s].objects.get(i)).and_then(|x| sheet_engine::follow::anchor_cells(&x.anchor));
    let src = sheet_engine::follow::anchor_cells(&o.anchor);
    match (sel, src) {
        (Some(at), Some(c)) => sheet_engine::follow::move_anchor(&mut o.anchor, at.r0 as i64 + 1 - c.r0 as i64, at.c0 as i64 + 1 - c.c0 as i64),
        (None, Some(c)) => { let p = app.sel.active; sheet_engine::follow::move_anchor(&mut o.anchor, p.row as i64 - c.r0 as i64, p.col as i64 - c.c0 as i64) }
        _ => {}
    }
    let base = sheet_engine::follow::base_name(&o);
    insert(app, o, base, "Paste");
    true
}

/// ⌘D on an object: a copy just below and right of it.
pub fn duplicate_object(app: &mut SheetApp, si: usize, i: usize) {
    let Some(o) = app.book.sheets[si].objects.get(i) else { return };
    let mut c = sheet_engine::follow::copy_object(o);
    sheet_engine::follow::move_anchor(&mut c.anchor, 1, 1);
    if let Anchor::Abs { x, y, .. } = &mut c.anchor { *x += 16.0; *y += 16.0; }
    let base = sheet_engine::follow::base_name(&c);
    insert(app, c, base, "Duplicate");
}

/// Copying cells drops a copied object.
pub fn clear_clip(ctx: &egui::Context) { let mut st = state(ctx); if st.clip.is_some() { st.clip = None; set_state(ctx, st); } }

fn nudge(app: &mut SheetApp, si: usize, i: usize, dx: f32, dy: f32) {
    let s = &app.book.sheets[si];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    app.checkpoint("Move Object");
    let o = &mut app.book.sheets[si].objects[i];
    let r = o.anchor.rect(|c| cols.pos(c), |r| rows.pos(r));
    let dx = if app.book.sheets[si].right_to_left { -dx } else { dx };
    let o = &mut app.book.sheets[si].objects[i];
    o.anchor.set_rect([r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy], |x| cols.at(x), |c| cols.pos(c), |y| rows.at(y), |r| rows.pos(r));
    app.dirty = true;
}

// ---- inserting -------------------------------------------------------------------------------------

pub fn insert_picture(app: &mut SheetApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() else { return };
    let data = match std::fs::read(&path) { Ok(d) => d, Err(e) => { app.error = Some(format!("{}\n{e}", tr("The picture couldn't be read."))); return } };
    let Ok(img) = image::load_from_memory(&data) else { app.error = Some(tr("That file isn't a picture Jadwal can show (PNG, JPEG).")); return };
    let ext = match image::guess_format(&data) { Ok(image::ImageFormat::Jpeg) => "jpeg", Ok(image::ImageFormat::Gif) => "gif", Ok(image::ImageFormat::Bmp) => "bmp", _ => "png" };
    let (w, h) = (img.width() as f32, img.height() as f32);
    let k = (480.0 / w).min(360.0 / h).min(1.0);
    let mut o = Object::new(ObjKind::Picture { data: Arc::new(data), ext: ext.into() }, anchor_at_cell(app, w * k, h * k));
    if let Some(Anchor::Two { edit_as, .. }) = Some(&mut o.anchor) { *edit_as = Some("oneCell".into()); }
    o.descr = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    insert(app, o, "Picture", "Insert Picture");
}

pub fn insert_shape(app: &mut SheetApp, preset: &str) {
    let text_box = preset == "textBox";
    let shape = if text_box { Shape { preset: "rect".into(), text_box: true, size: 11.0, fill: Some([255, 255, 255]), line: Some([0xBF, 0xBF, 0xBF]), ..Default::default() } }
        else { Shape { preset: preset.into(), size: 11.0, fill: app.book.theme_colors.get(4).copied(), line: app.book.theme_colors.get(4).map(|c| c.map(|x| (x as f32 * 0.7) as u8)), color: Some([255, 255, 255]), center: true, ..Default::default() } };
    let (w, h): (f32, f32) = if text_box { (190.0, 60.0) } else if preset == "line" { (150.0, 0.0) } else { (120.0, 80.0) };
    let o = Object::new(ObjKind::Shape(shape), anchor_at_cell(app, w, h.max(1.0)));
    let i = insert(app, o, if text_box { "TextBox" } else { "Shape" }, if text_box { "Insert Text Box" } else { "Insert Shape" });
    if text_box { open_dialog(app, ObjDialog::EditText { sheet: app.sheet(), idx: i, text: String::new() }); }
}

pub const SHAPES: &[(&str, &str)] = &[("rect", "Rectangle"), ("roundRect", "Rounded Rectangle"), ("ellipse", "Oval"), ("triangle", "Triangle"), ("line", "Line")];

// ---- the window's hooks ------------------------------------------------------------------------------

/// Called by the window before the grid: the PivotTable Fields pane and the object dialogs.
pub fn frame(app: &mut SheetApp, ui: &mut Ui) {
    crate::arrange::frame(app, ui);
    crate::pivot_ui::panel(app, ui);
    let ctx = ui.ctx().clone();
    crate::ins_pictures::frame(app, &ctx);
    crate::ins_pictures::clip_overlay(app, &ctx);
    let mut st = state(&ctx);
    let Some(mut d) = st.dialog.take() else { return };
    // Out of the stored state while it runs, so closing it sticks (and a dialog it opens wins).
    set_state(&ctx, st);
    let keep = match &mut d {
        ObjDialog::EditText { sheet, idx, text } => edit_text(app, &ctx, *sheet, *idx, text),
        ObjDialog::SelectData(sd) => crate::chart_ui::select_data_dialog(app, &ctx, sd),
        ObjDialog::ChartType(d) => crate::chart_ui::chart_type_dialog(app, &ctx, d),
        ObjDialog::FormatSeries(d) => crate::chart_ui::format_series_dialog(app, &ctx, d),
        ObjDialog::InsertPivot(ip) => crate::pivot_ui::insert_dialog(app, &ctx, ip),
        ObjDialog::InsertSlicer(is) => crate::pivot_ui::slicer_dialog(app, &ctx, is),
    };
    let mut st = state(&ctx);
    if keep && st.dialog.is_none() { st.dialog = Some(d); }
    set_state(&ctx, st);
}

fn edit_text(app: &mut SheetApp, ctx: &egui::Context, si: usize, i: usize, text: &mut String) -> bool {
    let mut keep = true;
    let mut open = true;
    crate::theme::dialog(ctx, "Edit Text").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(320.0);
        ui.add(egui::TextEdit::multiline(text).desired_rows(5).desired_width(320.0)).request_focus();
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                if let Some(o) = app.book.sheets.get(si).and_then(|s| s.objects.get(i)) {
                    if matches!(o.kind, ObjKind::Shape(_)) {
                        app.checkpoint("Edit Text");
                        let o = &mut app.book.sheets[si].objects[i];
                        if let ObjKind::Shape(s) = &mut o.kind { s.text = text.clone(); }
                        o.touch();
                        app.dirty = true;
                    }
                }
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

/// The common contextual groups for any selected object: arrange and delete.
pub fn arrange_group(app: &mut SheetApp, ui: &mut Ui) { crate::arrange::ribbon_group(app, ui); }

/// The Shape Format / Picture Format tab: fill and outline for shapes, text, arrange.
pub fn format_tab(app: &mut SheetApp, ui: &mut Ui) {
    use qalam_kit::widgets::*;
    let Some(i) = selected(app) else { return };
    let si = app.sheet();
    if let ObjKind::Shape(s) = app.book.sheets[si].objects[i].kind.clone() {
        group(ui, "Shape Styles", |ui| {
            let r = large_menu(ui, "shapeFill", "Fill", "Shape fill");
            crate::theme::menu(&r, |ui| { let th = app.book.theme_colors.clone(); if let Some(c) = crate::themes_ui::color_menu(ui, "No Fill", &th) { set_shape(app, si, i, "Shape Fill", |s| s.fill = c.map(|c| c.0)); ui.close(); } });
            let r = large_menu(ui, "shapeOutline", "Outline", "Shape outline");
            crate::theme::menu(&r, |ui| { let th = app.book.theme_colors.clone(); if let Some(c) = crate::themes_ui::color_menu(ui, "No Outline", &th) { set_shape(app, si, i, "Shape Outline", |s| s.line = c.map(|c| c.0)); ui.close(); } });
        });
        group(ui, "Text", |ui| {
            if large(ui, "textBox", "Edit Text", false, true, "Edit the shape's text").clicked() { open_dialog(app, ObjDialog::EditText { sheet: si, idx: i, text: s.text.clone() }); }
            rows(ui, |ui| {
                if small(ui, "bold", None, s.bold, true, "Bold").clicked() { set_shape(app, si, i, "Bold", |s| s.bold = !s.bold); }
                if small(ui, "alignCenter", None, s.center, true, "Center").clicked() { set_shape(app, si, i, "Center", |s| s.center = !s.center); }
                if small(ui, "fontGrow", None, false, true, "Larger text").clicked() { set_shape(app, si, i, "Font Size", |s| s.size = (s.size + 2.0).min(96.0)); }
                if small(ui, "fontShrink", None, false, true, "Smaller text").clicked() { set_shape(app, si, i, "Font Size", |s| s.size = (s.size - 2.0).max(6.0)); }
                let r = small_menu(ui, "textEffects", "Text Effects", "Shadow, glow, outline and WordArt styles");
                crate::theme::menu(&r, |ui| crate::ins_shapes::effects_menu(app, ui, si, i));
            });
        });
    }
    if let ObjKind::Picture { .. } = &app.book.sheets[si].objects[i].kind {
        group(ui, "Adjust", |ui| {
            if large(ui, "resetPicture", "Reset Size", false, true, "Give the picture its own size again").clicked() { reset_picture(app, si, i); }
        });
    }
    arrange_group(app, ui);
}

fn set_shape(app: &mut SheetApp, si: usize, i: usize, label: &'static str, f: impl FnOnce(&mut Shape)) {
    app.checkpoint(label);
    let o = &mut app.book.sheets[si].objects[i];
    if let ObjKind::Shape(s) = &mut o.kind { f(s); }
    o.touch();
    app.dirty = true;
}

fn reset_picture(app: &mut SheetApp, si: usize, i: usize) {
    let ObjKind::Picture { data, .. } = &app.book.sheets[si].objects[i].kind else { return };
    let Ok((w, h)) = image::load_from_memory(data).map(|i| (i.width() as f32, i.height() as f32)) else { return };
    let s = &app.book.sheets[si];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    app.checkpoint("Reset Picture");
    let o = &mut app.book.sheets[si].objects[i];
    let r = o.anchor.rect(|c| cols.pos(c), |r| rows.pos(r));
    o.anchor.set_rect([r[0], r[1], r[0] + w, r[1] + h], |x| cols.at(x), |c| cols.pos(c), |y| rows.at(y), |r| rows.pos(r));
    app.dirty = true;
}

#[cfg(test)]
mod tests {
    use crate::app::SheetApp;
    use sheet_engine::addr::{parse_a1, parse_range};
    use sheet_engine::chart::{ChartKind, Grouping};

    fn sample(ctx: &egui::Context) -> SheetApp {
        let mut a = SheetApp::blank(ctx.clone());
        let rows = [["Region", "Product", "Sales", "Qty"], ["East", "A", "10", "1"], ["West", "A", "20", "2"], ["East", "B", "5", "3"], ["North", "A", "7", "4"], ["West", "B", "12", "5"]];
        for (i, row) in rows.iter().enumerate() { for (j, t) in row.iter().enumerate() { a.book.set_input(0, sheet_engine::Pos::new(i as u32, j as u32), t); } }
        a.book.recalc();
        a
    }

    #[test]
    fn chart_pivot_slicer_flow() {
        let ctx = egui::Context::default();
        let mut a = sample(&ctx);
        a.sel = crate::app::Selection { anchor: parse_a1("B1").unwrap(), active: parse_a1("C6").unwrap(), extra: vec![] };
        crate::chart_ui::insert_chart(&mut a, ChartKind::Column, Grouping::Clustered);
        assert_eq!(a.book.sheets[0].objects.len(), 1);
        assert_eq!(super::selected(&a), Some(0));
        crate::chart_ui::edit_chart(&mut a, "t", |c| c.kind = ChartKind::Line);
        assert_eq!(a.book.sheets[0].objects[0].chart().unwrap().kind, ChartKind::Line);
        a.undo();
        assert_eq!(a.book.sheets[0].objects[0].chart().unwrap().kind, ChartKind::Column);
        // A pivot on a new sheet, then a slicer filtering it.
        a.sel = crate::app::Selection::at(parse_a1("A1").unwrap());
        crate::pivot_ui::recommended(&mut a);
        let si = a.sheet();
        assert_eq!(a.book.sheets[si].pivots.len(), 1);
        assert_eq!(a.book.display(si, parse_a1("A3").unwrap()), "Row Labels");
        assert_eq!(a.book.display(si, parse_a1("B4").unwrap()), "15");
        let name = a.book.sheets[si].pivots[0].name.clone();
        let sl = sheet_engine::drawing::Slicer { pivot: name, table: String::new(), field: 0, caption: "Region".into() };
        let anchor = super::anchor_at_cell(&a, 180.0, 230.0);
        let i = super::insert(&mut a, sheet_engine::drawing::Object::new(sheet_engine::drawing::ObjKind::Slicer(sl), anchor), "Slicer", "Insert Slicer");
        let r = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(180.0, 230.0));
        // The first item button ("East") alone.
        assert!(crate::pivot_ui::slicer_click(&mut a, si, i, r, egui::pos2(90.0, 30.0 + 13.0), 1.0, false));
        assert_eq!(a.book.display(si, parse_a1("A4").unwrap()), "East");
        assert_eq!(a.book.display(si, parse_a1("A5").unwrap()), "Grand Total");
        let _ = parse_range;
    }

    #[test]
    fn renders_objects() {
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 760.0)).with_accessibility_check(false).build_ui(move |ui| {
            thread_local!(static APP: std::cell::RefCell<Option<SheetApp>> = const { std::cell::RefCell::new(None) });
            APP.with(|a| {
                let mut a = a.borrow_mut();
                let app = a.get_or_insert_with(|| {
                    qalam_kit::fonts::install_fonts(ui.ctx());
                    let mut x = sample(ui.ctx());
                    x.sel = crate::app::Selection { anchor: parse_a1("B1").unwrap(), active: parse_a1("D6").unwrap(), extra: vec![] };
                    crate::chart_ui::insert_chart(&mut x, ChartKind::Column, Grouping::Clustered);
                    x.sel = crate::app::Selection { anchor: parse_a1("B1").unwrap(), active: parse_a1("C6").unwrap(), extra: vec![] };
                    crate::chart_ui::insert_chart(&mut x, ChartKind::Pie, Grouping::Clustered);
                    let s = &mut x.book.sheets[0].objects[1];
                    s.anchor = sheet_engine::drawing::Anchor::Abs { x: 20.0, y: 160.0, w: 360.0, h: 260.0 };
                    super::insert_shape(&mut x, "ellipse");
                    if let sheet_engine::drawing::ObjKind::Shape(s) = &mut x.book.sheets[0].objects[2].kind { s.text = "Hello".into(); }
                    x.book.sheets[0].objects[2].anchor = sheet_engine::drawing::Anchor::Abs { x: 420.0, y: 330.0, w: 140.0, h: 80.0 };
                    super::set_state(ui.ctx(), super::State { selected: Some((0, 0)), ..Default::default() });
                    x.tab = crate::app::Tab::ChartDesign;
                    x
                });
                app.frame(ui);
            });
        });
        harness.run_steps(4);
        if let Ok(dir) = std::env::var("JADWAL_SNAPSHOT_DIR") { if let Ok(img) = harness.render() { let _ = img.save(format!("{dir}/objects.png")); } }
    }

    #[test]
    fn renders_pivot_and_kinds() {
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1300.0, 820.0)).with_accessibility_check(false).build_ui(move |ui| {
            thread_local!(static APP: std::cell::RefCell<Option<SheetApp>> = const { std::cell::RefCell::new(None) });
            APP.with(|a| {
                let mut a = a.borrow_mut();
                let app = a.get_or_insert_with(|| {
                    qalam_kit::fonts::install_fonts(ui.ctx());
                    let mut x = sample(ui.ctx());
                    for (k, (kind, g)) in [(ChartKind::Bar, Grouping::Stacked), (ChartKind::Line, Grouping::Clustered), (ChartKind::Area, Grouping::Stacked), (ChartKind::Scatter, Grouping::Clustered)].into_iter().enumerate() {
                        x.sel = crate::app::Selection { anchor: parse_a1(if kind == ChartKind::Scatter { "C1" } else { "B1" }).unwrap(), active: parse_a1("D6").unwrap(), extra: vec![] };
                        crate::chart_ui::insert_chart(&mut x, kind, g);
                        x.book.sheets[0].objects[k].anchor = sheet_engine::drawing::Anchor::Abs { x: (k % 2) as f32 * 330.0 + 330.0, y: (k / 2) as f32 * 230.0 + 10.0, w: 320.0, h: 220.0 };
                    }
                    x.sel = crate::app::Selection::at(parse_a1("A1").unwrap());
                    crate::pivot_ui::recommended(&mut x);
                    let si = x.sheet();
                    x.book.sheets[si].pivots[0].cols = vec![1];
                    crate::pivot_ui::rerun(&mut x, si, 0);
                    let name = x.book.sheets[si].pivots[0].name.clone();
                    let sl = sheet_engine::drawing::Slicer { pivot: name, table: String::new(), field: 0, caption: "Region".into() };
                    let mut o = sheet_engine::drawing::Object::new(sheet_engine::drawing::ObjKind::Slicer(sl), Default::default());
                    o.anchor = sheet_engine::drawing::Anchor::Abs { x: 420.0, y: 60.0, w: 180.0, h: 200.0 };
                    super::insert(&mut x, o, "Slicer", "Insert Slicer");
                    x.sel = crate::app::Selection::at(parse_a1("A4").unwrap());
                    x.tab = crate::app::Tab::PivotAnalyze;
                    if std::env::var("JADWAL_SHOW_CHARTS").is_ok() { x.book.active = 1; }
                    x
                });
                app.frame(ui);
            });
        });
        harness.run_steps(4);
        if let Ok(dir) = std::env::var("JADWAL_SNAPSHOT_DIR") {
            if let Ok(img) = harness.render() { let _ = img.save(format!("{dir}/pivot.png")); }
        }
    }
}
