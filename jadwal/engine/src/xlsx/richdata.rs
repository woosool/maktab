//! Pictures in cells as Excel stores them: rich values (MS-XLSX 2.3.x "Rich Data").
//!
//! A placed picture is a `_localImage` rich value (xl/richData/rdrichvalue.xml) whose
//! `_rvRel:LocalImageIdentifier` indexes xl/richData/richValueRel.xml, a list of relationships
//! to the images; `CalcOrigin` 5 marks a picture placed in the cell, `Text` its alt text. The
//! structures are in rdrichvaluestructure.xml, key flags in rdRichValueTypes.xml. A cell shows
//! rich value i through xl/metadata.xml: its `vm` (1-based) picks a valueMetadata block whose
//! record names the XLRICHVALUE futureMetadata block holding `<xlrd:rvb i="i"/>`. Excel writes
//! such cells as `t="e"` with `#VALUE!` cached (what older readers show).
//!
//! Saving (cellimage::stage → `stage`): the parts go into `Workbook.parts` (carried by the
//! package writer, typed through [Content_Types].xml, related from the workbook) and each cell
//! holds `marker(vm)`, which the cell writer turns into `vm`; `metadata` builds metadata.xml
//! with the dynamic-array type when formulas spill. Reading: cells with `vm` hold the marker
//! until `load` resolves them into picture values and drops the rich data parts.

use super::read::{rels_name, resolve};
use super::xml::{attr, part_text, Element, Node};
use crate::addr::Pos;
use crate::book::{Input, Workbook};
use crate::value::{ErrKind, Value};

const MARK: char = '\u{E002}';
const NS_RD: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
const NS_RVREL: &str = "http://schemas.microsoft.com/office/spreadsheetml/2022/richvaluerel";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const RT_RV: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValue";
const RT_RVS: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueStructure";
const RT_RVT: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueTypes";
const RT_RVREL: &str = "http://schemas.microsoft.com/office/2022/10/relationships/richValueRel";
const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const P_RV: &str = "xl/richData/rdrichvalue.xml";
const P_RVS: &str = "xl/richData/rdrichvaluestructure.xml";
const P_RVT: &str = "xl/richData/rdRichValueTypes.xml";
const P_RVREL: &str = "xl/richData/richValueRel.xml";
/// The workbook relationship kinds (last path segment) the package writer keeps for these parts.
pub const WB_KINDS: [&str; 4] = ["rdRichValue", "rdRichValueStructure", "rdRichValueTypes", "richValueRel"];

/// A cell value standing for rich value metadata index `vm` (1-based) while saving or reading.
pub fn marker(vm: u32) -> Value { Value::str(format!("{MARK}{vm}")) }
pub fn marker_vm(v: &Value) -> Option<u32> { if let Value::Str(s) = v { s.strip_prefix(MARK)?.parse().ok() } else { None } }

const TYPES: &str = "<rvTypesInfo xmlns=\"http://schemas.microsoft.com/office/spreadsheetml/2017/richdata2\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"x\" xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><global><keyFlags>\
<key name=\"_Self\"><flag name=\"ExcludeFromFile\" value=\"1\"/><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_DisplayString\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_Flags\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_Format\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_SubLabel\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_Attribution\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_Icon\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_Display\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_CanonicalPropertyNames\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
<key name=\"_ClassificationId\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>\
</keyFlags></global></rvTypesInfo>";

fn parse(wb: &Workbook, p: &str) -> Option<Element> { wb.parts.get(p).and_then(|b| Element::parse(&part_text(b)).ok()) }

fn rels_of(wb: &Workbook, part: &str) -> Vec<(String, String, String)> {
    parse(wb, &rels_name(part)).map(|e| e.all("Relationship").filter(|r| r.attr("TargetMode") != Some("External")).filter_map(|r| Some((r.attr("Id")?.to_string(), r.attr("Type")?.to_string(), resolve(part, r.attr("Target")?)))).collect()).unwrap_or_default()
}

/// Removes the rich data parts, their workbook relationships and content type overrides.
fn drop_parts(wb: &mut Workbook) {
    let gone: Vec<String> = wb.parts.keys().filter(|k| k.starts_with("xl/richData/")).cloned().collect();
    for k in gone { wb.parts.remove(&k); }
    if let Some(mut r) = parse(wb, "xl/_rels/workbook.xml.rels") {
        r.children.retain(|n| !matches!(n, Node::Element(e) if e.attr("Type").and_then(|t| t.rsplit('/').next()).is_some_and(|t| WB_KINDS.contains(&t))));
        wb.parts.insert("xl/_rels/workbook.xml.rels".into(), r.to_xml().into_bytes());
    }
}

