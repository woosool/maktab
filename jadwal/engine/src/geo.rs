//! Maps: region boundaries from Natural Earth 1:110m (public domain; countries and US states,
//! simplified, built by tools/make-geo.py into `geo/regions.bin`), name lookup, projections
//! and the filled map chart, drawn as SVG and rasterized.
//!
//! A filled map is saved as a picture (so every program shows it) whose description carries
//! its settings: reading the file brings back a live map that redraws when its data changes.

use crate::book::Workbook;
use crate::value::Value;
use std::fmt::Write;
use std::sync::{Arc, OnceLock};

static DATA: &[u8] = include_bytes!("geo/regions.bin");

/// One region: whether it's a US state (else a country), its names (display name first, then
/// other names and codes), and its outline rings in degrees (lon, lat).
pub struct Region { pub us_state: bool, pub names: Vec<String>, pub rings: Vec<Vec<(f32, f32)>> }

impl Region { pub fn name(&self) -> &str { &self.names[0] } }

pub fn regions() -> &'static [Region] {
    static R: OnceLock<Vec<Region>> = OnceLock::new();
    R.get_or_init(|| {
        let d = DATA;
        let mut out = vec![];
        if d.len() < 6 || &d[..4] != b"JGEO" { return out; }
        let u16_at = |i: usize| u16::from_le_bytes([d[i], d[i + 1]]) as usize;
        let i16_at = |i: usize| i16::from_le_bytes([d[i], d[i + 1]]) as f32 / 100.0;
        let n = u16_at(4);
        let mut i = 6;
        for _ in 0..n {
            let kind = d[i]; let len = d[i + 1] as usize; i += 2;
            let names = String::from_utf8_lossy(&d[i..i + len]).split(';').map(str::to_string).collect();
            i += len;
            let nr = u16_at(i); i += 2;
            let mut rings = vec![];
            for _ in 0..nr {
                let np = u16_at(i); i += 2;
                rings.push((0..np).map(|k| (i16_at(i + k * 4), i16_at(i + k * 4 + 2))).collect());
                i += np * 4;
            }
            out.push(Region { us_state: kind == 1, names, rings });
        }
        out
    })
}

/// Common names Natural Earth spells differently.
const ALIASES: &[(&str, &str)] = &[
    ("usa", "United States of America"), ("us", "United States of America"), ("united states", "United States of America"), ("america", "United States of America"),
    ("uk", "United Kingdom"), ("great britain", "United Kingdom"), ("britain", "United Kingdom"), ("england", "United Kingdom"),
    ("russian federation", "Russia"), ("south korea", "South Korea"), ("korea", "South Korea"), ("republic of korea", "South Korea"), ("north korea", "North Korea"),
    ("czech republic", "Czechia"), ("ivory coast", "Côte d'Ivoire"), ("cote d'ivoire", "Côte d'Ivoire"), ("drc", "Dem. Rep. Congo"), ("democratic republic of the congo", "Dem. Rep. Congo"),
    ("uae", "United Arab Emirates"), ("holland", "Netherlands"), ("the netherlands", "Netherlands"), ("burma", "Myanmar"), ("swaziland", "eSwatini"), ("eswatini", "eSwatini"),
    ("turkiye", "Turkey"), ("türkiye", "Turkey"), ("viet nam", "Vietnam"), ("persia", "Iran"), ("macedonia", "North Macedonia"), ("bosnia", "Bosnia and Herz."),
];

/// The region a name, alias or code stands for (US states only when `states`, else countries
/// first then states).
pub fn find(name: &str, states: bool) -> Option<usize> {
    let n = name.trim();
    if n.is_empty() { return None; }
    let lc = n.to_lowercase();
    let target = ALIASES.iter().find(|a| a.0 == lc).map(|a| a.1.to_lowercase()).unwrap_or(lc);
    let rs = regions();
    let hit = |want_state: bool| rs.iter().position(|r| r.us_state == want_state && r.names.iter().any(|x| x.to_lowercase() == target));
    if states { hit(true) } else { hit(false).or_else(|| hit(true)) }
}

