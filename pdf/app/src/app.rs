//! Application state, commands and the window chrome (title bar, toolbar, panes).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, Vec2};
use pdf_engine::{Color, Document, SearchHit};

use crate::i18n::{n_, tr, trf};
use crate::raster::Raster;
use crate::theme::{self, Icon, palette};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
}

/// The tool sets across the top, like the reference's Tools / Comment tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSet {
    View,
    Organize,
    Comment,
    Edit,
    Sign,
    Forms,
    Redact,
}

impl ToolSet {
    pub const ALL: [ToolSet; 7] = [
        ToolSet::View,
        ToolSet::Organize,
        ToolSet::Comment,
        ToolSet::Edit,
        ToolSet::Sign,
        ToolSet::Forms,
        ToolSet::Redact,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ToolSet::View => n_("View"),
            ToolSet::Organize => n_("Organize pages"),
            ToolSet::Comment => n_("Comment"),
            ToolSet::Edit => n_("Edit PDF"),
            ToolSet::Sign => n_("Fill & Sign"),
            ToolSet::Forms => n_("Forms"),
            ToolSet::Redact => n_("Redact"),
        }
    }
}

/// What a press-and-drag on a page does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Highlight,
    Underline,
    Strike,
    TextBox,
    Note,
    Rectangle,
    Ellipse,
    Ink,
    /// Click a line of existing text to rewrite it.
    EditText,
    /// Click to add new page text.
    AddText,
    /// Click to place a signature, initials, date or check mark.
    Place(Stamp),
    /// Click form fields to fill them.
    Fill,
    /// Drag to add a form field.
    Field(pdf_engine::FieldKind),
    /// Drag across text to mark it for redaction.
    RedactText,
    /// Drag to mark an area for redaction.
    RedactArea,
}

/// Things Fill & Sign places with one click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stamp {
    Signature(usize),
    Initials(usize),
    Date,
    Check,
}

/// The selected comment or page object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picked {
    Annot {
        page: usize,
        slot: usize,
    },
    Item {
        page: usize,
        id: u32,
    },
    /// A form field widget (Prepare form).
    Field {
        page: usize,
        slot: usize,
    },
}

impl Picked {
    pub fn page(self) -> usize {
        match self {
            Picked::Annot { page, .. } | Picked::Item { page, .. } | Picked::Field { page, .. } => {
                page
            }
        }
    }
}

impl Tool {
    pub const COMMENT: [Tool; 9] = [
        Tool::Select,
        Tool::Highlight,
        Tool::Underline,
        Tool::Strike,
        Tool::TextBox,
        Tool::Note,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Ink,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => n_("Select"),
            Tool::Highlight => n_("Highlight"),
            Tool::Underline => n_("Underline"),
            Tool::Strike => n_("Strikethrough"),
            Tool::TextBox => n_("Text box"),
            Tool::Note => n_("Sticky note"),
            Tool::Rectangle => n_("Rectangle"),
            Tool::Ellipse => n_("Ellipse"),
            Tool::Ink => n_("Draw"),
            Tool::EditText => n_("Edit text"),
            Tool::AddText => n_("Add text"),
            Tool::Place(Stamp::Date) => n_("Date"),
            Tool::Place(Stamp::Check) => n_("Check mark"),
            Tool::Place(_) => n_("Place signature"),
            Tool::Fill => n_("Fill form"),
            Tool::Field(k) => crate::forms::kind_label(k),
            Tool::RedactText => n_("Mark text"),
            Tool::RedactArea => n_("Mark area"),
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Tool::Select => Icon::Select,
            Tool::Highlight => Icon::Highlight,
            Tool::Underline => Icon::Underline,
            Tool::Strike => Icon::Strike,
            Tool::TextBox => Icon::TextBox,
            Tool::Note => Icon::Note,
            Tool::Rectangle => Icon::Rectangle,
            Tool::Ellipse => Icon::Ellipse,
            Tool::Ink => Icon::Ink,
            Tool::EditText => Icon::EditText,
            Tool::AddText => Icon::TextBox,
            Tool::Place(Stamp::Date) => Icon::Calendar,
            Tool::Place(Stamp::Check) => Icon::Check,
            Tool::Place(_) => Icon::Signature,
            Tool::Fill => Icon::TextBox,
            Tool::Field(k) => crate::forms::kind_icon(k),
            Tool::RedactText => Icon::Strike,
            Tool::RedactArea => Icon::Rectangle,
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => n_(
                "Click a comment or object to select it, drag to move it, drag a handle to resize. Double-click text to edit it.",
            ),
            Tool::EditText => n_(
                "Click a line of text to rewrite it. The original glyphs are removed and the new text is written in their place.",
            ),
            Tool::AddText => n_("Click on the page where the text should start, then type."),
            Tool::Place(_) => n_("Click on the page to place it; then drag to move or resize."),
            Tool::Fill => n_("Click a field to fill it. Tab moves to the next field."),
            Tool::Field(_) => n_("Drag on the page to add the field, or click for a default size."),
            Tool::RedactText => n_("Drag across text to mark it for redaction."),
            Tool::RedactArea => n_("Drag over any area (text, pictures, drawings) to mark it."),
            Tool::Highlight | Tool::Underline | Tool::Strike => {
                n_("Drag across text to mark it, or drag over an empty area to mark a region.")
            }
            Tool::TextBox => n_("Click or drag on the page, then type."),
            Tool::Note => n_("Click on the page to place a note."),
            Tool::Rectangle | Tool::Ellipse => n_("Drag on the page to draw."),
            Tool::Ink => n_("Drag to draw freehand."),
        }
    }
    pub fn uses_stroke(self) -> bool {
        matches!(self, Tool::Rectangle | Tool::Ellipse | Tool::Ink)
    }
    pub fn default_color(self) -> Color {
        match self {
            Tool::Highlight => Color::rgb(255, 214, 10),
            Tool::Note => Color::rgb(255, 205, 70),
            Tool::TextBox => Color::rgb(21, 21, 21),
            _ => Color::rgb(200, 45, 45),
        }
    }
}

pub const SWATCHES: [Color; 7] = [
    Color::rgb(255, 214, 10),
    Color::rgb(120, 220, 110),
    Color::rgb(90, 180, 250),
    Color::rgb(245, 130, 190),
    Color::rgb(200, 45, 45),
    Color::rgb(40, 90, 200),
    Color::rgb(21, 21, 21),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePanel {
    Pages,
    Comments,
}

/// Something waiting on "save changes?".
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Close,
    OpenDialog,
    Open(PathBuf),
    New,
}

