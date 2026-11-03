//! Side panes beside the slide: Selection (names, visibility, order), Format Shape (fill,
//! line, effects, size, text box, alt text), Comments, Reuse Slides, and Designer (design
//! ideas made locally from the theme).

use crate::app::{Pane, SlidesApp};
use crate::i18n::tr;
use egui::{vec2, Color32, RichText, Sense, Stroke};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::format::Effect;
use slides_engine::scene::{Kind, Loc, Xfrm};
use slides_engine::Presentation;

fn hex(c: word_engine::Color) -> String { format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2) }

pub fn panel(app: &mut SlidesApp, ui: &mut egui::Ui) {
    if app.pane == Pane::None { return; }
    let panel = if crate::i18n::rtl() { egui::Panel::left("side-pane") } else { egui::Panel::right("side-pane") };
    panel.resizable(true).default_size(300.0).min_size(240.0).frame(egui::Frame::NONE.fill(c_card()).inner_margin(egui::Margin::symmetric(12, 10)).stroke(Stroke::new(1.0, c_line()))).show(ui, |ui| {
        let title = match app.pane { Pane::Animation => "Animation Pane", Pane::Selection => "Selection", Pane::Format => "Format Shape", Pane::Comments => "Comments", Pane::Reuse => "Reuse Slides", Pane::Designer => "Designer", Pane::Background => "Format Background", Pane::None => "" };
        crate::theme::hrow(ui, |ui| {
            ui.label(RichText::new(tr(title)).size(16.0).strong().color(c_ink()));
            ui.with_layout(far_layout(), |ui| if close_button(ui, "Close").clicked() { app.pane = Pane::None; });
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match app.pane {
            Pane::Animation => crate::animations_tab::pane(app, ui),
            Pane::Selection => selection(app, ui),
            Pane::Format => format(app, ui),
            Pane::Comments => comments(app, ui),
            Pane::Reuse => reuse(app, ui),
            Pane::Designer => designer(app, ui),
            Pane::Background => crate::design_tab::background_pane(app, ui),
            Pane::None => {}
        });
    });
}

fn set_hidden(p: &mut Presentation, l: &Loc, hidden: bool) {
    if let Some(c) = p.element_mut(l).and_then(|sp| sp.elements_mut().find(|e| e.local().starts_with("nv"))).and_then(|nv| nv.child_mut("p:cNvPr")) {
        if hidden { c.set_attr("hidden", "1"); } else { c.remove_attr("hidden"); }
    }
}

fn selection(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let Some(s) = app.slide() else { return };
    let n = app.pres.slide(&s).and_then(|r| r.path("p:cSld/p:spTree")).map(|t| t.elements().count()).unwrap_or(0);
    crate::theme::hrow(ui, |ui| {
        if ui.button(tr("Show All")).clicked() { app.checkpoint("visibility"); for k in 0..n { set_hidden(&mut app.pres, &Loc { part: s.clone(), path: vec![k] }, false); } }
        if ui.button(tr("Hide All")).clicked() { app.checkpoint("visibility"); for k in 0..n { let l = Loc { part: s.clone(), path: vec![k] }; if app.pres.element(&l).is_some_and(|e| !matches!(e.local(), "nvGrpSpPr" | "grpSpPr" | "extLst")) { set_hidden(&mut app.pres, &l, true); } } }
        if small(ui, "bringForward", None, false, !app.selection.is_empty(), "Bring Forward").clicked() { app.checkpoint("order"); app.selection = app.selection.clone().iter().filter_map(|l| app.pres.reorder(l, slides_engine::edit::Order::Forward)).collect(); }
        if small(ui, "sendBackward", None, false, !app.selection.is_empty(), "Send Backward").clicked() { app.checkpoint("order"); app.selection = app.selection.clone().iter().filter_map(|l| app.pres.reorder(l, slides_engine::edit::Order::Backward)).collect(); }
    });
    ui.add_space(6.0);
    // Topmost first, as PowerPoint lists them.
    for k in (0..n).rev() {
        let loc = Loc { part: s.clone(), path: vec![k] };
        let Some(el) = app.pres.element(&loc) else { continue };
        if matches!(el.local(), "nvGrpSpPr" | "grpSpPr" | "extLst") { continue; }
        let c = el.elements().find(|e| e.local().starts_with("nv")).and_then(|nv| nv.child("p:cNvPr"));
        let name = c.and_then(|c| c.attr("name")).unwrap_or("").to_string();
        let hidden = c.and_then(|c| c.attr_bool("hidden")).unwrap_or(false);
        let selected = app.selection.contains(&loc);
        let rename_id = egui::Id::new(("rename", k));
        let renaming: Option<String> = ui.data(|d| d.get_temp(rename_id));
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        if selected { ui.painter().rect_filled(row, 4.0, ACCENT.gamma_multiply(0.15)); } else if resp.hovered() { ui.painter().rect_filled(row, 4.0, c_hover()); }
        let eye = egui::Rect::from_min_size(egui::pos2(row.max.x - 26.0, row.min.y + 3.0), vec2(20.0, 20.0));
        let eye_r = ui.interact(eye, egui::Id::new(("eye", k)), Sense::click());
        ui.painter().text(eye.center(), egui::Align2::CENTER_CENTER, if hidden { "◌" } else { "👁" }, egui::FontId::proportional(13.0), if hidden { c_ink3() } else { c_ink2() });
        if eye_r.clicked() { app.checkpoint("visibility"); set_hidden(&mut app.pres, &loc, !hidden); }
        match renaming {
            Some(mut text) => {
                let r = ui.put(egui::Rect::from_min_max(row.min + vec2(6.0, 2.0), egui::pos2(eye.min.x - 4.0, row.max.y - 2.0)), egui::TextEdit::singleline(&mut text));
                if r.lost_focus() { app.checkpoint("rename"); app.pres.set_name(&loc, &text); ui.data_mut(|d| d.remove::<String>(rename_id)); } else { ui.data_mut(|d| d.insert_temp(rename_id, text)); r.request_focus(); }
            }
            None => { ui.painter().text(row.min + vec2(8.0, 13.0), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(13.0), if hidden { c_ink3() } else { c_ink() }); }
        }
        if resp.double_clicked() { ui.data_mut(|d| d.insert_temp(rename_id, name.clone())); }
        else if resp.clicked() {
            if ui.input(|i| i.modifiers.command || i.modifiers.shift) { if let Some(i) = app.selection.iter().position(|l| *l == loc) { app.selection.remove(i); } else { app.selection.push(loc.clone()); } }
            else { app.selection = vec![loc.clone()]; app.edit = None; }
        }
    }
}

const PATTERNS: [&str; 24] = ["pct5", "pct10", "pct20", "pct25", "pct50", "pct75", "ltHorz", "dkHorz", "ltVert", "dkVert", "ltDnDiag", "ltUpDiag", "dkDnDiag", "dkUpDiag", "smGrid", "lgGrid", "smCheck", "lgCheck", "dotGrid", "diagCross", "horzBrick", "weave", "wave", "zigZag"];

fn format(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let Some(scene) = app.scene() else { return };
    let Some(loc) = app.selection.first().cloned() else { ui.label(RichText::new(tr("Select a shape to format it.")).color(c_ink3())); return };
    let mut tab = app.format_tab;
    crate::theme::hrow(ui, |ui| for (k, icon, tip) in [(0u8, "shapeFill", "Fill & Line"), (1, "shapeEffects", "Effects"), (2, "size", "Size & Properties"), (3, "textBox", "Text Box")] {
        if small(ui, icon, None, tab == k, true, tip).clicked() { tab = k; }
    });
    app.format_tab = tab;
    ui.separator();
    let item = SlidesApp::items_of(&scene, &loc).into_iter().next().cloned();
    match tab {
        0 => {
            crate::theme::section(ui, "Fill");
            crate::theme::hrow(ui, |ui| {
                if ui.button(tr("No fill")).clicked() { app.shapes_cmd(|p, l| p.set_fill(l, Some(""))); }
                if ui.button(tr("Automatic")).clicked() { app.shapes_cmd(|p, l| p.set_fill(l, None)); }
            });
            let mut c = match item.as_ref().map(|i| &i.kind) { Some(Kind::Shape { fill: slides_engine::scene::Fill::Solid(c), .. }) => word_engine::Color(c.r, c.g, c.b), _ => word_engine::Color(0x44, 0x72, 0xC4) };
            crate::theme::hrow(ui, |ui| { ui.label(tr("Solid fill")); if color_button(ui, &mut c) { let h = hex(c); app.shapes_cmd(move |p, l| p.set_fill(l, Some(&h))); } });
            ui.label(tr("Gradient fill"));
            ui.horizontal_wrapped(|ui| for v in 0..8 {
                let (stops, angle, radial) = slides_engine::format::gradient_variation(&hex(c), v);
                let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                let a = stops.first().map(|s| s.1.clone()).unwrap_or_default();
                let b = stops.last().map(|s| s.1.clone()).unwrap_or_default();
                let col = |h: &str| { let c = slides_engine::Rgba::hex(h).unwrap_or(slides_engine::Rgba::BLACK); Color32::from_rgb(c.r, c.g, c.b) };
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(r.left_top(), col(&a)); mesh.colored_vertex(r.right_top(), col(&a)); mesh.colored_vertex(r.left_bottom(), col(&b)); mesh.colored_vertex(r.right_bottom(), col(&b));
                mesh.add_triangle(0, 1, 2); mesh.add_triangle(1, 3, 2);
                ui.painter().add(egui::Shape::mesh(mesh));
                if resp.clicked() { app.shapes_cmd(move |p, l| p.set_gradient_fill(l, &stops, angle, radial)); }
            });
            crate::theme::hrow(ui, |ui| {
                if ui.button(tr("Picture…")).clicked() {
                    if let Some(f) = rfd::FileDialog::new().add_filter(&tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() {
                        if let Ok(b) = std::fs::read(&f) { let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into()); app.shapes_cmd(move |p, l| p.set_picture_fill(l, b.clone(), &ext, false)); }
                    }
                }
                crate::theme::submenu(ui, tr("Texture"), |ui| for t in slides_engine::format::TEXTURES { if ui.button(tr(t)).clicked() { let png = slides_engine::format::texture_png(t); app.shapes_cmd(move |p, l| p.set_picture_fill(l, png.clone(), "png", true)); ui.close(); } });
            });
            ui.label(tr("Pattern fill"));
            ui.horizontal_wrapped(|ui| for pr in PATTERNS {
                let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                for y in 0..8u32 { for x in 0..8u32 { if slides_engine::render::pattern_bit(pr, x, y) { ui.painter().rect_filled(egui::Rect::from_min_size(r.min + vec2(x as f32 * 2.75, y as f32 * 2.75), vec2(2.75, 2.75)), 0.0, c_ink()); } } }
                ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, if resp.hovered() { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
                if resp.on_hover_text(pr).clicked() { let fg = hex(c); app.shapes_cmd(move |p, l| p.set_pattern_fill(l, pr, &fg, "FFFFFF")); }
            });
            crate::theme::section(ui, "Line");
            let mut lc = match item.as_ref().map(|i| &i.kind) { Some(Kind::Shape { line: Some(l), .. }) => word_engine::Color(l.color.r, l.color.g, l.color.b), _ => word_engine::Color(0x2F, 0x52, 0x8F) };
            let mut w = match item.as_ref().map(|i| &i.kind) { Some(Kind::Shape { line: Some(l), .. }) => l.width / 12_700.0, _ => 1.0 };
            crate::theme::hrow(ui, |ui| { if ui.button(tr("No line")).clicked() { app.shapes_cmd(|p, l| p.set_line(l, Some(""), None, None)); } ui.label(tr("Color")); if color_button(ui, &mut lc) { let h = hex(lc); app.shapes_cmd(move |p, l| p.set_line(l, Some(&h), None, None)); } });
            crate::theme::hrow(ui, |ui| { ui.label(tr("Width")); if ui.add(egui::DragValue::new(&mut w).range(0.0..=100.0).speed(0.25).suffix(" pt")).changed() { app.shapes_cmd(move |p, l| p.set_line(l, None, Some(w), None)); } });
            crate::theme::Combo::from_id_salt("fmt-dash").selected_text(tr("Dash type")).show_ui(ui, |ui| for (d, n) in [("solid", "Solid"), ("sysDot", "Round Dot"), ("sysDash", "Square Dot"), ("dash", "Dash"), ("dashDot", "Dash Dot"), ("lgDash", "Long Dash"), ("lgDashDot", "Long Dash Dot")] { if ui.selectable_label(false, tr(n)).clicked() { app.shapes_cmd(move |p, l| p.set_line(l, None, None, Some(d))); } });
        }
        1 => {
            crate::theme::section(ui, "Shadow");
            effect_presets(app, ui, &[("No Shadow", Effect::None), ("Outer Offset", Effect::Shadow { dir: 45.0, dist: 3.0, blur: 4.0, color: "000000".into(), alpha: 40 }), ("Outer Bottom", Effect::Shadow { dir: 90.0, dist: 4.0, blur: 6.0, color: "000000".into(), alpha: 40 }), ("Outer Centered", Effect::Shadow { dir: 0.0, dist: 0.0, blur: 10.0, color: "000000".into(), alpha: 45 }), ("Inner", Effect::InnerShadow { dir: 45.0, dist: 4.0, blur: 6.0 }), ("Perspective", Effect::Shadow { dir: 60.0, dist: 12.0, blur: 10.0, color: "000000".into(), alpha: 25 })]);
            crate::theme::section(ui, "Reflection");
            effect_presets(app, ui, &[("Tight", Effect::Reflection { size: 0.35, dist: 0.0 }), ("Half", Effect::Reflection { size: 0.55, dist: 2.0 }), ("Full", Effect::Reflection { size: 0.9, dist: 4.0 })]);
            crate::theme::section(ui, "Glow");
            let mut gc = word_engine::Color(0xFF, 0xC0, 0x00);
            let gid = egui::Id::new("glow-color");
            if let Some(c) = ui.data(|d| d.get_temp::<[u8; 3]>(gid)) { gc = word_engine::Color(c[0], c[1], c[2]); }
            crate::theme::hrow(ui, |ui| { ui.label(tr("Color")); if color_button(ui, &mut gc) { ui.data_mut(|d| d.insert_temp(gid, [gc.0, gc.1, gc.2])); } });
            effect_presets(app, ui, &[("5 pt", Effect::Glow { radius: 5.0, color: hex(gc) }), ("8 pt", Effect::Glow { radius: 8.0, color: hex(gc) }), ("11 pt", Effect::Glow { radius: 11.0, color: hex(gc) }), ("18 pt", Effect::Glow { radius: 18.0, color: hex(gc) })]);
            crate::theme::section(ui, "Soft Edges");
            effect_presets(app, ui, &[("1 pt", Effect::SoftEdge { radius: 1.0 }), ("2.5 pt", Effect::SoftEdge { radius: 2.5 }), ("5 pt", Effect::SoftEdge { radius: 5.0 }), ("10 pt", Effect::SoftEdge { radius: 10.0 }), ("25 pt", Effect::SoftEdge { radius: 25.0 })]);
            crate::three_d_ui::format(app, ui);
        }
        2 => {
            let Some(mut x) = app.frame_of(&scene, &loc) else { return };
            let orig = x;
            let lock_id = egui::Id::new("lock-aspect");
            let mut lock: bool = ui.data(|d| d.get_temp(lock_id)).unwrap_or(app.pres.element(&loc).is_some_and(|e| e.local() == "pic"));
            let inch = 914_400.0;
            crate::theme::section(ui, "Size");
            let (mut w, mut h) = (x.w / inch, x.h / inch);
            crate::theme::grid(ui, egui::Grid::new("fmt-size").num_columns(2), |ui| {
                ui.label(tr("Height")); let ch = ui.add(egui::DragValue::new(&mut h).range(0.01..=56.0).speed(0.01).max_decimals(2).suffix("\"")).changed(); ui.end_row();
                ui.label(tr("Width")); let cw = ui.add(egui::DragValue::new(&mut w).range(0.01..=56.0).speed(0.01).max_decimals(2).suffix("\"")).changed(); ui.end_row();
                if lock && ch { w = h * orig.w / orig.h.max(1.0); }
                if lock && cw { h = w * orig.h / orig.w.max(1.0); }
                ui.label(tr("Rotation")); ui.add(egui::DragValue::new(&mut x.rot).range(-360.0..=360.0).speed(1.0).suffix("°")); ui.end_row();
                ui.label(tr("Lock aspect ratio")); crate::theme::check(ui, &mut lock, ""); ui.end_row();
            });
            ui.data_mut(|d| d.insert_temp(lock_id, lock));
            x.w = w * inch; x.h = h * inch;
            crate::theme::section(ui, "Position");
            let (mut px, mut py) = (x.x / inch, x.y / inch);
            crate::theme::grid(ui, egui::Grid::new("fmt-pos").num_columns(2), |ui| {
                ui.label(tr("Horizontal position")); ui.add(egui::DragValue::new(&mut px).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
                ui.label(tr("Vertical position")); ui.add(egui::DragValue::new(&mut py).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
            });
            x.x = px * inch; x.y = py * inch;
            if x != orig { app.checkpoint("typing"); app.pres.set_xfrm(&loc, &x); }
            crate::theme::section(ui, "Alt Text");
            let alt_id = egui::Id::new(("alt", loc.path.clone()));
            let mut alt: String = ui.data(|d| d.get_temp(alt_id)).unwrap_or_else(|| app.pres.element(&loc).and_then(|e| e.elements().find(|c| c.local().starts_with("nv"))).and_then(|nv| nv.child("p:cNvPr")).and_then(|c| c.attr("descr")).unwrap_or("").to_string());
            if ui.add(egui::TextEdit::multiline(&mut alt).desired_rows(3).hint_text(tr("Describe this object for someone who is blind or has low vision"))).changed() { app.checkpoint("typing"); app.pres.set_alt_text(&loc, &alt); }
            ui.data_mut(|d| d.insert_temp(alt_id, alt));
        }
        _ => {
            let body = item.as_ref().and_then(|i| if let Kind::Shape { text: Some(t), .. } = &i.kind { Some(t.clone()) } else { None });
            let Some(body) = body else { ui.label(RichText::new(tr("This object has no text.")).color(c_ink3())); return };
            crate::theme::section(ui, "Text Box");
            crate::theme::grid(ui, egui::Grid::new("fmt-tb").num_columns(2), |ui| {
                ui.label(tr("Vertical alignment"));
                crate::theme::Combo::from_id_salt("fmt-va").selected_text(tr(match body.anchor { slides_engine::scene::Anchor::Top => "Top", slides_engine::scene::Anchor::Middle => "Middle", _ => "Bottom" })).show_ui(ui, |ui| for (a, n) in [("t", "Top"), ("ctr", "Middle"), ("b", "Bottom")] { if ui.selectable_label(false, tr(n)).clicked() { app.shapes_cmd(move |p, l| p.set_anchor(l, a)); } });
                ui.end_row();
                ui.label(tr("Text direction"));
                crate::theme::Combo::from_id_salt("fmt-dir").selected_text(tr(if body.rotation == 90.0 { "Rotate all text 90°" } else if body.rotation == 270.0 { "Rotate all text 270°" } else { "Horizontal" })).show_ui(ui, |ui| for (d, n) in TEXT_DIRS { if ui.selectable_label(false, tr(n)).clicked() { app.shapes_cmd(move |p, l| p.set_text_direction(l, d)); } });
                ui.end_row();
            });
            crate::theme::hrow(ui, |ui| for (k, n) in [("none", "Do not Autofit"), ("shrink", "Shrink text on overflow"), ("resize", "Resize shape to fit text")] {
                let on = match (k, body.autofit) { ("none", slides_engine::scene::Autofit::None) | ("shrink", slides_engine::scene::Autofit::Shrink { .. }) | ("resize", slides_engine::scene::Autofit::Grow) => true, _ => false };
                if crate::theme::radio(ui, on, tr(n)).clicked() { app.shapes_cmd(move |p, l| p.set_text_box(l, None, None, Some(k))); }
            });
            let mut ins = body.insets.map(|v| v / 12_700.0);
            crate::theme::grid(ui, egui::Grid::new("fmt-ins").num_columns(2), |ui| {
                let mut ch = false;
                for (k, n) in ["Left margin", "Top margin", "Right margin", "Bottom margin"].iter().enumerate() { ui.label(tr(n)); ch |= ui.add(egui::DragValue::new(&mut ins[k]).range(0.0..=200.0).speed(0.25).suffix(" pt")).changed(); ui.end_row(); }
                if ch { app.shapes_cmd(move |p, l| p.set_text_box(l, Some(ins), None, None)); }
            });
            let mut wrap = body.wrap;
            if crate::theme::check(ui, &mut wrap, tr("Wrap text in shape")).changed() { app.shapes_cmd(move |p, l| p.set_text_box(l, None, Some(wrap), None)); }
            if ui.button(tr("Columns…")).clicked() { app.dialog = Some(crate::dialogs::Dialog::Columns { count: body.columns.max(1), gap: body.column_gap / 12_700.0 }); }
        }
    }
}

pub const TEXT_DIRS: &[(&str, &str)] = &[("horz", "Horizontal"), ("vert", "Rotate all text 90°"), ("vert270", "Rotate all text 270°"), ("wordArtVert", "Stacked")];

fn effect_presets(app: &mut SlidesApp, ui: &mut egui::Ui, presets: &[(&str, Effect)]) {
    ui.horizontal_wrapped(|ui| for (n, e) in presets {
        if ui.button(tr(n)).clicked() { let e = e.clone(); app.shapes_cmd(move |p, l| p.set_effect(l, &e)); }
    });
}

fn comments(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let Some(s) = app.slide() else { return };
    let id = egui::Id::new("new-comment");
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).hint_text(tr("Add a comment")));
    let author = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "Author".into());
    crate::theme::hrow(ui, |ui| {
        if crate::theme::primary_enabled(ui, !text.trim().is_empty(), "Post").clicked() {
            let (x, y) = app.selection.first().and_then(|l| app.pres.xfrm(l)).map(|f| (f.x + f.w, f.y)).unwrap_or((0.0, 0.0));
            app.checkpoint("comment");
            app.pres.add_comment(&s, &author, text.trim(), x, y);
            text.clear();
        }
        if crate::theme::secondary(ui, "Cancel").clicked() { text.clear(); }
    });
    ui.data_mut(|d| d.insert_temp(id, text));
    ui.separator();
    let list = app.pres.comments(&s);
    if list.is_empty() { ui.label(RichText::new(tr("No comments on this slide.")).color(c_ink3())); }
    let mut delete = None;
    for (i, (who, body, date, _, _)) in list.iter().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            crate::theme::hrow(ui, |ui| { ui.label(RichText::new(who).strong()); ui.label(RichText::new(date.split('T').next().unwrap_or("")).size(11.0).color(c_ink3())); ui.with_layout(far_layout(), |ui| if small(ui, "deleteComment", None, false, true, "Delete Comment").clicked() { delete = Some(i); }); });
            ui.label(body);
        });
    }
    if let Some(i) = delete { app.checkpoint("comment"); app.pres.delete_comment(&s, i); }
}

