//! The table tabs: Table Design (style options, styles, shading, borders) and Table Layout
//! (select, delete, insert rows and columns, merge and split, cell size, alignment, margins).

use crate::app::SlidesApp;
use crate::i18n::tr;
use egui::{vec2, Color32, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::scene::{Kind, Loc};

impl SlidesApp {
    /// The table being worked on and the cells the command applies to: the selected range,
    /// the edited cell, or every cell.
    pub fn table_target(&self) -> Option<(Loc, Vec<(usize, usize)>)> {
        let loc = self.cell_sel.as_ref().map(|c| c.0.clone()).or_else(|| self.edit.as_ref().filter(|e| e.cell.is_some()).map(|e| e.loc.clone())).or_else(|| self.selection.first().cloned())?;
        let (rows, cols) = self.pres.table_size(&loc)?;
        let cells = match (&self.cell_sel, self.edit.as_ref().and_then(|e| e.cell)) {
            (Some((_, a, b)), _) => (a.0.min(b.0)..=a.0.max(b.0)).flat_map(|r| (a.1.min(b.1)..=a.1.max(b.1)).map(move |c| (r, c))).collect(),
            (None, Some(c)) => vec![c],
            _ => (0..rows).flat_map(|r| (0..cols).map(move |c| (r, c))).collect(),
        };
        Some((loc, cells))
    }
    /// The row and column under the caret (or the first selected cell).
    fn table_anchor(&self) -> Option<(Loc, usize, usize, usize, usize)> {
        let (loc, cells) = self.table_target()?;
        let r0 = cells.iter().map(|c| c.0).min()?; let r1 = cells.iter().map(|c| c.0).max()?;
        let c0 = cells.iter().map(|c| c.1).min()?; let c1 = cells.iter().map(|c| c.1).max()?;
        Some((loc, r0, r1, c0, c1))
    }
    fn table_cmd(&mut self, f: impl FnOnce(&mut slides_engine::Presentation, &Loc, &[(usize, usize)])) {
        let Some((loc, cells)) = self.table_target() else { return };
        self.checkpoint("table");
        f(&mut self.pres, &loc, &cells);
    }
}

fn hex(c: word_engine::Color) -> String { format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2) }

pub fn design(app: &mut SlidesApp, ui: &mut Ui) {
    let Some((loc, _)) = app.table_target() else { return };
    group(ui, "Table Style Options", |ui| {
        rows(ui, |ui| {
            for row in [[("firstRow", "Header Row"), ("firstCol", "First Column")], [("lastRow", "Total Row"), ("lastCol", "Last Column")], [("bandRow", "Banded Rows"), ("bandCol", "Banded Columns")]] {
                crate::theme::hrow(ui, |ui| for (k, n) in row {
                    let mut on = app.pres.table_option(&loc, k);
                    if crate::theme::check(ui, &mut on, tr(n)).changed() { app.checkpoint("table"); app.pres.set_table_option(&loc, k, on); }
                });
            }
        });
    });
    group(ui, "Table Styles", |ui| {
        for (id, name, token) in slides_engine::table::TABLE_STYLES {
            let base = match *token { "none" => Color32::WHITE, "grid" => Color32::WHITE, "tx1" => Color32::from_gray(0x40), t => { let n: usize = t.trim_start_matches("accent").parse().unwrap_or(1); let c = slides_engine::Rgba::hex(&app.accent_hex(n)).unwrap_or(slides_engine::Rgba::BLACK); Color32::from_rgb(c.r, c.g, c.b) } };
            let (r, resp) = ui.allocate_exact_size(vec2(58.0, 44.0), Sense::click());
            let cell = |row: usize, col: usize| egui::Rect::from_min_size(r.min + vec2(4.0 + col as f32 * 12.5, 4.0 + row as f32 * 9.0), vec2(12.0, 8.5));
            for row in 0..4 { for col in 0..4 {
                let fill = match *token { "none" => Color32::WHITE, "grid" => Color32::WHITE, _ => if row == 0 { base } else if row % 2 == 1 { base.gamma_multiply(0.3) } else { base.gamma_multiply(0.15) } };
                ui.painter().rect(cell(row, col), 0.0, fill, Stroke::new(0.5, if *token == "grid" { Color32::BLACK } else { Color32::WHITE }), egui::StrokeKind::Inside);
            } }
            if resp.hovered() { ui.painter().rect_stroke(r, 2.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside); }
            if resp.on_hover_text(tr(name)).clicked() { app.checkpoint("table"); app.pres.set_table_style(&loc, id, token); }
        }
        rows(ui, |ui| {
            let sh = small_menu(ui, "shading", "Shading", "Cell shading");
            crate::theme::menu(&sh, |ui| if let Some(pick) = color_menu(ui, "No Fill") { let v = pick.map(hex).unwrap_or_default(); app.table_cmd(move |p, l, c| p.set_cell_fill(l, c, Some(&v))); ui.close(); });
            let bd = small_menu(ui, "borders", "Borders", "Cell borders");
            crate::theme::menu(&bd, |ui| {
                let id = egui::Id::new("tbl-pen");
                let mut pen: [u8; 3] = ui.data(|d| d.get_temp(id)).unwrap_or([0, 0, 0]);
                let mut c = word_engine::Color(pen[0], pen[1], pen[2]);
                crate::theme::hrow(ui, |ui| { ui.label(tr("Pen Color")); if color_button(ui, &mut c) { pen = [c.0, c.1, c.2]; } });
                ui.data_mut(|d| d.insert_temp(id, pen));
                for (k, icon, n) in [("none", "borderNone", "No Border"), ("all", "borderAll", "All Borders"), ("outside", "borderOuter", "Outside Borders"), ("inside", "borderInside", "Inside Borders"), ("top", "borderTop", "Top Border"), ("bottom", "borderBottom", "Bottom Border"), ("left", "borderLeft", "Left Border"), ("right", "borderRight", "Right Border")] {
                    if menu_item(ui, icon, n) { let h = hex(c); app.table_cmd(move |p, l, cells| p.set_cell_borders(l, cells, k, &h, 1.0)); ui.close(); }
                }
            });
        });
    });
}

