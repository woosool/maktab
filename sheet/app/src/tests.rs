//! Interface tests: the editor driven headless (egui_kittest).

use crate::app::SheetApp;
use egui_kittest::kittest::Queryable;
use sheet_engine::addr::parse_a1;

fn app() -> SheetApp { SheetApp::blank(egui::Context::default()) }

#[test]
fn typing_and_formulas() {
    let mut a = app();
    a.begin_edit(Some("5".into()), false);
    a.commit(Some((1, 0)));
    a.begin_edit(Some("=A1*3".into()), false);
    a.commit(Some((1, 0)));
    assert_eq!(a.book.display(0, parse_a1("A2").unwrap()), "15");
    assert_eq!(a.sel.active, parse_a1("A3").unwrap());
    a.undo();
    assert_eq!(a.book.display(0, parse_a1("A2").unwrap()), "");
    a.redo();
    assert_eq!(a.book.display(0, parse_a1("A2").unwrap()), "15");
}

#[test]
fn autosum_and_styles() {
    let mut a = app();
    for (i, v) in ["1", "2", "3"].iter().enumerate() { a.book.set_input(0, sheet_engine::Pos::new(i as u32, 0), v); }
    a.book.recalc();
    a.select(parse_a1("A4").unwrap(), false);
    a.autosum("SUM");
    assert_eq!(a.edit.as_ref().unwrap().text, "=SUM(A1:A3)");
    a.commit(None);
    assert_eq!(a.book.display(0, parse_a1("A4").unwrap()), "6");
    a.apply_style("Bold", |s| s.bold = true);
    assert!(a.style().bold);
    assert_eq!(crate::ribbon::decimals("0.00", 1, &sheet_engine::Value::Num(1.0)), "0.000");
    assert_eq!(crate::ribbon::decimals("General", 1, &sheet_engine::Value::Num(1.5)), "0.00");
}

#[test]
fn renders() {
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1100.0, 700.0)).with_accessibility_check(false).build_ui(move |ui| {
        thread_local!(static APP: std::cell::RefCell<Option<SheetApp>> = const { std::cell::RefCell::new(None) });
        APP.with(|a| {
            let mut a = a.borrow_mut();
            let app = a.get_or_insert_with(|| { qalam_kit::fonts::install_fonts(ui.ctx()); let mut x = SheetApp::blank(ui.ctx().clone()); x.book.set_input(0, parse_a1("B2").unwrap(), "Hello"); x.book.set_input(0, parse_a1("C3").unwrap(), "=1/3"); x.book.recalc(); x });
            app.frame(ui);
        });
    });
    harness.run_steps(3);
}


/// Drives the whole window with a harness that clicks real widgets.
fn harness(app: SheetApp) -> egui_kittest::Harness<'static, SheetApp> {
    egui_kittest::Harness::builder().with_size(egui::vec2(1300.0, 800.0)).with_accessibility_check(false).build_ui_state(|ui, app: &mut SheetApp| { if app.grid_rect == egui::Rect::NOTHING && app.ctx.cumulative_frame_nr() == 0 { qalam_kit::fonts::install_fonts(ui.ctx()); app.ctx = ui.ctx().clone(); app.grid_rect = egui::Rect::ZERO; return; } app.frame(ui) }, app)
}

fn data_app() -> SheetApp {
    let mut a = SheetApp::blank(egui::Context::default());
    for (r, row) in [["Region", "Item", "Sales"], ["East", "Pen", "10"], ["West", "Pen", "20"], ["East", "Ink", "5"], ["West", "Ink", "7"]].iter().enumerate() {
        for (c, t) in row.iter().enumerate() { a.book.set_input(0, sheet_engine::Pos::new(r as u32, c as u32), t); }
    }
    a.book.recalc();
    a
}

#[test]
fn filter_popup_opens_and_filters() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::filter_ui::toggle_filter(h.state_mut());
    h.run_steps(2);
    assert!(h.state().book.sheets[0].autofilter.is_some());
    // Click the drop-down button in B1's right edge.
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let b = lay.cell_rect(sheet_engine::Pos::new(0, 0));
    let at = egui::pos2(b.max.x - 7.0, b.center().y);
    h.hover_at(at); h.run_steps(2);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }); h.run_steps(2);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }); h.run_steps(3);
    assert!(crate::filter_ui::popup_open(&h.ctx), "the filter popup should stay open");
}

#[test]
fn pivot_creates_and_fills() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::pivot_ui::open_insert(h.state_mut());
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let a = h.state();
    let si = a.sheet();
    assert_eq!(a.book.sheets[si].pivots.len(), 1, "a pivot should exist on the active sheet");
    let mut p = a.book.sheets[si].pivots[0].clone();
    let _ = &mut p;
    let r = h.get_by_label("Region").rect();
    let at = r.center();
    h.event(egui::Event::PointerMoved(at)); h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }); h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    h.get_by_label("Sales").click();
    h.run_steps(3);
    let a = h.state();
    let s = &a.book.sheets[a.sheet()];
    let texts: Vec<String> = s.cells.keys().map(|p| a.book.display(a.sheet(), *p)).collect();
    assert!(texts.iter().any(|t| t == "East") && texts.iter().any(|t| t == "15"), "pivot output: {texts:?}");
}

#[test]
fn freeze_panes_keep_working() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().select(sheet_engine::Pos::new(1, 1), false);
    h.state_mut().freeze(0);
    h.run_steps(2);
    // Scroll and click a cell in the body and one in the frozen part.
    h.state_mut().scroll = egui::vec2(0.0, 200.0);
    h.run_steps(2);
    for target in [sheet_engine::Pos::new(30, 4), sheet_engine::Pos::new(0, 0)] {
        let lay = crate::grid::layout(h.state(), h.state().grid_rect);
        let at = lay.cell_rect(target).center();
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }); h.run_steps(2);
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }); h.run_steps(2);
        assert_eq!(h.state().sel.active, target);
    }
    // Typing still works.
    h.event(egui::Event::Text("x".into())); h.run_steps(2);
    h.event(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }); h.run_steps(2);
    assert_eq!(h.state().book.display(0, sheet_engine::Pos::new(0, 0)), "x");
}

#[test]
fn freeze_then_wheel_and_far_freeze() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().select(sheet_engine::Pos::new(1, 1), false);
    h.state_mut().freeze(0);
    h.run_steps(2);
    let mid = h.state().grid_rect.center();
    h.event(egui::Event::PointerMoved(mid)); h.run_steps(1);
    h.event(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -300.0), modifiers: Default::default(), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel }); h.run_steps(8);
    assert!(h.state().scroll.y > 0.0, "wheel should scroll with frozen panes");
    // Freezing at a cell far down the sheet.
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().select(sheet_engine::Pos::new(200, 3), false);
    h.run_steps(3);
    h.state_mut().select(sheet_engine::Pos::new(188, 3), false);
    h.run_steps(2);
    h.state_mut().freeze(0);
    h.run_steps(3);
    let s = &h.state().book.sheets[0];
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let body_visible = lay.cell_rect(sheet_engine::Pos::new(s.frozen_rows, s.frozen_cols)).min.y < h.state().grid_rect.max.y - 40.0;
    assert!(body_visible, "the scrolling part must stay on screen");
}

#[test]
fn settles_when_idle() {
    let mut h = harness(SheetApp::blank(egui::Context::default()));
    h.run_steps(4);
    h.run();
}

fn drag(h: &mut egui_kittest::Harness<'static, SheetApp>, from: egui::Pos2, to: egui::Pos2) {
    let press = |p, down| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: down, modifiers: Default::default() };
    h.event(egui::Event::PointerMoved(from)); h.run_steps(1);
    h.event(press(from, true)); h.run_steps(1);
    for k in 1..=6 { let t = k as f32 / 6.0; h.event(egui::Event::PointerMoved(from + (to - from) * t)); h.run_steps(1); }
    h.event(press(to, false)); h.run_steps(2);
}

#[test]
fn resize_columns_and_rows() {
    let mut h = harness(data_app());
    h.run_steps(3);
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let b = lay.cell_rect(sheet_engine::Pos::new(0, 1));
    let head_y = h.state().grid_rect.min.y + lay.head_h / 2.0;
    drag(&mut h, egui::pos2(b.max.x, head_y), egui::pos2(b.max.x + 60.0, head_y));
    let w = h.state().book.sheets[0].col_width(1);
    assert!((w - 124.0).abs() < 3.0, "column B width after dragging 60px: {w}");
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let r = lay.cell_rect(sheet_engine::Pos::new(2, 0));
    let head_x = h.state().grid_rect.min.x + lay.head_w / 2.0;
    drag(&mut h, egui::pos2(head_x, r.max.y), egui::pos2(head_x, r.max.y + 30.0));
    let rh = h.state().book.sheets[0].row_height(2);
    assert!((rh - 50.0).abs() < 3.0, "row 3 height after dragging 30px: {rh}");
    // One undo step per drag.
    h.state_mut().undo();
    assert!((h.state().book.sheets[0].row_height(2) - 20.0).abs() < 0.5);
    assert!((h.state().book.sheets[0].col_width(1) - 124.0).abs() < 3.0);
}

#[test]
fn drag_cells_carries_picture_and_copy_paste_chart() {
    use sheet_engine::drawing::{Anchor, Mark, ObjKind, Object};
    let mut a = data_app();
    // A small picture inside B3 (within A1:C5), and a chart over E8:H15.
    let pic = Object::new(ObjKind::Picture { data: std::sync::Arc::new(vec![0u8; 4]), ext: "png".into() }, Anchor::One { from: Mark { row: 2, col: 1, dx: 2.0, dy: 2.0 }, w: 10.0, h: 8.0 });
    sheet_engine::drawing::add_object(&mut a.book.sheets[0].objects, pic, "Picture");
    let ch = sheet_engine::chart::Chart { series: vec![sheet_engine::chart::Series { val_ref: Some("Sheet1!$C$2:$C$5".into()), ..Default::default() }], ..Default::default() };
    let anchor = Anchor::Two { from: Mark { row: 7, col: 4, ..Default::default() }, to: Mark { row: 15, col: 8, ..Default::default() }, edit_as: None };
    sheet_engine::drawing::add_object(&mut a.book.sheets[0].objects, Object::new(ObjKind::Chart(Box::new(ch)), anchor), "Chart");
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().select(sheet_engine::Pos::new(0, 0), false);
    h.state_mut().select(sheet_engine::Pos::new(4, 2), true);
    h.run_steps(2);
    // Drag A1:C5 by its bottom border (under column A) four rows down.
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let a5 = lay.cell_rect(sheet_engine::Pos::new(4, 0));
    let from = egui::pos2(a5.center().x, a5.max.y);
    let step = lay.cell_rect(sheet_engine::Pos::new(1, 0)).height();
    drag(&mut h, from, from + egui::vec2(0.0, step * 4.0));
    let s = &h.state().book.sheets[0];
    assert_eq!(h.state().book.display(0, sheet_engine::Pos::new(4, 0)), "Region", "cells moved down four rows");
    let Anchor::One { from: m, .. } = &s.objects[0].anchor else { panic!() };
    assert_eq!((m.row, m.col), (6, 1), "the picture moved with its cells");
    // Select the chart, ⌘C then ⌘V: a second chart.
    crate::objects::select(h.state(), Some(1));
    h.run_steps(1);
    let cmd = egui::Modifiers::COMMAND;
    for k in [egui::Key::C, egui::Key::V] {
        h.event(egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: cmd }); h.run_steps(2);
    }
    let charts = h.state().book.sheets[0].objects.iter().filter(|o| o.is_chart()).count();
    assert_eq!(charts, 2, "⌘C/⌘V on a chart duplicates it");
    // ⌘D duplicates the selected (pasted) object too.
    h.event(egui::Event::Key { key: egui::Key::D, physical_key: None, pressed: true, repeat: false, modifiers: cmd }); h.run_steps(2);
    assert_eq!(h.state().book.sheets[0].objects.iter().filter(|o| o.is_chart()).count(), 3);
}

/// A recommended pivot (Region by Sum of Sales) on a new sheet, with the Analyze tab showing.
fn pivot_harness(a: SheetApp) -> (egui_kittest::Harness<'static, SheetApp>, usize) {
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::pivot_ui::recommended(h.state_mut());
    h.run_steps(2);
    let si = h.state().sheet();
    assert_eq!(h.state().book.sheets[si].pivots.len(), 1);
    (h, si)
}

