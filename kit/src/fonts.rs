//! Fonts for the interface and documents: every bundled (and installed) face registered with
//! egui as its own family, with Noto Sans and script faces as fallbacks.

use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

pub fn face_name(i: usize) -> String { format!("face{i}") }

/// Register every bundled face as its own egui family, falling back to Noto Sans.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    for (i, f) in word_engine::fonts::BUNDLED.iter().enumerate() {
        let name = face_name(i);
        let mut data = FontData::from_static(f.bytes());
        if f.weight > 0.0 { data.tweak.coords = egui::epaint::text::VariationCoords::new([(*b"wght", f.weight)]); }
        defs.font_data.insert(name.clone(), Arc::new(data));
        let mut chain = vec![name.clone()];
        if i != 0 { chain.push(face_name(0)); }
        chain.extend(defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
        defs.families.insert(FontFamily::Name(name.into()), chain);
    }
    // The chrome uses Noto Sans (as the JS app does); the other bundled faces back up symbols.
    let prop = defs.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, face_name(0));
    // Latin/Greek/Cyrillic, then Arabic, Hebrew and emoji for text typed into UI fields.
    let by_name = |file: &str| word_engine::fonts::BUNDLED.iter().position(|f| f.file == file && f.weight <= 400.0).unwrap_or(0);
    for i in [12, 4, by_name("NotoSansArabic.ttf"), by_name("NotoSansHebrew.ttf"), by_name("NotoSansMath-Regular.ttf"), by_name("NotoSansSymbols-Regular.ttf"), by_name("NotoSansSymbols2-Regular.ttf"), by_name("NotoEmoji.ttf")] { prop.push(face_name(i)); }
    // Bold chrome text falls back to the Arabic and symbol faces too (Arabic interface).
    defs.families.insert(FontFamily::Name("ui-bold".into()), vec![face_name(2), face_name(0), face_name(by_name("NotoSansArabic.ttf")), face_name(by_name("NotoSansHebrew.ttf")), face_name(by_name("NotoSansSymbols-Regular.ttf")), face_name(by_name("NotoSansSymbols2-Regular.ttf"))]);
    for i in [0, 12] { defs.families.entry(FontFamily::Monospace).or_default().push(face_name(i)); }
    ctx.set_fonts(defs);
}

