//! The Animations tab: Preview; the effect gallery (entrance, emphasis, exit, motion paths);
//! Effect Options; Advanced Animation (Add Animation, Animation Pane, Trigger, Animation
//! Painter); Timing (start, duration, delay, reorder). Also the Animation Pane.

use crate::app::{Pane, SlidesApp};
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::animation::{self, Anim, Class, Start, PRESETS};

const TILE: egui::Vec2 = vec2(64.0, 58.0);

pub fn class_color(c: Class) -> Color32 {
    match c { Class::Entrance => Color32::from_rgb(0x4C, 0xAF, 0x50), Class::Emphasis => Color32::from_rgb(0xE8, 0xA8, 0x00), Class::Exit => Color32::from_rgb(0xE0, 0x4B, 0x3A), Class::Path => Color32::from_rgb(0x3B, 0x82, 0xD6) }
}

/// The effect's emblem: a star in its class color (a dotted path for motion paths), posed as
/// the effect would pose it at `t`.
pub fn emblem(p: &egui::Painter, r: Rect, effect: &str, option: &str, t: f64) {
    let Some(pr) = animation::preset(effect) else { return };
    let col = class_color(pr.class);
    if pr.class == Class::Path {
        let pts = animation::path_points(&animation::motion_path(effect, option));
        let (mut lo, mut hi) = (pos2(f32::MAX, f32::MAX), pos2(f32::MIN, f32::MIN));
        for &(x, y) in &pts { lo = lo.min(pos2(x as f32, y as f32)); hi = hi.max(pos2(x as f32, y as f32)); }
        let span = (hi - lo).max(vec2(0.01, 0.01));
        let k = (r.width() / span.x).min(r.height() / span.y) * 0.8;
        let off = r.center() - (lo + span / 2.0).to_vec2() * k;
        let line: Vec<Pos2> = pts.iter().map(|&(x, y)| off + vec2(x as f32, y as f32) * k).collect();
        for w in line.windows(2).step_by(2) { p.line_segment([w[0], w[1]], Stroke::new(1.5, col)); }
        if let (Some(a), Some(b)) = (line.first(), line.last()) { p.circle_filled(*a, 2.5, Color32::from_rgb(0x4C, 0xAF, 0x50)); p.circle_filled(*b, 2.5, Color32::from_rgb(0xE0, 0x4B, 0x3A)); }
        return;
    }
    let a = Anim::new(0, effect);
    let a = Anim { option: option.into(), ..a };
    let pose = animation::pose(&[(&a, Some(t * a.duration_ms.max(1) as f64))], 0.4, 0.4);
    if !pose.visible { return; }
    let c = r.center() + vec2(pose.dx as f32 * r.width() * 0.3, pose.dy as f32 * r.height() * 0.3);
    let rad = r.height().min(r.width()) * 0.42;
    let rot = pose.rot.to_radians() as f32;
    let fill = match &pose.color { Some((h, k)) => { let v = u32::from_str_radix(h, 16).unwrap_or(0); let to = Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8); lerp_color(col, to, *k) } None => col };
    let pts: Vec<Pos2> = (0..10).map(|i| {
        let ang = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0 + rot;
        let rr = if i % 2 == 0 { rad } else { rad * 0.45 };
        c + vec2(ang.cos() * rr * pose.sx as f32, ang.sin() * rr * pose.sy as f32)
    }).collect();
    let fill = fill.gamma_multiply(pose.opacity.clamp(0.05, 1.0));
    // A star isn't convex: fan it from the center.
    let mut m = egui::Mesh::default();
    m.colored_vertex(c, fill);
    for q in &pts { m.colored_vertex(*q, fill); }
    for i in 0..10u32 { m.add_triangle(0, 1 + i, 1 + (i + 1) % 10); }
    p.add(egui::Shape::mesh(m));
    p.add(egui::Shape::closed_line(pts, Stroke::new(1.0, fill.gamma_multiply(0.7))));
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn effect_tile(ui: &mut Ui, id: &str, name: &str, on: bool, enabled: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(TILE, if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tr(name)));
    if on { ui.painter().rect_filled(r, 3.0, ACCENT.gamma_multiply(0.15)); } else if resp.hovered() && enabled { ui.painter().rect_filled(r, 3.0, c_hover()); }
    let pic = Rect::from_center_size(pos2(r.center().x, r.min.y + 18.0), vec2(30.0, 28.0));
    if id == "none" {
        paint_icon(ui, "noAnimation", Rect::from_center_size(pic.center(), vec2(24.0, 24.0)), 24, enabled);
    } else {
        // Hovering plays the effect in the tile.
        let t = if resp.hovered() && enabled { ui.ctx().request_repaint(); (ui.input(|i| i.time) * 0.8).fract() } else { 0.85 };
        let p = ui.painter().with_clip_rect(r);
        if enabled { emblem(&p, pic, id, "", t); } else { emblem(&p.with_clip_rect(r), pic, id, "", 1.0); ui.painter().rect_filled(r, 3.0, c_card().gamma_multiply(0.6)); }
    }
    let g = ui.painter().layout(tr(name), egui::FontId::proportional(10.5), if enabled { c_ink2() } else { c_ink3() }, TILE.x - 2.0);
    ui.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.min.y + 36.0), g, c_ink2());
    if on { ui.painter().rect_stroke(r, 3.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside); }
    resp.on_hover_text(tr(name))
}

