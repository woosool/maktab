//! Drawing a scene into pixels with tiny-skia: fills (solid, gradient, picture), outlines with
//! dashes and arrowheads, shadows, pictures with crops, tables, and text as glyph outlines.
//! The editor, the navigator's thumbnails, the slide show and PNG export all draw this way.

use crate::geom::{self, Cmd, FillMode};
use crate::scene::{Affine, Fill, Item, Kind, Line, Scene, TextBody};
use crate::text;
use crate::{Presentation, Rgba};
use std::collections::HashMap;
use std::sync::Arc;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, StrokeDash, Transform};

/// Decoded pictures, kept between renders.
#[derive(Default)]
pub struct Images { cache: HashMap<String, Option<Arc<Pixmap>>> }

impl Images {
    pub fn get(&mut self, pres: &Presentation, part: &str) -> Option<Arc<Pixmap>> {
        self.cache.entry(part.to_string()).or_insert_with(|| {
            let bytes = pres.pkg.bytes(part)?;
            let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (w, h) = img.dimensions();
            let mut pm = Pixmap::new(w.max(1), h.max(1))?;
            for (dst, src) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
                let [r, g, b, a] = src.0;
                *dst = tiny_skia::ColorU8::from_rgba(r, g, b, a).premultiply();
            }
            Some(Arc::new(pm))
        }).clone()
    }
    pub fn forget(&mut self, part: &str) { self.cache.remove(part); }
}

/// Options for one render.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Pixels per EMU.
    pub scale: f64,
    /// Show empty placeholders' prompts and dashed frames (the editor, not the show).
    pub prompts: bool,
    /// Skip text in these items (being edited in place).
    pub skip_text_of: Option<usize>,
    /// Translates placeholder prompts into the interface language.
    pub translate: Option<fn(&str) -> String>,
}

fn color(c: Rgba, alpha: f32) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, ((c.a * alpha).clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn ts(a: &Affine, scale: f64) -> Transform {
    let s = scale as f32;
    Transform::from_row(a.a as f32 * s, a.b as f32 * s, a.c as f32 * s, a.d as f32 * s, a.e as f32 * s, a.f as f32 * s)
}

fn build_path(cmds: &[Cmd]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let mut open = false;
    for c in cmds {
        match *c {
            Cmd::Move(p) => { pb.move_to(p.x as f32, p.y as f32); open = true; }
            Cmd::Line(p) => { if !open { pb.move_to(p.x as f32, p.y as f32); open = true; } else { pb.line_to(p.x as f32, p.y as f32); } }
            Cmd::Cubic(a, b, e) => { if !open { pb.move_to(a.x as f32, a.y as f32); open = true; } pb.cubic_to(a.x as f32, a.y as f32, b.x as f32, b.y as f32, e.x as f32, e.y as f32); }
            Cmd::Close => { pb.close(); }
        }
    }
    pb.finish()
}

/// The 48 DrawingML pattern presets as 8×8 tiles: true where the foreground shows.
pub fn pattern_bit(preset: &str, x: u32, y: u32) -> bool {
    let (x, y) = (x % 8, y % 8);
    let dens = |n: u32| { const B: [u32; 64] = [0, 32, 8, 40, 2, 34, 10, 42, 48, 16, 56, 24, 50, 18, 58, 26, 12, 44, 4, 36, 14, 46, 6, 38, 60, 28, 52, 20, 62, 30, 54, 22, 3, 35, 11, 43, 1, 33, 9, 41, 51, 19, 59, 27, 49, 17, 57, 25, 15, 47, 7, 39, 13, 45, 5, 37, 63, 31, 55, 23, 61, 29, 53, 21]; B[(y * 8 + x) as usize] < n * 64 / 100 };
    match preset {
        "pct5" => dens(5), "pct10" => dens(10), "pct20" => dens(20), "pct25" => dens(25), "pct30" => dens(30), "pct40" => dens(40), "pct50" => dens(50), "pct60" => dens(60), "pct70" => dens(70), "pct75" => dens(75), "pct80" => dens(80), "pct90" => dens(90),
        "horz" => y % 8 == 0, "ltHorz" => y % 4 == 0, "narHorz" => y % 2 == 0, "dkHorz" => y % 4 < 2,
        "vert" => x % 8 == 0, "ltVert" => x % 4 == 0, "narVert" => x % 2 == 0, "dkVert" => x % 4 < 2,
        "dnDiag" => (x + 8 - y) % 8 == 0, "ltDnDiag" => (x + 8 - y) % 4 == 0, "dkDnDiag" => (x + 8 - y) % 4 < 2, "wdDnDiag" => (x + 8 - y) % 8 < 3,
        "upDiag" => (x + y) % 8 == 0, "ltUpDiag" => (x + y) % 4 == 0, "dkUpDiag" => (x + y) % 4 < 2, "wdUpDiag" => (x + y) % 8 < 3,
        "dashDnDiag" => (x + 8 - y) % 4 == 0 && y % 8 < 4, "dashUpDiag" => (x + y) % 4 == 0 && y % 8 < 4,
        "dashHorz" => y % 4 == 0 && (x + y) % 8 < 4, "dashVert" => x % 4 == 0 && (x + y) % 8 < 4,
        "cross" => x % 8 == 0 || y % 8 == 0, "smGrid" => x % 4 == 0 || y % 4 == 0, "lgGrid" => x == 0 || y == 0,
        "diagCross" => (x + y) % 8 == 0 || (x + 8 - y) % 8 == 0, "openDmnd" => (x + y) % 8 == 0 || (x + 8 - y) % 8 == 0,
        "smCheck" => (x / 2 + y / 2) % 2 == 0, "lgCheck" => (x / 4 + y / 4) % 2 == 0,
        "dotGrid" => x % 4 == 0 && y % 4 == 0, "dotDmnd" => (x % 4 == 0 && y % 4 == 0) || (x % 4 == 2 && y % 4 == 2),
        "smConfetti" => matches!((x, y), (1, 0) | (5, 1) | (2, 3) | (7, 4) | (3, 6) | (6, 7)), "lgConfetti" => matches!((x / 2, y / 2), (0, 0) | (2, 1) | (1, 2) | (3, 3)) && (x + y) % 2 == 0,
        "horzBrick" => y % 4 == 0 || (x == if y < 4 { 0 } else { 4 }), "diagBrick" => (x + y) % 8 == 0 || ((x + 8 - y) % 8 == 0 && (x + y) % 16 < 8),
        "solidDmnd" => (x as i32 - 4).abs() + (y as i32 - 4).abs() <= 3, "plaid" => y < 3 || (x < 3 && y % 2 == 0), "sphere" => (x as i32 - 3).pow(2) + (y as i32 - 3).pow(2) <= 8,
        "weave" => ((x + y) % 8 < 2) ^ ((x + 8 - y) % 8 < 2 && y % 4 < 2), "divot" => matches!((x, y), (3, 1) | (4, 2) | (3, 3) | (0, 5) | (7, 6) | (0, 7)),
        "shingle" => y == (x / 2) || (y == 0 && x > 5), "wave" => y == [2, 1, 1, 2, 3, 4, 4, 3][x as usize], "trellis" => (x + y) % 4 == 0 || (x + 8 - y) % 4 == 0 && y % 2 == 0,
        "zigZag" => y == [0, 1, 2, 3, 3, 2, 1, 0][x as usize] || y == [4, 5, 6, 7, 7, 6, 5, 4][x as usize],
        _ => dens(50),
    }
}

thread_local! { static SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) }; }

