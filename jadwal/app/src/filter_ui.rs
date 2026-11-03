//! Tables and filters on screen (tabledialog.js, tableresize.js, the filter menu): the table
//! style drawn over cells, filter buttons in header cells with their popup (sort, search, value
//! checkboxes, Text/Number/Date Filters), the custom filter, Top 10, Create Table and Resize
//! Table dialogs, and the commands the ribbon's Home and Data tabs call.
//!
//! Dialog state lives in egui's temporary memory so the shared `Dialog` enum stays untouched.

use crate::app::{rgb, SheetApp};
use crate::grid::Layout;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Pos2, Rect, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use sheet_engine::addr::{parse_range, Pos, Range};
use sheet_engine::style::Style;
use sheet_engine::tables::{self, CmpOp, Cond, Criteria, Scope};
use std::borrow::Cow;
use std::collections::HashSet;

#[derive(Clone)]
struct Popup { scope: Scope, col: u32, at: Pos2, search: String, values: Vec<String>, blanks: bool, unchecked: HashSet<String>, blank_off: bool, sheet: usize }

/// The custom filter dialog: two conditions as (operator index into OPS, value).
#[derive(Clone)]
struct Custom { scope: Scope, col: u32, a: (usize, String), b: (usize, String), and: bool }

#[derive(Clone)]
struct Top10 { scope: Scope, col: u32, top: bool, n: f64, percent: bool }

#[derive(Clone, Default)]
struct State {
    popup: Option<Popup>,
    custom: Option<Custom>,
    top10: Option<Top10>,
    /// Create Table: the range text, "has headers", the style.
    create: Option<(String, bool, String)>,
    /// Resize Table: the table and the new range text.
    resize: Option<(usize, String)>,
}

fn state(ctx: &egui::Context) -> State { ctx.data(|d| d.get_temp::<State>(egui::Id::new("jadwal-tables")).unwrap_or_default()) }
fn set_state(ctx: &egui::Context, s: State) { ctx.data_mut(|d| d.insert_temp(egui::Id::new("jadwal-tables"), s)); }
fn edit_state(ctx: &egui::Context, f: impl FnOnce(&mut State)) { let mut s = state(ctx); f(&mut s); set_state(ctx, s); }

/// The custom filter's operators: label, comparison, and the wildcard pattern around the value.
const OPS: [(&str, CmpOp, &str); 12] = [
    ("equals", CmpOp::Eq, "{}"), ("does not equal", CmpOp::Ne, "{}"), ("is greater than", CmpOp::Gt, "{}"), ("is greater than or equal to", CmpOp::Ge, "{}"),
    ("is less than", CmpOp::Lt, "{}"), ("is less than or equal to", CmpOp::Le, "{}"), ("begins with", CmpOp::Eq, "{}*"), ("does not begin with", CmpOp::Ne, "{}*"),
    ("ends with", CmpOp::Eq, "*{}"), ("does not end with", CmpOp::Ne, "*{}"), ("contains", CmpOp::Eq, "*{}*"), ("does not contain", CmpOp::Ne, "*{}*"),
];

fn to_cond(op: usize, v: &str) -> Option<Cond> {
    let (_, cmp, pat) = OPS[op.min(OPS.len() - 1)];
    (!v.is_empty() || matches!(cmp, CmpOp::Eq | CmpOp::Ne)).then(|| Cond { op: cmp, val: pat.replace("{}", v) })
}

/// A saved condition back as an operator and value for the dialog.
fn from_cond(c: &Cond) -> (usize, String) {
    let v = &c.val;
    let (pat, inner) = if v.len() > 2 && v.starts_with('*') && v.ends_with('*') { ("*{}*", &v[1..v.len() - 1]) } else if v.len() > 1 && v.ends_with('*') { ("{}*", &v[..v.len() - 1]) } else if v.len() > 1 && v.starts_with('*') { ("*{}", &v[1..]) } else { ("{}", v.as_str()) };
    let i = OPS.iter().position(|(_, o, p)| *o == c.op && *p == pat).or_else(|| OPS.iter().position(|(_, o, _)| *o == c.op)).unwrap_or(0);
    (i, if OPS[i].2 == pat { inner.to_string() } else { v.clone() })
}

// ---- drawing ----

