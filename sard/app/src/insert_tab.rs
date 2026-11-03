//! The Insert tab, complete: Slides, Tables, Images, Illustrations, Links, Comments, Text,
//! Symbols and Media groups.

use crate::app::{Pane, SlidesApp};
use crate::dialogs::Dialog;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::scene::Xfrm;
use std::sync::{Arc, Mutex};

/// Captured screens and windows for Insert ▸ Screenshot.
pub type Shots = Arc<Mutex<Option<Result<Vec<(String, image::RgbaImage)>, String>>>>;

pub fn insert(app: &mut SlidesApp, ui: &mut Ui) {
    group(ui, "Slides", |ui| {
        let (main, arrow) = large_split(ui, "newSlide", "New Slide", "Add a slide (⌘M)");
        if main.clicked() { app.new_slide(None); }
        crate::theme::menu(&arrow, |ui| {
            if let Some(l) = crate::nav::layout_gallery(app, ui) { app.new_slide(Some(l)); ui.close(); }
            ui.separator();
            if menu_item(ui, "duplicate", "Duplicate Selected Slides") { app.duplicate_slides(); ui.close(); }
            if menu_item(ui, "seeAllSlides", "Reuse Slides…") { app.pane = Pane::Reuse; ui.close(); }
        });
        if large(ui, "seeAllSlides", "Reuse Slides", app.pane == Pane::Reuse, true, "Insert slides from another presentation").clicked() { app.pane = if app.pane == Pane::Reuse { Pane::None } else { Pane::Reuse }; }
    });
    group(ui, "Tables", |ui| {
        let t = large_menu(ui, "table", "Table", "Insert a table");
        crate::theme::menu(&t, |ui| table_menu(app, ui));
    });
    group(ui, "Images", |ui| {
        let (main, arrow) = large_split(ui, "pictures", "Pictures", "Insert a picture from this device");
        if main.clicked() { app.insert_picture_dialog(); }
        crate::theme::menu(&arrow, |ui| {
            if menu_item(ui, "folderOpen", "This Device…") { app.insert_picture_dialog(); ui.close(); }
            if menu_item(ui, "imageFile", "From the Clipboard") { app.paste_as("picture"); ui.close(); }
        });
        let shot = large_menu(ui, "screenshot", "Screenshot", "Insert a picture of a window or the screen");
        crate::theme::menu(&shot, |ui| screenshot_menu(app, ui));
        let (amain, aarrow) = large_split(ui, "photoAlbum", "Photo Album", "Create a presentation of pictures");
        if amain.clicked() { app.dialog = Some(Dialog::PhotoAlbum { files: vec![], per: 1, captions: false }); }
        crate::theme::menu(&aarrow, |ui| if menu_item(ui, "photoAlbum", "New Photo Album…") { app.dialog = Some(Dialog::PhotoAlbum { files: vec![], per: 1, captions: false }); ui.close(); });
    });
    group(ui, "Illustrations", |ui| {
        let shapes = large_menu(ui, "shapes", "Shapes", "Insert a shape");
        crate::theme::menu(&shapes, |ui| crate::ribbon::shape_gallery(app, ui));
        let models = large_menu(ui, "model3d", "3D Models", "Insert a 3-D object you can turn in any direction");
        crate::theme::menu(&models, |ui| crate::three_d_ui::model_gallery(app, ui));
        let icons = large_menu(ui, "icons", "Icons", "Insert an icon");
        crate::theme::menu(&icons, |ui| icon_gallery(app, ui));
        if large(ui, "smartArt", "SmartArt", false, true, "Insert a SmartArt graphic").clicked() { crate::smartart_ui::open_smartart(app, ""); }
        if large(ui, "smartArtConvert", "Diagram", false, true, "Draw a flowchart, sequence, state, pie, mind map or timeline from Mermaid code").clicked() { crate::smartart_ui::open_mermaid(app); }
        let chart = large_menu(ui, "chart", "Chart", "Insert a chart");
        crate::theme::menu(&chart, |ui| for (k, (kind, n)) in word_engine::objects::CHARTS.iter().enumerate() {
            let icon = ["chartColumn", "chartBar", "chartLine", "chartPie", "chartArea"].get(k).copied().unwrap_or("chart");
            if menu_item(ui, icon, n) { insert_chart(app, *kind); ui.close(); }
        });
    });
    group(ui, "Links", |ui| {
        let zoom = large_menu(ui, "zoomSlide", "Zoom", "Insert a zoom to another slide");
        crate::theme::menu(&zoom, |ui| zoom_menu(app, ui));
        if large(ui, "link", "Link", false, true, "Add a link (⌘K)").clicked() { open_link(app); }
        if large(ui, "action", "Action", false, !app.selection.is_empty(), "What happens when you click the selected object in the slide show").clicked() { app.dialog = Some(Dialog::Action { kind: 0, slide: 0, address: String::new() }); }
    });
    group(ui, "Comments", |ui| {
        if large(ui, "newComment", "Comment", false, true, "Add a comment").clicked() { app.pane = Pane::Comments; }
    });
    group(ui, "Text", |ui| {
        if large(ui, "textBox", "Text Box", app.insert_mode.as_deref() == Some("textbox"), true, "Click or drag on the slide to add a text box").clicked() { app.insert_mode = Some("textbox".into()); }
        if large(ui, "headerFooter", "Header & Footer", false, true, "Date, slide number and footer").clicked() {
            app.dialog = Some(Dialog::HeaderFooter { date: false, auto: true, fixed: slides_engine::format::today(), format: 0, number: true, footer: false, footer_text: String::new(), skip_title: true });
        }
        let wa = large_menu(ui, "wordArt", "WordArt", "Insert decorative text");
        crate::theme::menu(&wa, |ui| wordart_gallery(app, ui));
        rows(ui, |ui| {
            if small(ui, "dateTime", Some("Date & Time"), false, true, "Insert the date or time").clicked() { app.dialog = Some(Dialog::DateTime { format: 0, auto: true }); }
            if small(ui, "slideNumber", Some("Slide Number"), false, true, "Insert the slide number").clicked() { app.insert_field("slidenum"); }
            if small(ui, "object", Some("Object"), false, true, "Embed a file").clicked() { insert_object(app); }
        });
    });
    group(ui, "Symbols", |ui| {
        let (main, arrow) = large_split(ui, "equation", "Equation", "Insert an equation");
        if main.clicked() { app.dialog = Some(Dialog::Equation { loc: None, linear: String::new() }); }
        crate::theme::menu(&arrow, |ui| {
            for (n, src) in word_engine::equation::GALLERY { if ui.button(tr(n)).clicked() { app.insert_equation(None, src.to_string()); ui.close(); } }
            ui.separator();
            if ui.button(tr("Insert New Equation")).clicked() { app.dialog = Some(Dialog::Equation { loc: None, linear: String::new() }); ui.close(); }
        });
        let sym = large_menu(ui, "symbol", "Symbol", "Insert a symbol");
        crate::theme::menu(&sym, |ui| {
            ui.horizontal_wrapped(|ui| { ui.set_max_width(220.0); for c in qalam_kit::symbols::COMMON.chars() { if ui.add(egui::Button::new(egui::RichText::new(c.to_string()).font(qalam_kit::symbols::glyph_font(c, 16.0))).min_size(vec2(30.0, 30.0))).clicked() { app.insert_text_here(&c.to_string()); ui.close(); } } });
            ui.separator();
            if ui.button(tr("More Symbols…")).clicked() { app.dialog = Some(Dialog::Symbol { subset: 10, selected: None }); ui.close(); }
        });
    });
    group(ui, "Media", |ui| {
        let v = large_menu(ui, "video", "Video", "Insert a video");
        crate::theme::menu(&v, |ui| if menu_item(ui, "folderOpen", "This Device…") { insert_media(app, true); ui.close(); });
        let a = large_menu(ui, "audio", "Audio", "Insert audio");
        crate::theme::menu(&a, |ui| if menu_item(ui, "folderOpen", "Audio on My PC…") { insert_media(app, false); ui.close(); });
    });
}

