//! Arranging floating objects (Page Layout ▸ Arrange, Shape Format ▸ Arrange): stacking order,
//! the Selection Pane, Align and Distribute, Snap to Grid / Shape, Group / Regroup / Ungroup,
//! Rotate and Flip with More Rotation Options; plus what the object layer needs for them —
//! painting rotated objects and groups, the rubber band, the rotation handle, editing a
//! member of a group (SmartArt boxes) and the multi-selection keys.

use crate::app::SheetApp;
use crate::grid::{Axis, Layout, SELECT};
use crate::i18n::tr;
use crate::objects::{screen_rect, set_state, state, State};
use egui::{pos2, vec2, Color32, Id, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use sheet_engine::drawing::{Anchor, ObjKind, Object};
use sheet_engine::group;

// ---- painting ------------------------------------------------------------------------------------

fn turn_pt(p: Pos2, c: Pos2, deg: f32, fh: bool, fv: bool) -> Pos2 { let t = group::turn((p.x, p.y), (c.x, c.y), deg, fh, fv); pos2(t.0, t.1) }

/// A painted shape turned `deg` clockwise about `c` after the flips.
fn turn_shape(s: egui::Shape, c: Pos2, deg: f32, fh: bool, fv: bool) -> egui::Shape {
    use egui::Shape;
    let t = |p: Pos2| turn_pt(p, c, deg, fh, fv);
    match s {
        Shape::Vec(v) => Shape::Vec(v.into_iter().map(|x| turn_shape(x, c, deg, fh, fv)).collect()),
        Shape::Circle(mut k) => { k.center = t(k.center); Shape::Circle(k) }
        Shape::Ellipse(e) => {
            let pts: Vec<Pos2> = (0..48).map(|i| { let a = i as f32 / 48.0 * std::f32::consts::TAU; t(e.center + vec2(e.radius.x * a.cos(), e.radius.y * a.sin())) }).collect();
            Shape::Path(egui::epaint::PathShape::convex_polygon(pts, e.fill, e.stroke))
        }
        Shape::LineSegment { points, stroke } => Shape::LineSegment { points: points.map(t), stroke },
        Shape::Path(mut ps) => { for p in ps.points.iter_mut() { *p = t(*p); } Shape::Path(ps) }
        Shape::Rect(mut rs) => {
            let m = turn_pt(rs.rect.center(), c, 0.0, fh, fv);
            let nc = turn_pt(m, c, deg, false, false);
            rs.rect = Rect::from_center_size(nc, rs.rect.size());
            rs.angle += deg.to_radians();
            if (fh || fv) && rs.brush.is_some() {
                let mut b = (**rs.brush.as_ref().unwrap()).clone();
                if fh { b.uv = Rect::from_min_max(pos2(b.uv.max.x, b.uv.min.y), pos2(b.uv.min.x, b.uv.max.y)); }
                if fv { b.uv = Rect::from_min_max(pos2(b.uv.min.x, b.uv.max.y), pos2(b.uv.max.x, b.uv.min.y)); }
                rs.brush = Some(std::sync::Arc::new(b));
            }
            Shape::Rect(rs)
        }
        Shape::Text(mut ts) => {
            // Text keeps reading the right way round; it turns with the object.
            let r = ts.galley.rect.translate(ts.pos.to_vec2());
            let m = turn_pt(r.center(), c, 0.0, fh, fv);
            let p0 = m - r.size() / 2.0;
            ts.pos = turn_pt(p0, c, deg, false, false);
            ts.angle += deg.to_radians();
            Shape::Text(ts)
        }
        Shape::Mesh(m) => { let mut m = (*m).clone(); for v in m.vertices.iter_mut() { v.pos = t(v.pos); } Shape::Mesh(std::sync::Arc::new(m)) }
        Shape::QuadraticBezier(mut q) => { q.points = q.points.map(t); Shape::QuadraticBezier(q) }
        Shape::CubicBezier(mut q) => { q.points = q.points.map(t); Shape::CubicBezier(q) }
        other => other,
    }
}

/// Paints an object: groups paint their members, rotated and flipped objects are turned.
pub fn paint_object(app: &SheetApp, p: &egui::Painter, o: &Object, r: Rect, z: f32) {
    if o.hidden { return; }
    let turned = o.rot != 0.0 || o.flip_h || o.flip_v;
    let layer = p.layer_id();
    let start = p.ctx().graphics_mut(|g| g.entry(layer).next_idx());
    match &o.kind {
        ObjKind::Group(g) => for c in &g.children {
            let cr = g.child_rect(c, [r.min.x, r.min.y, r.max.x, r.max.y]);
            paint_object(app, p, c, Rect::from_min_max(pos2(cr[0], cr[1]), pos2(cr[2], cr[3])), z);
        },
        _ => crate::objects::paint(app, p, o, r, z),
    }
    if turned {
        let end = p.ctx().graphics_mut(|g| g.entry(layer).next_idx());
        let clip = p.clip_rect();
        p.ctx().graphics_mut(|g| {
            let list = g.entry(layer);
            for k in start.0..end.0 {
                list.mutate_shape(egui::layers::ShapeIdx(k), |cs| {
                    let s = std::mem::replace(&mut cs.shape, egui::Shape::Noop);
                    cs.shape = turn_shape(s, r.center(), o.rot, o.flip_h, o.flip_v);
                    cs.clip_rect = clip;
                });
            }
        });
    }
}

// ---- selection -------------------------------------------------------------------------------------

/// The selected objects on the active sheet, in stacking order.
pub fn selection(app: &SheetApp) -> Vec<usize> {
    let st = state(&app.ctx);
    let si = app.sheet();
    let n = app.book.sheets[si].objects.len();
    let mut v: Vec<usize> = st.selected.filter(|(s, i)| *s == si && *i < n).map(|x| x.1).into_iter().chain(st.multi.iter().copied().filter(|&k| k < n)).collect();
    v.sort(); v.dedup();
    v
}

fn set_selection(app: &SheetApp, v: &[usize]) {
    let mut st = state(&app.ctx);
    st.selected = v.last().map(|&k| (app.sheet(), k));
    st.multi = v[..v.len().saturating_sub(1)].to_vec();
    set_state(&app.ctx, st);
}

fn axes(app: &SheetApp, si: usize) -> (Axis, Axis) { let s = &app.book.sheets[si]; (Axis::of_cols(s), Axis::of_rows(s)) }

fn rect_of(app: &SheetApp, si: usize, k: usize) -> [f32; 4] {
    let (c, r) = axes(app, si);
    app.book.sheets[si].objects[k].anchor.rect(|x| c.pos(x), |y| r.pos(y))
}

fn place(app: &mut SheetApp, si: usize, k: usize, rc: [f32; 4]) {
    let (c, r) = axes(app, si);
    app.book.sheets[si].objects[k].anchor.set_rect(rc, |x| c.at(x), |x| c.pos(x), |y| r.at(y), |y| r.pos(y));
}

/// The rubber band: drawn while dragging, selecting what it encloses when released. True while
/// it is active (the object layer then takes no other mouse work).
pub fn band(app: &mut SheetApp, ui: &mut Ui, p: &egui::Painter, lay: &Layout, st: &mut State) -> bool {
    let Some((start, sel)) = st.band.clone() else { return false };
    let down = ui.input(|i| i.pointer.primary_down());
    let cur = ui.input(|i| i.pointer.interact_pos()).unwrap_or(start);
    let r = Rect::from_two_pos(start, cur);
    p.rect(r, 0.0, SELECT.gamma_multiply(0.08), Stroke::new(1.0, SELECT), StrokeKind::Inside);
    // The cells stay selected as they were.
    app.sel = sel;
    if !down {
        let si = app.sheet();
        let hits: Vec<usize> = app.book.sheets[si].objects.iter().enumerate().filter(|(_, o)| !o.hidden && r.contains_rect(screen_rect(lay, o))).map(|(k, _)| k).collect();
        st.band = None;
        st.selected = hits.last().map(|&k| (si, k));
        st.multi = hits[..hits.len().saturating_sub(1)].to_vec();
        st.multi.retain(|_| true);
    }
    ui.ctx().request_repaint();
    true
}

/// Delete and Escape with several objects selected. True when consumed.
pub fn keys(app: &mut SheetApp, ctx: &egui::Context) -> bool {
    let sel = selection(app);
    if sel.len() < 2 { return false; }
    let (del, esc) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, egui::Key::Delete) || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace), i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)));
    if del {
        app.checkpoint("Delete Object");
        let si = app.sheet();
        for &k in sel.iter().rev() { app.book.sheets[si].objects.remove(k); }
        app.dirty = true;
    }
    if del || esc { let mut st = state(ctx); st.selected = None; st.multi.clear(); set_state(ctx, st); return true; }
    false
}

