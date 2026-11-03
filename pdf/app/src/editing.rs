//! Edit PDF tools, and properties of the selected comment or page object.

use eframe::egui::{self, Vec2};
use pdf_engine::{Color, FontFace, ItemEdit, ItemKind, NewAnnot, NewItem, Rect};

use crate::app::{App, DraftTarget, Picked, SWATCHES, TextDraft, Tool, to_color32};
use crate::i18n::{n_, tr, trf};
use crate::theme::{self, Icon, palette};

/// What the selection is, for the properties panel and the canvas.
pub enum Shape {
    Annot(NewAnnot),
    Item(pdf_engine::PageItem),
    /// A comment kind the editor can only delete (links, fields, …).
    Fixed(Rect),
    /// A form field widget.
    Field(Rect),
}

impl Shape {
    pub fn bounds(&self) -> Rect {
        match self {
            Shape::Annot(a) => a.bounds(),
            Shape::Item(i) => i.rect,
            Shape::Fixed(r) | Shape::Field(r) => *r,
        }
    }
    pub fn resizable(&self) -> bool {
        match self {
            Shape::Annot(a) => a.resizable(),
            Shape::Item(_) | Shape::Field(_) => true,
            Shape::Fixed(_) => false,
        }
    }
    pub fn movable(&self) -> bool {
        !matches!(self, Shape::Fixed(_))
    }
}

pub fn shape(app: &App, picked: Picked) -> Option<Shape> {
    let doc = app.doc.as_ref()?;
    match picked {
        Picked::Annot { page, slot } => match doc.annotation_shape(page, slot) {
            Some(a) => Some(Shape::Annot(a)),
            None => doc
                .annotations(page)
                .into_iter()
                .find(|a| a.slot == slot)
                .map(|a| Shape::Fixed(a.rect)),
        },
        Picked::Item { page, id } => doc
            .items(page)
            .into_iter()
            .find(|i| i.id == id)
            .map(Shape::Item),
        Picked::Field { page, slot } => doc
            .fields()
            .into_iter()
            .flat_map(|f| f.widgets)
            .find(|w| w.page == page && w.slot == slot)
            .map(|w| Shape::Field(w.rect)),
    }
}

/// Applies a new rectangle (move or resize) to the selection.
pub fn set_rect(app: &mut App, picked: Picked, from: Rect, to: Rect) {
    match (picked, shape(app, picked)) {
        (Picked::Annot { page, slot }, Some(Shape::Annot(a))) => {
            let moved = a.transformed(from, to);
            app.apply(true, |d| d.replace_annotation(page, slot, &moved));
        }
        (Picked::Item { page, id }, Some(Shape::Item(_))) => {
            app.apply(true, |d| {
                d.update_item(
                    page,
                    id,
                    &ItemEdit {
                        rect: Some(to),
                        ..Default::default()
                    },
                )
            });
        }
        (Picked::Field { page, slot }, Some(Shape::Field(_))) => {
            app.apply(true, |d| d.set_widget_rect(page, slot, to));
        }
        _ => {}
    }
    // Keep the selection on the edited object.
    app.picked = Some(picked);
}

/// Opens the inline editor on a text box, note or text item.
pub fn edit_text_of(app: &mut App, picked: Picked) -> bool {
    let Some(s) = shape(app, picked) else {
        return false;
    };
    let (target, text, rect) = match (&s, picked) {
        (Shape::Annot(a), Picked::Annot { slot, .. }) => match a.text() {
            Some(t) => (DraftTarget::Annot(slot), t.to_string(), a.bounds()),
            None => return false,
        },
        (Shape::Item(i), Picked::Item { id, .. }) => match &i.kind {
            ItemKind::Text { text, .. } => (DraftTarget::Item(id), text.clone(), i.rect),
            _ => return false,
        },
        _ => return false,
    };
    app.draft = Some(TextDraft {
        page: picked.page(),
        rect,
        target,
        text,
        focus: true,
    });
    true
}

/// The label of a comment kind, marked for translation.
pub fn annot_label(k: pdf_engine::AnnotKind) -> &'static str {
    use pdf_engine::AnnotKind as K;
    match k {
        K::Highlight => n_("Highlight"),
        K::Underline => n_("Underline"),
        K::StrikeOut => n_("Strikethrough"),
        K::FreeText => n_("Text box"),
        K::Note => n_("Sticky note"),
        K::Square => n_("Rectangle"),
        K::Circle => n_("Ellipse"),
        K::Ink => n_("Drawing"),
        K::Link => n_("Link"),
        K::Widget => n_("Form field"),
        K::Other => n_("Comment"),
    }
}

