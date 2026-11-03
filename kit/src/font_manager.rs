//! Adding fonts: Import Font…, Download from Google Fonts, and the prompt for fonts a document
//! uses that aren't on this computer. Added fonts are copied into `fonts::user_fonts_dir()`, so
//! they're there again next time (`fonts::load_user_fonts`), and registered with the engine at
//! once, so layout, painting and PDF export use them.
//!
//! The apps call `show` every frame (it draws the dialogs and says when fonts were added, so the
//! document can be laid out again), and open the dialogs from their menus.

use crate::fonts::{is_font_file, user_fonts_dir};
use crate::i18n::{tr, trf};
use egui::{RichText, Ui};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// A family in the Google Fonts catalog.
#[derive(Clone, Debug)]
pub struct CatalogFamily { pub name: String, pub category: String, pub arabic: bool, pub popularity: i64 }

#[derive(Default)]
enum Catalog { #[default] Idle, Loading, Ready(Vec<CatalogFamily>), Failed(String) }

#[derive(Clone, PartialEq)]
enum Download { Running, Done, Failed(String) }

#[derive(Default)]
struct State {
    google_open: bool,
    query: String,
    arabic_only: bool,
    catalog: Catalog,
    downloads: HashMap<String, Download>,
    /// Fonts the open document uses that aren't available, while the prompt shows.
    missing: Vec<String>,
    missing_open: bool,
    dont_ask: bool,
    notice: Option<String>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
/// Set when fonts were added since the last `show`.
static CHANGED: AtomicBool = AtomicBool::new(false);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(State::default))
}

const CATALOG_URL: &str = "https://fonts.google.com/metadata/fonts";
const USER_AGENT: &str = concat!("Woffice/", env!("CARGO_PKG_VERSION"));

fn get(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(90))).build().into();
    agent.get(url).header("User-Agent", USER_AGENT).call().map_err(|e| e.to_string())?.body_mut().with_config().limit(64 << 20).read_to_vec().map_err(|e| e.to_string())
}

/// Google's JSON answers start with `)]}'`; the JSON is from the first brace on.
fn json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let t = String::from_utf8_lossy(bytes);
    let start = t.find('{').ok_or_else(|| "unexpected answer".to_string())?;
    serde_json::from_str(&t[start..]).map_err(|e| e.to_string())
}

/// Parse the catalog (fonts.google.com/metadata/fonts), most popular first.
pub fn parse_catalog(bytes: &[u8]) -> Result<Vec<CatalogFamily>, String> {
    let v = json(bytes)?;
    let list = v["familyMetadataList"].as_array().ok_or_else(|| "no font list".to_string())?;
    let mut out: Vec<CatalogFamily> = list.iter().filter_map(|f| {
        let name = f["family"].as_str()?.to_string();
        let subsets = f["subsets"].as_array().map(|s| s.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>()).unwrap_or_default();
        Some(CatalogFamily { name, category: f["category"].as_str().unwrap_or("").to_string(), arabic: subsets.contains(&"arabic") || f["primaryScript"].as_str() == Some("Arab"), popularity: f["popularity"].as_i64().unwrap_or(i64::MAX) })
    }).collect();
    out.sort_by_key(|f| f.popularity);
    Ok(out)
}

fn url_encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A file or folder name made from a family name.
fn safe_name(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).collect();
    let t = t.trim().trim_matches('.').to_string();
    if t.is_empty() { "font".into() } else { t }
}

/// Which of a family's files to download (paths as in its download list): the variable fonts
/// when there are any (every weight in one file), else the regular, bold and italic faces.
pub fn choose_files(files: &[String]) -> Vec<String> {
    let fonts: Vec<&String> = files.iter().filter(|f| is_font_file(std::path::Path::new(f.as_str()))).collect();
    let variable: Vec<String> = fonts.iter().filter(|f| f.contains('[')).map(|f| f.to_string()).collect();
    if !variable.is_empty() { return variable; }
    let style = |f: &str| std::path::Path::new(f).file_stem().and_then(|s| s.to_str()).and_then(|s| s.rsplit('-').next()).unwrap_or("").to_string();
    let main: Vec<String> = fonts.iter().filter(|f| matches!(style(f).as_str(), "Regular" | "Bold" | "Italic" | "BoldItalic")).map(|f| f.to_string()).collect();
    if main.iter().any(|f| style(f) == "Regular") { main } else { fonts.into_iter().cloned().collect() }
}

