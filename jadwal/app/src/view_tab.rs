//! The View tab laid out like Excel's (customviews.js, sheetviews.js, zoomdialog.js,
//! scroll-control.js) and app-wide pieces from statusbar.js, quick-access.js, keytips.js and the
//! Tell Me box: Workbook Views (the page views plus Custom Views saved in the workbook), Show,
//! Zoom (the Zoom dialog, 100%, Zoom to Selection), Window (New Window as a second egui
//! viewport onto the same workbook, Arrange All, Freeze Panes, Split into four scrollable panes
//! with draggable bars, Hide/Unhide, View Side by Side with Synchronous Scrolling, Reset Window
//! Position, Switch Windows); the customizable status bar; the Quick Access Toolbar and its
//! customization; Tell Me command search; KeyTips (Alt or F10).

use crate::app::{SheetApp, Tab};
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use sheet_engine::review::CustomView;

#[derive(Default)]
pub struct ViewState {
    pub zoom_dialog: Option<(u32, String)>,
    pub views_dialog: Option<(Option<usize>, String, bool, bool)>,
    pub window2: Option<Win2>,
    pub side_by_side: bool,
    pub sync: bool,
    /// Split panes: the top-left pane's scroll (the bottom-right one is the app's), and which
    /// pane last had the mouse.
    pub split_scroll: egui::Vec2,
    pub active_pane: usize,
    pub tell_me: String,
    pub keytips: Option<char>,
    alt_down: bool,
    alt_clean: bool,
    last_scroll: egui::Vec2,
    pub autocorrect_dialog: Option<(Vec<(String, String)>, String, String)>,
}

/// A second window's own view of the workbook.
pub struct Win2 { pub sheet: usize, pub sel: crate::app::Selection, pub scroll: egui::Vec2, pub zoom: f32, pub grid_rect: Rect }

const W2: &str = "jadwal-window-2";

// ---------------------------------------------------------------------------------------------
// Commands (Tell Me, Quick Access, KeyTips)

pub struct Cmd { pub id: &'static str, pub label: &'static str, pub icon: &'static str, pub tip: Option<(Tab, &'static str)>, pub run: fn(&mut SheetApp) }

pub fn commands() -> Vec<Cmd> {
    use Tab::{Review as R, View as V};
    let c = |id, label, icon, tip, run| Cmd { id, label, icon, tip, run };
    vec![
        c("save", "Save", "save", None, |a: &mut SheetApp| { a.save(false); }),
        c("undo", "Undo", "undo", None, |a: &mut SheetApp| a.undo()),
        c("redo", "Redo", "redo", None, |a: &mut SheetApp| a.redo()),
        c("new", "New Workbook", "filePlus", None, |a: &mut SheetApp| a.guarded(crate::filemenu::Pending::New)),
        c("open", "Open", "folderOpen", None, |a: &mut SheetApp| a.guarded(crate::filemenu::Pending::Open)),
        c("print", "Print", "print", None, |a: &mut SheetApp| crate::print::open(a)),
        c("templates", "New from Template", "sheet", None, |a: &mut SheetApp| crate::backstage::open_templates(a)),
        c("info", "Workbook Info", "info", None, |a: &mut SheetApp| crate::backstage::open_info(a)),
        c("sortAsc", "Sort A to Z", "sort", None, |a: &mut SheetApp| a.sort(true)),
        c("sortDesc", "Sort Z to A", "sort", None, |a: &mut SheetApp| a.sort(false)),
        c("filter", "Filter", "filter", None, |a: &mut SheetApp| crate::ribbon::toggle_filter(a)),
        c("autosum", "AutoSum", "autoSum", None, |a: &mut SheetApp| a.autosum("SUM")),
        c("spelling", "Spelling", "spelling", Some((R, "S")), crate::review_tab::start_spelling),
        c("thesaurus", "Thesaurus", "thesaurus", Some((R, "E")), crate::review_tab::open_thesaurus),
        c("stats", "Workbook Statistics", "wordCount", Some((R, "W")), |a: &mut SheetApp| a.rv.stats = true),
        c("access", "Check Accessibility", "accessibility", Some((R, "A")), |a: &mut SheetApp| a.rv.access = !a.rv.access),
        c("lookup", "Smart Lookup", "smartLookup", Some((R, "L")), crate::review_tab::smart_lookup),
        c("translate", "Translate", "translate", Some((R, "T")), crate::review_tab::open_translate),
        c("comment", "New Comment", "newComment", Some((R, "C")), crate::review_tab::new_comment),
        c("comments", "Show Comments", "showComments", Some((R, "H")), |a: &mut SheetApp| a.rv.comments_pane = !a.rv.comments_pane),
        c("note", "New Note", "note", Some((R, "N")), |a: &mut SheetApp| { let p = a.sel.active; let t = a.book.sheets[a.sheet()].notes.get(&p).cloned().unwrap_or_default(); a.dialog = Some(crate::app::Dialog::Note { pos: p, text: t }); }),
        c("allNotes", "Show All Notes", "showNotes", Some((R, "O")), |a: &mut SheetApp| a.rv.show_notes = !a.rv.show_notes),
        c("protect", "Protect Sheet", "protectSheet", Some((R, "P")), |a: &mut SheetApp| { if !a.book.sheets[a.sheet()].protected { a.rv.protect = Some(crate::review_tab::ProtectDraft { password: String::new(), confirm: String::new(), allow: Default::default(), error: None }); a.rv.protect.as_mut().unwrap().allow = sheet_engine::review::SheetProtection::default().allow; } else { a.rv.unprotect = Some((false, String::new(), None)); } }),
        c("protectBook", "Protect Workbook", "protectWorkbook", Some((R, "B")), |a: &mut SheetApp| if a.book.review.structure { a.rv.unprotect = Some((true, String::new(), None)); } else { a.rv.protect_book = Some((String::new(), String::new(), None)); }),
        c("normal", "Normal View", "table", Some((V, "N")), |a: &mut SheetApp| crate::page_view::set_mode(a, crate::page_view::ViewMode::Normal)),
        c("pageBreak", "Page Break Preview", "pageBreakPreview", Some((V, "I")), |a: &mut SheetApp| crate::page_view::set_mode(a, crate::page_view::ViewMode::PageBreak)),
        c("pageLayout", "Page Layout View", "printLayout", Some((V, "P")), |a: &mut SheetApp| crate::page_view::set_mode(a, crate::page_view::ViewMode::PageLayout)),
        c("customViews", "Custom Views", "multiplePages", Some((V, "C")), |a: &mut SheetApp| a.rv.view.views_dialog = Some((None, String::new(), true, false)),),
        c("gridlines", "Gridlines", "gridlines", Some((V, "G")), |a: &mut SheetApp| { let s = a.sheet(); a.book.sheets[s].gridlines = !a.book.sheets[s].gridlines; a.dirty = true; }),
        c("headings", "Headings", "headings", Some((V, "H")), |a: &mut SheetApp| { let s = a.sheet(); a.book.sheets[s].headers = !a.book.sheets[s].headers; a.dirty = true; }),
        c("formulaBar", "Formula Bar", "formula", Some((V, "F")), |a: &mut SheetApp| { a.prefs.show_formula_bar = !a.prefs.show_formula_bar; a.prefs.save(); }),
        c("zoom", "Zoom", "zoom", Some((V, "Q")), open_zoom),
        c("zoom100", "Zoom to 100%", "zoom100", Some((V, "J")), |a: &mut SheetApp| a.zoom = 1.0),
        c("zoomSel", "Zoom to Selection", "zoomSelection", Some((V, "Z")), zoom_to_selection),
        c("newWindow", "New Window", "newWindow", Some((V, "K")), new_window),
        c("arrange", "Arrange All", "columns", Some((V, "A")), |a: &mut SheetApp| arrange(a, 1)),
        c("freeze", "Freeze Panes", "freezePanes", Some((V, "R")), |a: &mut SheetApp| a.freeze(0)),
        c("split", "Split", "split", Some((V, "S")), toggle_split),
        c("sideBySide", "View Side by Side", "sideToSide", Some((V, "B")), side_by_side),
        c("dark", "Dark Mode", "darkPage", Some((V, "D")), |a: &mut SheetApp| { let mut p = a.prefs.clone(); p.dark = !a.dark; a.apply_prefs(p); }),
        c("autocorrect", "AutoCorrect Options", "spelling", None, open_autocorrect),
        c("exportOds", "Export as OpenDocument Spreadsheet", "download", None, |a: &mut SheetApp| crate::backstage::export(a, "ods")),
    ]
}

