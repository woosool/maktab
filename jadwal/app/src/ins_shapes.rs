//! Insert ▸ Shapes and WordArt: Excel's preset gallery (Sard's preset geometry), click-then-drag
//! drawing on the grid, the presets drawn with their real geometry and adjust values (yellow
//! handles change them), and WordArt text boxes with text effects.

use crate::app::SheetApp;
use crate::grid::Layout;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::menu_heading;
use sheet_engine::drawing::{ObjKind, Object, Shape};
use sheet_engine::shape_geom;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Excel's shape gallery, by group.
pub const GALLERY: &[(&str, &[&str])] = &[
    ("Lines", &["line", "straightConnector1", "bentConnector3", "curvedConnector3", "arc"]),
    ("Rectangles", &["rect", "roundRect", "snip1Rect", "snip2SameRect", "snip2DiagRect", "snipRoundRect", "round1Rect", "round2SameRect", "round2DiagRect"]),
    ("Basic Shapes", &["textBox", "ellipse", "triangle", "rtTriangle", "parallelogram", "trapezoid", "diamond", "pentagon", "hexagon", "heptagon", "octagon", "decagon", "dodecagon", "pie", "chord", "teardrop", "frame", "halfFrame", "corner", "diagStripe", "plus", "plaque", "can", "cube", "bevel", "donut", "noSmoking", "blockArc", "foldedCorner", "smileyFace", "heart", "lightningBolt", "sun", "moon", "cloud", "bracketPair", "bracePair", "leftBracket", "rightBracket", "leftBrace", "rightBrace"]),
    ("Block Arrows", &["rightArrow", "leftArrow", "upArrow", "downArrow", "leftRightArrow", "upDownArrow", "quadArrow", "leftRightUpArrow", "bentArrow", "uturnArrow", "leftUpArrow", "bentUpArrow", "curvedRightArrow", "curvedLeftArrow", "curvedUpArrow", "curvedDownArrow", "stripedRightArrow", "notchedRightArrow", "homePlate", "chevron", "rightArrowCallout", "downArrowCallout", "leftArrowCallout", "upArrowCallout", "leftRightArrowCallout", "quadArrowCallout", "circularArrow"]),
    ("Equation Shapes", &["mathPlus", "mathMinus", "mathMultiply", "mathDivide", "mathEqual", "mathNotEqual"]),
    ("Flowchart", &["flowChartProcess", "flowChartAlternateProcess", "flowChartDecision", "flowChartInputOutput", "flowChartPredefinedProcess", "flowChartInternalStorage", "flowChartDocument", "flowChartMultidocument", "flowChartTerminator", "flowChartPreparation", "flowChartManualInput", "flowChartManualOperation", "flowChartConnector", "flowChartOffpageConnector", "flowChartPunchedCard", "flowChartPunchedTape", "flowChartSummingJunction", "flowChartOr", "flowChartCollate", "flowChartSort", "flowChartExtract", "flowChartMerge", "flowChartOnlineStorage", "flowChartDelay", "flowChartMagneticTape", "flowChartMagneticDisk", "flowChartMagneticDrum", "flowChartDisplay"]),
    ("Stars and Banners", &["irregularSeal1", "irregularSeal2", "star4", "star5", "star6", "star7", "star8", "star10", "star12", "star16", "star24", "star32", "ribbon2", "ribbon", "ellipseRibbon2", "ellipseRibbon", "verticalScroll", "horizontalScroll", "wave", "doubleWave"]),
    ("Callouts", &["wedgeRectCallout", "wedgeRoundRectCallout", "wedgeEllipseCallout", "cloudCallout", "borderCallout1", "borderCallout2", "borderCallout3", "accentCallout1", "accentCallout2", "accentCallout3", "callout1", "callout2", "callout3", "accentBorderCallout1", "accentBorderCallout2", "accentBorderCallout3"]),
];

/// Pending "draw this shape" mode, kept in egui memory.
fn pid() -> Id { Id::new("jadwal-draw-shape") }
pub fn pending(ctx: &egui::Context) -> Option<String> { ctx.data(|d| d.get_temp::<String>(pid())) }
pub fn set_pending(ctx: &egui::Context, p: Option<&str>) { ctx.data_mut(|d| match p { Some(p) => { d.insert_temp(pid(), p.to_string()); } None => { d.remove::<String>(pid()); } }); }

