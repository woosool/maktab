//! The editor (views/editor.js): the open workbook, the selection, the cell being edited, undo,
//! the clipboard, and the commands the ribbon, menus and keys run; and the window's layout.

use crate::i18n::tr;
use egui::{Color32, RichText};
use qalam_kit::colors::*;
use sheet_engine::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
use sheet_engine::book::{Clip, Paste};
use sheet_engine::style::Style;
use sheet_engine::Workbook;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab { #[default] Home, Insert, PageLayout, Formulas, Data, Review, View, TableDesign, ChartDesign, ObjectFormat, PivotAnalyze, PivotDesign, Sparkline, HeaderFooter }

/// The selected cells: the range from `anchor` to `active` (the active cell), plus more ranges
/// added with ⌘-click.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection { pub active: Pos, pub anchor: Pos, pub extra: Vec<Range> }

impl Default for Selection { fn default() -> Self { Selection { active: Pos::new(0, 0), anchor: Pos::new(0, 0), extra: vec![] } } }

impl Selection {
    pub fn at(p: Pos) -> Selection { Selection { active: p, anchor: p, extra: vec![] } }
    pub fn range(&self) -> Range { Range::new(self.anchor, self.active) }
    pub fn ranges(&self) -> Vec<Range> { let mut v = self.extra.clone(); v.push(self.range()); v }
}

/// The cell being typed into (in the cell or in the formula bar).
#[derive(Clone, Debug)]
pub struct Editor {
    pub pos: Pos,
    pub sheet: usize,
    pub text: String,
    /// Started by typing over the cell (arrow keys then commit) rather than F2/double-click.
    pub typed: bool,
    /// Where the formula bar's or cell's text cursor is, in characters.
    pub cursor: usize,
    /// A reference being pointed at with the mouse or arrows: (start, end) of its text.
    pub pointing: Option<(usize, usize)>,
    pub focus_bar: bool,
    pub request_focus: bool,
}

struct Snapshot { book: Workbook, sheet: usize, sel: Selection, label: &'static str }

pub enum Dialog {
    Find { query: String, with: String, match_case: bool, whole: bool, in_formulas: bool, replace: bool, last: Option<Pos>, all_sheets: bool },
    GoTo { text: String },
    FormatCells(crate::dialogs::FormatCells),
    Size { cols: bool, value: f32 },
    Rename { sheet: usize, name: String },
    Sort { keys: Vec<(u32, bool)>, header: bool },
    Insert { delete: bool, choice: u8 },
    PasteSpecial { how: Paste, op: u8 },
    Names { name: String, refers: String, editing: Option<usize> },
    Note { pos: Pos, text: String },
    Link { pos: Pos, target: String },
    Function { query: String, picked: Option<&'static str> },
    /// Fill ▸ Series: in rows or columns; linear, growth, date or AutoFill; step and stop; date unit.
    Series { rows: bool, kind: u8, step: f64, stop: String, unit: u8, trend: bool },
    About,
    Shortcuts,
}

pub struct SheetApp {
    pub book: Workbook,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub sel: Selection,
    pub edit: Option<Editor>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    pub tab: Tab,
    pub zoom: f32,
    /// The grid's scroll position in (unzoomed) pixels past the frozen panes.
    pub scroll: egui::Vec2,
    pub clip: Option<Clip>,
    /// The last cut: pasting moves the cells instead of copying.
    pub cut: Option<(usize, Range)>,
    pub dialog: Option<Dialog>,
    pub notice: Option<(String, f64)>,
    pub error: Option<String>,
    pub dark: bool,
    pub ctx: egui::Context,
    pub prefs: crate::prefs::Prefs,
    pub options: Option<crate::prefs::Prefs>,
    pub recover: Option<Option<PathBuf>>,
    pub last_autosave: f64,
    pub pending: Option<crate::filemenu::Pending>,
    pub quitting: bool,
    /// The splash card: when it started (0 until its first frame).
    pub splash: Option<f64>,
    /// A workbook being read in the background (big files take seconds): its path and result.
    pub loading: Option<(PathBuf, std::sync::mpsc::Receiver<Result<Workbook, String>>)>,
    /// Format Painter: the copied style, and whether it stays on for several pastes.
    pub painter: Option<(Clip, bool)>,
    /// The grid's interaction state (dragging a selection, resizing, filling).
    pub grid: crate::grid::GridState,
    /// Scroll so the active cell is visible on the next frame.
    pub reveal: bool,
    /// The Print and Page Setup dialogs and the grid's page-break lines.
    pub printing: crate::print::PrintUi,
    /// Where the grid was drawn last frame.
    pub grid_rect: egui::Rect,
    /// Review and View tab state (review_tab.rs, view_tab.rs).
    pub rv: crate::review_tab::ReviewState,
}

#[cfg(test)]
thread_local! { pub static TAKE_SENT_FILES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

impl SheetApp {
    pub fn new(cc: &eframe::CreationContext, path: Option<PathBuf>) -> SheetApp {
        qalam_kit::fonts::install_fonts(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx);
        let mut app = SheetApp::blank(cc.egui_ctx.clone());
        app.apply_prefs(crate::prefs::Prefs::load());
        app.splash = (app.prefs.splash && !cfg!(test)).then_some(0.0);
        // A file macOS sent before the window existed opens like one on the command line.
        qalam_kit::open_files::set_context(&cc.egui_ctx);
        let (sent, others) = qalam_kit::open_files::startup_paths(Vec::new());
        let mut others = others;
        let path = match (path, sent) { (Some(p), Some(s)) => { others.insert(0, s); Some(p) } (p, s) => p.or(s) };
        if !cfg!(test) { qalam_kit::open_files::open_elsewhere(&others); }
        if let Some(p) = path { app.open_path(p); }
        app.offer_recovery();
        if app.splash.is_none() { app.auto_check_updates(&cc.egui_ctx); }
        app
    }

    pub fn blank(ctx: egui::Context) -> SheetApp {
        SheetApp {
            book: Workbook::new(), path: None, dirty: false, sel: Selection::default(), edit: None, undo: vec![], redo: vec![], tab: Tab::Home, zoom: 1.0,
            scroll: egui::Vec2::ZERO, clip: None, cut: None, dialog: None, notice: None, error: None, dark: false, ctx, prefs: Default::default(), options: None,
            recover: None, last_autosave: 0.0, pending: None, quitting: false, splash: None, loading: None, painter: None, grid: Default::default(), reveal: false, grid_rect: egui::Rect::NOTHING,
            printing: Default::default(), rv: Default::default(),
        }
    }

    pub fn sheet(&self) -> usize { self.book.active }

    // ---- history ----------------------------------------------------------------------------

    /// Remembers the workbook before a change (package parts are shared, not copied).
    pub fn checkpoint(&mut self, label: &'static str) {
        let parts = std::mem::take(&mut self.book.parts);
        let snap = Snapshot { book: self.book.clone(), sheet: self.book.active, sel: self.sel.clone(), label };
        self.book.parts = parts;
        self.undo.push(snap);
        if self.undo.len() > 100 { self.undo.remove(0); }
        self.redo.clear();
    }
    /// After a change: recalculate and mark unsaved.
    pub fn changed(&mut self) {
        if !self.book.manual { self.book.recalc(); } else { self.book.cf_dirty(); }
        self.dirty = true;
        crate::insert_extras::after_change(self);
    }
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }
    pub fn undo_label(&self) -> Option<&'static str> { self.undo.last().map(|s| s.label) }
    pub fn undo(&mut self) { self.edit = None; if let Some(s) = self.undo.pop() { let cur = self.swap(s); self.redo.push(cur); } }
    pub fn redo(&mut self) { self.edit = None; if let Some(s) = self.redo.pop() { let cur = self.swap(s); self.undo.push(cur); } }
    fn swap(&mut self, s: Snapshot) -> Snapshot {
        let parts = std::mem::take(&mut self.book.parts);
        let cur = Snapshot { book: std::mem::replace(&mut self.book, s.book), sheet: self.book.active, sel: std::mem::replace(&mut self.sel, s.sel), label: s.label };
        self.book.parts = parts;
        self.book.active = s.sheet.min(self.book.sheets.len() - 1);
        self.dirty = true;
        self.reveal = true;
        cur
    }

    // ---- files --------------------------------------------------------------------------------

    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Book1"));
        format!("{}{} - {}", name, if self.dirty { " •" } else { "" }, crate::app_name())
    }
    pub fn load(&mut self, book: Workbook, path: Option<PathBuf>) {
        self.book = book;
        self.path = path;
        self.dirty = false;
        self.undo.clear(); self.redo.clear();
        self.edit = None;
        let s = &self.book.sheets[self.book.active];
        self.sel = s.selection.map(|r| Selection { active: r.start(), anchor: r.start(), extra: vec![] }).unwrap_or_default();
        self.zoom = s.zoom as f32 / 100.0;
        self.scroll = egui::Vec2::ZERO;
        self.reveal = true;
    }
    pub fn new_book(&mut self) { self.load(Workbook::new(), None); }
    /// Opens a workbook. Reading happens on another thread (a large file takes seconds) while
    /// the splash card or an "Opening…" note shows; `finish_open` takes the result.
    pub fn open_path(&mut self, p: PathBuf) {
        if cfg!(test) { let r = sheet_engine::open(&p); self.finish_open(p, r); return; }
        let (tx, rx) = std::sync::mpsc::channel();
        let (path, ctx) = (p.clone(), self.ctx.clone());
        std::thread::spawn(move || { let _ = tx.send(sheet_engine::open(&path)); ctx.request_repaint(); });
        self.loading = Some((p, rx));
    }
    /// Takes the background read's result when it's ready; true while still reading.
    pub fn poll_loading(&mut self) -> bool {
        let Some((p, rx)) = &self.loading else { return false };
        match rx.try_recv() {
            Ok(r) => { let p = p.clone(); self.loading = None; self.finish_open(p, r); false }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => { self.loading = None; false }
        }
    }
    pub fn finish_open(&mut self, p: PathBuf, r: Result<Workbook, String>) {
        match r {
            Ok(b) => { self.prefs.add_recent(&p); let csv = p.extension().is_some_and(|e| !e.eq_ignore_ascii_case("xlsx") && !e.eq_ignore_ascii_case("xlsm")); self.load(b, (!csv).then_some(p.clone())); if csv { self.path = Some(p); } }
            Err(e) => self.error = Some(format!("{}\n{}", tr("The file couldn't be opened."), e)),
        }
    }
    /// Opens files the system sent while running (Finder, Open With): the first here (asking
    /// to save changes first), any others in new windows.
    pub fn open_sent_files(&mut self) {
        // The queue is shared by the whole process; tests opt in per thread.
        #[cfg(test)]
        if !TAKE_SENT_FILES.with(|t| t.get()) { return; }
        if self.pending.is_some() || self.loading.is_some() || !qalam_kit::open_files::pending() { return; }
        let (here, rest) = qalam_kit::open_files::route(qalam_kit::open_files::take());
        if !cfg!(test) { qalam_kit::open_files::open_elsewhere(&rest); }
        if let Some(p) = here { self.guarded(crate::filemenu::Pending::OpenPath(p)); }
    }
    pub fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter(tr("Spreadsheets"), &["xlsx", "xlsm", "xltx", "ods", "csv", "tsv", "txt"]).pick_file() { self.open_path(p); }
    }
    /// Saves (as .xlsx unless the file is CSV); `as_new` always asks where.
    pub fn save(&mut self, as_new: bool) -> bool {
        self.commit(None);
        let path = match (&self.path, as_new) {
            (Some(p), false) => p.clone(),
            _ => {
                let name = self.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Book1"));
                match rfd::FileDialog::new().set_file_name(format!("{name}.xlsx")).add_filter(tr("Excel Workbook"), &["xlsx"]).add_filter(tr("OpenDocument Spreadsheet"), &["ods"]).add_filter("CSV", &["csv"]).save_file() { Some(p) => p, None => return false }
            }
        };
        self.save_to(&path)
    }
    pub fn save_to(&mut self, path: &std::path::Path) -> bool {
        // The view state travels with the file.
        let a = self.book.active;
        self.book.sheets[a].selection = Some(self.sel.range());
        self.book.sheets[a].zoom = (self.zoom * 100.0).round() as u32;
        match sheet_engine::save(&self.book, path) {
            Ok(()) => { self.path = Some(path.to_path_buf()); self.dirty = false; self.prefs.add_recent(path); self.clear_recovery(); self.notify(&tr("Saved")); true }
            Err(e) => { self.error = Some(format!("{}\n{}", tr("The file couldn't be saved."), e)); false }
        }
    }
    pub fn notify(&mut self, text: &str) { self.notice = Some((text.to_string(), self.ctx.input(|i| i.time))); }

    // ---- selection ----------------------------------------------------------------------------

    pub fn select(&mut self, p: Pos, extend: bool) {
        let p = Pos::new(p.row.min(MAX_ROWS - 1), p.col.min(MAX_COLS - 1));
        if extend { self.sel.active = p; } else { self.sel = Selection::at(p); }
        self.reveal = true;
    }
    /// Moves the active cell by (dr, dc), skipping hidden rows and columns and leaving merges whole.
    pub fn step(&mut self, dr: i64, dc: i64, extend: bool) {
        let s = &self.book.sheets[self.sheet()];
        let mut p = self.sel.active;
        // From inside a merged area, step off its far edge.
        if let Some(m) = s.merge_at(p) { if dr > 0 { p.row = m.r1; } if dc > 0 { p.col = m.c1; } if dr < 0 { p.row = m.r0; } if dc < 0 { p.col = m.c0; } }
        let mut r = p.row as i64; let mut c = p.col as i64;
        for _ in 0..dr.unsigned_abs() { loop { r += dr.signum(); if r < 0 || r >= MAX_ROWS as i64 || !s.hidden_rows.contains(&(r as u32)) { break; } } }
        for _ in 0..dc.unsigned_abs() { loop { c += dc.signum(); if c < 0 || c >= MAX_COLS as i64 || !s.hidden_cols.contains(&(c as u32)) { break; } } }
        let mut q = Pos::new(r.clamp(0, MAX_ROWS as i64 - 1) as u32, c.clamp(0, MAX_COLS as i64 - 1) as u32);
        if !extend { if let Some(m) = s.merge_at(q) { q = m.start(); } }
        self.select(q, extend);
    }
    /// ⌘-arrow: to the edge of the current block of data (or the next one).
    pub fn jump(&mut self, dr: i64, dc: i64, extend: bool) {
        let s = &self.book.sheets[self.sheet()];
        let filled = |p: Pos| !s.value(p).is_empty();
        let e = s.extent().unwrap_or_default();
        let mut p = self.sel.active;
        let next = |p: Pos| -> Option<Pos> {
            let r = p.row as i64 + dr; let c = p.col as i64 + dc;
            (r >= 0 && c >= 0 && r < MAX_ROWS as i64 && c < MAX_COLS as i64).then(|| Pos::new(r as u32, c as u32))
        };
        let Some(first) = next(p) else { return };
        if filled(p) && filled(first) {
            while let Some(n) = next(p) { if !filled(n) { break; } p = n; }
        } else {
            p = first;
            while !filled(p) {
                let beyond = (dr > 0 && p.row > e.row) || (dc > 0 && p.col > e.col);
                match next(p) { Some(n) if !beyond => p = n, _ => { p = Pos::new(if dr > 0 { MAX_ROWS - 1 } else if dr < 0 { 0 } else { p.row }, if dc > 0 { MAX_COLS - 1 } else if dc < 0 { 0 } else { p.col }); break; } }
            }
        }
        self.select(p, extend);
    }
    pub fn select_all(&mut self) { self.sel = Selection { anchor: Pos::new(0, 0), active: Pos::new(MAX_ROWS - 1, MAX_COLS - 1), extra: vec![] }; self.sel.active = Pos::new(MAX_ROWS - 1, MAX_COLS - 1); }

    // ---- editing ------------------------------------------------------------------------------

    /// Starts editing the active cell, with its text (F2, double-click) or replacing it (typing).
    pub fn begin_edit(&mut self, replace: Option<String>, focus_bar: bool) {
        let p = self.sel.active;
        let sheet = self.sheet();
        if !crate::review_tab::may_edit(self, sheet, p) { return; }
        let typed = replace.is_some();
        let text = replace.unwrap_or_else(|| self.book.edit_text(sheet, p));
        let cursor = text.chars().count();
        self.edit = Some(Editor { pos: p, sheet, text, typed, cursor, pointing: None, focus_bar, request_focus: true });
    }
    /// Enters what was typed and moves the selection (None stays).
    pub fn commit(&mut self, mv: Option<(i64, i64)>) {
        let Some(e) = self.edit.take() else { return };
        let old = self.book.edit_text(e.sheet, e.pos);
        let mut text = crate::review_tab::finish_entry(self, &e);
        // Close unbalanced parentheses as Excel offers to.
        if text.starts_with('=') { let open = text.matches('(').count().saturating_sub(text.matches(')').count()); text.push_str(&")".repeat(open)); }
        if text != old {
            if text.starts_with('=') && text.len() > 1 {
                if let Err(err) = sheet_engine::formula::parser::parse(&text[1..]) {
                    // Keep editing so the mistake can be fixed.
                    self.error = Some(format!("{}\n{}", tr("There's a problem with this formula."), tr(&err.message)));
                    self.edit = Some(e);
                    return;
                }
            }
            if crate::validation_ui::check_entry(self, &e, &text, mv) { return; }
            self.checkpoint("Typing");
            self.book.set_input(e.sheet, e.pos, &text);
            self.book.table_after_input(e.sheet, e.pos);
            // A long entry widens nothing, but a typed line break turns on wrapping.
            if text.contains('\n') { self.book.apply_style(e.sheet, &[Range::cell(e.pos)], &|s| s.wrap = true); }
            self.changed();
        }
        self.book.active = e.sheet;
        if let Some((dr, dc)) = mv {
            // Enter inside a multi-cell selection moves within it.
            let r = self.sel.range();
            if !r.is_cell() && self.sel.extra.is_empty() {
                let mut p = self.sel.active;
                if dr != 0 { p.row = if (p.row as i64 + dr) > r.r1 as i64 { p.col = if p.col >= r.c1 { r.c0 } else { p.col + 1 }; r.r0 } else if (p.row as i64 + dr) < r.r0 as i64 { r.r1 } else { (p.row as i64 + dr) as u32 }; }
                if dc != 0 { p.col = if (p.col as i64 + dc) > r.c1 as i64 { p.row = if p.row >= r.r1 { r.r0 } else { p.row + 1 }; r.c0 } else if (p.col as i64 + dc) < r.c0 as i64 { r.c1 } else { (p.col as i64 + dc) as u32 }; }
                self.sel.active = p;
                self.reveal = true;
            } else { self.step(dr, dc, false); }
        }
    }
    pub fn cancel_edit(&mut self) { self.edit = None; }
    /// Enters the same text into every selected cell (⌘Enter).
    pub fn commit_all(&mut self) {
        let Some(e) = self.edit.take() else { return };
        self.checkpoint("Typing");
        let origin = e.pos;
        for r in self.sel.ranges() {
            for p in r.iter() {
                let t = if e.text.starts_with('=') { format!("={}", sheet_engine::formula::adjust::shift(&e.text[1..], p.row as i64 - origin.row as i64, p.col as i64 - origin.col as i64)) } else { e.text.clone() };
                self.book.set_input(e.sheet, p, &t);
            }
        }
        self.changed();
    }

    // ---- commands -----------------------------------------------------------------------------

    pub fn style(&self) -> Style { self.book.style_at(self.sheet(), self.sel.active).clone() }
    pub fn apply_style(&mut self, label: &'static str, f: impl Fn(&mut Style)) {
        if crate::page_view::hf_style(self, &f) { return; }
        if crate::review_tab::blocked(self, sheet_engine::review::Action::FormatCells) { return; }
        self.commit(None);
        self.checkpoint(label);
        let s = self.sheet();
        self.book.apply_style(s, &self.sel.ranges(), &f);
        self.dirty = true;
    }
    pub fn clear(&mut self, contents: bool, formats: bool, all: bool) {
        if crate::review_tab::locked_selection(self) { return; }
        self.checkpoint("Clear");
        let s = self.sheet();
        for r in self.sel.ranges() { self.book.clear(s, r, contents, formats, all); }
        self.changed();
    }
    pub fn copy(&mut self) {
        self.commit(None);
        let r = self.sel.range();
        let clip = self.book.copy(self.sheet(), r);
        if let Ok(mut c) = arboard::Clipboard::new() { let _ = c.set_text(clip.text.clone()); }
        self.clip = Some(clip);
        crate::objects::clear_clip(&self.ctx);
        self.cut = None;
        self.grid.marching = Some((self.sheet(), r));
    }
    pub fn cut(&mut self) { self.copy(); self.cut = Some((self.sheet(), self.sel.range())); }
    pub fn paste_with(&mut self, how: Paste) {
        if crate::review_tab::locked_selection(self) { return; }
        self.commit(None);
        if crate::objects::paste_object(self, false) { return; }
        let system = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok());
        let s = self.sheet();
        let at = self.sel.range();
        match (&self.clip, system) {
            (Some(clip), sys) if sys.as_deref().is_none_or(|t| t == clip.text) => {
                let clip = clip.clone();
                self.checkpoint("Paste");
                if let Some((from_sheet, src)) = self.cut.take() {
                    self.book.move_range(from_sheet, src, s, at.start());
                    self.sel = Selection { anchor: at.start(), active: Pos::new(at.r0 + src.rows() - 1, at.c0 + src.cols() - 1), extra: vec![] };
                    self.clip = None;
                    self.grid.marching = None;
                } else {
                    let dest = self.book.paste(s, at, &clip, how);
                    // Objects over the copied cells come along (move and size with cells).
                    if let Some((fs, src)) = clip.source.filter(|(fs, _)| *fs < self.book.sheets.len()) {
                        if matches!(how.what, sheet_engine::book::PasteWhat::All | sheet_engine::book::PasteWhat::NoBorders) && !how.transpose { sheet_engine::follow::copy_objects_with_cells(&mut self.book, fs, src, s, dest.start()); }
                    }
                    self.sel = Selection { anchor: dest.start(), active: dest.end(), extra: vec![] };
                }
                self.changed();
            }
            (_, Some(text)) if !text.is_empty() => {
                self.checkpoint("Paste");
                let dest = self.book.paste_text(s, at.start(), &text);
                self.sel = Selection { anchor: dest.start(), active: dest.end(), extra: vec![] };
                self.changed();
            }
            _ => {}
        }
    }
    pub fn paste(&mut self) { self.paste_with(Paste::default()); }

    pub fn insert_rows_cols(&mut self, rows: bool, delete: bool) {
        { use sheet_engine::review::Action::*; if crate::review_tab::blocked(self, match (rows, delete) { (true, false) => InsertRows, (true, true) => DeleteRows, (false, false) => InsertColumns, _ => DeleteColumns }) { return; } }
        self.commit(None);
        let r = self.sel.range();
        let (at, n) = if rows { (r.r0, r.rows() as i64) } else { (r.c0, r.cols() as i64) };
        self.checkpoint(if delete { "Delete" } else { "Insert" });
        let s = self.sheet();
        self.book.insert(s, rows, at, if delete { -n } else { n });
        self.changed();
    }
    /// Shifts cells (not whole rows or columns) right/down to insert, or left/up to delete.
    pub fn shift_cells(&mut self, down: bool, delete: bool) {
        if crate::review_tab::locked_selection(self) { return; }
        if let Some(m) = self.book.shift_cells_blocked(self.sheet(), self.sel.range(), down) { self.notify(m); return; }
        self.checkpoint(if delete { "Delete" } else { "Insert" });
        let s = self.sheet();
        self.book.shift_cells(s, self.sel.range(), down, delete);
        self.changed();
    }
    pub fn merge(&mut self, kind: u8) {
        if crate::review_tab::blocked(self, sheet_engine::review::Action::FormatCells) { return; }
        self.checkpoint("Merge");
        let s = self.sheet();
        for r in self.sel.ranges() {
            match kind {
                0 => { self.book.merge(s, r, false); self.book.apply_style(s, &[r], &|st| st.h = sheet_engine::style::HAlign::Center); }
                1 => self.book.merge(s, r, true),
                2 => self.book.merge(s, r, false),
                _ => self.book.unmerge(s, r),
            }
        }
        self.changed();
    }
    pub fn sort(&mut self, asc: bool) {
        if crate::review_tab::blocked(self, sheet_engine::review::Action::Sort) { return; }
        let s = self.sheet();
        let mut r = self.sel.range();
        if r.is_cell() { r = self.book.current_region(s, r.start()); }
        let header = r.rows() > 1 && self.book.looks_like_header(s, r);
        self.checkpoint("Sort");
        self.book.sort(s, r, &[(self.sel.active.col, asc)], header, false);
        self.changed();
    }
    /// AutoSum: =SUM over the numbers above (or to the left), or one total per selected column.
    pub fn autosum(&mut self, func: &str) {
        self.commit(None);
        let s = self.sheet();
        let r = self.sel.range();
        let sheet = &self.book.sheets[s];
        let is_num = |p: Pos| matches!(sheet.value(p), sheet_engine::Value::Num(_));
        if r.is_cell() {
            let p = r.start();
            let mut top = p.row;
            while top > 0 && is_num(Pos::new(top - 1, p.col)) { top -= 1; }
            let arg = if top < p.row { Range { r0: top, c0: p.col, r1: p.row - 1, c1: p.col }.a1() } else {
                let mut left = p.col;
                while left > 0 && is_num(Pos::new(p.row, left - 1)) { left -= 1; }
                if left < p.col { Range { r0: p.row, c0: left, r1: p.row, c1: p.col - 1 }.a1() } else { String::new() }
            };
            let text = format!("={func}({arg})");
            let cursor = text.chars().count() - 1;
            self.edit = Some(Editor { pos: p, sheet: s, text, typed: false, cursor, pointing: None, focus_bar: false, request_focus: true });
            return;
        }
        // Totals under each column of the selection.
        self.checkpoint("AutoSum");
        let row = if (r.c0..=r.c1).all(|c| self.book.sheets[s].value(Pos::new(r.r1, c)).is_empty()) && r.rows() > 1 { r.r1 } else { r.r1 + 1 };
        for c in r.c0..=r.c1 {
            let src = Range { r0: r.r0, c0: c, r1: row - 1, c1: c };
            self.book.set_input(s, Pos::new(row, c), &format!("={func}({})", src.a1()));
        }
        self.changed();
    }
    pub fn freeze(&mut self, kind: u8) {
        self.checkpoint("Freeze Panes");
        let s = self.sheet();
        let sh = &mut self.book.sheets[s];
        // The panes start at what's on screen now: rows and columns scrolled past stay out of
        // view, so freezing far down the sheet still leaves room to scroll.
        let (top, left) = (crate::grid::Axis::of_rows(sh), crate::grid::Axis::of_cols(sh));
        let first = sheet_engine::Pos::new(top.at(top.pos(sh.frozen_rows) + self.scroll.y), left.at(left.pos(sh.frozen_cols) + self.scroll.x));
        let (r, c, origin) = match kind {
            0 if sh.frozen_rows > 0 || sh.frozen_cols > 0 => (0, 0, sheet_engine::Pos::default()),
            // The top-left of the selection (a whole row selected has its active cell at the far right).
            0 => { let a = self.sel.range().start(); let o = sheet_engine::Pos::new(if a.row > first.row { first.row } else { 0 }, if a.col > first.col { first.col } else { 0 }); (if a.row > o.row { a.row } else { 0 }, if a.col > o.col { a.col } else { 0 }, o) }
            // The top row (or first column) on screen: with panes already frozen that's where the
            // frozen part starts, not the first row of the scrolling part.
            1 => { let t = if sh.frozen_rows > 0 { sh.frozen_origin.row } else { first.row }; (t + 1, 0, sheet_engine::Pos::new(t, 0)) }
            _ => { let l = if sh.frozen_cols > 0 { sh.frozen_origin.col } else { first.col }; (0, l + 1, sheet_engine::Pos::new(0, l)) }
        };
        sh.frozen_rows = r; sh.frozen_cols = c; sh.frozen_origin = origin;
        self.scroll = egui::Vec2::ZERO;
        self.dirty = true;
    }
    pub fn set_sheet(&mut self, i: usize) {
        self.commit(None);
        let a = self.book.active;
        self.book.sheets[a].selection = Some(self.sel.range());
        self.book.active = i;
        self.sel = self.book.sheets[i].selection.map(|r| Selection { active: r.start(), anchor: r.start(), extra: vec![] }).unwrap_or_default();
        self.scroll = egui::Vec2::ZERO;
        self.reveal = true;
    }
    pub fn fill_dir(&mut self, dir: u8) {
        let r = self.sel.range();
        let s = self.sheet();
        self.checkpoint("Fill");
        let (src, tgt) = match dir {
            0 if r.rows() > 1 => (Range { r1: r.r0, ..r }, r),
            1 if r.cols() > 1 => (Range { c1: r.c0, ..r }, r),
            2 if r.rows() > 1 => (Range { r0: r.r1, ..r }, r),
            3 if r.cols() > 1 => (Range { c0: r.c1, ..r }, r),
            0 => { if r.r0 == 0 { return; } (Range { r0: r.r0 - 1, r1: r.r0 - 1, ..r }, Range { r0: r.r0 - 1, ..r }) }
            1 => { if r.c0 == 0 { return; } (Range { c0: r.c0 - 1, c1: r.c0 - 1, ..r }, Range { c0: r.c0 - 1, ..r }) }
            2 => { (Range { r0: r.r1 + 1, r1: r.r1 + 1, ..r }, Range { r1: r.r1 + 1, ..r }) }
            _ => { (Range { c0: r.c1 + 1, c1: r.c1 + 1, ..r }, Range { c1: r.c1 + 1, ..r }) }
        };
        self.book.fill(s, src, tgt, false);
        self.changed();
    }

    // ---- the window ---------------------------------------------------------------------------

    pub fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let loading = self.poll_loading();
        if self.splash.is_some() { crate::theme::apply_mode(&ctx, self.dark); if crate::filemenu::splash(self, ui) { return; } }
        self.autosave();
        crate::theme::apply_mode(&ctx, self.dark);
        qalam_kit::colors::DARK.with(|d| d.set(self.dark));
        // Only when it changes: a viewport command asks for another frame, so sending it every
        // frame kept the window redrawing nonstop.
        let title = self.title();
        let id = egui::Id::new("window-title");
        if ctx.data(|d| d.get_temp::<String>(id)).as_deref() != Some(title.as_str()) { ctx.data_mut(|d| d.insert_temp(id, title.clone())); ctx.send_viewport_cmd(egui::ViewportCommand::Title(title)); }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| Some(f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty())).collect());
        if let Some(p) = dropped.into_iter().next() { self.guarded(crate::filemenu::Pending::OpenPath(p)); }
        self.open_sent_files();
        if !cfg!(test) && self.loading.is_none() { qalam_kit::default_apps::first_run_prompt(&ctx, &qalam_kit::default_apps::JADWAL, &crate::config_dir().join("default-apps-asked")); }
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(crate::filemenu::Pending::Quit);
        }
        crate::view_tab::keys(self, &ctx);
        crate::review_tab::frame_keys(self, &ctx);
        crate::formulas_tab::keys(self, &ctx);
        crate::refedit::frame(self, &ctx);
        crate::grid::shortcuts(self, &ctx);
        crate::ribbon::ribbon(self, ui);
        if self.prefs.show_formula_bar { crate::formulabar::bar(self, ui); }
        self.status_bar(ui);
        crate::tabs::tabs(self, ui);
        crate::objects::frame(self, ui);
        crate::insert_extras::panels(self, ui);
        crate::review_tab::panes(self, ui);
        crate::data_tab::panels(self, ui);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(c_card())).show(ui, |ui| crate::view_tab::central(self, ui));
        crate::dialogs::show(self, &ctx);
        crate::formulas_tab::show(self, &ctx);
        crate::condformat_ui::show(self, &ctx);
        crate::validation_ui::show(self, &ctx);
        crate::filter_ui::show(self, &ctx);
        crate::data_tab::show(self, &ctx);
        crate::pq_editor::show(self, &ctx);
        crate::filemenu::dialogs(self, &ctx);
        crate::print::dialogs(self, &ctx);
        crate::insert_extras::frame(self, &ctx);
        crate::sparkline_ui::show(self, &ctx);
        crate::prefs::recovery_dialog(self, &ctx);
        crate::prefs::options_dialog(self, &ctx);
        crate::review_tab::frame(self, &ctx);
        crate::view_tab::frame(self, &ctx);
        crate::backstage::dialogs(self, &ctx);
        crate::update::dialog(self, &ctx);
        if loading { crate::filemenu::loading_note(self, &ctx); }
        if let Some(e) = self.error.clone() {
            let mut open = true;
            crate::theme::dialog(&ctx, crate::app_name()).open(&mut open).collapsible(false).resizable(false).show(&ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(e);
                crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { self.error = None; });
            });
            if !open { self.error = None; }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin::symmetric(10, 3))).show(ui, |ui| {
            ui.with_layout(qalam_kit::widgets::row_layout(), |ui| {
                let mode = match &self.edit { Some(e) if e.typed => "Enter", Some(_) => "Edit", None => if self.grid.marching.is_some() { "Select destination and press Enter or choose Paste" } else { "Ready" } };
                ui.label(RichText::new(tr(mode)).size(12.0).color(c_ink2()));
                if let Some((t, at)) = &self.notice {
                    if ui.input(|i| i.time) - at < 3.0 { ui.label(RichText::new(t).size(12.0).color(ACCENT)); } else { self.notice = None; }
                }
                ui.with_layout(qalam_kit::widgets::far_layout(), |ui| {
                    crate::view_tab::status_items(self, ui);
                });
            });
        });
    }
}

impl eframe::App for SheetApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) { self.frame(ui); }
    /// Clear to transparent while the splash card shows (its window has no background).
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] { if self.splash.is_some() { [0.0; 4] } else { visuals.panel_fill.to_normalized_gamma_f32() } }
}

pub fn rgb(c: [u8; 3]) -> Color32 { Color32::from_rgb(c[0], c[1], c[2]) }
