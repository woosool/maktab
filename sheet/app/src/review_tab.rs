//! The Review tab laid out like Excel's (spelling.js, comments.js, mention-*.js, protect.js,
//! accessibility-pane.js, language-pane.js): Proofing (Spelling F7 with Qalam's dictionary,
//! Thesaurus, Workbook Statistics), Accessibility (the checker pane), Insights (Smart Lookup),
//! Language (Translate, the kit's offline translator), Comments (threaded, with replies,
//! @mentions and resolve, and the Comments pane), Notes (new, edit, browse, show all, convert to
//! comments) and Protect (Protect Sheet with a password and allowed actions, Protect Workbook
//! structure, Allow Edit Ranges); plus what those need elsewhere: the edit and action checks a
//! protected sheet enforces, AutoCorrect and AutoComplete when an entry is made, comment
//! indicators and the cell menu's comment items.

use crate::app::{Dialog, SheetApp};
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use sheet_engine::addr::{Pos, Range};
use sheet_engine::review::{self as rv, Action, SheetProtection};
use sheet_engine::Value;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

pub const COMMENT_COLOR: Color32 = Color32::from_rgb(0x8E, 0x4C, 0xC4);

// ---------------------------------------------------------------------------------------------
// State

#[derive(Default)]
pub struct ReviewState {
    pub spelling: Option<Spelling>,
    pub thesaurus: Option<Thesaurus>,
    pub stats: bool,
    pub access: bool,
    pub translator: Option<qalam_kit::translate_ui::Translator>,
    pub comments_pane: bool,
    pub comment: Option<CommentDraft>,
    pub replies: std::collections::HashMap<(usize, Pos), String>,
    pub protect: Option<ProtectDraft>,
    pub unprotect: Option<(bool, String, Option<String>)>,
    pub protect_book: Option<(String, String, Option<String>)>,
    pub edit_ranges: Option<RangesDraft>,
    pub range_password: Option<(String, String, Pos)>,
    pub unlocked: Vec<String>,
    pub show_notes: bool,
    /// The entry text when Backspace or Delete turned AutoComplete off for it.
    pub ac_reject: Option<String>,
    pub ignored: HashSet<String>,
    pub view: crate::view_tab::ViewState,
    pub settings: Option<Settings>,
}

/// What the user set that outlasts the session (review_view.json in the config folder).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Status bar: Average, Count, Numerical Count, Minimum, Maximum, Sum, View Shortcuts, Zoom Slider, Zoom.
    pub status: [bool; 9],
    pub qat: Vec<String>,
    pub autocorrect: Vec<(String, String)>,
    pub autocorrect_on: bool,
    pub autocomplete: bool,
    pub dictionary: Vec<String>,
}

pub const STATUS_ITEMS: [&str; 9] = ["Average", "Count", "Numerical Count", "Minimum", "Maximum", "Sum", "View Shortcuts", "Zoom Slider", "Zoom"];

pub fn default_autocorrect() -> Vec<(String, String)> {
    [("(c)", "©"), ("(r)", "®"), ("(tm)", "™"), ("...", "…"), ("-->", "→"), ("<--", "←"), ("==>", "⇒"), ("<==", "⇐"), ("<=>", "⇔"), (":)", "☺"),
     ("teh", "the"), ("adn", "and"), ("recieve", "receive"), ("seperate", "separate"), ("occured", "occurred"), ("untill", "until"), ("wich", "which"), ("thier", "their"), ("definately", "definitely"), ("accomodate", "accommodate"), ("i", "I")]
        .iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

impl Default for Settings {
    fn default() -> Self { Settings { status: [true, true, false, false, false, true, true, true, true], qat: vec!["save".into(), "undo".into(), "redo".into()], autocorrect: default_autocorrect(), autocorrect_on: true, autocomplete: true, dictionary: vec![] } }
}

fn settings_file() -> std::path::PathBuf { crate::config_dir().join("review_view.json") }

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(t) = std::fs::read_to_string(settings_file()) else { return s };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else { return s };
        if let Some(a) = v["status"].as_array() { for (i, b) in a.iter().enumerate().take(9) { s.status[i] = b.as_bool().unwrap_or(s.status[i]); } }
        if let Some(a) = v["qat"].as_array() { s.qat = a.iter().filter_map(|x| x.as_str().map(String::from)).collect(); }
        if let Some(a) = v["autocorrect"].as_array() { s.autocorrect = a.iter().filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_str()?.to_string()))).collect(); }
        if let Some(a) = v["dictionary"].as_array() { s.dictionary = a.iter().filter_map(|x| x.as_str().map(String::from)).collect(); }
        s.autocorrect_on = v["autocorrect_on"].as_bool().unwrap_or(true);
        s.autocomplete = v["autocomplete"].as_bool().unwrap_or(true);
        s
    }
    pub fn save(&self) {
        let v = serde_json::json!({ "status": self.status.to_vec(), "qat": self.qat, "autocorrect": self.autocorrect.iter().map(|(a, b)| vec![a.clone(), b.clone()]).collect::<Vec<_>>(), "autocorrect_on": self.autocorrect_on, "autocomplete": self.autocomplete, "dictionary": self.dictionary });
        let _ = std::fs::create_dir_all(crate::config_dir());
        let _ = std::fs::write(settings_file(), v.to_string());
    }
}

/// The settings, loaded on first use.
pub fn settings(app: &mut SheetApp) -> &mut Settings { app.rv.settings.get_or_insert_with(Settings::load) }

pub struct Spelling { pub cells: Vec<Pos>, pub at: usize, pub offset: usize, pub issue: Option<(Pos, usize, String)>, pub suggestions: Vec<String>, pub change_to: String, pub picked: Option<usize>, pub done: bool, pub change_all: Vec<(String, String)> }
pub struct Thesaurus { pub query: String, pub results: Vec<(String, String)>, pub job: Option<Arc<Mutex<Option<Result<Vec<(String, String)>, String>>>>>, pub error: Option<String> }
pub struct CommentDraft { pub sheet: usize, pub pos: Pos, pub text: String, pub focus: bool }
pub struct ProtectDraft { pub password: String, pub confirm: String, pub allow: [bool; 15], pub error: Option<String> }
pub struct RangesDraft { pub list: Vec<(String, String, String)>, pub error: Option<String> }

// ---------------------------------------------------------------------------------------------
// The tab