// ---- snapping ----------------------------------------------------------------------------------------

/// Snap to Grid / Snap to Shape while moving (handle 0) or resizing an object.
pub fn snap(app: &SheetApp, st: &State, lay: &Layout, si: usize, i: usize, handle: u8, sr: &mut [f32; 4]) {
    if !st.snap_grid && !st.snap_shape { return; }
    let tol = 6.0 / lay.z;
    let mut xs: Vec<f32> = vec![];
    let mut ys: Vec<f32> = vec![];
    if st.snap_grid {
        for x in [sr[0], sr[2]] { let c = lay.cols.at(x); xs.extend([lay.cols.pos(c), lay.cols.pos(c + 1)]); }
        for y in [sr[1], sr[3]] { let r = lay.rows.at(y); ys.extend([lay.rows.pos(r), lay.rows.pos(r + 1)]); }
    }
    if st.snap_shape {
        for (k, o) in app.book.sheets[si].objects.iter().enumerate() {
            if k == i || st.multi.contains(&k) || o.hidden { continue; }
            let r = crate::objects::sheet_rect(lay, o);
            xs.extend([r[0], r[2]]); ys.extend([r[1], r[3]]);
        }
    }
    let best = |v: f32, list: &[f32]| list.iter().map(|x| x - v).filter(|d| d.abs() <= tol).min_by(|a, b| a.abs().total_cmp(&b.abs()));
    if handle == 0 {
        let dx = best(sr[0], &xs).or_else(|| best(sr[2], &xs)).unwrap_or(0.0);
        let dy = best(sr[1], &ys).or_else(|| best(sr[3], &ys)).unwrap_or(0.0);
        sr[0] += dx; sr[2] += dx; sr[1] += dy; sr[3] += dy;
    } else {
        if matches!(handle, 1 | 7 | 8) { if let Some(d) = best(sr[0], &xs) { sr[0] += d; } }
        if matches!(handle, 3 | 4 | 5) { if let Some(d) = best(sr[2], &xs) { sr[2] += d; } }
        if matches!(handle, 1 | 2 | 3) { if let Some(d) = best(sr[1], &ys) { sr[1] += d; } }
        if matches!(handle, 5 | 6 | 7) { if let Some(d) = best(sr[3], &ys) { sr[3] += d; } }
    }
}

