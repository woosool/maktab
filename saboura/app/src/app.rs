//! The app's state: the open board, view, tool, selection and undo, and the actions on them
//! (open, save and autosave, clipboard, arrange, group, images, tables, mind maps).

use crate::i18n::tr;
use qalam_kit::colors::*;
use saboura_engine::geometry::{self, Box2, P};
use saboura_engine::{Board, History, Item, View, edit};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool { Select, Hand, Note, Text, Shape, Arrow, Pen, Frame, Comment }

impl Tool {
    pub fn kind(self) -> &'static str {
        match self { Tool::Note => "note", Tool::Text => "text", Tool::Shape => "rect", Tool::Arrow => "arrow", Tool::Pen => "pen", Tool::Frame => "frame", _ => "" }
    }
}

/// What a press on the board is doing while the pointer is held.
pub enum Drag {
    Pan,
    /// Moving objects: their state when the drag began; `moved` once undo has a step.
    Move { start: P, orig: Vec<Item>, moved: bool },
    Resize { handle: &'static str, start: P, from: Box2, orig: Vec<Item> },
    Rotate { center: P, start_angle: f64, orig: Vec<Item> },
    Marquee { start: P, now: P, add: bool },
    /// A shape, note or frame drawn by dragging out its box.
    Create { id: String, start: P },
    Draw { id: String },
    /// A new connection line, or one end of a selected line (`end`: its far end).
    Connect { id: String },
    Endpoint { id: String, end: bool },
    /// Dragging from a quick-connect port: a line from the object, or a copy on release nearby.
    Port { from: String, dir: &'static str, line: String, start: P },
    /// A table's column border: the column edges when the drag began.
    Column { id: String, index: usize, edges: Vec<f64> },
}

/// Settings kept between runs.
#[derive(Clone, Default)]
pub struct Prefs { pub auto_update: bool, pub skipped_version: Option<String>, pub last_update_check: u64, pub dark: bool, pub grid: bool, pub minimap: bool, pub snap: bool, pub lang: String, pub author: String, pub recent: Vec<PathBuf> }

impl Prefs {
    fn file() -> PathBuf { crate::config_dir().join("prefs.json") }
    pub fn load() -> Prefs {
        if cfg!(test) { return Prefs { grid: true, snap: true, author: "Me".into(), ..Prefs::default() }; }
        let v: serde_json::Value = std::fs::read_to_string(Prefs::file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Prefs {
            dark: v["dark"].as_bool().unwrap_or(false),
            auto_update: v["auto_update"].as_bool().unwrap_or(true),
            skipped_version: v["skipped_version"].as_str().map(String::from),
            last_update_check: v["last_update_check"].as_u64().unwrap_or(0),
            grid: v["grid"].as_bool().unwrap_or(true),
            minimap: v["minimap"].as_bool().unwrap_or(false),
            snap: v["snap"].as_bool().unwrap_or(true),
            lang: v["lang"].as_str().unwrap_or("en").to_string(),
            author: v["author"].as_str().map(str::to_string).or_else(|| std::env::var("USER").or_else(|_| std::env::var("USERNAME")).ok()).unwrap_or_else(|| "Me".into()),
            recent: v["recent"].as_array().map(|a| a.iter().filter_map(|p| p.as_str()).map(PathBuf::from).filter(|p| p.exists()).collect()).unwrap_or_default(),
        }
    }
    pub fn save(&self) {
        if cfg!(test) { return; }
        let v = serde_json::json!({ "auto_update": self.auto_update, "skipped_version": self.skipped_version, "last_update_check": self.last_update_check, "dark": self.dark, "grid": self.grid, "minimap": self.minimap, "snap": self.snap, "lang": self.lang, "author": self.author,
            "recent": self.recent.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>() });
        let _ = std::fs::create_dir_all(crate::config_dir());
        let _ = std::fs::write(Prefs::file(), v.to_string());
    }
}

/// Dialogs and popovers that can be open.
#[derive(Clone, PartialEq)]
pub enum Panel { None, Shapes, Library, Palette(String), Home, Link(String), Table(usize, usize), Image(ImageOptions), Tags, Shortcuts, About, Recover, Templates(String, String), Icons(String, String, usize) }

/// The side panel: none, the board's frames, or its comments.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side { None, Frames, Comments, Votes }

/// An image's alternative text and crop (fractions of the picture), being edited.
#[derive(Clone, PartialEq, Debug)]
pub struct ImageOptions { pub alt: String, pub crop: [f64; 4], pub ellipse: bool }

impl ImageOptions {
    pub fn of(i: &Item) -> ImageOptions {
        let c = i.get("crop");
        let f = |k: &str, d: f64| c.and_then(|c| c[k].as_f64()).unwrap_or(d);
        ImageOptions { alt: i.str("alt").unwrap_or("").to_string(), crop: [f("x", 0.0), f("y", 0.0), f("w", 1.0), f("h", 1.0)], ellipse: c.and_then(|c| c["mask"].as_str()) == Some("ellipse") }
    }
}

pub struct BoardApp {
    pub ctx: egui::Context,
    pub board: Board,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// When the board last changed (seconds, egui time), for autosave.
    pub changed_at: f64,
    pub view: View,
    pub fit_pending: bool,
    pub tool: Tool,
    /// The shape the Shape tool makes.
    pub shape: &'static str,
    pub selection: Vec<String>,
    pub history: History,
    pub drag: Option<Drag>,
    pub editing: Option<String>,
    /// The table cell being typed into, with `editing`.
    pub cell: Option<(usize, usize)>,
    /// An object a line being drawn would attach to.
    pub target: Option<String>,
    pub pen_color: String,
    pub prefs: Prefs,
    pub panel: Panel,
    pub clipboard: Vec<Item>,
    pub notice: Option<(String, f64)>,
    pub canvas_rect: egui::Rect,
    pub confirm_discard: Option<Pending>,
    pub textures: HashMap<String, (usize, Option<egui::TextureHandle>)>,
    pub side: Side,
    /// Find on board: the words and which match is shown.
    pub search: Option<(String, usize)>,
    /// Presenting frames: the frame shown.
    pub presenting: Option<usize>,
    pub timer_shown: bool,
    /// Alignment guides while moving objects.
    pub guides: Vec<saboura_engine::arrange::Guide>,
    /// The splash screen: when it started.
    pub splash: Option<f64>,
    /// A board found in the recovery file at start.
    pub recovered: Option<Board>,
    pub recovery_at: f64,
    /// A comment thread being replied to, with the reply's text.
    pub reply: Option<(String, String)>,
    /// The characters selected in the text being typed (char indices), for styling words.
    pub text_sel: Option<(usize, usize)>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub native: Option<crate::commands::native::Native>,
    pub menu_tried: bool,
}

/// An action waiting for the person to decide about unsaved changes.
#[derive(Clone)]
pub enum Pending { New(&'static str), Open(PathBuf), OpenDialog, Quit, Template(Box<Board>) }

const CLIP_MARK: &str = "saboura-items:";

impl BoardApp {
    pub fn new(cc: &eframe::CreationContext, path: Option<PathBuf>) -> BoardApp {
        qalam_kit::fonts::install_fonts(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx);
        qalam_kit::open_files::set_context(&cc.egui_ctx);
        let app_prefs = Prefs::load();
        let mut app = BoardApp::blank(cc.egui_ctx.clone(), app_prefs);
        DARK.with(|d| d.set(app.prefs.dark));
        crate::theme::apply_mode(&app.ctx, app.prefs.dark);
        crate::i18n::set(crate::i18n::Lang::from_code(&app.prefs.lang));
        match path { Some(p) => app.open_path(p), None => if !app.prefs.recent.is_empty() { app.panel = Panel::Home; } }
        if !cfg!(test) {
            app.splash = Some(0.0);
            // A board that wasn't saved when the app last closed can be restored.
            app.recovered = std::fs::read_to_string(recovery_file()).ok().and_then(|t| Board::from_json(&t).ok());
            if app.recovered.is_some() { app.panel = Panel::Recover; }
        }
        app
    }

    pub fn blank(ctx: egui::Context, prefs: Prefs) -> BoardApp {
        BoardApp {
            ctx, board: Board::new(&tr("Untitled board"), "blank"), path: None, dirty: false, changed_at: 0.0,
            view: View::default(), fit_pending: true, tool: Tool::Select, shape: "rectangle", selection: Vec::new(),
            history: History::default(), drag: None, editing: None, cell: None, target: None,
            pen_color: saboura_engine::model::PEN_COLORS[0].into(), prefs, panel: Panel::None,
            clipboard: Vec::new(), notice: None, canvas_rect: egui::Rect::NOTHING, confirm_discard: None, textures: HashMap::new(),
            side: Side::None, search: None, presenting: None, timer_shown: false, guides: Vec::new(), splash: None, recovered: None, recovery_at: 0.0, reply: None, text_sel: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            native: None, menu_tried: false,
        }
    }

    pub fn notify(&mut self, text: &str) { self.notice = Some((text.to_string(), self.ctx.input(|i| i.time))); }

    /// Records the board for undo before a change.
    pub fn checkpoint(&mut self) { self.history.push(&self.board.items); }
    pub fn changed(&mut self) {
        self.dirty = true;
        self.board.updated = saboura_engine::model::now_ms();
        self.changed_at = self.ctx.input(|i| i.time);
    }

    pub fn undo(&mut self) { self.stop_editing(); if self.history.undo(&mut self.board.items) { self.after_history(); } }
    pub fn redo(&mut self) { self.stop_editing(); if self.history.redo(&mut self.board.items) { self.after_history(); } }
    fn after_history(&mut self) {
        let ids: HashSet<&str> = self.board.items.iter().map(|i| i.id.as_str()).collect();
        self.selection.retain(|s| ids.contains(s.as_str()));
        self.changed();
    }

    pub fn stop_editing(&mut self) {
        let Some(id) = self.editing.take() else { return };
        self.cell = None;
        self.text_sel = None;
        // An empty text box is removed when typing ends.
        if self.board.find(&id).is_some_and(|i| i.kind == "text" && i.text.trim().is_empty()) {
            self.board.remove(&[id].into_iter().collect());
            self.selection.clear();
        }
    }

    pub fn is_selected(&self, id: &str) -> bool { self.selection.iter().any(|s| s == id) }
    pub fn selected_items(&self) -> Vec<&Item> { self.board.items.iter().filter(|i| self.is_selected(&i.id)).collect() }
    pub fn single(&self) -> Option<&Item> { if self.selection.len() == 1 { self.board.find(&self.selection[0]) } else { None } }

    /// Selects an object, with the rest of its group.
    pub fn select(&mut self, id: &str, add: bool) {
        if !add { self.selection.clear(); }
        let group = self.board.find(id).and_then(|i| i.group_id.clone());
        let ids: Vec<String> = match group {
            Some(g) => self.board.items.iter().filter(|i| i.group_id.as_deref() == Some(&g)).map(|i| i.id.clone()).collect(),
            None => vec![id.to_string()],
        };
        for id in ids { if !self.is_selected(&id) { self.selection.push(id); } }
    }

    pub fn select_all(&mut self) { self.selection = self.board.items.iter().map(|i| i.id.clone()).collect(); }

    /// IDs with every mind-map branch under them added.
    fn with_branches(&self, mut ids: HashSet<String>) -> HashSet<String> {
        loop {
            let more: Vec<String> = self.board.items.iter().filter(|i| i.mind_parent().is_some_and(|p| ids.contains(p)) && !ids.contains(&i.id)).map(|i| i.id.clone()).collect();
            if more.is_empty() { return ids; }
            ids.extend(more);
        }
    }

    /// Objects that move with the selection: the selection, what lies in selected frames, and
    /// the branches of selected mind-map nodes.
    pub fn moving_set(&self) -> Vec<Item> {
        let mut ids: HashSet<String> = self.selection.iter().cloned().collect();
        for f in self.selected_items().into_iter().filter(|i| i.kind == "frame") {
            let fb = geometry::object_bounds(f);
            for i in &self.board.items { if i.id != f.id && geometry::object_bounds(i).within(&fb) { ids.insert(i.id.clone()); } }
        }
        let ids = self.with_branches(ids);
        self.board.items.iter()
            .filter(|i| ids.contains(&i.id) && !i.is_locked() && !(i.is_connector() && (i.start_id.is_some() || i.end_id.is_some())))
            .cloned().collect()
    }

    /// The box the resize handles go around, or none for a lone line.
    pub fn resize_box(&self) -> Option<Box2> {
        let chosen: Vec<&Item> = self.selected_items().into_iter().filter(|i| !(i.is_connector() && (i.start_id.is_some() || i.end_id.is_some()))).collect();
        if chosen.is_empty() || chosen.len() == 1 && chosen[0].is_connector() || chosen.iter().any(|i| i.is_locked()) { return None; }
        if chosen.len() == 1 && chosen[0].rotation() != 0.0 { let i = chosen[0]; return Some(Box2 { x: i.x, y: i.y, w: i.w, h: i.h }); }
        let b = geometry::bounds(chosen);
        (b.w > 0.0 || b.h > 0.0).then_some(b)
    }

    pub fn add_item(&mut self, item: Item) -> String {
        let id = item.id.clone();
        self.board.items.push(item);
        self.changed();
        id
    }

    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() { return; }
        self.checkpoint();
        let chosen: HashSet<String> = self.selection.drain(..).collect();
        let ids = self.with_branches(chosen);
        self.board.remove(&ids);
        self.editing = None;
        self.changed();
    }

    /// Copies of objects with new IDs, offset, links between them kept.
    fn copies(items: &[Item], dx: f64, dy: f64) -> Vec<Item> {
        let ids: HashMap<String, String> = items.iter().map(|i| (i.id.clone(), saboura_engine::model::uid())).collect();
        let groups: HashMap<String, String> = items.iter().filter_map(|i| i.group_id.clone()).map(|g| (g, saboura_engine::model::uid())).collect();
        items.iter().map(|i| {
            let mut c = i.clone();
            c.id = ids[&i.id].clone();
            c.x += dx; c.y += dy;
            c.group_id = i.group_id.as_ref().map(|g| groups[g].clone());
            c.start_id = i.start_id.as_ref().and_then(|s| ids.get(s).cloned());
            c.end_id = i.end_id.as_ref().and_then(|s| ids.get(s).cloned());
            if let Some(p) = i.mind_parent() {
                match ids.get(p) {
                    Some(np) => { let mut m = i.get("mindMap").cloned().unwrap_or_default(); m["parent"] = np.clone().into(); c.set("mindMap", m); }
                    None => { c.extra.remove("mindMap"); }
                }
            }
            c
        }).collect()
    }

    fn place(&mut self, items: Vec<Item>) {
        if items.is_empty() { return; }
        self.checkpoint();
        self.selection = items.iter().map(|i| i.id.clone()).collect();
        self.board.items.extend(items);
        self.board.update_connectors();
        self.changed();
    }

    pub fn duplicate(&mut self) {
        let items: Vec<Item> = self.selected_items().into_iter().cloned().collect();
        self.place(Self::copies(&items, 24.0, 24.0));
    }

    pub fn copy(&mut self) {
        self.clipboard = self.selected_items().into_iter().cloned().collect();
        if !self.clipboard.is_empty() {
            let json = serde_json::to_string(&self.clipboard).unwrap_or_default();
            self.ctx.copy_text(format!("{CLIP_MARK}{json}"));
        }
    }
    pub fn cut(&mut self) { self.copy(); self.delete_selection(); }

    /// Pastes copied objects at `at`; other text becomes a note.
    pub fn paste(&mut self, text: Option<String>, at: P) {
        let items = match text.as_deref().and_then(|t| t.strip_prefix(CLIP_MARK)) {
            Some(json) => serde_json::from_str::<Vec<Item>>(json).unwrap_or_default(),
            None if text.as_deref().is_some_and(|t| !t.trim().is_empty()) => {
                let t = text.unwrap_or_default();
                vec![Item::new("note", at.0 - 95.0, at.1 - 80.0).with(|i| i.text = t.chars().take(20000).collect())]
            }
            None => self.clipboard.clone(),
        };
        if items.is_empty() { return; }
        let b = geometry::bounds(&items);
        self.place(Self::copies(&items, at.0 - b.x - b.w / 2.0, at.1 - b.y - b.h / 2.0));
    }

    pub fn bring_to_front(&mut self, front: bool) {
        if self.selection.is_empty() { return; }
        self.checkpoint();
        let (mut chosen, rest): (Vec<Item>, Vec<Item>) = std::mem::take(&mut self.board.items).into_iter().partition(|i| self.is_selected(&i.id));
        self.board.items = if front { let mut r = rest; r.append(&mut chosen); r } else { chosen.extend(rest); chosen };
        self.changed();
    }

    pub fn group(&mut self, on: bool) {
        if on && self.selection.len() < 2 || self.selection.is_empty() { return; }
        let g = on.then(saboura_engine::model::uid);
        self.edit_selected(|i| i.group_id = g.clone());
    }

    pub fn toggle_lock(&mut self) {
        let lock = !self.selected_items().iter().all(|i| i.is_locked());
        self.edit_selected(|i| i.locked = lock.then_some(true));
    }

    /// Changes every selected object, in one undo step.
    pub fn edit_selected(&mut self, f: impl Fn(&mut Item)) {
        if self.selection.is_empty() { return; }
        self.checkpoint();
        let sel = self.selection.clone();
        for i in self.board.items.iter_mut().filter(|i| sel.contains(&i.id)) { f(i); }
        self.board.update_connectors();
        self.changed();
    }

    pub fn nudge(&mut self, dx: f64, dy: f64) {
        let ids: HashSet<String> = self.moving_set().into_iter().map(|i| i.id).collect();
        if ids.is_empty() { return; }
        self.checkpoint();
        for i in self.board.items.iter_mut().filter(|i| ids.contains(&i.id)) { i.x += dx; i.y += dy; }
        self.board.update_connectors();
        self.changed();
    }

    pub fn quick_connect(&mut self, id: &str, dir: &str) {
        self.checkpoint();
        if let Some(n) = edit::quick_connect(&mut self.board, id, dir, "elbow") {
            self.selection = vec![n.clone()];
            self.editing = Some(n);
            self.changed();
        }
    }

    /// Adds a mind-map branch (Tab) or sibling (Enter on a node) and starts typing in it.
    pub fn add_branch(&mut self, sibling: bool) {
        let Some(id) = self.single().map(|i| i.id.clone()) else { return };
        self.stop_editing();
        self.checkpoint();
        if let Some(n) = edit::add_branch(&mut self.board, &id, sibling) {
            self.selection = vec![n.clone()];
            self.editing = Some(n);
            self.changed();
        }
    }

    pub fn new_mind_map(&mut self) {
        let c = self.view_center();
        self.checkpoint();
        let root = Item::new("rect", c.0 - 110.0, c.1 - 45.0).with(|i| {
            i.w = 220.0; i.h = 90.0; i.text = tr("Central idea"); i.font_size = Some(24.0); i.text_align = Some("center".into());
            i.auto_font_size = Some(true); i.color = Some(saboura_engine::model::COLORS[1].into());
            i.set("mindMap", serde_json::json!({ "parent": null, "direction": "right", "auto": true, "line": "curved" }));
        });
        let id = self.add_item(root);
        self.selection = vec![id];
        self.tool = Tool::Select;
    }

    pub fn insert_table(&mut self, rows: usize, cols: usize) {
        let c = self.view_center();
        self.checkpoint();
        let mut t = edit::new_table(0.0, 0.0, rows, cols);
        t.x = c.0 - t.w / 2.0; t.y = c.1 - t.h / 2.0;
        let id = self.add_item(t);
        self.selection = vec![id];
        self.tool = Tool::Select;
    }

    pub fn insert_image_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter(tr("Images"), &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file() else { return };
        match std::fs::read(&path) { Ok(bytes) => { let c = self.view_center(); self.insert_image(&bytes, c); } Err(e) => self.notify(&e.to_string()) }
    }

    /// A picture as a data URL for the file format (PNG, JPEG and WebP are kept as they are;
    /// other formats become PNG), with its size at most 480 board units across.
    pub fn encode_image(bytes: &[u8]) -> Result<(String, f64, f64), String> {
        let img = image::load_from_memory(bytes).map_err(|_| tr("This image can't be read."))?;
        if img.width() as u64 * img.height() as u64 > 40_000_000 { return Err(tr("Choose an image with fewer than 40 million pixels.")); }
        let mime = match image::guess_format(bytes) { Ok(image::ImageFormat::Png) => Some("png"), Ok(image::ImageFormat::Jpeg) => Some("jpeg"), Ok(image::ImageFormat::WebP) => Some("webp"), _ => None };
        let data = match mime {
            Some(m) => (m, bytes.to_vec()),
            None => { let mut out = std::io::Cursor::new(Vec::new()); img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?; ("png", out.into_inner()) }
        };
        use base64::Engine;
        let src = format!("data:image/{};base64,{}", data.0, base64::engine::general_purpose::STANDARD.encode(&data.1));
        let s = (480.0 / img.width().max(img.height()) as f64).min(1.0);
        Ok((src, img.width() as f64 * s, img.height() as f64 * s))
    }

    /// Adds a picture centred on `at`.
    pub fn insert_image(&mut self, bytes: &[u8], at: P) {
        match Self::encode_image(bytes) {
            Ok((src, w, h)) => {
                self.checkpoint();
                let id = self.add_item(Item::new("image", at.0 - w / 2.0, at.1 - h / 2.0).with(|i| { i.w = w; i.h = h; i.color = None; i.auto_font_size = None; i.set("src", src); }));
                self.selection = vec![id];
                self.tool = Tool::Select;
            }
            Err(e) => self.notify(&e),
        }
    }

    /// The texture of an image object (by ID), decoded once per picture.
    pub fn texture_of(&mut self, id: &str) -> Option<egui::TextureHandle> {
        let item = self.board.find(id)?;
        let src = item.str("src")?;
        if let Some((len, tex)) = self.textures.get(id) && *len == src.len() { return tex.clone(); }
        use base64::Engine;
        let tex = src.split_once(',').and_then(|(_, b)| base64::engine::general_purpose::STANDARD.decode(b).ok())
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
            .map(|img| {
                // At most 2048 px across on screen: a photo's full size would only use memory.
                let img = if img.width().max(img.height()) > 2048 { img.thumbnail(2048, 2048) } else { img };
                let rgba = img.to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba);
                self.ctx.load_texture(format!("img-{id}"), ci, egui::TextureOptions::LINEAR)
            });
        let len = src.len();
        self.textures.insert(id.to_string(), (len, tex.clone()));
        tex
    }

    /// Frees the textures of pictures no longer on the board.
    pub fn prune_textures(&mut self) {
        if self.textures.len() <= 8 { return; }
        let ids: HashSet<&str> = self.board.items.iter().filter(|i| i.kind == "image").map(|i| i.id.as_str()).collect();
        self.textures.retain(|k, _| k.starts_with("icon:") || ids.contains(k.as_str()));
    }

    pub fn fit(&mut self) {
        let size = (self.canvas_rect.width() as f64, self.canvas_rect.height() as f64);
        if !(size.0 > 0.0) { self.fit_pending = true; return; }
        self.view = View::fit(geometry::bounds(&self.board.items), size, 80.0);
    }

    pub fn zoom_by(&mut self, larger: bool) {
        let c = (self.canvas_rect.width() as f64 / 2.0, self.canvas_rect.height() as f64 / 2.0);
        self.view = self.view.zoom_at(c, geometry::zoom_step(self.view.z, larger));
    }
    pub fn zoom_reset(&mut self) {
        let c = (self.canvas_rect.width() as f64 / 2.0, self.canvas_rect.height() as f64 / 2.0);
        self.view = self.view.zoom_at(c, 1.0);
    }

    /// The board point at the middle of the view.
    pub fn view_center(&self) -> P { self.view.to_board((self.canvas_rect.width() as f64 / 2.0, self.canvas_rect.height() as f64 / 2.0)) }

    /// What survives opening another board: the menu bar and textures context.
    fn keep_from(&mut self, old: BoardApp) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        { self.native = old.native; }
        self.menu_tried = old.menu_tried;
        self.side = old.side;
    }

    pub fn toggle_side(&mut self, side: Side) { self.side = if self.side == side { Side::None } else { side }; }

    /// Lines up the selected objects.
    pub fn align(&mut self, how: &str) {
        if self.selection.len() < 2 { return; }
        self.checkpoint();
        let sel = self.selection.clone();
        let mut chosen: Vec<&mut Item> = self.board.items.iter_mut().filter(|i| sel.contains(&i.id) && !i.is_connector()).collect();
        saboura_engine::arrange::align(&mut chosen, how);
        self.board.update_connectors();
        self.changed();
    }

    pub fn distribute(&mut self, horizontal: bool) {
        if self.selection.len() < 3 { return; }
        self.checkpoint();
        let sel = self.selection.clone();
        let mut chosen: Vec<&mut Item> = self.board.items.iter_mut().filter(|i| sel.contains(&i.id) && !i.is_connector()).collect();
        saboura_engine::arrange::distribute(&mut chosen, horizontal);
        self.board.update_connectors();
        self.changed();
    }

    /// Writes unsaved changes to the recovery file every few seconds.
    fn write_recovery(&mut self) {
        if cfg!(test) || !self.dirty { return; }
        let now = self.ctx.input(|i| i.time);
        if now - self.recovery_at < 5.0 { return; }
        self.recovery_at = now;
        let _ = std::fs::create_dir_all(crate::config_dir());
        let _ = std::fs::write(recovery_file(), self.board.to_json());
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.prefs.dark = dark;
        // The light theme is built from the chrome colours, which read this flag: set it first,
        // or switching back to light builds a light theme out of dark colours.
        DARK.with(|d| d.set(dark));
        crate::theme::apply_mode(&self.ctx, dark);
        self.prefs.save();
    }

    // ---- Files ----

    pub fn title(&self) -> String { format!("{}{} — {}", if self.dirty { "• " } else { "" }, self.board.title, crate::app_name()) }

    /// Runs `p` now, or after asking about unsaved changes.
    pub fn guard(&mut self, p: Pending) {
        if self.dirty { self.confirm_discard = Some(p); } else { self.run_pending(p); }
    }

    pub fn run_pending(&mut self, p: Pending) {
        match p {
            Pending::New(template) => self.new_board(template),
            Pending::Template(board) => { self.new_board("blank"); self.board = *board; self.fit_pending = true; }
            Pending::Open(path) => self.open_path(path),
            Pending::OpenDialog => if let Some(path) = rfd::FileDialog::new().add_filter(tr("Saboura board"), &["sab", "json"]).add_filter(tr("Saboura board (.sab)"), &["sab"]).add_filter(tr("Whiteboard JSON"), &["json"]).pick_file() { self.open_path(path); },
            Pending::Quit => { self.dirty = false; let _ = std::fs::remove_file(recovery_file()); self.ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        }
    }

    pub fn new_board(&mut self, template: &str) {
        let title = tr("Untitled board");
        let old = std::mem::replace(self, BoardApp::blank(self.ctx.clone(), Prefs::default()));
        *self = BoardApp { board: Board::new(&title, template), ..BoardApp::blank(old.ctx.clone(), old.prefs.clone()) };
        self.keep_from(old);
    }

    pub fn open_path(&mut self, path: PathBuf) {
        // A .sab archive (pictures inside) or the JS app's JSON, told apart by their bytes.
        let read = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| saboura_engine::sab::open(&b));
        match read {
            Ok(mut board) => {
                board.update_connectors();
                let prefs = self.prefs.clone();
                let old = std::mem::replace(self, BoardApp::blank(self.ctx.clone(), Prefs::default()));
        *self = BoardApp { board, path: Some(path.clone()), ..BoardApp::blank(old.ctx.clone(), prefs) };
        self.keep_from(old);
                self.remember(path);
            }
            Err(e) => self.notify(&format!("{}: {}", tr("Couldn't open the board"), tr(&e))),
        }
    }

    fn remember(&mut self, path: PathBuf) {
        self.prefs.recent.retain(|p| *p != path);
        self.prefs.recent.insert(0, path);
        self.prefs.recent.truncate(12);
        self.prefs.save();
    }

    /// Saves to the board's file, asking where for a new board or with `save_as`.
    pub fn save(&mut self, save_as: bool) -> bool { self.save_with(save_as, true) }

    /// Saves as `.sab` (pictures inside, gzip-compressed), or as JSON when the file is named
    /// `.json`. A preview picture goes in when asked (not on every autosave).
    fn save_with(&mut self, save_as: bool, preview: bool) -> bool {
        let path = match (&self.path, save_as) {
            (Some(p), false) => p.clone(),
            _ => {
                let name: String = self.board.title.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
                match rfd::FileDialog::new().add_filter(tr("Saboura board (.sab)"), &["sab"]).add_filter(tr("Whiteboard JSON"), &["json"]).set_file_name(format!("{name}.sab")).save_file() {
                    Some(p) => p,
                    None => return false,
                }
            }
        };
        let json = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
        let bytes = if json { Ok(self.board.to_json().into_bytes()) } else {
            let thumb = preview.then(|| crate::export::thumbnail(&self.board.items)).flatten();
            saboura_engine::sab::write(&self.board, thumb.as_deref())
        };
        // Written beside the file first, then moved over it, so a failed save never leaves half a board.
        let tmp = path.with_extension("saving");
        let written = bytes.and_then(|b| std::fs::write(&tmp, b).map_err(|e| e.to_string())).and_then(|_| std::fs::rename(&tmp, &path).map_err(|e| e.to_string()));
        if written.is_err() { let _ = std::fs::remove_file(&tmp); }
        match written {
            Ok(()) => { self.path = Some(path.clone()); self.dirty = false; self.remember(path); let _ = std::fs::remove_file(recovery_file()); true }
            Err(e) => { self.notify(&format!("{}: {e}", tr("Couldn't save"))); false }
        }
    }

    /// A board that has a file saves itself a second after the last change.
    fn autosave(&mut self) {
        if !self.dirty || self.path.is_none() || self.drag.is_some() || self.editing.is_some() { return; }
        let now = self.ctx.input(|i| i.time);
        if now - self.changed_at >= 1.0 { self.save_with(false, false); } else { self.ctx.request_repaint_after(std::time::Duration::from_millis(1100)); }
    }

    pub fn open_sent_files(&mut self) {
        if !qalam_kit::open_files::pending() { return; }
        let (here, rest) = qalam_kit::open_files::route(qalam_kit::open_files::take());
        qalam_kit::open_files::open_elsewhere(&rest);
        if let Some(p) = here { self.guard(Pending::Open(p)); }
    }

    fn discard_dialog(&mut self) {
        let Some(p) = self.confirm_discard.clone() else { return };
        let mut open = true;
        let mut choice = None;
        crate::theme::dialog(&self.ctx, tr("Unsaved changes")).open(&mut open).show(&self.ctx.clone(), |ui| {
            ui.label(tr("Save the changes to this board first?"));
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, tr("Save")).clicked() { choice = Some(0); }
                if crate::theme::secondary(ui, &tr("Don't save")).clicked() { choice = Some(1); }
                if crate::theme::secondary(ui, &tr("Cancel")).clicked() { choice = Some(2); }
            });
        });
        match choice {
            Some(0) => { self.confirm_discard = None; if self.save(false) { self.run_pending(p); } }
            Some(1) => { self.confirm_discard = None; self.dirty = false; self.run_pending(p); }
            Some(_) => self.confirm_discard = None,
            None if !open => self.confirm_discard = None,
            None => {}
        }
    }

    /// Files dropped on the window: boards open, pictures are placed where they land.
    fn dropped_files(&mut self) {
        let (files, pos) = self.ctx.input(|i| (i.raw.dropped_files.clone(), i.pointer.hover_pos()));
        for f in files {
            let path = f.path().to_path_buf();
            if path.as_os_str().is_empty() { continue; }
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json") || e.eq_ignore_ascii_case("sab")) { self.guard(Pending::Open(path)); continue; }
            let at = pos.map(|p| self.view.to_board(((p.x - self.canvas_rect.min.x) as f64, (p.y - self.canvas_rect.min.y) as f64))).unwrap_or_else(|| self.view_center());
            if let Ok(bytes) = std::fs::read(&path) { self.insert_image(&bytes, at); }
        }
    }
}

