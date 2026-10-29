//! Charts on screen (chartsvg.js, chart-ribbon.js, chart-select-data.js, chart-type-dialog.js):
//! drawing a chart with egui's painter — title, legend, axes with nice ticks, gridlines, bars,
//! lines, areas, pies, scatter points, data labels — inserting charts from the selection, the
//! contextual Chart Design tab and the Select Data dialog.

use crate::app::{rgb, SheetApp};
use crate::i18n::tr;
use egui::{pos2, Color32, FontId, Rect, Stroke, StrokeKind, Ui};
use qalam_kit::widgets::*;
use sheet_engine::addr::Range;
use sheet_engine::chart::{Chart, ChartKind, Grouping, LegendPos, Prim, RadarStyle};
use sheet_engine::drawing::{ObjKind, Object};

fn font(px: f32, bold: bool) -> FontId { FontId::new(px.max(3.0), if bold { egui::FontFamily::Name("ui-bold".into()) } else { egui::FontFamily::Proportional }) }

/// Text measured with egui's fonts, for the chart layout.
struct ScreenMeasure<'a>(&'a egui::Painter);
impl sheet_engine::chart::Measure for ScreenMeasure<'_> {
    fn size(&self, t: &str, px: f32, bold: bool) -> [f32; 2] { let s = self.0.layout_no_wrap(t.to_string(), font(px, bold), Color32::BLACK).size(); [s.x, s.y] }
}

/// Draws a chart into `r` (screen), at zoom `z`: the engine's chart layout, painted.
pub fn paint(app: &SheetApp, p: &egui::Painter, c: &Chart, r: Rect, z: f32) {
    let data = c.data(&app.book, app.sheet());
    let prims = sheet_engine::chart::layout(c, &data, [r.min.x, r.min.y, r.max.x, r.max.y], &app.book.theme_colors, z, &ScreenMeasure(p));
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    paint_prims(&p, &prims);
}

/// Paints chart primitives.
pub fn paint_prims(p: &egui::Painter, prims: &[Prim]) {
    let pt = |q: [f32; 2]| pos2(q[0], q[1]);
    for prim in prims {
        match prim {
            Prim::Rect { r, fill, stroke } => {
                let rc = Rect::from_min_max(pos2(r[0], r[1]), pos2(r[2], r[3]));
                if let Some(f) = fill { p.rect_filled(rc, 0.0, rgb(*f)); }
                if let Some((c, w)) = stroke { p.rect_stroke(rc, 0.0, Stroke::new(*w, rgb(*c)), StrokeKind::Inside); }
            }
            Prim::Line { pts, color, width } => {
                let stroke = Stroke::new(*width, rgb(*color));
                if pts.len() == 2 { p.line_segment([pt(pts[0]), pt(pts[1])], stroke); } else if pts.len() > 2 { p.add(egui::Shape::line(pts.iter().map(|q| pt(*q)).collect(), stroke)); }
            }
            Prim::Poly { pts, fill, stroke, opacity, strip } => {
                if pts.len() < 3 { continue; }
                if let Some(f) = fill {
                    let col = if *opacity < 1.0 { rgb(*f).gamma_multiply(*opacity) } else { rgb(*f) };
                    if *strip {
                        // Two chains: the first half forward, the second half back.
                        let n = pts.len() / 2;
                        let mut m = egui::Mesh::default();
                        for k in 0..n {
                            m.colored_vertex(pt(pts[k]), col);
                            m.colored_vertex(pt(pts[pts.len() - 1 - k]), col);
                            if k > 0 { let b = (2 * k) as u32; m.add_triangle(b - 2, b - 1, b); m.add_triangle(b - 1, b, b + 1); }
                        }
                        p.add(m);
                    } else { p.add(egui::Shape::convex_polygon(pts.iter().map(|q| pt(*q)).collect(), col, Stroke::NONE)); }
                }
                if let Some((c, w)) = stroke { p.add(egui::Shape::closed_line(pts.iter().map(|q| pt(*q)).collect(), Stroke::new(*w, rgb(*c)))); }
            }
            Prim::Text { at, text, size, bold, color, angle, h, v } => {
                let g = p.layout_no_wrap(text.clone(), font(*size, *bold), rgb(*color));
                let (w, ht) = (g.size().x, g.size().y);
                // The anchor's offset within the text box, turned about the anchor.
                let (ox, oy) = (w * *h as f32 / 2.0, ht * *v as f32 / 2.0);
                let a = angle.to_radians();
                let (ca, sa) = (a.cos(), a.sin());
                let pos = pos2(at[0] - (ox * ca - oy * sa), at[1] - (ox * sa + oy * ca));
                if *angle == 0.0 { p.galley(pos, g, rgb(*color)); } else { p.add(egui::epaint::TextShape::new(pos, g, rgb(*color)).with_angle(a)); }
            }
        }
    }
}

// ---- inserting ---------------------------------------------------------------------------------------

/// The range a new chart uses: the selection, or the region around the active cell.
fn source_range(app: &SheetApp) -> Range {
    let r = app.sel.range();
    if r.is_cell() { app.book.current_region(app.sheet(), app.sel.active) } else { r }
}

pub fn insert_chart(app: &mut SheetApp, kind: ChartKind, grouping: Grouping) { insert_chart_with(app, kind, grouping, |_| {}); }

