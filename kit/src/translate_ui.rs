//! The Translate window shared by Qalam and Sard: offline translation with OPUS-MT models
//! (woffice-translate). The source language is detected (or chosen); a pair's model is
//! downloaded once with a progress bar, or installed from a folder, and is then used without
//! a network. The result can be edited, copied, or handed back to the app. Also the
//! Translation Models window (what's installed, its size, delete).

use crate::i18n::{tr, trf};
use egui::{Color32, RichText};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use woffice_translate as wt;
pub use woffice_translate::LANGUAGES;

/// Where models are kept: one folder both apps share.
pub fn models_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("WOFFICE_MODELS") { return PathBuf::from(d); }
    let base = std::env::var_os("APPDATA").map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(if cfg!(target_os = "macos") { "Library/Application Support" } else { ".local/share" })))
        .unwrap_or_else(std::env::temp_dir);
    base.join("Woosool").join("Translation Models")
}

/// What a background job is doing.
#[derive(Clone, Debug, Default)]
enum Work { #[default] Idle, Downloading(String), Translating(usize, usize), Done(Result<String, String>) }

/// What the app should do with the result.
#[derive(Clone, Debug, PartialEq)]
pub enum Action { Insert(String), NewDocument(String), Extra(String), Close }

pub struct Translator {
    pub text: String,
    pub from: String,
    pub to: String,
    pub result: Option<String>,
    pub error: Option<String>,
    work: Arc<Mutex<Work>>,
    /// Labels of the app's buttons: insert (replace the selection) and new document.
    pub insert_label: Option<String>,
    pub new_label: Option<String>,
    /// An extra button the app offers (e.g. Sard's "Translate Slide").
    pub extra_label: Option<String>,
}

impl Translator {
    pub fn new(text: &str, to: &str) -> Translator {
        Translator { text: text.to_string(), from: "auto".into(), to: to.to_string(), result: None, error: None, work: Default::default(), insert_label: Some("Insert".into()), new_label: None, extra_label: None }
    }
    pub fn busy(&self) -> bool { matches!(*self.work.lock().unwrap(), Work::Downloading(_) | Work::Translating(..)) }
    fn source(&self) -> String { if self.from == "auto" { wt::detect(&self.text).to_string() } else { self.from.clone() } }

    fn start_download(&self, ctx: &egui::Context) {
        let (from, to) = (self.source(), self.to.clone());
        let work = self.work.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let models = models_dir();
            let r = (|| -> Result<String, String> {
                for m in wt::missing(&models, &from, &to)? {
                    *work.lock().unwrap() = Work::Downloading(m.to_string());
                    ctx.request_repaint();
                    wt::download(&models, m)?;
                }
                Ok(String::new())
            })();
            *work.lock().unwrap() = match r { Ok(_) => Work::Idle, Err(e) => Work::Done(Err(e)) };
            ctx.request_repaint();
        });
    }

    pub fn start(&self, ctx: &egui::Context) {
        let (from, to, text) = (self.source(), self.to.clone(), self.text.clone());
        let work = self.work.clone();
        let ctx = ctx.clone();
        *work.lock().unwrap() = Work::Translating(0, 1);
        std::thread::spawn(move || {
            let w2 = work.clone();
            let c2 = ctx.clone();
            let r = wt::translate(&models_dir(), &from, &to, &text, &|d, t| { *w2.lock().unwrap() = Work::Translating(d, t); c2.request_repaint(); });
            *work.lock().unwrap() = Work::Done(r);
            ctx.request_repaint();
        });
    }
}

fn lang_name(code: &str) -> String { wt::LANGUAGES.iter().find(|l| l.0 == code).map(|l| tr(l.1)).unwrap_or_else(|| code.to_string()) }
/// The name in reading order, for filling a template (`trf` puts the line in display order).
fn lang_raw(code: &str) -> String { wt::LANGUAGES.iter().find(|l| l.0 == code).map(|l| crate::i18n::raw(l.1)).unwrap_or_else(|| code.to_string()) }

