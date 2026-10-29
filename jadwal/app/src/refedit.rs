//! Range fields in dialogs (Excel's RefEdit): a text box with a button that collapses the dialog so
//! cells can be picked on the sheet. While picking, dialogs aren't drawn (`theme::PICKING`), a small
//! bar shows the address being picked, and the grid works as usual (click, drag, other sheets).
//! Enter or the bar's button puts the address into the field; Escape leaves it as it was.

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{vec2, Key, RichText, Stroke, Ui};
use qalam_kit::colors::*;
use sheet_engine::addr::{col_name, quote_sheet, Range, MAX_COLS, MAX_ROWS};

#[derive(Clone)]
struct Picking { field: egui::Id, text: String, sheet: usize, last: Option<(usize, Range)> }

fn state_id() -> egui::Id { egui::Id::new("refedit-picking") }
fn result_id(field: egui::Id) -> egui::Id { field.with("result") }

/// Is a range field picking cells now?
pub fn active(ctx: &egui::Context) -> bool { ctx.data(|d| d.get_temp::<Picking>(state_id()).is_some()) }

/// A range field `width` wide; `salt` keeps it apart from the dialog's other range fields.
pub fn field(ui: &mut Ui, text: &mut String, salt: &str, width: f32) -> egui::Response {
    let id = egui::Id::new(("refedit", salt));
    let picked = ui.data_mut(|d| d.remove_temp::<String>(result_id(id)));
    let mut r = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let r = ui.add(egui::TextEdit::singleline(text).desired_width((width - 26.0).max(40.0)).id(id));
        let b = ui.add(egui::Button::new(RichText::new("⬚").size(13.0)).min_size(vec2(24.0, 20.0))).on_hover_text(tr("Collapse the dialog and select cells on the sheet"));
        if b.clicked() {
            let sheet = ui.ctx().data(|d| d.get_temp::<usize>(egui::Id::new("refedit-sheet"))).unwrap_or(0);
            ui.ctx().data_mut(|d| d.insert_temp(state_id(), Picking { field: id, text: text.clone(), sheet, last: None }));
        }
        r
    }).inner;
    if let Some(t) = picked { *text = t; r.mark_changed(); }
    r
}

/// The address of a range as a range field shows it: absolute, with the sheet when it's not the
/// sheet the dialog was opened on (or always, for names that need it).
pub fn address(app: &SheetApp, sheet: usize, r: Range, from: usize) -> String {
    let c = |row: u32, col: u32| format!("${}${}", col_name(col), row + 1);
    let body = if r.c0 == 0 && r.c1 == MAX_COLS - 1 { format!("${}:${}", r.r0 + 1, r.r1 + 1) }
        else if r.r0 == 0 && r.r1 == MAX_ROWS - 1 { format!("${}:${}", col_name(r.c0), col_name(r.c1)) }
        else if r.is_cell() { c(r.r0, r.c0) } else { format!("{}:{}", c(r.r0, r.c0), c(r.r1, r.c1)) };
    if sheet == from { body } else { format!("{}!{body}", quote_sheet(&app.book.sheets[sheet].name)) }
}

/// Each frame: keeps the kit's dialogs out of the way while picking, follows the selection, and draws
/// the bar. Call before the dialogs are shown.
pub fn frame(app: &mut SheetApp, ctx: &egui::Context) {
    // Remember the sheet a dialog opens on, so picked ranges on other sheets name their sheet.
    if !active(ctx) { ctx.data_mut(|d| d.insert_temp(egui::Id::new("refedit-sheet"), app.sheet())); }
    let Some(mut p) = ctx.data(|d| d.get_temp::<Picking>(state_id())) else { qalam_kit::theme::PICKING.with(|f| f.set(false)); return };
    qalam_kit::theme::PICKING.with(|f| f.set(true));
    // The selection drives the text, but typing in the bar wins until the selection moves again.
    let now = (app.sheet(), app.sel.range());
    if p.last.is_some_and(|l| l != now) || (p.last.is_none() && app.grid.drag == crate::grid::Drag::Cells) {
        let rs = app.sel.ranges();
        p.text = rs.iter().map(|r| address(app, now.0, *r, p.sheet)).collect::<Vec<_>>().join(",");
    }
    p.last = Some(now);
    let mut finish: Option<bool> = None;
    egui::Area::new(egui::Id::new("refedit-bar")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 150.0)).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).fill(c_card()).stroke(Stroke::new(1.0, crate::grid::SELECT)).show(ui, |ui| {
            ui.set_min_width(380.0);
            ui.label(RichText::new(tr("Select a range on the sheet, then press Enter")).size(12.0).color(c_ink2()));
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut p.text).desired_width(320.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { finish = Some(true); }
                if ui.add(egui::Button::new(RichText::new("⤶").size(14.0)).min_size(vec2(28.0, 22.0))).on_hover_text(tr("Back to the dialog")).clicked() { finish = Some(true); }
            });
        });
    });
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) { finish = Some(true); }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) { finish = Some(false); }
    match finish {
        Some(keep) => {
            if keep { ctx.data_mut(|d| d.insert_temp(result_id(p.field), p.text.clone())); }
            ctx.data_mut(|d| d.remove::<Picking>(state_id()));
            // Back on the sheet the dialog was opened on.
            if app.sheet() != p.sheet && p.sheet < app.book.sheets.len() { app.set_sheet(p.sheet); }
            qalam_kit::theme::PICKING.with(|f| f.set(false));
            ctx.request_repaint();
        }
        None => { ctx.data_mut(|d| d.insert_temp(state_id(), p)); }
    }
}