/// The cNvPr ids of the selected top-level shapes.
fn selected_ids(app: &SlidesApp) -> Vec<u32> {
    let mut v: Vec<u32> = app.selection.iter().filter_map(|l| app.pres.shape_id(&slides_engine::scene::Loc { part: l.part.clone(), path: vec![l.path[0]] })).collect();
    v.dedup();
    v
}

/// The animations the timing controls act on: the one picked in the pane, else those of the
/// selected shapes.
fn current(app: &SlidesApp, list: &[Anim]) -> Vec<usize> {
    if let Some(i) = app.anim_sel.filter(|&i| i < list.len()) { return vec![i]; }
    let ids = selected_ids(app);
    (0..list.len()).filter(|&i| ids.contains(&list[i].shape)).collect()
}

/// Change the slide's animations with `f`.
pub fn edit(app: &mut SlidesApp, f: impl FnOnce(&mut Vec<Anim>, &[usize], &[u32])) {
    let Some(slide) = app.slide() else { return };
    let mut list = app.pres.animations(&slide);
    let cur = current(app, &list);
    let ids = selected_ids(app);
    app.checkpoint("animation");
    f(&mut list, &cur, &ids);
    app.pres.set_animations(&slide, &list);
    if app.anim_sel.is_some_and(|i| i >= list.len()) { app.anim_sel = None; }
}

/// Apply the effect to the selection: it replaces the picked (or each shape's last) effect,
/// or adds one where a shape has none.
fn pick(app: &mut SlidesApp, id: &str) {
    let sel = app.anim_sel;
    edit(app, |list, cur, ids| {
        if id == "none" { list.retain(|a| !ids.contains(&a.shape)); return; }
        if let Some(i) = sel.filter(|&i| i < list.len()) { let s = list[i].shape; let keep = (list[i].start, list[i].delay_ms, list[i].trigger); list[i] = Anim { start: keep.0, delay_ms: keep.1, trigger: keep.2, ..Anim::new(s, id) }; return; }
        for &s in ids {
            match cur.iter().rev().find(|&&i| list[i].shape == s) {
                Some(&i) => { let keep = (list[i].start, list[i].delay_ms, list[i].trigger); list[i] = Anim { start: keep.0, delay_ms: keep.1, trigger: keep.2, ..Anim::new(s, id) }; }
                None => list.push(Anim::new(s, id)),
            }
        }
    });
    start_preview(app);
}

fn add(app: &mut SlidesApp, id: &str) {
    let mut added = 0;
    edit(app, |list, _, ids| for &s in ids { list.push(Anim::new(s, id)); added = list.len(); });
    if added > 0 { app.anim_sel = Some(added - 1); }
    start_preview(app);
}

pub fn start_preview(app: &mut SlidesApp) {
    let Some(slide) = app.slide() else { return };
    let Some((_, scale)) = app.canvas.view else { return };
    let (sw, _) = app.pres.size();
    let px = (sw as f32 * scale * app.ctx.pixels_per_point()).round().max(1.0) as u32;
    let ctx = app.ctx.clone();
    if let Some(l) = crate::anim_play::build(app, &ctx, &slide, px) {
        if !l.anims.is_empty() { app.anim_preview = Some((ctx.input(|i| i.time), l)); }
    }
}