#[test]
fn pivot_group_dialog_groups_numbers() {
    let (mut h, si) = pivot_harness(data_app());
    {
        let a = h.state_mut();
        a.book.sheets[si].pivots[0].rows = vec![2];
        a.book.sheets[si].pivots[0].values = vec![sheet_engine::pivot::ValueField::new(2, sheet_engine::pivot::Agg::Count)];
        crate::pivot_ui::rerun(a, si, 0);
        // The cursor on the first row item, then PivotTable Analyze ▸ Group Field.
        let anchor = a.book.sheets[si].pivots[0].anchor;
        a.select(sheet_engine::Pos::new(anchor.row + 1, anchor.col), false);
        a.tab = crate::app::Tab::PivotAnalyze;
    }
    h.run_steps(3);
    h.get_by_label("Group Field").click();
    h.run_steps(3);
    assert!(matches!(crate::pivot_ui::open_dialog(&h.ctx), Some(crate::pivot_ui::PivotDlg::Group(ref g)) if !g.date && g.field == 2), "the Group dialog opens for Sales");
    h.get_by_label("OK").click();
    h.run_steps(3);
    let a = h.state();
    let p = &a.book.sheets[si].pivots[0];
    assert!(matches!(p.groups.get(&2), Some(sheet_engine::pivot::Grouping::Range { by, .. }) if *by == 10.0));
    assert!(crate::pivot_ui::open_dialog(&h.ctx).is_none());
    let texts: Vec<String> = a.book.sheets[si].cells.keys().map(|q| a.book.display(si, *q)).collect();
    assert!(texts.iter().any(|t| t == "5-14") && texts.iter().any(|t| t == "15-24"), "grouped items: {texts:?}");
}

#[test]
fn pivot_group_dialog_groups_dates() {
    let mut a = SheetApp::blank(egui::Context::default());
    for (r, row) in [["Date", "Sales"], ["2024-01-15", "10"], ["2024-02-03", "20"], ["2025-03-20", "5"]].iter().enumerate() {
        for (c, t) in row.iter().enumerate() { a.book.set_input(0, sheet_engine::Pos::new(r as u32, c as u32), t); }
    }
    a.book.recalc();
    let (mut h, si) = pivot_harness(a);
    h.state_mut().book.sheets[si].pivots[0].rows = vec![0];
    crate::pivot_ui::rerun(h.state_mut(), si, 0);
    crate::pivot_ui::open_group(h.state_mut(), si, 0, 0);
    h.run_steps(3);
    assert!(matches!(crate::pivot_ui::open_dialog(&h.ctx), Some(crate::pivot_ui::PivotDlg::Group(ref g)) if g.date), "a date field opens the date grouping");
    h.get_by_label("Years").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let p = &h.state().book.sheets[si].pivots[0];
    assert_eq!(p.extra.len(), 1);
    assert_eq!(p.rows, vec![2, 0]);
    let a = h.state();
    let texts: Vec<String> = a.book.sheets[si].cells.keys().map(|q| a.book.display(si, *q)).collect();
    assert!(texts.iter().any(|t| t == "2024") && texts.iter().any(|t| t == "Jan"), "grouped items: {texts:?}");
}

#[test]
fn pivot_value_field_settings_dialog() {
    let (mut h, si) = pivot_harness(data_app());
    {
        let a = h.state_mut();
        let anchor = a.book.sheets[si].pivots[0].anchor;
        a.select(sheet_engine::Pos::new(anchor.row + 1, anchor.col + 1), false);
        a.tab = crate::app::Tab::PivotAnalyze;
    }
    h.run_steps(3);
    h.get_by_label("Field Settings").click();
    h.run_steps(3);
    assert!(matches!(crate::pivot_ui::open_dialog(&h.ctx), Some(crate::pivot_ui::PivotDlg::Values(_))), "Value Field Settings opens");
    h.get_by_label("Average").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let a = h.state();
    let p = &a.book.sheets[si].pivots[0];
    assert_eq!(p.values[0].agg, sheet_engine::pivot::Agg::Average);
    let texts: Vec<String> = a.book.sheets[si].cells.keys().map(|q| a.book.display(si, *q)).collect();
    assert!(texts.iter().any(|t| t == "Average of Sales"), "header: {texts:?}");
    // Show Values As through the dialog's state: % of Grand Total.
    crate::pivot_ui::open_value_settings(h.state_mut(), si, 0, 0);
    h.run_steps(2);
    h.get_by_label("Show Values As").click();
    h.run_steps(2);
    assert!(matches!(crate::pivot_ui::open_dialog(&h.ctx), Some(crate::pivot_ui::PivotDlg::Values(ref v)) if v.show_tab));
}

#[test]
fn pivot_design_tab_changes_layout() {
    let (mut h, si) = pivot_harness(data_app());
    {
        let a = h.state_mut();
        let anchor = a.book.sheets[si].pivots[0].anchor;
        a.select(anchor, false);
        a.tab = crate::app::Tab::PivotDesign;
    }
    h.run_steps(3);
    h.get_by_label("Banded Rows").click();
    h.run_steps(2);
    assert!(h.state().book.sheets[si].pivots[0].style.banded_rows);
}

#[test]
fn format_data_series_changes_the_marker() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    {
        let a = h.state_mut();
        a.sel = crate::app::Selection { anchor: parse_a1("B1").unwrap(), active: parse_a1("C5").unwrap(), extra: vec![] };
        crate::chart_ui::insert_variant(a, sheet_engine::chart::ChartKind::Line, sheet_engine::chart::Grouping::Clustered, 1);
        crate::chart_ui::open_format_series(a, 0);
    }
    h.run_steps(3);
    h.get_by_label("Diamond").click();
    h.run_steps(2);
    h.get_by_label("Smoothed line").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let a = h.state();
    let c = a.book.sheets[0].objects[0].chart().unwrap();
    assert_eq!(c.series[0].marker.map(|m| m.symbol), Some(sheet_engine::chart::Symbol::Diamond));
    assert!(c.series[0].smooth);
    let xml = c.to_xml(&a.book, 0);
    assert!(xml.contains("<c:symbol val=\"diamond\"/>") && xml.contains("<c:smooth val=\"1\"/>"));
    // Undo takes it back.
    h.state_mut().undo();
    assert_eq!(h.state().book.sheets[0].objects[0].chart().unwrap().series[0].marker, None);
}

// ---- Insert ▸ Illustrations, Text and Symbols (shapes, icons, SmartArt, equations…) ----

fn pic_count(a: &SheetApp) -> usize { a.book.sheets[0].objects.iter().filter(|o| matches!(o.kind, sheet_engine::drawing::ObjKind::Picture { .. })).count() }

#[test]
fn shape_drawn_by_dragging_on_the_grid() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::ins_shapes::set_pending(&h.ctx, Some("rightArrow"));
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let a = lay.cell_rect(sheet_engine::Pos::new(6, 1)).center();
    let b = a + egui::vec2(160.0, 90.0);
    h.hover_at(a); h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }); h.run_steps(1);
    for k in 1..=6 { h.hover_at(a + (b - a) * (k as f32 / 6.0)); h.run_steps(1); }
    h.event(egui::Event::PointerButton { pos: b, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }); h.run_steps(3);
    let objs = &h.state().book.sheets[0].objects;
    assert_eq!(objs.len(), 1, "one shape drawn");
    let sheet_engine::drawing::ObjKind::Shape(s) = &objs[0].kind else { panic!("a shape") };
    assert_eq!(s.preset, "rightArrow");
    assert!(!s.adjust.is_empty(), "takes the preset's adjust values");
    let r = crate::objects::sheet_rect(&crate::grid::layout(h.state(), h.state().grid_rect), &objs[0]);
    assert!((r[2] - r[0] - 160.0).abs() < 4.0 && (r[3] - r[1] - 90.0).abs() < 4.0, "{r:?}");
    assert!(crate::ins_shapes::pending(&h.ctx).is_none());
}

#[test]
fn illustrations_insert_pictures_that_save() {
    let ctx = egui::Context::default();
    let mut a = SheetApp::blank(ctx.clone());
    assert!(crate::ins_pictures::insert_icon(&mut a, "chartPie").is_some());
    assert!(crate::ins_pictures::insert_equation(&mut a, None, "x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}").is_some());
    let png = crate::ins_pictures::signature_png("Jane Roe", "Manager", true).unwrap();
    crate::ins_pictures::insert_image(&mut a, png, "jadwal-signature:Jane Roe", "Signature Line", "Insert Signature Line", Some((288.0, 144.0)));
    crate::ins_pictures::insert_object_bytes(&mut a, b"hello".to_vec(), "notes.txt", true);
    crate::ins_pictures::insert_model_bytes(&mut a, sheet_engine::model3d::cube_obj().into_bytes(), "cube.obj").unwrap();
    assert_eq!(pic_count(&a), 5);
    crate::ins_shapes::insert_wordart(&mut a, 3, "Big");
    crate::ins_smartart::insert(&mut a, &slides_engine::diagram::Spec::new("basicProcess", "One\nTwo\nThree"));
    let n = a.book.sheets[0].objects.len();
    assert!(n >= 5 + 1 + 1, "{n}");
    // Equations reopen in the editor; embedded objects open with the OS.
    assert!(crate::ins_pictures::open_object(&mut a, 0, 1));
    assert!(matches!(crate::ins_pictures::current(&ctx), Some(crate::ins_pictures::Dlg::Equation { .. })));
    assert!(crate::ins_pictures::open_object(&mut a, 0, 4));
    assert!(matches!(crate::ins_pictures::current(&ctx), Some(crate::ins_pictures::Dlg::Model { .. })));
    // Everything survives saving.
    let back = sheet_engine::xlsx::read(&sheet_engine::xlsx::write(&a.book).unwrap()).unwrap();
    assert_eq!(back.sheets[0].objects.len(), n);
    // Embedded files and signature lines come back after the drawing's objects (they live in the sheet's VML).
    let o = back.sheets[0].objects.iter().find(|o| o.descr.starts_with("jadwal-object:")).expect("embedded object");
    assert_eq!(sheet_engine::embed::carried_part(&back, o, sheet_engine::embed::RT_PACKAGE).unwrap().1, b"hello");
    assert!(sheet_engine::embed::carried_part(&back, &back.sheets[0].objects[0], "svg").is_some());
    // And prints.
    let mut items = vec![];
    for (k, ob) in back.sheets[0].objects.iter().enumerate() { sheet_engine::print::objects::draw_object(&back, 0, ob, [0.0, 0.0, 100.0, 100.0], 1.0, &mut Default::default(), &mut items); let _ = k; }
    assert!(items.len() > n);
}

#[test]
fn symbol_goes_into_the_active_cell() {
    let mut a = app();
    crate::ins_pictures::insert_symbol(&mut a, '€');
    a.commit(None);
    assert_eq!(a.book.display(0, parse_a1("A1").unwrap()), "€");
    assert_eq!(crate::ins_pictures::recent_symbols(&a.ctx)[0], '€');
}

#[test]
fn openverse_results_parse() {
    let j = br#"{"results":[{"title":"Cat","creator":"Ann","license":"by","license_version":"4.0","url":"https://x/cat.jpg","thumbnail":"https://x/t.jpg","foreign_landing_url":"https://x"},{"title":"No url","url":""}]}"#;
    let hits = crate::ins_pictures::parse_results(j).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].license, "CC BY 4.0");
    assert!(crate::ins_pictures::parse_results(b"not json").is_err());
}

#[test]
fn insert_dialogs_render() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    use crate::ins_pictures::{open, Dlg};
    for d in [Dlg::Icons { cat: 2, query: String::new() }, Dlg::Symbol { tab: 0, subset: 0, font: 0, selected: Some('©') }, Dlg::Symbol { tab: 1, subset: 0, font: 0, selected: None },
        Dlg::Equation { target: None, linear: "a^2+b^2=c^2".into() }, Dlg::Signature { signer: "A".into(), title: String::new(), email: String::new(), instructions: String::new(), show_date: true },
        Dlg::Object { icon: true }, Dlg::SmartArt(crate::ins_smartart::Editor::new()), Dlg::Online(Default::default())] {
        open(&h.ctx, d);
        h.run_steps(2);
        assert!(crate::ins_pictures::current(&h.ctx).is_some(), "the dialog stays open");
    }
    // OK in the SmartArt dialog inserts its shapes.
    open(&h.ctx, Dlg::SmartArt(crate::ins_smartart::Editor::new()));
    h.run_steps(2);
    h.get_by_label(&crate::i18n::tr("OK")).click();
    h.run_steps(2);
    // (one group of shapes)
    assert!(matches!(h.state().book.sheets[0].objects.last().map(|o| &o.kind), Some(sheet_engine::drawing::ObjKind::Group(g)) if g.children.len() >= 3));
}

