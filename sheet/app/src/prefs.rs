//! Jadwal's settings (settings.json in the config folder): language, dark mode, the window,
//! recent files and AutoRecover; the Options dialog; and AutoRecover (a copy of unsaved work
//! offered back after a crash).

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::RichText;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub lang: crate::i18n::Lang,
    pub dark: bool,
    pub recent: Vec<PathBuf>,
    /// AutoRecover every this many minutes (0 = off).
    pub autosave_minutes: u32,
    pub author: String,
    /// After Enter the selection moves: 0 down, 1 right, 2 up, 3 left, 4 stays.
    pub enter_moves: u8,
    pub show_formula_bar: bool,
    /// Page Layout view's rulers: 0 inches, 1 centimeters, 2 millimeters.
    pub ruler_units: u8,
    /// The window: size, position and maximized, remembered for next time.
    pub window: Option<([f32; 2], Option<[f32; 2]>, bool)>,
    /// Show the splash card at start.
    pub splash: bool,
    /// Look for a new version once a day; a version the user chose to skip; when we last looked.
    pub auto_update: bool,
    pub skipped_version: Option<String>,
    pub last_update_check: u64,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { lang: Default::default(), dark: false, recent: vec![], autosave_minutes: 5, author: std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default(), enter_moves: 0, show_formula_bar: true, ruler_units: 0, window: None, splash: true, auto_update: true, skipped_version: None, last_update_check: 0 }
    }
}

fn file() -> PathBuf { crate::config_dir().join("settings.json") }

impl Prefs {
    pub fn load() -> Prefs {
        let d = Prefs::default();
        let Ok(text) = std::fs::read_to_string(file()) else { return d };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return d };
        let f2 = |x: &serde_json::Value| -> Option<[f32; 2]> { Some([x.get(0)?.as_f64()? as f32, x.get(1)?.as_f64()? as f32]) };
        Prefs {
            lang: crate::i18n::Lang::from_code(v["lang"].as_str().unwrap_or("en")),
            dark: v["dark"].as_bool().unwrap_or(d.dark),
            recent: v["recent"].as_array().map(|a| a.iter().filter_map(|p| p.as_str().map(PathBuf::from)).collect()).unwrap_or_default(),
            autosave_minutes: v["autosave_minutes"].as_u64().map(|m| m as u32).unwrap_or(d.autosave_minutes),
            author: v["author"].as_str().map(String::from).unwrap_or(d.author),
            enter_moves: v["enter_moves"].as_u64().map(|m| m.min(4) as u8).unwrap_or(0),
            show_formula_bar: v["show_formula_bar"].as_bool().unwrap_or(true),
            ruler_units: v["ruler_units"].as_u64().map(|m| m.min(2) as u8).unwrap_or(0),
            window: v.get("window").and_then(|w| Some((f2(&w["size"])?, f2(&w["pos"]), w["maximized"].as_bool().unwrap_or(false)))),
            splash: v["splash"].as_bool().unwrap_or(true),
            auto_update: v["auto_update"].as_bool().unwrap_or(true), skipped_version: v["skipped_version"].as_str().map(String::from), last_update_check: v["last_update_check"].as_u64().unwrap_or(0),
        }
    }

    pub fn save(&self) {
        if cfg!(test) { return; }
        let window = self.window.map(|(s, p, m)| serde_json::json!({ "size": s, "pos": p, "maximized": m }));
        let v = serde_json::json!({
            "lang": self.lang.code(), "dark": self.dark, "recent": self.recent.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
            "autosave_minutes": self.autosave_minutes, "author": self.author, "enter_moves": self.enter_moves, "show_formula_bar": self.show_formula_bar, "ruler_units": self.ruler_units, "window": window,
            "splash": self.splash, "auto_update": self.auto_update, "skipped_version": self.skipped_version, "last_update_check": self.last_update_check,
        });
        let _ = std::fs::create_dir_all(crate::config_dir());
        let _ = std::fs::write(file(), serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    /// Put a file at the top of the recent list (ten kept).
    pub fn add_recent(&mut self, p: &std::path::Path) {
        let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        self.recent.retain(|r| r != &p);
        self.recent.insert(0, p);
        self.recent.truncate(10);
        self.save();
    }
}

// ---- AutoRecover --------------------------------------------------------------------------

pub fn recovery_file() -> PathBuf { crate::config_dir().join("AutoRecover.xlsx") }
fn recovery_note() -> PathBuf { crate::config_dir().join("AutoRecover.txt") }

impl SheetApp {
    /// Save a recovery copy of unsaved changes every few minutes.
    pub fn autosave(&mut self) {
        if self.prefs.autosave_minutes == 0 || !self.dirty || cfg!(test) { return; }
        let now = self.ctx.input(|i| i.time);
        if now - self.last_autosave < self.prefs.autosave_minutes as f64 * 60.0 { return; }
        self.last_autosave = now;
        if let Ok(bytes) = sheet_engine::xlsx::write(&self.book) {
            let _ = std::fs::create_dir_all(crate::config_dir());
            let _ = std::fs::write(recovery_file(), bytes);
            let _ = std::fs::write(recovery_note(), self.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
        }
    }
    pub fn clear_recovery(&mut self) {
        let _ = std::fs::remove_file(recovery_file());
        let _ = std::fs::remove_file(recovery_note());
    }
    pub fn offer_recovery(&mut self) {
        if cfg!(test) || !recovery_file().is_file() { return; }
        let original = std::fs::read_to_string(recovery_note()).ok().filter(|s| !s.is_empty()).map(PathBuf::from);
        self.recover = Some(original);
    }
    pub fn apply_prefs(&mut self, p: Prefs) {
        crate::i18n::set(p.lang);
        self.dark = p.dark;
        self.prefs = p;
        self.prefs.save();
    }
}

/// The recovered-work prompt.
pub fn recovery_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(original) = app.recover.clone() else { return };
    let mut done = false;
    crate::theme::dialog(ctx, "Document Recovery").collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.label(RichText::new(tr("Jadwal closed before your changes were saved.")).strong());
        let name = original.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Book1"));
        ui.label(crate::i18n::trf("A recovered copy of {} is available.", &[&name]));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Open Recovered").clicked() {
                if let Ok(b) = sheet_engine::open(&recovery_file()) {
                    app.load(b, original.clone());
                    app.dirty = true;
                }
                done = true;
            }
            if crate::theme::secondary(ui, "Discard").clicked() { app.clear_recovery(); done = true; }
        });
    });
    if done { app.recover = None; }
}

