//! The slide canvas: the slide rendered by the engine (cached as a texture), selection with
//! precise hit testing, smooth dragging with smart guides, resize and rotation handles,
//! drawing new shapes, in-place text editing (shapes and table cells) with a caret and
//! selection drawn from the engine's own text layout, and table cell ranges and borders.

use crate::app::{SlidesApp, TextEdit};
use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Stroke, Ui};
use qalam_kit::colors::ACCENT;
use slides_engine::render::{self, Options};
use slides_engine::scene::{Affine, Fill, Item, Kind, Loc, Scene, Xfrm};

const GUIDE: Color32 = Color32::from_rgb(0xE8, 0x3E, 0x8C);

#[derive(Default)]
pub struct Cache {
    pub key: Option<(String, u64, u32, u32)>,
    texture: Option<egui::TextureHandle>,
    pub fit_zoom: f32,
    /// Where the slide was drawn last frame: origin and pixels per EMU (tests aim with it).
    pub view: Option<(Pos2, f32)>,
    /// While moving objects: the slide without them, and them alone (smooth dragging).
    drag_layers: Option<(Vec<Loc>, egui::TextureHandle, egui::TextureHandle)>,
}

#[derive(Clone, Debug)]
pub enum Drag {
    /// Moving objects; `delta` is the current offset (EMU) applied on release.
    Move { start: Pos2, frames: Vec<(Loc, Xfrm)>, moved: bool, delta: (f64, f64) },
    Resize { handle: (i8, i8), loc: Loc, orig: Xfrm, start: Pos2, new: Xfrm },
    Rotate { loc: Loc, orig: Xfrm, new: Xfrm },
    Marquee { start: Pos2 },
    Draw { start: Pos2 },
    Text { anchor: usize },
    /// Selecting a range of table cells.
    Cells { loc: Loc, from: (usize, usize) },
    /// Moving a table's column (or row) border: index, original size, start.
    ColumnBorder { loc: Loc, col: usize, orig: f64, start: Pos2, new: f64 },
    RowBorder { loc: Loc, row: usize, orig: f64, start: Pos2, new: f64 },
    /// Dragging a shape's yellow adjust handle.
    Adjust { loc: Loc, handle: usize },
}

/// Slide (EMU) ↔ screen.
#[derive(Clone, Copy)]
pub struct View { pub origin: Pos2, pub scale: f32 }

impl View {
    pub fn to_screen(&self, x: f64, y: f64) -> Pos2 { pos2(self.origin.x + x as f32 * self.scale, self.origin.y + y as f32 * self.scale) }
    pub fn to_slide(&self, p: Pos2) -> (f64, f64) { (((p.x - self.origin.x) / self.scale) as f64, ((p.y - self.origin.y) / self.scale) as f64) }
}

fn top_loc(it: &Item) -> Loc { Loc { part: it.loc.part.clone(), path: vec![it.loc.path[0]] } }

/// Even-odd point in polygon.
fn inside(poly: &[slides_engine::geom::Pt], x: f64, y: f64) -> bool {
    let mut c = false;
    let n = poly.len();
    if n < 3 { return false; }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > y) != (b.y > y) && x < (b.x - a.x) * (y - a.y) / (b.y - a.y) + a.x { c = !c; }
        j = i;
    }
    c
}

