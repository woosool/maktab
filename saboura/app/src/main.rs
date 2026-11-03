//! Saboura (سبورة): a desktop whiteboard (macOS, Windows, Linux), a port of the JS whiteboard
//! onto the Rust board engine, with Qalam's interface.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
mod canvas;
mod commands;
mod export;
mod panels;
mod dialogs;
mod icons;
mod library;
mod rtl;
mod toolbar;
mod update;

pub use qalam_kit::{i18n, theme};

pub const APP_NAME: &str = "Saboura";
/// The name shown to people (Saboura, سبورة in Arabic).
/// A column aligned to the reading side.
pub fn rtl_column() -> egui::Layout { egui::Layout::top_down(if i18n::rtl() { egui::Align::Max } else { egui::Align::Min }) }

pub fn app_name() -> String { crate::i18n::tr(APP_NAME) }

/// Settings live in the platform's config folder.
pub fn config_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("SABOURA_CONFIG_DIR") { return std::path::PathBuf::from(d); }
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(if cfg!(target_os = "macos") { "Library/Application Support" } else { ".config" })))
        .unwrap_or_else(std::env::temp_dir);
    base.join(APP_NAME)
}

fn main() -> eframe::Result {
    qalam_kit::open_files::install();
    let (path, others) = qalam_kit::open_files::startup_paths(std::env::args().skip(1));
    qalam_kit::open_files::open_elsewhere(&others);
    let icon = image::load_from_memory(include_bytes!("../../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    // The window first appears as a small borderless splash card, then grows into the main window.
    let mut viewport = egui::ViewportBuilder::default().with_min_inner_size([360.0, 300.0]).with_title(app_name())
        .with_inner_size([440.0, 340.0]).with_decorations(false).with_transparent(true).with_resizable(false);
    if let Some(icon) = icon { viewport = viewport.with_icon(icon); }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native(APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::BoardApp::new(cc, path)))))
}

#[cfg(test)]
mod tests;
