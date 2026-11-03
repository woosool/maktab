//! The Format pane's 3-D Format (bevels, depth, contour, material, lighting) and 3-D Rotation
//! (presets, X/Y/Z rotation, perspective, distance from ground) sections.

use crate::app::SlidesApp;
use crate::i18n::tr;
use qalam_kit::widgets::*;
use slides_engine::color::Rgba;
use slides_engine::three_d::{Bevel, ThreeD, BEVELS, CAMERAS, LIGHTS, MATERIALS, MODELS};
use egui::{pos2, vec2, Color32, Rect, Sense, Stroke};

const PT: f64 = 12_700.0;

/// Store `t` on every selected shape; `undo` starts an undo step (once per edit, not per
/// frame of a drag).
fn store(app: &mut SlidesApp, t: &ThreeD, undo: bool) {
    let targets = app.selection.clone();
    if undo { app.checkpoint("3-D"); }
    for l in targets { app.pres.set_three_d(&l, t); }
}

fn drag(ui: &mut egui::Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str) -> (bool, bool) {
    ui.label(tr(label));
    let r = ui.add(egui::DragValue::new(v).range(range).speed(0.5).max_decimals(1).suffix(suffix));
    ui.end_row();
    (r.changed(), r.drag_started() || (r.changed() && !r.dragged()))
}

fn color_row(ui: &mut egui::Ui, label: &str, c: &mut Option<Rgba>) -> bool {
    ui.label(tr(label));
    let mut wc = c.map(|c| word_engine::Color(c.r, c.g, c.b)).unwrap_or(word_engine::Color(0x80, 0x80, 0x80));
    let changed = ui.horizontal(|ui| {
        let a = color_button(ui, &mut wc);
        let b = c.is_some() && ui.small_button(tr("Automatic")).clicked();
        if b { *c = None; } else if a { *c = Some(Rgba::rgb(wc.0, wc.1, wc.2)); }
        a || b
    }).inner;
    ui.end_row();
    changed
}

fn bevel_combo(ui: &mut egui::Ui, id: &str, b: &mut Option<Bevel>) -> bool {
    let name = b.as_ref().and_then(|b| BEVELS.iter().find(|x| x.0 == b.kind)).map(|x| tr(x.1)).unwrap_or_else(|| tr("No Bevel"));
    let mut changed = false;
    crate::theme::Combo::from_id_salt(id).width(150.0).selected_text(name).show_ui(ui, |ui| {
        if ui.selectable_label(b.is_none(), tr("No Bevel")).clicked() { *b = None; changed = true; }
        for (k, n) in BEVELS {
            if ui.selectable_label(b.as_ref().is_some_and(|x| x.kind == *k), tr(n)).clicked() {
                let (w, h) = b.as_ref().map(|x| (x.w, x.h)).unwrap_or((6.0 * PT, 6.0 * PT));
                *b = Some(Bevel { kind: k.to_string(), w, h });
                changed = true;
            }
        }
    });
    changed
}

fn bevel_size(ui: &mut egui::Ui, b: &mut Option<Bevel>, (changed, undo): &mut (bool, bool)) {
    let Some(bv) = b.as_mut() else { return };
    let (mut w, mut h) = (bv.w / PT, bv.h / PT);
    let a = drag(ui, "Width", &mut w, 0.0..=200.0, " pt");
    let c = drag(ui, "Height", &mut h, 0.0..=200.0, " pt");
    if a.0 || c.0 { bv.w = w * PT; bv.h = h * PT; *changed = true; *undo |= a.1 || c.1; }
}

