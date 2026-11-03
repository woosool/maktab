//! Charts (charts.js, chart-*.js, xlsx-charts.js): a model of the common chart kinds — column
//! and bar (clustered, stacked, 100%), line, pie, doughnut, area, scatter and simple combos —
//! read from a DrawingML chart part (c:chartSpace) and written back. Series name their data by
//! reference (Sheet1!$B$2:$B$9); the values shown come from those cells, so a chart follows the
//! sheet, with the file's cached values as the fallback.

use crate::addr::{parse_range, quote_sheet, Pos, Range};
use crate::book::Workbook;
use crate::value::Value;
use crate::xlsx::xml::{attr, text, Element};

mod layout;
pub mod ex;
pub use layout::{layout, layout_tagged, tick_text, value_text as label_text, Measure, Prim};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChartKind { #[default] Column, Bar, Line, Pie, Doughnut, Area, Scatter, Radar, Stock, Bubble, Waterfall, Histogram, Pareto, BoxWhisker, Treemap, Sunburst, Funnel }

impl ChartKind {
    pub const ALL: [ChartKind; 10] = [ChartKind::Column, ChartKind::Bar, ChartKind::Line, ChartKind::Pie, ChartKind::Doughnut, ChartKind::Area, ChartKind::Scatter, ChartKind::Bubble, ChartKind::Stock, ChartKind::Radar];
    pub fn label(self) -> &'static str {
        match self { ChartKind::Column => "Column", ChartKind::Bar => "Bar", ChartKind::Line => "Line", ChartKind::Pie => "Pie", ChartKind::Doughnut => "Doughnut", ChartKind::Area => "Area", ChartKind::Scatter => "Scatter", ChartKind::Radar => "Radar", ChartKind::Stock => "Stock", ChartKind::Bubble => "Bubble", ChartKind::Waterfall => "Waterfall", ChartKind::Histogram => "Histogram", ChartKind::Pareto => "Pareto", ChartKind::BoxWhisker => "Box and Whisker", ChartKind::Treemap => "Treemap", ChartKind::Sunburst => "Sunburst", ChartKind::Funnel => "Funnel" }
    }
    pub fn icon(self) -> &'static str {
        match self { ChartKind::Column => "chartColumn", ChartKind::Bar => "chartBar", ChartKind::Line | ChartKind::Stock => "chartLine", ChartKind::Pie => "chartPie", ChartKind::Doughnut => "chartDoughnut", ChartKind::Area => "chartArea", ChartKind::Scatter | ChartKind::Bubble => "chartScatter", ChartKind::Radar => "chartRadar", ChartKind::Waterfall | ChartKind::Histogram | ChartKind::Pareto | ChartKind::BoxWhisker => "chartColumn", ChartKind::Treemap | ChartKind::Funnel => "chartBar", ChartKind::Sunburst => "chartDoughnut" }
    }
    pub fn is_round(self) -> bool { matches!(self, ChartKind::Pie | ChartKind::Doughnut) }
    pub fn has_grouping(self) -> bool { matches!(self, ChartKind::Column | ChartKind::Bar | ChartKind::Area | ChartKind::Line) }
    /// Series of this kind can go on a secondary axis.
    pub fn allows_secondary(self) -> bool { self.comboable() || self.is_xy() }
    /// Both axes are value axes (x values, not categories).
    pub fn is_xy(self) -> bool { matches!(self, ChartKind::Scatter | ChartKind::Bubble) }
    /// Kinds a combo chart's series may take (and so may go on the secondary axis).
    pub fn comboable(self) -> bool { matches!(self, ChartKind::Column | ChartKind::Bar | ChartKind::Line | ChartKind::Area) }
}

/// A radar chart's style (c:radarStyle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RadarStyle { Standard, #[default] Marker, Filled }

/// A marker's shape (c:symbol).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Symbol { None, #[default] Auto, Circle, Square, Diamond, Triangle, X, Star, Dash, Dot, Plus }

impl Symbol {
    pub const ALL: [Symbol; 11] = [Symbol::Auto, Symbol::None, Symbol::Circle, Symbol::Square, Symbol::Diamond, Symbol::Triangle, Symbol::X, Symbol::Star, Symbol::Dash, Symbol::Dot, Symbol::Plus];
    pub fn code(self) -> &'static str {
        match self { Symbol::None => "none", Symbol::Auto => "auto", Symbol::Circle => "circle", Symbol::Square => "square", Symbol::Diamond => "diamond", Symbol::Triangle => "triangle", Symbol::X => "x", Symbol::Star => "star", Symbol::Dash => "dash", Symbol::Dot => "dot", Symbol::Plus => "plus" }
    }
    pub fn parse(s: &str) -> Symbol { Symbol::ALL.into_iter().find(|k| k.code() == s).unwrap_or(Symbol::Auto) }
    /// What an automatic marker is for series `i` (Excel's order).
    pub fn auto(i: usize) -> Symbol { [Symbol::Diamond, Symbol::Square, Symbol::Triangle, Symbol::X, Symbol::Star, Symbol::Circle, Symbol::Plus][i % 7] }
}

/// A series' own marker (c:marker): its symbol and size in points (2-72).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Marker { pub symbol: Symbol, pub size: Option<u8>, pub fill: Option<[u8; 3]>, pub line: Option<[u8; 3]> }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Grouping { #[default] Clustered, Stacked, Percent }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LegendPos { #[default] Right, Left, Top, Bottom }

