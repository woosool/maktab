//! Bundled fonts, their metrics and text shaping (font-metrics.js, font-families.js).
//!
//! Metric-compatible substitutes: Calibri→Carlito, Cambria→Caladea,
//! Arial→Liberation Sans, Times New Roman→Liberation Serif,
//! Courier New→Liberation Mono. Everything else uses Noto Sans, as the JS
//! engine does. Arabic, Hebrew and emoji fall back to bundled Noto faces.
//! Text is shaped with rustybuzz (OpenType GSUB/GPOS: Arabic joining,
//! ligatures, marks), and layout and painting use the same glyphs.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

macro_rules! face {
    ($family:expr, $bold:expr, $italic:expr, $file:expr) => { face!($family, $bold, $italic, $file, 0.0) };
    ($family:expr, $bold:expr, $italic:expr, $file:expr, $weight:expr) => {
        FaceData { family: $family, bold: $bold, italic: $italic, file: $file, weight: $weight, data: face_bytes!($file), path: None, index: 0 }
    };
}

#[cfg(feature = "embed-assets")]
macro_rules! face_bytes { ($file:expr) => { Some(include_bytes!(concat!("../../assets/fonts/", $file))) }; }
#[cfg(not(feature = "embed-assets"))]
macro_rules! face_bytes { ($file:expr) => { None }; }

pub struct FaceData {
    pub family: &'static str,
    pub bold: bool,
    pub italic: bool,
    pub file: &'static str,
    /// `wght` axis value for instances of a variable font (0 = default instance).
    pub weight: f32,
    data: Option<&'static [u8]>,
    /// A system font: its file and the face's index in a collection (.ttc).
    pub path: Option<&'static str>,
    pub index: u32,
}

impl FaceData {
    /// The font file: compiled in, or read from the resources folder once.
    pub fn bytes(&self) -> &'static [u8] {
        if let Some(b) = self.data { return b; }
        static LOADED: Mutex<Option<HashMap<&'static str, &'static [u8]>>> = Mutex::new(None);
        let mut m = LOADED.lock().unwrap_or_else(|e| e.into_inner());
        let key = self.path.unwrap_or(self.file);
        *m.get_or_insert_with(HashMap::new).entry(key).or_insert_with(|| match self.path {
            // A system font that disappeared since discovery reads as empty (it then has no glyphs).
            Some(p) => std::fs::read(p).map(|b| &*Box::leak(b.into_boxed_slice())).unwrap_or(&[]),
            None => crate::resources::require(&format!("fonts/{}", self.file)),
        })
    }
}

/// The fonts that ship with the app.
pub static BUNDLED: &[FaceData] = &[
    face!("Noto Sans", false, false, "NotoSans.ttf"),
    face!("Noto Sans", false, true, "NotoSans-Italic.ttf"),
    face!("Noto Sans", true, false, "NotoSans.ttf", 700.0),
    face!("Noto Sans", true, true, "NotoSans-Italic.ttf", 700.0),
    face!("Carlito", false, false, "Carlito-Regular.ttf"),
    face!("Carlito", true, false, "Carlito-Bold.ttf"),
    face!("Carlito", false, true, "Carlito-Italic.ttf"),
    face!("Carlito", true, true, "Carlito-BoldItalic.ttf"),
    face!("Caladea", false, false, "Caladea-Regular.ttf"),
    face!("Caladea", true, false, "Caladea-Bold.ttf"),
    face!("Caladea", false, true, "Caladea-Italic.ttf"),
    face!("Caladea", true, true, "Caladea-BoldItalic.ttf"),
    face!("Liberation Sans", false, false, "LiberationSans-Regular.ttf"),
    face!("Liberation Sans", true, false, "LiberationSans-Bold.ttf"),
    face!("Liberation Sans", false, true, "LiberationSans-Italic.ttf"),
    face!("Liberation Sans", true, true, "LiberationSans-BoldItalic.ttf"),
    face!("Liberation Serif", false, false, "LiberationSerif-Regular.ttf"),
    face!("Liberation Serif", true, false, "LiberationSerif-Bold.ttf"),
    face!("Liberation Serif", false, true, "LiberationSerif-Italic.ttf"),
    face!("Liberation Serif", true, true, "LiberationSerif-BoldItalic.ttf"),
    face!("Liberation Mono", false, false, "LiberationMono-Regular.ttf"),
    face!("Liberation Mono", true, false, "LiberationMono-Bold.ttf"),
    face!("Liberation Mono", false, true, "LiberationMono-Italic.ttf"),
    face!("Liberation Mono", true, true, "LiberationMono-BoldItalic.ttf"),
    face!("Noto Sans Arabic", false, false, "NotoSansArabic.ttf", 400.0),
    face!("Noto Sans Arabic", true, false, "NotoSansArabic.ttf", 700.0),
    face!("Noto Naskh Arabic", false, false, "NotoNaskhArabic.ttf", 400.0),
    face!("Noto Naskh Arabic", true, false, "NotoNaskhArabic.ttf", 700.0),
    face!("Noto Sans Hebrew", false, false, "NotoSansHebrew.ttf", 400.0),
    face!("Noto Sans Hebrew", true, false, "NotoSansHebrew.ttf", 700.0),
    face!("Noto Emoji", false, false, "NotoEmoji.ttf", 400.0),
    face!("Noto Sans Math", false, false, "NotoSansMath-Regular.ttf"),
    face!("Noto Sans Symbols", false, false, "NotoSansSymbols-Regular.ttf"),
    face!("Noto Sans Symbols 2", false, false, "NotoSansSymbols2-Regular.ttf"),
    // Arabic text faces (SIL Open Font License). Read from disk only when first used.
    face!("Amiri", false, false, "Amiri-Regular.ttf"),
    face!("Amiri", true, false, "Amiri-Bold.ttf"),
    face!("Amiri", false, true, "Amiri-Italic.ttf"),
    face!("Amiri", true, true, "Amiri-BoldItalic.ttf"),
    face!("Cairo", false, false, "Cairo.ttf", 400.0),
    face!("Cairo", true, false, "Cairo.ttf", 700.0),
    face!("Tajawal", false, false, "Tajawal-Regular.ttf"),
    face!("Tajawal", true, false, "Tajawal-Bold.ttf"),
    face!("Noto Kufi Arabic", false, false, "NotoKufiArabic.ttf", 400.0),
    face!("Noto Kufi Arabic", true, false, "NotoKufiArabic.ttf", 700.0),
];

