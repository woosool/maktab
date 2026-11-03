//! Native slicers: a slicer object (drawing.rs, `ObjKind::Slicer`) on a natively written pivot
//! or on a table is saved as Excel saves one — a slicer cache part per slicer
//! (xl/slicerCaches), a slicers part per sheet (xl/slicers), an `mc:AlternateContent` graphic
//! frame in the drawing naming the slicer, the workbook's x14/x15 `slicerCaches` extensions,
//! the sheet's x14 `slicerList` extensions and a `#N/A` defined name per cache. Reading turns
//! such frames back into slicer objects when every slicer of the file can be represented;
//! otherwise all slicer parts are carried untouched.

use super::pivot_parts::{parse_part, sheet_rel, Ctx, NS, NS_X14, NS_X15, RT_SLICER, RT_SLICER_CACHE};
use super::read::rels_name;
use super::xml::{attr, text, Element, Node};
use crate::book::{DefinedName, Workbook};
use crate::drawing::{ObjKind, Object, Slicer};
use std::collections::{HashMap, HashSet};

const CT_SLICER: &str = "application/vnd.ms-excel.slicer+xml";
const CT_SLICER_CACHE: &str = "application/vnd.ms-excel.slicerCache+xml";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const NS_SLE: &str = "http://schemas.microsoft.com/office/drawing/2010/slicer";
/// Sheet extensions listing slicers: on pivots (x14) and on tables (x15).
const EXT_SHEET_SLICERS: &str = "{A8765BA9-456A-4dab-B4F3-ACF838C121DE}";
const EXT_SHEET_TABLE_SLICERS: &str = "{3A4CF648-6AED-40f4-86FF-DC5316D8AED3}";

/// What a slicer filters, when it can be written natively.
enum Target { Pivot { sheet: usize, index: usize }, Table { sheet: usize, index: usize } }

fn target(w: &Workbook, sl: &Slicer) -> Option<Target> {
    if sl.pivot.is_empty() {
        let (sheet, index) = w.find_table(&sl.table)?;
        return (sl.field < w.sheets[sheet].tables[index].columns.len()).then_some(Target::Table { sheet, index });
    }
    let (sheet, index) = w.sheets.iter().enumerate().find_map(|(si, s)| s.pivots.iter().position(|p| p.name == sl.pivot).map(|k| (si, k)))?;
    let p = &w.sheets[sheet].pivots[index];
    (super::pivot_parts::writable(w, p) && sl.field <= (p.source.c1 - p.source.c0) as usize).then_some(Target::Pivot { sheet, index })
}

/// The source fields of a pivot that slicers filter.
pub(super) fn pivot_slicer_fields(w: &Workbook, pivot: &str) -> Vec<usize> {
    w.sheets.iter().flat_map(|s| s.objects.iter()).filter_map(|o| match &o.kind { ObjKind::Slicer(sl) if sl.pivot == pivot => Some(sl.field), _ => None }).collect()
}

/// Workbook-unique slicer names by (sheet, object index), for the slicers written natively.
fn names(w: &Workbook) -> HashMap<(usize, usize), String> {
    let mut out = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (si, s) in w.sheets.iter().enumerate() {
        for (oi, o) in s.objects.iter().enumerate() {
            let ObjKind::Slicer(sl) = &o.kind else { continue };
            if target(w, sl).is_none() { continue; }
            let base = if !o.name.trim().is_empty() { o.name.clone() } else if !sl.caption.is_empty() { sl.caption.clone() } else { "Slicer".into() };
            let mut n = base.clone();
            let mut k = 1;
            while !seen.insert(n.to_lowercase()) { n = format!("{base} {k}"); k += 1; }
            out.insert((si, oi), n);
        }
    }
    out
}

/// The drawing content of a slicer written natively (None: write the text-box fallback).
pub fn frame_xml(w: &Workbook, si: usize, oi: usize, o: &Object, sl: &Slicer) -> Option<String> {
    let t = target(w, sl)?;
    let name = names(w).remove(&(si, oi))?;
    let req = match t { Target::Pivot { .. } => "xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\" Requires=\"a14\"", Target::Table { .. } => "xmlns:sle15=\"http://schemas.microsoft.com/office/drawing/2012/slicer\" Requires=\"sle15\"" };
    let items: String = crate::drawing::slicer_items(w, sl).into_iter().map(|(i, on)| format!("<a:p><a:r><a:rPr lang=\"en-US\" sz=\"1000\"/><a:t>{} {}</a:t></a:r></a:p>", if on { "\u{2611}" } else { "\u{2610}" }, text(&i))).collect();
    Some(format!("<mc:AlternateContent xmlns:mc=\"{NS_MC}\"><mc:Choice {req}><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{n}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{NS_SLE}\"><sle:slicer xmlns:sle=\"{NS_SLE}\" name=\"{n}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice>\
<mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"{n}\"/><xdr:cNvSpPr txBox=\"1\"/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"BFBFBF\"/></a:solidFill></a:ln></xdr:spPr>\
<xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\" wrap=\"square\" rtlCol=\"0\" anchor=\"t\"/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1000\" b=\"1\"/><a:t>{cap}</a:t></a:r></a:p>{items}</xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent>",
        id = o.id, n = attr(&name), cap = text(&sl.caption)))
}