/// The label of a font face, marked for translation.
pub fn face_label(f: FontFace) -> &'static str {
    match f {
        FontFace::Sans => n_("Sans (Liberation Sans)"),
        FontFace::SansBold => n_("Sans Bold"),
        FontFace::Serif => n_("Serif (Liberation Serif)"),
        FontFace::Script => n_("Script (Great Vibes)"),
    }
}

/// Colour swatches; returns the clicked colour.
pub fn swatches(ui: &mut egui::Ui, current: Color, label: &str) -> Option<Color> {
    let p = palette(ui.visuals().dark_mode);
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(4.0);
        for (i, c) in SWATCHES.iter().enumerate() {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
            let on = color_eq(*c, current);
            r.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::RadioButton,
                    true,
                    on,
                    format!("{label} {}", i + 1),
                )
            });
            ui.painter()
                .circle_filled(rect.center(), 8.0, to_color32(*c));
            ui.painter()
                .circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.0, p.border));
            if on {
                ui.painter()
                    .circle_stroke(rect.center(), 9.8, egui::Stroke::new(2.0, p.accent));
            }
            if r.clicked() {
                out = Some(*c);
            }
        }
    });
    out
}

/// Fill swatches with a "none" choice first; returns the clicked choice.
pub fn fill_swatches(
    ui: &mut egui::Ui,
    current: Option<Color>,
    label: &str,
) -> Option<Option<Color>> {
    let p = palette(ui.visuals().dark_mode);
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(4.0);
        let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
        r.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::RadioButton,
                true,
                current.is_none(),
                format!("{label}: {}", tr("None")),
            )
        });
        ui.painter()
            .circle_filled(rect.center(), 8.0, egui::Color32::WHITE);
        ui.painter()
            .circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.0, p.border));
        ui.painter().line_segment(
            [
                rect.center() + egui::vec2(-6.0, 6.0),
                rect.center() + egui::vec2(6.0, -6.0),
            ],
            egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 45, 45)),
        );
        if current.is_none() {
            ui.painter()
                .circle_stroke(rect.center(), 9.8, egui::Stroke::new(2.0, p.accent));
        }
        if r.on_hover_text(tr("No fill")).clicked() {
            out = Some(None);
        }
        for (i, c) in FILLS.iter().enumerate() {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
            let on = current.is_some_and(|k| color_eq(k, *c));
            r.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::RadioButton,
                    true,
                    on,
                    format!("{label} {}", i + 1),
                )
            });
            ui.painter()
                .circle_filled(rect.center(), 8.0, to_color32(*c));
            ui.painter()
                .circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.0, p.border));
            if on {
                ui.painter()
                    .circle_stroke(rect.center(), 9.8, egui::Stroke::new(2.0, p.accent));
            }
            if r.clicked() {
                out = Some(Some(*c));
            }
        }
    });
    out
}

/// Fill colours: light tints that keep text and outlines readable, plus white and black.
pub const FILLS: [Color; 7] = [
    Color::rgb(255, 255, 255),
    Color::rgb(255, 241, 168),
    Color::rgb(205, 240, 200),
    Color::rgb(205, 228, 252),
    Color::rgb(252, 214, 228),
    Color::rgb(230, 230, 230),
    Color::rgb(21, 21, 21),
];

fn color_eq(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 0.01 && (a.g - b.g).abs() < 0.01 && (a.b - b.b).abs() < 0.01
}

/// A slider that applies its value once, when the drag ends (one undo step).
fn staged_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
) -> bool {
    let r = ui.add(egui::Slider::new(value, range).suffix(" pt"));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, label));
    r.drag_stopped() || (r.changed() && !r.dragged())
}

pub fn font_combo(ui: &mut egui::Ui, face: &mut FontFace, id: &str) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(tr(face_label(*face)))
        .width(180.0)
        .show_ui(ui, |ui| {
            for f in FontFace::ALL {
                changed |= ui.selectable_value(face, f, tr(face_label(f))).changed();
            }
        })
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tr("Font")));
    changed
}

