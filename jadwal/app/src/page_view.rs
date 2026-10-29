//! Workbook views (page-layout.js, pagebreaks.js): View ▸ Normal / Page Break Preview / Page
//! Layout, and Insert ▸ Header & Footer.
//!
//! Page Layout lays the live grid out on the printed pages (the print renderer's page breaks and
//! scale, with gaps between pages): cells select, edit in place, fill and move as in Normal view
//! (grid.rs draws and handles them through a page-mapped layout; frozen panes are ignored), each
//! page has its column letters and row numbers (click to select, drag an edge to resize), and a
//! page of empty cells right of and below the data says "Click to add data". Rulers above and left
//! show the margins in the unit chosen in Options; drag a margin's edge to change it. The header
//! and footer areas above and below each page are drawn by the print renderer (formatting and
//! pictures as they print): click a left, center or right section to edit it, which opens the
//! Header & Footer tab with its elements, font controls (the Home tab's work too) and Picture.
//!
//! Page Break Preview keeps the grid but grays everything that doesn't print, outlines the pages
//! in blue (dashed for automatic breaks, solid for manual ones), labels them, and lets you drag a
//! break line to move it (that makes it a manual break).

use crate::app::{SheetApp, Tab};
use crate::grid;
use crate::i18n::tr;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};
use sheet_engine::print::{self, PrintPage, PX_PER_INCH};
use sheet_engine::addr::{MAX_COLS, MAX_ROWS};
use sheet_engine::{Pos, Range};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode { #[default] Normal, PageBreak, PageLayout }

/// A header or footer section being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct HfEdit { pub page: usize, pub footer: bool, pub section: usize, pub text: String, pub fresh: bool }

#[derive(Default)]
struct St {
    mode: ViewMode,
    scroll: egui::Vec2,
    edit: Option<HfEdit>,
    tex: HashMap<usize, (u64, Option<egui::TextureHandle>)>,
    breaks: Option<(String, BTreeSet<u32>, BTreeSet<u32>)>,
    drag: Option<(bool, u32)>,
    hide_ruler: bool,
    /// A ruler margin being dragged: which, and where its page starts.
    margin_drag: Option<(u8, f32)>,
    markers: Vec<(u8, Rect)>,
    /// A column (true) or row being resized at a page's headers: index, pointer start, size.
    size_drag: Option<(bool, u32, f32, f32)>,
    pic_dlg: Option<PicDlg>,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St::default()); }

pub fn mode() -> ViewMode { ST.with(|s| s.borrow().mode) }
pub fn set_mode(app: &mut SheetApp, m: ViewMode) {
    ST.with(|s| { let mut s = s.borrow_mut(); s.mode = m; s.edit = None; s.drag = None; });
    if m == ViewMode::Normal && app.tab == Tab::HeaderFooter { app.tab = Tab::View; }
    app.reveal = true;
}
pub fn editing() -> Option<HfEdit> { ST.with(|s| s.borrow().edit.clone()) }

/// View ▸ Workbook Views.
pub fn views_group(app: &mut SheetApp, ui: &mut Ui) {
    use qalam_kit::widgets::{group, large};
    let m = mode();
    group(ui, "Workbook Views", |ui| {
        if large(ui, "table", "Normal", m == ViewMode::Normal, true, "Show the sheet as a grid").clicked() { set_mode(app, ViewMode::Normal); }
        if large(ui, "pageBreakPreview", "Page Break Preview", m == ViewMode::PageBreak, true, "See where pages break and drag the breaks").clicked() { set_mode(app, ViewMode::PageBreak); }
        if large(ui, "printLayout", "Page Layout", m == ViewMode::PageLayout, true, "See pages as they print, with headers and footers").clicked() { set_mode(app, ViewMode::PageLayout); }
    });
}

/// Insert ▸ Header & Footer: Page Layout view with the first page's center header open.
pub fn header_footer(app: &mut SheetApp) {
    app.commit(None);
    set_mode(app, ViewMode::PageLayout);
    let si = app.sheet();
    let text = split(&page_hf(&app.book.sheets[si].page.header_footer, 0, false))[1].clone();
    ST.with(|s| s.borrow_mut().edit = Some(HfEdit { page: 0, footer: false, section: 1, text, fresh: true }));
    app.tab = Tab::HeaderFooter;
}

// ---- header/footer text ----

/// Splits header/footer text into its left, center and right sections (text without &L &C &R
/// is centered).
pub fn split(s: &str) -> [String; 3] {
    let mut out = [String::new(), String::new(), String::new()];
    let mut sec = 1;
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '&' && i + 1 < cs.len() {
            match cs[i + 1] {
                'L' => { sec = 0; i += 2; continue; }
                'C' => { sec = 1; i += 2; continue; }
                'R' => { sec = 2; i += 2; continue; }
                c => { out[sec].push('&'); out[sec].push(c); i += 2; continue; }
            }
        }
        out[sec].push(cs[i]);
        i += 1;
    }
    out
}

pub fn join(p: &[String; 3]) -> String {
    let mut s = String::new();
    for (code, t) in ["&L", "&C", "&R"].iter().zip(p) { if !t.is_empty() { s.push_str(code); s.push_str(t); } }
    s
}

/// The header (or footer) text page `pos` (0-based within the sheet) uses.
fn page_hf(h: &print::HeaderFooter, pos: usize, footer: bool) -> String {
    let (a, b) = h.for_page(pos as u32 + 1);
    if footer { b.to_string() } else { a.to_string() }
}

fn set_page_hf(h: &mut print::HeaderFooter, pos: usize, footer: bool, text: String) {
    let n = pos as u32 + 1;
    let f = if h.different_first && n == 1 { if footer { &mut h.first_footer } else { &mut h.first_header } }
        else if h.different_odd_even && n % 2 == 0 { if footer { &mut h.even_footer } else { &mut h.even_header } }
        else if footer { &mut h.odd_footer } else { &mut h.odd_header };
    *f = text;
}

/// A section's codes filled in as plain text (for tests and the edit box's hint).
pub fn evaluate(src: &str, page: u32, pages: u32, sheet: &str, file: &str, folder: &str, now: f64) -> String {
    let mut out = String::new();
    let cs: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] != '&' || i + 1 >= cs.len() { out.push(cs[i]); i += 1; continue; }
        let d = cs[i + 1];
        i += 2;
        match d {
            'P' => out.push_str(&page.to_string()),
            'N' => out.push_str(&pages.to_string()),
            'D' => out.push_str(&sheet_engine::numfmt::format(&sheet_engine::Value::Num(now.floor()), "m/d/yyyy", false).text),
            'T' => out.push_str(&sheet_engine::numfmt::format(&sheet_engine::Value::Num(now), "h:mm AM/PM", false).text),
            'F' => out.push_str(file),
            'Z' => out.push_str(folder),
            'A' => out.push_str(sheet),
            'G' => out.push_str("[Picture]"),
            '&' => out.push('&'),
            '"' => { while i < cs.len() && cs[i] != '"' { i += 1; } i += 1; }
            'K' => i += 6.min(cs.len() - i),
            c if c.is_ascii_digit() => { while i < cs.len() && cs[i].is_ascii_digit() { i += 1; } }
            _ => {}
        }
    }
    out
}

// ---- pages ----

/// The pages Page Layout shows: the sheet's printed pages, or one empty page.
pub fn view_pages(app: &SheetApp) -> Vec<PrintPage> {
    let si = app.sheet();
    let pages = print::sheet_pages(&app.book, si, None);
    if !pages.is_empty() { return pages; }
    let s = &app.book.sheets[si];
    let (w, h) = s.page.paper_inches();
    let m = &s.page.margins;
    let (aw, ah) = ((w - m.left - m.right) * PX_PER_INCH, (h - m.top - m.bottom) * PX_PER_INCH);
    let (mut c, mut x) = (0u32, 0.0);
    while x + s.col_width(c) <= aw && c < 200 { x += s.col_width(c); c += 1; }
    let (mut r, mut y) = (0u32, 0.0);
    while y + s.row_height(r) <= ah && r < 400 { y += s.row_height(r); r += 1; }
    vec![PrintPage { sheet: si, rows: (0, r.saturating_sub(1)), cols: (0, c.saturating_sub(1)), title_rows: None, title_cols: None, number: 1, count: 1, scale: 1.0 }]
}

const GAP: f32 = 18.0;
const RULER: f32 = 18.0;

/// View ▸ Ruler (shown in Page Layout view).
pub fn ruler_shown() -> bool { ST.with(|s| !s.borrow().hide_ruler) }
pub fn set_ruler(on: bool) { ST.with(|s| s.borrow_mut().hide_ruler = !on); }

