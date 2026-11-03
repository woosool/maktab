//! Ribbon building blocks shared by the Qalam apps: icon buttons, groups, menus, and the
//! color picker.

use crate::colors::*;
use crate::icon_data::{DEFS, ICONS};
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextureHandle, Ui};
use std::cell::RefCell;
use std::collections::HashMap;
use word_engine::Color;

thread_local! {
    static TEXTURES: RefCell<HashMap<(&'static str, u32, u32), Option<TextureHandle>>> = RefCell::new(HashMap::new());
}

/// A command icon as a texture of `size` logical px (16 or 32), rendered for the current scale.
pub fn icon_texture(ctx: &egui::Context, name: &str, size: u32) -> Option<TextureHandle> {
    // Qalam's command icons first, then the slides editor's (new slide, layouts, transitions…).
    let (key, small, large, defs) = match ICONS.binary_search_by_key(&name, |e| e.0) {
        Ok(i) => (ICONS[i].0, ICONS[i].1, ICONS[i].2, DEFS),
        Err(_) => match crate::slide_icon_data::ICONS.binary_search_by_key(&name, |e| e.0) {
            Ok(i) => { let e = crate::slide_icon_data::ICONS[i]; (e.0, e.1, e.2, crate::slide_icon_data::DEFS) }
            // Then Jadwal's spreadsheet icons (autoSum, conditionalFormatting, charts…).
            Err(_) => { let i = crate::sheet_icon_data::ICONS.binary_search_by_key(&name, |e| e.0).ok()?; let e = crate::sheet_icon_data::ICONS[i]; (e.0, e.1, e.2, crate::sheet_icon_data::DEFS) }
        },
    };
    let px = (size as f32 * ctx.pixels_per_point()).round().max(1.0) as u32;
    let dark = dark();
    TEXTURES.with(|t| {
        t.borrow_mut().entry((key, size, px + if dark { 1 << 20 } else { 0 })).or_insert_with(|| {
            let body = if size <= 20 { small } else { large };
            // Dark Mode lightens the icons' gray outlines so they read on dark chrome.
            let body = if dark { body.replace("#636366", "#B8B8BE").replace("#2c2c2e", "#E5E5EA").replace("#1d1d1f", "#E5E5EA").replace("#3a3a3c", "#D1D1D6").replace("#48484a", "#C7C7CC") } else { body.to_string() };
            let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="{px}" height="{px}" color="{}"><defs>{defs}</defs>{body}</svg>"##, if dark { "#E5E5EA" } else { "#2c2c2e" });
            let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
            resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
            let image = egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data());
            Some(ctx.load_texture(format!("icon-{key}-{px}-{dark}"), image, egui::TextureOptions::LINEAR))
        }).clone()
    })
}

