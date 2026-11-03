//! File ▸ Options: general preferences, save options (AutoRecover, backup copies, default
//! folder), the window's size and position between sessions, and customizing the ribbon (tabs,
//! groups, a custom tab of commands) and the right-click menu. Settings are kept in
//! settings.json in the app's config folder.

use crate::app::WordApp;
use crate::insert::Job;
use egui::RichText;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::PathBuf;

/// The ribbon's tabs that can be hidden.
pub const TABS: &[&str] = &["Home", "Insert", "Draw", "Design", "Layout", "References", "Mailings", "Review", "View"];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub lang: crate::i18n::Lang,
    pub dark: bool,
    pub units: crate::view::Units,
    pub ruler: bool,
    pub nav_pane: bool,
    pub zoom: f32,
    pub autosave_minutes: u32,
    pub backup: bool,
    pub default_folder: Option<String>,
    pub remember_window: bool,
    /// Window inner size, position and maximized state from last time.
    pub window: Option<([f32; 2], Option<[f32; 2]>, bool)>,
    pub hidden_tabs: HashSet<String>,
    /// "Tab/Group" names hidden from the ribbon.
    pub hidden_groups: HashSet<String>,
    /// Commands (by name, from the command list) on the custom "My Tab", if any.
    pub custom_tab: Vec<String>,
    pub custom_tab_name: String,
    pub hidden_context: HashSet<String>,
    pub recent: Vec<String>,
    /// Look for new versions once a day.
    pub auto_update: bool,
    pub skipped_version: Option<String>,
    /// When updates were last checked (Unix seconds).
    pub last_update_check: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { lang: Default::default(), dark: false, units: Default::default(), ruler: true, nav_pane: false, zoom: 1.0, autosave_minutes: 10, backup: false, default_folder: None, remember_window: true, window: None, hidden_tabs: HashSet::new(), hidden_groups: HashSet::new(), custom_tab: vec![], custom_tab_name: "My Tab".into(), hidden_context: HashSet::new(), recent: vec![], auto_update: true, skipped_version: None, last_update_check: 0 }
    }
}

fn path() -> PathBuf { crate::config_dir().join("settings.json") }

impl Settings {
    pub fn load() -> Settings {
        let Ok(text) = std::fs::read_to_string(path()) else { return Settings::default() };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { return Settings::default() };
        let d = Settings::default();
        let set = |k: &str| -> HashSet<String> { v[k].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect() };
        let list = |k: &str| -> Vec<String> { v[k].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect() };
        let units = crate::view::Units::ALL.iter().find(|u| Some(u.1) == v["units"].as_str()).map(|u| u.0).unwrap_or_default();
        let window = v["window"].as_object().and_then(|w| {
            let size = [w.get("w")?.as_f64()? as f32, w.get("h")?.as_f64()? as f32];
            let pos = w.get("x").and_then(|x| x.as_f64()).zip(w.get("y").and_then(|y| y.as_f64())).map(|(x, y)| [x as f32, y as f32]);
            Some((size, pos, w.get("maximized").and_then(|m| m.as_bool()).unwrap_or(false)))
        });
        Settings {
            lang: crate::i18n::Lang::from_code(v["lang"].as_str().unwrap_or("en")), dark: v["dark"].as_bool().unwrap_or(d.dark), units, ruler: v["ruler"].as_bool().unwrap_or(d.ruler), nav_pane: v["nav_pane"].as_bool().unwrap_or(false),
            zoom: v["zoom"].as_f64().map(|z| z as f32).unwrap_or(1.0).clamp(0.25, 3.0),
            autosave_minutes: v["autosave_minutes"].as_u64().map(|m| m as u32).unwrap_or(d.autosave_minutes), backup: v["backup"].as_bool().unwrap_or(false),
            default_folder: v["default_folder"].as_str().map(String::from), remember_window: v["remember_window"].as_bool().unwrap_or(true), window,
            hidden_tabs: set("hidden_tabs"), hidden_groups: set("hidden_groups"), custom_tab: list("custom_tab"),
            custom_tab_name: v["custom_tab_name"].as_str().unwrap_or("My Tab").to_string(), hidden_context: set("hidden_context"), recent: list("recent"),
            auto_update: v["auto_update"].as_bool().unwrap_or(true), skipped_version: v["skipped_version"].as_str().map(String::from), last_update_check: v["last_update_check"].as_u64().unwrap_or(0),
        }
    }

