//! The full-feature sample workbook: every part family Jadwal writes natively in one file,
//! checked structurally (schemacheck), for package consistency (pkgcheck) and read back.

use crate::addr::{parse_range, Pos};
use crate::book::Workbook;
use crate::drawing::{add_object, Anchor, Mark, ObjKind, Object, Shape};
use crate::embed::{picture_with_parts, Extra, RT_MODEL3D, RT_PACKAGE};
use std::sync::Arc;

fn two(c0: u32, r0: u32, c1: u32, r1: u32) -> Anchor { Anchor::Two { from: Mark { col: c0, row: r0, ..Default::default() }, to: Mark { col: c1, row: r1, ..Default::default() }, edit_as: None } }

pub fn sample() -> Workbook {
    let mut wb = Workbook::new();
    wb.sheets[0].name = "Data".into();
    wb.add_sheet(1, Some("Report".into()));
    wb.add_sheet(2, Some("Objects".into()));
    wb.add_sheet(3, Some("Model".into()));
    let png = crate::model3d::render_png(&crate::model3d::cube(), 16, 16, 30.0, 20.0).unwrap();

    // Data: pivot source with dates, products and states; a threaded comment and a note.
    for (c, h) in ["Date", "Product", "State", "Amount"].iter().enumerate() { wb.set_input(0, Pos::new(0, c as u32), h); }
    let rows = [("1/15/2024", "Pens", "Texas", "10"), ("2/10/2024", "Ink", "Ohio", "20"), ("3/5/2024", "Pens", "Utah", "30"), ("7/1/2024", "Paper", "Texas", "40"), ("1/2/2025", "Ink", "Ohio", "50")];
    for (r, (d, p, s, a)) in rows.iter().enumerate() {
        for (c, v) in [d, p, s, a].iter().enumerate() { wb.set_input(0, Pos::new(r as u32 + 1, c as u32), v); }
    }
    crate::review::add_comment(&mut wb, 0, Pos::new(0, 3), "Ann", "Check totals @Bob");
    crate::review::add_comment(&mut wb, 0, Pos::new(0, 3), "Bob", "Done.");
    wb.sheets[0].notes.insert(Pos::new(0, 0), "A plain note".into());
    wb.recalc();

    // Report: pivot with a timeline and a slicer, chartex charts, a filled map, SmartArt.
    let mut p = crate::pivot::Pivot::new("PivotTable1", "Data", parse_range("A1:D6").unwrap(), Pos::new(0, 0));
    p.rows = vec![1];
    p.values.push(crate::pivot::ValueField { field: 3, ..Default::default() });
    p.apply(&mut wb, 1);
    wb.sheets[1].pivots.push(p);
    let ps = crate::timeline::periods(&wb, &crate::timeline::Timeline { pivot: "PivotTable1".into(), ..Default::default() });
    let t = crate::timeline::Timeline { pivot: "PivotTable1".into(), field: 0, caption: "Date".into(), level: crate::timeline::Level::Months, selection: Some((ps[0].start, ps[1].end)), ..Default::default() };
    add_object(&mut wb.sheets[1].objects, Object::new(ObjKind::Timeline(t), two(4, 0, 10, 6)), "Timeline");
    add_object(&mut wb.sheets[1].objects, Object::new(ObjKind::Slicer(crate::drawing::Slicer { pivot: "PivotTable1".into(), table: String::new(), field: 1, caption: "Product".into() }), two(10, 0, 13, 8)), "Slicer");
    crate::timeline::sync_pivots(&mut wb);
    for (i, k) in [crate::chart::ChartKind::Funnel, crate::chart::ChartKind::Pareto, crate::chart::ChartKind::Waterfall].iter().enumerate() {
        let c = crate::chart::ex::from_range(&wb, 0, parse_range("B1:D6").unwrap(), *k);
        add_object(&mut wb.sheets[1].objects, Object::new(ObjKind::Chart(Box::new(c)), two(0, 10 + i as u32 * 12, 6, 21 + i as u32 * 12)), "Chart");
    }
    let m = crate::geo::FilledMap { source: "Data!$C$1:$D$6".into(), title: "By state".into(), low: [230, 240, 250], high: [20, 60, 140] };
    let mut o = Object::new(ObjKind::Picture { data: Arc::new(m.png(&wb, 0, 160, 100).unwrap()), ext: "png".into() }, two(7, 10, 13, 22));
    o.descr = m.tag();
    add_object(&mut wb.sheets[1].objects, o, "Map");
    for layout in ["basicProcess", "orgChart", "basicCycle"] {
        let tag = format!("jadwal-smartart:{layout}");
        let items: Vec<(Object, [f32; 4])> = ["One", "Two", "Three"].iter().enumerate().map(|(k, t)| {
            let mut c = Object::new(ObjKind::Shape(Shape { text: t.to_string(), preset: "roundRect".into(), fill: Some([0x44, 0x72, 0xC4]), size: 12.0, center: true, ..Default::default() }), Anchor::default());
            c.descr = tag.clone();
            (c, [k as f32 * 120.0, 0.0, k as f32 * 120.0 + 100.0, 60.0])
        }).collect();
        let (g, _) = crate::group::make(items);
        let mut o = Object::new(ObjKind::Group(g), two(14, 0, 20, 6));
        o.descr = tag;
        add_object(&mut wb.sheets[1].objects, o, "SmartArt");
    }

    // Objects: OLE package, signature line, 3D model, equation, in-cell picture, header picture.
    let ole = picture_with_parts(&mut wb, 2, png.clone(), two(1, 1, 3, 5), (64.0, 64.0), "Object", &format!("{}notes.txt", super::vml_objects::OBJ_TAG), vec![Extra { rel: RT_PACKAGE, prefix: "xl/embeddings/oleObject", ext: "bin".into(), mime: super::vml_objects::CT_OLE.into(), bytes: b"hello".to_vec() }]);
    add_object(&mut wb.sheets[2].objects, ole, "Object");
    let sig = picture_with_parts(&mut wb, 2, png.clone(), two(5, 1, 8, 5), (96.0, 48.0), "Signature Line", &format!("{}Ann Lee\tCEO\tann@example.com\tSign here", super::vml_objects::SIG_TAG), vec![]);
    add_object(&mut wb.sheets[2].objects, sig, "Signature Line");
    let model = picture_with_parts(&mut wb, 2, png.clone(), Anchor::One { from: Mark { col: 1, row: 8, ..Default::default() }, w: 120.0, h: 120.0 }, (120.0, 120.0), "3D Model", &format!("{}30,20 cube.glb", crate::model3d::TAG), vec![Extra { rel: RT_MODEL3D, prefix: "xl/models/model3d", ext: "glb".into(), mime: "model/gltf.binary".into(), bytes: b"glTF\x02\x00\x00\x00fake".to_vec() }]);
    add_object(&mut wb.sheets[2].objects, model, "3D Model");
    let lin = "a^2+b^2=c^2";
    let mut eq = Object::new(ObjKind::Picture { data: Arc::new(super::equation_parts::render(lin).unwrap()), ext: "png".into() }, Anchor::One { from: Mark { col: 6, row: 8, ..Default::default() }, w: 120.0, h: 30.0 });
    eq.descr = format!("{}{lin}", super::equation_parts::EQ_TAG);
    add_object(&mut wb.sheets[2].objects, eq, "Equation");
    crate::cellimage::place(&mut wb, 2, Pos::new(20, 1), &png, "png", "Cube");
    let hf = &mut wb.sheets[2].page.header_footer;
    hf.odd_header = "&LTitle&C&G".into();
    hf.pictures.push(crate::hfpic::HfPicture::new("CH".into(), png.clone(), "png"));

    // Model: sparklines, outline, data table, scenarios, solver, protection, a connection.
    for r in 0..3u32 { for c in 0..5u32 { wb.set_input(3, Pos::new(r, c), &((r + 1) * (c + 2)).to_string()); } }
    crate::sparkline::create(&mut wb, 3, crate::sparkline::Kind::Line, "A1:E3", parse_range("F1:F3").unwrap()).unwrap();
    crate::outline::group(&mut wb.sheets[3], true, 1, 2).unwrap();
    wb.set_input(3, Pos::new(5, 0), "3");
    wb.set_input(3, Pos::new(5, 1), "=A6*10");
    for (i, v) in ["1", "2"].iter().enumerate() { wb.set_input(3, Pos::new(6 + i as u32, 0), v); }
    wb.recalc();
    crate::whatif::create_table(&mut wb, 3, parse_range("A6:B8").unwrap(), None, Some(Pos::new(5, 0))).unwrap();
    crate::whatif::set_scenarios(&mut wb, 3, &[crate::whatif::Scenario { name: "High".into(), cells: vec![(Pos::new(5, 0), "10".into())], ..Default::default() }], 0);
    crate::solver::save(&mut wb, 3, &crate::solver::Model { objective: "B6".into(), vars: "A6".into(), ..Default::default() });
    crate::review::protect_sheet(&mut wb.sheets[3], crate::review::SheetProtection::default());
    crate::connections::register(&mut wb, "Feed", crate::connections::Source::Web { url: "http://example.com".into(), table: 1 }, 3, parse_range("H1:I3").unwrap());
    wb.recalc();
    wb
}

