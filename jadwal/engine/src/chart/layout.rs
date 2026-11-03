//! Chart geometry, shared by the screen (app chart_ui.rs, painted with egui) and printing
//! (print_objects.rs, word-engine draw items): a chart laid out into a rectangle as a list of
//! primitives — rectangles, polylines, polygons, text — in the target's pixels.

use super::{nice_scale, num_fmt, Chart, ChartKind, Grouping, LegendPos, RadarStyle, SeriesData, Symbol, TimeUnit};

pub type Rgb = [u8; 3];

mod ex;

const INK: Rgb = [0x59, 0x59, 0x59];
const GRID: Rgb = [0xD9, 0xD9, 0xD9];
const WHITE: Rgb = [0xFF, 0xFF, 0xFF];
const DARK: Rgb = [0x40, 0x40, 0x40];

/// A drawing primitive; points are [x, y] in the target's pixels (y down).
#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Rect { r: [f32; 4], fill: Option<Rgb>, stroke: Option<(Rgb, f32)> },
    /// An open polyline.
    Line { pts: Vec<[f32; 2]>, color: Rgb, width: f32 },
    /// A closed polygon. A `strip` polygon is two chains of equal length — the first half
    /// forward, the second half back (a pie or doughnut slice, an area band, a filled radar) —
    /// so it can be filled as a triangle strip between them; otherwise it is convex.
    /// `opacity` below 1 lets what is under it show through.
    Poly { pts: Vec<[f32; 2]>, fill: Option<Rgb>, stroke: Option<(Rgb, f32)>, opacity: f32, strip: bool },
    /// Text anchored at `at` by `h` (0 left, 1 center, 2 right) and `v` (0 top, 1 middle,
    /// 2 bottom), turned `angle` degrees about the anchor (negative: counterclockwise).
    Text { at: [f32; 2], text: String, size: f32, bold: bool, color: Rgb, angle: f32, h: u8, v: u8 },
}

/// Text measurement of the target: [width, line height] of `t` at `px` pixels.
pub trait Measure { fn size(&self, t: &str, px: f32, bold: bool) -> [f32; 2]; }

/// An axis tick label: as many decimals as the step needs, General style, thousands separators.
pub fn tick_text(v: f64, step: f64, percent: bool) -> String {
    if percent { return format!("{}%", (v * 100.0).round()); }
    let dec = (0..8).find(|&d| { let k = step * 10f64.powi(d as i32); (k - k.round()).abs() < 1e-6 * k.abs().max(1.0) }).unwrap_or(8);
    let s = format!("{:.*}", dec.min(8), v);
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if dec == 0 && v.abs() >= 10000.0 { crate::numfmt::format_number(v, "#,##0", false).text } else if s == "-0" { "0".into() } else { s }
}

/// A data label's text.
pub fn value_text(v: f64) -> String { crate::numfmt::format_number(v, "General", false).text }

struct Out<'a> { v: Vec<Prim>, m: &'a dyn Measure, z: f32, marks: Vec<(usize, Option<usize>)> }

impl Out<'_> {
    /// Primitives pushed from here on belong to series `i` (None: to no series).
    fn series(&mut self, i: Option<usize>) { self.marks.push((self.v.len(), i)); }
    /// `t` broken into lines no wider than `width` (at spaces; long words stay whole).
    fn wrap(&self, t: &str, size: f32, bold: bool, width: f32) -> Vec<String> {
        let mut out = vec![];
        for para in t.split('\n') {
            let mut line = String::new();
            for w in para.split(' ') {
                let cand = if line.is_empty() { w.to_string() } else { format!("{line} {w}") };
                if !line.is_empty() && self.w(&cand, size, bold) > width { out.push(std::mem::replace(&mut line, w.to_string())); } else { line = cand; }
            }
            out.push(line);
        }
        out
    }
    fn text(&mut self, t: &str, at: [f32; 2], size: f32, bold: bool, color: Rgb, h: u8, v: u8) { self.rot_text(t, at, size, bold, color, h, v, 0.0); }
    #[allow(clippy::too_many_arguments)]
    fn rot_text(&mut self, t: &str, at: [f32; 2], size: f32, bold: bool, color: Rgb, h: u8, v: u8, angle: f32) {
        if !t.is_empty() { self.v.push(Prim::Text { at, text: t.to_string(), size: size.max(3.0), bold, color, angle, h, v }); }
    }
    fn w(&self, t: &str, size: f32, bold: bool) -> f32 { self.m.size(t, size.max(3.0), bold)[0] }
    fn h(&self, size: f32) -> f32 { self.m.size("Ag", size.max(3.0), false)[1] }
    fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: Rgb) { self.v.push(Prim::Line { pts: vec![a, b], color, width }); }
    fn rect(&mut self, r: [f32; 4], fill: Rgb) { if r[2] > r[0] && r[3] > r[1] { self.v.push(Prim::Rect { r, fill: Some(fill), stroke: None }); } }
    fn hair(&self) -> f32 { self.z.max(0.5) }
}

fn ellipse(c: [f32; 2], rx: f32, ry: f32, n: usize) -> Vec<[f32; 2]> { (0..n).map(|i| { let t = i as f32 / n as f32 * std::f32::consts::TAU; [c[0] + rx * t.cos(), c[1] + ry * t.sin()] }).collect() }

/// A marker of `sym`, `d` pixels across, centered at `p`.

