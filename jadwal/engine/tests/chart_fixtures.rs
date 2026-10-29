//! Charts written by another program (openpyxl 3.1, generated for these tests): the model read
//! from them, and that saving (kept and regenerated) reopens to the same model.

use sheet_engine::chart::{Chart, ChartKind, Grouping, LegendPos, Marker, Prim, RadarStyle, Symbol, TimeUnit};
use sheet_engine::Workbook;

fn open(name: &str) -> Workbook {
    let p = format!("{}/tests/fixtures/charts/{name}", env!("CARGO_MANIFEST_DIR"));
    sheet_engine::xlsx::read(&std::fs::read(p).unwrap()).unwrap()
}

fn charts(wb: &Workbook) -> Vec<Chart> { wb.sheets.iter().flat_map(|s| s.objects.iter().filter_map(|o| o.chart().cloned())).collect() }

/// The parts of the model a save must keep (caches and the cached name may be filled in).
fn model(c: &Chart) -> Chart {
    Chart { dirty: false, part: None, series: c.series.iter().map(|s| sheet_engine::chart::Series { cat_cache: vec![], val_cache: vec![], size_cache: vec![], color: None, name: if s.name_ref.is_some() { String::new() } else { s.name.clone() }, ..s.clone() }).collect(), ..c.clone() }
}

const ALL: [&str; 10] = ["format", "bars", "combo_secondary", "pie_doughnut", "scatter_bubble", "dates_crosssheet", "radar_stock", "legends_labels", "lines_areas", "types"];

#[test]
fn bars_kinds_groupings_and_axis_titles() {
    let cs = charts(&open("openpyxl_bars.xlsx"));
    let got: Vec<_> = cs.iter().map(|c| (c.kind, c.grouping)).collect();
    use ChartKind::*; use Grouping::*;
    assert_eq!(got, vec![(Column, Clustered), (Column, Stacked), (Column, Percent), (Bar, Clustered), (Bar, Stacked), (Bar, Percent)]);
    for c in &cs {
        // openpyxl writes axPos="l" on both axes: titles follow the axis role.
        assert_eq!(c.x_title.as_deref(), Some("Month"), "{:?}", c.title);
        assert_eq!(c.y_title.as_deref(), Some("Sales"));
        assert_eq!(c.series.len(), 3);
        assert_eq!(c.series[0].val_ref.as_deref(), Some("'Data'!$B$2:$B$7"));
        assert_eq!(c.series[0].cat_ref.as_deref(), Some("'Data'!$A$2:$A$7"));
    }
    let wb = open("openpyxl_bars.xlsx");
    let d = cs[0].data(&wb, 0);
    assert_eq!(d[1].name, "South");
    assert_eq!(d[0].cats[..2], ["Jan".to_string(), "Feb".to_string()]);
    assert_eq!(d[2].vals[1], Some(25.0));
}

#[test]
fn combo_secondary_axis() {
    let c = &charts(&open("openpyxl_combo_secondary.xlsx"))[0];
    assert_eq!(c.kind, ChartKind::Column);
    assert_eq!(c.series.len(), 3);
    assert_eq!(c.series[2].kind, Some(ChartKind::Line));
    // The line is on the secondary axis, with its own title.
    assert_eq!(c.series.iter().map(|s| s.secondary).collect::<Vec<_>>(), vec![false, false, true]);
    assert_eq!(c.y_title.as_deref(), Some("Units"));
    assert_eq!(c.y2_title.as_deref(), Some("East"));
}

#[test]
fn round_scatter_legends_dates() {
    let pie = charts(&open("openpyxl_pie_doughnut.xlsx"));
    assert_eq!((pie[0].kind, pie[0].data_labels, pie[0].percent_labels), (ChartKind::Pie, true, true));
    assert_eq!(pie[1].kind, ChartKind::Doughnut);
    let sc = charts(&open("openpyxl_scatter_bubble.xlsx"));
    assert_eq!(sc[0].kind, ChartKind::Scatter);
    assert_eq!((sc[0].x_title.as_deref(), sc[0].y_title.as_deref()), (Some("X"), Some("Y")));
    assert_eq!(sc[0].series.len(), 2);
    assert_eq!(sc[1].kind, ChartKind::Bubble);
    assert_eq!(sc[1].series[0].size_ref.as_deref(), Some("'Data'!$D$2:$D$9"));
    assert_eq!(sc[0].series[0].marker, Some(Marker { symbol: Symbol::Circle, size: None, ..Default::default() }));
    let lg: Vec<_> = charts(&open("openpyxl_legends_labels.xlsx")).iter().map(|c| c.legend).collect();
    use LegendPos::*;
    assert_eq!(lg, vec![Some(Right), Some(Left), Some(Top), Some(Bottom), Some(Right), None]);
    let wb = open("openpyxl_dates_crosssheet.xlsx");
    let c = &charts(&wb)[0];
    assert_eq!(c.x_title.as_deref(), Some("Date"));
    assert_eq!(c.series[0].val_ref.as_deref(), Some("'My Data'!$B$2:$B$11"));
    let d = c.data(&wb, 0);
    assert_eq!(d[0].name, "Visits");
    assert_eq!(d[0].cats[0], "1-Mar");
    assert!(c.date_axis);
    assert_eq!(c.cat_axis_format.as_deref(), Some("d-mmm"));
    // The untitled pie's series has no name: no automatic title.
    assert_eq!(charts(&wb)[1].title, None);
}