fn gallery_menu(app: &mut SlidesApp, ui: &mut Ui, adding: bool, on: Option<&str>) {
    ui.set_width(8.0 * (TILE.x + 4.0));
    let enabled = !app.selection.is_empty();
    egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
        if !adding {
            menu_heading(ui, "None");
            ui.horizontal_wrapped(|ui| if effect_tile(ui, "none", "None", on.is_none(), enabled).clicked() { pick(app, "none"); ui.close(); });
        }
        for (class, title) in [(Class::Entrance, "Entrance"), (Class::Emphasis, "Emphasis"), (Class::Exit, "Exit"), (Class::Path, "Motion Paths")] {
            menu_heading(ui, title);
            ui.horizontal_wrapped(|ui| for p in PRESETS.iter().filter(|p| p.class == class) {
                if effect_tile(ui, p.id, p.name, on == Some(p.id), enabled).clicked() { if adding { add(app, p.id) } else { pick(app, p.id) } ui.close(); }
            });
        }
    });
}

pub fn animations(app: &mut SlidesApp, ui: &mut Ui) {
    let Some(slide) = app.slide() else { return };
    let list = app.pres.animations(&slide);
    let cur = current(app, &list);
    let first = cur.first().map(|&i| list[i].clone());
    let has_sel = !app.selection.is_empty();
    group(ui, "Preview", |ui| {
        if large(ui, "previewAnimation", "Preview", false, !list.is_empty(), "Play this slide's animations").clicked() { start_preview(app); }
    });
    group(ui, "Animation", |ui| {
        let on = first.as_ref().map(|a| a.effect.as_str());
        let strip = ((ui.ctx().content_rect().width() - 900.0) / (TILE.x + 2.0)).floor().clamp(3.0, 8.0) as usize;
        if effect_tile(ui, "none", "None", has_sel && on.is_none(), has_sel).clicked() { pick(app, "none"); }
        for p in PRESETS.iter().filter(|p| p.class == Class::Entrance).take(strip) {
            if effect_tile(ui, p.id, p.name, on == Some(p.id), has_sel).clicked() { pick(app, p.id); }
        }
        let more = large_menu(ui, "animationGallery", "", "All effects");
        let on = on.map(String::from);
        crate::theme::menu(&more, |ui| gallery_menu(app, ui, false, on.as_deref()));
        let opts = first.as_ref().and_then(|a| animation::preset(&a.effect)).map(|p| p.options).unwrap_or(&[]);
        let color_fx = first.as_ref().is_some_and(|a| matches!(a.effect.as_str(), "objectColor" | "fillColor" | "lineColor" | "fontColor" | "colorPulse" | "complementary"));
        let eo = large_menu(ui, "effectOptions", "Effect Options", "Direction, amount or color of the effect");
        if !opts.is_empty() || color_fx {
            let opt = first.as_ref().map(|a| a.option.clone()).unwrap_or_default();
            crate::theme::menu(&eo, |ui| {
                for (id, label) in opts {
                    if ui.selectable_label(opt == *id, tr(label)).clicked() { let id = id.to_string(); edit(app, |l, c, _| for &i in c { l[i].option = id.clone(); }); start_preview(app); ui.close(); }
                }
                if color_fx {
                    menu_heading(ui, "Color");
                    ui.horizontal_wrapped(|ui| for hex in ["FF0000", "FFC000", "FFFF00", "92D050", "00B050", "00B0F0", "0070C0", "7030A0", "000000", "FFFFFF"] {
                        let v = u32::from_str_radix(hex, 16).unwrap();
                        if swatch(ui, Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8), 18.0, false).on_hover_text(hex).clicked() { let h = hex.to_string(); edit(app, |l, c, _| for &i in c { l[i].color = Some(h.clone()); }); start_preview(app); ui.close(); }
                    });
                }
            });
        }
    });
    group(ui, "Advanced Animation", |ui| {
        let aa = large_menu(ui, "addAnimation", "Add Animation", "Add another effect to the selected objects");
        if has_sel { crate::theme::menu(&aa, |ui| gallery_menu(app, ui, true, None)); }
        rows(ui, |ui| {
            if small(ui, "animationPane", Some("Animation Pane"), app.pane == Pane::Animation, true, "Show the slide's animations in order").clicked() { app.pane = if app.pane == Pane::Animation { Pane::None } else { Pane::Animation }; }
            let tm = small_menu(ui, "trigger", "Trigger", "Play the effect when an object is clicked");
            if !cur.is_empty() {
                let shapes: Vec<(u32, String)> = shape_names(app, &slide);
                let trig = first.as_ref().and_then(|a| a.trigger);
                crate::theme::menu(&tm, |ui| {
                    if ui.selectable_label(trig.is_none(), tr("As Part of Click Sequence")).clicked() { edit(app, |l, c, _| for &i in c { l[i].trigger = None; }); ui.close(); }
                    menu_heading(ui, "On Click of");
                    for (id, name) in &shapes {
                        if ui.selectable_label(trig == Some(*id), name).clicked() { let id = *id; edit(app, |l, c, _| for &i in c { l[i].trigger = Some(id); }); ui.close(); }
                    }
                });
            }
            let painting = app.anim_painter.is_some();
            if small(ui, "animationPainter", Some("Animation Painter"), painting, first.is_some() || painting, "Copy these animations to another object").clicked() {
                app.anim_painter = if painting { None } else {
                    let s = first.as_ref().map(|a| a.shape).unwrap_or(0);
                    Some(list.iter().filter(|a| a.shape == s).cloned().collect())
                };
            }
        });
    });
    group(ui, "Timing", |ui| {
        let enabled = first.is_some();
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                paint_label(ui, "playFromStart", "Start:");
                let start = first.as_ref().map(|a| a.start).unwrap_or(Start::OnClick);
                let name = |s: Start| match s { Start::OnClick => "On Click", Start::WithPrevious => "With Previous", Start::AfterPrevious => "After Previous" };
                ui.add_enabled_ui(enabled, |ui| crate::theme::Combo::from_id_salt("anim-start").width(120.0).selected_text(tr(name(start))).show_ui(ui, |ui| {
                    for s in [Start::OnClick, Start::WithPrevious, Start::AfterPrevious] {
                        if ui.selectable_label(start == s, tr(name(s))).clicked() { edit(app, |l, c, _| for &i in c { l[i].start = s; }); }
                    }
                }));
            });
            ms_field(app, ui, "duration", "Duration:", "anim-dur", enabled, first.as_ref().map(|a| a.duration_ms).unwrap_or(500), 0.0..=59.0, |a, ms| a.duration_ms = ms);
            ms_field(app, ui, "delay", "Delay:", "anim-delay", enabled, first.as_ref().map(|a| a.delay_ms).unwrap_or(0), 0.0..=3600.0, |a, ms| a.delay_ms = ms);
        });
        ui.add_space(8.0);
        rows(ui, |ui| {
            ui.label(egui::RichText::new(tr("Reorder Animation")).size(12.0).color(c_ink2()));
            let one = cur.len() == 1;
            let i = cur.first().copied().unwrap_or(0);
            if small(ui, "moveEarlier", Some("Move Earlier"), false, one && i > 0, "Play this effect sooner").clicked() { reorder(app, i, -1); }
            if small(ui, "moveLater", Some("Move Later"), false, one && i + 1 < list.len(), "Play this effect later").clicked() { reorder(app, i, 1); }
        });
    });
}