/// A text box that shows right-to-left text correctly: read-only display with an Edit switch.
fn text_box(ui: &mut egui::Ui, text: &mut String, id: &str, rows: usize) {
    let eid = egui::Id::new(("tr-editing", id));
    let editing = ui.data(|d| d.get_temp::<bool>(eid)).unwrap_or(false);
    if crate::widgets::is_rtl_text(text) && !editing {
        egui::Frame::NONE.stroke(egui::Stroke::new(1.0, crate::colors::c_line())).inner_margin(6).corner_radius(4).show(ui, |ui| {
            let w = ui.available_width() - 4.0;
            egui::ScrollArea::vertical().id_salt(id).max_height(rows as f32 * 22.0).show(ui, |ui| { let c = ui.visuals().text_color(); crate::widgets::rtl_text(ui, text, w, egui::FontId::proportional(15.0), c) });
        });
        if ui.small_button(tr("Edit")).on_hover_text(tr("Edit the text (shown in typing order while editing)")).clicked() { ui.data_mut(|d| d.insert_temp(eid, true)); }
    } else {
        egui::ScrollArea::vertical().id_salt(id).max_height(rows as f32 * 24.0).show(ui, |ui| { ui.add(egui::TextEdit::multiline(text).desired_width(f32::INFINITY).desired_rows(rows).hint_text(tr("Text to translate"))); });
        if editing && ui.small_button(tr("Done")).clicked() { ui.data_mut(|d| d.insert_temp(eid, false)); }
    }
}

/// Translate one text in the background (an app translating many pieces, e.g. every slide):
/// returns the translations in order, or the first error.
pub fn translate_all(from: &str, to: &str, texts: Vec<String>) -> Result<Vec<String>, String> {
    let models = models_dir();
    texts.iter().map(|t| if t.trim().is_empty() { Ok(t.clone()) } else { let f = if from == "auto" { wt::detect(t) } else { from }; wt::translate(&models, f, to, t, &|_, _| {}) }).collect()
}

/// The language of a text.
pub fn detect(text: &str) -> &'static str { wt::detect(text) }

/// Are the models for a translation installed?
pub fn ready(from: &str, to: &str) -> bool { wt::missing(&models_dir(), from, to).is_ok_and(|m| m.is_empty()) }