// ---- rotation handle -----------------------------------------------------------------------------------

fn rotatable(o: &Object) -> bool { matches!(o.kind, ObjKind::Picture { .. } | ObjKind::Shape(_) | ObjKind::Group(_)) }

/// The circular arrow above a selected picture, shape or group: drag to rotate (Shift: 15° steps).
pub fn rotate_handle(app: &mut SheetApp, ui: &mut Ui, p: &egui::Painter, _lay: &Layout, si: usize, i: usize, r: Rect) {
    if !app.book.sheets[si].objects.get(i).is_some_and(rotatable) { return; }
    let c = r.center_top() - vec2(0.0, 22.0);
    p.line_segment([r.center_top(), c + vec2(0.0, 5.0)], Stroke::new(1.0, SELECT));
    p.circle_filled(c, 5.0, Color32::WHITE);
    p.circle_stroke(c, 5.0, Stroke::new(1.4, SELECT));
    let resp = ui.interact(Rect::from_center_size(c, vec2(12.0, 12.0)), Id::new(("sheet-object-rotate", si, i)), Sense::drag());
    if resp.hovered() || resp.dragged() { ui.ctx().set_cursor_icon(egui::CursorIcon::Alias); }
    if resp.drag_started() { app.checkpoint("Rotate"); }
    if resp.dragged() {
        if let Some(pt) = resp.interact_pointer_pos() {
            let d = pt - r.center();
            let mut deg = d.x.atan2(-d.y).to_degrees().rem_euclid(360.0);
            if ui.input(|x| x.modifiers.shift) { deg = (deg / 15.0).round() * 15.0 % 360.0; }
            app.book.sheets[si].objects[i].rot = deg.round();
            app.dirty = true;
        }
    }
}

