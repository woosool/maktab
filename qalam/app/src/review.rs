//! Review tab: Editor (spelling and grammar) with squiggles and suggestions, Thesaurus,
//! Read Aloud, Check Accessibility, Translate, the Reviewing Pane, Compare, and Restrict
//! Editing with its enforcement.

use crate::app::WordApp;
use crate::insert::Job;
use egui::{Color32, RichText, Ui};
use std::collections::HashSet;
use word_engine::proofing::Issue;
use word_engine::review::{Protection, Restriction};

#[derive(Default)]
pub struct ReviewState {
    /// Spelling and grammar issues, for the document revision they were found in.
    pub issues: Vec<Issue>,
    pub issues_rev: u64,
    pub dirty_since: Option<f64>,
    pub ignored: HashSet<String>,
    pub custom: HashSet<String>,
    pub custom_loaded: bool,
    pub check_as_you_type: bool,
    pub grammar: bool,
    pub editor_open: bool,
    pub current: usize,
    pub thesaurus: Option<Thesaurus>,
    pub speaking: Option<std::process::Child>,
    pub accessibility: bool,
    pub reviewing_pane: bool,
    pub restrict: Option<RestrictDraft>,
    pub unlock: Option<String>,
    pub compare: Option<CompareDraft>,
    pub translate: Option<TranslateState>,
    /// The Translation Models window.
    pub models_open: bool,
    pub show_comment_marks: bool,
    pub settings_init: bool,
}

pub struct Thesaurus { pub query: String, pub job: Option<Job<Vec<(String, String)>>>, pub results: Vec<(String, String)>, pub error: Option<String> }

#[derive(Clone)]
pub struct RestrictDraft { pub kind: Restriction, pub password: String, pub confirm: String }

#[derive(Clone, Default)]
pub struct CompareDraft { pub original: Option<std::path::PathBuf>, pub revised: Option<std::path::PathBuf>, pub use_current: bool }

/// The Translate window (offline models) and whether it translates the whole document.
pub struct TranslateState { pub t: qalam_kit::translate_ui::Translator, pub whole: bool, pub open: bool }

/// Languages offered for translation (offline models).
pub use qalam_kit::translate_ui::LANGUAGES;

fn dictionary_path() -> std::path::PathBuf { crate::crash_log_path().with_file_name("custom.dic") }

impl ReviewState {
    fn ensure_init(&mut self) {
        if !self.settings_init { self.settings_init = true; self.check_as_you_type = true; self.grammar = true; self.show_comment_marks = true; }
        if !self.custom_loaded {
            self.custom_loaded = true;
            if let Ok(t) = std::fs::read_to_string(dictionary_path()) { self.custom = t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(); }
        }
    }
    fn skip_words(&self) -> HashSet<String> { self.ignored.union(&self.custom).cloned().collect() }
}

impl WordApp {
    /// Refresh the proofing issues a moment after typing stops.
    pub fn refresh_proofing(&mut self, now: f64) {
        let r = &mut self.extras.review;
        r.ensure_init();
        if !r.check_as_you_type && !r.editor_open { r.issues.clear(); return; }
        if r.issues_rev == self.doc.revision { return; }
        let since = *r.dirty_since.get_or_insert(now);
        if now - since < 0.6 && !r.issues.is_empty() { return; }
        r.issues = self.doc.proofing_issues(&r.skip_words(), r.grammar);
        r.issues_rev = self.doc.revision;
        r.dirty_since = None;
        if r.current >= r.issues.len() { r.current = 0; }
    }

    /// The issue under a document position (for the right-click menu).
    pub fn issue_at(&self, pos: usize) -> Option<Issue> { self.extras.review.issues.iter().find(|i| pos >= i.start && pos <= i.end).cloned() }

    pub fn ignore_word(&mut self, word: &str) { self.extras.review.ignored.insert(word.to_string()); self.extras.review.issues_rev = u64::MAX; }