pub fn paint_icon(ui: &Ui, name: &str, rect: Rect, size: u32, enabled: bool) {
    if let Some(t) = icon_texture(ui.ctx(), name, size) {
        let tint = if enabled { Color32::WHITE } else { Color32::from_white_alpha(90) };
        ui.painter().image(t.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
    }
}

pub fn button_bg(ui: &Ui, rect: Rect, r: &Response, on: bool) {
    let fill = if on || r.is_pointer_button_down_on() { c_press() } else if r.hovered() { c_hover() } else { return };
    ui.painter().rect_filled(rect, crate::theme::RADIUS as f32, fill);
}

thread_local! {
    /// Set while drawing a ribbon tab that doesn't fit: small buttons drop their labels and
    /// large ones wrap them narrower (the labels stay as tooltips).
    pub static COMPACT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Groups of the current tab hidden in Options ▸ Customize Ribbon.
    pub static HIDDEN_GROUPS: RefCell<std::collections::HashSet<String>> = RefCell::new(Default::default());
    /// Draw each group's name under it (Excel's ribbon); off for Qalam and Sard.
    pub static CAPTIONS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Compact drawing applies to the ribbon itself, not to menus opened from it.
pub fn compact(ui: &Ui) -> bool { COMPACT.with(|c| c.get()) && ui.layer_id().order == egui::Order::Background }

/// A small command: 16px icon in a 24px button, optionally with a label.
pub fn small(ui: &mut Ui, icon: &str, label: Option<&str>, on: bool, enabled: bool, tip: &str) -> Response {
    let (label_t, tip_t) = (label.map(crate::i18n::tr), crate::i18n::tr(tip));
    let (label, tip) = (label_t.as_deref(), tip_t.as_str());
    let font = FontId::proportional(12.0);
    let tip_owned;
    let (label, tip) = if compact(ui) && label.is_some() { tip_owned = if tip.is_empty() || tip == label.unwrap_or_default() { label.unwrap_or_default().to_string() } else { format!("{}: {tip}", label.unwrap_or_default()) }; (None, tip_owned.as_str()) } else { (label, tip) };
    let text_w = label.map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), c_ink2()).size().x + 6.0).unwrap_or(0.0);
    let in_menu = crate::theme::IN_MENU.with(|m| m.get()) && label.is_some();
    let w = if in_menu { (26.0 + text_w).max(ui.available_width()) } else { 26.0 + text_w };
    let (rect, r) = ui.allocate_exact_size(vec2(w, if in_menu { 28.0 } else { 24.0 }), if enabled { Sense::click() } else { Sense::hover() });
    if enabled { button_bg(ui, rect, &r, on); }
    // In Arabic, menu rows mirror: icon at the right, text right-aligned beside it.
    let mirror = in_menu && crate::i18n::rtl();
    let icon_x = if mirror { rect.max.x - 21.0 } else { rect.min.x + 5.0 };
    paint_icon(ui, icon, Rect::from_min_size(pos2(icon_x, rect.center().y - 8.0), vec2(16.0, 16.0)), 16, enabled);
    if let Some(l) = label {
        let (at, align) = if mirror { (pos2(rect.max.x - 26.0, rect.center().y), Align2::RIGHT_CENTER) } else { (pos2(rect.min.x + 26.0, rect.center().y), Align2::LEFT_CENTER) };
        ui.painter().text(at, align, l, font, if enabled { c_ink2() } else { c_ink3() });
    }
    let name = label.map(String::from).unwrap_or_else(|| tip.to_string());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.trim()));
    if in_menu && (tip.is_empty() || Some(tip) == label) { r } else { r.on_hover_text(crate::i18n::tr(&tip)) }
}

/// A plain × button for closing panes.
pub fn close_button(ui: &mut Ui, tip: &str) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    button_bg(ui, rect, &r, false);
    let c = rect.center();
    let s = Stroke::new(1.3, c_ink2());
    ui.painter().line_segment([c + vec2(-4.5, -4.5), c + vec2(4.5, 4.5)], s);
    ui.painter().line_segment([c + vec2(4.5, -4.5), c + vec2(-4.5, 4.5)], s);
    r.on_hover_text(crate::i18n::tr(&tip))
}

