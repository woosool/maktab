//! SmartArt and Mermaid editing: the SmartArt dialog (layouts by category with pictures,
//! the text pane, colors, styles, right-to-left, a live preview), the Mermaid editor (code
//! with templates, a live preview and errors), and the SmartArt Design tab shown when one is
//! selected (add, promote, demote, reorder, layouts, colors, styles, reset, convert).

use crate::app::SlidesApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::diagram::{self, Category, Palette, Spec, CATEGORIES, COLORS, LAYOUTS, STYLES};
use slides_engine::scene::{Loc, Xfrm};

pub struct Editor {
    pub loc: Option<Loc>,
    pub spec: Spec,
    pub text: String,
    pub cat: Option<Category>,
    pub mermaid: bool,
    pub code: String,
    pub error: Option<String>,
    preview: Option<(String, egui::TextureHandle)>,
    /// A text shape being converted (replaced by the graphic).
    pub replace: Option<Loc>,
}

impl Editor {
    pub fn smartart(loc: Option<Loc>, spec: Spec) -> Editor { let text = spec.text(); Editor { loc, spec, text, cat: None, mermaid: false, code: String::new(), error: None, preview: None, replace: None } }
    pub fn mermaid(loc: Option<Loc>, code: String) -> Editor { Editor { loc, spec: Spec::new("basicProcess", ""), text: String::new(), cat: None, mermaid: true, code, error: None, preview: None, replace: None } }
}

pub fn open_smartart(app: &mut SlidesApp, text: &str) {
    let text = if text.trim().is_empty() { tr("Plan\nBuild\nTest\nLaunch") } else { text.to_string() };
    app.diagram_editor = Some(Editor::smartart(None, Spec::new("basicProcess", &text)));
}
pub fn open_mermaid(app: &mut SlidesApp) { app.diagram_editor = Some(Editor::mermaid(None, slides_engine::mermaid::TEMPLATES[0].1.to_string())); }

/// Edit what's selected (SmartArt or Mermaid), if it is one.
pub fn edit_selected(app: &mut SlidesApp, loc: &Loc) -> bool {
    if let Some(code) = app.pres.mermaid(loc) { app.diagram_editor = Some(Editor::mermaid(Some(loc.clone()), code)); return true; }
    if let Some(spec) = app.pres.diagram(loc) { app.diagram_editor = Some(Editor::smartart(Some(loc.clone()), spec)); return true; }
    false
}

fn palette(app: &SlidesApp) -> Palette { app.slide().map(|s| Palette::of(&app.pres, &s)).unwrap_or_default() }

/// Draw nodes made by `make` (in a 16:9 frame) into a texture.
fn picture(ctx: &egui::Context, key: &str, w: u32, make: impl FnOnce(&Xfrm) -> Result<Vec<diagram::Node>, String>) -> Result<egui::TextureHandle, String> {
    crate::texcache::get_or(ctx, &format!("diagram{w}{key}"), || {
        let mut p = slides_engine::Presentation::new_blank(true);
        let l = p.all_layouts().last().cloned().ok_or("no layout")?;
        let s = p.add_slide(&l, 0);
        let (sw, sh) = p.size();
        let fr = Xfrm { x: sw * 0.04, y: sh * 0.06, w: sw * 0.92, h: sh * 0.88, ..Default::default() };
        let nodes = make(&fr)?;
        p.insert_nodes(&s, &nodes, "", "");
        let mut scene = p.scene(&s);
        scene.background = slides_engine::scene::Fill::None;
        let mut images = slides_engine::render::Images::default();
        let pm = slides_engine::render::render(&p, &scene, &mut images, slides_engine::render::Options { scale: w as f64 / sw, prompts: false, skip_text_of: None, translate: None }).ok_or("render")?;
        Ok(ctx.load_texture("diagram", egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR))
    })
}

fn spec_picture(ctx: &egui::Context, spec: &Spec, pal: &Palette, w: u32) -> Option<egui::TextureHandle> {
    let key = format!("{}{}", spec.to_string(), pal.accents.join(""));
    picture(ctx, &key, w, |fr| {
        let mut n = diagram::layout(&spec.layout, &spec.items, fr);
        diagram::style(&mut n, &spec.colors, &spec.style, pal);
        if spec.rtl { diagram::mirror(&mut n, fr); }
        Ok(n)
    }).ok()
}

