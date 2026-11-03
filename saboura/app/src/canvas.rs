//! The board canvas: drawing objects (`render.js`) and the pointer and keyboard on the board
//! (the editor in `main.js`): select, move, resize, rotate, marquee, create, draw, connect
//! (with quick-connect ports and draggable line ends), pan and zoom.

use crate::app::{BoardApp, Drag, Panel, Tool};
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use qalam_kit::colors::ACCENT;
use saboura_engine::geometry::{self, Box2, P};
use saboura_engine::model::{self, DETAIL_ZOOM, Item};
use saboura_engine::{View, edit};
use std::collections::HashMap;

const INK: Color32 = Color32::from_rgb(0x30, 0x34, 0x2f);
const INK_DARK: Color32 = Color32::from_rgb(0xe8, 0xeb, 0xe6);
pub const SHAPE_LINE: Color32 = Color32::from_rgb(0x87, 0x8f, 0x86);
const FRAME_LINE: Color32 = Color32::from_rgb(0xa2, 0xaa, 0xa0);
const LINK: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xd6);
/// How far quick-connect ports sit outside an object, and the rotation handle from its corner (screen px).
const PORT_GAP: f32 = 22.0;
const ROTATE_GAP: f32 = 18.0;

pub fn hex(s: Option<&str>, fallback: Color32) -> Color32 {
    s.and_then(model::parse_hex).map(|[r, g, b]| Color32::from_rgb(r, g, b)).unwrap_or(fallback)
}

/// Converts between board points and screen points of the canvas.
#[derive(Clone, Copy)]
pub struct Xf { pub v: View, pub origin: Pos2 }

impl Xf {
    pub fn s(&self, p: P) -> Pos2 { let q = self.v.to_screen(p); self.origin + vec2(q.0 as f32, q.1 as f32) }
    pub fn b(&self, s: Pos2) -> P { self.v.to_board(((s.x - self.origin.x) as f64, (s.y - self.origin.y) as f64)) }
    pub fn z(&self) -> f32 { self.v.z as f32 }
    /// A point in an object's own frame, turned with it, on screen.
    pub fn obj(&self, i: &Item, p: P) -> Pos2 { self.s(geometry::world_point(i, (i.x + p.0, i.y + p.1))) }
}

pub fn canvas(app: &mut BoardApp, ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    app.canvas_rect = rect;
    if app.fit_pending && rect.width() > 0.0 { app.fit_pending = false; app.fit(); }
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let dark = app.prefs.dark;
    painter.rect_filled(rect, 0.0, board_bg(dark));
    if app.prefs.grid { draw_grid(app.view, &painter, rect, dark); }

    handle_input(app, ui, &response, rect.min);

    let xf = Xf { v: app.view, origin: rect.min };
    // Pictures are decoded before drawing.
    let images: Vec<String> = app.board.items.iter().filter(|i| i.kind == "image").map(|i| i.id.clone()).collect();
    let textures: HashMap<String, egui::TextureId> = images.into_iter().filter_map(|id| app.texture_of(&id).map(|t| (id, t.id()))).collect();
    let visible = Box2::from_points(xf.b(rect.min), xf.b(rect.max));
    let pad = 60.0 / app.view.z;
    let visible = Box2 { x: visible.x - pad, y: visible.y - pad, w: visible.w + pad * 2.0, h: visible.h + pad * 2.0 };
    draw_mind_lines(&painter, &app.board.items, &xf);
    let tags = tag_map(&app.board);
    for i in geometry::stacked(&app.board.items) {
        if !geometry::object_bounds(i).intersects(&visible) { continue; }
        let hide = app.editing.as_deref() == Some(&i.id);
        draw_item(&painter, i, &xf, dark, hide, textures.get(&i.id).copied(), app.cell.filter(|_| hide));
        draw_tags(&painter, i, &xf, &tags);
        if i.flag("mindCollapsed") {
            let n = saboura_engine::arrange::collapsed_hidden(std::slice::from_ref(i)).len();
            let count = app.board.items.iter().filter(|c| c.mind_parent() == Some(i.id.as_str())).count().max(n);
            let c = xf.s((i.x + i.w + 12.0, i.y + i.h / 2.0));
            painter.circle(c, 9.0, ACCENT, Stroke::new(1.5, Color32::WHITE));
            painter.text(c, egui::Align2::CENTER_CENTER, format!("{count}"), FontId::proportional(10.0), Color32::WHITE);
        }
    }
    if let Some((q, _)) = &app.search && !q.trim().is_empty() {
        for id in crate::panels::search_matches(app, q) {
            if let Some(i) = app.board.find(&id) {
                let corners = [(0.0, 0.0), (i.w, 0.0), (i.w, i.h), (0.0, i.h)].map(|p| xf.obj(i, p));
                painter.add(Shape::closed_line(corners.to_vec(), Stroke::new(3.0, Color32::from_rgb(0xf5, 0x9e, 0x0b))));
            }
        }
    }
    for g in &app.guides {
        let (a, b) = if g.vertical { ((g.at, g.from), (g.at, g.to)) } else { ((g.from, g.at), (g.to, g.at)) };
        painter.line_segment([xf.s(a), xf.s(b)], Stroke::new(1.0, Color32::from_rgb(0xe1, 0x1d, 0x74)));
    }
    draw_comment_pins(app, &painter, &xf);
    draw_votes(app, &painter, &xf);
    overlays(app, ui, &painter, &xf, &response);
    text_editor(app, ui, &xf);
}

pub fn board_bg(dark: bool) -> Color32 { if dark { Color32::from_rgb(0x16, 0x17, 0x17) } else { Color32::from_rgb(0xf7, 0xf7, 0xf4) } }

fn draw_grid(v: View, painter: &egui::Painter, rect: Rect, dark: bool) {
    let mut step = 24.0 * v.z;
    while step < 14.0 { step *= 5.0; }
    let dot = if dark { Color32::from_gray(0x33) } else { Color32::from_gray(0xd4) };
    let (ox, oy) = (v.x.rem_euclid(step), v.y.rem_euclid(step));
    let mut x = ox;
    while x < rect.width() as f64 {
        let mut y = oy;
        while y < rect.height() as f64 {
            painter.rect_filled(Rect::from_center_size(rect.min + vec2(x as f32, y as f32), vec2(1.5, 1.5)), 0.0, dot);
            y += step;
        }
        x += step;
    }
}

// ---- Drawing ----

/// Fills a simple polygon (convex or not) by ear clipping.
pub fn fill_poly(painter: &egui::Painter, pts: &[Pos2], color: Color32) {
    let n = pts.len();
    if n < 3 || color.a() == 0 { return; }
    let area: f32 = (0..n).map(|k| { let (a, b) = (pts[k], pts[(k + 1) % n]); a.x * b.y - b.x * a.y }).sum();
    let mut idx: Vec<usize> = if area >= 0.0 { (0..n).collect() } else { (0..n).rev().collect() };
    let mut mesh = egui::Mesh::default();
    for p in pts { mesh.colored_vertex(*p, color); }
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            if cross(a, b, c) <= 0.0 { continue; }
            let inside = idx.iter().any(|&j| j != ia && j != ib && j != ic && { let p = pts[j]; cross(a, b, p) > 0.0 && cross(b, c, p) > 0.0 && cross(c, a, p) > 0.0 });
            if inside { continue; }
            mesh.add_triangle(ia as u32, ib as u32, ic as u32);
            idx.remove(k);
            clipped = true;
            break;
        }
        if !clipped { break; }
    }
    if idx.len() == 3 { mesh.add_triangle(idx[0] as u32, idx[1] as u32, idx[2] as u32); }
    else { for k in 1..idx.len().saturating_sub(1) { mesh.add_triangle(idx[0] as u32, idx[k] as u32, idx[k + 1] as u32); } }
    painter.add(Shape::mesh(mesh));
}

/// A stroked path, solid, dashed or dotted.
fn stroke_path(painter: &egui::Painter, pts: &[Pos2], closed: bool, stroke: Stroke, style: &str, z: f32) {
    if pts.len() < 2 || stroke.width <= 0.0 { return; }
    let mut pts = pts.to_vec();
    if closed { pts.push(pts[0]); }
    match style {
        "dashed" => painter.extend(Shape::dashed_line(&pts, stroke, (8.0 * z).max(3.0), (6.0 * z).max(2.0))),
        "dotted" => painter.extend(Shape::dotted_line(&pts, stroke.color, (stroke.width * 2.5).max(3.0), stroke.width / 2.0 + 0.3)),
        _ => { painter.add(Shape::line(pts, stroke)); }
    }
}

