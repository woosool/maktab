//! Native timelines (MS-XLSX 2.6.2xx, x15): a timeline object (`ObjKind::Timeline`) on a
//! natively written pivot is saved as Excel saves one — a timeline cache part per timeline
//! (xl/timelineCaches, its state: selection, bounds, pivot cache id), a timelines part per sheet
//! (xl/timelines), an `mc:AlternateContent` frame in the drawing naming the timeline
//! (tsle:timeslicer, with a text-box fallback), the workbook's x15 `timelineCacheRefs`
//! extension, the sheet's x15 `timelineRefs` extension and a `#N/A` defined name per cache.
//! The pivot cache writes the field as a date field (pivot_parts). Table timelines, which
//! Excel doesn't have, stay tagged text boxes (timeline.rs). Reading turns frames back into
//! timeline objects when every timeline of the file can be represented; otherwise all timeline
//! parts are carried untouched.

use super::pivot_parts::{iso, parse_iso, parse_part, sheet_rel, Ctx, NS, NS_X15};
use super::read::rels_name;
use super::xml::{attr, Element};
use crate::book::{DefinedName, Workbook};
use crate::drawing::{ObjKind, Object};
use crate::timeline::{Level, Timeline};
use std::collections::{HashMap, HashSet};

pub(super) const RT_TIMELINE: &str = "http://schemas.microsoft.com/office/2011/relationships/timeline";
pub(super) const RT_TIMELINE_CACHE: &str = "http://schemas.microsoft.com/office/2011/relationships/timelineCache";
const CT_TIMELINE: &str = "application/vnd.ms-excel.timeline+xml";
const CT_TIMELINE_CACHE: &str = "application/vnd.ms-excel.timelineCache+xml";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const NS_TSLE: &str = "http://schemas.microsoft.com/office/drawing/2012/timeslicer";
/// Workbook extension listing timeline caches, sheet extension listing timelines parts.
pub(super) const EXT_WB_TIMELINES: &str = "{D0CA8CA8-9F24-4464-BF8E-62219DCF47F9}";
const EXT_SHEET_TIMELINES: &str = "{7E03D99C-DC04-49d9-9315-930204A7B6E9}";

/// The (sheet, index) of the pivot a timeline filters, when it can be written natively.
fn target(w: &Workbook, t: &Timeline) -> Option<(usize, usize)> {
    if t.pivot.is_empty() { return None; }
    let (sheet, index) = w.sheets.iter().enumerate().find_map(|(si, s)| s.pivots.iter().position(|p| p.name == t.pivot).map(|k| (si, k)))?;
    let p = &w.sheets[sheet].pivots[index];
    (super::pivot_parts::writable(w, p) && t.field <= (p.source.c1 - p.source.c0) as usize).then_some((sheet, index))
}

/// The source fields of a pivot that timelines filter (written as date fields).
pub(super) fn pivot_timeline_fields(w: &Workbook, pivot: &str) -> Vec<usize> {
    w.sheets.iter().flat_map(|s| s.objects.iter()).filter_map(|o| match &o.kind { ObjKind::Timeline(t) if t.pivot == pivot => Some(t.field), _ => None }).collect()
}

/// The date filters a pivot's timelines put on it, in the order they get filter ids (1, 2 …):
/// (source field, first day, last day) of each native timeline with a selection.
pub(super) fn pivot_filters(w: &Workbook, pivot: &str) -> Vec<(usize, f64, f64)> {
    w.sheets.iter().flat_map(|s| s.objects.iter()).filter_map(|o| match &o.kind {
        ObjKind::Timeline(t) if t.pivot == pivot && target(w, t).is_some() => t.selection.map(|(a, b)| (t.field, a.floor(), b.floor())),
        _ => None,
    }).collect()
}

/// The pivot table `<filter>` of a timeline selection (MS-XLSX: type dateBetween, evaluated
/// first, an autoFilter with the inclusive day bounds as serials, x15 useWholeDay), so Excel
/// shows the filtered pivot without refreshing.
pub(super) fn filter_xml(id: usize, field: usize, a: f64, b: f64, name: &str) -> String {
    format!("<filter fld=\"{field}\" type=\"dateBetween\" evalOrder=\"-1\" id=\"{id}\" name=\"{}\"><autoFilter ref=\"A1\"><filterColumn colId=\"0\"><customFilters and=\"1\"><customFilter operator=\"greaterThanOrEqual\" val=\"{a}\"/><customFilter operator=\"lessThanOrEqual\" val=\"{b}\"/></customFilters></filterColumn></autoFilter><extLst><ext uri=\"{{0605FD5F-26C8-4aeb-8148-2DB25E43C511}}\" xmlns:x15=\"{NS_X15}\"><x15:pivotFilter useWholeDay=\"1\"/></ext></extLst></filter>", attr(name))
}

