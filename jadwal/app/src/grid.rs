//! The grid (ui/grid.js): column and row headers, frozen panes, cells drawn with their formats
//! (fills, fonts, alignment, overflowing text, borders, merges), the selection with its fill
//! handle, the in-cell editor, mouse and keyboard work, scrolling and zoom.

use crate::app::{rgb, Editor, Selection, SheetApp};
use crate::i18n::tr;
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};
use qalam_kit::colors::*;
use sheet_engine::addr::{col_name, Pos, Range, MAX_COLS, MAX_ROWS};
use sheet_engine::style::{HAlign, LineStyle, Style, Underline, VAlign};
use sheet_engine::{Sheet, Value};
use std::collections::{BTreeMap, BTreeSet};

const HEADER_H: f32 = 22.0;
const SCROLLBAR: f32 = 12.0;
pub const SELECT: Color32 = Color32::from_rgb(0x21, 0x73, 0x46);

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Drag {
    #[default]
    None,
    Cells,
    Cols(u32),
    Rows(u32),
    ColSize { col: u32, from: f32, width: f32 },
    RowSize { row: u32, from: f32, height: f32 },
    Fill { src: Range, to: Range },
    Move { src: Range, grab: Pos, to: Pos },
    /// Pointing at a reference while typing a formula: the cell the drag started on.
    Point(Pos),
    VScroll { from: f32, start: f32 },
    HScroll { from: f32, start: f32 },
}

#[derive(Default)]
pub struct GridState {
    pub drag: Drag,
    /// The copied range shown with a moving dashed border.
    pub marching: Option<(usize, Range)>,
    pub last_click: Option<(Pos, f64)>,
}

/// One direction of the grid: sizes with exceptions, as prefix sums for quick lookups.
pub struct Axis { def: f32, max: u32, marks: Vec<(u32, f32)> }

impl Axis {
    fn new(def: f32, custom: &BTreeMap<u32, f32>, hidden: &BTreeSet<u32>, max: u32) -> Axis {
        let mut idx: BTreeSet<u32> = custom.keys().copied().collect();
        idx.extend(hidden.iter().copied());
        let mut acc = 0.0;
        let marks = idx.into_iter().filter(|i| *i < max).map(|i| { let s = if hidden.contains(&i) { 0.0 } else { *custom.get(&i).unwrap_or(&def) }; acc += s - def; (i, acc) }).collect();
        Axis { def, max, marks }
    }
    pub fn of_cols(s: &Sheet) -> Axis { Axis::new(s.default_col_width, &s.col_widths, &s.hidden_cols, MAX_COLS) }
    pub fn of_rows(s: &Sheet) -> Axis { Axis::new(s.default_row_height, &s.row_heights, &s.hidden_rows, MAX_ROWS) }
    /// Where line `i` starts, in unzoomed pixels.
    pub fn pos(&self, i: u32) -> f32 {
        let k = self.marks.partition_point(|m| m.0 < i);
        i as f32 * self.def + if k == 0 { 0.0 } else { self.marks[k - 1].1 }
    }
    pub fn size(&self, i: u32) -> f32 { self.pos(i + 1) - self.pos(i) }
    /// The line under `x` (unzoomed pixels from the start).
    pub fn at(&self, x: f32) -> u32 {
        let (mut lo, mut hi) = (0u32, self.max);
        while lo + 1 < hi { let mid = lo + (hi - lo) / 2; if self.pos(mid) <= x { lo = mid; } else { hi = mid; } }
        lo
    }
    pub fn end(&self) -> f32 { self.pos(self.max) }
}

/// Where things are on screen this frame.
pub struct Layout {
    pub rect: Rect,
    pub z: f32,
    pub cols: Axis,
    pub rows: Axis,
    pub fr: u32,
    /// The first frozen row and column shown.
    pub or: u32,
    pub oc: u32,
    pub fc: u32,
    pub head_w: f32,
    pub head_h: f32,
    pub scroll: egui::Vec2,
    pub rtl: bool,
    /// Page Layout view: where runs of columns and rows sit on the pages.
    pub pages: Option<std::rc::Rc<PageMap>>,
}

/// Page Layout view (page_view.rs): runs of columns and rows laid out on pages, each run as
/// (first, last, screen position of the first). A line past the last run continues it.
#[derive(Clone, Debug, Default)]
pub struct PageMap { pub cols: Vec<(u32, u32, f32)>, pub rows: Vec<(u32, u32, f32)> }

fn band_pos(axis: &Axis, bands: &[(u32, u32, f32)], i: u32, z: f32) -> Option<f32> {
    let b = bands.iter().find(|b| i <= b.1).or(bands.last())?;
    Some(b.2 + (axis.pos(i) - axis.pos(b.0)) * z)
}

fn band_at(axis: &Axis, bands: &[(u32, u32, f32)], x: f32, z: f32) -> Option<u32> {
    let b = bands.iter().rev().find(|b| x >= b.2).or(bands.first())?;
    let i = axis.at((axis.pos(b.0) + (x - b.2) / z).max(axis.pos(b.0)));
    Some(if bands.last() == Some(b) { i } else { i.min(b.1) })
}

impl Layout {
    /// The body's corner after the frozen panes.
    fn body(&self) -> Pos2 { pos2(self.rect.min.x + self.head_w + (self.cols.pos(self.fc) - self.cols.pos(self.oc)) * self.z, self.rect.min.y + self.head_h + (self.rows.pos(self.fr) - self.rows.pos(self.or)) * self.z) }
    pub fn view(&self) -> Rect { Rect::from_min_max(self.rect.min + vec2(self.head_w, self.head_h), self.rect.max - vec2(SCROLLBAR, SCROLLBAR)) }
    pub fn x(&self, c: u32) -> f32 {
        if let Some(x) = self.pages.as_ref().and_then(|m| band_pos(&self.cols, &m.cols, c, self.z)) { return x; }
        let x = if c < self.fc { self.rect.min.x + self.head_w + (self.cols.pos(c) - self.cols.pos(self.oc)) * self.z } else { self.body().x + (self.cols.pos(c) - self.cols.pos(self.fc) - self.scroll.x) * self.z };
        if self.rtl { self.rect.max.x - SCROLLBAR - (x - self.rect.min.x) - self.cols.size(c) * self.z } else { x }
    }
    pub fn y(&self, r: u32) -> f32 {
        if let Some(y) = self.pages.as_ref().and_then(|m| band_pos(&self.rows, &m.rows, r, self.z)) { return y; }
        if r < self.fr { self.rect.min.y + self.head_h + (self.rows.pos(r) - self.rows.pos(self.or)) * self.z } else { self.body().y + (self.rows.pos(r) - self.rows.pos(self.fr) - self.scroll.y) * self.z }
    }
    pub fn cell_rect(&self, p: Pos) -> Rect { Rect::from_min_size(pos2(self.x(p.col), self.y(p.row)), vec2(self.cols.size(p.col) * self.z, self.rows.size(p.row) * self.z)) }
    pub fn range_rect(&self, r: Range) -> Rect { self.cell_rect(r.start()).union(self.cell_rect(r.end())) }
    /// The column under a screen x.
    pub fn col_at(&self, x: f32) -> u32 {
        if let Some(c) = self.pages.as_ref().and_then(|m| band_at(&self.cols, &m.cols, x, self.z)) { return c; }
        let x = if self.rtl { self.rect.max.x - SCROLLBAR - (x - self.rect.min.x) } else { x };
        let local = (x - self.rect.min.x - self.head_w) / self.z;
        if local < self.cols.pos(self.fc) - self.cols.pos(self.oc) { self.cols.at(local.max(0.0) + self.cols.pos(self.oc)) } else { self.cols.at(local + self.scroll.x).max(self.fc) }
    }
    pub fn row_at(&self, y: f32) -> u32 {
        if let Some(r) = self.pages.as_ref().and_then(|m| band_at(&self.rows, &m.rows, y, self.z)) { return r; }
        let local = (y - self.rect.min.y - self.head_h) / self.z;
        if local < self.rows.pos(self.fr) - self.rows.pos(self.or) { self.rows.at(local.max(0.0) + self.rows.pos(self.or)) } else { self.rows.at(local + self.scroll.y).max(self.fr) }
    }
    pub fn cell_at(&self, p: Pos2) -> Pos { Pos::new(self.row_at(p.y), self.col_at(p.x)) }
    /// The first and last lines visible in the scrolling part.
    fn visible_rows(&self) -> (u32, u32) {
        let top = self.rows.pos(self.fr) + self.scroll.y;
        let h = (self.rect.max.y - self.body().y) / self.z;
        (self.rows.at(top).max(self.fr), (self.rows.at(top + h) + 1).min(MAX_ROWS - 1))
    }
    fn visible_cols(&self) -> (u32, u32) {
        let left = self.cols.pos(self.fc) + self.scroll.x;
        let w = (self.rect.max.x - self.body().x) / self.z;
        (self.cols.at(left).max(self.fc), (self.cols.at(left + w) + 1).min(MAX_COLS - 1))
    }
}

pub fn layout(app: &SheetApp, rect: Rect) -> Layout {
    let s = &app.book.sheets[app.sheet()];
    let z = app.zoom;
    let rows = Axis::of_rows(s);
    // The row header fits the largest row number in view.
    let last = rows.at(rows.pos(s.frozen_rows) + app.scroll.y + rect.height() / z) + 1;
    let digits = (last + 1).to_string().len().max(3) as f32;
    let head_w = if s.headers { (digits * 8.0 + 14.0) * z.max(0.6) } else { 0.0 };
    let head_h = if s.headers { HEADER_H * z.max(0.6) } else { 0.0 };
    Layout { rect, z, cols: Axis::of_cols(s), rows, fr: s.frozen_rows, fc: s.frozen_cols, or: s.frozen_origin.row.min(s.frozen_rows), oc: s.frozen_origin.col.min(s.frozen_cols), head_w, head_h, scroll: app.scroll, rtl: s.right_to_left, pages: None }
}

/// The face for a cell's font: the bundled face that best matches, else the interface font.
pub fn font_for(st: &Style, px: f32) -> FontId {
    let key = word_engine::fonts::resolve(&st.font, st.bold, st.italic);
    let fam = if (key.face as usize) < word_engine::fonts::BUNDLED.len() { FontFamily::Name(qalam_kit::fonts::face_name(key.face as usize).into()) } else if st.bold { FontFamily::Name("ui-bold".into()) } else { FontFamily::Proportional };
    FontId::new(px.max(1.0), fam)
}

