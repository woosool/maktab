//! View tab: Print Layout, Web Layout, Outline and Draft views; Immersive Reader; units;
//! zoom; split window and side-by-side documents; a new window; document properties; and
//! dark mode.

use crate::app::WordApp;
use egui::{Color32, RichText, Ui};
use std::cell::Cell;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode { #[default] Print, Web, Outline, Draft }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Units { #[default] Inches, Centimeters, Millimeters, Points, Picas }

thread_local! {
    pub static UNITS: Cell<Units> = const { Cell::new(Units::Inches) };
}
pub use qalam_kit::colors::DARK;

impl Units {
    pub const ALL: [(Units, &'static str); 5] = [(Units::Inches, "Inches"), (Units::Centimeters, "Centimeters"), (Units::Millimeters, "Millimeters"), (Units::Points, "Points"), (Units::Picas, "Picas")];
    pub fn name(&self) -> &'static str { Self::ALL.iter().find(|u| u.0 == *self).map(|u| u.1).unwrap_or("Inches") }
    /// Ruler: px per numbered step at 100%, ticks per step, and the number each step adds.
    pub fn ruler(&self) -> (f32, i32, i32) {
        match self { Units::Inches => (96.0, 8, 1), Units::Centimeters => (37.795, 4, 1), Units::Millimeters => (37.795, 2, 10), Units::Points => (48.0, 4, 36), Units::Picas => (16.0, 2, 1) }
    }
    fn per_twip(&self) -> f32 { match self { Units::Inches => 1.0 / 1440.0, Units::Centimeters => 2.54 / 1440.0, Units::Millimeters => 25.4 / 1440.0, Units::Points => 1.0 / 20.0, Units::Picas => 1.0 / 240.0 } }
    pub fn from_twips(&self, t: i32) -> f32 { t as f32 * self.per_twip() }
    pub fn to_twips(&self, v: f32) -> i32 { (v / self.per_twip()).round() as i32 }
    pub fn suffix(&self) -> &'static str { match self { Units::Inches => " in", Units::Centimeters => " cm", Units::Millimeters => " mm", Units::Points => " pt", Units::Picas => " pi" } }
    pub fn speed(&self) -> f64 { match self { Units::Inches => 0.01, Units::Centimeters => 0.02, Units::Millimeters => 0.2, Units::Points => 0.5, Units::Picas => 0.05 } }
}

/// The unit setting for the current frame.
pub fn units() -> Units { UNITS.with(|u| u.get()) }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageTint { None, Sepia, Inverse }

pub struct Immersive { pub width: f32, pub tint: PageTint, pub spacing: bool, pub syllables: bool, pub focus: Option<usize>, pub current: usize }

pub struct SideBySide { pub doc: word_engine::Document, pub layout: word_engine::Layout, pub cache: word_engine::layout::MeasureCache, pub name: String }

#[derive(Clone)]
pub struct PropsDraft { pub title: String, pub subject: String, pub author: String, pub keywords: String, pub description: String }

#[derive(Default)]
pub struct ViewState {
    pub mode: ViewMode,
    pub units: Units,
    pub immersive: Option<Immersive>,
    pub split: bool,
    pub side: Option<SideBySide>,
    pub dark: bool,
    pub page_grid: bool,
    pub properties: Option<PropsDraft>,
    pub zoom_dialog: Option<(f32, u8)>,
    pub outline_level: u8,
    pub collapsed: HashSet<u64>,
    pub canvas_width: f32,
    pub canvas_height: f32,
    pub applied: (ViewMode, i32),
    pub multi_page: bool,
    /// Which view the canvas is drawing: 0 the main one, 1 the split's top, 2 side by side.
    pub pane: usize,
    /// Each view's scroll offset, its movement last frame, and whether that movement was
    /// following its partner.
    pub offsets: [egui::Vec2; 3],
    pub deltas: [egui::Vec2; 3],
    pub followed: [bool; 3],
    /// Synchronous Scrolling for Split and for View Side by Side.
    pub sync_split: bool,
    pub sync_side: bool,
}

impl ViewState {
    fn partner(&self, pane: usize) -> Option<usize> {
        match pane {
            1 if self.split && self.sync_split => Some(0),
            2 if self.side.is_some() && self.sync_side => Some(0),
            0 if self.split && self.sync_split => Some(1),
            0 if self.side.is_some() && self.sync_side => Some(2),
            _ => None,
        }
    }

    /// The offset a view should take to follow its partner's scrolling, if it moved.
    pub fn follow(&self, pane: usize) -> Option<egui::Vec2> {
        let j = self.partner(pane)?;
        let d = self.deltas[j];
        (d != egui::Vec2::ZERO && !self.followed[j]).then(|| (self.offsets[pane] + d).max(egui::Vec2::ZERO))
    }

    /// Note a view's offset after drawing; true when it moved.
    pub fn scrolled(&mut self, pane: usize, offset: egui::Vec2, followed: bool) -> bool {
        let d = offset - self.offsets[pane];
        self.deltas[pane] = if d.length() < 0.01 { egui::Vec2::ZERO } else { d };
        self.offsets[pane] = offset;
        self.followed[pane] = followed;
        self.deltas[pane] != egui::Vec2::ZERO
    }
}

impl WordApp {
    /// The page the current view lays out on (Web Layout and Draft differ from the real page).
    pub fn page_geometry(&self) -> word_engine::page::PageGeometry {
        self.doc.view_override.as_ref().map(|v| v.0.geometry()).unwrap_or_else(|| self.doc.page.geometry())
    }

    /// Lay out for the current view: Web Layout uses the window's width with no page breaks,
    /// Draft a continuous page; Print Layout and the others the real pages.
    pub fn apply_view_override(&mut self) {
        let v = &self.extras.view;
        let width_twips = ((v.canvas_width.max(320.0) - 40.0) / self.zoom.max(0.1) * 15.0) as i32;
        let key = (v.mode, if v.mode == ViewMode::Web { width_twips / 300 } else { 0 });
        if key == v.applied && (self.doc.view_override.is_some() == matches!(v.mode, ViewMode::Web | ViewMode::Draft)) { return; }
        self.extras.view.applied = key;
        self.doc.view_override = match self.extras.view.mode {
            ViewMode::Web => { let mut p = self.doc.page.clone(); p.width = width_twips.max(4000); p.height = 9_000_000; p.left = 360; p.right = 360; p.top = 360; p.bottom = 360; p.gutter = 0; Some((p, true)) }
            ViewMode::Draft => { let mut p = self.doc.page.clone(); p.height = 9_000_000; p.top = 360; p.bottom = 360; Some((p, true)) }
            _ => None,
        };
        self.layout.pages.clear();
        self.measure_cache = Default::default();
    }

    pub fn set_view(&mut self, mode: ViewMode) {
        self.extras.view.mode = mode;
        self.read_mode = false;
        self.extras.view.immersive = None;
        self.apply_view_override();
    }

    pub fn open_side_by_side(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Documents", &["docx", "md", "txt"]).pick_file() else { return };
        let bytes = match std::fs::read(&p) { Ok(b) => b, Err(e) => { self.dialogs.error = Some(e.to_string()); return; } };
        let doc = match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "md" => Ok(word_engine::import::from_markdown(&String::from_utf8_lossy(&bytes))),
            "txt" => Ok(word_engine::import::from_text(&String::from_utf8_lossy(&bytes))),
            _ => word_engine::docx::read(&bytes),
        };
        match doc {
            Ok(doc) => { let layout = word_engine::layout_document(&doc); self.extras.view.side = Some(SideBySide { doc, layout, cache: Default::default(), name: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() }); }
            Err(e) => self.dialogs.error = Some(crate::i18n::trf("Couldn't open {}: {}", &[&p.display(), &e])),
        }
    }

    /// New Window: another window on this document (saved documents open from their file).
    pub fn new_window(&mut self) {
        let Ok(exe) = std::env::current_exe() else { return };
        let mut cmd = std::process::Command::new(exe);
        if let Some(p) = &self.path { cmd.arg(p); }
        if let Err(e) = cmd.spawn() { self.dialogs.error = Some(format!("Couldn't open a new window: {e}")); }
    }

    pub fn open_properties(&mut self) {
        let p = &self.doc.properties;
        self.extras.view.properties = Some(PropsDraft { title: p.title.clone(), subject: p.subject.clone(), author: p.author.clone(), keywords: p.keywords.clone(), description: p.description.clone() });
    }

    pub fn set_dark(&mut self, ctx: &egui::Context, on: bool) {
        self.extras.view.dark = on;
        DARK.with(|d| d.set(on));
        crate::theme::apply_mode(ctx, on);
    }
}

/// The central area for views that don't show pages.
pub fn central(app: &mut WordApp, ui: &mut Ui) -> bool {
    if app.extras.view.immersive.is_some() { immersive(app, ui); return true; }
    if app.extras.view.mode == ViewMode::Outline { outline(app, ui); return true; }
    false
}

fn heading_level(app: &WordApp, i: usize) -> u8 { app.doc.para_format(&app.doc.paragraphs[i]).outline_level }

fn set_level(app: &mut WordApp, i: usize, level: u8) {
    let s = app.doc.paragraph_start(i);
    app.doc.select(s, s);
    if level >= 9 { app.doc.set_paragraph_style("Normal"); }
    else if app.doc.styles.get(&format!("Heading{}", level + 1)).is_some() || level < 3 { app.doc.set_paragraph_style(&format!("Heading{}", level + 1)); }
    else { app.doc.set_para_props(word_engine::ParaProps { outline_level: Some(level), ..Default::default() }); }
}

fn outline(app: &mut WordApp, ui: &mut Ui) {
    let (cur, _) = app.doc.locate(app.doc.range().0);
    crate::theme::hrow(ui, |ui| {
        ui.add_space(12.0);
        let lvl = heading_level(app, cur);
        if ui.button(crate::i18n::tr("« Heading 1")).on_hover_text(crate::i18n::tr("Promote to Heading 1")).clicked() { set_level(app, cur, 0); }
        if ui.button(crate::i18n::tr("‹ Promote")).clicked() { let to = if lvl >= 9 { app.doc.paragraphs[..cur].iter().rev().map(|p| app.doc.para_format(p).outline_level).find(|l| *l < 9).unwrap_or(0) } else { lvl.saturating_sub(1) }; set_level(app, cur, to); }
        if ui.button(crate::i18n::tr("Demote ›")).clicked() { if lvl < 8 { set_level(app, cur, lvl + 1); } }
        if ui.button(crate::i18n::tr("Body Text »")).clicked() { set_level(app, cur, 9); }
        ui.separator();
        if ui.button(crate::i18n::tr("▲ Move Up")).clicked() { app.doc.move_paragraphs(true); }
        if ui.button(crate::i18n::tr("▼ Move Down")).clicked() { app.doc.move_paragraphs(false); }
        ui.separator();
        ui.label(crate::i18n::tr("Show Level:"));
        let v = &mut app.extras.view.outline_level;
        crate::theme::Combo::from_id_salt("outline-level").width(110.0).selected_text(crate::i18n::tr(&(if *v == 0 { "All Levels".to_string() } else { crate::i18n::trf("Level {}", &[&v]) }))).show_ui(ui, |ui| {
            ui.selectable_value(v, 0, "All Levels");
            for k in 1..=9u8 { ui.selectable_value(v, k, crate::i18n::trf("Level {}", &[&k])); }
        });
        ui.separator();
        if crate::theme::primary(ui, "Close Outline View").clicked() { app.set_view(ViewMode::Print); }
    });
    ui.separator();
    let show = app.extras.view.outline_level;
    let mut hide_below: Option<u8> = None;
    let mut click = None;
    let mut toggle = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.add_space(8.0);
        for i in 0..app.doc.paragraphs.len() {
            let p = &app.doc.paragraphs[i];
            if p.cell.is_some() { continue; }
            let level = heading_level(app, i);
            if let Some(h) = hide_below { if level > h { continue; } hide_below = None; }
            if show > 0 && level >= show { continue; }
            if level < 9 && app.extras.view.collapsed.contains(&p.id) { hide_below = Some(level); }
            let indent = if level < 9 { level as f32 * 26.0 } else { (app.doc.paragraphs[..i].iter().rev().map(|q| app.doc.para_format(q).outline_level).find(|l| *l < 9).map(|l| l + 1).unwrap_or(0)) as f32 * 26.0 };
            crate::theme::hrow(ui, |ui| {
                ui.add_space(24.0 + indent);
                let has_children = level < 9 && app.doc.paragraphs.get(i + 1).is_some_and(|n| n.cell.is_none());
                let sym = if level < 9 { if app.extras.view.collapsed.contains(&p.id) { "⊕" } else if has_children { "⊖" } else { "⊕" } } else { "•" };
                if ui.add(egui::Button::new(RichText::new(crate::i18n::tr(&sym)).size(14.0)).frame(false)).on_hover_text(crate::i18n::tr("Expand or collapse")).clicked() && level < 9 { toggle = Some(p.id); }
                let text = crate::i18n::clean(&p.text());
                let rich = if level < 9 { RichText::new(if text.is_empty() { " ".into() } else { text }).size(18.0 - level.min(4) as f32).family(egui::FontFamily::Name("ui-bold".into())) } else { RichText::new(text).size(14.0) };
                let selected = i == cur;
                if ui.selectable_label(selected, rich).clicked() { click = Some(i); }
            });
        }
    });
    if let Some(id) = toggle { if !app.extras.view.collapsed.remove(&id) { app.extras.view.collapsed.insert(id); } }
    if let Some(i) = click { let s = app.doc.paragraph_start(i); app.doc.select(s, s); }
}