#[derive(Default)]
pub struct Search {
    pub query: String,
    pub searched: String,
    pub hits: Vec<SearchHit>,
    pub current: usize,
    pub focus: bool,
}

/// What the inline text editor is writing.
#[derive(Clone, Debug, PartialEq)]
pub enum DraftTarget {
    NewTextBox,
    NewNote,
    /// Re-wording an existing text box or note.
    Annot(usize),
    /// New page text.
    NewText,
    /// Re-wording page text added earlier.
    Item(u32),
    /// Filling a text form field.
    Field(String),
    /// Rewriting an existing line of the original page text.
    Line {
        at: pdf_engine::Point,
        line: pdf_engine::TextLine,
    },
}

/// Text waiting to be committed.
pub struct TextDraft {
    pub page: usize,
    pub rect: pdf_engine::Rect,
    pub target: DraftTarget,
    pub text: String,
    pub focus: bool,
}

impl TextDraft {
    pub fn is_note(&self) -> bool {
        self.target == DraftTarget::NewNote
            || matches!(self.target, DraftTarget::Annot(_)) && self.rect.width() <= 21.0
    }
}

pub struct App {
    pub doc: Option<Document>,
    pub path: Option<PathBuf>,
    pub name: String,
    pub raster: Raster,
    /// The next raster refresh may keep old bitmaps (page order unchanged).
    pub keep_stale: bool,
    pub theme: ThemeChoice,
    pub toolset: ToolSet,
    pub tool: Tool,
    pub colors: [Color; 9],
    pub stroke: f32,
    pub font_size: f32,
    pub zoom: f32,
    pub fit_width: bool,
    pub current_page: usize,
    pub scroll_to_page: Option<usize>,
    pub scroll_offset: Vec2,
    pub pending_offset: Option<Vec2>,
    pub selection: BTreeSet<usize>,
    pub anchor: Option<usize>,
    pub side: Option<SidePanel>,
    pub tools_pane: bool,
    pub search: Search,
    pub draft: Option<TextDraft>,
    pub picked: Option<Picked>,
    /// Style for new page text (Edit PDF).
    pub text_face: pdf_engine::FontFace,
    pub text_size: f32,
    pub text_color: Color,
    pub signatures: crate::signatures::Store,
    pub sign_dialog: Option<crate::signatures::Dialog>,
    /// Slider values for the selection, staged until the drag ends.
    /// Fill and opacity for new rectangles/ellipses; background for new text boxes.
    pub shape_fill: Option<Color>,
    pub shape_opacity: f32,
    pub text_box_fill: Option<Color>,
    pub redact: crate::redact::RedactState,
    pub forms: crate::forms::FormsState,
    pub settings: crate::prefs::Settings,
    /// The splash card: when it started (0 until its first frame).
    pub splash: Option<f64>,
    /// A file to open once the splash card is gone.
    pub pending_open: Option<PathBuf>,
    pub about_open: bool,
    pub settings_open: bool,
    pub prop_for: Option<Picked>,
    pub prop_for_opacity: Option<Picked>,
    pub prop_opacity: f32,
    pub prop_width: f32,
    pub prop_size: f32,
    pub toast: Option<(String, Instant)>,
    pub confirm: Option<Pending>,
    pub allow_close: bool,
    pub canvas: crate::canvas::CanvasState,
    pub organize: crate::organize::OrganizeState,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>, threaded: bool) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::install(&cc.egui_ctx);
        // Ctrl +/- zooms the document, not the whole interface.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let settings = crate::prefs::Settings::load();
        let mut app = Self::with_context(&cc.egui_ctx, threaded);
        app.theme = settings.theme;
        app.settings = settings;
        if let Some(dir) = crate::signatures::default_dir() {
            app.signatures = crate::signatures::Store::open(dir);
        }
        app.splash = app.settings.splash.then_some(0.0);
        match path {
            Some(path) if app.splash.is_some() => app.pending_open = Some(path),
            Some(path) => app.open_path(&path),
            None => {}
        }
        app
    }

    pub fn with_context(ctx: &egui::Context, threaded: bool) -> Self {
        Self {
            doc: None,
            path: None,
            name: String::new(),
            raster: Raster::new(ctx, threaded),
            keep_stale: false,
            theme: ThemeChoice::System,
            toolset: ToolSet::View,
            tool: Tool::Select,
            colors: Tool::COMMENT.map(Tool::default_color),
            stroke: 2.0,
            font_size: 14.0,
            zoom: 1.0,
            fit_width: true,
            current_page: 0,
            scroll_to_page: None,
            scroll_offset: Vec2::ZERO,
            pending_offset: None,
            selection: BTreeSet::new(),
            anchor: None,
            side: Some(SidePanel::Pages),
            tools_pane: true,
            search: Search::default(),
            draft: None,
            picked: None,
            text_face: pdf_engine::FontFace::Sans,
            text_size: 14.0,
            text_color: Color::rgb(21, 21, 21),
            signatures: crate::signatures::Store::default(),
            sign_dialog: None,
            shape_fill: None,
            shape_opacity: 1.0,
            text_box_fill: None,
            redact: Default::default(),
            forms: Default::default(),
            settings: crate::prefs::Settings::default(),
            splash: None,
            pending_open: None,
            about_open: false,
            settings_open: false,
            prop_for: None,
            prop_for_opacity: None,
            prop_opacity: 100.0,
            prop_width: 2.0,
            prop_size: 14.0,
            toast: None,
            confirm: None,
            allow_close: false,
            canvas: Default::default(),
            organize: Default::default(),
        }
    }

    // ----- documents -------------------------------------------------------

    pub fn load_document(&mut self, doc: Document, path: Option<PathBuf>, name: String) {
        self.doc = Some(doc);
        self.path = path;
        self.name = name;
        self.raster.clear();
        self.current_page = 0;
        self.scroll_to_page = Some(0);
        self.selection.clear();
        self.anchor = None;
        self.search = Search::default();
        self.draft = None;
        self.picked = None;
        self.fit_width = true;
    }

    pub fn open_path(&mut self, path: &Path) {
        match Document::open(path) {
            Ok(doc) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| tr("Untitled.pdf"));
                self.load_document(doc, Some(path.to_path_buf()), name);
            }
            Err(e) => self.notify(trf("Could not open {}: {}", &[&path.display(), &e])),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_title(tr("Open a PDF"))
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    /// Runs `next` now, or after asking about unsaved changes.
    pub fn guard(&mut self, next: Pending) {
        if self.doc.as_ref().is_some_and(Document::is_dirty) {
            self.confirm = Some(next);
        } else {
            self.proceed(next);
        }
    }

    fn proceed(&mut self, next: Pending) {
        match next {
            Pending::Close => self.allow_close = true,
            Pending::OpenDialog => self.open_dialog(),
            Pending::Open(p) => self.open_path(&p),
            Pending::New => self.load_document(Document::blank(), None, tr("Untitled.pdf")),
        }
    }

    /// Saves to the current path, or asks for one. Returns whether it saved.
    pub fn save(&mut self, save_as: bool) -> bool {
        let Some(doc) = self.doc.as_mut() else {
            return false;
        };
        let path = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(&self.name)
                .set_title(tr("Save PDF"))
                .save_file(),
        };
        let Some(path) = path else { return false };
        match doc.save(&path) {
            Ok(()) => {
                self.name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.path = Some(path);
                self.notify(tr("Saved"));
                true
            }
            Err(e) => {
                self.notify(trf("Could not save: {}", &[&e]));
                false
            }
        }
    }

    /// Keeps the main window's size, position and maximized state for next time.
    pub fn remember_window(&mut self, ctx: &egui::Context) {
        let (size, pos, max) = ctx.input(|i| {
            let v = i.viewport();
            (
                v.inner_rect.map(|r| [r.width(), r.height()]),
                v.outer_rect.map(|r| [r.min.x, r.min.y]),
                v.maximized.unwrap_or(false),
            )
        });
        if let Some(s) = size {
            self.settings.window = Some((s, pos, max));
            self.settings.save();
        }
    }

    pub fn notify(&mut self, message: String) {
        self.toast = Some((message, Instant::now()));
    }

    /// Applies an edit; `same_pages` keeps page bitmaps while new ones render.
    pub fn apply(
        &mut self,
        same_pages: bool,
        f: impl FnOnce(&mut Document) -> pdf_engine::Result<()>,
    ) {
        let Some(doc) = self.doc.as_mut() else { return };
        match f(doc) {
            Ok(()) => {
                self.keep_stale = same_pages;
                self.after_edit();
            }
            Err(e) => self.notify(e.to_string()),
        }
    }

    pub fn after_edit(&mut self) {
        let Some(doc) = &self.doc else { return };
        let n = doc.page_count();
        self.current_page = self.current_page.min(n - 1);
        self.selection.retain(|&i| i < n);
        self.picked = None;
        // Search results point at old positions; run it again.
        if !self.search.searched.is_empty() {
            self.search.hits = doc.search(&self.search.searched);
            self.search.current = self
                .search
                .current
                .min(self.search.hits.len().saturating_sub(1));
        }
    }

    pub fn undo(&mut self) {
        self.apply(false, Document::undo);
    }

    pub fn redo(&mut self) {
        self.apply(false, Document::redo);
    }

    /// Pages an organize command acts on: the selection, or the current page.
    pub fn target_pages(&self) -> Vec<usize> {
        if self.toolset == ToolSet::Organize && !self.selection.is_empty() {
            self.selection.iter().copied().collect()
        } else {
            vec![self.current_page]
        }
    }

    pub fn rotate(&mut self, degrees: i64) {
        let pages = self.target_pages();
        self.apply(false, |d| d.rotate_pages(&pages, degrees));
    }

    pub fn delete_pages(&mut self) {
        let pages = self.target_pages();
        self.apply(false, |d| d.delete_pages(&pages));
        self.selection.clear();
    }

    pub fn duplicate(&mut self) {
        let pages = self.target_pages();
        self.apply(false, |d| d.duplicate_pages(&pages));
    }

    pub fn insert_blank(&mut self) {
        let at = self.target_pages().last().map_or(0, |p| p + 1);
        self.apply(false, |d| d.insert_blank_page(at));
        self.selection = [at].into();
        self.current_page = at;
        self.scroll_to_page = Some(at);
    }

    pub fn extract(&mut self) {
        let pages = self.target_pages();
        let Some(doc) = &self.doc else { return };
        let bytes = match doc.extract_pages(&pages) {
            Ok(b) => b,
            Err(e) => return self.notify(e.to_string()),
        };
        let stem = self.name.trim_end_matches(".pdf");
        let suggested = if pages.len() == 1 {
            format!("{stem} (page {}).pdf", pages[0] + 1)
        } else {
            format!("{stem} (pages).pdf")
        };
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(suggested)
            .set_title(tr("Extract pages"))
            .save_file()
        {
            match std::fs::write(&path, bytes) {
                Ok(()) => self.notify(trf("Extracted {} page(s)", &[&pages.len()])),
                Err(e) => self.notify(trf("Could not write the file: {}", &[&e])),
            }
        }
    }

    pub fn merge_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_title(tr("Insert pages from PDF"))
            .pick_file()
        else {
            return;
        };
        match std::fs::read(&path) {
            Ok(bytes) => self.merge_bytes(&bytes),
            Err(e) => self.notify(trf("Could not read the file: {}", &[&e])),
        }
    }

    pub fn merge_bytes(&mut self, bytes: &[u8]) {
        let at = self.target_pages().last().map_or(0, |p| p + 1);
        self.apply(false, |d| d.insert_pdf(bytes, at));
    }

    pub fn move_selection(&mut self, to: usize) {
        let pages: Vec<usize> = self.selection.iter().copied().collect();
        if pages.is_empty() {
            return;
        }
        let before = pages.iter().filter(|&&p| p < to).count();
        let start = to - before;
        self.apply(false, |d| d.move_pages(&pages, to));
        self.selection = (start..start + pages.len()).collect();
    }

    pub fn go_to(&mut self, page: usize) {
        if let Some(doc) = &self.doc {
            let page = page.min(doc.page_count() - 1);
            self.current_page = page;
            self.scroll_to_page = Some(page);
        }
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.1, 8.0);
        self.fit_width = false;
    }

    pub fn tool_color(&self) -> Color {
        match Tool::COMMENT.iter().position(|t| *t == self.tool) {
            Some(i) => self.colors[i],
            None => self.text_color,
        }
    }

    pub fn set_tool_color(&mut self, c: Color) {
        match Tool::COMMENT.iter().position(|t| *t == self.tool) {
            Some(i) => self.colors[i] = c,
            None => self.text_color = c,
        }
    }

    /// Deletes the selected comment or page object.
    pub fn delete_picked(&mut self) {
        match self.picked.take() {
            Some(Picked::Annot { page, slot }) => {
                self.apply(true, |d| d.delete_annotation(page, slot))
            }
            Some(Picked::Item { page, id }) => self.apply(true, |d| d.delete_item(page, id)),
            Some(Picked::Field { page, slot }) => self.apply(true, |d| d.delete_field(page, slot)),
            None => {}
        }
    }

    // ----- search ----------------------------------------------------------

    pub fn run_search(&mut self, forward: bool) {
        let Some(doc) = &self.doc else { return };
        if self.search.query.trim().is_empty() {
            self.search.hits.clear();
            self.search.searched.clear();
            return;
        }
        if self.search.query != self.search.searched {
            self.search.searched = self.search.query.clone();
            self.search.hits = doc.search(&self.search.query);
            // Start from the first match at or after the current page.
            self.search.current = self
                .search
                .hits
                .iter()
                .position(|h| h.page >= self.current_page)
                .unwrap_or(0);
        } else if !self.search.hits.is_empty() {
            let n = self.search.hits.len();
            self.search.current = if forward {
                (self.search.current + 1) % n
            } else {
                (self.search.current + n - 1) % n
            };
        }
        if let Some(hit) = self.search.hits.get(self.search.current) {
            self.canvas.reveal = Some((hit.page, hit.rects.first().copied().unwrap_or_default()));
            self.current_page = hit.page;
        } else {
            self.notify(trf("No matches for “{}”", &[&self.search.query.trim()]));
        }
    }

    // ----- frame -----------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let cmd = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let cmd_shift = |k| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, k);
        let hit = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if hit(cmd(Key::Comma)) {
            self.settings_open = true;
        }
        if hit(cmd(Key::O)) {
            self.guard(Pending::OpenDialog);
        }
        if self.doc.is_none() {
            return;
        }
        if hit(cmd_shift(Key::S)) {
            self.save(true);
        }
        if hit(cmd(Key::S)) {
            self.save(false);
        }
        if hit(cmd(Key::F)) {
            self.search.focus = true;
        }
        if typing {
            return;
        }
        if hit(cmd_shift(Key::Z)) || hit(cmd(Key::Y)) {
            self.redo();
        }
        if hit(cmd(Key::Z)) {
            self.undo();
        }
        if hit(cmd(Key::Plus)) || hit(cmd(Key::Equals)) {
            self.set_zoom(self.zoom * 1.25);
        }
        if hit(cmd(Key::Minus)) {
            self.set_zoom(self.zoom / 1.25);
        }
        if hit(cmd(Key::Num0)) {
            self.fit_width = true;
        }
        if hit(cmd(Key::A)) && self.toolset == ToolSet::Organize {
            let n = self.doc.as_ref().map_or(0, Document::page_count);
            self.selection = (0..n).collect();
        }
        let key = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if key(Key::Delete) || key(Key::Backspace) {
            if self.toolset == ToolSet::Organize && !self.selection.is_empty() {
                self.delete_pages();
            } else {
                self.delete_picked();
            }
        }
        if key(Key::Escape) {
            self.picked = None;
            self.tool = Tool::Select;
        }
        if self.toolset != ToolSet::Organize {
            if key(Key::PageDown) || key(Key::ArrowRight) {
                self.go_to(self.current_page + 1);
            }
            if key(Key::PageUp) || key(Key::ArrowLeft) {
                self.go_to(self.current_page.saturating_sub(1));
            }
            if key(Key::Home) {
                self.go_to(0);
            }
            if key(Key::End) {
                self.go_to(usize::MAX);
            }
        }
    }

    fn title_bar(&mut self, root: &mut egui::Ui) {
        let p = palette(root.visuals().dark_mode);
        egui::Panel::top("title_bar")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(p.chrome)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(root, |ui| {
                theme::row(ui, |ui| {
                    ui.set_height(30.0);
                    let (r, logo) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
                    logo.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Image, true, crate::APP_NAME)
                    });
                    if let Some(t) = crate::icon_texture(ui.ctx()) {
                        let uv =
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                        ui.painter().image(t.id(), r, uv, egui::Color32::WHITE);
                    }
                    ui.add_space(2.0);
                    self.menus(ui);
                });
            });
    }

    /// Document tab and appearance toggle, laid out inside the menu bar row.
    fn title_extras(&mut self, ui: &mut egui::Ui) {
        if self.doc.is_some() {
            ui.add_space(12.0);
            self.document_tab(ui);
        }
        ui.with_layout(theme::far_layout(), |ui| {
            let dark = ui.visuals().dark_mode;
            let (icon, label) = if dark {
                (Icon::Sun, n_("Switch to light appearance"))
            } else {
                (Icon::Moon, n_("Switch to dark appearance"))
            };
            if theme::icon_button(ui, icon, &tr(label), false).clicked() {
                self.theme = if dark {
                    ThemeChoice::Light
                } else {
                    ThemeChoice::Dark
                };
            }
        });
    }

    fn document_tab(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.visuals().dark_mode);
        let dirty = self.doc.as_ref().is_some_and(Document::is_dirty);
        let font = egui::FontId::proportional(13.0);
        let galley = ui
            .painter()
            .layout_no_wrap(crate::i18n::shape(&self.name), font, p.text);
        let width = galley.size().x.min(260.0) + 60.0;
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.0), egui::Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &self.name));
        ui.painter().rect(
            rect,
            6.0,
            p.surface,
            egui::Stroke::new(1.0, p.line),
            egui::StrokeKind::Inside,
        );
        theme::draw_icon(
            ui.painter(),
            egui::pos2(rect.left() + 16.0, rect.center().y),
            Icon::Pdf,
            p.accent,
            16.0,
        );
        let clip = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 30.0, rect.top()),
            egui::pos2(rect.right() - 22.0, rect.bottom()),
        );
        ui.painter().with_clip_rect(clip).galley(
            egui::pos2(clip.left(), rect.center().y - galley.size().y / 2.0),
            galley,
            p.text,
        );
        if dirty {
            ui.painter().circle_filled(
                egui::pos2(rect.right() - 12.0, rect.center().y),
                3.5,
                p.accent,
            );
        }
    }

    fn menus(&mut self, ui: &mut egui::Ui) {
        let has_doc = self.doc.is_some();
        let can_undo = self.doc.as_ref().is_some_and(Document::can_undo);
        let can_redo = self.doc.as_ref().is_some_and(Document::can_redo);
        use theme::{menu, menu_item as item, menu_separator as sep};
        menu(ui, &tr("File"), 240.0, |ui| {
            if item(ui, &tr("Settings…"), Some(&sc(",")), true, None).clicked() {
                self.settings_open = true;
            }
            sep(ui);
            if item(ui, &tr("New blank PDF"), None, true, None).clicked() {
                self.guard(Pending::New);
            }
            if item(ui, &tr("Open…"), Some(&sc("O")), true, None).clicked() {
                self.guard(Pending::OpenDialog);
            }
            sep(ui);
            if item(ui, &tr("Save"), Some(&sc("S")), has_doc, None).clicked() {
                self.save(false);
            }
            if item(ui, &tr("Save As…"), Some(&sc("⇧S")), has_doc, None).clicked() {
                self.save(true);
            }
            sep(ui);
            if item(ui, &tr("Insert pages from PDF…"), None, has_doc, None).clicked() {
                self.merge_dialog();
            }
            if item(ui, &tr("Extract pages…"), None, has_doc, None).clicked() {
                self.extract();
            }
        });
        menu(ui, &tr("Edit"), 220.0, |ui| {
            if item(ui, &tr("Undo"), Some(&sc("Z")), can_undo, None).clicked() {
                self.undo();
            }
            if item(ui, &tr("Redo"), Some(&sc("⇧Z")), can_redo, None).clicked() {
                self.redo();
            }
            sep(ui);
            if item(ui, &tr("Find text"), Some(&sc("F")), has_doc, None).clicked() {
                self.search.focus = true;
            }
            if item(
                ui,
                &tr("Delete selection"),
                None,
                self.picked.is_some(),
                None,
            )
            .clicked()
            {
                self.delete_picked();
            }
        });
        menu(ui, &tr("View"), 240.0, |ui| {
            if item(ui, &tr("Zoom in"), Some(&sc("+")), has_doc, None).clicked() {
                self.set_zoom(self.zoom * 1.25);
            }
            if item(ui, &tr("Zoom out"), Some(&sc("-")), has_doc, None).clicked() {
                self.set_zoom(self.zoom / 1.25);
            }
            if item(
                ui,
                &tr("Fit width"),
                Some(&sc("0")),
                has_doc,
                Some(self.fit_width),
            )
            .clicked()
            {
                self.fit_width = true;
            }
            if item(ui, &tr("Actual size"), None, has_doc, None).clicked() {
                self.set_zoom(1.0);
            }
            sep(ui);
            if item(ui, &tr("Tools pane"), None, true, Some(self.tools_pane)).clicked() {
                self.tools_pane = !self.tools_pane;
            }
            let pages = self.side == Some(SidePanel::Pages);
            if item(ui, &tr("Page thumbnails"), None, true, Some(pages)).clicked() {
                self.side = (!pages).then_some(SidePanel::Pages);
            }
            sep(ui);
            for (choice, label) in [
                (ThemeChoice::System, n_("Match system appearance")),
                (ThemeChoice::Light, n_("Light appearance")),
                (ThemeChoice::Dark, n_("Dark appearance")),
            ] {
                if item(ui, &tr(label), None, true, Some(self.theme == choice)).clicked() {
                    self.theme = choice;
                }
            }
        });
        menu(ui, &tr("Page"), 240.0, |ui| {
            if item(ui, &tr("Rotate clockwise"), None, has_doc, None).clicked() {
                self.rotate(90);
            }
            if item(ui, &tr("Rotate counterclockwise"), None, has_doc, None).clicked() {
                self.rotate(-90);
            }
            sep(ui);
            if item(ui, &tr("Insert blank page"), None, has_doc, None).clicked() {
                self.insert_blank();
            }
            if item(ui, &tr("Duplicate page"), None, has_doc, None).clicked() {
                self.duplicate();
            }
            if item(ui, &tr("Delete page"), None, has_doc, None).clicked() {
                self.delete_pages();
            }
            sep(ui);
            if item(ui, &tr("Organize pages"), None, has_doc, None).clicked() {
                self.toolset = ToolSet::Organize;
            }
        });
        menu(ui, &tr("Help"), 240.0, |ui| {
            if item(ui, &trf("About {}", &[&crate::APP_NAME]), None, true, None).clicked() {
                self.about_open = true;
            }
            if item(ui, &tr("Check for Updates…"), None, true, None).clicked() {
                crate::update::check(ui.ctx(), true, None);
            }
            if item(ui, &tr("Waraq on woosool.com"), None, true, None).clicked() {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab(crate::PRODUCT_PAGE));
            }
        });
        self.title_extras(ui);
    }

    fn toolbar(&mut self, root: &mut egui::Ui) {
        let p = palette(root.visuals().dark_mode);
        egui::Panel::top("toolbar")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(p.chrome)
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 10,
                        top: 0,
                        bottom: 6,
                    }),
            )
            .show(root, |ui| {
                theme::row(ui, |ui| {
                    ui.set_height(34.0);
                    // Tool sets as tabs.
                    for set in ToolSet::ALL {
                        if tab(ui, &tr(set.label()), self.toolset == set).clicked() {
                            self.toolset = set;
                            self.picked = None;
                            self.tool = match set {
                                ToolSet::Comment => self.tool,
                                ToolSet::Forms => Tool::Fill,
                                ToolSet::Redact => Tool::RedactText,
                                _ => Tool::Select,
                            };
                        }
                    }
                    ui.with_layout(theme::far_layout(), |ui| {
                        let has_doc = self.doc.is_some();
                        ui.add_enabled_ui(has_doc, |ui| {
                            if theme::primary_button(ui, &tr("Save")).clicked() {
                                self.save(false);
                            }
                            ui.add_space(4.0);
                            let can_redo = self.doc.as_ref().is_some_and(Document::can_redo);
                            let can_undo = self.doc.as_ref().is_some_and(Document::can_undo);
                            if ui
                                .add_enabled_ui(can_redo, |ui| {
                                    theme::icon_button(ui, Icon::Redo, &tr("Redo"), false)
                                })
                                .inner
                                .clicked()
                            {
                                self.redo();
                            }
                            if ui
                                .add_enabled_ui(can_undo, |ui| {
                                    theme::icon_button(ui, Icon::Undo, &tr("Undo"), false)
                                })
                                .inner
                                .clicked()
                            {
                                self.undo();
                            }
                            theme::divider(ui);
                            self.search_box(ui);
                        });
                    });
                });
            });
    }

    fn search_box(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.visuals().dark_mode);
        let n = self.search.hits.len();
        let searched =
            !self.search.searched.is_empty() && self.search.searched == self.search.query;
        if searched {
            if theme::icon_button(ui, Icon::Down, &tr("Next match"), false).clicked() {
                self.run_search(true);
            }
            if theme::icon_button(ui, Icon::Up, &tr("Previous match"), false).clicked() {
                self.run_search(false);
            }
            let label = if n == 0 {
                tr("No matches")
            } else {
                trf("{} of {}", &[&(self.search.current + 1), &n])
            };
            ui.label(egui::RichText::new(label).color(p.muted).size(12.0));
        }
        let edit = egui::TextEdit::singleline(&mut self.search.query)
            .hint_text(tr("Find text"))
            .desired_width(200.0)
            .min_size(Vec2::new(0.0, 28.0))
            .align(egui::Align2::LEFT_CENTER)
            .margin(egui::Margin {
                left: 28,
                right: 8,
                top: 4,
                bottom: 4,
            });
        let response = ui.add(edit);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("Find text"))
        });
        theme::draw_icon(
            ui.painter(),
            egui::pos2(response.rect.left() + 15.0, response.rect.center().y),
            Icon::Search,
            p.muted,
            15.0,
        );
        if self.search.focus {
            response.request_focus();
            self.search.focus = false;
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let back = ui.input(|i| i.modifiers.shift);
            self.run_search(!back);
            response.request_focus();
        }
        if response.changed() && self.search.query.trim().is_empty() {
            self.search.hits.clear();
            self.search.searched.clear();
        }
    }

    fn tools_pane(&mut self, root: &mut egui::Ui) {
        let p = palette(root.visuals().dark_mode);
        (if crate::i18n::rtl() {
            egui::Panel::right("tools_pane_rtl")
        } else {
            egui::Panel::left("tools_pane")
        })
        .exact_size(212.0)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(p.chrome)
                .inner_margin(egui::Margin {
                    left: 10,
                    right: 10,
                    top: 8,
                    bottom: 8,
                }),
        )
        .show(root, |ui| {
            ui.with_layout(theme::column_layout(), |ui| self.tools_pane_contents(ui));
        });
    }

    fn tools_pane_contents(&mut self, ui: &mut egui::Ui) {
        {
            {
                let has_doc = self.doc.is_some();
                match self.toolset {
                    ToolSet::View => {
                        theme::caption(ui, &tr("Open"));
                        if theme::tool_row(ui, Icon::Open, &tr("Open a PDF…"), false).clicked() {
                            self.guard(Pending::OpenDialog);
                        }
                        ui.add_space(8.0);
                        theme::caption(ui, &tr("Zoom"));
                        ui.add_enabled_ui(has_doc, |ui| {
                            if theme::tool_row(ui, Icon::FitWidth, &tr("Fit width"), self.fit_width)
                                .clicked()
                            {
                                self.fit_width = true;
                            }
                            for z in [0.75, 1.0, 1.5, 2.0] {
                                let label = format!("{:.0}%", z * 100.0);
                                let on = !self.fit_width && (self.zoom - z).abs() < 0.001;
                                if theme::tool_row(ui, Icon::ZoomIn, &label, on).clicked() {
                                    self.set_zoom(z);
                                }
                            }
                        });
                        ui.add_space(8.0);
                        theme::caption(ui, &tr("Tools"));
                        if theme::tool_row(ui, Icon::Organize, &tr("Organize pages"), false)
                            .clicked()
                        {
                            self.toolset = ToolSet::Organize;
                        }
                        if theme::tool_row(ui, Icon::Comments, &tr("Add comments"), false).clicked()
                        {
                            self.toolset = ToolSet::Comment;
                        }
                    }
                    ToolSet::Organize => self.organize_tools(ui),
                    ToolSet::Comment => {
                        crate::editing::selection_panel(self, ui);
                        self.comment_tools(ui);
                    }
                    ToolSet::Edit => {
                        crate::editing::selection_panel(self, ui);
                        crate::editing::tools(self, ui);
                    }
                    ToolSet::Sign => {
                        crate::editing::selection_panel(self, ui);
                        crate::signatures::tools(self, ui);
                    }
                    ToolSet::Forms => {
                        crate::forms::properties(self, ui);
                        crate::forms::tools(self, ui);
                    }
                    ToolSet::Redact => crate::redact::tools(self, ui),
                }
            }
        }
    }

    fn organize_tools(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.visuals().dark_mode);
        let n = self.selection.len();
        theme::caption(ui, &tr("Pages"));
        ui.label(
            egui::RichText::new(if n == 0 {
                tr("Select pages in the grid. Drag to reorder.")
            } else if n == 1 {
                tr("1 page selected")
            } else {
                trf("{} pages selected", &[&n])
            })
            .color(p.muted)
            .size(12.0),
        );
        ui.add_space(4.0);
        let has_doc = self.doc.is_some();
        ui.add_enabled_ui(has_doc, |ui| {
            if theme::tool_row(ui, Icon::RotateCcw, &tr("Rotate left"), false).clicked() {
                self.rotate(-90);
            }
            if theme::tool_row(ui, Icon::RotateCw, &tr("Rotate right"), false).clicked() {
                self.rotate(90);
            }
            if theme::tool_row(ui, Icon::Duplicate, &tr("Duplicate"), false).clicked() {
                self.duplicate();
            }
            if theme::tool_row(ui, Icon::BlankPage, &tr("Insert blank page"), false).clicked() {
                self.insert_blank();
            }
            if theme::tool_row(ui, Icon::Merge, &tr("Insert from PDF…"), false).clicked() {
                self.merge_dialog();
            }
            if theme::tool_row(ui, Icon::Extract, &tr("Extract…"), false).clicked() {
                self.extract();
            }
            ui.add_space(6.0);
            if theme::tool_row(ui, Icon::Trash, &tr("Delete"), false).clicked() {
                self.delete_pages();
            }
        });
    }

    fn comment_tools(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.visuals().dark_mode);
        theme::caption(ui, &tr("Comment tools"));
        for tool in Tool::COMMENT {
            if theme::tool_row(ui, tool.icon(), &tr(tool.label()), self.tool == tool).clicked() {
                self.tool = tool;
                self.picked = None;
            }
        }
        if self.tool == Tool::Select {
            return;
        }
        ui.add_space(10.0);
        theme::caption(ui, &tr("Color"));
        if let Some(c) = crate::editing::swatches(ui, self.tool_color(), &tr("Color")) {
            self.set_tool_color(c);
        }
        if self.tool.uses_stroke() {
            ui.add_space(8.0);
            theme::caption(ui, &tr("Line width"));
            ui.add(egui::Slider::new(&mut self.stroke, 0.5..=8.0).suffix(" pt"))
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Slider, true, tr("Line width"))
                });
        }
        if matches!(self.tool, Tool::Rectangle | Tool::Ellipse) {
            ui.add_space(8.0);
            theme::caption(ui, &tr("Fill"));
            if let Some(f) = crate::editing::fill_swatches(ui, self.shape_fill, &tr("Fill")) {
                self.shape_fill = f;
            }
            theme::caption(ui, &tr("Opacity"));
            let mut o = self.shape_opacity * 100.0;
            ui.add(egui::Slider::new(&mut o, 5.0..=100.0).suffix(" %"))
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, tr("Opacity")));
            self.shape_opacity = o / 100.0;
        }
        if self.tool == Tool::TextBox {
            ui.add_space(8.0);
            theme::caption(ui, &tr("Background"));
            if let Some(f) =
                crate::editing::fill_swatches(ui, self.text_box_fill, &tr("Background"))
            {
                self.text_box_fill = f;
            }
            ui.add_space(8.0);
            theme::caption(ui, &tr("Font size"));
            ui.add(egui::Slider::new(&mut self.font_size, 8.0..=36.0).suffix(" pt"))
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Slider, true, tr("Font size"))
                });
        }
        ui.add_space(10.0);
        theme::para(ui, self.tool.hint(), 12.0, Some(p.muted));
    }

    fn rail(&mut self, root: &mut egui::Ui) {
        let p = palette(root.visuals().dark_mode);
        (if crate::i18n::rtl() {
            egui::Panel::left("rail_rtl")
        } else {
            egui::Panel::right("rail")
        })
        .exact_size(44.0)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(p.chrome)
                .inner_margin(egui::Margin::symmetric(7, 8)),
        )
        .show(root, |ui| {
            for (panel, icon, label) in [
                (SidePanel::Pages, Icon::Pages, n_("Page thumbnails")),
                (SidePanel::Comments, Icon::Comments, n_("Comments")),
            ] {
                let on = self.side == Some(panel);
                if theme::icon_button(ui, icon, &tr(label), on).clicked() {
                    self.side = if on { None } else { Some(panel) };
                }
            }
        });
        if let Some(panel) = self.side
            && self.doc.is_some()
        {
            (if crate::i18n::rtl() {
                egui::Panel::left("side_panel_rtl")
            } else {
                egui::Panel::right("side_panel")
            })
            .exact_size(if panel == SidePanel::Pages {
                176.0
            } else {
                260.0
            })
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(p.chrome)
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(root, |ui| match panel {
                SidePanel::Pages => crate::organize::thumbnail_list(self, ui),
                SidePanel::Comments => self.comments_panel(ui),
            });
        }
    }

    fn comments_panel(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.visuals().dark_mode);
        let Some(doc) = &self.doc else { return };
        let mut items = Vec::new();
        for page in 0..doc.page_count() {
            for a in doc.annotations(page) {
                if a.kind.is_comment() {
                    items.push((page, a));
                }
            }
        }
        theme::row(ui, |ui| {
            ui.label(theme_heading(&tr("Comments")));
            ui.label(egui::RichText::new(items.len().to_string()).color(p.muted));
        });
        ui.add_space(4.0);
        if items.is_empty() {
            ui.label(
                egui::RichText::new(tr(
                    "No comments yet. Choose Comment above to add highlights, notes and drawings.",
                ))
                .color(p.muted)
                .size(12.0),
            );
            return;
        }
        let mut go = None;
        let mut delete = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut last_page = usize::MAX;
                for (page, a) in &items {
                    if *page != last_page {
                        last_page = *page;
                        ui.add_space(4.0);
                        theme::caption(ui, &trf("Page {}", &[&(page + 1)]));
                    }
                    let selected = self.picked
                        == Some(Picked::Annot {
                            page: *page,
                            slot: a.slot,
                        });
                    let frame = egui::Frame::new()
                        .fill(if selected { p.accent_soft } else { p.surface })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if selected { p.accent } else { p.line },
                        ))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(10, 8));
                    let r = frame
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            theme::row(ui, |ui| {
                                let (dot, _) =
                                    ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                                ui.painter().circle_filled(
                                    dot.center(),
                                    5.0,
                                    a.color.map(to_color32).unwrap_or(p.muted),
                                );
                                ui.label(
                                    egui::RichText::new(tr(crate::editing::annot_label(a.kind)))
                                        .strong(),
                                );
                                ui.with_layout(theme::far_layout(), |ui| {
                                    if theme::icon_button_sized(
                                        ui,
                                        Icon::Trash,
                                        &tr("Delete comment"),
                                        false,
                                        22.0,
                                    )
                                    .clicked()
                                    {
                                        delete = Some((*page, a.slot));
                                    }
                                });
                            });
                            if !a.contents.is_empty() {
                                ui.label(
                                    egui::RichText::new(crate::i18n::shape(&a.contents)).size(12.5),
                                );
                            }
                            if !a.author.is_empty() {
                                ui.label(egui::RichText::new(&a.author).color(p.muted).size(11.0));
                            }
                        })
                        .response
                        .interact(egui::Sense::click());
                    if r.clicked() {
                        go = Some((*page, a.slot, a.rect));
                    }
                }
            });
        if let Some((page, slot)) = delete {
            self.apply(true, |d| d.delete_annotation(page, slot));
        } else if let Some((page, slot, rect)) = go {
            self.picked = Some(Picked::Annot { page, slot });
            self.toolset = ToolSet::Comment;
            self.tool = Tool::Select;
            self.canvas.reveal = Some((page, rect));
            self.current_page = page;
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.visuals().dark_mode);
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, p.canvas);
        let card = egui::Rect::from_center_size(rect.center(), Vec2::new(380.0, 240.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.0, p.line))
                .corner_radius(12)
                .inner_margin(24)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 12.0, p.accent_soft);
                        theme::draw_icon(ui.painter(), r.center(), Icon::Pdf, p.accent, 30.0);
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(tr("Open a PDF to get started"))
                                .font(theme::bold(17.0)),
                        );
                        ui.label(
                            egui::RichText::new(tr("Or drop a PDF file anywhere in this window."))
                                .color(p.muted),
                        );
                        ui.add_space(10.0);
                        theme::row(ui, |ui| {
                            ui.add_space((ui.available_width() - 230.0).max(0.0) / 2.0);
                            if theme::primary_button(ui, &tr("Open PDF…")).clicked() {
                                self.guard(Pending::OpenDialog);
                            }
                            if ui.button(tr("New blank PDF")).clicked() {
                                self.guard(Pending::New);
                            }
                        });
                    });
                });
        });
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.confirm.clone() else {
            return;
        };
        let p = palette(ctx.global_style().visuals.dark_mode);
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("confirm_save")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(
                egui::RichText::new(trf("Save changes to “{}”?", &[&self.name]))
                    .font(theme::bold(15.0)),
            );
            ui.label(
                egui::RichText::new(tr("Your changes will be lost if you don’t save them."))
                    .color(p.muted),
            );
            ui.add_space(10.0);
            theme::row(ui, |ui| {
                if ui.button(tr("Don’t save")).clicked() {
                    choice = Some(1);
                }
                ui.with_layout(theme::far_layout(), |ui| {
                    if theme::primary_button(ui, &tr("Save")).clicked() {
                        choice = Some(0);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        choice = Some(2);
                    }
                });
            });
        });
        match choice {
            Some(0) => {
                self.confirm = None;
                if self.save(false) {
                    self.proceed(pending);
                }
            }
            Some(1) => {
                self.confirm = None;
                if let Some(d) = self.doc.as_mut() {
                    d.mark_saved();
                }
                self.proceed(pending);
            }
            Some(2) => self.confirm = None,
            _ => {}
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn toast(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = &self.toast else { return };
        if at.elapsed() > Duration::from_secs(4) {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        let p = palette(ctx.global_style().visuals.dark_mode);
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 92.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(p.text)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(msg.as_str()).color(p.chrome));
                    });
            });
    }

    fn dropped_files(&mut self, ctx: &egui::Context) {
        let files = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = files
            .into_iter()
            .map(|f| f.path().to_path_buf())
            .find(|p| !p.as_os_str().is_empty())
        {
            self.guard(Pending::Open(path));
        }
    }
}

