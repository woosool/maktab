//! Rulers (ui/canvas-rulers.js): margins, indent markers and tab stops for the caret's paragraph,
//! all draggable, plus a vertical ruler with the page's top and bottom margins.

use crate::app::WordApp;
use crate::ribbon::{c_card, c_frame, c_ink2, c_ink3, c_line};
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui};
use word_engine::{Leader, PageSettings, ParaProps, TabKind, TabStop};

pub const H: f32 = 24.0;
pub const W: f32 = 18.0;
const SNAP: f32 = 90.0; // 1/16 inch in twips

/// Which ruler control a drag moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RulerDrag { FirstLine, Hanging, LeftBox, Right, Tab(i32), NewTab, MarginLeft, MarginRight, MarginTop, MarginBottom, Column(usize, usize) }

fn snap(twips: f32, free: bool) -> i32 { if free { twips.round() as i32 } else { ((twips / SNAP).round() * SNAP) as i32 } }

fn ticks(painter: &egui::Painter, horizontal: bool, origin: f32, cross: f32, zero: f32, from: f32, to: f32, zoom: f32) {
    // Numbered every major unit from the margin (inches by default: every 1/8 inch, 12 px).
    let (major, sub, mult) = crate::view::UNITS.with(|u| u.get()).ruler();
    let step = major / sub as f32 * zoom;
    let first = ((from - zero) / step).floor() as i32;
    let last = ((to - zero) / step).ceil() as i32;
    for k in first..=last {
        let p = zero + k as f32 * step;
        if p < from || p > to { continue; }
        let r = k.rem_euclid(sub);
        let (len, label) = if r == 0 && k != 0 { (0.0, Some((k / sub).abs() * mult)) } else if sub % 2 == 0 && r == sub / 2 { (6.0, None) } else { (3.0, None) };
        let mid = origin + cross / 2.0;
        if let Some(n) = label {
            let at = if horizontal { pos2(p, mid) } else { pos2(mid, p) };
            painter.text(at, Align2::CENTER_CENTER, n.to_string(), FontId::proportional(10.0), c_ink3());
        } else {
            let (a, b) = if horizontal { (pos2(p, mid - len / 2.0), pos2(p, mid + len / 2.0)) } else { (pos2(mid - len / 2.0, p), pos2(mid + len / 2.0, p)) };
            painter.line_segment([a, b], Stroke::new(1.0, c_ink3()));
        }
    }
}

fn triangle(painter: &egui::Painter, tip: Pos2, down: bool) {
    let d = if down { -1.0 } else { 1.0 };
    let pts = vec![tip, tip + vec2(-5.0, 6.0 * d), tip + vec2(5.0, 6.0 * d)];
    painter.add(Shape::convex_polygon(pts, c_card(), Stroke::new(1.0, c_ink2())));
}

/// The caret paragraph's text box in page px: the page's margins, or its table cell's content box.
fn text_box(app: &WordApp) -> (f32, f32) {
    let g = app.page_geometry();
    if let Some(cell) = app.doc.current_cell() {
        let page = app.layout.page_of(app.doc.selection.focus);
        if let Some(b) = app.layout.cells.iter().find(|c| c.cell == cell && c.page == page).or_else(|| app.layout.cells.iter().find(|c| c.cell == cell)) {
            let t = &app.doc.tables[b.table];
            let c = t.cells().find(|c| c.id == cell);
            let m = |i: usize| c.and_then(|c| c.margins).map(|m| m[i]).filter(|v| *v >= 0).unwrap_or(t.cell_margins[i]) as f32 / 15.0;
            return (b.x0 + m(3), b.x1 - m(1));
        }
    }
    (g.left, g.width - g.right)
}

/// Physical marker positions in page px: (first line, hanging/left, right, start side is right).
fn markers(app: &WordApp) -> (f32, f32, f32, bool) {
    let (bl, br) = text_box(app);
    let f = app.doc.current_para_format();
    let rtl = app.doc.paragraph_rtl(app.doc.current_paragraph());
    let (il, ir, fl) = (f.indent_left as f32 / 15.0, f.indent_right as f32 / 15.0, f.first_line as f32 / 15.0);
    if rtl {
        let start = br - il;
        (start - fl, start, bl + ir, true)
    } else {
        let left = bl + il;
        (left + fl, left, br - ir, false)
    }
}

