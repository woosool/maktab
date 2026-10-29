//! The slide navigator: numbered thumbnails down the side, with Shift/Ctrl selection,
//! drag to reorder, a right-click menu, and hidden slides marked.

use crate::app::SlidesApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke};
use qalam_kit::colors::*;
use slides_engine::render::{self, Options};

const THUMB_W: f32 = 168.0;

pub fn panel(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let slides = app.slides();
    let (sw, sh) = app.pres.size();
    let th = THUMB_W * (sh / sw) as f32;
    let panel = if crate::i18n::rtl() { egui::Panel::right("navigator") } else { egui::Panel::left("navigator") };
    panel.resizable(false).exact_size(THUMB_W + 52.0).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin { left: 8, right: 8, top: 10, bottom: 10 })).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let ppp = ui.ctx().pixels_per_point();
            let mut rendered = 0;
            let mut drop_target: Option<usize> = None;
            let dragging = ui.ctx().dragged_id().is_some_and(|id| id == egui::Id::new("nav-drag"));
            let sections = app.pres.sections();
            let mut collapsed = false;
            for (i, part) in slides.iter().enumerate() {
                // A section header where a section starts.
                if let Some((k, (name, first))) = sections.iter().enumerate().find(|(_, s)| s.1 == i) {
                    collapsed = app.collapsed_sections.contains(first);
                    let (hr, hresp) = ui.allocate_exact_size(vec2(THUMB_W + 34.0, 24.0), Sense::click());
                    if hresp.hovered() { ui.painter().rect_filled(hr, 3.0, c_hover()); }
                    let arrow = if collapsed { "▸" } else { "▾" };
                    let end = sections.get(k + 1).map(|s| s.1).unwrap_or(slides.len());
                    match app.renaming_section.clone() {
                        Some((at, mut text)) if at == *first => {
                            let r = ui.put(hr.shrink2(vec2(16.0, 2.0)), egui::TextEdit::singleline(&mut text));
                            if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                let mut secs = sections.clone();
                                if let Some(sx) = secs.iter_mut().find(|x| x.1 == at) { sx.0 = text.clone(); }
                                app.checkpoint("section");
                                app.pres.set_sections(secs);
                                app.renaming_section = None;
                            } else { r.request_focus(); app.renaming_section = Some((at, text)); }
                        }
                        _ => { ui.painter().text(hr.min + vec2(6.0, 12.0), egui::Align2::LEFT_CENTER, format!("{arrow} {name} ({})", end - first), egui::FontId::proportional(12.0), c_ink2()); }
                    }
                    if hresp.double_clicked() { app.renaming_section = Some((*first, name.clone())); }
                    else if hresp.clicked() { if collapsed { app.collapsed_sections.retain(|x| x != first); } else { app.collapsed_sections.push(*first); } }
                }
                if collapsed && i != app.current { continue; }
                let (row, resp) = ui.allocate_exact_size(vec2(THUMB_W + 34.0, th + 14.0), Sense::click_and_drag());
                let rtl = crate::i18n::rtl();
                let thumb = Rect::from_min_size(pos2(if rtl { row.min.x + 2.0 } else { row.max.x - THUMB_W - 2.0 }, row.min.y + 4.0), vec2(THUMB_W, th));
                // Re-render a thumbnail when the presentation changed (a couple per frame).
                let stale = app.thumbs.get(part).is_none_or(|(rev, _)| *rev != app.revision);
                if stale && rendered < 2 && ui.is_rect_visible(row) {
                    let scene = app.pres.scene(part);
                    if let Some(pm) = render::render(&app.pres, &scene, &mut app.images, Options { scale: (THUMB_W * ppp) as f64 / sw, prompts: false, skip_text_of: None, translate: None }) {
                        let img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
                        let tex = ui.ctx().load_texture(format!("thumb-{part}"), img, egui::TextureOptions::LINEAR);
                        app.thumbs.insert(part.clone(), (app.revision, tex));
                    }
                    rendered += 1;
                }
                if rendered >= 2 && stale { ui.ctx().request_repaint(); }
                let selected = i == app.current || app.selected_slides.contains(&i);
                let p = ui.painter();
                p.rect_filled(thumb, 0.0, Color32::WHITE);
                if let Some((_, t)) = app.thumbs.get(part) { p.image(t.id(), thumb, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), if app.pres.hidden(part) { Color32::from_white_alpha(120) } else { Color32::WHITE }); }
                let border = if i == app.current { Stroke::new(2.5, ACCENT) } else if selected { Stroke::new(2.0, ACCENT.gamma_multiply(0.6)) } else if resp.hovered() { Stroke::new(1.5, c_ink3()) } else { Stroke::new(1.0, c_line()) };
                p.rect_stroke(thumb.expand(if i == app.current { 1.5 } else { 0.5 }), 2.0, border, egui::StrokeKind::Outside);
                let num_x = if rtl { row.max.x - 12.0 } else { row.min.x + 12.0 };
                p.text(pos2(num_x, thumb.min.y + 8.0), egui::Align2::CENTER_CENTER, (i + 1).to_string(), egui::FontId::proportional(12.0), if selected { ACCENT } else { c_ink2() });
                if app.pres.hidden(part) {
                    // Hidden slides: the number struck through.
                    p.line_segment([pos2(num_x - 7.0, thumb.min.y + 2.0), pos2(num_x + 7.0, thumb.min.y + 14.0)], Stroke::new(1.0, c_ink2()));
                }
                if resp.clicked() {
                    let (shift, cmd) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
                    if shift { let (a, b) = (app.current.min(i), app.current.max(i)); app.selected_slides = (a..=b).collect(); app.current = i; app.selection.clear(); app.edit = None; }
                    else if cmd { if !app.selected_slides.insert(i) { app.selected_slides.remove(&i); } app.current = i; }
                    else { app.go_to(i); }
                }
                if resp.drag_started() { ui.ctx().set_dragged_id(egui::Id::new("nav-drag")); ui.data_mut(|d| d.insert_temp(egui::Id::new("nav-drag-from"), i)); }
                if dragging && ui.input(|inp| inp.pointer.hover_pos()).is_some_and(|hp| row.contains(hp)) {
                    let before = ui.input(|inp| inp.pointer.hover_pos()).is_some_and(|hp| hp.y < row.center().y);
                    let y = if before { row.min.y } else { row.max.y };
                    ui.painter().line_segment([pos2(thumb.min.x, y), pos2(thumb.max.x, y)], Stroke::new(2.0, ACCENT));
                    drop_target = Some(if before { i } else { i + 1 });
                }
                resp.context_menu(|ui| {
                    if !app.selected_slides.contains(&i) { app.go_to(i); }
                    crate::theme::full_width(ui, |ui| {
                        if qalam_kit::widgets::menu_item(ui, "newSlide", "New Slide") { app.new_slide(None); ui.close(); }
                        if qalam_kit::widgets::menu_item(ui, "duplicate", "Duplicate Slide") { app.duplicate_slides(); ui.close(); }
                        if qalam_kit::widgets::menu_item(ui, "deleteSlide", "Delete Slide") { app.delete_slides(); ui.close(); }
                        ui.separator();
                        crate::theme::submenu(ui, tr("Layout"), |ui| layout_list(app, ui));
                        if qalam_kit::widgets::menu_item(ui, "hideSlide", if app.pres.hidden(part) { "Show Slide" } else { "Hide Slide" }) { app.toggle_hidden(); ui.close(); }
                    });
                });
            }
            // The dragged thumbnail follows the pointer.
            if dragging {
                if let (Some(hp), Some(from)) = (ui.input(|i| i.pointer.hover_pos()), ui.data(|d| d.get_temp::<usize>(egui::Id::new("nav-drag-from")))) {
                    if let Some((_, t)) = slides.get(from).and_then(|p| app.thumbs.get(p)) {
                        let r = Rect::from_center_size(hp + vec2(10.0, 10.0), vec2(THUMB_W * 0.6, th * 0.6));
                        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("nav-ghost")));
                        painter.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(0.85));
                        painter.rect_stroke(r, 2.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Outside);
                    }
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            // Dropping a dragged thumbnail moves the slide(s).
            if dragging && ui.input(|i| i.pointer.any_released()) {
                if let (Some(to), Some(from)) = (drop_target, ui.data(|d| d.get_temp::<usize>(egui::Id::new("nav-drag-from")))) {
                    let to = if to > from { to - 1 } else { to };
                    app.move_slide(from, to);
                }
                ui.ctx().stop_dragging();
            }
            ui.add_space(8.0);
            if ui.add(egui::Button::new(RichText::new(format!("+  {}", tr("New Slide"))).color(c_ink2())).frame(false)).clicked() { app.new_slide(None); }
        });
        // Keys in the navigator act on slides.
        if ui.ui_contains_pointer() && app.edit.is_none() && app.selection.is_empty() && !ui.ctx().memory(|m| m.focused().is_some()) {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::PageDown)) { app.go_to(app.current + 1); }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::PageUp)) { app.go_to(app.current.saturating_sub(1)); }
            if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) { app.delete_slides(); }
        }
    });
}

