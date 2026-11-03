//! Macros (View ▸ Macros): record what you do, run, edit, create and delete macros, and give
//! them keyboard shortcuts (⌘⌥⇧1–9). Macros are kept in the document or in "All Documents"
//! (a macros.json in the app's config folder, like Word's Normal template).

use crate::app::WordApp;
use egui::{Color32, ComboBox, Key, RichText, Ui};
use word_engine::macros::{self as m, Macro, COMMANDS};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Store { #[default] Global, Document }

impl Store { pub fn label(&self) -> &'static str { match self { Store::Global => "All Documents (Normal)", Store::Document => "This document" } } }

pub struct RecordDraft { pub name: String, pub description: String, pub store: Store, pub shortcut: Option<u8> }

pub struct Editor { pub name: String, pub description: String, pub store: Store, pub code: String, pub original: Option<(Store, String)>, pub output: Vec<String>, pub error: Option<(String, Option<usize>)>, pub shortcut: Option<u8> }

#[derive(Default)]
pub struct MacrosState {
    pub global: Vec<Macro>,
    pub loaded: bool,
    pub record_dialog: Option<RecordDraft>,
    /// While recording: where the macro goes.
    pub recording: Option<RecordDraft>,
    pub list: bool,
    pub selected: Option<(Store, String)>,
    pub editor: Option<Editor>,
    pub filter: u8,
}

fn store_path() -> std::path::PathBuf { crate::crash_log_path().with_file_name("macros.json") }

fn shortcut_label(k: u8) -> String { if cfg!(target_os = "macos") { format!("⌘⌥⇧{k}") } else { format!("Ctrl+Alt+Shift+{k}") } }

impl WordApp {
    fn ensure_macros(&mut self) {
        let st = &mut self.extras.macros;
        if st.loaded { return; }
        st.loaded = true;
        let Ok(text) = std::fs::read_to_string(store_path()) else { return };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return };
        st.global = v.as_array().into_iter().flatten().map(|x| Macro {
            name: x["name"].as_str().unwrap_or_default().into(), description: x["description"].as_str().unwrap_or_default().into(),
            code: x["code"].as_str().unwrap_or_default().into(), shortcut: x["shortcut"].as_str().map(String::from),
        }).filter(|m| !m.name.is_empty()).collect();
    }

    fn save_global_macros(&mut self) {
        let v: Vec<serde_json::Value> = self.extras.macros.global.iter().map(|m| serde_json::json!({ "name": m.name, "description": m.description, "code": m.code, "shortcut": m.shortcut })).collect();
        let path = store_path();
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default()) { self.dialogs.error = Some(crate::i18n::trf("Couldn't save macros: {}", &[&e])); }
    }

    /// All macros: (where, macro).
    pub fn all_macros(&mut self) -> Vec<(Store, Macro)> {
        self.ensure_macros();
        self.doc.macros.iter().cloned().map(|m| (Store::Document, m)).chain(self.extras.macros.global.iter().cloned().map(|m| (Store::Global, m))).collect()
    }

    fn put_macro(&mut self, store: Store, mac: Macro, replacing: Option<&str>) {
        self.ensure_macros();
        let list = match store { Store::Document => &mut self.doc.macros, Store::Global => &mut self.extras.macros.global };
        if let Some(old) = replacing { list.retain(|m| m.name != old); }
        // A shortcut belongs to one macro.
        if let Some(sc) = &mac.shortcut { for m in self.doc.macros.iter_mut().chain(self.extras.macros.global.iter_mut()) { if m.shortcut.as_ref() == Some(sc) { m.shortcut = None; } } }
        let list = match store { Store::Document => &mut self.doc.macros, Store::Global => &mut self.extras.macros.global };
        match list.iter_mut().find(|m| m.name == mac.name) { Some(m) => *m = mac, None => list.push(mac) }
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        match store { Store::Document => self.doc.dirty = true, Store::Global => self.save_global_macros() }
    }

    fn delete_macro(&mut self, store: Store, name: &str) {
        match store {
            Store::Document => { self.doc.macros.retain(|m| m.name != name); self.doc.dirty = true; }
            Store::Global => { self.extras.macros.global.retain(|m| m.name != name); self.save_global_macros(); }
        }
    }

    /// Run a macro on the document; problems open the editor at the failing line.
    pub fn run_macro(&mut self, store: Store, mac: &Macro) {
        if self.doc.protection.is_some() { self.dialogs.error = Some("Macros can't run while the document is protected.".into()); return; }
        let r = m::run(&mut self.doc, &mac.code);
        self.layout.pages.clear();
        if let Some(e) = r.error.clone() {
            self.extras.macros.editor = Some(Editor { name: mac.name.clone(), description: mac.description.clone(), store, code: mac.code.clone(), original: Some((store, mac.name.clone())), output: r.output, error: Some(e), shortcut: mac.shortcut.as_deref().and_then(parse_shortcut) });
        } else {
            let msg = if r.output.is_empty() { crate::i18n::trf("Ran {}", &[&mac.name]) } else { r.output.join(" · ") };
            self.notify(&self.ctx.clone(), msg);
        }
        self.canvas_focus = true;
    }

    pub fn start_macro_recording(&mut self, d: RecordDraft) {
        self.doc.start_recording();
        self.extras.macros.recording = Some(d);
        self.canvas_focus = true;
    }

    pub fn stop_macro_recording(&mut self) {
        let code = self.doc.stop_recording();
        let Some(d) = self.extras.macros.recording.take() else { return };
        let mac = Macro { name: d.name.clone(), description: d.description.clone(), code: if code.trim().is_empty() { "// Nothing was recorded.".into() } else { code }, shortcut: d.shortcut.map(|k| shortcut_label(k)) };
        self.put_macro(d.store, mac, None);
        self.notify(&self.ctx.clone(), crate::i18n::trf("Recorded macro {}", &[&d.name]));
    }

    /// While recording: caret keys become movement commands (mouse selection isn't recorded,
    /// as in Word).
    pub fn record_keys(&mut self, ui: &Ui) {
        if self.doc.recorder.is_none() || !self.canvas_focus { return; }
        let lines: Vec<String> = ui.input(|i| i.events.iter().filter_map(|e| match e {
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                let word = modifiers.alt || (modifiers.ctrl && !cfg!(target_os = "macos"));
                let doc = modifiers.command && cfg!(target_os = "macos") || modifiers.ctrl && !cfg!(target_os = "macos");
                Some(match (key, modifiers.shift) {
                    (Key::ArrowLeft, false) if doc && cfg!(target_os = "macos") => "line_start();".into(),
                    (Key::ArrowRight, false) if doc && cfg!(target_os = "macos") => "line_end();".into(),
                    (Key::ArrowLeft, false) => if word { "move_word_left(1);".into() } else { "move_left(1);".into() },
                    (Key::ArrowRight, false) => if word { "move_word_right(1);".into() } else { "move_right(1);".into() },
                    (Key::ArrowLeft, true) => "extend_left(1);".into(),
                    (Key::ArrowRight, true) => "extend_right(1);".into(),
                    (Key::ArrowUp, false) if word => "previous_paragraph();".into(),
                    (Key::ArrowDown, false) if word => "next_paragraph();".into(),
                    (Key::Home, false) => if doc { "doc_start();".into() } else { "line_start();".into() },
                    (Key::End, false) => if doc { "doc_end();".into() } else { "line_end();".into() },
                    _ => return None,
                })
            }
            _ => None,
        }).collect());
        for l in lines { self.doc.record_line(&l); }
    }

    /// ⌘⌥⇧1–9 (Ctrl+Alt+Shift elsewhere) run the macro with that shortcut.
    pub fn macro_shortcuts(&mut self, ctx: &egui::Context) {
        let keys = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
        let hit = ctx.input_mut(|i| keys.iter().position(|k| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::ALT | egui::Modifiers::SHIFT, *k)));
        let Some(k) = hit else { return };
        let label = shortcut_label(k as u8 + 1);
        if let Some((store, mac)) = self.all_macros().into_iter().find(|(_, m)| m.shortcut.as_deref() == Some(label.as_str())) { self.run_macro(store, &mac); }
    }
}