pub fn reorder(app: &mut SlidesApp, i: usize, by: i32) {
    let j = i as i32 + by;
    edit(app, |l, _, _| if j >= 0 && (j as usize) < l.len() { l.swap(i, j as usize); });
    if j >= 0 { app.anim_sel = Some(j as usize); }
}

fn ms_field(app: &mut SlidesApp, ui: &mut Ui, icon: &str, label: &str, id: &str, enabled: bool, ms: u32, range: std::ops::RangeInclusive<f64>, set: impl Fn(&mut Anim, u32) + Copy) {
    crate::theme::hrow(ui, |ui| {
        paint_label(ui, icon, label);
        let mut secs = ms as f64 / 1000.0;
        let r = ui.push_id(id, |ui| ui.add_enabled(enabled, egui::DragValue::new(&mut secs).range(range).speed(0.05).max_decimals(2).min_decimals(2))).inner;
        let v = (secs * 1000.0).round() as u32;
        if r.changed() && (r.dragged() || r.has_focus() || r.lost_focus()) && v != ms { edit(app, move |l, c, _| for &i in c { set(&mut l[i], v); }); }
    });
}

fn paint_label(ui: &mut Ui, icon: &str, text: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    paint_icon(ui, icon, r, 16, true);
    ui.label(egui::RichText::new(tr(text)).size(12.0).color(c_ink2()));
}