pub fn horizontal(app: &mut WordApp, ui: &mut Ui) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), H), Sense::click_and_drag());
    app.ruler_rect = rect;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, c_frame());
    let (ox, _, _, zoom) = app.canvas_map.clone();
    if zoom <= 0.0 || app.layout.pages.is_empty() { return; }
    let g = app.page_geometry();
    let page_x0 = ox + crate::canvas::GAP * zoom;
    let sx = |px: f32| page_x0 + px * zoom;
    let band = Rect::from_min_max(pos2(sx(0.0), rect.min.y + 4.0), pos2(sx(g.width), rect.max.y - 4.0));
    painter.rect_filled(band, 1.0, if crate::view::DARK.with(|d| d.get()) { Color32::from_gray(0x3A) } else { Color32::from_gray(222) });
    painter.rect_filled(Rect::from_min_max(pos2(sx(g.left), band.min.y), pos2(sx(g.width - g.right), band.max.y)), 0.0, c_card());
    painter.rect_stroke(band, 1.0, Stroke::new(1.0, c_line()), egui::StrokeKind::Inside);
    let (bl, br) = text_box(app);
    let in_cell = app.doc.current_cell().is_some();
    ticks(&painter, true, band.min.y, band.height(), sx(bl), band.min.x, band.max.x, zoom);
    // In a table, the table's column edges show as grey markers on the ruler.
    let columns: Vec<(usize, usize, f32)> = app.doc.current_cell().and_then(|c| app.doc.table_of_cell(c)).map(|ti| {
        let page = app.layout.page_of(app.doc.selection.focus);
        let mut v: Vec<(usize, usize, f32)> = app.layout.cells.iter().filter(|c| c.table == ti && c.page == page).map(|c| (ti, c.col + c.span - 1, c.x1)).collect();
        v.sort_by(|a, b| a.2.total_cmp(&b.2));
        v.dedup_by(|a, b| (a.2 - b.2).abs() < 0.5);
        v
    }).unwrap_or_default();
    for &(_, _, x) in &columns {
        painter.rect_filled(Rect::from_center_size(pos2(sx(x), band.center().y), vec2(6.0, band.height() - 2.0)), 1.0, Color32::from_gray(190));
    }

    let (first, left, right, rtl) = markers(app);
    let f = app.doc.current_para_format();
    // Tab stops are measured from the start of the text box.
    let tab_x = |pos: i32| if rtl { br - pos as f32 / 15.0 } else { bl + pos as f32 / 15.0 };
    for t in f.tabs.iter().filter(|t| t.kind != TabKind::Clear) {
        let x = sx(tab_x(t.pos));
        let y = band.max.y - 3.0;
        let s = Stroke::new(1.5, c_ink2());
        painter.line_segment([pos2(x, y - 6.0), pos2(x, y)], s);
        match t.kind {
            TabKind::Left => { painter.line_segment([pos2(x, y), pos2(x + 5.0, y)], s); }
            TabKind::Right => { painter.line_segment([pos2(x - 5.0, y), pos2(x, y)], s); }
            _ => { painter.line_segment([pos2(x - 4.0, y), pos2(x + 4.0, y)], s); }
        }
    }
    triangle(&painter, pos2(sx(first), band.min.y + 6.0), true);
    triangle(&painter, pos2(sx(left), band.max.y - 6.0), false);
    painter.rect(Rect::from_center_size(pos2(sx(left), band.max.y - 2.5), vec2(10.0, 5.0)), 0.0, c_card(), Stroke::new(1.0, c_ink2()), egui::StrokeKind::Inside);
    triangle(&painter, pos2(sx(right), band.max.y - 6.0), false);

    // Interaction.
    let id = ui.make_persistent_id("ruler-drag");
    let free = ui.input(|i| i.modifiers.alt);
    let to_twips = |screen_x: f32| (screen_x - page_x0) / zoom * 15.0;
    if response.drag_started() || response.clicked() {
        // Hit-test where the press began (a drag starts only after the pointer has moved).
        if let Some(p) = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos()) {
            let near = |x: f32| (p.x - sx(x)).abs() < 6.0;
            let top = p.y < band.center().y;
            let hit = if near(first) && top { Some(RulerDrag::FirstLine) }
                else if near(left) && p.y > band.max.y - 4.0 { Some(RulerDrag::LeftBox) }
                else if near(left) && !top { Some(RulerDrag::Hanging) }
                else if near(right) && !top { Some(RulerDrag::Right) }
                else if let Some(t) = f.tabs.iter().find(|t| t.kind != TabKind::Clear && near(tab_x(t.pos))) { Some(RulerDrag::Tab(t.pos)) }
                else if let Some(&(ti, col, _)) = columns.iter().find(|c| (p.x - sx(c.2)).abs() < 4.0) { Some(RulerDrag::Column(ti, col)) }
                else if !in_cell && (p.x - sx(g.left)).abs() < 4.0 { Some(RulerDrag::MarginLeft) }
                else if !in_cell && (p.x - sx(g.width - g.right)).abs() < 4.0 { Some(RulerDrag::MarginRight) }
                else if p.x > sx(bl) && p.x < sx(br) { Some(RulerDrag::NewTab) }
                else { None };
            ui.data_mut(|d| d.insert_temp(id, hit));
        }
    }
    let drag: Option<RulerDrag> = ui.data(|d| d.get_temp(id)).flatten();
    if let (Some(kind), Some(p)) = (drag, ui.input(|i| i.pointer.latest_pos())) {
        if response.dragged() || response.drag_started() {
            // Guide line down the page while dragging.
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            let x = p.x;
            ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, id)).line_segment([pos2(x, rect.max.y), pos2(x, rect.max.y + 3000.0)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(40, 110, 220, 160)));
        }
        if response.drag_stopped() || response.clicked() {
            let tw = to_twips(p.x);
            let start = bl * 15.0;
            let end = br * 15.0;
            let page_w = g.width * 15.0;
            // Positions measured from the start margin (mirrored in RTL paragraphs).
            let from_start = |t: f32| if rtl { end - t } else { t - start };
            let from_end = |t: f32| if rtl { t - start } else { end - t };
            let fmt = app.doc.current_para_format();
            match kind {
                RulerDrag::FirstLine => { let v = snap(from_start(tw), free) - fmt.indent_left; app.doc.set_para_props(ParaProps { first_line: Some(v), ..Default::default() }); }
                RulerDrag::Hanging => {
                    // The hanging marker moves the left indent; the first line stays put.
                    let first_pos = fmt.indent_left + fmt.first_line;
                    let li = snap(from_start(tw), free);
                    app.doc.set_para_props(ParaProps { indent_left: Some(li), first_line: Some(first_pos - li), ..Default::default() });
                }
                RulerDrag::LeftBox => { let li = snap(from_start(tw), free); app.doc.set_para_props(ParaProps { indent_left: Some(li), ..Default::default() }); }
                RulerDrag::Right => { let ri = snap(from_end(tw), free); app.doc.set_para_props(ParaProps { indent_right: Some(ri), ..Default::default() }); }
                RulerDrag::NewTab => {
                    if response.clicked() || response.drag_stopped() {
                        let pos = snap(from_start(tw), free);
                        let mut stops: Vec<TabStop> = fmt.tabs.iter().copied().filter(|t| t.kind != TabKind::Clear && t.pos != pos).collect();
                        stops.push(TabStop { pos, kind: app.ruler_tab_kind, leader: Leader::None });
                        app.doc.set_tab_stops(stops);
                    }
                }
                RulerDrag::Tab(old) => {
                    let mut stops: Vec<TabStop> = fmt.tabs.iter().copied().filter(|t| t.kind != TabKind::Clear).collect();
                    let kind = stops.iter().find(|t| t.pos == old).map(|t| (t.kind, t.leader));
                    stops.retain(|t| t.pos != old);
                    // Dragging a tab off the ruler removes it.
                    if (p.y - rect.center().y).abs() < 30.0 {
                        if let Some((k, l)) = kind { stops.push(TabStop { pos: snap(from_start(tw), free), kind: k, leader: l }); }
                    }
                    app.doc.set_tab_stops(stops);
                }
                RulerDrag::Column(ti, col) => {
                    // The column's right edge follows the pointer; the next column absorbs it.
                    if let Some(&(_, _, edge)) = columns.iter().find(|c| c.1 == col) {
                        let w = app.doc.tables[ti].grid.get(col).copied().unwrap_or(0) as f32 + (p.x - sx(edge)) / zoom * 15.0;
                        app.doc.set_column_width(ti, col, snap(w, free), true);
                    }
                }
                RulerDrag::MarginLeft | RulerDrag::MarginRight => {
                    let mut s: PageSettings = app.doc.page.clone();
                    if kind == RulerDrag::MarginLeft { s.left = snap(tw, free).max(0) - s.gutter; } else { s.right = snap(page_w - tw, free).max(0); }
                    if let Err(e) = app.doc.set_page_settings(s) { app.notify(ui.ctx(), e); }
                }
                _ => {}
            }
            ui.data_mut(|d| d.insert_temp::<Option<RulerDrag>>(id, None));
        }
    }
    // Tab selector at the far left: click to cycle the kind of tab a click adds.
    let sel = Rect::from_min_size(rect.min + vec2(2.0, 3.0), vec2(18.0, 18.0));
    let r = ui.interact(sel, id.with("tab-kind"), Sense::click());
    painter.rect(sel, 2.0, if r.hovered() { c_card() } else { c_frame() }, Stroke::new(1.0, c_line()), egui::StrokeKind::Inside);
    let c = sel.center();
    let s = Stroke::new(1.5, c_ink2());
    painter.line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 4.0)], s);
    match app.ruler_tab_kind {
        TabKind::Left => { painter.line_segment([c + vec2(0.0, 4.0), c + vec2(5.0, 4.0)], s); }
        TabKind::Right => { painter.line_segment([c + vec2(-5.0, 4.0), c + vec2(0.0, 4.0)], s); }
        TabKind::Center => { painter.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, 4.0)], s); }
        _ => { painter.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, 4.0)], s); painter.circle_filled(c + vec2(3.0, 0.0), 1.2, c_ink2()); }
    }
    if r.on_hover_text(format!("{:?} tab (click to change)", app.ruler_tab_kind)).clicked() {
        app.ruler_tab_kind = match app.ruler_tab_kind { TabKind::Left => TabKind::Center, TabKind::Center => TabKind::Right, TabKind::Right => TabKind::Decimal, _ => TabKind::Left };
    }
}