fn cache_name(field: &str, used: &mut HashSet<String>) -> String {
    let mut base: String = field.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if base.is_empty() { base = "Field".into(); }
    let base = format!("Slicer_{base}");
    let mut n = base.clone();
    let mut k = 1;
    while !used.insert(n.to_lowercase()) { n = format!("{base}{k}"); k += 1; }
    n
}

/// Writes the slicer caches, slicers parts and extensions of the copy `w` (pivot_parts::store).
pub(super) fn store(w: &mut Workbook, ctx: &mut Ctx) {
    let names = names(w);
    if names.is_empty() { return; }
    let mut used: HashSet<String> = w.names.iter().map(|n| n.name.to_lowercase()).collect();
    for si in 0..w.sheets.len() {
        // (slicer name, cache name, caption) for pivot slicers and table slicers.
        let mut lists: [Vec<(String, String, String)>; 2] = [vec![], vec![]];
        for oi in 0..w.sheets[si].objects.len() {
            let Some(name) = names.get(&(si, oi)).cloned() else { continue };
            let ObjKind::Slicer(sl) = w.sheets[si].objects[oi].kind.clone() else { continue };
            let Some(t) = target(w, &sl) else { continue };
            let (xml, table) = match t {
                Target::Pivot { sheet, index } => {
                    let p = w.sheets[sheet].pivots[index].clone();
                    let Some(&cache_id) = ctx.cache_ids.get(&(sheet, index)) else { continue };
                    let Some(items) = super::pivot_parts::field_items(w, &p, sl.field) else { continue };
                    let source = super::pivot_parts::unique_names(p.fields(w)).get(sl.field).cloned().unwrap_or_default();
                    let cname = cache_name(&source, &mut used);
                    let h = p.hidden.get(&sl.field);
                    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<slicerCacheDefinition xmlns=\"{NS_X14}\" xmlns:mc=\"{NS_MC}\" mc:Ignorable=\"x\" xmlns:x=\"{NS}\" name=\"{}\" sourceName=\"{}\"><pivotTables><pivotTable tabId=\"{}\" name=\"{}\"/></pivotTables><data><tabular pivotCacheId=\"{cache_id}\"><items count=\"{}\">",
                        attr(&cname), attr(&source), sheet + 1, attr(&p.name), items.sorted.len());
                    for s in &items.sorted { x.push_str(&format!("<i x=\"{s}\" s=\"{}\"/>", !h.is_some_and(|h| h.contains(&items.texts[*s])) as u8)); }
                    x.push_str("</items></tabular></data></slicerCacheDefinition>");
                    ((x, cname), false)
                }
                Target::Table { sheet, index } => {
                    let tb = &w.sheets[sheet].tables[index];
                    let source = tb.columns[sl.field].name.clone();
                    let table_id = w.sheets[..sheet].iter().map(|s| s.tables.len()).sum::<usize>() + index + 1;
                    let cname = cache_name(&source, &mut used);
                    let x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<slicerCacheDefinition xmlns=\"{NS_X14}\" xmlns:mc=\"{NS_MC}\" mc:Ignorable=\"x x15\" xmlns:x=\"{NS}\" xmlns:x15=\"{NS_X15}\" name=\"{}\" sourceName=\"{}\"><extLst><x:ext uri=\"{{2F2917AC-EB37-4324-AD4E-5DD8C200BD13}}\"><x15:tableSlicerCache tableId=\"{table_id}\" column=\"{}\"/></x:ext></extLst></slicerCacheDefinition>",
                        attr(&cname), attr(&source), sl.field + 1);
                    ((x, cname), true)
                }
            };
            let ((xml, cname), table) = (xml, table);
            let cpath = ctx.free("xl/slicerCaches/slicerCache", ".xml");
            w.parts.insert(cpath.clone(), xml.into_bytes());
            ctx.overrides.push((cpath.clone(), CT_SLICER_CACHE.into()));
            let rid = ctx.wb_rel(RT_SLICER_CACHE, &cpath);
            if table { ctx.x15.push(rid) } else { ctx.x14.push(rid) }
            w.names.push(DefinedName { name: cname.clone(), sheet: None, formula: "#N/A".into(), hidden: false, comment: String::new() });
            lists[table as usize].push((name, cname, sl.caption.clone()));
        }
        let mut exts = String::new();
        for (k, list) in lists.iter().enumerate() {
            if list.is_empty() { continue; }
            let spath = ctx.free("xl/slicers/slicer", ".xml");
            let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<slicers xmlns=\"{NS_X14}\" xmlns:mc=\"{NS_MC}\" mc:Ignorable=\"x\" xmlns:x=\"{NS}\">");
            for (n, c, cap) in list { x.push_str(&format!("<slicer name=\"{}\" cache=\"{}\" caption=\"{}\" rowHeight=\"241300\"/>", attr(n), attr(c), attr(if cap.is_empty() { n } else { cap }))); }
            x.push_str("</slicers>");
            w.parts.insert(spath.clone(), x.into_bytes());
            ctx.overrides.push((spath.clone(), CT_SLICER.into()));
            let rid = sheet_rel(w, si, RT_SLICER, &spath, "rIdSl");
            if k == 0 { exts.push_str(&format!("<ext uri=\"{EXT_SHEET_SLICERS}\" xmlns:x14=\"{NS_X14}\"><x14:slicerList><x14:slicer r:id=\"{rid}\"/></x14:slicerList></ext>")); }
            else { exts.push_str(&format!("<ext uri=\"{EXT_SHEET_TABLE_SLICERS}\" xmlns:x15=\"{NS_X15}\"><x14:slicerList xmlns:x14=\"{NS_X14}\"><x14:slicer r:id=\"{rid}\"/></x14:slicerList></ext>")); }
        }
        if exts.is_empty() { continue; }
        let s = &mut w.sheets[si];
        let old = s.extra.iter().position(|(k, _)| k == "extLst").map(|i| s.extra.remove(i).1);
        let kept = old.as_deref().map(strip_slicer_exts).unwrap_or_default();
        s.extra.push(("extLst".into(), format!("<extLst>{kept}{exts}</extLst>")));
    }
}