/// Inserts a chart of a gallery entry (kind, grouping and variant).
pub fn insert_variant(app: &mut SheetApp, kind: ChartKind, grouping: Grouping, variant: u8) { insert_chart_with(app, kind, grouping, |c| apply_type(c, kind, grouping, variant)); }

/// Inserts a combo chart: columns, with the last series as a line (on the secondary axis).
pub fn insert_combo(app: &mut SheetApp, secondary: bool) {
    insert_chart_with(app, ChartKind::Column, Grouping::Clustered, |c| {
        if c.series.len() < 2 { return; }
        let last = c.series.len() - 1;
        c.series[last].kind = Some(ChartKind::Line);
        c.series[last].secondary = secondary;
        c.markers = false;
        if c.legend.is_none() { c.legend = Some(LegendPos::Bottom); }
    });
}

fn insert_chart_with(app: &mut SheetApp, kind: ChartKind, grouping: Grouping, adjust: impl FnOnce(&mut Chart)) {
    let si = app.sheet();
    let r = source_range(app);
    let has_numbers = r.rows() <= 100_000 && r.iter().take(200_000).any(|p| matches!(app.book.sheets[si].value(p), sheet_engine::Value::Num(_)));
    if !has_numbers { app.error = Some(tr("Select the cells with the data to chart (with some numbers) first.")); return; }
    let mut c = Chart::from_range(&app.book, si, r, kind, None);
    c.grouping = grouping;
    adjust(&mut c);
    // Place it beside the data, about 8 × 15 cells.
    let s = &app.book.sheets[si];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    let (x, y) = (cols.pos(r.c1 + 2), rows.pos(r.r0));
    let mut a = sheet_engine::drawing::Anchor::default();
    a.set_rect([x, y, x + 480.0, y + 288.0], |x| cols.at(x), |c| cols.pos(c), |y| rows.at(y), |r| rows.pos(r));
    crate::objects::insert(app, Object::new(ObjKind::Chart(Box::new(c)), a), "Chart", "Insert Chart");
    app.tab = crate::app::Tab::ChartDesign;
}

/// Chart kinds suited to the selected data, best first.
pub fn recommended(app: &SheetApp) -> Vec<(ChartKind, Grouping, &'static str)> {
    let si = app.sheet();
    let r = source_range(app);
    let c = Chart::from_range(&app.book, si, r, ChartKind::Column, None);
    let n = c.series.len();
    let pts = c.series.first().map(|s| s.val_cache.len()).unwrap_or(0);
    let numeric_x = r.cols() >= 2 && (r.r0 + 1..=r.r1.min(r.r0 + 50)).all(|row| matches!(app.book.sheets[si].value(sheet_engine::Pos::new(row, r.c0)), sheet_engine::Value::Num(_)));
    let mut out = vec![(ChartKind::Column, Grouping::Clustered, "Clustered Column")];
    if numeric_x { out.insert(0, (ChartKind::Scatter, Grouping::Clustered, "Scatter")); }
    if pts > 6 { out.push((ChartKind::Line, Grouping::Clustered, "Line with Markers")); }
    if n == 1 && pts <= 8 { out.push((ChartKind::Pie, Grouping::Clustered, "Pie")); }
    if n > 1 { out.push((ChartKind::Column, Grouping::Stacked, "Stacked Column")); }
    out.push((ChartKind::Bar, Grouping::Clustered, "Clustered Bar"));
    if pts <= 6 && n == 1 { out.push((ChartKind::Doughnut, Grouping::Clustered, "Doughnut")); }
    out.truncate(5);
    out
}

/// The chart galleries: per kind, its entries as (grouping, variant, label). The variant picks
/// markers (Line and Radar 1, Scatter 1 for markers only), a filled radar (2) or
/// open-high-low-close stock with up/down bars (1).
pub const GALLERY: &[(ChartKind, &[(Grouping, u8, &str)])] = &[
    (ChartKind::Column, &[(Grouping::Clustered, 0, "Clustered Column"), (Grouping::Stacked, 0, "Stacked Column"), (Grouping::Percent, 0, "100% Stacked Column")]),
    (ChartKind::Bar, &[(Grouping::Clustered, 0, "Clustered Bar"), (Grouping::Stacked, 0, "Stacked Bar"), (Grouping::Percent, 0, "100% Stacked Bar")]),
    (ChartKind::Line, &[(Grouping::Clustered, 0, "Line"), (Grouping::Clustered, 1, "Line with Markers"), (Grouping::Stacked, 0, "Stacked Line"), (Grouping::Percent, 0, "100% Stacked Line")]),
    (ChartKind::Pie, &[(Grouping::Clustered, 0, "Pie")]),
    (ChartKind::Doughnut, &[(Grouping::Clustered, 0, "Doughnut")]),
    (ChartKind::Area, &[(Grouping::Clustered, 0, "Area"), (Grouping::Stacked, 0, "Stacked Area"), (Grouping::Percent, 0, "100% Stacked Area")]),
    (ChartKind::Scatter, &[(Grouping::Clustered, 1, "Scatter"), (Grouping::Clustered, 0, "Scatter with Straight Lines")]),
    (ChartKind::Bubble, &[(Grouping::Clustered, 0, "Bubble")]),
    (ChartKind::Stock, &[(Grouping::Clustered, 0, "High-Low-Close"), (Grouping::Clustered, 1, "Open-High-Low-Close")]),
    (ChartKind::Radar, &[(Grouping::Clustered, 0, "Radar"), (Grouping::Clustered, 1, "Radar with Markers"), (Grouping::Clustered, 2, "Filled Radar")]),
];