pub fn draw_item(painter: &egui::Painter, i: &Item, xf: &Xf, dark: bool, hide_text: bool, texture: Option<egui::TextureId>, cell: Option<(usize, usize)>) {
    let z = xf.z();
    let fill = hex(i.color.as_deref(), Color32::from_rgb(0xf6, 0xe7, 0xa4));
    let to = |pts: Vec<P>| -> Vec<Pos2> { pts.into_iter().map(|p| xf.obj(i, p)).collect() };
    match i.kind.as_str() {
        "pen" => {
            let Some(pts) = &i.points else { return };
            let s: Vec<Pos2> = pts.iter().map(|p| xf.obj(i, (p[0], p[1]))).collect();
            let width = (i.line_width.unwrap_or(3.0) as f32 * z).max(0.5);
            if s.len() == 1 { painter.circle_filled(s[0], width / 2.0, fill); } else { painter.add(Shape::line(s, Stroke::new(width, fill))); }
            return;
        }
        "arrow" => { draw_connector(painter, i, xf, dark, hide_text); return; }
        "note" => {
            if i.rotation() == 0.0 {
                let r = Rect::from_two_pos(xf.s((i.x, i.y)), xf.s((i.x + i.w, i.y + i.h)));
                let shadow = egui::epaint::Shadow { offset: [0, (3.0 * z).min(6.0) as i8], blur: (10.0 * z).min(18.0) as u8, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 38 }) };
                painter.add(shadow.as_shape(r, (2.0 * z) as u8));
            }
            fill_poly(painter, &to(geometry::round_rect(i.w, i.h, 2.0)), fill);
        }
        "rect" | "ellipse" => {
            let s = to(geometry::item_outline(i));
            fill_poly(painter, &s, fill);
            let width = i.line_width.unwrap_or(1.0) as f32 * z;
            let line = hex(i.str("borderColor"), SHAPE_LINE);
            let style = i.str("lineStyle").unwrap_or("solid");
            stroke_path(painter, &s, true, Stroke::new(width.max(0.5), line), style, z);
            let key = i.shape.as_deref().unwrap_or("");
            for d in geometry::shape_details(key, i.w, i.h) { stroke_path(painter, &to(d), false, Stroke::new(width.max(0.5), line), "solid", z); }
        }
        "frame" => {
            let s = to(geometry::round_rect(i.w, i.h, 6.0));
            fill_poly(painter, &s, if dark { Color32::from_rgb(0x1e, 0x1f, 0x1f) } else { fill.gamma_multiply(0.45) });
            stroke_path(painter, &s, true, Stroke::new(1.0, if dark { Color32::from_gray(0x3a) } else { FRAME_LINE }), if dark { "solid" } else { "dashed" }, 1.0);
            // The frame's name sits above its top-left corner, as in the JS app.
            if !hide_text && !i.text.is_empty() {
                let at = xf.s((i.x, i.y)) + vec2(0.0, -6.0);
                let g = painter.layout_no_wrap(crate::i18n::shape(&i.text), FontId::new(13.0, FontFamily::Name("ui-bold".into())), if dark { Color32::from_gray(0xb8) } else { Color32::from_gray(0x55) });
                painter.galley(at - vec2(0.0, g.size().y), g, Color32::PLACEHOLDER);
            }
            return;
        }
        "image" => {
            let r = Rect::from_two_pos(xf.s((i.x, i.y)), xf.s((i.x + i.w, i.y + i.h)));
            let o = crate::app::ImageOptions::of(i);
            let [cx, cy, cw, ch] = o.crop.map(|v| v as f32);
            match texture {
                Some(t) => {
                    // A crop shows part of the picture; an ellipse mask a fan of triangles.
                    let mut mesh = egui::Mesh::with_texture(t);
                    let pts: Vec<(f64, f64)> = if o.ellipse { geometry::shape_outline("ellipse", i.w, i.h) } else { vec![(0.0, 0.0), (i.w, 0.0), (i.w, i.h), (0.0, i.h)] };
                    let vert = |p: (f64, f64)| egui::epaint::Vertex { pos: xf.obj(i, p), uv: pos2(cx + cw * (p.0 / i.w) as f32, cy + ch * (p.1 / i.h) as f32), color: Color32::WHITE };
                    mesh.vertices.push(vert((i.w / 2.0, i.h / 2.0)));
                    for p in &pts { mesh.vertices.push(vert(*p)); }
                    let n = pts.len() as u32;
                    for k in 0..n { mesh.add_triangle(0, 1 + k, 1 + (k + 1) % n); }
                    painter.add(Shape::mesh(mesh));
                }
                None => { painter.rect(r, 0.0, Color32::from_gray(0xdd), Stroke::new(1.0, SHAPE_LINE), StrokeKind::Inside); }
            }
            return;
        }
        "table" => { draw_table(painter, i, xf, hide_text, cell); return; }
        "text" => {}
        // Objects this port does not draw yet keep their place as a labelled plate.
        other => {
            let s = to(geometry::round_rect(i.w, i.h, 6.0));
            fill_poly(painter, &s, if dark { Color32::from_gray(0x33) } else { Color32::from_gray(0xee) });
            painter.add(Shape::closed_line(s, Stroke::new(1.0, SHAPE_LINE)));
            if xf.v.z >= DETAIL_ZOOM { painter.text(xf.s(geometry::center(i)), egui::Align2::CENTER_CENTER, crate::i18n::tr(other), FontId::proportional(12.0), SHAPE_LINE); }
            return;
        }
    }
    if !hide_text && xf.v.z >= DETAIL_ZOOM && !i.text.is_empty() { draw_text(painter, i, xf, dark); }
}

/// Lines from mind-map nodes to their branches, curved like the JS map.
fn draw_mind_lines(painter: &egui::Painter, items: &[Item], xf: &Xf) {
    let by_id: HashMap<&str, &Item> = items.iter().map(|i| (i.id.as_str(), i)).collect();
    let hidden = saboura_engine::arrange::collapsed_hidden(items);
    for i in items.iter().filter(|i| !hidden.contains(&i.id)) {
        let Some(p) = i.mind_parent().and_then(|p| by_id.get(p)) else { continue };
        let right = i.x + i.w / 2.0 >= p.x + p.w / 2.0;
        let a = if right { (p.x + p.w, p.y + p.h / 2.0) } else { (p.x, p.y + p.h / 2.0) };
        let b = if right { (i.x, i.y + i.h / 2.0) } else { (i.x + i.w, i.y + i.h / 2.0) };
        let mid = (a.0 + b.0) / 2.0;
        let c = hex(i.color.as_deref(), SHAPE_LINE);
        let line = Color32::from_rgb(c.r().saturating_sub(70), c.g().saturating_sub(70), c.b().saturating_sub(70));
        painter.add(Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
            [xf.s(a), xf.s((mid, a.1)), xf.s((mid, b.1)), xf.s(b)], false, Color32::TRANSPARENT, Stroke::new((2.0 * xf.z()).max(1.0), line))));
    }
}

pub fn text_color(i: &Item, dark: bool) -> Color32 {
    let theme_ink = if dark { INK_DARK } else { INK };
    let default = if matches!(i.kind.as_str(), "text" | "frame" | "arrow") { theme_ink } else { INK };
    if i.str("link").is_some() && i.str("textColor").is_none() { return LINK; }
    hex(i.str("textColor"), default)
}

/// The face for an object's text: its family (`fontFamily`), weight and slant.
pub fn font_for(i: &Item, size: f32) -> FontId {
    let (bold, italic) = (i.is_bold(), i.flag("italic"));
    let face = match i.str("fontFamily") {
        Some("eb-garamond") => 16 + bold as usize + 2 * italic as usize,
        Some("roboto-mono") => 20 + bold as usize + 2 * italic as usize,
        _ => italic as usize + 2 * bold as usize,
    };
    FontId::new(size, FontFamily::Name(qalam_kit::fonts::face_name(face).into()))
}

/// Text with list markers from `textBlocks` (bullets and numbers) in front of its paragraphs.
pub fn display_text(i: &Item) -> String {
    let Some(blocks) = i.get("textBlocks").and_then(|b| b.as_array()) else { return i.text.clone() };
    let starts: HashMap<u64, (String, u64)> = blocks.iter().filter_map(|b| Some((b["start"].as_u64()?, (b["list"].as_str()?.to_string(), b["indent"].as_u64().unwrap_or(0))))).collect();
    let mut out = String::new();
    let (mut at, mut number) = (0u64, 0);
    for (k, para) in i.text.split('\n').enumerate() {
        if k > 0 { out.push('\n'); }
        match starts.get(&at) {
            Some((list, indent)) => {
                out.push_str(&"    ".repeat(*indent as usize));
                if list == "number" { number += 1; out.push_str(&format!("{number}. ")); } else { number = 0; out.push_str("• "); }
            }
            None => number = 0,
        }
        out.push_str(para);
        at += para.encode_utf16().count() as u64 + 1;
    }
    out
}

/// Where an object's text goes, in its own frame.
pub fn text_box(i: &Item) -> Box2 {
    match i.kind.as_str() {
        "text" => Box2 { x: 4.0, y: 4.0, w: (i.w - 8.0).max(20.0), h: (i.h - 8.0).max(10.0) },
        "ellipse" => Box2 { x: i.w * 0.15, y: i.h * 0.15, w: i.w * 0.7, h: i.h * 0.7 },
        "rect" if matches!(i.shape.as_deref(), Some("diamond" | "triangle" | "star" | "or" | "summing-junction")) => Box2 { x: i.w * 0.22, y: i.h * 0.25, w: i.w * 0.56, h: i.h * 0.5 },
        "rect" if i.shape.as_deref() == Some("cylinder") => Box2 { x: 10.0, y: i.h * 0.24, w: i.w - 20.0, h: i.h * 0.64 },
        _ => Box2 { x: 14.0, y: 12.0, w: (i.w - 28.0).max(20.0), h: (i.h - 24.0).max(10.0) },
    }
}

pub fn galley_for(painter: &egui::Painter, i: &Item, text: &str, size: f32, color: Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut fmt = egui::TextFormat::simple(font_for(i, size), color);
    let line = Stroke::new((size / 14.0).max(1.0), color);
    if i.flag("underline") || i.str("link").is_some() { fmt.underline = line; }
    if i.flag("strike") { fmt.strikethrough = line; }
    let halign = match align_of(i, text) { "center" => egui::Align::Center, "right" => egui::Align::RIGHT, _ => egui::Align::LEFT };
    let runs = saboura_engine::rich::read(i.get("textRuns"));
    if !runs.is_empty() && text == i.text && !text.chars().any(is_rtl) {
        let mut job = rich_job(i, text, &runs, size, color);
        job.wrap.max_width = width;
        job.halign = halign;
        return painter.layout_job(job);
    }
    if !text.chars().any(is_rtl) {
        let mut job = egui::text::LayoutJob::single_section(text.to_string(), fmt);
        job.wrap.max_width = width;
        job.halign = halign;
        return painter.layout_job(job);
    }
    // Right-to-left text wraps word by word in reading order; each line is then shaped and
    // put in visual order on its own, so the first words stay on the first line.
    let measure = |t: &str| painter.layout_no_wrap(crate::i18n::shape(t), FontId::proportional(fmt.font_id.size), color).size().x;
    let mut lines: Vec<String> = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && measure(&candidate) > width { lines.push(std::mem::take(&mut line)); line = word.to_string(); } else { line = candidate; }
        }
        lines.push(line);
    }
    let lines: Vec<String> = lines.iter().map(|l| crate::i18n::shape(l)).collect();
    // Arabic and Hebrew lines are drawn with the interface font's script fallbacks.
    let mut fmt = fmt;
    fmt.font_id = FontId::proportional(fmt.font_id.size);
    let mut job = egui::text::LayoutJob::single_section(lines.join("\n"), fmt);
    job.halign = halign;
    painter.layout_job(job)
}

pub fn is_rtl(c: char) -> bool { ('\u{0590}'..='\u{08FF}').contains(&c) || ('\u{FB1D}'..='\u{FEFF}').contains(&c) }