/// (cNvPr id, name) of the slide's top-level shapes.
pub fn shape_names(app: &SlidesApp, slide: &str) -> Vec<(u32, String)> {
    let Some(tree) = app.pres.slide(slide).and_then(|r| r.path("p:cSld/p:spTree")) else { return vec![] };
    tree.elements().filter_map(|e| {
        let c = e.elements().find(|c| c.local().starts_with("nv"))?.child("p:cNvPr")?;
        Some((c.attr_i64("id")? as u32, c.attr("name").unwrap_or("Shape").to_string()))
    }).collect()
}

/// Animation Painter: give `loc`'s shape the copied animations.
pub fn paint_onto(app: &mut SlidesApp, loc: &slides_engine::scene::Loc) {
    let (Some(copied), Some(id)) = (app.anim_painter.take(), app.pres.shape_id(&slides_engine::scene::Loc { part: loc.part.clone(), path: vec![loc.path[0]] })) else { return };
    let part = loc.part.clone();
    let mut list = app.pres.animations(&part);
    app.checkpoint("animation");
    list.retain(|a| a.shape != id);
    for a in copied { list.push(Anim { shape: id, ..a }); }
    app.pres.set_animations(&part, &list);
}

/// The Animation Pane: the slide's effects in order with their timeline bars.
pub fn pane(app: &mut SlidesApp, ui: &mut Ui) {
    let Some(slide) = app.slide() else { return };
    let list = app.pres.animations(&slide);
    let names = shape_names(app, &slide);
    crate::theme::hrow(ui, |ui| {
        let label = if app.anim_sel.is_some() { "Play Selected" } else { "Play All" };
        if ui.add_enabled(!list.is_empty(), egui::Button::new(format!("▶ {}", tr(label)))).clicked() { start_preview(app); }
        ui.with_layout(far_layout(), |ui| {
            if ui.add_enabled(app.anim_sel.is_some_and(|i| i + 1 < list.len()), egui::Button::new("▼")).on_hover_text(tr("Move Later")).clicked() { let i = app.anim_sel.unwrap(); reorder(app, i, 1); }
            if ui.add_enabled(app.anim_sel.is_some_and(|i| i > 0), egui::Button::new("▲")).on_hover_text(tr("Move Earlier")).clicked() { let i = app.anim_sel.unwrap(); reorder(app, i, -1); }
        });
    });
    ui.add_space(6.0);
    if list.is_empty() { ui.label(egui::RichText::new(tr("Select an object on the slide and pick an effect to animate it.")).color(c_ink3())); return; }
    let (sched, _) = animation::schedule(&list);
    let total = sched.iter().map(|t| t.at_ms + list[t.index].duration_ms.max(1)).max().unwrap_or(1).max(1000) as f32;
    let mut step_shown = usize::MAX;
    let mut trig_shown = None;
    let mut remove = None;
    for (i, a) in list.iter().enumerate() {
        let timed = sched.iter().find(|t| t.index == i);
        // Headings: the click step, or the trigger.
        if let Some(t) = timed { if t.step != step_shown { step_shown = t.step; ui.label(egui::RichText::new(if t.step == 0 { tr("Automatically") } else { format!("{} {}", tr("Click"), t.step) }).size(11.0).color(c_ink3())); } }
        else if a.trigger != trig_shown { trig_shown = a.trigger; let n = names.iter().find(|n| Some(n.0) == a.trigger).map(|n| n.1.clone()).unwrap_or_default(); ui.label(egui::RichText::new(format!("{}: {}", tr("Trigger"), n)).size(11.0).color(c_ink3())); }
        let on = app.anim_sel == Some(i);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        if on { ui.painter().rect_filled(r, 3.0, ACCENT.gamma_multiply(0.15)); } else if resp.hovered() { ui.painter().rect_filled(r, 3.0, c_hover()); }
        let p = ui.painter();
        let mut x = r.min.x + 4.0;
        let num = timed.map(|t| if a.start == Start::OnClick { t.step.to_string() } else { String::new() }).unwrap_or_default();
        p.text(pos2(x + 6.0, r.center().y), egui::Align2::CENTER_CENTER, num, egui::FontId::proportional(11.0), c_ink3());
        x += 14.0;
        let icon = match a.start { Start::OnClick => "🖱", Start::WithPrevious => "", Start::AfterPrevious => "⏱" };
        p.text(pos2(x + 6.0, r.center().y), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(11.0), c_ink3());
        x += 14.0;
        emblem(p, Rect::from_min_size(pos2(x, r.min.y + 4.0), vec2(18.0, 18.0)), &a.effect, &a.option, 1.0);
        x += 22.0;
        let name = names.iter().find(|n| n.0 == a.shape).map(|n| n.1.clone()).unwrap_or_else(|| format!("#{}", a.shape));
        let bar_w = 70.0;
        let text_w = (r.max.x - x - bar_w - 8.0).max(20.0);
        let g = p.layout_no_wrap(format!("{}: {}", tr(a.name()), name), egui::FontId::proportional(12.0), c_ink());
        p.with_clip_rect(Rect::from_min_size(pos2(x, r.min.y), vec2(text_w, r.height()))).galley(pos2(x, r.center().y - g.size().y / 2.0), g, c_ink());
        // The timeline bar.
        let bar = Rect::from_min_max(pos2(r.max.x - bar_w - 4.0, r.min.y + 8.0), pos2(r.max.x - 4.0, r.max.y - 8.0));
        let at = timed.map(|t| t.at_ms).unwrap_or(a.delay_ms) as f32;
        let b = Rect::from_min_max(pos2(bar.min.x + bar.width() * at / total, bar.min.y), pos2((bar.min.x + bar.width() * (at + a.duration_ms.max(60) as f32) / total).min(bar.max.x), bar.max.y));
        p.rect_filled(b, 2.0, class_color(a.class()));
        if resp.clicked() {
            app.anim_sel = if on { None } else { Some(i) };
            if let Some(l) = app.pres.shape_by_id(&slide, a.shape) { app.selection = vec![l]; }
        }
        resp.context_menu(|ui| {
            for (s, label) in [(Start::OnClick, "Start On Click"), (Start::WithPrevious, "Start With Previous"), (Start::AfterPrevious, "Start After Previous")] {
                if ui.selectable_label(a.start == s, tr(label)).clicked() { app.anim_sel = Some(i); edit(app, |l, _, _| l[i].start = s); ui.close(); }
            }
            ui.separator();
            if ui.button(tr("Move Earlier")).clicked() { reorder(app, i, -1); ui.close(); }
            if ui.button(tr("Move Later")).clicked() { reorder(app, i, 1); ui.close(); }
            ui.menu_button(tr("Repeat"), |ui| for (n, label) in [(0u32, "(none)"), (2, "2"), (3, "3"), (5, "5"), (10, "10"), (u32::MAX, "Until Next Click")] {
                if ui.selectable_label(a.repeat == n, tr(label)).clicked() { edit(app, |l, _, _| l[i].repeat = n); ui.close(); }
            });
            let mut rw = a.rewind;
            if ui.checkbox(&mut rw, tr("Rewind when done playing")).changed() { edit(app, |l, _, _| l[i].rewind = rw); }
            ui.separator();
            if ui.button(tr("Remove")).clicked() { remove = Some(i); ui.close(); }
        });
    }
    if let Some(i) = remove { edit(app, |l, _, _| { l.remove(i); }); app.anim_sel = None; }
}