fn run(app: &mut SheetApp, id: &str) { if let Some(c) = commands().into_iter().find(|c| c.id == id) { (c.run)(app); } }

/// The Quick Access Toolbar, its customize menu and the Tell Me box, at the end of the tab row.
pub fn quick_access(app: &mut SheetApp, ui: &mut Ui) {
    ui.spacing_mut().item_spacing.x = 4.0;
    let cmds = commands();
    let qat = crate::review_tab::settings(app).qat.clone();
    let r = small(ui, "more", None, false, true, "Customize Quick Access Toolbar");
    crate::theme::menu(&r, |ui| {
        menu_heading(ui, &tr("Customize Quick Access Toolbar"));
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for c in &cmds {
                let mut on = qat.iter().any(|q| q == c.id);
                if crate::theme::check(ui, &mut on, tr(c.label)).changed() {
                    let s = crate::review_tab::settings(app);
                    if on { s.qat.push(c.id.into()); } else { s.qat.retain(|q| q != c.id); }
                    s.save();
                }
            }
        });
    });
    for id in qat.iter().rev() {
        let Some(c) = cmds.iter().find(|c| c.id == *id) else { continue };
        let enabled = match c.id { "undo" => app.can_undo(), "redo" => app.can_redo(), _ => true };
        let tip = match c.id { "save" => "Save (⌘S)", "undo" => "Undo (⌘Z)", "redo" => "Redo (⇧⌘Z)", _ => c.label };
        if small(ui, c.icon, None, false, enabled, tip).clicked() { (c.run)(app); }
    }
    tell_me(app, ui, &cmds);
}

fn tell_me(app: &mut SheetApp, ui: &mut Ui, cmds: &[Cmd]) {
    let mut q = std::mem::take(&mut app.rv.view.tell_me);
    let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Tell me what you want to do")).desired_width(190.0).id(egui::Id::new("tell-me")));
    let low = q.to_lowercase();
    let hits: Vec<&Cmd> = if low.trim().is_empty() { vec![] } else { cmds.iter().filter(|c| tr(c.label).to_lowercase().contains(low.trim()) || c.label.to_lowercase().contains(low.trim())).take(10).collect() };
    let mut chosen: Option<&'static str> = None;
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { chosen = hits.first().map(|c| c.id); }
    if !hits.is_empty() && (r.has_focus() || r.lost_focus()) {
        egui::Area::new(egui::Id::new("tell-me-results")).order(egui::Order::Foreground).fixed_pos(r.rect.left_bottom() + vec2(0.0, 2.0)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(240.0);
                for c in &hits { if small(ui, c.icon, Some(&tr(c.label)), false, true, c.label).clicked() { chosen = Some(c.id); } }
            });
        });
    }
    if let Some(id) = chosen { q.clear(); run(app, id); }
    app.rv.view.tell_me = q;
}

