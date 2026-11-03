//! Printing to real printers: File ▸ Print (⌘P) lists the system's printers, previews the
//! pages, and sends a PDF of the chosen pages to the print queue — CUPS (`lp`) on macOS and
//! Linux, the Windows print spooler via PowerShell — with copies, collation, two-sided
//! printing, pages per sheet and grayscale.

use crate::app::WordApp;
use crate::insert::Job;
use egui::{ComboBox, RichText, TextureHandle};
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub struct Printer { pub name: String, pub default: bool, pub status: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range { All, Current, Selection, Custom }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sides { One, LongEdge, ShortEdge }

#[derive(Clone, Debug, PartialEq)]
pub struct Settings { pub printer: String, pub copies: u32, pub collate: bool, pub range: Range, pub custom: String, pub sides: Sides, pub per_sheet: u32, pub gray: bool }

impl Default for Settings { fn default() -> Self { Settings { printer: String::new(), copies: 1, collate: true, range: Range::All, custom: String::new(), sides: Sides::One, per_sheet: 1, gray: false } } }

pub struct PrintState {
    pub settings: Settings,
    pub printers: Vec<Printer>,
    pub finding: Option<Job<Vec<Printer>>>,
    pub sending: Option<Job<String>>,
    pub message: Option<(String, bool)>,
    pub preview_page: usize,
    pub preview: Option<(u64, usize, TextureHandle)>,
}

/// Pages from "1-3, 5, 8-" (1-based, inclusive) as 0-based indices, in order, within `count`.
pub fn parse_ranges(s: &str, count: usize) -> Result<Vec<usize>, String> {
    let mut out = vec![];
    for part in s.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let a: usize = if a.is_empty() { 1 } else { a.parse().map_err(|_| format!("\"{part}\" isn't a page range."))? };
        let b: usize = if b.is_empty() { count } else { b.parse().map_err(|_| format!("\"{part}\" isn't a page range."))? };
        if a == 0 || a > b { return Err(format!("\"{part}\" isn't a page range.")); }
        for p in a..=b.min(count) { out.push(p - 1); }
    }
    if out.is_empty() { return Err(format!("The document has pages 1 to {count}.")); }
    Ok(out)
}

/// The `lp` arguments for a print job (CUPS).
pub fn lp_args(s: &Settings, title: &str, file: &str) -> Vec<String> {
    let mut a: Vec<String> = vec![];
    if !s.printer.is_empty() { a.extend(["-d".into(), s.printer.clone()]); }
    a.extend(["-n".into(), s.copies.max(1).to_string(), "-t".into(), title.to_string()]);
    if s.copies > 1 { a.extend(["-o".into(), format!("collate={}", s.collate)]); }
    a.extend(["-o".into(), match s.sides { Sides::One => "sides=one-sided", Sides::LongEdge => "sides=two-sided-long-edge", Sides::ShortEdge => "sides=two-sided-short-edge" }.into()]);
    if s.per_sheet > 1 { a.extend(["-o".into(), format!("number-up={}", s.per_sheet)]); }
    if s.gray { a.extend(["-o".into(), "print-color-mode=monochrome".into(), "-o".into(), "ColorModel=Gray".into()]); }
    a.push(file.to_string());
    a
}

fn run(cmd: &mut Command) -> Result<String, String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() { Ok(text) } else { Err(format!("{}{}", String::from_utf8_lossy(&out.stderr).trim(), if text.trim().is_empty() { String::new() } else { format!("\n{}", text.trim()) })) }
}

