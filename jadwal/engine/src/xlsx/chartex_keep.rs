//! What a chartex chart read from a file keeps when it is written again from the model
//! (chart/ex.rs regenerates the part on every save): the part's relationships (its chart style
//! and color style parts, chartExStyleN.xml / colorsN.xml, and any embedded data), the series'
//! formatting (spPr, data points, data labels) and the chart space's own formatting (spPr,
//! txPr, clrMapOvr).

use super::read::resolve;
use super::xml::{part_text, Element, Node};
use crate::book::Workbook;

fn rels_name(part: &str) -> String {
    match part.rfind('/') { Some(i) => format!("{}/_rels/{}.rels", &part[..i], &part[i + 1..]), None => format!("_rels/{part}.rels") }
}

/// The original part's relationships whose targets exist, as (id, type, absolute target, external).
fn orig_rels(w: &Workbook, orig: &str) -> Vec<(String, String, String, bool)> {
    let Some(e) = w.parts.get(&rels_name(orig)).and_then(|b| Element::parse(&part_text(b)).ok()) else { return vec![] };
    e.all("Relationship").filter_map(|r| {
        let ext = r.attr("TargetMode") == Some("External");
        let t = r.attr("Target")?;
        let target = if ext { t.to_string() } else { resolve(orig, t) };
        let (id, kind) = (r.attr("Id")?.to_string(), r.attr("Type")?.to_string());
        (ext || w.parts.contains_key(&target)).then_some((id, kind, target, ext))
    }).collect()
}

fn el(e: &Element, l: &str) -> Vec<Element> { e.all(l).cloned().collect() }

/// The series of a plot area region in order, without Pareto lines.
fn series_mut(root: &mut Element) -> Vec<&mut Element> {
    let mut out = vec![];
    for c in root.children.iter_mut() {
        let Node::Element(chart) = c else { continue };
        if chart.local() != "chart" { continue; }
        for pa in chart.children.iter_mut() {
            let Node::Element(pa) = pa else { continue };
            if pa.local() != "plotArea" { continue; }
            for r in pa.children.iter_mut() {
                let Node::Element(r) = r else { continue };
                if r.local() != "plotAreaRegion" { continue; }
                for s in r.children.iter_mut() { if let Node::Element(s) = s { if s.local() == "series" && s.attr("layoutId") != Some("paretoLine") { out.push(s); } } }
            }
        }
    }
    out
}

