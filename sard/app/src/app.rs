//! The editor window: the presentation, the current slide and selection, undo history, file
//! commands, and the frame: ribbon, slide navigator, slide canvas, notes and status bar.

use crate::canvas::{self, Drag};
use crate::i18n::tr;
use egui::{vec2, Color32, RichText};
use qalam_kit::colors::*;
use slides_engine::opc::Package;
use slides_engine::render::Images;
use slides_engine::scene::{Item, Kind, Loc, Scene, Xfrm};
use slides_engine::Presentation;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Tab { #[default] Home, Insert, Design, Transitions, Animations, SlideShow, View, ShapeFormat, Table, TableLayout, Master, SmartArt }

/// In-place text editing of one shape (or table cell).
#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit { pub loc: Loc, pub cell: Option<(usize, usize)>, pub cursor: usize, pub anchor: usize }

impl TextEdit {
    pub fn range(&self) -> (usize, usize) { (self.cursor.min(self.anchor), self.cursor.max(self.anchor)) }
}

struct Snapshot { pkg: Package, current: usize }

/// Side panes on the right of the slide.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pane { #[default] None, Animation, Selection, Format, Comments, Reuse, Designer, Background }

/// What Format Painter copied: the shape's look (spPr fill/line/effects and style) and the
/// text formatting (run properties).
#[derive(Clone, Debug)]
pub struct Painted { pub sp_pr: Option<slides_engine::xml::Element>, pub style: Option<slides_engine::xml::Element>, pub rpr: Option<slides_engine::xml::Element> }

pub struct SlidesApp {
    pub pres: Presentation,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub current: usize,
    pub selected_slides: BTreeSet<usize>,
    pub selection: Vec<Loc>,
    pub edit: Option<TextEdit>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// What the last change was, to merge typing into one undo step.
    last_change: Option<&'static str>,
    pub tab: Tab,
    /// Zoom: None fits the slide to the window.
    pub zoom: Option<f32>,
    pub show_notes: bool,
    pub notes: (String, String),
    pub images: Images,
    /// Bumped on every change; renders are cached against it.
    pub revision: u64,
    pub canvas: canvas::Cache,
    pub thumbs: HashMap<String, (u64, egui::TextureHandle)>,
    pub drag: Option<Drag>,
    /// A shape to draw with the next drag (preset name, or "textbox").
    pub insert_mode: Option<String>,
    pub clipboard: Vec<slides_engine::xml::Element>,
    pub show: Option<crate::show::Show>,
    pub find: Option<(String, String)>,
    pub dialog: Option<crate::dialogs::Dialog>,
    pub pane: Pane,
    /// Format Painter: the copied look, and whether it stays on (double-click).
    pub painter: Option<(Painted, bool)>,
    /// The part the internal clipboard's shapes came from, and the text put on the system clipboard.
    pub clip_src: String,
    pub clip_text: String,
    pub reuse: Option<crate::panes::Reuse>,
    /// Selected table cells: (table, from, to).
    pub cell_sel: Option<(Loc, (usize, usize), (usize, usize))>,
    /// Where files held over the window would land (slide EMU).
    pub drop_at: Option<(f64, f64)>,
    /// View ▸ Gridlines and Guides.
    pub show_guides: bool,
    pub show_adjust: bool,
    /// The Format pane's tab (0 fill & line, 1 effects, 2 size, 3 text box).
    pub format_tab: u8,
    /// A transition preview playing on the canvas: (start time, the previous slide's picture).
    pub preview: Option<(f64, Option<egui::TextureHandle>)>,
    /// An animation preview playing on the canvas: (start time, the slide's layers).
    pub anim_preview: Option<(f64, crate::anim_play::Layers)>,
    /// The animation picked in the Animation Pane (index in the slide's list).
    pub anim_sel: Option<usize>,
    /// The View tab: the view, color preview, ruler, grid, guides and the master views' state.
    pub view: crate::views::View,
    pub color_mode: crate::views::ColorMode,
    pub show_ruler: bool,
    pub show_grid: bool,
    pub snap_grid: bool,
    pub smart_guides: bool,
    pub grid_spacing: f64,
    /// Slide Master view: the master or layout being edited.
    pub master_part: Option<String>,
    pub handout: crate::views::PageMaster,
    pub notes_master: crate::views::PageMaster,
    pub view_dialog: Option<crate::views::ViewDialog>,
    /// Whether Reading View runs the show; the window's place before a show moved it.
    pub before_show: Option<egui::Rect>,
    /// Video or audio previewed on the editing canvas.
    pub player: Option<crate::media::Player>,
    pub reading: bool,
    /// The Morph transition being previewed on the canvas.
    pub morph_preview: Option<crate::morph::Morph>,
    /// Animation Painter: the animations to give the next clicked object.
    pub anim_painter: Option<Vec<slides_engine::animation::Anim>>,
    /// Set as Default Shape: the look new shapes take.
    pub default_shape: Option<slides_engine::xml::Element>,
    pub shots: Option<crate::insert_tab::Shots>,
    /// A section being renamed: (its first slide, the name being typed).
    pub renaming_section: Option<(usize, String)>,
    /// Collapsed sections in the navigator (by first slide).
    pub collapsed_sections: Vec<usize>,
    /// The shapes last ungrouped, for Regroup.
    pub regroup: Vec<Loc>,
    pub last_group: usize,
    /// Selection Pane: shapes hidden while editing (shown again on save/close).
    pub error: Option<String>,
    pub notice: Option<(String, f64)>,
    pub dark: bool,
    pub ctx: egui::Context,
    /// Settings, and the File menu's state: AutoRecover, its prompt, Options, the File
    /// dialogs, the action waiting on "save changes?", quitting, and the splash (start time).
    pub prefs: crate::prefs::Prefs,
    pub last_autosave: f64,
    pub recover: Option<Option<PathBuf>>,
    pub options: Option<crate::prefs::Prefs>,
    pub file_dialog: Option<crate::filemenu::FileDialog>,
    pub pending: Option<crate::filemenu::Pending>,
    pub quitting: bool,
    pub splash: Option<f64>,
    /// The SmartArt or Mermaid editor.
    pub diagram_editor: Option<crate::smartart_ui::Editor>,
    /// The item picked inside a selected SmartArt graphic: (its group, item index).
    pub diagram_item: Option<(Loc, usize)>,
    /// The offline Translate window.
    pub translator: Option<crate::translate::State>,
    pub translate_job: Option<crate::translate::Job>,
    /// The last slide data built: (part, revision, scene), shared by everything that frame.
    pub scene_cache: std::cell::RefCell<Option<(String, u64, std::rc::Rc<Scene>)>>,
}

#[cfg(test)]
thread_local! { pub static TAKE_SENT_FILES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

impl SlidesApp {
    pub fn new(cc: &eframe::CreationContext, path: Option<PathBuf>) -> SlidesApp {
        qalam_kit::fonts::install_fonts(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx);
        let mut app = SlidesApp::blank(cc.egui_ctx.clone());
        let prefs = crate::prefs::Prefs::load();
        app.splash = (prefs.splash && !cfg!(test)).then_some(0.0);
        app.apply_prefs(prefs);
        if !app.prefs.widescreen { app.pres.set_size(9_144_000, 6_858_000); }
        // A file macOS sent before the window existed opens like one on the command line.
        qalam_kit::open_files::set_context(&cc.egui_ctx);
        let (sent, mut others) = qalam_kit::open_files::startup_paths(Vec::new());
        let path = match (path, sent) { (Some(p), Some(s)) => { others.insert(0, s); Some(p) } (p, s) => p.or(s) };
        if !cfg!(test) { qalam_kit::open_files::open_elsewhere(&others); }
        if let Some(p) = path { app.open_path(p); }
        app.offer_recovery();
        if app.splash.is_none() { app.auto_check_updates(&cc.egui_ctx); }
        app
    }

    pub fn blank(ctx: egui::Context) -> SlidesApp {
        let mut pres = Presentation::new_blank(true);
        // A new presentation opens on a Title Slide.
        if let Some(l) = pres.all_layouts().into_iter().find(|l| pres.layout_type(l) == "title") { pres.add_slide(&l, 0); }
        SlidesApp {
            pres, path: None, dirty: false, current: 0, selected_slides: BTreeSet::new(), selection: vec![], edit: None,
            undo: vec![], redo: vec![], last_change: None, tab: Tab::Home, zoom: None, show_notes: true, notes: Default::default(),
            images: Images::default(), revision: 1, canvas: Default::default(), thumbs: HashMap::new(), drag: None, insert_mode: None,
            clipboard: vec![], show: None, find: None, dialog: None, pane: Pane::None, painter: None, clip_src: String::new(), clip_text: String::new(), reuse: None, cell_sel: None, drop_at: None, show_guides: false, show_adjust: false, format_tab: 0, default_shape: None, preview: None, anim_preview: None, anim_sel: None, view: Default::default(), color_mode: Default::default(), show_ruler: false, show_grid: false, snap_grid: true, smart_guides: true, grid_spacing: 91_440.0, master_part: None, handout: Default::default(), notes_master: crate::views::PageMaster { per_page: 1, ..Default::default() }, view_dialog: None, before_show: None, player: None, reading: false, morph_preview: None, anim_painter: None, shots: None, renaming_section: None, collapsed_sections: vec![], regroup: vec![], last_group: 0, error: None, notice: None, dark: false, ctx, prefs: Default::default(), last_autosave: 0.0, recover: None, options: None, file_dialog: None, pending: None, quitting: false, splash: None, diagram_editor: None, diagram_item: None, translator: None, translate_job: None, scene_cache: Default::default(),
        }
    }

    // ---- document state ---------------------------------------------------------------

    pub fn slides(&self) -> Vec<String> { self.pres.slides() }
    /// Thumbnails of slides that are gone are freed (checked when the deck changes).
    pub fn prune_thumbs(&mut self) {
        if self.thumbs.len() < 40 && self.revision % 16 != 0 { return; }
        let pres = &self.pres;
        let rev = self.revision;
        self.thumbs.retain(|k, (r, _)| pres.pkg.has(k.split('#').next().unwrap_or(k)) && (rev - *r < 64 || !k.contains('#')));
    }
    /// The part being edited: the current slide (or, in Slide Master view, the master or layout).
    pub fn slide(&self) -> Option<String> {
        if self.view == crate::views::View::SlideMaster { if let Some(p) = &self.master_part { return Some(p.clone()); } }
        self.slides().get(self.current).cloned()
    }
    pub fn scene(&self) -> Option<Scene> { self.slide().map(|s| self.scene_of(&s)) }
    /// A part's slide data, built once and shared until the presentation changes.
    pub fn scene_of(&self, part: &str) -> Scene {
        if let Some((p, rev, sc)) = self.scene_cache.borrow().as_ref() { if p == part && *rev == self.revision { return (**sc).clone(); } }
        let sc = self.pres.scene(part);
        *self.scene_cache.borrow_mut() = Some((part.to_string(), self.revision, std::rc::Rc::new(sc.clone())));
        sc
    }

    /// Record an undo step before a change. Consecutive changes of the same `kind` (typing)
    /// merge into one step.
    pub fn checkpoint(&mut self, kind: &'static str) {
        if !(kind == "typing" && self.last_change == Some("typing")) {
            self.undo.push(Snapshot { pkg: self.pres.pkg.clone(), current: self.current });
            if self.undo.len() > 100 { self.undo.remove(0); }
        }
        self.redo.clear();
        self.last_change = Some(kind);
        self.changed();
    }
    /// Note a change that's already recorded (redraw, mark unsaved).
    pub fn changed(&mut self) { self.revision += 1; self.dirty = true; *self.scene_cache.borrow_mut() = None; }
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }
    pub fn undo(&mut self) {
        let Some(s) = self.undo.pop() else { return };
        self.redo.push(Snapshot { pkg: std::mem::replace(&mut self.pres.pkg, s.pkg), current: self.current });
        self.after_history(s.current);
    }
    pub fn redo(&mut self) {
        let Some(s) = self.redo.pop() else { return };
        self.undo.push(Snapshot { pkg: std::mem::replace(&mut self.pres.pkg, s.pkg), current: self.current });
        self.after_history(s.current);
    }
    fn after_history(&mut self, current: usize) {
        self.current = current.min(self.slides().len().saturating_sub(1));
        self.selection.clear();
        self.edit = None;
        self.last_change = None;
        self.images = Images::default();
        self.revision += 1;
        self.dirty = true;
    }

    pub fn go_to(&mut self, i: usize) {
        let n = self.slides().len();
        if n == 0 { return; }
        self.current = i.min(n - 1);
        self.selection.clear();
        self.edit = None;
        self.selected_slides = [self.current].into();
    }

    /// The scene items of a selected top-level shape (several when it's a group).
    pub fn items_of<'a>(scene: &'a Scene, loc: &Loc) -> Vec<&'a Item> {
        scene.items.iter().filter(|i| i.own && i.loc.part == loc.part && i.loc.path.first() == loc.path.first()).collect()
    }

    /// The frame of a top-level shape: as written, or from where it's drawn (inherited).
    pub fn frame_of(&self, scene: &Scene, loc: &Loc) -> Option<Xfrm> {
        if let Some(x) = self.pres.xfrm(loc) { return Some(x); }
        let it = Self::items_of(scene, loc).into_iter().find(|i| i.loc.path.len() == 1)?;
        let (cx, cy) = it.transform.apply(it.w / 2.0, it.h / 2.0);
        Some(Xfrm { x: cx - it.w / 2.0, y: cy - it.h / 2.0, w: it.w, h: it.h, rot: it.transform.angle(), flip_h: false, flip_v: false })
    }

    /// The selected shape's kind (for contextual tabs).
    pub fn selected_kind(&self, scene: &Scene) -> Option<&'static str> {
        let loc = self.selection.first()?;
        let it = Self::items_of(scene, loc).into_iter().next()?;
        Some(match &it.kind { Kind::Table(_) => "table", Kind::Picture { .. } => "picture", Kind::Shape { .. } => "shape", Kind::Chart(_) => "chart", Kind::Frame { .. } => "frame" })
    }

    // ---- files ------------------------------------------------------------------------

    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Presentation1"));
        format!("{}{} — {}", name, if self.dirty { " •" } else { "" }, crate::app_name())
    }

    pub fn new_presentation(&mut self) {
        let ctx = self.ctx.clone();
        let (dark, prefs) = (self.dark, self.prefs.clone());
        slides_engine::three_d::clear_cache();
        crate::texcache::clear(&ctx);
        *self = SlidesApp::blank(ctx);
        self.dark = dark;
        if !prefs.widescreen { self.pres.set_size(9_144_000, 6_858_000); }
        self.show_notes = prefs.show_notes;
        self.prefs = prefs;
    }

    /// Opens files the system sent while running (Finder, Open With): the first here (asking
    /// to save changes first), any others in new windows.
    pub fn open_sent_files(&mut self) {
        // The queue is shared by the whole process; tests opt in per thread.
        #[cfg(test)]
        if !TAKE_SENT_FILES.with(|t| t.get()) { return; }
        if self.pending.is_some() || !qalam_kit::open_files::pending() { return; }
        let (here, rest) = qalam_kit::open_files::route(qalam_kit::open_files::take());
        if !cfg!(test) { qalam_kit::open_files::open_elsewhere(&rest); }
        if let Some(p) = here { self.guarded(crate::filemenu::Pending::OpenPath(p)); }
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| Presentation::open(&b)) {
            Ok(p) => {
                let ctx = self.ctx.clone();
                let (dark, mut prefs) = (self.dark, self.prefs.clone());
                slides_engine::three_d::clear_cache();
                crate::texcache::clear(&ctx);
                *self = SlidesApp::blank(ctx);
                self.pres = p;
                if !cfg!(test) { prefs.add_recent(&path); }
                self.show_notes = prefs.show_notes;
                self.prefs = prefs;
                self.path = Some(path);
                self.dark = dark;
                self.dirty = false;
                // Say when a font inside the file couldn't be used.
                let missing: Vec<String> = self.pres.load_embedded_fonts().into_iter().filter(|f| !f.loaded).map(|f| format!("{} ({})", f.family, f.note)).collect();
                if !missing.is_empty() { self.notify(&crate::i18n::trf("Some embedded fonts couldn't be used: {}", &[&missing.join(", ")])); }
            }
            Err(e) => self.error = Some(crate::i18n::trf("Couldn't open the presentation.\n\n{}", &[&e])),
        }
    }

    pub fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter(&tr("PowerPoint Presentation"), &["pptx", "potx", "ppsx"]).pick_file() { self.open_path(p); }
    }

    pub fn save(&mut self, save_as: bool) {
        let path = match (&self.path, save_as) {
            (Some(p), false) => p.clone(),
            _ => match rfd::FileDialog::new().add_filter(&tr("PowerPoint Presentation"), &["pptx"]).set_file_name("Presentation1.pptx").save_file() { Some(p) => p, None => return },
        };
        // Who and when, in the file's properties.
        let now = slides_engine::export::w3c_now();
        if self.pres.property("dcterms:created").is_empty() { self.pres.set_property("dcterms:created", &now); }
        if self.pres.property("dc:creator").is_empty() { let a = self.prefs.author.clone(); self.pres.set_property("dc:creator", &a); }
        self.pres.set_property("dcterms:modified", &now);
        let a = self.prefs.author.clone();
        self.pres.set_property("cp:lastModifiedBy", &a);
        match self.pres.save().and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string())) {
            Ok(()) => {
                if !cfg!(test) { self.prefs.add_recent(&path); }
                self.path = Some(path);
                self.dirty = false;
                self.clear_recovery();
                self.notify(&tr("Saved"));
            }
            Err(e) => self.error = Some(crate::i18n::trf("Couldn't save the presentation.\n\n{}", &[&e])),
        }
    }

    /// Export the current slide, or every slide (numbered files in a folder), as PNG.
    pub fn export_png(&mut self, all: bool) {
        let slides = self.slides();
        if all {
            let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
            for (i, s) in slides.iter().enumerate() {
                if let Some(png) = slides_engine::render::slide_png(&self.pres, s, 1920) { let _ = std::fs::write(dir.join(format!("Slide{}.png", i + 1)), png); }
            }
            self.notify(&crate::i18n::trf("Exported {} slides", &[&slides.len()]));
        } else if let Some(s) = self.slide() {
            let Some(p) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name(format!("Slide{}.png", self.current + 1)).save_file() else { return };
            if let Some(png) = slides_engine::render::slide_png(&self.pres, &s, 1920) { let _ = std::fs::write(p, png); self.notify(&tr("Exported")); }
        }
    }

    pub fn notify(&mut self, text: &str) { self.notice = Some((text.to_string(), self.ctx.input(|i| i.time))); }

    // ---- slide commands ---------------------------------------------------------------

    pub fn new_slide(&mut self, layout: Option<String>) {
        let layouts = self.pres.all_layouts();
        // Like PowerPoint: after a title slide comes Title and Content, else the same layout.
        let current_layout = self.slide().and_then(|s| self.pres.layout_of(&s));
        let layout = layout.or_else(|| match current_layout.as_deref().map(|l| self.pres.layout_type(l)).as_deref() {
            Some("title") | None => layouts.iter().find(|l| self.pres.layout_type(l) == "obj").cloned(),
            _ => current_layout.clone(),
        }).or_else(|| layouts.first().cloned());
        let Some(layout) = layout else { return };
        self.checkpoint("slide");
        let at = if self.slides().is_empty() { 0 } else { self.current + 1 };
        self.pres.add_slide(&layout, at);
        self.go_to(at);
    }
    pub fn duplicate_slides(&mut self) {
        let slides = self.slides();
        let mut sel: Vec<usize> = if self.selected_slides.is_empty() { vec![self.current] } else { self.selected_slides.iter().copied().collect() };
        sel.sort_unstable();
        self.checkpoint("slide");
        let mut last = self.current;
        for &i in sel.iter().rev() { if let Some(s) = slides.get(i) { self.pres.duplicate_slide(s); last = i + 1; } }
        self.go_to(last);
    }
    pub fn delete_slides(&mut self) {
        let slides = self.slides();
        let sel: Vec<usize> = if self.selected_slides.is_empty() { vec![self.current] } else { self.selected_slides.iter().copied().collect() };
        if sel.len() >= slides.len() && slides.len() <= 1 { return; }
        self.checkpoint("slide");
        for &i in sel.iter().rev() { if let Some(s) = slides.get(i) { self.pres.delete_slide(s); } }
        let first = sel.first().copied().unwrap_or(0);
        self.go_to(first.min(self.slides().len().saturating_sub(1)));
    }
    pub fn toggle_hidden(&mut self) {
        let slides = self.slides();
        let sel: Vec<usize> = if self.selected_slides.is_empty() { vec![self.current] } else { self.selected_slides.iter().copied().collect() };
        let hide = !sel.iter().all(|&i| slides.get(i).is_some_and(|s| self.pres.hidden(s)));
        self.checkpoint("slide");
        for i in sel { if let Some(s) = slides.get(i) { self.pres.set_hidden(s, hide); } }
    }
    pub fn move_slide(&mut self, from: usize, to: usize) {
        if from == to { return; }
        self.checkpoint("slide");
        self.pres.move_slide(from, to);
        self.go_to(to);
    }
    pub fn set_layout(&mut self, layout: &str) {
        let Some(s) = self.slide() else { return };
        self.checkpoint("slide");
        self.pres.set_layout(&s, layout);
    }

    // ---- shape commands ---------------------------------------------------------------

    /// The default frame for a new object: centered on the slide.
    pub fn centered(&self, w: f64, h: f64) -> Xfrm {
        let (sw, sh) = self.pres.size();
        Xfrm { x: (sw - w) / 2.0, y: (sh - h) / 2.0, w, h, ..Default::default() }
    }

    pub fn insert_shape(&mut self, preset: &str, x: Xfrm) {
        let Some(s) = self.slide() else { return };
        self.checkpoint("insert");
        let text_box = preset == "textbox";
        if let Some(loc) = self.pres.insert_shape(&s, if text_box { "rect" } else { preset }, &x, text_box) {
            // The default shape's look (fill, outline, effects, style, text formatting).
            if let (false, Some(def)) = (text_box, self.default_shape.clone()) {
                if let Some(sp) = self.pres.element_mut(&loc) {
                    if let (Some(src), Some(dst)) = (def.child("p:spPr"), sp.child_mut("p:spPr")) {
                        dst.children.retain(|n| matches!(n, slides_engine::xml::Node::Element(e) if matches!(e.name.as_str(), "a:xfrm" | "a:prstGeom" | "a:custGeom")));
                        for c in src.elements().filter(|e| !matches!(e.name.as_str(), "a:xfrm" | "a:prstGeom" | "a:custGeom")) { dst.children.push(slides_engine::xml::Node::Element(c.clone())); }
                    }
                    sp.remove_children("p:style");
                    if let Some(st) = def.child("p:style") { let pos = sp.children.iter().position(|n| matches!(n, slides_engine::xml::Node::Element(e) if e.name == "p:txBody")).unwrap_or(sp.children.len()); sp.children.insert(pos, slides_engine::xml::Node::Element(st.clone())); }
                }
            }
            self.selection = vec![loc.clone()];
            if text_box { self.edit = Some(TextEdit { loc, cell: None, cursor: 0, anchor: 0 }); }
        }
        self.insert_mode = None;
    }

    pub fn insert_picture_dialog(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter(&tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() else { return };
        self.insert_picture_file(&p);
    }
    pub fn insert_picture_file(&mut self, p: &std::path::Path) {
        let Ok(bytes) = std::fs::read(p) else { return };
        let Ok((iw, ih)) = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().map_err(|_| ()).and_then(|r| r.into_dimensions().map_err(|_| ())) else { self.error = Some(tr("That file isn't a picture Sard can read.")); return };
        let Some(s) = self.slide() else { return };
        let (sw, sh) = self.pres.size();
        // Pictures come in at their size (96 dpi), scaled down to fit the slide.
        let (mut w, mut h) = (iw as f64 * 9525.0, ih as f64 * 9525.0);
        let k = (sw * 0.8 / w).min(sh * 0.8 / h).min(1.0);
        w *= k; h *= k;
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into());
        self.checkpoint("insert");
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(loc) = self.pres.insert_picture(&s, bytes, &ext, &self.centered(w, h), &name) { self.selection = vec![loc]; }
    }

    pub fn insert_table(&mut self, rows: usize, cols: usize) {
        let Some(s) = self.slide() else { return };
        let (sw, _) = self.pres.size();
        self.checkpoint("insert");
        let x = self.centered(sw * 0.7, 370_840.0 * rows as f64);
        if let Some(loc) = self.pres.insert_table(&s, rows, cols, &x) { self.selection = vec![loc]; }
    }

    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() { return; }
        self.checkpoint("delete");
        let sel = std::mem::take(&mut self.selection);
        self.pres.delete_shapes(&sel);
        self.edit = None;
    }

    /// Copy the selected shapes (and their text, or the edited text, to the system clipboard).
    pub fn copy(&mut self) {
        if let Some(e) = &self.edit {
            let (a, b) = e.range();
            let t: String = self.pres.shape_text(&e.loc, e.cell).chars().skip(a).take(b - a).collect();
            if !t.is_empty() { self.set_system_text(&t); }
            return;
        }
        self.clipboard = self.selection.iter().filter_map(|l| self.pres.element(l).cloned()).collect();
        self.clip_src = self.slide().unwrap_or_default();
        let text: Vec<String> = self.selection.iter().map(|l| self.pres.shape_text(l, None)).filter(|t| !t.is_empty()).collect();
        let t = if text.is_empty() { "\u{FFFC}".to_string() } else { text.join("\n") };
        self.clip_text = t.clone();
        self.set_system_text(&t);
    }
    fn set_system_text(&self, t: &str) { if let Ok(mut c) = arboard::Clipboard::new() { let _ = c.set_text(t.to_string()); } }
    pub fn cut(&mut self) {
        if let Some(e) = self.edit.clone() {
            self.copy();
            let (a, b) = e.range();
            if a < b { self.checkpoint("cut"); self.pres.delete_text(&e.loc, e.cell, a, b); self.edit = Some(TextEdit { cursor: a, anchor: a, ..e }); }
            return;
        }
        self.copy();
        self.delete_selection();
    }
    /// Paste: our shapes when they're what was copied last; else a picture or text from the
    /// system clipboard. `how`: "keep" (source formatting), "picture", "text".
    pub fn paste_as(&mut self, how: &str) {
        let Some(s) = self.slide() else { return };
        let system_text = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok());
        let ours = !self.clipboard.is_empty() && system_text.as_deref().is_none_or(|t| t == self.clip_text);
        if ours && how != "text" && how != "picture" {
            self.checkpoint("paste");
            let same = self.clip_src == s;
            let src = self.clip_src.clone();
            let off = if same { 228_600.0 } else { 0.0 };
            self.selection = self.clipboard.clone().iter().filter_map(|el| self.pres.insert_copy(&s, &src, el, off, off)).collect();
            return;
        }
        if ours && how == "picture" {
            // The copied shapes as one picture: render them alone on the slide.
            let scene = self.pres.scene(&s);
            let _ = scene;
            if let Some(png) = self.selection_png() {
                let (w, h) = (self.pres.size().0 / 3.0, self.pres.size().1 / 3.0);
                self.checkpoint("paste");
                if let Some(l) = self.pres.insert_picture(&s, png, "png", &self.centered(w, h), "Picture") { self.selection = vec![l]; }
                return;
            }
        }
        // An image on the system clipboard becomes a picture.
        if how != "text" {
            if let Some(img) = arboard::Clipboard::new().ok().and_then(|mut c| c.get_image().ok()) {
                let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned());
                if let Some(rgba) = rgba {
                    let mut png = std::io::Cursor::new(Vec::new());
                    if image::DynamicImage::ImageRgba8(rgba.clone()).write_to(&mut png, image::ImageFormat::Png).is_ok() {
                        let (sw, sh) = self.pres.size();
                        let (mut w, mut h) = (rgba.width() as f64 * 9525.0, rgba.height() as f64 * 9525.0);
                        let k = (sw * 0.8 / w).min(sh * 0.8 / h).min(1.0);
                        w *= k; h *= k;
                        self.checkpoint("paste");
                        if let Some(l) = self.pres.insert_picture(&s, png.into_inner(), "png", &self.centered(w, h), "Pasted picture") { self.selection = vec![l]; }
                        return;
                    }
                }
            }
        }
        if let Some(t) = system_text.or_else(|| (!self.clip_text.is_empty()).then(|| self.clip_text.clone())) {
            let t = t.replace("\r\n", "\n").replace('\u{FFFC}', "");
            if t.is_empty() { return; }
            if self.edit.is_some() { self.insert_text_here(&t); return; }
            self.checkpoint("paste");
            let (sw, _) = self.pres.size();
            if let Some(l) = self.pres.insert_shape(&s, "rect", &self.centered(sw * 0.5, 600_000.0), true) { self.pres.insert_text(&l, None, 0, &t); self.selection = vec![l]; }
        }
    }
    pub fn paste(&mut self) { self.paste_as("dest"); }

    /// The selected shapes rendered alone as a PNG (Paste as Picture, Save as Picture).
    pub fn selection_png(&mut self) -> Option<Vec<u8>> {
        let s = self.slide()?;
        let scene = self.pres.scene(&s);
        let keep: Vec<usize> = self.selection.iter().filter_map(|l| l.path.first().copied()).collect();
        let mut only = scene.clone();
        only.items.retain(|i| i.own && keep.contains(&i.loc.path[0]));
        only.background = slides_engine::scene::Fill::None;
        let frames: Vec<Xfrm> = self.selection.iter().filter_map(|l| self.frame_of(&scene, l)).collect();
        let x0 = frames.iter().map(|f| f.x).fold(f64::MAX, f64::min);
        let y0 = frames.iter().map(|f| f.y).fold(f64::MAX, f64::min);
        let x1 = frames.iter().map(|f| f.x + f.w).fold(f64::MIN, f64::max);
        let y1 = frames.iter().map(|f| f.y + f.h).fold(f64::MIN, f64::max);
        let scale = 1600.0 / only.w;
        let mut pm = slides_engine::render::render(&self.pres, &only, &mut self.images, slides_engine::render::Options { scale, prompts: false, skip_text_of: None, translate: None })?;
        // Transparent outside the shapes: the render filled white; clear by re-rendering is
        // costly, so crop to the shapes' box.
        let (cx, cy) = ((x0 * scale).max(0.0) as u32, (y0 * scale).max(0.0) as u32);
        let (cw, ch) = (((x1 - x0) * scale).ceil().max(1.0) as u32, ((y1 - y0) * scale).ceil().max(1.0) as u32);
        let mut out = tiny_skia::Pixmap::new(cw.min(pm.width() - cx.min(pm.width() - 1)), ch.min(pm.height() - cy.min(pm.height() - 1)))?;
        out.draw_pixmap(-(cx as i32), -(cy as i32), pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        let _ = &mut pm;
        out.encode_png().ok()
    }

    // ---- format painter ---------------------------------------------------------------

    pub fn pick_format(&mut self, sticky: bool) {
        let Some(l) = self.edit.as_ref().map(|e| e.loc.clone()).or_else(|| self.selection.first().cloned()) else { return };
        let Some(el) = self.pres.element(&l) else { return };
        let rpr = el.find("a:rPr").cloned().or_else(|| el.find("a:endParaRPr").cloned());
        let mut sp_pr = el.child("p:spPr").cloned();
        if let Some(sp) = &mut sp_pr { sp.remove_children("a:xfrm"); sp.remove_children("a:prstGeom"); sp.remove_children("a:custGeom"); }
        self.painter = Some((Painted { sp_pr, style: el.child("p:style").cloned(), rpr }, sticky));
    }
    /// Apply the painted look to a shape (or the edited text selection).
    pub fn apply_format(&mut self, loc: &Loc) {
        let Some((p, sticky)) = self.painter.clone() else { return };
        self.checkpoint("format-painter");
        if let Some(e) = self.edit.clone() {
            if let Some(rpr) = &p.rpr { let (a, b) = e.range(); self.apply_rpr(&e.loc, e.cell, Some((a, b)), rpr); }
        } else if let Some(sp) = self.pres.element_mut(loc) {
            if let Some(src) = &p.sp_pr {
                let dst = sp.ensure_at("p:spPr", &["p:style", "p:txBody"]);
                dst.children.retain(|n| matches!(n, slides_engine::xml::Node::Element(e) if matches!(e.name.as_str(), "a:xfrm" | "a:prstGeom" | "a:custGeom")));
                for c in src.elements() { dst.children.push(slides_engine::xml::Node::Element(c.clone())); }
            }
            sp.remove_children("p:style");
            if let Some(st) = &p.style { let pos = sp.children.iter().position(|n| matches!(n, slides_engine::xml::Node::Element(e) if e.name == "p:txBody")).unwrap_or(sp.children.len()); sp.children.insert(pos, slides_engine::xml::Node::Element(st.clone())); }
            if let Some(rpr) = &p.rpr { self.apply_rpr(loc, None, None, rpr); }
        }
        if !sticky { self.painter = None; }
    }
    fn apply_rpr(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, rpr: &slides_engine::xml::Element) {
        if let Some((a, b)) = range { if a < b { self.pres.format_range(loc, cell, a, b, &Default::default()); } }
        let Some(sp) = self.pres.element_mut(loc) else { return };
        let Some(body) = sp.child_mut("p:txBody") else { return };
        let mut pos = 0usize;
        for p in body.elements_mut().filter(|e| e.name == "a:p") {
            for r in p.elements_mut() {
                let len = match r.name.as_str() { "a:r" | "a:fld" => r.child("a:t").map(|t| t.text().chars().count()).unwrap_or(0), "a:br" => 1, _ => 0 };
                let inside = range.is_none_or(|(a, b)| a >= b || (pos >= a && pos + len <= b && len > 0));
                if inside && matches!(r.name.as_str(), "a:r" | "a:fld") {
                    let mut new = rpr.clone();
                    new.name = "a:rPr".into();
                    new.children.retain(|c| !matches!(c, slides_engine::xml::Node::Element(e) if e.name == "a:hlinkClick"));
                    r.remove_children("a:rPr");
                    r.children.insert(0, slides_engine::xml::Node::Element(new));
                }
                pos += len;
            }
            pos += 1;
        }
    }

    // ---- commands the dialogs and panes use ---------------------------------------------

    /// Run a text command on the edited text (its selection) or the selected shapes' text.
    pub fn text_cmd(&mut self, f: impl Fn(&mut Presentation, &Loc, Option<(usize, usize)>, Option<(usize, usize)>)) {
        let targets: Vec<(Loc, Option<(usize, usize)>, Option<(usize, usize)>)> = match &self.edit {
            Some(e) => vec![(e.loc.clone(), e.cell, Some(e.range()).filter(|(a, b)| a < b))],
            None => self.selection.iter().map(|l| (l.clone(), None, None)).collect(),
        };
        if targets.is_empty() { return; }
        self.checkpoint("format");
        for (l, c, r) in targets { f(&mut self.pres, &l, c, r); }
    }
    /// Run a command on each selected shape.
    pub fn shapes_cmd(&mut self, f: impl Fn(&mut Presentation, &Loc)) {
        let targets: Vec<Loc> = match &self.edit { Some(e) => vec![e.loc.clone()], None => self.selection.clone() };
        if targets.is_empty() { return; }
        self.checkpoint("format");
        for l in targets { f(&mut self.pres, &l); }
    }
    /// Insert text at the caret (or in a new text box when not editing).
    pub fn insert_text_here(&mut self, t: &str) {
        match self.edit.clone() {
            Some(e) => {
                let (a, b) = e.range();
                self.checkpoint("typing");
                if a < b { self.pres.delete_text(&e.loc, e.cell, a, b); }
                self.pres.insert_text(&e.loc, e.cell, a, t);
                let c = a + t.chars().count();
                self.edit = Some(TextEdit { cursor: c, anchor: c, ..e });
            }
            None => {
                let Some(s) = self.slide() else { return };
                self.checkpoint("insert");
                let (sw, _) = self.pres.size();
                if let Some(l) = self.pres.insert_shape(&s, "rect", &self.centered(sw * 0.3, 500_000.0), true) {
                    self.pres.insert_text(&l, None, 0, t);
                    let n = t.chars().count();
                    self.selection = vec![l.clone()];
                    self.edit = Some(TextEdit { loc: l, cell: None, cursor: n, anchor: n });
                }
            }
        }
    }
    /// Insert a field (date/time or slide number) at the caret, or in a new text box.
    pub fn insert_field(&mut self, kind: &str) {
        if self.edit.is_none() { self.insert_text_here(""); }
        let Some(e) = self.edit.clone() else { return };
        self.checkpoint("insert");
        let (a, _) = e.range();
        self.pres.insert_field(&e.loc, e.cell, a, kind, self.current + 1);
        let n = self.pres.shape_text(&e.loc, e.cell).chars().count();
        let c = (a + 1).min(n);
        self.edit = Some(TextEdit { cursor: c, anchor: c, ..e });
    }
    pub fn insert_equation(&mut self, loc: Option<Loc>, linear: String) {
        let Some(s) = self.slide() else { return };
        self.checkpoint("equation");
        let (sw, sh) = self.pres.size();
        let center = loc.as_ref().and_then(|l| self.pres.xfrm(l)).map(|x| (x.x + x.w / 2.0, x.y + x.h / 2.0)).unwrap_or((sw / 2.0, sh / 2.0));
        if let Some(l) = &loc { self.pres.delete_shapes(std::slice::from_ref(l)); }
        if let Some(l) = self.pres.insert_equation(&s, &linear, 28.0, center) { self.selection = vec![l]; }
    }
    pub fn insert_smartart(&mut self, loc: Option<Loc>, kind: word_engine::objects::DiagramKind, text: &str) {
        let Some(s) = self.slide() else { return };
        let items = word_engine::objects::Diagram::items_from_text(text);
        if items.is_empty() { return; }
        self.checkpoint("smartart");
        let accent = self.accent_hex(1);
        let new = match &loc {
            Some(l) => self.pres.update_smartart(l, kind, &items, &accent),
            None => { let (sw, sh) = self.pres.size(); let x = self.centered(sw * 0.7, sh * 0.6); self.pres.insert_smartart(&s, kind, &items, &x, &accent) }
        };
        if let Some(l) = new { self.selection = vec![l]; }
    }
    /// The theme's accent color N as hex.
    pub fn accent_hex(&self, n: usize) -> String {
        let s = self.slide().unwrap_or_default();
        let (theme, _) = slides_engine::scene::color_context(&self.pres, &s);
        theme.scheme.get(&format!("accent{n}")).map(|c| c.to_hex()).unwrap_or_else(|| "4472C4".into())
    }
    /// Link the edited text (selection) or the selected shapes.
    pub fn set_link(&mut self, target: Option<&str>) {
        let t = target.map(String::from);
        if let Some(e) = self.edit.clone() {
            self.checkpoint("link");
            let r = Some(e.range()).filter(|(a, b)| a < b);
            self.pres.set_text_link(&e.loc, e.cell, r, t.as_deref());
        } else {
            self.shapes_cmd(move |p, l| p.set_shape_link(l, t.as_deref()));
        }
    }
    pub fn photo_album(&mut self, files: &[PathBuf], per: usize, captions: bool) {
        let pics: Vec<(Vec<u8>, String, u32, u32)> = files.iter().filter_map(|f| {
            let b = std::fs::read(f).ok()?;
            let (w, h) = image::ImageReader::new(std::io::Cursor::new(&b)).with_guessed_format().ok()?.into_dimensions().ok()?;
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into());
            Some((b, ext, w, h))
        }).collect();
        if pics.is_empty() { return; }
        self.checkpoint("album");
        let before = self.slides().len();
        self.pres.photo_album(&pics, per, captions);
        self.go_to(before);
    }
    /// Find the next match after `last` across the slides; selects it for editing.
    pub fn find_next(&mut self, query: &str, match_case: bool, whole: bool, last: Option<(usize, usize)>) -> Option<(usize, usize)> {
        if query.is_empty() { return None; }
        let slides = self.slides();
        let norm = |s: &str| if match_case { s.to_string() } else { s.to_lowercase() };
        let q = norm(query);
        let (start_slide, start_shape) = last.map(|(s, k)| (s, k + 1)).unwrap_or((self.current, 0));
        for step in 0..=slides.len() {
            let si = (start_slide + step) % slides.len().max(1);
            let n = self.pres.slide(&slides[si]).and_then(|r| r.path("p:cSld/p:spTree")).map(|t| t.elements().count()).unwrap_or(0);
            let from = if step == 0 { start_shape } else { 0 };
            for k in from..n {
                let loc = Loc { part: slides[si].clone(), path: vec![k] };
                let text = self.pres.shape_text(&loc, None);
                let hay = norm(&text);
                let mut found = None;
                let mut i = 0;
                while let Some(p) = hay[i..].find(&q) {
                    let at = i + p;
                    let ok = !whole || ((at == 0 || !hay[..at].chars().last().is_some_and(char::is_alphanumeric)) && !hay[at + q.len()..].chars().next().is_some_and(char::is_alphanumeric));
                    if ok { found = Some(at); break; }
                    i = at + q.len().max(1);
                }
                if let Some(byte) = found {
                    let a = hay[..byte].chars().count();
                    let b = a + query.chars().count();
                    self.go_to(si);
                    self.selection = vec![loc.clone()];
                    self.edit = Some(TextEdit { loc, cell: None, cursor: b, anchor: a });
                    return Some((si, k));
                }
            }
        }
        self.notify(&tr("Search finished. No more matches."));
        None
    }
    pub fn replace_current(&mut self, query: &str, with: &str, match_case: bool) {
        let Some(e) = self.edit.clone() else { return };
        let (a, b) = e.range();
        let sel: String = self.pres.shape_text(&e.loc, e.cell).chars().skip(a).take(b - a).collect();
        let same = if match_case { sel == query } else { sel.to_lowercase() == query.to_lowercase() };
        if !same || a == b { return; }
        self.checkpoint("replace");
        self.pres.delete_text(&e.loc, e.cell, a, b);
        self.pres.insert_text(&e.loc, e.cell, a, with);
        let c = a + with.chars().count();
        self.edit = Some(TextEdit { cursor: c, anchor: c, ..e });
    }

    pub fn duplicate_selection(&mut self) {
        if self.selection.is_empty() { return; }
        self.checkpoint("duplicate");
        let sel = self.selection.clone();
        self.selection = sel.iter().filter_map(|l| self.pres.duplicate_shape(l, 228_600.0, 228_600.0)).collect();
    }

    /// Apply character formatting: to the text selection while editing, else to the selected
    /// shapes' whole text.
    pub fn format(&mut self, f: slides_engine::edit::RunFormat) {
        if let Some(e) = self.edit.clone() {
            let (a, b) = e.range();
            self.checkpoint("format");
            if a < b { self.pres.format_range(&e.loc, e.cell, a, b, &f); } else { self.pres.format_shape_text(&e.loc, e.cell, &f); }
            return;
        }
        if self.selection.is_empty() { return; }
        self.checkpoint("format");
        for l in self.selection.clone() { self.pres.format_shape_text(&l, None, &f); }
    }

    /// The paragraphs the text selection covers, while editing.
    pub fn para_range(&self) -> Option<(usize, usize)> {
        let e = self.edit.as_ref()?;
        let text = self.pres.shape_text(&e.loc, e.cell);
        let (a, b) = e.range();
        let pa = text.chars().take(a).filter(|c| *c == '\n').count();
        let pb = text.chars().take(b).filter(|c| *c == '\n').count();
        Some((pa, pb))
    }

    /// Run a paragraph command on the edited paragraphs, or the selected shapes.
    pub fn paragraphs(&mut self, f: impl Fn(&mut Presentation, &Loc, Option<(usize, usize)>, Option<(usize, usize)>)) {
        let targets: Vec<(Loc, Option<(usize, usize)>, Option<(usize, usize)>)> = match &self.edit {
            Some(e) => vec![(e.loc.clone(), e.cell, self.para_range())],
            None => self.selection.iter().map(|l| (l.clone(), None, None)).collect(),
        };
        if targets.is_empty() { return; }
        self.checkpoint("paragraph");
        for (l, c, p) in targets { f(&mut self.pres, &l, c, p); }
    }

    /// The format at the caret (or of the selected shape's first run), for the ribbon.
    pub fn current_run(&self, scene: &Scene) -> Option<slides_engine::scene::Run> {
        let loc = self.edit.as_ref().map(|e| e.loc.clone()).or_else(|| self.selection.first().cloned())?;
        let it = Self::items_of(scene, &loc).into_iter().next()?;
        let cell_text = match (&it.kind, self.edit.as_ref().and_then(|e| e.cell)) {
            (Kind::Table(tb), Some((r, c))) => tb.cells.iter().find(|k| k.row == r && k.col == c).map(|k| &k.text),
            (Kind::Table(tb), None) => tb.cells.first().map(|k| &k.text),
            _ => None,
        };
        let t = match (&it.kind, cell_text) { (_, Some(t)) => t, (Kind::Shape { text: Some(t), .. }, _) => t, _ => return None };
        let idx = self.edit.as_ref().map(|e| e.range().0.saturating_sub(1)).unwrap_or(0);
        let mut pos = 0;
        for p in &t.paras {
            for r in &p.runs { let n = if r.br { 1 } else { r.text.chars().count() }; if idx < pos + n.max(1) { return Some(r.clone()); } pos += n; }
            pos += 1;
        }
        t.paras.first().and_then(|p| p.runs.first().cloned()).or_else(|| t.paras.first().map(|p| slides_engine::scene::Run { font: p.end_font.clone(), size: p.end_size, ..Default::default() }))
    }

    pub fn replace_all(&mut self, find: &str, with: &str) {
        self.checkpoint("replace");
        let n = self.pres.replace_all(find, with, false);
        self.notify(&crate::i18n::trf("Replaced {} occurrences", &[&n]));
    }

    // ---- the frame ----------------------------------------------------------------------

    fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Slide data is shared within a frame; edits made directly between frames are seen.
        *self.scene_cache.borrow_mut() = None;
        if self.splash.is_some() && crate::filemenu::splash(self, ui) { return; }
        self.autosave();
        crate::theme::apply_mode(&ctx, self.dark);
        qalam_kit::colors::DARK.with(|d| d.set(self.dark));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        // Files dropped on the window: a presentation opens, a picture is inserted.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        // Files held over the window: say what letting go will do.
        let hovering = ctx.input(|i| i.raw.hovered_files.iter().filter_map(|f| f.path.as_ref()?.extension().map(|e| e.to_string_lossy().to_lowercase())).collect::<Vec<_>>());
        if !hovering.is_empty() {
            let msg = if hovering.iter().any(|e| matches!(e.as_str(), "pptx" | "potx" | "ppsx")) { tr("Drop to open the presentation") } else { tr("Drop to insert on this slide") };
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop-hint")));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(60));
            painter.rect(egui::Rect::from_center_size(screen.center(), egui::vec2(360.0, 64.0)), 12.0, c_card(), egui::Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
            painter.text(screen.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(16.0), c_ink());
            if let Some(p) = ctx.input(|i| i.pointer.hover_pos()) { self.drop_at = self.canvas.view.map(|(o, s)| (((p.x - o.x) / s) as f64, ((p.y - o.y) / s) as f64)); }
        }
        self.open_sent_files();
        if !cfg!(test) { qalam_kit::default_apps::first_run_prompt(&ctx, &qalam_kit::default_apps::SARD, &crate::config_dir().join("default-apps-asked")); }
        let drop_at = self.drop_at.take();
        for p in dropped {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(), "pptx" | "potx" | "ppsx") { self.open_path(p); }
            else {
                self.insert_picture_file(&p);
                // Placed where it was dropped (centered on the pointer), kept on the slide.
                if let (Some((x, y)), Some(l)) = (drop_at, self.selection.first().cloned()) {
                    if let Some(mut f) = self.pres.xfrm(&l) {
                        let (sw, sh) = self.pres.size();
                        f.x = (x - f.w / 2.0).clamp(0.0, (sw - f.w).max(0.0));
                        f.y = (y - f.h / 2.0).clamp(0.0, (sh - f.h).max(0.0));
                        self.pres.set_xfrm(&l, &f);
                    }
                }
            }
        }
        if self.show.is_some() && !self.reading { crate::show::frame(self, ui); return; }
        self.shortcuts(&ctx);
        crate::ribbon::ribbon(self, ui);
        self.status_bar(ui);
        if self.show_notes && matches!(self.view, crate::views::View::Normal | crate::views::View::Outline) { self.notes_panel(ui); }
        crate::panes::panel(self, ui);
        crate::views::dialogs(self, &ctx);
        if self.reading {
            // Reading View: the show in this window, under the ribbon.
            if self.show.is_some() { crate::show::frame(self, ui); }
            if self.show.is_none() { self.reading = false; crate::views::set_view(self, crate::views::View::Normal); }
            self.dialogs(&ctx);
            return;
        }
        // (The ribbon and panes may have changed the slide this frame.)
        *self.scene_cache.borrow_mut() = None;
        self.prune_thumbs();
        if !crate::views::side(self, ui) { crate::nav::panel(self, ui); }
        if crate::views::central(self, ui) { self.dialogs(&ctx); crate::dialogs::show(self, &ctx); crate::filemenu::dialogs(self, &ctx); return; }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(if self.dark { Color32::from_gray(0x24) } else { Color32::from_rgb(0xE6, 0xE6, 0xE6) })).show(ui, |ui| canvas::canvas(self, ui));
        self.dialogs(&ctx);
        crate::dialogs::show(self, &ctx);
        crate::filemenu::dialogs(self, &ctx);
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing_elsewhere = ctx.memory(|m| m.focused().is_some());
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: egui::Key| ctx.input_mut(|i| i.consume_key(if cmd { egui::Modifiers::COMMAND } else { egui::Modifiers::NONE } | if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE }, k));
        if cmd {
            if pressed(egui::Key::S) { self.save(shift); }
            if pressed(egui::Key::O) { self.guarded(crate::filemenu::Pending::Open); }
            if pressed(egui::Key::N) { self.guarded(crate::filemenu::Pending::New); }
            if pressed(egui::Key::W) { self.guarded(crate::filemenu::Pending::Close); }
            if pressed(egui::Key::Q) { self.guarded(crate::filemenu::Pending::Quit); }
            if pressed(egui::Key::Comma) { self.options = Some(self.prefs.clone()); }
            if pressed(egui::Key::P) && self.show.is_none() { self.file_dialog = Some(crate::filemenu::FileDialog::Print { o: slides_engine::export::PrintOptions { slides: (0..self.slides().len()).collect(), ..Default::default() }, range: 0, custom: String::new(), page: 0, printer: String::new(), printers: vec![], copies: 1, export: false, preview: None }); }
            if pressed(egui::Key::M) { self.new_slide(None); }
            if typing_elsewhere { return; }
            if pressed(egui::Key::Z) { if shift { self.redo(); } else { self.undo(); } }
            if pressed(egui::Key::Y) { self.redo(); }
            if self.edit.is_none() {
                if pressed(egui::Key::C) { self.copy(); }
                if pressed(egui::Key::X) { self.cut(); }
                if pressed(egui::Key::V) { self.paste(); }
                if pressed(egui::Key::D) { self.duplicate_selection(); }
            }
            if pressed(egui::Key::F) { self.dialog = Some(crate::dialogs::Dialog::Find { query: String::new(), with: String::new(), match_case: false, whole: false, replace: false, last: None }); }
            if pressed(egui::Key::H) { self.dialog = Some(crate::dialogs::Dialog::Find { query: String::new(), with: String::new(), match_case: false, whole: false, replace: true, last: None }); }
            if pressed(egui::Key::K) { self.dialog = Some(crate::dialogs::Dialog::Link { target: 0, address: String::new(), slide: 0, text_only: false }); }
        }
        if !cmd && !typing_elsewhere {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) { self.start_show(0); }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F5)) { self.start_show(self.current); }
        }
    }

    /// Play the current slide's transition on the canvas (from the previous slide).
    pub fn start_preview(&mut self) {
        let slides = self.slides();
        let from = if self.current > 0 { slides.get(self.current - 1).cloned() } else { None };
        let tex = from.and_then(|f| {
            let (_, scale) = self.canvas.view?;
            let ppp = self.ctx.pixels_per_point();
            let scene = self.pres.scene(&f);
            let pm = slides_engine::render::render(&self.pres, &scene, &mut self.images, slides_engine::render::Options { scale: (scale * ppp) as f64, prompts: false, skip_text_of: None, translate: None })?;
            Some(self.ctx.load_texture("preview-from", egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR))
        });
        if let Some(s) = self.slide() { if let Some(w) = self.pres.transition_sound(&s) { crate::transitions_tab::play_sound(&w); } }
        self.morph_preview = None;
        if let (Some(cur), true) = (self.slide(), self.current > 0) {
            if self.pres.slide_transition(&cur).effect == "morph" {
                if let Some((_, scale)) = self.canvas.view {
                    let (sw, _) = self.pres.size();
                    let px = (sw as f32 * scale * self.ctx.pixels_per_point()).round().max(1.0) as u32;
                    let ctx = self.ctx.clone();
                    let prev = slides[self.current - 1].clone();
                    self.morph_preview = crate::morph::build(self, &ctx, &prev, &cur, px);
                }
            }
        }
        self.preview = Some((self.ctx.input(|i| i.time), tex));
    }

    pub fn start_show(&mut self, from: usize) {
        self.edit = None;
        self.show = Some(crate::show::Show::new(self, from, None, None));
        self.full_screen_show();
    }

    /// Play these slides (a custom show), optionally rehearsing (Some(record)).
    pub fn start_show_with(&mut self, order: Vec<usize>, rehearse: Option<bool>) {
        self.edit = None;
        let from = order.first().copied().unwrap_or(0);
        self.show = Some(crate::show::Show::new(self, from, Some(order), rehearse));
        self.full_screen_show();
    }

    fn full_screen_show(&mut self) {
        let browsed = self.show.as_ref().is_some_and(|s| s.setup.kind == slides_engine::show_setup::ShowType::Browsed);
        // With Presenter View, the audience gets its own full-screen window.
        let presenter = self.show.as_ref().is_some_and(|s| s.setup.presenter_view);
        if browsed || presenter { return; }
        // On the chosen monitor: the window moves there first, and back afterwards.
        if let Some((pos, _)) = self.show.as_ref().and_then(|s| crate::views::show_monitor(&s.setup)) {
            self.before_show = self.ctx.input(|i| i.viewport().outer_rect);
            self.ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos + egui::vec2(40.0, 40.0)));
        }
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let n = self.slides().len();
        egui::Panel::bottom("status").frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin::symmetric(10, 3))).show(ui, |ui| {
            ui.with_layout(qalam_kit::widgets::row_layout(), |ui| {
                ui.label(RichText::new(crate::i18n::trf("Slide {} of {}", &[&(self.current + 1).min(n), &n])).size(12.0).color(c_ink2()));
                if let Some((t, at)) = &self.notice {
                    if ui.input(|i| i.time) - at < 3.0 { ui.label(RichText::new(t).size(12.0).color(ACCENT)); } else { self.notice = None; }
                }
                ui.with_layout(qalam_kit::widgets::far_layout(), |ui| {
                    if ui.small_button(RichText::new(tr("Fit")).size(12.0)).on_hover_text(tr("Fit slide to window")).clicked() { self.zoom = None; }
                    let mut z = self.zoom.unwrap_or(self.canvas.fit_zoom) * 100.0;
                    if ui.add(egui::Slider::new(&mut z, 25.0..=400.0).show_value(false).logarithmic(true)).changed() { self.zoom = Some(z / 100.0); }
                    ui.label(RichText::new(format!("{:.0}%", z)).size(12.0).color(c_ink2()));
                    if qalam_kit::widgets::small(ui, "slideShowSmall", None, false, true, "Slide Show (F5)").clicked() { self.start_show(self.current); }
                    if qalam_kit::widgets::small(ui, "notesPane", Some("Notes"), self.show_notes, true, "Show or hide the notes").clicked() { self.show_notes = !self.show_notes; }
                });
            });
        });
    }

    fn notes_panel(&mut self, ui: &mut egui::Ui) {
        let Some(slide) = self.slide() else { return };
        if self.notes.0 != slide { self.notes = (slide.clone(), self.pres.notes_text(&slide)); }
        egui::Panel::bottom("notes").resizable(true).default_size(90.0).min_size(40.0).frame(egui::Frame::NONE.fill(c_card()).inner_margin(egui::Margin::symmetric(12, 6)).stroke(egui::Stroke::new(1.0, c_line()))).show(ui, |ui| {
            let mut text = self.notes.1.clone();
            let r = ui.add_sized(ui.available_size(), egui::TextEdit::multiline(&mut text).frame(egui::Frame::NONE).hint_text(tr("Click to add notes")));
            if r.changed() {
                self.checkpoint("notes");
                self.pres.set_notes(&slide, &text);
                self.notes.1 = text;
                self.last_change = Some("typing");
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(msg) = self.error.clone() {
            let mut open = true;
            let mut ok = false;
            crate::theme::dialog(ctx, &crate::app_name()).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(msg);
                crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { ok = true; });
            });
            if !open || ok { self.error = None; }
        }
        if let Some((mut q, mut r)) = self.find.clone() {
            let mut open = true;
            let mut replace = false;
            crate::theme::dialog(ctx, "Replace").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                crate::theme::grid(ui, egui::Grid::new("find").num_columns(2), |ui| {
                    ui.label(tr("Find what"));
                    ui.add(egui::TextEdit::singleline(&mut q).desired_width(240.0));
                    ui.end_row();
                    ui.label(tr("Replace with"));
                    ui.add(egui::TextEdit::singleline(&mut r).desired_width(240.0));
                    ui.end_row();
                });
                crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Replace All").clicked() { replace = true; });
            });
            if replace { self.replace_all(&q, &r); }
            self.find = if open { Some((q, r)) } else { None };
        }
    }
}

impl eframe::App for SlidesApp {
    /// Clear to transparent while the splash card shows (its window has no background).
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.splash.is_some() { [0.0; 4] } else { visuals.panel_fill.to_normalized_gamma_f32() }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}
