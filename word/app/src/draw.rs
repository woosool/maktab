//! The Draw tab: pens, pencil and highlighter strokes saved as ink, the stroke eraser, lasso
//! selection, the ruler, Ink to Shape, drawing canvases, Ink Replay and Hide Ink.

use crate::app::WordApp;
use egui::Pos2;
use word_engine::model::{Anchor, RelFrom, Wrap};
use word_engine::objects::{Ink, InkStroke, Object, Shape, ShapeKind};
use word_engine::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tool { #[default] Select, Lasso, Eraser, Pen(usize) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenKind { Pen, Pencil, Highlighter }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenDef { pub kind: PenKind, pub color: Color, pub width: f32 }

impl PenDef {
    /// Stroke width in px from the pen's width in mm.
    pub fn px(&self) -> f32 { self.width * 96.0 / 25.4 }
}

pub const PEN_COLORS: &[Color] = &[Color(0, 0, 0), Color(0xFF, 0xFF, 0xFF), Color(0xC0, 0, 0), Color(0xFF, 0, 0), Color(0xFF, 0xC0, 0), Color(0xFF, 0xFF, 0), Color(0x92, 0xD0, 0x50), Color(0, 0xB0, 0x50), Color(0, 0xB0, 0xF0), Color(0, 0x70, 0xC0), Color(0, 0x20, 0x60), Color(0x70, 0x30, 0xA0), Color(0x80, 0x80, 0x80), Color(0xFF, 0x66, 0xCC), Color(0x99, 0x66, 0x33), Color(0x00, 0xFF, 0xFF)];
pub const PEN_WIDTHS: &[f32] = &[0.25, 0.35, 0.5, 0.7, 1.0, 2.0, 3.5];
pub const HIGHLIGHTER_WIDTHS: &[f32] = &[2.0, 4.0, 6.0, 8.0, 10.0];

pub struct DrawState {
    pub tool: Tool,
    pub pens: Vec<PenDef>,
    /// Touch input draws (otherwise it scrolls).
    pub touch: bool,
    /// The ruler straightens strokes drawn along it.
    pub ruler: bool,
    pub ink_to_shape: bool,
    pub hide_ink: bool,
    /// Ink Replay: started at this time.
    pub replay: Option<f64>,
    /// The stroke being drawn: page and points in page px.
    pub stroke: Option<(usize, Vec<(f32, f32)>)>,
    pub lasso: Option<(usize, Vec<(f32, f32)>)>,
    /// Lasso-selected strokes: (object position, stroke index).
    pub selected: Vec<(usize, usize)>,
    /// Dragging the selection from this page point.
    pub moving: Option<(f32, f32)>,
    pub next_seq: u32,
}

impl Default for DrawState {
    fn default() -> Self {
        DrawState {
            tool: Tool::Select,
            pens: vec![
                PenDef { kind: PenKind::Pen, color: Color(0, 0, 0), width: 0.5 },
                PenDef { kind: PenKind::Pen, color: Color(0xE0, 0x30, 0x30), width: 0.5 },
                PenDef { kind: PenKind::Pen, color: Color(0x2F, 0x6F, 0xE0), width: 0.5 },
                PenDef { kind: PenKind::Pencil, color: Color(0x40, 0x40, 0x40), width: 1.0 },
                PenDef { kind: PenKind::Highlighter, color: Color(0xFF, 0xF1, 0x00), width: 6.0 },
            ],
            touch: true, ruler: false, ink_to_shape: false, hide_ink: false, replay: None, stroke: None, lasso: None, selected: vec![], moving: None, next_seq: 1,
        }
    }
}

/// Distance from p to the segment ab.
fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (x, y) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - x).powi(2) + (p.1 - y).powi(2)).sqrt()
}

fn inside(poly: &[(f32, f32)], p: (f32, f32)) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 { c = !c; }
    }
    c
}

/// Ramer–Douglas–Peucker simplification.
fn simplify(pts: &[(f32, f32)], eps: f32) -> Vec<(f32, f32)> {
    if pts.len() < 3 { return pts.to_vec(); }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (k, d) = pts.iter().enumerate().skip(1).take(pts.len() - 2).map(|(k, p)| (k, seg_dist(*p, a, b))).fold((0, 0.0), |m, x| if x.1 > m.1 { x } else { m });
    if d > eps { let mut l = simplify(&pts[..=k], eps); l.pop(); l.extend(simplify(&pts[k..], eps)); l } else { vec![a, b] }
}