/// A marker with its own fill and outline color.
fn marker2(o: &mut Out, sym: Symbol, p: [f32; 2], d: f32, col: Rgb, line: Option<Rgb>) {
    let r = d / 2.0;
    let hair = o.hair();
    let poly = |o: &mut Out, pts: Vec<[f32; 2]>| o.v.push(Prim::Poly { pts, fill: Some(col), stroke: line.map(|l| (l, hair)), opacity: 1.0, strip: false });
    let col = if matches!(sym, Symbol::X | Symbol::Star | Symbol::Plus) { line.unwrap_or(col) } else { col };
    let w = (d / 5.0).max(o.z);
    match sym {
        Symbol::None => {}
        Symbol::Circle | Symbol::Auto => poly(o, ellipse(p, r, r, 24)),
        Symbol::Square => poly(o, vec![[p[0] - r, p[1] - r], [p[0] + r, p[1] - r], [p[0] + r, p[1] + r], [p[0] - r, p[1] + r]]),
        Symbol::Diamond => poly(o, vec![[p[0], p[1] - r * 1.2], [p[0] + r * 1.2, p[1]], [p[0], p[1] + r * 1.2], [p[0] - r * 1.2, p[1]]]),
        Symbol::Triangle => poly(o, vec![[p[0], p[1] - r * 1.15], [p[0] + r * 1.1, p[1] + r * 0.8], [p[0] - r * 1.1, p[1] + r * 0.8]]),
        Symbol::Dash => poly(o, vec![[p[0] - r, p[1] - w / 2.0], [p[0] + r, p[1] - w / 2.0], [p[0] + r, p[1] + w / 2.0], [p[0] - r, p[1] + w / 2.0]]),
        Symbol::Dot => { let q = r * 0.45; poly(o, vec![[p[0] - q, p[1] - q], [p[0] + q, p[1] - q], [p[0] + q, p[1] + q], [p[0] - q, p[1] + q]]) }
        Symbol::X | Symbol::Star | Symbol::Plus => {
            if sym != Symbol::Plus { o.line([p[0] - r, p[1] - r], [p[0] + r, p[1] + r], w, col); o.line([p[0] - r, p[1] + r], [p[0] + r, p[1] - r], w, col); }
            if sym != Symbol::X { o.line([p[0], p[1] - r], [p[0], p[1] + r], w, col); }
            if sym == Symbol::Plus { o.line([p[0] - r, p[1]], [p[0] + r, p[1]], w, col); }
        }
    }
}

/// The marker series `i` shows (symbol and size in pixels), if any; `points` is how many
/// points it has (a lone point always shows).
fn series_marker(c: &Chart, i: usize, points: usize, z: f32) -> Option<(Symbol, f32)> {
    let dflt = 7.0 * z;
    match c.series.get(i).and_then(|s| s.marker) {
        Some(m) if m.symbol == Symbol::None => (points == 1).then_some((Symbol::Circle, dflt)),
        Some(m) => Some((if m.symbol == Symbol::Auto { Symbol::auto(i) } else { m.symbol }, m.size.map_or(dflt, |s| s as f32 * 4.0 / 3.0 * z))),
        None => (c.markers || points == 1).then_some((Symbol::Circle, dflt)),
    }
}

/// The marker colors of series `i`: fill and outline.
fn marker_colors(c: &Chart, i: usize, col: Rgb) -> (Rgb, Option<Rgb>) {
    let m = c.series.get(i).and_then(|s| s.marker);
    (m.and_then(|m| m.fill).unwrap_or(col), m.and_then(|m| m.line))
}

/// Whether series `i` shows data labels.
fn shows_labels(c: &Chart, i: usize) -> bool { c.series.get(i).and_then(|s| s.labels).unwrap_or(c.data_labels) }

/// A series' line width in pixels.
fn line_px(c: &Chart, i: usize, z: f32) -> f32 { c.series.get(i).and_then(|s| s.line_width).map_or(2.25 * z, |w| (w * 4.0 / 3.0 * z).max(0.5)) }

/// A polyline drawn with a series' dash (preset dash names, in multiples of the width) and smoothing.
fn styled_line(o: &mut Out, c: &Chart, i: usize, pts: Vec<[f32; 2]>, color: Rgb, width: f32) {
    let s = c.series.get(i);
    let pts = if s.is_some_and(|s| s.smooth) && pts.len() > 2 { smooth(&pts) } else { pts };
    let pattern: &[f32] = match s.and_then(|s| s.dash.as_deref()) {
        None | Some("solid") => { o.v.push(Prim::Line { pts, color, width }); return; }
        Some("sysDot") | Some("dot") => &[1.0, 1.0],
        Some("sysDash") => &[3.0, 1.0],
        Some("lgDash") => &[8.0, 3.0],
        Some("dashDot") | Some("sysDashDot") => &[4.0, 3.0, 1.0, 3.0],
        Some("lgDashDot") => &[8.0, 3.0, 1.0, 3.0],
        Some("lgDashDotDot") | Some("sysDashDotDot") => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
        Some(_) => &[4.0, 3.0],
    };
    let unit = width.max(1.0);
    let (mut k, mut left, mut on) = (0usize, pattern[0] * unit, true);
    let mut cur: Vec<[f32; 2]> = vec![pts[0]];
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let mut t = 0.0;
        while len - t > left {
            t += left;
            let p = [a[0] + (b[0] - a[0]) * t / len, a[1] + (b[1] - a[1]) * t / len];
            if on { cur.push(p); if cur.len() >= 2 { o.v.push(Prim::Line { pts: std::mem::take(&mut cur), color, width }); } } else { cur = vec![p]; }
            on = !on; k = (k + 1) % pattern.len(); left = pattern[k] * unit;
        }
        left -= len - t;
        if on { cur.push(b); }
    }
    if on && cur.len() >= 2 { o.v.push(Prim::Line { pts: cur, color, width }); }
}

/// A smoothed polyline through the points (Catmull-Rom, as Excel's smoothed lines).
fn smooth(p: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out = vec![p[0]];
    for i in 0..p.len() - 1 {
        let (p0, p1, p2, p3) = (p[i.saturating_sub(1)], p[i], p[i + 1], p[(i + 2).min(p.len() - 1)]);
        for s in 1..=8 {
            let t = s as f32 / 8.0;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
            out.push([f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]);
        }
    }
    out
}

