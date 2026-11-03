//! Page thumbnails: the side list and the organize grid with drag-to-reorder.

use crate::i18n::{n_, tr, trf};
use eframe::egui::{self, Color32, Pos2, Stroke, pos2, vec2};

use crate::app::{App, ToolSet};
use crate::raster::Purpose;
use crate::theme::{self, palette};

#[derive(Default)]
pub struct OrganizeState {
    /// Where dragged pages would land (index into the page list), while dragging.
    drop_at: Option<usize>,
    dragging: bool,
}

/// Fits a page of `size` points into `bounds`, centred; returns the screen rect.
fn fit(bounds: egui::Rect, size: (f32, f32)) -> egui::Rect {
    let s = (bounds.width() / size.0).min(bounds.height() / size.1);
    egui::Rect::from_center_size(bounds.center(), vec2(size.0 * s, size.1 * s))
}

fn paint_thumb(app: &mut App, ui: &egui::Ui, page: usize, rect: egui::Rect, dark: bool) {
    let painter = ui.painter();
    painter.rect_filled(
        rect.translate(vec2(0.0, 1.0)).expand(1.0),
        2.0,
        Color32::from_black_alpha(if dark { 90 } else { 30 }),
    );
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let Some(doc) = &app.doc else { return };
    let (w, _) = doc.page_size(page);
    // One thumbnail resolution for every size keeps the cache small.
    let scale = 180.0 / w.max(1.0) * ui.ctx().pixels_per_point();
    if let Some(tex) = app.raster.texture(ui.ctx(), page, Purpose::Thumb, scale) {
        painter.image(
            tex,
            rect,
            egui::Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

/// The "Pages" side panel: a column of thumbnails; click to go to a page.
pub fn thumbnail_list(app: &mut App, ui: &mut egui::Ui) {
    let dark = ui.visuals().dark_mode;
    let p = palette(dark);
    let Some(doc) = &app.doc else { return };
    let n = doc.page_count();
    let sizes: Vec<(f32, f32)> = (0..n).map(|i| doc.page_size(i)).collect();
    ui.label(egui::RichText::new(tr("Pages")).font(theme::bold(14.0)));
    ui.add_space(4.0);
    let current = app.current_page;
    let mut go = None;
    egui::ScrollArea::vertical()
        .id_salt("thumbs")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let width = ui.available_width();
            for (i, size) in sizes.iter().enumerate() {
                let h = (width - 24.0) / size.0 * size.1;
                let (cell, response) =
                    ui.allocate_exact_size(vec2(width, h.min(220.0) + 30.0), egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::Button,
                        true,
                        i == current,
                        trf("Thumbnail of page {}", &[&(i + 1)]),
                    )
                });
                if !ui.is_rect_visible(cell) {
                    continue;
                }
                let bounds = egui::Rect::from_min_max(
                    cell.min + vec2(12.0, 4.0),
                    pos2(cell.right() - 12.0, cell.bottom() - 24.0),
                );
                let thumb = fit(bounds, *size);
                paint_thumb(app, ui, i, thumb, dark);
                if i == current {
                    ui.painter().rect_stroke(
                        thumb.expand(3.0),
                        4.0,
                        Stroke::new(2.0, p.accent),
                        egui::StrokeKind::Outside,
                    );
                } else if response.hovered() {
                    ui.painter().rect_stroke(
                        thumb.expand(3.0),
                        4.0,
                        Stroke::new(1.0, p.border_strong),
                        egui::StrokeKind::Outside,
                    );
                }
                ui.painter().text(
                    pos2(cell.center().x, cell.bottom() - 11.0),
                    egui::Align2::CENTER_CENTER,
                    (i + 1).to_string(),
                    egui::FontId::proportional(12.0),
                    if i == current { p.accent_text } else { p.muted },
                );
                if response.clicked() {
                    go = Some(i);
                }
            }
        });
    if let Some(i) = go {
        app.go_to(i);
    }
}