fn table_menu(app: &mut SlidesApp, ui: &mut Ui) {
    let id = egui::Id::new("tbl-grid");
    let mut hover: (usize, usize) = ui.data(|d| d.get_temp(id)).unwrap_or((0, 0));
    ui.label(if hover.0 > 0 { format!("{}x{} {}", hover.1, hover.0, tr("Table")) } else { tr("Insert Table") });
    let (r, _) = ui.allocate_exact_size(vec2(10.0 * 20.0, 8.0 * 20.0), Sense::hover());
    let mut clicked = false;
    for row in 0..8 {
        for col in 0..10 {
            let cell = Rect::from_min_size(pos2(r.min.x + col as f32 * 20.0, r.min.y + row as f32 * 20.0), vec2(18.0, 18.0));
            let resp = ui.interact(cell, id.with((row, col)), Sense::click());
            if resp.hovered() { hover = (row + 1, col + 1); }
            if resp.clicked() { clicked = true; }
            let on = row < hover.0 && col < hover.1;
            ui.painter().rect(cell, 2.0, if on { ACCENT.gamma_multiply(0.25) } else { c_card() }, Stroke::new(1.0, if on { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
        }
    }
    ui.data_mut(|d| d.insert_temp(id, hover));
    if clicked && hover.0 > 0 { app.insert_table(hover.0, hover.1); ui.close(); }
    ui.separator();
    if menu_item(ui, "table", "Insert Table…") { app.dialog = Some(Dialog::InsertTable { rows: 2, cols: 5 }); ui.close(); }
    if menu_item(ui, "drawTable", "Draw Table") { app.insert_mode = Some("table".into()); ui.close(); }
    if menu_item(ui, "chartArea", "Excel Spreadsheet") { app.insert_table(4, 4); app.notify(&tr("Inserted an editable table (spreadsheet objects need Excel).")); ui.close(); }
}

fn screenshot_menu(app: &mut SlidesApp, ui: &mut Ui) {
    // Capture in the background the first time the menu opens.
    if app.shots.is_none() {
        let shots: Shots = Arc::new(Mutex::new(None));
        let s = shots.clone();
        let ctx = ui.ctx().clone();
        std::thread::spawn(move || {
            let mut out = vec![];
            if let Ok(ms) = xcap::Monitor::all() { for m in ms { if let Ok(img) = m.capture_image() { out.push((format!("{} {}", crate::i18n::raw("Screen"), m.name().unwrap_or_default()), img)); } } }
            if let Ok(ws) = xcap::Window::all() {
                for w in ws.into_iter().filter(|w| !w.is_minimized().unwrap_or(true)).take(12) {
                    let title = w.title().unwrap_or_default();
                    if title.trim().is_empty() || title.contains(crate::APP_NAME) { continue; }
                    if let Ok(img) = w.capture_image() { if img.width() > 40 && img.height() > 40 { out.push((format!("{} — {}", w.app_name().unwrap_or_default(), title), img)); } }
                }
            }
            *s.lock().unwrap() = Some(if out.is_empty() { Err("No screens or windows could be captured. On macOS, allow screen recording for Sard in System Settings → Privacy & Security.".into()) } else { Ok(out) });
            ctx.request_repaint();
        });
        app.shots = Some(shots);
    }
    menu_heading(ui, "Available Windows");
    let state = app.shots.as_ref().and_then(|s| s.lock().ok().and_then(|g| g.clone()));
    match state {
        None => { ui.horizontal(|ui| { ui.spinner(); ui.label(tr("Capturing…")); }); }
        Some(Err(e)) => { ui.label(egui::RichText::new(tr(&e)).color(c_ink3())); }
        Some(Ok(list)) => {
            ui.set_width(420.0);
            ui.horizontal_wrapped(|ui| for (k, (title, img)) in list.iter().enumerate() {
                let key = format!("shot-{k}");
                let tex = ui.data(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new(&key))).unwrap_or_else(|| {
                    let small = image::imageops::thumbnail(img, 180, (180 * img.height() / img.width().max(1)).max(1));
                    let t = ui.ctx().load_texture(&key, egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw()), egui::TextureOptions::LINEAR);
                    ui.data_mut(|d| d.insert_temp(egui::Id::new(&key), t.clone()));
                    t
                });
                let (r, resp) = ui.allocate_exact_size(vec2(130.0, 90.0), Sense::click());
                let s = tex.size_vec2();
                let k2 = (r.width() / s.x).min(r.height() / s.y);
                ui.painter().image(tex.id(), Rect::from_center_size(r.center(), s * k2), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                if resp.hovered() { ui.painter().rect_stroke(r, 2.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside); }
                if resp.on_hover_text(title).clicked() {
                    let mut png = std::io::Cursor::new(Vec::new());
                    if image::DynamicImage::ImageRgba8(img.clone()).write_to(&mut png, image::ImageFormat::Png).is_ok() { insert_png(app, png.into_inner(), img.width(), img.height(), "Screenshot"); }
                    app.shots = None;
                    ui.close();
                }
            });
        }
    }
    ui.separator();
    if menu_item(ui, "screenshot", "Refresh") { app.shots = None; }
}

/// Insert PNG bytes as a picture fitted to the slide.
pub fn insert_png(app: &mut SlidesApp, png: Vec<u8>, w: u32, h: u32, name: &str) {
    let Some(s) = app.slide() else { return };
    let (sw, sh) = app.pres.size();
    let (mut pw, mut ph) = (w as f64 * 9525.0, h as f64 * 9525.0);
    let k = (sw * 0.8 / pw).min(sh * 0.8 / ph).min(1.0);
    pw *= k; ph *= k;
    app.checkpoint("insert");
    if let Some(l) = app.pres.insert_picture(&s, png, "png", &app.centered(pw, ph), name) { app.selection = vec![l]; }
}

fn icon_gallery(app: &mut SlidesApp, ui: &mut Ui) {
    let id = egui::Id::new("icon-search");
    let mut q: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Search icons")).desired_width(300.0));
    ui.data_mut(|d| d.insert_temp(id, q.clone()));
    let ql = q.to_lowercase();
    ui.set_width(320.0);
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let names: Vec<&str> = qalam_kit::icon_data::ICONS.iter().map(|e| e.0).chain(qalam_kit::slide_icon_data::ICONS.iter().map(|e| e.0)).filter(|n| ql.is_empty() || n.to_lowercase().contains(&ql)).collect();
            for n in names {
                let (r, resp) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
                if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
                paint_icon(ui, n, Rect::from_center_size(r.center(), vec2(32.0, 32.0)), 32, true);
                if resp.on_hover_text(n).clicked() { if let Some(png) = icon_png(n, 512) { insert_png(app, png, 96, 96, n); } ui.close(); }
            }
        });
    });
}

