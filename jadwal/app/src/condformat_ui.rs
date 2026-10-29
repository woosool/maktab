//! Conditional formatting (ui/condformat.js, condformat-quick.js, data-bar.js,
//! conditional-icon.js): the overlay the grid draws (formats, color scales, data bars, icons),
//! the Conditional Formatting menu with its quick rules and galleries, the New/Edit Rule dialog
//! and the Rules Manager.

use crate::app::SheetApp;
use crate::grid::Layout;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui};
use qalam_kit::colors::*;
use sheet_engine::addr::{parse_range, Pos, Range};
use sheet_engine::condformat::{cf_at, icon_count, Bar, CfKind, Cfvo, CondFormat, DatePeriod, Dxf, Op, TextOp, VoKind, ICON_SETS};
use sheet_engine::style::{Border, LineStyle, Rgb, Style};
use std::borrow::Cow;

// ---- the grid's overlay -------------------------------------------------------------------------

/// A cell's style with its conditional format applied.
pub fn cell_style<'a>(app: &SheetApp, si: usize, q: Pos, st: &'a Style) -> Cow<'a, Style> {
    match cf_at(&app.book, si, q) {
        Some(cf) if !cf.dxf.is_empty() || cf.fill.is_some() => { let mut s = st.clone(); cf.apply(&mut s); Cow::Owned(s) }
        _ => Cow::Borrowed(st),
    }
}

/// A data bar or icon rule set to show only the bar or icon.
pub fn hide_value(app: &SheetApp, si: usize, q: Pos) -> bool { cf_at(&app.book, si, q).is_some_and(|c| c.hide_value) }

/// Data bars and icons over the cells of `area`.
pub fn draw(app: &SheetApp, p: &Painter, lay: &Layout, area: Range) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    if !s.cond_formats.iter().any(|c| matches!(c.kind, CfKind::DataBar { .. } | CfKind::IconSet { .. }) && c.ranges.iter().any(|r| r.intersects(&area))) { return; }
    for r in area.r0..=area.r1 {
        if lay.rows.size(r) <= 0.0 { continue; }
        for c in area.c0..=area.c1 {
            let q = Pos::new(r, c);
            if lay.cols.size(c) <= 0.0 || s.merge_at(q).is_some_and(|m| m.start() != q) { continue; }
            let Some(cf) = cf_at(&app.book, si, q) else { continue };
            let rc = s.merge_at(q).map(|m| lay.range_rect(m)).unwrap_or_else(|| lay.cell_rect(q));
            if let Some(b) = &cf.bar { draw_bar(p, rc, b, lay.rtl); }
            if let Some((set, k)) = &cf.icon {
                let sz = (16.0 * lay.z).min(rc.height() - 2.0).max(4.0);
                let x = if lay.rtl { rc.max.x - sz - 2.0 } else { rc.min.x + 2.0 };
                paint_icon(p, Rect::from_min_size(pos2(x, rc.center().y - sz / 2.0), vec2(sz, sz)), set, *k);
            }
        }
    }
}

fn rgb(c: Rgb) -> Color32 { Color32::from_rgb(c[0], c[1], c[2]) }

/// A rectangle shaded from `a` on the left to `b` on the right.
fn gradient(p: &Painter, r: Rect, a: Color32, b: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.left_top(), a); m.colored_vertex(r.right_top(), b); m.colored_vertex(r.right_bottom(), b); m.colored_vertex(r.left_bottom(), a);
    m.add_triangle(0, 1, 2); m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
}

fn draw_bar(p: &Painter, cell: Rect, b: &Bar, rtl: bool) {
    let inner = cell.shrink2(vec2(2.0, 2.0));
    let x = |f: f32| if rtl { inner.max.x - f * inner.width() } else { inner.min.x + f * inner.width() };
    let (x0, x1) = (x(b.lo).min(x(b.hi)), x(b.lo).max(x(b.hi)));
    if x1 - x0 >= 0.5 {
        let r = Rect::from_min_max(pos2(x0, inner.min.y), pos2(x1, inner.max.y));
        let c = rgb(b.color);
        if b.gradient {
            // Fades away from the axis (from the left, or from the right for negative bars).
            let fade = Color32::from_rgba_unmultiplied(255, 255, 255, 0).lerp_to_gamma(c, 0.12);
            let towards_left = b.axis.is_some_and(|a| b.hi <= a) != rtl;
            if towards_left { gradient(p, r, fade, c); } else { gradient(p, r, c, fade); }
        } else { p.rect_filled(r, 0.0, c); }
        if let Some(bc) = b.border { p.rect_stroke(r, 0.0, Stroke::new(1.0, rgb(bc)), StrokeKind::Inside); }
        else if b.gradient { p.rect_stroke(r, 0.0, Stroke::new(1.0, c), StrokeKind::Inside); }
    }
    if let Some(a) = b.axis {
        let ax = x(a);
        let mut y = inner.min.y;
        while y < inner.max.y { p.line_segment([pos2(ax, y), pos2(ax, (y + 2.0).min(inner.max.y))], Stroke::new(1.0, Color32::BLACK)); y += 4.0; }
    }
}

// ---- icons --------------------------------------------------------------------------------------

const RED: Color32 = Color32::from_rgb(0xE0, 0x3E, 0x3E);
const YELLOW: Color32 = Color32::from_rgb(0xF2, 0xB1, 0x00);
const GREEN: Color32 = Color32::from_rgb(0x2E, 0xA0, 0x4A);
const GRAY: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
const BLACK: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
const GOLD: Color32 = Color32::from_rgb(0xF2, 0xB1, 0x00);
const BLUE: Color32 = Color32::from_rgb(0x3A, 0x6E, 0xC6);

enum Glyph { Arrow(f32), Circle, Flag, Check, Bang, Cross, Diamond, Triangle, Star(f32), Dash, TriUp, TriDown, Bars(u8, u8), Pie(f32), Boxes(u8) }

/// The icon `idx` (0 = lowest values) of a set: its shape, color, and whether it sits in a disc.
fn glyph(set: &str, idx: u8) -> (Glyph, Color32, bool) {
    let n = icon_count(set) as u8;
    let i = idx.min(n - 1);
    let tri = [RED, YELLOW, GREEN];
    let ramp = |i: u8| if i == 0 { RED } else if i + 1 == n { GREEN } else { YELLOW };
    let dir = |i: u8| -> f32 { match (n, i) { (3, 0) | (4, 0) | (5, 0) => 180.0, (3, 1) | (5, 2) => 90.0, (4, 1) | (5, 1) => 135.0, (4, 2) | (5, 3) => 45.0, _ => 0.0 } };
    match set {
        "3Arrows" | "4Arrows" | "5Arrows" => (Glyph::Arrow(dir(i)), ramp(i), false),
        "3ArrowsGray" | "4ArrowsGray" | "5ArrowsGray" => (Glyph::Arrow(dir(i)), GRAY, false),
        "3Flags" => (Glyph::Flag, tri[i as usize], false),
        "3TrafficLights1" | "3TrafficLights2" => (Glyph::Circle, tri[i as usize], set.ends_with('2')),
        "4TrafficLights" => (Glyph::Circle, [BLACK, RED, YELLOW, GREEN][i as usize], false),
        "4RedToBlack" => (Glyph::Circle, [BLACK, GRAY, Color32::from_rgb(0xF0, 0x9A, 0x9A), RED][i as usize], false),
        "3Signs" => ([Glyph::Diamond, Glyph::Triangle, Glyph::Circle].into_iter().nth(i as usize).unwrap(), tri[i as usize], false),
        "3Symbols" | "3Symbols2" => ([Glyph::Cross, Glyph::Bang, Glyph::Check].into_iter().nth(i as usize).unwrap(), tri[i as usize], set == "3Symbols"),
        "3Stars" => (Glyph::Star(i as f32 / 2.0), GOLD, false),
        "3Triangles" => ([Glyph::TriDown, Glyph::Dash, Glyph::TriUp].into_iter().nth(i as usize).unwrap(), tri[i as usize], false),
        "4Rating" => (Glyph::Bars(i + 1, 4), BLUE, false),
        "5Rating" => (Glyph::Bars(i, 4), BLUE, false),
        "5Quarters" => (Glyph::Pie(i as f32 / 4.0), BLACK, false),
        "5Boxes" => (Glyph::Boxes(i), BLUE, false),
        _ => (Glyph::Circle, ramp(i), false),
    }
}

