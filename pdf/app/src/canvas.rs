//! The page view: continuous pages, zoom, tools on the page, floating controls.

use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, CursorIcon, Pos2, Stroke, pos2, vec2};
use pdf_engine::{ItemEdit, ItemKind, NewAnnot, NewItem, Point, Rect};

use crate::app::{App, DraftTarget, Picked, TextDraft, Tool, ToolSet, to_color32};
use crate::editing;
use crate::raster::Purpose;
use crate::theme::{self, Icon, palette};

const MARGIN: f32 = 24.0;
const GAP: f32 = 16.0;

#[derive(Default)]
pub struct CanvasState {
    /// Scroll so this page rectangle (view points) is visible.
    pub reveal: Option<(usize, Rect)>,
    drag: Option<Drag>,
    page_input: String,
    page_input_focused: bool,
}

struct Drag {
    page: usize,
    start: Point,
    current: Point,
    mode: DragMode,
}

enum DragMode {
    /// Text selection from glyph `a` to glyph `b`.
    Text {
        a: usize,
        b: usize,
    },
    Area,
    Ink(Vec<Point>),
    Pan,
    Move {
        picked: Picked,
        from: Rect,
    },
    Resize {
        picked: Picked,
        from: Rect,
        handle: usize,
    },
}

enum Action {
    FillAt(usize, Point),
    CreateField(usize, pdf_engine::FieldKind, Rect),
    MarkText(usize, usize, usize),
    MarkArea(usize, Rect),
    Add(usize, NewAnnot),
    AddItem(usize, NewItem),
    Pick(Option<Picked>),
    SetRect(Picked, Rect, Rect),
    Draft(TextDraft),
    Notify(String),
}

fn to_view(p: Pos2, page: egui::Rect, zoom: f32) -> Point {
    Point::new((p.x - page.left()) / zoom, (p.y - page.top()) / zoom)
}

fn to_screen(r: Rect, page: egui::Rect, zoom: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        page.min + vec2(r.x0, r.y0) * zoom,
        page.min + vec2(r.x1, r.y1) * zoom,
    )
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(doc) = app.doc.as_ref() else { return };
    let dark = ui.visuals().dark_mode;
    let p = palette(dark);
    let avail = ui.available_rect_before_wrap();
    let sizes: Vec<(f32, f32)> = (0..doc.page_count()).map(|i| doc.page_size(i)).collect();
    let max_w = sizes.iter().map(|s| s.0).fold(1.0, f32::max);

    if app.fit_width {
        app.zoom = ((avail.width() - 2.0 * MARGIN - 8.0) / max_w).clamp(0.1, 8.0);
    }

    // Ctrl/Cmd + wheel or pinch: zoom around the pointer.
    let (zoom_delta, pointer) = ui.input(|i| (i.zoom_delta(), i.pointer.hover_pos()));
    if zoom_delta != 1.0
        && let Some(ptr) = pointer.filter(|p| avail.contains(*p))
    {
        let old = app.zoom;
        let new = (old * zoom_delta).clamp(0.1, 8.0);
        let local = ptr - avail.min;
        let anchor = app.scroll_offset + local;
        app.pending_offset = Some(anchor * (new / old) - local);
        app.zoom = new;
        app.fit_width = false;
    }
    let zoom = app.zoom;

    // Lay pages out top to bottom, centred.
    let content_w = avail.width().max(max_w * zoom + 2.0 * MARGIN);
    let mut y = MARGIN;
    let rects: Vec<egui::Rect> = sizes
        .iter()
        .map(|&(w, h)| {
            let r = egui::Rect::from_min_size(
                pos2((content_w - w * zoom) / 2.0, y),
                vec2(w * zoom, h * zoom),
            );
            y += h * zoom + GAP;
            r
        })
        .collect();
    let content = vec2(content_w, y + MARGIN + 48.0);

    if let Some(page) = app.scroll_to_page.take()
        && let Some(r) = rects.get(page)
    {
        app.pending_offset = Some(vec2(app.scroll_offset.x, r.top() - 12.0));
    }
    if let Some((page, r)) = app.canvas.reveal.take()
        && let Some(pr) = rects.get(page)
    {
        let target = to_screen(r, *pr, zoom);
        let view = egui::Rect::from_min_size(app.scroll_offset.to_pos2(), avail.size());
        if !view.shrink(40.0).contains_rect(target) {
            app.pending_offset = Some(vec2(
                (target.center().x - avail.width() / 2.0).max(0.0),
                (target.top() - avail.height() * 0.3).max(0.0),
            ));
        }
    }

    let mut area = egui::ScrollArea::both()
        .id_salt("canvas")
        .auto_shrink([false, false]);
    if let Some(o) = app.pending_offset.take() {
        area = area.scroll_offset(vec2(o.x.max(0.0), o.y.max(0.0)));
    }
    let mut actions = Vec::new();
    let out = area.show_viewport(ui, |ui, viewport| {
        ui.set_min_size(content);
        let origin = ui.max_rect().min;
        for (i, r) in rects.iter().enumerate() {
            if !r.intersects(viewport.expand(400.0)) {
                continue;
            }
            let screen = r.translate(origin.to_vec2());
            page_view(app, ui, i, screen, dark, &mut actions);
        }
    });
    app.scroll_offset = out.state.clamped_offset();
    let origin = out.inner_rect.min - out.state.clamped_offset();

    // The page under the middle of the view is the current page.
    if app.scroll_to_page.is_none() {
        let mid = out.state.clamped_offset().y + out.inner_rect.height() * 0.4;
        if let Some(i) = rects.iter().position(|r| r.bottom() + GAP >= mid) {
            app.current_page = i;
        }
    }

    for a in actions {
        match a {
            Action::Add(page, annot) => app.apply(true, |d| d.add_annotation(page, &annot)),
            Action::AddItem(page, item) => editing::add_item(app, page, item),
            Action::Pick(p) => app.picked = p,
            Action::SetRect(p, from, to) => editing::set_rect(app, p, from, to),
            Action::Draft(d) => app.draft = Some(d),
            Action::Notify(m) => app.notify(m),
            Action::FillAt(page, at) => crate::forms::click(app, page, at),
            Action::CreateField(page, kind, rect) => crate::forms::create(app, page, kind, rect),
            Action::MarkText(page, a, b) => crate::redact::mark_text(app, page, a, b),
            Action::MarkArea(page, r) => app.redact.add(page, r, String::new()),
        }
    }

    draft_editor(app, ui.ctx(), &rects, origin, zoom);
    let page_screen = |i: usize| rects.get(i).map(|r| r.translate(origin.to_vec2()));
    crate::forms::choice_popup(app, ui.ctx(), &page_screen, zoom);
    if app.toolset == ToolSet::Forms && app.draft.is_none() {
        crate::forms::keyboard(app, ui.ctx());
    }
    floating_controls(app, ui, out.inner_rect, rects.len(), p);
}