/// The ext elements of a sheet's extLst other than slicer lists, as XML.
fn strip_slicer_exts(raw: &str) -> String {
    let Ok(e) = Element::parse(raw) else { return String::new() };
    e.elements().filter(|x| !matches!(x.attr("uri"), Some(EXT_SHEET_SLICERS | EXT_SHEET_TABLE_SLICERS))).map(|x| x.to_xml()).collect()
}

/// The workbook extLst for the package writer: the slicer cache lists of the reference workbook
/// part whose relationships survive (other extensions are not carried).
pub fn workbook_ext(orig: Option<&Element>, used: &HashSet<String>) -> String {
    let Some(el) = orig.and_then(|w| w.child("extLst")) else { return String::new() };
    let root_ns: Vec<(String, String)> = orig.unwrap().attrs.iter().filter(|(k, _)| k.starts_with("xmlns:") && k != "xmlns:r").cloned().collect();
    let mut out = String::new();
    for ext in el.all("ext") {
        if !matches!(ext.attr("uri"), Some(super::pivot_parts::EXT_WB_SLICERS | super::pivot_parts::EXT_WB_TABLE_SLICERS | super::timeline_parts::EXT_WB_TIMELINES)) { continue; }
        let mut e = super::styles::strip_prefix(ext);
        let mut any = false;
        for c in e.children.iter_mut() {
            let Node::Element(list) = c else { continue };
            list.children.retain(|n| match n { Node::Element(sc) => sc.attrs.iter().any(|(k, v)| k.ends_with(":id") && !k.starts_with("xmlns") && used.contains(v)), _ => false });
            any |= !list.children.is_empty();
        }
        if !any { continue; }
        for (k, v) in &root_ns { if e.attr(k).is_none() && !e.attrs.iter().any(|(a, _)| a == k) { e.attrs.push((k.clone(), v.clone())); } }
        out.push_str(&e.to_xml());
    }
    if out.is_empty() { out } else { format!("<extLst>{out}</extLst>") }
}

// ---- reading ---------------------------------------------------------------------------------------

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }

fn caches(wb: &Workbook) -> Vec<(String, Element)> {
    wb.parts.keys().filter(|k| k.starts_with("xl/slicerCaches/") && k.ends_with(".xml") && !k.contains("/_rels/")).filter_map(|k| Some((k.clone(), parse_part(wb, k)?))).collect()
}

