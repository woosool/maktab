//! The app driven headlessly (egui_kittest): it starts, draws a board, and takes pointer and
//! keyboard input. Snapshots go to SABOURA_SNAPSHOTS when set.

use crate::app::{BoardApp, Tool};
use egui::{Event, PointerButton, Pos2, pos2};
use egui_kittest::Harness;

fn harness(path: Option<std::path::PathBuf>) -> Harness<'static, BoardApp> {
    Harness::builder().with_size(egui::vec2(1300.0, 820.0)).wgpu().build_eframe(move |cc| BoardApp::new(cc, path.clone()))
}

fn save(h: &mut Harness<'static, BoardApp>, name: &str) {
    if let Ok(dir) = std::env::var("SABOURA_SNAPSHOTS") && let Ok(img) = h.render() { let _ = img.save(format!("{dir}/{name}.png")); }
}

fn drag(h: &mut Harness<'static, BoardApp>, from: Pos2, to: Pos2) {
    let press = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    h.input_mut().events.push(Event::PointerMoved(from));
    h.run_steps(1);
    h.input_mut().events.push(press(from, true));
    h.run_steps(1);
    for k in 1..=8 {
        h.input_mut().events.push(Event::PointerMoved(from + (to - from) * (k as f32 / 8.0)));
        h.run_steps(1);
    }
    h.input_mut().events.push(press(to, false));
    h.run_steps(2);
}

const SAMPLE: &str = r##"{"version":1,"id":"s","title":"Sample","updated":0,"favorite":false,"items":[
{"id":"f","type":"frame","x":0,"y":0,"w":900,"h":520,"text":"Plan","color":"#dce5f7"},
{"id":"n1","type":"note","x":40,"y":80,"w":190,"h":160,"text":"What if we tried something completely different this time?","color":"#f6e7a4","autoFontSize":true},
{"id":"n2","type":"note","x":260,"y":80,"w":190,"h":160,"text":"فكرة جديدة","color":"#d9e9d2"},
{"id":"r","type":"rect","x":520,"y":80,"w":160,"h":100,"text":"Decide","shape":"diamond","color":"#eadcf0"},
{"id":"e","type":"ellipse","x":700,"y":300,"w":160,"h":100,"text":"Done","color":"#f3d6c8"},
{"id":"a","type":"arrow","x":0,"y":0,"w":1,"h":1,"startId":"r","endId":"e","lineType":"curved","text":"yes"},
{"id":"t","type":"text","x":40,"y":300,"w":400,"h":60,"text":"Big heading","fontSize":36},
{"id":"p","type":"pen","x":100,"y":400,"w":200,"h":80,"points":[[0,0],[50,60],[100,10],[150,70],[200,0]],"color":"#d03a3a"},
{"id":"s5","type":"rect","x":480,"y":330,"w":120,"h":120,"shape":"star","color":"#f6e7a4","rotation":15}]}"##;

#[test]
fn opens_and_draws_a_board() {
    let path = std::env::temp_dir().join("saboura-sample.json");
    std::fs::write(&path, SAMPLE).unwrap();
    let mut h = harness(Some(path));
    h.run_steps(4);
    assert_eq!(h.state().board.items.len(), 9);
    assert_eq!(h.state().board.title, "Sample");
    save(&mut h, "board");
    h.state_mut().set_dark(true);
    h.state_mut().selection = vec!["r".into()];
    h.run_steps(3);
    save(&mut h, "board-dark");
}

fn click_at(h: &mut Harness<'static, BoardApp>, p: Pos2) {
    let press = |pressed| Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    h.input_mut().events.push(Event::PointerMoved(p));
    h.run_steps(1);
    h.input_mut().events.push(press(true));
    h.run_steps(1);
    h.input_mut().events.push(press(false));
    h.run_steps(2);
}

/// The screen point of a board point.
fn screen(h: &Harness<'static, BoardApp>, p: (f64, f64)) -> Pos2 {
    let a = h.state();
    let s = a.view.to_screen(p);
    a.canvas_rect.min + egui::vec2(s.0 as f32, s.1 as f32)
}