/// Properties of the selected comment or object; returns true if shown.
pub fn selection_panel(app: &mut App, ui: &mut egui::Ui) -> bool {
    let Some(picked) = app.picked else {
        return false;
    };
    let Some(s) = shape(app, picked) else {
        app.picked = None;
        return false;
    };
    if matches!(s, Shape::Field(_)) {
        return false;
    }
    let p = palette(ui.visuals().dark_mode);
    if app.prop_for != Some(picked) {
        app.prop_for = Some(picked);
        match &s {
            Shape::Annot(a) => {
                app.prop_width = a.line_width().unwrap_or(2.0);
                app.prop_size = a.font_size().unwrap_or(14.0);
            }
            Shape::Item(i) => match &i.kind {
                ItemKind::Text { size, .. } => app.prop_size = *size,
                ItemKind::Ink { width, .. } => app.prop_width = *width,
                ItemKind::Image => {}
            },
            Shape::Fixed(_) | Shape::Field(_) => {}
        }
    }
    let label = match &s {
        Shape::Annot(_) | Shape::Fixed(_) | Shape::Field(_) => {
            let page = picked.page();
            let slot = match picked {
                Picked::Annot { slot, .. } => slot,
                _ => 0,
            };
            app.doc
                .as_ref()
                .and_then(|d| d.annotations(page).into_iter().find(|a| a.slot == slot))
                .map_or(tr("Comment"), |a| tr(annot_label(a.kind)))
        }
        Shape::Item(i) => match i.kind {
            ItemKind::Text { .. } => tr("Page text"),
            ItemKind::Image => tr("Picture"),
            ItemKind::Ink { .. } => tr("Drawing"),
        },
    };
    theme::caption(ui, &tr("Selection"));
    ui.label(egui::RichText::new(label).font(theme::bold(13.5)));
    let color = match &s {
        Shape::Annot(a) => Some(a.color()),
        Shape::Item(i) => match &i.kind {
            ItemKind::Text { color, .. } | ItemKind::Ink { color, .. } => Some(*color),
            ItemKind::Image => None,
        },
        Shape::Fixed(_) | Shape::Field(_) => None,
    };
    if let Some(c) = color
        && let Some(new) = swatches(ui, c, &tr("Selection color"))
    {
        match (&s, picked) {
            (Shape::Annot(a), Picked::Annot { page, slot }) => {
                let mut a = a.clone();
                a.set_color(new);
                app.apply(true, |d| d.replace_annotation(page, slot, &a));
            }
            (Shape::Item(_), Picked::Item { page, id }) => {
                app.apply(true, |d| {
                    d.update_item(
                        page,
                        id,
                        &ItemEdit {
                            color: Some(new),
                            ..Default::default()
                        },
                    )
                });
            }
            _ => {}
        }
        app.picked = Some(picked);
    }
    let has_width = match &s {
        Shape::Annot(a) => a.line_width().is_some(),
        Shape::Item(i) => matches!(i.kind, ItemKind::Ink { .. }),
        Shape::Fixed(_) | Shape::Field(_) => false,
    };
    if has_width {
        ui.label(
            egui::RichText::new(tr("Line width"))
                .size(12.0)
                .color(p.muted),
        );
        let mut w = app.prop_width;
        if staged_slider(ui, &mut w, 0.5..=12.0, &tr("Selection line width")) {
            apply_style(app, picked, &s, Some(w), None);
        }
        app.prop_width = w;
    }
    let has_size = match &s {
        Shape::Annot(a) => a.font_size().is_some(),
        Shape::Item(i) => matches!(i.kind, ItemKind::Text { .. }),
        Shape::Fixed(_) | Shape::Field(_) => false,
    };
    if has_size {
        ui.label(
            egui::RichText::new(tr("Font size"))
                .size(12.0)
                .color(p.muted),
        );
        let mut z = app.prop_size;
        if staged_slider(ui, &mut z, 6.0..=72.0, &tr("Selection font size")) {
            apply_style(app, picked, &s, None, Some(z));
        }
        app.prop_size = z;
    }
    if let (Shape::Item(i), Picked::Item { page, id }) = (&s, picked)
        && let ItemKind::Text { face, .. } = &i.kind
    {
        let mut f = *face;
        if font_combo(ui, &mut f, "selection_font") {
            app.apply(true, |d| {
                d.update_item(
                    page,
                    id,
                    &ItemEdit {
                        face: Some(f),
                        ..Default::default()
                    },
                )
            });
            app.picked = Some(picked);
        }
    }
    if let (Shape::Annot(a), Picked::Annot { page, slot }) = (&s, picked)
        && a.fillable()
    {
        let shape = a.opacity().is_some();
        ui.label(
            egui::RichText::new(if shape { tr("Fill") } else { tr("Background") })
                .size(12.0)
                .color(p.muted),
        );
        if let Some(f) = fill_swatches(ui, a.fill(), &tr("Selection fill")) {
            let mut a = a.clone();
            a.set_fill(f);
            app.apply(true, |d| d.replace_annotation(page, slot, &a));
            app.picked = Some(picked);
        }
        if let Some(o) = a.opacity() {
            ui.label(egui::RichText::new(tr("Opacity")).size(12.0).color(p.muted));
            if app.prop_for_opacity != Some(picked) {
                app.prop_for_opacity = Some(picked);
                app.prop_opacity = o * 100.0;
            }
            let mut v = app.prop_opacity;
            let r = ui.add(egui::Slider::new(&mut v, 5.0..=100.0).suffix(" %"));
            r.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Slider, true, tr("Selection opacity"))
            });
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                let mut a = a.clone();
                a.set_opacity(v / 100.0);
                app.apply(true, |d| d.replace_annotation(page, slot, &a));
                app.picked = Some(picked);
            }
            app.prop_opacity = v;
        }
    }
    let editable_text = match &s {
        Shape::Annot(a) => a.text().is_some(),
        Shape::Item(i) => matches!(i.kind, ItemKind::Text { .. }),
        Shape::Fixed(_) | Shape::Field(_) => false,
    };
    ui.add_space(4.0);
    if editable_text && theme::tool_row(ui, Icon::TextBox, &tr("Edit text…"), false).clicked() {
        edit_text_of(app, picked);
    }
    if theme::tool_row(ui, Icon::Trash, &tr("Delete"), false).clicked() {
        app.delete_picked();
    }
    ui.add_space(6.0);
    ui.separator();
    true
}

