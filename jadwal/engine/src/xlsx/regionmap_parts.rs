//! Filled maps (geo.rs) as Excel's region map charts: a chartex part whose series has
//! layoutId="regionMap", with the region names and values ([MS-ODRAWXML] 2.24.3), the color
//! scale (valueColors), and a geography cache holding the regions' locations. The drawing frame
//! is an `mc:AlternateContent` whose fallback is the map picture Jadwal renders. Reading turns
//! such a frame back into the filled map picture (geo::FilledMap tag in its description).

use super::native_objects::Sink;
use super::xml::{attr, part_text, text, Element};
use crate::book::Workbook;
use crate::chart::{make_ref, parse_ref};
use crate::drawing::{ObjKind, Object};
use crate::geo::FilledMap;
use crate::value::Value;
use crate::{Pos, Range};
use std::sync::Arc;

const NS_CX: &str = crate::chart::ex::NS_CX;
const NS_CX4: &str = "http://schemas.microsoft.com/office/drawing/2016/5/10/chartex";
const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const BING: &str = "{E9337A44-BEBE-4D9F-B70C-5C5E7DAFC167}";

fn hex(c: [u8; 3]) -> String { format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }
fn unhex(s: &str) -> Option<[u8; 3]> { let v = u32::from_str_radix(s.get(..6)?, 16).ok()?; Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]) }
/// A ring in Bing Maps' point compression (the encoding of CT_GeoPolygon@pcaRings): latitude
/// and longitude at 1e-5 degrees, zig-zag deltas, Cantor-paired, base-64 digits low first.
fn pca(ring: &[(f32, f32)]) -> String {
    const SAFE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    let (mut plat, mut plon) = (0i64, 0i64);
    let mut out = String::new();
    for &(lon, lat) in ring {
        let (la, lo) = ((lat as f64 * 1e5).round() as i64, (lon as f64 * 1e5).round() as i64);
        let (dy, dx) = (la - plat, lo - plon);
        (plat, plon) = (la, lo);
        let (dy, dx) = ((dy << 1) ^ (dy >> 63), (dx << 1) ^ (dx >> 63));
        let mut index = ((dy + dx) * (dy + dx + 1) / 2) + dy;
        loop {
            let mut rem = index & 31;
            index = (index - rem) / 32;
            if index > 0 { rem += 32; }
            out.push(SAFE[rem as usize] as char);
            if index == 0 { break; }
        }
    }
    out
}