/// Where the pages sit this frame: page columns (runs of sheet columns) across, page rows down,
/// with one more of each of empty cells for "Click to add data".
struct Geo {
    area: Rect,
    z: f32,
    /// Cells' scale on screen: the zoom times the print scale.
    k: f32,
    pw: f32,
    ph: f32,
    cols: Vec<(u32, u32)>,
    rows: Vec<(u32, u32)>,
    /// The print renderer's runs (the pages' own), before the last is filled out.
    pcols: Vec<(u32, u32)>,
    prows: Vec<(u32, u32)>,
    px: Vec<f32>,
    py: Vec<f32>,
    ox: Vec<f32>,
    oy: Vec<f32>,
    total: egui::Vec2,
    head: egui::Vec2,
    pages: Vec<PrintPage>,
}

impl Geo {
    fn page_rect(&self, ri: usize, ci: usize) -> Rect { Rect::from_min_size(pos2(self.px[ci], self.py[ri]), vec2(self.pw, self.ph)) }
    fn page_of(&self, ri: usize, ci: usize) -> Option<usize> { let (r, c) = (self.prows.get(ri)?, self.pcols.get(ci)?); self.pages.iter().position(|p| p.rows == *r && p.cols == *c) }
}

fn geo(app: &SheetApp, area: Rect, scroll: egui::Vec2) -> Geo {
    let s = &app.book.sheets[app.sheet()];
    let pages = view_pages(app);
    let z = app.zoom;
    let scale = pages.first().map_or(1.0, |p| p.scale);
    let k = z * scale;
    let (w, h) = s.page.paper_inches();
    let m = s.page.margins;
    let (pw, ph) = (w * PX_PER_INCH * z, h * PX_PER_INCH * z);
    let mut cols: Vec<(u32, u32)> = pages.iter().map(|p| p.cols).collect::<BTreeSet<_>>().into_iter().collect();
    let mut rows: Vec<(u32, u32)> = pages.iter().map(|p| p.rows).collect::<BTreeSet<_>>().into_iter().collect();
    let (aw, ah) = (((w - m.left - m.right) * PX_PER_INCH / scale).max(24.0), ((h - m.top - m.bottom) * PX_PER_INCH / scale).max(24.0));
    let fit = |start: u32, size: &dyn Fn(u32) -> f32, avail: f32, max: u32| -> (u32, u32) {
        let (mut e, mut acc) = (start, 0.0);
        while e + 1 < max && e - start < 400 { let sz = size(e); if acc + sz > avail && e > start { break; } acc += sz; e += 1; }
        (start, e.saturating_sub(1).max(start))
    };
    let (pcols, prows) = (cols.clone(), rows.clone());
    // The last pages show empty cells to their edges, as Excel does; then a page of empty cells.
    if let Some(last) = cols.last_mut() { last.1 = last.1.max(fit(last.0, &|c| s.col_width(c), aw, MAX_COLS).1); }
    if let Some(last) = rows.last_mut() { last.1 = last.1.max(fit(last.0, &|r| s.row_height(r), ah, MAX_ROWS).1); }
    if let Some(&(_, c1)) = cols.last() { if c1 + 1 < MAX_COLS { cols.push(fit(c1 + 1, &|c| s.col_width(c), aw, MAX_COLS)); } }
    if let Some(&(_, r1)) = rows.last() { if r1 + 1 < MAX_ROWS { rows.push(fit(r1 + 1, &|r| s.row_height(r), ah, MAX_ROWS)); } }
    let head = if s.headers { vec2(30.0 * z.clamp(0.7, 1.4), 16.0 * z.clamp(0.7, 1.4)) } else { egui::Vec2::ZERO };
    let gap = vec2(GAP, GAP) + head;
    let total = vec2(cols.len() as f32 * (pw + gap.x) + GAP, rows.len() as f32 * (ph + gap.y) + GAP);
    let x0 = area.min.x + ((area.width() - total.x) / 2.0).max(0.0) - scroll.x;
    let y0 = area.min.y - scroll.y;
    let px: Vec<f32> = (0..cols.len()).map(|i| x0 + gap.x + i as f32 * (pw + gap.x)).collect();
    let py: Vec<f32> = (0..rows.len()).map(|i| y0 + gap.y + i as f32 * (ph + gap.y)).collect();
    let n_data = (cols.len().saturating_sub(1), rows.len().saturating_sub(1));
    let span = |a: u32, b: u32, f: &dyn Fn(u32) -> f32| (a..=b.min(a + 2000)).map(f).sum::<f32>() * k;
    let ox = cols.iter().enumerate().map(|(i, c)| { let c = pcols.get(i).unwrap_or(c); px[i] + m.left * PX_PER_INCH * z + if s.page.center_h && i < n_data.0 { ((w - m.left - m.right) * PX_PER_INCH * z - span(c.0, c.1, &|c| s.col_width(c))).max(0.0) / 2.0 } else { 0.0 } }).collect();
    let oy = rows.iter().enumerate().map(|(i, r)| { let r = prows.get(i).unwrap_or(r); py[i] + m.top * PX_PER_INCH * z + if s.page.center_v && i < n_data.1 { ((h - m.top - m.bottom) * PX_PER_INCH * z - span(r.0, r.1, &|r| s.row_height(r))).max(0.0) / 2.0 } else { 0.0 } }).collect();
    Geo { area, z, k, pw, ph, cols, rows, pcols, prows, px, py, ox, oy, total, head, pages }
}

/// The grid's layout mapped onto the pages: the real grid drawing and mouse work on it.
fn grid_layout(app: &SheetApp, g: &Geo) -> grid::Layout {
    let s = &app.book.sheets[app.sheet()];
    let map = grid::PageMap { cols: g.cols.iter().zip(&g.ox).map(|(c, x)| (c.0, c.1, *x)).collect(), rows: g.rows.iter().zip(&g.oy).map(|(r, y)| (r.0, r.1, *y)).collect() };
    // The grid keeps a scrollbar's width at the right and bottom of its rectangle.
    grid::Layout { rect: Rect::from_min_max(g.area.min, g.area.max + vec2(12.0, 12.0)), z: g.k, cols: grid::Axis::of_cols(s), rows: grid::Axis::of_rows(s), fr: 0, or: 0, oc: 0, fc: 0, head_w: 0.0, head_h: 0.0, scroll: egui::Vec2::ZERO, rtl: false, pages: Some(std::rc::Rc::new(map)) }
}

fn area_of(full: Rect) -> Rect { if ruler_shown() { Rect::from_min_max(full.min + vec2(RULER, RULER), full.max) } else { full } }

/// The layout Page Layout view used last frame (for tests and tools that place things on cells).
pub fn current_layout(app: &SheetApp) -> grid::Layout {
    let scroll = ST.with(|s| s.borrow().scroll);
    grid_layout(app, &geo(app, area_of(app.grid_rect), scroll))
}

/// A ruler's margin marker as drawn last frame: 0 left, 1 right, 2 top, 3 bottom, 4 header, 5 footer.
pub fn ruler_marker(which: u8) -> Option<Rect> { ST.with(|s| s.borrow().markers.iter().find(|m| m.0 == which).map(|m| m.1)) }

fn hf_fingerprint(app: &SheetApp, pg: &PrintPage, size: egui::Vec2) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let s = &app.book.sheets[pg.sheet];
    format!("{}{}{:?}{:?}{:?}{}{}{}", pg.number, pg.count, s.page.header_footer, s.page.margins, s.page.paper_inches(), pg.scale, app.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(), s.name).hash(&mut h);
    (size.x.to_bits(), size.y.to_bits(), app.ctx.pixels_per_point().to_bits()).hash(&mut h);
    h.finish()
}

/// The header and footer of a page, drawn by the print renderer (so formatting and pictures
/// show as they print), on a transparent texture.
fn hf_texture(app: &SheetApp, k: usize, pg: &PrintPage, size: egui::Vec2) -> Option<egui::TextureHandle> {
    let (a, b) = app.book.sheets[pg.sheet].page.header_footer.for_page(pg.number);
    if a.is_empty() && b.is_empty() { return None; }
    let key = hf_fingerprint(app, pg, size);
    if let Some(t) = ST.with(|s| s.borrow().tex.get(&k).filter(|t| t.0 == key).map(|t| t.1.clone())) { return t; }
    let mut assets = print::objects::Assets::new();
    let pb = print::header_footer_page(&app.book, pg, &crate::print::job_info(app), &mut assets);
    let lay = word_engine::layout::Layout { pages: vec![pb], ..Default::default() };
    let mut doc = word_engine::Document::default();
    doc.assets.extend(assets);
    let svg = word_engine::svg::page_svg(&doc, &lay, 0);
    let t = (|| {
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
        let scale = size.x / tree.size().width().max(1.0) * app.ctx.pixels_per_point();
        let (w, h) = ((tree.size().width() * scale).ceil() as u32, (tree.size().height() * scale).ceil() as u32);
        let mut pm = resvg::tiny_skia::Pixmap::new(w.clamp(1, 4096), h.clamp(1, 4096))?;
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
        let img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
        Some(app.ctx.load_texture(format!("page-layout-hf-{k}"), img, egui::TextureOptions::LINEAR))
    })();
    ST.with(|s| s.borrow_mut().tex.insert(k, (key, t.clone())));
    t
}

