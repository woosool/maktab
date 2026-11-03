//! The Design tab: Themes, Variants (with Colors, Fonts, Effects and Background Styles),
//! Customize (Slide Size, Format Background) and Designer.

use crate::app::{Pane, SlidesApp};
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::design::{catalog, theme_deck};
use slides_engine::render::{self, Options};

const THUMB: egui::Vec2 = vec2(96.0, 54.0);

/// A theme deck's title layout, rendered small (cached).
fn theme_thumb(ui: &Ui, id: &str, variant: usize) -> Option<egui::TextureHandle> {
    let key = format!("theme-thumb-{id}-{variant}");
    if let Some(t) = ui.data(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new(&key))) { return Some(t); }
    let deck = theme_deck(id, variant)?;
    let layout = deck.all_layouts().into_iter().find(|l| deck.layout_type(l) == "title").or_else(|| deck.all_layouts().into_iter().next())?;
    let scene = deck.scene(&layout);
    let mut images = render::Images::default();
    let ppp = ui.ctx().pixels_per_point();
    let pm = render::render(&deck, &scene, &mut images, Options { scale: (THUMB.x * ppp * 2.0) as f64 / scene.w, prompts: false, skip_text_of: None, translate: None })?;
    let t = ui.ctx().load_texture(&key, egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR);
    ui.data_mut(|d| d.insert_temp(egui::Id::new(&key), t.clone()));
    Some(t)
}