fn pattern_tile(preset: &str, fg: Rgba, bg: Rgba, alpha: f32) -> Option<Pixmap> {
    let mut pm = Pixmap::new(8, 8)?;
    for y in 0..8 { for x in 0..8 {
        let c = if pattern_bit(preset, x, y) { fg } else { bg };
        pm.pixels_mut()[(y * 8 + x) as usize] = tiny_skia::ColorU8::from_rgba(c.r, c.g, c.b, (c.a * alpha * 255.0) as u8).premultiply();
    } }
    Some(pm)
}

/// The picture a fill needs (decoded), so a paint can borrow it.
fn fill_image(fill: &Fill, background: Option<&Fill>, images: &mut Images, pres: &Presentation) -> Option<Arc<Pixmap>> {
    match fill {
        Fill::Image { part, .. } => images.get(pres, part),
        Fill::Pattern { preset, fg, bg } => pattern_tile(preset, *fg, *bg, 1.0).map(Arc::new),
        Fill::Background => background.and_then(|b| fill_image(b, None, images, pres)),
        _ => None,
    }
}

/// A paint for a fill over a w×h shape (shape space); `img` is the fill's picture.
fn paint_for<'a>(fill: &Fill, w: f64, h: f64, img: Option<&'a Pixmap>, opacity: f32, background: Option<&Fill>) -> Option<Paint<'a>> {
    let mut paint = Paint { anti_alias: true, ..Default::default() };
    match fill {
        Fill::None => return None,
        Fill::Solid(c) => paint.set_color(color(*c, opacity)),
        Fill::Gradient { stops, angle, radial } => {
            let st: Vec<tiny_skia::GradientStop> = stops.iter().map(|(p, c)| tiny_skia::GradientStop::new(*p, color(*c, opacity))).collect();
            if st.len() == 1 { paint.set_color(color(stops[0].1, opacity)); return Some(paint); }
            let shader = if *radial {
                tiny_skia::RadialGradient::new(tiny_skia::Point::from_xy(w as f32 / 2.0, h as f32 / 2.0), tiny_skia::Point::from_xy(w as f32 / 2.0, h as f32 / 2.0), (w.max(h) / 2.0 * 1.2) as f32, st, tiny_skia::SpreadMode::Pad, Transform::identity())
            } else {
                // The gradient line runs through the center at `angle`, spanning the box's projection.
                let (s, c) = angle.to_radians().sin_cos();
                let half = (w * c.abs() + h * s.abs()) / 2.0;
                let (cx, cy) = (w / 2.0, h / 2.0);
                tiny_skia::LinearGradient::new(tiny_skia::Point::from_xy((cx - c * half) as f32, (cy - s * half) as f32), tiny_skia::Point::from_xy((cx + c * half) as f32, (cy + s * half) as f32), st, tiny_skia::SpreadMode::Pad, Transform::identity())
            };
            paint.shader = shader?;
        }
        Fill::Image { tile, alpha, .. } => {
            let img = img?;
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            let t = if *tile { Transform::from_scale(12_700.0 * 0.75, 12_700.0 * 0.75) } else { Transform::from_scale(w as f32 / iw, h as f32 / ih) };
            paint.shader = tiny_skia::Pattern::new(img.as_ref(), if *tile { tiny_skia::SpreadMode::Repeat } else { tiny_skia::SpreadMode::Pad }, tiny_skia::FilterQuality::Bilinear, alpha * opacity, t);
        }
        Fill::Pattern { .. } => {
            // One tile pixel per device pixel, whatever the zoom.
            let k = 1.0 / SCALE.with(|s| s.get()).max(1e-9);
            paint.shader = tiny_skia::Pattern::new(img?.as_ref(), tiny_skia::SpreadMode::Repeat, tiny_skia::FilterQuality::Nearest, opacity, Transform::from_scale(k, k));
        }
        Fill::Background => return background.and_then(|b| if matches!(b, Fill::Background) { None } else { paint_for(b, w, h, img, opacity, None) }),
    }
    Some(paint)
}