/// Recognize a stroke as a shape (Ink to Shape): a line, triangle, rectangle, diamond or
/// ellipse, with its box (x, y, w, h) and whether a line rises.
fn recognize(pts: &[(f32, f32)]) -> Option<(ShapeKind, (f32, f32, f32, f32), bool)> {
    if pts.len() < 4 { return None; }
    let (x0, y0) = pts.iter().fold((f32::MAX, f32::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
    let (x1, y1) = pts.iter().fold((f32::MIN, f32::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
    let (w, h) = (x1 - x0, y1 - y0);
    let diag = (w * w + h * h).sqrt();
    if diag < 12.0 { return None; }
    let (s, e) = (pts[0], pts[pts.len() - 1]);
    let closed = ((s.0 - e.0).powi(2) + (s.1 - e.1).powi(2)).sqrt() < diag * 0.25;
    if !closed {
        let dev = pts.iter().map(|p| seg_dist(*p, s, e)).fold(0.0, f32::max);
        if dev < diag * 0.08 { return Some((ShapeKind::Line, (x0, y0, w, h), (e.1 < s.1) != (e.0 < s.0))); }
        return None;
    }
    let mut ring = pts.to_vec();
    ring.push(s);
    let corners = simplify(&ring, diag * 0.09);
    let n = corners.len().saturating_sub(1);
    let near = |p: (f32, f32), q: (f32, f32)| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt() < diag * 0.2;
    let bx = (x0, y0, w, h);
    match n {
        3 => Some((ShapeKind::Triangle, bx, false)),
        4 => {
            let box_corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
            if corners[..4].iter().all(|c| box_corners.iter().any(|b| near(*c, *b))) { Some((ShapeKind::Rect, bx, false)) } else { Some((ShapeKind::Diamond, bx, false)) }
        }
        _ => {
            // An ellipse: points keep a steady normalized distance from the center.
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let r: Vec<f32> = pts.iter().map(|p| (((p.0 - cx) / (w / 2.0).max(1.0)).powi(2) + ((p.1 - cy) / (h / 2.0).max(1.0)).powi(2)).sqrt()).collect();
            let mean = r.iter().sum::<f32>() / r.len() as f32;
            let var = r.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / r.len() as f32;
            (var.sqrt() < 0.18).then_some((ShapeKind::Ellipse, bx, false))
        }
    }
}

impl WordApp {
    /// Ink objects on a page: (position, page box x, y, w, h, the ink).
    fn inks_on(&self, page: usize) -> Vec<(usize, f32, f32, f32, f32, Ink)> {
        let mut out = vec![];
        let mut pos = 0;
        for p in &self.doc.paragraphs {
            for r in &p.runs {
                if let Some(Object::Ink(i)) = &r.object {
                    if let Some((pg, x, y, w, h)) = self.layout.object_box(pos) { if pg == page { out.push((pos, x, y, w, h, i.clone())); } }
                }
                pos += r.len();
            }
            pos += 1;
        }
        out
    }

    /// The body paragraph that page-anchored ink on `page` belongs to.
    fn page_anchor_pos(&self, page: usize) -> Option<usize> {
        self.layout.lines.iter().find(|l| l.page == page && l.cell.is_none()).map(|l| self.doc.paragraph_start(self.doc.locate(l.start).0))
    }

    /// Add a finished stroke: into a drawing canvas or the page's ink (pen ink in front of the
    /// text, highlighter behind it), or as a shape with Ink to Shape.
    pub fn commit_stroke(&mut self, page: usize, mut pts: Vec<(f32, f32)>) {
        let Tool::Pen(k) = self.extras.draw.tool else { return };
        let Some(pen) = self.extras.draw.pens.get(k).copied() else { return };
        if pts.is_empty() { return; }
        if self.extras.draw.ruler && pts.len() > 1 { pts = vec![pts[0], *pts.last().unwrap()]; }
        let saved = self.doc.selection;
        if self.extras.draw.ink_to_shape && pen.kind != PenKind::Highlighter {
            if let Some((kind, (x, y, w, h), rising)) = recognize(&pts) {
                let mut s = Shape::new(kind);
                s.width = w.max(1.0); s.height = if kind == ShapeKind::Line { h } else { h.max(1.0) };
                s.fill = None; s.outline = Some(pen.color); s.outline_width = pen.px() * 0.75; s.flip_v = rising;
                let Some(at) = self.page_anchor_pos(page) else { return };
                self.doc.select(at, at);
                self.doc.insert_object(Object::Shape(s));
                let mut a = Anchor::new(Wrap::InFront, x, y);
                a.x_from = RelFrom::Page; a.y_from = RelFrom::Page; a.dist = 0.0;
                self.doc.set_anchor(at, Some(a));
                self.doc.selection = saved;
                return;
            }
        }
        let seq = self.extras.draw.next_seq;
        self.extras.draw.next_seq += 1;
        let highlighter = pen.kind == PenKind::Highlighter;
        let width = if pen.kind == PenKind::Pencil { pen.px() * 0.8 } else { pen.px() };
        // A drawing canvas under the stroke's start takes it.
        let start = pts[0];
        let target = self.inks_on(page).into_iter().find(|(_, x, y, w, h, i)| i.canvas && start.0 >= *x && start.0 <= x + w && start.1 >= *y && start.1 <= y + h)
            .or_else(|| self.inks_on(page).into_iter().find(|(pos, _, _, _, _, i)| !i.canvas && self.doc.anchor_at(*pos).is_some_and(|a| a.x_from == RelFrom::Page && (a.wrap == Wrap::Behind) == highlighter)));
        let stroke = |ox: f32, oy: f32| InkStroke { points: pts.iter().map(|p| (p.0 - ox, p.1 - oy)).collect(), color: pen.color, width, highlighter, seq };
        match target {
            Some((pos, x, y, _, _, mut ink)) => {
                ink.strokes.push(stroke(x, y));
                self.doc.set_object(pos, Object::Ink(ink));
            }
            None => {
                let Some(at) = self.page_anchor_pos(page) else { return };
                let g = self.doc.page.geometry();
                let ink = Ink { width: g.width, height: g.height, strokes: vec![stroke(0.0, 0.0)], canvas: false };
                self.doc.select(at, at);
                self.doc.insert_object(Object::Ink(ink));
                let mut a = Anchor::new(if highlighter { Wrap::Behind } else { Wrap::InFront }, 0.0, 0.0);
                a.x_from = RelFrom::Page; a.y_from = RelFrom::Page; a.dist = 0.0;
                self.doc.set_anchor(at, Some(a));
                // Positions after the new run moved by one.
                let shift = |p: usize| if p >= at { p + 1 } else { p };
                self.doc.selection = word_engine::model::Selection { anchor: shift(saved.anchor), focus: shift(saved.focus) };
                return;
            }
        }
        self.doc.selection = saved;
    }

    /// Erase whole strokes under (x, y) on `page`.
    pub fn erase_at(&mut self, page: usize, x: f32, y: f32) {
        let tol = 6.0 / self.zoom;
        for (pos, ox, oy, _, _, mut ink) in self.inks_on(page) {
            let p = (x - ox, y - oy);
            let before = ink.strokes.len();
            ink.strokes.retain(|s| {
                let r = tol + s.width / 2.0;
                if s.points.len() == 1 { return seg_dist(p, s.points[0], s.points[0]) > r; }
                !s.points.windows(2).any(|w| seg_dist(p, w[0], w[1]) <= r)
            });
            if ink.strokes.len() == before { continue; }
            if ink.strokes.is_empty() && !ink.canvas {
                let saved = self.doc.selection;
                self.doc.select(pos, pos + 1);
                self.doc.delete(true, false);
                let back = |q: usize| if q > pos { q - 1 } else { q };
                self.doc.selection = word_engine::model::Selection { anchor: back(saved.anchor), focus: back(saved.focus) };
            } else {
                let saved = self.doc.selection;
                self.doc.set_object(pos, Object::Ink(ink));
                self.doc.selection = saved;
            }
            return;
        }
    }

    /// Select the strokes inside a lasso.
    pub fn lasso_select(&mut self, page: usize, poly: &[(f32, f32)]) {
        let mut sel = vec![];
        for (pos, ox, oy, _, _, ink) in self.inks_on(page) {
            for (k, s) in ink.strokes.iter().enumerate() {
                if !s.points.is_empty() && s.points.iter().all(|p| inside(poly, (p.0 + ox, p.1 + oy))) { sel.push((pos, k)); }
            }
        }
        self.extras.draw.selected = sel;
    }

    /// The selected strokes' box on the page.
    pub fn ink_selection_box(&self) -> Option<(usize, f32, f32, f32, f32)> {
        let mut b: Option<(usize, f32, f32, f32, f32)> = None;
        for &(pos, k) in &self.extras.draw.selected {
            let (Some(Object::Ink(ink)), Some((pg, ox, oy, _, _))) = (self.doc.object_at(pos), self.layout.object_box(pos)) else { continue };
            let Some(s) = ink.strokes.get(k) else { continue };
            for p in &s.points {
                let (x, y) = (p.0 + ox, p.1 + oy);
                b = Some(match b { None => (pg, x, y, x, y), Some((g, a, c, d, e)) => (g, a.min(x), c.min(y), d.max(x), e.max(y)) });
            }
        }
        b.map(|(g, a, c, d, e)| (g, a, c, d - a, e - c))
    }

    /// Delete or move the lasso selection.
    pub fn edit_ink_selection(&mut self, delete: bool, dx: f32, dy: f32) {
        let mut by_obj: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for &(pos, k) in &self.extras.draw.selected { by_obj.entry(pos).or_default().push(k); }
        let saved = self.doc.selection;
        for (pos, ks) in by_obj.into_iter().rev() {
            let Some(Object::Ink(mut ink)) = self.doc.object_at(pos).cloned() else { continue };
            if delete { let mut k = 0; ink.strokes.retain(|_| { k += 1; !ks.contains(&(k - 1)) }); }
            else { for &k in &ks { if let Some(s) = ink.strokes.get_mut(k) { for p in &mut s.points { p.0 += dx; p.1 += dy; } } } }
            self.doc.set_object(pos, Object::Ink(ink));
        }
        self.doc.selection = saved;
        if delete { self.extras.draw.selected.clear(); }
    }

    /// Insert a drawing canvas: a framed area that keeps the ink drawn in it together.
    pub fn insert_drawing_canvas(&mut self) {
        let g = self.doc.page.geometry();
        let ink = Ink { width: (g.width - g.left - g.right).min(576.0), height: 288.0, strokes: vec![], canvas: true };
        if !self.doc.current_paragraph().is_empty() { self.doc.insert_paragraph(); }
        self.doc.insert_object(Object::Ink(ink));
        let e = self.doc.range().1;
        self.doc.select(e, e);
        self.doc.insert_paragraph();
        if let Tool::Select = self.extras.draw.tool { self.extras.draw.tool = Tool::Pen(0); }
    }

    /// The highest stroke number in the document (Ink Replay's end).
    pub fn last_ink_seq(&self) -> u32 {
        self.doc.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| if let Some(Object::Ink(i)) = &r.object { i.strokes.iter().map(|s| s.seq).max() } else { None }).max().unwrap_or(0)
    }

    /// Keep new stroke numbers after those already in the document.
    pub fn sync_ink_seq(&mut self) { let m = self.last_ink_seq(); if self.extras.draw.next_seq <= m { self.extras.draw.next_seq = m + 1; } }
}

/// Pointer handling while a drawing tool is active. Returns true when the event was used.
#[allow(clippy::too_many_arguments)]
pub fn pointer(app: &mut WordApp, ui: &egui::Ui, hovered: bool, pressed: bool, down: bool, released: bool, at: (usize, f32, f32), pointer: Pos2) -> bool {
    let tool = app.extras.draw.tool;
    if tool == Tool::Select { return false; }
    let _ = pointer;
    // With Draw with Touch off, fingers scroll instead of drawing.
    if !app.extras.draw.touch && ui.input(|i| i.any_touches()) { return false; }
    let (page, x, y) = at;
    ui.ctx().set_cursor_icon(match tool { Tool::Eraser => egui::CursorIcon::Cell, Tool::Lasso => egui::CursorIcon::Crosshair, _ => egui::CursorIcon::Crosshair });
    match tool {
        Tool::Pen(_) => {
            if pressed && hovered { app.sync_ink_seq(); app.extras.draw.stroke = Some((page, vec![(x, y)])); }
            else if down {
                if let Some((pg, pts)) = &mut app.extras.draw.stroke {
                    let last = *pts.last().unwrap();
                    if *pg == page && ((last.0 - x).powi(2) + (last.1 - y).powi(2)).sqrt() > 0.8 / app.zoom { pts.push((x, y)); }
                }
            }
            if released { if let Some((pg, pts)) = app.extras.draw.stroke.take() { app.commit_stroke(pg, pts); } }
        }
        Tool::Eraser => if (pressed && hovered) || down { app.erase_at(page, x, y); },
        Tool::Lasso => {
            if pressed && hovered {
                // Pressing inside the selection drags it; elsewhere starts a new lasso.
                match app.ink_selection_box() {
                    Some((pg, bx, by, bw, bh)) if pg == page && x >= bx - 4.0 && x <= bx + bw + 4.0 && y >= by - 4.0 && y <= by + bh + 4.0 => app.extras.draw.moving = Some((x, y)),
                    _ => { app.extras.draw.selected.clear(); app.extras.draw.lasso = Some((page, vec![(x, y)])); }
                }
            } else if down {
                if let Some((pg, pts)) = &mut app.extras.draw.lasso { if *pg == page { pts.push((x, y)); } }
            }
            if released {
                if let Some((pg, pts)) = app.extras.draw.lasso.take() { if pts.len() > 2 { app.lasso_select(pg, &pts); } }
                if let Some((sx, sy)) = app.extras.draw.moving.take() { if (x - sx).abs() > 0.5 || (y - sy).abs() > 0.5 { app.edit_ink_selection(false, x - sx, y - sy); } }
            }
        }
        Tool::Select => {}
    }
    true
}

/// Paint the stroke in progress, the lasso and the ink selection.
pub fn paint_overlay(app: &WordApp, painter: &egui::Painter, to_screen: &dyn Fn(usize, f32, f32) -> Pos2, zoom: f32, pointer: Option<(usize, f32, f32)>) {
    let d = &app.extras.draw;
    if let (Some((pg, pts)), Tool::Pen(k)) = (&d.stroke, d.tool) {
        if let Some(pen) = d.pens.get(k) {
            let c = egui::Color32::from_rgb(pen.color.0, pen.color.1, pen.color.2);
            let c = if pen.kind == PenKind::Highlighter { c.gamma_multiply(0.5) } else { c };
            let mut line: Vec<Pos2> = pts.iter().map(|p| to_screen(*pg, p.0, p.1)).collect();
            if d.ruler && line.len() > 1 { line = vec![line[0], *line.last().unwrap()]; }
            if line.len() == 1 { line.push(line[0] + egui::vec2(0.5, 0.5)); }
            painter.add(egui::Shape::line(line, egui::Stroke::new((pen.px() * zoom).max(1.0), c)));
        }
    }
    if let Some((pg, pts)) = &d.lasso {
        let line: Vec<Pos2> = pts.iter().map(|p| to_screen(*pg, p.0, p.1)).collect();
        painter.extend(egui::Shape::dashed_line(&line, egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 110, 220)), 4.0, 3.0));
    }
    if let Some((pg, x, y, w, h)) = app.ink_selection_box() {
        let off = match (d.moving, pointer) { (Some((sx, sy)), Some((p, px, py))) if p == pg => ((px - sx), (py - sy)), _ => (0.0, 0.0) };
        let r = egui::Rect::from_min_max(to_screen(pg, x + off.0 - 4.0, y + off.1 - 4.0), to_screen(pg, x + w + off.0 + 4.0, y + h + off.1 + 4.0));
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 110, 220)), egui::StrokeKind::Outside);
    }
    if d.ruler {
        // The ruler: a straightedge across the visible page (strokes snap to straight lines).
        if let Some((pg, _, py)) = pointer {
            let g = app.doc.page.geometry();
            let a = to_screen(pg, 0.0, py);
            let b = to_screen(pg, g.width, py);
            let band = egui::Rect::from_min_max(a - egui::vec2(0.0, 14.0), b + egui::vec2(0.0, 14.0));
            painter.rect(band, 2.0, egui::Color32::from_rgba_unmultiplied(240, 240, 240, 120), egui::Stroke::new(1.0, egui::Color32::from_gray(150)), egui::StrokeKind::Inside);
            let mut t = 0.0;
            while t < g.width { let p = to_screen(pg, t, py); let len = if (t / 96.0).fract() < 0.01 { 10.0 } else { 5.0 }; painter.line_segment([egui::pos2(p.x, band.min.y), egui::pos2(p.x, band.min.y + len)], egui::Stroke::new(1.0, egui::Color32::from_gray(120))); t += 12.0; }
        }
    }
}