/// Retypes a chart to a gallery entry.
pub fn apply_type(c: &mut Chart, k: ChartKind, g: Grouping, variant: u8) {
    c.kind = k;
    c.grouping = if k.has_grouping() { g } else { Grouping::Clustered };
    c.markers = match k { ChartKind::Line | ChartKind::Radar | ChartKind::Scatter => variant == 1, _ => false };
    for s in &mut c.series {
        s.marker = None;
        if !k.comboable() { s.kind = None; if !k.allows_secondary() { s.secondary = false; } }
    }
    match k {
        ChartKind::Radar => c.radar_style = if variant == 2 { RadarStyle::Filled } else { RadarStyle::Marker },
        ChartKind::Stock => { c.hi_low_lines = true; c.up_down_bars = variant == 1; }
        _ => {}
    }
    if k.is_round() { c.series.truncate(1); c.legend = c.legend.or(Some(LegendPos::Right)); }
}

/// A gallery menu's items for some kinds; picking inserts (or with `change`, retypes the selected chart).
pub fn gallery_menu(app: &mut SheetApp, ui: &mut Ui, kinds: &[ChartKind], change: bool) {
    for (k, variants) in GALLERY.iter().filter(|(k, _)| kinds.contains(k)) {
        if kinds.len() > 1 { menu_heading(ui, k.label()); }
        for (g, v, label) in variants.iter() {
            if menu_item(ui, k.icon(), label) {
                if change { edit_chart(app, "Change Chart Type", |c| apply_type(c, *k, *g, *v)); }
                else { insert_variant(app, *k, *g, *v); }
                ui.close();
            }
        }
    }
}

/// The combo gallery (Insert Combo Chart).
pub fn combo_menu(app: &mut SheetApp, ui: &mut Ui) {
    if menu_item(ui, "chartCombo", "Clustered Column - Line") { insert_combo(app, false); ui.close(); }
    if menu_item(ui, "chartCombo", "Clustered Column - Line on Secondary Axis") { insert_combo(app, true); ui.close(); }
}

// ---- Change Chart Type ------------------------------------------------------------------------------------

/// The Change Chart Type dialog: a gallery entry for the whole chart, or a combination with a
/// kind and an axis per series.
#[derive(Clone)]
pub struct ChartTypeDlg { pub sheet: usize, pub idx: usize, pub pick: (ChartKind, Grouping, u8), pub combo: bool, pub series: Vec<(String, ChartKind, bool)> }

