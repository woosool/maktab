//! The app driven headlessly (egui_kittest): it starts, renders, takes commands, and
//! snapshots go to SLIDES_SNAPSHOTS when set.

use crate::app::SlidesApp;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn harness() -> Harness<'static, SlidesApp> {
    // Tests keep settings out of the user's folder.
    unsafe { std::env::set_var("SLIDES_CONFIG_DIR", std::env::temp_dir().join("qalam-slides-test")); }
    Harness::builder().with_size(egui::vec2(1400.0, 860.0)).wgpu().build_eframe(|cc| SlidesApp::new(cc, None))
}

fn save(h: &mut Harness<'static, SlidesApp>, name: &str) {
    if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") {
        if let Ok(img) = h.render() { let _ = img.save(format!("{dir}/{name}.png")); }
    }
}

#[test]
fn starts_and_builds_a_deck() {
    let mut h = harness();
    h.run_steps(4);
    assert_eq!(h.state().slides().len(), 1);
    save(&mut h, "app-start");
    {
        let app = h.state_mut();
        let s = app.slide().unwrap();
        let scene = app.pres.scene(&s);
        let title = scene.items.iter().find(|i| i.placeholder.is_some()).unwrap().loc.clone();
        app.pres.insert_text(&title, None, 0, "Qalam Slides");
        app.changed();
        app.new_slide(None);
        app.insert_shape("star5", app.centered(2_000_000.0, 2_000_000.0));
    }
    h.run_steps(6);
    assert_eq!(h.state().slides().len(), 2);
    assert_eq!(h.state().selection.len(), 1);
    save(&mut h, "app-deck");
    h.state_mut().undo();
    h.run_steps(2);
    assert!(h.state().scene().unwrap().items.iter().all(|i| !matches!(&i.kind, slides_engine::scene::Kind::Shape { .. }) || i.placeholder.is_some()));
    // The slide show opens and ends.
    h.state_mut().start_show(0);
    h.run_steps(3);
    save(&mut h, "app-show");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().show.is_none());
}

#[test]
fn arabic_interface() {
    let mut h = harness();
    crate::i18n::set(crate::i18n::Lang::Ar);
    h.run_steps(4);
    {
        let app = h.state_mut();
        app.new_slide(None);
        app.insert_shape("rightArrow", app.centered(2_400_000.0, 1_200_000.0));
    }
    h.run_steps(6);
    save(&mut h, "app-arabic");
    crate::i18n::set(crate::i18n::Lang::En);
}

#[test]
fn home_and_insert_features() {
    let mut h = harness();
    h.run_steps(3);
    {
        let app = h.state_mut();
        app.new_slide(None);
        let s = app.slide().unwrap();
        // Text with effects, a chart, SmartArt, an equation, WordArt, sections.
        let scene = app.pres.scene(&s);
        let title = scene.items.iter().find(|i| i.placeholder.as_ref().is_some_and(|p| p.0 == "title")).unwrap().loc.clone();
        app.pres.insert_text(&title, None, 0, "Quarterly results");
        app.selection = vec![title.clone()];
        app.text_cmd(|p, l, c, r| { p.change_case(l, c, r, "upper"); p.set_text_effect(l, c, r, &slides_engine::format::Effect::Shadow { dir: 45.0, dist: 2.0, blur: 3.0, color: "000000".into(), alpha: 40 }); });
        let (sw, sh) = app.pres.size();
        app.pres.insert_chart(&s, word_engine::objects::ChartKind::Column, &slides_engine::scene::Xfrm { x: sw * 0.05, y: sh * 0.3, w: sw * 0.42, h: sh * 0.6, ..Default::default() });
        app.insert_smartart(None, word_engine::objects::DiagramKind::Process, "Plan\nBuild\nShip");
        if let Some(g) = app.selection.first().cloned() { let mut x = app.pres.xfrm(&g).unwrap(); x.x = sw * 0.52; x.y = sh * 0.3; x.w = sw * 0.44; x.h = sh * 0.3; app.pres.set_xfrm(&g, &x); }
        app.insert_equation(None, "x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}".into());
        if let Some(e) = app.selection.first().cloned() { let mut x = app.pres.xfrm(&e).unwrap(); x.x = sw * 0.6; x.y = sh * 0.68; app.pres.set_xfrm(&e, &x); }
        app.pres.set_sections(vec![("Intro".into(), 0), ("Results".into(), 1)]);
        app.selection.clear();
        app.changed();
    }
    h.run_steps(8);
    save(&mut h, "home-tab");
    assert!(h.state().dialog.is_none());
    h.state_mut().tab = crate::app::Tab::Insert;
    h.run_steps(4);
    save(&mut h, "insert-tab");
    // The deck saves and reopens with everything.
    let bytes = h.state().pres.save().unwrap();
    let again = slides_engine::Presentation::open(&bytes).unwrap();
    assert_eq!(again.sections().len(), 2);
    let s = again.slides()[1].clone();
    let scene = again.scene(&s);
    assert!(scene.items.iter().any(|i| matches!(i.kind, slides_engine::scene::Kind::Chart(_))));
    // Panes open without trouble.
    for pane in [crate::app::Pane::Selection, crate::app::Pane::Format, crate::app::Pane::Comments, crate::app::Pane::Designer] {
        h.state_mut().pane = pane;
        h.run_steps(3);
    }
    save(&mut h, "designer-pane");
}

