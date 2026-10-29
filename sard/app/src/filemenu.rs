//! The File menu (like Qalam's, with a presentation's extras): New, New from Template, Open,
//! Open Recent, Close; Save, Save As, Save a Copy, Save as Template; Print; Export (PDF,
//! handouts, notes, outline, pictures, video); Info and Properties; Options, Keyboard
//! Shortcuts, About, Check for Updates; Quit. Also the "save changes?" guard, the splash
//! screen, and the About, Shortcuts, Properties, Print/Export and Video dialogs.

use crate::app::SlidesApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::export::{self, Layout, PrintOptions};
use std::path::PathBuf;

/// What waits for "save changes?".
#[derive(Clone, Debug, PartialEq)]
pub enum Pending { New, Open, OpenPath(PathBuf), Template, Close, Quit }

pub enum FileDialog {
    Print { o: PrintOptions, range: u8, custom: String, page: usize, printer: String, printers: Vec<String>, copies: u32, export: bool, preview: Option<(String, Vec<egui::TextureHandle>)> },
    Properties { values: Vec<String> },
    Video { secs: f64, height: u32, use_timings: bool },
    About,
    Shortcuts,
}

fn item(ui: &mut Ui, icon: &str, label: &str, key: &str) -> bool { file_item(ui, icon, label, key) }

