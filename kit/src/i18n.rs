//! Interface languages: English, French, and Arabic with a right-to-left layout. Translations
//! are resource files (`i18n/fr.json`, `i18n/ar.json`) keyed by the English text; anything
//! untranslated shows in English.
//! egui shapes Arabic text itself; the layout is mirrored for Arabic (`rtl`).

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang { #[default] En, Fr, Ar }

impl Lang {
    pub const ALL: [(Lang, &'static str); 3] = [(Lang::En, "English"), (Lang::Fr, "Français"), (Lang::Ar, "العربية")];
    pub fn code(&self) -> &'static str { match self { Lang::En => "en", Lang::Fr => "fr", Lang::Ar => "ar" } }
    pub fn from_code(c: &str) -> Lang { match c { "fr" => Lang::Fr, "ar" => Lang::Ar, _ => Lang::En } }
    pub fn name(&self) -> String { shape(Self::ALL.iter().find(|l| l.0 == *self).map(|l| l.1).unwrap_or("English")) }
}

thread_local! { static LANG: Cell<Lang> = const { Cell::new(Lang::En) }; }

pub fn set(lang: Lang) { LANG.with(|l| l.set(lang)); }
pub fn lang() -> Lang { LANG.with(|l| l.get()) }
/// Is the interface right to left (Arabic)?
pub fn rtl() -> bool { lang() == Lang::Ar }

fn table(lang: Lang) -> &'static HashMap<String, String> {
    static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
    static AR: OnceLock<HashMap<String, String>> = OnceLock::new();
    static EMPTY: OnceLock<HashMap<String, String>> = OnceLock::new();
    let load = |code: &str| -> HashMap<String, String> {
        word_engine::resources::load(&format!("i18n/{code}.json"))
            .and_then(|b| serde_json::from_slice::<HashMap<String, String>>(b).ok())
            .unwrap_or_default()
    };
    match lang {
        Lang::Fr => FR.get_or_init(|| load("fr")),
        Lang::Ar => AR.get_or_init(|| load("ar")),
        Lang::En => EMPTY.get_or_init(HashMap::new),
    }
}

static MISSING: std::sync::Mutex<std::collections::BTreeSet<String>> = std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Translate an interface string into the current language (ready to draw). Strings without a
/// translation show in English and are noted (see `missing`).
pub fn tr(s: &str) -> String {
    // Document text shown in the interface can hold the engine's private-use placeholders
    // (note references, objects); they have no glyphs.
    let cleaned;
    let s = if s.chars().any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c) || c == '\u{FFFC}') { cleaned = s.chars().filter(|c| !(('\u{E000}'..='\u{F8FF}').contains(c) || *c == '\u{FFFC}')).collect::<String>(); cleaned.as_str() } else { s };
    match lang() {
        Lang::En => s.to_string(),
        l => match table(l).get(s) {
            Some(t) if l == Lang::Ar => shape(t),
            Some(t) => t.clone(),
            None => {
                // Text that is already translated (it has Arabic letters) isn't missing.
                if s.chars().any(|c| c.is_ascii_alphabetic()) && !s.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)) && table(l).values().all(|v| v != s) { if let Ok(mut m) = MISSING.lock() { m.insert(s.to_string()); } }
                s.to_string()
            }
        },
    }
}

/// A translation in reading order, for text a document renderer shapes itself (placeholder
/// prompts drawn on slides); falls back to the English.
pub fn raw(s: &str) -> String {
    match lang() { Lang::En => s.to_string(), l => table(l).get(s).cloned().unwrap_or_else(|| s.to_string()) }
}

/// Document text for display in the interface, without the engine's placeholder characters.
pub fn clean(s: &str) -> String { s.chars().filter(|c| !(('\u{E000}'..='\u{F8FF}').contains(c) || *c == '\u{FFFC}')).collect() }

/// Interface strings looked up this session that have no translation.
pub fn missing() -> Vec<String> { MISSING.lock().map(|m| m.iter().cloned().collect()).unwrap_or_default() }