/// The timeline cache name of a source field (also the pivot filter's name).
pub(super) fn cache_base(field: &str) -> String {
    let mut base: String = field.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if base.is_empty() { base = "Field".into(); }
    format!("NativeTimeline_{base}")
}

/// Workbook-unique timeline names by (sheet, object index), for those written natively.
fn names(w: &Workbook) -> HashMap<(usize, usize), String> {
    let mut out = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (si, s) in w.sheets.iter().enumerate() {
        for (oi, o) in s.objects.iter().enumerate() {
            let ObjKind::Timeline(t) = &o.kind else { continue };
            if target(w, t).is_none() { continue; }
            let base = if !o.name.trim().is_empty() { o.name.clone() } else if !t.caption.is_empty() { t.caption.clone() } else { "Timeline".into() };
            let mut n = base.clone();
            let mut k = 1;
            while !seen.insert(n.to_lowercase()) { n = format!("{base} {k}"); k += 1; }
            out.insert((si, oi), n);
        }
    }
    out
}

/// The drawing content of a timeline written natively (None: the tagged text box).
pub fn frame_xml(w: &Workbook, si: usize, oi: usize, o: &Object, t: &Timeline) -> Option<String> {
    target(w, t)?;
    let name = names(w).remove(&(si, oi))?;
    let fb = crate::drawing::shape_xml(o, &crate::timeline::fallback_shape(w, t), &crate::timeline::tag(t), (0.0, 0.0));
    Some(format!("<mc:AlternateContent xmlns:mc=\"{NS_MC}\"><mc:Choice xmlns:tsle=\"{NS_TSLE}\" Requires=\"tsle\"><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{}\" name=\"{n}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{NS_TSLE}\"><tsle:timeslicer name=\"{n}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice><mc:Fallback>{fb}</mc:Fallback></mc:AlternateContent>",
        o.id, n = attr(&name)))
}

fn cache_name(field: &str, used: &mut HashSet<String>) -> String {
    let base = cache_base(field);
    let mut n = base.clone();
    let mut k = 1;
    while !used.insert(n.to_lowercase()) { n = format!("{base}{k}"); k += 1; }
    n
}