fn border_stroke(b: &sheet_engine::style::Border, z: f32) -> Option<Stroke> {
    if b.style == LineStyle::None { return None; }
    Some(Stroke::new((b.style.width() * z).max(1.0), b.color.map(rgb).unwrap_or(Color32::BLACK)))
}

fn dashed(p: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke, dash: f32, gap: f32, offset: f32) {
    let len = a.distance(b);
    if len <= 0.0 { return; }
    let dir = (b - a) / len;
    let mut t = -(offset % (dash + gap));
    while t < len { let s = t.max(0.0); let e = (t + dash).min(len); if e > s { p.line_segment([a + dir * s, a + dir * e], stroke); } t += dash + gap; }
}

pub fn grid(app: &mut SheetApp, ui: &mut Ui) {
    if crate::page_view::grid_hook(app, ui) { return; }
    let full = crate::outline_ui::reserve(app, ui.available_rect_before_wrap());
    let resp = ui.allocate_rect(full, Sense::click_and_drag());
    app.grid_rect = full;
    let lay = layout(app, full);
    let painter = ui.painter_at(full);
    let si = app.sheet();
    if app.reveal { reveal(app, &lay); app.reveal = false; }
    let lay = layout(app, full);
    let view = lay.view();
    // ---- cells ----
    let (vr0, vr1) = lay.visible_rows();
    let (vc0, vc1) = lay.visible_cols();
    let body = lay.body();
    // Four panes: the frozen corner, frozen rows, frozen columns and the scrolling body.
    let (fr, fc) = (lay.fr, lay.fc);
    let (bx, by) = if lay.rtl { (view.max.x - (body.x - view.min.x), body.y) } else { (body.x, body.y) };
    let (left, right) = |a: f32, b: f32| -> (f32, f32) { (a.min(b), a.max(b)) }(view.min.x, bx);
    let frozen_x = if lay.rtl { (bx, view.max.x) } else { (left, right) };
    let body_x = if lay.rtl { (view.min.x, bx) } else { (bx, view.max.x) };
    let mut panes = vec![((vr0, vr1), (vc0, vc1), Rect::from_min_max(pos2(body_x.0, by), pos2(body_x.1, view.max.y)))];
    if fr > 0 { panes.push(((lay.or, fr - 1), (vc0, vc1), Rect::from_min_max(pos2(body_x.0, view.min.y), pos2(body_x.1, by)))); }
    if fc > 0 { panes.push(((vr0, vr1), (lay.oc, fc - 1), Rect::from_min_max(pos2(frozen_x.0, by), pos2(frozen_x.1, view.max.y)))); }
    if fr > 0 && fc > 0 { panes.push(((lay.or, fr - 1), (lay.oc, fc - 1), Rect::from_min_max(pos2(frozen_x.0, view.min.y), pos2(frozen_x.1, by)))); }
    let time = ui.input(|i| i.time);
    for ((r0, r1), (c0, c1), clip) in panes {
        if clip.width() <= 0.0 || clip.height() <= 0.0 { continue; }
        let p = painter.with_clip_rect(clip);
        crate::page_layout::paint_background(app, &p, &lay);
        draw_cells(app, &p, &lay, r0..=r1, c0..=c1);
        crate::filter_ui::draw(app, &p, &lay, r0..=r1, c0..=c1);
        crate::formulas_tab::draw_triangles(app, &p, &lay, r0..=r1, c0..=c1);
        draw_selection(app, &p, &lay, time);
    }
    crate::print::draw_breaks(app, &painter, &lay);
    crate::page_view::break_overlay(app, ui, &painter, &lay);
    // Frozen pane dividers.
    if lay.fr > 0 { painter.line_segment([pos2(view.min.x, body.y), pos2(view.max.x, body.y)], Stroke::new(1.0, c_ink3())); }
    if lay.fc > 0 { painter.line_segment([pos2(body.x, view.min.y), pos2(body.x, view.max.y)], Stroke::new(1.0, c_ink3())); }
    headers(app, &painter, &lay);
    crate::outline_ui::draw(app, ui, &lay);
    scrollbars(app, ui, &painter, &lay);
    // ---- mouse ----
    crate::formulas_tab::grid_hook(app, ui, &painter, &lay);
    crate::validation_ui::overlay(app, ui, &lay);
    mouse(app, ui, &resp, &lay);
    context_menu(app, &resp);
    crate::objects::grid_hook(app, ui, &painter, &lay, &resp);
    // ---- the in-cell editor ----
    if app.edit.as_ref().is_some_and(|e| e.sheet == si && !e.focus_bar) { cell_editor(app, ui, &lay); }
    if app.grid.marching.is_some() { ui.ctx().request_repaint_after(std::time::Duration::from_millis(80)); }
}

/// Scrolls so the active cell shows.
fn reveal(app: &mut SheetApp, lay: &Layout) {
    let p = app.sel.active;
    let view = lay.view();
    let body = lay.body();
    let (vw, vh) = ((view.max.x - body.x) / lay.z, (view.max.y - body.y) / lay.z);
    if p.col >= lay.fc {
        let x0 = lay.cols.pos(p.col) - lay.cols.pos(lay.fc); let x1 = x0 + lay.cols.size(p.col);
        if x0 < app.scroll.x { app.scroll.x = x0; } else if x1 > app.scroll.x + vw { app.scroll.x = (x1 - vw).min(x0); }
    }
    if p.row >= lay.fr {
        let y0 = lay.rows.pos(p.row) - lay.rows.pos(lay.fr); let y1 = y0 + lay.rows.size(p.row);
        if y0 < app.scroll.y { app.scroll.y = y0; } else if y1 > app.scroll.y + vh { app.scroll.y = (y1 - vh).min(y0); }
    }
    app.scroll = app.scroll.max(egui::Vec2::ZERO);
}

/// What one cell shows: its text, color and alignment.
struct Shown { text: String, color: Option<Color32>, align: HAlign, num: bool }

fn shown(app: &SheetApp, s: &Sheet, p: Pos, st: &Style) -> Option<Shown> {
    let v = s.value(p);
    if v.is_empty() { return None; }
    if !s.show_zeros && v == Value::Num(0.0) { return None; }
    if sheet_engine::cellimage::is_image(&v) { return None; }
    let show_formulas = crate::formulas_tab::showing_formulas(app);
    let f = sheet_engine::numfmt::format(&v, &st.num_fmt, app.book.date1904);
    let text = if show_formulas { app.book.edit_text(app.sheet(), p) } else { f.text };
    let align = match st.h {
        HAlign::General => match v { Value::Num(_) => HAlign::Right, Value::Bool(_) | Value::Err(_) => HAlign::Center, _ => HAlign::Left },
        a => a,
    };
    Some(Shown { text, color: f.color.map(rgb), align, num: matches!(v, Value::Num(_)) })
}