fn syllables(text: &str) -> String {
    text.split(' ').map(|w| word_engine::proofing::syllables(w).join("·")).collect::<Vec<_>>().join(" ")
}

fn immersive(app: &mut WordApp, ui: &mut Ui) {
    let Some(im) = app.extras.view.immersive.as_mut() else { return };
    let (bg, fg) = match im.tint { PageTint::None => (Color32::WHITE, Color32::from_gray(30)), PageTint::Sepia => (Color32::from_rgb(0xF4, 0xEC, 0xD8), Color32::from_rgb(0x4A, 0x3B, 0x2A)), PageTint::Inverse => (Color32::from_gray(24), Color32::from_gray(235)) };
    let mut close = false;
    let mut speak = false;
    egui::Frame::NONE.fill(Color32::from_rgb(0xF3, 0xF3, 0xF3)).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
        crate::theme::hrow(ui, |ui| {
            ui.label(RichText::new(crate::i18n::tr("Immersive Reader")).strong());
            ui.separator();
            ui.label(crate::i18n::tr("Column Width"));
            crate::theme::Combo::from_id_salt("im-width").width(110.0).selected_text(crate::i18n::tr(&(match im.width as i32 { 480 => "Very Narrow", 640 => "Narrow", 800 => "Moderate", _ => "Wide" }))).show_ui(ui, |ui| { for (w, n) in [(480.0, "Very Narrow"), (640.0, "Narrow"), (800.0, "Moderate"), (1100.0, "Wide")] { ui.selectable_value(&mut im.width, w, crate::i18n::tr(&n)); } });
            ui.label(crate::i18n::tr("Page Color"));
            crate::theme::Combo::from_id_salt("im-tint").width(90.0).selected_text(crate::i18n::tr(&(match im.tint { PageTint::None => "None", PageTint::Sepia => "Sepia", PageTint::Inverse => "Inverse" }))).show_ui(ui, |ui| { for (t, n) in [(PageTint::None, "None"), (PageTint::Sepia, "Sepia"), (PageTint::Inverse, "Inverse")] { ui.selectable_value(&mut im.tint, t, crate::i18n::tr(&n)); } });
            ui.label(crate::i18n::tr("Line Focus"));
            crate::theme::Combo::from_id_salt("im-focus").width(90.0).selected_text(crate::i18n::tr(&(match im.focus { None => "Off".to_string(), Some(n) => crate::i18n::trf("{} lines", &[&n]) }))).show_ui(ui, |ui| { ui.selectable_value(&mut im.focus, None, crate::i18n::tr("Off")); for n in [1usize, 3, 5] { ui.selectable_value(&mut im.focus, Some(n), crate::i18n::trf("{} lines", &[&n])); } });
            crate::theme::check(ui, &mut im.spacing, crate::i18n::tr("Text Spacing"));
            crate::theme::check(ui, &mut im.syllables, crate::i18n::tr("Syllables"));
            if ui.button(crate::i18n::tr("Read Aloud")).clicked() { speak = true; }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| if crate::theme::primary(ui, "Close Immersive Reader").clicked() { close = true; });
        });
    });
    let paras: Vec<String> = app.doc.paragraphs.iter().filter(|p| p.cell.is_none()).map(|p| crate::i18n::clean(&p.text())).filter(|t| !t.trim().is_empty()).collect();
    let im = app.extras.view.immersive.as_mut().unwrap();
    if im.focus.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) { im.current = (im.current + 1).min(paras.len().saturating_sub(1)); }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) { im.current = im.current.saturating_sub(1); }
    }
    egui::Frame::NONE.fill(bg).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let w = im.width.min(ui.available_width() - 40.0);
            let pad = ((ui.available_width() - w) / 2.0).max(20.0);
            ui.add_space(30.0);
            let spacing = if im.spacing { 1.8 } else { 1.0 };
            for (k, t) in paras.iter().enumerate() {
                let dim = im.focus.is_some_and(|n| (k as isize - im.current as isize).unsigned_abs() > n / 2);
                let text = if im.syllables { syllables(t) } else { t.clone() };
                let color = if dim { fg.gamma_multiply(0.15) } else { fg };
                crate::theme::hrow(ui, |ui| {
                    ui.add_space(pad);
                    ui.allocate_ui(egui::vec2(w, 0.0), |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0 * spacing;
                        let mut job = egui::text::LayoutJob::single_section(text, egui::TextFormat { font_id: egui::FontId::proportional(22.0), color, extra_letter_spacing: if im.spacing { 1.5 } else { 0.0 }, line_height: Some(22.0 * 1.5 * spacing), ..Default::default() });
                        job.wrap.max_width = w;
                        let r = ui.add(egui::Label::new(job).sense(egui::Sense::click()));
                        if r.clicked() { im.current = k; }
                    });
                });
                ui.add_space(14.0 * spacing);
            }
            ui.add_space(60.0);
        });
    });
    if speak { app.toggle_read_aloud(); }
    if close { app.extras.view.immersive = None; }
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    if let Some(mut d) = app.extras.view.properties.take() {
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        crate::theme::dialog(ctx, "Properties").open(&mut open).default_width(460.0).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::grid(ui, egui::Grid::new("props").num_columns(2).spacing(egui::vec2(12.0, 8.0)), |ui| {
                for (label, v) in [("Title", &mut d.title), ("Subject", &mut d.subject), ("Author", &mut d.author), ("Keywords", &mut d.keywords)] { ui.label(crate::i18n::tr(label)); ui.add(egui::TextEdit::singleline(v).desired_width(300.0)); ui.end_row(); }
                ui.label(crate::i18n::tr("Comments")); ui.add(egui::TextEdit::multiline(&mut d.description).desired_rows(3).desired_width(300.0)); ui.end_row();
            });
            crate::theme::section(ui, "Statistics");
            crate::theme::grid(ui, egui::Grid::new("stats").num_columns(2), |ui| {
                for (k, v) in [("Pages", app.layout.page_count()), ("Words", app.doc.word_count()), ("Characters", app.doc.character_count()), ("Paragraphs", app.doc.paragraphs.iter().filter(|p| !p.is_empty()).count())] { ui.label(crate::i18n::tr(k)); ui.label(v.to_string()); ui.end_row(); }
                ui.label(crate::i18n::tr("Location")); ui.label(app.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "Not saved".into())); ui.end_row();
            });
            crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok {
            let p = &mut app.doc.properties;
            (p.title, p.subject, p.author, p.keywords, p.description) = (d.title.clone(), d.subject.clone(), d.author.clone(), d.keywords.clone(), d.description.clone());
            app.doc.dirty = true;
        }
        if open && !ok && !cancel { app.extras.view.properties = Some(d); } else { app.canvas_focus = true; }
    }
    if let Some((mut z, mut preset)) = app.extras.view.zoom_dialog.take() {
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        crate::theme::dialog(ctx, "Zoom").open(&mut open).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::hrow(ui, |ui| {
                crate::theme::vcol(ui, |ui| {
                    for (p, n, v) in [(1u8, "200%", 2.0f32), (2, "100%", 1.0), (3, "75%", 0.75), (4, "50%", 0.5)] { if crate::theme::radio(ui, preset == p, n).clicked() { preset = p; z = v; } }
                });
                ui.add_space(24.0);
                crate::theme::vcol(ui, |ui| {
                    if crate::theme::radio(ui, preset == 5, crate::i18n::tr("Page width")).clicked() { preset = 5; }
                    if crate::theme::radio(ui, preset == 6, crate::i18n::tr("Whole page")).clicked() { preset = 6; }
                    if crate::theme::radio(ui, preset == 7, crate::i18n::tr("Many pages")).clicked() { preset = 7; }
                });
            });
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Percent:")); let mut pct = (z * 100.0).round(); if ui.add(egui::DragValue::new(&mut pct).range(25.0..=300.0).suffix("%")).changed() { z = pct / 100.0; preset = 0; } });
            crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok {
            match preset { 5 => app.fit_width = true, 6 => app.fit_page(1), 7 => app.fit_page(2), _ => app.set_zoom(z) }
        }
        if open && !ok && !cancel { app.extras.view.zoom_dialog = Some((z, preset)); } else { app.canvas_focus = true; }
    }
}

impl WordApp {
    /// One Page / Multiple Pages: zoom so `n` pages fit side by side in the window height.
    pub fn fit_page(&mut self, n: usize) {
        let g = self.doc.page.geometry();
        let h = self.extras.view.canvas_height.max(300.0) - 40.0;
        let w = self.extras.view.canvas_width.max(300.0) - 40.0;
        let z = (h / g.height).min(w / (g.width * n as f32 + 24.0 * (n as f32 - 1.0)));
        self.set_zoom(z.clamp(0.1, 3.0));
        self.extras.view.multi_page = n > 1;
    }
}
