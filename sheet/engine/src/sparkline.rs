//! Sparklines (core sparkline model, native-sparkline.js): tiny line, column and win/loss charts
//! drawn inside cells. A sheet holds groups; each group's settings (type, markers, colors, axis)
//! apply to all its sparklines, and each sparkline draws one data range into one cell. They are
//! read from and written to Excel's `x14:sparklineGroups` worksheet extension, laid out into
//! chart primitives for the screen and for printing, and follow structural edits.

use crate::addr::{Pos, Range};
use crate::book::{Sheet, Workbook};
use crate::chart::Prim;
use crate::value::Value;
use crate::xlsx::xml::{attr, text, Element, Node};

pub const EXT_URI: &str = "{05C60535-1F16-4fd2-B633-F4F36F0B64E0}";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind { #[default] Line, Column, WinLoss }

impl Kind {
    pub fn code(self) -> &'static str { match self { Kind::Line => "line", Kind::Column => "column", Kind::WinLoss => "stacked" } }
    pub fn parse(s: &str) -> Kind { match s { "column" => Kind::Column, "stacked" => Kind::WinLoss, _ => Kind::Line } }
    pub fn label(self) -> &'static str { match self { Kind::Line => "Line", Kind::Column => "Column", Kind::WinLoss => "Win/Loss" } }
}

/// How empty cells show (displayEmptyCellsAs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Empty { #[default] Gap, Zero, Connect }

impl Empty {
    pub fn code(self) -> &'static str { match self { Empty::Gap => "gap", Empty::Zero => "zero", Empty::Connect => "span" } }
    pub fn parse(s: &str) -> Empty { match s { "zero" => Empty::Zero, "span" => Empty::Connect, _ => Empty::Gap } }
}

/// A vertical axis bound: each sparkline its own, the same for the whole group, or a fixed value.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Bound { #[default] Individual, Group, Custom(f64) }

/// One sparkline: the data it draws (a reference such as Sheet1!$A$2:$E$2) and its cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Sparkline { pub data: String, pub loc: Pos }

/// The colors of a group (Excel's default style).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors { pub series: [u8; 3], pub negative: [u8; 3], pub axis: [u8; 3], pub markers: [u8; 3], pub first: [u8; 3], pub last: [u8; 3], pub high: [u8; 3], pub low: [u8; 3] }

impl Default for Colors {
    fn default() -> Self {
        let red = [0xD0, 0x00, 0x00];
        Colors { series: [0x37, 0x60, 0x92], negative: red, axis: [0, 0, 0], markers: red, first: red, last: red, high: red, low: red }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub kind: Kind,
    pub sparklines: Vec<Sparkline>,
    pub markers: bool,
    pub high: bool,
    pub low: bool,
    pub first: bool,
    pub last: bool,
    pub negative: bool,
    pub colors: Colors,
    /// Line weight in points.
    pub line_weight: f32,
    pub min: Bound,
    pub max: Bound,
    pub show_axis: bool,
    /// The date range the points are spaced by (a reference), if a date axis is used.
    pub date_axis: Option<String>,
    pub empty: Empty,
    /// Data in hidden rows and columns is shown.
    pub show_hidden: bool,
    pub right_to_left: bool,
}

impl Default for Group {
    fn default() -> Self {
        Group { kind: Kind::Line, sparklines: vec![], markers: false, high: false, low: false, first: false, last: false, negative: false, colors: Colors::default(), line_weight: 0.75, min: Bound::Individual, max: Bound::Individual, show_axis: false, date_axis: None, empty: Empty::Gap, show_hidden: false, right_to_left: false }
    }
}

/// The group and sparkline drawn in cell `p`.
pub fn at(s: &Sheet, p: Pos) -> Option<(usize, usize)> {
    s.sparklines.iter().enumerate().find_map(|(g, x)| x.sparklines.iter().position(|sp| sp.loc == p).map(|i| (g, i)))
}

/// Removes the sparklines in `r` (groups left empty go too).
pub fn clear(s: &mut Sheet, r: Range) {
    for g in &mut s.sparklines { g.sparklines.retain(|sp| !r.contains(sp.loc)); }
    s.sparklines.retain(|g| !g.sparklines.is_empty());
}

/// Adds a group of sparklines drawing `data` into `loc`: one sparkline per row (or column) of
/// the data, matched to the location cells in order. Err when the shapes don't fit.
pub fn create(wb: &mut Workbook, sheet: usize, kind: Kind, data: &str, loc: Range) -> Result<(), &'static str> {
    let (name, d) = crate::chart::parse_ref(data).ok_or("The data range isn't valid.")?;
    let dsheet = match &name { Some(n) => n.clone(), None => wb.sheets[sheet].name.clone() };
    if wb.sheet_index(&dsheet).is_none() { return Err("The data range isn't valid."); }
    if loc.rows() > 1 && loc.cols() > 1 { return Err("The location range must be a single row or column."); }
    let n = loc.rows().max(loc.cols());
    // The data splits along the side that matches the location's length.
    let by_rows = if loc.cols() == 1 && loc.rows() > 1 { d.rows() == n } else if loc.rows() == 1 && loc.cols() > 1 { d.cols() != n } else { d.rows() == 1 || d.cols() != 1 };
    let parts: Vec<Range> = if n == 1 { vec![d] } else if by_rows && d.rows() == n { (d.r0..=d.r1).map(|r| Range { r0: r, r1: r, ..d }).collect() } else if d.cols() == n { (d.c0..=d.c1).map(|c| Range { c0: c, c1: c, ..d }).collect() } else { return Err("The data range doesn't match the location range.") };
    let locs: Vec<Pos> = loc.iter().collect();
    clear(&mut wb.sheets[sheet], loc);
    let g = Group { kind, sparklines: parts.into_iter().zip(locs).map(|(r, p)| Sparkline { data: crate::chart::make_ref(&dsheet, r), loc: p }).collect(), markers: false, ..Default::default() };
    wb.sheets[sheet].sparklines.push(g);
    Ok(())
}

/// The values a sparkline draws (None: an empty cell shown as a gap), with hidden cells left
/// out unless the group shows them.
pub fn values(wb: &Workbook, sheet: usize, g: &Group, sp: &Sparkline) -> Vec<Option<f64>> {
    let Some((name, r)) = crate::chart::parse_ref(&sp.data) else { return vec![] };
    let Some(si) = (match name { Some(n) => wb.sheet_index(&n), None => Some(sheet) }) else { return vec![] };
    let s = &wb.sheets[si];
    let r = Range { r1: r.r1.min(r.r0 + 999), c1: r.c1.min(r.c0 + 999), ..r };
    let mut out = vec![];
    for p in r.iter() {
        if !g.show_hidden && (s.hidden_rows.contains(&p.row) || s.hidden_cols.contains(&p.col)) { continue; }
        out.push(match s.value(p) {
            Value::Num(n) => Some(n),
            Value::Bool(b) => Some(b as u8 as f64),
            Value::Empty | Value::Str(_) | Value::Err(_) | Value::Array(_) => if g.empty == Empty::Zero { Some(0.0) } else { None },
        });
    }
    out
}

fn date_xs(wb: &Workbook, sheet: usize, g: &Group, n: usize) -> Option<Vec<f64>> {
    let f = g.date_axis.as_deref()?;
    let v = crate::chart::ref_values(wb, sheet, f)?;
    let xs: Vec<f64> = v.iter().map(|x| if let Value::Num(n) = x { *n } else { f64::NAN }).collect();
    (xs.len() >= n && xs.iter().take(n).all(|x| x.is_finite())).then(|| xs[..n].to_vec())
}

fn range_of(v: &[Option<f64>]) -> Option<(f64, f64)> {
    v.iter().flatten().fold(None, |a, &x| Some(a.map_or((x, x), |(lo, hi): (f64, f64)| (lo.min(x), hi.max(x)))))
}

/// The vertical scale (min, max) of a sparkline of `g` whose values are `own`.
pub fn scale(wb: &Workbook, sheet: usize, g: &Group, own: &[Option<f64>]) -> (f64, f64) {
    let all = || g.sparklines.iter().filter_map(|sp| range_of(&values(wb, sheet, g, sp))).fold(None, |a: Option<(f64, f64)>, (l, h)| Some(a.map_or((l, h), |(x, y)| (x.min(l), y.max(h)))));
    let me = range_of(own).unwrap_or((0.0, 0.0));
    let grp = if matches!(g.min, Bound::Group) || matches!(g.max, Bound::Group) { all().unwrap_or(me) } else { me };
    let lo = match g.min { Bound::Individual => me.0, Bound::Group => grp.0, Bound::Custom(v) => v };
    let hi = match g.max { Bound::Individual => me.1, Bound::Group => grp.1, Bound::Custom(v) => v };
    (lo, hi)
}

/// The primitives of sparkline `sp` of `g` drawn into `r` ([x0, y0, x1, y1], pixels), at zoom `z`.
pub fn layout(wb: &Workbook, sheet: usize, g: &Group, sp: &Sparkline, r: [f32; 4], z: f32) -> Vec<Prim> {
    let vals = values(wb, sheet, g, sp);
    let (lo, hi) = scale(wb, sheet, g, &vals);
    let xs = date_xs(wb, sheet, g, vals.len());
    layout_values(g, &vals, xs.as_deref(), lo, hi, r, z)
}

/// The layout of given values (the drawing itself, without the workbook).
pub fn layout_values(g: &Group, vals: &[Option<f64>], xs: Option<&[f64]>, lo: f64, hi: f64, r: [f32; 4], z: f32) -> Vec<Prim> {
    let mut out = vec![];
    let n = vals.len();
    if n == 0 { return out; }
    let pad = 2.0 * z.max(0.5);
    let (x0, y0, x1, y1) = (r[0] + pad, r[1] + pad, r[2] - pad, r[3] - pad);
    if x1 <= x0 || y1 <= y0 { return out; }
    let c = &g.colors;
    let (w, h) = (x1 - x0, y1 - y0);
    let flip = |x: f32| if g.right_to_left { x0 + x1 - x } else { x };
    let first = vals.iter().position(|v| v.is_some());
    let last = vals.iter().rposition(|v| v.is_some());
    let (vmin, vmax) = range_of(vals).unwrap_or((0.0, 0.0));
    // The point's color when it is singled out (later rules win, as in Excel).
    let special = |i: usize, v: f64| -> Option<[u8; 3]> {
        let mut k = None;
        if g.negative && v < 0.0 { k = Some(c.negative); }
        if g.first && Some(i) == first { k = Some(c.first); }
        if g.last && Some(i) == last { k = Some(c.last); }
        if g.low && v == vmin { k = Some(c.low); }
        if g.high && v == vmax { k = Some(c.high); }
        k
    };
    match g.kind {
        Kind::Line => {
            let (lo, hi) = if hi - lo < 1e-12 { (lo - 1.0, hi + 1.0) } else { (lo, hi) };
            let y = |v: f64| y1 - ((v.clamp(lo, hi) - lo) / (hi - lo)) as f32 * h;
            let x = |i: usize| -> f32 {
                let t = match xs { Some(d) if d.len() == n && n > 1 => { let (a, b) = (d.iter().cloned().fold(f64::MAX, f64::min), d.iter().cloned().fold(f64::MIN, f64::max)); if b > a { ((d[i] - a) / (b - a)) as f32 } else { 0.5 } } _ => if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 } };
                flip(x0 + t * w)
            };
            if g.show_axis && lo < 0.0 && hi > 0.0 { out.push(Prim::Line { pts: vec![[x0, y(0.0)], [x1, y(0.0)]], color: c.axis, width: z.max(0.5) }); }
            let width = (g.line_weight * 4.0 / 3.0 * z).max(0.5);
            let mut run: Vec<[f32; 2]> = vec![];
            for (i, v) in vals.iter().enumerate() {
                match v {
                    Some(v) => run.push([x(i), y(*v)]),
                    None if g.empty == Empty::Connect => {}
                    None => { if run.len() > 1 { out.push(Prim::Line { pts: std::mem::take(&mut run), color: c.series, width }); } else { run.clear(); } }
                }
            }
            if run.len() > 1 { out.push(Prim::Line { pts: run, color: c.series, width }); }
            let d = (2.2 * z).max(1.5);
            for (i, v) in vals.iter().enumerate() {
                let Some(v) = *v else { continue };
                let col = special(i, v).or(g.markers.then_some(c.markers));
                if let Some(col) = col { let (px, py) = (x(i), y(v)); out.push(Prim::Poly { pts: vec![[px, py - d], [px + d, py], [px, py + d], [px - d, py]], fill: Some(col), stroke: None, opacity: 1.0, strip: false }); }
            }
        }
        Kind::Column | Kind::WinLoss => {
            let slot = w / n as f32;
            let gap = (slot * 0.15).min(2.0 * z);
            let win = g.kind == Kind::WinLoss;
            let (lo, hi) = if win { (-1.0, 1.0) } else { let (l, h2) = (lo.min(0.0), hi.max(0.0)); if h2 - l < 1e-12 { (l - 1.0, h2 + 1.0) } else { (l, h2) } };
            let y = |v: f64| y1 - ((v.clamp(lo, hi) - lo) / (hi - lo)) as f32 * h;
            let base = y(0.0);
            for (i, v) in vals.iter().enumerate() {
                let Some(v) = *v else { continue };
                let bv = if win { v.signum() } else { v };
                if bv == 0.0 { continue; }
                let xa = flip(x0 + i as f32 * slot + gap / 2.0);
                let xb = flip(x0 + (i + 1) as f32 * slot - gap / 2.0);
                let (xa, xb) = (xa.min(xb), xa.max(xb));
                let (ya, yb) = (y(bv).min(base), y(bv).max(base));
                let yb = if yb - ya < 1.0 { ya + 1.0 } else { yb };
                let col = special(i, v).unwrap_or(c.series);
                out.push(Prim::Rect { r: [xa, ya, xb, yb], fill: Some(col), stroke: None });
            }
            if g.show_axis || win { out.push(Prim::Line { pts: vec![[x0, base], [x1, base]], color: c.axis, width: (0.75 * z).max(0.5) }); }
        }
    }
    out
}

