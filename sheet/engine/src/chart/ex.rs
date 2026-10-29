//! Office 2016 charts (chartex): waterfall, histogram, Pareto, box & whisker, treemap,
//! sunburst and funnel. They live in their own part (xl/charts/chartExN.xml, the cx: namespace)
//! which a drawing names from an `mc:AlternateContent` frame. The model is the common `Chart`
//! (kind, title, legend, series by reference) plus `ExOpts`; hierarchy categories (several
//! label columns) come to the layout as one path per point, levels joined by `LEVEL_SEP`.

use super::{make_ref, parse_ref, ref_values, Chart, ChartKind, LegendPos, Series, SeriesData};
use crate::addr::{Pos, Range};
use crate::book::Workbook;
use crate::value::Value;
use crate::xlsx::xml::{attr, text, Element};

pub const NS_CX: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";
pub const RT: &str = "http://schemas.microsoft.com/office/2014/relationships/chartEx";
pub const CT: &str = "application/vnd.ms-office.chartex+xml";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// Between the levels of a hierarchy category (root first).
pub const LEVEL_SEP: char = '\u{1F}';

/// The chartex kinds' own settings.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ExOpts {
    /// Waterfall: the points that are totals (drawn from zero).
    pub subtotals: Vec<usize>,
    /// Histogram: a fixed bin width or bin count (else automatic, Scott's rule as Excel).
    pub bin_width: Option<f64>,
    pub bin_count: Option<u32>,
    /// Box & whisker: exclusive quartiles (Excel's default) or inclusive; mean marker shown;
    /// outlier points shown.
    pub inclusive: bool,
    pub hide_mean: bool,
    pub hide_outliers: bool,
}

impl ChartKind {
    /// The Office 2016 kinds (written as chartex parts).
    pub fn is_ex(self) -> bool { matches!(self, ChartKind::Waterfall | ChartKind::Histogram | ChartKind::Pareto | ChartKind::BoxWhisker | ChartKind::Treemap | ChartKind::Sunburst | ChartKind::Funnel) }
    pub fn is_hierarchy(self) -> bool { matches!(self, ChartKind::Treemap | ChartKind::Sunburst) }
    fn layout_id(self) -> &'static str {
        match self { ChartKind::Waterfall => "waterfall", ChartKind::Histogram | ChartKind::Pareto => "clusteredColumn", ChartKind::BoxWhisker => "boxWhisker", ChartKind::Treemap => "treemap", ChartKind::Sunburst => "sunburst", ChartKind::Funnel => "funnel", _ => "clusteredColumn" }
    }
}

/// The Insert ▸ Hierarchy, Statistic or Waterfall menu: (group, kinds).
pub const MENU: &[(&str, &[ChartKind])] = &[
    ("Hierarchy", &[ChartKind::Treemap, ChartKind::Sunburst]),
    ("Statistic", &[ChartKind::Histogram, ChartKind::Pareto, ChartKind::BoxWhisker]),
    ("Waterfall, Funnel", &[ChartKind::Waterfall, ChartKind::Funnel]),
];

fn cell_text(wb: &Workbook, si: usize, p: Pos) -> String {
    let s = &wb.sheets[si];
    let v = s.value(p);
    if v.is_empty() { return String::new(); }
    crate::numfmt::format(&v, &wb.style_at(si, p).num_fmt, wb.date1904).text
}

/// Category paths of a (possibly several columns wide) reference, row by row; blank parents
/// repeat the row above.
fn paths(wb: &Workbook, sheet: usize, f: &str) -> Option<Vec<String>> {
    let (name, r) = parse_ref(f)?;
    let si = match name { Some(n) => wb.sheet_index(&n)?, None => sheet };
    let r = Range { r1: r.r1.min(r.r0 + 3999), ..r };
    let mut prev: Vec<String> = vec![String::new(); r.cols() as usize];
    let mut out = vec![];
    for row in r.r0..=r.r1 {
        let mut cur: Vec<String> = (r.c0..=r.c1).map(|c| cell_text(wb, si, Pos::new(row, c))).collect();
        let last = cur.iter().rposition(|t| !t.is_empty()).unwrap_or(0);
        for (i, t) in cur.iter_mut().enumerate().take(last) { if t.is_empty() { *t = prev[i].clone(); } }
        out.push(cur[..=last].join(&LEVEL_SEP.to_string()));
        prev = cur;
    }
    Some(out)
}