pub fn tab(app: &mut SheetApp, ui: &mut Ui) {
    let s = app.sheet();
    let p = app.sel.active;
    group(ui, "Proofing", |ui| {
        if large(ui, "spelling", "Spelling", app.rv.spelling.is_some(), true, "Check the spelling of text on the sheet (F7)").clicked() { start_spelling(app); }
        rows(ui, |ui| {
            if small(ui, "thesaurus", Some("Thesaurus"), app.rv.thesaurus.is_some(), true, "Suggest other words with a similar meaning").clicked() { open_thesaurus(app); }
            if small(ui, "wordCount", Some("Workbook Statistics"), app.rv.stats, true, "Counts of cells, formulas, tables and more").clicked() { app.rv.stats = !app.rv.stats; }
        });
    });
    group(ui, "Accessibility", |ui| {
        if large(ui, "accessibility", "Check Accessibility", app.rv.access, true, "Find content that's hard to read for people with disabilities").clicked() { app.rv.access = !app.rv.access; }
    });
    group(ui, "Insights", |ui| {
        if large(ui, "smartLookup", "Smart Lookup", false, true, "Search the web for the selected text").clicked() { smart_lookup(app); }
    });
    group(ui, "Language", |ui| {
        if large(ui, "translate", "Translate", app.rv.translator.is_some(), true, "Translate the selected cells into another language").clicked() { open_translate(app); }
    });
    let has_thread = app.book.sheets[s].review.threads.contains_key(&p);
    group(ui, "Comments", |ui| {
        if large(ui, "newComment", if has_thread { "Reply" } else { "New Comment" }, false, true, "Start a conversation about this cell").clicked() { new_comment(app); }
        if large(ui, "deleteComment", "Delete", false, has_thread, "Delete the comment thread").clicked() { app.checkpoint("Delete Comment"); app.book.sheets[s].review.threads.remove(&p); app.dirty = true; }
        rows(ui, |ui| {
            let any = app.book.sheets.iter().any(|x| !x.review.threads.is_empty());
            if small(ui, "previous", Some("Previous Comment"), false, any, "Go to the previous comment").clicked() { browse_comments(app, false); }
            if small(ui, "next", Some("Next Comment"), false, any, "Go to the next comment").clicked() { browse_comments(app, true); }
        });
        if large(ui, "showComments", "Show Comments", app.rv.comments_pane, true, "Show the Comments pane").clicked() { app.rv.comments_pane = !app.rv.comments_pane; }
    });
    let has_note = app.book.sheets[s].notes.contains_key(&p);
    group(ui, "Notes", |ui| {
        let r = large_menu(ui, "note", "Notes", "Notes are plain annotations without replies");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "newComment", if has_note { "Edit Note" } else { "New Note" }) { let t = app.book.sheets[s].notes.get(&p).cloned().unwrap_or_default(); app.dialog = Some(Dialog::Note { pos: p, text: t }); ui.close(); }
            if menu_item(ui, "previous", "Previous Note") { browse_notes(app, false); ui.close(); }
            if menu_item(ui, "next", "Next Note") { browse_notes(app, true); ui.close(); }
            if menu_item(ui, "showNotes", if app.rv.show_notes { "Hide All Notes" } else { "Show All Notes" }) { app.rv.show_notes = !app.rv.show_notes; ui.close(); }
            if menu_item(ui, "comment", "Convert to Comments") { convert_notes(app); ui.close(); }
            if has_note && menu_item(ui, "deleteComment", "Delete Note") { app.checkpoint("Delete Note"); app.book.sheets[s].notes.remove(&p); app.dirty = true; ui.close(); }
        });
    });
    group(ui, "Protect", |ui| {
        let prot = app.book.sheets[s].protected;
        if large(ui, "protectSheet", if prot { "Unprotect Sheet" } else { "Protect Sheet" }, prot, true, "Stop changes to locked cells, with the actions people may still take").clicked() {
            if prot { if app.book.sheets[s].review.protection.password.is_some() { app.rv.unprotect = Some((false, String::new(), None)); } else { rv::unprotect_sheet(&mut app.book.sheets[s], ""); app.dirty = true; } }
            else { app.rv.protect = Some(ProtectDraft { password: String::new(), confirm: String::new(), allow: SheetProtection::default().allow, error: None }); }
        }
        let wprot = app.book.review.structure;
        if large(ui, "protectWorkbook", if wprot { "Unprotect Workbook" } else { "Protect Workbook" }, wprot, true, "Stop sheets being added, deleted, moved, renamed or hidden").clicked() {
            if wprot { if app.book.review.structure_password.is_some() { app.rv.unprotect = Some((true, String::new(), None)); } else { rv::unprotect_workbook(&mut app.book, ""); app.dirty = true; } }
            else { app.rv.protect_book = Some((String::new(), String::new(), None)); }
        }
        if large(ui, "lockTracking", "Allow Edit Ranges", false, !prot, "Ranges people can edit when the sheet is protected").clicked() {
            let list = app.book.sheets[s].review.edit_ranges.iter().map(|r| (r.name.clone(), r.ranges.iter().map(|x| x.a1()).collect::<Vec<_>>().join(" "), String::new())).collect();
            app.rv.edit_ranges = Some(RangesDraft { list, error: None });
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Proofing

/// Text constants in reading order from the active cell, wrapping round.
fn text_cells(app: &SheetApp) -> Vec<Pos> {
    let s = &app.book.sheets[app.sheet()];
    let mut cells: Vec<Pos> = s.cells.iter().filter(|(_, c)| matches!(&c.input, sheet_engine::Input::Value(Value::Str(_)))).map(|(p, _)| *p).collect();
    let r = app.sel.range();
    if !r.is_cell() { cells.retain(|p| r.contains(*p)); }
    let a = app.sel.active;
    let k = cells.iter().position(|p| *p >= a).unwrap_or(0);
    cells.rotate_left(k);
    cells
}

pub fn start_spelling(app: &mut SheetApp) {
    app.commit(None);
    let cells = text_cells(app);
    let mut sp = Spelling { cells, at: 0, offset: 0, issue: None, suggestions: vec![], change_to: String::new(), picked: None, done: false, change_all: vec![] };
    next_issue(app, &mut sp);
    app.rv.spelling = Some(sp);
}

fn known(app: &mut SheetApp, w: &str) -> bool {
    app.rv.ignored.contains(&w.to_lowercase()) || settings(app).dictionary.iter().any(|d| d.eq_ignore_ascii_case(w)) || word_engine::proofing::correct(w)
}

fn next_issue(app: &mut SheetApp, sp: &mut Spelling) {
    let si = app.sheet();
    while sp.at < sp.cells.len() {
        let p = sp.cells[sp.at];
        let text = app.book.sheets[si].value(p).display();
        for (off, w) in word_engine::proofing::words(&text) {
            if off < sp.offset || known(app, &w) { continue; }
            if let Some((_, to)) = sp.change_all.iter().find(|(a, _)| *a == w).cloned() { replace_word(app, p, off, &w, &to); sp.offset = off + to.chars().count(); return next_issue(app, sp); }
            sp.suggestions = word_engine::proofing::suggest(&w, 8);
            sp.change_to = sp.suggestions.first().cloned().unwrap_or_else(|| w.clone());
            sp.picked = (!sp.suggestions.is_empty()).then_some(0);
            sp.issue = Some((p, off, w));
            app.select(p, false);
            return;
        }
        sp.at += 1; sp.offset = 0;
    }
    sp.issue = None; sp.done = true;
}

fn replace_word(app: &mut SheetApp, p: Pos, off: usize, word: &str, to: &str) {
    let si = app.sheet();
    let text = app.book.sheets[si].value(p).display();
    let chars: Vec<char> = text.chars().collect();
    let n = word.chars().count();
    if off + n > chars.len() { return; }
    let new: String = chars[..off].iter().collect::<String>() + to + chars[off + n..].iter().collect::<String>().as_str();
    app.checkpoint("Spelling");
    app.book.set_cell_input(si, p, sheet_engine::Input::Value(Value::str(new)), None);
    app.changed();
}

fn spelling_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut sp) = app.rv.spelling.take() else { return };
    let mut open = true;
    let mut keep = true;
    crate::theme::dialog(ctx, "Spelling").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_width(440.0);
        let Some((p, off, word)) = sp.issue.clone() else {
            ui.label(tr("Spell check complete. You're good to go!"));
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { keep = false; });
            return;
        };
        ui.label(RichText::new(tr("Not in Dictionary:")).color(c_ink2()));
        let cell_text = app.book.sheets[app.sheet()].value(p).display();
        ui.label(RichText::new(format!("{}  ({})", word, p.a1())).strong().color(Color32::from_rgb(0xC0, 0x20, 0x20)));
        ui.label(RichText::new(cell_text).color(c_ink2()).size(12.0));
        ui.add_space(4.0);
        crate::theme::hrow(ui, |ui| { ui.label(tr("Change to:")); ui.add(egui::TextEdit::singleline(&mut sp.change_to).desired_width(260.0)); });
        ui.label(tr("Suggestions:"));
        egui::ScrollArea::vertical().max_height(120.0).id_salt("spell-sugg").show(ui, |ui| {
            if sp.suggestions.is_empty() { ui.label(RichText::new(tr("(No Suggestions)")).color(c_ink3())); }
            for (i, s) in sp.suggestions.clone().into_iter().enumerate() {
                let r = ui.selectable_label(sp.picked == Some(i), &s);
                if r.clicked() { sp.picked = Some(i); sp.change_to = s.clone(); }
                if r.double_clicked() { sp.change_to = s; }
            }
        });
        ui.add_space(6.0);
        crate::theme::hrow(ui, |ui| {
            if ui.button(tr("Add to Dictionary")).clicked() { let s = settings(app); s.dictionary.push(word.clone()); s.save(); sp.offset = off + word.chars().count(); next_issue(app, &mut sp); }
            if ui.button(tr("Ignore All")).clicked() { app.rv.ignored.insert(word.to_lowercase()); next_issue(app, &mut sp); }
            if ui.button(tr("Ignore Once")).clicked() { sp.offset = off + word.chars().count(); next_issue(app, &mut sp); }
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, tr("Change")).clicked() { let to = sp.change_to.clone(); replace_word(app, p, off, &word, &to); sp.offset = off + to.chars().count(); next_issue(app, &mut sp); }
            if crate::theme::secondary(ui, &tr("Change All")).clicked() { let to = sp.change_to.clone(); sp.change_all.push((word.clone(), to.clone())); replace_word(app, p, off, &word, &to); sp.offset = off + to.chars().count(); next_issue(app, &mut sp); }
            if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
        });
    });
    if open && keep { app.rv.spelling = Some(sp); }
}