#[test]
fn full_feature_sample_passes_schema_check() {
    let wb = sample();
    // xlsx::write runs the schema check on every package in tests (pkgcheck::on_write).
    let bytes = super::write(&wb).unwrap();
    let files = super::pkgcheck::unzip(&bytes);
    super::pkgcheck::check_package(&files);
    let errs = super::schemacheck::check_files(&files);
    assert!(errs.is_empty(), "{}", errs.join("\n"));
    if let Ok(d) = std::env::var("JADWAL_DUMP") { std::fs::write(format!("{d}/full-sample.xlsx"), &bytes).unwrap(); }
    // Every family is present.
    for prefix in ["xl/pivotTables/", "xl/pivotCache/", "xl/slicerCaches/", "xl/slicers/", "xl/timelineCaches/", "xl/timelines/", "xl/charts/chartEx", "xl/diagrams/layout", "xl/diagrams/drawing",
        "xl/threadedComments/", "xl/persons/", "xl/comments", "xl/richData/", "xl/metadata.xml", "xl/embeddings/oleObject", "xl/models/", "xl/drawings/vmlDrawingHF", "xl/queryTables/", "xl/connections.xml"] {
        assert!(files.keys().any(|k| k.starts_with(prefix)), "no {prefix}");
    }
    let pt = files.iter().find(|(k, v)| k.starts_with("xl/pivotTables/pivotTable") && v.contains("PivotTable1")).unwrap().1;
    assert!(pt.contains("type=\"dateBetween\""), "{pt}");
    assert!(files.values().any(|v| v.contains("Requires=\"cx2\"")), "funnel frame");
    assert!(files["xl/comments1.xml"].contains("<author>tc={") && files["xl/comments1.xml"].contains("xr:uid=\"{"), "threaded note pairing");
    assert!(files["xl/connections.xml"].contains("type=\"4\"") && files["xl/queryTables/queryTable1.xml"].contains("connectionId=\"1\""));
    assert!(files.values().any(|v| v.contains("<dgm:alg type=\"hierChild\"")), "org chart layout");
    // Read back and written again: still passes.
    let back = super::read(&bytes).unwrap();
    let again = super::pkgcheck::unzip(&super::write(&back).unwrap());
    super::pkgcheck::check_package(&again);
    assert!(super::schemacheck::check_files(&again).is_empty());
}