static SYSTEM: OnceLock<Vec<FaceData>> = OnceLock::new();

/// Every font: the bundled ones, then fonts installed on the system (after `load_system_fonts`).
pub fn faces() -> &'static [FaceData] {
    static ALL: OnceLock<Vec<FaceData>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut v: Vec<FaceData> = BUNDLED.iter().map(|f| FaceData { family: f.family, bold: f.bold, italic: f.italic, file: f.file, weight: f.weight, data: f.data, path: None, index: 0 }).collect();
        if let Some(sys) = SYSTEM.get() { v.extend(sys.iter().map(|f| FaceData { family: f.family, bold: f.bold, italic: f.italic, file: f.file, weight: 0.0, data: None, path: f.path, index: f.index })); }
        v
    })
}

// ---- fonts embedded in documents --------------------------------------------------------------

/// Room for fonts registered while the app runs: fonts that documents bring with them, and
/// fonts the user added (imported or downloaded; see `register_font_file`).
const EMBEDDED_SLOTS: usize = 1024;
static EMBEDDED: [OnceLock<FaceData>; EMBEDDED_SLOTS] = [const { OnceLock::new() }; EMBEDDED_SLOTS];
static EMBEDDED_USED: Mutex<usize> = Mutex::new(0);

/// The face at an index (bundled and system faces, then embedded ones).
pub fn face(i: u16) -> &'static FaceData {
    let all = faces();
    let i = i as usize;
    if i < all.len() { return &all[i]; }
    EMBEDDED.get(i - all.len()).and_then(|s| s.get()).unwrap_or(&all[0])
}
fn face_slots() -> usize { faces().len() + EMBEDDED_SLOTS }

/// Make a font that came inside a document available by its family name (it's preferred over
/// installed and bundled fonts of that name). The same font registered again is reused.
/// Returns false when it isn't a font (or there's no room left).
pub fn register_embedded(family: &str, bold: bool, italic: bool, bytes: Vec<u8>) -> bool {
    if ttf_parser::Face::parse(&bytes, 0).is_err() { return false; }
    let mut used = EMBEDDED_USED.lock().unwrap_or_else(|e| e.into_inner());
    let fam = family.trim();
    if EMBEDDED[..*used].iter().filter_map(|s| s.get()).any(|f| f.family.eq_ignore_ascii_case(fam) && f.bold == bold && f.italic == italic && f.data.is_some_and(|d| d == bytes.as_slice())) { return true; }
    if *used >= EMBEDDED_SLOTS { return false; }
    let leak_s = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let data: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let _ = EMBEDDED[*used].set(FaceData { family: leak_s(fam.to_string()), bold, italic, file: "embedded", weight: 0.0, data: Some(data), path: None, index: 0 });
    *used += 1;
    true
}

/// The most recent embedded face of a family and style (index), if any.
fn embedded_face(family: &str, bold: bool, italic: bool) -> Option<usize> {
    let used = *EMBEDDED_USED.lock().unwrap_or_else(|e| e.into_inner());
    (0..used).rev().find(|&k| EMBEDDED[k].get().is_some_and(|f| f.family.eq_ignore_ascii_case(family.trim()) && f.bold == bold && f.italic == italic)).map(|k| faces().len() + k)
}

/// Families that documents brought with them.
pub fn embedded_families() -> Vec<String> { runtime_families("embedded") }

/// Families the user added (Import Font, Google Fonts), sorted.
pub fn user_families() -> Vec<String> {
    let mut v = runtime_families(USER_FILE);
    v.sort_by_key(|s| s.to_lowercase());
    v.dedup();
    v
}

fn runtime_families(file: &str) -> Vec<String> {
    let used = *EMBEDDED_USED.lock().unwrap_or_else(|e| e.into_inner());
    let mut v: Vec<String> = vec![];
    for f in EMBEDDED[..used].iter().filter_map(|s| s.get()).filter(|f| f.file == file) { if !v.iter().any(|x| x == f.family) { v.push(f.family.to_string()); } }
    v
}

/// The `file` of faces the user added.
pub const USER_FILE: &str = "user";

/// Is this face one installed on the computer (not bundled, embedded or added by the user)?
pub fn is_system_face(i: u16) -> bool { (i as usize) < faces().len() && face(i).path.is_some() }

/// Make the fonts in a file the user added (.ttf, .otf, .ttc) available by their family names,
/// ahead of installed and bundled fonts of the same name. The file is read again only when its
/// text is drawn. A variable font with a weight axis gives a regular and a bold instance.
/// Returns the families found (none when it isn't a usable font).
pub fn register_font_file(path: &std::path::Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else { return vec![] };
    let key = path.to_string_lossy().to_string();
    let n = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
    let mut found: Vec<(u32, String, bool, bool, f32)> = vec![];
    for ix in 0..n {
        let Ok(f) = ttf_parser::Face::parse(&bytes, ix) else { continue };
        if f.tables().cmap.is_none() { continue; }
        let Some(fam) = name_of(&f, &[16, 1]) else { continue };
        let fam = fam.trim().to_string();
        let italic = f.is_italic() || f.is_oblique();
        let wght = f.variation_axes().into_iter().find(|a| a.tag == ttf_parser::Tag::from_bytes(b"wght"));
        match wght {
            Some(a) => {
                let w = |v: f32| v.clamp(a.min_value, a.max_value);
                found.push((ix, fam.clone(), false, italic, w(400.0)));
                if a.max_value >= 600.0 { found.push((ix, fam, true, italic, w(700.0))); }
            }
            None => found.push((ix, fam, f.is_bold() || f.weight().to_number() >= 600, italic, 0.0)),
        }
    }
    if found.is_empty() { return vec![]; }
    if let Some(c) = CJK_CACHE.get() { c.lock().unwrap_or_else(|e| e.into_inner()).clear(); }
    let path_s: &'static str = Box::leak(key.into_boxed_str());
    let mut used = EMBEDDED_USED.lock().unwrap_or_else(|e| e.into_inner());
    let mut families = vec![];
    for (ix, fam, bold, italic, weight) in found {
        if !families.contains(&fam) { families.push(fam.clone()); }
        let same = |f: &FaceData| f.file == USER_FILE && f.path == Some(path_s) && f.index == ix && f.weight == weight;
        if EMBEDDED[..*used].iter().filter_map(|s| s.get()).any(same) { continue; }
        if *used >= EMBEDDED_SLOTS { break; }
        let _ = EMBEDDED[*used].set(FaceData { family: Box::leak(fam.into_boxed_str()), bold, italic, file: USER_FILE, weight, data: None, path: Some(path_s), index: ix });
        *used += 1;
    }
    families
}