/// A fan of triangles from `c` through `pts` (stars and pie slices).
fn fan(p: &Painter, c: Pos2, pts: &[Pos2], col: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(c, col);
    for q in pts { m.colored_vertex(*q, col); }
    for k in 1..pts.len() as u32 { m.add_triangle(0, k, k + 1); }
    p.add(Shape::mesh(m));
}

pub fn paint_icon(p: &Painter, r: Rect, set: &str, idx: u8) {
    let (g, col, disc) = glyph(set, idx);
    let c = r.center();
    let s = r.width() / 2.0;
    let white = Color32::WHITE;
    let ink = if disc { white } else { col };
    let w = (s * 0.28).max(1.2);
    if disc { p.circle_filled(c, s * 0.95, col); }
    match g {
        Glyph::Arrow(deg) => {
            let a = deg.to_radians();
            let rot = |x: f32, y: f32| c + vec2(x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos()) * s;
            p.add(Shape::convex_polygon(vec![rot(0.0, -0.9), rot(0.75, -0.05), rot(-0.75, -0.05)], col, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![rot(-0.3, -0.1), rot(0.3, -0.1), rot(0.3, 0.85), rot(-0.3, 0.85)], col, Stroke::NONE));
        }
        Glyph::Circle => { if !disc { p.circle_filled(c, s * 0.85, col); p.circle_stroke(c, s * 0.85, Stroke::new(1.0, col.gamma_multiply(0.6))); } else { p.circle_filled(c, s * 0.7, col); p.circle_stroke(c, s * 0.95, Stroke::new(1.5, BLACK)); } }
        Glyph::Flag => {
            p.line_segment([c + vec2(-0.6, -0.9) * s, c + vec2(-0.6, 0.9) * s], Stroke::new(w * 0.6, BLACK));
            p.add(Shape::convex_polygon(vec![c + vec2(-0.55, -0.85) * s, c + vec2(0.8, -0.45) * s, c + vec2(-0.55, -0.05) * s], col, Stroke::NONE));
        }
        Glyph::Check => { p.add(Shape::line(vec![c + vec2(-0.5, 0.0) * s, c + vec2(-0.15, 0.4) * s, c + vec2(0.5, -0.4) * s], Stroke::new(w, ink))); }
        Glyph::Bang => { p.line_segment([c + vec2(0.0, -0.55) * s, c + vec2(0.0, 0.15) * s], Stroke::new(w, ink)); p.circle_filled(c + vec2(0.0, 0.45) * s, w * 0.6, ink); }
        Glyph::Cross => { p.line_segment([c + vec2(-0.4, -0.4) * s, c + vec2(0.4, 0.4) * s], Stroke::new(w, ink)); p.line_segment([c + vec2(0.4, -0.4) * s, c + vec2(-0.4, 0.4) * s], Stroke::new(w, ink)); }
        Glyph::Diamond => { p.add(Shape::convex_polygon(vec![c + vec2(0.0, -0.9) * s, c + vec2(0.9, 0.0) * s, c + vec2(0.0, 0.9) * s, c + vec2(-0.9, 0.0) * s], col, Stroke::NONE)); }
        Glyph::Triangle | Glyph::TriUp => { p.add(Shape::convex_polygon(vec![c + vec2(0.0, -0.8) * s, c + vec2(0.9, 0.7) * s, c + vec2(-0.9, 0.7) * s], col, Stroke::NONE)); }
        Glyph::TriDown => { p.add(Shape::convex_polygon(vec![c + vec2(-0.9, -0.7) * s, c + vec2(0.9, -0.7) * s, c + vec2(0.0, 0.8) * s], col, Stroke::NONE)); }
        Glyph::Dash => { p.rect_filled(Rect::from_center_size(c, vec2(1.4 * s, 0.45 * s)), 1.0, col); }
        Glyph::Star(fill) => {
            let pts: Vec<Pos2> = (0..=10).map(|k| { let a = (k as f32 * 36.0 - 90.0).to_radians(); let rr = if k % 2 == 0 { 0.95 } else { 0.4 }; c + vec2(a.cos(), a.sin()) * s * rr }).collect();
            if fill > 0.0 { fan(&p.with_clip_rect(Rect::from_min_max(r.min, pos2(r.min.x + r.width() * fill, r.max.y)).intersect(p.clip_rect())), c, &pts, col); }
            p.add(Shape::closed_line(pts[..10].to_vec(), Stroke::new(1.0, col)));
        }
        Glyph::Bars(on, n) => {
            let bw = r.width() / (n as f32 * 1.5);
            for k in 0..n {
                let h = r.height() * (0.3 + 0.7 * (k + 1) as f32 / n as f32) * 0.9;
                let x = r.min.x + k as f32 * bw * 1.5 + bw * 0.25;
                let br = Rect::from_min_max(pos2(x, r.max.y - h), pos2(x + bw, r.max.y - r.height() * 0.05));
                if k < on { p.rect_filled(br, 0.0, col); } else { p.rect_stroke(br, 0.0, Stroke::new(1.0, col.gamma_multiply(0.5)), StrokeKind::Inside); }
            }
        }
        Glyph::Pie(f) => {
            p.circle_stroke(c, s * 0.85, Stroke::new(1.0, col));
            if f >= 1.0 { p.circle_filled(c, s * 0.85, col); } else if f > 0.0 {
                let pts: Vec<Pos2> = (0..=24).map(|k| { let a = (-90.0 + 360.0 * f * k as f32 / 24.0).to_radians(); c + vec2(a.cos(), a.sin()) * s * 0.85 }).collect();
                fan(p, c, &pts, col);
            }
        }
        Glyph::Boxes(on) => {
            let h = s * 0.85;
            for k in 0..4u8 {
                let (x, y) = ((k % 2) as f32, (k / 2) as f32);
                let br = Rect::from_min_size(c + vec2(-h + x * h * 1.05, -h + y * h * 1.05), vec2(h * 0.95, h * 0.95));
                if k < on { p.rect_filled(br, 1.0, col); } else { p.rect_stroke(br, 1.0, Stroke::new(1.0, col.gamma_multiply(0.5)), StrokeKind::Inside); }
            }
        }
    }
}

// ---- presets ------------------------------------------------------------------------------------

/// Formats the quick rules offer (Excel's presets).
pub const QUICK_STYLES: [(&str, Option<Rgb>, Option<Rgb>, bool); 6] = [
    ("Light Red Fill with Dark Red Text", Some([0xFF, 0xC7, 0xCE]), Some([0x9C, 0x00, 0x06]), false),
    ("Yellow Fill with Dark Yellow Text", Some([0xFF, 0xEB, 0x9C]), Some([0x9C, 0x57, 0x00]), false),
    ("Green Fill with Dark Green Text", Some([0xC6, 0xEF, 0xCE]), Some([0x00, 0x61, 0x00]), false),
    ("Light Red Fill", Some([0xFF, 0xC7, 0xCE]), None, false),
    ("Red Text", None, Some([0x9C, 0x00, 0x06]), false),
    ("Red Border", None, None, true),
];