// ---- commands --------------------------------------------------------------------------------------------

/// Stacking order of the selection: ±1 one step, ±2 to the front or back.
pub fn reorder(app: &mut SheetApp, how: i32) {
    let sel = selection(app);
    if sel.is_empty() { return; }
    let si = app.sheet();
    app.checkpoint(if how > 0 { "Bring Forward" } else { "Send Backward" });
    let objs = &mut app.book.sheets[si].objects;
    let ids: Vec<u32> = sel.iter().map(|&k| objs[k].id).collect();
    let n = objs.len();
    let inset = |o: &Object| ids.contains(&o.id);
    match how {
        2 => { let (a, b): (Vec<Object>, Vec<Object>) = objs.drain(..).partition(|o| !inset(o)); objs.extend(a); objs.extend(b); }
        -2 => { let (a, b): (Vec<Object>, Vec<Object>) = objs.drain(..).partition(|o| inset(o)); objs.extend(a); objs.extend(b); }
        1 => for k in (0..n.saturating_sub(1)).rev() { if inset(&objs[k]) && !inset(&objs[k + 1]) { objs.swap(k, k + 1); } },
        _ => for k in 1..n { if inset(&objs[k]) && !inset(&objs[k - 1]) { objs.swap(k, k - 1); } },
    }
    let v: Vec<usize> = objs.iter().enumerate().filter(|(_, o)| inset(o)).map(|(k, _)| k).collect();
    app.dirty = true;
    set_selection(app, &v);
}

/// Align: 0 left, 1 center, 2 right, 3 top, 4 middle, 5 bottom (to the selection's bounds).
pub fn align(app: &mut SheetApp, how: u8) {
    let sel = selection(app);
    if sel.len() < 2 { return; }
    let si = app.sheet();
    let rs: Vec<[f32; 4]> = sel.iter().map(|&k| rect_of(app, si, k)).collect();
    let bb = rs.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, r| [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[2]), b[3].max(r[3])]);
    app.checkpoint("Align");
    for (&k, r) in sel.iter().zip(&rs) {
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let (x, y) = match how {
            0 => (bb[0], r[1]), 1 => ((bb[0] + bb[2] - w) / 2.0, r[1]), 2 => (bb[2] - w, r[1]),
            3 => (r[0], bb[1]), 4 => (r[0], (bb[1] + bb[3] - h) / 2.0), _ => (r[0], bb[3] - h),
        };
        place(app, si, k, [x, y, x + w, y + h]);
    }
    app.dirty = true;
}

/// Distribute Horizontally / Vertically: equal gaps between three or more objects.
pub fn distribute(app: &mut SheetApp, horizontal: bool) {
    let sel = selection(app);
    if sel.len() < 3 { return; }
    let si = app.sheet();
    let a = if horizontal { 0 } else { 1 };
    let mut rs: Vec<(usize, [f32; 4])> = sel.iter().map(|&k| (k, rect_of(app, si, k))).collect();
    rs.sort_by(|x, y| x.1[a].total_cmp(&y.1[a]));
    let lo = rs[0].1[a];
    let hi = rs.iter().map(|x| x.1[a + 2]).fold(f32::MIN, f32::max);
    let total: f32 = rs.iter().map(|x| x.1[a + 2] - x.1[a]).sum();
    let gap = (hi - lo - total) / (rs.len() - 1) as f32;
    app.checkpoint("Distribute");
    let mut at = lo;
    for (k, r) in rs {
        let len = r[a + 2] - r[a];
        let mut n = r;
        n[a] = at; n[a + 2] = at + len;
        place(app, si, k, n);
        at += len + gap;
    }
    app.dirty = true;
}

fn groupable(o: &Object) -> bool { matches!(o.kind, ObjKind::Picture { .. } | ObjKind::Shape(_) | ObjKind::Group(_)) || matches!(o.kind, ObjKind::Other) && o.raw.is_some() }