/// A large command: 32px icon over a one- or two-line label.
pub fn large(ui: &mut Ui, icon: &str, label: &str, on: bool, enabled: bool, tip: &str) -> Response {
    // A label may carry a line break where Excel breaks it ("Insert\nFunction"): it's translated
    // and named as one line, and the break is kept only when the text is shown untranslated.
    let plain = label.replace('\n', " ");
    let name_t = crate::i18n::tr(&plain);
    let label_t = if name_t == plain && label.contains('\n') { label.to_string() } else { name_t.clone() };
    let tip_t = crate::i18n::tr(tip);
    let (label, tip) = (label_t.as_str(), tip_t.as_str());
    let font = FontId::proportional(11.5);
    // One line where it fits; long labels wrap onto a second line within the button.
    // Compact labels wrap narrower, but only between words.
    let (font, wrap) = if compact(ui) {
        let f = FontId::proportional(10.5);
        let longest = label.split_whitespace().map(|w| ui.painter().layout_no_wrap(w.to_string(), f.clone(), c_ink2()).size().x).fold(0.0, f32::max);
        // Short labels stay on one line; long ones break between words.
        let full = ui.painter().layout_no_wrap(label.to_string(), f.clone(), c_ink2()).size().x;
        (f, if full <= 64.0 { full + 1.0 } else { longest.max(40.0) + 1.0 })
    } else { (font, 84.0) };
    let galley = ui.painter().layout(label.to_string(), font, if enabled { c_ink2() } else { c_ink3() }, wrap);
    let w = galley.size().x.max(36.0) + 10.0;
    let (rect, r) = ui.allocate_exact_size(vec2(w, 70.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled { button_bg(ui, rect, &r, on); }
    paint_icon(ui, icon, Rect::from_center_size(pos2(rect.center().x, rect.min.y + 20.0), vec2(32.0, 32.0)), 32, enabled);
    let gpos = pos2(rect.center().x - galley.size().x / 2.0, rect.min.y + 39.0);
    ui.painter().galley(gpos, galley, c_ink2());
    let name = name_t.trim().to_string();
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
    r.on_hover_text(crate::i18n::tr(&tip))
}

pub fn caret(ui: &Ui, center: egui::Pos2) {
    let pts = vec![center + vec2(-3.5, -1.5), center + vec2(3.5, -1.5), center + vec2(0.0, 2.5)];
    ui.painter().add(egui::Shape::convex_polygon(pts, c_ink3(), Stroke::NONE));
}

/// A group of commands with its name underneath (Word's group label).
pub fn group(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) { group_launcher(ui, label, add, None); }

/// A group with a dialog launcher (↘) at its label's right end; returns whether it was clicked.
pub fn group_l(ui: &mut Ui, label: &str, tip: &str, add: impl FnOnce(&mut Ui)) -> bool { group_launcher(ui, label, add, Some(tip)) }

pub fn group_launcher(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui), launcher: Option<&str>) -> bool {
    // Customize Ribbon can hide a group.
    if HIDDEN_GROUPS.with(|h| h.borrow().contains(label)) { return false; }
    let top = ui.cursor().min.y;
    let r = ui.allocate_ui_with_layout(vec2(2000.0, GROUP_H), row_layout(), |ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.set_min_height(GROUP_H);
        ui.set_max_height(GROUP_H);
        add(ui);
    });
    let rect = r.response.rect;
    // Qalam's groups have no captions (the label is kept for the launcher's id); an app can turn on
    // Excel-style captions under each group with `CAPTIONS`. A thin rule separates groups.
    let captions = CAPTIONS.with(|c| c.get()) && !compact(ui);
    if captions {
        let g = ui.painter().layout_no_wrap(crate::i18n::tr(label), FontId::proportional(11.0), c_ink3());
        let at = pos2(rect.center().x - g.size().x / 2.0, top + GROUP_H + 15.0);
        ui.painter().galley(at, g, c_ink3());
    }
    let launch_y = if captions { top + GROUP_H + 21.0 } else { top + GROUP_H - 4.0 };
    let mut clicked = false;
    if let Some(tip) = launcher {
        let b = Rect::from_center_size(pos2(rect.max.x + 2.0, launch_y), vec2(12.0, 12.0));
        let resp = ui.interact(b, ui.make_persistent_id(("launcher", label)), Sense::click());
        if resp.hovered() { ui.painter().rect_filled(b, crate::theme::RADIUS as f32, c_hover()); }
        let s = Stroke::new(1.0, c_ink3());
        let (a, c) = (b.shrink(2.5).left_top(), b.shrink(2.5).right_bottom());
        ui.painter().line_segment([pos2(a.x, a.y), pos2(c.x, a.y)], s);
        ui.painter().line_segment([pos2(a.x, a.y), pos2(a.x, c.y)], s);
        ui.painter().line_segment([pos2(a.x + 2.0, a.y + 2.0), c], s);
        ui.painter().line_segment([c, c - vec2(3.0, 0.0)], s);
        ui.painter().line_segment([c, c - vec2(0.0, 3.0)], s);
        clicked = resp.on_hover_text(crate::i18n::tr(&tip)).clicked();
    }
    ui.add_space(6.0);
    let x = ui.cursor().min.x;
    ui.painter().line_segment([pos2(x, top + 8.0), pos2(x, top + GROUP_H - 4.0 + if captions { CAPTION_H } else { 0.0 })], Stroke::new(1.0, c_line()));
    ui.add_space(6.0);
    clicked
}