/// Writes the timeline caches, timelines parts and sheet extensions of the copy `w`
/// (pivot_parts::store, after the pivots); cache relationship ids go to `ctx.timelines`.
pub(super) fn store(w: &mut Workbook, ctx: &mut Ctx) {
    let names = names(w);
    if names.is_empty() { return; }
    let d1904 = w.date1904;
    let mut used: HashSet<String> = w.names.iter().map(|n| n.name.to_lowercase()).collect();
    for si in 0..w.sheets.len() {
        let mut list: Vec<(String, String, Timeline)> = vec![];
        for oi in 0..w.sheets[si].objects.len() {
            let Some(name) = names.get(&(si, oi)).cloned() else { continue };
            let ObjKind::Timeline(t) = w.sheets[si].objects[oi].kind.clone() else { continue };
            let Some((sheet, index)) = target(w, &t) else { continue };
            let Some(&cache_id) = ctx.cache_ids.get(&(sheet, index)) else { continue };
            let p = w.sheets[sheet].pivots[index].clone();
            let source = super::pivot_parts::unique_names(p.fields(w)).get(t.field).cloned().unwrap_or_default();
            let cname = cache_name(&source, &mut used);
            let ds = crate::timeline::dates(w, &t);
            let bounds = match (ds.iter().copied().reduce(f64::min), ds.iter().copied().reduce(f64::max)) {
                (Some(lo), Some(hi)) => { let (a, _, _) = crate::date::ymd(lo, d1904); let (b, _, _) = crate::date::ymd(hi, d1904); format!("<bounds startDate=\"{}\" endDate=\"{}\"/>", iso(crate::date::serial(a, 1, 1, d1904), d1904), iso(crate::date::serial(b + 1, 1, 1, d1904), d1904)) }
                _ => String::new(),
            };
            // The selection's end is its last day (inclusive, as Excel writes it); a selection
            // also names the pivot filter it applies (filterId / filterTabId / filterPivotName).
            let filter = t.selection.and_then(|_| {
                let mine = (si, oi);
                let k = w.sheets.iter().enumerate().flat_map(|(a, s)| s.objects.iter().enumerate().map(move |(b, o)| ((a, b), o)))
                    .filter(|(_, o)| matches!(&o.kind, ObjKind::Timeline(x) if x.pivot == t.pivot && x.selection.is_some() && target(w, x).is_some()))
                    .position(|(at, _)| at == mine)?;
                Some(format!(" filterId=\"{}\" filterTabId=\"{}\" filterPivotName=\"{}\"", k + 1, sheet + 1, attr(&p.name)))
            }).unwrap_or_default();
            let (ft, sel) = match t.selection { Some((a, b)) => ("dateBetween", format!("<selection startDate=\"{}\" endDate=\"{}\"/>", iso(a.floor(), d1904), iso(b.floor(), d1904))), None => ("unknown", String::new()) };
            let x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<timelineCacheDefinition xmlns=\"{NS_X15}\" xmlns:mc=\"{NS_MC}\" mc:Ignorable=\"x\" xmlns:x=\"{NS}\" name=\"{}\" sourceName=\"{}\"><pivotTables><pivotTable tabId=\"{}\" name=\"{}\"/></pivotTables><state minimalRefreshVersion=\"6\" lastRefreshVersion=\"6\" pivotCacheId=\"{cache_id}\" filterType=\"{ft}\"{filter}>{sel}{bounds}</state></timelineCacheDefinition>",
                attr(&cname), attr(&source), sheet + 1, attr(&p.name));
            let cpath = ctx.free("xl/timelineCaches/timelineCache", ".xml");
            w.parts.insert(cpath.clone(), x.into_bytes());
            ctx.overrides.push((cpath.clone(), CT_TIMELINE_CACHE.into()));
            let rid = ctx.wb_rel(RT_TIMELINE_CACHE, &cpath);
            ctx.timelines.push(rid);
            w.names.push(DefinedName { name: cname.clone(), sheet: None, formula: "#N/A".into(), hidden: false, comment: String::new() });
            list.push((name, cname, t));
        }
        if list.is_empty() { continue; }
        let tpath = ctx.free("xl/timelines/timeline", ".xml");
        let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<timelines xmlns=\"{NS_X15}\" xmlns:mc=\"{NS_MC}\" mc:Ignorable=\"x\" xmlns:x=\"{NS}\">");
        for (n, c, t) in &list {
            let scroll = t.selection.map(|(a, _)| format!(" scrollPosition=\"{}\"", iso(a, d1904))).unwrap_or_default();
            x.push_str(&format!("<timeline name=\"{}\" cache=\"{}\" caption=\"{}\" level=\"{l}\" selectionLevel=\"{l}\"{scroll}/>", attr(n), attr(c), attr(if t.caption.is_empty() { n } else { &t.caption }), l = t.level.code()));
        }
        x.push_str("</timelines>");
        w.parts.insert(tpath.clone(), x.into_bytes());
        ctx.overrides.push((tpath.clone(), CT_TIMELINE.into()));
        let rid = sheet_rel(w, si, RT_TIMELINE, &tpath, "rIdTl");
        let ext = format!("<ext uri=\"{EXT_SHEET_TIMELINES}\" xmlns:x15=\"{NS_X15}\"><x15:timelineRefs><x15:timelineRef r:id=\"{rid}\"/></x15:timelineRefs></ext>");
        let s = &mut w.sheets[si];
        let old = s.extra.iter().position(|(k, _)| k == "extLst").map(|i| s.extra.remove(i).1);
        let kept = old.as_deref().map(strip_timeline_exts).unwrap_or_default();
        s.extra.push(("extLst".into(), format!("<extLst>{kept}{ext}</extLst>")));
    }
}

/// The ext elements of a sheet's extLst other than timeline lists, as XML.
fn strip_timeline_exts(raw: &str) -> String {
    let Ok(e) = Element::parse(raw) else { return String::new() };
    e.elements().filter(|x| x.attr("uri") != Some(EXT_SHEET_TIMELINES)).map(|x| x.to_xml()).collect()
}

