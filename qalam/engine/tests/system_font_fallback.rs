//! scripts with no bundled face (Bengali, Chinese) draw with an installed
//! font instead of empty boxes. Its own test binary, so system fonts load before any layout.

use word_engine::fonts;

#[test]
fn bengali_and_chinese_fall_back_to_installed_fonts() {
    fonts::load_system_fonts(None);
    let latin = fonts::resolve("Calibri", false, false);
    for c in ['অ', 'ব', '中', '字'] {
        let installed = (0..fonts::faces().len()).any(|i| fonts::is_system_face(i as u16) && fonts::has_glyph(i as u16, c));
        let k = fonts::fallback(latin, c);
        if installed { assert!(fonts::has_glyph(k.face, c), "{c} draws with {}", fonts::face(k.face).family); }
        else { eprintln!("no installed font covers {c}; skipped"); }
    }
    // Arabic keeps its bundled Noto faces.
    let ar = fonts::fallback(latin, 'ب');
    assert!(fonts::face(ar.face).family.contains("Arabic"));
}
