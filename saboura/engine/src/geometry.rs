//! Board geometry: rotation, bounds, hit testing, connector paths, shape outlines and zoom
//! (`geometry.js`, `model.js`, `connectors.js`, `shapes.js`).

use crate::model::{Item, MAX_ZOOM, MIN_ZOOM};

pub type P = (f64, f64);

const ROTATABLE: [&str; 7] = ["text", "rect", "ellipse", "image", "link", "file", "pen"];

pub fn can_rotate(i: &Item) -> bool { ROTATABLE.contains(&i.kind.as_str()) && !i.extra.contains_key("mindMap") }
pub fn normalize_rotation(a: f64) -> f64 { ((a + 180.0) % 360.0 + 360.0) % 360.0 - 180.0 }
pub fn center(i: &Item) -> P { (i.x + i.w / 2.0, i.y + i.h / 2.0) }

pub fn rotate_point(p: P, c: P, degrees: f64) -> P {
    if degrees == 0.0 { return p; }
    let (s, co) = degrees.to_radians().sin_cos();
    let (x, y) = (p.0 - c.0, p.1 - c.1);
    (c.0 + x * co - y * s, c.1 + x * s + y * co)
}
/// A board point in the object's unrotated frame.
pub fn local_point(i: &Item, p: P) -> P { rotate_point(p, center(i), -i.rotation()) }
pub fn world_point(i: &Item, p: P) -> P { rotate_point(p, center(i), i.rotation()) }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box2 { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }

impl Box2 {
    pub fn contains(&self, p: P) -> bool { p.0 >= self.x && p.0 <= self.x + self.w && p.1 >= self.y && p.1 <= self.y + self.h }
    pub fn intersects(&self, o: &Box2) -> bool { self.x <= o.x + o.w && o.x <= self.x + self.w && self.y <= o.y + o.h && o.y <= self.y + self.h }
    pub fn within(&self, o: &Box2) -> bool { self.x >= o.x && self.y >= o.y && self.x + self.w <= o.x + o.w && self.y + self.h <= o.y + o.h }
    pub fn from_points(a: P, b: P) -> Box2 { Box2 { x: a.0.min(b.0), y: a.1.min(b.1), w: (a.0 - b.0).abs(), h: (a.1 - b.1).abs() } }
}

