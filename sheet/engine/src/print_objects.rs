//! Floating objects on printed pages: pictures, charts, shapes and text boxes, slicers — placed
//! on each page they overlap, clipped to that page's cell area, as word-engine draw items.
//! Charts come from the chart layout the screen renderer (app chart_ui.rs) shares.

use super::{measure, shape_text, wrap_lines, PrintPage};
use crate::book::{Sheet, Workbook};
use crate::chart::{Chart, Prim};
use crate::drawing::{Anchor, ObjKind, Object, Shape, Slicer};
use std::collections::HashMap;
use word_engine::layout::{DrawItem, Effects, Glyph};
use word_engine::model::Asset;
use word_engine::Color;

/// Images a render uses, by asset name (the PDF writer and SVG renderer look them up here).
pub type Assets = HashMap<String, Asset>;

const WHITE: Color = Color(255, 255, 255);

fn col(c: [u8; 3]) -> Color { Color(c[0], c[1], c[2]) }

/// Start of each column (or row) in sheet pixels, computed up to what is asked.
struct Pos<'a> { s: &'a Sheet, rows: bool, acc: Vec<f32> }
impl Pos<'_> {
    fn at(&mut self, i: u32) -> f32 {
        if self.acc.is_empty() { self.acc.push(0.0); }
        while self.acc.len() <= i as usize {
            let j = (self.acc.len() - 1) as u32;
            let sz = if self.rows { self.s.row_height(j) } else { self.s.col_width(j) };
            let last = *self.acc.last().unwrap();
            self.acc.push(last + sz);
        }
        self.acc[i as usize]
    }
    /// The track holding sheet pixel `x`.
    fn index(&mut self, x: f32) -> u32 {
        let mut i = 0u32;
        while i < 1_100_000 && self.at(i + 1) <= x { i += 1; }
        i
    }
}

/// An object's rectangle in sheet pixels.
pub fn sheet_rect(s: &Sheet, o: &Object) -> [f32; 4] {
    let cp = std::cell::RefCell::new(Pos { s, rows: false, acc: vec![] });
    let rp = std::cell::RefCell::new(Pos { s, rows: true, acc: vec![] });
    o.anchor.rect(|c| cp.borrow_mut().at(c), |r| rp.borrow_mut().at(r))
}

/// The last cell an object covers (for the printed range).
pub fn last_cell(s: &Sheet, o: &Object) -> Option<crate::addr::Pos> {
    if matches!(o.kind, ObjKind::Other) { return None; }
    if let Anchor::Two { to, .. } = &o.anchor {
        return Some(crate::addr::Pos::new(if to.dy > 0.0 || to.row == 0 { to.row } else { to.row - 1 }, if to.dx > 0.0 || to.col == 0 { to.col } else { to.col - 1 }));
    }
    let r = sheet_rect(s, o);
    let mut cp = Pos { s, rows: false, acc: vec![] };
    let mut rp = Pos { s, rows: true, acc: vec![] };
    Some(crate::addr::Pos::new(rp.index((r[3] - 0.5).max(0.0)), cp.index((r[2] - 0.5).max(0.0))))
}

/// A block of consecutive printed tracks: its sheet extent and where it starts on the page.
struct Block { s0: f32, s1: f32, p0: f32 }

fn blocks(s: &Sheet, title: Option<(u32, u32)>, body: (u32, u32), rows: bool, k: f32) -> Vec<Block> {
    let mut pos = Pos { s, rows, acc: vec![] };
    let mut out = vec![];
    let mut at = 0.0;
    for (a, b) in title.into_iter().chain(std::iter::once(body)) {
        let (s0, s1) = (pos.at(a), pos.at(b + 1));
        if s1 > s0 { out.push(Block { s0, s1, p0: at }); at += (s1 - s0) * k; }
    }
    out
}