/// A large button with a drop-down arrow under its label; returns (main, arrow).
pub fn large_split(ui: &mut Ui, icon: &str, label: &str, tip: &str) -> (Response, Response) {
    let main = large(ui, icon, label, false, true, tip);
    let arrow_rect = Rect::from_min_size(pos2(main.rect.min.x, main.rect.max.y - 12.0), vec2(main.rect.width(), 12.0));
    let arrow = ui.interact(arrow_rect, ui.make_persistent_id(("split", label)), Sense::click());
    caret(ui, arrow_rect.center() + vec2(0.0, 1.0));
    (main, arrow)
}

/// A small icon button with a separate drop-down arrow.
pub fn small_split(ui: &mut Ui, icon: &str, on: bool, tip: &str) -> (Response, Response) { small_split_label(ui, icon, None, on, tip) }

/// A small button (with an optional label) and its drop-down arrow, side by side even in a
/// column of stacked commands.
pub fn small_split_label(ui: &mut Ui, icon: &str, label: Option<&str>, on: bool, tip: &str) -> (Response, Response) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 24.0), row_layout(), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let main = small(ui, icon, label, on, true, tip);
        let (arect, ar) = ui.allocate_exact_size(vec2(12.0, 24.0), Sense::click());
        button_bg(ui, arect, &ar, false);
        caret(ui, arect.center());
        (main, ar)
    }).inner
}

/// Height of a group's commands (three 24px rows), above its label.
pub const GROUP_H: f32 = 76.0;
/// Extra height under groups when captions are on.
pub const CAPTION_H: f32 = 30.0;

/// Two or three rows of small commands, stacked.
/// The interface's reading direction for rows (right to left in Arabic).
pub fn row_layout() -> egui::Layout { if crate::i18n::rtl() { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) } }
/// The opposite end of a row (where Tell Me and quick access sit).
pub fn far_layout() -> egui::Layout { if crate::i18n::rtl() { egui::Layout::left_to_right(egui::Align::Center) } else { egui::Layout::right_to_left(egui::Align::Center) } }

pub fn rows(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let align = if crate::i18n::rtl() { egui::Align::Max } else { egui::Align::Min };
    ui.with_layout(egui::Layout::top_down(align), |ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        add(ui);
    });
}

pub const PALETTE: [[u8; 3]; 40] = [
    [0, 0, 0], [68, 84, 106], [68, 114, 196], [237, 125, 49], [165, 165, 165], [255, 192, 0], [91, 155, 213], [112, 173, 71], [128, 128, 128], [255, 255, 255],
    [64, 64, 64], [214, 220, 228], [217, 226, 243], [251, 228, 213], [237, 237, 237], [255, 242, 204], [222, 234, 246], [226, 239, 217], [242, 242, 242], [191, 191, 191],
    [192, 0, 0], [255, 0, 0], [255, 192, 0], [255, 255, 0], [146, 208, 80], [0, 176, 80], [0, 176, 240], [0, 112, 192], [0, 32, 96], [112, 48, 160],
    [128, 0, 0], [153, 51, 0], [128, 96, 0], [51, 102, 0], [0, 97, 0], [0, 102, 102], [0, 51, 153], [51, 51, 153], [102, 0, 102], [89, 89, 89],
];

/// A color field for dialogs and panes: the current color with a drop-down of the palette,
/// recent colors and the custom picker. Returns true when the color changed.
pub fn color_button(ui: &mut Ui, c: &mut Color) -> bool {
    let (rect, r) = ui.allocate_exact_size(vec2(48.0, 24.0), Sense::click());
    let v = if r.hovered() { c_hover() } else { c_card() };
    ui.painter().rect(rect, 6.0, v, Stroke::new(1.0, c_line()), StrokeKind::Inside);
    let rtl = crate::i18n::rtl();
    let sw = if rtl { Rect::from_min_size(pos2(rect.max.x - 25.0, rect.min.y + 4.0), vec2(21.0, 16.0)) } else { Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(21.0, 16.0)) };
    ui.painter().rect(sw, 4.0, Color32::from_rgb(c.0, c.1, c.2), Stroke::new(1.0, Color32::from_black_alpha(30)), StrokeKind::Inside);
    caret(ui, pos2(if rtl { rect.min.x + 11.0 } else { rect.max.x - 11.0 }, rect.center().y));
    let r = r.on_hover_text(format!("#{}", c.hex()));
    let mut changed = false;
    crate::theme::menu(&r, |ui| if let Some(Some(n)) = color_menu(ui, "") { changed = n != *c; *c = n; });
    changed
}