/// The box an object covers, rotation included (`objectBounds`).
pub fn object_bounds(i: &Item) -> Box2 {
    if i.kind == "pen" && let Some(pts) = &i.points && i.rotation() == 0.0 && !pts.is_empty() {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in pts { x0 = x0.min(p[0]); y0 = y0.min(p[1]); x1 = x1.max(p[0]); y1 = y1.max(p[1]); }
        return Box2 { x: i.x + x0, y: i.y + y0, w: x1 - x0, h: y1 - y0 };
    }
    let r = i.rotation();
    let corners = [(i.x, i.y), (i.x + i.w, i.y), (i.x + i.w, i.y + i.h), (i.x, i.y + i.h)].map(|p| rotate_point(p, center(i), r));
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in corners { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); }
    Box2 { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

/// The box around objects, or a default page when there are none (`bounds`).
pub fn bounds<'a>(items: impl IntoIterator<Item = &'a Item>) -> Box2 {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut any = false;
    for i in items {
        any = true;
        let b = object_bounds(i);
        x0 = x0.min(b.x); y0 = y0.min(b.y); x1 = x1.max(b.x + b.w); y1 = y1.max(b.y + b.h);
    }
    if !any { return Box2 { x: 0.0, y: 0.0, w: 900.0, h: 600.0 }; }
    Box2 { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

pub fn segment_distance(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx * dx + dy * dy;
    let t = if len > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

/// Objects in drawing order: frames behind everything else (`stackedItems`), leaving out
/// branches of collapsed mind-map nodes.
pub fn stacked(items: &[Item]) -> std::vec::IntoIter<&Item> {
    let hidden = if items.iter().any(|i| i.flag("mindCollapsed")) { crate::arrange::collapsed_hidden(items) } else { Default::default() };
    let shown = |i: &&Item| !hidden.contains(&i.id);
    let v: Vec<&Item> = items.iter().filter(|i| i.kind == "frame").filter(shown).chain(items.iter().filter(|i| i.kind != "frame").filter(shown)).collect();
    v.into_iter()
}

/// The shapes this port draws, with their names (`SHAPES` in shapes.js). "rectangle" and
/// "ellipse" are the plain rect and ellipse objects; the rest are rects with a `shape`.
pub const SHAPES: [(&str, &str); 36] = [
    ("rectangle", "Rectangle"), ("terminator", "Rounded rectangle"), ("ellipse", "Ellipse"), ("diamond", "Diamond"),
    ("triangle", "Triangle"), ("star", "Star"), ("hexagon", "Hexagon"), ("parallelogram", "Parallelogram"),
    ("cylinder", "Cylinder"), ("document", "Document"), ("predefined-process", "Predefined process"),
    ("manual-input", "Manual input"), ("manual-operation", "Manual operation"), ("delay", "Delay"),
    ("display", "Display"), ("off-page", "Off-page connector"), ("merge", "Merge"), ("internal-storage", "Internal storage"),
    ("or", "Or"), ("summing-junction", "Summing junction"), ("uml-note", "Note"), ("data-store", "Data store"),
    ("start-event", "Start event"), ("intermediate-event", "Intermediate event"), ("end-event", "End event"), ("bpmn-task", "Task"),
    ("gateway-exclusive", "Exclusive gateway"), ("gateway-parallel", "Parallel gateway"), ("uml-class", "Class"), ("uml-package", "Package"),
    ("cloud", "Cloud"), ("browser", "Browser window"), ("mobile", "Mobile device"), ("wf-button", "Button"), ("wf-input", "Text input"), ("wf-checkbox", "Checkbox"),
];

fn arc(cx: f64, cy: f64, rx: f64, ry: f64, a0: f64, a1: f64, n: usize) -> impl Iterator<Item = P> {
    (0..=n).map(move |k| { let a = (a0 + (a1 - a0) * k as f64 / n as f64).to_radians(); (cx + rx * a.cos(), cy + ry * a.sin()) })
}

/// A rounded rectangle's outline from (0, 0) to (w, h).
pub fn round_rect(w: f64, h: f64, r: f64) -> Vec<P> {
    let r = r.min(w.abs() / 2.0).min(h.abs() / 2.0).max(0.0);
    let mut v = Vec::new();
    for (cx, cy, a0) in [(w - r, r, -90.0), (w - r, h - r, 0.0), (r, h - r, 90.0), (r, r, 180.0)] { v.extend(arc(cx, cy, r, r, a0, a0 + 90.0, 6)); }
    v
}

/// A shape's outline in its own frame, (0, 0) to (w, h) (`shapePath`). Unknown keys are a
/// rounded rectangle, as in the JS.
pub fn shape_outline(key: &str, w: f64, h: f64) -> Vec<P> {
    let poly = |pts: &[(f64, f64)]| pts.iter().map(|(x, y)| (x * w, y * h)).collect::<Vec<P>>();
    match key {
        "ellipse" | "or" | "summing-junction" | "start-event" | "intermediate-event" | "end-event" | "wf-checkbox-x" => arc(w / 2.0, h / 2.0, w / 2.0, h / 2.0, 0.0, 360.0, 72).take(72).collect(),
        "gateway-exclusive" | "gateway-parallel" | "diamond" => poly(&[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)]),
        "triangle" => poly(&[(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)]),
        "parallelogram" => poly(&[(0.2, 0.0), (1.0, 0.0), (0.8, 1.0), (0.0, 1.0)]),
        "hexagon" => poly(&[(0.25, 0.0), (0.75, 0.0), (1.0, 0.5), (0.75, 1.0), (0.25, 1.0), (0.0, 0.5)]),
        "manual-input" => poly(&[(0.0, 0.25), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
        "manual-operation" => poly(&[(0.0, 0.0), (1.0, 0.0), (0.8, 1.0), (0.2, 1.0)]),
        "off-page" => poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 0.65), (0.5, 1.0), (0.0, 0.65)]),
        "merge" => poly(&[(0.0, 0.0), (1.0, 0.0), (0.5, 1.0)]),
        "uml-note" => { let f = (w.min(h) * 0.22).min(24.0); vec![(0.0, 0.0), (w - f, 0.0), (w, f), (w, h), (0.0, h)] }
        "star" => (0..10).map(|n| {
            let a = -std::f64::consts::FRAC_PI_2 + n as f64 * std::f64::consts::PI / 5.0;
            let r = if n % 2 == 1 { 0.22 } else { 0.5 };
            ((0.5 + r * a.cos()) * w, (0.5 + r * a.sin()) * h)
        }).collect(),
        "terminator" | "wf-button" => round_rect(w, h, w.min(h) / 2.0),
        "bpmn-task" | "mobile" => round_rect(w, h, (w.min(h) * if key == "mobile" { 0.16 } else { 0.12 }).min(18.0)),
        "wf-input" | "browser" | "uml-class" | "wf-checkbox" => round_rect(w, h, 4.0),
        "uml-package" => { let (tw, th) = (w * 0.4, (h * 0.18).min(24.0)); vec![(0.0, 0.0), (tw, 0.0), (tw, th), (w, th), (w, h), (0.0, h)] }
        "cloud" => (0..96).map(|k| {
            let a = k as f64 / 96.0 * std::f64::consts::TAU;
            let r = 1.0 + 0.09 * (a * 7.0).cos().abs();
            (w / 2.0 + w / 2.0 / 1.09 * r * a.cos(), h / 2.0 + h / 2.0 / 1.09 * r * a.sin())
        }).collect(),
        "cylinder" => { let ry = h * 0.12; arc(w / 2.0, ry, w / 2.0, ry, 180.0, 360.0, 24).chain(arc(w / 2.0, h - ry, w / 2.0, ry, 0.0, 180.0, 24)).collect() }
        "data-store" => { let rx = w * 0.1; arc(rx, h / 2.0, rx, h / 2.0, 90.0, 270.0, 16).chain([(w, 0.0), (w, h)]).collect() }
        "delay" => { let r = (h / 2.0).min(w / 2.0); [(0.0, 0.0)].into_iter().chain(arc(w - r, h / 2.0, r, h / 2.0, -90.0, 90.0, 24)).chain([(0.0, h)]).collect() }
        "display" => { let r = w * 0.15; [(r, 0.0)].into_iter().chain(arc(w - r, h / 2.0, r, h / 2.0, -90.0, 90.0, 24)).chain([(r, h), (0.0, h / 2.0)]).collect() }
        "document" => {
            let mut v = vec![(0.0, 0.0), (w, 0.0)];
            v.extend((0..=24).map(|k| { let t = 1.0 - k as f64 / 24.0; (w * t, h * 0.88 + h * 0.08 * (t * std::f64::consts::TAU).sin()) }));
            v
        }
        _ => round_rect(w, h, 10.0),
    }
}

/// Inner lines some shapes draw over their fill (`shapeDetail`).
pub fn shape_details(key: &str, w: f64, h: f64) -> Vec<Vec<P>> {
    match key {
        "cylinder" => vec![arc(w / 2.0, h * 0.12, w / 2.0, h * 0.12, 0.0, 180.0, 24).collect()],
        "predefined-process" => { let d = (w * 0.1).min(16.0); vec![vec![(d, 0.0), (d, h)], vec![(w - d, 0.0), (w - d, h)]] }
        "internal-storage" => { let d = (w.min(h) * 0.18).min(18.0); vec![vec![(d, 0.0), (d, h)], vec![(0.0, d), (w, d)]] }
        "summing-junction" => { let k = std::f64::consts::FRAC_1_SQRT_2 / 2.0; vec![vec![(w * (0.5 - k), h * (0.5 - k)), (w * (0.5 + k), h * (0.5 + k))], vec![(w * (0.5 + k), h * (0.5 - k)), (w * (0.5 - k), h * (0.5 + k))]] }
        "or" => vec![vec![(w / 2.0, 0.0), (w / 2.0, h)], vec![(0.0, h / 2.0), (w, h / 2.0)]],
        "intermediate-event" => vec![arc(w / 2.0, h / 2.0, w / 2.0 - 5.0, h / 2.0 - 5.0, 0.0, 360.0, 64).collect()],
        "end-event" => vec![arc(w / 2.0, h / 2.0, w / 2.0 - 2.0, h / 2.0 - 2.0, 0.0, 360.0, 64).collect(), arc(w / 2.0, h / 2.0, w / 2.0 - 3.5, h / 2.0 - 3.5, 0.0, 360.0, 64).collect()],
        "gateway-exclusive" => vec![vec![(w * 0.36, h * 0.36), (w * 0.64, h * 0.64)], vec![(w * 0.64, h * 0.36), (w * 0.36, h * 0.64)]],
        "gateway-parallel" => vec![vec![(w * 0.5, h * 0.3), (w * 0.5, h * 0.7)], vec![(w * 0.3, h * 0.5), (w * 0.7, h * 0.5)]],
        "uml-class" => vec![vec![(0.0, (h * 0.28).min(36.0)), (w, (h * 0.28).min(36.0))], vec![(0.0, h * 0.64), (w, h * 0.64)]],
        "browser" => { let b = (h * 0.16).min(26.0); vec![vec![(0.0, b), (w, b)], arc(b * 0.6, b / 2.0, 3.0, 3.0, 0.0, 360.0, 12).collect(), arc(b * 1.1, b / 2.0, 3.0, 3.0, 0.0, 360.0, 12).collect()] }
        "mobile" => { let t = (h * 0.06).min(20.0); vec![vec![(w * 0.38, t), (w * 0.62, t)], arc(w / 2.0, h - t, t * 0.45, t * 0.45, 0.0, 360.0, 16).collect()] }
        "wf-checkbox" => vec![vec![(w * 0.25, h * 0.5), (w * 0.42, h * 0.68), (w * 0.76, h * 0.3)]],
        "uml-note" => { let f = (w.min(h) * 0.22).min(24.0); vec![vec![(w - f, 0.0), (w - f, f), (w, f)]] }
        _ => Vec::new(),
    }
}

/// The outline of a rect, ellipse or shaped rect object, in its own frame.
pub fn item_outline(i: &Item) -> Vec<P> {
    let key = if i.kind == "ellipse" { "ellipse" } else { i.shape.as_deref().unwrap_or("rectangle") };
    shape_outline(key, i.w, i.h)
}

pub fn polygon_contains(poly: &[P], p: P) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 { inside = !inside; }
        j = k;
    }
    inside
}

