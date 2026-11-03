//! Layout in the interface's reading direction: rows, wrapped rows, menu items and corners
//! that mirror in Arabic.

use egui::{Align, Align2, Layout, Pos2, Rect, Response, Sense, Ui, Vec2, pos2, vec2};
use qalam_kit::colors::*;

pub fn rtl() -> bool { crate::i18n::rtl() }

/// A row of widgets in reading order (replaces `ui.horizontal`). In Arabic the row runs right
/// to left, sized to its content (measured on the previous frame), so it also works in
/// floating bars, whose width comes from what they hold.
pub fn row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if !rtl() { return ui.horizontal(add); }
    let id = ui.next_auto_id().with("rtl-row");
    let w: f32 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(0.0);
    let h = ui.spacing().interact_size.y;
    let r = ui.allocate_ui_with_layout(vec2(w.max(1.0), h), Layout::right_to_left(Align::Center), |ui| {
        let out = add(ui);
        (out, ui.min_rect().width())
    });
    let (out, used) = r.inner;
    if (used - w).abs() > 0.5 { ui.ctx().data_mut(|d| d.insert_temp(id, used)); ui.ctx().request_repaint(); }
    egui::InnerResponse::new(out, r.response)
}

/// A row that wraps onto more lines, from the reading side.
pub fn wrap<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if !rtl() { return ui.horizontal_wrapped(add); }
    ui.with_layout(Layout::right_to_left(Align::Center).with_main_wrap(true), add)
}

/// A menu item: its text at the reading side and its shortcut at the far side.
pub fn item(ui: &mut Ui, text: &str, shortcut: &str, enabled: bool) -> Response {
    let font = egui::FontId::proportional(13.5);
    let ink = if enabled { c_ink() } else { c_ink3() };
    let label = ui.painter().layout_no_wrap(text.to_string(), font.clone(), ink);
    let keys = ui.painter().layout_no_wrap(shortcut.to_string(), egui::FontId::proportional(12.0), c_ink3());
    let w = (label.size().x + keys.size().x + 48.0).max(ui.available_width()).max(200.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), if enabled { Sense::click() } else { Sense::hover() });
    if resp.hovered() && enabled { ui.painter().rect_filled(rect, 4.0, c_hover()); }
    let (lx, kx) = if rtl() { (rect.max.x - 10.0 - label.size().x, rect.min.x + 10.0) } else { (rect.min.x + 10.0, rect.max.x - 10.0 - keys.size().x) };
    let y = rect.center().y;
    ui.painter().galley(pos2(lx, y - label.size().y / 2.0), label, ink);
    ui.painter().galley(pos2(kx, y - keys.size().y / 2.0), keys, c_ink3());
    resp
}

/// A corner of the board, mirrored in Arabic: the point `inset` inside it and the pivot to
/// place a floating bar there. `start` is the reading side (left in English).
pub fn corner(board: Rect, start: bool, top: bool, inset: Vec2) -> (Pos2, Align2) {
    let left = start != rtl();
    let x = if left { board.min.x + inset.x } else { board.max.x - inset.x };
    let y = if top { board.min.y + inset.y } else { board.max.y - inset.y };
    let pivot = match (left, top) { (true, true) => Align2::LEFT_TOP, (false, true) => Align2::RIGHT_TOP, (true, false) => Align2::LEFT_BOTTOM, (false, false) => Align2::RIGHT_BOTTOM };
    (pos2(x, y), pivot)
}
