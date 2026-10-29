//! Application state, file commands and keyboard shortcuts (editor.js).

use std::collections::HashMap;
use std::path::PathBuf;
use word_engine::{docx, import, layout_document, Document, Layout, Paragraph};

/// Dialogs that can be open at once.
#[derive(Default)]
pub struct Dialogs {
    pub page_setup: Option<word_engine::PageSettings>,
    pub paragraph: Option<ParagraphDraft>,
    pub font: Option<FontDraft>,
    pub table_props: Option<TableDraft>,
    pub list: Option<ListDraft>,
    pub link: Option<String>,
    pub table: Option<(usize, usize)>,
    pub about: bool,
    pub shortcuts: bool,
    pub error: Option<String>,
    /// A pending action waiting for the user to save/discard changes.
    pub confirm: Option<Pending>,
}

#[derive(Clone)]
pub struct ParagraphDraft {
    pub left: f32, pub right: f32, pub first: f32, pub before: f32, pub after: f32, pub rule: word_engine::LineRule, pub spacing: f32,
    pub alignment: word_engine::Alignment, pub rtl: bool, pub outline: u8, pub contextual: bool,
    pub widow: bool, pub keep_next: bool, pub keep_lines: bool, pub page_break_before: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListDialogKind { Number, Bullet, Multilevel }

/// Define New Number Format / Bullet / Multilevel List: the list being defined and the level shown.
#[derive(Clone)]
pub struct ListDraft { pub kind: ListDialogKind, pub list: word_engine::editing::CustomList, pub level: usize, pub save: bool }

/// The Font dialog's settings (font-dialog.js).
#[derive(Clone)]
pub struct FontDraft {
    pub font: String, pub bold: bool, pub italic: bool, pub size: f32, pub color: Option<word_engine::Color>,
    pub underline: bool, pub strike: bool, pub double_strike: bool, pub superscript: bool, pub subscript: bool,
    pub small_caps: bool, pub all_caps: bool, pub hidden: bool, pub outline: bool, pub shadow: bool, pub spacing: f32,
}

/// The Table Properties dialog's settings (table-properties).
#[derive(Clone)]
pub struct TableDraft {
    pub table: usize, pub alignment: word_engine::Alignment, pub indent: f32, pub width: f32,
    pub row_height: f32, pub exact: bool, pub header: bool, pub cant_split: bool,
    pub v_align: word_engine::model::CellVAlign, pub margins: [f32; 4],
}

#[derive(Clone, Debug)]
pub enum Pending { New, Open, Quit, OpenPath(PathBuf) }

#[derive(Default)]
pub struct FindState {
    pub open: bool,
    pub query: String,
    pub replace: String,
    pub match_case: bool,
    pub whole_words: bool,
    pub focus: bool,
    pub matches: Vec<(usize, usize)>,
    pub current: Option<usize>,
    pub revision: u64,
}

pub struct WordApp {
    pub doc: Document,
    pub layout: Layout,
    pub measure_cache: word_engine::MeasureCache,
    pub path: Option<PathBuf>,
    pub zoom: f32,
    pub dialogs: Dialogs,
    pub find: FindState,
    pub show_outline: bool,
    pub show_marks: bool,
    /// Rich copy of the last in-app copy, with its plain text to recognize it on paste.
    pub clipboard: Option<(String, Vec<Paragraph>)>,
    pub goal_x: Option<f32>,
    pub scroll_to_caret: bool,
    pub caret_blink: f64,
    pub preedit: String,
    pub textures: HashMap<String, Option<egui::TextureHandle>>,
    pub status: Option<(String, f64)>,
    pub drag: canvas_state::Drag,
    pub allow_close: bool,
    pub canvas_focus: bool,
    pub author: String,
    pub show_comments: bool,
    /// Comment being written or edited: (id, draft, is_new).
    pub comment_edit: Option<(String, String, bool)>,
    /// Reply being written: (thread root id, draft).
    pub reply: Option<(String, String)>,
    pub show_notes: bool,
    /// Note being edited in the notes pane: (kind, id, draft).
    pub note_edit: Option<(word_engine::NoteKind, String, String)>,
    pub tab: crate::ribbon::Tab,
    /// Copied formatting waiting to be applied by the next selection (Format Painter).
    pub format_painter: Option<word_engine::RunProps>,
    pub last_color: word_engine::Color,
    pub last_highlight: String,
    /// Fit the page width to the window on the next frame.
    pub fit_width: bool,
    /// Last primary press (time, screen position, click count) for double/triple clicks.
    pub last_click: (f64, egui::Pos2, u8),
    /// Last painted canvas mapping: page origin x, canvas top y, page tops (px), zoom.
    pub canvas_map: (f32, f32, Vec<f32>, f32),
    pub show_gridlines: bool,
    pub atlas: crate::glyphs::GlyphAtlas,
    /// The egui context, for background jobs started outside a frame's UI code.
    pub ctx: egui::Context,
    /// The splash screen: when it started, and a file to open once fonts are ready.
    pub splash: Option<(f64, Option<PathBuf>)>,
    pub show_ruler: bool,
    pub insert: crate::insert::InsertState,
    /// Named lists defined by the user, saved between sessions.
    pub custom_lists: Vec<word_engine::editing::CustomList>,
    pub ruler_tab_kind: word_engine::TabKind,
    /// Focus mode hides the ribbon and status bar.
    pub focus_mode: bool,
    /// Read Mode: pages side by side, read-only, turned with the keyboard.
    pub read_mode: bool,
    pub read_row: usize,
    pub read_scroll: bool,
    /// The horizontal ruler's screen rectangle (last frame).
    pub ruler_rect: egui::Rect,
    /// Draw Table pen or eraser, while active.
    pub table_tool: Option<TableTool>,
    pub border_pen: word_engine::Border,
    /// Formula dialog: (formula, number format, open).
    pub formula: Option<(String, String)>,
    pub extras: crate::extras::Extras,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableTool { Pen, Eraser }

/// The current UTC time as an OOXML date (yyyy-mm-ddThh:mm:ssZ).
/// Local date and time for DATE/TIME fields.
pub fn local_iso() -> String { chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string() }

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

pub mod canvas_state {
    /// What a mouse drag selects by (Word extends by the unit of the click that started it).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum Unit { #[default] Char, Word, Paragraph, Line }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub enum Drag {
        #[default]
        None,
        /// Selecting by `unit`; `anchor` is the range the first click selected.
        Select { unit: Unit, anchor: (usize, usize) },
        /// Pressed inside the selection: a drag moves the text, a plain click places the caret.
        Pending { s: usize, e: usize, at: usize },
        /// Dragging selected text to `target`.
        Move { s: usize, e: usize, target: Option<usize> },
        /// Resizing the picture or object at `pos` (w×h when pressed) by one of its eight handles.
        Resize { pos: usize, handle: usize, w: f32, h: f32, from: egui::Pos2 },
        /// Dragging the floating object at `pos` whose box was at (x, y) on `page` when pressed.
        Float { pos: usize, from: egui::Pos2, page: usize, x: f32, y: f32 },
        /// Rotating the shape at `pos` about its screen center; `base` is its rotation when pressed.
        Rotate { pos: usize, center: (f32, f32), base: f32, from: egui::Pos2 },
        /// Dragging a table column's right border; `x` is the border's layout x when pressed.
        Column { table: usize, col: usize, x: f32 },
        /// Dragging a table's move handle; `target` is the drop position.
        TableMove { table: usize, target: Option<usize> },
        /// Dragging a table's resize handle; `width` is its width in px when pressed.
        TableResize { table: usize, x: f32, width: f32 },
        /// Drawing a table stroke from (page, x, y).
        Stroke { page: usize, x: f32, y: f32 },
    }
}

/// Where custom lists are kept: the platform's config folder.
fn custom_lists_path() -> Option<PathBuf> { Some(crate::config_dir().join("lists.txt")) }

fn load_custom_lists() -> Vec<word_engine::editing::CustomList> {
    custom_lists_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| t.lines().filter_map(word_engine::editing::CustomList::from_line).collect()).unwrap_or_default()
}