fn new_rect(h: &mut Harness<'static, BoardApp>, from: Pos2, to: Pos2) -> String {
    h.state_mut().tool = Tool::Shape;
    drag(h, from, to);
    h.state_mut().stop_editing();
    h.run_steps(1);
    h.state().board.items.last().unwrap().id.clone()
}

#[test]
fn creates_moves_and_undoes() {
    let mut h = harness(None);
    h.run_steps(3);
    let id = new_rect(&mut h, pos2(400.0, 300.0), pos2(600.0, 420.0));
    assert_eq!(h.state().board.items.len(), 1);
    assert_eq!(h.state().tool, Tool::Select);
    let (w, x) = { let i = h.state().board.find(&id).unwrap(); (i.w, i.x) };
    assert!((w * h.state().view.z - 200.0).abs() < 2.0, "width {w}");
    drag(&mut h, pos2(500.0, 360.0), pos2(560.0, 360.0));
    let moved = h.state().board.items[0].x;
    assert!((moved - x) * h.state().view.z > 50.0, "{x} -> {moved}");
    h.state_mut().undo();
    assert_eq!(h.state().board.items[0].x, x);
    // A click with a tool places an object at its default size.
    h.state_mut().tool = Tool::Note;
    click_at(&mut h, pos2(900.0, 600.0));
    assert_eq!(h.state().board.items.last().unwrap().kind, "note");
    h.state_mut().stop_editing();
    // A pen stroke.
    h.state_mut().tool = Tool::Pen;
    drag(&mut h, pos2(200.0, 600.0), pos2(400.0, 650.0));
    assert_eq!(h.state().board.items.last().unwrap().kind, "pen");
    h.state_mut().tool = Tool::Select;
    drag(&mut h, pos2(120.0, 200.0), pos2(1200.0, 790.0));
    assert_eq!(h.state().selection.len(), 3);
    h.state_mut().delete_selection();
    assert!(h.state().board.items.is_empty());
}

#[test]
fn resizes_and_rotates() {
    let mut h = harness(None);
    h.run_steps(3);
    let id = new_rect(&mut h, pos2(400.0, 300.0), pos2(600.0, 420.0));
    let b = h.state().board.find(&id).unwrap().clone();
    // Each side and corner handle.
    let se = screen(&h, (b.x + b.w, b.y + b.h));
    drag(&mut h, se, se + egui::vec2(100.0, 50.0));
    let a = h.state().board.find(&id).unwrap().clone();
    assert!(a.w > b.w + 50.0 && a.h > b.h + 25.0, "{} {}", a.w, a.h);
    let w = screen(&h, (a.x, a.y + a.h / 2.0));
    drag(&mut h, w, w - egui::vec2(60.0, 0.0));
    let c = h.state().board.find(&id).unwrap().clone();
    assert!(c.x < a.x - 30.0 && (c.x + c.w - (a.x + a.w)).abs() < 1.0);
    let n = screen(&h, (c.x + c.w / 2.0, c.y));
    drag(&mut h, n, n - egui::vec2(0.0, 40.0));
    assert!(h.state().board.find(&id).unwrap().h > c.h + 20.0);
    // The rotation handle sits below the bottom-left corner.
    let d = h.state().board.find(&id).unwrap().clone();
    let handle = screen(&h, (d.x, d.y + d.h)) + egui::vec2(-18.0, 18.0);
    let center = screen(&h, (d.x + d.w / 2.0, d.y + d.h / 2.0));
    drag(&mut h, handle, center + egui::vec2(0.0, 200.0));
    let r = h.state().board.find(&id).unwrap().rotation.unwrap_or(0.0);
    assert!(r.abs() > 5.0, "rotation {r}");
    save(&mut h, "rotated");
}