pub fn format(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let Some(loc) = app.selection.first().cloned() else { return };
    let mut t = app.pres.three_d(&loc);
    let mut ch = (false, false);
    crate::theme::section(ui, "3-D Format");
    crate::theme::grid(ui, egui::Grid::new("fmt-3d").num_columns(2), |ui| {
        ui.label(tr("Top bevel"));
        ui.horizontal(|ui| {
            let cur = preview_tex(ui.ctx(), &ThreeD { top: t.top.clone().map(|b| Bevel { w: 120_000.0, h: 120_000.0, ..b }), ..ThreeD::default() }, 26);
            let b = ui.add(egui::Button::image(egui::Image::new(&cur).fit_to_exact_size(vec2(26.0, 26.0))));
            egui::Popup::menu(&b).show(|ui| bevel_gallery(app, ui));
            if bevel_combo(ui, "3d-top", &mut t.top) { ch = (true, true); }
        });
        ui.end_row();
        bevel_size(ui, &mut t.top, &mut ch);
        ui.label(tr("Bottom bevel")); if bevel_combo(ui, "3d-bottom", &mut t.bottom) { ch = (true, true); } ui.end_row();
        bevel_size(ui, &mut t.bottom, &mut ch);
        if color_row(ui, "Depth color", &mut t.depth_color) { ch = (true, true); }
        let mut d = t.depth / PT;
        let r = drag(ui, "Depth", &mut d, 0.0..=1000.0, " pt");
        if r.0 { t.depth = d * PT; ch = (true, ch.1 || r.1); }
        if color_row(ui, "Contour color", &mut t.contour_color) { ch = (true, true); }
        let mut c = t.contour / PT;
        let r = drag(ui, "Contour size", &mut c, 0.0..=100.0, " pt");
        if r.0 { t.contour = c * PT; ch = (true, ch.1 || r.1); }
        ui.label(tr("Material"));
        crate::theme::Combo::from_id_salt("3d-material").width(150.0).selected_text(MATERIALS.iter().find(|m| m.0 == t.material).map(|m| tr(m.1)).unwrap_or_default()).show_ui(ui, |ui| for (k, n) in MATERIALS {
            if ui.selectable_label(t.material == *k, tr(n)).clicked() { t.material = k.to_string(); ch = (true, true); }
        });
        ui.end_row();
        ui.label(tr("Lighting"));
        crate::theme::Combo::from_id_salt("3d-light").width(150.0).selected_text(LIGHTS.iter().find(|m| m.0 == t.light).map(|m| tr(m.1)).unwrap_or_default()).show_ui(ui, |ui| for (k, n) in LIGHTS {
            if ui.selectable_label(t.light == *k, tr(n)).clicked() { t.light = k.to_string(); ch = (true, true); }
        });
        ui.end_row();
        let r = drag(ui, "Angle", &mut t.light_angle, 0.0..=359.9, "°");
        if r.0 { ch = (true, ch.1 || r.1); }
    });
    if ui.button(tr("Reset")).clicked() { let rot = (t.camera.clone(), t.rot, t.fov, t.z); t = ThreeD { camera: rot.0, rot: rot.1, fov: rot.2, z: rot.3, ..ThreeD::default() }; ch = (true, true); }

    crate::theme::section(ui, "3-D Rotation");
    crate::theme::hrow(ui, |ui| {
        ui.label(tr("Presets:"));
        let cur = preview_tex(ui.ctx(), &ThreeD { camera: t.camera.clone(), rot: t.rot, fov: t.fov, ..ThreeD::default() }, 30);
        let b = ui.add(egui::Button::image_and_text(egui::Image::new(&cur).fit_to_exact_size(vec2(30.0, 30.0)), "⏷"));
        egui::Popup::menu(&b).show(|ui| rotation_gallery(app, ui));
    });
    crate::theme::grid(ui, egui::Grid::new("fmt-3d-rot").num_columns(2), |ui| {
        let (mut x, mut y, mut z) = t.rotation();
        let a = drag(ui, "X Rotation", &mut x, 0.0..=359.9, "°");
        let b = drag(ui, "Y Rotation", &mut y, 0.0..=359.9, "°");
        let c = drag(ui, "Z Rotation", &mut z, 0.0..=359.9, "°");
        if a.0 || b.0 || c.0 { t.rot = Some((x, y, z)); ch = (true, ch.1 || a.1 || b.1 || c.1); }
        let mut fov = t.perspective();
        ui.label(tr("Perspective"));
        let r = ui.add_enabled(t.camera.starts_with("perspective"), egui::DragValue::new(&mut fov).range(0.0..=120.0).speed(0.5).suffix("°"));
        ui.end_row();
        if r.changed() { t.fov = fov; ch = (true, ch.1 || r.drag_started() || !r.dragged()); }
        let mut g = t.z / PT;
        let r = drag(ui, "Distance from ground", &mut g, -1000.0..=1000.0, " pt");
        if r.0 { t.z = g * PT; ch = (true, ch.1 || r.1); }
    });
    if ui.button(tr("Reset rotation")).clicked() { t.camera = "orthographicFront".into(); t.rot = None; t.fov = 0.0; t.z = 0.0; ch = (true, true); }
    if ch.0 { store(app, &t, ch.1); }
}

