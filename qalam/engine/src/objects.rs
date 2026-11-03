//! Inline drawing objects: preset shapes (canvas-shapes.js, preset-geometries.js), charts
//! (canvas-charts.js, docx-charts.js) and SmartArt-style diagrams (diagram-builder.js).
//! Each renders to `DrawItem`s, so the screen, PDF and SVG draw them the same way.

use crate::fonts;
use crate::format::Color;
use crate::layout::{DrawItem, Effects, Glyph};
use std::f32::consts::PI;

/// Office theme accents (the default chart and SmartArt palette).
pub const PALETTE: [Color; 6] = [Color(0x44, 0x72, 0xC4), Color(0xED, 0x7D, 0x31), Color(0xA5, 0xA5, 0xA5), Color(0xFF, 0xC0, 0x00), Color(0x5B, 0x9B, 0xD5), Color(0x70, 0xAD, 0x47)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Rect, RoundRect, Ellipse, Triangle, RightTriangle, Diamond, Pentagon, Hexagon, Octagon, Star5, Heart,
    RightArrow, LeftArrow, UpArrow, DownArrow, Chevron, Parallelogram, Trapezoid, Plus, Callout, Line, Arrow,
}

/// Every shape with its OOXML preset name and gallery label.
pub const SHAPES: &[(ShapeKind, &str, &str)] = &[
    (ShapeKind::Line, "line", "Line"), (ShapeKind::Arrow, "straightConnector1", "Arrow"),
    (ShapeKind::Rect, "rect", "Rectangle"), (ShapeKind::RoundRect, "roundRect", "Rounded Rectangle"), (ShapeKind::Ellipse, "ellipse", "Oval"),
    (ShapeKind::Triangle, "triangle", "Triangle"), (ShapeKind::RightTriangle, "rtTriangle", "Right Triangle"), (ShapeKind::Diamond, "diamond", "Diamond"),
    (ShapeKind::Parallelogram, "parallelogram", "Parallelogram"), (ShapeKind::Trapezoid, "trapezoid", "Trapezoid"), (ShapeKind::Pentagon, "pentagon", "Pentagon"),
    (ShapeKind::Hexagon, "hexagon", "Hexagon"), (ShapeKind::Octagon, "octagon", "Octagon"), (ShapeKind::Star5, "star5", "Star"), (ShapeKind::Heart, "heart", "Heart"),
    (ShapeKind::Plus, "mathPlus", "Plus"), (ShapeKind::RightArrow, "rightArrow", "Right Arrow"), (ShapeKind::LeftArrow, "leftArrow", "Left Arrow"),
    (ShapeKind::UpArrow, "upArrow", "Up Arrow"), (ShapeKind::DownArrow, "downArrow", "Down Arrow"), (ShapeKind::Chevron, "chevron", "Chevron"),
    (ShapeKind::Callout, "wedgeRectCallout", "Callout"),
];

impl ShapeKind {
    pub fn preset(&self) -> &'static str { SHAPES.iter().find(|s| s.0 == *self).map(|s| s.1).unwrap_or("rect") }
    pub fn from_preset(p: &str) -> Option<ShapeKind> { SHAPES.iter().find(|s| s.1 == p).map(|s| s.0).or(match p { "flowChartProcess" => Some(ShapeKind::Rect), "flowChartAlternateProcess" => Some(ShapeKind::RoundRect), "straightConnector" => Some(ShapeKind::Line), _ => None }) }
    pub fn is_line(&self) -> bool { matches!(self, ShapeKind::Line | ShapeKind::Arrow) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub width: f32, // px
    pub height: f32,
    pub fill: Option<Color>,
    pub outline: Option<Color>,
    pub outline_width: f32, // pt
    pub text: String,
    pub text_color: Color,
    /// Clockwise rotation in degrees and mirroring (a:xfrm rot/flipH/flipV).
    pub rotation: f32,
    pub flip_h: bool,
    pub flip_v: bool,
    /// A text box: text starts at the top left inside the margins (bodyPr anchor="t").
    pub textbox: bool,
    /// Text size in points, and bold/italic text.
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    /// The drawing's name (wp:docPr name), e.g. "PowerPlusWaterMarkObject" for watermarks.
    pub name: String,
    /// Original `w:drawing` XML, written back while unchanged.
    pub raw: Option<String>,
}

impl Shape {
    pub fn new(kind: ShapeKind) -> Shape {
        let (w, h) = if kind.is_line() { (192.0, 0.0) } else { (144.0, 96.0) };
        Shape { kind, width: w, height: h, fill: (!kind.is_line()).then_some(PALETTE[0]), outline: Some(Color(0x2F, 0x52, 0x8F)), outline_width: 1.0, text: String::new(), text_color: Color::WHITE, rotation: 0.0, flip_h: false, flip_v: false, textbox: false, font_size: 11.0, bold: false, italic: false, name: String::new(), raw: None }
    }
    /// A text box: white, thin black border, black text from the top left.
    pub fn text_box(text: &str) -> Shape {
        let mut s = Shape::new(ShapeKind::Rect);
        s.width = 192.0; s.height = 96.0;
        s.fill = Some(Color::WHITE); s.outline = Some(Color::BLACK); s.outline_width = 0.75;
        s.text = text.to_string(); s.text_color = Color::BLACK; s.textbox = true;
        s
    }
}

/// One pen stroke: points in px from the ink's top-left, its color and width, and a sequence
/// number giving the drawing order (Ink Replay).
#[derive(Clone, Debug, PartialEq)]
pub struct InkStroke { pub points: Vec<(f32, f32)>, pub color: Color, pub width: f32, pub highlighter: bool, pub seq: u32 }

/// Ink drawn with the Draw tab's pens. A drawing canvas shows its frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Ink { pub width: f32, pub height: f32, pub strokes: Vec<InkStroke>, pub canvas: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartKind { Column, Bar, Line, Pie, Area }

pub const CHARTS: &[(ChartKind, &str)] = &[(ChartKind::Column, "Column"), (ChartKind::Bar, "Bar"), (ChartKind::Line, "Line"), (ChartKind::Pie, "Pie"), (ChartKind::Area, "Area")];

#[derive(Clone, Debug, PartialEq)]
pub struct Series { pub name: String, pub values: Vec<f64>, pub color: Option<Color> }

#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub kind: ChartKind,
    pub title: String,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    pub width: f32,
    pub height: f32,
    pub legend: bool,
    /// The original chart part and drawing, written back while unchanged.
    pub part: Option<String>,
    pub raw: Option<String>,
}