impl ChartTypeDlg {
    pub fn new(app: &SheetApp, si: usize, i: usize) -> ChartTypeDlg {
        let c = app.book.sheets[si].objects[i].chart().cloned().unwrap_or_default();
        let data = c.data(&app.book, si);
        let combo = c.series.iter().any(|s| s.secondary || s.kind.is_some_and(|k| k != c.kind));
        let variant = match c.kind { ChartKind::Radar if c.radar_style == RadarStyle::Filled => 2, ChartKind::Stock => c.up_down_bars as u8, ChartKind::Line | ChartKind::Radar | ChartKind::Scatter => c.markers as u8, _ => 0 };
        let series = c.series.iter().enumerate().map(|(k, s)| (data.get(k).map(|d| d.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Series{}", k + 1)), s.kind.unwrap_or(c.kind), s.secondary)).collect();
        ChartTypeDlg { sheet: si, idx: i, pick: (c.kind, c.grouping, variant), combo, series }
    }
}

pub fn chart_type_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut ChartTypeDlg) -> bool {
    let mut keep = true;
    let mut open = true;
    if app.book.sheets.get(d.sheet).and_then(|s| s.objects.get(d.idx)).is_none_or(|o| !o.is_chart()) { return false; }
    crate::theme::dialog(ctx, "Change Chart Type").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(520.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                egui::ScrollArea::vertical().max_height(320.0).id_salt("chart-types").show(ui, |ui| {
                    ui.set_min_width(190.0);
                    for (k, variants) in GALLERY {
                        menu_heading(ui, &tr(k.label()));
                        for (g, v, label) in variants.iter() {
                            if ui.selectable_label(!d.combo && d.pick == (*k, *g, *v), tr(label)).clicked() { d.pick = (*k, *g, *v); d.combo = false; }
                        }
                    }
                    menu_heading(ui, &tr("Combo"));
                    if ui.selectable_label(d.combo, tr("Custom Combination")).clicked() { d.combo = true; }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                if d.combo {
                    ui.label(egui::RichText::new(tr("Choose the chart type and axis for your data series:")).strong());
                    ui.add_space(4.0);
                    egui::Grid::new("combo-series").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                        ui.label(tr("Series Name")); ui.label(tr("Chart Type")); ui.label(tr("Secondary Axis")); ui.end_row();
                        for (k, (name, kind, sec)) in d.series.iter_mut().enumerate() {
                            ui.label(name.as_str());
                            egui::ComboBox::from_id_salt(("combo-kind", k)).selected_text(tr(kind.label())).show_ui(ui, |ui| {
                                for opt in [ChartKind::Column, ChartKind::Bar, ChartKind::Line, ChartKind::Area, ChartKind::Scatter, ChartKind::Bubble] { ui.selectable_value(kind, opt, tr(opt.label())); }
                            });
                            ui.checkbox(sec, "");
                            ui.end_row();
                        }
                    });
                    // Scatter and bubble series plot x values: they combine only with their own kind.
                    let first = d.series.first().map(|s| s.1);
                    if first.is_some_and(|f| d.series.iter().any(|s| (s.1.is_xy() || f.is_xy()) && s.1 != f)) {
                        ui.label(egui::RichText::new(tr("Scatter and bubble series combine only with series of the same type; all series will use the first one's type.")).weak());
                    }
                } else {
                    let label = GALLERY.iter().find(|(k, _)| *k == d.pick.0).and_then(|(_, vs)| vs.iter().find(|(g, v, _)| (*g, *v) == (d.pick.1, d.pick.2))).map(|x| x.2).unwrap_or("");
                    ui.label(egui::RichText::new(tr(label)).strong());
                    let hint = match d.pick.0 { ChartKind::Stock => "Arrange the data as High, Low, Close (or Open, High, Low, Close) columns, in that order.", ChartKind::Bubble => "Arrange the data as X values, Y values and bubble sizes, in that order.", _ => "" };
                    if !hint.is_empty() { ui.label(tr(hint)); }
                }
            });
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let (combo, pick, series) = (d.combo, d.pick, d.series.clone());
                app.checkpoint("Change Chart Type");
                if let Some(c) = app.book.sheets[d.sheet].objects[d.idx].chart_mut() {
                    if combo && !series.is_empty() {
                        let first = series[0].1;
                        if !c.kind.comboable() || c.kind != first { let g = if c.kind.has_grouping() { c.grouping } else { Grouping::Clustered }; apply_type(c, first, g, if first == ChartKind::Scatter { 1 } else { 0 }); }
                        let lines = series.iter().any(|s| s.1 == ChartKind::Line);
                        for (s, (_, k, sec)) in c.series.iter_mut().zip(&series) { s.kind = (*k != first && !first.is_xy() && !k.is_xy()).then_some(*k); s.secondary = *sec; }
                        if lines && c.legend.is_none() { c.legend = Some(LegendPos::Bottom); }
                    } else {
                        apply_type(c, pick.0, pick.1, pick.2);
                    }
                }
                app.dirty = true;
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

// ---- the Chart Design tab -------------------------------------------------------------------------------

/// The selected chart: (sheet, object index).
pub fn selected_chart(app: &SheetApp) -> Option<(usize, usize)> {
    let i = crate::objects::selected(app)?;
    app.book.sheets[app.sheet()].objects[i].is_chart().then_some((app.sheet(), i))
}

/// Changes the selected chart with undo.
pub fn edit_chart(app: &mut SheetApp, label: &'static str, f: impl FnOnce(&mut Chart)) {
    let Some((si, i)) = selected_chart(app) else { return };
    app.checkpoint(label);
    if let Some(c) = app.book.sheets[si].objects[i].chart_mut() { f(c); }
    app.dirty = true;
}

fn layout_preset(c: &mut Chart, n: u8) {
    match n {
        1 => { c.title = Some(c.title.clone().unwrap_or_default()); c.legend = Some(LegendPos::Right); c.data_labels = false; c.gridlines = true; c.x_title = None; c.y_title = None; }
        2 => { c.title = Some(c.title.clone().unwrap_or_default()); c.legend = Some(LegendPos::Top); c.data_labels = true; c.gridlines = false; c.x_title = None; c.y_title = None; }
        3 => { c.title = Some(c.title.clone().unwrap_or_default()); c.legend = Some(LegendPos::Bottom); c.data_labels = false; c.gridlines = true; c.x_title = None; c.y_title = None; }
        4 => { c.title = None; c.legend = Some(LegendPos::Bottom); c.data_labels = true; c.gridlines = false; }
        _ => { c.title = Some(c.title.clone().unwrap_or_default()); c.legend = None; c.gridlines = true; c.x_title = Some(c.x_title.clone().unwrap_or_else(|| "Axis Title".into())); c.y_title = Some(c.y_title.clone().unwrap_or_else(|| "Axis Title".into())); }
    }
}

pub fn design_tab(app: &mut SheetApp, ui: &mut Ui) {
    let Some((si, i)) = selected_chart(app) else { return };
    let c = app.book.sheets[si].objects[i].chart().cloned().unwrap_or_default();
    group(ui, "Chart Layouts", |ui| {
        let r = large_menu(ui, "chartArea", "Add Chart Element", "Show or hide chart elements");
        crate::theme::menu(&r, |ui| {
            if ui.checkbox(&mut c.title.is_some(), tr("Chart Title")).clicked() { edit_chart(app, "Chart Title", |c| c.title = if c.title.is_some() { None } else { Some(String::new()) }); }
            if ui.checkbox(&mut c.data_labels.clone(), tr("Data Labels")).clicked() { edit_chart(app, "Data Labels", |c| { c.data_labels = !c.data_labels; for s in &mut c.series { s.labels = None; } }); }
            if !c.kind.is_round() {
                if ui.checkbox(&mut c.gridlines.clone(), tr("Gridlines")).clicked() { edit_chart(app, "Gridlines", |c| c.gridlines = !c.gridlines); }
                if ui.checkbox(&mut c.x_title.is_some(), tr("Horizontal Axis Title")).clicked() { edit_chart(app, "Axis Titles", |c| c.x_title = if c.x_title.is_some() { None } else { Some("Axis Title".into()) }); }
                if ui.checkbox(&mut c.y_title.is_some(), tr("Vertical Axis Title")).clicked() { edit_chart(app, "Axis Titles", |c| c.y_title = if c.y_title.is_some() { None } else { Some("Axis Title".into()) }); }
                if ui.checkbox(&mut c.markers.clone(), tr("Markers")).clicked() { edit_chart(app, "Markers", |c| { c.markers = !c.markers; for s in &mut c.series { s.marker = None; } }); }
                if c.series.iter().any(|s| s.secondary) && ui.checkbox(&mut c.y2_title.is_some(), tr("Secondary Vertical Axis Title")).clicked() { edit_chart(app, "Axis Titles", |c| c.y2_title = if c.y2_title.is_some() { None } else { Some("Axis Title".into()) }); }
            }
            menu_heading(ui, &tr("Legend"));
            for (l, name) in [(None, "None"), (Some(LegendPos::Right), "Right"), (Some(LegendPos::Top), "Top"), (Some(LegendPos::Left), "Left"), (Some(LegendPos::Bottom), "Bottom")] {
                if ui.radio(c.legend == l, tr(name)).clicked() { edit_chart(app, "Legend", |c| c.legend = l); ui.close(); }
            }
        });
        let r = large_menu(ui, "layout", "Quick Layout", "Pick a layout of chart elements");
        crate::theme::menu(&r, |ui| for k in 1..=5u8 { if ui.button(format!("{} {k}", tr("Layout"))).clicked() { edit_chart(app, "Quick Layout", |c| layout_preset(c, k)); ui.close(); } });
    });
    group(ui, "Chart Styles", |ui| {
        let r = large_menu(ui, "colors", "Change Colors", "The chart's color scheme");
        crate::theme::menu(&r, |ui| {
            let theme = app.book.theme_colors.clone();
            for k in 0..=6u8 {
                ui.horizontal(|ui| {
                    let resp = ui.selectable_label(c.palette == k, if k == 0 { tr("Colorful") } else { format!("{} {k}", tr("Monochromatic")) });
                    for j in 0..5 { swatch(ui, rgb(sheet_engine::chart::palette_color(k, j, &theme)), 14.0, false); }
                    if resp.clicked() { edit_chart(app, "Change Colors", |c| { c.palette = k; for s in &mut c.series { s.color = None; } }); ui.close(); }
                });
            }
        });
    });
    group(ui, "Data", |ui| {
        let can_switch = c.source().is_some() && !c.kind.is_round();
        if large(ui, "convertToTable", "Switch Row/Column", false, can_switch, "Swap the data over the axes").clicked() { switch_row_col(app); }
        if large(ui, "formatPainter", "Format Selection", false, !c.series.is_empty(), "Format a data series: colors, line, markers, axis, labels").clicked() { open_format_series(app, 0); }
        if large(ui, "selectTable", "Select Data", false, true, "Change the data range in the chart").clicked() { crate::objects::open_dialog(app, crate::objects::ObjDialog::SelectData(SelectData::new(app, si, i))); }
    });
    group(ui, "Type", |ui| {
        if large(ui, "chart", "Change Chart Type", false, true, "Change to a different type of chart, or combine types").clicked() { crate::objects::open_dialog(app, crate::objects::ObjDialog::ChartType(ChartTypeDlg::new(app, si, i))); }
    });
    group(ui, "Title", |ui| {
        crate::theme::vcol(ui, |ui| {
            ui.label(tr("Chart Title:"));
            let id = egui::Id::new(("chart-title-edit", si, i));
            let mut t = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| c.title.clone().unwrap_or_default());
            let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(130.0).hint_text(tr("Automatic")));
            if r.changed() { ui.data_mut(|d| d.insert_temp(id, t.clone())); }
            if r.lost_focus() { ui.data_mut(|d| d.remove::<String>(id)); if c.title.as_deref() != Some(t.as_str()) { edit_chart(app, "Chart Title", |c| c.title = Some(t.clone())); } }
        });
    });
    crate::objects::arrange_group(app, ui);
}

