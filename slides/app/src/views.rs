//! The View tab (Normal, Outline View, Slide Sorter, Notes Page, Reading View; Slide,
//! Handout and Notes Master; Ruler, Gridlines, Guides, Notes; Zoom; Color / Grayscale /
//! Black and White; New Window) with each view's screen, the Slide Show tab (From Beginning,
//! From Current Slide, Present Online, Custom Slide Show; Set Up Slide Show, Hide Slide,
//! Rehearse Timings, Record; Play Narrations, Use Timings, Show Media Controls; Monitor,
//! Use Presenter View) and their dialogs.

use crate::app::{SlidesApp, Tab};
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::render::{self, Options};
use slides_engine::show_setup::{Range, ShowSetup, ShowType};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum View { #[default] Normal, Outline, Sorter, NotesPage, Reading, SlideMaster, HandoutMaster, NotesMaster }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorMode { #[default] Color, Grayscale, BlackWhite }

/// Handout and notes master settings (how printed pages are laid out).
#[derive(Clone, Debug, PartialEq)]
pub struct PageMaster { pub per_page: usize, pub landscape: bool, pub header: bool, pub date: bool, pub footer: bool, pub number: bool, pub image: bool, pub body: bool }
impl Default for PageMaster { fn default() -> Self { PageMaster { per_page: 3, landscape: false, header: true, date: true, footer: true, number: true, image: true, body: true } } }

impl PageMaster {
    fn to_text(&self) -> String {
        let b = |v: bool| if v { '1' } else { '0' };
        format!("{}|{}{}{}{}{}{}{}", self.per_page, b(self.landscape), b(self.header), b(self.date), b(self.footer), b(self.number), b(self.image), b(self.body))
    }
    fn from_text(t: &str, default: PageMaster) -> PageMaster {
        let Some((n, f)) = t.split_once('|') else { return default };
        let f: Vec<bool> = f.chars().map(|c| c == '1').collect();
        if f.len() < 7 { return default; }
        PageMaster { per_page: n.parse().unwrap_or(default.per_page), landscape: f[0], header: f[1], date: f[2], footer: f[3], number: f[4], image: f[5], body: f[6] }
    }
}

/// The handout and notes masters as saved in the presentation.
pub fn load_page_masters(app: &mut SlidesApp) {
    app.handout = app.pres.page_master("handout").map(|t| PageMaster::from_text(&t, PageMaster::default())).unwrap_or_default();
    let nd = PageMaster { per_page: 1, ..PageMaster::default() };
    app.notes_master = app.pres.page_master("notes").map(|t| PageMaster::from_text(&t, nd.clone())).unwrap_or(nd);
}

/// The displays: (name, position, size in points, primary).
pub fn monitors() -> Vec<(String, egui::Pos2, egui::Vec2, bool)> {
    let mut v: Vec<_> = display_info::DisplayInfo::all().unwrap_or_default().into_iter().map(|d| {
        let name = if d.friendly_name.trim().is_empty() { d.name.clone() } else { d.friendly_name.clone() };
        let (w, h) = if cfg!(target_os = "macos") { (d.width as f32, d.height as f32) } else { (d.width as f32 / d.scale_factor.max(0.5), d.height as f32 / d.scale_factor.max(0.5)) };
        (format!("{name} ({}×{})", d.width, d.height), pos2(d.x as f32, d.y as f32), vec2(w, h), d.is_primary)
    }).collect();
    v.sort_by_key(|m| !m.3);
    v
}

/// Where the show plays: the chosen monitor, or automatically the secondary one when
/// presenting with Presenter View (else the one the window is on).
pub fn show_monitor(setup: &ShowSetup) -> Option<(egui::Pos2, egui::Vec2)> {
    let list = monitors();
    if !setup.monitor.is_empty() { if let Some(m) = list.iter().find(|m| m.0 == setup.monitor) { return Some((m.1, m.2)); } }
    if setup.presenter_view { return list.iter().find(|m| !m.3).map(|m| (m.1, m.2)); }
    None
}

pub enum ViewDialog {
    SetUp(ShowSetup),
    /// The custom shows being edited, and which one is picked.
    CustomShows { shows: Vec<(String, Vec<String>)>, picked: usize, editing: Option<(usize, String, Vec<String>)> },
    Zoom { percent: f32 },
    GridGuides,
    KeepTimings { times: Vec<(String, u32)>, total: f64 },
    KeepInk { strokes: Vec<(String, Vec<(f64, f64)>, String, f64, f64)> },
    Rename { layout: String, name: String },
}

/// A slide (or layout, master) picture of `width` px, cached by revision.
pub fn thumb(app: &mut SlidesApp, ctx: &egui::Context, part: &str, width: f32, prompts: bool) -> Option<egui::TextureHandle> {
    let px = (width * ctx.pixels_per_point()).round().max(8.0) as u32;
    let key = format!("{part}#{px}#{prompts}");
    if let Some((rev, t)) = app.thumbs.get(&key) { if *rev == app.revision { return Some(t.clone()); } }
    let scene = app.pres.scene(part);
    let pm = render::render(&app.pres, &scene, &mut app.images, Options { scale: px as f64 / scene.w, prompts, skip_text_of: None, translate: Some(crate::i18n::raw) })?;
    let mut img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
    recolor(&mut img, app.color_mode);
    let t = ctx.load_texture(key.clone(), img, egui::TextureOptions::LINEAR);
    app.thumbs.insert(key, (app.revision, t.clone()));
    Some(t)
}

/// Grayscale and black-and-white previews.
pub fn recolor(img: &mut egui::ColorImage, mode: ColorMode) {
    if mode == ColorMode::Color { return; }
    for p in img.pixels.iter_mut() {
        let a = p.a();
        let l = (0.299 * p.r() as f32 + 0.587 * p.g() as f32 + 0.114 * p.b() as f32) as u8;
        let l = if mode == ColorMode::BlackWhite { if a == 0 { 0 } else if l as u16 * 255 / (a.max(1) as u16) > 200 { a } else { 0 } } else { l };
        *p = Color32::from_rgba_premultiplied(l, l, l, a);
    }
}

fn uv() -> Rect { Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)) }

pub fn set_view(app: &mut SlidesApp, v: View) {
    app.edit = None;
    app.selection.clear();
    if matches!(v, View::SlideMaster) {
        let layout = app.slide().and_then(|s| app.pres.layout_of(&s));
        app.master_part = layout.or_else(|| app.pres.masters().first().cloned());
        app.tab = Tab::Master;
    } else if matches!(v, View::HandoutMaster | View::NotesMaster) {
        app.master_part = None;
        load_page_masters(app);
        app.tab = Tab::Master;
    } else {
        app.master_part = None;
        if app.tab == Tab::Master { app.tab = Tab::View; }
    }
    app.view = v;
    app.canvas.key = None;
}

// ---- the View tab ---------------------------------------------------------------------------

