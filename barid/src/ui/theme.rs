use super::*;

/// Design tokens shared by every widget, in light and dark appearance.
pub(super) struct Palette {
    pub surface: Color32,
    pub canvas: Color32,
    pub raised: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub input: Color32,
    pub line: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_text: Color32,
}
pub(super) fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            surface: Color32::from_rgb(28, 30, 34),
            canvas: Color32::from_rgb(22, 24, 27),
            raised: Color32::from_rgb(36, 39, 44),
            control: Color32::from_rgb(44, 48, 55),
            control_hover: Color32::from_rgb(54, 59, 67),
            input: Color32::from_rgb(20, 22, 25),
            line: Color32::from_rgb(46, 50, 57),
            border: Color32::from_rgb(64, 69, 78),
            border_strong: Color32::from_rgb(92, 99, 110),
            text: Color32::from_rgb(230, 233, 237),
            text_muted: Color32::from_rgb(155, 162, 172),
            accent: Color32::from_rgb(77, 154, 255),
            accent_soft: Color32::from_rgb(33, 55, 84),
            accent_text: Color32::from_rgb(160, 202, 255),
        }
    } else {
        Palette {
            surface: Color32::WHITE,
            canvas: Color32::from_rgb(247, 248, 250),
            raised: Color32::WHITE,
            control: Color32::from_rgb(243, 244, 247),
            control_hover: Color32::from_rgb(232, 235, 240),
            input: Color32::WHITE,
            line: Color32::from_rgb(232, 235, 240),
            border: Color32::from_rgb(208, 213, 221),
            border_strong: Color32::from_rgb(152, 160, 172),
            text: INK,
            text_muted: Color32::from_rgb(102, 109, 120),
            accent: BLUE,
            accent_soft: Color32::from_rgb(214, 230, 251),
            accent_text: Color32::from_rgb(18, 98, 196),
        }
    }
}
pub(super) fn line(dark: bool) -> Color32 {
    palette(dark).line
}
pub(super) fn layout() -> egui::Layout {
    egui::Layout::top_down(if crate::i18n::language().rtl() {
        egui::Align::Max
    } else {
        egui::Align::Min
    })
}
pub(super) fn scope<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(layout(), content).inner
}
pub(super) fn visuals(dark: bool) -> egui::Visuals {
    use egui::{CornerRadius, Shadow};
    let p = palette(dark);
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.text_muted);
    v.hyperlink_color = p.accent_text;
    v.panel_fill = p.surface;
    v.window_fill = p.raised;
    v.faint_bg_color = p.canvas;
    v.extreme_bg_color = p.input;
    v.code_bg_color = p.control;
    v.text_edit_bg_color = Some(p.input);
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_highlight_topmost = false;
    v.window_shadow = Shadow {
        offset: [0, 12],
        blur: 36,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 38 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 90 } else { 30 }),
    };
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent_text);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.striped = false;

    let radius = CornerRadius::same(6);
    let w = &mut v.widgets;
    // Static content: labels, separators, frames.
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text_muted);
    w.noninteractive.corner_radius = radius;
    // Resting controls: buttons, inputs, checkboxes.
    w.inactive.weak_bg_fill = p.control;
    w.inactive.bg_fill = p.control;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.5, p.text);
    w.inactive.corner_radius = radius;
    w.inactive.expansion = 0.0;
    // Hover.
    w.hovered.weak_bg_fill = p.control_hover;
    w.hovered.bg_fill = p.control_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;
    // Pressed / focused.
    w.active.weak_bg_fill = p.accent_soft;
    w.active.bg_fill = p.accent_soft;
    w.active.bg_stroke = Stroke::new(1.5, p.accent);
    // egui derives strong text from this, so keep it neutral.
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;
    // Open menus and combo boxes.
    w.open.weak_bg_fill = p.accent_soft;
    w.open.bg_fill = p.accent_soft;
    w.open.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.fg_stroke = Stroke::new(1.5, p.accent_text);
    w.open.corner_radius = radius;
    v
}
/// Frameless toolbar controls: no fill or border until hovered.
pub(super) fn ghost_controls(ui: &mut egui::Ui) {
    let w = &mut ui.visuals_mut().widgets;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.bg_stroke = Stroke::NONE;
    ui.spacing_mut().button_padding = Vec2::new(8.0, 5.0);
    ui.spacing_mut().item_spacing.x = 2.0;
}
/// Square icon button that shows an on/off state.
pub(super) fn icon_toggle(
    ui: &mut egui::Ui,
    icon: &str,
    hint: &str,
    selected: bool,
) -> egui::Response {
    let p = palette(ui.visuals().dark_mode);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::click());
    let hint = crate::i18n::display(hint);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), selected, &hint)
    });
    let fill = if selected {
        p.accent_soft
    } else if response.hovered() {
        p.control_hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    draw_icon(
        ui.painter(),
        rect.center(),
        icon,
        if selected { p.accent_text } else { p.text },
        0.85,
    );
    response.on_hover_text(hint)
}
/// Thin vertical rule between toolbar groups.
pub(super) fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 20.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0, palette(ui.visuals().dark_mode).line),
    );
}
/// Segmented control: a track with one raised, selected segment.
pub(super) fn segmented(ui: &mut egui::Ui, value: &mut usize, labels: &[&str]) {
    let p = palette(ui.visuals().dark_mode);
    let font = egui::FontId::proportional(13.0);
    let galleys: Vec<_> = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(crate::i18n::display(l), font.clone(), p.text)
        })
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 24.0).collect();
    let size = Vec2::new(widths.iter().sum::<f32>() + 4.0, 30.0);
    let (track, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(track, 7.0, p.control);
    let mut x = track.left() + 2.0;
    for (i, (galley, width)) in galleys.into_iter().zip(widths).enumerate() {
        let rect = egui::Rect::from_min_size(
            egui::pos2(x, track.top() + 2.0),
            Vec2::new(width, track.height() - 4.0),
        );
        x += width;
        let response = ui.interact(rect, ui.scope_id().with(("segment", labels[i])), egui::Sense::click());
        let selected = *value == i;
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::RadioButton,
                ui.is_enabled(),
                selected,
                crate::i18n::display(labels[i]),
            )
        });
        if response.clicked() {
            *value = i;
        }
        if selected {
            ui.painter().rect(
                rect,
                5.0,
                p.raised,
                Stroke::new(1.0, p.border),
                egui::StrokeKind::Inside,
            );
        }
        let color = if selected || response.hovered() {
            p.text
        } else {
            p.text_muted
        };
        ui.painter().galley_with_override_text_color(
            rect.center() - galley.size() / 2.0,
            galley,
            color,
        );
    }
}
/// Gives an unlabeled ComboBox an accessible name for screen readers.
pub(super) fn named_combo<R>(
    name: &str,
    shown: egui::InnerResponse<Option<R>>,
) -> egui::InnerResponse<Option<R>> {
    let enabled = shown.response.enabled();
    let name = crate::i18n::display(name);
    shown
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, &name));
    shown
}
/// Filled blue call-to-action with white text and icon.
pub(super) fn primary_button(
    ui: &mut egui::Ui,
    size: impl Into<Vec2>,
    label: &str,
) -> egui::Response {
    let size = size.into();
    ui.scope(|ui| {
        let v = &mut ui.style_mut().visuals;
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.fg_stroke.color = Color32::WHITE;
            w.bg_stroke = Stroke::NONE;
        }
        v.override_text_color = Some(Color32::WHITE);
        ui.add_sized(size, action_button(label).fill(BLUE).corner_radius(6.0))
    })
    .inner
}
pub(super) fn install(ctx: &egui::Context, scale: f32) {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let mut style = (*ctx.style_of(theme)).clone();
        style.visuals = visuals(theme == egui::Theme::Dark);
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.icon_width = 16.0;
        style.spacing.icon_width_inner = 9.0;
        style.spacing.icon_spacing = 8.0;
        style.spacing.slider_rail_height = 4.0;
        style.spacing.combo_width = 180.0;
        style.spacing.text_edit_width = 260.0;
        style.spacing.window_margin = egui::Margin::same(16);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.scroll = egui::style::ScrollStyle::floating();
        style.interaction.selectable_labels = false;
        for (text, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 13.0),
            (egui::TextStyle::Heading, 19.0),
            (egui::TextStyle::Small, 11.0),
        ] {
            style.text_styles.insert(
                text,
                egui::FontId::proportional(size * scale.clamp(0.85, 1.3)),
            );
        }
        ctx.set_style_of(theme, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn message_actions_are_over_the_reader_and_disabled_without_a_message() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        app.selected = None;
        let ctx = egui::Context::default();
        install(&ctx, 1.0);
        ctx.set_visuals(visuals(false));
        let mut forward = None;
        for phase in 0..4 {
            let events = match (phase, forward) {
                (1, Some(pos)) => vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                (2, Some(pos)) => vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                _ => vec![],
            };
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.toolbar(ui);
                    app.sidebar(ui);
                    app.mail_list(ui);
                    app.reader(ui);
                },
            );
            for shape in &out.shapes {
                if let egui::Shape::Text(t) = &shape.shape
                    && t.galley.text() == "Forward"
                {
                    assert!(t.pos.x > 500.0 && t.pos.y > 80.0);
                    forward = Some(t.pos + egui::vec2(3.0, 3.0));
                }
            }
            out.textures_delta.clear();
        }
        assert!(forward.is_some());
        assert!(app.composer.is_none());
        assert!(app.dialogs.quick_reply.is_none());
    }
    #[test]
    fn arabic_row_checkbox_hit_region_is_on_the_right() {
        crate::i18n::set_language(crate::i18n::Language::Arabic);
        let ctx = egui::Context::default();
        let mail = Mail {
            id: "test".into(),
            from: "sender@example.com".into(),
            subject: "test".into(),
            ..Default::default()
        };
        let mut check = None;
        let mut toggled = false;
        for phase in 0..3 {
            let events = match (phase, check) {
                (1, Some(pos)) => vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                (2, Some(pos)) => vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                _ => vec![],
            };
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(500.0, 200.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    scope(ui, |ui| {
                        assert!(ui.layout().prefer_right_to_left());
                        let row = message_row(ui, &mail, false, false, true, false, true);
                        check = Some(row.0.rect.right_center() - egui::vec2(13.0, 0.0));
                        toggled |= row.1;
                    });
                },
            );
            out.textures_delta.clear();
        }
        assert!(toggled);
        crate::i18n::set_language(crate::i18n::Language::English);
    }
}