fn is_ellipse(i: &Item) -> bool { i.kind == "ellipse" || i.shape.as_deref() == Some("ellipse") }

/// Whether a point in the object's own frame (0..w, 0..h) lies inside its outline.
pub fn shape_contains(i: &Item, x: f64, y: f64) -> bool {
    if is_ellipse(i) {
        return ((x - i.w / 2.0) / (i.w / 2.0)).powi(2) + ((y - i.h / 2.0) / (i.h / 2.0)).powi(2) <= 1.0;
    }
    if x < 0.0 || y < 0.0 || x > i.w || y > i.h { return false; }
    match i.shape.as_deref() {
        None | Some("rectangle") => true,
        Some(_) => polygon_contains(&item_outline(i), (x, y)),
    }
}

/// Where a line from an object's centre toward `toward` leaves its outline (`shapeAnchor`).
pub fn anchor(i: &Item, toward: P) -> P {
    let (cx, cy) = center(i);
    let (dx, dy) = (toward.0 - cx, toward.1 - cy);
    if dx == 0.0 && dy == 0.0 || i.w <= 0.0 || i.h <= 0.0 { return (cx, cy); }
    let (mut lo, mut hi) = (0.0, 1.0 / (dx.abs() / (i.w / 2.0)).max(dy.abs() / (i.h / 2.0)));
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if shape_contains(i, i.w / 2.0 + dx * mid, i.h / 2.0 + dy * mid) { lo = mid; } else { hi = mid; }
    }
    (cx + dx * lo, cy + dy * lo)
}

