//! Settings kept in settings.json in the app's config folder.

use serde_json::{Value, json};

use crate::app::ThemeChoice;
use crate::i18n::Lang;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub lang: Lang,
    pub theme: ThemeChoice,
    /// Look for a new version once a day.
    pub auto_update: bool,
    pub last_update_check: u64,
    pub skipped_version: Option<String>,
    /// Show the splash card at start.
    pub splash: bool,
    /// The window: size, position and maximized, remembered for next time.
    pub window: Option<([f32; 2], Option<[f32; 2]>, bool)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: Lang::En,
            theme: ThemeChoice::System,
            auto_update: true,
            last_update_check: 0,
            skipped_version: None,
            splash: true,
            window: None,
        }
    }
}

fn path() -> std::path::PathBuf {
    crate::config_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        let Ok(text) = std::fs::read_to_string(path()) else {
            return Settings::default();
        };
        Self::from_json(&serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    pub fn from_json(v: &Value) -> Settings {
        let d = Settings::default();
        Settings {
            lang: Lang::from_code(v["lang"].as_str().unwrap_or("en")),
            theme: match v["theme"].as_str() {
                Some("light") => ThemeChoice::Light,
                Some("dark") => ThemeChoice::Dark,
                _ => ThemeChoice::System,
            },
            auto_update: v["auto_update"].as_bool().unwrap_or(d.auto_update),
            last_update_check: v["last_update_check"].as_u64().unwrap_or(0),
            skipped_version: v["skipped_version"].as_str().map(String::from),
            splash: v["splash"].as_bool().unwrap_or(true),
            window: v.get("window").and_then(|w| {
                let f2 = |x: &Value| Some([x.get(0)?.as_f64()? as f32, x.get(1)?.as_f64()? as f32]);
                Some((
                    f2(&w["size"])?,
                    f2(&w["pos"]),
                    w["maximized"].as_bool().unwrap_or(false),
                ))
            }),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "lang": self.lang.code(),
            "theme": match self.theme { ThemeChoice::System => "system", ThemeChoice::Light => "light", ThemeChoice::Dark => "dark" },
            "auto_update": self.auto_update,
            "last_update_check": self.last_update_check,
            "skipped_version": self.skipped_version,
            "splash": self.splash,
            "window": self.window.map(|(s, p, m)| json!({ "size": s, "pos": p, "maximized": m })),
        })
    }

    pub fn save(&self) {
        if cfg!(test) {
            return;
        }
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(
            p,
            serde_json::to_string_pretty(&self.to_json()).unwrap_or_default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let s = Settings {
            lang: Lang::Ar,
            theme: ThemeChoice::Dark,
            auto_update: false,
            last_update_check: 42,
            skipped_version: Some("0.3.0".into()),
            splash: false,
            window: Some(([1200.0, 800.0], Some([40.0, 60.0]), true)),
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_json(&Value::Null), Settings::default());
    }
}