pub fn switch_row_col(app: &mut SheetApp) {
    let Some((si, i)) = selected_chart(app) else { return };
    let Some(c) = app.book.sheets[si].objects[i].chart().cloned() else { return };
    let Some((sheet, r)) = c.source() else { return };
    let Some(src) = app.book.sheet_index(&sheet) else { return };
    let mut n = Chart::from_range(&app.book, src, r, c.kind, Some(!c.by_rows()));
    n.grouping = c.grouping; n.title = c.title.clone(); n.legend = c.legend.or(Some(LegendPos::Bottom)); n.data_labels = c.data_labels; n.gridlines = c.gridlines; n.x_title = c.x_title.clone(); n.y_title = c.y_title.clone(); n.palette = c.palette; n.markers = c.markers; n.part = c.part.clone();
    carry(&c, &mut n);
    edit_chart(app, "Switch Row/Column", |c| *c = n);
}

/// Keeps a chart's type details and per-series kinds, axes and markers on a rebuilt chart.
fn carry(old: &Chart, n: &mut Chart) {
    n.radar_style = old.radar_style; n.hi_low_lines = old.hi_low_lines; n.up_down_bars = old.up_down_bars; n.bubble_scale = old.bubble_scale; n.y2_title = old.y2_title.clone(); n.cat_axis_format = old.cat_axis_format.clone();
    for (s, o) in n.series.iter_mut().zip(&old.series) { s.kind = o.kind; s.secondary = o.secondary; s.marker = o.marker; }
}