/// A row of preset swatches followed by a custom color button (dialogs' accent colors).
pub fn swatch_choice(ui: &mut Ui, presets: &[Color], c: &mut Color) -> bool {
    let mut changed = false;
    crate::theme::hrow(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for p in presets {
            if swatch(ui, Color32::from_rgb(p.0, p.1, p.2), 20.0, p == c).clicked() && p != c { *c = *p; changed = true; }
        }
        ui.add_space(4.0);
        changed |= color_button(ui, c);
    });
    changed
}

/// One color swatch: rounded, with a ring on hover and when it is the current color.
pub fn swatch(ui: &mut Ui, c: Color32, size: f32, selected: bool) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let hot = r.hovered() || selected;
    let inner = if r.hovered() { rect } else { rect.shrink(1.0) };
    ui.painter().rect(inner, 5.0, c, Stroke::new(1.0, Color32::from_black_alpha(28)), StrokeKind::Inside);
    if hot { ui.painter().rect_stroke(rect.expand(2.0), 7.0, Stroke::new(1.5, ACCENT), StrokeKind::Inside); }
    r
}

/// A row of swatches with even spacing (a fixed width, so it also works in right-to-left menus).
pub fn swatch_row(ui: &mut Ui, colors: &[Color], picked: &mut Option<Option<Color>>) {
    ui.allocate_ui_with_layout(vec2(10.0 * 24.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for c in colors {
            if swatch(ui, Color32::from_rgb(c.0, c.1, c.2), 20.0, false).on_hover_text(format!("#{}", c.hex())).clicked() { *picked = Some(Some(*c)); ui.close(); }
        }
    });
}

pub fn menu_heading(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(crate::i18n::tr(text)).size(11.5).strong().color(c_ink3()));
}

/// A "no color / automatic" row: a crossed swatch and its label.
pub fn none_row(ui: &mut Ui, label: &str) -> bool {
    let w = ui.available_width().max(240.0).min(260.0);
    let (rect, r) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
    if r.hovered() { ui.painter().rect_filled(rect, 6.0, c_hover()); }
    let rtl = crate::i18n::rtl();
    let sw = if rtl { Rect::from_min_size(pos2(rect.max.x - 26.0, rect.min.y + 5.0), vec2(18.0, 18.0)) } else { Rect::from_min_size(rect.min + vec2(8.0, 5.0), vec2(18.0, 18.0)) };
    ui.painter().rect(sw, 5.0, Color32::WHITE, Stroke::new(1.0, c_line()), StrokeKind::Inside);
    ui.painter().line_segment([sw.left_bottom() + vec2(3.0, -3.0), sw.right_top() + vec2(-3.0, 3.0)], Stroke::new(1.5, Color32::from_rgb(0xD1, 0x34, 0x38)));
    let (pos, align) = if rtl { (pos2(sw.min.x - 10.0, rect.center().y), Align2::RIGHT_CENTER) } else { (pos2(sw.max.x + 10.0, rect.center().y), Align2::LEFT_CENTER) };
    ui.painter().text(pos, align, crate::i18n::tr(label), FontId::proportional(13.0), c_ink());
    r.clicked()
}

/// A color picker flyout under a split button; returns the picked color (None = automatic).
pub fn color_menu(ui: &mut Ui, auto_label: &str) -> Option<Option<Color>> {
    let mut picked = None;
    ui.spacing_mut().item_spacing.y = 4.0;
    if !auto_label.is_empty() && none_row(ui, auto_label) { picked = Some(None); ui.close(); }
    menu_heading(ui, "Theme Colors");
    let all: Vec<Color> = PALETTE.iter().map(|c| Color(c[0], c[1], c[2])).collect();
    for row in all[..all.len() - 20].chunks(10) { swatch_row(ui, row, &mut picked); }
    menu_heading(ui, "Standard Colors");
    for row in all[all.len() - 20..].chunks(10) { swatch_row(ui, row, &mut picked); }
    // Recent custom colors, then a full color picker.
    let recent_id = egui::Id::new("recent-custom-colors");
    let recent: Vec<[u8; 3]> = ui.ctx().data_mut(|d| d.get_persisted::<Vec<[u8; 3]>>(recent_id)).unwrap_or_default();
    if !recent.is_empty() {
        menu_heading(ui, "Recent Colors");
        let r: Vec<Color> = recent.iter().take(10).map(|c| Color(c[0], c[1], c[2])).collect();
        swatch_row(ui, &r, &mut picked);
    }
    ui.separator();
    let pick_id = egui::Id::new("custom-color-value");
    let mut rgb: [u8; 3] = ui.ctx().data_mut(|d| d.get_persisted::<[u8; 3]>(pick_id)).unwrap_or([0x18, 0x5A, 0xBD]);
    crate::theme::submenu(ui, crate::i18n::tr("More Colors…"), |ui| {
        if let Some(c) = custom_picker(ui, &mut rgb) {
            picked = Some(Some(c));
            let mut r = recent.clone();
            r.retain(|x| *x != rgb);
            r.insert(0, rgb);
            r.truncate(10);
            ui.ctx().data_mut(|d| d.insert_persisted(recent_id, r));
            ui.close();
        }
    });
    ui.ctx().data_mut(|d| d.insert_persisted(pick_id, rgb));
    picked
}