#[test]
fn sparklines_insert_draw_and_tab() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    // Select the sales column and insert column sparklines through the Insert tab's hook.
    h.state_mut().select(sheet_engine::Pos::new(1, 2), false);
    h.state_mut().select(sheet_engine::Pos::new(4, 2), true);
    crate::insert_hooks::sparklines(h.state_mut(), 1);
    h.run_steps(2);
    assert!(crate::sparkline_ui::dialog_open(), "Create Sparklines opens");
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert!(!crate::sparkline_ui::dialog_open());
    let s = &h.state().book.sheets[0];
    assert_eq!(s.sparklines.len(), 1);
    assert_eq!(s.sparklines[0].kind, sheet_engine::sparkline::Kind::Column);
    assert_eq!(s.sparklines[0].sparklines[0].loc, sheet_engine::Pos::new(5, 2), "placed below the column of data");
    // The active cell on the sparkline brings the Sparkline tab; its Win/Loss button retypes.
    h.state_mut().select(sheet_engine::Pos::new(5, 2), false);
    h.state_mut().tab = crate::app::Tab::Sparkline;
    h.run_steps(2);
    h.get_by_label("Win/Loss").click();
    h.run_steps(2);
    assert_eq!(h.state().book.sheets[0].sparklines[0].kind, sheet_engine::sparkline::Kind::WinLoss);
    // It draws: the layout of the cell's sparkline has bars.
    let a = h.state();
    let g = &a.book.sheets[0].sparklines[0];
    let prims = sheet_engine::sparkline::layout(&a.book, 0, g, &g.sparklines[0], [0.0, 0.0, 64.0, 20.0], 1.0);
    assert!(prims.iter().filter(|p| matches!(p, sheet_engine::chart::Prim::Rect { .. })).count() >= 4);
    // Leaving the cell leaves the tab.
    h.state_mut().select(sheet_engine::Pos::new(0, 0), false);
    h.run_steps(2);
    assert_ne!(h.state().tab, crate::app::Tab::Sparkline);
}

#[test]
fn picture_in_cell_and_more_charts() {
    let mut a = data_app();
    a.ctx = egui::Context::default();
    let png = { let mut b = vec![]; image::RgbaImage::from_pixel(4, 2, image::Rgba([200, 0, 0, 255])).write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Png).unwrap(); b };
    a.sel = crate::app::Selection { active: sheet_engine::Pos::new(7, 0), anchor: sheet_engine::Pos::new(7, 0), extra: vec![] };
    crate::sparkline_ui::place_picture(&mut a, &png, "Red");
    assert!(sheet_engine::cellimage::is_image(&a.book.sheets[0].value(sheet_engine::Pos::new(7, 0))));
    a.sel = crate::app::Selection { active: sheet_engine::Pos::new(0, 1), anchor: sheet_engine::Pos::new(4, 2), extra: vec![] };
    crate::sparkline_ui::insert_ex_chart(&mut a, sheet_engine::chart::ChartKind::Treemap);
    assert_eq!(a.book.sheets[0].objects.last().and_then(|o| o.chart()).map(|c| c.kind), Some(sheet_engine::chart::ChartKind::Treemap));
    let mut h = harness(a);
    h.run_steps(3);
}

// ---- timelines, PivotCharts, maps, page views, scripts ----

fn click(h: &mut egui_kittest::Harness<'static, SheetApp>, at: egui::Pos2) {
    let press = |down| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: down, modifiers: Default::default() };
    h.event(egui::Event::PointerMoved(at)); h.run_steps(1);
    h.event(press(true)); h.run_steps(1);
    h.event(press(false)); h.run_steps(3);
}

fn date_app() -> SheetApp {
    let mut a = SheetApp::blank(egui::Context::default());
    for (r, row) in [["Date", "Sales"], ["2024-01-15", "10"], ["2024-02-03", "20"], ["2024-03-20", "5"], ["2025-03-20", "7"]].iter().enumerate() {
        for (c, t) in row.iter().enumerate() { a.book.set_input(0, sheet_engine::Pos::new(r as u32, c as u32), t); }
    }
    a.book.recalc();
    a
}

fn grand_total(a: &SheetApp, si: usize) -> f64 {
    let p = &a.book.sheets[si].pivots[0];
    let o = p.output.unwrap();
    (o.r0..=o.r1).flat_map(|r| (o.c0..=o.c1).map(move |c| sheet_engine::Pos::new(r, c))).filter_map(|q| if let sheet_engine::Value::Num(n) = a.book.sheets[si].value(q) { Some(n) } else { None }).fold(0.0, f64::max)
}

#[test]
fn timeline_inserts_and_filters_the_pivot() {
    let (mut h, si) = pivot_harness(date_app());
    {
        let a = h.state_mut();
        a.book.sheets[si].pivots[0].rows = vec![];
        a.book.sheets[si].pivots[0].values = vec![sheet_engine::pivot::ValueField::new(1, sheet_engine::pivot::Agg::Sum)];
        crate::pivot_ui::rerun(a, si, 0);
        let anchor = a.book.sheets[si].pivots[0].anchor;
        a.select(sheet_engine::Pos::new(anchor.row + 1, anchor.col), false);
        crate::insert_hooks::timeline(a);
    }
    h.run_steps(3);
    assert_eq!(grand_total(h.state(), si), 42.0);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let i = h.state().book.sheets[si].objects.iter().position(|o| matches!(o.kind, sheet_engine::drawing::ObjKind::Timeline(_))).expect("a timeline");
    // Click the first period (January 2024) on the strip.
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let r = crate::objects::screen_rect(&lay, &h.state().book.sheets[si].objects[i]);
    let at = egui::pos2(r.min.x + 22.0 + 19.0, r.min.y + 46.0 + 23.0);
    click(&mut h, at);
    let a = h.state();
    assert!(matches!(&a.book.sheets[si].objects[i].kind, sheet_engine::drawing::ObjKind::Timeline(t) if t.selection.is_some()));
    assert_eq!(grand_total(a, si), 10.0, "only January 2024 counts");
    // Drag across January–March selects the quarter's months.
    drag(&mut h, at, egui::pos2(at.x + 2.0 * 38.0, at.y));
    assert_eq!(grand_total(h.state(), si), 35.0);
    // Clear Filter.
    let at = egui::pos2(r.max.x - 17.0, r.min.y + 14.0);
    click(&mut h, at);
    assert_eq!(grand_total(h.state(), si), 42.0);
    // Saved and read back.
    let back = sheet_engine::xlsx::read(&sheet_engine::xlsx::write(&h.state().book).unwrap()).unwrap();
    assert!(back.sheets[si].objects.iter().any(|o| matches!(o.kind, sheet_engine::drawing::ObjKind::Timeline(_))));
}

#[test]
fn pivot_chart_follows_its_pivot() {
    let (mut h, si) = pivot_harness(data_app());
    {
        let a = h.state_mut();
        a.book.sheets[si].pivots[0].rows = vec![0];
        a.book.sheets[si].pivots[0].values = vec![sheet_engine::pivot::ValueField::new(2, sheet_engine::pivot::Agg::Sum)];
        crate::pivot_ui::rerun(a, si, 0);
        let anchor = a.book.sheets[si].pivots[0].anchor;
        a.select(sheet_engine::Pos::new(anchor.row + 1, anchor.col), false);
        crate::pivot_chart_ui::insert_for_active(a);
    }
    h.run_steps(3);
    let chart = |a: &SheetApp| a.book.sheets[si].objects.iter().find_map(|o| o.chart().filter(|c| c.pivot_source.is_some()).cloned()).expect("pivot chart");
    assert_eq!(chart(h.state()).series[0].val_cache, vec![15.0, 27.0]);
    {
        let a = h.state_mut();
        a.book.sheets[si].pivots[0].hidden.insert(0, ["West".to_string()].into());
        crate::pivot_ui::rerun(a, si, 0);
    }
    h.run_steps(2);
    assert_eq!(chart(h.state()).series[0].val_cache, vec![15.0]);
    assert!(chart(h.state()).series[0].cat_cache == vec!["East"]);
}

#[test]
fn filled_map_inserts_and_redraws() {
    let mut a = SheetApp::blank(egui::Context::default());
    for (r, row) in [["Country", "Sales"], ["France", "10"], ["Japan", "30"], ["Brazil", "20"]].iter().enumerate() {
        for (c, t) in row.iter().enumerate() { a.book.set_input(0, sheet_engine::Pos::new(r as u32, c as u32), t); }
    }
    a.book.recalc();
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::maps_ui::insert_filled_map(h.state_mut());
    h.run_steps(2);
    let pic = |a: &SheetApp| a.book.sheets[0].objects.iter().find_map(|o| match &o.kind { sheet_engine::drawing::ObjKind::Picture { data, .. } if o.descr.starts_with("jadwal:filledmap") => Some(data.clone()), _ => None });
    let first = pic(h.state()).expect("a map");
    {
        let a = h.state_mut();
        a.book.set_input(0, parse_a1("B2").unwrap(), "99");
        a.changed();
    }
    h.run_steps(2);
    assert_ne!(*pic(h.state()).unwrap(), *first, "the map redraws when its data changes");
    let back = sheet_engine::xlsx::read(&sheet_engine::xlsx::write(&h.state().book).unwrap()).unwrap();
    assert!(back.sheets[0].objects.iter().any(|o| sheet_engine::geo::FilledMap::from_tag(&o.descr).is_some()));
}

#[test]
fn map_view_tours_and_captures() {
    let mut a = SheetApp::blank(egui::Context::default());
    for (r, row) in [["City", "Lat", "Lon", "Visitors"], ["Paris", "48.85", "2.35", "100"], ["Tokyo", "35.68", "139.69", "80"]].iter().enumerate() {
        for (c, t) in row.iter().enumerate() { a.book.set_input(0, sheet_engine::Pos::new(r as u32, c as u32), t); }
    }
    a.book.recalc();
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::insert_hooks::map_3d(h.state_mut());
    h.run_steps(3);
    let v = crate::maps_ui::view_state(&h.ctx).expect("map view");
    assert_eq!(v.loc_col, 0);
    assert_eq!(v.scenes[0].value_col, 3);
    h.get_by_label("New Scene").click(); h.run_steps(2);
    assert_eq!(crate::maps_ui::view_state(&h.ctx).unwrap().scenes.len(), 2);
    h.get_by_label("Bubble").click(); h.run_steps(2);
    h.get_by_label("Capture Screen").click(); h.run_steps(3);
    assert_eq!(pic_count(h.state()), 1);
}

#[test]
fn header_footer_in_page_layout_view() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::insert_hooks::header_footer(h.state_mut());
    h.run_steps(3);
    assert_eq!(crate::page_view::mode(), crate::page_view::ViewMode::PageLayout);
    assert_eq!(h.state().tab, crate::app::Tab::HeaderFooter);
    h.get_by_label("Page Number").click(); h.run_steps(2);
    h.get_by_label("Different First Page").click(); h.run_steps(2);
    h.get_by_label("Close Header and Footer").click(); h.run_steps(2);
    let hf = &h.state().book.sheets[0].page.header_footer;
    assert!(hf.different_first);
    assert!(hf.first_header.contains("&P") || hf.odd_header.contains("&P"), "{hf:?}");
    // The View tab switches views.
    h.state_mut().tab = crate::app::Tab::View;
    h.run_steps(2);
    h.get_by_label("Normal").click(); h.run_steps(2);
    assert_eq!(crate::page_view::mode(), crate::page_view::ViewMode::Normal);
}

#[test]
fn page_break_preview_moves_a_break() {
    let mut a = SheetApp::blank(egui::Context::default());
    for r in 0..120 { a.book.set_input(0, sheet_engine::Pos::new(r, 0), &r.to_string()); }
    a.book.recalc();
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().zoom = 0.5;
    crate::page_view::set_mode(h.state_mut(), crate::page_view::ViewMode::PageBreak);
    h.run_steps(3);
    let (rows, _) = sheet_engine::print::break_lines(&h.state().book, 0);
    let first = *rows.iter().next().expect("a page break");
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let y = lay.y(first);
    let x = lay.view().min.x + 30.0;
    drag(&mut h, egui::pos2(x, y), egui::pos2(x, lay.y(first - 10)));
    let rb = &h.state().book.sheets[0].page.row_breaks;
    assert!(rb.contains(&(first - 10)), "{rb:?}");
    crate::page_view::set_mode(h.state_mut(), crate::page_view::ViewMode::Normal);
}

#[test]
fn scripts_pane_runs_records_and_saves() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::insert_hooks::scripts(h.state_mut());
    h.run_steps(2);
    h.get_by_label("Samples").click(); h.run_steps(2);
    h.state_mut().select(parse_a1("A2").unwrap(), false);
    h.state_mut().select(parse_a1("A3").unwrap(), true);
    // The first sample: Highlight selection.
    h.get_all_by_label("Run").next().unwrap().click(); h.run_steps(2);
    assert!(h.state().book.style_at(0, parse_a1("A3").unwrap()).bold);
    h.state_mut().undo();
    assert!(!h.state().book.style_at(0, parse_a1("A3").unwrap()).bold);
    // Record a change, stop, save it to the workbook.
    h.get_by_label("● Record Actions").click(); h.run_steps(2);
    { let a = h.state_mut(); a.checkpoint("Typing"); a.book.set_input(0, parse_a1("D1").unwrap(), "Total"); a.changed(); }
    h.run_steps(1);
    h.get_by_label("■ Stop Recording").click(); h.run_steps(2);
    h.get_by_label("Save").click(); h.run_steps(2);
    let list = sheet_engine::scripts::load(&h.state().book);
    assert_eq!(list.len(), 1);
    assert!(list[0].code.contains("set D1 Total"), "{}", list[0].code);
    crate::scripts_ui::PANE.with(|p| p.borrow_mut().open = false);
}

