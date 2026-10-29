//! A slide resolved for drawing (render-slide.ts): background, then the master's and layout's
//! own shapes, then the slide's, each with its transform, geometry, fill, outline and text —
//! every value resolved through the placeholder chain (slide → layout → master), the master's
//! text styles, the presentation defaults and the theme.

use crate::color::{self, ColorMap, Ctx, Rgba};
use crate::geom::{self, Geometry};
use crate::pres::{Presentation, Theme, EMU_PER_PT};
use crate::xml::Element;

/// A 2×3 affine transform: (x, y) ↦ (a·x + c·y + e, b·x + d·y + f).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine { pub a: f64, pub b: f64, pub c: f64, pub d: f64, pub e: f64, pub f: f64 }

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };
    pub fn translate(x: f64, y: f64) -> Affine { Affine { e: x, f: y, ..Affine::IDENTITY } }
    pub fn scale(sx: f64, sy: f64) -> Affine { Affine { a: sx, d: sy, ..Affine::IDENTITY } }
    pub fn rotate(deg: f64) -> Affine { let (s, c) = deg.to_radians().sin_cos(); Affine { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 } }
    /// self ∘ other (apply `other` first).
    pub fn then(&self, other: &Affine) -> Affine {
        Affine {
            a: self.a * other.a + self.c * other.b, b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d, d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e, f: self.b * other.e + self.d * other.f + self.f,
        }
    }
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) { (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f) }
    pub fn invert(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 { return None; }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine { a, b, c, d, e: -(a * self.e + c * self.f), f: -(b * self.e + d * self.f) })
    }
    /// The rotation (degrees) this transform applies.
    pub fn angle(&self) -> f64 { self.b.atan2(self.a).to_degrees() }
}

/// A shape's frame: offset, size, rotation and flips (a:xfrm), in EMU.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Xfrm { pub x: f64, pub y: f64, pub w: f64, pub h: f64, pub rot: f64, pub flip_h: bool, pub flip_v: bool }