    pub fn save(&self) {
        fn sorted(s: &HashSet<String>) -> Vec<String> { let mut v: Vec<String> = s.iter().cloned().collect(); v.sort(); v }
        let window = self.window.map(|(s, p, m)| json!({ "w": s[0], "h": s[1], "x": p.map(|p| p[0]), "y": p.map(|p| p[1]), "maximized": m }));
        let v = json!({
            "lang": self.lang.code(), "dark": self.dark, "units": self.units.name(), "ruler": self.ruler, "nav_pane": self.nav_pane, "zoom": self.zoom,
            "autosave_minutes": self.autosave_minutes, "backup": self.backup, "default_folder": self.default_folder, "remember_window": self.remember_window,
            "window": window, "hidden_tabs": sorted(&self.hidden_tabs), "hidden_groups": sorted(&self.hidden_groups), "custom_tab": self.custom_tab,
            "custom_tab_name": self.custom_tab_name, "hidden_context": sorted(&self.hidden_context), "recent": self.recent,
            "auto_update": self.auto_update, "skipped_version": self.skipped_version, "last_update_check": self.last_update_check,
        });
        let p = path();
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, serde_json::to_string_pretty(&v).unwrap_or_default());
    }
}

#[derive(Default)]
pub struct OptionsState {
    pub settings: Settings,
    pub dialog: Option<(u8, Settings)>,
    pub hidden_context: HashSet<String>,
    pub synonyms: Option<(String, Job<Vec<String>>, Option<Result<Vec<String>, String>>)>,
    /// Last window state seen, and when it changed (saved a moment later).
    pub window_seen: Option<([f32; 2], Option<[f32; 2]>, bool)>,
    pub window_changed: Option<f64>,
    /// Ribbon-set preferences last saved: dark, units, ruler, navigation pane, zoom.
    pub prefs_seen: Option<(bool, crate::view::Units, bool, bool, i32)>,
    pub last_autosave: f64,
    pub recovered: Option<Vec<PathBuf>>,
    pub applied: bool,
}

fn recovery_dir() -> PathBuf { crate::config_dir().join("AutoRecover") }

impl WordApp {
    /// Apply saved settings once at start, and offer documents AutoRecover saved before a crash.
    pub fn apply_settings(&mut self, ctx: &egui::Context) {
        if self.extras.options.applied { return; }
        self.extras.options.applied = true;
        let s = Settings::load();
        self.show_ruler = s.ruler;
        self.show_outline = s.nav_pane;
        self.extras.view.units = s.units;
        if (s.zoom - 1.0).abs() > 0.01 { self.set_zoom(s.zoom); }
        if s.dark { self.set_dark(ctx, true); }
        self.extras.options.hidden_context = s.hidden_context.clone();
        crate::i18n::set(s.lang);
        self.extras.options.settings = s;
        let files: Vec<PathBuf> = std::fs::read_dir(recovery_dir()).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "docx")).collect();
        if !files.is_empty() { self.extras.options.recovered = Some(files); }
    }

    fn persist_settings(&mut self) {
        let s = &mut self.extras.options.settings;
        s.dark = self.extras.view.dark;
        s.units = self.extras.view.units;
        s.ruler = self.show_ruler;
        s.nav_pane = self.show_outline;
        s.zoom = self.zoom;
        s.hidden_context = self.extras.options.hidden_context.clone();
        s.save();
    }

    /// Each frame: remember the window's size and position (saved a second after it settles),
    /// and AutoRecover the document every few minutes while it has unsaved changes.
    pub fn track_window_and_autosave(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let state = ctx.input(|i| { let v = i.viewport(); v.inner_rect.map(|r| ([r.width(), r.height()], v.outer_rect.map(|o| [o.min.x, o.min.y]), v.maximized.unwrap_or(false))) });
        if let Some(st) = state {
            let o = &mut self.extras.options;
            if o.window_seen != Some(st) { o.window_seen = Some(st); o.window_changed = Some(now); }
            if o.window_changed.is_some_and(|t| now - t > 1.0) {
                o.window_changed = None;
                if o.settings.remember_window { o.settings.window = Some(st); }
                self.persist_settings();
            }
        }
        // Preferences changed from the ribbon are saved too.
        let prefs = (self.extras.view.dark, self.extras.view.units, self.show_ruler, self.show_outline, (self.zoom * 100.0).round() as i32);
        let o = &mut self.extras.options;
        if o.prefs_seen != Some(prefs) { if o.prefs_seen.is_some() { o.window_changed = Some(now); } o.prefs_seen = Some(prefs); }
        let minutes = self.extras.options.settings.autosave_minutes;
        if minutes > 0 && self.doc.dirty && now - self.extras.options.last_autosave > minutes as f64 * 60.0 {
            self.extras.options.last_autosave = now;
            if let Ok(bytes) = word_engine::docx::write(&self.doc) {
                let _ = std::fs::create_dir_all(recovery_dir());
                let _ = std::fs::write(self.recovery_file(), bytes);
            }
        }
    }

    fn recovery_file(&self) -> PathBuf {
        let name = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
        recovery_dir().join(format!("{name} ({}).docx", std::process::id()))
    }

    /// After a successful save or a deliberate close: the AutoRecover copy isn't needed.
    pub fn clear_recovery(&self) { let _ = std::fs::remove_file(self.recovery_file()); }

    /// Before saving over a file: keep the previous version as "Backup of NAME.wbk".
    pub fn backup_before_save(&self, path: &std::path::Path) {
        if !self.extras.options.settings.backup || !path.exists() { return; }
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let _ = std::fs::copy(path, path.with_file_name(format!("Backup of {name}.wbk")));
    }

    pub fn remember_recent(&mut self, path: &std::path::Path) {
        let p = path.display().to_string();
        let r = &mut self.extras.options.settings.recent;
        r.retain(|x| *x != p);
        r.insert(0, p);
        r.truncate(10);
        self.persist_settings();
    }

    pub fn tab_hidden(&self, tab: &str) -> bool { self.extras.options.settings.hidden_tabs.contains(tab) }
}