fn stroke_line(pm: &mut Pixmap, path: &tiny_skia::Path, line: &Line, t: Transform, opacity: f32) {
    let mut paint = Paint { anti_alias: true, ..Default::default() };
    paint.set_color(color(line.color, opacity));
    let width = line.width.max(6_350.0) as f32;
    let mut stroke = Stroke { width, line_cap: tiny_skia::LineCap::Butt, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
    if !line.dash.is_empty() { stroke.dash = StrokeDash::new(line.dash.iter().map(|d| (*d as f32 * width).max(1.0)).collect(), 0.0); }
    pm.stroke_path(path, &paint, &stroke, t, None);
}

fn arrowhead(pm: &mut Pixmap, tip: geom::Pt, from: geom::Pt, line: &Line, t: Transform, opacity: f32) {
    let (dx, dy) = (tip.x - from.x, tip.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 { return; }
    let (ux, uy) = (dx / len, dy / len);
    let size = (line.width * 3.0).max(50_000.0);
    let (bx, by) = (tip.x - ux * size, tip.y - uy * size);
    let (px, py) = (-uy * size * 0.5, ux * size * 0.5);
    let mut pb = PathBuilder::new();
    pb.move_to(tip.x as f32, tip.y as f32);
    pb.line_to((bx + px) as f32, (by + py) as f32);
    pb.line_to((bx - px) as f32, (by - py) as f32);
    pb.close();
    if let Some(p) = pb.finish() {
        let mut paint = Paint { anti_alias: true, ..Default::default() };
        paint.set_color(color(line.color, opacity));
        pm.fill_path(&p, &paint, FillRule::Winding, t, None);
    }
}

/// Blur a pixmap in place (three box passes approximate a Gaussian of `radius` px).
pub fn blur(pm: &mut Pixmap, radius: f32) {
    let r = (radius / 2.0).round() as i32;
    if r < 1 { return; }
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    let mut buf: Vec<[f32; 4]> = pm.pixels().iter().map(|p| [p.red() as f32, p.green() as f32, p.blue() as f32, p.alpha() as f32]).collect();
    let mut tmp = buf.clone();
    let pass = |src: &Vec<[f32; 4]>, dst: &mut Vec<[f32; 4]>, horizontal: bool| {
        let (len, lines) = if horizontal { (w, h) } else { (h, w) };
        let norm = 1.0 / (2 * r + 1) as f32;
        for l in 0..lines {
            let idx = |i: i32| -> usize { let i = i.clamp(0, len - 1); (if horizontal { l * w + i } else { i * w + l }) as usize };
            let mut acc = [0.0f32; 4];
            for i in -r..=r { let p = src[idx(i)]; for c in 0..4 { acc[c] += p[c]; } }
            for i in 0..len {
                let o = idx(i);
                for c in 0..4 { dst[o][c] = acc[c] * norm; }
                let (add, sub) = (src[idx(i + r + 1)], src[idx(i - r)]);
                for c in 0..4 { acc[c] += add[c] - sub[c]; }
            }
        }
    };
    for _ in 0..3 { pass(&buf, &mut tmp, true); pass(&tmp, &mut buf, false); }
    for (d, s) in pm.pixels_mut().iter_mut().zip(buf) {
        let a = s[3].clamp(0.0, 255.0) as u8;
        let c = |v: f32| (v.clamp(0.0, 255.0) as u8).min(a);
        *d = tiny_skia::PremultipliedColorU8::from_rgba(c(s[0]), c(s[1]), c(s[2]), a).unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
    }
}

/// An offscreen layer covering `bounds` (target pixels) for drawing effects.
struct Layer { pm: Pixmap, x: i32, y: i32 }

impl Layer {
    fn new(target: &Pixmap, bounds: tiny_skia::Rect, margin: f32) -> Option<Layer> {
        let x0 = (bounds.left() - margin).floor().max(0.0) as i32;
        let y0 = (bounds.top() - margin).floor().max(0.0) as i32;
        let x1 = (bounds.right() + margin).ceil().min(target.width() as f32) as i32;
        let y1 = (bounds.bottom() + margin).ceil().min(target.height() as f32) as i32;
        if x1 <= x0 || y1 <= y0 { return None; }
        Some(Layer { pm: Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32)?, x: x0, y: y0 })
    }
    /// A transform drawn into the layer instead of the target.
    fn local(&self, t: Transform) -> Transform { t.post_translate(-self.x as f32, -self.y as f32) }
    /// Replace every pixel's color with `c`, keeping (scaled) coverage.
    fn tint(&mut self, c: Rgba, alpha: f32) {
        for p in self.pm.pixels_mut() {
            let a = (p.alpha() as f32 * c.a * alpha).clamp(0.0, 255.0) / 255.0;
            *p = tiny_skia::ColorU8::from_rgba(c.r, c.g, c.b, (a * 255.0) as u8).premultiply();
        }
    }
    fn draw_onto(&self, target: &mut Pixmap, dx: f32, dy: f32) {
        target.draw_pixmap(self.x + dx.round() as i32, self.y + dy.round() as i32, self.pm.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), None);
    }
}

/// The pixel bounds of an item's box under transform `t` (target pixels).
fn bounds_of(t: Transform, w: f64, h: f64) -> Option<tiny_skia::Rect> {
    let pts = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| { let mut p = tiny_skia::Point::from_xy(x as f32, y as f32); t.map_point(&mut p); p });
    let (minx, maxx) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.x), a.1.max(p.x)));
    let (miny, maxy) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.y), a.1.max(p.y)));
    tiny_skia::Rect::from_ltrb(minx, miny, maxx.max(minx + 1.0), maxy.max(miny + 1.0))
}

struct Outline<'a> { pb: &'a mut PathBuilder, x: f32, y: f32, s: f32, skew: f32 }
impl ttf_parser::OutlineBuilder for Outline<'_> {
    fn move_to(&mut self, x: f32, y: f32) { self.pb.move_to(self.x + (x + y * self.skew) * self.s, self.y - y * self.s); }
    fn line_to(&mut self, x: f32, y: f32) { self.pb.line_to(self.x + (x + y * self.skew) * self.s, self.y - y * self.s); }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.pb.quad_to(self.x + (x1 + y1 * self.skew) * self.s, self.y - y1 * self.s, self.x + (x + y * self.skew) * self.s, self.y - y * self.s); }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.pb.cubic_to(self.x + (x1 + y1 * self.skew) * self.s, self.y - y1 * self.s, self.x + (x2 + y2 * self.skew) * self.s, self.y - y2 * self.s, self.x + (x + y * self.skew) * self.s, self.y - y * self.s); }
    fn close(&mut self) { self.pb.close(); }
}