impl LegendPos {
    fn code(self) -> &'static str { match self { LegendPos::Right => "r", LegendPos::Left => "l", LegendPos::Top => "t", LegendPos::Bottom => "b" } }
    fn parse(s: &str) -> LegendPos { match s { "l" => LegendPos::Left, "t" => LegendPos::Top, "b" => LegendPos::Bottom, _ => LegendPos::Right } }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Series {
    /// The literal name, or the cached text of `name_ref`.
    pub name: String,
    pub name_ref: Option<String>,
    /// Categories (scatter: x values) and values (scatter: y values), by reference.
    pub cat_ref: Option<String>,
    pub val_ref: Option<String>,
    pub cat_cache: Vec<String>,
    pub val_cache: Vec<f64>,
    pub color: Option<[u8; 3]>,
    /// A combo chart's series drawn as another kind than the chart's.
    pub kind: Option<ChartKind>,
    /// Plotted against the secondary value axis (on the right).
    pub secondary: bool,
    /// Its own marker, overriding the chart's markers setting.
    pub marker: Option<Marker>,
    /// Bubble sizes, by reference, and their cache.
    pub size_ref: Option<String>,
    pub size_cache: Vec<f64>,
    /// The number format of the cached categories (dates), from c:numCache's formatCode.
    pub cat_format: Option<String>,
    /// The outline of filled kinds (bars, areas, bubbles); lines use `color`.
    pub line_color: Option<[u8; 3]>,
    /// Line width in points, and dash (a:prstDash: dash, sysDot, lgDash…; None solid).
    pub line_width: Option<f32>,
    pub dash: Option<String>,
    /// Smoothed line (c:smooth).
    pub smooth: bool,
    /// Its own data labels setting (c:ser/c:dLbls), overriding the chart's.
    pub labels: Option<bool>,
}

/// A time unit of a date axis (c:baseTimeUnit, c:majorTimeUnit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit { Days, Months, Years }

impl TimeUnit {
    pub fn code(self) -> &'static str { match self { TimeUnit::Days => "days", TimeUnit::Months => "months", TimeUnit::Years => "years" } }
    pub fn parse(s: &str) -> Option<TimeUnit> { match s { "days" => Some(TimeUnit::Days), "months" => Some(TimeUnit::Months), "years" => Some(TimeUnit::Years), _ => None } }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Chart {
    pub kind: ChartKind,
    pub grouping: Grouping,
    pub markers: bool,
    /// None: no title; Some(""): the automatic title (the series name).
    pub title: Option<String>,
    /// A title linked to a cell (c:tx/c:strRef): its reference; `title` holds the cached text.
    pub title_ref: Option<String>,
    pub legend: Option<LegendPos>,
    pub data_labels: bool,
    /// Data labels show percentages (pie and doughnut) instead of values.
    pub percent_labels: bool,
    pub gridlines: bool,
    pub x_title: Option<String>,
    pub y_title: Option<String>,
    /// The secondary value axis's title.
    pub y2_title: Option<String>,
    /// The category axis is a date axis (c:dateAx).
    pub date_axis: bool,
    /// The category axis's own number format (numFmt with sourceLinked="0").
    pub cat_axis_format: Option<String>,
    pub radar_style: RadarStyle,
    /// Stock charts: high-low lines and up/down bars (open to close).
    pub hi_low_lines: bool,
    pub up_down_bars: bool,
    /// Bubble charts: the largest bubble's size in percent of the default.
    pub bubble_scale: u32,
    /// Bubble charts: negative sizes shown (c:showNegBubbles), size as width not area (c:sizeRepresents).
    pub show_neg_bubbles: bool,
    pub size_width: bool,
    /// Date axis: base unit, major unit (and its unit), and fixed bounds (serial dates).
    pub base_unit: Option<TimeUnit>,
    pub major_unit: Option<f64>,
    pub major_time_unit: Option<TimeUnit>,
    pub axis_min: Option<f64>,
    pub axis_max: Option<f64>,
    pub series: Vec<Series>,
    /// The color scheme: 0 colorful (theme accents), 1-6 shades of one accent.
    pub palette: u8,
    /// Settings of the chartex kinds (waterfall totals, histogram bins, box plot options).
    pub ex: ex::ExOpts,
    /// Edited since it was read: writing regenerates the part instead of copying it.
    pub dirty: bool,
    /// The chart part it came from (kept while unchanged, with its style and color parts).
    pub part: Option<String>,
    /// A PivotChart: the PivotTable it shows (by name); pivot_chart.rs keeps its series in step.
    pub pivot_source: Option<String>,
}

/// What a series shows: its name, category labels and values.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SeriesData { pub name: String, pub cats: Vec<String>, pub vals: Vec<Option<f64>>, pub xs: Vec<f64>, pub sizes: Vec<Option<f64>> }

const NS_C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

// ---- references ------------------------------------------------------------------------------

/// A reference as (sheet name, range): "Sheet1!$A$1:$A$5", "'My data'!B2". Unions fail.
pub fn parse_ref(f: &str) -> Option<(Option<String>, Range)> {
    let f = f.trim().trim_start_matches('=');
    let f = if f.starts_with('(') && f.ends_with(')') && !f.contains(',') { &f[1..f.len() - 1] } else { f };
    if f.contains(',') { return None; }
    match f.rfind('!') {
        Some(i) => {
            let s = &f[..i];
            let s = if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 { s[1..s.len() - 1].replace("''", "'") } else { s.to_string() };
            Some((Some(s), parse_range(&f[i + 1..])?))
        }
        None => Some((None, parse_range(f)?)),
    }
}

/// A range as an absolute reference with its sheet.
pub fn make_ref(sheet: &str, r: Range) -> String {
    let a = |p: Pos| format!("${}${}", crate::addr::col_name(p.col), p.row + 1);
    if r.is_cell() { format!("{}!{}", quote_sheet(sheet), a(r.start())) } else { format!("{}!{}:{}", quote_sheet(sheet), a(r.start()), a(r.end())) }
}

/// The values a reference points at, row by row (at most 4000), or None if it doesn't resolve.
pub fn ref_values(wb: &Workbook, sheet: usize, f: &str) -> Option<Vec<Value>> {
    let (name, r) = parse_ref(f)?;
    let si = match name { Some(n) => wb.sheet_index(&n)?, None => sheet };
    let s = wb.sheets.get(si)?;
    let r = Range { r1: r.r1.min(r.r0 + 3999), c1: r.c1.min(r.c0 + 3999), ..r };
    let mut out = vec![];
    for p in r.iter() { if out.len() >= 4000 { break; } out.push(s.value(p)); }
    Some(out)
}

fn value_text(wb: &Workbook, sheet: usize, f: &str, v: &Value) -> String {
    let _ = (wb, sheet, f);
    match v { Value::Empty => String::new(), Value::Num(n) => crate::numfmt::format(&Value::Num(*n), "General", false).text, Value::Str(s) => s.to_string(), Value::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() }, Value::Err(e) => e.text().into(), Value::Array(_) => String::new() }
}

fn num_fmt(n: f64, f: &str) -> String { crate::numfmt::format(&Value::Num(n), f, false).text }

/// Cell text as shown (with its number format), for category labels.
fn shown_texts(wb: &Workbook, sheet: usize, f: &str) -> Option<Vec<String>> {
    let (name, r) = parse_ref(f)?;
    let si = match name { Some(n) => wb.sheet_index(&n)?, None => sheet };
    if si >= wb.sheets.len() { return None; }
    let r = Range { r1: r.r1.min(r.r0 + 3999), c1: r.c1.min(r.c0 + 3999), ..r };
    Some(r.iter().take(4000).map(|p| wb.display(si, p)).collect())
}

impl Chart {
    /// Whether this chart's series may use the secondary axis.
    pub fn allows_secondary(&self) -> bool { self.kind.allows_secondary() }

