//! The File menu (like Qalam's and Sard's, with a workbook's extras): New, New from Template,
//! Open, Open Recent, Close; Save, Save As; Print; Export (PDF, CSV, text, OpenDocument);
//! Info, Show in Folder; Options, Keyboard Shortcuts, About, Check for Updates; Quit. Also the
//! "save your changes?" guard, the splash card and the "Opening…" note while a workbook loads.

use crate::app::{Dialog, SheetApp};
use crate::i18n::tr;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Ui};
use qalam_kit::widgets::file_item as item;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub enum Pending { New, Open, OpenPath(PathBuf), Close, Quit }

pub fn file_menu(app: &mut SheetApp, ui: &mut Ui) {
    ui.set_width(270.0);
    if item(ui, "filePlus", "New", "⌘N") { app.guarded(Pending::New); ui.close(); }
    if item(ui, "themes", "New from Template…", "") { crate::backstage::open_templates(app); ui.close(); }
    if item(ui, "folderOpen", "Open…", "⌘O") { app.guarded(Pending::Open); ui.close(); }
    crate::theme::submenu(ui, tr("Open Recent"), |ui| {
        ui.set_min_width(300.0);
        let recent = app.prefs.recent.clone();
        if recent.is_empty() { ui.weak(tr("No recent workbooks")); }
        for p in recent {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let r = ui.add_enabled(p.exists(), egui::Button::new(name).frame(false)).on_hover_text(p.to_string_lossy());
            if r.clicked() { app.guarded(Pending::OpenPath(p)); ui.close(); }
        }
        ui.separator();
        if ui.button(tr("Clear Recent Files")).clicked() { app.prefs.recent.clear(); app.prefs.save(); ui.close(); }
    });
    if item(ui, "close", "Close", "⌘W") { app.guarded(Pending::Close); ui.close(); }
    ui.separator();
    if item(ui, "save", "Save", "⌘S") { app.save(false); ui.close(); }
    if item(ui, "rename", "Save As…", "⇧⌘S") { app.save(true); ui.close(); }
    ui.separator();
    if item(ui, "print", "Print…", "⌘P") { crate::print::open(app); ui.close(); }
    crate::theme::submenu(ui, tr("Export"), |ui| {
        ui.set_min_width(260.0);
        if item(ui, "pdf", "PDF…", "") { crate::print::export_pdf(app, sheet_engine::print::What::ActiveSheet); ui.close(); }
        if item(ui, "pdf", "PDF (Entire Workbook)…", "") { crate::print::export_pdf(app, sheet_engine::print::What::Workbook); ui.close(); }
        ui.separator();
        if item(ui, "pasteText", "CSV (Comma delimited)…", "") { export_text(app, "csv", ','); ui.close(); }
        if item(ui, "pasteText", "CSV UTF-8 (Comma delimited)…", "") { crate::backstage::export(app, "csv8"); ui.close(); }
        if item(ui, "pasteText", "Text (Tab delimited)…", "") { export_text(app, "txt", '\t'); ui.close(); }
        if item(ui, "download", "OpenDocument Spreadsheet…", "") { crate::backstage::export(app, "ods"); ui.close(); }
    });
    if item(ui, "info", "Info and Properties…", "") { crate::backstage::open_info(app); ui.close(); }
    if let Some(p) = app.path.clone() {
        if item(ui, "folderOpen", "Show in Folder", "") { reveal(&p); ui.close(); }
    }
    ui.separator();
    if item(ui, "preferences", "Options…", "⌘,") { app.options = Some(app.prefs.clone()); ui.close(); }
    if item(ui, "keyboard", "Keyboard Shortcuts", "") { app.dialog = Some(Dialog::Shortcuts); ui.close(); }
    if item(ui, "info", &crate::i18n::trf("About {}", &[&crate::app_name()]), "") { app.dialog = Some(Dialog::About); ui.close(); }
    if item(ui, "download", "Check for Updates…", "") { crate::update::check(ui.ctx(), true, None); ui.close(); }
    ui.separator();
    if item(ui, "close", "Quit", "⌘Q") { app.guarded(Pending::Quit); ui.close(); }
}

fn export_text(app: &mut SheetApp, ext: &str, delim: char) {
    app.commit(None);
    let name = format!("{}.{ext}", app.book.sheets[app.sheet()].name);
    if let Some(p) = rfd::FileDialog::new().set_file_name(name).save_file() {
        if let Err(e) = std::fs::write(&p, sheet_engine::csv::write(&app.book, app.sheet(), delim)) { app.error = Some(e.to_string()); } else { app.notify(&tr("Exported")); }
    }
}