#[test]
fn pointing_a_range_in_a_formula() {
    let mut h = harness(data_app());
    h.run_steps(3);
    h.state_mut().select(sheet_engine::Pos::new(10, 2), false);
    h.run_steps(2);
    h.event(egui::Event::Text("=SUM(".into())); h.run_steps(3);
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let a = lay.cell_rect(sheet_engine::Pos::new(3, 2)).center();
    let b = lay.cell_rect(sheet_engine::Pos::new(6, 2)).center();
    drag(&mut h, a, b);
    assert_eq!(h.state().edit.as_ref().unwrap().text, "=SUM(C4:C7");
    // Clicking another cell replaces the reference just pointed at.
    let c = lay.cell_rect(sheet_engine::Pos::new(2, 3)).center();
    let press = |p, down| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: down, modifiers: Default::default() };
    h.event(egui::Event::PointerMoved(c)); h.run_steps(1);
    h.event(press(c, true)); h.run_steps(1); h.event(press(c, false)); h.run_steps(2);
    assert_eq!(h.state().edit.as_ref().unwrap().text, "=SUM(D3");
    h.event(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }); h.run_steps(2);
    assert_eq!(h.state().book.display(0, sheet_engine::Pos::new(10, 2)), "0");
}

// ---- Formulas tab ----

fn formulas_harness(a: SheetApp) -> egui_kittest::Harness<'static, SheetApp> {
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().tab = crate::app::Tab::Formulas;
    h.run_steps(2);
    h
}

#[test]
fn insert_function_then_function_arguments() {
    let mut h = formulas_harness(data_app());
    h.state_mut().select(parse_a1("C7").unwrap(), false);
    h.get_by_label("Insert Function").click(); h.run_steps(3);
    assert!(matches!(h.state().dialog, Some(crate::app::Dialog::Function { .. })));
    if let Some(crate::app::Dialog::Function { picked, .. }) = &mut h.state_mut().dialog { *picked = Some("SUM"); }
    h.run_steps(2);
    h.get_by_label("OK").click(); h.run_steps(3);
    let st = crate::formulas_tab::state(h.state());
    assert!(matches!(st.dlg, Some(crate::formulas_tab::Dlg::Args { ref name, .. }) if name == "SUM"));
    // Fill number1 and check the live result, then OK.
    let mut st = crate::formulas_tab::state(h.state());
    if let Some(crate::formulas_tab::Dlg::Args { args, .. }) = &mut st.dlg { args[0] = "C2:C5".into(); }
    h.state().ctx.data_mut(|d| d.insert_temp(egui::Id::unique("formulas-tab-state"), st));
    h.run_steps(2);
    let _ = h.get_by_label("Formula result = 42");
    h.get_by_label("OK").click(); h.run_steps(3);
    assert_eq!(h.state().book.edit_text(0, parse_a1("C7").unwrap()), "=SUM(C2:C5)");
    assert_eq!(h.state().book.display(0, parse_a1("C7").unwrap()), "42");
    assert_eq!(crate::formulas_tab::state(h.state()).recent.first().map(String::as_str), Some("SUM"));
}

#[test]
fn trace_precedents_and_dependents_arrows() {
    let mut a = data_app();
    a.book.set_input(0, parse_a1("C6").unwrap(), "=SUM(C2:C5)");
    a.book.set_input(0, parse_a1("C8").unwrap(), "=C6*2");
    a.book.recalc();
    let mut h = formulas_harness(a);
    h.state_mut().select(parse_a1("C8").unwrap(), false);
    h.run_steps(2);
    crate::formulas_tab::trace(h.state_mut(), false); h.run_steps(2);
    assert_eq!(crate::formulas_tab::state(h.state()).arrows.len(), 1);
    crate::formulas_tab::trace(h.state_mut(), false); h.run_steps(2);
    assert_eq!(crate::formulas_tab::state(h.state()).arrows.len(), 2, "a second click goes one level further");
    h.state_mut().select(parse_a1("C3").unwrap(), false);
    crate::formulas_tab::trace(h.state_mut(), true); h.run_steps(2);
    assert_eq!(crate::formulas_tab::state(h.state()).arrows.len(), 3);
    crate::formulas_tab::remove_arrows(h.state_mut(), Some(false));
    assert_eq!(crate::formulas_tab::state(h.state()).arrows.len(), 1);
    crate::formulas_tab::remove_arrows(h.state_mut(), None);
    assert!(crate::formulas_tab::state(h.state()).arrows.is_empty());
}

#[test]
fn error_smart_tag_converts_number_stored_as_text() {
    let mut a = data_app();
    a.book.set_input(0, parse_a1("E3").unwrap(), "'12");
    a.book.recalc();
    let mut h = formulas_harness(a);
    h.state_mut().select(parse_a1("E3").unwrap(), false);
    h.run_steps(3);
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let r = lay.cell_rect(parse_a1("E3").unwrap());
    let tag = egui::pos2(r.min.x - 11.0, r.min.y + 9.0);
    let press = |p, down| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: down, modifiers: Default::default() };
    h.event(egui::Event::PointerMoved(tag)); h.run_steps(1);
    h.event(press(tag, true)); h.run_steps(1); h.event(press(tag, false)); h.run_steps(3);
    h.get_by_label("Convert to Number").click(); h.run_steps(2);
    assert_eq!(h.state().book.sheets[0].value(parse_a1("E3").unwrap()), sheet_engine::Value::Num(12.0));
}

#[test]
fn evaluate_formula_steps_and_error_checking() {
    let mut a = data_app();
    a.book.set_input(0, parse_a1("E2").unwrap(), "=C2+C3*2");
    a.book.set_input(0, parse_a1("E4").unwrap(), "=1/0");
    a.book.recalc();
    let mut h = formulas_harness(a);
    h.state_mut().select(parse_a1("E2").unwrap(), false);
    { let p = h.state().sel.active; crate::formulas_tab::open_evaluate(h.state_mut(), 0, p); } h.run_steps(2);
    let idx = |h: &egui_kittest::Harness<'static, SheetApp>| match crate::formulas_tab::state(h.state()).dlg { Some(crate::formulas_tab::Dlg::Evaluate { stack }) => stack[0].3, _ => usize::MAX };
    assert_eq!(idx(&h), 0);
    for k in 1..=4 { h.get_by_label("Evaluate").click(); h.run_steps(2); assert_eq!(idx(&h), k); }
    let _ = h.get_by_label("Restart");
    h.get_all_by_label("Close").last().unwrap().click(); h.run_steps(2);
    assert!(crate::formulas_tab::state(h.state()).dlg.is_none());
    crate::formulas_tab::open_error_checking(h.state_mut()); h.run_steps(2);
    assert_eq!(h.state().sel.active, parse_a1("E4").unwrap());
    h.get_by_label("Ignore Error").click(); h.run_steps(2);
    assert!(crate::formulas_tab::state(h.state()).ignored.contains(&(0, parse_a1("E4").unwrap())));
}

#[test]
fn names_show_formulas_watch_and_calculation() {
    let mut h = formulas_harness(data_app());
    h.state_mut().sel = crate::app::Selection { anchor: parse_a1("A1").unwrap(), active: parse_a1("C5").unwrap(), extra: vec![] };
    crate::formulas_tab::open_create_names(h.state_mut()); h.run_steps(2);
    h.get_by_label("OK").click(); h.run_steps(2);
    let names: Vec<String> = h.state().book.names.iter().map(|n| n.name.clone()).collect();
    assert!(names.contains(&"Sales".to_string()), "{names:?}");
    assert_eq!(sheet_engine::formula::eval::evaluate_in(&h.state().book, 0, Default::default(), "SUM(Sales)"), sheet_engine::Value::Num(42.0));
    // Name Manager lists them; New Name with a sheet scope.
    h.get_by_label("Name Manager").click(); h.run_steps(2);
    let _ = h.get_by_label("Sales");
    h.get_by_label("New…").click(); h.run_steps(2);
    let mut st = crate::formulas_tab::state(h.state());
    if let Some(crate::formulas_tab::Dlg::NewName { name, scope, refers, .. }) = &mut st.dlg { *name = "TopSale".into(); *scope = Some(0); *refers = "=Sheet1!$C$2".into(); }
    h.state().ctx.data_mut(|d| d.insert_temp(egui::Id::unique("formulas-tab-state"), st));
    h.run_steps(2);
    h.get_by_label("OK").click(); h.run_steps(2);
    assert!(h.state().book.names.iter().any(|n| n.name == "TopSale" && n.sheet == Some(0)));
    h.get_all_by_label("Close").last().unwrap().click(); h.run_steps(2);
    // ⌘` shows formulas.
    h.event(egui::Event::Key { key: egui::Key::Backtick, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }); h.run_steps(2);
    assert!(crate::formulas_tab::showing_formulas(h.state()));
    // Watch Window: add the selection.
    h.state_mut().select(parse_a1("C2").unwrap(), false);
    h.get_by_label("Watch Window").click(); h.run_steps(2);
    h.get_by_label("Add Watch…").click(); h.run_steps(2);
    assert_eq!(crate::formulas_tab::state(h.state()).watches, vec![(0, parse_a1("C2").unwrap())]);
    crate::formulas_tab::set_calc_mode(h.state_mut(), 2);
    assert!(h.state().book.manual);
    crate::formulas_tab::set_calc_mode(h.state_mut(), 1);
    assert!(!h.state().book.manual && h.state().book.calc_no_tables);
}

// ---- Page Layout: themes, background, arranging objects ----

fn click_mod(h: &mut egui_kittest::Harness<'static, SheetApp>, at: egui::Pos2, modifiers: egui::Modifiers) {
    let press = |down| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: down, modifiers };
    h.event(egui::Event::ModifiersChanged(modifiers));
    h.event(egui::Event::PointerMoved(at)); h.run_steps(1);
    h.event(press(true)); h.run_steps(1);
    h.event(press(false)); h.run_steps(1);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(2);
}

fn two_shapes() -> SheetApp {
    use sheet_engine::drawing::{Anchor, ObjKind, Object, Shape};
    let mut a = data_app();
    for (x, fill) in [(300.0, [200, 0, 0]), (480.0, [0, 0, 200])] {
        let o = Object::new(ObjKind::Shape(Shape { preset: "rect".into(), fill: Some(fill), size: 11.0, ..Default::default() }), Anchor::Abs { x, y: 200.0, w: 100.0, h: 60.0 });
        sheet_engine::drawing::add_object(&mut a.book.sheets[0].objects, o, "Shape");
    }
    a
}

#[test]
fn select_two_shapes_group_and_move_the_group() {
    use sheet_engine::drawing::ObjKind;
    let mut h = harness(two_shapes());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().tab = crate::app::Tab::PageLayout;
    h.run_steps(2);
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let r0 = crate::objects::screen_rect(&lay, &h.state().book.sheets[0].objects[0]);
    let r1 = crate::objects::screen_rect(&lay, &h.state().book.sheets[0].objects[1]);
    click_mod(&mut h, r0.center(), egui::Modifiers::NONE);
    click_mod(&mut h, r1.center(), egui::Modifiers::SHIFT);
    assert_eq!(crate::arrange::selection(h.state()), vec![0, 1], "Shift-click adds to the selection");
    // Arrange ▸ Group ▸ Group (the menu opens; the command groups the selection).
    h.get_by_label_contains("Group   ").click();
    h.run_steps(2);
    crate::arrange::group_selection(h.state_mut());
    h.run_steps(3);
    let objs = &h.state().book.sheets[0].objects;
    assert_eq!(objs.len(), 1, "two shapes became one group");
    assert!(matches!(&objs[0].kind, ObjKind::Group(g) if g.children.len() == 2));
    // Dragging the group moves both members.
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let g0 = crate::objects::screen_rect(&lay, &h.state().book.sheets[0].objects[0]);
    let from = r0.center();
    drag(&mut h, from, from + egui::vec2(40.0, 60.0));
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let g1 = crate::objects::screen_rect(&lay, &h.state().book.sheets[0].objects[0]);
    assert!(((g1.min - g0.min) - egui::vec2(40.0, 60.0)).length() < 3.0, "group moved: {g0:?} -> {g1:?}");
    assert!((g1.width() - g0.width()).abs() < 2.0);
    // Ungroup gives the shapes back where they now show.
    crate::arrange::ungroup(h.state_mut());
    let s = &h.state().book.sheets[0];
    assert_eq!(s.objects.len(), 2);
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let m0 = crate::objects::sheet_rect(&lay, &s.objects[0]);
    assert!((m0[0] - 340.0).abs() < 3.0 && (m0[1] - 260.0).abs() < 3.0, "{m0:?}");
    // Rotate and the Selection Pane render.
    crate::arrange::turn(h.state_mut(), 90.0, false, false, "Rotate");
    crate::arrange::toggle_pane(h.state());
    h.run_steps(3);
    assert_eq!(h.state().book.sheets[0].objects.iter().filter(|o| o.rot == 90.0).count(), 2);
}