pub fn open_thesaurus(app: &mut SheetApp) {
    let t = app.book.sheets[app.sheet()].value(app.sel.active).display();
    let q = t.split_whitespace().next().unwrap_or("").trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    let mut th = Thesaurus { query: q, results: vec![], job: None, error: None };
    lookup(&mut th, &app.ctx);
    app.rv.thesaurus = Some(th);
}

fn lookup(t: &mut Thesaurus, ctx: &egui::Context) {
    if t.query.trim().is_empty() { return; }
    let q: String = t.query.trim().chars().map(|c| if c.is_alphanumeric() { c.to_string() } else { format!("%{:02X}", c as u32 & 0xff) }).collect();
    let slot = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let get = |u: &str| -> Result<serde_json::Value, String> { let body = ureq::get(u).call().map_err(|e| e.to_string())?.into_body().read_to_string().map_err(|e| e.to_string())?; serde_json::from_str(&body).map_err(|e| e.to_string()) };
        let r = (|| {
            let syn = get(&format!("https://api.datamuse.com/words?rel_syn={q}&max=40&md=p"))?;
            let near = get(&format!("https://api.datamuse.com/words?ml={q}&max=20&md=p"))?;
            let pos = |v: &serde_json::Value| v["tags"].as_array().and_then(|t| t.iter().filter_map(|x| x.as_str()).find(|x| ["n", "v", "adj", "adv"].contains(x))).map(|p| match p { "n" => "noun", "v" => "verb", "adj" => "adjective", _ => "adverb" }).unwrap_or("").to_string();
            let mut out: Vec<(String, String)> = vec![];
            for v in syn.as_array().into_iter().flatten().chain(near.as_array().into_iter().flatten()) { if let Some(w) = v["word"].as_str() { if !out.iter().any(|o| o.0 == w) { out.push((w.to_string(), pos(v))); } } }
            Ok(out)
        })();
        *out.lock().unwrap() = Some(r);
        ctx.request_repaint();
    });
    t.job = Some(slot);
    t.error = None;
}

fn thesaurus_pane(app: &mut SheetApp, ui: &mut Ui) {
    let Some(mut t) = app.rv.thesaurus.take() else { return };
    let mut keep = true;
    egui::Panel::right("thesaurus-pane").default_size(260.0).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(10)).show(ui, |ui| {
        crate::theme::hrow(ui, |ui| { ui.label(RichText::new(tr("Thesaurus")).strong()); ui.with_layout(far_layout(), |ui| if close_button(ui, "Close").clicked() { keep = false; }); });
        let go = crate::theme::hrow(ui, |ui| { let r = ui.add(egui::TextEdit::singleline(&mut t.query).desired_width(170.0)); let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)); ui.button(tr("Look Up")).clicked() || enter }).inner;
        if go { lookup(&mut t, &app.ctx); }
        if let Some(j) = t.job.clone() { if let Some(r) = j.lock().unwrap().take() { match r { Ok(v) => t.results = v, Err(e) => t.error = Some(e) } t.job = None; } else { ui.spinner(); } }
        if let Some(e) = &t.error { ui.colored_label(Color32::from_rgb(190, 40, 40), format!("{}\n{e}", tr("The thesaurus needs an internet connection."))); }
        if t.job.is_none() && t.results.is_empty() && t.error.is_none() && !t.query.is_empty() { ui.label(RichText::new(tr("No results.")).color(c_ink3())); }
        egui::ScrollArea::vertical().id_salt("thes-list").show(ui, |ui| {
            for (w, pos) in t.results.clone() {
                crate::theme::hrow(ui, |ui| {
                    if ui.link(&w).on_hover_text(tr("Look up this word")).clicked() { t.query = w.clone(); lookup(&mut t, &app.ctx); }
                    if !pos.is_empty() { ui.label(RichText::new(tr(&pos)).size(11.0).color(c_ink3())); }
                    if ui.small_button(tr("Insert")).clicked() {
                        let si = app.sheet(); let p = app.sel.active;
                        let cur = app.book.sheets[si].value(p).display();
                        let new = if cur.is_empty() { w.clone() } else { cur.replacen(&t.query, &w, 1) };
                        app.checkpoint("Thesaurus"); app.book.set_cell_input(si, p, sheet_engine::Input::Value(Value::str(if new == cur { w.clone() } else { new })), None); app.changed();
                    }
                });
            }
        });
    });
    if keep { app.rv.thesaurus = Some(t); }
}