fn reveal(p: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(p).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(d) = p.parent() { let _ = std::process::Command::new("xdg-open").arg(d).spawn(); }
}

/// Jadwal's icon as a texture (the splash card and About).
pub fn icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    static ICON: std::sync::OnceLock<Option<egui::TextureHandle>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| image::load_from_memory(include_bytes!("../../assets/icon.png")).ok().map(|i| {
        let i = i.resize(256, 256, image::imageops::FilterType::Lanczos3).to_rgba8();
        ctx.load_texture("jadwal-icon", egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw()), egui::TextureOptions::LINEAR)
    })).clone()
}

/// The splash card: the icon, name and maker, shown at least 1.3 s and for as long as the
/// workbook named at start is still being read; then the window grows to its usual size.
/// Returns true while it shows.
pub fn splash(app: &mut SheetApp, ui: &mut Ui) -> bool {
    let now = ui.input(|i| i.time);
    let Some(start) = &mut app.splash else { return false };
    if *start == 0.0 { *start = now.max(1e-6); }
    let elapsed = now - *start;
    let loading = app.loading.as_ref().and_then(|(p, _)| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    if elapsed > 1.3 && loading.is_none() {
        app.splash = None;
        let (size, pos, maximized) = app.prefs.window.unwrap_or(([1280.0, 820.0], None, false));
        let ctx = ui.ctx();
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(480.0, 320.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(size[0], size[1])));
        let pos = pos.map(|p| pos2(p[0], p[1])).or_else(|| {
            let m = ctx.input(|i| i.viewport().monitor_size)?;
            Some(pos2(((m.x - size[0]) / 2.0).max(0.0), ((m.y - size[1]) / 2.0).max(24.0)))
        });
        if let Some(p) = pos { ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(p)); }
        if maximized { ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true)); }
        app.auto_check_updates(&ui.ctx().clone());
        return false;
    }
    let tex = icon(ui.ctx());
    let (bg, ink, sub) = if app.dark { (Color32::from_gray(0x1C), Color32::from_gray(0xF2), Color32::from_gray(0xA8)) } else { (Color32::WHITE, Color32::from_rgb(0x0B, 0x3D, 0x22), Color32::from_gray(0x70)) };
    let status = match loading { Some(n) => crate::i18n::trf("Opening {}…", &[&n]), None => tr("Starting…") };
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let rect = ui.max_rect().shrink(10.0);
        ui.painter().add(egui::epaint::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(rect, 16.0));
        ui.painter().rect(rect, 16.0, bg, egui::Stroke::new(1.0, sub.gamma_multiply(0.25)), egui::StrokeKind::Inside);
        if ui.interact(rect, egui::Id::new("splash-drag"), Sense::drag()).drag_started() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag); }
        let a = (elapsed / 0.4).clamp(0.0, 1.0) as f32;
        let c = rect.center() - vec2(0.0, 44.0);
        if let Some(t) = &tex { ui.painter().image(t.id(), Rect::from_center_size(c - vec2(0.0, 10.0), vec2(120.0, 120.0)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(a)); }
        ui.painter().text(c + vec2(0.0, 70.0), Align2::CENTER_CENTER, "Jadwal  جدول", FontId::new(30.0, egui::FontFamily::Name("ui-bold".into())), ink.gamma_multiply(a));
        let g = ui.painter().layout(status, FontId::proportional(14.0), sub.gamma_multiply(a), rect.width() - 40.0);
        ui.painter().galley(c + vec2(-g.size().x / 2.0, 104.0 - g.size().y / 2.0), g, sub);
        let w = 160.0;
        let bar = Rect::from_center_size(c + vec2(0.0, 128.0), vec2(w, 3.0));
        ui.painter().rect_filled(bar, 1.5, sub.gamma_multiply(0.25 * a));
        let t = ((elapsed * 0.8) % 1.0) as f32;
        ui.painter().rect_filled(Rect::from_min_size(pos2(bar.min.x + (w - 50.0) * t, bar.min.y), vec2(50.0, 3.0)), 1.5, Color32::from_rgb(0x21, 0x73, 0x46).gamma_multiply(a));
        ui.painter().text(pos2(rect.center().x, rect.max.y - 22.0), Align2::CENTER_CENTER, format!("{} Woosool · woosool.com", tr("Made by")), FontId::proportional(12.0), sub.gamma_multiply(a));
    });
    ui.ctx().request_repaint();
    true
}