pub(crate) fn draw_cells(app: &SheetApp, p: &egui::Painter, lay: &Layout, rows: std::ops::RangeInclusive<u32>, cols: std::ops::RangeInclusive<u32>) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    let z = lay.z;
    let grid_c = if app.dark { Color32::from_gray(0x44) } else { Color32::from_rgb(0xE1, 0xE1, 0xE1) };
    let ink = if app.dark { Color32::from_gray(0xEE) } else { Color32::BLACK };
    let (r0, r1, c0, c1) = (*rows.start(), *rows.end(), *cols.start(), *cols.end());
    if r0 > r1 || c0 > c1 { return; }
    let area = Range { r0, r1, c0, c1 };
    let merges: Vec<Range> = s.merges.iter().copied().filter(|m| m.intersects(&area)).collect();
    let covered = |q: Pos| merges.iter().any(|m| m.contains(q) && m.start() != q);
    // Fills and gridlines.
    for r in r0..=r1 {
        let rh = lay.rows.size(r);
        if rh <= 0.0 { continue; }
        for c in c0..=c1 {
            let q = Pos::new(r, c);
            let cw = lay.cols.size(c);
            if cw <= 0.0 { continue; }
            let st = app.book.styles.get(s.style_index(q));
            let st = &*crate::filter_ui::cell_style(app, si, q, st);
            let st = &*crate::condformat_ui::cell_style(app, si, q, st);
            if let Some(f) = st.fill { if !covered(q) { p.rect_filled(lay.cell_rect(q), 0.0, rgb(f)); } }
            else if s.gridlines {
                let rc = lay.cell_rect(q);
                p.line_segment([rc.right_top(), rc.right_bottom()], Stroke::new(1.0, grid_c));
                p.line_segment([rc.left_bottom(), rc.right_bottom()], Stroke::new(1.0, grid_c));
            }
        }
    }
    for m in &merges {
        let st = app.book.styles.get(s.style_index(m.start()));
        let st = &*crate::condformat_ui::cell_style(app, si, m.start(), st);
        let rc = lay.range_rect(*m);
        p.rect_filled(rc, 0.0, st.fill.map(rgb).unwrap_or(c_card()));
    }
    crate::condformat_ui::draw(app, p, lay, area);
    crate::sparkline_ui::draw(app, p, lay, area);
    // Text, with long text running into empty neighbours.
    let empty = |q: Pos| s.value(q).is_empty() && !covered(q) && s.merge_at(q).is_none();
    let draw_text = |q: Pos, cell: Rect, st: &Style, sh: Shown, can_spill: bool| {
        let px = st.size_pt() * 96.0 / 72.0 * z;
        let font = font_for(st, px);
        let color = sh.color.or(st.color.map(rgb)).unwrap_or(ink);
        let pad = 3.0 * z + st.indent as f32 * 8.0 * z;
        let wrap = st.wrap && !sh.num;
        let galley = if wrap { p.layout(sh.text.clone(), font.clone(), color, (cell.width() - 2.0 * pad).max(4.0)) } else { p.layout_no_wrap(sh.text.replace('\n', " "), font.clone(), color) };
        let w = galley.size().x;
        let mut clip = cell;
        let mut text = sh.text.clone();
        // Numbers too wide for their column show ### (General shortens instead).
        if sh.num && w > cell.width() - 2.0 * pad && cell.width() > 0.0 {
            let n = ((cell.width() - 2.0 * pad) / (px * 0.55)).floor().max(1.0) as usize;
            text = if st.num_fmt.eq_ignore_ascii_case("general") { if let Value::Num(x) = s.value(q) { sheet_engine::numfmt::general(x, n) } else { "#".repeat(n) } } else { "#".repeat(n) };
            let g2 = p.layout_no_wrap(text.clone(), font.clone(), color);
            if g2.size().x > cell.width() - 2.0 * pad { text = "#".repeat(n); }
        } else if can_spill && !wrap && w > cell.width() - 2.0 * pad {
            let need = w + 2.0 * pad;
            let mut grow_r = |clip: &mut Rect| { let mut c = q.col + 1; while clip.width() < need && c < MAX_COLS && empty(Pos::new(q.row, c)) { clip.max.x += lay.cols.size(c) * z; c += 1; } };
            let mut grow_l = |clip: &mut Rect| { let mut c = q.col; while clip.width() < need && c > 0 && empty(Pos::new(q.row, c - 1)) { c -= 1; clip.min.x -= lay.cols.size(c) * z; } };
            match sh.align { HAlign::Right => grow_l(&mut clip), HAlign::Center | HAlign::CenterAcross => { grow_r(&mut clip); grow_l(&mut clip); } _ => grow_r(&mut clip) }
            if lay.rtl { std::mem::swap(&mut clip.min.x, &mut clip.max.x); clip = Rect::from_two_pos(clip.min, clip.max); }
        }
        let galley = if text != sh.text { p.layout_no_wrap(text, font, color) } else { galley };
        let gs = galley.size();
        let x = match sh.align {
            HAlign::Right => cell.max.x - pad - gs.x,
            HAlign::Center | HAlign::CenterAcross | HAlign::Distributed => cell.center().x - gs.x / 2.0,
            _ => cell.min.x + pad,
        };
        let y = match st.v { VAlign::Top => cell.min.y + 1.0, VAlign::Center | VAlign::Justify | VAlign::Distributed => cell.center().y - gs.y / 2.0, VAlign::Bottom => cell.max.y - gs.y - 1.0 };
        let cp = p.with_clip_rect(clip.intersect(p.clip_rect()));
        let origin = pos2(x, y);
        cp.galley(origin, galley.clone(), color);
        let lw = (px / 14.0).max(1.0);
        if st.underline != Underline::None { let uy = origin.y + gs.y - px * 0.12; cp.line_segment([pos2(origin.x, uy), pos2(origin.x + gs.x, uy)], Stroke::new(lw, color)); if matches!(st.underline, Underline::Double | Underline::DoubleAccounting) { cp.line_segment([pos2(origin.x, uy + 2.0 * lw), pos2(origin.x + gs.x, uy + 2.0 * lw)], Stroke::new(lw, color)); } }
        if st.strike { let sy = origin.y + gs.y * 0.55; cp.line_segment([pos2(origin.x, sy), pos2(origin.x + gs.x, sy)], Stroke::new(lw, color)); }
    };
    // Text from cells left of the view can run into it: start a few columns early.
    let c_from = c0.saturating_sub(8);
    let c_to = (c1 + 8).min(MAX_COLS - 1);
    // Those cells only add their spill: one whose own box lies over this pane (a column scrolled
    // under frozen ones) belongs to another pane and isn't drawn here.
    let (span0, span1) = { let (a, b) = (lay.cell_rect(Pos::new(r0, c0)), lay.cell_rect(Pos::new(r0, c1))); (a.min.x.min(b.min.x), a.max.x.max(b.max.x)) };
    let over_pane = |q: Pos| { let rc = lay.cell_rect(q); rc.max.x > span0 + 0.5 && rc.min.x < span1 - 0.5 };
    for r in r0..=r1 {
        if lay.rows.size(r) <= 0.0 { continue; }
        let row_cells: Vec<Pos> = s.cells.range(Pos::new(r, c_from)..=Pos::new(r, c_to)).map(|(q, _)| *q).chain(s.spill.keys().filter(|q| q.row == r && q.col >= c_from && q.col <= c_to).copied()).collect();
        for q in row_cells {
            if covered(q) || lay.cols.size(q.col) <= 0.0 { continue; }
            if (q.col < c0 || q.col > c1) && over_pane(q) { continue; }
            if s.merge_at(q).is_some() { continue; }
            let st = app.book.styles.get(s.style_index(q));
            let st = &*crate::filter_ui::cell_style(app, si, q, st);
            let st = &*crate::condformat_ui::cell_style(app, si, q, st);
            if let Some(sh) = shown(app, s, q, st).filter(|_| !crate::condformat_ui::hide_value(app, si, q)) { draw_text(q, lay.cell_rect(q), st, sh, true); }
        }
    }
    for m in &merges {
        let q = m.start();
        let st = app.book.styles.get(s.style_index(q));
        let st = &*crate::condformat_ui::cell_style(app, si, q, st);
        if let Some(sh) = shown(app, s, q, st).filter(|_| !crate::condformat_ui::hide_value(app, si, q)) { draw_text(q, lay.range_rect(*m), st, sh, false); }
    }
    // Borders.
    for r in r0.saturating_sub(1)..=r1 {
        for c in c0.saturating_sub(1)..=c1 {
            let q = Pos::new(r, c);
            let Some(cell) = s.cells.get(&q) else { if s.row_styles.is_empty() && s.col_styles.is_empty() { continue; } else { draw_borders(p, lay, q, app.book.styles.get(s.style_index(q)), s); continue; } };
            if cell.style == 0 { continue; }
            draw_borders(p, lay, q, app.book.styles.get(cell.style), s);
        }
    }
}

fn draw_borders(p: &egui::Painter, lay: &Layout, q: Pos, st: &Style, s: &Sheet) {
    let z = lay.z;
    let rc = match s.merge_at(q) { Some(m) if m.start() == q => lay.range_rect(m), Some(_) => return, None => lay.cell_rect(q) };
    let sides = [(&st.top, rc.left_top(), rc.right_top()), (&st.bottom, rc.left_bottom(), rc.right_bottom()), (&st.left, rc.left_top(), rc.left_bottom()), (&st.right, rc.right_top(), rc.right_bottom())];
    for (b, a, e) in sides {
        if let Some(stroke) = border_stroke(b, z) {
            match b.style {
                LineStyle::Dashed | LineStyle::MediumDashed => dashed(p, a, e, stroke, 4.0 * z, 2.0 * z, 0.0),
                LineStyle::Dotted | LineStyle::Hair => dashed(p, a, e, stroke, 1.0 * z, 1.5 * z, 0.0),
                LineStyle::DashDot | LineStyle::MediumDashDot | LineStyle::DashDotDot | LineStyle::MediumDashDotDot | LineStyle::SlantDashDot => dashed(p, a, e, stroke, 5.0 * z, 2.0 * z, 0.0),
                LineStyle::Double => { let n = if (a.y - e.y).abs() < 0.5 { vec2(0.0, 1.5) } else { vec2(1.5, 0.0) }; let s1 = Stroke::new(1.0, stroke.color); p.line_segment([a - n, e - n], s1); p.line_segment([a + n, e + n], s1); }
                _ => { p.line_segment([a, e], stroke); }
            }
        }
    }
    if st.diag_down { if let Some(stroke) = border_stroke(&st.diagonal, z) { p.line_segment([rc.left_top(), rc.right_bottom()], stroke); } }
    if st.diag_up { if let Some(stroke) = border_stroke(&st.diagonal, z) { p.line_segment([rc.left_bottom(), rc.right_top()], stroke); } }
}

pub(crate) fn draw_selection(app: &SheetApp, p: &egui::Painter, lay: &Layout, time: f64) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    let tint = Color32::from_rgba_unmultiplied(0x21, 0x73, 0x46, 28);
    let ranges = app.sel.ranges();
    let grow = |r: Range| -> Range { let mut r = r; for m in &s.merges { if m.intersects(&r) { r = r.union(m); } } r };
    for r in &ranges {
        let r = grow(*r);
        let rc = lay.range_rect(clip_range(r));
        if !r.is_cell() { p.rect_filled(rc, 0.0, tint); }
    }
    // The active cell stays clear inside the tint.
    let active = s.merge_at(app.sel.active).unwrap_or(Range::cell(app.sel.active));
    let arc = lay.range_rect(active);
    if !app.sel.range().is_cell() || !app.sel.extra.is_empty() { p.rect_filled(arc, 0.0, c_card().gamma_multiply(0.0)); }
    let main = grow(app.sel.range());
    let mrc = lay.range_rect(clip_range(main));
    p.rect_stroke(mrc, 0.0, Stroke::new(2.0, SELECT), StrokeKind::Middle);
    if !main.is_cell() { p.rect_stroke(arc, 0.0, Stroke::new(1.0, SELECT), StrokeKind::Inside); }
    // The fill handle.
    if app.edit.is_none() && app.sel.extra.is_empty() {
        let h = Rect::from_center_size(mrc.right_bottom(), vec2(6.0, 6.0));
        p.rect_filled(h.expand(1.0), 0.0, c_card());
        p.rect_filled(h, 0.0, SELECT);
    }
    if let Drag::Fill { to, .. } = app.grid.drag { p.rect_stroke(lay.range_rect(to), 0.0, Stroke::new(1.0, c_ink3()), StrokeKind::Middle); }
    if let Drag::Move { src, grab, to } = app.grid.drag {
        let dr = to.row as i64 - grab.row as i64; let dc = to.col as i64 - grab.col as i64;
        let r = Range { r0: (src.r0 as i64 + dr).max(0) as u32, r1: (src.r1 as i64 + dr).max(0) as u32, c0: (src.c0 as i64 + dc).max(0) as u32, c1: (src.c1 as i64 + dc).max(0) as u32 };
        p.rect_stroke(lay.range_rect(r), 0.0, Stroke::new(2.0, c_ink3()), StrokeKind::Middle);
    }
    // References in the formula being typed, in colors.
    if let Some(e) = &app.edit { if e.text.starts_with('=') && e.sheet == si { for (i, (r, _)) in formula_refs(&e.text, s, &app.book).into_iter().enumerate() { let c = REF_COLORS[i % REF_COLORS.len()]; let rc = lay.range_rect(clip_range(r)); p.rect_filled(rc, 0.0, c.gamma_multiply(0.12)); p.rect_stroke(rc, 0.0, Stroke::new(1.5, c), StrokeKind::Middle); } } }
    if let Some((ms, r)) = app.grid.marching { if ms == si { let rc = lay.range_rect(r).shrink(1.0); let off = (time * 20.0) as f32; for (a, b) in [(rc.left_top(), rc.right_top()), (rc.right_top(), rc.right_bottom()), (rc.right_bottom(), rc.left_bottom()), (rc.left_bottom(), rc.left_top())] { dashed(p, a, b, Stroke::new(2.0, SELECT), 4.0, 3.0, off); } } }
}