/// File ▸ Options.
pub fn options_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut p) = app.options.take() else { return };
    let mut open = true;
    let mut done = false;
    let mut cancel = false;
    crate::theme::dialog(ctx, "Jadwal Options").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_min_width(420.0);
        crate::theme::section(ui, "General");
        crate::theme::grid(ui, egui::Grid::new("opts").num_columns(2).spacing(egui::vec2(12.0, 8.0)), |ui| {
            ui.label(tr("Display language:"));
            crate::theme::Combo::from_id_salt("opt-lang").selected_text(p.lang.name()).show_ui(ui, |ui| for (l, _) in crate::i18n::Lang::ALL { ui.selectable_value(&mut p.lang, l, l.name()); });
            ui.end_row();
            ui.label(tr("User name:")); ui.text_edit_singleline(&mut p.author); ui.end_row();
            ui.label(tr("Theme:"));
            ui.horizontal(|ui| { ui.selectable_value(&mut p.dark, false, tr("Light")); ui.selectable_value(&mut p.dark, true, tr("Dark")); });
            ui.end_row();
            ui.label(tr("After pressing Enter, move:"));
            let dirs = ["Down", "Right", "Up", "Left", "Don't move"];
            crate::theme::Combo::from_id_salt("opt-enter").selected_text(tr(dirs[p.enter_moves as usize])).show_ui(ui, |ui| for (i, d) in dirs.iter().enumerate() { ui.selectable_value(&mut p.enter_moves, i as u8, tr(d)); });
            ui.end_row();
            ui.label(tr("Ruler units:"));
            let units = ["Inches", "Centimeters", "Millimeters"];
            crate::theme::Combo::from_id_salt("opt-ruler").selected_text(tr(units[p.ruler_units.min(2) as usize])).show_ui(ui, |ui| for (i, d) in units.iter().enumerate() { ui.selectable_value(&mut p.ruler_units, i as u8, tr(d)); });
            ui.end_row();
        });
        crate::theme::check(ui, &mut p.show_formula_bar, tr("Show the formula bar"));
        crate::theme::check(ui, &mut p.splash, tr("Show the start screen"));
        crate::theme::section(ui, "Save");
        crate::theme::hrow(ui, |ui| {
            let mut on = p.autosave_minutes > 0;
            if crate::theme::check(ui, &mut on, tr("Save AutoRecover information every")).changed() { p.autosave_minutes = if on { 5 } else { 0 }; }
            ui.add_enabled(on, egui::DragValue::new(&mut p.autosave_minutes).range(1..=120));
            ui.label(tr("minutes"));
        });
        if ui.button(tr("Clear Recent Files")).clicked() { p.recent.clear(); }
        crate::formulas_tab::options_section(app, ui);
        qalam_kit::default_apps::options_section(ui, &qalam_kit::default_apps::JADWAL);
        crate::theme::section(ui, "Updates");
        crate::theme::check(ui, &mut p.auto_update, tr("Check for updates automatically"));
        crate::theme::hrow(ui, |ui| {
            ui.label(crate::i18n::trf("Version {}", &[&crate::update::CURRENT]));
            if ui.button(tr("Check for Updates…")).clicked() { crate::update::check(ui.ctx(), true, None); }
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { done = true; app.apply_prefs(p.clone()); }
            if crate::theme::secondary(ui, "Cancel").clicked() { cancel = true; }
        });
    });
    if open && !done && !cancel { app.options = Some(p); }
}