pub fn rgb_to_hsv(c: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = c.map(|v| v as f32 / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 { 0.0 } else if max == r { ((g - b) / d).rem_euclid(6.0) } else if max == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 } / 6.0;
    [h, if max == 0.0 { 0.0 } else { d / max }, max]
}

pub fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [u8; 3] {
    let c = egui::ecolor::Hsva::new(h, s, v, 1.0).to_srgb();
    [c[0], c[1], c[2]]
}

/// A clean color picker: a saturation/brightness field, a hue strip, a preview and a hex box.
/// Returns the color when OK is pressed.
pub fn custom_picker(ui: &mut Ui, rgb: &mut [u8; 3]) -> Option<Color> {
    let hsv_id = egui::Id::new("custom-color-hsv");
    // Hue is kept separately so it survives dragging to gray or black.
    let mut hsv: [f32; 3] = ui.ctx().data(|d| d.get_temp::<[f32; 3]>(hsv_id)).filter(|h| hsv_to_rgb(*h) == *rgb).unwrap_or_else(|| rgb_to_hsv(*rgb));
    let w = 232.0;
    ui.set_width(w);
    ui.spacing_mut().item_spacing.y = 8.0;
    ui.label(RichText::new(crate::i18n::tr("Custom color")).size(13.5).strong().color(c_ink()));
    // Saturation (x) by brightness (y) for the current hue.
    let (field, fr) = ui.allocate_exact_size(vec2(w, 150.0), Sense::click_and_drag());
    let mut mesh = egui::Mesh::default();
    let n = 24;
    for yi in 0..=n {
        for xi in 0..=n {
            let (fx, fy) = (xi as f32 / n as f32, yi as f32 / n as f32);
            let c = hsv_to_rgb([hsv[0], fx, 1.0 - fy]);
            mesh.colored_vertex(pos2(field.min.x + fx * w, field.min.y + fy * field.height()), Color32::from_rgb(c[0], c[1], c[2]));
        }
    }
    for yi in 0..n { for xi in 0..n { let a = yi * (n + 1) + xi; mesh.add_triangle(a, a + 1, a + n + 1); mesh.add_triangle(a + 1, a + n + 2, a + n + 1); } }
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().rect_stroke(field, 6.0, Stroke::new(1.0, c_line()), StrokeKind::Inside);
    if let Some(p) = fr.interact_pointer_pos() {
        hsv[1] = ((p.x - field.min.x) / w).clamp(0.0, 1.0);
        hsv[2] = 1.0 - ((p.y - field.min.y) / field.height()).clamp(0.0, 1.0);
    }
    let knob = pos2(field.min.x + hsv[1] * w, field.min.y + (1.0 - hsv[2]) * field.height());
    ui.painter().circle(knob, 7.0, Color32::TRANSPARENT, Stroke::new(2.5, Color32::WHITE));
    ui.painter().circle_stroke(knob, 8.5, Stroke::new(1.0, Color32::from_black_alpha(90)));
    // The hue strip.
    let (strip, sr) = ui.allocate_exact_size(vec2(w, 14.0), Sense::click_and_drag());
    let mut mesh = egui::Mesh::default();
    let steps = 36;
    for k in 0..=steps {
        let t = k as f32 / steps as f32;
        let c = Color32::from(egui::ecolor::Hsva::new(t, 1.0, 1.0, 1.0));
        mesh.colored_vertex(pos2(strip.min.x + t * w, strip.min.y), c);
        mesh.colored_vertex(pos2(strip.min.x + t * w, strip.max.y), c);
        if k > 0 { let a = (k as u32 - 1) * 2; mesh.add_triangle(a, a + 1, a + 2); mesh.add_triangle(a + 1, a + 3, a + 2); }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    if let Some(p) = sr.interact_pointer_pos() { hsv[0] = ((p.x - strip.min.x) / w).clamp(0.0, 0.999); }
    let hx = strip.min.x + hsv[0] * w;
    ui.painter().circle(pos2(hx, strip.center().y), 8.0, Color32::from(egui::ecolor::Hsva::new(hsv[0], 1.0, 1.0, 1.0)), Stroke::new(2.5, Color32::WHITE));
    ui.painter().circle_stroke(pos2(hx, strip.center().y), 9.5, Stroke::new(1.0, Color32::from_black_alpha(90)));
    if fr.dragged() || fr.clicked() || sr.dragged() || sr.clicked() { *rgb = hsv_to_rgb(hsv); }
    // Preview and hex.
    ui.allocate_ui_with_layout(vec2(w, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let (pv, _) = ui.allocate_exact_size(vec2(56.0, 28.0), Sense::hover());
        ui.painter().rect(pv, 6.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]), Stroke::new(1.0, Color32::from_black_alpha(30)), StrokeKind::Inside);
        ui.add_space(6.0);
        ui.label(RichText::new("#").color(c_ink3()));
        let id = egui::Id::new("custom-color-hex");
        let mut hex = ui.ctx().data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
        let r = ui.add(egui::TextEdit::singleline(&mut hex).desired_width(70.0).font(FontId::monospace(13.0)).char_limit(7));
        if r.changed() { if let Some(c) = Color::from_hex(hex.trim_start_matches('#')) { *rgb = [c.0, c.1, c.2]; hsv = rgb_to_hsv(*rgb); } }
        if r.has_focus() { ui.ctx().data_mut(|d| d.insert_temp(id, hex)); } else { ui.ctx().data_mut(|d| d.remove::<String>(id)); }
        ui.label(RichText::new(format!("{} {} {}", rgb[0], rgb[1], rgb[2])).size(11.5).color(c_ink3()));
    });
    ui.ctx().data_mut(|d| d.insert_temp(hsv_id, hsv));
    let mut ok = false;
    crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { ok = true; });
    ok.then(|| Color(rgb[0], rgb[1], rgb[2]))
}