pub const REF_COLORS: [Color32; 7] = [Color32::from_rgb(0x1F, 0x6F, 0xD8), Color32::from_rgb(0xD8, 0x3B, 0x01), Color32::from_rgb(0x7B, 0x3F, 0xC4), Color32::from_rgb(0x10, 0x89, 0x3E), Color32::from_rgb(0xC2, 0x18, 0x5B), Color32::from_rgb(0x00, 0x83, 0x8F), Color32::from_rgb(0xB8, 0x86, 0x0B)];

/// The references a formula's text names on this sheet, with their character spans.
pub fn formula_refs(text: &str, s: &Sheet, book: &sheet_engine::Workbook) -> Vec<(Range, (usize, usize))> {
    let Ok(toks) = sheet_engine::formula::lexer::tokenize(&text[1..]) else { return vec![] };
    toks.into_iter().filter_map(|t| match t.tok {
        sheet_engine::formula::lexer::Tok::Ref { sheet, body, .. } if sheet.as_deref().is_none_or(|n| book.sheet_index(n).is_some_and(|i| book.sheets[i].name == s.name)) => sheet_engine::addr::parse_range(&body).map(|r| (r, (t.start + 1, t.end + 1))),
        _ => None,
    }).collect()
}

/// Whole rows and columns drawn only to the edge of a sensible area.
fn clip_range(r: Range) -> Range { Range { r1: r.r1.min(r.r0 + 20_000), c1: r.c1.min(r.c0 + 2_000), ..r } }

fn headers(app: &SheetApp, p: &egui::Painter, lay: &Layout) {
    if lay.head_w <= 0.0 { return; }
    let r = lay.rect;
    let bg = c_frame();
    let line = c_line();
    let sel = app.sel.ranges();
    let on_col = |c: u32| sel.iter().any(|x| x.c0 <= c && c <= x.c1);
    let on_row = |rw: u32| sel.iter().any(|x| x.r0 <= rw && rw <= x.r1);
    let full_col = |c: u32| sel.iter().any(|x| x.c0 <= c && c <= x.c1 && x.r0 == 0 && x.r1 == MAX_ROWS - 1);
    let full_row = |rw: u32| sel.iter().any(|x| x.r0 <= rw && rw <= x.r1 && x.c0 == 0 && x.c1 == MAX_COLS - 1);
    let font = FontId::proportional(11.5 * lay.z.clamp(0.7, 1.6));
    let view = lay.view();
    // Column letters.
    let ch = Rect::from_min_max(pos2(view.min.x, r.min.y), pos2(view.max.x, r.min.y + lay.head_h));
    p.rect_filled(ch, 0.0, bg);
    let (vc0, vc1) = lay.visible_cols();
    for c in (lay.oc..lay.fc).chain(vc0..=vc1) {
        let w = lay.cols.size(c) * lay.z;
        if w <= 0.0 { continue; }
        let x = lay.x(c);
        let rc = Rect::from_min_size(pos2(x, r.min.y), vec2(w, lay.head_h));
        let cp = p.with_clip_rect(if c < lay.fc { ch } else { Rect::from_min_max(pos2(lay.body().x.max(view.min.x), ch.min.y), ch.max) }.intersect(p.clip_rect()));
        if full_col(c) { cp.rect_filled(rc, 0.0, SELECT); } else if on_col(c) { cp.rect_filled(rc, 0.0, Color32::from_rgba_unmultiplied(0x21, 0x73, 0x46, 40)); cp.line_segment([rc.left_bottom(), rc.right_bottom()], Stroke::new(2.0, SELECT)); }
        cp.line_segment([rc.right_top(), rc.right_bottom()], Stroke::new(1.0, line));
        cp.text(rc.center(), Align2::CENTER_CENTER, col_name(c), font.clone(), if full_col(c) { Color32::WHITE } else if on_col(c) { SELECT } else { c_ink2() });
    }
    // Row numbers.
    let rh = Rect::from_min_max(pos2(r.min.x, view.min.y), pos2(r.min.x + lay.head_w, view.max.y));
    let rh = if lay.rtl { Rect::from_min_max(pos2(r.max.x - SCROLLBAR - lay.head_w, view.min.y), pos2(r.max.x - SCROLLBAR, view.max.y)) } else { rh };
    p.rect_filled(rh, 0.0, bg);
    let (vr0, vr1) = lay.visible_rows();
    for rw in (lay.or..lay.fr).chain(vr0..=vr1) {
        let h = lay.rows.size(rw) * lay.z;
        if h <= 0.0 { continue; }
        let y = lay.y(rw);
        let rc = Rect::from_min_size(pos2(rh.min.x, y), vec2(lay.head_w, h));
        let cp = p.with_clip_rect(if rw < lay.fr { rh } else { Rect::from_min_max(pos2(rh.min.x, lay.body().y.max(view.min.y)), rh.max) }.intersect(p.clip_rect()));
        if full_row(rw) { cp.rect_filled(rc, 0.0, SELECT); } else if on_row(rw) { cp.rect_filled(rc, 0.0, Color32::from_rgba_unmultiplied(0x21, 0x73, 0x46, 40)); cp.line_segment([rc.right_top(), rc.right_bottom()], Stroke::new(2.0, SELECT)); }
        cp.line_segment([rc.left_bottom(), rc.right_bottom()], Stroke::new(1.0, line));
        cp.text(rc.center(), Align2::CENTER_CENTER, (rw + 1).to_string(), font.clone(), if full_row(rw) { Color32::WHITE } else if on_row(rw) { SELECT } else { c_ink2() });
    }
    // The corner selects everything.
    let corner = if lay.rtl { Rect::from_min_size(pos2(r.max.x - SCROLLBAR - lay.head_w, r.min.y), vec2(lay.head_w, lay.head_h)) } else { Rect::from_min_size(r.min, vec2(lay.head_w, lay.head_h)) };
    p.rect_filled(corner, 0.0, bg);
    let t = corner.shrink(4.0);
    p.add(egui::Shape::convex_polygon(vec![t.right_top() + vec2(0.0, 4.0), t.right_bottom(), t.left_bottom() + vec2(4.0, 0.0)], c_ink3(), Stroke::NONE));
    p.line_segment([pos2(view.min.x, ch.max.y), pos2(view.max.x, ch.max.y)], Stroke::new(1.0, line));
    p.line_segment([pos2(rh.max.x, view.min.y), pos2(rh.max.x, view.max.y)], Stroke::new(1.0, line));
}

/// The total scrollable size: the used area plus a screenful.
fn content(app: &SheetApp, lay: &Layout) -> egui::Vec2 {
    let s = &app.book.sheets[app.sheet()];
    let e = s.extent().unwrap_or_default();
    let p = app.sel.active;
    let view = lay.view();
    let body = lay.body();
    vec2(lay.cols.pos(e.col.max(p.col) + 1) - lay.cols.pos(lay.fc) + (view.max.x - body.x) / lay.z, lay.rows.pos(e.row.max(p.row) + 1) - lay.rows.pos(lay.fr) + (view.max.y - body.y) / lay.z)
}

fn scrollbars(app: &mut SheetApp, ui: &mut Ui, p: &egui::Painter, lay: &Layout) {
    let r = lay.rect;
    let total = content(app, lay).max(app.scroll + lay.view().size() / lay.z);
    let view = lay.view();
    let track_v = Rect::from_min_max(pos2(r.max.x - SCROLLBAR, view.min.y), pos2(r.max.x, view.max.y));
    let track_h = Rect::from_min_max(pos2(view.min.x, r.max.y - SCROLLBAR), pos2(view.max.x, r.max.y));
    p.rect_filled(track_v, 0.0, c_frame());
    p.rect_filled(track_h, 0.0, c_frame());
    let vis = view.size() / lay.z;
    let thumb = |track: Rect, along_y: bool| -> Rect {
        let (len, tot, at, vis) = if along_y { (track.height(), total.y, app.scroll.y, vis.y) } else { (track.width(), total.x, app.scroll.x, vis.x) };
        let size = (len * vis / tot).clamp(24.0, len);
        let start = (len - size) * (at / (tot - vis).max(1.0)).clamp(0.0, 1.0);
        if along_y { Rect::from_min_size(pos2(track.min.x + 2.0, track.min.y + start), vec2(SCROLLBAR - 4.0, size)) } else { Rect::from_min_size(pos2(track.min.x + start, track.min.y + 2.0), vec2(size, SCROLLBAR - 4.0)) }
    };
    let tv = thumb(track_v, true); let th = thumb(track_h, false);
    p.rect_filled(tv, 4.0, c_ink3().gamma_multiply(0.6));
    p.rect_filled(th, 4.0, c_ink3().gamma_multiply(0.6));
    let rv = ui.interact(track_v, ui.id().with("vscroll"), Sense::click_and_drag());
    let rh = ui.interact(track_h, ui.id().with("hscroll"), Sense::click_and_drag());
    if rv.drag_started() { if let Some(pp) = rv.interact_pointer_pos() { app.grid.drag = Drag::VScroll { from: pp.y, start: app.scroll.y }; } }
    if rh.drag_started() { if let Some(pp) = rh.interact_pointer_pos() { app.grid.drag = Drag::HScroll { from: pp.x, start: app.scroll.x }; } }
    if let (Drag::VScroll { from, start }, Some(pp)) = (app.grid.drag.clone(), rv.interact_pointer_pos()) { if rv.dragged() { app.scroll.y = (start + (pp.y - from) * total.y / track_v.height()).max(0.0); } }
    if let (Drag::HScroll { from, start }, Some(pp)) = (app.grid.drag.clone(), rh.interact_pointer_pos()) { if rh.dragged() { app.scroll.x = (start + (pp.x - from) * total.x / track_h.width()).max(0.0); } }
    if rv.drag_stopped() || rh.drag_stopped() { app.grid.drag = Drag::None; }
    if rv.clicked() { if let Some(pp) = rv.interact_pointer_pos() { app.scroll.y = (app.scroll.y + if pp.y < tv.min.y { -vis.y } else { vis.y }).max(0.0); } }
    if rh.clicked() { if let Some(pp) = rh.interact_pointer_pos() { app.scroll.x = (app.scroll.x + if pp.x < th.min.x { -vis.x } else { vis.x }).max(0.0); } }
}