fn num(n: f64) -> String { if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") } }

/// (sheet index, sheet name, range, header row present) of a map's source.
fn source(wb: &Workbook, si: usize, m: &FilledMap) -> Option<(usize, String, Range, bool)> {
    let (sh, r) = parse_ref(&m.source)?;
    let s = sh.and_then(|n| wb.sheet_index(&n)).unwrap_or(si);
    let header = !matches!(wb.sheets[s].value(Pos::new(r.r0, r.c1)), Value::Num(_)) && r.r1 > r.r0;
    Some((s, wb.sheets[s].name.clone(), r, header))
}

/// The chartex part of a filled map.
pub fn chart_xml(wb: &Workbook, si: usize, m: &FilledMap) -> Option<String> {
    let (s, name, r, header) = source(wb, si, m)?;
    let r0 = r.r0 + header as u32;
    let rows: Vec<u32> = (r0..=r.r1).collect();
    let cat_f = make_ref(&name, Range { r0, c0: r.c0, r1: r.r1, c1: r.c0 });
    let val_f = make_ref(&name, Range { r0, c0: r.c1, r1: r.r1, c1: r.c1 });
    let mut cats = String::new();
    let mut vals = String::new();
    let mut names = vec![];
    for (i, row) in rows.iter().enumerate() {
        let n = wb.display(s, Pos::new(*row, r.c0));
        if !n.trim().is_empty() { cats.push_str(&format!("<cx:pt idx=\"{i}\">{}</cx:pt>", text(&n))); names.push(n); }
        if let Value::Num(v) = wb.sheets[s].value(Pos::new(*row, r.c1)) { vals.push_str(&format!("<cx:pt idx=\"{i}\">{}</cx:pt>", num(v))); }
    }
    let n = rows.len();
    let tx = if header {
        let h = wb.display(s, Pos::new(r.r0, r.c1));
        format!("<cx:tx><cx:txData><cx:f>{}</cx:f><cx:v>{}</cx:v></cx:txData></cx:tx>", text(&make_ref(&name, Range { r0: r.r0, c0: r.c1, r1: r.r0, c1: r.c1 })), text(&h))
    } else { String::new() };
    // The geography cache ([MS-ODRAWXML] CT_GeoCache, uncompressed `clear` form): for each known
    // region, the location query result (where the name resolves), a data-point-to-entity result
    // (that location → the region's entity) and the entity's data (bounds and outline polygons,
    // Bing point-compressed). Entity ids are Jadwal's own ("jadwal-geo-<n>"), not Bing's: Excel
    // draws from the cache offline, and replaces it from Bing when it refreshes the map online.
    let states = names.iter().filter(|x| crate::geo::find(x, true).is_some()).count() * 2 > names.len();
    let (mut locs, mut points, mut ents) = (String::new(), String::new(), String::new());
    let mut seen = std::collections::HashSet::new();
    for nm in &names {
        let Some(i) = crate::geo::find(nm, states) else { continue };
        if !seen.insert(i) { continue; }
        let reg = &crate::geo::regions()[i];
        let (lon, lat) = crate::geo::centroid(i);
        let et = if reg.us_state { "AdminDistrict" } else { "CountryRegion" };
        let q = if reg.us_state { format!(" countryRegion=\"United States\" adminDistrict1=\"{}\"", attr(nm)) } else { format!(" countryRegion=\"{}\"", attr(nm)) };
        let country = if reg.us_state { "United States".to_string() } else { reg.name().to_string() };
        let admin = if reg.us_state { format!(" adminDistrict1=\"{}\"", attr(reg.name())) } else { String::new() };
        let id = format!("jadwal-geo-{i}");
        locs.push_str(&format!("<cx:geoLocationQueryResult><cx:geoLocationQuery{q} entityType=\"{et}\"/><cx:geoLocations><cx:geoLocation latitude=\"{lat:.5}\" longitude=\"{lon:.5}\" entityName=\"{}\" entityType=\"{et}\"><cx:address{admin} countryRegion=\"{}\"/></cx:geoLocation></cx:geoLocations></cx:geoLocationQueryResult>", attr(reg.name()), attr(&country)));
        points.push_str(&format!("<cx:geoDataPointToEntityQueryResult><cx:geoDataPointQuery entityType=\"{et}\" latitude=\"{lat:.5}\" longitude=\"{lon:.5}\"/><cx:geoDataPointToEntityQuery entityType=\"{et}\" entityId=\"{id}\"/></cx:geoDataPointToEntityQueryResult>"));
        let all = reg.rings.iter().flatten();
        let (w, e) = all.clone().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (so, no) = all.fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let polys: String = reg.rings.iter().enumerate().filter(|(_, r)| r.len() >= 3).map(|(k, r)| format!("<cx:geoPolygon polygonId=\"{}\" numPoints=\"{}\" pcaRings=\"{}\"/>", k + 1, r.len(), pca(r))).collect();
        let polys = if polys.is_empty() { String::new() } else { format!("<cx:geoPolygons>{polys}</cx:geoPolygons>") };
        ents.push_str(&format!("<cx:geoDataEntityQueryResult><cx:geoDataEntityQuery entityType=\"{et}\" entityId=\"{id}\"/><cx:geoData entityName=\"{}\" entityId=\"{id}\" east=\"{e}\" west=\"{w}\" north=\"{no}\" south=\"{so}\">{polys}<cx:copyrights><cx:copyright>Natural Earth (public domain)</cx:copyright></cx:copyrights></cx:geoData></cx:geoDataEntityQueryResult>", attr(reg.name())));
    }
    let cache = if locs.is_empty() { String::new() } else {
        format!("<cx:geoCache provider=\"{BING}\"><cx:clear><cx:geoLocationQueryResults>{locs}</cx:geoLocationQueryResults><cx:geoDataEntityQueryResults>{ents}</cx:geoDataEntityQueryResults><cx:geoDataPointToEntityQueryResults>{points}</cx:geoDataPointToEntityQueryResults></cx:clear></cx:geoCache>")
    };
    let title = if m.title.is_empty() { String::new() } else { format!("<cx:title pos=\"t\" align=\"ctr\" overlay=\"0\"><cx:tx><cx:txData><cx:v>{}</cx:v></cx:txData></cx:tx></cx:title>", text(&m.title)) };
    Some(format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cx:chartSpace xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:cx=\"{NS_CX}\">\
<cx:chartData><cx:data id=\"0\"><cx:strDim type=\"cat\"><cx:f>{}</cx:f><cx:lvl ptCount=\"{n}\">{cats}</cx:lvl></cx:strDim><cx:numDim type=\"colorVal\"><cx:f>{}</cx:f><cx:lvl ptCount=\"{n}\" formatCode=\"General\">{vals}</cx:lvl></cx:numDim></cx:data></cx:chartData>\
<cx:chart>{title}<cx:plotArea><cx:plotAreaRegion><cx:series layoutId=\"regionMap\" uniqueId=\"{{A1B2C3D4-0000-4000-8000-4A6164776C4D}}\">{tx}\
<cx:valueColors><cx:minColor><a:srgbClr val=\"{}\"/></cx:minColor><cx:maxColor><a:srgbClr val=\"{}\"/></cx:maxColor></cx:valueColors><cx:valueColorPositions count=\"2\"/>\
<cx:dataId val=\"0\"/><cx:layoutPr><cx:regionLabelLayout val=\"none\"/><cx:geography viewedRegionType=\"dataOnly\" cultureLanguage=\"en-US\" cultureRegion=\"US\" attribution=\"Powered by Bing\">{cache}</cx:geography></cx:layoutPr></cx:series></cx:plotAreaRegion></cx:plotArea>\
<cx:legend pos=\"r\" align=\"min\" overlay=\"0\"/></cx:chart></cx:chartSpace>",
        text(&cat_f), text(&val_f), hex(m.low), hex(m.high)))
}