fn quick_dxf(i: usize) -> Dxf {
    let (_, fill, color, border) = QUICK_STYLES[i.min(QUICK_STYLES.len() - 1)];
    Dxf { fill, color, border: border.then_some(Border { style: LineStyle::Thin, color: Some([0xC0, 0x00, 0x00]) }), ..Default::default() }
}

const BAR_COLORS: [(&str, Rgb); 6] = [("Blue", [0x63, 0x8E, 0xC6]), ("Green", [0x63, 0xC3, 0x84]), ("Red", [0xFF, 0x55, 0x5A]), ("Orange", [0xFF, 0xB6, 0x28]), ("Light Blue", [0x00, 0x8A, 0xEF]), ("Purple", [0xD6, 0x00, 0x7B])];

const SCALES: [(&str, &[Rgb]); 12] = [
    ("Green - Yellow - Red", &[[0x63, 0xBE, 0x7B], [0xFF, 0xEB, 0x84], [0xF8, 0x69, 0x6B]]), ("Red - Yellow - Green", &[[0xF8, 0x69, 0x6B], [0xFF, 0xEB, 0x84], [0x63, 0xBE, 0x7B]]),
    ("Green - White - Red", &[[0x63, 0xBE, 0x7B], [0xFC, 0xFC, 0xFF], [0xF8, 0x69, 0x6B]]), ("Red - White - Green", &[[0xF8, 0x69, 0x6B], [0xFC, 0xFC, 0xFF], [0x63, 0xBE, 0x7B]]),
    ("Blue - White - Red", &[[0x5A, 0x8A, 0xC6], [0xFC, 0xFC, 0xFF], [0xF8, 0x69, 0x6B]]), ("Red - White - Blue", &[[0xF8, 0x69, 0x6B], [0xFC, 0xFC, 0xFF], [0x5A, 0x8A, 0xC6]]),
    ("White - Red", &[[0xFC, 0xFC, 0xFF], [0xF8, 0x69, 0x6B]]), ("Red - White", &[[0xF8, 0x69, 0x6B], [0xFC, 0xFC, 0xFF]]),
    ("Green - White", &[[0x63, 0xBE, 0x7B], [0xFC, 0xFC, 0xFF]]), ("White - Green", &[[0xFC, 0xFC, 0xFF], [0x63, 0xBE, 0x7B]]),
    ("Green - Yellow", &[[0x63, 0xBE, 0x7B], [0xFF, 0xEF, 0x9C]]), ("Yellow - Green", &[[0xFF, 0xEF, 0x9C], [0x63, 0xBE, 0x7B]]),
];

const ICON_GROUPS: [(&str, &[&str]); 4] = [
    ("Directional", &["3Arrows", "3ArrowsGray", "3Triangles", "4Arrows", "4ArrowsGray", "5Arrows", "5ArrowsGray"]),
    ("Shapes", &["3TrafficLights1", "3TrafficLights2", "3Signs", "4TrafficLights", "4RedToBlack"]),
    ("Indicators", &["3Symbols", "3Symbols2", "3Flags"]),
    ("Ratings", &["3Stars", "4Rating", "5Quarters", "5Rating", "5Boxes"]),
];

/// A rule's value box as a formula: =formula, a number, or quoted text.
pub fn value_formula(t: &str) -> String {
    let t = t.trim();
    if let Some(f) = t.strip_prefix('=') { return f.trim().to_string(); }
    if t.is_empty() || t.parse::<f64>().is_ok() { return t.to_string(); }
    format!("\"{}\"", t.replace('"', "\"\""))
}

/// The inverse of `value_formula`, for showing a rule's value.
pub fn formula_value(f: &str) -> String {
    if f.is_empty() || f.parse::<f64>().is_ok() { return f.to_string(); }
    if let Some(s) = f.strip_prefix('"').and_then(|s| s.strip_suffix('"')) { if !s.replace("\"\"", "").contains('"') { return s.replace("\"\"", "\""); } }
    format!("={f}")
}

// ---- state --------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QuickKind { Greater, Less, Between, Equal, Contains, Date, Duplicate, Top, TopPct, Bottom, BottomPct, Above, Below }

impl QuickKind {
    fn title(&self) -> &'static str {
        match self {
            QuickKind::Greater => "Greater Than", QuickKind::Less => "Less Than", QuickKind::Between => "Between", QuickKind::Equal => "Equal To", QuickKind::Contains => "Text That Contains",
            QuickKind::Date => "A Date Occurring", QuickKind::Duplicate => "Duplicate Values", QuickKind::Top => "Top 10 Items", QuickKind::TopPct => "Top 10%", QuickKind::Bottom => "Bottom 10 Items",
            QuickKind::BottomPct => "Bottom 10%", QuickKind::Above => "Above Average", QuickKind::Below => "Below Average",
        }
    }
    fn prompt(&self) -> &'static str {
        match self {
            QuickKind::Greater => "Format cells that are GREATER THAN:", QuickKind::Less => "Format cells that are LESS THAN:", QuickKind::Between => "Format cells that are BETWEEN:",
            QuickKind::Equal => "Format cells that are EQUAL TO:", QuickKind::Contains => "Format cells that contain the text:", QuickKind::Date => "Format cells that contain a date occurring:",
            QuickKind::Duplicate => "Format cells that contain:", QuickKind::Top | QuickKind::TopPct => "Format cells that rank in the TOP:", QuickKind::Bottom | QuickKind::BottomPct => "Format cells that rank in the BOTTOM:",
            QuickKind::Above => "Format cells that are ABOVE AVERAGE:", QuickKind::Below => "Format cells that are BELOW AVERAGE:",
        }
    }
}

#[derive(Clone)]
pub struct Quick { kind: QuickKind, a: String, b: String, period: DatePeriod, unique: bool, rank: u32, style: usize, ranges: Vec<Range> }

/// The New/Edit Rule dialog: the rule being made, its Applies To text, and where it goes
/// (an index into the manager's list, or straight onto the sheet).
#[derive(Clone)]
pub struct Editor { cf: CondFormat, sqref: String, f1: String, f2: String, target: Option<usize>, in_manager: bool, new: bool }

#[derive(Clone)]
pub struct Manager { rules: Vec<CondFormat>, sel: Option<usize>, sheet_wide: bool }

#[derive(Clone, Default)]
pub struct State { pub quick: Option<Quick>, pub editor: Option<Editor>, pub manager: Option<Manager> }

fn id() -> egui::Id { egui::Id::new("jadwal-condformat") }
fn state(ctx: &egui::Context) -> State { ctx.data(|d| d.get_temp::<State>(id()).unwrap_or_default()) }
fn set_state(ctx: &egui::Context, s: State) { ctx.data_mut(|d| d.insert_temp(id(), s)); }

fn add_rule(app: &mut SheetApp, cf: CondFormat) {
    app.checkpoint("Conditional Formatting");
    // A new rule goes first: it has the highest priority.
    let si = app.sheet();
    app.book.sheets[si].cond_formats.insert(0, cf);
    app.book.cf_dirty();
    app.dirty = true;
}

