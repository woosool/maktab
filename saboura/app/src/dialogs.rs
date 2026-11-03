//! Dialogs over the board: the shape library, the command palette, the boards screen and
//! the link editor.

use crate::app::{BoardApp, ImageOptions, Panel, Pending, Tool};
use crate::i18n::tr;
use egui::{Color32, Rect, Sense, Stroke, Ui, vec2};
use qalam_kit::colors::*;
use saboura_engine::geometry;

pub fn show(app: &mut BoardApp, ui: &mut Ui) {
    match app.panel.clone() {
        Panel::Library => library(app, ui),
        Panel::Palette(q) => palette(app, ui, q),
        Panel::Home => home(app, ui),
        Panel::Link(url) => link(app, ui, url),
        Panel::Image(o) => image_options(app, ui, o),
        Panel::Tags => tags(app, ui),
        Panel::Shortcuts => shortcuts(app, ui),
        Panel::About => about(app, ui),
        Panel::Recover => recover(app, ui),
        Panel::Templates(q, c) => crate::library::template_dialog(app, ui, q, c),
        Panel::Icons(q, c, k) => crate::library::icon_dialog(app, ui, q, c, k),
        _ => {}
    }
}

/// A shape drawn as a preview in `r`.
fn preview(ui: &Ui, key: &str, r: Rect) {
    let (w, h) = if matches!(key, "ellipse" | "diamond" | "star" | "triangle" | "hexagon" | "or" | "summing-junction") { (r.height() as f64, r.height() as f64) } else { (r.width() as f64, r.height() as f64 * 0.7) };
    let o = r.center() - vec2(w as f32 / 2.0, h as f32 / 2.0);
    let to = |p: (f64, f64)| o + vec2(p.0 as f32, p.1 as f32);
    let pts: Vec<_> = geometry::shape_outline(key, w, h).into_iter().map(to).collect();
    crate::canvas::fill_poly(ui.painter(), &pts, Color32::from_rgb(0xdc, 0xe5, 0xf7));
    ui.painter().add(egui::Shape::closed_line(pts, Stroke::new(1.2, crate::canvas::SHAPE_LINE)));
    for d in geometry::shape_details(key, w, h) { ui.painter().add(egui::Shape::line(d.into_iter().map(to).collect(), Stroke::new(1.2, crate::canvas::SHAPE_LINE))); }
}

fn library(app: &mut BoardApp, ui: &mut Ui) {
    let mut open = true;
    crate::theme::dialog(ui.ctx(), tr("Shape library")).open(&mut open).default_width(520.0).show(ui.ctx(), |ui| {
        ui.label(tr("Choose a shape, then click or drag on the board."));
        crate::toolbar::Grid::new("library").spacing(vec2(8.0, 8.0)).show(ui, |ui| {
            for (n, (key, name)) in geometry::SHAPES.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(110.0, 84.0), Sense::click());
                if resp.hovered() || app.shape == *key { ui.painter().rect_filled(r, 8.0, if app.shape == *key { ACCENT.gamma_multiply(0.15) } else { c_hover() }); }
                preview(ui, key, Rect::from_min_size(r.min + vec2(25.0, 8.0), vec2(60.0, 46.0)));
                ui.painter().text(r.center_bottom() - vec2(0.0, 6.0), egui::Align2::CENTER_BOTTOM, tr(name), egui::FontId::proportional(11.5), c_ink2());
                if resp.clicked() { app.tool = Tool::Shape; app.shape = key; app.panel = Panel::None; }
                if n % 4 == 3 { ui.end_row(); }
            }
        });
    });
    if !open { app.panel = Panel::None; }
}