/// A layout's sample picture.
fn layout_tile(ui: &mut Ui, id: &str, name: &str, on: bool, pal: &Palette, colors: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(96.0, 80.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr(name)));
    if on { ui.painter().rect_filled(r, 4.0, ACCENT.gamma_multiply(0.15)); } else if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
    let sample = if id.contains("ierarchy") || id == "orgChart" { "A\n\tB\n\tC\n\t\tD\n\t\tE" } else { "A\n\tx\nB\nC\nD" };
    let spec = Spec { layout: id.into(), colors: colors.into(), style: "flat".into(), items: diagram::parse_items(sample).into_iter().map(|(l, _)| (l, String::new())).collect(), rtl: false };
    if let Some(t) = spec_picture(ui.ctx(), &spec, pal, 160) { ui.painter().image(t.id(), Rect::from_center_size(pos2(r.center().x, r.min.y + 32.0), vec2(86.0, 48.0)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
    let g = ui.painter().layout(tr(name), egui::FontId::proportional(10.0), c_ink2(), 92.0);
    ui.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.min.y + 58.0), g, c_ink2());
    if on { ui.painter().rect_stroke(r, 4.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside); }
    resp.on_hover_text(tr(name))
}

/// A color scheme's swatch strip.
fn colors_tile(ui: &mut Ui, id: &str, name: &str, on: bool, pal: &Palette) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(62.0, 34.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr(name)));
    if resp.hovered() || on { ui.painter().rect_filled(r, 3.0, if on { ACCENT.gamma_multiply(0.15) } else { c_hover() }); }
    let mut nodes: Vec<diagram::Node> = (0..4).map(|k| diagram::Node::shape("rect", Xfrm { x: k as f64, y: 0.0, w: 1.0, h: 1.0, ..Default::default() }, "x", 0)).collect();
    diagram::style(&mut nodes, id, "flat", pal);
    for (k, n) in nodes.iter().enumerate() {
        let c = match &n.fill { diagram::Paint::Solid(h) | diagram::Paint::Gradient(h, _) => crate::show::hex_color(h), _ => Color32::WHITE };
        let b = Rect::from_min_size(r.min + vec2(5.0 + k as f32 * 13.0, 7.0), vec2(12.0, 20.0));
        ui.painter().rect_filled(b, 1.0, c);
        if let Some((l, _, _)) = &n.line { ui.painter().rect_stroke(b, 1.0, Stroke::new(1.0, crate::show::hex_color(l)), egui::StrokeKind::Inside); }
    }
    if on { ui.painter().rect_stroke(r, 3.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside); }
    resp.on_hover_text(tr(name))
}

pub fn window(app: &mut SlidesApp, ctx: &egui::Context) {
    let Some(mut ed) = app.diagram_editor.take() else { return };
    let mut open = true;
    let mut done = false;
    let pal = palette(app);
    let title = if ed.mermaid { tr("Mermaid Diagram") } else { tr("SmartArt Graphic") };
    crate::theme::dialog(ctx, &title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        if ed.mermaid { mermaid_body(app, ui, &mut ed, &pal, &mut done); } else { smartart_body(app, ui, &mut ed, &pal, &mut done); }
    });
    if open && !done { app.diagram_editor = Some(ed); }
}