pub fn file_menu(app: &mut SlidesApp, ui: &mut Ui) {
    ui.set_width(270.0);
    if item(ui, "filePlus", "New", "⌘N") { app.guarded(Pending::New); ui.close(); }
    if item(ui, "themes", "New from Template…", "") { app.guarded(Pending::Template); ui.close(); }
    if item(ui, "folderOpen", "Open…", "⌘O") { app.guarded(Pending::Open); ui.close(); }
    crate::theme::submenu(ui, tr("Open Recent"), |ui| {
        ui.set_min_width(300.0);
        let recent = app.prefs.recent.clone();
        if recent.is_empty() { ui.weak(tr("No recent presentations")); }
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
    if item(ui, "save", "Save As…", "⇧⌘S") { app.save(true); ui.close(); }
    if item(ui, "duplicate", "Save a Copy…", "") { save_copy(app, false); ui.close(); }
    if item(ui, "layout", "Save as Template…", "") { save_copy(app, true); ui.close(); }
    ui.separator();
    if item(ui, "printer", "Print…", "⌘P") { open_print(app, false); ui.close(); }
    crate::theme::submenu(ui, tr("Export"), |ui| {
        ui.set_min_width(260.0);
        if item(ui, "pdf", "PDF…", "") { quick_pdf(app, Layout::Slides); ui.close(); }
        if item(ui, "pdf", "Handouts, Notes or Outline as PDF…", "") { open_print(app, true); ui.close(); }
        ui.separator();
        if item(ui, "imageFile", "Current Slide as PNG…", "") { app.export_png(false); ui.close(); }
        if item(ui, "pictures", "All Slides as PNG…", "") { app.export_png(true); ui.close(); }
        if item(ui, "imageFile", "Current Slide as JPEG…", "") { export_jpeg(app, false); ui.close(); }
        if item(ui, "pictures", "All Slides as JPEG…", "") { export_jpeg(app, true); ui.close(); }
        ui.separator();
        if item(ui, "outlineView", "Outline as Text…", "") { export_outline(app); ui.close(); }
        if item(ui, "video", "Video (MP4)…", "") { app.file_dialog = Some(FileDialog::Video { secs: 5.0, height: 1080, use_timings: true }); ui.close(); }
    });
    if item(ui, "info", "Info and Properties…", "") { open_properties(app); ui.close(); }
    if let Some(p) = app.path.clone() {
        if item(ui, "folderOpen", "Show in Folder", "") { reveal(&p); ui.close(); }
    }
    ui.separator();
    if item(ui, "settings", "Options…", "⌘,") { app.options = Some(app.prefs.clone()); ui.close(); }
    if item(ui, "translate", "Translation Models…", "") { crate::translate::models(app); ui.close(); }
    if item(ui, "keyboard", "Keyboard Shortcuts", "") { app.file_dialog = Some(FileDialog::Shortcuts); ui.close(); }
    if item(ui, "info", &crate::i18n::trf("About {}", &[&crate::app_name()]), "") { app.file_dialog = Some(FileDialog::About); ui.close(); }
    if item(ui, "download", "Check for Updates…", "") { crate::update::check(ui.ctx(), true, None); ui.close(); }
    ui.separator();
    if item(ui, "close", "Quit", "⌘Q") { app.guarded(Pending::Quit); ui.close(); }
}

fn reveal(p: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(p).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(d) = p.parent() { let _ = std::process::Command::new("xdg-open").arg(d).spawn(); }
}

fn stem(app: &SlidesApp) -> String { app.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Presentation1")) }

fn save_copy(app: &mut SlidesApp, template: bool) {
    let (ext, filter) = if template { ("potx", "PowerPoint Template") } else { ("pptx", "PowerPoint Presentation") };
    let Some(p) = rfd::FileDialog::new().add_filter(tr(filter), &[ext]).set_file_name(format!("{}.{ext}", stem(app))).save_file() else { return };
    let mut pres = app.pres.clone();
    if template {
        // A template's main part has its own content type.
        let main = pres.main.clone();
        pres.pkg.set_override(&format!("/{main}"), "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml");
    }
    match pres.save().and_then(|b| std::fs::write(&p, b).map_err(|e| e.to_string())) {
        Ok(()) => app.notify(&tr("Saved a copy")),
        Err(e) => app.error = Some(crate::i18n::trf("Couldn't save the presentation.\n\n{}", &[&e])),
    }
}

fn quick_pdf(app: &mut SlidesApp, layout: Layout) {
    let o = PrintOptions { layout, slides: (0..app.slides().len()).collect(), ..Default::default() };
    save_pdf(app, &o);
}

fn save_pdf(app: &mut SlidesApp, o: &PrintOptions) -> Option<PathBuf> {
    let p = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{}.pdf", stem(app))).save_file()?;
    let pages = export::pages(&app.pres, o);
    let title = app.pres.property("dc:title");
    let title = if title.is_empty() { stem(app) } else { title };
    let bytes = export::pdf(&pages, &title);
    match std::fs::write(&p, bytes) {
        Ok(()) => { app.notify(&tr("Exported")); Some(p) }
        Err(e) => { app.error = Some(e.to_string()); None }
    }
}

fn export_jpeg(app: &mut SlidesApp, all: bool) {
    let slides = app.slides();
    if all {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        for (i, s) in slides.iter().enumerate() { if let Some(j) = export::slide_jpeg(&app.pres, s, 1920) { let _ = std::fs::write(dir.join(format!("Slide{}.jpg", i + 1)), j); } }
        app.notify(&crate::i18n::trf("Exported {} slides", &[&slides.len()]));
    } else if let Some(s) = app.slide() {
        let Some(p) = rfd::FileDialog::new().add_filter("JPEG", &["jpg", "jpeg"]).set_file_name(format!("Slide{}.jpg", app.current + 1)).save_file() else { return };
        if let Some(j) = export::slide_jpeg(&app.pres, &s, 1920) { let _ = std::fs::write(p, j); app.notify(&tr("Exported")); }
    }
}

fn export_outline(app: &mut SlidesApp) {
    let Some(p) = rfd::FileDialog::new().add_filter(tr("Text"), &["txt"]).set_file_name(format!("{}.txt", stem(app))).save_file() else { return };
    let text = export::outline_text(&app.pres, &(0..app.slides().len()).collect::<Vec<_>>());
    if std::fs::write(p, text).is_ok() { app.notify(&tr("Exported")); }
}

fn open_print(app: &mut SlidesApp, export: bool) {
    let n = app.slides().len();
    let o = PrintOptions { slides: (0..n).collect(), dpi: 150.0, ..Default::default() };
    app.file_dialog = Some(FileDialog::Print { o, range: 0, custom: String::new(), page: 0, printer: String::new(), printers: printers(), copies: 1, export, preview: None });
}

fn open_properties(app: &mut SlidesApp) {
    let values = export::PROPS.iter().map(|(k, _)| app.pres.property(k)).collect();
    app.file_dialog = Some(FileDialog::Properties { values });
}

fn printers() -> Vec<String> {
    if cfg!(windows) { return vec![]; }
    let Ok(o) = std::process::Command::new("lpstat").arg("-a").output() else { return vec![] };
    String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.split_whitespace().next().map(String::from)).collect()
}

/// "1-3, 5" → slide indexes.
pub fn parse_ranges(s: &str, count: usize) -> Vec<usize> {
    let mut out = vec![];
    for part in s.split([',', ';']) {
        let part = part.trim();
        if part.is_empty() { continue; }
        let (a, b) = part.split_once('-').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((part, part));
        let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) else { continue };
        for i in a.min(b)..=a.max(b) { if i >= 1 && i <= count && !out.contains(&(i - 1)) { out.push(i - 1); } }
    }
    out
}

// ---- guard, splash, dialogs -------------------------------------------------------------

impl SlidesApp {
    /// Do `p`, first asking to save unsaved changes.
    pub fn guarded(&mut self, p: Pending) {
        if self.dirty { self.pending = Some(p); } else { self.run_pending(p); }
    }

    pub fn run_pending(&mut self, p: Pending) {
        match p {
            Pending::New => self.new_presentation(),
            Pending::Open => self.open_dialog(),
            Pending::OpenPath(path) => self.open_path(path),
            Pending::Template => {
                if let Some(path) = rfd::FileDialog::new().add_filter(tr("PowerPoint Template"), &["potx", "pptx"]).pick_file() {
                    self.open_path(path);
                    // A new presentation from it: untitled, so Save asks where.
                    self.path = None;
                    let main = self.pres.main.clone();
                    self.pres.pkg.set_override(&format!("/{main}"), "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml");
                    self.dirty = true;
                }
            }
            Pending::Close => { self.new_presentation(); self.clear_recovery(); }
            Pending::Quit => { self.clear_recovery(); self.remember_window(); self.quitting = true; self.ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        }
    }

    pub fn remember_window(&mut self) {
        let (r, max) = self.ctx.input(|i| (i.viewport().outer_rect, i.viewport().maximized.unwrap_or(false)));
        if let (Some(r), true) = (r, self.splash.is_none()) {
            self.prefs.window = Some(([r.width(), r.height()], Some([r.min.x, r.min.y]), max));
            self.prefs.save();
        }
    }
}

pub fn guard_dialog(app: &mut SlidesApp, ctx: &egui::Context) {
    // Closing the window with unsaved changes asks first.
    if ctx.input(|i| i.viewport().close_requested()) && !app.quitting {
        if app.dirty { ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose); app.pending = Some(Pending::Quit); }
        else { app.remember_window(); app.clear_recovery(); }
    }
    let Some(p) = app.pending.clone() else { return };
    let mut done = None;
    crate::theme::dialog(ctx, &crate::app_name()).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_max_width(400.0);
        let name = app.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Presentation1"));
        ui.label(RichText::new(crate::i18n::trf("Do you want to save the changes you made to {}?", &[&name])).strong());
        ui.label(tr("Your changes will be lost if you don't save them."));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Save").clicked() { app.save(false); if !app.dirty { done = Some(true); } }
            if crate::theme::secondary(ui, "Don't Save").clicked() { done = Some(true); }
            if crate::theme::secondary(ui, "Cancel").clicked() { done = Some(false); }
        });
    });
    match done {
        Some(true) => { app.pending = None; app.dirty = false; app.run_pending(p); }
        Some(false) => app.pending = None,
        None => {}
    }
}