pub use qalam_kit::fonts::{face_name, install_fonts};

#[cfg(test)]
thread_local! { pub static TAKE_SENT_FILES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

impl WordApp {
    pub fn new(cc: &eframe::CreationContext, path: Option<PathBuf>) -> WordApp {
        install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::Theme::Light);
        crate::theme::apply(&cc.egui_ctx);
        cc.egui_ctx.all_styles_mut(|s| {
            s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
            s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
            s.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
            s.text_styles.insert(egui::TextStyle::Heading, egui::FontId::new(16.0, egui::FontFamily::Name("ui-bold".into())));
            s.spacing.button_padding = egui::vec2(8.0, 3.0);
            s.spacing.interact_size.y = 24.0;
            s.spacing.menu_margin = egui::Margin::same(6);
        });
        let doc = Self::welcome();
        // Nothing is laid out until the system fonts are known (the splash shows meanwhile).
        let layout = if cfg!(test) { layout_document(&doc) } else { Default::default() };
        let mut app = WordApp {
            doc, layout, measure_cache: Default::default(), path: None, zoom: 1.0, dialogs: Dialogs::default(), find: FindState::default(),
            show_outline: false, show_marks: false, clipboard: None, goal_x: None, scroll_to_caret: false,
            caret_blink: 0.0, preedit: String::new(), textures: HashMap::new(), status: None,
            drag: canvas_state::Drag::None, allow_close: false, canvas_focus: true,
            author: std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "Author".into()), show_comments: false, comment_edit: None, reply: None, show_notes: false, note_edit: None, tab: Default::default(), format_painter: None, last_color: word_engine::Color(0xC0, 0, 0), last_highlight: "yellow".into(), fit_width: false, last_click: (f64::NEG_INFINITY, egui::Pos2::ZERO, 0), canvas_map: (0.0, 0.0, vec![], 1.0), show_gridlines: true, atlas: Default::default(), ctx: cc.egui_ctx.clone(), splash: None, custom_lists: load_custom_lists(), insert: Default::default(), show_ruler: true, ruler_tab_kind: word_engine::TabKind::Left, focus_mode: false, read_mode: false, read_row: 0, read_scroll: false, ruler_rect: egui::Rect::NOTHING, table_tool: None, border_pen: word_engine::Border { width: 0.5, space: 0.0, color: None }, formula: None, extras: crate::extras::Extras { quick_parts: crate::text_tools::load_quick_parts(), ..Default::default() },
        };
        // A file macOS sent before the window existed opens like one on the command line.
        qalam_kit::open_files::set_context(&cc.egui_ctx);
        let (sent, mut others) = qalam_kit::open_files::startup_paths(Vec::new());
        let path = match (path, sent) { (Some(p), Some(s)) => { others.insert(0, s); Some(p) } (p, s) => p.or(s) };
        if !cfg!(test) { qalam_kit::open_files::open_elsewhere(&others); }
        if cfg!(test) {
            if let Some(p) = path { app.open_path(p); }
        } else {
            // Fonts installed on this computer load in the background (names cached between runs).
            app.splash = Some((0.0, path));
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                word_engine::fonts::load_system_fonts(Some(&crate::config_dir().join("font-index.txt")));
                FONTS_READY.store(true, std::sync::atomic::Ordering::Release);
                ctx.request_repaint();
            });
        }
        app
    }

    fn welcome() -> Document {
        let mut d = Document::new(vec![Paragraph::default()]);
        // Set as Default (Design tab) applies to new documents.
        if let Some(defaults) = crate::design::load_defaults() { crate::design::apply_defaults(&mut d, &defaults); }
        d.dirty = false;
        d
    }

    pub fn relayout(&mut self) {
        self.apply_view_override();
        if self.layout.revision != self.doc.revision || self.layout.pages.is_empty() {
            self.layout = word_engine::layout_with_cache(&self.doc, &mut self.measure_cache);
            // A header or footer being edited is laid out where it sits on its page.
            if self.extras.story.is_some() { self.place_story_layout(); }
        }
    }

    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
        format!("{}{} — {}", if self.doc.dirty { "• " } else { "" }, name, crate::APP_NAME)
    }

    pub fn notify(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.status = Some((text.into(), ctx.input(|i| i.time) + 4.0));
    }

    // ---- file commands ----------------------------------------------------------

    /// Run an action that would discard the document, asking first when it has unsaved changes.
    pub fn guarded(&mut self, ctx: &egui::Context, action: Pending) {
        if self.doc.dirty { self.dialogs.confirm = Some(action); } else { self.run_pending(ctx, action); }
    }

    pub fn run_pending(&mut self, ctx: &egui::Context, action: Pending) {
        match action {
            Pending::New => { self.set_document(Self::welcome(), None); }
            Pending::Open => {
                let mut picker = rfd::FileDialog::new();
                if let Some(d) = &self.extras.options.settings.default_folder { picker = picker.set_directory(d); }
                if let Some(p) = picker
                    .add_filter("Documents", &["docx", "dotx", "docm", "dotm", "md", "markdown", "txt"])
                    .add_filter("Word documents", &["docx", "dotx", "docm", "dotm"])
                    .add_filter("All files", &["*"]).pick_file() { self.open_path(p); }
            }
            Pending::OpenPath(p) => self.open_path(p),
            Pending::Quit => { self.clear_recovery(); self.allow_close = true; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        }
    }

    pub fn set_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.extras.story = None;
        self.extras.draw.selected.clear();
        self.doc = doc;
        self.doc.dirty = false;
        self.path = path;
        self.textures.clear();
        self.measure_cache = Default::default();
        self.show_comments = !self.doc.comments.is_empty();
        self.comment_edit = None;
        self.reply = None;
        self.note_edit = None;
        self.show_notes = !self.doc.notes.is_empty();
        self.find.revision = u64::MAX;
        // A newly opened document starts in Print Layout, without the previous one's split,
        // side-by-side document, reader views or panes; preferences (dark mode, units, ruler,
        // navigation pane) stay.
        let v = &mut self.extras.view;
        (v.mode, v.split, v.side, v.immersive, v.multi_page) = (crate::view::ViewMode::Print, false, None, None, false);
        v.collapsed.clear();
        v.offsets = Default::default();
        v.deltas = Default::default();
        self.doc.view_override = None;
        self.read_mode = false;
        self.extras.nav = Default::default();
        let r = &mut self.extras.review;
        (r.editor_open, r.thesaurus, r.accessibility, r.reviewing_pane, r.restrict, r.unlock, r.compare, r.translate) = (false, None, false, false, None, None, None, None);
        r.issues.clear();
        r.issues_rev = u64::MAX;
        self.extras.print = None;
        self.extras.mailings = Default::default();
        self.extras.macros.editor = None;
        self.layout = layout_document(&self.doc);
        self.scroll_to_caret = true;
    }

    /// Opens files the system sent while running (Finder, Open With): the first here (asking
    /// to save changes first), any others in new windows.
    pub fn open_sent_files(&mut self, ctx: &egui::Context) {
        // The queue is shared by the whole process; tests opt in per thread.
        #[cfg(test)]
        if !TAKE_SENT_FILES.with(|t| t.get()) { return; }
        if self.dialogs.confirm.is_some() || !qalam_kit::open_files::pending() { return; }
        let (here, rest) = qalam_kit::open_files::route(qalam_kit::open_files::take());
        if !cfg!(test) { qalam_kit::open_files::open_elsewhere(&rest); }
        if let Some(p) = here { self.guarded(ctx, Pending::OpenPath(p)); }
    }

    pub fn open_path(&mut self, path: PathBuf) {
        self.end_story();
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let result = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|bytes| match ext.as_str() {
            "md" | "markdown" => Ok((import::from_markdown(&String::from_utf8_lossy(&bytes)), false)),
            "txt" | "text" => Ok((import::from_text(&String::from_utf8_lossy(&bytes)), false)),
            _ => docx::read(&bytes).map(|d| (d, true)),
        });
        match result {
            Ok((doc, native)) => { self.remember_recent(&path); self.set_document(doc, if native { Some(path) } else { None }) }
            Err(e) => self.dialogs.error = Some(crate::i18n::trf("Couldn't open {}.\n\n{}", &[&path.display(), &e])),
        }
    }

    pub fn save(&mut self, ctx: &egui::Context, save_as: bool) -> bool {
        // A header or footer being edited is closed into the document first.
        self.end_story();
        let path = match (&self.path, save_as) {
            (Some(p), false) => p.clone(),
            _ => {
                let name = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into());
                let mut picker = rfd::FileDialog::new();
                if let Some(d) = &self.extras.options.settings.default_folder { picker = picker.set_directory(d); }
                match picker.add_filter("Word document", &["docx"]).set_file_name(format!("{name}.docx")).save_file() {
                    Some(p) => if p.extension().is_none() { p.with_extension("docx") } else { p },
                    None => return false,
                }
            }
        };
        self.backup_before_save(&path);
        match docx::write(&self.doc).and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.clear_recovery();
                self.remember_recent(&path);
                // Later saves preserve the parts we just wrote.
                if let Ok(saved) = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| docx::read(&b)) { self.doc.package = saved.package; }
                self.doc.numbering.changed = false;
                self.doc.dirty = false;
                self.notify(ctx, crate::i18n::trf("Saved {}", &[&path.display()]));
                self.path = Some(path);
                true
            }
            Err(e) => { self.dialogs.error = Some(crate::i18n::trf("Couldn't save the document.\n\n{}", &[&e])); false }
        }
    }

    pub fn export(&mut self, ctx: &egui::Context, markdown: bool) {
        self.end_story();
        let (ext, label) = if markdown { ("md", "Markdown") } else { ("txt", "Plain text") };
        let name = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into());
        if let Some(p) = rfd::FileDialog::new().add_filter(label, &[ext]).set_file_name(format!("{name}.{ext}")).save_file() {
            let text = if markdown { import::to_markdown(&self.doc) } else { import::to_text(&self.doc) };
            match std::fs::write(&p, text) {
                Ok(()) => self.notify(ctx, crate::i18n::trf("Exported {}", &[&p.display()])),
                Err(e) => self.dialogs.error = Some(e.to_string()),
            }
        }
    }

    pub fn export_pdf(&mut self, ctx: &egui::Context) {
        self.end_story();
        let name = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into());
        let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{name}.pdf")).save_file() else { return };
        self.relayout();
        let bytes = word_engine::pdf::export(&self.doc, &self.layout, &name);
        match std::fs::write(&p, bytes) {
            Ok(()) => self.notify(ctx, crate::i18n::trf("Exported {}", &[&p.display()])),
            Err(e) => self.dialogs.error = Some(e.to_string()),
        }
    }

    /// Paste matching the formatting at the caret (Merge Formatting).
    pub fn paste_merged(&mut self) {
        if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) { self.doc.replace(&text.replace("\r\n", "\n")); self.scroll_to_caret = true; }
    }
    /// Paste plain text, even when the clipboard holds a rich in-app copy (Keep Text Only).
    pub fn paste_plain(&mut self) {
        let text = arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok().or_else(|| self.clipboard.as_ref().map(|c| c.0.clone()));
        if let Some(t) = text { self.doc.replace(&t.replace("\r\n", "\n")); self.scroll_to_caret = true; }
    }
    pub fn save_custom_lists(&self) {
        if let Some(p) = custom_lists_path() {
            let _ = std::fs::create_dir_all(p.parent().unwrap());
            let _ = std::fs::write(p, self.custom_lists.iter().map(|l| l.to_line()).collect::<Vec<_>>().join("\n"));
        }
    }
    pub fn open_list_dialog(&mut self, kind: ListDialogKind) {
        use word_engine::editing::CustomList;
        use word_engine::numbering::{NumFmt, Script};
        let script = self.doc.list_script(self.doc.locate(self.doc.range().0).0);
        let list = match kind {
            ListDialogKind::Bullet => CustomList::bullet("My bullet", '\u{25B6}', None),
            ListDialogKind::Number => CustomList::number("My numbering", NumFmt::Decimal, "%1.", 1, script),
            ListDialogKind::Multilevel => {
                let mut l = CustomList::number("My outline", NumFmt::Decimal, "%1.", 1, Script::Latin);
                for (i, lv) in l.levels.iter_mut().enumerate().skip(1) { lv.fmt = NumFmt::Decimal; lv.text = (1..=i + 1).map(|k| format!("%{k}.")).collect(); lv.hanging = 360 + 120 * i as i32; }
                l
            }
        };
        self.dialogs.list = Some(ListDraft { kind, list, level: 0, save: true });
    }
    pub fn open_font_dialog(&mut self) {
        let f = self.doc.current_format();
        self.dialogs.font = Some(FontDraft {
            font: f.font.clone(), bold: f.bold, italic: f.italic, size: f.size, color: f.color, underline: f.underline,
            strike: f.strike == word_engine::Strike::Single, double_strike: f.strike == word_engine::Strike::Double,
            superscript: f.vert_align == word_engine::VertAlign::Superscript, subscript: f.vert_align == word_engine::VertAlign::Subscript,
            small_caps: f.caps == word_engine::Caps::SmallCaps, all_caps: f.caps == word_engine::Caps::AllCaps,
            hidden: f.vanish, outline: f.outline, shadow: f.shadow, spacing: f.spacing,
        });
    }
    pub fn open_table_properties(&mut self) {
        let Some(b) = self.doc.cell_block() else { return };
        let t = &self.doc.tables[b.table];
        let v = t.cell_at_grid(b.r0, b.c0).map(|(_, c)| c.v_align).unwrap_or_default();
        self.dialogs.table_props = Some(TableDraft {
            table: b.table, alignment: t.alignment, indent: t.indent as f32 / 1440.0, width: t.grid.iter().sum::<i32>() as f32 / 1440.0,
            row_height: t.rows[b.r0].min_height as f32 / 1440.0, exact: t.rows[b.r0].exact, header: t.rows[b.r0].header, cant_split: t.rows[b.r0].cant_split,
            v_align: v, margins: t.cell_margins.map(|m| m as f32 / 1440.0),
        });
    }

    pub fn insert_picture(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() else { return };
        match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|bytes| image::load_from_memory(&bytes).map(|i| (bytes, i)).map_err(|e| e.to_string())) {
            Ok((bytes, img)) => {
                let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                let ct = match ext.as_str() { "png" => "image/png", "gif" => "image/gif", "bmp" => "image/png", _ => "image/jpeg" };
                // BMP isn't a portable DOCX picture format; store it as PNG.
                let bytes = if ext == "bmp" { let mut out = std::io::Cursor::new(vec![]); let _ = img.write_to(&mut out, image::ImageFormat::Png); out.into_inner() } else { bytes };
                // Pixels at 96 dpi.
                self.doc.insert_image(bytes, ct, img.width() as f32, img.height() as f32);
                self.scroll_to_caret = true;
            }
            Err(e) => self.dialogs.error = Some(crate::i18n::trf("Couldn't insert the picture.\n\n{}", &[&e])),
        }
    }

    // ---- clipboard ------------------------------------------------------------------

    pub fn copy(&mut self, ctx: &egui::Context) -> bool {
        if self.doc.selection.is_caret() { return false; }
        let text = self.doc.selected_text();
        self.clipboard = Some((text.clone(), self.doc.fragment()));
        ctx.copy_text(text);
        true
    }
    pub fn cut(&mut self, ctx: &egui::Context) {
        if self.copy(ctx) { self.doc.replace(""); self.scroll_to_caret = true; }
    }
    pub fn paste_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        match &self.clipboard {
            Some((plain, fragment)) if *plain == text => { let f = fragment.clone(); self.doc.paste_fragment(f); }
            _ => self.doc.replace(&text),
        }
        self.scroll_to_caret = true;
    }
    /// Menu paste reads the system clipboard directly (keyboard paste arrives as an event).
    pub fn paste_from_system(&mut self) {
        if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) { self.paste_text(&text); }
    }

    // ---- find -------------------------------------------------------------------------

    pub fn refresh_find(&mut self) {
        if self.find.revision == self.doc.revision { return; }
        self.find.matches = self.doc.find_all(&self.find.query, self.find.match_case, self.find.whole_words);
        self.find.revision = self.doc.revision;
        self.find.current = None;
    }
    pub fn find_next(&mut self, forward: bool) {
        self.find.revision = u64::MAX;
        self.refresh_find();
        if self.find.matches.is_empty() { return; }
        let (s, e) = self.doc.range();
        let n = self.find.matches.len();
        let i = if forward {
            self.find.matches.iter().position(|m| m.0 >= e && (m.0, m.1) != (s, e)).unwrap_or(0)
        } else {
            self.find.matches.iter().rposition(|m| m.1 <= s).unwrap_or(n - 1)
        };
        let (a, b) = self.find.matches[i];
        self.doc.select(a, b);
        self.find.current = Some(i);
        self.scroll_to_caret = true;
    }
    pub fn replace_current(&mut self) {
        let (s, e) = self.doc.range();
        if self.doc.find_all(&self.find.query, self.find.match_case, self.find.whole_words).contains(&(s, e)) {
            let with = self.find.replace.clone();
            self.doc.replace(&with);
        }
        self.find_next(true);
    }
    pub fn replace_all(&mut self, ctx: &egui::Context) {
        let n = self.doc.replace_all(&self.find.query.clone(), &self.find.replace.clone(), self.find.match_case, self.find.whole_words);
        self.notify(ctx, crate::i18n::trf(if n == 1 { "Replaced {} match" } else { "Replaced {} matches" }, &[&n]));
    }

    // ---- comments ------------------------------------------------------------------

    pub fn new_comment(&mut self) {
        let id = self.doc.add_comment(&self.author.clone(), "", &now_iso());
        self.show_comments = true;
        self.comment_edit = Some((id, String::new(), true));
        self.canvas_focus = false;
    }

    pub fn insert_note(&mut self, kind: word_engine::NoteKind) {
        let id = self.doc.insert_note(kind);
        self.show_notes = true;
        self.note_edit = Some((kind, id, String::new()));
        self.canvas_focus = false;
    }

    // ---- zoom --------------------------------------------------------------------------

    /// Screen position of a layout point (as the canvas last painted it).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn screen_pos(&self, page: usize, x: f32, y: f32) -> egui::Pos2 {
        let (ox, top, tops, zoom) = &self.canvas_map;
        egui::pos2(ox + (crate::canvas::GAP + x) * zoom, top + (tops.get(page).copied().unwrap_or(0.0) + y) * zoom)
    }
    /// Screen position of the middle of the caret slot before `pos`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn screen_of(&self, pos: usize) -> egui::Pos2 {
        let c = self.layout.caret_rect(pos).unwrap();
        self.screen_pos(c.page, c.x, c.y + c.height / 2.0)
    }

    pub fn set_zoom(&mut self, z: f32) { self.zoom = (z * 20.0).round() / 20.0; self.zoom = self.zoom.clamp(0.25, 4.0); self.scroll_to_caret = true; }
}

