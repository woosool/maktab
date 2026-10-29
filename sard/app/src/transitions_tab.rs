//! The Transitions tab: Preview; Transition to This Slide (the gallery by category, with
//! each tile showing its effect half-way); Effect Options; Timing (sound, duration, apply to
//! all, advance on click and after a time).

use crate::app::SlidesApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::transition::{self, Transition, EFFECTS, SOUNDS};

const TILE: egui::Vec2 = vec2(64.0, 58.0);

/// Two little "slides" the gallery tiles transition between.
fn sample_textures(ui: &Ui) -> (egui::TextureHandle, egui::TextureHandle) {
    let id = egui::Id::new("transition-samples");
    if let Some(t) = ui.data(|d| d.get_temp::<(egui::TextureHandle, egui::TextureHandle)>(id)) { return t; }
    let make = |dark: bool| {
        let (w, h) = (64usize, 36usize);
        let mut px = vec![Color32::WHITE; w * h];
        for y in 0..h { for x in 0..w {
            px[y * w + x] = if dark { Color32::from_rgb(0x2F + (y as u8), 0x6E + (x as u8 / 2), 0xC8) } else if y < 8 { Color32::from_rgb(0xDE, 0xEB, 0xF7) } else if (12..15).contains(&y) && x > 6 && x < 50 { Color32::from_gray(150) } else if (19..21).contains(&y) && x > 6 && x < 40 { Color32::from_gray(190) } else { Color32::WHITE };
        } }
        ui.ctx().load_texture(if dark { "tr-b" } else { "tr-a" }, egui::ColorImage { size: [w, h], source_size: egui::vec2(w as f32, h as f32), pixels: px }, egui::TextureOptions::LINEAR)
    };
    let t = (make(false), make(true));
    ui.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// One gallery tile: the effect frozen mid-way, and its name.
fn effect_tile(ui: &mut Ui, id: &str, name: &str, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(TILE, Sense::click());
    if on { ui.painter().rect_filled(r, 3.0, ACCENT.gamma_multiply(0.15)); } else if resp.hovered() { ui.painter().rect_filled(r, 3.0, c_hover()); }
    let pic = Rect::from_center_size(pos2(r.center().x, r.min.y + 20.0), vec2(40.0, 24.0));
    let (a, b) = sample_textures(ui);
    if id == "none" { ui.painter().image(a.id(), pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
    else { crate::transition_fx::draw(&ui.painter().with_clip_rect(pic.expand(1.0)), id, "", 0.55, pic, Some(a.id()), b.id()); }
    ui.painter().rect_stroke(pic, 0.0, Stroke::new(1.0, c_line()), egui::StrokeKind::Outside);
    let g = ui.painter().layout(tr(name), egui::FontId::proportional(10.5), c_ink2(), TILE.x - 2.0);
    ui.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.min.y + 36.0), g, c_ink2());
    if on { ui.painter().rect_stroke(r, 3.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside); }
    resp.on_hover_text(tr(name))
}

fn targets(app: &SlidesApp) -> Vec<String> {
    let slides = app.slides();
    if app.selected_slides.len() > 1 { app.selected_slides.iter().filter_map(|&i| slides.get(i).cloned()).collect() } else { app.slide().into_iter().collect() }
}

/// Change the transition of the current (or selected) slides with `f`, then preview it.
fn change(app: &mut SlidesApp, f: impl Fn(&mut Transition), preview: bool) {
    let list = targets(app);
    if list.is_empty() { return; }
    app.checkpoint("transition");
    for s in &list { let mut t = app.pres.slide_transition(s); f(&mut t); app.pres.set_slide_transition(s, &t); }
    if preview { app.start_preview(); }
}

pub fn transitions(app: &mut SlidesApp, ui: &mut Ui) {
    let Some(slide) = app.slide() else { return };
    let cur = app.pres.slide_transition(&slide);
    group(ui, "Preview", |ui| {
        if large(ui, "previewTransition", "Preview", false, cur.effect != "none", "Preview the transition on this slide").clicked() { app.start_preview(); }
    });
    group(ui, "Transition to This Slide", |ui| {
        let strip = ((ui.ctx().content_rect().width() - 760.0) / (TILE.x + 2.0)).floor().clamp(4.0, 9.0) as usize;
        for e in EFFECTS.iter().take(strip) {
            if effect_tile(ui, e.id, e.name, cur.effect == e.id).clicked() { pick(app, e.id); }
        }
        let more = large_menu(ui, "transitionGallery", "", "All transitions");
        crate::theme::menu(&more, |ui| {
            ui.set_width(8.0 * (TILE.x + 4.0));
            egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| for cat in ["Subtle", "Exciting", "Dynamic Content"] {
                menu_heading(ui, cat);
                ui.horizontal_wrapped(|ui| for e in EFFECTS.iter().filter(|e| e.category == cat) {
                    if effect_tile(ui, e.id, e.name, cur.effect == e.id).clicked() { pick(app, e.id); ui.close(); }
                });
            });
        });
        let opts = transition::effect(&cur.effect).map(|e| e.options).unwrap_or(&[]);
        let eo = large_menu(ui, "effectOptions", "Effect Options", "Direction and variation of the transition");
        if !opts.is_empty() {
            crate::theme::menu(&eo, |ui| for (id, label) in opts {
                if ui.selectable_label(cur.option == *id, tr(label)).clicked() { let id = id.to_string(); change(app, move |t| t.option = id.clone(), true); ui.close(); }
            });
        }
    });
    group(ui, "Timing", |ui| {
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                paint_label(ui, "volume", "Sound:");
                let label = cur.sound.clone().map(|s| tr(&s)).unwrap_or_else(|| tr("[No Sound]"));
                crate::theme::Combo::from_id_salt("tr-sound").width(130.0).selected_text(label).show_ui(ui, |ui| {
                    if ui.selectable_label(cur.sound.is_none(), tr("[No Sound]")).clicked() { change(app, |t| t.sound = None, false); }
                    for s in SOUNDS { if ui.selectable_label(cur.sound.as_deref() == Some(*s), tr(s)).clicked() { let name = s.to_string(); play_sound(&transition::sound_wav(&name)); change(app, move |t| t.sound = Some(name.clone()), false); } }
                });
            });
            crate::theme::hrow(ui, |ui| {
                paint_label(ui, "duration", "Duration:");
                let mut secs = cur.duration_ms as f64 / 1000.0;
                let r = ui.add(egui::DragValue::new(&mut secs).range(0.01..=59.0).speed(0.05).max_decimals(2).min_decimals(2));
                // Only a real edit changes it (not the field settling on its first frame).
                let ms = (secs * 1000.0).round() as u32;
                if r.changed() && (r.dragged() || r.has_focus() || r.lost_focus()) && ms != cur.duration_ms { change(app, move |t| t.duration_ms = ms, false); }
            });
            if small(ui, "applyToAll", Some("Apply To All"), false, true, "Use these transition settings for every slide").clicked() {
                let t = cur.clone();
                app.checkpoint("transition");
                for s in app.slides() { app.pres.set_slide_transition(&s, &t); }
                app.notify(&tr("Applied to all slides"));
            }
        });
        ui.add_space(8.0);
        rows(ui, |ui| {
            ui.label(egui::RichText::new(tr("Advance Slide")).size(12.0).color(c_ink2()));
            let mut click = cur.on_click;
            if crate::theme::check(ui, &mut click, tr("On Mouse Click")).changed() { change(app, move |t| t.on_click = click, false); }
            crate::theme::hrow(ui, |ui| {
                let mut after = cur.after_ms.is_some();
                if crate::theme::check(ui, &mut after, tr("After:")).changed() { change(app, move |t| t.after_ms = if after { Some(t.after_ms.unwrap_or(0)) } else { None }, false); }
                let mut secs = cur.after_ms.unwrap_or(0) as f64 / 1000.0;
                let r = ui.add_enabled(cur.after_ms.is_some(), egui::DragValue::new(&mut secs).range(0.0..=3600.0).speed(0.1).max_decimals(2).custom_formatter(|v, _| format!("{:02}:{:05.2}", (v / 60.0) as u32, v % 60.0)));
                let ms = (secs * 1000.0).round() as u32;
                if r.changed() && (r.dragged() || r.has_focus() || r.lost_focus()) && Some(ms) != cur.after_ms { change(app, move |t| t.after_ms = Some(ms), false); }
            });
        });
    });
}

fn paint_label(ui: &mut Ui, icon: &str, text: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    paint_icon(ui, icon, r, 16, true);
    ui.label(egui::RichText::new(tr(text)).size(12.0).color(c_ink2()));
}

fn pick(app: &mut SlidesApp, id: &str) {
    let id = id.to_string();
    let first = transition::effect(&id).and_then(|e| e.options.first()).map(|o| o.0.to_string()).unwrap_or_default();
    change(app, move |t| {
        let changed = t.effect != id;
        t.effect = id.clone();
        if changed { t.option = first.clone(); t.duration_ms = transition::default_duration(&id); }
    }, true);
}

/// Play WAV bytes with the system's player (transition sounds).
pub fn play_sound(wav: &[u8]) {
    let path = std::env::temp_dir().join(format!("sard-sound-{}.wav", wav.len()));
    if std::fs::write(&path, wav).is_err() { return; }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("afplay").arg(&path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &format!("(New-Object Media.SoundPlayer '{}').PlaySync()", path.display())]).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("aplay").arg("-q").arg(&path).spawn();
}

#[cfg(test)]
pub fn tile_for_test(ui: &mut Ui, id: &str, name: &str) { effect_tile(ui, id, name, false); }