/// Groups the given objects (indexes on the active sheet); returns the group's index.
pub fn group_objects(app: &mut SheetApp, sel: &[usize]) -> Option<usize> {
    let si = app.sheet();
    if sel.len() < 2 { return None; }
    if !sel.iter().all(|&k| app.book.sheets[si].objects.get(k).is_some_and(groupable)) {
        app.notify(&tr("Charts, slicers and timelines can't be grouped."));
        return None;
    }
    app.checkpoint("Group");
    let items: Vec<(Object, [f32; 4])> = sel.iter().map(|&k| (app.book.sheets[si].objects[k].clone(), rect_of(app, si, k))).collect();
    let (g, bb) = group::make(items);
    let objs = &mut app.book.sheets[si].objects;
    let id = objs.iter().map(|o| o.id).max().unwrap_or(1) + 1;
    for &k in sel.iter().rev() { objs.remove(k); }
    let at = sel.last().unwrap() + 1 - sel.len();
    let mut o = Object::new(ObjKind::Group(g), Anchor::default());
    o.id = id;
    o.name = format!("Group {}", id - 1);
    objs.insert(at, o);
    place(app, si, at, bb);
    app.dirty = true;
    set_selection(app, &[at]);
    Some(at)
}

pub fn group_selection(app: &mut SheetApp) -> Option<usize> { let sel = selection(app); group_objects(app, &sel) }

/// Ungroup the selected groups: their members come back where they show.
pub fn ungroup(app: &mut SheetApp) {
    let si = app.sheet();
    let sel: Vec<usize> = selection(app).into_iter().filter(|&k| matches!(app.book.sheets[si].objects[k].kind, ObjKind::Group(_))).collect();
    if sel.is_empty() { return; }
    app.checkpoint("Ungroup");
    let mut regroup = vec![];
    let mut picked = vec![];
    for &k in sel.iter().rev() {
        let outer = rect_of(app, si, k);
        let parts = group::ungroup(&app.book.sheets[si].objects[k], outer);
        app.book.sheets[si].objects.remove(k);
        // Selected indexes above k shift by the members inserted.
        for p in picked.iter_mut() { *p += parts.len() - 1; }
        for (j, (mut o, r)) in parts.into_iter().enumerate() {
            let taken: Vec<u32> = app.book.sheets[si].objects.iter().map(|x| x.id).collect();
            if o.id == 0 || taken.contains(&o.id) { o.id = taken.iter().copied().max().unwrap_or(1) + 1; }
            o.anchor = Anchor::default();
            regroup.push(o.id);
            app.book.sheets[si].objects.insert(k + j, o);
            place(app, si, k + j, r);
            picked.push(k + j);
        }
    }
    picked.sort();
    app.dirty = true;
    set_selection(app, &picked);
    let mut st = state(&app.ctx); st.regroup = regroup; set_state(&app.ctx, st);
}

/// Regroup: the objects last ungrouped, grouped again.
pub fn regroup(app: &mut SheetApp) {
    let si = app.sheet();
    let ids = state(&app.ctx).regroup;
    let sel: Vec<usize> = app.book.sheets[si].objects.iter().enumerate().filter(|(_, o)| ids.contains(&o.id)).map(|(k, _)| k).collect();
    if group_objects(app, &sel).is_some() { let mut st = state(&app.ctx); st.regroup.clear(); set_state(&app.ctx, st); }
}

/// Rotate (degrees, clockwise) or flip the selected pictures, shapes and groups.
pub fn turn(app: &mut SheetApp, deg: f32, flip_h: bool, flip_v: bool, label: &'static str) {
    let si = app.sheet();
    let sel: Vec<usize> = selection(app).into_iter().filter(|&k| rotatable(&app.book.sheets[si].objects[k])).collect();
    if sel.is_empty() { return; }
    app.checkpoint(label);
    for k in sel {
        let o = &mut app.book.sheets[si].objects[k];
        o.rot = (o.rot + deg).rem_euclid(360.0);
        o.flip_h ^= flip_h; o.flip_v ^= flip_v;
    }
    app.dirty = true;
}

// ---- dialogs and the pane ------------------------------------------------------------------------------

#[derive(Clone)]
enum ArrangeDialog {
    /// More Rotation Options (Format ▸ Size): height, width, rotation, lock aspect ratio.
    Size { si: usize, i: usize, w: f32, h: f32, rot: f32, lock: bool },
    /// Editing the text of a group member (a SmartArt box).
    Member { si: usize, i: usize, k: usize, text: String },
}