/// Projects a point for a map of countries (Equal Earth) or of US states (a conic-like
/// equirectangular with Alaska and Hawaii moved into insets), as unitless x right, y down.
pub fn project(lon: f32, lat: f32, states: bool) -> (f32, f32) {
    if states {
        let (mut lon, mut lat) = (if lon > 0.0 { lon - 360.0 } else { lon }, lat);
        if lat > 50.0 { lon = -123.0 + (lon + 170.0) * 0.35; lat = 22.0 + (lat - 52.0) * 0.35; }
        else if lon < -150.0 { lon += 50.0; lat += 5.0; }
        let c = 38f32.to_radians().cos();
        return (lon * c, -lat);
    }
    // Equal Earth (Šavrič, Patterson, Jenny 2018).
    let (a1, a2, a3, a4) = (1.340264f32, -0.081106f32, 0.000893f32, 0.003796f32);
    let m = 3f32.sqrt() / 2.0;
    let phi = lat.to_radians();
    let t = (m * phi.sin()).asin();
    let t2 = t * t; let t6 = t2 * t2 * t2;
    let x = 2.0 * 3f32.sqrt() * lon.to_radians() * t.cos() / (3.0 * (9.0 * a4 * t6 * t2 + 7.0 * a3 * t6 + 3.0 * a2 * t2 + a1));
    let y = t * (a4 * t6 * t2 + a3 * t6 + a2 * t2 + a1);
    (x * 57.3, -y * 57.3)
}

/// A filled map chart's settings: a range whose first column names regions and whose last
/// column holds values (an optional header row), a title, and the two ends of the color scale.
#[derive(Clone, Debug, PartialEq)]
pub struct FilledMap { pub source: String, pub title: String, pub low: [u8; 3], pub high: [u8; 3] }

impl Default for FilledMap { fn default() -> Self { FilledMap { source: String::new(), title: String::new(), low: [0xDE, 0xEB, 0xF7], high: [0x08, 0x51, 0x9C] } } }

const TAG: &str = "jadwal:filledmap";
fn hex(c: [u8; 3]) -> String { format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }
fn unhex(s: &str) -> Option<[u8; 3]> { let v = u32::from_str_radix(s.get(..6)?, 16).ok()?; Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]) }

impl FilledMap {
    /// The picture description that carries the settings.
    pub fn tag(&self) -> String { format!("{TAG}\t{}\t{}\t{}\t{}", self.source, self.title.replace('\t', " "), hex(self.low), hex(self.high)) }
    pub fn from_tag(descr: &str) -> Option<FilledMap> {
        let f: Vec<&str> = descr.strip_prefix(TAG)?.strip_prefix('\t')?.split('\t').collect();
        let d = FilledMap::default();
        Some(FilledMap { source: f.first()?.to_string(), title: f.get(1).copied().unwrap_or("").into(), low: f.get(2).and_then(|x| unhex(x)).unwrap_or(d.low), high: f.get(3).and_then(|x| unhex(x)).unwrap_or(d.high) })
    }

    /// The (region name, value) rows of the source (rows with a number in the last column).
    pub fn rows(&self, wb: &Workbook, sheet: usize) -> Vec<(String, f64)> {
        let Some((sh, r)) = crate::chart::parse_ref(&self.source) else { return vec![] };
        let si = sh.and_then(|n| wb.sheet_index(&n)).unwrap_or(sheet);
        (r.r0..=r.r1).filter_map(|row| {
            let name = wb.display(si, crate::Pos::new(row, r.c0));
            match wb.sheets[si].value(crate::Pos::new(row, r.c1)) { Value::Num(n) if !name.trim().is_empty() => Some((name, n)), _ => None }
        }).collect()
    }