impl Chart {
    /// Word's sample data for a new chart.
    pub fn sample(kind: ChartKind) -> Chart {
        let categories = ["Category 1", "Category 2", "Category 3", "Category 4"].map(String::from).to_vec();
        let series = if kind == ChartKind::Pie {
            vec![Series { name: "Sales".into(), values: vec![8.2, 3.2, 1.4, 1.2], color: None }]
        } else {
            vec![
                Series { name: "Series 1".into(), values: vec![4.3, 2.5, 3.5, 4.5], color: None },
                Series { name: "Series 2".into(), values: vec![2.4, 4.4, 1.8, 2.8], color: None },
                Series { name: "Series 3".into(), values: vec![2.0, 2.0, 3.0, 5.0], color: None },
            ]
        };
        Chart { kind, title: if kind == ChartKind::Pie { "Sales".into() } else { "Chart Title".into() }, categories, series, width: 480.0, height: 288.0, legend: true, part: None, raw: None }
    }
    fn color(&self, i: usize) -> Color { self.series.get(i).and_then(|s| s.color).unwrap_or(PALETTE[i % PALETTE.len()]) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramKind { List, Process, Cycle, Hierarchy, Pyramid }

pub const DIAGRAMS: &[(DiagramKind, &str, &str)] = &[
    (DiagramKind::List, "list", "Basic List"), (DiagramKind::Process, "process", "Basic Process"), (DiagramKind::Cycle, "cycle", "Basic Cycle"),
    (DiagramKind::Hierarchy, "hierarchy", "Hierarchy"), (DiagramKind::Pyramid, "pyramid", "Basic Pyramid"),
];

/// A SmartArt-style diagram built from indented text: (level, text) items.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagram { pub kind: DiagramKind, pub items: Vec<(u8, String)>, pub width: f32, pub height: f32, pub color: Color }

impl Diagram {
    pub fn sample(kind: DiagramKind) -> Diagram {
        let items = match kind {
            DiagramKind::Hierarchy => vec![(0, "Director".into()), (1, "Design".into()), (2, "UX".into()), (1, "Engineering".into()), (2, "Platform".into()), (2, "Apps".into()), (1, "Sales".into())],
            _ => vec![(0, "Plan".into()), (0, "Build".into()), (0, "Test".into()), (0, "Launch".into())],
        };
        Diagram { kind, items, width: 480.0, height: 260.0, color: PALETTE[0] }
    }
    /// Spec stored in a saved picture's description, so this app can edit the diagram again.
    pub fn to_spec(&self) -> String {
        let kind = DIAGRAMS.iter().find(|d| d.0 == self.kind).map(|d| d.1).unwrap_or("list");
        let mut s = format!("word-diagram:{kind}:{}", self.color.hex());
        for (l, t) in &self.items { s.push('\n'); s.push_str(&"\t".repeat(*l as usize)); s.push_str(&t.replace('\n', " ")); }
        s
    }
    pub fn from_spec(spec: &str, width: f32, height: f32) -> Option<Diagram> {
        let mut lines = spec.lines();
        let head = lines.next()?.strip_prefix("word-diagram:")?;
        let (kind, color) = head.split_once(':').unwrap_or((head, "4472C4"));
        let kind = DIAGRAMS.iter().find(|d| d.1 == kind)?.0;
        let items = lines.map(|l| ((l.len() - l.trim_start_matches('\t').len()) as u8, l.trim_start_matches('\t').to_string())).filter(|i| !i.1.trim().is_empty()).collect();
        Some(Diagram { kind, items, width, height, color: Color::from_hex(color).unwrap_or(PALETTE[0]) })
    }
    /// Items from indented text: one per line, tabs or two spaces per level.
    pub fn items_from_text(text: &str) -> Vec<(u8, String)> {
        text.lines().filter(|l| !l.trim().is_empty()).map(|l| {
            let indent = l.chars().take_while(|c| c.is_whitespace()).map(|c| if c == '\t' { 2 } else { 1 }).sum::<usize>();
            ((indent / 2).min(4) as u8, l.trim().to_string())
        }).collect()
    }
    pub fn text(&self) -> String { self.items.iter().map(|(l, t)| format!("{}{t}", "\t".repeat(*l as usize))).collect::<Vec<_>>().join("\n") }
}

/// An inline object's content.
#[derive(Clone, Debug, PartialEq)]
pub enum Object { Shape(Shape), Chart(Chart), Diagram(Diagram), Equation(crate::equation::Equation), Ink(Ink) }

impl Object {
    pub fn size(&self) -> (f32, f32) {
        match self {
            Object::Shape(s) => (s.width, s.height.max(if s.kind.is_line() { 4.0 } else { 0.0 })), Object::Chart(c) => (c.width, c.height), Object::Diagram(d) => (d.width, d.height),
            Object::Equation(e) => { let (w, h, _) = e.metrics(); (w, h) }
            Object::Ink(i) => (i.width, i.height),
        }
    }
    /// How far the object reaches below the text baseline (equations sit on it like text).
    pub fn descent(&self) -> f32 { match self { Object::Equation(e) => e.metrics().2, _ => 0.0 } }
    pub fn is_inline_only(&self) -> bool { matches!(self, Object::Equation(_)) }
    pub fn set_size(&mut self, w: f32, h: f32) {
        match self {
            Object::Shape(s) => { s.width = w; s.height = h; s.raw = None; }
            Object::Chart(c) => { c.width = w; c.height = h; c.raw = None; }
            Object::Diagram(d) => { d.width = w; d.height = h; }
            // An equation scales its type size.
            Object::Equation(e) => { let (_, oh, _) = e.metrics(); if oh > 0.0 { e.size = (e.size * h / oh).clamp(4.0, 200.0); } }
            Object::Ink(i) => {
                let (sx, sy) = (if i.width > 0.0 { w / i.width } else { 1.0 }, if i.height > 0.0 { h / i.height } else { 1.0 });
                for s in &mut i.strokes { for p in &mut s.points { p.0 *= sx; p.1 *= sy; } }
                i.width = w; i.height = h;
            }
        }
    }
    pub fn clear_raw(&mut self) {
        match self { Object::Shape(s) => s.raw = None, Object::Chart(c) => c.raw = None, _ => {} }
    }
    /// Draw at the object's top-left corner (x, y).
    pub fn render(&self, x: f32, y: f32) -> Vec<DrawItem> {
        match self { Object::Shape(s) => render_shape(s, x, y), Object::Chart(c) => render_chart(c, x, y), Object::Diagram(d) => render_diagram(d, x, y), Object::Equation(e) => e.render(x, y), Object::Ink(i) => render_ink(i, x, y) }
    }
}

// ---------------------------------------------------------------------------
// Geometry

fn ellipse_pts(cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n).map(|i| { let a = a0 + (a1 - a0) * i as f32 / n as f32; (cx + rx * a.cos(), cy + ry * a.sin()) }).collect()
}

/// The outline of a preset shape in a w×h box (preset-geometries.js, default adjustments).
pub fn geometry(kind: ShapeKind, w: f32, h: f32) -> Vec<(f32, f32)> {
    let ss = w.min(h);
    match kind {
        ShapeKind::Rect | ShapeKind::Line | ShapeKind::Arrow => vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)],
        ShapeKind::RoundRect => {
            let r = ss * 0.1667;
            let mut v = ellipse_pts(w - r, r, r, r, -PI / 2.0, 0.0, 6);
            v.extend(ellipse_pts(w - r, h - r, r, r, 0.0, PI / 2.0, 6));
            v.extend(ellipse_pts(r, h - r, r, r, PI / 2.0, PI, 6));
            v.extend(ellipse_pts(r, r, r, r, PI, 1.5 * PI, 6));
            v
        }
        ShapeKind::Ellipse => ellipse_pts(w / 2.0, h / 2.0, w / 2.0, h / 2.0, 0.0, 2.0 * PI, 64),
        ShapeKind::Triangle => vec![(w / 2.0, 0.0), (w, h), (0.0, h)],
        ShapeKind::RightTriangle => vec![(0.0, 0.0), (w, h), (0.0, h)],
        ShapeKind::Diamond => vec![(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)],
        ShapeKind::Parallelogram => { let d = ss * 0.25; vec![(d, 0.0), (w, 0.0), (w - d, h), (0.0, h)] }
        ShapeKind::Trapezoid => { let d = ss * 0.25; vec![(d, 0.0), (w - d, 0.0), (w, h), (0.0, h)] }
        ShapeKind::Pentagon => (0..5).map(|i| { let a = -PI / 2.0 + i as f32 * 2.0 * PI / 5.0; (w / 2.0 + w / 2.0 * a.cos(), h * 0.5 + h * 0.5 * a.sin() * 1.05 + h * 0.025) }).collect(),
        ShapeKind::Hexagon => { let d = ss * 0.25; vec![(d, 0.0), (w - d, 0.0), (w, h / 2.0), (w - d, h), (d, h), (0.0, h / 2.0)] }
        ShapeKind::Octagon => { let d = ss * 0.29; vec![(d, 0.0), (w - d, 0.0), (w, d), (w, h - d), (w - d, h), (d, h), (0.0, h - d), (0.0, d)] }
        ShapeKind::Star5 => (0..10).map(|i| {
            let a = -PI / 2.0 + i as f32 * PI / 5.0;
            let r = if i % 2 == 0 { 1.0 } else { 0.38 };
            (w / 2.0 + w / 2.0 * r * a.cos(), h * 0.53 + h * 0.53 * r * a.sin())
        }).collect(),
        ShapeKind::Heart => {
            // Two lobes and a point, as cubic curves flattened.
            let mut v = vec![];
            let bez = |p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), v: &mut Vec<(f32, f32)>| for i in 0..=12 {
                let t = i as f32 / 12.0; let u = 1.0 - t;
                v.push((u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0, u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1));
            };
            bez((w / 2.0, h * 0.25), (w * 0.5, -h * 0.05), (w * 1.05, h * 0.0), (w, h * 0.35), &mut v);
            bez((w, h * 0.35), (w * 0.95, h * 0.65), (w * 0.6, h * 0.8), (w / 2.0, h), &mut v);
            bez((w / 2.0, h), (w * 0.4, h * 0.8), (w * 0.05, h * 0.65), (0.0, h * 0.35), &mut v);
            bez((0.0, h * 0.35), (-w * 0.05, h * 0.0), (w * 0.5, -h * 0.05), (w / 2.0, h * 0.25), &mut v);
            v
        }
        ShapeKind::RightArrow => { let d = h * 0.5; vec![(0.0, h * 0.25), (w - d, h * 0.25), (w - d, 0.0), (w, h / 2.0), (w - d, h), (w - d, h * 0.75), (0.0, h * 0.75)] }
        ShapeKind::LeftArrow => { let d = h * 0.5; vec![(w, h * 0.25), (d, h * 0.25), (d, 0.0), (0.0, h / 2.0), (d, h), (d, h * 0.75), (w, h * 0.75)] }
        ShapeKind::DownArrow => { let d = w * 0.5; vec![(w * 0.25, 0.0), (w * 0.75, 0.0), (w * 0.75, h - d), (w, h - d), (w / 2.0, h), (0.0, h - d), (w * 0.25, h - d)] }
        ShapeKind::UpArrow => { let d = w * 0.5; vec![(w * 0.25, h), (w * 0.75, h), (w * 0.75, d), (w, d), (w / 2.0, 0.0), (0.0, d), (w * 0.25, d)] }
        ShapeKind::Chevron => { let d = ss * 0.5; vec![(0.0, 0.0), (w - d, 0.0), (w, h / 2.0), (w - d, h), (0.0, h), (d, h / 2.0)] }
        ShapeKind::Plus => { let (a, b) = (w * 0.37, h * 0.37); vec![(a, 0.0), (w - a, 0.0), (w - a, b), (w, b), (w, h - b), (w - a, h - b), (w - a, h), (a, h), (a, h - b), (0.0, h - b), (0.0, b), (a, b)] }
        ShapeKind::Callout => vec![(0.0, 0.0), (w, 0.0), (w, h * 0.78), (w * 0.42, h * 0.78), (w * 0.2, h), (w * 0.25, h * 0.78), (0.0, h * 0.78)],
    }
}