// ---------------------------------------------------------------------------------------------
// KeyTips

const TAB_TIPS: [(Tab, &str); 7] = [(Tab::Home, "H"), (Tab::Insert, "N"), (Tab::PageLayout, "P"), (Tab::Formulas, "M"), (Tab::Data, "A"), (Tab::Review, "R"), (Tab::View, "W")];

fn letter(k: egui::Key) -> Option<char> { k.symbol_or_name().chars().next().filter(|c| c.is_ascii_alphanumeric()).filter(|_| k.symbol_or_name().len() == 1) }

/// Alt (pressed and released alone) or F10 shows KeyTips; letters then pick a tab and a command.
pub fn keys(app: &mut SheetApp, ctx: &egui::Context) {
    let v = &mut app.rv.view;
    let (alt, any_key, f10) = ctx.input(|i| (i.modifiers.alt, i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, key, .. } if *key != egui::Key::F10) || matches!(e, egui::Event::PointerButton { .. })), i.key_pressed(egui::Key::F10)));
    if alt && !v.alt_down { v.alt_clean = true; }
    if alt && any_key { v.alt_clean = false; }
    let released = !alt && v.alt_down && v.alt_clean;
    v.alt_down = alt;
    if (released || f10) && app.edit.is_none() { v.keytips = if v.keytips.is_some() { None } else { Some('*') }; }
    let Some(level) = app.rv.view.keytips else { return };
    let mut pressed: Option<char> = None;
    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) { pressed = Some('\u{1b}'); }
        for e in std::mem::take(&mut i.events) {
            match &e { egui::Event::Key { key, pressed: true, .. } => { if let Some(c) = letter(*key) { pressed = Some(c.to_ascii_uppercase()); } } egui::Event::Text(_) | egui::Event::Key { .. } => {}, _ => i.events.push(e) }
        }
    });
    let Some(c) = pressed else { return };
    if c == '\u{1b}' { app.rv.view.keytips = if level == '*' { None } else { Some('*') }; return; }
    if level == '*' {
        if let Some((tab, _)) = TAB_TIPS.iter().find(|(_, l)| l.starts_with(c)) { app.tab = *tab; app.rv.view.keytips = if matches!(tab, Tab::Review | Tab::View) { Some(c) } else { None }; return; }
        if let Some(d) = c.to_digit(10) { let qat = crate::review_tab::settings(app).qat.clone(); if let Some(id) = qat.get(d as usize - 1) { run(app, id); } }
        app.rv.view.keytips = None;
        return;
    }
    let tab = if level == 'R' { Tab::Review } else { Tab::View };
    app.rv.view.keytips = None;
    if let Some(cmd) = commands().into_iter().find(|x| x.tip.is_some_and(|(t, l)| t == tab && l.starts_with(c))) { (cmd.run)(app); }
}

fn keytips_overlay(app: &SheetApp, ctx: &egui::Context) {
    let Some(level) = app.rv.view.keytips else { return };
    let items: Vec<(String, String)> = if level == '*' {
        let mut v: Vec<(String, String)> = TAB_TIPS.iter().map(|(t, l)| (l.to_string(), tr(match t { Tab::Home => "Home", Tab::Insert => "Insert", Tab::PageLayout => "Page Layout", Tab::Formulas => "Formulas", Tab::Data => "Data", Tab::Review => "Review", _ => "View" }))).collect();
        let cmds = commands();
        for (k, id) in app.rv.settings.as_ref().map(|s| s.qat.clone()).unwrap_or_default().iter().enumerate().take(9) { if let Some(c) = cmds.iter().find(|c| c.id == id) { v.push(((k + 1).to_string(), tr(c.label))); } }
        v
    } else {
        let tab = if level == 'R' { Tab::Review } else { Tab::View };
        commands().into_iter().filter_map(|c| c.tip.filter(|(t, _)| *t == tab).map(|(_, l)| (l.to_string(), tr(c.label)))).collect()
    };
    egui::Area::new(egui::Id::new("keytips")).order(egui::Order::Foreground).fixed_pos(pos2(8.0, 40.0)).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(720.0);
                for (k, label) in items {
                    ui.label(RichText::new(format!(" {k} ")).monospace().strong().background_color(Color32::from_rgb(0x30, 0x30, 0x30)).color(Color32::WHITE));
                    ui.label(RichText::new(label).size(12.0));
                    ui.add_space(6.0);
                }
            });
        });
    });
}

// ---------------------------------------------------------------------------------------------
// The tab