/// The header (or footer) zone of a page, and its three sections.
fn zones(app: &SheetApp, r: Rect, footer: bool) -> [Rect; 3] {
    let m = &app.book.sheets[app.sheet()].page.margins;
    let z = app.zoom * PX_PER_INCH;
    let (x0, x1) = (r.min.x + m.left * z, r.max.x - m.right * z);
    let (y0, y1) = if footer { (r.max.y - m.bottom * z + 2.0, r.max.y - (m.footer * z - 6.0).max(4.0)) } else { (r.min.y + (m.header * z - 6.0).max(4.0), r.min.y + m.top * z - 2.0) };
    let (y0, y1) = (y0.min(y1 - 14.0), y1);
    let w = (x1 - x0) / 3.0;
    [0, 1, 2].map(|i| Rect::from_min_max(pos2(x0 + i as f32 * w, y0), pos2(x0 + (i + 1) as f32 * w, y1)))
}

/// Draws Page Layout view instead of the grid; false in the other views.
pub fn grid_hook(app: &mut SheetApp, ui: &mut Ui) -> bool {
    if mode() != ViewMode::PageLayout { return false; }
    let full = ui.available_rect_before_wrap();
    app.grid_rect = full;
    let resp = ui.allocate_rect(full, Sense::click_and_drag());
    let painter = ui.painter_at(full);
    painter.rect_filled(full, 0.0, if app.dark { Color32::from_gray(0x30) } else { Color32::from_gray(0xE6) });
    let area = area_of(full);
    let mut scroll = ST.with(|s| s.borrow().scroll);
    let g = geo(app, area, scroll);
    // Keyboard moves keep the active cell in view.
    if app.reveal {
        app.reveal = false;
        let r = grid_layout(app, &g).cell_rect(app.sel.active);
        if r.is_finite() {
            if r.min.x < area.min.x { scroll.x -= area.min.x - r.min.x + 8.0; } else if r.max.x > area.max.x { scroll.x += (r.max.x - area.max.x + 8.0).min(r.min.x - area.min.x); }
            if r.min.y < area.min.y { scroll.y -= area.min.y - r.min.y + 8.0; } else if r.max.y > area.max.y { scroll.y += (r.max.y - area.max.y + 8.0).min(r.min.y - area.min.y); }
        }
    }
    let clamp = |sc: egui::Vec2, g: &Geo| vec2(sc.x.clamp(0.0, (g.total.x - area.width()).max(0.0)), sc.y.clamp(0.0, (g.total.y - area.height()).max(0.0)));
    scroll = clamp(scroll, &g);
    ST.with(|s| s.borrow_mut().scroll = scroll);
    let g = geo(app, area, scroll);
    let lay = grid_layout(app, &g);
    let si = app.sheet();
    let time = ui.input(|i| i.time);
    let edit = editing();
    let hf = app.book.sheets[si].page.header_footer.clone();
    let ap = painter.with_clip_rect(area);
    let mut cells_at: Vec<Rect> = vec![];
    let mut page_rects: HashMap<usize, Rect> = HashMap::new();
    let mut clicked_zone: Option<(usize, bool, usize)> = None;
    let faint = Color32::from_gray(0x9A);
    for ri in 0..g.rows.len() {
        for ci in 0..g.cols.len() {
            let r = g.page_rect(ri, ci);
            if !r.intersects(area) { continue; }
            let pg = g.page_of(ri, ci);
            let band = Range { r0: g.rows[ri].0, r1: g.rows[ri].1, c0: g.cols[ci].0, c1: g.cols[ci].1 };
            let live = pg.is_some() || band.contains(app.sel.active);
            ap.rect_filled(r.translate(vec2(2.0, 2.0)), 0.0, Color32::from_black_alpha(40));
            ap.rect_filled(r, 0.0, if live { Color32::WHITE } else { Color32::from_gray(0xF6) });
            // The header and footer (under the cells, which they don't reach).
            if let Some(k) = pg { if let Some(t) = hf_texture(app, k, &g.pages[k], r.size()) { ap.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); } }
            let cr = lay.range_rect(band);
            cells_at.push(cr.intersect(area));
            if live {
                let cp = painter.with_clip_rect(cr.intersect(area));
                grid::draw_cells(app, &cp, &lay, band.r0..=band.r1, band.c0..=band.c1);
                crate::filter_ui::draw(app, &cp, &lay, band.r0..=band.r1, band.c0..=band.c1);
                crate::formulas_tab::draw_triangles(app, &cp, &lay, band.r0..=band.r1, band.c0..=band.c1);
                grid::draw_selection(app, &cp, &lay, time);
            } else {
                ap.text(r.center(), Align2::CENTER_CENTER, tr("Click to add data"), FontId::proportional(13.0 * app.zoom.clamp(0.6, 1.5)), faint);
            }
            let Some(k) = pg else { continue };
            page_rects.insert(k, r);
            for footer in [false, true] {
                let zs = zones(app, r, footer);
                let text = split(&page_hf(&hf, k, footer));
                for (sec, z) in zs.iter().enumerate() {
                    let editing_here = edit.as_ref().is_some_and(|e| e.page == k && e.footer == footer && e.section == sec);
                    let zr = ui.interact(z.intersect(area), egui::Id::new(("hf-zone", k, footer, sec)), Sense::click());
                    if zr.hovered() && !editing_here { ap.rect_stroke(*z, 0.0, Stroke::new(1.0, Color32::from_gray(0x90)), StrokeKind::Inside); }
                    if text.iter().all(|t| t.is_empty()) && sec == 1 && !editing_here {
                        ap.text(z.center(), Align2::CENTER_CENTER, tr(if footer { "Click to add footer" } else { "Click to add header" }), FontId::proportional(11.0 * app.zoom.clamp(0.7, 1.5)), if zr.hovered() { Color32::from_gray(0x60) } else { Color32::from_gray(0xB0) });
                    }
                    if zr.clicked() { clicked_zone = Some((k, footer, sec)); }
                }
            }
            if app.book.sheets[si].headers { page_headers(app, ui, &ap, &lay, &g, band, r, (ri, ci)); }
            ap.text(pos2(r.center().x, r.max.y + GAP / 2.0), Align2::CENTER_CENTER, format!("{} {}", tr("Page"), g.pages[k].number), FontId::proportional(10.0), Color32::from_gray(0x80));
        }
    }
    // Cells: the grid's own mouse work (selection, fill handle, moving, editing on double-click).
    let press = ui.input(|i| i.pointer.press_origin());
    let on_cells = press.is_none_or(|p| cells_at.iter().any(|r| r.contains(p)));
    if on_cells || app.grid.drag != grid::Drag::None {
        // The grid scrolls `app.scroll`; here that moves the pages instead.
        let saved = app.scroll;
        let probe = vec2(1.0e5, 1.0e5);
        app.scroll = probe;
        grid::mouse(app, ui, &resp, &lay);
        let d = app.scroll - probe;
        app.scroll = saved;
        if d != egui::Vec2::ZERO { ST.with(|s| { let mut s = s.borrow_mut(); s.scroll = clamp(s.scroll + d * lay.z, &g); }); }
    } else if resp.hovered() {
        let (d, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        if zoom != 1.0 { app.zoom = (app.zoom * zoom).clamp(0.1, 4.0); }
        ST.with(|s| { let mut s = s.borrow_mut(); s.scroll = clamp(s.scroll - d, &g); });
    }
    grid::context_menu(app, &resp);
    crate::objects::grid_hook(app, ui, &ap, &lay, &resp);
    if ruler_shown() { rulers(app, ui, &painter, &g, full); }
    // The header/footer section being edited.
    let mut commit: Option<HfEdit> = None;
    let mut cancel = false;
    if let Some(mut e) = edit.clone() {
        if let Some(r) = page_rects.get(&e.page) {
            let z = zones(app, *r, e.footer)[e.section];
            painter.rect_filled(z, 0.0, Color32::WHITE);
            painter.rect_stroke(z, 0.0, Stroke::new(1.0, crate::grid::SELECT), StrokeKind::Outside);
            let id = egui::Id::new("hf-section-edit");
            let out = ui.put(z.shrink(2.0), egui::TextEdit::singleline(&mut e.text).id(id).horizontal_align(match e.section { 0 => egui::Align::Min, 1 => egui::Align::Center, _ => egui::Align::Max }).frame(egui::Frame::NONE));
            if e.fresh { out.request_focus(); e.fresh = false; }
            if out.has_focus() { format_keys(ui, &mut e); }
            let pg = &g.pages[e.page.min(g.pages.len() - 1)];
            let info = crate::print::job_info(app);
            let preview = evaluate(&e.text, pg.number, pg.count, &app.book.sheets[si].name, &info.file_name, &info.folder, sheet_engine::numfmt::now_serial());
            if preview != e.text { painter.text(pos2(z.center().x, if e.footer { z.min.y - 8.0 } else { z.max.y + 8.0 }), Align2::CENTER_CENTER, preview, FontId::proportional(10.0), Color32::from_gray(0x70)); }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) { cancel = true; }
            else if out.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { commit = Some(e.clone()); }
            if commit.is_none() && !cancel { ST.with(|s| s.borrow_mut().edit = Some(e)); }
        }
    }
    if let Some((k, footer, sec)) = clicked_zone {
        if let Some(e) = editing() { if !(e.page == k && e.footer == footer && e.section == sec) { commit = Some(e); } }
        if let Some(c) = commit.take() { save_edit(app, &c); }
        app.commit(None);
        let text = split(&page_hf(&app.book.sheets[si].page.header_footer, k, footer))[sec].clone();
        let text = if editing().is_some_and(|e| e.page == k && e.footer == footer && e.section == sec) { editing().unwrap().text } else { text };
        ST.with(|s| s.borrow_mut().edit = Some(HfEdit { page: k, footer, section: sec, text, fresh: true }));
        app.tab = Tab::HeaderFooter;
    } else if resp.clicked() || resp.drag_started() {
        if let Some(e) = editing() { commit = Some(e); }
    }
    if let Some(c) = commit { save_edit(app, &c); close_edit(app); }
    if cancel { close_edit(app); }
    // The in-cell editor.
    if app.edit.as_ref().is_some_and(|e| e.sheet == si && !e.focus_bar) { grid::cell_editor(app, ui, &lay); }
    if app.grid.marching.is_some() { ui.ctx().request_repaint_after(std::time::Duration::from_millis(80)); }
    picture_dialog(app, ui.ctx());
    true
}

