//! Forms tool set: filling fields on the page (with Tab between fields) and preparing forms
//! (adding, moving, resizing, renaming and deleting fields).

use eframe::egui::{self, Color32, RichText, Stroke};
use pdf_engine::{FieldInfo, FieldKind, NewField, Rect};

use crate::app::{App, DraftTarget, Picked, TextDraft, Tool};
use crate::i18n::{n_, tr};
use crate::theme::{self, Icon, palette};

#[derive(Default)]
pub struct FormsState {
    /// Choice list open for a dropdown/list field: (field name, page, widget rect).
    pub choosing: Option<(String, usize, Rect)>,
    /// The field with keyboard focus (Tab order) when it isn't being typed in.
    pub focused: Option<String>,
    /// Property editor values for the selected field.
    pub edit_for: Option<String>,
    pub name: String,
    pub default: String,
    pub options: String,
}

pub const KINDS: [FieldKind; 6] = [
    FieldKind::Text,
    FieldKind::CheckBox,
    FieldKind::Radio,
    FieldKind::ComboBox,
    FieldKind::ListBox,
    FieldKind::Signature,
];

pub fn kind_label(k: FieldKind) -> &'static str {
    match k {
        FieldKind::Text => n_("Text field"),
        FieldKind::CheckBox => n_("Check box"),
        FieldKind::Radio => n_("Radio button"),
        FieldKind::ComboBox => n_("Dropdown"),
        FieldKind::ListBox => n_("List box"),
        FieldKind::Signature => n_("Signature field"),
        FieldKind::PushButton => n_("Button"),
    }
}

pub fn kind_icon(k: FieldKind) -> Icon {
    match k {
        FieldKind::Text => Icon::TextBox,
        FieldKind::CheckBox => Icon::Check,
        FieldKind::Radio => Icon::Ellipse,
        FieldKind::ComboBox => Icon::Down,
        FieldKind::ListBox => Icon::Pages,
        FieldKind::Signature | FieldKind::PushButton => Icon::Signature,
    }
}

/// Fields in tab order: by page, then top to bottom, then left to right.
pub fn ordered(app: &App) -> Vec<FieldInfo> {
    let Some(doc) = &app.doc else {
        return Vec::new();
    };
    let mut f: Vec<FieldInfo> = doc
        .fields()
        .into_iter()
        .filter(|f| !f.widgets.is_empty() && f.kind != FieldKind::PushButton)
        .collect();
    f.sort_by(|a, b| {
        let (wa, wb) = (&a.widgets[0], &b.widgets[0]);
        (
            wa.page,
            (wa.rect.y0 / 4.0) as i64,
            (wa.rect.x0 * 10.0) as i64,
        )
            .cmp(&(
                wb.page,
                (wb.rect.y0 / 4.0) as i64,
                (wb.rect.x0 * 10.0) as i64,
            ))
    });
    f
}

/// The field widget under `at` on `page`: (field, widget index).
pub fn field_at(app: &App, page: usize, at: pdf_engine::Point) -> Option<(FieldInfo, usize)> {
    let doc = app.doc.as_ref()?;
    doc.fields().into_iter().find_map(|f| {
        let i = f
            .widgets
            .iter()
            .position(|w| w.page == page && w.rect.expand(1.0).contains(at))?;
        Some((f, i))
    })
}

/// Opens or toggles the field at `at` (Fill mode).
pub fn click(app: &mut App, page: usize, at: pdf_engine::Point) {
    let Some((field, wi)) = field_at(app, page, at) else {
        app.forms.choosing = None;
        app.forms.focused = None;
        return;
    };
    app.forms.focused = Some(field.name.clone());
    open_field(app, &field, wi);
}

