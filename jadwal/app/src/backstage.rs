//! File ▸ New from a template (templates.js), File ▸ Info (document properties, protection and
//! Inspect Workbook) and the extra export formats (CSV UTF-8, OpenDocument).

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{vec2, RichText, Sense, Stroke};
use qalam_kit::colors::*;
use sheet_engine::xlsx::xml::{attr, text, Element};

#[derive(Default)]
pub struct Backstage { pub templates: bool, pub info: Option<Vec<(String, String)>>, pub inspect: Option<Vec<(usize, bool)>> }

thread_local! { static ST: std::cell::RefCell<Backstage> = Default::default(); }

pub fn open_templates(_app: &mut SheetApp) { ST.with(|s| s.borrow_mut().templates = true); }
pub fn open_info(app: &mut SheetApp) { let p = core_props(&app.book); ST.with(|s| s.borrow_mut().info = Some(p)); }

const PROPS: [(&str, &str); 6] = [("dc:title", "Title"), ("dc:subject", "Subject"), ("dc:creator", "Author"), ("cp:keywords", "Tags"), ("cp:category", "Categories"), ("dc:description", "Comments")];

/// The document properties from docProps/core.xml.
pub fn core_props(book: &sheet_engine::Workbook) -> Vec<(String, String)> {
    let e = book.parts.get("docProps/core.xml").and_then(|b| Element::parse(&String::from_utf8_lossy(b)).ok());
    PROPS.iter().map(|(k, _)| (k.to_string(), e.as_ref().and_then(|e| e.elements().find(|c| c.name == *k)).map(|c| c.text()).unwrap_or_default())).collect()
}

pub fn set_core_props(book: &mut sheet_engine::Workbook, props: &[(String, String)]) {
    let old = book.parts.get("docProps/core.xml").and_then(|b| Element::parse(&String::from_utf8_lossy(b)).ok());
    let mut body = String::new();
    if let Some(e) = &old { for c in e.elements() { if !PROPS.iter().any(|(k, _)| *k == c.name) { body.push_str(&c.to_xml()); } } }
    else { let now = "2026-01-01T00:00:00Z"; body.push_str(&format!("<dcterms:created xsi:type=\"dcterms:W3CDTF\">{now}</dcterms:created>")); }
    for (k, v) in props { if !v.is_empty() { body.push_str(&format!("<{k}>{}</{k}>", text(v))); } }
    let x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"{}\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">{body}</cp:coreProperties>", attr("http://schemas.openxmlformats.org/package/2006/metadata/core-properties"));
    book.parts.insert("docProps/core.xml".into(), x.into_bytes());
}

/// Exports the active sheet or workbook in another format: "csv8" (UTF-8 with BOM), "ods".
pub fn export(app: &mut SheetApp, kind: &str) {
    app.commit(None);
    let stem = app.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Book1"));
    let (ext, filter) = match kind { "ods" => ("ods", "OpenDocument Spreadsheet"), _ => ("csv", "CSV UTF-8") };
    let Some(p) = rfd::FileDialog::new().set_file_name(format!("{stem}.{ext}")).add_filter(tr(filter), &[ext]).save_file() else { return };
    let r = match kind {
        "ods" => sheet_engine::save(&app.book, &p),
        _ => std::fs::write(&p, [&[0xEF, 0xBB, 0xBF][..], sheet_engine::csv::write(&app.book, app.sheet(), ',').as_bytes()].concat()).map_err(|e| e.to_string()),
    };
    match r { Ok(()) => app.notify(&tr("Exported")), Err(e) => app.error = Some(format!("{}\n{e}", tr("The file couldn't be saved."))) }
}

/// What Inspect Workbook looks for: (label, count, what Remove All does).
fn inspect_items(book: &sheet_engine::Workbook) -> Vec<(&'static str, usize)> {
    let notes: usize = book.sheets.iter().map(|s| s.notes.len()).sum();
    let threads: usize = book.sheets.iter().map(|s| s.review.threads.len()).sum();
    let props = core_props(book).iter().filter(|(_, v)| !v.is_empty()).count() + book.review.persons.len();
    let hidden_sheets = book.sheets.iter().filter(|s| s.hidden).count();
    let hidden_rc: usize = book.sheets.iter().map(|s| s.hidden_rows.len() + s.hidden_cols.len()).sum();
    let views = book.review.views.len();
    vec![("Comments and notes", notes + threads), ("Document properties and personal information", props), ("Hidden worksheets", hidden_sheets), ("Hidden rows and columns", hidden_rc), ("Custom views", views)]
}