/// Column letters above a page and row numbers left of it: click to select, drag an edge to
/// resize.
#[allow(clippy::too_many_arguments)]
fn page_headers(app: &mut SheetApp, ui: &mut Ui, p: &egui::Painter, lay: &grid::Layout, g: &Geo, band: Range, page: Rect, at: (usize, usize)) {
    use qalam_kit::colors::{c_frame, c_ink2, c_line};
    let font = FontId::proportional(10.5 * g.z.clamp(0.7, 1.4));
    let sel = app.sel.ranges();
    let cells = lay.range_rect(band);
    let ch = Rect::from_min_max(pos2(cells.min.x, page.min.y - g.head.y - 2.0), pos2(cells.max.x, page.min.y - 2.0));
    let rh = Rect::from_min_max(pos2(page.min.x - g.head.x - 2.0, cells.min.y), pos2(page.min.x - 2.0, cells.max.y));
    p.rect_filled(ch, 0.0, c_frame());
    p.rect_filled(rh, 0.0, c_frame());
    let tint = Color32::from_rgba_unmultiplied(0x21, 0x73, 0x46, 40);
    for c in band.c0..=band.c1 {
        let w = lay.cols.size(c) * lay.z;
        if w <= 0.0 { continue; }
        let rc = Rect::from_min_size(pos2(lay.x(c), ch.min.y), vec2(w, ch.height()));
        if sel.iter().any(|x| x.c0 <= c && c <= x.c1) { p.rect_filled(rc, 0.0, tint); }
        p.line_segment([rc.right_top(), rc.right_bottom()], Stroke::new(1.0, c_line()));
        if w > 8.0 { p.with_clip_rect(rc.intersect(p.clip_rect())).text(rc.center(), Align2::CENTER_CENTER, sheet_engine::addr::col_name(c), font.clone(), c_ink2()); }
    }
    for r in band.r0..=band.r1 {
        let h = lay.rows.size(r) * lay.z;
        if h <= 0.0 { continue; }
        let rc = Rect::from_min_size(pos2(rh.min.x, lay.y(r)), vec2(rh.width(), h));
        if sel.iter().any(|x| x.r0 <= r && r <= x.r1) { p.rect_filled(rc, 0.0, tint); }
        p.line_segment([rc.left_bottom(), rc.right_bottom()], Stroke::new(1.0, c_line()));
        if h > 7.0 { p.with_clip_rect(rc.intersect(p.clip_rect())).text(rc.center(), Align2::CENTER_CENTER, (r + 1).to_string(), font.clone(), c_ink2()); }
    }
    let edge = |cols: bool, pt: Pos2| -> Option<u32> {
        if cols { (band.c0..=band.c1).find(|&c| lay.cols.size(c) > 0.0 && (pt.x - (lay.x(c) + lay.cols.size(c) * lay.z)).abs() <= 4.0) }
        else { (band.r0..=band.r1).find(|&r| lay.rows.size(r) > 0.0 && (pt.y - (lay.y(r) + lay.rows.size(r) * lay.z)).abs() <= 3.0) }
    };
    for (cols, strip) in [(true, ch), (false, rh)] {
        let resp = ui.interact(strip.intersect(g.area), egui::Id::new(("pl-head", cols, at)), Sense::click_and_drag());
        if let Some(h) = resp.hover_pos() { if edge(cols, h).is_some() || ST.with(|s| s.borrow().size_drag.is_some()) { ui.ctx().set_cursor_icon(if cols { egui::CursorIcon::ResizeColumn } else { egui::CursorIcon::ResizeRow }); } }
        let origin = ui.input(|i| i.pointer.press_origin());
        if resp.drag_started() {
            if let Some(o) = origin {
                if let Some(i) = edge(cols, o) {
                    let size = if cols { lay.cols.size(i) } else { lay.rows.size(i) };
                    app.checkpoint(if cols { "Column Width" } else { "Row Height" });
                    ST.with(|s| s.borrow_mut().size_drag = Some((cols, i, if cols { o.x } else { o.y }, size)));
                }
            }
        }
        if let (true, Some((c, i, from, size)), Some(pt)) = (resp.dragged(), ST.with(|s| s.borrow().size_drag), resp.interact_pointer_pos()) {
            if c == cols {
                let v = (size + ((if cols { pt.x } else { pt.y }) - from) / lay.z).max(0.0);
                let si = app.sheet();
                let s = &mut app.book.sheets[si];
                let (hidden, sizes) = if cols { (&mut s.hidden_cols, &mut s.col_widths) } else { (&mut s.hidden_rows, &mut s.row_heights) };
                if v < 2.0 { hidden.insert(i); } else { hidden.remove(&i); sizes.insert(i, v.round()); }
                app.dirty = true;
            }
        }
        if resp.drag_stopped() { ST.with(|s| s.borrow_mut().size_drag = None); }
        if resp.clicked() {
            if let Some(pt) = resp.interact_pointer_pos() {
                app.commit(None);
                if cols { let c = lay.col_at(pt.x); app.sel = crate::app::Selection { anchor: Pos::new(0, c), active: Pos::new(MAX_ROWS - 1, c), extra: vec![] }; }
                else { let r = lay.row_at(pt.y); app.sel = crate::app::Selection { anchor: Pos::new(r, 0), active: Pos::new(r, MAX_COLS - 1), extra: vec![] }; }
            }
        }
    }
}