pub fn layout(app: &mut SlidesApp, ui: &mut Ui) {
    let Some((loc, r0, r1, c0, c1)) = app.table_anchor() else { return };
    group(ui, "Table", |ui| {
        rows(ui, |ui| {
            if small(ui, "selectCell", Some("Select Cell"), false, true, "Select the cell").clicked() { app.edit = None; app.cell_sel = Some((loc.clone(), (r0, c0), (r0, c0))); }
            if small(ui, "selectRow", Some("Select Row"), false, true, "Select the row").clicked() { let cols = app.pres.table_size(&loc).map(|s| s.1).unwrap_or(1); app.edit = None; app.cell_sel = Some((loc.clone(), (r0, 0), (r1, cols - 1))); }
            if small(ui, "selectColumn", Some("Select Column"), false, true, "Select the column").clicked() { let rows = app.pres.table_size(&loc).map(|s| s.0).unwrap_or(1); app.edit = None; app.cell_sel = Some((loc.clone(), (0, c0), (rows - 1, c1))); }
        });
    });
    group(ui, "Rows & Columns", |ui| {
        let del = large_menu(ui, "deleteTable", "Delete", "Delete rows, columns or the table");
        crate::theme::menu(&del, |ui| {
            if menu_item(ui, "deleteColumn", "Delete Columns") { app.checkpoint("table"); for c in (c0..=c1).rev() { app.pres.delete_column(&loc, c); } app.edit = None; app.cell_sel = None; ui.close(); }
            if menu_item(ui, "deleteRow", "Delete Rows") { app.checkpoint("table"); for r in (r0..=r1).rev() { app.pres.delete_row(&loc, r); } app.edit = None; app.cell_sel = None; ui.close(); }
            if menu_item(ui, "deleteTable", "Delete Table") { app.checkpoint("table"); app.pres.delete_table(&loc); app.selection.clear(); app.edit = None; app.cell_sel = None; ui.close(); }
        });
        if large(ui, "insertAbove", "Insert Above", false, true, "Insert a row above").clicked() { app.checkpoint("table"); app.pres.insert_row(&loc, r0); bump_edit(app, 1, 0); }
        rows(ui, |ui| {
            if small(ui, "insertBelow", Some("Insert Below"), false, true, "Insert a row below").clicked() { app.checkpoint("table"); app.pres.insert_row(&loc, r1 + 1); }
            if small(ui, "insertLeft", Some("Insert Left"), false, true, "Insert a column to the left").clicked() { app.checkpoint("table"); app.pres.insert_column(&loc, c0); bump_edit(app, 0, 1); }
            if small(ui, "insertRight", Some("Insert Right"), false, true, "Insert a column to the right").clicked() { app.checkpoint("table"); app.pres.insert_column(&loc, c1 + 1); }
        });
    });
    group(ui, "Merge", |ui| {
        if large(ui, "mergeCells", "Merge Cells", false, r1 > r0 || c1 > c0, "Merge the selected cells").clicked() { app.checkpoint("table"); app.pres.merge_cells(&loc, r0, c0, r1, c1); app.cell_sel = None; }
        if large(ui, "splitCells", "Split Cells", false, true, "Split a merged cell").clicked() { app.checkpoint("table"); app.pres.split_cell(&loc, r0, c0); }
    });
    group(ui, "Cell Size", |ui| {
        let inch = 914_400.0;
        let scene = app.scene();
        let (cw, rh) = scene.as_ref().and_then(|s| s.items.iter().find(|i| i.own && i.loc == loc)).and_then(|i| if let Kind::Table(t) = &i.kind { Some((t.cols.get(c0).copied().unwrap_or(0.0), t.rows.get(r0).copied().unwrap_or(0.0))) } else { None }).unwrap_or((0.0, 0.0));
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| { ui.label(tr("Height")); let mut h = rh / inch; if ui.add(egui::DragValue::new(&mut h).range(0.1..=20.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { app.checkpoint("typing"); for r in r0..=r1 { app.pres.set_row_height(&loc, r, h * inch); } } });
            crate::theme::hrow(ui, |ui| { ui.label(tr("Width")); let mut w = cw / inch; if ui.add(egui::DragValue::new(&mut w).range(0.1..=30.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { app.checkpoint("typing"); for c in c0..=c1 { app.pres.set_column_width(&loc, c, w * inch); } } });
            crate::theme::hrow(ui, |ui| {
                if small(ui, "distributeRows", Some("Distribute Rows"), false, true, "Make the rows the same height").clicked() { app.checkpoint("table"); app.pres.distribute_rows(&loc); }
                if small(ui, "distributeColumns", Some("Distribute Columns"), false, true, "Make the columns the same width").clicked() { app.checkpoint("table"); app.pres.distribute_columns(&loc); }
            });
        });
    });
    group(ui, "Alignment", |ui| {
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| for (icon, a, n) in [("alignLeft", "l", "Align Left"), ("alignCenter", "ctr", "Center"), ("alignRight", "r", "Align Right")] {
                if small(ui, icon, None, false, true, n).clicked() { let cells = app.table_target().map(|t| t.1).unwrap_or_default(); let l = loc.clone(); app.checkpoint("table"); for c in cells { app.pres.set_align(&l, Some(c), None, a); } }
            });
            crate::theme::hrow(ui, |ui| for (icon, a, n) in [("valignTop", "t", "Align Top"), ("valignMiddle", "ctr", "Center Vertically"), ("valignBottom", "b", "Align Bottom")] {
                if small(ui, icon, None, false, true, n).clicked() { app.table_cmd(move |p, l, c| p.set_cell_anchor(l, c, a)); }
            });
            let m = small_menu(ui, "cellMargins", "Cell Margins", "Space between the cell's edge and its text");
            crate::theme::menu(&m, |ui| for (n, v) in [("Normal", [7.2, 3.6, 7.2, 3.6]), ("None", [0.0; 4]), ("Narrow", [3.6, 3.6, 3.6, 3.6]), ("Wide", [14.4, 14.4, 14.4, 14.4])] {
                if ui.button(tr(n)).clicked() { app.table_cmd(move |p, l, c| p.set_cell_margins(l, c, v)); ui.close(); }
            });
        });
    });
}

/// Keep the edited cell on its text after rows or columns are inserted before it.
fn bump_edit(app: &mut SlidesApp, dr: usize, dc: usize) {
    if let Some(e) = &mut app.edit { if let Some((r, c)) = e.cell { e.cell = Some((r + dr, c + dc)); } }
    if let Some((_, a, b)) = &mut app.cell_sel { a.0 += dr; b.0 += dr; a.1 += dc; b.1 += dc; }
}
