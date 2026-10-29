//! Page canvas: painting and the input adapter (canvas-editor.js).
//!
//! Pages stack vertically with a gap. Layout coordinates are 96-dpi px; the
//! screen position of a layout point is `page_origin + p * zoom`.

use crate::app::{canvas_state::{Drag, Unit}, face_name, WordApp};
use egui::{pos2, vec2, Color32, CursorIcon, Event, FontFamily, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui};
use word_engine::layout::DrawItem;
use std::collections::HashMap;
use word_engine::Color;

pub const GAP: f32 = 24.0;

fn c32(c: Color) -> Color32 { Color32::from_rgb(c.0, c.1, c.2) }

/// Top of each page and the canvas size, in unzoomed px.
/// Page positions (unzoomed, from the canvas origin) and the canvas size. Print layout stacks
/// pages; Read Mode lays them out side by side in rows of `columns`.
fn page_places(app: &WordApp, columns: usize) -> (Vec<f32>, Vec<f32>, f32, f32) {
    let (mut lefts, mut tops) = (vec![], vec![]);
    let (w, h) = app.layout.pages.iter().fold((0.0f32, 0.0f32), |a, p| (a.0.max(p.width), a.1.max(p.height)));
    let columns = columns.max(1);
    for i in 0..app.layout.pages.len() {
        let (col, row) = (i % columns, i / columns);
        lefts.push(GAP + col as f32 * (w + GAP));
        tops.push(GAP + row as f32 * (h + GAP));
    }
    let rows = app.layout.pages.len().div_ceil(columns).max(1);
    (lefts, tops, columns as f32 * (w + GAP) + GAP, rows as f32 * (h + GAP) + GAP)
}