// ---------------------------------------------------------------------------
// Text

/// One line of text as glyph runs, shaped per fallback face. Returns the items and the width.
pub fn text_line(text: &str, font: &str, bold: bool, size_pt: f32, color: Color, x: f32, baseline: f32) -> (Vec<DrawItem>, f32) {
    let size = size_pt * 4.0 / 3.0;
    let base = fonts::resolve(font, bold, false);
    let rtl = text.chars().any(fonts::is_complex);
    let mut items = vec![];
    let mut pen = x;
    // Split into runs drawn by one face.
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut segments: Vec<(fonts::FontKey, String)> = vec![];
    while i < chars.len() {
        let key = fonts::fallback(base, chars[i]);
        let mut seg = String::new();
        while i < chars.len() && fonts::fallback(base, chars[i]) == key { seg.push(chars[i]); i += 1; }
        segments.push((key, seg));
    }
    if rtl { segments.reverse(); }
    for (key, seg) in segments {
        let shaped = fonts::shape(key, &seg, rtl, size);
        let first = seg.chars().next().unwrap_or(' ');
        let glyphs: Vec<Glyph> = shaped.iter().scan(pen, |p, g| { let gl = Glyph { ch: first, gid: g.gid, x: *p + g.dx, dy: g.dy }; *p += g.advance; Some(gl) }).collect();
        pen += shaped.iter().map(|g| g.advance).sum::<f32>();
        items.push(DrawItem::Glyphs { glyphs, baseline, font: key, size, color, effects: Effects::default() });
    }
    (items, pen - x)
}

pub fn text_width(text: &str, font: &str, bold: bool, size_pt: f32) -> f32 { text_line(text, font, bold, size_pt, Color::BLACK, 0.0, 0.0).1 }

/// Text wrapped to `width` and centered in a box (shape text, diagram boxes).
fn centered_text(text: &str, size_pt: f32, bold: bool, color: Color, x: f32, y: f32, w: f32, h: f32) -> Vec<DrawItem> {
    let mut lines: Vec<String> = vec![];
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let try_line = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if !cur.is_empty() && text_width(&try_line, "Calibri", bold, size_pt) > w - 8.0 { lines.push(std::mem::take(&mut cur)); cur = word.to_string(); } else { cur = try_line; }
        }
        lines.push(cur);
    }
    let lh = size_pt * 4.0 / 3.0 * 1.2;
    let mut by = y + (h - lh * lines.len() as f32) / 2.0 + lh * 0.8;
    let mut out = vec![];
    for l in lines {
        let lw = text_width(&l, "Calibri", bold, size_pt);
        out.extend(text_line(&l, "Calibri", bold, size_pt, color, x + (w - lw) / 2.0, by).0);
        by += lh;
    }
    out
}

fn path(points: Vec<(f32, f32)>, closed: bool, fill: Option<Color>, stroke: Option<(Color, f32)>) -> DrawItem { DrawItem::Path { points, closed, fill, stroke, ink: None } }

fn shifted(points: Vec<(f32, f32)>, x: f32, y: f32) -> Vec<(f32, f32)> { points.into_iter().map(|(a, b)| (a + x, b + y)).collect() }

// ---------------------------------------------------------------------------
// Shapes

impl Shape {
    /// Map a point of the unrotated w×h box: mirror, then rotate about the center.
    pub fn transform(&self, (px, py): (f32, f32)) -> (f32, f32) {
        let (cx, cy) = (self.width / 2.0, self.height / 2.0);
        let (mut dx, mut dy) = (px - cx, py - cy);
        if self.flip_h { dx = -dx; }
        if self.flip_v { dy = -dy; }
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    }
}

pub fn render_shape(s: &Shape, x: f32, y: f32) -> Vec<DrawItem> {
    let mut out = render_unrotated(s);
    for item in &mut out {
        if let DrawItem::Path { points, .. } = item { for p in points.iter_mut() { let (a, b) = s.transform(*p); *p = (a + x, b + y); } }
        else if let DrawItem::Glyphs { glyphs, baseline, effects, .. } = item {
            // Text turns with the shape about its center; mirroring leaves it readable.
            *baseline += y;
            for g in glyphs.iter_mut() { g.x += x; }
            if s.rotation.rem_euclid(360.0) != 0.0 { effects.rotate = Some((s.rotation, x + s.width / 2.0, y + s.height / 2.0)); }
        }
    }
    out
}