/// Draws the sheet's objects onto a page whose cell grid starts at (gx, gy) with width cw.
pub(super) fn draw(wb: &Workbook, pg: &PrintPage, gx: f32, gy: f32, cw: f32, rtl: bool, assets: &mut Assets, out: &mut Vec<DrawItem>) {
    let s = &wb.sheets[pg.sheet];
    if s.objects.is_empty() { return; }
    let k = pg.scale;
    let cb = blocks(s, pg.title_cols, pg.cols, false, k);
    let rb = blocks(s, pg.title_rows, pg.rows, true, k);
    for o in &s.objects {
        if matches!(o.kind, ObjKind::Other) { continue; }
        let r = sheet_rect(s, o);
        for bx in &cb {
            if r[2] <= bx.s0 || r[0] >= bx.s1 { continue; }
            for by in &rb {
                if r[3] <= by.s0 || r[1] >= by.s1 { continue; }
                let px = |sx: f32| bx.p0 + (sx - bx.s0) * k;
                let (mut x0, mut x1) = (gx + px(r[0]), gx + px(r[2]));
                let (mut c0, mut c1) = (gx + bx.p0, gx + bx.p0 + (bx.s1 - bx.s0) * k);
                if rtl {
                    (x0, x1) = (2.0 * gx + cw - x1, 2.0 * gx + cw - x0);
                    (c0, c1) = (2.0 * gx + cw - c1, 2.0 * gx + cw - c0);
                }
                let y0 = gy + by.p0 + (r[1] - by.s0) * k;
                let y1 = gy + by.p0 + (r[3] - by.s0) * k;
                let clip = [c0, gy + by.p0, c1, gy + by.p0 + (by.s1 - by.s0) * k];
                let mut items = vec![];
                draw_object(wb, pg.sheet, o, [x0, y0, x1, y1], k, assets, &mut items);
                for it in items { clip_item(it, clip, assets, out); }
            }
        }
    }
}

/// Draws one object filling `r` (page pixels), unclipped.
pub fn draw_object(wb: &Workbook, si: usize, o: &Object, r: [f32; 4], z: f32, assets: &mut Assets, out: &mut Vec<DrawItem>) {
    // Groups, rotated and flipped objects (group.rs); hidden ones don't print.
    if crate::group::print(wb, si, o, r, z, assets, out) { return; }
    match &o.kind {
        ObjKind::Picture { data, ext } => {
            let name = format!("jadwal-obj-{si}-{}-{:x}", o.id, std::sync::Arc::as_ptr(data) as usize);
            let ct = match ext.to_ascii_lowercase().as_str() { "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", _ => "image/png" };
            assets.entry(name.clone()).or_insert_with(|| Asset { bytes: data.to_vec(), content_type: ct.into() });
            out.push(DrawItem::Image { asset: name, x: r[0], y: r[1], w: r[2] - r[0], h: r[3] - r[1], pos: 0, rot: 0.0, flip: (false, false) });
        }
        ObjKind::Chart(c) => chart(wb, si, c, r, z, out),
        ObjKind::Shape(sh) => shape(sh, r, z, out),
        ObjKind::Slicer(sl) => slicer(wb, sl, r, z, out),
        ObjKind::Timeline(t) => timeline(wb, t, r, z, out),
        ObjKind::Group(_) | ObjKind::Other => {}
    }
}

// ---- primitives ----

fn rect_fill(out: &mut Vec<DrawItem>, r: [f32; 4], c: Color) { if r[2] > r[0] && r[3] > r[1] { out.push(DrawItem::Rect { x: r[0], y: r[1], w: r[2] - r[0], h: r[3] - r[1], color: c }); } }
fn rect_pts(r: [f32; 4]) -> Vec<(f32, f32)> { vec![(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])] }
fn line(out: &mut Vec<DrawItem>, a: (f32, f32), b: (f32, f32), w: f32, c: Color) { out.push(DrawItem::Line { x1: a.0, y1: a.1, x2: b.0, y2: b.1, width: w, color: c }); }
fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> { (0..48).map(|i| { let t = i as f32 / 48.0 * std::f32::consts::TAU; (cx + rx * t.cos(), cy + ry * t.sin()) }).collect() }
fn round_rect(r: [f32; 4], rad: f32) -> Vec<(f32, f32)> {
    let mut v = vec![];
    for (cx, cy, a0) in [(r[2] - rad, r[1] + rad, -90.0f32), (r[2] - rad, r[3] - rad, 0.0), (r[0] + rad, r[3] - rad, 90.0), (r[0] + rad, r[1] + rad, 180.0)] {
        for i in 0..=6 { let t = (a0 + 15.0 * i as f32).to_radians(); v.push((cx + rad * t.cos(), cy + rad * t.sin())); }
    }
    v
}

fn font(bold: bool) -> word_engine::fonts::FontKey { word_engine::fonts::resolve("Calibri", bold, false) }