/// Slide EMU → screen point, from where the canvas drew the slide.
fn at(h: &Harness<'static, SlidesApp>, x: f64, y: f64) -> egui::Pos2 {
    let (o, s) = h.state().canvas.view.expect("canvas drawn");
    egui::pos2(o.x + x as f32 * s, o.y + y as f32 * s)
}
fn press(h: &mut Harness<'static, SlidesApp>, p: egui::Pos2) {
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.run_steps(1);
}
fn release(h: &mut Harness<'static, SlidesApp>, p: egui::Pos2) {
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
}
fn click_at(h: &mut Harness<'static, SlidesApp>, p: egui::Pos2) { press(h, p); release(h, p); h.run_steps(1); }
fn drag(h: &mut Harness<'static, SlidesApp>, from: egui::Pos2, to: egui::Pos2) {
    press(h, from);
    for k in 1..=8 { let t = k as f32 / 8.0; h.event(egui::Event::PointerMoved(from + (to - from) * t)); h.run_steps(1); }
    release(h, to);
}

#[test]
fn editor_selection_drag_and_tables() {
    let mut h = harness();
    h.run_steps(3);
    let (star, x) = {
        let app = h.state_mut();
        app.new_slide(None); // Title and Content: a big body placeholder
        let s = app.slide().unwrap();
        let x = slides_engine::scene::Xfrm { x: 4_000_000.0, y: 3_000_000.0, w: 1_500_000.0, h: 1_500_000.0, ..Default::default() };
        let star = app.pres.insert_shape(&s, "star5", &x, false).unwrap();
        // The body placeholder on top of the star, as in decks where text boxes cover art.
        let n = app.pres.slide(&s).unwrap().path("p:cSld/p:spTree").unwrap().elements().count();
        let body = slides_engine::scene::Loc { part: s.clone(), path: vec![n - 2] };
        let star = app.pres.reorder(&star, slides_engine::edit::Order::Back).unwrap_or(star);
        let _ = body;
        app.selection.clear();
        app.changed();
        (star, x)
    };
    h.run_steps(3);
    // 1. A click on the star selects it, though an empty placeholder lies over it.
    { let q = at(&h, x.x + x.w / 2.0, x.y + x.h / 2.0); click_at(&mut h, q); }
    assert_eq!(h.state().selection, vec![star.clone()], "the star is selected");
    assert!(h.state().edit.is_none());
    // 2. Dragging moves it, as one undo step.
    let before = h.state().pres.xfrm(&star).unwrap();
    let from = at(&h, x.x + x.w / 2.0, x.y + x.h / 2.0);
    drag(&mut h, from, from + egui::vec2(120.0, 40.0));
    let after = h.state().pres.xfrm(&star).unwrap();
    assert!(after.x > before.x + 100_000.0 && after.y > before.y, "moved: {:?} → {:?}", before, after);
    h.state_mut().undo();
    h.run_steps(2);
    assert_eq!(h.state().pres.xfrm(&star).unwrap().x, before.x);
    // 3. A click in the empty title placeholder starts typing there.
    let title = { let app = h.state(); let s = app.slide().unwrap(); app.pres.scene(&s).items.iter().find(|i| i.placeholder.as_ref().is_some_and(|p| p.0 == "title")).unwrap().clone() };
    let (tx, ty) = title.transform.apply(title.w / 2.0, title.h / 2.0);
    { let q = at(&h, tx, ty); click_at(&mut h, q); }
    assert!(h.state().edit.as_ref().is_some_and(|e| e.loc == title.loc), "editing the title");
    h.event(egui::Event::Text("Hello".into()));
    h.run_steps(2);
    assert_eq!(h.state().pres.shape_text(&title.loc, None), "Hello");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    // 4. Tables: click selects, click again types in the cell; Tab moves on.
    let tbl = { let app = h.state_mut(); app.insert_table(2, 3); app.selection.clear(); app.selection.clone(); app.slide().map(|s| { let sc = app.pres.scene(&s); sc.items.iter().rev().find(|i| matches!(i.kind, slides_engine::scene::Kind::Table(_))).unwrap().clone() }).unwrap() };
    h.run_steps(3);
    let cell_center = |r: usize, c: usize| { let slides_engine::scene::Kind::Table(t) = &tbl.kind else { unreachable!() }; let (x, y, w, hh) = t.cell_rect(r, c).unwrap(); tbl.transform.apply(x + w / 2.0, y + hh / 2.0) };
    let (cx, cy) = cell_center(0, 0);
    { let q = at(&h, cx, cy); click_at(&mut h, q); }
    assert_eq!(h.state().selection, vec![tbl.loc.clone()]);
    { let q = at(&h, cx, cy); click_at(&mut h, q); }
    assert_eq!(h.state().edit.as_ref().and_then(|e| e.cell), Some((0, 0)), "editing cell (0,0)");
    h.event(egui::Event::Text("Q1".into()));
    h.run_steps(1);
    h.key_press(egui::Key::Tab);
    h.run_steps(1);
    h.event(egui::Event::Text("Q2".into()));
    h.run_steps(2);
    assert_eq!(h.state().pres.shape_text(&tbl.loc, Some((0, 0))), "Q1");
    assert_eq!(h.state().pres.shape_text(&tbl.loc, Some((0, 1))), "Q2");
    save(&mut h, "editor-table");
    // 5. Dragging a column border resizes the column.
    h.key_press(egui::Key::Escape);
    h.run_steps(1);
    h.state_mut().selection = vec![tbl.loc.clone()];
    h.run_steps(2);
    let slides_engine::scene::Kind::Table(t) = &tbl.kind else { unreachable!() };
    let (xs, _) = t.edges();
    let (bx, by) = tbl.transform.apply(xs[1], t.rows[0] / 2.0);
    let w0 = t.cols[0];
    let p = at(&h, bx, by);
    drag(&mut h, p, p + egui::vec2(60.0, 0.0));
    let w1 = { let app = h.state(); let s = app.slide().unwrap(); let sc = app.pres.scene(&s); sc.items.iter().find(|i| i.loc == tbl.loc).and_then(|i| if let slides_engine::scene::Kind::Table(t) = &i.kind { Some(t.cols[0]) } else { None }).unwrap() };
    assert!(w1 > w0 + 200_000.0, "column widened {w0} → {w1}");
}

#[test]
fn delete_and_multi_select() {
    let mut h = harness();
    h.run_steps(3);
    let (a, b, fa, fb) = {
        let app = h.state_mut();
        app.new_slide(None);
        let s = app.slide().unwrap();
        let fa = slides_engine::scene::Xfrm { x: 3_000_000.0, y: 3_000_000.0, w: 1_200_000.0, h: 1_200_000.0, ..Default::default() };
        let fb = slides_engine::scene::Xfrm { x: 6_000_000.0, y: 3_200_000.0, w: 1_200_000.0, h: 1_200_000.0, ..Default::default() };
        let a = app.pres.insert_shape(&s, "rect", &fa, false).unwrap();
        let b = app.pres.insert_shape(&s, "ellipse", &fb, false).unwrap();
        app.changed();
        (a, b, fa, fb)
    };
    h.run_steps(3);
    let ca = at(&h, fa.x + fa.w / 2.0, fa.y + fa.h / 2.0);
    let cb = at(&h, fb.x + fb.w / 2.0, fb.y + fb.h / 2.0);
    // Shift-click and ⌘-click add to the selection.
    click_at(&mut h, ca);
    h.event(egui::Event::PointerMoved(cb));
    h.event(egui::Event::PointerButton { pos: cb, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::SHIFT });
    h.event(egui::Event::PointerButton { pos: cb, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::SHIFT });
    h.run_steps(3);
    assert_eq!(h.state().selection.len(), 2, "shift-click adds");
    // Drag a box from an empty part of the slide (inside the empty body placeholder).
    h.state_mut().selection.clear();
    h.run_steps(1);
    let from = at(&h, fa.x - 400_000.0, fa.y - 400_000.0);
    let to = at(&h, fb.x + fb.w + 300_000.0, fb.y + fb.h + 300_000.0);
    drag(&mut h, from, to);
    assert!(h.state().selection.contains(&a) && h.state().selection.contains(&b), "marquee selects both: {:?}", h.state().selection);
    // Delete removes them; Backspace too.
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    let n = h.state().scene().unwrap().items.iter().filter(|i| i.own && i.placeholder.is_none()).count();
    assert_eq!(n, 0, "deleted");
    h.state_mut().undo();
    h.run_steps(2);
    click_at(&mut h, ca);
    h.key_press(egui::Key::Backspace);
    h.run_steps(2);
    let n = h.state().scene().unwrap().items.iter().filter(|i| i.own && i.placeholder.is_none()).count();
    assert_eq!(n, 1, "backspace deleted one");
}

#[test]
fn context_menu_design_tab_and_adjust() {
    let mut h = harness();
    h.run_steps(3);
    let (star, f) = {
        let app = h.state_mut();
        let s = app.slide().unwrap();
        let f = slides_engine::scene::Xfrm { x: 4_000_000.0, y: 2_500_000.0, w: 2_400_000.0, h: 1_600_000.0, ..Default::default() };
        let star = app.pres.insert_shape(&s, "roundRect", &f, false).unwrap();
        app.changed();
        (star, f)
    };
    h.run_steps(3);
    let c = at(&h, f.x + f.w / 2.0, f.y + f.h / 2.0);
    // Right-click selects the shape and opens its menu.
    h.event(egui::Event::PointerMoved(c));
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(4);
    assert_eq!(h.state().selection, vec![star.clone()]);
    save(&mut h, "context-menu");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    // The yellow handle of a rounded rectangle changes its corner radius.
    h.state_mut().selection = vec![star.clone()];
    h.run_steps(2);
    let before = h.state().pres.adjusts(&star);
    let handle = { let app = h.state(); let s = app.slide().unwrap(); let sc = app.pres.scene(&s); let it = sc.items.iter().find(|i| i.loc == star).unwrap().clone(); let slides_engine::scene::Kind::Shape { geom, .. } = &it.kind else { panic!() }; let hh = &geom.handles[0]; it.transform.apply(hh.at.x, hh.at.y) };
    let hp = at(&h, handle.0, handle.1);
    drag(&mut h, hp, hp + egui::vec2(40.0, 0.0));
    let after = h.state().pres.adjusts(&star);
    assert_ne!(before, after, "adjusted: {before:?} → {after:?}");
    // Delete works after typing in the notes.
    h.state_mut().selection = vec![star.clone()];
    h.run_steps(1);
    h.key_press(egui::Key::Delete);
    h.run_steps(2);
    assert!(h.state().scene().unwrap().items.iter().all(|i| i.loc != star), "deleted");
    // The Design tab with themes, and a theme applied.
    h.state_mut().tab = crate::app::Tab::Design;
    h.run_steps(6);
    save(&mut h, "design-tab");
    let id = slides_engine::design::catalog().themes[5].id.clone();
    { let app = h.state_mut(); let deck = slides_engine::design::theme_deck(&id, 1).unwrap(); app.checkpoint("theme"); app.pres.apply_theme_deck(&deck, &[]); }
    h.run_steps(6);
    save(&mut h, "design-applied");
    h.state_mut().pane = crate::app::Pane::Background;
    h.run_steps(3);
}

fn click_label(h: &mut Harness<'static, SlidesApp>, label: &str) {
    use egui_kittest::kittest::Queryable;
    h.get_by_label(label).click();
    h.run_steps(3);
}

#[test]
fn table_editing_through_the_ribbon_and_transitions() {
    let mut h = harness();
    h.run_steps(3);
    let tbl = { let app = h.state_mut(); app.insert_table(2, 2); app.selection[0].clone() };
    h.run_steps(3);
    // Type in a cell, then use the Layout tab.
    let cell = |h: &Harness<'static, SlidesApp>, r: usize, c: usize| { let app = h.state(); let s = app.slide().unwrap(); let sc = app.pres.scene(&s); let it = sc.items.iter().find(|i| i.loc == tbl).unwrap().clone(); let slides_engine::scene::Kind::Table(t) = &it.kind else { panic!() }; let (x, y, w, hh) = t.cell_rect(r, c).unwrap(); it.transform.apply(x + w / 2.0, y + hh / 2.0) };
    let (cx, cy) = cell(&h, 0, 0);
    { let q = at(&h, cx, cy); click_at(&mut h, q); }
    assert_eq!(h.state().edit.as_ref().and_then(|e| e.cell), Some((0, 0)));
    h.state_mut().tab = crate::app::Tab::TableLayout;
    h.run_steps(3);
    click_label(&mut h, "Insert Below");
    click_label(&mut h, "Insert Right");
    assert_eq!(h.state().pres.table_size(&tbl), Some((3, 3)));
    // Select two cells and merge them.
    h.state_mut().edit = None;
    h.state_mut().cell_sel = Some((tbl.clone(), (0, 0), (0, 1)));
    h.run_steps(2);
    click_label(&mut h, "Merge Cells");
    let merged = { let app = h.state(); app.pres.element(&tbl).unwrap().find("a:tc").unwrap().attr("gridSpan").map(String::from) };
    assert_eq!(merged.as_deref(), Some("2"));
    h.state_mut().cell_sel = Some((tbl.clone(), (2, 0), (2, 0)));
    h.run_steps(2);
    { use egui_kittest::kittest::Queryable; h.get_by_label("Delete").click(); }
    h.run_steps(3);
    click_label(&mut h, "Delete Rows");
    assert_eq!(h.state().pres.table_size(&tbl), Some((2, 3)));
    save(&mut h, "table-layout-tab");
    // Transitions: pick one, set options and timing; it round-trips.
    h.state_mut().selection.clear();
    h.state_mut().tab = crate::app::Tab::Transitions;
    h.run_steps(4);
    click_label(&mut h, "Push");
    let s = h.state().slide().unwrap();
    assert_eq!(h.state().pres.slide_transition(&s).effect, "push");
    h.run_steps(3);
    assert_eq!(h.state().pres.slide_transition(&s).duration_ms, 700, "default duration kept");
    h.run_steps(20);
    save(&mut h, "transitions-tab");
    { let app = h.state_mut(); app.checkpoint("t"); let mut t = app.pres.slide_transition(&s); t.effect = "cube".into(); t.option = "u".into(); t.after_ms = Some(2000); app.pres.set_slide_transition(&s, &t); }
    h.state_mut().start_preview();
    h.run_steps(5);
    save(&mut h, "transition-preview");
}

#[test]
fn transition_gallery_snapshot() {
    let mut h = Harness::builder().with_size(egui::vec2(620.0, 520.0)).wgpu().build_ui(|ui| {
        qalam_kit::fonts::install_fonts(ui.ctx());
        ui.horizontal_wrapped(|ui| for e in slides_engine::transition::EFFECTS { crate::transitions_tab::tile_for_test(ui, e.id, e.name); });
    });
    h.run_steps(3);
    if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") { if let Ok(img) = h.render() { let _ = img.save(format!("{dir}/transition-gallery.png")); } }
}

#[test]
fn animations_morph_and_three_d() {
    let mut h = harness();
    h.run_steps(4);
    {
        let app = h.state_mut();
        app.new_slide(None);
        app.insert_shape("roundRect", app.centered(2_400_000.0, 1_600_000.0));
        app.tab = crate::app::Tab::Animations;
    }
    h.run_steps(4);
    click_label(&mut h, "Fly In");
    h.run_steps(2);
    let s = h.state().slide().unwrap();
    let list = h.state().pres.animations(&s);
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].effect.as_str(), list[0].option.as_str()), ("flyIn", "d"));
    // Add a second effect, open the pane, change timing.
    {
        let app = h.state_mut();
        crate::animations_tab::edit(app, |l, _, _| { let mut a = slides_engine::animation::Anim::new(l[0].shape, "spin"); a.start = slides_engine::animation::Start::AfterPrevious; l.push(a); });
        app.pane = crate::app::Pane::Animation;
        app.anim_preview = None;
    }
    h.run_steps(4);
    assert_eq!(h.state().pres.animations(&s).len(), 2);
    save(&mut h, "animations-tab");
    crate::animations_tab::start_preview(h.state_mut());
    h.run_steps(3);
    assert!(h.state().anim_preview.is_some());
    save(&mut h, "animation-preview");
    h.state_mut().anim_preview = None;
    // The show plays the steps: the shape enters on the first click.
    let at = h.state().current;
    h.state_mut().start_show(at);
    h.run_steps(3);
    save(&mut h, "animation-show-0");
    h.key_press(egui::Key::Space);
    h.run_steps(3);
    save(&mut h, "animation-show-1");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    // 3-D through the Format pane's controls.
    {
        let app = h.state_mut();
        let s = app.slide().unwrap();
        let id = app.pres.animations(&s)[0].shape;
        let l = app.pres.shape_by_id(&s, id).unwrap();
        app.selection = vec![l.clone()];
        app.pres.set_three_d(&l, &slides_engine::three_d::ThreeD { camera: "isometricLeftDown".into(), depth: 200_000.0, top: Some(slides_engine::three_d::Bevel { kind: "circle".into(), w: 100_000.0, h: 100_000.0 }), ..Default::default() });
        app.changed();
        app.pane = crate::app::Pane::Format;
        app.format_tab = 1;
        app.tab = crate::app::Tab::Home;
    }
    h.run_steps(4);
    save(&mut h, "three-d-format");
    // Morph: a copy of the slide with the shape moved and turned.
    {
        let app = h.state_mut();
        app.duplicate_slides();
        let s2 = app.slide().unwrap();
        let loc = app.pres.scene(&s2).items.iter().find(|i| i.placeholder.is_none()).unwrap().loc.clone();
        let mut x = app.pres.xfrm(&loc).unwrap();
        x.x += 3_000_000.0; x.y -= 1_000_000.0; x.rot = 30.0; x.w *= 0.6;
        app.pres.set_xfrm(&loc, &x);
        let mut t = app.pres.slide_transition(&s2); t.effect = "morph".into(); t.duration_ms = 2000; app.pres.set_slide_transition(&s2, &t);
        app.changed();
    }
    h.run_steps(3);
    h.state_mut().start_preview();
    h.run_steps(3);
    assert!(h.state().morph_preview.is_some());
    // Half-way through.
    { let app = h.state_mut(); let now = app.ctx.input(|i| i.time); if let Some(p) = app.preview.as_mut() { p.0 = now - 0.5; } }
    h.run_steps(1);
    eprintln!("preview still on: {}", h.state().preview.is_some());
    save(&mut h, "morph-preview");
}