pub fn open_field(app: &mut App, field: &FieldInfo, wi: usize) {
    if field.read_only {
        app.notify(tr("This field is read-only."));
        return;
    }
    let w = &field.widgets[wi];
    match field.kind {
        FieldKind::Text => {
            app.draft = Some(TextDraft {
                page: w.page,
                rect: w.rect,
                target: DraftTarget::Field(field.name.clone()),
                text: field.value.clone(),
                focus: true,
            });
        }
        FieldKind::CheckBox => {
            let on = w.on_state.clone().unwrap_or_else(|| "Yes".into());
            let value = if field.value == on {
                "Off".to_string()
            } else {
                on
            };
            let name = field.name.clone();
            app.apply(true, |d| d.set_field_value(&name, &value));
        }
        FieldKind::Radio => {
            if let Some(on) = w.on_state.clone() {
                let name = field.name.clone();
                app.apply(true, |d| d.set_field_value(&name, &on));
            }
        }
        FieldKind::ComboBox | FieldKind::ListBox => {
            app.forms.choosing = Some((field.name.clone(), w.page, w.rect))
        }
        FieldKind::Signature => {
            app.toolset = crate::app::ToolSet::Sign;
            app.tool = Tool::Select;
            app.notify(tr(
                "Choose a signature, then click inside the signature field.",
            ));
        }
        FieldKind::PushButton => {}
    }
    app.canvas.reveal = Some((w.page, w.rect));
}

/// Moves to the next (or previous) field after `name`: text fields open for typing, other
/// fields take the keyboard focus (Space or Enter toggles or opens them).
pub fn tab(app: &mut App, name: &str, back: bool) {
    let list = ordered(app);
    if list.is_empty() {
        return;
    }
    let n = list.len();
    let i = list
        .iter()
        .position(|f| f.name == name)
        .unwrap_or(if back { 0 } else { n - 1 });
    let next = if back { (i + n - 1) % n } else { (i + 1) % n };
    let f = list[next].clone();
    app.forms.focused = Some(f.name.clone());
    let w = &f.widgets[0];
    app.canvas.reveal = Some((w.page, w.rect));
    app.current_page = w.page;
    if f.kind == FieldKind::Text {
        open_field(app, &f, 0);
    }
}

/// Tab / Shift+Tab / Space / Enter when no field is being typed in.
pub fn keyboard(app: &mut App, ctx: &egui::Context) {
    if app.tool != Tool::Fill || ctx.egui_wants_keyboard_input() {
        return;
    }
    let (tab_back, tab_fwd, activate) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Space)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
        )
    });
    let current = app.forms.focused.clone().unwrap_or_default();
    if tab_back || tab_fwd {
        tab(app, &current, tab_back);
    } else if activate && let Some(f) = ordered(app).into_iter().find(|f| f.name == current) {
        open_field(app, &f, 0);
    }
}

/// Field overlays drawn on a page in the Forms tool set.
pub fn paint_fields(
    app: &App,
    painter: &egui::Painter,
    page: usize,
    screen: egui::Rect,
    zoom: f32,
) {
    let Some(doc) = &app.doc else { return };
    let p = palette(false);
    let prepare = app.tool != Tool::Fill;
    for f in doc.fields() {
        for w in f.widgets.iter().filter(|w| w.page == page) {
            let r = egui::Rect::from_min_max(
                screen.min + egui::vec2(w.rect.x0, w.rect.y0) * zoom,
                screen.min + egui::vec2(w.rect.x1, w.rect.y1) * zoom,
            );
            painter.rect_filled(r, 1.0, Color32::from_rgba_unmultiplied(60, 120, 230, 28));
            painter.rect_stroke(
                r,
                1.0,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 120, 230, 150)),
                egui::StrokeKind::Middle,
            );
            if prepare {
                painter.text(
                    r.left_top() + egui::vec2(2.0, -2.0),
                    egui::Align2::LEFT_BOTTOM,
                    &f.name,
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(40, 90, 200),
                );
            }
        }
    }
    let _ = p;
}