fn smartart_body(app: &mut SlidesApp, ui: &mut Ui, ed: &mut Editor, pal: &Palette, done: &mut bool) {
    ui.horizontal_top(|ui| {
        // Categories.
        ui.vertical(|ui| {
            ui.set_width(110.0);
            if ui.selectable_label(ed.cat.is_none(), tr("All")).clicked() { ed.cat = None; }
            for (c, n) in CATEGORIES { if ui.selectable_label(ed.cat == Some(*c), tr(n)).clicked() { ed.cat = Some(*c); } }
            ui.add_space(12.0);
            if ui.button(tr("Mermaid…")).on_hover_text(tr("Draw a diagram from Mermaid code")).clicked() { let loc = ed.loc.clone(); *ed = Editor::mermaid(None, slides_engine::mermaid::TEMPLATES[0].1.into()); ed.replace = loc; }
        });
        ui.add_space(10.0);
        // Layouts.
        ui.vertical(|ui| {
            ui.set_width(3.0 * 100.0 + 10.0);
            egui::ScrollArea::vertical().id_salt("sa-layouts").max_height(470.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| for (id, name, cat) in LAYOUTS.iter().filter(|l| ed.cat.is_none_or(|c| c == l.2)) {
                    let _ = cat;
                    if layout_tile(ui, id, name, ed.spec.layout == *id, pal, &ed.spec.colors).clicked() { ed.spec.layout = id.to_string(); }
                });
            });
        });
        ui.add_space(10.0);
        // The text pane, the look, the preview.
        ui.vertical(|ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(LAYOUTS.iter().find(|l| l.0 == ed.spec.layout).map(|l| tr(l.1)).unwrap_or_default()).strong().size(15.0));
            if let Some(t) = spec_picture(ui.ctx(), &Spec { items: diagram::parse_items(&ed.text), ..ed.spec.clone() }, pal, 640) {
                let (r, _) = ui.allocate_exact_size(vec2(360.0, 203.0), Sense::hover());
                ui.painter().rect_filled(r, 4.0, Color32::WHITE);
                ui.painter().rect_stroke(r, 4.0, Stroke::new(1.0, c_line()), egui::StrokeKind::Inside);
                ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            ui.label(RichText::new(tr("Type your text here (Tab for a lower level, Shift+Tab for higher):")).size(11.5).color(c_ink3()));
            text_pane(ui, &mut ed.text);
            crate::theme::hrow(ui, |ui| {
                ui.label(tr("Colors"));
                ui.horizontal_wrapped(|ui| for (id, name) in COLORS.iter().take(6) { if colors_tile(ui, id, name, ed.spec.colors == *id, pal).clicked() { ed.spec.colors = id.to_string(); } });
            });
            crate::theme::hrow(ui, |ui| {
                ui.label(tr("More"));
                crate::theme::Combo::from_id_salt("sa-colors").width(220.0).selected_text(COLORS.iter().find(|c| c.0 == ed.spec.colors).map(|c| tr(c.1)).unwrap_or_default()).show_ui(ui, |ui| for (id, name) in COLORS { ui.selectable_value(&mut ed.spec.colors, id.to_string(), tr(name)); });
            });
            crate::theme::hrow(ui, |ui| {
                ui.label(tr("Style"));
                crate::theme::Combo::from_id_salt("sa-style").width(180.0).selected_text(STYLES.iter().find(|c| c.0 == ed.spec.style).map(|c| tr(c.1)).unwrap_or_default()).show_ui(ui, |ui| for (id, name) in STYLES { ui.selectable_value(&mut ed.spec.style, id.to_string(), tr(name)); });
                crate::theme::check(ui, &mut ed.spec.rtl, tr("Right to Left"));
            });
        });
    });
    let ok = crate::theme::footer(ui, |ui| {
        let ok = crate::theme::primary(ui, "OK").clicked();
        if crate::theme::secondary(ui, "Cancel").clicked() { *done = true; }
        ok
    });
    if ok {
        let spec = Spec { items: diagram::parse_items(&ed.text), ..ed.spec.clone() };
        if !spec.items.is_empty() { apply_spec(app, ed.loc.clone(), ed.replace.clone(), &spec); }
        *done = true;
    }
}

/// A text box where Tab and Shift+Tab change the line's level.
fn text_pane(ui: &mut Ui, text: &mut String) {
    let id = egui::Id::new("sa-text");
    let out = egui::TextEdit::multiline(text).id(id).desired_width(360.0).desired_rows(8).lock_focus(true).font(egui::TextStyle::Monospace).show(ui);
    if out.response.has_focus() {
        let (tab, shift) = ui.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, egui::Key::Tab), i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab)));
        // (A plain Tab was typed as a tab already by the text box; Shift+Tab lifts the line.)
        let _ = tab;
        if shift {
            if let Some(c) = out.cursor_range {
                let at = c.primary.index.0.min(text.chars().count());
                let byte = text.char_indices().nth(at).map(|(b, _)| b).unwrap_or(text.len());
                let start = text[..byte].rfind('\n').map(|k| k + 1).unwrap_or(0);
                if text[start..].starts_with('\t') { text.remove(start); }
            }
        }
    }
}