fn render_unrotated(s: &Shape) -> Vec<DrawItem> {
    let (x, y) = (0.0, 0.0);
    let stroke = s.outline.map(|c| (c, (s.outline_width * 4.0 / 3.0).max(0.75)));
    let mut out = vec![];
    if s.kind.is_line() {
        let (x1, y1, x2, y2) = (x, y, x + s.width, y + s.height);
        let c = s.outline.unwrap_or(Color::BLACK);
        let w = (s.outline_width * 4.0 / 3.0).max(0.75);
        out.push(path(vec![(x1, y1), (x2, y2)], false, None, Some((c, w))));
        if s.kind == ShapeKind::Arrow {
            let (dx, dy) = (x2 - x1, y2 - y1);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (ux, uy) = (dx / len, dy / len);
            let head = (w * 4.0).max(8.0);
            out.push(path(vec![(x2, y2), (x2 - ux * head - uy * head * 0.5, y2 - uy * head + ux * head * 0.5), (x2 - ux * head + uy * head * 0.5, y2 - uy * head - ux * head * 0.5)], true, Some(c), None));
        }
        return out;
    }
    out.push(path(shifted(geometry(s.kind, s.width, s.height), x, y), true, s.fill, stroke));
    if !s.text.trim().is_empty() {
        if s.textbox { out.extend(box_text(s, x, y)); } else { out.extend(centered_text(&s.text, s.font_size, s.bold, s.text_color, x, y, s.width, s.height)); }
    }
    out
}

/// A text box's text: wrapped inside 0.1"/0.05" insets, from the top left.
fn box_text(s: &Shape, x: f32, y: f32) -> Vec<DrawItem> {
    let (ix, iy) = (9.6, 4.8);
    let w = s.width - 2.0 * ix;
    let mut out = vec![];
    let lh = s.font_size * 4.0 / 3.0 * 1.2;
    let mut by = y + iy + lh * 0.8;
    for para in s.text.split('\n') {
        let mut cur = String::new();
        let mut lines = vec![];
        for word in para.split_whitespace() {
            let t = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if !cur.is_empty() && text_width(&t, "Calibri", s.bold, s.font_size) > w { lines.push(std::mem::take(&mut cur)); cur = word.to_string(); } else { cur = t; }
        }
        lines.push(cur);
        for l in lines {
            if by > y + s.height { return out; }
            out.extend(text_line(&l, "Calibri", s.bold, s.font_size, s.text_color, x + ix, by).0);
            by += lh;
        }
    }
    out
}

/// Ink strokes (and a drawing canvas's frame) with the ink's top-left at (x, y).
pub fn render_ink(i: &Ink, x: f32, y: f32) -> Vec<DrawItem> {
    let mut out = vec![];
    if i.canvas {
        out.push(DrawItem::Path { points: vec![(x, y), (x + i.width, y), (x + i.width, y + i.height), (x, y + i.height)], closed: true, fill: None, stroke: Some((Color(0xBF, 0xBF, 0xBF), 0.75)), ink: None });
    }
    for s in &i.strokes {
        let pts: Vec<(f32, f32)> = if s.points.len() == 1 { vec![(s.points[0].0 + x, s.points[0].1 + y), (s.points[0].0 + x + 0.5, s.points[0].1 + y + 0.5)] } else { s.points.iter().map(|p| (p.0 + x, p.1 + y)).collect() };
        out.push(DrawItem::Path { points: pts, closed: false, fill: None, stroke: Some((s.color, s.width)), ink: Some(s.seq) });
    }
    out
}