// ---- Format Data Series ----------------------------------------------------------------------------------

/// The series under a point of a chart drawn in `r` (screen), if any.
pub fn series_at(app: &SheetApp, p: &egui::Painter, c: &Chart, r: Rect, z: f32, at: egui::Pos2) -> Option<usize> {
    let data = c.data(&app.book, app.sheet());
    let (prims, tags) = sheet_engine::chart::layout_tagged(c, &data, [r.min.x, r.min.y, r.max.x, r.max.y], &app.book.theme_colors, z, &ScreenMeasure(p));
    let near_seg = |a: [f32; 2], b: [f32; 2], tol: f32| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = (dx * dx + dy * dy).max(1e-6);
        let t = (((at.x - a[0]) * dx + (at.y - a[1]) * dy) / l2).clamp(0.0, 1.0);
        ((a[0] + t * dx - at.x).powi(2) + (a[1] + t * dy - at.y).powi(2)).sqrt() <= tol
    };
    for (prim, tag) in prims.iter().zip(&tags).rev() {
        let Some(k) = tag else { continue };
        let hit = match prim {
            Prim::Rect { r, .. } => at.x >= r[0] - 2.0 && at.x <= r[2] + 2.0 && at.y >= r[1] - 2.0 && at.y <= r[3] + 2.0,
            Prim::Poly { pts, .. } => { let (x0, y0, x1, y1) = pts.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(a, b, c, d), p| (a.min(p[0]), b.min(p[1]), c.max(p[0]), d.max(p[1]))); at.x >= x0 - 2.0 && at.x <= x1 + 2.0 && at.y >= y0 - 2.0 && at.y <= y1 + 2.0 }
            Prim::Line { pts, width, .. } => pts.windows(2).any(|w| near_seg(w[0], w[1], width / 2.0 + 4.0)),
            Prim::Text { .. } => false,
        };
        if hit { return Some(*k); }
    }
    None
}

/// The Format Data Series dialog: one series' look, axis and labels; bubble options.
#[derive(Clone)]
pub struct FormatSeries { pub sheet: usize, pub idx: usize, pub ser: usize, pub s: sheet_engine::chart::Series, pub labels: bool, pub bubble_scale: u32, pub neg: bool, pub width: bool }

impl FormatSeries {
    pub fn new(app: &SheetApp, si: usize, i: usize, ser: usize) -> FormatSeries {
        let c = app.book.sheets[si].objects[i].chart().cloned().unwrap_or_default();
        let ser = ser.min(c.series.len().saturating_sub(1));
        let s = c.series.get(ser).cloned().unwrap_or_default();
        FormatSeries { sheet: si, idx: i, ser, labels: s.labels.unwrap_or(c.data_labels), s, bubble_scale: if c.bubble_scale == 0 { 100 } else { c.bubble_scale }, neg: c.show_neg_bubbles, width: c.size_width }
    }
}

/// Opens Format Data Series for the selected chart (the first series).
pub fn open_format_series(app: &mut SheetApp, ser: usize) {
    let Some((si, i)) = selected_chart(app) else { return };
    crate::objects::open_dialog(app, crate::objects::ObjDialog::FormatSeries(FormatSeries::new(app, si, i, ser)));
}

/// An optional color: Automatic, or a picked one.
fn color_row(ui: &mut Ui, label: &str, c: &mut Option<[u8; 3]>, auto: [u8; 3], none_label: &str) {
    ui.label(tr(label));
    ui.horizontal(|ui| {
        let mut own = c.is_some();
        if ui.checkbox(&mut own, tr(none_label)).changed() { *c = if own { Some(auto) } else { None }; }
        if let Some(v) = c { ui.color_edit_button_srgb(v); }
    });
}