/// A readable name for a preset ("flowChartDecision" → "Flowchart: Decision").
pub fn label(p: &str) -> String {
    if p == "textBox" { return tr("Text Box"); }
    slides_engine::edit::preset_label(p)
}

/// The Shapes gallery menu: picking one arms drawing (click-then-drag on the grid).
pub fn gallery(app: &mut SheetApp, ui: &mut Ui) {
    ui.set_width(330.0);
    egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
        for (group, names) in GALLERY {
            menu_heading(ui, group);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                for n in *names {
                    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                    if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
                    if *n == "textBox" {
                        ui.painter().rect_stroke(r.shrink(6.0), 0.0, Stroke::new(1.1, c_ink2()), StrokeKind::Inside);
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "A", egui::FontId::proportional(11.0), c_ink2());
                    } else {
                        let g = slides_engine::geom::preset(n, 18.0, 18.0, &[]);
                        for p in &g.paths {
                            for poly in slides_engine::geom::flatten(&p.cmds, 6) {
                                let pts: Vec<Pos2> = poly.iter().map(|q| pos2(r.min.x + 5.0 + q.x as f32, r.min.y + 5.0 + q.y as f32)).collect();
                                if pts.len() > 1 { ui.painter().add(egui::Shape::line(pts, Stroke::new(1.1, c_ink2()))); }
                            }
                        }
                    }
                    if resp.on_hover_text(label(n)).clicked() {
                        set_pending(ui.ctx(), Some(n));
                        app.notify(&tr("Drag on the sheet to draw the shape (Esc cancels)."));
                        ui.close();
                    }
                }
            });
        }
    });
}

/// A new shape of a preset with the workbook's accent look.
pub fn new_shape(app: &SheetApp, preset: &str) -> Shape {
    if preset == "textBox" { return Shape { preset: "rect".into(), text_box: true, size: 11.0, fill: Some([255, 255, 255]), line: Some([0xBF, 0xBF, 0xBF]), ..Default::default() }; }
    let accent = app.book.theme_colors.get(4).copied().unwrap_or([0x44, 0x72, 0xC4]);
    let line_only = preset == "line" || preset.contains("Connector") || preset == "arc" || preset.contains("racket") || preset.contains("race");
    Shape { preset: preset.into(), size: 11.0, fill: (!line_only).then_some(accent), line: Some(accent.map(|x| (x as f32 * 0.7) as u8)), color: Some([255, 255, 255]), center: true, adjust: shape_geom::default_adjusts(preset), ..Default::default() }
}

/// Inserts a shape over sheet rectangle r (sheet pixels) and selects it.
pub fn insert_at(app: &mut SheetApp, lay: &Layout, preset: &str, r: [f32; 4]) -> usize {
    let text_box = preset == "textBox";
    let mut o = Object::new(ObjKind::Shape(new_shape(app, preset)), crate::objects::anchor_at_cell(app, 10.0, 10.0));
    o.anchor.set_rect(r, |x| lay.cols.at(x), |c| lay.cols.pos(c), |y| lay.rows.at(y), |r| lay.rows.pos(r));
    let i = crate::objects::insert(app, o, if text_box { "TextBox" } else { "Shape" }, if text_box { "Insert Text Box" } else { "Insert Shape" });
    if text_box { crate::objects::open_dialog(app, crate::objects::ObjDialog::EditText { sheet: app.sheet(), idx: i, text: String::new() }); }
    i
}

#[derive(Clone, Copy)]
struct DrawStart(Pos2);

