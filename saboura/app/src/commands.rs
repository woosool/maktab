//! Every command, and the main menu built from them: the native menu bar on macOS and
//! Windows (muda), a menu bar in the window elsewhere, and the command palette.

use crate::app::{BoardApp, Panel, Pending, Tool};
use crate::i18n::tr;

/// A menu entry: command ID, label and accelerator (muda syntax), or a separator ("-").
pub type Entry = (&'static str, &'static str, &'static str);

pub const MENUS: &[(&str, &[Entry])] = &[
    ("File", &[
        ("new", "New Board", "CmdOrCtrl+N"), ("templates", "New from Template…", ""), ("home", "Boards…", ""), ("open", "Open…", "CmdOrCtrl+O"), ("recent", "Open Recent…", ""),
        ("-", "", ""), ("save", "Save", "CmdOrCtrl+S"), ("save-as", "Save As…", "CmdOrCtrl+Shift+S"),
        ("-", "", ""), ("export-png", "Export as PNG…", ""), ("export-svg", "Export as SVG…", ""), ("export-pdf", "Export as PDF…", ""), ("export-pptx", "Export as PowerPoint…", ""), ("export-xlsx", "Export as Excel Workbook…", ""), ("export-json", "Export as Whiteboard JSON…", ""),
        ("-", "", ""), ("import-image", "Import Image…", ""), ("import-text", "Import Text as Notes…", ""), ("import-miro", "Import Miro Board…", ""),
        ("-", "", ""), ("quit", "Quit", "CmdOrCtrl+Q"),
    ]),
    ("Edit", &[
        ("undo", "Undo", "CmdOrCtrl+Z"), ("redo", "Redo", "CmdOrCtrl+Shift+Z"),
        ("-", "", ""), ("cut", "Cut", "CmdOrCtrl+X"), ("copy", "Copy", "CmdOrCtrl+C"), ("paste", "Paste", "CmdOrCtrl+V"),
        ("duplicate", "Duplicate", "CmdOrCtrl+D"), ("delete", "Delete", ""),
        ("-", "", ""), ("select-all", "Select All", "CmdOrCtrl+A"), ("find", "Find on Board…", "CmdOrCtrl+F"),
        ("-", "", ""), ("link", "Link…", ""), ("image-options", "Image Options…", ""), ("tags", "Tags…", ""),
    ]),
    ("Insert", &[
        ("tool-note", "Sticky Note", ""), ("tool-text", "Text", ""), ("tool-rect", "Rectangle", ""), ("tool-ellipse", "Ellipse", ""),
        ("library", "Shape Library…", ""), ("icons", "Icons and Stickers…", ""), ("tool-line", "Connection Line", ""), ("tool-pen", "Pen", ""), ("tool-frame", "Frame", ""),
        ("-", "", ""), ("table", "Table", ""), ("mind-map", "Mind Map", ""), ("image", "Image…", ""),
        ("-", "", ""), ("tool-comment", "Comment", ""), ("timer", "Timer", ""), ("votes", "Dot Voting", ""), ("templates-add", "Template…", ""),
    ]),
    ("Arrange", &[
        ("front", "Bring to Front", "CmdOrCtrl+]"), ("back", "Send to Back", "CmdOrCtrl+["),
        ("-", "", ""), ("group", "Group", "CmdOrCtrl+G"), ("ungroup", "Ungroup", "CmdOrCtrl+Shift+G"), ("lock", "Lock or Unlock", "CmdOrCtrl+Shift+L"),
        ("-", "", ""), ("align-left", "Align Left", ""), ("align-center", "Align Centers", ""), ("align-right", "Align Right", ""),
        ("align-top", "Align Top", ""), ("align-middle", "Align Middles", ""), ("align-bottom", "Align Bottom", ""),
        ("distribute-h", "Distribute Horizontally", ""), ("distribute-v", "Distribute Vertically", ""),
        ("-", "", ""), ("reset-rotation", "Reset Rotation", ""), ("collapse", "Collapse or Expand Branch", ""),
    ]),
    ("View", &[
        ("zoom-in", "Zoom In", "CmdOrCtrl+="), ("zoom-out", "Zoom Out", "CmdOrCtrl+-"), ("zoom-100", "Actual Size", "CmdOrCtrl+0"), ("fit", "Fit Board", "Shift+1"),
        ("-", "", ""), ("frames", "Frames Panel", ""), ("comments", "Comments Panel", ""), ("present", "Present Frames", ""),
        ("-", "", ""), ("minimap", "Minimap", ""), ("grid", "Grid", ""), ("snap", "Snap to Objects", ""), ("dark", "Dark Mode", ""),
        ("-", "", ""), ("palette", "Command Palette…", "CmdOrCtrl+K"),
    ]),
    ("Help", &[("shortcuts", "Keyboard Shortcuts", ""), ("-", "", ""), ("lang-en", "English", ""), ("lang-fr", "Français", ""), ("lang-ar", "العربية", ""), ("-", "", ""), ("check-updates", "Check for Updates…", ""), ("auto-update", "Check for Updates Automatically", ""), ("-", "", ""), ("about", "About Saboura", "")]),
];

/// The label of a command, translated.
pub fn label(id: &str) -> String {
    if id.starts_with("lang-") { return MENUS.iter().flat_map(|m| m.1.iter()).find(|e| e.0 == id).map(|e| crate::i18n::shape(e.1)).unwrap_or_default(); }
    MENUS.iter().flat_map(|m| m.1.iter()).find(|e| e.0 == id).map(|e| tr(e.1)).unwrap_or_else(|| id.to_string())
}

/// An accelerator as people read it (⌘ on macOS, Ctrl elsewhere).
pub fn shortcut_text(accel: &str) -> String {
    let cmd = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
    let shift = if cfg!(target_os = "macos") { "⇧" } else { "Shift+" };
    accel.replace("CmdOrCtrl+", cmd).replace("Shift+", shift)
}

/// Whether a command applies now (greyed out in menus otherwise).
pub fn enabled(app: &BoardApp, id: &str) -> bool {
    let sel = app.selection.len();
    match id {
        "undo" => app.history.can_undo(),
        "redo" => app.history.can_redo(),
        "cut" | "copy" | "duplicate" | "delete" | "front" | "back" | "lock" | "link" => sel > 0,
        "group" => sel > 1,
        "ungroup" => app.selected_items().iter().any(|i| i.group_id.is_some()),
        "align-left" | "align-center" | "align-right" | "align-top" | "align-middle" | "align-bottom" => sel > 1,
        "distribute-h" | "distribute-v" => sel > 2,
        "image-options" => app.single().is_some_and(|i| i.kind == "image"),
        "reset-rotation" => app.single().is_some_and(|i| i.rotation() != 0.0),
        "collapse" => app.single().is_some_and(|i| app.board.items.iter().any(|c| c.mind_parent() == Some(i.id.as_str()))),
        _ => true,
    }
}

fn checked_ids(id: &str) -> bool { matches!(id, "minimap" | "grid" | "snap" | "dark" | "frames" | "comments" | "votes" | "lang-en" | "lang-fr" | "lang-ar" | "auto-update") }

/// Whether a toggle command is on (shown with a check).
pub fn checked(app: &BoardApp, id: &str) -> Option<bool> {
    Some(match id {
        "minimap" => app.prefs.minimap,
        "grid" => app.prefs.grid,
        "snap" => app.prefs.snap,
        "dark" => app.prefs.dark,
        "auto-update" => app.prefs.auto_update,
        "lang-en" | "lang-fr" | "lang-ar" => app.prefs.lang == id[5..],
        "frames" => app.side == crate::app::Side::Frames,
        "comments" => app.side == crate::app::Side::Comments,
        "votes" => app.side == crate::app::Side::Votes,
        _ => return None,
    })
}

pub fn run(app: &mut BoardApp, id: &str) {
    let center = app.view_center();
    match id {
        "new" => app.guard(Pending::New("blank")),
        "home" | "recent" => app.panel = Panel::Home,
        "templates" | "templates-add" => app.panel = Panel::Templates(String::new(), String::new()),
        "icons" => app.panel = Panel::Icons(String::new(), String::new(), 0),
        "votes" => app.toggle_side(crate::app::Side::Votes),
        "open" => app.guard(Pending::OpenDialog),
        "save" => { app.save(false); }
        "save-as" => { app.save(true); }
        "export-png" => crate::export::export(app, "png"),
        "export-svg" => crate::export::export(app, "svg"),
        "export-pdf" => crate::export::export(app, "pdf"),
        "export-pptx" => crate::export::export(app, "pptx"),
        "export-xlsx" => crate::export::export(app, "xlsx"),
        "export-json" => crate::export::export(app, "json"),
        "import-miro" => crate::export::import_miro(app),
        "import-image" | "image" => app.insert_image_dialog(),
        "import-text" => crate::export::import_text(app),
        "quit" => app.guard(Pending::Quit),
        "undo" => app.undo(),
        "redo" => app.redo(),
        "cut" => app.cut(),
        "copy" => app.copy(),
        "paste" => { let t = crate::export::clipboard_text(); app.paste(t, center); }
        "duplicate" => app.duplicate(),
        "delete" => app.delete_selection(),
        "select-all" => app.select_all(),
        "find" => app.search = Some((String::new(), 0)),
        "link" => app.panel = Panel::Link(app.single().and_then(|i| i.str("link")).unwrap_or("").to_string()),
        "image-options" => if let Some(i) = app.single() { app.panel = Panel::Image(crate::app::ImageOptions::of(i)); },
        "tags" => app.panel = Panel::Tags,
        "tool-note" => app.tool = Tool::Note,
        "tool-text" => app.tool = Tool::Text,
        "tool-rect" => { app.tool = Tool::Shape; app.shape = "rectangle"; }
        "tool-ellipse" => { app.tool = Tool::Shape; app.shape = "ellipse"; }
        "library" => app.panel = Panel::Library,
        "tool-line" => app.tool = Tool::Arrow,
        "tool-pen" => app.tool = Tool::Pen,
        "tool-frame" => app.tool = Tool::Frame,
        "tool-comment" => app.tool = Tool::Comment,
        "table" => app.insert_table(3, 3),
        "mind-map" => app.new_mind_map(),
        "timer" => app.timer_shown = !app.timer_shown,
        "front" => app.bring_to_front(true),
        "back" => app.bring_to_front(false),
        "group" => app.group(true),
        "ungroup" => app.group(false),
        "lock" => app.toggle_lock(),
        "align-left" | "align-center" | "align-right" | "align-top" | "align-middle" | "align-bottom" => app.align(&id[6..]),
        "distribute-h" => app.distribute(true),
        "distribute-v" => app.distribute(false),
        "reset-rotation" => app.edit_selected(|i| i.rotation = None),
        "collapse" => app.edit_selected(|i| { let on = !i.flag("mindCollapsed"); i.set_flag("mindCollapsed", on); }),
        "zoom-in" => app.zoom_by(true),
        "zoom-out" => app.zoom_by(false),
        "zoom-100" => app.zoom_reset(),
        "fit" => app.fit(),
        "frames" => app.toggle_side(crate::app::Side::Frames),
        "comments" => app.toggle_side(crate::app::Side::Comments),
        "present" => crate::panels::start_presenting(app),
        "minimap" => { app.prefs.minimap = !app.prefs.minimap; app.prefs.save(); }
        "grid" => { app.prefs.grid = !app.prefs.grid; app.prefs.save(); }
        "snap" => { app.prefs.snap = !app.prefs.snap; app.prefs.save(); }
        "dark" => { let d = !app.prefs.dark; app.set_dark(d); }
        "palette" => app.panel = Panel::Palette(String::new()),
        "shortcuts" => app.panel = Panel::Shortcuts,
        "about" => app.panel = Panel::About,
        "check-updates" => crate::update::check(&app.ctx.clone(), true, None),
        "auto-update" => { app.prefs.auto_update = !app.prefs.auto_update; app.prefs.save(); }
        "lang-en" | "lang-fr" | "lang-ar" => {
            app.prefs.lang = id[5..].to_string();
            app.prefs.save();
            crate::i18n::set(crate::i18n::Lang::from_code(&app.prefs.lang));
            // The native menu is rebuilt in the new language.
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            { app.native = None; }
            app.menu_tried = false;
        }
        _ => {}
    }
}

/// Commands that text fields handle themselves while one is focused.
const TEXT_COMMANDS: [&str; 6] = ["undo", "redo", "cut", "copy", "paste", "select-all"];

/// Runs a command chosen from the native menu. While text is being typed, the editing
/// commands go to the text field as the keys they stand for.
pub fn from_menu(app: &mut BoardApp, id: &str) {
    let typing = app.ctx.memory(|m| m.focused().is_some());
    if typing && TEXT_COMMANDS.contains(&id) {
        let cmd = egui::Modifiers::COMMAND;
        let ev = match id {
            "cut" => egui::Event::Cut,
            "copy" => egui::Event::Copy,
            "paste" => match crate::export::clipboard_text() { Some(t) => egui::Event::Paste(t), None => return },
            "undo" => egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: cmd },
            "redo" => egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: cmd | egui::Modifiers::SHIFT },
            _ => egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: cmd },
        };
        app.ctx.input_mut(|i| i.events.push(ev));
        return;
    }
    run(app, id);
}