/// Set once the system fonts are loaded.
static FONTS_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl WordApp {
    /// The splash screen while fonts load: the icon, name and maker. Returns true while it shows.
    fn show_splash(&mut self, ui: &mut egui::Ui) -> bool {
        let now = ui.input(|i| i.time);
        let Some((start, _)) = &mut self.splash else { return false };
        if *start == 0.0 { *start = now.max(1e-6); }
        let elapsed = now - *start;
        let ready = FONTS_READY.load(std::sync::atomic::Ordering::Acquire);
        if ready && elapsed > 1.2 {
            let pending = self.splash.take().and_then(|s| s.1);
            // Grow the splash card into the main window.
            let (size, pos, maximized) = crate::WINDOW_TARGET.get().copied().unwrap_or(([1200.0, 860.0], None, false));
            let ctx = ui.ctx();
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(480.0, 360.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(size[0], size[1])));
            let pos = pos.map(|p| egui::pos2(p[0], p[1])).or_else(|| {
                let m = ctx.input(|i| i.viewport().monitor_size)?;
                Some(egui::pos2(((m.x - size[0]) / 2.0).max(0.0), ((m.y - size[1]) / 2.0).max(24.0)))
            });
            if let Some(p) = pos { ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(p)); }
            if maximized { ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true)); }
            self.layout = layout_document(&self.doc);
            if let Some(p) = pending { self.open_path(p); }
            self.auto_check_updates(&ui.ctx().clone());
            return false;
        }
        static ICON: std::sync::OnceLock<Option<egui::TextureHandle>> = std::sync::OnceLock::new();
        let icon = ICON.get_or_init(|| image::load_from_memory(include_bytes!("../../assets/icon.png")).ok().map(|i| {
            let i = i.to_rgba8();
            ui.ctx().load_texture("splash-icon", egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw()), egui::TextureOptions::LINEAR)
        }));
        let dark = self.extras.view.dark;
        let (bg, ink, sub) = if dark { (egui::Color32::from_gray(0x1C), egui::Color32::from_gray(0xF2), egui::Color32::from_gray(0xA8)) } else { (egui::Color32::WHITE, egui::Color32::from_rgb(0x1B, 0x2B, 0x4B), egui::Color32::from_gray(0x70)) };
        egui::CentralPanel::no_frame().show(ui, |ui| {
            // A rounded card on a transparent window, with a soft edge.
            let rect = ui.max_rect().shrink(10.0);
            ui.painter().add(egui::epaint::Shadow { offset: [0, 3], blur: 12, spread: 0, color: egui::Color32::from_black_alpha(70) }.as_shape(rect, 16.0));
            ui.painter().rect(rect, 16.0, bg, egui::Stroke::new(1.0, sub.gamma_multiply(0.25)), egui::StrokeKind::Inside);
            if ui.interact(rect, egui::Id::new("splash-drag"), egui::Sense::drag()).drag_started() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag); }
            // A gentle fade-in.
            let a = (elapsed / 0.4).clamp(0.0, 1.0) as f32;
            let c = rect.center() - egui::vec2(0.0, 44.0);
            if let Some(t) = icon { ui.painter().image(t.id(), egui::Rect::from_center_size(c - egui::vec2(0.0, 10.0), egui::vec2(120.0, 120.0)), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE.gamma_multiply(a)); }
            ui.painter().text(c + egui::vec2(0.0, 70.0), egui::Align2::CENTER_CENTER, "Qalam  قلم", egui::FontId::new(30.0, egui::FontFamily::Name("ui-bold".into())), ink.gamma_multiply(a));
            ui.painter().text(c + egui::vec2(0.0, 104.0), egui::Align2::CENTER_CENTER, crate::i18n::tr(if ready { "Starting…" } else { "Loading fonts…" }), egui::FontId::proportional(14.0), sub.gamma_multiply(a));
            // A thin progress shimmer.
            let w = 160.0;
            let bar = egui::Rect::from_center_size(c + egui::vec2(0.0, 128.0), egui::vec2(w, 3.0));
            ui.painter().rect_filled(bar, 1.5, sub.gamma_multiply(0.25 * a));
            let t = ((elapsed * 0.8) % 1.0) as f32;
            let seg = egui::Rect::from_min_size(egui::pos2(bar.min.x + (w - 50.0) * t, bar.min.y), egui::vec2(50.0, 3.0));
            ui.painter().rect_filled(seg, 1.5, crate::ribbon::ACCENT.gamma_multiply(a));
            ui.painter().text(egui::pos2(rect.center().x, rect.max.y - 22.0), egui::Align2::CENTER_CENTER, format!("{} Woosool · woosool.com", crate::i18n::tr("Made by")), egui::FontId::proportional(12.0), sub.gamma_multiply(a));
        });
        ui.ctx().request_repaint();
        true
    }
}

