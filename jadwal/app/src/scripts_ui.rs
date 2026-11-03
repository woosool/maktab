//! The Scripts pane (automate.js, script-gallery.js, script-recorder.js): Insert ▸ Add-ins ▸
//! Scripts. Scripts saved in the workbook, a gallery of samples, an editor, Run on the current
//! selection (one undo step; a script that fails changes nothing), and Record Actions, which
//! writes what you do to cells as a script you can replay and edit. The language is
//! sheet_engine::scripts — a small command language that can only touch the workbook.

use crate::app::SheetApp;
use crate::i18n::tr;
use sheet_engine::scripts::{self, Script};
use std::cell::RefCell;

#[derive(Default)]
pub struct Pane {
    pub open: bool,
    pub gallery: bool,
    /// The script being edited: its index in the workbook's scripts (None: new) and its text.
    pub editing: Option<(Option<usize>, Script)>,
    pub error: String,
    /// Recording: the sheet as it was after the last recorded change, and the lines so far.
    pub recording: Option<Recording>,
}

pub struct Recording { pub sheet: usize, pub before: sheet_engine::Sheet, pub styles: sheet_engine::style::Styles, pub lines: Vec<String> }

thread_local! { pub static PANE: RefCell<Pane> = RefCell::new(Pane::default()); }

pub fn open(app: &mut SheetApp) { let _ = app; PANE.with(|p| p.borrow_mut().open = true); }
pub fn is_open() -> bool { PANE.with(|p| p.borrow().open) }

/// Insert ▸ Add-ins: what is installed (no add-ins ship with Jadwal), the workbook's scripts, Manage….
pub fn add_ins_menu(app: &mut SheetApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new(tr("No add-ins are installed.")).weak());
    let list = scripts::load(&app.book);
    if !list.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new(tr("Scripts in this workbook")).small().strong());
        for (i, s) in list.iter().enumerate() {
            if ui.button(format!("▶ {}", s.name)).on_hover_text(&s.description).clicked() { let code = list[i].code.clone(); let _ = run(app, &code); ui.close(); }
        }
    }
    ui.separator();
    if ui.button(tr("Manage…")).clicked() { open(app); ui.close(); }
}

/// Runs a script on the selection as one undoable step; on an error nothing is changed.
pub fn run(app: &mut SheetApp, code: &str) -> Result<(), String> {
    app.commit(None);
    app.checkpoint("Run Script");
    let si = app.sheet();
    let parts = app.book.parts.clone();
    match scripts::run(&mut app.book, si, app.sel.range(), code) {
        Ok(o) => {
            app.book.active = o.sheet;
            app.sel = crate::app::Selection { anchor: o.sel.start(), active: o.sel.end(), extra: vec![] };
            app.changed();
            let msg = if o.messages.is_empty() { format!("{} ({} {})", tr("Script finished"), o.cells, tr("changes")) } else { o.messages.join(" · ") };
            app.notify(&msg);
            Ok(())
        }
        Err(e) => {
            app.undo();
            app.book.parts = parts;
            let m = format!("{} {}: {}", tr("Line"), e.line, tr(&e.message));
            app.notify(&m);
            Err(m)
        }
    }
}

/// After every change: the recorder writes what changed on the recorded sheet.
pub fn after_change(app: &mut SheetApp) {
    PANE.with(|p| {
        let mut p = p.borrow_mut();
        let Some(r) = p.recording.as_mut() else { return };
        if r.sheet >= app.book.sheets.len() { return; }
        let lines = scripts::record_diff(&r.before, &r.styles, &app.book, r.sheet);
        r.lines.extend(lines);
        r.before = app.book.sheets[r.sheet].clone();
        r.styles = app.book.styles.clone();
    });
}

fn start_recording(app: &SheetApp) {
    let si = app.sheet();
    PANE.with(|p| p.borrow_mut().recording = Some(Recording { sheet: si, before: app.book.sheets[si].clone(), styles: app.book.styles.clone(), lines: vec![] }));
}

fn stop_recording(app: &SheetApp) {
    PANE.with(|p| {
        let mut p = p.borrow_mut();
        let Some(r) = p.recording.take() else { return };
        let name = format!("{} {}", tr("Recorded Script"), scripts::load(&app.book).len() + 1);
        let mut code = format!("# {}\n", tr("Recorded actions (cell addresses are absolute)"));
        code.push_str(&r.lines.join("\n"));
        if r.lines.is_empty() { code.push_str(&format!("# {}\n", tr("Nothing was recorded."))); } else { code.push('\n'); }
        p.editing = Some((None, Script { name, description: tr("Recorded"), code }));
        p.gallery = false;
    });
}

fn save_scripts(app: &mut SheetApp, list: &[Script]) { scripts::store(&mut app.book, list); app.dirty = true; }