#[test]
fn connects_with_ports_and_line_ends() {
    let mut h = harness(None);
    h.run_steps(3);
    let a = new_rect(&mut h, pos2(200.0, 300.0), pos2(360.0, 400.0));
    let b = new_rect(&mut h, pos2(700.0, 500.0), pos2(860.0, 600.0));
    // Clicking the right port of A adds a joined copy.
    h.state_mut().selection = vec![a.clone()];
    h.run_steps(2);
    let ai = h.state().board.find(&a).unwrap().clone();
    let port = screen(&h, (ai.x + ai.w, ai.y + ai.h / 2.0)) + egui::vec2(22.0, 0.0);
    click_at(&mut h, port);
    assert_eq!(h.state().board.items.len(), 4, "a copy and a line");
    h.state_mut().stop_editing();
    // Dragging A's bottom port onto B joins them.
    h.state_mut().selection = vec![a.clone()];
    h.run_steps(2);
    let bottom = screen(&h, (ai.x + ai.w / 2.0, ai.y + ai.h)) + egui::vec2(0.0, 22.0);
    drag(&mut h, bottom, pos2(780.0, 550.0));
    let line = h.state().board.items.iter().find(|i| i.is_connector() && i.end_id.as_deref() == Some(&b)).cloned();
    let line = line.expect("line from A to B");
    assert_eq!(line.start_id.as_deref(), Some(a.as_str()));
    // Its far end dragged off B detaches, then back onto the copy re-attaches.
    h.state_mut().selection = vec![line.id.clone()];
    h.run_steps(2);
    let l = h.state().board.find(&line.id).unwrap().clone();
    let end = screen(&h, (l.x + l.w, l.y + l.h));
    drag(&mut h, end, pos2(1100.0, 200.0));
    assert!(h.state().board.find(&line.id).unwrap().end_id.is_none());
    let copy = h.state().board.items.iter().find(|i| i.kind == "rect" && i.id != a && i.id != b).unwrap().clone();
    let l = h.state().board.find(&line.id).unwrap().clone();
    let (from, to) = (screen(&h, (l.x + l.w, l.y + l.h)), screen(&h, (copy.x + copy.w / 2.0, copy.y + copy.h / 2.0)));
    drag(&mut h, from, to);
    assert_eq!(h.state().board.find(&line.id).unwrap().end_id.as_deref(), Some(copy.id.as_str()));
    // Moving B keeps the other line attached (routed to a side's middle).
    save(&mut h, "connected");
}

#[test]
fn tables_and_mind_maps() {
    let mut h = harness(None);
    h.run_steps(3);
    h.state_mut().insert_table(2, 3);
    h.state_mut().new_mind_map();
    h.state_mut().add_branch(false);
    h.state_mut().add_branch(true);
    h.run_steps(3);
    let kinds: Vec<String> = h.state().board.items.iter().map(|i| i.kind.clone()).collect();
    assert_eq!(kinds, ["table", "rect", "rect", "rect"]);
    save(&mut h, "table-mind");
}

#[test]
fn every_menu_command_runs() {
    let path = std::env::temp_dir().join("saboura-sample-2.json");
    std::fs::write(&path, SAMPLE).unwrap();
    let mut h = harness(Some(path));
    h.run_steps(3);
    // Commands that open system file dialogs or quit are left out.
    let skip = ["check-updates", "open", "save", "save-as", "export-png", "export-svg", "export-pdf", "export-pptx", "export-xlsx", "export-json", "import-miro", "import-image", "import-text", "image", "quit", "paste", "present", "lang-ar", "lang-fr", "new", "home", "recent"];
    for (_, entries) in crate::commands::MENUS {
        for (id, _, _) in entries.iter() {
            if *id == "-" || skip.contains(id) { continue; }
            h.state_mut().select_all();
            crate::commands::run(h.state_mut(), id);
            h.run_steps(2);
            h.state_mut().panel = crate::app::Panel::None;
        }
    }
    save(&mut h, "after-commands");
}

#[test]
fn exports_svg_png_pdf() {
    let board = saboura_engine::Board::from_json(SAMPLE).unwrap();
    let svg = crate::export::render(&board.items, "svg", false).unwrap();
    assert!(String::from_utf8_lossy(&svg).contains("Decide"));
    let png = crate::export::render(&board.items, "png", false).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    let pdf = crate::export::render(&board.items, "pdf", false).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    if let Ok(dir) = std::env::var("SABOURA_SNAPSHOTS") { let _ = std::fs::write(format!("{dir}/export.png"), &png); }
}