// ---- xlsx ----------------------------------------------------------------------------------

fn color_of(e: Option<&Element>, theme: &[[u8; 3]], def: [u8; 3]) -> [u8; 3] {
    let Some(e) = e else { return def };
    if let Some(h) = e.attr("rgb") { let h = h.trim(); let h = if h.len() == 8 { &h[2..] } else { h }; if let (Ok(v), 6) = (u32::from_str_radix(h, 16), h.len()) { return [(v >> 16) as u8, (v >> 8) as u8, v as u8]; } }
    if let Some(t) = e.attr_u32("theme") {
        // Theme indexes count dk1, lt1, dk2, lt2 then the accents (theme_colors lists them in that order).
        let base = theme.get(t as usize).copied().unwrap_or(def);
        let tint = e.attr_f64("tint").unwrap_or(0.0) as f32;
        return base.map(|x| if tint < 0.0 { (x as f32 * (1.0 + tint)).round() as u8 } else { (x as f32 + (255.0 - x as f32) * tint).round() as u8 });
    }
    def
}

fn group_of(e: &Element, theme: &[[u8; 3]]) -> Group {
    let d = Colors::default();
    let col = |n: &str, def: [u8; 3]| color_of(e.child(n), theme, def);
    let bound = |t: Option<&str>, v: Option<f64>| match t { Some("group") => Bound::Group, Some("custom") => Bound::Custom(v.unwrap_or(0.0)), _ => Bound::Individual };
    let f_of = |x: &Element| x.child("f").map(|f| f.text());
    let sparklines = e.child("sparklines").map(|l| l.all("sparkline").filter_map(|sp| {
        let data = f_of(sp)?;
        let loc = crate::addr::parse_range(&sp.child("sqref")?.text())?.start();
        Some(Sparkline { data, loc })
    }).collect()).unwrap_or_default();
    Group {
        kind: Kind::parse(e.attr("type").unwrap_or("line")), sparklines,
        markers: e.flag("markers", false), high: e.flag("high", false), low: e.flag("low", false), first: e.flag("first", false), last: e.flag("last", false), negative: e.flag("negative", false),
        colors: Colors { series: col("colorSeries", d.series), negative: col("colorNegative", d.negative), axis: col("colorAxis", d.axis), markers: col("colorMarkers", d.markers), first: col("colorFirst", d.first), last: col("colorLast", d.last), high: col("colorHigh", d.high), low: col("colorLow", d.low) },
        line_weight: e.attr_f64("lineWeight").unwrap_or(0.75) as f32,
        min: bound(e.attr("minAxisType"), e.attr_f64("manualMin")), max: bound(e.attr("maxAxisType"), e.attr_f64("manualMax")),
        show_axis: e.flag("displayXAxis", false),
        date_axis: if e.flag("dateAxis", false) { f_of(e) } else { None },
        empty: Empty::parse(e.attr("displayEmptyCellsAs").unwrap_or("zero")),
        show_hidden: e.flag("displayHidden", false), right_to_left: e.flag("rightToLeft", false),
    }
}