/// The rulers above and left of the pages, in the unit chosen in Options, with the margins
/// shaded; dragging a margin's edge changes it.
fn rulers(app: &mut SheetApp, ui: &mut Ui, painter: &egui::Painter, g: &Geo, full: Rect) {
    use qalam_kit::colors::{c_frame, c_ink2, c_line};
    let area = g.area;
    let hr = Rect::from_min_max(pos2(area.min.x, full.min.y), pos2(full.max.x, area.min.y));
    let vr = Rect::from_min_max(pos2(full.min.x, area.min.y), pos2(area.min.x, full.max.y));
    painter.rect_filled(Rect::from_min_max(full.min, area.min), 0.0, c_frame());
    painter.rect_filled(hr, 0.0, c_frame());
    painter.rect_filled(vr, 0.0, c_frame());
    painter.line_segment([hr.left_bottom(), hr.right_bottom()], Stroke::new(1.0, c_line()));
    painter.line_segment([vr.right_top(), vr.right_bottom()], Stroke::new(1.0, c_line()));
    let s = &app.book.sheets[app.sheet()];
    let m = s.page.margins;
    let (w, h) = s.page.paper_inches();
    let zi = PX_PER_INCH * g.z;
    let unit = app.prefs.ruler_units.min(2);
    let n_cols = g.cols.len().saturating_sub(1).max(1);
    let n_rows = g.rows.len().saturating_sub(1).max(1);
    let hp = painter.with_clip_rect(hr);
    let vp = painter.with_clip_rect(vr);
    let mut spans: Vec<(u8, Rect, f32)> = vec![];
    for ci in 0..n_cols {
        let x0 = g.px[ci];
        ruler_strip(&hp, true, hr, x0, g.pw, m.left * zi, m.right * zi, unit, zi, c_ink2());
        spans.push((0, Rect::from_center_size(pos2(x0 + m.left * zi, hr.center().y), vec2(8.0, hr.height())), x0));
        spans.push((1, Rect::from_center_size(pos2(x0 + g.pw - m.right * zi, hr.center().y), vec2(8.0, hr.height())), x0));
    }
    for ri in 0..n_rows {
        let y0 = g.py[ri];
        ruler_strip(&vp, false, vr, y0, g.ph, m.top * zi, m.bottom * zi, unit, zi, c_ink2());
        for (which, y) in [(2u8, y0 + m.top * zi), (3, y0 + g.ph - m.bottom * zi), (4, y0 + m.header * zi), (5, y0 + g.ph - m.footer * zi)] {
            if which >= 4 { vp.add(egui::Shape::convex_polygon(vec![pos2(vr.max.x - 1.0, y), pos2(vr.max.x - 6.0, y - 4.0), pos2(vr.max.x - 6.0, y + 4.0)], c_ink2(), Stroke::NONE)); }
            spans.push((which, Rect::from_center_size(pos2(vr.center().x, y), vec2(vr.width(), 7.0)), y0));
        }
    }
    let names = ["Left Margin", "Right Margin", "Top Margin", "Bottom Margin", "Header Margin", "Footer Margin"];
    let values = [m.left, m.right, m.top, m.bottom, m.header, m.footer];
    let mut markers = vec![];
    let drag = ST.with(|x| x.borrow().margin_drag);
    for (k, (which, rect, start)) in spans.into_iter().enumerate() {
        let clip = if which < 2 { hr } else { vr };
        if !rect.intersects(clip) { continue; }
        if !markers.iter().any(|m: &(u8, Rect)| m.0 == which) { markers.push((which, rect)); }
        let resp = ui.interact(rect.intersect(clip), egui::Id::new(("ruler-margin", which, k)), Sense::drag());
        let label = format!("{}: {}", tr(names[which as usize]), unit_text(values[which as usize], unit));
        let resp = resp.on_hover_text(label);
        if resp.hovered() || resp.dragged() { ui.ctx().set_cursor_icon(if which < 2 { egui::CursorIcon::ResizeColumn } else { egui::CursorIcon::ResizeRow }); }
        if resp.drag_started() { ST.with(|x| x.borrow_mut().margin_drag = Some((which, start))); }
        if resp.drag_stopped() {
            if let (Some(pt), Some((wh, st))) = (resp.interact_pointer_pos(), drag) {
                let v = margin_value(wh, st, pt, g, w, h, &m);
                let label = if wh >= 4 { "Header & Footer Margins" } else { "Margins" };
                crate::print::edit_setup(app, label, move |p| { let mm = &mut p.margins; match wh { 0 => mm.left = v, 1 => mm.right = v, 2 => mm.top = v, 3 => mm.bottom = v, 4 => mm.header = v, _ => mm.footer = v } });
            }
            ST.with(|x| x.borrow_mut().margin_drag = None);
        }
    }
    // The edge being dragged follows the pointer across the pages, with its value.
    if let (Some((wh, st)), Some(pt)) = (ST.with(|x| x.borrow().margin_drag), ui.input(|i| i.pointer.interact_pos())) {
        let v = margin_value(wh, st, pt, g, w, h, &m);
        let blue = Color32::from_rgb(0x22, 0x55, 0xCC);
        let p = painter.with_clip_rect(area);
        let at = match wh { 0 => st + v * zi, 1 => st + g.pw - v * zi, 2 | 4 => st + v * zi, _ => st + g.ph - v * zi };
        if wh < 2 { p.line_segment([pos2(at, area.min.y), pos2(at, area.max.y)], Stroke::new(1.0, blue)); } else { p.line_segment([pos2(area.min.x, at), pos2(area.max.x, at)], Stroke::new(1.0, blue)); }
        p.text(pt + vec2(12.0, 12.0), Align2::LEFT_TOP, format!("{}: {}", tr(names[wh as usize]), unit_text(v, unit)), FontId::proportional(11.0), blue);
        if !ui.input(|i| i.pointer.any_down()) { ST.with(|x| x.borrow_mut().margin_drag = None); }
    }
    ST.with(|x| x.borrow_mut().markers = markers);
}

/// A margin in inches from where the pointer is on the ruler of the page starting at `start`.
fn margin_value(which: u8, start: f32, pt: Pos2, g: &Geo, w: f32, h: f32, m: &print::Margins) -> f32 {
    let zi = PX_PER_INCH * g.z;
    let v = match which { 0 => (pt.x - start) / zi, 1 => (start + g.pw - pt.x) / zi, 2 | 4 => (pt.y - start) / zi, _ => (start + g.ph - pt.y) / zi };
    let max = match which { 0 => w - m.right - 0.2, 1 => w - m.left - 0.2, 2 => h - m.bottom - 0.2, 3 => h - m.top - 0.2, _ => h / 2.0 };
    (v.clamp(0.0, max.max(0.0)) * 100.0).round() / 100.0
}

/// A length in inches in the ruler's unit.
pub fn unit_text(inches: f32, unit: u8) -> String {
    match unit { 1 => format!("{:.2} cm", inches * 2.54), 2 => format!("{:.1} mm", inches * 25.4), _ => format!("{:.2}\"", inches) }
}

/// One page's stretch of a ruler: margins shaded, ticks counted from the first margin.
#[allow(clippy::too_many_arguments)]
fn ruler_strip(p: &egui::Painter, across: bool, strip: Rect, start: f32, len: f32, m0: f32, m1: f32, unit: u8, zi: f32, ink: Color32) {
    let seg = |a: f32, b: f32| if across { Rect::from_min_max(pos2(a, strip.min.y + 3.0), pos2(b, strip.max.y - 3.0)) } else { Rect::from_min_max(pos2(strip.min.x + 3.0, a), pos2(strip.max.x - 3.0, b)) };
    let shade = Color32::from_gray(0xC8);
    p.rect_filled(seg(start, start + m0), 0.0, shade);
    p.rect_filled(seg(start + len - m1, start + len), 0.0, shade);
    p.rect_filled(seg(start + m0, start + len - m1), 0.0, Color32::WHITE);
    // A major tick per inch or centimeter, with minor ticks between.
    let (major, minors) = match unit { 0 => (zi, 8), _ => (zi / 2.54, if zi / 2.54 > 30.0 { 10 } else { 4 }) };
    let origin = start + m0;
    let font = FontId::proportional(8.5);
    let step = major / minors as f32;
    let first = -((m0 / step).floor() as i32);
    let last = ((len - m0) / step).ceil() as i32;
    for i in first..=last {
        let t = origin + i as f32 * step;
        if t < start || t > start + len { continue; }
        let n = i.rem_euclid(minors);
        let thick = if n == 0 { 0.0 } else if minors % 2 == 0 && n == minors / 2 { 0.35 } else { 0.2 };
        if n == 0 {
            if i == 0 { continue; }
            let label = (i / minors).abs() * if unit == 2 { 10 } else { 1 };
            let at = if across { pos2(t, strip.center().y) } else { pos2(strip.center().x, t) };
            p.text(at, Align2::CENTER_CENTER, label.to_string(), font.clone(), ink);
            continue;
        }
        let half = (if across { strip.height() } else { strip.width() }) * thick;
        if across { p.line_segment([pos2(t, strip.center().y - half), pos2(t, strip.center().y + half)], Stroke::new(1.0, ink)); }
        else { p.line_segment([pos2(strip.center().x - half, t), pos2(strip.center().x + half, t)], Stroke::new(1.0, ink)); }
    }
}

fn save_edit(app: &mut SheetApp, e: &HfEdit) {
    let si = app.sheet();
    let mut parts = split(&page_hf(&app.book.sheets[si].page.header_footer, e.page, e.footer));
    if parts[e.section] == e.text { return; }
    parts[e.section] = e.text.clone();
    let text = join(&parts);
    let (page, footer) = (e.page, e.footer);
    // A section without &G no longer has a picture.
    let (slot, keep) = (edit_slot(app, e), e.text.contains("&G"));
    crate::print::edit_setup(app, "Header & Footer", move |p| { set_page_hf(&mut p.header_footer, page, footer, text); if !keep { p.header_footer.pictures.retain(|x| x.slot != slot); } });
}

fn close_edit(app: &mut SheetApp) {
    ST.with(|s| s.borrow_mut().edit = None);
    if app.tab == Tab::HeaderFooter { app.tab = Tab::Insert; }
}

/// Puts a field code into the section being edited (or the center header).
pub fn insert_code(app: &mut SheetApp, code: &str) {
    if editing().is_none() { header_footer(app); }
    ST.with(|s| { if let Some(e) = s.borrow_mut().edit.as_mut() { e.text.push_str(code); e.fresh = true; } });
}

// ---- the Header & Footer tab ----