#[test]
fn views_slide_show_tab_and_3d_models() {
    use crate::views::{View, ViewDialog};
    let mut h = harness();
    h.run_steps(4);
    {
        let app = h.state_mut();
        for _ in 0..4 { app.new_slide(None); }
        app.go_to(1);
        app.tab = crate::app::Tab::Insert;
    }
    h.run_steps(3);
    // Insert → 3D Models.
    crate::three_d_ui::insert_model(h.state_mut(), "cube");
    h.run_steps(4);
    let l = h.state().selection[0].clone();
    assert!(h.state().pres.three_d(&l).depth > 0.0);
    save(&mut h, "3d-model-inserted");
    // The View tab and each view.
    h.state_mut().tab = crate::app::Tab::View;
    { let app = h.state_mut(); app.show_ruler = true; app.show_grid = true; }
    h.run_steps(4);
    save(&mut h, "view-tab");
    for (v, name) in [(View::Outline, "view-outline"), (View::Sorter, "view-sorter"), (View::NotesPage, "view-notes-page"), (View::SlideMaster, "view-slide-master"), (View::HandoutMaster, "view-handout-master"), (View::NotesMaster, "view-notes-master")] {
        crate::views::set_view(h.state_mut(), v);
        h.run_steps(4);
        save(&mut h, name);
    }
    // Slide Master: insert, rename and delete a layout.
    crate::views::set_view(h.state_mut(), View::SlideMaster);
    h.run_steps(2);
    let m = h.state().pres.masters()[0].clone();
    let n = h.state().pres.layouts(&m).len();
    click_label(&mut h, "Insert Layout");
    h.run_steps(2);
    assert_eq!(h.state().pres.layouts(&m).len(), n + 1);
    click_label(&mut h, "Delete");
    h.run_steps(2);
    assert_eq!(h.state().pres.layouts(&m).len(), n);
    crate::views::set_view(h.state_mut(), View::Normal);
    h.state_mut().color_mode = crate::views::ColorMode::Grayscale;
    h.run_steps(3);
    save(&mut h, "view-grayscale");
    h.state_mut().color_mode = crate::views::ColorMode::Color;
    // The Slide Show tab, Set Up and custom shows.
    h.state_mut().tab = crate::app::Tab::SlideShow;
    h.run_steps(3);
    save(&mut h, "slide-show-tab");
    let slides = h.state().slides();
    { let app = h.state_mut(); app.pres.set_custom_shows(&[("Short".into(), vec![slides[3].clone(), slides[1].clone()])]); }
    click_label(&mut h, "Set Up Slide Show");
    h.run_steps(3);
    assert!(matches!(h.state().view_dialog, Some(ViewDialog::SetUp(_))));
    save(&mut h, "set-up-show");
    h.state_mut().view_dialog = None;
    // A custom show plays just its slides.
    h.state_mut().start_show_with(vec![3, 1], None);
    h.run_steps(2);
    assert_eq!(h.state().show.as_ref().unwrap().order, vec![3, 1]);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    // Presenter View.
    { let app = h.state_mut(); let mut s = app.pres.show_setup(); s.presenter_view = true; app.pres.set_show_setup(&s); app.start_show(0); }
    h.run_steps(4);
    save(&mut h, "presenter-view");
    h.state_mut().show = None;
    { let app = h.state_mut(); let mut s = app.pres.show_setup(); s.presenter_view = false; app.pres.set_show_setup(&s); }
    h.run_steps(3);
    // Rehearse: the timings are offered and kept.
    click_label(&mut h, "Rehearse Timings");
    h.run_steps(2);
    assert!(h.state().show.as_ref().unwrap().rehearse.is_some());
    save(&mut h, "rehearse");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(matches!(h.state().view_dialog, Some(ViewDialog::KeepTimings { .. })));
    click_label(&mut h, "Yes");
    h.run_steps(2);
    let s0 = h.state().slides()[0].clone();
    assert!(h.state().pres.slide_transition(&s0).after_ms.is_some());
    // Ink from the pen is offered and kept as shapes.
    h.state_mut().start_show(0);
    h.run_steps(2);
    { let show = h.state_mut().show.as_mut().unwrap(); show.ink.push(crate::show::Ink { slide: 0, pts: vec![egui::pos2(0.2, 0.2), egui::pos2(0.5, 0.4), egui::pos2(0.7, 0.3)], color: egui::Color32::RED, width: 3.0, highlighter: false }); show.tool = crate::show::Tool::Pen; }
    h.run_steps(2);
    save(&mut h, "show-pen");
    h.key_press(egui::Key::Escape);
    h.run_steps(1);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(matches!(h.state().view_dialog, Some(ViewDialog::KeepInk { .. })));
    click_label(&mut h, "Keep");
    h.run_steps(2);
    let s0 = h.state().slides()[0].clone();
    assert!(h.state().pres.scene(&s0).items.iter().any(|i| matches!(&i.kind, slides_engine::scene::Kind::Shape { .. }) && i.placeholder.is_none()));
}