/// The data to draw (Chart::data for chartex kinds).
pub fn data(c: &Chart, wb: &Workbook, sheet: usize) -> Vec<SeriesData> {
    c.series.iter().map(|s| {
        let name = s.name_ref.as_deref().and_then(|f| ref_values(wb, sheet, f)).and_then(|v| v.first().map(|x| match x { Value::Str(t) => t.to_string(), Value::Num(n) => super::layout::value_text(*n), _ => String::new() })).filter(|n| !n.is_empty()).unwrap_or_else(|| s.name.clone());
        let vals: Vec<Option<f64>> = match s.val_ref.as_deref().and_then(|f| ref_values(wb, sheet, f)) {
            Some(v) => v.iter().map(|x| match x { Value::Num(n) => Some(*n), Value::Str(t) => t.trim().parse().ok(), _ => None }).collect(),
            None => s.val_cache.iter().map(|v| Some(*v)).collect(),
        };
        let cats = s.cat_ref.as_deref().and_then(|f| paths(wb, sheet, f)).unwrap_or_else(|| s.cat_cache.clone());
        SeriesData { name, cats, vals, ..Default::default() }
    }).collect()
}

/// A chartex chart over a range: label columns (text) first, then value columns; a first row
/// of text is the header.
pub fn from_range(wb: &Workbook, sheet: usize, r: Range, kind: ChartKind) -> Chart {
    let s = &wb.sheets[sheet];
    let num = |p: Pos| matches!(s.value(p), Value::Num(_));
    let head = r.rows() > 1 && (r.c0..=r.c1).all(|c| !num(Pos::new(r.r0, c)));
    let body = Range { r0: r.r0 + head as u32, ..r };
    // Label columns: those with text in the body (left of the first numeric column).
    let first_num = (r.c0..=r.c1).find(|&c| (body.r0..=body.r1.min(body.r0 + 50)).any(|row| num(Pos::new(row, c)))).unwrap_or(r.c1);
    let labels = (first_num > r.c0).then(|| Range { c0: r.c0, c1: first_num - 1, ..body });
    let name = &s.name;
    let mut series = vec![];
    let cols: Vec<u32> = match kind { ChartKind::BoxWhisker => (first_num..=r.c1).collect(), _ => vec![first_num] };
    for c in cols {
        let labels = match kind { ChartKind::Treemap | ChartKind::Sunburst => labels, _ => labels.map(|l| Range { c0: l.c1, ..l }) };
        series.push(Series {
            name_ref: head.then(|| make_ref(name, Range::cell(Pos::new(r.r0, c)))),
            cat_ref: if kind == ChartKind::Histogram { None } else { labels.map(|l| make_ref(name, l)) },
            val_ref: Some(make_ref(name, Range { c0: c, c1: c, ..body })),
            ..Default::default()
        });
    }
    let mut c = Chart { kind, title: Some(String::new()), legend: matches!(kind, ChartKind::Waterfall | ChartKind::Sunburst | ChartKind::Treemap).then_some(LegendPos::Top).filter(|_| kind != ChartKind::Sunburst), gridlines: true, series, dirty: true, ..Default::default() };
    if kind == ChartKind::BoxWhisker && c.series.len() > 1 { c.legend = Some(LegendPos::Bottom); }
    c.refresh_cache(wb, sheet);
    c
}

// ---- reading -------------------------------------------------------------------------------

fn lvl_points(l: &Element) -> Vec<String> {
    let n = l.attr_u32("ptCount").unwrap_or(0) as usize;
    let mut v = vec![String::new(); n];
    for p in l.all("pt") { let i = p.attr_u32("idx").unwrap_or(0) as usize; if i >= v.len() { v.resize(i + 1, String::new()); } v[i] = p.text(); }
    v
}