fn stats_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    if !app.rv.stats { return; }
    let st = rv::statistics(&app.book, app.sheet());
    let mut open = true;
    crate::theme::dialog(ctx, "Workbook Statistics").open(&mut open).show(ctx, |ui| {
        ui.set_width(320.0);
        crate::theme::section(ui, &tr("Current sheet"));
        egui::Grid::new("stats-sheet").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
            let row = |ui: &mut Ui, k: &str, v: String| { ui.label(tr(k)); ui.label(v); ui.end_row(); };
            row(ui, "End of sheet", st.sheet_end.map(|p| p.a1()).unwrap_or_else(|| "A1".into()));
            row(ui, "Cells with data", st.cells.to_string());
            row(ui, "Formulas", st.formulas.to_string());
            row(ui, "Tables", st.tables.to_string());
            row(ui, "PivotTables", st.pivots.to_string());
            row(ui, "Charts and objects", st.objects.to_string());
            row(ui, "Notes", st.notes.to_string());
            row(ui, "Comments", st.threads.to_string());
        });
        crate::theme::section(ui, &tr("Workbook"));
        egui::Grid::new("stats-book").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
            let row = |ui: &mut Ui, k: &str, v: String| { ui.label(tr(k)); ui.label(v); ui.end_row(); };
            row(ui, "Sheets", st.sheets.to_string());
            row(ui, "Cells with data", st.book_cells.to_string());
            row(ui, "Formulas", st.book_formulas.to_string());
            row(ui, "Tables", st.book_tables.to_string());
            row(ui, "Notes", st.book_notes.to_string());
            row(ui, "Comments", st.book_threads.to_string());
        });
        crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { app.rv.stats = false; });
    });
    if !open { app.rv.stats = false; }
}

pub fn smart_lookup(app: &mut SheetApp) {
    let t = app.book.display(app.sheet(), app.sel.active);
    if t.trim().is_empty() { app.notify(&tr("Select a cell with text to look up.")); return; }
    let q: String = t.trim().bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    let _ = crate::grid::open_url(&format!("https://duckduckgo.com/?q={q}"));
}

pub fn open_translate(app: &mut SheetApp) {
    let si = app.sheet();
    let r = app.sel.range();
    let r = Range { r1: r.r1.min(r.r0 + 500), c1: r.c1.min(r.c0 + 50), ..r };
    let text: Vec<String> = r.iter().map(|p| app.book.sheets[si].value(p).display()).collect();
    let mut t = qalam_kit::translate_ui::Translator::new(&text.join("\n"), if crate::i18n::rtl() { "en" } else { "ar" });
    t.insert_label = Some("Insert".into());
    app.rv.translator = Some(t);
}

fn translate_window(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut t) = app.rv.translator.take() else { return };
    let mut open = true;
    let action = qalam_kit::translate_ui::window(ctx, &mut t, &mut open);
    if let Some(qalam_kit::translate_ui::Action::Insert(out)) = action {
        // One line back into each selected cell, in reading order.
        let si = app.sheet();
        let r = app.sel.range();
        app.checkpoint("Translate");
        for (p, line) in r.iter().zip(out.lines()) { if !app.book.sheets[si].value(p).is_empty() || !line.is_empty() { app.book.set_cell_input(si, p, sheet_engine::Input::Value(Value::str(line)), None); } }
        app.changed();
    }
    if open { app.rv.translator = Some(t); }
}

// ---------------------------------------------------------------------------------------------
// Accessibility

pub struct Finding { pub error: bool, pub title: &'static str, pub detail: String, pub sheet: usize, pub at: Option<Pos> }

fn luminance(c: [u8; 3]) -> f32 {
    let f = |v: u8| { let x = v as f32 / 255.0; if x <= 0.03928 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) } };
    0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])
}
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 { let (x, y) = (luminance(a), luminance(b)); (x.max(y) + 0.05) / (x.min(y) + 0.05) }

pub fn findings(book: &sheet_engine::Workbook) -> Vec<Finding> {
    let mut out = vec![];
    for (si, s) in book.sheets.iter().enumerate() {
        for o in &s.objects { if o.descr.trim().is_empty() { out.push(Finding { error: true, title: "Missing alternative text", detail: format!("{} ({})", o.name, s.name), sheet: si, at: None }); } }
        for t in &s.tables { if !t.header_row { out.push(Finding { error: true, title: "No header row", detail: format!("{} ({})", t.display_name, s.name), sheet: si, at: Some(t.range.start()) }); } }
        for m in &s.merges { out.push(Finding { error: false, title: "Merged cells", detail: format!("{}!{}", s.name, m.a1()), sheet: si, at: Some(m.start()) }); }
        let default_name = s.name.strip_prefix("Sheet").is_some_and(|n| n.parse::<u32>().is_ok());
        if default_name && book.sheets.len() > 1 && !s.cells.is_empty() { out.push(Finding { error: false, title: "Default sheet name", detail: s.name.clone(), sheet: si, at: None }); }
        let mut hard = 0;
        for (p, c) in s.cells.iter().filter(|(_, c)| !c.is_blank()) {
            let st = book.styles.get(c.style);
            if st.color.is_none() && st.fill.is_none() { continue; }
            let (fg, bg) = (st.color.unwrap_or([0, 0, 0]), st.fill.unwrap_or([255, 255, 255]));
            if contrast(fg, bg) < 4.5 && hard < 50 { hard += 1; out.push(Finding { error: false, title: "Hard-to-read text contrast", detail: format!("{}!{}", s.name, p.a1()), sheet: si, at: Some(*p) }); }
        }
        if s.hidden { out.push(Finding { error: false, title: "Hidden sheet", detail: s.name.clone(), sheet: si, at: None }); }
    }
    out
}

