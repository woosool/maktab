//! Interface languages: English, French and Arabic (right to left), following the sibling
//! apps' kit: catalogs keyed by the English text (`i18n/fr.json`, `i18n/ar.json`), anything
//! untranslated shows in English. `tr` returns text ready to draw: Arabic is put into the
//! visual order egui needs (see `shape`).

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Fr,
    Ar,
}

impl Lang {
    pub const ALL: [(Lang, &'static str); 3] = [
        (Lang::En, "English"),
        (Lang::Fr, "Français"),
        (Lang::Ar, "العربية"),
    ];
    pub fn code(&self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::Ar => "ar",
        }
    }
    pub fn from_code(c: &str) -> Lang {
        match c {
            "fr" => Lang::Fr,
            "ar" => Lang::Ar,
            _ => Lang::En,
        }
    }
    /// The language's own name, ready to draw.
    pub fn name(&self) -> String {
        shape(
            Self::ALL
                .iter()
                .find(|l| l.0 == *self)
                .map_or("English", |l| l.1),
        )
    }
}

thread_local! { static LANG: Cell<Lang> = const { Cell::new(Lang::En) }; }

pub fn set(lang: Lang) {
    LANG.with(|l| l.set(lang));
}
pub fn lang() -> Lang {
    LANG.with(|l| l.get())
}
/// Is the interface right to left (Arabic)?
pub fn rtl() -> bool {
    lang() == Lang::Ar
}

pub(crate) fn table(lang: Lang) -> &'static HashMap<String, String> {
    static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
    static AR: OnceLock<HashMap<String, String>> = OnceLock::new();
    static EMPTY: OnceLock<HashMap<String, String>> = OnceLock::new();
    let load =
        |text: &str| serde_json::from_str::<HashMap<String, String>>(text).unwrap_or_default();
    match lang {
        Lang::Fr => FR.get_or_init(|| load(include_str!("../i18n/fr.json"))),
        Lang::Ar => AR.get_or_init(|| load(include_str!("../i18n/ar.json"))),
        Lang::En => EMPTY.get_or_init(HashMap::new),
    }
}

/// Marks a static interface string for translation (it is translated where it is drawn).
pub const fn n_(s: &'static str) -> &'static str {
    s
}

/// Translate an interface string into the current language, ready to draw.
pub fn tr(s: &str) -> String {
    match lang() {
        Lang::En => s.to_string(),
        l => match table(l).get(s) {
            Some(t) if l == Lang::Ar => shape(t),
            Some(t) => t.clone(),
            None => s.to_string(),
        },
    }
}

/// The translation in reading order (not yet shaped for drawing).
pub fn raw(s: &str) -> String {
    match lang() {
        Lang::En => s.to_string(),
        l => table(l).get(s).cloned().unwrap_or_else(|| s.to_string()),
    }
}

/// Translate a template with `{}` placeholders filled in order ("{} of {}").
pub fn trf(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let ar = lang() == Lang::Ar;
    let t = match lang() {
        Lang::En => template.to_string(),
        l => table(l)
            .get(template)
            .cloned()
            .unwrap_or_else(|| template.to_string()),
    };
    let mut out = String::new();
    let mut parts = t.split("{}");
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for (k, part) in parts.enumerate() {
        if let Some(a) = args.get(k) {
            out.push_str(&a.to_string());
        }
        out.push_str(part);
    }
    if ar { shape(&out) } else { out }
}

fn arabic(c: char) -> bool {
    ('\u{0600}'..='\u{06FF}').contains(&c)
        || ('\u{0750}'..='\u{077F}').contains(&c)
        || ('\u{FB50}'..='\u{FEFF}').contains(&c)
}

/// Text ready to draw. egui joins Arabic letters and lays each word right to left, but puts
/// the words themselves left to right. So the line is reordered with the Unicode bidi
/// algorithm (right-to-left paragraph), then each Arabic run is put back in reading order for
/// egui to shape. Text without Arabic is returned unchanged.
pub fn shape(s: &str) -> String {
    if !s.chars().any(arabic) {
        return s.to_string();
    }
    use unicode_bidi::{BidiInfo, Level};
    let first = BidiInfo::new(s, Some(Level::rtl()));
    let mirrored: String = s
        .char_indices()
        .map(|(i, c)| {
            if first.levels[i].is_rtl() {
                match c {
                    '(' => ')',
                    ')' => '(',
                    '[' => ']',
                    ']' => '[',
                    '«' => '»',
                    '»' => '«',
                    _ => c,
                }
            } else {
                c
            }
        })
        .collect();
    let s = mirrored.as_str();
    let info = BidiInfo::new(s, Some(Level::rtl()));
    let mut visual = String::with_capacity(s.len());
    for (k, para) in info.paragraphs.iter().enumerate() {
        if k > 0 {
            visual.push('\n');
        }
        let end = if s[..para.range.end].ends_with('\n') {
            para.range.end - 1
        } else {
            para.range.end
        };
        visual.push_str(&info.reorder_line(para, para.range.start..end));
    }
    // egui shapes each run of Arabic letters right to left itself (letters joined, in the
    // right order), but lays runs out left to right: so the letters of each run go back
    // into reading order while words, numbers and punctuation keep their visual places.
    let mut out = String::with_capacity(visual.len());
    let mut run: Vec<char> = Vec::new();
    for c in visual.chars() {
        if arabic(c) {
            run.push(c);
        } else {
            out.extend(run.drain(..).rev());
            out.push(c);
        }
    }
    out.extend(run.drain(..).rev());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arabic_words_in_visual_order() {
        assert_eq!(shape("ألوان النسق"), "النسق ألوان");
        assert_eq!(shape("الصفحة 1 من 3"), "3 من 1 الصفحة");
        assert_eq!(shape("تحرير PDF"), "PDF تحرير");
        assert_eq!(shape("Home"), "Home");
    }

    #[test]
    fn falls_back_to_english() {
        set(Lang::Fr);
        assert_eq!(tr("No such string here"), "No such string here");
        set(Lang::En);
    }

    /// Every interface string in the sources (`tr("…")`, `trf("…")`, `n_("…")`) has a French
    /// and an Arabic translation.
    #[test]
    fn every_ui_string_is_translated() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path
                .file_name()
                .is_some_and(|n| n == "i18n.rs" || n == "visual_tour.rs")
            {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            for key in ui_strings(&src) {
                count += 1;
                for l in [Lang::Fr, Lang::Ar] {
                    if !table(l).contains_key(&key) {
                        missing.push(format!(
                            "{} [{}] {key:?}",
                            path.file_name().unwrap().to_string_lossy(),
                            l.code()
                        ));
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(count > 100, "found only {count} strings");
        assert!(
            missing.is_empty(),
            "{} untranslated:\n{}",
            missing.len(),
            missing.join("\n")
        );
    }

    /// String literals passed to `tr(`, `trf(` and `n_(`.
    pub(crate) fn ui_strings(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for marker in ["tr(", "trf(", "n_(", "raw("] {
            let mut rest = src;
            while let Some(i) = rest.find(marker) {
                let before = rest[..i].chars().last();
                rest = &rest[i + marker.len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let trimmed = rest.trim_start();
                if !trimmed.starts_with('"') {
                    continue;
                }
                rest = &trimmed[1..];
                let mut s = String::new();
                let mut chars = rest.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some(o) => s.push(o),
                            None => {}
                        },
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                out.push(s);
            }
        }
        out
    }
}