fn tx(e: &Element) -> (Option<String>, String) {
    let d = e.child("tx").and_then(|t| t.child("txData"));
    let f = d.and_then(|d| d.child("f")).map(|f| f.text());
    let v = d.and_then(|d| d.child("v")).map(|v| v.text()).unwrap_or_else(|| { let mut s = String::new(); fn walk(e: &Element, s: &mut String) { for c in e.elements() { if c.local() == "t" { s.push_str(&c.text()); } else { walk(c, s); } } } if let Some(t) = e.child("tx") { walk(t, &mut s); } s });
    (f, v)
}

/// A chartex part (cx:chartSpace) as a chart, if it's one of the modeled kinds.
pub fn parse(xml: &str) -> Option<Chart> {
    let root = Element::parse(xml).ok()?;
    if root.local() != "chartSpace" { return None; }
    struct Dat { cat_f: Option<String>, cats: Vec<String>, val_f: Option<String>, vals: Vec<f64> }
    let mut data: Vec<(String, Dat)> = vec![];
    for d in root.child("chartData").into_iter().flat_map(|c| c.all("data")) {
        let mut x = Dat { cat_f: None, cats: vec![], val_f: None, vals: vec![] };
        for dim in d.elements() {
            let f = dim.child("f").map(|f| f.text());
            let lvls: Vec<Vec<String>> = dim.all("lvl").map(lvl_points).collect();
            match dim.local() {
                "strDim" | "numDim" if dim.attr("type") == Some("cat") => {
                    x.cat_f = f;
                    // Levels come leaf first; paths are root first.
                    let n = lvls.first().map_or(0, |l| l.len());
                    x.cats = (0..n).map(|i| lvls.iter().rev().map(|l| l.get(i).cloned().unwrap_or_default()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(&LEVEL_SEP.to_string())).collect();
                }
                "numDim" => { x.val_f = f; x.vals = lvls.first().map(|l| l.iter().map(|t| t.parse().unwrap_or(0.0)).collect()).unwrap_or_default(); }
                _ => {}
            }
        }
        data.push((d.attr("id").unwrap_or("0").to_string(), x));
    }
    let chart = root.child("chart")?;
    let region = chart.child("plotArea")?.child("plotAreaRegion")?;
    let all: Vec<&Element> = region.all("series").collect();
    let pareto = all.iter().any(|s| s.attr("layoutId") == Some("paretoLine"));
    let first = all.iter().find(|s| s.attr("layoutId") != Some("paretoLine"))?;
    let kind = match first.attr("layoutId")? {
        "waterfall" => ChartKind::Waterfall, "boxWhisker" => ChartKind::BoxWhisker, "treemap" => ChartKind::Treemap, "sunburst" => ChartKind::Sunburst, "funnel" => ChartKind::Funnel,
        "clusteredColumn" => if pareto { ChartKind::Pareto } else { ChartKind::Histogram },
        _ => return None,
    };
    let mut c = Chart { kind, legend: None, gridlines: true, ..Default::default() };
    if let Some(t) = chart.child("title") { let (f, v) = tx(t); c.title_ref = f; c.title = Some(v); }
    if let Some(l) = chart.child("legend") { c.legend = Some(match l.attr("pos") { Some("l") => LegendPos::Left, Some("r") => LegendPos::Right, Some("b") => LegendPos::Bottom, _ => LegendPos::Top }); }
    for s in all.iter().filter(|s| s.attr("layoutId") != Some("paretoLine")) {
        let id = s.child("dataId").and_then(|d| d.attr("val")).unwrap_or("0");
        let Some((_, d)) = data.iter().find(|(i, _)| i == id) else { continue };
        let (nf, nv) = tx(s);
        c.series.push(Series { name: nv, name_ref: nf, cat_ref: d.cat_f.clone(), val_ref: d.val_f.clone(), cat_cache: d.cats.clone(), val_cache: d.vals.clone(), ..Default::default() });
        if let Some(lp) = s.child("layoutPr") {
            if let Some(st) = lp.child("subtotals") { c.ex.subtotals = st.all("idx").filter_map(|i| i.attr_u32("val").map(|v| v as usize)).collect(); }
            if let Some(b) = lp.child("binning") { c.ex.bin_width = b.child("binSize").and_then(|x| x.attr_f64("val")); c.ex.bin_count = b.child("binCount").and_then(|x| x.attr_u32("val")); }
            if let Some(v) = lp.child("visibility") { c.ex.hide_mean = !v.flag("meanMarker", true); c.ex.hide_outliers = !v.flag("outliers", true); }
            if let Some(q) = lp.child("statistics") { c.ex.inclusive = q.attr("quartileMethod") == Some("inclusive"); }
        }
        if s.child("dataLabels").is_some() { c.data_labels = true; }
    }
    if c.series.is_empty() { return None; }
    Some(c)
}

/// The chart of an `mc:AlternateContent` drawing frame naming a chartex part (drawing::load).
pub fn kind_of_frame(wb: &Workbook, content: &Element, rel_target: &dyn Fn(&str) -> Option<String>) -> Option<crate::drawing::ObjKind> {
    fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }
    let choice = content.child("Choice")?;
    let cx = find(choice, "chart").filter(|c| c.name.starts_with("cx") || c.attrs.iter().any(|(_, v)| v == NS_CX))?;
    let id = cx.attrs.iter().find(|(k, _)| k.ends_with(":id")).map(|(_, v)| v.clone())?;
    let part = rel_target(&id)?;
    let mut ch = parse(&crate::xlsx::xml::part_text(wb.parts.get(&part)?))?;
    ch.part = Some(part);
    Some(crate::drawing::ObjKind::Chart(Box::new(ch)))
}

