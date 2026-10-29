//! Whether the app is the system's default for its file types, and making it so (Options ▸
//! Default Apps, and a one-time prompt at first start).
//!
//! - macOS: LaunchServices (`LSCopyDefaultRoleHandlerForContentType` /
//!   `LSSetDefaultRoleHandlerForContentType`) per file type, by bundle id. Newer macOS may show
//!   its own confirmation; the status is checked again afterwards.
//! - Windows: the user's choice (`HKCU\…\Explorer\FileExts\.ext\UserChoice` ProgId) is read;
//!   it can't be set by an app, so Settings ▸ Default apps opens on the app's page.
//! - Linux: `xdg-mime query default` / `xdg-mime default <desktop file> <types>`.
//!
//! The platform calls sit behind [`Backend`] so tests can replace them ([`set_backend`]).

use crate::i18n::{tr, trf};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A file type an app opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Format { pub ext: &'static str, pub mime: &'static str }

/// How the packaging registers an app with the system.
#[derive(Debug, PartialEq)]
pub struct AppId {
    /// The app's English name (also the Windows registered-application name).
    pub name: &'static str,
    /// The macOS bundle identifier (Info.plist CFBundleIdentifier).
    pub bundle_id: &'static str,
    /// The Linux desktop entry's file name.
    pub desktop: &'static str,
    /// Windows ProgIDs are `<prog_prefix>.<ext>` (an older `<name>.<ext>` is accepted too).
    pub prog_prefix: &'static str,
    pub formats: &'static [Format],
}

const fn f(ext: &'static str, mime: &'static str) -> Format { Format { ext, mime } }

pub static QALAM: AppId = AppId {
    name: "Qalam", bundle_id: "com.woosool.qalam", desktop: "qalam.desktop", prog_prefix: "Woosool.Qalam",
    formats: &[
        f("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        f("dotx", "application/vnd.openxmlformats-officedocument.wordprocessingml.template"),
        f("docm", "application/vnd.ms-word.document.macroEnabled.12"),
        f("dotm", "application/vnd.ms-word.template.macroEnabled.12"),
        f("md", "text/markdown"),
        f("txt", "text/plain"),
    ],
};

pub static SARD: AppId = AppId {
    name: "Sard", bundle_id: "com.woosool.sard", desktop: "sard.desktop", prog_prefix: "Woosool.Sard",
    formats: &[
        f("pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        f("potx", "application/vnd.openxmlformats-officedocument.presentationml.template"),
        f("ppsx", "application/vnd.openxmlformats-officedocument.presentationml.slideshow"),
    ],
};

pub static JADWAL: AppId = AppId {
    name: "Jadwal", bundle_id: "com.woosool.jadwal", desktop: "jadwal.desktop", prog_prefix: "Woosool.Jadwal",
    formats: &[
        f("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        f("xlsm", "application/vnd.ms-excel.sheet.macroEnabled.12"),
        f("xltx", "application/vnd.openxmlformats-officedocument.spreadsheetml.template"),
        f("xltm", "application/vnd.ms-excel.template.macroEnabled.12"),
        f("ods", "application/vnd.oasis.opendocument.spreadsheet"),
        f("csv", "text/csv"),
        f("tsv", "text/tab-separated-values"),
    ],
};

/// What happened after asking to become the default.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The system took the change (macOS may still ask the user to confirm).
    Changed,
    /// The system's settings opened for the user to choose (Windows).
    SettingsOpened,
}

/// The platform's default-app calls.
pub trait Backend: Send + Sync {
    /// The handler the system uses for this type: a bundle id, ProgID or desktop file name.
    fn handler(&self, app: &AppId, format: &Format) -> Option<String>;
    fn make_default(&self, app: &AppId) -> Result<Outcome, String>;
}

/// Whether a handler name (bundle id, ProgID or desktop file) is this app.
pub fn is_ours(app: &AppId, handler: &str) -> bool {
    let h = handler.trim().to_ascii_lowercase();
    if h.is_empty() { return false; }
    if h == app.bundle_id.to_ascii_lowercase() || h == app.desktop.to_ascii_lowercase() { return true; }
    // ProgIDs: Woosool.Jadwal.xlsx (or Jadwal.xlsx).
    let prog = h.strip_prefix("woosool.").unwrap_or(&h);
    prog.strip_prefix(&format!("{}.", app.name.to_ascii_lowercase())).is_some_and(|rest| !rest.is_empty())
}

/// Each type's extension and whether this app is its default.
pub fn status_with(b: &dyn Backend, app: &AppId) -> Vec<(&'static str, bool)> {
    app.formats.iter().map(|f| (f.ext, b.handler(app, f).is_some_and(|h| is_ours(app, &h)))).collect()
}

/// The ProgId from `reg query …\UserChoice /v ProgId` output.
pub fn parse_reg_progid(out: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        (it.next()? .eq_ignore_ascii_case("ProgId") && it.next()?.starts_with("REG_")).then(|| it.collect::<Vec<_>>().join(" ")).filter(|s| !s.is_empty())
    })
}