/// Draw laid-out text; `t` maps the text box's points to pixels.
pub fn draw_text(pm: &mut Pixmap, laid: &text::Layout, t: Transform, opacity: f32) {
    for run in &laid.runs {
        let face = word_engine::fonts::ttf(run.face);
        let upm = face.units_per_em() as f32;
        let s = run.size as f32 / upm;
        if let Some(h) = run.highlight {
            let m = word_engine::fonts::metrics(word_engine::fonts::FontKey { face: run.face, synthetic_bold: false });
            if let Some(r) = tiny_skia::Rect::from_ltrb(run.span.0 as f32, (run.baseline - m.ascent as f64 * run.size) as f32, run.span.1 as f32, (run.baseline + m.descent.abs() as f64 * run.size) as f32) {
                let mut p = Paint::default();
                p.set_color(color(h, opacity));
                pm.fill_rect(r, &p, t, None);
            }
        }
        let mut pb = PathBuilder::new();
        for g in &run.glyphs {
            let mut o = Outline { pb: &mut pb, x: g.x as f32, y: g.y as f32, s, skew: if run.italic_skew { 0.2 } else { 0.0 } };
            face.outline_glyph(ttf_parser::GlyphId(g.gid), &mut o);
        }
        let mut paint = Paint { anti_alias: true, ..Default::default() };
        paint.set_color(color(run.color, opacity));
        let m0 = word_engine::fonts::metrics(word_engine::fonts::FontKey { face: run.face, synthetic_bold: false });
        let (top, bottom) = (run.baseline - m0.ascent as f64 * run.size, run.baseline + m0.descent.abs() as f64 * run.size);
        if let Some(path) = pb.finish() {
            // Shadow and glow go under the letters, blurred.
            let fx = &run.fx;
            if fx.shadow.is_some() || fx.glow.is_some() {
                let scale = (t.sx * t.sx + t.ky * t.ky).sqrt();
                if let Some(b) = path.bounds().transform(t) {
                    if let Some((c, rad)) = fx.glow {
                        let r = (rad as f32 / 12_700.0 * scale).max(1.0);
                        if let Some(mut l) = Layer::new(pm, b, r * 2.5 + 2.0) {
                            let mut p = Paint { anti_alias: true, ..Default::default() };
                            p.set_color(tiny_skia::Color::WHITE);
                            l.pm.stroke_path(&path, &p, &Stroke { width: r * 2.0 / scale.max(1e-6), line_join: tiny_skia::LineJoin::Round, ..Default::default() }, l.local(t), None);
                            l.pm.fill_path(&path, &p, FillRule::Winding, l.local(t), None);
                            blur(&mut l.pm, r);
                            l.tint(c, opacity);
                            l.draw_onto(pm, 0.0, 0.0);
                        }
                    }
                    if let Some(sh) = &fx.shadow {
                        let r = (sh.blur as f32 / 12_700.0 * scale).max(0.0);
                        if let Some(mut l) = Layer::new(pm, b, r * 2.5 + 2.0) {
                            let mut p = Paint { anti_alias: true, ..Default::default() };
                            p.set_color(tiny_skia::Color::WHITE);
                            l.pm.fill_path(&path, &p, FillRule::Winding, l.local(t), None);
                            blur(&mut l.pm, r);
                            l.tint(sh.color, opacity);
                            l.draw_onto(pm, sh.dx as f32 / 12_700.0 * scale, sh.dy as f32 / 12_700.0 * scale);
                        }
                    }
                }
            }
            // Gradient-filled text runs top to bottom across the line.
            if let Some(stops) = &fx.gradient {
                let st: Vec<tiny_skia::GradientStop> = stops.iter().map(|(p, c)| tiny_skia::GradientStop::new(*p, color(*c, opacity))).collect();
                if let Some(sh) = tiny_skia::LinearGradient::new(tiny_skia::Point::from_xy(0.0, top as f32), tiny_skia::Point::from_xy(0.0, bottom as f32), st, tiny_skia::SpreadMode::Pad, Transform::identity()) { paint.shader = sh; }
            }
            pm.fill_path(&path, &paint, FillRule::Winding, t, None);
            if run.synthetic_bold { pm.stroke_path(&path, &paint, &Stroke { width: run.size as f32 * 0.03, ..Default::default() }, t, None); }
            if let Some((c, w)) = fx.outline {
                let mut p = Paint { anti_alias: true, ..Default::default() };
                p.set_color(color(c, opacity));
                pm.stroke_path(&path, &p, &Stroke { width: (w / 12_700.0) as f32, line_join: tiny_skia::LineJoin::Round, ..Default::default() }, t, None);
            }
        }
        paint.shader = tiny_skia::Shader::SolidColor(color(run.color, opacity));
        let m = word_engine::fonts::metrics(word_engine::fonts::FontKey { face: run.face, synthetic_bold: false });
        let deco = |off: f64, thick: f64, pm: &mut Pixmap| {
            let y = run.baseline - off * run.size;
            if let Some(r) = tiny_skia::Rect::from_ltrb(run.span.0 as f32, y as f32, run.span.1 as f32, (y + (thick * run.size).max(0.5)) as f32) { pm.fill_rect(r, &paint, t, None); }
        };
        if run.underline { deco(m.underline_offset as f64, m.underline_thickness as f64, pm); if run.fx.double_underline { deco(m.underline_offset as f64 - m.underline_thickness as f64 * 2.2, m.underline_thickness as f64, pm); } }
        if run.strike {
            if run.fx.double_strike { deco(m.strike_offset as f64 + m.underline_thickness as f64 * 1.2, m.underline_thickness as f64, pm); deco(m.strike_offset as f64 - m.underline_thickness as f64 * 1.2, m.underline_thickness as f64, pm); }
            else { deco(m.strike_offset as f64, m.underline_thickness as f64, pm); }
        }
    }
}

/// Lay out and draw a text body in a shape of w×h EMU whose transform is `shape_t`.
#[allow(clippy::too_many_arguments)]
fn text_in_shape(pm: &mut Pixmap, body: &TextBody, w: f64, h: f64, text_rect: Option<[f64; 4]>, shape: &Affine, scale: f64, opacity: f32, prompts: bool, translate: Option<fn(&str) -> String>) {
    let (bx, by, bw, bh) = text::place(body, w, h, text_rect);
    let empty = body.paras.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty()));
    let mut shown = body.clone();
    if empty {
        if !(prompts && body.prompt.is_some()) { return; }
        let p = body.prompt.clone().unwrap_or_default();
        let p = match translate { Some(t) => t(&p), None => p };
        if let Some(first) = shown.paras.first_mut() {
            first.runs = vec![crate::scene::Run { text: p, font: first.end_font.clone(), size: first.end_size, color: Rgba::rgb(0x76, 0x76, 0x76), ..Default::default() }];
            first.bullet = None;
        }
        shown.paras.truncate(1);
    }
    let gap = shown.column_gap / 12_700.0;
    let cols = shown.columns.max(1) as f64;
    let col_w = ((bw - gap * (cols - 1.0)) / cols).max(1.0);
    let wrap_w = if shown.wrap { col_w } else { 1e6 };
    let mut laid = text::layout(&shown, wrap_w, bh * cols);
    text::flow_columns(&mut laid, shown.columns, col_w, gap, bh);
    let dy = text::anchor_offset(&shown, &laid, bh);
    // Vertical text turns the box about its center.
    let pt = 12_700.0;
    let mut local = Affine::translate(bx * pt, (by + dy) * pt).then(&Affine::scale(pt, pt));
    if shown.rotation != 0.0 {
        let (cx, cy) = (w / 2.0, h / 2.0);
        local = Affine::translate(cx, cy).then(&Affine::rotate(shown.rotation)).then(&Affine::translate(-cx, -cy)).then(&local);
    }
    let t = ts(&shape.then(&local), scale);
    draw_text(pm, &laid, t, opacity);
}

/// The layout of a table cell's text (as drawn), with the transform from its points to slide EMU.
pub fn cell_layout_for(item: &Item, row: usize, col: usize) -> Option<(text::Layout, Affine)> {
    let Kind::Table(tbl) = &item.kind else { return None };
    let (x, y, w, h) = tbl.cell_rect(row, col)?;
    let cell = tbl.cells.iter().find(|c| c.row == row && c.col == col)?;
    let body = &cell.text;
    let (bx, by, bw, bh) = text::place(body, w, h, None);
    let laid = text::layout(body, bw.max(1.0), bh);
    let dy = text::anchor_offset(body, &laid, bh);
    let pt = 12_700.0;
    let local = Affine::translate(x + bx * pt, y + (by + dy) * pt).then(&Affine::scale(pt, pt));
    Some((laid, item.transform.then(&local)))
}