/// The alignment of an object's text: its own, or the reading direction's start.
pub fn align_of<'a>(i: &'a Item, text: &str) -> &'a str {
    if i.text_align.is_none() && matches!(i.kind.as_str(), "note" | "text" | "frame" | "table") && text.chars().find(|c| c.is_alphabetic()).is_some_and(is_rtl) { "right" } else { i.align() }
}

fn draw_text(painter: &egui::Painter, i: &Item, xf: &Xf, dark: bool) {
    let z = xf.z();
    let b = text_box(i);
    let (w, h) = (b.w as f32 * z, b.h as f32 * z);
    let color = text_color(i, dark);
    let text = display_text(i);
    let mut size = i.font_size() as f32;
    let mut galley = galley_for(painter, i, &text, size * z, color, w);
    // A note or shape fits its text by shrinking it (autoFontSize).
    if i.auto_font_size == Some(true) {
        while (galley.size().y > h || galley.size().x > w + 1.0) && size > 8.0 {
            size = (size * 0.9).max(8.0);
            galley = galley_for(painter, i, &text, size * z, color, w);
        }
    }
    let centered_v = matches!(i.kind.as_str(), "note" | "rect" | "ellipse");
    let top = b.y + if centered_v { ((h - galley.size().y) / 2.0).max(0.0) as f64 / xf.v.z } else { 0.0 };
    let left = match align_of(i, &text) { "center" => b.x + b.w / 2.0, "right" => b.x + b.w, _ => b.x };
    let pos = xf.obj(i, (left, top));
    let clip = Rect::from_two_pos(xf.s((i.x, i.y)), xf.s((i.x + i.w, i.y + i.h))).expand(2.0);
    let painter = if i.rotation() == 0.0 && i.kind != "text" { painter.with_clip_rect(clip.intersect(painter.clip_rect())) } else { painter.clone() };
    // A highlight colour behind each line of text.
    if let Some(hl) = i.str("highlight").and_then(|h| model::parse_hex(h)) && i.rotation() == 0.0 {
        for row in &galley.rows {
            let r = row.rect().translate(pos.to_vec2());
            if r.width() > 1.0 { painter.rect_filled(r.expand2(vec2(2.0, 0.0)), 2.0, Color32::from_rgb(hl[0], hl[1], hl[2])); }
        }
    }
    painter.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(i.rotation().to_radians() as f32));
}

fn draw_table(painter: &egui::Painter, i: &Item, xf: &Xf, editing: bool, cell: Option<(usize, usize)>) {
    let z = xf.z();
    let cells = i.cells().unwrap_or_default();
    let (cols, rows) = edit::table_layout(i);
    let r = Rect::from_two_pos(xf.s((i.x, i.y)), xf.s((i.x + i.w, i.y + i.h)));
    painter.rect_filled(r, 0.0, hex(i.color.as_deref(), Color32::WHITE));
    // The first row is a header, shaded.
    if rows.len() > 2 { painter.rect_filled(Rect::from_min_max(r.min, pos2(r.max.x, xf.s((i.x, i.y + rows[1])).y)), 0.0, Color32::from_black_alpha(14)); }
    let line = Stroke::new(1.0, Color32::from_gray(0xbf));
    for x in &cols { painter.line_segment([xf.s((i.x + x, i.y)), xf.s((i.x + x, i.y + i.h))], line); }
    for y in &rows { painter.line_segment([xf.s((i.x, i.y + y)), xf.s((i.x + i.w, i.y + y))], line); }
    if xf.v.z < DETAIL_ZOOM { return; }
    let size = i.font_size.unwrap_or(15.0) as f32 * z;
    for (ri, row) in cells.iter().enumerate() {
        for (ci, text) in row.iter().enumerate() {
            if text.is_empty() || editing && cell == Some((ri, ci)) || ci + 1 >= cols.len() || ri + 1 >= rows.len() { continue; }
            let cb = Rect::from_min_max(xf.s((i.x + cols[ci], i.y + rows[ri])), xf.s((i.x + cols[ci + 1], i.y + rows[ri + 1])));
            let mut item = i.clone();
            if ri == 0 { item.bold = Some(true); }
            let g = galley_for(painter, &item, text, size, INK, (cb.width() - 12.0 * z).max(10.0));
            let x = match i.align() { "center" => cb.center().x, "right" => cb.max.x - 6.0 * z, _ => cb.min.x + 6.0 * z };
            painter.with_clip_rect(cb.intersect(painter.clip_rect())).galley(pos2(x, cb.center().y - g.size().y / 2.0), g, Color32::PLACEHOLDER);
        }
    }
}

/// The table cell at a board point.
pub fn cell_at(i: &Item, p: P) -> Option<(usize, usize)> {
    let (cols, rows) = edit::table_layout(i);
    let (x, y) = (p.0 - i.x, p.1 - i.y);
    let c = cols.windows(2).position(|w| x >= w[0] && x < w[1])?;
    let r = rows.windows(2).position(|w| y >= w[0] && y < w[1])?;
    Some((r, c))
}

/// A connection line's screen polyline; elbow corners are rounded.
fn connector_screen(i: &Item, xf: &Xf) -> Vec<Pos2> {
    let pts: Vec<Pos2> = geometry::connector_segments(i).into_iter().map(|p| xf.s(p)).collect();
    if i.line_type.as_deref() != Some("elbow") || pts.len() != 4 { return pts; }
    let mut out = vec![pts[0]];
    for k in 1..3 {
        let (a, b, c) = (pts[k - 1], pts[k], pts[k + 1]);
        let r = (10.0 * xf.z()).min((b - a).length() / 2.0).min((c - b).length() / 2.0);
        if r < 0.5 { out.push(b); continue; }
        let p0 = b - (b - a).normalized() * r;
        let p1 = b + (c - b).normalized() * r;
        for t in 0..=6 { let t = t as f32 / 6.0; let u = 1.0 - t; out.push(pos2(u * u * p0.x + 2.0 * u * t * b.x + t * t * p1.x, u * u * p0.y + 2.0 * u * t * b.y + t * t * p1.y)); }
    }
    out.push(pts[3]);
    out
}

fn draw_connector(painter: &egui::Painter, i: &Item, xf: &Xf, dark: bool, hide_text: bool) {
    let z = xf.z();
    let default = if dark { Color32::from_gray(0xd8) } else { SHAPE_LINE };
    let color = if i.color.as_deref() == Some(model::PEN_COLORS[0]) && dark { default } else { hex(i.color.as_deref(), default) };
    let width = (i.line_width.unwrap_or(2.0) as f32 * z).max(0.75);
    let pts = connector_screen(i, xf);
    if pts.len() < 2 { return; }
    stroke_path(painter, &pts, false, Stroke::new(width, color), i.str("lineStyle").unwrap_or("solid"), z);
    let n = pts.len();
    cap(painter, i.end_cap.as_deref().unwrap_or("arrow"), pts[n - 1], pts[n - 2], width, z, color, dark);
    cap(painter, i.start_cap.as_deref().unwrap_or("none"), pts[0], pts[1], width, z, color, dark);
    if !hide_text && !i.text.is_empty() && xf.v.z >= DETAIL_ZOOM {
        let c = label_point(i, xf);
        let galley = galley_for(painter, i, &i.text, i.font_size() as f32 * z, text_color(i, dark), 240.0 * z);
        let r = Rect::from_center_size(c, galley.size() + vec2(12.0, 6.0) * z);
        painter.rect_filled(r, 4.0 * z, board_bg(dark));
        let x = match i.align() { "center" => r.center().x, "right" => r.max.x - 6.0 * z, _ => r.min.x + 6.0 * z };
        painter.galley(pos2(x, r.min.y + 3.0 * z), galley, Color32::PLACEHOLDER);
    }
}

/// The middle of a line along its length, where its label sits.
pub fn label_point(i: &Item, xf: &Xf) -> Pos2 {
    let pts = connector_screen(i, xf);
    let total: f32 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut left = total / 2.0;
    for w in pts.windows(2) {
        let l = (w[1] - w[0]).length();
        if l >= left && l > 0.0 { return w[0] + (w[1] - w[0]) * (left / l); }
        left -= l;
    }
    pts[0]
}

/// A line end (`connector-caps.js`): arrowhead, triangles, diamonds, circles, bar, crow's foot.
#[allow(clippy::too_many_arguments)]
fn cap(painter: &egui::Painter, kind: &str, tip: Pos2, from: Pos2, width: f32, z: f32, color: Color32, dark: bool) {
    let d = (tip - from).normalized();
    if !d.x.is_finite() || kind == "none" { return; }
    let n = vec2(-d.y, d.x);
    let len = (8.0 * z + width * 2.5).max(6.0);
    let stroke = Stroke::new(width, color);
    let hollow = board_bg(dark);
    match kind {
        "arrow" => { painter.add(Shape::line(vec![tip - d * len + n * len * 0.55, tip, tip - d * len - n * len * 0.55], stroke)); }
        "triangle" | "triangle-open" => {
            let pts = vec![tip, tip - d * len + n * len * 0.55, tip - d * len - n * len * 0.55];
            painter.add(Shape::convex_polygon(pts, if kind == "triangle" { color } else { hollow }, stroke));
        }
        "diamond" | "diamond-open" => {
            let pts = vec![tip, tip - d * len * 0.75 + n * len * 0.4, tip - d * len * 1.5, tip - d * len * 0.75 - n * len * 0.4];
            painter.add(Shape::convex_polygon(pts, if kind == "diamond" { color } else { hollow }, stroke));
        }
        "circle" | "circle-open" => { painter.circle(tip - d * len * 0.35, len * 0.35, if kind == "circle" { color } else { hollow }, stroke); }
        "bar" => { painter.line_segment([tip - d * len * 0.6 + n * len * 0.5, tip - d * len * 0.6 - n * len * 0.5], stroke); }
        "crowfoot" => { let b = tip - d * len; for s in [-0.55, 0.0, 0.55] { painter.line_segment([b, tip + n * len * s], stroke); } }
        _ => {}
    }
}

// ---- Handles ----

/// What a press on the selection's handles grabs.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Grab { End(bool), Port(&'static str), Rotate, Resize(&'static str), Column(usize) }