/// An icon with a color bar beneath (font color, highlight) and a drop-down arrow.
pub fn color_split(ui: &mut Ui, icon: &str, color: Color32, tip: &str) -> (Response, Response) {
    let (rect, r) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
    button_bg(ui, rect, &r, false);
    paint_icon(ui, icon, Rect::from_min_size(rect.min + vec2(5.0, 2.0), vec2(16.0, 16.0)), 16, true);
    ui.painter().rect_filled(Rect::from_min_size(rect.min + vec2(5.0, 18.0), vec2(16.0, 3.5)), 0.0, color);
    let (arect, ar) = ui.allocate_exact_size(vec2(12.0, 24.0), Sense::click());
    button_bg(ui, arect, &ar, false);
    caret(ui, arect.center());
    (r.on_hover_text(crate::i18n::tr(&tip)), ar.on_hover_text(crate::i18n::tr(&tip)))
}

/// A large button whose label is followed by a drop-down caret (Word's split menus).
pub fn large_menu(ui: &mut Ui, icon: &str, label: &str, tip: &str) -> Response {
    let label_t = crate::i18n::tr(label);
    let label = label_t.as_str();
    // Room for the caret after the label: the label is centered with a trailing gap, and the
    // caret follows the label's last line.
    let padded = format!("{label}\u{2003}");
    let r = large(ui, icon, &padded, false, true, tip);
    let font = if compact(ui) { FontId::proportional(10.5) } else { FontId::proportional(11.5) };
    let wrap = if compact(ui) { r.rect.width() - 9.0 } else { 84.0 };
    let g = ui.painter().layout(padded, font.clone(), c_ink2(), wrap);
    let last = g.rows.last().map(|row| row.rect()).unwrap_or(Rect::NOTHING);
    let space = ui.painter().layout_no_wrap("\u{2003}".into(), font, c_ink2()).size().x;
    let x0 = r.rect.center().x - g.size().x / 2.0;
    caret(ui, pos2(x0 + last.max.x - space + 6.0, r.rect.min.y + 39.0 + last.center().y));
    r
}

