//! Sheet tabs (ui/sheettabs.js): one tab per visible sheet with its color, a + to add one,
//! double-click to rename, drag to reorder, and a menu to insert, delete, rename, move or copy,
//! color, hide and unhide.

use crate::app::{Dialog, SheetApp};
use crate::i18n::tr;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Rect, Sense, Stroke, Ui};
use qalam_kit::colors::*;

pub fn tabs(app: &mut SheetApp, ui: &mut Ui) {
    egui::Panel::bottom("sheet-tabs").show_separator_line(false).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin { left: 8, right: 8, top: 0, bottom: 2 })).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.set_height(28.0);
            // Previous / next sheet.
            let visible: Vec<usize> = (0..app.book.sheets.len()).filter(|i| !app.book.sheets[*i].hidden).collect();
            let at = visible.iter().position(|i| *i == app.sheet()).unwrap_or(0);
            if ui.add_enabled(at > 0, egui::Button::new("◀").frame(false)).on_hover_text(tr("Previous sheet")).clicked() { app.set_sheet(visible[at - 1]); }
            if ui.add_enabled(at + 1 < visible.len(), egui::Button::new("▶").frame(false)).on_hover_text(tr("Next sheet")).clicked() { app.set_sheet(visible[at + 1]); }
            ui.add_space(6.0);
            let drag_id = egui::Id::new("tab-drag");
            let mut rects: Vec<(usize, Rect)> = vec![];
            egui::ScrollArea::horizontal().id_salt("tabs-scroll").max_width(ui.available_width() - 40.0).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for &i in &visible {
                        let s = &app.book.sheets[i];
                        let active = i == app.sheet();
                        let font = FontId::new(13.0, if active { FontFamily::Name("ui-bold".into()) } else { FontFamily::Proportional });
                        let g = ui.painter().layout_no_wrap(s.name.clone(), font, if active { crate::grid::SELECT } else { c_ink2() });
                        let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 26.0), Sense::click_and_drag());
                        let p = ui.painter();
                        if active { p.rect_filled(rect, 0.0, c_card()); p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(3.0, crate::grid::SELECT)); }
                        else if r.hovered() { p.rect_filled(rect, 0.0, c_hover()); }
                        if let Some(c) = s.tab_color { p.rect_filled(Rect::from_min_size(pos2(rect.min.x + 4.0, rect.max.y - 4.0), vec2(rect.width() - 8.0, 3.0)), 1.0, crate::app::rgb(c)); }
                        p.line_segment([rect.right_top() + vec2(0.0, 6.0), rect.right_bottom() - vec2(0.0, 6.0)], Stroke::new(1.0, c_line()));
                        p.galley(pos2(rect.center().x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0), g, c_ink());
                        if r.clicked() { app.set_sheet(i); }
                        if r.double_clicked() { app.dialog = Some(Dialog::Rename { sheet: i, name: app.book.sheets[i].name.clone() }); }
                        if r.drag_started() { ui.data_mut(|d| d.insert_temp(drag_id, i)); }
                        r.context_menu(|ui| tab_menu(app, ui, i));
                        rects.push((i, rect));
                    }
                });
            });
            // Dropping a dragged tab moves the sheet.
            if let Some(from) = ui.data(|d| d.get_temp::<usize>(drag_id)) {
                if let Some(pp) = ui.input(|i| i.pointer.hover_pos()) {
                    if let Some((to, rc)) = rects.iter().find(|(_, rc)| rc.x_range().contains(pp.x)) {
                        ui.painter().line_segment([pos2(if pp.x < rc.center().x { rc.min.x } else { rc.max.x }, rc.min.y), pos2(if pp.x < rc.center().x { rc.min.x } else { rc.max.x }, rc.max.y)], Stroke::new(2.0, crate::grid::SELECT));
                        if ui.input(|i| i.pointer.any_released()) {
                            let to = if pp.x < rc.center().x { *to } else { to + 1 };
                            let to = if to > from { to - 1 } else { to };
                            if to != from { app.checkpoint("Move Sheet"); app.book.move_sheet(from, to); app.dirty = true; }
                        }
                    }
                }
                if ui.input(|i| i.pointer.any_released()) { ui.data_mut(|d| d.remove::<usize>(drag_id)); }
            }
            let (rect, r) = ui.allocate_exact_size(vec2(28.0, 26.0), Sense::click());
            if r.hovered() { ui.painter().rect_filled(rect, 4.0, c_hover()); }
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "+", FontId::proportional(18.0), c_ink2());
            if r.on_hover_text(tr("New sheet")).clicked() { add_sheet(app, app.sheet() + 1); }
        });
    });
}