pub fn show(app: &mut WordApp, ui: &mut Ui) {
    app.extras.view.canvas_width = ui.available_width();
    app.extras.view.canvas_height = ui.available_height();
    if app.fit_width {
        app.fit_width = false;
        if let Some(p) = app.layout.pages.first() { app.set_zoom((ui.available_width() - 2.0 * GAP - 16.0) / p.width); }
    }
    // Read Mode fits a page to the window height, two side by side when there is room.
    let (zoom, columns) = if app.read_mode {
        let p = app.layout.pages.first().map(|p| (p.width, p.height)).unwrap_or((816.0, 1056.0));
        let z = ((ui.available_height() - 2.0 * GAP) / p.1).clamp(0.25, 3.0);
        let two = ui.available_width() >= (2.0 * p.0 + 3.0 * GAP) * z;
        (z, if two { 2 } else { 1 })
    } else if app.extras.view.multi_page && app.extras.view.mode == crate::view::ViewMode::Print {
        // Multiple Pages: as many side by side as fit.
        let w = app.layout.pages.first().map(|p| p.width).unwrap_or(816.0);
        (app.zoom, (((ui.available_width() - GAP) / ((w + GAP) * app.zoom)).floor() as usize).clamp(1, 8))
    } else { (app.zoom, 1) };
    let (lefts, tops, total_w, total_h) = page_places(app, columns);
    let bg = if app.extras.view.dark { Color32::from_gray(0x12) } else { Color32::from_rgb(0xE9, 0xEB, 0xEE) };
    ui.painter().rect_filled(ui.max_rect(), 0.0, bg);
    let id = ui.make_persistent_id("document-canvas");
    app.atlas.begin(ui.ctx());
    // Synchronous scrolling: follow the partner view's last movement (Split or Side by Side).
    let pane = app.extras.view.pane;
    let forced = app.extras.view.follow(pane);
    let mut area = egui::ScrollArea::both().id_salt("canvas-scroll").auto_shrink(false);
    if let Some(o) = forced { area = area.scroll_offset(o); }
    let out = area.show_viewport(ui, |ui, viewport| {
        let avail_w = ui.available_width().max(total_w * zoom);
        let (rect, _) = ui.allocate_exact_size(vec2(avail_w, total_h * zoom), Sense::hover());
        let response = ui.interact(rect, id, Sense::click_and_drag());
        let origin_x = rect.min.x + ((avail_w - total_w * zoom) / 2.0).max(0.0);
        // Page places were computed before this frame's edits; a page count that changed since
        // (paste, undo) must not index past them.
        let page_rect = |i: usize, pages: &[word_engine::layout::PageBox]| -> Rect {
            match (lefts.get(i), tops.get(i), pages.get(i)) {
                (Some(l), Some(t), Some(p)) => Rect::from_min_size(pos2(origin_x + l * zoom, rect.min.y + t * zoom), vec2(p.width * zoom, p.height * zoom)),
                _ => Rect::NOTHING,
            }
        };
        let to_screen = |page: usize, x: f32, y: f32| pos2(origin_x + (lefts.get(page).copied().unwrap_or(0.0) + x) * zoom, rect.min.y + (tops.get(page).copied().unwrap_or(0.0) + y) * zoom);
        // Screen point -> (page, layout x, y): the page in the pointer's row and column.
        let from_screen = |p: Pos2| -> (usize, f32, f32) {
            let (x, y) = ((p.x - origin_x) / zoom, (p.y - rect.min.y) / zoom);
            let page = (0..tops.len()).filter(|&i| tops[i] <= y + GAP / 2.0).filter(|&i| lefts[i] <= x + GAP / 2.0 || lefts[i] == GAP).max_by(|&a, &b| (tops[a], lefts[a]).partial_cmp(&(tops[b], lefts[b])).unwrap()).unwrap_or(0);
            (page, x - lefts.get(page).copied().unwrap_or(0.0), y - tops.get(page).copied().unwrap_or(0.0))
        };

        app.canvas_map = (origin_x, rect.min.y, tops.clone(), zoom);
        // ---- input ----
        if response.clicked() || response.drag_started() { response.request_focus(); app.canvas_focus = true; }
        if app.canvas_focus && !ui.memory(|m| m.focused().is_some()) { response.request_focus(); }
        let focused = response.has_focus();
        if focused {
            ui.memory_mut(|m| m.set_focus_lock_filter(id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: false }));
            if app.read_mode { read_keys(app, ui, columns); } else { handle_keys(app, ui); }
        }
        if app.read_mode && app.read_scroll {
            app.read_scroll = false;
            let first = app.read_row * columns;
            if first < app.layout.pages.len() { ui.scroll_to_rect(page_rect(first, &app.layout.pages), Some(egui::Align::Min)); }
        }
        handle_pointer(app, ui, &response, &from_screen);
        app.relayout();
        if response.hovered() {
            let hover = ui.input(|i| i.pointer.hover_pos());
            let at = hover.map(|p| from_screen(p));
            let over_image = at.and_then(|(pg, x, y)| app.layout.image_at(pg, x, y)).is_some();
            let (s, e) = app.doc.range();
            let over_selection = s != e && at.and_then(|(pg, x, y)| app.layout.hit_test(pg, x, y)).is_some_and(|p| p >= s && p < e);
            let margin = at.is_some_and(|(pg, x, y)| in_left_margin(app, pg, x, y));
            let on_border = at.and_then(|(pg, x, y)| column_border_at(app, pg, x, y)).is_some();
            let handle = at.and_then(|(pg, x, y)| object_handle_at(app, pg, x, y)).map(|h| h.1);
            if let Some(k) = handle.filter(|_| matches!(app.drag, Drag::None)) { ui.ctx().set_cursor_icon(if k == 8 { CursorIcon::Grab } else { HANDLE_CURSORS[k] }); }
            else if app.table_tool.is_some() { ui.ctx().set_cursor_icon(if app.table_tool == Some(crate::app::TableTool::Pen) { CursorIcon::Crosshair } else { CursorIcon::NotAllowed }); }
            else if on_border || matches!(app.drag, Drag::Column { .. }) { ui.ctx().set_cursor_icon(CursorIcon::ResizeColumn); }
            else if !matches!(app.drag, Drag::Move { .. } | Drag::Resize { .. } | Drag::Rotate { .. } | Drag::Float { .. }) {
                ui.ctx().set_cursor_icon(if over_image || over_selection { CursorIcon::Default } else if margin { CursorIcon::PointingHand } else { CursorIcon::Text });
            }
        }
        response.context_menu(|ui| crate::context::menu(app, ui));

        // ---- paint ----
        let painter = ui.painter_at(viewport.translate(rect.min.to_vec2()).intersect(ui.clip_rect()).expand(2.0));
        let visible = viewport.translate(rect.min.to_vec2());
        let (sel_s, sel_e) = app.doc.range();
        // A selection spanning cells highlights whole cells, as Word does.
        let cell_sel: Vec<u64> = {
            let cell_of = |p: usize| app.doc.paragraphs[app.doc.locate(p).0].cell;
            let (a, f) = (cell_of(app.doc.selection.anchor), cell_of(app.doc.selection.focus));
            if a.is_some() && f.is_some() && a != f { app.doc.cell_block().map(|b| app.doc.block_cells(b)).unwrap_or_default() } else { vec![] }
        };
        let sel_rects: Vec<(usize, f32, f32, f32, f32)> = if cell_sel.is_empty() { app.layout.selection_rects(sel_s, sel_e) } else {
            let in_cells = |pos: usize| app.doc.paragraphs[app.doc.locate(pos).0].cell.is_some();
            let mut v: Vec<_> = app.layout.cells.iter().filter(|c| cell_sel.contains(&c.cell)).map(|c| (c.page, c.x0, c.y0, c.x1 - c.x0, c.y1 - c.y0)).collect();
            // Body text selected along with the cells keeps its ordinary highlight.
            v.extend(app.layout.selection_rects(sel_s, sel_e).into_iter().filter(|r| app.layout.hit_test(r.0, r.1 + 1.0, r.2 + r.4 / 2.0).is_some_and(|p| !in_cells(p))));
            v
        };
        // Comment anchors: pale for open threads, stronger for the thread at the caret.
        let active_comments = app.doc.comments_at(app.doc.selection.focus);
        let comment_rects: Vec<_> = app.doc.comment_anchors().into_iter()
            .filter(|_| app.extras.review.show_comment_marks)
            .filter(|(id, ..)| app.doc.comments.iter().any(|c| c.id == *id && c.parent.is_none() && !c.done))
            .flat_map(|(id, a, b)| { let strong = active_comments.contains(&id); app.layout.selection_rects(a, b).into_iter().map(move |r| (r, strong)) }).collect();
        // Highlight Merge Fields: gray shading on every merge field.
        let merge_rects: Vec<_> = if app.doc.mail_merge.highlight {
            let mut out = vec![];
            let mut pos = 0;
            for p in &app.doc.paragraphs {
                for r in &p.runs { if r.field.as_ref().is_some_and(word_engine::mailmerge::is_merge_field) { out.extend(app.layout.selection_rects(pos, pos + r.len())); } pos += r.len(); }
                pos += 1;
            }
            out
        } else { vec![] };
        let find_rects: Vec<_> = if app.find.open { app.find.matches.iter().flat_map(|&(a, b)| app.layout.selection_rects(a, b)).collect() } else { vec![] };
        // Hide Ink shows no strokes; Ink Replay reveals them one by one.
        let now = ui.input(|i| i.time);
        let limit = if app.extras.draw.hide_ink { Some(0) } else if let Some(start) = app.extras.draw.replay {
            let mut seqs: Vec<u32> = app.doc.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| if let Some(word_engine::objects::Object::Ink(i)) = &r.object { Some(i.strokes.iter().map(|s| s.seq).collect::<Vec<_>>()) } else { None }).flatten().collect();
            seqs.sort_unstable();
            let shown = ((now - start) / 0.35) as usize;
            ui.ctx().request_repaint();
            if shown >= seqs.len() { app.extras.draw.replay = None; None } else { Some(seqs[shown]) }
        } else { None };
        INK_LIMIT.with(|l| l.set(limit));
        let story_page = app.extras.story.as_ref().map(|s| s.page);
        for i in 0..app.layout.pages.len() {
            let pr = page_rect(i, &app.layout.pages);
            if !pr.intersects(visible) { continue; }
            painter.rect_filled(pr.translate(vec2(0.0, 2.0)), 2.0, Color32::from_black_alpha(28));
            painter.rect_filled(pr, 0.0, Color32::WHITE);
            if app.extras.view.page_grid {
                // View ▸ Gridlines: a drawing grid inside the margins.
                let g = app.page_geometry();
                let (x0, x1, y0, y1) = (pr.min.x + g.left * zoom, pr.max.x - g.right * zoom, pr.min.y + g.top * zoom, pr.max.y - g.bottom * zoom);
                let step = 12.0 * zoom;
                let stroke = Stroke::new(0.5, Color32::from_rgba_unmultiplied(110, 150, 220, 70));
                let mut x = x0; while x <= x1 { painter.line_segment([pos2(x, y0), pos2(x, y1)], stroke); x += step; }
                let mut y = y0; while y <= y1 { painter.line_segment([pos2(x0, y), pos2(x1, y)], stroke); y += step; }
            }
            // Editing a header or footer: the body shows dimmed behind it, and the story repeats
            // on every page.
            if let Some(st) = &app.extras.story {
                if let Some(hp) = st.host_layout.pages.get(i) {
                    let (textures, atlas) = (&mut app.textures, &mut app.atlas);
                    let mut body = egui::Mesh::with_texture(atlas.texture_id().unwrap_or_default());
                    let mut pen = Pen { atlas, mesh: &mut body, ppp: ui.ctx().pixels_per_point() };
                    for item in hp.under.iter().chain(hp.items.iter()).chain(hp.over.iter()) { paint_item(textures, &st.host.assets, ui, &painter, item, i, zoom, &to_screen, &mut pen); }
                    painter.add(egui::Shape::mesh(body));
                    painter.rect_filled(pr, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 150));
                }
                if let Some((b0, b1)) = app.story_band() {
                    let y = if st.header { b1 } else { b0 };
                    let (a, b) = (to_screen(i, 0.0, y), to_screen(i, app.layout.pages[i].width, y));
                    painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, Color32::from_rgb(0x2B, 0x57, 0x9A)), 5.0, 3.0));
                    let label = if st.header { "Header" } else { "Footer" };
                    let tab = Rect::from_min_size(if st.header { a } else { a - vec2(0.0, 18.0) } + vec2(8.0, 0.0), vec2(54.0, 18.0));
                    painter.rect_filled(tab, 2.0, Color32::from_rgb(0xE8, 0xEE, 0xF8));
                    painter.text(tab.center(), egui::Align2::CENTER_CENTER, label, FontId::proportional(11.0), Color32::from_rgb(0x2B, 0x57, 0x9A));
                }
            }
            let page = &app.layout.pages[story_page.unwrap_or(i)];
            let (textures, assets, atlas) = (&mut app.textures, &app.doc.assets, &mut app.atlas);
            let mut text = egui::Mesh::with_texture(atlas.texture_id().unwrap_or_default());
            let mut pen = Pen { atlas, mesh: &mut text, ppp: ui.ctx().pixels_per_point() };
            for item in page.under.iter() { paint_item(textures, assets, ui, &painter, item, i, zoom, &to_screen, &mut pen); }
            for &((pg, x, y, w, h), strong) in &comment_rects { if pg == i { painter.rect_filled(Rect::from_min_size(to_screen(pg, x, y), vec2(w * zoom, h * zoom)), 0.0, if strong { Color32::from_rgba_unmultiplied(250, 190, 60, 110) } else { Color32::from_rgba_unmultiplied(250, 215, 110, 60) }); } }
            crate::review::paint_squiggles(&app.extras.review, &app.layout, &painter, i, zoom, &to_screen);
            for &(pg, x, y, w, h) in &merge_rects { if pg == i { painter.rect_filled(Rect::from_min_size(to_screen(pg, x, y), vec2(w * zoom, h * zoom)), 0.0, Color32::from_rgba_unmultiplied(150, 150, 150, 70)); } }
            for &(pg, x, y, w, h) in &find_rects { if pg == i { painter.rect_filled(Rect::from_min_size(to_screen(pg, x, y), vec2(w * zoom, h * zoom)), 0.0, Color32::from_rgba_unmultiplied(255, 200, 0, 90)); } }
            // A selected picture or object shows its frame and handles, not a text highlight.
            let object_selected = sel_e == sel_s + 1 && (app.doc.image_at(sel_s).is_some() || app.doc.object_at(sel_s).is_some());
            for &(pg, x, y, w, h) in &sel_rects { if pg == i && !object_selected { painter.rect_filled(Rect::from_min_size(to_screen(pg, x, y), vec2(w * zoom, h * zoom)), 0.0, Color32::from_rgba_unmultiplied(70, 130, 230, 80)); } }
            for item in page.items.iter() { paint_item(textures, assets, ui, &painter, item, i, zoom, &to_screen, &mut pen); }
            let Pen { atlas, ppp, .. } = pen;
            painter.add(egui::Shape::mesh(text));
            if !page.over.is_empty() {
                // Objects in front of text, with their own text above them.
                let mut front = egui::Mesh::with_texture(atlas.texture_id().unwrap_or_default());
                let mut pen = Pen { atlas, mesh: &mut front, ppp };
                for item in page.over.iter() { paint_item(textures, assets, ui, &painter, item, i, zoom, &to_screen, &mut pen); }
                painter.add(egui::Shape::mesh(front));
            }
            if app.show_marks && story_page.is_none_or(|p| p == i) { paint_marks(app, &painter, i, zoom, &to_screen); }
            // Gridlines for tables without borders (View Gridlines).
            if app.show_gridlines {
                for c in app.layout.cells.iter().filter(|c| c.page == i && !app.doc.tables.get(c.table).is_some_and(|t| t.borders || t.style.as_deref().and_then(|s| app.doc.styles.get(s)).is_some_and(|s| s.table_borders))) {
                    let r = Rect::from_min_max(to_screen(i, c.x0, c.y0), to_screen(i, c.x1, c.y1));
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(110, 150, 220, 90)), egui::StrokeKind::Middle);
                }
            }
            // Pen stroke preview.
            if let Drag::Stroke { page: sp, x: sx, y: sy } = app.drag {
                if sp == i { if let Some(p) = ui.input(|inp| inp.pointer.latest_pos()) {
                    let a = to_screen(i, sx, sy);
                    let (dx, dy) = ((p.x - a.x).abs(), (p.y - a.y).abs());
                    let in_cell = app.layout.cells.iter().any(|c| c.page == i && sx > c.x0 && sx < c.x1 && sy > c.y0 && sy < c.y1);
                    let pen = Stroke::new(1.5, Color32::from_rgb(40, 40, 40));
                    if !in_cell { painter.rect_stroke(Rect::from_two_pos(a, p), 0.0, pen, egui::StrokeKind::Middle); }
                    else if dy > dx { painter.line_segment([a, pos2(a.x, p.y)], pen); } else { painter.line_segment([a, pos2(p.x, a.y)], pen); }
                } }
            }
            // Column resize guide.
            if let Drag::Column { .. } = app.drag {
                if let Some(p) = ui.input(|inp| inp.pointer.latest_pos()) {
                    let (pg, ..) = from_screen(p);
                    if pg == i { painter.line_segment([pos2(p.x, pr.top()), pos2(p.x, pr.bottom())], Stroke::new(1.0, Color32::from_rgb(40, 110, 220))); }
                }
            }
            // Margin corner guides, as in Word.
            let g = app.page_geometry();
            let guide = Stroke::new(1.0, Color32::from_gray(200));
            for (cx, cy, dx, dy) in [(g.left, g.top, -1.0, -1.0), (g.width - g.right, g.top, 1.0, -1.0), (g.left, g.height - g.bottom, -1.0, 1.0), (g.width - g.right, g.height - g.bottom, 1.0, 1.0)] {
                let c = to_screen(i, cx, cy);
                painter.line_segment([c, c + vec2(dx * 12.0 * zoom, 0.0)], guide);
                painter.line_segment([c, c + vec2(0.0, dy * 12.0 * zoom)], guide);
            }
        }

        // Table move (top-left) and resize (bottom-right) handles.
        if let Some(ti) = handle_table(app, ui, &from_screen) {
            if let Some((p0, x0, y0, p1, x1, y1)) = table_bounds(app, ti) {
                let mv = Rect::from_center_size(to_screen(p0, x0, y0) - vec2(9.0, 9.0), vec2(13.0, 13.0));
                painter.rect(mv, 2.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(120)), egui::StrokeKind::Inside);
                let c = mv.center();
                let ink = Stroke::new(1.0, Color32::from_gray(60));
                for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    let tip = c + vec2(dx * 4.5, dy * 4.5);
                    painter.line_segment([c, tip], ink);
                    painter.line_segment([tip, tip - vec2(dx * 2.0 + dy * 2.0, dy * 2.0 + dx * 2.0)], ink);
                    painter.line_segment([tip, tip - vec2(dx * 2.0 - dy * 2.0, dy * 2.0 - dx * 2.0)], ink);
                }
                let rs = Rect::from_min_size(to_screen(p1, x1, y1) + vec2(2.0, 2.0), vec2(7.0, 7.0));
                painter.rect(rs, 1.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(120)), egui::StrokeKind::Inside);
            }
        }
        if let Drag::TableResize { table, x, width } = app.drag {
            if let (Some(p), Some((_, x0, y0, p1, _, y1))) = (ui.input(|i| i.pointer.latest_pos()), table_bounds(app, table)) {
                let (_, px, _) = from_screen(p);
                let w = (width + px - x).max(40.0);
                let r = Rect::from_min_max(to_screen(p1, x0, y0.min(y1)), to_screen(p1, x0 + w, y1));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgb(40, 110, 220)), egui::StrokeKind::Middle);
            }
        }
        if let Drag::TableMove { target: Some(t), .. } = app.drag {
            if let Some(c) = app.layout.caret_rect(t) {
                let a = to_screen(c.page, c.x, c.y);
                painter.line_segment([a, a + vec2(80.0 * zoom, 0.0)], Stroke::new(2.0, Color32::from_rgb(40, 110, 220)));
            }
        }
        // Drop position while dragging text.
        if let Drag::Move { target: Some(t), .. } = app.drag {
            if let Some(c) = app.layout.caret_rect(t) {
                let top = to_screen(c.page, c.x, c.y);
                let mut y = top.y;
                while y < top.y + c.height * zoom { painter.line_segment([pos2(top.x, y), pos2(top.x, (y + 2.0).min(top.y + c.height * zoom))], Stroke::new(1.5, Color32::from_rgb(40, 40, 40))); y += 4.0; }
            }
        }
        // A selected equation shows a light frame; pictures and objects get handles.
        if sel_e == sel_s + 1 && matches!(app.doc.object_at(sel_s), Some(word_engine::objects::Object::Equation(_))) {
            if let Some((pg, x, y, w, h)) = image_box(app, sel_s) {
                let r = Rect::from_min_size(to_screen(pg, x, y), vec2(w * zoom, h * zoom)).expand(2.0);
                painter.rect(r, 2.0, Color32::from_rgba_unmultiplied(70, 130, 230, 28), Stroke::new(1.0, Color32::from_gray(170)), egui::StrokeKind::Outside);
            }
        } else if sel_e == sel_s + 1 && (app.doc.image_at(sel_s).is_some() || app.doc.object_at(sel_s).is_some()) {
            if let Some((pg, x, y, w, h)) = image_box(app, sel_s) {
                let blue = Color32::from_rgb(40, 110, 220);
                let r = Rect::from_min_size(to_screen(pg, x, y), vec2(w * zoom, h * zoom));
                // The frame and handles turn with a rotated object.
                let angle = object_rotation(app, sel_s);
                let c = (x + w / 2.0, y + h / 2.0);
                let at = |p: (f32, f32)| { let q = rotate_about(p, c, angle); to_screen(pg, q.0, q.1) };
                let frame: Vec<Pos2> = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)].into_iter().map(at).collect();
                painter.add(egui::Shape::closed_line(frame.clone(), Stroke::new(1.0, blue)));
                if let Some(rot) = rotation_handle(app, sel_s, pg, x, y, w) {
                    let hc = at(rot);
                    painter.line_segment([at((x + w / 2.0, y)), hc], Stroke::new(1.0, blue));
                    painter.circle(hc, 5.5, Color32::WHITE, Stroke::new(1.5, blue));
                }
                for (hx, hy) in handles(x, y, w, h) {
                    let hc = at((hx, hy));
                    painter.rect(Rect::from_center_size(hc, vec2(8.0, 8.0)), 1.0, Color32::WHITE, Stroke::new(1.2, blue), egui::StrokeKind::Inside);
                }
                // Layout Options: wrap choices beside the selection (hidden while dragging).
                if matches!(app.drag, Drag::None) && !app.read_mode {
                    let right = frame.iter().map(|p| p.x).fold(f32::MIN, f32::max);
                    let top = frame.iter().map(|p| p.y).fold(f32::MAX, f32::min);
                    let b = Rect::from_min_size(pos2(right + 10.0, top), vec2(24.0, 24.0));
                    let resp = ui.put(b, egui::Button::new("").fill(Color32::WHITE).stroke(Stroke::new(1.0, Color32::from_gray(170))).corner_radius(3.0)).on_hover_text(crate::i18n::tr("Layout Options"));
                    crate::ribbon::paint_icon(ui, "layoutOptions", b.shrink(4.0), 16, true);
                    crate::theme::menu(&resp, |ui| {
                        ui.label(egui::RichText::new(crate::i18n::tr("Layout Options")).strong());
                        crate::picture::wrap_menu(app, ui);
                    });
                }
                // Live outline while resizing or rotating.
                let now = ui.input(|i| (i.pointer.latest_pos(), i.modifiers.shift));
                if let Some(p) = now.0 {
                    let dash = Stroke::new(1.0, Color32::from_rgb(90, 90, 90));
                    match app.drag {
                        Drag::Resize { pos, handle, w, h, from: o } => {
                            let (dx, dy) = rotate_about(((p.x - o.x) / zoom, (p.y - o.y) / zoom), (0.0, 0.0), -angle);
                            let (nw, nh) = resized(app, pos, handle, w, h, dx, dy, now.1);
                            let nc = (x + nw / 2.0, y + nh.max(1.0) / 2.0);
                            let pts: Vec<Pos2> = [(x, y), (x + nw, y), (x + nw, y + nh.max(1.0)), (x, y + nh.max(1.0)), (x, y)].into_iter().map(|q| { let q = rotate_about(q, nc, angle); to_screen(pg, q.0, q.1) }).collect();
                            let corner = pts[2];
                            painter.extend(egui::Shape::dashed_line(&pts, dash, 4.0, 3.0));
                            painter.text(corner + vec2(6.0, 4.0), egui::Align2::LEFT_TOP, format!("{:.2}\" × {:.2}\"", nw / 96.0, nh / 96.0), FontId::proportional(11.0), Color32::from_rgb(60, 60, 60));
                        }
                        Drag::Float { from: o, .. } => {
                            let mut pts: Vec<Pos2> = frame.iter().map(|q| *q + (p - o)).collect();
                            pts.push(pts[0]);
                            painter.extend(egui::Shape::dashed_line(&pts, dash, 4.0, 3.0));
                        }
                        Drag::Rotate { center, base, from: o, .. } => {
                            let a = rotated_angle(center, o, p, base, now.1);
                            let (sin, cos) = a.to_radians().sin_cos();
                            let ctr = pos2(center.0, center.1);
                            let corners: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)].iter().map(|(sx, sy)| { let (dx, dy) = (sx * r.width() / 2.0, sy * r.height() / 2.0); ctr + vec2(dx * cos - dy * sin, dx * sin + dy * cos) }).collect();
                            painter.extend(egui::Shape::dashed_line(&corners, dash, 4.0, 3.0));
                            painter.text(r.right_top() + vec2(8.0, -14.0), egui::Align2::LEFT_TOP, format!("{:.0}°", a.rem_euclid(360.0)), FontId::proportional(11.0), Color32::from_rgb(60, 60, 60));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Ink being drawn, the lasso and the ink selection.
        let hover_at = ui.input(|i| i.pointer.latest_pos()).map(|p| from_screen(p));
        crate::draw::paint_overlay(app, &painter, &to_screen, zoom, hover_at);

        // Caret (blinks; solid while typing) and IME preedit.
        let now = ui.input(|i| i.time);
        if let Some(c) = app.layout.caret_rect(app.doc.selection.focus) {
            let top = to_screen(c.page, c.x, c.y);
            let caret = Rect::from_min_size(top, vec2(1.5_f32.max(zoom), c.height * zoom));
            if focused && app.doc.selection.is_caret() && ((now - app.caret_blink) % 1.0) < 0.6 {
                painter.rect_filled(caret, 0.0, Color32::BLACK);
            }
            if focused {
                if !app.preedit.is_empty() {
                    let f = app.doc.current_format();
                    let key = word_engine::fonts::resolve(&f.font, f.bold, f.italic);
                    let font = FontId::new(f.size * 4.0 / 3.0 * zoom, FontFamily::Name(face_name(key.face as usize).into()));
                    let g = painter.layout_no_wrap(app.preedit.clone(), font, Color32::BLACK);
                    let r = Rect::from_min_size(top, g.size());
                    painter.rect_filled(r, 0.0, Color32::WHITE);
                    painter.galley(top, g, Color32::BLACK);
                    painter.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, Color32::BLACK));
                }
                ui.ctx().output_mut(|o| o.ime = Some(egui::output::IMEOutput { purpose: egui::IMEPurpose::Normal, rect: response.rect, cursor_rect: caret, should_interrupt_composition: false }));
            }
            if app.scroll_to_caret {
                ui.scroll_to_rect(caret.expand2(vec2(40.0, 60.0)), None);
                app.scroll_to_caret = false;
            }
        }
        if focused { ui.ctx().request_repaint_after(std::time::Duration::from_millis(500)); }
    });
    if app.extras.view.scrolled(pane, out.state.clamped_offset(), forced.is_some()) { ui.ctx().request_repaint(); }
}