fn near_polyline(poly: &[slides_engine::geom::Pt], x: f64, y: f64, tol: f64) -> bool {
    poly.windows(2).any(|w| {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((x - a.x) * dx + (y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let (px, py) = (a.x + t * dx - x, a.y + t * dy - y);
        px * px + py * py <= tol * tol
    })
}

/// Does a point (slide EMU) hit an item's visible ink: its filled shape, outline, text or
/// picture (not the empty part of its box)?
fn solid_hit(it: &Item, x: f64, y: f64, tol: f64) -> bool {
    let Some(inv) = it.transform.invert() else { return false };
    let (lx, ly) = inv.apply(x, y);
    let k = it.transform.a.hypot(it.transform.b).max(1e-9);
    let t = tol / k;
    match &it.kind {
        Kind::Picture { .. } | Kind::Chart(_) | Kind::Table(_) | Kind::Frame { .. } => lx >= -t && ly >= -t && lx <= it.w + t && ly <= it.h + t,
        Kind::Shape { geom, fill, line, text } => {
            for p in &geom.paths {
                let polys = slides_engine::geom::flatten(&p.cmds, 8);
                let filled = p.fill != slides_engine::geom::FillMode::None && !matches!(fill, Fill::None);
                for poly in &polys {
                    if filled && inside(poly, lx, ly) { return true; }
                    if (line.is_some() || !filled) && near_polyline(poly, lx, ly, t.max(line.as_ref().map(|l| l.width / 2.0).unwrap_or(0.0))) && (line.is_some() || it.w < t * 3.0 || it.h < t * 3.0) { return true; }
                }
            }
            // Lines and connectors (zero width or height) are hit near their path.
            if (it.w < t || it.h < t) && lx >= -t && ly >= -t && lx <= it.w + t && ly <= it.h + t { return true; }
            // Text: over its laid-out lines.
            if text.as_ref().is_some_and(|b| b.paras.iter().any(|p| p.runs.iter().any(|r| !r.text.is_empty()))) {
                if let Some((laid, tt)) = render::text_layout_for(it) {
                    if let Some(ti) = tt.invert() {
                        let (px, py) = ti.apply(x, y);
                        let pad = tol / 12_700.0;
                        return laid.lines.iter().any(|l| px >= l.x0.min(l.x1) - pad && px <= l.x1.max(l.x0) + pad && py >= l.top - pad && py <= l.top + l.height + pad);
                    }
                }
            }
            false
        }
    }
}

fn in_box(it: &Item, x: f64, y: f64, tol: f64) -> bool {
    let Some(inv) = it.transform.invert() else { return false };
    let (lx, ly) = inv.apply(x, y);
    let t = tol / it.transform.a.hypot(it.transform.b).max(1e-9);
    lx >= -t && ly >= -t && lx <= it.w + t && ly <= it.h + t
}

/// The item a click picks: a selected object under the pointer first (so it can be dragged
/// from anywhere in its box), then the topmost visible ink, then the smallest text box
/// (empty placeholders) containing the point.
fn pick(scene: &Scene, selection: &[Loc], x: f64, y: f64, tol: f64) -> Option<usize> {
    let own = |it: &Item| it.own && !it.hidden;
    if let Some(i) = scene.items.iter().enumerate().rev().find(|(_, it)| own(it) && selection.iter().any(|l| l.part == it.loc.part && l.path[0] == it.loc.path[0]) && in_box(it, x, y, tol)).map(|(i, _)| i) {
        // Unless something unselected is drawn on top of it right there.
        let above = scene.items.iter().enumerate().skip(i + 1).rev().find(|(_, it)| own(it) && !selection.iter().any(|l| l.path[0] == it.loc.path[0]) && solid_hit(it, x, y, tol)).map(|(k, _)| k);
        return above.or(Some(i));
    }
    if let Some(i) = scene.items.iter().enumerate().rev().find(|(_, it)| own(it) && solid_hit(it, x, y, tol)).map(|(i, _)| i) { return Some(i); }
    scene.items.iter().enumerate().filter(|(_, it)| own(it) && in_box(it, x, y, tol) && matches!(&it.kind, Kind::Shape { text: Some(_), .. }))
        .min_by(|a, b| (a.1.w * a.1.h).total_cmp(&(b.1.w * b.1.h))).map(|(i, _)| i)
}

/// Corners of a box on screen.
fn corners(view: &View, t: &Affine, w: f64, h: f64) -> [Pos2; 4] {
    [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| { let (sx, sy) = t.apply(x, y); view.to_screen(sx, sy) })
}

/// Is a screen point on the border band of a frame (inside within `band` px)?
fn on_border(view: &View, x: &Xfrm, p: Pos2, band: f32) -> bool {
    let Some(inv) = x.matrix().invert() else { return false };
    let (sx, sy) = view.to_slide(p);
    let (lx, ly) = inv.apply(sx, sy);
    let b = (band / view.scale) as f64;
    let inside_outer = lx >= -b && ly >= -b && lx <= x.w + b && ly <= x.h + b;
    let inside_inner = lx > b && ly > b && lx < x.w - b && ly < x.h - b;
    inside_outer && !inside_inner
}

pub fn canvas(app: &mut SlidesApp, ui: &mut Ui) {
    let Some(slide) = app.slide() else {
        ui.centered_and_justified(|ui| if ui.button(crate::i18n::tr("Click to add first slide")).clicked() { app.new_slide(None); });
        return;
    };
    let scene = app.scene_of(&slide);
    let avail = ui.available_rect_before_wrap();
    let margin = 28.0;
    let fit = ((avail.width() - margin * 2.0) / scene.w as f32).min((avail.height() - margin * 2.0) / scene.h as f32).max(1e-7);
    // Zoom is relative to 100% = 96 px per inch.
    let px_per_emu_100 = 1.0 / 9525.0;
    app.canvas.fit_zoom = fit / px_per_emu_100;
    let scale = app.zoom.map(|z| z * px_per_emu_100).unwrap_or(fit);
    let size = vec2(scene.w as f32 * scale, scene.h as f32 * scale);
    let content = vec2(size.x + margin * 2.0, size.y + margin * 2.0);
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        let (outer, resp) = ui.allocate_exact_size(vec2(content.x.max(avail.width()), content.y.max(avail.height())), Sense::click_and_drag());
        let origin = pos2(outer.center().x - size.x / 2.0, outer.center().y - size.y / 2.0);
        let view = View { origin, scale };
        app.canvas.view = Some((origin, scale));
        let slide_rect = Rect::from_min_size(origin, size);
        let ppp = ui.ctx().pixels_per_point();
        let (pw, ph) = ((size.x * ppp).round() as u32, (size.y * ppp).round() as u32);
        let key = (slide.clone(), app.revision, pw, ph);
        if app.canvas.key.as_ref() != Some(&key) {
            if let Some(pm) = render::render(&app.pres, &scene, &mut app.images, Options { scale: (scale * ppp) as f64, prompts: true, skip_text_of: None, translate: Some(crate::i18n::raw) }) {
                let mut img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
                crate::views::recolor(&mut img, app.color_mode);
                match &mut app.canvas.texture { Some(t) => t.set(img, egui::TextureOptions::LINEAR), None => app.canvas.texture = Some(ui.ctx().load_texture("slide", img, egui::TextureOptions::LINEAR)) }
            }
            app.canvas.key = Some(key);
        }
        let painter = ui.painter_at(outer);
        painter.rect_filled(slide_rect.translate(vec2(0.0, 2.0)).expand(1.0), 0.0, Color32::from_black_alpha(40));
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        // While moving: the slide without the moved objects, then them at the new place.
        match (&app.drag, &app.canvas.drag_layers) {
            (Some(Drag::Move { delta, moved: true, .. }), Some((_, bg, fg))) => {
                painter.image(bg.id(), slide_rect, uv, Color32::WHITE);
                let off = vec2(delta.0 as f32 * scale, delta.1 as f32 * scale);
                painter.with_clip_rect(outer).image(fg.id(), slide_rect.translate(off), uv, Color32::WHITE);
            }
            _ => { if let Some(t) = &app.canvas.texture { painter.image(t.id(), slide_rect, uv, Color32::WHITE); } }
        }
        // A transition preview plays over the slide.
        if let (Some((start, from)), Some(to)) = (app.preview.clone(), app.canvas.texture.clone()) {
            let tr = app.pres.slide_transition(&slide);
            let dur = (tr.duration_ms.max(1) as f64) / 1000.0;
            let t = ((ui.input(|i| i.time) - start) / dur) as f32;
            if t >= 1.0 || tr.effect == "none" { app.preview = None; }
            else {
                painter.rect_filled(slide_rect, 0.0, Color32::BLACK);
                if let (Some(m), "morph") = (&app.morph_preview, tr.effect.as_str()) { crate::morph::draw(&painter.with_clip_rect(slide_rect), m, t, slide_rect); }
                else { crate::transition_fx::draw(&painter.with_clip_rect(slide_rect), &tr.effect, &tr.option, t, slide_rect, from.as_ref().map(|f| f.id()), to.id()); }
                ui.ctx().request_repaint();
            }
        }
        // An animation preview plays the slide's effects over it.
        if let Some((start, layers)) = &app.anim_preview {
            let ms = (ui.input(|i| i.time) - start) * 1000.0;
            let el = crate::anim_play::preview_elapsed(&layers.anims, ms);
            if ms > crate::anim_play::preview_length(&layers.anims) + 300.0 { app.anim_preview = None; }
            else { crate::anim_play::paint(&painter.with_clip_rect(slide_rect), slide_rect, layers, &|i| el[i]); ui.ctx().request_repaint(); }
        } else if app.tab == crate::app::Tab::Animations {
            let to = |x: f64, y: f64| pos2(slide_rect.min.x + x as f32 * scale, slide_rect.min.y + y as f32 * scale);
            crate::animations_tab::tags(app, &painter, &slide, &to);
        }
        // Clicking the slide takes the keyboard back from other fields (notes, size box).
        if resp.clicked() || resp.drag_started() || resp.secondary_clicked() { if let Some(id) = ui.ctx().memory(|m| m.focused()) { ui.ctx().memory_mut(|m| m.surrender_focus(id)); } }
        // Right-click selects what's under the pointer, then shows its menu.
        if resp.secondary_clicked() {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                let (x, y) = view.to_slide(p);
                match pick(&scene, &app.selection, x, y, 5.0 / view.scale as f64) {
                    Some(i) => {
                        let l = top_loc(&scene.items[i]);
                        if !app.selection.contains(&l) { app.selection = vec![l.clone()]; app.edit = None; }
                        // On a table, the cell under the pointer is what table commands act on.
                        if let Some(cell) = table_cell(&scene, i, x, y) {
                            let inside = app.cell_sel.as_ref().is_some_and(|(cl, a, b)| *cl == l && (a.0.min(b.0)..=a.0.max(b.0)).contains(&cell.0) && (a.1.min(b.1)..=a.1.max(b.1)).contains(&cell.1));
                            let editing_it = app.edit.as_ref().is_some_and(|e| e.loc == l && e.cell == Some(cell));
                            if !inside && !editing_it { app.edit = None; app.cell_sel = Some((l, cell, cell)); }
                        }
                    }
                    None => { app.selection.clear(); app.edit = None; }
                }
            }
        }
        resp.context_menu(|ui| crate::theme::full_width(ui, |ui| context_menu(app, ui)));
        if app.show_grid {
            // Gridlines at the grid's spacing (every other one when they'd crowd).
            let mut step = app.grid_spacing.max(9_144.0);
            while step * (view.scale as f64) < 8.0 { step *= 2.0; }
            let mut x = step; while x < scene.w { let a = view.to_screen(x, 0.0); painter.vline(a.x, slide_rect.y_range(), Stroke::new(0.5, Color32::from_black_alpha(28))); x += step; }
            let mut y = step; while y < scene.h { let a = view.to_screen(0.0, y); painter.hline(slide_rect.x_range(), a.y, Stroke::new(0.5, Color32::from_black_alpha(28))); y += step; }
        }
        if app.show_guides {
            let c = slide_rect.center();
            painter.add(egui::Shape::dashed_line(&[pos2(c.x, slide_rect.min.y), pos2(c.x, slide_rect.max.y)], Stroke::new(1.0, Color32::from_gray(130)), 4.0, 4.0));
            painter.add(egui::Shape::dashed_line(&[pos2(slide_rect.min.x, c.y), pos2(slide_rect.max.x, c.y)], Stroke::new(1.0, Color32::from_gray(130)), 4.0, 4.0));
        }
        crate::views::rulers(app, &painter, ui.clip_rect(), slide_rect, scale);
        interact(app, ui, &resp, &scene, &view, ppp);
        selection_keys(app, ui.ctx(), &scene);
        draw_hover(app, ui, &painter, &scene, &view);
        draw_selection(app, ui, &painter, &scene, &view);
        crate::three_d_ui::rotate_handle(app, ui, &scene, &|x, y| view.to_screen(x, y));
        // Media: the selected one gets its controls, and plays in place.
        if app.player.as_ref().is_some_and(|p| !slide_media(app, &slide).contains(&p.part)) { app.player = None; }
        let selected_media = app.selection.first().filter(|l| l.path.len() == 1).and_then(|l| app.pres.media_part(l));
        if app.player.is_some() || selected_media.is_some() {
            if let (Some(m), None) = (&selected_media, &app.player) {
                // A selected media object shows a play bar to start it.
                let loc = app.selection[0].clone();
                if let Some((.., r)) = crate::media::objects(&app.pres, &slide, slide_rect).into_iter().find(|o| &o.1 == m) {
                    let b = Rect::from_min_size(pos2(r.min.x, r.max.y + 4.0), vec2(30.0, 24.0));
                    painter.rect_filled(b, 4.0, Color32::from_black_alpha(170));
                    painter.add(egui::Shape::convex_polygon(vec![b.min + vec2(11.0, 6.0), b.min + vec2(21.0, 12.0), b.min + vec2(11.0, 18.0)], Color32::WHITE, Stroke::NONE));
                    if ui.interact(b, egui::Id::new("media-start"), Sense::click()).on_hover_text(crate::i18n::tr("Play")).clicked() {
                        let video = app.pres.element(&loc).is_some_and(|e| e.find("a:videoFile").is_some());
                        crate::media::toggle(&mut app.player, &app.pres, m, video);
                    }
                }
            }
            crate::media::overlay(&mut app.player, &app.pres, ui, &slide, slide_rect, true);
        }
        if app.edit.is_some() { text_input(app, ui, &scene, &view); }
        if app.insert_mode.is_some() { ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair); }
        if app.painter.is_some() || app.anim_painter.is_some() { ui.ctx().set_cursor_icon(egui::CursorIcon::Copy); }
    });
}

/// Handle positions (screen) for a box, with their (hx, hy) codes.
fn handles(c: &[Pos2; 4]) -> Vec<((i8, i8), Pos2)> {
    let mid = |a: Pos2, b: Pos2| pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    vec![((-1, -1), c[0]), ((0, -1), mid(c[0], c[1])), ((1, -1), c[1]), ((1, 0), mid(c[1], c[2])), ((1, 1), c[2]), ((0, 1), mid(c[2], c[3])), ((-1, 1), c[3]), ((-1, 0), mid(c[3], c[0]))]
}

/// The selected preset shape's adjust handles on screen: (index, position).
fn adjust_handles(app: &SlidesApp, scene: &Scene, view: &View) -> Vec<(usize, Pos2)> {
    if app.selection.len() != 1 { return vec![]; }
    let loc = &app.selection[0];
    let Some(it) = scene.items.iter().find(|i| i.own && i.loc == *loc) else { return vec![] };
    let Kind::Shape { geom, .. } = &it.kind else { return vec![] };
    if app.pres.element(loc).and_then(|e| e.path("p:spPr/a:prstGeom")).is_none() { return vec![]; }
    geom.handles.iter().enumerate().map(|(k, h)| { let (x, y) = it.transform.apply(h.at.x, h.at.y); (k, view.to_screen(x, y)) }).collect()
}

fn rotate_handle(c: &[Pos2; 4]) -> Pos2 {
    let top = pos2((c[0].x + c[1].x) / 2.0, (c[0].y + c[1].y) / 2.0);
    let bottom = pos2((c[3].x + c[2].x) / 2.0, (c[3].y + c[2].y) / 2.0);
    let d = (top - bottom).normalized();
    top + d * 22.0
}

/// The selected single object's frame on screen (for handles), if exactly one is selected.
fn single_frame(app: &SlidesApp, scene: &Scene, view: &View) -> Option<(Loc, Xfrm, [Pos2; 4])> {
    if app.selection.len() != 1 { return None; }
    let loc = app.selection[0].clone();
    let x = app.frame_of(scene, &loc)?;
    Some((loc, x, corners(view, &x.matrix(), x.w, x.h)))
}

fn resize_cursor(h: (i8, i8), rot: f64) -> egui::CursorIcon {
    let a = ((h.1 as f64).atan2(h.0 as f64).to_degrees() + rot).rem_euclid(180.0);
    if !(22.5..157.5).contains(&a) { egui::CursorIcon::ResizeHorizontal } else if a < 67.5 { egui::CursorIcon::ResizeNwSe } else if a < 112.5 { egui::CursorIcon::ResizeVertical } else { egui::CursorIcon::ResizeNeSw }
}

/// A table under a point: (item index, row, column) in table space.
fn table_cell(scene: &Scene, i: usize, x: f64, y: f64) -> Option<(usize, usize)> {
    let it = &scene.items[i];
    let Kind::Table(t) = &it.kind else { return None };
    let (lx, ly) = it.transform.invert()?.apply(x, y);
    t.cell_at(lx, ly)
}