pub fn vertical(app: &mut WordApp, ui: &mut Ui) {
    let (rect, response) = ui.allocate_exact_size(vec2(W, ui.available_height()), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, c_frame());
    let (_, top, tops, zoom) = app.canvas_map.clone();
    let page = app.layout.page_of(app.doc.selection.focus);
    let Some(page_top) = tops.get(page) else { return };
    let g = app.page_geometry();
    let sy = |py: f32| top + (page_top + py) * zoom;
    let band = Rect::from_min_max(pos2(rect.min.x + 3.0, sy(0.0)), pos2(rect.max.x - 3.0, sy(g.height)));
    painter.rect_filled(band, 1.0, if crate::view::DARK.with(|d| d.get()) { Color32::from_gray(0x3A) } else { Color32::from_gray(222) });
    painter.rect_filled(Rect::from_min_max(pos2(band.min.x, sy(g.top)), pos2(band.max.x, sy(g.height - g.bottom))), 0.0, c_card());
    ticks(&painter, false, band.min.x, band.width(), sy(g.top), band.min.y.max(rect.min.y), band.max.y.min(rect.max.y), zoom);
    let id = ui.make_persistent_id("vruler-drag");
    if response.drag_started() {
        if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
            let hit = if (p.y - sy(g.top)).abs() < 5.0 { Some(RulerDrag::MarginTop) } else if (p.y - sy(g.height - g.bottom)).abs() < 5.0 { Some(RulerDrag::MarginBottom) } else { None };
            ui.data_mut(|d| d.insert_temp(id, hit));
        }
    }
    if response.hovered() { if let Some(p) = response.hover_pos() { if (p.y - sy(g.top)).abs() < 5.0 || (p.y - sy(g.height - g.bottom)).abs() < 5.0 { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical); } } }
    let drag: Option<RulerDrag> = ui.data(|d| d.get_temp(id)).flatten();
    if let (Some(kind), true, Some(p)) = (drag, response.drag_stopped(), ui.input(|i| i.pointer.latest_pos())) {
        let tw = ((p.y - top) / zoom - page_top) * 15.0;
        let free = ui.input(|i| i.modifiers.alt);
        let mut s = app.doc.page.clone();
        if kind == RulerDrag::MarginTop { s.top = snap(tw, free).max(0); } else { s.bottom = snap(g.height * 15.0 - tw, free).max(0); }
        if let Err(e) = app.doc.set_page_settings(s) { app.notify(ui.ctx(), e); }
        ui.data_mut(|d| d.insert_temp::<Option<RulerDrag>>(id, None));
    }
}