pub fn add_sheet(app: &mut SheetApp, at: usize) {
    if crate::review_tab::structure_locked(app) { return; }
    app.commit(None);
    app.checkpoint("Insert Sheet");
    let i = app.book.add_sheet(at, None);
    app.set_sheet(i);
    app.dirty = true;
}

const TAB_COLORS: [[u8; 3]; 10] = [[0xC0, 0x00, 0x00], [0xFF, 0x00, 0x00], [0xFF, 0xC0, 0x00], [0xFF, 0xFF, 0x00], [0x92, 0xD0, 0x50], [0x00, 0xB0, 0x50], [0x00, 0xB0, 0xF0], [0x00, 0x70, 0xC0], [0x00, 0x20, 0x60], [0x70, 0x30, 0xA0]];

fn tab_menu(app: &mut SheetApp, ui: &mut Ui, i: usize) {
    ui.set_min_width(180.0);
    let item = |ui: &mut Ui, icon: &str, label: &str| qalam_kit::widgets::menu_item(ui, icon, label);
    if app.book.review.structure { ui.label(crate::i18n::tr("The workbook structure is protected.")); if item(ui, "protectWorkbook", "Unprotect Workbook…") { app.rv.unprotect = Some((true, String::new(), None)); ui.close(); } return; }
    if item(ui, "insertTab", "Insert Sheet") { add_sheet(app, i); ui.close(); }
    if item(ui, "delete", "Delete") {
        app.checkpoint("Delete Sheet");
        match app.book.delete_sheet(i) { Ok(()) => { let a = app.book.active; app.set_sheet(a); app.changed(); } Err(e) => { app.undo(); app.error = Some(tr(&e)); } }
        ui.close();
    }
    if item(ui, "rename", "Rename") { app.dialog = Some(Dialog::Rename { sheet: i, name: app.book.sheets[i].name.clone() }); ui.close(); }
    if item(ui, "copy", "Duplicate") { app.checkpoint("Duplicate Sheet"); let n = app.book.duplicate_sheet(i); app.set_sheet(n); app.changed(); ui.close(); }
    if i > 0 && item(ui, "moveUp", "Move Left") { app.checkpoint("Move Sheet"); app.book.move_sheet(i, i - 1); app.dirty = true; ui.close(); }
    if i + 1 < app.book.sheets.len() && item(ui, "moveDown", "Move Right") { app.checkpoint("Move Sheet"); app.book.move_sheet(i, i + 1); app.dirty = true; ui.close(); }
    crate::theme::submenu(ui, tr("Tab Color"), |ui| {
        ui.horizontal(|ui| {
            for c in TAB_COLORS {
                if qalam_kit::widgets::swatch(ui, crate::app::rgb(c), 18.0, app.book.sheets[i].tab_color == Some(c)).clicked() { app.checkpoint("Tab Color"); app.book.sheets[i].tab_color = Some(c); app.dirty = true; ui.close(); }
            }
        });
        if qalam_kit::widgets::none_row(ui, "No Color") { app.checkpoint("Tab Color"); app.book.sheets[i].tab_color = None; app.dirty = true; ui.close(); }
    });
    ui.separator();
    if item(ui, "hideInk", "Hide") {
        if app.book.sheets.iter().filter(|s| !s.hidden).count() > 1 {
            app.checkpoint("Hide Sheet");
            app.book.sheets[i].hidden = true;
            if app.sheet() == i { let n = (0..app.book.sheets.len()).find(|k| !app.book.sheets[*k].hidden).unwrap_or(0); app.set_sheet(n); }
            app.dirty = true;
        } else { app.error = Some(tr("A workbook must keep at least one visible sheet.")); }
        ui.close();
    }
    let hidden: Vec<usize> = (0..app.book.sheets.len()).filter(|k| app.book.sheets[*k].hidden).collect();
    if !hidden.is_empty() {
        crate::theme::submenu(ui, tr("Unhide"), |ui| {
            for k in hidden { if ui.button(app.book.sheets[k].name.clone()).clicked() { app.checkpoint("Unhide Sheet"); app.book.sheets[k].hidden = false; app.set_sheet(k); app.dirty = true; ui.close(); } }
        });
    }
    let protected = app.book.sheets[i].protected;
    if item(ui, "protect", if protected { "Unprotect Sheet" } else { "Protect Sheet" }) { app.checkpoint("Protect Sheet"); app.book.sheets[i].protected = !protected; app.dirty = true; ui.close(); }
    let _ = Color32::BLACK;
}