/// Quick-connect ports of a lone selected shape, note or text: (direction, screen point).
fn ports(app: &BoardApp, xf: &Xf) -> Vec<(&'static str, Pos2)> {
    let Some(i) = app.single() else { return Vec::new() };
    if !edit::can_quick_connect(i) || i.is_locked() || app.editing.is_some() || i.rotation() != 0.0 { return Vec::new(); }
    let c = xf.s(geometry::center(i));
    let (hw, hh) = (i.w as f32 * xf.z() / 2.0, i.h as f32 * xf.z() / 2.0);
    vec![("top", c + vec2(0.0, -hh - PORT_GAP)), ("right", c + vec2(hw + PORT_GAP, 0.0)), ("bottom", c + vec2(0.0, hh + PORT_GAP)), ("left", c + vec2(-hw - PORT_GAP, 0.0))]
}

/// The rotation handle, below the bottom-left corner of a lone rotatable object.
fn rotate_handle(app: &BoardApp, xf: &Xf) -> Option<Pos2> {
    let i = app.single()?;
    if !geometry::can_rotate(i) || i.is_locked() || app.editing.is_some() { return None; }
    let corner = xf.obj(i, (0.0, i.h));
    let a = i.rotation().to_radians() as f32;
    let off = vec2(-ROTATE_GAP, ROTATE_GAP);
    Some(corner + vec2(off.x * a.cos() - off.y * a.sin(), off.x * a.sin() + off.y * a.cos()))
}

/// Resize handles on screen, turned with a lone rotated object.
fn resize_handles(app: &BoardApp, xf: &Xf) -> Vec<(&'static str, Pos2)> {
    let Some(b) = app.resize_box() else { return Vec::new() };
    let single = app.single().filter(|i| i.rotation() != 0.0);
    let small = (b.w.min(b.h) as f32 * xf.z()) < 24.0;
    let corners_only = small || app.single().is_some_and(|i| i.kind == "pen");
    geometry::HANDLES.iter().filter(|(h, _, _)| !corners_only || h.len() == 2).map(|(h, fx, fy)| {
        let p = match single { Some(i) => xf.obj(i, (fx * i.w, fy * i.h)), None => xf.s((b.x + fx * b.w, b.y + fy * b.h)) };
        (*h, p)
    }).collect()
}

fn line_ends(app: &BoardApp, xf: &Xf) -> Option<(Pos2, Pos2)> {
    let i = app.single().filter(|i| i.is_connector() && !i.is_locked())?;
    Some((xf.s((i.x, i.y)), xf.s((i.x + i.w, i.y + i.h))))
}

fn grab_at(app: &BoardApp, xf: &Xf, p: Pos2) -> Option<Grab> {
    if let Some((a, b)) = line_ends(app, xf) {
        if (p - b).length() <= 9.0 { return Some(Grab::End(true)); }
        if (p - a).length() <= 9.0 { return Some(Grab::End(false)); }
    }
    if let Some((d, _)) = ports(app, xf).into_iter().find(|(_, q)| (p - *q).length() <= 9.0) { return Some(Grab::Port(d)); }
    if rotate_handle(app, xf).is_some_and(|q| (p - q).length() <= 9.0) { return Some(Grab::Rotate); }
    if let Some(h) = resize_handles(app, xf).into_iter().find(|(_, q)| (p - *q).abs().max_elem() <= 8.0).map(|(h, _)| Grab::Resize(h)) { return Some(h); }
    // The borders between a selected table's columns.
    let t = app.single().filter(|i| i.kind == "table" && !i.is_locked() && i.rotation() == 0.0)?;
    let (cols, _) = edit::table_layout(t);
    let bp = xf.b(p);
    if bp.1 < t.y || bp.1 > t.y + t.h { return None; }
    (1..cols.len() - 1).find(|k| ((t.x + cols[*k] - bp.0) * xf.v.z).abs() <= 4.0).map(Grab::Column)
}

/// An object a line end dropped at `p` attaches to: the one under it, or one within reach.
fn target_at(app: &BoardApp, p: P, exclude: &[&str]) -> Option<String> {
    let reach = 14.0 / app.view.z;
    let ok = |i: &&Item| !i.is_connector() && !matches!(i.kind.as_str(), "pen" | "frame") && !exclude.contains(&i.id.as_str());
    geometry::stacked(&app.board.items).rev().filter(ok).find(|i| geometry::hits(i, p, 0.0) || {
        let b = geometry::object_bounds(i);
        Box2 { x: b.x - reach, y: b.y - reach, w: b.w + reach * 2.0, h: b.h + reach * 2.0 }.contains(p)
    }).map(|i| i.id.clone())
}

fn overlays(app: &mut BoardApp, ui: &mut Ui, painter: &egui::Painter, xf: &Xf, response: &egui::Response) {
    let pointer = ui.ctx().input(|i| i.pointer.hover_pos()).filter(|p| app.canvas_rect.contains(*p));
    let hovered = pointer.filter(|_| app.drag.is_none() && app.tool == Tool::Select && response.hovered())
        .and_then(|p| geometry::hit_test(&app.board.items, xf.b(p), 6.0 / app.view.z)).map(|i| i.id.clone());
    for i in &app.board.items {
        let selected = app.is_selected(&i.id);
        let target = app.target.as_deref() == Some(&i.id);
        if !selected && !target && hovered.as_deref() != Some(&i.id) { continue; }
        let stroke = Stroke::new(if target { 2.5 } else if selected { 1.5 } else { 1.0 }, ACCENT);
        if i.is_connector() {
            if !selected { painter.add(Shape::line(connector_screen(i, xf), Stroke::new(stroke.width + 3.0, ACCENT.gamma_multiply(0.3)))); }
        } else {
            let corners = [(0.0, 0.0), (i.w, 0.0), (i.w, i.h), (0.0, i.h)].map(|p| xf.obj(i, p));
            painter.add(Shape::closed_line(corners.to_vec(), stroke));
        }
    }
    let show_handles = app.drag.as_ref().is_none_or(|d| matches!(d, Drag::Resize { .. } | Drag::Rotate { .. }));
    if show_handles {
        if app.selection.len() > 1 && let Some(b) = app.resize_box() {
            painter.rect_stroke(Rect::from_two_pos(xf.s((b.x, b.y)), xf.s((b.x + b.w, b.y + b.h))), 0.0, Stroke::new(1.0, ACCENT), StrokeKind::Middle);
        }
        for (_, c) in resize_handles(app, xf) { painter.rect(Rect::from_center_size(c, vec2(8.0, 8.0)), 1.5, Color32::WHITE, Stroke::new(1.5, ACCENT), StrokeKind::Middle); }
        if let Some(c) = rotate_handle(app, xf) { painter.circle(c, 5.5, Color32::WHITE, Stroke::new(1.5, ACCENT)); }
    }
    if app.drag.is_none() {
        for (d, c) in ports(app, xf) {
            let hot = pointer.is_some_and(|p| (p - c).length() <= 9.0);
            painter.circle(c, if hot { 8.0 } else { 6.5 }, ACCENT, Stroke::new(1.5, Color32::WHITE));
            let n = match d { "top" => vec2(0.0, -1.0), "right" => vec2(1.0, 0.0), "bottom" => vec2(0.0, 1.0), _ => vec2(-1.0, 0.0) };
            let t = vec2(-n.y, n.x);
            painter.add(Shape::line(vec![c - n * 1.5 + t * 2.5, c + n * 1.5, c - n * 1.5 - t * 2.5], Stroke::new(1.5, Color32::WHITE)));
        }
        if let Some((a, b)) = line_ends(app, xf) { for p in [a, b] { painter.circle(p, 5.5, Color32::WHITE, Stroke::new(1.5, ACCENT)); } }
    }
    if let Some(Drag::Marquee { start, now, .. }) = &app.drag {
        painter.rect(Rect::from_two_pos(xf.s(*start), xf.s(*now)), 0.0, ACCENT.gamma_multiply(0.08), Stroke::new(1.0, ACCENT), StrokeKind::Middle);
    }

    if response.hovered() || app.drag.is_some() {
        let icon = match (&app.drag, app.tool) {
            (Some(Drag::Pan | Drag::Rotate { .. }), _) => egui::CursorIcon::Grabbing,
            (Some(Drag::Move { .. }), _) => egui::CursorIcon::Move,
            (_, Tool::Hand) => egui::CursorIcon::Grab,
            (_, Tool::Select) => match pointer.and_then(|p| grab_at(app, xf, p)) {
                Some(Grab::Resize("se" | "nw")) => egui::CursorIcon::ResizeNwSe,
                Some(Grab::Resize("ne" | "sw")) => egui::CursorIcon::ResizeNeSw,
                Some(Grab::Resize("e" | "w")) => egui::CursorIcon::ResizeHorizontal,
                Some(Grab::Resize(_)) => egui::CursorIcon::ResizeVertical,
                Some(Grab::Rotate) => egui::CursorIcon::Grab,
                Some(Grab::Column(_)) => egui::CursorIcon::ResizeColumn,
                Some(_) => egui::CursorIcon::PointingHand,
                None if hovered.is_some() => egui::CursorIcon::Move,
                None => egui::CursorIcon::Default,
            },
            (_, Tool::Text) => egui::CursorIcon::Text,
            (_, Tool::Comment) => egui::CursorIcon::Cell,
            _ => egui::CursorIcon::Crosshair,
        };
        ui.ctx().set_cursor_icon(icon);
    }
}

// ---- Input ----