/// The saved window size and position for the next start.
pub fn initial_window() -> Option<([f32; 2], Option<[f32; 2]>, bool)> {
    let s = Settings::load();
    if s.remember_window { s.window } else { None }
}

/// Groups of each tab, for Customize Ribbon (as the ribbon names them).
pub const GROUPS: &[(&str, &[&str])] = &[
    ("Home", &["Clipboard", "Font", "Paragraph", "Styles", "Panes", "Voice", "Editing"]),
    ("Insert", &["Pages", "Tables", "Illustrations", "Links", "Comments", "Header & Footer", "Text", "Symbols"]),
    ("Draw", &["Tools", "Touch", "Convert", "Insert", "Replay"]),
    ("Design", &["Themes", "Document Formatting", "Colors & Fonts", "Page Background"]),
    ("Layout", &["Page Setup", "Paragraph", "Arrange"]),
    ("References", &["Table of Contents", "Footnotes", "Research", "Citations & Bibliography", "Captions", "Index", "Table of Authorities"]),
    ("Mailings", &["Create", "Start Mail Merge", "Write & Insert Fields", "Preview Results", "Finish"]),
    ("Review", &["Proofing", "Speech", "Accessibility", "Language", "Comments", "Tracking", "Changes", "Compare", "Protect", "Ink"]),
    ("View", &["Views", "Immersive", "Show", "Zoom", "Window", "Macros", "Properties", "Dark Mode"]),
];

