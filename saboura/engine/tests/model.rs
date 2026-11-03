use saboura_engine::geometry::*;
use saboura_engine::{Board, History, Item};

#[test]
fn round_trips_unknown_fields() {
    let src = r##"{"version":1,"id":"b","title":"T","updated":1,"favorite":false,"comments":[],
        "items":[{"id":"a","type":"note","x":1,"y":2,"w":3,"h":4,"text":"hi","color":"#f6e7a4","tags":["x"],"mindMap":{"parent":null}}]}"##;
    let b = Board::from_json(src).unwrap();
    assert_eq!(b.items[0].text, "hi");
    let again: serde_json::Value = serde_json::from_str(&b.to_json()).unwrap();
    assert_eq!(again["items"][0]["tags"][0], "x");
    assert!(again["comments"].is_array());
}

#[test]
fn rejects_bad_boards() {
    assert!(Board::from_json(r#"{"version":2,"id":"b","title":"T","items":[]}"#).is_err());
    assert!(Board::from_json(r#"{"version":1,"id":"b","title":"T","items":[{"id":"a","type":"bogus","x":0,"y":0,"w":1,"h":1}]}"#).is_err());
    assert!(Board::from_json(r#"{"version":1,"id":"b","title":"T","items":[{"id":"a","type":"note","x":0,"y":0,"w":1,"h":1,"color":"red"}]}"#).is_err());
}

#[test]
fn templates_and_uids() {
    assert_eq!(Board::new("x", "kanban").items.len(), 6);
    assert_eq!(Board::new("x", "brainstorm").items.len(), 5);
    let (a, b) = (saboura_engine::model::uid(), saboura_engine::model::uid());
    assert_ne!(a, b);
    assert_eq!(a.len(), 36);
}

#[test]
fn hit_testing() {
    let note = Item::new("note", 0.0, 0.0);
    let frame = Item::new("frame", -100.0, -100.0).with(|i| { i.w = 500.0; i.h = 500.0; });
    let items = vec![frame, note.clone()];
    assert_eq!(hit_test(&items, (10.0, 10.0), 0.0).unwrap().id, note.id);
    // Inside a frame's body nothing is hit; its title bar is.
    assert!(hit_test(&items, (300.0, 300.0), 0.0).is_none());
    assert_eq!(hit_test(&items, (300.0, -90.0), 0.0).unwrap().kind, "frame");
    let diamond = Item::new("rect", 0.0, 0.0).with(|i| { i.w = 100.0; i.h = 100.0; i.shape = Some("diamond".into()); });
    assert!(hits(&diamond, (50.0, 50.0), 0.0));
    assert!(!hits(&diamond, (5.0, 5.0), 0.0));
}

#[test]
fn connectors_follow_objects() {
    let mut b = Board::new("x", "blank");
    let a = Item::new("rect", 0.0, 0.0).with(|i| { i.w = 100.0; i.h = 100.0; });
    let c = Item::new("rect", 300.0, 0.0).with(|i| { i.w = 100.0; i.h = 100.0; });
    let line = Item::new("arrow", 0.0, 0.0).with(|i| { i.start_id = Some(a.id.clone()); i.end_id = Some(c.id.clone()); });
    b.items = vec![a.clone(), c, line];
    b.update_connectors();
    let l = &b.items[2];
    assert!((l.x - 100.0).abs() < 1e-6 && (l.x + l.w - 300.0).abs() < 1e-6 && (l.y - 50.0).abs() < 1e-6);
    b.remove(&[a.id].into_iter().collect());
    assert_eq!(b.items.len(), 1);
}

#[test]
fn zoom_and_resize() {
    let v = View::default().zoom_at((100.0, 100.0), 2.0);
    assert_eq!(v.to_board((100.0, 100.0)), (100.0, 100.0));
    assert_eq!(zoom_step(1.0 / 1.2, true), 1.0);
    let r = resized(Box2 { x: 0.0, y: 0.0, w: 100.0, h: 50.0 }, "se", 100.0, 0.0, true, 1.0);
    assert_eq!((r.w, r.h), (200.0, 100.0));
}

#[test]
fn history() {
    let mut h = History::default();
    let mut items = vec![Item::new("note", 0.0, 0.0)];
    h.push(&items);
    items.clear();
    assert!(h.undo(&mut items));
    assert_eq!(items.len(), 1);
    assert!(h.redo(&mut items));
    assert!(items.is_empty());
}

#[test]
fn quick_connect_and_mind_map() {
    let mut b = Board::new("x", "blank");
    let a = Item::new("rect", 0.0, 0.0);
    let id = a.id.clone();
    b.items.push(a);
    let n = saboura_engine::edit::quick_connect(&mut b, &id, "right", "elbow").unwrap();
    assert_eq!(b.items.len(), 3);
    assert!(b.find(&n).unwrap().x > 190.0);
    // A second one to the right goes beside the first, not over it.
    let m = saboura_engine::edit::quick_connect(&mut b, &id, "right", "elbow").unwrap();
    assert!(!object_bounds(b.find(&m).unwrap()).intersects(&object_bounds(b.find(&n).unwrap())));
    let c1 = saboura_engine::edit::add_branch(&mut b, &id, false).unwrap();
    let c2 = saboura_engine::edit::add_branch(&mut b, &c1, true).unwrap();
    assert_eq!(b.find(&c2).unwrap().mind_parent(), Some(id.as_str()));
    assert!(b.find(&c2).unwrap().y > b.find(&c1).unwrap().y);
}

#[test]
fn tables() {
    let mut t = saboura_engine::edit::new_table(0.0, 0.0, 2, 3);
    saboura_engine::edit::table_insert(&mut t, true, 1, false);
    assert_eq!(t.cells().unwrap().len(), 3);
    let (cols, rows) = saboura_engine::edit::table_layout(&t);
    assert_eq!((cols.len(), rows.len()), (4, 4));
}

#[test]
fn arrange_export_templates() {
    use saboura_engine::arrange::*;
    let mut a = Item::new("rect", 0.0, 0.0);
    let mut b = Item::new("rect", 300.0, 50.0);
    let mut c = Item::new("rect", 1000.0, 20.0);
    align(&mut [&mut a, &mut b, &mut c], "top");
    assert_eq!((a.y, b.y, c.y), (0.0, 0.0, 0.0));
    distribute(&mut [&mut a, &mut b, &mut c], true);
    assert!((b.x - (0.0 + 190.0 + (1190.0 - 570.0) / 2.0)).abs() < 1e-6);
    let (dx, _, g) = snap(Box2 { x: 103.0, y: 500.0, w: 50.0, h: 50.0 }, &[Box2 { x: 100.0, y: 0.0, w: 50.0, h: 50.0 }], 5.0);
    assert_eq!((dx, g.len()), (-3.0, 1));
    for (t, _) in saboura_engine::model::TEMPLATES {
        let board = Board::new("x", t);
        let (svg, _) = saboura_engine::export::to_svg(&board.items, false, true);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    }
    assert_eq!(Board::new("x", "flowchart").items.iter().filter(|i| i.is_connector() && i.start_id.is_some()).count(), 4);
}

#[test]
fn rich_runs_and_miro() {
    use saboura_engine::rich;
    let t = "hello world";
    let runs = rich::apply(t, &[], 6, 11, "bold", true.into());
    assert_eq!((runs[0].start, runs[0].end), (6, 11));
    assert!(rich::has(t, &runs, 6, 11, "bold") && !rich::has(t, &runs, 0, 5, "bold"));
    let moved = rich::adjust(t, "hey, hello world", &runs);
    assert_eq!((moved[0].start, moved[0].end), (11, 16));
    let off = rich::apply(t, &runs, 0, 11, "bold", serde_json::Value::Null);
    assert!(off.is_empty());
    let miro = serde_json::json!({
        "board": { "name": "From Miro" },
        "items": [
            { "id": "1", "type": "frame", "data": { "title": "Plan" }, "position": { "x": 0, "y": 0, "origin": "center" }, "geometry": { "width": 1000, "height": 800 } },
            { "id": "2", "type": "sticky_note", "data": { "content": "<p>Hi &amp; bye</p>" }, "style": { "fillColor": "light_yellow" }, "position": { "x": 100, "y": 100, "relativeTo": "parent_top_left" }, "parent": { "id": "1" } },
            { "id": "3", "type": "shape", "data": { "shape": "flow_chart_decision", "content": "Ok?" }, "style": { "fillColor": "#ff000080" }, "position": { "x": 300, "y": 0 } }
        ],
        "connectors": [{ "id": "c", "startItem": { "id": "2" }, "endItem": { "id": "3" }, "shape": "elbowed", "captions": [{ "content": "yes" }] }]
    });
    let (b, r) = saboura_engine::miro::convert(&miro).unwrap();
    assert_eq!((b.title.as_str(), r.converted), ("From Miro", 4));
    let note = b.items.iter().find(|i| i.kind == "note").unwrap();
    assert_eq!((note.text.as_str(), note.x + note.w / 2.0), ("Hi & bye", -400.0));
    assert_eq!(b.items.iter().find(|i| i.shape.as_deref() == Some("diamond")).unwrap().color.as_deref(), Some("#ff0000"));
    assert!(b.items.iter().any(|i| i.is_connector() && i.text == "yes" && i.end_id.is_some()));
}

#[test]
fn history_shares_pictures() {
    let mut h = History::default();
    let big = format!("data:image/png;base64,{}", "A".repeat(100_000));
    let mut items = vec![Item::new("image", 0.0, 0.0).with(|i| i.set("src", big.clone()))];
    for k in 0..50 { h.push(&items); items[0].x = k as f64; }
    assert_eq!(h.pooled(), 1);
    assert!(h.undo(&mut items));
    assert_eq!(items[0].str("src"), Some(big.as_str()));
    items.clear();
    h.push(&items);
    for _ in 0..60 { h.undo(&mut items); }
    assert_eq!(items[0].str("src").map(|s| s.len()), Some(big.len()));
}

#[test]
fn sab_archive_round_trip() {
    use base64::Engine;
    let png = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/icon.png")).unwrap();
    let src = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png));
    let mut b = Board::new("Pictures", "brainstorm");
    for k in 0..3 { b.items.push(Item::new("image", k as f64 * 100.0, 0.0).with(|i| i.set("src", src.clone()))); }
    let bytes = saboura_engine::sab::write(&b, Some(&png)).unwrap();
    assert!(saboura_engine::sab::is_sab(&bytes));
    // The picture is stored once, as a file, so the archive is about its size, not 3× base64.
    let plain = saboura_engine::sab::write(&b, None).unwrap();
    assert!(plain.len() < png.len() + 8000, "{} vs {}", plain.len(), png.len());
    let back = saboura_engine::sab::open(&bytes).unwrap();
    assert_eq!(back.items.len(), b.items.len());
    assert_eq!(back.items.last().unwrap().str("src"), Some(src.as_str()));
    assert_eq!(saboura_engine::sab::thumbnail(&bytes).unwrap(), png);
    // JSON still opens; other bytes don't.
    assert!(saboura_engine::sab::open(b.to_json().as_bytes()).is_ok());
    assert!(saboura_engine::sab::open(&[0x1f, 0x8b, 0, 1, 2]).is_err());
}