#[test]
fn snaps_comments_search_present() {
    let mut h = harness(None);
    h.run_steps(3);
    let a = new_rect(&mut h, pos2(300.0, 300.0), pos2(460.0, 400.0));
    let b = new_rect(&mut h, pos2(700.0, 500.0), pos2(860.0, 600.0));
    // Dragging B so its top is 3 px from A's snaps them level.
    let (ay, by) = (h.state().board.find(&a).unwrap().y, h.state().board.find(&b).unwrap().y);
    let dy = ((ay - by) * h.state().view.z) as f32 + 3.0;
    drag(&mut h, pos2(780.0, 550.0), pos2(780.0, 550.0 + dy));
    assert!((h.state().board.find(&b).unwrap().y - ay).abs() < 1e-6);
    // A comment placed on A: written in the panel, pinned to A.
    h.state_mut().tool = Tool::Comment;
    click_at(&mut h, pos2(380.0, 350.0));
    let (id, _) = h.state().reply.clone().expect("a comment being written");
    assert!(id.starts_with("new:"));
    h.state_mut().reply = Some((id, "Looks good".into()));
    h.run_steps(2);
    save(&mut h, "comment");
    // Searching finds text.
    h.state_mut().selection = vec![a.clone()];
    h.state_mut().edit_selected(|i| i.text = "Find me".into());
    assert_eq!(crate::panels::search_matches(h.state(), "find"), vec![a.clone()]);
    // Frames present one by one.
    h.state_mut().selection.clear();
    h.state_mut().tool = Tool::Frame;
    drag(&mut h, pos2(150.0, 230.0), pos2(950.0, 700.0));
    let kinds: Vec<String> = h.state().board.items.iter().map(|i| i.kind.clone()).collect();
    assert!(kinds.contains(&"frame".to_string()), "{kinds:?} tool {:?}", h.state().tool);
    crate::panels::start_presenting(h.state_mut());
    h.run_steps(2);
    assert_eq!(h.state().presenting, Some(0));
    save(&mut h, "present");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().presenting, None);
    h.state_mut().timer_shown = true;
    h.run_steps(2);
    save(&mut h, "timer");
}

/// Every dialog and panel in the Arabic interface (right to left), where egui's own grid
/// would panic.
#[test]
fn arabic_interface_dialogs() {
    let path = std::env::temp_dir().join("saboura-sample-3.json");
    std::fs::write(&path, SAMPLE).unwrap();
    let mut h = harness(Some(path));
    crate::i18n::set(crate::i18n::Lang::Ar);
    h.run_steps(2);
    use crate::app::{ImageOptions, Panel, Side};
    let panels = [Panel::Library, Panel::Home, Panel::Shortcuts, Panel::About, Panel::Tags, Panel::Templates(String::new(), String::new()),
        Panel::Icons(String::new(), String::new(), 0), Panel::Palette(String::new()), Panel::Link(String::new()), Panel::Shapes, Panel::Table(3, 3),
        Panel::Image(ImageOptions { alt: String::new(), crop: [0.0, 0.0, 1.0, 1.0], ellipse: false })];
    for p in panels {
        h.state_mut().selection = vec!["r".into()];
        h.state_mut().panel = p;
        h.run_steps(2);
    }
    for side in [Side::Frames, Side::Comments, Side::Votes] { h.state_mut().side = side; h.run_steps(2); }
    h.state_mut().panel = Panel::None;
    h.state_mut().side = Side::Comments;
    h.state_mut().selection = vec!["n2".into()];
    h.run_steps(4);
    save(&mut h, "arabic");
    h.state_mut().side = Side::None;
    h.state_mut().panel = Panel::Templates(String::new(), String::new());
    h.run_steps(4);
    save(&mut h, "templates-ar");
    crate::i18n::set(crate::i18n::Lang::En);
    h.run_steps(3);
    save(&mut h, "templates-en");
}

