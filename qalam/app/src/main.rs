//! Word: a desktop word processor (macOS, Windows, Linux) on the Rust engine.
//!
//! Ported from the JS editor (`documents/src/views/editor.js` and
//! `documents/src/engine/canvas-editor.js`): menus and toolbar in `ui`,
//! the page canvas renderer and input adapter in `canvas`.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod app;
pub use qalam_kit::{i18n, icon_data, theme};
mod canvas;
mod glyphs;
mod insert;
mod picture;
mod extras;
mod commands;
mod story;
mod draw;
mod symbols;
mod text_tools;
mod design;
mod refs;
mod review;
mod view;
mod mailings;
mod print;
mod macros;
mod nav;
mod context;
mod options;
mod panes;
mod ribbon;
mod ruler;
mod ui;
mod update;

/// The app's name (Arabic: قلم, "pen").
pub const APP_NAME: &str = "Qalam";

/// The app's config folder (custom lists, defaults, macros, dictionary, crash log):
/// WORD_CONFIG_DIR when set (tests use a scratch folder), else the platform's location.
pub fn config_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("WORD_CONFIG_DIR") { return std::path::PathBuf::from(d); }
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(if cfg!(target_os = "macos") { "Library/Application Support" } else { ".config" })))
        .unwrap_or_else(std::env::temp_dir);
    // Settings from before the rename move over once.
    let (old, new) = (base.join("Word"), base.join(APP_NAME));
    if !new.exists() && old.is_dir() { let _ = std::fs::rename(&old, &new); }
    new
}

/// Where panics are recorded.
pub fn crash_log_path() -> std::path::PathBuf { config_dir().join("crash.log") }

/// Record every panic with its backtrace before the frame's recovery runs.
fn install_panic_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let path = crash_log_path();
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
        let bt = std::backtrace::Backtrace::force_capture();
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let entry = format!("--- panic at unix time {secs}\n{info}\n{bt}\n");
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) { let _ = f.write_all(entry.as_bytes()); }
        default(info);
    }));
}

/// The splash card's size.
pub const SPLASH_SIZE: [f32; 2] = [440.0, 340.0];
/// The main window's size, position and maximized state, applied when the splash ends.
pub static WINDOW_TARGET: std::sync::OnceLock<([f32; 2], Option<[f32; 2]>, bool)> = std::sync::OnceLock::new();

fn main() -> eframe::Result {
    install_panic_log();
    // Translation models can be installed from a folder (for computers that are never online).
    *qalam_kit::translate_ui::FOLDER_PICKER.lock().unwrap() = Some(|| rfd::FileDialog::new().pick_folder());
    // Files the system opens with Qalam: on the command line, or (macOS Finder) as an event.
    qalam_kit::open_files::install();
    let (path, others) = qalam_kit::open_files::startup_paths(std::env::args().skip(1));
    qalam_kit::open_files::open_elsewhere(&others);
    let icon = image::load_from_memory(include_bytes!("../../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    // The window first appears as a small borderless splash card, then grows into the main
    // window (where it was last time) once fonts have loaded.
    let target = options::initial_window().unwrap_or(([1200.0, 860.0], None, false));
    let _ = WINDOW_TARGET.set(target);
    let mut viewport = egui::ViewportBuilder::default().with_min_inner_size([360.0, 300.0]).with_title(APP_NAME)
        .with_inner_size(SPLASH_SIZE).with_decorations(false).with_transparent(true).with_resizable(false);
    if let Some(icon) = icon { viewport = viewport.with_icon(icon); }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native(APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::WordApp::new(cc, path)))))
}

#[cfg(test)]
mod tests;