/// Pictures of the gallery's models (rendered once).
fn model_pictures(ctx: &egui::Context) -> Vec<egui::TextureHandle> {
    let id = egui::Id::new("3d-model-pictures");
    if let Some(v) = ctx.data(|d| d.get_temp::<Vec<egui::TextureHandle>>(id)) { return v; }
    let mut out = vec![];
    let mut p = slides_engine::Presentation::new_blank(false);
    let l = p.all_layouts().last().cloned().unwrap_or_default();
    for m in MODELS {
        let s = p.add_slide(&l, 0);
        let (sw, sh) = p.size();
        p.insert_3d_model(&s, m.0, (sw - m.3) / 2.0, (sh - m.4) / 2.0);
        let mut scene = p.scene(&s);
        scene.background = slides_engine::scene::Fill::None;
        let mut images = slides_engine::render::Images::default();
        if let Some(pm) = slides_engine::render::render(&p, &scene, &mut images, slides_engine::render::Options { scale: 160.0 / sw, prompts: false, skip_text_of: None, translate: None }) {
            out.push(ctx.load_texture(format!("model-{}", m.0), egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR));
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, out.clone()));
    out
}

/// Insert → 3D Models: the gallery of ready-made 3-D objects.
pub fn model_gallery(app: &mut SlidesApp, ui: &mut egui::Ui) {
    qalam_kit::widgets::menu_heading(ui, "3D Models");
    ui.set_width(4.0 * 104.0);
    let pics = model_pictures(ui.ctx());
    ui.horizontal_wrapped(|ui| for (k, m) in MODELS.iter().enumerate() {
        let (r, resp) = ui.allocate_exact_size(vec2(100.0, 92.0), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr(m.1)));
        if resp.hovered() { ui.painter().rect_filled(r, 4.0, qalam_kit::colors::c_hover()); }
        if let Some(t) = pics.get(k) {
            let a = t.size_vec2();
            let s = (92.0 / a.x).min(66.0 / a.y);
            ui.painter().image(t.id(), Rect::from_center_size(pos2(r.center().x, r.min.y + 36.0), a * s), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        ui.painter().text(pos2(r.center().x, r.max.y - 8.0), egui::Align2::CENTER_CENTER, tr(m.1), egui::FontId::proportional(11.0), qalam_kit::colors::c_ink2());
        if resp.on_hover_text(tr(m.1)).clicked() { insert_model(app, m.0); ui.close(); }
    });
}

pub fn insert_model(app: &mut SlidesApp, id: &str) {
    let Some(slide) = app.slide() else { return };
    let Some(m) = MODELS.iter().find(|m| m.0 == id) else { return };
    let (sw, sh) = app.pres.size();
    app.checkpoint("3-D model");
    if let Some(l) = app.pres.insert_3d_model(&slide, id, (sw - m.3) / 2.0, (sh - m.4) / 2.0) { app.selection = vec![l]; app.edit = None; }
    app.changed();
}

/// The 3-D rotation handle at the center of a selected 3-D object: dragging turns it (left
/// and right turns about the vertical axis, up and down tilts it; Shift keeps 15° steps).
pub fn rotate_handle(app: &mut SlidesApp, ui: &egui::Ui, scene: &slides_engine::scene::Scene, to_screen: &dyn Fn(f64, f64) -> egui::Pos2) {
    if app.selection.len() != 1 || app.edit.is_some() { return; }
    let loc = app.selection[0].clone();
    let Some(it) = scene.items.iter().find(|i| i.own && i.loc.path.first() == loc.path.first()) else { return };
    let t = app.pres.three_d(&loc);
    if !t.visible() { return; }
    let (cx, cy) = it.transform.apply(it.w / 2.0, it.h / 2.0);
    let c = to_screen(cx, cy);
    let r = Rect::from_center_size(c, vec2(30.0, 30.0));
    let id = egui::Id::new("3d-rotate-handle");
    let resp = ui.interact(r, id, Sense::drag());
    let p = ui.painter();
    let hot = resp.hovered() || resp.dragged();
    p.circle_filled(c, 13.0, if hot { qalam_kit::colors::ACCENT } else { Color32::from_white_alpha(230) });
    p.circle_stroke(c, 13.0, Stroke::new(1.5, qalam_kit::colors::ACCENT));
    // A little turning arrow round the middle.
    let col = if hot { Color32::WHITE } else { qalam_kit::colors::ACCENT };
    let pts: Vec<egui::Pos2> = (0..=20).map(|k| { let a = 0.6 + k as f32 / 20.0 * 4.6; c + vec2(a.cos() * 7.0, a.sin() * 4.0) }).collect();
    let tip = *pts.last().unwrap();
    p.add(egui::Shape::line(pts, Stroke::new(1.6, col)));
    p.add(egui::Shape::convex_polygon(vec![tip + vec2(3.0, -2.5), tip + vec2(-1.0, 3.0), tip + vec2(4.0, 3.0)], col, Stroke::NONE));
    if hot { ui.ctx().set_cursor_icon(egui::CursorIcon::Grab); }
    let resp = resp.on_hover_text(tr("Drag to turn the 3-D object"));
    if resp.drag_started() {
        app.checkpoint("3-D rotation");
        ui.ctx().data_mut(|d| d.insert_temp(id, t.rotation()));
    }
    if resp.dragged() {
        let start: (f64, f64, f64) = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(t.rotation());
        let total = resp.drag_delta() + ui.ctx().data(|d| d.get_temp::<egui::Vec2>(id.with("acc"))).unwrap_or_default();
        ui.ctx().data_mut(|d| d.insert_temp(id.with("acc"), total));
        let snap = ui.input(|i| i.modifiers.shift);
        let q = |v: f64| { let v = v.rem_euclid(360.0); if snap { (v / 15.0).round() * 15.0 % 360.0 } else { v } };
        let mut nt = t.clone();
        nt.rot = Some((q(start.0 - total.x as f64 * 0.6), q(start.1 + total.y as f64 * 0.6), start.2));
        if nt.camera.starts_with("oblique") || nt.camera == "orthographicFront" { nt.camera = "orthographicFront".into(); }
        app.pres.set_three_d(&loc, &nt);
        app.changed();
    }
    if resp.drag_stopped() { ui.ctx().data_mut(|d| d.remove::<egui::Vec2>(id.with("acc"))); }
}

/// A gallery picture of a 3-D look (rendered once per look and size).
fn preview_tex(ctx: &egui::Context, t: &ThreeD, size: u32) -> egui::TextureHandle {
    let key = format!("3d-preview-{t:?}-{size}");
    crate::texcache::get_or::<()>(ctx, &key, || {
        let px = (size as f32 * ctx.pixels_per_point() * 1.5) as u32;
        let img = match slides_engine::three_d::preview(t, px) {
            Some((rgba, w, h)) => egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], &rgba),
            None => egui::ColorImage::new([1, 1], vec![Color32::TRANSPARENT]),
        };
        Ok(ctx.load_texture("3d-preview", img, egui::TextureOptions::LINEAR))
    }).unwrap()
}