#[test]
fn smartart_inserts_as_one_group() {
    use sheet_engine::drawing::ObjKind;
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    crate::ins_smartart::insert(h.state_mut(), &slides_engine::diagram::Spec::new("basicProcess", "One\nTwo\nThree"));
    h.run_steps(2);
    let objs = &h.state().book.sheets[0].objects;
    assert_eq!(objs.len(), 1);
    let ObjKind::Group(g) = &objs[0].kind else { panic!("SmartArt isn't a group") };
    assert!(g.children.len() >= 3);
    // Clicking one box selects the whole graphic.
    crate::objects::select(h.state(), None);
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let gr = crate::objects::screen_rect(&lay, &objs[0]);
    let first = g.child_rect(&g.children[0], [gr.min.x, gr.min.y, gr.max.x, gr.max.y]);
    click_mod(&mut h, egui::pos2((first[0] + first[2]) / 2.0, (first[1] + first[3]) / 2.0), egui::Modifiers::NONE);
    assert_eq!(crate::objects::selected(h.state()), Some(0));
    // It saves as one grpSp and reads back as a group.
    let back = sheet_engine::xlsx::read(&sheet_engine::xlsx::write(&h.state().book).unwrap()).unwrap();
    assert!(matches!(&back.sheets[0].objects[0].kind, ObjKind::Group(g) if g.children.len() >= 3));
}

#[test]
fn applying_a_theme_recolors_a_cell_style() {
    use sheet_engine::theme::ThemeColor;
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().tab = crate::app::Tab::PageLayout;
    // A themed fill (Accent 1, 40% lighter), as the Fill Color picker's theme swatches give.
    let tc = ThemeColor::new(4, 0.6);
    let c = tc.resolve(&h.state().book.theme_colors);
    h.state_mut().apply_style("Fill Color", move |s| { s.fill = Some(c); s.fill_th = Some(tc); });
    let p = h.state().sel.active;
    let before = h.state().book.style_at(0, p).fill;
    h.run_steps(2);
    let t = sheet_engine::theme::builtins().into_iter().find(|t| t.name == "Office 2007 - 2010 Theme").unwrap();
    h.state_mut().checkpoint("Theme");
    sheet_engine::theme::apply(&mut h.state_mut().book, &t);
    h.run_steps(2);
    let after = h.state().book.style_at(0, p).fill;
    assert_ne!(before, after);
    assert_eq!(after, Some(tc.resolve(&t.colors.colors)));
    // Undo puts the old theme and color back.
    h.state_mut().undo();
    assert_eq!(h.state().book.style_at(0, p).fill, before);
    // A background picture paints without trouble.
    let png = crate::ins_pictures::signature_png("A", "B", false).unwrap();
    crate::page_layout::set_background(h.state_mut(), png);
    h.run_steps(2);
    assert!(sheet_engine::sheet_background::get(&h.state().book, 0).is_some());
}

// ---- Review and View tabs ----------------------------------------------------------------------

fn key(h: &mut egui_kittest::Harness<'static, SheetApp>, k: egui::Key) {
    h.event(egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }); h.run_steps(1);
    h.event(egui::Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: Default::default() }); h.run_steps(2);
}

#[test]
fn spelling_dialog_changes_a_word() {
    let mut a = SheetApp::blank(egui::Context::default());
    a.book.set_input(0, parse_a1("B2").unwrap(), "a speling mistake");
    a.book.set_input(0, parse_a1("B3").unwrap(), "all good here");
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    key(&mut h, egui::Key::F7);
    h.run_steps(2);
    let issue = h.state().rv.spelling.as_ref().and_then(|s| s.issue.clone()).expect("a misspelling is found");
    assert_eq!((issue.0, issue.2.as_str()), (parse_a1("B2").unwrap(), "speling"));
    assert!(h.state().rv.spelling.as_ref().unwrap().suggestions.iter().any(|s| s == "spelling"));
    h.state_mut().rv.spelling.as_mut().unwrap().change_to = "spelling".into();
    h.run_steps(4);
    h.get_by_label("Change").click();
    h.run_steps(4);
    assert_eq!(h.state().book.display(0, parse_a1("B2").unwrap()), "a spelling mistake");
    assert!(h.state().rv.spelling.as_ref().is_some_and(|s| s.done), "nothing else is misspelled");
    h.run_steps(4);
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert!(h.state().rv.spelling.is_none());
}

#[test]
fn protect_sheet_blocks_an_edit() {
    let mut a = data_app();
    // B2 is unlocked: it stays editable.
    a.book.apply_style(0, &[sheet_engine::Range::cell(parse_a1("B2").unwrap())], &|s| s.locked = false);
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().tab = crate::app::Tab::Review;
    h.run_steps(2);
    h.get_by_label("Protect Sheet").click();
    h.run_steps(2);
    h.state_mut().rv.protect.as_mut().unwrap().password = "pw".into();
    h.state_mut().rv.protect.as_mut().unwrap().confirm = "pw".into();
    h.run_steps(4);
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert!(h.state().book.sheets[0].protected);
    assert!(h.state().book.sheets[0].review.protection.password.as_ref().unwrap().matches("pw"));
    // Typing over a locked cell is refused.
    h.state_mut().select(parse_a1("A2").unwrap(), false);
    h.event(egui::Event::Text("x".into())); h.run_steps(2);
    key(&mut h, egui::Key::Enter);
    assert_eq!(h.state().book.display(0, parse_a1("A2").unwrap()), "East");
    assert!(h.state().error.is_some());
    h.state_mut().error = None;
    // Formatting and sorting aren't allowed either.
    h.state_mut().apply_style("Bold", |s| s.bold = true);
    assert!(!h.state().style().bold && h.state().error.is_some());
    h.state_mut().error = None;
    // The unlocked cell takes the entry.
    h.state_mut().select(parse_a1("B2").unwrap(), false);
    h.run_steps(1);
    h.event(egui::Event::Text("y".into())); h.run_steps(2);
    key(&mut h, egui::Key::Enter);
    assert_eq!(h.state().book.display(0, parse_a1("B2").unwrap()), "y");
    // Unprotecting needs the password.
    assert!(!sheet_engine::review::unprotect_sheet(&mut h.state_mut().book.sheets[0], "nope"));
}

#[test]
fn split_panes_scroll_and_drag() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().select(sheet_engine::Pos::new(6, 3), false);
    crate::view_tab::toggle_split(h.state_mut());
    h.run_steps(3);
    let (x0, y0) = h.state().book.sheets[0].review.split.expect("split");
    assert!(x0 > 0.0 && y0 > 0.0);
    // The bottom-right pane scrolls on its own; the top panes keep showing the first rows.
    h.state_mut().scroll = egui::vec2(0.0, 400.0);
    h.run_steps(2);
    assert_eq!(h.state().rv.view.split_scroll, egui::Vec2::ZERO);
    assert_eq!(h.state().scroll.y, 400.0);
    // Drag the vertical bar left.
    let br = h.state().grid_rect;
    let at = egui::pos2(br.min.x, br.center().y);
    drag(&mut h, at, at - egui::vec2(60.0, 0.0));
    let (x1, _) = h.state().book.sheets[0].review.split.expect("still split");
    assert!((x0 - x1 - 60.0).abs() < 8.0, "{x0} → {x1}");
    // Clicking a cell in the bottom-right pane selects it.
    let br = h.state().grid_rect;
    let lay = crate::grid::layout(h.state(), br);
    let target = lay.cell_at(br.center());
    click(&mut h, br.center());
    assert_eq!(h.state().sel.active, target);
    // And it's saved as a split pane.
    let bytes = sheet_engine::xlsx::write(&h.state().book).unwrap();
    assert!(sheet_engine::xlsx::read(&bytes).unwrap().sheets[0].review.split.is_some());
    crate::view_tab::toggle_split(h.state_mut());
    assert!(h.state().book.sheets[0].review.split.is_none());
}

#[test]
fn autocomplete_fills_from_the_column() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().select(parse_a1("A6").unwrap(), false);
    h.event(egui::Event::Text("W".into())); h.run_steps(2);
    assert_eq!(crate::review_tab::completion(h.state()).as_deref(), Some("West"));
    key(&mut h, egui::Key::Enter);
    assert_eq!(h.state().book.display(0, parse_a1("A6").unwrap()), "West");
    // "E" matches East only; Backspace turns the completion off for what's left.
    h.event(egui::Event::Text("Ea".into())); h.run_steps(2);
    assert_eq!(crate::review_tab::completion(h.state()).as_deref(), Some("East"));
    key(&mut h, egui::Key::Backspace);
    key(&mut h, egui::Key::Enter);
    assert_eq!(h.state().book.display(0, parse_a1("A7").unwrap()), "E");
    // AutoCorrect fixes a typo as the entry is made.
    h.event(egui::Event::Text("teh end".into())); h.run_steps(2);
    key(&mut h, egui::Key::Enter);
    assert_eq!(h.state().book.display(0, parse_a1("A8").unwrap()), "the end");
}

#[test]
fn review_and_view_tabs_render() {
    let mut a = data_app();
    sheet_engine::review::add_comment(&mut a.book, 0, parse_a1("C2").unwrap(), "Ann", "Check this");
    a.book.sheets[0].notes.insert(parse_a1("C3").unwrap(), "a note".into());
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().tab = crate::app::Tab::Review; h.run_steps(2);
    h.get_by_label("Show Comments").click(); h.run_steps(2);
    h.get_by_label("Check Accessibility").click(); h.run_steps(2);
    h.state_mut().rv.stats = true; h.run_steps(2);
    let _ = h.get_by_label("End of sheet");
    h.state_mut().rv.stats = false;
    h.state_mut().tab = crate::app::Tab::View; h.run_steps(2);
    h.get_by_label("Zoom").click(); h.run_steps(4);
    h.get_by_label("Cancel").click(); h.run_steps(2);
    assert!(h.state().rv.view.zoom_dialog.is_none());
    h.get_by_label("Custom Views").click(); h.run_steps(4);
    h.get_by_label("Add…").click(); h.run_steps(4);
    h.state_mut().rv.view.views_dialog.as_mut().unwrap().1 = "Mine".into();
    h.run_steps(4);
    h.get_by_label("OK").click(); h.run_steps(2);
    assert_eq!(h.state().book.review.views.len(), 1);
    // Tell Me runs a command.
    h.state_mut().rv.view.tell_me = "gridl".into();
    h.run_steps(2);
    let cmds = crate::view_tab::commands();
    assert!(cmds.iter().any(|c| c.label == "Gridlines"));
}

fn data_tab_harness(a: SheetApp) -> egui_kittest::Harness<'static, SheetApp> {
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().tab = crate::app::Tab::Data;
    h.run_steps(2);
    h
}

#[test]
fn goal_seek_from_the_data_tab() {
    let mut a = SheetApp::blank(egui::Context::default());
    a.book.set_input(0, parse_a1("A1").unwrap(), "2");
    a.book.set_input(0, parse_a1("B1").unwrap(), "=A1*A1");
    a.book.recalc();
    a.select(parse_a1("B1").unwrap(), false);
    let mut h = data_tab_harness(a);
    h.get_by_label("What-If Analysis").click(); h.run_steps(2);
    h.get_by_label("Goal Seek…").click(); h.run_steps(2);
    crate::data_tab::with(h.state(), |s| if let Some(crate::data_tab::Dlg::GoalSeek(g)) = &mut s.dlg { assert_eq!(g.set, "$B$1"); g.to = "49".into(); g.by = "A1".into(); } else { panic!("Goal Seek is not open") });
    h.run_steps(2);
    h.get_by_label("OK").click(); h.run_steps(3);
    assert!((h.state().book.sheets[0].value(parse_a1("A1").unwrap()).to_num().unwrap() - 7.0).abs() < 1e-3);
    assert!(matches!(crate::data_tab::state(h.state()).dlg, Some(crate::data_tab::Dlg::GoalSeek(ref g)) if g.status.as_ref().is_some_and(|s| s.0)));
    // Cancel on the status restores the value.
    h.get_by_label("Cancel").click(); h.run_steps(2);
    assert_eq!(h.state().book.display(0, parse_a1("A1").unwrap()), "2");
    assert!(crate::data_tab::state(h.state()).dlg.is_none());
}