    pub fn add_to_dictionary(&mut self, word: &str) {
        let r = &mut self.extras.review;
        r.custom.insert(word.to_string());
        let mut words: Vec<&String> = r.custom.iter().collect();
        words.sort();
        let path = dictionary_path();
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, words.iter().map(|w| format!("{w}\n")).collect::<String>());
        r.issues_rev = u64::MAX;
    }

    pub fn open_editor(&mut self) {
        let r = &mut self.extras.review;
        r.ensure_init();
        r.editor_open = true;
        r.issues_rev = u64::MAX;
        r.dirty_since = Some(-10.0);
        r.current = 0;
    }

    pub fn open_thesaurus(&mut self) {
        let word = { let t = self.doc.selected_text(); if t.trim().is_empty() { let (s, e) = self.doc.word_at(self.doc.range().0); self.doc.text_between(s, e) } else { t } };
        self.extras.review.thesaurus = Some(Thesaurus { query: word.trim().to_string(), job: None, results: vec![], error: None });
        self.lookup_synonyms();
    }

    fn lookup_synonyms(&mut self) {
        let Some(t) = &mut self.extras.review.thesaurus else { return };
        if t.query.trim().is_empty() { return; }
        let q = crate::insert::encode(t.query.trim());
        t.error = None;
        t.job = Some(Job::spawn(&self.ctx, move || {
            let syn = crate::insert::get_json(&format!("https://api.datamuse.com/words?rel_syn={q}&max=40&md=p"))?;
            let near = crate::insert::get_json(&format!("https://api.datamuse.com/words?ml={q}&max=20&md=p"))?;
            let pos = |v: &serde_json::Value| v["tags"].as_array().and_then(|t| t.iter().filter_map(|x| x.as_str()).find(|x| ["n", "v", "adj", "adv"].contains(x))).map(|p| match p { "n" => "noun", "v" => "verb", "adj" => "adjective", _ => "adverb" }).unwrap_or("").to_string();
            let mut out: Vec<(String, String)> = vec![];
            for v in syn.as_array().into_iter().flatten().chain(near.as_array().into_iter().flatten()) {
                if let Some(w) = v["word"].as_str() { if !out.iter().any(|o| o.0 == w) { out.push((w.to_string(), pos(v))); } }
            }
            Ok(out)
        }));
    }

    /// Read Aloud: speak from the caret (or the selection) with the system voice; again stops.
    pub fn toggle_read_aloud(&mut self) {
        if let Some(mut c) = self.extras.review.speaking.take() { let _ = c.kill(); return; }
        let (s, e) = self.doc.range();
        let text = if s < e { self.doc.selected_text() } else { let (i, _) = self.doc.locate(s); self.doc.text_between(self.doc.paragraph_start(i), self.doc.len()) };
        let text: String = text.chars().filter(|c| !c.is_control() || *c == '\n').collect();
        if text.trim().is_empty() { return; }
        let child = if cfg!(target_os = "macos") { std::process::Command::new("say").arg(&text).spawn() }
            else if cfg!(target_os = "windows") {
                let script = format!("Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak([Console]::In.ReadToEnd())");
                std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).stdin(std::process::Stdio::piped()).spawn().map(|mut c| { if let Some(mut i) = c.stdin.take() { use std::io::Write; let _ = i.write_all(text.as_bytes()); } c })
            } else { std::process::Command::new("spd-say").arg("-w").arg(&text).spawn().or_else(|_| std::process::Command::new("espeak").arg(&text).spawn()) };
        match child { Ok(c) => self.extras.review.speaking = Some(c), Err(e) => self.dialogs.error = Some(crate::i18n::trf("Read Aloud needs a system voice: {}", &[&e])) }
    }

    pub fn reading_aloud(&mut self) -> bool {
        if let Some(c) = &mut self.extras.review.speaking { if matches!(c.try_wait(), Ok(Some(_))) { self.extras.review.speaking = None; } }
        self.extras.review.speaking.is_some()
    }

    /// Translate the selection (or the whole document) offline, opening the Translate window;
    /// it starts right away when the models are installed.
    pub fn start_translate(&mut self, to: &str, whole: bool) {
        let text = if whole { self.doc.text() } else { self.doc.selected_text() };
        let mut t = qalam_kit::translate_ui::Translator::new(&text, to);
        if whole { t.insert_label = None; t.new_label = Some("Open as New Document".into()); }
        if !text.trim().is_empty() && qalam_kit::translate_ui::ready(&qalam_woffice_detect(&text), to) { t.start(&self.ctx); }
        self.extras.review.translate = Some(TranslateState { t, whole, open: true });
    }

    /// Restrict Editing: undo edits a protected document doesn't allow.
    pub fn enforce_protection(&mut self, before_rev: u64, before_comments: usize, before_texts: &[String]) {
        let Some(p) = self.doc.protection.clone() else { return };
        match p.kind {
            Restriction::TrackedChanges => if !self.doc.track_changes { self.doc.set_track_changes(true); },
            Restriction::ReadOnly | Restriction::Comments | Restriction::Forms => {
                if self.doc.revision == before_rev { return; }
                let comment_change = self.doc.comments.len() != before_comments || self.doc.comments.iter().map(|c| c.text.clone()).collect::<Vec<_>>() != before_texts;
                let allowed = p.kind == Restriction::Comments && comment_change;
                if !allowed && self.doc.can_undo() {
                    self.doc.undo();
                    self.notify(&self.ctx.clone(), "This document is protected. Review ▸ Restrict Editing to stop protection.");
                }
            }
        }
    }
}