/// Download a Google Fonts family into the user fonts folder and register it. Returns the
/// families registered.
pub fn download_family(family: &str) -> Result<Vec<String>, String> {
    let v = json(&get(&format!("https://fonts.google.com/download/list?family={}", url_encode(family)))?)?;
    let m = &v["manifest"];
    let refs: Vec<(String, String)> = m["fileRefs"].as_array().map(|a| a.iter().filter_map(|r| Some((r["filename"].as_str()?.to_string(), r["url"].as_str()?.to_string()))).collect()).unwrap_or_default();
    let chosen = choose_files(&refs.iter().map(|r| r.0.clone()).collect::<Vec<_>>());
    if chosen.is_empty() { return Err(tr("Not found on Google Fonts.")); }
    let dir = user_fonts_dir().join(safe_name(family));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // The license comes along (OFL fonts ask for it to travel with them).
    for f in m["files"].as_array().into_iter().flatten() {
        if let (Some(name), Some(text)) = (f["filename"].as_str(), f["contents"].as_str()) {
            if name.ends_with(".txt") { let _ = std::fs::write(dir.join(safe_name(name)), text); }
        }
    }
    let mut families = vec![];
    for (name, url) in refs.iter().filter(|r| chosen.contains(&r.0)) {
        let bytes = get(url)?;
        let file = std::path::Path::new(name).file_name().map(|n| safe_name(&n.to_string_lossy())).unwrap_or_else(|| "font.ttf".into());
        let path = dir.join(file);
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        let got = word_engine::fonts::register_font_file(&path);
        if got.is_empty() { let _ = std::fs::remove_file(&path); }
        for g in got { if !families.contains(&g) { families.push(g); } }
    }
    if families.is_empty() { return Err(tr("The downloaded files aren't fonts.")); }
    Ok(families)
}

/// Copy font files into the user fonts folder and register them. Returns the families added
/// and the files that weren't fonts.
pub fn import_files(paths: &[std::path::PathBuf]) -> (Vec<String>, Vec<String>) {
    let dir = user_fonts_dir().join("Imported");
    let _ = std::fs::create_dir_all(&dir);
    let (mut families, mut failed) = (vec![], vec![]);
    for p in paths {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let Ok(bytes) = std::fs::read(p) else { failed.push(name); continue };
        // Same name, same bytes: already imported. Same name, other bytes: a numbered copy.
        let (stem, ext) = (p.file_stem().map(|s| safe_name(&s.to_string_lossy())).unwrap_or_else(|| "font".into()), p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_else(|| "ttf".into()));
        let mut dest = dir.join(format!("{stem}.{ext}"));
        let mut k = 2;
        while dest.exists() && std::fs::read(&dest).ok().as_deref() != Some(bytes.as_slice()) { dest = dir.join(format!("{stem}-{k}.{ext}")); k += 1; }
        if !dest.exists() && std::fs::write(&dest, &bytes).is_err() { failed.push(name); continue; }
        let got = word_engine::fonts::register_font_file(&dest);
        if got.is_empty() { let _ = std::fs::remove_file(&dest); failed.push(name); continue; }
        for g in got { if !families.contains(&g) { families.push(g); } }
    }
    if !families.is_empty() { CHANGED.store(true, Ordering::Release); }
    (families, failed)
}

/// Import Font…: pick font files, then add them.
pub fn import_font() {
    let Some(paths) = rfd::FileDialog::new().set_title(tr("Import Font")).add_filter(tr("Fonts"), &["ttf", "otf", "ttc", "otc", "TTF", "OTF", "TTC"]).pick_files() else { return };
    let (families, failed) = import_files(&paths);
    let mut msg = if families.is_empty() { String::new() } else { trf("Added {}.", &[&families.join(", ")]) };
    if !failed.is_empty() { if !msg.is_empty() { msg.push_str("\n\n"); } msg.push_str(&trf("These files aren't fonts that can be used: {}", &[&failed.join(", ")])); }
    if !msg.is_empty() { with(|s| s.notice = Some(msg)); }
}

