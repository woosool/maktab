//! Printing (ui/print.js, printdialog.js, print-preview.js): File ▸ Print… with a page preview,
//! the printer, copies, what to print and the common page setup choices; File ▸ Export ▸ PDF;
//! and the dashed page-break lines the grid shows once a sheet has been printed or previewed.

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Stroke, TextureHandle};
use qalam_kit::colors::*;
use sheet_engine::print::{self, JobInfo, Margins, PrintPage, What, PAPERS};
use std::collections::BTreeSet;

/// The print state kept on the app: the open dialogs and the grid's page-break lines.
#[derive(Default)]
pub struct PrintUi {
    pub dialog: Option<PrintDialog>,
    pub setup: Option<crate::page_layout::SetupDialog>,
    /// Show page breaks on the grid (after printing or previewing, or from the ribbon).
    pub show_breaks: bool,
    breaks: Option<(String, BTreeSet<u32>, BTreeSet<u32>)>,
}

pub struct PrintDialog {
    /// 0 = active sheet, 1 = entire workbook, 2 = selection.
    pub what: u8,
    pub printer: String,
    pub printers: Vec<String>,
    pub copies: u32,
    pub page: usize,
    preview: Option<(String, TextureHandle)>,
}

pub fn open(app: &mut SheetApp) {
    app.commit(None);
    let printers = printers();
    let printer = default_printer().filter(|d| printers.contains(d)).or_else(|| printers.first().cloned()).unwrap_or_default();
    app.printing.dialog = Some(PrintDialog { what: 0, printer, printers, copies: 1, page: 0, preview: None });
    app.printing.show_breaks = true;
}

fn printers() -> Vec<String> {
    if cfg!(windows) { return vec![]; }
    let Ok(o) = std::process::Command::new("lpstat").arg("-a").output() else { return vec![] };
    String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.split_whitespace().next().map(String::from)).collect()
}

fn default_printer() -> Option<String> {
    if cfg!(windows) { return None; }
    let o = std::process::Command::new("lpstat").arg("-d").output().ok()?;
    String::from_utf8_lossy(&o.stdout).lines().find_map(|l| l.split(':').nth(1).map(|s| s.trim().to_string())).filter(|s| !s.is_empty())
}

fn what(app: &SheetApp, w: u8) -> What {
    match w { 1 => What::Workbook, 2 => What::Selection(app.sel.ranges()), _ => What::ActiveSheet }
}

pub fn job_info(app: &SheetApp) -> JobInfo {
    let name = app.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Book1"));
    let folder = app.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    JobInfo { file_name: name, folder, now: None }
}

fn title(app: &SheetApp) -> String {
    app.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Book1"))
}

/// File ▸ Export ▸ PDF: the active sheet (or the workbook) as a PDF file.
pub fn export_pdf(app: &mut SheetApp, w: What) {
    app.commit(None);
    let pages = print::pages(&app.book, &w);
    if pages.is_empty() { app.error = Some(tr("There's nothing to print.")); return; }
    let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{}.pdf", title(app))).save_file() else { return };
    let bytes = print::pdf(&app.book, &pages, &job_info(app), &title(app), 1);
    match std::fs::write(&path, bytes) { Ok(()) => app.notify(&tr("Exported")), Err(e) => app.error = Some(e.to_string()) }
    app.printing.show_breaks = true;
}

/// Sends a PDF to a printer (lp on macOS and Linux; Windows prints it with the PDF app).
fn send(app: &mut SheetApp, file: &std::path::Path, printer: &str, copies: u32) {
    #[cfg(windows)]
    {
        let _ = copies;
        let script = format!("Start-Process -FilePath '{}' -Verb PrintTo -ArgumentList '\"{}\"' -WindowStyle Hidden", file.display().to_string().replace('\'', "''"), printer.replace('\'', "''"));
        match std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).output() {
            Ok(o) if o.status.success() => app.notify(&tr("Sent to the printer")),
            Ok(o) => app.error = Some(crate::i18n::trf("Couldn't print.\n\n{}", &[&String::from_utf8_lossy(&o.stderr)])),
            Err(e) => app.error = Some(crate::i18n::trf("Couldn't print.\n\n{}", &[&e])),
        }
    }
    #[cfg(not(windows))]
    {
        let mut c = std::process::Command::new("lp");
        if !printer.is_empty() { c.args(["-d", printer]); }
        c.args(["-n", &copies.max(1).to_string(), "-t", &title(app)]).arg(file);
        match c.output() {
            Ok(o) if o.status.success() => app.notify(&tr("Sent to the printer")),
            Ok(o) => app.error = Some(crate::i18n::trf("Couldn't print.\n\n{}", &[&String::from_utf8_lossy(&o.stderr)])),
            Err(e) => app.error = Some(crate::i18n::trf("Couldn't print.\n\n{}", &[&e])),
        }
    }
}