/// Is a font of this name available as itself (or as a metric-compatible substitute), rather
/// than drawn with an unrelated fallback?
pub fn is_available(name: &str) -> bool {
    let n = name.trim();
    if n.is_empty() || embedded_face(n, false, false).is_some() || embedded_face(n, true, false).is_some() || system_family(n).is_some() { return true; }
    let used = *EMBEDDED_USED.lock().unwrap_or_else(|e| e.into_inner());
    if EMBEDDED[..used].iter().filter_map(|s| s.get()).any(|f| f.family.eq_ignore_ascii_case(n)) { return true; }
    FAMILIES.iter().any(|(d, _)| d.eq_ignore_ascii_case(n))
}

/// Folders where the operating system keeps fonts.
fn system_font_dirs() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut d: Vec<std::path::PathBuf> = vec![];
    if cfg!(target_os = "macos") {
        d.extend(["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts"].map(std::path::PathBuf::from));
        if let Some(h) = &home { d.push(h.join("Library/Fonts")); }
    } else if cfg!(target_os = "windows") {
        let win = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
        d.push(win.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") { d.push(std::path::PathBuf::from(l).join("Microsoft\\Windows\\Fonts")); }
    } else {
        d.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(std::path::PathBuf::from));
        if let Some(h) = &home { d.push(h.join(".local/share/fonts")); d.push(h.join(".fonts")); }
    }
    d
}

fn font_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>, depth: u8) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() { if depth < 4 { font_files(&p, out, depth + 1); } continue; }
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") { out.push(p); }
    }
}

fn name_of(face: &ttf_parser::Face, ids: &[u16]) -> Option<String> {
    for &id in ids {
        let mut best = None;
        for n in face.names() {
            if n.name_id != id || !n.is_unicode() { continue; }
            if let Some(s) = n.to_string() { if n.language_id == 0x0409 || n.platform_id == ttf_parser::PlatformId::Unicode { return Some(s); } best.get_or_insert(s); }
        }
        if best.is_some() { return best; }
    }
    None
}

/// Find the fonts installed on this computer and make them available to documents (by family
/// name). Discovery reads each font's names once; `cache` (a file) remembers them by path,
/// size and date so later starts are quick. Call before any text is laid out.
pub fn load_system_fonts(cache: Option<&std::path::Path>) -> usize {
    if SYSTEM.get().is_some() { return SYSTEM.get().map(|v| v.len()).unwrap_or(0); }
    let mut files = vec![];
    for d in system_font_dirs() { font_files(&d, &mut files, 0); }
    files.sort();
    files.dedup();
    // cache lines: path \t size \t mtime \t index \t family \t bold \t italic
    let old: HashMap<String, Vec<(u64, u64, u32, String, bool, bool)>> = cache.and_then(|c| std::fs::read_to_string(c).ok()).map(|t| {
        let mut m: HashMap<String, Vec<_>> = HashMap::new();
        for l in t.lines() {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() == 7 { if let (Ok(sz), Ok(mt), Ok(ix)) = (f[1].parse(), f[2].parse(), f[3].parse()) { m.entry(f[0].to_string()).or_default().push((sz, mt, ix, f[4].to_string(), f[5] == "1", f[6] == "1")); } }
        }
        m
    }).unwrap_or_default();
    let bundled: std::collections::HashSet<&str> = BUNDLED.iter().map(|f| f.family).collect();
    let mut found: Vec<FaceData> = vec![];
    let mut lines = String::new();
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    for p in files {
        let Ok(meta) = std::fs::metadata(&p) else { continue };
        let (size, mtime) = (meta.len(), meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0));
        let key = p.to_string_lossy().to_string();
        let entries: Vec<(u32, String, bool, bool)> = match old.get(&key).filter(|v| v.iter().all(|e| e.0 == size && e.1 == mtime)) {
            Some(v) => v.iter().map(|e| (e.2, e.3.clone(), e.4, e.5)).collect(),
            None => {
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let n = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
                (0..n).filter_map(|ix| {
                    let f = ttf_parser::Face::parse(&bytes, ix).ok()?;
                    // Text fonts only: they need a Unicode character map and outlines.
                    ['a', '\u{0627}', '\u{1000}', '\u{0985}', '\u{0915}', '\u{4E2D}', '\u{0E01}'].iter().find_map(|&c| f.glyph_index(c))?;
                    let fam = name_of(&f, &[16, 1])?;
                    Some((ix, fam, f.is_bold(), f.is_italic()))
                }).collect()
            }
        };
        for (ix, fam, bold, italic) in entries {
            lines += &format!("{key}\t{size}\t{mtime}\t{ix}\t{fam}\t{}\t{}\n", bold as u8, italic as u8);
            // Bundled families keep their bundled faces; colour emoji fonts are left out.
            if bundled.contains(fam.as_str()) || fam.contains("Emoji") || fam.starts_with('.') { continue; }
            if found.iter().any(|f| f.family == fam && f.bold == bold && f.italic == italic) { continue; }
            found.push(FaceData { family: leak(fam), bold, italic, file: leak(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), weight: 0.0, data: None, path: Some(leak(key.clone())), index: ix });
        }
    }
    if let Some(c) = cache { let _ = std::fs::create_dir_all(c.parent().unwrap_or(std::path::Path::new("."))); let _ = std::fs::write(c, lines); }
    let n = found.len();
    let _ = SYSTEM.set(found);
    n
}

