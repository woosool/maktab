//! The Insert tab's object groups (chart-ribbon.js, insert menus): PivotTable, Recommended
//! PivotTables, Pictures, Shapes, Text Box, the chart galleries and Slicer; and the contextual
//! tabs objects bring (Chart Design, Shape/Picture Format, PivotTable Analyze).

use crate::app::{SheetApp, Tab};
use crate::i18n::tr;
use egui::Ui;
use qalam_kit::widgets::*;
use sheet_engine::chart::ChartKind;
use sheet_engine::drawing::ObjKind;

/// The Insert tab, group by group as Excel lays it out: Tables, Illustrations, Add-ins, Charts,
/// Tours, Sparklines, Filters, Links, Comments, Text and Symbols. Each command lives in
/// `insert_hooks`, which hands it to the module that implements it.
pub fn groups(app: &mut SheetApp, ui: &mut Ui) {
    use crate::insert_hooks as h;
    group(ui, "Tables", |ui| {
        if large(ui, "pivotTable", "PivotTable", false, true, "Summarize data with a PivotTable").clicked() { crate::pivot_ui::open_insert(app); }
        if large(ui, "recommendedPivot", "Recommended PivotTables", false, true, "A PivotTable picked for the selected data").clicked() { crate::pivot_ui::recommended(app); }
        if large(ui, "table", "Table", false, true, "Create a table (⌘T)").clicked() { h::table(app); }
    });
    group(ui, "Illustrations", |ui| {
        let r = large_menu(ui, "picture", "Pictures", "Insert a picture");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "picture", "This Device…") { crate::objects::insert_picture(app); ui.close(); }
            if menu_item(ui, "picture", "Picture in Cell…") { h::picture_in_cell(app); ui.close(); }
        });
        if large(ui, "link", "Online\nPictures", false, true, "Find and insert pictures from the web").clicked() { h::online_pictures(app); }
        // Excel's icon-only column: shapes, icons and SmartArt, then screenshot and 3D models.
        rows(ui, |ui| {
            let r = small_menu(ui, "shapes", "", "Shapes");
            crate::theme::menu(&r, |ui| h::shapes_gallery(app, ui));
            if small(ui, "icons", None, false, true, "Icons").clicked() { h::icons(app); }
            if small(ui, "smartArt", None, false, true, "SmartArt").clicked() { h::smart_art(app); }
        });
        rows(ui, |ui| {
            let r = small_menu(ui, "screenshot", "", "Screenshot");
            crate::theme::menu(&r, |ui| h::screenshot_menu(app, ui));
            if small(ui, "model3d", None, false, true, "3D Models").clicked() { h::model_3d(app); }
        });
    });
    group(ui, "Add-ins", |ui| {
        rows(ui, |ui| {
            if small(ui, "macros", Some("Scripts"), false, true, "Run, record and manage scripts that automate the workbook").clicked() { h::scripts(app); }
            let r = small_menu(ui, "more", "My Add-ins", "Add-ins installed for Jadwal");
            crate::theme::menu(&r, |ui| h::add_ins_menu(app, ui));
        });
    });
    group(ui, "Charts", |ui| {
        let r = large_menu(ui, "recommendedCharts", "Recommended Charts", "Charts suited to the selected data");
        crate::theme::menu(&r, |ui| {
            for (k, g, label) in crate::chart_ui::recommended(app) { if menu_item(ui, k.icon(), label) { crate::chart_ui::insert_chart(app, k, g); ui.close(); } }
        });
        rows(ui, |ui| {
            for (kinds, icon, tip) in [(&[ChartKind::Column, ChartKind::Bar][..], "chartColumn", "Insert Column or Bar Chart"), (&[ChartKind::Line, ChartKind::Area][..], "chartLine", "Insert Line or Area Chart"), (&[ChartKind::Pie, ChartKind::Doughnut][..], "chartPie", "Insert Pie or Doughnut Chart")] {
                let r = small_menu(ui, icon, "", tip);
                crate::theme::menu(&r, |ui| crate::chart_ui::gallery_menu(app, ui, kinds, false));
            }
        });
        rows(ui, |ui| {
            let r = small_menu(ui, "chartScatter", "", "Insert Scatter (X, Y) or Bubble Chart");
            crate::theme::menu(&r, |ui| crate::chart_ui::gallery_menu(app, ui, &[ChartKind::Scatter, ChartKind::Bubble], false));
            let r = small_menu(ui, "chartRadar", "", "Insert Stock or Radar Chart");
            crate::theme::menu(&r, |ui| crate::chart_ui::gallery_menu(app, ui, &[ChartKind::Stock, ChartKind::Radar], false));
            let r = small_menu(ui, "chartCombo", "", "Insert Combo Chart");
            crate::theme::menu(&r, |ui| {
                crate::chart_ui::combo_menu(app, ui);
                ui.separator();
                menu_heading(ui, &tr("All Charts"));
                crate::chart_ui::gallery_menu(app, ui, &ChartKind::ALL, false);
            });
        });
        rows(ui, |ui| {
            let r = small_menu(ui, "chartArea", "", "Insert Hierarchy, Statistic or Waterfall Chart");
            crate::theme::menu(&r, |ui| h::more_charts_menu(app, ui));
            let r = small_menu(ui, "map", "", "Insert Map Chart");
            crate::theme::menu(&r, |ui| h::map_chart_menu(app, ui));
        });
        let r = large_menu(ui, "pivotTable", "PivotChart", "Insert a PivotChart (with or without a PivotTable)");
        crate::theme::menu(&r, |ui| h::pivot_chart_menu(app, ui));
    });
    group(ui, "Tours", |ui| {
        if large(ui, "map", "3D Map", false, true, "Plot data with places on a map").clicked() { h::map_3d(app); }
    });
    group(ui, "Sparklines", |ui| {
        for (icon, label, kind) in [("sparkline", "Line", 0u8), ("chartColumn", "Column", 1), ("chartBar", "Win/Loss", 2)] {
            if large(ui, icon, label, false, true, "Insert sparklines: tiny charts in cells").clicked() { h::sparklines(app, kind); }
        }
    });
    group(ui, "Filters", |ui| {
        if large(ui, "slicer", "Slicer", false, true, "Filter a PivotTable or table with buttons").clicked() { crate::pivot_ui::open_slicers(app); }
        if large(ui, "timeline", "Timeline", false, true, "Filter PivotTable dates with a timeline").clicked() { h::timeline(app); }
    });
    group(ui, "Links", |ui| {
        if large(ui, "link", "Link", false, true, "Insert Link (⌘K)").clicked() { h::link(app); }
    });
    group(ui, "Text", |ui| {
        if large(ui, "textBox", "Text Box", false, true, "Draw a text box").clicked() { crate::objects::insert_shape(app, "textBox"); }
        if large(ui, "header", "Header\n& Footer", false, true, "Edit the page header and footer in Page Layout view").clicked() { h::header_footer(app); }
        rows(ui, |ui| {
            let r = small_menu(ui, "wordArt", "", "Insert WordArt");
            crate::theme::menu(&r, |ui| h::word_art_menu(app, ui));
            let r = small_menu(ui, "signatureLine", "", "Insert a signature line");
            crate::theme::menu(&r, |ui| h::signature_menu(app, ui));
            if small(ui, "object", None, false, true, "Insert an object: a file shown as an icon or embedded").clicked() { h::object(app); }
        });
    });
    group(ui, "Symbols", |ui| {
        rows(ui, |ui| {
            let (e, ea) = small_split_label(ui, "equation", Some("Equation"), false, "Insert an equation");
            if e.clicked() { h::equation(app, None); }
            crate::theme::menu(&ea, |ui| h::equation_menu(app, ui));
            if small(ui, "symbol", Some("Symbol"), false, true, "Insert a symbol").clicked() { h::symbol(app); }
        });
    });
}