pub fn dialog(app: &mut WordApp, ctx: &egui::Context) {
    use crate::theme::{dialog, footer, primary, secondary, section};
    if let Some(files) = app.extras.options.recovered.clone() {
        let (mut open, mut close) = (true, false);
        let mut pick = None;
        dialog(ctx, "Document Recovery").open(&mut open).default_width(460.0).show(ctx, |ui| {
            ui.label(crate::i18n::trf("{} saved these documents before it closed unexpectedly:", &[&crate::APP_NAME]));
            for f in &files {
                crate::theme::hrow(ui, |ui| {
                    ui.label(f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                    if ui.button(crate::i18n::tr("Open")).clicked() { pick = Some(f.clone()); }
                });
            }
            footer(ui, |ui| { if secondary(ui, "Discard All").clicked() { for f in &files { let _ = std::fs::remove_file(f); } close = true; } if secondary(ui, "Close").clicked() { close = true; } });
        });
        if let Some(f) = pick {
            if let Ok(doc) = std::fs::read(&f).map_err(|e| e.to_string()).and_then(|b| word_engine::docx::read(&b)) { app.set_document(doc, None); app.doc.dirty = true; }
            let _ = std::fs::remove_file(&f);
            let left: Vec<PathBuf> = files.into_iter().filter(|x| *x != f).collect();
            app.extras.options.recovered = if left.is_empty() { None } else { Some(left) };
        } else if !open || close { app.extras.options.recovered = None; }
    }
    let Some((mut page, mut s)) = app.extras.options.dialog.take() else { return };
    let (mut open, mut ok, mut cancel) = (true, false, false);
    let mut hidden_context = app.extras.options.hidden_context.clone();
    dialog(ctx, crate::i18n::trf("{} Options", &[&crate::APP_NAME])).open(&mut open).default_width(760.0).show(ctx, |ui| {
        app.canvas_focus = false;
        // Pages as a row of tabs; the page scrolls so the dialog always fits on screen.
        crate::theme::hrow(ui, |ui| {
            for (k, n) in ["General", "Save", "Customize Ribbon", "Context Menu"].iter().enumerate() {
                let on = page == k as u8;
                let r = ui.add(egui::Button::new(RichText::new(crate::i18n::tr(n)).color(if on { crate::ribbon::ACCENT } else { crate::ribbon::c_ink2() })).frame(false).min_size(egui::vec2(0.0, 30.0)));
                if on { ui.painter().line_segment([r.rect.left_bottom(), r.rect.right_bottom()], egui::Stroke::new(2.0, crate::ribbon::ACCENT)); }
                if r.clicked() { page = k as u8; }
            }
        });
        ui.separator();
        let max_h = (ui.ctx().content_rect().height() - 260.0).clamp(200.0, 520.0);
        egui::ScrollArea::vertical().id_salt("options-page").max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
            crate::theme::vcol(ui, |ui| {
                ui.set_width(700.0);
                match page {
                    0 => {
                        section(ui, "Language");
                        crate::theme::hrow(ui, |ui| {
                            ui.label(crate::i18n::tr("Display language:"));
                            crate::theme::Combo::from_id_salt("opt-lang").selected_text(crate::i18n::tr(&(s.lang.name()))).show_ui(ui, |ui| { for (l, _) in crate::i18n::Lang::ALL { ui.selectable_value(&mut s.lang, l, crate::i18n::tr(&l.name())); } });
                        });
                        ui.label(RichText::new(crate::i18n::tr("Arabic uses a right-to-left layout.")).small().weak());
                        section(ui, "Personalize");
                        crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("User name:")); ui.text_edit_singleline(&mut app.author); });
                        crate::theme::check(ui, &mut s.dark, crate::i18n::tr("Use Dark Mode"));
                        crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Show measurements in units of:")); crate::theme::Combo::from_id_salt("opt-units").selected_text(crate::i18n::tr(&(s.units.name()))).show_ui(ui, |ui| { for (u, n) in crate::view::Units::ALL { ui.selectable_value(&mut s.units, u, crate::i18n::tr(&n)); } }); });
                        section(ui, "Start up");
                        crate::theme::check(ui, &mut s.ruler, crate::i18n::tr("Show the ruler"));
                        crate::theme::check(ui, &mut s.nav_pane, crate::i18n::tr("Show the Navigation pane"));
                        crate::theme::check(ui, &mut s.remember_window, crate::i18n::tr("Remember the window's size and position"));
                        crate::theme::hrow(ui, |ui| {
                            crate::theme::check(ui, &mut s.auto_update, crate::i18n::tr("Check for updates automatically"));
                            if crate::theme::secondary(ui, "Check Now").clicked() { crate::update::check(ui.ctx(), true, None); }
                        });
                        qalam_kit::default_apps::options_section(ui, &qalam_kit::default_apps::QALAM);
                    }
                    1 => {
                        section(ui, "Save documents");
                        crate::theme::hrow(ui, |ui| {
                            let mut on = s.autosave_minutes > 0;
                            if crate::theme::check(ui, &mut on, crate::i18n::tr("Save AutoRecover information every")).changed() { s.autosave_minutes = if on { 10 } else { 0 }; }
                            ui.add_enabled(on, egui::DragValue::new(&mut s.autosave_minutes).range(1..=120));
                            ui.label(crate::i18n::tr("minutes"));
                        });
                        ui.label(RichText::new(crate::i18n::trf("AutoRecover files: {}", &[&recovery_dir().display()])).small().weak());
                        crate::theme::check(ui, &mut s.backup, crate::i18n::tr("Always create a backup copy (Backup of NAME.wbk)"));
                        crate::theme::hrow(ui, |ui| {
                            ui.label(crate::i18n::tr("Default local file location:"));
                            ui.label(RichText::new(s.default_folder.clone().unwrap_or_else(|| "(last used folder)".into())).weak());
                            if ui.button(crate::i18n::tr("Browse…")).clicked() { if let Some(d) = rfd::FileDialog::new().pick_folder() { s.default_folder = Some(d.display().to_string()); } }
                            if s.default_folder.is_some() && ui.button(crate::i18n::tr("Clear")).clicked() { s.default_folder = None; }
                        });
                        section(ui, "Recent documents");
                        if s.recent.is_empty() { ui.label(RichText::new(crate::i18n::tr("None yet.")).weak()); }
                        for r in s.recent.iter().take(5) { ui.label(RichText::new(crate::i18n::tr(&r)).small()); }
                        if !s.recent.is_empty() && ui.button(crate::i18n::tr("Clear the list")).clicked() { s.recent.clear(); }
                    }
                    2 => {
                        ui.label(RichText::new(crate::i18n::tr("Choose which tabs and groups the ribbon shows, and build your own tab of commands.")).weak());
                        crate::theme::hcols(ui, |ui| {
                            crate::theme::vcol(ui, |ui| {
                                ui.set_width(250.0);
                                section(ui, "Main Tabs");
                                egui::ScrollArea::vertical().id_salt("opt-tabs").max_height(330.0).show(ui, |ui| {
                                    for tab in TABS {
                                        let mut on = !s.hidden_tabs.contains(*tab);
                                        if crate::theme::check(ui, &mut on, RichText::new(crate::i18n::tr(tab)).strong()).changed() { if on { s.hidden_tabs.remove(*tab); } else { s.hidden_tabs.insert(tab.to_string()); } }
                                        if let Some((_, groups)) = GROUPS.iter().find(|g| g.0 == *tab) {
                                            for g in *groups {
                                                let key = format!("{tab}/{g}");
                                                let mut on = !s.hidden_groups.contains(&key);
                                                crate::theme::hrow(ui, |ui| { ui.add_space(22.0); if crate::theme::check(ui, &mut on, crate::i18n::tr(g)).changed() { if on { s.hidden_groups.remove(&key); } else { s.hidden_groups.insert(key.clone()); } } });
                                            }
                                        }
                                    }
                                });
                            });
                            ui.add_space(12.0);
                            crate::theme::vcol(ui, |ui| {
                                section(ui, "Custom tab");
                                crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Name:")); ui.add(egui::TextEdit::singleline(&mut s.custom_tab_name).desired_width(160.0)); });
                                ui.label(RichText::new(crate::i18n::tr("Commands on it (click to remove):")).small().weak());
                                let mut remove = None;
                                for (k, c) in s.custom_tab.iter().enumerate() { if ui.button(format!("✕  {c}")).clicked() { remove = Some(k); } }
                                if let Some(k) = remove { s.custom_tab.remove(k); }
                                if s.custom_tab.is_empty() { ui.label(RichText::new(crate::i18n::tr("Empty: the tab is hidden.")).small().weak()); }
                                crate::theme::submenu(ui, crate::i18n::tr("Add a command…"), |ui| {
                                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                        for c in crate::commands::all() { if !s.custom_tab.iter().any(|x| x == c.name) && ui.button(crate::i18n::tr(&c.name)).clicked() { s.custom_tab.push(c.name.to_string()); ui.close(); } }
                                    });
                                });
                                ui.add_space(8.0);
                                if ui.button(crate::i18n::tr("Reset the ribbon")).clicked() { s.hidden_tabs.clear(); s.hidden_groups.clear(); }
                            });
                        });
                    }
                    _ => {
                        ui.label(RichText::new(crate::i18n::tr("Choose what the right-click menu shows on text.")).weak());
                        for (id, label) in crate::context::ITEMS {
                            let mut on = !hidden_context.contains(*id);
                            if crate::theme::check(ui, &mut on, crate::i18n::tr(label)).changed() { if on { hidden_context.remove(*id); } else { hidden_context.insert(id.to_string()); } }
                        }
                        if ui.button(crate::i18n::tr("Reset")).clicked() { hidden_context.clear(); }
                    }
                }
            });
        });
        footer(ui, |ui| { if primary(ui, "OK").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
    });
    if ok {
        let ctx2 = ctx.clone();
        if s.dark != app.extras.view.dark { app.set_dark(&ctx2, s.dark); }
        app.extras.view.units = s.units;
        app.show_ruler = s.ruler;
        app.show_outline = s.nav_pane;
        app.extras.options.hidden_context = hidden_context;
        app.extras.options.settings = s;
        app.persist_settings();
        app.canvas_focus = true;
    } else if open && !cancel {
        app.extras.options.hidden_context = hidden_context;
        app.extras.options.dialog = Some((page, s));
    } else {
        app.extras.options.hidden_context = app.extras.options.settings.hidden_context.clone();
        app.canvas_focus = true;
    }
}