pub fn contextual(_app: &SheetApp) -> Vec<(Tab, &'static str)> {
    if mode() == ViewMode::PageLayout && editing().is_some() { vec![(Tab::HeaderFooter, "Header & Footer")] } else { vec![] }
}

pub fn hf_tab(app: &mut SheetApp, ui: &mut Ui) {
    use qalam_kit::widgets::{group, large, rows};
    if editing().is_none() { app.tab = Tab::Insert; return; }
    group(ui, "Header & Footer", |ui| {
        let r = qalam_kit::widgets::large_menu(ui, "header", "Header", "Choose a ready-made header");
        crate::theme::menu(&r, |ui| presets(app, ui, false));
        let r = qalam_kit::widgets::large_menu(ui, "footer", "Footer", "Choose a ready-made footer");
        crate::theme::menu(&r, |ui| presets(app, ui, true));
    });
    group(ui, "Header & Footer Elements", |ui| {
        for (icon, label, code, tip) in [("pageNumber", "Page Number", "&P", "Insert the page number"), ("numbers", "Number of Pages", "&N", "Insert the number of pages"), ("dateTime", "Current Date", "&D", "Insert the date when printed"), ("dateTime", "Current Time", "&T", "Insert the time when printed")] {
            if large(ui, icon, label, false, true, tip).clicked() { insert_code(app, code); }
        }
        rows(ui, |ui| {
            if qalam_kit::widgets::small(ui, "folderOpen", Some("File Path"), false, true, "Insert the file's folder and name").clicked() { insert_code(app, "&Z&F"); }
            if qalam_kit::widgets::small(ui, "filePlus", Some("File Name"), false, true, "Insert the file's name").clicked() { insert_code(app, "&F"); }
            if qalam_kit::widgets::small(ui, "newSheet", Some("Sheet Name"), false, true, "Insert the sheet's name").clicked() { insert_code(app, "&A"); }
        });
        if large(ui, "picture", "Picture", false, true, "Insert a picture into this section (&G)").clicked() { pick_hf_picture(app); }
        let has = current_picture(app).is_some();
        if large(ui, "crop", "Format Picture", false, has, "Size, scale and crop this section's picture").clicked() { open_format_picture(app); }
    });
    group(ui, "Font", |ui| {
        rows(ui, |ui| {
            crate::theme::Combo::from_id_salt("hf-font").width(120.0).height(400.0).selected_text(tr("Font")).show_ui(ui, |ui| {
                for name in word_engine::fonts::family_names() { if ui.selectable_label(false, egui::RichText::new(&name).size(13.0)).clicked() { hf_format(app, HfFormat::Font(name.clone())); } }
            });
            crate::theme::Combo::from_id_salt("hf-size").width(50.0).selected_text(tr("Size")).show_ui(ui, |ui| {
                for sz in [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0, 48.0, 72.0] { if ui.selectable_label(false, format!("{sz}")).clicked() { hf_format(app, HfFormat::Size(sz)); } }
            });
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if qalam_kit::widgets::small(ui, "bold", None, false, true, "Bold (⌘B)").clicked() { hf_format(app, HfFormat::Bold); }
            if qalam_kit::widgets::small(ui, "italic", None, false, true, "Italic (⌘I)").clicked() { hf_format(app, HfFormat::Italic); }
            if qalam_kit::widgets::small(ui, "underline", None, false, true, "Underline (⌘U)").clicked() { hf_format(app, HfFormat::Underline); }
            if qalam_kit::widgets::small(ui, "strike", None, false, true, "Strikethrough").clicked() { hf_format(app, HfFormat::Strike); }
            let r = qalam_kit::widgets::small(ui, "textColor", None, false, true, "Font Color");
            crate::theme::menu(&r, |ui| { let th = app.book.theme_colors.clone(); if let Some(c) = crate::themes_ui::color_menu(ui, "Automatic", &th) { hf_format(app, HfFormat::Color(c.map(|x| x.0))); ui.close(); } });
        });
    });
    group(ui, "Navigation", |ui| {
        let footer = editing().is_some_and(|e| e.footer);
        if large(ui, if footer { "goToHeader" } else { "goToFooter" }, if footer { "Go to Header" } else { "Go to Footer" }, false, true, "Switch between the header and the footer").clicked() {
            if let Some(e) = editing() { save_edit(app, &e); let si = app.sheet(); let text = split(&page_hf(&app.book.sheets[si].page.header_footer, e.page, !e.footer))[e.section].clone(); ST.with(|s| s.borrow_mut().edit = Some(HfEdit { footer: !e.footer, text, fresh: true, ..e })); }
        }
    });
    group(ui, "Options", |ui| {
        rows(ui, |ui| {
            let si = app.sheet();
            let h = app.book.sheets[si].page.header_footer.clone();
            let mut first = h.different_first;
            if crate::theme::check(ui, &mut first, tr("Different First Page")).changed() { if let Some(e) = editing() { save_edit(app, &e); } crate::print::edit_setup(app, "Header & Footer", |p| p.header_footer.different_first = first); refresh_edit(app); }
            let mut odd = h.different_odd_even;
            if crate::theme::check(ui, &mut odd, tr("Different Odd & Even Pages")).changed() { if let Some(e) = editing() { save_edit(app, &e); } crate::print::edit_setup(app, "Header & Footer", |p| p.header_footer.different_odd_even = odd); refresh_edit(app); }
            let mut sc = h.scale_with_doc;
            if crate::theme::check(ui, &mut sc, tr("Scale with Document")).changed() { crate::print::edit_setup(app, "Header & Footer", |p| p.header_footer.scale_with_doc = sc); }
            let mut al = h.align_with_margins;
            if crate::theme::check(ui, &mut al, tr("Align with Page Margins")).changed() { crate::print::edit_setup(app, "Header & Footer", |p| p.header_footer.align_with_margins = al); }
        });
    });
    group(ui, "Close", |ui| {
        if large(ui, "closeView", "Close Header and Footer", false, true, "Finish editing the header and footer").clicked() { if let Some(e) = editing() { save_edit(app, &e); } close_edit(app); }
    });
}

/// After the first-page or odd/even options change, the edit box shows that page's text.
fn refresh_edit(app: &mut SheetApp) {
    let si = app.sheet();
    let h = app.book.sheets[si].page.header_footer.clone();
    ST.with(|s| { if let Some(e) = s.borrow_mut().edit.as_mut() { e.text = split(&page_hf(&h, e.page, e.footer))[e.section].clone(); } });
}

fn presets(app: &mut SheetApp, ui: &mut Ui, footer: bool) {
    let sheet = app.book.sheets[app.sheet()].name.clone();
    for (label, code) in [("(none)", ""), ("Page 1", "&CPage &P"), ("Page 1 of ?", "&CPage &P of &N"), ("Sheet name", "&C&A"), ("File name", "&C&F"), ("Confidential, date, page", "&LConfidential&C&D&RPage &P")] {
        let shown = if label == "Sheet name" { sheet.clone() } else { tr(label) };
        if ui.button(shown).clicked() {
            let page = editing().map_or(0, |e| e.page);
            let text = code.to_string();
            crate::print::edit_setup(app, "Header & Footer", move |p| set_page_hf(&mut p.header_footer, page, footer, text));
            refresh_edit(app);
            ui.close();
        }
    }
}

// ---- header/footer text formatting and pictures ----

/// Formatting put into a header/footer section as Excel's codes.
#[derive(Clone, Debug, PartialEq)]
pub enum HfFormat { Bold, Italic, Underline, Strike, Font(String), Size(f32), Color(Option<[u8; 3]>) }

/// The codes that turn a format on, and back off.
fn codes(f: &HfFormat) -> (String, String) {
    match f {
        HfFormat::Bold => ("&B".into(), "&B".into()),
        HfFormat::Italic => ("&I".into(), "&I".into()),
        HfFormat::Underline => ("&U".into(), "&U".into()),
        HfFormat::Strike => ("&S".into(), "&S".into()),
        HfFormat::Font(n) => (format!("&\"{n},Regular\""), "&\"-,Regular\"".into()),
        HfFormat::Size(s) => (format!("&{}", s.round().max(1.0) as u32), "&11".into()),
        HfFormat::Color(c) => { let [r, g, b] = c.unwrap_or([0, 0, 0]); (format!("&K{r:02X}{g:02X}{b:02X}"), "&K000000".into()) }
    }
}

