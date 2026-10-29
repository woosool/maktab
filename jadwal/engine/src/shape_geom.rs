//! Preset shape geometry for floating shapes: the DrawingML presets (Sard's `geom`), evaluated
//! for a shape's rectangle and adjust values and flattened into polygons the grid painter and
//! the print renderer both draw.

use crate::drawing::Shape;
use slides_engine::geom::{self, Cmd, FillMode};

/// One drawn piece: its points (sheet or screen units), whether it closes, fills, strokes, and
/// a shade for darkened/lightened faces (1.0 = the fill as is).
#[derive(Clone, Debug)]
pub struct Piece { pub points: Vec<(f32, f32)>, pub closed: bool, pub fill: bool, pub stroke: bool, pub shade: f32 }

/// Whether a preset name is one the geometry library knows.
pub fn known(preset: &str) -> bool { geom::is_preset(preset) }

/// The default adjust values of a preset.
pub fn default_adjusts(preset: &str) -> Vec<(String, f64)> { geom::preset_adjusts(preset) }

/// A shape's pieces in the rectangle r = [x0, y0, x1, y1].
pub fn pieces(s: &Shape, r: [f32; 4]) -> Vec<Piece> {
    let name = if s.preset.is_empty() { "rect" } else { s.preset.as_str() };
    let (w, h) = ((r[2] - r[0]).max(0.0) as f64, (r[3] - r[1]).max(0.0) as f64);
    let g = geom::preset(name, w.max(0.01), h.max(0.01), &s.adjust);
    let mut out = vec![];
    for p in &g.paths {
        // Split the commands at each Move so every sub-path knows whether it closes.
        let mut start = 0;
        let cmds = &p.cmds;
        let mut bounds = vec![];
        for (i, c) in cmds.iter().enumerate() { if matches!(c, Cmd::Move(_)) && i > start { bounds.push((start, i)); start = i; } }
        if start < cmds.len() { bounds.push((start, cmds.len())); }
        for (a, b) in bounds {
            let part = &cmds[a..b];
            let closed = part.iter().any(|c| matches!(c, Cmd::Close));
            for poly in geom::flatten(part, 12) {
                if poly.len() < 2 { continue; }
                let pts: Vec<(f32, f32)> = poly.iter().map(|q| (r[0] + q.x as f32, r[1] + q.y as f32)).collect();
                let shade = match p.fill { FillMode::Darken => 0.6, FillMode::DarkenLess => 0.8, FillMode::Lighten => 1.4, FillMode::LightenLess => 1.2, _ => 1.0 };
                out.push(Piece { points: pts, closed, fill: closed && p.fill != FillMode::None, stroke: p.stroke, shade });
            }
        }
    }
    out
}

/// A color darkened or lightened by a piece's shade.
pub fn shaded(c: [u8; 3], k: f32) -> [u8; 3] {
    if k <= 1.0 { c.map(|x| (x as f32 * k).round() as u8) } else { let t = k - 1.0; c.map(|x| (x as f32 + (255.0 - x as f32) * t).round().min(255.0) as u8) }
}

/// Where a shape's text goes: the preset's text rectangle, or the whole shape.
pub fn text_rect(s: &Shape, r: [f32; 4]) -> [f32; 4] {
    let name = if s.preset.is_empty() { "rect" } else { s.preset.as_str() };
    let (w, h) = ((r[2] - r[0]).max(0.01) as f64, (r[3] - r[1]).max(0.01) as f64);
    match geom::preset(name, w, h, &s.adjust).text_rect {
        Some(t) => [r[0] + t[0] as f32, r[1] + t[1] as f32, r[0] + t[2] as f32, r[1] + t[3] as f32],
        None => r,
    }
}

/// The adjust handles: (position in r, guide x with range, guide y with range).
pub fn handles(s: &Shape, r: [f32; 4]) -> Vec<geom::Handle> {
    let name = if s.preset.is_empty() { "rect" } else { s.preset.as_str() };
    let (w, h) = ((r[2] - r[0]).max(0.01) as f64, (r[3] - r[1]).max(0.01) as f64);
    let mut v = geom::preset(name, w, h, &s.adjust).handles;
    for hd in v.iter_mut() { hd.at.x += r[0] as f64; hd.at.y += r[1] as f64; }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{add_object, Anchor, ObjKind, Object};

    #[test]
    fn presets_adjusts_and_effects_round_trip_and_print() {
        let mut wb = crate::book::Workbook::new();
        let s = Shape { preset: "star5".into(), fill: Some([200, 0, 0]), line: Some([0, 0, 0]), text: "Star".into(), size: 11.0, adjust: vec![("adj".into(), 30000.0), ("hf".into(), 105146.0), ("vf".into(), 110557.0)], shadow: true, glow: Some([0x8F, 0xAA, 0xDC]), text_outline: Some([0x40, 0x40, 0x40]), color: Some([255, 255, 255]), ..Default::default() };
        let o = Object::new(ObjKind::Shape(s.clone()), Anchor::One { from: Default::default(), w: 120.0, h: 100.0 });
        add_object(&mut wb.sheets[0].objects, o, "Shape");
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let ObjKind::Shape(b) = &back.sheets[0].objects[0].kind else { panic!("not a shape") };
        assert_eq!(b.preset, "star5");
        assert_eq!(b.adjust, s.adjust);
        assert!(b.shadow);
        assert_eq!(b.glow, s.glow);
        assert_eq!(b.text_outline, s.text_outline);
        // Printing draws the star's ten-point outline, not a rectangle.
        let mut items = vec![];
        crate::print::objects::draw_object(&back, 0, &back.sheets[0].objects[0], [0.0, 0.0, 120.0, 100.0], 1.0, &mut Default::default(), &mut items);
        let pts = items.iter().find_map(|i| if let word_engine::layout::DrawItem::Path { points, .. } = i { Some(points.len()) } else { None }).unwrap();
        assert!(pts >= 10, "{pts}");
        // A smaller adjust value moves the star's inner points.
        let a = pieces(&s, [0.0, 0.0, 100.0, 100.0]);
        let mut t = s.clone();
        t.adjust[0].1 = 10000.0;
        assert_ne!(a[0].points, pieces(&t, [0.0, 0.0, 100.0, 100.0])[0].points);
    }

    #[test]
    fn every_gallery_preset_has_geometry() {
        for p in ["rightArrow", "flowChartDecision", "mathPlus", "wedgeRectCallout", "cloud", "ribbon2", "bentConnector3", "can"] {
            assert!(known(p), "{p}");
            assert!(!pieces(&Shape { preset: p.into(), ..Default::default() }, [0.0, 0.0, 80.0, 60.0]).is_empty(), "{p}");
        }
    }
}