fn open_quick(app: &SheetApp, kind: QuickKind) {
    let si = app.sheet();
    // Excel suggests a value from the selection's numbers.
    let st = app.book.selection_stats(si, &app.sel.ranges());
    let avg = if st.nums > 0 { sheet_engine::value::fmt_num((st.sum / st.nums as f64 * 100.0).round() / 100.0) } else { String::new() };
    let (a, b) = match kind { QuickKind::Between => (sheet_engine::value::fmt_num(st.min), sheet_engine::value::fmt_num(st.max)), QuickKind::Contains => (app.book.display(si, app.sel.active), String::new()), _ => (avg, String::new()) };
    let rank = if matches!(kind, QuickKind::Top | QuickKind::TopPct | QuickKind::Bottom | QuickKind::BottomPct) { 10 } else { 0 };
    let mut s = state(&app.ctx);
    s.quick = Some(Quick { kind, a, b, period: DatePeriod::Yesterday, unique: false, rank, style: 0, ranges: app.sel.ranges() });
    set_state(&app.ctx, s);
}

/// Opens the New Formatting Rule dialog for the selection.
pub fn open_new_rule(app: &SheetApp) {
    let ranges = app.sel.ranges();
    let cf = CondFormat::new(ranges.clone(), CfKind::CellIs { op: Op::Greater, f1: String::new(), f2: String::new() }, quick_dxf(0));
    let mut s = state(&app.ctx);
    s.editor = Some(Editor { sqref: sqref_text(&ranges), cf, f1: String::new(), f2: String::new(), target: None, in_manager: false, new: true });
    set_state(&app.ctx, s);
}

pub fn open_manager(app: &SheetApp) {
    let mut s = state(&app.ctx);
    s.manager = Some(Manager { rules: app.book.sheets[app.sheet()].cond_formats.clone(), sel: None, sheet_wide: false });
    set_state(&app.ctx, s);
}

/// Ranges as absolute references ("$A$1:$A$10,$C$2"), as Applies To shows them.
pub fn sqref_text(r: &[Range]) -> String {
    let abs = |part: &str| { let i = part.find(|c: char| c.is_ascii_digit()).unwrap_or(part.len()); let (c, n) = part.split_at(i); format!("{}{}", if c.is_empty() { String::new() } else { format!("${c}") }, if n.is_empty() { String::new() } else { format!("${n}") }) };
    r.iter().map(|r| r.a1().split(':').map(abs).collect::<Vec<_>>().join(":")).collect::<Vec<_>>().join(",")
}
pub fn parse_sqref(t: &str) -> Vec<Range> { t.split([',', ' ', ';']).filter(|x| !x.trim().is_empty()).filter_map(|x| parse_range(x.trim().trim_start_matches('='))).collect() }

// ---- the menu -----------------------------------------------------------------------------------

/// The Conditional Formatting drop-down's contents (the Home tab puts it under its button).
pub fn cond_format_menu(app: &mut SheetApp, ui: &mut Ui) {
    let quick = |app: &mut SheetApp, ui: &mut Ui, kind: QuickKind| if ui.button(format!("{}…", tr(kind.title()))).clicked() { open_quick(app, kind); ui.close(); };
    crate::theme::submenu(ui, tr("Highlight Cells Rules"), |ui| {
        for k in [QuickKind::Greater, QuickKind::Less, QuickKind::Between, QuickKind::Equal, QuickKind::Contains, QuickKind::Date, QuickKind::Duplicate] { quick(app, ui, k); }
        ui.separator();
        if ui.button(tr("More Rules…")).clicked() { open_new_rule(app); ui.close(); }
    });
    crate::theme::submenu(ui, tr("Top/Bottom Rules"), |ui| {
        for k in [QuickKind::Top, QuickKind::TopPct, QuickKind::Bottom, QuickKind::BottomPct, QuickKind::Above, QuickKind::Below] { quick(app, ui, k); }
        ui.separator();
        if ui.button(tr("More Rules…")).clicked() { open_new_rule(app); ui.close(); }
    });
    crate::theme::submenu(ui, tr("Data Bars"), |ui| {
        for (label, gradient) in [("Gradient Fill", true), ("Solid Fill", false)] {
            qalam_kit::widgets::menu_heading(ui, label);
            ui.horizontal(|ui| for (name, c) in BAR_COLORS {
                let (r, resp) = ui.allocate_exact_size(vec2(34.0, 30.0), Sense::click());
                if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
                for (k, f) in [0.9f32, 0.6, 0.3].iter().enumerate() {
                    let row = Rect::from_min_size(r.min + vec2(4.0, 4.0 + k as f32 * 8.0), vec2(26.0, 7.0));
                    draw_bar(ui.painter(), row.expand(2.0), &Bar { lo: 0.0, hi: *f, color: c, gradient, border: None, axis: None }, false);
                }
                if resp.on_hover_text(format!("{} {}", tr(name), tr("Data Bar"))).clicked() { add_rule(app, CondFormat::new(app.sel.ranges(), CfKind::data_bar(c, gradient), Dxf::default())); ui.close(); }
            });
        }
        ui.separator();
        if ui.button(tr("More Rules…")).clicked() { open_new_rule(app); ui.close(); }
    });
    crate::theme::submenu(ui, tr("Color Scales"), |ui| {
        for row in SCALES.chunks(4) {
            ui.horizontal(|ui| for (name, colors) in row {
                let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
                for k in 0..3 {
                    let t = k as f64 / 2.0;
                    let c = if colors.len() == 3 { if t <= 0.5 { mix(colors[0], colors[1], t * 2.0) } else { mix(colors[1], colors[2], t * 2.0 - 1.0) } } else { mix(colors[0], colors[1], t) };
                    ui.painter().rect_filled(Rect::from_min_size(r.min + vec2(5.0, 4.0 + k as f32 * 9.0), vec2(24.0, 8.0)), 1.0, rgb(c));
                }
                if resp.on_hover_text(tr(name)).clicked() { add_rule(app, CondFormat::new(app.sel.ranges(), CfKind::color_scale(colors), Dxf::default())); ui.close(); }
            });
        }
        ui.separator();
        if ui.button(tr("More Rules…")).clicked() { open_new_rule(app); ui.close(); }
    });
    crate::theme::submenu(ui, tr("Icon Sets"), |ui| {
        for (group, sets) in ICON_GROUPS {
            qalam_kit::widgets::menu_heading(ui, group);
            for set in sets.iter() {
                let n = icon_count(set);
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(120.0), 22.0), Sense::click());
                if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
                for k in 0..n { paint_icon(ui.painter(), Rect::from_min_size(r.min + vec2(8.0 + k as f32 * 20.0, 3.0), vec2(16.0, 16.0)), set, (n - 1 - k) as u8); }
                if resp.clicked() { add_rule(app, CondFormat::new(app.sel.ranges(), CfKind::icon_set(set), Dxf::default())); ui.close(); }
            }
        }
        ui.separator();
        if ui.button(tr("More Rules…")).clicked() { open_new_rule(app); ui.close(); }
    });
    ui.separator();
    if ui.button(tr("New Rule…")).clicked() { open_new_rule(app); ui.close(); }
    crate::theme::submenu(ui, tr("Clear Rules"), |ui| {
        let si = app.sheet();
        if ui.button(tr("Clear Rules from Selected Cells")).clicked() {
            app.checkpoint("Clear Rules");
            for r in app.sel.ranges() { app.book.sheets[si].clear_rules_in(r); }
            app.dirty = true; ui.close();
        }
        if ui.button(tr("Clear Rules from Entire Sheet")).clicked() {
            app.checkpoint("Clear Rules");
            app.book.sheets[si].cond_formats.clear(); app.book.cf_dirty();
            app.dirty = true; ui.close();
        }
    });
    if ui.button(tr("Manage Rules…")).clicked() { open_manager(app); ui.close(); }
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb { [0, 1, 2].map(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t.clamp(0.0, 1.0)).round() as u8) }