fn icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    static ICON: std::sync::OnceLock<Option<egui::TextureHandle>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| image::load_from_memory(include_bytes!("../../assets/icon-full.png")).ok().map(|i| {
        let i = i.resize(256, 256, image::imageops::FilterType::Lanczos3).to_rgba8();
        ctx.load_texture("sard-icon", egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw()), egui::TextureOptions::LINEAR)
    })).clone()
}

/// The splash card: the icon, name and maker, then the window grows to its usual size.
/// Returns true while it shows.
pub fn splash(app: &mut SlidesApp, ui: &mut Ui) -> bool {
    let now = ui.input(|i| i.time);
    let Some(start) = &mut app.splash else { return false };
    if *start == 0.0 { *start = now.max(1e-6); }
    let elapsed = now - *start;
    if elapsed > 1.3 {
        app.splash = None;
        let (size, pos, maximized) = app.prefs.window.unwrap_or(([1280.0, 820.0], None, false));
        let ctx = ui.ctx();
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(640.0, 420.0)));
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
    let dark = app.dark;
    let (bg, ink, sub) = if dark { (Color32::from_gray(0x1C), Color32::from_gray(0xF2), Color32::from_gray(0xA8)) } else { (Color32::WHITE, Color32::from_rgb(0x4A, 0x1A, 0x08), Color32::from_gray(0x70)) };
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let rect = ui.max_rect().shrink(10.0);
        ui.painter().add(egui::epaint::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(rect, 16.0));
        ui.painter().rect(rect, 16.0, bg, egui::Stroke::new(1.0, sub.gamma_multiply(0.25)), egui::StrokeKind::Inside);
        if ui.interact(rect, egui::Id::new("splash-drag"), Sense::drag()).drag_started() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag); }
        let a = (elapsed / 0.4).clamp(0.0, 1.0) as f32;
        let c = rect.center() - vec2(0.0, 44.0);
        if let Some(t) = &tex { ui.painter().image(t.id(), Rect::from_center_size(c - vec2(0.0, 10.0), vec2(120.0, 120.0)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(a)); }
        ui.painter().text(c + vec2(0.0, 70.0), Align2::CENTER_CENTER, "Sard  سرد", FontId::new(30.0, egui::FontFamily::Name("ui-bold".into())), ink.gamma_multiply(a));
        ui.painter().text(c + vec2(0.0, 104.0), Align2::CENTER_CENTER, tr("Starting…"), FontId::proportional(14.0), sub.gamma_multiply(a));
        let w = 160.0;
        let bar = Rect::from_center_size(c + vec2(0.0, 128.0), vec2(w, 3.0));
        ui.painter().rect_filled(bar, 1.5, sub.gamma_multiply(0.25 * a));
        let t = ((elapsed * 0.8) % 1.0) as f32;
        ui.painter().rect_filled(Rect::from_min_size(pos2(bar.min.x + (w - 50.0) * t, bar.min.y), vec2(50.0, 3.0)), 1.5, Color32::from_rgb(0xE2, 0x57, 0x2A).gamma_multiply(a));
        ui.painter().text(pos2(rect.center().x, rect.max.y - 22.0), Align2::CENTER_CENTER, format!("{} Woosool · woosool.com", tr("Made by")), FontId::proportional(12.0), sub.gamma_multiply(a));
    });
    ui.ctx().request_repaint();
    true
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("⌘N", "New presentation"), ("⌘O", "Open"), ("⌘S", "Save"), ("⇧⌘S", "Save As"), ("⌘P", "Print"), ("⌘W", "Close"), ("⌘Q", "Quit"), ("⌘,", "Options"),
    ("⌘M", "New slide"), ("⌘Z / ⇧⌘Z", "Undo / Redo"), ("⌘C / ⌘X / ⌘V", "Copy / Cut / Paste"), ("⌘D", "Duplicate"), ("⌘F / ⌘H", "Find / Replace"), ("⌘K", "Insert hyperlink"),
    ("F5", "Slide show from the beginning"), ("⇧F5", "Slide show from this slide"),
    ("→ ↓ Space N", "Show: next"), ("← ↑ P", "Show: previous"), ("B / W", "Show: black / white screen"), ("G", "Show: see all slides"), ("⌘P / ⌘L / ⌘I / ⌘E", "Show: pen / laser / highlighter / eraser"), ("E", "Show: erase the ink"), ("Esc", "Show: end"),
];