fn handle_input(app: &mut BoardApp, ui: &mut Ui, response: &egui::Response, origin: Pos2) {
    let (zoom, scroll, mods, hover) = ui.ctx().input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.modifiers, i.pointer.hover_pos()));
    let space = ui.ctx().input(|i| i.key_down(egui::Key::Space)) && app.editing.is_none();
    let xf = Xf { v: app.view, origin };

    // Wheel pans, pinch or Ctrl/Cmd+wheel zooms at the pointer.
    if response.contains_pointer() && let Some(h) = hover {
        if zoom != 1.0 { app.view = app.view.zoom_at(((h.x - origin.x) as f64, (h.y - origin.y) as f64), app.view.z * zoom as f64); }
        else if scroll != egui::Vec2::ZERO { app.view.x += scroll.x as f64; app.view.y += scroll.y as f64; }
    }

    let middle = ui.ctx().input(|i| i.pointer.middle_down());
    if response.drag_started() {
        let Some(p) = ui.ctx().input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos()) else { return };
        let bp = xf.b(p);
        let grab = (app.tool == Tool::Select && !middle && !space).then(|| grab_at(app, &xf, p)).flatten();
        app.stop_editing();
        app.drag = Some(if middle || space || app.tool == Tool::Hand { Drag::Pan } else { start_drag(app, bp, grab, mods.shift) });
    }
    if app.drag.is_some() && response.dragged() {
        let p = response.interact_pointer_pos().or(hover).unwrap_or(origin);
        drag_to(app, xf.b(p), response.drag_delta(), mods.shift);
    }
    if response.drag_stopped() && let Some(drag) = app.drag.take() {
        app.guides.clear();
        let p = response.interact_pointer_pos().or(hover).unwrap_or(origin);
        finish_drag(app, drag, xf.b(p));
        app.target = None;
    }

    if response.clicked() && let Some(p) = response.interact_pointer_pos() { click(app, &xf, p, mods.shift || mods.command); }
    if response.double_clicked() && let Some(p) = response.interact_pointer_pos() { double_click(app, xf.b(p)); }
    if response.secondary_clicked() && let Some(p) = response.interact_pointer_pos()
        && let Some(id) = geometry::hit_test(&app.board.items, xf.b(p), 6.0 / app.view.z).map(|i| i.id.clone()) && !app.is_selected(&id) { app.select(&id, false); }
    response.context_menu(|ui| crate::toolbar::context_menu(app, ui));
    keyboard(app, ui, hover.filter(|h| app.canvas_rect.contains(*h)).map(|h| xf.b(h)));
}

fn start_drag(app: &mut BoardApp, bp: P, grab: Option<Grab>, shift: bool) -> Drag {
    let tol = 6.0 / app.view.z;
    match app.tool {
        Tool::Select => {
            match grab {
                Some(Grab::End(end)) => {
                    app.checkpoint();
                    return Drag::Endpoint { id: app.selection[0].clone(), end };
                }
                Some(Grab::Port(dir)) => {
                    let from = app.selection[0].clone();
                    app.checkpoint();
                    let line = app.add_item(Item::new("arrow", bp.0, bp.1).with(|l| { l.w = 0.0; l.h = 0.0; l.start_id = Some(from.clone()); l.line_type = Some("elbow".into()); }));
                    return Drag::Port { from, dir, line, start: bp };
                }
                Some(Grab::Rotate) => {
                    let i = app.single().unwrap().clone();
                    let c = geometry::center(&i);
                    app.checkpoint();
                    return Drag::Rotate { center: c, start_angle: (bp.1 - c.1).atan2(bp.0 - c.0).to_degrees(), orig: vec![i] };
                }
                Some(Grab::Column(k)) => {
                    app.checkpoint();
                    let t = app.single().unwrap();
                    let (cols, _) = edit::table_layout(t);
                    return Drag::Column { id: t.id.clone(), index: k, edges: cols };
                }
                Some(Grab::Resize(handle)) => {
                    let b = app.resize_box().unwrap();
                    let orig = app.board.items.iter().filter(|i| app.is_selected(&i.id) && !i.is_locked()).cloned().collect();
                    app.checkpoint();
                    return Drag::Resize { handle, start: bp, from: b, orig };
                }
                None => {}
            }
            let hit = geometry::hit_test(&app.board.items, bp, tol).map(|i| i.id.clone());
            // A press inside a selected object's box moves the selection, even off its ink.
            let in_selection = app.selected_items().iter().any(|i| !i.is_connector() && geometry::object_bounds(i).contains(bp));
            match hit {
                _ if in_selection && !shift => Drag::Move { start: bp, orig: app.moving_set(), moved: false },
                Some(id) => {
                    if !app.is_selected(&id) { app.select(&id, shift); }
                    Drag::Move { start: bp, orig: app.moving_set(), moved: false }
                }
                None => { if !shift { app.selection.clear(); } Drag::Marquee { start: bp, now: bp, add: shift } }
            }
        }
        Tool::Pen => {
            app.checkpoint();
            let color = app.pen_color.clone();
            let id = app.add_item(Item::new("pen", bp.0, bp.1).with(|i| { i.points = Some(vec![[0.0, 0.0]]); i.w = 0.0; i.h = 0.0; i.color = Some(color); i.line_width = Some(3.0); i.auto_font_size = None; }));
            Drag::Draw { id }
        }
        Tool::Arrow => {
            app.checkpoint();
            let start = target_at(app, bp, &[]);
            let id = app.add_item(Item::new("arrow", bp.0, bp.1).with(|i| { i.w = 0.0; i.h = 0.0; i.start_id = start; i.line_type = Some("elbow".into()); }));
            app.board.update_connectors();
            Drag::Connect { id }
        }
        Tool::Note | Tool::Text | Tool::Shape | Tool::Frame => {
            app.checkpoint();
            let mut i = new_object(app, bp);
            i.x = bp.0; i.y = bp.1; i.w = 4.0; i.h = 4.0;
            let id = app.add_item(i);
            Drag::Create { id, start: bp }
        }
        Tool::Hand | Tool::Comment => Drag::Pan,
    }
}

/// A new object for the current tool, at its default size, centred on `at`.
fn new_object(app: &BoardApp, at: P) -> Item {
    let kind = app.tool.kind();
    let mut i = Item::new(if kind == "rect" && app.shape == "ellipse" { "ellipse" } else { kind }, 0.0, 0.0);
    match i.kind.as_str() {
        "frame" => { i.text = format!("{} {}", crate::i18n::tr("Frame"), app.board.items.iter().filter(|i| i.kind == "frame").count() + 1); i.color = Some(model::COLORS[5].into()); i.w = 480.0; i.h = 340.0; i.auto_font_size = None; }
        "rect" | "ellipse" => {
            i.color = Some(model::COLORS[2].into());
            i.text_align = Some("center".into());
            i.auto_font_size = Some(true);
            if !matches!(app.shape, "rectangle" | "ellipse") { i.shape = Some(app.shape.to_string()); }
            if matches!(app.shape, "ellipse" | "diamond" | "star" | "or" | "summing-junction" | "triangle" | "hexagon") { i.w = 140.0; i.h = 140.0; } else { i.w = 180.0; i.h = 100.0; }
        }
        "note" => { i.w = 200.0; i.h = 200.0; }
        "text" => { i.auto_font_size = None; }
        _ => {}
    }
    i.x = at.0 - i.w / 2.0;
    i.y = at.1 - i.h / 2.0;
    i
}

/// Puts a line's free end (`end`: its far end) at `p`, keeping attached ends routed.
fn set_free_end(app: &mut BoardApp, id: &str, end: bool, p: P) {
    let place = |i: &mut Item| {
        if end { i.w = p.0 - i.x; i.h = p.1 - i.y; }
        else { let far = (i.x + i.w, i.y + i.h); i.x = p.0; i.y = p.1; i.w = far.0 - p.0; i.h = far.1 - p.1; }
    };
    if let Some(i) = app.board.find_mut(id) { place(i); }
    app.board.update_connectors();
    if let Some(i) = app.board.find_mut(id) { place(i); }
}

fn drag_to(app: &mut BoardApp, bp: P, delta: egui::Vec2, shift: bool) {
    let Some(drag) = &mut app.drag else { return };
    match drag {
        Drag::Pan => { app.view.x += delta.x as f64; app.view.y += delta.y as f64; }
        Drag::Move { start, orig, moved } => {
            let (mut dx, mut dy) = (bp.0 - start.0, bp.1 - start.1);
            if !*moved && (dx != 0.0 || dy != 0.0) { *moved = true; app.history.push(&app.board.items); }
            let orig = orig.clone();
            // Edges and centres snap to other objects' (Alt moves freely).
            app.guides.clear();
            if app.prefs.snap && !app.ctx.input(|i| i.modifiers.alt) && !orig.is_empty() {
                let b = geometry::bounds(&orig);
                let moving = Box2 { x: b.x + dx, y: b.y + dy, ..b };
                let ids: std::collections::HashSet<&str> = orig.iter().map(|o| o.id.as_str()).collect();
                let view = Box2::from_points(app.view.to_board((0.0, 0.0)), app.view.to_board((app.canvas_rect.width() as f64, app.canvas_rect.height() as f64)));
                let others: Vec<Box2> = app.board.items.iter().filter(|i| !ids.contains(i.id.as_str()) && !i.is_connector() && i.kind != "pen")
                    .map(geometry::object_bounds).filter(|o| o.intersects(&view) && !moving.within(o)).take(400).collect();
                let (sx, sy, g) = saboura_engine::arrange::snap(moving, &others, 6.0 / app.view.z);
                dx += sx; dy += sy;
                app.guides = g;
            }
            for o in &orig { if let Some(i) = app.board.find_mut(&o.id) { i.x = o.x + dx; i.y = o.y + dy; } }
            app.board.update_connectors();
            app.changed();
        }
        Drag::Resize { handle, start, from, orig } => {
            let (handle, start, from, orig) = (*handle, *start, *from, orig.clone());
            if orig.len() == 1 && orig[0].rotation() != 0.0 {
                // A rotated object resizes along its own axes; the opposite corner stays put.
                let o = &orig[0];
                let (c, r) = (geometry::center(o), o.rotation());
                let (lp, ls) = (geometry::rotate_point(bp, c, -r), geometry::rotate_point(start, c, -r));
                let ob = Box2 { x: o.x, y: o.y, w: o.w, h: o.h };
                let to = geometry::resized(ob, handle, lp.0 - ls.0, lp.1 - ls.1, shift, 8.0);
                let (fx, fy) = geometry::HANDLES.iter().find(|h| h.0 == handle).map(|h| (h.1, h.2)).unwrap_or((1.0, 1.0));
                let fixed = geometry::rotate_point((o.x + (1.0 - fx) * o.w, o.y + (1.0 - fy) * o.h), c, r);
                let moved = geometry::rotate_point((to.x + (1.0 - fx) * to.w, to.y + (1.0 - fy) * to.h), (to.x + to.w / 2.0, to.y + to.h / 2.0), r);
                if let Some(i) = app.board.find_mut(&o.id) {
                    geometry::fit_into(i, o, ob, to);
                    i.x += fixed.0 - moved.0; i.y += fixed.1 - moved.1;
                }
            } else {
                let keep = shift || orig.iter().any(|i| i.kind == "image");
                let to = geometry::resized(from, handle, bp.0 - start.0, bp.1 - start.1, keep, 8.0);
                for o in &orig {
                    if let Some(i) = app.board.find_mut(&o.id) {
                        geometry::fit_into(i, o, from, to);
                        // A text box's text scales with a corner drag.
                        if o.kind == "text" && handle.len() == 2 && from.h > 0.0 { i.font_size = Some((o.font_size() * to.h / from.h).clamp(6.0, 400.0)); }
                    }
                }
            }
            app.board.update_connectors();
            app.changed();
        }
        Drag::Rotate { center, start_angle, orig } => {
            let a = (bp.1 - center.1).atan2(bp.0 - center.0).to_degrees();
            let (o, start_angle) = (orig[0].clone(), *start_angle);
            let mut r = geometry::normalize_rotation(o.rotation() + a - start_angle);
            if shift { r = geometry::normalize_rotation((r / 15.0).round() * 15.0); }
            if let Some(i) = app.board.find_mut(&o.id) { i.rotation = (r.abs() > 0.01).then_some(r); }
            app.board.update_connectors();
            app.changed();
        }
        Drag::Marquee { now, .. } => *now = bp,
        Drag::Column { id, index, edges } => {
            // The border moves; the columns on either side share the difference.
            let (id, k, e) = (id.clone(), *index, edges.clone());
            if let Some(t) = app.board.find_mut(&id) {
                let x = (bp.0 - t.x).clamp(e[k - 1] + 40.0, e[k + 1] - 40.0);
                let mut widths: Vec<f64> = e.windows(2).map(|w| w[1] - w[0]).collect();
                widths[k - 1] = x - e[k - 1];
                widths[k] = e[k + 1] - x;
                t.set("columnWidths", serde_json::to_value(widths).unwrap_or_default());
            }
            app.changed();
        }
        Drag::Create { id, start } => {
            let (id, start) = (id.clone(), *start);
            let mut b = Box2::from_points(start, bp);
            if shift { let s = b.w.max(b.h); b.w = s; b.h = s; if bp.0 < start.0 { b.x = start.0 - s; } if bp.1 < start.1 { b.y = start.1 - s; } }
            if let Some(i) = app.board.find_mut(&id) { i.x = b.x; i.y = b.y; i.w = b.w.max(4.0); i.h = b.h.max(4.0); }
        }
        Drag::Draw { id } => {
            let (id, z) = (id.clone(), app.view.z);
            if let Some(i) = app.board.find_mut(&id) && let Some(pts) = &mut i.points && pts.len() < model::PEN_POINTS {
                let (x, y) = (bp.0 - i.x, bp.1 - i.y);
                let last = pts[pts.len() - 1];
                if (x - last[0]).hypot(y - last[1]) * z >= 0.5 { pts.push([x, y]); }
            }
        }
        Drag::Connect { id } | Drag::Port { line: id, .. } => {
            let id = id.clone();
            let from = app.board.find(&id).and_then(|l| l.start_id.clone());
            let exclude: Vec<&str> = [id.as_str()].into_iter().chain(from.as_deref()).collect();
            app.target = target_at(app, bp, &exclude);
            set_free_end(app, &id, true, bp);
        }
        Drag::Endpoint { id, end } => {
            let (id, end) = (id.clone(), *end);
            let other = app.board.find(&id).and_then(|l| if end { l.start_id.clone() } else { l.end_id.clone() });
            let exclude: Vec<&str> = [id.as_str()].into_iter().chain(other.as_deref()).collect();
            app.target = target_at(app, bp, &exclude);
            if let Some(i) = app.board.find_mut(&id) { if end { i.end_id = None; } else { i.start_id = None; } }
            set_free_end(app, &id, end, bp);
            app.changed();
        }
    }
}