fn look_tile(ui: &mut egui::Ui, t: &ThreeD, name: &str, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(58.0, 58.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr(name)));
    if on { ui.painter().rect_filled(r, 3.0, Color32::from_rgb(0xFC, 0xE4, 0x9C)); ui.painter().rect_stroke(r, 3.0, Stroke::new(1.5, Color32::from_rgb(0xE5, 0xA8, 0x18)), egui::StrokeKind::Inside); }
    else if resp.hovered() { ui.painter().rect_filled(r, 3.0, qalam_kit::colors::c_hover()); }
    let tex = preview_tex(ui.ctx(), t, 52);
    ui.painter().image(tex.id(), r.shrink(3.0), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    resp.on_hover_text(tr(name))
}

/// The selected shapes' 3-D settings changed by `f` (one undo step).
fn change_3d(app: &mut SlidesApp, f: impl Fn(&mut ThreeD)) {
    let targets = app.selection.clone();
    if targets.is_empty() { return; }
    app.checkpoint("3-D");
    for l in targets { let mut t = app.pres.three_d(&l); f(&mut t); app.pres.set_three_d(&l, &t); }
    app.changed();
}

/// 3-D Rotation presets, drawn: No Rotation, Parallel, Perspective, Oblique.
pub fn rotation_gallery(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let cur = app.selection.first().map(|l| app.pres.three_d(l));
    ui.set_width(5.0 * 62.0);
    egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
        for (title, filter) in [("No Rotation", "orthographicFront"), ("Parallel", "isometric"), ("Perspective", "perspective"), ("Oblique", "oblique")] {
            qalam_kit::widgets::menu_heading(ui, title);
            ui.horizontal_wrapped(|ui| for (k, n, ..) in CAMERAS.iter().filter(|c| c.0.starts_with(filter)) {
                let look = ThreeD { camera: k.to_string(), ..ThreeD::default() };
                let on = cur.as_ref().is_some_and(|c| c.camera == *k && c.rot.is_none());
                if look_tile(ui, &look, n, on).clicked() {
                    let k = k.to_string();
                    change_3d(app, move |t| { t.camera = k.clone(); t.rot = None; t.fov = 0.0; });
                    ui.close();
                }
            });
        }
        ui.separator();
        if ui.button(tr("3-D Rotation Options…")).clicked() { app.pane = crate::app::Pane::Format; app.format_tab = 1; ui.close(); }
    });
}