#[test]
fn group_rows_and_collapse_with_the_outline_bar() {
    let mut a = data_app();
    a.sel = crate::app::Selection { anchor: sheet_engine::Pos::new(1, 0), active: sheet_engine::Pos::new(3, sheet_engine::addr::MAX_COLS - 1), extra: vec![] };
    let mut h = data_tab_harness(a);
    let before = h.state().grid_rect;
    h.get_by_label("Group").click(); h.run_steps(3);
    assert_eq!(h.state().book.sheets[0].outline.groups(true, 1), vec![(1, 3)]);
    // The grid moved right to make room for the bar.
    let g = h.state().grid_rect;
    assert!(g.min.x > before.min.x + 20.0, "{before:?} → {g:?}");
    // The minus box on the summary row (row 5) collapses the group.
    let lay = crate::grid::layout(h.state(), g);
    let r = lay.cell_rect(sheet_engine::Pos::new(4, 0));
    let x = g.min.x - 36.0 + 12.0;
    click(&mut h, egui::pos2(x, r.center().y));
    h.run_steps(2);
    let s = &h.state().book.sheets[0];
    assert!((1..=3).all(|i| s.hidden_rows.contains(&i)), "hidden: {:?}", s.hidden_rows);
    assert!(s.outline.collapsed_rows.contains(&4));
    // Level button 2 shows everything again.
    let y = g.min.y + lay.head_h / 2.0;
    click(&mut h, egui::pos2(g.min.x - 36.0 + 4.0 + 16.0 * 1.5, y));
    h.run_steps(2);
    assert!(h.state().book.sheets[0].hidden_rows.is_empty());
    // Undo goes back through the steps.
    h.state_mut().undo();
    assert!(h.state().book.sheets[0].hidden_rows.contains(&2));
}

#[test]
fn subtotal_dialog_inserts_totals_and_outline() {
    let mut a = data_app();
    a.select(parse_a1("A1").unwrap(), false);
    let mut h = data_tab_harness(a);
    h.get_by_label("Subtotal").click(); h.run_steps(3);
    assert!(matches!(crate::data_tab::state(h.state()).dlg, Some(crate::data_tab::Dlg::Subtotal(ref d)) if d.cols == vec![false, false, true]));
    h.get_by_label("OK").click(); h.run_steps(3);
    let b = &h.state().book;
    assert_eq!(b.display(0, parse_a1("A3").unwrap()), "East Total");
    assert_eq!(b.display(0, parse_a1("C3").unwrap()), "10");
    assert_eq!(b.display(0, parse_a1("A10").unwrap()), "Grand Total");
    assert_eq!(b.display(0, parse_a1("C10").unwrap()), "42");
    assert_eq!(b.sheets[0].outline.max_level(true), 2);
    assert!(h.state().grid_rect.min.x > 50.0);
    // Remove All takes them out again.
    h.get_by_label("Subtotal").click(); h.run_steps(3);
    h.get_by_label("Remove All").click(); h.run_steps(3);
    assert_eq!(h.state().book.display(0, parse_a1("A3").unwrap()), "West");
    assert!(h.state().book.sheets[0].outline.rows.is_empty());
}

#[test]
fn data_tab_dialogs_render() {
    let mut a = data_app();
    a.select(parse_a1("A1").unwrap(), false);
    let mut h = data_tab_harness(a);
    let openers: [fn(&mut SheetApp); 9] = [crate::data_tools::open_dedupe, crate::data_tools::open_consolidate, crate::data_tools::open_form, crate::data_tools::open_advanced, crate::analysis_ui::open, crate::whatif_ui::open_solver, crate::whatif_ui::open_scenarios, crate::data_import::open_text_to_columns, crate::whatif_ui::open_forecast];
    for f in openers {
        f(h.state_mut()); h.run_steps(3);
        assert!(crate::data_tab::state(h.state()).dlg.is_some());
        crate::data_tab::with(h.state(), |s| s.dlg = None);
        h.run_steps(1);
    }
    crate::data_tab::with(h.state(), |s| s.pane = true);
    h.run_steps(2);
    let _ = h.get_by_label("This workbook has no queries or connections. Use Get Data to add one.");
}

#[test]
fn page_layout_live_grid_click_and_type() {
    use crate::page_view::{set_mode, ViewMode};
    let mut h = harness(data_app());
    h.run_steps(2);
    set_mode(h.state_mut(), ViewMode::PageLayout);
    h.run_steps(3);
    let target = sheet_engine::Pos::new(6, 1);
    let at = crate::page_view::current_layout(h.state()).cell_rect(target).center();
    assert!(h.state().grid_rect.contains(at));
    click(&mut h, at);
    assert_eq!(h.state().sel.active, target);
    // Typing opens the in-cell editor (not the formula bar) and Enter commits and moves down.
    h.event(egui::Event::Text("42".into())); h.run_steps(2);
    assert!(h.state().edit.as_ref().is_some_and(|e| !e.focus_bar));
    h.event(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }); h.run_steps(2);
    assert_eq!(h.state().book.display(0, target), "42");
    assert_eq!(h.state().sel.active, sheet_engine::Pos::new(7, 1));
    // Keys move the selection too.
    h.event(egui::Event::Key { key: egui::Key::ArrowRight, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }); h.run_steps(2);
    assert_eq!(h.state().sel.active, sheet_engine::Pos::new(7, 2));
    // A column header on the page resizes its column.
    let lay = crate::page_view::current_layout(h.state());
    let b = lay.cell_rect(sheet_engine::Pos::new(0, 0));
    let w0 = h.state().book.sheets[0].col_width(0);
    let y = b.min.y - h.state().book.sheets[0].page.margins.top * 96.0 * h.state().zoom - 9.0;
    drag(&mut h, egui::pos2(b.max.x, y), egui::pos2(b.max.x + 40.0 * lay.z, y));
    let w1 = h.state().book.sheets[0].col_width(0);
    assert!(w1 > w0 + 20.0, "{w0} -> {w1}");
    if let Ok(dir) = std::env::var("JADWAL_SNAPSHOT_DIR") {
        let img = image::RgbaImage::from_pixel(40, 20, image::Rgba([30, 90, 200, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let hf = &mut h.state_mut().book.sheets[0].page.header_footer;
        hf.odd_header = "&L&\"Arial,Bold\"&14&KC00000Report&C&G&RPage &P".into();
        hf.pictures.push(sheet_engine::hfpic::HfPicture::new("CH".into(), png.into_inner(), "png"));
        h.run_steps(3);
        if let Ok(img) = h.render() { let _ = img.save(format!("{dir}/page-layout.png")); }
    }
}

#[test]
fn page_layout_ruler_drags_margin() {
    use crate::page_view::{set_mode, ViewMode};
    let mut h = harness(data_app());
    h.run_steps(2);
    set_mode(h.state_mut(), ViewMode::PageLayout);
    h.run_steps(3);
    let m0 = h.state().book.sheets[0].page.margins.left;
    let r = crate::page_view::ruler_marker(0).expect("left margin marker");
    let dx = 0.5 * 96.0 * h.state().zoom;
    drag(&mut h, r.center(), r.center() + egui::vec2(dx, 0.0));
    let m1 = h.state().book.sheets[0].page.margins.left;
    assert!((m1 - (m0 + 0.5)).abs() < 0.03, "{m0} -> {m1}");
    h.state_mut().undo();
    assert_eq!(h.state().book.sheets[0].page.margins.left, m0);
    // The View tab's Ruler checkbox hides the rulers.
    crate::page_view::set_ruler(false);
    h.run_steps(2);
    assert!(crate::page_view::ruler_marker(0).is_none() || !crate::page_view::ruler_shown());
    crate::page_view::set_ruler(true);
}

#[test]
fn header_section_formatting_and_picture() {
    use crate::page_view::{editing, header_footer, hf_format, HfFormat};
    let mut h = harness(data_app());
    h.run_steps(2);
    header_footer(h.state_mut());
    h.run_steps(2);
    crate::page_view::insert_code(h.state_mut(), "Title");
    h.run_steps(1);
    hf_format(h.state_mut(), HfFormat::Bold);
    assert_eq!(editing().unwrap().text, "&BTitle");
    // The Home tab's controls format the section while it's open.
    h.state_mut().apply_style("Font Size", |s| s.size = 140);
    assert_eq!(editing().unwrap().text, "&14&BTitle");
    h.state_mut().apply_style("Font Color", |s| s.color = Some([255, 0, 0]));
    assert!(editing().unwrap().text.starts_with("&KFF0000"));
    let img = image::RgbaImage::from_pixel(20, 10, image::Rgba([0, 0, 255, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img).write_to(&mut png, image::ImageFormat::Png).unwrap();
    crate::page_view::insert_hf_picture(h.state_mut(), png.into_inner(), "png");
    h.run_steps(2);
    let hf = &h.state().book.sheets[0].page.header_footer;
    assert!(hf.odd_header.contains("&G") && hf.pictures.len() == 1 && hf.pictures[0].slot == "CH");
    assert!(crate::page_view::current_picture(h.state()).is_some());
    h.state_mut().undo();
    assert!(h.state().book.sheets[0].page.header_footer.pictures.is_empty());
}

#[test]
fn range_field_picks_cells_on_the_sheet() {
    let mut a = data_app();
    a.select(sheet_engine::Pos::new(0, 4), false);
    let mut h = data_tab_harness(a);
    h.get_by_label("What-If Analysis").click(); h.run_steps(2);
    h.get_by_label("Goal Seek…").click(); h.run_steps(2);
    // The collapse button beside "By changing cell".
    let buttons: Vec<_> = h.query_all_by_label("⬚").collect();
    assert!(buttons.len() >= 2, "Goal Seek should have two range buttons");
    buttons[1].click(); h.run_steps(2);
    assert!(crate::refedit::active(&h.ctx), "picking mode should be on");
    // The dialog steps aside: drag C2:C4 on the grid.
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    let from = lay.cell_rect(sheet_engine::Pos::new(1, 2)).center();
    let to = lay.cell_rect(sheet_engine::Pos::new(3, 2)).center();
    drag(&mut h, from, to);
    h.event(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }); h.run_steps(3);
    assert!(!crate::refedit::active(&h.ctx));
    crate::data_tab::with(h.state(), |s| if let Some(crate::data_tab::Dlg::GoalSeek(g)) = &mut s.dlg { assert_eq!(g.by, "$C$2:$C$4"); } else { panic!("Goal Seek closed") });
}

/// Renders each ribbon tab (and the window around it) to PNGs for a visual check:
/// `JADWAL_SNAPSHOT_DIR=dir cargo test -p sheet-app ribbon_snapshots -- --ignored`.
#[test]
#[ignore]
fn ribbon_snapshots() {
    let dir = std::env::var("JADWAL_SNAPSHOT_DIR").expect("JADWAL_SNAPSHOT_DIR");
    let width: f32 = std::env::var("JADWAL_SNAPSHOT_WIDTH").ok().and_then(|w| w.parse().ok()).unwrap_or(1730.0);
    for (tab, name) in [(crate::app::Tab::Home, "home"), (crate::app::Tab::Insert, "insert"), (crate::app::Tab::PageLayout, "pagelayout"), (crate::app::Tab::Formulas, "formulas"), (crate::app::Tab::Data, "data"), (crate::app::Tab::Review, "review"), (crate::app::Tab::View, "view")] {
        let mut a = data_app();
        a.tab = tab;
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(width, 420.0)).with_accessibility_check(false).build_ui_state(|ui, app: &mut SheetApp| { if app.grid_rect == egui::Rect::NOTHING && app.ctx.cumulative_frame_nr() == 0 { qalam_kit::fonts::install_fonts(ui.ctx()); app.ctx = ui.ctx().clone(); app.grid_rect = egui::Rect::ZERO; return; } app.frame(ui) }, a);
        h.run_steps(6);
        h.render().expect("render").save(format!("{dir}/{name}.png")).unwrap();
    }
}

#[test]
fn power_query_editor_from_a_range() {
    use egui::accesskit::Role;
    let mut a = data_app();
    a.select(parse_a1("A1").unwrap(), false);
    let mut h = data_tab_harness(a);
    h.get_by_label("Get Data").click(); h.run_steps(2);
    h.get_by_label("From Table/Range").click(); h.run_steps(2);
    h.get_by_label("OK").click(); h.run_steps(4);
    let e = crate::pq_editor::state(h.state()).expect("the Power Query Editor is open");
    assert_eq!(e.queries[e.sel].name, "Table1");
    assert_eq!(e.queries[e.sel].steps.len(), 1, "Changed Type");
    // The range became a table, as Excel's Create Table does.
    assert_eq!(h.state().book.sheets[0].tables[0].name, "Table1");
    // Filtered Rows: clear West in Region's drop-down.
    h.query_all_by_label("▾").next().expect("a column drop-down").click(); h.run_steps(3);
    h.get_by_role_and_label(Role::CheckBox, "West").click(); h.run_steps(2);
    h.get_by_label("OK").click(); h.run_steps(3);
    // Removed Columns: select the Sales header, then Remove Columns ▸ Remove Columns.
    h.get_by_role_and_label(Role::Button, "Sales").click(); h.run_steps(2);
    h.query_all_by_label("Remove Columns").next().unwrap().click(); h.run_steps(3);
    h.query_all_by_label("Remove Columns").last().unwrap().click(); h.run_steps(3);
    let e = crate::pq_editor::state(h.state()).unwrap();
    let names: Vec<&str> = e.queries[e.sel].steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Changed Type", "Filtered Rows", "Removed Columns"]);
    assert!(e.queries[e.sel].formula().contains("Table.SelectRows(#\"Changed Type\", each ([Region] <> \"West\"))"), "{}", e.queries[e.sel].formula());
    // Close & Load: the query's result goes to a table on a new sheet.
    h.get_by_label("Close & Load").click(); h.run_steps(4);
    assert!(crate::pq_editor::state(h.state()).is_none(), "the editor closed: {:?}", h.state().error);
    let b = &h.state().book;
    assert_eq!(b.sheets.len(), 2);
    assert_eq!(b.active, 1);
    let t = &b.sheets[1].tables[0];
    assert_eq!(t.range, sheet_engine::addr::parse_range("A1:B3").unwrap());
    assert_eq!(b.display(1, parse_a1("A2").unwrap()), "East");
    assert_eq!(b.display(1, parse_a1("B3").unwrap()), "Ink");
    let q = sheet_engine::powerquery::find(b, "Table1").unwrap();
    assert_eq!(q.load, sheet_engine::powerquery::Load::Table(t.name.clone()));
    assert!(sheet_engine::connections::list(b).iter().any(|c| c.name == "Query - Table1"));
    // The Queries & Connections pane edits it again.
    crate::data_tab::with(h.state(), |s| { s.pane = true; s.pane_sel = sheet_engine::powerquery::connection_id(&h.state().book, "Table1"); });
    h.run_steps(2);
    h.get_by_label("Edit").click(); h.run_steps(3);
    let e = crate::pq_editor::state(h.state()).unwrap();
    assert_eq!(e.queries[e.sel].steps.len(), 3);
    // Undo takes the load back.
    crate::pq_editor::with(h.state(), |e| e.queries.clear());
    h.get_by_label("Close").click(); h.run_steps(2);
    h.state_mut().undo();
    assert_eq!(h.state().book.sheets.len(), 1);
}

#[test]
fn power_query_editor_commands_render() {
    use sheet_engine::powerquery::{Op, Query, Source};
    let mut a = data_app();
    let ti = a.book.create_table(0, sheet_engine::addr::parse_range("A1:C5").unwrap(), true, "").unwrap();
    let name = a.book.sheets[0].tables[ti].name.clone();
    let mut h = data_tab_harness(a);
    let mut q = Query::new("Sales", Source::Current { name });
    q.add_step(Op::FirstN(3), None);
    crate::pq_editor::open(h.state_mut(), None, Some(q));
    h.run_steps(3);
    // Every ribbon tab draws, and each dialog opens and cancels.
    for tab in ["Transform", "Add Column", "Home"] { h.get_by_label(tab).click(); h.run_steps(2); }
    crate::pq_editor::with(h.state(), |e| e.cols = vec!["Item".into()]);
    h.run_steps(1);
    for label in ["Choose Columns", "Advanced Editor"] {
        h.get_by_label(label).click(); h.run_steps(3);
        assert!(crate::pq_editor::state(h.state()).unwrap().dlg.is_some(), "{label}");
        h.get_by_label("Cancel").click(); h.run_steps(2);
    }
    // The gear opens the step's settings; OK changes it in place.
    h.get_by_label("⚙").click(); h.run_steps(3);
    crate::pq_editor::with(h.state(), |e| if let Some(crate::pq_editor::Dlg::Rows { n, .. }) = &mut e.dlg { *n = "2".into(); } else { panic!("no rows dialog") });
    h.run_steps(1);
    h.get_by_label("OK").click(); h.run_steps(3);
    let e = crate::pq_editor::state(h.state()).unwrap();
    assert_eq!(e.queries[e.sel].steps[0].op, Op::FirstN(2));
    // A custom column through its dialog.
    h.get_by_label("Add Column").click(); h.run_steps(2);
    h.get_by_label("Custom Column").click(); h.run_steps(3);
    crate::pq_editor::with(h.state(), |e| if let Some(crate::pq_editor::Dlg::Custom { formula, .. }) = &mut e.dlg { *formula = "[Sales] * 2".into(); });
    h.run_steps(1);
    h.get_by_label("OK").click(); h.run_steps(3);
    let e = crate::pq_editor::state(h.state()).unwrap();
    assert!(matches!(&e.queries[e.sel].steps[1].op, Op::AddColumn { name, .. } if name == "Custom"));
    // Close & Load ▾ ▸ Only Create Connection saves the query without a table.
    h.get_by_label("Home").click(); h.run_steps(2);
    let r = h.get_by_label("Close & Load").rect();
    click(&mut h, egui::pos2(r.center().x, r.max.y - 5.0));
    h.get_by_label("Only Create Connection").click(); h.run_steps(4);
    assert!(crate::pq_editor::state(h.state()).is_none());
    let b = &h.state().book;
    assert_eq!(b.sheets.len(), 1);
    let q = sheet_engine::powerquery::find(b, "Sales").unwrap();
    assert_eq!(q.load, sheet_engine::powerquery::Load::ConnectionOnly);
    assert_eq!(q.steps.len(), 2);
}

/// Renders frozen panes for a visual check: `JADWAL_SNAPSHOT_DIR=dir cargo test -p sheet-app freeze_snapshots -- --ignored`.
#[test]
#[ignore]
fn freeze_snapshots() {
    let dir = std::env::var("JADWAL_SNAPSHOT_DIR").expect("JADWAL_SNAPSHOT_DIR");
    let mut a = data_app();
    for r in 0..80u32 { for c in 0..12u32 { if r > 5 || c > 3 { a.book.set_input(0, sheet_engine::Pos::new(r, c), &format!("{}{}", sheet_engine::addr::col_name(c), r + 1)); } } }
    a.book.recalc();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100.0, 640.0)).with_accessibility_check(false).build_ui_state(|ui, app: &mut SheetApp| { if app.ctx.cumulative_frame_nr() == 0 && app.grid_rect == egui::Rect::NOTHING { qalam_kit::fonts::install_fonts(ui.ctx()); app.ctx = ui.ctx().clone(); app.grid_rect = egui::Rect::ZERO; return; } app.frame(ui) }, a);
    h.run_steps(3);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().select(sheet_engine::Pos::new(3, 2), false);
    h.state_mut().freeze(0);
    h.run_steps(3);
    h.render().unwrap().save(format!("{dir}/freeze-c4.png")).unwrap();
    h.state_mut().scroll = egui::vec2(300.0, 400.0);
    h.run_steps(3);
    h.render().unwrap().save(format!("{dir}/freeze-c4-scrolled.png")).unwrap();
    h.state_mut().freeze(1);
    h.run_steps(3);
    h.render().unwrap().save(format!("{dir}/freeze-top.png")).unwrap();
}