fn access_pane(app: &mut SheetApp, ui: &mut Ui) {
    if !app.rv.access { return; }
    let list = findings(&app.book);
    egui::Panel::right("access-pane").default_size(280.0).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(10)).show(ui, |ui| {
        crate::theme::hrow(ui, |ui| { ui.label(RichText::new(tr("Accessibility")).strong()); ui.with_layout(far_layout(), |ui| if close_button(ui, "Close").clicked() { app.rv.access = false; }); });
        if list.is_empty() { ui.label(tr("No accessibility issues found. People with disabilities should have no trouble reading this workbook.")); return; }
        egui::ScrollArea::vertical().id_salt("access-list").show(ui, |ui| {
            for (heading, err) in [("Errors", true), ("Warnings", false)] {
                let items: Vec<&Finding> = list.iter().filter(|f| f.error == err).collect();
                if items.is_empty() { continue; }
                crate::theme::section(ui, &format!("{} ({})", tr(heading), items.len()));
                for f in items {
                    let r = ui.add(egui::Button::new(RichText::new(format!("{}\n{}", tr(f.title), f.detail)).size(12.0)).frame(false));
                    if r.clicked() { if f.sheet != app.sheet() { app.set_sheet(f.sheet); } if let Some(p) = f.at { app.select(p, false); } }
                    r.on_hover_text(tr(match f.title { "Missing alternative text" => "Add a description so screen readers can say what the object shows.", "Merged cells" => "Merged cells confuse screen readers; unmerge them or use Center Across Selection.", "Hard-to-read text contrast" => "The text color doesn't stand out enough from the fill.", "No header row" => "A table needs a header row to say what each column holds.", "Default sheet name" => "Give the sheet a name that says what it holds.", _ => "Hidden content may be missed." }));
                }
            }
        });
    });
}

// ---------------------------------------------------------------------------------------------
// Comments

pub fn new_comment(app: &mut SheetApp) {
    app.commit(None);
    app.rv.comment = Some(CommentDraft { sheet: app.sheet(), pos: app.sel.active, text: String::new(), focus: true });
}

fn browse_comments(app: &mut SheetApp, forward: bool) {
    let all: Vec<(usize, Pos)> = app.book.sheets.iter().enumerate().flat_map(|(i, s)| s.review.threads.keys().map(move |p| (i, *p))).collect();
    let cur = (app.sheet(), app.sel.active);
    let next = if forward { all.iter().find(|x| **x > cur).or(all.first()) } else { all.iter().rev().find(|x| **x < cur).or(all.last()) };
    if let Some(&(s, p)) = next { if s != app.sheet() { app.set_sheet(s); } app.select(p, false); app.rv.comment = Some(CommentDraft { sheet: s, pos: p, text: String::new(), focus: false }); }
}

fn browse_notes(app: &mut SheetApp, forward: bool) {
    let s = app.sheet(); let p = app.sel.active;
    let keys: Vec<Pos> = app.book.sheets[s].notes.keys().copied().collect();
    let q = if forward { keys.iter().find(|q| **q > p).or(keys.first()) } else { keys.iter().rev().find(|q| **q < p).or(keys.last()) };
    if let Some(q) = q { app.select(*q, false); }
}

pub fn convert_notes(app: &mut SheetApp) {
    let s = app.sheet();
    if app.book.sheets[s].notes.is_empty() { app.notify(&tr("There are no notes on this sheet.")); return; }
    app.checkpoint("Convert to Comments");
    let notes: Vec<(Pos, String)> = std::mem::take(&mut app.book.sheets[s].notes).into_iter().collect();
    let author = if app.prefs.author.is_empty() { tr("Author") } else { app.prefs.author.clone() };
    for (p, t) in notes { rv::add_comment(&mut app.book, s, p, &author, &t); }
    app.dirty = true;
}

/// The people whose names start with what follows the last '@' in the text.
fn mention_matches(app: &SheetApp, text: &str) -> Vec<String> {
    let Some(i) = text.rfind('@') else { return vec![] };
    let part = &text[i + 1..];
    if part.contains(char::is_whitespace) && !app.book.review.persons.iter().any(|p| p.name.starts_with(part)) { return vec![]; }
    let mut names: Vec<String> = app.book.review.persons.iter().map(|p| p.name.clone()).collect();
    if !app.prefs.author.is_empty() && !names.contains(&app.prefs.author) { names.push(app.prefs.author.clone()); }
    names.into_iter().filter(|n| n.to_lowercase().starts_with(&part.to_lowercase()) && n != part).take(6).collect()
}

fn mention_box(app: &SheetApp, ui: &mut Ui, text: &mut String) {
    for n in mention_matches(app, text) {
        if ui.button(format!("@{n}")).clicked() { if let Some(i) = text.rfind('@') { text.truncate(i); text.push('@'); text.push_str(&n); text.push(' '); } }
    }
}

fn thread_view(app: &mut SheetApp, ui: &mut Ui, si: usize, p: Pos) -> bool {
    let Some(t) = app.book.sheets[si].review.threads.get(&p).cloned() else { return false };
    let mut deleted = false;
    for (k, c) in t.comments.iter().enumerate() {
        crate::theme::hrow(ui, |ui| {
            let name = app.book.review.name_of(&c.person);
            ui.label(RichText::new(if name.is_empty() { tr("Unknown") } else { name }).strong().color(COMMENT_COLOR));
            ui.label(RichText::new(c.date.get(..16).unwrap_or(&c.date).replace('T', " ")).size(11.0).color(c_ink3()));
            if k > 0 && ui.small_button("✕").on_hover_text(tr("Delete reply")).clicked() { app.checkpoint("Delete Reply"); if let Some(t) = app.book.sheets[si].review.threads.get_mut(&p) { t.comments.remove(k); } app.dirty = true; }
        });
        ui.label(&c.text);
        ui.add_space(2.0);
    }
    crate::theme::hrow(ui, |ui| {
        if ui.button(tr(if t.done { "Reopen" } else { "Resolve" })).clicked() { app.checkpoint("Resolve"); if let Some(t) = app.book.sheets[si].review.threads.get_mut(&p) { t.done = !t.done; } app.dirty = true; }
        if ui.button(tr("Delete Thread")).clicked() { app.checkpoint("Delete Comment"); app.book.sheets[si].review.threads.remove(&p); app.dirty = true; deleted = true; }
    });
    !deleted
}

fn post(app: &mut SheetApp, si: usize, p: Pos, text: &str) {
    if text.trim().is_empty() { return; }
    app.checkpoint("Comment");
    let author = if app.prefs.author.is_empty() { tr("Author") } else { app.prefs.author.clone() };
    rv::add_comment(&mut app.book, si, p, &author, text.trim());
    app.dirty = true;
}