    /// The map as an SVG of `w` × `h` pixels, and how many rows named no known region.
    pub fn svg(&self, wb: &Workbook, sheet: usize, w: f32, h: f32) -> (String, usize) {
        let rows = self.rows(wb, sheet);
        let rs = regions();
        let mut unknown = 0;
        // US states when every name that's found is a state.
        let found: Vec<(usize, f64)> = rows.iter().filter_map(|(n, v)| { let r = find(n, false); if r.is_none() { unknown += 1; } r.map(|i| (i, *v)) }).collect();
        let states = !found.is_empty() && found.iter().all(|(i, _)| rs[*i].us_state);
        let (lo, hi) = found.iter().fold((f64::MAX, f64::MIN), |a, x| (a.0.min(x.1), a.1.max(x.1)));
        let shown: Vec<usize> = (0..rs.len()).filter(|&i| rs[i].us_state == states && rs[i].name() != "Antarctica").collect();
        let mut bb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &i in &shown { for ring in &rs[i].rings { for &(x, y) in ring { let (px, py) = project(x, y, states); bb = (bb.0.min(px), bb.1.min(py), bb.2.max(px), bb.3.max(py)); } } }
        let title_h = if self.title.is_empty() { 8.0 } else { 34.0 };
        let legend_h = if found.is_empty() { 8.0 } else { 34.0 };
        let (aw, ah) = (w - 16.0, (h - title_h - legend_h).max(10.0));
        let k = (aw / (bb.2 - bb.0).max(1e-3)).min(ah / (bb.3 - bb.1).max(1e-3));
        let ox = 8.0 + (aw - (bb.2 - bb.0) * k) / 2.0;
        let oy = title_h + (ah - (bb.3 - bb.1) * k) / 2.0;
        let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"#FFFFFF\"/>");
        if !self.title.is_empty() { let _ = write!(s, "<text x=\"{}\" y=\"24\" font-family=\"Liberation Sans\" font-size=\"16\" text-anchor=\"middle\" fill=\"#404040\">{}</text>", w / 2.0, crate::xlsx::xml::text(&self.title)); }
        for &i in &shown {
            let v = found.iter().find(|f| f.0 == i).map(|f| f.1);
            let fill = match v { Some(v) => { let t = if hi > lo { ((v - lo) / (hi - lo)) as f32 } else { 1.0 }; mix(self.low, self.high, t) } None => [0xE6, 0xE6, 0xE6] };
            let mut d = String::new();
            for ring in &rs[i].rings {
                for (j, &(x, y)) in ring.iter().enumerate() { let (px, py) = project(x, y, states); let _ = write!(d, "{}{:.1} {:.1}", if j == 0 { "M" } else { "L" }, ox + (px - bb.0) * k, oy + (py - bb.1) * k); }
                d.push('Z');
            }
            let _ = write!(s, "<path d=\"{d}\" fill=\"#{}\" stroke=\"#FFFFFF\" stroke-width=\"0.6\"/>", hex(fill));
        }
        if !found.is_empty() {
            let (lx, ly, lw) = (w / 2.0 - 90.0, h - 26.0, 180.0);
            let _ = write!(s, "<defs><linearGradient id=\"g\"><stop offset=\"0\" stop-color=\"#{}\"/><stop offset=\"1\" stop-color=\"#{}\"/></linearGradient></defs><rect x=\"{lx}\" y=\"{ly}\" width=\"{lw}\" height=\"8\" fill=\"url(#g)\"/>", hex(self.low), hex(self.high));
            let num = |v: f64| crate::numfmt::format(&Value::Num(v), if v.fract() == 0.0 { "#,##0" } else { "#,##0.00" }, false).text;
            let _ = write!(s, "<text x=\"{lx}\" y=\"{}\" font-family=\"Liberation Sans\" font-size=\"10\" fill=\"#606060\">{}</text><text x=\"{}\" y=\"{}\" font-family=\"Liberation Sans\" font-size=\"10\" text-anchor=\"end\" fill=\"#606060\">{}</text>", ly + 19.0, num(lo), lx + lw, ly + 19.0, num(hi));
        }
        s.push_str("</svg>");
        (s, unknown)
    }

    /// The map as PNG bytes.
    pub fn png(&self, wb: &Workbook, sheet: usize, w: u32, h: u32) -> Option<Vec<u8>> { svg_png(&self.svg(wb, sheet, w as f32, h as f32).0, 2.0) }
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] { let t = t.clamp(0.0, 1.0); [0, 1, 2].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8) }

/// Rasterizes an SVG (text in Liberation Sans) at `scale` into PNG bytes.
pub fn svg_png(svg: &str, scale: f32) -> Option<Vec<u8>> {
    static DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let db = DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        for f in word_engine::fonts::faces().iter().filter(|f| f.family == "Liberation Sans" && f.file != "embedded") { db.load_font_data(f.bytes().to_vec()); }
        Arc::new(db)
    });
    let opt = resvg::usvg::Options { fontdb: db.clone(), font_family: "Liberation Sans".into(), ..Default::default() };
    let tree = resvg::usvg::Tree::from_str(svg, &opt).ok()?;
    let size = tree.size();
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let mut pm = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
    pm.encode_png().ok()
}