pub fn dialogs(app: &mut SlidesApp, ctx: &egui::Context) {
    guard_dialog(app, ctx);
    crate::prefs::recovery_dialog(app, ctx);
    crate::prefs::options_dialog(app, ctx);
    crate::update::dialog(app, ctx);
    crate::smartart_ui::window(app, ctx);
    crate::translate::window(app, ctx);
    let Some(mut d) = app.file_dialog.take() else { return };
    let mut open = true;
    let mut done = false;
    let title = match &d { FileDialog::Print { export: true, .. } => "Export", FileDialog::Print { .. } => "Print", FileDialog::Properties { .. } => "Properties", FileDialog::Video { .. } => "Export Video", FileDialog::About => "About", FileDialog::Shortcuts => "Keyboard Shortcuts" };
    let title = if title == "About" { crate::i18n::trf("About {}", &[&crate::app_name()]) } else { tr(title) };
    crate::theme::dialog(ctx, &title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        match &mut d {
            FileDialog::About => {
                crate::theme::hrow(ui, |ui| {
                    if let Some(t) = icon(ui.ctx()) { ui.add(egui::Image::new((t.id(), vec2(80.0, 80.0)))); }
                    crate::theme::vcol(ui, |ui| {
                        ui.label(RichText::new("Sard  سرد").size(22.0).family(egui::FontFamily::Name("ui-bold".into())));
                        ui.label(crate::i18n::trf("Version {}", &[&crate::update::CURRENT]));
                        ui.label(tr("A free desktop presentation app for creating, editing and presenting .pptx slides offline."));
                    });
                });
                ui.add_space(8.0);
                crate::theme::hrow(ui, |ui| { ui.label(tr("Made by")); ui.hyperlink_to("Woosool", "https://woosool.com"); });
                ui.hyperlink_to(tr("Sard on woosool.com"), "https://woosool.com/en/products/sard");
                if crate::theme::secondary(ui, "Check for Updates…").clicked() { crate::update::check(ui.ctx(), true, None); }
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Released under the MIT License.")).strong());
                ui.label(RichText::new("Copyright © 2026 Woosool").small());
                egui::CollapsingHeader::new(tr("License")).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| ui.label(RichText::new(include_str!("../../../qalam/LICENSE")).small().monospace()));
                });
                ui.add_space(6.0);
                ui.weak(tr("Fonts: Noto Sans, Carlito, Caladea (OFL / Apache-2.0), Liberation (OFL)."));
                ui.weak(tr("Video playback and export use ffmpeg when it's installed."));
            }
            FileDialog::Shortcuts => {
                egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                    crate::theme::grid(ui, egui::Grid::new("keys").num_columns(2).striped(true).spacing(vec2(16.0, 6.0)), |ui| {
                        for (k, what) in SHORTCUTS { ui.label(RichText::new(*k).monospace().strong()); ui.label(tr(what)); ui.end_row(); }
                    });
                });
                crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Close").clicked() { done = true; });
            }
            FileDialog::Properties { values } => {
                let n = app.slides().len();
                crate::theme::grid(ui, egui::Grid::new("props").num_columns(2).spacing(vec2(12.0, 6.0)), |ui| {
                    for (k, (name, label)) in export::PROPS.iter().enumerate() {
                        ui.label(tr(label));
                        if name.starts_with("dcterms:") || *name == "cp:lastModifiedBy" { ui.weak(if values[k].is_empty() { "—".to_string() } else { values[k].replace('T', " ").trim_end_matches('Z').to_string() }); }
                        else if *name == "dc:description" { ui.add(egui::TextEdit::multiline(&mut values[k]).desired_rows(3).desired_width(280.0)); }
                        else { ui.add(egui::TextEdit::singleline(&mut values[k]).desired_width(280.0)); }
                        ui.end_row();
                    }
                    ui.label(tr("Slides")); ui.label(n.to_string()); ui.end_row();
                    ui.label(tr("Hidden slides")); ui.label(app.slides().iter().filter(|s| app.pres.hidden(s)).count().to_string()); ui.end_row();
                    let (w, h) = app.pres.size();
                    ui.label(tr("Slide size")); ui.label(format!("{:.2}\" × {:.2}\"", w / 914_400.0, h / 914_400.0)); ui.end_row();
                    if let Some(p) = &app.path {
                        ui.label(tr("Location")); ui.label(p.to_string_lossy()); ui.end_row();
                        if let Ok(m) = std::fs::metadata(p) { ui.label(tr("Size")); ui.label(format!("{:.1} KB", m.len() as f64 / 1024.0)); ui.end_row(); }
                    }
                });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() {
                        app.checkpoint("properties");
                        for (k, (name, _)) in export::PROPS.iter().enumerate() { if !name.starts_with("dcterms:") && *name != "cp:lastModifiedBy" { app.pres.set_property(name, values[k].trim()); } }
                        app.changed();
                        done = true;
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { done = true; }
                });
            }
            FileDialog::Video { secs, height, use_timings } => {
                let ff = crate::media::tool("ffmpeg").is_some();
                if !ff { ui.colored_label(Color32::from_rgb(0xC0, 0x50, 0x00), tr("Exporting video needs ffmpeg (ffmpeg.org) installed.")); }
                crate::theme::grid(ui, egui::Grid::new("video").num_columns(2).spacing(vec2(12.0, 8.0)), |ui| {
                    ui.label(tr("Quality:"));
                    crate::theme::Combo::from_id_salt("vq").selected_text(format!("{height}p")).show_ui(ui, |ui| for h in [2160u32, 1080, 720, 480] { ui.selectable_value(height, h, format!("{h}p")); });
                    ui.end_row();
                    ui.label(tr("Seconds on each slide:")); ui.add(egui::DragValue::new(secs).range(0.5..=600.0).speed(0.1).suffix(" s")); ui.end_row();
                });
                ui.checkbox(use_timings, tr("Use recorded timings"));
                crate::theme::footer(ui, |ui| {
                    if ui.add_enabled_ui(ff, |ui| crate::theme::primary(ui, "Export")).inner.clicked() { let (s, h, t) = (*secs, *height, *use_timings); export_video(app, s, h, t); done = true; }
                    if crate::theme::secondary(ui, "Cancel").clicked() { done = true; }
                });
            }
            FileDialog::Print { o, range, custom, page, printer, printers, copies, export, preview } => {
                let n = app.slides().len();
                ui.horizontal_top(|ui| {
                    // Settings on one side, the preview on the other.
                    ui.vertical(|ui| {
                        ui.set_width(270.0);
                        if !*export {
                            crate::theme::hrow(ui, |ui| {
                                ui.label(tr("Printer:"));
                                let label = if printer.is_empty() { tr("Default printer") } else { printer.clone() };
                                crate::theme::Combo::from_id_salt("printer").width(170.0).selected_text(label).show_ui(ui, |ui| {
                                    ui.selectable_value(printer, String::new(), tr("Default printer"));
                                    for p in printers.iter() { ui.selectable_value(printer, p.clone(), p); }
                                });
                            });
                            crate::theme::hrow(ui, |ui| { ui.label(tr("Copies:")); ui.add(egui::DragValue::new(copies).range(1..=99)); });
                        }
                        crate::theme::section(ui, "Slides");
                        ui.radio_value(range, 0, tr("Print All Slides"));
                        ui.radio_value(range, 1, tr("Print Current Slide"));
                        ui.horizontal(|ui| { ui.radio_value(range, 2, tr("Custom Range:")); ui.add(egui::TextEdit::singleline(custom).desired_width(90.0).hint_text("1-3, 5")); });
                        crate::theme::section(ui, "Layout");
                        let name = |l: Layout| match l { Layout::Slides => tr("Full Page Slides"), Layout::Notes => tr("Notes Pages"), Layout::Outline => tr("Outline"), Layout::Handouts(n) => crate::i18n::trf("Handouts: {} Slides", &[&n]) };
                        crate::theme::Combo::from_id_salt("print-layout").width(230.0).selected_text(name(o.layout)).show_ui(ui, |ui| {
                            for l in [Layout::Slides, Layout::Notes, Layout::Outline, Layout::Handouts(1), Layout::Handouts(2), Layout::Handouts(3), Layout::Handouts(4), Layout::Handouts(6), Layout::Handouts(9)] { ui.selectable_value(&mut o.layout, l, name(l)); }
                        });
                        ui.checkbox(&mut o.frame, tr("Frame Slides"));
                        ui.checkbox(&mut o.gray, tr("Grayscale"));
                        ui.checkbox(&mut o.landscape, tr("Landscape (handouts and outline)"));
                        crate::theme::section(ui, "Header and Footer");
                        crate::theme::hrow(ui, |ui| { ui.label(tr("Header:")); ui.text_edit_singleline(&mut o.header); });
                        crate::theme::hrow(ui, |ui| { ui.label(tr("Footer:")); ui.text_edit_singleline(&mut o.footer); });
                        ui.checkbox(&mut o.date, tr("Date"));
                        ui.checkbox(&mut o.numbers, tr("Page numbers"));
                    });
                    o.slides = match range { 1 => vec![app.current.min(n.saturating_sub(1))], 2 => parse_ranges(custom, n), _ => (0..n).filter(|&i| !app.pres.hidden(&app.slides()[i])).collect() };
                    // The preview, redrawn when the settings change.
                    let key = format!("{:?}{:?}{}{}{}{}{}{}{}", o.layout, o.slides, o.frame, o.gray, o.landscape, o.header, o.footer, o.date, o.numbers);
                    if preview.as_ref().is_none_or(|p| p.0 != key) {
                        let small = PrintOptions { dpi: 60.0, ..o.clone() };
                        let ctx = ui.ctx().clone();
                        let texs = export::pages(&app.pres, &small).into_iter().enumerate().map(|(k, p)| {
                            let px: Vec<Color32> = p.pixmap.pixels().iter().map(|c| Color32::from_rgba_premultiplied(c.red(), c.green(), c.blue(), c.alpha())).collect();
                            ctx.load_texture(format!("print-{k}"), egui::ColorImage { size: [p.pixmap.width() as usize, p.pixmap.height() as usize], source_size: vec2(p.pixmap.width() as f32, p.pixmap.height() as f32), pixels: px }, egui::TextureOptions::LINEAR)
                        }).collect();
                        *preview = Some((key, texs));
                        *page = 0;
                    }
                    ui.vertical(|ui| {
                        let pages = &preview.as_ref().unwrap().1;
                        let area = vec2(380.0, 420.0);
                        let (r, _) = ui.allocate_exact_size(area, Sense::hover());
                        ui.painter().rect_filled(r, 4.0, Color32::from_gray(0xD8));
                        if let Some(t) = pages.get(*page) {
                            let s = t.size_vec2();
                            let k = ((area.x - 20.0) / s.x).min((area.y - 20.0) / s.y);
                            let pr = Rect::from_center_size(r.center(), s * k);
                            ui.painter().rect_filled(pr.translate(vec2(2.0, 2.0)), 0.0, Color32::from_black_alpha(40));
                            ui.painter().image(t.id(), pr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        } else { ui.painter().text(r.center(), Align2::CENTER_CENTER, tr("Nothing to print"), FontId::proportional(13.0), c_ink3()); }
                        ui.horizontal(|ui| {
                            if ui.add_enabled(*page > 0, egui::Button::new("◀")).clicked() { *page -= 1; }
                            ui.label(format!("{} / {}", (*page + 1).min(pages.len()), pages.len()));
                            if ui.add_enabled(*page + 1 < pages.len(), egui::Button::new("▶")).clicked() { *page += 1; }
                        });
                    });
                });
                crate::theme::footer(ui, |ui| {
                    let label = if *export { "Export PDF" } else { "Print" };
                    if crate::theme::primary(ui, label).clicked() {
                        if *export { if save_pdf(app, o).is_some() { done = true; } }
                        else {
                            let pages = export::pages(&app.pres, o);
                            let file = std::env::temp_dir().join(format!("{}-print.pdf", stem(app)));
                            if std::fs::write(&file, export::pdf(&pages, &stem(app))).is_ok() { print_file(app, &file, printer, *copies); }
                            done = true;
                        }
                    }
                    if !*export && crate::theme::secondary(ui, "Save as PDF…").clicked() { if save_pdf(app, o).is_some() { done = true; } }
                    if crate::theme::secondary(ui, "Cancel").clicked() { done = true; }
                });
            }
        }
    });
    if open && !done { app.file_dialog = Some(d); }
}