fn adid() -> Id { Id::new("jadwal-arrange-dialog") }

fn open_size(app: &SheetApp) {
    let Some(&i) = selection(app).last() else { return };
    let si = app.sheet();
    let r = rect_of(app, si, i);
    let rot = app.book.sheets[si].objects[i].rot;
    app.ctx.data_mut(|d| d.insert_temp(adid(), Some(ArrangeDialog::Size { si, i, w: r[2] - r[0], h: r[3] - r[1], rot, lock: false })));
}

/// Double-click in a group: edit the text of the shape under the pointer.
pub fn edit_member(app: &mut SheetApp, si: usize, i: usize, r: Rect, at: Pos2) {
    let Some(o) = app.book.sheets[si].objects.get(i) else { return };
    let ObjKind::Group(g) = &o.kind else { return };
    let outer = [r.min.x, r.min.y, r.max.x, r.max.y];
    let hit = g.children.iter().enumerate().rev().find(|(_, c)| { let cr = g.child_rect(c, outer); at.x >= cr[0] && at.x <= cr[2] && at.y >= cr[1] && at.y <= cr[3] && matches!(c.kind, ObjKind::Shape(_)) });
    if let Some((k, c)) = hit {
        let text = if let ObjKind::Shape(s) = &c.kind { s.text.clone() } else { String::new() };
        app.ctx.data_mut(|d| d.insert_temp(adid(), Some(ArrangeDialog::Member { si, i, k, text })));
    }
}