/// The layout the editor uses for in-place editing (same as drawn), with the transform from
/// text points to slide EMU.
pub fn text_layout_for(item: &Item) -> Option<(text::Layout, Affine)> {
    let (body, rect) = match &item.kind { Kind::Shape { text: Some(t), geom, .. } => (t, geom.text_rect), _ => return None };
    let (bx, by, bw, bh) = text::place(body, item.w, item.h, rect);
    let gap = body.column_gap / 12_700.0;
    let cols = body.columns.max(1) as f64;
    let col_w = ((bw - gap * (cols - 1.0)) / cols).max(1.0);
    let mut laid = text::layout(body, if body.wrap { col_w } else { 1e6 }, bh * cols);
    text::flow_columns(&mut laid, body.columns, col_w, gap, bh);
    let dy = text::anchor_offset(body, &laid, bh);
    let pt = 12_700.0;
    let local = Affine::translate(bx * pt, (by + dy) * pt).then(&Affine::scale(pt, pt));
    Some((laid, item.transform.then(&local)))
}

pub fn render(pres: &Presentation, scene: &Scene, images: &mut Images, opts: Options) -> Option<Pixmap> {
    let (pw, ph) = ((scene.w * opts.scale).ceil().max(1.0) as u32, (scene.h * opts.scale).ceil().max(1.0) as u32);
    let mut pm = Pixmap::new(pw, ph)?;
    SCALE.with(|s| s.set(opts.scale as f32));
    let full = Affine::IDENTITY;
    let bg_path = build_path(&[Cmd::Move(geom::Pt { x: 0.0, y: 0.0 }), Cmd::Line(geom::Pt { x: scene.w, y: 0.0 }), Cmd::Line(geom::Pt { x: scene.w, y: scene.h }), Cmd::Line(geom::Pt { x: 0.0, y: scene.h }), Cmd::Close])?;
    let bg_img = fill_image(&scene.background, None, images, pres);
    match paint_for(&scene.background, scene.w, scene.h, bg_img.as_deref(), 1.0, None) {
        Some(p) => pm.fill_path(&bg_path, &p, FillRule::Winding, ts(&full, opts.scale), None),
        None => {} // No background: transparent (layers, Paste as Picture).
    }
    for (k, item) in scene.items.iter().enumerate() {
        if item.hidden { continue; }
        draw_item(&mut pm, pres, scene, item, images, opts, Some(k) == opts.skip_text_of);
    }
    Some(pm)
}

/// The slide split for playing animations: the slide without the animated top-level shapes
/// (by index in the shape tree), and each of those on its own transparent slide-sized layer.
pub fn render_layers(pres: &Presentation, scene: &Scene, images: &mut Images, opts: Options, animated: &[usize]) -> Option<(Pixmap, Vec<(usize, Pixmap)>)> {
    let top = |it: &Item| it.loc.path.first().copied().unwrap_or(usize::MAX);
    let mut base_scene = scene.clone();
    base_scene.items.retain(|it| !it.own || !animated.contains(&top(it)));
    let base = render(pres, &base_scene, images, opts)?;
    let mut layers = vec![];
    for &a in animated {
        let mut s = scene.clone();
        s.background = crate::scene::Fill::None;
        s.items.retain(|it| it.own && top(it) == a);
        if s.items.is_empty() { continue; }
        layers.push((a, render(pres, &s, images, opts)?));
    }
    Some((base, layers))
}