pub fn view_tab(app: &mut SlidesApp, ui: &mut Ui) {
    group(ui, "Presentation Views", |ui| {
        for (v, icon, label, tip) in [(View::Normal, "normalView", "Normal", "Edit slides one at a time"), (View::Outline, "outlineView", "Outline View", "Edit the text of every slide as an outline"), (View::Sorter, "slideSorter", "Slide Sorter", "All slides as thumbnails, to reorder"), (View::NotesPage, "notesPage", "Notes Page", "The slide with its notes as a printed page"), (View::Reading, "readingView", "Reading View", "Play the slides in this window")] {
            if large(ui, icon, label, app.view == v, true, tip).clicked() { set_view(app, v); }
        }
    });
    group(ui, "Master Views", |ui| {
        for (v, icon, label, tip) in [(View::SlideMaster, "slideMaster", "Slide Master", "Edit the master and layouts every slide is built on"), (View::HandoutMaster, "handoutMaster", "Handout Master", "The layout of printed handouts"), (View::NotesMaster, "notesMaster", "Notes Master", "The layout of printed notes pages")] {
            if large(ui, icon, label, app.view == v, true, tip).clicked() { set_view(app, v); }
        }
    });
    group(ui, "Show", |ui| {
        rows(ui, |ui| {
            crate::theme::check(ui, &mut app.show_ruler, tr("Ruler"));
            if crate::theme::check(ui, &mut app.show_grid, tr("Gridlines")).changed() {}
            crate::theme::check(ui, &mut app.show_guides, tr("Guides"));
        });
        if large(ui, "notesPane", "Notes", app.show_notes, true, "Show or hide the speaker notes").clicked() { app.show_notes = !app.show_notes; }
        if small(ui, "gridlines", None, false, true, "Grid and Guides…").clicked() { app.view_dialog = Some(ViewDialog::GridGuides); }
    });
    group(ui, "Zoom", |ui| {
        if large(ui, "zoom", "Zoom", false, true, "Choose the zoom level").clicked() { app.view_dialog = Some(ViewDialog::Zoom { percent: app.zoom.unwrap_or(app.canvas.fit_zoom) * 100.0 }); }
        if large(ui, "fitWindow", "Fit to Window", app.zoom.is_none(), true, "Fit the slide to the window").clicked() { app.zoom = None; }
    });
    group(ui, "Color/Grayscale", |ui| {
        rows(ui, |ui| {
            for (m, icon, label) in [(ColorMode::Color, "color", "Color"), (ColorMode::Grayscale, "grayscale", "Grayscale"), (ColorMode::BlackWhite, "grayscale", "Black and White")] {
                if small(ui, icon, Some(label), app.color_mode == m, true, label).clicked() { app.color_mode = m; app.canvas.key = None; app.thumbs.clear(); }
            }
        });
    });
    group(ui, "Window", |ui| {
        if large(ui, "newWindow", "New Window", false, true, "Open this presentation in another window").clicked() { new_window(app); }
        if large(ui, "darkPage", "Dark Mode", app.dark, true, "Dark interface (slides stay as designed)").clicked() { app.dark = !app.dark; }
    });
}

/// Another window on the same file (saved first so both see it).
fn new_window(app: &mut SlidesApp) {
    let Some(path) = app.path.clone() else { app.notify(&tr("Save the presentation first")); return };
    if app.dirty { app.save(false); }
    if let Ok(exe) = std::env::current_exe() { let _ = std::process::Command::new(exe).arg(path).spawn(); }
}

// ---- the Slide Show tab ---------------------------------------------------------------------

pub fn slide_show_tab(app: &mut SlidesApp, ui: &mut Ui) {
    let mut setup = app.pres.show_setup();
    let before = setup.clone();
    group(ui, "Start Slide Show", |ui| {
        if large(ui, "playFromStart", "From Beginning", false, true, "Start from the first slide (F5)").clicked() { app.start_show(0); }
        if large(ui, "playFromCurrent", "From Current Slide", false, true, "Start from this slide (⇧F5)").clicked() { app.start_show(app.current); }
        if large(ui, "present", "Present Online", false, true, "Make a web page of the slides that anyone can open in a browser").clicked() { present_online(app); }
        let cs = large_menu(ui, "customShow", "Custom Slide Show", "Play or make a custom show (a chosen set of slides)");
        crate::theme::menu(&cs, |ui| {
            let shows = app.pres.custom_shows();
            for (name, _, slides) in &shows {
                if ui.button(name).clicked() {
                    let all = app.slides();
                    let order: Vec<usize> = slides.iter().filter_map(|p| all.iter().position(|s| s == p)).collect();
                    if !order.is_empty() { app.start_show_with(order, None); }
                    ui.close();
                }
            }
            if !shows.is_empty() { ui.separator(); }
            if menu_item(ui, "customShow", "Custom Shows…") { open_custom_shows(app); ui.close(); }
        });
    });
    group(ui, "Set Up", |ui| {
        if large(ui, "setUpShow", "Set Up Slide Show", false, true, "Show type, slides, advance, pen color and more").clicked() { app.view_dialog = Some(ViewDialog::SetUp(setup.clone())); }
        if large(ui, "hideSlide", "Hide Slide", app.slide().is_some_and(|s| app.pres.hidden(&s)), true, "Skip this slide in the slide show").clicked() { app.toggle_hidden(); }
        if large(ui, "rehearse", "Rehearse Timings", false, true, "Play the show and keep how long each slide stays up").clicked() { let order = app.pres.show_order(&setup); app.start_show_with(order, Some(false)); }
        let rec = large_menu(ui, "recordShow", "Record Slide Show", "Record the timing and ink of your presentation");
        crate::theme::menu(&rec, |ui| {
            if menu_item(ui, "playFromCurrent", "Record from Current Slide…") { let mut order = app.pres.show_order(&setup); order.retain(|&i| i >= app.current); if order.is_empty() { order.push(app.current); } app.start_show_with(order, Some(true)); ui.close(); }
            if menu_item(ui, "playFromStart", "Record from Beginning…") { let order = app.pres.show_order(&setup); app.start_show_with(order, Some(true)); ui.close(); }
            ui.separator();
            crate::theme::submenu(ui, tr("Clear"), |ui| {
                if ui.button(tr("Clear Timing on Current Slide")).clicked() { if let Some(s) = app.slide() { app.checkpoint("timing"); app.pres.set_timing(&s, None); } ui.close(); }
                if ui.button(tr("Clear Timings on All Slides")).clicked() { app.checkpoint("timing"); for s in app.slides() { app.pres.set_timing(&s, None); } ui.close(); }
                if ui.button(tr("Clear Ink on Current Slide")).clicked() { clear_ink(app, false); ui.close(); }
                if ui.button(tr("Clear Ink on All Slides")).clicked() { clear_ink(app, true); ui.close(); }
            });
        });
        rows(ui, |ui| {
            crate::theme::check(ui, &mut setup.narration, tr("Play Narrations"));
            crate::theme::check(ui, &mut setup.use_timings, tr("Use Timings"));
            crate::theme::check(ui, &mut setup.media_controls, tr("Show Media Controls"));
        });
    });
    group(ui, "Monitors", |ui| {
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                ui.label(RichText::new(tr("Monitor:")).size(12.0).color(c_ink2()));
                let label = if setup.monitor.is_empty() { tr("Automatic") } else { setup.monitor.clone() };
                crate::theme::Combo::from_id_salt("monitor").width(180.0).selected_text(label).show_ui(ui, |ui| {
                    if ui.selectable_label(setup.monitor.is_empty(), tr("Automatic")).clicked() { setup.monitor.clear(); }
                    for (k, (name, _, _, primary)) in monitors().into_iter().enumerate() {
                        let label = format!("{}: {name}{}", k + 1, if primary { format!(" ({})", tr("Primary Monitor")) } else { String::new() });
                        if ui.selectable_label(setup.monitor == name, label).clicked() { setup.monitor = name; }
                    }
                });
            });
            crate::theme::check(ui, &mut setup.presenter_view, tr("Use Presenter View"));
        });
    });
    if setup != before { app.checkpoint("show setup"); app.pres.set_show_setup(&setup); }
}