/// The organize view: every page in a grid; select, drag to reorder, right-click for actions.
pub fn grid(app: &mut App, ui: &mut egui::Ui) {
    let dark = ui.visuals().dark_mode;
    let p = palette(dark);
    let Some(doc) = &app.doc else { return };
    let n = doc.page_count();
    let sizes: Vec<(f32, f32)> = (0..n).map(|i| doc.page_size(i)).collect();
    let cell = vec2(168.0, 220.0);
    let mut clicked: Option<(usize, egui::Modifiers)> = None;
    let mut open: Option<usize> = None;
    let mut context: Option<(usize, &'static str)> = None;
    let mut cells: Vec<egui::Rect> = Vec::with_capacity(n);

    egui::ScrollArea::vertical()
        .id_salt("organize")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let width = ui.available_width() - 32.0;
            let cols = ((width / cell.x).floor() as usize).max(1);
            let used = cols as f32 * cell.x;
            let left = ui.max_rect().left() + 16.0 + (width - used).max(0.0) / 2.0;
            let top = ui.max_rect().top() + 20.0;
            let rows = n.div_ceil(cols);
            ui.allocate_space(vec2(width, rows as f32 * cell.y + 60.0));
            for (i, size) in sizes.iter().enumerate() {
                let (r, c) = (i / cols, i % cols);
                let rect = egui::Rect::from_min_size(
                    pos2(left + c as f32 * cell.x, top + r as f32 * cell.y),
                    cell,
                );
                cells.push(rect);
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let selected = app.selection.contains(&i);
                let response = ui.interact(
                    rect,
                    ui.scope_id().with(("cell", i)),
                    egui::Sense::click_and_drag(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::CheckBox,
                        true,
                        selected,
                        trf("Page {}", &[&(i + 1)]),
                    )
                });
                let bounds = egui::Rect::from_min_max(
                    rect.min + vec2(14.0, 10.0),
                    pos2(rect.right() - 14.0, rect.bottom() - 34.0),
                );
                let thumb = fit(bounds, *size);
                if selected {
                    ui.painter()
                        .rect_filled(rect.shrink(4.0), 10.0, p.accent_soft);
                } else if response.hovered() {
                    ui.painter()
                        .rect_filled(rect.shrink(4.0), 10.0, p.control_hover);
                }
                let faded = app.organize.dragging && selected;
                paint_thumb(app, ui, i, thumb, dark);
                if faded {
                    ui.painter()
                        .rect_filled(thumb, 0.0, Color32::from_white_alpha(140));
                }
                if selected {
                    ui.painter().rect_stroke(
                        thumb.expand(2.0),
                        3.0,
                        Stroke::new(2.0, p.accent),
                        egui::StrokeKind::Outside,
                    );
                }
                ui.painter().text(
                    pos2(rect.center().x, rect.bottom() - 20.0),
                    egui::Align2::CENTER_CENTER,
                    (i + 1).to_string(),
                    theme::bold(12.0),
                    if selected { p.accent_text } else { p.muted },
                );
                if response.clicked() {
                    clicked = Some((i, ui.input(|inp| inp.modifiers)));
                }
                if response.double_clicked() {
                    open = Some(i);
                }
                if response.drag_started() {
                    if !selected {
                        clicked = Some((i, egui::Modifiers::NONE));
                    }
                    app.organize.dragging = true;
                }
                response.context_menu(|ui| {
                    if !selected {
                        clicked = Some((i, egui::Modifiers::NONE));
                    }
                    for label in [
                        n_("Rotate right"),
                        n_("Rotate left"),
                        n_("Duplicate"),
                        n_("Insert blank page after"),
                        n_("Extract…"),
                        n_("Delete"),
                    ] {
                        if ui.button(tr(label)).clicked() {
                            context = Some((i, label));
                        }
                    }
                });
            }

            // Drop indicator while dragging.
            if app.organize.dragging {
                let pointer = ui.input(|i| i.pointer.hover_pos());
                app.organize.drop_at = pointer.map(|ptr| drop_index(&cells, ptr));
                if let Some(at) = app.organize.drop_at {
                    let x_rect = if at < cells.len() {
                        let r = cells[at];
                        egui::Rect::from_min_max(
                            pos2(r.left() - 2.0, r.top() + 10.0),
                            pos2(r.left() + 2.0, r.bottom() - 30.0),
                        )
                    } else if let Some(r) = cells.last() {
                        egui::Rect::from_min_max(
                            pos2(r.right() - 2.0, r.top() + 10.0),
                            pos2(r.right() + 2.0, r.bottom() - 30.0),
                        )
                    } else {
                        egui::Rect::NOTHING
                    };
                    ui.painter().rect_filled(x_rect, 2.0, p.accent);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
            }
        });

    if app.organize.dragging && ui.input(|i| i.pointer.any_released()) {
        app.organize.dragging = false;
        if let Some(at) = app.organize.drop_at.take() {
            let sel: Vec<usize> = app.selection.iter().copied().collect();
            // Dropping a contiguous block onto itself changes nothing.
            let noop = sel.first().is_some_and(|&f| {
                at >= f && at <= f + sel.len() && sel.windows(2).all(|w| w[1] == w[0] + 1)
            });
            if !noop {
                app.move_selection(at);
            }
        }
    }

    if let Some((i, m)) = clicked {
        if m.shift
            && let Some(a) = app.anchor
        {
            app.selection = (a.min(i)..=a.max(i)).collect();
        } else if m.command {
            if !app.selection.remove(&i) {
                app.selection.insert(i);
            }
            app.anchor = Some(i);
        } else {
            app.selection = [i].into();
            app.anchor = Some(i);
        }
        app.current_page = i;
    }
    if let Some(i) = open {
        app.toolset = ToolSet::View;
        app.go_to(i);
    }
    if let Some((_, label)) = context {
        match label {
            "Rotate right" => app.rotate(90),
            "Rotate left" => app.rotate(-90),
            "Duplicate" => app.duplicate(),
            "Insert blank page after" => app.insert_blank(),
            "Extract…" => app.extract(),
            _ => app.delete_pages(),
        }
    }
    let hint = if n == 1 {
        "1 page".to_string()
    } else {
        format!("{n} pages")
    };
    let r = ui.max_rect();
    ui.painter().text(
        pos2(r.left() + 16.0, r.top() + 10.0),
        egui::Align2::LEFT_CENTER,
        hint,
        egui::FontId::proportional(11.0),
        p.muted,
    );
}

/// The gap nearest to the pointer: before the cell under it, or after it on its right half.
fn drop_index(cells: &[egui::Rect], ptr: Pos2) -> usize {
    let mut best = (0usize, f32::MAX);
    for (i, r) in cells.iter().enumerate() {
        let d = r.center().distance(ptr);
        if d < best.1 {
            best = (i, d);
        }
    }
    let (i, _) = best;
    match cells.get(i) {
        Some(r) if ptr.x > r.center().x => i + 1,
        _ => i,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_index_picks_gap() {
        let cells: Vec<egui::Rect> = (0..3)
            .map(|i| egui::Rect::from_min_size(pos2(i as f32 * 100.0, 0.0), vec2(100.0, 100.0)))
            .collect();
        assert_eq!(drop_index(&cells, pos2(10.0, 50.0)), 0);
        assert_eq!(drop_index(&cells, pos2(90.0, 50.0)), 1);
        assert_eq!(drop_index(&cells, pos2(290.0, 50.0)), 3);
    }
}