/// Text at (x, y) aligned by `h` (0 left, 1 center, 2 right) and `v` (0 top, 1 middle, 2 bottom);
/// `rot` turns it about the anchor point (degrees, negative = counterclockwise). Returns its width.
fn text(out: &mut Vec<DrawItem>, t: &str, x: f32, y: f32, h: u8, v: u8, size: f32, bold: bool, color: Color, rot: Option<f32>) -> f32 {
    if t.is_empty() { return 0.0; }
    let key = font(bold);
    let met = word_engine::fonts::metrics(key);
    let sh = shape_text(t, key, size);
    let lx = x - sh.width * h as f32 / 2.0;
    let hgt = (met.ascent + met.descent) * size;
    let top = y - hgt * v as f32 / 2.0;
    let bl = top + met.ascent * size;
    for (k, glyphs) in sh.runs {
        let g: Vec<Glyph> = glyphs.into_iter().map(|g| Glyph { x: lx + g.x, ..g }).collect();
        out.push(DrawItem::Glyphs { glyphs: g, baseline: bl, font: k, size, color, effects: Effects { rotate: rot.map(|a| (a, x, y)), ..Default::default() } });
    }
    sh.width
}
fn text_w(t: &str, size: f32, bold: bool) -> f32 { measure(t, font(bold), size) }
fn line_h(size: f32) -> f32 { let m = word_engine::fonts::metrics(font(false)); (m.ascent + m.descent) * size }

// ---- shapes and slicers ----

fn shape(s: &Shape, r: [f32; 4], z: f32, out: &mut Vec<DrawItem>) {
    let fill = s.fill.map(col);
    let stroke = s.line.map(|c| (col(c), z.max(0.5)));
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    match s.preset.as_str() {
        "line" | "straightConnector1" => line(out, (r[0], r[1]), (r[2], r[3]), stroke.map_or(z, |s| s.1), stroke.map_or(Color(0, 0, 0), |s| s.0)),
        p if crate::shape_geom::known(p) && p != "rect" => {
            // The preset's real geometry (all presets, with their adjust values).
            for pc in crate::shape_geom::pieces(s, r) {
                let f = if pc.fill { s.fill.map(|c| col(crate::shape_geom::shaded(c, pc.shade))) } else { None };
                let st = if pc.stroke { stroke } else { None };
                if f.is_some() || st.is_some() { out.push(DrawItem::Path { points: pc.points, closed: pc.closed, fill: f, stroke: st, ink: None }); }
            }
        }
        p => {
            let pts = match p {
                "ellipse" => ellipse((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0, w / 2.0, h / 2.0),
                "roundRect" => round_rect(r, w.min(h) * 0.16),
                "triangle" => vec![((r[0] + r[2]) / 2.0, r[1]), (r[2], r[3]), (r[0], r[3])],
                _ => rect_pts(r),
            };
            if fill.is_some() || stroke.is_some() { out.push(DrawItem::Path { points: pts, closed: true, fill, stroke, ink: None }); }
        }
    }
    let r = if crate::shape_geom::known(&s.preset) { crate::shape_geom::text_rect(s, r) } else { r };
    if s.text.is_empty() { return; }
    let color = s.color.map(col).unwrap_or(if s.fill.is_some_and(|f| (f[0] as u32 + f[1] as u32 + f[2] as u32) < 300) { WHITE } else { Color(0, 0, 0) });
    let size = (s.size.max(1.0) * 4.0 / 3.0 * z).max(4.0);
    let pad = 4.0 * z;
    let inner = [r[0] + pad, r[1] + pad, r[2] - pad, r[3] - pad];
    let lines = wrap_lines(&s.text, font(s.bold), size, (inner[2] - inner[0]).max(4.0));
    let lh = line_h(size);
    let total = lh * lines.len() as f32;
    let mut y = if s.text_box { inner[1] } else { (inner[1] + inner[3]) / 2.0 - total / 2.0 };
    let mut items = vec![];
    for l in &lines {
        let (x, ha) = if s.center { ((inner[0] + inner[2]) / 2.0, 1) } else { (inner[0], 0) };
        // WordArt effects: a glow and a shadow behind, an outline around the letters.
        if let Some(g) = s.glow { for (dx, dy) in [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0), (-1.5, -1.5), (1.5, 1.5), (-1.5, 1.5), (1.5, -1.5)] { text(&mut items, l, x + dx * z, y + dy * z, ha, 0, size, s.bold, col(g), None); } }
        if s.shadow { text(&mut items, l, x + 2.0 * z, y + 2.0 * z, ha, 0, size, s.bold, Color(0x90, 0x90, 0x90), None); }
        if let Some(o) = s.text_outline { for (dx, dy) in [(-0.8, 0.0), (0.8, 0.0), (0.0, -0.8), (0.0, 0.8)] { text(&mut items, l, x + dx * z, y + dy * z, ha, 0, size, s.bold, col(o), None); } }
        text(&mut items, l, x, y, ha, 0, size, s.bold, color, None);
        y += lh;
    }
    // Text stays inside the shape.
    for it in items { clip_vec(it, r, out); }
}