fn clear_ink(app: &mut SlidesApp, all: bool) {
    let slides = if all { app.slides() } else { app.slide().into_iter().collect() };
    app.checkpoint("ink");
    for s in slides {
        let n = app.pres.slide(&s).and_then(|r| r.path("p:cSld/p:spTree")).map(|t| t.elements().count()).unwrap_or(0);
        for i in (0..n).rev() {
            let loc = slides_engine::scene::Loc { part: s.clone(), path: vec![i] };
            let is_ink = app.pres.element(&loc).and_then(|e| e.find("p:cNvPr")).and_then(|c| c.attr("name")).is_some_and(|n| n.starts_with("Ink "));
            if is_ink { app.pres.delete_shapes(&[loc]); }
        }
    }
}

fn open_custom_shows(app: &mut SlidesApp) {
    let shows = app.pres.custom_shows().into_iter().map(|c| (c.0, c.2)).collect();
    app.view_dialog = Some(ViewDialog::CustomShows { shows, picked: 0, editing: None });
}

/// Present Online: a web page of the slides (pictures, notes, keys to move) opened in the
/// browser, ready to put anywhere people can open it.
fn present_online(app: &mut SlidesApp) {
    let slides = app.pres.show_order(&app.pres.show_setup());
    let all = app.slides();
    let mut imgs = vec![];
    for &i in &slides {
        let Some(part) = all.get(i) else { continue };
        if let Some(png) = render::slide_png(&app.pres, part, 1600) {
            imgs.push(format!("\"data:image/png;base64,{}\"", base64(&png)));
        }
    }
    let title = app.title().replace('<', "&lt;");
    let html = format!(r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title>
<style>html,body{{margin:0;height:100%;background:#000}}img{{position:absolute;inset:0;margin:auto;max-width:100%;max-height:100%}}#n{{position:fixed;right:12px;bottom:8px;color:#aaa;font:13px sans-serif}}</style></head>
<body><img id="s"><div id="n"></div><script>const S=[{}];let i=0;const show=()=>{{s.src=S[i];n.textContent=(i+1)+' / '+S.length}};
addEventListener('keydown',e=>{{if(['ArrowRight','ArrowDown',' ','PageDown'].includes(e.key))i=Math.min(i+1,S.length-1);if(['ArrowLeft','ArrowUp','PageUp','Backspace'].includes(e.key))i=Math.max(i-1,0);if(e.key=='Home')i=0;if(e.key=='End')i=S.length-1;show()}});
addEventListener('click',e=>{{i=e.clientX<innerWidth/3?Math.max(i-1,0):Math.min(i+1,S.length-1);show()}});show();</script></body></html>"#, imgs.join(","));
    let dir = std::env::temp_dir().join("sard-online");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("{}.html", app.title().replace(['/', '\\', ':'], "-")));
    if std::fs::write(&file, html).is_ok() { crate::canvas::open_path(&file.to_string_lossy()); app.notify(&tr("The web page is open in your browser")); }
}

// ---- master tabs ----------------------------------------------------------------------------

pub fn master_tab(app: &mut SlidesApp, ui: &mut Ui) {
    match app.view {
        View::SlideMaster => slide_master_tab(app, ui),
        View::HandoutMaster | View::NotesMaster => page_master_tab(app, ui),
        _ => {}
    }
    group(ui, "Close", |ui| { if large(ui, "normalView", "Close Master View", false, true, "Back to editing slides").clicked() { set_view(app, View::Normal); } });
}

fn slide_master_tab(app: &mut SlidesApp, ui: &mut Ui) {
    let part = app.master_part.clone().unwrap_or_default();
    let is_layout = part.contains("slideLayout");
    let master = if is_layout { app.pres.master_of(&part).unwrap_or_default() } else { part.clone() };
    group(ui, "Edit Master", |ui| {
        if large(ui, "newSlide", "Insert Layout", false, !master.is_empty(), "Add a layout to the master").clicked() {
            app.checkpoint("layout");
            if let Some(l) = app.pres.insert_layout(&master) { app.master_part = Some(l); app.changed(); }
        }
        rows(ui, |ui| {
            if small(ui, "deleteSlide", Some("Delete"), false, is_layout, "Delete this layout (when no slide uses it)").clicked() {
                app.checkpoint("layout");
                if app.pres.delete_layout(&part) { app.master_part = Some(master.clone()); app.changed(); } else { app.notify(&tr("Slides use this layout, so it can't be deleted")); }
            }
            if small(ui, "duplicate", Some("Duplicate Layout"), false, is_layout, "A copy of this layout").clicked() {
                app.checkpoint("layout");
                let name = format!("{} {}", app.pres.layout_name(&part), tr("copy"));
                if let Some(l) = app.pres.duplicate_layout(&part, &name) { app.master_part = Some(l); app.changed(); }
            }
            if small(ui, "textBox", Some("Rename"), false, is_layout, "Rename this layout").clicked() { app.view_dialog = Some(ViewDialog::Rename { layout: part.clone(), name: app.pres.layout_name(&part) }); }
        });
    });
    group(ui, "Master Layout", |ui| {
        rows(ui, |ui| {
            for (kind, label) in [("title", "Title"), ("ftr", "Footers")] {
                let mut on = app.pres.has_placeholder(&part, kind);
                if ui.add_enabled_ui(is_layout, |ui| crate::theme::check(ui, &mut on, tr(label))).inner.changed() {
                    app.checkpoint("layout");
                    app.pres.set_placeholder(&part, kind, on);
                    if kind == "ftr" { app.pres.set_placeholder(&part, "dt", on); app.pres.set_placeholder(&part, "sldNum", on); }
                    app.changed();
                }
            }
        });
    });
    group(ui, "Edit Theme", |ui| {
        let t = large_menu(ui, "themes", "Themes", "Change the master's theme");
        crate::theme::menu(&t, |ui| crate::design_tab::all_themes(app, ui));
        let v = small_menu(ui, "colors", "Colors, Fonts, Effects", "The theme's colors, fonts, effects and background styles");
        crate::theme::menu(&v, |ui| crate::design_tab::variants_menu(app, ui));
    });
    group(ui, "Background", |ui| {
        if large(ui, "formatBackground", "Background Styles", app.pane == crate::app::Pane::Background, true, "Background fill, picture or pattern").clicked() { app.pane = crate::app::Pane::Background; }
        let mut hide = app.pres.hides_background_graphics(&part);
        if crate::theme::check(ui, &mut hide, tr("Hide Background Graphics")).changed() { app.checkpoint("background"); app.pres.set_hide_background_graphics(&part, hide); app.changed(); }
    });
    group(ui, "Size", |ui| {
        if large(ui, "slideSize", "Slide Size", false, true, "Widescreen, standard or a custom size").clicked() { let (w, h) = app.pres.size(); app.dialog = Some(crate::dialogs::Dialog::SlideSize { w: w / 914_400.0, h: h / 914_400.0, fit: None }); }
    });
}

fn page_master_tab(app: &mut SlidesApp, ui: &mut Ui) {
    let handout = app.view == View::HandoutMaster;
    let mut m = if handout { app.handout.clone() } else { app.notes_master.clone() };
    group(ui, "Page Setup", |ui| {
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                ui.label(tr("Orientation"));
                if ui.selectable_label(!m.landscape, tr("Portrait")).clicked() { m.landscape = false; }
                if ui.selectable_label(m.landscape, tr("Landscape")).clicked() { m.landscape = true; }
            });
            if handout {
                crate::theme::hrow(ui, |ui| {
                    ui.label(tr("Slides Per Page"));
                    crate::theme::Combo::from_id_salt("per-page").width(60.0).selected_text(m.per_page.to_string()).show_ui(ui, |ui| for n in [1, 2, 3, 4, 6, 9] { if ui.selectable_label(m.per_page == n, n.to_string()).clicked() { m.per_page = n; } });
                });
            }
        });
    });
    group(ui, "Placeholders", |ui| {
        rows(ui, |ui| {
            crate::theme::check(ui, &mut m.header, tr("Header"));
            crate::theme::check(ui, &mut m.date, tr("Date"));
            if !handout { crate::theme::check(ui, &mut m.image, tr("Slide Image")); }
        });
        rows(ui, |ui| {
            crate::theme::check(ui, &mut m.footer, tr("Footer"));
            crate::theme::check(ui, &mut m.number, tr("Page Number"));
            if !handout { crate::theme::check(ui, &mut m.body, tr("Body")); }
        });
    });
    let before = if handout { app.handout.clone() } else { app.notes_master.clone() };
    if m != before {
        app.checkpoint("master");
        app.pres.set_page_master(if handout { "handout" } else { "notes" }, &m.to_text(), (m.header, m.date, m.footer, m.number));
        app.changed();
    }
    if handout { app.handout = m; } else { app.notes_master = m; }
}

