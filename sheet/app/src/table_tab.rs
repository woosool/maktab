//! The contextual Table Design tab (shown while the active cell is in a table): its name,
//! Resize, Convert to Range, Remove Duplicates, the style options and the style gallery.

use crate::app::{SheetApp, Tab};
use crate::i18n::tr;
use egui::Ui;
use qalam_kit::widgets::*;

/// The tab to add after the fixed ones, when the active cell is in a table (else leaves it).
pub fn contextual(app: &mut SheetApp) -> Option<(Tab, &'static str)> {
    let s = app.sheet();
    let inside = app.book.table_at(s, app.sel.active).is_some();
    if !inside && app.tab == Tab::TableDesign { app.tab = Tab::Home; }
    inside.then_some((Tab::TableDesign, "Table Design"))
}

pub fn table_design(app: &mut SheetApp, ui: &mut Ui) {
    let s = app.sheet();
    let Some(i) = app.book.table_at(s, app.sel.active) else { return };
    group(ui, "Properties", |ui| {
        crate::theme::vcol(ui, |ui| {
            ui.label(tr("Table Name:"));
            let id = egui::Id::new(("table-name", s, i));
            let mut name = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| app.book.sheets[s].tables[i].display_name.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(110.0));
            if r.lost_focus() {
                if name.trim() != app.book.sheets[s].tables[i].display_name {
                    app.checkpoint("Table Name");
                    match app.book.rename_table(s, i, &name) { Ok(()) => app.changed(), Err(e) => { app.undo(); app.error = Some(tr(&e)); } }
                }
                ui.data_mut(|d| d.remove::<String>(id));
            } else if r.has_focus() { ui.data_mut(|d| d.insert_temp(id, name)); }
            if small(ui, "tableProperties", Some("Resize Table"), false, true, "Add or remove rows and columns").clicked() { crate::filter_ui::open_resize(app, i); }
        });
    });
    group(ui, "Tools", |ui| {
        rows(ui, |ui| {
            if small(ui, "deleteRow", Some("Remove Duplicates"), false, true, "Delete rows that repeat in the table").clicked() {
                app.checkpoint("Remove Duplicates");
                let n = app.book.table_remove_duplicates(s, i);
                if n == 0 { app.undo(); app.notify(&tr("No duplicate values found.")); } else { app.changed(); app.notify(&crate::i18n::trf("{} duplicate values removed.", &[&n])); }
            }
            if small(ui, "convertToTable", Some("Convert to Range"), false, true, "Make the table an ordinary range, keeping its look").clicked() {
                app.checkpoint("Convert to Range");
                app.book.convert_to_range(s, i);
                app.changed();
                app.tab = Tab::Home;
            }
        });
    });
    group(ui, "Table Style Options", |ui| {
        let t = app.book.sheets[s].tables[i].clone();
        let mut opts = [t.header_row, t.totals_row, t.banded_rows, t.first_col, t.last_col, t.banded_cols, t.filter.is_some()];
        let labels = ["Header Row", "Total Row", "Banded Rows", "First Column", "Last Column", "Banded Columns", "Filter Button"];
        let mut changed = None;
        for chunk in [0..3, 3..6, 6..7] {
            crate::theme::vcol(ui, |ui| for k in chunk { if crate::theme::check(ui, &mut opts[k], tr(labels[k])).changed() { changed = Some(k); } });
        }
        if let Some(k) = changed {
            app.checkpoint("Table Style Options");
            let on = opts[k];
            let res = match k {
                0 => app.book.set_table_header(s, i, on),
                1 => { app.book.set_table_totals(s, i, on); Ok(()) }
                6 => { let t = &mut app.book.sheets[s].tables[i]; t.filter = if on { Some(Default::default()) } else { None }; app.book.apply_filters(s); Ok(()) }
                _ => { let t = &mut app.book.sheets[s].tables[i]; match k { 2 => t.banded_rows = on, 3 => t.first_col = on, 4 => t.last_col = on, _ => t.banded_cols = on } Ok(()) }
            };
            match res { Ok(()) => app.changed(), Err(e) => { app.undo(); app.error = Some(tr(&e)); } }
        }
    });
    group(ui, "Table Styles", |ui| {
        let current = app.book.sheets[s].tables[i].style.clone();
        // A row of nearby styles, and the whole gallery behind the menu.
        let names = sheet_engine::tables::style_names();
        let at = names.iter().position(|n| *n == current).unwrap_or(22);
        let from = at.saturating_sub(at % 7);
        crate::theme::vcol(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for n in names.iter().skip(from).take(7) {
                    if crate::filter_ui::style_swatch(app, ui, n, *n == current) { app.checkpoint("Table Style"); app.book.sheets[s].tables[i].style = n.clone(); app.changed(); }
                }
            });
            let r = small_menu(ui, "tableStyles", "More Styles", "All table styles");
            crate::theme::menu(&r, |ui| {
                if let Some(n) = crate::filter_ui::style_gallery(app, ui, &current) { app.checkpoint("Table Style"); app.book.sheets[s].tables[i].style = n; app.changed(); ui.close(); }
                ui.separator();
                if menu_item(ui, "clear", "Clear") { app.checkpoint("Table Style"); app.book.sheets[s].tables[i].style.clear(); app.changed(); ui.close(); }
            });
        });
    });
}