/// Lays out chart `c` showing `data` in `r` (x0, y0, x1, y1) at zoom `z`; `theme` gives the
/// palette's accents.
pub fn layout(c: &Chart, data: &[SeriesData], r: [f32; 4], theme: &[[u8; 3]], z: f32, m: &dyn Measure) -> Vec<Prim> { layout_tagged(c, data, r, theme, z, m).0 }

/// The layout with, for each primitive, the series it draws (for hit testing).
pub fn layout_tagged(c: &Chart, data: &[SeriesData], r: [f32; 4], theme: &[[u8; 3]], z: f32, m: &dyn Measure) -> (Vec<Prim>, Vec<Option<usize>>) {
    let mut o = Out { v: vec![], m, z, marks: vec![] };
    o.v.push(Prim::Rect { r, fill: Some(WHITE), stroke: Some((GRID, o.hair())) });
    let pad = 8.0 * z;
    let mut area = [r[0] + pad, r[1] + pad, r[2] - pad, r[3] - pad];
    let small = 12.0 * z;
    if let Some(t) = c.title_text(data) {
        // Wrapped to the chart's width, each line centered.
        let sz = 18.0 * z;
        let lh = o.h(sz);
        for l in o.wrap(&t, sz, false, area[2] - area[0]) { o.text(&l, [(area[0] + area[2]) / 2.0, area[1]], sz, false, INK, 1, 0); area[1] += lh; }
        area[1] += 6.0 * z;
    }
    // Legend entries: categories for round charts, else series.
    let entries: Vec<(String, Rgb)> = if c.kind.is_round() {
        data.first().map(|d| d.cats.iter().enumerate().map(|(i, n)| (if n.is_empty() { format!("{}", i + 1) } else { n.clone() }, c.color(i, theme))).collect()).unwrap_or_default()
    } else { data.iter().enumerate().map(|(i, d)| (if d.name.is_empty() { format!("Series{}", i + 1) } else { d.name.clone() }, c.color(i, theme))).collect() };
    if let Some(pos) = c.legend.filter(|_| !entries.is_empty()) {
        let key = 9.0 * z;
        let h = o.h(small);
        let ws: Vec<f32> = entries.iter().map(|e| o.w(&e.0, small, false)).collect();
        match pos {
            LegendPos::Right | LegendPos::Left => {
                let w = (ws.iter().cloned().fold(0.0, f32::max) + key + 10.0 * z).min((area[2] - area[0]) * 0.4);
                let total = entries.len() as f32 * (h + 3.0 * z);
                let x = if pos == LegendPos::Right { area[2] - w } else { area[0] };
                let mut y = (area[1] + area[3]) / 2.0 - total / 2.0;
                for (n, cl) in &entries { o.rect([x, y + (h - key) / 2.0, x + key, y + (h + key) / 2.0], *cl); o.text(n, [x + key + 4.0 * z, y], small, false, INK, 0, 0); y += h + 3.0 * z; }
                if pos == LegendPos::Right { area[2] -= w + 6.0 * z; } else { area[0] += w + 6.0 * z; }
            }
            LegendPos::Top | LegendPos::Bottom => {
                let total: f32 = ws.iter().map(|w| w + key + 14.0 * z).sum();
                let y = if pos == LegendPos::Bottom { area[3] - h } else { area[1] };
                let mut x = ((area[0] + area[2]) / 2.0 - total / 2.0).max(area[0]);
                for ((n, cl), w) in entries.iter().zip(&ws) { o.rect([x, y + (h - key) / 2.0, x + key, y + (h + key) / 2.0], *cl); o.text(n, [x + key + 4.0 * z, y], small, false, INK, 0, 0); x += w + key + 14.0 * z; }
                if pos == LegendPos::Bottom { area[3] -= h + 6.0 * z; } else { area[1] += h + 6.0 * z; }
            }
        }
    }
    if area[2] - area[0] >= 10.0 && area[3] - area[1] >= 10.0 {
        match c.kind {
            ChartKind::Pie | ChartKind::Doughnut => pie(&mut o, c, data, area, theme),
            ChartKind::Radar => radar(&mut o, c, data, area, theme),
            k if k.is_ex() => ex::draw(&mut o, c, data, area, theme),
            _ => axes(&mut o, c, data, area, theme),
        }
    }
    let mut tags = vec![None; o.v.len()];
    let mut marks = o.marks.clone();
    marks.push((o.v.len(), None));
    for w in marks.windows(2) { for t in tags.iter_mut().take(w[1].0).skip(w[0].0) { *t = w[0].1; } }
    (o.v, tags)
}

fn pie(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    let z = o.z;
    let Some(d) = data.first() else { return };
    o.series(Some(0));
    let vals: Vec<f64> = d.vals.iter().map(|v| v.unwrap_or(0.0).max(0.0)).collect();
    let total: f64 = vals.iter().sum();
    if total <= 0.0 { return; }
    let (cx, cy) = ((area[0] + area[2]) / 2.0, (area[1] + area[3]) / 2.0);
    let rad = (area[2] - area[0]).min(area[3] - area[1]) / 2.0 * 0.92;
    let hole = if c.kind == ChartKind::Doughnut { rad * 0.5 } else { 0.0 };
    let at = |a: f32, r: f32| [cx + a.cos() * r, cy + a.sin() * r];
    let mut a0 = -std::f32::consts::FRAC_PI_2;
    let mut labels = vec![];
    for (i, v) in vals.iter().enumerate() {
        let sweep = (*v / total) as f32 * std::f32::consts::TAU;
        let steps = ((sweep / 0.06).ceil() as usize).max(1);
        let mut pts: Vec<[f32; 2]> = (0..=steps).map(|k| at(a0 + sweep * k as f32 / steps as f32, rad)).collect();
        // The inner chain: the hole's arc back, or the center repeated.
        pts.extend((0..=steps).rev().map(|k| if hole > 0.0 { at(a0 + sweep * k as f32 / steps as f32, hole) } else { [cx, cy] }));
        if sweep > 0.0 { o.v.push(Prim::Poly { pts, fill: Some(c.color(i, theme)), stroke: None, opacity: 1.0, strip: true }); }
        o.line(at(a0, hole), at(a0, rad), 1.5 * z, WHITE);
        if shows_labels(c, 0) && *v > 0.0 {
            let t = if c.percent_labels { format!("{}%", (*v / total * 100.0).round()) } else { value_text(*v) };
            labels.push((at(a0 + sweep / 2.0, if hole > 0.0 { (rad + hole) / 2.0 } else { rad * 0.65 }), t));
        }
        a0 += sweep;
    }
    o.series(None);
    for (p, t) in labels { o.text(&t, p, 11.0 * z, false, WHITE, 1, 1); }
}