fn image_box(app: &WordApp, pos: usize) -> Option<(usize, f32, f32, f32, f32)> { app.layout.object_box(pos) }

/// Drop a floating object with its top-left at (x, y) on `page`: it anchors to the body
/// paragraph there, keeping the margin- and paragraph-relative offset.
pub fn drop_float(app: &mut WordApp, pos: usize, page: usize, x: f32, y: f32) {
    let g = app.page_geometry();
    let own = app.doc.locate(pos).0;
    // A float in a table cell stays in its cell; a body float re-anchors to the body paragraph
    // beside its new top edge.
    let in_cell = app.doc.paragraphs[own].cell.is_some();
    let target = if in_cell { pos } else {
        app.layout.lines.iter().filter(|l| l.page == page && l.cell.is_none() && l.y <= y.max(g.top)).last().map(|l| l.start)
            .or_else(|| app.layout.lines.iter().find(|l| l.page == page && l.cell.is_none()).map(|l| l.start)).unwrap_or(pos)
    };
    let pi = app.doc.locate(target).0;
    let (ox, top) = match app.layout.float_origins.get(&pos) {
        Some(&(pg, ox, top)) if pi == own && pg == page => (ox, top),
        // Another paragraph: offsets from the margin and that paragraph's first line there.
        _ => (g.left, app.layout.lines.iter().find(|l| l.paragraph == pi && l.page == page).map(|l| l.y).unwrap_or(y)),
    };
    let target = if pi == own { pos } else { app.doc.paragraph_start(pi) };
    let new_pos = app.doc.move_float(pos, target, x - ox, y - top);
    app.doc.select(new_pos, new_pos + 1);
}