fn comment_window(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = app.rv.comment.take() else { return };
    if d.sheet >= app.book.sheets.len() { return; }
    let mut open = true; let mut keep = true;
    let title = format!("{} {}", tr("Comment"), d.pos.a1());
    let at = { let lay = crate::grid::layout(app, app.grid_rect); lay.cell_rect(d.pos).right_top() + vec2(8.0, 0.0) };
    egui::Window::new(title).id(egui::Id::new("comment-window")).default_pos(at).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_width(300.0);
        let has = thread_view(app, ui, d.sheet, d.pos);
        let done = app.book.sheets[d.sheet].review.threads.get(&d.pos).is_some_and(|t| t.done);
        if done { ui.label(RichText::new(tr("This thread is resolved.")).color(c_ink3())); return; }
        let r = ui.add(egui::TextEdit::multiline(&mut d.text).hint_text(tr(if has { "Reply…" } else { "Start a conversation. Type @ to mention someone." })).desired_rows(3).desired_width(290.0));
        if d.focus { r.request_focus(); d.focus = false; }
        mention_box(app, ui, &mut d.text);
        let send = r.has_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, !d.text.trim().is_empty(), tr("Post")).clicked() || send { post(app, d.sheet, d.pos, &d.text); d.text.clear(); }
            if crate::theme::secondary(ui, &tr("Close")).clicked() { keep = false; }
        });
    });
    if open && keep { app.rv.comment = Some(d); }
}

fn comments_pane(app: &mut SheetApp, ui: &mut Ui) {
    if !app.rv.comments_pane { return; }
    let si = app.sheet();
    egui::Panel::right("comments-pane").default_size(300.0).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(10)).show(ui, |ui| {
        crate::theme::hrow(ui, |ui| {
            ui.label(RichText::new(tr("Comments")).strong());
            ui.with_layout(far_layout(), |ui| { if close_button(ui, "Close").clicked() { app.rv.comments_pane = false; } if ui.button(tr("New")).clicked() { new_comment(app); } });
        });
        let keys: Vec<Pos> = app.book.sheets[si].review.threads.keys().copied().collect();
        if keys.is_empty() { ui.label(RichText::new(tr("There are no comments on this sheet. Select a cell and choose New Comment.")).color(c_ink3())); }
        egui::ScrollArea::vertical().id_salt("comments-list").show(ui, |ui| {
            for p in keys {
                egui::Frame::NONE.fill(c_card()).stroke(Stroke::new(1.0, if app.sel.active == p { COMMENT_COLOR } else { c_line() })).inner_margin(8).corner_radius(4.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if ui.link(RichText::new(p.a1()).strong()).clicked() { app.select(p, false); }
                    if !thread_view(app, ui, si, p) { return; }
                    if app.book.sheets[si].review.threads.get(&p).is_some_and(|t| t.done) { return; }
                    let mut reply = app.rv.replies.remove(&(si, p)).unwrap_or_default();
                    let r = ui.add(egui::TextEdit::singleline(&mut reply).hint_text(tr("Reply…")).desired_width(f32::INFINITY));
                    mention_box(app, ui, &mut reply);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { post(app, si, p, &reply); reply.clear(); }
                    if !reply.is_empty() { app.rv.replies.insert((si, p), reply); }
                });
                ui.add_space(6.0);
            }
        });
    });
}

// ---------------------------------------------------------------------------------------------
// Protection

fn protect_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = app.rv.protect.take() else { return };
    let mut open = true; let mut keep = true;
    crate::theme::dialog(ctx, "Protect Sheet").open(&mut open).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.label(tr("Password to unprotect sheet (optional):"));
        ui.add(egui::TextEdit::singleline(&mut d.password).password(true).desired_width(320.0));
        if !d.password.is_empty() { ui.label(tr("Reenter password to proceed:")); ui.add(egui::TextEdit::singleline(&mut d.confirm).password(true).desired_width(320.0)); }
        ui.add_space(4.0);
        ui.label(tr("Allow all users of this worksheet to:"));
        egui::ScrollArea::vertical().max_height(220.0).id_salt("prot-actions").show(ui, |ui| {
            for (i, (_, label)) in rv::ACTIONS.iter().enumerate() { crate::theme::check(ui, &mut d.allow[i], tr(label)); }
        });
        if let Some(e) = &d.error { ui.colored_label(Color32::from_rgb(190, 40, 40), tr(e)); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                if !d.password.is_empty() && d.password != d.confirm { d.error = Some("Confirmation password is not identical.".into()); return; }
                app.commit(None);
                app.checkpoint("Protect Sheet");
                let s = app.sheet();
                rv::protect_sheet(&mut app.book.sheets[s], SheetProtection { password: rv::PwHash::new(&d.password), allow: d.allow });
                app.dirty = true; keep = false;
            }
            if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
        });
    });
    if open && keep { app.rv.protect = Some(d); }
}

fn password_dialogs(app: &mut SheetApp, ctx: &egui::Context) {
    if let Some((book, mut pw, err)) = app.rv.unprotect.take() {
        let mut open = true; let mut keep = true; let mut error = err;
        crate::theme::dialog(ctx, if book { "Unprotect Workbook" } else { "Unprotect Sheet" }).open(&mut open).show(ctx, |ui| {
            ui.label(tr("Password:"));
            let r = ui.add(egui::TextEdit::singleline(&mut pw).password(true).desired_width(260.0));
            r.request_focus();
            if let Some(e) = &error { ui.colored_label(Color32::from_rgb(190, 40, 40), tr(e)); }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "OK").clicked() || enter {
                    let s = app.sheet();
                    let ok = if book { rv::unprotect_workbook(&mut app.book, &pw) } else { rv::unprotect_sheet(&mut app.book.sheets[s], &pw) };
                    if ok { app.dirty = true; keep = false; } else { error = Some("The password you supplied is not correct.".into()); }
                }
                if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
            });
        });
        if open && keep { app.rv.unprotect = Some((book, pw, error)); }
    }
    if let Some((mut pw, mut confirm, mut err)) = app.rv.protect_book.take() {
        let mut open = true; let mut keep = true;
        crate::theme::dialog(ctx, "Protect Structure and Windows").open(&mut open).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.label(tr("Password (optional):"));
            ui.add(egui::TextEdit::singleline(&mut pw).password(true).desired_width(280.0));
            if !pw.is_empty() { ui.label(tr("Reenter password to proceed:")); ui.add(egui::TextEdit::singleline(&mut confirm).password(true).desired_width(280.0)); }
            let mut on = true;
            ui.add_enabled(false, egui::Checkbox::new(&mut on, tr("Structure")));
            if let Some(e) = &err { ui.colored_label(Color32::from_rgb(190, 40, 40), tr(e)); }
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "OK").clicked() {
                    if !pw.is_empty() && pw != confirm { err = Some("Confirmation password is not identical.".into()); return; }
                    rv::protect_workbook(&mut app.book, &pw); app.dirty = true; keep = false;
                }
                if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
            });
        });
        if open && keep { app.rv.protect_book = Some((pw, confirm, err)); }
    }
    if let Some((name, mut pw, p)) = app.rv.range_password.take() {
        let mut open = true; let mut keep = true;
        crate::theme::dialog(ctx, "Unlock Range").open(&mut open).show(ctx, |ui| {
            ui.label(format!("{} \"{}\" {}", tr("A cell you are trying to change is in range"), name, tr("and requires a password.")));
            ui.add(egui::TextEdit::singleline(&mut pw).password(true).desired_width(260.0)).request_focus();
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let s = app.sheet();
                    let ok = app.book.sheets[s].review.edit_ranges.iter().find(|r| r.name == name).and_then(|r| r.password.as_ref()).is_none_or(|h| h.matches(&pw));
                    if ok { app.rv.unlocked.push(name.clone()); keep = false; app.select(p, false); app.begin_edit(None, false); } else { app.error = Some(tr("The password you supplied is not correct.")); keep = false; }
                }
                if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
            });
        });
        if open && keep { app.rv.range_password = Some((name, pw, p)); }
    }
}