    /// The data to draw: from the referenced cells where they resolve, else the cached values.
    pub fn data(&self, wb: &Workbook, sheet: usize) -> Vec<SeriesData> {
        if self.kind.is_ex() { return ex::data(self, wb, sheet); }
        self.series.iter().map(|s| {
            let name = s.name_ref.as_deref().and_then(|f| ref_values(wb, sheet, f)).map(|v| v.iter().map(|x| value_text(wb, sheet, "", x)).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")).filter(|n| !n.is_empty()).unwrap_or_else(|| s.name.clone());
            let vals: Vec<Option<f64>> = match s.val_ref.as_deref().and_then(|f| ref_values(wb, sheet, f)) {
                Some(v) => v.iter().map(|x| match x { Value::Num(n) => Some(*n), Value::Bool(b) => Some(*b as u8 as f64), Value::Str(t) => t.trim().parse().ok(), _ => None }).collect(),
                None => s.val_cache.iter().map(|v| Some(*v)).collect(),
            };
            // Categories as the cells show them, or in the axis's own format; cached
            // numbers (dates) in the cache's format.
            let axis_fmt = self.cat_axis_format.as_deref().filter(|f| !f.eq_ignore_ascii_case("general"));
            let cat_vals = s.cat_ref.as_deref().and_then(|f| ref_values(wb, sheet, f));
            let cats = match (&cat_vals, axis_fmt) {
                (Some(v), Some(fm)) => v.iter().map(|x| match x { Value::Num(n) => num_fmt(*n, fm), x => value_text(wb, sheet, "", x) }).collect(),
                _ => s.cat_ref.as_deref().and_then(|f| shown_texts(wb, sheet, f)).unwrap_or_else(|| {
                    let fm = axis_fmt.or(s.cat_format.as_deref().filter(|f| !f.eq_ignore_ascii_case("general")));
                    s.cat_cache.iter().map(|c| match (fm, c.parse::<f64>()) { (Some(fm), Ok(n)) => num_fmt(n, fm), _ => c.clone() }).collect()
                }),
            };
            let xs = match &cat_vals {
                Some(v) => v.iter().enumerate().map(|(i, x)| if let Value::Num(n) = x { *n } else { (i + 1) as f64 }).collect(),
                None => s.cat_cache.iter().enumerate().map(|(i, c)| c.parse().unwrap_or((i + 1) as f64)).collect(),
            };
            let sizes: Vec<Option<f64>> = match s.size_ref.as_deref().and_then(|f| ref_values(wb, sheet, f)) {
                Some(v) => v.iter().map(|x| match x { Value::Num(n) => Some(*n), Value::Str(t) => t.trim().parse().ok(), _ => None }).collect(),
                None => s.size_cache.iter().map(|v| Some(*v)).collect(),
            };
            SeriesData { name, cats, vals, xs, sizes }
        }).collect()
    }

    /// The color of series (or pie slice) `i`: its own, else from the palette.
    pub fn color(&self, i: usize, theme: &[[u8; 3]]) -> [u8; 3] {
        if !self.kind.is_round() { if let Some(c) = self.series.get(i).and_then(|s| s.color) { return c; } }
        palette_color(self.palette, i, theme)
    }

    /// The range all the series' references cover on one sheet (for Switch Row/Column).
    pub fn source(&self) -> Option<(String, Range)> {
        let mut out: Option<(String, Range)> = None;
        for s in &self.series {
            for f in [&s.name_ref, &s.cat_ref, &s.val_ref].into_iter().flatten() {
                let (Some(sh), r) = parse_ref(f)? else { return None };
                match &mut out { None => out = Some((sh, r)), Some((n, u)) => { if *n != sh { return None; } *u = u.union(&r); } }
            }
        }
        out
    }

    /// A chart of `kind` over a range: a header row and label column are used when they look
    /// like ones; series run down columns unless `by_rows`.
    pub fn from_range(wb: &Workbook, sheet: usize, r: Range, kind: ChartKind, by_rows: Option<bool>) -> Chart {
        let s = &wb.sheets[sheet];
        let is_text = |p: Pos| matches!(s.value(p), Value::Str(_));
        let is_blank = |p: Pos| s.value(p).is_empty();
        // A first row of text (or a blank corner) is the series names; a first column of text the categories.
        let head_row = r.rows() > 1 && (r.c0..=r.c1).filter(|&c| c != r.c0 || r.cols() == 1).all(|c| is_text(Pos::new(r.r0, c)) || is_blank(Pos::new(r.r0, c))) && (r.c0..=r.c1).any(|c| is_text(Pos::new(r.r0, c)));
        let head_col = r.cols() > 1 && (r.r0..=r.r1).skip(head_row as usize).any(|row| is_text(Pos::new(row, r.c0))) || (r.cols() > 1 && head_row && is_blank(r.start()));
        let body = Range { r0: r.r0 + head_row as u32, c0: r.c0 + head_col as u32, ..r };
        let by_rows = by_rows.unwrap_or(body.cols() > body.rows() && !kind.is_round() && !kind.is_xy() && kind != ChartKind::Stock);
        let name = &s.name;
        let mut series = vec![];
        let scatter = kind.is_xy();
        if !by_rows {
            let cats = if head_col { Some(Range { c0: r.c0, c1: r.c0, ..body }) } else if scatter && body.cols() > 1 { None } else { None };
            let (first, xcol) = if scatter && !head_col && body.cols() > 1 { (body.c0 + 1, Some(body.c0)) } else { (body.c0, None) };
            for c in first..=body.c1 {
                let cat = cats.or(xcol.map(|x| Range { c0: x, c1: x, ..body }));
                series.push(Series {
                    name: if head_row { String::new() } else { format!("Series{}", series.len() + 1) },
                    name_ref: head_row.then(|| make_ref(name, Range::cell(Pos::new(r.r0, c)))),
                    cat_ref: cat.map(|x| make_ref(name, x)),
                    val_ref: Some(make_ref(name, Range { c0: c, c1: c, ..body })),
                    ..Default::default()
                });
            }
        } else {
            let cats = head_row.then(|| Range { r0: r.r0, r1: r.r0, ..body });
            for row in body.r0..=body.r1 {
                series.push(Series {
                    name: if head_col { String::new() } else { format!("Series{}", series.len() + 1) },
                    name_ref: head_col.then(|| make_ref(name, Range::cell(Pos::new(row, r.c0)))),
                    cat_ref: cats.map(|x| make_ref(name, x)),
                    val_ref: Some(make_ref(name, Range { r0: row, r1: row, ..body })),
                    ..Default::default()
                });
            }
        }
        if kind.is_round() { series.truncate(1); }
        // Bubbles: the columns pair up as (y values, sizes).
        if kind == ChartKind::Bubble && series.len() >= 2 {
            let mut it = std::mem::take(&mut series).into_iter();
            while let Some(mut y) = it.next() { if let Some(sz) = it.next() { y.size_ref = sz.val_ref; } series.push(y); }
        }
        // Date categories keep their format (written as numbers with it).
        if let Some(f) = series.first().and_then(|x| x.cat_ref.as_deref()).and_then(parse_ref).map(|(_, r)| r.start()) {
            let fmt = &wb.style_at(sheet, f).num_fmt;
            if matches!(s.value(f), Value::Num(_)) && crate::numfmt::is_date_format(fmt) && !scatter {
                for x in &mut series { x.cat_format = Some(fmt.clone()); }
            }
        }
        let mut c = Chart { kind, markers: kind == ChartKind::Line || kind == ChartKind::Scatter, hi_low_lines: kind == ChartKind::Stock, up_down_bars: kind == ChartKind::Stock && series.len() >= 4, date_axis: series.first().is_some_and(|x| x.cat_format.is_some()), title: Some(String::new()), legend: Some(if kind.is_round() || series.len() > 1 { LegendPos::Bottom } else { LegendPos::Bottom }), gridlines: !kind.is_round(), series, dirty: true, ..Default::default() };
        if c.series.len() == 1 && !kind.is_round() { c.legend = None; }
        // Cache what the references show now, so the part is complete without the workbook.
        c.refresh_cache(wb, sheet);
        c
    }

    /// Whether the series run along rows of their source (for Switch Row/Column).
    pub fn by_rows(&self) -> bool {
        self.series.first().and_then(|s| s.val_ref.as_deref()).and_then(parse_ref).is_some_and(|(_, r)| r.rows() == 1 && r.cols() > 1)
    }

    /// Copies the current cell values into the series' caches.
    pub fn refresh_cache(&mut self, wb: &Workbook, sheet: usize) {
        let data = self.data(wb, sheet);
        if let Some(v) = self.title_ref.as_deref().and_then(|f| ref_values(wb, sheet, f)) { self.title = Some(v.first().map(|v| value_text(wb, sheet, "", v)).unwrap_or_default()); }
        for (s, d) in self.series.iter_mut().zip(data) {
            if s.name_ref.is_some() { s.name = d.name; }
            // Dates cache their serial numbers (shown with `cat_format`).
            if s.cat_ref.is_some() { s.cat_cache = if s.cat_format.is_some() { d.xs.iter().map(|v| num_text(*v)).collect() } else { d.cats }; }
            if s.val_ref.is_some() { s.val_cache = d.vals.iter().map(|v| v.unwrap_or(0.0)).collect(); }
            if s.size_ref.is_some() { s.size_cache = d.sizes.iter().map(|v| v.unwrap_or(0.0)).collect(); }
        }
    }

    /// The title shown: its text, or for the automatic title the series name (single series).
    pub fn title_text(&self, data: &[SeriesData]) -> Option<String> {
        match &self.title {
            None => None,
            Some(t) if !t.is_empty() => Some(t.clone()),
            Some(_) => Some(if data.len() == 1 && !data[0].name.is_empty() { data[0].name.clone() } else { "Chart Title".into() }),
        }
    }
}

/// The palette: colorful uses the six accents, monochrome schemes shade one accent.
pub fn palette_color(palette: u8, i: usize, theme: &[[u8; 3]]) -> [u8; 3] {
    let accent = |k: usize| theme.get(4 + k).copied().unwrap_or(crate::numfmt::OFFICE_THEME[4 + k]);
    if palette == 0 || palette > 6 {
        let base = accent(i % 6);
        // Past six series Excel repeats the accents darker, then lighter.
        return match (i / 6) % 3 { 0 => base, 1 => shade(base, 0.6), _ => tint(base, 0.4) };
    }
    let base = accent(palette as usize - 1);
    let steps = [0.0, 0.45, -0.3, 0.7, -0.5, 0.25];
    let t = steps[i % steps.len()];
    if t >= 0.0 { tint(base, t as f32) } else { shade(base, 1.0 + t as f32) }
}
fn shade(c: [u8; 3], k: f32) -> [u8; 3] { c.map(|x| (x as f32 * k).round().clamp(0.0, 255.0) as u8) }
fn tint(c: [u8; 3], k: f32) -> [u8; 3] { c.map(|x| (x as f32 + (255.0 - x as f32) * k).round().clamp(0.0, 255.0) as u8) }

/// Nice axis bounds and step for values from `lo` to `hi` (about `target` steps).
pub fn nice_scale(lo: f64, hi: f64, target: usize) -> (f64, f64, f64) {
    let (mut lo, mut hi) = (lo.min(hi), lo.max(hi));
    if !lo.is_finite() || !hi.is_finite() { return (0.0, 1.0, 0.2); }
    if lo > 0.0 && lo < hi * 0.8 { lo = 0.0; }
    if hi < 0.0 && hi > lo * 0.8 { hi = 0.0; }
    if (hi - lo).abs() < 1e-12 { if hi == 0.0 { hi = 1.0; } else if hi > 0.0 { lo = 0.0; } else { hi = 0.0; } }
    let raw = (hi - lo) / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    ((lo / step).floor() * step, (hi / step).ceil() * step, step)
}

// ---- reading -----------------------------------------------------------------------------------

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }
fn val<'a>(e: &'a Element, l: &str) -> Option<&'a str> { e.child(l).and_then(|c| c.attr("val")) }
fn on(e: &Element, l: &str) -> Option<bool> { e.child(l).map(|c| c.attr("val").is_none_or(|v| v == "1" || v == "true")) }