// ---- dialogs ------------------------------------------------------------------------------------

/// Draws whichever conditional formatting dialogs are open.
pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let mut s = state(ctx);
    if s.quick.is_none() && s.editor.is_none() && s.manager.is_none() { return; }
    // The rule editor sits over the manager when both are open.
    if let Some(mut e) = s.editor.take() {
        if let Some(done) = editor(app, ctx, &mut e) {
            match (done, e.in_manager) {
                (Some(cf), true) => if let Some(m) = s.manager.as_mut() { match e.target { Some(i) if i < m.rules.len() => m.rules[i] = cf, _ => { m.rules.insert(0, cf); m.sel = Some(0); } } },
                (Some(cf), false) => add_rule(app, cf),
                _ => {}
            }
        } else { s.editor = Some(e); }
    } else if let Some(mut m) = s.manager.take() {
        match manager(app, ctx, &mut m) {
            Some(Action::Close) => {}
            Some(Action::Edit(i)) => { s.editor = Some(edit_existing(&m.rules[i], Some(i))); s.manager = Some(m); }
            Some(Action::New) => { let mut e = edit_existing(&CondFormat::new(app.sel.ranges(), CfKind::CellIs { op: Op::Greater, f1: String::new(), f2: String::new() }, quick_dxf(0)), None); e.new = true; s.editor = Some(e); s.manager = Some(m); }
            None => s.manager = Some(m),
        }
    } else if let Some(mut q) = s.quick.take() {
        if quick_dialog(app, ctx, &mut q) { s.quick = Some(q); }
    }
    set_state(ctx, s);
}

fn edit_existing(cf: &CondFormat, target: Option<usize>) -> Editor {
    let (f1, f2) = match &cf.kind { CfKind::CellIs { f1, f2, .. } => (formula_value(f1), formula_value(f2)), CfKind::Expression(f) => (format!("={f}"), String::new()), _ => (String::new(), String::new()) };
    Editor { sqref: sqref_text(&cf.ranges), cf: cf.clone(), f1, f2, target, in_manager: true, new: false }
}