fn parse_shortcut(s: &str) -> Option<u8> { s.chars().last().and_then(|c| c.to_digit(10)).map(|d| d as u8) }

fn shortcut_combo(ui: &mut Ui, id: &str, v: &mut Option<u8>) {
    crate::theme::Combo::from_id_salt(id).width(120.0).selected_text(crate::i18n::tr(&(v.map(shortcut_label).unwrap_or_else(|| "(none)".into())))).show_ui(ui, |ui| {
        ui.selectable_value(v, None, "(none)");
        for k in 1..=9u8 { ui.selectable_value(v, Some(k), crate::i18n::tr(&(shortcut_label(k)))); }
    });
}

/// The View ▸ Macros menu.
pub fn menu(app: &mut WordApp, ui: &mut Ui) {
    if ui.button(crate::i18n::tr("View Macros…")).clicked() { app.ensure_macros(); app.extras.macros.list = true; ui.close(); }
    let recording = app.doc.recorder.is_some();
    if recording {
        if ui.button(crate::i18n::tr("■ Stop Recording")).clicked() { app.stop_macro_recording(); ui.close(); }
        let paused = app.doc.recorder.as_ref().is_some_and(|r| r.paused);
        if ui.button(if paused { "Resume Recording" } else { "Pause Recording" }).clicked() { if let Some(r) = &mut app.doc.recorder { r.paused = !r.paused; } ui.close(); }
    } else if ui.button(crate::i18n::tr("● Record Macro…")).clicked() {
        let n = app.all_macros().len() + 1;
        app.extras.macros.record_dialog = Some(RecordDraft { name: format!("Macro{n}"), description: String::new(), store: Store::Global, shortcut: None });
        ui.close();
    }
}