fn wavy(painter: &egui::Painter, x0: f32, x1: f32, y: f32, color: Color32) {
    let mut pts = vec![];
    let mut x = x0;
    let mut up = true;
    while x <= x1 { pts.push(egui::pos2(x, y + if up { 0.0 } else { 2.0 })); x += 2.0; up = !up; }
    if pts.len() > 1 { painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0, color))); }
}

/// Squiggles under spelling (red) and grammar (blue) issues on one page.
pub fn paint_squiggles(r: &ReviewState, layout: &word_engine::Layout, painter: &egui::Painter, page: usize, zoom: f32, to_screen: &dyn Fn(usize, f32, f32) -> egui::Pos2) {
    if !r.check_as_you_type && !r.editor_open { return; }
    for i in &r.issues {
        let color = if i.kind.is_grammar() { Color32::from_rgb(40, 90, 220) } else { Color32::from_rgb(220, 40, 40) };
        for (pg, x, y, w, h) in layout.selection_rects(i.start, i.end) {
            if pg != page { continue; }
            let a = to_screen(pg, x, y + h);
            wavy(painter, a.x, a.x + w * zoom, a.y - 2.0, color);
        }
    }
}

/// Suggestions for the issue under the right-click, at the top of the context menu.
pub fn context_items(app: &mut WordApp, ui: &mut Ui) -> bool {
    let Some(issue) = app.issue_at(app.doc.range().0) else { return false };
    let mut sugg = issue.suggestions.clone();
    if sugg.is_empty() && !issue.kind.is_grammar() { sugg = word_engine::proofing::suggest(&issue.word, 6); }
    ui.label(RichText::new(crate::i18n::tr(&issue.kind.label())).small().weak());
    if sugg.is_empty() { ui.label(RichText::new(crate::i18n::tr("(no suggestions)")).weak()); }
    for s in sugg.iter().take(6) { if ui.button(RichText::new(s).strong()).clicked() { app.doc.fix_issue(&issue, s); ui.close(); } }
    if ui.button(crate::i18n::tr("Ignore All")).clicked() { app.ignore_word(&issue.word); ui.close(); }
    if !issue.kind.is_grammar() && ui.button(crate::i18n::tr("Add to Dictionary")).clicked() { app.add_to_dictionary(&issue.word); ui.close(); }
    ui.separator();
    true
}