/// Puts a format's codes around the selected text of the section being edited, or at its start
/// (the whole section) when nothing is selected.
fn apply_codes(ctx: &egui::Context, e: &mut HfEdit, f: &HfFormat) {
    let sel = egui::TextEdit::load_state(ctx, egui::Id::new("hf-section-edit")).and_then(|s| s.cursor.char_range()).map(|r| { let (a, b) = (usize::from(r.primary.index), usize::from(r.secondary.index)); (a.min(b), a.max(b)) });
    let chars: Vec<char> = e.text.chars().collect();
    let (a, b) = match sel { Some((a, b)) if a < b && b <= chars.len() => (a, b), _ => (0, chars.len()) };
    let (mut open, mut close) = codes(f);
    // A size code mustn't run into digits that follow it.
    if matches!(f, HfFormat::Size(_)) { if chars.get(a).is_some_and(|c| c.is_ascii_digit()) { open.push(' '); } if chars.get(b).is_some_and(|c| c.is_ascii_digit()) { close.push(' '); } }
    let mut out: String = chars[..a].iter().collect();
    out.push_str(&open);
    out.extend(&chars[a..b]);
    if b < chars.len() { out.push_str(&close); }
    out.extend(&chars[b..]);
    e.text = out;
    e.fresh = true;
}

/// Formats the section being edited (the Header & Footer tab's font controls, the Home tab's
/// while a section is open); false when no section is being edited.
pub fn hf_format(app: &mut SheetApp, f: HfFormat) -> bool {
    let Some(mut e) = editing() else { return false };
    apply_codes(&app.ctx, &mut e, &f);
    ST.with(|s| s.borrow_mut().edit = Some(e));
    true
}

/// ⌘B, ⌘I and ⌘U in the section being edited.
fn format_keys(ui: &mut Ui, e: &mut HfEdit) {
    for (k, f) in [(egui::Key::B, HfFormat::Bold), (egui::Key::I, HfFormat::Italic), (egui::Key::U, HfFormat::Underline)] {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, k)) { apply_codes(ui.ctx(), e, &f); }
    }
}

/// A cell style change (the Home tab's font controls) while a header/footer section is being
/// edited formats the section instead; true when it was taken.
pub fn hf_style(app: &mut SheetApp, f: &dyn Fn(&mut sheet_engine::style::Style)) -> bool {
    if mode() != ViewMode::PageLayout || editing().is_none() { return false; }
    use sheet_engine::style::{Style, Underline};
    // What the change sets is what comes out the same from two different styles.
    let (mut a, mut b) = (Style::default(), Style { font: "\u{1}".into(), size: 10, bold: true, italic: true, underline: Underline::Double, strike: true, color: Some([1, 2, 3]), ..Style::default() });
    f(&mut a);
    f(&mut b);
    let fmt = if a.bold == b.bold { Some(HfFormat::Bold) } else if a.italic == b.italic { Some(HfFormat::Italic) } else if a.underline == b.underline { Some(HfFormat::Underline) } else if a.strike == b.strike { Some(HfFormat::Strike) }
        else if a.font == b.font { Some(HfFormat::Font(a.font.clone())) } else if a.size == b.size { Some(HfFormat::Size(a.size_pt())) } else if a.color == b.color { Some(HfFormat::Color(a.color)) } else { None };
    if let Some(x) = fmt { hf_format(app, x); }
    true
}

/// The picture slot of the section being edited.
fn edit_slot(app: &SheetApp, e: &HfEdit) -> String {
    let h = &app.book.sheets[app.sheet()].page.header_footer;
    sheet_engine::hfpic::slot(e.section, e.footer, sheet_engine::hfpic::page_kind(h, e.page as u32 + 1))
}

/// The picture of the section being edited.
pub fn current_picture(app: &SheetApp) -> Option<sheet_engine::hfpic::HfPicture> {
    let e = editing()?;
    let slot = edit_slot(app, &e);
    app.book.sheets[app.sheet()].page.header_footer.pictures.iter().find(|p| p.slot == slot).cloned()
}

/// Header & Footer ▸ Picture: the picture goes into the section being edited (replacing its
/// picture), shown where its &G stands.
pub fn insert_hf_picture(app: &mut SheetApp, data: Vec<u8>, ext: &str) {
    if editing().is_none() { header_footer(app); }
    let Some(mut e) = editing() else { return };
    if !e.text.contains("&G") { e.text.push_str("&G"); }
    let slot = edit_slot(app, &e);
    let pic = sheet_engine::hfpic::HfPicture::new(slot.clone(), data, ext);
    let si = app.sheet();
    let mut parts = split(&page_hf(&app.book.sheets[si].page.header_footer, e.page, e.footer));
    parts[e.section] = e.text.clone();
    let text = join(&parts);
    let (page, footer) = (e.page, e.footer);
    crate::print::edit_setup(app, "Insert Picture", move |p| {
        set_page_hf(&mut p.header_footer, page, footer, text);
        p.header_footer.pictures.retain(|x| x.slot != slot);
        p.header_footer.pictures.push(pic);
    });
    e.fresh = true;
    ST.with(|s| s.borrow_mut().edit = Some(e));
}

fn pick_hf_picture(app: &mut SheetApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() else { return };
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("png").to_ascii_lowercase();
    match std::fs::read(&path) {
        Ok(b) if sheet_engine::cellimage::image_size(&b).is_some() => insert_hf_picture(app, b, &ext),
        Ok(_) => app.notify(&tr("This file isn't a picture Jadwal can show.")),
        Err(e) => app.notify(&e.to_string()),
    }
}

/// Format Picture for a header/footer picture: size, scale and crop.
#[derive(Clone, Debug)]
struct PicDlg { pic: sheet_engine::hfpic::HfPicture, lock: bool }

pub fn open_format_picture(app: &mut SheetApp) {
    if let Some(pic) = current_picture(app) { ST.with(|s| s.borrow_mut().pic_dlg = Some(PicDlg { pic, lock: true })); }
}

fn picture_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = ST.with(|s| s.borrow().pic_dlg.clone()) else { return };
    let unit = app.prefs.ruler_units.min(2);
    // Points to the ruler's unit.
    let (k, uname) = match unit { 0 => (1.0 / 72.0, "in"), 1 => (2.54 / 72.0, "cm"), _ => (25.4 / 72.0, "mm") };
    let (nw, nh) = d.pic.natural();
    let full = |c: [f32; 4]| ((nw * (1.0 - c[0] - c[2])).max(1.0), (nh * (1.0 - c[1] - c[3])).max(1.0));
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new(tr("Format Picture")).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
        let (fw, fh) = full(d.pic.crop);
        crate::theme::section(ui, "Size and rotate");
        egui::Grid::new("hf-pic-size").num_columns(2).show(ui, |ui| {
            let mut h = d.pic.height * k;
            ui.label(tr("Height:"));
            if ui.add(egui::DragValue::new(&mut h).speed(0.01).range(0.01..=1000.0).suffix(format!(" {uname}"))).changed() { d.pic.height = h / k; if d.lock { d.pic.width = d.pic.height * fw / fh; } }
            ui.end_row();
            let mut w = d.pic.width * k;
            ui.label(tr("Width:"));
            if ui.add(egui::DragValue::new(&mut w).speed(0.01).range(0.01..=1000.0).suffix(format!(" {uname}"))).changed() { d.pic.width = w / k; if d.lock { d.pic.height = d.pic.width * fh / fw; } }
            ui.end_row();
        });
        crate::theme::section(ui, "Scale");
        egui::Grid::new("hf-pic-scale").num_columns(2).show(ui, |ui| {
            let mut sh = d.pic.height / fh * 100.0;
            ui.label(tr("Height:"));
            if ui.add(egui::DragValue::new(&mut sh).speed(1.0).range(1.0..=1000.0).suffix(" %")).changed() { d.pic.height = fh * sh / 100.0; if d.lock { d.pic.width = fw * sh / 100.0; } }
            ui.end_row();
            let mut sw = d.pic.width / fw * 100.0;
            ui.label(tr("Width:"));
            if ui.add(egui::DragValue::new(&mut sw).speed(1.0).range(1.0..=1000.0).suffix(" %")).changed() { d.pic.width = fw * sw / 100.0; if d.lock { d.pic.height = fh * sw / 100.0; } }
            ui.end_row();
        });
        crate::theme::check(ui, &mut d.lock, tr("Lock aspect ratio"));
        crate::theme::section(ui, "Crop");
        egui::Grid::new("hf-pic-crop").num_columns(4).show(ui, |ui| {
            let scale = (d.pic.width / fw, d.pic.height / fh);
            let mut crop = d.pic.crop;
            for (i, label) in ["Left:", "Top:", "Right:", "Bottom:"].iter().enumerate() {
                let side = if i % 2 == 0 { nw } else { nh };
                let mut v = crop[i] * side * k;
                ui.label(tr(label));
                if ui.add(egui::DragValue::new(&mut v).speed(0.01).range(0.0..=side * k * 0.9).suffix(format!(" {uname}"))).changed() { crop[i] = (v / k / side).clamp(0.0, 0.9); }
                if i % 2 == 1 { ui.end_row(); }
            }
            // The two sides of each direction leave some picture.
            if crop[0] + crop[2] > 0.95 { crop[2] = 0.95 - crop[0]; }
            if crop[1] + crop[3] > 0.95 { crop[3] = 0.95 - crop[1]; }
            if crop != d.pic.crop { d.pic.crop = crop; let (fw2, fh2) = full(crop); d.pic.width = fw2 * scale.0; d.pic.height = fh2 * scale.1; }
        });
        crate::theme::hrow(ui, |ui| {
            if ui.button(tr("Reset")).clicked() { d.pic.crop = [0.0; 4]; d.pic.width = nw; d.pic.height = nh; }
            if crate::theme::primary(ui, "OK").clicked() { ok = true; }
            if ui.button(tr("Cancel")).clicked() { cancel = true; }
        });
    });
    if ok {
        let pic = d.pic.clone();
        crate::print::edit_setup(app, "Format Picture", move |p| { if let Some(x) = p.header_footer.pictures.iter_mut().find(|x| x.slot == pic.slot) { *x = pic; } });
    }
    ST.with(|s| s.borrow_mut().pic_dlg = if ok || cancel || !open { None } else { Some(d) });
}

