//! Shape geometry: the 187 preset shapes (Apache POI's presetShapeDefinitions.xml, as the JS
//! editor uses) and custom geometry, evaluated with their guide formulas (ECMA-376 §20.1.9)
//! into paths in the shape's own EMU space. Arcs become cubic Béziers.

use crate::xml::Element;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pt { pub x: f64, pub y: f64 }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd { Move(Pt), Line(Pt), Cubic(Pt, Pt, Pt), Close }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FillMode { None, #[default] Norm, Lighten, LightenLess, Darken, DarkenLess }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Path { pub cmds: Vec<Cmd>, pub fill: FillMode, pub stroke: bool }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Handle { pub at: Pt, pub x: Option<(String, f64, f64)>, pub y: Option<(String, f64, f64)> }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Geometry {
    pub paths: Vec<Path>,
    /// Where text goes (l, t, r, b) in shape EMU; None means the whole shape.
    pub text_rect: Option<[f64; 4]>,
    /// Adjust handles: position (shape EMU), and for each axis the guide it sets with its
    /// range (x guide, min, max), (y guide, min, max).
    pub handles: Vec<Handle>,
}

const FULL: f64 = 21_600_000.0;

fn presets() -> &'static HashMap<String, Element> {
    static P: OnceLock<HashMap<String, Element>> = OnceLock::new();
    P.get_or_init(|| {
        let xml = include_str!("../assets/presetShapeDefinitions.xml");
        let root = Element::parse(xml).expect("preset definitions");
        root.elements().map(|e| (e.name.clone(), e.clone())).collect()
    })
}

/// Names of all preset shapes.
pub fn preset_names() -> Vec<&'static str> { let mut v: Vec<&str> = presets().keys().map(String::as_str).collect(); v.sort(); v }
pub fn is_preset(name: &str) -> bool { presets().contains_key(name) }

/// The default adjust values of a preset (name → value), for the Format pane.
pub fn preset_adjusts(name: &str) -> Vec<(String, f64)> {
    let Some(def) = presets().get(name) else { return vec![] };
    def.child("avLst").map(|av| av.elements().filter_map(|g| {
        let v = g.attr("fmla")?.strip_prefix("val ")?.trim().parse().ok()?;
        Some((g.attr("name")?.to_string(), v))
    }).collect()).unwrap_or_default()
}