/// Saving: the placed pictures (sheet, cell, image bytes, extension, alt text) become rich
/// values in the copy `w`; each cell's value and input become `marker(vm)`.
pub fn stage(w: &mut Workbook, pics: Vec<(usize, Pos, Vec<u8>, String, String)>) {
    drop_parts(w);
    if pics.is_empty() { return; }
    let mut rv = format!("{DECL}<rvData xmlns=\"{NS_RD}\" count=\"{}\">", pics.len());
    let mut rel = format!("{DECL}<richValueRels xmlns=\"{NS_RVREL}\" xmlns:r=\"{NS_R}\">");
    let mut rel_rels = format!("{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    let mut images: Vec<(Vec<u8>, String)> = vec![];
    for (k, (si, p, data, ext, alt)) in pics.into_iter().enumerate() {
        // Identical pictures share one image relationship.
        let img = match images.iter().position(|(d, _)| *d == data) {
            Some(i) => i,
            None => {
                let ext = ext.to_ascii_lowercase();
                let path = (1..).map(|i| format!("xl/media/image{i}.{ext}")).find(|n| !w.parts.contains_key(n)).unwrap();
                w.parts.insert(path.clone(), data.clone());
                crate::embed::ensure_content_type(w, &ext, match ext.as_str() { "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", "svg" => "image/svg+xml", "tif" | "tiff" => "image/tiff", _ => "image/png" });
                let n = images.len();
                rel.push_str(&format!("<rel r:id=\"rId{}\"/>", n + 1));
                rel_rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"{RT_IMAGE}\" Target=\"../media/{}\"/>", n + 1, attr(path.rsplit('/').next().unwrap())));
                images.push((data, path));
                n
            }
        };
        if alt.is_empty() { rv.push_str(&format!("<rv s=\"0\"><v>{img}</v><v>5</v></rv>")); }
        else { rv.push_str(&format!("<rv s=\"1\"><v>{img}</v><v>5</v><v>{}</v></rv>", super::xml::text(&alt))); }
        if let Some(c) = w.sheets[si].cells.get_mut(&p) { c.value = marker(k as u32 + 1); c.input = Input::Value(c.value.clone()); }
    }
    rv.push_str("</rvData>");
    rel.push_str("</richValueRels>");
    rel_rels.push_str("</Relationships>");
    let rvs = format!("{DECL}<rvStructures xmlns=\"{NS_RD}\" count=\"2\"><s t=\"_localImage\"><k n=\"_rvRel:LocalImageIdentifier\" t=\"i\"/><k n=\"CalcOrigin\" t=\"i\"/></s><s t=\"_localImage\"><k n=\"_rvRel:LocalImageIdentifier\" t=\"i\"/><k n=\"CalcOrigin\" t=\"i\"/><k n=\"Text\" t=\"s\"/></s></rvStructures>");
    w.parts.insert(P_RV.into(), rv.into_bytes());
    w.parts.insert(P_RVS.into(), rvs.into_bytes());
    w.parts.insert(P_RVT.into(), format!("{DECL}{TYPES}").into_bytes());
    w.parts.insert(P_RVREL.into(), rel.into_bytes());
    w.parts.insert(rels_name(P_RVREL), rel_rels.into_bytes());
    // Content types.
    let mut ct = parse(w, "[Content_Types].xml").unwrap_or_else(|| Element { name: "Types".into(), attrs: vec![("xmlns".into(), "http://schemas.openxmlformats.org/package/2006/content-types".into())], children: vec![] });
    for (p, t) in [(P_RV, "rdrichvalue"), (P_RVS, "rdrichvaluestructure"), (P_RVT, "rdrichvaluetypes"), (P_RVREL, "richvaluerel")] {
        let pn = format!("/{p}");
        ct.children.retain(|n| !matches!(n, Node::Element(e) if e.local() == "Override" && e.attr("PartName") == Some(pn.as_str())));
        ct.children.push(Node::Element(Element { name: "Override".into(), attrs: vec![("PartName".into(), pn), ("ContentType".into(), format!("application/vnd.ms-excel.{t}+xml"))], children: vec![] }));
    }
    w.parts.insert("[Content_Types].xml".into(), ct.to_xml().into_bytes());
    // Workbook relationships (kept by the package writer, WB_KINDS).
    let mut r = parse(w, "xl/_rels/workbook.xml.rels").unwrap_or_else(|| Element { name: "Relationships".into(), attrs: vec![("xmlns".into(), "http://schemas.openxmlformats.org/package/2006/relationships".into())], children: vec![] });
    for (kind, p) in [(RT_RV, P_RV), (RT_RVS, P_RVS), (RT_RVT, P_RVT), (RT_RVREL, P_RVREL)] {
        let id = (1..).map(|i| format!("rIdRd{i}")).find(|i| !r.all("Relationship").any(|e| e.attr("Id") == Some(i.as_str()))).unwrap();
        r.children.push(Node::Element(Element { name: "Relationship".into(), attrs: vec![("Id".into(), id), ("Type".into(), kind.into()), ("Target".into(), p.trim_start_matches("xl/").into())], children: vec![] }));
    }
    w.parts.insert("xl/_rels/workbook.xml.rels".into(), r.to_xml().into_bytes());
}