fn finish_drag(app: &mut BoardApp, drag: Drag, bp: P) {
    match drag {
        Drag::Marquee { start, now, add } => {
            let b = Box2::from_points(start, now);
            if !add { app.selection.clear(); }
            let ids: Vec<String> = app.board.items.iter().filter(|i| geometry::object_bounds(i).within(&b)).map(|i| i.id.clone()).collect();
            for id in ids { app.select(&id, true); }
        }
        Drag::Create { id, start } => {
            // A drag too short to mean a size makes the object at its default size.
            let tiny = (bp.0 - start.0).abs() * app.view.z < 8.0 && (bp.1 - start.1).abs() * app.view.z < 8.0;
            if tiny {
                let fresh = new_object(app, start);
                if let Some(i) = app.board.find_mut(&id) { i.x = fresh.x; i.y = fresh.y; i.w = fresh.w; i.h = fresh.h; }
            }
            app.selection = vec![id.clone()];
            app.tool = Tool::Select;
            if app.board.find(&id).is_some_and(|i| matches!(i.kind.as_str(), "note" | "text")) { app.editing = Some(id); }
            app.changed();
        }
        Drag::Draw { id } => {
            // The drawing's box is fitted around its points.
            if let Some(i) = app.board.find_mut(&id) && let Some(pts) = &mut i.points {
                let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for p in pts.iter() { x0 = x0.min(p[0]); y0 = y0.min(p[1]); x1 = x1.max(p[0]); y1 = y1.max(p[1]); }
                for p in pts.iter_mut() { p[0] -= x0; p[1] -= y0; }
                i.x += x0; i.y += y0; i.w = x1 - x0; i.h = y1 - y0;
            }
            app.changed();
        }
        Drag::Connect { id } => {
            let end = app.target.take();
            let short = app.board.find(&id).is_some_and(|i| i.w.hypot(i.h) * app.view.z < 10.0) && end.is_none();
            if short { app.board.remove(&[id].into_iter().collect()); }
            else {
                if let Some(i) = app.board.find_mut(&id) { i.end_id = end; }
                app.board.update_connectors();
                app.selection = vec![id];
            }
            app.tool = Tool::Select;
            app.changed();
        }
        Drag::Port { from, dir, line, start } => {
            let far = (bp.0 - start.0).hypot(bp.1 - start.1) * app.view.z;
            match app.target.take() {
                Some(t) => {
                    if let Some(i) = app.board.find_mut(&line) { i.end_id = Some(t); }
                    app.board.update_connectors();
                    app.selection = vec![line];
                }
                None => {
                    app.board.remove(&[line].into_iter().collect());
                    if far < 10.0 {
                        // A press on a port without moving: a copy beside the object, joined to it.
                        if let Some(n) = edit::quick_connect(&mut app.board, &from, dir, "elbow") { app.selection = vec![n.clone()]; app.editing = Some(n); }
                    } else if let Some(src) = app.board.find(&from).cloned() {
                        // Dropped on the board: a copy there, joined to the object.
                        let mut copy = src.clone();
                        copy.id = model::uid();
                        copy.text.clear();
                        copy.group_id = None;
                        copy.extra.remove("textBlocks");
                        copy.x = bp.0 - copy.w / 2.0;
                        copy.y = bp.1 - copy.h / 2.0;
                        let n = copy.id.clone();
                        app.board.items.push(copy);
                        app.board.items.push(Item::new("arrow", 0.0, 0.0).with(|l| { l.start_id = Some(from.clone()); l.end_id = Some(n.clone()); l.line_type = Some("elbow".into()); }));
                        app.board.update_connectors();
                        app.selection = vec![n.clone()];
                        app.editing = Some(n);
                    }
                }
            }
            app.changed();
        }
        Drag::Endpoint { id, end } => {
            let t = app.target.take();
            if let Some(i) = app.board.find_mut(&id) { if end { i.end_id = t; } else { i.start_id = t; } }
            app.board.update_connectors();
            app.changed();
        }
        _ => {}
    }
}

/// A click: a port adds a joined copy; the select tool selects; creation tools place an
/// object at its default size.
fn click(app: &mut BoardApp, xf: &Xf, p: Pos2, add: bool) {
    let bp = xf.b(p);
    if matches!(app.panel, Panel::Shapes | Panel::Table(..)) { app.panel = Panel::None; }
    // While a vote runs, clicks on its objects vote (a click on one of your dots takes it back).
    if app.side == crate::app::Side::Votes && crate::panels::vote(app).is_some_and(|v| v["state"] == "active") {
        if let Some((id, mine)) = vote_target_at(app, xf, p) { crate::panels::cast(app, &id, mine); return; }
    }
    if let Some(id) = comment_pin_at(app, xf, p) { app.side = crate::app::Side::Comments; app.reply = Some((id, String::new())); return; }
    match app.tool {
        Tool::Comment => {
            crate::panels::new_comment(app, bp);
            app.tool = Tool::Select;
        }
        Tool::Select => {
            if let Some(Grab::Port(dir)) = grab_at(app, xf, p) {
                let id = app.selection[0].clone();
                app.quick_connect(&id, dir);
                return;
            }
            let hit = geometry::hit_test(&app.board.items, bp, 6.0 / app.view.z).map(|i| (i.id.clone(), i.str("link").map(str::to_string)));
            if app.editing.as_deref().is_some_and(|e| hit.as_ref().is_none_or(|h| h.0 != e)) { app.stop_editing(); }
            match hit {
                // Cmd/Ctrl-click follows an object's link.
                Some((_, Some(url))) if app.ctx.input(|i| i.modifiers.command) => app.ctx.open_url(egui::OpenUrl::new_tab(url)),
                Some((id, _)) if add && app.is_selected(&id) => app.selection.retain(|s| *s != id),
                Some((id, _)) => app.select(&id, add),
                None if !add => app.selection.clear(),
                None => {}
            }
        }
        Tool::Note | Tool::Text | Tool::Shape | Tool::Frame => {
            app.stop_editing();
            app.checkpoint();
            let i = new_object(app, bp);
            let edits = matches!(i.kind.as_str(), "note" | "text");
            let id = app.add_item(i);
            app.selection = vec![id.clone()];
            if edits { app.editing = Some(id); }
            app.tool = Tool::Select;
        }
        _ => {}
    }
}

fn double_click(app: &mut BoardApp, bp: P) {
    if app.tool != Tool::Select { return; }
    let hit = geometry::hit_test(&app.board.items, bp, 6.0 / app.view.z).map(|i| (i.id.clone(), i.kind.clone()));
    match hit {
        Some((id, kind)) if !matches!(kind.as_str(), "pen" | "image") => {
            app.select(&id, false);
            if kind == "table" { app.cell = app.board.find(&id).and_then(|i| cell_at(i, bp)); }
            start_editing(app, &id);
        }
        Some(_) => {}
        None => {
            app.checkpoint();
            let mut n = Item::new("note", 0.0, 0.0);
            n.w = 200.0; n.h = 200.0; n.x = bp.0 - 100.0; n.y = bp.1 - 100.0;
            let id = app.add_item(n);
            app.selection = vec![id.clone()];
            app.editing = Some(id);
        }
    }
}