/// The system's printers.
pub fn list_printers() -> Result<Vec<Printer>, String> {
    if cfg!(target_os = "windows") {
        let out = run(Command::new("powershell").args(["-NoProfile", "-Command", "Get-CimInstance Win32_Printer | ForEach-Object { \"$($_.Name)|$($_.Default)|$($_.PrinterStatus)\" }"]))?;
        return Ok(out.lines().filter_map(|l| { let mut p = l.trim().split('|'); let name = p.next()?.to_string(); (!name.is_empty()).then(|| Printer { name, default: p.next() == Some("True"), status: match p.next() { Some("3") => crate::i18n::tr("Ready").into(), Some("4") => crate::i18n::tr("Printing").into(), Some("7") => crate::i18n::tr("Offline").into(), _ => String::new() } }) }).collect());
    }
    let out = run(Command::new("lpstat").args(["-p", "-d"])).or_else(|e| if e.contains("No destinations") { Ok(String::new()) } else { Err(e) })?;
    let default = out.lines().find_map(|l| l.strip_prefix("system default destination: ")).map(|s| s.trim().to_string());
    Ok(out.lines().filter_map(|l| {
        let rest = l.strip_prefix("printer ")?;
        let name = rest.split_whitespace().next()?.to_string();
        let status = if rest.contains("disabled") { crate::i18n::tr("Offline") } else if rest.contains("printing") { crate::i18n::tr("Printing") } else { crate::i18n::tr("Ready") }.to_string();
        Some(Printer { default: default.as_deref() == Some(&name), name, status })
    }).collect())
}

/// Send a PDF file to a printer. Returns the queue's response (a job id on CUPS).
pub fn send(s: &Settings, title: &str, file: &std::path::Path) -> Result<String, String> {
    if cfg!(target_os = "windows") {
        // The spooler prints PDFs through the registered PDF app's PrintTo verb.
        let script = format!("Start-Process -FilePath '{}' -Verb PrintTo -ArgumentList '\"{}\"' -WindowStyle Hidden", file.display().to_string().replace('\'', "''"), s.printer.replace('\'', "''"));
        run(Command::new("powershell").args(["-NoProfile", "-Command", &script]))?;
        return Ok(crate::i18n::trf("Sent to {}.", &[&s.printer]));
    }
    let args = lp_args(s, title, &file.display().to_string());
    let out = run(Command::new("lp").args(&args))?;
    Ok(out.trim().to_string())
}

impl WordApp {
    pub fn open_print(&mut self) {
        let mut st = PrintState { settings: Settings::default(), printers: vec![], finding: None, sending: None, message: None, preview_page: 0, preview: None };
        if let Some(prev) = self.extras.print_settings.clone() { st.settings = prev; }
        st.settings.range = if self.doc.range().0 != self.doc.range().1 && st.settings.range == Range::Selection { Range::Selection } else if st.settings.range == Range::Selection { Range::All } else { st.settings.range };
        st.finding = Some(Job::spawn(&self.ctx, list_printers));
        st.preview_page = self.current_page();
        self.extras.print = Some(st);
    }

    fn current_page(&self) -> usize { self.layout.page_of(self.doc.range().0) }

    /// The 0-based pages a print job covers.
    pub fn print_pages(&self, s: &Settings) -> Result<Vec<usize>, String> {
        let n = self.layout.pages.len();
        match s.range {
            Range::All => Ok((0..n).collect()),
            Range::Current => Ok(vec![self.current_page()]),
            Range::Selection => { let (a, b) = self.doc.range(); Ok((self.layout.page_of(a)..=self.layout.page_of(b.max(a))).collect()) }
            Range::Custom => parse_ranges(&s.custom, n),
        }
    }

    fn start_print(&mut self) {
        let Some(st) = &mut self.extras.print else { return };
        let s = st.settings.clone();
        // Print Layout, not Web/Draft, and the light page colors.
        let mut pages = match self.print_pages(&s) { Ok(p) => p, Err(e) => { if let Some(st) = &mut self.extras.print { st.message = Some((e, true)); } return; } };
        let view = self.doc.view_override.take();
        let layout = word_engine::layout_document(&self.doc);
        self.doc.view_override = view;
        pages.retain(|p| *p < layout.pages.len());
        // Windows can't pass copies or collation to the PrintTo verb: they're laid out in the PDF.
        if cfg!(target_os = "windows") && s.copies > 1 {
            let base = pages.clone();
            pages = if s.collate { (0..s.copies).flat_map(|_| base.clone()).collect() } else { base.iter().flat_map(|p| std::iter::repeat_n(*p, s.copies as usize)).collect() };
        }
        let title = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into());
        let pdf = word_engine::pdf::export_pages(&self.doc, &layout, &title, &pages);
        let file = std::env::temp_dir().join(format!("word-print-{}.pdf", std::process::id()));
        if let Err(e) = std::fs::write(&file, pdf) { if let Some(st) = &mut self.extras.print { st.message = Some((e.to_string(), true)); } return; }
        self.extras.print_settings = Some(s.clone());
        if let Some(st) = &mut self.extras.print {
            st.message = None;
            st.sending = Some(Job::spawn(&self.ctx, move || send(&s, &title, &file)));
        }
    }
}