pub fn shortcut_hint() -> String {
    if cfg!(target_os = "macos") {
        tr("⌘↩ to add")
    } else {
        tr("Ctrl+Enter to add")
    }
}

fn sc(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{key}")
    }
}

fn theme_heading(text: &str) -> egui::RichText {
    egui::RichText::new(text).font(theme::bold(14.0))
}

pub fn to_color32(c: Color) -> Color32 {
    Color32::from_rgb(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
    )
}

/// A tool-set tab: text with an accent underline when selected.
fn tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let p = palette(ui.visuals().dark_mode);
    let font = if selected {
        theme::bold(13.0)
    } else {
        egui::FontId::proportional(13.0)
    };
    let galley = ui.painter().layout_no_wrap(label.into(), font, p.text);
    let size = Vec2::new(galley.size().x + 24.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, label));
    if response.hovered() && !selected {
        ui.painter()
            .rect_filled(rect.shrink2(Vec2::new(2.0, 2.0)), 6.0, p.control_hover);
    }
    let color = if selected { p.text } else { p.muted };
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
    if selected {
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 10.0, rect.bottom() - 3.0),
            egui::pos2(rect.right() - 10.0, rect.bottom() - 1.0),
        );
        ui.painter().rect_filled(bar, 1.0, p.accent);
    }
    response
}