fn rich(e: &Element) -> String {
    // A title's text: rich runs by paragraph, or a reference's cached string.
    if let Some(r) = find(e, "rich") {
        let paras: Vec<String> = r.elements().filter(|p| p.local() == "p").map(|p| { let mut s = String::new(); collect_t(p, &mut s); s }).collect();
        return paras.join("\n");
    }
    if let Some(c) = find(e, "strCache") { return c.all("pt").filter_map(|p| p.child("v")).map(|v| v.text()).collect::<Vec<_>>().join(" "); }
    String::new()
}
fn collect_t(e: &Element, out: &mut String) { for c in e.elements() { if c.local() == "t" { out.push_str(&c.text()); } else { collect_t(c, out); } } }

fn color_of(sp: &Element, theme: &[[u8; 3]]) -> Option<[u8; 3]> {
    // The fill's color, or for a line the line's.
    let fill = sp.child("solidFill").or_else(|| sp.child("ln").and_then(|l| l.child("solidFill")))?;
    if let Some(c) = fill.child("srgbClr").and_then(|c| c.attr("val")) { return hex(c); }
    let s = fill.child("schemeClr")?;
    let k = match s.attr("val")? { "accent1" => 4, "accent2" => 5, "accent3" => 6, "accent4" => 7, "accent5" => 8, "accent6" => 9, "tx1" | "dk1" => 0, "bg1" | "lt1" => 1, "tx2" | "dk2" => 2, "bg2" | "lt2" => 3, _ => return None };
    let mut c = theme.get(k).copied()?;
    for m in s.elements() {
        let v = m.attr_f64("val").unwrap_or(100000.0) / 100000.0;
        match m.local() { "lumMod" => c = shade(c, v as f32), "lumOff" => c = c.map(|x| (x as f64 + 255.0 * v).min(255.0) as u8), "shade" => c = shade(c, v as f32), "tint" => c = tint(c, 1.0 - v as f32), _ => {} }
    }
    Some(c)
}
fn hex(s: &str) -> Option<[u8; 3]> {
    if s.len() != 6 { return None; }
    let p = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([p(0)?, p(2)?, p(4)?])
}

fn data_ref(e: &Element) -> (Option<String>, Vec<String>) {
    // strRef / numRef / multiLvlStrRef: the formula and the cached points; or literals.
    for k in ["numRef", "strRef", "multiLvlStrRef"] {
        if let Some(r) = e.child(k) {
            let f = r.child("f").map(|f| f.text()).filter(|f| !f.trim().is_empty());
            let cache = r.elements().find(|c| matches!(c.local(), "numCache" | "strCache" | "multiLvlStrCache"));
            let pts = cache.map(points).unwrap_or_default();
            return (f, pts);
        }
    }
    for k in ["numLit", "strLit"] { if let Some(l) = e.child(k) { return (None, points(l)); } }
    (None, vec![])
}
fn points(cache: &Element) -> Vec<String> {
    let cache = if cache.local() == "multiLvlStrCache" { cache.child("lvl").unwrap_or(cache) } else { cache };
    let n = val(cache, "ptCount").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let mut out = vec![String::new(); n.min(100_000)];
    for p in cache.all("pt") {
        let i = p.attr_u32("idx").unwrap_or(0) as usize;
        if i >= out.len() { if i > 100_000 { continue; } out.resize(i + 1, String::new()); }
        out[i] = p.child("v").map(|v| v.text()).unwrap_or_default();
    }
    out
}