/// Font family names to offer in font menus: the usual document fonts, then installed ones.
pub fn family_names() -> Vec<String> {
    // Fonts documents brought with them come first.
    let mut out: Vec<String> = embedded_families();
    for f in FAMILIES { if !out.iter().any(|o| o == f.0) { out.push(f.0.to_string()); } }
    // Then installed fonts and the ones the user added, together in alphabetical order.
    let mut sys: Vec<String> = SYSTEM.get().map(|v| v.iter().map(|f| f.family.to_string()).collect()).unwrap_or_default();
    sys.extend(user_families());
    sys.sort_by_key(|s| s.to_lowercase());
    sys.dedup();
    for s in sys { if !out.iter().any(|o| o.eq_ignore_ascii_case(&s)) { out.push(s); } }
    out
}

/// An installed family with this exact name, if any.
fn system_family(name: &str) -> Option<&'static str> {
    SYSTEM.get()?.iter().find(|f| f.family.eq_ignore_ascii_case(name.trim())).map(|f| f.family)
}

/// Font families offered in the font picker (display name, bundled family).
pub const FAMILIES: &[(&str, &str)] = &[
    ("Noto Sans", "Noto Sans"), ("Calibri", "Carlito"), ("Cambria", "Caladea"), ("Arial", "Liberation Sans"),
    ("Times New Roman", "Liberation Serif"), ("Courier New", "Liberation Mono"),
    ("Carlito", "Carlito"), ("Caladea", "Caladea"), ("Liberation Sans", "Liberation Sans"), ("Liberation Serif", "Liberation Serif"), ("Liberation Mono", "Liberation Mono"),
    ("Noto Naskh Arabic", "Noto Naskh Arabic"), ("Noto Sans Arabic", "Noto Sans Arabic"), ("Noto Kufi Arabic", "Noto Kufi Arabic"),
    ("Amiri", "Amiri"), ("Cairo", "Cairo"), ("Tajawal", "Tajawal"), ("Noto Sans Hebrew", "Noto Sans Hebrew"),
    ("Cambria Math", "Noto Sans Math"), ("Noto Sans Math", "Noto Sans Math"), ("Symbol", "Noto Sans Symbols"),
];

/// The bundled family used for a document font name.
pub fn bundled_family(name: &str) -> &'static str {
    let n = name.trim().to_ascii_lowercase();
    if let Some((_, f)) = FAMILIES.iter().find(|(d, _)| d.to_ascii_lowercase() == n) { return f; }
    match n.as_str() {
        "calibri light" | "segoe ui" | "aptos" | "candara" | "corbel" => "Carlito",
        "georgia" | "garamond" | "book antiqua" | "palatino linotype" | "times" | "constantia" => "Liberation Serif",
        "helvetica" | "verdana" | "tahoma" | "trebuchet ms" | "arial narrow" | "century gothic" | "franklin gothic" => "Liberation Sans",
        "courier" | "consolas" | "lucida console" | "menlo" | "monaco" => "Liberation Mono",
        // Common Arabic document fonts.
        "traditional arabic" | "arabic typesetting" | "sakkal majalla" | "scheherazade" | "scheherazade new" | "lateef" | "amiri quran" => "Amiri",
        "simplified arabic" | "noto naskh arabic ui" | "al bayan" | "geeza pro" | "baghdad" | "nadeem" => "Noto Naskh Arabic",
        "dubai" | "segoe ui arabic" | "simplified arabic fixed" | "andalus" | "aldhabi" | "almarai" | "changa" => "Noto Sans Arabic",
        "noto kufi arabic ui" | "reem kufi" | "kufi" => "Noto Kufi Arabic",
        "cairo play" => "Cairo",
        "david" | "miriam" | "narkisim" | "frankruehl" | "levenim mt" | "gisha" | "aharoni" | "arial hebrew" => "Noto Sans Hebrew",
        _ if n.contains("arab") || n.contains("naskh") || n.contains("kufi") => "Noto Naskh Arabic",
        _ if n.contains("hebrew") => "Noto Sans Hebrew",
        _ if n.contains("mono") || n.contains("courier") => "Liberation Mono",
        _ if n.contains("serif") && !n.contains("sans") => "Liberation Serif",
        _ => "Noto Sans",
    }
}

/// A resolved face: index into `faces()` plus synthetic emboldening when the family has no bold face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontKey { pub face: u16, pub synthetic_bold: bool }

fn find_face(family: &str, bold: bool, italic: bool) -> Option<usize> { faces().iter().position(|f| f.family == family && f.bold == bold && f.italic == italic) }

/// The face for a document font name: the installed font of that name when there is one,
/// else the bundled substitute.
pub fn resolve(name: &str, bold: bool, italic: bool) -> FontKey {
    // A font the document brought with it comes first.
    if let Some(i) = embedded_face(name, bold, italic) { return FontKey { face: i as u16, synthetic_bold: false }; }
    if let Some(i) = embedded_face(name, bold, false).or_else(|| embedded_face(name, false, italic)) { return FontKey { face: i as u16, synthetic_bold: false }; }
    if let Some(i) = embedded_face(name, false, false) { return FontKey { face: i as u16, synthetic_bold: bold }; }
    let exact_bundled = FAMILIES.iter().any(|(d, f)| d.eq_ignore_ascii_case(name.trim()) && d == f);
    if system_family(name).is_none() {
        if let Some(a) = close_families(name).iter().find(|a| !a.eq_ignore_ascii_case(name.trim()) && embedded_face(a, false, false).is_some()) { return resolve(a, bold, italic); }
    }
    let installed = system_family(name).or_else(|| close_families(name).iter().find_map(|a| system_family(a)));
    match installed.filter(|_| !exact_bundled) { Some(f) => resolve_family(f, bold, italic), None => resolve_family(bundled_family(name), bold, italic) }
}