fn page_view(
    app: &mut App,
    ui: &mut egui::Ui,
    index: usize,
    screen: egui::Rect,
    dark: bool,
    actions: &mut Vec<Action>,
) {
    let p = palette(dark);
    let zoom = app.zoom;
    let painter = ui.painter().clone();
    // Shadow and paper.
    painter.rect_filled(
        screen.translate(vec2(0.0, 2.0)).expand(1.0),
        2.0,
        Color32::from_black_alpha(if dark { 90 } else { 28 }),
    );
    painter.rect_filled(screen, 0.0, Color32::WHITE);
    let ppp = ui.ctx().pixels_per_point();
    if let Some(tex) = app
        .raster
        .texture(ui.ctx(), index, Purpose::Page, zoom * ppp)
    {
        painter.image(
            tex,
            screen,
            egui::Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    // Search matches.
    for (k, hit) in app.search.hits.iter().enumerate() {
        if hit.page != index {
            continue;
        }
        let fill = if k == app.search.current {
            p.find_current
        } else {
            p.find
        };
        for r in &hit.rects {
            painter.rect_filled(to_screen(*r, screen, zoom).expand(1.0), 2.0, fill);
        }
    }

    if app.toolset == ToolSet::Forms {
        crate::forms::paint_fields(app, &painter, index, screen, zoom);
        if let Some(name) = &app.forms.focused
            && let Some(f) = app
                .doc
                .as_ref()
                .and_then(|d| d.fields().into_iter().find(|f| &f.name == name))
        {
            for w in f.widgets.iter().filter(|w| w.page == index) {
                painter.rect_stroke(
                    to_screen(w.rect, screen, zoom).expand(2.0),
                    2.0,
                    Stroke::new(2.0, p.accent),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }
    crate::redact::paint_marks(app, &painter, index, screen, zoom);
    let tool = match app.toolset {
        ToolSet::Comment | ToolSet::Edit | ToolSet::Sign | ToolSet::Forms | ToolSet::Redact => {
            app.tool
        }
        _ => Tool::Select,
    };
    // Selecting objects is part of the editing tool sets; View only pans.
    let selecting = tool == Tool::Select && app.toolset != ToolSet::View;
    let id = ui.scope_id().with(("page", index));
    let response = ui.interact(screen, id, egui::Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Canvas, true, trf("Page {}", &[&(index + 1)]))
    });
    let color = app.tool_color();
    let c32 = to_color32(color);
    let hover = response.hover_pos().map(|p| to_view(p, screen, zoom));

    // The current selection, unless it is being dragged.
    let picked_shape = app
        .picked
        .filter(|p| p.page() == index)
        .and_then(|p| editing::shape(app, p).map(|s| (p, s)));
    let dragging_picked = app
        .canvas
        .drag
        .as_ref()
        .is_some_and(|d| matches!(d.mode, DragMode::Move { .. } | DragMode::Resize { .. }));
    if let Some((_, s)) = &picked_shape
        && !dragging_picked
    {
        let r = to_screen(s.bounds(), screen, zoom).expand(3.0);
        painter.rect_stroke(
            r,
            2.0,
            Stroke::new(1.5, p.accent),
            egui::StrokeKind::Outside,
        );
        if s.resizable() && selecting {
            for h in handles(s.bounds()) {
                draw_handle(&painter, screen.min + vec2(h.x, h.y) * zoom, &p);
            }
        }
    }

    // Hover feedback and cursors.
    if response.hovered()
        && let Some(at) = hover
    {
        let mut cursor = match tool {
            Tool::Select if !selecting => CursorIcon::Grab,
            Tool::Select => CursorIcon::Default,
            Tool::Highlight
            | Tool::Underline
            | Tool::Strike
            | Tool::TextBox
            | Tool::AddText
            | Tool::EditText => CursorIcon::Text,
            _ => CursorIcon::Crosshair,
        };
        if selecting {
            if let Some((_, s)) = &picked_shape
                && s.resizable()
                && let Some(h) = handle_at(s.bounds(), at, 6.0 / zoom)
            {
                cursor = [
                    CursorIcon::ResizeNwSe,
                    CursorIcon::ResizeNeSw,
                    CursorIcon::ResizeNwSe,
                    CursorIcon::ResizeNeSw,
                    CursorIcon::ResizeVertical,
                    CursorIcon::ResizeHorizontal,
                    CursorIcon::ResizeVertical,
                    CursorIcon::ResizeHorizontal,
                ][h];
            } else if let Some(hit) = hit_test(app, index, at) {
                cursor = CursorIcon::Move;
                if Some(hit) != app.picked
                    && let Some(s) = editing::shape(app, hit)
                {
                    painter.rect_stroke(
                        to_screen(s.bounds(), screen, zoom).expand(2.0),
                        2.0,
                        Stroke::new(1.0, p.accent.gamma_multiply(0.6)),
                        egui::StrokeKind::Outside,
                    );
                }
                if let Picked::Annot { slot, .. } = hit
                    && let Some(a) = app
                        .doc
                        .as_ref()
                        .and_then(|d| d.annotations(index).into_iter().find(|a| a.slot == slot))
                    && !a.contents.is_empty()
                {
                    response
                        .clone()
                        .on_hover_text(format!("{}\n{}", a.kind.label(), a.contents));
                }
            }
        }
        if app.canvas.drag.is_none() {
            ui.ctx().set_cursor_icon(cursor);
        }
    }

    if response.double_clicked()
        && selecting
        && let Some(at) = hover
        && let Some(hit) = hit_test(app, index, at)
    {
        app.picked = Some(hit);
        editing::edit_text_of(app, hit);
    } else if response.clicked()
        && let Some(ptr) = response.interact_pointer_pos()
    {
        let at = to_view(ptr, screen, zoom);
        match tool {
            Tool::Select if selecting => actions.push(Action::Pick(hit_test(app, index, at))),
            Tool::Fill => actions.push(Action::FillAt(index, at)),
            Tool::Field(kind) => actions.push(Action::CreateField(
                index,
                kind,
                Rect::new(at.x, at.y, at.x, at.y),
            )),
            Tool::TextBox => actions.push(Action::Draft(TextDraft {
                page: index,
                rect: Rect::new(
                    at.x,
                    at.y - app.font_size * 0.6,
                    at.x + 200.0,
                    at.y + app.font_size * 1.4,
                ),
                target: DraftTarget::NewTextBox,
                text: String::new(),
                focus: true,
            })),
            Tool::Note => actions.push(Action::Draft(TextDraft {
                page: index,
                rect: Rect::new(at.x - 10.0, at.y - 10.0, at.x + 10.0, at.y + 10.0),
                target: DraftTarget::NewNote,
                text: String::new(),
                focus: true,
            })),
            Tool::AddText => {
                let size = app.text_size;
                actions.push(Action::Draft(TextDraft {
                    page: index,
                    rect: Rect::new(
                        at.x,
                        at.y - size * 0.7,
                        at.x + size * 8.0,
                        at.y + size * 0.6,
                    ),
                    target: DraftTarget::NewText,
                    text: String::new(),
                    focus: true,
                }));
            }
            Tool::EditText => match app.doc.as_ref().map(|d| d.text_line_at(index, at)) {
                Some(Ok(line)) => actions.push(Action::Draft(TextDraft {
                    page: index,
                    rect: line.rect,
                    text: line.text.clone(),
                    target: DraftTarget::Line { at, line },
                    focus: true,
                })),
                Some(Err(msg)) => actions.push(Action::Notify(msg)),
                None => {}
            },
            Tool::Place(stamp) => match crate::signatures::stamp_item(&app.signatures, stamp, at) {
                Some(item) => actions.push(Action::AddItem(index, item)),
                None => actions.push(Action::Notify(tr("Create a signature first."))),
            },
            _ => {}
        }
    }

    let doc = app.doc.as_ref().expect("document");
    if response.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        let start = to_view(origin, screen, zoom);
        let mode = match tool {
            Tool::Select if selecting => {
                let on_handle = picked_shape.as_ref().and_then(|(pk, s)| {
                    (s.resizable())
                        .then(|| {
                            handle_at(s.bounds(), start, 6.0 / zoom).map(|h| (*pk, s.bounds(), h))
                        })
                        .flatten()
                });
                if let Some((picked, from, handle)) = on_handle {
                    DragMode::Resize {
                        picked,
                        from,
                        handle,
                    }
                } else if let Some(hit) = hit_test(app, index, start)
                    && editing::shape(app, hit).is_some_and(|s| s.movable())
                {
                    let from = editing::shape(app, hit)
                        .map(|s| s.bounds())
                        .unwrap_or_default();
                    actions.push(Action::Pick(Some(hit)));
                    DragMode::Move { picked: hit, from }
                } else {
                    DragMode::Pan
                }
            }
            Tool::Select => DragMode::Pan,
            Tool::Highlight | Tool::Underline | Tool::Strike => {
                match doc.page_text(index).glyph_at(start, 6.0) {
                    Some(g) => DragMode::Text { a: g, b: g },
                    None => DragMode::Area,
                }
            }
            Tool::Ink => DragMode::Ink(vec![start]),
            Tool::RedactText => match doc.page_text(index).glyph_at(start, 6.0) {
                Some(g) => DragMode::Text { a: g, b: g },
                None => DragMode::Area,
            },
            Tool::RedactArea | Tool::Field(_) => DragMode::Area,
            Tool::TextBox | Tool::Rectangle | Tool::Ellipse => DragMode::Area,
            _ => DragMode::Pan,
        };
        app.canvas.drag = Some(Drag {
            page: index,
            start,
            current: start,
            mode,
        });
    }

    let Some(drag) = app.canvas.drag.as_mut().filter(|d| d.page == index) else {
        return;
    };
    if response.dragged()
        && let Some(ptr) = response.interact_pointer_pos()
    {
        drag.current = to_view(ptr, screen, zoom);
        match &mut drag.mode {
            DragMode::Text { b, .. } => {
                if let Some(g) = doc.page_text(index).glyph_at(drag.current, 14.0) {
                    *b = g;
                }
            }
            DragMode::Ink(points) => {
                if points.last().is_none_or(|l| {
                    (l.x - drag.current.x).abs() + (l.y - drag.current.y).abs() > 1.0 / zoom
                }) {
                    points.push(drag.current);
                }
            }
            DragMode::Pan => {
                let delta = response.drag_delta();
                app.pending_offset = Some(app.scroll_offset - delta);
            }
            _ => {}
        }
    }

    // Live preview.
    let area = Rect::from_points(drag.start, drag.current);
    let (dx, dy) = (drag.current.x - drag.start.x, drag.current.y - drag.start.y);
    let fill = Color32::from_rgba_unmultiplied(c32.r(), c32.g(), c32.b(), 90);
    let stroke = Stroke::new((app.stroke * zoom).max(1.0), c32);
    let mut target_rect = None;
    match (&drag.mode, tool) {
        (DragMode::Text { a, b }, _) => {
            for r in doc.page_text(index).line_rects(*a, *b) {
                painter.rect_filled(to_screen(r, screen, zoom), 1.0, fill);
            }
        }
        (DragMode::Ink(points), _) => {
            let pts: Vec<Pos2> = points
                .iter()
                .map(|q| screen.min + vec2(q.x, q.y) * zoom)
                .collect();
            painter.add(egui::Shape::line(pts, stroke));
        }
        (DragMode::Move { from, .. }, _) => {
            target_rect = Some(Rect::new(
                from.x0 + dx,
                from.y0 + dy,
                from.x1 + dx,
                from.y1 + dy,
            ))
        }
        (DragMode::Resize { from, handle, .. }, _) => {
            target_rect = Some(resize(*from, *handle, dx, dy))
        }
        (DragMode::Area, Tool::Rectangle) => {
            painter.rect_stroke(
                to_screen(area, screen, zoom),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        (DragMode::Area, Tool::Ellipse) => {
            let r = to_screen(area, screen, zoom);
            painter.add(egui::Shape::ellipse_stroke(
                r.center(),
                r.size() / 2.0,
                stroke,
            ));
        }
        (DragMode::Area, Tool::TextBox) => {
            painter.rect_stroke(
                to_screen(area, screen, zoom),
                2.0,
                Stroke::new(1.0, p.accent),
                egui::StrokeKind::Middle,
            );
        }
        (DragMode::Area, _) => {
            painter.rect_filled(to_screen(area, screen, zoom), 1.0, fill);
        }
        (DragMode::Pan, _) => {}
    }
    if let Some(r) = target_rect {
        let sr = to_screen(r, screen, zoom);
        painter.rect_filled(sr, 2.0, p.accent.gamma_multiply(0.08));
        painter.add(egui::Shape::dashed_line(
            &[
                sr.left_top(),
                sr.right_top(),
                sr.right_bottom(),
                sr.left_bottom(),
                sr.left_top(),
            ],
            Stroke::new(1.5, p.accent),
            5.0,
            3.0,
        ));
        if matches!(drag.mode, DragMode::Resize { .. }) {
            for h in handles(r) {
                draw_handle(&painter, screen.min + vec2(h.x, h.y) * zoom, &p);
            }
        }
    }

    if response.drag_stopped() {
        let drag = app.canvas.drag.take().expect("drag in progress");
        let big = area.width() > 3.0 && area.height() > 3.0;
        let annot = match (drag.mode, tool) {
            (DragMode::Pan, _) => None,
            (DragMode::Move { picked, from }, _) | (DragMode::Resize { picked, from, .. }, _) => {
                if let Some(to) = target_rect
                    && ((to.x0 - from.x0).abs()
                        + (to.y0 - from.y0).abs()
                        + (to.x1 - from.x1).abs()
                        + (to.y1 - from.y1).abs())
                        > 0.5
                {
                    actions.push(Action::SetRect(picked, from, to));
                }
                None
            }
            (DragMode::Text { a, b }, Tool::RedactText) => {
                actions.push(Action::MarkText(index, a, b));
                None
            }
            (DragMode::Area, Tool::RedactText | Tool::RedactArea) if big => {
                actions.push(Action::MarkArea(index, area));
                None
            }
            (DragMode::Area, Tool::Field(kind)) => {
                actions.push(Action::CreateField(index, kind, area));
                None
            }
            (DragMode::Text { a, b }, t) => markup(t, doc.page_text(index).line_rects(a, b), color),
            (DragMode::Area, Tool::Highlight | Tool::Underline | Tool::Strike) if big => {
                markup(tool, vec![area], color)
            }
            (DragMode::Area, Tool::Rectangle) if big => Some(NewAnnot::Square {
                rect: area,
                color,
                width: app.stroke,
                fill: app.shape_fill,
                opacity: app.shape_opacity,
            }),
            (DragMode::Area, Tool::Ellipse) if big => Some(NewAnnot::Circle {
                rect: area,
                color,
                width: app.stroke,
                fill: app.shape_fill,
                opacity: app.shape_opacity,
            }),
            (DragMode::Area, Tool::TextBox) => {
                let rect = if big {
                    area
                } else {
                    Rect::new(area.x0, area.y0, area.x0 + 200.0, area.y0 + 24.0)
                };
                actions.push(Action::Draft(TextDraft {
                    page: index,
                    rect,
                    target: DraftTarget::NewTextBox,
                    text: String::new(),
                    focus: true,
                }));
                None
            }
            (DragMode::Ink(points), _) if points.len() > 1 => Some(NewAnnot::Ink {
                strokes: vec![points],
                color,
                width: app.stroke,
            }),
            _ => None,
        };
        if let Some(a) = annot {
            actions.push(Action::Add(index, a));
        }
    }
}

/// The comment or page object under `at`, topmost first (comments sit above content).
fn hit_test(app: &App, page: usize, at: Point) -> Option<Picked> {
    let doc = app.doc.as_ref()?;
    if app.toolset == ToolSet::Forms {
        return crate::forms::field_at(app, page, at).map(|(f, i)| Picked::Field {
            page,
            slot: f.widgets[i].slot,
        });
    }
    if let Some(a) = doc
        .annotations(page)
        .into_iter()
        .rev()
        .find(|a| a.kind.is_comment() && a.rect.expand(2.0).contains(at))
    {
        return Some(Picked::Annot { page, slot: a.slot });
    }
    doc.items(page)
        .into_iter()
        .rev()
        .find(|i| i.rect.expand(2.0).contains(at))
        .map(|i| Picked::Item { page, id: i.id })
}

/// Corner handles (0–3, clockwise from top-left) then edge handles (4–7: top, right, bottom, left).
fn handles(r: Rect) -> [Point; 8] {
    let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
    [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
        Point::new(cx, r.y0),
        Point::new(r.x1, cy),
        Point::new(cx, r.y1),
        Point::new(r.x0, cy),
    ]
}

fn handle_at(r: Rect, at: Point, radius: f32) -> Option<usize> {
    handles(r)
        .iter()
        .position(|h| (h.x - at.x).abs() <= radius && (h.y - at.y).abs() <= radius)
}

fn draw_handle(painter: &egui::Painter, c: Pos2, p: &theme::Palette) {
    let r = egui::Rect::from_center_size(c, vec2(8.0, 8.0));
    painter.rect(
        r,
        2.0,
        Color32::WHITE,
        Stroke::new(1.5, p.accent),
        egui::StrokeKind::Middle,
    );
}

/// `from` with the given handle dragged by (dx, dy); never smaller than 4 pt.
pub(crate) fn resize(from: Rect, handle: usize, dx: f32, dy: f32) -> Rect {
    let mut r = from;
    let (left, right, top, bottom) = match handle {
        0 => (true, false, true, false),
        1 => (false, true, true, false),
        2 => (false, true, false, true),
        3 => (true, false, false, true),
        4 => (false, false, true, false),
        5 => (false, true, false, false),
        6 => (false, false, false, true),
        _ => (true, false, false, false),
    };
    if left {
        r.x0 = (from.x0 + dx).min(from.x1 - 4.0);
    }
    if right {
        r.x1 = (from.x1 + dx).max(from.x0 + 4.0);
    }
    if top {
        r.y0 = (from.y0 + dy).min(from.y1 - 4.0);
    }
    if bottom {
        r.y1 = (from.y1 + dy).max(from.y0 + 4.0);
    }
    r
}

fn markup(tool: Tool, quads: Vec<Rect>, color: pdf_engine::Color) -> Option<NewAnnot> {
    if quads.is_empty() {
        return None;
    }
    Some(match tool {
        Tool::Underline => NewAnnot::Underline { quads, color },
        Tool::Strike => NewAnnot::StrikeOut { quads, color },
        _ => NewAnnot::Highlight { quads, color },
    })
}

/// The inline editor for text boxes, notes and page text.
fn draft_editor(app: &mut App, ctx: &egui::Context, rects: &[egui::Rect], origin: Pos2, zoom: f32) {
    let Some(draft) = app.draft.as_mut() else {
        return;
    };
    let Some(page) = rects.get(draft.page) else {
        app.draft = None;
        return;
    };
    let screen = to_screen(draft.rect, page.translate(origin.to_vec2()), zoom);
    let p = palette(ctx.global_style().visuals.dark_mode);
    let note = draft.is_note();
    // Page text is edited over the text itself, at its size.
    let in_place = matches!(
        draft.target,
        DraftTarget::NewText
            | DraftTarget::Item(_)
            | DraftTarget::Line { .. }
            | DraftTarget::Field(_)
    );
    let field = match &draft.target {
        DraftTarget::Field(name) => Some(name.clone()),
        _ => None,
    };
    let mut tab: Option<bool> = None;
    if field.is_some() {
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                tab = Some(true);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                tab = Some(false);
            }
        });
    }
    let font_size = match &draft.target {
        DraftTarget::Line { line, .. } => line.size,
        DraftTarget::Item(id) => app
            .doc
            .as_ref()
            .and_then(|d| d.items(draft.page).into_iter().find(|i| i.id == *id))
            .and_then(|i| match i.kind {
                ItemKind::Text { size, .. } => Some(size),
                _ => None,
            })
            .unwrap_or(app.text_size),
        _ => app.text_size,
    };
    let pos = if note {
        screen.right_top() + vec2(8.0, 0.0)
    } else if in_place {
        screen.left_top() - vec2(13.0, 32.0)
    } else {
        screen.left_top()
    };
    let title = match &draft.target {
        DraftTarget::NewTextBox => tr("Text box"),
        DraftTarget::NewNote => tr("Sticky note"),
        DraftTarget::Annot(_) => {
            if note {
                tr("Edit note")
            } else {
                tr("Edit text box")
            }
        }
        DraftTarget::NewText => tr("New text"),
        DraftTarget::Field(name) => name.clone(),
        DraftTarget::Item(_) => tr("Edit text"),
        DraftTarget::Line { line, .. } => trf(
            "Edit line · {} → {}",
            &[
                &short_font(&line.font_name),
                &tr(editing::face_label(line.face)),
            ],
        ),
    };
    let mut commit = false;
    let mut cancel = false;
    egui::Area::new(egui::Id::unique("text_draft"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.accent))
                .corner_radius(8)
                .inner_margin(8)
                .shadow(ctx.global_style().visuals.popup_shadow)
                .show(ui, |ui| {
                    let width = if in_place {
                        (screen.width() + 24.0).clamp(240.0, 900.0)
                    } else {
                        240.0
                    };
                    ui.set_max_width(width + 16.0);
                    ui.label(
                        egui::RichText::new(title)
                            .font(theme::bold(12.0))
                            .color(p.muted),
                    );
                    let mut edit = egui::TextEdit::multiline(&mut draft.text)
                        .desired_width(width)
                        .desired_rows(if in_place { 1 } else { 3 })
                        .hint_text(if note {
                            tr("Write a note")
                        } else {
                            tr("Type text")
                        });
                    if in_place {
                        edit = edit.font(egui::FontId::proportional(
                            (font_size * zoom).clamp(10.0, 48.0),
                        ));
                    }
                    let r = ui.add(edit);
                    // egui doesn't order right-to-left text while typing: show it shaped.
                    if draft
                        .text
                        .chars()
                        .any(|c| ('\u{0590}'..='\u{08FF}').contains(&c))
                    {
                        let preview = crate::i18n::shape(&draft.text);
                        egui::Frame::new()
                            .fill(p.canvas)
                            .corner_radius(4)
                            .inner_margin(egui::Margin::symmetric(6, 3))
                            .show(ui, |ui| {
                                ui.set_width(width - 12.0);
                                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                    let size = if in_place {
                                        (font_size * zoom).clamp(10.0, 48.0)
                                    } else {
                                        14.0
                                    };
                                    ui.label(egui::RichText::new(preview).size(size));
                                });
                            });
                    }
                    r.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::TextInput,
                            true,
                            tr("Text being edited"),
                        )
                    });
                    if draft.focus {
                        r.request_focus();
                        draft.focus = false;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command) {
                        commit = true;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                    theme::row(ui, |ui| {
                        ui.label(
                            egui::RichText::new(crate::app::shortcut_hint())
                                .size(11.0)
                                .color(p.muted),
                        );
                        ui.with_layout(theme::far_layout(), |ui| {
                            let label = if matches!(
                                draft.target,
                                DraftTarget::NewTextBox
                                    | DraftTarget::NewNote
                                    | DraftTarget::NewText
                            ) {
                                tr("Add")
                            } else {
                                tr("Apply")
                            };
                            if theme::primary_button(ui, &label).clicked() {
                                commit = true;
                            }
                            if ui.button(tr("Cancel")).clicked() {
                                cancel = true;
                            }
                        });
                    });
                });
        });
    if tab.is_some() {
        commit = true;
    }
    if cancel {
        app.draft = None;
    } else if commit {
        let d = app.draft.take().expect("draft");
        commit_draft(app, d);
        if let (Some(back), Some(name)) = (tab, field) {
            crate::forms::tab(app, &name, back);
        }
    }
}