/// Start loading the catalog, unless it's loaded or loading.
fn fetch_catalog(ctx: &egui::Context) {
    let start = with(|s| { let go = matches!(s.catalog, Catalog::Idle | Catalog::Failed(_)); if go { s.catalog = Catalog::Loading; } go });
    if !start { return; }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let r = get(CATALOG_URL).and_then(|b| parse_catalog(&b));
        with(|s| s.catalog = match r { Ok(v) => Catalog::Ready(v), Err(e) => Catalog::Failed(e) });
        ctx.request_repaint();
    });
}

/// Download a family in the background.
fn start_download(ctx: &egui::Context, family: &str) {
    let fam = family.to_string();
    let go = with(|s| { let busy = s.downloads.get(&fam) == Some(&Download::Running); if !busy { s.downloads.insert(fam.clone(), Download::Running); } !busy });
    if !go { return; }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let r = download_family(&fam);
        if r.is_ok() { CHANGED.store(true, Ordering::Release); }
        with(|s| { s.downloads.insert(fam, match r { Ok(_) => Download::Done, Err(e) => Download::Failed(e) }); });
        ctx.request_repaint();
    });
}

/// Download from Google Fonts…: search the catalog and download families.
pub fn open_google_fonts(ctx: &egui::Context) {
    with(|s| s.google_open = true);
    fetch_catalog(ctx);
}

fn no_prompt_marker() -> std::path::PathBuf { user_fonts_dir().join(".no-missing-fonts-prompt") }

/// After opening a document: if it uses fonts that aren't available, say so and offer to
/// download them from Google Fonts. Names starting with `+` (theme fonts) are ignored.
pub fn check_document_fonts(ctx: &egui::Context, names: impl IntoIterator<Item = String>) {
    if no_prompt_marker().exists() { return; }
    let mut missing: Vec<String> = vec![];
    for n in names {
        let n = n.trim().to_string();
        if n.is_empty() || n.starts_with('+') || missing.iter().any(|m| m.eq_ignore_ascii_case(&n)) || word_engine::fonts::is_available(&n) { continue; }
        missing.push(n);
    }
    if missing.is_empty() { return; }
    with(|s| { s.missing = missing; s.missing_open = true; });
    fetch_catalog(ctx);
}

/// The two font commands, for a font menu. Returns true when one was chosen.
pub fn menu_items(ui: &mut Ui) -> bool {
    let mut chosen = false;
    if ui.button(tr("Import Font…")).clicked() { import_font(); chosen = true; }
    if ui.button(tr("Download from Google Fonts…")).clicked() { open_google_fonts(ui.ctx()); chosen = true; }
    chosen
}

fn status_row(ui: &mut Ui, ctx: &egui::Context, name: &str, state: Option<&Download>, available: bool) {
    match state {
        Some(Download::Running) => { ui.spinner(); ui.label(RichText::new(tr("Downloading…")).weak()); }
        Some(Download::Done) => { ui.label(RichText::new(tr("Installed")).weak()); }
        Some(Download::Failed(e)) => {
            if ui.button(tr("Retry")).clicked() { start_download(ctx, name); }
            ui.label(RichText::new(tr("Failed")).color(egui::Color32::from_rgb(0xC0, 0x30, 0x30))).on_hover_text(e.as_str());
        }
        None if available => { ui.label(RichText::new(tr("Installed")).weak()); }
        None => { if ui.button(tr("Download")).clicked() { start_download(ctx, name); } }
    }
}

/// Draw the font dialogs. Returns true when fonts were added since the last call (lay the
/// document out again).
pub fn show(ctx: &egui::Context) -> bool {
    google_dialog(ctx);
    missing_dialog(ctx);
    notice_dialog(ctx);
    CHANGED.swap(false, Ordering::AcqRel)
}