/// Moves the sheet's sparkline groups from the carried extLst into the model (after reading).
pub fn take(sheet: &mut Sheet, theme: &[[u8; 3]]) {
    let Some(i) = sheet.extra.iter().position(|(k, _)| k == "extLst") else { return };
    let Ok(mut x) = Element::parse(&sheet.extra[i].1) else { return };
    let mut found = vec![];
    x.children.retain(|n| {
        let Node::Element(ext) = n else { return true };
        if !ext.attr("uri").is_some_and(|u| u.eq_ignore_ascii_case(EXT_URI)) { return true; }
        for gs in ext.all("sparklineGroups") { for g in gs.all("sparklineGroup") { let g = group_of(g, theme); if !g.sparklines.is_empty() { found.push(g); } } }
        false
    });
    if found.is_empty() { return; }
    sheet.sparklines.extend(found);
    if x.elements().next().is_some() { sheet.extra[i].1 = x.to_xml(); } else { sheet.extra.remove(i); }
}

fn rgb_xml(n: &str, c: [u8; 3]) -> String { format!("<x14:{n} rgb=\"FF{:02X}{:02X}{:02X}\"/>", c[0], c[1], c[2]) }

/// The sheet's sparklines as an `<ext>` for the worksheet's extLst (empty when none).
pub fn ext_xml(s: &Sheet) -> String {
    if s.sparklines.is_empty() { return String::new(); }
    let mut x = format!("<ext uri=\"{EXT_URI}\" xmlns:x14=\"{NS_X14}\"><x14:sparklineGroups xmlns:xm=\"{NS_XM}\">");
    for g in &s.sparklines {
        x.push_str("<x14:sparklineGroup");
        if g.kind != Kind::Line { x.push_str(&format!(" type=\"{}\"", g.kind.code())); }
        if (g.line_weight - 0.75).abs() > 1e-3 { x.push_str(&format!(" lineWeight=\"{}\"", g.line_weight)); }
        x.push_str(&format!(" displayEmptyCellsAs=\"{}\"", g.empty.code()));
        for (k, on) in [("markers", g.markers), ("high", g.high), ("low", g.low), ("first", g.first), ("last", g.last), ("negative", g.negative), ("displayXAxis", g.show_axis), ("displayHidden", g.show_hidden), ("rightToLeft", g.right_to_left), ("dateAxis", g.date_axis.is_some())] { if on { x.push_str(&format!(" {k}=\"1\"")); } }
        for (k, m, b) in [("minAxisType", "manualMin", g.min), ("maxAxisType", "manualMax", g.max)] {
            match b { Bound::Individual => {} Bound::Group => x.push_str(&format!(" {k}=\"group\"")), Bound::Custom(v) => x.push_str(&format!(" {k}=\"custom\" {m}=\"{}\"", crate::value::fmt_num(v))) }
        }
        x.push('>');
        let c = &g.colors;
        for (n, v) in [("colorSeries", c.series), ("colorNegative", c.negative), ("colorAxis", c.axis), ("colorMarkers", c.markers), ("colorFirst", c.first), ("colorLast", c.last), ("colorHigh", c.high), ("colorLow", c.low)] { x.push_str(&rgb_xml(n, v)); }
        if let Some(d) = &g.date_axis { x.push_str(&format!("<xm:f>{}</xm:f>", text(d.trim_start_matches('=')))); }
        x.push_str("<x14:sparklines>");
        for sp in &g.sparklines { x.push_str(&format!("<x14:sparkline><xm:f>{}</xm:f><xm:sqref>{}</xm:sqref></x14:sparkline>", text(sp.data.trim_start_matches('=')), attr(&sp.loc.a1()))); }
        x.push_str("</x14:sparklines></x14:sparklineGroup>");
    }
    x.push_str("</x14:sparklineGroups></ext>");
    x
}