/// A table's internal border near a point: Ok(column border index) or Err(row border index).
fn table_border(scene: &Scene, loc: &Loc, x: f64, y: f64, tol: f64) -> Option<Result<usize, usize>> {
    let it = scene.items.iter().find(|i| i.own && i.loc.path == loc.path)?;
    let Kind::Table(t) = &it.kind else { return None };
    let (lx, ly) = it.transform.invert()?.apply(x, y);
    if lx < -tol || ly < -tol || lx > it.w + tol || ly > it.h + tol { return None; }
    let (xs, ys) = t.edges();
    if let Some(c) = (1..xs.len()).find(|&k| (lx - xs[k]).abs() < tol) { return Some(Ok(c - 1)); }
    if let Some(r) = (1..ys.len()).find(|&k| (ly - ys[k]).abs() < tol) { return Some(Err(r - 1)); }
    None
}

fn interact(app: &mut SlidesApp, ui: &mut Ui, resp: &egui::Response, scene: &Scene, view: &View, ppp: f32) {
    let hover = ui.input(|i| i.pointer.hover_pos());
    // Shift (or ⌘/Ctrl) adds to the selection: read from the press itself as well as the keyboard.
    let mods_id = egui::Id::new("press-modifiers");
    if let Some(m) = ui.input(|i| i.events.iter().rev().find_map(|e| if let egui::Event::PointerButton { modifiers, pressed: true, .. } = e { Some(*modifiers) } else { None })) { ui.data_mut(|d| d.insert_temp(mods_id, m)); }
    let press_mods: Option<egui::Modifiers> = ui.data(|d| d.get_temp(mods_id));
    let mods = ui.input(|i| i.modifiers);
    let shift = mods.shift || press_mods.is_some_and(|m| m.shift);
    let toggle = shift || mods.command || press_mods.is_some_and(|m| m.command);
    let tol = 5.0 / view.scale as f64;
    // Cursors: handles, borders, table borders.
    if let (Some(p), None) = (hover, &app.drag) {
        let (x, y) = view.to_slide(p);
        if adjust_handles(app, scene, view).iter().any(|(_, hp)| hp.distance(p) < 7.0) { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        else if let Some((loc, f, c)) = single_frame(app, scene, view) {
            if let Some((h, _)) = handles(&c).into_iter().find(|(_, hp)| hp.distance(p) < 7.0) { ui.ctx().set_cursor_icon(resize_cursor(h, f.rot)); }
            else if rotate_handle(&c).distance(p) < 8.0 { ui.ctx().set_cursor_icon(egui::CursorIcon::Alias); }
            else if let Some(b) = table_border(scene, &loc, x, y, tol) { ui.ctx().set_cursor_icon(if b.is_ok() { egui::CursorIcon::ResizeColumn } else { egui::CursorIcon::ResizeRow }); }
            else if on_border(view, &f, p, 6.0) { ui.ctx().set_cursor_icon(egui::CursorIcon::Move); }
            else if app.edit.is_some() && in_box(scene.items.iter().find(|i| i.loc.path == loc.path).unwrap_or(&scene.items[0]), x, y, 0.0) { ui.ctx().set_cursor_icon(egui::CursorIcon::Text); }
        }
    }
    // A press: where it happened (not where the drag was noticed).
    let pressed = resp.drag_started() || resp.clicked() || resp.double_clicked();
    if pressed {
        let p = if resp.drag_started() { ui.input(|i| i.pointer.press_origin()).or(hover) } else { ui.input(|i| i.pointer.interact_pos()) };
        let Some(p) = p else { return };
        let (x, y) = view.to_slide(p);
        // Drawing a new shape.
        if app.insert_mode.is_some() {
            if resp.clicked() {
                let preset = app.insert_mode.clone().unwrap();
                let (w, h) = if preset == "textbox" { (2_743_200.0, 369_332.0) } else if preset == "table" { (6_096_000.0, 1_112_520.0) } else { (1_828_800.0, 1_371_600.0) };
                if preset == "table" { if let Some(s) = app.slide() { app.checkpoint("insert"); if let Some(l) = app.pres.insert_table(&s, 3, 5, &Xfrm { x: x - w / 2.0, y: y - h / 2.0, w, h, ..Default::default() }) { app.selection = vec![l]; } } app.insert_mode = None; }
                else { app.insert_shape(&preset, Xfrm { x: x - w / 2.0, y: y - h / 2.0, w, h, ..Default::default() }); }
            } else if resp.drag_started() { app.drag = Some(Drag::Draw { start: p }); }
            return;
        }
        // Yellow adjust handles come first (they sit inside the shape).
        if let Some((k, _)) = adjust_handles(app, scene, view).into_iter().find(|(_, hp)| hp.distance(p) < 7.0) {
            if resp.drag_started() { app.checkpoint("adjust"); app.drag = Some(Drag::Adjust { loc: app.selection[0].clone(), handle: k }); }
            return;
        }
        // Handles, borders of the selected object.
        if let Some((loc, orig, c)) = single_frame(app, scene, view) {
            if let Some((h, _)) = handles(&c).into_iter().find(|(_, hp)| hp.distance(p) < 7.0) {
                if resp.drag_started() { app.drag = Some(Drag::Resize { handle: h, loc, orig, start: p, new: orig }); }
                return;
            }
            if rotate_handle(&c).distance(p) < 8.0 {
                if resp.drag_started() { app.drag = Some(Drag::Rotate { loc, orig, new: orig }); }
                return;
            }
            if let Some(b) = table_border(scene, &loc, x, y, tol) {
                if resp.drag_started() {
                    let it = scene.items.iter().find(|i| i.own && i.loc.path == loc.path);
                    if let Some(Kind::Table(t)) = it.map(|i| &i.kind) {
                        app.drag = Some(match b { Ok(c) => Drag::ColumnBorder { loc, col: c, orig: t.cols[c], start: p, new: t.cols[c] }, Err(r) => Drag::RowBorder { loc, row: r, orig: t.rows[r], start: p, new: t.rows[r] } });
                    }
                }
                return;
            }
            // The border band moves the object, even while typing in it.
            if on_border(view, &orig, p, 6.0) && resp.drag_started() { start_move(app, scene, p, ppp, view); return; }
        }
        // Editing text: clicks and drags inside the edited box place the caret / select.
        if let Some(e) = app.edit.clone() {
            if let Some(ci) = edit_hit(scene, &e, x, y) {
                let anchor = if shift { e.anchor } else { ci };
                app.edit = Some(TextEdit { cursor: ci, anchor, ..e.clone() });
                if resp.double_clicked() { select_word(app); }
                if resp.drag_started() { app.drag = Some(Drag::Text { anchor }); }
                return;
            }
            // Another cell of the same table.
            if e.cell.is_some() {
                if let Some(i) = scene.items.iter().position(|it| it.loc == e.loc) {
                    if let Some(cell) = table_cell(scene, i, x, y) {
                        if resp.drag_started() { app.edit = None; app.cell_sel = Some((e.loc.clone(), cell, cell)); app.drag = Some(Drag::Cells { loc: e.loc.clone(), from: cell }); return; }
                        start_cell_edit(app, scene, &e.loc, cell, x, y);
                        return;
                    }
                }
            }
            app.edit = None;
        }
        // A drag that starts on empty space (even inside an empty placeholder) draws a
        // selection box rather than moving the placeholder.
        let picked = pick(scene, &app.selection, x, y, tol).filter(|&i| {
            if !resp.drag_started() { return true; }
            let it = &scene.items[i];
            solid_hit(it, x, y, tol) || app.selection.iter().any(|l| l.path[0] == it.loc.path[0])
        });
        match picked {
            Some(i) => {
                let it = &scene.items[i];
                let loc = top_loc(it);
                // Inside a SmartArt graphic, a click also picks the item under it.
                if resp.clicked() || resp.drag_started() {
                    let child = slides_engine::scene::Loc { part: it.loc.part.clone(), path: it.loc.path.iter().take(2).copied().collect() };
                    app.diagram_item = (it.loc.path.len() >= 2).then(|| app.pres.diagram_item(&child)).flatten().filter(|_| app.pres.diagram(&loc).is_some()).map(|k| (loc.clone(), k));
                }
                if app.painter.is_some() && resp.clicked() { app.selection = vec![loc.clone()]; app.apply_format(&loc); return; }
                if app.anim_painter.is_some() && resp.clicked() { app.selection = vec![loc.clone()]; crate::animations_tab::paint_onto(app, &loc); return; }
                // Double-click opens the object's own editor.
                if resp.double_clicked() {
                    app.selection = vec![loc.clone()];
                    if let Some(chart) = app.pres.chart(&loc) { app.dialog = Some(crate::dialogs::Dialog::ChartData { loc, chart }); return; }
                    if crate::smartart_ui::edit_selected(app, &loc) { return; }
                    if let Some(eq) = app.pres.equation(&loc) { app.dialog = Some(crate::dialogs::Dialog::Equation { loc: Some(loc), linear: eq }); return; }
                    if let Some((name, bytes)) = app.pres.object_file(&loc) { open_file(&name, &bytes); return; }
                    if let Some(m) = app.pres.media_part(&loc) { if let Some(b) = app.pres.pkg.bytes(&m) { open_file(m.rsplit('/').next().unwrap_or("media"), &b); } return; }
                }
                if resp.clicked() && ui.input(|i| i.modifiers.command) {
                    if let Some((kind, target)) = app.pres.shape_link(&loc) { follow_link(app, &kind, &target); return; }
                }
                // Tables: a click in a selected table edits that cell; a drag selects cells.
                if matches!(it.kind, Kind::Table(_)) && it.loc.path.len() == 1 {
                    let was_selected = app.selection.contains(&loc);
                    if let Some(cell) = table_cell(scene, i, x, y) {
                        if resp.drag_started() && was_selected { app.cell_sel = Some((loc.clone(), cell, cell)); app.drag = Some(Drag::Cells { loc, from: cell }); return; }
                        if (resp.clicked() && was_selected) || resp.double_clicked() { app.selection = vec![loc.clone()]; start_cell_edit(app, scene, &loc, cell, x, y); return; }
                    }
                }
                let has_text = matches!(&it.kind, Kind::Shape { text: Some(_), .. }) && it.loc.path.len() == 1;
                // Empty placeholders and text boxes take a click as typing; shapes are only
                // typed into over their existing text (else a click selects them).
                let text_box = it.placeholder.is_some() || matches!(&it.kind, Kind::Shape { fill: Fill::None, line: None, .. });
                let over_text = has_text && ((text_box && is_empty_text(it)) || text_hit_strict(it, x, y));
                if toggle && resp.clicked() {
                    // (Edits in progress end; the object joins or leaves the selection.)
                    app.edit = None;
                    if let Some(k) = app.selection.iter().position(|l| *l == loc) { app.selection.remove(k); } else { app.selection.push(loc.clone()); }
                    return;
                }
                // A drag from empty space inside an unselected text box (not on its text or
                // ink) draws a selection box, as from anywhere else that's empty.
                if resp.drag_started() && !app.selection.contains(&loc) && !solid_hit(it, x, y, 3.0 / view.scale as f64) && !text_hit_strict(it, x, y) {
                    if !toggle { app.selection.clear(); app.cell_sel = None; }
                    app.edit = None;
                    app.drag = Some(Drag::Marquee { start: p });
                    return;
                }
                if toggle && resp.drag_started() && !app.selection.contains(&loc) { app.selection.push(loc.clone()); start_move(app, scene, p, ppp, view); return; }
                let was_selected = app.selection.contains(&loc);
                if !was_selected { app.selection = vec![loc.clone()]; app.cell_sel = None; }
                if resp.drag_started() { start_move(app, scene, p, ppp, view); return; }
                // A click on text (or an empty placeholder) types there, as in PowerPoint;
                // a double-click selects the word.
                if (resp.clicked() || resp.double_clicked()) && over_text {
                    let ci = text_hit(it, x, y).unwrap_or(0);
                    app.edit = Some(TextEdit { loc, cell: None, cursor: ci, anchor: ci });
                    if resp.double_clicked() { select_word(app); }
                }
            }
            None => {
                if !toggle { app.selection.clear(); app.cell_sel = None; }
                if resp.drag_started() { app.drag = Some(Drag::Marquee { start: p }); }
            }
        }
        return;
    }
    let Some(drag) = app.drag.clone() else { return };
    let Some(p) = hover.or(ui.input(|i| i.pointer.interact_pos())) else { return };
    let done = resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down());
    let alt = ui.input(|i| i.modifiers.alt);
    match drag {
        Drag::Move { start, frames, moved, .. } => {
            let d = p - start;
            if !moved && d.length() < 3.0 { if done { app.drag = None; app.canvas.drag_layers = None; } return; }
            let (mut dx, mut dy) = ((d.x / view.scale) as f64, (d.y / view.scale) as f64);
            if shift { if dx.abs() > dy.abs() { dy = 0.0; } else { dx = 0.0; } }
            let mut guides = vec![];
            if !alt && !frames.is_empty() {
                let (sx, sy, g) = smart_snap(app, scene, &frames, dx, dy, view);
                dx = sx; dy = sy; guides = g;
            }
            draw_guides(ui, view, &guides);
            // Outlines at the new place.
            for (_, f) in &frames { let c = corners(view, &Xfrm { x: f.x + dx, y: f.y + dy, ..*f }.matrix(), f.w, f.h); for k in 0..4 { ui.painter().line_segment([c[k], c[(k + 1) % 4]], Stroke::new(1.0, ACCENT)); } }
            if done {
                app.checkpoint("move");
                for (l, f) in &frames { app.pres.set_xfrm(l, &Xfrm { x: f.x + dx, y: f.y + dy, ..*f }); }
                app.changed();
                app.drag = None;
                app.canvas.drag_layers = None;
            } else {
                app.drag = Some(Drag::Move { start, frames, moved: true, delta: (dx, dy) });
                ui.ctx().request_repaint();
            }
        }
        Drag::Resize { handle, loc, orig, start, .. } => {
            let d = p - start;
            let (s, c) = orig.rot.to_radians().sin_cos();
            let (wx, wy) = ((d.x / view.scale) as f64, (d.y / view.scale) as f64);
            let (mut lx, mut ly) = (wx * c + wy * s, -wx * s + wy * c);
            let (hx, hy) = (handle.0 as f64, handle.1 as f64);
            // Unrotated edges snap to the slide and other objects.
            if !alt && orig.rot.abs() < 0.01 {
                let (sw, sh) = app.pres.size();
                let others = other_frames(app, scene, std::slice::from_ref(&loc));
                let snap1 = |pos: f64, targets: &[f64]| -> f64 { targets.iter().copied().find(|t| (pos - t).abs() < 6.0 / view.scale as f64).map(|t| t - pos).unwrap_or(0.0) };
                let xs: Vec<f64> = [0.0, sw / 2.0, sw].into_iter().chain(others.iter().flat_map(|f| [f.x, f.x + f.w])).collect();
                let ys: Vec<f64> = [0.0, sh / 2.0, sh].into_iter().chain(others.iter().flat_map(|f| [f.y, f.y + f.h])).collect();
                if handle.0 == 1 { lx += snap1(orig.x + orig.w + lx, &xs); } else if handle.0 == -1 { lx -= snap1(orig.x - lx, &xs); }
                if handle.1 == 1 { ly += snap1(orig.y + orig.h + ly, &ys); } else if handle.1 == -1 { ly -= snap1(orig.y - ly, &ys); }
            }
            let mut nw = (orig.w + hx * lx).max(12_700.0);
            let mut nh = (orig.h + hy * ly).max(if orig.h < 1.0 { 0.0 } else { 12_700.0 });
            let is_pic = app.pres.element(&loc).is_some_and(|e| e.local() == "pic");
            if (shift ^ is_pic) && handle.0 != 0 && handle.1 != 0 && orig.w > 0.0 && orig.h > 0.0 {
                let k = (nw / orig.w).max(nh / orig.h);
                nw = orig.w * k; nh = orig.h * k;
            }
            let (sx, sy) = (hx * (nw - orig.w) / 2.0, hy * (nh - orig.h) / 2.0);
            let (cx, cy) = (orig.x + orig.w / 2.0 + sx * c - sy * s, orig.y + orig.h / 2.0 + sx * s + sy * c);
            let new = Xfrm { x: cx - nw / 2.0, y: cy - nh / 2.0, w: nw, h: nh, ..orig };
            draw_ghost(ui, scene, view, &loc, &new);
            if done { app.checkpoint("resize"); app.pres.set_xfrm(&loc, &new); app.drag = None; }
            else { app.drag = Some(Drag::Resize { handle, loc, orig, start, new }); ui.ctx().request_repaint(); }
        }
        Drag::Rotate { loc, orig, .. } => {
            let c = view.to_screen(orig.x + orig.w / 2.0, orig.y + orig.h / 2.0);
            let mut a = ((p.y - c.y).atan2(p.x - c.x).to_degrees() + 90.0) as f64;
            if shift { a = (a / 15.0).round() * 15.0; }
            let new = Xfrm { rot: a.rem_euclid(360.0), ..orig };
            draw_ghost(ui, scene, view, &loc, &new);
            ui.painter().text(p + vec2(14.0, 14.0), egui::Align2::LEFT_TOP, format!("{:.0}°", new.rot), egui::FontId::proportional(12.0), ACCENT);
            if done { app.checkpoint("rotate"); app.pres.set_xfrm(&loc, &new); app.drag = None; }
            else { app.drag = Some(Drag::Rotate { loc, orig, new }); ui.ctx().request_repaint(); }
        }
        Drag::Marquee { start } => {
            let r = Rect::from_two_pos(start, p);
            ui.painter().rect(r, 0.0, ACCENT.gamma_multiply(0.08), Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
            // Objects inside (or touched, holding Alt) are selected as the box grows.
            let (x0, y0) = view.to_slide(r.min);
            let (x1, y1) = view.to_slide(r.max);
            let mut sel: Vec<Loc> = vec![];
            for it in scene.items.iter().filter(|i| i.own && !i.hidden) {
                let cs = [(0.0, 0.0), (it.w, 0.0), (it.w, it.h), (0.0, it.h)].map(|(a, b)| it.transform.apply(a, b));
                let all_in = cs.iter().all(|(x, y)| *x >= x0 && *x <= x1 && *y >= y0 && *y <= y1);
                let touches = cs.iter().any(|(x, y)| *x >= x0 && *x <= x1 && *y >= y0 && *y <= y1);
                if all_in || (alt && touches) { let l = top_loc(it); if !sel.contains(&l) { sel.push(l); } }
            }
            if !toggle { app.selection = sel; } else { for l in sel { if !app.selection.contains(&l) { app.selection.push(l); } } }
            if done { app.drag = None; }
        }
        Drag::Draw { start } => {
            let mut q = p;
            // Shift draws squares, circles and straight lines.
            if shift { let d = p - start; let m = d.x.abs().max(d.y.abs()); q = start + vec2(m * d.x.signum(), m * d.y.signum()); }
            let r = Rect::from_two_pos(start, q);
            ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
            if done {
                let (x0, y0) = view.to_slide(r.min);
                let (x1, y1) = view.to_slide(r.max);
                let preset = app.insert_mode.clone().unwrap_or_else(|| "rect".into());
                let (w, h) = ((x1 - x0).max(91_440.0), (y1 - y0).max(if preset == "line" { 0.0 } else { 91_440.0 }));
                if preset == "table" {
                    let rows = ((h / 370_840.0).round() as usize).clamp(1, 20);
                    let cols = ((w / 1_219_200.0).round() as usize).clamp(1, 12);
                    if let Some(s) = app.slide() { app.checkpoint("insert"); if let Some(l) = app.pres.insert_table(&s, rows, cols, &Xfrm { x: x0, y: y0, w, h, ..Default::default() }) { app.selection = vec![l]; } }
                    app.insert_mode = None;
                } else {
                    let flip_v = preset == "line" && (q.y < start.y) != (q.x < start.x);
                    app.insert_shape(&preset, Xfrm { x: x0, y: y0, w, h, flip_v, ..Default::default() });
                }
                app.drag = None;
            }
        }
        Drag::Text { anchor } => {
            let (x, y) = view.to_slide(p);
            if let Some(e) = app.edit.clone() { if let Some(ci) = edit_hit_loose(scene, &e, x, y) { app.edit = Some(TextEdit { cursor: ci, anchor, ..e }); } }
            if done { app.drag = None; }
        }
        Drag::Cells { loc, from } => {
            let (x, y) = view.to_slide(p);
            if let Some(i) = scene.items.iter().position(|it| it.own && it.loc == loc) { if let Some(to) = table_cell(scene, i, x, y) { app.cell_sel = Some((loc.clone(), from, to)); } }
            if done {
                app.drag = None;
                // A single cell: type in it.
                if let Some((l, a, b)) = app.cell_sel.clone() { if a == b { app.cell_sel = None; start_cell_edit(app, scene, &l, a, x, y); } }
            }
        }
        Drag::Adjust { loc, handle } => {
            // Each axis takes the value whose handle lands nearest the pointer.
            let Some(it) = scene.items.iter().find(|i| i.own && i.loc == loc) else { app.drag = None; return };
            let Some(inv) = it.transform.invert() else { return };
            let (px, py) = view.to_slide(p);
            let (lx, ly) = inv.apply(px, py);
            let Some(prst) = app.pres.element(&loc).and_then(|e| e.path("p:spPr/a:prstGeom")).and_then(|g| g.attr("prst")).map(String::from) else { return };
            let mut adj = app.pres.adjusts(&loc);
            let geom = slides_engine::geom::preset(&prst, it.w, it.h, &adj);
            let Some(h) = geom.handles.get(handle).cloned() else { return };
            for axis in [h.x.clone(), h.y.clone()].into_iter().flatten() {
                let (name, lo, hi) = axis;
                let (lo, hi) = (lo.min(hi), lo.max(hi));
                let mut best = (f64::MAX, adj.iter().find(|a| a.0 == name).map(|a| a.1).unwrap_or(lo));
                for k in 0..=120 {
                    let v = lo + (hi - lo) * k as f64 / 120.0;
                    let mut a2 = adj.clone();
                    if let Some(e) = a2.iter_mut().find(|e| e.0 == name) { e.1 = v; } else { a2.push((name.clone(), v)); }
                    if let Some(hh) = slides_engine::geom::preset(&prst, it.w, it.h, &a2).handles.get(handle) {
                        let d = (hh.at.x - lx).powi(2) + (hh.at.y - ly).powi(2);
                        if d < best.0 { best = (d, v); }
                    }
                }
                if let Some(e) = adj.iter_mut().find(|e| e.0 == name) { e.1 = best.1; } else { adj.push((name.clone(), best.1)); }
                app.pres.set_adjust(&loc, &name, best.1);
            }
            app.changed();
            if done { app.drag = None; }
        }
        Drag::ColumnBorder { loc, col, orig, start, .. } => {
            let d = ((p.x - start.x) / view.scale) as f64;
            let new = (orig + d).max(91_440.0);
            let x = view.to_slide(p).0;
            ui.painter().vline(view.to_screen(x, 0.0).x, Rect::from_two_pos(view.to_screen(0.0, 0.0), view.to_screen(1.0, app.pres.size().1)).y_range(), Stroke::new(1.0, ACCENT));
            if done { app.checkpoint("table"); app.pres.set_column_width(&loc, col, new); app.drag = None; }
            else { app.drag = Some(Drag::ColumnBorder { loc, col, orig, start, new }); }
        }
        Drag::RowBorder { loc, row, orig, start, .. } => {
            let d = ((p.y - start.y) / view.scale) as f64;
            let new = (orig + d).max(91_440.0);
            ui.painter().hline(Rect::from_two_pos(view.to_screen(0.0, 0.0), view.to_screen(app.pres.size().0, 1.0)).x_range(), p.y, Stroke::new(1.0, ACCENT));
            if done { app.checkpoint("table"); app.pres.set_row_height(&loc, row, new); app.drag = None; }
            else { app.drag = Some(Drag::RowBorder { loc, row, orig, start, new }); }
        }
    }
}

/// Start moving the selection: render the slide without it and it alone, once.
fn start_move(app: &mut SlidesApp, scene: &Scene, p: Pos2, ppp: f32, view: &View) {
    let frames: Vec<(Loc, Xfrm)> = app.selection.iter().filter_map(|l| app.frame_of(scene, l).map(|f| (l.clone(), f))).collect();
    if frames.is_empty() { return; }
    let moving: Vec<usize> = app.selection.iter().filter_map(|l| l.path.first().copied()).collect();
    let mut bg = scene.clone();
    bg.items.retain(|i| !(i.own && moving.contains(&i.loc.path[0])));
    let mut fg = scene.clone();
    fg.items.retain(|i| i.own && moving.contains(&i.loc.path[0]));
    fg.background = Fill::None;
    let opts = Options { scale: (view.scale * ppp) as f64, prompts: true, skip_text_of: None, translate: Some(crate::i18n::raw) };
    let tex = |ctx: &egui::Context, s: &Scene, name: &str, images: &mut slides_engine::render::Images, pres: &slides_engine::Presentation| -> Option<egui::TextureHandle> {
        let pm = render::render(pres, s, images, opts)?;
        Some(ctx.load_texture(name, egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR))
    };
    let ctx = app.ctx.clone();
    if let (Some(b), Some(f)) = (tex(&ctx, &bg, "drag-bg", &mut app.images, &app.pres), tex(&ctx, &fg, "drag-fg", &mut app.images, &app.pres)) {
        app.canvas.drag_layers = Some((app.selection.clone(), b, f));
    }
    app.edit = None;
    app.drag = Some(Drag::Move { start: p, frames, moved: false, delta: (0.0, 0.0) });
}

/// Frames of the slide's other objects (snapping targets).
fn other_frames(app: &SlidesApp, scene: &Scene, exclude: &[Loc]) -> Vec<Xfrm> {
    let mut out = vec![];
    let mut seen = vec![];
    for it in scene.items.iter().filter(|i| i.own && !i.hidden) {
        let l = top_loc(it);
        if exclude.contains(&l) || seen.contains(&l) { continue; }
        seen.push(l.clone());
        if let Some(f) = app.frame_of(scene, &l) { out.push(f); }
    }
    out
}

/// Smart guides: snap the moved group's edges and center to the slide's and to other
/// objects' edges and centers. Returns the snapped delta and the guide lines (slide EMU).
fn smart_snap(app: &SlidesApp, scene: &Scene, frames: &[(Loc, Xfrm)], dx: f64, dy: f64, view: &View) -> (f64, f64, Vec<(bool, f64)>) {
    let (sw, sh) = app.pres.size();
    let tol = 6.0 / view.scale as f64;
    let locs: Vec<Loc> = frames.iter().map(|f| f.0.clone()).collect();
    let x0 = frames.iter().map(|f| f.1.x).fold(f64::MAX, f64::min);
    let y0 = frames.iter().map(|f| f.1.y).fold(f64::MAX, f64::min);
    let x1 = frames.iter().map(|f| f.1.x + f.1.w).fold(f64::MIN, f64::max);
    let y1 = frames.iter().map(|f| f.1.y + f.1.h).fold(f64::MIN, f64::max);
    let others = other_frames(app, scene, &locs);
    let mut xs = vec![0.0, sw / 2.0, sw];
    let mut ys = vec![0.0, sh / 2.0, sh];
    // Snap to grid: every grid line is a target too.
    if app.snap_grid && app.show_grid {
        let g = app.grid_spacing.max(9_144.0);
        let mut v = g; while v < sw { xs.push(v); v += g; }
        let mut v = g; while v < sh { ys.push(v); v += g; }
    }
    let others = if app.smart_guides { others } else { vec![] };
    for f in &others { xs.extend([f.x, f.x + f.w / 2.0, f.x + f.w]); ys.extend([f.y, f.y + f.h / 2.0, f.y + f.h]); }
    let best = |pos: [f64; 3], targets: &[f64], d: f64| -> (f64, Option<f64>) {
        let mut best: Option<(f64, f64)> = None;
        for p in pos { for t in targets { let e = (p + d - t).abs(); if e < tol && best.is_none_or(|b| e < b.0) { best = Some((e, *t - p)); } } }
        match best { Some((_, nd)) => (nd, pos.iter().map(|p| p + nd).find(|p| targets.iter().any(|t| (p - t).abs() < 1e-6))), None => (d, None) }
    };
    let (ndx, gx) = best([x0, (x0 + x1) / 2.0, x1], &xs, dx);
    let (ndy, gy) = best([y0, (y0 + y1) / 2.0, y1], &ys, dy);
    let mut guides = vec![];
    if let Some(g) = gx { guides.push((true, g)); }
    if let Some(g) = gy { guides.push((false, g)); }
    (ndx, ndy, guides)
}

fn draw_guides(ui: &Ui, view: &View, guides: &[(bool, f64)]) {
    let clip = ui.clip_rect();
    for (vertical, at) in guides {
        let p = view.to_screen(*at, *at);
        if *vertical { ui.painter().add(egui::Shape::dashed_line(&[pos2(p.x, clip.min.y), pos2(p.x, clip.max.y)], Stroke::new(1.0, GUIDE), 5.0, 3.0)); }
        else { ui.painter().add(egui::Shape::dashed_line(&[pos2(clip.min.x, p.y), pos2(clip.max.x, p.y)], Stroke::new(1.0, GUIDE), 5.0, 3.0)); }
    }
}

/// The outline of an object at a new frame (resize and rotate previews).
fn draw_ghost(ui: &Ui, scene: &Scene, view: &View, loc: &Loc, x: &Xfrm) {
    let geom = scene.items.iter().find(|i| i.own && i.loc.path == loc.path).and_then(|i| if let Kind::Shape { .. } = &i.kind { Some(()) } else { None });
    let t = x.matrix();
    let preset = geom.and_then(|_| scene.items.iter().find(|i| i.loc.path == loc.path)).and_then(|i| if let Kind::Shape { geom, .. } = &i.kind { Some(geom.clone()) } else { None });
    let paths = match preset { Some(g) if !g.paths.is_empty() => {
        // Scale the item's geometry to the new size.
        let it = scene.items.iter().find(|i| i.loc.path == loc.path).unwrap();
        let (kx, ky) = (x.w / it.w.max(1.0), x.h / it.h.max(1.0));
        g.paths.iter().flat_map(|p| slides_engine::geom::flatten(&p.cmds, 8)).map(|poly| poly.iter().map(|q| (q.x * kx, q.y * ky)).collect::<Vec<_>>()).collect::<Vec<_>>()
    } _ => vec![vec![(0.0, 0.0), (x.w, 0.0), (x.w, x.h), (0.0, x.h), (0.0, 0.0)]] };
    for poly in paths {
        let pts: Vec<Pos2> = poly.iter().map(|(a, b)| { let (sx, sy) = t.apply(*a, *b); view.to_screen(sx, sy) }).collect();
        ui.painter().add(egui::Shape::dashed_line(&pts, Stroke::new(1.2, ACCENT), 4.0, 3.0));
    }
    let c = corners(view, &t, x.w, x.h);
    for k in 0..4 { ui.painter().line_segment([c[k], c[(k + 1) % 4]], Stroke::new(1.0, ACCENT.gamma_multiply(0.6))); }
}

fn is_empty_text(it: &Item) -> bool {
    matches!(&it.kind, Kind::Shape { text: Some(t), .. } if t.paras.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty())))
}

