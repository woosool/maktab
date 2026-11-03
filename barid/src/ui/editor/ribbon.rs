use super::*;
fn group(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.set_min_height(76.0);
        content(ui);
        ui.add_space(3.0);
        ui.label(crate::i18n::widget(
            RichText::new(crate::i18n::display(label))
                .size(10.0)
                .color(MUTED),
            ui.style(),
        ));
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 88.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, theme::line(ui.visuals().dark_mode)),
    );
}
fn color_menu(
    ui: &mut egui::Ui,
    label: &str,
    highlight: bool,
    command: &mut Option<EditorCommand>,
) {
    ui.menu_button(icon_label(label), |ui| {
        ui.horizontal_wrapped(|ui| {
            for rgb in if highlight {
                vec![
                    [255, 244, 120],
                    [188, 236, 193],
                    [192, 220, 255],
                    [255, 206, 221],
                    [224, 209, 249],
                    [255, 255, 255],
                ]
            } else {
                vec![
                    [42, 44, 47],
                    [24, 118, 224],
                    [204, 52, 52],
                    [39, 135, 88],
                    [133, 74, 178],
                    [237, 133, 20],
                ]
            } {
                if ui
                    .add(action_button("    ").fill(Color32::from_rgb(rgb[0], rgb[1], rgb[2])))
                    .clicked()
                {
                    *command = Some(EditorCommand::Mark(if highlight {
                        Mark::Highlight(rgb)
                    } else {
                        Mark::Color(rgb)
                    }));
                    ui.close();
                }
            }
        });
    });
}
pub(super) fn show(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    state: &mut EditorState,
) -> Option<EditorCommand> {
    if state.inline || ui.available_width() < 720.0 {
        return compact(ui, draft, state);
    }
    let mut command = None;
    ui.horizontal(|ui| {
        for (i, label) in ["Message", "Insert", "Format Text", "Review"]
            .iter()
            .enumerate()
        {
            if ui
                .selectable_label(
                    state.ribbon_tab == i,
                    crate::i18n::widget(crate::i18n::display(label), ui.style()),
                )
                .clicked()
            {
                state.ribbon_tab = i;
                state.ribbon_collapsed = false;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button(if state.ribbon_collapsed { "⌄" } else { "⌃" })
                .on_hover_text(crate::i18n::display("Show / hide ribbon"))
                .clicked()
            {
                state.ribbon_collapsed = !state.ribbon_collapsed;
            }
        });
    });
    if !state.ribbon_collapsed {
        egui::Frame::new()
            .fill(if ui.visuals().dark_mode {
                Color32::from_gray(34)
            } else {
                Color32::from_rgb(249, 249, 250)
            })
            .stroke(Stroke::new(1.0, theme::line(ui.visuals().dark_mode)))
            .corner_radius(5.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.set_max_height(110.0);
                egui::ScrollArea::both()
                    .id_salt(("outlook_ribbon", &draft.id))
                    .max_height(105.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_max_height(100.0);
                        ui.horizontal_top(|ui| {
                            group(ui, "Clipboard", |ui| {
                                if ui
                                    .add_sized(
                                        [90.0, 35.0],
                                        action_button("Paste")
                                            .wrap_mode(egui::TextWrapMode::Extend),
                                    )
                                    .on_hover_text(crate::i18n::display(
                                        "Paste with original formatting · Ctrl/Cmd+V",
                                    ))
                                    .clicked()
                                {
                                    command = Some(EditorCommand::Paste);
                                }
                                if ui
                                    .add(action_button("Paste plain").small())
                                    .on_hover_text(crate::i18n::display(
                                        "Paste text only · Ctrl/Cmd+Shift+V",
                                    ))
                                    .clicked()
                                {
                                    command = Some(EditorCommand::PastePlain);
                                }
                            });
                            if state.ribbon_tab == 0 || state.ribbon_tab == 2 {
                                group(ui, "Message format", |ui| {
                                    ui.selectable_value(
                                        &mut draft.mode,
                                        MessageMode::Html,
                                        "Aa HTML",
                                    );
                                    ui.selectable_value(
                                        &mut draft.mode,
                                        MessageMode::Plain,
                                        "Aa Plain Text",
                                    );
                                });
                            }
                            ui.add_enabled_ui(draft.mode == MessageMode::Html, |ui| {
                                if state.ribbon_tab == 0 || state.ribbon_tab == 2 {
                                    group(ui, "Font", |ui| {
                                        ui.horizontal(|ui| {
                                            crate::ui::theme::named_combo("Font", egui::ComboBox::from_id_salt((
                                                "ribbon_font",
                                                &draft.id,
                                            ))
                                            .width(110.0)
                                            .selected_text(
                                                match state
                                                    .typing
                                                    .iter()
                                                    .rev()
                                                    .find_map(|m| {
                                                        if let Mark::Family(f) = m {
                                                            Some(f.clone())
                                                        } else {
                                                            None
                                                        }
                                                    })
                                                    .or_else(|| {
                                                        draft
                                                            .rich
                                                            .spans
                                                            .iter()
                                                            .filter(|s| {
                                                                s.start <= state.selection.start
                                                                    && state.selection.start < s.end
                                                            })
                                                            .find_map(|s| {
                                                                if let Mark::Family(f) = &s.mark {
                                                                    Some(f.clone())
                                                                } else {
                                                                    None
                                                                }
                                                            })
                                                    })
                                                    .unwrap_or_default()
                                                {
                                                    FontFamily::Sans => "Arial",
                                                    FontFamily::Serif => "Georgia",
                                                    FontFamily::Mono => "Courier New",
                                                },
                                            )
                                            .show_ui(
                                                ui,
                                                |ui| {
                                                    for (label, family) in [
                                                        ("Arial", FontFamily::Sans),
                                                        ("Georgia", FontFamily::Serif),
                                                        ("Courier New", FontFamily::Mono),
                                                    ] {
                                                        if ui.add(action_button(label)).clicked() {
                                                            command = Some(EditorCommand::Mark(
                                                                Mark::Family(family),
                                                            ));
                                                            ui.close();
                                                        }
                                                    }
                                                },
                                            ));
                                            crate::ui::theme::named_combo("Font size", egui::ComboBox::from_id_salt((
                                                "ribbon_size",
                                                &draft.id,
                                            ))
                                            .width(44.0)
                                            .selected_text(
                                                state
                                                    .typing
                                                    .iter()
                                                    .rev()
                                                    .find_map(|m| {
                                                        if let Mark::Size(s) = m {
                                                            Some(*s)
                                                        } else {
                                                            None
                                                        }
                                                    })
                                                    .or_else(|| {
                                                        draft
                                                            .rich
                                                            .spans
                                                            .iter()
                                                            .filter(|s| {
                                                                s.start <= state.selection.start
                                                                    && state.selection.start < s.end
                                                            })
                                                            .find_map(|s| {
                                                                if let Mark::Size(size) = s.mark {
                                                                    Some(size)
                                                                } else {
                                                                    None
                                                                }
                                                            })
                                                    })
                                                    .unwrap_or(14)
                                                    .to_string(),
                                            )
                                            .show_ui(
                                                ui,
                                                |ui| {
                                                    for size in [10, 12, 14, 16, 18, 24, 28, 32, 48]
                                                    {
                                                        if ui
                                                            .add(action_button(format!("{size}")))
                                                            .clicked()
                                                        {
                                                            command = Some(EditorCommand::Mark(
                                                                Mark::Size(size),
                                                            ));
                                                            ui.close();
                                                        }
                                                    }
                                                },
                                            ));
                                        });
                                        ui.horizontal(|ui| {
                                            for (label, mark, hint) in [
                                                ("B", Mark::Bold, "Bold"),
                                                ("I", Mark::Italic, "Italic"),
                                                ("U", Mark::Underline, "Underline"),
                                                ("S", Mark::Strike, "Strikethrough"),
                                                ("x₂", Mark::Subscript, "Subscript"),
                                                ("x²", Mark::Superscript, "Superscript"),
                                            ] {
                                                if ui
                                                    .add_sized(
                                                        [23.0, 24.0],
                                                        action_button(RichText::new(label).font(
                                                            if label == "B" {
                                                                bold_font(13.0)
                                                            } else {
                                                                egui::FontId::proportional(13.0)
                                                            },
                                                        )),
                                                    )
                                                    .on_hover_text(hint)
                                                    .clicked()
                                                {
                                                    command = Some(EditorCommand::Mark(mark));
                                                }
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            crate::ui::theme::divider(ui);
                    color_menu(ui, "Text color", false, &mut command);
                                            color_menu(ui, "Highlight", true, &mut command);
                                            if ui
                                                .add(action_button("Clear").small())
                                                .on_hover_text(crate::i18n::display(
                                                    "Clear selection formatting",
                                                ))
                                                .clicked()
                                            {
                                                command = Some(EditorCommand::Clear);
                                            }
                                        });
                                    });
                                    group(ui, "Paragraph", |ui| {
                                        ui.horizontal(|ui| {
                                            for (label, align) in [
                                                ("Left", Alignment::Left),
                                                ("Center", Alignment::Center),
                                                ("Right", Alignment::Right),
                                            ] {
                                                if ui
                                                    .selectable_label(
                                                        draft.rich.alignment == align,
                                                        icon_label(label),
                                                    )
                                                    .clicked()
                                                {
                                                    command = Some(EditorCommand::Align(align));
                                                }
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            if ui.add(action_button("• List").small()).clicked() {
                                                command = Some(EditorCommand::Bullet);
                                            }
                                            if ui.add(action_button("1. List").small()).clicked() {
                                                command = Some(EditorCommand::Numbered);
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            if ui.add(action_button("← Outdent").small()).clicked()
                                            {
                                                command = Some(EditorCommand::Outdent);
                                            }
                                            if ui.add(action_button("Indent →").small()).clicked()
                                            {
                                                command = Some(EditorCommand::Indent);
                                            }
                                        });
                                    });
                                }
                                if state.ribbon_tab == 1 {
                                    group(ui, "Illustrations", |ui| {
                                        if tool_button(ui, "Picture", "image", false).clicked() {
                                            command = Some(EditorCommand::Image);
                                        }
                                        ui.label(crate::i18n::widget(
                                            RichText::new(crate::i18n::display(
                                                "PNG · JPEG · GIF · WebP",
                                            ))
                                            .size(10.0)
                                            .color(MUTED),
                                            ui.style(),
                                        ));
                                    });
                                    group(ui, "Links", |ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut state.link)
                                                .hint_text(crate::i18n::display(
                                                    "https://example.com",
                                                ))
                                                .desired_width(210.0),
                                        );
                                        if ui
                                            .add(action_button("Insert link on selection"))
                                            .clicked()
                                        {
                                            if safe_link(&state.link) {
                                                command = Some(EditorCommand::Mark(Mark::Link(
                                                    state.link.clone(),
                                                )));
                                            } else {
                                                state.error =
                                                    "Use an https://, http://, or mailto: link."
                                                        .into();
                                            }
                                        }
                                    });
                                }
                                if state.ribbon_tab == 2 {
                                    group(ui, "Styles", |ui| {
                                        ui.menu_button(icon_label("Styles"), |ui| {
                                            for (label, marks) in [
                                                (
                                                    "Normal",
                                                    vec![
                                                        Mark::Size(14),
                                                        Mark::Family(FontFamily::Sans),
                                                    ],
                                                ),
                                                ("Heading 1", vec![Mark::Size(28), Mark::Bold]),
                                                ("Heading 2", vec![Mark::Size(22), Mark::Bold]),
                                                (
                                                    "Code",
                                                    vec![
                                                        Mark::Family(FontFamily::Mono),
                                                        Mark::Size(13),
                                                    ],
                                                ),
                                            ] {
                                                if ui.add(action_button(label)).clicked() {
                                                    command = Some(EditorCommand::Style(marks));
                                                    ui.close();
                                                }
                                            }
                                        });
                                        if ui.add(action_button("Clear style").small()).clicked() {
                                            command = Some(EditorCommand::Clear);
                                        }
                                    });
                                }
                            });
                            if state.ribbon_tab == 3 {
                                group(ui, "Find & replace", |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut state.find)
                                                .hint_text(crate::i18n::display(
                                                    "Find text (case-sensitive)",
                                                ))
                                                .desired_width(180.0),
                                        );
                                        if ui.add(action_button("Find next")).clicked() {
                                            find_next(draft, state);
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut state.replace)
                                                .hint_text(crate::i18n::display("Replace with"))
                                                .desired_width(180.0),
                                        );
                                        if ui.add(action_button("Replace")).clicked()
                                            && !state.find.is_empty()
                                        {
                                            if draft
                                                .body
                                                .chars()
                                                .skip(state.selection.start)
                                                .take(
                                                    state
                                                        .selection
                                                        .end
                                                        .saturating_sub(state.selection.start),
                                                )
                                                .collect::<String>()
                                                != state.find
                                            {
                                                find_next(draft, state);
                                            }
                                            let selected = draft
                                                .body
                                                .chars()
                                                .skip(state.selection.start)
                                                .take(
                                                    state
                                                        .selection
                                                        .end
                                                        .saturating_sub(state.selection.start),
                                                )
                                                .collect::<String>();
                                            if selected == state.find {
                                                state.checkpoint(draft);
                                                replace_selection(
                                                    draft,
                                                    state,
                                                    &state.replace.clone(),
                                                );
                                            }
                                        }
                                    });
                                    if ui.add(action_button("Replace all")).clicked()
                                        && !state.find.is_empty()
                                    {
                                        state.checkpoint(draft);
                                        let ranges: Vec<_> = draft
                                            .body
                                            .match_indices(&state.find)
                                            .map(|(start, text)| {
                                                (
                                                    draft.body[..start].chars().count(),
                                                    text.chars().count(),
                                                )
                                            })
                                            .collect();
                                        for (start, len) in ranges.into_iter().rev() {
                                            state.selection = start..start + len;
                                            replace_selection(draft, state, &state.replace.clone());
                                        }
                                    }
                                });
                            }
                            group(ui, "Zoom", |ui| {
                                crate::ui::theme::named_combo("Zoom", egui::ComboBox::from_id_salt(("editor_zoom", &draft.id))
                                    .width(55.0)
                                    .selected_text(format!("{}%", state.zoom))
                                    .show_ui(ui, |ui| {
                                        for zoom in [75, 100, 125, 150, 200] {
                                            ui.selectable_value(
                                                &mut state.zoom,
                                                zoom,
                                                format!("{zoom}%"),
                                            );
                                        }
                                    }));
                                ui.label(crate::i18n::widget(
                                    RichText::new(crate::i18n::display("View only"))
                                        .size(10.0)
                                        .color(MUTED),
                                    ui.style(),
                                ));
                            });
                        });
                    });
            });
    }
    ui.horizontal(|ui| {
        ui.label(crate::i18n::widget(
            RichText::new(crate::i18n::display(if draft.mode == MessageMode::Html {
                "HTML message · styled paste"
            } else {
                "Plain text · formatting and inline images are omitted"
            }))
            .size(10.0)
            .color(MUTED),
            ui.style(),
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.selectable_value(
                &mut state.preview,
                true,
                crate::i18n::widget(crate::i18n::display("Preview"), ui.style()),
            );
            ui.selectable_value(
                &mut state.preview,
                false,
                crate::i18n::widget(crate::i18n::display("Edit"), ui.style()),
            );
        });
    });
    ui.separator();
    command
}
fn find_next(draft: &Draft, state: &mut EditorState) {
    if state.find.is_empty() {
        return;
    }
    let start = char_byte(&draft.body, state.selection.end);
    let found = draft.body[start..]
        .find(&state.find)
        .map(|i| i + start)
        .or_else(|| draft.body.find(&state.find));
    if let Some(index) = found {
        let start = draft.body[..index].chars().count();
        state.selection = start..start + state.find.chars().count();
        state.focus = true;
        state.preview = false;
    } else {
        state.error = "No matching text found.".into();
    }
}