/// A timeline as on screen (app timeline_ui.rs): caption, selection, level, and the period strip.
fn timeline(wb: &Workbook, t: &crate::timeline::Timeline, r: [f32; 4], z: f32, out: &mut Vec<DrawItem>) {
    out.push(DrawItem::Path { points: rect_pts(r), closed: true, fill: Some(WHITE), stroke: Some((Color(0xBF, 0xBF, 0xBF), z)), ink: None });
    let mut inner = vec![];
    text(&mut inner, &t.caption, r[0] + 8.0 * z, r[1] + 14.0 * z, 0, 1, 13.0 * z, true, Color(0x40, 0x40, 0x40), None);
    text(&mut inner, &crate::timeline::selection_text(wb, t), r[0] + 8.0 * z, r[1] + 34.0 * z, 0, 1, 10.0 * z, false, Color(0x60, 0x60, 0x60), None);
    text(&mut inner, t.level.label(), r[2] - 8.0 * z, r[1] + 34.0 * z, 2, 1, 9.0 * z, false, Color(0x44, 0x72, 0xC4), None);
    let accent = col(wb.theme_colors.get(4).copied().unwrap_or([0x44, 0x72, 0xC4]));
    let ps = crate::timeline::periods(wb, t);
    let (x0, y0, w, h) = (r[0] + 8.0 * z, r[1] + 62.0 * z, 38.0 * z, 18.0 * z);
    for (i, p) in ps.iter().enumerate() {
        let x = x0 + i as f32 * w;
        if x + w > r[2] - 4.0 * z { break; }
        let on = t.selection.is_some_and(|(a, b)| p.start <= b && p.end >= a);
        if i == 0 || ps[i - 1].year != p.year { text(&mut inner, &p.year.to_string(), x + 2.0 * z, y0 - 8.0 * z, 0, 1, 9.0 * z, false, Color(0x60, 0x60, 0x60), None); }
        inner.push(DrawItem::Path { points: rect_pts([x + 1.0 * z, y0, x + w - 1.0 * z, y0 + h]), closed: true, fill: Some(if on || t.selection.is_none() { accent } else { Color(0xE6, 0xE6, 0xE6) }), stroke: None, ink: None });
        text(&mut inner, &p.label, x + w / 2.0, y0 + h + 9.0 * z, 1, 1, 8.0 * z, false, Color(0x60, 0x60, 0x60), None);
    }
    for it in inner { clip_vec(it, r, out); }
}