/// The grid's drawing mode: while a shape is armed, a drag on the grid draws it (a click puts
/// it at its default size). Called first by the objects layer; true while it owns the mouse.
pub fn draw_hook(app: &mut SheetApp, ui: &mut Ui, painter: &egui::Painter, lay: &Layout) -> bool {
    let Some(preset) = pending(ui.ctx()) else { return false };
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) { set_pending(ui.ctx(), None); return false; }
    let view = lay.view();
    let resp = ui.interact(view, Id::new("jadwal-draw-shape-area"), Sense::click_and_drag());
    if resp.hovered() { ui.ctx().set_cursor_icon(CursorIcon::Crosshair); }
    let sid = Id::new("jadwal-draw-shape-start");
    if resp.drag_started() { if let Some(p) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) { ui.ctx().data_mut(|d| d.insert_temp(sid, DrawStart(p))); } }
    let start = ui.ctx().data(|d| d.get_temp::<DrawStart>(sid)).map(|s| s.0);
    let now = ui.input(|i| i.pointer.interact_pos());
    if let (Some(a), Some(b)) = (start, now) {
        if resp.dragged() {
            let mut r = Rect::from_two_pos(a, b);
            if ui.input(|i| i.modifiers.shift) { let s = r.width().max(r.height()); r = Rect::from_min_size(r.min, vec2(s, s)); }
            let pv = painter.with_clip_rect(view);
            let mut s = new_shape(app, &preset);
            s.text.clear();
            paint_geom(&app.ctx, &pv, r, &s, lay.z);
            pv.rect_stroke(r, 0.0, Stroke::new(1.0, crate::grid::SELECT), StrokeKind::Outside);
        }
    }
    let done_drag = resp.drag_stopped();
    if done_drag || resp.clicked() {
        let (a, b) = if done_drag { (start.unwrap_or_default(), now.unwrap_or_default()) } else { let p = resp.interact_pointer_pos().unwrap_or(view.center()); (p, p) };
        ui.ctx().data_mut(|d| d.remove::<DrawStart>(sid));
        set_pending(ui.ctx(), None);
        let (x0, y0) = crate::objects::to_sheet(lay, a);
        let (x1, y1) = crate::objects::to_sheet(lay, b);
        let mut r = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
        if done_drag && ui.input(|i| i.modifiers.shift) { let s = (r[2] - r[0]).max(r[3] - r[1]); r[2] = r[0] + s; r[3] = r[1] + s; }
        // A click (or a tiny drag) gives the default size.
        if r[2] - r[0] < 4.0 && r[3] - r[1] < 4.0 {
            let (w, h): (f32, f32) = if preset == "textBox" { (190.0, 60.0) } else if preset == "line" || preset.contains("Connector") { (150.0, 0.0) } else { (120.0, 80.0) };
            r = [r[0], r[1], r[0] + w, r[1] + h.max(1.0)];
        }
        insert_at(app, lay, &preset, r);
    }
    true
}

// ---- painting ------------------------------------------------------------------------------------

thread_local! { static GEOM: RefCell<HashMap<u64, TextureHandle>> = RefCell::new(HashMap::new()); }