/// Called every frame by the object layer: the Selection Pane and the arrange and theme dialogs.
pub fn frame(app: &mut SheetApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    crate::themes_ui::dialogs(app, &ctx);
    if state(&ctx).pane { pane(app, ui); }
    let Some(mut d) = ctx.data(|x| x.get_temp::<Option<ArrangeDialog>>(adid())).flatten() else { return };
    let mut keep = true;
    let mut open = true;
    match &mut d {
        ArrangeDialog::Size { si, i, w, h, rot, lock } => {
            crate::theme::dialog(&ctx, "Size and Rotation").open(&mut open).resizable(false).show(&ctx, |ui| {
                let (w0, h0) = (*w, *h);
                egui::Grid::new("size-rot").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                    ui.label(tr("Height:")); ui.add(egui::DragValue::new(h).range(1.0..=10000.0).suffix(" px")); ui.end_row();
                    ui.label(tr("Width:")); ui.add(egui::DragValue::new(w).range(1.0..=10000.0).suffix(" px")); ui.end_row();
                    ui.label(tr("Rotation:")); ui.add(egui::DragValue::new(rot).range(-360.0..=360.0).suffix("°")); ui.end_row();
                });
                crate::theme::check(ui, lock, tr("Lock aspect ratio"));
                if *lock && h0 > 0.0 && w0 > 0.0 { if *w != w0 { *h = *w * h0 / w0; } else if *h != h0 { *w = *h * w0 / h0; } }
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() {
                        if app.book.sheets.get(*si).is_some_and(|s| *i < s.objects.len()) {
                            app.checkpoint("Size and Rotation");
                            let r = rect_of(app, *si, *i);
                            place(app, *si, *i, [r[0], r[1], r[0] + *w, r[1] + *h]);
                            app.book.sheets[*si].objects[*i].rot = rot.rem_euclid(360.0);
                            app.dirty = true;
                        }
                        keep = false;
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
        }
        ArrangeDialog::Member { si, i, k, text } => {
            crate::theme::dialog(&ctx, "Edit Text").open(&mut open).resizable(false).show(&ctx, |ui| {
                ui.add(egui::TextEdit::multiline(text).desired_rows(4).desired_width(300.0)).request_focus();
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() {
                        if let Some(ObjKind::Group(g)) = app.book.sheets.get(*si).and_then(|s| s.objects.get(*i)).map(|o| &o.kind) {
                            if matches!(g.children.get(*k).map(|c| &c.kind), Some(ObjKind::Shape(_))) {
                                app.checkpoint("Edit Text");
                                if let ObjKind::Group(g) = &mut app.book.sheets[*si].objects[*i].kind {
                                    let c = &mut g.children[*k];
                                    if let ObjKind::Shape(s) = &mut c.kind { s.text = text.clone(); }
                                    c.touch();
                                }
                                app.dirty = true;
                            }
                        }
                        keep = false;
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
        }
    }
    ctx.data_mut(|x| x.insert_temp(adid(), if keep && open { Some(d) } else { None }));
}

pub fn toggle_pane(app: &SheetApp) { let mut st = state(&app.ctx); st.pane = !st.pane; set_state(&app.ctx, st); }

/// The Selection Pane: the sheet's objects, top first, with show/hide, reorder and rename.
fn pane(app: &mut SheetApp, ui: &mut Ui) {
    let si = app.sheet();
    let rename_id = Id::new("selection-pane-rename");
    egui::Panel::right("selection-pane").resizable(true).default_size(240.0).min_size(200.0)
        .frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin::same(10)).stroke(Stroke::new(1.0, c_line())))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Selection")).size(15.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if close_button(ui, "Close").clicked() { toggle_pane(app); } });
            });
            ui.horizontal(|ui| {
                if ui.button(tr("Show All")).clicked() { app.checkpoint("Show All"); for o in &mut app.book.sheets[si].objects { o.hidden = false; } app.dirty = true; }
                if ui.button(tr("Hide All")).clicked() { app.checkpoint("Hide All"); for o in &mut app.book.sheets[si].objects { o.hidden = true; } app.dirty = true; }
                if small(ui, "bringForward", None, false, true, "Bring Forward").clicked() { reorder(app, 1); }
                if small(ui, "sendBackward", None, false, true, "Send Backward").clicked() { reorder(app, -1); }
            });
            ui.separator();
            let sel = selection(app);
            let mut renaming: Option<(usize, String)> = ui.data(|d| d.get_temp(rename_id));
            let n = app.book.sheets[si].objects.len();
            if n == 0 { ui.label(RichText::new(tr("This sheet has no objects.")).color(c_ink3())); }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for k in (0..n).rev() {
                    let (name, hidden, members) = { let o = &app.book.sheets[si].objects[k]; (o.name.clone(), o.hidden, if let ObjKind::Group(g) = &o.kind { g.children.iter().rev().map(|c| c.name.clone()).collect::<Vec<_>>() } else { vec![] }) };
                    ui.horizontal(|ui| {
                        let eye = small(ui, if hidden { "hideSlide" } else { "showMarkup" }, None, !hidden, true, if hidden { "Show" } else { "Hide" });
                        if eye.clicked() { app.checkpoint(if hidden { "Show Object" } else { "Hide Object" }); app.book.sheets[si].objects[k].hidden = !hidden; app.dirty = true; }
                        match &mut renaming {
                            Some((rk, text)) if *rk == k => {
                                let r = ui.add(egui::TextEdit::singleline(text).desired_width(150.0));
                                r.request_focus();
                                if r.lost_focus() {
                                    let t = text.trim().to_string();
                                    if !t.is_empty() && t != name { app.checkpoint("Rename Object"); app.book.sheets[si].objects[k].name = t; app.dirty = true; }
                                    renaming = None;
                                }
                            }
                            _ => {
                                let r = ui.add(egui::Button::selectable(sel.contains(&k), if hidden { RichText::new(&name).color(c_ink3()) } else { RichText::new(&name) }).min_size(vec2(150.0, 0.0)));
                                if r.double_clicked() { renaming = Some((k, name.clone())); }
                                else if r.clicked() {
                                    if ui.input(|i| i.modifiers.command || i.modifiers.shift) { let mut v = sel.clone(); if let Some(p) = v.iter().position(|&x| x == k) { v.remove(p); } else { v.push(k); v.sort(); } set_selection(app, &v); }
                                    else { set_selection(app, &[k]); }
                                }
                            }
                        }
                    });
                    for m in members { ui.horizontal(|ui| { ui.add_space(36.0); ui.label(RichText::new(m).color(c_ink2())); }); }
                }
            });
            ui.data_mut(|d| match renaming { Some(r) => { d.insert_temp(rename_id, r); } None => { d.remove::<(usize, String)>(rename_id); } });
        });
}