impl eframe::App for App {
    /// Clear to transparent while the splash card shows (its window has no background).
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.splash.is_some() {
            [0.0; 4]
        } else {
            visuals.panel_fill.to_normalized_gamma_f32()
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::i18n::set(self.settings.lang);
        if self.settings.theme != self.theme {
            self.settings.theme = self.theme;
            self.settings.save();
        }
        ctx.set_theme(match self.theme {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        });
        if self.splash.is_some() {
            ui.set_style(ctx.global_style());
            if crate::splash::show(self, ui) {
                return;
            }
        } else {
            self.auto_check_updates(&ctx);
        }
        // The root Ui was created before the theme switch; restyle it.
        ui.set_style(ctx.global_style());

        if ctx.input(|i| i.viewport().close_requested()) && self.splash.is_none() {
            self.remember_window(&ctx);
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.doc.as_ref().is_some_and(Document::is_dirty)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Pending::Close);
        }

        self.dropped_files(&ctx);
        if self.confirm.is_none() {
            self.shortcuts(&ctx);
        }
        if let Some(doc) = &self.doc {
            self.raster
                .set_document(doc.revision(), doc.bytes(), self.keep_stale);
        }
        self.raster.poll(&ctx);

        let p = palette(ui.visuals().dark_mode);
        self.title_bar(ui);
        self.toolbar(ui);
        if self.tools_pane {
            self.tools_pane(ui);
        }
        self.rail(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.canvas))
            .show(ui, |ui| {
                // Hairline separating the chrome from the work area.
                let r = ui.max_rect();
                ui.painter()
                    .hline(r.x_range(), r.top(), egui::Stroke::new(1.0, p.line));
                if self.doc.is_none() {
                    self.welcome(ui);
                } else if self.toolset == ToolSet::Organize {
                    crate::organize::grid(self, ui);
                } else {
                    crate::canvas::show(self, ui);
                }
            });
        crate::signatures::dialog(self, &ctx);
        crate::redact::dialogs(self, &ctx);
        crate::about::about(self, &ctx);
        crate::about::settings(self, &ctx);
        crate::update::dialog(self, &ctx);
        self.confirm_dialog(&ctx);
        self.toast(&ctx);
    }
}