/// installed designs that stand in for a missing font far better than the
/// generic bundled substitute (Palatino Linotype is Palatino on macOS, Pagella/P052 on Linux).
fn close_families(name: &str) -> &'static [&'static str] {
    match name.trim().to_ascii_lowercase().as_str() {
        "palatino linotype" | "book antiqua" | "palatino" => &["Palatino Linotype", "Palatino", "Book Antiqua", "TeX Gyre Pagella", "URW Palladio L", "P052"],
        "century schoolbook" => &["Century Schoolbook", "New Century Schoolbook", "TeX Gyre Schola", "C059"],
        "bookman old style" => &["Bookman Old Style", "Bookman", "TeX Gyre Bonum", "URW Bookman"],
        _ => &[],
    }
}

fn resolve_family(family: &str, bold: bool, italic: bool) -> FontKey {
    if let Some(i) = find_face(family, bold, italic) { return FontKey { face: i as u16, synthetic_bold: false }; }
    if let Some(i) = find_face(family, bold, false) { return FontKey { face: i as u16, synthetic_bold: false }; }
    if let Some(i) = find_face(family, false, italic) { return FontKey { face: i as u16, synthetic_bold: bold }; }
    FontKey { face: find_face(family, false, false).unwrap_or(0) as u16, synthetic_bold: bold }
}

/// Characters of complex scripts that use a run's complex-script properties (w:cs, szCs, bCs).
pub fn is_complex(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10E60..=0x10E7F | 0x1EE00..=0x1EEFF)
}
fn is_hebrew(c: char) -> bool { matches!(c as u32, 0x0590..=0x05FF | 0xFB1D..=0xFB4F) }
fn is_emoji(c: char) -> bool { matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2300..=0x23FF | 0x2B00..=0x2BFF | 0xFE0F | 0x200D) }

/// The face that draws `c`: the run's face when it has the glyph, else a bundled fallback by
/// script (Arabic → Naskh for serif runs, Sans otherwise; Hebrew; emoji; then Noto Sans).
pub fn fallback(key: FontKey, c: char) -> FontKey {
    if c.is_control() || has_glyph(key.face, c) { return key; }
    let bold = face(key.face).bold || key.synthetic_bold;
    let serif = matches!(face(key.face).family, "Liberation Serif" | "Caladea" | "Noto Naskh Arabic" | "Amiri");
    let chain: &[&str] = if is_complex(c) && !is_hebrew(c) {
        if serif { &["Noto Naskh Arabic", "Noto Sans Arabic"] } else { &["Noto Sans Arabic", "Noto Naskh Arabic"] }
    } else if is_hebrew(c) { &["Noto Sans Hebrew"] } else if is_emoji(c) { &["Noto Emoji", "Noto Sans"] } else { &["Noto Sans", "Liberation Sans", "Noto Sans Math", "Noto Sans Symbols", "Noto Sans Symbols 2", "Noto Emoji"] };
    for family in chain {
        let k = resolve_family(family, bold, false);
        if has_glyph(k.face, c) { return k; }
    }
    if let Some(face) = cjk_face(c, bold) { return FontKey { face, synthetic_bold: bold && !self::face(face).bold }; }
    // Scripts with no bundled face (Bengali, Devanagari, CJK, Thai…) use an installed font.
    system_fallback(c, bold).unwrap_or(key)
}

/// An installed upright face with a glyph for `c`, remembered per 128-character block so a run
/// of one script keeps one face and the font files are searched once.
fn system_fallback(c: char, bold: bool) -> Option<FontKey> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: OnceLock<Mutex<HashMap<(u32, bool), Option<u16>>>> = OnceLock::new();
    let block = (c as u32 >> 7, bold);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(&hit) = cache.lock().ok()?.get(&block) {
        if hit.is_some_and(|f| has_glyph(f, c)) { return hit.map(|f| FontKey { face: f, synthetic_bold: bold && !face(f).bold }); }
        if hit.is_none() { return None; }
    }
    let all = faces();
    let pick = |want_bold: bool| (0..all.len()).find(|&i| all[i].path.is_some() && !all[i].italic && all[i].bold == want_bold && has_glyph(i as u16, c));
    let found = pick(bold).or_else(|| if bold { pick(false) } else { None }).map(|i| i as u16);
    if let Ok(mut m) = cache.lock() { m.insert(block, found); }
    found.map(|f| FontKey { face: f, synthetic_bold: bold && !face(f).bold })
}

/// installed font families likely to cover `c`'s script when no bundled font
/// does (Myanmar, Thai, Indic, …), so only those few are opened (#605).
fn script_hints(c: char) -> &'static [&'static str] {
    match c as u32 {
        0x1000..=0x109F | 0xA9E0..=0xA9FF | 0xAA60..=0xAA7F => &["Myanmar", "Padauk", "Zawgyi"],
        0x0E00..=0x0E7F => &["Thai", "Thonburi", "Ayuthaya", "Sukhumvit", "Tahoma", "Leelawadee"],
        0x0E80..=0x0EFF => &["Lao"],
        0x1780..=0x17FF | 0x19E0..=0x19FF => &["Khmer"],
        0x0900..=0x097F | 0xA8E0..=0xA8FF => &["Devanagari", "Kohinoor", "Mangal", "Nirmala"],
        0x0980..=0x09FF => &["Bengali", "Bangla", "Vrinda", "Nirmala"],
        0x0A00..=0x0A7F => &["Gurmukhi", "Raavi", "Nirmala"],
        0x0A80..=0x0AFF => &["Gujarati", "Shruti", "Nirmala"],
        0x0B00..=0x0B7F => &["Oriya", "Odia", "Nirmala"],
        0x0B80..=0x0BFF => &["Tamil", "Latha", "Nirmala"],
        0x0C00..=0x0C7F => &["Telugu", "Gautami", "Nirmala"],
        0x0C80..=0x0CFF => &["Kannada", "Tunga", "Nirmala"],
        0x0D00..=0x0D7F => &["Malayalam", "Kartika", "Nirmala"],
        0x0D80..=0x0DFF => &["Sinhala", "Iskoola"],
        0x0F00..=0x0FFF => &["Tibetan", "Kailasa", "Himalaya"],
        0x10A0..=0x10FF => &["Georgian"],
        0x0530..=0x058F => &["Armenian"],
        0x1200..=0x139F | 0x2D80..=0x2DDF => &["Ethiopic", "Kefa", "Nyala", "Ebrima"],
        0x13A0..=0x13FF => &["Cherokee", "Plantagenet"],
        0x0700..=0x074F => &["Syriac"],
        0x0780..=0x07BF => &["Thaana", "MV Boli"],
        _ => &[],
    }
}