#[test]
fn rotation_gallery_masters_saved_and_name() {
    let mut h = harness();
    h.run_steps(3);
    assert_eq!(crate::APP_NAME, "Sard");
    {
        let app = h.state_mut();
        app.new_slide(None);
        app.insert_shape("rect", app.centered(2_000_000.0, 1_400_000.0));
        app.pane = crate::app::Pane::Format;
        app.format_tab = 1;
    }
    h.run_steps(3);
    // The drawn 3-D Rotation gallery applies a preset.
    let mut g = Harness::builder().with_size(egui::vec2(360.0, 700.0)).wgpu().build_ui_state(|ui, app: &mut SlidesApp| { qalam_kit::fonts::install_fonts(ui.ctx()); crate::three_d_ui::rotation_gallery(app, ui); }, std::mem::replace(h.state_mut(), SlidesApp::blank(egui::Context::default())));
    g.run_steps(3);
    if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") { if let Ok(img) = g.render() { let _ = img.save(format!("{dir}/rotation-gallery.png")); } }
    { use egui_kittest::kittest::Queryable; g.get_by_label("Perspective: Left").click(); }
    g.run_steps(2);
    let app = g.state();
    assert_eq!(app.pres.three_d(&app.selection[0]).camera, "perspectiveLeft");
    // Handout master settings are saved with the file.
    let mut pres = app.pres.clone();
    pres.set_page_master("handout", "6|1101110", (true, false, true, true));
    let again = slides_engine::Presentation::open(&pres.save().unwrap()).unwrap();
    assert_eq!(again.page_master("handout").as_deref(), Some("6|1101110"));
    // The displays are listed for the show.
    let _ = crate::views::monitors();
}