/// A cell's style with its table's look laid under it (the cell's own fill and color win).
pub fn cell_style<'a>(app: &SheetApp, si: usize, q: Pos, st: &'a Style) -> Cow<'a, Style> {
    if app.book.sheets[si].tables.is_empty() { return Cow::Borrowed(st); }
    let Some(l) = tables::look_at(&app.book, si, q) else { return Cow::Borrowed(st) };
    let mut s = st.clone();
    if s.fill.is_none() { s.fill = l.fill; }
    if s.color.is_none() { s.color = l.color; }
    s.bold |= l.bold;
    Cow::Owned(s)
}

fn button_rect(lay: &Layout, q: Pos) -> Rect {
    let rc = lay.cell_rect(q);
    let s = (rc.height() - 4.0).clamp(4.0, 16.0 * lay.z);
    let x = if lay.rtl { rc.min.x + 2.0 } else { rc.max.x - s - 2.0 };
    Rect::from_min_size(pos2(x, rc.center().y - s / 2.0), vec2(s, s))
}

/// Table borders and filter buttons over the cells of one pane.
pub fn draw(app: &SheetApp, p: &egui::Painter, lay: &Layout, rows: std::ops::RangeInclusive<u32>, cols: std::ops::RangeInclusive<u32>) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    if s.tables.is_empty() && s.autofilter.is_none() { return; }
    let (r0, r1, c0, c1) = (*rows.start(), *rows.end(), *cols.start(), *cols.end());
    if r0 > r1 || c0 > c1 { return; }
    let area = Range { r0, r1, c0, c1 };
    for t in &s.tables {
        let Some(vis) = intersect(t.range, area) else { continue };
        for q in vis.iter() {
            if lay.rows.size(q.row) <= 0.0 || lay.cols.size(q.col) <= 0.0 { continue; }
            let Some(l) = tables::look_at(&app.book, si, q) else { continue };
            let rc = lay.cell_rect(q);
            let line = |b: Option<([u8; 3], f32)>, a: Pos2, e: Pos2| if let Some((c, w)) = b { p.line_segment([a, e], Stroke::new(w * lay.z.max(0.5), rgb(c))); };
            line(l.top, rc.left_top(), rc.right_top());
            line(l.bottom, rc.left_bottom(), rc.right_bottom());
            line(l.left, rc.left_top(), rc.left_bottom());
            line(l.right, rc.right_top(), rc.right_bottom());
        }
    }
    // Filter buttons.
    let mut heads: Vec<(Range, Scope)> = s.tables.iter().enumerate().filter(|(_, t)| t.filter.is_some() && t.header_row).map(|(i, t)| (t.range, Scope::Table(i))).collect();
    if let Some(a) = s.autofilter { heads.push((a, Scope::Auto)); }
    for (r, scope) in heads {
        if r.r0 < r0 || r.r0 > r1 || lay.rows.size(r.r0) <= 0.0 { continue; }
        let filter = app.book.filter_of(si, scope);
        for c in r.c0.max(c0)..=r.c1.min(c1) {
            if lay.cols.size(c) <= 0.0 { continue; }
            let b = button_rect(lay, Pos::new(r.r0, c));
            let on = filter.and_then(|f| f.get(c - r.c0)).is_some();
            p.rect_filled(b, 2.0, if app.dark { Color32::from_gray(0x3A) } else { Color32::from_gray(0xF4) });
            p.rect_stroke(b, 2.0, Stroke::new(1.0, Color32::from_gray(0xA0)), egui::StrokeKind::Inside);
            let ink = if app.dark { Color32::from_gray(0xE0) } else { Color32::from_gray(0x40) };
            let m = b.center();
            let u = b.width() / 16.0;
            if on {
                // A funnel beside a small arrow.
                let f = [pos2(m.x - 5.5 * u, m.y - 4.0 * u), pos2(m.x + 1.5 * u, m.y - 4.0 * u), pos2(m.x - 0.5 * u, m.y), pos2(m.x - 0.5 * u, m.y + 4.0 * u), pos2(m.x - 2.5 * u, m.y + 3.0 * u), pos2(m.x - 2.5 * u, m.y)];
                p.add(egui::Shape::convex_polygon(f.to_vec(), crate::grid::SELECT, Stroke::NONE));
                p.add(egui::Shape::convex_polygon(vec![pos2(m.x + 2.5 * u, m.y + 1.0 * u), pos2(m.x + 6.0 * u, m.y + 1.0 * u), pos2(m.x + 4.25 * u, m.y + 3.5 * u)], ink, Stroke::NONE));
            } else {
                p.add(egui::Shape::convex_polygon(vec![pos2(m.x - 3.5 * u, m.y - 1.5 * u), pos2(m.x + 3.5 * u, m.y - 1.5 * u), pos2(m.x, m.y + 2.5 * u)], ink, Stroke::NONE));
            }
        }
    }
}