pub fn tab(app: &mut SheetApp, ui: &mut Ui) {
    let s = app.sheet();
    crate::page_view::views_group(app, ui);
    group(ui, "Custom Views", |ui| {
        if large(ui, "multiplePages", "Custom Views", app.rv.view.views_dialog.is_some(), true, "Save the zoom, panes, hidden rows and selection as a named view").clicked() { app.rv.view.views_dialog = Some((None, String::new(), true, false)); }
    });
    group(ui, "Show", |ui| {
        rows(ui, |ui| {
            let mut ruler = crate::page_view::ruler_shown();
            if ui.add_enabled(crate::page_view::mode() == crate::page_view::ViewMode::PageLayout, egui::Checkbox::new(&mut ruler, tr("Ruler"))).on_disabled_hover_text(tr("The ruler shows in Page Layout view")).changed() { crate::page_view::set_ruler(ruler); }
            let mut g = app.book.sheets[s].gridlines;
            if crate::theme::check(ui, &mut g, tr("Gridlines")).changed() { app.book.sheets[s].gridlines = g; app.dirty = true; }
        });
        ui.add_space(8.0);
        rows(ui, |ui| {
            let mut f = app.prefs.show_formula_bar;
            if crate::theme::check(ui, &mut f, tr("Formula Bar")).changed() { app.prefs.show_formula_bar = f; app.prefs.save(); }
            let mut h = app.book.sheets[s].headers;
            if crate::theme::check(ui, &mut h, tr("Headings")).changed() { app.book.sheets[s].headers = h; app.dirty = true; }
        });
    });
    group(ui, "Zoom", |ui| {
        if large(ui, "zoom", "Zoom", app.rv.view.zoom_dialog.is_some(), true, "Choose a zoom level").clicked() { open_zoom(app); }
        if large(ui, "zoom100", "100%", false, true, "Zoom to 100%").clicked() { app.zoom = 1.0; }
        if large(ui, "zoomSelection", "Zoom to Selection", false, true, "Zoom so the selection fills the window").clicked() { zoom_to_selection(app); }
    });
    group(ui, "Window", |ui| {
        if large(ui, "newWindow", "New Window", app.rv.view.window2.is_some(), true, "Open a second window onto this workbook").clicked() { new_window(app); }
        let r = large_menu(ui, "columns", "Arrange All", "Place the open windows side by side or stacked");
        crate::theme::menu(&r, |ui| {
            for (k, label) in [(0, "Tiled"), (1, "Vertical"), (2, "Horizontal"), (3, "Cascade")] { if ui.button(tr(label)).clicked() { arrange(app, k); ui.close(); } }
        });
        let r = large_menu(ui, "freezePanes", "Freeze Panes", "Keep rows and columns in view while scrolling");
        crate::theme::menu(&r, |ui| {
            let frozen = app.book.sheets[s].frozen_rows > 0 || app.book.sheets[s].frozen_cols > 0;
            if menu_item(ui, "freezePanes", if frozen { "Unfreeze Panes" } else { "Freeze Panes" }) { app.book.sheets[s].review.split = None; app.freeze(0); ui.close(); }
            if menu_item(ui, "headerRow", "Freeze Top Row") { app.book.sheets[s].review.split = None; app.freeze(1); ui.close(); }
            if menu_item(ui, "columnsLeft", "Freeze First Column") { app.book.sheets[s].review.split = None; app.freeze(2); ui.close(); }
        });
        rows(ui, |ui| {
            if small(ui, "split", Some("Split"), app.book.sheets[s].review.split.is_some(), true, "Split the window into panes that scroll separately").clicked() { toggle_split(app); }
            let two = app.rv.view.window2.is_some();
            if small(ui, "hideInk", Some("Hide"), false, true, "Hide the current window").clicked() { hide_window(app, true); }
            if small(ui, "showMarkup", Some("Unhide"), false, true, "Show hidden windows").clicked() { hide_window(app, false); }
            let _ = two;
        });
        rows(ui, |ui| {
            let two = app.rv.view.window2.is_some();
            if small(ui, "sideToSide", Some("View Side by Side"), app.rv.view.side_by_side, two, "Compare the two windows next to each other").clicked() { side_by_side(app); }
            if small(ui, "moveDown", Some("Synchronous Scrolling"), app.rv.view.sync, two && app.rv.view.side_by_side, "Scroll both windows together").clicked() { app.rv.view.sync = !app.rv.view.sync; }
            if small(ui, "resetPicture", Some("Reset Window Position"), false, two, "Make the windows share the screen equally").clicked() { arrange(app, 1); }
        });
        let r = large_menu(ui, "multiplePages", "Switch Windows", "Go to another open window");
        crate::theme::menu(&r, |ui| {
            if ui.button(format!("1 {}", app.title())).clicked() { app.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus); ui.close(); }
            if app.rv.view.window2.is_some() && ui.button(format!("2 {}:2", app.title())).clicked() { app.ctx.send_viewport_cmd_to(egui::ViewportId::from_hash_of(W2), egui::ViewportCommand::Focus); ui.close(); }
        });
        let dark = app.dark;
        if large(ui, "darkPage", "Dark Mode", dark, true, "Switch between light and dark").clicked() { let mut p = app.prefs.clone(); p.dark = !dark; app.apply_prefs(p); }
    });
}

pub fn zoom_to_selection(app: &mut SheetApp) {
    let s = &app.book.sheets[app.sheet()];
    let r = app.sel.range();
    let cols = crate::grid::Axis::of_cols(s); let rows = crate::grid::Axis::of_rows(s);
    let w = cols.pos(r.c1.min(r.c0 + 200) + 1) - cols.pos(r.c0); let h = rows.pos(r.r1.min(r.r0 + 1000) + 1) - rows.pos(r.r0);
    let avail = if app.grid_rect.width() > 10.0 { app.grid_rect.size() - vec2(60.0, 40.0) } else { app.ctx.content_rect().size() - vec2(80.0, 260.0) };
    app.zoom = (avail.x / w).min(avail.y / h).clamp(0.1, 4.0);
    app.scroll = vec2(cols.pos(r.c0) - cols.pos(s.frozen_cols), rows.pos(r.r0) - rows.pos(s.frozen_rows)).max(egui::Vec2::ZERO);
}