// ---- writing -------------------------------------------------------------------------------

fn tx_xml(f: Option<&str>, v: &str) -> String {
    match f { Some(f) => format!("<cx:tx><cx:txData><cx:f>{}</cx:f><cx:v>{}</cx:v></cx:txData></cx:tx>", text(f), text(v)), None => format!("<cx:tx><cx:txData><cx:v>{}</cx:v></cx:txData></cx:tx>", text(v)) }
}

/// The chartex part.
pub fn to_xml(c: &Chart, wb: &Workbook, sheet: usize) -> String {
    let data = c.data(wb, sheet);
    let mut o = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cx:chartSpace xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:cx=\"{NS_CX}\"><cx:chartData>");
    for (i, (s, d)) in c.series.iter().zip(&data).enumerate() {
        o.push_str(&format!("<cx:data id=\"{i}\">"));
        if !d.cats.is_empty() {
            let depth = d.cats.iter().map(|p| p.split(LEVEL_SEP).count()).max().unwrap_or(1);
            o.push_str("<cx:strDim type=\"cat\">");
            if let Some(f) = &s.cat_ref { o.push_str(&format!("<cx:f>{}</cx:f>", text(f))); }
            for lv in (0..depth).rev() {
                o.push_str(&format!("<cx:lvl ptCount=\"{}\">", d.cats.len()));
                for (k, p) in d.cats.iter().enumerate() { if let Some(t) = p.split(LEVEL_SEP).nth(lv) { o.push_str(&format!("<cx:pt idx=\"{k}\">{}</cx:pt>", text(t))); } }
                o.push_str("</cx:lvl>");
            }
            o.push_str("</cx:strDim>");
        }
        o.push_str("<cx:numDim type=\"val\">");
        if let Some(f) = &s.val_ref { o.push_str(&format!("<cx:f>{}</cx:f>", text(f))); }
        o.push_str(&format!("<cx:lvl ptCount=\"{}\" formatCode=\"General\">", d.vals.len()));
        for (k, v) in d.vals.iter().enumerate() { if let Some(v) = v { o.push_str(&format!("<cx:pt idx=\"{k}\">{}</cx:pt>", crate::value::fmt_num(*v))); } }
        o.push_str("</cx:lvl></cx:numDim></cx:data>");
    }
    o.push_str("</cx:chartData><cx:chart>");
    if let Some(t) = &c.title { o.push_str(&format!("<cx:title pos=\"t\" align=\"ctr\" overlay=\"0\">{}</cx:title>", tx_xml(c.title_ref.as_deref(), &if t.is_empty() { c.title_text(&data).unwrap_or_default() } else { t.clone() }))); }
    o.push_str("<cx:plotArea><cx:plotAreaRegion>");
    for (i, (s, d)) in c.series.iter().zip(&data).enumerate() {
        o.push_str(&format!("<cx:series layoutId=\"{}\" uniqueId=\"{{0000000{i}-0000-4000-8000-00000000000{i}}}\">", c.kind.layout_id()));
        if !d.name.is_empty() { o.push_str(&tx_xml(s.name_ref.as_deref(), &d.name)); }
        if c.data_labels { o.push_str("<cx:dataLabels pos=\"outEnd\"><cx:visibility seriesName=\"0\" categoryName=\"0\" value=\"1\"/></cx:dataLabels>"); }
        o.push_str(&format!("<cx:dataId val=\"{i}\"/>"));
        match c.kind {
            ChartKind::Waterfall => { o.push_str("<cx:layoutPr><cx:subtotals>"); for k in &c.ex.subtotals { o.push_str(&format!("<cx:idx val=\"{k}\"/>")); } o.push_str("</cx:subtotals></cx:layoutPr>"); }
            ChartKind::Histogram => {
                o.push_str("<cx:layoutPr><cx:binning intervalClosed=\"r\">");
                if let Some(w) = c.ex.bin_width { o.push_str(&format!("<cx:binSize val=\"{}\"/>", crate::value::fmt_num(w))); } else if let Some(n) = c.ex.bin_count { o.push_str(&format!("<cx:binCount val=\"{n}\"/>")); }
                o.push_str("</cx:binning></cx:layoutPr>");
            }
            ChartKind::Pareto => o.push_str("<cx:layoutPr><cx:aggregation/></cx:layoutPr><cx:axisId val=\"1\"/>"),
            ChartKind::BoxWhisker => o.push_str(&format!("<cx:layoutPr><cx:visibility meanLine=\"0\" meanMarker=\"{}\" nonoutliers=\"0\" outliers=\"{}\"/><cx:statistics quartileMethod=\"{}\"/></cx:layoutPr>", !c.ex.hide_mean as u8, !c.ex.hide_outliers as u8, if c.ex.inclusive { "inclusive" } else { "exclusive" })),
            ChartKind::Treemap => o.push_str("<cx:layoutPr><cx:parentLabelLayout val=\"overlapping\"/></cx:layoutPr>"),
            _ => {}
        }
        o.push_str("</cx:series>");
        if c.kind == ChartKind::Pareto && i == 0 { o.push_str("<cx:series layoutId=\"paretoLine\" ownerIdx=\"0\" uniqueId=\"{00000009-0000-4000-8000-000000000009}\"><cx:axisId val=\"2\"/></cx:series>"); }
    }
    o.push_str("</cx:plotAreaRegion>");
    let grid = if c.gridlines { "<cx:majorGridlines/>" } else { "" };
    match c.kind {
        ChartKind::Treemap | ChartKind::Sunburst => {}
        ChartKind::Funnel => o.push_str("<cx:axis id=\"0\"><cx:catScaling gapWidth=\"0.06\"/><cx:tickLabels/></cx:axis>"),
        ChartKind::Pareto => o.push_str(&format!("<cx:axis id=\"0\"><cx:catScaling gapWidth=\"0\"/><cx:tickLabels/></cx:axis><cx:axis id=\"1\"><cx:valScaling/>{grid}<cx:tickLabels/></cx:axis><cx:axis id=\"2\"><cx:valScaling max=\"1\" min=\"0\"/><cx:units unit=\"percentage\"/><cx:tickLabels/></cx:axis>")),
        _ => o.push_str(&format!("<cx:axis id=\"0\"><cx:catScaling gapWidth=\"{}\"/><cx:tickLabels/></cx:axis><cx:axis id=\"1\"><cx:valScaling/>{grid}<cx:tickLabels/></cx:axis>", if c.kind == ChartKind::Histogram { "0" } else { "0.5" })),
    }
    o.push_str("</cx:plotArea>");
    if let Some(l) = c.legend { o.push_str(&format!("<cx:legend pos=\"{}\" align=\"ctr\" overlay=\"0\"/>", match l { LegendPos::Left => "l", LegendPos::Right => "r", LegendPos::Bottom => "b", LegendPos::Top => "t" })); }
    o.push_str("</cx:chart></cx:chartSpace>");
    o
}