fn ticks(min: f64, max: f64, step: f64) -> Vec<f64> { let mut v = vec![]; let mut t = min; while t <= max + step * 1e-6 && v.len() < 50 { v.push(t); t += step; } v }

/// A value axis's scale: bounds, step, and the stacked totals per category.
struct Scale { min: f64, max: f64, step: f64, percent: bool, sums: Vec<(f64, f64)> }

impl Scale {
    fn pct(&self, k: usize, v: f64) -> f64 { if self.percent { let t = self.sums[k].0 - self.sums[k].1; if t == 0.0 { 0.0 } else { v / t } } else { v } }
    fn t(&self, v: f64) -> f32 { ((v - self.min) / (self.max - self.min).max(1e-12)) as f32 }
}

// ---- dates ----

/// (year, month, day) of a serial date (1900 date system).
pub(crate) fn ymd(serial: i64) -> (i64, i64, i64) {
    let z = serial - 25569 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

/// The serial date of (year, month, day).
pub(crate) fn serial(y: i64, m: i64, d: i64) -> i64 {
    let y = y - (m <= 2) as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468 + 25569
}

/// The index of the base unit holding a serial date.
fn unit_index(x: f64, u: TimeUnit) -> i64 {
    let (y, m, _) = ymd(x.floor() as i64);
    match u { TimeUnit::Days => x.floor() as i64, TimeUnit::Months => y * 12 + m - 1, TimeUnit::Years => y }
}

/// The first day of a base unit.
fn unit_start(i: i64, u: TimeUnit) -> f64 {
    (match u { TimeUnit::Days => i, TimeUnit::Months => serial(i.div_euclid(12), i.rem_euclid(12) + 1, 1), TimeUnit::Years => serial(i, 1, 1) }) as f64
}

/// Excel's automatic base unit: years when every date is a 1 January, months when each is a
/// first of the month, else days.
fn auto_unit(xs: &[f64]) -> TimeUnit {
    let d: Vec<(i64, i64, i64)> = xs.iter().map(|x| ymd(x.floor() as i64)).collect();
    if d.len() > 1 && d.iter().all(|t| t.1 == 1 && t.2 == 1) { TimeUnit::Years } else if d.len() > 1 && d.iter().all(|t| t.2 == 1) { TimeUnit::Months } else { TimeUnit::Days }
}

/// How many base units a major unit spans.
fn units_in(major: f64, mu: TimeUnit, base: TimeUnit) -> f64 {
    let days = |u: TimeUnit| match u { TimeUnit::Days => 1.0, TimeUnit::Months => 30.4375, TimeUnit::Years => 365.25 };
    match (mu, base) { (TimeUnit::Years, TimeUnit::Months) => major * 12.0, (TimeUnit::Months, TimeUnit::Years) => major / 12.0, _ => major * days(mu) / days(base) }
}

/// A category axis spaced by time: the base unit, its first index, the slot count, and each
/// point's slot.
struct DateAxis { unit: TimeUnit, first: i64, slots: usize, slot: Vec<f32> }

fn date_axis(c: &Chart, data: &[SeriesData], n: usize) -> Option<DateAxis> {
    let dated = c.date_axis || c.series.first().and_then(|s| s.cat_format.as_deref()).is_some_and(crate::numfmt::is_date_format);
    if !dated || c.kind.is_xy() { return None; }
    let d = data.iter().find(|d| !d.xs.is_empty())?;
    // Categories that aren't numbers (text) keep even spacing.
    if d.cats.iter().zip(&d.xs).any(|(c, x)| !c.is_empty() && *x <= 0.0) { return None; }
    let xs: Vec<f64> = d.xs.iter().copied().take(n).collect();
    if xs.is_empty() { return None; }
    let unit = c.base_unit.unwrap_or_else(|| auto_unit(&xs));
    let lo = c.axis_min.unwrap_or(xs.iter().cloned().fold(f64::INFINITY, f64::min));
    let hi = c.axis_max.unwrap_or(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let (first, last) = (unit_index(lo, unit), unit_index(hi, unit));
    if last < first || last - first > 20000 { return None; }
    Some(DateAxis { unit, first, slots: (last - first + 1) as usize, slot: xs.iter().map(|x| (unit_index(*x, unit) - first) as f32).collect() })
}

fn axes(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    let z = o.z;
    let small = 11.5 * z;
    let horizontal = c.kind == ChartKind::Bar;
    let xy = c.kind.is_xy();
    let groupable = c.kind.has_grouping();
    let percent = c.grouping == Grouping::Percent && groupable;
    let stacked = c.grouping != Grouping::Clustered && groupable;
    let n = data.iter().map(|d| d.vals.len()).max().unwrap_or(0).max(1);
    let kind_of = |i: usize| if c.kind.comboable() { c.series.get(i).and_then(|s| s.kind).unwrap_or(c.kind) } else { c.kind };
    let sec_of = |i: usize| c.allows_secondary() && c.series.get(i).is_some_and(|s| s.secondary);
    let has_sec = (0..data.len()).any(sec_of);
    let stacks = |i: usize| stacked && kind_of(i) == c.kind;
    // Value scales: primary, and secondary for the series on it.
    let scale = |sec: bool| -> Scale {
        let (mut lo, mut hi) = (0.0f64, 0.0f64);
        let mut sums = vec![(0.0f64, 0.0f64); n];
        let mut any_stacked = false;
        for (i, d) in data.iter().enumerate().filter(|(i, _)| sec_of(*i) == sec) {
            any_stacked |= stacks(i);
            for (k, v) in d.vals.iter().enumerate() {
                let Some(v) = v else { continue };
                if stacks(i) { if *v >= 0.0 { sums[k].0 += v; } else { sums[k].1 += v; } } else { lo = lo.min(*v); hi = hi.max(*v); }
            }
        }
        if stacked { for (a, b) in &sums { hi = hi.max(*a); lo = lo.min(*b); } }
        let pc = percent && (!sec || any_stacked);
        let (min, max, step) = if pc { (if lo < 0.0 { -1.0 } else { 0.0 }, 1.0, 0.2) } else { nice_scale(lo, hi, 5) };
        Scale { min, max, step, percent: pc, sums }
    };
    let s1 = scale(false);
    let s2 = if has_sec { Some(scale(true)) } else { None };
    // Scatter and bubble x range.
    let (xmin, xmax, xstep) = if xy {
        let xs: Vec<f64> = data.iter().flat_map(|d| d.xs.iter().copied()).collect();
        let (a, b) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(*x), b.max(*x)));
        if a.is_finite() { nice_scale(a, b, 5) } else { (0.0, 1.0, 0.2) }
    } else { (0.0, 1.0, 1.0) };
    // A date axis places points by date in slots of its base unit.
    let dates = date_axis(c, data, n);
    let date_fmt = c.cat_axis_format.clone().or_else(|| c.series.first().and_then(|s| s.cat_format.clone())).unwrap_or_else(|| match dates.as_ref().map(|d| d.unit) { Some(TimeUnit::Years) => "yyyy".into(), Some(TimeUnit::Months) => "mmm-yy".into(), _ => "d-mmm".into() });
    let slots = dates.as_ref().map_or(n, |d| d.slots.max(1));
    // Labels and margins.
    let t1 = ticks(s1.min, s1.max, s1.step);
    let tick_w = t1.iter().map(|t| o.w(&tick_text(*t, s1.step, s1.percent), small, false)).fold(0.0, f32::max);
    let t2 = s2.as_ref().map(|s| ticks(s.min, s.max, s.step)).unwrap_or_default();
    let tick_w2 = s2.as_ref().map(|s| t2.iter().map(|t| o.w(&tick_text(*t, s.step, s.percent), small, false)).fold(0.0, f32::max)).unwrap_or(0.0);
    let cats: Vec<String> = match &dates {
        Some(d) => (0..d.slots).map(|j| num_fmt(unit_start(d.first + j as i64, d.unit), &date_fmt)).collect(),
        None => data.iter().map(|d| &d.cats).find(|c| !c.is_empty()).cloned().unwrap_or_else(|| (1..=n).map(|i| i.to_string()).collect()),
    };
    let cat_w = cats.iter().take(2000).map(|t| o.w(t, small, false)).fold(0.0, f32::max);
    let lh = small * 1.3;
    let aw = area[2] - area[0];
    let ah = area[3] - area[1];
    // Axis titles wrap: along the plot's width (x), or its height (rotated y titles).
    let xt: Vec<String> = c.x_title.as_ref().map(|t| o.wrap(t, small, true, aw * 0.9)).unwrap_or_default();
    let yt: Vec<String> = c.y_title.as_ref().map(|t| o.wrap(t, small, true, ah * 0.9)).unwrap_or_default();
    let y2t: Vec<String> = if has_sec { c.y2_title.as_ref().map(|t| o.wrap(t, small, true, if horizontal { aw * 0.9 } else { ah * 0.9 })).unwrap_or_default() } else { vec![] };
    let mut plot = area;
    if !yt.is_empty() { plot[0] += lh * yt.len() as f32 + 4.0 * z; }
    if !xt.is_empty() { plot[3] -= lh * xt.len() as f32 + 4.0 * z; }
    if !y2t.is_empty() { if horizontal { plot[1] += lh * y2t.len() as f32 + 4.0 * z; } else { plot[2] -= lh * y2t.len() as f32 + 4.0 * z; } }
    if horizontal { plot[0] += cat_w.min(aw * 0.35) + 6.0 * z; if has_sec { plot[1] += lh; } } else { plot[0] += tick_w + 6.0 * z; if has_sec { plot[2] -= tick_w2 + 6.0 * z; } }
    plot[3] -= lh;
    plot[2] -= 4.0 * z;
    plot[1] += 4.0 * z;
    let (pw, ph) = (plot[2] - plot[0], plot[3] - plot[1]);
    if pw < 8.0 || ph < 8.0 { return; }
    let (pcx, pcy) = ((plot[0] + plot[2]) / 2.0, (plot[1] + plot[3]) / 2.0);
    // Axis titles, line by line.
    for (j, t) in xt.iter().enumerate() { o.text(t, [pcx, area[3] - lh * (xt.len() - j) as f32 + lh / 2.0], small, true, INK, 1, 1); }
    for (j, t) in yt.iter().enumerate() { o.rot_text(t, [area[0] + lh / 2.0 + lh * j as f32, pcy], small, true, INK, 1, 1, -90.0); }
    for (j, t) in y2t.iter().enumerate() {
        if horizontal { o.text(t, [pcx, area[1] + lh / 2.0 + lh * j as f32], small, true, INK, 1, 1); }
        else { o.rot_text(t, [area[2] - lh / 2.0 - lh * (y2t.len() - 1 - j) as f32, pcy], small, true, INK, 1, 1, 90.0); }
    }
    // Positions: along the value axis (of a scale), x values, category centers.
    let vpos = |s: &Scale, v: f64| -> f32 { let t = s.t(v); if horizontal { plot[0] + t * pw } else { plot[3] - t * ph } };
    let xpos = |x: f64| -> f32 { plot[0] + ((x - xmin) / (xmax - xmin).max(1e-12)) as f32 * pw };
    let band = if horizontal { ph } else { pw } / slots as f32;
    // Slot of point k: its date's on a date axis, else k. Bar charts list the first category at the bottom.
    let slot_of = |k: usize| -> f32 { dates.as_ref().and_then(|d| d.slot.get(k).copied()).unwrap_or(k as f32) };
    let spos = |j: f32| -> f32 { if horizontal { plot[3] - band * (j + 0.5) } else { plot[0] + band * (j + 0.5) } };
    let cpos = |k: usize| -> f32 { spos(slot_of(k)) };
    let hair = o.hair();
    // Gridlines and tick labels.
    for t in &t1 {
        let v = vpos(&s1, *t);
        let label = tick_text(*t, s1.step, s1.percent);
        if horizontal {
            if c.gridlines { o.line([v, plot[1]], [v, plot[3]], hair, GRID); }
            o.text(&label, [v, plot[3] + 2.0 * z], small, false, INK, 1, 0);
        } else {
            if c.gridlines { o.line([plot[0], v], [plot[2], v], hair, GRID); }
            o.text(&label, [plot[0] - 4.0 * z, v], small, false, INK, 2, 1);
        }
    }
    if let Some(s) = &s2 {
        for t in &t2 {
            let v = vpos(s, *t);
            let label = tick_text(*t, s.step, s.percent);
            if horizontal { o.text(&label, [v, plot[1] - 2.0 * z], small, false, INK, 1, 2); } else { o.text(&label, [plot[2] + 4.0 * z, v], small, false, INK, 0, 1); }
        }
    }
    // Category (or x, or date) labels, thinned so they don't collide.
    if xy {
        let mut x = xmin;
        while x <= xmax + xstep * 1e-6 {
            let px = xpos(x);
            if c.gridlines { o.line([px, plot[1]], [px, plot[3]], 0.5 * z, GRID); }
            o.text(&tick_text(x, xstep, false), [px, plot[3] + 2.0 * z], small, false, INK, 1, 0);
            x += xstep;
        }
    } else {
        let fit = if horizontal { ((lh / band).ceil() as usize).max(1) } else { (((cat_w + 6.0 * z) / band).ceil() as usize).max(1) };
        // A date axis's major unit sets the label step (at least what fits).
        let every = match (&dates, c.major_unit) { (Some(d), Some(mu)) => (units_in(mu, c.major_time_unit.unwrap_or(d.unit), d.unit).round() as usize).max(fit).max(1), _ => fit };
        for k in (0..slots).step_by(every) {
            let t = cats.get(k).cloned().unwrap_or_default();
            let at = spos(k as f32);
            if horizontal { o.text(&t, [plot[0] - 4.0 * z, at], small, false, INK, 2, 1); } else { o.text(&t, [at, plot[3] + 2.0 * z], small, false, INK, 1, 0); }
        }
    }
    // The category axis line at zero.
    let zero = vpos(&s1, 0.0f64.clamp(s1.min, s1.max));
    if horizontal { o.line([zero, plot[1]], [zero, plot[3]], hair, GRID); } else { o.line([plot[0], zero], [plot[2], zero], hair, GRID); }
    let mut labels: Vec<([f32; 2], u8, u8, String)> = vec![];
    let scale_of = |i: usize| if sec_of(i) { s2.as_ref().unwrap_or(&s1) } else { &s1 };
    let outline = |i: usize| c.series.get(i).and_then(|s| s.line_color).map(|l| (l, c.series[i].line_width.map_or(hair, |w| w * 4.0 / 3.0 * z)));
    // Bars: each axis's bars cluster (or stack) on their own.
    let group = band / 2.5;
    for sec in [false, true] {
        let bars: Vec<usize> = (0..data.len()).filter(|&i| sec_of(i) == sec && matches!(kind_of(i), ChartKind::Column | ChartKind::Bar)).collect();
        let mut pos_acc = vec![0.0f64; n];
        let mut neg_acc = vec![0.0f64; n];
        for (bi, &i) in bars.iter().enumerate() {
            o.series(Some(i));
            let sc = scale_of(i);
            let cl = c.color(i, theme);
            for (k, v) in data[i].vals.iter().enumerate() {
                let Some(v) = *v else { continue };
                let same = stacks(i);
                let v = if kind_of(i) == c.kind { sc.pct(k, v) } else { v };
                let (w, off) = if same { (group, -group / 2.0) } else { let w = group / bars.len() as f32; (w, -group / 2.0 + w * bi as f32) };
                let (a, b) = if same { let acc = if v >= 0.0 { &mut pos_acc[k] } else { &mut neg_acc[k] }; let a = *acc; *acc += v; (a, *acc) } else { (0.0, v) };
                let (va, vb) = (vpos(sc, a), vpos(sc, b));
                let center = cpos(k);
                let rc = if horizontal { [va.min(vb), center - off - w, va.max(vb), center - off] } else { [center + off, va.min(vb), center + off + w, va.max(vb)] };
                match outline(i) { Some(st) => o.v.push(Prim::Rect { r: rc, fill: Some(cl), stroke: Some(st) }), None => o.rect(rc, cl) }
                if shows_labels(c, i) {
                    let raw = value_text(data[i].vals[k].unwrap_or(0.0));
                    if horizontal { labels.push(([rc[2] + 2.0 * z, (rc[1] + rc[3]) / 2.0], 0, 1, raw)); } else { labels.push(([(rc[0] + rc[2]) / 2.0, rc[1] - z], 1, 2, raw)); }
                }
            }
        }
    }
    // Areas (stacked areas accumulate).
    let mut area_acc = [vec![0.0f64; n], vec![0.0f64; n]];
    for i in (0..data.len()).filter(|&i| kind_of(i) == ChartKind::Area) {
        o.series(Some(i));
        let sc = scale_of(i);
        let acc = &mut area_acc[sec_of(i) as usize];
        let fill = c.color(i, theme);
        let mut tops = vec![];
        for k in 0..n {
            let v = data[i].vals.get(k).copied().flatten().unwrap_or(0.0);
            let v = if kind_of(i) == c.kind { sc.pct(k, v) } else { v };
            let b = if stacks(i) { acc[k] } else { 0.0 };
            if stacks(i) { acc[k] = b + v; }
            tops.push((cpos(k), vpos(sc, b), vpos(sc, b + v)));
        }
        if tops.len() == 1 { let a = tops[0]; o.line([a.0, a.1], [a.0, a.2], 2.0 * z, fill); continue; }
        let mut pts: Vec<[f32; 2]> = tops.iter().map(|t| [t.0, t.2]).collect();
        pts.extend(tops.iter().rev().map(|t| [t.0, t.1]));
        o.v.push(Prim::Poly { pts, fill: Some(fill), stroke: outline(i), opacity: 0.85, strip: true });
        if shows_labels(c, i) { for (k, t) in tops.iter().enumerate() { labels.push(([t.0, t.2 - 2.0 * z], 1, 2, value_text(data[i].vals.get(k).copied().flatten().unwrap_or(0.0)))); } }
    }
    // Stock: high-low lines and up/down bars.
    if c.kind == ChartKind::Stock {
        o.series(None);
        let st: Vec<usize> = (0..data.len()).filter(|&i| kind_of(i) == ChartKind::Stock).collect();
        for k in 0..n {
            let vs: Vec<f64> = st.iter().filter_map(|&i| data[i].vals.get(k).copied().flatten()).collect();
            let x = cpos(k);
            if c.hi_low_lines && vs.len() >= 2 {
                let (lo, hi) = vs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
                o.line([x, vpos(&s1, lo)], [x, vpos(&s1, hi)], hair, DARK);
            }
            if c.up_down_bars && st.len() >= 4 {
                if let (Some(op), Some(cl)) = (data[st[0]].vals.get(k).copied().flatten(), data[st[st.len() - 1]].vals.get(k).copied().flatten()) {
                    let (a, b) = (vpos(&s1, op), vpos(&s1, cl));
                    let r = [x - group / 2.0, a.min(b), x + group / 2.0, a.max(b).max(a.min(b) + hair)];
                    o.v.push(Prim::Rect { r, fill: Some(if cl >= op { WHITE } else { DARK }), stroke: Some((DARK, hair)) });
                }
            }
        }
        for (si, &i) in st.iter().enumerate() {
            o.series(Some(i));
            // The close shows as a tick unless up/down bars show it, or the series has a marker.
            let own = c.series.get(i).and_then(|s| s.marker).is_some();
            let mk = if own { series_marker(c, i, 2, z) } else { (!c.up_down_bars && si == st.len() - 1 && st.len() >= 3).then_some((Symbol::Dash, (group * 0.6).min(10.0 * z))) };
            let Some((sym, d)) = mk else { continue };
            let (fill, line) = if own { marker_colors(c, i, c.color(i, theme)) } else { (DARK, None) };
            for (k, v) in data[i].vals.iter().enumerate() {
                let Some(v) = v else { continue };
                let x = cpos(k) + if !own { d / 2.0 } else { 0.0 };
                marker2(o, sym, [x, vpos(&s1, *v)], d, fill, line);
            }
        }
    }
    // Bubbles: area (or width) by size, the largest a quarter of the plot's smaller side across.
    if c.kind == ChartKind::Bubble {
        let smax = data.iter().flat_map(|d| d.sizes.iter().flatten()).fold(0.0f64, |a, s| a.max(s.abs()));
        let scale = if c.bubble_scale == 0 { 1.0 } else { c.bubble_scale as f32 / 100.0 };
        let dmax = pw.min(ph) * 0.25 * scale;
        for (i, d) in data.iter().enumerate() {
            o.series(Some(i));
            let sc = scale_of(i);
            let cl = c.color(i, theme);
            let stroke = outline(i).unwrap_or((WHITE, hair));
            for (k, v) in d.vals.iter().enumerate() {
                let (Some(v), Some(Some(s))) = (v, d.sizes.get(k)) else { continue };
                if smax <= 0.0 || *s == 0.0 || (*s < 0.0 && !c.show_neg_bubbles) { continue; }
                let f = (s.abs() / smax) as f32;
                let rad = ((if c.size_width { f } else { f.sqrt() }) * dmax / 2.0).max(1.5 * z);
                let p = [xpos(d.xs.get(k).copied().unwrap_or((k + 1) as f64)), vpos(sc, *v)];
                // Negative bubbles show as outlines.
                if *s < 0.0 { o.v.push(Prim::Poly { pts: ellipse(p, rad, rad, 40), fill: None, stroke: Some((cl, hair.max(1.0))), opacity: 1.0, strip: false }); }
                else { o.v.push(Prim::Poly { pts: ellipse(p, rad, rad, 40), fill: Some(cl), stroke: Some(stroke), opacity: 0.75, strip: false }); }
                if shows_labels(c, i) { labels.push((p, 1, 1, value_text(*v))); }
            }
        }
    }
    // Lines and scatter points on top.
    let mut line_acc = [vec![0.0f64; n], vec![0.0f64; n]];
    for i in (0..data.len()).filter(|&i| matches!(kind_of(i), ChartKind::Line | ChartKind::Scatter)) {
        o.series(Some(i));
        let sc = scale_of(i);
        let cl = c.color(i, theme);
        let mut pts: Vec<Option<[f32; 2]>> = vec![];
        for (k, v) in data[i].vals.iter().enumerate() {
            let Some(v) = *v else { pts.push(None); continue };
            let pt = if xy && kind_of(i) == ChartKind::Scatter {
                [xpos(data[i].xs.get(k).copied().unwrap_or((k + 1) as f64)), vpos(sc, v)]
            } else {
                let v2 = if kind_of(i) == c.kind { sc.pct(k, v) } else { v };
                let y = if stacks(i) { let a = &mut line_acc[sec_of(i) as usize]; a[k] += v2; a[k] } else { v2 };
                if horizontal { [vpos(sc, y), cpos(k)] } else { [cpos(k), vpos(sc, y)] }
            };
            pts.push(Some(pt));
        }
        if !(xy && kind_of(i) == ChartKind::Scatter && c.markers) {
            // Connected runs as polylines.
            let w = line_px(c, i, z);
            for run in pts.split(|p| p.is_none()) {
                let run: Vec<[f32; 2]> = run.iter().flatten().copied().collect();
                if run.len() >= 2 { styled_line(o, c, i, run, cl, w); }
            }
        }
        if let Some((sym, d)) = series_marker(c, i, pts.iter().flatten().count(), z) {
            let (fill, line) = marker_colors(c, i, cl);
            for p in pts.iter().flatten() { marker2(o, sym, *p, d, fill, line); }
        }
        if shows_labels(c, i) { for (k, p) in pts.iter().enumerate() { if let Some(p) = p { labels.push(([p[0], p[1] - 5.0 * z], 1, 2, value_text(data[i].vals[k].unwrap_or(0.0)))); } } }
    }
    o.series(None);
    for (p, h, v, t) in labels { o.text(&t, p, small, false, INK, h, v); }
}