#[test]
fn radar_stock_bubble_kinds() {
    let wb = open("openpyxl_radar_stock.xlsx");
    let cs = charts(&wb);
    assert_eq!((cs[0].kind, cs[0].radar_style, cs[0].markers), (ChartKind::Radar, RadarStyle::Marker, false));
    assert_eq!((cs[1].kind, cs[1].hi_low_lines, cs[1].up_down_bars), (ChartKind::Stock, true, false));
    assert_eq!(cs[1].series.len(), 3);
    let wb = open("openpyxl_types.xlsx");
    let cs = charts(&wb);
    use ChartKind::*;
    assert_eq!(cs.iter().map(|c| c.kind).collect::<Vec<_>>(), vec![Radar, Radar, Line, Column, Column, Stock, Bubble]);
    assert_eq!(cs[0].radar_style, RadarStyle::Standard);
    assert_eq!(cs[1].radar_style, RadarStyle::Filled);
    // Per-series markers.
    let m: Vec<_> = cs[2].series.iter().map(|s| s.marker).collect();
    assert_eq!(m, vec![Some(Marker { symbol: Symbol::Diamond, size: Some(9), ..Default::default() }), Some(Marker { symbol: Symbol::X, size: Some(4), ..Default::default() }), Some(Marker { symbol: Symbol::Dash, size: Some(7), ..Default::default() })]);
    // No title element and no autoTitleDeleted: the automatic title, the series name.
    assert_eq!(cs[3].title, Some(String::new()));
    assert_eq!(cs[3].title_text(&cs[3].data(&wb, 0)).as_deref(), Some("North"));
    // A column series on the secondary axis.
    assert_eq!(cs[4].series.iter().map(|s| (s.kind, s.secondary)).collect::<Vec<_>>(), vec![(None, false), (None, true)]);
    assert_eq!((cs[4].y_title.as_deref(), cs[4].y2_title.as_deref()), (Some("North"), Some("East")));
    // Open-high-low-close with up/down bars.
    assert_eq!((cs[5].series.len(), cs[5].hi_low_lines, cs[5].up_down_bars), (4, true, true));
    // Bubbles: sizes from the third range, scaled.
    assert_eq!(cs[6].bubble_scale, 50);
    let d = cs[6].data(&wb, 1);
    assert_eq!(d[0].sizes[..3], [Some(1.0), Some(2.0), Some(3.0)]);
    assert_eq!(d[0].xs[1], 1.5);
}

#[test]
fn layouts_of_every_fixture_chart() {
    struct M;
    impl sheet_engine::chart::Measure for M { fn size(&self, t: &str, px: f32, _: bool) -> [f32; 2] { [t.chars().count() as f32 * px * 0.5, px * 1.2] } }
    for f in ALL {
        let wb = open(&format!("openpyxl_{f}.xlsx"));
        for (si, s) in wb.sheets.iter().enumerate() {
            for c in s.objects.iter().filter_map(|o| o.chart()) {
                let prims = sheet_engine::chart::layout(c, &c.data(&wb, si), [0.0, 0.0, 480.0, 290.0], &wb.theme_colors, 1.0, &M);
                let marks = prims.iter().filter(|p| !matches!(p, sheet_engine::chart::Prim::Text { .. })).count();
                assert!(marks > 5, "{f} {:?}: {marks}", c.kind);
                for p in &prims { if let sheet_engine::chart::Prim::Poly { pts, strip: true, .. } = p { assert_eq!(pts.len() % 2, 0, "{f}"); } }
            }
        }
    }
}

#[test]
fn save_keeps_and_regenerates_the_model() {
    for f in ALL {
        let wb = open(&format!("openpyxl_{f}.xlsx"));
        let before = charts(&wb);
        assert!(!before.is_empty(), "{f}");
        let mut regen = wb.clone();
        for s in &mut regen.sheets { for o in &mut s.objects { let _ = o.chart_mut(); } }
        for (label, w) in [("keep", &wb), ("regen", &regen)] {
            let back = sheet_engine::xlsx::read(&sheet_engine::xlsx::write(w).unwrap()).unwrap();
            let after = charts(&back);
            assert_eq!(before.len(), after.len(), "{f} {label}");
            for (a, b) in before.iter().zip(&after) { assert_eq!(model(a), model(b), "{f} {label}"); }
        }
    }
}

struct M;
impl sheet_engine::chart::Measure for M { fn size(&self, t: &str, px: f32, _: bool) -> [f32; 2] { [t.chars().count() as f32 * px * 0.5, px * 1.2] } }

