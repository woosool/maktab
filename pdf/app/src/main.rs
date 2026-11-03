//! Waraq (ورق): a desktop PDF editor.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod app;
mod canvas;
mod editing;
mod forms;
mod i18n;
mod organize;
mod prefs;
mod raster;
mod redact;
mod signatures;
mod splash;
mod theme;
mod update;

#[cfg(test)]
mod visual_tour;

pub const APP_NAME: &str = "Waraq";
pub const APP_NAME_AR: &str = "ورق";
pub const PRODUCT_PAGE: &str = "https://woosool.com/en/products/waraq";

/// The app's config folder (settings, saved signatures): WARAQ_CONFIG_DIR when set (tests use a
/// scratch folder), else the platform's location. Data from before the rename moves over once.
pub fn config_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("WARAQ_CONFIG_DIR") {
        return std::path::PathBuf::from(d);
    }
    let base = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                std::path::PathBuf::from(h).join(if cfg!(target_os = "macos") {
                    "Library/Application Support"
                } else {
                    ".config"
                })
            })
        })
        .unwrap_or_else(std::env::temp_dir);
    let new = base.join(APP_NAME);
    migrate_old_signatures(&new);
    new
}

/// Signatures saved by the editor when it was called "Maktab PDF".
fn migrate_old_signatures(new: &std::path::Path) {
    let Some(old) = directories::ProjectDirs::from("app", "Maktab", "Maktab PDF")
        .map(|d| d.data_dir().join("signatures"))
    else {
        return;
    };
    let target = new.join("signatures");
    if old.is_dir() && !target.exists() {
        let _ = std::fs::create_dir_all(new);
        if std::fs::rename(&old, &target).is_err()
            && std::fs::create_dir_all(&target).is_ok()
            && let Ok(entries) = std::fs::read_dir(&old)
        {
            for e in entries.flatten() {
                let _ = std::fs::copy(e.path(), target.join(e.file_name()));
            }
        }
    }
}

/// The app icon (assets/icon.png, rendered from assets/icon.svg by tools/render-icon).
pub const ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

fn icon_data() -> Option<eframe::egui::IconData> {
    let i = image::load_from_memory(ICON_PNG).ok()?.to_rgba8();
    Some(eframe::egui::IconData {
        width: i.width(),
        height: i.height(),
        rgba: i.into_raw(),
    })
}

/// The icon as a texture, for the splash card and About.
pub fn icon_texture(ctx: &eframe::egui::Context) -> Option<eframe::egui::TextureHandle> {
    // Kept in the context's memory: a texture belongs to one context.
    let id = eframe::egui::Id::unique("waraq-app-icon");
    if let Some(t) = ctx.data(|d| d.get_temp::<eframe::egui::TextureHandle>(id)) {
        return Some(t);
    }
    let i = image::load_from_memory(ICON_PNG)
        .ok()?
        .resize(256, 256, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let img = eframe::egui::ColorImage::from_rgba_unmultiplied(
        [i.width() as usize, i.height() as usize],
        i.as_raw(),
    );
    let t = ctx.load_texture("app-icon", img, eframe::egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

/// The window at startup: the borderless splash card (or, with the splash off, the main
/// window where it was last time). Nothing else may size the window before the splash ends.
pub fn native_options(settings: &prefs::Settings) -> eframe::NativeOptions {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_app_id("waraq")
        .with_min_inner_size([360.0, 300.0])
        .with_drag_and_drop(true);
    viewport = if settings.splash {
        viewport
            .with_inner_size(splash::SIZE)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
    } else {
        let (s, p, m) = settings.window.unwrap_or(([1360.0, 880.0], None, false));
        let mut v = viewport.with_inner_size(s).with_maximized(m);
        if let Some(p) = p {
            v = v.with_position(p);
        }
        v
    };
    if let Some(icon) = icon_data() {
        viewport = viewport.with_icon(icon);
    }
    eframe::NativeOptions {
        viewport,
        centered: settings.splash,
        // Waraq restores the window itself (settings.window); eframe's restore would reopen
        // the splash card at the last full window size.
        persist_window: false,
        ..Default::default()
    }
}

fn main() -> eframe::Result {
    update::clean_old();
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let settings = prefs::Settings::load();
    i18n::set(settings.lang);
    let options = native_options(&settings);
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, path, true)))),
    )
}

#[cfg(test)]
mod startup_tests {
    #[test]
    fn startup_window_is_the_splash_card() {
        let s = crate::prefs::Settings {
            window: Some(([1800.0, 1100.0], Some([0.0, 0.0]), true)),
            ..Default::default()
        };
        let o = crate::native_options(&s);
        assert_eq!(
            o.viewport.inner_size,
            Some(eframe::egui::vec2(
                crate::splash::SIZE[0],
                crate::splash::SIZE[1]
            ))
        );
        assert_eq!(o.viewport.decorations, Some(false));
        assert_eq!(
            o.viewport.maximized, None,
            "the saved maximized state waits for the splash to end"
        );
        assert!(
            !o.persist_window,
            "eframe must not restore the last window over the splash"
        );
        assert!(o.centered);
        // With the splash off, the saved window is used directly.
        let o = crate::native_options(&crate::prefs::Settings { splash: false, ..s });
        assert_eq!(
            o.viewport.inner_size,
            Some(eframe::egui::vec2(1800.0, 1100.0))
        );
        assert_eq!(o.viewport.maximized, Some(true));
    }
}
