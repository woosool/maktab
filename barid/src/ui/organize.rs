use super::dialogs::{field, heading, modal_surface, section};
use super::*;
#[derive(Default)]
pub(super) struct Organize {
    pub advanced: bool,
    pub folders: bool,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub words: String,
    pub label: String,
    pub after: String,
    pub before: String,
    pub attachment: bool,
    pub unread: bool,
    pub account: String,
    pub carddav_urls: std::collections::HashMap<String, String>,
    pub folder_name: String,
    pub selected_folder: String,
    pub selected_is_label: bool,
    pub confirm_delete: bool,
}
impl Barid {
    pub(super) fn advanced_search(&mut self, ctx: &egui::Context) {
        let mut open = self.organize.advanced;
        modal_surface(
            ctx,
            "advanced_search",
            "Advanced search",
            Vec2::new(580.0, 600.0),
            self.dark,
            &mut open,
            |ui| {
                heading(
                    ui,
                    "Find messages",
                    "Combine fields; search uses the current mailbox or All Mail.",
                );
                field(ui, "From", &mut self.organize.from, "Name or address");
                field(ui, "To", &mut self.organize.to, "Name or address");
                field(ui, "Subject", &mut self.organize.subject, "Exact phrase");
                field(
                    ui,
                    "Contains words",
                    &mut self.organize.words,
                    "Words, -excluded, or OR",
                );
                field(
                    ui,
                    "Label",
                    &mut self.organize.label,
                    "Local or provider label",
                );
                ui.columns(2, |columns| {
                    field(
                        &mut columns[0],
                        "After",
                        &mut self.organize.after,
                        "YYYY-MM-DD",
                    );
                    field(
                        &mut columns[1],
                        "Before",
                        &mut self.organize.before,
                        "YYYY-MM-DD",
                    );
                });
                ui.checkbox(
                    &mut self.organize.attachment,
                    crate::i18n::display("Has attachment"),
                );
                ui.checkbox(
                    &mut self.organize.unread,
                    crate::i18n::display("Unread only"),
                );
                ui.horizontal(|ui| {
                    if ui.add(action_button("Search all mail")).clicked() {
                        self.folder = "All Mail".into();
                        self.apply_advanced();
                        self.organize.advanced = false;
                    }
                    if ui.add(action_button("Search this mailbox")).clicked() {
                        self.apply_advanced();
                        self.organize.advanced = false;
                    }
                });
            },
        );
        self.organize.advanced &= open;
    }
    fn apply_advanced(&mut self) {
        let mut terms = Vec::new();
        for (key, value) in [
            ("from", &self.organize.from),
            ("to", &self.organize.to),
            ("subject", &self.organize.subject),
            ("label", &self.organize.label),
            ("after", &self.organize.after),
            ("before", &self.organize.before),
        ] {
            if !value.trim().is_empty() {
                terms.push(format!("{key}:{}", crate::search::quote(value.trim())));
            }
        }
        if !self.organize.words.trim().is_empty() {
            terms.push(self.organize.words.trim().into());
        }
        if self.organize.attachment {
            terms.push("has:attachment".into());
        }
        if self.organize.unread {
            terms.push("is:unread".into());
        }
        self.query = terms.join(" ");
        self.remember_search();
        let folder = self.folder.clone();
        self.select_folder(&folder, self.account_filter.clone());
    }
    pub(super) fn folder_manager(&mut self, ctx: &egui::Context) {
        let mut open = self.organize.folders;
        if self.organize.account.is_empty() {
            self.organize.account = self
                .account_filter
                .clone()
                .or_else(|| self.accounts.first().map(|a| a.id.clone()))
                .unwrap_or_default();
        }
        modal_surface(
            ctx,
            "folder_manager",
            "Folders & labels",
            Vec2::new(720.0, 540.0),
            self.dark,
            &mut open,
            |ui| {
                ui.label(crate::i18n::widget(
                    RichText::new(crate::i18n::display(
                        "Changes apply to the selected mail account.",
                    ))
                    .size(12.0)
                    .color(MUTED),
                    ui.style(),
                ));
                ui.add_space(10.0);
                crate::ui::theme::named_combo("Account", egui::ComboBox::from_id_salt("folder_account")
                    .selected_text(
                        self.accounts
                            .iter()
                            .find(|a| a.id == self.organize.account)
                            .map(|a| a.email.as_str())
                            .unwrap_or("Choose an account"),
                    )
                    .show_ui(ui, |ui| {
                        for a in &self.accounts {
                            if ui
                                .selectable_value(
                                    &mut self.organize.account,
                                    a.id.clone(),
                                    &a.email,
                                )
                                .clicked()
                            {
                                self.organize.selected_folder.clear();
                            }
                        }
                    }));
                section(ui, "Existing folders");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for f in self
                            .folders
                            .clone()
                            .into_iter()
                            .filter(|f| f.account == self.organize.account)
                        {
                            if ui
                                .selectable_label(
                                    self.organize.selected_folder == f.path,
                                    format!("{}{}", if f.is_label { "# " } else { "" }, f.name),
                                )
                                .clicked()
                            {
                                self.organize.selected_is_label = f.is_label;
                                self.organize.selected_folder = f.path;
                                self.organize.folder_name = f.name;
                                self.organize.confirm_delete = false;
                            }
                        }
                    });
                field(
                    ui,
                    "Folder or label name",
                    &mut self.organize.folder_name,
                    "New name",
                );
                let selected = self
                    .folders
                    .iter()
                    .find(|f| {
                        f.account == self.organize.account
                            && f.is_label == self.organize.selected_is_label
                            && f.path == self.organize.selected_folder
                    })
                    .cloned();
                let mutable = selected.as_ref().is_some_and(|f| {
                    !network::folders::protected(f)
                        && (!f.is_label
                            || self
                                .accounts
                                .iter()
                                .any(|a| a.id == f.account && a.connection == Connection::Gmail))
                });
                ui.add_enabled_ui(self.busy==0,|ui|{ui.horizontal(|ui|{if ui.add(action_button("Create")).clicked(){self.change_folder(network::folders::Change::Create(self.organize.folder_name.clone()));}
if ui.add_enabled(mutable,action_button("Rename")).clicked(){self.change_folder(network::folders::Change::Rename{path:self.organize.selected_folder.clone(),name:self.organize.folder_name.clone()});}});
   ui.checkbox(&mut self.organize.confirm_delete,crate::i18n::widget(crate::i18n::display("Confirm deletion of the selected folder or label and its server contents"),ui.style()));
   if ui.add_enabled(mutable&&self.organize.confirm_delete,action_button("Delete selected")).clicked(){self.change_folder(network::folders::Change::Delete(self.organize.selected_folder.clone()));}
  });
                if !self.demo {
                    ui.add_space(6.0);
                    ui.label(crate::i18n::widget(
                        RichText::new(&self.status).size(12.0).color(MUTED),
                        ui.style(),
                    ));
                }
            },
        );
        self.organize.folders = open;
    }
    fn change_folder(&mut self, change: network::folders::Change) {
        if let Some(account) = self
            .accounts
            .iter()
            .find(|a| a.id == self.organize.account)
            .cloned()
        {
            let tx = self.tx.clone();
            self.busy += 1;
            self.status = "Updating server folders…".into();
            std::thread::spawn(move || {
                let result = network::folders::change(&account, change);
                let _ = tx.send(Event::Folders(account.id, result));
            });
        }
    }
    pub(super) fn provider_labels(&mut self, ui: &mut egui::Ui, mail: &Mail) {
        let labels: Vec<_> = self
            .folders
            .iter()
            .filter(|f| f.account == mail.account && f.is_label)
            .cloned()
            .collect();
        if !labels.is_empty() {
            ui.menu_button(icon_label("Provider labels"), |ui| {
                for label in labels {
                    let mut applied = mail.remote_labels.contains(&label.name);
                    if ui.checkbox(&mut applied, &label.name).changed() {
                        self.remote_label(mail, label.path, applied);
                        ui.close();
                    }
                }
            });
        }
    }
    fn remote_label(&mut self, mail: &Mail, label: String, apply: bool) {
        if self.busy > 0 {
            return;
        }
        if let Some(a) = self.accounts.iter().find(|a| a.id == mail.account).cloned() {
            let tx = self.tx.clone();
            let mail = mail.clone();
            self.busy += 1;
            std::thread::spawn(move || {
                let action = RemoteAction::Label(label, apply);
                let result = network::action(&a, &mail, action.clone());
                let _ = tx.send(Event::Action(mail.id, action, result));
            });
        }
    }
    pub(super) fn sync_contacts(&mut self, id: &str) {
        if self.busy > 0 {
            return;
        }
        if let Some(account) = self.accounts.iter().find(|a| a.id == id).cloned() {
            let tx = self.tx.clone();
            self.busy += 1;
            self.status = "Downloading contacts…".into();
            std::thread::spawn(move || {
                let result = network::contacts::sync(&account);
                let _ = tx.send(Event::Contacts(account.id, result));
            });
        }
    }
    pub(super) fn contact_sync_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(icon_label("Sync contacts"), |ui| {
            for a in self.accounts.clone() {
                if ui
                    .add_enabled(self.busy == 0, action_button(&a.email))
                    .clicked()
                {
                    self.sync_contacts(&a.id);
                    ui.close();
                }
            }
            if self.accounts.is_empty() {
                ui.label(crate::i18n::widget(
                    crate::i18n::display("Add an email account"),
                    ui.style(),
                ));
            }
        });
    }
    pub(super) fn contacts_sync_settings(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "Contacts sync",
            "Download contacts from Google, Outlook, or an HTTPS CardDAV address book.",
        );
        ui.label(crate::i18n::widget(crate::i18n::display("Downloaded contacts merge by provider ID. Local contacts, favorites, groups and Trash are retained. Edits here stay on this device."),ui.style()));
        for mut a in self.accounts.clone() {
            section(ui, &a.email);
            if a.connection.api() {
                ui.label(crate::i18n::widget(
                    "Reconnect this account if contacts permission has not been granted.",
                    ui.style(),
                ));
            } else {
                field(
                    ui,
                    "CardDAV address-book URL",
                    self.organize
                        .carddav_urls
                        .entry(a.id.clone())
                        .or_insert_with(|| a.contacts_url.clone()),
                    "https://server/addressbooks/…/",
                );
                if ui
                    .add(action_button(format!(
                        "Save CardDAV settings · {}",
                        a.email
                    )))
                    .clicked()
                {
                    a.contacts_url = self
                        .organize
                        .carddav_urls
                        .get(&a.id)
                        .cloned()
                        .unwrap_or_default();
                    match self.store.save("account", &a.id, &a) {
                        Ok(()) => {
                            if let Some(old) = self.accounts.iter_mut().find(|old| old.id == a.id) {
                                old.contacts_url = a.contacts_url.clone();
                            }
                            self.status = "CardDAV settings saved".into();
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
            if ui
                .add_enabled(
                    self.busy == 0,
                    action_button(format!("Sync contacts · {}", a.email)),
                )
                .clicked()
            {
                self.sync_contacts(&a.id);
            }
        }
    }
    pub(super) fn layout_settings(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "Layout & desktop",
            "Choose how your mail workspace behaves.",
        );
        ui.checkbox(
            &mut self.prefs.email_body_dark,
            crate::i18n::display("Use the app theme for the email body (white by default)"),
        );
        ui.checkbox(
            &mut self.prefs.reading_below,
            crate::i18n::display("Reading pane below the message list"),
        );
        ui.checkbox(
            &mut self.prefs.group_threads,
            crate::i18n::display("Group messages into conversations"),
        );
        ui.checkbox(
            &mut self.prefs.unread_badge,
            crate::i18n::display("Show unread count on the app icon"),
        );
        ui.checkbox(
            &mut self.prefs.minimize_to_tray,
            crate::i18n::display("Keep Barid in the tray when the window is closed"),
        );
        if ui.add(action_button("Minimize to tray")).clicked() {
            self.hide_to_tray(ui.ctx());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advanced_fields_quote_phrases_and_operators() {
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        app.organize.from = "Alice OR Bob".into();
        app.organize.subject = "Project plan".into();
        app.organize.attachment = true;
        app.apply_advanced();
        assert_eq!(
            crate::search::tokens(&app.query),
            vec![
                "from:Alice OR Bob",
                "subject:Project plan",
                "has:attachment"
            ]
        );
    }
}