/// Bevel presets, drawn.
pub fn bevel_gallery(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let cur = app.selection.first().map(|l| app.pres.three_d(l)).and_then(|t| t.top).map(|b| b.kind);
    ui.set_width(4.0 * 62.0);
    qalam_kit::widgets::menu_heading(ui, "No Bevel");
    if look_tile(ui, &ThreeD { material: "flat".into(), ..ThreeD::default() }, "No Bevel", cur.is_none()).clicked() { change_3d(app, |t| t.top = None); ui.close(); }
    qalam_kit::widgets::menu_heading(ui, "Bevel");
    ui.horizontal_wrapped(|ui| for (k, n) in BEVELS {
        let look = ThreeD { top: Some(Bevel { kind: k.to_string(), w: 120_000.0, h: 120_000.0 }), ..ThreeD::default() };
        if look_tile(ui, &look, n, cur.as_deref() == Some(*k)).clicked() {
            let k = k.to_string();
            change_3d(app, move |t| { let (w, h) = t.top.as_ref().map(|b| (b.w, b.h)).unwrap_or((6.0 * PT, 6.0 * PT)); t.top = Some(Bevel { kind: k.clone(), w, h }); });
            ui.close();
        }
    });
    ui.separator();
    if ui.button(tr("3-D Options…")).clicked() { app.pane = crate::app::Pane::Format; app.format_tab = 1; ui.close(); }
}