/// The layouts of the presentation, as a gallery of small previews.
pub fn layout_list(app: &mut SlidesApp, ui: &mut egui::Ui) {
    for l in app.pres.all_layouts() {
        let name = app.pres.layout_name(&l);
        if ui.button(crate::i18n::tr(&name)).clicked() { app.set_layout(&l); ui.close(); }
    }
}

/// A gallery of layout thumbnails; returns the layout clicked.
pub fn layout_gallery(app: &mut SlidesApp, ui: &mut egui::Ui) -> Option<String> {
    let (sw, sh) = app.pres.size();
    let (w, h) = (120.0, 120.0 * (sh / sw) as f32);
    let ppp = ui.ctx().pixels_per_point();
    let mut picked = None;
    let layouts = app.pres.all_layouts();
    ui.set_width(3.0 * (w + 16.0));
    for row in layouts.chunks(3) {
        ui.allocate_ui_with_layout(vec2(3.0 * (w + 16.0), h + 28.0), qalam_kit::widgets::row_layout(), |ui| {
            for l in row {
                let (r, resp) = ui.allocate_exact_size(vec2(w + 12.0, h + 26.0), Sense::click());
                let key = format!("layout-{l}");
                let tex = ui.data(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new(&key)));
                let tex = tex.or_else(|| {
                    let scene = app.pres.scene(l);
                    let pm = render::render(&app.pres, &scene, &mut app.images, Options { scale: (w * ppp) as f64 / sw, prompts: true, skip_text_of: None, translate: Some(crate::i18n::raw) })?;
                    let img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
                    let t = ui.ctx().load_texture(&key, img, egui::TextureOptions::LINEAR);
                    ui.data_mut(|d| d.insert_temp(egui::Id::new(&key), t.clone()));
                    Some(t)
                });
                if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
                let img = Rect::from_min_size(r.min + vec2(6.0, 4.0), vec2(w, h));
                ui.painter().rect_filled(img, 0.0, Color32::WHITE);
                if let Some(t) = tex { ui.painter().image(t.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
                ui.painter().rect_stroke(img, 0.0, Stroke::new(1.0, c_line()), egui::StrokeKind::Outside);
                ui.painter().text(pos2(img.center().x, img.max.y + 11.0), egui::Align2::CENTER_CENTER, crate::i18n::tr(&app.pres.layout_name(l)), egui::FontId::proportional(11.0), c_ink2());
                if resp.clicked() { picked = Some(l.clone()); }
            }
        });
    }
    picked
}