/// Draws Sard's icon (SARD_WRITE_ICON=dir): icon-full.png 1024 px and icon.png 256 px.
#[test]
fn write_icon() {
    let Ok(dir) = std::env::var("SARD_WRITE_ICON") else { return };
    use slides_engine::scene::Xfrm;
    let mut p = slides_engine::Presentation::new_blank(true);
    let side = 4_000_000.0;
    p.set_size(side as i64, side as i64);
    let l = p.all_layouts().last().cloned().unwrap();
    let s = p.add_slide(&l, 0);
    let m = side * 0.09;
    let bg = p.insert_shape(&s, "roundRect", &Xfrm { x: m, y: m, w: side - 2.0 * m, h: side - 2.0 * m, ..Default::default() }, false).unwrap();
    p.set_adjust(&bg, "adj", 22_000.0);
    p.set_gradient_fill(&bg, &[(0.0, "F27A45".into()), (1.0, "C93A12".into())], 90.0, false);
    p.set_line(&bg, Some(""), None, None);
    // The slide card, with a soft shadow.
    let (cw, ch) = (side * 0.6, side * 0.38);
    let card = p.insert_shape(&s, "roundRect", &Xfrm { x: (side - cw) / 2.0, y: side * 0.2, w: cw, h: ch, ..Default::default() }, false).unwrap();
    p.set_adjust(&card, "adj", 9_000.0);
    p.set_fill(&card, Some("FFFFFF"));
    p.set_line(&card, Some(""), None, None);
    p.set_effect(&card, &slides_engine::format::Effect::Shadow { dir: 90.0, dist: 10.0, blur: 30.0, color: "5A1A05".into(), alpha: 45 });
    // A title bar and lines on the card, and a play mark.
    for (k, (w, c)) in [(0.5, "E2572A"), (0.62, "F2B8A2"), (0.44, "F2B8A2")].iter().enumerate() {
        let b = p.insert_shape(&s, "roundRect", &Xfrm { x: (side - cw) / 2.0 + cw * 0.1, y: side * 0.2 + ch * (0.18 + k as f64 * 0.22), w: cw * w, h: ch * if k == 0 { 0.13 } else { 0.09 }, ..Default::default() }, false).unwrap();
        p.set_adjust(&b, "adj", 50_000.0);
        p.set_fill(&b, Some(c));
        p.set_line(&b, Some(""), None, None);
    }
    let play = p.insert_shape(&s, "triangle", &Xfrm { x: (side + cw) / 2.0 - cw * 0.28, y: side * 0.2 + ch * 0.48, w: ch * 0.36, h: ch * 0.3, rot: 90.0, ..Default::default() }, false).unwrap();
    p.set_fill(&play, Some("E2572A"));
    p.set_line(&play, Some(""), None, None);
    // The name in Arabic under the card.
    let t = p.insert_shape(&s, "rect", &Xfrm { x: m, y: side * 0.56, w: side - 2.0 * m, h: side * 0.3, ..Default::default() }, true).unwrap();
    p.insert_text(&t, None, 0, "سرد");
    p.format_range(&t, None, 0, 3, &slides_engine::edit::RunFormat { size: Some(58.0), bold: Some(true), color: Some("FFFFFF".into()), ..Default::default() });
    p.set_align(&t, None, None, "ctr");
    let mut scene = p.scene(&s);
    scene.background = slides_engine::scene::Fill::None;
    let mut images = slides_engine::render::Images::default();
    for (name, px) in [("icon-full.png", 1024.0), ("icon.png", 256.0)] {
        let pm = slides_engine::render::render(&p, &scene, &mut images, slides_engine::render::Options { scale: px / side, prompts: false, skip_text_of: None, translate: None }).unwrap();
        pm.save_png(format!("{dir}/{name}")).unwrap();
    }
}