// ---- the views' screens ---------------------------------------------------------------------

/// The central area for views other than Normal (true when it was drawn).
pub fn central(app: &mut SlidesApp, ui: &mut Ui) -> bool {
    let bg = if app.dark { Color32::from_gray(0x24) } else { Color32::from_rgb(0xE6, 0xE6, 0xE6) };
    match app.view {
        View::Sorter => { egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg).inner_margin(16)).show(ui, |ui| sorter(app, ui)); true }
        View::NotesPage => { egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg)).show(ui, |ui| notes_page(app, ui)); true }
        View::Reading => { egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_gray(0x30))).show(ui, |ui| reading(app, ui)); true }
        View::HandoutMaster | View::NotesMaster => { egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg)).show(ui, |ui| page_master(app, ui)); true }
        _ => false,
    }
}

/// The left side for Outline View and Slide Master view (true when drawn instead of the
/// slide navigator).
pub fn side(app: &mut SlidesApp, ui: &mut Ui) -> bool {
    match app.view {
        View::Outline => { egui::Panel::left("outline").resizable(true).default_size(340.0).frame(egui::Frame::NONE.fill(c_card()).inner_margin(10)).show(ui, |ui| outline(app, ui)); true }
        View::SlideMaster => { egui::Panel::left("masters").resizable(false).exact_size(230.0).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(8)).show(ui, |ui| master_nav(app, ui)); true }
        View::Sorter | View::Reading | View::HandoutMaster | View::NotesMaster | View::NotesPage => true,
        View::Normal => false,
    }
}

fn outline(app: &mut SlidesApp, ui: &mut Ui) {
    let slides = app.slides();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, part) in slides.iter().enumerate() {
            let scene = app.pres.scene(part);
            // Title and body placeholders of the slide, as editable outline text.
            let mut boxes: Vec<(slides_engine::scene::Loc, String, bool)> = vec![];
            for it in scene.items.iter().filter(|it| it.own) {
                let Some(ph) = &it.placeholder else { continue };
                let title = matches!(ph.0.as_str(), "title" | "ctrTitle");
                if !(title || matches!(ph.0.as_str(), "body" | "obj" | "subTitle")) { continue; }
                let text = match &it.kind { slides_engine::scene::Kind::Shape { text: Some(t), .. } => t.paras.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n"), _ => String::new() };
                boxes.push((it.loc.clone(), text, title));
            }
            boxes.sort_by_key(|b| !b.2);
            let on = i == app.current;
            ui.horizontal_top(|ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(34.0, 22.0), Sense::click());
                ui.painter().text(r.left_center() + vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, (i + 1).to_string(), egui::FontId::proportional(13.0), if on { ACCENT } else { c_ink2() });
                ui.painter().rect_stroke(Rect::from_min_size(r.min + vec2(16.0, 5.0), vec2(14.0, 11.0)), 1.0, Stroke::new(1.0, if on { ACCENT } else { c_ink3() }), egui::StrokeKind::Inside);
                if resp.clicked() { app.go_to(i); }
                ui.vertical(|ui| {
                    if boxes.is_empty() { ui.label(RichText::new(tr("(no text placeholders)")).color(c_ink3()).italics()); }
                    for (loc, text, title) in boxes {
                        let mut t = text.clone();
                        let font = if title { egui::FontId::proportional(15.0) } else { egui::FontId::proportional(13.0) };
                        let r = ui.add(egui::TextEdit::multiline(&mut t).font(font).desired_rows(1).frame(egui::Frame::NONE).desired_width(f32::INFINITY).hint_text(if title { tr("Title") } else { tr("Text") }));
                        if r.gained_focus() { app.current = i; }
                        if r.changed() {
                            app.checkpoint("outline");
                            let len: usize = text.chars().count();
                            app.pres.delete_text(&loc, None, 0, len);
                            app.pres.insert_text(&loc, None, 0, &t);
                            app.changed();
                        }
                    }
                });
            });
            ui.add_space(8.0);
        }
    });
}

fn master_nav(app: &mut SlidesApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let (sw, sh) = app.pres.size();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (mi, m) in app.pres.masters().into_iter().enumerate() {
            let mut parts = vec![(m.clone(), true)];
            parts.extend(app.pres.layouts(&m).into_iter().map(|l| (l, false)));
            for (part, is_master) in parts {
                let w = if is_master { 190.0 } else { 160.0 };
                let h = w * (sh / sw) as f32;
                let (row, resp) = ui.allocate_exact_size(vec2(210.0, h + 8.0), Sense::click());
                let r = Rect::from_min_size(pos2(row.max.x - w - 2.0, row.min.y + 4.0), vec2(w, h));
                if !is_master { ui.painter().line_segment([pos2(r.min.x - 14.0, r.center().y), pos2(r.min.x - 4.0, r.center().y)], Stroke::new(1.0, c_ink3())); }
                else { ui.painter().text(pos2(row.min.x + 4.0, r.min.y + 8.0), egui::Align2::LEFT_CENTER, (mi + 1).to_string(), egui::FontId::proportional(12.0), c_ink2()); }
                ui.painter().rect_filled(r, 0.0, Color32::WHITE);
                if let Some(t) = thumb(app, &ctx, &part, w, true) { ui.painter().image(t.id(), r, uv(), Color32::WHITE); }
                let on = app.master_part.as_deref() == Some(part.as_str());
                ui.painter().rect_stroke(r, 1.0, if on { Stroke::new(2.5, ACCENT) } else if resp.hovered() { Stroke::new(1.5, c_ink3()) } else { Stroke::new(0.8, c_line()) }, egui::StrokeKind::Outside);
                let name = if is_master { tr("Slide Master") } else { app.pres.layout_name(&part) };
                let users = if is_master { 0 } else { app.pres.layout_users(&part) };
                let tip = if is_master { name.clone() } else { format!("{name}: {}", crate::i18n::trf("used by {} slides", &[&users])) };
                if resp.clicked() { app.master_part = Some(part.clone()); app.selection.clear(); app.edit = None; app.canvas.key = None; }
                resp.on_hover_text(tip);
            }
            ui.add_space(10.0);
        }
    });
}