fn slicer(wb: &Workbook, s: &Slicer, r: [f32; 4], z: f32, out: &mut Vec<DrawItem>) {
    const HEAD: f32 = 30.0;
    const ITEM: f32 = 26.0;
    out.push(DrawItem::Path { points: rect_pts(r), closed: true, fill: Some(WHITE), stroke: Some((Color(0xBF, 0xBF, 0xBF), z)), ink: None });
    let mut inner = vec![];
    text(&mut inner, &s.caption, r[0] + 8.0 * z, r[1] + HEAD * z / 2.0, 0, 1, 13.0 * z, true, Color(0x40, 0x40, 0x40), None);
    let accent = col(wb.theme_colors.get(4).copied().unwrap_or([0x44, 0x72, 0xC4]));
    let soft = Color(((accent.0 as f32) * 0.85 + 255.0 * 0.0) as u8, ((accent.1 as f32) * 0.85) as u8, ((accent.2 as f32) * 0.85) as u8);
    let items = crate::drawing::slicer_items(wb, s);
    let top = r[1] + HEAD * z;
    for (i, (name, on)) in items.iter().enumerate() {
        let b = [r[0] + 8.0 * z, top + i as f32 * (ITEM + 4.0) * z, r[2] - 8.0 * z, top + i as f32 * (ITEM + 4.0) * z + ITEM * z];
        if b[3] > r[3] - 2.0 * z { break; }
        inner.push(DrawItem::Path { points: round_rect(b, 3.0 * z), closed: true, fill: Some(if *on { soft } else { Color(0xF0, 0xF0, 0xF0) }), stroke: Some((if *on { accent } else { Color(0xD0, 0xD0, 0xD0) }, z)), ink: None });
        let mut t = vec![];
        text(&mut t, name, (b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0, 1, 1, 12.0 * z, false, if *on { WHITE } else { Color(0x80, 0x80, 0x80) }, None);
        for it in t { clip_vec(it, b, &mut inner); }
    }
    for it in inner { clip_vec(it, r, out); }
}

// ---- charts ----

/// Text measured with the print fonts, for the chart layout.
struct PrintMeasure;
impl crate::chart::Measure for PrintMeasure {
    fn size(&self, t: &str, px: f32, bold: bool) -> [f32; 2] { [text_w(t, px, bold), line_h(px)] }
}

/// Over white, a color seen through at `opacity`.
fn over_white(c: [u8; 3], opacity: f32) -> Color { let k = opacity.clamp(0.0, 1.0); let f = |x: u8| (x as f32 * k + 255.0 * (1.0 - k)).round() as u8; Color(f(c[0]), f(c[1]), f(c[2])) }

/// Draws a chart into `r` at zoom `z`: the shared chart layout as draw items.
pub fn chart(wb: &Workbook, si: usize, c: &Chart, r: [f32; 4], z: f32, out: &mut Vec<DrawItem>) {
    let data = c.data(wb, si);
    let mut items = vec![];
    for p in crate::chart::layout(c, &data, r, &wb.theme_colors, z, &PrintMeasure) {
        match p {
            Prim::Rect { r, fill, stroke } => {
                if stroke.is_none() { if let Some(f) = fill { rect_fill(&mut items, r, col(f)); } }
                else { items.push(DrawItem::Path { points: rect_pts(r), closed: true, fill: fill.map(col), stroke: stroke.map(|(c, w)| (col(c), w)), ink: None }); }
            }
            Prim::Line { pts, color, width } => {
                if pts.len() == 2 { line(&mut items, (pts[0][0], pts[0][1]), (pts[1][0], pts[1][1]), width, col(color)); }
                else if pts.len() > 2 { items.push(DrawItem::Path { points: pts.iter().map(|p| (p[0], p[1])).collect(), closed: false, fill: None, stroke: Some((col(color), width)), ink: None }); }
            }
            Prim::Poly { pts, fill, stroke, opacity, .. } => {
                if pts.len() >= 3 { items.push(DrawItem::Path { points: pts.iter().map(|p| (p[0], p[1])).collect(), closed: true, fill: fill.map(|f| over_white(f, opacity)), stroke: stroke.map(|(c, w)| (col(c), w)), ink: None }); }
            }
            Prim::Text { at, text: t, size, bold, color, angle, h, v } => { text(&mut items, &t, at[0], at[1], h, v, size, bold, col(color), (angle != 0.0).then_some(angle)); }
        }
    }
    for it in items { clip_vec(it, r, out); }
}

// ---- clipping ----

/// Clips an item to `c` (x0, y0, x1, y1) without image cropping (images are kept or dropped).
fn clip_vec(it: DrawItem, c: [f32; 4], out: &mut Vec<DrawItem>) {
    let mut none = Assets::new();
    clip_item(it, c, &mut none, out);
}

fn clip_seg(mut a: (f32, f32), mut b: (f32, f32), c: [f32; 4]) -> Option<((f32, f32), (f32, f32))> {
    // Liang-Barsky.
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [(-dx, a.0 - c[0]), (dx, c[2] - a.0), (-dy, a.1 - c[1]), (dy, c[3] - a.1)] {
        if p == 0.0 { if q < 0.0 { return None; } continue; }
        let t = q / p;
        if p < 0.0 { if t > t1 { return None; } t0 = t0.max(t); } else { if t < t0 { return None; } t1 = t1.min(t); }
    }
    let o = a;
    a = (o.0 + dx * t0, o.1 + dy * t0);
    b = (o.0 + dx * t1, o.1 + dy * t1);
    Some((a, b))
}

fn clip_poly(pts: &[(f32, f32)], c: [f32; 4]) -> Vec<(f32, f32)> {
    // Sutherland-Hodgman against each edge.
    let mut v = pts.to_vec();
    for e in 0..4 {
        let inside = |p: (f32, f32)| match e { 0 => p.0 >= c[0], 1 => p.0 <= c[2], 2 => p.1 >= c[1], _ => p.1 <= c[3] };
        let cross = |a: (f32, f32), b: (f32, f32)| {
            let t = match e { 0 => (c[0] - a.0) / (b.0 - a.0), 1 => (c[2] - a.0) / (b.0 - a.0), 2 => (c[1] - a.1) / (b.1 - a.1), _ => (c[3] - a.1) / (b.1 - a.1) };
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        };
        let src = std::mem::take(&mut v);
        for i in 0..src.len() {
            let (p, q) = (src[i], src[(i + 1) % src.len()]);
            match (inside(p), inside(q)) {
                (true, true) => v.push(q),
                (true, false) => v.push(cross(p, q)),
                (false, true) => { v.push(cross(p, q)); v.push(q); }
                _ => {}
            }
        }
        if v.is_empty() { break; }
    }
    v
}

/// Clips an item to `c`; pictures cut by the edge are cropped into new assets.
fn clip_item(it: DrawItem, c: [f32; 4], assets: &mut Assets, out: &mut Vec<DrawItem>) {
    let eps = 0.01;
    match it {
        DrawItem::Rect { x, y, w, h, color } => {
            let r = [x.max(c[0]), y.max(c[1]), (x + w).min(c[2]), (y + h).min(c[3])];
            rect_fill(out, r, color);
        }
        DrawItem::Line { x1, y1, x2, y2, width, color } => if let Some((a, b)) = clip_seg((x1, y1), (x2, y2), c) { line(out, a, b, width, color); },
        DrawItem::Path { points, closed, fill, stroke, ink } => {
            let inside = points.iter().all(|p| p.0 >= c[0] - eps && p.0 <= c[2] + eps && p.1 >= c[1] - eps && p.1 <= c[3] + eps);
            if inside { out.push(DrawItem::Path { points, closed, fill, stroke, ink }); return; }
            if closed {
                if let Some(f) = fill { let p = clip_poly(&points, c); if p.len() >= 3 { out.push(DrawItem::Path { points: p, closed: true, fill: Some(f), stroke: None, ink }); } }
                if let Some((sc, w)) = stroke {
                    for i in 0..points.len() { if let Some((a, b)) = clip_seg(points[i], points[(i + 1) % points.len()], c) { line(out, a, b, w, sc); } }
                }
            } else if let Some((sc, w)) = stroke {
                for s in points.windows(2) { if let Some((a, b)) = clip_seg(s[0], s[1], c) { line(out, a, b, w, sc); } }
            }
        }
        DrawItem::Glyphs { glyphs, baseline, font, size, color, effects } => {
            if let Some((_, cx, cy)) = effects.rotate {
                if cx >= c[0] && cx <= c[2] && cy >= c[1] && cy <= c[3] { out.push(DrawItem::Glyphs { glyphs, baseline, font, size, color, effects }); }
                return;
            }
            let met = word_engine::fonts::metrics(font);
            if baseline - met.ascent * size * 0.5 < c[1] || baseline + met.descent * size * 0.2 > c[3] { return; }
            let g: Vec<Glyph> = glyphs.into_iter().filter(|g| g.x >= c[0] - 0.5 && g.x + size * 0.3 <= c[2] + 0.5).collect();
            if !g.is_empty() { out.push(DrawItem::Glyphs { glyphs: g, baseline, font, size, color, effects }); }
        }
        DrawItem::Image { asset, x, y, w, h, pos, rot, flip } => {
            let v = [x.max(c[0]), y.max(c[1]), (x + w).min(c[2]), (y + h).min(c[3])];
            if v[2] - v[0] < eps || v[3] - v[1] < eps { return; }
            if v[0] <= x + eps && v[1] <= y + eps && v[2] >= x + w - eps && v[3] >= y + h - eps { out.push(DrawItem::Image { asset, x, y, w, h, pos, rot, flip }); return; }
            let key = format!("{asset}@{:.2},{:.2},{:.2},{:.2}", (v[0] - x) / w, (v[1] - y) / h, (v[2] - x) / w, (v[3] - y) / h);
            if !assets.contains_key(&key) {
                let Some(src) = assets.get(&asset) else { return };
                let Ok(img) = image::load_from_memory(&src.bytes) else { return };
                let (iw, ih) = (img.width() as f32, img.height() as f32);
                let (u0, v0) = (((v[0] - x) / w * iw).floor().max(0.0) as u32, ((v[1] - y) / h * ih).floor().max(0.0) as u32);
                let (u1, v1) = (((v[2] - x) / w * iw).ceil().min(iw) as u32, ((v[3] - y) / h * ih).ceil().min(ih) as u32);
                if u1 <= u0 || v1 <= v0 { return; }
                let part = img.crop_imm(u0, v0, u1 - u0, v1 - v0);
                let mut buf = std::io::Cursor::new(vec![]);
                if part.write_to(&mut buf, image::ImageFormat::Png).is_err() { return; }
                assets.insert(key.clone(), Asset { bytes: buf.into_inner(), content_type: "image/png".into() });
            }
            out.push(DrawItem::Image { asset: key, x: v[0], y: v[1], w: v[2] - v[0], h: v[3] - v[1], pos, rot, flip });
        }
        other => out.push(other),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{pages, pdf, render, JobInfo, What};
    use crate::addr::{Pos as Cell, Range};
    use crate::book::Workbook;
    use crate::chart::{Chart, ChartKind};
    use crate::drawing::{Anchor, Mark, ObjKind, Object};
    use std::sync::Arc;
    use word_engine::layout::DrawItem;

    fn info() -> JobInfo { JobInfo { file_name: "t.xlsx".into(), folder: String::new(), now: Some(45000.0) } }
    fn png() -> Vec<u8> {
        let img = image::RgbImage::from_fn(8, 8, |x, y| image::Rgb([(x * 30) as u8, (y * 30) as u8, 128]));
        let mut b = std::io::Cursor::new(vec![]);
        image::DynamicImage::ImageRgb8(img).write_to(&mut b, image::ImageFormat::Png).unwrap();
        b.into_inner()
    }
    fn two(c0: u32, r0: u32, c1: u32, r1: u32) -> Anchor { Anchor::Two { from: Mark { col: c0, row: r0, ..Default::default() }, to: Mark { col: c1, row: r1, ..Default::default() }, edit_as: None } }

    #[test]
    fn pdf_has_chart_and_picture() {
        let mut wb = Workbook::new();
        for (i, row) in [["", "North", "South"], ["Jan", "10", "4"], ["Feb", "20", "8"], ["Mar", "15", "12"]].iter().enumerate() {
            for (j, t) in row.iter().enumerate() { wb.set_input(0, Cell::new(i as u32, j as u32), t); }
        }
        let mut c = Chart::from_range(&wb, 0, Range::new(Cell::new(0, 0), Cell::new(3, 2)), ChartKind::Line, None);
        c.markers = true;
        wb.sheets[0].objects.push(Object::new(ObjKind::Chart(Box::new(c)), two(4, 0, 10, 15)));
        wb.sheets[0].objects.push(Object::new(ObjKind::Picture { data: Arc::new(png()), ext: "png".into() }, two(0, 5, 3, 12)));
        let pg = pages(&wb, &What::ActiveSheet);
        let (lay, doc) = render(&wb, &pg, &info());
        assert_eq!(doc.assets.len(), 1);
        let over = &lay.pages[0].over;
        assert!(over.iter().any(|i| matches!(i, DrawItem::Image { .. })));
        assert!(over.iter().filter(|i| matches!(i, DrawItem::Path { closed: false, .. })).count() >= 2, "series lines");
        assert!(over.iter().any(|i| matches!(i, DrawItem::Path { closed: true, fill: Some(_), .. })), "markers");
        assert!(over.iter().any(|i| matches!(i, DrawItem::Glyphs { glyphs, .. } if glyphs.iter().any(|g| g.ch == 'N'))), "legend text");
        let bytes = pdf(&wb, &pg, &info(), "t", 1);
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Subtype /Image"));
        assert!(s.contains("/Im"));
    }

    #[test]
    fn secondary_axis_and_radar_charts_print() {
        let mut wb = Workbook::new();
        for (i, row) in [["", "North", "South"], ["Jan", "10", "400"], ["Feb", "20", "800"], ["Mar", "15", "1200"], ["Apr", "5", "300"]].iter().enumerate() {
            for (j, t) in row.iter().enumerate() { wb.set_input(0, Cell::new(i as u32, j as u32), t); }
        }
        wb.recalc();
        let r = Range::new(Cell::new(0, 0), Cell::new(4, 2));
        let mut combo = Chart::from_range(&wb, 0, r, ChartKind::Column, None);
        combo.series[1].kind = Some(ChartKind::Line);
        combo.series[1].secondary = true;
        combo.y2_title = Some("Southward".into());
        let mut radar = Chart::from_range(&wb, 0, r, ChartKind::Radar, None);
        radar.markers = true;
        wb.sheets[0].objects.push(Object::new(ObjKind::Chart(Box::new(combo)), two(4, 0, 10, 15)));
        wb.sheets[0].objects.push(Object::new(ObjKind::Chart(Box::new(radar)), two(4, 16, 10, 31)));
        let pg = pages(&wb, &What::ActiveSheet);
        let (lay, _) = render(&wb, &pg, &info());
        let over: Vec<&DrawItem> = lay.pages.iter().flat_map(|p| p.over.iter()).collect();
        let glyphs = |s: &str| over.iter().any(|i| matches!(i, DrawItem::Glyphs { glyphs, .. } if glyphs.iter().map(|g| g.ch).collect::<String>() == s));
        // The secondary axis: its own title and its own ticks (to 1250 beside the primary's 20).
        assert!(glyphs("Southward"), "secondary axis title");
        let all: Vec<String> = over.iter().filter_map(|i| if let DrawItem::Glyphs { glyphs, .. } = i { Some(glyphs.iter().map(|g| g.ch).collect()) } else { None }).collect();
        assert!(glyphs("1000") && glyphs("20") && !glyphs("25"), "both axes' ticks: {all:?}");
        // Lines (the secondary series, the radar's outlines and rings) and filled markers.
        assert!(over.iter().filter(|i| matches!(i, DrawItem::Path { closed: false, .. })).count() >= 4, "open paths");
        assert!(over.iter().filter(|i| matches!(i, DrawItem::Path { closed: true, fill: Some(_), .. })).count() >= 8, "markers");
        let bytes = pdf(&wb, &pg, &info(), "t", 1);
        assert!(bytes.len() > 1000);
    }

    #[test]
    fn formatted_series_wrapped_titles_and_dates_print() {
        let p = format!("{}/tests/fixtures/charts/openpyxl_format.xlsx", env!("CARGO_MANIFEST_DIR"));
        let wb = crate::xlsx::read(&std::fs::read(p).unwrap()).unwrap();
        let pg = pages(&wb, &What::ActiveSheet);
        let (lay, _) = render(&wb, &pg, &info());
        let over: Vec<&DrawItem> = lay.pages.iter().flat_map(|p| p.over.iter()).collect();
        let runs: Vec<String> = over.iter().filter_map(|i| if let DrawItem::Glyphs { glyphs, .. } = i { Some(glyphs.iter().map(|g| g.ch).collect()) } else { None }).collect();
        // The long title prints on several lines (the chart straddles a page edge, so lines may be cut there).
        assert!(runs.iter().any(|r| r.starts_with("A rather long")) && runs.iter().any(|r| r.starts_with("chart title")), "{runs:?}");
        assert!(!runs.iter().any(|r| r.contains("long chart")), "{runs:?}");
        // The scatter's secondary axis ticks, and the date axis's labels every two months.
        assert!(runs.iter().any(|r| r == "1000") && runs.iter().any(|r| r == "Falling"));
        assert!(runs.iter().any(|r| r == "May-24") && runs.iter().any(|r| r == "Jul-24") && !runs.iter().any(|r| r == "Jun-24"), "{runs:?}");
        // The dashed series: many short red segments.
        let red = over.iter().filter(|i| matches!(i, DrawItem::Line { color: word_engine::Color(255, 0, 0), .. } | DrawItem::Path { stroke: Some((word_engine::Color(255, 0, 0), _)), .. })).count();
        assert!(red >= 10, "dashes: {red}");
    }

    #[test]
    fn object_spanning_pages_prints_on_both() {
        let mut wb = Workbook::new();
        wb.set_input(0, Cell::new(0, 0), "x");
        // Rows 0..=120 are more than one page tall; the shape covers rows 30..90.
        let sh = crate::drawing::Shape { text: "Box".into(), preset: "rect".into(), fill: Some([200, 0, 0]), line: Some([0, 0, 0]), size: 11.0, ..Default::default() };
        wb.sheets[0].objects.push(Object::new(ObjKind::Shape(sh), two(1, 30, 4, 90)));
        wb.sheets[0].objects.push(Object::new(ObjKind::Picture { data: Arc::new(png()), ext: "png".into() }, two(0, 40, 2, 70)));
        let pg = pages(&wb, &What::ActiveSheet);
        assert!(pg.len() >= 2, "used range grows to the objects");
        let (lay, doc) = render(&wb, &pg, &info());
        let red = |p: &word_engine::layout::PageBox| p.over.iter().any(|i| matches!(i, DrawItem::Path { fill: Some(word_engine::Color(200, 0, 0)), .. }));
        let pic = |p: &word_engine::layout::PageBox| p.over.iter().any(|i| matches!(i, DrawItem::Image { .. }));
        let hits: Vec<usize> = (0..lay.pages.len()).filter(|&i| red(&lay.pages[i])).collect();
        assert!(hits.len() >= 2, "{hits:?}");
        assert!(lay.pages.iter().filter(|p| pic(p)).count() >= 2);
        // Clipped to each page's cell area; the cut picture is cropped into new assets.
        assert!(doc.assets.len() >= 3);
        for p in &lay.pages {
            for i in &p.over { if let DrawItem::Path { points, .. } = i { assert!(points.iter().all(|q| q.1 >= 0.0 && q.1 <= p.height)); } }
        }
    }
}