// Narrow composers use one wrapping toolbar rather than clipping a desktop ribbon.
fn compact(ui: &mut egui::Ui, draft: &mut Draft, state: &mut EditorState) -> Option<EditorCommand> {
    let mut command = None;
    ui.horizontal(|ui| {
        crate::ui::theme::ghost_controls(ui);
        if crate::ui::theme::icon_toggle(ui, "text", "Formatting", !state.ribbon_collapsed)
            .clicked()
        {
            state.ribbon_collapsed = !state.ribbon_collapsed;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut tab = usize::from(state.preview);
            crate::ui::theme::segmented(ui, &mut tab, &["Write", "Preview"]);
            state.preview = tab == 1;
            crate::ui::theme::named_combo("Message format", egui::ComboBox::from_id_salt(("compact_mode", &draft.id))
                .width(84.0)
                .selected_text(crate::i18n::display(if draft.mode == MessageMode::Html {
                    "HTML"
                } else {
                    "Plain text"
                }))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut draft.mode,
                        MessageMode::Html,
                        crate::i18n::widget(crate::i18n::display("HTML"), ui.style()),
                    );
                    ui.selectable_value(
                        &mut draft.mode,
                        MessageMode::Plain,
                        crate::i18n::widget(crate::i18n::display("Plain text"), ui.style()),
                    );
                }));
        });
    });
    if !state.ribbon_collapsed && !state.preview && draft.mode == MessageMode::Html {
        ui.horizontal_wrapped(|ui| {
            crate::ui::theme::ghost_controls(ui);

                    for (label, mark, hint) in [
                        (RichText::new("B").font(crate::ui::bold_font(14.0)), Mark::Bold, "Bold"),
                        (RichText::new("I").italics(), Mark::Italic, "Italic"),
                        (RichText::new("U").underline(), Mark::Underline, "Underline"),
                        (RichText::new("S").strikethrough(), Mark::Strike, "Strikethrough"),
                    ] {
                        let active = state.typing.contains(&mark);
                        if ui
                            .add(
                                action_button(label.size(14.0))
                                    .selected(active)
                                    .min_size(egui::vec2(30.0, 30.0)),
                            )
                            .on_hover_text(crate::i18n::display(hint))
                            .clicked()
                        {
                            command = Some(EditorCommand::Mark(mark));
                        }
                    }
                    color_menu(ui, "Text color", false, &mut command);
                    ui.menu_button(icon_label("Paragraph"), |ui| {
                        for (label, cmd) in [
                            ("Align left", EditorCommand::Align(Alignment::Left)),
                            ("Align center", EditorCommand::Align(Alignment::Center)),
                            ("Align right", EditorCommand::Align(Alignment::Right)),
                            ("Bulleted list", EditorCommand::Bullet),
                            ("Numbered list", EditorCommand::Numbered),
                            ("Indent", EditorCommand::Indent),
                            ("Outdent", EditorCommand::Outdent),
                        ] {
                            if ui.add(action_button(label)).clicked() {
                                command = Some(cmd);
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button(icon_label("More"), |ui| {
                        for (label, cmd) in [
                            ("Paste", EditorCommand::Paste),
                            ("Paste plain", EditorCommand::PastePlain),
                            ("Insert image…", EditorCommand::Image),
                            ("Clear formatting", EditorCommand::Clear),
                            ("Undo", EditorCommand::Undo),
                            ("Redo", EditorCommand::Redo),
                        ] {
                            if ui.add(action_button(label)).clicked() {
                                command = Some(cmd);
                                ui.close();
                            }
                        }
                        ui.menu_button(icon_label("Font"), |ui| {
                            for (label, family) in [
                                ("Arial", FontFamily::Sans),
                                ("Georgia", FontFamily::Serif),
                                ("Monospace", FontFamily::Mono),
                            ] {
                                if ui.add(action_button(label)).clicked() {
                                    command = Some(EditorCommand::Mark(Mark::Family(family)));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            for size in [10, 12, 14, 16, 18, 24, 32] {
                                if ui.add(action_button(format!("{size} px"))).clicked() {
                                    command = Some(EditorCommand::Mark(Mark::Size(size)));
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button(icon_label("Insert link"), |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.link)
                                    .hint_text(crate::i18n::display("https://example.com")),
                            );
                            if ui
                                .add_enabled(safe_link(&state.link), action_button("Apply link"))
                                .clicked()
                            {
                                command = Some(EditorCommand::Mark(Mark::Link(state.link.clone())));
                                ui.close();
                            }
                        });
                        color_menu(ui, "Highlight", true, &mut command);
                    });
        });
    }
    ui.add_space(2.0);
    ui.separator();
    command
}