fn mermaid_body(app: &mut SlidesApp, ui: &mut Ui, ed: &mut Editor, pal: &Palette, done: &mut bool) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(380.0);
            crate::theme::hrow(ui, |ui| {
                ui.label(tr("Start from:"));
                crate::theme::Combo::from_id_salt("mm-tpl").width(170.0).selected_text(tr("Template")).show_ui(ui, |ui| for (name, src) in slides_engine::mermaid::TEMPLATES { if ui.selectable_label(false, tr(name)).clicked() { ed.code = src.to_string(); } });
                ui.hyperlink_to(tr("Syntax"), "https://mermaid.js.org/intro/syntax-reference.html");
            });
            egui::ScrollArea::vertical().id_salt("mm-code").max_height(380.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut ed.code).code_editor().desired_width(370.0).desired_rows(20).lock_focus(true));
            });
            ui.label(RichText::new(tr("Supported: flowchart / graph, sequenceDiagram, stateDiagram, classDiagram, erDiagram, gantt, pie, mindmap, timeline.")).size(11.0).color(c_ink3()));
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.set_width(420.0);
            let key = format!("mm{}{}", ed.code, pal.accents.join(""));
            let code = ed.code.clone();
            let pal2 = pal.clone();
            let r = picture(ui.ctx(), &key, 760, move |fr| slides_engine::mermaid::nodes(&code, fr, &pal2));
            let (rect, _) = ui.allocate_exact_size(vec2(420.0, 236.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, Color32::WHITE);
            ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, c_line()), egui::StrokeKind::Inside);
            match r {
                Ok(t) => { ui.painter().image(t.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); ed.error = None; }
                Err(e) => { ed.error = Some(e); }
            }
            if let Some(e) = &ed.error { ui.colored_label(Color32::from_rgb(0xC0, 0x30, 0x20), tr(e)); }
            ui.label(RichText::new(tr("The diagram is made of normal shapes: after inserting you can restyle it, or double-click it to edit the code.")).size(11.0).color(c_ink3()));
        });
    });
    let ok = crate::theme::footer(ui, |ui| {
        let ok = ui.add_enabled_ui(ed.error.is_none(), |ui| crate::theme::primary(ui, if ed.loc.is_some() { "Update" } else { "Insert Diagram" })).inner.clicked();
        if crate::theme::secondary(ui, "Cancel").clicked() { *done = true; }
        ok
    });
    if ok {
        let Some(slide) = app.slide() else { return };
        app.checkpoint("diagram");
        let r = match ed.loc.clone().or(ed.replace.clone()) {
            Some(l) => app.pres.update_mermaid(&l, &ed.code),
            None => { let (sw, sh) = app.pres.size(); let x = Xfrm { x: sw * 0.06, y: sh * 0.12, w: sw * 0.88, h: sh * 0.8, ..Default::default() }; app.pres.insert_mermaid(&slide, &ed.code, &x) }
        };
        match r { Ok(l) => { app.selection = vec![l]; app.changed(); *done = true; } Err(e) => ed.error = Some(e) }
    }
}

/// Insert or rebuild a SmartArt graphic (replacing a converted text shape).
pub fn apply_spec(app: &mut SlidesApp, loc: Option<Loc>, replace: Option<Loc>, spec: &Spec) {
    let Some(slide) = app.slide() else { return };
    app.checkpoint("smartart");
    let new = match (loc, replace) {
        (Some(l), _) => app.pres.update_diagram(&l, spec),
        (None, Some(t)) => { let x = app.pres.xfrm(&t); app.pres.delete_shapes(std::slice::from_ref(&t)); x.and_then(|x| app.pres.insert_diagram(&slide, spec, &x)) }
        (None, None) => { let (sw, sh) = app.pres.size(); let x = Xfrm { x: sw * 0.1, y: sh * 0.22, w: sw * 0.8, h: sh * 0.66, ..Default::default() }; app.pres.insert_diagram(&slide, spec, &x) }
    };
    app.edit = None;
    if let Some(l) = new { app.selection = vec![l]; }
    app.changed();
}

// ---- the SmartArt Design tab ---------------------------------------------------------------