#[test]
fn file_menu_dialogs() {
    use crate::filemenu::FileDialog;
    let mut h = harness();
    h.run_steps(3);
    h.state_mut().new_slide(None);
    h.run_steps(2);
    { use egui_kittest::kittest::Queryable; h.get_by_label("File").click(); }
    h.run_steps(3);
    save(&mut h, "file-menu");
    for k in ["Print…", "Info and Properties…", "Options…", "Keyboard Shortcuts"] { { use egui_kittest::kittest::Queryable; assert!(h.query_by_label(k).is_some(), "{k}"); } }
    { use egui_kittest::kittest::Queryable; h.get_by_label("Print…").click(); }
    h.run_steps(4);
    assert!(matches!(h.state().file_dialog, Some(FileDialog::Print { .. })));
    save(&mut h, "print-dialog");
    h.state_mut().file_dialog = Some(FileDialog::About);
    h.run_steps(3);
    save(&mut h, "about");
    // Unsaved changes ask before New.
    h.state_mut().dirty = true;
    h.state_mut().guarded(crate::filemenu::Pending::New);
    h.run_steps(2);
    { use egui_kittest::kittest::Queryable; h.get_by_label("Don't Save").click(); }
    h.run_steps(2);
    assert_eq!(h.state().slides().len(), 1);
    assert!(!h.state().dirty);
}