/// Turn mesh vertices clockwise by `deg` about `c` (rotated pictures and shape text).
fn rotate_vertices(vs: &mut [egui::epaint::Vertex], deg: f32, c: Pos2) {
    let (sin, cos) = deg.to_radians().sin_cos();
    for v in vs { let d = v.pos - c; v.pos = c + vec2(d.x * cos - d.y * sin, d.x * sin + d.y * cos); }
}

/// A selected picture's or shape's rotation in degrees.
fn object_rotation(app: &WordApp, pos: usize) -> f32 {
    match (app.doc.object_at(pos), app.doc.image_at(pos)) { (Some(word_engine::objects::Object::Shape(s)), _) => s.rotation, (_, Some(i)) => i.rotation, _ => 0.0 }
}

/// Turn point `p` clockwise by `deg` about `c`.
fn rotate_about(p: (f32, f32), c: (f32, f32), deg: f32) -> (f32, f32) {
    if deg == 0.0 { return p; }
    let (sin, cos) = deg.to_radians().sin_cos();
    let (dx, dy) = (p.0 - c.0, p.1 - c.1);
    (c.0 + dx * cos - dy * sin, c.1 + dx * sin + dy * cos)
}

/// The eight resize handles of a box, clockwise from the top-left corner.
const HANDLE_DIRS: [(f32, f32); 8] = [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)];
const HANDLE_CURSORS: [CursorIcon; 8] = [CursorIcon::ResizeNwSe, CursorIcon::ResizeVertical, CursorIcon::ResizeNeSw, CursorIcon::ResizeHorizontal, CursorIcon::ResizeNwSe, CursorIcon::ResizeVertical, CursorIcon::ResizeNeSw, CursorIcon::ResizeHorizontal];

fn handles(x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 8] {
    HANDLE_DIRS.map(|(sx, sy)| (x + w * (sx + 1.0) / 2.0, y + h * (sy + 1.0) / 2.0))
}

/// Shapes and pictures rotate: their handle sits above the top edge's middle.
fn rotation_handle(app: &WordApp, pos: usize, _page: usize, x: f32, y: f32, w: f32) -> Option<(f32, f32)> {
    (matches!(app.doc.object_at(pos), Some(word_engine::objects::Object::Shape(_))) || app.doc.image_at(pos).is_some()).then(|| (x + w / 2.0, y - 22.0 / app.zoom))
}

/// The size a handle drag of (dx, dy) layout px gives. Corners keep a picture's aspect ratio
/// (Shift frees it); for shapes Shift keeps it.
#[allow(clippy::too_many_arguments)]
fn resized(app: &WordApp, pos: usize, handle: usize, w: f32, h: f32, dx: f32, dy: f32, shift: bool) -> (f32, f32) {
    let (sx, sy) = HANDLE_DIRS[handle];
    let line = matches!(app.doc.object_at(pos), Some(word_engine::objects::Object::Shape(s)) if s.kind.is_line());
    let mut nw = (w + sx * dx).max(8.0);
    let mut nh = (h + sy * dy).max(if line { 0.0 } else { 8.0 });
    let picture = app.doc.image_at(pos).is_some();
    if sx != 0.0 && sy != 0.0 && (picture != shift) && w > 0.0 && h > 0.0 {
        let k = if (dx / w).abs() >= (dy / h).abs() { nw / w } else { nh / h };
        nw = (w * k).max(8.0);
        nh = (h * k).max(8.0);
    }
    (nw, nh)
}

/// The rotation after dragging the rotate handle from `from` to `to` around `center` (screen
/// points); Shift snaps to 15°.
fn rotated_angle(center: (f32, f32), from: Pos2, to: Pos2, base: f32, snap: bool) -> f32 {
    let ang = |p: Pos2| (p.y - center.1).atan2(p.x - center.0).to_degrees();
    let a = base + ang(to) - ang(from);
    let a = if snap { (a / 15.0).round() * 15.0 } else { a };
    a.rem_euclid(360.0)
}

/// The handle of the selected picture or object under (page, x, y): Some(0..8) resizes, Some(8) rotates.
fn object_handle_at(app: &WordApp, page: usize, x: f32, y: f32) -> Option<(usize, usize, f32, f32, f32, f32)> {
    let (s, e) = app.doc.range();
    if e != s + 1 { return None; }
    if matches!(app.doc.object_at(s), Some(word_engine::objects::Object::Equation(_))) { return None; }
    let (pg, ix, iy, w, h) = image_box(app, s)?;
    if pg != page { return None; }
    let tol = 6.0 / app.zoom;
    // Test in the object's own (unrotated) frame.
    let (x, y) = rotate_about((x, y), (ix + w / 2.0, iy + h / 2.0), -object_rotation(app, s));
    if let Some((rx, ry)) = rotation_handle(app, s, pg, ix, iy, w) { if (x - rx).abs() < tol && (y - ry).abs() < tol { return Some((s, 8, ix, iy, w, h)); } }
    handles(ix, iy, w, h).iter().position(|&(hx, hy)| (x - hx).abs() < tol && (y - hy).abs() < tol).map(|k| (s, k, ix, iy, w, h))
}