fn draw_item(pm: &mut Pixmap, pres: &Presentation, scene: &Scene, item: &Item, images: &mut Images, opts: Options, skip_text: bool) {
    // 3-D: the shape drawn flat on its own (only its area), then shaded, extruded and turned.
    // Drawn shapes are cached, so an unchanged 3-D shape isn't drawn again.
    if let Some(t3) = item.effects.three_d.as_ref() {
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            format!("{item:?}").hash(&mut h);
            opts.scale.to_bits().hash(&mut h);
            (pm.width(), pm.height(), skip_text, opts.prompts).hash(&mut h);
            h.finish()
        };
        let paint = tiny_skia::PixmapPaint::default();
        if let Some((layer, x, y)) = crate::three_d::cached(key) { pm.draw_pixmap(x, y, layer.as_ref().as_ref(), &paint, Transform::identity(), None); return; }
        let Some(b) = bounds_of(ts(&item.transform, opts.scale), item.w, item.h) else { return };
        let margin = (opts.scale * 400_000.0) as f32;
        let (x0, y0) = ((b.left() - margin).floor().max(0.0) as i32, (b.top() - margin).floor().max(0.0) as i32);
        let (x1, y1) = ((b.right() + margin).ceil().min(pm.width() as f32) as i32, (b.bottom() + margin).ceil().min(pm.height() as f32) as i32);
        if x1 <= x0 || y1 <= y0 { return; }
        let Some(mut src) = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else { return };
        let mut flat = item.clone();
        flat.effects.three_d = None;
        flat.transform.e -= x0 as f64 / opts.scale;
        flat.transform.f -= y0 as f64 / opts.scale;
        draw_item(&mut src, pres, scene, &flat, images, opts, skip_text);
        let (cx, cy) = item.transform.apply(item.w / 2.0, item.h / 2.0);
        // Only the pixels the flat shape covers (its margin for effects is mostly empty).
        let Some((src, x0, y0)) = crop_to_ink(src, x0, y0) else { return };
        if let Some((layer, x, y)) = crate::three_d::compose_layer(src, x0, y0, (cx * opts.scale, cy * opts.scale), t3, opts.scale, pm.width(), pm.height()) {
            let layer = crate::three_d::remember(key, layer, x, y);
            pm.draw_pixmap(x, y, layer.as_ref().as_ref(), &paint, Transform::identity(), None);
        }
        return;
    }
    let t = ts(&item.transform, opts.scale);
    let op = item.opacity;
    match &item.kind {
        Kind::Shape { geom, fill, line, text } => {
            let fx = &item.effects;
            let silhouette = |l: &mut Layer, grow: f32| {
                let mut p = Paint { anti_alias: true, ..Default::default() };
                p.set_color(tiny_skia::Color::WHITE);
                for gp in &geom.paths {
                    let Some(path) = build_path(&gp.cmds) else { continue };
                    if gp.fill != FillMode::None && !matches!(fill, Fill::None) { l.pm.fill_path(&path, &p, FillRule::EvenOdd, l.local(t), None); }
                    if let Some(ln) = line { if gp.stroke { l.pm.stroke_path(&path, &p, &Stroke { width: ln.width as f32 + grow, line_join: tiny_skia::LineJoin::Round, ..Default::default() }, l.local(t), None); } }
                    if grow > 0.0 { l.pm.stroke_path(&path, &p, &Stroke { width: grow, line_join: tiny_skia::LineJoin::Round, ..Default::default() }, l.local(t), None); }
                }
            };
            let px = opts.scale as f32;
            let bounds = bounds_of(t, item.w, item.h);
            // Glow and shadow go behind the shape, blurred.
            if let (Some((c, rad)), Some(b)) = (fx.glow, bounds) {
                let r = (rad as f32 * px).max(1.0);
                if let Some(mut l) = Layer::new(pm, b, r * 2.5 + 2.0) { silhouette(&mut l, rad as f32 * 2.0); blur(&mut l.pm, r); l.tint(c, op); l.draw_onto(pm, 0.0, 0.0); }
            }
            if let (Some(sh), Some(b)) = (&fx.shadow, bounds) {
                let r = sh.blur as f32 * px;
                if let Some(mut l) = Layer::new(pm, b, r * 2.5 + 2.0) { silhouette(&mut l, 0.0); blur(&mut l.pm, r); l.tint(sh.color, op); l.draw_onto(pm, sh.dx as f32 * px, sh.dy as f32 * px); }
            }
            // Reflection: the shape mirrored below itself, fading out.
            if let (Some(rf), Some(b)) = (&fx.reflection, bounds) {
                let mirror = item.transform.then(&Affine::translate(0.0, item.h * 2.0 + rf.dist)).then(&Affine::scale(1.0, -1.0));
                let mt = ts(&mirror, opts.scale);
                if let Some(mb) = bounds_of(mt, item.w, item.h) {
                    if let Some(mut l) = Layer::new(pm, mb, rf.blur as f32 * px * 2.5 + 2.0) {
                        for gp in &geom.paths {
                            let Some(path) = build_path(&gp.cmds) else { continue };
                            let img = fill_image(fill, Some(&scene.background), images, pres);
                            if gp.fill != FillMode::None { if let Some(paint) = paint_for(fill, item.w, item.h, img.as_deref(), op, Some(&scene.background)) { l.pm.fill_path(&path, &paint, FillRule::EvenOdd, l.local(mt), None); } }
                        }
                        blur(&mut l.pm, rf.blur as f32 * px);
                        // Fade from start alpha at the shape's edge to end alpha at `size`.
                        let (top, span) = (b.bottom() + rf.dist as f32 * px - l.y as f32, (b.height() * rf.size as f32).max(1.0));
                        let w = l.pm.width() as usize;
                        for (i, p) in l.pm.pixels_mut().iter_mut().enumerate() {
                            let y = (i / w) as f32;
                            let f = ((y - top) / span).clamp(0.0, 1.0);
                            let a = rf.start_alpha + (rf.end_alpha - rf.start_alpha) * f;
                            let a = if y - top > span { 0.0 } else { a };
                            let c = |v: u8| (v as f32 * a) as u8;
                            *p = tiny_skia::PremultipliedColorU8::from_rgba(c(p.red()), c(p.green()), c(p.blue()), c(p.alpha())).unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
                        }
                        l.draw_onto(pm, 0.0, 0.0);
                    }
                }
            }
            // Soft edges: the shape is drawn on a layer whose edges are faded.
            let soft = fx.soft_edge.zip(bounds).and_then(|(r, b)| Layer::new(pm, b, 2.0).map(|l| (l, r)));
            let mut soft_layer = soft;
            let t_body = match &soft_layer { Some((l, _)) => l.local(t), None => t };
            let target: &mut Pixmap = match &mut soft_layer { Some((l, _)) => &mut l.pm, None => pm };
            let _ = &skip_text;
            for p in &geom.paths {
                let Some(path) = build_path(&p.cmds) else { continue };
                if p.fill != FillMode::None {
                    let img = fill_image(fill, Some(&scene.background), images, pres);
                    if let Some(mut paint) = paint_for(fill, item.w, item.h, img.as_deref(), op, Some(&scene.background)) {
                        // Darken/lighten variants shade the fill.
                        if let (Fill::Solid(c), m) = (fill, p.fill) {
                            let f = match m { FillMode::Darken => 0.6, FillMode::DarkenLess => 0.8, FillMode::Lighten => 1.4, FillMode::LightenLess => 1.2, _ => 1.0 };
                            if f != 1.0 { let sc = |v: u8| ((v as f64 * f).min(255.0)) as u8; paint.set_color(color(Rgba { r: sc(c.r), g: sc(c.g), b: sc(c.b), a: c.a }, op)); }
                        }
                        target.fill_path(&path, &paint, FillRule::EvenOdd, t_body, None);
                    }
                }
                if p.stroke { if let Some(l) = line { stroke_line(target, &path, l, t_body, op); } }
            }
            if let Some((mut l, r)) = soft_layer {
                // Fade alpha toward the outline: multiply by an eroded, blurred mask.
                let mut mask = l.pm.clone();
                blur(&mut mask, (r as f32 * px).max(1.0));
                for (p, m) in l.pm.pixels_mut().iter_mut().zip(mask.pixels()) {
                    let k = ((m.alpha() as f32 / 255.0 - 0.5) * 2.0).clamp(0.0, 1.0);
                    let c = |v: u8| (v as f32 * k) as u8;
                    *p = tiny_skia::PremultipliedColorU8::from_rgba(c(p.red()), c(p.green()), c(p.blue()), c(p.alpha())).unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
                }
                l.draw_onto(pm, 0.0, 0.0);
            }
            // Inner shadow: the outside of the shape, offset and blurred, kept inside it.
            if let (Some(sh), Some(b)) = (&fx.inner, bounds) {
                let r = sh.blur as f32 * px;
                if let (Some(mut inside), Some(mut outside)) = (Layer::new(pm, b, 0.0), Layer::new(pm, b, 0.0)) {
                    silhouette(&mut inside, 0.0);
                    outside.pm.fill(tiny_skia::Color::WHITE);
                    let mut erase = Paint { anti_alias: true, blend_mode: tiny_skia::BlendMode::Clear, ..Default::default() };
                    erase.set_color(tiny_skia::Color::BLACK);
                    let shifted = t.post_translate(sh.dx as f32 * px, sh.dy as f32 * px);
                    for gp in &geom.paths { if let Some(path) = build_path(&gp.cmds) { outside.pm.fill_path(&path, &erase, FillRule::EvenOdd, outside.local(shifted), None); } }
                    blur(&mut outside.pm, r);
                    outside.tint(sh.color, op);
                    for (o, i) in outside.pm.pixels_mut().iter_mut().zip(inside.pm.pixels()) {
                        let k = i.alpha() as f32 / 255.0;
                        let c = |v: u8| (v as f32 * k) as u8;
                        *o = tiny_skia::PremultipliedColorU8::from_rgba(c(o.red()), c(o.green()), c(o.blue()), c(o.alpha())).unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
                    }
                    outside.draw_onto(pm, 0.0, 0.0);
                }
            }
            if let Some(l) = line {
                // Arrowheads at the open path's ends.
                if let Some(first) = geom.paths.first() {
                    let pts = geom::flatten(&first.cmds, 4);
                    if let Some(poly) = pts.first() {
                        if poly.len() >= 2 {
                            if l.head.is_some() { arrowhead(pm, poly[0], poly[1], l, t, op); }
                            if l.tail.is_some() { let n = poly.len(); arrowhead(pm, poly[n - 1], poly[n - 2], l, t, op); }
                        }
                    }
                }
            }
            // Empty placeholders show a dashed frame in the editor.
            if opts.prompts && item.placeholder.is_some() && item.own && text.as_ref().is_some_and(|b| b.paras.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty()))) && line.is_none() && matches!(fill, Fill::None) {
                if let Some(path) = build_path(&geom.paths.first().map(|p| p.cmds.clone()).unwrap_or_default()) {
                    let l = Line { color: Rgba::rgb(0xA6, 0xA6, 0xA6), width: 9_525.0, dash: vec![3.0, 2.0], head: None, tail: None };
                    stroke_line(pm, &path, &l, t, 1.0);
                }
            }
            if let (Some(body), false) = (text, skip_text) { text_in_shape(pm, body, item.w, item.h, geom.text_rect, &item.transform, opts.scale, op, opts.prompts && item.own, opts.translate); }
        }
        Kind::Picture { part, crop, geom, line } => {
            if let Some(img) = images.get(pres, part) {
                let (iw, ih) = (img.width() as f32, img.height() as f32);
                let [l, tp, r, b] = *crop;
                let vis_w = ((1.0 - l - r) as f32).max(0.01);
                let vis_h = ((1.0 - tp - b) as f32).max(0.01);
                let sx = item.w as f32 / (iw * vis_w);
                let sy = item.h as f32 / (ih * vis_h);
                let pat_t = Transform::from_row(sx, 0.0, 0.0, sy, -(l as f32) * iw * sx, -(tp as f32) * ih * sy);
                let paint = Paint { shader: tiny_skia::Pattern::new(img.as_ref().as_ref(), tiny_skia::SpreadMode::Pad, tiny_skia::FilterQuality::Bicubic, op, pat_t), anti_alias: true, ..Default::default() };
                for p in &geom.paths { if let Some(path) = build_path(&p.cmds) { pm.fill_path(&path, &paint, FillRule::EvenOdd, t, None); } }
            } else {
                draw_placeholder_box(pm, item, "Picture", t);
            }
            if let Some(l) = line { for p in &geom.paths { if let Some(path) = build_path(&p.cmds) { stroke_line(pm, &path, l, t, op); } } }
        }
        Kind::Table(tbl) => {
            let xs: Vec<f64> = std::iter::once(0.0).chain(tbl.cols.iter().scan(0.0, |a, w| { *a += w; Some(*a) })).collect();
            let ys: Vec<f64> = std::iter::once(0.0).chain(tbl.rows.iter().scan(0.0, |a, h| { *a += h; Some(*a) })).collect();
            for c in &tbl.cells {
                let (x0, y0) = (xs.get(c.col).copied().unwrap_or(0.0), ys.get(c.row).copied().unwrap_or(0.0));
                let (x1, y1) = (xs.get(c.col + c.col_span).copied().unwrap_or(*xs.last().unwrap()), ys.get(c.row + c.row_span).copied().unwrap_or(*ys.last().unwrap()));
                let cell_t = item.transform.then(&Affine::translate(x0, y0));
                let (w, h) = (x1 - x0, y1 - y0);
                if let Some(path) = build_path(&[Cmd::Move(geom::Pt { x: 0.0, y: 0.0 }), Cmd::Line(geom::Pt { x: w, y: 0.0 }), Cmd::Line(geom::Pt { x: w, y: h }), Cmd::Line(geom::Pt { x: 0.0, y: h }), Cmd::Close]) {
                    let img = fill_image(&c.fill, None, images, pres);
                    if let Some(p) = paint_for(&c.fill, w, h, img.as_deref(), op, None) { pm.fill_path(&path, &p, FillRule::Winding, ts(&cell_t, opts.scale), None); }
                }
                let edges = [((0.0, 0.0), (0.0, h)), ((0.0, 0.0), (w, 0.0)), ((w, 0.0), (w, h)), ((0.0, h), (w, h))];
                for (k, b) in c.borders.iter().enumerate() {
                    let Some(l) = b else { continue };
                    let ((ax, ay), (bx, by)) = edges[k];
                    if let Some(path) = build_path(&[Cmd::Move(geom::Pt { x: ax, y: ay }), Cmd::Line(geom::Pt { x: bx, y: by })]) { stroke_line(pm, &path, l, ts(&cell_t, opts.scale), op); }
                }
                text_in_shape(pm, &c.text, w, h, None, &cell_t, opts.scale, op, false, None);
            }
        }
        Kind::Chart(c) => {
            // A white chart area, then the chart (Qalam's renderer works in CSS px).
            if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, item.w as f32, item.h as f32) { let mut p = Paint::default(); p.set_color(tiny_skia::Color::WHITE); pm.fill_rect(r, &p, t, None); }
            let mut c = c.clone();
            c.width = (item.w / crate::objects::PX) as f32;
            c.height = (item.h / crate::objects::PX) as f32;
            let items = word_engine::objects::render_chart(&c, 0.0, 0.0);
            crate::objects::draw_items(pm, &items, t.pre_scale(crate::objects::PX as f32, crate::objects::PX as f32));
        }
        Kind::Frame { label, image } => {
            if let Some(img) = image.as_ref().and_then(|p| images.get(pres, p)) {
                let paint = Paint { shader: tiny_skia::Pattern::new(img.as_ref().as_ref(), tiny_skia::SpreadMode::Pad, tiny_skia::FilterQuality::Bilinear, op, Transform::from_scale(item.w as f32 / img.width() as f32, item.h as f32 / img.height() as f32)), ..Default::default() };
                if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, item.w as f32, item.h as f32) { pm.fill_rect(r, &paint, t, None); }
            } else { draw_placeholder_box(pm, item, label, t); }
        }
    }
}