fn sorter(app: &mut SlidesApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let slides = app.slides();
    let (sw, sh) = app.pres.size();
    let zoom = app.zoom.unwrap_or(1.0).clamp(0.4, 3.0);
    let w = 220.0 * zoom;
    let h = w * (sh / sw) as f32;
    let dragging = ctx.dragged_id() == Some(egui::Id::new("sorter-drag"));
    let mut drop: Option<usize> = None;
    let sections = app.pres.sections();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let mut i = 0;
        while i < slides.len() {
            // Each section on its own run of rows, with its name.
            let end = sections.iter().map(|s| s.1).filter(|&f| f > i).min().unwrap_or(slides.len());
            if let Some((name, _)) = sections.iter().find(|s| s.1 == i) { ui.label(RichText::new(format!("▾ {name} ({})", end - i)).color(c_ink2()).strong()); }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(18.0, 22.0);
                for k in i..end {
                    let part = &slides[k];
                    let (cell, resp) = ui.allocate_exact_size(vec2(w, h + 20.0), Sense::click_and_drag());
                    let r = Rect::from_min_size(cell.min, vec2(w, h));
                    ui.painter().rect_filled(r, 0.0, Color32::WHITE);
                    if let Some(t) = thumb(app, &ctx, part, w, false) { ui.painter().image(t.id(), r, uv(), if app.pres.hidden(part) { Color32::from_white_alpha(110) } else { Color32::WHITE }); }
                    let sel = k == app.current || app.selected_slides.contains(&k);
                    ui.painter().rect_stroke(r, 2.0, if sel { Stroke::new(3.0, ACCENT) } else if resp.hovered() { Stroke::new(2.0, c_ink3()) } else { Stroke::new(0.8, c_line()) }, egui::StrokeKind::Outside);
                    ui.painter().text(pos2(r.max.x, r.max.y + 4.0), egui::Align2::RIGHT_TOP, (k + 1).to_string(), egui::FontId::proportional(12.0), c_ink2());
                    // A star for slides with a transition or animations; their timing.
                    let tr_ = app.pres.slide_transition(part);
                    let mut x = r.min.x;
                    if tr_.effect != "none" || !app.pres.animations(part).is_empty() { ui.painter().text(pos2(x, r.max.y + 4.0), egui::Align2::LEFT_TOP, "★", egui::FontId::proportional(12.0), c_ink3()); x += 16.0; }
                    if let Some(a) = tr_.after_ms { ui.painter().text(pos2(x, r.max.y + 4.0), egui::Align2::LEFT_TOP, format!("{:02}:{:02}", a / 60_000, (a / 1000) % 60), egui::FontId::proportional(11.0), c_ink3()); }
                    if resp.double_clicked() { app.go_to(k); set_view(app, View::Normal); }
                    else if resp.clicked() {
                        let (shift, cmd) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
                        if shift { let (a, b) = (app.current.min(k), app.current.max(k)); app.selected_slides = (a..=b).collect(); app.current = k; }
                        else if cmd { if !app.selected_slides.insert(k) { app.selected_slides.remove(&k); } app.current = k; }
                        else { app.go_to(k); }
                    }
                    if resp.drag_started() { ctx.set_dragged_id(egui::Id::new("sorter-drag")); ui.data_mut(|d| d.insert_temp(egui::Id::new("sorter-from"), k)); }
                    if dragging { if let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| cell.expand(9.0).contains(*p)) {
                        let before = p.x < r.center().x;
                        let x = if before { r.min.x - 9.0 } else { r.max.x + 9.0 };
                        ui.painter().line_segment([pos2(x, r.min.y), pos2(x, r.max.y)], Stroke::new(3.0, ACCENT));
                        drop = Some(if before { k } else { k + 1 });
                    } }
                    resp.context_menu(|ui| {
                        if menu_item(ui, "newSlide", "New Slide") { app.go_to(k); app.new_slide(None); ui.close(); }
                        if menu_item(ui, "duplicate", "Duplicate Slide") { app.go_to(k); app.duplicate_slides(); ui.close(); }
                        if menu_item(ui, "deleteSlide", "Delete Slide") { app.go_to(k); app.delete_slides(); ui.close(); }
                        if menu_item(ui, "hideSlide", if app.pres.hidden(part) { "Show Slide" } else { "Hide Slide" }) { app.go_to(k); app.toggle_hidden(); ui.close(); }
                    });
                }
            });
            ui.add_space(12.0);
            i = end;
        }
    });
    if dragging && ui.input(|i| i.pointer.any_released()) {
        if let (Some(to), Some(from)) = (drop, ui.data(|d| d.get_temp::<usize>(egui::Id::new("sorter-from")))) { app.move_slide(from, if to > from { to - 1 } else { to }); }
        ctx.stop_dragging();
    }
    if !ctx.memory(|m| m.focused().is_some()) {
        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) { app.go_to(app.current + 1); }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) { app.go_to(app.current.saturating_sub(1)); }
        if ui.input(|i| i.key_pressed(egui::Key::Delete)) { app.delete_slides(); }
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) { set_view(app, View::Normal); }
    }
}

/// A printed page (8.5×11 in, portrait or landscape) fitted into `area`.
fn page_rect(area: Rect, landscape: bool) -> Rect {
    let (pw, ph) = if landscape { (11.0, 8.5) } else { (8.5, 11.0) };
    let k = ((area.width() - 60.0) / pw).min((area.height() - 60.0) / ph);
    Rect::from_center_size(area.center(), vec2(pw * k, ph * k))
}

fn notes_page(app: &mut SlidesApp, ui: &mut Ui) {
    let Some(part) = app.slide() else { return };
    let ctx = ui.ctx().clone();
    let area = ui.max_rect();
    let page = page_rect(area, false);
    ui.painter().rect_filled(page.translate(vec2(2.0, 3.0)), 0.0, Color32::from_black_alpha(40));
    ui.painter().rect_filled(page, 0.0, Color32::WHITE);
    let (sw, sh) = app.pres.size();
    let iw = page.width() * 0.76;
    let img = Rect::from_min_size(pos2(page.center().x - iw / 2.0, page.min.y + page.height() * 0.08), vec2(iw, iw * (sh / sw) as f32));
    if let Some(t) = thumb(app, &ctx, &part, iw, false) { ui.painter().image(t.id(), img, uv(), Color32::WHITE); }
    ui.painter().rect_stroke(img, 0.0, Stroke::new(1.0, Color32::from_gray(160)), egui::StrokeKind::Outside);
    let body = Rect::from_min_max(pos2(page.min.x + page.width() * 0.12, img.max.y + 20.0), pos2(page.max.x - page.width() * 0.12, page.max.y - page.height() * 0.08));
    if app.notes.0 != part { app.notes = (part.clone(), app.pres.notes_text(&part)); }
    let mut text = app.notes.1.clone();
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body));
    let r = c.add_sized(body.size(), egui::TextEdit::multiline(&mut text).frame(egui::Frame::NONE).text_color(Color32::from_gray(30)).background_color(Color32::WHITE).hint_text(tr("Click to add text")));
    if r.changed() { app.checkpoint("notes"); app.pres.set_notes(&part, &text); app.notes.1 = text; }
    ui.painter().text(pos2(page.max.x - 14.0, page.max.y - 12.0), egui::Align2::RIGHT_BOTTOM, (app.current + 1).to_string(), egui::FontId::proportional(11.0), Color32::from_gray(90));
    // Page through the slides.
    if !ctx.memory(|m| m.focused().is_some()) {
        if ui.input(|i| i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::ArrowDown)) { app.go_to(app.current + 1); }
        if ui.input(|i| i.key_pressed(egui::Key::PageUp) || i.key_pressed(egui::Key::ArrowUp)) { app.go_to(app.current.saturating_sub(1)); }
    }
}