/// Ink as a drawing group (wpg:wgp) of free-form shapes, one per stroke (custGeom paths), so
/// Word shows it; the group's description marks it as ink for reading back.
pub fn ink_drawing_xml(i: &Ink, id: u32) -> String {
    let (cx, cy) = (emu(i.width.max(1.0)), emu(i.height.max(1.0)));
    let mut shapes = String::new();
    for s in &i.strokes {
        if s.points.is_empty() { continue; }
        let (x0, y0) = s.points.iter().fold((f32::MAX, f32::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
        let (x1, y1) = s.points.iter().fold((f32::MIN, f32::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
        let (w, h) = (emu((x1 - x0).max(0.5)), emu((y1 - y0).max(0.5)));
        let pts: Vec<String> = s.points.iter().map(|p| format!(r#"<a:pt x="{}" y="{}"/>"#, emu(p.0 - x0), emu(p.1 - y0))).collect();
        let path = format!("<a:moveTo>{}</a:moveTo>{}", pts[0], pts[1..].iter().map(|p| format!("<a:lnTo>{p}</a:lnTo>")).collect::<String>());
        shapes += &format!(r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/><a:pathLst><a:path w="{w}" h="{h}" fill="none">{path}</a:path></a:pathLst></a:custGeom><a:noFill/><a:ln w="{}" cap="rnd"><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:round/></a:ln></wps:spPr><wps:bodyPr/></wps:wsp>"#, emu(x0), emu(y0), emu(s.width), s.color.hex());
    }
    let kind = if i.strokes.iter().any(|s| s.highlighter) { "word-ink:highlighter" } else if i.canvas { "word-ink:canvas" } else { "word-ink" };
    format!(r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="Ink {id}" descr="{kind}"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/><a:chOff x="0" y="0"/><a:chExt cx="{cx}" cy="{cy}"/></a:xfrm></wpg:grpSpPr>{shapes}</wpg:wgp></a:graphicData></a:graphic></wp:inline></w:drawing>"#)
}

/// Ink from a group written by `ink_drawing_xml`.
pub fn parse_ink(group: &crate::xml::Element, width: f32, height: f32, descr: &str) -> Ink {
    let highlighter = descr.contains("highlighter");
    let mut strokes = vec![];
    let num = |e: &crate::xml::Element, a: &str| e.attr(a).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 9525.0;
    for (k, wsp) in group.children_named("wps:wsp").enumerate() {
        let Some(sp) = wsp.child("wps:spPr") else { continue };
        let off = sp.child("a:xfrm").and_then(|x| x.child("a:off"));
        let (ox, oy) = off.map(|o| (num(o, "x"), num(o, "y"))).unwrap_or((0.0, 0.0));
        let mut points = vec![];
        if let Some(path) = sp.find("a:path") {
            for seg in path.children.iter().filter_map(|n| if let crate::xml::Node::Element(e) = n { Some(e) } else { None }) {
                if let Some(pt) = seg.child("a:pt") { points.push((ox + num(pt, "x"), oy + num(pt, "y"))); }
            }
        }
        let ln = sp.child("a:ln");
        let color = ln.and_then(|l| l.find("a:srgbClr")).and_then(|c| c.attr("val")).and_then(Color::from_hex).unwrap_or(Color::BLACK);
        let w = ln.map(|l| num(l, "w")).filter(|w| *w > 0.0).unwrap_or(1.5);
        strokes.push(InkStroke { points, color, width: w, highlighter, seq: k as u32 + 1 });
    }
    Ink { width, height, strokes, canvas: descr.contains("canvas") }
}

// ---------------------------------------------------------------------------
// Charts

/// A tick step of 1, 2 or 5 × 10ⁿ giving about five gridlines up to `max`.
fn nice_step(max: f64) -> f64 {
    if max <= 0.0 { return 1.0; }
    let raw = max / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    mag * if n <= 1.0 { 1.0 } else if n <= 2.0 { 2.0 } else if n <= 5.0 { 5.0 } else { 10.0 }
}

fn fmt_value(v: f64) -> String { if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v:.1}") } }

pub fn render_chart(c: &Chart, x: f32, y: f32) -> Vec<DrawItem> {
    let mut out = vec![];
    let (w, h) = (c.width, c.height);
    let grey = Color(0xD9, 0xD9, 0xD9);
    let ink = Color(0x59, 0x59, 0x59);
    out.push(path(shifted(geometry(ShapeKind::Rect, w, h), x, y), true, Some(Color::WHITE), Some((grey, 0.75))));
    let mut top = y + 10.0;
    if !c.title.is_empty() {
        let tw = text_width(&c.title, "Calibri", false, 14.0);
        out.extend(text_line(&c.title, "Calibri", false, 14.0, ink, x + (w - tw) / 2.0, top + 16.0).0);
        top += 28.0;
    }
    let legend_h = if c.legend { 22.0 } else { 0.0 };
    let bottom = y + h - 10.0 - legend_h;
    // Legend: series (or categories, for a pie) along the bottom.
    if c.legend {
        let entries: Vec<(String, Color)> = if c.kind == ChartKind::Pie { c.categories.iter().enumerate().map(|(i, n)| (n.clone(), PALETTE[i % 6])).collect() } else { c.series.iter().enumerate().map(|(i, s)| (s.name.clone(), c.color(i))).collect() };
        let total: f32 = entries.iter().map(|(n, _)| text_width(n, "Calibri", false, 9.0) + 22.0).sum();
        let mut lx = x + (w - total) / 2.0;
        let ly = y + h - 16.0;
        for (name, col) in entries {
            out.push(path(shifted(geometry(ShapeKind::Rect, 8.0, 8.0), lx, ly - 7.0), true, Some(col), None));
            out.extend(text_line(&name, "Calibri", false, 9.0, ink, lx + 11.0, ly).0);
            lx += text_width(&name, "Calibri", false, 9.0) + 22.0;
        }
    }
    if c.kind == ChartKind::Pie {
        let values: Vec<f64> = c.series.first().map(|s| s.values.clone()).unwrap_or_default();
        let total: f64 = values.iter().filter(|v| **v > 0.0).sum();
        let r = ((bottom - top) / 2.0).min(w / 2.0 - 20.0).max(10.0);
        let (cx, cy) = (x + w / 2.0, (top + bottom) / 2.0);
        let mut a = -PI / 2.0;
        for (i, v) in values.iter().enumerate() {
            if *v <= 0.0 || total <= 0.0 { continue; }
            let sweep = (*v / total) as f32 * 2.0 * PI;
            let mut pts = vec![(cx, cy)];
            pts.extend(ellipse_pts(cx, cy, r, r, a, a + sweep, (sweep * 20.0).ceil() as usize + 2));
            out.push(path(pts, true, Some(PALETTE[i % 6]), Some((Color::WHITE, 1.5))));
            a += sweep;
        }
        return out;
    }
    let max = c.series.iter().flat_map(|s| s.values.iter().copied()).fold(0.0f64, f64::max);
    let step = nice_step(max);
    let top_value = (max / step).ceil().max(1.0) * step;
    let horizontal = c.kind == ChartKind::Bar;
    // Axis labels' room.
    let label_w = (0..=((top_value / step) as usize)).map(|k| text_width(&fmt_value(k as f64 * step), "Calibri", false, 9.0)).fold(0.0, f32::max);
    let cat_w = c.categories.iter().map(|s| text_width(s, "Calibri", false, 9.0)).fold(0.0, f32::max);
    let (plot_l, plot_r, plot_t, plot_b) = if horizontal { (x + 14.0 + cat_w, x + w - 16.0, top, bottom - 18.0) } else { (x + 14.0 + label_w, x + w - 16.0, top, bottom - 18.0) };
    let ticks = (top_value / step).round() as usize;
    for k in 0..=ticks {
        let v = k as f64 * step;
        let f = (v / top_value) as f32;
        if horizontal {
            let gx = plot_l + (plot_r - plot_l) * f;
            out.push(path(vec![(gx, plot_t), (gx, plot_b)], false, None, Some((grey, 0.75))));
            let lw = text_width(&fmt_value(v), "Calibri", false, 9.0);
            out.extend(text_line(&fmt_value(v), "Calibri", false, 9.0, ink, gx - lw / 2.0, plot_b + 13.0).0);
        } else {
            let gy = plot_b - (plot_b - plot_t) * f;
            out.push(path(vec![(plot_l, gy), (plot_r, gy)], false, None, Some((grey, 0.75))));
            let lw = text_width(&fmt_value(v), "Calibri", false, 9.0);
            out.extend(text_line(&fmt_value(v), "Calibri", false, 9.0, ink, plot_l - 5.0 - lw, gy + 3.5).0);
        }
    }
    let n = c.categories.len().max(1);
    let ns = c.series.len().max(1);
    for (ci, cat) in c.categories.iter().enumerate() {
        let lw = text_width(cat, "Calibri", false, 9.0);
        if horizontal {
            let band = (plot_b - plot_t) / n as f32;
            // Bar charts list the first category at the bottom, as Word does.
            let cy = plot_b - band * (ci as f32 + 0.5);
            out.extend(text_line(cat, "Calibri", false, 9.0, ink, plot_l - 5.0 - lw, cy + 3.5).0);
        } else {
            let band = (plot_r - plot_l) / n as f32;
            let cx = plot_l + band * (ci as f32 + 0.5);
            out.extend(text_line(cat, "Calibri", false, 9.0, ink, cx - lw / 2.0, plot_b + 13.0).0);
        }
    }
    let to_y = |v: f64| plot_b - (plot_b - plot_t) * (v.max(0.0) / top_value) as f32;
    match c.kind {
        ChartKind::Column | ChartKind::Bar => {
            for (si, s) in c.series.iter().enumerate() {
                for (ci, v) in s.values.iter().enumerate().take(n) {
                    let f = (v.max(0.0) / top_value) as f32;
                    if horizontal {
                        let band = (plot_b - plot_t) / n as f32;
                        let bh = band * 0.6 / ns as f32;
                        let by = plot_b - band * (ci as f32 + 1.0) + band * 0.2 + bh * (ns - 1 - si) as f32;
                        out.push(path(shifted(geometry(ShapeKind::Rect, (plot_r - plot_l) * f, bh), plot_l, by), true, Some(c.color(si)), None));
                    } else {
                        let band = (plot_r - plot_l) / n as f32;
                        let bw = band * 0.6 / ns as f32;
                        let bx = plot_l + band * ci as f32 + band * 0.2 + bw * si as f32;
                        let bh = (plot_b - plot_t) * f;
                        out.push(path(shifted(geometry(ShapeKind::Rect, bw, bh), bx, plot_b - bh), true, Some(c.color(si)), None));
                    }
                }
            }
        }
        ChartKind::Line | ChartKind::Area => {
            let band = (plot_r - plot_l) / n as f32;
            for (si, s) in c.series.iter().enumerate().rev() {
                let pts: Vec<(f32, f32)> = s.values.iter().take(n).enumerate().map(|(ci, v)| (plot_l + band * (ci as f32 + 0.5), to_y(*v))).collect();
                if c.kind == ChartKind::Area && pts.len() > 1 {
                    let mut poly = pts.clone();
                    poly.push((pts.last().unwrap().0, plot_b));
                    poly.push((pts[0].0, plot_b));
                    out.push(path(poly, true, Some(c.color(si)), None));
                } else {
                    out.push(path(pts.clone(), false, None, Some((c.color(si), 2.25))));
                    for p in pts { out.push(path(ellipse_pts(p.0, p.1, 3.0, 3.0, 0.0, 2.0 * PI, 12), true, Some(c.color(si)), None)); }
                }
            }
        }
        ChartKind::Pie => {}
    }
    // Axis line.
    if horizontal { out.push(path(vec![(plot_l, plot_t), (plot_l, plot_b)], false, None, Some((grey, 1.0)))); }
    else { out.push(path(vec![(plot_l, plot_b), (plot_r, plot_b)], false, None, Some((grey, 1.0)))); }
    out
}

// ---------------------------------------------------------------------------
// Diagrams

fn shade(c: Color, k: f32) -> Color {
    let m = |v: u8| (v as f32 + (255.0 - v as f32) * k).round().clamp(0.0, 255.0) as u8;
    Color(m(c.0), m(c.1), m(c.2))
}

fn boxed(out: &mut Vec<DrawItem>, kind: ShapeKind, x: f32, y: f32, w: f32, h: f32, fill: Color, text: &str, size: f32) {
    out.push(path(shifted(geometry(kind, w, h), x, y), true, Some(fill), Some((Color::WHITE, 1.5))));
    out.extend(centered_text(text, size, false, Color::WHITE, x, y, w, h));
}

pub fn render_diagram(d: &Diagram, x: f32, y: f32) -> Vec<DrawItem> {
    let mut out = vec![];
    let (w, h) = (d.width, d.height);
    let tops: Vec<&(u8, String)> = d.items.iter().filter(|i| i.0 == 0).collect();
    let n = tops.len().max(1);
    let children = |k: usize| -> Vec<String> {
        // Level-1+ items following the k-th top-level item.
        let mut seen = 0usize;
        let mut out = vec![];
        let mut inside = false;
        for (l, t) in &d.items {
            if *l == 0 { inside = seen == k; seen += 1; } else if inside { out.push(t.clone()); }
        }
        out
    };
    match d.kind {
        DiagramKind::List => {
            let gap = 8.0;
            let bw = (w - gap * (n as f32 - 1.0)) / n as f32;
            for (k, (_, t)) in tops.iter().enumerate() {
                let bx = x + k as f32 * (bw + gap);
                boxed(&mut out, ShapeKind::Rect, bx, y, bw, h * 0.45, shade(d.color, 0.0), t, 14.0);
                let kids = children(k);
                if !kids.is_empty() {
                    out.push(path(shifted(geometry(ShapeKind::Rect, bw, h * 0.5), bx, y + h * 0.5), true, Some(shade(d.color, 0.8)), Some((shade(d.color, 0.4), 0.75))));
                    let mut ty = y + h * 0.5 + 18.0;
                    for kid in kids { out.extend(text_line(&format!("• {kid}"), "Calibri", false, 10.0, Color(0x33, 0x33, 0x33), bx + 8.0, ty).0); ty += 16.0; }
                }
            }
        }
        DiagramKind::Process => {
            let arrow = 26.0;
            let bw = (w - arrow * (n as f32 - 1.0) - 8.0 * (n as f32 - 1.0)) / n as f32;
            let bh = h * 0.6;
            let by = y + (h - bh) / 2.0;
            for (k, (_, t)) in tops.iter().enumerate() {
                let bx = x + k as f32 * (bw + arrow + 8.0);
                boxed(&mut out, ShapeKind::RoundRect, bx, by, bw, bh, d.color, t, 13.0);
                if k + 1 < n { out.push(path(shifted(geometry(ShapeKind::RightArrow, arrow, 24.0), bx + bw + 4.0, by + bh / 2.0 - 12.0), true, Some(shade(d.color, 0.55)), None)); }
            }
        }
        DiagramKind::Cycle => {
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            let r = (h / 2.0 - 30.0).max(20.0);
            let bw = (r * 1.1).min(110.0);
            for (k, (_, t)) in tops.iter().enumerate() {
                let a = -PI / 2.0 + k as f32 * 2.0 * PI / n as f32;
                let (px, py) = (cx + r * 1.15 * a.cos(), cy + r * a.sin());
                boxed(&mut out, ShapeKind::Ellipse, px - bw / 2.0, py - 25.0, bw, 50.0, d.color, t, 12.0);
                // Arc arrows between neighbours.
                let a2 = a + 2.0 * PI / n as f32;
                let mid = (a + a2) / 2.0;
                let ar = ellipse_pts(cx, cy, r * 1.15, r, a + 0.35, a2 - 0.35, 8);
                out.push(path(ar, false, None, Some((shade(d.color, 0.5), 3.0))));
                let tip = (cx + r * 1.15 * (a2 - 0.35).cos(), cy + r * (a2 - 0.35).sin());
                let dir = (-(a2 - 0.35).sin(), (a2 - 0.35).cos());
                let _ = mid;
                out.push(path(vec![(tip.0 + dir.0 * 7.0, tip.1 + dir.1 * 7.0), (tip.0 - dir.1 * 6.0, tip.1 + dir.0 * 6.0), (tip.0 + dir.1 * 6.0, tip.1 - dir.0 * 6.0)], true, Some(shade(d.color, 0.5)), None));
            }
        }
        DiagramKind::Hierarchy => {
            // Rows by level; each item centered over the span of its descendants.
            let max_level = d.items.iter().map(|i| i.0).max().unwrap_or(0) as usize;
            let rows = max_level + 1;
            let bh = ((h - 16.0 * (rows as f32 - 1.0)) / rows as f32).min(54.0);
            let leaves: Vec<usize> = (0..d.items.len()).filter(|&i| d.items.get(i + 1).is_none_or(|n| n.0 <= d.items[i].0)).collect();
            let lw = w / leaves.len().max(1) as f32;
            let mut centers = vec![0.0f32; d.items.len()];
            for i in (0..d.items.len()).rev() {
                if let Some(k) = leaves.iter().position(|&l| l == i) { centers[i] = x + lw * (k as f32 + 0.5); continue; }
                let level = d.items[i].0;
                let kids: Vec<usize> = (i + 1..d.items.len()).take_while(|&j| d.items[j].0 > level).filter(|&j| d.items[j].0 == level + 1).collect();
                centers[i] = if kids.is_empty() { x + w / 2.0 } else { (centers[kids[0]] + centers[*kids.last().unwrap()]) / 2.0 };
            }
            let bw = (lw - 10.0).clamp(40.0, 140.0);
            for (i, (level, t)) in d.items.iter().enumerate() {
                let by = y + *level as f32 * (bh + 16.0);
                // Connector to the parent.
                if let Some(p) = (0..i).rev().find(|&j| d.items[j].0 + 1 == *level) {
                    let py = y + d.items[p].0 as f32 * (bh + 16.0) + bh;
                    out.push(path(vec![(centers[p], py), (centers[p], by - 8.0), (centers[i], by - 8.0), (centers[i], by)], false, None, Some((shade(d.color, 0.4), 1.25))));
                }
                boxed(&mut out, ShapeKind::RoundRect, centers[i] - bw / 2.0, by, bw, bh, shade(d.color, *level as f32 * 0.18), t, 11.0);
            }
        }
        DiagramKind::Pyramid => {
            let bh = h / n as f32;
            for (k, (_, t)) in tops.iter().enumerate() {
                let (t0, t1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
                let cx = x + w / 2.0;
                let pts = vec![(cx - w / 2.0 * t0, y + bh * k as f32), (cx + w / 2.0 * t0, y + bh * k as f32), (cx + w / 2.0 * t1, y + bh * (k + 1) as f32), (cx - w / 2.0 * t1, y + bh * (k + 1) as f32)];
                out.push(path(pts, true, Some(shade(d.color, k as f32 * 0.12)), Some((Color::WHITE, 1.5))));
                out.extend(centered_text(t, 12.0, false, Color::WHITE, cx - w / 2.0 * t1, y + bh * k as f32, w * t1, bh));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn objects_render_and_round_trip_specs() {
        for (k, _, _) in SHAPES { assert!(!render_shape(&Shape::new(*k), 0.0, 0.0).is_empty()); }
        for (k, _) in CHARTS { assert!(render_chart(&Chart::sample(*k), 0.0, 0.0).len() > 5); }
        for (k, _, _) in DIAGRAMS {
            let d = Diagram::sample(*k);
            assert!(render_diagram(&d, 0.0, 0.0).len() > 3);
            assert_eq!(Diagram::from_spec(&d.to_spec(), d.width, d.height).unwrap(), d);
        }
        assert_eq!(nice_step(4.5), 1.0);
        assert_eq!(Diagram::items_from_text("A\n  a1\nB"), vec![(0, "A".to_string()), (1, "a1".to_string()), (0, "B".to_string())]);
    }
}

// ---------------------------------------------------------------------------
// DOCX

fn xml_text(s: &str) -> String { let mut o = String::new(); crate::xml::escape_into(s, &mut o, false); o }
fn emu(px: f32) -> i64 { (px * 9525.0).round() as i64 }

/// A shape as `w:drawing` (inline wps:wsp, §20.4 / MS-ODRAWXML wordprocessingShape).
pub fn shape_drawing_xml(s: &Shape, id: u32) -> String {
    let (cx, cy) = (emu(s.width), emu(s.height.max(if s.kind.is_line() { 0.0 } else { 1.0 })));
    let fill = match s.fill { Some(c) if !s.kind.is_line() => format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, c.hex()), _ => "<a:noFill/>".into() };
    let line = match s.outline {
        Some(c) => format!(r#"<a:ln w="{}"><a:solidFill><a:srgbClr val="{}"/></a:solidFill>{}</a:ln>"#, emu(s.outline_width * 4.0 / 3.0), c.hex(), if s.kind == ShapeKind::Arrow { r#"<a:tailEnd type="triangle"/>"# } else { "" }),
        None => "<a:ln><a:noFill/></a:ln>".into(),
    };
    let rot = (s.rotation.rem_euclid(360.0) * 60000.0).round() as i64;
    let xfrm = format!("{}{}{}", if rot != 0 { format!(r#" rot="{rot}""#) } else { String::new() }, if s.flip_h { r#" flipH="1""# } else { "" }, if s.flip_v { r#" flipV="1""# } else { "" });
    let text = if s.text.trim().is_empty() { String::new() } else {
        let jc = if s.textbox { "left" } else { "center" };
        let rpr = format!(r#"{}{}<w:color w:val="{}"/><w:sz w:val="{}"/>"#, if s.bold { "<w:b/>" } else { "" }, if s.italic { "<w:i/>" } else { "" }, s.text_color.hex(), (s.font_size * 2.0).round() as i64);
        let paras: String = s.text.split('\n').map(|l| format!(r#"<w:p><w:pPr><w:jc w:val="{jc}"/></w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, xml_text(l))).collect();
        format!("<wps:txbx><w:txbxContent>{paras}</w:txbxContent></wps:txbx>")
    };
    format!(r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="{name}"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr{tb}/><wps:spPr><a:xfrm{xfrm}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="{}"><a:avLst/></a:prstGeom>{fill}{line}</wps:spPr>{text}<wps:bodyPr rot="0" vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="{anchor}" anchorCtr="0"/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing>"#, s.kind.preset(),
        name = { let n = if s.name.is_empty() { format!("{} {id}", if s.textbox { "Text Box" } else { "Shape" }) } else { s.name.clone() }; xml_text(&n) }, tb = if s.textbox { r#" txBox="1""# } else { "" }, anchor = if s.textbox { "t" } else { "ctr" })
}

pub fn parse_shape(wsp: &crate::xml::Element, width: f32, height: f32) -> Option<Shape> {
    let sp = wsp.child("wps:spPr")?;
    let kind = sp.child("a:prstGeom").and_then(|g| g.attr("prst")).and_then(ShapeKind::from_preset)?;
    let color = |e: Option<&crate::xml::Element>| e.and_then(|f| f.find("a:srgbClr")).and_then(|c| c.attr("val")).and_then(Color::from_hex);
    let fill = if sp.child("a:noFill").is_some() { None } else { color(sp.child("a:solidFill")).or((!kind.is_line()).then_some(PALETTE[0])) };
    let ln = sp.child("a:ln");
    let outline = if ln.is_some_and(|l| l.child("a:noFill").is_some()) { None } else { color(ln.and_then(|l| l.child("a:solidFill"))).or(Some(Color(0x2F, 0x52, 0x8F))) };
    let outline_width = ln.and_then(|l| l.attr("w")).and_then(|w| w.parse::<f32>().ok()).map(|w| w / 12700.0).unwrap_or(1.0);
    let mut paras = vec![];
    if let Some(tx) = wsp.find("w:txbxContent") { for p in tx.children_named("w:p") { paras.push(p.text()); } }
    let text_color = wsp.find("w:txbxContent").and_then(|t| t.find("w:color")).and_then(|c| c.attr("w:val")).and_then(Color::from_hex).unwrap_or(Color::WHITE);
    let kind = if kind == ShapeKind::Line && ln.is_some_and(|l| l.child("a:tailEnd").is_some_and(|t| t.attr("type").is_some_and(|v| v != "none"))) { ShapeKind::Arrow } else { kind };
    let xfrm = sp.child("a:xfrm");
    let flag = |a: &str| xfrm.and_then(|x| x.attr(a)).is_some_and(|v| v == "1" || v == "true");
    let rotation = xfrm.and_then(|x| x.attr("rot")).and_then(|r| r.parse::<f32>().ok()).map(|r| r / 60000.0).unwrap_or(0.0);
    let textbox = wsp.child("wps:cNvSpPr").and_then(|c| c.attr("txBox")).is_some_and(|v| v == "1" || v == "true") || wsp.child("wps:bodyPr").and_then(|b| b.attr("anchor")).is_some_and(|a| a == "t");
    let rpr = wsp.find("w:txbxContent").and_then(|t| t.find("w:rPr"));
    let font_size = rpr.and_then(|r| r.child("w:sz")).and_then(|z| z.attr("w:val")).and_then(|v| v.parse::<f32>().ok()).map(|v| v / 2.0).unwrap_or(11.0);
    let on = |tag: &str| rpr.and_then(|r| r.child(tag)).is_some_and(|b| b.attr("w:val").is_none_or(|v| v != "0" && v != "false"));
    Some(Shape { kind, width, height, fill, outline, outline_width, text: paras.join("\n"), text_color, rotation, flip_h: flag("flipH"), flip_v: flag("flipV"), textbox, font_size, bold: on("w:b"), italic: on("w:i"), name: String::new(), raw: None })
}

/// A chart part (§21.2 DrawingML charts) with literal data, so no embedded workbook is needed.
pub fn chart_part_xml(c: &Chart) -> String {
    let cats = |n: usize| -> String {
        let pts: String = c.categories.iter().enumerate().map(|(i, t)| format!(r#"<c:pt idx="{i}"><c:v>{}</c:v></c:pt>"#, xml_text(t))).collect();
        let _ = n;
        format!(r#"<c:cat><c:strLit><c:ptCount val="{}"/>{pts}</c:strLit></c:cat>"#, c.categories.len())
    };
    let series: String = c.series.iter().enumerate().map(|(i, s)| {
        let color = c.color(i);
        let vals: String = s.values.iter().enumerate().map(|(k, v)| format!(r#"<c:pt idx="{k}"><c:v>{v}</c:v></c:pt>"#)).collect();
        let sppr = match c.kind {
            ChartKind::Line => format!(r#"<c:spPr><a:ln w="28575" cap="rnd"><a:solidFill><a:srgbClr val="{}"/></a:solidFill></a:ln></c:spPr><c:marker><c:symbol val="circle"/><c:size val="5"/></c:marker>"#, color.hex()),
            ChartKind::Pie => String::new(),
            _ => format!(r#"<c:spPr><a:solidFill><a:srgbClr val="{}"/></a:solidFill></c:spPr>"#, color.hex()),
        };
        let invert = if matches!(c.kind, ChartKind::Column | ChartKind::Bar) { "<c:invertIfNegative val=\"0\"/>" } else { "" };
        let dpts: String = if c.kind == ChartKind::Pie { (0..s.values.len()).map(|k| format!(r#"<c:dPt><c:idx val="{k}"/><c:bubble3D val="0"/><c:spPr><a:solidFill><a:srgbClr val="{}"/></a:solidFill></c:spPr></c:dPt>"#, PALETTE[k % 6].hex())).collect() } else { String::new() };
        let smooth = if c.kind == ChartKind::Line { "<c:smooth val=\"0\"/>" } else { "" };
        format!(r#"<c:ser><c:idx val="{i}"/><c:order val="{i}"/><c:tx><c:v>{}</c:v></c:tx>{sppr}{invert}{dpts}{}<c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="{}"/>{vals}</c:numLit></c:val>{smooth}</c:ser>"#, xml_text(&s.name), cats(c.categories.len()), s.values.len())
    }).collect();
    let axes = r#"<c:axId val="50010001"/><c:axId val="50010002"/>"#;
    let plot = match c.kind {
        ChartKind::Column | ChartKind::Bar => format!(r#"<c:barChart><c:barDir val="{}"/><c:grouping val="clustered"/><c:varyColors val="0"/>{series}<c:gapWidth val="219"/><c:overlap val="-27"/>{axes}</c:barChart>"#, if c.kind == ChartKind::Bar { "bar" } else { "col" }),
        ChartKind::Line => format!(r#"<c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/>{series}<c:marker val="1"/>{axes}</c:lineChart>"#),
        ChartKind::Area => format!(r#"<c:areaChart><c:grouping val="standard"/><c:varyColors val="0"/>{series}{axes}</c:areaChart>"#),
        ChartKind::Pie => format!(r#"<c:pieChart><c:varyColors val="1"/>{series}<c:firstSliceAng val="0"/></c:pieChart>"#),
    };
    let axis_xml = if c.kind == ChartKind::Pie { String::new() } else {
        let (cat_pos, val_pos) = if c.kind == ChartKind::Bar { ("l", "b") } else { ("b", "l") };
        format!(r#"<c:catAx><c:axId val="50010001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="{cat_pos}"/><c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:crossAx val="50010002"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/><c:noMultiLvlLbl val="0"/></c:catAx><c:valAx><c:axId val="50010002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="{val_pos}"/><c:majorGridlines/><c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:crossAx val="50010001"/><c:crosses val="autoZero"/><c:crossBetween val="between"/></c:valAx>"#)
    };
    let title = if c.title.is_empty() { r#"<c:autoTitleDeleted val="1"/>"#.to_string() } else {
        format!(r#"<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title><c:autoTitleDeleted val="0"/>"#, xml_text(&c.title))
    };
    let legend = if c.legend { r#"<c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend>"# } else { "" };
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:roundedCorners val="0"/><c:chart>{title}<c:plotArea><c:layout/>{plot}{axis_xml}</c:plotArea>{legend}<c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart></c:chartSpace>"#)
}

pub fn chart_drawing_xml(c: &Chart, rid: &str, id: u32) -> String {
    format!(r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{}" cy="{}"/><wp:docPr id="{id}" name="Chart {id}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></wp:inline></w:drawing>"#, emu(c.width), emu(c.height))
}

/// Read a chart part: its type, title, categories and series values (cached or literal).
pub fn parse_chart(root: &crate::xml::Element, width: f32, height: f32) -> Option<Chart> {
    let plot = root.find("c:plotArea")?;
    let (node, kind) = [("c:barChart", ChartKind::Column), ("c:bar3DChart", ChartKind::Column), ("c:lineChart", ChartKind::Line), ("c:line3DChart", ChartKind::Line), ("c:pieChart", ChartKind::Pie), ("c:pie3DChart", ChartKind::Pie), ("c:doughnutChart", ChartKind::Pie), ("c:areaChart", ChartKind::Area)]
        .iter().find_map(|(n, k)| plot.child(n).map(|e| (e, *k)))?;
    let kind = if kind == ChartKind::Column && node.child("c:barDir").and_then(|d| d.attr("val")) == Some("bar") { ChartKind::Bar } else { kind };
    let points = |e: Option<&crate::xml::Element>| -> Vec<String> {
        let mut pts = vec![];
        if let Some(e) = e { e.find_all("c:pt", &mut pts); }
        let mut v: Vec<(usize, String)> = pts.iter().map(|p| (p.attr("idx").and_then(|i| i.parse().ok()).unwrap_or(0), p.child("c:v").map(|v| v.text()).unwrap_or_default())).collect();
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| x.1).collect()
    };
    let sers: Vec<&crate::xml::Element> = node.children_named("c:ser").collect();
    let categories = sers.first().map(|s| points(s.child("c:cat"))).unwrap_or_default();
    let series = sers.iter().enumerate().map(|(i, s)| Series {
        name: s.child("c:tx").map(|t| { let mut v = vec![]; t.find_all("c:v", &mut v); v.first().map(|x| x.text()).unwrap_or_default() }).filter(|n| !n.is_empty()).unwrap_or(format!("Series {}", i + 1)),
        values: points(s.child("c:val")).iter().map(|v| v.parse().unwrap_or(0.0)).collect(),
        color: s.child("c:spPr").and_then(|p| p.find("a:srgbClr")).and_then(|c| c.attr("val")).and_then(Color::from_hex),
    }).collect();
    let title = root.find("c:title").map(|t| { let mut v = vec![]; t.find_all("a:t", &mut v); v.iter().map(|x| x.text()).collect::<String>() }).unwrap_or_default();
    let deleted = root.find("c:autoTitleDeleted").and_then(|d| d.attr("val")).is_some_and(|v| v == "1" || v == "true");
    Some(Chart { kind, title: if deleted { String::new() } else { title }, categories, series, width, height, legend: root.find("c:legend").is_some(), part: None, raw: None })
}

/// A diagram as a PNG picture at twice its size (Word shows it; this app re-reads its spec).
pub fn diagram_png(d: &Diagram) -> Option<Vec<u8>> { object_png(&Object::Diagram(d.clone())) }

/// An object rendered at 2× as PNG (diagram parts, Save as Picture).
pub fn object_png(o: &Object) -> Option<Vec<u8>> {
    let (w, h) = o.size();
    let (w, h) = (w.max(1.0), h.max(1.0));
    let items = o.render(0.0, 0.0);
    let refs: Vec<&DrawItem> = items.iter().collect();
    let svg = crate::svg::items_svg(None, &refs, w, h, 0.0, 0.0);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let (w, h) = ((w * 2.0).ceil() as u32, (h * 2.0).ceil() as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(2.0, 2.0), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}