fn intersect(a: Range, b: Range) -> Option<Range> {
    let r = Range { r0: a.r0.max(b.r0), r1: a.r1.min(b.r1), c0: a.c0.max(b.c0), c1: a.c1.min(b.c1) };
    (r.r0 <= r.r1 && r.c0 <= r.c1).then_some(r)
}

/// A press on a filter button: true when `pt` is on one (the click opens its popup).
pub fn click(app: &mut SheetApp, lay: &Layout, pt: Pos2, clicked: bool) -> bool {
    if !lay.view().contains(pt) { return false; }
    let si = app.sheet();
    let q = lay.cell_at(pt);
    let Some((scope, _, col)) = app.book.filter_button_at(si, q) else { return false };
    let b = button_rect(lay, q);
    if !b.contains(pt) { return false; }
    if clicked { app.commit(None); open_popup(app, scope, col, pos2(if lay.rtl { b.min.x } else { b.max.x - 240.0 }.max(lay.rect.min.x), b.max.y + 2.0)); }
    true
}

fn open_popup(app: &mut SheetApp, scope: Scope, col: u32, at: Pos2) {
    let si = app.sheet();
    let (values, blanks) = app.book.filter_values(si, scope, col);
    let (unchecked, blank_off) = match app.book.filter_of(si, scope).and_then(|f| f.get(col)) {
        Some(Criteria::Values { values: keep, blanks: kb }) => { let k: HashSet<String> = keep.iter().map(|v| v.to_lowercase()).collect(); (values.iter().filter(|v| !k.contains(&v.to_lowercase())).cloned().collect(), !kb) }
        _ => (HashSet::new(), false),
    };
    let frame = app.ctx.cumulative_frame_nr();
    app.ctx.data_mut(|d| d.insert_temp(egui::Id::new("filter-popup-opened"), frame));
    edit_state(&app.ctx, |s| s.popup = Some(Popup { scope, col, at, search: String::new(), values, blanks, unchecked, blank_off, sheet: si }));
}

// ---- commands ----

/// Data ▸ Filter (and Home ▸ Sort & Filter ▸ Filter): AutoFilter on or off, or a table's buttons.
pub fn toggle_filter(app: &mut SheetApp) {
    app.commit(None);
    app.checkpoint("Filter");
    let s = app.sheet();
    let (r, p) = (app.sel.range(), app.sel.active);
    app.book.toggle_filter(s, r, p);
    app.changed();
}

/// Data ▸ Clear: shows every row the sheet's filters hid.
pub fn clear_filters(app: &mut SheetApp) {
    app.checkpoint("Clear Filter");
    let s = app.sheet();
    app.book.clear_filters(s);
    app.changed();
}

/// Data ▸ Reapply: runs the filters again over the current values.
pub fn reapply(app: &mut SheetApp) {
    app.checkpoint("Reapply");
    let s = app.sheet();
    app.book.apply_filters(s);
    app.changed();
}

pub fn filter_on(app: &SheetApp) -> bool {
    let s = app.sheet();
    match app.book.table_at(s, app.sel.active) { Some(i) => app.book.sheets[s].tables[i].filter.is_some(), None => app.book.sheets[s].autofilter.is_some() }
}

/// Opens Create Table over the selection (or its current region) with a style.
pub fn open_create(app: &mut SheetApp, style: &str) {
    let s = app.sheet();
    let r = app.sel.range();
    let r = if r.is_cell() { app.book.current_region(s, r.start()) } else { r };
    let header = app.book.looks_like_header(s, r);
    edit_state(&app.ctx, |st| st.create = Some((format!("${}${}:${}${}", sheet_engine::addr::col_name(r.c0), r.r0 + 1, sheet_engine::addr::col_name(r.c1), r.r1 + 1), header, style.to_string())));
}