/// The Translate window. Returns what the app should do; `open` turns false when closed.
pub fn window(ctx: &egui::Context, t: &mut Translator, open: &mut bool) -> Option<Action> {
    let mut action = None;
    // A finished job's result.
    let work = t.work.lock().unwrap().clone();
    if let Work::Done(r) = &work {
        match r { Ok(v) if !v.is_empty() => { t.result = Some(v.clone()); t.error = None; } Ok(_) => {} Err(e) => t.error = Some(e.clone()) }
        *t.work.lock().unwrap() = Work::Idle;
    }
    if t.busy() { ctx.request_repaint_after(std::time::Duration::from_millis(150)); }
    crate::theme::dialog(ctx, tr("Translate")).open(open).collapsible(false).resizable(true).default_width(560.0).show(ctx, |ui| {
        let models = models_dir();
        let src = t.source();
        crate::theme::hrow(ui, |ui| {
            ui.label(tr("From"));
            let label = if t.from == "auto" { trf("Detect ({})", &[&lang_raw(&src)]) } else { lang_name(&t.from) };
            crate::theme::Combo::from_id_salt("tr-from").width(150.0).selected_text(label).show_ui(ui, |ui| {
                ui.selectable_value(&mut t.from, "auto".to_string(), tr("Detect language"));
                for (c, n) in wt::LANGUAGES { ui.selectable_value(&mut t.from, c.to_string(), tr(n)); }
            });
            if ui.button("⇄").on_hover_text(tr("Swap languages")).clicked() && t.from != "auto" { std::mem::swap(&mut t.from, &mut t.to); }
            ui.label(tr("To"));
            crate::theme::Combo::from_id_salt("tr-to").width(150.0).selected_text(lang_name(&t.to)).show_ui(ui, |ui| {
                for (c, n) in wt::LANGUAGES { ui.selectable_value(&mut t.to, c.to_string(), tr(n)); }
            });
        });
        ui.add_space(4.0);
        text_box(ui, &mut t.text, "tr-src", 4);
        // The models this needs, and getting them.
        let missing = wt::missing(&models, &src, &t.to);
        match (&missing, &work) {
            (_, Work::Downloading(m)) => {
                let (d, n) = (wt::DONE.load(std::sync::atomic::Ordering::Relaxed), wt::TOTAL.load(std::sync::atomic::Ordering::Relaxed));
                ui.label(trf("Downloading the translation model {}… (once; then it works offline)", &[&m]));
                ui.add(egui::ProgressBar::new(if n > 0 { d as f32 / n as f32 } else { 0.0 }).show_percentage().animate(n == 0).text(format!("{:.0} / {:.0} MB", d as f64 / 1e6, n as f64 / 1e6)));
            }
            (Err(e), _) => { ui.colored_label(Color32::from_rgb(190, 40, 40), tr(e)); }
            (Ok(m), _) if !m.is_empty() => {
                ui.label(RichText::new(trf("{} → {} needs a translation model (about {} MB), downloaded once. After that, translating works without the internet.", &[&lang_raw(&src), &lang_raw(&t.to), &(m.len() * 110)])).color(crate::colors::c_ink2()));
                crate::theme::hrow(ui, |ui| {
                    if crate::theme::primary(ui, tr("Download Model")).clicked() { t.error = None; t.start_download(ui.ctx()); }
                    if ui.button(tr("Install from Folder…")).on_hover_text(tr("For computers without internet: a folder holding config.json, tokenizer.json and the encoder and decoder .onnx files")).clicked() {
                        if let Some(dir) = rfd_folder() { for repo in m { if let Err(e) = wt::install_from(&models, repo, &dir) { t.error = Some(e); break; } } }
                    }
                });
            }
            _ => {}
        }
        if let Work::Translating(d, n) = &work { ui.add(egui::ProgressBar::new(*d as f32 / (*n).max(1) as f32).show_percentage().text(tr("Translating…"))); }
        if let Some(e) = &t.error { ui.colored_label(Color32::from_rgb(190, 40, 40), tr(e)); }
        if let Some(r) = &mut t.result {
            ui.separator();
            ui.label(RichText::new(lang_name(&t.to)).strong());
            text_box(ui, r, "tr-out", 8);
        }
        let ready = missing.as_ref().is_ok_and(|m| m.is_empty());
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, ready && !t.busy() && !t.text.trim().is_empty(), tr("Translate")).clicked() { t.error = None; t.result = None; t.start(ui.ctx()); }
            if let Some(r) = t.result.clone() {
                if let Some(l) = t.insert_label.clone() { if crate::theme::secondary(ui, &tr(&l)).clicked() { action = Some(Action::Insert(r.clone())); } }
                if let Some(l) = t.new_label.clone() { if crate::theme::secondary(ui, &tr(&l)).clicked() { action = Some(Action::NewDocument(r.clone())); } }
                if crate::theme::secondary(ui, &tr("Copy")).clicked() { ui.ctx().copy_text(r); }
            }
            if let Some(l) = t.extra_label.clone() { if ui.add_enabled(ready && !t.busy(), egui::Button::new(tr(&l))).clicked() { action = Some(Action::Extra(t.to.clone())); } }
            if crate::theme::secondary(ui, &tr("Close")).clicked() { action = Some(Action::Close); }
        });
    });
    if action == Some(Action::Close) { *open = false; }
    action
}

fn rfd_folder() -> Option<PathBuf> { FOLDER_PICKER.lock().unwrap().as_ref().and_then(|f| f()) }

/// The app's folder picker (the kit doesn't depend on a file-dialog crate).
pub static FOLDER_PICKER: Mutex<Option<fn() -> Option<PathBuf>>> = Mutex::new(None);

/// The Translation Models window: what's installed and how big; delete to free space.
pub fn models_window(ctx: &egui::Context, open: &mut bool) {
    crate::theme::dialog(ctx, tr("Translation Models")).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        let models = models_dir();
        ui.label(RichText::new(tr("Models are downloaded the first time you translate between two languages, then used offline. Qalam and Sard share them.")).color(crate::colors::c_ink2()));
        ui.label(RichText::new(models.to_string_lossy()).small().weak());
        ui.add_space(6.0);
        let list = wt::list_installed(&models);
        if list.is_empty() { ui.weak(tr("No models installed yet.")); }
        let mut remove = None;
        crate::theme::grid(ui, egui::Grid::new("tr-models").num_columns(3).striped(true).spacing(egui::vec2(16.0, 6.0)), |ui| {
            for (name, size) in &list {
                ui.label(name.trim_start_matches("opus-mt-").replace('-', " → "));
                ui.label(format!("{:.0} MB", *size as f64 / 1e6));
                if ui.small_button(tr("Delete")).clicked() { remove = Some(name.clone()); }
                ui.end_row();
            }
        });
        if let Some(n) = remove { let _ = wt::remove(&models, &n); }
        let total: u64 = list.iter().map(|l| l.1).sum();
        ui.add_space(4.0);
        ui.label(trf("Total: {} MB", &[&format!("{:.0}", total as f64 / 1e6)]));
    });
}