thread_local! {
    /// The highest ink stroke drawn (Hide Ink: none; Ink Replay: those replayed so far).
    pub static INK_LIMIT: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Where a page's document text goes: shaped glyphs into one atlas-textured mesh.
#[allow(clippy::too_many_arguments)]
struct Pen<'a, 'm> { atlas: &'a mut crate::glyphs::GlyphAtlas, mesh: &'m mut egui::Mesh, ppp: f32 }

fn paint_item(textures: &mut HashMap<String, Option<egui::TextureHandle>>, assets: &HashMap<String, word_engine::Asset>, ui: &Ui, painter: &egui::Painter, item: &DrawItem, page: usize, zoom: f32, to_screen: &dyn Fn(usize, f32, f32) -> Pos2, pen: &mut Pen) {
    match item {
        DrawItem::Frame { .. } => {}
        DrawItem::Path { points, closed, fill, stroke, ink } => {
            // Hidden ink, and strokes not yet reached by Ink Replay, aren't drawn.
            if let Some(seq) = ink { if INK_LIMIT.with(|l| l.get()).is_some_and(|max| *seq > max) { return; } }
            let pts: Vec<Pos2> = points.iter().map(|p| to_screen(page, p.0, p.1)).collect();
            if let (Some(f), true) = (fill, *closed) { painter.add(egui::Shape::mesh(fill_polygon(&pts, c32(*f)))); }
            if let Some((c, w)) = stroke {
                let s = Stroke::new((w * zoom).max(0.6), c32(*c));
                painter.add(if *closed { egui::Shape::closed_line(pts, s) } else { egui::Shape::line(pts, s) });
            }
        }
        DrawItem::Rect { x, y, w, h, color } => { painter.rect_filled(Rect::from_min_size(to_screen(page, *x, *y), vec2(w * zoom, h * zoom)), 0.0, c32(*color)); }
        DrawItem::Line { x1, y1, x2, y2, width, color } => { painter.line_segment([to_screen(page, *x1, *y1), to_screen(page, *x2, *y2)], Stroke::new((width * zoom).max(0.6), c32(*color))); }
        DrawItem::Glyphs { glyphs, baseline, font, size, color, effects } => {
            let clip = painter.clip_rect();
            let p = to_screen(page, 0.0, *baseline);
            if p.y < clip.min.y - 100.0 || p.y > clip.max.y + 100.0 { return; }
            use crate::glyphs::{Paint, Style};
            let first_vertex = pen.mesh.vertices.len();
            let at = |dx: f32, dy: f32| glyphs.iter().map(move |g| (g.gid, to_screen(page, g.x + dx, baseline - g.dy + dy)));
            if let Some(glow) = effects.glow {
                // Glow: the blurred glyph in the glow color (60% opacity), behind the text.
                let r = (glow.radius * 4.0 / 3.0 * zoom * pen.ppp / 2.0).clamp(1.0, 24.0) as u8;
                let c = Color32::from_rgba_unmultiplied(glow.color.0, glow.color.1, glow.color.2, 170);
                pen.atlas.add_styled(pen.mesh, pen.ppp, *font, size * zoom, at(0.0, 0.0), Style::Blur(r), Paint::Flat(c));
                pen.atlas.add_styled(pen.mesh, pen.ppp, *font, size * zoom, at(0.0, 0.0), Style::Blur(r / 2 + 1), Paint::Flat(c));
            }
            if effects.shadow {
                let d = size / 18.0;
                pen.atlas.add_glyphs(pen.mesh, pen.ppp, *font, size * zoom, at(d, d), Color32::from_gray(150), false);
            }
            let style = if effects.outline { Style::Stroke } else { Style::Fill };
            if effects.reflection {
                pen.atlas.add_styled(pen.mesh, pen.ppp, *font, size * zoom, at(0.0, 0.0), style, Paint::Reflection(c32(*color), size * zoom * 0.22));
            }
            let paint = match effects.gradient { Some((a, b)) => Paint::Gradient(c32(a), c32(b)), None => Paint::Flat(c32(*color)) };
            pen.atlas.add_styled(pen.mesh, pen.ppp, *font, size * zoom, at(0.0, 0.0), style, paint);
            if let Some((a, cx, cy)) = effects.rotate { rotate_vertices(&mut pen.mesh.vertices[first_vertex..], a, to_screen(page, cx, cy)); }
        }
        DrawItem::Image { asset, x, y, w, h, rot, flip, .. } => {
            let r = Rect::from_min_size(to_screen(page, *x, *y), vec2(w * zoom, h * zoom));
            // Mirrored pictures swap their texture coordinates.
            let (u0, u1) = if flip.0 { (1.0, 0.0) } else { (0.0, 1.0) };
            let (v0, v1) = if flip.1 { (1.0, 0.0) } else { (0.0, 1.0) };
            let uv = Rect::from_min_max(pos2(u0, v0), pos2(u1, v1));
            if !textures.contains_key(asset) {
                let tex = assets.get(asset).and_then(|a| image::load_from_memory(&a.bytes).ok()).map(|img| {
                    // Large pictures are downscaled for display only.
                    let img = if img.width() > 2400 || img.height() > 2400 { img.thumbnail(2400, 2400) } else { img };
                    let rgba = img.to_rgba8();
                    let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
                    ui.ctx().load_texture(asset.clone(), ci, egui::TextureOptions::LINEAR)
                });
                textures.insert(asset.clone(), tex);
            }
            match textures.get(asset).and_then(|t| t.as_ref()) {
                Some(t) if *rot != 0.0 => {
                    let mut mesh = egui::Mesh::with_texture(t.id());
                    mesh.add_rect_with_uv(r, uv, Color32::WHITE);
                    rotate_vertices(&mut mesh.vertices, *rot, r.center());
                    painter.add(egui::Shape::mesh(mesh));
                }
                Some(t) => { painter.image(t.id(), r, uv, Color32::WHITE); }
                None => {
                    // Unsupported formats (EMF/WMF/TIFF) show a placeholder.
                    painter.rect(r, 0.0, Color32::from_gray(240), Stroke::new(1.0, Color32::from_gray(170)), egui::StrokeKind::Inside);
                    painter.text(r.center(), egui::Align2::CENTER_CENTER, "Picture", FontId::proportional(12.0 * zoom), Color32::from_gray(120));
                }
            }
        }
    }
}

/// Formatting marks: ¶ at paragraph ends, · for spaces, → for tabs.
fn paint_marks(app: &WordApp, painter: &egui::Painter, page: usize, zoom: f32, to_screen: &dyn Fn(usize, f32, f32) -> Pos2) {
    let color = Color32::from_rgb(90, 120, 200);
    let font = FontId::proportional(11.0 * zoom);
    let text = app.doc.text();
    let chars: Vec<char> = text.chars().collect();
    for l in app.layout.lines.iter().filter(|l| l.page == page) {
        for pos in l.start..l.end.min(chars.len()) {
            let Some(&x) = l.stops.get(pos - l.start) else { continue };
            let mark = match chars[pos] { ' ' => "·", '\t' => "→", '\u{000B}' => "↵", '\u{000C}' => "⤓", _ => continue };
            let w = l.stops.get(pos - l.start + 1).copied().unwrap_or(x) - x;
            painter.text(to_screen(page, x + w / 2.0, l.baseline - 3.0), egui::Align2::CENTER_BOTTOM, mark, font.clone(), color);
        }
        if l.last_in_paragraph {
            let x = *l.stops.last().unwrap_or(&l.left);
            painter.text(to_screen(page, x + 1.0, l.baseline), egui::Align2::LEFT_BOTTOM, "¶", font.clone(), color);
        }
    }
}

fn handle_pointer(app: &mut WordApp, ui: &Ui, response: &egui::Response, from_screen: &dyn Fn(Pos2) -> (usize, f32, f32)) {
    let (pressed, down, released, latest, modifiers, now) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released(), i.pointer.latest_pos(), i.modifiers, i.time));
    if response.secondary_clicked() {
        // Right-click inside the selection keeps it for the context menu.
        if let Some(p) = response.interact_pointer_pos() {
            let (page, x, y) = from_screen(p);
            if let Some((pos, ..)) = app.layout.image_at(page, x, y) { app.doc.select(pos, pos + 1); }
            else if let Some(pos) = app.layout.hit_test(page, x, y) { let (s, e) = app.doc.range(); if pos < s || pos > e { app.doc.select(pos, pos); } }
        }
        return;
    }
    let Some(pointer) = latest else { return };
    let (page, x, y) = from_screen(pointer);
    // Draw tab tools take the pointer.
    if app.extras.draw.tool != crate::draw::Tool::Select && !app.read_mode && app.extras.story.is_none() && (response.hovered() || !matches!(app.extras.draw.stroke, None) || app.extras.draw.lasso.is_some() || app.extras.draw.moving.is_some()) {
        if crate::draw::pointer(app, ui, response.hovered(), pressed, down, released, (page, x, y), pointer) { return; }
    }

    if pressed && response.hovered() {
        // Click counting for double and triple clicks.
        let (t, p, n) = app.last_click;
        let count = if now - t < 0.45 && p.distance(pointer) < 6.0 { (n % 3) + 1 } else { 1 };
        app.last_click = (now, pointer, count);
        app.goal_x = None;
        app.caret_blink = now;
        // Headers and footers: double-click to edit; while editing, clicks outside them are
        // ignored and a double-click there goes back to the document.
        if app.extras.story.is_some() {
            if let Some((b0, b1)) = app.story_band() { if y < b0 || y > b1 { if count >= 2 { app.end_story(); } return; } }
            if let Some(st) = app.extras.story.as_mut().filter(|s| s.page != page) { st.page = page; app.layout = Default::default(); app.relayout(); }
        } else if count >= 2 && !app.read_mode && app.layout.image_at(page, x, y).is_none() {
            let g = app.page_geometry();
            if y < g.top - 2.0 { app.edit_story(true, Some(page)); return; }
            if y > g.height - g.bottom + 2.0 { app.edit_story(false, Some(page)); return; }
        }
        if let Some(ti) = handle_table(app, ui, from_screen) {
            if let Some((p0, x0, y0, p1, x1, y1)) = table_bounds(app, ti) {
                let tol = 9.0 / app.zoom;
                if page == p0 && (x - (x0 - 9.0 / app.zoom)).abs() < tol && (y - (y0 - 9.0 / app.zoom)).abs() < tol { app.drag = Drag::TableMove { table: ti, target: None }; return; }
                if page == p1 && x > x1 && x < x1 + 12.0 / app.zoom && y > y1 && y < y1 + 12.0 / app.zoom {
                    app.drag = Drag::TableResize { table: ti, x, width: x1 - x0 };
                    return;
                }
            }
        }
        match app.table_tool {
            Some(crate::app::TableTool::Pen) => { app.drag = Drag::Stroke { page, x, y }; return; }
            Some(crate::app::TableTool::Eraser) => {
                if let Some((a, b)) = shared_border_at(app, page, x, y) { app.doc.erase_border(a, b); }
                else { app.notify(ui.ctx(), "Click a line between two cells to erase it"); }
                return;
            }
            None => {}
        }
        if let Some((table, col, bx)) = column_border_at(app, page, x, y) { app.drag = Drag::Column { table, col, x: bx }; return; }
        let (s, e) = app.doc.range();
        // A selected picture's or object's handles resize or rotate it.
        if let Some((pos, handle, ix, iy, w, h)) = object_handle_at(app, page, x, y) {
            if handle == 8 {
                let base = match (app.doc.object_at(pos), app.doc.image_at(pos)) { (Some(word_engine::objects::Object::Shape(sh)), _) => sh.rotation, (_, Some(i)) => i.rotation, _ => 0.0 };
                // The shape's center in screen points, from the pointer's offset to it.
                let center = (pointer.x + (ix + w / 2.0 - x) * app.zoom, pointer.y + (iy + h / 2.0 - y) * app.zoom);
                app.drag = Drag::Rotate { pos, center, base, from: pointer };
            } else {
                app.drag = Drag::Resize { pos, handle, w, h, from: pointer };
            }
            return;
        }
        if let Some((pos, ..)) = app.layout.image_at(page, x, y) {
            // Double-clicking a shape, chart or diagram opens its editor.
            if count >= 2 && app.doc.object_at(pos).is_some() { app.edit_object(pos); return; }
            if count >= 2 && app.doc.image_at(pos).is_some() { app.doc.select(pos, pos + 1); app.tab = crate::ribbon::Tab::PictureFormat; return; }
            app.doc.select(pos, pos + 1);
            // Floating objects move freely on the page; inline ones move like text.
            if app.doc.anchor_at(pos).is_some() {
                if let Some((pg, bx, by, ..)) = image_box(app, pos) { app.drag = Drag::Float { pos, from: pointer, page: pg, x: bx, y: by }; return; }
            }
            app.drag = Drag::Pending { s: pos, e: pos + 1, at: pos };
            return;
        }
        let Some(pos) = app.layout.hit_test(page, x, y) else { return };
        let in_margin = in_left_margin(app, page, x, y);
        if !modifiers.shift && count == 1 && !in_margin && s != e && pos >= s && pos < e {
            app.drag = Drag::Pending { s, e, at: pos };
            return;
        }
        let unit = match (in_margin, count) { (true, 1) => Unit::Line, (true, _) | (false, 3) => Unit::Paragraph, (false, 2) => Unit::Word, _ => Unit::Char };
        if modifiers.command && count == 1 && !in_margin {
            if let Some(link) = app.doc.link_at(pos) {
                // Internal links (cross-references) jump to their bookmark.
                match link.strip_prefix('#') {
                    Some(name) => match app.doc.bookmarks().into_iter().find(|b| b.0 == name) { Some((_, s, e)) => { app.doc.select(s, e); app.scroll_to_caret = true; } None => app.doc.select(pos, pos) },
                    None => { ui.ctx().open_url(egui::OpenUrl::new_tab(link)); app.doc.select(pos, pos); }
                }
                return;
            }
            let (a, b) = app.doc.sentence_at(pos);
            app.doc.select(a, b);
            app.drag = Drag::Select { unit: Unit::Char, anchor: (a, b) };
            return;
        }
        let range = unit_range(app, unit, pos);
        if modifiers.shift {
            let anchor = app.doc.selection.anchor;
            let (a, b) = if pos < anchor { (range.0, anchor) } else { (anchor, range.1) };
            if pos < anchor { app.doc.select(b, a) } else { app.doc.select(a, b) }
            app.drag = Drag::Select { unit, anchor: (anchor, anchor) };
        } else {
            app.doc.select(range.0, range.1);
            app.drag = Drag::Select { unit, anchor: range };
        }
        return;
    }

    if down {
        match app.drag {
            Drag::Select { unit, anchor } => {
                if let Some(pos) = app.layout.hit_test(page, x, y) {
                    let r = unit_range(app, unit, pos);
                    // Extend by whole units, keeping the first click's unit selected.
                    if r.0 < anchor.0 { app.doc.selection = word_engine::model::Selection { anchor: anchor.1, focus: r.0 }; }
                    else { app.doc.selection = word_engine::model::Selection { anchor: anchor.0, focus: r.1.max(anchor.1) }; }
                    app.scroll_to_caret = true;
                }
            }
            Drag::Pending { s, e, .. } if response.dragged() => app.drag = Drag::Move { s, e, target: None },
            Drag::Move { s, e, .. } => {
                let target = app.layout.hit_test(page, x, y);
                app.drag = Drag::Move { s, e, target };
                let copy = if cfg!(target_os = "macos") { modifiers.alt } else { modifiers.ctrl };
                ui.ctx().set_cursor_icon(if copy { CursorIcon::Copy } else { CursorIcon::Grabbing });
            }
            Drag::Resize { handle, .. } => ui.ctx().set_cursor_icon(HANDLE_CURSORS[handle]),
            Drag::Rotate { .. } => ui.ctx().set_cursor_icon(CursorIcon::Grabbing),
            Drag::Float { .. } => ui.ctx().set_cursor_icon(CursorIcon::Move),
            Drag::TableResize { .. } => ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe),
            Drag::TableMove { table, .. } if response.dragged() => {
                // Drop before the body paragraph under the pointer.
                let target = app.layout.hit_test(page, x, y).filter(|&p| app.doc.paragraphs[app.doc.locate(p).0].cell.is_none());
                let target = target.map(|p| app.doc.paragraph_bounds(p).0);
                app.drag = Drag::TableMove { table, target };
                ui.ctx().set_cursor_icon(CursorIcon::Move);
            }
            _ => {}
        }
    }

    if released {
        match app.drag {
            Drag::Pending { at, s, e } => { if e != s + 1 || (app.doc.image_at(s).is_none() && app.doc.object_at(s).is_none()) { app.doc.select(at, at); } }
            Drag::Move { s, e, target: Some(t) } => {
                // Option (macOS) or Ctrl drags a copy; a plain drag moves the text.
                let copy = if cfg!(target_os = "macos") { modifiers.alt } else { modifiers.ctrl };
                if !app.doc.move_range(s, e, t, copy) { app.doc.select(t.min(app.doc.len()), t.min(app.doc.len())); }
                app.scroll_to_caret = true;
            }
            Drag::Stroke { page: p0, x: x0, y: y0 } => draw_stroke(app, p0, x0, y0, page, x, y),
            Drag::TableMove { table, target } => match target {
                Some(t) => { if !app.doc.move_table(table, t) { app.notify(ui.ctx(), "Drop the table between paragraphs outside it"); } }
                // A click on the handle selects the table.
                None => {
                    let first = app.doc.tables[table].rows[0].cells[0].id;
                    if let Some(i) = app.doc.paragraphs.iter().position(|p| p.cell == Some(first)) { let s = app.doc.paragraph_start(i); app.doc.select(s, s); app.doc.select_table_part(word_engine::model::TablePart::Table); }
                }
            },
            Drag::TableResize { table, x: x0, width } => {
                let w = (width + x - x0).max(40.0);
                if (w - width).abs() > 1.0 { app.doc.scale_table(table, w / width); }
            }
            Drag::Column { table, col, x: bx } => {
                if let Some(t) = app.doc.tables.get(table) {
                    let width = t.grid.get(col).copied().unwrap_or(0) as f32 + (x - bx) * 15.0;
                    // Alt/Shift drags change the table width; a plain drag trades width with the next column.
                    let keep = !(modifiers.shift || modifiers.alt);
                    if (x - bx).abs() > 0.5 { app.doc.set_column_width(table, col, width.round() as i32, keep); }
                }
            }
            Drag::Float { pos, from, page: pg, x: bx, y: by } => {
                let (dx, dy) = ((pointer.x - from.x) / app.zoom, (pointer.y - from.y) / app.zoom);
                if dx.abs() > 1.0 || dy.abs() > 1.0 { drop_float(app, pos, pg, bx + dx, by + dy); }
                else { app.doc.select(pos, pos + 1); }
            }
            Drag::Resize { pos, handle, w, h, from: start } => {
                {
                    let (dx, dy) = rotate_about(((pointer.x - start.x) / app.zoom, (pointer.y - start.y) / app.zoom), (0.0, 0.0), -object_rotation(app, pos));
                    let (nw, nh) = resized(app, pos, handle, w, h, dx, dy, modifiers.shift);
                    if (nw - w).abs() > 0.5 || (nh - h).abs() > 0.5 { app.doc.resize_image(pos, nw, nh); }
                    app.doc.select(pos, pos + 1);
                }
            }
            Drag::Rotate { pos, center, base, from: start } => {
                let a = rotated_angle(center, start, pointer, base, modifiers.shift);
                if (a - base).abs() > 0.1 {
                    match app.doc.object_at(pos).cloned() {
                        Some(word_engine::objects::Object::Shape(mut sh)) => { sh.rotation = a; sh.raw = None; app.doc.set_object(pos, word_engine::objects::Object::Shape(sh)); }
                        _ => if app.doc.image_at(pos).is_some() { app.doc.set_image_rotation(pos, a); },
                    }
                }
                app.doc.select(pos, pos + 1);
            }
            _ => {}
        }
        // Format Painter: the copied formatting lands on the selection made with the mouse.
        if app.format_painter.is_some() && response.hovered() && !matches!(app.drag, Drag::None) {
            if app.doc.selection.is_caret() { let (a, b) = app.doc.word_at(app.doc.selection.focus); app.doc.select(a, b); }
            if let Some(props) = app.format_painter.take() { app.doc.set_run_props(props); }
        }
        app.drag = Drag::None;
    }
}

