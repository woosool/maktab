//! Document text rendering: shaped glyph ids rasterized from the bundled outlines into one
//! texture atlas and drawn as a textured mesh. egui's own text system can't draw glyph ids, which
//! Arabic shaping and ligatures produce, so the canvas paints document text here.

use egui::{pos2, vec2, Color32, ColorImage, Mesh, Pos2, Rect, TextureHandle, TextureOptions};
use resvg::tiny_skia::{FillRule, PathBuilder, Pixmap, Transform};
use std::collections::HashMap;
use word_engine::fonts::{self, FontKey};

const ATLAS: usize = 2048;
/// Subpixel horizontal positions per pixel.
const SUBPIXEL: f32 = 4.0;

/// How a glyph is rasterized: filled, stroked (outline effect) or blurred by a radius in px (glow).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style { Fill, Stroke, Blur(u8) }

#[derive(Clone, Copy)]
struct Slot { uv: Rect, offset: egui::Vec2, size: egui::Vec2 }

pub struct GlyphAtlas {
    texture: Option<TextureHandle>,
    slots: HashMap<(u16, u16, u32, u8, Style), Option<Slot>>,
    shelf: (usize, usize, usize), // x, y, row height
    full: bool,
}

struct Sink(PathBuilder);
impl word_engine::ttf_parser::OutlineBuilder for Sink {
    fn move_to(&mut self, x: f32, y: f32) { self.0.move_to(x, y); }
    fn line_to(&mut self, x: f32, y: f32) { self.0.line_to(x, y); }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.0.quad_to(x1, y1, x, y); }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.0.cubic_to(x1, y1, x2, y2, x, y); }
    fn close(&mut self) { self.0.close(); }
}

impl Default for GlyphAtlas { fn default() -> Self { GlyphAtlas { texture: None, slots: HashMap::new(), shelf: (1, 1, 0), full: false } } }

impl GlyphAtlas {
    /// Start a frame: an atlas that filled up last frame starts over.
    pub fn begin(&mut self, ctx: &egui::Context) {
        if self.full || self.texture.is_none() {
            self.slots.clear();
            self.shelf = (1, 1, 0);
            self.full = false;
            let blank = ColorImage::filled([ATLAS, ATLAS], Color32::TRANSPARENT);
            self.texture = Some(ctx.load_texture("document-glyphs", blank, TextureOptions::LINEAR));
        }
    }

    /// Rasterize (or reuse) a glyph at a physical pixel size and subpixel phase.
    fn slot(&mut self, face: u16, gid: u16, px: f32, phase: u8, bold: bool, style: Style) -> Option<Slot> {
        let size_q = (px * 4.0).round() as u32 | if bold { 1 << 31 } else { 0 };
        if let Some(s) = self.slots.get(&(face, gid, size_q, phase, style)) { return *s; }
        let made = self.rasterize(face, gid, px, phase, bold, style);
        self.slots.insert((face, gid, size_q, phase, style), made);
        made
    }

    #[allow(clippy::too_many_arguments)]
    fn rasterize(&mut self, face: u16, gid: u16, px: f32, phase: u8, bold: bool, style: Style) -> Option<Slot> {
        let outline = style == Style::Stroke;
        let pad = match style { Style::Blur(r) => r as f32 * 2.0, _ => 0.0 };
        let f = fonts::ttf(face);
        let scale = px / f.units_per_em() as f32;
        let mut sink = Sink(PathBuilder::new());
        let bbox = f.outline_glyph(word_engine::ttf_parser::GlyphId(gid), &mut sink)?;
        let path = sink.0.finish()?;
        let shift = phase as f32 / SUBPIXEL;
        let embolden = if bold { px / 32.0 } else { 0.0 };
        let x0 = (bbox.x_min as f32 * scale + shift).floor() - 1.0 - pad;
        let y0 = (-(bbox.y_max as f32) * scale).floor() - 1.0 - pad;
        let w = ((bbox.x_max as f32 * scale + shift + embolden + pad).ceil() - x0 + 2.0) as usize;
        let h = ((-(bbox.y_min as f32) * scale + pad).ceil() - y0 + 2.0) as usize;
        if w == 0 || h == 0 || w > 640 || h > 640 { return None; }
        let mut pixmap = Pixmap::new(w as u32, h as u32)?;
        let mut paint = resvg::tiny_skia::Paint::default();
        paint.set_color_rgba8(255, 255, 255, 255);
        paint.anti_alias = true;
        // Font units are y-up; the pixmap is y-down with the glyph origin at (-x0, -y0).
        let t = Transform::from_row(scale, 0.0, 0.0, -scale, shift - x0, -y0);
        if outline {
            // Hollow letters (w:outline): the outline stroked at about 1/30 em.
            let stroke = resvg::tiny_skia::Stroke { width: (px / 30.0).max(0.8) / scale, ..Default::default() };
            pixmap.stroke_path(&path, &paint, &stroke, t, None);
        } else {
            pixmap.fill_path(&path, &paint, FillRule::Winding, t, None);
            if bold { pixmap.fill_path(&path, &paint, FillRule::Winding, t.post_translate(embolden, 0.0), None); }
        }
        // Shelf packing.
        let (mut x, mut y, mut row) = self.shelf;
        if x + w + 1 > ATLAS { x = 1; y += row + 1; row = 0; }
        if y + h + 1 > ATLAS { self.full = true; return None; }
        let mut a: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();
        if let Style::Blur(r) = style { blur(&mut a, w, h, r as usize); }
        let alpha: Vec<Color32> = a.into_iter().map(|a| Color32::from_rgba_premultiplied(a, a, a, a)).collect();
        let image = ColorImage { size: [w, h], pixels: alpha, source_size: vec2(w as f32, h as f32) };
        self.texture.as_mut()?.set_partial([x, y], image, TextureOptions::LINEAR);
        self.shelf = (x + w + 1, y, row.max(h));
        let n = ATLAS as f32;
        Some(Slot { uv: Rect::from_min_size(pos2(x as f32 / n, y as f32 / n), vec2(w as f32 / n, h as f32 / n)), offset: vec2(x0, y0), size: vec2(w as f32, h as f32) })
    }