fn palette(app: &mut BoardApp, ui: &mut Ui, mut query: String) {
    let mut run = None;
    let mut close = false;
    egui::Area::new(egui::Id::unique("palette")).anchor(egui::Align2::CENTER_TOP, [0.0, 90.0]).order(egui::Order::Tooltip).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).inner_margin(10).corner_radius(10).show(ui, |ui| {
            ui.set_width(440.0);
            let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text(tr("Search commands")).desired_width(f32::INFINITY));
            r.request_focus();
            let q = query.to_lowercase();
            let all: Vec<&crate::commands::Entry> = crate::commands::MENUS.iter().flat_map(|m| m.1.iter()).filter(|e| e.0 != "-" && crate::commands::enabled(app, e.0)).collect();
            let found: Vec<&crate::commands::Entry> = all.into_iter().filter(|c| q.is_empty() || tr(c.1).to_lowercase().contains(&q) || c.1.to_lowercase().contains(&q)).collect();
            ui.separator();
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (n, c) in found.iter().enumerate() {
                    let resp = crate::rtl::row(ui, |ui| {
                        let b = ui.add(egui::Button::new(tr(c.1)).selected(n == 0).frame_when_inactive(false).min_size(vec2(330.0, 26.0)));
                        ui.weak(crate::commands::shortcut_text(c.2));
                        b
                    }).inner;
                    if resp.clicked() { run = Some(c.0); }
                }
            });
            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if enter && let Some(c) = found.first() { run = Some(c.0); }
            if esc || r.lost_focus() && !enter && ui.input(|i| i.pointer.any_pressed()) { close = true; }
        });
    });
    app.panel = if run.is_some() || close { Panel::None } else { Panel::Palette(query) };
    if let Some(id) = run { crate::commands::run(app, id); }
}

fn home(app: &mut BoardApp, ui: &mut Ui) {
    let mut open = true;
    let mut action = None;
    crate::theme::dialog(ui.ctx(), tr("Boards")).open(&mut open).default_width(460.0).show(ui.ctx(), |ui| {
        crate::theme::section(ui, &tr("Start a board"));
        crate::rtl::row(ui, |ui| {
            crate::rtl::wrap(ui, |ui| {
                for (key, name) in saboura_engine::model::TEMPLATES {
                    if ui.add_sized([130.0, 60.0], egui::Button::new(tr(name))).clicked() { action = Some(Pending::New(key)); }
                }
            });
        });
        crate::rtl::row(ui, |ui| {
            if ui.button(tr("More templates…")).clicked() { app.panel = Panel::Templates(String::new(), String::new()); }
            if ui.button(tr("Open a board file…")).clicked() { action = Some(Pending::OpenDialog); }
        });
        if !app.prefs.recent.is_empty() {
            crate::theme::section(ui, &tr("Recent boards"));
            for p in app.prefs.recent.clone() {
                let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let current = app.path.as_ref() == Some(&p);
                if ui.add(egui::Button::new(name).selected(current).frame_when_inactive(false).min_size(vec2(420.0, 26.0))).on_hover_text(p.display().to_string()).clicked() && !current { action = Some(Pending::Open(p)); }
            }
        }
    });
    if !open && app.panel == Panel::Home { app.panel = Panel::None; }
    if let Some(a) = action { app.panel = Panel::None; app.guard(a); }
}

fn link(app: &mut BoardApp, ui: &mut Ui, mut url: String) {
    let mut open = true;
    let mut done = None;
    crate::theme::dialog(ui.ctx(), tr("Link")).open(&mut open).default_width(380.0).show(ui.ctx(), |ui| {
        ui.label(tr("Web address (Ctrl/Cmd-click the object to open it)"));
        let r = ui.add(egui::TextEdit::singleline(&mut url).hint_text("https://").desired_width(f32::INFINITY));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, tr("Apply")).clicked() || enter { done = Some(Some(url.trim().to_string())); }
            if crate::theme::secondary(ui, &tr("Remove link")).clicked() { done = Some(None); }
        });
    });
    match done {
        Some(v) => {
            let v = v.filter(|u| !u.is_empty()).map(|u| if u.contains("://") || u.starts_with("mailto:") { u } else { format!("https://{u}") });
            crate::canvas::set_text_style(app, "link", v.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null));
            app.panel = Panel::None;
        }
        None => app.panel = if open { Panel::Link(url) } else { Panel::None },
    }
}