/// A region's label point: the area-weighted centroid of its largest ring (lon, lat).
pub fn centroid(i: usize) -> (f32, f32) {
    let r = &regions()[i];
    let area = |ring: &Vec<(f32, f32)>| ring.windows(2).map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1).sum::<f32>() / 2.0;
    let Some(ring) = r.rings.iter().max_by(|a, b| area(a).abs().total_cmp(&area(b).abs())) else { return (0.0, 0.0) };
    let a = area(ring);
    if a.abs() < 1e-6 { let n = ring.len().max(1) as f32; return (ring.iter().map(|p| p.0).sum::<f32>() / n, ring.iter().map(|p| p.1).sum::<f32>() / n); }
    let (mut cx, mut cy) = (0.0, 0.0);
    for w in ring.windows(2) { let f = w[0].0 * w[1].1 - w[1].0 * w[0].1; cx += (w[0].0 + w[1].0) * f; cy += (w[0].1 + w[1].1) * f; }
    (cx / (6.0 * a), cy / (6.0 * a))
}

/// How the map view plots values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlotKind { #[default] Columns, Bubbles, Heat }

/// A point to plot: where, its value and label.
#[derive(Clone, Debug, PartialEq)]
pub struct Point { pub lon: f32, pub lat: f32, pub value: f64, pub label: String }