// ---- Page Break Preview ----

fn breaks(app: &SheetApp) -> (BTreeSet<u32>, BTreeSet<u32>, Vec<PrintPage>) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    let key = format!("{si}{:?}{:?}{:?}{:?}{}{}{}{}{}", s.page, app.book.print_area(si), app.book.print_titles(si), print::used_range(&app.book, si), s.col_widths.len(), s.row_heights.len(), s.hidden_rows.len(), s.hidden_cols.len(), s.default_row_height);
    let pages = print::sheet_pages(&app.book, si, None);
    if let Some((k, r, c)) = ST.with(|x| x.borrow().breaks.clone()) { if k == key { return (r, c, pages); } }
    let (r, c) = print::break_lines(&app.book, si);
    ST.with(|x| x.borrow_mut().breaks = Some((key, r.clone(), c.clone())));
    (r, c, pages)
}

/// Page Break Preview over the grid (called by the grid after the cells): gray outside the
/// pages, blue page outlines and breaks, page labels, and dragging breaks.
pub fn break_overlay(app: &mut SheetApp, ui: &mut Ui, painter: &egui::Painter, lay: &crate::grid::Layout) {
    if mode() != ViewMode::PageBreak { return; }
    let si = app.sheet();
    let (rows, cols, pages) = breaks(app);
    let view = lay.view();
    let p = painter.with_clip_rect(view);
    let rect_of = |pg: &PrintPage| lay.range_rect(Range { r0: pg.rows.0, c0: pg.cols.0, r1: pg.rows.1, c1: pg.cols.1 });
    // Gray everything that doesn't print.
    let shapes: Vec<Rect> = pages.iter().map(&rect_of).collect();
    let gray = Color32::from_black_alpha(if app.dark { 90 } else { 60 });
    let mut bands = vec![view];
    for r in &shapes { bands = bands.into_iter().flat_map(|b| subtract(b, *r)).collect(); }
    for b in bands { p.rect_filled(b, 0.0, gray); }
    let blue = Color32::from_rgb(0x22, 0x55, 0xCC);
    for (k, r) in shapes.iter().enumerate() {
        p.rect_stroke(*r, 0.0, Stroke::new(3.0, blue), StrokeKind::Inside);
        if r.width() > 60.0 && r.height() > 30.0 { p.text(r.center(), Align2::CENTER_CENTER, format!("{} {}", tr("Page"), pages[k].number), FontId::proportional((r.height() / 6.0).clamp(14.0, 48.0)), Color32::from_rgba_unmultiplied(0x80, 0x80, 0x80, 110)); }
    }
    let s = &app.book.sheets[si];
    let (manual_r, manual_c) = (s.page.row_breaks.clone(), s.page.col_breaks.clone());
    let span = shapes.iter().fold(Rect::NOTHING, |a, b| a.union(*b));
    let drag = ST.with(|x| x.borrow().drag);
    for &r in &rows {
        let y = lay.y(r);
        let a = pos2(span.min.x.max(view.min.x), y);
        let b = pos2(span.max.x.min(view.max.x), y);
        dashed(&p, a, b, blue, manual_r.contains(&r));
        let id = ui.interact(Rect::from_min_max(pos2(a.x, y - 3.0), pos2(b.x, y + 3.0)).intersect(view), egui::Id::new(("pb-row", r)), Sense::drag());
        if id.hovered() || id.dragged() { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical); }
        if id.drag_started() { ST.with(|x| x.borrow_mut().drag = Some((true, r))); }
        if id.drag_stopped() { if let Some(pt) = id.interact_pointer_pos() { move_break(app, true, r, lay.row_at(pt.y + lay.rows.size(lay.row_at(pt.y)) * lay.z / 2.0)); } }
    }
    for &c in &cols {
        let x = lay.x(c);
        let a = pos2(x, span.min.y.max(view.min.y));
        let b = pos2(x, span.max.y.min(view.max.y));
        dashed(&p, a, b, blue, manual_c.contains(&c));
        let id = ui.interact(Rect::from_min_max(pos2(x - 3.0, a.y), pos2(x + 3.0, b.y)).intersect(view), egui::Id::new(("pb-col", c)), Sense::drag());
        if id.hovered() || id.dragged() { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); }
        if id.drag_started() { ST.with(|x| x.borrow_mut().drag = Some((false, c))); }
        if id.drag_stopped() { if let Some(pt) = id.interact_pointer_pos() { move_break(app, false, c, lay.col_at(pt.x + lay.cols.size(lay.col_at(pt.x)) * lay.z / 2.0)); } }
    }
    // The line being dragged follows the pointer.
    if let (Some((is_row, _)), Some(pt)) = (drag, ui.input(|i| i.pointer.interact_pos())) {
        if is_row { p.line_segment([pos2(view.min.x, pt.y), pos2(view.max.x, pt.y)], Stroke::new(2.0, blue)); } else { p.line_segment([pos2(pt.x, view.min.y), pos2(pt.x, view.max.y)], Stroke::new(2.0, blue)); }
        if !ui.input(|i| i.pointer.any_down()) { ST.with(|x| x.borrow_mut().drag = None); }
    }
}

/// Moves a page break (rows: `from` is the first row of a page) to start at `to`.
pub fn move_break(app: &mut SheetApp, rows: bool, from: u32, to: u32) {
    ST.with(|x| x.borrow_mut().drag = None);
    if from == to || to == 0 { return; }
    crate::print::edit_setup(app, "Move Page Break", move |p| {
        let set = if rows { &mut p.row_breaks } else { &mut p.col_breaks };
        set.remove(&from);
        set.insert(to);
    });
    ST.with(|x| x.borrow_mut().breaks = None);
}

fn dashed(p: &egui::Painter, a: Pos2, b: Pos2, c: Color32, solid: bool) {
    if solid { p.line_segment([a, b], Stroke::new(2.5, c)); return; }
    let len = a.distance(b);
    if len <= 0.0 { return; }
    let dir = (b - a) / len;
    let mut t = 0.0;
    while t < len { p.line_segment([a + dir * t, a + dir * (t + 6.0).min(len)], Stroke::new(2.0, c)); t += 10.0; }
}

/// `a` minus `b`, as up to four rectangles.
fn subtract(a: Rect, b: Rect) -> Vec<Rect> {
    let b = a.intersect(b);
    if !b.is_positive() { return vec![a]; }
    let mut out = vec![];
    if b.min.y > a.min.y { out.push(Rect::from_min_max(a.min, pos2(a.max.x, b.min.y))); }
    if b.max.y < a.max.y { out.push(Rect::from_min_max(pos2(a.min.x, b.max.y), a.max)); }
    if b.min.x > a.min.x { out.push(Rect::from_min_max(pos2(a.min.x, b.min.y), pos2(b.min.x, b.max.y))); }
    if b.max.x < a.max.x { out.push(Rect::from_min_max(pos2(b.max.x, b.min.y), pos2(a.max.x, b.max.y))); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_split_join_and_evaluate() {
        let s = split("&LLeft &&co&CPage &P of &N&RSheet &A");
        assert_eq!(s, ["Left &&co".to_string(), "Page &P of &N".into(), "Sheet &A".into()]);
        assert_eq!(join(&s), "&LLeft &&co&CPage &P of &N&RSheet &A");
        assert_eq!(split("plain")[1], "plain");
        assert_eq!(evaluate("Page &P of &N, &A — &F", 2, 5, "Data", "Book.xlsx", "/tmp", 45292.5), "Page 2 of 5, Data — Book.xlsx");
        assert_eq!(evaluate("&\"Arial,Bold\"&14Big &KFF0000red", 1, 1, "", "", "", 0.0), "Big red");
        let mut h = print::HeaderFooter { different_first: true, ..Default::default() };
        set_page_hf(&mut h, 0, false, "first".into());
        set_page_hf(&mut h, 1, false, "rest".into());
        assert_eq!((h.first_header.as_str(), h.odd_header.as_str()), ("first", "rest"));
    }
}