fn texts(p: &[Prim]) -> Vec<(String, [f32; 2])> { p.iter().filter_map(|p| if let Prim::Text { text, at, .. } = p { Some((text.clone(), *at)) } else { None }).collect() }

#[test]
fn series_format_secondary_scatter_and_dates() {
    let wb = open("openpyxl_format.xlsx");
    let cs = charts(&wb);
    // Scatter with a series on secondary value axes.
    let sc = &cs[0];
    assert_eq!(sc.kind, ChartKind::Scatter);
    assert_eq!(sc.series.iter().map(|s| s.secondary).collect::<Vec<_>>(), vec![false, true]);
    assert_eq!((sc.y_title.as_deref(), sc.y2_title.as_deref()), (Some("Squares"), Some("Falling")));
    let xml = sc.to_xml(&wb, 0);
    assert_eq!(xml.matches("<c:valAx>").count(), 4, "two value axis pairs");
    let back = sheet_engine::chart::parse(&xml, &wb.theme_colors).unwrap();
    assert_eq!(back.series[1].secondary, true);
    assert_eq!(back.y2_title.as_deref(), Some("Falling"));
    // The secondary axis's ticks (to 1000) beside the primary's (to 80).
    let tx = texts(&sheet_engine::chart::layout(sc, &sc.data(&wb, 0), [0.0, 0.0, 480.0, 300.0], &wb.theme_colors, 1.0, &M));
    assert!(tx.iter().any(|t| t.0 == "1000") && tx.iter().any(|t| t.0 == "80"), "{tx:?}");
    // Series formatting.
    let ln = &cs[1];
    let (a, b) = (&ln.series[0], &ln.series[1]);
    assert_eq!((a.smooth, a.line_width, a.dash.as_deref(), a.color), (true, Some(4.0), Some("dash"), Some([255, 0, 0])));
    assert_eq!(b.marker, Some(Marker { symbol: Symbol::Square, size: Some(8), fill: Some([255, 255, 0]), line: Some([0, 0, 255]) }));
    assert_eq!((a.labels, b.labels), (None, Some(true)));
    // The date axis: months, every 2, fixed bounds; points placed by date (April is a gap).
    assert!(ln.date_axis);
    assert_eq!((ln.base_unit, ln.major_unit, ln.major_time_unit, ln.axis_min, ln.axis_max), (Some(TimeUnit::Months), Some(2.0), Some(TimeUnit::Months), Some(45292.0), Some(45580.0)));
    let tx = texts(&sheet_engine::chart::layout(ln, &ln.data(&wb, 0), [0.0, 0.0, 600.0, 300.0], &wb.theme_colors, 1.0, &M));
    let labels: Vec<&str> = tx.iter().map(|t| t.0.as_str()).filter(|t| t.contains("-24")).collect();
    assert_eq!(labels, vec!["Jan-24", "Mar-24", "May-24", "Jul-24", "Sep-24"], "a label every two months");
    let x = |l: &str| tx.iter().find(|t| t.0 == l).unwrap().1[0];
    // Ten month slots from Jan to Oct: May sits four slots after January, not three.
    assert!(((x("May-24") - x("Jan-24")) / (x("Mar-24") - x("Jan-24")) - 2.0).abs() < 0.01);
    // Bars with an outline.
    let bar = &cs[2];
    assert_eq!((bar.series[0].color, bar.series[0].line_color, bar.series[0].line_width), (Some([0x70, 0xAD, 0x47]), Some([0, 0, 0]), Some(1.5)));
    // Bubbles: negatives shown, size as width, scale.
    let bb = &cs[3];
    assert_eq!((bb.show_neg_bubbles, bb.size_width, bb.bubble_scale), (true, true, 150));
    let prims = sheet_engine::chart::layout(bb, &bb.data(&wb, 0), [0.0, 0.0, 480.0, 300.0], &wb.theme_colors, 1.0, &M);
    assert!(prims.iter().any(|p| matches!(p, Prim::Poly { fill: None, stroke: Some(_), .. })), "negative bubbles as outlines");
    // A long title and axis titles wrap.
    let t = &cs[4];
    let tx = texts(&sheet_engine::chart::layout(t, &t.data(&wb, 0), [0.0, 0.0, 260.0, 300.0], &wb.theme_colors, 1.0, &M));
    let title_lines = tx.iter().filter(|x| t.title.as_deref().unwrap().contains(x.0.as_str()) && x.0.len() > 3).count();
    assert!(title_lines >= 2, "{tx:?}");
    // Each title line fits the chart's width (18 px text, 9 px a character here).
    assert!(tx.iter().take(title_lines).all(|x| x.0.chars().count() as f32 * 9.0 <= 244.0), "{tx:?}");
    // The y title wraps into two rotated lines side by side.
    assert!(tx.iter().any(|x| x.0 == "month") && tx.iter().any(|x| x.0.starts_with("Sales in")));
}