fn short_font(name: &str) -> &str {
    let n = name.split_once('+').map_or(name, |(_, b)| b);
    if n.is_empty() { "?" } else { n }
}

fn commit_draft(app: &mut App, d: TextDraft) {
    let text = d.text.trim_end().to_string();
    let page = d.page;
    match d.target {
        DraftTarget::Field(name) => {
            let value = d.text.clone();
            app.apply(true, |doc| doc.set_field_value(&name, &value));
        }
        DraftTarget::NewTextBox | DraftTarget::NewNote => {
            if text.is_empty() {
                return;
            }
            let annot = if d.target == DraftTarget::NewNote {
                NewAnnot::Note {
                    at: Point::new(d.rect.x0, d.rect.y0),
                    text,
                    color: app.colors[5],
                }
            } else {
                let size = app.font_size;
                let mut a = NewAnnot::FreeText {
                    rect: d.rect,
                    text,
                    color: app.colors[4],
                    size,
                    fill: app.text_box_fill,
                };
                a.set_font_size(size);
                a
            };
            app.apply(true, |doc| doc.add_annotation(page, &annot));
        }
        DraftTarget::Annot(slot) => {
            let Some(mut a) = app
                .doc
                .as_ref()
                .and_then(|doc| doc.annotation_shape(page, slot))
            else {
                return;
            };
            let size = a.font_size();
            a.set_text(text);
            if let Some(s) = size {
                a.set_font_size(s);
            }
            app.apply(true, |doc| doc.replace_annotation(page, slot, &a));
            app.picked = Some(Picked::Annot { page, slot });
        }
        DraftTarget::NewText => {
            if text.is_empty() {
                return;
            }
            let item = NewItem::Text {
                at: Point::new(d.rect.x0, d.rect.y0),
                text,
                face: app.text_face,
                size: app.text_size,
                color: app.text_color,
            };
            editing::add_item(app, page, item);
        }
        DraftTarget::Item(id) => {
            app.apply(true, |doc| {
                doc.update_item(
                    page,
                    id,
                    &ItemEdit {
                        text: Some(text),
                        ..Default::default()
                    },
                )
            });
            if app
                .doc
                .as_ref()
                .is_some_and(|doc| doc.items(page).iter().any(|i| i.id == id))
            {
                app.picked = Some(Picked::Item { page, id });
            }
        }
        DraftTarget::Line { at, line } => {
            if text == line.text {
                return;
            }
            let Some(doc) = app.doc.as_mut() else { return };
            match doc.replace_text_line(page, at, &text, None) {
                Ok(old) => {
                    app.keep_stale = true;
                    app.after_edit();
                    app.picked = app.doc.as_ref().and_then(|doc| {
                        doc.items(page)
                            .last()
                            .map(|i| Picked::Item { page, id: i.id })
                    });
                    let from = short_font(&old.font_name).to_string();
                    app.notify(trf(
                        "Line rewritten. “{}” isn’t available for new text, so {} was used.",
                        &[&from, &tr(editing::face_label(old.face))],
                    ));
                }
                Err(e) => app.notify(e.to_string()),
            }
        }
    }
}

