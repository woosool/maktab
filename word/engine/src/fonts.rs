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

/// Room for fonts that documents bring with them (registered while the app runs).
const EMBEDDED_SLOTS: usize = 96;
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
pub fn embedded_families() -> Vec<String> {
    let used = *EMBEDDED_USED.lock().unwrap_or_else(|e| e.into_inner());
    let mut v: Vec<String> = EMBEDDED[..used].iter().filter_map(|s| s.get()).map(|f| f.family.to_string()).collect();
    v.dedup();
    v
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
                    f.glyph_index('a').or(f.glyph_index('\u{0627}'))?;
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
    let mut sys: Vec<String> = SYSTEM.get().map(|v| v.iter().map(|f| f.family.to_string()).collect()).unwrap_or_default();
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
    ("Noto Naskh Arabic", "Noto Naskh Arabic"), ("Noto Sans Arabic", "Noto Sans Arabic"), ("Noto Sans Hebrew", "Noto Sans Hebrew"),
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
        "traditional arabic" | "simplified arabic" | "arabic typesetting" | "sakkal majalla" | "amiri" | "scheherazade" | "noto naskh arabic ui" | "al bayan" | "geeza pro" | "baghdad" | "nadeem" => "Noto Naskh Arabic",
        "dubai" | "segoe ui arabic" | "simplified arabic fixed" | "andalus" | "aldhabi" | "noto kufi arabic" | "cairo" | "tajawal" => "Noto Sans Arabic",
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
    match system_family(name).filter(|_| !exact_bundled) { Some(f) => resolve_family(f, bold, italic), None => resolve_family(bundled_family(name), bold, italic) }
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
    let serif = matches!(face(key.face).family, "Liberation Serif" | "Caladea" | "Noto Naskh Arabic");
    let chain: &[&str] = if is_complex(c) && !is_hebrew(c) {
        if serif { &["Noto Naskh Arabic", "Noto Sans Arabic"] } else { &["Noto Sans Arabic", "Noto Naskh Arabic"] }
    } else if is_hebrew(c) { &["Noto Sans Hebrew"] } else if is_emoji(c) { &["Noto Emoji", "Noto Sans"] } else { &["Noto Sans", "Liberation Sans", "Noto Sans Math", "Noto Sans Symbols", "Noto Sans Symbols 2", "Noto Emoji"] };
    for family in chain {
        let k = resolve_family(family, bold, false);
        if has_glyph(k.face, c) { return k; }
    }
    key
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
