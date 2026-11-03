use super::*;

#[derive(Default)]
pub(super) struct DialogState {
    pub section: usize,
    pub raise_compose: Option<String>,
    pub compose_positions: std::collections::HashMap<String, egui::Pos2>,
    pub contact_query: String,
    pub contact_filter: String,
    pub contact_form: Contact,
    pub contact_selected: String,
    pub contact_editing: bool,
    pub contact_error: String,
    pub compose_error: String,
    pub picker_query: String,
    pub show_cc: bool,
    pub show_bcc: bool,
    pub compose_minimized: bool,
    pub compose_expanded: bool,
    pub quick_reply: Option<QuickReply>,
    pub focus_quick_reply: bool,
    pub compose_offset: f32,
    pub compose_slot: usize,
    pub about: bool,
}

pub(super) struct ComposeSession {
    pub slot: usize,
    pub draft: Draft,
    pub minimized: bool,
    pub expanded: bool,
    pub cc: bool,
    pub bcc: bool,
    pub error: String,
}
impl ComposeSession {
    fn capture(draft: Draft, state: &DialogState) -> Self {
        Self {
            draft,
            slot: state.compose_slot,
            minimized: state.compose_minimized,
            expanded: state.compose_expanded,
            cc: state.show_cc,
            bcc: state.show_bcc,
            error: state.compose_error.clone(),
        }
    }
}
pub(super) struct QuickReply {
    pub message: String,
    pub draft: Draft,
    pub all: bool,
    pub error: String,
}

pub(super) fn modal_surface(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: Vec2,
    dark: bool,
    open: &mut bool,
    content: impl FnOnce(&mut egui::Ui),
) {
    let viewport = ctx.viewport_rect();
    let response = egui::Modal::new(egui::Id::unique(id))
        .frame(window_frame(dark))
        .backdrop_color(Color32::from_black_alpha(if dark { 140 } else { 90 }))
        .show(ctx, |ui| {
            theme::scope(ui, |ui| {
                ui.set_width(size.x.min(viewport.width() - 56.0).max(300.0));
                ui.set_height(size.y.min(viewport.height() - 90.0).max(250.0));
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::widget(
                        RichText::new(crate::i18n::display(title)).font(bold_font(17.0)),
                        ui.style(),
                    ));
                    ui.with_layout(
                        if crate::i18n::language().rtl() {
                            egui::Layout::left_to_right(egui::Align::Center)
                        } else {
                            egui::Layout::right_to_left(egui::Align::Center)
                        },
                        |ui| {
                            if tool_button(ui, "", "close", false)
                                .on_hover_text(crate::i18n::display("Close dialog"))
                                .clicked()
                            {
                                *open = false;
                            }
                        },
                    );
                });
                ui.separator();
                ui.add_space(10.0);
                content(ui);
            })
        });
    if response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        *open = false;
    }
}
fn window_frame(dark: bool) -> egui::Frame {
    let v = theme::visuals(dark);
    egui::Frame::new()
        .fill(v.window_fill)
        .corner_radius(v.window_corner_radius)
        .stroke(Stroke::new(1.0, theme::palette(dark).border))
        .shadow(v.window_shadow)
        .inner_margin(egui::Margin::symmetric(20, 16))
}
pub(super) fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(crate::i18n::widget(
        RichText::new(crate::i18n::display(title)).font(bold_font(21.0)),
        ui.style(),
    ));
    ui.add_space(3.0);
    ui.label(crate::i18n::widget(
        RichText::new(crate::i18n::display(subtitle))
            .size(12.0)
            .color(MUTED),
        ui.style(),
    ));
    ui.add_space(16.0);
}
pub(super) fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(crate::i18n::widget(
        RichText::new(crate::i18n::display(title)).font(bold_font(13.0)),
        ui.style(),
    ));
    ui.add_space(7.0);
}
pub(super) fn field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(crate::i18n::widget(
        RichText::new(crate::i18n::display(label))
            .size(12.0)
            .color(MUTED),
        ui.style(),
    ));
    let machine = [
        "email", "server", "host", "client", "uri", "url", "password", "port",
    ]
    .iter()
    .any(|part| label.to_lowercase().contains(part));
    ui.add(
        egui::TextEdit::singleline(value)
            .align(if crate::i18n::language().rtl() && !machine {
                egui::Align2::RIGHT_CENTER
            } else {
                egui::Align2::LEFT_CENTER
            })
            .desired_width(f32::INFINITY)
            .hint_text(crate::i18n::display(hint)),
    );
    ui.add_space(6.0);
}
fn envelope_field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [ENVELOPE_LABEL_WIDTH, ui.spacing().interact_size.y],
            egui::Label::new(
                RichText::new(crate::i18n::display(label))
                    .size(12.0)
                    .color(MUTED),
            ),
        );
        ui.add(
            egui::TextEdit::singleline(value)
                .frame(egui::Frame::NONE)
                .min_size(Vec2::new(0.0, ui.spacing().interact_size.y))
                .align(egui::Align2::LEFT_CENTER)
                .hint_text(crate::i18n::display(hint))
                .desired_width(f32::INFINITY),
        );
    });
    ui.separator();
}