fn reading(app: &mut SlidesApp, ui: &mut Ui) {
    // Reading View is a slide show in this window.
    if app.show.is_none() {
        let mut s = crate::show::Show::new(app, app.current, None, None);
        s.setup.kind = ShowType::Browsed;
        app.show = Some(s);
        app.reading = true;
    }
    let _ = ui;
}

fn page_master(app: &mut SlidesApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let handout = app.view == View::HandoutMaster;
    let m = if handout { app.handout.clone() } else { app.notes_master.clone() };
    let page = page_rect(ui.max_rect(), m.landscape);
    let p = ui.painter();
    p.rect_filled(page.translate(vec2(2.0, 3.0)), 0.0, Color32::from_black_alpha(40));
    p.rect_filled(page, 0.0, Color32::WHITE);
    let dash = |r: Rect, label: &str| {
        for (a, b) in [(r.left_top(), r.right_top()), (r.right_top(), r.right_bottom()), (r.right_bottom(), r.left_bottom()), (r.left_bottom(), r.left_top())] { p.add(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, Color32::from_gray(150)), 4.0, 3.0)); }
        p.text(r.left_top() + vec2(6.0, 4.0), egui::Align2::LEFT_TOP, tr(label), egui::FontId::proportional(11.0), Color32::from_gray(110));
    };
    let mg = page.width() * 0.06;
    let band = page.height() * 0.06;
    let date = chrono_today();
    if m.header { dash(Rect::from_min_size(page.min + vec2(mg, mg * 0.5), vec2(page.width() * 0.4, band)), "Header"); }
    if m.date { let r = Rect::from_min_size(pos2(page.max.x - mg - page.width() * 0.4, page.min.y + mg * 0.5), vec2(page.width() * 0.4, band)); dash(r, ""); p.text(r.right_top() + vec2(-6.0, 4.0), egui::Align2::RIGHT_TOP, &date, egui::FontId::proportional(11.0), Color32::from_gray(110)); }
    if m.footer { dash(Rect::from_min_size(pos2(page.min.x + mg, page.max.y - mg * 0.5 - band), vec2(page.width() * 0.4, band)), "Footer"); }
    if m.number { let r = Rect::from_min_size(pos2(page.max.x - mg - page.width() * 0.4, page.max.y - mg * 0.5 - band), vec2(page.width() * 0.4, band)); dash(r, ""); p.text(r.right_top() + vec2(-6.0, 4.0), egui::Align2::RIGHT_TOP, "‹#›", egui::FontId::proportional(11.0), Color32::from_gray(110)); }
    let inner = Rect::from_min_max(pos2(page.min.x + mg, page.min.y + mg + band), pos2(page.max.x - mg, page.max.y - mg - band));
    let (sw, sh) = app.pres.size();
    let ar = (sh / sw) as f32;
    let slides = app.slides();
    if handout {
        // The slide frames, as many per page as chosen (3: with lines for notes).
        let n = m.per_page;
        let (cols, rows) = match (n, m.landscape) { (1, _) => (1, 1), (2, _) => (1, 2), (3, false) => (1, 3), (3, true) => (3, 1), (4, _) => (2, 2), (6, false) => (2, 3), (6, true) => (3, 2), (_, _) => (3, 3) };
        let cw = inner.width() / cols as f32;
        let rh = inner.height() / rows as f32;
        for k in 0..n {
            let (c, r) = (k % cols, k / cols);
            let cell = Rect::from_min_size(pos2(inner.min.x + c as f32 * cw, inner.min.y + r as f32 * rh), vec2(cw, rh)).shrink(8.0);
            let fw = if n == 3 && !m.landscape { cell.width() * 0.48 } else { cell.width() };
            let fw = fw.min(cell.height() / ar);
            let f = Rect::from_min_size(cell.min, vec2(fw, fw * ar));
            if let Some(part) = slides.get(k) { if let Some(t) = thumb(app, &ctx, part, fw, false) { ui.painter().image(t.id(), f, uv(), Color32::WHITE); } }
            ui.painter().rect_stroke(f, 0.0, Stroke::new(1.0, Color32::from_gray(140)), egui::StrokeKind::Outside);
            if n == 3 && !m.landscape {
                let mut y = f.min.y + 14.0;
                while y < f.max.y { ui.painter().hline(f.max.x + 14.0..=cell.max.x, y, Stroke::new(0.7, Color32::from_gray(170))); y += 18.0; }
            }
        }
    } else {
        if m.image {
            let iw = inner.width() * 0.8;
            let r = Rect::from_min_size(pos2(inner.center().x - iw / 2.0, inner.min.y + 6.0), vec2(iw, iw * ar));
            if let Some(part) = slides.get(app.current) { if let Some(t) = thumb(app, &ctx, part, iw, false) { ui.painter().image(t.id(), r, uv(), Color32::WHITE); } }
            dash(r, "Slide Image");
        }
        if m.body {
            let top = inner.min.y + if m.image { inner.width() * 0.8 * ar + 24.0 } else { 6.0 };
            let r = Rect::from_min_max(pos2(inner.min.x + 20.0, top), pos2(inner.max.x - 20.0, inner.max.y - 6.0));
            dash(r, "Click to edit Master text styles");
            for (k, l) in ["Second level", "Third level", "Fourth level", "Fifth level"].iter().enumerate() {
                ui.painter().text(r.left_top() + vec2(18.0 + k as f32 * 16.0, 24.0 + k as f32 * 18.0), egui::Align2::LEFT_TOP, format!("• {}", tr(l)), egui::FontId::proportional(11.0 - k as f32 * 0.5), Color32::from_gray(90));
            }
        }
    }
}

fn chrono_today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = secs / 86_400 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{m}/{d}/{y}")
}

// ---- rulers and grid on the editing canvas --------------------------------------------------

/// Rulers (inches, from the slide's center like PowerPoint) along the canvas's top and left.
pub fn rulers(app: &SlidesApp, painter: &egui::Painter, outer: Rect, slide: Rect, scale: f32) {
    if !app.show_ruler { return; }
    let t = 18.0;
    let top = Rect::from_min_max(pos2(outer.min.x, outer.min.y), pos2(outer.max.x, outer.min.y + t));
    let left = Rect::from_min_max(pos2(outer.min.x, outer.min.y), pos2(outer.min.x + t, outer.max.y));
    let bg = if app.dark { Color32::from_gray(0x33) } else { Color32::from_gray(0xF4) };
    painter.rect_filled(top, 0.0, bg);
    painter.rect_filled(left, 0.0, bg);
    let ink = c_ink2();
    let inch = 914_400.0 * scale;
    let c = slide.center();
    // The slide's span shows lighter.
    painter.rect_filled(Rect::from_min_max(pos2(slide.min.x, top.min.y), pos2(slide.max.x, top.max.y)), 0.0, if app.dark { Color32::from_gray(0x44) } else { Color32::WHITE });
    painter.rect_filled(Rect::from_min_max(pos2(left.min.x, slide.min.y), pos2(left.max.x, slide.max.y)), 0.0, if app.dark { Color32::from_gray(0x44) } else { Color32::WHITE });
    let step = inch / 8.0;
    let (mut k, n) = (-(((c.x - top.min.x) / step) as i32), ((top.max.x - c.x) / step) as i32);
    while k <= n {
        let x = c.x + k as f32 * step;
        let h = if k % 8 == 0 { 9.0 } else if k % 4 == 0 { 6.0 } else if k % 2 == 0 { 4.0 } else { 2.0 };
        painter.vline(x, top.max.y - h..=top.max.y, Stroke::new(0.8, ink));
        if k % 8 == 0 && k != 0 { painter.text(pos2(x, top.min.y + 1.0), egui::Align2::CENTER_TOP, (k / 8).abs().to_string(), egui::FontId::proportional(9.0), ink); }
        k += 1;
    }
    let (mut k, n) = (-(((c.y - left.min.y) / step) as i32), ((left.max.y - c.y) / step) as i32);
    while k <= n {
        let y = c.y + k as f32 * step;
        let w = if k % 8 == 0 { 9.0 } else if k % 4 == 0 { 6.0 } else if k % 2 == 0 { 4.0 } else { 2.0 };
        painter.hline(left.max.x - w..=left.max.x, y, Stroke::new(0.8, ink));
        if k % 8 == 0 && k != 0 { painter.text(pos2(left.min.x + 1.0, y), egui::Align2::LEFT_CENTER, (k / 8).abs().to_string(), egui::FontId::proportional(9.0), ink); }
        k += 1;
    }
    // Where the pointer is.
    if let Some(p) = painter.ctx().input(|i| i.pointer.hover_pos()).filter(|p| outer.contains(*p)) {
        painter.vline(p.x, top.y_range(), Stroke::new(1.0, ACCENT));
        painter.hline(left.x_range(), p.y, Stroke::new(1.0, ACCENT));
    }
}