fn icon_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    static ICON: std::sync::OnceLock<Option<egui::TextureHandle>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| image::load_from_memory(include_bytes!("../../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        ctx.load_texture("app-icon", egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw()), egui::TextureOptions::LINEAR)
    })).clone()
}

/// The splash card at start (the icon, name and maker), then the window grows into the
/// main window. Returns true while it shows.
pub fn splash(app: &mut BoardApp, ui: &mut Ui) -> bool {
    let now = ui.input(|i| i.time);
    let Some(start) = &mut app.splash else { return false };
    if *start == 0.0 { *start = now.max(1e-6); }
    let elapsed = now - *start;
    if elapsed > 1.4 {
        app.splash = None;
        let ctx = ui.ctx();
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(480.0, 360.0)));
        let size = vec2(1280.0, 820.0);
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        if let Some(m) = ctx.input(|i| i.viewport().monitor_size) {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(((m.x - size.x) / 2.0).max(0.0), ((m.y - size.y) / 2.0).max(24.0))));
        }
        return false;
    }
    let icon = icon_texture(ui.ctx());
    let dark = app.prefs.dark;
    let (bg, ink, sub) = if dark { (Color32::from_gray(0x1C), Color32::from_gray(0xF2), Color32::from_gray(0xA8)) } else { (Color32::WHITE, Color32::from_rgb(0x0b, 0x4d, 0x47), Color32::from_gray(0x70)) };
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let rect = ui.max_rect().shrink(10.0);
        ui.painter().add(egui::epaint::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(rect, 16.0));
        ui.painter().rect(rect, 16.0, bg, Stroke::new(1.0, sub.gamma_multiply(0.25)), egui::StrokeKind::Inside);
        if ui.interact(rect, egui::Id::unique("splash-drag"), Sense::drag()).drag_started() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag); }
        let a = (elapsed / 0.4).clamp(0.0, 1.0) as f32;
        let c = rect.center() - vec2(0.0, 44.0);
        if let Some(t) = icon { ui.painter().image(t.id(), Rect::from_center_size(c - vec2(0.0, 10.0), vec2(120.0, 120.0)), Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(a)); }
        ui.painter().text(c + vec2(0.0, 70.0), egui::Align2::CENTER_CENTER, format!("Saboura  {}", crate::i18n::shape("سبورة")), egui::FontId::new(30.0, egui::FontFamily::Name("ui-bold".into())), ink.gamma_multiply(a));
        ui.painter().text(c + vec2(0.0, 104.0), egui::Align2::CENTER_CENTER, tr("Starting…"), egui::FontId::proportional(14.0), sub.gamma_multiply(a));
        let w = 160.0;
        let bar = Rect::from_center_size(c + vec2(0.0, 128.0), vec2(w, 3.0));
        ui.painter().rect_filled(bar, 1.5, sub.gamma_multiply(0.25 * a));
        let t = ((elapsed * 0.8) % 1.0) as f32;
        ui.painter().rect_filled(Rect::from_min_size(egui::pos2(bar.min.x + (w - 50.0) * t, bar.min.y), vec2(50.0, 3.0)), 1.5, Color32::from_rgb(0x17, 0x95, 0x8a).gamma_multiply(a));
        ui.painter().text(egui::pos2(rect.center().x, rect.max.y - 22.0), egui::Align2::CENTER_CENTER, format!("{} Woosool · woosool.com", tr("Made by")), egui::FontId::proportional(12.0), sub.gamma_multiply(a));
    });
    ui.ctx().request_repaint();
    true
}