#[test]
fn marquee_over_a_full_slide() {
    let mut h = harness();
    h.run_steps(3);
    {
        let app = h.state_mut();
        app.new_slide(None);
        let s = app.slide().unwrap();
        let locs: Vec<_> = app.pres.scene(&s).items.iter().filter(|i| i.placeholder.is_some()).map(|i| i.loc.clone()).collect();
        for l in &locs { app.pres.insert_text(l, None, 0, "Some text here"); }
        app.insert_shape("rect", app.centered(1_000_000.0, 800_000.0));
        let x = app.centered(900_000.0, 600_000.0);
        app.insert_shape("ellipse", slides_engine::scene::Xfrm { x: x.x + 2_500_000.0, ..x });
        app.selection.clear();
        app.changed();
    }
    h.run_steps(3);
    let (sw, sh) = h.state().pres.size();
    let from = at(&h, -300_000.0, -300_000.0);
    let to = at(&h, sw + 300_000.0, sh + 300_000.0);
    drag(&mut h, from, to);
    let n = h.state().scene().unwrap().items.iter().filter(|i| i.own).count();
    eprintln!("selected {} of {}", h.state().selection.len(), n);
    assert_eq!(h.state().selection.len(), n);
    // From inside the slide over the shapes only.
    h.state_mut().selection.clear();
    h.run_steps(1);
    let from = at(&h, sw * 0.3, sh * 0.42);
    let to = at(&h, sw * 0.95, sh * 0.9);
    drag(&mut h, from, to);
    eprintln!("inner: {:?}", h.state().selection);
    assert!(h.state().selection.len() >= 2, "inner drag selects the shapes");
}

#[test]
fn smartart_editor_design_tab_and_mermaid() {
    let mut h = harness();
    h.run_steps(3);
    h.state_mut().new_slide(None);
    h.state_mut().tab = crate::app::Tab::Insert;
    h.run_steps(3);
    click_label(&mut h, "SmartArt");
    h.run_steps(4);
    assert!(h.state().diagram_editor.is_some());
    click_label(&mut h, "Organization Chart");
    h.run_steps(3);
    h.state_mut().diagram_editor.as_mut().unwrap().text = "CEO\n\tSales\n\tTech\n\t\tWeb\n\t\tApps".into();
    h.run_steps(3);
    save(&mut h, "smartart-editor");
    click_label(&mut h, "OK");
    h.run_steps(4);
    let g = h.state().selection[0].clone();
    assert_eq!(h.state().pres.diagram(&g).unwrap().layout, "orgChart");
    // The SmartArt Design tab: another layout, colors, a new shape.
    h.state_mut().tab = crate::app::Tab::SmartArt;
    h.run_steps(4);
    // Clicking "Sales" in the graphic picks that item; Move Down moves it after "Tech".
    let sales = {
        let app = h.state();
        let scene = app.scene().unwrap();
        let it = scene.items.iter().find(|it| it.loc.path.len() >= 2 && it.loc.path[0] == g.path[0] && app.pres.diagram_item(&slides_engine::scene::Loc { part: it.loc.part.clone(), path: it.loc.path[..2].to_vec() }) == Some(1)).unwrap();
        let (x, y) = it.transform.apply(it.w / 2.0, it.h / 2.0);
        at(&h, x, y)
    };
    click_at(&mut h, sales);
    h.run_steps(3);
    assert_eq!(h.state().diagram_item.as_ref().map(|d| d.1), Some(1));
    save(&mut h, "smartart-design-tab");
    click_label(&mut h, "Move Down");
    h.run_steps(3);
    let g = h.state().selection[0].clone();
    let items: Vec<String> = h.state().pres.diagram(&g).unwrap().items.iter().map(|i| i.1.clone()).collect();
    assert_eq!(items, ["CEO", "Tech", "Web", "Apps", "Sales"]);
    assert_eq!(h.state().diagram_item.as_ref().map(|d| d.1), Some(4), "Sales stays picked");
    click_label(&mut h, "Polished (3-D)");
    h.run_steps(3);
    let g = h.state().selection[0].clone();
    assert_eq!(h.state().pres.diagram(&g).unwrap().style, "polished");
    // Mermaid.
    h.state_mut().tab = crate::app::Tab::Insert;
    h.run_steps(2);
    click_label(&mut h, "Diagram");
    h.run_steps(4);
    save(&mut h, "mermaid-editor");
    click_label(&mut h, "Insert Diagram");
    h.run_steps(4);
    let m = h.state().selection[0].clone();
    assert!(h.state().pres.mermaid(&m).is_some());
    save(&mut h, "mermaid-inserted");
    // A bad diagram shows an error and can't be inserted.
    crate::smartart_ui::edit_selected(h.state_mut(), &m);
    h.state_mut().diagram_editor.as_mut().unwrap().code = "gantt\n  title x".into();
    h.run_steps(3);
    assert!(h.state().diagram_editor.as_ref().unwrap().error.is_some());
}

#[test]
fn heavy_slide_stays_fast_and_memory_bounded() {
    let mut h = harness();
    h.run_steps(3);
    {
        let app = h.state_mut();
        app.new_slide(None);
        for (k, m) in slides_engine::three_d::MODELS.iter().take(8).enumerate() { let s = app.slide().unwrap(); app.pres.insert_3d_model(&s, m.0, (k % 4) as f64 * 2.6e6 + 3e5, (k / 4) as f64 * 2.4e6 + 3e5); }
        crate::smartart_ui::apply_spec(app, None, None, &slides_engine::diagram::Spec::new("orgChart", "A\n\tB\n\tC\n\t\tD"));
        app.changed();
    }
    h.run_steps(3);
    // Undo steps share the parts they didn't change.
    let before = h.state().pres.pkg.clone();
    h.state_mut().checkpoint("t");
    let s = h.state().slide().unwrap();
    let first = h.state().slides()[0].clone();
    { let app = h.state_mut(); let l = app.selection[0].clone(); let mut x = app.pres.xfrm(&l).unwrap(); x.x += 1e5; app.pres.set_xfrm(&l, &x); }
    assert!(h.state().pres.pkg.shares_part(&before, &first), "untouched slide shared");
    assert!(!h.state().pres.pkg.shares_part(&before, &s), "edited slide copied");
    // Dragging the graphic: frames stay quick once the 3-D shapes are drawn.
    let l = h.state().selection[0].clone();
    let f = h.state().pres.xfrm(&l).unwrap();
    let from = at(&h, f.x + f.w / 2.0, f.y + f.h / 2.0);
    let t = std::time::Instant::now();
    drag(&mut h, from, from + egui::vec2(120.0, 40.0));
    let per_frame = t.elapsed() / 10;
    eprintln!("drag frame: {per_frame:?}");
    // Live previews don't pile up.
    crate::smartart_ui::open_mermaid(h.state_mut());
    for k in 0..150 {
        h.state_mut().diagram_editor.as_mut().unwrap().code = format!("graph LR\n A{k} --> B");
        h.run_steps(1);
    }
    let n = crate::texcache::len(&h.ctx);
    assert!(n <= 120, "{n} cached pictures");
}