// ---- dialogs --------------------------------------------------------------------------------

pub fn dialogs(app: &mut SlidesApp, ctx: &egui::Context) {
    let Some(mut d) = app.view_dialog.take() else { return };
    let mut open = true;
    let mut done = false;
    let title = match &d { ViewDialog::SetUp(_) => "Set Up Show", ViewDialog::CustomShows { .. } => "Custom Shows", ViewDialog::Zoom { .. } => "Zoom", ViewDialog::GridGuides => "Grid and Guides", ViewDialog::KeepTimings { .. } => "Rehearse Timings", ViewDialog::KeepInk { .. } => "Slide Show", ViewDialog::Rename { .. } => "Rename Layout" };
    crate::theme::dialog(ctx, title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_min_width(380.0);
        match &mut d {
            ViewDialog::SetUp(s) => {
                let n = app.slides().len();
                let shows = app.pres.custom_shows();
                ui.columns(2, |cols| {
                    let ui = &mut cols[0];
                    crate::theme::section(ui, "Show type");
                    ui.radio_value(&mut s.kind, ShowType::Speaker, tr("Presented by a speaker (full screen)"));
                    ui.radio_value(&mut s.kind, ShowType::Browsed, tr("Browsed by an individual (window)"));
                    ui.radio_value(&mut s.kind, ShowType::Kiosk, tr("Browsed at a kiosk (full screen)"));
                    crate::theme::section(ui, "Show options");
                    let kiosk = s.kind == ShowType::Kiosk;
                    let mut lp = s.loop_until_esc || kiosk;
                    if ui.add_enabled(!kiosk, egui::Checkbox::new(&mut lp, tr("Loop continuously until 'Esc'"))).changed() { s.loop_until_esc = lp; }
                    let mut nn = !s.narration; if ui.checkbox(&mut nn, tr("Show without narration")).changed() { s.narration = !nn; }
                    let mut na = !s.animation; if ui.checkbox(&mut na, tr("Show without animation")).changed() { s.animation = !na; }
                    if s.kind == ShowType::Browsed { ui.checkbox(&mut s.scrollbar, tr("Show scrollbar")); }
                    crate::theme::hrow(ui, |ui| { ui.label(tr("Pen color:")); let mut c = hexc(&s.pen_color); if color_button(ui, &mut c) { s.pen_color = hexs(c); } });
                    crate::theme::hrow(ui, |ui| { ui.label(tr("Laser pointer color:")); let mut c = hexc(&s.laser_color); if color_button(ui, &mut c) { s.laser_color = hexs(c); } });
                    let ui = &mut cols[1];
                    crate::theme::section(ui, "Show slides");
                    if ui.radio(s.range == Range::All, tr("All")).clicked() { s.range = Range::All; }
                    ui.horizontal(|ui| {
                        let (mut a, mut z) = match s.range { Range::From(a, z) => (a, z), _ => (1, n.max(1)) };
                        let on = matches!(s.range, Range::From(..));
                        if ui.radio(on, tr("From:")).clicked() { s.range = Range::From(a, z); }
                        let ra = ui.add(egui::DragValue::new(&mut a).range(1..=n.max(1)));
                        ui.label(tr("To:"));
                        let rz = ui.add(egui::DragValue::new(&mut z).range(a..=n.max(1)));
                        if ra.changed() || rz.changed() { s.range = Range::From(a, z.max(a)); }
                    });
                    let cur = match &s.range { Range::Custom(c) => Some(c.clone()), _ => None };
                    ui.add_enabled_ui(!shows.is_empty(), |ui| {
                        if ui.radio(cur.is_some(), tr("Custom show:")).clicked() { if let Some(f) = shows.first() { s.range = Range::Custom(f.0.clone()); } }
                        crate::theme::Combo::from_id_salt("setup-custom").width(160.0).selected_text(cur.clone().unwrap_or_default()).show_ui(ui, |ui| for c in &shows { if ui.selectable_label(cur.as_deref() == Some(c.0.as_str()), &c.0).clicked() { s.range = Range::Custom(c.0.clone()); } });
                    });
                    crate::theme::section(ui, "Advance slides");
                    if ui.radio(!s.use_timings, tr("Manually")).clicked() { s.use_timings = false; }
                    if ui.radio(s.use_timings, tr("Using timings, if present")).clicked() { s.use_timings = true; }
                    crate::theme::section(ui, "Multiple monitors");
                    crate::theme::hrow(ui, |ui| {
                        ui.label(tr("Slide show monitor:"));
                        let label = if s.monitor.is_empty() { tr("Automatic") } else { s.monitor.clone() };
                        crate::theme::Combo::from_id_salt("setup-monitor").width(170.0).selected_text(label).show_ui(ui, |ui| {
                            if ui.selectable_label(s.monitor.is_empty(), tr("Automatic")).clicked() { s.monitor.clear(); }
                            for (name, ..) in monitors() { if ui.selectable_label(s.monitor == name, &name).clicked() { s.monitor = name; } }
                        });
                    });
                    ui.checkbox(&mut s.presenter_view, tr("Use Presenter View"));
                });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() { let s = s.clone(); app.checkpoint("show setup"); app.pres.set_show_setup(&s); done = true; }
                    if ui.button(tr("Cancel")).clicked() { done = true; }
                });
            }
            ViewDialog::CustomShows { shows, picked, editing } => {
                if let Some((idx, name, chosen)) = editing {
                    // Define Custom Show: the name, all slides on the left, the show's on the right.
                    crate::theme::hrow(ui, |ui| { ui.label(tr("Slide show name:")); ui.text_edit_singleline(name); });
                    let all = app.slides();
                    ui.columns(2, |cols| {
                        cols[0].label(tr("Slides in presentation:"));
                        egui::ScrollArea::vertical().id_salt("cs-all").max_height(240.0).show(&mut cols[0], |ui| for (k, s) in all.iter().enumerate() {
                            if ui.button(format!("{}. {}", k + 1, app.pres.slide_title(s))).on_hover_text(tr("Add")).clicked() { chosen.push(s.clone()); }
                        });
                        cols[1].label(tr("Slides in custom show:"));
                        let mut remove = None; let mut up = None;
                        egui::ScrollArea::vertical().id_salt("cs-show").max_height(240.0).show(&mut cols[1], |ui| for (k, s) in chosen.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let n = all.iter().position(|x| x == s).map(|i| i + 1).unwrap_or(0);
                                ui.label(format!("{}. {}", n, app.pres.slide_title(s)));
                                if ui.small_button("▲").clicked() && k > 0 { up = Some(k); }
                                if ui.small_button("✕").clicked() { remove = Some(k); }
                            });
                        });
                        if let Some(k) = up { chosen.swap(k, k - 1); }
                        if let Some(k) = remove { chosen.remove(k); }
                    });
                    let mut close_edit = false;
                    crate::theme::footer(ui, |ui| {
                        if crate::theme::primary(ui, "OK").clicked() {
                            let entry = (if name.trim().is_empty() { tr("Custom Show") } else { name.clone() }, chosen.clone());
                            if *idx < shows.len() { shows[*idx] = entry; } else { shows.push(entry); }
                            close_edit = true;
                        }
                        if ui.button(tr("Cancel")).clicked() { close_edit = true; }
                    });
                    if close_edit { *editing = None; }
                } else {
                    ui.horizontal_top(|ui| {
                        egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(6).show(ui, |ui| {
                            ui.set_min_size(vec2(220.0, 180.0));
                            for (k, s) in shows.iter().enumerate() { if ui.selectable_label(*picked == k, &s.0).clicked() { *picked = k; } }
                        });
                        ui.vertical(|ui| {
                            if ui.button(tr("New…")).clicked() { *editing = Some((shows.len(), format!("{} {}", tr("Custom Show"), shows.len() + 1), vec![])); }
                            if ui.add_enabled(*picked < shows.len(), egui::Button::new(tr("Edit…"))).clicked() { let s = shows[*picked].clone(); *editing = Some((*picked, s.0, s.1)); }
                            if ui.add_enabled(*picked < shows.len(), egui::Button::new(tr("Remove"))).clicked() { shows.remove(*picked); *picked = 0; }
                            if ui.add_enabled(*picked < shows.len(), egui::Button::new(tr("Copy"))).clicked() { let mut s = shows[*picked].clone(); s.0 = format!("{} ({})", s.0, tr("copy")); shows.push(s); }
                        });
                    });
                    crate::theme::footer(ui, |ui| {
                        if ui.add_enabled(*picked < shows.len(), egui::Button::new(tr("Show"))).clicked() {
                            let all = app.slides();
                            let order: Vec<usize> = shows[*picked].1.iter().filter_map(|p| all.iter().position(|s| s == p)).collect();
                            let s = shows.clone(); app.checkpoint("custom shows"); app.pres.set_custom_shows(&s);
                            if !order.is_empty() { app.start_show_with(order, None); }
                            done = true;
                        }
                        if crate::theme::primary(ui, "Close").clicked() { let s = shows.clone(); app.checkpoint("custom shows"); app.pres.set_custom_shows(&s); done = true; }
                    });
                }
            }
            ViewDialog::Zoom { percent } => {
                ui.columns(2, |cols| {
                    for (k, z) in [0.0f32, 400.0, 200.0, 100.0, 66.0, 50.0, 33.0].iter().enumerate() {
                        let col = &mut cols[if k < 4 { 0 } else { 1 }];
                        let label = if *z == 0.0 { tr("Fit") } else { format!("{z:.0}%") };
                        if col.radio((*percent - z).abs() < 0.5 || (*z == 0.0 && *percent <= 0.0), label).clicked() { *percent = *z; }
                    }
                });
                crate::theme::hrow(ui, |ui| { ui.label(tr("Percent:")); ui.add(egui::DragValue::new(percent).range(0.0..=400.0).suffix("%")); });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "OK").clicked() { app.zoom = if *percent <= 0.0 { None } else { Some((*percent / 100.0).clamp(0.1, 4.0)) }; done = true; }
                    if ui.button(tr("Cancel")).clicked() { done = true; }
                });
            }
            ViewDialog::GridGuides => {
                crate::theme::section(ui, "Snap to");
                ui.checkbox(&mut app.snap_grid, tr("Snap objects to grid"));
                crate::theme::section(ui, "Grid settings");
                crate::theme::hrow(ui, |ui| {
                    ui.label(tr("Spacing:"));
                    let mut inches = app.grid_spacing / 914_400.0;
                    crate::theme::Combo::from_id_salt("grid-sp").width(110.0).selected_text(format!("{:.3}\"", inches)).show_ui(ui, |ui| for v in [1.0 / 24.0, 1.0 / 16.0, 1.0 / 12.0, 1.0 / 8.0, 1.0 / 6.0, 1.0 / 5.0, 1.0 / 4.0, 1.0 / 3.0, 0.5, 1.0] { if ui.selectable_label((inches - v).abs() < 1e-6, format!("{v:.3}\"")).clicked() { inches = v; } });
                    app.grid_spacing = inches * 914_400.0;
                });
                ui.checkbox(&mut app.show_grid, tr("Display grid on screen"));
                crate::theme::section(ui, "Guide settings");
                ui.checkbox(&mut app.show_guides, tr("Display drawing guides on screen"));
                ui.checkbox(&mut app.smart_guides, tr("Display smart guides when shapes are aligned"));
                crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { done = true; });
            }
            ViewDialog::KeepTimings { times, total } => {
                ui.label(crate::i18n::trf("The total time for the slide show was {}. Do you want to keep the new slide timings?", &[&format!("{}:{:02}:{:02}", (*total / 3600.0) as u32, (*total / 60.0) as u32 % 60, *total as u32 % 60)]));
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "Yes").clicked() {
                        app.checkpoint("timings");
                        for (s, ms) in times.iter() { app.pres.set_timing(s, Some(*ms)); }
                        let mut st = app.pres.show_setup(); st.use_timings = true; app.pres.set_show_setup(&st);
                        app.changed();
                        done = true;
                    }
                    if ui.button(tr("No")).clicked() { done = true; }
                });
            }
            ViewDialog::KeepInk { strokes } => {
                ui.label(tr("Do you want to keep your ink annotations?"));
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "Keep").clicked() {
                        app.checkpoint("ink");
                        for (part, pts, color, w, alpha) in strokes.iter() { app.pres.insert_ink(part, pts, color, *w, *alpha); }
                        app.changed();
                        done = true;
                    }
                    if ui.button(tr("Discard")).clicked() { done = true; }
                });
            }
            ViewDialog::Rename { layout, name } => {
                crate::theme::hrow(ui, |ui| { ui.label(tr("Layout name:")); ui.text_edit_singleline(name); });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "Rename").clicked() { app.checkpoint("layout"); let (l, n) = (layout.clone(), name.clone()); app.pres.rename_layout(&l, &n); app.changed(); done = true; }
                    if ui.button(tr("Cancel")).clicked() { done = true; }
                });
            }
        }
    });
    if open && !done { app.view_dialog = Some(d); }
}

fn hexc(h: &str) -> word_engine::Color { let c = crate::show::hex_color(h); word_engine::Color(c.r(), c.g(), c.b()) }
fn hexs(c: word_engine::Color) -> String { format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2) }

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len() * 4 / 3 + 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 { if k <= c.len() { out.push(T[(n >> (18 - 6 * k) & 63) as usize] as char); } else { out.push('='); } }
    }
    out
}