pub fn open_zoom(app: &mut SheetApp) { let z = (app.zoom * 100.0).round() as u32; app.rv.view.zoom_dialog = Some((z, z.to_string())); }

fn zoom_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some((mut pick, mut custom)) = app.rv.view.zoom_dialog.take() else { return };
    let mut open = true; let mut keep = true;
    crate::theme::dialog(ctx, "Zoom").open(&mut open).show(ctx, |ui| {
        ui.set_width(220.0);
        ui.label(tr("Magnification"));
        for z in [200, 100, 75, 50, 25] { crate::theme::radio_value(ui, &mut pick, z, format!("{z}%")); }
        crate::theme::radio_value(ui, &mut pick, 0, tr("Fit selection"));
        crate::theme::hrow(ui, |ui| { crate::theme::radio_value(ui, &mut pick, 1, tr("Custom:")); if ui.add(egui::TextEdit::singleline(&mut custom).desired_width(50.0)).changed() { pick = 1; } ui.label("%"); });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                match pick { 0 => zoom_to_selection(app), 1 => if let Ok(z) = custom.trim().parse::<f32>() { app.zoom = (z / 100.0).clamp(0.1, 4.0); }, z => app.zoom = z as f32 / 100.0 }
                keep = false;
            }
            if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
        });
    });
    if open && keep { app.rv.view.zoom_dialog = Some((pick, custom)); }
}

fn views_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some((mut picked, mut name, mut hidden, mut adding)) = app.rv.view.views_dialog.take() else { return };
    let mut open = true; let mut keep = true;
    crate::theme::dialog(ctx, if adding { "Add View" } else { "Custom Views" }).open(&mut open).show(ctx, |ui| {
        ui.set_width(340.0);
        if adding {
            crate::theme::hrow(ui, |ui| { ui.label(tr("Name:")); ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0)).request_focus(); });
            ui.label(tr("Include in view"));
            crate::theme::check(ui, &mut hidden, tr("Hidden rows, columns and filter settings"));
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary_enabled(ui, !name.trim().is_empty(), "OK").clicked() {
                    app.commit(None);
                    let a = app.book.active;
                    app.book.sheets[a].selection = Some(app.sel.range());
                    app.book.sheets[a].zoom = (app.zoom * 100.0).round() as u32;
                    let v = CustomView::capture(&app.book, name.trim(), hidden);
                    app.book.review.views.retain(|x| !x.name.eq_ignore_ascii_case(name.trim()));
                    app.book.review.views.push(v);
                    app.dirty = true; adding = false; name.clear();
                }
                if crate::theme::secondary(ui, &tr("Cancel")).clicked() { adding = false; }
            });
            return;
        }
        ui.label(tr("Views:"));
        egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| {
            ui.set_min_size(vec2(320.0, 140.0));
            if app.book.review.views.is_empty() { ui.label(RichText::new(tr("No views yet. Add one to save how the workbook looks now.")).color(c_ink3())); }
            for (i, v) in app.book.review.views.iter().enumerate() {
                let r = ui.selectable_label(picked == Some(i), &v.name);
                if r.clicked() { picked = Some(i); }
                if r.double_clicked() { picked = Some(i); }
            }
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary_enabled(ui, picked.is_some(), "Show").clicked() {
                if let Some(v) = picked.and_then(|i| app.book.review.views.get(i)).cloned() {
                    app.checkpoint("Show View");
                    v.apply(&mut app.book);
                    let a = app.book.active;
                    app.sel = app.book.sheets[a].selection.map(|r| crate::app::Selection { active: r.start(), anchor: r.start(), extra: vec![] }).unwrap_or_default();
                    app.zoom = app.book.sheets[a].zoom as f32 / 100.0;
                    app.scroll = egui::Vec2::ZERO; app.reveal = true; app.dirty = true;
                }
                keep = false;
            }
            if crate::theme::secondary(ui, &tr("Close")).clicked() { keep = false; }
            if crate::theme::secondary(ui, &tr("Delete")).clicked() { if let Some(i) = picked.take() { if i < app.book.review.views.len() { app.book.review.views.remove(i); app.dirty = true; } } }
            if crate::theme::secondary(ui, &tr("Add…")).clicked() { adding = true; }
        });
    });
    if open && keep { app.rv.view.views_dialog = Some((picked, name, hidden, adding)); }
}

fn open_autocorrect(app: &mut SheetApp) {
    let list = crate::review_tab::settings(app).autocorrect.clone();
    app.rv.view.autocorrect_dialog = Some((list, String::new(), String::new()));
}

