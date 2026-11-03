use serde::{Deserialize, Serialize};
use std::{cell::Cell, collections::HashMap, sync::OnceLock};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    French,
    Arabic,
}
impl Language {
    pub fn rtl(self) -> bool {
        self == Self::Arabic
    }
}
thread_local! {static LANGUAGE:Cell<Language>=const{Cell::new(Language::English)};}
pub fn set_language(language: Language) {
    LANGUAGE.set(language);
}
pub fn language() -> Language {
    LANGUAGE.get()
}
fn catalog() -> &'static HashMap<String, HashMap<String, String>> {
    static CATALOG: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/interface.json"))
            .expect("Valid translation catalog")
    })
}
pub fn tr(source: &str) -> String {
    let key = match language() {
        Language::English => return source.into(),
        Language::French => "fr",
        Language::Arabic => "ar",
    };
    catalog()
        .get(source.trim())
        .and_then(|r| r.get(key))
        .cloned()
        .unwrap_or_else(|| source.into())
}
pub fn display(source: &str) -> String {
    visual(&tr(source))
}
pub fn visual(text: &str) -> String {
    text.into()
}
pub fn widget(
    text: impl Into<eframe::egui::WidgetText>,
    _style: &eframe::egui::Style,
) -> eframe::egui::WidgetText {
    // egui 0.37 lays out a right-to-left line itself (Arabic shaping and run order); putting
    // the runs in visual order here as well turned mixed lines around, e.g. an address shown
    // as "gro.elpmaxe@amlas". The text keeps its reading order.
    text.into()
}
pub fn source(label: &str) -> String {
    if language() == Language::English {
        return label.into();
    }
    for (key, row) in catalog() {
        if row.values().any(|v| v == label || visual(v) == label) {
            return key.clone();
        }
    }
    label.into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogs_cover_both_languages() {
        for row in catalog().values() {
            assert!(!row["fr"].is_empty());
            assert!(!row["ar"].is_empty());
        }
    }
    #[test]
    fn arabic_display_preserves_latin_addresses() {
        let value = visual("البريد user@example.com");
        assert!(value.contains("user@example.com"));
        assert_eq!(value, "البريد user@example.com");
    }
    #[test]
    fn bidi_widgets_keep_reading_order_and_style() {
        // egui lays out right-to-left lines itself: the text reaches it in reading order, with
        // its style, and the Latin address isn't turned around.
        let text = widget(
            eframe::egui::RichText::new("البريد user@example.com اليوم").size(20.0),
            &eframe::egui::Style::default(),
        );
        assert_eq!(text.text(), "البريد user@example.com اليوم");
        assert!(matches!(text, eframe::egui::WidgetText::RichText(_)), "the text keeps its style");
    }
    #[test]
    fn preference_roundtrip_keeps_language() {
        let value = serde_json::to_string(&Language::Arabic).unwrap();
        assert_eq!(
            serde_json::from_str::<Language>(&value).unwrap(),
            Language::Arabic
        );
    }
}

#[cfg(test)]
mod translation_regressions {
    use super::*;
    #[test]
    fn padded_menu_labels_resolve_to_the_same_translation() {
        set_language(Language::Arabic);
        for label in [
            "Inboxes", "Windows", "Mark", "Snooze", "More", "Label", "Filter",
        ] {
            assert_eq!(tr(&format!("{label}   ")), tr(label));
            assert_ne!(tr(label), label);
        }
        set_language(Language::English);
    }
}