fn remove(app: &mut SheetApp, k: usize) {
    app.checkpoint("Inspect Workbook");
    let b = &mut app.book;
    match k {
        0 => for s in &mut b.sheets { s.notes.clear(); s.review.threads.clear(); },
        1 => { set_core_props(b, &PROPS.iter().map(|(k, _)| (k.to_string(), String::new())).collect::<Vec<_>>()); b.review.persons.clear(); for s in &mut b.sheets { for t in s.review.threads.values_mut() { for c in &mut t.comments { c.person.clear(); } } } }
        2 => { if b.sheets.iter().any(|s| !s.hidden) { let mut i = 0; while i < b.sheets.len() { if b.sheets[i].hidden { let _ = b.delete_sheet(i); } else { i += 1; } } b.active = b.active.min(b.sheets.len() - 1); } }
        3 => for s in &mut b.sheets { s.hidden_rows.clear(); s.hidden_cols.clear(); },
        _ => b.review.views.clear(),
    }
    app.changed();
}

pub fn dialogs(app: &mut SheetApp, ctx: &egui::Context) {
    let (templates, info, inspect) = ST.with(|s| { let s = s.borrow(); (s.templates, s.info.clone(), s.inspect.clone()) });
    if templates {
        let mut open = true; let mut picked = None;
        crate::theme::dialog(ctx, "New").open(&mut open).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.label(RichText::new(tr("Start from a template")).color(c_ink2()));
            egui::Grid::new("templates").num_columns(3).spacing([10.0, 10.0]).show(ui, |ui| {
                for (k, (id, name, desc, icon)) in sheet_engine::templates::TEMPLATES.iter().enumerate() {
                    let (rect, r) = ui.allocate_exact_size(vec2(195.0, 110.0), Sense::click());
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr(name)));
                    let p = ui.painter();
                    p.rect_filled(rect, 6.0, if r.hovered() { c_hover() } else { c_card() });
                    p.rect_stroke(rect, 6.0, Stroke::new(1.0, c_line()), egui::StrokeKind::Inside);
                    qalam_kit::widgets::paint_icon(ui, icon, egui::Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(24.0, 24.0)), 24, true);
                    p.text(rect.min + vec2(42.0, 14.0), egui::Align2::LEFT_TOP, tr(name), egui::FontId::proportional(14.0), c_ink());
                    let g = p.layout(tr(desc), egui::FontId::proportional(11.5), c_ink2(), rect.width() - 20.0);
                    p.galley(rect.min + vec2(10.0, 44.0), g, c_ink2());
                    if r.on_hover_text(tr(desc)).clicked() { picked = Some(*id); }
                    if k % 3 == 2 { ui.end_row(); }
                }
            });
        });
        if let Some(id) = picked { if let Some(wb) = sheet_engine::templates::build(id) { open = false; if !app.dirty { app.load(wb, None); } else { app.pending = Some(crate::filemenu::Pending::New); ST.with(|s| s.borrow_mut().templates = false); PENDING.with(|p| *p.borrow_mut() = Some(id)); return; } } }
        if !open { ST.with(|s| s.borrow_mut().templates = false); }
    }
    // A template chosen while there were unsaved changes opens once the guard has run.
    if app.pending.is_none() { if let Some(id) = PENDING.with(|p| p.borrow_mut().take()) { if !app.dirty { if let Some(wb) = sheet_engine::templates::build(id) { app.load(wb, None); } } } }
    if let Some(mut props) = info {
        let mut open = true; let mut keep = true;
        crate::theme::dialog(ctx, "Info").open(&mut open).show(ctx, |ui| {
            ui.set_width(460.0);
            let name = app.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Not saved yet"));
            ui.label(RichText::new(name).color(c_ink2()));
            crate::theme::section(ui, &tr("Properties"));
            egui::Grid::new("props").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                for (k, v) in props.iter_mut() { ui.label(tr(PROPS.iter().find(|p| p.0 == k).map(|p| p.1).unwrap_or(""))); ui.add(egui::TextEdit::singleline(v).desired_width(320.0)); ui.end_row(); }
                let st = sheet_engine::review::statistics(&app.book, app.sheet());
                ui.label(tr("Sheets")); ui.label(st.sheets.to_string()); ui.end_row();
                ui.label(tr("Cells with data")); ui.label(st.book_cells.to_string()); ui.end_row();
            });
            crate::theme::section(ui, &tr("Protect Workbook"));
            let s = app.sheet();
            let sheet_p = app.book.sheets[s].protected; let book_p = app.book.review.structure;
            ui.label(RichText::new(match (sheet_p, book_p) { (false, false) => tr("Anyone can open, copy and change any part of this workbook."), (true, false) => tr("This sheet is protected."), (false, true) => tr("The workbook's structure is locked."), _ => tr("This sheet is protected and the workbook's structure is locked.") }).color(c_ink2()));
            crate::theme::hrow(ui, |ui| {
                if ui.button(tr(if sheet_p { "Unprotect Current Sheet" } else { "Protect Current Sheet" })).clicked() { app.tab = crate::app::Tab::Review; if sheet_p { app.rv.unprotect = Some((false, String::new(), None)); } else { app.rv.protect = Some(crate::review_tab::ProtectDraft { password: String::new(), confirm: String::new(), allow: sheet_engine::review::SheetProtection::default().allow, error: None }); } keep = false; }
                if ui.button(tr(if book_p { "Unprotect Workbook Structure" } else { "Protect Workbook Structure" })).clicked() { if book_p { app.rv.unprotect = Some((true, String::new(), None)); } else { app.rv.protect_book = Some((String::new(), String::new(), None)); } keep = false; }
            });
            crate::theme::section(ui, &tr("Inspect Workbook"));
            ui.label(RichText::new(tr("Before sharing, check for hidden content and personal information.")).color(c_ink2()));
            if ui.button(tr("Check for Issues…")).clicked() { let v = inspect_items(&app.book).iter().enumerate().map(|(i, _)| (i, true)).collect(); ST.with(|s| s.borrow_mut().inspect = Some(v)); }
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "OK").clicked() { if props != core_props(&app.book) { set_core_props(&mut app.book, &props); app.dirty = true; } keep = false; }
                if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
            });
        });
        ST.with(|s| s.borrow_mut().info = (open && keep).then_some(props));
    }
    if let Some(mut sel) = inspect {
        let mut open = true;
        let items = inspect_items(&app.book);
        crate::theme::dialog(ctx, "Document Inspector").open(&mut open).show(ctx, |ui| {
            ui.set_width(420.0);
            for (i, (label, n)) in items.iter().enumerate() {
                crate::theme::hrow(ui, |ui| {
                    crate::theme::check(ui, &mut sel[i].1, tr(label));
                    ui.with_layout(qalam_kit::widgets::far_layout(), |ui| {
                        if *n > 0 { if ui.button(tr("Remove All")).clicked() { remove(app, i); } ui.label(RichText::new(n.to_string()).color(c_ink2())); } else { ui.label(RichText::new(tr("None found")).color(c_ink3())); }
                    });
                });
            }
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Close").clicked() { ST.with(|s| s.borrow_mut().inspect = None); });
        });
        ST.with(|s| { let mut s = s.borrow_mut(); if !open { s.inspect = None; } else if s.inspect.is_some() { s.inspect = Some(sel); } });
    }
}

thread_local! { static PENDING: std::cell::RefCell<Option<&'static str>> = const { std::cell::RefCell::new(None) }; }

/// Which File-menu windows are open: (New from Template, Info).
#[cfg(test)]
pub fn shown() -> (bool, bool) { ST.with(|s| { let s = s.borrow(); (s.templates, s.info.is_some()) }) }
#[cfg(test)]
pub fn reset() { ST.with(|s| *s.borrow_mut() = Default::default()); }
