use super::editor::EditorCommand;
use super::*;
use crate::rich_text::{Alignment, Mark, safe_link};
fn menu(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.menu_button(crate::i18n::display(label), |ui| {
        ui.visuals_mut().override_text_color = Some(if ui.visuals().dark_mode {
            Color32::WHITE
        } else {
            INK
        });
        ui.set_min_width(205.0);
        if !ui.visuals().dark_mode {
            ui.visuals_mut().widgets.hovered.weak_bg_fill = Color32::from_gray(239);
        }
        content(ui);
    });
}
impl Barid {
    pub(super) fn main_menu(&mut self, root: &mut egui::Ui) {
        #[cfg(target_os = "macos")]
        if self.native_menu.is_some() {
            return;
        }
        egui::Panel::top("application_menu")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(root.visuals().panel_fill)
                    .inner_margin(egui::Margin::symmetric(6, 2)),
            )
            .show(root, |ui| {
                ui.visuals_mut().override_text_color = Some(ui.visuals().text_color());
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
                ui.visuals_mut().widgets.hovered.weak_bg_fill = Color32::from_white_alpha(30);
                ui.spacing_mut().button_padding = Vec2::new(8.0, 3.0);
                egui::MenuBar::new().ui(ui, |ui| {
                    menu(ui, "File", |ui| {
                        if ui.add(action_button("Add an email account")).clicked() {
                            self.open_setup();
                            ui.close();
                        }
                        if ui
                            .add(action_button("New message                 Ctrl/Cmd+N"))
                            .clicked()
                        {
                            self.compose("new");
                            ui.close();
                        }
                        if ui.add(action_button("Save all open drafts")).clicked() {
                            self.save_all_composers();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.composer.is_some(),
                                action_button("Send active message"),
                            )
                            .clicked()
                        {
                            self.send_active_composer();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.composer.is_some(),
                                action_button("Close active draft"),
                            )
                            .clicked()
                        {
                            self.close_active_composer();
                            ui.close();
                        }
                        ui.separator();
                        if ui.add(action_button("Settings…")).clicked() {
                            self.settings = true;
                            ui.close();
                        }
                        ui.separator();
                        if ui.add(action_button("Quit Barid")).clicked() {
                            self.quitting = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            ui.close();
                        }
                    });
                    menu(ui, "Edit", |ui| {
                        ui.add_enabled_ui(!self.active_editor.is_empty(), |ui| {
                            for (label, command) in [
                                ("Undo typing", EditorCommand::Undo),
                                ("Redo typing", EditorCommand::Redo),
                                ("Cut", EditorCommand::Cut),
                                ("Copy", EditorCommand::Copy),
                                ("Paste", EditorCommand::Paste),
                                ("Select all", EditorCommand::SelectAll),
                            ] {
                                if ui.add(action_button(label)).clicked() {
                                    self.pending_editor = Some(command);
                                    ui.close();
                                }
                            }
                        });
                        ui.separator();
                        if ui
                            .add(action_button("Find mail                       Ctrl/Cmd+F"))
                            .clicked()
                        {
                            self.focus_search = true;
                            ui.close();
                        }
                    });
                    menu(ui, "View", |ui| {
                        if ui.add(action_button("All inboxes")).clicked() {
                            self.select_folder("All Inboxes", None);
                            ui.close();
                        }
                        if ui
                            .checkbox(
                                &mut self.unread_only,
                                crate::i18n::display("Unread messages only"),
                            )
                            .clicked()
                        {
                            ui.close();
                        }
                        if ui
                            .checkbox(
                                &mut self.prefs.compact_rows,
                                crate::i18n::display("Compact message rows"),
                            )
                            .clicked()
                        {
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .checkbox(&mut self.dark, crate::i18n::display("Dark appearance"))
                            .clicked()
                        {
                            ui.close();
                        }
                    });
                    menu(ui, "Mailbox", |ui| {
                        if ui.add(action_button("Refresh all accounts")).clicked() {
                            self.sync();
                            ui.close();
                        }
                        ui.menu_button(icon_label("Open inbox"), |ui| {
                            if ui.add(action_button("Unified inbox")).clicked() {
                                self.select_folder("All Inboxes", None);
                                ui.close();
                            }
                            for account in self.accounts.clone() {
                                if ui.add(action_button(&account.email)).clicked() {
                                    self.select_folder("Inbox", Some(account.id));
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button(icon_label("Go to folder"), |ui| {
                            for folder in FOLDERS {
                                if ui.add(action_button(*folder)).clicked() {
                                    self.select_folder(folder, self.account_filter.clone());
                                    ui.close();
                                }
                            }
                        });
                        ui.separator();
                        if ui.add(action_button("Manage email accounts…")).clicked() {
                            self.dialogs.section = 0;
                            self.settings = true;
                            ui.close();
                        }
                        if ui.add(action_button("Contact manager…")).clicked() {
                            self.contact_window = true;
                            ui.close();
                        }
                    });
                    menu(ui, "Message", |ui| {
                        ui.add_enabled_ui(self.selected.is_some(), |ui| {
                            if ui.add(action_button("Reply")).clicked() {
                                if let Some(id) = self.selected.clone() {
                                    self.start_quick_reply(&id, false);
                                }
                                ui.close();
                            }
                            if ui.add(action_button("Reply all")).clicked() {
                                if let Some(id) = self.selected.clone() {
                                    self.start_quick_reply(&id, true);
                                }
                                ui.close();
                            }
                            if ui.add(action_button("Forward")).clicked() {
                                self.compose("forward");
                                ui.close();
                            }
                            ui.separator();
                            for (label, action) in [
                                ("Read / unread", "read"),
                                ("Flag / unflag", "star"),
                                ("Archive", "Archive"),
                                ("Move to Trash", "Trash"),
                                ("Mark as junk", "Junk"),
                            ] {
                                if ui.add(action_button(label)).clicked() {
                                    self.action(action);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.add(action_button("Save sender as contact…")).clicked() {
                                self.contact_from_sender();
                                ui.close();
                            }
                            if ui.add(action_button("Export original message…")).clicked() {
                                self.export();
                                ui.close();
                            }
                        });
                    });
                    menu(ui, "Format", |ui| {
                        ui.add_enabled_ui(!self.active_editor.is_empty(), |ui| {
                            for (label, mark) in [
                                ("Bold", Mark::Bold),
                                ("Italic", Mark::Italic),
                                ("Underline", Mark::Underline),
                                ("Strikethrough", Mark::Strike),
                            ] {
                                if ui.add(action_button(label)).clicked() {
                                    self.pending_editor = Some(EditorCommand::Mark(mark));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            for (label, command) in [
                                ("Bulleted list", EditorCommand::Bullet),
                                ("Numbered list", EditorCommand::Numbered),
                                ("Indent", EditorCommand::Indent),
                                ("Outdent", EditorCommand::Outdent),
                                ("Clear formatting", EditorCommand::Clear),
                            ] {
                                if ui.add(action_button(label)).clicked() {
                                    self.pending_editor = Some(command);
                                    ui.close();
                                }
                            }
                            ui.menu_button(icon_label("Alignment"), |ui| {
                                for (label, align) in [
                                    ("Left", Alignment::Left),
                                    ("Center", Alignment::Center),
                                    ("Right", Alignment::Right),
                                ] {
                                    if ui.add(action_button(label)).clicked() {
                                        self.pending_editor = Some(EditorCommand::Align(align));
                                        ui.close();
                                    }
                                }
                            });
                            ui.menu_button(icon_label("Insert link"), |ui| {
                                let state = self
                                    .editor_states
                                    .entry(self.active_editor.clone())
                                    .or_default();
                                ui.text_edit_singleline(&mut state.link);
                                if ui.add(action_button("Apply link")).clicked() {
                                    if safe_link(&state.link) {
                                        self.pending_editor = Some(EditorCommand::Mark(
                                            Mark::Link(state.link.clone()),
                                        ));
                                        ui.close();
                                    } else {
                                        self.status =
                                            "Use an https://, http://, or mailto: link.".into();
                                    }
                                }
                            });
                            if ui.add(action_button("Insert image…")).clicked() {
                                self.pending_editor = Some(EditorCommand::Image);
                                ui.close();
                            }
                        });
                    });
                    menu(ui, "Window", |ui| {
                        if ui.add(action_button("New compose window")).clicked() {
                            self.compose("new");
                            ui.close();
                        }
                        if ui
                            .add(action_button("Minimize all compose windows"))
                            .clicked()
                        {
                            self.minimize_composers(true);
                            ui.close();
                        }
                        if ui
                            .add(action_button("Restore all compose windows"))
                            .clicked()
                        {
                            self.minimize_composers(false);
                            ui.close();
                        }
                        ui.separator();
                        for (id, title) in self.open_compose_titles() {
                            if ui
                                .selectable_label(
                                    self.composer.as_ref().is_some_and(|d| d.id == id),
                                    title,
                                )
                                .clicked()
                            {
                                self.focus_compose(&id);
                                ui.close();
                            }
                        }
                        if self.composer.is_none() && self.compose_sessions.is_empty() {
                            ui.label(crate::i18n::widget(
                                crate::i18n::display("No open compose windows"),
                                ui.style(),
                            ));
                        }
                    });
                    menu(ui, "Help", |ui| {
                        if ui
                            .add(action_button("Keyboard shortcuts & search"))
                            .clicked()
                        {
                            self.shortcuts = true;
                            ui.close();
                        }
                        if ui.add(action_button("About Barid")).clicked() {
                            self.dialogs.about = true;
                            ui.close();
                        }
                    });
                });
            });
    }
    fn open_compose_titles(&self) -> Vec<(String, String)> {
        self.compose_sessions
            .iter()
            .map(|s| &s.draft)
            .chain(self.composer.iter())
            .map(|d| {
                (
                    d.id.clone(),
                    if d.subject.trim().is_empty() {
                        "New message".into()
                    } else {
                        d.subject.clone()
                    },
                )
            })
            .collect()
    }
    fn minimize_composers(&mut self, minimized: bool) {
        for session in &mut self.compose_sessions {
            session.minimized = minimized;
            session.expanded = false;
        }
        self.dialogs.compose_minimized = minimized;
        self.dialogs.compose_expanded = false;
    }
    pub(super) fn focus_compose(&mut self, id: &str) {
        self.dialogs.raise_compose = Some(id.to_owned());
        if self.composer.as_ref().is_some_and(|d| d.id == id) {
            self.dialogs.compose_minimized = false;
            self.focused_composer = id.into();
            return;
        }
        if let Some(index) = self.compose_sessions.iter().position(|s| s.draft.id == id) {
            let session = self.compose_sessions.remove(index);
            if self.park_composer().is_ok() {
                self.load_session(session);
                self.dialogs.compose_minimized = false;
                self.focused_composer = id.into();
            } else {
                self.compose_sessions.push(session);
            }
        }
    }
    pub(super) fn save_all_composers(&mut self) {
        let mut sessions = std::mem::take(&mut self.compose_sessions);
        for session in &mut sessions {
            if let Err(e) = self
                .flush_recipients(&mut session.draft)
                .and_then(|_| self.save_local_draft(&session.draft))
            {
                self.status = e;
            }
        }
        self.compose_sessions = sessions;
        if let Some(mut draft) = self.composer.take() {
            match self
                .flush_recipients(&mut draft)
                .and_then(|_| self.save_local_draft(&draft))
            {
                Ok(()) => self.status = "Open drafts saved".into(),
                Err(e) => self.status = e,
            }
            self.composer = Some(draft);
        }
    }
    fn close_active_composer(&mut self) {
        if let Some(mut draft) = self.composer.take()
            && let Err(e) = self
                .flush_recipients(&mut draft)
                .and_then(|_| self.save_local_draft(&draft))
        {
            self.status = e;
            self.composer = Some(draft);
        }
    }
    fn send_active_composer(&mut self) {
        if let Some(mut draft) = self.composer.take()
            && let Err(e) = self
                .flush_recipients(&mut draft)
                .and_then(|_| self.dispatch_draft(&draft))
        {
            self.dialogs.compose_error = e;
            self.composer = Some(draft);
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(super) enum NativeAction {
    RestoreWindow,
    AddAccount,
    New,
    Save,
    Send,
    Close,
    Settings,
    About,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    PastePlain,
    SelectAll,
    Find,
    Unified,
    Unread,
    Compact,
    Dark,
    Refresh,
    Folder(String),
    Account(String),
    Accounts,
    Contacts,
    Reply(bool),
    Forward,
    Message(&'static str),
    Sender,
    Export,
    Bold,
    Italic,
    Underline,
    Strike,
    Clear,
    Image,
    Bullet,
    Numbered,
    Align(Alignment),
    Minimize(bool),
    FocusCompose(String),
    Shortcuts,
}
#[cfg(target_os = "macos")]
impl Barid {
    pub(super) fn handle_native_menu(&mut self, ctx: &egui::Context) {
        let actions = self
            .native_menu
            .as_ref()
            .map(|m| m.drain())
            .unwrap_or_default();
        for action in actions {
            self.native_action(action, ctx);
        }
        let state = super::native_menu::State {
            language: self.prefs.language,
            modal: self.organize.advanced
                || self.organize.folders
                || self.setup.open
                || self.settings
                || self.contact_window
                || self.shortcuts
                || self.dialogs.about,
            editor: ctx.memory(|m| m.focused().is_some()),
            format: !self.active_editor.is_empty()
                && self
                    .editor_states
                    .get(&self.active_editor)
                    .and_then(|state| state.widget_id)
                    .is_some_and(|id| ctx.memory(|m| m.has_focus(id))),
            message: self.selected.is_some(),
            compose: self.composer.is_some() || !self.compose_sessions.is_empty(),
            send: self
                .composer
                .as_ref()
                .is_some_and(|d| self.accounts.iter().any(|a| a.id == d.account))
                && self.busy == 0,
            busy: self.busy > 0,
            dark: self.dark,
            unread: self.unread_only,
            compact: self.prefs.compact_rows,
            windows: self.open_compose_titles(),
            accounts: self
                .accounts
                .iter()
                .map(|a| (a.id.clone(), a.email.clone()))
                .collect(),
        };
        if let Some(menu) = self.native_menu.as_mut()
            && let Err(e) = menu.update(state)
        {
            self.status = format!("Menu unavailable: {e}");
        }
    }
    fn native_action(&mut self, action: NativeAction, ctx: &egui::Context) {
        use NativeAction::*;
        if (self.organize.advanced
            || self.organize.folders
            || self.setup.open
            || self.settings
            || self.contact_window
            || self.shortcuts
            || self.dialogs.about)
            && !matches!(
                action,
                RestoreWindow
                    | Quit
                    | About
                    | Cut
                    | Copy
                    | Paste
                    | PastePlain
                    | Undo
                    | Redo
                    | SelectAll
            )
        {
            return;
        }
        match action {
            AddAccount => self.open_setup(),
            New => {
                self.restore_window(ctx);
                self.compose("new");
            }
            Save => self.save_all_composers(),
            Send => self.send_active_composer(),
            Close => {
                if self.composer.is_some() {
                    self.close_active_composer()
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close)
                }
            }
            RestoreWindow => self.restore_window(ctx),
            Quit => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Settings => self.settings = true,
            About => self.dialogs.about = true,
            Find => self.focus_search = true,
            Unified => self.select_folder("All Inboxes", None),
            Unread => self.unread_only = !self.unread_only,
            Compact => self.prefs.compact_rows = !self.prefs.compact_rows,
            Dark => self.dark = !self.dark,
            Refresh => self.sync(),
            Folder(folder) => self.select_folder(&folder, self.account_filter.clone()),
            Account(id) => self.select_folder("Inbox", Some(id)),
            Accounts => {
                self.dialogs.section = 0;
                self.settings = true
            }
            Contacts => self.contact_window = true,
            Reply(all) => {
                if let Some(id) = self.selected.clone() {
                    self.start_quick_reply(&id, all)
                }
            }
            Forward => self.compose("forward"),
            Message(action) => self.action(action),
            Sender => self.contact_from_sender(),
            Export => self.export(),
            Bold => self.pending_editor = Some(EditorCommand::Mark(Mark::Bold)),
            Italic => self.pending_editor = Some(EditorCommand::Mark(Mark::Italic)),
            Underline => self.pending_editor = Some(EditorCommand::Mark(Mark::Underline)),
            Strike => self.pending_editor = Some(EditorCommand::Mark(Mark::Strike)),
            Clear => self.pending_editor = Some(EditorCommand::Clear),
            Image => self.pending_editor = Some(EditorCommand::Image),
            Bullet => self.pending_editor = Some(EditorCommand::Bullet),
            Numbered => self.pending_editor = Some(EditorCommand::Numbered),
            Align(a) => self.pending_editor = Some(EditorCommand::Align(a)),
            Minimize(value) => self.minimize_composers(value),
            FocusCompose(id) => self.focus_compose(&id),
            Shortcuts => self.shortcuts = true,
            Cut | Copy | Paste | PastePlain | Undo | Redo | SelectAll => {
                let event = match action {
                    Cut => egui::Event::Cut,
                    Copy => egui::Event::Copy,
                    Paste | PastePlain => {
                        if matches!(action, PastePlain)
                            && self
                                .editor_states
                                .get(&self.active_editor)
                                .and_then(|state| state.widget_id)
                                .is_some_and(|id| ctx.memory(|m| m.has_focus(id)))
                        {
                            self.pending_editor = Some(EditorCommand::PastePlain);
                            return;
                        }
                        let text = arboard::Clipboard::new()
                            .and_then(|mut c| c.get_text())
                            .unwrap_or_default();
                        egui::Event::Paste(text)
                    }
                    _ => egui::Event::Key {
                        key: if matches!(action, SelectAll) {
                            egui::Key::A
                        } else {
                            egui::Key::Z
                        },
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            shift: matches!(action, Redo),
                            ..egui::Modifiers::COMMAND
                        },
                    },
                };
                ctx.input_mut(|i| i.events.push(event));
            }
        }
    }
}
