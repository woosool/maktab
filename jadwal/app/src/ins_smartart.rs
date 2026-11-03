//! Insert ▸ SmartArt: Sard's diagram layouts (List, Process, Cycle, Hierarchy, Relationship,
//! Matrix, Pyramid) with a text pane, color schemes and styles, inserted as a group of shapes on the
//! sheet so Excel (and every other reader) shows them.

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke, StrokeKind};
use qalam_kit::colors::*;
use sheet_engine::drawing::{ObjKind, Object, Shape};
use slides_engine::diagram::{self, Paint, Palette, Spec, CATEGORIES, COLORS, LAYOUTS, STYLES};
use slides_engine::scene::Xfrm;

const EMU: f64 = 9525.0;

#[derive(Clone)]
pub struct Editor { pub spec: Spec, pub text: String, pub cat: Option<usize> }

impl Editor {
    pub fn new() -> Editor { let text = "Plan\nBuild\nTest\nLaunch".to_string(); Editor { spec: Spec::new("basicProcess", &text), text, cat: None } }
}

fn hex(c: [u8; 3]) -> String { format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }
fn unhex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() < 6 { return None; }
    Some([u8::from_str_radix(&s[0..2], 16).ok()?, u8::from_str_radix(&s[2..4], 16).ok()?, u8::from_str_radix(&s[4..6], 16).ok()?])
}

/// The workbook theme as a diagram palette.
pub fn palette(app: &SheetApp) -> Palette {
    let t = &app.book.theme_colors;
    let g = |i: usize, d: [u8; 3]| hex(t.get(i).copied().unwrap_or(d));
    Palette { accents: (4..10).map(|i| g(i, [0x44, 0x72, 0xC4])).collect(), dk1: g(1, [0, 0, 0]), lt1: g(0, [255, 255, 255]), dk2: g(3, [0x44, 0x54, 0x6A]) }
}

/// A diagram's shapes in a w × h pixel frame at (x, y): (shape, sheet rectangle).
pub fn shapes(spec: &Spec, pal: &Palette, x: f32, y: f32, w: f32, h: f32) -> Vec<(Shape, [f32; 4])> {
    let fr = Xfrm { x: 0.0, y: 0.0, w: w as f64 * EMU, h: h as f64 * EMU, ..Default::default() };
    let mut nodes = diagram::layout(&spec.layout, &spec.items, &fr);
    diagram::style(&mut nodes, &spec.colors, &spec.style, pal);
    if spec.rtl { diagram::mirror(&mut nodes, &fr); }
    diagram::fit_text(&mut nodes);
    nodes.iter().map(|n| {
        let r = [x + (n.x.x / EMU) as f32, y + (n.x.y / EMU) as f32, x + ((n.x.x + n.x.w) / EMU) as f32, y + ((n.x.y + n.x.h) / EMU) as f32];
        let fill = match &n.fill { Paint::None => None, Paint::Solid(c) => unhex(c), Paint::Gradient(a, _) => unhex(a) };
        let s = Shape {
            text: n.text.clone(), preset: if n.label { "rect".into() } else { n.preset.clone() }, text_box: n.label,
            fill: if n.label { None } else { fill }, line: if n.label { None } else { n.line.as_ref().and_then(|l| unhex(&l.0)) },
            color: unhex(&n.text_color), size: (n.size as f32).clamp(6.0, 60.0), bold: n.bold, center: n.align == "ctr",
            adjust: n.adjust.iter().map(|(k, v)| (k.to_string(), *v)).collect(), shadow: false, ..Default::default()
        };
        (s, r)
    }).collect()
}

/// Inserts the diagram at the active cell: one shape per box, named "SmartArt N - k".
pub fn insert(app: &mut SheetApp, spec: &Spec) -> usize {
    let pal = palette(app);
    let (w, h) = (480.0, 270.0);
    let a = crate::objects::anchor_at_cell(app, w, h);
    let s = &app.book.sheets[app.sheet()];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    let r0 = a.rect(|c| cols.pos(c), |r| rows.pos(r));
    let list = shapes(spec, &pal, r0[0], r0[1], w, h);
    app.commit(None);
    app.checkpoint("Insert SmartArt");
    let si = app.sheet();
    let tag = format!("jadwal-smartart:{}", spec.layout);
    // One group (as Excel inserts a graphic): selecting a box selects the whole diagram,
    // double-click edits a box, Ungroup gets at the shapes.
    let mut items = vec![];
    let base = app.book.sheets[si].objects.iter().map(|o| o.id).max().unwrap_or(1) + 1;
    for (k, (sh, r)) in list.into_iter().enumerate() {
        let mut o = Object::new(ObjKind::Shape(sh), a.clone());
        o.descr = tag.clone();
        o.id = base + 1 + k as u32;
        o.name = format!("SmartArt Shape {}", k + 1);
        items.push((o, r));
    }
    let (g, bb) = sheet_engine::group::make(items);
    let mut o = Object::new(ObjKind::Group(g), a.clone());
    o.anchor.set_rect(bb, |x| cols.at(x), |c| cols.pos(c), |y| rows.at(y), |r| rows.pos(r));
    o.descr = tag;
    let last = sheet_engine::drawing::add_object(&mut app.book.sheets[si].objects, o, "SmartArt");
    app.dirty = true;
    crate::objects::select(app, Some(last));
    last
}