fn builtins(w: f64, h: f64) -> HashMap<String, f64> {
    let ss = w.min(h);
    let cd4 = FULL / 4.0;
    let cd8 = FULL / 8.0;
    [
        ("w", w), ("h", h), ("ss", ss), ("ls", w.max(h)), ("hc", w / 2.0), ("vc", h / 2.0), ("t", 0.0), ("l", 0.0), ("b", h), ("r", w),
        ("wd2", w / 2.0), ("wd3", w / 3.0), ("wd4", w / 4.0), ("wd5", w / 5.0), ("wd6", w / 6.0), ("wd8", w / 8.0), ("wd10", w / 10.0), ("wd12", w / 12.0), ("wd32", w / 32.0),
        ("hd2", h / 2.0), ("hd3", h / 3.0), ("hd4", h / 4.0), ("hd5", h / 5.0), ("hd6", h / 6.0), ("hd8", h / 8.0), ("hd10", h / 10.0),
        ("ssd2", ss / 2.0), ("ssd4", ss / 4.0), ("ssd6", ss / 6.0), ("ssd8", ss / 8.0), ("ssd16", ss / 16.0), ("ssd32", ss / 32.0),
        ("cd2", FULL / 2.0), ("cd4", cd4), ("cd6", FULL / 6.0), ("cd8", cd8), ("3cd4", 3.0 * cd4), ("3cd8", 3.0 * cd8), ("5cd8", 5.0 * cd8), ("7cd8", 7.0 * cd8),
    ].iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn token(t: &str, g: &HashMap<String, f64>) -> f64 {
    t.parse::<f64>().ok().or_else(|| g.get(t).copied()).filter(|v| v.is_finite()).unwrap_or(0.0)
}

fn rad(a: f64) -> f64 { a / 60_000.0 * std::f64::consts::PI / 180.0 }

fn formula(f: &str, g: &HashMap<String, f64>) -> f64 {
    let p: Vec<&str> = f.split_whitespace().collect();
    let a = |i: usize| token(p.get(i + 1).copied().unwrap_or(""), g);
    let div = |n: f64, d: f64| if d == 0.0 { 0.0 } else { n / d };
    match p.first().copied().unwrap_or("") {
        "val" => a(0),
        "*/" => div(a(0) * a(1), a(2)),
        "+-" => a(0) + a(1) - a(2),
        "+/" => div(a(0) + a(1), a(2)),
        "?:" => if a(0) > 0.0 { a(1) } else { a(2) },
        "abs" => a(0).abs(),
        "sqrt" => a(0).max(0.0).sqrt(),
        "max" => a(0).max(a(1)),
        "min" => a(0).min(a(1)),
        "mod" => (a(0) * a(0) + a(1) * a(1) + a(2) * a(2)).sqrt(),
        "pin" => { let (lo, v, hi) = (a(0), a(1), a(2)); if v < lo { lo } else if v > hi { hi } else { v } }
        "sin" => a(0) * rad(a(1)).sin(),
        "cos" => a(0) * rad(a(1)).cos(),
        "tan" => a(0) * rad(a(1)).tan(),
        "at2" => a(1).atan2(a(0)).to_degrees() * 60_000.0,
        "cat2" => a(0) * a(2).atan2(a(1)).cos(),
        "sat2" => a(0) * a(2).atan2(a(1)).sin(),
        _ => 0.0,
    }
}

fn eval_list(list: Option<&Element>, g: &mut HashMap<String, f64>) {
    let Some(list) = list else { return };
    for gd in list.elements().filter(|e| e.local() == "gd") {
        if let (Some(n), Some(f)) = (gd.attr("name"), gd.attr("fmla")) { let v = formula(f, g); g.insert(n.to_string(), v); }
    }
}

/// Evaluate a geometry definition (a preset's, or custGeom) for a w×h shape. `adjusts`
/// overrides avLst values (from the shape's prstGeom/avLst).
fn evaluate(def: &Element, w: f64, h: f64, adjusts: &[(String, f64)]) -> Geometry {
    let mut g = builtins(w, h);
    let find = |n: &str| def.elements().find(|e| e.local() == n);
    eval_list(find("avLst"), &mut g);
    for (k, v) in adjusts { g.insert(k.clone(), *v); }
    eval_list(find("gdLst"), &mut g);
    let mut geo = Geometry::default();
    if let Some(r) = find("rect") {
        let t = |k: &str| token(r.attr(k).unwrap_or("0"), &g);
        geo.text_rect = Some([t("l"), t("t"), t("r"), t("b")]);
    }
    if let Some(ah) = find("ahLst") {
        for h in ah.elements() {
            if let Some(pos) = h.elements().find(|e| e.local() == "pos") {
                let axis = |r: &str, mn: &str, mx: &str| h.attr(r).map(|n| (n.to_string(), token(h.attr(mn).unwrap_or("0"), &g), token(h.attr(mx).unwrap_or("100000"), &g)));
                let (x, y) = if h.local() == "ahPolar" { (axis("gdRefR", "minR", "maxR"), axis("gdRefAng", "minAng", "maxAng")) } else { (axis("gdRefX", "minX", "maxX"), axis("gdRefY", "minY", "maxY")) };
                geo.handles.push(Handle { at: Pt { x: token(pos.attr("x").unwrap_or("0"), &g), y: token(pos.attr("y").unwrap_or("0"), &g) }, x, y });
            }
        }
    }
    let Some(list) = find("pathLst") else { return geo };
    for p in list.elements().filter(|e| e.local() == "path") {
        let pw = p.attr_f64("w").filter(|v| *v > 0.0);
        let ph = p.attr_f64("h").filter(|v| *v > 0.0);
        let (sx, sy) = (pw.map(|pw| w / pw).unwrap_or(1.0), ph.map(|ph| h / ph).unwrap_or(1.0));
        let pt = |e: &Element| -> Pt {
            let p = e.elements().find(|c| c.local() == "pt").unwrap_or(e);
            Pt { x: token(p.attr("x").unwrap_or("0"), &g) * sx, y: token(p.attr("y").unwrap_or("0"), &g) * sy }
        };
        let fill = match p.attr("fill") { Some("none") => FillMode::None, Some("lighten") => FillMode::Lighten, Some("lightenLess") => FillMode::LightenLess, Some("darken") => FillMode::Darken, Some("darkenLess") => FillMode::DarkenLess, _ => FillMode::Norm };
        let mut path = Path { cmds: vec![], fill, stroke: p.attr("stroke").map(|s| s != "0" && s != "false").unwrap_or(true) };
        let mut cur = Pt { x: 0.0, y: 0.0 };
        let mut start = cur;
        for c in p.elements() {
            match c.local() {
                "moveTo" => { cur = pt(c); start = cur; path.cmds.push(Cmd::Move(cur)); }
                "lnTo" => { cur = pt(c); path.cmds.push(Cmd::Line(cur)); }
                "cubicBezTo" => {
                    let pts: Vec<Pt> = c.elements().filter(|e| e.local() == "pt").map(pt).collect();
                    if pts.len() == 3 { path.cmds.push(Cmd::Cubic(pts[0], pts[1], pts[2])); cur = pts[2]; }
                }
                "quadBezTo" => {
                    let pts: Vec<Pt> = c.elements().filter(|e| e.local() == "pt").map(pt).collect();
                    if pts.len() == 2 {
                        let (q, e) = (pts[0], pts[1]);
                        let c1 = Pt { x: cur.x + 2.0 / 3.0 * (q.x - cur.x), y: cur.y + 2.0 / 3.0 * (q.y - cur.y) };
                        let c2 = Pt { x: e.x + 2.0 / 3.0 * (q.x - e.x), y: e.y + 2.0 / 3.0 * (q.y - e.y) };
                        path.cmds.push(Cmd::Cubic(c1, c2, e));
                        cur = e;
                    }
                }
                "arcTo" => {
                    let t = |k: &str| token(c.attr(k).unwrap_or("0"), &g);
                    cur = arc(&mut path.cmds, cur, t("wR") * sx, t("hR") * sy, t("stAng"), t("swAng"));
                }
                "close" => { path.cmds.push(Cmd::Close); cur = start; }
                _ => {}
            }
        }
        geo.paths.push(path);
    }
    geo
}

/// DrawingML arcTo from the current point (geometry-arc.ts), as cubic Béziers.
fn arc(out: &mut Vec<Cmd>, from: Pt, wr: f64, hr: f64, st: f64, sw: f64) -> Pt {
    if sw == 0.0 || (wr == 0.0 && hr == 0.0) { return from; }
    let sweep = sw.clamp(-FULL, FULL);
    let collapsed = wr == 0.0 || hr == 0.0;
    let angle = |a: f64| (a % FULL) / FULL * std::f64::consts::TAU;
    let param = |a: f64| { let th = angle(a); if collapsed { th } else { (wr * th.sin()).atan2(hr * th.cos()) } };
    let start = param(st);
    let mut span = param((st % FULL) + sweep) - start;
    let full = sweep.abs() == FULL;
    if full { span = sweep.signum() * std::f64::consts::TAU; }
    else if collapsed { span = sweep / FULL * std::f64::consts::TAU; }
    else if sweep > 0.0 && span < 0.0 { span += std::f64::consts::TAU; }
    else if sweep < 0.0 && span > 0.0 { span -= std::f64::consts::TAU; }
    if span == 0.0 { return from; }
    let (cx, cy) = (from.x - wr * start.cos(), from.y - hr * start.sin());
    let at = |t: f64| Pt { x: cx + wr * t.cos(), y: cy + hr * t.sin() };
    let n = (span.abs() / (std::f64::consts::PI / 2.0)).ceil().max(1.0) as usize;
    let step = span / n as f64;
    let mut t0 = start;
    for i in 0..n {
        let t1 = if i == n - 1 { start + span } else { t0 + step };
        let k = 4.0 / 3.0 * ((t1 - t0) / 4.0).tan();
        let (p0, p3) = (at(t0), at(t1));
        let c1 = Pt { x: p0.x - k * wr * t0.sin(), y: p0.y + k * hr * t0.cos() };
        let c2 = Pt { x: p3.x + k * wr * t1.sin(), y: p3.y - k * hr * t1.cos() };
        out.push(Cmd::Cubic(c1, c2, if full && i == n - 1 { from } else { p3 }));
        t0 = t1;
    }
    if full { from } else { at(start + span) }
}

/// A shape's geometry from its spPr (prstGeom or custGeom); a rectangle when it has neither.
pub fn shape_geometry(sp_pr: Option<&Element>, w: f64, h: f64) -> Geometry {
    if let Some(c) = sp_pr.and_then(|s| s.child("a:custGeom")) { return evaluate(c, w, h, &[]); }
    let (name, adj) = match sp_pr.and_then(|s| s.child("a:prstGeom")) {
        Some(p) => {
            let adj: Vec<(String, f64)> = p.child("a:avLst").map(|av| av.elements().filter_map(|g| Some((g.attr("name")?.to_string(), formula(g.attr("fmla")?, &HashMap::new())))).collect()).unwrap_or_default();
            (p.attr("prst").unwrap_or("rect").to_string(), adj)
        }
        None => ("rect".to_string(), vec![]),
    };
    preset(&name, w, h, &adj)
}

pub fn preset(name: &str, w: f64, h: f64, adjusts: &[(String, f64)]) -> Geometry {
    match presets().get(name).or_else(|| presets().get("rect")) { Some(def) => evaluate(def, w, h, adjusts), None => Geometry::default() }
}

/// Points along a path (curves flattened), for hit testing and rendering.
pub fn flatten(cmds: &[Cmd], tolerance_steps: usize) -> Vec<Vec<Pt>> {
    let mut out: Vec<Vec<Pt>> = vec![];
    let mut cur = Pt { x: 0.0, y: 0.0 };
    for c in cmds {
        match *c {
            Cmd::Move(p) => { out.push(vec![p]); cur = p; }
            Cmd::Line(p) => { if out.is_empty() { out.push(vec![cur]); } out.last_mut().unwrap().push(p); cur = p; }
            Cmd::Cubic(a, b, e) => {
                if out.is_empty() { out.push(vec![cur]); }
                let poly = out.last_mut().unwrap();
                for i in 1..=tolerance_steps {
                    let t = i as f64 / tolerance_steps as f64;
                    let mt = 1.0 - t;
                    poly.push(Pt {
                        x: mt * mt * mt * cur.x + 3.0 * mt * mt * t * a.x + 3.0 * mt * t * t * b.x + t * t * t * e.x,
                        y: mt * mt * mt * cur.y + 3.0 * mt * mt * t * a.y + 3.0 * mt * t * t * b.y + t * t * t * e.y,
                    });
                }
                cur = e;
            }
            Cmd::Close => { if let Some(first) = out.last().and_then(|p| p.first().copied()) { out.last_mut().unwrap().push(first); cur = first; } }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_load_and_evaluate() {
        assert!(preset_names().len() >= 180);
        let r = preset("rect", 100.0, 50.0, &[]);
        assert_eq!(r.paths.len(), 1);
        assert_eq!(r.paths[0].cmds[2], Cmd::Line(Pt { x: 100.0, y: 50.0 }));
        // Every preset evaluates to finite points.
        for n in preset_names() {
            let g = preset(n, 1000.0, 600.0, &[]);
            for p in &g.paths { for c in &p.cmds { if let Cmd::Line(p) | Cmd::Move(p) = c { assert!(p.x.is_finite() && p.y.is_finite(), "{n}"); } } }
        }
        // An ellipse is four quarter arcs ending where it started.
        let e = preset("ellipse", 200.0, 100.0, &[]);
        let pts = flatten(&e.paths[0].cmds, 8);
        let (a, b) = (pts[0][0], *pts[0].last().unwrap());
        assert!((a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6);
        assert!(pts[0].iter().all(|p| p.x >= -1e-6 && p.x <= 200.0 + 1e-6));
        assert!(preset("roundRect", 100.0, 100.0, &[]).text_rect.is_some());
    }
}