/// The recording indicator in the status bar.
pub fn status(app: &mut WordApp, ui: &mut Ui) {
    if app.doc.recorder.is_none() { return; }
    ui.add_space(10.0);
    let paused = app.doc.recorder.as_ref().is_some_and(|r| r.paused);
    let label = RichText::new(if paused { "⏸ Recording paused" } else { "● Recording macro" }).size(12.0).color(Color32::from_rgb(200, 40, 40));
    if ui.add(egui::Button::new(label).frame(false)).on_hover_text(crate::i18n::tr("Click to stop recording")).clicked() { app.stop_macro_recording(); }
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    use crate::theme::{dialog, footer, primary_enabled, secondary};
    if let Some(mut d) = app.extras.macros.record_dialog.take() {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        dialog(ctx, "Record Macro").open(&mut open).default_width(440.0).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::grid(ui, egui::Grid::new("rec").num_columns(2).spacing(egui::vec2(12.0, 8.0)), |ui| {
                ui.label(crate::i18n::tr("Macro name:")); ui.text_edit_singleline(&mut d.name); ui.end_row();
                ui.label(crate::i18n::tr("Store macro in:"));
                crate::theme::Combo::from_id_salt("rec-store").width(220.0).selected_text(crate::i18n::tr(&d.store.label())).show_ui(ui, |ui| { for s in [Store::Global, Store::Document] { ui.selectable_value(&mut d.store, s, crate::i18n::tr(&(s.label()))); } });
                ui.end_row();
                ui.label(crate::i18n::tr("Keyboard shortcut:")); shortcut_combo(ui, "rec-key", &mut d.shortcut); ui.end_row();
                ui.label(crate::i18n::tr("Description:")); ui.add(egui::TextEdit::multiline(&mut d.description).desired_rows(2)); ui.end_row();
            });
            if !m::valid_name(&d.name) { ui.colored_label(Color32::from_rgb(190, 40, 40), "Use letters, digits and _ (no spaces), starting with a letter."); }
            ui.label(RichText::new(crate::i18n::tr("Typing, formatting, styles and caret keys are recorded; mouse selection isn't.")).small().weak());
            footer(ui, |ui| { if primary_enabled(ui, m::valid_name(&d.name), "Start Recording").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok { app.start_macro_recording(d); } else if open && !cancel { app.extras.macros.record_dialog = Some(d); } else { app.canvas_focus = true; }
    }
    if app.extras.macros.list {
        let (mut open, mut close) = (true, false);
        let all = app.all_macros();
        let mut action: Option<(u8, Store, Macro)> = None;
        dialog(ctx, "Macros").open(&mut open).default_width(560.0).show(ctx, |ui| {
            app.canvas_focus = false;
            let st = &mut app.extras.macros;
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Macros in:"));
                crate::theme::Combo::from_id_salt("mac-in").width(220.0).selected_text(crate::i18n::tr(&(["All active templates and documents", "This document", "All Documents (Normal)"][st.filter as usize]))).show_ui(ui, |ui| {
                    for (k, n) in ["All active templates and documents", "This document", "All Documents (Normal)"].iter().enumerate() { ui.selectable_value(&mut st.filter, k as u8, crate::i18n::tr(&*n)); }
                });
            });
            let shown: Vec<&(Store, Macro)> = all.iter().filter(|(s, _)| match st.filter { 1 => *s == Store::Document, 2 => *s == Store::Global, _ => true }).collect();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_size(egui::vec2(500.0, 220.0));
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    if shown.is_empty() { ui.label(RichText::new(crate::i18n::tr("No macros yet. Record one, or Create a new one.")).weak()); }
                    for (s, mac) in &shown {
                        let sel = st.selected.as_ref().is_some_and(|x| x.0 == *s && x.1 == mac.name);
                        let label = format!("{}{}{}", mac.name, if *s == Store::Document { "  (document)" } else { "" }, mac.shortcut.as_ref().map(|k| format!("   {k}")).unwrap_or_default());
                        let r = ui.selectable_label(sel, crate::i18n::tr(&label));
                        if r.clicked() { st.selected = Some((*s, mac.name.clone())); }
                        if r.double_clicked() { action = Some((0, *s, mac.clone())); }
                    }
                });
            });
            let current = st.selected.as_ref().and_then(|(s, n)| all.iter().find(|(s2, m2)| s2 == s && &m2.name == n)).cloned();
            if let Some((_, mac)) = &current { if !mac.description.is_empty() { ui.label(RichText::new(crate::i18n::tr(&mac.description)).weak()); } }
            crate::theme::hrow(ui, |ui| {
                ui.add_enabled_ui(current.is_some(), |ui| {
                    if let Some((s, mac)) = &current {
                        if ui.button(crate::i18n::tr("Edit")).clicked() { action = Some((1, *s, mac.clone())); }
                        if ui.button(crate::i18n::tr("Delete")).clicked() { action = Some((2, *s, mac.clone())); }
                    } else { let _ = ui.button(crate::i18n::tr("Edit")); let _ = ui.button(crate::i18n::tr("Delete")); }
                });
                if ui.button(crate::i18n::tr("Create")).clicked() { action = Some((3, Store::Global, Macro::default())); }
            });
            footer(ui, |ui| {
                if primary_enabled(ui, current.is_some(), "Run").clicked() { if let Some((s, mac)) = &current { action = Some((0, *s, mac.clone())); } }
                if secondary(ui, "Close").clicked() { close = true; }
            });
        });
        match action {
            Some((0, s, mac)) => { app.extras.macros.list = false; app.run_macro(s, &mac); }
            Some((1, s, mac)) => { app.extras.macros.editor = Some(Editor { name: mac.name.clone(), description: mac.description.clone(), store: s, code: mac.code.clone(), original: Some((s, mac.name.clone())), output: vec![], error: None, shortcut: mac.shortcut.as_deref().and_then(parse_shortcut) }); app.extras.macros.list = false; }
            Some((2, s, mac)) => app.delete_macro(s, &mac.name),
            Some((_, _, _)) => {
                let n = app.all_macros().len() + 1;
                app.extras.macros.editor = Some(Editor { name: format!("Macro{n}"), description: String::new(), store: Store::Global, code: "// Type the macro's commands. Click Help for the list.\nselect_paragraph();\nbold();\n".into(), original: None, output: vec![], error: None, shortcut: None });
                app.extras.macros.list = false;
            }
            None => {}
        }
        if !open || close { app.extras.macros.list = false; app.canvas_focus = true; }
    }
    editor(app, ctx);
}