// ---- reading ---------------------------------------------------------------------------------------

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }

fn caches(wb: &Workbook) -> Vec<(String, Element)> {
    wb.parts.keys().filter(|k| k.starts_with("xl/timelineCaches/") && k.ends_with(".xml") && !k.contains("/_rels/")).filter_map(|k| Some((k.clone(), parse_part(wb, k)?))).collect()
}

/// The timeline a drawing's `mc:AlternateContent` frame shows, if representable.
pub fn kind(wb: &Workbook, si: usize, content: &Element) -> Option<ObjKind> {
    let name = find(content, "graphicData").and_then(|g| find(g, "timeslicer")).and_then(|s| s.attr("name"))?;
    let tl = wb.sheets[si].extra.iter().filter(|(k, _)| k == "rel").filter_map(|(_, v)| {
        let f: Vec<&str> = v.split('\t').collect();
        (f.len() >= 4 && f[1] == RT_TIMELINE && f[3] != "1").then(|| parse_part(wb, f[2])).flatten()
    }).find_map(|e| e.all("timeline").find(|s| s.attr("name") == Some(name)).cloned())?;
    let cache = tl.attr("cache")?;
    let (_, c) = caches(wb).into_iter().find(|(_, c)| c.attr("name") == Some(cache))?;
    let source = c.attr("sourceName")?;
    let pt = c.child("pivotTables").and_then(|p| p.child("pivotTable")).and_then(|t| t.attr("name"))?;
    let p = wb.sheets.iter().flat_map(|s| s.pivots.iter()).find(|p| p.name == pt)?;
    let field = super::pivot_parts::unique_names(p.fields(wb)).iter().position(|n| n.eq_ignore_ascii_case(source))?;
    let d1904 = wb.date1904;
    let selection = c.child("state").and_then(|s| s.child("selection")).and_then(|s| Some((parse_iso(s.attr("startDate")?, d1904)?.floor(), parse_iso(s.attr("endDate")?, d1904)?.floor())));
    Some(ObjKind::Timeline(Timeline { pivot: pt.into(), table: String::new(), field, caption: tl.attr("caption").unwrap_or(name).into(), level: Level::from_code(tl.attr_u32("level").unwrap_or(2) as u8), selection }))
}