pub fn recovery_file() -> PathBuf { crate::config_dir().join("recovery.json") }

impl eframe::App for BoardApp {
    /// Clear to transparent while the splash card shows (its window has no background).
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.splash.is_some() { [0.0; 4] } else { visuals.panel_fill.to_normalized_gamma_f32() }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        DARK.with(|d| d.set(self.prefs.dark));
        if crate::dialogs::splash(self, ui) { return; }
        self.auto_check_updates(&ui.ctx().clone());
        if !self.menu_tried && !cfg!(test) {
            self.menu_tried = true;
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            { self.native = crate::commands::native::Native::build(frame); }
        }
        let _ = &frame;
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if let Some(n) = self.native.take() { n.poll(self); self.native = Some(n); }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let in_window_menu = self.native.is_none();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let in_window_menu = true;
        if self.presenting.is_some() { crate::panels::present(self, ui); return; }
        if in_window_menu { crate::commands::menu_bar(self, ui); }
        crate::panels::side_panel(self, ui);
        self.open_sent_files();
        self.dropped_files();
        if ui.ctx().input(|i| i.viewport().close_requested()) && self.dirty {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_discard = Some(Pending::Quit);
        }
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| crate::canvas::canvas(self, ui));
        crate::toolbar::chrome(self, ui);
        self.discard_dialog();
        crate::update::dialog(self, &ui.ctx().clone());
        self.autosave();
        self.write_recovery();
        self.prune_textures();
        crate::panels::overlays(self, ui);
        if let Some((text, at)) = self.notice.clone() {
            let now = ui.ctx().input(|i| i.time);
            if now - at > 3.0 { self.notice = None; } else {
                egui::Area::new(egui::Id::unique("notice")).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -70.0]).order(egui::Order::Tooltip).show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(text));
                });
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
    }
}