fn quick_dialog(app: &mut SheetApp, ctx: &egui::Context, q: &mut Quick) -> bool {
    let mut open = true;
    let mut keep = true;
    crate::theme::dialog(ctx, q.kind.title()).open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(440.0);
        ui.label(tr(q.kind.prompt()));
        ui.add_space(4.0);
        crate::theme::hrow(ui, |ui| {
            match q.kind {
                QuickKind::Greater | QuickKind::Less | QuickKind::Equal | QuickKind::Contains => { ui.add(egui::TextEdit::singleline(&mut q.a).desired_width(160.0)); }
                QuickKind::Between => { ui.add(egui::TextEdit::singleline(&mut q.a).desired_width(110.0)); ui.label(tr("and")); ui.add(egui::TextEdit::singleline(&mut q.b).desired_width(110.0)); }
                QuickKind::Date => { crate::theme::Combo::from_id_salt("cf-date").width(160.0).selected_text(tr(q.period.label())).show_ui(ui, |ui| for (d, _, l) in DatePeriod::ALL { ui.selectable_value(&mut q.period, d, tr(l)); }); }
                QuickKind::Duplicate => { crate::theme::Combo::from_id_salt("cf-dup").width(120.0).selected_text(tr(if q.unique { "Unique" } else { "Duplicate" })).show_ui(ui, |ui| { ui.selectable_value(&mut q.unique, false, tr("Duplicate")); ui.selectable_value(&mut q.unique, true, tr("Unique")); }); ui.label(tr("values with")); }
                QuickKind::Top | QuickKind::Bottom => { ui.add(egui::DragValue::new(&mut q.rank).range(1..=1000)); }
                QuickKind::TopPct | QuickKind::BottomPct => { ui.add(egui::DragValue::new(&mut q.rank).range(1..=100)); ui.label("%"); }
                QuickKind::Above | QuickKind::Below => {}
            }
            ui.label(tr(if matches!(q.kind, QuickKind::Duplicate) { "" } else { "with" }));
            crate::theme::Combo::from_id_salt("cf-quick-style").width(240.0).selected_text(tr(QUICK_STYLES[q.style].0)).show_ui(ui, |ui| for (i, st) in QUICK_STYLES.iter().enumerate() { ui.selectable_value(&mut q.style, i, tr(st.0)); });
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let kind = match q.kind {
                    QuickKind::Greater => CfKind::CellIs { op: Op::Greater, f1: value_formula(&q.a), f2: String::new() },
                    QuickKind::Less => CfKind::CellIs { op: Op::Less, f1: value_formula(&q.a), f2: String::new() },
                    QuickKind::Equal => CfKind::CellIs { op: Op::Equal, f1: value_formula(&q.a), f2: String::new() },
                    QuickKind::Between => CfKind::CellIs { op: Op::Between, f1: value_formula(&q.a), f2: value_formula(&q.b) },
                    QuickKind::Contains => CfKind::Text { op: TextOp::Contains, text: q.a.clone() },
                    QuickKind::Date => CfKind::Date(q.period),
                    QuickKind::Duplicate => CfKind::Duplicate { unique: q.unique },
                    QuickKind::Top | QuickKind::TopPct | QuickKind::Bottom | QuickKind::BottomPct => CfKind::Top { bottom: matches!(q.kind, QuickKind::Bottom | QuickKind::BottomPct), percent: matches!(q.kind, QuickKind::TopPct | QuickKind::BottomPct), rank: q.rank.max(1) },
                    QuickKind::Above | QuickKind::Below => CfKind::Average { below: q.kind == QuickKind::Below, equal: false, std_dev: 0 },
                };
                add_rule(app, CondFormat::new(q.ranges.clone(), kind, quick_dxf(q.style)));
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

/// The rule types of the New Formatting Rule dialog.
const CATEGORIES: [&str; 6] = ["Format all cells based on their values", "Format only cells that contain", "Format only top or bottom ranked values",
    "Format only values that are above or below average", "Format only unique or duplicate values", "Use a formula to determine which cells to format"];

fn category(k: &CfKind) -> usize {
    match k {
        CfKind::ColorScale { .. } | CfKind::DataBar { .. } | CfKind::IconSet { .. } => 0,
        CfKind::CellIs { .. } | CfKind::Text { .. } | CfKind::Date(_) | CfKind::Blanks(_) | CfKind::Errors(_) => 1,
        CfKind::Top { .. } => 2, CfKind::Average { .. } => 3, CfKind::Duplicate { .. } => 4, CfKind::Expression(_) => 5,
    }
}

fn default_of(cat: usize) -> CfKind {
    match cat {
        0 => CfKind::color_scale(SCALES[0].1), 1 => CfKind::CellIs { op: Op::Between, f1: String::new(), f2: String::new() },
        2 => CfKind::Top { bottom: false, percent: false, rank: 10 }, 3 => CfKind::Average { below: false, equal: false, std_dev: 0 },
        4 => CfKind::Duplicate { unique: false }, _ => CfKind::Expression(String::new()),
    }
}

fn color_field(ui: &mut Ui, c: &mut Rgb) {
    let mut col = word_engine::Color(c[0], c[1], c[2]);
    if qalam_kit::widgets::color_button(ui, &mut col) { *c = [col.0, col.1, col.2]; }
}

fn opt_color(ui: &mut Ui, label: &str, c: &mut Option<Rgb>, default: Rgb) {
    let mut on = c.is_some();
    if crate::theme::check(ui, &mut on, tr(label)).changed() { *c = on.then_some(c.unwrap_or(default)); }
    if let Some(x) = c.as_mut() { color_field(ui, x); }
}

fn opt_flag(ui: &mut Ui, label: &str, v: &mut Option<bool>) {
    let mut on = *v == Some(true);
    if crate::theme::check(ui, &mut on, tr(label)).changed() { *v = on.then_some(true); }
}

/// The format part of a rule (Format Cells for a differential format, simplified).
fn dxf_editor(ui: &mut Ui, d: &mut Dxf) {
    crate::theme::section(ui, "Format");
    crate::theme::hrow(ui, |ui| { opt_flag(ui, "Bold", &mut d.bold); opt_flag(ui, "Italic", &mut d.italic); opt_flag(ui, "Strikethrough", &mut d.strike);
        let mut u = d.underline.is_some_and(|u| u != sheet_engine::style::Underline::None);
        if crate::theme::check(ui, &mut u, tr("Underline")).changed() { d.underline = u.then_some(sheet_engine::style::Underline::Single); } });
    crate::theme::hrow(ui, |ui| { opt_color(ui, "Font color", &mut d.color, [0x9C, 0x00, 0x06]); ui.add_space(12.0); opt_color(ui, "Fill", &mut d.fill, [0xFF, 0xC7, 0xCE]); });
    crate::theme::hrow(ui, |ui| {
        let mut bc = d.border.map(|b| b.color);
        let mut on = bc.is_some();
        if crate::theme::check(ui, &mut on, tr("Border")).changed() { bc = on.then_some(Some([0xC0, 0, 0])); }
        if let Some(c) = bc.as_mut() { let mut x = c.unwrap_or([0, 0, 0]); color_field(ui, &mut x); *c = Some(x); }
        d.border = bc.map(|c| Border { style: LineStyle::Thin, color: c });
        ui.add_space(12.0);
        let mut nf = d.num_fmt.is_some();
        if crate::theme::check(ui, &mut nf, tr("Number format")).changed() { d.num_fmt = nf.then(|| "0.00".to_string()); }
        if let Some(f) = d.num_fmt.as_mut() { ui.add(egui::TextEdit::singleline(f).desired_width(110.0)); }
    });
    // A preview of the format.
    let mut st = Style::default();
    d.apply(&mut st);
    let (r, _) = ui.allocate_exact_size(vec2(220.0, 26.0), Sense::hover());
    ui.painter().rect(r, 2.0, st.fill.map(rgb).unwrap_or(Color32::WHITE), Stroke::new(if d.border.is_some() { 1.5 } else { 1.0 }, d.border.and_then(|b| b.color).map(rgb).unwrap_or(c_line())), StrokeKind::Inside);
    let font = egui::FontId::new(13.0, if st.bold { egui::FontFamily::Name("ui-bold".into()) } else { egui::FontFamily::Proportional });
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, tr("AaBbCcYyZz"), font, st.color.map(rgb).unwrap_or(Color32::BLACK));
}

const VO_LABELS: [(VoKind, &str); 8] = [(VoKind::Min, "Lowest value"), (VoKind::Max, "Highest value"), (VoKind::Num, "Number"), (VoKind::Percent, "Percent"), (VoKind::Percentile, "Percentile"), (VoKind::Formula, "Formula"), (VoKind::AutoMin, "Automatic"), (VoKind::AutoMax, "Automatic")];

fn vo_editor(ui: &mut Ui, salt: impl std::hash::Hash + std::fmt::Debug + Copy, v: &mut Cfvo, allowed: &[VoKind]) {
    let label = |k: VoKind| tr(VO_LABELS.iter().find(|x| x.0 == k).map_or("Number", |x| x.1));
    crate::theme::Combo::from_id_salt(("vo", salt)).width(120.0).selected_text(label(v.kind)).show_ui(ui, |ui| for k in allowed { ui.selectable_value(&mut v.kind, *k, label(*k)); });
    ui.add_enabled(!matches!(v.kind, VoKind::Min | VoKind::Max | VoKind::AutoMin | VoKind::AutoMax), egui::TextEdit::singleline(&mut v.val).desired_width(90.0));
}

fn values_editor(ui: &mut Ui, kind: &mut CfKind) {
    let style = match kind { CfKind::ColorScale { colors, .. } if colors.len() == 2 => 0, CfKind::ColorScale { .. } => 1, CfKind::DataBar { .. } => 2, _ => 3 };
    let mut ns = style;
    crate::theme::hrow(ui, |ui| {
        ui.label(tr("Format Style:"));
        crate::theme::Combo::from_id_salt("cf-style").width(160.0).selected_text(tr(["2-Color Scale", "3-Color Scale", "Data Bar", "Icon Sets"][style])).show_ui(ui, |ui| for (i, l) in ["2-Color Scale", "3-Color Scale", "Data Bar", "Icon Sets"].iter().enumerate() { ui.selectable_value(&mut ns, i, tr(l)); });
    });
    if ns != style { *kind = match ns { 0 => CfKind::color_scale(SCALES[6].1), 1 => CfKind::color_scale(SCALES[0].1), 2 => CfKind::data_bar(BAR_COLORS[0].1, true), _ => CfKind::icon_set("3TrafficLights1") }; }
    let vk = [VoKind::Min, VoKind::Num, VoKind::Percent, VoKind::Percentile, VoKind::Formula, VoKind::Max];
    match kind {
        CfKind::ColorScale { vos, colors } => {
            let names = if colors.len() == 3 { ["Minimum", "Midpoint", "Maximum"].as_slice() } else { ["Minimum", "Maximum"].as_slice() };
            for (i, n) in names.iter().enumerate() {
                if i >= vos.len() || i >= colors.len() { break; }
                crate::theme::hrow(ui, |ui| { ui.label(RichText::new(tr(n)).color(c_ink2())); vo_editor(ui, ("scale", i), &mut vos[i], &vk); color_field(ui, &mut colors[i]); });
            }
        }
        CfKind::DataBar { min, max, color, negative, gradient, border, show_value } => {
            crate::theme::hrow(ui, |ui| { ui.label(tr("Minimum")); vo_editor(ui, "bar-min", min, &[VoKind::AutoMin, VoKind::Min, VoKind::Num, VoKind::Percent, VoKind::Percentile, VoKind::Formula]); });
            crate::theme::hrow(ui, |ui| { ui.label(tr("Maximum")); vo_editor(ui, "bar-max", max, &[VoKind::AutoMax, VoKind::Max, VoKind::Num, VoKind::Percent, VoKind::Percentile, VoKind::Formula]); });
            crate::theme::hrow(ui, |ui| {
                ui.label(tr("Fill")); color_field(ui, color);
                crate::theme::radio_value(ui, gradient, true, tr("Gradient")); crate::theme::radio_value(ui, gradient, false, tr("Solid"));
            });
            crate::theme::hrow(ui, |ui| { opt_color(ui, "Negative bar color", negative, [0xFF, 0, 0]); ui.add_space(12.0); opt_color(ui, "Border", border, *color); });
            let mut only = !*show_value;
            if crate::theme::check(ui, &mut only, tr("Show Bar Only")).changed() { *show_value = !only; }
        }
        CfKind::IconSet { set, vos, reverse, show_value } => {
            crate::theme::hrow(ui, |ui| {
                ui.label(tr("Icon Style:"));
                let before = set.clone();
                crate::theme::Combo::from_id_salt("cf-icons").width(180.0).selected_text(set.clone()).show_ui(ui, |ui| for s in ICON_SETS { ui.selectable_value(set, s.to_string(), *s); });
                if *set != before { *vos = sheet_engine::condformat::icon_cfvos(set); }
                crate::theme::check(ui, reverse, tr("Reverse Icon Order"));
                let mut only = !*show_value;
                if crate::theme::check(ui, &mut only, tr("Show Icon Only")).changed() { *show_value = !only; }
            });
            // Each icon from the top: shown when the value is >= (or >) its threshold.
            let n = icon_count(set).min(vos.len());
            for i in (0..n).rev() {
                crate::theme::hrow(ui, |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    paint_icon(ui.painter(), r, set, if *reverse { (n - 1 - i) as u8 } else { i as u8 });
                    if i == 0 { ui.label(tr("when the value is lower")); return; }
                    ui.label(tr("when value is"));
                    let mut gte = vos[i].gte;
                    crate::theme::Combo::from_id_salt(("gte", i)).width(50.0).selected_text(if gte { ">=" } else { ">" }).show_ui(ui, |ui| { ui.selectable_value(&mut gte, true, ">="); ui.selectable_value(&mut gte, false, ">"); });
                    vos[i].gte = gte;
                    vo_editor(ui, ("icon", i), &mut vos[i], &[VoKind::Num, VoKind::Percent, VoKind::Percentile, VoKind::Formula]);
                });
            }
        }
        _ => {}
    }
}

/// The New/Edit Formatting Rule dialog; Some(result) when it closes (Some(rule) on OK).
fn editor(app: &SheetApp, ctx: &egui::Context, e: &mut Editor) -> Option<Option<CondFormat>> {
    let mut open = true;
    let mut done = None;
    let title = if e.new { "New Formatting Rule" } else { "Edit Formatting Rule" };
    crate::theme::dialog(ctx, title).open(&mut open).resizable(false).default_width(560.0).show(ctx, |ui| {
        ui.set_min_width(540.0);
        crate::theme::section(ui, "Select a Rule Type:");
        let cat = category(&e.cf.kind);
        for (i, c) in CATEGORIES.iter().enumerate() { if ui.selectable_label(cat == i, format!("► {}", tr(c))).clicked() && cat != i { e.cf.kind = default_of(i); e.f1.clear(); e.f2.clear(); } }
        ui.separator();
        crate::theme::section(ui, "Edit the Rule Description:");
        let mut sub = match &e.cf.kind { CfKind::CellIs { .. } => 0, CfKind::Text { .. } => 1, CfKind::Date(_) => 2, CfKind::Blanks(true) => 3, CfKind::Blanks(false) => 4, CfKind::Errors(true) => 5, CfKind::Errors(false) => 6, _ => 0 };
        match &mut e.cf.kind {
            CfKind::ColorScale { .. } | CfKind::DataBar { .. } | CfKind::IconSet { .. } => values_editor(ui, &mut e.cf.kind),
            k @ (CfKind::CellIs { .. } | CfKind::Text { .. } | CfKind::Date(_) | CfKind::Blanks(_) | CfKind::Errors(_)) => {
                crate::theme::hrow(ui, |ui| {
                    ui.label(tr("Format only cells with:"));
                    let subs = ["Cell Value", "Specific Text", "Dates Occurring", "Blanks", "No Blanks", "Errors", "No Errors"];
                    let before = sub;
                    crate::theme::Combo::from_id_salt("cf-sub").width(140.0).selected_text(tr(subs[sub])).show_ui(ui, |ui| for (i, l) in subs.iter().enumerate() { ui.selectable_value(&mut sub, i, tr(l)); });
                    if sub != before { *k = match sub { 0 => CfKind::CellIs { op: Op::Between, f1: String::new(), f2: String::new() }, 1 => CfKind::Text { op: TextOp::Contains, text: String::new() }, 2 => CfKind::Date(DatePeriod::Yesterday), 3 => CfKind::Blanks(true), 4 => CfKind::Blanks(false), 5 => CfKind::Errors(true), _ => CfKind::Errors(false) }; }
                    match k {
                        CfKind::CellIs { op, .. } => {
                            crate::theme::Combo::from_id_salt("cf-op").width(170.0).selected_text(tr(op.label())).show_ui(ui, |ui| for (o, _, l) in Op::ALL { ui.selectable_value(op, o, tr(l)); });
                            ui.add(egui::TextEdit::singleline(&mut e.f1).desired_width(90.0));
                            if op.two() { ui.label(tr("and")); ui.add(egui::TextEdit::singleline(&mut e.f2).desired_width(90.0)); }
                        }
                        CfKind::Text { op, text } => {
                            crate::theme::Combo::from_id_salt("cf-textop").width(130.0).selected_text(tr(op.label())).show_ui(ui, |ui| for (o, _, _, l) in TextOp::ALL { ui.selectable_value(op, o, tr(l)); });
                            ui.add(egui::TextEdit::singleline(text).desired_width(140.0));
                        }
                        CfKind::Date(d) => { crate::theme::Combo::from_id_salt("cf-period").width(150.0).selected_text(tr(d.label())).show_ui(ui, |ui| for (p, _, l) in DatePeriod::ALL { ui.selectable_value(d, p, tr(l)); }); }
                        _ => {}
                    }
                });
            }
            CfKind::Top { bottom, percent, rank } => { crate::theme::hrow(ui, |ui| {
                ui.label(tr("Format values that rank in the"));
                crate::theme::Combo::from_id_salt("cf-top").width(90.0).selected_text(tr(if *bottom { "Bottom" } else { "Top" })).show_ui(ui, |ui| { ui.selectable_value(bottom, false, tr("Top")); ui.selectable_value(bottom, true, tr("Bottom")); });
                ui.add(egui::DragValue::new(rank).range(1..=1000));
                crate::theme::check(ui, percent, tr("% of the selected range"));
            }); }
            CfKind::Average { below, equal, std_dev } => { crate::theme::hrow(ui, |ui| {
                ui.label(tr("Format values that are:"));
                let opts: Vec<(bool, bool, u32, String)> = vec![(false, false, 0, "above".into()), (true, false, 0, "below".into()), (false, true, 0, "equal or above".into()), (true, true, 0, "equal or below".into()),
                    (false, false, 1, "1 std dev above".into()), (true, false, 1, "1 std dev below".into()), (false, false, 2, "2 std dev above".into()), (true, false, 2, "2 std dev below".into()), (false, false, 3, "3 std dev above".into()), (true, false, 3, "3 std dev below".into())];
                let cur = opts.iter().find(|o| (o.0, o.1, o.2) == (*below, *equal, *std_dev)).map(|o| o.3.clone()).unwrap_or_default();
                crate::theme::Combo::from_id_salt("cf-avg").width(150.0).selected_text(tr(&cur)).show_ui(ui, |ui| for o in &opts { if ui.selectable_label((o.0, o.1, o.2) == (*below, *equal, *std_dev), tr(&o.3)).clicked() { *below = o.0; *equal = o.1; *std_dev = o.2; } });
                ui.label(tr("the average for the selected range"));
            }); }
            CfKind::Duplicate { unique } => { crate::theme::hrow(ui, |ui| {
                ui.label(tr("Format all:"));
                crate::theme::Combo::from_id_salt("cf-dup2").width(110.0).selected_text(tr(if *unique { "unique" } else { "duplicate" })).show_ui(ui, |ui| { ui.selectable_value(unique, false, tr("duplicate")); ui.selectable_value(unique, true, tr("unique")); });
                ui.label(tr("values in the selected range"));
            }); }
            CfKind::Expression(_) => {
                ui.label(tr("Format values where this formula is true:"));
                if e.f1.is_empty() { e.f1.push('='); }
                ui.add(egui::TextEdit::singleline(&mut e.f1).desired_width(480.0).font(egui::TextStyle::Monospace));
            }
        }
        if e.cf.kind.uses_dxf() { dxf_editor(ui, &mut e.cf.dxf); }
        ui.add_space(6.0);
        crate::theme::hrow(ui, |ui| { ui.label(tr("Applies to")); crate::refedit::field(ui, &mut e.sqref, "condformat_ui.rs:672", 260.0); if e.cf.kind.uses_dxf() { crate::theme::check(ui, &mut e.cf.stop, tr("Stop If True")); } });
        let ranges = parse_sqref(&e.sqref);
        if ranges.is_empty() { ui.label(RichText::new(tr("Enter the cells the rule applies to, such as $A$1:$A$10.")).size(12.0).color(Color32::from_rgb(0xC0, 0x20, 0x20))); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, !ranges.is_empty(), "OK").clicked() {
                let mut cf = e.cf.clone();
                cf.ranges = ranges.clone();
                match &mut cf.kind {
                    CfKind::CellIs { f1, f2, .. } => { *f1 = value_formula(&e.f1); *f2 = value_formula(&e.f2); }
                    CfKind::Expression(f) => *f = e.f1.trim().trim_start_matches('=').to_string(),
                    _ => {}
                }
                done = Some(Some(cf));
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { done = Some(None); }
        });
        let _ = app;
    });
    if !open { done = Some(None); }
    done
}