/// Send a PDF to a printer (lp on macOS/Linux; Windows opens it for printing).
fn print_file(app: &mut SlidesApp, file: &std::path::Path, printer: &str, copies: u32) {
    #[cfg(windows)]
    {
        let _ = (printer, copies);
        let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &format!("Start-Process -FilePath '{}' -Verb Print", file.display())]).spawn();
        app.notify(&tr("Sent to the printer"));
    }
    #[cfg(not(windows))]
    {
        let mut c = std::process::Command::new("lp");
        if !printer.is_empty() { c.args(["-d", printer]); }
        c.args(["-n", &copies.to_string(), "-t", &stem(app)]).arg(file);
        match c.output() {
            Ok(o) if o.status.success() => app.notify(&tr("Sent to the printer")),
            Ok(o) => app.error = Some(crate::i18n::trf("Couldn't print.\n\n{}", &[&String::from_utf8_lossy(&o.stderr)])),
            Err(e) => app.error = Some(crate::i18n::trf("Couldn't print.\n\n{}", &[&e])),
        }
    }
}

/// File ▸ Export ▸ Video: each shown slide for its time, as an MP4 (ffmpeg).
fn export_video(app: &mut SlidesApp, secs: f64, height: u32, timings: bool) {
    let Some(ff) = crate::media::tool("ffmpeg") else { return };
    let Some(out) = rfd::FileDialog::new().add_filter("MP4", &["mp4"]).set_file_name(format!("{}.mp4", stem(app))).save_file() else { return };
    let dir = std::env::temp_dir().join(format!("sard-video-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let (sw, sh) = app.pres.size();
    let width = ((height as f64 * sw / sh).round() as u32) & !1;
    let mut list = String::new();
    let mut last = None;
    for (k, s) in app.slides().iter().enumerate() {
        if app.pres.hidden(s) { continue; }
        let Some((rgba, w, h)) = export::slide_rgba(&app.pres, s, width) else { continue };
        let Some(img) = image::RgbaImage::from_raw(w, h, rgba) else { continue };
        let f = dir.join(format!("s{k:04}.png"));
        if img.save(&f).is_err() { continue; }
        let t = if timings { app.pres.slide_transition(s).after_ms.map(|m| m as f64 / 1000.0).unwrap_or(secs) } else { secs };
        list.push_str(&format!("file '{}'\nduration {:.3}\n", f.display(), t.max(0.1)));
        last = Some(f);
    }
    // The concat format wants the last picture listed once more.
    if let Some(f) = last { list.push_str(&format!("file '{}'\n", f.display())); }
    let list_file = dir.join("list.txt");
    if std::fs::write(&list_file, list).is_err() { return; }
    let r = std::process::Command::new(ff).args(["-y", "-v", "error", "-f", "concat", "-safe", "0", "-i"]).arg(&list_file)
        .args(["-vf", &format!("fps=30,scale={width}:{}:flags=lanczos,format=yuv420p", height & !1), "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-movflags", "+faststart"]).arg(&out).output();
    let _ = std::fs::remove_dir_all(&dir);
    match r {
        Ok(o) if o.status.success() => app.notify(&tr("Video exported")),
        Ok(o) => app.error = Some(crate::i18n::trf("Couldn't export the video.\n\n{}", &[&String::from_utf8_lossy(&o.stderr)])),
        Err(e) => app.error = Some(crate::i18n::trf("Couldn't export the video.\n\n{}", &[&e])),
    }
}