pub fn open_resize(app: &mut SheetApp, i: usize) {
    let r = app.book.sheets[app.sheet()].tables[i].range;
    edit_state(&app.ctx, |st| st.resize = Some((i, r.a1())));
}

/// A style gallery swatch; true when clicked.
pub fn style_swatch(app: &SheetApp, ui: &mut Ui, name: &str, selected: bool) -> bool {
    let (rect, r) = ui.allocate_exact_size(vec2(36.0, 28.0), egui::Sense::click());
    let (head, band, line) = tables::style_preview(&app.book, name).unwrap_or(([255; 3], [255; 3], [0x80; 3]));
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::WHITE);
    let rh = rect.height() / 5.0;
    for k in 0..5 {
        let row = Rect::from_min_size(pos2(rect.min.x, rect.min.y + k as f32 * rh), vec2(rect.width(), rh));
        p.rect_filled(row, 0.0, rgb(if k == 0 { head } else if k % 2 == 1 { band } else { [255; 3] }));
        p.line_segment([row.left_bottom(), row.right_bottom()], Stroke::new(0.6, rgb(line)));
    }
    p.rect_stroke(rect, 0.0, Stroke::new(if selected || r.hovered() { 2.0 } else { 1.0 }, if selected || r.hovered() { crate::grid::SELECT } else { c_line() }), egui::StrokeKind::Outside);
    r.on_hover_text(style_label(name)).clicked()
}

/// TableStyleMedium2 → "Medium 2".
pub fn style_label(name: &str) -> String {
    let n = name.trim_start_matches("TableStyle");
    let k = n.find(|c: char| c.is_ascii_digit()).unwrap_or(n.len());
    format!("{} {}", tr(&n[..k]), &n[k..])
}

/// The gallery of every built-in style; returns the one clicked.
pub fn style_gallery(app: &SheetApp, ui: &mut Ui, current: &str) -> Option<String> {
    let mut picked = None;
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        ui.set_width(7.0 * 40.0);
        for fam in ["Light", "Medium", "Dark"] {
            menu_heading(ui, fam);
            let names: Vec<String> = tables::style_names().into_iter().filter(|n| n.starts_with(&format!("TableStyle{fam}"))).collect();
            for chunk in names.chunks(7) {
                ui.horizontal(|ui| { ui.spacing_mut().item_spacing.x = 4.0; for n in chunk { if style_swatch(app, ui, n, n == current) { picked = Some(n.clone()); } } });
            }
        }
    });
    picked
}

/// Home ▸ Format as Table: the style gallery, then Create Table.
pub fn format_as_table_menu(app: &mut SheetApp, ui: &mut Ui) {
    let s = app.sheet();
    let current = app.book.table_at(s, app.sel.active).map(|i| app.book.sheets[s].tables[i].style.clone()).unwrap_or_default();
    if let Some(n) = style_gallery(app, ui, &current) {
        ui.close();
        match app.book.table_at(s, app.sel.active) {
            Some(i) => { app.checkpoint("Table Style"); app.book.sheets[s].tables[i].style = n; app.changed(); }
            None => open_create(app, &n),
        }
    }
}

// ---- dialogs and the popup ----

pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let mut st = state(ctx);
    if st.popup.is_some() { popup(app, ctx, &mut st); }
    if st.custom.is_some() { custom(app, ctx, &mut st); }
    if st.top10.is_some() { top10(app, ctx, &mut st); }
    if st.create.is_some() { create(app, ctx, &mut st); }
    if st.resize.is_some() { resize(app, ctx, &mut st); }
    set_state(ctx, st);
}

fn set_filter(app: &mut SheetApp, scope: Scope, col: u32, crit: Option<Criteria>) {
    app.checkpoint("Filter");
    let s = app.sheet();
    app.book.set_column_filter(s, scope, col, crit);
    app.changed();
}