/// Little numbered tags beside animated objects on the editing canvas (while the tab is open).
pub fn tags(app: &SlidesApp, painter: &egui::Painter, slide: &str, to_screen: &dyn Fn(f64, f64) -> Pos2) {
    let list = app.pres.animations(slide);
    if list.is_empty() { return; }
    let (sched, _) = animation::schedule(&list);
    let scene = app.pres.scene(slide);
    let mut placed: Vec<(usize, usize)> = vec![];
    for t in &sched {
        let a = &list[t.index];
        let Some(loc) = app.pres.shape_by_id(slide, a.shape) else { continue };
        let Some(it) = scene.items.iter().find(|i| i.own && i.loc.path.first() == loc.path.first()) else { continue };
        let (x, y) = it.transform.apply(0.0, 0.0);
        let n = placed.iter().filter(|p| p.0 == loc.path[0]).count();
        placed.push((loc.path[0], t.step));
        let p = to_screen(x, y) + vec2(-18.0, n as f32 * 18.0);
        let r = Rect::from_min_size(p, vec2(16.0, 16.0));
        let sel = app.anim_sel == Some(t.index);
        painter.rect_filled(r, 2.0, if sel { ACCENT } else { Color32::from_gray(235) });
        painter.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_gray(140)), egui::StrokeKind::Inside);
        painter.text(r.center(), egui::Align2::CENTER_CENTER, t.step.to_string(), egui::FontId::proportional(10.0), if sel { Color32::WHITE } else { Color32::from_gray(40) });
    }
}