/// The number of rich values the copy being saved holds.
fn count(wb: &Workbook) -> usize { parse(wb, P_RV).map(|e| e.all("rv").count()).unwrap_or(0) }

/// xl/metadata.xml for a save: the dynamic-array type (cm="1") and the rich values (vm="1"…).
/// None when neither is needed.
pub fn metadata(wb: &Workbook, dynamic: bool) -> Option<String> {
    let n = count(wb);
    if n == 0 && !dynamic { return None; }
    const FLAGS: &str = "minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\"";
    let mut types = vec![];
    let mut future = String::new();
    let mut tail = String::new();
    if dynamic {
        types.push(format!("<metadataType name=\"XLDAPR\" {FLAGS} cellMeta=\"1\"/>"));
        future.push_str("<futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\"><xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata>");
        tail.push_str(&format!("<cellMetadata count=\"1\"><bk><rc t=\"{}\" v=\"0\"/></bk></cellMetadata>", types.len()));
    }
    if n > 0 {
        types.push(format!("<metadataType name=\"XLRICHVALUE\" {FLAGS}/>"));
        let t = types.len();
        future.push_str(&format!("<futureMetadata name=\"XLRICHVALUE\" count=\"{n}\">"));
        for i in 0..n { future.push_str(&format!("<bk><extLst><ext uri=\"{{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}}\"><xlrd:rvb i=\"{i}\"/></ext></extLst></bk>")); }
        future.push_str("</futureMetadata>");
        tail.push_str(&format!("<valueMetadata count=\"{n}\">"));
        for i in 0..n { tail.push_str(&format!("<bk><rc t=\"{t}\" v=\"{i}\"/></bk>")); }
        tail.push_str("</valueMetadata>");
    }
    Some(format!("{DECL}<metadata xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:xlrd=\"{NS_RD}\" xmlns:xda=\"http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray\"><metadataTypes count=\"{}\">{}</metadataTypes>{future}{tail}</metadata>", types.len(), types.concat()))
}