/// The map view (Jadwal's "3D Map"): countries on an Equal Earth projection centered on
/// (`lon`, `lat`) at `zoom` (1 = whole world), with the points as columns, bubbles or a heat map.
pub fn plot_svg(points: &[Point], kind: PlotKind, lon: f32, lat: f32, zoom: f32, title: &str, w: f32, h: f32) -> String {
    let (cx, cy) = project(lon, lat, false);
    let k = w / 330.0 * zoom.max(0.5);
    let tx = |x: f32, y: f32| -> (f32, f32) { let (px, py) = project(x, y, false); (w / 2.0 + (px - cx) * k, h / 2.0 + (py - cy) * k) };
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"#DCE9F5\"/>");
    for r in regions().iter().filter(|r| !r.us_state) {
        let mut d = String::new();
        for ring in &r.rings { for (j, &(x, y)) in ring.iter().enumerate() { let (px, py) = tx(x, y); let _ = write!(d, "{}{:.1} {:.1}", if j == 0 { "M" } else { "L" }, px, py); } d.push('Z'); }
        let _ = write!(s, "<path d=\"{d}\" fill=\"#F4F1EA\" stroke=\"#C9C2B3\" stroke-width=\"0.5\"/>");
    }
    let max = points.iter().map(|p| p.value.abs()).fold(0.0, f64::max).max(1e-9);
    let mut pts: Vec<&Point> = points.iter().collect();
    pts.sort_by(|a, b| b.lat.total_cmp(&a.lat));
    if kind == PlotKind::Heat { s.push_str("<defs><radialGradient id=\"h\"><stop offset=\"0\" stop-color=\"#E8401C\" stop-opacity=\"0.85\"/><stop offset=\"0.5\" stop-color=\"#F6A91B\" stop-opacity=\"0.45\"/><stop offset=\"1\" stop-color=\"#F6E71B\" stop-opacity=\"0\"/></radialGradient></defs>"); }
    for p in &pts {
        let (x, y) = tx(p.lon, p.lat);
        let t = (p.value.abs() / max) as f32;
        match kind {
            PlotKind::Columns => {
                let (bw, bh) = (6.0 * zoom.sqrt(), 4.0 + 80.0 * t * zoom.sqrt().min(2.0));
                let _ = write!(s, "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"#1F4E79\"/>", x + bw / 2.0, y, x + bw, y - bw / 3.0, x + bw, y - bw / 3.0 - bh, x + bw / 2.0, y - bh);
                let _ = write!(s, "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#2E75B6\"/>", x - bw / 2.0, y - bh, bw, bh);
                let _ = write!(s, "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"#9DC3E6\"/>", x - bw / 2.0, y - bh, x + bw / 2.0, y - bh, x + bw, y - bh - bw / 3.0, x, y - bh - bw / 3.0);
            }
            PlotKind::Bubbles => { let _ = write!(s, "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{:.1}\" fill=\"#ED7D31\" fill-opacity=\"0.6\" stroke=\"#C55A11\" stroke-width=\"1\"/>", 3.0 + 22.0 * t.sqrt() * zoom.sqrt()); }
            PlotKind::Heat => { let _ = write!(s, "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{:.1}\" fill=\"url(#h)\"/>", (10.0 + 40.0 * t) * zoom.sqrt()); }
        }
    }
    if !title.is_empty() { let _ = write!(s, "<rect x=\"10\" y=\"10\" width=\"{:.0}\" height=\"28\" rx=\"3\" fill=\"#FFFFFF\" fill-opacity=\"0.85\"/><text x=\"20\" y=\"29\" font-family=\"Liberation Sans\" font-size=\"15\" fill=\"#303030\">{}</text>", 20.0 + title.chars().count() as f32 * 8.0, crate::xlsx::xml::text(title)); }
    s.push_str("</svg>");
    s
}

/// Points from a range: a header row naming the columns, then rows. Latitude and longitude
/// columns (by header) give positions; otherwise column `loc` names countries or US states.
/// Returns the points and how many rows had no known place.
pub fn points_from(wb: &Workbook, si: usize, r: crate::Range, loc: u32, value: u32) -> (Vec<Point>, usize) {
    let head = |c: u32| wb.display(si, crate::Pos::new(r.r0, c)).to_lowercase();
    let find_col = |names: &[&str]| (r.c0..=r.c1).find(|c| names.contains(&head(*c).trim()));
    let lat_c = find_col(&["lat", "latitude"]);
    let lon_c = find_col(&["lon", "long", "lng", "longitude"]);
    let mut out = vec![];
    let mut missing = 0;
    for row in r.r0 + 1..=r.r1 {
        let num = |c: u32| match wb.sheets[si].value(crate::Pos::new(row, c)) { Value::Num(n) => Some(n), _ => None };
        let label = wb.display(si, crate::Pos::new(row, loc));
        let v = num(value).unwrap_or(0.0);
        let pos = match (lat_c, lon_c) {
            (Some(a), Some(b)) => num(a).zip(num(b)).map(|(la, lo)| (lo as f32, la as f32)),
            _ => find(&label, false).map(centroid),
        };
        match pos { Some((lon, lat)) => out.push(Point { lon, lat, value: v, label }), None => if !label.trim().is_empty() { missing += 1; } }
    }
    (out, missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_load_and_match() {
        let rs = regions();
        assert!(rs.len() > 200);
        assert_eq!(rs[find("France", false).unwrap()].name(), "France");
        assert_eq!(rs[find("usa", false).unwrap()].name(), "United States of America");
        assert_eq!(rs[find("DEU", false).unwrap()].name(), "Germany");
        assert!(rs[find("Texas", false).unwrap()].us_state);
        assert!(rs[find("CA", true).unwrap()].us_state);
        assert!(find("Atlantis", false).is_none());
    }

    #[test]
    fn filled_map_renders_and_round_trips() {
        let mut wb = Workbook::new();
        for (i, (n, v)) in [("Country", ""), ("France", "10"), ("Brazil", "30"), ("Japan", "20"), ("Narnia", "5")].iter().enumerate() {
            wb.set_input(0, crate::Pos::new(i as u32, 0), n);
            wb.set_input(0, crate::Pos::new(i as u32, 1), v);
        }
        let m = FilledMap { source: "Sheet1!$A$1:$B$5".into(), title: "Sales".into(), ..Default::default() };
        assert_eq!(m.rows(&wb, 0).len(), 4);
        let (svg, unknown) = m.svg(&wb, 0, 400.0, 260.0);
        assert_eq!(unknown, 1);
        assert!(svg.contains("#08519C"));
        let png = m.png(&wb, 0, 400, 260).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(FilledMap::from_tag(&m.tag()), Some(m));
        let (pts, missing) = points_from(&wb, 0, crate::addr::parse_range("A1:B5").unwrap(), 0, 1);
        assert_eq!((pts.len(), missing), (3, 1));
        let (lon, lat) = (pts[0].lon, pts[0].lat);
        assert!((-5.0..10.0).contains(&lon) && (42.0..51.0).contains(&lat), "{lon} {lat}");
        for k in [PlotKind::Columns, PlotKind::Bubbles, PlotKind::Heat] { assert!(svg_png(&plot_svg(&pts, k, 0.0, 20.0, 1.0, "Tour", 400.0, 240.0), 1.0).is_some()); }
    }
}