/// A presentation opened in the Reuse Slides pane, with thumbnails.
pub struct Reuse { pub path: std::path::PathBuf, pub pres: Presentation, pub thumbs: Vec<Option<egui::TextureHandle>>, pub keep: bool }

fn reuse(app: &mut SlidesApp, ui: &mut egui::Ui) {
    if ui.button(tr("Browse…")).clicked() {
        if let Some(p) = rfd::FileDialog::new().add_filter(&tr("PowerPoint Presentation"), &["pptx", "potx", "ppsx"]).pick_file() {
            match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| Presentation::open(&b)) {
                Ok(pres) => { let n = pres.slides().len(); app.reuse = Some(Reuse { path: p, pres, thumbs: vec![None; n], keep: false }); }
                Err(e) => app.error = Some(e),
            }
        }
    }
    let Some(mut r) = app.reuse.take() else { ui.label(RichText::new(tr("Pick a presentation to insert its slides into this one.")).color(c_ink3())); return };
    ui.label(RichText::new(r.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).strong());
    crate::theme::check(ui, &mut r.keep, tr("Use source formatting"));
    let slides = r.pres.slides();
    let (sw, sh) = r.pres.size();
    let w = (ui.available_width() - 8.0).min(240.0);
    let h = w * (sh / sw) as f32;
    let mut insert = None;
    for (i, part) in slides.iter().enumerate() {
        if r.thumbs[i].is_none() {
            let scene = r.pres.scene(part);
            let mut images = slides_engine::render::Images::default();
            if let Some(pm) = slides_engine::render::render(&r.pres, &scene, &mut images, slides_engine::render::Options { scale: (w * ui.ctx().pixels_per_point()) as f64 / sw, prompts: false, skip_text_of: None, translate: None }) {
                r.thumbs[i] = Some(ui.ctx().load_texture(format!("reuse-{i}"), egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR));
            }
        }
        let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
        if let Some(t) = &r.thumbs[i] { ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE); }
        ui.painter().rect_stroke(rect, 0.0, Stroke::new(if resp.hovered() { 2.0 } else { 1.0 }, if resp.hovered() { ACCENT } else { c_line() }), egui::StrokeKind::Outside);
        ui.label(RichText::new(format!("{}. {}", i + 1, r.pres.slide_title(part))).size(11.5).color(c_ink2()));
        if resp.on_hover_text(tr("Click to insert this slide")).clicked() { insert = Some(i); }
        ui.add_space(6.0);
    }
    if ui.button(tr("Insert All")).clicked() { insert = Some(usize::MAX); }
    if let Some(i) = insert {
        app.checkpoint("reuse");
        let mut at = if app.slides().is_empty() { 0 } else { app.current + 1 };
        let range: Vec<usize> = if i == usize::MAX { (0..slides.len()).collect() } else { vec![i] };
        for k in range { if app.pres.reuse_slide(&r.pres, k, at, r.keep).is_some() { at += 1; } }
        app.go_to(at.saturating_sub(1));
    }
    app.reuse = Some(r);
}