/// Draws a preset shape (not its text) with its real geometry; false for presets the library
/// doesn't know (the caller draws those its own way).
pub fn paint_geom(ctx: &egui::Context, p: &egui::Painter, r: Rect, s: &Shape, z: f32) -> bool {
    let name = if s.preset.is_empty() { "rect" } else { s.preset.as_str() };
    if !shape_geom::known(name) || !r.is_finite() { return false; }
    let line_only = s.fill.is_none() && s.line.is_none();
    let stroke = s.line.or(if line_only && (name == "line" || name.contains("Connector")) { Some([0, 0, 0]) } else { None });
    let sw = z.max(0.75);
    let pad = sw * 2.0 + 1.0;
    let ppp = ctx.pixels_per_point();
    let full = r.expand(pad);
    let (pw, ph) = ((full.width() * ppp).ceil().clamp(1.0, 4096.0) as u32, (full.height() * ppp).ceil().clamp(1.0, 4096.0) as u32);
    let k = pw as f32 / full.width().max(0.01);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (name, s.fill, stroke, pw, ph, (sw * 100.0) as i32, (r.width() * 10.0) as i32, (r.height() * 10.0) as i32).hash(&mut h);
    for (n, v) in &s.adjust { n.hash(&mut h); v.to_bits().hash(&mut h); }
    let key = h.finish();
    let tex = GEOM.with(|g| {
        let mut g = g.borrow_mut();
        if let Some(t) = g.get(&key) { return Some(t.clone()); }
        if g.len() > 400 { g.clear(); }
        let mut pm = resvg::tiny_skia::Pixmap::new(pw, ph)?;
        let rr = [pad * k, pad * k, (pad + r.width()) * k, (pad + r.height()) * k];
        for pc in shape_geom::pieces(s, rr) {
            let mut pb = resvg::tiny_skia::PathBuilder::new();
            for (i, q) in pc.points.iter().enumerate() { if i == 0 { pb.move_to(q.0, q.1); } else { pb.line_to(q.0, q.1); } }
            if pc.closed { pb.close(); }
            let Some(path) = pb.finish() else { continue };
            if let (true, Some(f)) = (pc.fill, s.fill) {
                let c = shape_geom::shaded(f, pc.shade);
                let mut paint = resvg::tiny_skia::Paint::default();
                paint.set_color_rgba8(c[0], c[1], c[2], 255);
                paint.anti_alias = true;
                pm.fill_path(&path, &paint, resvg::tiny_skia::FillRule::EvenOdd, resvg::tiny_skia::Transform::identity(), None);
            }
            if let (true, Some(c)) = (pc.stroke, stroke) {
                let mut paint = resvg::tiny_skia::Paint::default();
                paint.set_color_rgba8(c[0], c[1], c[2], 255);
                paint.anti_alias = true;
                let st = resvg::tiny_skia::Stroke { width: sw * k, line_join: resvg::tiny_skia::LineJoin::Round, ..Default::default() };
                pm.stroke_path(&path, &paint, &st, resvg::tiny_skia::Transform::identity(), None);
            }
        }
        let img = egui::ColorImage::from_rgba_premultiplied([pw as usize, ph as usize], pm.data());
        let t = ctx.load_texture(format!("shape-geom-{key:x}"), img, egui::TextureOptions::LINEAR);
        g.insert(key, t.clone());
        Some(t)
    });
    if let Some(t) = tex { p.image(t.id(), full, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
    true
}

/// Where a shape's text goes on screen (the preset's text rectangle).
pub fn text_rect(s: &Shape, r: Rect) -> Rect {
    if !shape_geom::known(&s.preset) { return r; }
    let t = shape_geom::text_rect(s, [r.min.x, r.min.y, r.max.x, r.max.y]);
    let t = Rect::from_min_max(pos2(t[0], t[1]), pos2(t[2], t[3]));
    if t.is_finite() && t.width() > 4.0 && t.height() > 4.0 { t } else { r }
}

/// WordArt effects drawn under the text: glow, shadow, outline.
pub fn text_effects(p: &egui::Painter, s: &Shape, g: &std::sync::Arc<egui::Galley>, at: Pos2, z: f32) {
    if let Some(c) = s.glow {
        let col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 110);
        for (dx, dy) in [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0), (-1.5, -1.5), (1.5, 1.5), (-1.5, 1.5), (1.5, -1.5)] { p.galley_with_override_text_color(at + vec2(dx * z, dy * z), g.clone(), col); }
    }
    if s.shadow { p.galley_with_override_text_color(at + vec2(2.0 * z, 2.0 * z), g.clone(), Color32::from_black_alpha(90)); }
    if let Some(c) = s.text_outline {
        let col = Color32::from_rgb(c[0], c[1], c[2]);
        for (dx, dy) in [(-0.8, 0.0), (0.8, 0.0), (0.0, -0.8), (0.0, 0.8)] { p.galley_with_override_text_color(at + vec2(dx * z, dy * z), g.clone(), col); }
    }
}

// ---- adjust handles --------------------------------------------------------------------------------

#[derive(Clone)]
struct AdjDrag { handle: usize, start: Vec<(String, f64)> }