fn editor(app: &mut WordApp, ctx: &egui::Context) {
    use crate::theme::{dialog, footer, primary_enabled, secondary};
    let Some(mut ed) = app.extras.macros.editor.take() else { return };
    let (mut open, mut save, mut run, mut close) = (true, false, false, false);
    dialog(ctx, "Macro Editor").open(&mut open).default_width(860.0).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hrow(ui, |ui| {
            ui.label(crate::i18n::tr("Name:")); ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(160.0));
            ui.label(crate::i18n::tr("Store in:"));
            crate::theme::Combo::from_id_salt("ed-store").width(190.0).selected_text(crate::i18n::tr(&ed.store.label())).show_ui(ui, |ui| { for s in [Store::Global, Store::Document] { ui.selectable_value(&mut ed.store, s, crate::i18n::tr(&(s.label()))); } });
            ui.label(crate::i18n::tr("Shortcut:")); shortcut_combo(ui, "ed-key", &mut ed.shortcut);
        });
        crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Description:")); ui.add(egui::TextEdit::singleline(&mut ed.description).desired_width(f32::INFINITY)); });
        crate::theme::hcols(ui, |ui| {
            crate::theme::vcol(ui, |ui| {
                ui.set_width(520.0);
                // Line numbers beside the code; the error line is marked.
                let lines = ed.code.lines().count().max(1);
                let err_line = ed.error.as_ref().and_then(|e| e.1);
                egui::ScrollArea::vertical().id_salt("macro-code").max_height((ui.ctx().content_rect().height() - 470.0).clamp(140.0, 420.0)).show(ui, |ui| {
                    crate::theme::hcols(ui, |ui| {
                        let nums: String = (1..=lines).map(|k| if Some(k) == err_line { format!("▶{k}\n") } else { format!("{k}\n") }).collect();
                        ui.add(egui::Label::new(RichText::new(crate::i18n::tr(&nums)).monospace().color(Color32::from_gray(140))).selectable(false));
                        ui.add(egui::TextEdit::multiline(&mut ed.code).code_editor().desired_rows(12).desired_width(470.0).lock_focus(true));
                    });
                });
                if let Some((msg, line)) = &ed.error { ui.colored_label(Color32::from_rgb(190, 40, 40), format!("{}{msg}", line.map(|l| format!("Line {l}: ")).unwrap_or_default())); }
                if !ed.output.is_empty() { ui.label(RichText::new(ed.output.join("\n")).monospace()); }
            });
            ui.add_space(16.0);
            crate::theme::vcol(ui, |ui| {
                crate::theme::section(ui, "Commands");
                egui::ScrollArea::vertical().id_salt("macro-help").max_height((ui.ctx().content_rect().height() - 440.0).clamp(160.0, 440.0)).show(ui, |ui| {
                    for (sig, what) in COMMANDS {
                        if ui.add(egui::Label::new(RichText::new(crate::i18n::tr(&*sig)).monospace().size(11.5)).sense(egui::Sense::click())).on_hover_text(crate::i18n::tr("Click to insert")).clicked() {
                            let call = sig.split(" / ").next().unwrap_or(sig).split(" ->").next().unwrap_or(sig);
                            ed.code.push_str(&format!("\n{};", call.trim()));
                        }
                        ui.label(RichText::new(crate::i18n::tr(&*what)).small().weak());
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new(crate::i18n::tr("Rhai syntax: let x = 1; if x > 0 { … } for i in 0..3 { … } while … { } fn name() { … }")).small().weak());
                });
            });
        });
        footer(ui, |ui| {
            let valid = m::valid_name(&ed.name);
            if primary_enabled(ui, valid, "Save").clicked() { save = true; }
            if secondary(ui, "Run").on_hover_text(crate::i18n::tr("Run on this document")).clicked() { run = true; }
            if secondary(ui, "Check").clicked() { ed.error = m::check(&ed.code).err(); ed.output = if ed.error.is_none() { vec!["No syntax errors.".into()] } else { vec![] }; }
            if secondary(ui, "Close").clicked() { close = true; }
            if !valid { ui.colored_label(Color32::from_rgb(190, 40, 40), "Name: letters, digits, _"); }
        });
    });
    let mac = Macro { name: ed.name.clone(), description: ed.description.clone(), code: ed.code.clone(), shortcut: ed.shortcut.map(shortcut_label) };
    if save {
        // Moving between stores removes it from the old one.
        if let Some((s, old)) = ed.original.clone() { if s != ed.store { app.delete_macro(s, &old); } }
        let replacing = ed.original.as_ref().filter(|o| o.0 == ed.store).map(|o| o.1.clone());
        app.put_macro(ed.store, mac.clone(), replacing.as_deref());
        ed.original = Some((ed.store, ed.name.clone()));
        ed.output = vec![crate::i18n::trf("Saved {}.", &[&ed.name])];
        ed.error = None;
    }
    if run {
        let r = m::run(&mut app.doc, &mac.code);
        app.layout.pages.clear();
        ed.error = r.error;
        ed.output = r.output;
        if ed.error.is_none() && ed.output.is_empty() { ed.output = vec!["Done.".into()]; }
    }
    if open && !close { app.extras.macros.editor = Some(ed); } else { app.canvas_focus = true; }
}