impl Xfrm {
    pub fn read(xfrm: &Element) -> Xfrm {
        let off = xfrm.child("a:off");
        let ext = xfrm.child("a:ext");
        Xfrm {
            x: off.and_then(|o| o.attr_f64("x")).unwrap_or(0.0), y: off.and_then(|o| o.attr_f64("y")).unwrap_or(0.0),
            w: ext.and_then(|e| e.attr_f64("cx")).unwrap_or(0.0), h: ext.and_then(|e| e.attr_f64("cy")).unwrap_or(0.0),
            rot: xfrm.attr_f64("rot").unwrap_or(0.0) / 60_000.0,
            flip_h: xfrm.attr_bool("flipH").unwrap_or(false), flip_v: xfrm.attr_bool("flipV").unwrap_or(false),
        }
    }
    /// Shape space (0..w, 0..h) to its parent's space: flips and rotation about the center.
    pub fn matrix(&self) -> Affine {
        let (cx, cy) = (self.w / 2.0, self.h / 2.0);
        let flip = Affine::scale(if self.flip_h { -1.0 } else { 1.0 }, if self.flip_v { -1.0 } else { 1.0 });
        Affine::translate(self.x + cx, self.y + cy).then(&Affine::rotate(self.rot)).then(&flip).then(&Affine::translate(-cx, -cy))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    None,
    Solid(Rgba),
    /// Stops (position 0–1, color); angle in degrees (clockwise from 3 o'clock); radial when `path`.
    Gradient { stops: Vec<(f32, Rgba)>, angle: f64, radial: bool },
    /// An image part, stretched (or tiled) over the shape.
    Image { part: String, tile: bool, alpha: f32 },
    /// The slide background shows through (a:grpFill / useBgFill).
    Background,
    /// A two-color pattern (a:pattFill preset like "pct20", "dkHorz", "smGrid").
    Pattern { preset: String, fg: Rgba, bg: Rgba },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub color: Rgba,
    /// Width in EMU.
    pub width: f64,
    /// Dash pattern in multiples of the line width (empty = solid).
    pub dash: Vec<f64>,
    pub head: Option<String>,
    pub tail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shadow { pub color: Rgba, pub blur: f64, pub dx: f64, pub dy: f64 }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Reflection { pub blur: f64, pub start_alpha: f32, pub end_alpha: f32, pub dist: f64, /// How much of the shape it shows (0–1).
    pub size: f64 }

/// Shape effects (a:effectLst): outer and inner shadow, glow, soft edges, reflection.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Effects { pub shadow: Option<Shadow>, pub inner: Option<Shadow>, pub glow: Option<(Rgba, f64)>, pub soft_edge: Option<f64>, pub reflection: Option<Reflection>, pub three_d: Option<crate::three_d::ThreeD> }

impl Effects {
    pub fn is_empty(&self) -> bool { *self == Effects::default() }
    /// Read an effect list (colors resolved against `ctx`).
    pub fn read(eff: &Element, ctx: &crate::color::Ctx) -> Effects {
        let shadow = |s: &Element| {
            let color = color::resolve_child(s, ctx).unwrap_or(Rgba::BLACK.with_alpha(0.35));
            let dist = s.attr_f64("dist").unwrap_or(0.0);
            let dir = s.attr_f64("dir").unwrap_or(0.0) / 60_000.0;
            Shadow { color, blur: s.attr_f64("blurRad").unwrap_or(0.0), dx: dist * dir.to_radians().cos(), dy: dist * dir.to_radians().sin() }
        };
        Effects {
            shadow: eff.child("a:outerShdw").or_else(|| eff.child("a:prstShdw")).map(shadow),
            inner: eff.child("a:innerShdw").map(shadow),
            glow: eff.child("a:glow").map(|g| (color::resolve_child(g, ctx).unwrap_or(Rgba::rgb(255, 192, 0).with_alpha(0.6)), g.attr_f64("rad").unwrap_or(63_500.0))),
            soft_edge: eff.child("a:softEdge").and_then(|e| e.attr_f64("rad")),
            reflection: eff.child("a:reflection").map(|r| Reflection {
                blur: r.attr_f64("blurRad").unwrap_or(0.0),
                start_alpha: (r.attr_f64("stA").unwrap_or(100_000.0) / 100_000.0) as f32,
                end_alpha: (r.attr_f64("endA").unwrap_or(0.0) / 100_000.0) as f32,
                dist: r.attr_f64("dist").unwrap_or(0.0),
                size: r.attr_f64("endPos").unwrap_or(100_000.0) / 100_000.0,
            }),
            three_d: None,
        }
    }
}

impl Default for Rgba { fn default() -> Self { Rgba::BLACK } }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Align { #[default] Left, Center, Right, Justify }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Anchor { #[default] Top, Middle, Bottom }

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Spacing { #[default] Auto, Percent(f64), Points(f64) }

#[derive(Clone, Debug, PartialEq)]
pub enum Bullet { Char { ch: String, font: Option<String>, color: Option<Rgba>, size: f64 }, Number { scheme: String, value: u32, color: Option<Rgba>, size: f64 } }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Run {
    pub text: String,
    pub font: String,
    /// Size in points.
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Rgba,
    /// Superscript (+) or subscript (−), as a fraction of the size.
    pub baseline: f64,
    pub caps: bool,
    pub small_caps: bool,
    /// Extra letter spacing in points.
    pub spacing: f64,
    pub highlight: Option<Rgba>,
    pub link: Option<String>,
    /// A line break (a:br) rather than text.
    pub br: bool,
    /// Where this run is in the source (paragraph, run index) for editing.
    pub src: (usize, usize),
    /// Text effects (WordArt): outline (color, width EMU), shadow, glow, gradient fill.
    pub outline: Option<(Rgba, f64)>,
    pub shadow: Option<Shadow>,
    pub glow: Option<(Rgba, f64)>,
    pub gradient: Option<Vec<(f32, Rgba)>>,
    pub double_strike: bool,
    /// The underline style ("sng", "dbl", "dotted", "wavy", …) when underlined.
    pub underline_style: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Para {
    pub runs: Vec<Run>,
    pub align: Align,
    pub level: u32,
    /// Left margin and first-line indent in EMU.
    pub margin: f64,
    pub indent: f64,
    pub before: Spacing,
    pub after: Spacing,
    pub line: Spacing,
    pub bullet: Option<Bullet>,
    pub rtl: bool,
    /// The size of an empty paragraph's line (points).
    pub end_size: f64,
    pub end_font: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Autofit { #[default] None, Shrink { font_scale: f64, line_reduction: f64 }, Grow }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TextBody {
    pub paras: Vec<Para>,
    /// Insets l, t, r, b in EMU.
    pub insets: [f64; 4],
    pub anchor: Anchor,
    pub anchor_center: bool,
    pub wrap: bool,
    /// Text rotation: 0, 90 (vert) or 270 (vert270), plus bodyPr@rot.
    pub rotation: f64,
    pub autofit: Autofit,
    pub columns: u32,
    /// Space between columns (EMU).
    pub column_gap: f64,
    /// Placeholder prompt shown when the box is empty in the editor ("Click to add title").
    pub prompt: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub row: usize, pub col: usize, pub row_span: usize, pub col_span: usize,
    pub fill: Fill,
    pub borders: [Option<Line>; 4],
    pub text: TextBody,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table { pub cols: Vec<f64>, pub rows: Vec<f64>, pub cells: Vec<Cell> }

impl Table {
    /// Column and row edges (EMU from the table's corner).
    pub fn edges(&self) -> (Vec<f64>, Vec<f64>) {
        let xs = std::iter::once(0.0).chain(self.cols.iter().scan(0.0, |a, w| { *a += w; Some(*a) })).collect();
        let ys = std::iter::once(0.0).chain(self.rows.iter().scan(0.0, |a, h| { *a += h; Some(*a) })).collect();
        (xs, ys)
    }
    /// The cell (row, column) covering a point in table space, following merges.
    pub fn cell_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        let (xs, ys) = self.edges();
        let c = (0..self.cols.len()).find(|&i| x >= xs[i] && x < xs[i + 1])?;
        let r = (0..self.rows.len()).find(|&i| y >= ys[i] && y < ys[i + 1])?;
        self.cells.iter().find(|k| r >= k.row && r < k.row + k.row_span && c >= k.col && c < k.col + k.col_span).map(|k| (k.row, k.col))
    }
    /// A cell's rectangle (x, y, w, h) in table space.
    pub fn cell_rect(&self, row: usize, col: usize) -> Option<(f64, f64, f64, f64)> {
        let (xs, ys) = self.edges();
        let k = self.cells.iter().find(|k| k.row == row && k.col == col)?;
        let (x1, y1) = (*xs.get(k.col + k.col_span).unwrap_or(xs.last()?), *ys.get(k.row + k.row_span).unwrap_or(ys.last()?));
        Some((xs[k.col], ys[k.row], x1 - xs[k.col], y1 - ys[k.row]))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Shape { geom: Geometry, fill: Fill, line: Option<Line>, text: Option<TextBody> },
    Picture { part: String, crop: [f64; 4], geom: Geometry, line: Option<Line> },
    Table(Table),
    /// A chart (drawn by Qalam's chart renderer).
    Chart(word_engine::objects::Chart),
    /// Something drawn as a labelled box (OLE objects without a preview).
    Frame { label: String, image: Option<String> },
}

/// Where a scene item lives in the package, for editing: the part and the element path
/// (indices among element children from the spTree).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Loc { pub part: String, pub path: Vec<usize> }

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub loc: Loc,
    pub id: u32,
    pub name: String,
    /// Shape space to slide space.
    pub transform: Affine,
    pub w: f64,
    pub h: f64,
    pub kind: Kind,
    pub effects: Effects,
    pub opacity: f32,
    /// From the slide itself (selectable), rather than its layout or master.
    pub own: bool,
    pub placeholder: Option<(String, Option<u32>)>,
    pub link: Option<String>,
    pub hidden: bool,
    /// The group this item belongs to (its top-level spTree index) when inside a group.
    pub group: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene { pub w: f64, pub h: f64, pub background: Fill, pub items: Vec<Item> }

/// Inheritance context for one part's shapes.
struct Res<'a> {
    pres: &'a Presentation,
    theme: Theme,
    ctx: Ctx,
    layout: Option<String>,
    master: Option<String>,
    /// Slide number (for slidenum fields).
    number: usize,
    /// The part whose shapes are being resolved (relationship ids are relative to it).
    part: std::cell::RefCell<String>,
}

fn ph_of(sp: &Element) -> Option<(String, Option<u32>)> {
    let nv = sp.elements().find(|e| e.local().starts_with("nv"))?;
    let ph = nv.path("p:nvPr/p:ph")?;
    Some((ph.attr("type").unwrap_or("obj").to_string(), ph.attr("idx").and_then(|i| i.parse().ok())))
}

/// The master placeholder type a slide/layout placeholder type inherits from.
fn master_type(t: &str) -> &str {
    match t { "ctrTitle" | "title" => "title", "dt" => "dt", "ftr" => "ftr", "sldNum" => "sldNum", "hdr" => "hdr", _ => "body" }
}

fn tree(root: &Element) -> Option<&Element> { root.path("p:cSld/p:spTree") }

/// Find the placeholder in a layout or master that `ph` inherits from.
fn find_ph<'a>(root: &'a Element, ph: &(String, Option<u32>), is_master: bool) -> Option<&'a Element> {
    let tree = tree(root)?;
    let shapes: Vec<&Element> = tree.elements().filter(|e| e.local() == "sp").collect();
    if is_master {
        let want = master_type(&ph.0);
        return shapes.iter().copied().find(|s| ph_of(s).is_some_and(|p| master_type(&p.0) == want));
    }
    if let Some(idx) = ph.1 {
        if let Some(s) = shapes.iter().copied().find(|s| ph_of(s).is_some_and(|p| p.1 == Some(idx))) { return Some(s); }
    }
    let t = ph.0.as_str();
    shapes.iter().copied().find(|s| ph_of(s).is_some_and(|p| p.0 == t || (master_type(&p.0) == master_type(t) && t != "body" && master_type(t) != "body")))
        .or_else(|| if ph.1.is_none() { shapes.iter().copied().find(|s| ph_of(s).is_some_and(|p| master_type(&p.0) == master_type(t))) } else { None })
}

impl<'a> Res<'a> {
    fn new(pres: &'a Presentation, slide: &'a str, number: usize) -> Res<'a> {
        let master = pres.master_for(slide);
        let root = pres.pkg.xml(slide);
        let layout = if root.is_some_and(|r| r.is("p:sld")) { pres.layout_of(slide) } else if root.is_some_and(|r| r.is("p:sldLayout")) { Some(slide.to_string()) } else { None };
        let theme = master.as_deref().map(|m| pres.theme(m)).unwrap_or_default();
        let ctx = Ctx { scheme: theme.scheme.clone(), map: pres.color_map(slide), ph: None };
        Res { pres, theme, ctx, layout, master, number, part: std::cell::RefCell::new(slide.to_string()) }
    }

    /// The chain of elements a placeholder shape inherits from: (layout's, master's).
    fn ph_chain(&self, sp: &Element, part: &str) -> (Option<&'a Element>, Option<&'a Element>) {
        let Some(ph) = ph_of(sp) else { return (None, None) };
        let is_slide = self.pres.pkg.xml(part).is_some_and(|r| r.is("p:sld"));
        let is_layout = self.pres.pkg.xml(part).is_some_and(|r| r.is("p:sldLayout"));
        let layout = if is_slide { self.layout.as_deref().and_then(|l| self.pres.pkg.xml(l)).and_then(|l| find_ph(l, &ph, false)) } else { None };
        let master = if is_slide || is_layout { self.master.as_deref().and_then(|m| self.pres.pkg.xml(m)).and_then(|m| find_ph(m, &ph, true)) } else { None };
        (layout, master)
    }

    fn fill_from(&self, el: &Element, ctx: &Ctx) -> Option<Fill> {
        match el.local() {
            "noFill" => Some(Fill::None),
            "solidFill" => Some(color::resolve_child(el, ctx).map(Fill::Solid).unwrap_or(Fill::None)),
            "gradFill" => {
                let mut stops: Vec<(f32, Rgba)> = el.path("a:gsLst").map(|l| l.children_named("a:gs").filter_map(|g| Some(((g.attr_f64("pos").unwrap_or(0.0) / 100_000.0) as f32, color::resolve_child(g, ctx)?))).collect()).unwrap_or_default();
                stops.sort_by(|a, b| a.0.total_cmp(&b.0));
                if stops.is_empty() { return Some(Fill::None); }
                let radial = el.child("a:path").is_some();
                let angle = el.child("a:lin").and_then(|l| l.attr_f64("ang")).unwrap_or(5_400_000.0 * if radial { 0.0 } else { 1.0 }) / 60_000.0;
                Some(Fill::Gradient { stops, angle, radial })
            }
            "blipFill" => {
                let id = el.child("a:blip").and_then(|b| b.attr("r:embed"))?;
                let part = self.pres.pkg.rel_target(&self.cur_part(), id)?;
                let alpha = el.path("a:blip/a:alphaModFix").and_then(|a| a.attr_f64("amt")).map(|a| (a / 100_000.0) as f32).unwrap_or(1.0);
                Some(Fill::Image { part, tile: el.child("a:tile").is_some(), alpha })
            }
            "pattFill" => {
                let fg = el.child("a:fgClr").and_then(|c| color::resolve_child(c, ctx)).unwrap_or(Rgba::BLACK);
                let bg = el.child("a:bgClr").and_then(|c| color::resolve_child(c, ctx)).unwrap_or(Rgba::WHITE);
                Some(Fill::Pattern { preset: el.attr("prst").unwrap_or("pct5").to_string(), fg, bg })
            }
            "grpFill" => Some(Fill::Background),
            _ => None,
        }
    }

    fn cur_part(&self) -> String { self.part.borrow().clone() }

    /// The fill element directly in an spPr (if any).
    fn sp_fill(&self, sp_pr: Option<&Element>, ctx: &Ctx) -> Option<Fill> {
        sp_pr?.elements().find_map(|e| if matches!(e.local(), "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill" | "grpFill") { self.fill_from(e, ctx) } else { None })
    }

    fn line_from(&self, ln: &Element, ctx: &Ctx) -> Option<Option<Line>> {
        let fill = ln.elements().find_map(|e| if matches!(e.local(), "noFill" | "solidFill" | "gradFill" | "pattFill") { self.fill_from(e, ctx) } else { None });
        let color = match fill {
            Some(Fill::None) => return Some(None),
            Some(Fill::Solid(c)) => Some(c),
            Some(Fill::Gradient { stops, .. }) => stops.first().map(|s| s.1),
            _ => None,
        };
        let width = ln.attr_f64("w");
        if color.is_none() && width.is_none() && ln.child("a:prstDash").is_none() && ln.child("a:headEnd").is_none() && ln.child("a:tailEnd").is_none() { return None; }
        let dash = match ln.child("a:prstDash").and_then(|d| d.attr("val")) {
            Some("dash") => vec![4.0, 3.0], Some("dot") | Some("sysDot") => vec![1.0, 1.0], Some("dashDot") => vec![4.0, 3.0, 1.0, 3.0],
            Some("lgDash") => vec![8.0, 3.0], Some("lgDashDot") => vec![8.0, 3.0, 1.0, 3.0], Some("lgDashDotDot") => vec![8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
            Some("sysDash") => vec![3.0, 1.0], Some("sysDashDot") => vec![3.0, 1.0, 1.0, 1.0], Some("sysDashDotDot") => vec![3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            _ => vec![],
        };
        let end = |n: &str| ln.child(n).and_then(|e| e.attr("type")).filter(|t| *t != "none").map(String::from);
        Some(Some(Line { color: color.unwrap_or(Rgba::BLACK), width: width.unwrap_or(9525.0), dash, head: end("a:headEnd"), tail: end("a:tailEnd") }))
    }

    /// A style reference (p:style/a:fillRef etc.) resolved against the theme's matrix.
    fn style_ref(&self, sp: &Element, which: &str) -> Option<(usize, Ctx)> {
        let r = sp.path(&format!("p:style/a:{which}"))?;
        let idx = r.attr("idx")?.parse::<usize>().ok()?;
        let mut ctx = self.ctx.clone();
        ctx.ph = color::resolve_child(r, &self.ctx);
        Some((idx, ctx))
    }

    fn resolve_fill(&self, sp: &Element, chain: &[Option<&Element>]) -> Fill {
        let ctx = &self.ctx;
        if let Some(f) = self.sp_fill(sp.child("p:spPr"), ctx) { return f; }
        if sp.attr_bool("useBgFill") == Some(true) { return Fill::Background; }
        if let Some((idx, sctx)) = self.style_ref(sp, "fillRef") {
            if idx == 0 { return Fill::None; }
            let list = if idx >= 1000 { &self.theme.bg_fill_styles } else { &self.theme.fill_styles };
            let i = if idx >= 1000 { idx - 1001 } else { idx - 1 };
            if let Some(f) = list.get(i).and_then(|e| self.fill_from(e, &sctx)) { return f; }
        }
        for anc in chain.iter().flatten() {
            if let Some(f) = self.sp_fill(anc.child("p:spPr"), ctx) { return f; }
            if let Some((idx, sctx)) = self.style_ref(anc, "fillRef") {
                if idx == 0 { return Fill::None; }
                if let Some(f) = self.theme.fill_styles.get(idx.saturating_sub(1)).and_then(|e| self.fill_from(e, &sctx)) { return f; }
            }
        }
        Fill::None
    }

    fn resolve_line(&self, sp: &Element, chain: &[Option<&Element>]) -> Option<Line> {
        let ctx = &self.ctx;
        // A line's own properties can be partial (width only); the style supplies the rest.
        let own = sp.path("p:spPr/a:ln");
        let from_style = self.style_ref(sp, "lnRef").and_then(|(idx, sctx)| if idx == 0 { None } else { self.theme.line_styles.get(idx - 1).and_then(|e| self.line_from(e, &sctx)).flatten() });
        if let Some(ln) = own {
            match self.line_from(ln, ctx) {
                Some(None) => return None,
                Some(Some(mut l)) => {
                    let has_color = ln.elements().any(|e| e.local() == "solidFill");
                    if !has_color { if let Some(s) = &from_style { l.color = s.color; } }
                    return Some(l);
                }
                None => {}
            }
        }
        if from_style.is_some() { return from_style; }
        for anc in chain.iter().flatten() {
            if let Some(ln) = anc.path("p:spPr/a:ln") { if let Some(l) = self.line_from(ln, ctx) { return l; } }
        }
        None
    }

    fn effects(&self, sp: &Element) -> Effects {
        let mut ctx = self.ctx.clone();
        if let Some((_, c)) = self.style_ref(sp, "effectRef") { ctx = c; }
        let eff = match sp.path("p:spPr/a:effectLst") {
            Some(e) => Some(e),
            None => self.style_ref(sp, "effectRef").and_then(|(idx, _)| if idx == 0 { None } else { self.theme.effect_styles.get(idx - 1).and_then(|e| e.child("a:effectLst")) }),
        };
        let mut fx = eff.map(|e| Effects::read(e, &ctx)).unwrap_or_default();
        let style = self.style_ref(sp, "effectRef").and_then(|(idx, _)| if idx == 0 { None } else { self.theme.effect_styles.get(idx - 1) });
        fx.three_d = crate::three_d::ThreeD::read(sp.child("p:spPr"), style, &ctx).filter(|t| t.visible());
        fx
    }

    /// The level-N paragraph property elements, highest priority first, for a shape.
    fn level_props(&self, sp: &Element, chain: &[Option<&Element>], level: u32) -> Vec<Element> {
        let lvl = format!("a:lvl{}pPr", level + 1);
        let mut out = vec![];
        if let Some(l) = sp.path(&format!("p:txBody/a:lstStyle/{lvl}")) { out.push(l.clone()); }
        for anc in chain.iter().flatten() {
            if let Some(l) = anc.path(&format!("p:txBody/a:lstStyle/{lvl}")) { out.push(l.clone()); }
        }
        let ph = ph_of(sp).or_else(|| chain.iter().flatten().find_map(|a| ph_of(a)));
        let master = self.master.as_deref().and_then(|m| self.pres.pkg.xml(m));
        let style = match ph.as_ref().map(|p| master_type(&p.0)) { Some("title") => Some("p:titleStyle"), Some("body") => Some("p:bodyStyle"), Some(_) => Some("p:otherStyle"), None => None };
        if let Some(style) = style {
            if let Some(l) = master.and_then(|m| m.path(&format!("p:txStyles/{style}/{lvl}"))) { out.push(l.clone()); }
        } else {
            // Text boxes and shapes: the presentation's default text style, then otherStyle.
            if let Some(l) = self.pres.root().path(&format!("p:defaultTextStyle/{lvl}")) { out.push(l.clone()); }
            if let Some(l) = master.and_then(|m| m.path(&format!("p:txStyles/p:otherStyle/{lvl}"))) { out.push(l.clone()); }
        }
        out
    }

    fn text_body(&self, sp: &Element, chain: &[Option<&Element>], tx: &Element, fill: &Fill) -> TextBody {
        // bodyPr: the shape's, else the layout's, else the master's attribute by attribute.
        let bodies: Vec<&Element> = std::iter::once(tx.child("a:bodyPr")).chain(chain.iter().map(|a| a.and_then(|a| a.path("p:txBody/a:bodyPr")))).flatten().collect();
        let battr = |k: &str| bodies.iter().find_map(|b| b.attr(k));
        let bf = |k: &str, d: f64| battr(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
        let anchor = match battr("anchor") { Some("ctr") => Anchor::Middle, Some("b") => Anchor::Bottom, _ => Anchor::Top };
        let autofit = bodies.iter().find_map(|b| {
            if let Some(n) = b.child("a:normAutofit") { return Some(Autofit::Shrink { font_scale: n.attr_f64("fontScale").unwrap_or(100_000.0) / 100_000.0, line_reduction: n.attr_f64("lnSpcReduction").unwrap_or(0.0) / 100_000.0 }); }
            if b.child("a:spAutoFit").is_some() { return Some(Autofit::Grow); }
            if b.child("a:noAutofit").is_some() { return Some(Autofit::None); }
            None
        }).unwrap_or_default();
        let rotation = match battr("vert") { Some("vert") | Some("eaVert") | Some("mongolianVert") | Some("wordArtVertRtl") => 90.0, Some("vert270") => 270.0, _ => 0.0 } + bf("rot", 0.0) / 60_000.0;
        let ph = ph_of(sp);
        // Theme color for text on dark fills follows the master's styles; default is tx1.
        let _ = fill;
        let mut body = TextBody {
            paras: vec![],
            insets: [bf("lIns", 91_440.0), bf("tIns", 45_720.0), bf("rIns", 91_440.0), bf("bIns", 45_720.0)],
            anchor,
            anchor_center: battr("anchorCtr").is_some_and(|v| v == "1"),
            wrap: battr("wrap") != Some("none"),
            rotation,
            autofit,
            columns: bf("numCol", 1.0).max(1.0) as u32,
            column_gap: bf("spcCol", 0.0),
            prompt: ph.as_ref().map(|p| match p.0.as_str() { "title" | "ctrTitle" => "Click to add title", "subTitle" => "Click to add subtitle", "body" | "obj" => "Click to add text", "pic" => "Click icon to add picture", "chart" => "Click icon to add chart", "tbl" => "Click icon to add table", _ => "" }.to_string()).filter(|s| !s.is_empty()),
        };
        // Number bullets count per level.
        let mut counters = [0u32; 10];
        for (pi, p) in tx.children_named("a:p").enumerate() {
            let ppr = p.child("a:pPr");
            let level = ppr.and_then(|x| x.attr_i64("lvl")).unwrap_or(0).clamp(0, 8) as u32;
            let mut levels = self.level_props(sp, chain, level);
            if let Some(own) = ppr { levels.insert(0, own.clone()); }
            let pattr = |k: &str| levels.iter().find_map(|l| l.attr(k).map(String::from));
            let pchild = |k: &str| levels.iter().find_map(|l| l.child(k).cloned());
            let spacing = |k: &str| -> Spacing {
                match pchild(k) {
                    Some(s) => if let Some(p) = s.child("a:spcPct").and_then(|p| p.attr_f64("val")) { Spacing::Percent(p / 100_000.0) } else if let Some(p) = s.child("a:spcPts").and_then(|p| p.attr_f64("val")) { Spacing::Points(p / 100.0) } else { Spacing::Auto },
                    None => Spacing::Auto,
                }
            };
            // Run defaults: each level's defRPr, highest priority first.
            let defs: Vec<Element> = levels.iter().filter_map(|l| l.child("a:defRPr").cloned()).collect();
            let mut para = Para {
                runs: vec![],
                align: match pattr("algn").as_deref() { Some("ctr") => Align::Center, Some("r") => Align::Right, Some("just") | Some("dist") => Align::Justify, _ => Align::Left },
                level,
                margin: pattr("marL").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                indent: pattr("indent").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                before: spacing("a:spcBef"),
                after: spacing("a:spcAft"),
                line: spacing("a:lnSpc"),
                bullet: None,
                rtl: pattr("rtl").is_some_and(|v| v == "1"),
                end_size: 18.0,
                end_font: String::new(),
            };
            let run_props = |rpr: Option<&Element>| -> Run {
                let all: Vec<&Element> = rpr.into_iter().chain(defs.iter()).collect();
                let ra = |k: &str| all.iter().find_map(|e| e.attr(k));
                let rc = |k: &str| all.iter().find_map(|e| e.child(k));
                let size = ra("sz").and_then(|v| v.parse::<f64>().ok()).map(|v| v / 100.0).unwrap_or(18.0);
                let face = rc("a:latin").and_then(|l| l.attr("typeface")).or_else(|| rc("a:ea").and_then(|l| l.attr("typeface"))).unwrap_or("+mn-lt");
                let color = all.iter().find_map(|e| {
                    let f = e.child("a:solidFill")?;
                    color::resolve_child(f, &self.ctx)
                }).or_else(|| all.iter().find_map(|e| e.child("a:gradFill").and_then(|g| g.path("a:gsLst/a:gs")).and_then(|g| color::resolve_child(g, &self.ctx))))
                    .unwrap_or_else(|| self.ctx.scheme_color("tx1").unwrap_or(Rgba::BLACK));
                let caps = ra("cap");
                Run {
                    font: self.theme.typeface(face),
                    size,
                    bold: ra("b").is_some_and(|v| v == "1" || v == "true"),
                    italic: ra("i").is_some_and(|v| v == "1" || v == "true"),
                    underline: ra("u").is_some_and(|v| v != "none"),
                    strike: ra("strike").is_some_and(|v| v != "noStrike"),
                    color,
                    baseline: ra("baseline").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / 100_000.0,
                    caps: caps == Some("all"),
                    small_caps: caps == Some("small"),
                    spacing: ra("spc").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / 100.0,
                    highlight: rc("a:highlight").and_then(|h| color::resolve_child(h, &self.ctx)),
                    outline: rc("a:ln").and_then(|ln| { let c = ln.child("a:solidFill").and_then(|f| color::resolve_child(f, &self.ctx))?; Some((c, ln.attr_f64("w").unwrap_or(9_525.0))) }),
                    shadow: rc("a:effectLst").and_then(|e| Effects::read(e, &self.ctx).shadow),
                    glow: rc("a:effectLst").and_then(|e| Effects::read(e, &self.ctx).glow),
                    gradient: all.iter().find_map(|e| e.child("a:gradFill")).map(|g| { let mut v: Vec<(f32, Rgba)> = g.path("a:gsLst").map(|l| l.children_named("a:gs").filter_map(|s| Some(((s.attr_f64("pos").unwrap_or(0.0) / 100_000.0) as f32, color::resolve_child(s, &self.ctx)?))).collect()).unwrap_or_default(); v.sort_by(|a, b| a.0.total_cmp(&b.0)); v }).filter(|v| v.len() > 1),
                    double_strike: ra("strike") == Some("dblStrike"),
                    underline_style: ra("u").filter(|u| *u != "none").unwrap_or("").to_string(),
                    link: rpr.and_then(|r| r.child("a:hlinkClick")).and_then(|h| h.attr("r:id")).and_then(|id| self.pres.pkg.rel_external(&self.cur_part(), id)),
                    ..Default::default()
                }
            };
            for (ri, r) in p.elements().enumerate() {
                match r.local() {
                    "r" => {
                        let mut run = run_props(r.child("a:rPr"));
                        run.text = r.child("a:t").map(|t| t.text()).unwrap_or_default();
                        run.src = (pi, ri);
                        para.runs.push(run);
                    }
                    "br" => { let mut run = run_props(r.child("a:rPr")); run.br = true; run.src = (pi, ri); para.runs.push(run); }
                    "fld" => {
                        let mut run = run_props(r.child("a:rPr"));
                        run.text = match r.attr("type") {
                            Some("slidenum") if self.number > 0 => self.number.to_string(),
                            _ => r.child("a:t").map(|t| t.text()).unwrap_or_default(),
                        };
                        run.src = (pi, ri);
                        para.runs.push(run);
                    }
                    _ => {}
                }
            }
            let end = run_props(p.child("a:endParaRPr"));
            para.end_size = end.size;
            para.end_font = end.font.clone();
            // Bullets: buNone / buChar / buAutoNum, first found in the chain.
            let first_size = para.runs.first().map(|r| r.size).unwrap_or(end.size);
            let first_color = para.runs.first().map(|r| r.color).unwrap_or(end.color);
            for l in &levels {
                if l.child("a:buNone").is_some() { break; }
                let color = l.child("a:buClr").and_then(|c| color::resolve_child(c, &self.ctx)).or(Some(first_color));
                let size = l.child("a:buSzPct").and_then(|s| s.attr_f64("val")).map(|v| first_size * v / 100_000.0).unwrap_or(first_size);
                if let Some(ch) = l.child("a:buChar") {
                    let font = levels.iter().find_map(|x| x.child("a:buFont")).and_then(|f| f.attr("typeface")).map(|f| self.theme.typeface(f));
                    para.bullet = Some(Bullet::Char { ch: ch.attr("char").unwrap_or("•").to_string(), font, color, size });
                    break;
                }
                if let Some(n) = l.child("a:buAutoNum") {
                    let start = n.attr_i64("startAt").unwrap_or(1) as u32;
                    counters[level as usize] += 1;
                    for c in counters.iter_mut().skip(level as usize + 1) { *c = 0; }
                    para.bullet = Some(Bullet::Number { scheme: n.attr("type").unwrap_or("arabicPeriod").to_string(), value: start + counters[level as usize] - 1, color, size });
                    break;
                }
            }
            if !matches!(para.bullet, Some(Bullet::Number { .. })) { counters[level as usize] = 0; }
            // Empty paragraphs show no bullet.
            if para.runs.iter().all(|r| r.text.is_empty() && !r.br) { para.bullet = None; }
            body.paras.push(para);
        }
        body
    }
}


/// Text of a numbered bullet ("1.", "a)", "iv.").
pub fn number_text(scheme: &str, n: u32) -> String {
    fn roman(mut n: u32) -> String {
        let table = [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
        let mut s = String::new();
        for (v, r) in table { while n >= v { s.push_str(r); n -= v; } }
        s
    }
    let alpha = |n: u32| { let mut s = String::new(); let mut k = n; while k > 0 { k -= 1; s.insert(0, (b'a' + (k % 26) as u8) as char); k /= 26; } s };
    let (body, wrap) = if scheme.starts_with("alphaLc") { (alpha(n), &scheme[7..]) } else if scheme.starts_with("alphaUc") { (alpha(n).to_uppercase(), &scheme[7..]) }
        else if scheme.starts_with("romanLc") { (roman(n), &scheme[7..]) } else if scheme.starts_with("romanUc") { (roman(n).to_uppercase(), &scheme[7..]) }
        else if let Some(rest) = scheme.strip_prefix("arabic") { (n.to_string(), rest) } else { (n.to_string(), "Period") };
    match wrap { "ParenBoth" => format!("({body})"), "ParenR" => format!("{body})"), "Plain" => body, "Minus" => format!("- {body} -"), _ => format!("{body}.") }
}

impl Presentation {
    /// The scene for a slide (or a layout or master, drawn on its own).
    pub fn scene(&self, slide: &str) -> Scene {
        let number = self.slides().iter().position(|s| s == slide).map(|i| i + 1).unwrap_or(0);
        let res = Res::new(self, slide, number);
        let (w, h) = self.size();
        let mut items = vec![];
        let root = self.pkg.xml(slide);
        let is_slide = root.is_some_and(|r| r.is("p:sld"));
        let is_layout = root.is_some_and(|r| r.is("p:sldLayout"));
        let show = |part: &Option<String>| part.as_deref().and_then(|p| self.pkg.xml(p)).and_then(|r| r.attr("showMasterSp")).is_none_or(|v| v != "0");
        let slide_shows_master = root.and_then(|r| r.attr("showMasterSp")).is_none_or(|v| v != "0");
        // Master and layout decorations (their non-placeholder shapes).
        if (is_slide || is_layout) && slide_shows_master && (is_layout || show(&res.layout)) {
            if let Some(m) = &res.master { self.collect(&res, m, false, &mut items); }
        }
        if is_slide && slide_shows_master {
            if let Some(l) = &res.layout { self.collect(&res, l, false, &mut items); }
        }
        self.collect(&res, slide, true, &mut items);
        Scene { w, h, background: self.background(&res, slide), items }
    }

    fn background(&self, res: &Res, slide: &str) -> Fill {
        let parts = [Some(slide.to_string()), res.layout.clone(), res.master.clone()];
        for p in parts.iter().flatten() {
            *res.part.borrow_mut() = p.clone();
            let Some(bg) = self.pkg.xml(p).and_then(|r| r.path("p:cSld/p:bg")) else { continue };
            if let Some(pr) = bg.child("p:bgPr") {
                if let Some(f) = pr.elements().find_map(|e| res.fill_from(e, &res.ctx)) { return f; }
            }
            if let Some(r) = bg.child("p:bgRef") {
                let idx = r.attr("idx").and_then(|i| i.parse::<usize>().ok()).unwrap_or(0);
                let mut ctx = res.ctx.clone();
                ctx.ph = color::resolve_child(r, &res.ctx);
                let style = if idx >= 1001 { res.theme.bg_fill_styles.get(idx - 1001) } else if idx >= 1 { res.theme.fill_styles.get(idx - 1) } else { None };
                if let Some(f) = style.and_then(|s| res.fill_from(s, &ctx)) { return f; }
                if let Some(c) = ctx.ph { return Fill::Solid(c); }
            }
        }
        Fill::Solid(res.ctx.scheme_color("bg1").unwrap_or(Rgba::WHITE))
    }

    fn collect(&self, res: &Res, part: &str, own: bool, out: &mut Vec<Item>) {
        let Some(root) = self.pkg.xml(part) else { return };
        let Some(tree) = tree(root) else { return };
        *res.part.borrow_mut() = part.to_string();
        for (i, sp) in tree.elements().enumerate() {
            // A layout's or master's placeholders aren't drawn on the slide.
            if !own && ph_of(sp).is_some() { continue; }
            self.item(res, part, sp, vec![i], Affine::IDENTITY, own, None, out);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn item(&self, res: &Res, part: &str, sp: &Element, path: Vec<usize>, parent: Affine, own: bool, group: Option<usize>, out: &mut Vec<Item>) {
        let el = if sp.local() == "AlternateContent" { sp.child("mc:Choice").and_then(|c| c.first_element()).or_else(|| sp.child("mc:Fallback").and_then(|c| c.first_element())) } else { Some(sp) };
        let Some(sp) = el else { return };
        let nv = sp.elements().find(|e| e.local().starts_with("nv"));
        let cnv = nv.and_then(|n| n.child("p:cNvPr"));
        let id = cnv.and_then(|c| c.attr_i64("id")).unwrap_or(0) as u32;
        let name = cnv.and_then(|c| c.attr("name")).unwrap_or("").to_string();
        let hidden = cnv.and_then(|c| c.attr_bool("hidden")).unwrap_or(false);
        let link = cnv.and_then(|c| c.child("a:hlinkClick")).and_then(|h| h.attr("r:id")).and_then(|i| self.pkg.rel_external(part, i));
        let (lay_ph, mas_ph) = res.ph_chain(sp, part);
        let chain = [lay_ph, mas_ph];
        let xfrm_el = match sp.local() {
            "grpSp" => sp.path("p:grpSpPr/a:xfrm"),
            "graphicFrame" => sp.child("p:xfrm"),
            _ => sp.path("p:spPr/a:xfrm"),
        }.or_else(|| chain.iter().flatten().find_map(|a| a.path("p:spPr/a:xfrm")));
        let Some(xfrm_el) = xfrm_el else { return };
        let xf = Xfrm::read(xfrm_el);
        let transform = parent.then(&xf.matrix());
        let ph = ph_of(sp);
        let mk = |kind: Kind, effects: Effects| Item {
            loc: Loc { part: part.to_string(), path: path.clone() }, id, name: name.clone(), transform, w: xf.w, h: xf.h, kind, effects, opacity: 1.0, own,
            placeholder: ph.clone(), link: link.clone(), hidden, group,
        };
        match sp.local() {
            "grpSp" => {
                // Children live in chOff/chExt space, mapped onto the group's frame.
                let ch_off = xfrm_el.child("a:chOff");
                let ch_ext = xfrm_el.child("a:chExt");
                let (cx, cy) = (ch_off.and_then(|o| o.attr_f64("x")).unwrap_or(0.0), ch_off.and_then(|o| o.attr_f64("y")).unwrap_or(0.0));
                let (cw, chh) = (ch_ext.and_then(|o| o.attr_f64("cx")).unwrap_or(xf.w), ch_ext.and_then(|o| o.attr_f64("cy")).unwrap_or(xf.h));
                let sx = if cw.abs() > 0.0 { xf.w / cw } else { 1.0 };
                let sy = if chh.abs() > 0.0 { xf.h / chh } else { 1.0 };
                let inner = transform.then(&Affine::scale(sx, sy)).then(&Affine::translate(-cx, -cy));
                let top = group.or(Some(path[0]));
                for (k, c) in sp.elements().enumerate() {
                    if matches!(c.local(), "sp" | "pic" | "grpSp" | "graphicFrame" | "cxnSp" | "AlternateContent") {
                        let mut p = path.clone();
                        p.push(k);
                        self.item(res, part, c, p, inner, own, top, out);
                    }
                }
            }
            "sp" | "cxnSp" => {
                let sp_pr = sp.child("p:spPr").filter(|s| s.child("a:prstGeom").is_some() || s.child("a:custGeom").is_some())
                    .or_else(|| chain.iter().flatten().find_map(|a| a.child("p:spPr").filter(|s| s.child("a:prstGeom").is_some() || s.child("a:custGeom").is_some())))
                    .or(sp.child("p:spPr"));
                let geom = geom::shape_geometry(sp_pr, xf.w, xf.h);
                let fill = if sp.local() == "cxnSp" { Fill::None } else { res.resolve_fill(sp, &chain) };
                let line = res.resolve_line(sp, &chain);
                let text = sp.child("p:txBody").map(|tx| res.text_body(sp, &chain, tx, &fill));
                // A placeholder on the slide with no text still shows its prompt (editor only).
                let text = text.or_else(|| ph.as_ref().map(|_| res.text_body(sp, &chain, &Element::new("p:txBody"), &fill)));
                out.push(mk(Kind::Shape { geom, fill, line, text }, res.effects(sp)));
            }
            "pic" => {
                let blip = sp.path("p:blipFill/a:blip");
                let Some(target) = blip.and_then(|b| b.attr("r:embed")).and_then(|id| self.pkg.rel_target(part, id)) else { return };
                let src = sp.path("p:blipFill/a:srcRect");
                let c = |k: &str| src.and_then(|s| s.attr_f64(k)).unwrap_or(0.0) / 100_000.0;
                let geom = geom::shape_geometry(sp.child("p:spPr"), xf.w, xf.h);
                let line = res.resolve_line(sp, &chain);
                let mut it = mk(Kind::Picture { part: target, crop: [c("l"), c("t"), c("r"), c("b")], geom, line }, res.effects(sp));
                if let Some(a) = blip.and_then(|b| b.child("a:alphaModFix")).and_then(|a| a.attr_f64("amt")) { it.opacity = (a / 100_000.0) as f32; }
                out.push(it);
            }
            "graphicFrame" => {
                let data = sp.path("a:graphic/a:graphicData");
                let uri = data.and_then(|d| d.attr("uri")).unwrap_or("");
                if let Some(tbl) = data.and_then(|d| d.child("a:tbl")) {
                    let t = self.table(res, tbl);
                    let mut it = mk(Kind::Table(t.clone()), Effects::default());
                    // The frame is the grid's real size (rows grow to fit their text).
                    it.w = t.cols.iter().sum::<f64>().max(1.0);
                    it.h = t.rows.iter().sum::<f64>().max(1.0);
                    out.push(it);
                } else if uri.ends_with("/diagram") {
                    // SmartArt: draw its cached drawing when present.
                    let drawing = data.and_then(|d| d.child("dgm:relIds")).and_then(|r| r.attr("r:dm"))
                        .and_then(|dm| self.pkg.rel_target(part, dm))
                        .and_then(|dm| self.pkg.rels(&dm).into_iter().find(|r| r.kind.ends_with("/diagramDrawing")).map(|r| crate::opc::resolve(&dm, &r.target)))
                        .or_else(|| self.pkg.rels(part).into_iter().find(|r| r.kind.ends_with("/diagramDrawing")).map(|r| crate::opc::resolve(part, &r.target)));
                    match drawing.as_deref().and_then(|d| self.pkg.xml(d).map(|x| (d, x))) {
                        Some((dpart, dx)) => {
                            if let Some(t) = dx.find("dsp:spTree") {
                                let saved = res.part.replace(dpart.to_string());
                                let inner = transform;
                                for (k, c) in t.elements().enumerate() {
                                    if matches!(c.local(), "sp" | "grpSp") {
                                        let shape = rename_dsp(c);
                                        let mut items = vec![];
                                        self.item(res, dpart, &shape, vec![k], Affine::translate(0.0, 0.0), false, None, &mut items);
                                        for mut it in items {
                                            it.transform = inner.then(&Affine::translate(-xf.x, -xf.y)).then(&it.transform);
                                            it.transform = Affine::translate(0.0, 0.0).then(&it.transform);
                                            it.loc = Loc { part: part.to_string(), path: path.clone() };
                                            it.own = own;
                                            it.group = Some(path[0]);
                                            out.push(it);
                                        }
                                    }
                                }
                                *res.part.borrow_mut() = saved;
                            }
                        }
                        None => out.push(mk(Kind::Frame { label: "SmartArt".into(), image: None }, Effects::default())),
                    }
                } else if let Some(chart) = uri.ends_with("/chart").then(|| sp.find("c:chart").and_then(|c| c.attr("r:id")).and_then(|rid| self.chart_part(part, rid, xf.w, xf.h))).flatten() {
                    out.push(mk(Kind::Chart(chart), Effects::default()));
                } else {
                    let label = if uri.ends_with("/chart") { "Chart" } else if uri.contains("ole") { "Object" } else { "Content" };
                    // OLE objects carry a preview picture.
                    let image = sp.find("p:pic").and_then(|p| p.path("p:blipFill/a:blip")).and_then(|b| b.attr("r:embed")).and_then(|id| self.pkg.rel_target(part, id));
                    out.push(mk(Kind::Frame { label: label.into(), image }, Effects::default()));
                }
            }
            _ => {}
        }
    }

    fn table(&self, res: &Res, tbl: &Element) -> Table {
        let cols: Vec<f64> = tbl.path("a:tblGrid").map(|g| g.children_named("a:gridCol").map(|c| c.attr_f64("w").unwrap_or(0.0)).collect()).unwrap_or_default();
        let pr = tbl.child("a:tblPr");
        let flag = |k: &str| pr.and_then(|p| p.attr_bool(k)).unwrap_or(false);
        let (first_row, band_row, first_col, last_row) = (flag("firstRow"), flag("bandRow"), flag("firstCol"), flag("lastRow"));
        // The style: its accent, or no style / plain grid.
        let style_id = pr.and_then(|p| p.child("a:tableStyleId")).map(|e| e.text().trim().to_string());
        let token = pr.and_then(|p| p.attr("qalamAccent")).map(String::from)
            .or_else(|| style_id.as_deref().and_then(|id| crate::table::TABLE_STYLES.iter().find(|s| s.0.eq_ignore_ascii_case(id)).map(|s| s.2.to_string())))
            .unwrap_or_else(|| "accent1".into());
        let grid_only = token == "grid";
        let no_style = token == "none";
        let accent = if token == "tx1" { res.ctx.scheme_color("tx1").unwrap_or(Rgba::BLACK) } else { res.ctx.scheme_color(&token).unwrap_or(Rgba::rgb(0x44, 0x72, 0xC4)) };
        let tint = |c: Rgba, t: f32| Rgba::rgb((c.r as f32 + (255.0 - c.r as f32) * t) as u8, (c.g as f32 + (255.0 - c.g as f32) * t) as u8, (c.b as f32 + (255.0 - c.b as f32) * t) as u8);
        let has_style = style_id.is_some() && !grid_only && !no_style;
        let mut rows = vec![];
        let mut cells = vec![];
        let nrows = tbl.children_named("a:tr").count();
        for (r, tr) in tbl.children_named("a:tr").enumerate() {
            rows.push(tr.attr_f64("h").unwrap_or(370_840.0));
            for (c, tc) in tr.children_named("a:tc").enumerate() {
                if tc.attr_bool("hMerge") == Some(true) || tc.attr_bool("vMerge") == Some(true) { continue; }
                let tcpr = tc.child("a:tcPr");
                let header = (first_row && r == 0) || (last_row && r + 1 == nrows) || (first_col && c == 0);
                // Medium Style 2 – Accent 1 look when the table uses a style and the cell has no fill.
                let style_fill = if !has_style { Fill::None } else if header { Fill::Solid(accent) } else if band_row && (r + first_row as usize) % 2 == 1 { Fill::Solid(tint(accent, 0.8)) } else { Fill::Solid(tint(accent, 0.9)) };
                let fill = tcpr.and_then(|p| p.elements().find_map(|e| if matches!(e.local(), "noFill" | "solidFill" | "gradFill" | "blipFill") { res.fill_from(e, &res.ctx) } else { None })).unwrap_or(style_fill);
                let border = |n: &str| -> Option<Line> {
                    match tcpr.and_then(|p| p.child(n)).and_then(|l| res.line_from(l, &res.ctx)) {
                        Some(l) => l,
                        None if has_style => Some(Line { color: Rgba::WHITE, width: 12_700.0, dash: vec![], head: None, tail: None }),
                        None if grid_only => Some(Line { color: Rgba::BLACK, width: 12_700.0, dash: vec![], head: None, tail: None }),
                        None => None,
                    }
                };
                let empty = Element::new("p:sp");
                let mut text = tc.child("a:txBody").map(|tx| {
                    // Cell bodies use a:txBody with a:bodyPr; reuse the shape resolver.
                    let mut sp = Element::new("p:sp");
                    let mut t = tx.clone();
                    t.name = "p:txBody".into();
                    sp.children.push(crate::xml::Node::Element(t.clone()));
                    res.text_body(&sp, &[], &t, &fill)
                }).unwrap_or_else(|| res.text_body(&empty, &[], &Element::new("p:txBody"), &fill));
                let m = |k: &str, d: f64| tcpr.and_then(|p| p.attr_f64(k)).unwrap_or(d);
                text.insets = [m("marL", 91_440.0), m("marT", 45_720.0), m("marR", 91_440.0), m("marB", 45_720.0)];
                text.anchor = match tcpr.and_then(|p| p.attr("anchor")) { Some("ctr") => Anchor::Middle, Some("b") => Anchor::Bottom, _ => Anchor::Top };
                if has_style && header { for p in &mut text.paras { for r in &mut p.runs { if !tc_has_color(tc) { r.color = Rgba::WHITE; } r.bold = true; } } }
                cells.push(Cell {
                    row: r, col: c,
                    row_span: tc.attr_i64("rowSpan").unwrap_or(1).max(1) as usize, col_span: tc.attr_i64("gridSpan").unwrap_or(1).max(1) as usize,
                    fill, borders: [border("a:lnL"), border("a:lnT"), border("a:lnR"), border("a:lnB")], text,
                });
            }
        }
        // Rows grow to fit their text (PowerPoint keeps h as a minimum).
        let mut rows = rows;
        for c in &cells {
            if c.row_span != 1 { continue; }
            let w: f64 = cols.iter().skip(c.col).take(c.col_span).sum();
            let (_, _, bw, _) = crate::text::place(&c.text, w, 1e9, None);
            let laid = crate::text::layout(&c.text, bw.max(1.0), 1e9);
            let empty_h = c.text.paras.first().map(|p| p.end_size * 1.2).unwrap_or(18.0 * 1.2);
            let need = (laid.height.max(empty_h)) * 12_700.0 + c.text.insets[1] + c.text.insets[3];
            if let Some(h) = rows.get_mut(c.row) { *h = h.max(need); }
        }
        Table { cols, rows, cells }
    }
}

fn tc_has_color(tc: &Element) -> bool { let mut v = vec![]; tc.find_all("a:solidFill", &mut v); v.iter().any(|_| true) && tc.find("a:rPr").is_some_and(|r| r.child("a:solidFill").is_some()) }

/// A SmartArt drawing shape (dsp:sp) as a slide shape (p:sp), so the same resolver draws it.
fn rename_dsp(e: &Element) -> Element {
    let mut c = e.clone();
    fn walk(e: &mut Element) {
        if let Some(rest) = e.name.strip_prefix("dsp:") {
            e.name = match rest { "txXfrm" => "p:txXfrm".into(), other => format!("p:{other}") };
        }
        for ch in e.elements_mut() { walk(ch); }
    }
    walk(&mut c);
    c
}

/// Size helpers.
pub fn pt_to_emu(pt: f64) -> f64 { pt * EMU_PER_PT }

/// For callers outside: a slide's color context (theme and map), for the editor's pickers.
pub fn color_context(pres: &Presentation, slide: &str) -> (Theme, ColorMap) {
    let m = pres.master_for(slide);
    (m.as_deref().map(|m| pres.theme(m)).unwrap_or_default(), pres.color_map(slide))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_slides_resolve() {
        let p = Presentation::open(include_bytes!("../tests/fixtures/sample.pptx")).unwrap();
        let slides = p.slides();
        let s = p.scene(&slides[0]);
        assert_eq!((s.w, s.h), (12_192_000.0, 6_858_000.0));
        assert!(matches!(s.background, Fill::Solid(_)));
        // The title placeholder inherits its frame from the layout and its font from the theme.
        let title = s.items.iter().find(|i| i.placeholder.as_ref().is_some_and(|p| p.0 == "ctrTitle")).expect("title");
        assert!(title.w > 0.0 && title.h > 0.0);
        let Kind::Shape { text: Some(t), .. } = &title.kind else { panic!("title text") };
        assert_eq!(t.paras[0].runs[0].text, "Qalam Slides");
        assert!(t.paras[0].runs[0].size > 30.0, "title size {}", t.paras[0].runs[0].size);
        assert!(!t.paras[0].runs[0].font.is_empty() && !t.paras[0].runs[0].font.starts_with('+'));
        // Body text: three bulleted paragraphs.
        let s2 = p.scene(&slides[1]);
        let body = s2.items.iter().find_map(|i| match &i.kind { Kind::Shape { text: Some(t), .. } if t.paras.len() == 3 => Some(t), _ => None }).expect("body");
        assert_eq!(body.paras[1].runs[0].text, "Second point");
        assert!(body.paras[0].bullet.is_some());
    }

    #[test]
    fn numbering() {
        assert_eq!(number_text("arabicPeriod", 3), "3.");
        assert_eq!(number_text("alphaLcParenR", 2), "b)");
        assert_eq!(number_text("romanUcPeriod", 4), "IV.");
    }

    #[test]
    fn group_transform() {
        let x = Xfrm { x: 100.0, y: 50.0, w: 20.0, h: 10.0, rot: 90.0, flip_h: false, flip_v: false };
        let (px, py) = x.matrix().apply(0.0, 0.0);
        // Rotated 90° about the center (110, 55): top-left goes to (115, 45).
        assert!((px - 115.0).abs() < 1e-9 && (py - 45.0).abs() < 1e-9);
    }
}