/// An icon rendered as a PNG of `px` pixels.
pub fn icon_png(name: &str, px: u32) -> Option<Vec<u8>> {
    let (body, defs) = qalam_kit::icon_data::ICONS.iter().find(|e| e.0 == name).map(|e| (e.2, qalam_kit::icon_data::DEFS))
        .or_else(|| qalam_kit::slide_icon_data::ICONS.iter().find(|e| e.0 == name).map(|e| (e.2, qalam_kit::slide_icon_data::DEFS)))?;
    let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="{px}" height="{px}" color="#2c2c2e"><defs>{defs}</defs>{body}</svg>"##);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let mut pm = resvg::tiny_skia::Pixmap::new(px, px)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pm.as_mut());
    pm.encode_png().ok()
}

fn insert_chart(app: &mut SlidesApp, kind: word_engine::objects::ChartKind) {
    let Some(s) = app.slide() else { return };
    let (sw, sh) = app.pres.size();
    app.checkpoint("chart");
    if let Some(l) = app.pres.insert_chart(&s, kind, &app.centered(sw * 0.6, sh * 0.6)) {
        app.selection = vec![l.clone()];
        if let Some(chart) = app.pres.chart(&l) { app.dialog = Some(Dialog::ChartData { loc: l, chart }); }
    }
}