#[test]
fn rich_words_votes_libraries_exports() {
    let path = std::env::temp_dir().join("saboura-sample-4.json");
    std::fs::write(&path, SAMPLE).unwrap();
    let mut h = harness(Some(path));
    h.run_steps(2);
    // Bold on one word while typing.
    h.state_mut().selection = vec!["n1".into()];
    crate::canvas::start_editing(h.state_mut(), "n1");
    h.run_steps(2);
    h.state_mut().text_sel = Some((0, 4));
    crate::canvas::toggle_flag(h.state_mut(), "bold");
    let runs = saboura_engine::rich::read(h.state().board.find("n1").unwrap().get("textRuns"));
    assert_eq!((runs[0].start, runs[0].end, runs[0].style.get("bold").cloned()), (0, 4, Some(true.into())));
    h.state_mut().stop_editing();
    // A vote on the notes.
    h.state_mut().selection = vec!["n1".into(), "n2".into()];
    h.state_mut().side = crate::app::Side::Votes;
    let mut w = serde_json::Map::new();
    w.insert("vote".into(), serde_json::json!({ "id": "vote-1", "title": "Pick", "votes": 2, "multiple": true, "anonymous": false, "hidden": false, "targets": ["n1", "n2"], "state": "active", "revealed": true, "startedAt": 0, "endsAt": 9_000_000_000_000i64 }));
    h.state_mut().board.extra.insert("workshop".into(), serde_json::Value::Object(w));
    crate::panels::cast(h.state_mut(), "n1", false);
    crate::panels::cast(h.state_mut(), "n1", false);
    crate::panels::cast(h.state_mut(), "n2", false);
    let (all, _) = crate::panels::tally(h.state());
    assert_eq!((all.get("n1"), all.get("n2")), (Some(&2), None), "two votes each");
    h.run_steps(2);
    save(&mut h, "votes");
    // A template and an icon.
    let b = crate::library::template_board("scrum-board").or_else(|| crate::library::template_board("retrospective")).expect("a template");
    assert!(!b.items.is_empty());
    crate::library::add_items(h.state_mut(), b.items);
    // Every export format.
    let items = h.state().board.items.clone();
    for f in ["svg", "png", "pdf", "pdf-image", "pptx", "xlsx"] {
        let bytes = crate::export::render(&items, f, false).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert!(bytes.len() > 100, "{f}");
        if f == "pptx" || f == "xlsx" { assert_eq!(&bytes[..2], b"PK"); }
        if f == "pdf" { assert!(bytes.starts_with(b"%PDF")); }
    }
}