/// Han, kana, hangul, bopomofo and CJK punctuation/compatibility forms: none are in the bundled
/// fonts, so they come from an installed (or user-added) CJK font.
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x2E80..=0x2FDF | 0x3000..=0x31FF | 0x3200..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

static CJK_CACHE: OnceLock<Mutex<HashMap<(char, bool), Option<u16>>>> = OnceLock::new();

/// an installed or user-added CJK font that has `c` (best-known families
/// first), remembered per character so fonts are only opened once.
fn cjk_face(c: char, bold: bool) -> Option<u16> {
    const PREFERRED: &[&str] = &["PingFang SC", "Hiragino Sans GB", "Hiragino Sans", "Heiti SC", "Songti SC", "Apple SD Gothic Neo", "Microsoft YaHei", "Microsoft JhengHei", "Yu Gothic", "Meiryo", "Malgun Gothic", "SimSun", "MS Gothic", "Noto Sans CJK SC", "Noto Sans SC", "Source Han Sans SC", "WenQuanYi Micro Hei", "Droid Sans Fallback", "Arial Unicode MS"];
    const HINTS: &[&str] = &["CJK", "SC", "TC", "JP", "KR", "Hei", "Song", "Kai", "Ming", "Gothic", "Mincho", "Hiragino", "PingFang", "YaHei", "Han", "Unicode"];
    let hints = if is_cjk(c) { HINTS } else { script_hints(c) };
    if hints.is_empty() { return None; }
    let cache = CJK_CACHE.get_or_init(Default::default);
    if let Some(r) = cache.lock().unwrap().get(&(c, bold)) { return *r; }
    let candidates: Vec<usize> = (faces().len() - SYSTEM.get().map_or(0, |s| s.len())..face_slots())
        .filter(|&i| i < faces().len() || EMBEDDED.get(i - faces().len()).is_some_and(|s| s.get().is_some()))
        .filter(|&i| { let f = face(i as u16).family; (is_cjk(c) && PREFERRED.contains(&f)) || hints.iter().any(|h| f.contains(h)) })
        .collect();
    let rank = |i: &usize| { let f = face(*i as u16); (PREFERRED.iter().position(|p| *p == f.family).unwrap_or(PREFERRED.len()), f.bold != bold, f.italic) };
    let mut sorted = candidates;
    sorted.sort_by_key(rank);
    let found = sorted.into_iter().find(|&i| has_glyph(i as u16, c)).map(|i| i as u16);
    cache.lock().unwrap().insert((c, bold), found);
    found
}

/// Vertical metrics as fractions of the em.
#[derive(Clone, Copy, Debug)]
pub struct FaceMetrics { pub ascent: f32, pub descent: f32, pub underline_offset: f32, pub underline_thickness: f32, pub strike_offset: f32, pub cap_height: f32 }

/// Every bundled face, parsed once at its instance's variation coordinates.
pub fn ttf(index: u16) -> &'static ttf_parser::Face<'static> {
    // Parsed on first use, one face at a time (system fonts are read only when used).
    static F: OnceLock<Vec<OnceLock<ttf_parser::Face<'static>>>> = OnceLock::new();
    let slots = F.get_or_init(|| (0..face_slots()).map(|_| OnceLock::new()).collect());
    let i = (index as usize).min(slots.len() - 1);
    slots[i].get_or_init(|| {
        let d = face(i as u16);
        let mut face = ttf_parser::Face::parse(d.bytes(), d.index).or_else(|_| ttf_parser::Face::parse(BUNDLED[0].bytes(), 0)).expect("font parses");
        if d.weight > 0.0 { face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), d.weight); }
        face
    })
}

fn shaper(index: u16) -> &'static rustybuzz::Face<'static> {
    static F: OnceLock<Vec<OnceLock<rustybuzz::Face<'static>>>> = OnceLock::new();
    let slots = F.get_or_init(|| (0..face_slots()).map(|_| OnceLock::new()).collect());
    let i = (index as usize).min(slots.len() - 1);
    slots[i].get_or_init(|| {
        let d = face(i as u16);
        let mut face = rustybuzz::Face::from_slice(d.bytes(), d.index).or_else(|| rustybuzz::Face::from_slice(BUNDLED[0].bytes(), 0)).expect("font shapes");
        if d.weight > 0.0 { face.set_variations(&[rustybuzz::Variation { tag: ttf_parser::Tag::from_bytes(b"wght"), value: d.weight }]); }
        face
    })
}

fn face_metrics(i: u16) -> FaceMetrics {
    static M: OnceLock<Vec<OnceLock<FaceMetrics>>> = OnceLock::new();
    let slots = M.get_or_init(|| (0..face_slots()).map(|_| OnceLock::new()).collect());
    *slots[(i as usize).min(slots.len() - 1)].get_or_init(|| {
        let face = ttf(i);
        let upem = face.units_per_em() as f32;
        // Word lays out lines with the Windows metrics (usWinAscent/usWinDescent).
        let (asc, desc) = match face.tables().os2 {
            Some(os2) if os2.windows_ascender() > 0 => (os2.windows_ascender() as f32, os2.windows_descender().unsigned_abs() as f32),
            _ => (face.ascender() as f32, -(face.descender() as f32)),
        };
        let ul = face.underline_metrics();
        let st = face.strikeout_metrics();
        let cap = face.capital_height().filter(|h| *h > 0).map(|h| h as f32 / upem).unwrap_or(0.7);
        FaceMetrics {
            ascent: asc / upem, descent: desc / upem, cap_height: cap,
            underline_offset: ul.map(|m| -(m.position as f32) / upem).unwrap_or(0.1),
            underline_thickness: ul.map(|m| m.thickness as f32 / upem).unwrap_or(0.05).max(0.03),
            strike_offset: st.map(|m| m.position as f32 / upem).unwrap_or(0.3),
        }
    })
}

pub fn metrics(key: FontKey) -> FaceMetrics { face_metrics(key.face) }

