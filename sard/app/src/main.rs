//! Sard (سرد): a desktop presentation editor for .pptx files (macOS, Windows, Linux), a
//! port of the JS presentation editor onto the Rust slides engine, with Qalam's interface.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod anim_play;
mod animations_tab;
mod app;
mod canvas;
mod design_tab;
mod dialogs;
mod home_tab;
mod insert_tab;
mod panes;
mod media;
mod morph;
mod filemenu;
mod prefs;
mod update;
mod nav;
mod ribbon;
mod show;
mod texcache;
mod translate;
mod smartart_ui;
mod table_tab;
mod three_d_ui;
mod transition_fx;
mod views;
mod transitions_tab;
pub use qalam_kit::{i18n, theme};

pub const APP_NAME: &str = "Sard";
/// The name shown to people (Sard, سرد in Arabic).
pub fn app_name() -> String { crate::i18n::tr(APP_NAME) }

/// Settings and recovery files live in the platform's config folder.
pub fn config_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("SLIDES_CONFIG_DIR") { return std::path::PathBuf::from(d); }
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(if cfg!(target_os = "macos") { "Library/Application Support" } else { ".config" })))
        .unwrap_or_else(std::env::temp_dir);
    base.join(APP_NAME)
}

fn main() -> eframe::Result {
    // Files the system opens with Sard: on the command line, or (macOS Finder) as an event.
    qalam_kit::open_files::install();
    let (path, others) = qalam_kit::open_files::startup_paths(std::env::args().skip(1));
    qalam_kit::open_files::open_elsewhere(&others);
    let icon = image::load_from_memory(include_bytes!("../../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    // The window first appears as a small borderless splash card, then grows into the main
    // window (where it was last time).
    // Translation models can be installed from a folder (for computers that are never online).
    *qalam_kit::translate_ui::FOLDER_PICKER.lock().unwrap() = Some(|| rfd::FileDialog::new().pick_folder());
    let prefs = prefs::Prefs::load();
    crate::i18n::set(prefs.lang);
    let mut viewport = egui::ViewportBuilder::default().with_min_inner_size([360.0, 300.0]).with_title(app_name());
    viewport = if prefs.splash { viewport.with_inner_size([440.0, 340.0]).with_decorations(false).with_transparent(true).with_resizable(false) }
        else { let (s, p, m) = prefs.window.unwrap_or(([1280.0, 820.0], None, false)); let mut v = viewport.with_inner_size(s).with_maximized(m); if let Some(p) = p { v = v.with_position(p); } v };
    if let Some(icon) = icon { viewport = viewport.with_icon(icon); }
    let options = eframe::NativeOptions { viewport, centered: prefs.splash, ..Default::default() };
    eframe::run_native(APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::SlidesApp::new(cc, path)))))
}

#[cfg(test)]
mod tests;