pub fn panes(app: &mut WordApp, ui: &mut Ui) {
    let now = ui.input(|i| i.time);
    app.refresh_proofing(now);
    if app.extras.review.dirty_since.is_some() { ui.ctx().request_repaint_after(std::time::Duration::from_millis(700)); }
    if app.extras.review.editor_open { editor_pane(app, ui); }
    if app.extras.review.thesaurus.is_some() { thesaurus_pane(app, ui); }
    if app.extras.review.accessibility { accessibility_pane(app, ui); }
    if app.extras.review.reviewing_pane { reviewing_pane(app, ui); }
    if app.extras.review.restrict.is_some() || app.doc.protection.is_some() && app.extras.review.unlock.is_some() { restrict_pane(app, ui); }
    if app.reading_aloud() { ui.ctx().request_repaint_after(std::time::Duration::from_millis(400)); }
}

fn pane_header(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    crate::theme::hrow(ui, |ui| {
        ui.label(RichText::new(crate::i18n::tr(title)).size(16.0).family(egui::FontFamily::Name("ui-bold".into())));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if crate::ribbon::close_button(ui, "Close").clicked() { close = true; } });
    });
    ui.separator();
    close
}

fn editor_pane(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::right("editor-pane").resizable(true).default_size(300.0).show(ui, |ui| {
        if pane_header(ui, "Editor") { app.extras.review.editor_open = false; return; }
        let issues = app.extras.review.issues.clone();
        let (sp, gr) = (issues.iter().filter(|i| !i.kind.is_grammar()).count(), issues.iter().filter(|i| i.kind.is_grammar()).count());
        crate::theme::hrow(ui, |ui| {
            ui.label(RichText::new(format!("{sp}")).size(22.0).color(Color32::from_rgb(200, 40, 40)));
            ui.label(crate::i18n::tr("Spelling"));
            ui.add_space(16.0);
            ui.label(RichText::new(format!("{gr}")).size(22.0).color(Color32::from_rgb(40, 90, 220)));
            ui.label(crate::i18n::tr("Grammar"));
        });
        crate::theme::check(ui, &mut app.extras.review.check_as_you_type, crate::i18n::tr("Check spelling as you type"));
        if crate::theme::check(ui, &mut app.extras.review.grammar, crate::i18n::tr("Mark grammar errors")).changed() { app.extras.review.issues_rev = u64::MAX; }
        ui.separator();
        if issues.is_empty() { ui.label(RichText::new(crate::i18n::tr("No issues found. ✓")).size(14.0)); return; }
        let k = app.extras.review.current.min(issues.len() - 1);
        let issue = issues[k].clone();
        // Show the issue in its sentence and select it in the document.
        if app.doc.range() != (issue.start, issue.end) && ui.input(|i| !i.pointer.any_down()) && app.extras.review.issues_rev == app.doc.revision {
            app.doc.select(issue.start, issue.end);
            app.scroll_to_caret = true;
        }
        ui.label(RichText::new(crate::i18n::trf("{} ({} of {})", &[&crate::i18n::tr(issue.kind.label()), &(k + 1), &issues.len()])).strong());
        ui.label(RichText::new(crate::i18n::tr(&issue.word)).size(16.0).color(if issue.kind.is_grammar() { Color32::from_rgb(40, 90, 220) } else { Color32::from_rgb(200, 40, 40) }));
        let sugg = if issue.suggestions.is_empty() && !issue.kind.is_grammar() { word_engine::proofing::suggest(&issue.word, 8) } else { issue.suggestions.clone() };
        ui.add_space(4.0);
        if sugg.is_empty() { ui.label(RichText::new(crate::i18n::tr("No suggestions")).weak()); }
        let mut fixed = false;
        for s in &sugg { if ui.add(egui::Button::new(RichText::new(s).size(14.0)).min_size(egui::vec2(ui.available_width(), 28.0))).clicked() { app.doc.fix_issue(&issue, s); fixed = true; } }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Ignore Once")).clicked() { app.extras.review.current = k + 1; }
            if ui.button(crate::i18n::tr("Ignore All")).clicked() { app.ignore_word(&issue.word); }
            if !issue.kind.is_grammar() && ui.button(crate::i18n::tr("Add to Dictionary")).clicked() { app.add_to_dictionary(&issue.word); }
        });
        crate::theme::hrow(ui, |ui| {
            if ui.add_enabled(k > 0, egui::Button::new(crate::i18n::tr("◀ Previous"))).clicked() { app.extras.review.current = k - 1; }
            if ui.add_enabled(k + 1 < issues.len(), egui::Button::new(crate::i18n::tr("Next ▶"))).clicked() { app.extras.review.current = k + 1; }
        });
        if fixed { app.extras.review.dirty_since = Some(-10.0); }
    });
}