fn autocorrect_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some((mut list, mut a, mut b)) = app.rv.view.autocorrect_dialog.take() else { return };
    let mut open = true; let mut keep = true;
    let (mut on, mut ac) = { let s = crate::review_tab::settings(app); (s.autocorrect_on, s.autocomplete) };
    crate::theme::dialog(ctx, "AutoCorrect").open(&mut open).show(ctx, |ui| {
        ui.set_width(380.0);
        crate::theme::check(ui, &mut on, tr("Replace text as you type"));
        crate::theme::check(ui, &mut ac, tr("Enable AutoComplete for cell values"));
        crate::theme::hrow(ui, |ui| { ui.label(tr("Replace:")); ui.add(egui::TextEdit::singleline(&mut a).desired_width(100.0)); ui.label(tr("With:")); ui.add(egui::TextEdit::singleline(&mut b).desired_width(120.0));
            if ui.button(tr("Add")).clicked() && !a.is_empty() { list.retain(|(x, _)| *x != a); list.push((a.clone(), b.clone())); a.clear(); b.clear(); } });
        egui::ScrollArea::vertical().max_height(220.0).id_salt("ac-list").show(ui, |ui| {
            let mut del = None;
            egui::Grid::new("ac-grid").num_columns(3).show(ui, |ui| {
                for (i, (x, y)) in list.iter().enumerate() { if ui.selectable_label(false, x).clicked() { a = x.clone(); b = y.clone(); } ui.label(y); if ui.small_button("✕").clicked() { del = Some(i); } ui.end_row(); }
            });
            if let Some(i) = del { list.remove(i); }
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { let s = crate::review_tab::settings(app); s.autocorrect = list.clone(); s.save(); keep = false; }
            if crate::theme::secondary(ui, &tr("Reset")).clicked() { list = crate::review_tab::default_autocorrect(); }
            if crate::theme::secondary(ui, &tr("Cancel")).clicked() { keep = false; }
        });
    });
    { let s = crate::review_tab::settings(app); if s.autocorrect_on != on || s.autocomplete != ac { s.autocorrect_on = on; s.autocomplete = ac; s.save(); } }
    if open && keep { app.rv.view.autocorrect_dialog = Some((list, a, b)); }
}

// ---------------------------------------------------------------------------------------------
// Windows

pub fn new_window(app: &mut SheetApp) {
    if app.rv.view.window2.is_some() { app.ctx.send_viewport_cmd_to(egui::ViewportId::from_hash_of(W2), egui::ViewportCommand::Focus); return; }
    app.rv.view.window2 = Some(Win2 { sheet: app.sheet(), sel: app.sel.clone(), scroll: app.scroll, zoom: app.zoom, grid_rect: Rect::NOTHING });
}

fn screen(app: &SheetApp) -> egui::Vec2 { app.ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1440.0, 900.0)) }

/// Arranges the windows: 0 tiled, 1 vertical (side by side), 2 horizontal (stacked), 3 cascade.
pub fn arrange(app: &mut SheetApp, how: u8) {
    if app.rv.view.window2.is_none() { app.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Maximized(true)); return; }
    let s = screen(app) - vec2(0.0, 40.0);
    let (a, b, size) = match how {
        2 => (pos2(0.0, 25.0), pos2(0.0, 25.0 + s.y / 2.0), vec2(s.x, s.y / 2.0 - 30.0)),
        3 => (pos2(20.0, 30.0), pos2(80.0, 90.0), s * 0.75),
        _ => (pos2(0.0, 25.0), pos2(s.x / 2.0, 25.0), vec2(s.x / 2.0, s.y - 30.0)),
    };
    for (id, p) in [(egui::ViewportId::ROOT, a), (egui::ViewportId::from_hash_of(W2), b)] {
        app.ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Maximized(false));
        app.ctx.send_viewport_cmd_to(id, egui::ViewportCommand::OuterPosition(p));
        app.ctx.send_viewport_cmd_to(id, egui::ViewportCommand::InnerSize(size));
    }
}

pub fn side_by_side(app: &mut SheetApp) {
    if app.rv.view.window2.is_none() { new_window(app); }
    app.rv.view.side_by_side = !app.rv.view.side_by_side;
    if app.rv.view.side_by_side { app.rv.view.sync = true; arrange(app, 1); }
}

fn hide_window(app: &mut SheetApp, hide: bool) {
    let id = if app.rv.view.window2.is_some() && hide { egui::ViewportId::from_hash_of(W2) } else { egui::ViewportId::ROOT };
    if hide { app.ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Minimized(true)); }
    else { for id in [egui::ViewportId::ROOT, egui::ViewportId::from_hash_of(W2)] { app.ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Minimized(false)); } }
}

/// Swaps the second window's view (sheet, selection, scroll, zoom) with the app's.
fn swap(app: &mut SheetApp, w: &mut Win2) {
    std::mem::swap(&mut app.book.active, &mut w.sheet);
    std::mem::swap(&mut app.sel, &mut w.sel);
    std::mem::swap(&mut app.scroll, &mut w.scroll);
    std::mem::swap(&mut app.zoom, &mut w.zoom);
    std::mem::swap(&mut app.grid_rect, &mut w.grid_rect);
}