fn draw_placeholder_box(pm: &mut Pixmap, item: &Item, label: &str, t: Transform) {
    if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, item.w as f32, item.h as f32) {
        let mut p = Paint::default();
        p.set_color(tiny_skia::Color::from_rgba8(0xF2, 0xF2, 0xF2, 255));
        pm.fill_rect(r, &p, t, None);
        let path = PathBuilder::from_rect(r);
        p.set_color(tiny_skia::Color::from_rgba8(0xBF, 0xBF, 0xBF, 255));
        pm.stroke_path(&path, &p, &Stroke { width: 9_525.0, ..Default::default() }, t, None);
    }
    let body = TextBody {
        paras: vec![crate::scene::Para { runs: vec![crate::scene::Run { text: label.into(), font: "Calibri".into(), size: 14.0, color: Rgba::rgb(0x76, 0x76, 0x76), ..Default::default() }], align: crate::scene::Align::Center, end_size: 14.0, ..Default::default() }],
        anchor: crate::scene::Anchor::Middle, wrap: true, insets: [91_440.0, 45_720.0, 91_440.0, 45_720.0], ..Default::default()
    };
    let (bx, by, bw, bh) = text::place(&body, item.w, item.h, None);
    let laid = text::layout(&body, bw, bh);
    let dy = text::anchor_offset(&body, &laid, bh);
    let local = t.pre_concat(Transform::from_translate((bx * 12_700.0) as f32, ((by + dy) * 12_700.0) as f32)).pre_scale(12_700.0, 12_700.0);
    draw_text(pm, &laid, local, 1.0);
}