/// Pivots that must stay carried because a slicer cache in the file can't be represented.
pub(super) fn blocked_pivots(wb: &Workbook) -> HashSet<String> {
    let cs = caches(wb);
    let ok = cs.iter().all(|(_, c)| c.child("pivotTables").is_some_and(|p| p.child("pivotTable").is_some()) && find(c, "tabular").is_some() || find(c, "tableSlicerCache").is_some());
    if ok { return HashSet::new(); }
    cs.iter().flat_map(|(_, c)| c.child("pivotTables").map(|p| p.all("pivotTable").filter_map(|t| t.attr("name").map(String::from)).collect::<Vec<_>>()).unwrap_or_default()).collect()
}

/// The slicer a drawing's `mc:AlternateContent` frame shows (drawing::load), if representable.
pub fn kind(wb: &Workbook, si: usize, content: &Element) -> Option<ObjKind> {
    let name = find(content, "graphicData").and_then(|g| find(g, "slicer")).and_then(|s| s.attr("name"))?;
    let (cache, caption) = wb.sheets[si].extra.iter().filter(|(k, _)| k == "rel").filter_map(|(_, v)| {
        let f: Vec<&str> = v.split('\t').collect();
        (f.len() >= 4 && f[1] == RT_SLICER && f[3] != "1").then(|| parse_part(wb, f[2])).flatten()
    }).find_map(|e| e.all("slicer").find(|s| s.attr("name") == Some(name)).map(|s| (s.attr("cache").unwrap_or("").to_string(), s.attr("caption").unwrap_or(name).to_string())))?;
    let (_, c) = caches(wb).into_iter().find(|(_, c)| c.attr("name") == Some(cache.as_str()))?;
    let source = c.attr("sourceName")?;
    if let Some(pt) = c.child("pivotTables").and_then(|p| p.child("pivotTable")).and_then(|t| t.attr("name")) {
        find(&c, "tabular")?;
        let p = wb.sheets.iter().flat_map(|s| s.pivots.iter()).find(|p| p.name == pt)?;
        let field = super::pivot_parts::unique_names(p.fields(wb)).iter().position(|n| n.eq_ignore_ascii_case(source))?;
        return Some(ObjKind::Slicer(Slicer { pivot: pt.into(), table: String::new(), field, caption }));
    }
    find(&c, "tableSlicerCache")?;
    // The table by its column name (the model doesn't keep table ids); it must be unambiguous.
    let hits: Vec<(String, usize)> = wb.sheets.iter().flat_map(|s| s.tables.iter()).filter_map(|t| t.columns.iter().position(|col| col.name.eq_ignore_ascii_case(source)).map(|i| (t.display_name.clone(), i))).collect();
    let [(table, field)] = hits.as_slice() else { return None };
    Some(ObjKind::Slicer(Slicer { pivot: String::new(), table: table.clone(), field: *field, caption }))
}

/// After drawings are read: when every slicer of the file became a slicer object, the slicer
/// parts, extensions and names go (they are written afresh); otherwise those objects revert to
/// carried content and the parts stay.
pub fn after_load(wb: &mut Workbook) {
    let total: usize = wb.parts.keys().filter(|k| k.starts_with("xl/slicers/") && k.ends_with(".xml") && !k.contains("/_rels/")).filter_map(|k| parse_part(wb, k)).map(|e| e.all("slicer").count()).sum();
    let native = |o: &Object| matches!(o.kind, ObjKind::Slicer(_)) && o.raw.as_ref().is_some_and(|r| r.local() == "AlternateContent");
    let converted: usize = wb.sheets.iter().map(|s| s.objects.iter().filter(|o| native(o)).count()).sum();
    if total == 0 && converted == 0 { return; }
    if converted != total {
        for s in &mut wb.sheets { for o in &mut s.objects { if native(o) { o.kind = ObjKind::Other; } } }
        return;
    }
    for s in &mut wb.sheets { for o in &mut s.objects { if native(o) { o.raw = None; } } }
    let cache_names: HashSet<String> = caches(wb).iter().filter_map(|(_, c)| c.attr("name").map(String::from)).collect();
    let gone: Vec<String> = wb.parts.keys().filter(|k| k.starts_with("xl/slicers/") || k.starts_with("xl/slicerCaches/")).cloned().collect();
    for k in gone { wb.parts.remove(&rels_name(&k)); wb.parts.remove(&k); }
    for s in &mut wb.sheets {
        s.extra.retain(|(k, v)| !(k == "rel" && v.split('\t').nth(1) == Some(RT_SLICER)));
        if let Some(i) = s.extra.iter().position(|(k, _)| k == "extLst") {
            let kept = strip_slicer_exts(&s.extra[i].1);
            if kept.is_empty() { s.extra.remove(i); } else { s.extra[i].1 = format!("<extLst>{kept}</extLst>"); }
        }
    }
    wb.names.retain(|n| !(n.sheet.is_none() && cache_names.contains(&n.name)));
}