pub fn has_glyph(face: u16, c: char) -> bool { ttf(face).glyph_index(c).is_some() }
pub fn glyph_id(face: u16, c: char) -> u16 { ttf(face).glyph_index(c).map(|g| g.0).unwrap_or(0) }

fn advances() -> &'static Mutex<HashMap<(u16, char), f32>> {
    static A: OnceLock<Mutex<HashMap<(u16, char), f32>>> = OnceLock::new();
    A.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Advance width of one character in em units, through the script fallback.
fn advance_em(face: u16, c: char) -> f32 {
    if let Some(a) = advances().lock().unwrap().get(&(face, c)) { return *a; }
    let k = fallback(FontKey { face, synthetic_bold: false }, c);
    let f = ttf(k.face);
    let a = f.glyph_index(c).and_then(|g| f.glyph_hor_advance(g)).map(|a| a as f32 / f.units_per_em() as f32).unwrap_or(0.5);
    advances().lock().unwrap().insert((face, c), a);
    a
}

/// Width in px of `c` at `size_px`, with synthetic bold adding a stroke width.
pub fn advance(key: FontKey, c: char, size_px: f32) -> f32 {
    let w = advance_em(key.face, c) * size_px;
    if key.synthetic_bold && !c.is_whitespace() { w + size_px / 32.0 } else { w }
}

/// One shaped glyph. `cluster` is the byte offset of the first character it renders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shaped { pub gid: u16, pub cluster: usize, pub advance: f32, pub dx: f32, pub dy: f32 }

type ShapeKey = (FontKey, u32, bool, String);

fn shape_cache() -> &'static Mutex<HashMap<ShapeKey, std::sync::Arc<Vec<Shaped>>>> {
    static C: OnceLock<Mutex<HashMap<ShapeKey, std::sync::Arc<Vec<Shaped>>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Shape `text` in one face, through a cache (documents repeat words constantly).
/// Glyphs come back in visual (left-to-right) order.
pub fn shape(key: FontKey, text: &str, rtl: bool, size_px: f32) -> std::sync::Arc<Vec<Shaped>> {
    let k = (key, size_px.to_bits(), rtl, text.to_string());
    if let Some(v) = shape_cache().lock().unwrap().get(&k) { return v.clone(); }
    let v = std::sync::Arc::new(shape_uncached(key, text, rtl, size_px));
    let mut cache = shape_cache().lock().unwrap();
    if cache.len() > 200_000 { cache.clear(); }
    cache.insert(k, v.clone());
    v
}