fn apply_style(app: &mut App, picked: Picked, s: &Shape, width: Option<f32>, size: Option<f32>) {
    match (s, picked) {
        (Shape::Annot(a), Picked::Annot { page, slot }) => {
            let mut a = a.clone();
            if let Some(w) = width {
                a.set_line_width(w);
            }
            if let Some(z) = size {
                a.set_font_size(z);
            }
            app.apply(true, |d| d.replace_annotation(page, slot, &a));
        }
        (Shape::Item(_), Picked::Item { page, id }) => {
            app.apply(true, |d| {
                d.update_item(
                    page,
                    id,
                    &ItemEdit {
                        width,
                        size,
                        ..Default::default()
                    },
                )
            });
        }
        _ => {}
    }
    app.picked = Some(picked);
}

/// Edit PDF tools pane.
pub fn tools(app: &mut App, ui: &mut egui::Ui) {
    let p = palette(ui.visuals().dark_mode);
    theme::caption(ui, &tr("Edit PDF"));
    for tool in [Tool::Select, Tool::EditText, Tool::AddText] {
        if theme::tool_row(ui, tool.icon(), tool.label(), app.tool == tool).clicked() {
            app.tool = tool;
            app.draft = None;
        }
    }
    if theme::tool_row(ui, Icon::Image, &tr("Add image…"), false).clicked() {
        add_image_dialog(app);
    }
    if matches!(app.tool, Tool::AddText | Tool::EditText) {
        ui.add_space(10.0);
        theme::caption(ui, &tr("Text style"));
        let mut face = app.text_face;
        if font_combo(ui, &mut face, "new_text_font") {
            app.text_face = face;
        }
        ui.add(egui::Slider::new(&mut app.text_size, 6.0..=72.0).suffix(" pt"))
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, tr("Text size")));
        if let Some(c) = swatches(ui, app.text_color, &tr("Text color")) {
            app.text_color = c;
        }
        if app.tool == Tool::EditText {
            theme::para(
                ui,
                n_(
                    "Edited lines keep their original size and colour; the font becomes the closest bundled one.",
                ),
                11.5,
                Some(p.muted),
            );
        }
    }
    ui.add_space(10.0);
    theme::para(ui, app.tool.hint(), 12.0, Some(p.muted));
}

pub fn add_image_dialog(app: &mut App) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Pictures", &["png", "jpg", "jpeg"])
        .set_title(tr("Add image"))
        .pick_file()
    else {
        return;
    };
    match std::fs::read(&path) {
        Ok(bytes) => add_image(app, bytes),
        Err(e) => app.notify(trf("Could not read the picture: {}", &[&e])),
    }
}

/// Places a picture in the middle of the current page, at most 240 pt wide.
pub fn add_image(app: &mut App, bytes: Vec<u8>) {
    let Some(doc) = &app.doc else { return };
    let page = app.current_page;
    let (pw, ph) = doc.page_size(page);
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .map_or((200.0, 150.0), |(w, h)| (w as f32, h as f32));
    let s = (240.0 / w).min((ph * 0.5) / h).min(1.0);
    let (w, h) = (w * s, h * s);
    let rect = Rect::new(
        (pw - w) / 2.0,
        (ph - h) / 2.0,
        (pw + w) / 2.0,
        (ph + h) / 2.0,
    );
    add_item(app, page, NewItem::Image { rect, data: bytes });
}

/// Adds an item and selects it.
pub fn add_item(app: &mut App, page: usize, item: NewItem) {
    let Some(doc) = app.doc.as_mut() else { return };
    match doc.add_item(page, &item) {
        Ok(id) => {
            app.keep_stale = true;
            app.after_edit();
            app.picked = Some(Picked::Item { page, id });
        }
        Err(e) => app.notify(e.to_string()),
    }
}