/// Reads a chart part. Kinds the model doesn't draw map to the nearest one it does.
pub fn parse(xml: &str, theme: &[[u8; 3]]) -> Option<Chart> {
    let root = Element::parse(xml).ok()?;
    let chart = root.child("chart")?;
    let plot = chart.child("plotArea")?;
    let mut c = Chart { legend: None, ..Default::default() };
    let auto_deleted = on(chart, "autoTitleDeleted");
    c.pivot_source = root.child("pivotSource").and_then(|p| p.child("name")).map(|n| { let t = n.text(); t.rsplit('!').next().unwrap_or(&t).to_string() });
    c.title = chart.child("title").map(rich);
    c.title_ref = chart.child("title").and_then(|t| t.child("tx")).and_then(|t| t.child("strRef")).and_then(|r| r.child("f")).map(|f| f.text());
    if let Some(l) = chart.child("legend") { c.legend = Some(LegendPos::parse(val(l, "legendPos").unwrap_or("r"))); }
    let mut first = true;
    let mut has_markers = None;
    // The first group's axes are the primary pair; a group on other value axes is secondary.
    let group_axes = |g: &Element| -> Vec<String> { g.all("axId").filter_map(|a| a.attr("val")).map(str::to_string).collect() };
    let mut primary_axes: Option<Vec<String>> = None;
    for g in plot.elements() {
        let (kind, grouping) = match g.local() {
            "barChart" | "bar3DChart" => (if val(g, "barDir") == Some("bar") { ChartKind::Bar } else { ChartKind::Column }, val(g, "grouping")),
            "lineChart" | "line3DChart" => (ChartKind::Line, val(g, "grouping")),
            // A stock group after another (a volume combo) draws as lines.
            "stockChart" => (if first { ChartKind::Stock } else { ChartKind::Line }, None),
            "radarChart" => (if first { ChartKind::Radar } else { ChartKind::Line }, None),
            "pieChart" | "pie3DChart" | "ofPieChart" => (ChartKind::Pie, None),
            "doughnutChart" => (ChartKind::Doughnut, None),
            "areaChart" | "area3DChart" | "surfaceChart" | "surface3DChart" => (ChartKind::Area, val(g, "grouping")),
            "scatterChart" => (ChartKind::Scatter, None),
            "bubbleChart" => (if first { ChartKind::Bubble } else { ChartKind::Scatter }, None),
            _ => continue,
        };
        let axes = group_axes(g);
        let secondary = match &primary_axes {
            None => { if !axes.is_empty() { primary_axes = Some(axes.clone()); } false }
            Some(p) => !axes.is_empty() && axes.get(1) != p.get(1) && (kind.comboable() || kind.is_xy()),
        };
        if first {
            c.kind = kind;
            c.grouping = match grouping { Some("stacked") => Grouping::Stacked, Some("percentStacked") => Grouping::Percent, _ => Grouping::Clustered };
            if let Some(d) = g.child("dLbls") { let (v, p) = (on(d, "showVal").unwrap_or(false), on(d, "showPercent").unwrap_or(false)); c.data_labels |= v || p; c.percent_labels |= p && !v; }
            match kind {
                ChartKind::Radar => c.radar_style = match val(g, "radarStyle") { Some("filled") => RadarStyle::Filled, Some("standard") => RadarStyle::Standard, _ => RadarStyle::Marker },
                ChartKind::Stock => { c.hi_low_lines = g.child("hiLowLines").is_some(); c.up_down_bars = g.child("upDownBars").is_some(); }
                ChartKind::Bubble => {
                    c.bubble_scale = val(g, "bubbleScale").and_then(|v| v.parse().ok()).unwrap_or(0);
                    c.show_neg_bubbles = on(g, "showNegBubbles").unwrap_or(false);
                    c.size_width = val(g, "sizeRepresents") == Some("w");
                }
                _ => {}
            }
        }
        for s in g.all("ser") {
            let mut se = Series { kind: (!first && kind != c.kind).then_some(kind), secondary, ..Default::default() };
            if let Some(tx) = s.child("tx") {
                if let Some(r) = tx.child("strRef") { se.name_ref = r.child("f").map(|f| f.text()); se.name = rich(tx); } else { se.name = tx.child("v").map(|v| v.text()).unwrap_or_default(); }
            }
            let (cat, val_) = if kind.is_xy() { (s.child("xVal"), s.child("yVal")) } else { (s.child("cat"), s.child("val")) };
            if let Some(e) = cat {
                let (f, pts) = data_ref(e); se.cat_ref = f; se.cat_cache = pts;
                se.cat_format = e.child("numRef").and_then(|r| r.child("numCache")).and_then(|nc| nc.child("formatCode")).map(|f| f.text()).filter(|f| !f.trim().is_empty() && !f.eq_ignore_ascii_case("general"));
            }
            if let Some(e) = val_ { let (f, pts) = data_ref(e); se.val_ref = f; se.val_cache = pts.iter().map(|p| p.parse().unwrap_or(0.0)).collect(); }
            if let Some(e) = s.child("bubbleSize") { let (f, pts) = data_ref(e); se.size_ref = f; se.size_cache = pts.iter().map(|p| p.parse().unwrap_or(0.0)).collect(); }
            if let Some(sp) = s.child("spPr") {
                se.color = color_of(sp, theme);
                if let Some(ln) = sp.child("ln") {
                    se.line_width = ln.attr_f64("w").map(|w| (w / 12700.0 * 100.0).round() as f32 / 100.0);
                    se.dash = ln.child("prstDash").and_then(|d| d.attr("val")).filter(|d| *d != "solid").map(str::to_string);
                    // A filled shape's outline color.
                    if sp.child("solidFill").is_some() { se.line_color = ln.child("solidFill").and_then(|_| color_of(ln, theme)); }
                }
            }
            se.smooth = on(s, "smooth").unwrap_or(false);
            if let Some(m) = s.child("marker") {
                if first && matches!(kind, ChartKind::Line | ChartKind::Scatter | ChartKind::Radar | ChartKind::Stock) && has_markers.is_none() { has_markers = Some(val(m, "symbol") != Some("none")); }
                if val(m, "symbol").is_some() || val(m, "size").is_some() {
                    let sp = m.child("spPr");
                    se.marker = Some(Marker {
                        symbol: val(m, "symbol").map(Symbol::parse).unwrap_or_default(), size: val(m, "size").and_then(|v| v.parse::<u8>().ok()),
                        fill: sp.filter(|sp| sp.child("solidFill").is_some()).and_then(|sp| color_of(sp, theme)),
                        line: sp.and_then(|sp| sp.child("ln")).filter(|ln| ln.child("solidFill").is_some()).and_then(|ln| color_of(ln, theme)),
                    });
                }
            }
            if let Some(d) = s.child("dLbls") {
                let (v, p) = (on(d, "showVal").unwrap_or(false), on(d, "showPercent").unwrap_or(false));
                se.labels = Some(v || p);
                if first { c.percent_labels |= p && !v; }
            }
            c.series.push(se);
        }
        if first && matches!(kind, ChartKind::Line) { if let Some(m) = on(g, "marker") { has_markers = Some(has_markers.unwrap_or(m)); } }
        if first && kind == ChartKind::Scatter { if let Some(st) = val(g, "scatterStyle") { has_markers = Some(has_markers.unwrap_or(st.contains("arker") || st == "lineMarker")); } }
        first = false;
    }
    c.markers = has_markers.unwrap_or(matches!(c.kind, ChartKind::Line | ChartKind::Scatter) || (c.kind == ChartKind::Radar && c.radar_style == RadarStyle::Marker));
    // "No marker" on a series of a chart without markers says nothing of its own.
    if !c.markers { for s in &mut c.series { if s.marker == Some(Marker { symbol: Symbol::None, size: None, ..Default::default() }) { s.marker = None; } } }
    // Axes: titles and gridlines (the value axis's).
    // An axis's role comes from the first chart group's axis ids (category or x first, then
    // value), not from axPos: writers such as openpyxl put axPos="l" on every axis. A secondary
    // group's value axis titles the secondary axis.
    let ids: Vec<&str> = plot.elements().find(|g| g.local().ends_with("Chart")).map(|g| g.all("axId").filter_map(|a| a.attr("val")).collect()).unwrap_or_default();
    let has_secondary = c.series.iter().any(|s| s.secondary);
    for ax in plot.elements().filter(|e| matches!(e.local(), "catAx" | "valAx" | "dateAx" | "serAx")) {
        let id = val(ax, "axId");
        let is_x = match ids.iter().position(|i| Some(*i) == id) {
            Some(0) => true,
            Some(1) => false,
            _ => match ax.local() { "serAx" => continue, "catAx" | "dateAx" => true, _ => if c.kind.is_xy() { matches!(val(ax, "axPos"), Some("b" | "t")) } else { false } },
        };
        let primary = id.is_some_and(|i| ids.first() == Some(&i) || ids.get(1) == Some(&i)) || ids.is_empty();
        if !is_x && primary && ax.child("majorGridlines").is_some() { c.gridlines = true; }
        if is_x && primary {
            if ax.local() == "dateAx" {
                c.date_axis = true;
                c.base_unit = val(ax, "baseTimeUnit").and_then(TimeUnit::parse);
                c.major_time_unit = val(ax, "majorTimeUnit").and_then(TimeUnit::parse);
                c.major_unit = val(ax, "majorUnit").and_then(|v| v.parse().ok());
                if let Some(sc) = ax.child("scaling") { c.axis_min = val(sc, "min").and_then(|v| v.parse().ok()); c.axis_max = val(sc, "max").and_then(|v| v.parse().ok()); }
            }
            if let Some(nf) = ax.child("numFmt") {
                if nf.attr("sourceLinked").is_some_and(|v| v == "0" || v == "false") { c.cat_axis_format = nf.attr("formatCode").map(str::to_string).filter(|f| !f.eq_ignore_ascii_case("general") && !f.is_empty()); }
            }
        }
        if let Some(t) = ax.child("title").map(rich).filter(|t| !t.is_empty()) {
            if !is_x && !primary && has_secondary { c.y2_title = Some(t); continue; }
            let slot = if is_x { &mut c.x_title } else { &mut c.y_title };
            if primary || slot.is_none() { *slot = Some(t); }
        }
    }
    // Without a title element, Excel titles a single named series with its name unless the
    // automatic title is deleted.
    if c.title.is_none() && auto_deleted != Some(true) && c.series.len() == 1 && (c.series[0].name_ref.is_some() || !c.series[0].name.is_empty()) { c.title = Some(String::new()); }
    if c.title.as_deref() == Some("") && auto_deleted == Some(true) { c.title = None; }
    Some(c)
}

// ---- writing -------------------------------------------------------------------------------------

fn fill_xml(c: [u8; 3], line: bool) -> String {
    let h = format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
    if line { format!("<c:spPr><a:ln w=\"28575\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill><a:round/></a:ln></c:spPr>") }
    else { format!("<c:spPr><a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill></c:spPr>") }
}

fn rich_xml(t: &str, size: u32, bold: bool) -> String {
    let paras: String = t.split('\n').map(|l| format!("<a:p><a:pPr><a:defRPr sz=\"{size}\" b=\"{}\"/></a:pPr><a:r><a:rPr lang=\"en-US\" sz=\"{size}\" b=\"{}\"/><a:t>{}</a:t></a:r></a:p>", bold as u8, bold as u8, text(l))).collect();
    format!("<c:tx><c:rich><a:bodyPr/><a:lstStyle/>{paras}</c:rich></c:tx>")
}

fn title_xml(t: &str) -> String {
    if t.is_empty() { "<c:title><c:overlay val=\"0\"/></c:title>".into() } else { format!("<c:title>{}<c:overlay val=\"0\"/></c:title>", rich_xml(t, 1400, false)) }
}

fn str_data(tag: &str, f: Option<&str>, pts: &[String], numeric: bool) -> String { str_data_fmt(tag, f, pts, numeric, "General") }