pub fn start_editing(app: &mut BoardApp, id: &str) {
    if app.board.find(id).is_some_and(|i| i.is_locked() || matches!(i.kind.as_str(), "pen" | "image")) { return; }
    app.checkpoint();
    if app.board.find(id).is_some_and(|i| i.kind == "table") && app.cell.is_none() { app.cell = Some((0, 0)); }
    app.editing = Some(id.to_string());
}

/// The text field over the object (or table cell) being typed into.
fn text_editor(app: &mut BoardApp, ui: &mut Ui, xf: &Xf) {
    let Some(id) = app.editing.clone() else { return };
    let Some(item) = app.board.find(&id).cloned() else { app.editing = None; return };
    let z = xf.z();
    let (rect, mut text, align, size) = if item.is_connector() {
        (Rect::from_center_size(label_point(&item, xf), vec2(240.0 * z, 30.0 * z)), item.text.clone(), egui::Align::Center, item.font_size() as f32 * z)
    } else if item.kind == "table" {
        let cells = item.cells().unwrap_or_default();
        let (cols, rows) = edit::table_layout(&item);
        let (r, c) = app.cell.unwrap_or((0, 0));
        let (r, c) = (r.min(rows.len().saturating_sub(2)), c.min(cols.len().saturating_sub(2)));
        app.cell = Some((r, c));
        let cb = Rect::from_min_max(xf.s((item.x + cols[c], item.y + rows[r])), xf.s((item.x + cols[c + 1], item.y + rows[r + 1])));
        let t = cells.get(r).and_then(|row| row.get(c)).cloned().unwrap_or_default();
        (cb.shrink(6.0 * z), t, egui::Align::LEFT, item.font_size.unwrap_or(15.0) as f32 * z)
    } else {
        let b = text_box(&item);
        let r = Rect::from_min_size(xf.obj(&item, (b.x, b.y)), vec2(b.w as f32 * z, b.h as f32 * z));
        let a = match align_of(&item, &item.text) { "center" => egui::Align::Center, "right" => egui::Align::RIGHT, _ => egui::Align::LEFT };
        (r, item.text.clone(), a, item.font_size() as f32 * z)
    };
    let before = text.clone();
    let edit_id = egui::Id::unique(("edit", &id, app.cell));
    let color = text_color(&item, app.prefs.dark);
    let rtl_text = text.chars().any(is_rtl);
    let runs = saboura_engine::rich::read(item.get("textRuns"));
    let styled = !runs.is_empty() && item.kind != "table" && !item.text.chars().any(is_rtl);
    let item_for_layout = item.clone();
    let mut layouter = move |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
        let t = buf.as_str();
        let r = saboura_engine::rich::adjust(&item_for_layout.text, t, &runs);
        let mut job = rich_job(&item_for_layout, t, &r, size, color);
        job.wrap.max_width = wrap;
        job.halign = align;
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let out = egui::Area::new(egui::Id::unique("text-editor")).fixed_pos(rect.min).order(egui::Order::Middle).show(ui.ctx(), |ui| {
        ui.set_width(rect.width());
        let mut te = egui::TextEdit::multiline(&mut text).id(edit_id).frame(egui::Frame::NONE).desired_width(rect.width())
            .desired_rows(1).font(if rtl_text { FontId::proportional(size) } else { font_for(&item, size) }).text_color(color).lock_focus(true)
            .align(egui::Align2([align, egui::Align::TOP]));
        if styled { te = te.layouter(&mut layouter); }
        te.show(ui)
    }).inner;
    if out.response.response.has_focus() {
        app.text_sel = out.cursor_range.map(|r| { let (a, b) = (r.primary.index.0, r.secondary.index.0); (a.min(b), a.max(b)) });
    }
    if !out.response.response.has_focus() && !ui.ctx().memory(|m| m.had_focus_last_frame(edit_id)) { out.response.response.request_focus(); }
    if text != before {
        if let Some(i) = app.board.find_mut(&id) {
            if i.kind == "table" {
                let mut cells = i.cells().unwrap_or_default();
                if let Some((r, c)) = app.cell && let Some(cell) = cells.get_mut(r).and_then(|row| row.get_mut(c)) { *cell = text.chars().take(2000).collect(); }
                i.set_cells(&cells);
            } else {
                let new: String = text.chars().take(20000).collect();
                // Styled words stay on the same characters.
                let runs = saboura_engine::rich::read(i.get("textRuns"));
                if !runs.is_empty() { let r = saboura_engine::rich::adjust(&i.text, &new, &runs); i.set("textRuns", saboura_engine::rich::write(&r)); }
                i.text = new;
                // Lists cover every paragraph while on.
                if i.get("textBlocks").is_some() { let k = list_kind(i); set_list(i, k.as_deref()); }
                // A text box grows with its lines.
                if i.kind == "text" { i.h = i.h.max(out.galley.size().y as f64 / xf.v.z + 8.0); }
            }
        }
        app.changed();
    }
    let (escape, tab) = ui.ctx().input(|i| (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Tab)));
    if item.kind == "table" && tab {
        // Tab moves to the next cell.
        let cells = item.cells().unwrap_or_default();
        let (r, c) = app.cell.unwrap_or((0, 0));
        let cols = cells.first().map(|r| r.len()).unwrap_or(1);
        app.cell = Some(if c + 1 < cols { (r, c + 1) } else { ((r + 1) % cells.len().max(1), 0) });
        return;
    }
    if tab && item.is_mind() { app.add_branch(false); return; }
    if escape || out.response.response.lost_focus() && !ui.ctx().input(|i| i.pointer.any_down()) { app.stop_editing(); }
}

/// The list style of an object's text: "bullet", "number" or none.
pub fn list_kind(i: &Item) -> Option<String> {
    i.get("textBlocks").and_then(|b| b.as_array()).and_then(|b| b.first()).and_then(|b| b["list"].as_str()).map(str::to_string)
}

/// Makes every paragraph a list item of `kind`, or no list (`textBlocks`).
pub fn set_list(i: &mut Item, kind: Option<&str>) {
    let Some(kind) = kind else { i.extra.remove("textBlocks"); return };
    let mut blocks = Vec::new();
    let mut at = 0usize;
    for para in i.text.split('\n') {
        blocks.push(serde_json::json!({ "start": at, "list": kind, "indent": 0 }));
        at += para.encode_utf16().count() + 1;
    }
    i.set("textBlocks", serde_json::Value::Array(blocks));
}