pub(super) fn navigation_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 38.0), egui::Sense::click());
    let rtl = crate::i18n::language().rtl();
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            6.0,
            if selected {
                palette(ui.visuals().dark_mode).accent_soft
            } else {
                ui.visuals().widgets.hovered.weak_bg_fill
            },
        );
    }
    let color = if selected {
        palette(ui.visuals().dark_mode).accent_text
    } else {
        ui.visuals().text_color()
    };
    draw_icon(
        ui.painter(),
        egui::pos2(
            if rtl {
                rect.right() - 18.0
            } else {
                rect.left() + 18.0
            },
            rect.center().y,
        ),
        icons::icon_for_label(label),
        color,
        0.85,
    );
    let job = crate::i18n::widget(
        RichText::new(crate::i18n::display(label))
            .size(13.0)
            .color(color),
        ui.style(),
    )
    .into_layout_job(ui.style(), egui::FontSelection::Default, egui::Align::Min);
    let mut job = (*job).clone();
    job.wrap.max_width = (width - 48.0).max(30.0);
    job.wrap.max_rows = 1;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        egui::pos2(
            if rtl {
                rect.right() - 36.0 - galley.size().x
            } else {
                rect.left() + 36.0
            },
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Tab, true, selected, crate::i18n::display(label))
    });
    response
}