#[test]
fn freeze_top_row_over_frozen_panes() {
    let mut a = data_app();
    for r in 0..80u32 { for c in 0..12u32 { if r > 5 || c > 3 { a.book.set_input(0, sheet_engine::Pos::new(r, c), &format!("{}{}", sheet_engine::addr::col_name(c), r + 1)); } } }
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().select(sheet_engine::Pos::new(3, 2), false);
    h.state_mut().freeze(0);
    h.state_mut().scroll = egui::vec2(300.0, 400.0);
    h.run_steps(2);
    // Excel replaces the panes with the top row on screen, which is row 1 here.
    h.state_mut().freeze(1);
    let sh = &h.state().book.sheets[0];
    assert_eq!((sh.frozen_rows, sh.frozen_cols, sh.frozen_origin), (1, 0, sheet_engine::Pos::new(0, 0)));
    h.state_mut().freeze(2);
    let sh = &h.state().book.sheets[0];
    assert_eq!((sh.frozen_rows, sh.frozen_cols), (0, 1));
    // Columns scrolled under frozen ones don't paint over them: freeze C4, scroll right, and the
    // cells in A:B show only their own text.
    h.state_mut().freeze(0); h.state_mut().freeze(0);
    let sh = &h.state().book.sheets[0];
    assert_eq!((sh.frozen_rows, sh.frozen_cols), (3, 2));
}

#[test]
fn power_query_editor_keeps_its_size() {
    use sheet_engine::powerquery::{Query, Source};
    // A wide table with long text, like real data.
    let mut a = data_app();
    for c in 0..20u32 { for r in 0..30u32 { a.book.set_input(0, sheet_engine::Pos::new(r, c + 5), &if r == 0 { format!("Column number {c} with a long name") } else { format!("Some long text value {r}-{c} that is wide") }); } }
    a.book.recalc();
    let ti = a.book.create_table(0, sheet_engine::addr::parse_range("F1:Y30").unwrap(), true, "").unwrap();
    let name = a.book.sheets[0].tables[ti].name.clone();
    let mut h = data_tab_harness(a);
    crate::pq_editor::open(h.state_mut(), None, Some(Query::new("Sales", Source::Current { name })));
    h.run_steps(3);
    let size = |h: &egui_kittest::Harness<'static, SheetApp>| h.ctx.memory(|m| m.area_rect(egui::Id::unique(("dialog", "Power Query Editor")))).map(|r| r.size());
    let first = size(&h);
    assert!(first.is_some(), "editor window not found");
    // Switching tabs, adding many steps and showing an error must not change the window's size.
    crate::pq_editor::with(h.state(), |e| e.cols = vec!["Column number 0 with a long name".into()]);
    for tab in ["Transform", "Add Column", "Home"] {
        h.get_by_label(tab).click();
        h.run_steps(3);
        assert_eq!(size(&h), first, "the window changed size on {tab}");
    }
    crate::pq_editor::with(h.state(), |e| for k in 0..40 { e.queries[0].add_step(sheet_engine::powerquery::Op::FirstN(25 - k % 5), None); });
    h.run_steps(3);
    assert_eq!(size(&h), first, "the window changed size with many steps");
    crate::pq_editor::with(h.state(), |e| e.error = Some("Expression.Error: something went wrong".into()));
    h.run_steps(3);
    assert_eq!(size(&h), first, "the window changed size showing an error");
}

#[test]
fn freeze_panes_with_a_whole_row_selected() {
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    // Select row 3 by its header: rows 1-2 freeze, no columns.
    h.state_mut().sel = crate::app::Selection { anchor: sheet_engine::Pos::new(2, 0), active: sheet_engine::Pos::new(2, sheet_engine::addr::MAX_COLS - 1), extra: vec![] };
    h.state_mut().freeze(0);
    let sh = &h.state().book.sheets[0];
    assert_eq!((sh.frozen_rows, sh.frozen_cols), (2, 0));
    // A whole column: columns freeze, no rows.
    h.state_mut().freeze(0);
    h.state_mut().sel = crate::app::Selection { anchor: sheet_engine::Pos::new(0, 1), active: sheet_engine::Pos::new(sheet_engine::addr::MAX_ROWS - 1, 1), extra: vec![] };
    h.state_mut().freeze(0);
    let sh = &h.state().book.sheets[0];
    assert_eq!((sh.frozen_rows, sh.frozen_cols), (0, 1));
}

#[test]
fn selecting_after_freezing_rows() {
    let mut a = data_app();
    for r in 0..60u32 { for c in 0..6u32 { if r > 5 { a.book.set_input(0, sheet_engine::Pos::new(r, c), &format!("{r}")); } } }
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().sel = crate::app::Selection { anchor: sheet_engine::Pos::new(2, 0), active: sheet_engine::Pos::new(2, sheet_engine::addr::MAX_COLS - 1), extra: vec![] };
    h.state_mut().freeze(0);
    h.run_steps(3);
    let click = |h: &mut egui_kittest::Harness<'static, SheetApp>, at: egui::Pos2| {
        h.event(egui::Event::PointerMoved(at)); h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }); h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }); h.run_steps(2);
    };
    for target in [sheet_engine::Pos::new(0, 1), sheet_engine::Pos::new(1, 2), sheet_engine::Pos::new(5, 3), sheet_engine::Pos::new(10, 0)] {
        let lay = crate::grid::layout(h.state(), h.state().grid_rect);
        click(&mut h, lay.cell_rect(target).center());
        assert_eq!(h.state().sel.range(), sheet_engine::addr::Range::cell(target), "clicking {target:?}");
    }
    // Scrolled down, frozen rows and body still select.
    h.state_mut().scroll = egui::vec2(0.0, 300.0); h.run_steps(2);
    for target in [sheet_engine::Pos::new(1, 1), sheet_engine::Pos::new(25, 2)] {
        let lay = crate::grid::layout(h.state(), h.state().grid_rect);
        click(&mut h, lay.cell_rect(target).center());
        assert_eq!(h.state().sel.range(), sheet_engine::addr::Range::cell(target), "clicking {target:?} after scrolling");
    }
    // Drag from a frozen row into the body.
    let lay = crate::grid::layout(h.state(), h.state().grid_rect);
    drag(&mut h, lay.cell_rect(sheet_engine::Pos::new(0, 1)).center(), lay.cell_rect(sheet_engine::Pos::new(25, 2)).center());
    assert_eq!(h.state().sel.range(), sheet_engine::addr::Range { r0: 0, r1: 25, c0: 1, c1: 2 });
    // Row header in the frozen part, then in the body.
    for row in [1u32, 25] {
        let lay = crate::grid::layout(h.state(), h.state().grid_rect);
        let y = lay.cell_rect(sheet_engine::Pos::new(row, 0)).center().y;
        let x = h.state().grid_rect.min.x + lay.head_w / 2.0;
        click(&mut h, egui::pos2(x, y));
        let r = h.state().sel.range();
        assert_eq!((r.r0, r.r1, r.c0, r.c1), (row, row, 0, sheet_engine::addr::MAX_COLS - 1), "row header {row}");
    }
}