fn about(app: &mut BoardApp, ui: &mut Ui) {
    let mut open = true;
    crate::theme::dialog(ui.ctx(), crate::i18n::trf("About {}", &[&crate::APP_NAME])).open(&mut open).resizable(false).default_width(420.0).show(ui.ctx(), |ui| {
        crate::rtl::row(ui, |ui| {
            if let Some(t) = icon_texture(ui.ctx()) { ui.add(egui::Image::new((t.id(), vec2(72.0, 72.0)))); }
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(format!("{}  {}", crate::APP_NAME, crate::i18n::shape("سبورة"))).size(22.0).family(egui::FontFamily::Name("ui-bold".into())));
                ui.label(crate::i18n::trf("Version {}", &[&env!("CARGO_PKG_VERSION")]));
                ui.label(tr("A free desktop whiteboard for sticky notes, diagrams, mind maps and planning, offline."));
            });
        });
        ui.add_space(8.0);
        crate::rtl::row(ui, |ui| { ui.label(tr("Made by")); ui.hyperlink_to("Woosool", "https://woosool.com"); });
        ui.hyperlink_to(tr("Saboura on woosool.com"), "https://woosool.com/en/products/saboura");
        if crate::theme::secondary(ui, &tr("Check for Updates…")).clicked() { crate::update::check(ui.ctx(), true, None); }
        ui.add_space(6.0);
        ui.label(egui::RichText::new(tr("Released under the MIT License.")).strong());
        ui.label(egui::RichText::new("Copyright © 2026 Woosool").small());
        egui::CollapsingHeader::new(tr("License")).show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| ui.label(egui::RichText::new(include_str!("../../LICENSE")).small().monospace()));
        });
        ui.add_space(6.0);
        ui.weak(tr("Fonts: Noto Sans (OFL), Liberation (OFL)."));
    });
    if !open { app.panel = Panel::None; }
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("V", "Select"), ("H / Space", "Hand (move the board)"), ("N", "Sticky note"), ("T", "Text"), ("R", "Rectangle"), ("O", "Ellipse"),
    ("S", "Shape library"), ("L", "Connection line"), ("P", "Pen"), ("F", "Frame"), ("C", "Comment"), ("M", "Minimap"),
    ("Tab", "Add a branch (mind map) or a joined copy"), ("Shift Enter", "Add a sibling branch"), ("Enter", "Edit the selected object's text"),
    ("Arrows / Shift Arrows", "Nudge by 1 / 10"), ("Shift drag", "Keep proportions, snap rotation to 15°"), ("Alt drag", "Move without snapping"),
    ("Ctrl/Cmd click", "Open an object's link"), ("Ctrl/Cmd F", "Find on board"), ("Ctrl/Cmd K", "Command palette"),
    ("Ctrl/Cmd B / I / U", "Bold, italic, underline"), ("Shift 1", "Fit the board"), ("Ctrl/Cmd 0", "Zoom to 100%"),
];

fn shortcuts(app: &mut BoardApp, ui: &mut Ui) {
    let mut open = true;
    crate::theme::dialog(ui.ctx(), tr("Keyboard Shortcuts")).open(&mut open).default_width(460.0).show(ui.ctx(), |ui| {
        egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
            crate::toolbar::Grid::new("shortcuts").striped(true).spacing(vec2(20.0, 6.0)).show(ui, |ui| {
                for (k, what) in SHORTCUTS { ui.label(egui::RichText::new(*k).strong()); ui.label(tr(what)); ui.end_row(); }
                for (_, entries) in crate::commands::MENUS {
                    for (id, name, accel) in entries.iter() {
                        if *id == "-" || accel.is_empty() { continue; }
                        ui.label(egui::RichText::new(crate::commands::shortcut_text(accel)).strong()); ui.label(tr(name)); ui.end_row();
                    }
                }
            });
        });
    });
    if !open { app.panel = Panel::None; }
}