fn zoom_menu(app: &mut SlidesApp, ui: &mut Ui) {
    if menu_item(ui, "zoomSummary", "Summary Zoom") { summary_zoom(app); ui.close(); }
    crate::theme::submenu(ui, tr("Slide Zoom"), |ui| {
        let slides = app.slides();
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| for (i, s) in slides.iter().enumerate() {
            if i == app.current { continue; }
            let t = app.pres.slide_title(s);
            if ui.button(format!("{}. {}", i + 1, if t.is_empty() { tr("Slide") } else { t })).clicked() { slide_zoom(app, i, None); ui.close(); }
        });
    });
    if menu_item(ui, "zoomSection", "Section Zoom") {
        let secs = app.pres.sections();
        if secs.is_empty() { app.notify(&tr("Add sections first (Home ▸ Section)")); }
        for (k, (_, first)) in secs.iter().enumerate() { slide_zoom(app, *first, Some(k)); }
        ui.close();
    }
}

/// A picture of a slide that jumps to it when clicked in the show.
fn slide_zoom(app: &mut SlidesApp, target: usize, index: Option<usize>) {
    let Some(s) = app.slide() else { return };
    let Some(part) = app.slides().get(target).cloned() else { return };
    let Some(png) = slides_engine::render::slide_png(&app.pres, &part, 640) else { return };
    let (sw, sh) = app.pres.size();
    let w = sw * 0.25;
    let h = w * sh / sw;
    let k = index.unwrap_or(0) as f64;
    let x = if index.is_some() { Xfrm { x: sw * 0.05 + (k % 3.0) * (w + sw * 0.05), y: sh * 0.3 + (k / 3.0).floor() * (h + sh * 0.05), w, h, ..Default::default() } } else { app.centered(w, h) };
    app.checkpoint("zoom");
    if let Some(l) = app.pres.insert_slide_zoom(&s, target, png, &x) { app.selection = vec![l]; }
}

