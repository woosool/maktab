//! Files the operating system asks the app to open.
//!
//! Windows and Linux pass them on the command line (as does macOS for `open --args`). macOS
//! Finder (double-click, Open With, dropping on the Dock icon) instead sends an Apple Event,
//! which AppKit hands to the application delegate's `application:openURLs:`; winit's delegate
//! doesn't have that method, so [`install`] adds it. Paths land in a queue the app drains each
//! frame with [`take`] (a repaint is requested when one arrives).
//!
//! The apps are single-window, so a second file opened from the system on Windows/Linux simply
//! starts another copy of the app (another window), as before; there's no single-instance
//! forwarding. On macOS the system always sends the file to the running copy: the first file
//! replaces the current document (asking to save first when it has changes, as File ▸ Open
//! does), and any further files at once each open in another copy ([`open_elsewhere`]).

use std::path::PathBuf;
use std::sync::Mutex;

static QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static WAKE: Mutex<Option<egui::Context>> = Mutex::new(None);

/// Adds files to the queue (from the system's open event) and wakes the app.
pub fn push(paths: impl IntoIterator<Item = PathBuf>) {
    let mut n = 0;
    if let Ok(mut q) = QUEUE.lock() { for p in paths { if !q.contains(&p) { q.push(p); n += 1; } } }
    if n > 0 { if let Some(ctx) = WAKE.lock().ok().and_then(|c| c.clone()) { ctx.request_repaint(); } }
}

/// Takes every file waiting to be opened.
pub fn take() -> Vec<PathBuf> { QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default() }

/// Whether files are waiting.
pub fn pending() -> bool { QUEUE.lock().map(|q| !q.is_empty()).unwrap_or(false) }

/// The context to repaint when files arrive while the app is idle.
pub fn set_context(ctx: &egui::Context) { if let Ok(mut c) = WAKE.lock() { *c = Some(ctx.clone()); } }

/// The files named on the command line (skipping macOS's old `-psn_…` process argument and
/// other options).
pub fn args_paths(args: impl IntoIterator<Item = String>) -> Vec<PathBuf> {
    args.into_iter().filter(|a| !a.is_empty() && !a.starts_with("-psn_") && !a.starts_with("--")).map(PathBuf::from).collect()
}

/// The files to open at start: the command line's, then any the system sent before the window
/// existed. The first opens in this window, the rest in other copies (see [`route`]).
pub fn startup_paths(args: impl IntoIterator<Item = String>) -> (Option<PathBuf>, Vec<PathBuf>) {
    let mut all = args_paths(args);
    for p in take() { if !all.contains(&p) { all.push(p); } }
    route(all)
}

/// Splits files that arrived together: the first opens in this window, the rest elsewhere.
pub fn route(mut paths: Vec<PathBuf>) -> (Option<PathBuf>, Vec<PathBuf>) {
    if paths.is_empty() { return (None, paths); }
    let first = paths.remove(0);
    (Some(first), paths)
}

/// Opens each file in another copy of this app (a new window).
pub fn open_elsewhere(paths: &[PathBuf]) {
    let Ok(exe) = std::env::current_exe() else { return };
    for p in paths { let _ = std::process::Command::new(&exe).arg(p).spawn(); }
}

/// Starts listening for the system's "open these files" event (macOS); call before the event
/// loop starts. Elsewhere it does nothing (the command line carries the files).
pub fn install() {
    #[cfg(target_os = "macos")]
    mac::install();
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, NSObject, Sel};
    use objc2::{ClassType, class, msg_send, sel};
    use std::ffi::{CStr, c_char};

    /// `-[NSApplicationDelegate application:openURLs:]`, added to winit's delegate class.
    unsafe extern "C-unwind" fn open_urls(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject, urls: *mut AnyObject) {
        if urls.is_null() { return; }
        let mut out = vec![];
        unsafe {
            let n: usize = msg_send![urls, count];
            for i in 0..n {
                let url: *mut AnyObject = msg_send![urls, objectAtIndex: i];
                if url.is_null() { continue; }
                let is_file: bool = msg_send![url, isFileURL];
                if !is_file { continue; }
                let path: *mut AnyObject = msg_send![url, path];
                if path.is_null() { continue; }
                let s: *const c_char = msg_send![path, UTF8String];
                if s.is_null() { continue; }
                out.push(std::path::PathBuf::from(CStr::from_ptr(s).to_string_lossy().into_owned()));
            }
        }
        super::push(out);
    }

    /// Runs on NSApplicationWillFinishLaunchingNotification: the delegate exists by now, and
    /// files opened at launch are delivered right after this.
    extern "C-unwind" fn will_finish(_this: *mut NSObject, _cmd: Sel, _note: *mut AnyObject) {
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            if app.is_null() { return; }
            let delegate: *mut AnyObject = msg_send![app, delegate];
            if delegate.is_null() { return; }
            let cls = objc2::ffi::object_getClass(delegate) as *mut AnyClass;
            let imp: objc2::runtime::Imp = std::mem::transmute(open_urls as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject));
            objc2::ffi::class_addMethod(cls, sel!(application:openURLs:), imp, c"v@:@@".as_ptr());
            // NSApplication remembers which methods its delegate has when it's set; set it again.
            let _: () = msg_send![app, setDelegate: delegate];
        }
    }

    pub fn install() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            let Some(mut b) = ClassBuilder::new(c"WofficeOpenFilesObserver", NSObject::class()) else { return };
            b.add_method(sel!(willFinish:), will_finish as extern "C-unwind" fn(*mut NSObject, Sel, *mut AnyObject));
            let cls = b.register();
            let observer: *mut AnyObject = msg_send![cls, new]; // kept for the app's lifetime
            let name: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: c"NSApplicationWillFinishLaunchingNotification".as_ptr()];
            let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![center, addObserver: observer, selector: sel!(willFinish:), name: name, object: nil];
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // The queue is global; one test exercises it in order.
    #[test]
    fn queue_and_startup() {
        let _ = take();
        push([PathBuf::from("/a.xlsx"), PathBuf::from("/b.xlsx"), PathBuf::from("/a.xlsx")]);
        assert!(pending());
        assert_eq!(take(), vec![PathBuf::from("/a.xlsx"), PathBuf::from("/b.xlsx")]);
        assert!(take().is_empty());
        // At start the command line comes first, then files the system sent.
        push([PathBuf::from("/sent.csv")]);
        let (first, rest) = startup_paths(["-psn_0_123".to_string(), "/arg.xlsx".to_string()]);
        assert_eq!(first, Some(PathBuf::from("/arg.xlsx")));
        assert_eq!(rest, vec![PathBuf::from("/sent.csv")]);
        assert!(!pending());
        push([PathBuf::from("/x.ods"), PathBuf::from("/y.ods")]);
        assert_eq!(startup_paths(Vec::<String>::new()), (Some(PathBuf::from("/x.ods")), vec![PathBuf::from("/y.ods")]));
        assert_eq!(startup_paths(Vec::<String>::new()), (None, vec![]));
    }

    #[test]
    fn args_and_routing() {
        assert_eq!(args_paths(["--flag".into(), "-psn_0_1".into(), "f.docx".into(), "".into()]), vec![PathBuf::from("f.docx")]);
        let (here, rest) = route(vec!["1".into(), "2".into(), "3".into()]);
        assert_eq!(here, Some(PathBuf::from("1")));
        assert_eq!(rest, vec![PathBuf::from("2"), PathBuf::from("3")]);
        assert_eq!(route(vec![]), (None, vec![]));
    }
}