/// Design ideas: variations of the current slide built from the theme's colors.
fn designer(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let Some(s) = app.slide() else { return };
    ui.label(RichText::new(tr("Pick a design for this slide.")).color(c_ink3()));
    let key = (s.clone(), app.revision);
    let id = egui::Id::new("design-ideas");
    let cached: Option<((String, u64), Vec<egui::TextureHandle>)> = ui.data(|d| d.get_temp(id));
    let thumbs = match cached.filter(|c| c.0 == key) {
        Some((_, t)) => t,
        None => {
            let (sw, _) = app.pres.size();
            let w = 240.0;
            let ideas: Vec<Presentation> = (0..6).map(|k| design_variant(&app.pres, &s, k, app)).collect();
            let t: Vec<egui::TextureHandle> = ideas.iter().enumerate().filter_map(|(k, p)| {
                let scene = p.scene(&s);
                let mut images = slides_engine::render::Images::default();
                let pm = slides_engine::render::render(p, &scene, &mut images, slides_engine::render::Options { scale: (w * ui.ctx().pixels_per_point()) as f64 / sw, prompts: false, skip_text_of: None, translate: None })?;
                Some(ui.ctx().load_texture(format!("idea-{k}"), egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR))
            }).collect();
            ui.data_mut(|d| d.insert_temp(id, (key, t.clone())));
            t
        }
    };
    let (sw, sh) = app.pres.size();
    let w = (ui.available_width() - 8.0).min(240.0);
    for (k, t) in thumbs.iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(vec2(w, w * (sh / sw) as f32), Sense::click());
        ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        ui.painter().rect_stroke(rect, 0.0, Stroke::new(if resp.hovered() { 2.5 } else { 1.0 }, if resp.hovered() { ACCENT } else { c_line() }), egui::StrokeKind::Outside);
        if resp.clicked() { let idea = design_variant(&app.pres, &s, k, app); app.checkpoint("design"); app.pres = idea; }
        ui.add_space(8.0);
    }
}