fn shape_uncached(key: FontKey, text: &str, rtl: bool, size_px: f32) -> Vec<Shaped> {
    let face = shaper(key.face);
    let scale = size_px / face.units_per_em() as f32;
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(text);
    buf.guess_segment_properties();
    buf.set_direction(if rtl { rustybuzz::Direction::RightToLeft } else { rustybuzz::Direction::LeftToRight });
    // Word applies no kerning or Latin ligatures unless a document asks for them; Arabic and
    // Hebrew keep their required features.
    let off = |tag: &[u8; 4]| rustybuzz::Feature::new(ttf_parser::Tag::from_bytes(tag), 0, ..);
    let features = if text.chars().any(is_complex) { vec![] } else { vec![off(b"kern"), off(b"liga"), off(b"clig")] };
    let out = rustybuzz::shape(face, &features, buf);
    let bold = if key.synthetic_bold { size_px / 32.0 } else { 0.0 };
    out.glyph_infos().iter().zip(out.glyph_positions()).map(|(i, p)| Shaped {
        gid: i.glyph_id as u16, cluster: i.cluster as usize,
        advance: p.x_advance as f32 * scale + if p.x_advance != 0 { bold } else { 0.0 },
        dx: p.x_offset as f32 * scale, dy: p.y_offset as f32 * scale,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cjk_falls_back_to_an_installed_cjk_font() {
        // Chinese typed in Noto Sans drew as tofu (no bundled CJK face).
        let Some(p) = ["/System/Library/Fonts/Hiragino Sans GB.ttc", "/System/Library/Fonts/STHeiti Medium.ttc", "C:\\Windows\\Fonts\\msyh.ttc", "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"]
            .iter().map(std::path::Path::new).find(|p| p.exists()) else { return };
        assert!(!register_font_file(p).is_empty());
        let key = resolve("Noto Sans", false, false);
        for c in ['中', '删', '拒', 'あ', '。'] {
            let k = fallback(key, c);
            assert!(has_glyph(k.face, c), "{c} has no glyph in {}", face(k.face).family);
        }
        assert_eq!(fallback(key, 'a').face, key.face);
    }
    #[test]
    fn myanmar_and_palatino_use_installed_fonts() {
        // Burmese fell back to tofu; it now comes from an installed Myanmar font.
        if let Some(p) = ["/System/Library/Fonts/Supplemental/Myanmar Sangam MN.ttc", "/System/Library/Fonts/NotoSansMyanmar.ttc", "/usr/share/fonts/truetype/noto/NotoSansMyanmar-Regular.ttf", "C:\\Windows\\Fonts\\mmrtext.ttf"]
            .iter().map(std::path::Path::new).find(|p| p.exists()) {
            assert!(!register_font_file(p).is_empty());
            let key = resolve("Calibri", false, false);
            for c in "မြန်မာစာ".chars() {
                let k = fallback(key, c);
                assert!(has_glyph(k.face, c), "{c:?} has no glyph in {}", face(k.face).family);
            }
            assert!(!shape(fallback(key, 'မ'), "မြန်မာ", false, 16.0).is_empty());
        }
        // Characters of scripts without hints never open installed fonts.
        assert!(script_hints('\u{17000}').is_empty());
        // Palatino Linotype resolves to Palatino, not the generic serif.
        if let Some(p) = ["/System/Library/Fonts/Palatino.ttc", "/usr/share/fonts/opentype/urw-base35/P052-Roman.otf"].iter().map(std::path::Path::new).find(|p| p.exists()) {
            let fams = register_font_file(p);
            assert!(!fams.is_empty());
            let k = resolve("Palatino Linotype", false, false);
            assert!(fams.iter().any(|f| f == face(k.face).family), "got {}", face(k.face).family);
            assert_eq!(face(resolve("Book Antiqua", false, false).face).family, face(k.face).family);
        }
    }
    #[test]
    fn user_font_files_register_by_family() {
        let dir = std::env::temp_dir().join(format!("qalam-user-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = crate::resources::dir().expect("resources").join("fonts");
        // A static face, then a variable one (regular and bold instances).
        let p = dir.join("Tajawal-Bold.ttf");
        std::fs::copy(src.join("Tajawal-Bold.ttf"), &p).unwrap();
        assert_eq!(register_font_file(&p), vec!["Tajawal".to_string()]);
        let k = resolve("Tajawal", true, false);
        assert_eq!(face(k.face).file, USER_FILE);
        assert!(!k.synthetic_bold);
        assert!(user_families().contains(&"Tajawal".to_string()));
        let v = dir.join("Kufi.ttf");
        std::fs::copy(src.join("NotoKufiArabic.ttf"), &v).unwrap();
        assert_eq!(register_font_file(&v), vec!["Noto Kufi Arabic".to_string()]);
        let (r, b) = (resolve("Noto Kufi Arabic", false, false), resolve("Noto Kufi Arabic", true, false));
        assert_eq!((face(r.face).weight, face(b.face).weight), (400.0, 700.0));
        assert!(has_glyph(r.face, '\u{0628}'));
        // Registering the same file again adds nothing; a non-font adds nothing.
        let used = *EMBEDDED_USED.lock().unwrap();
        register_font_file(&v);
        assert_eq!(*EMBEDDED_USED.lock().unwrap(), used);
        std::fs::write(dir.join("x.ttf"), b"nope").unwrap();
        assert!(register_font_file(&dir.join("x.ttf")).is_empty());
        assert!(is_available("Calibri") && is_available("Amiri") && !is_available("Some Missing Font"));
    }
    #[test]
    fn substitutes_and_metrics() {
        let k = resolve("Calibri", true, false);
        assert_eq!(face(k.face as u16).file, "Carlito-Bold.ttf");
        let n = resolve("Noto Sans", true, false);
        assert!(!n.synthetic_bold && face(n.face as u16).weight == 700.0);
        assert!(advance(n, 'W', 16.0) > advance(resolve("Noto Sans", false, false), 'W', 16.0), "variable bold is wider");
        let m = metrics(resolve("Arial", false, false));
        assert!(m.ascent > 0.8 && m.descent > 0.1);
        assert!(advance(resolve("Courier New", false, false), 'i', 16.0) == advance(resolve("Courier New", false, false), 'W', 16.0));
    }

    #[test]
    fn arabic_joins_and_falls_back() {
        let latin = resolve("Times New Roman", false, false);
        let k = fallback(latin, 'ب');
        assert_eq!(face(k.face as u16).family, "Noto Naskh Arabic", "serif runs fall back to Naskh");
        assert_eq!(faces()[fallback(resolve("Arial", false, false), 'ب').face as usize].family, "Noto Sans Arabic");
        let hebrew = fallback(resolve("Calibri", false, false), 'ש');
        assert!(has_glyph(hebrew.face, 'ש'), "Hebrew resolves to a face with the glyph ({})", face(hebrew.face as u16).family);
        assert_eq!(faces()[fallback(latin, '😀').face as usize].family, "Noto Emoji");
        // Joining: the initial form of ب in "بب" differs from the isolated glyph.
        // (Naskh draws dots as separate zero-advance mark glyphs; compare the base letters.)
        let base = |v: std::sync::Arc<Vec<Shaped>>| v.iter().copied().filter(|g| g.advance > 0.0).collect::<Vec<_>>();
        let isolated = base(shape(k, "ب", true, 16.0))[0].gid;
        let joined = base(shape(k, "بب", true, 16.0));
        assert_eq!(joined.len(), 2);
        assert!(joined.iter().all(|g| g.gid != isolated), "contextual forms replace the isolated one");
        assert!(joined[0].cluster > joined[joined.len() - 1].cluster, "RTL glyphs come back in visual order");
    }
}

/// A face as a standalone font file for embedding (a face inside a .ttc collection is
/// extracted), and whether its outlines are CFF (PostScript) rather than TrueType.
pub fn embeddable(face: u16) -> (std::borrow::Cow<'static, [u8]>, bool) {
    let d = self::face(face);
    let bytes = d.bytes();
    let be32 = |b: &[u8], o: usize| -> Option<u32> { b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]])) };
    let be16 = |b: &[u8], o: usize| -> Option<u16> { b.get(o..o + 2).map(|x| u16::from_be_bytes([x[0], x[1]])) };
    let cff = |b: &[u8], start: usize| -> bool { let n = be16(b, start + 4).unwrap_or(0) as usize; (0..n).any(|k| b.get(start + 12 + k * 16..start + 16 + k * 16) == Some(b"CFF ")) };
    if bytes.get(0..4) != Some(b"ttcf") { return (std::borrow::Cow::Borrowed(bytes), cff(bytes, 0)); }
    // Rebuild one face from the collection: its table directory with tables copied after it.
    let build = || -> Option<Vec<u8>> {
        let start = be32(bytes, 12 + 4 * d.index as usize)? as usize;
        let n = be16(bytes, start + 4)? as usize;
        let mut out = bytes.get(start..start + 12)?.to_vec();
        let mut data = vec![];
        let head = 12 + 16 * n;
        for k in 0..n {
            let rec = start + 12 + 16 * k;
            let (off, len) = (be32(bytes, rec + 8)? as usize, be32(bytes, rec + 12)? as usize);
            out.extend_from_slice(bytes.get(rec..rec + 8)?);
            out.extend_from_slice(&((head + data.len()) as u32).to_be_bytes());
            out.extend_from_slice(&(len as u32).to_be_bytes());
            data.extend_from_slice(bytes.get(off..off + len)?);
            while data.len() % 4 != 0 { data.push(0); }
        }
        out.extend(data);
        Some(out)
    };
    match build() { Some(v) => { let c = cff(&v, 0); (std::borrow::Cow::Owned(v), c) } None => (std::borrow::Cow::Borrowed(bytes), false) }
}