/// Translate a template with `{}` placeholders filled in order ("Page {} of {}").
pub fn trf(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    // Fill in the logical-order text, then shape the whole line (Arabic reorders around the values).
    let raw = lang() == Lang::Ar;
    let t = if raw { table(Lang::Ar).get(template).cloned().unwrap_or_else(|| tr(template)) } else { tr(template) };
    let mut out = String::new();
    let mut parts = t.split("{}");
    if let Some(first) = parts.next() { out.push_str(first); }
    for (k, part) in parts.enumerate() { if let Some(a) = args.get(k) { out.push_str(&a.to_string()); } out.push_str(part); }
    if raw { shape(&out) } else { out }
}

/// Text ready to draw. egui joins Arabic letters and lays each word right to left, but it puts
/// the words themselves left to right. So the line is reordered with the Unicode bidi
/// algorithm (right-to-left paragraph), then each word is put back in reading order for egui
/// to shape: words, numbers and punctuation end up where an Arabic reader expects them.
pub fn shape(s: &str) -> String {
    let arabic = |c: char| ('\u{0600}'..='\u{06FF}').contains(&c) || ('\u{0750}'..='\u{077F}').contains(&c) || ('\u{FB50}'..='\u{FEFF}').contains(&c);
    if !s.chars().any(arabic) { return s.to_string(); }
    use unicode_bidi::{BidiInfo, Level};
    // Brackets inside right-to-left text are drawn mirrored (Unicode bidi rule L4): swap them
    // before reordering, as the reordering only moves characters.
    let first = BidiInfo::new(s, Some(Level::rtl()));
    let mirrored: String = s.char_indices().map(|(i, c)| if first.levels[i].is_rtl() { match c { '(' => ')', ')' => '(', '[' => ']', ']' => '[', '{' => '}', '}' => '{', '<' => '>', '>' => '<', '«' => '»', '»' => '«', _ => c } } else { c }).collect();
    let s = mirrored.as_str();
    let info = BidiInfo::new(s, Some(Level::rtl()));
    let mut visual = String::with_capacity(s.len());
    for (k, para) in info.paragraphs.iter().enumerate() {
        if k > 0 { visual.push('\n'); }
        let end = if s[..para.range.end].ends_with('\n') { para.range.end - 1 } else { para.range.end };
        visual.push_str(&info.reorder_line(para, para.range.start..end));
    }
    // Undo the reversal of each run of Arabic letters (egui reverses those itself); punctuation
    // and digits beside them keep their visual place.
    let mut out = String::with_capacity(visual.len());
    let mut run: Vec<char> = Vec::new();
    for c in visual.chars() {
        if arabic(c) { run.push(c); } else { out.extend(run.drain(..).rev()); out.push(c); }
    }
    out.extend(run.drain(..).rev());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_load_and_fall_back() {
        set(Lang::Fr);
        assert_eq!(tr("Home"), "Accueil");
        assert_eq!(tr("No such string"), "No such string");
        set(Lang::Ar);
        assert_eq!(tr("Insert"), "إدراج");
        assert!(rtl());
        set(Lang::En);
        assert_eq!(tr("Home"), "Home");
    }

    #[test]
    fn arabic_words_in_visual_order() {
        // Words reverse order; letters within a word stay in reading order (egui shapes them).
        assert_eq!(shape("ألوان النسق"), "النسق ألوان");
        assert_eq!(shape("الصفحة 1 من 3"), "3 من 1 الصفحة");
        assert_eq!(shape("Home"), "Home");
        assert_eq!(shape("ألوان إضافية…"), "…إضافية ألوان");
        set(Lang::Ar);
        assert_eq!(trf("Page {} of {}", &[&1, &3]), shape(&table(Lang::Ar)["Page {} of {}"].replacen("{}", "1", 1).replacen("{}", "3", 1)));
        set(Lang::En);
    }
}
