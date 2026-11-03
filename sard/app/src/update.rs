//! Checking for, downloading and installing new versions.
//!
//! A JSON manifest (SARD_UPDATE_URL, or the default on woosool.com) lists the latest version
//! and one archive per platform:
//!
//! ```json
//! { "version": "0.2.0", "notes": "What's new…", "page": "https://woosool.com/en/products/sard",
//!   "downloads": { "macos-aarch64": { "url": "https://…/Sard-0.2.0-macos-aarch64.zip", "sha256": "…" } } }
//! ```
//!
//! The archive is a zip of Sard.app (macOS) or of the program folder (Windows, Linux). It is
//! checked against its SHA-256, unpacked next to the running copy and swapped in; the old
//! copy is kept until the next start.

use crate::app::SlidesApp;
use egui::RichText;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_URL: &str = "https://woosool.com/sard/update.json";
const DAY: u64 = 24 * 60 * 60;

#[derive(Clone, Debug)]
pub struct Release { pub version: String, pub notes: String, pub page: String, pub url: Option<String>, pub sha256: Option<String> }

#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading(Release),
    /// Installed; takes effect after a restart (the path is what to launch).
    Ready(Release, PathBuf),
    Failed(String),
}

static STATE: Mutex<State> = Mutex::new(State::Idle);
static SHOW: AtomicBool = AtomicBool::new(false);
static DONE: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(0);

fn set(s: State) { if let Ok(mut st) = STATE.lock() { *st = s; } }
fn state() -> State { STATE.lock().map(|s| s.clone()).unwrap_or_default() }
fn now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

fn manifest_url() -> String { std::env::var("SARD_UPDATE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string()) }
pub fn platform() -> String { format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH) }

fn agent(timeout: u64) -> ureq::Agent { ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(timeout))).build().into() }

/// "1.10.2" > "1.9" (missing parts count as 0; anything after '-' or '+' is ignored).
pub fn newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (parts(candidate), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y { return x > y; }
    }
    false
}

pub fn parse_manifest(v: &serde_json::Value) -> Result<Release, String> {
    let version = v["version"].as_str().ok_or("The update information has no version.")?.to_string();
    let d = &v["downloads"][platform()];
    Ok(Release {
        version,
        notes: v["notes"].as_str().unwrap_or("").to_string(),
        page: v["page"].as_str().unwrap_or("https://woosool.com/en/products/sard").to_string(),
        url: d["url"].as_str().map(String::from),
        sha256: d["sha256"].as_str().map(|s| s.to_lowercase()),
    })
}

fn fetch() -> Result<Release, String> {
    let text = agent(15).get(&manifest_url()).header("User-Agent", &format!("Sard/{CURRENT}")).call().map_err(|e| e.to_string())?
        .body_mut().read_to_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    parse_manifest(&v)
}

/// Check in the background. `manual` shows the dialog with the result whatever it is;
/// otherwise it appears only when a version the user hasn't skipped is out.
pub fn check(ctx: &egui::Context, manual: bool, skipped: Option<String>) {
    if matches!(state(), State::Checking | State::Downloading(_)) { if manual { SHOW.store(true, Ordering::Relaxed); } return; }
    set(State::Checking);
    if manual { SHOW.store(true, Ordering::Relaxed); }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let s = match fetch() {
            Ok(r) if newer(&r.version, CURRENT) => {
                if !manual && skipped.as_deref() == Some(r.version.as_str()) { State::Idle } else { SHOW.store(true, Ordering::Relaxed); State::Available(r) }
            }
            Ok(_) => State::UpToDate,
            Err(e) => if manual { State::Failed(e) } else { State::Idle },
        };
        set(s);
        ctx.request_repaint();
    });
}

pub fn open_dialog() { SHOW.store(true, Ordering::Relaxed); }

/// Where the running copy lives: the .app bundle on macOS, the program folder elsewhere.
fn install_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    if cfg!(target_os = "macos") {
        exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
    } else {
        exe.parent().map(Path::to_path_buf)
    }
}