fn ranges_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = app.rv.edit_ranges.take() else { return };
    let mut open = true; let mut keep = true;
    crate::theme::dialog(ctx, "Allow Users to Edit Ranges").open(&mut open).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.label(tr("Ranges unlocked by a password when the sheet is protected:"));
        egui::Grid::new("ranges").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new(tr("Title")).strong()); ui.label(RichText::new(tr("Refers to cells")).strong()); ui.label(RichText::new(tr("Password")).strong()); ui.label(""); ui.end_row();
            let mut del = None;
            for (i, (n, r, pw)) in d.list.iter_mut().enumerate() {
                ui.add(egui::TextEdit::singleline(n).desired_width(110.0));
                ui.add(egui::TextEdit::singleline(r).desired_width(130.0));
                ui.add(egui::TextEdit::singleline(pw).password(true).desired_width(100.0).hint_text(tr("(keep)")));
                if ui.small_button("✕").clicked() { del = Some(i); }
                ui.end_row();
            }
            if let Some(i) = del { d.list.remove(i); }
        });
        if ui.button(tr("New…")).clicked() { let n = format!("{}{}", tr("Range"), d.list.len() + 1); d.list.push((n, app.sel.range().a1(), String::new())); }
        if let Some(e) = &d.error { ui.colored_label(Color32::from_rgb(190, 40, 40), tr(e)); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let s = app.sheet();
                let old = app.book.sheets[s].review.edit_ranges.clone();
                let mut out = vec![];
                for (n, r, pw) in &d.list {
                    let ranges: Vec<Range> = r.split([' ', ',']).filter(|x| !x.is_empty()).filter_map(|x| sheet_engine::addr::parse_range(x.trim_start_matches('='))).collect();
                    if ranges.is_empty() || n.trim().is_empty() { d.error = Some("Each range needs a title and cells such as A1:B5.".into()); return; }
                    let password = if pw.is_empty() { old.iter().find(|o| o.name == *n).and_then(|o| o.password.clone()) } else { rv::PwHash::new(pw) };
                    out.push(rv::EditRange { name: n.trim().into(), ranges, password });
                }
                app.checkpoint("Allow Edit Ranges");
                rv::set_edit_ranges(&mut app.book.sheets[s], out);
                app.dirty = true; keep = false;
            }
            if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
        });
    });
    if open && keep { app.rv.edit_ranges = Some(d); }
}

/// Whether the active sheet's protection allows an action; tells why not.
pub fn blocked(app: &mut SheetApp, a: Action) -> bool {
    let s = &app.book.sheets[app.sheet()];
    if rv::allowed(s, a) { return false; }
    app.error = Some(tr("The cell or chart you're trying to change is on a protected sheet. To make a change, unprotect the sheet."));
    true
}

/// Whether changing the selected cells' contents is blocked (locked cells on a protected sheet).
pub fn locked_selection(app: &mut SheetApp) -> bool {
    let si = app.sheet();
    if !app.book.sheets[si].protected { return false; }
    let unlocked = app.rv.unlocked.clone();
    let blocked = app.sel.ranges().iter().any(|r| { let r = Range { r1: r.r1.min(r.r0 + 2000), c1: r.c1.min(r.c0 + 200), ..*r }; r.iter().any(|p| rv::editable(&app.book, si, p, &unlocked).is_err()) });
    if blocked { app.error = Some(tr("The cell you're trying to change is on a protected sheet.")); }
    blocked
}

/// Whether a cell may be edited; asks for an edit range's password when it needs one.
pub fn may_edit(app: &mut SheetApp, si: usize, p: Pos) -> bool {
    match rv::editable(&app.book, si, p, &app.rv.unlocked) {
        Ok(()) => true,
        Err(Some(range)) => { app.rv.range_password = Some((range, String::new(), p)); false }
        Err(None) => { app.error = Some(tr("The cell you're trying to change is on a protected sheet.")); false }
    }
}

/// Whether the workbook's structure is locked (sheets can't be added, removed or renamed).
pub fn structure_locked(app: &mut SheetApp) -> bool {
    if !app.book.review.structure { return false; }
    app.error = Some(tr("The workbook is protected: sheets can't be added, deleted, moved, renamed or hidden. Unprotect it on the Review tab."));
    true
}

// ---------------------------------------------------------------------------------------------
// Entry: AutoCorrect and AutoComplete

/// The value from the column that the typed text completes, if exactly one does.
pub fn completion(app: &SheetApp) -> Option<String> { completion_for(app, app.edit.as_ref()?) }

pub fn completion_for(app: &SheetApp, e: &crate::app::Editor) -> Option<String> {
    if e.text.is_empty() || e.text.starts_with(['=', '+', '-']) || e.text.chars().next().is_some_and(|c| c.is_ascii_digit()) || e.cursor != e.text.chars().count() { return None; }
    if app.rv.ac_reject.as_deref() == Some(e.text.as_str()) { return None; }
    if !app.rv.settings.as_ref().is_none_or(|s| s.autocomplete) { return None; }
    let s = &app.book.sheets[e.sheet];
    let low = e.text.to_lowercase();
    let mut found: Option<String> = None;
    // The block of entries the cell sits in, up and down the column.
    for dir in [-1i64, 1] {
        let mut r = e.pos.row as i64 + dir;
        let mut gap = 0;
        while r >= 0 && r < sheet_engine::addr::MAX_ROWS as i64 && gap < 1 {
            let v = s.value(Pos::new(r as u32, e.pos.col));
            match &v {
                Value::Empty => gap += 1,
                Value::Str(t) => {
                    let t = t.to_string();
                    if t.to_lowercase().starts_with(&low) && t.chars().count() > e.text.chars().count() {
                        match &found { Some(f) if !f.eq_ignore_ascii_case(&t) => return None, _ => found = Some(t) }
                    }
                }
                _ => {}
            }
            r += dir;
        }
    }
    found
}

/// What an entry becomes: completed from the column, then AutoCorrected.
pub fn finish_entry(app: &mut SheetApp, e: &crate::app::Editor) -> String {
    let mut text = e.text.clone();
    // Reopening a cell and leaving it as it was changes nothing.
    if app.book.edit_text(e.sheet, e.pos) == text { return text; }
    if let Some(c) = completion_for(app, e) { text = c; }
    app.rv.ac_reject = None;
    if text.starts_with('=') { return text; }
    let st = settings(app).clone();
    if !st.autocorrect_on { return text; }
    autocorrect(&text, &st.autocorrect)
}