impl eframe::App for WordApp {
    /// One frame. A panic anywhere in it is caught: the edit it interrupted is undone, the
    /// details go to crash.log, and the app keeps running instead of closing.
    /// Clear to transparent while the splash card shows (its window has no background).
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.splash.is_some() { [0.0; 4] } else { visuals.panel_fill.to_normalized_gamma_f32() }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let before = self.doc.revision;
        let (comments, texts) = (self.doc.comments.len(), self.doc.comments.iter().map(|c| c.text.clone()).collect::<Vec<_>>());
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.frame(ui, frame)));
        if r.is_ok() { self.enforce_protection(before, comments, &texts); }
        if r.is_err() {
            if self.doc.revision != before { self.doc.undo(); }
            self.layout.pages.clear();
            self.measure_cache = Default::default();
            self.canvas_focus = true;
            self.dialogs.error = Some(crate::i18n::trf("Something went wrong and the last action was undone. Details were saved to {}.", &[&crate::crash_log_path().display()]));
            ui.ctx().request_repaint();
        }
    }
}

impl WordApp {
    fn frame(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.splash.is_some() { self.apply_settings(&ctx); crate::i18n::set(self.extras.options.settings.lang); if self.show_splash(ui) { return; } }
        self.apply_settings(&ctx);
        crate::i18n::set(self.extras.options.settings.lang);
        self.track_window_and_autosave(&ctx);
        // Macro shortcuts come first: other shortcuts on the same digits ignore Shift.
        self.macro_shortcuts(&ctx);
        // Closing with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && self.doc.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialogs.confirm = Some(Pending::Quit);
        }
        // Files dropped on the window open.
        if let Some(p) = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| Some(f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()))) { self.guarded(&ctx, Pending::OpenPath(p)); }
        self.open_sent_files(&ctx);
        if !cfg!(test) { qalam_kit::default_apps::first_run_prompt(&ctx, &qalam_kit::default_apps::QALAM, &crate::config_dir().join("default-apps-asked")); }
        self.doc.revision_author = self.author.clone();
        self.doc.clock = now_iso;
        self.doc.local_clock = local_iso;
        self.doc.file_name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        crate::ui::shortcuts(self, &ctx);
        self.relayout();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        if self.read_mode {
            crate::ui::read_bar(self, ui);
        } else if self.focus_mode {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { self.focus_mode = false; }
        } else {
            crate::ribbon::ribbon(self, ui);
        }
        if !self.focus_mode && !self.read_mode { crate::ui::status_bar(self, ui); }
        if self.find.open { crate::ui::find_panel(self, ui); }
        if self.show_outline { crate::nav::pane(self, ui); }
        if self.show_comments { crate::ui::comments_panel(self, ui); }
        if self.show_notes { crate::ui::notes_panel(self, ui); }
        crate::insert::panes(self, ui);
        crate::panes::panes(self, ui);
        crate::refs::selection_pane(self, ui);
        crate::review::panes(self, ui);
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if self.show_ruler && !self.focus_mode && !self.read_mode {
                egui::Panel::top("hruler").show_separator_line(false).exact_size(crate::ruler::H).frame(egui::Frame::NONE).show(ui, |ui| crate::ruler::horizontal(self, ui));
                egui::Panel::left("vruler").show_separator_line(false).exact_size(crate::ruler::W).resizable(false).frame(egui::Frame::NONE).show(ui, |ui| crate::ruler::vertical(self, ui));
            }
            crate::view::UNITS.with(|u| u.set(self.extras.view.units));
            self.record_keys(ui);
            if let Some(mut side) = self.extras.view.side.take() {
                // View Side by Side: the other document on the right, editable in place.
                let mut close = false;
                egui::Panel::right("side-by-side").resizable(true).default_size(ui.available_width() / 2.0).frame(egui::Frame::NONE).show(ui, |ui| {
                    ui.horizontal(|ui| { ui.add_space(8.0); ui.label(egui::RichText::new(crate::i18n::tr(&side.name)).strong()); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| if crate::ribbon::close_button(ui, "Close side by side").clicked() { close = true; }); });
                    std::mem::swap(&mut self.doc, &mut side.doc);
                    std::mem::swap(&mut self.layout, &mut side.layout);
                    std::mem::swap(&mut self.measure_cache, &mut side.cache);
                    let path = self.path.take();
                    self.extras.view.pane = 2;
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| egui::CentralPanel::no_frame().show(ui, |ui| crate::canvas::show(self, ui))));
                    self.path = path;
                    std::mem::swap(&mut self.doc, &mut side.doc);
                    std::mem::swap(&mut self.layout, &mut side.layout);
                    std::mem::swap(&mut self.measure_cache, &mut side.cache);
                    if let Err(e) = r { std::panic::resume_unwind(e); }
                });
                if !close { self.extras.view.side = Some(side); }
            }
            if !crate::view::central(self, ui) {
                if self.extras.view.split {
                    // Split: a second view of the same document above.
                    self.extras.view.pane = 1;
                    egui::Panel::top("split-view").resizable(true).default_size(ui.available_height() / 2.0).frame(egui::Frame::NONE).show(ui, |ui| egui::CentralPanel::no_frame().show(ui, |ui| crate::canvas::show(self, ui)));
                }
                self.extras.view.pane = 0;
                egui::CentralPanel::no_frame().show(ui, |ui| crate::canvas::show(self, ui));
            }
        });
        crate::ui::dialogs(self, &ctx);
        crate::insert::dialogs(self, &ctx);
        crate::symbols::dialogs(self, &ctx);
        crate::text_tools::dialogs(self, &ctx);
        crate::design::dialogs(self, &ctx);
        crate::refs::dialogs(self, &ctx);
        crate::review::dialogs(self, &ctx);
        crate::view::dialogs(self, &ctx);
        crate::mailings::dialogs(self, &ctx);
        crate::print::dialog(self, &ctx);
        crate::macros::dialogs(self, &ctx);
        crate::options::dialog(self, &ctx);
    }
}