/// The Arrange group (Page Layout and the Shape / Picture Format tabs).
pub fn ribbon_group(app: &mut SheetApp, ui: &mut Ui) {
    let sel = selection(app);
    let si = app.sheet();
    let any = !sel.is_empty();
    let has_group = sel.iter().any(|&k| matches!(app.book.sheets[si].objects[k].kind, ObjKind::Group(_)));
    qalam_kit::widgets::group(ui, "Arrange", |ui| {
        let (b, ba) = large_split(ui, "bringForward", "Bring Forward", "Bring the selected object forward");
        if b.clicked() && any { reorder(app, 1); }
        crate::theme::menu(&ba, |ui| {
            if menu_item(ui, "bringForward", "Bring Forward") { reorder(app, 1); ui.close(); }
            if menu_item(ui, "bringToFront", "Bring to Front") { reorder(app, 2); ui.close(); }
        });
        let (s, sa) = large_split(ui, "sendBackward", "Send Backward", "Send the selected object backward");
        if s.clicked() && any { reorder(app, -1); }
        crate::theme::menu(&sa, |ui| {
            if menu_item(ui, "sendBackward", "Send Backward") { reorder(app, -1); ui.close(); }
            if menu_item(ui, "sendToBack", "Send to Back") { reorder(app, -2); ui.close(); }
        });
        if large(ui, "selectionPane", "Selection Pane", state(&app.ctx).pane, true, "Show the list of objects on this sheet").clicked() { toggle_pane(app); }
        rows(ui, |ui| {
            let r = small_menu(ui, "alignObjLeft", "Align", "Align or distribute the selected objects");
            crate::theme::menu(&r, |ui| {
                for (k, (icon, label)) in [("alignObjLeft", "Align Left"), ("alignObjCenter", "Align Center"), ("alignObjRight", "Align Right"), ("alignObjTop", "Align Top"), ("alignObjMiddle", "Align Middle"), ("alignObjBottom", "Align Bottom")].into_iter().enumerate() {
                    if small(ui, icon, Some(label), false, sel.len() >= 2, label).clicked() { align(app, k as u8); ui.close(); }
                    if k == 2 { ui.separator(); }
                }
                ui.separator();
                if small(ui, "distributeH", Some("Distribute Horizontally"), false, sel.len() >= 3, "Distribute Horizontally").clicked() { distribute(app, true); ui.close(); }
                if small(ui, "distributeV", Some("Distribute Vertically"), false, sel.len() >= 3, "Distribute Vertically").clicked() { distribute(app, false); ui.close(); }
                ui.separator();
                let mut st = state(&app.ctx);
                let mut changed = crate::theme::check(ui, &mut st.snap_grid, tr("Snap to Grid")).changed();
                changed |= crate::theme::check(ui, &mut st.snap_shape, tr("Snap to Shape")).changed();
                if changed { set_state(&app.ctx, st); }
            });
            let r = small_menu(ui, "groupObjects", "Group", "Group objects so they act as one");
            crate::theme::menu(&r, |ui| {
                if small(ui, "groupObjects", Some("Group"), false, sel.len() >= 2, "Group").clicked() { group_selection(app); ui.close(); }
                if small(ui, "group", Some("Regroup"), false, !state(&app.ctx).regroup.is_empty(), "Regroup").clicked() { regroup(app); ui.close(); }
                if small(ui, "ungroupObjects", Some("Ungroup"), false, has_group, "Ungroup").clicked() { ungroup(app); ui.close(); }
            });
            let r = small_menu(ui, "rotate", "Rotate", "Rotate or flip the selected object");
            crate::theme::menu(&r, |ui| {
                if menu_item(ui, "rotateRight", "Rotate Right 90°") { turn(app, 90.0, false, false, "Rotate"); ui.close(); }
                if menu_item(ui, "rotateLeft", "Rotate Left 90°") { turn(app, -90.0, false, false, "Rotate"); ui.close(); }
                if menu_item(ui, "flipVertical", "Flip Vertical") { turn(app, 0.0, false, true, "Flip"); ui.close(); }
                if menu_item(ui, "flipHorizontal", "Flip Horizontal") { turn(app, 0.0, true, false, "Flip"); ui.close(); }
                ui.separator();
                if ui.add_enabled(any, egui::Button::new(tr("More Rotation Options…"))).clicked() { open_size(app); ui.close(); }
            });
        });
    });
}