/// The selected shape's yellow adjust handles; dragging one changes its adjust value.
pub fn adjust_handles(app: &mut SheetApp, ui: &mut Ui, p: &egui::Painter, si: usize, i: usize, r: Rect) {
    let Some(ObjKind::Shape(s)) = app.book.sheets[si].objects.get(i).map(|o| o.kind.clone()) else { return };
    if !shape_geom::known(&s.preset) || r.width() < 8.0 || r.height() < 8.0 { return; }
    let rr = [r.min.x, r.min.y, r.max.x, r.max.y];
    let hs = shape_geom::handles(&s, rr);
    let did = Id::new(("jadwal-adjust", si, i));
    for (k, h) in hs.iter().enumerate() {
        let c = pos2(h.at.x as f32, h.at.y as f32);
        if !c.is_finite() { continue; }
        let d = [c + vec2(0.0, -5.0), c + vec2(5.0, 0.0), c + vec2(0.0, 5.0), c + vec2(-5.0, 0.0)];
        p.add(egui::Shape::convex_polygon(d.to_vec(), Color32::from_rgb(0xFF, 0xD7, 0x00), Stroke::new(1.0, Color32::from_rgb(0x80, 0x60, 0x00))));
        let resp = ui.interact(Rect::from_center_size(c, vec2(11.0, 11.0)), Id::new(("jadwal-adjust-h", si, i, k)), Sense::drag());
        if resp.hovered() || resp.dragged() { ui.ctx().set_cursor_icon(CursorIcon::PointingHand); }
        if resp.drag_started() { app.checkpoint("Adjust Shape"); ui.ctx().data_mut(|dd| dd.insert_temp(did, AdjDrag { handle: k, start: s.adjust.clone() })); }
        if resp.dragged() {
            let Some(ad) = ui.ctx().data(|dd| dd.get_temp::<AdjDrag>(did)) else { continue };
            let Some(pt) = resp.interact_pointer_pos() else { continue };
            let mut cur = Shape { adjust: ad.start.clone(), ..s.clone() };
            if cur.adjust.is_empty() { cur.adjust = shape_geom::default_adjusts(&s.preset); }
            // Each axis's guide: the value whose handle lands nearest the pointer (searched).
            for axis in 0..2 {
                let g = if axis == 0 { hs[ad.handle].x.clone() } else { hs[ad.handle].y.clone() };
                let Some((name, lo, hi)) = g else { continue };
                let (lo, hi) = (lo.min(hi), lo.max(hi));
                let mut best = (f32::MAX, lo);
                for step in 0..=60 {
                    let v = lo + (hi - lo) * step as f64 / 60.0;
                    let mut t = cur.clone();
                    set_adj(&mut t.adjust, &name, v);
                    let Some(hh) = shape_geom::handles(&t, rr).get(ad.handle).cloned() else { continue };
                    let dist = if axis == 0 { (hh.at.x as f32 - pt.x).abs() } else { (hh.at.y as f32 - pt.y).abs() };
                    if dist < best.0 { best = (dist, v); }
                }
                set_adj(&mut cur.adjust, &name, best.1);
            }
            let o = &mut app.book.sheets[si].objects[i];
            if let ObjKind::Shape(sh) = &mut o.kind { sh.adjust = cur.adjust; }
            o.touch();
            app.dirty = true;
        }
    }
}

fn set_adj(a: &mut Vec<(String, f64)>, name: &str, v: f64) {
    match a.iter_mut().find(|x| x.0 == name) { Some(x) => x.1 = v, None => a.push((name.to_string(), v)) }
}

// ---- WordArt -----------------------------------------------------------------------------------------

/// WordArt styles: (name, fill, outline, shadow, glow).
pub const WORDART: &[(&str, [u8; 3], Option<[u8; 3]>, bool, Option<[u8; 3]>)] = &[
    ("Fill: Black, Text color 1; Shadow", [0x00, 0x00, 0x00], None, true, None),
    ("Fill: Blue, Accent color 1; Shadow", [0x44, 0x72, 0xC4], None, true, None),
    ("Fill: Orange, Accent color 2; Outline: Orange", [0xED, 0x7D, 0x31], Some([0xC5, 0x5A, 0x11]), false, None),
    ("Fill: White; Outline: Blue, Accent color 1; Glow: Blue", [0xFF, 0xFF, 0xFF], Some([0x44, 0x72, 0xC4]), false, Some([0x8F, 0xAA, 0xDC])),
    ("Fill: Gold, Accent color 4; Soft Bevel", [0xFF, 0xC0, 0x00], Some([0xBF, 0x90, 0x00]), true, None),
    ("Fill: Gray; Outline: Black", [0xA5, 0xA5, 0xA5], Some([0x40, 0x40, 0x40]), false, None),
    ("Fill: Green, Accent color 6; Glow", [0x70, 0xAD, 0x47], None, false, Some([0xC5, 0xE0, 0xB4])),
    ("Fill: White; Outline: Black; Shadow", [0xFF, 0xFF, 0xFF], Some([0x00, 0x00, 0x00]), true, None),
    ("Pattern Fill: Blue; Glow: Gold", [0x2F, 0x55, 0x97], None, false, Some([0xFF, 0xE6, 0x99])),
];