fn page_texture(app: &mut WordApp, ctx: &egui::Context, page: usize) -> Option<TextureHandle> {
    let st = app.extras.print.as_ref()?;
    if let Some((rev, p, t)) = &st.preview { if *rev == app.doc.revision && *p == page { return Some(t.clone()); } }
    let svg = word_engine::svg::page_svg(&app.doc, &app.layout, page);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = 900.0 / size.width().max(1.0);
    let (w, h) = ((size.width() * scale) as u32, (size.height() * scale) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let image = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], pixmap.data());
    let t = ctx.load_texture(format!("print-preview-{page}"), image, egui::TextureOptions::LINEAR);
    app.extras.print.as_mut()?.preview = Some((app.doc.revision, page, t.clone()));
    Some(t)
}

pub fn dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut st) = app.extras.print.take() else { return };
    if let Some(r) = st.finding.as_ref().and_then(|j| j.take()) {
        st.finding = None;
        match r {
            Ok(list) => {
                if st.settings.printer.is_empty() || !list.iter().any(|p| p.name == st.settings.printer) { st.settings.printer = list.iter().find(|p| p.default).or(list.first()).map(|p| p.name.clone()).unwrap_or_default(); }
                st.printers = list;
            }
            Err(e) => st.message = Some((format!("Couldn't list printers: {e}"), true)),
        }
    }
    if let Some(r) = st.sending.as_ref().and_then(|j| j.take()) {
        st.sending = None;
        st.message = Some(match r { Ok(m) => (if m.is_empty() { crate::i18n::tr("Sent to the printer.").into() } else { format!("Sent to the printer. {m}") }, false), Err(e) => (format!("Printing failed: {e}"), true) });
    }
    if st.finding.is_some() || st.sending.is_some() { ctx.request_repaint_after(std::time::Duration::from_millis(150)); }
    let n = app.layout.pages.len().max(1);
    st.preview_page = st.preview_page.min(n - 1);
    app.extras.print = Some(st);
    let (mut open, mut print, mut to_pdf, mut close, mut refresh) = (true, false, false, false, false);
    let tex = page_texture(app, ctx, app.extras.print.as_ref().map(|s| s.preview_page).unwrap_or(0));
    let has_selection = app.doc.range().0 != app.doc.range().1;
    crate::theme::dialog(ctx, "Print").open(&mut open).default_width(900.0).show(ctx, |ui| {
        app.canvas_focus = false;
        let pages_hint = app.extras.print.as_ref().map(|st| app.print_pages(&st.settings));
        let st = app.extras.print.as_mut().unwrap();
        crate::theme::hcols(ui, |ui| {
            crate::theme::vcol(ui, |ui| {
                ui.set_width(320.0);
                crate::theme::hrow(ui, |ui| {
                    if crate::theme::primary_enabled(ui, st.sending.is_none() && !st.printers.is_empty(), "Print").clicked() { print = true; }
                    ui.label(crate::i18n::tr("Copies:"));
                    ui.add(egui::DragValue::new(&mut st.settings.copies).range(1..=999));
                });
                if st.sending.is_some() { crate::theme::hrow(ui, |ui| { ui.spinner(); ui.label(crate::i18n::tr("Sending to the printer…")); }); }
                crate::theme::section(ui, "Printer");
                crate::theme::hrow(ui, |ui| {
                    let label = st.printers.iter().find(|p| p.name == st.settings.printer).map(|p| format!("{}{}", p.name, if p.status.is_empty() { String::new() } else { format!(" — {}", p.status) })).unwrap_or_else(|| if st.finding.is_some() { crate::i18n::tr("Looking for printers…").into() } else { crate::i18n::tr("No printers").into() });
                    crate::theme::Combo::from_id_salt("printer").width(250.0).selected_text(crate::i18n::tr(&label)).show_ui(ui, |ui| {
                        for p in &st.printers { ui.selectable_value(&mut st.settings.printer, p.name.clone(), format!("{}{}{}", p.name, if p.default { " (default)" } else { "" }, if p.status.is_empty() { String::new() } else { format!(" — {}", p.status) })); }
                    });
                    if ui.button("⟳").on_hover_text(crate::i18n::tr("Refresh the printer list")).clicked() { refresh = true; }
                });
                if st.printers.is_empty() && st.finding.is_none() { ui.label(RichText::new(crate::i18n::tr("No printers are installed. Add one in your system's printer settings, or save a PDF.")).small().weak()); }
                crate::theme::section(ui, "Settings");
                crate::theme::Combo::from_id_salt("print-range").width(300.0).selected_text(crate::i18n::tr(&(match st.settings.range { Range::All => crate::i18n::tr("Print All Pages"), Range::Current => crate::i18n::tr("Print Current Page"), Range::Selection => crate::i18n::tr("Print Selection"), Range::Custom => crate::i18n::tr("Custom Print") }))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.settings.range, Range::All, "Print All Pages");
                    ui.selectable_value(&mut st.settings.range, Range::Current, "Print Current Page");
                    if has_selection { ui.selectable_value(&mut st.settings.range, Range::Selection, "Print Selection"); }
                    ui.selectable_value(&mut st.settings.range, Range::Custom, "Custom Print");
                });
                crate::theme::hrow(ui, |ui| {
                    ui.label(crate::i18n::tr("Pages:"));
                    let r = ui.add(egui::TextEdit::singleline(&mut st.settings.custom).hint_text(crate::i18n::tr("e.g. 1-3, 5, 8-")).desired_width(200.0));
                    if r.changed() { st.settings.range = if st.settings.custom.trim().is_empty() { Range::All } else { Range::Custom }; }
                });
                crate::theme::Combo::from_id_salt("print-sides").width(300.0).selected_text(crate::i18n::tr(&(match st.settings.sides { Sides::One => crate::i18n::tr("Print One Sided"), Sides::LongEdge => crate::i18n::tr("Print on Both Sides (flip on long edge)"), Sides::ShortEdge => crate::i18n::tr("Print on Both Sides (flip on short edge)") }))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.settings.sides, Sides::One, "Print One Sided");
                    ui.selectable_value(&mut st.settings.sides, Sides::LongEdge, "Print on Both Sides (flip on long edge)");
                    ui.selectable_value(&mut st.settings.sides, Sides::ShortEdge, "Print on Both Sides (flip on short edge)");
                });
                crate::theme::Combo::from_id_salt("print-collate").width(300.0).selected_text(crate::i18n::tr(&(if st.settings.collate { crate::i18n::tr("Collated  1,2,3  1,2,3  1,2,3") } else { crate::i18n::tr("Uncollated  1,1,1  2,2,2  3,3,3") }))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.settings.collate, true, "Collated  1,2,3  1,2,3  1,2,3");
                    ui.selectable_value(&mut st.settings.collate, false, "Uncollated  1,1,1  2,2,2  3,3,3");
                });
                crate::theme::Combo::from_id_salt("print-nup").width(300.0).selected_text(crate::i18n::trf(if st.settings.per_sheet > 1 { "{} Pages Per Sheet" } else { "{} Page Per Sheet" }, &[&st.settings.per_sheet])).show_ui(ui, |ui| {
                    for k in [1u32, 2, 4, 6, 9, 16] { ui.selectable_value(&mut st.settings.per_sheet, k, crate::i18n::trf(if k > 1 { "{} Pages Per Sheet" } else { "{} Page Per Sheet" }, &[&k])); }
                });
                crate::theme::check(ui, &mut st.settings.gray, crate::i18n::tr("Print in grayscale"));
                if cfg!(target_os = "windows") { ui.label(RichText::new(crate::i18n::tr("Two-sided, pages per sheet and grayscale use the printer's own settings on Windows.")).small().weak()); }
                match &pages_hint { Some(Ok(p)) => { ui.label(RichText::new(crate::i18n::trf(if p.len() == 1 { "{} page will print" } else { "{} pages will print" }, &[&p.len()])).small().weak()); } Some(Err(e)) => { ui.colored_label(egui::Color32::from_rgb(190, 40, 40), e); } None => {} }
                if let Some((m, err)) = &st.message { if *err { ui.colored_label(egui::Color32::from_rgb(190, 40, 40), m); } else { ui.colored_label(egui::Color32::from_rgb(30, 130, 60), m); } }
                ui.add_space(8.0);
                crate::theme::hrow(ui, |ui| {
                    if ui.button(crate::i18n::tr("Save as PDF…")).clicked() { to_pdf = true; }
                    if ui.button(crate::i18n::tr("Page Setup…")).clicked() { app.dialogs.page_setup = Some(app.doc.page.clone()); }
                });
            });
            ui.add_space(16.0);
            crate::theme::vcol(ui, |ui| {
                // Preview: the page as it prints.
                let avail = egui::vec2(ui.available_width().max(300.0), (ui.ctx().content_rect().height() - 330.0).clamp(200.0, 560.0));
                if let Some(t) = &tex {
                    let s = t.size_vec2();
                    let k = (avail.x / s.x).min(avail.y / s.y);
                    let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
                    let r = egui::Rect::from_center_size(rect.center(), s * k);
                    ui.painter().rect_filled(r.translate(egui::vec2(2.0, 3.0)), 0.0, egui::Color32::from_black_alpha(40));
                    ui.painter().image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0, egui::Color32::from_gray(190)), egui::StrokeKind::Outside);
                }
                crate::theme::hrow(ui, |ui| {
                    if ui.add_enabled(st.preview_page > 0, egui::Button::new("◀")).clicked() { st.preview_page -= 1; }
                    ui.label(crate::i18n::trf("{} of {}", &[&(st.preview_page + 1), &n]));
                    if ui.add_enabled(st.preview_page + 1 < n, egui::Button::new("▶")).clicked() { st.preview_page += 1; }
                });
            });
        });
        crate::theme::footer(ui, |ui| if crate::theme::secondary(ui, "Close").clicked() { close = true; });
    });
    if refresh { if let Some(st) = &mut app.extras.print { st.finding = Some(Job::spawn(&app.ctx, list_printers)); } }
    if print { app.start_print(); }
    if to_pdf { app.export_pdf(ctx); }
    if !open || close { if let Some(st) = app.extras.print.take() { app.extras.print_settings = Some(st.settings); } app.canvas_focus = true; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_and_lp_arguments() {
        assert_eq!(parse_ranges("1-3, 5, 8-", 9).unwrap(), vec![0, 1, 2, 4, 7, 8]);
        assert!(parse_ranges("4-2", 9).is_err() && parse_ranges("x", 9).is_err() && parse_ranges("12", 9).is_err());
        let s = Settings { printer: "Office_Laser".into(), copies: 3, collate: false, sides: Sides::LongEdge, per_sheet: 2, gray: true, ..Default::default() };
        let a = lp_args(&s, "Report", "/tmp/r.pdf");
        let joined = a.join(" ");
        assert_eq!(&a[..2], &["-d".to_string(), "Office_Laser".to_string()]);
        for want in ["-n 3", "-t Report", "collate=false", "sides=two-sided-long-edge", "number-up=2", "print-color-mode=monochrome"] { assert!(joined.contains(want), "{joined}"); }
        assert_eq!(a.last().unwrap(), "/tmp/r.pdf");
    }
}