/// The pane, at the right of the window (call before the grid's central panel).
pub fn panel(app: &mut SheetApp, ui: &mut egui::Ui) {
    if !is_open() { return; }
    egui::Panel::right("jadwal-scripts-pane").resizable(true).default_size(330.0).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading(tr("Scripts"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if ui.button("✕").on_hover_text(tr("Close")).clicked() { PANE.with(|p| p.borrow_mut().open = false); } });
        });
        let recording = PANE.with(|p| p.borrow().recording.as_ref().map(|r| r.lines.len()));
        ui.horizontal(|ui| {
            match recording {
                Some(n) => { if ui.button(format!("■ {}", tr("Stop Recording"))).clicked() { stop_recording(app); } ui.label(format!("{n} {}", tr("steps"))); }
                None => { if ui.button(format!("● {}", tr("Record Actions"))).on_hover_text(tr("Record what you do to cells as a script")).clicked() { start_recording(app); } }
            }
            if ui.button(tr("New Script")).clicked() { PANE.with(|p| { let mut p = p.borrow_mut(); p.editing = Some((None, Script { name: tr("Script"), description: String::new(), code: format!("# {}\nbold on\n", tr("Runs on the selected cells")) })); p.error.clear(); }); }
        });
        ui.separator();
        let editing = PANE.with(|p| p.borrow().editing.clone());
        if let Some((idx, mut s)) = editing { editor(app, ui, idx, &mut s); return; }
        let gallery = PANE.with(|p| p.borrow().gallery);
        ui.horizontal(|ui| {
            if ui.selectable_label(!gallery, tr("In this workbook")).clicked() { PANE.with(|p| p.borrow_mut().gallery = false); }
            if ui.selectable_label(gallery, tr("Samples")).clicked() { PANE.with(|p| p.borrow_mut().gallery = true); }
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            if gallery {
                for s in scripts::gallery() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.strong(tr(&s.name));
                        ui.label(egui::RichText::new(tr(&s.description)).small());
                        ui.horizontal(|ui| {
                            if ui.button(tr("Run")).clicked() { let _ = run(app, &s.code); }
                            if ui.button(tr("Copy to Workbook")).clicked() { let mut l = scripts::load(&app.book); l.push(Script { name: tr(&s.name), ..s.clone() }); save_scripts(app, &l); PANE.with(|p| p.borrow_mut().gallery = false); }
                        });
                    });
                }
            } else {
                let list = scripts::load(&app.book);
                if list.is_empty() { ui.label(egui::RichText::new(tr("This workbook has no scripts. Record your actions, start a new script, or copy a sample.")).weak()); }
                let mut delete = None;
                for (i, s) in list.iter().enumerate() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.strong(&s.name);
                        if !s.description.is_empty() { ui.label(egui::RichText::new(&s.description).small()); }
                        ui.horizontal(|ui| {
                            if ui.button(tr("Run")).clicked() { let _ = run(app, &s.code); }
                            if ui.button(tr("Edit")).clicked() { PANE.with(|p| { let mut p = p.borrow_mut(); p.editing = Some((Some(i), s.clone())); p.error.clear(); }); }
                            if ui.button(tr("Delete")).clicked() { delete = Some(i); }
                        });
                    });
                }
                if let Some(i) = delete { let mut l = list.clone(); l.remove(i); save_scripts(app, &l); }
            }
        });
    });
}

fn editor(app: &mut SheetApp, ui: &mut egui::Ui, idx: Option<usize>, s: &mut Script) {
    egui::Grid::new("script-meta").num_columns(2).show(ui, |ui| {
        ui.label(tr("Name:")); ui.text_edit_singleline(&mut s.name); ui.end_row();
        ui.label(tr("Description:")); ui.text_edit_singleline(&mut s.description); ui.end_row();
    });
    let err = PANE.with(|p| p.borrow().error.clone());
    let mut close = false;
    ui.horizontal(|ui| {
        if crate::theme::primary(ui, "Run").clicked() { let r = run(app, &s.code); PANE.with(|p| p.borrow_mut().error = r.err().unwrap_or_default()); }
        if ui.button(tr("Save")).clicked() {
            match scripts::check(&s.code) {
                Ok(_) => { let mut l = scripts::load(&app.book); match idx { Some(i) if i < l.len() => l[i] = s.clone(), _ => l.push(s.clone()) } save_scripts(app, &l); close = true; }
                Err(e) => PANE.with(|p| p.borrow_mut().error = format!("{} {}: {}", tr("Line"), e.line, tr(&e.message))),
            }
        }
        if ui.button(tr("Close")).clicked() { close = true; }
    });
    if !err.is_empty() { ui.colored_label(egui::Color32::from_rgb(0xC0, 0x20, 0x20), err); }
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add(egui::TextEdit::multiline(&mut s.code).code_editor().desired_width(f32::INFINITY).desired_rows(16).id_salt("script-code"));
        ui.collapsing(tr("Commands"), |ui| { ui.label(egui::RichText::new(HELP).monospace().small()); });
    });
    PANE.with(|p| { let mut p = p.borrow_mut(); p.editing = if close { None } else { Some((idx, s.clone())) }; if close { p.error.clear(); } });
}

const HELP: &str = "select A1:B5      sheet \"Name\"\noffset 1 0        resize 3 2\nset B1 =SUM(A1:A9)\nvalue ={value}*2  ({row} {col} {n})\nbold on|off  italic  underline\nfill #FFFF00|none  color #C00000\nformat \"0.00\"  align left|center|right\nclear contents|formats|all\nupper lower proper trim\nadd 5  multiply 1.1  round 2\nsort asc|desc\nrepeat 3 … end\nmessage text      # comment";