/// Is the formula being typed at a place where pointing at a cell inserts a reference?
pub fn can_point(e: &Editor) -> bool {
    if !e.text.starts_with('=') { return false; }
    if e.pointing.is_some() { return true; }
    let before: String = e.text.chars().take(e.cursor).collect();
    let last = before.trim_end().chars().last();
    matches!(last, Some('=' | '(' | ',' | ';' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':' | '%'))
}

/// Puts a reference into the formula at the cursor (replacing the one being pointed at).
pub fn insert_ref(app: &mut SheetApp, r: Range, sheet: usize) {
    let Some(e) = app.edit.as_mut() else { return };
    let mut text = r.a1();
    if sheet != e.sheet { text = format!("{}!{}", sheet_engine::addr::quote_sheet(&app.book.sheets[sheet].name), text); }
    let chars: Vec<char> = e.text.chars().collect();
    let (a, b) = e.pointing.unwrap_or((e.cursor, e.cursor));
    let mut out: String = chars[..a.min(chars.len())].iter().collect();
    out.push_str(&text);
    out.extend(&chars[b.min(chars.len())..]);
    e.text = out;
    let end = a + text.chars().count();
    e.pointing = Some((a, end));
    e.cursor = end;
    e.request_focus = true;
}

pub(crate) fn mouse(app: &mut SheetApp, ui: &mut Ui, resp: &egui::Response, lay: &Layout) {
    let si = app.sheet();
    let view = lay.view();
    let (cmd, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
    // Wheel and trackpad.
    if resp.hovered() || resp.contains_pointer() {
        let (d, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        if zoom != 1.0 { app.zoom = (app.zoom * zoom).clamp(0.1, 4.0); }
        else if d != egui::Vec2::ZERO {
            let d = if shift && d.x == 0.0 { vec2(d.y, 0.0) } else { d };
            app.scroll = (app.scroll - d / lay.z).max(egui::Vec2::ZERO);
            if lay.rtl { app.scroll.x = (app.scroll.x + 2.0 * d.x / lay.z).max(0.0); }
        }
    }
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let hover = ui.input(|i| i.pointer.hover_pos());
    // Cursor hints: resize at header edges, crosshair at the fill handle, move at the border.
    if let Some(h) = hover.filter(|h| lay.rect.contains(*h)) {
        if let Some(_) = header_edge(lay, h) { ui.ctx().set_cursor_icon(if h.y < lay.rect.min.y + lay.head_h { egui::CursorIcon::ResizeColumn } else { egui::CursorIcon::ResizeRow }); }
        else if fill_handle(app, lay).contains(h) && app.edit.is_none() { ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair); }
        else if on_border(app, lay, h) && app.edit.is_none() { ui.ctx().set_cursor_icon(egui::CursorIcon::Move); }
        else if view.contains(h) { ui.ctx().set_cursor_icon(egui::CursorIcon::Cell); }
    }
    if resp.drag_started() || (resp.clicked() && !resp.dragged()) || resp.secondary_clicked() {
        // Where the button went down: a drag only starts after the pointer has moved a little,
        // and by then it may have left the header edge or the fill handle.
        let Some(pt) = ui.input(|i| i.pointer.press_origin()).filter(|_| resp.drag_started()).or(pointer) else { return };
        if !resp.secondary_clicked() && crate::filter_ui::click(app, lay, pt, resp.clicked()) { return; }
        if resp.secondary_clicked() {
            let q = lay.cell_at(pt);
            if !app.sel.ranges().iter().any(|r| r.contains(q)) && view.contains(pt) { app.commit(None); app.select(q, false); }
            return;
        }
        let in_col_head = pt.y < lay.rect.min.y + lay.head_h && pt.x > view.min.x.min(lay.rect.min.x + lay.head_w);
        let in_row_head = if lay.rtl { pt.x > lay.rect.max.x - SCROLLBAR - lay.head_w } else { pt.x < lay.rect.min.x + lay.head_w } && pt.y > lay.rect.min.y + lay.head_h;
        let now = ui.input(|i| i.time);
        if let Some(edge) = header_edge(lay, pt) {
            // Double-click on a header edge fits the column to its contents.
            let double = resp.double_clicked() || app.grid.last_click.is_some_and(|(_, t)| now - t < 0.4);
            match edge {
                (true, c) if double => { autofit_cols(app, &[c]); }
                (true, c) => app.grid.drag = Drag::ColSize { col: c, from: pt.x, width: lay.cols.size(c) },
                (false, r) if double => { autofit_rows(app, &[r]); }
                (false, r) => app.grid.drag = Drag::RowSize { row: r, from: pt.y, height: lay.rows.size(r) },
            }
            app.grid.last_click = Some((Pos::default(), now));
            return;
        }
        if pt.x < lay.rect.min.x + lay.head_w && pt.y < lay.rect.min.y + lay.head_h && !lay.rtl { app.commit(None); app.select_all(); return; }
        if in_col_head {
            let c = lay.col_at(pt.x);
            app.commit(None);
            if shift { app.sel.active = Pos::new(MAX_ROWS - 1, c); app.sel.anchor.row = 0; } else {
                if cmd { app.sel.extra.push(app.sel.range()); }
                app.sel.anchor = Pos::new(0, c); app.sel.active = Pos::new(MAX_ROWS - 1, c);
            }
            app.grid.drag = Drag::Cols(c);
            return;
        }
        if in_row_head {
            let r = lay.row_at(pt.y);
            app.commit(None);
            if shift { app.sel.active = Pos::new(r, MAX_COLS - 1); app.sel.anchor.col = 0; } else {
                if cmd { app.sel.extra.push(app.sel.range()); }
                app.sel.anchor = Pos::new(r, 0); app.sel.active = Pos::new(r, MAX_COLS - 1);
            }
            app.grid.drag = Drag::Rows(r);
            return;
        }
        if !view.contains(pt) { return; }
        let q = lay.cell_at(pt);
        // Pointing at cells while typing a formula.
        if let Some(e) = &app.edit { if can_point(e) { let s = app.sheet(); let r = if shift { app.edit.as_ref().and_then(|e| e.pointing).map(|_| Range::cell(q)).unwrap_or(Range::cell(q)) } else { Range::cell(q) }; insert_ref(app, r, s); app.grid.drag = Drag::Point(q); return; } }
        if fill_handle(app, lay).contains(pt) && app.edit.is_none() { app.grid.drag = Drag::Fill { src: app.sel.range(), to: app.sel.range() }; return; }
        if on_border(app, lay, pt) && app.edit.is_none() { app.grid.drag = Drag::Move { src: app.sel.range(), grab: q, to: q }; return; }
        // Format Painter: paint onto the clicked cells.
        app.commit(None);
        let double = app.grid.last_click.is_some_and(|(p, t)| p == q && now - t < 0.4);
        app.grid.last_click = Some((q, now));
        if double && !resp.drag_started() {
            if app.book.sheets[si].links.contains_key(&q) && cmd { open_link(app, q); return; }
            app.begin_edit(None, false);
            return;
        }
        if cmd && !shift { app.sel.extra.push(app.sel.range()); app.sel.anchor = q; app.sel.active = q; }
        else { let q2 = if !shift { app.book.sheets[si].merge_at(q).map(|m| m.start()).unwrap_or(q) } else { q }; app.select(q2, shift); }
        app.reveal = false;
        app.grid.drag = Drag::Cells;
        // A click on a link opens it (as in Excel, a plain click follows hyperlinks).
        if !resp.drag_started() && app.book.sheets[si].links.contains_key(&q) && !shift && !cmd { open_link(app, q); }
    }
    if resp.dragged() {
        let Some(pt) = pointer else { return };
        // Dragging past the edge scrolls.
        let mut auto = egui::Vec2::ZERO;
        if pt.y > view.max.y { auto.y = (pt.y - view.max.y).min(60.0); } else if pt.y < view.min.y + lay.head_h * 0.0 && pt.y < lay.body().y && !matches!(app.grid.drag, Drag::ColSize { .. }) { auto.y = -(lay.body().y - pt.y).min(60.0); }
        if pt.x > view.max.x { auto.x = (pt.x - view.max.x).min(60.0); } else if pt.x < lay.body().x && !matches!(app.grid.drag, Drag::RowSize { .. }) { auto.x = -(lay.body().x - pt.x).min(60.0); }
        let pc = pos2(pt.x.clamp(view.min.x, view.max.x - 1.0), pt.y.clamp(view.min.y, view.max.y - 1.0));
        let q = lay.cell_at(pc);
        match app.grid.drag.clone() {
            Drag::Cells => { if auto != egui::Vec2::ZERO { app.scroll = (app.scroll + auto * 0.3).max(egui::Vec2::ZERO); } app.sel.active = q; }
            Drag::Cols(c0) => { app.sel.anchor = Pos::new(0, c0); app.sel.active = Pos::new(MAX_ROWS - 1, lay.col_at(pc.x)); }
            Drag::Rows(r0) => { app.sel.anchor = Pos::new(r0, 0); app.sel.active = Pos::new(lay.row_at(pc.y), MAX_COLS - 1); }
            Drag::ColSize { col, from, width } => {
                let w = (width + (pt.x - from) / lay.z * if lay.rtl { -1.0 } else { 1.0 }).max(0.0);
                let cols: Vec<u32> = if app.sel.range().r0 == 0 && app.sel.range().r1 == MAX_ROWS - 1 && app.sel.range().c0 <= col && col <= app.sel.range().c1 { (app.sel.range().c0..=app.sel.range().c1).collect() } else { vec![col] };
                if !matches!(app.undo_label(), Some("Column Width")) || resp.drag_started() { app.checkpoint("Column Width"); }
                let s = &mut app.book.sheets[si];
                for c in cols { if w < 2.0 { s.hidden_cols.insert(c); } else { s.hidden_cols.remove(&c); s.col_widths.insert(c, w.round()); } }
                app.dirty = true;
            }
            Drag::RowSize { row, from, height } => {
                let h = (height + (pt.y - from) / lay.z).max(0.0);
                let rows: Vec<u32> = if app.sel.range().c0 == 0 && app.sel.range().c1 == MAX_COLS - 1 && app.sel.range().r0 <= row && row <= app.sel.range().r1 { (app.sel.range().r0..=app.sel.range().r1).collect() } else { vec![row] };
                if !matches!(app.undo_label(), Some("Row Height")) || resp.drag_started() { app.checkpoint("Row Height"); }
                let s = &mut app.book.sheets[si];
                for r in rows { if h < 2.0 { s.hidden_rows.insert(r); } else { s.hidden_rows.remove(&r); s.row_heights.insert(r, h.round()); } }
                app.dirty = true;
            }
            Drag::Fill { src, .. } => {
                if auto != egui::Vec2::ZERO { app.scroll = (app.scroll + auto * 0.3).max(egui::Vec2::ZERO); }
                // Fill along the direction pulled furthest.
                let down = q.row as i64 - src.r1 as i64; let up = src.r0 as i64 - q.row as i64;
                let right = q.col as i64 - src.c1 as i64; let left = src.c0 as i64 - q.col as i64;
                let vert = down.max(up); let horiz = right.max(left);
                let to = if vert <= 0 && horiz <= 0 { src } else if vert >= horiz { if down > 0 { Range { r1: q.row, ..src } } else { Range { r0: q.row, ..src } } } else if right > 0 { Range { c1: q.col, ..src } } else { Range { c0: q.col, ..src } };
                app.grid.drag = Drag::Fill { src, to };
            }
            Drag::Move { src, grab, .. } => { if auto != egui::Vec2::ZERO { app.scroll = (app.scroll + auto * 0.3).max(egui::Vec2::ZERO); } app.grid.drag = Drag::Move { src, grab, to: q }; }
            Drag::Point(start) => { let s = app.sheet(); insert_ref(app, Range::new(start, q), s); }
            _ => {}
        }
    }
    if resp.drag_stopped() {
        match std::mem::take(&mut app.grid.drag) {
            Drag::Fill { src, to } if to != src => {
                app.checkpoint("AutoFill");
                let s = app.sheet();
                // Dragging back inside the selection clears the cells left behind.
                if to.r1 < src.r1 || to.c1 < src.c1 && to.r0 == src.r0 && to.c0 == src.c0 { let cleared = if to.r1 < src.r1 { Range { r0: to.r1 + 1, ..src } } else { Range { c0: to.c1 + 1, ..src } }; app.book.clear(s, cleared, true, false, false); }
                else { app.book.fill(s, src, to, !cmd); }
                app.sel = Selection { anchor: to.start(), active: to.end(), extra: vec![] };
                app.changed();
            }
            Drag::Move { src, grab, to } if to != grab => {
                let dr = to.row as i64 - grab.row as i64; let dc = to.col as i64 - grab.col as i64;
                let at = Pos::new((src.r0 as i64 + dr).max(0) as u32, (src.c0 as i64 + dc).max(0) as u32);
                app.checkpoint("Move");
                let s = app.sheet();
                if cmd { let clip = app.book.copy(s, src); app.book.paste(s, Range::cell(at), &clip, Default::default()); sheet_engine::follow::copy_objects_with_cells(&mut app.book, s, src, s, at); } else { app.book.move_range(s, src, s, at); }
                app.sel = Selection { anchor: at, active: Pos::new(at.row + src.rows() - 1, at.col + src.cols() - 1), extra: vec![] };
                app.changed();
            }
            Drag::Cells => { if let Some((clip, sticky)) = app.painter.clone() { paint_format(app, &clip); if !sticky { app.painter = None; } } }
            _ => {}
        }
    } else if resp.clicked() && matches!(app.grid.drag, Drag::Cells) {
        app.grid.drag = Drag::None;
        if let Some((clip, sticky)) = app.painter.clone() { paint_format(app, &clip); if !sticky { app.painter = None; } }
    } else if resp.clicked() { app.grid.drag = Drag::None; }
}

/// Format Painter: the copied formats onto the selection.
fn paint_format(app: &mut SheetApp, clip: &sheet_engine::book::Clip) {
    let s = app.sheet();
    app.checkpoint("Format Painter");
    app.book.paste(s, app.sel.range(), clip, sheet_engine::book::Paste { what: sheet_engine::book::PasteWhat::Formats, ..Default::default() });
    app.dirty = true;
}

fn open_link(app: &mut SheetApp, q: Pos) {
    let Some(l) = app.book.sheets[app.sheet()].links.get(&q).cloned() else { return };
    if let Some(inner) = l.target.strip_prefix('#') {
        let (sheet, cell) = inner.rsplit_once('!').map(|(s, c)| (Some(s.trim_matches('\'').to_string()), c.to_string())).unwrap_or((None, inner.to_string()));
        if let Some(i) = sheet.and_then(|n| app.book.sheet_index(&n)) { app.set_sheet(i); }
        if let Some(r) = sheet_engine::addr::parse_range(&cell) { app.sel = Selection { anchor: r.start(), active: r.end(), extra: vec![] }; app.reveal = true; }
    } else {
        let _ = open_url(&l.target);
    }
}

pub fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")] { std::process::Command::new("open").arg(url).spawn().map(|_| ()) }
    #[cfg(target_os = "windows")] { std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn().map(|_| ()) }
    #[cfg(all(unix, not(target_os = "macos")))] { std::process::Command::new("xdg-open").arg(url).spawn().map(|_| ()) }
}