fn start_cell_edit(app: &mut SlidesApp, scene: &Scene, loc: &Loc, cell: (usize, usize), x: f64, y: f64) {
    let Some(it) = scene.items.iter().find(|i| i.own && i.loc == *loc) else { return };
    let ci = render::cell_layout_for(it, cell.0, cell.1).and_then(|(laid, t)| { let (tx, ty) = t.invert()?.apply(x, y); Some(laid.hit(tx, ty)) }).unwrap_or(0);
    app.cell_sel = None;
    app.selection = vec![loc.clone()];
    app.edit = Some(TextEdit { loc: loc.clone(), cell: Some(cell), cursor: ci, anchor: ci });
}

/// The layout being edited (a shape's text or a table cell's), with its transform.
fn edit_layout(scene: &Scene, e: &TextEdit) -> Option<(slides_engine::text::Layout, Affine)> {
    let it = scene.items.iter().find(|i| i.loc == e.loc)?;
    match e.cell { Some((r, c)) => render::cell_layout_for(it, r, c), None => render::text_layout_for(it) }
}

/// The character under a point inside the edited box (None when outside it).
fn edit_hit(scene: &Scene, e: &TextEdit, x: f64, y: f64) -> Option<usize> {
    let it = scene.items.iter().find(|i| i.loc == e.loc)?;
    let (lx, ly) = it.transform.invert()?.apply(x, y);
    match e.cell {
        Some((r, c)) => {
            let Kind::Table(t) = &it.kind else { return None };
            if t.cell_at(lx, ly) != Some((r, c)) { return None; }
        }
        None => if lx < 0.0 || ly < 0.0 || lx > it.w || ly > it.h { return None; },
    }
    edit_hit_loose(scene, e, x, y)
}
fn edit_hit_loose(scene: &Scene, e: &TextEdit, x: f64, y: f64) -> Option<usize> {
    let (laid, t) = edit_layout(scene, e)?;
    let (tx, ty) = t.invert()?.apply(x, y);
    Some(laid.hit(tx, ty))
}