enum Action { Close, Edit(usize), New }

/// A small sample of what a rule does, for the manager's list.
fn preview(ui: &mut Ui, cf: &CondFormat) {
    let (r, _) = ui.allocate_exact_size(vec2(90.0, 20.0), Sense::hover());
    let p = ui.painter();
    match &cf.kind {
        CfKind::ColorScale { colors, .. } => { let w = r.width() / colors.len().max(1) as f32; for (i, c) in colors.iter().enumerate() { p.rect_filled(Rect::from_min_size(r.min + vec2(i as f32 * w, 0.0), vec2(w, r.height())), 0.0, rgb(*c)); } }
        CfKind::DataBar { color, gradient, .. } => { p.rect_filled(r, 0.0, Color32::WHITE); draw_bar(p, r, &Bar { lo: 0.0, hi: 0.7, color: *color, gradient: *gradient, border: None, axis: None }, false); }
        CfKind::IconSet { set, .. } => { let n = icon_count(set); for k in 0..n { paint_icon(p, Rect::from_min_size(r.min + vec2(k as f32 * 18.0, 2.0), vec2(16.0, 16.0)), set, (n - 1 - k) as u8); } }
        _ => {
            let mut st = Style::default(); cf.dxf.apply(&mut st);
            p.rect(r, 0.0, st.fill.map(rgb).unwrap_or(Color32::WHITE), Stroke::new(1.0, cf.dxf.border.and_then(|b| b.color).map(rgb).unwrap_or(c_line())), StrokeKind::Inside);
            p.text(r.center(), egui::Align2::CENTER_CENTER, "AaBbCcYyZz", egui::FontId::proportional(11.0), st.color.map(rgb).unwrap_or(Color32::BLACK));
        }
    }
}