/// One design idea: background, accent shapes behind the content, and text colors.
pub fn design_variant(pres: &Presentation, slide: &str, k: usize, app: &SlidesApp) -> Presentation {
    let mut p = pres.clone();
    let (sw, sh) = p.size();
    let a1 = app.accent_hex(1);
    let a2 = app.accent_hex(2);
    let a5 = app.accent_hex(5);
    let dark = |h: &str| { let c = slides_engine::Rgba::hex(h).unwrap_or(slides_engine::Rgba::BLACK); slides_engine::Rgba::rgb((c.r as f64 * 0.35) as u8, (c.g as f64 * 0.35) as u8, (c.b as f64 * 0.35) as u8).to_hex() };
    let tintc = |h: &str, t: f64| { let c = slides_engine::Rgba::hex(h).unwrap_or(slides_engine::Rgba::BLACK); let m = |v: u8| ((v as f64) + (255.0 - v as f64) * t) as u8; slides_engine::Rgba::rgb(m(c.r), m(c.g), m(c.b)).to_hex() };
    let n = p.slide(slide).and_then(|r| r.path("p:cSld/p:spTree")).map(|t| t.elements().count()).unwrap_or(0);
    let texts: Vec<Loc> = (0..n).map(|i| Loc { part: slide.to_string(), path: vec![i] }).filter(|l| p.element(l).is_some_and(|e| e.local() == "sp")).collect();
    let color_text = |p: &mut Presentation, c: &str| for l in &texts { p.format_shape_text(l, None, &slides_engine::edit::RunFormat { color: Some(c.to_string()), ..Default::default() }); };
    let to_back = |p: &mut Presentation, l: Loc| { let mut cur = l; for _ in 0..200 { match p.reorder(&cur, slides_engine::edit::Order::Back) { Some(n) if n != cur => cur = n, _ => break } } };
    let bar = |p: &mut Presentation, x: Xfrm, fill: &str| { if let Some(l) = p.insert_shape(slide, "rect", &x, false) { p.set_fill(&l, Some(fill)); p.set_line(&l, Some(""), None, None); p.set_name(&l, "Design Accent"); to_back(p, l); } };
    match k {
        0 => { p.set_background(slide, Some(&dark(&a1))); color_text(&mut p, "FFFFFF"); bar(&mut p, Xfrm { x: 0.0, y: sh * 0.94, w: sw, h: sh * 0.06, ..Default::default() }, &a2); }
        1 => { p.set_background(slide, Some(&tintc(&a1, 0.88))); bar(&mut p, Xfrm { x: 0.0, y: 0.0, w: sw * 0.035, h: sh, ..Default::default() }, &a1); }
        2 => { p.set_background(slide, Some("FFFFFF")); bar(&mut p, Xfrm { x: 0.0, y: 0.0, w: sw, h: sh * 0.22, ..Default::default() }, &a1); if let Some(t) = texts.first() { p.format_shape_text(t, None, &slides_engine::edit::RunFormat { color: Some("FFFFFF".into()), ..Default::default() }); } }
        3 => { p.set_background(slide, Some(&dark(&a5))); color_text(&mut p, "FFFFFF"); bar(&mut p, Xfrm { x: sw * 0.62, y: 0.0, w: sw * 0.38, h: sh, ..Default::default() }, &a5); }
        4 => { p.set_background(slide, Some(&tintc(&a2, 0.9))); bar(&mut p, Xfrm { x: sw * 0.06, y: sh * 0.06, w: sw * 0.88, h: sh * 0.88, ..Default::default() }, "FFFFFF"); }
        _ => { p.set_background(slide, Some("F2F2F2")); bar(&mut p, Xfrm { x: 0.0, y: sh * 0.8, w: sw, h: sh * 0.2, ..Default::default() }, &dark(&a1)); bar(&mut p, Xfrm { x: 0.0, y: sh * 0.78, w: sw, h: sh * 0.02, ..Default::default() }, &a2); }
    }
    p
}