fn thesaurus_pane(app: &mut WordApp, ui: &mut Ui) {
    if let Some(t) = &mut app.extras.review.thesaurus {
        if let Some(r) = t.job.as_ref().and_then(|j| j.take()) { t.job = None; match r { Ok(v) => t.results = v, Err(e) => t.error = Some(e) } }
    }
    let mut search = false;
    let mut insert = None;
    let mut close = false;
    egui::Panel::right("thesaurus").resizable(true).default_size(280.0).show(ui, |ui| {
        close = pane_header(ui, "Thesaurus");
        let Some(t) = &mut app.extras.review.thesaurus else { return };
        crate::theme::hrow(ui, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut t.query).desired_width(170.0));
            if r.has_focus() { app.canvas_focus = false; }
            if ui.button(crate::i18n::tr("Search")).clicked() || r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { search = true; }
        });
        if t.job.is_some() { ui.spinner(); }
        if let Some(e) = &t.error { ui.colored_label(Color32::from_rgb(190, 40, 40), crate::i18n::tr(e)); }
        if t.results.is_empty() && t.job.is_none() && t.error.is_none() { ui.label(RichText::new(crate::i18n::tr("No synonyms found.")).weak()); }
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut last = String::new();
            for (w, pos) in &t.results {
                if *pos != last && !pos.is_empty() { ui.label(RichText::new(crate::i18n::tr(&pos)).strong()); last = pos.clone(); }
                crate::theme::hrow(ui, |ui| {
                    if ui.link(w).on_hover_text(crate::i18n::tr("Look up this word")).clicked() { t.query = w.clone(); search = true; }
                    if ui.small_button(crate::i18n::tr("Insert")).clicked() { insert = Some(w.clone()); }
                });
            }
        });
    });
    if close { app.extras.review.thesaurus = None; return; }
    if search { app.lookup_synonyms(); }
    if let Some(w) = insert {
        let (s, e) = app.doc.range();
        if s == e { let (a, b) = app.doc.word_at(s); app.doc.select(a, b); }
        app.doc.replace(&w);
        app.canvas_focus = true;
    }
}

fn accessibility_pane(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::right("accessibility").resizable(true).default_size(300.0).show(ui, |ui| {
        if pane_header(ui, "Accessibility") { app.extras.review.accessibility = false; return; }
        let issues = app.doc.accessibility_issues();
        if issues.is_empty() { ui.label(crate::i18n::tr("No accessibility issues found. ✓")); return; }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for sev in ["Error", "Warning", "Tip"] {
                let list: Vec<_> = issues.iter().filter(|i| i.severity == sev).collect();
                if list.is_empty() { continue; }
                ui.label(RichText::new(format!("{} ({})", crate::i18n::tr(&format!("{sev}s")), list.len())).strong());
                for i in list {
                    let r = ui.add(egui::Button::new(RichText::new(crate::i18n::tr(&i.title))).frame(false));
                    ui.label(RichText::new(crate::i18n::trf(&i.detail, &i.args.iter().map(|a| a as &dyn std::fmt::Display).collect::<Vec<_>>())).small().weak());
                    if r.clicked() { if let Some(p) = i.pos { app.doc.select(p, p); app.scroll_to_caret = true; } }
                    ui.add_space(4.0);
                }
            }
        });
    });
}