impl Barid {
    pub(super) fn preferred_account(&self) -> String {
        self.account_filter
            .as_ref()
            .filter(|id| self.accounts.iter().any(|a| &a.id == *id))
            .cloned()
            .or_else(|| {
                self.accounts
                    .iter()
                    .find(|a| a.id == self.prefs.default_account)
                    .map(|a| a.id.clone())
            })
            .or_else(|| self.accounts.first().map(|a| a.id.clone()))
            .unwrap_or_default()
    }
    pub(super) fn save_local_draft(&mut self, draft: &Draft) -> Result<(), String> {
        if draft.is_empty() {
            return Ok(());
        }
        let mail = draft.as_mail();
        self.store.save("mail", &mail.id, &mail)?;
        self.mails.retain(|m| m.id != mail.id);
        self.mails.push(mail);
        Ok(())
    }
    pub(super) fn park_composer(&mut self) -> Result<(), String> {
        if let Some(mut draft) = self.composer.take() {
            if let Err(e) = self
                .flush_recipients(&mut draft)
                .and_then(|_| self.save_local_draft(&draft))
            {
                self.composer = Some(draft);
                return Err(e);
            }
            self.compose_sessions
                .push(ComposeSession::capture(draft, &self.dialogs));
        }
        Ok(())
    }
    pub(super) fn load_session(&mut self, session: ComposeSession) {
        self.dialogs.compose_slot = session.slot;
        self.dialogs.compose_minimized = session.minimized;
        self.dialogs.compose_expanded = session.expanded;
        self.dialogs.show_cc = session.cc;
        self.dialogs.show_bcc = session.bcc;
        self.dialogs.compose_error = session.error;
        self.composer = Some(session.draft);
    }
    pub(super) fn next_compose_slot(&self) -> usize {
        (0..=self.compose_sessions.len())
            .find(|slot| !self.compose_sessions.iter().any(|s| s.slot == *slot))
            .unwrap_or(self.compose_sessions.len())
    }
    pub(super) fn window_menu(&mut self, ui: &mut egui::Ui) {
        tool_menu(ui, "Windows", "expand", |ui| {
            if ui.add(action_button("Settings")).clicked() {
                self.settings = true;
                ui.close();
            }
            if ui.add(action_button("Contacts")).clicked() {
                self.contact_window = true;
                ui.close();
            }
            ui.separator();
            let mut windows: Vec<_> = self
                .compose_sessions
                .iter()
                .map(|s| (s.draft.id.clone(), s.draft.subject.clone(), s.minimized))
                .collect();
            if let Some(d) = &self.composer {
                windows.push((
                    d.id.clone(),
                    d.subject.clone(),
                    self.dialogs.compose_minimized,
                ));
            }
            ui.add_enabled_ui(!windows.is_empty(), |ui| {
                if ui.add(action_button("Arrange composers")).clicked() {
                    self.dialogs.compose_positions.clear();
                    self.dialogs.compose_expanded = false;
                    for s in &mut self.compose_sessions {
                        s.expanded = false;
                    }
                    ui.close();
                }
                if ui.add(action_button("Minimize all composers")).clicked() {
                    self.dialogs.compose_minimized = true;
                    for s in &mut self.compose_sessions {
                        s.minimized = true;
                    }
                    ui.close();
                }
                if ui.add(action_button("Restore all composers")).clicked() {
                    self.dialogs.compose_minimized = false;
                    self.dialogs.compose_expanded = false;
                    for s in &mut self.compose_sessions {
                        s.minimized = false;
                        s.expanded = false;
                    }
                    ui.close();
                }
            });
            for (id, title, minimized) in windows {
                let title = if title.is_empty() {
                    crate::i18n::display("New message")
                } else {
                    title
                };
                if ui
                    .add(
                        egui::Button::new(icon_label(RichText::new(title)))
                            .selected(id == self.focused_composer),
                    )
                    .on_hover_text(crate::i18n::display(if minimized {
                        "Restore message"
                    } else {
                        "Compose message"
                    }))
                    .clicked()
                {
                    self.focus_compose(&id);
                    ui.close();
                }
            }
        });
    }
    pub(super) fn compose_windows(&mut self, ctx: &egui::Context) {
        let mut sessions = std::mem::take(&mut self.compose_sessions);
        if let Some(draft) = self.composer.take() {
            sessions.push(ComposeSession::capture(draft, &self.dialogs));
        }
        let mut kept = Vec::new();
        let mut positions: Vec<_> = sessions
            .iter()
            .map(|s| {
                (
                    s.slot,
                    s.draft.id.clone(),
                    if s.minimized { 320.0 } else { 600.0 },
                )
            })
            .collect();
        positions.sort_by_key(|s| s.0);
        let mut offsets = std::collections::HashMap::new();
        let mut offset = 0.0;
        for (_, id, width) in positions {
            offsets.insert(id, offset);
            offset += width + 12.0;
            if offset + width > ctx.viewport_rect().width() - 24.0 {
                offset = (offset % 180.0) + 28.0;
            }
        }
        for session in sessions {
            let offset = offsets[&session.draft.id];
            self.load_session(session);
            self.dialogs.compose_offset = offset;
            self.compose_window(ctx);
            if let Some(draft) = self.composer.take() {
                kept.push(ComposeSession::capture(draft, &self.dialogs));
            }
        }
        kept.sort_by_key(|s| s.draft.id == self.focused_composer);
        if let Some(active) = kept.pop() {
            self.load_session(active);
        }
        self.compose_sessions = kept;
    }
    pub(super) fn compose_window(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.composer.take() else {
            return;
        };
        self.editor_states
            .entry(draft.id.clone())
            .or_default()
            .inline = false;
        let minimized = self.dialogs.compose_minimized;
        let expanded = self.dialogs.compose_expanded && !minimized;
        let viewport = ctx.viewport_rect();
        let size = if minimized {
            Vec2::new(320.0, 40.0)
        } else if expanded {
            Vec2::new(
                (viewport.width() - 100.0).min(1100.0),
                (viewport.height() - 100.0).max(440.0),
            )
        } else {
            Vec2::new(
                600.0_f32.min(viewport.width() - 40.0),
                640.0_f32.min(viewport.height() - 110.0),
            )
        };
        let position = if expanded {
            viewport.center() - size * 0.5
        } else {
            egui::pos2(
                (viewport.max.x - size.x - 22.0 - self.dialogs.compose_offset).max(12.0),
                viewport.max.y - size.y - 34.0,
            )
        };
        let position = if !minimized && !expanded {
            self.dialogs
                .compose_positions
                .get(&draft.id)
                .copied()
                .unwrap_or(position)
        } else {
            position
        };
        let position = egui::pos2(
            position.x.clamp(
                viewport.left() + 8.0,
                (viewport.right() - size.x - 8.0).max(viewport.left() + 8.0),
            ),
            position.y.clamp(
                viewport.top() + 8.0,
                (viewport.bottom() - size.y - 8.0).max(viewport.top() + 8.0),
            ),
        );
        let mut send = false;
        let mut save = false;
        let mut discard = false;
        let window = egui::Window::new("Compose message")
            .id(egui::Id::unique(("gmail_compose_v5", &draft.id)))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .min_size(Vec2::ZERO)
            .fixed_pos(position)
            .fixed_size(size)
            .frame(
                egui::Frame::new()
                    .fill(surface(self.dark))
                    .corner_radius(12.0)
                    .stroke(Stroke::new(1.0, theme::palette(self.dark).border))
                    .shadow(theme::visuals(self.dark).window_shadow)
                    .inner_margin(0.0),
            )
            .show(ctx, |ui| {
                theme::scope(ui, |ui| {
                    ui.set_width(size.x);
                    egui::Frame::new()
                        .fill(theme::palette(self.dark).canvas)
                        .corner_radius(egui::CornerRadius {
                            nw: 12,
                            ne: 12,
                            sw: if minimized { 12 } else { 0 },
                            se: if minimized { 12 } else { 0 },
                        })
                        .inner_margin(egui::Margin {
                            left: 16,
                            right: 8,
                            top: 6,
                            bottom: 6,
                        })
                        .show(ui, |ui| {
                            ui.set_min_width(size.x - 24.0);
                            ui.horizontal(|ui| {
                                let title = if draft.subject.trim().is_empty() {
                                    "New message"
                                } else {
                                    draft.subject.as_str()
                                };
                                let title_response = ui
                                    .add_sized(
                                        [ui.available_width() - 108.0, 30.0],
                                        egui::Label::new(
                                            RichText::new(crate::i18n::display(title))
                                                .font(bold_font(14.0)),
                                        )
                                        .truncate()
                                        .halign(egui::Align::Min)
                                        .sense(egui::Sense::click_and_drag()),
                                    )
                                    .on_hover_text(if minimized {
                                        "Restore message"
                                    } else {
                                        "Compose message"
                                    });
                                if title_response.dragged() && !minimized && !expanded {
                                    self.dialogs.compose_positions.insert(
                                        draft.id.clone(),
                                        position + title_response.drag_delta(),
                                    );
                                }
                                if title_response.clicked() && minimized {
                                    self.dialogs.compose_minimized = false;
                                }
                                if tool_button(
                                    ui,
                                    "",
                                    if minimized { "restore" } else { "minimize" },
                                    false,
                                )
                                .on_hover_text(if minimized {
                                    "Restore composer"
                                } else {
                                    "Minimize composer"
                                })
                                .clicked()
                                {
                                    self.dialogs.compose_minimized = !minimized;
                                }
                                if tool_button(
                                    ui,
                                    "",
                                    if expanded { "collapse" } else { "expand" },
                                    false,
                                )
                                .on_hover_text(if expanded {
                                    "Exit expanded view"
                                } else {
                                    "Expand composer"
                                })
                                .clicked()
                                {
                                    self.dialogs.compose_expanded = !expanded;
                                    self.dialogs.compose_minimized = false;
                                }
                                if tool_button(ui, "", "close", false)
                                    .on_hover_text(crate::i18n::display("Close and save draft"))
                                    .clicked()
                                {
                                    save = true;
                                }
                            });
                        });
                    if minimized {
                        return;
                    }
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(14, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [ENVELOPE_LABEL_WIDTH, ui.spacing().interact_size.y],
                                    egui::Label::new(
                                        RichText::new(crate::i18n::display("From"))
                                            .size(12.0)
                                            .color(MUTED),
                                    ),
                                );
                                let previous = draft.account.clone();
                                crate::ui::theme::named_combo("From", egui::ComboBox::from_id_salt("compose_sender")
                                    .width((ui.available_width() - 178.0).max(160.0))
                                    .selected_text(
                                        self.accounts
                                            .iter()
                                            .find(|a| a.id == draft.account)
                                            .map(|a| a.email.as_str())
                                            .unwrap_or("Connect an account in Settings"),
                                    )
                                    .show_ui(ui, |ui| {
                                        for a in &self.accounts {
                                            ui.selectable_value(
                                                &mut draft.account,
                                                a.id.clone(),
                                                format!("{} <{}>", a.name, a.email),
                                            );
                                        }
                                    }));
                                if previous != draft.account {
                                    let old = self
                                        .accounts
                                        .iter()
                                        .find(|a| a.id == previous)
                                        .map(|a| {
                                            if a.signature.is_empty() {
                                                &self.signature
                                            } else {
                                                &a.signature
                                            }
                                        })
                                        .unwrap_or(&self.signature);
                                    if draft.body.trim() == old.trim() {
                                        draft.body = self.signature_for(&draft.account);
                                    }
                                }
                                theme::ghost_controls(ui);
                                for (label, shown) in [
                                    ("Cc", &mut self.dialogs.show_cc),
                                    ("Bcc", &mut self.dialogs.show_bcc),
                                ] {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(crate::i18n::display(label))
                                                    .size(13.0)
                                                    .color(MUTED),
                                            )
                                            .selected(*shown),
                                        )
                                        .clicked()
                                    {
                                        *shown = !*shown;
                                    }
                                }
                            });
                            ui.separator();
                            self.recipient_chips(ui, &mut draft, 0);
                            let show_cc = self.dialogs.show_cc || !draft.cc.is_empty();
                            let show_bcc = self.dialogs.show_bcc || !draft.bcc.is_empty();
                            if show_cc {
                                self.recipient_chips(ui, &mut draft, 1);
                            }
                            if show_bcc {
                                self.recipient_chips(ui, &mut draft, 2);
                            }
                            envelope_field(ui, "Subject", &mut draft.subject, "Subject");
                            let attachments_height = if draft.attachments.is_empty() {
                                0.0
                            } else {
                                56.0
                            };
                            let image_height = if draft.rich.images.is_empty() {
                                0.0
                            } else {
                                110.0
                            };
                            let rich_height =
                                (ui.available_height() - 54.0 - attachments_height - image_height)
                                    .max(80.0);
                            self.rich_editor(ui, &mut draft, rich_height);
                            if !draft.attachments.is_empty() {
                                self.attachment_chips(ui, &mut draft);
                            }
                            if !self.dialogs.compose_error.is_empty() {
                                ui.label(crate::i18n::widget(
                                    RichText::new(&self.dialogs.compose_error)
                                        .size(12.0)
                                        .color(Color32::from_rgb(190, 65, 55)),
                                    ui.style(),
                                ));
                            }
                            ui.separator();
                            ui.horizontal(|ui| {
                                send = ui
                                    .add_enabled(
                                        !self.accounts.is_empty() && self.busy == 0,
                                        action_button(
                                            RichText::new(crate::i18n::display("Send"))
                                                .color(Color32::WHITE),
                                        )
                                        .fill(BLUE)
                                        .corner_radius(5.0)
                                        .min_size(egui::vec2(78.0, 34.0)),
                                    )
                                    .clicked();
                                if tool_button(ui, "", "attach", false)
                                    .on_hover_text(crate::i18n::display("Attach files"))
                                    .clicked()
                                {
                                    self.attach_files(&mut draft);
                                }
                                self.recipient_picker(ui, &mut draft);
                                if tool_button(ui, "", "draft", false)
                                    .on_hover_text(crate::i18n::display("Save and close draft"))
                                    .clicked()
                                {
                                    save = true;
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        discard = tool_button(ui, "", "trash", false)
                                            .on_hover_text(crate::i18n::display("Discard draft"))
                                            .clicked();
                                        ui.label(crate::i18n::widget(
                                            RichText::new(crate::i18n::display(
                                                "Draft saved automatically",
                                            ))
                                            .size(11.0)
                                            .color(MUTED),
                                            ui.style(),
                                        ));
                                    },
                                );
                            });
                        });
                })
            });
        if self.dialogs.raise_compose.as_deref() == Some(draft.id.as_str())
            && let Some(window) = &window
        {
            ctx.move_to_top(window.response.layer_id);
            self.dialogs.raise_compose = None;
        }
        if window
            .as_ref()
            .is_some_and(|w| w.response.contains_pointer())
            && ctx.input(|i| i.pointer.any_pressed())
        {
            self.focused_composer = draft.id.clone();
        }
        if (send || save)
            && let Err(e) = self.flush_recipients(&mut draft)
        {
            self.dialogs.compose_error = e;
            self.composer = Some(draft);
            return;
        }
        if send {
            match self.dispatch_draft(&draft) {
                Ok(()) => self.dialogs.compose_minimized = false,
                Err(e) => {
                    self.dialogs.compose_error = e;
                    self.composer = Some(draft);
                }
            }
        } else if discard {
            match self.store.remove("mail", &draft.id) {
                Ok(()) => {
                    self.mails.retain(|m| m.id != draft.id);
                    self.status = "Draft discarded".into();
                }
                Err(e) => {
                    self.dialogs.compose_error = e;
                    self.composer = Some(draft);
                }
            }
        } else if save {
            match self.save_local_draft(&draft) {
                Ok(()) => {
                    self.status = if draft.is_empty() {
                        "Composer closed"
                    } else {
                        "Draft saved on this device"
                    }
                    .into();
                    self.dialogs.compose_minimized = false;
                }
                Err(e) => {
                    self.dialogs.compose_error = e;
                    self.composer = Some(draft);
                }
            }
        } else {
            self.composer = Some(draft);
        }
    }
    fn signature_for(&self, account: &str) -> String {
        self.accounts
            .iter()
            .find(|a| a.id == account)
            .filter(|a| !a.signature.is_empty())
            .map(|a| a.signature.clone())
            .unwrap_or_else(|| self.signature.clone())
    }
    fn recipient_picker(&mut self, ui: &mut egui::Ui, draft: &mut Draft) {
        tool_menu(ui, "Contacts", "contacts", |ui| {
            ui.set_min_width(280.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.dialogs.picker_query)
                    .hint_text(crate::i18n::display("Find a contact")),
            );
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(210.0)
                .show(ui, |ui| {
                    for c in self
                        .contacts
                        .iter()
                        .filter(|c| !c.deleted && c.matches(&self.dialogs.picker_query))
                    {
                        if ui
                            .add(action_button(format!("{} <{}>", c.name, c.email)))
                            .clicked()
                        {
                            if let Ok(address) = c.mailbox() {
                                if !draft.to.trim().is_empty() {
                                    draft.to.push_str(", ");
                                }
                                draft.to.push_str(&address);
                            }
                            ui.close();
                        }
                    }
                });
            if self.contacts.iter().all(|c| c.deleted) {
                ui.label(crate::i18n::widget(
                    crate::i18n::display("No saved contacts yet."),
                    ui.style(),
                ));
            }
        });
    }
    fn attachment_chips(&self, ui: &mut egui::Ui, draft: &mut Draft) {
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt(("attachment_chips", &draft.id))
            .max_height(52.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, a) in draft.attachments.iter().enumerate() {
                        egui::Frame::new()
                            .fill(if self.dark {
                                Color32::from_gray(40)
                            } else {
                                BG
                            })
                            .stroke(Stroke::new(1.0, theme::line(self.dark)))
                            .corner_radius(4.0)
                            .inner_margin(5.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(crate::i18n::widget(
                                        RichText::new(format!(
                                            "{} · {} KB",
                                            a.name,
                                            a.data.len().div_ceil(1024)
                                        ))
                                        .size(11.0),
                                        ui.style(),
                                    ));
                                    if ui.add(action_button("×").small()).clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
                    }
                });
            });
        if let Some(i) = remove {
            draft.attachments.remove(i);
        }
    }
    pub(super) fn dispatch_draft(&mut self, draft: &Draft) -> Result<(), String> {
        if self.busy > 0 {
            return Err("Wait for the current mail operation to finish.".into());
        }
        let account = self
            .accounts
            .iter()
            .find(|a| a.id == draft.account)
            .cloned()
            .ok_or("Choose a connected sending account.")?;
        network::build_message(&account, draft)?;
        self.save_local_draft(draft)?;
        let draft = draft.clone();
        let tx = self.tx.clone();
        self.busy += 1;
        self.status = format!("Sending from {}…", account.email);
        std::thread::spawn(move || {
            let result = network::send(&account, &draft);
            let _ = tx.send(Event::Sent(account.id, Box::new(draft), result));
        });
        Ok(())
    }
    pub(super) fn start_quick_reply(&mut self, message: &str, all: bool) {
        let Some(mail) = self.mails.iter().find(|m| m.id == message).cloned() else {
            return;
        };
        let (to, cc) = match network::reply_recipients(&mail, &self.accounts, all) {
            Ok(value) => value,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if let Some(reply) = self
            .dialogs
            .quick_reply
            .as_mut()
            .filter(|r| r.message == message)
        {
            reply.draft.to = to;
            reply.draft.cc = cc;
            reply.all = all;
            self.dialogs.focus_quick_reply = true;
            return;
        }
        if let Some(previous) = self.dialogs.quick_reply.take()
            && let Err(e) = self.save_local_draft(&previous.draft)
        {
            self.dialogs.quick_reply = Some(previous);
            self.status = e;
            return;
        }
        let account = if self.accounts.iter().any(|a| a.id == mail.account) {
            mail.account.clone()
        } else {
            self.preferred_account()
        };
        let draft = Draft {
            id: uuid::Uuid::new_v4().to_string(),
            account: account.clone(),
            to,
            cc,
            subject: if mail.subject.to_lowercase().starts_with("re:") {
                mail.subject.clone()
            } else {
                format!("Re: {}", mail.subject)
            },
            body: self.signature_for(&account),
            in_reply_to: mail.message_id.clone(),
            references: format!("{} {}", mail.references, mail.message_id)
                .trim()
                .into(),
            ..Default::default()
        };
        self.editor_states
            .entry(draft.id.clone())
            .or_default()
            .ribbon_collapsed = true;
        self.dialogs.quick_reply = Some(QuickReply {
            message: message.into(),
            draft,
            all,
            error: String::new(),
        });
        self.dialogs.focus_quick_reply = true;
    }
    pub(super) fn pop_out_quick_reply(&mut self) -> Result<(), String> {
        self.park_composer()?;
        self.dialogs.compose_slot = self.next_compose_slot();
        if let Some(reply) = self.dialogs.quick_reply.take() {
            self.editor_states
                .entry(reply.draft.id.clone())
                .or_default()
                .ribbon_collapsed = false;
            self.focused_composer = reply.draft.id.clone();
            self.active_editor = reply.draft.id.clone();
            self.editor_states
                .entry(reply.draft.id.clone())
                .or_default()
                .focus = true;
            self.composer = Some(reply.draft);
            self.dialogs.compose_minimized = false;
            self.dialogs.compose_expanded = false;
            self.dialogs.compose_error = reply.error;
        }
        Ok(())
    }
    pub(super) fn quick_reply_ui(&mut self, ui: &mut egui::Ui, mail: &Mail) {
        if self
            .dialogs
            .quick_reply
            .as_ref()
            .is_none_or(|r| r.message != mail.id)
        {
            ui.horizontal(|ui| {
                if tool_button(ui, "Reply", "reply", false).clicked() {
                    self.start_quick_reply(&mail.id, false);
                }
                if tool_button(ui, "Reply all", "reply_all", false).clicked() {
                    self.start_quick_reply(&mail.id, true);
                }
                if tool_button(ui, "Forward", "forward", false).clicked() {
                    self.compose("forward");
                }
            });
            return;
        }
        let Some(mut reply) = self.dialogs.quick_reply.take() else {
            return;
        };
        let mut send = false;
        let mut expand = false;
        let mut discard = false;
        let focus = self.dialogs.focus_quick_reply;
        self.dialogs.focus_quick_reply = false;
        let response = egui::Frame::new()
            .fill(surface(self.dark))
            .stroke(Stroke::new(
                1.0,
                if self.dark {
                    Color32::from_gray(65)
                } else {
                    Color32::from_gray(215)
                },
            ))
            .corner_radius(8.0)
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    tool_menu(
                        ui,
                        if reply.all { "Reply all" } else { "Reply" },
                        "reply",
                        |ui| {
                            for (all, label) in [(false, "Reply to sender"), (true, "Reply to all")]
                            {
                                if ui
                                    .selectable_label(
                                        reply.all == all,
                                        crate::i18n::widget(
                                            crate::i18n::display(label),
                                            ui.style(),
                                        ),
                                    )
                                    .clicked()
                                {
                                    match network::reply_recipients(mail, &self.accounts, all) {
                                        Ok((to, cc)) => {
                                            reply.draft.to = to;
                                            reply.draft.cc = cc;
                                            reply.all = all;
                                        }
                                        Err(e) => reply.error = e,
                                    }
                                    ui.close();
                                }
                            }
                        },
                    );
                    ui.add(
                        egui::Label::new(crate::i18n::widget(
                            RichText::new(&reply.draft.to).size(12.0).color(MUTED),
                            ui.style(),
                        ))
                        .truncate(),
                    );
                    ui.with_layout(
                        if crate::i18n::language().rtl() {
                            egui::Layout::left_to_right(egui::Align::Center)
                        } else {
                            egui::Layout::right_to_left(egui::Align::Center)
                        },
                        |ui| {
                            expand = tool_button(ui, "", "expand", false)
                                .on_hover_text(crate::i18n::display("Open in composer"))
                                .clicked();
                        },
                    );
                });
                if !reply.draft.cc.is_empty() {
                    ui.label(crate::i18n::widget(
                        RichText::new(format!(
                            "{}: {}",
                            crate::i18n::display("Cc"),
                            reply.draft.cc
                        ))
                        .size(11.0)
                        .color(MUTED),
                        ui.style(),
                    ));
                }
                ui.label(crate::i18n::widget(
                    RichText::new(format!(
                        "{}: {}",
                        crate::i18n::display("From"),
                        self.accounts
                            .iter()
                            .find(|a| a.id == reply.draft.account)
                            .map(|a| a.email.as_str())
                            .unwrap_or("Connect a sending account in Settings")
                    ))
                    .size(11.0)
                    .color(MUTED),
                    ui.style(),
                ));
                ui.add_space(8.0);
                {
                    let state = self
                        .editor_states
                        .entry(reply.draft.id.clone())
                        .or_default();
                    state.inline = true;
                    if !state.ribbon_initialized {
                        state.ribbon_collapsed = true;
                    }
                }
                self.rich_editor(ui, &mut reply.draft, 280.0);
                if focus {
                    self.active_editor = reply.draft.id.clone();
                    self.editor_states
                        .entry(reply.draft.id.clone())
                        .or_default()
                        .focus = true;
                }
                if !reply.draft.attachments.is_empty() {
                    self.attachment_chips(ui, &mut reply.draft);
                }
                if !reply.error.is_empty() {
                    ui.label(crate::i18n::widget(
                        RichText::new(&reply.error)
                            .size(12.0)
                            .color(Color32::from_rgb(190, 65, 55)),
                        ui.style(),
                    ));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    send = ui
                        .add_enabled(
                            self.busy == 0
                                && self.accounts.iter().any(|a| a.id == reply.draft.account),
                            action_button(
                                RichText::new(crate::i18n::display("Send reply"))
                                    .color(Color32::WHITE),
                            )
                            .fill(BLUE)
                            .corner_radius(5.0)
                            .min_size(egui::vec2(96.0, 32.0)),
                        )
                        .clicked();
                    if tool_button(ui, "", "attach", false)
                        .on_hover_text(crate::i18n::display("Attach files"))
                        .clicked()
                    {
                        self.attach_files(&mut reply.draft);
                    }
                    ui.label(crate::i18n::widget(
                        RichText::new(crate::i18n::display("Draft saves automatically"))
                            .size(11.0)
                            .color(MUTED),
                        ui.style(),
                    ));
                    ui.with_layout(
                        if crate::i18n::language().rtl() {
                            egui::Layout::left_to_right(egui::Align::Center)
                        } else {
                            egui::Layout::right_to_left(egui::Align::Center)
                        },
                        |ui| {
                            discard = tool_button(ui, "", "trash", false)
                                .on_hover_text(crate::i18n::display("Discard reply"))
                                .clicked();
                        },
                    );
                });
            })
            .response;
        if focus {
            response.scroll_to_me(Some(egui::Align::BOTTOM));
        }
        if send {
            match self.dispatch_draft(&reply.draft) {
                Ok(()) => {}
                Err(e) => {
                    reply.error = e;
                    self.dialogs.quick_reply = Some(reply);
                }
            }
        } else if discard {
            match self.store.remove("mail", &reply.draft.id) {
                Ok(()) => {
                    self.mails.retain(|m| m.id != reply.draft.id);
                    self.status = "Reply discarded".into();
                }
                Err(e) => {
                    reply.error = e;
                    self.dialogs.quick_reply = Some(reply);
                }
            }
        } else {
            self.dialogs.quick_reply = Some(reply);
            if expand
                && let Err(e) = self.pop_out_quick_reply()
                && let Some(reply) = &mut self.dialogs.quick_reply
            {
                reply.error = e;
            }
        }
    }
    pub(super) fn attach_files(&mut self, draft: &mut Draft) {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            for path in paths {
                match std::fs::read(&path) {
                    Ok(data) => draft.attachments.push(Attachment {
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                        mime: "application/octet-stream".into(),
                        data,
                    }),
                    Err(e) => self.dialogs.compose_error = e.to_string(),
                }
            }
        }
    }
    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.dialogs.about;
        modal_surface(
            ctx,
            "about_barid",
            "About Barid",
            Vec2::new(440.0, 240.0),
            self.dark,
            &mut open,
            |ui| {
                heading(ui, "Barid", "Barid by Woosool");
                ui.label(crate::i18n::widget(
                    format!("Version {}", env!("CARGO_PKG_VERSION")),
                    ui.style(),
                ));
                ui.label(crate::i18n::widget(
                    crate::i18n::display("Rust · egui / eframe"),
                    ui.style(),
                ));
                ui.add_space(18.0);
                ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Mail and contacts stay on your device. Credentials use the operating system credential store.")).size(12.0).color(MUTED), ui.style()));
            },
        );
        self.dialogs.about = open;
    }
    pub(super) fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.shortcuts;
        modal_surface(
            ctx,
            "shortcuts_modal",
            "Shortcuts & search",
            Vec2::new(500.0, 390.0),
            self.dark,
            &mut open,
            |ui| {
                heading(
                    ui,
                    "Work faster",
                    "Keyboard shortcuts apply when you are not typing.",
                );
                egui::Grid::new("keyboard_help")
                    .spacing([34.0, 12.0])
                    .show(ui, |ui| {
                        for (key, action) in [
                            ("Cmd / Ctrl + N", "New message"),
                            ("R", "Reply"),
                            ("E", "Archive"),
                            ("Delete", "Move to Trash"),
                        ] {
                            ui.monospace(key);
                            ui.label(crate::i18n::widget(action, ui.style()));
                            ui.end_row();
                        }
                    });
                section(ui, "Search your mail");
                for example in [
                    "from:amelia subject:design",
                    "is:unread has:attachment",
                    "to:alex is:starred",
                ] {
                    ui.monospace(example);
                }
                ui.add_space(12.0);
                ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Combine terms to narrow results. Closing the composer saves a local draft.")).size(12.0).color(MUTED), ui.style()));
            },
        );
        self.shortcuts = open;
    }
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings;
        let previous = serde_json::to_string(&self.prefs).unwrap_or_default();
        let previous_dark = self.dark;
        modal_surface(
            ctx,
            "settings_modal",
            "Settings",
            Vec2::new(900.0, 610.0),
            self.dark,
            &mut open,
            |ui| {
                ui.set_min_height(ui.available_height().clamp(260.0, 500.0));
                (if crate::i18n::language().rtl() {
                    egui::Panel::right("settings_nav_rtl")
                } else {
                    egui::Panel::left("settings_nav")
                })
                .exact_size(190.0)
                .resizable(false)
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 12)))
                .show(ui, |ui| {
                    theme::scope(ui, |ui| {
                        ui.label(crate::i18n::widget(
                            RichText::new(crate::i18n::display("SETTINGS"))
                                .size(10.0)
                                .color(MUTED),
                            ui.style(),
                        ));
                        ui.add_space(18.0);
                        for (i, label) in [
                            "Email accounts",
                            "General",
                            "Appearance",
                            "Composition",
                            "Layout & desktop",
                            "Contacts sync",
                            "Search & privacy",
                            "Notifications",
                        ]
                        .iter()
                        .enumerate()
                        {
                            if theme::navigation_tab(ui, label, self.dialogs.section == i).clicked()
                            {
                                self.dialogs.section = i;
                            }
                        }
                        ui.add_space(18.0);
                        ui.separator();
                        if ui.add(action_button("Shortcuts & search")).clicked() {
                            self.shortcuts = true;
                        }
                    })
                });
                egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(20,12))).show(ui,|ui| {theme::scope(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("settings_content").show(ui,|ui| {
                    match self.dialogs.section {
                        6=>{
                            heading(ui,"Search & privacy","Find mail faster and control what is saved on this device.");
                            ui.checkbox(&mut self.prefs.search_suggestions,crate::i18n::widget(crate::i18n::display("Suggest search filters, senders and labels"),ui.style()));
                            ui.checkbox(&mut self.prefs.remember_searches,crate::i18n::widget(crate::i18n::display("Remember recent searches on this device"),ui.style()));
                            ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Search history stores up to 20 searches locally. Press Enter to save a search.")).small().color(MUTED),ui.style()));
                            section(ui,"Recent searches");
                            if ui.add_enabled(!self.search_history.is_empty(),action_button("Clear search history")).clicked() {
                                self.search_history.clear();
                                if let Err(e)=self.store.remove("search_history","default") {self.status=e;}
                            }
                            for query in &self.search_history {ui.label(crate::i18n::widget(query,ui.style()));}
                            section(ui,"Search tools");
                            if ui.add(action_button("Advanced search…")).clicked() {self.settings=false;self.organize.advanced=true;}
                            ui.label(crate::i18n::widget(crate::i18n::display("Use the down and up arrows to choose a suggestion, Enter to apply it, and Escape to dismiss."),ui.style()));
                        }
                        7=>{
                            heading(ui,"Notifications","Get a desktop alert when mail arrives or is sent.");
                            section(ui,"Alerts");
                            ui.checkbox(&mut self.prefs.notify_received,crate::i18n::widget(crate::i18n::display("New mail in your inboxes"),ui.style()));
                            ui.checkbox(&mut self.prefs.notify_sent,crate::i18n::widget(crate::i18n::display("Message sent"),ui.style()));
                            section(ui,"Content");
                            ui.add_enabled_ui(self.prefs.notify_received||self.prefs.notify_sent,|ui|{
                                ui.checkbox(&mut self.prefs.notify_previews,crate::i18n::widget(crate::i18n::display("Show sender and subject"),ui.style()));
                                ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Turn off to keep message details private on a shared screen.")).size(12.0).color(MUTED),ui.style()));
                                ui.add_space(6.0);
                                ui.checkbox(&mut self.prefs.notify_sound,crate::i18n::widget(crate::i18n::display("Play a sound for new mail"),ui.style()));
                            });
                            ui.add_space(12.0);
                            if ui.add(action_button("Send test notification")).clicked() {
                                crate::notify::show(crate::i18n::display("Barid"),crate::i18n::display("Notifications are working."),self.prefs.notify_sound);
                            }
                            ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("If nothing appears, allow notifications for Barid in your system settings.")).size(12.0).color(MUTED),ui.style()));
                        }
                        4=>self.layout_settings(ui),
                        5=>self.contacts_sync_settings(ui),
                        0=>self.account_settings(ui),
                        1=>{
                            heading(ui,"General","Control how Barid receives and opens your mail.");
                            section(ui,"Language");
                            self.language_control(ui);
                            section(ui,"Synchronization");
                            ui.horizontal(|ui| {ui.label(crate::i18n::widget(crate::i18n::display("Check for mail every"), ui.style()));ui.add(egui::DragValue::new(&mut self.prefs.sync_minutes).range(0..=120).suffix(" minutes"));});
                            ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Set to 0 for manual refresh. Applies while Barid is open.")).size(11.0).color(MUTED), ui.style()));
                            ui.add_space(12.0);ui.checkbox(&mut self.prefs.mark_read_on_open,crate::i18n::widget(crate::i18n::display("Mark messages as read when opened"),ui.style()));
                            section(ui,"Local storage & privacy");ui.label(crate::i18n::widget(crate::i18n::display("Mail, drafts, contacts, and settings are stored on this device. Account passwords are kept in the operating system credential store."), ui.style()));
                            ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("HTML formatting and embedded images are displayed. Remote images load only when requested.")).size(12.0).color(MUTED), ui.style()));
                        }
                        2=>{
                            heading(ui,"Appearance","A consistent desktop workspace, at your preferred density.");
                            section(ui,"Text size");
                            ui.add(egui::Slider::new(&mut self.prefs.text_scale,0.85..=1.3).step_by(0.05).custom_formatter(|v,_|format!("{:.0}%",v*100.0)).custom_parser(|s|s.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|v|v/100.0)).text(crate::i18n::display("Interface scale")));
                            section(ui,"Theme");ui.checkbox(&mut self.dark,crate::i18n::widget(crate::i18n::display("Use dark appearance"),ui.style()));
                            section(ui,"Message list");ui.checkbox(&mut self.prefs.compact_rows,crate::i18n::widget(crate::i18n::display("Compact message rows"),ui.style()));ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Compact: 74 px · Comfortable: 88 px")).size(12.0).color(MUTED), ui.style()));
                        }
                        _=>{
                            heading(ui,"Composition","Choose your sending identity and signature.");
                            section(ui,"New messages");
                            ui.checkbox(&mut self.prefs.plain_compose,crate::i18n::widget(crate::i18n::display("Compose new messages in plain text"),ui.style()));
                            ui.checkbox(&mut self.prefs.expanded_compose,crate::i18n::widget(crate::i18n::display("Open new composers in expanded view"),ui.style()));
                            section(ui,"Default sending account");
                            crate::ui::theme::named_combo("Default sending account", egui::ComboBox::from_id_salt("default_sender").width(350.0).selected_text(self.accounts.iter().find(|a|a.id==self.prefs.default_account).map(|a|a.email.as_str()).unwrap_or("First connected account")).show_ui(ui,|ui| {
                                ui.selectable_value(&mut self.prefs.default_account,String::new(),"First connected account");for a in &self.accounts{ui.selectable_value(&mut self.prefs.default_account,a.id.clone(),&a.email);}
                            }));
                            ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("An active account inbox takes precedence. Replies use the receiving account.")).size(12.0).color(MUTED), ui.style()));
                            section(ui,"Default signature");
                            if ui.add(egui::TextEdit::multiline(&mut self.signature).desired_width(f32::INFINITY).desired_rows(7).hint_text(crate::i18n::display("Your name, title, and contact information"))).changed() && let Err(e)=self.store.save("signature","default",&self.signature){self.status=e;}
                            ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Account-specific signatures can be set on the Email accounts page.")).size(12.0).color(MUTED), ui.style()));
                        }
                    }
                    ui.add_space(20.0);ui.separator();ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Changes are saved automatically")).size(11.0).color(MUTED), ui.style()));
                });
            })});
            },
        );
        if previous != serde_json::to_string(&self.prefs).unwrap_or_default()
            && let Err(e) = self.store.save("preferences", "default", &self.prefs)
        {
            self.status = e;
        }
        if previous_dark != self.dark
            && let Err(e) = self.store.save("appearance", "dark", &self.dark)
        {
            self.status = e;
        }
        self.settings = open && !self.setup.open && !self.organize.advanced;
    }
    fn account_settings(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "Email accounts",
            "Manage all your inboxes in one place.",
        );
        if self.accounts.is_empty() {
            ui.label(crate::i18n::widget(
                RichText::new(crate::i18n::display(
                    "No accounts yet. Add Gmail, Outlook, Woosool, or any IMAP/POP3 account to start receiving mail.",
                ))
                .color(MUTED),
                ui.style(),
            ));
            ui.add_space(10.0);
        }
        if ui.add(action_button("Add an email account")).clicked() {
            self.open_setup();
        }
        ui.add_space(12.0);
        for account in self.accounts.clone() {
            egui::Frame::new()
                .stroke(Stroke::new(1.0, theme::line(self.dark)))
                .corner_radius(4.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        avatar(ui, &account.email, 32.0);
                        ui.vertical(|ui| {
                            ui.label(crate::i18n::widget(
                                RichText::new(if account.name.is_empty() {
                                    &account.email
                                } else {
                                    &account.name
                                })
                                .font(bold_font(14.0)),
                                ui.style(),
                            ));
                            ui.label(crate::i18n::widget(
                                RichText::new(&account.email).size(12.0).color(MUTED),
                                ui.style(),
                            ));
                        });
                        ui.with_layout(
                            if crate::i18n::language().rtl() {
                                egui::Layout::left_to_right(egui::Align::Center)
                            } else {
                                egui::Layout::right_to_left(egui::Align::Center)
                            },
                            |ui| {
                                if ui
                                    .add_enabled(self.busy == 0, action_button("Edit"))
                                    .clicked()
                                {
                                    if account.connection.api() {
                                        self.reconnect_account(account.clone());
                                    } else {
                                        self.account_form = account.clone();
                                        self.secret.clear();
                                    }
                                }
                                if ui
                                    .add_enabled(self.busy == 0, action_button("Sync"))
                                    .clicked()
                                {
                                    self.sync_one(&account.id);
                                }
                            },
                        );
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let count = self
                            .mails
                            .iter()
                            .filter(|m| m.account == account.id && m.folder == "Inbox")
                            .count();
                        ui.label(crate::i18n::widget(
                            RichText::new(format!(
                                "{count} inbox messages · {}",
                                if account.enabled {
                                    "Sync active"
                                } else {
                                    "Sync paused"
                                }
                            ))
                            .size(11.0)
                            .color(MUTED),
                            ui.style(),
                        ));
                        let mut enabled = account.enabled;
                        if ui
                            .add_enabled(
                                self.busy == 0,
                                egui::Checkbox::new(&mut enabled, "Enable sync"),
                            )
                            .changed()
                        {
                            let mut updated = account.clone();
                            updated.enabled = enabled;
                            match self.store.save("account", &updated.id, &updated) {
                                Ok(()) => {
                                    if let Some(a) =
                                        self.accounts.iter_mut().find(|a| a.id == updated.id)
                                    {
                                        *a = updated;
                                    }
                                }
                                Err(e) => self.status = e,
                            }
                        }
                        if ui.add(action_button("Open inbox").small()).clicked() {
                            self.select_folder("Inbox", Some(account.id.clone()));
                        }
                    });
                });
            if network::password_session_only(&account.id) {
                ui.label(crate::i18n::widget(RichText::new(crate::i18n::display("Password is kept for this session only. Enter it again after restarting Barid.")).color(Color32::from_rgb(170,105,25)),ui.style()));
            }
            if let Some(status) = self.account_status.get(&account.id) {
                ui.label(crate::i18n::widget(
                    RichText::new(status).size(11.0).color(MUTED),
                    ui.style(),
                ));
            }
            ui.add_space(8.0);
        }
        if self.account_form.id.is_empty() {
            return;
        }
        section(ui, "Edit account");
        egui::Grid::new("account_connection_form")
            .num_columns(2)
            .spacing([14.0, 10.0])
            .show(ui, |ui| {
                for (label, value) in [
                    ("Account name", &mut self.account_form.name),
                    ("Email address", &mut self.account_form.email),
                    ("Incoming server", &mut self.account_form.imap_host),
                    ("SMTP server", &mut self.account_form.smtp_host),
                ] {
                    ui.label(crate::i18n::widget(crate::i18n::display(label), ui.style()));
                    ui.add(egui::TextEdit::singleline(value).desired_width(330.0));
                    ui.end_row();
                }
                ui.label(crate::i18n::widget(
                    crate::i18n::display("Incoming TLS port"),
                    ui.style(),
                ));
                ui.add(egui::DragValue::new(&mut self.account_form.imap_port).range(1..=65535));
                ui.end_row();
                ui.label(crate::i18n::widget(
                    crate::i18n::display("SMTP TLS / STARTTLS port"),
                    ui.style(),
                ));
                ui.add(egui::DragValue::new(&mut self.account_form.smtp_port).range(1..=65535));
                ui.end_row();
                ui.label(crate::i18n::widget(
                    crate::i18n::display("App password"),
                    ui.style(),
                ));
                ui.add(
                    egui::TextEdit::singleline(&mut self.secret)
                        .password(true)
                        .desired_width(330.0)
                        .hint_text(if self.account_form.id.is_empty() {
                            "Required"
                        } else {
                            "Blank keeps the saved password"
                        }),
                );
                ui.end_row();
            });
        ui.add_space(10.0);
        ui.label(crate::i18n::widget(
            RichText::new(crate::i18n::display("Account signature (optional)"))
                .size(12.0)
                .color(MUTED),
            ui.style(),
        ));
        ui.add(
            egui::TextEdit::multiline(&mut self.account_form.signature)
                .desired_width(f32::INFINITY)
                .desired_rows(3),
        );
        ui.horizontal(|ui| {
            ui.add_enabled_ui(self.busy == 0, |ui| {
                if tool_button(ui, "Save account", "select", true).clicked() {
                    self.save_account();
                }
            });
            if !self.account_form.id.is_empty() && ui.add(action_button("Cancel edit")).clicked() {
                self.reset_account_form();
            }
        });
    }
    fn reset_account_form(&mut self) {
        self.account_form = Account {
            imap_port: 993,
            smtp_port: 587,
            enabled: true,
            ..Default::default()
        };
        self.secret.clear();
    }
    fn save_account(&mut self) {
        let mut account = self.account_form.clone();
        account.email = account.email.trim().into();
        account.imap_host = account.imap_host.trim().into();
        account.smtp_host = account.smtp_host.trim().into();
        let new = account.id.is_empty();
        if account.email.parse::<lettre::Address>().is_err()
            || account.imap_host.is_empty()
            || account.smtp_host.is_empty()
            || (new && self.secret.is_empty())
        {
            self.status =
                "Enter a valid email address, both server names, and an app password.".into();
            return;
        }
        if self
            .accounts
            .iter()
            .any(|a| a.id != account.id && a.email.eq_ignore_ascii_case(&account.email))
        {
            self.status =
                "This account is already connected. Use Edit to change its settings.".into();
            return;
        }
        if new {
            account.id = uuid::Uuid::new_v4().to_string();
            account.enabled = true;
        }
        let credentials = if self.secret.is_empty() {
            Ok(())
        } else {
            network::save_password(&account, &self.secret)
        };
        match credentials.and_then(|()| self.store.save("account", &account.id, &account)) {
            Ok(()) => {
                let id = account.id.clone();
                self.accounts.retain(|a| a.id != id);
                self.accounts.push(account);
                self.reset_account_form();
                if self.demo {
                    self.mails.retain(|m| m.account != "demo");
                    self.selected = None;
                    self.demo = false;
                }
                self.status = if network::password_session_only(&id) {"Password available for this session only. Your keychain could not save and unlock it; enter it again after restarting Barid."}else{"Account saved securely."}.into();
                self.sync_one(&id);
            }
            Err(e) => self.status = format!("Could not save account: {e}"),
        }
    }
    pub(super) fn sync_one(&mut self, id: &str) {
        if self.busy > 0 {
            self.status = "Wait for the current mail operation to finish.".into();
            return;
        }
        if let Some(account) = self.accounts.iter().find(|a| a.id == id).cloned() {
            self.busy += 1;
            self.last_sync = std::time::Instant::now();
            self.status = format!("Syncing {}…", account.email);
            self.account_status
                .insert(account.id.clone(), "Syncing…".into());
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = network::sync(&account);
                let _ = tx.send(Event::Synced(account.id, result));
            });
        }
    }
    pub(super) fn contacts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.contact_window;
        modal_surface(
            ctx,
            "contacts_modal",
            "Contact manager",
            Vec2::new(1020.0, 610.0),
            self.dark,
            &mut open,
            |ui| {
                self.contact_sync_menu(ui);
                if !self.demo {
                    ui.label(crate::i18n::widget(
                        RichText::new(&self.status).size(12.0).color(MUTED),
                        ui.style(),
                    ));
                }
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::widget(
                        RichText::new(crate::i18n::display("Contacts")).font(bold_font(20.0)),
                        ui.style(),
                    ));
                    ui.add_space(18.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.dialogs.contact_query)
                            .hint_text(crate::i18n::display("Search people, email, company…"))
                            .desired_width(310.0),
                    );
                    ui.add_enabled_ui(!self.dialogs.contact_editing, |ui| {
                        if tool_button(ui, "New contact", "plus", true).clicked() {
                            self.dialogs.contact_form = Contact {
                                id: uuid::Uuid::new_v4().to_string(),
                                ..Default::default()
                            };
                            self.dialogs.contact_editing = true;
                            self.dialogs.contact_error.clear();
                        }
                    });
                    tool_menu(ui, "More", "more", |ui| {
                        if ui.add(action_button("Export contacts (.json)")).clicked() {
                            self.export_contacts();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                !self.dialogs.contact_editing,
                                action_button("Import contacts (.json)"),
                            )
                            .clicked()
                        {
                            self.import_contacts();
                            ui.close();
                        }
                    });
                });
                ui.add_space(10.0);
                ui.separator();
                ui.set_min_height(510.0);
                (if crate::i18n::language().rtl() {
                    egui::Panel::right("contact_groups_rtl")
                } else {
                    egui::Panel::left("contact_groups")
                })
                .exact_size(145.0)
                .resizable(false)
                .frame(egui::Frame::new().inner_margin(8.0))
                .show(ui, |ui| {
                    theme::scope(ui, |ui| {
                        section(ui, "ADDRESS BOOK");
                        for (key, label) in [
                            ("", "All contacts"),
                            ("favorites", "Favorites"),
                            ("trash", "Trash"),
                        ] {
                            if ui
                                .add_sized(
                                    [125.0, 32.0],
                                    egui::Button::selectable(
                                        self.dialogs.contact_filter == key,
                                        label,
                                    ),
                                )
                                .clicked()
                            {
                                self.dialogs.contact_filter = key.into();
                            }
                        }
                        section(ui, "GROUPS");
                        let groups: std::collections::BTreeSet<_> = self
                            .contacts
                            .iter()
                            .filter(|c| !c.deleted && !c.group.is_empty())
                            .map(|c| c.group.clone())
                            .collect();
                        for group in groups {
                            if ui
                                .selectable_label(
                                    self.dialogs.contact_filter == format!("group:{group}"),
                                    &group,
                                )
                                .clicked()
                            {
                                self.dialogs.contact_filter = format!("group:{group}");
                            }
                        }
                    })
                });
                (if crate::i18n::language().rtl() {
                    egui::Panel::right("contact_list_rtl")
                } else {
                    egui::Panel::left("contact_list")
                })
                .exact_size(280.0)
                .resizable(false)
                .frame(egui::Frame::new().inner_margin(8.0))
                .show(ui, |ui| {
                    theme::scope(ui, |ui| {
                        let mut contacts: Vec<Contact> = self
                            .contacts
                            .iter()
                            .filter(|c| {
                                c.matches(&self.dialogs.contact_query)
                                    && match self.dialogs.contact_filter.as_str() {
                                        "trash" => c.deleted,
                                        "favorites" => !c.deleted && c.favorite,
                                        g if g.starts_with("group:") => {
                                            !c.deleted && c.group == g[6..]
                                        }
                                        _ => !c.deleted,
                                    }
                            })
                            .cloned()
                            .collect();
                        contacts.sort_by_key(|c| c.name.to_lowercase());
                        ui.label(crate::i18n::widget(
                            RichText::new(format!("{} contacts", contacts.len()))
                                .size(11.0)
                                .color(MUTED),
                            ui.style(),
                        ));
                        ui.add_space(8.0);
                        egui::ScrollArea::vertical()
                            .id_salt("people_list")
                            .max_height(450.0)
                            .show(ui, |ui| {
                                if contacts.is_empty() {
                                    ui.add_space(35.0);
                                    ui.label(crate::i18n::widget(
                                        RichText::new(crate::i18n::display("No contacts here"))
                                            .color(MUTED),
                                        ui.style(),
                                    ));
                                }
                                for c in contacts {
                                    if ui
                                        .add_enabled(
                                            !self.dialogs.contact_editing,
                                            action_button(format!(
                                                "{}{}\n{}",
                                                if c.favorite { "★  " } else { "" },
                                                if c.name.is_empty() { &c.email } else { &c.name },
                                                c.email
                                            ))
                                            .selected(self.dialogs.contact_selected == c.key())
                                            .frame(false)
                                            .min_size(egui::vec2(258.0, 52.0)),
                                        )
                                        .clicked()
                                    {
                                        self.dialogs.contact_selected = c.key().into();
                                        self.dialogs.contact_form = c;
                                        if self.dialogs.contact_form.id.is_empty() {
                                            self.dialogs.contact_form.id =
                                                self.dialogs.contact_selected.clone();
                                        }
                                        self.dialogs.contact_error.clear();
                                    }
                                }
                            });
                    })
                });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 10)))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("contact_detail")
                            .max_height(460.0)
                            .show(ui, |ui| {
                                if self.dialogs.contact_editing {
                                    heading(
                                        ui,
                                        if self
                                            .contacts
                                            .iter()
                                            .any(|c| c.key() == self.dialogs.contact_form.key())
                                        {
                                            "Edit contact"
                                        } else {
                                            "New contact"
                                        },
                                        "Keep the details that help you stay in touch.",
                                    );
                                    field(
                                        ui,
                                        "Full name",
                                        &mut self.dialogs.contact_form.name,
                                        "Full name",
                                    );
                                    field(
                                        ui,
                                        "Email",
                                        &mut self.dialogs.contact_form.email,
                                        "name@example.com",
                                    );
                                    field(
                                        ui,
                                        "Phone",
                                        &mut self.dialogs.contact_form.phone,
                                        "+1 555 000 0000",
                                    );
                                    field(
                                        ui,
                                        "Company",
                                        &mut self.dialogs.contact_form.company,
                                        "Company or organization",
                                    );
                                    field(
                                        ui,
                                        "Group",
                                        &mut self.dialogs.contact_form.group,
                                        "Work, Personal, or a new group",
                                    );
                                    ui.label(crate::i18n::widget(
                                        RichText::new(crate::i18n::display("Notes"))
                                            .size(12.0)
                                            .color(MUTED),
                                        ui.style(),
                                    ));
                                    ui.add(
                                        egui::TextEdit::multiline(
                                            &mut self.dialogs.contact_form.notes,
                                        )
                                        .desired_rows(3)
                                        .desired_width(f32::INFINITY),
                                    );
                                    ui.checkbox(
                                        &mut self.dialogs.contact_form.favorite,
                                        "Add to favorites",
                                    );
                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        if tool_button(ui, "Save contact", "select", true).clicked()
                                        {
                                            self.save_contact();
                                        }
                                        if ui.add(action_button("Cancel")).clicked() {
                                            self.dialogs.contact_editing = false;
                                            self.dialogs.contact_form = self
                                                .contacts
                                                .iter()
                                                .find(|c| c.key() == self.dialogs.contact_selected)
                                                .cloned()
                                                .unwrap_or_default();
                                        }
                                    });
                                } else if !self.dialogs.contact_selected.is_empty() {
                                    let contact = self.dialogs.contact_form.clone();
                                    avatar(
                                        ui,
                                        &format!("{} <{}>", contact.name, contact.email),
                                        56.0,
                                    );
                                    if !contact.source_account.is_empty() {
                                        ui.label(crate::i18n::widget(
                                            crate::i18n::display("Downloaded contact · local copy"),
                                            ui.style(),
                                        ));
                                    }
                                    ui.add_space(12.0);
                                    heading(
                                        ui,
                                        if contact.name.is_empty() {
                                            &contact.email
                                        } else {
                                            &contact.name
                                        },
                                        &contact.email,
                                    );
                                    for (label, value) in [
                                        ("Phone", &contact.phone),
                                        ("Company", &contact.company),
                                        ("Group", &contact.group),
                                        ("Notes", &contact.notes),
                                    ] {
                                        if !value.is_empty() {
                                            section(ui, label);
                                            ui.label(crate::i18n::widget(value, ui.style()));
                                        }
                                    }
                                    ui.add_space(18.0);
                                    ui.horizontal_wrapped(|ui| {
                                        if contact.deleted {
                                            if ui.add(action_button("Restore contact")).clicked() {
                                                self.set_contact_deleted(false);
                                            }
                                        } else {
                                            if tool_button(ui, "Write email", "mail", true)
                                                .clicked()
                                            {
                                                match contact.mailbox() {
                                                    Ok(address) => {
                                                        self.compose("new");
                                                        if let Some(d) = &mut self.composer {
                                                            d.to = address;
                                                        }
                                                    }
                                                    Err(e) => self.dialogs.contact_error = e,
                                                }
                                            }
                                            if ui.add(action_button("Edit")).clicked() {
                                                self.dialogs.contact_editing = true;
                                            }
                                            if ui.add(action_button("Move to Trash")).clicked() {
                                                self.set_contact_deleted(true);
                                            }
                                        }
                                    });
                                } else {
                                    ui.add_space(100.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(crate::i18n::widget(
                                            RichText::new(crate::i18n::display(
                                                "Your people, in one place",
                                            ))
                                            .font(bold_font(18.0)),
                                            ui.style(),
                                        ));
                                        ui.label(crate::i18n::widget(
                                            RichText::new(
                                                "Select a contact or create your first one.",
                                            )
                                            .color(MUTED),
                                            ui.style(),
                                        ));
                                    });
                                }
                                if !self.dialogs.contact_error.is_empty() {
                                    ui.add_space(12.0);
                                    ui.label(crate::i18n::widget(
                                        RichText::new(&self.dialogs.contact_error)
                                            .size(12.0)
                                            .color(Color32::from_rgb(190, 65, 55)),
                                        ui.style(),
                                    ));
                                }
                            });
                    });
            },
        );
        self.contact_window = open;
    }
    pub(super) fn contact_from_sender(&mut self) {
        let Some(mail) = self
            .mails
            .iter()
            .find(|m| Some(&m.id) == self.selected.as_ref())
        else {
            return;
        };
        match network::parse_addresses(&mail.from) {
            Ok(addresses) => {
                if let Some(address) = addresses.into_iter().next() {
                    if let Some(existing) = self
                        .contacts
                        .iter()
                        .find(|c| c.email.eq_ignore_ascii_case(address.email.as_ref()))
                        .cloned()
                    {
                        self.dialogs.contact_selected = existing.key().into();
                        self.dialogs.contact_form = existing;
                        self.dialogs.contact_editing = false;
                    } else {
                        self.dialogs.contact_form = Contact {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: address.name.unwrap_or_default(),
                            email: address.email.to_string(),
                            ..Default::default()
                        };
                        self.dialogs.contact_selected.clear();
                        self.dialogs.contact_editing = true;
                    }
                    self.contact_window = true;
                }
            }
            Err(e) => self.status = e,
        }
    }
    fn save_contact(&mut self) {
        let mut contact = self.dialogs.contact_form.clone();
        if !contact.source_account.is_empty() {
            contact.local_override = true;
        }
        contact.email = contact.email.trim().into();
        contact.name = contact.name.trim().into();
        contact.group = contact.group.trim().into();
        if let Err(e) = contact.mailbox() {
            self.dialogs.contact_error = format!("Enter a valid email address: {e}");
            return;
        }
        if self.contacts.iter().any(|c| {
            c.key() != contact.key()
                && c.source_account == contact.source_account
                && c.email.eq_ignore_ascii_case(&contact.email)
        }) {
            self.dialogs.contact_error =
                "A contact with this email already exists. Check All contacts or Trash.".into();
            return;
        }
        if contact.id.is_empty() {
            contact.id = contact.key().to_string();
            if contact.id.is_empty() {
                contact.id = uuid::Uuid::new_v4().to_string();
            }
        }
        match self.store.save("contact", contact.key(), &contact) {
            Ok(()) => {
                self.contacts.retain(|c| c.key() != contact.key());
                self.contacts.push(contact.clone());
                self.dialogs.contact_selected = contact.key().into();
                self.dialogs.contact_form = contact;
                self.dialogs.contact_editing = false;
                self.dialogs.contact_error.clear();
                self.status = "Contact saved".into();
            }
            Err(e) => self.dialogs.contact_error = e,
        }
    }
    fn set_contact_deleted(&mut self, deleted: bool) {
        let mut contact = self.dialogs.contact_form.clone();
        contact.deleted = deleted;
        match self.store.save("contact", contact.key(), &contact) {
            Ok(()) => {
                if let Some(c) = self.contacts.iter_mut().find(|c| c.key() == contact.key()) {
                    *c = contact;
                }
                self.dialogs.contact_selected.clear();
                self.dialogs.contact_form = Contact::default();
                self.status = if deleted {
                    "Contact moved to Trash"
                } else {
                    "Contact restored"
                }
                .into();
            }
            Err(e) => self.dialogs.contact_error = e,
        }
    }
    fn export_contacts(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("barid-contacts.json")
            .add_filter("Contact backup", &["json"])
            .save_file()
        {
            match serde_json::to_vec_pretty(&self.contacts)
                .map_err(|e| e.to_string())
                .and_then(|data| std::fs::write(path, data).map_err(|e| e.to_string()))
            {
                Ok(()) => self.status = "Contact backup exported".into(),
                Err(e) => self.dialogs.contact_error = e,
            }
        }
    }
    fn import_contacts(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Contact backup", &["json"])
            .pick_file()
        {
            let result = std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    if bytes.len() > 10_000_000 {
                        return Err("Contact backup is larger than 10 MB".into());
                    }
                    serde_json::from_slice::<Vec<Contact>>(&bytes).map_err(|e| e.to_string())
                })
                .and_then(|contacts| self.merge_contacts(contacts));
            match result {
                Ok(count) => {
                    self.status = format!("Imported {count} contacts");
                    self.dialogs.contact_error.clear();
                }
                Err(e) => self.dialogs.contact_error = e,
            }
        }
    }
    fn merge_contacts(&mut self, contacts: Vec<Contact>) -> Result<usize, String> {
        let count = contacts.len();
        let mut merged = self.contacts.clone();
        for mut contact in contacts {
            contact.email = contact.email.trim().into();
            contact.mailbox()?;
            if let Some(existing) = merged
                .iter_mut()
                .find(|c| c.email.eq_ignore_ascii_case(&contact.email))
            {
                contact.id = existing.key().into();
                *existing = contact;
            } else {
                contact.id = uuid::Uuid::new_v4().to_string();
                merged.push(contact);
            }
        }
        self.store.save_contacts(&merged)?;
        self.contacts = merged;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Barid {
        Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap())
    }
    #[test]
    fn compose_title_dragging_preserves_the_draft_and_window_position() {
        let mut app = app();
        app.compose("new");
        let id = app.composer.as_ref().unwrap().id.clone();
        app.composer.as_mut().unwrap().body = "Keep my draft".into();
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        let mut title = None;
        for phase in 0..5 {
            let events = match (phase, title) {
                (2, Some(pos)) => vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                (3, Some(pos)) => vec![egui::Event::PointerMoved(pos - egui::vec2(80.0, 25.0))],
                (4, Some(pos)) => vec![egui::Event::PointerButton {
                    pos: pos - egui::vec2(80.0, 25.0),
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
                |_| app.compose_windows(&ctx),
            );
            if phase < 2 {
                for shape in &out.shapes {
                    if let egui::Shape::Text(t) = &shape.shape
                        && t.galley.text() == "New message"
                    {
                        title = Some(t.pos + egui::vec2(10.0, 5.0));
                    }
                }
            }
            out.textures_delta.clear();
        }
        assert!(title.is_some());
        assert!(app.dialogs.compose_positions.contains_key(&id));
        assert_eq!(app.composer.as_ref().unwrap().body, "Keep my draft");
        app.dialogs.compose_minimized = true;
        app.focus_compose(&id);
        assert!(!app.dialogs.compose_minimized);
        assert_eq!(app.dialogs.raise_compose.as_deref(), Some(id.as_str()));
    }
    #[test]
    fn inline_reply_keeps_thread_headers_body_and_attachments_when_opened_in_composer() {
        let mut app = app();
        app.accounts = vec![Account {
            id: "me".into(),
            email: "me@example.com".into(),
            ..Default::default()
        }];
        app.mails = vec![Mail {
            id: "thread".into(),
            account: "me".into(),
            from: "Amelia <amelia@example.com>".into(),
            to: "me@example.com".into(),
            cc: "bob@example.com".into(),
            subject: "Review".into(),
            message_id: "<parent@example.com>".into(),
            references: "<root@example.com>".into(),
            ..Default::default()
        }];
        app.start_quick_reply("thread", true);
        let reply = app.dialogs.quick_reply.as_mut().unwrap();
        reply.draft.body = "My inline reply".into();
        reply.draft.attachments.push(Attachment {
            name: "notes.txt".into(),
            data: vec![1, 2, 3],
            ..Default::default()
        });
        let id = reply.draft.id.clone();
        app.composer = Some(Draft {
            id: "other-draft".into(),
            body: "Keep this draft".into(),
            ..Default::default()
        });
        app.pop_out_quick_reply().unwrap();
        let draft = app.composer.as_ref().unwrap();
        assert_eq!(draft.id, id);
        assert_eq!(draft.body, "My inline reply");
        assert_eq!(draft.in_reply_to, "<parent@example.com>");
        assert!(draft.references.contains("<root@example.com>"));
        assert!(draft.cc.contains("bob@example.com"));
        assert_eq!(draft.attachments[0].data, [1, 2, 3]);
        assert!(app.dialogs.quick_reply.is_none());
        assert!(
            app.store
                .mails()
                .unwrap()
                .iter()
                .any(|m| m.id == "other-draft")
        );
    }
    #[test]
    fn inline_reply_can_pop_out_and_render_again_in_the_same_frame() {
        let mut app = app();
        let mail = app.mails[0].clone();
        app.start_quick_reply(&mail.id, false);
        let draft = &mut app.dialogs.quick_reply.as_mut().unwrap().draft;
        draft.body = "Reply body".into();
        draft.rich.apply(0, 5, crate::rich_text::Mark::Bold);
        let id = draft.id.clone();
        let state = app.editor_states.entry(id.clone()).or_default();
        state.selection = 2..5;
        state.focus = true;
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1440.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| {
                app.quick_reply_ui(ui, &mail);
                app.pop_out_quick_reply().unwrap();
                app.compose_windows(&ctx);
            },
        );
        output.textures_delta.clear();
        let draft = app.composer.as_ref().unwrap();
        assert_eq!(draft.id, id);
        assert_eq!(draft.body, "Reply body");
        assert_eq!(draft.rich.spans.len(), 1);
        assert_eq!(app.editor_states[&id].selection, 2..5);
    }
    #[test]
    fn switching_threads_preserves_the_previous_inline_draft() {
        let mut app = app();
        let first = app.mails[0].id.clone();
        let second = app.mails[1].id.clone();
        app.start_quick_reply(&first, false);
        app.dialogs.quick_reply.as_mut().unwrap().draft.body = "Do not lose this reply".into();
        let id = app.dialogs.quick_reply.as_ref().unwrap().draft.id.clone();
        app.start_quick_reply(&second, false);
        assert_eq!(app.dialogs.quick_reply.as_ref().unwrap().message, second);
        assert!(
            app.store
                .mails()
                .unwrap()
                .iter()
                .any(|m| m.id == id && m.body == "Do not lose this reply")
        );
    }
    #[test]
    fn minimized_and_expanded_composer_render_without_changing_the_draft() {
        let mut app = app();
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        app.compose("new");
        app.composer.as_mut().unwrap().body = "Retain this text".into();
        let id = app.composer.as_ref().unwrap().id.clone();
        for (minimized, expanded) in [(false, false), (true, false), (false, true), (false, false)]
        {
            app.dialogs.compose_minimized = minimized;
            app.dialogs.compose_expanded = expanded;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |_| app.compose_window(&ctx),
            );
            output.textures_delta.clear();
            assert_eq!(app.composer.as_ref().unwrap().body, "Retain this text");
            assert_eq!(app.composer.as_ref().unwrap().id, id);
        }
    }
    #[test]
    fn modal_dialog_blocks_main_toolbar_and_does_not_dismiss_on_backdrop_click() {
        let mut app = app();
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1440.0, 900.0));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(bounds),
                ..Default::default()
            },
            |ui| app.workspace_ui(ui),
        );
        let new_button = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    if text.galley.job.text == "New" {
                        return Some(text.pos + text.galley.size() * 0.5);
                    }
                    None
                } else {
                    None
                }
            })
            .expect("New button is rendered");
        output.textures_delta.clear();
        app.settings = true;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(bounds),
                ..Default::default()
            },
            |ui| app.workspace_ui(ui),
        );
        output.textures_delta.clear();
        for pressed in [true, false] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(bounds),
                    events: vec![
                        egui::Event::PointerMoved(new_button),
                        egui::Event::PointerButton {
                            pos: new_button,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| app.workspace_ui(ui),
            );
            output.textures_delta.clear();
        }
        assert!(app.settings);
        assert!(
            app.composer.is_none(),
            "Background New action must stay blocked while modal is open"
        );
    }
    #[test]
    fn opening_and_closing_an_empty_composer_does_not_create_blank_drafts() {
        let mut app = app();
        let draft = Draft {
            id: "empty".into(),
            ..Default::default()
        };
        app.save_local_draft(&draft).unwrap();
        assert!(app.store.mails().unwrap().is_empty());
        let draft = Draft {
            id: "content".into(),
            body: "A real draft".into(),
            ..Default::default()
        };
        app.save_local_draft(&draft).unwrap();
        assert_eq!(app.store.mails().unwrap().len(), 1);
    }
    #[test]
    fn contacts_support_edit_trash_restore_and_reject_duplicate_addresses() {
        let mut app = app();
        app.dialogs.contact_form = Contact {
            id: "person-1".into(),
            name: "Amelia".into(),
            email: "amelia@example.com".into(),
            company: "Studio".into(),
            group: "Work".into(),
            ..Default::default()
        };
        app.save_contact();
        assert_eq!(app.contacts.len(), 1);
        app.dialogs.contact_form.email = "amelia.new@example.com".into();
        app.dialogs.contact_form.phone = "+1 555 0100".into();
        app.save_contact();
        assert_eq!(app.store.load::<Contact>("contact").unwrap().len(), 1);
        assert_eq!(app.contacts[0].phone, "+1 555 0100");
        app.set_contact_deleted(true);
        assert!(app.contacts[0].deleted);
        app.dialogs.contact_form = app.contacts[0].clone();
        app.set_contact_deleted(false);
        assert!(!app.contacts[0].deleted);
        app.dialogs.contact_form = Contact {
            id: "person-2".into(),
            email: "AMELIA.NEW@example.com".into(),
            ..Default::default()
        };
        app.save_contact();
        assert_eq!(app.contacts.len(), 1);
        assert!(!app.dialogs.contact_error.is_empty());
    }
    #[test]
    fn contact_import_validates_entire_batch_before_persisting() {
        let mut app = app();
        let contacts = vec![
            Contact {
                email: "valid@example.com".into(),
                ..Default::default()
            },
            Contact {
                email: "not an address".into(),
                ..Default::default()
            },
        ];
        assert!(app.merge_contacts(contacts).is_err());
        assert!(app.contacts.is_empty());
        assert!(app.store.load::<Contact>("contact").unwrap().is_empty());
        app.merge_contacts(vec![Contact {
            name: "First".into(),
            email: "valid@example.com".into(),
            ..Default::default()
        }])
        .unwrap();
        let id = app.contacts[0].key().to_string();
        app.merge_contacts(vec![Contact {
            name: "Updated".into(),
            email: "VALID@example.com".into(),
            ..Default::default()
        }])
        .unwrap();
        assert_eq!(app.contacts.len(), 1);
        assert_eq!(app.contacts[0].key(), id);
        assert_eq!(app.contacts[0].name, "Updated");
    }
    #[test]
    fn sender_follows_active_inbox_then_default_and_preserves_per_account_signature() {
        let mut app = app();
        app.accounts = vec![
            Account {
                id: "work".into(),
                email: "work@example.com".into(),
                signature: "Work signature".into(),
                ..Default::default()
            },
            Account {
                id: "personal".into(),
                email: "me@example.com".into(),
                signature: "Personal signature".into(),
                ..Default::default()
            },
        ];
        app.prefs.default_account = "personal".into();
        app.compose("new");
        assert_eq!(app.composer.as_ref().unwrap().account, "personal");
        assert_eq!(app.composer.as_ref().unwrap().body, "Personal signature");
        app.account_filter = Some("work".into());
        app.compose("new");
        assert_eq!(app.composer.as_ref().unwrap().account, "work");
        assert_eq!(app.composer.as_ref().unwrap().body, "Work signature");
    }
    #[test]
    fn account_filter_separates_inboxes_and_unified_view_contains_both() {
        let mut app = app();
        app.mails = vec![
            Mail {
                id: "w".into(),
                account: "work".into(),
                folder: "Inbox".into(),
                ..Default::default()
            },
            Mail {
                id: "p".into(),
                account: "personal".into(),
                folder: "Inbox".into(),
                ..Default::default()
            },
        ];
        app.select_folder("Inbox", Some("work".into()));
        assert_eq!(
            app.mails
                .iter()
                .filter(|m| app.visible_in(m, "Inbox"))
                .count(),
            1
        );
        app.select_folder("All Inboxes", None);
        assert_eq!(
            app.mails
                .iter()
                .filter(|m| app.visible_in(m, "All Inboxes"))
                .count(),
            2
        );
    }
    #[test]
    fn older_contact_records_and_preferences_remain_compatible() {
        let contact: Contact =
            serde_json::from_str(r#"{"name":"Legacy","email":"old@example.com"}"#).unwrap();
        assert_eq!(contact.key(), "old@example.com");
        assert!(!contact.deleted);
        assert!(contact.matches("legacy"));
        let prefs: Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(prefs.sync_minutes, 5);
        assert!(prefs.compact_rows);
        let account:Account=serde_json::from_str(r#"{"id":"a","name":"","email":"a@example.com","imap_host":"imap.example.com","imap_port":993,"smtp_host":"smtp.example.com","smtp_port":587}"#).unwrap();
        assert!(account.enabled);
    }
}