/// One page rendered for the preview.
fn preview_texture(ctx: &egui::Context, app: &SheetApp, page: &PrintPage) -> Option<TextureHandle> {
    let (lay, doc) = print::render(&app.book, std::slice::from_ref(page), &job_info(app));
    let svg = word_engine::svg::page_svg(&doc, &lay, 0);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = 900.0 / size.width().max(size.height()).max(1.0);
    let (w, h) = ((size.width() * scale) as u32, (size.height() * scale) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let image = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], pixmap.data());
    Some(ctx.load_texture("sheet-print-preview", image, egui::TextureOptions::LINEAR))
}

/// Changes the active sheet's page setup as one undoable step.
pub fn edit_setup(app: &mut SheetApp, label: &'static str, f: impl FnOnce(&mut print::PageSetup)) {
    let s = app.sheet();
    let mut p = app.book.sheets[s].page.clone();
    f(&mut p);
    if p != app.book.sheets[s].page { app.checkpoint(label); app.book.sheets[s].page = p; app.dirty = true; }
}

pub fn dialogs(app: &mut SheetApp, ctx: &egui::Context) {
    crate::page_layout::setup_dialog(app, ctx);
    let Some(mut d) = app.printing.dialog.take() else { return };
    let mut keep = true;
    let mut open = true;
    let mut go: Option<bool> = None; // Some(false) = print, Some(true) = save PDF
    let w = what(app, d.what);
    let pages = print::pages(&app.book, &w);
    if d.page >= pages.len() { d.page = pages.len().saturating_sub(1); }
    let si = app.sheet();
    crate::theme::dialog(ctx, "Print").open(&mut open).default_width(860.0).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            // Settings.
            ui.vertical(|ui| {
                ui.set_width(300.0);
                egui::Grid::new("print-grid").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
                    ui.label(tr("Copies:"));
                    ui.add(egui::DragValue::new(&mut d.copies).range(1..=999));
                    ui.end_row();
                    ui.label(tr("Printer:"));
                    let shown = if d.printer.is_empty() { tr("Default printer") } else { d.printer.clone() };
                    crate::theme::Combo::from_id_salt("print-printer").width(200.0).selected_text(shown).show_ui(ui, |ui| {
                        if d.printers.is_empty() { ui.label(tr("No printers found")); }
                        for p in d.printers.clone() { ui.selectable_value(&mut d.printer, p.clone(), p); }
                    });
                    ui.end_row();
                    ui.label(tr("Print:"));
                    let labels = ["Print Active Sheets", "Print Entire Workbook", "Print Selection"];
                    crate::theme::Combo::from_id_salt("print-what").width(200.0).selected_text(tr(labels[d.what as usize])).show_ui(ui, |ui| {
                        for (i, l) in labels.iter().enumerate() { if ui.selectable_label(d.what == i as u8, tr(l)).clicked() { d.what = i as u8; d.page = 0; } }
                    });
                    ui.end_row();
                    let p = app.book.sheets[si].page.clone();
                    ui.label(tr("Orientation:"));
                    crate::theme::Combo::from_id_salt("print-orient").width(200.0).selected_text(tr(if p.landscape { "Landscape Orientation" } else { "Portrait Orientation" })).show_ui(ui, |ui| {
                        if ui.selectable_label(!p.landscape, tr("Portrait Orientation")).clicked() { edit_setup(app, "Orientation", |p| p.landscape = false); }
                        if ui.selectable_label(p.landscape, tr("Landscape Orientation")).clicked() { edit_setup(app, "Orientation", |p| p.landscape = true); }
                    });
                    ui.end_row();
                    ui.label(tr("Paper:"));
                    crate::theme::Combo::from_id_salt("print-paper").width(200.0).selected_text(tr(p.paper_name())).show_ui(ui, |ui| {
                        for (code, name, w, h) in PAPERS {
                            if ui.selectable_label(p.paper == *code, format!("{}  {:.2}\" × {:.2}\"", tr(name), w, h)).clicked() { let c = *code; edit_setup(app, "Size", |p| p.paper = c); }
                        }
                    });
                    ui.end_row();
                    ui.label(tr("Margins:"));
                    let mname = crate::page_layout::margins_name(&p.margins);
                    crate::theme::Combo::from_id_salt("print-margins").width(200.0).selected_text(tr(mname)).show_ui(ui, |ui| {
                        for (n, m) in [("Normal Margins", Margins::NORMAL), ("Wide Margins", Margins::WIDE), ("Narrow Margins", Margins::NARROW)] {
                            if ui.selectable_label(p.margins == m, tr(n)).clicked() { edit_setup(app, "Margins", |p| p.margins = m); }
                        }
                    });
                    ui.end_row();
                    ui.label(tr("Scaling:"));
                    let scaling = crate::page_layout::scaling_name(&p);
                    crate::theme::Combo::from_id_salt("print-scale").width(200.0).selected_text(tr(scaling)).show_ui(ui, |ui| {
                        for (i, n) in crate::page_layout::SCALINGS.iter().enumerate() {
                            if ui.selectable_label(scaling == *n, tr(n)).clicked() { edit_setup(app, "Scale to Fit", |p| crate::page_layout::apply_scaling(p, i)); }
                        }
                    });
                    ui.end_row();
                });
                ui.add_space(6.0);
                if ui.link(tr("Page Setup…")).clicked() { crate::page_layout::open_setup(app, 0); }
            });
            ui.separator();
            // Preview.
            ui.vertical_centered(|ui| {
                let area = vec2(500.0, 520.0);
                if pages.is_empty() {
                    ui.allocate_ui(area, |ui| { ui.add_space(240.0); ui.label(RichText::new(tr("There's nothing to print.")).color(c_ink2())); });
                } else {
                    let pg = &pages[d.page];
                    let key = format!("{:?}{:?}{:?}{:?}", pg, app.book.sheets[pg.sheet].page, app.book.print_area(pg.sheet), app.dark);
                    if d.preview.as_ref().is_none_or(|(k, _)| *k != key) {
                        d.preview = preview_texture(ctx, app, pg).map(|t| (key, t));
                    }
                    let (rect, _) = ui.allocate_exact_size(area, egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, c_frame());
                    if let Some((_, t)) = &d.preview {
                        let ts = t.size_vec2();
                        let s = ((area.x - 24.0) / ts.x).min((area.y - 24.0) / ts.y);
                        let r = Rect::from_center_size(rect.center(), ts * s);
                        ui.painter().rect_filled(r.translate(vec2(2.0, 3.0)), 0.0, Color32::from_black_alpha(40));
                        ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, c_line()), egui::StrokeKind::Outside);
                    }
                }
                ui.horizontal(|ui| {
                    ui.add_space(170.0);
                    if ui.add_enabled(d.page > 0, egui::Button::new("◀")).clicked() { d.page -= 1; }
                    ui.label(crate::i18n::trf("{} of {}", &[&(if pages.is_empty() { 0 } else { d.page + 1 }), &pages.len()]));
                    if ui.add_enabled(d.page + 1 < pages.len(), egui::Button::new("▶")).clicked() { d.page += 1; }
                });
            });
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, !pages.is_empty(), "Print").clicked() { go = Some(false); }
            if ui.add_enabled(!pages.is_empty(), egui::Button::new(tr("Save as PDF…"))).clicked() { go = Some(true); }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    if let Some(save) = go {
        let pages = print::pages(&app.book, &what(app, d.what));
        let info = job_info(app);
        if save {
            if let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{}.pdf", title(app))).save_file() {
                match std::fs::write(&path, print::pdf(&app.book, &pages, &info, &title(app), 1)) { Ok(()) => { app.notify(&tr("Exported")); keep = false; } Err(e) => app.error = Some(e.to_string()) }
            }
        } else {
            let file = std::env::temp_dir().join(format!("jadwal-print-{}.pdf", std::process::id()));
            // Windows prints one copy per job; repeat the pages for more.
            let copies = if cfg!(windows) { d.copies as usize } else { 1 };
            match std::fs::write(&file, print::pdf(&app.book, &pages, &info, &title(app), copies)) {
                Ok(()) => { send(app, &file, &d.printer.clone(), d.copies); keep = false; }
                Err(e) => app.error = Some(e.to_string()),
            }
        }
    }
    if keep && open { app.printing.dialog = Some(d); }
}

