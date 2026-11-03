use super::*;
impl Barid {
    pub(super) fn remember_search(&mut self) {
        let query = self.query.trim().to_owned();
        if !self.prefs.remember_searches || query.is_empty() {
            return;
        }
        self.search_history
            .retain(|q| !q.eq_ignore_ascii_case(&query));
        self.search_history.insert(0, query);
        self.search_history.truncate(20);
        if let Err(error) = self
            .store
            .save("search_history", "default", &self.search_history)
        {
            self.status = error;
        }
    }
    pub(super) fn search_choices(&self) -> Vec<String> {
        let query = self.query.trim();
        let lower = query.to_lowercase();
        let mut choices = Vec::new();
        if self.prefs.remember_searches {
            choices.extend(
                self.search_history
                    .iter()
                    .filter(|q| q.to_lowercase().contains(&lower))
                    .cloned(),
            );
        }
        if self.prefs.search_suggestions && !query.is_empty() {
            let (prefix, token) = query
                .rsplit_once(' ')
                .map(|(p, t)| (format!("{p} "), t))
                .unwrap_or((String::new(), query));
            let token = token.to_lowercase();
            for operator in [
                "from:",
                "to:",
                "subject:",
                "is:unread",
                "is:read",
                "is:starred",
                "is:pinned",
                "has:attachment",
                "label:",
                "after:",
                "before:",
            ] {
                if operator.starts_with(&token) {
                    choices.push(format!("{prefix}{operator}"));
                }
            }
            for mail in self
                .mails
                .iter()
                .filter(|m| self.account_filter.as_ref().is_none_or(|a| &m.account == a))
            {
                if token.starts_with("from:")
                    && let Ok(addresses) = network::parse_addresses(&mail.from)
                {
                    for address in addresses {
                        let candidate = format!("from:{}", address.email);
                        if candidate.to_lowercase().starts_with(&token) {
                            choices.push(format!("{prefix}{candidate}"));
                        }
                    }
                }
                if token.starts_with("label:") {
                    for label in mail.tags.iter().chain(&mail.remote_labels) {
                        if label
                            .to_lowercase()
                            .starts_with(token.trim_start_matches("label:").trim_matches('"'))
                        {
                            choices.push(format!("{prefix}label:\"{}\"", label.replace('"', "")));
                        }
                    }
                }
            }
        }
        let mut seen = HashSet::new();
        choices.retain(|q| q != query && seen.insert(q.to_lowercase()));
        choices.truncate(8);
        choices
    }
    pub(super) fn search_popup(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
        if !response.has_focus()
            && !response.lost_focus()
            && !response.gained_focus()
            && !response.changed()
            && !self.search_open
        {
            return;
        }
        if response.gained_focus() || response.changed() {
            self.search_open = true;
            self.search_cursor = None;
        }
        let choices = self.search_choices();
        let mut choose = None;
        if response.has_focus() {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.search_open = false;
            }
            if !choices.is_empty()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            {
                self.search_open = true;
                self.search_cursor =
                    Some(self.search_cursor.map_or(0, |i| (i + 1) % choices.len()));
            }
            if !choices.is_empty()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
            {
                self.search_cursor = Some(self.search_cursor.map_or(choices.len() - 1, |i| {
                    (i + choices.len() - 1) % choices.len()
                }));
            }
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            choose = self.search_cursor.and_then(|i| choices.get(i).cloned());
            if choose.is_none() {
                self.remember_search();
                self.search_open = false;
            }
        }
        let mut open = self.search_open && !choices.is_empty();
        egui::Popup::from_response(response)
            .open_bool(&mut open)
            .width(response.rect.width().max(300.0))
            .show(|ui| {
                theme::scope(ui, |ui| {
                    ui.label(crate::i18n::widget(
                        RichText::new(crate::i18n::display(if self.query.is_empty() {
                            "Recent searches"
                        } else {
                            "Suggestions & recent searches"
                        }))
                        .small()
                        .color(MUTED),
                        ui.style(),
                    ));
                    for (index, choice) in choices.iter().enumerate() {
                        if ui
                            .selectable_label(
                                self.search_cursor == Some(index),
                                crate::i18n::widget(choice, ui.style()),
                            )
                            .clicked()
                        {
                            choose = Some(choice.clone());
                            ui.close();
                        }
                    }
                })
            });
        if !open {
            self.search_open = false;
        }
        if let Some(query) = choose {
            self.query = query;
            if !self.query.ends_with(':') {
                self.remember_search();
            }
            let folder = self.folder.clone();
            self.select_folder(&folder, self.account_filter.clone());
            self.search_open = false;
            self.search_cursor = None;
            response.request_focus();
            // Move the caret after the inserted completion instead of retaining the old range.
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(self.query.chars().count()),
                    )));
                state.store(ui.ctx(), response.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Barid {
        Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap())
    }
    #[test]
    fn history_is_recent_unique_bounded_persisted_and_optional() {
        let mut app = app();
        for n in 0..25 {
            app.query = format!("subject:project{n}");
            app.remember_search();
        }
        assert_eq!(app.search_history.len(), 20);
        app.query = "subject:project21".into();
        app.remember_search();
        assert_eq!(app.search_history[0], "subject:project21");
        assert_eq!(
            app.search_history
                .iter()
                .filter(|q| q.as_str() == "subject:project21")
                .count(),
            1
        );
        let saved = app.store.load::<Vec<String>>("search_history").unwrap();
        assert_eq!(saved[0], app.search_history);
        app.prefs.remember_searches = false;
        app.query = "private query".into();
        app.remember_search();
        assert!(!app.search_history.contains(&app.query));
        assert!(
            !app.search_choices()
                .contains(&"subject:project21".to_owned())
        );
    }
    #[test]
    fn completions_include_filters_senders_labels_and_current_token() {
        let mut app = app();
        app.query = "is:u".into();
        assert!(app.search_choices().contains(&"is:unread".into()));
        app.query = "has:attachment from:amel".into();
        assert!(
            app.search_choices()
                .contains(&"has:attachment from:amelia@studio.example".into())
        );
        app.query = "label:W".into();
        assert!(app.search_choices().contains(&"label:\"Work\"".into()));
        app.prefs.search_suggestions = false;
        assert!(app.search_choices().is_empty());
    }
    #[test]
    fn history_popup_and_all_settings_pages_render_in_all_languages() {
        let mut app = app();
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        app.search_history = vec!["is:unread".into(), "from:alice@example.com".into()];
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::French,
            crate::i18n::Language::Arabic,
        ] {
            app.prefs.language = language;
            crate::i18n::set_language(language);
            for dark in [false, true] {
                app.dark = dark;
                theme::install(&ctx, 1.0);
                ctx.set_visuals(theme::visuals(dark));
                for section in 0..8 {
                    app.settings = true;
                    app.dialogs.section = section;
                    let mut out = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                Vec2::new(1040.0, 760.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            app.toolbar(ui);
                            app.settings_window(&ctx);
                        },
                    );
                    out.textures_delta.clear();
                }
                app.settings = false;
                app.search_open = true;
                app.focus_search = true;
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1040.0, 760.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.toolbar(ui),
                );
                out.textures_delta.clear();
            }
        }
        crate::i18n::set_language(crate::i18n::Language::English);
    }
}