/// The drawing content for a filled map picture: the region map frame with the picture as fallback.
pub fn write(w: &mut Workbook, si: usize, o: &Object, sink: &mut Sink) -> Option<String> {
    let ObjKind::Picture { data, ext } = &o.kind else { return None };
    let m = FilledMap::from_tag(&o.descr)?;
    let xml = chart_xml(w, si, &m)?;
    let cpath = (sink.free)("xl/charts/chartEx", ".xml");
    w.parts.insert(cpath.clone(), xml.into_bytes());
    sink.overrides.push((cpath.clone(), crate::chart::ex::CT.into()));
    let crid = (sink.add_rel)(crate::chart::ex::RT, cpath, false);
    let ipath = (sink.free)("xl/media/image", &format!(".{ext}"));
    w.parts.insert(ipath.clone(), (**data).clone());
    sink.defaults.push((ext.clone(), if ext == "png" { "image/png".into() } else { format!("image/{ext}") }));
    let irid = (sink.add_rel)(RT_IMAGE, ipath, false);
    let (id, n, d) = (o.id, attr(&o.name), attr(&o.descr));
    Some(format!("<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:cx4=\"{NS_CX4}\" Requires=\"cx4\"><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{n}\" descr=\"{d}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chart xmlns:cx=\"{NS_CX}\" r:id=\"{crid}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice>\
<mc:Fallback><xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"{n}\" descr=\"{d}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{irid}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic></mc:Fallback></mc:AlternateContent>"))
}

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }
fn rid(e: &Element) -> Option<String> { e.attrs.iter().find(|(k, _)| k.contains(':') && !k.starts_with("xmlns") && (k.ends_with(":id") || k.ends_with(":embed"))).map(|(_, v)| v.clone()) }

/// The filled map settings of a region map chartex part.
pub fn parse(xml: &str, wb: &Workbook) -> Option<FilledMap> {
    let root = Element::parse(xml).ok()?;
    let series = find(&root, "series").filter(|s| s.attr("layoutId") == Some("regionMap"))?;
    let data = root.child("chartData")?.child("data")?;
    let f_of = |l: &str| data.elements().find(|d| d.local() == l).and_then(|d| d.child("f")).map(|f| f.text());
    let (sh, cr) = parse_ref(&f_of("strDim")?)?;
    let (_, vr) = parse_ref(&f_of("numDim")?)?;
    let header = series.child("tx").and_then(|t| find(t, "f")).and_then(|f| parse_ref(&f.text())).map(|(_, r)| r.r0);
    let sheet = sh.unwrap_or_else(|| wb.sheets.first().map(|s| s.name.clone()).unwrap_or_default());
    let r = Range { r0: header.unwrap_or(cr.r0).min(cr.r0), c0: cr.c0, r1: cr.r1.max(vr.r1), c1: vr.c0 };
    let d = FilledMap::default();
    let color = |l: &str| series.child("valueColors").and_then(|v| v.child(l)).and_then(|c| find(c, "srgbClr")).and_then(|c| c.attr("val")).and_then(unhex);
    let title = root.child("chart").and_then(|c| c.child("title")).and_then(|t| find(t, "v")).map(|v| v.text()).unwrap_or_default();
    Some(FilledMap { source: make_ref(&sheet, r), title, low: color("minColor").unwrap_or(d.low), high: color("maxColor").unwrap_or(d.high) })
}