fn keyboard(app: &mut BoardApp, ui: &mut Ui, pointer: Option<P>) {
    if app.editing.is_some() || ui.ctx().memory(|m| m.focused().is_some()) { return; }
    let events = ui.ctx().input(|i| i.events.clone());
    let at = pointer.unwrap_or_else(|| app.view_center());
    for e in events {
        match e {
            egui::Event::Copy => app.copy(),
            egui::Event::Cut => app.cut(),
            egui::Event::Paste(text) if text.is_empty() => if let Some(png) = crate::export::clipboard_image() { app.insert_image(&png, at); },
            egui::Event::Paste(text) => app.paste(Some(text), at),
            egui::Event::Key { key, pressed: true, modifiers: m, .. } => {
                let step = if m.shift { 10.0 } else { 1.0 };
                match key {
                    egui::Key::Z if m.command && m.shift => app.redo(),
                    egui::Key::Z if m.command => app.undo(),
                    egui::Key::Y if m.command => app.redo(),
                    egui::Key::A if m.command => app.select_all(),
                    egui::Key::D if m.command => app.duplicate(),
                    egui::Key::G if m.command => app.group(!m.shift),
                    egui::Key::K if m.command => app.panel = Panel::Palette(String::new()),
                    egui::Key::F if m.command => app.search = Some((String::new(), 0)),
                    egui::Key::Q if m.command => app.guard(crate::app::Pending::Quit),
                    // A picture on the clipboard (no text) pastes as an image.
                    egui::Key::V if m.command && crate::export::clipboard_text().is_none_or(|t| t.is_empty()) => if let Some(png) = crate::export::clipboard_image() { app.insert_image(&png, at); },
                    egui::Key::L if m.command && m.shift => app.toggle_lock(),
                    egui::Key::S if m.command => { app.save(m.shift); }
                    egui::Key::O if m.command => app.guard(crate::app::Pending::OpenDialog),
                    egui::Key::N if m.command => app.guard(crate::app::Pending::New("blank")),
                    egui::Key::B if m.command => toggle_flag(app, "bold"),
                    egui::Key::I if m.command => toggle_flag(app, "italic"),
                    egui::Key::U if m.command => toggle_flag(app, "underline"),
                    egui::Key::Equals | egui::Key::Plus if m.command => app.zoom_by(true),
                    egui::Key::Minus if m.command => app.zoom_by(false),
                    egui::Key::Num0 if m.command => app.zoom_reset(),
                    egui::Key::Num1 if m.shift => app.fit(),
                    egui::Key::CloseBracket if m.command => app.bring_to_front(true),
                    egui::Key::OpenBracket if m.command => app.bring_to_front(false),
                    egui::Key::Delete | egui::Key::Backspace => app.delete_selection(),
                    egui::Key::Escape => { app.selection.clear(); app.tool = Tool::Select; app.panel = Panel::None; }
                    egui::Key::Tab if app.single().is_some_and(|i| i.is_mind() || edit::can_quick_connect(i)) => app.add_branch(false),
                    egui::Key::Enter if m.shift && app.single().is_some_and(|i| i.mind_parent().is_some()) => app.add_branch(true),
                    egui::Key::Enter if app.selection.len() == 1 => { let id = app.selection[0].clone(); start_editing(app, &id); }
                    egui::Key::ArrowLeft => app.nudge(-step, 0.0),
                    egui::Key::ArrowRight => app.nudge(step, 0.0),
                    egui::Key::ArrowUp => app.nudge(0.0, -step),
                    egui::Key::ArrowDown => app.nudge(0.0, step),
                    _ if !m.command && !m.alt => match key {
                        egui::Key::V => app.tool = Tool::Select,
                        egui::Key::H => app.tool = Tool::Hand,
                        egui::Key::N => app.tool = Tool::Note,
                        egui::Key::T => app.tool = Tool::Text,
                        egui::Key::R => { app.tool = Tool::Shape; app.shape = "rectangle"; }
                        egui::Key::O => { app.tool = Tool::Shape; app.shape = "ellipse"; }
                        egui::Key::L => app.tool = Tool::Arrow,
                        egui::Key::P => app.tool = Tool::Pen,
                        egui::Key::F => app.tool = Tool::Frame,
                        egui::Key::C => app.tool = Tool::Comment,
                        egui::Key::S => app.panel = Panel::Library,
                        egui::Key::M => { app.prefs.minimap = !app.prefs.minimap; app.prefs.save(); }
                        _ => {}
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Turns a style flag (bold, italic, underline, strike) on for every selected object, or
/// off when all have it.
pub fn toggle_flag(app: &mut BoardApp, key: &'static str) {
    // Words selected while typing: on unless they all have it.
    if let (Some(id), Some((a, b))) = (app.editing.clone(), app.text_sel) && b > a && app.cell.is_none() {
        let on = !app.board.find(&id).is_some_and(|i| saboura_engine::rich::has(&i.text, &saboura_engine::rich::read(i.get("textRuns")), a, b, key) || (key == "bold" && i.is_bold()) || i.flag(key));
        set_text_style(app, key, if on { true.into() } else { false.into() });
        return;
    }
    let on = !app.selected_items().iter().all(|i| if key == "bold" { i.is_bold() } else { i.flag(key) });
    set_text_style(app, key, on.into());
}

/// Tag names and colours by ID (`tags` on the board).
pub fn tag_map(board: &saboura_engine::Board) -> HashMap<String, (String, Color32)> {
    board.extra.get("tags").and_then(|t| t.as_array()).map(|a| a.iter().filter_map(|t| Some((t["id"].as_str()?.to_string(), (t["name"].as_str()?.to_string(), hex(t["color"].as_str(), SHAPE_LINE))))).collect()).unwrap_or_default()
}

/// An object's tags as chips along its bottom edge.
fn draw_tags(painter: &egui::Painter, i: &Item, xf: &Xf, tags: &HashMap<String, (String, Color32)>) {
    let Some(ids) = i.get("tagIds").and_then(|t| t.as_array()) else { return };
    if xf.v.z < 0.3 || i.rotation() != 0.0 { return; }
    let mut x = xf.s((i.x, i.y + i.h)).x + 6.0;
    let y = xf.s((i.x, i.y + i.h)).y - 6.0;
    for id in ids.iter().filter_map(|v| v.as_str()) {
        let Some((name, color)) = tags.get(id) else { continue };
        let g = painter.layout_no_wrap(name.clone(), FontId::proportional(11.0 * xf.z().min(1.4)), Color32::WHITE);
        let r = Rect::from_min_size(pos2(x, y - g.size().y - 4.0), g.size() + vec2(10.0, 4.0));
        painter.rect_filled(r, r.height() / 2.0, *color);
        painter.galley(r.min + vec2(5.0, 2.0), g, Color32::WHITE);
        x = r.max.x + 4.0;
    }
}

/// Where a comment thread's pin sits on the board: by its object, or where it was placed.
pub fn pin_point(app: &BoardApp, t: &serde_json::Value) -> Option<P> {
    let (x, y) = (t["x"].as_f64()?, t["y"].as_f64()?);
    Some(match t["anchorId"].as_str().and_then(|a| app.board.find(a)) {
        Some(o) => (o.x + t["dx"].as_f64().unwrap_or(0.0), o.y + t["dy"].as_f64().unwrap_or(0.0)),
        None => (x, y),
    })
}

fn comment_threads(app: &BoardApp) -> Vec<serde_json::Value> {
    app.board.extra.get("comments").and_then(|c| c.as_array()).cloned().unwrap_or_default()
}

fn draw_comment_pins(app: &BoardApp, painter: &egui::Painter, xf: &Xf) {
    for t in comment_threads(app) {
        if t["resolved"].as_bool() == Some(true) { continue; }
        let Some(p) = pin_point(app, &t) else { continue };
        let c = xf.s(p);
        let open = app.reply.as_ref().is_some_and(|r| Some(r.0.as_str()) == t["id"].as_str());
        let fill = if open { ACCENT } else { Color32::from_rgb(0xf5, 0x9e, 0x0b) };
        painter.add(Shape::convex_polygon(vec![c, c + vec2(-11.0, -12.0), c + vec2(-11.0, -30.0), c + vec2(11.0, -30.0), c + vec2(11.0, -12.0)], fill, Stroke::new(1.5, Color32::WHITE)));
        let n = t["messages"].as_array().map(|m| m.len()).unwrap_or(0);
        painter.text(c + vec2(0.0, -21.0), egui::Align2::CENTER_CENTER, format!("{n}"), FontId::proportional(11.0), Color32::WHITE);
    }
}

fn comment_pin_at(app: &BoardApp, xf: &Xf, p: Pos2) -> Option<String> {
    comment_threads(app).into_iter().filter(|t| t["resolved"].as_bool() != Some(true)).find(|t| pin_point(app, t).is_some_and(|q| {
        let c = xf.s(q);
        Rect::from_min_max(c + vec2(-12.0, -31.0), c + vec2(12.0, 0.0)).contains(p)
    })).and_then(|t| t["id"].as_str().map(str::to_string))
}

/// Vote counts on the objects of the vote under way or just ended, with your own dots.
fn draw_votes(app: &BoardApp, painter: &egui::Painter, xf: &Xf) {
    let Some(v) = crate::panels::vote(app) else { return };
    let (all, mine) = crate::panels::tally(app);
    let active = v["state"] == "active";
    for t in v["targets"].as_array().into_iter().flatten().filter_map(|t| t.as_str()) {
        let Some(i) = app.board.find(t) else { continue };
        let n = all.get(t).copied().unwrap_or(0);
        if !active && n == 0 { continue; }
        let c = xf.s((i.x + i.w, i.y)) + vec2(-4.0, 4.0);
        let r = 11.0;
        painter.circle(c, r, if n > 0 { Color32::from_rgb(0xe1, 0x1d, 0x74) } else { Color32::from_gray(0x9a) }, Stroke::new(1.5, Color32::WHITE));
        painter.text(c, egui::Align2::CENTER_CENTER, format!("{n}"), FontId::proportional(11.0), Color32::WHITE);
        // Your own votes as dots along the bottom.
        let m = mine.get(t).copied().unwrap_or(0);
        for k in 0..m.min(20) {
            let d = xf.s((i.x, i.y + i.h)) + vec2(10.0 + k as f32 * 14.0, -10.0);
            painter.circle(d, 5.0, Color32::from_rgb(0xe1, 0x1d, 0x74), Stroke::new(1.0, Color32::WHITE));
        }
    }
}

/// The vote target under a click, and whether the click is on one of your dots.
fn vote_target_at(app: &BoardApp, xf: &Xf, p: Pos2) -> Option<(String, bool)> {
    let v = crate::panels::vote(app)?;
    let targets: Vec<&str> = v["targets"].as_array()?.iter().filter_map(|t| t.as_str()).collect();
    let (_, mine) = crate::panels::tally(app);
    for t in &targets {
        let Some(i) = app.board.find(t) else { continue };
        let m = mine.get(*t).copied().unwrap_or(0);
        for k in 0..m.min(20) {
            let d = xf.s((i.x, i.y + i.h)) + vec2(10.0 + k as f32 * 14.0, -10.0);
            if (p - d).length() <= 7.0 { return Some((t.to_string(), true)); }
        }
    }
    let bp = xf.b(p);
    geometry::stacked(&app.board.items).rev().find(|i| targets.contains(&i.id.as_str()) && geometry::hits(i, bp, 0.0)).map(|i| (i.id.clone(), false))
}

/// Sets a text style (bold, textColor, link...) on the words selected while typing, or on
/// the whole of every selected object.
pub fn set_text_style(app: &mut BoardApp, key: &str, value: serde_json::Value) {
    if let (Some(id), Some((a, b))) = (app.editing.clone(), app.text_sel) && b > a && app.cell.is_none() {
        app.checkpoint();
        if let Some(i) = app.board.find_mut(&id) {
            let runs = saboura_engine::rich::read(i.get("textRuns"));
            let runs = saboura_engine::rich::apply(&i.text, &runs, a, b, key, value);
            i.set("textRuns", saboura_engine::rich::write(&runs));
        }
        app.changed();
        return;
    }
    let key = key.to_string();
    app.edit_selected(move |i| {
        if key == "bold" { i.bold = match value.as_bool() { Some(on) if on != (i.kind == "frame") => Some(on), _ => None }; }
        else if value.as_bool() == Some(false) { i.extra.remove(&key); }
        else { i.set(&key, value.clone()); }
        // A style for the whole object replaces the same style on its words.
        let runs: Vec<saboura_engine::rich::Run> = saboura_engine::rich::read(i.get("textRuns")).into_iter().filter_map(|mut r| { r.style.remove(&key); (!r.style.is_empty()).then_some(r) }).collect();
        i.set("textRuns", saboura_engine::rich::write(&runs));
    });
}

/// A layout of text whose words carry their own styles (`textRuns`).
pub fn rich_job(i: &Item, text: &str, runs: &[saboura_engine::rich::Run], size: f32, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let byte_of: Vec<usize> = text.char_indices().map(|(b, _)| b).chain([text.len()]).collect();
    for (r, st) in saboura_engine::rich::spans(text, runs) {
        let flag = |k: &str| st.get(k).and_then(|v| v.as_bool());
        let mut item = i.clone();
        if let Some(b) = flag("bold") { item.bold = Some(b); }
        if let Some(it) = flag("italic") { item.set_flag("italic", it); }
        let c = st.get("textColor").and_then(|v| v.as_str()).map(|c| hex(Some(c), color)).unwrap_or(if st.contains_key("link") { LINK } else { color });
        let mut fmt = egui::TextFormat::simple(font_for(&item, size), c);
        let line = Stroke::new((size / 14.0).max(1.0), c);
        if flag("underline").unwrap_or(i.flag("underline")) || st.contains_key("link") || i.str("link").is_some() { fmt.underline = line; }
        if flag("strike").unwrap_or(i.flag("strike")) { fmt.strikethrough = line; }
        if let Some(h) = st.get("highlight").and_then(|v| v.as_str()) { fmt.background = hex(Some(h), Color32::TRANSPARENT); }
        job.append(&text[byte_of[r.start]..byte_of[r.end]], 0.0, fmt);
    }
    job
}