fn window2(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut w) = app.rv.view.window2.take() else { return };
    if w.sheet >= app.book.sheets.len() { w.sheet = 0; }
    // Synchronous scrolling follows the main window's moves.
    if app.rv.view.sync && app.rv.view.side_by_side { let d = app.scroll - app.rv.view.last_scroll; w.scroll = (w.scroll + d).max(egui::Vec2::ZERO); }
    app.rv.view.last_scroll = app.scroll;
    let title = format!("{}:2", app.title());
    let mut close = false;
    let builder = egui::ViewportBuilder::default().with_title(title).with_inner_size([900.0, 640.0]);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(W2), builder, |ui, _class| {
        if ui.ctx().input(|i| i.viewport().close_requested()) { close = true; }
        let main_scroll = app.scroll;
        swap(app, &mut w);
        let before = app.scroll;
        egui::Panel::top("w2-bar").frame(egui::Frame::NONE.fill(c_frame()).inner_margin(6)).show(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                for i in 0..app.book.sheets.len() {
                    if app.book.sheets[i].hidden { continue; }
                    if ui.selectable_label(app.book.active == i, &app.book.sheets[i].name).clicked() { app.book.active = i; app.sel = Default::default(); app.scroll = egui::Vec2::ZERO; }
                }
                ui.with_layout(far_layout(), |ui| { let mut z = app.zoom * 100.0; if ui.add(egui::Slider::new(&mut z, 10.0..=400.0).logarithmic(true).suffix("%")).changed() { app.zoom = z / 100.0; } });
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(c_card())).show(ui, |ui| {
            crate::grid::shortcuts(app, ui.ctx());
            crate::grid::grid(app, ui);
        });
        let moved = app.scroll - before;
        swap(app, &mut w);
        if app.rv.view.sync && app.rv.view.side_by_side && moved != egui::Vec2::ZERO { app.scroll = (main_scroll + moved).max(egui::Vec2::ZERO); app.rv.view.last_scroll = app.scroll; }
    });
    if !close { app.rv.view.window2 = Some(w); } else { app.rv.view.side_by_side = false; }
}

// ---------------------------------------------------------------------------------------------
// Split panes

pub fn toggle_split(app: &mut SheetApp) {
    let si = app.sheet();
    if app.book.sheets[si].review.split.is_some() { app.book.sheets[si].review.split = None; app.dirty = true; return; }
    let s = &app.book.sheets[si];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    let a = app.sel.active;
    let mut x = cols.pos(a.col) - app.scroll.x;
    let mut y = rows.pos(a.row) - app.scroll.y;
    let view = app.grid_rect.size() / app.zoom;
    if !(x > 0.0 && x < view.x - 40.0) && !(y > 0.0 && y < view.y - 40.0) || a == sheet_engine::Pos::new(0, 0) { x = (view.x / 2.0).max(100.0); y = (view.y / 2.0).max(60.0); }
    let x = if x > 0.0 && x < view.x { x } else { 0.0 };
    let y = if y > 0.0 && y < view.y { y } else { 0.0 };
    let sh = &mut app.book.sheets[si];
    sh.frozen_rows = 0; sh.frozen_cols = 0;
    sh.review.split = Some((x, y));
    app.rv.view.split_scroll = egui::Vec2::ZERO;
    app.rv.view.active_pane = 3;
    app.dirty = true;
}