/// An image's alternative text, crop and shape; replacing the picture.
fn image_options(app: &mut BoardApp, ui: &mut Ui, mut o: ImageOptions) {
    let mut open = true;
    let mut done = None;
    crate::theme::dialog(ui.ctx(), tr("Image")).open(&mut open).default_width(380.0).show(ui.ctx(), |ui| {
        ui.label(tr("Alternative text"));
        ui.add(egui::TextEdit::multiline(&mut o.alt).desired_rows(2).desired_width(f32::INFINITY).hint_text(tr("Describe the picture for people who can't see it")).char_limit(1000));
        crate::theme::section(ui, &tr("Crop"));
        let [x, y, w, h] = &mut o.crop;
        let (mut l, mut t, mut r, mut b) = (*x * 100.0, *y * 100.0, (1.0 - *x - *w) * 100.0, (1.0 - *y - *h) * 100.0);
        for (v, name) in [(&mut l, "Left"), (&mut t, "Top"), (&mut r, "Right"), (&mut b, "Bottom")] {
            ui.add(egui::Slider::new(v, 0.0..=90.0).suffix("%").text(tr(name)));
        }
        let (l, t) = (l.min(99.0 - r), t.min(99.0 - b));
        *x = l / 100.0; *y = t / 100.0; *w = ((100.0 - l - r) / 100.0).max(0.01); *h = ((100.0 - t - b) / 100.0).max(0.01);
        ui.checkbox(&mut o.ellipse, tr("Crop to a circle"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, tr("Apply")).clicked() { done = Some(false); }
            if crate::theme::secondary(ui, &tr("Replace picture…")).clicked() { done = Some(true); }
        });
    });
    match done {
        Some(false) => {
            let before = app.single().map(ImageOptions::of);
            let o2 = o.clone();
            app.edit_selected(move |i| {
                i.set("alt", if o2.alt.trim().is_empty() { serde_json::Value::Null } else { o2.alt.clone().into() });
                // The object keeps its scale: it grows or shrinks with the part shown.
                if let Some(b) = &before {
                    let (fw, fh) = (i.w / b.crop[2], i.h / b.crop[3]);
                    i.w = fw * o2.crop[2]; i.h = fh * o2.crop[3];
                }
                let full = o2.crop == [0.0, 0.0, 1.0, 1.0] && !o2.ellipse;
                let mut c = serde_json::json!({ "x": o2.crop[0], "y": o2.crop[1], "w": o2.crop[2], "h": o2.crop[3] });
                if o2.ellipse { c["mask"] = "ellipse".into(); }
                i.set("crop", if full { serde_json::Value::Null } else { c });
            });
            app.panel = Panel::None;
        }
        Some(true) => {
            app.panel = Panel::None;
            if let Some(path) = rfd::FileDialog::new().add_filter(tr("Images"), &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file()
                && let Ok(bytes) = std::fs::read(path) { replace_image(app, &bytes); }
        }
        None => app.panel = if open { Panel::Image(o) } else { Panel::None },
    }
}

/// Puts another picture in the selected image, keeping its place and width.
fn replace_image(app: &mut BoardApp, bytes: &[u8]) {
    match BoardApp::encode_image(bytes) {
        Ok((src, w, h)) => app.edit_selected(|i| { i.h = i.w * h / w; i.set("src", src.clone()); i.extra.remove("crop"); }),
        Err(e) => app.notify(&e),
    }
}