fn popup(app: &mut SheetApp, ctx: &egui::Context, st: &mut State) {
    let Some(mut pp) = st.popup.take() else { return };
    if pp.sheet != app.sheet() || app.book.filter_of(pp.sheet, pp.scope).is_none() { return; }
    let si = pp.sheet;
    let mut keep = true;
    let col_kind = {
        let d = app.book.filter_data(si, pp.scope).unwrap_or(Range::cell(Pos::default()));
        let p = Pos::new(d.r0, d.c0 + pp.col);
        let nums = pp.values.iter().filter(|v| v.trim().parse::<f64>().is_ok()).count();
        if sheet_engine::numfmt::is_date_format(&app.book.style_at(si, p).num_fmt) { 2 } else if nums * 2 > pp.values.len() && nums > 0 { 1 } else { 0 }
    };
    let filtered = app.book.filter_of(si, pp.scope).and_then(|f| f.get(pp.col)).cloned();
    let header = app.book.filter_data(si, pp.scope).map(|d| app.book.display(si, Pos::new(d.r0.saturating_sub(1), d.c0 + pp.col))).unwrap_or_default();
    let area = egui::Area::new(egui::Id::new("filter-popup")).order(egui::Order::Foreground).fixed_pos(pp.at).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(240.0);
            if menu_item(ui, "sort", &tr(if col_kind == 1 { "Sort Smallest to Largest" } else { "Sort A to Z" })) { app.checkpoint("Sort"); app.book.sort_filter_range(si, pp.scope, pp.col, true); app.changed(); keep = false; }
            if menu_item(ui, "sort", &tr(if col_kind == 1 { "Sort Largest to Smallest" } else { "Sort Z to A" })) { app.checkpoint("Sort"); app.book.sort_filter_range(si, pp.scope, pp.col, false); app.changed(); keep = false; }
            ui.separator();
            let clear_label = crate::i18n::trf("Clear Filter From “{}”", &[&header]);
            if small(ui, "clearFilter", Some(&clear_label), false, filtered.is_some(), &clear_label).clicked() { set_filter(app, pp.scope, pp.col, None); keep = false; }
            let sub = tr(["Text Filters", "Number Filters", "Date Filters"][col_kind]);
            let mut custom_op = None;
            crate::theme::submenu(ui, sub, |ui| {
                let mut open_custom = |op: usize, ui: &mut Ui| { custom_op = Some(op); ui.close(); };
                match col_kind {
                    0 => for (label, op) in [("Equals…", 0), ("Does Not Equal…", 1), ("Begins With…", 6), ("Ends With…", 8), ("Contains…", 10), ("Does Not Contain…", 11)] { if ui.button(tr(label)).clicked() { open_custom(op, ui); } },
                    1 => {
                        for (label, op) in [("Equals…", 0), ("Does Not Equal…", 1), ("Greater Than…", 2), ("Greater Than Or Equal To…", 3), ("Less Than…", 4), ("Less Than Or Equal To…", 5)] { if ui.button(tr(label)).clicked() { open_custom(op, ui); } }
                        if ui.button(tr("Between…")).clicked() { st.custom = Some(Custom { scope: pp.scope, col: pp.col, a: (3, String::new()), b: (5, String::new()), and: true }); keep = false; ui.close(); }
                        if ui.button(tr("Top 10…")).clicked() { st.top10 = Some(Top10 { scope: pp.scope, col: pp.col, top: true, n: 10.0, percent: false }); keep = false; ui.close(); }
                        for (label, k) in [("Above Average", "aboveAverage"), ("Below Average", "belowAverage")] { if ui.button(tr(label)).clicked() { set_filter(app, pp.scope, pp.col, Some(Criteria::Dynamic(k.into()))); keep = false; ui.close(); } }
                    }
                    _ => {
                        for (label, k) in [("Today", "today"), ("Yesterday", "yesterday"), ("Tomorrow", "tomorrow"), ("This Week", "thisWeek"), ("Last Week", "lastWeek"), ("Next Week", "nextWeek"), ("This Month", "thisMonth"), ("Last Month", "lastMonth"), ("Next Month", "nextMonth"), ("This Quarter", "thisQuarter"), ("Last Quarter", "lastQuarter"), ("This Year", "thisYear"), ("Last Year", "lastYear"), ("Year to Date", "yearToDate")] {
                            if ui.button(tr(label)).clicked() { set_filter(app, pp.scope, pp.col, Some(Criteria::Dynamic(k.into()))); keep = false; ui.close(); }
                        }
                        for (label, op) in [("Before…", 4), ("After…", 2)] { if ui.button(tr(label)).clicked() { open_custom(op, ui); } }
                    }
                }
                ui.separator();
                if ui.button(tr("Custom Filter…")).clicked() {
                    let (a, b, and) = match &filtered { Some(Criteria::Custom { a, b, and }) => (from_cond(a), b.as_ref().map(from_cond).unwrap_or((0, String::new())), *and), _ => ((0, String::new()), (0, String::new()), true) };
                    st.custom = Some(Custom { scope: pp.scope, col: pp.col, a, b, and }); keep = false; ui.close();
                }
            });
            if let Some(op) = custom_op { st.custom = Some(Custom { scope: pp.scope, col: pp.col, a: (op, String::new()), b: (0, String::new()), and: true }); keep = false; }
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut pp.search).hint_text(tr("Search")).desired_width(f32::INFINITY));
            let q = pp.search.to_lowercase();
            let shown: Vec<String> = pp.values.iter().filter(|v| q.is_empty() || v.to_lowercase().contains(&q)).cloned().collect();
            egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                    let all_on = shown.iter().all(|v| !pp.unchecked.contains(v)) && (!pp.blanks || !pp.blank_off || !q.is_empty());
                    let mut all = all_on;
                    if crate::theme::check(ui, &mut all, tr(if q.is_empty() { "(Select All)" } else { "(Select All Search Results)" })).changed() {
                        for v in &shown { if all { pp.unchecked.remove(v); } else { pp.unchecked.insert(v.clone()); } }
                        if q.is_empty() { pp.blank_off = !all; }
                    }
                    for v in shown.iter().take(1000) {
                        let mut on = !pp.unchecked.contains(v);
                        if crate::theme::check(ui, &mut on, v.as_str()).changed() { if on { pp.unchecked.remove(v); } else { pp.unchecked.insert(v.clone()); } }
                    }
                    if pp.blanks && q.is_empty() { let mut on = !pp.blank_off; if crate::theme::check(ui, &mut on, tr("(Blanks)")).changed() { pp.blank_off = !on; } }
                });
            });
            crate::theme::footer(ui, |ui| {
                let any = pp.values.iter().any(|v| !pp.unchecked.contains(v) && (q.is_empty() || v.to_lowercase().contains(&q))) || (pp.blanks && !pp.blank_off && q.is_empty());
                if crate::theme::primary_enabled(ui, any, "OK").clicked() {
                    let keep_vals: Vec<String> = pp.values.iter().filter(|v| !pp.unchecked.contains(*v) && (q.is_empty() || v.to_lowercase().contains(&q))).cloned().collect();
                    let blanks = pp.blanks && !pp.blank_off && q.is_empty();
                    let everything = q.is_empty() && keep_vals.len() == pp.values.len() && (blanks || !pp.blanks);
                    set_filter(app, pp.scope, pp.col, if everything { None } else { Some(Criteria::Values { values: keep_vals, blanks }) });
                    keep = false;
                }
                if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
            });
        });
    });
    // The click that opened the popup lands outside it; only later clicks close it.
    let opened: u64 = ctx.data(|d| d.get_temp(egui::Id::new("filter-popup-opened"))).unwrap_or(0);
    if area.response.clicked_elsewhere() && !ctx.any_popup_open() && ctx.cumulative_frame_nr() > opened + 1 { keep = false; }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { keep = false; }
    if keep { st.popup = Some(pp); }
}