    /// Add one glyph run to a mesh. `origin` is the baseline start in screen points.
    #[allow(clippy::too_many_arguments)]
    pub fn add_glyphs(&mut self, mesh: &mut Mesh, ppp: f32, key: FontKey, size_points: f32, glyphs: impl Iterator<Item = (u16, Pos2)>, color: Color32, outline: bool) {
        let style = if outline { Style::Stroke } else { Style::Fill };
        self.add_styled(mesh, ppp, key, size_points, glyphs, style, Paint::Flat(color));
    }

    /// Add glyphs with a rasterization style and a paint: flat, a vertical gradient (top → bottom
    /// of the em box), or a reflection (flipped below the baseline, fading out).
    #[allow(clippy::too_many_arguments)]
    pub fn add_styled(&mut self, mesh: &mut Mesh, ppp: f32, key: FontKey, size_points: f32, glyphs: impl Iterator<Item = (u16, Pos2)>, style: Style, paint: Paint) {
        let px = size_points * ppp;
        if px < 1.0 { return; }
        for (gid, at) in glyphs {
            // Snap the baseline to the pixel grid; keep a subpixel phase horizontally.
            let (xp, yp) = (at.x * ppp, (at.y * ppp).round());
            let phase = ((xp.fract() + 1.0).fract() * SUBPIXEL).floor() as u8;
            let Some(s) = self.slot(key.face, gid, px, phase.min(SUBPIXEL as u8 - 1), key.synthetic_bold, style) else { continue };
            let min = pos2((xp.floor() + s.offset.x) / ppp, (yp + s.offset.y) / ppp);
            let rect = Rect::from_min_size(min, s.size / ppp);
            match paint {
                Paint::Flat(c) => mesh.add_rect_with_uv(rect, s.uv, c),
                Paint::Gradient(top, bottom) => {
                    // The gradient spans the em box: 0.8 em above the baseline to 0.2 em below.
                    let em = size_points;
                    let t = |y: f32| ((y - (at.y - em * 0.8)) / em).clamp(0.0, 1.0);
                    let mix = |k: f32| lerp(top, bottom, k);
                    quad(mesh, rect, s.uv, [mix(t(rect.min.y)), mix(t(rect.min.y)), mix(t(rect.max.y)), mix(t(rect.max.y))], false);
                }
                Paint::Reflection(c, gap) => {
                    // Mirror the glyph box about the baseline + gap; fade from 35% to nothing.
                    let axis = at.y + gap;
                    let flipped = Rect::from_min_max(pos2(rect.min.x, 2.0 * axis - rect.max.y), pos2(rect.max.x, 2.0 * axis - rect.min.y));
                    let fade = |y: f32| ((1.0 - (y - axis) / (size_points * 0.6)).clamp(0.0, 1.0) * 0.35 * 255.0) as u8;
                    let top = c.gamma_multiply_u8(fade(flipped.min.y));
                    let bot = c.gamma_multiply_u8(fade(flipped.max.y));
                    quad(mesh, flipped, s.uv, [top, top, bot, bot], true);
                }
            }
        }
    }

    pub fn texture_id(&self) -> Option<egui::TextureId> { self.texture.as_ref().map(|t| t.id()) }
}

#[derive(Clone, Copy)]
pub enum Paint { Flat(Color32), Gradient(Color32, Color32), Reflection(Color32, f32) }

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// A textured quad with per-corner colors (top-left, top-right, bottom-left, bottom-right).
fn quad(mesh: &mut Mesh, rect: Rect, uv: Rect, colors: [Color32; 4], flip_v: bool) {
    let i = mesh.vertices.len() as u32;
    let (v0, v1) = if flip_v { (uv.max.y, uv.min.y) } else { (uv.min.y, uv.max.y) };
    let corners = [(rect.left_top(), pos2(uv.min.x, v0)), (rect.right_top(), pos2(uv.max.x, v0)), (rect.left_bottom(), pos2(uv.min.x, v1)), (rect.right_bottom(), pos2(uv.max.x, v1))];
    for (k, (p, t)) in corners.into_iter().enumerate() { mesh.vertices.push(egui::epaint::Vertex { pos: p, uv: t, color: colors[k] }); }
    mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i + 1, i + 3, i + 2]);
}

/// Three box-blur passes approximate a Gaussian (glow).
fn blur(a: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 { return; }
    let mut tmp = vec![0u32; a.len()];
    for _ in 0..3 {
        for y in 0..h { let mut acc = 0u32; for x in 0..w + r { if x < w { acc += a[y * w + x] as u32; } if x >= 2 * r + 1 { acc -= a[y * w + x - 2 * r - 1] as u32; } if x >= r && x - r < w { tmp[y * w + x - r] = acc / (2 * r as u32 + 1); } } }
        for x in 0..w { let mut acc = 0u32; for y in 0..h + r { if y < h { acc += tmp[y * w + x]; } if y >= 2 * r + 1 { acc -= tmp[(y - 2 * r - 1) * w + x]; } if y >= r && y - r < h { a[(y - r) * w + x] = (acc / (2 * r as u32 + 1)).min(255) as u8; } } }
    }
}