/// PNG bytes of a slide at `width` pixels wide.
pub fn slide_png(pres: &Presentation, slide: &str, width: u32) -> Option<Vec<u8>> {
    let scene = pres.scene(slide);
    let mut images = Images::default();
    let pm = render(pres, &scene, &mut images, Options { scale: width as f64 / scene.w, prompts: false, skip_text_of: None, translate: None })?;
    pm.encode_png().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_sample() {
        let p = Presentation::open(include_bytes!("../tests/fixtures/sample.pptx")).unwrap();
        let slides = p.slides();
        let scene = p.scene(&slides[0]);
        let mut images = Images::default();
        let pm = render(&p, &scene, &mut images, Options { scale: 960.0 / scene.w, prompts: true, skip_text_of: None, translate: None }).unwrap();
        assert_eq!(pm.width(), 960);
        // The title's glyphs darken some pixels of the white slide.
        let dark = pm.pixels().iter().filter(|px| px.red() < 128).count();
        assert!(dark > 200, "dark pixels {dark}");
        assert!(slide_png(&p, &slides[1], 480).is_some_and(|b| b.starts_with(b"\x89PNG")));
        if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") {
            for (i, s) in slides.iter().enumerate() { std::fs::write(format!("{dir}/slide{}.png", i + 1), slide_png(&p, s, 1280).unwrap()).unwrap(); }
        }
    }
}

#[cfg(test)]
mod gallery {
    use super::*;
    use crate::scene::Xfrm;

    /// A slide of assorted shapes, a table and text, written to SLIDES_SNAPSHOTS for a look.
    #[test]
    fn shape_gallery() {
        let mut p = Presentation::new_blank(true);
        let blank = p.all_layouts().into_iter().find(|l| p.layout_type(l) == "blank").unwrap();
        let s = p.add_slide(&blank, 0);
        let names = ["rect", "roundRect", "ellipse", "triangle", "star5", "heart", "donut", "rightArrow", "cloud", "wedgeRectCallout", "frame", "moon", "lightningBolt", "smileyFace", "can", "cube", "chevron", "blockArc"];
        for (i, n) in names.iter().enumerate() {
            let x = Xfrm { x: 400_000.0 + (i % 6) as f64 * 1_900_000.0, y: 300_000.0 + (i / 6) as f64 * 1_500_000.0, w: 1_500_000.0, h: 1_200_000.0, ..Default::default() };
            let loc = p.insert_shape(&s, n, &x, false).unwrap();
            p.set_shape_text(&loc, None, n);
        }
        // Effects on the first row; WordArt in the text box.
        {
            use crate::format::Effect;
            let scene = p.scene(&s);
            let locs: Vec<_> = scene.items.iter().filter(|i| i.own).map(|i| i.loc.clone()).collect();
            p.set_effect(&locs[0], &Effect::Shadow { dir: 45.0, dist: 6.0, blur: 8.0, color: "000000".into(), alpha: 50 });
            p.set_effect(&locs[1], &Effect::Glow { radius: 10.0, color: "FFC000".into() });
            p.set_effect(&locs[2], &Effect::Reflection { size: 0.5, dist: 2.0 });
            p.set_effect(&locs[3], &Effect::SoftEdge { radius: 8.0 });
            p.set_effect(&locs[4], &Effect::InnerShadow { dir: 45.0, dist: 5.0, blur: 8.0 });
            p.set_gradient_fill(&locs[5], &[(0.0, "FFD966".into()), (1.0, "C00000".into())], 90.0, false);
            p.set_quick_style(&locs[6], "accent2", 5);
            let tex = crate::format::texture_png("Wood");
            p.set_picture_fill(&locs[7], tex, "png", true);
        }
        let t = p.insert_table(&s, 3, 4, &Xfrm { x: 400_000.0, y: 5_000_000.0, w: 6_000_000.0, h: 1_000_000.0, ..Default::default() }).unwrap();
        p.set_shape_text(&t, Some((0, 0)), "Header");
        p.set_shape_text(&t, Some((1, 1)), "Cell");
        let tb = p.insert_shape(&s, "rect", &Xfrm { x: 6_800_000.0, y: 5_000_000.0, w: 5_000_000.0, h: 1_200_000.0, ..Default::default() }, true).unwrap();
        p.set_shape_text(&tb, None, "WordArt مرحبا بالعالم");
        p.format_shape_text(&tb, None, &crate::edit::RunFormat { size: Some(40.0), bold: Some(true), ..Default::default() });
        p.set_wordart(&tb, 5);
        let scene = p.scene(&s);
        let mut images = Images::default();
        let pm = render(&p, &scene, &mut images, Options { scale: 1280.0 / scene.w, prompts: true, skip_text_of: None, translate: None }).unwrap();
        if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") { pm.save_png(format!("{dir}/gallery.png")).unwrap(); }
        assert!(pm.pixels().iter().filter(|px| px.blue() > 150 && px.red() < 120).count() > 1000, "accent fills");
    }
}

/// A layer cropped to its non-transparent pixels, and its new position.
fn crop_to_ink(pm: Pixmap, x: i32, y: i32) -> Option<(Pixmap, i32, i32)> {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let px = pm.pixels();
    let row = |r: usize| px[r * w..(r + 1) * w].iter().any(|p| p.alpha() > 0);
    let top = (0..h).find(|&r| row(r))?;
    let bottom = (0..h).rev().find(|&r| row(r))?;
    let col = |c: usize| (top..=bottom).any(|r| px[r * w + c].alpha() > 0);
    let left = (0..w).find(|&c| col(c))?;
    let right = (0..w).rev().find(|&c| col(c))?;
    if left == 0 && top == 0 && right + 1 == w && bottom + 1 == h { return Some((pm, x, y)); }
    let mut out = Pixmap::new((right - left + 1) as u32, (bottom - top + 1) as u32)?;
    out.draw_pixmap(-(left as i32), -(top as i32), pm.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), None);
    Some((out, x + left as i32, y + top as i32))
}