fn custom(app: &mut SheetApp, ctx: &egui::Context, st: &mut State) {
    let Some(mut c) = st.custom.take() else { return };
    let (mut keep, mut open) = (true, true);
    crate::theme::dialog(ctx, "Custom AutoFilter").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(420.0);
        ui.label(tr("Show rows where:"));
        let row = |ui: &mut Ui, id: &str, v: &mut (usize, String)| {
            crate::theme::hrow(ui, |ui| {
                crate::theme::Combo::from_id_salt(("cf-op", id)).width(190.0).selected_text(tr(OPS[v.0].0)).show_ui(ui, |ui| for (i, (l, _, _)) in OPS.iter().enumerate() { ui.selectable_value(&mut v.0, i, tr(l)); });
                ui.add(egui::TextEdit::singleline(&mut v.1).desired_width(180.0));
            });
        };
        row(ui, "a", &mut c.a);
        crate::theme::hrow(ui, |ui| { crate::theme::radio_value(ui, &mut c.and, true, tr("And")); crate::theme::radio_value(ui, &mut c.and, false, tr("Or")); });
        row(ui, "b", &mut c.b);
        ui.label(egui::RichText::new(tr("Use ? to represent any single character. Use * to represent any series of characters.")).size(11.0).color(c_ink2()));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let a = to_cond(c.a.0, c.a.1.trim());
                let b = if c.b.1.trim().is_empty() { None } else { to_cond(c.b.0, c.b.1.trim()) };
                let crit = match (a, b) { (Some(a), b) => Some(Criteria::Custom { a, b, and: c.and }), (None, Some(b)) => Some(Criteria::Custom { a: b, b: None, and: c.and }), _ => None };
                set_filter(app, c.scope, c.col, crit);
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    if keep && open { st.custom = Some(c); }
}