fn radar(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    let z = o.z;
    let small = 11.5 * z;
    let n = data.iter().map(|d| d.vals.len()).max().unwrap_or(0).max(1);
    let (lo, hi) = data.iter().flat_map(|d| d.vals.iter().flatten()).fold((0.0f64, 0.0f64), |(a, b), v| (a.min(*v), b.max(*v)));
    let (min, max, step) = nice_scale(lo, hi, 5);
    let cats: Vec<String> = data.iter().map(|d| &d.cats).find(|c| !c.is_empty()).cloned().unwrap_or_else(|| (1..=n).map(|i| i.to_string()).collect());
    let cat_w = cats.iter().map(|t| o.w(t, small, false)).fold(0.0, f32::max);
    let lh = o.h(small);
    let (cx, cy) = ((area[0] + area[2]) / 2.0, (area[1] + area[3]) / 2.0);
    let rad = ((area[2] - area[0]) / 2.0 - cat_w - 6.0 * z).min((area[3] - area[1]) / 2.0 - lh - 6.0 * z);
    if rad < 8.0 { return; }
    let ang = |k: usize| -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / n as f32;
    let at = |k: usize, v: f64| { let t = (((v - min) / (max - min).max(1e-12)) as f32).max(0.0) * rad; [cx + ang(k).cos() * t, cy + ang(k).sin() * t] };
    let hair = o.hair();
    // Rings at the ticks, spokes, tick labels up the first spoke, category labels outside.
    let tk = ticks(min, max, step);
    for t in &tk {
        if *t <= min && tk.len() > 1 { continue; }
        if c.gridlines || *t >= max - step * 1e-6 {
            let mut ring: Vec<[f32; 2]> = (0..n).map(|k| at(k, *t)).collect();
            if n < 3 { ring = ellipse([cx, cy], (at(0, *t)[1] - cy).abs(), (at(0, *t)[1] - cy).abs(), 48); }
            ring.push(ring[0]);
            o.v.push(Prim::Line { pts: ring, color: GRID, width: hair });
        }
    }
    for k in 0..n { o.line([cx, cy], at(k, max), hair, GRID); }
    for t in &tk { let p = at(0, *t); o.text(&tick_text(*t, step, false), [p[0] - 4.0 * z, p[1]], small, false, INK, 2, 1); }
    for (k, t) in cats.iter().enumerate().take(n) {
        let a = ang(k);
        let p = [cx + a.cos() * (rad + 4.0 * z), cy + a.sin() * (rad + 4.0 * z)];
        let h = if a.cos() > 0.3 { 0 } else if a.cos() < -0.3 { 2 } else { 1 };
        let v = if a.sin() > 0.3 { 0 } else if a.sin() < -0.3 { 2 } else { 1 };
        o.text(t, p, small, false, INK, h, v);
    }
    let filled = c.radar_style == RadarStyle::Filled;
    let mut labels = vec![];
    for (i, d) in data.iter().enumerate() {
        o.series(Some(i));
        let cl = c.color(i, theme);
        let lw = line_px(c, i, z);
        let pts: Vec<Option<[f32; 2]>> = (0..n).map(|k| d.vals.get(k).copied().flatten().map(|v| at(k, v))).collect();
        if filled {
            // A fan from the center: the outline closed, the center repeated as the other chain.
            let mut ring: Vec<[f32; 2]> = (0..n).map(|k| pts[k].unwrap_or([cx, cy])).collect();
            ring.push(ring[0]);
            let m = ring.len();
            ring.extend(std::iter::repeat_n([cx, cy], m));
            o.v.push(Prim::Poly { pts: ring, fill: Some(cl), stroke: None, opacity: 1.0, strip: true });
        } else {
            // The outline, closed where the points allow it.
            let mut run: Vec<[f32; 2]> = vec![];
            for k in 0..=n {
                match pts[k % n] { Some(p) => run.push(p), None => { if run.len() >= 2 { styled_line(o, c, i, std::mem::take(&mut run), cl, lw); } run.clear(); } }
            }
            if run.len() >= 2 { styled_line(o, c, i, run, cl, lw); }
        }
        if !filled || c.series.get(i).and_then(|s| s.marker).is_some() {
            if let Some((sym, dm)) = series_marker(c, i, pts.iter().flatten().count(), z) { let (f, l) = marker_colors(c, i, cl); for p in pts.iter().flatten() { marker2(o, sym, *p, dm, f, l); } }
        }
        if shows_labels(c, i) { for (k, p) in pts.iter().enumerate() { if let Some(p) = p { labels.push(([p[0], p[1] - 5.0 * z], value_text(d.vals[k].unwrap_or(0.0)))); } } }
    }
    o.series(None);
    for (p, t) in labels { o.text(&t, p, small, false, INK, 1, 2); }
}