/// The Windows Settings page for the app's default apps.
pub fn windows_settings_uri(app: &AppId) -> String { format!("ms-settings:defaultapps?registeredAppUser={}", app.name) }

/// The `xdg-mime default` arguments.
pub fn xdg_default_args(app: &AppId) -> Vec<String> {
    let mut v = vec!["default".to_string(), app.desktop.to_string()];
    v.extend(app.formats.iter().map(|f| f.mime.to_string()));
    v
}

// ---- the real platform calls --------------------------------------------------------------

struct System;

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void};
    type CFStringRef = *const c_void;
    const UTF8: u32 = 0x0800_0100;
    const ROLES_ALL: u32 = 0xFFFF_FFFF;
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithBytes(alloc: *const c_void, bytes: *const u8, len: isize, enc: u32, external: u8) -> CFStringRef;
        fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, enc: u32) -> u8;
        fn CFRelease(p: *const c_void);
    }
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultRoleHandlerForContentType(uti: CFStringRef, role: u32) -> CFStringRef;
        fn LSSetDefaultRoleHandlerForContentType(uti: CFStringRef, role: u32, bundle: CFStringRef) -> i32;
        fn UTTypeCreatePreferredIdentifierForTag(class: CFStringRef, tag: CFStringRef, conforming: CFStringRef) -> CFStringRef;
    }
    struct Cf(CFStringRef);
    impl Drop for Cf { fn drop(&mut self) { if !self.0.is_null() { unsafe { CFRelease(self.0) } } } }
    fn cf(s: &str) -> Cf { Cf(unsafe { CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as isize, UTF8, 0) }) }
    fn string(s: &Cf) -> Option<String> {
        if s.0.is_null() { return None; }
        let mut buf = vec![0 as c_char; 1024];
        (unsafe { CFStringGetCString(s.0, buf.as_mut_ptr(), buf.len() as isize, UTF8) } != 0)
            .then(|| unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
    }
    /// The file type identifier for an extension (public.comma-separated-values-text, …).
    pub fn uti(ext: &str) -> Cf { let (c, t) = (cf("public.filename-extension"), cf(ext)); Cf(unsafe { UTTypeCreatePreferredIdentifierForTag(c.0, t.0, std::ptr::null()) }) }
    pub fn handler(ext: &str) -> Option<String> { let u = uti(ext); if u.0.is_null() { return None; } string(&Cf(unsafe { LSCopyDefaultRoleHandlerForContentType(u.0, ROLES_ALL) })) }
    pub fn set(ext: &str, bundle: &str) -> Result<(), String> {
        let (u, b) = (uti(ext), cf(bundle));
        if u.0.is_null() { return Err(format!(".{ext}")); }
        match unsafe { LSSetDefaultRoleHandlerForContentType(u.0, ROLES_ALL, b.0) } { 0 => Ok(()), e => Err(format!(".{ext}: {e}")) }
    }
}

#[cfg(target_os = "windows")]
fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}