fn reviewing_pane(app: &mut WordApp, ui: &mut Ui) {
    (if crate::i18n::rtl() { egui::Panel::right("reviewing-pane") } else { egui::Panel::left("reviewing-pane") }).resizable(true).default_size(280.0).show(ui, |ui| {
        if pane_header(ui, "Revisions") { app.extras.review.reviewing_pane = false; return; }
        let revs = app.doc.revisions();
        ui.label(crate::i18n::trf("{} revisions, {} comments", &[&revs.len(), &app.doc.comments.len()]));
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (start, end, kind, author) in revs {
                let text = app.doc.text_between(start, end);
                let kind = if kind == word_engine::model::RevKind::Ins { "Inserted" } else { "Deleted" };
                ui.label(RichText::new(format!("{author} {kind}")).strong());
                if ui.add(egui::Button::new(RichText::new(text.chars().take(120).collect::<String>())).frame(false)).clicked() { app.doc.select(start, end); app.scroll_to_caret = true; }
                ui.add_space(4.0);
            }
            for c in app.doc.comments.clone() {
                ui.label(RichText::new(crate::i18n::trf("{} Comment", &[&c.author])).strong());
                ui.label(crate::i18n::tr(&c.text));
                ui.add_space(4.0);
            }
        });
    });
}