/// A new slide with zooms to the first slide of each section (or every slide).
fn summary_zoom(app: &mut SlidesApp) {
    let secs = app.pres.sections();
    let targets: Vec<usize> = if secs.is_empty() { (0..app.slides().len()).take(6).collect() } else { secs.iter().map(|s| s.1).collect() };
    let layout = app.pres.all_layouts().into_iter().find(|l| app.pres.layout_type(l) == "titleOnly");
    app.new_slide(layout);
    if let Some(s) = app.slide() {
        let scene = app.pres.scene(&s);
        for it in scene.items.iter().filter(|i| i.own) { if it.placeholder.as_ref().is_some_and(|p| p.0 == "title") { app.pres.set_shape_text(&it.loc, None, &crate::i18n::raw("Summary")); } }
    }
    for (k, t) in targets.into_iter().enumerate() { let t = if t >= app.current { t + 1 } else { t }; slide_zoom(app, t, Some(k)); }
}

fn open_link(app: &mut SlidesApp) {
    let existing = app.selection.first().and_then(|l| app.pres.shape_link(l));
    let (target, address, slide) = match existing {
        Some((k, v)) if k == "slide" => (1, String::new(), v.parse().unwrap_or(0)),
        Some((k, v)) if k == "url" && v.starts_with("mailto:") => (2, v.trim_start_matches("mailto:").to_string(), 0),
        Some((_, v)) => (0, v, 0),
        None => (0, String::new(), 0),
    };
    app.dialog = Some(Dialog::Link { target, address, slide, text_only: app.edit.is_some() });
}