/// Dashed lines where pages break (solid for manual breaks), over the grid.
pub fn draw_breaks(app: &mut SheetApp, painter: &egui::Painter, lay: &crate::grid::Layout) {
    if !app.printing.show_breaks { return; }
    let si = app.sheet();
    let s = &app.book.sheets[si];
    let key = format!("{si}{:?}{:?}{:?}{:?}{}{}{}{}{}", s.page, app.book.print_area(si), app.book.print_titles(si), print::used_range(&app.book, si), s.col_widths.len(), s.row_heights.len(), s.hidden_rows.len(), s.hidden_cols.len(), s.default_row_height);
    if app.printing.breaks.as_ref().is_none_or(|b| b.0 != key) {
        let (r, c) = print::break_lines(&app.book, si);
        app.printing.breaks = Some((key, r, c));
    }
    let Some((_, rows, cols)) = &app.printing.breaks else { return };
    let s = &app.book.sheets[si];
    let view = lay.view();
    let color = Color32::from_rgb(0x44, 0x66, 0xAA);
    let line = |a: egui::Pos2, b: egui::Pos2, manual: bool| {
        if manual { painter.line_segment([a, b], Stroke::new(1.5, color)); return; }
        let len = a.distance(b);
        let dir = (b - a) / len.max(1.0);
        let mut t = 0.0;
        while t < len { painter.line_segment([a + dir * t, a + dir * (t + 4.0).min(len)], Stroke::new(1.0, color)); t += 7.0; }
    };
    for r in rows {
        let y = lay.y(*r);
        if y > view.min.y && y < view.max.y { line(pos2(view.min.x, y), pos2(view.max.x, y), s.page.row_breaks.contains(r)); }
    }
    for c in cols {
        let x = if s.right_to_left { lay.x(*c) + lay.cols.size(*c) * lay.z } else { lay.x(*c) };
        if x > view.min.x && x < view.max.x { line(pos2(x, view.min.y), pos2(x, view.max.y), s.page.col_breaks.contains(c)); }
    }
}