/// The drawing frame for a chartex chart: Excel 2016+ shows the chart, older readers a note.
/// The Choice requires the chartex revision that introduced the layout, as Excel writes it:
/// cx1 (2015/9/8) for the 2016 charts, cx2 (2015/10/21) for funnels.
pub fn frame_xml(id: u32, name: &str, rid: &str, kind: ChartKind) -> String {
    let req = if kind == ChartKind::Funnel { "xmlns:cx2=\"http://schemas.microsoft.com/office/drawing/2015/10/21/chartex\" Requires=\"cx2\"" } else { "xmlns:cx1=\"http://schemas.microsoft.com/office/drawing/2015/9/8/chartex\" Requires=\"cx1\"" };
    format!("<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice {req}><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{n}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chart xmlns:cx=\"{NS_CX}\" r:id=\"{rid}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice>\
<mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"{n}\"/><xdr:cNvSpPr><a:spLocks noTextEdit=\"1\"/></xdr:cNvSpPr></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"4572000\" cy=\"2743200\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:prstClr val=\"white\"/></a:solidFill><a:ln w=\"1\"><a:solidFill><a:prstClr val=\"green\"/></a:solidFill></a:ln></xdr:spPr><xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\"/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1100\"/><a:t>This chart isn't available in your version of Excel.</a:t></a:r></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent>", n = attr(name))
}