/// Can this copy replace itself? (Not when installed by a package manager into a system folder.)
fn self_updatable() -> bool {
    let Some(root) = install_root() else { return false };
    let Some(dir) = root.parent() else { return false };
    let probe = dir.join(format!(".sard-update-probe-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok && !root.to_string_lossy().contains("/.mount_")
}

fn download(r: &Release) -> Result<PathBuf, String> {
    let url = r.url.as_deref().ok_or("No download for this computer.")?;
    let mut resp = agent(600).get(url).header("User-Agent", &format!("Sard/{CURRENT}")).call().map_err(|e| e.to_string())?;
    TOTAL.store(resp.headers().get("content-length").and_then(|v| v.to_str().ok()?.parse().ok()).unwrap_or(0), Ordering::Relaxed);
    DONE.store(0, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("sard-update-{}", r.version));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("update.zip");
    let mut out = std::fs::File::create(&file).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut reader = resp.body_mut().with_config().limit(1 << 30).reader();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        hash.update(&buf[..n]);
        std::io::Write::write_all(&mut out, &buf[..n]).map_err(|e| e.to_string())?;
        DONE.fetch_add(n as u64, Ordering::Relaxed);
    }
    let got: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    match &r.sha256 {
        Some(want) if *want != got => Err(crate::i18n::tr("The download is damaged (its checksum doesn't match).").to_string()),
        _ => Ok(file),
    }
}

fn run(cmd: &mut std::process::Command) -> Result<(), String> {
    let o = cmd.output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) }
}

fn unzip(zip: &Path, to: &Path) -> Result<(), String> {
    if cfg!(target_os = "macos") { run(std::process::Command::new("ditto").args(["-x", "-k"]).arg(zip).arg(to)) }
    else if cfg!(windows) { run(std::process::Command::new("powershell").args(["-NoProfile", "-Command", "Expand-Archive", "-Force", "-LiteralPath"]).arg(zip).arg("-DestinationPath").arg(to)) }
    else { run(std::process::Command::new("unzip").args(["-o", "-q"]).arg(zip).arg("-d").arg(to)) }
}

/// The unpacked copy: a .app bundle (macOS) or the folder holding the program.
fn find_new(dir: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "sard.exe" } else { "sard" };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).ok()?.flatten() {
            let p = e.path();
            if cfg!(target_os = "macos") && p.extension().is_some_and(|x| x == "app") { return Some(p); }
            if !cfg!(target_os = "macos") && p.file_name().is_some_and(|n| n == exe) { return p.parent().map(Path::to_path_buf); }
            if p.is_dir() { stack.push(p); }
        }
    }
    None
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() { copy_dir(&src, &dst)?; } else { std::fs::copy(&src, &dst).map_err(|e| e.to_string())?; }
    }
    Ok(())
}

/// Swap the new copy in; returns what to launch.
fn install(zip: &Path) -> Result<PathBuf, String> {
    let root = install_root().ok_or("Sard isn't running from an installed copy, so it can't update itself.")?;
    let dir = zip.parent().unwrap().join("unpacked");
    let _ = std::fs::remove_dir_all(&dir);
    unzip(zip, &dir)?;
    let new = find_new(&dir).ok_or("The download doesn't contain Sard.")?;
    if cfg!(target_os = "macos") {
        let old = root.with_extension("app.old");
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&root, &old).map_err(|e| crate::i18n::trf("Couldn't replace {}: {}", &[&root.display(), &e]))?;
        let moved = std::fs::rename(&new, &root).or_else(|_| run(std::process::Command::new("ditto").arg(&new).arg(&root)).map_err(std::io::Error::other));
        if let Err(e) = moved { let _ = std::fs::rename(&old, &root); return Err(e.to_string()); }
        Ok(root)
    } else {
        // A running program can be renamed but not overwritten (Windows); move it aside first.
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let old = exe.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
        if let Err(e) = copy_dir(&new, &root) { let _ = std::fs::rename(&old, &exe); return Err(e); }
        Ok(exe)
    }
}

/// Remove the copy an update replaced (called at start).
pub fn clean_old() {
    if let Some(root) = install_root() {
        if cfg!(target_os = "macos") { let _ = std::fs::remove_dir_all(root.with_extension("app.old")); }
        else if let Ok(exe) = std::env::current_exe() { let _ = std::fs::remove_file(exe.with_extension("old")); }
    }
}

fn start_download(ctx: &egui::Context, r: Release) {
    set(State::Downloading(r.clone()));
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let s = match download(&r).and_then(|z| install(&z)) { Ok(p) => State::Ready(r, p), Err(e) => State::Failed(e) };
        set(s);
        ctx.request_repaint();
    });
}

fn restart(path: &Path) {
    let _ = if cfg!(target_os = "macos") { std::process::Command::new("open").arg("-n").arg(path).spawn() } else { std::process::Command::new(path).spawn() };
    std::process::exit(0);
}

impl SlidesApp {
    /// Once a day at start (unless turned off in Options), look for a new version.
    pub fn auto_check_updates(&mut self, ctx: &egui::Context) {
        static ONCE: std::sync::Once = std::sync::Once::new();
        if cfg!(test) { return; }
        ONCE.call_once(|| {
            clean_old();
            let s = &mut self.prefs;
            if !s.auto_update || now().saturating_sub(s.last_update_check) < DAY { return; }
            s.last_update_check = now();
            s.save();
            check(ctx, false, s.skipped_version.clone());
        });
    }
}

