//! Translate in Sard, offline (shared Translate window): the selected text or shape's text,
//! put back in place; or every text on the slide or in the presentation, translated in place.

use crate::app::SlidesApp;
use crate::i18n::tr;
use qalam_kit::translate_ui::{self as tu, Action, Translator};

pub struct State { pub t: Translator, pub open: bool, pub models: bool }

/// Slides being translated in the background: the shapes, and the result when it comes.
pub struct Job { targets: Vec<(slides_engine::scene::Loc, String)>, rx: std::sync::mpsc::Receiver<Result<Vec<String>, String>>, started: f64 }

/// Open the Translate window for `text` (into the interface language, or English from it).
pub fn open(app: &mut SlidesApp, text: &str) {
    let ui_lang = crate::i18n::lang().code().to_string();
    let to = if tu::detect(text) == ui_lang { if ui_lang == "en" { "ar".to_string() } else { "en".to_string() } } else { ui_lang };
    let mut t = Translator::new(text, &to);
    t.insert_label = if app.edit.is_some() || !app.selection.is_empty() { Some("Replace".into()) } else { None };
    t.extra_label = Some("Translate Slide".into());
    if !text.trim().is_empty() && tu::ready(tu::detect(text), &to) { t.start(&app.ctx); }
    app.translator = Some(State { t, open: true, models: false });
}

/// The window and its results.
pub fn window(app: &mut SlidesApp, ctx: &egui::Context) {
    poll(app);
    let Some(mut st) = app.translator.take() else { return };
    if st.open {
        match tu::window(ctx, &mut st.t, &mut st.open) {
            Some(Action::Insert(r)) => { replace_selection(app, &r); st.open = false; }
            Some(Action::Extra(to)) => { let from = st.t.from.clone(); translate_slides(app, &from, &to, false); st.open = false; }
            _ => {}
        }
    }
    if st.models { tu::models_window(ctx, &mut st.models); }
    if st.open || st.models { app.translator = Some(st); }
}

/// Open just the Translation Models window.
pub fn models(app: &mut SlidesApp) {
    let t = Translator::new("", "en");
    app.translator = Some(State { t, open: false, models: true });
}

/// Put the translation where the text came from: the selected text being edited, else the
/// selected shapes' text.
fn replace_selection(app: &mut SlidesApp, text: &str) {
    if let Some(e) = app.edit.clone() {
        let (a, b) = e.range();
        if a < b {
            app.checkpoint("translate");
            app.pres.delete_text(&e.loc, e.cell, a, b);
            app.pres.insert_text(&e.loc, e.cell, a, text);
            app.edit = None;
            app.changed();
            return;
        }
    }
    let Some(l) = app.edit.as_ref().map(|e| e.loc.clone()).or_else(|| app.selection.first().cloned()) else { return };
    app.checkpoint("translate");
    app.pres.set_shape_text(&l, None, text);
    app.edit = None;
    app.changed();
}

/// Translate every text shape on the slide (or every slide), paragraph by paragraph, keeping
/// the shapes and their formatting.
pub fn translate_slides(app: &mut SlidesApp, from: &str, to: &str, all: bool) {
    let slides = if all { app.slides() } else { app.slide().into_iter().collect() };
    let mut targets = vec![];
    for s in &slides {
        let n = app.pres.slide(s).and_then(|r| r.path("p:cSld/p:spTree")).map(|t| t.elements().count()).unwrap_or(0);
        for i in 0..n {
            let loc = slides_engine::scene::Loc { part: s.clone(), path: vec![i] };
            let text = app.pres.shape_text(&loc, None);
            if !text.trim().is_empty() { targets.push((loc, text)); }
        }
    }
    if targets.is_empty() { app.notify(&tr("There's no text to translate.")); return; }
    // Without the model yet: the Translate window offers to download it.
    let sample = targets.iter().map(|t| t.1.as_str()).collect::<Vec<_>>().join("\n");
    let src = if from == "auto" { tu::detect(&sample).to_string() } else { from.to_string() };
    if !tu::ready(&src, to) {
        open(app, &sample.chars().take(400).collect::<String>());
        if let Some(st) = app.translator.as_mut() { st.t.to = to.to_string(); }
        app.notify(&tr("Download the translation model, then choose Translate Slide again."));
        return;
    }
    let texts: Vec<String> = targets.iter().map(|t| t.1.clone()).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    let (from, to, ctx) = (from.to_string(), to.to_string(), app.ctx.clone());
    std::thread::spawn(move || { let _ = tx.send(tu::translate_all(&from, &to, texts)); ctx.request_repaint(); });
    app.notify(&tr("Translating…"));
    app.translate_job = Some(Job { targets, rx, started: app.ctx.input(|i| i.time) });
}

/// A finished slide translation goes into the slides (each paragraph keeps its formatting).
pub fn poll(app: &mut SlidesApp) {
    let Some(job) = app.translate_job.take() else { return };
    let r = match job.rx.try_recv() {
        Ok(r) => r,
        Err(std::sync::mpsc::TryRecvError::Empty) => { app.ctx.request_repaint_after(std::time::Duration::from_millis(200)); let t = job.started; app.translate_job = Some(job); let _ = t; return; }
        Err(_) => Err("The translation stopped.".into()),
    };
    match r {
        Ok(out) => {
            app.checkpoint("translate");
            for ((loc, before), t) in job.targets.iter().zip(out) {
                // Only if the shape still says what was translated.
                let orig: Vec<String> = app.pres.shape_text(loc, None).split('\n').map(String::from).collect();
                if orig.join("\n") != *before { continue; }
                let mut paras = t.split('\n');
                let mut pos = 0usize;
                for o in &orig {
                    let new = paras.next().unwrap_or("").to_string();
                    app.pres.delete_text(loc, None, pos, pos + o.chars().count());
                    app.pres.insert_text(loc, None, pos, &new);
                    pos += new.chars().count() + 1;
                }
            }
            app.changed();
            app.notify(&tr("Translated"));
        }
        Err(e) => app.error = Some(tr(&e)),
    }
}