// ---- statistics (shared with the layout) ---------------------------------------------------

/// Histogram bins over `v`: (lower, upper, count) each, intervals closed on the right.
pub fn bins(v: &[f64], width: Option<f64>, count: Option<u32>) -> Vec<(f64, f64, usize)> {
    if v.is_empty() { return vec![]; }
    let (lo, hi) = v.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    let n = v.len() as f64;
    let w = match (width, count) {
        (Some(w), _) if w > 0.0 => w,
        (_, Some(k)) if k > 0 => ((hi - lo) / k as f64).max(1e-9),
        _ => {
            let mean = v.iter().sum::<f64>() / n;
            let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
            let w = 3.5 * sd / n.cbrt();
            if w > 0.0 { w } else { 1.0 }
        }
    };
    let k = (((hi - lo) / w).ceil() as usize).clamp(1, 500);
    let mut out: Vec<(f64, f64, usize)> = (0..k).map(|i| (lo + i as f64 * w, lo + (i + 1) as f64 * w, 0)).collect();
    for &x in v { let i = if x <= lo { 0 } else { (((x - lo) / w).ceil() as usize).saturating_sub(1).min(k - 1) }; out[i].2 += 1; }
    out
}

/// A sorted sample's quartile (`q` 1-3), exclusive (Excel's default) or inclusive.
pub fn quartile(s: &[f64], q: usize, inclusive: bool) -> f64 {
    let n = s.len();
    if n == 0 { return 0.0; }
    if n == 1 { return s[0]; }
    let p = q as f64 / 4.0;
    let h = if inclusive { (n - 1) as f64 * p } else { (n + 1) as f64 * p - 1.0 };
    let h = h.clamp(0.0, (n - 1) as f64);
    let (i, f) = (h.floor() as usize, h.fract());
    if i + 1 < n { s[i] + f * (s[i + 1] - s[i]) } else { s[i] }
}