/// The contextual tabs to show now (and leave a contextual tab that no longer applies).
pub fn contextual(app: &mut SheetApp) -> Vec<(Tab, &'static str)> {
    let mut out = vec![];
    let sel = crate::objects::selected(app).map(|i| &app.book.sheets[app.sheet()].objects[i].kind);
    match sel {
        Some(ObjKind::Chart(_)) => out.push((Tab::ChartDesign, "Chart Design")),
        Some(ObjKind::Picture { .. }) => out.push((Tab::ObjectFormat, "Picture Format")),
        Some(ObjKind::Shape(_) | ObjKind::Slicer(_) | ObjKind::Timeline(_) | ObjKind::Group(_) | ObjKind::Other) => out.push((Tab::ObjectFormat, "Shape Format")),
        None => {}
    }
    if crate::pivot_ui::pivot_at(app).is_some() { out.push((Tab::PivotAnalyze, "PivotTable Analyze")); out.push((Tab::PivotDesign, "Design")); }
    out.extend(crate::sparkline_ui::contextual(app));
    if matches!(app.tab, Tab::ChartDesign | Tab::ObjectFormat | Tab::PivotAnalyze | Tab::PivotDesign | Tab::Sparkline) && !out.iter().any(|t| t.0 == app.tab) { app.tab = Tab::Insert; }
    out
}

/// The body of a contextual tab.
pub fn contextual_body(app: &mut SheetApp, ui: &mut Ui) {
    match app.tab {
        Tab::ChartDesign => crate::chart_ui::design_tab(app, ui),
        Tab::ObjectFormat => crate::objects::format_tab(app, ui),
        Tab::PivotAnalyze => crate::pivot_ui::analyze_tab(app, ui),
        Tab::PivotDesign => crate::pivot_ui::design_tab(app, ui),
        Tab::Sparkline => crate::sparkline_ui::tab(app, ui),
        _ => {}
    }
    let _ = tr;
}