impl Backend for System {
    #[allow(unused_variables)]
    fn handler(&self, app: &AppId, format: &Format) -> Option<String> {
        #[cfg(target_os = "macos")]
        { mac::handler(format.ext) }
        #[cfg(target_os = "windows")]
        {
            let key = format!(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{}\UserChoice", format.ext);
            let out = hidden(&mut std::process::Command::new("reg")).args(["query", &key, "/v", "ProgId"]).output().ok()?;
            parse_reg_progid(&String::from_utf8_lossy(&out.stdout))
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let out = std::process::Command::new("xdg-mime").args(["query", "default", format.mime]).output().ok()?;
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
        }
    }
    #[allow(unused_variables)]
    fn make_default(&self, app: &AppId) -> Result<Outcome, String> {
        #[cfg(target_os = "macos")]
        {
            let errs: Vec<String> = app.formats.iter().filter_map(|f| mac::set(f.ext, app.bundle_id).err()).collect();
            if errs.is_empty() { Ok(Outcome::Changed) } else { Err(errs.join(", ")) }
        }
        #[cfg(target_os = "windows")]
        {
            hidden(&mut std::process::Command::new("cmd")).args(["/C", "start", "", &windows_settings_uri(app)]).spawn().map_err(|e| e.to_string())?;
            Ok(Outcome::SettingsOpened)
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let st = std::process::Command::new("xdg-mime").args(xdg_default_args(app)).status().map_err(|e| e.to_string())?;
            if st.success() { Ok(Outcome::Changed) } else { Err(format!("xdg-mime: {st}")) }
        }
    }
}

static BACKEND: Mutex<Option<Arc<dyn Backend>>> = Mutex::new(None);

/// Replaces the platform calls (tests).
pub fn set_backend(b: Arc<dyn Backend>) {
    if let Ok(mut g) = BACKEND.lock() { *g = Some(b); }
    if let Ok(mut s) = STATE.lock() { s.clear(); }
}
fn backend() -> (Arc<dyn Backend>, bool) {
    match BACKEND.lock().ok().and_then(|g| g.clone()) { Some(b) => (b, true), None => (Arc::new(System), false) }
}

// ---- cached status and the interface ---------------------------------------------------------

#[derive(Default, Clone)]
struct State { status: Option<Vec<(&'static str, bool)>>, checking: bool, note: Option<String> }
static STATE: Mutex<Vec<(&'static str, State)>> = Mutex::new(Vec::new());

fn with<R>(app: &AppId, f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let i = match g.iter().position(|(n, _)| *n == app.name) { Some(i) => i, None => { g.push((app.name, State::default())); g.len() - 1 } };
    f(&mut g[i].1)
}

/// The cached status, starting a check (off the interface thread) when there's none.
pub fn status(app: &'static AppId, ctx: &egui::Context) -> Option<Vec<(&'static str, bool)>> {
    let (start, cur) = with(app, |s| { let start = s.status.is_none() && !s.checking; if start { s.checking = true; } (start, s.status.clone()) });
    if start {
        let (b, test) = backend();
        let run = move || { let st = status_with(&*b, app); with(app, |s| { s.status = Some(st); s.checking = false; }); };
        if test { run(); return with(app, |s| s.status.clone()); }
        let ctx = ctx.clone();
        std::thread::spawn(move || { run(); ctx.request_repaint(); });
    }
    cur
}

/// Forget the status so it's checked again.
pub fn refresh(app: &AppId) { with(app, |s| if !s.checking { s.status = None; }); }

/// Asks the system to make the app the default for all its types.
pub fn make_default(app: &'static AppId, ctx: &egui::Context) {
    let note = match backend().0.make_default(app) {
        Ok(Outcome::Changed) => if cfg!(target_os = "macos") { Some(tr("If macOS asks, confirm the change.")) } else { None },
        Ok(Outcome::SettingsOpened) => Some(trf("Windows Settings opened: choose {} for these file types, then select Check Again.", &[&tr(app.name)])),
        Err(e) => Some(trf("Couldn't change the default app: {}", &[&e])),
    };
    with(app, |s| { s.note = note; if !s.checking { s.status = None; } });
    // macOS's confirmation comes a moment later; look again then.
    if cfg!(target_os = "macos") && backend().1 == false {
        let ctx = ctx.clone();
        std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_secs(3)); refresh(app); ctx.request_repaint(); });
    }
}

/// Options ▸ Default Apps: each type with whether the app opens it, and a button to make it so.
pub fn options_section(ui: &mut egui::Ui, app: &'static AppId) {
    crate::theme::section(ui, "Default Apps");
    let name = tr(app.name);
    match status(app, ui.ctx()) {
        None => { ui.label(tr("Checking…")); }
        Some(st) => {
            egui::Grid::new(("default-apps", app.name)).num_columns(2).spacing(egui::vec2(16.0, 4.0)).show(ui, |ui| {
                for (ext, ours) in &st {
                    ui.label(format!(".{ext}"));
                    ui.label(if *ours { format!("✔ {name}") } else { tr("Another app") });
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                if st.iter().all(|(_, o)| *o) { ui.label(trf("{} is the default app for all of these file types.", &[&name])); }
                else if ui.button(trf("Make {} the Default App", &[&name])).clicked() { make_default(app, ui.ctx()); }
                if ui.button(tr("Check Again")).clicked() { refresh(app); }
            });
        }
    }
    if let Some(n) = with(app, |s| s.note.clone()) { ui.label(egui::RichText::new(n).weak()); }
}

/// Once, at first start: offers to make the app the default when it isn't. `marker` is a file
/// written once the question has been answered (or wasn't needed).
pub fn first_run_prompt(ctx: &egui::Context, app: &'static AppId, marker: &Path) {
    if marker.exists() { return; }
    let Some(st) = status(app, ctx) else { return };
    let done = || { if let Some(d) = marker.parent() { let _ = std::fs::create_dir_all(d); } let _ = std::fs::write(marker, "1"); };
    if st.iter().all(|(_, o)| *o) { done(); return; }
    let name = tr(app.name);
    let list = app.formats.iter().map(|f| format!(".{}", f.ext)).collect::<Vec<_>>().join(" ");
    let mut open = true;
    let mut answered = false;
    crate::theme::dialog(ctx, "Default Apps").open(&mut open).show(ctx, |ui| {
        ui.label(trf("Make {} your default app for {}?", &[&name, &list]));
        ui.label(egui::RichText::new(tr("You can change this later in Options.")).weak());
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Make Default").clicked() { make_default(app, ui.ctx()); answered = true; }
            if crate::theme::secondary(ui, "Not Now").clicked() { answered = true; }
        });
    });
    if answered || !open { done(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Fake(Mutex<HashMap<&'static str, String>>);
    impl Backend for Fake {
        fn handler(&self, _: &AppId, f: &Format) -> Option<String> { self.0.lock().unwrap().get(f.ext).cloned() }
        fn make_default(&self, app: &AppId) -> Result<Outcome, String> {
            let mut m = self.0.lock().unwrap();
            for f in app.formats { m.insert(f.ext, app.bundle_id.to_string()); }
            Ok(Outcome::Changed)
        }
    }

    #[test]
    fn recognizes_handlers() {
        assert!(is_ours(&JADWAL, "com.woosool.jadwal"));
        assert!(is_ours(&JADWAL, "COM.WOOSOOL.JADWAL"));
        assert!(is_ours(&JADWAL, "jadwal.desktop"));
        assert!(is_ours(&JADWAL, "Woosool.Jadwal.xlsx"));
        assert!(is_ours(&QALAM, "Qalam.docx"));
        assert!(!is_ours(&JADWAL, "Excel.Sheet.12"));
        assert!(!is_ours(&JADWAL, "libreoffice-calc.desktop"));
        assert!(!is_ours(&JADWAL, ""));
        assert!(!is_ours(&SARD, "com.woosool.qalam"));
        assert!(is_ours(&SARD, "com.woosool.sard"));
    }

    #[test]
    fn parses_windows_and_linux_helpers() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\...\\UserChoice\r\n    ProgId    REG_SZ    Woosool.Jadwal.xlsx\r\n\r\n";
        assert_eq!(parse_reg_progid(out).as_deref(), Some("Woosool.Jadwal.xlsx"));
        assert_eq!(parse_reg_progid("ERROR: The system was unable to find the specified registry key"), None);
        assert_eq!(windows_settings_uri(&SARD), "ms-settings:defaultapps?registeredAppUser=Sard");
        let a = xdg_default_args(&JADWAL);
        assert_eq!(&a[..2], &["default".to_string(), "jadwal.desktop".to_string()]);
        assert!(a.contains(&"text/csv".to_string()));
    }

    #[test]
    fn status_and_make_default() {
        let fake = Fake(Mutex::new(HashMap::from([("xlsx", "com.microsoft.excel".to_string()), ("csv", "jadwal.desktop".to_string())])));
        let st = status_with(&fake, &JADWAL);
        assert_eq!(st.iter().find(|s| s.0 == "csv").unwrap().1, true);
        assert_eq!(st.iter().find(|s| s.0 == "xlsx").unwrap().1, false);
        assert_eq!(fake.make_default(&JADWAL), Ok(Outcome::Changed));
        assert!(status_with(&fake, &JADWAL).iter().all(|s| s.1));
    }
}