thread_local! { static PREVIEW: std::cell::RefCell<Option<(String, egui::TextureHandle)>> = const { std::cell::RefCell::new(None) }; }

fn preview(ctx: &egui::Context, spec: &Spec, pal: &Palette) -> Option<egui::TextureHandle> {
    let key = format!("{}{}", spec.to_string(), pal.accents.join(""));
    if let Some(t) = PREVIEW.with(|p| p.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, t)| t.clone())) { return Some(t); }
    let png = sheet_engine::embed::shapes_png(&shapes(spec, pal, 0.0, 0.0, 360.0, 203.0), 360.0, 203.0)?;
    let img = image::load_from_memory(&png).ok()?.to_rgba8();
    let t = ctx.load_texture("smartart-preview", egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR);
    PREVIEW.with(|p| *p.borrow_mut() = Some((key, t.clone())));
    Some(t)
}

/// The Choose a SmartArt Graphic dialog; false when it closes.
pub fn dialog(app: &mut SheetApp, ctx: &egui::Context, ed: &mut Editor) -> bool {
    let mut open = true;
    let mut done = false;
    let pal = palette(app);
    crate::theme::dialog(ctx, "Choose a SmartArt Graphic").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(110.0);
                if ui.selectable_label(ed.cat.is_none(), tr("All")).clicked() { ed.cat = None; }
                for (k, (_, n)) in CATEGORIES.iter().enumerate() { if ui.selectable_label(ed.cat == Some(k), tr(n)).clicked() { ed.cat = Some(k); } }
            });
            ui.vertical(|ui| {
                ui.set_width(250.0);
                egui::ScrollArea::vertical().id_salt("sa-layouts").max_height(420.0).show(ui, |ui| {
                    for (id, name, cat) in LAYOUTS.iter().filter(|l| ed.cat.is_none_or(|k| CATEGORIES[k].0 == l.2)) {
                        let _ = cat;
                        if ui.selectable_label(ed.spec.layout == *id, tr(name)).clicked() { ed.spec.layout = id.to_string(); }
                    }
                });
            });
            ui.vertical(|ui| {
                ui.set_width(360.0);
                ui.label(RichText::new(LAYOUTS.iter().find(|l| l.0 == ed.spec.layout).map(|l| tr(l.1)).unwrap_or_default()).strong().size(15.0));
                let spec = Spec { items: diagram::parse_items(&ed.text), ..ed.spec.clone() };
                let (r, _) = ui.allocate_exact_size(vec2(360.0, 203.0), Sense::hover());
                ui.painter().rect_filled(r, 4.0, Color32::WHITE);
                if let Some(t) = preview(ui.ctx(), &spec, &pal) { ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
                ui.painter().rect_stroke(r, 4.0, Stroke::new(1.0, c_line()), StrokeKind::Inside);
                ui.label(RichText::new(tr("Type your text here (Tab for a lower level, Shift+Tab for higher):")).size(11.5).color(c_ink3()));
                ui.add(egui::TextEdit::multiline(&mut ed.text).desired_rows(6).desired_width(360.0).lock_focus(true));
                ui.horizontal(|ui| {
                    crate::theme::Combo::from_id_salt("sa-colors").width(170.0).selected_text(tr(COLORS.iter().find(|c| c.0 == ed.spec.colors).map(|c| c.1).unwrap_or("Colors"))).show_ui(ui, |ui| for (id, n) in COLORS { if ui.selectable_label(ed.spec.colors == *id, tr(n)).clicked() { ed.spec.colors = id.to_string(); } });
                    crate::theme::Combo::from_id_salt("sa-style").width(150.0).selected_text(tr(STYLES.iter().find(|c| c.0 == ed.spec.style).map(|c| c.1).unwrap_or("Style"))).show_ui(ui, |ui| for (id, n) in STYLES { if ui.selectable_label(ed.spec.style == *id, tr(n)).clicked() { ed.spec.style = id.to_string(); } });
                });
            });
        });
        done = crate::theme::footer(ui, |ui| crate::theme::primary_enabled(ui, !diagram::parse_items(&ed.text).is_empty(), "OK").clicked());
    });
    if done {
        ed.spec.items = diagram::parse_items(&ed.text);
        insert(app, &ed.spec);
        return false;
    }
    open
}