#[cfg(test)]
mod screenshot_regressions {
    use super::*;
    #[test]
    fn arabic_reader_keeps_english_mail_inside_the_reading_pane_and_reply_is_compact() {
        crate::i18n::set_language(crate::i18n::Language::Arabic);
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        app.prefs.language = crate::i18n::Language::Arabic;
        app.mails = vec![Mail {
            id: "rtl-mail".into(),
            folder: "Inbox".into(),
            from: "Sender <sender@example.com>".into(),
            subject: "English subject".into(),
            body: "English message content".into(),
            ..Default::default()
        }];
        app.selected = Some("rtl-mail".into());
        app.start_quick_reply("rtl-mail", false);
        let id = app.dialogs.quick_reply.as_ref().unwrap().draft.id.clone();
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        install(&ctx, 1.0);
        let mut found = false;
        for _ in 0..2 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 1000.0),
                    )),
                    ..Default::default()
                },
                |ui| app.reader(ui),
            );
            for clipped in &output.shapes {
                if let egui::Shape::Text(t) = &clipped.shape
                    && t.galley.text() == "English message content"
                {
                    found = true;
                    assert!(t.pos.x >= clipped.clip_rect.left());
                    assert!(t.pos.x + t.galley.size().x <= clipped.clip_rect.right());
                }
            }
            output.textures_delta.clear();
        }
        assert!(found);
        let editor = &app.editor_states[&id];
        assert!(editor.inline);
        assert!(editor.ribbon_collapsed);
        crate::i18n::set_language(crate::i18n::Language::English);
    }
    #[test]
    fn sidebar_groups_hide_and_restore_children_without_changing_the_folder() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        let ctx = egui::Context::default();
        let folder = app.folder.clone();
        for collapsed in [false, true, false] {
            if collapsed {
                app.collapsed_groups.insert("favorites".into());
            } else {
                app.collapsed_groups.remove("favorites");
            }
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1040.0, 760.0),
                    )),
                    ..Default::default()
                },
                |ui| app.sidebar(ui),
            );
            let visible = out
                .shapes
                .iter()
                .any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="All Inboxes"));
            assert_eq!(visible, !collapsed);
            assert_eq!(app.folder, folder);
            out.textures_delta.clear();
        }
    }
}

#[cfg(test)]
mod live_menu_regressions {
    use super::*;
    #[test]
    fn workspace_popup_content_survives_sizing_and_switching_in_all_languages() {
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Arabic,
            crate::i18n::Language::French,
        ] {
            crate::i18n::set_language(language);
            let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
            app.prefs.language = language;
            let ctx = egui::Context::default();
            configure_fonts(&ctx);
            install(&ctx, 1.0);
            for label in [
                "Windows", "Mark", "Snooze", "More", "Inboxes", "Filter", "Label", "Windows",
            ] {
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::unique("test_forced_tool_menu"), label.to_owned());
                    d.remove::<String>(egui::Id::unique("test_opened_tool_menu"));
                });
                for _ in 0..3 {
                    let mut out = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                Vec2::new(1440.0, 1000.0),
                            )),
                            ..Default::default()
                        },
                        |ui| app.workspace_ui(ui),
                    );
                    out.textures_delta.clear();
                }
                assert_eq!(
                    ctx.data(|d| d.get_temp::<String>(egui::Id::unique("test_opened_tool_menu")))
                        .as_deref(),
                    Some(label),
                    "Popup did not render in {language:?}"
                );
            }
        }
        crate::i18n::set_language(crate::i18n::Language::English);
    }
}