fn google_dialog(ctx: &egui::Context) {
    let mut open = with(|s| s.google_open);
    if !open { return; }
    crate::theme::dialog(ctx, "Download from Google Fonts").open(&mut open).default_width(480.0).show(ctx, |ui| {
        let (mut query, mut arabic) = with(|s| (s.query.clone(), s.arabic_only));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut query).hint_text(tr("Search fonts")).desired_width(280.0));
            ui.checkbox(&mut arabic, tr("Arabic"));
        });
        with(|s| { s.query = query.clone(); s.arabic_only = arabic; });
        ui.add_space(6.0);
        let (list, failed, loading) = with(|s| match &s.catalog {
            Catalog::Ready(v) => {
                let q = query.trim().to_lowercase();
                (v.iter().filter(|f| (!arabic || f.arabic) && (q.is_empty() || f.name.to_lowercase().contains(&q))).cloned().collect::<Vec<_>>(), None, false)
            }
            Catalog::Failed(e) => (vec![], Some(e.clone()), false),
            _ => (vec![], None, true),
        });
        if loading { ui.horizontal(|ui| { ui.spinner(); ui.label(tr("Loading the Google Fonts catalog…")); }); }
        if let Some(e) = failed {
            ui.label(trf("Couldn't load the Google Fonts catalog. {}", &[&e]));
            if ui.button(tr("Retry")).clicked() { fetch_catalog(ui.ctx()); }
        }
        if !loading && list.is_empty() && with(|s| matches!(s.catalog, Catalog::Ready(_))) { ui.label(RichText::new(tr("No fonts match.")).weak()); }
        let row_h = 30.0;
        egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, true]).show_rows(ui, row_h, list.len(), |ui, range| {
            for f in &list[range] {
                let st = with(|s| s.downloads.get(&f.name).cloned());
                let available = word_engine::fonts::is_available(&f.name);
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_h), crate::widgets::row_layout(), |ui| {
                    ui.label(RichText::new(&f.name).size(14.0));
                    ui.label(RichText::new(tr(&f.category)).weak().size(11.5));
                    ui.with_layout(crate::widgets::far_layout(), |ui| status_row(ui, ctx, &f.name, st.as_ref(), available));
                });
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(tr("Fonts from Google Fonts are free to use (most under the SIL Open Font License). Downloaded fonts are kept for next time.")).weak().size(11.0));
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "Close").clicked() { with(|s| s.google_open = false); } });
    });
    if !open { with(|s| s.google_open = false); }
}

fn missing_dialog(ctx: &egui::Context) {
    let mut open = with(|s| s.missing_open);
    if !open { return; }
    crate::theme::dialog(ctx, "Missing Fonts").open(&mut open).default_width(460.0).show(ctx, |ui| {
        ui.label(tr("This document uses fonts that aren't on this computer. Similar fonts are shown in their place."));
        ui.add_space(8.0);
        let missing = with(|s| s.missing.clone());
        let catalog: Option<Vec<String>> = with(|s| match &s.catalog { Catalog::Ready(v) => Some(v.iter().map(|f| f.name.clone()).collect()), Catalog::Failed(_) => Some(vec![]), _ => None });
        let mut on_google = vec![];
        for name in &missing {
            let sub = word_engine::fonts::face(word_engine::fonts::resolve(name, false, false).face).family;
            let google = catalog.as_ref().and_then(|c| c.iter().find(|f| f.eq_ignore_ascii_case(name)).cloned());
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 28.0), crate::widgets::row_layout(), |ui| {
                ui.label(RichText::new(name).strong());
                ui.label(RichText::new(trf("shown as {}", &[&sub])).weak().size(11.5));
                ui.with_layout(crate::widgets::far_layout(), |ui| match (&catalog, &google) {
                    (None, _) => { ui.spinner(); }
                    (Some(_), None) => { ui.label(RichText::new(tr("Not on Google Fonts")).weak().size(11.5)); }
                    (Some(_), Some(g)) => { let st = with(|s| s.downloads.get(g).cloned()); status_row(ui, ctx, g, st.as_ref(), false); }
                });
            });
            if let Some(g) = google { if with(|s| !s.downloads.contains_key(&g)) { on_google.push(g); } }
        }
        ui.add_space(6.0);
        let mut dont = with(|s| s.dont_ask);
        if ui.checkbox(&mut dont, tr("Don't check documents for missing fonts")).changed() {
            with(|s| s.dont_ask = dont);
            let m = no_prompt_marker();
            if dont { let _ = std::fs::create_dir_all(user_fonts_dir()); let _ = std::fs::write(&m, ""); } else { let _ = std::fs::remove_file(&m); }
        }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, !on_google.is_empty(), "Download All").clicked() { for g in &on_google { start_download(ui.ctx(), g); } }
            if crate::theme::secondary(ui, "Close").clicked() { with(|s| s.missing_open = false); }
        });
    });
    if !open { with(|s| s.missing_open = false); }
}