/// A drawing frame naming a region map chart → the filled map picture.
pub fn read(wb: &Workbook, si: usize, content: &Element, o: &mut Object) -> bool {
    if content.local() != "AlternateContent" { return false; }
    let Some(choice) = content.child("Choice") else { return false };
    let Some(cx) = find(choice, "chart").filter(|c| c.attrs.iter().any(|(_, v)| v == NS_CX) || c.name.starts_with("cx")) else { return false };
    let target = |id: &str| o.rels.iter().find(|r| r.id == id && !r.external).map(|r| r.target.clone());
    let Some(part) = rid(cx).and_then(|i| target(&i)) else { return false };
    let Some(m) = wb.parts.get(&part).and_then(|b| parse(&part_text(b), wb)) else { return false };
    let img = content.child("Fallback").and_then(|f| find(f, "blip")).and_then(rid).and_then(|i| target(&i)).and_then(|t| wb.parts.get(&t).map(|b| (t.clone(), b.clone())));
    let (data, ext) = match img {
        Some((t, b)) if !b.is_empty() => (b, t.rsplit('.').next().unwrap_or("png").to_ascii_lowercase()),
        _ => (m.png(wb, si, 640, 400).unwrap_or_default(), "png".into()),
    };
    if data.is_empty() { return false; }
    o.kind = ObjKind::Picture { data: Arc::new(data), ext };
    o.descr = m.tag();
    o.raw = None;
    o.rels.clear();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{add_object, Anchor, Mark};

    #[test]
    fn point_compression_matches_bing() {
        // Bing Maps' documented example points (35.894309, -110.72522), (35.893931, -110.72578),
        // (35.893745, -110.72606) start the documented encoding "vx1vilihnM6hR7mE…".
        assert_eq!(pca(&[(-110.72522, 35.89431), (-110.72578, 35.89393), (-110.72606, 35.89374)]), "vx1vilihnM6hR7mE");
    }

    #[test]
    fn filled_map_round_trips_as_region_map() {
        let mut wb = Workbook::new();
        for (i, (n, v)) in [("State", ""), ("Texas", "5"), ("Ohio", "3"), ("Utah", "8")].iter().enumerate() {
            wb.set_input(0, Pos::new(i as u32, 0), n);
            wb.set_input(0, Pos::new(i as u32, 1), if i == 0 { "Sales" } else { v });
        }
        wb.recalc();
        let m = FilledMap { source: "Sheet1!$A$1:$B$4".into(), title: "Sales by state".into(), low: [1, 2, 3], high: [200, 100, 50] };
        let png = m.png(&wb, 0, 160, 100).unwrap();
        let mut o = Object::new(ObjKind::Picture { data: Arc::new(png.clone()), ext: "png".into() }, Anchor::Two { from: Mark { col: 3, ..Default::default() }, to: Mark { col: 9, row: 15, ..Default::default() }, edit_as: None });
        o.descr = m.tag();
        add_object(&mut wb.sheets[0].objects, o, "Map");
        let bytes = crate::xlsx::write(&wb).unwrap();
        let files = super::super::pkgcheck::unzip(&bytes);
        super::super::pkgcheck::check_package(&files);
        let cx = &files["xl/charts/chartEx1.xml"];
        assert!(cx.contains("layoutId=\"regionMap\"") && cx.contains("adminDistrict1=\"Texas\"") && cx.contains("entityType=\"AdminDistrict\"") && cx.contains("<cx:geoPolygon ") && cx.contains("<cx:geoDataPointToEntityQuery ") && cx.contains("Sheet1!$A$2:$A$4"), "{cx}");
        let d = files.iter().find(|(k, _)| k.starts_with("xl/drawings/drawing")).unwrap().1;
        assert!(d.contains("Requires=\"cx4\"") && d.contains("<mc:Fallback><xdr:pic>"));
        if let Ok(dir) = std::env::var("JADWAL_DUMP") { std::fs::write(format!("{dir}/regionmap.xlsx"), &bytes).unwrap(); }
        let back = crate::xlsx::read(&bytes).unwrap();
        let o = &back.sheets[0].objects[0];
        assert!(matches!(&o.kind, ObjKind::Picture { data, .. } if data.as_slice() == png.as_slice()));
        assert_eq!(FilledMap::from_tag(&o.descr), Some(m.clone()));
        // Saved again: still one region map.
        let again = super::super::pkgcheck::unzip(&crate::xlsx::write(&back).unwrap());
        super::super::pkgcheck::check_package(&again);
        assert_eq!(again.keys().filter(|k| k.starts_with("xl/charts/chartEx") && !k.contains("_rels")).count(), 1);
    }
}
