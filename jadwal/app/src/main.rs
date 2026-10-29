//! Jadwal (جدول): a desktop spreadsheet editor for .xlsx files (macOS, Windows, Linux), a port
//! of the JS spreadsheet onto the Rust sheet engine, with Qalam's interface.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod chart_ui;
mod condformat_ui;
mod dialogs;
mod filemenu;
mod filter_ui;
mod fnhelp;
mod formulabar;
mod formulas_tab;
mod grid;
mod page_layout;
mod themes_ui;
mod arrange;
mod home_hooks;
mod ins_pictures;
mod ins_shapes;
mod ins_smartart;
mod insert_hooks;
mod insert_tab;
mod insert_extras;
mod maps_ui;
mod page_view;
mod pivot_chart_ui;
mod scripts_ui;
mod timeline_ui;
mod objects;
mod pivot_ui;
mod prefs;
mod print;
mod refedit;
mod ribbon;
mod sparkline_ui;
mod table_tab;
mod tabs;
mod validation_ui;
mod review_tab;
mod view_tab;
mod backstage;
mod analysis_ui;
mod data_import;
mod data_tab;
mod data_tools;
mod pq_editor;
mod outline_ui;
mod whatif_ui;
mod update;
pub use qalam_kit::{i18n, theme};

pub const APP_NAME: &str = "Jadwal";
/// The name shown to people (Jadwal, جدول in Arabic).
pub fn app_name() -> String { crate::i18n::tr(APP_NAME) }

/// Settings and recovery files live in the platform's config folder.
pub fn config_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("JADWAL_CONFIG_DIR") { return std::path::PathBuf::from(d); }
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(if cfg!(target_os = "macos") { "Library/Application Support" } else { ".config" })))
        .unwrap_or_else(std::env::temp_dir);
    base.join(APP_NAME)
}

/// The splash card's size.
pub const SPLASH_SIZE: [f32; 2] = [440.0, 340.0];

fn main() -> eframe::Result {
    // Files the system opens with Jadwal: on the command line, or (macOS Finder) as an event.
    qalam_kit::open_files::install();
    let (path, others) = qalam_kit::open_files::startup_paths(std::env::args().skip(1));
    qalam_kit::open_files::open_elsewhere(&others);
    let icon = image::load_from_memory(include_bytes!("../../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    let prefs = prefs::Prefs::load();
    crate::i18n::set(prefs.lang);
    let mut viewport = egui::ViewportBuilder::default().with_min_inner_size([360.0, 300.0]).with_title(app_name());
    // The window first appears as a small borderless splash card (while the workbook named on
    // the command line is read), then grows into the main window where it was last time.
    viewport = if prefs.splash { viewport.with_inner_size(SPLASH_SIZE).with_decorations(false).with_transparent(true).with_resizable(false) }
        else { let (s, p, m) = prefs.window.unwrap_or(([1280.0, 820.0], None, false)); let mut v = viewport.with_inner_size(s).with_maximized(m); if let Some(p) = p { v = v.with_position(p); } v };
    if let Some(icon) = icon { viewport = viewport.with_icon(icon); }
    let options = eframe::NativeOptions { viewport, centered: prefs.splash, ..Default::default() };
    eframe::run_native(APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::SheetApp::new(cc, path)))))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod chart_corpus;
