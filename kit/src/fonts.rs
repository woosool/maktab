//! Fonts for the interface and documents: every bundled (and installed) face registered with
//! egui as its own family, with Noto Sans and script faces as fallbacks. The faces the interface
//! needs are loaded up front; every other face (the rest of the bundled ones, installed fonts and
//! fonts the user added) is loaded the first time text is drawn in it.

use egui::epaint::text::{FontInsert, FontProvider};
use egui::{FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;
use std::sync::Arc;

pub fn face_name(i: usize) -> String { format!("face{i}") }

fn face_data(i: usize) -> FontData {
    let f = word_engine::fonts::face(i as u16);
    let mut data = FontData::from_static(f.bytes());
    data.index = f.index;
    if f.weight > 0.0 { data.tweak.coords = egui::epaint::text::VariationCoords::new([(*b"wght", f.weight)]); }
    data
}

fn by_file(file: &str) -> usize { word_engine::fonts::BUNDLED.iter().position(|f| f.file == file && f.weight <= 400.0).unwrap_or(0) }

/// The bundled faces behind every family: Noto Sans, then Arabic, Hebrew and symbols.
fn fallbacks() -> Vec<usize> {
    vec![0, by_file("NotoSansArabic.ttf"), by_file("NotoSansHebrew.ttf"), by_file("NotoSansMath-Regular.ttf"), by_file("NotoSansSymbols-Regular.ttf"), by_file("NotoSansSymbols2-Regular.ttf"), by_file("NotoEmoji.ttf")]
}

/// Faces loaded on demand: `faceN` families that aren't in the font definitions.
struct LazyFaces;

impl FontProvider for LazyFaces {
    fn fonts_for_family(&self, family: &FontFamily) -> Vec<FontInsert> {
        let FontFamily::Name(n) = family else { return vec![] };
        let Some(i) = n.strip_prefix("face").and_then(|s| s.parse::<usize>().ok()) else { return vec![] };
        if EAGER.contains(&i) { return vec![]; }
        let mut chain = vec![i];
        chain.extend(fallbacks().into_iter().filter(|f| *f != i));
        chain.into_iter().map(|k| FontInsert { name: face_name(k), data: face_data(k), families: vec![] }).collect()
    }
}

/// Faces in the font definitions from the start (the interface draws with them).
const EAGER: [usize; 6] = [0, 1, 2, 3, 4, 12];

/// Register the interface faces with egui, and the rest (every face `word_engine::fonts` knows,
/// including ones registered later) to load on first use as family `face_name(i)`.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut eager: Vec<usize> = EAGER.to_vec();
    eager.extend(fallbacks());
    eager.sort();
    eager.dedup();
    for &i in &eager { defs.font_data.insert(face_name(i), Arc::new(face_data(i))); }
    for &i in &EAGER {
        let name = face_name(i);
        let mut chain = vec![name.clone()];
        if i != 0 { chain.push(face_name(0)); }
        chain.extend(defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
        chain.extend(fallbacks().into_iter().skip(1).map(face_name));
        defs.families.insert(FontFamily::Name(name.into()), chain);
    }
    for &i in &fallbacks() {
        if EAGER.contains(&i) { continue; }
        let mut chain = vec![face_name(i), face_name(0)];
        chain.extend(defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
        defs.families.insert(FontFamily::Name(face_name(i).into()), chain);
    }
    // The chrome uses Noto Sans (as the JS app does); the other bundled faces back up symbols.
    let prop = defs.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, face_name(0));
    // Latin/Greek/Cyrillic, then Arabic, Hebrew and emoji for text typed into UI fields.
    for i in [12, 4].into_iter().chain(fallbacks().into_iter().skip(1)) { prop.push(face_name(i)); }
    // Bold chrome text falls back to the Arabic and symbol faces too (Arabic interface).
    defs.families.insert(FontFamily::Name("ui-bold".into()), vec![face_name(2), face_name(0), face_name(by_file("NotoSansArabic.ttf")), face_name(by_file("NotoSansHebrew.ttf")), face_name(by_file("NotoSansSymbols-Regular.ttf")), face_name(by_file("NotoSansSymbols2-Regular.ttf"))]);
    for i in [0, 12] { defs.families.entry(FontFamily::Monospace).or_default().push(face_name(i)); }
    ctx.set_fonts(defs);
    // The lazy faces are added once per context.
    let id = egui::Id::new("qalam-lazy-faces");
    if !ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false)) {
        ctx.data_mut(|d| d.insert_temp(id, true));
        ctx.add_font_provider(Arc::new(LazyFaces));
    }
}

/// The egui family that draws face `i` in its own font, for previews. Installed fonts preview in
/// the interface font (reading every installed font to fill a menu would be slow).
pub fn preview_family(i: u16) -> FontFamily {
    if word_engine::fonts::is_system_face(i) { FontFamily::Proportional } else { FontFamily::Name(face_name(i as usize).into()) }
}

/// Where fonts the user adds (imported or downloaded) are kept; shared by all the apps.
pub fn user_fonts_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("WOFFICE_FONTS_DIR") { return PathBuf::from(d); }
    let base = std::env::var_os("APPDATA").map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(if cfg!(target_os = "macos") { "Library/Application Support" } else { ".local/share" })))
        .unwrap_or_else(std::env::temp_dir);
    base.join("Woffice").join("Fonts")
}

pub fn is_font_file(p: &std::path::Path) -> bool {
    matches!(p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc")
}

/// Register the fonts the user added before (call once at startup, after system fonts load).
/// Returns how many families were found.
pub fn load_user_fonts() -> usize {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>, depth: u8) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() { if depth < 3 { walk(&p, out, depth + 1); } } else if is_font_file(&p) { out.push(p); }
        }
    }
    let mut files = vec![];
    walk(&user_fonts_dir(), &mut files, 0);
    files.sort();
    let mut fams: Vec<String> = files.iter().flat_map(|f| word_engine::fonts::register_font_file(f)).collect();
    fams.sort();
    fams.dedup();
    fams.len()
}