#[test]
fn saves_and_opens_sab() {
    let dir = std::env::temp_dir().join("saboura-sab-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("board.sab");
    let mut h = harness(None);
    h.run_steps(2);
    let png = include_bytes!("../../assets/icon.png");
    h.state_mut().insert_image(png, (0.0, 0.0));
    h.state_mut().path = Some(path.clone());
    assert!(h.state_mut().save(false));
    let bytes = std::fs::read(&path).unwrap();
    assert!(saboura_engine::sab::is_sab(&bytes) && saboura_engine::sab::thumbnail(&bytes).is_some());
    h.state_mut().open_path(path.clone());
    assert_eq!(h.state().board.items.len(), 1);
    assert!(h.state().board.items[0].str("src").unwrap().starts_with("data:image/png;base64,"));
    // Named .json, it saves as plain JSON.
    let json = dir.join("board.json");
    h.state_mut().path = Some(json.clone());
    assert!(h.state_mut().save(false));
    assert!(saboura_engine::Board::from_json(&std::fs::read_to_string(&json).unwrap()).is_ok());
}

/// Dark mode and back: the chrome returns to its light colours.
#[test]
fn dark_then_light() {
    let mut h = harness(None);
    h.run_steps(2);
    let light = h.ctx.global_style().visuals.window_fill;
    h.state_mut().set_dark(true);
    h.run_steps(2);
    assert_ne!(h.ctx.global_style().visuals.window_fill, light);
    h.state_mut().set_dark(false);
    h.run_steps(2);
    assert_eq!(h.ctx.global_style().visuals.window_fill, light);
    save(&mut h, "light-again");
    assert!(crate::update::newer("0.2.0", crate::update::CURRENT));
}

/// README screenshots: a board in English, French and Arabic, with the interface in each.
/// `MAKTAB_SCREENSHOTS=… cargo test -p saboura-app readme_screenshots -- --ignored`
#[test]
#[ignore]
fn readme_screenshots() {
    let Ok(dir) = std::env::var("MAKTAB_SCREENSHOTS") else { return };
    std::fs::create_dir_all(&dir).unwrap();
    let board = r##"{"version":1,"id":"readme","title":"Plan 2027","updated":0,"favorite":false,"items":[
{"id":"f","type":"frame","x":0,"y":0,"w":1180,"h":620,"text":"Ideas · Idées · أفكار","color":"#dce5f7"},
{"id":"n1","type":"note","x":40,"y":60,"w":200,"h":180,"text":"Launch the new website","color":"#f6e7a4","autoFontSize":true},
{"id":"n2","type":"note","x":260,"y":60,"w":200,"h":180,"text":"Former l'équipe à Maktab","color":"#d9e9d2","autoFontSize":true},
{"id":"n3","type":"note","x":480,"y":60,"w":200,"h":180,"text":"إطلاق تطبيق الجوال قبل الصيف","color":"#eadcf0","autoFontSize":true},
{"id":"s","type":"rect","x":760,"y":70,"w":170,"h":70,"text":"Start · بداية","shape":"terminator","color":"#d9e9d2","textAlign":"center","autoFontSize":true},
{"id":"d","type":"rect","x":760,"y":200,"w":170,"h":130,"text":"Ready?","shape":"diamond","color":"#f3d6c8","textAlign":"center","autoFontSize":true},
{"id":"e","type":"rect","x":980,"y":225,"w":170,"h":80,"text":"Publier","color":"#dce5f7","textAlign":"center","autoFontSize":true},
{"id":"a1","type":"arrow","x":0,"y":0,"w":1,"h":1,"startId":"s","endId":"d","lineType":"elbow"},
{"id":"a2","type":"arrow","x":0,"y":0,"w":1,"h":1,"startId":"d","endId":"e","lineType":"elbow","text":"نعم"},
{"id":"m","type":"rect","x":40,"y":330,"w":200,"h":80,"text":"Product · منتج","color":"#d9e9d2","textAlign":"center","fontSize":22,"autoFontSize":true,"mindMap":{"parent":null,"direction":"right","auto":true,"line":"curved"}},
{"id":"m1","type":"rect","x":340,"y":280,"w":170,"h":60,"text":"Design","color":"#dce5f7","textAlign":"center","autoFontSize":true,"mindMap":{"parent":"m","direction":"right","auto":true,"line":"curved"}},
{"id":"m2","type":"rect","x":340,"y":360,"w":170,"h":60,"text":"Marketing","color":"#f6e7a4","textAlign":"center","autoFontSize":true,"mindMap":{"parent":"m","direction":"right","auto":true,"line":"curved"}},
{"id":"m3","type":"rect","x":340,"y":440,"w":170,"h":60,"text":"الدعم الفني","color":"#eadcf0","textAlign":"center","autoFontSize":true,"mindMap":{"parent":"m","direction":"right","auto":true,"line":"curved"}},
{"id":"t","type":"text","x":620,"y":420,"w":520,"h":60,"text":"Bonjour · Hello · مرحبًا","fontSize":34},
{"id":"p","type":"pen","x":640,"y":510,"w":400,"h":60,"points":[[0,30],[60,0],[120,50],[180,10],[240,55],[300,5],[360,40],[400,20]],"color":"#d03a3a"}]}"##;
    let path = std::env::temp_dir().join("saboura-readme.json");
    std::fs::write(&path, board).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 860.0)).wgpu().build_eframe(move |cc| BoardApp::new(cc, Some(path.clone())));
    for (lang, code) in [(crate::i18n::Lang::En, "en"), (crate::i18n::Lang::Fr, "fr"), (crate::i18n::Lang::Ar, "ar")] {
        crate::i18n::set(lang);
        h.state_mut().prefs.lang = code.into();
        h.state_mut().selection = vec!["n3".into()];
        h.state_mut().fit_pending = true;
        h.run_steps(6);
        let img = h.render().unwrap();
        img.save(format!("{dir}/saboura-{code}.png")).unwrap();
    }
    crate::i18n::set(crate::i18n::Lang::En);
}