/// A small labelled command with a drop-down caret.
pub fn small_menu(ui: &mut Ui, icon: &str, label: &str, tip: &str) -> Response {
    let label_t = crate::i18n::tr(label);
    let label = label_t.as_str();
    let r = small(ui, icon, Some(&format!("{label}   ")), false, true, tip);  // label is translated above
    if !compact(ui) { caret(ui, pos2(r.rect.max.x - 8.0, r.rect.center().y)); }
    r
}

/// A row of the File menu (Qalam, Sard, Jadwal): icon, label, and the shortcut at the far
/// side, full width, mirrored for right-to-left languages.
pub fn file_item(ui: &mut Ui, icon: &str, label: &str, key: &str) -> bool {
    let label = crate::i18n::tr(label);
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    button_bg(ui, rect, &r, false);
    let rtl = crate::i18n::rtl();
    let (icon_x, text_at, text_align, key_at, key_align) = if rtl {
        (rect.max.x - 24.0, pos2(rect.max.x - 34.0, rect.center().y), Align2::RIGHT_CENTER, pos2(rect.min.x + 10.0, rect.center().y), Align2::LEFT_CENTER)
    } else {
        (rect.min.x + 8.0, pos2(rect.min.x + 34.0, rect.center().y), Align2::LEFT_CENTER, pos2(rect.max.x - 10.0, rect.center().y), Align2::RIGHT_CENTER)
    };
    paint_icon(ui, icon, Rect::from_min_size(pos2(icon_x, rect.center().y - 8.0), vec2(16.0, 16.0)), 16, true);
    ui.painter().text(text_at, text_align, label, FontId::proportional(13.0), c_ink());
    if !key.is_empty() { ui.painter().text(key_at, key_align, key, FontId::proportional(12.0), c_ink3()); }
    r.clicked()
}

pub fn menu_item(ui: &mut Ui, icon: &str, label: &str) -> bool { small(ui, icon, Some(label), false, true, label).clicked() }

/// Is this mostly right-to-left text (Arabic, Hebrew)?
pub fn is_rtl_text(s: &str) -> bool {
    let rtl = s.chars().filter(|c| ('\u{0590}'..='\u{08FF}').contains(c) || ('\u{FB1D}'..='\u{FEFF}').contains(c)).count();
    let ltr = s.chars().filter(|c| c.is_alphabetic()).count().saturating_sub(rtl);
    rtl > 0 && rtl * 2 >= ltr
}

/// Right-to-left text shown correctly: each paragraph is wrapped into lines first (in reading
/// order, by measured width), then each line is put in display order and aligned right.
/// (egui on its own lays words left to right and wraps after, which jumbles Arabic.)
pub fn rtl_text(ui: &mut Ui, text: &str, width: f32, font: egui::FontId, color: Color32) {
    let measure = |ui: &Ui, s: &str| ui.painter().layout_no_wrap(crate::i18n::shape(s), font.clone(), color).size().x;
    ui.vertical(|ui| {
        ui.set_width(width);
        for para in text.split('\n') {
            let mut lines: Vec<String> = vec![];
            let mut cur = String::new();
            for w in para.split_whitespace() {
                let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
                if !cur.is_empty() && measure(ui, &cand) > width - 8.0 { lines.push(std::mem::replace(&mut cur, w.to_string())); } else { cur = cand; }
            }
            if !cur.is_empty() || lines.is_empty() { lines.push(cur); }
            let row = ui.fonts_mut(|f| f.row_height(&font)) + 4.0;
            for l in lines {
                ui.allocate_ui_with_layout(egui::vec2(width, row), egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(egui::RichText::new(crate::i18n::shape(&l)).font(font.clone()).color(color)); });
            }
        }
    });
}