fn top10(app: &mut SheetApp, ctx: &egui::Context, st: &mut State) {
    let Some(mut t) = st.top10.take() else { return };
    let (mut keep, mut open) = (true, true);
    crate::theme::dialog(ctx, "Top 10 AutoFilter").open(&mut open).resizable(false).show(ctx, |ui| {
        crate::theme::hrow(ui, |ui| {
            crate::theme::Combo::from_id_salt("t10-top").width(90.0).selected_text(tr(if t.top { "Top" } else { "Bottom" })).show_ui(ui, |ui| { ui.selectable_value(&mut t.top, true, tr("Top")); ui.selectable_value(&mut t.top, false, tr("Bottom")); });
            ui.add(egui::DragValue::new(&mut t.n).range(1.0..=500.0).speed(1.0));
            crate::theme::Combo::from_id_salt("t10-pct").width(90.0).selected_text(tr(if t.percent { "Percent" } else { "Items" })).show_ui(ui, |ui| { ui.selectable_value(&mut t.percent, false, tr("Items")); ui.selectable_value(&mut t.percent, true, tr("Percent")); });
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { set_filter(app, t.scope, t.col, Some(Criteria::Top10 { top: t.top, percent: t.percent, n: t.n })); keep = false; }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    if keep && open { st.top10 = Some(t); }
}

fn create(app: &mut SheetApp, ctx: &egui::Context, st: &mut State) {
    let Some((mut text, mut header, style)) = st.create.take() else { return };
    let (mut keep, mut open) = (true, true);
    crate::theme::dialog(ctx, "Create Table").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(300.0);
        ui.label(tr("Where is the data for your table?"));
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
        crate::theme::check(ui, &mut header, tr("My table has headers"));
        let range = parse_range(&text.replace('$', ""));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, range.is_some(), "OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && range.is_some()) {
                let s = app.sheet();
                app.checkpoint("Create Table");
                match app.book.create_table(s, range.unwrap(), header, &style) {
                    Ok(i) => { let t = app.book.sheets[s].tables[i].range; app.sel = crate::app::Selection { active: t.start(), anchor: t.end(), extra: vec![] }; app.changed(); app.tab = crate::app::Tab::TableDesign; keep = false; }
                    Err(e) => { app.undo(); app.error = Some(tr(&e)); }
                }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    if keep && open { st.create = Some((text, header, style)); }
}

fn resize(app: &mut SheetApp, ctx: &egui::Context, st: &mut State) {
    let Some((i, mut text)) = st.resize.take() else { return };
    let (mut keep, mut open) = (true, true);
    let s = app.sheet();
    if i >= app.book.sheets[s].tables.len() { return; }
    crate::theme::dialog(ctx, "Resize Table").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(300.0);
        ui.label(tr("Select the new data range for your table:"));
        ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
        ui.label(egui::RichText::new(tr("The headers must remain in the same row, and the resulting table range must overlap the original table.")).size(11.0).color(c_ink2()));
        let range = parse_range(&text.replace('$', ""));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, range.is_some(), "OK").clicked() {
                app.checkpoint("Resize Table");
                match app.book.resize_table(s, i, range.unwrap()) { Ok(()) => { app.changed(); keep = false; } Err(e) => { app.undo(); app.error = Some(tr(&e)); } }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    if keep && open { st.resize = Some((i, text)); }
}

/// Is the filter drop-down open (for tests)?
#[cfg(test)]
pub fn popup_open(ctx: &egui::Context) -> bool { state(ctx).popup.is_some() }