pub fn format_series_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut FormatSeries) -> bool {
    use sheet_engine::chart::{Marker, Symbol};
    let mut keep = true;
    let mut open = true;
    let Some(c) = app.book.sheets.get(d.sheet).and_then(|s| s.objects.get(d.idx)).and_then(|o| o.chart()).cloned() else { return false };
    if c.series.is_empty() { return false; }
    let data = c.data(&app.book, d.sheet);
    let theme = app.book.theme_colors.clone();
    let kind = c.series.get(d.ser).and_then(|s| s.kind).unwrap_or(c.kind);
    let line_kind = matches!(kind, ChartKind::Line | ChartKind::Scatter | ChartKind::Radar) && !(kind == ChartKind::Radar && c.radar_style == RadarStyle::Filled);
    let marks = matches!(kind, ChartKind::Line | ChartKind::Scatter | ChartKind::Radar | ChartKind::Stock);
    crate::theme::dialog(ctx, "Format Data Series").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(380.0);
        let name = |k: usize| data.get(k).map(|x| x.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Series{}", k + 1));
        egui::ComboBox::from_label(tr("Series")).selected_text(name(d.ser)).show_ui(ui, |ui| {
            for k in 0..c.series.len() { if ui.selectable_label(d.ser == k, name(k)).clicked() { *d = FormatSeries::new(app, d.sheet, d.idx, k); } }
        });
        ui.separator();
        let auto = c.color(d.ser, &theme);
        color_row(ui, if line_kind { "Line color:" } else { "Fill color:" }, &mut d.s.color, auto, "Custom");
        if !line_kind { color_row(ui, "Border color:", &mut d.s.line_color, [0x40, 0x40, 0x40], "Border"); }
        ui.horizontal(|ui| {
            ui.label(tr("Width (pt):"));
            let mut w = d.s.line_width.unwrap_or(if line_kind { 2.25 } else { 0.75 });
            if ui.add(egui::DragValue::new(&mut w).range(0.25..=20.0).speed(0.25)).changed() { d.s.line_width = Some(w); }
            ui.label(tr("Dash:"));
            const DASHES: [(&str, &str); 8] = [("solid", "Solid"), ("sysDot", "Round Dot"), ("sysDash", "Square Dot"), ("dash", "Dash"), ("dashDot", "Dash Dot"), ("lgDash", "Long Dash"), ("lgDashDot", "Long Dash Dot"), ("lgDashDotDot", "Long Dash Dot Dot")];
            let cur = d.s.dash.clone().unwrap_or_else(|| "solid".into());
            egui::ComboBox::from_id_salt("series-dash").selected_text(tr(DASHES.iter().find(|x| x.0 == cur).map_or("Dash", |x| x.1))).show_ui(ui, |ui| {
                for (code, label) in DASHES { if ui.selectable_label(cur == code, tr(label)).clicked() { d.s.dash = (code != "solid").then(|| code.to_string()); } }
            });
        });
        if matches!(kind, ChartKind::Line | ChartKind::Scatter) { ui.checkbox(&mut d.s.smooth, tr("Smoothed line")); }
        if marks {
            ui.separator();
            ui.label(egui::RichText::new(tr("Marker")).strong());
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(d.s.marker.is_none(), tr("Automatic")).clicked() { d.s.marker = None; }
                for sym in Symbol::ALL {
                    let label = match sym { Symbol::Auto => "Auto", Symbol::None => "None", Symbol::Circle => "Circle", Symbol::Square => "Square", Symbol::Diamond => "Diamond", Symbol::Triangle => "Triangle", Symbol::X => "X", Symbol::Star => "Star", Symbol::Dash => "Dash", Symbol::Dot => "Dot", Symbol::Plus => "Plus" };
                    if ui.selectable_label(d.s.marker.is_some_and(|m| m.symbol == sym), tr(label)).clicked() { d.s.marker = Some(Marker { symbol: sym, ..d.s.marker.unwrap_or_default() }); }
                }
            });
            if let Some(m) = &mut d.s.marker {
                ui.horizontal(|ui| {
                    ui.label(tr("Size:"));
                    let mut sz = m.size.unwrap_or(5);
                    if ui.add(egui::DragValue::new(&mut sz).range(2..=72)).changed() { m.size = Some(sz); }
                });
                color_row(ui, "Marker fill:", &mut m.fill, auto, "Custom");
                color_row(ui, "Marker border:", &mut m.line, auto, "Custom");
            }
        }
        if c.allows_secondary() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr("Plot series on:"));
                ui.radio_value(&mut d.s.secondary, false, tr("Primary Axis"));
                ui.radio_value(&mut d.s.secondary, true, tr("Secondary Axis"));
            });
        }
        ui.checkbox(&mut d.labels, tr("Data Labels"));
        if c.kind == ChartKind::Bubble {
            ui.separator();
            ui.label(egui::RichText::new(tr("Bubbles")).strong());
            ui.horizontal(|ui| { ui.label(tr("Bubble sizes:")); let mut t = d.s.size_ref.clone().unwrap_or_default(); if ui.add(egui::TextEdit::singleline(&mut t).desired_width(180.0)).changed() { d.s.size_ref = (!t.trim().is_empty()).then(|| t.trim().trim_start_matches('=').to_string()); } });
            ui.horizontal(|ui| { ui.label(tr("Scale bubble size to (%):")); ui.add(egui::DragValue::new(&mut d.bubble_scale).range(0..=300)); });
            ui.horizontal(|ui| { ui.label(tr("Size represents:")); ui.radio_value(&mut d.width, false, tr("Area of bubbles")); ui.radio_value(&mut d.width, true, tr("Width of bubbles")); });
            ui.checkbox(&mut d.neg, tr("Show negative bubbles"));
        }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let d2 = d.clone();
                app.checkpoint("Format Data Series");
                if let Some(c) = app.book.sheets[d2.sheet].objects[d2.idx].chart_mut() {
                    let all = c.data_labels;
                    if let Some(s) = c.series.get_mut(d2.ser) { *s = d2.s.clone(); s.labels = (d2.labels != all).then_some(d2.labels); }
                    if c.kind == ChartKind::Bubble { c.bubble_scale = if d2.bubble_scale == 100 && c.bubble_scale == 0 { 0 } else { d2.bubble_scale }; c.show_neg_bubbles = d2.neg; c.size_width = d2.width; }
                }
                if let Some(mut c2) = app.book.sheets[d2.sheet].objects[d2.idx].chart().cloned() {
                    c2.refresh_cache(&app.book, d2.sheet);
                    if let Some(c) = app.book.sheets[d2.sheet].objects[d2.idx].chart_mut() { *c = c2; }
                }
                app.dirty = true;
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

// ---- Select Data ------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct SelectData { pub sheet: usize, pub idx: usize, pub range: String, pub series: Vec<[String; 4]>, pub sel: usize, pub bubble: bool }

fn series_fields(s: &sheet_engine::chart::Series) -> [String; 4] { [s.name_ref.clone().unwrap_or_else(|| s.name.clone()), s.cat_ref.clone().unwrap_or_default(), s.val_ref.clone().unwrap_or_default(), s.size_ref.clone().unwrap_or_default()] }