fn floating_controls(
    app: &mut App,
    ui: &mut egui::Ui,
    view: egui::Rect,
    pages: usize,
    p: theme::Palette,
) {
    let dark = ui.visuals().dark_mode;
    let size = vec2(400.0, 38.0);
    let pos = pos2(
        view.center().x - size.x / 2.0,
        view.bottom() - size.y - 16.0,
    );
    egui::Area::new(egui::Id::unique("page_controls"))
        .fixed_pos(pos)
        .order(egui::Order::Middle)
        .show(ui.ctx(), |ui| {
            theme::floating_frame(dark).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if theme::icon_button(ui, Icon::Up, &tr("Previous page"), false).clicked() {
                        app.go_to(app.current_page.saturating_sub(1));
                    }
                    if theme::icon_button(ui, Icon::Down, &tr("Next page"), false).clicked() {
                        app.go_to(app.current_page + 1);
                    }
                    if !app.canvas.page_input_focused {
                        app.canvas.page_input = (app.current_page + 1).to_string();
                    }
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut app.canvas.page_input)
                            .desired_width(34.0)
                            .align(egui::Align2::CENTER_CENTER),
                    );
                    r.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("Page number"))
                    });
                    app.canvas.page_input_focused = r.has_focus();
                    if r.lost_focus() {
                        if let Ok(n) = app.canvas.page_input.trim().parse::<usize>() {
                            app.go_to(n.saturating_sub(1));
                        }
                        app.canvas.page_input_focused = false;
                    }
                    ui.label(egui::RichText::new(trf("of {}", &[&pages])).color(p.muted));
                    theme::divider(ui);
                    if theme::icon_button(ui, Icon::ZoomOut, &tr("Zoom out"), false).clicked() {
                        app.set_zoom(app.zoom / 1.25);
                    }
                    let label = format!("{:.0}%", app.zoom * 100.0);
                    let zr = ui.add(
                        egui::Button::new(egui::RichText::new(label).size(12.5))
                            .frame(false)
                            .min_size(vec2(48.0, 28.0)),
                    );
                    egui::Popup::menu(&zr).show(|ui| {
                        for z in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0] {
                            if ui.button(format!("{:.0}%", z * 100.0)).clicked() {
                                app.set_zoom(z);
                            }
                        }
                        if ui.button(tr("Fit width")).clicked() {
                            app.fit_width = true;
                        }
                    });
                    if theme::icon_button(ui, Icon::ZoomIn, &tr("Zoom in"), false).clicked() {
                        app.set_zoom(app.zoom * 1.25);
                    }
                    if theme::icon_button(ui, Icon::FitWidth, &tr("Fit width"), app.fit_width)
                        .clicked()
                    {
                        app.fit_width = true;
                    }
                });
            });
        });
}