/// The table whose handles show: the one under the pointer, else the caret's.
fn handle_table(app: &WordApp, ui: &Ui, from_screen: &dyn Fn(Pos2) -> (usize, f32, f32)) -> Option<usize> {
    if let Drag::TableMove { table, .. } | Drag::TableResize { table, .. } = app.drag { return Some(table); }
    let hovered = ui.input(|i| i.pointer.hover_pos()).map(from_screen).and_then(|(pg, x, y)| {
        app.layout.cells.iter().find(|c| c.page == pg && x >= c.x0 - 20.0 && x <= c.x1 + 14.0 && y >= c.y0 - 20.0 && y <= c.y1 + 14.0).map(|c| c.table)
    });
    hovered.or_else(|| app.doc.current_cell().and_then(|c| app.doc.table_of_cell(c)))
}

/// A table's outer corners: (first page, left, top, last page, right, bottom).
fn table_bounds(app: &WordApp, table: usize) -> Option<(usize, f32, f32, usize, f32, f32)> {
    let cells: Vec<_> = app.layout.cells.iter().filter(|c| c.table == table).collect();
    let p0 = cells.iter().map(|c| c.page).min()?;
    let p1 = cells.iter().map(|c| c.page).max()?;
    let first: Vec<_> = cells.iter().filter(|c| c.page == p0).collect();
    let last: Vec<_> = cells.iter().filter(|c| c.page == p1).collect();
    Some((p0, first.iter().map(|c| c.x0).fold(f32::MAX, f32::min), first.iter().map(|c| c.y0).fold(f32::MAX, f32::min),
          p1, last.iter().map(|c| c.x1).fold(f32::MIN, f32::max), last.iter().map(|c| c.y1).fold(f32::MIN, f32::max)))
}