fn manager(app: &mut SheetApp, ctx: &egui::Context, m: &mut Manager) -> Option<Action> {
    let mut open = true;
    let mut act = None;
    crate::theme::dialog(ctx, "Conditional Formatting Rules Manager").open(&mut open).resizable(false).default_width(680.0).show(ctx, |ui| {
        ui.set_min_width(660.0);
        crate::theme::hrow(ui, |ui| {
            ui.label(tr("Show formatting rules for:"));
            crate::theme::Combo::from_id_salt("cf-scope").width(160.0).selected_text(tr(if m.sheet_wide { "This Worksheet" } else { "Current Selection" })).show_ui(ui, |ui| { ui.selectable_value(&mut m.sheet_wide, false, tr("Current Selection")); ui.selectable_value(&mut m.sheet_wide, true, tr("This Worksheet")); });
        });
        let sel = app.sel.ranges();
        let shown: Vec<usize> = (0..m.rules.len()).filter(|&i| m.sheet_wide || m.rules[i].ranges.iter().any(|r| sel.iter().any(|s| s.intersects(r)))).collect();
        crate::theme::hrow(ui, |ui| {
            if ui.button(tr("New Rule…")).clicked() { act = Some(Action::New); }
            if ui.add_enabled(m.sel.is_some(), egui::Button::new(tr("Edit Rule…"))).clicked() { act = m.sel.map(Action::Edit); }
            if ui.add_enabled(m.sel.is_some(), egui::Button::new(tr("Delete Rule"))).clicked() { if let Some(i) = m.sel.take() { m.rules.remove(i); } }
            let pos = m.sel.and_then(|i| shown.iter().position(|&x| x == i));
            if ui.add_enabled(pos.is_some_and(|k| k > 0), egui::Button::new("▲")).on_hover_text(tr("Move Up")).clicked() { let (k, i) = (pos.unwrap(), m.sel.unwrap()); let j = shown[k - 1]; m.rules.swap(i, j); m.sel = Some(j); }
            if ui.add_enabled(pos.is_some_and(|k| k + 1 < shown.len()), egui::Button::new("▼")).on_hover_text(tr("Move Down")).clicked() { let (k, i) = (pos.unwrap(), m.sel.unwrap()); let j = shown[k + 1]; m.rules.swap(i, j); m.sel = Some(j); }
        });
        ui.add_space(4.0);
        egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("cf-rules").num_columns(4).striped(true).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
                    for h in ["Rule (applied in order shown)", "Format", "Applies to", "Stop If True"] { ui.label(RichText::new(tr(h)).size(12.0).color(c_ink2())); }
                    ui.end_row();
                    for &i in &shown {
                        let cf = &mut m.rules[i];
                        if ui.selectable_label(m.sel == Some(i), tr(&cf.kind.describe())).clicked() { m.sel = Some(i); }
                        preview(ui, cf);
                        let mut t = sqref_text(&cf.ranges);
                        if crate::refedit::field(ui, &mut t, &format!("condformat-rule-{i}"), 150.0).changed() { let r = parse_sqref(&t); if !r.is_empty() { cf.ranges = r; } }
                        if cf.kind.uses_dxf() { ui.checkbox(&mut cf.stop, ""); } else { ui.label(""); }
                        ui.end_row();
                    }
                });
                if shown.is_empty() { ui.label(RichText::new(tr("No rules apply here.")).color(c_ink3())); }
            });
        });
        crate::theme::footer(ui, |ui| {
            let apply = |app: &mut SheetApp, rules: &Vec<CondFormat>| if *rules != app.book.sheets[app.sheet()].cond_formats {
                app.checkpoint("Conditional Formatting");
                let si = app.sheet(); app.book.sheets[si].cond_formats = rules.clone(); app.book.cf_dirty(); app.dirty = true;
            };
            if crate::theme::primary(ui, "OK").clicked() { apply(app, &m.rules); act = Some(Action::Close); }
            if crate::theme::secondary(ui, "Cancel").clicked() { act = Some(Action::Close); }
            if crate::theme::secondary(ui, "Apply").clicked() { apply(app, &m.rules); }
        });
    });
    if !open { act = Some(Action::Close); }
    act
}