/// The character under a slide point inside an item's text box, if inside the shape.
fn text_hit(it: &Item, x: f64, y: f64) -> Option<usize> {
    let inv = it.transform.invert()?;
    let (lx, ly) = inv.apply(x, y);
    if lx < 0.0 || ly < 0.0 || lx > it.w || ly > it.h { return None; }
    let (laid, t) = render::text_layout_for(it)?;
    let (tx, ty) = t.invert()?.apply(x, y);
    Some(laid.hit(tx, ty))
}
/// Is the point over the item's text lines?
fn text_hit_strict(it: &Item, x: f64, y: f64) -> bool {
    if is_empty_text(it) { return false; }
    let Some((laid, t)) = render::text_layout_for(it) else { return false };
    let Some(ti) = t.invert() else { return false };
    let (px, py) = ti.apply(x, y);
    laid.lines.iter().any(|l| px >= l.x0.min(l.x1) - 6.0 && px <= l.x1.max(l.x0) + 6.0 && py >= l.top - 2.0 && py <= l.top + l.height + 2.0)
}

fn select_word(app: &mut SlidesApp) {
    let Some(e) = app.edit.clone() else { return };
    let text: Vec<char> = app.pres.shape_text(&e.loc, e.cell).chars().collect();
    let mut a = e.cursor.min(text.len());
    let mut b = a;
    while a > 0 && text[a - 1].is_alphanumeric() { a -= 1; }
    while b < text.len() && text[b].is_alphanumeric() { b += 1; }
    if a < b { app.edit = Some(TextEdit { anchor: a, cursor: b, ..e }); }
}