/// Merges what the original part `orig` had into the regenerated part `xml` (written at `path`),
/// writing the new part's relationships. Returns the merged part.
pub fn carry(w: &mut Workbook, orig: Option<&str>, path: &str, xml: String) -> String {
    let Some(orig) = orig.filter(|o| w.parts.contains_key(*o)) else { return xml };
    let Some(old) = Element::parse(&part_text(&w.parts[orig])).ok() else { return xml };
    // Relationships: same ids, targets relative to the new part.
    let rels = orig_rels(w, orig);
    if !rels.is_empty() {
        let mut r = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        for (id, kind, target, ext) in &rels {
            let t = if *ext { target.clone() } else { super::pivot_parts::relative(path, target) };
            r.push_str(&format!("<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>", super::xml::attr(id), super::xml::attr(kind), super::xml::attr(&t), if *ext { " TargetMode=\"External\"" } else { "" }));
        }
        r.push_str("</Relationships>");
        w.parts.insert(rels_name(path), r.into_bytes());
    }
    let Ok(mut new) = Element::parse(&xml) else { return xml };
    // Namespace declarations the carried elements may use.
    for (k, v) in old.attrs.iter().filter(|(k, _)| k.starts_with("xmlns:")) { if new.attr(k).is_none() { new.attrs.push((k.clone(), v.clone())); } }
    // Series formatting, by position.
    let old_series: Vec<Element> = old.path("chart/plotArea/plotAreaRegion").map(|r| r.all("series").filter(|s| s.attr("layoutId") != Some("paretoLine")).cloned().collect()).unwrap_or_default();
    for (s, o) in series_mut(&mut new).into_iter().zip(&old_series) {
        let mut add: Vec<Element> = el(o, "spPr");
        add.extend(el(o, "valueColors"));
        add.extend(el(o, "valueColorPositions"));
        add.extend(el(o, "dataPt"));
        let labels = o.child("dataLabels").cloned();
        // CT_Series: tx, spPr, valueColors, valueColorPositions, dataPt*, dataLabels, dataId, ...
        let at = s.children.iter().position(|n| matches!(n, Node::Element(e) if e.local() != "tx")).unwrap_or(s.children.len());
        s.children.retain(|n| !matches!(n, Node::Element(e) if matches!(e.local(), "spPr" | "valueColors" | "valueColorPositions" | "dataPt")));
        let at = at.min(s.children.len());
        for (k, e) in add.into_iter().enumerate() { s.children.insert(at + k, Node::Element(e)); }
        if let Some(l) = labels {
            if let Some(i) = s.children.iter().position(|n| matches!(n, Node::Element(e) if e.local() == "dataLabels")) { s.children[i] = Node::Element(l); }
        }
    }
    // Chart space formatting after cx:chart (CT_ChartSpace: chartData, chart, spPr, txPr, clrMapOvr, fmtOvrs, ...).
    for l in ["spPr", "txPr", "clrMapOvr", "fmtOvrs"] {
        if new.child(l).is_some() { continue; }
        if let Some(e) = old.child(l) { new.children.push(Node::Element(e.clone())); }
    }
    // Title and plot area formatting.
    if let (Some(ot), true) = (old.path("chart/title"), new.path("chart/title").is_some()) {
        let extra: Vec<Element> = ot.elements().filter(|e| matches!(e.local(), "spPr" | "txPr")).cloned().collect();
        for c in new.children.iter_mut() { if let Node::Element(ch) = c { if ch.local() == "chart" { for t in ch.children.iter_mut() { if let Node::Element(t) = t { if t.local() == "title" { t.children.retain(|n| !matches!(n, Node::Element(e) if matches!(e.local(), "spPr" | "txPr"))); for e in &extra { t.children.push(Node::Element(e.clone())); } } } } } } }
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n{}", new.to_xml())
}

#[cfg(test)]
mod tests {
    use crate::addr::{parse_range, Pos};
    use crate::book::Workbook;
    use crate::chart::ChartKind;
    use crate::drawing::{add_object, Anchor, ObjKind, Object};
    use crate::xlsx::pkgcheck::{check_package, unzip};

    #[test]
    fn chartex_keeps_style_parts_and_series_formatting() {
        let mut wb = Workbook::new();
        for (i, (a, b)) in [("Start", "100"), ("Up", "20"), ("Down", "-30"), ("End", "90")].iter().enumerate() {
            wb.set_input(0, Pos::new(i as u32, 0), a);
            wb.set_input(0, Pos::new(i as u32, 1), b);
        }
        wb.recalc();
        let c = crate::chart::ex::from_range(&wb, 0, parse_range("A1:B4").unwrap(), ChartKind::Waterfall);
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Chart(Box::new(c)), Anchor::default()), "Chart");
        let bytes = crate::xlsx::write(&wb).unwrap();
        // Make it look like Excel's: style and colors parts and a series fill.
        let mut files = unzip(&bytes);
        let part = files.keys().find(|k| k.starts_with("xl/charts/chartEx") && !k.contains("_rels")).unwrap().clone();
        files.insert(part.clone(), files[&part].replacen("<cx:dataId", "<cx:spPr><a:solidFill><a:srgbClr val=\"FF0000\"/></a:solidFill></cx:spPr><cx:dataPt idx=\"1\"><cx:spPr><a:solidFill><a:srgbClr val=\"00FF00\"/></a:solidFill></cx:spPr></cx:dataPt><cx:dataId", 1)
            .replacen("</cx:chartSpace>", "<cx:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"900\"/></a:pPr><a:endParaRPr lang=\"en-US\"/></a:p></cx:txPr></cx:chartSpace>", 1));
        let rels = part.replace("charts/", "charts/_rels/") + ".rels";
        files.insert(rels, "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId2\" Type=\"http://schemas.microsoft.com/office/2011/relationships/chartColorStyle\" Target=\"colors1.xml\"/><Relationship Id=\"rId1\" Type=\"http://schemas.microsoft.com/office/2011/relationships/chartStyle\" Target=\"style1.xml\"/></Relationships>".into());
        files.insert("xl/charts/style1.xml".into(), "<cs:chartStyle xmlns:cs=\"http://schemas.microsoft.com/office/drawing/2012/chartStyle\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" id=\"395\"/>".into());
        files.insert("xl/charts/colors1.xml".into(), "<cs:colorStyle xmlns:cs=\"http://schemas.microsoft.com/office/drawing/2012/chartStyle\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" meth=\"cycle\" id=\"10\"/>".into());
        let ct = files["[Content_Types].xml"].replace("</Types>", "<Override PartName=\"/xl/charts/style1.xml\" ContentType=\"application/vnd.ms-office.chartstyle+xml\"/><Override PartName=\"/xl/charts/colors1.xml\" ContentType=\"application/vnd.ms-office.chartcolorstyle+xml\"/></Types>");
        files.insert("[Content_Types].xml".into(), ct);
        let mut buf = std::io::Cursor::new(Vec::new());
        { use std::io::Write; let mut z = zip::ZipWriter::new(&mut buf); for (k, v) in &files { z.start_file(k.as_str(), zip::write::SimpleFileOptions::default()).unwrap(); z.write_all(v.as_bytes()).unwrap(); } z.finish().unwrap(); }
        let mut back = crate::xlsx::read(&buf.into_inner()).unwrap();
        // Edit the chart (so it is regenerated) and save twice.
        if let Some(c) = back.sheets[0].objects[0].chart_mut() { c.title = Some("Bridge".into()); }
        let mut out = crate::xlsx::write(&back).unwrap();
        for _ in 0..2 {
            let f = unzip(&out);
            check_package(&f);
            let part = f.keys().find(|k| k.starts_with("xl/charts/chartEx") && !k.contains("_rels")).unwrap();
            let x = &f[part];
            assert!(x.contains("FF0000") && x.contains("<cx:dataPt idx=\"1\">") && x.contains("<cx:txPr>") && x.contains("Bridge"), "{x}");
            assert!(x.find("<cx:spPr>").unwrap() < x.find("<cx:dataId").unwrap());
            let r = &f[&(part.replace("charts/", "charts/_rels/") + ".rels")];
            assert!(r.contains("chartStyle") && r.contains("chartColorStyle"));
            assert!(f.contains_key("xl/charts/style1.xml") && f.contains_key("xl/charts/colors1.xml"));
            assert!(f["[Content_Types].xml"].contains("/xl/charts/style1.xml"));
            out = crate::xlsx::write(&crate::xlsx::read(&out).unwrap()).unwrap();
        }
    }
}