/// A header boundary under the pointer: (is column, index of the line before the boundary).
fn header_edge(lay: &Layout, pt: Pos2) -> Option<(bool, u32)> {
    if pt.y < lay.rect.min.y + lay.head_h && pt.y >= lay.rect.min.y {
        let c = lay.col_at(pt.x);
        let rc = lay.cell_rect(Pos::new(lay.fr.max(0), c));
        let right = if lay.rtl { rc.min.x } else { rc.max.x };
        let left = if lay.rtl { rc.max.x } else { rc.min.x };
        if (pt.x - right).abs() <= 4.0 { return Some((true, c)); }
        if (pt.x - left).abs() <= 4.0 && c > 0 { return Some((true, c - 1)); }
    }
    let in_row_head = if lay.rtl { pt.x > lay.rect.max.x - SCROLLBAR - lay.head_w } else { pt.x < lay.rect.min.x + lay.head_w && pt.x >= lay.rect.min.x };
    if in_row_head && pt.y > lay.rect.min.y + lay.head_h {
        let r = lay.row_at(pt.y);
        let rc = lay.cell_rect(Pos::new(r, lay.fc));
        if (pt.y - rc.max.y).abs() <= 3.0 { return Some((false, r)); }
        if (pt.y - rc.min.y).abs() <= 3.0 && r > 0 { return Some((false, r - 1)); }
    }
    None
}

fn fill_handle(app: &SheetApp, lay: &Layout) -> Rect {
    if !app.sel.extra.is_empty() { return Rect::NOTHING; }
    Rect::from_center_size(lay.range_rect(clip_range(app.sel.range())).right_bottom(), vec2(10.0, 10.0))
}

fn on_border(app: &SheetApp, lay: &Layout, pt: Pos2) -> bool {
    if !app.sel.extra.is_empty() { return false; }
    let r = app.sel.range();
    if r.r1 - r.r0 > 100_000 || r.c1 - r.c0 > 10_000 { return false; }
    let rc = lay.range_rect(r);
    rc.expand(3.0).contains(pt) && !rc.shrink(3.0).contains(pt)
}

/// Widens columns to fit their longest text.
pub fn autofit_cols(app: &mut SheetApp, cols: &[u32]) {
    let si = app.sheet();
    app.checkpoint("AutoFit Column Width");
    let ctx = app.ctx.clone();
    for &c in cols {
        let s = &app.book.sheets[si];
        let mut w: f32 = 0.0;
        let cells: Vec<Pos> = s.cells.keys().copied().filter(|p| p.col == c).chain(s.spill.keys().copied().filter(|p| p.col == c)).collect();
        for q in cells {
            if s.merge_at(q).is_some_and(|m| !m.is_cell()) { continue; }
            let st = app.book.styles.get(s.style_index(q));
            let text = app.book.display(si, q);
            if text.is_empty() { continue; }
            let font = font_for(st, st.size_pt() * 96.0 / 72.0);
            let tw = ctx.fonts_mut(|f| f.layout_no_wrap(text.lines().map(|l| l.to_string()).max_by_key(|l| l.len()).unwrap_or_default(), font, Color32::BLACK).size().x);
            w = w.max(tw + 10.0);
        }
        let s = &mut app.book.sheets[si];
        if w > 0.0 { s.col_widths.insert(c, w.ceil().min(1500.0)); } else { s.col_widths.remove(&c); }
    }
    app.dirty = true;
}

/// Sets rows to the height their (wrapped) text needs.
pub fn autofit_rows(app: &mut SheetApp, rows: &[u32]) {
    let si = app.sheet();
    app.checkpoint("AutoFit Row Height");
    let ctx = app.ctx.clone();
    for &r in rows {
        let s = &app.book.sheets[si];
        let mut h: f32 = 0.0;
        let cells: Vec<Pos> = s.cells.range(Pos::new(r, 0)..=Pos::new(r, MAX_COLS)).map(|(p, _)| *p).collect();
        for q in cells {
            let st = app.book.styles.get(s.style_index(q));
            let text = app.book.display(si, q);
            let px = st.size_pt() * 96.0 / 72.0;
            let font = font_for(st, px);
            let width = if st.wrap { s.col_width(q.col) - 6.0 } else { f32::INFINITY };
            let th = if text.is_empty() { px * 1.25 } else { ctx.fonts_mut(|f| f.layout(text.clone(), font, Color32::BLACK, width.max(4.0)).size().y) };
            h = h.max(th + 4.0);
        }
        let s = &mut app.book.sheets[si];
        if h > s.default_row_height + 0.5 { s.row_heights.insert(r, h.ceil()); } else { s.row_heights.remove(&r); }
    }
    app.dirty = true;
}