/// A light outline around the object under the pointer (what a click would select).
fn draw_hover(app: &SlidesApp, ui: &Ui, painter: &egui::Painter, scene: &Scene, view: &View) {
    if app.drag.is_some() || app.insert_mode.is_some() { return; }
    let Some(p) = ui.input(|i| i.pointer.hover_pos()) else { return };
    if !ui.clip_rect().contains(p) { return; }
    let (x, y) = view.to_slide(p);
    let Some(i) = pick(scene, &app.selection, x, y, 5.0 / view.scale as f64) else { return };
    let loc = top_loc(&scene.items[i]);
    if app.selection.contains(&loc) { return; }
    let Some(f) = app.frame_of(scene, &loc) else { return };
    let c = corners(view, &f.matrix(), f.w, f.h);
    for k in 0..4 { painter.line_segment([c[k], c[(k + 1) % 4]], Stroke::new(1.0, ACCENT.gamma_multiply(0.45))); }
}

fn draw_selection(app: &mut SlidesApp, ui: &Ui, painter: &egui::Painter, scene: &Scene, view: &View) {
    let stroke = Stroke::new(1.0, ACCENT);
    // The picked SmartArt item: a dashed frame round its shapes.
    if let Some((g, k)) = app.diagram_item.clone() {
        if app.selection.first() == Some(&g) {
            for it in scene.items.iter().filter(|it| it.own && it.loc.path.len() >= 2 && it.loc.path[0] == g.path[0]) {
                let child = slides_engine::scene::Loc { part: it.loc.part.clone(), path: it.loc.path.iter().take(2).copied().collect() };
                if app.pres.diagram_item(&child) != Some(k) { continue; }
                let c = corners(view, &it.transform, it.w, it.h);
                for j in 0..4 { painter.add(egui::Shape::dashed_line(&[c[j], c[(j + 1) % 4]], Stroke::new(2.0, Color32::from_rgb(0xE2, 0x57, 0x2A)), 5.0, 3.0)); }
            }
        } else { app.diagram_item = None; }
    }
    let moving = matches!(app.drag, Some(Drag::Move { moved: true, .. }) | Some(Drag::Resize { .. }) | Some(Drag::Rotate { .. }));
    if !moving {
        for loc in &app.selection {
            let Some(x) = app.frame_of(scene, loc) else { continue };
            let c = corners(view, &x.matrix(), x.w, x.h);
            let editing = app.edit.as_ref().is_some_and(|e| e.loc.path.first() == loc.path.first());
            // While editing, the frame is dashed (drag it to move the box).
            if editing { for k in 0..4 { painter.add(egui::Shape::dashed_line(&[c[k], c[(k + 1) % 4]], stroke, 4.0, 3.0)); } } else { for k in 0..4 { painter.line_segment([c[k], c[(k + 1) % 4]], stroke); } }
            if app.selection.len() == 1 {
                let r = rotate_handle(&c);
                painter.line_segment([pos2((c[0].x + c[1].x) / 2.0, (c[0].y + c[1].y) / 2.0), r], stroke);
                painter.circle(r, 5.0, Color32::WHITE, stroke);
                for (h, hp) in handles(&c) {
                    if h.0 != 0 && h.1 != 0 { painter.circle(hp, 4.5, Color32::WHITE, stroke); } else { painter.rect(Rect::from_center_size(hp, vec2(8.0, 8.0)), 1.0, Color32::WHITE, stroke, egui::StrokeKind::Middle); }
                }
            }
        }
    }
    // Yellow adjust handles.
    if !moving { for (_, hp) in adjust_handles(app, scene, view) { painter.circle(hp, 4.5, Color32::from_rgb(0xFF, 0xD7, 0x00), Stroke::new(1.0, Color32::from_gray(80))); } }
    // Selected table cells.
    if let Some((loc, a, b)) = &app.cell_sel {
        if let Some(it) = scene.items.iter().find(|i| i.own && i.loc == *loc) {
            if let Kind::Table(t) = &it.kind {
                let (xs, ys) = t.edges();
                let (r0, r1, c0, c1) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
                let q = [(xs[c0], ys[r0]), (xs[(c1 + 1).min(xs.len() - 1)], ys[r0]), (xs[(c1 + 1).min(xs.len() - 1)], ys[(r1 + 1).min(ys.len() - 1)]), (xs[c0], ys[(r1 + 1).min(ys.len() - 1)])].map(|(x, y)| { let (sx, sy) = it.transform.apply(x, y); view.to_screen(sx, sy) });
                painter.add(egui::Shape::convex_polygon(q.to_vec(), ACCENT.gamma_multiply(0.18), Stroke::new(1.5, ACCENT)));
            }
        }
    }
    // The caret and text selection while editing.
    if let Some(e) = app.edit.clone() {
        let Some((laid, t)) = edit_layout(scene, &e) else { if scene.items.iter().all(|i| i.loc != e.loc) { app.edit = None; } return };
        let (a, b) = e.range();
        if a < b {
            for cb in laid.chars.iter().skip(a).take(b - a) {
                let Some(line) = laid.lines.get(cb.line) else { continue };
                let q = [(cb.x0, line.top), (cb.x1.max(cb.x0 + 2.0), line.top), (cb.x1.max(cb.x0 + 2.0), line.top + line.height), (cb.x0, line.top + line.height)].map(|(x, y)| { let (sx, sy) = t.apply(x, y); view.to_screen(sx, sy) });
                painter.add(egui::Shape::convex_polygon(q.to_vec(), ACCENT.gamma_multiply(0.25), Stroke::NONE));
            }
        }
        let blink = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0 || a != b;
        if blink {
            let (cx, top, h) = laid.caret(e.cursor);
            let (x0, y0) = t.apply(cx, top);
            let (x1, y1) = t.apply(cx, top + h);
            painter.line_segment([view.to_screen(x0, y0), view.to_screen(x1, y1)], Stroke::new(1.5, Color32::from_gray(20)));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }
}

/// Keyboard input while editing text in place.
fn text_input(app: &mut SlidesApp, ui: &mut Ui, scene: &Scene, view: &View) {
    let Some(mut e) = app.edit.clone() else { return };
    let events = ui.input(|i| i.events.clone());
    let text: Vec<char> = app.pres.shape_text(&e.loc, e.cell).chars().collect();
    let n = text.len();
    let mut changed_text = false;
    let insert = |app: &mut SlidesApp, e: &mut TextEdit, t: &str| {
        let (a, b) = e.range();
        app.checkpoint("typing");
        if a < b { app.pres.delete_text(&e.loc, e.cell, a, b); }
        app.pres.insert_text(&e.loc, e.cell, a, t);
        e.cursor = a + t.chars().count();
        e.anchor = e.cursor;
    };
    for ev in events {
        match ev {
            egui::Event::Text(t) => { insert(app, &mut e, &t); changed_text = true; }
            egui::Event::Ime(egui::ImeEvent::Commit(t)) if !t.is_empty() => { insert(app, &mut e, &t); changed_text = true; }
            egui::Event::Paste(t) => { insert(app, &mut e, &t.replace("\r\n", "\n")); changed_text = true; }
            egui::Event::Copy | egui::Event::Cut => {
                let (a, b) = e.range();
                if a < b {
                    let s: String = text[a..b.min(n)].iter().collect();
                    ui.ctx().copy_text(s);
                    if matches!(ev, egui::Event::Cut) { app.checkpoint("cut"); app.pres.delete_text(&e.loc, e.cell, a, b); e.cursor = a; e.anchor = a; changed_text = true; }
                }
            }
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                let (a, b) = e.range();
                let word_mod = modifiers.alt || (modifiers.ctrl && !cfg!(target_os = "macos"));
                let moved = |e: &mut TextEdit, to: usize, shift: bool| { e.cursor = to; if !shift { e.anchor = to; } };
                match key {
                    egui::Key::Escape => { app.edit = None; return; }
                    egui::Key::Enter => { insert(app, &mut e, "\n"); changed_text = true; }
                    egui::Key::Backspace => {
                        app.checkpoint("typing");
                        if a < b { app.pres.delete_text(&e.loc, e.cell, a, b); e.cursor = a; }
                        else if a > 0 {
                            let mut k = a - 1;
                            if word_mod { while k > 0 && !text[k - 1].is_whitespace() { k -= 1; } }
                            app.pres.delete_text(&e.loc, e.cell, k, a); e.cursor = k;
                        }
                        e.anchor = e.cursor; changed_text = true;
                    }
                    egui::Key::Delete => {
                        app.checkpoint("typing");
                        if a < b { app.pres.delete_text(&e.loc, e.cell, a, b); e.cursor = a; } else if a < n { app.pres.delete_text(&e.loc, e.cell, a, a + 1); }
                        e.anchor = e.cursor; changed_text = true;
                    }
                    egui::Key::ArrowLeft => {
                        let mut to = if a < b && !modifiers.shift { a } else { e.cursor.saturating_sub(1) };
                        if word_mod { while to > 0 && !(text[to - 1].is_whitespace()) { to -= 1; } }
                        moved(&mut e, to, modifiers.shift);
                    }
                    egui::Key::ArrowRight => {
                        let mut to = if a < b && !modifiers.shift { b } else { (e.cursor + 1).min(n) };
                        if word_mod { while to < n && !text[to].is_whitespace() { to += 1; } }
                        moved(&mut e, to, modifiers.shift);
                    }
                    egui::Key::ArrowUp | egui::Key::ArrowDown => {
                        if let Some((laid, _)) = edit_layout(scene, &e) {
                            let (x, top, h) = laid.caret(e.cursor);
                            let y = if key == egui::Key::ArrowUp { top - 1.0 } else { top + h + 1.0 };
                            let to = if y < 0.0 { 0 } else if y > laid.height { n } else { laid.hit(x, y) };
                            moved(&mut e, to, modifiers.shift);
                        }
                    }
                    egui::Key::Home => { let mut k = e.cursor; while k > 0 && text[k - 1] != '\n' { k -= 1; } moved(&mut e, k, modifiers.shift); }
                    egui::Key::End => { let mut k = e.cursor; while k < n && text[k] != '\n' { k += 1; } moved(&mut e, k, modifiers.shift); }
                    egui::Key::A if modifiers.command => { e.anchor = 0; e.cursor = n; }
                    egui::Key::B if modifiers.command => { let bold = app.current_run(scene).map(|r| !r.bold).unwrap_or(true); app.edit = Some(e.clone()); app.format(slides_engine::edit::RunFormat { bold: Some(bold), ..Default::default() }); }
                    egui::Key::I if modifiers.command => { let it = app.current_run(scene).map(|r| !r.italic).unwrap_or(true); app.edit = Some(e.clone()); app.format(slides_engine::edit::RunFormat { italic: Some(it), ..Default::default() }); }
                    egui::Key::U if modifiers.command => { let u = app.current_run(scene).map(|r| !r.underline).unwrap_or(true); app.edit = Some(e.clone()); app.format(slides_engine::edit::RunFormat { underline: Some(u), ..Default::default() }); }
                    egui::Key::Tab => {
                        if let Some((r, c)) = e.cell {
                            // Tab moves between cells; past the last cell, a new row.
                            let (rows, cols) = app.pres.table_size(&e.loc).unwrap_or((1, 1));
                            let (mut nr, mut nc) = if modifiers.shift { if c == 0 { (r.saturating_sub(1), cols - 1) } else { (r, c - 1) } } else if c + 1 < cols { (r, c + 1) } else { (r + 1, 0) };
                            if nr >= rows { app.checkpoint("table"); app.pres.insert_row(&e.loc, rows); nr = rows; nc = 0; }
                            let len = app.pres.shape_text(&e.loc, Some((nr, nc))).chars().count();
                            e = TextEdit { cell: Some((nr, nc)), cursor: len, anchor: 0, ..e };
                            changed_text = true;
                        } else {
                            let delta = if modifiers.shift { -1 } else { 1 };
                            app.edit = Some(e.clone());
                            app.paragraphs(move |p, l, c, r| p.set_level(l, c, r, delta));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if changed_text { app.changed(); }
    // Let the system's input methods and dictation type at the caret.
    if let Some((laid, t)) = edit_layout(scene, &e) {
        let (cx, top, h) = laid.caret(e.cursor);
        let (x0, y0) = t.apply(cx, top);
        let (x1, y1) = t.apply(cx, top + h);
        let r = Rect::from_two_pos(view.to_screen(x0, y0), view.to_screen(x1, y1)).expand2(vec2(1.0, 0.0));
        ui.ctx().output_mut(|o| o.ime = Some(egui::output::IMEOutput { rect: r, cursor_rect: r, purpose: Default::default(), should_interrupt_composition: false }));
    }
    if app.edit.is_some() { app.edit = Some(e); }
    ui.input_mut(|i| i.events.retain(|ev| !matches!(ev, egui::Event::Text(_) | egui::Event::Key { .. } | egui::Event::Ime(_))));
}

/// Keys for the selection when not editing: delete, nudge, Tab to cycle, Escape, Enter.
pub fn selection_keys(app: &mut SlidesApp, ctx: &egui::Context, scene: &Scene) {
    if app.edit.is_some() || ctx.memory(|m| m.focused().is_some()) { return; }
    // Tab / Shift+Tab cycle through the slide's objects.
    let tab = ctx.input(|i| i.key_pressed(egui::Key::Tab));
    if tab {
        let back = ctx.input(|i| i.modifiers.shift);
        let mut all: Vec<Loc> = vec![];
        for it in scene.items.iter().filter(|i| i.own && !i.hidden) { let l = top_loc(it); if !all.contains(&l) { all.push(l); } }
        if !all.is_empty() {
            let cur = app.selection.first().and_then(|l| all.iter().position(|x| x == l));
            let next = match cur { Some(k) => if back { (k + all.len() - 1) % all.len() } else { (k + 1) % all.len() }, None => 0 };
            app.selection = vec![all[next].clone()];
        }
    }
    if app.selection.is_empty() && app.cell_sel.is_none() { return; }
    let mut nudge = (0.0, 0.0);
    let step = if ctx.input(|i| i.modifiers.shift) { 457_200.0 / 4.0 } else if ctx.input(|i| i.modifiers.alt) { 12_700.0 } else { 457_200.0 / 16.0 };
    ctx.input(|i| {
        if i.key_pressed(egui::Key::ArrowLeft) { nudge.0 -= step; }
        if i.key_pressed(egui::Key::ArrowRight) { nudge.0 += step; }
        if i.key_pressed(egui::Key::ArrowUp) { nudge.1 -= step; }
        if i.key_pressed(egui::Key::ArrowDown) { nudge.1 += step; }
    });
    if (nudge.0 != 0.0 || nudge.1 != 0.0) && !app.selection.is_empty() {
        app.checkpoint("nudge");
        for l in app.selection.clone() { if let Some(f) = app.frame_of(scene, &l) { app.pres.set_xfrm(&l, &Xfrm { x: f.x + nudge.0, y: f.y + nudge.1, ..f }); } }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
        // Selected cells are cleared; otherwise the objects are deleted.
        if let Some((loc, a, b)) = app.cell_sel.clone() {
            app.checkpoint("table");
            for r in a.0.min(b.0)..=a.0.max(b.0) { for c in a.1.min(b.1)..=a.1.max(b.1) { let n = app.pres.shape_text(&loc, Some((r, c))).chars().count(); app.pres.delete_text(&loc, Some((r, c)), 0, n); } }
        } else { app.delete_selection(); }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { app.selection.clear(); app.cell_sel = None; app.insert_mode = None; }
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && app.selection.len() == 1 {
        let loc = app.selection[0].clone();
        let n = app.pres.shape_text(&loc, None).chars().count();
        if app.pres.element(&loc).is_some_and(|e| e.local() == "sp") { app.edit = Some(TextEdit { loc, cell: None, cursor: n, anchor: 0 }); }
    }
    // Typing on a selected shape replaces its text (PowerPoint).
    let typed: Option<String> = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }));
    if let (Some(t), 1) = (typed, app.selection.len()) {
        let loc = app.selection[0].clone();
        if app.pres.element(&loc).is_some_and(|e| e.local() == "sp") {
            app.checkpoint("typing");
            let n = app.pres.shape_text(&loc, None).chars().count();
            app.pres.delete_text(&loc, None, 0, n);
            app.pres.insert_text(&loc, None, 0, &t);
            let c = t.chars().count();
            app.edit = Some(TextEdit { loc, cell: None, cursor: c, anchor: c });
        }
    }
}

/// Open bytes with the system's app for them (embedded files, media).
pub fn open_file(name: &str, bytes: &[u8]) {
    let dir = std::env::temp_dir().join("sard-open");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    if std::fs::write(&path, bytes).is_ok() { open_path(&path.to_string_lossy()); }
}

/// Open a file or web address with the system.
pub fn open_path(target: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(target).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", target]).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
}

/// Follow a link in the editor: open a web address, or go to a slide.
pub fn follow_link(app: &mut SlidesApp, kind: &str, target: &str) {
    match kind {
        "url" => open_path(target),
        "slide" => { if let Ok(i) = target.parse() { app.go_to(i); } }
        _ => {}
    }
}

/// The right-click menu: for the selected objects, or for the slide.
fn context_menu(app: &mut SlidesApp, ui: &mut Ui) {
    use crate::i18n::tr;
    use qalam_kit::widgets::{menu_heading, menu_item, small};
    let has = !app.selection.is_empty() || app.edit.is_some();
    if has {
        if menu_item(ui, "cut", "Cut") { app.cut(); ui.close(); }
        if menu_item(ui, "copy", "Copy") { app.copy(); ui.close(); }
    }
    menu_heading(ui, "Paste Options:");
    crate::theme::hrow(ui, |ui| {
        if small(ui, "paste", None, false, true, "Use Destination Theme").clicked() { app.paste_as("dest"); ui.close(); }
        if small(ui, "formatPainter", None, false, true, "Keep Source Formatting").clicked() { app.paste_as("keep"); ui.close(); }
        if small(ui, "picture", None, false, true, "Picture").clicked() { app.paste_as("picture"); ui.close(); }
        if small(ui, "textBox", None, false, true, "Keep Text Only").clicked() { app.paste_as("text"); ui.close(); }
    });
    ui.separator();
    if !has {
        // The slide's own menu.
        crate::theme::submenu(ui, tr("Layout"), |ui| if let Some(l) = crate::nav::layout_gallery(app, ui) { app.set_layout(&l); ui.close(); });
        if menu_item(ui, "resetSlide", "Reset Slide") { reset_placeholders(app); ui.close(); }
        if menu_item(ui, "gridlines", "Grid and Guides…") { app.view_dialog = Some(crate::views::ViewDialog::GridGuides); ui.close(); }
        if menu_item(ui, "formatBackground", "Format Background…") { app.pane = crate::app::Pane::Background; ui.close(); }
        if menu_item(ui, "hideSlide", "Hide Slide") { app.toggle_hidden(); ui.close(); }
        ui.separator();
        if menu_item(ui, "newComment", "New Comment") { app.pane = crate::app::Pane::Comments; ui.close(); }
        return;
    }
    // Tables: Qalam's table commands.
    if let Some((tloc, r0, r1, c0, c1)) = app.table_target().and_then(|(l, cells)| {
        if app.pres.table_size(&l).is_none() { return None; }
        Some((l, cells.iter().map(|c| c.0).min()?, cells.iter().map(|c| c.0).max()?, cells.iter().map(|c| c.1).min()?, cells.iter().map(|c| c.1).max()?))
    }) {
        crate::theme::submenu(ui, tr("Insert"), |ui| {
            if menu_item(ui, "insertAbove", "Insert Rows Above") { app.checkpoint("table"); app.pres.insert_row(&tloc, r0); ui.close(); }
            if menu_item(ui, "insertBelow", "Insert Rows Below") { app.checkpoint("table"); app.pres.insert_row(&tloc, r1 + 1); ui.close(); }
            if menu_item(ui, "insertLeft", "Insert Columns to the Left") { app.checkpoint("table"); app.pres.insert_column(&tloc, c0); ui.close(); }
            if menu_item(ui, "insertRight", "Insert Columns to the Right") { app.checkpoint("table"); app.pres.insert_column(&tloc, c1 + 1); ui.close(); }
        });
        crate::theme::submenu(ui, tr("Delete"), |ui| {
            if menu_item(ui, "deleteRow", "Delete Rows") { app.checkpoint("table"); for r in (r0..=r1).rev() { app.pres.delete_row(&tloc, r); } app.edit = None; app.cell_sel = None; ui.close(); }
            if menu_item(ui, "deleteColumn", "Delete Columns") { app.checkpoint("table"); for c in (c0..=c1).rev() { app.pres.delete_column(&tloc, c); } app.edit = None; app.cell_sel = None; ui.close(); }
            if menu_item(ui, "deleteTable", "Delete Table") { app.checkpoint("table"); app.pres.delete_table(&tloc); app.selection.clear(); app.edit = None; app.cell_sel = None; ui.close(); }
        });
        crate::theme::submenu(ui, tr("Select"), |ui| {
            let (rows, cols) = app.pres.table_size(&tloc).unwrap_or((1, 1));
            if menu_item(ui, "selectRow", "Select Row") { app.edit = None; app.cell_sel = Some((tloc.clone(), (r0, 0), (r1, cols - 1))); ui.close(); }
            if menu_item(ui, "selectColumn", "Select Column") { app.edit = None; app.cell_sel = Some((tloc.clone(), (0, c0), (rows - 1, c1))); ui.close(); }
            if menu_item(ui, "selectTable", "Select Table") { app.edit = None; app.cell_sel = Some((tloc.clone(), (0, 0), (rows - 1, cols - 1))); ui.close(); }
        });
        if menu_item(ui, "mergeCells", "Merge Cells") && (r1 > r0 || c1 > c0) { app.checkpoint("table"); app.pres.merge_cells(&tloc, r0, c0, r1, c1); app.cell_sel = None; ui.close(); }
        if menu_item(ui, "splitCells", "Split Cells") { app.checkpoint("table"); app.pres.split_cell(&tloc, r0, c0); ui.close(); }
        if menu_item(ui, "distributeRows", "Distribute Rows Evenly") { app.checkpoint("table"); app.pres.distribute_rows(&tloc); ui.close(); }
        if menu_item(ui, "distributeColumns", "Distribute Columns Evenly") { app.checkpoint("table"); app.pres.distribute_columns(&tloc); ui.close(); }
        crate::theme::submenu(ui, tr("Shading"), |ui| if let Some(pick) = qalam_kit::widgets::color_menu(ui, "No Fill") {
            let v = pick.map(|c| format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2)).unwrap_or_default();
            if let Some((l, cells)) = app.table_target() { app.checkpoint("table"); app.pres.set_cell_fill(&l, &cells, Some(&v)); }
            ui.close();
        });
        if menu_item(ui, "tableProperties", "Table Layout…") { app.tab = crate::app::Tab::TableLayout; ui.close(); }
        ui.separator();
    }
    let single = app.selection.len() == 1;
    let loc = app.selection.first().cloned();
    let is_text = loc.as_ref().is_some_and(|l| app.pres.element(l).is_some_and(|e| e.local() == "sp"));
    if is_text && menu_item(ui, "addText", "Edit Text") {
        if let Some(l) = loc.clone() { let n = app.pres.shape_text(&l, None).chars().count(); app.edit = Some(TextEdit { loc: l, cell: None, cursor: n, anchor: n }); }
        ui.close();
    }
    if single && is_text && menu_item(ui, "editShape", "Edit Points") { app.notify(&tr("Drag the yellow handles to change the shape; use Change Shape for another outline.")); app.show_adjust = true; ui.close(); }
    crate::theme::submenu(ui, tr("Group"), |ui| crate::home_tab::group_items(app, ui));
    crate::theme::submenu(ui, tr("Bring to Front"), |ui| {
        if menu_item(ui, "bringToFront", "Bring to Front") { order(app, slides_engine::edit::Order::Front); ui.close(); }
        if menu_item(ui, "bringForward", "Bring Forward") { order(app, slides_engine::edit::Order::Forward); ui.close(); }
    });
    crate::theme::submenu(ui, tr("Send to Back"), |ui| {
        if menu_item(ui, "sendToBack", "Send to Back") { order(app, slides_engine::edit::Order::Back); ui.close(); }
        if menu_item(ui, "sendBackward", "Send Backward") { order(app, slides_engine::edit::Order::Backward); ui.close(); }
    });
    crate::theme::submenu(ui, tr("Link"), |ui| {
        if menu_item(ui, "link", "Insert Link…") { app.dialog = Some(crate::dialogs::Dialog::Link { target: 0, address: String::new(), slide: 0, text_only: app.edit.is_some() }); ui.close(); }
        if menu_item(ui, "unlink", "Remove Link") { app.set_link(None); ui.close(); }
    });
    let text: String = loc.as_ref().map(|l| app.pres.shape_text(l, None)).unwrap_or_default();
    let sel_text = app.edit.as_ref().map(|e| { let (a, b) = e.range(); text.chars().skip(a).take(b - a).collect::<String>() }).filter(|t| !t.is_empty()).unwrap_or_else(|| text.clone());
    if menu_item(ui, "search", "Search") { open_path(&format!("https://www.bing.com/search?q={}", url_encode(&sel_text))); ui.close(); }
    ui.separator();
    if menu_item(ui, "save", "Save as Picture…") {
        if let Some(png) = app.selection_png() { if let Some(p) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("Picture.png").save_file() { let _ = std::fs::write(p, png); } }
        ui.close();
    }
    if menu_item(ui, "translate", "Translate…") { crate::translate::open(app, &sel_text); ui.close(); }
    if menu_item(ui, "altText", "Edit Alt Text…") { app.pane = crate::app::Pane::Format; app.format_tab = 2; ui.close(); }
    if single && is_text && menu_item(ui, "setDefault", "Set as Default Shape") { if let Some(l) = &loc { app.default_shape = app.pres.element(l).cloned(); app.notify(&tr("New shapes will use this look.")); } ui.close(); }
    if menu_item(ui, "size", "Size and Position…") { app.pane = crate::app::Pane::Format; app.format_tab = 2; ui.close(); }
    if menu_item(ui, "formatPane", "Format Shape…") { app.pane = crate::app::Pane::Format; app.format_tab = 0; ui.close(); }
    ui.separator();
    if menu_item(ui, "newComment", "New Comment") { app.pane = crate::app::Pane::Comments; ui.close(); }
}

fn order(app: &mut SlidesApp, o: slides_engine::edit::Order) {
    app.checkpoint("order");
    app.selection = app.selection.clone().iter().filter_map(|l| app.pres.reorder(l, o)).collect();
}

fn reset_placeholders(app: &mut SlidesApp) {
    let Some(s) = app.slide() else { return };
    app.checkpoint("reset");
    let n = app.pres.slide(&s).and_then(|r| r.path("p:cSld/p:spTree")).map(|t| t.elements().count()).unwrap_or(0);
    for i in 0..n {
        let loc = Loc { part: s.clone(), path: vec![i] };
        if let Some(sp) = app.pres.element_mut(&loc) { if sp.path("p:nvSpPr/p:nvPr/p:ph").is_some() { if let Some(pr) = sp.child_mut("p:spPr") { pr.children.clear(); } } }
    }
}

fn url_encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn slide_media(app: &SlidesApp, slide: &str) -> Vec<String> {
    crate::media::objects(&app.pres, slide, Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0))).into_iter().map(|o| o.1).collect()
}