/// Reading: the picture (bytes, extension, alt) of each valueMetadata index (1-based), from
/// metadata.xml and the rich data parts.
fn pictures(wb: &Workbook) -> std::collections::HashMap<u32, (Vec<u8>, String, String)> {
    let mut out = std::collections::HashMap::new();
    let Some(md) = parse(wb, "xl/metadata.xml") else { return out };
    let types: Vec<String> = md.child("metadataTypes").map(|t| t.all("metadataType").map(|m| m.attr("name").unwrap_or("").to_string()).collect()).unwrap_or_default();
    let rvb: Vec<Option<usize>> = md.all("futureMetadata").find(|f| f.attr("name") == Some("XLRICHVALUE"))
        .map(|f| f.all("bk").map(|bk| find(bk, "rvb").and_then(|r| r.attr("i")).and_then(|i| i.parse().ok())).collect()).unwrap_or_default();
    // The rich data parts through the workbook's relationships (fallback: the usual names).
    let wrels = rels_of(wb, "xl/workbook.xml");
    let part = |kind: &str, dflt: &str| wrels.iter().find(|r| r.1 == kind).map(|r| r.2.clone()).unwrap_or_else(|| dflt.to_string());
    let (prv, prvs, prel) = (part(RT_RV, P_RV), part(RT_RVS, P_RVS), part(RT_RVREL, P_RVREL));
    let rvs: Vec<Element> = parse(wb, &prv).map(|e| e.all("rv").cloned().collect()).unwrap_or_default();
    let structs: Vec<Vec<String>> = parse(wb, &prvs).map(|e| e.all("s").map(|s| s.all("k").map(|k| k.attr("n").unwrap_or("").to_string()).collect()).collect()).unwrap_or_default();
    let rel_rels = rels_of(wb, &prel);
    let rel_ids: Vec<String> = parse(wb, &prel).map(|e| e.all("rel").filter_map(|r| r.attrs.iter().find(|(k, _)| k.ends_with(":id")).map(|(_, v)| v.clone())).collect()).unwrap_or_default();
    let Some(vmd) = md.child("valueMetadata") else { return out };
    for (k, bk) in vmd.all("bk").enumerate() {
        let Some(rc) = bk.child("rc") else { continue };
        let (Some(t), Some(v)) = (rc.attr_u32("t"), rc.attr_u32("v")) else { continue };
        if types.get(t as usize - 1).map(String::as_str) != Some("XLRICHVALUE") { continue; }
        let Some(rv) = rvb.get(v as usize).copied().flatten().and_then(|i| rvs.get(i)) else { continue };
        let Some(keys) = rv.attr_u32("s").and_then(|s| structs.get(s as usize)) else { continue };
        let vals: Vec<String> = rv.all("v").map(|x| x.text()).collect();
        let get = |n: &str| keys.iter().position(|k| k == n).and_then(|i| vals.get(i)).cloned();
        let Some(img) = get("_rvRel:LocalImageIdentifier").and_then(|x| x.trim().parse::<usize>().ok()) else { continue };
        let Some(target) = rel_ids.get(img).and_then(|id| rel_rels.iter().find(|r| &r.0 == id)).map(|r| r.2.clone()) else { continue };
        let Some(data) = wb.parts.get(&target).cloned() else { continue };
        let ext = target.rsplit('.').next().unwrap_or("png").to_ascii_lowercase();
        out.insert(k as u32 + 1, (data, ext, get("Text").unwrap_or_default()));
    }
    out
}

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }

/// After reading: marker cells become picture values (or #VALUE!, what Excel caches, when the
/// rich value isn't a picture); the rich data parts go (a save writes them afresh).
pub fn load(wb: &mut Workbook) {
    let any = wb.sheets.iter().any(|s| s.cells.values().any(|c| marker_vm(&c.value).is_some()));
    if !any && !wb.parts.keys().any(|k| k.starts_with("xl/richData/")) { return; }
    let pics = pictures(wb);
    let wrels = rels_of(wb, "xl/workbook.xml");
    let prel = wrels.iter().find(|r| r.1 == RT_RVREL).map(|r| r.2.clone()).unwrap_or_else(|| P_RVREL.to_string());
    for (_, _, t) in rels_of(wb, &prel) { if t.starts_with("xl/media/") { wb.parts.remove(&t); } }
    for s in &mut wb.sheets {
        for c in s.cells.values_mut() {
            let Some(vm) = marker_vm(&c.value) else { continue };
            let v = match pics.get(&vm) {
                Some((data, ext, alt)) => crate::cellimage::value(&crate::cellimage::data_url(data, ext), alt),
                None => Value::Err(ErrKind::Value),
            };
            c.value = v.clone();
            c.input = Input::Value(v);
        }
    }
    drop_parts(wb);
    wb.parts.remove("xl/metadata.xml");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xlsx::pkgcheck::{check_package, unzip};

    const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x2D, 0x7A, 0x9C, 0x0F, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82];

    fn img(wb: &Workbook, s: usize, p: Pos) -> Option<(Vec<u8>, String)> {
        let v = wb.sheets[s].value(p);
        let (src, alt) = crate::cellimage::parse(&v)?;
        Some((crate::cellimage::bytes(src)?.0, alt.to_string()))
    }

    #[test]
    fn pictures_in_cells_round_trip_as_rich_values() {
        let mut wb = Workbook::new();
        crate::cellimage::place(&mut wb, 0, Pos::new(1, 1), PNG, "png", "Dot");
        crate::cellimage::place(&mut wb, 0, Pos::new(2, 1), PNG, "png", "");
        let out = crate::xlsx::write(&wb).unwrap();
        if let Ok(d) = std::env::var("JADWAL_DUMP") { std::fs::write(format!("{d}/richdata.xlsx"), &out).unwrap(); }
        let files = unzip(&out);
        check_package(&files);
        assert!(files["xl/worksheets/sheet1.xml"].contains("<c r=\"B2\" t=\"e\" vm=\"1\"><v>#VALUE!</v></c>"));
        assert!(files["xl/worksheets/sheet1.xml"].contains("vm=\"2\""));
        let md = &files["xl/metadata.xml"];
        assert!(md.contains("XLRICHVALUE") && md.contains("<xlrd:rvb i=\"1\"/>") && !md.contains("XLDAPR"));
        assert_eq!(files.keys().filter(|k| k.starts_with("xl/media/")).count(), 1, "identical pictures share an image");
        for k in ["xl/richData/rdrichvalue.xml", "xl/richData/rdrichvaluestructure.xml", "xl/richData/rdRichValueTypes.xml", "xl/richData/richValueRel.xml"] { assert!(files.contains_key(k), "{k}"); }
        assert!(files["xl/_rels/workbook.xml.rels"].contains("rdRichValueTypes") && files["xl/_rels/workbook.xml.rels"].contains("2022/10/relationships/richValueRel"));
        let back = crate::xlsx::read(&out).unwrap();
        assert!(back.sheets[0].objects.is_empty());
        assert_eq!(img(&back, 0, Pos::new(1, 1)), Some((PNG.to_vec(), "Dot".into())));
        assert_eq!(img(&back, 0, Pos::new(2, 1)), Some((PNG.to_vec(), String::new())));
        // Saved again: the same parts once, no leftovers.
        let files2 = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&files2);
        assert_eq!(files2.keys().filter(|k| k.starts_with("xl/media/")).count(), 1);
        assert_eq!(files2["xl/_rels/workbook.xml.rels"].matches("rdRichValue\"").count(), 1);
    }

    #[test]
    fn rich_values_share_metadata_with_dynamic_arrays() {
        let mut wb = Workbook::new();
        crate::cellimage::place(&mut wb, 0, Pos::new(0, 0), PNG, "png", "Dot");
        wb.set_input(0, Pos::new(0, 3), "=SEQUENCE(3)");
        wb.recalc();
        let out = crate::xlsx::write(&wb).unwrap();
        if let Ok(d) = std::env::var("JADWAL_DUMP") { std::fs::write(format!("{d}/richdata_da.xlsx"), &out).unwrap(); }
        let files = unzip(&out);
        check_package(&files);
        let md = Element::parse(&files["xl/metadata.xml"]).unwrap();
        let types: Vec<&str> = md.child("metadataTypes").unwrap().all("metadataType").filter_map(|t| t.attr("name")).collect();
        assert_eq!(types, ["XLDAPR", "XLRICHVALUE"]);
        assert_eq!(md.path("cellMetadata/bk/rc").unwrap().attr("t"), Some("1"));
        assert_eq!(md.path("valueMetadata/bk/rc").unwrap().attr("t"), Some("2"));
        let sh = &files["xl/worksheets/sheet1.xml"];
        assert!(sh.contains("cm=\"1\"") && sh.contains("vm=\"1\""));
        let back = crate::xlsx::read(&out).unwrap();
        assert!(img(&back, 0, Pos::new(0, 0)).is_some());
        assert_eq!(back.sheets[0].value(Pos::new(2, 3)), Value::Num(3.0));
    }

    /// A placed picture as Excel writes it (rich value with only the two keys, no alt text).
    #[test]
    fn reads_excel_rich_value_pictures() {
        let mut wb = Workbook::new();
        crate::cellimage::place(&mut wb, 0, Pos::new(0, 0), PNG, "png", "x");
        let files = unzip(&crate::xlsx::write(&wb).unwrap());
        // Rewrite as Excel's own layout: one structure, other relationship ids.
        let mut zipped = std::io::Cursor::new(Vec::new());
        {
            use std::io::Write;
            let mut z = zip::ZipWriter::new(&mut zipped);
            for (k, v) in &files {
                let v = match k.as_str() {
                    "xl/richData/rdrichvalue.xml" => format!("{DECL}<rvData xmlns=\"{NS_RD}\" count=\"1\"><rv s=\"0\"><v>0</v><v>5</v></rv></rvData>"),
                    "xl/richData/rdrichvaluestructure.xml" => format!("{DECL}<rvStructures xmlns=\"{NS_RD}\" count=\"1\"><s t=\"_localImage\"><k n=\"_rvRel:LocalImageIdentifier\" t=\"i\"/><k n=\"CalcOrigin\" t=\"i\"/></s></rvStructures>"),
                    _ => v.clone(),
                };
                z.start_file(k.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
                if k.starts_with("xl/media/") { z.write_all(PNG).unwrap(); } else { z.write_all(v.as_bytes()).unwrap(); }
            }
            z.finish().unwrap();
        }
        let back = crate::xlsx::read(&zipped.into_inner()).unwrap();
        assert_eq!(img(&back, 0, Pos::new(0, 0)), Some((PNG.to_vec(), String::new())));
    }
}