/// A connection line's control points: two for straight, four for elbow and curved
/// (`connectorPath`).
pub fn connector_path(i: &Item) -> Vec<P> {
    let (a, b) = ((i.x, i.y), (i.x + i.w, i.y + i.h));
    match i.line_type.as_deref() {
        None | Some("straight") => vec![a, b],
        _ => {
            let horizontal = match i.extra.get("routeAxis").and_then(|v| v.as_str()) { Some(ax) => ax == "x", None => i.w.abs() >= i.h.abs() };
            if horizontal { vec![a, (a.0 + i.w / 2.0, a.1), (a.0 + i.w / 2.0, b.1), b] }
            else { vec![a, (a.0, a.1 + i.h / 2.0), (b.0, a.1 + i.h / 2.0), b] }
        }
    }
}

/// The polyline a connection line is drawn and tested along; curves are flattened.
pub fn connector_segments(i: &Item) -> Vec<P> {
    let path = connector_path(i);
    if i.line_type.as_deref() != Some("curved") { return path; }
    let (a, b, c, d) = (path[0], path[1], path[2], path[3]);
    (0..=32).map(|n| {
        let t = n as f64 / 32.0;
        let u = 1.0 - t;
        let (k0, k1, k2, k3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        (k0 * a.0 + k1 * b.0 + k2 * c.0 + k3 * d.0, k0 * a.1 + k1 * b.1 + k2 * c.1 + k3 * d.1)
    }).collect()
}

/// The topmost object at a board point (`hitTest`). `tolerance` is in board units.
pub fn hit_test(items: &[Item], p: P, tolerance: f64) -> Option<&Item> {
    stacked(items).rev().find(|i| hits(i, p, tolerance))
}

pub fn hits(i: &Item, p: P, tolerance: f64) -> bool {
    let lp = local_point(i, p);
    let (x, y) = (lp.0 - i.x, lp.1 - i.y);
    match i.kind.as_str() {
        "arrow" => {
            let margin = tolerance + i.line_width.unwrap_or(2.5) / 2.0;
            connector_segments(i).windows(2).any(|s| segment_distance(p, s[0], s[1]) <= margin)
        }
        "pen" => {
            let Some(pts) = &i.points else { return false };
            let margin = tolerance + i.line_width.unwrap_or(3.0) / 2.0;
            if pts.len() == 1 { return (x - pts[0][0]).hypot(y - pts[0][1]) <= margin; }
            pts.windows(2).any(|s| segment_distance((x, y), (s[0][0], s[0][1]), (s[1][0], s[1][1])) <= margin)
        }
        // A frame is picked by its title bar and edges, so the objects in it stay reachable.
        "frame" => x >= -tolerance && y >= -tolerance && x <= i.w + tolerance && y <= i.h + tolerance
            && (y < 45.0 || x < tolerance || x > i.w - tolerance || y > i.h - tolerance),
        "rect" | "ellipse" => shape_contains(i, x, y),
        _ => x >= 0.0 && y >= 0.0 && x <= i.w && y <= i.h,
    }
}

/// Resize handles as fractions of the selection box.
pub const HANDLES: [(&str, f64, f64); 8] = [
    ("se", 1.0, 1.0), ("nw", 0.0, 0.0), ("ne", 1.0, 0.0), ("sw", 0.0, 1.0),
    ("e", 1.0, 0.5), ("w", 0.0, 0.5), ("n", 0.5, 0.0), ("s", 0.5, 1.0),
];

/// The box `start` becomes when its `handle` is dragged by (dx, dy). Shift keeps its
/// proportions. Width and height stay at least `min`.
pub fn resized(start: Box2, handle: &str, dx: f64, dy: f64, keep_ratio: bool, min: f64) -> Box2 {
    let (mut x0, mut y0, mut x1, mut y1) = (start.x, start.y, start.x + start.w, start.y + start.h);
    if handle.contains('w') { x0 = (x0 + dx).min(x1 - min); }
    if handle.contains('e') { x1 = (x1 + dx).max(x0 + min); }
    if handle.contains('n') { y0 = (y0 + dy).min(y1 - min); }
    if handle.contains('s') { y1 = (y1 + dy).max(y0 + min); }
    if keep_ratio && handle.len() == 2 && start.w > 0.0 && start.h > 0.0 {
        let s = ((x1 - x0) / start.w).max((y1 - y0) / start.h);
        let (w, h) = (start.w * s, start.h * s);
        if handle.contains('w') { x0 = x1 - w; } else { x1 = x0 + w; }
        if handle.contains('n') { y0 = y1 - h; } else { y1 = y0 + h; }
    }
    Box2 { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

/// Moves and scales an object from box `from` into box `to`.
pub fn fit_into(i: &mut Item, orig: &Item, from: Box2, to: Box2) {
    let sx = if from.w > 0.0 { to.w / from.w } else { 1.0 };
    let sy = if from.h > 0.0 { to.h / from.h } else { 1.0 };
    i.x = to.x + (orig.x - from.x) * sx;
    i.y = to.y + (orig.y - from.y) * sy;
    i.w = orig.w * sx;
    i.h = orig.h * sy;
    if let (Some(pts), Some(op)) = (&mut i.points, &orig.points) {
        for (p, o) in pts.iter_mut().zip(op) { *p = [o[0] * sx, o[1] * sy]; }
    }
}

/// The pan and zoom of the view: a board point p shows at p * z + (x, y).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View { pub x: f64, pub y: f64, pub z: f64 }

impl Default for View { fn default() -> View { View { x: 0.0, y: 0.0, z: 1.0 } } }

impl View {
    pub fn to_screen(&self, p: P) -> P { (p.0 * self.z + self.x, p.1 * self.z + self.y) }
    pub fn to_board(&self, s: P) -> P { ((s.0 - self.x) / self.z, (s.1 - self.y) / self.z) }
    /// Zooms to `next`, keeping the board point under screen point `at` in place (`zoomAt`).
    pub fn zoom_at(&self, at: P, next: f64) -> View {
        let next = next.clamp(MIN_ZOOM, MAX_ZOOM);
        View { x: at.0 - (at.0 - self.x) * next / self.z, y: at.1 - (at.1 - self.y) * next / self.z, z: next }
    }
    /// The view showing `b` whole in a screen area `size`, with a margin.
    pub fn fit(b: Box2, size: P, margin: f64) -> View {
        let z = ((size.0 - margin * 2.0) / b.w.max(1.0)).min((size.1 - margin * 2.0) / b.h.max(1.0)).clamp(MIN_ZOOM, 1.0);
        View { x: size.0 / 2.0 - (b.x + b.w / 2.0) * z, y: size.1 / 2.0 - (b.y + b.h / 2.0) * z, z }
    }
}

/// One zoom button step (`zoomStep`): ×1.2, snapping to 100% when close.
pub fn zoom_step(z: f64, larger: bool) -> f64 {
    let next = (z * if larger { 1.2 } else { 1.0 / 1.2 }).clamp(MIN_ZOOM, MAX_ZOOM);
    if (next - 1.0).abs() < 0.01 { 1.0 } else { next }
}