/// Box plot figures of a sample: (min whisker, q1, median, q3, max whisker, mean, outliers).
pub fn box_stats(v: &[f64], inclusive: bool) -> Option<(f64, f64, f64, f64, f64, f64, Vec<f64>)> {
    if v.is_empty() { return None; }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let (q1, q2, q3) = (quartile(&s, 1, inclusive), quartile(&s, 2, inclusive), quartile(&s, 3, inclusive));
    let iqr = q3 - q1;
    let (lo_f, hi_f) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
    let inside: Vec<f64> = s.iter().copied().filter(|x| *x >= lo_f && *x <= hi_f).collect();
    let out: Vec<f64> = s.iter().copied().filter(|x| *x < lo_f || *x > hi_f).collect();
    let mean = s.iter().sum::<f64>() / s.len() as f64;
    Some((inside.first().copied().unwrap_or(q1), q1, q2, q3, inside.last().copied().unwrap_or(q3), mean, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::parse_range;
    use crate::drawing::{add_object, Anchor, ObjKind, Object};

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        let rows = [["Group", "Item", "Amount"], ["Fruit", "Apple", "10"], ["", "Pear", "6"], ["Veg", "Leek", "4"], ["Veg", "Kale", "-3"], ["Nuts", "Pecan", "8"]];
        for (r, row) in rows.iter().enumerate() { for (c, t) in row.iter().enumerate() { if !t.is_empty() { wb.set_input(0, Pos::new(r as u32, c as u32), t); } } }
        wb.recalc();
        wb
    }

    #[test]
    fn hierarchy_paths_fill_down() {
        let wb = book();
        let c = from_range(&wb, 0, parse_range("A1:C6").unwrap(), ChartKind::Treemap);
        let d = c.data(&wb, 0);
        assert_eq!(d[0].cats[1], format!("Fruit{LEVEL_SEP}Pear"));
        assert_eq!(d[0].vals.len(), 5);
        assert_eq!(c.series[0].name_ref.as_deref(), Some("Sheet1!$C$1"));
    }

    #[test]
    fn stats() {
        let b = bins(&[1.0, 2.0, 2.0, 3.0, 10.0], Some(3.0), None);
        assert_eq!(b.iter().map(|x| x.2).sum::<usize>(), 5);
        assert_eq!(b[0].2, 4);
        let (lo, q1, med, q3, hi, mean, out) = box_stats(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 100.0], false).unwrap();
        assert_eq!((lo, med, hi), (1.0, 4.5, 7.0));
        assert!(q1 < q3 && mean > med);
        assert_eq!(out, vec![100.0]);
    }

    #[test]
    fn xlsx_round_trip_all_kinds() {
        let mut wb = book();
        let kinds = [ChartKind::Waterfall, ChartKind::Histogram, ChartKind::Pareto, ChartKind::BoxWhisker, ChartKind::Treemap, ChartKind::Sunburst, ChartKind::Funnel];
        for (i, k) in kinds.iter().enumerate() {
            let mut c = from_range(&wb, 0, parse_range("A1:C6").unwrap(), *k);
            if *k == ChartKind::Waterfall { c.ex.subtotals = vec![4]; }
            if *k == ChartKind::Histogram { c.ex.bin_count = Some(3); }
            add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Chart(Box::new(c)), Anchor::Abs { x: 0.0, y: i as f32 * 300.0, w: 400.0, h: 280.0 }), "Chart");
        }
        let bytes = crate::xlsx::write(&wb).unwrap();
        let back = crate::xlsx::read(&bytes).unwrap();
        let got: Vec<ChartKind> = back.sheets[0].objects.iter().filter_map(|o| o.chart().map(|c| c.kind)).collect();
        assert_eq!(got, kinds);
        let w = back.sheets[0].objects[0].chart().unwrap();
        assert_eq!(w.ex.subtotals, vec![4]);
        assert_eq!(back.sheets[0].objects[1].chart().unwrap().ex.bin_count, Some(3));
        let t = back.sheets[0].objects[4].chart().unwrap();
        assert_eq!(t.series[0].cat_cache[1], format!("Fruit{LEVEL_SEP}Pear"));
        assert_eq!(t.series[0].cat_ref.as_deref(), Some("Sheet1!$A$2:$B$6"));
        assert!(back.parts.keys().any(|k| k.starts_with("xl/charts/chartEx")));
        // Saved again unchanged, still there.
        let again = crate::xlsx::read(&crate::xlsx::write(&back).unwrap()).unwrap();
        assert_eq!(again.sheets[0].objects.iter().filter(|o| o.is_chart()).count(), 7);
    }
}