/// The selected SmartArt (or Mermaid diagram), if one is selected alone.
pub fn selected(app: &SlidesApp) -> Option<(Loc, Option<Spec>)> {
    if app.selection.len() != 1 { return None; }
    let l = Loc { part: app.selection[0].part.clone(), path: vec![app.selection[0].path[0]] };
    if let Some(s) = app.pres.diagram(&l) { return Some((l, Some(s))); }
    app.pres.mermaid(&l).map(|_| (l, None))
}

/// An edit on one item; the result stays picked.
fn item_op(app: &mut SlidesApp, k: usize, op: &str, text: &str) {
    let Some((_, Some(mut spec))) = selected(app) else { return };
    let next = diagram::edit_item(&mut spec.items, k, op, text);
    change(app, |s| s.items = spec.items.clone());
    if let (Some((l, _)), Some(n)) = (selected(app), next) { app.diagram_item = Some((l, n)); }
}

fn change(app: &mut SlidesApp, f: impl FnOnce(&mut Spec)) {
    let Some((loc, Some(mut spec))) = selected(app) else { return };
    f(&mut spec);
    if spec.items.is_empty() { return; }
    apply_spec(app, Some(loc), None, &spec);
}

pub fn design_tab(app: &mut SlidesApp, ui: &mut Ui) {
    let Some((loc, spec)) = selected(app) else { return };
    let pal = palette(app);
    let Some(spec) = spec else {
        group(ui, "Mermaid", |ui| {
            if large(ui, "smartArt", "Edit Code", false, true, "Edit the diagram's Mermaid code").clicked() { edit_selected(app, &loc); }
            if large(ui, "ungroup", "Convert to Shapes", false, true, "Keep the shapes, forget the code").clicked() { convert_to_shapes(app, &loc); }
        });
        return;
    };
    // The item the buttons act on: the one clicked in the graphic, else the last.
    let k = app.diagram_item.as_ref().filter(|d| d.0 == loc).map(|d| d.1).unwrap_or(spec.items.len().saturating_sub(1));
    let picked = spec.items.get(k).map(|i| i.1.clone()).unwrap_or_default();
    group(ui, "Create Graphic", |ui| {
        rows(ui, |ui| {
            let add = small_menu(ui, "newSlide", "Add Shape", "Add an item next to the picked one");
            crate::theme::menu(&add, |ui| {
                for (op, label) in [("after", "Add Shape After"), ("before", "Add Shape Before"), ("above", "Add Shape Above"), ("below", "Add Shape Below")] {
                    if ui.button(tr(label)).clicked() { item_op(app, k, op, &tr("New")); ui.close(); }
                }
            });
            if small(ui, "bullets", Some("Add Bullet"), false, true, "Add a sub-item to the picked item").clicked() { item_op(app, k, "below", &tr("Text")); }
            if small(ui, "textBox", Some("Text Pane"), false, true, "Edit the graphic's text").clicked() { edit_selected(app, &loc); }
        });
        rows(ui, |ui| {
            if small(ui, "indentLess", Some("Promote"), false, spec.items.get(k).is_some_and(|i| i.0 > 0), "Raise the picked item a level").clicked() { item_op(app, k, "promote", ""); }
            if small(ui, "indentMore", Some("Demote"), false, k > 0, "Lower the picked item a level").clicked() { item_op(app, k, "demote", ""); }
            if small(ui, "readingOrder", Some("Right to Left"), spec.rtl, true, "Mirror the graphic").clicked() { change(app, |s| s.rtl = !s.rtl); }
        });
        rows(ui, |ui| {
            if small(ui, "moveEarlier", Some("Move Up"), false, true, "Move the picked item before the previous one").clicked() { item_op(app, k, "up", ""); }
            if small(ui, "moveLater", Some("Move Down"), false, true, "Move the picked item after the next one").clicked() { item_op(app, k, "down", ""); }
            if small(ui, "deleteSlide", Some("Remove"), false, spec.items.len() > 1, "Remove the picked item").clicked() { item_op(app, k, "remove", ""); }
        });
        // The picked item's text, edited in place.
        rows(ui, |ui| {
            ui.label(RichText::new(crate::i18n::trf("Item {}", &[&(k + 1)])).size(11.0).color(c_ink3()));
            let mut t = picked.clone();
            let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(120.0));
            if r.lost_focus() && t != picked { let t2 = t.clone(); change(app, move |s| if let Some(i) = s.items.get_mut(k) { i.1 = t2; }); app.diagram_item = selected(app).map(|(l, _)| (l, k)); }
        });
    });
    group(ui, "Layouts", |ui| {
        let room = ((ui.ctx().content_rect().width() - 1000.0) / 98.0).floor().clamp(2.0, 6.0) as usize;
        let same: Vec<_> = LAYOUTS.iter().filter(|l| LAYOUTS.iter().find(|x| x.0 == spec.layout).is_some_and(|c| c.2 == l.2)).chain(LAYOUTS.iter()).take(room).collect();
        let mut seen = vec![];
        for (id, name, _) in same { if seen.contains(id) { continue; } seen.push(*id); if layout_tile(ui, id, name, spec.layout == *id, &pal, &spec.colors).clicked() { let id = id.to_string(); change(app, |s| s.layout = id); } }
        let more = large_menu(ui, "more", "", "All layouts");
        crate::theme::menu(&more, |ui| {
            ui.set_width(5.0 * 100.0);
            egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| for (c, n) in CATEGORIES {
                menu_heading(ui, n);
                ui.horizontal_wrapped(|ui| for (id, name, _) in LAYOUTS.iter().filter(|l| l.2 == *c) { if layout_tile(ui, id, name, spec.layout == *id, &pal, &spec.colors).clicked() { let id = id.to_string(); change(app, |s| s.layout = id); ui.close(); } });
            });
        });
    });
    group(ui, "SmartArt Styles", |ui| {
        let cc = large_menu(ui, "colors", "Change Colors", "Color schemes from the theme");
        crate::theme::menu(&cc, |ui| {
            ui.set_width(4.0 * 66.0);
            ui.horizontal_wrapped(|ui| for (id, name) in COLORS { if colors_tile(ui, id, name, spec.colors == *id, &pal).clicked() { let id = id.to_string(); change(app, |s| s.colors = id); ui.close(); } });
        });
        rows(ui, |ui| {
            for (id, name) in STYLES.iter().take(3) { if ui.selectable_label(spec.style == *id, tr(name)).clicked() { let id = id.to_string(); change(app, |s| s.style = id); } }
        });
        rows(ui, |ui| {
            for (id, name) in STYLES.iter().skip(3) { if ui.selectable_label(spec.style == *id, tr(name)).clicked() { let id = id.to_string(); change(app, |s| s.style = id); } }
        });
    });
    group(ui, "Reset", |ui| {
        rows(ui, |ui| {
            if small(ui, "resetSlide", Some("Reset Graphic"), false, true, "Default colors and style").clicked() { change(app, |s| { s.colors = "colorful".into(); s.style = "subtle".into(); s.rtl = false; }); }
            let cv = small_menu(ui, "smartArtConvert", "Convert", "Convert to shapes or text");
            crate::theme::menu(&cv, |ui| {
                if ui.button(tr("Convert to Shapes")).clicked() { convert_to_shapes(app, &loc); ui.close(); }
                if ui.button(tr("Convert to Text")).clicked() { convert_to_text(app, &loc, &spec); ui.close(); }
            });
        });
    });
}

/// Keep the shapes as a plain group.
fn convert_to_shapes(app: &mut SlidesApp, loc: &Loc) {
    app.checkpoint("convert");
    if let Some(c) = app.pres.element_mut(loc).and_then(|e| e.path_mut("p:nvGrpSpPr/p:cNvPr")) { c.remove_attr("descr"); c.set_attr("name", "Group"); }
    app.changed();
}

/// The graphic's text as a bulleted text box in its place.
fn convert_to_text(app: &mut SlidesApp, loc: &Loc, spec: &Spec) {
    let Some(x) = app.pres.xfrm(loc) else { return };
    app.checkpoint("convert");
    app.pres.delete_shapes(std::slice::from_ref(loc));
    if let Some(t) = app.pres.insert_shape(&loc.part, "rect", &x, true) {
        app.pres.set_shape_text(&t, None, &spec.items.iter().map(|i| i.1.clone()).collect::<Vec<_>>().join("\n"));
        for (k, (lvl, _)) in spec.items.iter().enumerate() { if *lvl > 0 { app.pres.set_level(&t, None, Some((k, k)), *lvl as i32); } }
        app.selection = vec![t];
    }
    app.changed();
}