/// The Software Update dialog.
pub fn dialog(app: &mut SlidesApp, ctx: &egui::Context) {
    if !SHOW.load(Ordering::Relaxed) { return; }
    let mut open = true;
    let st = state();
    if matches!(st, State::Checking | State::Downloading(_)) { ctx.request_repaint_after(Duration::from_millis(100)); }
    let mut close = false;
    crate::theme::dialog(ctx, "Software Update").open(&mut open).collapsible(false).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.set_width(420.0);
        let dirty = app.dirty;
        match &st {
            State::Idle | State::Checking => { crate::theme::hrow(ui, |ui| { ui.spinner(); ui.label(crate::i18n::tr("Checking for updates…")); }); }
            State::UpToDate => {
                ui.label(RichText::new(crate::i18n::tr("Sard is up to date.")).strong());
                ui.label(crate::i18n::trf("Version {}", &[&CURRENT]));
                crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { close = true; });
            }
            State::Available(r) => {
                ui.label(RichText::new(crate::i18n::trf("Sard {} is available", &[&r.version])).size(16.0).strong());
                ui.label(crate::i18n::trf("You have version {}.", &[&CURRENT]));
                if !r.notes.is_empty() {
                    ui.add_space(4.0);
                    egui::Frame::group(ui.style()).show(ui, |ui| { ui.set_width(400.0); egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| ui.label(&r.notes)); });
                }
                ui.hyperlink_to(crate::i18n::tr("Release page"), &r.page);
                let can_install = r.url.is_some() && self_updatable();
                if !can_install { ui.weak(crate::i18n::tr("Download it from the release page to update this copy.")); }
                crate::theme::footer(ui, |ui| {
                    if can_install && crate::theme::primary(ui, "Download and Install").clicked() { start_download(ctx, r.clone()); }
                    if crate::theme::secondary(ui, "Skip This Version").clicked() {
                        app.prefs.skipped_version = Some(r.version.clone());
                        app.prefs.save();
                        close = true;
                    }
                    if crate::theme::secondary(ui, "Remind Me Later").clicked() { close = true; }
                });
            }
            State::Downloading(r) => {
                ui.label(crate::i18n::trf("Downloading Sard {}…", &[&r.version]));
                let (done, total) = (DONE.load(Ordering::Relaxed), TOTAL.load(Ordering::Relaxed));
                let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                ui.add(egui::ProgressBar::new(frac).desired_width(400.0).show_percentage().animate(total == 0));
                ui.weak(format!("{:.1} / {:.1} MB", done as f64 / 1e6, total as f64 / 1e6));
            }
            State::Ready(r, path) => {
                ui.label(RichText::new(crate::i18n::trf("Sard {} is installed.", &[&r.version])).strong());
                ui.label(crate::i18n::tr("Restart Sard to use the new version."));
                if dirty { ui.colored_label(egui::Color32::from_rgb(0xC0, 0x50, 0x00), crate::i18n::tr("Save your presentation before restarting.")); }
                crate::theme::footer(ui, |ui| {
                    if ui.add_enabled_ui(!dirty, |ui| crate::theme::primary(ui, "Restart Now")).inner.clicked() { restart(path); }
                    if crate::theme::secondary(ui, "Later").clicked() { close = true; }
                });
            }
            State::Failed(e) => {
                ui.label(RichText::new(crate::i18n::tr("Couldn't update Sard.")).strong());
                ui.weak(e);
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "Try Again").clicked() { check(ctx, true, None); }
                    if crate::theme::secondary(ui, "Close").clicked() { close = true; }
                });
            }
        }
    });
    if close { open = false; }
    if !open {
        SHOW.store(false, Ordering::Relaxed);
        // A finished check goes back to idle; a download keeps going in the background.
        if matches!(state(), State::UpToDate | State::Failed(_)) { set(State::Idle); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("1.10", "1.9.5"));
        assert!(newer("v1.0.1", "1.0"));
        assert!(!newer("1.0.0", "1.0"));
        assert!(!newer("0.9.9-beta", "1.0.0"));
    }

    #[test]
    fn manifest_picks_this_platform() {
        let v = serde_json::json!({ "version": "9.0.0", "notes": "New", "downloads": { platform(): { "url": "https://x/q.zip", "sha256": "ABC" } } });
        let r = parse_manifest(&v).unwrap();
        assert_eq!(r.version, "9.0.0");
        assert_eq!(r.url.as_deref(), Some("https://x/q.zip"));
        assert_eq!(r.sha256.as_deref(), Some("abc"));
        assert!(parse_manifest(&serde_json::json!({ "notes": "x" })).is_err());
    }
}