/// After drawings are read: when every timeline of the file became a timeline object, the
/// timeline parts, extensions and names go (they are written afresh); otherwise those objects
/// revert to carried content and the parts stay.
pub fn after_load(wb: &mut Workbook) {
    let total: usize = wb.parts.keys().filter(|k| k.starts_with("xl/timelines/") && k.ends_with(".xml") && !k.contains("/_rels/")).filter_map(|k| parse_part(wb, k)).map(|e| e.all("timeline").count()).sum();
    let native = |o: &Object| matches!(o.kind, ObjKind::Timeline(_)) && o.raw.as_ref().is_some_and(|r| r.local() == "AlternateContent");
    let converted: usize = wb.sheets.iter().map(|s| s.objects.iter().filter(|o| native(o)).count()).sum();
    if total == 0 && converted == 0 { return; }
    if converted != total {
        for s in &mut wb.sheets { for o in &mut s.objects { if native(o) { o.kind = ObjKind::Other; } } }
        return;
    }
    for s in &mut wb.sheets { for o in &mut s.objects { if native(o) { o.raw = None; } } }
    let cache_names: HashSet<String> = caches(wb).iter().filter_map(|(_, c)| c.attr("name").map(String::from)).collect();
    let gone: Vec<String> = wb.parts.keys().filter(|k| k.starts_with("xl/timelines/") || k.starts_with("xl/timelineCaches/")).cloned().collect();
    for k in gone { wb.parts.remove(&rels_name(&k)); wb.parts.remove(&k); }
    for s in &mut wb.sheets {
        s.extra.retain(|(k, v)| !(k == "rel" && v.split('\t').nth(1) == Some(RT_TIMELINE)));
        if let Some(i) = s.extra.iter().position(|(k, _)| k == "extLst") {
            let kept = strip_timeline_exts(&s.extra[i].1);
            if kept.is_empty() { s.extra.remove(i); } else { s.extra[i].1 = format!("<extLst>{kept}</extLst>"); }
        }
    }
    wb.names.retain(|n| !(n.sheet.is_none() && cache_names.contains(&n.name)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::Pos;
    use crate::drawing::{add_object, Anchor, Mark};
    use crate::xlsx::pkgcheck::{check_package, unzip};

    #[test]
    fn pivot_timeline_native_round_trip() {
        let mut wb = Workbook::new();
        wb.set_input(0, Pos::new(0, 0), "Date");
        wb.set_input(0, Pos::new(0, 1), "Amount");
        for (i, d) in ["1/15/2024", "2/10/2024", "3/5/2024", "7/1/2024", "1/2/2025"].iter().enumerate() {
            wb.set_input(0, Pos::new(i as u32 + 1, 0), d);
            wb.set_input(0, Pos::new(i as u32 + 1, 1), &((i + 1) * 10).to_string());
        }
        let mut p = crate::pivot::Pivot::new("PivotTable1", "Sheet1", crate::addr::parse_range("A1:B6").unwrap(), Pos::new(0, 4));
        p.values.push(crate::pivot::ValueField { field: 1, ..Default::default() });
        wb.sheets[0].pivots.push(p);
        let ps = crate::timeline::periods(&wb, &Timeline { pivot: "PivotTable1".into(), ..Default::default() });
        let t = Timeline { pivot: "PivotTable1".into(), field: 0, caption: "Date".into(), level: Level::Months, selection: Some((ps[0].start, ps[1].end)), ..Default::default() };
        let anchor = Anchor::Two { from: Mark { col: 8, row: 1, ..Default::default() }, to: Mark { col: 14, row: 8, ..Default::default() }, edit_as: None };
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Timeline(t.clone()), anchor), "Timeline");
        crate::timeline::sync_pivots(&mut wb);
        let bytes = crate::xlsx::write(&wb).unwrap();
        if let Ok(dir) = std::env::var("JADWAL_TEST_OUT") { let _ = std::fs::write(format!("{dir}/timeline.xlsx"), &bytes); }
        let files = unzip(&bytes);
        check_package(&files);
        let c = &files["xl/timelineCaches/timelineCache1.xml"];
        assert!(c.contains("sourceName=\"Date\"") && c.contains("filterType=\"dateBetween\"") && c.contains("startDate=\"2024-01-01T00:00:00\" endDate=\"2024-02-29T00:00:00\"") && c.contains("filterId=\"1\" filterTabId=\"1\" filterPivotName=\"PivotTable1\""), "{c}");
        let pt = &files["xl/pivotTables/pivotTable1.xml"];
        assert!(pt.contains("<filters count=\"1\"><filter fld=\"0\" type=\"dateBetween\" evalOrder=\"-1\" id=\"1\" name=\"NativeTimeline_Date\"") && pt.contains("operator=\"lessThanOrEqual\" val=\"45351\""), "{pt}");
        assert!(files["xl/timelines/timeline1.xml"].contains("level=\"2\""));
        assert!(files["xl/workbook.xml"].contains("x15:timelineCacheRef ") && files["xl/workbook.xml"].contains("NativeTimeline_Date"));
        assert!(files["xl/worksheets/sheet1.xml"].contains("x15:timelineRef "));
        assert!(files["xl/drawings/drawing1.xml"].contains("tsle:timeslicer"));
        assert!(files["xl/pivotCache/pivotCacheDefinition1.xml"].contains("containsDate=\"1\""));
        assert!(files["[Content_Types].xml"].contains(CT_TIMELINE_CACHE) && files["[Content_Types].xml"].contains(CT_TIMELINE));
        let back = crate::xlsx::read(&bytes).unwrap();
        let tl = back.sheets[0].objects.iter().find_map(|o| if let ObjKind::Timeline(t) = &o.kind { Some(t.clone()) } else { None }).expect("timeline");
        assert_eq!(tl, t);
        assert!(!back.names.iter().any(|n| n.name.starts_with("NativeTimeline")));
        // Saved again: one cache, one name.
        let again = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&again);
        assert_eq!(again["xl/workbook.xml"].matches("<x15:timelineCacheRef ").count(), 1);
        assert_eq!(again["xl/workbook.xml"].matches("NativeTimeline_Date<").count() + again["xl/workbook.xml"].matches("name=\"NativeTimeline_Date\"").count(), 1);
    }
}