fn restrict_pane(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::right("restrict").resizable(true).default_size(300.0).show(ui, |ui| {
        if pane_header(ui, "Restrict Editing") { app.extras.review.restrict = None; app.extras.review.unlock = None; return; }
        if let Some(p) = app.doc.protection.clone() {
            ui.label(RichText::new(crate::i18n::tr("Protection is on")).strong());
            ui.label(match p.kind { Restriction::ReadOnly => "This document is read-only.", Restriction::Comments => "You may only add comments.", Restriction::TrackedChanges => "All changes are tracked.", Restriction::Forms => "You may only fill in forms." });
            ui.add_space(6.0);
            if p.hash.is_some() {
                let pw = app.extras.review.unlock.get_or_insert_with(String::new);
                ui.label(crate::i18n::tr("Password:"));
                let r = ui.add(egui::TextEdit::singleline(pw).password(true));
                if r.has_focus() { app.canvas_focus = false; }
            }
            if crate::theme::primary(ui, "Stop Protection").clicked() {
                let pw = app.extras.review.unlock.clone().unwrap_or_default();
                if p.unlocks(&pw) { app.doc.protection = None; app.doc.dirty = true; app.extras.review.unlock = None; app.notify(&ui.ctx().clone(), "Protection stopped."); }
                else { app.dialogs.error = Some("The password is incorrect.".into()); }
            }
            return;
        }
        let Some(d) = &mut app.extras.review.restrict else { return };
        crate::theme::section(ui, "Editing restrictions");
        ui.label(crate::i18n::tr("Allow only this type of editing in the document:"));
        crate::theme::Combo::from_id_salt("restrict-kind").width(220.0).selected_text(crate::i18n::tr(&(Restriction::ALL.iter().find(|r| r.0 == d.kind).map(|r| r.1).unwrap_or("")))).show_ui(ui, |ui| { for (k, n) in Restriction::ALL { ui.selectable_value(&mut d.kind, k, crate::i18n::tr(&n)); } });
        crate::theme::section(ui, "Start enforcement");
        ui.label(crate::i18n::tr("Password (optional):"));
        let a = ui.add(egui::TextEdit::singleline(&mut d.password).password(true));
        ui.label(crate::i18n::tr("Reenter password:"));
        let b = ui.add(egui::TextEdit::singleline(&mut d.confirm).password(true));
        if a.has_focus() || b.has_focus() { app.canvas_focus = false; }
        ui.add_space(6.0);
        if crate::theme::primary(ui, "Yes, Start Enforcing Protection").clicked() {
            if d.password != d.confirm { app.dialogs.error = Some("The passwords don't match.".into()); }
            else {
                let p = Protection::new(d.kind, &d.password);
                if d.kind == Restriction::TrackedChanges { app.doc.set_track_changes(true); }
                app.doc.protection = Some(p);
                app.doc.dirty = true;
                app.extras.review.restrict = None;
                app.extras.review.unlock = Some(String::new());
            }
        }
    });
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    if let Some(mut d) = app.extras.review.compare.take() {
        let mut open = true;
        let (mut go, mut cancel) = (false, false);
        crate::theme::dialog(ctx, "Compare Documents").open(&mut open).default_width(460.0).show(ctx, |ui| {
            app.canvas_focus = false;
            let pick = |p: &mut Option<std::path::PathBuf>| if let Some(f) = rfd::FileDialog::new().add_filter("Documents", &["docx", "md", "txt"]).pick_file() { *p = Some(f); };
            crate::theme::section(ui, "Original document");
            crate::theme::check(ui, &mut d.use_current, crate::i18n::tr("This document"));
            ui.add_enabled_ui(!d.use_current, |ui| crate::theme::hrow(ui, |ui| { if ui.button(crate::i18n::tr("Browse…")).clicked() { pick(&mut d.original); } ui.label(d.original.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "No file selected".into())); }));
            crate::theme::section(ui, "Revised document");
            crate::theme::hrow(ui, |ui| { if ui.button(crate::i18n::tr("Browse…")).clicked() { pick(&mut d.revised); } ui.label(d.revised.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "No file selected".into())); });
            ui.label(RichText::new(crate::i18n::tr("Differences open as tracked changes in place of this document.")).small().weak());
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary_enabled(ui, d.revised.is_some() && (d.use_current || d.original.is_some()), "Compare").clicked() { go = true; }
                if crate::theme::secondary(ui, "Cancel").clicked() { cancel = true; }
            });
        });
        if go {
            let load = |p: &std::path::Path| -> Result<word_engine::Document, String> {
                let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
                match p.extension().and_then(|e| e.to_str()).unwrap_or("") { "md" => Ok(word_engine::import::from_markdown(&String::from_utf8_lossy(&bytes))), "txt" => Ok(word_engine::import::from_text(&String::from_utf8_lossy(&bytes))), _ => word_engine::docx::read(&bytes) }
            };
            let result = (|| -> Result<word_engine::Document, String> {
                let revised = load(d.revised.as_ref().unwrap())?;
                let author = app.author.clone();
                if d.use_current { app.doc.compare(&revised, &author, &crate::app::now_iso()) } else { load(d.original.as_ref().unwrap())?.compare(&revised, &author, &crate::app::now_iso()) }
            })();
            match result { Ok(doc) => { app.set_document(doc, None); app.doc.dirty = true; app.extras.review.reviewing_pane = true; } Err(e) => app.dialogs.error = Some(crate::i18n::trf("Couldn't compare: {}", &[&e])) }
        } else if open && !cancel { app.extras.review.compare = Some(d); } else { app.canvas_focus = true; }
    }
    if let Some(mut st) = app.extras.review.translate.take() {
        app.canvas_focus = false;
        match qalam_kit::translate_ui::window(ctx, &mut st.t, &mut st.open) {
            Some(qalam_kit::translate_ui::Action::Insert(r)) => { app.doc.replace(&r); st.open = false; }
            Some(qalam_kit::translate_ui::Action::NewDocument(r)) => { app.set_document(word_engine::import::from_text(&r), None); app.doc.dirty = true; st.open = false; }
            _ => {}
        }
        if st.open { app.extras.review.translate = Some(st); } else { app.canvas_focus = true; }
    }
    if app.extras.review.models_open {
        let mut open = true;
        qalam_kit::translate_ui::models_window(ctx, &mut open);
        app.extras.review.models_open = open;
    }
}

/// The language of a text (for picking models before translating).
fn qalam_woffice_detect(text: &str) -> String { qalam_kit::translate_ui::detect(text).to_string() }