/// The WordArt gallery: a style makes a text box with "Your text here".
pub fn wordart_menu(app: &mut SheetApp, ui: &mut Ui) {
    ui.set_width(250.0);
    ui.horizontal_wrapped(|ui| {
        for (k, (name, fill, outline, shadow, glow)) in WORDART.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(76.0, 56.0), Sense::click());
            ui.painter().rect(r, 3.0, if resp.hovered() { c_hover() } else { c_card() }, Stroke::new(1.0, c_line()), StrokeKind::Inside);
            let s = Shape { text: "A".into(), size: 28.0, bold: true, color: Some(*fill), text_outline: *outline, shadow: *shadow, glow: *glow, ..Default::default() };
            let g = ui.painter().layout_no_wrap("A".into(), egui::FontId::new(34.0, egui::FontFamily::Name("ui-bold".into())), rgb(*fill));
            let at = r.center() - g.size() / 2.0;
            text_effects(ui.painter(), &s, &g, at, 1.4);
            ui.painter().galley(at, g, rgb(*fill));
            if resp.on_hover_text(tr(name)).clicked() { insert_wordart(app, k, &tr("Your text here")); ui.close(); }
        }
    });
}

fn rgb(c: [u8; 3]) -> Color32 { Color32::from_rgb(c[0], c[1], c[2]) }

/// Inserts a WordArt text box of style k.
pub fn insert_wordart(app: &mut SheetApp, k: usize, text: &str) -> usize {
    let (_, fill, outline, shadow, glow) = WORDART[k.min(WORDART.len() - 1)];
    let s = Shape { text: text.into(), preset: "rect".into(), text_box: true, size: 36.0, bold: true, center: true, color: Some(fill), text_outline: outline, shadow, glow, ..Default::default() };
    let o = Object::new(ObjKind::Shape(s), crate::objects::anchor_at_cell(app, 420.0, 80.0));
    let i = crate::objects::insert(app, o, "TextBox", "Insert WordArt");
    crate::objects::open_dialog(app, crate::objects::ObjDialog::EditText { sheet: app.sheet(), idx: i, text: text.into() });
    i
}

/// Text effects for the Shape Format tab (shadow, glow, outline on/off).
pub fn effects_menu(app: &mut SheetApp, ui: &mut Ui, si: usize, i: usize) {
    let Some(ObjKind::Shape(s)) = app.book.sheets[si].objects.get(i).map(|o| o.kind.clone()) else { return };
    let mut set = |app: &mut SheetApp, f: &dyn Fn(&mut Shape)| { app.checkpoint("Text Effects"); let o = &mut app.book.sheets[si].objects[i]; if let ObjKind::Shape(sh) = &mut o.kind { f(sh); } o.touch(); app.dirty = true; };
    if ui.selectable_label(s.shadow, tr("Shadow")).clicked() { set(app, &|x| x.shadow = !x.shadow); ui.close(); }
    if ui.selectable_label(s.glow.is_some(), tr("Glow")).clicked() { set(app, &|x| x.glow = if x.glow.is_some() { None } else { Some([0x8F, 0xAA, 0xDC]) }); ui.close(); }
    if ui.selectable_label(s.text_outline.is_some(), tr("Text Outline")).clicked() { set(app, &|x| x.text_outline = if x.text_outline.is_some() { None } else { Some([0x40, 0x40, 0x40]) }); ui.close(); }
    ui.separator();
    menu_heading(ui, "WordArt Styles");
    for (k, (name, ..)) in WORDART.iter().enumerate() {
        if ui.button(tr(name)).clicked() { let (_, f, o, sh, g) = WORDART[k]; set(app, &move |x| { x.color = Some(f); x.text_outline = o; x.shadow = sh; x.glow = g; }); ui.close(); }
    }
}