/// Adds the sheet's sparkline extension to the worksheet's extension list being written.
pub fn add_ext(ext: &mut Option<String>, s: &Sheet) {
    let x = ext_xml(s);
    if x.is_empty() { return; }
    match ext { Some(e) => e.push_str(&x), None => *ext = Some(x) }
}

// ---- following edits -----------------------------------------------------------------------

fn rewrite_refs(wb: &mut Workbook, f: impl Fn(&str, &str) -> String) {
    for s in &mut wb.sheets {
        let own = s.name.clone();
        for g in &mut s.sparklines {
            for sp in &mut g.sparklines { sp.data = f(&own, &sp.data); }
            if let Some(d) = &mut g.date_axis { *d = f(&own, d); }
        }
    }
}

fn move_locs(s: &mut Sheet, f: impl Fn(Pos) -> Option<Pos>) {
    for g in &mut s.sparklines { g.sparklines.retain_mut(|sp| match f(sp.loc) { Some(p) => { sp.loc = p; true } None => false }); }
    s.sparklines.retain(|g| !g.sparklines.is_empty());
}

/// Sparkline data references and cells following a structural edit (called by follow::follow).
pub fn follow(wb: &mut Workbook, e: &crate::follow::Edit) {
    use crate::follow::Edit;
    use crate::formula::adjust;
    if wb.sheets.iter().all(|s| s.sparklines.is_empty()) { return; }
    match *e {
        Edit::Insert { sheet, rows, at, n } => {
            let target = wb.sheets[sheet].name.clone();
            rewrite_refs(wb, |own, f| adjust::insert(f, own, &target, rows, at, n));
            move_locs(&mut wb.sheets[sheet], |p| crate::condformat::shift_range(Range::cell(p), rows, at, n).map(|r| r.start()));
        }
        Edit::ShiftCells { sheet, r, down, delete } => {
            let name = wb.sheets[sheet].name.clone();
            rewrite_refs(wb, |own, f| adjust::shift_cells(f, own, &name, r, down, delete));
            let n = if down { r.rows() as i64 } else { r.cols() as i64 };
            move_locs(&mut wb.sheets[sheet], |p| {
                let inside = if down { p.col >= r.c0 && p.col <= r.c1 && p.row >= r.r0 } else { p.row >= r.r0 && p.row <= r.r1 && p.col >= r.c0 };
                if !inside { return Some(p); }
                if delete && r.contains(p) { return None; }
                let d = if delete { -n } else { n };
                let (row, col) = if down { (p.row as i64 + d, p.col as i64) } else { (p.row as i64, p.col as i64 + d) };
                (row >= 0 && col >= 0 && row < crate::addr::MAX_ROWS as i64 && col < crate::addr::MAX_COLS as i64).then(|| Pos::new(row as u32, col as u32))
            });
        }
        Edit::Move { from, src, to, at } => {
            let (dr, dc) = (at.row as i64 - src.r0 as i64, at.col as i64 - src.c0 as i64);
            let (fname, tname) = (wb.sheets[from].name.clone(), wb.sheets[to].name.clone());
            rewrite_refs(wb, |own, f| if from == to { adjust::move_area(f, own, &fname, src, dr, dc) } else { crate::ops::moved_to_sheet(f, own, &fname, &tname, src, dr, dc) });
            let dest = Range { r0: at.row, c0: at.col, r1: (at.row as i64 + src.rows() as i64 - 1) as u32, c1: (at.col as i64 + src.cols() as i64 - 1) as u32 };
            // Sparklines at the destination are overwritten; those in the source go along.
            let mut moving = vec![];
            for g in &mut wb.sheets[from].sparklines {
                let (inside, rest): (Vec<Sparkline>, Vec<Sparkline>) = g.sparklines.drain(..).partition(|sp| src.contains(sp.loc));
                g.sparklines = rest;
                if !inside.is_empty() { moving.push(Group { sparklines: inside.into_iter().map(|mut sp| { sp.loc = Pos::new((sp.loc.row as i64 + dr) as u32, (sp.loc.col as i64 + dc) as u32); sp }).collect(), ..g.clone() }); }
            }
            wb.sheets[from].sparklines.retain(|g| !g.sparklines.is_empty());
            clear(&mut wb.sheets[to], dest);
            wb.sheets[to].sparklines.extend(moving);
        }
        Edit::Rename { old, new } => rewrite_refs(wb, |_, f| adjust::rename_sheet(f, old, new)),
        Edit::DeleteSheet { gone } => rewrite_refs(wb, |_, f| adjust::drop_sheet(f, gone)),
        Edit::DuplicateSheet { orig, copy } => {
            let oname = wb.sheets[orig].name.clone();
            for g in &mut wb.sheets[copy].sparklines { for sp in &mut g.sparklines { if let Some((None, rg)) = crate::chart::parse_ref(&sp.data) { sp.data = crate::chart::make_ref(&oname, rg); } } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::parse_range;

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        for r in 0..3u32 { for c in 0..5u32 { wb.set_input(0, Pos::new(r, c), &format!("{}", (r as i64 + 1) * (c as i64 - 2))); } }
        wb.recalc();
        wb
    }

    #[test]
    fn create_splits_rows() {
        let mut wb = book();
        create(&mut wb, 0, Kind::Column, "A1:E3", parse_range("F1:F3").unwrap()).unwrap();
        let g = &wb.sheets[0].sparklines[0];
        assert_eq!(g.sparklines.len(), 3);
        assert_eq!(g.sparklines[1].data, "Sheet1!$A$2:$E$2");
        assert_eq!(g.sparklines[2].loc, Pos::new(2, 5));
        assert_eq!(values(&wb, 0, g, &g.sparklines[0]), vec![Some(-2.0), Some(-1.0), Some(0.0), Some(1.0), Some(2.0)]);
        assert!(create(&mut wb, 0, Kind::Line, "A1:E3", parse_range("F1:G2").unwrap()).is_err());
        assert!(at(&wb.sheets[0], Pos::new(1, 5)).is_some());
    }

    #[test]
    fn layout_draws_each_kind() {
        let mut g = Group { high: true, low: true, ..Default::default() };
        let v = [Some(1.0), None, Some(3.0), Some(-2.0)];
        let p = layout_values(&g, &v, None, -2.0, 3.0, [0.0, 0.0, 60.0, 20.0], 1.0);
        assert_eq!(p.iter().filter(|x| matches!(x, Prim::Line { .. })).count(), 1, "gap splits the line, single point run dropped");
        assert_eq!(p.iter().filter(|x| matches!(x, Prim::Poly { .. })).count(), 2);
        g.empty = Empty::Connect;
        let p = layout_values(&g, &v, None, -2.0, 3.0, [0.0, 0.0, 60.0, 20.0], 1.0);
        assert!(matches!(&p[0], Prim::Line { pts, .. } if pts.len() == 3));
        g.kind = Kind::Column; g.negative = true;
        let p = layout_values(&g, &v, None, -2.0, 3.0, [0.0, 0.0, 60.0, 20.0], 1.0);
        assert_eq!(p.iter().filter(|x| matches!(x, Prim::Rect { .. })).count(), 3);
        assert!(p.iter().any(|x| matches!(x, Prim::Rect { fill: Some(c), .. } if *c == g.colors.low)));
        g.kind = Kind::WinLoss;
        let p = layout_values(&g, &v, None, 0.0, 0.0, [0.0, 0.0, 60.0, 20.0], 1.0);
        assert!(p.iter().any(|x| matches!(x, Prim::Line { .. })));
    }

    #[test]
    fn xlsx_round_trip() {
        let mut wb = book();
        create(&mut wb, 0, Kind::Line, "A1:E3", parse_range("F1:F3").unwrap()).unwrap();
        { let g = &mut wb.sheets[0].sparklines[0]; g.markers = true; g.high = true; g.min = Bound::Custom(-5.0); g.max = Bound::Group; g.empty = Empty::Connect; g.colors.series = [1, 2, 3]; g.right_to_left = true; g.show_axis = true; }
        create(&mut wb, 0, Kind::WinLoss, "A1:E1", parse_range("G1").unwrap()).unwrap();
        let bytes = crate::xlsx::write(&wb).unwrap();
        let back = crate::xlsx::read(&bytes).unwrap();
        assert_eq!(back.sheets[0].sparklines, wb.sheets[0].sparklines);
        assert!(!back.sheets[0].extra.iter().any(|(_, v)| v.contains("sparklineGroups")));
        // Saved again, nothing doubles.
        let again = crate::xlsx::read(&crate::xlsx::write(&back).unwrap()).unwrap();
        assert_eq!(again.sheets[0].sparklines.len(), 2);
    }

    #[test]
    fn follows_inserts_and_deletes() {
        let mut wb = book();
        create(&mut wb, 0, Kind::Line, "A1:E3", parse_range("F1:F3").unwrap()).unwrap();
        crate::follow::follow(&mut wb, crate::follow::Edit::Insert { sheet: 0, rows: true, at: 0, n: 2 });
        let g = &wb.sheets[0].sparklines[0];
        assert_eq!(g.sparklines[0].loc, Pos::new(2, 5));
        assert_eq!(g.sparklines[0].data, "Sheet1!$A$3:$E$3");
        crate::follow::follow(&mut wb, crate::follow::Edit::Insert { sheet: 0, rows: false, at: 5, n: -1 });
        assert!(wb.sheets[0].sparklines.is_empty(), "deleting the location column removes them");
    }
}

// ---- printing ------------------------------------------------------------------------------

/// Draws cell `p`'s sparkline or picture in cell into `r` (page pixels) for printing.
pub fn print_cell(wb: &Workbook, si: usize, p: Pos, r: [f32; 4], z: f32, assets: &mut crate::print::objects::Assets, out: &mut Vec<word_engine::layout::DrawItem>) {
    use word_engine::layout::DrawItem;
    use word_engine::Color;
    let s = &wb.sheets[si];
    let col = |c: [u8; 3]| Color(c[0], c[1], c[2]);
    if let Some((g, i)) = at(s, p) {
        let g = &s.sparklines[g];
        for prim in layout(wb, si, g, &g.sparklines[i], r, z) {
            match prim {
                Prim::Rect { r, fill: Some(f), .. } => out.push(DrawItem::Rect { x: r[0], y: r[1], w: r[2] - r[0], h: r[3] - r[1], color: col(f) }),
                Prim::Line { pts, color, width } => out.push(DrawItem::Path { points: pts.iter().map(|q| (q[0], q[1])).collect(), closed: false, fill: None, stroke: Some((col(color), width)), ink: None }),
                Prim::Poly { pts, fill, .. } => out.push(DrawItem::Path { points: pts.iter().map(|q| (q[0], q[1])).collect(), closed: true, fill: fill.map(col), stroke: None, ink: None }),
                _ => {}
            }
        }
    }
    let v = s.value(p);
    if let Some((data, ext)) = crate::cellimage::parse(&v).and_then(|(src, _)| crate::cellimage::bytes(src)) {
        let Some((iw, ih)) = crate::cellimage::image_size(&data) else { return };
        let f = crate::cellimage::fit([r[0] + z, r[1] + z, r[2] - z, r[3] - z], iw as f32, ih as f32);
        let name = format!("jadwal-cellpic-{si}-{}-{}", p.row, p.col);
        let ct = match ext.as_str() { "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", _ => "image/png" };
        assets.insert(name.clone(), word_engine::model::Asset { bytes: data, content_type: ct.into() });
        out.push(DrawItem::Image { asset: name, x: f[0], y: f[1], w: f[2] - f[0], h: f[3] - f[1], pos: 0, rot: 0.0, flip: (false, false) });
    }
}

#[cfg(test)]
mod print_tests {
    use super::*;

    #[test]
    fn sparklines_print() {
        let mut wb = Workbook::new();
        for c in 0..4u32 { wb.set_input(0, Pos::new(0, c), &format!("{}", c * c)); }
        wb.recalc();
        create(&mut wb, 0, Kind::Column, "A1:D1", Range::cell(Pos::new(0, 4))).unwrap();
        let pages = crate::print::sheet_pages(&wb, 0, None);
        let pg = crate::print::draw_page(&wb, &pages[0], &Default::default());
        let blue = word_engine::Color(0x37, 0x60, 0x92);
        assert!(pg.items.iter().filter(|i| matches!(i, word_engine::layout::DrawItem::Rect { color, .. } if *color == blue)).count() >= 3);
    }
}