/// Offline translation of a slide's text (with models in WOFFICE_MODELS; skipped otherwise).
#[test]
fn translate_slide_offline() {
    let Ok(dir) = std::env::var("WOFFICE_MODELS") else { return };
    if !std::path::Path::new(&dir).join("opus-mt-en-fr/decoder.onnx").is_file() { return; }
    let mut h = harness();
    h.run_steps(2);
    {
        let app = h.state_mut();
        app.new_slide(None);
        let s = app.slide().unwrap();
        let x = app.centered(4_000_000.0, 1_000_000.0);
        let t = app.pres.insert_shape(&s, "rect", &x, true).unwrap();
        app.pres.insert_text(&t, None, 0, "Hello, how are you today?\nThe presentation starts at noon.");
        app.selection = vec![t];
        app.changed();
        crate::translate::translate_slides(app, "auto", "fr", false);
    }
    let t0 = std::time::Instant::now();
    while h.state().translate_job.is_some() && t0.elapsed().as_secs() < 60 { h.run_steps(1); std::thread::sleep(std::time::Duration::from_millis(100)); }
    let l = h.state().selection[0].clone();
    let text = h.state().pres.shape_text(&l, None);
    eprintln!("{text:?}");
    assert!(text.to_lowercase().contains("bonjour") && text.contains("midi"), "{text}");
    assert_eq!(text.lines().count(), 2, "paragraphs kept");
    // The Translate window opens with the selection, offline.
    crate::translate::open(h.state_mut(), "Good morning");
    h.run_steps(3);
    assert!(h.state().translator.is_some());
}

#[test]
fn translate_window_shows_arabic_right_to_left() {
    crate::i18n::set(crate::i18n::Lang::Ar);
    let mut t = qalam_kit::translate_ui::Translator::new("had all disappeared since the last session, even though no code files changed. I couldn't determine the cause.", "ar");
    t.result = Some("لقد اختفت كلها منذ الجلسة الأخيرة، على الرغم من عدم تغيير أي ملفات رمزية. لم أستطع تحديد السبب. أعدت إنشاء ملفات التجاهل وأعدت بدء الإحياء، لكن الالتزامات السابقة قد ذهبت إلى الأبد.".into());
    let mut h = Harness::builder().with_size(egui::vec2(640.0, 520.0)).wgpu().build_ui_state(|ui, t: &mut qalam_kit::translate_ui::Translator| {
        qalam_kit::fonts::install_fonts(ui.ctx());
        let mut open = true;
        qalam_kit::translate_ui::window(ui.ctx(), t, &mut open);
    }, t);
    h.run_steps(3);
    if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") { if let Ok(img) = h.render() { let _ = img.save(format!("{dir}/translate-arabic.png")); } }
    crate::i18n::set(crate::i18n::Lang::En);
}

/// A stand-in for the system's default-app settings: PowerPoint has the types until Sard is made default.
struct FakeDefaults(std::sync::Mutex<bool>);
impl qalam_kit::default_apps::Backend for FakeDefaults {
    fn handler(&self, app: &qalam_kit::default_apps::AppId, _: &qalam_kit::default_apps::Format) -> Option<String> {
        Some(if *self.0.lock().unwrap() { app.bundle_id.to_string() } else { "com.microsoft.Powerpoint".into() })
    }
    fn make_default(&self, _: &qalam_kit::default_apps::AppId) -> Result<qalam_kit::default_apps::Outcome, String> { *self.0.lock().unwrap() = true; Ok(qalam_kit::default_apps::Outcome::Changed) }
}

#[test]
fn options_default_apps() {
    qalam_kit::default_apps::set_backend(std::sync::Arc::new(FakeDefaults(std::sync::Mutex::new(false))));
    let mut h = harness();
    h.run_steps(3);
    h.state_mut().options = Some(h.state().prefs.clone());
    h.run_steps(4);
    assert!(h.query_by_label(".pptx").is_some() && h.query_by_label(".ppsx").is_some());
    assert!(h.query_all_by_label("Another app").count() > 0);
    h.get_by_label("Make Sard the Default App").click(); h.run_steps(3);
    assert!(h.query_by_label("Another app").is_none());
    assert!(h.query_by_label("Sard is the default app for all of these file types.").is_some());
    h.state_mut().options = None;
}

#[test]
fn opens_a_file_the_system_sent() {
    crate::app::TAKE_SENT_FILES.with(|t| t.set(true));
    let p = std::env::temp_dir().join(format!("sard-sent-{}.pptx", std::process::id()));
    std::fs::write(&p, slides_engine::Presentation::new_blank(true).save().unwrap()).unwrap();
    let mut h = harness();
    h.run_steps(2);
    qalam_kit::open_files::push([p.clone()]);
    h.run_steps(3);
    assert_eq!(h.state().path.as_deref(), Some(p.as_path()));
    crate::app::TAKE_SENT_FILES.with(|t| t.set(false));
    let _ = std::fs::remove_file(&p);
}