/// The choice list for an open dropdown/list field.
pub fn choice_popup(
    app: &mut App,
    ctx: &egui::Context,
    page_screen: &dyn Fn(usize) -> Option<egui::Rect>,
    zoom: f32,
) {
    let Some((name, page, rect)) = app.forms.choosing.clone() else {
        return;
    };
    let Some(screen) = page_screen(page) else {
        return;
    };
    let Some(field) = app
        .doc
        .as_ref()
        .and_then(|d| d.fields().into_iter().find(|f| f.name == name))
    else {
        app.forms.choosing = None;
        return;
    };
    let pos = screen.min + egui::vec2(rect.x0, rect.y1) * zoom + egui::vec2(0.0, 2.0);
    let mut pick = None;
    let resp = egui::Area::new(egui::Id::unique("field_choices"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(((rect.x1 - rect.x0) * zoom).max(140.0));
                for o in &field.options {
                    if ui
                        .selectable_label(*o == field.value, crate::i18n::shape(o))
                        .clicked()
                    {
                        pick = Some(o.clone());
                    }
                }
            });
        });
    if let Some(v) = pick {
        app.forms.choosing = None;
        app.apply(true, |d| d.set_field_value(&name, &v));
    } else if resp.response.clicked_elsewhere() {
        app.forms.choosing = None;
    }
}

/// Creates a field from a dragged rectangle (Prepare form).
pub fn create(app: &mut App, page: usize, kind: FieldKind, rect: Rect) {
    let Some(doc) = &app.doc else { return };
    let rect = match kind {
        FieldKind::CheckBox | FieldKind::Radio if rect.width() < 6.0 => {
            Rect::new(rect.x0, rect.y0, rect.x0 + 14.0, rect.y0 + 14.0)
        }
        _ if rect.width() < 6.0 => Rect::new(
            rect.x0,
            rect.y0,
            rect.x0 + 160.0,
            rect.y0
                + if kind == FieldKind::ListBox {
                    60.0
                } else {
                    22.0
                },
        ),
        _ => rect,
    };
    let base = match kind {
        FieldKind::Text => "Text",
        FieldKind::CheckBox => "Check",
        FieldKind::Radio => "Choice",
        FieldKind::ComboBox => "Dropdown",
        FieldKind::ListBox => "List",
        FieldKind::Signature => "Signature",
        FieldKind::PushButton => "Button",
    };
    let names: Vec<String> = doc.fields().into_iter().map(|f| f.name).collect();
    // A new radio button joins the group of the selected radio, if any.
    let group = match app.picked {
        Some(Picked::Field { page: pp, slot }) if kind == FieldKind::Radio => doc
            .fields()
            .into_iter()
            .find(|f| {
                f.kind == FieldKind::Radio
                    && f.widgets.iter().any(|w| w.page == pp && w.slot == slot)
            })
            .map(|f| (f.name, f.options.len())),
        _ => None,
    };
    let (name, options) = match (kind, group) {
        (FieldKind::Radio, Some((name, n))) => (name, vec![format!("Choice{}", n + 1)]),
        (FieldKind::Radio, None) => (
            (1..)
                .map(|i| format!("Group{i}"))
                .find(|n| !names.contains(n))
                .unwrap_or_default(),
            vec!["Choice1".to_string()],
        ),
        (FieldKind::ComboBox | FieldKind::ListBox, _) => (
            (1..)
                .map(|i| format!("{base}{i}"))
                .find(|n| !names.contains(n))
                .unwrap_or_default(),
            vec!["Option 1".into(), "Option 2".into(), "Option 3".into()],
        ),
        _ => (
            (1..)
                .map(|i| format!("{base}{i}"))
                .find(|n| !names.contains(n))
                .unwrap_or_default(),
            Vec::new(),
        ),
    };
    let field = NewField {
        kind,
        name: name.clone(),
        rect,
        options,
    };
    let Some(doc) = app.doc.as_mut() else { return };
    match doc.add_field(page, &field) {
        Ok(_) => {
            app.keep_stale = true;
            app.after_edit();
            app.picked = app
                .doc
                .as_ref()
                .and_then(|d| d.fields().into_iter().find(|f| f.name == name))
                .and_then(|f| {
                    f.widgets.last().map(|w| Picked::Field {
                        page: w.page,
                        slot: w.slot,
                    })
                });
        }
        Err(e) => app.notify(e.to_string()),
    }
}