/// The grid, or four panes when the sheet is split.
pub fn central(app: &mut SheetApp, ui: &mut Ui) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    let split = s.review.split.filter(|_| s.frozen_rows == 0 && s.frozen_cols == 0 && crate::page_view::mode() == crate::page_view::ViewMode::Normal);
    let Some((sx, sy)) = split else { crate::grid::grid(app, ui); return; };
    let full = ui.available_rect_before_wrap();
    let lay = crate::grid::layout(app, full);
    let bx = if sx > 0.0 { (full.min.x + lay.head_w + sx * app.zoom).clamp(full.min.x + 40.0, full.max.x - 40.0) } else { full.min.x };
    let by = if sy > 0.0 { (full.min.y + lay.head_h + sy * app.zoom).clamp(full.min.y + 30.0, full.max.y - 30.0) } else { full.min.y };
    // Panes: 0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right (scrolls as the app does).
    let rects = [Rect::from_min_max(full.min, pos2(bx, by)), Rect::from_min_max(pos2(bx, full.min.y), pos2(full.max.x, by)), Rect::from_min_max(pos2(full.min.x, by), pos2(bx, full.max.y)), Rect::from_min_max(pos2(bx, by), full.max)];
    if let Some(p) = ui.input(|i| if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None }) { if let Some(k) = rects.iter().position(|r| r.contains(p) && r.width() > 1.0 && r.height() > 1.0) { app.rv.view.active_pane = k; } }
    let main = app.scroll;
    let tl = app.rv.view.split_scroll;
    // Each pane's scroll: columns shared down, rows shared across.
    let mut sc = [tl, vec2(main.x, tl.y), vec2(tl.x, main.y), main];
    let active = app.rv.view.active_pane;
    let reveal = app.reveal;
    for k in 0..4 {
        let r = rects[k];
        if r.width() < 2.0 || r.height() < 2.0 { continue; }
        app.scroll = sc[k];
        let edit = if k != active { app.edit.take() } else { None };
        app.reveal = reveal && k == active;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r).id_salt(("split-pane", k)));
        crate::grid::grid(app, &mut child);
        if k != active { if let Some(e) = edit { app.edit = Some(e); } }
        let d = app.scroll - sc[k];
        if d != egui::Vec2::ZERO {
            let (col_partner, row_partner) = match k { 0 => (2, 1), 1 => (3, 0), 2 => (0, 3), _ => (1, 2) };
            sc[k] = app.scroll;
            sc[col_partner].x = app.scroll.x;
            sc[row_partner].y = app.scroll.y;
        }
    }
    app.reveal = false;
    app.scroll = sc[3];
    app.rv.view.split_scroll = sc[0];
    app.grid_rect = rects[active.min(3)];
    // The split bars, dragged to move, double-clicked or dragged off the edge to remove.
    let bar = Color32::from_gray(if app.dark { 90 } else { 200 });
    for (vertical, on) in [(true, sx > 0.0), (false, sy > 0.0)] {
        if !on { continue; }
        let rect = if vertical { Rect::from_center_size(pos2(bx, full.center().y), vec2(6.0, full.height())) } else { Rect::from_center_size(pos2(full.center().x, by), vec2(full.width(), 6.0)) };
        let resp = ui.interact(rect, egui::Id::new(("split-bar", vertical)), Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, bar);
        if resp.hovered() || resp.dragged() { ui.ctx().set_cursor_icon(if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical }); }
        let sh = &mut app.book.sheets[si];
        let Some((x, y)) = sh.review.split.as_mut() else { continue };
        if resp.dragged() { let d = resp.drag_delta() / app.zoom; if vertical { *x = (*x + d.x).max(0.0); } else { *y = (*y + d.y).max(0.0); } }
        if resp.drag_stopped() || resp.double_clicked() {
            let gone = resp.double_clicked() || if vertical { bx <= full.min.x + lay.head_w + 12.0 || bx >= full.max.x - 45.0 } else { by <= full.min.y + lay.head_h + 8.0 || by >= full.max.y - 35.0 };
            if gone { if vertical { *x = 0.0; } else { *y = 0.0; } }
            if *x <= 0.0 && *y <= 0.0 { sh.review.split = None; }
            app.dirty = true;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The status bar

/// The status bar's right side: statistics, view shortcuts, zoom slider and zoom, as chosen
/// from its right-click menu.
pub fn status_items(app: &mut SheetApp, ui: &mut Ui) {
    let on = crate::review_tab::settings(app).status;
    let label = |ui: &mut Ui, t: String| ui.add(egui::Label::new(RichText::new(t).size(12.0).color(c_ink2())).sense(Sense::click()));
    let mut menu_resps = vec![];
    if on[8] { let r = label(ui, format!("{:.0}%", app.zoom * 100.0)).on_hover_text(tr("Zoom level")); if r.clicked() { open_zoom(app); } menu_resps.push(r); }
    if on[7] {
        let mut z = app.zoom * 100.0;
        if ui.add(egui::Slider::new(&mut z, 10.0..=400.0).show_value(false).logarithmic(true)).changed() { app.zoom = (z / 100.0).clamp(0.1, 4.0); }
    }
    if on[6] {
        use crate::page_view::ViewMode as M;
        let m = crate::page_view::mode();
        for (mode, icon, tip) in [(M::PageBreak, "pageBreakPreview", "Switch to Page Break Preview"), (M::PageLayout, "printLayout", "Switch to Page Layout"), (M::Normal, "table", "Switch to Normal view")] {
            if small(ui, icon, None, m == mode, true, tip).clicked() { crate::page_view::set_mode(app, mode); }
        }
    }
    ui.add_space(12.0);
    let st = app.book.selection_stats(app.sheet(), &app.sel.ranges());
    if st.count > 1 {
        let fmt = app.style().num_fmt;
        let f = |x: f64| sheet_engine::numfmt::format_number(x, &fmt, app.book.date1904).text;
        let items: [(usize, &str, Option<String>); 6] = [
            (5, "Sum", (st.nums > 0).then(|| f(st.sum))), (4, "Maximum", (st.nums > 0).then(|| f(st.max))), (3, "Minimum", (st.nums > 0).then(|| f(st.min))),
            (2, "Numerical Count", Some(st.nums.to_string())), (1, "Count", Some(st.count.to_string())), (0, "Average", (st.nums > 0).then(|| f(st.sum / st.nums as f64))),
        ];
        for (k, name, v) in items { if let (true, Some(v)) = (on[k], v) { let r = label(ui, format!("{}: {}", tr(name), v)); if r.clicked() { ui.ctx().copy_text(v); app.notify(&tr("Copied")); } menu_resps.push(r); } }
    }
    let filler = ui.allocate_response(vec2(ui.available_width().max(0.0), 16.0), Sense::click());
    menu_resps.push(filler);
    for r in menu_resps { r.context_menu(|ui| status_menu(app, ui)); }
}

fn status_menu(app: &mut SheetApp, ui: &mut Ui) {
    menu_heading(ui, &tr("Customize Status Bar"));
    let st = app.book.selection_stats(app.sheet(), &app.sel.ranges());
    for (i, name) in crate::review_tab::STATUS_ITEMS.iter().enumerate() {
        let s = crate::review_tab::settings(app);
        let mut v = s.status[i];
        let value = match i { 0 if st.nums > 0 => format!("  {}", st.sum / st.nums as f64), 1 => format!("  {}", st.count), 2 => format!("  {}", st.nums), 3 if st.nums > 0 => format!("  {}", st.min), 4 if st.nums > 0 => format!("  {}", st.max), 5 => format!("  {}", st.sum), _ => String::new() };
        if crate::theme::check(ui, &mut v, format!("{}{}", tr(name), value)).changed() { s.status[i] = v; s.save(); }
    }
}

// ---------------------------------------------------------------------------------------------

/// Each frame: KeyTips, the second window and the View tab's dialogs.
pub fn frame(app: &mut SheetApp, ctx: &egui::Context) {
    keytips_overlay(app, ctx);
    zoom_dialog(app, ctx);
    views_dialog(app, ctx);
    autocorrect_dialog(app, ctx);
    window2(app, ctx);
}