/// Board tags (name and colour) and which the selected objects carry.
fn tags(app: &mut BoardApp, ui: &mut Ui) {
    let mut open = true;
    let mut list: Vec<serde_json::Value> = app.board.extra.get("tags").and_then(|t| t.as_array()).cloned().unwrap_or_default();
    let mut changed = false;
    let mut toggle: Option<(String, bool)> = None;
    let id = egui::Id::unique("new-tag");
    let mut name: String = ui.ctx().data_mut(|d| d.get_temp(id)).unwrap_or_default();
    crate::theme::dialog(ui.ctx(), tr("Tags")).open(&mut open).default_width(340.0).show(ui.ctx(), |ui| {
        let sel: Vec<_> = app.selected_items().into_iter().cloned().collect();
        if sel.is_empty() { ui.weak(tr("Select objects to tag them.")); }
        let mut remove = None;
        for (n, t) in list.iter().enumerate() {
            let tid = t["id"].as_str().unwrap_or_default().to_string();
            crate::theme::hrow(ui, |ui| {
                let c = crate::canvas::hex(t["color"].as_str(), Color32::GRAY);
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 6.0, c);
                let has = !sel.is_empty() && sel.iter().all(|i| i.get("tagIds").and_then(|v| v.as_array()).is_some_and(|a| a.iter().any(|v| v.as_str() == Some(&tid))));
                let mut on = has;
                if ui.add_enabled(!sel.is_empty(), egui::Checkbox::new(&mut on, t["name"].as_str().unwrap_or(""))).changed() { toggle = Some((tid.clone(), on)); }
                ui.with_layout(qalam_kit::widgets::far_layout(), |ui| { if ui.small_button("✕").on_hover_text(tr("Delete tag")).clicked() { remove = Some(n); } });
            });
        }
        if let Some(n) = remove {
            let tid = list.remove(n)["id"].as_str().unwrap_or_default().to_string();
            for i in app.board.items.iter_mut() {
                if let Some(a) = i.extra.get_mut("tagIds").and_then(|v| v.as_array_mut()) { a.retain(|v| v.as_str() != Some(&tid)); if a.is_empty() { i.extra.remove("tagIds"); } }
            }
            changed = true;
        }
        ui.separator();
        crate::rtl::row(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut name).hint_text(tr("New tag")).desired_width(200.0).char_limit(64));
            if ui.button(tr("Add")).clicked() && !name.trim().is_empty() {
                const TAG_COLORS: [&str; 6] = ["#2f6fd6", "#26894a", "#c86b0a", "#8a5bd6", "#d03a3a", "#0b7d74"];
                list.push(serde_json::json!({ "id": format!("tag-{}", &saboura_engine::model::uid()[..8]), "name": name.trim(), "color": TAG_COLORS[list.len() % 6] }));
                name.clear();
                changed = true;
            }
        });
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, name));
    if changed || toggle.is_some() {
        app.checkpoint();
        if list.is_empty() { app.board.extra.remove("tags"); } else { app.board.extra.insert("tags".into(), serde_json::Value::Array(list)); }
        if let Some((tid, on)) = toggle {
            let sel = app.selection.clone();
            for i in app.board.items.iter_mut().filter(|i| sel.contains(&i.id)) {
                let mut ids: Vec<String> = i.get("tagIds").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
                ids.retain(|t| *t != tid);
                if on { ids.push(tid.clone()); }
                i.set("tagIds", if ids.is_empty() { serde_json::Value::Null } else { serde_json::to_value(ids).unwrap_or_default() });
            }
        }
        app.changed();
    }
    if !open { app.panel = Panel::None; }
}

/// Offers the board that was open, unsaved, when the app last closed.
fn recover(app: &mut BoardApp, ui: &mut Ui) {
    let Some(board) = app.recovered.clone() else { app.panel = Panel::None; return };
    let mut choice = None;
    crate::theme::dialog(ui.ctx(), tr("Recover unsaved changes")).default_width(380.0).show(ui.ctx(), |ui| {
        ui.label(crate::i18n::trf("“{}” had changes that weren't saved when Saboura closed.", &[&board.title]));
        ui.weak(crate::i18n::trf("{} objects", &[&board.items.len()]));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, tr("Restore")).clicked() { choice = Some(true); }
            if crate::theme::secondary(ui, &tr("Discard")).clicked() { choice = Some(false); }
        });
    });
    if let Some(restore) = choice {
        app.recovered = None;
        app.panel = Panel::None;
        let _ = std::fs::remove_file(crate::app::recovery_file());
        if restore {
            let mut b = board;
            b.update_connectors();
            app.board = b;
            app.path = None;
            app.dirty = true;
            app.fit_pending = true;
        }
    }
}