fn str_data_fmt(tag: &str, f: Option<&str>, pts: &[String], numeric: bool, fmt: &str) -> String {
    let mut o = format!("<c:{tag}>");
    let cache = |o: &mut String, kind: &str| {
        o.push_str(&format!("<c:{kind}>"));
        if kind.starts_with("num") { o.push_str(&format!("<c:formatCode>{}</c:formatCode>", text(fmt))); }
        o.push_str(&format!("<c:ptCount val=\"{}\"/>", pts.len()));
        for (i, p) in pts.iter().enumerate() { if !p.is_empty() { o.push_str(&format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", text(p))); } }
        o.push_str(&format!("</c:{kind}>"));
    };
    match f {
        Some(f) => {
            let (r, c) = if numeric { ("numRef", "numCache") } else { ("strRef", "strCache") };
            o.push_str(&format!("<c:{r}><c:f>{}</c:f>", text(f)));
            cache(&mut o, c);
            o.push_str(&format!("</c:{r}>"));
        }
        None => cache(&mut o, if numeric { "numLit" } else { "strLit" }),
    }
    o.push_str(&format!("</c:{tag}>"));
    o
}

fn num_text(v: f64) -> String { if v == v.trunc() && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") } }

fn dlbls_xml(show: bool, pct: bool) -> String {
    format!("<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"{}\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"{}\"/><c:showBubbleSize val=\"0\"/></c:dLbls>", (show && !pct) as u8, (show && pct) as u8)
}

fn hex6(c: [u8; 3]) -> String { format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }

fn marker_xml(m: Marker) -> String {
    let size = m.size.map(|s| format!("<c:size val=\"{}\"/>", s.clamp(2, 72))).unwrap_or_default();
    let fill = m.fill.map(|c| format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hex6(c))).unwrap_or_default();
    let line = m.line.map(|c| format!("<a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>", hex6(c))).unwrap_or_default();
    let sp = if fill.is_empty() && line.is_empty() || m.symbol == Symbol::None { String::new() } else { format!("<c:spPr>{fill}{line}</c:spPr>") };
    format!("<c:marker><c:symbol val=\"{}\"/>{size}{sp}</c:marker>", m.symbol.code())
}

/// A series' shape properties: its line (line kinds) or fill and outline.
fn series_sppr(s: &Series, color: [u8; 3], line: bool) -> String {
    let w = s.line_width.map(|w| format!(" w=\"{}\"", (w * 12700.0).round() as i64)).unwrap_or_default();
    let dash = s.dash.as_ref().map(|d| format!("<a:prstDash val=\"{}\"/>", attr(d))).unwrap_or_default();
    if line { return format!("<c:spPr><a:ln{w} cap=\"rnd\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>{dash}<a:round/></a:ln></c:spPr>", hex6(color)); }
    let ln = if s.line_color.is_some() || !w.is_empty() || !dash.is_empty() {
        format!("<a:ln{w}>{}{dash}</a:ln>", s.line_color.map(|c| format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hex6(c))).unwrap_or_default())
    } else { String::new() };
    format!("<c:spPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>{ln}</c:spPr>", hex6(color))
}

impl Chart {
    /// The chart part (c:chartSpace), with caches from the current data.
    pub fn to_xml(&self, wb: &Workbook, sheet: usize) -> String {
        if self.kind.is_ex() { return ex::to_xml(self, wb, sheet); }
        let data = self.data(wb, sheet);
        let theme = &wb.theme_colors;
        let mut o = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<c:chartSpace xmlns:c=\"{NS_C}\" xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\"><c:roundedCorners val=\"0\"/>");
        if let Some(pv) = &self.pivot_source { o.push_str(&crate::pivot_chart::source_xml(wb, pv)); }
        o.push_str("<c:chart>");
        if let Some(f) = &self.title_ref {
            // A title linked to a cell: its reference and the cell's current text.
            let t = ref_values(wb, sheet, f).and_then(|v| v.first().map(|v| value_text(wb, sheet, "", v))).or_else(|| self.title.clone()).unwrap_or_default();
            o.push_str(&format!("<c:title><c:tx><c:strRef><c:f>{}</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{}</c:v></c:pt></c:strCache></c:strRef></c:tx><c:overlay val=\"0\"/></c:title><c:autoTitleDeleted val=\"0\"/>", text(f), text(&t)));
        } else { match &self.title { Some(t) => { o.push_str(&title_xml(t)); o.push_str("<c:autoTitleDeleted val=\"0\"/>"); } None => o.push_str("<c:autoTitleDeleted val=\"1\"/>") } }
        if self.pivot_source.is_some() { o.push_str("<c:pivotFmts><c:pivotFmt><c:idx val=\"0\"/></c:pivotFmt></c:pivotFmts>"); }
        o.push_str("<c:plotArea><c:layout/>");
        // Groups: the chart's kind, then each other kind its series use (combo), on the primary
        // axes and then the secondary ones.
        let combo = self.kind.comboable();
        let sec_of = |s: &Series| self.allows_secondary() && s.secondary;
        let mut groups: Vec<(ChartKind, bool)> = vec![(self.kind, false)];
        for s in &self.series { let g = (if combo { s.kind.unwrap_or(self.kind) } else { self.kind }, sec_of(s)); if !groups.contains(&g) { groups.push(g); } }
        groups.sort_by_key(|g| g.1);
        let has_secondary = groups.iter().any(|g| g.1);
        let (ax1, ax2, ax3, ax4) = (500000001u32, 500000002u32, 500000003u32, 500000004u32);
        let mut idx = 0;
        for (k, sec) in &groups {
            let members: Vec<usize> = (0..self.series.len()).filter(|&i| { let s = &self.series[i]; (if combo { s.kind.unwrap_or(self.kind) } else { self.kind }) == *k && sec_of(s) == *sec }).collect();
            if members.is_empty() { continue; }
            let grouping = match (k, self.grouping) {
                (ChartKind::Column | ChartKind::Bar, Grouping::Clustered) => "clustered",
                (_, Grouping::Stacked) => "stacked",
                (_, Grouping::Percent) => "percentStacked",
                _ => "standard",
            };
            let grouping = if *k == self.kind { grouping } else if matches!(k, ChartKind::Column | ChartKind::Bar) { "clustered" } else { "standard" };
            let mut ser = String::new();
            for &i in &members {
                let s = &self.series[i];
                let d = &data[i];
                let color = self.color(i, theme);
                ser.push_str(&format!("<c:ser><c:idx val=\"{idx}\"/><c:order val=\"{idx}\"/>"));
                idx += 1;
                match &s.name_ref {
                    Some(f) => ser.push_str(&format!("<c:tx><c:strRef><c:f>{}</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>{}</c:v></c:pt></c:strCache></c:strRef></c:tx>", text(f), text(&d.name))),
                    None => if !d.name.is_empty() { ser.push_str(&format!("<c:tx><c:v>{}</c:v></c:tx>", text(&d.name))); },
                }
                let line = matches!(k, ChartKind::Line | ChartKind::Scatter) || (*k == ChartKind::Radar && self.radar_style != RadarStyle::Filled);
                if *k == ChartKind::Stock { ser.push_str("<c:spPr><a:ln><a:noFill/></a:ln></c:spPr>"); }
                else if !k.is_round() { ser.push_str(&series_sppr(s, color, line)); }
                match k {
                    ChartKind::Column | ChartKind::Bar | ChartKind::Bubble => ser.push_str("<c:invertIfNegative val=\"0\"/>"),
                    ChartKind::Line | ChartKind::Scatter | ChartKind::Radar | ChartKind::Stock => match s.marker {
                        Some(m) => ser.push_str(&marker_xml(m)),
                        None => if !self.markers || *k == ChartKind::Stock { ser.push_str("<c:marker><c:symbol val=\"none\"/></c:marker>"); },
                    },
                    ChartKind::Pie | ChartKind::Doughnut => {
                        for p in 0..d.vals.len().max(d.cats.len()) {
                            ser.push_str(&format!("<c:dPt><c:idx val=\"{p}\"/><c:bubble3D val=\"0\"/>{}</c:dPt>", fill_xml(palette_color(self.palette, p, theme), false)));
                        }
                    }
                    _ => {}
                }
                if let Some(l) = s.labels { ser.push_str(&dlbls_xml(l, self.percent_labels && k.is_round())); }
                let vals: Vec<String> = d.vals.iter().map(|v| v.map(num_text).unwrap_or_default()).collect();
                if k.is_xy() {
                    let xs: Vec<String> = d.xs.iter().map(|v| num_text(*v)).collect();
                    let numeric_x = s.cat_ref.is_none() || d.cats.iter().zip(&xs).all(|(a, _)| a.is_empty() || a.parse::<f64>().is_ok());
                    if s.cat_ref.is_some() || !xs.is_empty() { ser.push_str(&str_data("xVal", s.cat_ref.as_deref(), if numeric_x { &xs } else { &d.cats }, numeric_x)); }
                    ser.push_str(&str_data("yVal", s.val_ref.as_deref(), &vals, true));
                    if *k == ChartKind::Bubble {
                        let sizes: Vec<String> = d.sizes.iter().map(|v| v.map(num_text).unwrap_or_default()).collect();
                        ser.push_str(&str_data("bubbleSize", s.size_ref.as_deref(), &sizes, true));
                        ser.push_str("<c:bubble3D val=\"0\"/>");
                    } else { ser.push_str(&format!("<c:smooth val=\"{}\"/>", s.smooth as u8)); }
                } else {
                    if let Some(fmt) = &s.cat_format {
                        // Dates: the serial numbers, with their format.
                        let xs: Vec<String> = d.xs.iter().map(|v| num_text(*v)).collect();
                        ser.push_str(&str_data_fmt("cat", s.cat_ref.as_deref(), &xs, true, fmt));
                    } else if s.cat_ref.is_some() || !d.cats.is_empty() { ser.push_str(&str_data("cat", s.cat_ref.as_deref(), &d.cats, false)); }
                    ser.push_str(&str_data("val", s.val_ref.as_deref(), &vals, true));
                    if matches!(k, ChartKind::Line | ChartKind::Stock) { ser.push_str(&format!("<c:smooth val=\"{}\"/>", (s.smooth && *k == ChartKind::Line) as u8)); }
                }
                ser.push_str("</c:ser>");
            }
            let dl = if self.data_labels { dlbls_xml(true, self.percent_labels && k.is_round()) } else { String::new() };
            let axes = if *sec { format!("<c:axId val=\"{ax3}\"/><c:axId val=\"{ax4}\"/>") } else { format!("<c:axId val=\"{ax1}\"/><c:axId val=\"{ax2}\"/>") };
            o.push_str(&match k {
                ChartKind::Column | ChartKind::Bar => format!("<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{ser}{dl}<c:gapWidth val=\"150\"/>{}{axes}</c:barChart>", if *k == ChartKind::Bar { "bar" } else { "col" }, if grouping != "clustered" { "<c:overlap val=\"100\"/>" } else { "" }),
                ChartKind::Line => format!("<c:lineChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{ser}{dl}<c:marker val=\"1\"/>{axes}</c:lineChart>"),
                ChartKind::Area => format!("<c:areaChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{ser}{dl}{axes}</c:areaChart>"),
                ChartKind::Pie => format!("<c:pieChart><c:varyColors val=\"1\"/>{ser}{dl}<c:firstSliceAng val=\"0\"/></c:pieChart>"),
                ChartKind::Doughnut => format!("<c:doughnutChart><c:varyColors val=\"1\"/>{ser}{dl}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"50\"/></c:doughnutChart>"),
                ChartKind::Scatter => format!("<c:scatterChart><c:scatterStyle val=\"{}\"/><c:varyColors val=\"0\"/>{ser}{dl}{axes}</c:scatterChart>", if self.markers { "lineMarker" } else { "line" }),
                ChartKind::Radar => format!("<c:radarChart><c:radarStyle val=\"{}\"/><c:varyColors val=\"0\"/>{ser}{dl}{axes}</c:radarChart>", match self.radar_style { RadarStyle::Standard => "standard", RadarStyle::Marker => "marker", RadarStyle::Filled => "filled" }),
                ChartKind::Stock => format!("<c:stockChart>{ser}{dl}{}{}{axes}</c:stockChart>",
                    if self.hi_low_lines { "<c:hiLowLines><c:spPr><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"404040\"/></a:solidFill></a:ln></c:spPr></c:hiLowLines>" } else { "" },
                    if self.up_down_bars { "<c:upDownBars><c:gapWidth val=\"150\"/><c:upBars><c:spPr><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"404040\"/></a:solidFill></a:ln></c:spPr></c:upBars><c:downBars><c:spPr><a:solidFill><a:srgbClr val=\"404040\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"404040\"/></a:solidFill></a:ln></c:spPr></c:downBars></c:upDownBars>" } else { "" }),
                ChartKind::Bubble => format!("<c:bubbleChart><c:varyColors val=\"0\"/>{ser}{dl}<c:bubble3D val=\"0\"/>{}<c:showNegBubbles val=\"{}\"/>{}{axes}</c:bubbleChart>", if self.bubble_scale > 0 { format!("<c:bubbleScale val=\"{}\"/>", self.bubble_scale.min(300)) } else { String::new() }, self.show_neg_bubbles as u8, if self.size_width { "<c:sizeRepresents val=\"w\"/>" } else { "" }),
                _ => String::new(),
            });
        }
        if !self.kind.is_round() {
            let grid = if self.gridlines { "<c:majorGridlines/>" } else { "" };
            let t = |t: &Option<String>| t.as_ref().map(|t| format!("<c:title>{}<c:overlay val=\"0\"/></c:title>", rich_xml(t, 1000, true))).unwrap_or_default();
            let (cat_pos, val_pos) = if self.kind == ChartKind::Bar { ("l", "b") } else { ("b", "l") };
            let common = "<c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>";
            let line = "<c:spPr><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"D9D9D9\"/></a:solidFill></a:ln></c:spPr>";
            let cat_fmt = match &self.cat_axis_format { Some(f) => format!("<c:numFmt formatCode=\"{}\" sourceLinked=\"0\"/>", attr(f)), None => "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>".into() };
            if self.kind.is_xy() {
                o.push_str(&format!("<c:valAx><c:axId val=\"{ax1}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>{common}{line}<c:crossAx val=\"{ax2}\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>", t(&self.x_title)));
            } else if self.date_axis {
                let scaling = format!("{}{}", self.axis_max.map(|v| format!("<c:max val=\"{}\"/>", num_text(v))).unwrap_or_default(), self.axis_min.map(|v| format!("<c:min val=\"{}\"/>", num_text(v))).unwrap_or_default());
                let units = format!("{}{}{}", self.base_unit.map(|u| format!("<c:baseTimeUnit val=\"{}\"/>", u.code())).unwrap_or_default(), self.major_unit.map(|v| format!("<c:majorUnit val=\"{}\"/>", num_text(v))).unwrap_or_default(), self.major_time_unit.map(|u| format!("<c:majorTimeUnit val=\"{}\"/>", u.code())).unwrap_or_default());
                o.push_str(&format!("<c:dateAx><c:axId val=\"{ax1}\"/><c:scaling><c:orientation val=\"minMax\"/>{scaling}</c:scaling><c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/>{}{cat_fmt}{common}{line}<c:crossAx val=\"{ax2}\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblOffset val=\"100\"/>{units}</c:dateAx>", t(&self.x_title)));
            } else {
                o.push_str(&format!("<c:catAx><c:axId val=\"{ax1}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/>{}{cat_fmt}{common}{line}<c:crossAx val=\"{ax2}\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>", t(&self.x_title)));
            }
            let val_ax = |id: u32, cross: u32, pos: &str, grid: &str, title: &Option<String>, crosses: &str| format!("<c:valAx><c:axId val=\"{id}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{pos}\"/>{grid}{}<c:numFmt formatCode=\"{}\" sourceLinked=\"{}\"/>{common}<c:spPr><a:ln><a:noFill/></a:ln></c:spPr><c:crossAx val=\"{cross}\"/><c:crosses val=\"{crosses}\"/><c:crossBetween val=\"{}\"/></c:valAx>",
                t(title), if self.grouping == Grouping::Percent { "0%" } else { "General" }, (self.grouping != Grouping::Percent) as u8, if self.kind.is_xy() || self.kind == ChartKind::Area { "midCat" } else { "between" });
            o.push_str(&val_ax(ax2, ax1, val_pos, grid, &self.y_title, "autoZero"));
            if has_secondary && self.kind.is_xy() {
                // Scatter and bubble: a second pair of value axes, the x one hidden, the y one on the right.
                o.push_str(&format!("<c:valAx><c:axId val=\"{ax3}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"1\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>{common}<c:crossAx val=\"{ax4}\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>"));
                o.push_str(&val_ax(ax4, ax3, "r", "", &self.y2_title, "max"));
            } else if has_secondary {
                // The secondary pair: a hidden category axis and a value axis crossing at its maximum (the right).
                o.push_str(&format!("<c:catAx><c:axId val=\"{ax3}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"1\"/><c:axPos val=\"{cat_pos}\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>{common}<c:crossAx val=\"{ax4}\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>"));
                o.push_str(&val_ax(ax4, ax3, if self.kind == ChartKind::Bar { "t" } else { "r" }, "", &self.y2_title, "max"));
            }
        }
        o.push_str("</c:plotArea>");
        if let Some(l) = self.legend { o.push_str(&format!("<c:legend><c:legendPos val=\"{}\"/><c:overlay val=\"0\"/></c:legend>", l.code())); }
        o.push_str("<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>");
        o.push_str("<c:spPr><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"D9D9D9\"/></a:solidFill></a:ln></c:spPr>");
        o.push_str("<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"900\"><a:solidFill><a:srgbClr val=\"595959\"/></a:solidFill></a:defRPr></a:pPr><a:endParaRPr lang=\"en-US\"/></a:p></c:txPr>");
        o.push_str("<c:printSettings><c:headerFooter/><c:pageMargins b=\"0.75\" l=\"0.7\" r=\"0.7\" t=\"0.75\" header=\"0.3\" footer=\"0.3\"/><c:pageSetup/></c:printSettings></c:chartSpace>");
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        for (i, row) in [["", "North", "South"], ["Jan", "10", "4"], ["Feb", "20", "8"], ["Mar", "15", "12"]].iter().enumerate() {
            for (j, t) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, j as u32), t); }
        }
        wb.recalc();
        wb
    }

    #[test]
    fn from_range_and_round_trip() {
        let mut wb = book();
        let r = parse_range("A1:C4").unwrap();
        let c = Chart::from_range(&wb, 0, r, ChartKind::Column, None);
        assert_eq!(c.series.len(), 2);
        assert_eq!(c.series[0].name_ref.as_deref(), Some("Sheet1!$B$1"));
        assert_eq!(c.series[0].cat_ref.as_deref(), Some("Sheet1!$A$2:$A$4"));
        assert_eq!(c.series[1].val_ref.as_deref(), Some("Sheet1!$C$2:$C$4"));
        let d = c.data(&wb, 0);
        assert_eq!(d[0].name, "North");
        assert_eq!(d[0].cats, vec!["Jan", "Feb", "Mar"]);
        assert_eq!(d[1].vals, vec![Some(4.0), Some(8.0), Some(12.0)]);
        let xml = c.to_xml(&wb, 0);
        let mut back = parse(&xml, &wb.theme_colors).unwrap();
        assert_eq!(back.kind, ChartKind::Column);
        assert_eq!(back.series.len(), 2);
        assert_eq!(back.series[1].val_cache, vec![4.0, 8.0, 12.0]);
        assert_eq!(back.series[0].cat_cache, vec!["Jan", "Feb", "Mar"]);
        assert_eq!(back.series[0].val_ref, c.series[0].val_ref);
        assert_eq!(back.title, Some(String::new()));
        // Live data: editing a cell changes what the chart shows.
        wb.set_input(0, Pos::new(1, 1), "99");
        wb.recalc();
        assert_eq!(back.data(&wb, 0)[0].vals[0], Some(99.0));
        // Switching rows and columns.
        let sw = Chart::from_range(&wb, 0, back.source().map(|s| s.1).unwrap(), ChartKind::Column, Some(!back.by_rows()));
        assert_eq!(sw.series.len(), 3);
        back.kind = ChartKind::Bar; back.grouping = Grouping::Stacked; back.data_labels = true; back.x_title = Some("Month".into()); back.y_title = Some("Units".into()); back.title = Some("Sales".into()); back.legend = Some(LegendPos::Top);
        let again = parse(&back.to_xml(&wb, 0), &wb.theme_colors).unwrap();
        assert_eq!((again.kind, again.grouping, again.data_labels, again.legend), (ChartKind::Bar, Grouping::Stacked, true, Some(LegendPos::Top)));
        assert_eq!((again.x_title.as_deref(), again.y_title.as_deref(), again.title.as_deref()), (Some("Month"), Some("Units"), Some("Sales")));
    }

    #[test]
    fn kinds_round_trip() {
        let wb = book();
        let r = parse_range("A1:C4").unwrap();
        for k in ChartKind::ALL {
            let c = Chart::from_range(&wb, 0, r, k, None);
            let back = parse(&c.to_xml(&wb, 0), &wb.theme_colors).unwrap();
            assert_eq!(back.kind, k, "{k:?}");
            assert!(Element::parse(&c.to_xml(&wb, 0)).is_ok());
        }
    }

    #[test]
    fn excel_part() {
        let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:autoTitleDeleted val="1"/><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>Q</c:v></c:pt></c:strCache></c:strRef></c:tx><c:spPr><a:solidFill><a:schemeClr val="accent2"/></a:solidFill></c:spPr><c:val><c:numRef><c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser></c:barChart><c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="1"/><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>3</c:v></c:pt><c:pt idx="1"><c:v>4</c:v></c:pt></c:numLit></c:val></c:ser></c:lineChart><c:catAx><c:axPos val="b"/></c:catAx><c:valAx><c:axPos val="l"/><c:majorGridlines/></c:valAx></c:plotArea><c:legend><c:legendPos val="b"/></c:legend></c:chart></c:chartSpace>"#;
        let c = parse(xml, &crate::numfmt::OFFICE_THEME).unwrap();
        assert_eq!(c.title, None);
        assert_eq!(c.series.len(), 2);
        assert_eq!(c.series[0].name, "Q");
        assert_eq!(c.series[0].val_cache, vec![1.0, 2.5]);
        assert_eq!(c.series[0].color, Some(crate::numfmt::OFFICE_THEME[5]));
        assert_eq!(c.series[1].kind, Some(ChartKind::Line));
        assert!(c.gridlines);
        assert_eq!(c.legend, Some(LegendPos::Bottom));
    }

    #[test]
    fn nice() {
        assert_eq!(nice_scale(0.0, 97.0, 5), (0.0, 100.0, 20.0));
        let (lo, hi, _) = nice_scale(-3.0, 7.0, 5);
        assert!(lo <= -3.0 && hi >= 7.0);
    }

    #[test]
    fn secondary_axis_markers_and_types_round_trip() {
        let wb = book();
        let r = parse_range("A1:C4").unwrap();
        let mut c = Chart::from_range(&wb, 0, r, ChartKind::Column, None);
        c.series[1].kind = Some(ChartKind::Line);
        c.series[1].secondary = true;
        c.series[1].marker = Some(Marker { symbol: Symbol::Triangle, size: Some(8), ..Default::default() });
        c.y2_title = Some("Right".into());
        let xml = c.to_xml(&wb, 0);
        assert!(xml.contains("<c:crosses val=\"max\"/>") && xml.contains("<c:axPos val=\"r\"/>"));
        let back = parse(&xml, &wb.theme_colors).unwrap();
        assert_eq!((back.series[1].kind, back.series[1].secondary, back.series[1].marker), (Some(ChartKind::Line), true, c.series[1].marker));
        assert_eq!((back.series[0].secondary, back.y2_title.as_deref()), (false, Some("Right")));
        for (k, check) in [(ChartKind::Stock, "<c:hiLowLines>"), (ChartKind::Radar, "radarStyle val=\"marker\""), (ChartKind::Bubble, "<c:bubbleSize>")] {
            let mut c = Chart::from_range(&wb, 0, r, k, None);
            c.radar_style = RadarStyle::Filled;
            let xml = c.to_xml(&wb, 0);
            assert!(xml.contains(check) || k == ChartKind::Radar && xml.contains("radarStyle val=\"filled\""), "{k:?}");
            let back = parse(&xml, &wb.theme_colors).unwrap();
            assert_eq!(back.kind, k);
            if k == ChartKind::Radar { assert_eq!(back.radar_style, RadarStyle::Filled); }
            if k == ChartKind::Bubble { assert_eq!(back.series[0].size_ref.as_deref(), Some("Sheet1!$C$2:$C$4")); }
        }
    }

    #[test]
    fn cached_dates_use_their_format() {
        let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:lineChart><c:ser><c:idx val="0"/><c:tx><c:v>Visits</c:v></c:tx><c:cat><c:numRef><c:f>Gone!$A$2:$A$3</c:f><c:numCache><c:formatCode>d-mmm</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>45352</c:v></c:pt><c:pt idx="1"><c:v>45353</c:v></c:pt></c:numCache></c:numRef></c:cat><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>3</c:v></c:pt><c:pt idx="1"><c:v>4</c:v></c:pt></c:numLit></c:val></c:ser></c:lineChart></c:plotArea></c:chart></c:chartSpace>"#;
        let c = parse(xml, &crate::numfmt::OFFICE_THEME).unwrap();
        assert_eq!(c.series[0].cat_format.as_deref(), Some("d-mmm"));
        let wb = Workbook::new();
        assert_eq!(c.data(&wb, 0)[0].cats, vec!["1-Mar", "2-Mar"]);
        // A single named series without autoTitleDeleted: the automatic title.
        assert_eq!(c.title_text(&c.data(&wb, 0)).as_deref(), Some("Visits"));
        let back = parse(&c.to_xml(&wb, 0), &wb.theme_colors).unwrap();
        assert_eq!(back.series[0].cat_format.as_deref(), Some("d-mmm"));
    }
}