/// A pen stroke: inside a cell, a vertical stroke splits it into columns and a horizontal one into
/// rows; outside tables, a box stroke inserts a one-cell table of that width.
fn draw_stroke(app: &mut WordApp, p0: usize, x0: f32, y0: f32, p1: usize, x1: f32, y1: f32) {
    let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
    if p0 != p1 || dx.max(dy) < 6.0 { return; }
    let mid = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let cell = app.layout.cells.iter().find(|c| c.page == p0 && mid.0 > c.x0 && mid.0 < c.x1 && mid.1 > c.y0 && mid.1 < c.y1).cloned();
    match cell {
        Some(c) => {
            let start = app.doc.paragraphs.iter().position(|p| p.cell == Some(c.cell)).map(|i| app.doc.paragraph_start(i));
            let Some(s) = start else { return };
            app.doc.select(s, s);
            if dy > dx { app.doc.split_cell_at((mid.0 - c.x0) / (c.x1 - c.x0)); } else { app.doc.split_cell_rows(); }
        }
        None => {
            if let Some(pos) = app.layout.hit_test(p0, x0.min(x1), y0.min(y1)) {
                app.doc.select(pos, pos);
                app.doc.insert_table(1, 1);
                if let Some(ti) = app.doc.current_cell().and_then(|c| app.doc.table_of_cell(c)) {
                    let w = (dx * 15.0).round() as i32;
                    app.doc.set_column_width(ti, 0, w.max(720), false);
                    app.doc.set_row_height((dy * 15.0).round() as i32, false);
                }
            }
        }
    }
}

/// Two cells sharing the border at a point (for the eraser).
fn shared_border_at(app: &WordApp, page: usize, x: f32, y: f32) -> Option<(u64, u64)> {
    let tol = 4.0 / app.zoom.max(0.25);
    let cells: Vec<_> = app.layout.cells.iter().filter(|c| c.page == page).collect();
    for a in &cells {
        // Vertical border on a's right.
        if (x - a.x1).abs() <= tol && y > a.y0 && y < a.y1 {
            if let Some(b) = cells.iter().find(|b| b.table == a.table && (b.x0 - a.x1).abs() < 1.0 && y > b.y0 && y < b.y1) { return Some((a.cell, b.cell)); }
        }
        // Horizontal border below a.
        if (y - a.y1).abs() <= tol && x > a.x0 && x < a.x1 {
            if let Some(b) = cells.iter().find(|b| b.table == a.table && (b.y0 - a.y1).abs() < 1.0 && x > b.x0 && x < b.x1) { return Some((a.cell, b.cell)); }
        }
    }
    None
}

/// A table column's right border near a point: (table, grid column, border x).
fn column_border_at(app: &WordApp, page: usize, x: f32, y: f32) -> Option<(usize, usize, f32)> {
    let tol = 3.0 / app.zoom.max(0.25);
    app.layout.cells.iter().filter(|c| c.page == page && y >= c.y0 && y <= c.y1)
        .find(|c| (x - c.x1).abs() <= tol)
        .map(|c| (c.table, c.col + c.span - 1, c.x1))
}

/// The range a click of `unit` selects at `pos`.
fn unit_range(app: &WordApp, unit: Unit, pos: usize) -> (usize, usize) {
    match unit {
        Unit::Char => (pos, pos),
        Unit::Word => app.doc.word_at(pos),
        Unit::Paragraph => { let (a, b) = app.doc.paragraph_bounds(pos); (a, (b + 1).min(app.doc.len())) }
        Unit::Line => app.layout.line_index(pos).map(|i| { let l = &app.layout.lines[i]; (l.start, l.end) }).unwrap_or((pos, pos)),
    }
}

/// Whether a point lies in the selection bar left of the text of the nearest line.
fn in_left_margin(app: &WordApp, page: usize, x: f32, y: f32) -> bool {
    app.layout.lines.iter().filter(|l| l.page == page && l.cell.is_none() && y >= l.y && y <= l.y + l.height)
        .any(|l| x < l.stops.first().copied().unwrap_or(l.left).min(l.left) - 6.0)
}

/// Read Mode: the keyboard turns screens; text is read-only.
fn read_keys(app: &mut WordApp, ui: &Ui, columns: usize) {
    let rows = app.layout.pages.len().div_ceil(columns.max(1)).max(1);
    for e in ui.input(|i| i.events.clone()) {
        if let Event::Key { key, pressed: true, .. } = e {
            match key {
                Key::ArrowRight | Key::ArrowDown | Key::PageDown | Key::Space => { app.read_row = (app.read_row + 1).min(rows - 1); app.read_scroll = true; }
                Key::ArrowLeft | Key::ArrowUp | Key::PageUp => { app.read_row = app.read_row.saturating_sub(1); app.read_scroll = true; }
                Key::Home => { app.read_row = 0; app.read_scroll = true; }
                Key::End => { app.read_row = rows - 1; app.read_scroll = true; }
                Key::Escape => app.read_mode = false,
                _ => {}
            }
        }
    }
}