#[test]
fn dialogs_keep_their_width() {
    // Custom Views, Goal Seek and Insert Function: each settles at its own width, well under the window's.
    let mut h = data_tab_harness(data_app());
    let id = |t: &str| egui::Id::unique(("dialog", t));
    let width = |h: &egui_kittest::Harness<'static, SheetApp>, t: &str| h.ctx.memory(|m| m.area_rect(id(t))).map(|r| r.width());
    for (open, title) in [((|a: &mut SheetApp| a.rv.view.views_dialog = Some((None, String::new(), true, false))) as fn(&mut SheetApp), "Custom Views"), (crate::whatif_ui::open_goal_seek, "Goal Seek"), (crate::formulas_tab::open_insert_function, "Insert Function")] {
        open(h.state_mut());
        h.run_steps(3);
        let w0 = width(&h, title).unwrap_or_else(|| panic!("{title} not open"));
        h.run_steps(30);
        let w1 = width(&h, title).unwrap();
        assert_eq!(w0, w1, "{title} kept growing");
        assert!(w1 < 900.0, "{title} is {w1} wide");
        h.event(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }); h.run_steps(3);
    }
}

#[test]
fn copy_cut_paste_from_macos_events() {
    // macOS sends ⌘C/⌘X/⌘V as Copy/Cut/Paste events rather than key presses.
    let mut h = harness(data_app());
    h.run_steps(2);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().select(sheet_engine::Pos::new(1, 2), false); // C2 = 10
    h.run_steps(1);
    h.event(egui::Event::Copy); h.run_steps(2);
    assert!(h.state().clip.is_some(), "Copy did nothing");
    h.state_mut().select(sheet_engine::Pos::new(8, 4), false);
    h.run_steps(1);
    h.event(egui::Event::Paste(String::new())); h.run_steps(2);
    assert_eq!(h.state().book.display(0, sheet_engine::Pos::new(8, 4)), "10");
    h.state_mut().select(sheet_engine::Pos::new(1, 1), false); // B2 = Pen
    h.event(egui::Event::Cut); h.run_steps(2);
    h.state_mut().select(sheet_engine::Pos::new(9, 4), false);
    h.event(egui::Event::Paste(String::new())); h.run_steps(2);
    assert_eq!(h.state().book.display(0, sheet_engine::Pos::new(9, 4)), "Pen");
    assert_eq!(h.state().book.display(0, sheet_engine::Pos::new(1, 1)), "");
}

// ---- File menu, About, splash ---------------------------------------------------------------

fn open_file_menu(h: &mut egui_kittest::Harness<'static, SheetApp>) {
    if h.query_by_label("Quit").is_none() { h.get_by_label("File").click(); h.run_steps(2); }
}

#[test]
fn file_menu_opens_each_section() {
    let mut h = data_tab_harness(data_app());
    h.state_mut().tab = crate::app::Tab::Home;
    open_file_menu(&mut h);
    for label in ["New", "New from Template…", "Open…", "Open Recent", "Close", "Save", "Save As…", "Print…", "Export", "Info and Properties…", "Options…", "Keyboard Shortcuts", "About Jadwal", "Check for Updates…", "Quit"] {
        assert!(h.query_by_label(label).is_some(), "the File menu has {label}");
    }
    // Open Recent and Export are submenus.
    h.get_by_label("Open Recent").hover(); h.run_steps(3);
    assert!(h.query_by_label("Clear Recent Files").is_some());
    h.get_by_label("Export").hover(); h.run_steps(3);
    for label in ["PDF…", "PDF (Entire Workbook)…", "CSV (Comma delimited)…", "CSV UTF-8 (Comma delimited)…", "Text (Tab delimited)…", "OpenDocument Spreadsheet…"] {
        assert!(h.query_by_label(label).is_some(), "Export has {label}");
    }
    h.key_press(egui::Key::Escape); h.run_steps(2);

    open_file_menu(&mut h);
    h.get_by_label("New from Template…").click(); h.run_steps(3);
    assert!(crate::backstage::shown().0, "the templates gallery is open");
    let (_, name, _, _) = sheet_engine::templates::TEMPLATES[0];
    assert!(h.query_by_label_contains(&crate::i18n::tr(name)).is_some(), "it shows the templates");
    crate::backstage::reset(); h.run_steps(2);

    open_file_menu(&mut h);
    h.get_by_label("Info and Properties…").click(); h.run_steps(3);
    assert!(crate::backstage::shown().1, "Info is open");
    assert!(h.query_by_label("Inspect Workbook").is_some());
    crate::backstage::reset(); h.run_steps(2);

    open_file_menu(&mut h);
    h.get_by_label("Options…").click(); h.run_steps(3);
    assert!(h.state().options.is_some());
    assert!(h.query_by_label("Check for updates automatically").is_some(), "Options has the update toggle");
    assert!(h.query_by_label("Show the start screen").is_some());
    h.state_mut().options = None; h.run_steps(2);

    open_file_menu(&mut h);
    h.get_by_label("Keyboard Shortcuts").click(); h.run_steps(3);
    assert!(matches!(h.state().dialog, Some(crate::app::Dialog::Shortcuts)));
    assert!(h.query_by_label("Find / Replace").is_some());
    h.state_mut().dialog = None; h.run_steps(2);

    open_file_menu(&mut h);
    h.get_by_label("Print…").click(); h.run_steps(3);
    assert!(h.state().printing.dialog.is_some(), "Print opens with its preview");
}

#[test]
fn about_page() {
    let mut h = data_tab_harness(data_app());
    open_file_menu(&mut h);
    h.get_by_label("About Jadwal").click(); h.run_steps(3);
    assert!(matches!(h.state().dialog, Some(crate::app::Dialog::About)));
    assert!(h.query_by_label(&format!("Version {}", env!("CARGO_PKG_VERSION"))).is_some());
    assert!(h.query_by_label("Woosool").is_some(), "links to the maker");
    assert!(h.query_by_label("Jadwal on woosool.com").is_some());
    assert!(h.query_by_label("Released under the MIT License.").is_some());
    h.get_by_label("OK").click(); h.run_steps(2);
    assert!(h.state().dialog.is_none());
}

#[test]
fn splash_covers_loading_a_workbook() {
    let mut h = data_tab_harness(data_app());
    let (tx, rx) = std::sync::mpsc::channel();
    h.state_mut().splash = Some(0.0);
    h.state_mut().loading = Some((std::path::PathBuf::from("/tmp/big.xlsx"), rx));
    h.run_steps(120); // two seconds: past the splash's minimum, but the workbook isn't read yet
    assert!(h.state().splash.is_some(), "the splash stays while the workbook loads");
    assert!(h.query_by_label("File").is_none(), "the window isn't drawn behind it");
    let mut wb = sheet_engine::Workbook::new();
    wb.set_input(0, sheet_engine::Pos::new(0, 0), "loaded");
    tx.send(Ok(wb)).unwrap();
    h.run_steps(3);
    assert!(h.state().splash.is_none() && h.state().loading.is_none());
    assert_eq!(h.state().book.sheets[0].value(sheet_engine::Pos::new(0, 0)).to_text().unwrap(), "loaded");
    assert!(h.query_by_label("File").is_some());
}

#[test]
#[ignore]
fn file_menu_snapshot() {
    let dir = std::env::var("JADWAL_SNAPSHOT_DIR").expect("JADWAL_SNAPSHOT_DIR");
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(900.0, 700.0)).with_accessibility_check(false).build_ui_state(|ui, app: &mut SheetApp| { if app.grid_rect == egui::Rect::NOTHING && app.ctx.cumulative_frame_nr() == 0 { qalam_kit::fonts::install_fonts(ui.ctx()); app.ctx = ui.ctx().clone(); app.grid_rect = egui::Rect::ZERO; return; } app.frame(ui) }, data_app());
    h.run_steps(4);
    h.state_mut().ctx = h.ctx.clone();
    h.state_mut().dialog = Some(crate::app::Dialog::About); h.run_steps(4);
    h.render().expect("render").save(format!("{dir}/about.png")).unwrap();
    h.state_mut().dialog = None; h.run_steps(2);
    h.get_by_label("File").click(); h.run_steps(4);
    h.render().expect("render").save(format!("{dir}/filemenu.png")).unwrap();
    h.get_by_label("Export").hover(); h.run_steps(4);
    h.render().expect("render").save(format!("{dir}/filemenu-export.png")).unwrap();
}

/// A stand-in for the system's default-app settings: Excel has .xlsx until Jadwal is made default.
struct FakeDefaults(std::sync::Mutex<bool>);
impl qalam_kit::default_apps::Backend for FakeDefaults {
    fn handler(&self, app: &qalam_kit::default_apps::AppId, f: &qalam_kit::default_apps::Format) -> Option<String> {
        Some(if *self.0.lock().unwrap() || f.ext == "csv" { app.bundle_id.to_string() } else { "com.microsoft.Excel".into() })
    }
    fn make_default(&self, _: &qalam_kit::default_apps::AppId) -> Result<qalam_kit::default_apps::Outcome, String> { *self.0.lock().unwrap() = true; Ok(qalam_kit::default_apps::Outcome::Changed) }
}

#[test]
fn options_default_apps() {
    qalam_kit::default_apps::set_backend(std::sync::Arc::new(FakeDefaults(std::sync::Mutex::new(false))));
    let mut a = app();
    a.options = Some(a.prefs.clone());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1300.0, 1400.0)).with_accessibility_check(false).build_ui_state(|ui, app: &mut SheetApp| { if app.grid_rect == egui::Rect::NOTHING && app.ctx.cumulative_frame_nr() == 0 { qalam_kit::fonts::install_fonts(ui.ctx()); app.ctx = ui.ctx().clone(); app.grid_rect = egui::Rect::ZERO; return; } app.frame(ui) }, a);
    h.run_steps(3);
    assert!(h.query_by_label("Default Apps").is_some());
    assert!(h.query_by_label(".xlsx").is_some() && h.query_by_label(".ods").is_some() && h.query_by_label(".csv").is_some());
    assert!(h.query_by_label("✔ Jadwal").is_some(), ".csv already opens with Jadwal");
    assert!(h.query_all_by_label("Another app").count() > 0);
    h.get_by_label("Make Jadwal the Default App").click(); h.run_steps(3);
    assert!(h.query_by_label("Another app").is_none());
    assert!(h.query_by_label("Jadwal is the default app for all of these file types.").is_some());
}

#[test]
fn opens_a_file_the_system_sent() {
    crate::app::TAKE_SENT_FILES.with(|t| t.set(true));
    let dir = std::env::temp_dir().join(format!("jadwal-sent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("sent.csv");
    std::fs::write(&p, "a,b\n1,2\n").unwrap();
    let mut h = harness(app());
    h.run_steps(2);
    qalam_kit::open_files::push([p.clone()]);
    h.run_steps(3);
    assert_eq!(h.state().path.as_deref(), Some(p.as_path()), "an untitled, unchanged workbook is replaced");
    assert_eq!(h.state().book.display(0, parse_a1("B2").unwrap()), "2");
    // With unsaved changes it asks first, as File ▸ Open does.
    h.state_mut().dirty = true;
    let q = dir.join("second.csv");
    std::fs::write(&q, "x\n").unwrap();
    qalam_kit::open_files::push([q.clone()]);
    h.run_steps(3);
    assert!(matches!(h.state().pending, Some(crate::filemenu::Pending::OpenPath(ref x)) if x == &q));
    assert_eq!(h.state().path.as_deref(), Some(p.as_path()));
    crate::app::TAKE_SENT_FILES.with(|t| t.set(false));
    let _ = std::fs::remove_dir_all(&dir);
}