/// While a workbook opened from the File menu is read: a note over the window, which stays
/// responsive (repainted when the read finishes).
pub fn loading_note(app: &SheetApp, ctx: &egui::Context) {
    let name = app.loading.as_ref().and_then(|(p, _)| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    egui::Modal::new(egui::Id::new("opening-workbook")).show(ctx, |ui| {
        crate::theme::hrow(ui, |ui| { ui.spinner(); ui.label(crate::i18n::trf("Opening {}…", &[&name])); });
    });
}

/// Jadwal's keyboard shortcuts (grid.rs, formulas_tab.rs), for File ▸ Keyboard Shortcuts.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("⌘N", "New workbook"), ("⌘O", "Open"), ("⌘S", "Save"), ("⇧⌘S", "Save As"), ("⌘P", "Print"), ("⌘W", "Close"), ("⌘,", "Options"),
    ("⌘Z / ⇧⌘Z", "Undo / Redo"), ("⌘C / ⌘X / ⌘V", "Copy / Cut / Paste"), ("⌥⌘V", "Paste Special"), ("⌘B / ⌘I / ⌘U", "Bold / Italic / Underline"),
    ("⌘A", "Select the current region, then all"), ("⌘D / ⌘R", "Fill down / right"), ("⌘F / ⌘H", "Find / Replace"), ("⌘G / F5", "Go To"), ("⌘1", "Format Cells"), ("⌘K", "Insert hyperlink"),
    ("⌘; / ⇧⌘;", "Insert today's date / the time"), ("⌃Space / ⇧Space", "Select column / row"), ("F2", "Edit the active cell"), ("F9", "Calculate now"), ("⇧F3", "Insert Function"), ("⌘`", "Show formulas"),
    ("Arrows, Page Up/Down", "Move"), ("⌘ + arrow", "Jump to the edge of the data"), ("Home / ⌘Home / ⌘End", "Row start / A1 / last cell"),
];

impl SheetApp {
    /// Runs `p` now, or after asking whether to save unsaved changes.
    pub fn guarded(&mut self, p: Pending) {
        self.commit(None);
        if self.dirty && !matches!(p, Pending::Open) { self.pending = Some(p); } else { self.run_pending(p); }
    }
    pub fn run_pending(&mut self, p: Pending) {
        match p {
            Pending::New | Pending::Close => self.new_book(),
            Pending::Open => { if self.dirty { self.pending = Some(Pending::Open); return; } self.open_dialog(); }
            Pending::OpenPath(path) => self.open_path(path),
            Pending::Quit => { self.remember_window(); self.quitting = true; self.clear_recovery(); self.ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        }
    }
    pub fn remember_window(&mut self) {
        let (size, pos, max) = self.ctx.input(|i| { let v = i.viewport(); (v.inner_rect.map(|r| [r.width(), r.height()]), v.outer_rect.map(|r| [r.min.x, r.min.y]), v.maximized.unwrap_or(false)) });
        if let Some(s) = size { self.prefs.window = Some((s, pos, max)); self.prefs.save(); }
    }
}

pub fn dialogs(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(p) = app.pending.clone() else { return };
    let mut choice = None;
    crate::theme::dialog(ctx, crate::app_name()).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_max_width(420.0);
        let name = app.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Book1"));
        ui.label(egui::RichText::new(crate::i18n::trf("Do you want to save the changes you made to {}?", &[&name])).strong());
        ui.label(tr("Your changes will be lost if you don't save them."));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Save").clicked() { choice = Some(0); }
            if crate::theme::secondary(ui, "Don't Save").clicked() { choice = Some(1); }
            if crate::theme::secondary(ui, "Cancel").clicked() { choice = Some(2); }
        });
    });
    match choice {
        Some(0) => { app.pending = None; if app.save(false) { app.dirty = false; if let Pending::Open = p { app.open_dialog(); } else { app.run_pending(p); } } }
        Some(1) => { app.pending = None; app.dirty = false; if let Pending::Open = p { app.open_dialog(); } else { app.run_pending(p); } }
        Some(2) => app.pending = None,
        _ => {}
    }
}