fn wordart_gallery(app: &mut SlidesApp, ui: &mut Ui) {
    ui.set_width(330.0);
    ui.horizontal_wrapped(|ui| for (k, (name, fill, outline, shadow, gradient, glow)) in slides_engine::format::WORDART.iter().enumerate() {
        let (r, resp) = ui.allocate_exact_size(vec2(60.0, 54.0), Sense::click());
        if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
        let col = |h: &str| { let c = slides_engine::Rgba::hex(h).unwrap_or(slides_engine::Rgba::BLACK); Color32::from_rgb(c.r, c.g, c.b) };
        let font = egui::FontId::new(30.0, egui::FontFamily::Name("ui-bold".into()));
        if *glow { for d in [-2.0, 2.0] { ui.painter().text(r.center() + vec2(d, 0.0), egui::Align2::CENTER_CENTER, "A", font.clone(), Color32::from_rgba_unmultiplied(255, 192, 0, 120)); } }
        if *shadow { ui.painter().text(r.center() + vec2(1.5, 1.5), egui::Align2::CENTER_CENTER, "A", font.clone(), Color32::from_black_alpha(90)); }
        if let Some(o) = outline { for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] { ui.painter().text(r.center() + vec2(dx, dy), egui::Align2::CENTER_CENTER, "A", font.clone(), col(o)); } }
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "A", font, if *gradient { col(fill).gamma_multiply(0.85) } else { col(fill) });
        if resp.on_hover_text(*name).clicked() {
            let Some(s) = app.slide() else { return };
            let (sw, sh) = app.pres.size();
            app.checkpoint("wordart");
            if let Some(l) = app.pres.insert_shape(&s, "rect", &Xfrm { x: sw * 0.2, y: sh * 0.4, w: sw * 0.6, h: sh * 0.2, ..Default::default() }, true) {
                app.pres.set_shape_text(&l, None, &crate::i18n::raw("Your text here"));
                app.pres.format_shape_text(&l, None, &slides_engine::edit::RunFormat { size: Some(54.0), bold: Some(true), ..Default::default() });
                app.pres.set_align(&l, None, None, "ctr");
                app.pres.set_wordart(&l, k);
                let n = app.pres.shape_text(&l, None).chars().count();
                app.selection = vec![l.clone()];
                app.edit = Some(crate::app::TextEdit { loc: l, cell: None, cursor: n, anchor: 0 });
            }
            ui.close();
        }
    });
}

fn insert_object(app: &mut SlidesApp) {
    let Some(p) = rfd::FileDialog::new().pick_file() else { return };
    let Ok(bytes) = std::fs::read(&p) else { return };
    let Some(s) = app.slide() else { return };
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let icon = icon_png("object", 256).unwrap_or_default();
    app.checkpoint("object");
    if let Some(l) = app.pres.insert_object_file(&s, bytes, &name, icon, &app.centered(914_400.0, 914_400.0)) { app.selection = vec![l]; }
}

fn insert_media(app: &mut SlidesApp, video: bool) {
    let exts: &[&str] = if video { &["mp4", "m4v", "mov", "wmv", "avi"] } else { &["mp3", "m4a", "wav", "wma", "aac"] };
    let Some(p) = rfd::FileDialog::new().add_filter(&tr(if video { "Video" } else { "Audio" }), exts).pick_file() else { return };
    let Ok(bytes) = std::fs::read(&p) else { return };
    let Some(s) = app.slide() else { return };
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let poster = icon_png(if video { "video" } else { "audio" }, 512).unwrap_or_default();
    let (sw, sh) = app.pres.size();
    let x = if video { app.centered(sw * 0.5, sh * 0.5) } else { app.centered(812_800.0, 812_800.0) };
    app.checkpoint("media");
    if let Some(l) = app.pres.insert_media(&s, bytes, &ext, poster, &x, video) { app.selection = vec![l]; }
}
