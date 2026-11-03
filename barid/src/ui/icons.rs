use super::*;
use egui::IntoAtoms;

pub(super) fn icon_for_label(label: &str) -> &'static str {
    let name = crate::i18n::source(label).trim().to_lowercase();
    if ["email accounts", "manage accounts…"].contains(&name.as_str()) {
        return "mail";
    }
    if name == "general" {
        return "settings";
    }
    if name == "appearance" {
        return "palette";
    }
    if name == "composition" {
        return "edit";
    }
    if name == "layout & desktop" {
        return "expand";
    }
    if name == "contacts sync" {
        return "contacts";
    }
    if name == "notifications" || name.contains("notification") {
        return "bell";
    }
    if name == "search & privacy" {
        return "search";
    }
    if name == "windows" {
        return "expand";
    }
    if name.is_empty() {
        return "more";
    }
    if name == "×" || name == "x" || name.contains("cancel") || name.starts_with("close") {
        return "close";
    }
    if name.contains("pin") {
        return "pin";
    }
    if name.contains("folder") {
        return "folder";
    }
    if name.contains("search") {
        return "search";
    }
    if name.contains("sync contacts") {
        return "contacts";
    }
    if name.contains("reply all") {
        return "reply_all";
    }
    if name.contains("reply") {
        return "reply";
    }
    if name.contains("forward") {
        return "forward";
    }
    if name.contains("restore") {
        return "restore";
    }
    if name.contains("minimize") {
        return "minimize";
    }
    if name.contains("expand") {
        return "expand";
    }
    if name.contains("export") || name.contains("download") {
        return "download";
    }
    if name.contains("import") || name.contains("upload") {
        return "upload";
    }
    if name.contains("paste") {
        return "paste";
    }
    if name.contains("copy") {
        return "copy";
    }
    if name == "cut" {
        return "cut";
    }
    if name.contains("undo") {
        return "undo";
    }
    if name.contains("redo") {
        return "redo";
    }
    if name.contains("remove")
        || name.contains("discard")
        || name.contains("trash")
        || name.contains("delete")
    {
        return "trash";
    }
    if name.starts_with("save") || name.contains("draft") {
        return "save";
    }
    if name == "rename" {
        return "edit";
    }
    if name == "create" || name.contains("new") || name.starts_with("add ") || name.contains("compose") {
        return "plus";
    }
    if name.contains("sync") || name.contains("refresh") {
        return "refresh";
    }
    if name.starts_with("send") || name == "sent" {
        return "send";
    }
    if name.contains("archive") {
        return "archive";
    }
    if name.contains("flag") || name.contains("star") || name.contains("favorite") {
        return "flag";
    }
    if name.contains("junk") {
        return "junk";
    }
    if name.contains("snooze") {
        return "clock";
    }
    if name.contains("unread") || name.contains("read / ") {
        return "mail";
    }
    if name.contains("inbox") {
        return "inbox";
    }
    if name.contains("folder") {
        return "folder";
    }
    if name.contains("bold") {
        return "bold";
    }
    if name.contains("italic") {
        return "italic";
    }
    if name.contains("underline") {
        return "underline";
    }
    if name.contains("strikethrough") {
        return "strike";
    }
    if name.contains("outdent") {
        return "outdent";
    }
    if name.contains("indent") {
        return "indent";
    }
    if name.contains("bulleted") || name.contains("• list") {
        return "list";
    }
    if name.contains("numbered") || name.contains("1. list") {
        return "numbered";
    }
    if name == "formatting" {
        return "text";
    }
    if name == "paragraph" {
        return "align_left";
    }
    if name.contains("align") || ["left", "center", "right"].contains(&name.as_str()) {
        return match name.as_str() {
            "center" | "align center" => "align_center",
            "right" | "align right" => "align_right",
            _ => "align_left",
        };
    }
    if name.contains("text color") || name.contains("highlight") {
        return "palette";
    }
    if name.contains("plain")
        || name.contains("html")
        || name.contains("font")
        || name.contains("style")
        || name.contains("heading")
        || name == "normal"
        || name == "code"
    {
        return "text";
    }
    if name.contains("replace") {
        return "replace";
    }
    if name.contains("search") || name.contains("find") {
        return "search";
    }
    if name.contains("image") || name.contains("picture") || name.contains("photo") {
        return "image";
    }
    if name.contains("link") {
        return "link";
    }
    if name.contains("contact")
        || name.contains("recipient")
        || name.contains('@')
        || name == "cc"
        || name == "bcc"
    {
        return "contacts";
    }
    if name.contains("account")
        || name.contains("settings")
        || name == "general"
        || name == "composition"
    {
        return "settings";
    }
    if name.contains("about")
        || name.contains("help")
        || name.contains("shortcut")
        || name.contains("keyboard")
    {
        return "help";
    }
    if name.contains("dark") || name.contains("appearance") {
        return "moon";
    }
    if name.contains("compact") {
        return "density";
    }
    if name.contains("quit") {
        return "power";
    }
    if name.contains("preview") {
        return "eye";
    }
    if name.contains("edit") {
        return "edit";
    }
    if name.contains("select") || name.contains("apply") {
        return "select";
    }
    if name.contains("clear") {
        return "eraser";
    }
    if name == "mail"
        || name.contains("gmail")
        || name.contains("outlook")
        || name.contains("icloud")
    {
        return "mail";
    }
    if name.contains("woosool") || name.contains("imap") || name.contains("pop3") {
        return "mail";
    }
    if name == "continue" || name == "connect" || name.contains("sign in") {
        return "forward";
    }
    if name == "back" {
        return "reply";
    }
    if name.contains("attachment") {
        return "attach";
    }
    "more"
}
pub(super) fn icon_label(text: impl Into<egui::WidgetText>) -> egui::Atoms<'static> {
    let text = text.into();
    let text = match text {
        egui::WidgetText::Text(text) => egui::WidgetText::Text(crate::i18n::display(&text)),
        egui::WidgetText::RichText(rt) => {
            let translated = crate::i18n::display(rt.text());
            if translated == rt.text() {
                egui::WidgetText::RichText(rt)
            } else {
                let job = egui::WidgetText::RichText(rt).into_layout_job(
                    &egui::Style::default(),
                    egui::FontSelection::Default,
                    egui::Align::Min,
                );
                let mut job = (*job).clone();
                job.text = translated;
                for section in &mut job.sections {
                    section.byte_range = 0.into()..job.text.len().into();
                }
                egui::WidgetText::LayoutJob(std::sync::Arc::new(job))
            }
        }
        other => other,
    };
    let label = text.text();
    // Formatting glyphs, numeric choices, and color swatches already are their own visual controls.
    if label.trim().is_empty()
        || ["B", "I", "U", "S", "x₂", "x²", "×", "⌃", "⌄"].contains(&label)
        || label.trim_end_matches(" px").parse::<u16>().is_ok()
    {
        return text.into_atoms();
    }
    let icon = icon_for_label(label);
    let text = crate::i18n::widget(text, &egui::Style::default());
    (
        egui::Atom::paint(Vec2::splat(16.0), move |ui, args| {
            draw_icon(
                ui.painter(),
                args.rect.center(),
                icon,
                args.fallback_text_color,
                0.82,
            );
        }),
        text,
    )
        .into_atoms()
}
pub(super) fn action_button(text: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    egui::Button::new(icon_label(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_actions_have_specific_icons() {
        for label in [
            "New message",
            "Reply all",
            "Paste plain",
            "Undo typing",
            "Copy",
            "Cut",
            "Delete",
            "Save all drafts",
            "Contact manager…",
            "Settings…",
            "Insert image…",
            "Bold",
            "Align center",
            "Export original message…",
            "Keyboard shortcuts",
            "Clear formatting",
        ] {
            assert_ne!(icon_for_label(label), "more", "{label}");
        }
    }
}

#[cfg(test)]
mod popup_regressions {
    use super::*;
    #[test]
    fn action_buttons_fit_popup_sizing_passes_in_all_languages() {
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Arabic,
            crate::i18n::Language::French,
        ] {
            crate::i18n::set_language(language);
            let ctx = egui::Context::default();
            configure_fonts(&ctx);
            theme::install(&ctx, 1.0);
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1040.0, 760.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        let anchor = tool_button(ui, "More", "more", false);
                        egui::Popup::menu(&anchor).open(true).show(|ui| {
                            ui.set_min_width(180.0);
                            theme::scope(ui, |ui| {
                                for label in [
                                    "Read / unread",
                                    "Flag / unflag",
                                    "Settings",
                                    "Contacts",
                                    "Minimize all composers",
                                    "Restore all composers",
                                ] {
                                    let response = ui.add(action_button(label));
                                    assert!(response.rect.is_finite());
                                    assert!(response.rect.width() < 1040.0);
                                }
                            });
                        });
                    },
                );
                output.textures_delta.clear();
            }
        }
        crate::i18n::set_language(crate::i18n::Language::English);
    }
}