/// Replaces whole words (and symbol sequences) from the AutoCorrect list.
pub fn autocorrect(text: &str, list: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if let Some((_, to)) = list.iter().find(|(a, _)| a == word) { out.push_str(to); } else { out.push_str(word); }
        word.clear();
    };
    for c in text.chars() { if c.is_whitespace() { flush(&mut word, &mut out); out.push(c); } else { word.push(c); } }
    flush(&mut word, &mut out);
    out
}

// ---------------------------------------------------------------------------------------------
// The grid: indicators, notes shown, the completion

fn overlay(app: &mut SheetApp, ctx: &egui::Context) {
    if app.grid_rect.width() <= 0.0 { return; }
    let si = app.sheet();
    let lay = crate::grid::layout(app, app.grid_rect);
    let view = lay.view();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("review-overlay"))).with_clip_rect(view);
    let pointer = ctx.pointer_hover_pos();
    for (p, t) in app.book.sheets[si].review.threads.iter() {
        let r = lay.cell_rect(*p);
        if !view.intersects(r) { continue; }
        let c = if t.done { c_ink3() } else { COMMENT_COLOR };
        painter.add(egui::Shape::convex_polygon(vec![r.right_top(), r.right_top() + vec2(-7.0, 0.0), r.right_top() + vec2(0.0, 7.0)], c, Stroke::NONE));
        if pointer.is_some_and(|q| r.contains(q)) && app.rv.comment.is_none() {
            let first = t.comments.first().map(|c| format!("{}: {}", app.book.review.name_of(&c.person), c.text)).unwrap_or_default();
            let more = t.comments.len().saturating_sub(1);
            let text = if more > 0 { format!("{first}\n+{more} {}", tr("replies")) } else { first };
            let g = painter.layout(text, egui::FontId::proportional(12.0), c_ink(), 240.0);
            let b = Rect::from_min_size(r.right_top() + vec2(6.0, 0.0), g.size() + vec2(12.0, 8.0));
            painter.rect_filled(b, 4.0, c_card()); painter.rect_stroke(b, 4.0, Stroke::new(1.0, COMMENT_COLOR), egui::StrokeKind::Inside);
            painter.galley(b.min + vec2(6.0, 4.0), g, c_ink());
        }
    }
    if app.rv.show_notes {
        for (p, n) in app.book.sheets[si].notes.iter() {
            let r = lay.cell_rect(*p);
            if !view.intersects(r.expand(200.0)) { continue; }
            let g = painter.layout(n.clone(), egui::FontId::proportional(12.0), Color32::BLACK, 160.0);
            let b = Rect::from_min_size(r.right_top() + vec2(10.0, 0.0), g.size() + vec2(10.0, 8.0));
            painter.line_segment([r.right_top(), b.left_top()], Stroke::new(1.0, Color32::BLACK));
            painter.rect_filled(b, 0.0, Color32::from_rgb(0xFF, 0xFF, 0xE1)); painter.rect_stroke(b, 0.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
            painter.galley(b.min + vec2(5.0, 4.0), g, Color32::BLACK);
        }
    }
    // The completion, highlighted after the typed text.
    if let (Some(c), Some(e)) = (completion(app), app.edit.as_ref()) {
        if e.sheet == si && !e.focus_bar {
            let st = app.book.style_at(si, e.pos).clone();
            let font = crate::grid::font_for(&st, st.size_pt() * 96.0 / 72.0 * lay.z);
            let typed_w = painter.layout_no_wrap(e.text.clone(), font.clone(), Color32::BLACK).size().x;
            let rest: String = c.chars().skip(e.text.chars().count()).collect();
            let g = painter.layout_no_wrap(rest, font, Color32::WHITE);
            let cell = lay.cell_rect(e.pos);
            let at = pos2(cell.min.x + 2.0 + typed_w + 2.0, cell.min.y + 1.0);
            let top = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("ac-ghost")));
            top.rect_filled(Rect::from_min_size(at, g.size()), 0.0, crate::grid::SELECT);
            top.galley(at, g, Color32::WHITE);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Hooks

/// Cell menu items for comments, Smart Lookup and Translate.
pub fn cell_menu(app: &mut SheetApp, ui: &mut Ui) {
    let has = app.book.sheets[app.sheet()].review.threads.contains_key(&app.sel.active);
    if menu_item(ui, "comment", if has { "Reply to Comment" } else { "New Comment" }) { new_comment(app); ui.close(); }
    if has && menu_item(ui, "deleteComment", "Delete Comment") { app.checkpoint("Delete Comment"); let s = app.sheet(); let p = app.sel.active; app.book.sheets[s].review.threads.remove(&p); app.dirty = true; ui.close(); }
    if menu_item(ui, "smartLookup", "Smart Lookup") { smart_lookup(app); ui.close(); }
    if menu_item(ui, "translate", "Translate") { open_translate(app); ui.close(); }
}

/// Side panes (before the grid takes the rest of the window).
pub fn panes(app: &mut SheetApp, ui: &mut Ui) {
    comments_pane(app, ui);
    access_pane(app, ui);
    thesaurus_pane(app, ui);
}

/// Keys: F7 spelling, Shift+F2 a comment; Backspace turns AutoComplete off for the entry.
pub fn frame_keys(app: &mut SheetApp, ctx: &egui::Context) {
    if app.dialog.is_none() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F7)) { start_spelling(app); }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F2)) && app.edit.is_none() { new_comment(app); }
    // Backspace or Delete while typing turns the completion off for that text.
    if let Some(e) = &app.edit { if ctx.input(|i| i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete)) { let mut t = e.text.clone(); if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) { t.pop(); } app.rv.ac_reject = Some(t); } }
    if app.edit.is_none() { app.rv.ac_reject = None; }
}

/// Dialogs and the grid overlay, each frame.
pub fn frame(app: &mut SheetApp, ctx: &egui::Context) {
    spelling_dialog(app, ctx);
    stats_dialog(app, ctx);
    translate_window(app, ctx);
    comment_window(app, ctx);
    protect_dialog(app, ctx);
    password_dialogs(app, ctx);
    ranges_dialog(app, ctx);
    overlay(app, ctx);
}

#[cfg(test)]
mod tests {
    #[test]
    fn autocorrects_words() {
        let l = super::default_autocorrect();
        assert_eq!(super::autocorrect("teh cat (c) 2024", &l), "the cat © 2024");
        assert_eq!(super::autocorrect("tehx", &l), "tehx");
    }
    #[test]
    fn contrast_ratio() {
        assert!((super::contrast([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 0.1);
        assert!(super::contrast([200, 200, 200], [255, 255, 255]) < 4.5);
    }
}
