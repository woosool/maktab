//! Dialogs (ui/formatcells.js, findbar.js, sortdialog.js, names.js, insertfunction.js…):
//! Format Cells, Find and Replace, Go To, row height and column width, rename sheet, Sort,
//! Insert and Delete cells, Paste Special, the Name Manager, notes, links and Insert Function.

use crate::app::{Dialog, Selection, SheetApp};
use crate::i18n::tr;
use egui::{vec2, RichText, Ui};
use qalam_kit::colors::*;
use sheet_engine::addr::{col_name, Pos, Range};
use sheet_engine::book::{FindOptions, Paste, PasteWhat};
use sheet_engine::style::{Border, HAlign, LineStyle, Style, Underline, VAlign};

pub fn function_help(name: &str) -> Option<(String, &'static str)> {
    crate::fnhelp::SIGNATURES.binary_search_by(|x| x.0.cmp(name)).ok().map(|i| { let s = crate::fnhelp::SIGNATURES[i]; (format!("{}({})", s.0, s.1), s.2) })
}

/// The implemented functions of a Function Library category ("" = the ones in no other list).
pub fn functions_in(cat: &str) -> Vec<&'static str> {
    let all = sheet_engine::formula::functions::names();
    let key = match cat { "financial" => "Financial", "logical" => "Logical", "text" => "Text", "date" => "Date & Time", "lookup" => "Lookup & Reference", "math" => "Math & Trig", other => other };
    if cat.is_empty() {
        let listed: std::collections::HashSet<&str> = crate::fnhelp::CATEGORIES.iter().filter(|c| matches!(c.0, "Financial" | "Logical" | "Text" | "Date & Time" | "Lookup & Reference" | "Math & Trig")).flat_map(|c| c.1.iter().copied()).collect();
        return all.into_iter().filter(|n| !listed.contains(n)).collect();
    }
    let list = crate::fnhelp::CATEGORIES.iter().find(|c| c.0 == key).map(|c| c.1).unwrap_or(&[]);
    all.into_iter().filter(|n| list.contains(n)).collect()
}

pub fn open_find(app: &mut SheetApp, replace: bool) {
    let query = match &app.dialog { Some(Dialog::Find { query, .. }) => query.clone(), _ => String::new() };
    app.dialog = Some(Dialog::Find { query, with: String::new(), match_case: false, whole: false, in_formulas: false, replace, last: None, all_sheets: false });
}

pub fn open_sort(app: &mut SheetApp) {
    let s = app.sheet();
    if app.sel.range().is_cell() { let r = app.book.current_region(s, app.sel.active); app.sel = Selection { anchor: r.start(), active: r.end(), extra: vec![] }; }
    let header = app.book.looks_like_header(s, app.sel.range());
    app.dialog = Some(Dialog::Sort { keys: vec![(app.sel.range().c0, true)], header });
}

#[derive(Clone)]
pub struct FormatCells { pub tab: usize, pub orig: Style, pub st: Style, pub custom: String, pub cat: usize }

pub fn open_format_cells(app: &mut SheetApp, tab: usize) {
    app.commit(None);
    let st = app.style();
    let cat = sheet_engine::numfmt::CATEGORIES.iter().position(|(_, fs)| fs.contains(&st.num_fmt.as_str())).unwrap_or(if st.num_fmt == "General" { 0 } else { 11 });
    app.dialog = Some(Dialog::FormatCells(FormatCells { tab, custom: st.num_fmt.clone(), orig: st.clone(), st, cat }));
}

pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let mut open = true;
    let keep = match &mut d {
        Dialog::FormatCells(f) => format_cells(app, ctx, f, &mut open),
        Dialog::Find { .. } => find(app, ctx, &mut d, &mut open),
        Dialog::GoTo { text } => {
            let mut keep = true;
            crate::theme::dialog(ctx, "Go To").open(&mut open).resizable(false).show(ctx, |ui| {
                ui.set_min_width(300.0);
                if !app.book.names.is_empty() {
                    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| for n in &app.book.names { if ui.selectable_label(text == &n.name, &n.name).clicked() { *text = n.name.clone(); } });
                }
                ui.label(tr("Reference:"));
                let r = ui.text_edit_singleline(text);
                r.request_focus();
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) { let t = text.clone(); if crate::formulabar::go_to(app, &t) { keep = false; } }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
            keep
        }
        Dialog::Size { cols, value } => {
            let mut keep = true;
            let default = *value < 0.0;
            let title = if default { "Standard Width" } else if *cols { "Column Width" } else { "Row Height" };
            crate::theme::dialog(ctx, title).open(&mut open).resizable(false).show(ctx, |ui| {
                // Widths are shown in characters and heights in points, as Excel does.
                let (unit, mut shown) = if *cols { ("characters", ((value.abs() - 5.0) / 7.0 * 100.0).round() / 100.0) } else { ("points", (*value * 0.75 * 100.0).round() / 100.0) };
                crate::theme::hrow(ui, |ui| { ui.label(tr(title)); if ui.add(egui::DragValue::new(&mut shown).range(0.0..=if *cols { 255.0 } else { 409.0 }).speed(0.25)).changed() { *value = if *cols { (shown * 7.0 + 5.0).round() * if default { -1.0 } else { 1.0 } } else { (shown / 0.75).round() }; } ui.label(tr(unit)); });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() {
                        let s = app.sheet(); let r = app.sel.range(); let v = value.abs();
                        app.checkpoint(if *cols { "Column Width" } else { "Row Height" });
                        let sh = &mut app.book.sheets[s];
                        if default { sh.default_col_width = v; }
                        else if *cols { for c in r.c0..=r.c1.min(r.c0 + 16_384) { if v <= 0.0 { sh.hidden_cols.insert(c); } else { sh.col_widths.insert(c, v); } } }
                        else { for rw in r.r0..=r.r1.min(r.r0 + 1_048_576) { if v <= 0.0 { sh.hidden_rows.insert(rw); } else { sh.row_heights.insert(rw, v); } } }
                        app.dirty = true;
                        keep = false;
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
            keep
        }
        Dialog::Rename { sheet, name } => {
            let mut keep = true;
            crate::theme::dialog(ctx, "Rename Sheet").open(&mut open).resizable(false).show(ctx, |ui| {
                let r = ui.text_edit_singleline(name);
                r.request_focus();
                if let Err(e) = app.book.valid_sheet_name(name, Some(*sheet)) { ui.label(RichText::new(tr(&e)).color(egui::Color32::from_rgb(0xC0, 0x20, 0x20)).size(12.0)); }
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        app.checkpoint("Rename Sheet");
                        match app.book.rename_sheet(*sheet, name) { Ok(()) => { app.changed(); keep = false; } Err(e) => { app.undo(); app.error = Some(tr(&e)); } }
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
            keep
        }
        Dialog::Sort { keys, header } => {
            let mut keep = true;
            crate::theme::dialog(ctx, "Sort").open(&mut open).resizable(false).show(ctx, |ui| {
                ui.set_min_width(380.0);
                let s = app.sheet(); let r = app.sel.range();
                let label = |c: u32| if *header { let t = app.book.display(s, Pos::new(r.r0, c)); if t.is_empty() { format!("{} {}", tr("Column"), col_name(c)) } else { t } } else { format!("{} {}", tr("Column"), col_name(c)) };
                let mut remove = None;
                for (i, (col, asc)) in keys.iter_mut().enumerate() {
                    crate::theme::hrow(ui, |ui| {
                        ui.label(tr(if i == 0 { "Sort by" } else { "Then by" }));
                        crate::theme::Combo::from_id_salt(("sort-col", i)).width(160.0).selected_text(label(*col)).show_ui(ui, |ui| for c in r.c0..=r.c1.min(r.c0 + 200) { ui.selectable_value(col, c, label(c)); });
                        crate::theme::Combo::from_id_salt(("sort-dir", i)).width(110.0).selected_text(tr(if *asc { "A to Z" } else { "Z to A" })).show_ui(ui, |ui| { ui.selectable_value(asc, true, tr("A to Z")); ui.selectable_value(asc, false, tr("Z to A")); });
                        if i > 0 && ui.small_button("✕").clicked() { remove = Some(i); }
                    });
                }
                if let Some(i) = remove { keys.remove(i); }
                crate::theme::hrow(ui, |ui| {
                    if ui.button(tr("Add Level")).clicked() && keys.len() < 64 { keys.push((r.c0, true)); }
                    crate::theme::check(ui, header, tr("My data has headers"));
                });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() { app.checkpoint("Sort"); app.book.sort(s, r, keys, *header, false); app.changed(); keep = false; }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
            keep
        }
        Dialog::Insert { delete, choice } => {
            let mut keep = true;
            crate::theme::dialog(ctx, if *delete { "Delete" } else { "Insert" }).open(&mut open).resizable(false).show(ctx, |ui| {
                let opts = if *delete { ["Shift cells left", "Shift cells up", "Entire row", "Entire column"] } else { ["Shift cells right", "Shift cells down", "Entire row", "Entire column"] };
                for (i, o) in opts.iter().enumerate() { crate::theme::radio_value(ui, choice, i as u8, tr(o)); }
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() {
                        match choice { 0 => app.shift_cells(false, *delete), 1 => app.shift_cells(true, *delete), 2 => app.insert_rows_cols(true, *delete), _ => app.insert_rows_cols(false, *delete) }
                        keep = false;
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
            keep
        }
        Dialog::PasteSpecial { how, op } => {
            let mut keep = true;
            crate::theme::dialog(ctx, "Paste Special").open(&mut open).resizable(false).show(ctx, |ui| {
                ui.set_min_width(360.0);
                crate::theme::section(ui, "Paste");
                for (w, l) in [(PasteWhat::All, "All"), (PasteWhat::Formulas, "Formulas"), (PasteWhat::Values, "Values"), (PasteWhat::Formats, "Formats"), (PasteWhat::NoBorders, "All except borders")] { crate::theme::radio_value(ui, &mut how.what, w, tr(l)); }
                crate::theme::check(ui, &mut how.widths, tr("Column widths"));
                crate::theme::section(ui, "Operation");
                crate::theme::hrow(ui, |ui| for (i, l) in ["None", "Add", "Subtract", "Multiply", "Divide"].iter().enumerate() { crate::theme::radio_value(ui, op, i as u8, tr(l)); });
                crate::theme::hrow(ui, |ui| { crate::theme::check(ui, &mut how.skip_blanks, tr("Skip blanks")); crate::theme::check(ui, &mut how.transpose, tr("Transpose")); });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() { let mut h = *how; h.op = match op { 1 => Some('+'), 2 => Some('-'), 3 => Some('*'), 4 => Some('/'), _ => None }; app.paste_with(h); keep = false; }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
            keep
        }
        Dialog::Names { .. } => { crate::formulas_tab::open_name_manager(app); false }
        Dialog::Note { pos, text } => {
            let mut keep = true;
            crate::theme::dialog(ctx, "Note").open(&mut open).resizable(false).show(ctx, |ui| {
                ui.label(RichText::new(format!("{} — {}", pos.a1(), app.prefs.author)).size(12.0).color(c_ink2()));
                ui.add(egui::TextEdit::multiline(text).desired_rows(5).desired_width(320.0));
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() { app.checkpoint("Note"); let s = app.sheet(); if text.trim().is_empty() { app.book.sheets[s].notes.remove(pos); } else { app.book.sheets[s].notes.insert(*pos, text.clone()); } app.dirty = true; keep = false; }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
            keep
        }
        Dialog::Link { pos, target } => {
            let mut keep = true;
            crate::theme::dialog(ctx, "Insert Link").open(&mut open).resizable(false).show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(tr("Address (a web page, an email, or #Sheet1!A1 for a place in this workbook):"));
                ui.add(egui::TextEdit::singleline(target).desired_width(360.0));
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() {
                        app.checkpoint("Link");
                        let s = app.sheet();
                        let t = target.trim().to_string();
                        if t.is_empty() { app.book.sheets[s].links.remove(pos); } else {
                            let t = if t.contains('@') && !t.contains(':') { format!("mailto:{t}") } else if !t.starts_with('#') && !t.contains(':') { format!("https://{t}") } else { t };
                            app.book.sheets[s].links.insert(*pos, sheet_engine::book::Hyperlink { target: t.clone(), tooltip: None });
                            if app.book.sheets[s].value(*pos).is_empty() { app.book.set_input(s, *pos, &format!("'{}", target.trim())); }
                            app.book.apply_style(s, &[Range::cell(*pos)], &|st| { st.underline = Underline::Single; st.color = Some([0x05, 0x63, 0xC1]); });
                            app.changed();
                        }
                        app.dirty = true;
                        keep = false;
                    }
                    if crate::theme::secondary(ui, "Remove Link").clicked() { app.checkpoint("Remove Link"); let s = app.sheet(); app.book.sheets[s].links.remove(pos); app.dirty = true; keep = false; }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
            keep
        }
        Dialog::Function { query, picked } => crate::formulas_tab::insert_function_dialog(app, ctx, query, picked, &mut open),
        Dialog::Series { .. } => crate::home_hooks::series_dialog(app, ctx, &mut d, &mut open),
        Dialog::About => {
            let mut keep = true;
            crate::theme::dialog(ctx, crate::i18n::trf("About {}", &[&crate::app_name()])).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                crate::theme::hrow(ui, |ui| {
                    if let Some(t) = crate::filemenu::icon(ui.ctx()) { ui.add(egui::Image::new((t.id(), vec2(80.0, 80.0)))); }
                    crate::theme::vcol(ui, |ui| {
                        ui.label(RichText::new("Jadwal  جدول").size(22.0).family(egui::FontFamily::Name("ui-bold".into())));
                        ui.label(crate::i18n::trf("Version {}", &[&crate::update::CURRENT]));
                        ui.label(tr("A free desktop spreadsheet app for creating and editing .xlsx workbooks offline."));
                    });
                });
                ui.add_space(8.0);
                crate::theme::hrow(ui, |ui| { ui.label(tr("Made by")); ui.hyperlink_to("Woosool", "https://woosool.com"); });
                ui.hyperlink_to(tr("Jadwal on woosool.com"), "https://woosool.com/en/products/jadwal");
                if crate::theme::secondary(ui, "Check for Updates…").clicked() { crate::update::check(ui.ctx(), true, None); }
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Released under the MIT License.")).strong());
                ui.label(RichText::new("Copyright © 2026 Woosool").small());
                egui::CollapsingHeader::new(tr("License")).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| ui.label(RichText::new(include_str!("../../LICENSE")).small().monospace()));
                });
                ui.add_space(6.0);
                ui.weak(tr("Fonts: Noto Sans, Carlito, Caladea (OFL / Apache-2.0), Liberation (OFL)."));
                crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { keep = false; });
            });
            keep
        }
        Dialog::Shortcuts => {
            let mut keep = true;
            crate::theme::dialog(ctx, "Keyboard Shortcuts").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                    crate::theme::grid(ui, egui::Grid::new("keys").num_columns(2).striped(true).spacing(vec2(16.0, 6.0)), |ui| {
                        for (k, what) in crate::filemenu::SHORTCUTS { ui.label(RichText::new(*k).strong()); ui.label(tr(what)); ui.end_row(); }
                    });
                });
                crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Close").clicked() { keep = false; });
            });
            keep
        }
    };
    if keep && open && app.dialog.is_none() { app.dialog = Some(d); }
}

fn find(app: &mut SheetApp, ctx: &egui::Context, d: &mut Dialog, open: &mut bool) -> bool {
    let Dialog::Find { query, with, match_case, whole, in_formulas, replace, last, all_sheets } = d else { return false };
    let mut keep = true;
    crate::theme::dialog(ctx, if *replace { "Find and Replace" } else { "Find" }).open(open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(400.0);
        crate::theme::hrow(ui, |ui| { ui.selectable_value(replace, false, tr("Find")); ui.selectable_value(replace, true, tr("Replace")); });
        crate::theme::grid(ui, egui::Grid::new("find").num_columns(2).spacing(vec2(8.0, 6.0)), |ui| {
            ui.label(tr("Find what:")); let r = ui.add(egui::TextEdit::singleline(query).desired_width(280.0)); if !ui.memory(|m| m.focused().is_some()) { r.request_focus(); } ui.end_row();
            if *replace { ui.label(tr("Replace with:")); ui.add(egui::TextEdit::singleline(with).desired_width(280.0)); ui.end_row(); }
        });
        crate::theme::hrow(ui, |ui| { crate::theme::check(ui, match_case, tr("Match case")); crate::theme::check(ui, whole, tr("Match entire cell contents")); });
        crate::theme::hrow(ui, |ui| { crate::theme::check(ui, in_formulas, tr("Look in formulas")); crate::theme::check(ui, all_sheets, tr("Search the whole workbook")); });
        let opts = FindOptions { match_case: *match_case, whole: *whole, in_formulas: *in_formulas || *replace };
        crate::theme::footer(ui, |ui| {
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if crate::theme::primary(ui, "Find Next").clicked() || enter {
                let s = app.sheet();
                let mut hit = app.book.find(s, query, *last, opts).map(|p| (s, p));
                if *all_sheets && hit.is_none_or(|(_, p)| last.is_some_and(|l| p <= l)) {
                    let n = app.book.sheets.len();
                    hit = (1..=n).map(|k| (s + k) % n).find_map(|i| app.book.find(i, query, None, opts).map(|p| (i, p))).or(hit);
                }
                match hit { Some((si, p)) => { if si != app.sheet() { app.set_sheet(si); } app.select(p, false); *last = Some(p); } None => app.notify(&tr("No matches were found.")) }
            }
            if *replace {
                if crate::theme::secondary(ui, "Replace").clicked() {
                    let s = app.sheet(); let p = app.sel.active;
                    let t = app.book.edit_text(s, p);
                    let hit = if *match_case { t.contains(query.as_str()) } else { t.to_lowercase().contains(&query.to_lowercase()) };
                    if hit && !query.is_empty() {
                        app.checkpoint("Replace");
                        let new = if *whole { with.clone() } else if *match_case { t.replace(query.as_str(), with) } else { replace_ci(&t, query, with) };
                        app.book.set_input(s, p, &new);
                        app.changed();
                    }
                    *last = Some(p);
                    if let Some(n) = app.book.find(s, query, Some(p), opts) { app.select(n, false); }
                }
                if crate::theme::secondary(ui, "Replace All").clicked() {
                    app.checkpoint("Replace All");
                    let sheets: Vec<usize> = if *all_sheets { (0..app.book.sheets.len()).collect() } else { vec![app.sheet()] };
                    let n: usize = sheets.into_iter().map(|s| app.book.replace_all(s, query, with, opts)).sum();
                    app.changed();
                    app.notify(&crate::i18n::trf("All done. {} replacements were made.", &[&n]));
                }
            }
            if crate::theme::secondary(ui, "Close").clicked() { keep = false; }
        });
    });
    keep
}

fn replace_ci(t: &str, q: &str, with: &str) -> String {
    let lt = t.to_lowercase(); let lq = q.to_lowercase();
    if lt.len() != t.len() || lq.is_empty() { return t.replace(q, with); }
    let mut out = String::new(); let mut i = 0;
    while let Some(k) = lt[i..].find(&lq) { out.push_str(&t[i..i + k]); out.push_str(with); i += k + lq.len(); }
    out.push_str(&t[i..]);
    out
}

fn format_cells(app: &mut SheetApp, ctx: &egui::Context, f: &mut FormatCells, open: &mut bool) -> bool {
    let mut keep = true;
    crate::theme::dialog(ctx, "Format Cells").open(open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(520.0);
        ui.set_min_height(360.0);
        crate::theme::hrow(ui, |ui| for (i, t) in ["Number", "Alignment", "Font", "Border", "Fill", "Protection"].iter().enumerate() { ui.selectable_value(&mut f.tab, i, tr(t)); });
        ui.separator();
        let st = &mut f.st;
        match f.tab {
            0 => {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(130.0);
                        ui.label(tr("Category:"));
                        for (i, (name, fmts)) in sheet_engine::numfmt::CATEGORIES.iter().enumerate() { if ui.selectable_label(f.cat == i, tr(name)).clicked() { f.cat = i; st.num_fmt = fmts[0].to_string(); f.custom = st.num_fmt.clone(); } }
                        if ui.selectable_label(f.cat == 11, tr("Custom")).clicked() { f.cat = 11; }
                    });
                    ui.vertical(|ui| {
                        let v = app.book.sheets[app.sheet()].value(app.sel.active);
                        let sample = sheet_engine::numfmt::format(&if v.is_empty() { sheet_engine::Value::Num(1234.5) } else { v.clone() }, &st.num_fmt, app.book.date1904).text;
                        ui.label(RichText::new(tr("Sample")).size(12.0).color(c_ink2()));
                        ui.label(RichText::new(sample).size(14.0));
                        ui.add_space(6.0);
                        let fmts: Vec<String> = if f.cat < sheet_engine::numfmt::CATEGORIES.len() { sheet_engine::numfmt::CATEGORIES[f.cat].1.iter().map(|s| s.to_string()).collect() } else { sheet_engine::numfmt::CATEGORIES.iter().flat_map(|c| c.1.iter().map(|s| s.to_string())).collect() };
                        if f.cat == 11 { ui.label(tr("Type:")); if ui.add(egui::TextEdit::singleline(&mut f.custom).desired_width(320.0)).changed() { st.num_fmt = f.custom.clone(); } }
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            ui.set_min_width(320.0);
                            for fm in fmts {
                                let shown = sheet_engine::numfmt::format(&if v.is_empty() { sheet_engine::Value::Num(-1234.5) } else { v.clone() }, &fm, app.book.date1904);
                                if ui.selectable_label(st.num_fmt == fm, format!("{}    {}", shown.text, if f.cat >= 10 { fm.clone() } else { String::new() })).clicked() { st.num_fmt = fm.clone(); f.custom = fm; }
                            }
                        });
                    });
                });
            }
            1 => {
                crate::theme::grid(ui, egui::Grid::new("fc-align").num_columns(2).spacing(vec2(12.0, 8.0)), |ui| {
                    ui.label(tr("Horizontal:"));
                    let hs = [(HAlign::General, "General"), (HAlign::Left, "Left (Indent)"), (HAlign::Center, "Center"), (HAlign::Right, "Right (Indent)"), (HAlign::Fill, "Fill"), (HAlign::Justify, "Justify"), (HAlign::CenterAcross, "Center Across Selection"), (HAlign::Distributed, "Distributed (Indent)")];
                    crate::theme::Combo::from_id_salt("fc-h").width(200.0).selected_text(tr(hs.iter().find(|x| x.0 == st.h).map_or("General", |x| x.1))).show_ui(ui, |ui| for (h, l) in hs { ui.selectable_value(&mut st.h, h, tr(l)); });
                    ui.end_row();
                    ui.label(tr("Vertical:"));
                    let vs = [(VAlign::Top, "Top"), (VAlign::Center, "Center"), (VAlign::Bottom, "Bottom"), (VAlign::Justify, "Justify"), (VAlign::Distributed, "Distributed")];
                    crate::theme::Combo::from_id_salt("fc-v").width(200.0).selected_text(tr(vs.iter().find(|x| x.0 == st.v).map_or("Bottom", |x| x.1))).show_ui(ui, |ui| for (v, l) in vs { ui.selectable_value(&mut st.v, v, tr(l)); });
                    ui.end_row();
                    ui.label(tr("Indent:")); ui.add(egui::DragValue::new(&mut st.indent).range(0..=15)); ui.end_row();
                    ui.label(tr("Orientation:")); ui.add(egui::DragValue::new(&mut st.rotation).range(-90..=90).suffix("°")); ui.end_row();
                });
                crate::theme::check(ui, &mut st.wrap, tr("Wrap text"));
                crate::theme::check(ui, &mut st.shrink, tr("Shrink to fit"));
                let mut rtl = st.reading == 2;
                if crate::theme::check(ui, &mut rtl, tr("Right-to-left reading order")).changed() { st.reading = if rtl { 2 } else { 0 }; }
            }
            2 => {
                crate::theme::grid(ui, egui::Grid::new("fc-font").num_columns(2).spacing(vec2(12.0, 8.0)), |ui| {
                    ui.label(tr("Font:"));
                    crate::theme::Combo::from_id_salt("fc-fontname").width(220.0).height(320.0).selected_text(st.font.clone()).show_ui(ui, |ui| for n in word_engine::fonts::family_names() { if ui.selectable_label(st.font == n, &n).clicked() { st.font = n; } });
                    ui.end_row();
                    ui.label(tr("Size:")); let mut sz = st.size_pt(); if ui.add(egui::DragValue::new(&mut sz).range(1.0..=409.0).speed(0.5)).changed() { st.size = (sz * 10.0).round() as u32; } ui.end_row();
                    ui.label(tr("Style:")); ui.horizontal(|ui| { crate::theme::check(ui, &mut st.bold, tr("Bold")); crate::theme::check(ui, &mut st.italic, tr("Italic")); }); ui.end_row();
                    ui.label(tr("Underline:"));
                    let us = [(Underline::None, "None"), (Underline::Single, "Single"), (Underline::Double, "Double"), (Underline::SingleAccounting, "Single Accounting"), (Underline::DoubleAccounting, "Double Accounting")];
                    crate::theme::Combo::from_id_salt("fc-u").width(200.0).selected_text(tr(us.iter().find(|x| x.0 == st.underline).map_or("None", |x| x.1))).show_ui(ui, |ui| for (u, l) in us { ui.selectable_value(&mut st.underline, u, tr(l)); });
                    ui.end_row();
                    ui.label(tr("Color:")); let mut c = st.color.map(|c| word_engine::Color(c[0], c[1], c[2])).unwrap_or(word_engine::Color(0, 0, 0)); if qalam_kit::widgets::color_button(ui, &mut c) { st.color = Some([c.0, c.1, c.2]); } ui.end_row();
                    ui.label(tr("Effects:")); ui.horizontal(|ui| { crate::theme::check(ui, &mut st.strike, tr("Strikethrough")); let mut sup = st.script == 1; if crate::theme::check(ui, &mut sup, tr("Superscript")).changed() { st.script = if sup { 1 } else { 0 }; } let mut sub = st.script == -1; if crate::theme::check(ui, &mut sub, tr("Subscript")).changed() { st.script = if sub { -1 } else { 0 }; } }); ui.end_row();
                });
                let preview = egui::RichText::new("AaBbCcYyZz").size(st.size_pt() * 1.33).family(crate::grid::font_for(st, 12.0).family);
                ui.add_space(8.0);
                ui.label(preview);
            }
            3 => {
                let line_id = egui::Id::new("fc-line");
                let mut line: LineStyle = ui.data(|d| d.get_temp(line_id)).unwrap_or(LineStyle::Thin);
                let color_id = egui::Id::new("fc-bcolor");
                let mut color: word_engine::Color = ui.data(|d| d.get_temp(color_id)).unwrap_or(word_engine::Color(0, 0, 0));
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label(tr("Line style:"));
                        for (l, _) in LineStyle::ALL.iter().skip(1) { if ui.selectable_label(line == *l, l.name()).clicked() { line = *l; } }
                        ui.label(tr("Color:")); qalam_kit::widgets::color_button(ui, &mut color);
                    });
                    ui.vertical(|ui| {
                        let b = Border { style: line, color: Some([color.0, color.1, color.2]) };
                        let none = Border::default();
                        crate::theme::hrow(ui, |ui| {
                            if ui.button(tr("None")).clicked() { st.top = none; st.bottom = none; st.left = none; st.right = none; st.diag_down = false; st.diag_up = false; }
                            if ui.button(tr("Outline")).clicked() { st.top = b; st.bottom = b; st.left = b; st.right = b; }
                        });
                        for (label, side) in [("Top", 0), ("Bottom", 1), ("Left", 2), ("Right", 3)] {
                            let cur = match side { 0 => &mut st.top, 1 => &mut st.bottom, 2 => &mut st.left, _ => &mut st.right };
                            let mut on = cur.style != LineStyle::None;
                            if crate::theme::check(ui, &mut on, tr(label)).changed() { *cur = if on { b } else { none }; }
                        }
                        if crate::theme::check(ui, &mut st.diag_down, tr("Diagonal down")).changed() || crate::theme::check(ui, &mut st.diag_up, tr("Diagonal up")).changed() { st.diagonal = b; }
                    });
                });
                ui.data_mut(|d| { d.insert_temp(line_id, line); d.insert_temp(color_id, color); });
            }
            4 => {
                ui.label(tr("Background color:"));
                let mut picked = None;
                qalam_kit::widgets::swatch_row(ui, &qalam_kit::widgets::PALETTE.iter().map(|c| word_engine::Color(c[0], c[1], c[2])).collect::<Vec<_>>(), &mut picked);
                if let Some(c) = picked { st.fill = c.map(|c| [c.0, c.1, c.2]); }
                if ui.button(tr("No Color")).clicked() { st.fill = None; }
                if let Some(c) = st.fill { let (r, _) = ui.allocate_exact_size(vec2(200.0, 40.0), egui::Sense::hover()); ui.painter().rect_filled(r, 2.0, crate::app::rgb(c)); }
            }
            _ => {
                crate::theme::check(ui, &mut st.locked, tr("Locked"));
                crate::theme::check(ui, &mut st.hidden, tr("Hidden"));
                ui.label(RichText::new(tr("Locking cells or hiding formulas has no effect until you protect the sheet.")).size(12.0).color(c_ink2()));
            }
        }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                // Only what changed applies, so a mixed selection keeps the rest of its formats.
                let (o, n) = (f.orig.clone(), f.st.clone());
                app.apply_style("Format Cells", move |s| apply_diff(s, &o, &n));
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep
}

fn apply_diff(s: &mut Style, o: &Style, n: &Style) {
    macro_rules! d { ($($f:ident),*) => { $( if o.$f != n.$f { s.$f = n.$f.clone(); } )* }; }
    d!(font, size, bold, italic, underline, strike, color, script, fill, h, v, wrap, shrink, indent, rotation, num_fmt, left, right, top, bottom, diag_up, diag_down, diagonal, locked, hidden, reading);
}

#[allow(dead_code)]
fn unused(_: &mut Ui) {}