fn handle_keys(app: &mut WordApp, ui: &Ui) {
    if app.extras.story.is_some() && ui.input(|i| i.key_pressed(Key::Escape)) { app.end_story(); return; }
    if app.extras.draw.tool != crate::draw::Tool::Select && ui.input(|i| i.key_pressed(Key::Escape)) { app.extras.draw.tool = crate::draw::Tool::Select; app.extras.draw.selected.clear(); return; }
    if !app.extras.draw.selected.is_empty() && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) { app.edit_ink_selection(true, 0.0, 0.0); return; }
    let events = ui.input(|i| i.events.clone());
    let mac = cfg!(target_os = "macos");
    for event in events {
        let before = (app.doc.revision, app.doc.selection);
        match event {
            Event::Text(t) => {
                if t.chars().all(|c| !c.is_control()) && !t.is_empty() { app.doc.replace(&t); }
            }
            Event::Ime(egui::ImeEvent::Preedit { text, .. }) => app.preedit = text,
            Event::Ime(egui::ImeEvent::Commit(t)) => { app.preedit.clear(); if !t.is_empty() { app.doc.replace(&t); } }
            Event::Copy => { app.copy(ui.ctx()); }
            Event::Cut => app.cut(ui.ctx()),
            Event::Paste(t) => app.paste_text(&t),
            Event::Key { key, pressed: true, modifiers, .. } => key_press(app, ui, key, modifiers, mac),
            _ => {}
        }
        if (app.doc.revision, app.doc.selection) != before {
            app.scroll_to_caret = true;
            app.caret_blink = ui.input(|i| i.time);
            app.relayout();
        }
    }
}

fn key_press(app: &mut WordApp, ui: &Ui, key: Key, m: Modifiers, mac: bool) {
    let doc = &mut app.doc;
    let (s, e) = doc.range();
    let focus = doc.selection.focus;
    let word = if mac { m.alt } else { m.ctrl };
    let line = mac && m.command;
    // Move the focus, extending with Shift.
    // Arrows nudge a selected floating object (Ctrl/⌘ in fine steps).
    if e == s + 1 && matches!(key, Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown) && !m.shift {
        if let (Some(_), Some((pg, x, y, ..))) = (doc.anchor_at(s), app.layout.object_box(s)) {
            let step = if m.command || m.ctrl { 1.0 } else { 6.0 };
            let (dx, dy) = match key { Key::ArrowLeft => (-step, 0.0), Key::ArrowRight => (step, 0.0), Key::ArrowUp => (0.0, -step), _ => (0.0, step) };
            drop_float(app, s, pg, x + dx, y + dy);
            return;
        }
    }
    let go = |app: &mut WordApp, pos: usize, keep_goal: bool| {
        let anchor = if m.shift { app.doc.selection.anchor } else { pos };
        app.doc.select(anchor, pos);
        if !keep_goal { app.goal_x = None; }
    };
    match key {
        Key::ArrowLeft | Key::ArrowRight => {
            let fwd = key == Key::ArrowRight;
            // In right-to-left paragraphs → moves toward the start of the text.
            let rtl = app.layout.line_rtl(focus);
            let forward = fwd != rtl;
            let pos = if !m.shift && s != e && !word && !line { if forward { e } else { s } }
                else if line { let (a, b) = app.layout.line_bounds(focus).unwrap_or((focus, focus)); if forward { b } else { a } }
                else if word { app.doc.word_boundary(focus, forward) }
                else {
                    // Visual movement in mixed-direction lines, by grapheme cluster elsewhere.
                    app.layout.visual_step(focus, fwd).unwrap_or_else(|| app.doc.next_grapheme(focus, forward).min(app.doc.len()))
                };
            go(app, pos, false);
        }
        Key::ArrowUp | Key::ArrowDown if if mac { m.ctrl && m.shift } else { m.alt && m.shift } => {
            // Move Paragraph Up/Down.
            app.doc.move_paragraphs(key == Key::ArrowUp);
        }
        Key::ArrowUp | Key::ArrowDown => {
            let down = key == Key::ArrowDown;
            if line { go(app, if down { app.doc.len() } else { 0 }, false); return; }
            if !m.shift && s != e { go(app, if down { e } else { s }, false); return; }
            let goal = app.goal_x.or_else(|| app.layout.caret_rect(focus).map(|c| c.x)).unwrap_or(0.0);
            let pos = app.layout.vertical(focus, down, goal).unwrap_or(if down { app.doc.len() } else { 0 });
            go(app, pos, true);
            app.goal_x = Some(goal);
        }
        Key::Home | Key::End => {
            let end = key == Key::End;
            let pos = if m.command { if end { app.doc.len() } else { 0 } } else { let (a, b) = app.layout.line_bounds(focus).unwrap_or((focus, focus)); if end { b } else { a } };
            go(app, pos, false);
        }
        Key::PageUp | Key::PageDown => {
            let down = key == Key::PageDown;
            if let Some(c) = app.layout.caret_rect(focus) {
                let target_y = c.y + if down { 700.0 } else { -700.0 };
                let mut page = c.page as i64;
                let mut y = target_y;
                let h = app.layout.pages.get(c.page).map(|p| p.height).unwrap_or(1056.0);
                if y > h { page += 1; y -= h; } else if y < 0.0 { page -= 1; y += h; }
                let page = page.clamp(0, app.layout.pages.len() as i64 - 1) as usize;
                if let Some(pos) = app.layout.hit_test(page, c.x, y) { go(app, pos, false); }
            }
        }
        Key::Backspace | Key::Delete => {
            app.doc.delete(key == Key::Delete, word || (mac && m.command && key == Key::Backspace && false));
            if mac && m.command && key == Key::Backspace && s == e {
                // Cmd+Backspace deletes to the line start.
                if let Some((a, _)) = app.layout.line_bounds(focus) { app.doc.select(a, focus); app.doc.replace(""); }
            }
        }
        Key::Enter => {
            if m.shift { app.doc.insert_line_break(); }
            else if m.command { app.doc.insert_page_break(); }
            else if app.doc.current_para_format().numbering.is_some() && app.doc.current_paragraph().is_empty() {
                // Enter on an empty list item ends the list.
                app.doc.toggle_list(app.doc.numbering.kind(app.doc.current_para_format().numbering.unwrap().0).unwrap_or(word_engine::numbering::ListKind::Bullet));
            } else { app.doc.insert_paragraph(); }
        }
        Key::Tab => {
            if app.doc.current_cell().is_some() && !m.command { app.doc.move_cell(!m.shift); }
            else if app.doc.current_para_format().numbering.is_some() && (app.doc.locate(s).1 == 0 || s != e) { app.doc.change_list_level(if m.shift { -1 } else { 1 }); }
            else if !m.shift { app.doc.insert_tab(); }
        }
        Key::Escape => {
            if app.table_tool.is_some() { app.table_tool = None; }
            else if app.find.open { app.find.open = false; }
            else if s != e { go(app, focus, false); }
        }
        _ => { let _ = ui; }
    }
}

/// Fill any simple polygon (convex or not) by ear clipping.
fn fill_polygon(pts: &[Pos2], color: Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    if pts.len() < 3 { return mesh; }
    for p in pts { mesh.colored_vertex(*p, color); }
    let area: f32 = (0..pts.len()).map(|i| { let (a, b) = (pts[i], pts[(i + 1) % pts.len()]); a.x * b.y - b.x * a.y }).sum();
    let ccw = area > 0.0;
    let mut idx: Vec<u32> = (0..pts.len() as u32).collect();
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let inside = |p: Pos2, a: Pos2, b: Pos2, c: Pos2| { let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p)); !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0)) };
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = idx.len();
        let mut clipped = false;
        for i in 0..n {
            let (ia, ib, ic) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            let (a, b, c) = (pts[ia as usize], pts[ib as usize], pts[ic as usize]);
            let convex = if ccw { cross(a, b, c) > 0.0 } else { cross(a, b, c) < 0.0 };
            if !convex { continue; }
            if idx.iter().any(|&j| j != ia && j != ib && j != ic && inside(pts[j as usize], a, b, c)) { continue; }
            mesh.add_triangle(ia, ib, ic);
            idx.remove(i);
            clipped = true;
            break;
        }
        // Degenerate input (collinear points): fan the rest.
        if !clipped { for k in 1..idx.len() - 1 { mesh.add_triangle(idx[0], idx[k], idx[k + 1]); } return mesh; }
    }
    if idx.len() == 3 { mesh.add_triangle(idx[0], idx[1], idx[2]); }
    mesh
}

/// Paint draw items into a dialog preview at `origin`, scaled (chart and diagram editors).
pub fn paint_preview(app: &mut WordApp, ui: &Ui, items: &[DrawItem], origin: Pos2, scale: f32) {
    let painter = ui.painter().clone();
    let ts = |_: usize, x: f32, y: f32| pos2(origin.x + x * scale, origin.y + y * scale);
    let ppp = ui.ctx().pixels_per_point();
    let mut mesh = egui::Mesh::with_texture(app.atlas.texture_id().unwrap_or_default());
    {
        let (textures, assets, atlas) = (&mut app.textures, &app.doc.assets, &mut app.atlas);
        let mut pen = Pen { atlas, mesh: &mut mesh, ppp };
        for it in items { paint_item(textures, assets, ui, &painter, it, 0, scale, &ts, &mut pen); }
    }
    painter.add(egui::Shape::mesh(mesh));
}