pub(crate) fn cell_editor(app: &mut SheetApp, ui: &mut Ui, lay: &Layout) {
    let si = app.sheet();
    let Some(e) = app.edit.as_ref() else { return };
    let p = e.pos;
    let s = &app.book.sheets[si];
    let st = app.book.styles.get(s.style_index(p)).clone();
    let cell = match s.merge_at(p) { Some(m) => lay.range_rect(m), None => lay.cell_rect(p) };
    let px = st.size_pt() * 96.0 / 72.0 * lay.z;
    let font = font_for(&st, px);
    let galley = ui.painter().layout(e.text.clone() + " ", font.clone(), Color32::BLACK, if st.wrap { cell.width() } else { f32::INFINITY });
    let w = (galley.size().x + 10.0).max(cell.width()).min(lay.view().max.x - cell.min.x).max(cell.width());
    let h = (galley.size().y + 4.0).max(cell.height());
    let rect = Rect::from_min_size(cell.min, vec2(w, h));
    edit_keys(app, ui, false);
    let Some(e) = app.edit.as_mut() else { return };
    let id = egui::Id::new("cell-editor");
    ui.painter().rect_filled(rect, 0.0, st.fill.map(rgb).unwrap_or(c_card()));
    ui.painter().rect_stroke(rect, 0.0, Stroke::new(2.0, SELECT), StrokeKind::Outside);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(2.0, 1.0))));
    let color = st.color.map(rgb).unwrap_or(if app.dark { Color32::from_gray(0xEE) } else { Color32::BLACK });
    let te = egui::TextEdit::multiline(&mut e.text).id(id).font(font).text_color(color).frame(egui::Frame::NONE).desired_rows(1).desired_width(rect.width() - 4.0).lock_focus(true);
    let out = te.show(&mut child);
    let pending = e.request_focus;
    if e.request_focus { out.response.request_focus(); e.request_focus = false; set_cursor(ui.ctx(), id, e.cursor); }
    // The field reports its old cursor the frame a reference is put in; only later moves count.
    if !pending { if let Some(c) = out.cursor_range { let n = usize::from(c.primary.index); if n != e.cursor { e.cursor = n; e.pointing = None; } } }
    if out.response.changed() && !pending { e.pointing = None; }
}

/// Places the text cursor of the text field `id` at character `at`.
pub fn set_cursor(ctx: &egui::Context, id: egui::Id, at: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let c = egui::text::CCursor::new(at);
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(c)));
        state.store(ctx, id);
    }
}

/// Keys while editing a cell: Enter, Tab, Escape, arrows (which point at cells in a formula or
/// end a typed entry), Alt+Enter for a new line, ⌘Enter to fill the selection.
pub fn edit_keys(app: &mut SheetApp, ui: &mut Ui, in_bar: bool) {
    let Some(e) = app.edit.as_ref() else { return };
    let typed = e.typed && !in_bar;
    let pointing_ok = can_point(e);
    let enter_mv = match app.prefs.enter_moves { 0 => (1, 0), 1 => (0, 1), 2 => (-1, 0), 3 => (0, -1), _ => (0, 0) };
    let mut action: Option<(i64, i64, bool)> = None; // (dr, dc, commit)
    let mut newline = false; let mut all = false; let mut cancel = false;
    let mut point: Option<(i64, i64, bool)> = None;
    let mut toggle_abs = false;
    ui.input_mut(|i| {
        if i.consume_key(Modifiers::ALT, Key::Enter) || i.consume_key(Modifiers::CTRL | Modifiers::ALT, Key::Enter) { newline = true; }
        else if i.consume_key(Modifiers::COMMAND, Key::Enter) { all = true; }
        else if i.consume_key(Modifiers::SHIFT, Key::Enter) { action = Some((-enter_mv.0, -enter_mv.1, true)); }
        else if i.consume_key(Modifiers::NONE, Key::Enter) { action = Some((enter_mv.0, enter_mv.1, true)); }
        if i.consume_key(Modifiers::SHIFT, Key::Tab) { action = Some((0, -1, true)); }
        else if i.consume_key(Modifiers::NONE, Key::Tab) { action = Some((0, 1, true)); }
        if i.consume_key(Modifiers::NONE, Key::Escape) { cancel = true; }
        if i.consume_key(Modifiers::NONE, Key::F4) { toggle_abs = true; }
        if typed || pointing_ok {
            for (k, d) in [(Key::ArrowDown, (1, 0)), (Key::ArrowUp, (-1, 0)), (Key::ArrowLeft, (0, -1)), (Key::ArrowRight, (0, 1))] {
                if pointing_ok && i.consume_key(Modifiers::SHIFT, k) { point = Some((d.0, d.1, true)); }
                if i.consume_key(Modifiers::NONE, k) { if pointing_ok { point = Some((d.0, d.1, false)); } else { action = Some((d.0, d.1, true)); } }
            }
        }
    });
    if newline { if let Some(e) = app.edit.as_mut() { let mut ch: Vec<char> = e.text.chars().collect(); ch.insert(e.cursor.min(ch.len()), '\n'); e.text = ch.into_iter().collect(); e.cursor += 1; e.request_focus = true; } }
    if toggle_abs { if let Some(e) = app.edit.as_mut() { toggle_absolute(e); } }
    if cancel { app.cancel_edit(); return; }
    if all { app.commit_all(); return; }
    if let Some((dr, dc, extend)) = point {
        let Some(e) = app.edit.as_ref() else { return };
        let s = &app.book.sheets[app.sheet()];
        let cur = e.pointing.and_then(|(a, b)| sheet_engine::addr::parse_range(&e.text.chars().skip(a).take(b - a).collect::<String>().rsplit('!').next().unwrap_or("").to_string()));
        let base = cur.map(|r| if extend { r } else { Range::cell(r.end()) }).unwrap_or(Range::cell(e.pos));
        let end = base.end();
        let n = Pos::new((end.row as i64 + dr).clamp(0, MAX_ROWS as i64 - 1) as u32, (end.col as i64 + dc).clamp(0, MAX_COLS as i64 - 1) as u32);
        let _ = s;
        let r = if extend { Range::new(base.start(), n) } else { Range::cell(n) };
        let si = app.sheet();
        insert_ref(app, r, si);
        return;
    }
    if let Some((dr, dc, _)) = action { app.commit(Some((dr, dc))); }
}

/// F4: cycles the reference at the cursor through A1 → $A$1 → A$1 → $A1.
fn toggle_absolute(e: &mut Editor) {
    if !e.text.starts_with('=') { return; }
    let Ok(toks) = sheet_engine::formula::lexer::tokenize(&e.text[1..]) else { return };
    let chars: Vec<char> = e.text.chars().collect();
    for t in toks {
        let (a, b) = (t.start + 1, t.end + 1);
        if let sheet_engine::formula::lexer::Tok::Ref { body, .. } = &t.tok {
            if e.cursor < a || e.cursor > b { continue; }
            let cycle = |part: &str| -> String {
                let col_abs = part.starts_with('$');
                let rest = part.trim_start_matches('$');
                let split = rest.find(|c: char| c == '$' || c.is_ascii_digit()).unwrap_or(rest.len());
                let (l, d) = rest.split_at(split);
                let row_abs = d.starts_with('$');
                let d = d.trim_start_matches('$');
                let (nc, nr) = match (col_abs, row_abs) { (false, false) => (true, true), (true, true) => (false, true), (false, true) => (true, false), (true, false) => (false, false) };
                format!("{}{}{}{}", if nc && !l.is_empty() { "$" } else { "" }, l, if nr && !d.is_empty() { "$" } else { "" }, d)
            };
            let nb = body.split(':').map(cycle).collect::<Vec<_>>().join(":");
            let whole: String = chars[a..b].iter().collect();
            let prefix = &whole[..whole.len() - body.len()];
            let new = format!("{prefix}{nb}");
            let mut out: String = chars[..a].iter().collect();
            out.push_str(&new);
            out.extend(&chars[b..]);
            e.text = out;
            e.cursor = a + new.chars().count();
            e.request_focus = true;
            return;
        }
    }
}