/// The field under a Prepare-mode selection.
pub fn picked_field(app: &App) -> Option<FieldInfo> {
    let Some(Picked::Field { page, slot }) = app.picked else {
        return None;
    };
    app.doc
        .as_ref()?
        .fields()
        .into_iter()
        .find(|f| f.widgets.iter().any(|w| w.page == page && w.slot == slot))
}

/// Properties of the selected field (Prepare form).
pub fn properties(app: &mut App, ui: &mut egui::Ui) -> bool {
    let Some(field) = picked_field(app) else {
        return false;
    };
    let p = palette(ui.visuals().dark_mode);
    if app.forms.edit_for.as_deref() != Some(field.name.as_str()) {
        app.forms.edit_for = Some(field.name.clone());
        app.forms.name = field.name.clone();
        app.forms.default = field.default.clone();
        app.forms.options = field.options.join("\n");
    }
    theme::caption(ui, &tr("Selection"));
    ui.label(RichText::new(tr(kind_label(field.kind))).font(theme::bold(13.5)));
    ui.label(RichText::new(tr("Name")).size(12.0).color(p.muted));
    ui.add(egui::TextEdit::singleline(&mut app.forms.name).desired_width(f32::INFINITY))
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("Field name")));
    if !matches!(
        field.kind,
        FieldKind::Signature | FieldKind::Radio | FieldKind::CheckBox
    ) {
        ui.label(RichText::new(tr("Default value")).size(12.0).color(p.muted));
        ui.add(egui::TextEdit::singleline(&mut app.forms.default).desired_width(f32::INFINITY))
            .widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("Default value"))
            });
    }
    if matches!(field.kind, FieldKind::ComboBox | FieldKind::ListBox) {
        ui.label(
            RichText::new(tr("Options, one per line"))
                .size(12.0)
                .color(p.muted),
        );
        ui.add(
            egui::TextEdit::multiline(&mut app.forms.options)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        )
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("Options")));
    }
    if ui.button(tr("Apply changes")).clicked() {
        let (old, new, default) = (
            field.name.clone(),
            app.forms.name.trim().to_string(),
            app.forms.default.clone(),
        );
        let options: Vec<String> = app
            .forms
            .options
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let keep = app.picked;
        app.apply(true, |d| {
            d.set_field_properties(&old, &new, &default, &options)
        });
        app.picked = keep;
        app.forms.edit_for = None;
    }
    if theme::tool_row(ui, Icon::Trash, &tr("Delete field"), false).clicked()
        && let Some(Picked::Field { page, slot }) = app.picked
    {
        app.apply(true, |d| d.delete_field(page, slot));
        app.picked = None;
    }
    ui.add_space(6.0);
    ui.separator();
    true
}

pub fn tools(app: &mut App, ui: &mut egui::Ui) {
    let p = palette(ui.visuals().dark_mode);
    theme::caption(ui, &tr("Fill in"));
    if theme::tool_row(ui, Icon::TextBox, &tr("Fill form"), app.tool == Tool::Fill).clicked() {
        app.tool = Tool::Fill;
        app.picked = None;
    }
    let n = app.doc.as_ref().map_or(0, |d| d.fields().len());
    if n == 0 {
        theme::para(
            ui,
            n_("This PDF has no form fields yet."),
            12.0,
            Some(p.muted),
        );
    } else {
        let text =
            crate::i18n::raw("{} fields. Click a field to fill it; Tab moves to the next one.")
                .replacen("{}", &n.to_string(), 1);
        theme::para_text(ui, &text, 12.0, Some(p.muted));
    }
    ui.add_space(8.0);
    theme::caption(ui, &tr("Prepare form"));
    if theme::tool_row(
        ui,
        Icon::Select,
        &tr("Select fields"),
        app.tool == Tool::Select,
    )
    .clicked()
    {
        app.tool = Tool::Select;
    }
    for k in KINDS {
        let tool = Tool::Field(k);
        if theme::tool_row(ui, kind_icon(k), &tr(kind_label(k)), app.tool == tool).clicked() {
            app.tool = tool;
            app.picked = None;
        }
    }
    ui.add_space(8.0);
    theme::para(ui, app.tool.hint(), 12.0, Some(p.muted));
}