// ---- The native menu bar ----

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod native {
    use super::*;
    use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu, accelerator::Accelerator};

    /// The menu bar and its items (kept to update enabled and checked states).
    pub struct Native { _menu: Menu, items: Vec<(String, MenuItem)>, checks: Vec<(String, CheckMenuItem)> }

    impl Native {
        pub fn build(_frame: &eframe::Frame) -> Option<Native> {
            let menu = Menu::new();
            let (mut items, mut checks) = (Vec::new(), Vec::new());
            #[cfg(target_os = "macos")]
            {
                let app_menu = Submenu::new(crate::APP_NAME, true);
                let about = MenuItem::with_id("about", crate::i18n::raw("About Saboura"), true, None);
                let _ = app_menu.append_items(&[&about, &PredefinedMenuItem::separator(), &PredefinedMenuItem::services(None), &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::hide(None), &PredefinedMenuItem::hide_others(None), &PredefinedMenuItem::show_all(None), &PredefinedMenuItem::separator(),
                    &MenuItem::with_id("quit", crate::i18n::raw("Quit"), true, "CmdOrCtrl+Q".parse::<Accelerator>().ok())]);
                let _ = menu.append(&app_menu);
            }
            for (title, entries) in MENUS {
                let sub = Submenu::new(crate::i18n::raw(title), true);
                for (id, name, accel) in entries.iter() {
                    if cfg!(target_os = "macos") && matches!(*id, "quit" | "about") { continue; }
                    if *id == "-" { let _ = sub.append(&PredefinedMenuItem::separator()); continue; }
                    let acc = (!accel.is_empty()).then(|| accel.parse::<Accelerator>().ok()).flatten();
                    if checked_ids(id) {
                        let c = CheckMenuItem::with_id(*id, crate::i18n::raw(name), true, false, acc);
                        let _ = sub.append(&c);
                        checks.push((id.to_string(), c));
                    } else {
                        let m = MenuItem::with_id(*id, crate::i18n::raw(name), true, acc);
                        let _ = sub.append(&m);
                        items.push((id.to_string(), m));
                    }
                }
                #[cfg(target_os = "macos")]
                if *title == "View" { let _ = sub.append_items(&[&PredefinedMenuItem::separator(), &PredefinedMenuItem::fullscreen(None)]); }
                let _ = menu.append(&sub);
            }
            #[cfg(target_os = "macos")]
            menu.init_for_nsapp();
            #[cfg(target_os = "windows")]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                let Ok(h) = _frame.window_handle() else { return None };
                let RawWindowHandle::Win32(w) = h.as_raw() else { return None };
                // SAFETY: the handle is the app's live main window.
                unsafe { let _ = menu.init_for_hwnd(w.hwnd.get()); }
            }
            Some(Native { _menu: menu, items, checks })
        }

        /// Runs commands chosen since the last frame and refreshes enabled and checked states.
        pub fn poll(&self, app: &mut BoardApp) {
            while let Ok(ev) = MenuEvent::receiver().try_recv() { from_menu(app, ev.id().as_ref()); }
            for (id, m) in &self.items { let on = enabled(app, id); if m.is_enabled() != on { m.set_enabled(on); } }
            for (id, c) in &self.checks { let on = checked(app, id).unwrap_or(false); if c.is_checked() != on { c.set_checked(on); } }
        }
    }
}

/// The menu bar drawn in the window (Linux, and wherever the native bar isn't available).
pub fn menu_bar(app: &mut BoardApp, ui: &mut egui::Ui) {
    egui::Panel::top("menu-bar").frame(egui::Frame::NONE.fill(qalam_kit::colors::c_frame()).inner_margin(egui::Margin::symmetric(6, 2))).show(ui, |ui| {
        let layout = if crate::i18n::rtl() { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) };
        ui.with_layout(layout, |ui| {
            for (title, entries) in MENUS {
                ui.menu_button(tr(title), |ui| {
                    ui.set_min_width(230.0);
                    for (id, _, accel) in entries.iter() {
                        if *id == "-" { ui.separator(); continue; }
                        let mut text = label(id);
                        if let Some(on) = checked(app, id) { text = format!("{} {text}", if on { "✓" } else { "  " }); }
                        if crate::rtl::item(ui, &text, &shortcut_text(accel), enabled(app, id)).clicked() { ui.close(); run(app, id); }
                    }
                });
            }
        });
    });
}