impl SelectData {
    pub fn new(app: &SheetApp, si: usize, i: usize) -> SelectData {
        let c = app.book.sheets[si].objects[i].chart().cloned().unwrap_or_default();
        let range = c.source().map(|(s, r)| sheet_engine::chart::make_ref(&s, r)).unwrap_or_default();
        let series = c.series.iter().map(series_fields).collect();
        SelectData { sheet: si, idx: i, range, series, sel: 0, bubble: c.kind == ChartKind::Bubble }
    }
}

pub fn select_data_dialog(app: &mut SheetApp, ctx: &egui::Context, sd: &mut SelectData) -> bool {
    let mut keep = true;
    let mut open = true;
    if app.book.sheets.get(sd.sheet).and_then(|s| s.objects.get(sd.idx)).is_none_or(|o| !o.is_chart()) { return false; }
    crate::theme::dialog(ctx, "Select Data Source").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(460.0);
        crate::theme::hrow(ui, |ui| {
            ui.label(tr("Chart data range:"));
            crate::refedit::field(ui, &mut sd.range, "chart_ui.rs:553", 220.0);
            if ui.button(tr("Use Selection")).clicked() { sd.range = sheet_engine::chart::make_ref(&app.book.sheets[app.sheet()].name, app.sel.range()); }
            if ui.button(tr("Apply")).clicked() {
                if let Some((Some(sheet), r)) = sheet_engine::chart::parse_ref(&sd.range) .map(|(s, r)| (s.or(Some(app.book.sheets[sd.sheet].name.clone())), r)) {
                    if let Some(src) = app.book.sheet_index(&sheet) {
                        let old = app.book.sheets[sd.sheet].objects[sd.idx].chart().cloned().unwrap_or_default();
                        let mut n = Chart::from_range(&app.book, src, r, old.kind, None);
                        carry(&old, &mut n);
                        n.grouping = old.grouping; n.title = old.title; n.legend = old.legend; n.data_labels = old.data_labels; n.gridlines = old.gridlines; n.x_title = old.x_title; n.y_title = old.y_title; n.palette = old.palette; n.part = old.part;
                        sd.series = n.series.iter().map(series_fields).collect();
                        app.checkpoint("Select Data");
                        if let Some(c) = app.book.sheets[sd.sheet].objects[sd.idx].chart_mut() { *c = n; }
                        app.dirty = true;
                    }
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(tr("Legend Entries (Series)")).strong());
                egui::ScrollArea::vertical().max_height(150.0).id_salt("series-list").show(ui, |ui| {
                    ui.set_min_width(150.0);
                    for (k, s) in sd.series.iter().enumerate() { if ui.selectable_label(sd.sel == k, if s[0].is_empty() { format!("Series{}", k + 1) } else { s[0].clone() }).clicked() { sd.sel = k; } }
                });
                ui.horizontal(|ui| {
                    if ui.button(tr("Add")).clicked() { sd.series.push([format!("Series{}", sd.series.len() + 1), sd.series.first().map(|s| s[1].clone()).unwrap_or_default(), String::new(), String::new()]); sd.sel = sd.series.len() - 1; }
                    if ui.add_enabled(sd.series.len() > 1, egui::Button::new(tr("Remove"))).clicked() { sd.series.remove(sd.sel.min(sd.series.len() - 1)); sd.sel = sd.sel.min(sd.series.len() - 1); }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                if let Some(s) = sd.series.get_mut(sd.sel) {
                    let n = if sd.bubble { 4 } else { 3 };
                    for (k, label) in ["Series name:", "Category (X) values:", "Series values:", "Series bubble size:"].iter().enumerate().take(n) {
                        ui.label(tr(label));
                        ui.add(egui::TextEdit::singleline(&mut s[k]).desired_width(250.0));
                    }
                }
            });
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let series = sd.series.clone();
                app.checkpoint("Select Data");
                let wb_sheet = sd.sheet;
                if let Some(c) = app.book.sheets[wb_sheet].objects[sd.idx].chart_mut() {
                    let old = std::mem::take(&mut c.series);
                    c.series = series.iter().enumerate().map(|(k, s)| {
                        let base = old.get(k).cloned().unwrap_or_default();
                        let is_ref = |t: &str| sheet_engine::chart::parse_ref(t).is_some() && t.contains('!');
                        sheet_engine::chart::Series {
                            name: if is_ref(&s[0]) { base.name.clone() } else { s[0].clone() },
                            name_ref: is_ref(&s[0]).then(|| s[0].clone()),
                            cat_ref: (!s[1].trim().is_empty()).then(|| s[1].trim().trim_start_matches('=').to_string()),
                            val_ref: (!s[2].trim().is_empty()).then(|| s[2].trim().trim_start_matches('=').to_string()),
                            size_ref: (!s[3].trim().is_empty()).then(|| s[3].trim().trim_start_matches('=').to_string()),
                            ..base
                        }
                    }).collect();
                }
                if let Some(mut c2) = app.book.sheets[wb_sheet].objects[sd.idx].chart().cloned() {
                    c2.refresh_cache(&app.book, wb_sheet);
                    if let Some(c) = app.book.sheets[wb_sheet].objects[sd.idx].chart_mut() { *c = c2; }
                }
                app.dirty = true;
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}