/// Grid keys when no cell is being edited, and typing that starts an entry.
pub fn shortcuts(app: &mut SheetApp, ctx: &egui::Context) {
    let focused_elsewhere = ctx.memory(|m| m.focused().is_some_and(|f| f != egui::Id::new("cell-editor") && f != egui::Id::new("formula-bar")));
    let cmd_m = Modifiers::COMMAND;
    let mut pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
    // File and history work everywhere.
    if pressed(cmd_m, Key::S) { app.save(false); }
    if pressed(cmd_m | Modifiers::SHIFT, Key::S) { app.save(true); }
    if pressed(cmd_m, Key::O) { app.guarded(crate::filemenu::Pending::Open); }
    if pressed(cmd_m, Key::N) { app.guarded(crate::filemenu::Pending::New); }
    if pressed(cmd_m, Key::P) { crate::print::open(app); }
    if pressed(cmd_m, Key::Comma) { app.options = Some(app.prefs.clone()); }
    if pressed(cmd_m, Key::W) { app.guarded(crate::filemenu::Pending::Close); }
    if focused_elsewhere || app.dialog.is_some() { return; }
    if app.edit.is_some() { return; }
    if crate::objects::keys(app, ctx) { return; }
    if pressed(cmd_m, Key::Z) { app.undo(); }
    if pressed(cmd_m | Modifiers::SHIFT, Key::Z) || pressed(cmd_m, Key::Y) { app.redo(); }
    // On macOS ⌘C/⌘X/⌘V arrive as Copy/Cut/Paste events, not key presses.
    let take = |want: fn(&egui::Event) -> bool| ctx.input_mut(|i| { let had = i.events.iter().any(want); if had { i.events.retain(|e| !want(e)); } had });
    let (copy_ev, cut_ev, paste_ev) = (take(|e| matches!(e, egui::Event::Copy)), take(|e| matches!(e, egui::Event::Cut)), take(|e| matches!(e, egui::Event::Paste(_))));
    if pressed(cmd_m, Key::C) || copy_ev { app.copy(); }
    if pressed(cmd_m, Key::X) || cut_ev { app.cut(); }
    if pressed(cmd_m, Key::V) || paste_ev { app.paste(); }
    if pressed(cmd_m | Modifiers::ALT, Key::V) { app.dialog = Some(crate::app::Dialog::PasteSpecial { how: Default::default(), op: 0 }); }
    if pressed(cmd_m, Key::B) { let b = !app.style().bold; app.apply_style("Bold", move |s| s.bold = b); }
    if pressed(cmd_m, Key::I) { let b = !app.style().italic; app.apply_style("Italic", move |s| s.italic = b); }
    if pressed(cmd_m, Key::U) { let u = app.style().underline == Underline::None; app.apply_style("Underline", move |s| s.underline = if u { Underline::Single } else { Underline::None }); }
    if pressed(cmd_m, Key::A) {
        let r = app.book.current_region(app.sheet(), app.sel.active);
        if app.sel.range() == r || r.is_cell() { app.select_all(); } else { app.sel = Selection { anchor: r.start(), active: r.end(), extra: vec![] }; }
    }
    if pressed(cmd_m, Key::D) { app.fill_dir(0); }
    if pressed(cmd_m, Key::R) { app.fill_dir(1); }
    if pressed(cmd_m, Key::F) { crate::dialogs::open_find(app, false); }
    if pressed(cmd_m, Key::H) { crate::dialogs::open_find(app, true); }
    if pressed(cmd_m, Key::G) || pressed(Modifiers::NONE, Key::F5) { app.dialog = Some(crate::app::Dialog::GoTo { text: String::new() }); }
    if pressed(cmd_m, Key::Num1) { crate::dialogs::open_format_cells(app, 0); }
    if pressed(cmd_m, Key::K) { let p = app.sel.active; let t = app.book.sheets[app.sheet()].links.get(&p).map(|l| l.target.clone()).unwrap_or_default(); app.dialog = Some(crate::app::Dialog::Link { pos: p, target: t }); }
    if pressed(cmd_m, Key::Semicolon) { let d = sheet_engine::numfmt::format_number(sheet_engine::numfmt::now_serial().floor(), "m/d/yyyy", false).text; app.begin_edit(Some(d), false); }
    if pressed(cmd_m | Modifiers::SHIFT, Key::Semicolon) { let d = sheet_engine::numfmt::format_number(sheet_engine::numfmt::now_serial().fract(), "h:mm AM/PM", false).text; app.begin_edit(Some(d), false); }
    if pressed(Modifiers::CTRL, Key::Space) { app.sel.anchor.row = 0; app.sel.active.row = MAX_ROWS - 1; }
    if pressed(Modifiers::SHIFT, Key::Space) { app.sel.anchor.col = 0; app.sel.active.col = MAX_COLS - 1; }
    if pressed(Modifiers::NONE, Key::F9) { sheet_engine::whatif::recalc_all(&mut app.book); }
    if pressed(Modifiers::NONE, Key::Escape) { app.grid.marching = None; app.cut = None; app.painter = None; }
    if pressed(Modifiers::NONE, Key::F2) { app.begin_edit(None, false); return; }
    if pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace) { app.clear(true, false, false); }
    // Movement.
    let page = 20;
    for (k, dr, dc) in [(Key::ArrowDown, 1, 0), (Key::ArrowUp, -1, 0), (Key::ArrowLeft, 0, -1), (Key::ArrowRight, 0, 1), (Key::PageDown, page, 0), (Key::PageUp, -page, 0)] {
        let (dr, dc) = if app.book.sheets[app.sheet()].right_to_left && dr == 0 { (dr, -dc) } else { (dr, dc) };
        if pressed(Modifiers::NONE, k) { app.step(dr, dc, false); }
        if pressed(Modifiers::SHIFT, k) { app.step(dr, dc, true); }
        if pressed(cmd_m, k) { app.jump(dr.signum(), dc.signum(), false); }
        if pressed(cmd_m | Modifiers::SHIFT, k) { app.jump(dr.signum(), dc.signum(), true); }
    }
    if pressed(Modifiers::NONE, Key::Home) { let r = app.sel.active.row; app.select(Pos::new(r, 0), false); }
    if pressed(cmd_m, Key::Home) { app.select(Pos::new(0, 0), false); }
    if pressed(cmd_m, Key::End) { let e = app.book.sheets[app.sheet()].extent().unwrap_or_default(); app.select(e, false); }
    let enter_mv = match app.prefs.enter_moves { 0 => (1, 0), 1 => (0, 1), 2 => (-1, 0), 3 => (0, -1), _ => (0, 0) };
    if pressed(Modifiers::NONE, Key::Enter) { app.commit(None); app.step_in_selection(enter_mv.0, enter_mv.1); }
    if pressed(Modifiers::SHIFT, Key::Enter) { app.step_in_selection(-enter_mv.0, -enter_mv.1); }
    if pressed(Modifiers::NONE, Key::Tab) { app.step_in_selection(0, 1); }
    if pressed(Modifiers::SHIFT, Key::Tab) { app.step_in_selection(0, -1); }
    // Typing starts an entry that replaces the cell.
    let typed: String = ctx.input(|i| i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect());
    if !typed.is_empty() && !ctx.input(|i| i.modifiers.command) {
        app.begin_edit(Some(typed), false);
        if app.edit.is_some() { ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_)))); }
    }
}

impl SheetApp {
    /// Enter and Tab move within a multi-cell selection, else step.
    pub fn step_in_selection(&mut self, dr: i64, dc: i64) {
        let r = self.sel.range();
        if r.is_cell() || !self.sel.extra.is_empty() { self.step(dr, dc, false); return; }
        let mut p = self.sel.active;
        if dr > 0 { if p.row < r.r1 { p.row += 1; } else { p.row = r.r0; p.col = if p.col < r.c1 { p.col + 1 } else { r.c0 }; } }
        if dr < 0 { if p.row > r.r0 { p.row -= 1; } else { p.row = r.r1; p.col = if p.col > r.c0 { p.col - 1 } else { r.c1 }; } }
        if dc > 0 { if p.col < r.c1 { p.col += 1; } else { p.col = r.c0; p.row = if p.row < r.r1 { p.row + 1 } else { r.r0 }; } }
        if dc < 0 { if p.col > r.c0 { p.col -= 1; } else { p.col = r.c1; p.row = if p.row > r.r0 { p.row - 1 } else { r.r1 }; } }
        self.sel.active = p;
        self.reveal = true;
    }
}

pub(crate) fn context_menu(app: &mut SheetApp, resp: &egui::Response) {
    resp.context_menu(|ui| {
        ui.set_min_width(200.0);
        let r = app.sel.range();
        let whole_rows = r.c0 == 0 && r.c1 == MAX_COLS - 1;
        let whole_cols = r.r0 == 0 && r.r1 == MAX_ROWS - 1;
        let item = |ui: &mut Ui, icon: &str, label: &str| qalam_kit::widgets::menu_item(ui, icon, label);
        if item(ui, "cut", "Cut") { app.cut(); ui.close(); }
        if item(ui, "copy", "Copy") { app.copy(); ui.close(); }
        if item(ui, "paste", "Paste") { app.paste(); ui.close(); }
        if item(ui, "pasteText", "Paste Special…") { app.dialog = Some(crate::app::Dialog::PasteSpecial { how: Default::default(), op: 0 }); ui.close(); }
        ui.separator();
        if whole_rows {
            if item(ui, "insertAbove", "Insert Rows") { app.insert_rows_cols(true, false); ui.close(); }
            if item(ui, "deleteRow", "Delete Rows") { app.insert_rows_cols(true, true); ui.close(); }
        } else if whole_cols {
            if item(ui, "insertLeft", "Insert Columns") { app.insert_rows_cols(false, false); ui.close(); }
            if item(ui, "deleteColumn", "Delete Columns") { app.insert_rows_cols(false, true); ui.close(); }
        } else {
            if item(ui, "insertCells", "Insert…") { app.dialog = Some(crate::app::Dialog::Insert { delete: false, choice: 1 }); ui.close(); }
            if item(ui, "deleteCells", "Delete…") { app.dialog = Some(crate::app::Dialog::Insert { delete: true, choice: 1 }); ui.close(); }
        }
        if item(ui, "clearFormat", "Clear Contents") { app.clear(true, false, false); ui.close(); }
        ui.separator();
        if item(ui, "sort", "Sort A to Z") { app.sort(true); ui.close(); }
        if item(ui, "sort", "Sort Z to A") { app.sort(false); ui.close(); }
        ui.separator();
        let p = app.sel.active;
        let has_note = app.book.sheets[app.sheet()].notes.contains_key(&p);
        if item(ui, "newComment", if has_note { "Edit Note" } else { "Insert Note" }) { let t = app.book.sheets[app.sheet()].notes.get(&p).cloned().unwrap_or_default(); app.dialog = Some(crate::app::Dialog::Note { pos: p, text: t }); ui.close(); }
        if has_note && item(ui, "deleteComment", "Delete Note") { app.checkpoint("Delete Note"); let s = app.sheet(); app.book.sheets[s].notes.remove(&p); app.dirty = true; ui.close(); }
        crate::review_tab::cell_menu(app, ui);
        if item(ui, "formatPainter", "Format Cells…") { crate::dialogs::open_format_cells(app, 0); ui.close(); }
        crate::pivot_ui::cell_menu(app, ui);
        if whole_cols {
            if item(ui, "columnWidth", "Column Width…") { let w = app.book.sheets[app.sheet()].col_width(r.c0); app.dialog = Some(crate::app::Dialog::Size { cols: true, value: w }); ui.close(); }
            if item(ui, "hideInk", "Hide") { app.checkpoint("Hide"); let s = app.sheet(); for c in r.c0..=r.c1 { app.book.sheets[s].hidden_cols.insert(c); } app.dirty = true; ui.close(); }
            if item(ui, "showMarkup", "Unhide") { app.checkpoint("Unhide"); let s = app.sheet(); for c in r.c0..=r.c1 { app.book.sheets[s].hidden_cols.remove(&c); } app.dirty = true; ui.close(); }
        }
        if whole_rows {
            if item(ui, "rowHeight", "Row Height…") { let h = app.book.sheets[app.sheet()].row_height(r.r0); app.dialog = Some(crate::app::Dialog::Size { cols: false, value: h }); ui.close(); }
            if item(ui, "hideInk", "Hide") { app.checkpoint("Hide"); let s = app.sheet(); for rw in r.r0..=r.r1 { app.book.sheets[s].hidden_rows.insert(rw); } app.dirty = true; ui.close(); }
            if item(ui, "showMarkup", "Unhide") { app.checkpoint("Unhide"); let s = app.sheet(); for rw in r.r0..=r.r1 { app.book.sheets[s].hidden_rows.remove(&rw); } app.dirty = true; ui.close(); }
        }
        if item(ui, "link", "Link…") { let t = app.book.sheets[app.sheet()].links.get(&p).map(|l| l.target.clone()).unwrap_or_default(); app.dialog = Some(crate::app::Dialog::Link { pos: p, target: t }); ui.close(); }
    });
    let _ = tr;
}