fn notice_dialog(ctx: &egui::Context) {
    let Some(msg) = with(|s| s.notice.clone()) else { return };
    let mut open = true;
    crate::theme::dialog(ctx, "Fonts").open(&mut open).show(ctx, |ui| {
        ui.label(msg.as_str());
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { with(|s| s.notice = None); } });
    });
    if !open { with(|s| s.notice = None); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_variable_or_main_styles() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(choose_files(&v(&["OFL.txt", "Cairo[slnt,wght].ttf", "static/Cairo-Regular.ttf", "static/Cairo-Bold.ttf"])), v(&["Cairo[slnt,wght].ttf"]));
        assert_eq!(choose_files(&v(&["Tajawal-Light.ttf", "Tajawal-Regular.ttf", "Tajawal-Bold.ttf", "Tajawal-Black.ttf"])), v(&["Tajawal-Regular.ttf", "Tajawal-Bold.ttf"]));
        assert_eq!(choose_files(&v(&["Foo-Light.ttf"])), v(&["Foo-Light.ttf"]));
        assert_eq!(url_encode("Noto Kufi Arabic"), "Noto%20Kufi%20Arabic");
    }

    /// Talks to Google Fonts: `cargo test -- --ignored live`.
    #[test]
    #[ignore]
    fn live_catalog_and_download() {
        let dir = std::env::temp_dir().join(format!("woffice-fonts-live-{}", std::process::id()));
        unsafe { std::env::set_var("WOFFICE_FONTS_DIR", &dir); }
        let c = parse_catalog(&get(CATALOG_URL).unwrap()).unwrap();
        assert!(c.len() > 1000 && c.iter().any(|f| f.name == "Lateef" && f.arabic));
        assert_eq!(download_family("Lateef").unwrap(), vec!["Lateef".to_string()]);
        assert!(word_engine::fonts::is_available("Lateef"));
        assert!(download_family("No Such Family Here").is_err());
    }

    #[test]
    fn parses_catalog() {
        let t = br#")]}'
{"familyMetadataList":[{"family":"B","category":"Serif","subsets":["latin"],"popularity":9},{"family":"Amiri","category":"Serif","subsets":["arabic","latin"],"popularity":3}]}"#;
        let c = parse_catalog(t).unwrap();
        assert_eq!(c[0].name, "Amiri");
        assert!(c[0].arabic && !c[1].arabic);
    }

    #[test]
    fn imports_into_user_folder() {
        let dir = std::env::temp_dir().join(format!("woffice-fonts-test-{}", std::process::id()));
        // SAFETY: tests in this module don't read the variable concurrently with other writers.
        unsafe { std::env::set_var("WOFFICE_FONTS_DIR", &dir); }
        let src = word_engine::resources::dir().expect("resources").join("fonts").join("Tajawal-Regular.ttf");
        let bad = dir.join("bad.ttf");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&bad, b"not a font").unwrap();
        let (fams, failed) = import_files(&[src.clone(), bad]);
        assert_eq!(fams, vec!["Tajawal".to_string()]);
        assert_eq!(failed, vec!["bad.ttf".to_string()]);
        assert!(dir.join("Imported/Tajawal-Regular.ttf").exists());
        // Importing it again reuses the copy.
        import_files(&[src]);
        assert!(!dir.join("Imported/Tajawal-Regular-2.ttf").exists());
        assert!(crate::fonts::load_user_fonts() >= 1);
    }
}