fn tile(ui: &mut Ui, tex: Option<egui::TextureHandle>, label: &str, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(THUMB + vec2(6.0, 6.0), Sense::click());
    let img = r.shrink(3.0);
    ui.painter().rect_filled(img, 0.0, Color32::WHITE);
    if let Some(t) = tex { ui.painter().image(t.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
    // "Aa" over the thumbnail, as PowerPoint's gallery shows themes.
    ui.painter().text(img.min + vec2(8.0, 10.0), egui::Align2::LEFT_TOP, "Aa", egui::FontId::proportional(16.0), Color32::from_black_alpha(160));
    let stroke = if on { Stroke::new(2.5, ACCENT) } else if resp.hovered() { Stroke::new(2.0, ACCENT.gamma_multiply(0.6)) } else { Stroke::new(1.0, c_line()) };
    ui.painter().rect_stroke(img, 0.0, stroke, egui::StrokeKind::Outside);
    resp.on_hover_text(tr(label))
}

/// The slides a Design command applies to: the selected ones, or all.
fn targets(app: &SlidesApp) -> Vec<String> {
    let slides = app.slides();
    if app.selected_slides.len() > 1 { app.selected_slides.iter().filter_map(|&i| slides.get(i).cloned()).collect() } else { slides }
}

pub fn design(app: &mut SlidesApp, ui: &mut Ui) {
    let cat = catalog();
    let current_theme = app.slide().map(|s| app.pres.theme_name(&s)).unwrap_or_default();
    let current = cat.themes.iter().position(|t| current_theme.starts_with(&t.name) || t.variants.iter().any(|v| v.name == current_theme));
    group(ui, "Themes", |ui| {
        if cat.themes.is_empty() { ui.label(tr("Themes aren't installed (themes folder missing).")); return; }
        // A strip of the first themes, and the full gallery in a drop-down.
        // As many themes as the window has room for; all of them in the drop-down.
        let room = ((ui.ctx().content_rect().width() - 760.0) / (THUMB.x + 8.0)).floor().clamp(2.0, 7.0) as usize;
        for (k, t) in cat.themes.iter().enumerate().take(room) {
            if tile(ui, theme_thumb(ui, &t.id, 0), &t.name, current == Some(k)).clicked() { apply_theme(app, &t.id, 0); }
        }
        let more = large_menu(ui, "themes", "", "All themes");
        crate::theme::menu(&more, |ui| all_themes(app, ui));
    });
    group(ui, "Variants", |ui| {
        if let Some(k) = current {
            let t = &cat.themes[k];
            for (v, var) in t.variants.iter().enumerate().take(4) {
                if tile(ui, theme_thumb(ui, &t.id, v), &var.name, current_theme == var.name).clicked() { apply_theme(app, &t.id, v); }
            }
        } else {
            ui.label(egui::RichText::new(tr("Pick a theme to see its variants")).size(11.5).color(c_ink3()));
        }
        let more = large_menu(ui, "variants", "", "Colors, fonts, effects and background styles");
        crate::theme::menu(&more, |ui| variants_menu(app, ui));
    });
    group(ui, "Customize", |ui| {
        let size = large_menu(ui, "slideSize", "Slide Size", "Widescreen, standard or a custom size");
        crate::theme::menu(&size, |ui| {
            if menu_item(ui, "slideSize", "Standard (4:3)") { ask_scale(app, 9_144_000, 6_858_000); ui.close(); }
            if menu_item(ui, "slideSize", "Widescreen (16:9)") { ask_scale(app, 12_192_000, 6_858_000); ui.close(); }
            if menu_item(ui, "slideSize", "Custom Slide Size…") { let (w, h) = app.pres.size(); app.dialog = Some(crate::dialogs::Dialog::SlideSize { w: w / 914_400.0, h: h / 914_400.0, fit: None }); ui.close(); }
        });
        if large(ui, "formatBackground", "Format Background", app.pane == Pane::Background, true, "Background fill, picture or pattern").clicked() { app.pane = if app.pane == Pane::Background { Pane::None } else { Pane::Background }; }
    });
    group(ui, "Designer", |ui| {
        if large(ui, "designer", "Design Ideas", app.pane == Pane::Designer, true, "Design ideas for this slide").clicked() { app.pane = if app.pane == Pane::Designer { Pane::None } else { Pane::Designer }; }
    });
}

fn apply_theme(app: &mut SlidesApp, id: &str, variant: usize) {
    let Some(deck) = theme_deck(id, variant) else { app.error = Some(tr("That theme couldn't be loaded.")); return };
    let slides = targets(app);
    let all = slides.len() == app.slides().len();
    app.checkpoint("theme");
    app.pres.apply_theme_deck(&deck, if all { &[] } else { &slides });
    app.images = render::Images::default();
    app.thumbs.clear();
}

fn browse_theme(app: &mut SlidesApp) {
    let Some(p) = rfd::FileDialog::new().add_filter(&tr("Office Theme or Presentation"), &["thmx", "pptx", "potx"]).pick_file() else { return };
    match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| slides_engine::Presentation::open(&b)) {
        Ok(deck) => { app.checkpoint("theme"); app.pres.apply_theme_deck(&deck, &[]); app.thumbs.clear(); }
        Err(e) => app.error = Some(crate::i18n::trf("Couldn't open the presentation.\n\n{}", &[&e])),
    }
}

/// Save the current design as a template presentation (.potx) with no slides.
fn save_theme(app: &mut SlidesApp) {
    let Some(p) = rfd::FileDialog::new().add_filter(&tr("PowerPoint Template"), &["potx", "pptx"]).set_file_name("Theme.potx").save_file() else { return };
    let mut copy = app.pres.clone();
    for s in copy.slides() { copy.delete_slide(&s); }
    match copy.save().and_then(|b| std::fs::write(&p, b).map_err(|e| e.to_string())) { Ok(()) => app.notify(&tr("Saved")), Err(e) => app.error = Some(e) }
}

fn ask_scale(app: &mut SlidesApp, w: i64, h: i64) {
    app.dialog = Some(crate::dialogs::Dialog::SlideSize { w: w as f64 / 914_400.0, h: h as f64 / 914_400.0, fit: Some(true) });
}

pub fn variants_menu(app: &mut SlidesApp, ui: &mut Ui) {
    let cat = catalog();
    let masters = app.pres.masters_of(&targets(app));
    crate::theme::submenu(ui, tr("Colors"), |ui| {
        ui.set_width(300.0);
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| for scheme in &cat.color_schemes {
            let (r, resp) = ui.allocate_exact_size(vec2(290.0, 24.0), Sense::click());
            if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
            for (k, slot) in ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6"].iter().enumerate() {
                let c = scheme.colors.iter().find(|c| c.0 == *slot).and_then(|c| slides_engine::Rgba::hex(&c.1)).unwrap_or(slides_engine::Rgba::WHITE);
                let sw = Rect::from_min_size(r.min + vec2(6.0 + k as f32 * 13.0, 5.0), vec2(12.0, 14.0));
                ui.painter().rect(sw, 0.0, Color32::from_rgb(c.r, c.g, c.b), Stroke::new(0.5, c_line()), egui::StrokeKind::Inside);
            }
            ui.painter().text(r.min + vec2(144.0, 12.0), egui::Align2::LEFT_CENTER, &scheme.name, egui::FontId::proportional(12.5), c_ink());
            if resp.clicked() { app.checkpoint("colors"); app.pres.set_theme_colors(&masters, &scheme.name, &scheme.colors, Some(scheme.dark)); app.thumbs.clear(); ui.close(); }
        });
    });
    crate::theme::submenu(ui, tr("Fonts"), |ui| {
        ui.set_width(260.0);
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| for (name, major, minor) in &cat.font_pairs {
            let (r, resp) = ui.allocate_exact_size(vec2(250.0, 44.0), Sense::click());
            if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
            ui.painter().text(r.min + vec2(8.0, 6.0), egui::Align2::LEFT_TOP, "Aa", egui::FontId::proportional(20.0), c_ink());
            ui.painter().text(r.min + vec2(48.0, 6.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(12.5), c_ink());
            ui.painter().text(r.min + vec2(48.0, 24.0), egui::Align2::LEFT_TOP, format!("{major} / {minor}"), egui::FontId::proportional(11.0), c_ink3());
            if resp.clicked() { app.checkpoint("fonts"); app.pres.set_theme_fonts(&masters, name, major, minor); ui.close(); }
        });
    });
    crate::theme::submenu(ui, tr("Effects"), |ui| for e in &cat.effects {
        if ui.button(tr(&e.name)).clicked() { app.checkpoint("effects"); app.pres.set_theme_effects(&masters, e); ui.close(); }
    });
    crate::theme::submenu(ui, tr("Background Styles"), |ui| {
        let slide = app.slide().unwrap_or_default();
        let (theme, map) = slides_engine::scene::color_context(&app.pres, &slide);
        let ctx = slides_engine::color::Ctx { scheme: theme.scheme.clone(), map, ph: None };
        ui.set_width(4.0 * 64.0);
        for row in 0..3 {
            ui.allocate_ui_with_layout(vec2(4.0 * 62.0, 42.0), egui::Layout::left_to_right(egui::Align::Center), |ui| for col in 0..4 {
                let style = row * 4 + col;
                let token = ["bg1", "bg2", "tx2", "tx1"][col];
                let c = ctx.scheme_color(token).unwrap_or(slides_engine::Rgba::WHITE);
                let base = Color32::from_rgb(c.r, c.g, c.b);
                let (r, resp) = ui.allocate_exact_size(vec2(58.0, 38.0), Sense::click());
                let (top, bottom) = match row { 0 => (base, base), 1 => (base.lerp_to_gamma(Color32::WHITE, 0.15), base.lerp_to_gamma(Color32::BLACK, 0.12)), _ => (base.lerp_to_gamma(Color32::WHITE, 0.3), base.lerp_to_gamma(Color32::BLACK, 0.3)) };
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(r.left_top(), top); mesh.colored_vertex(r.right_top(), top); mesh.colored_vertex(r.left_bottom(), bottom); mesh.colored_vertex(r.right_bottom(), bottom);
                mesh.add_triangle(0, 1, 2); mesh.add_triangle(1, 3, 2);
                ui.painter().add(egui::Shape::mesh(mesh));
                ui.painter().rect_stroke(r, 0.0, Stroke::new(if resp.hovered() { 2.0 } else { 1.0 }, if resp.hovered() { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
                if resp.on_hover_text(crate::i18n::trf("Style {}", &[&(style + 1)])).clicked() { let t = targets(app); app.checkpoint("background"); app.pres.set_background_style(&t, style); ui.close(); }
            });
        }
        ui.separator();
        if ui.button(tr("Format Background…")).clicked() { app.pane = Pane::Background; ui.close(); }
        if ui.button(tr("Reset Slide Background")).clicked() { let t = targets(app); app.checkpoint("background"); for s in t { app.pres.set_background_fill(&s, None); } ui.close(); }
    });
}

/// The Format Background pane.
pub fn background_pane(app: &mut SlidesApp, ui: &mut Ui) {
    let Some(slide) = app.slide() else { return };
    let id = egui::Id::new("bg-kind");
    let mut kind: u8 = ui.data(|d| d.get_temp(id)).unwrap_or(0);
    crate::theme::section(ui, "Fill");
    for (k, n) in [(0u8, "Solid fill"), (1, "Gradient fill"), (2, "Picture or texture fill"), (3, "Pattern fill")] { crate::theme::radio_value(ui, &mut kind, k, tr(n)); }
    ui.data_mut(|d| d.insert_temp(id, kind));
    let cid = egui::Id::new("bg-color");
    let mut c: [u8; 3] = ui.data(|d| d.get_temp(cid)).unwrap_or([0xFF, 0xFF, 0xFF]);
    let mut col = word_engine::Color(c[0], c[1], c[2]);
    let hex = |c: word_engine::Color| format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2);
    match kind {
        0 => { crate::theme::hrow(ui, |ui| { ui.label(tr("Color")); if color_button(ui, &mut col) { app.checkpoint("background"); app.pres.set_background_fill(&slide, Some(&format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, hex(col)))); } }); }
        1 => {
            crate::theme::hrow(ui, |ui| { ui.label(tr("Color")); color_button(ui, &mut col); });
            ui.horizontal_wrapped(|ui| for v in 0..8 {
                if ui.button(format!("{}", v + 1)).on_hover_text(tr(if v < 4 { "Light Variations" } else { "Dark Variations" })).clicked() {
                    let (stops, angle, radial) = slides_engine::format::gradient_variation(&hex(col), v);
                    let gs: String = stops.iter().map(|(p, c)| format!(r#"<a:gs pos="{}"><a:srgbClr val="{c}"/></a:gs>"#, (p * 100_000.0) as i64)).collect();
                    let shape = if radial { r#"<a:path path="circle"><a:fillToRect l="50000" t="50000" r="50000" b="50000"/></a:path>"#.to_string() } else { format!(r#"<a:lin ang="{}" scaled="0"/>"#, (angle * 60_000.0) as i64) };
                    app.checkpoint("background");
                    app.pres.set_background_fill(&slide, Some(&format!(r#"<a:gradFill rotWithShape="1"><a:gsLst>{gs}</a:gsLst>{shape}</a:gradFill>"#)));
                }
            });
        }
        2 => {
            let tid = egui::Id::new("bg-transp");
            let mut tr_: f64 = ui.data(|d| d.get_temp(tid)).unwrap_or(0.0);
            crate::theme::hrow(ui, |ui| { ui.label(tr("Transparency")); ui.add(egui::Slider::new(&mut tr_, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))); });
            ui.data_mut(|d| d.insert_temp(tid, tr_));
            if ui.button(tr("Insert picture…")).clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter(&tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() {
                    if let Ok(b) = std::fs::read(&f) { let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into()); app.checkpoint("background"); app.pres.set_background_picture(&slide, b, &ext, false, tr_); }
                }
            }
            crate::theme::submenu(ui, tr("Texture"), |ui| for t in slides_engine::format::TEXTURES { if ui.button(tr(t)).clicked() { app.checkpoint("background"); app.pres.set_background_picture(&slide, slides_engine::format::texture_png(t), "png", true, tr_); ui.close(); } });
        }
        _ => {
            crate::theme::hrow(ui, |ui| { ui.label(tr("Foreground")); color_button(ui, &mut col); });
            ui.horizontal_wrapped(|ui| for pr in ["pct10", "pct25", "pct50", "ltHorz", "dkVert", "ltDnDiag", "smGrid", "lgCheck", "dotGrid", "diagCross", "horzBrick", "weave", "wave", "zigZag"] {
                let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                for y in 0..8u32 { for x in 0..8u32 { if slides_engine::render::pattern_bit(pr, x, y) { ui.painter().rect_filled(Rect::from_min_size(r.min + vec2(x as f32 * 2.75, y as f32 * 2.75), vec2(2.75, 2.75)), 0.0, c_ink()); } } }
                ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, if resp.hovered() { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
                if resp.clicked() { app.checkpoint("background"); app.pres.set_background_fill(&slide, Some(&format!(r#"<a:pattFill prst="{pr}"><a:fgClr><a:srgbClr val="{}"/></a:fgClr><a:bgClr><a:srgbClr val="FFFFFF"/></a:bgClr></a:pattFill>"#, hex(col)))); }
            });
        }
    }
    c = [col.0, col.1, col.2];
    ui.data_mut(|d| d.insert_temp(cid, c));
    ui.separator();
    let mut hide = app.pres.hides_background_graphics(&slide);
    if crate::theme::check(ui, &mut hide, tr("Hide background graphics")).changed() { app.checkpoint("background"); app.pres.set_hide_background_graphics(&slide, hide); }
    crate::theme::hrow(ui, |ui| {
        if crate::theme::secondary(ui, "Apply to All").clicked() {
            let bg = app.pres.slide(&slide).and_then(|r| r.path("p:cSld/p:bg").cloned());
            let hide = app.pres.hides_background_graphics(&slide);
            app.checkpoint("background");
            for s in app.slides() {
                if let Some(csld) = app.pres.pkg.xml_mut(&s).and_then(|r| r.child_mut("p:cSld")) { csld.remove_children("p:bg"); if let Some(b) = &bg { csld.children.insert(0, slides_engine::xml::Node::Element(b.clone())); } }
                // Pictures in the background need the slide's relationship.
                if let (Some(b), true) = (&bg, s != slide) { if let Some(id) = b.find("a:blip").and_then(|x| x.attr("r:embed")) { if let Some(t) = app.pres.pkg.rel_target(&slide, id) { let nid = app.pres.pkg.add_rel(&s, slides_engine::opc::RT_IMAGE, &t, false); if let Some(bl) = app.pres.pkg.xml_mut(&s).and_then(|r| r.path_mut("p:cSld/p:bg")).and_then(|bgx| { fn f(e: &mut slides_engine::xml::Element) -> Option<&mut slides_engine::xml::Element> { if e.name == "a:blip" { return Some(e); } for c in e.elements_mut() { if let Some(x) = f(c) { return Some(x); } } None } f(bgx) }) { bl.set_attr("r:embed", nid); } } } }
                app.pres.set_hide_background_graphics(&s, hide);
            }
        }
        if crate::theme::secondary(ui, "Reset Background").clicked() { app.checkpoint("background"); app.pres.set_background_fill(&slide, None); }
    });
}

/// Every theme, with Browse and Save (the Themes drop-down; also Slide Master's Themes).
pub fn all_themes(app: &mut SlidesApp, ui: &mut Ui) {
    let cat = catalog();
    let current_theme = app.slide().map(|s| app.pres.theme_name(&s)).unwrap_or_default();
    let current = cat.themes.iter().position(|t| current_theme.starts_with(&t.name) || t.variants.iter().any(|v| v.name == current_theme));
    menu_heading(ui, "Office");
    ui.set_width(5.0 * (THUMB.x + 8.0));
    ui.horizontal_wrapped(|ui| for (k, t) in cat.themes.iter().enumerate() {
        if tile(ui, theme_thumb(ui, &t.id, 0), &t.name, current == Some(k)).clicked() { apply_theme(app, &t.id, 0); ui.close(); }
    });
    ui.separator();
    if menu_item(ui, "folderOpen", "Browse for Themes…") { browse_theme(app); ui.close(); }
    if menu_item(ui, "save", "Save Current Theme…") { save_theme(app); ui.close(); }
}
