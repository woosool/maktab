//! SmartArt: Jadwal inserts a graphic as a group of shapes (app ins_smartart.rs, described
//! "jadwal-smartart:<layout>"). Saving also writes Excel's diagram parts — data model, layout
//! definition (by the built-in layout's uniqueId), quick style, colors and the drawing of the
//! rendered shapes (dsp, from the group's members) — named by a `dgm:relIds` graphic frame, so
//! Excel opens it as SmartArt. Reading turns a SmartArt frame into the group again, built from
//! the diagram drawing's shapes (the diagram parts are written afresh on saving).

use super::native_objects::Sink;
use super::xml::{attr, part_text, text, Element};
use crate::book::Workbook;
use crate::drawing::{Anchor, ObjKind, Object, Shape, EMU};
use crate::group::Group;

const TAG: &str = "jadwal-smartart:";
const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const RT_DRAWING: &str = "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing";
const CT: &str = "application/vnd.openxmlformats-officedocument.drawingml.";
const CT_DRAWING: &str = "application/vnd.ms-office.drawingml.diagramDrawing+xml";
const NS_DGM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const NS_DSP: &str = "http://schemas.microsoft.com/office/drawing/2008/diagram";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

/// Jadwal's layouts and Office's built-in layouts of the same design.
const LAYOUTS: &[(&str, &str)] = &[
    ("basicBlockList", "2005/8/layout/default"), ("verticalBulletList", "2005/8/layout/vList2"), ("horizontalBulletList", "2005/8/layout/hList1"),
    ("stackedList", "2005/8/layout/hList9"), ("basicProcess", "2005/8/layout/process1"), ("chevronProcess", "2005/8/layout/chevron1"),
    ("stepUpProcess", "2009/3/layout/StepUpProcess"), ("verticalProcess", "2005/8/layout/process2"), ("basicTimeline", "2005/8/layout/hProcess11"),
    ("basicCycle", "2005/8/layout/cycle2"), ("segmentedCycle", "2005/8/layout/cycle8"), ("radialCycle", "2005/8/layout/radial6"),
    ("orgChart", "2005/8/layout/orgChart1"), ("horizontalHierarchy", "2005/8/layout/hierarchy2"), ("basicVenn", "2005/8/layout/venn1"),
    ("basicRadial", "2005/8/layout/radial1"), ("basicTarget", "2005/8/layout/target1"), ("funnel", "2005/8/layout/funnel1"),
    ("basicMatrix", "2005/8/layout/matrix3"), ("basicPyramid", "2005/8/layout/pyramid1"), ("invertedPyramid", "2005/8/layout/pyramid3"),
];
const URN: &str = "urn:microsoft.com/office/officeart/";

fn layout_urn(id: &str) -> String {
    if id.starts_with("urn:") { return id.into(); }
    format!("{URN}{}", LAYOUTS.iter().find(|l| l.0 == id).map_or("2005/8/layout/default", |l| l.1))
}
fn layout_id(urn: &str) -> String { LAYOUTS.iter().find(|l| urn.strip_prefix(URN) == Some(l.1)).map_or_else(|| urn.to_string(), |l| l.0.to_string()) }

fn guid(seed: u32, k: usize) -> String { format!("{{6A61646C-{:04X}-4000-8000-{:012X}}}", seed & 0xFFFF, k) }
fn emu(px: f32) -> i64 { (px as f64 * EMU as f64).round() as i64 }
fn hexs(c: [u8; 3]) -> String { format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }

fn paras(t: &str, s: &Shape) -> String {
    let color = s.color.map(|c| format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hexs(c))).unwrap_or_default();
    t.split('\n').map(|l| format!("<a:p>{}<a:r><a:rPr lang=\"en-US\" sz=\"{}\" b=\"{}\">{color}</a:rPr><a:t>{}</a:t></a:r></a:p>", if s.center { "<a:pPr algn=\"ctr\"/>" } else { "" }, (s.size.max(1.0) * 100.0).round() as i64, s.bold as u8, text(l))).collect()
}

// ---- layout definitions ----------------------------------------------------------------------
//
// Each layout part is a complete dgm:layoutDef (ECMA-376 Part 1 §21.4.2) for its family: the
// root layout node with the family's algorithm (snake / lin / cycle / pyramid / hierChild), the
// constraints and rules that size boxes, spacing and text, a forEach over the data points with a
// text node per point, and the sibling transitions (connector arrows or spaces). The uniqueId stays
// the built-in layout's URN, so Office versions with that layout use their own copy; the
// definition written here is what other consumers (and Office without the built-in) lay out with.

const SHAPE_NS: &str = "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";

/// The layout family and parameters of an Office layout id: (algorithm XML of the root, box
/// shape, sibling transition shape or "" for a space, category).
fn family(office: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    let id = office.rsplit('/').next().unwrap_or("default");
    match id {
        "vList2" => ("<dgm:alg type=\"lin\"><dgm:param type=\"linDir\" val=\"fromT\"/><dgm:param type=\"vertAlign\" val=\"mid\"/></dgm:alg>", "roundRect", "", "list"),
        "hList1" | "hList9" => ("<dgm:alg type=\"lin\"/>", "rect", "", "list"),
        "process1" | "hProcess11" => ("<dgm:alg type=\"lin\"/>", "roundRect", "rightArrow", "process"),
        "process2" => ("<dgm:alg type=\"lin\"><dgm:param type=\"linDir\" val=\"fromT\"/></dgm:alg>", "roundRect", "rightArrow", "process"),
        "chevron1" => ("<dgm:alg type=\"lin\"/>", "chevron", "", "process"),
        "StepUpProcess" => ("<dgm:alg type=\"lin\"><dgm:param type=\"nodeVertAlign\" val=\"b\"/></dgm:alg>", "rect", "", "process"),
        "cycle2" => ("<dgm:alg type=\"cycle\"/>", "ellipse", "rightArrow", "cycle"),
        "cycle8" => ("<dgm:alg type=\"cycle\"/>", "pieWedge", "", "cycle"),
        "radial6" | "radial1" => ("<dgm:alg type=\"cycle\"><dgm:param type=\"ctrShpMap\" val=\"fNode\"/></dgm:alg>", "ellipse", "", "relationship"),
        "venn1" => ("<dgm:alg type=\"cycle\"/>", "ellipse", "", "relationship"),
        "target1" => ("<dgm:alg type=\"lin\"><dgm:param type=\"linDir\" val=\"fromT\"/></dgm:alg>", "donut", "", "relationship"),
        "funnel1" => ("<dgm:alg type=\"lin\"><dgm:param type=\"linDir\" val=\"fromT\"/></dgm:alg>", "trapezoid", "", "relationship"),
        "pyramid1" => ("<dgm:alg type=\"pyra\"><dgm:param type=\"linDir\" val=\"fromT\"/><dgm:param type=\"txDir\" val=\"fromT\"/></dgm:alg>", "trapezoid", "", "pyramid"),
        "pyramid3" => ("<dgm:alg type=\"pyra\"><dgm:param type=\"linDir\" val=\"fromB\"/><dgm:param type=\"txDir\" val=\"fromT\"/></dgm:alg>", "trapezoid", "", "pyramid"),
        "orgChart1" | "hierarchy2" => ("", "rect", "", "hierarchy"),
        "matrix3" => ("<dgm:alg type=\"snake\"><dgm:param type=\"grDir\" val=\"tL\"/><dgm:param type=\"flowDir\" val=\"row\"/><dgm:param type=\"contDir\" val=\"sameDir\"/></dgm:alg>", "roundRect", "", "matrix"),
        _ => ("<dgm:alg type=\"snake\"><dgm:param type=\"grDir\" val=\"tL\"/><dgm:param type=\"flowDir\" val=\"row\"/><dgm:param type=\"contDir\" val=\"sameDir\"/><dgm:param type=\"off\" val=\"ctr\"/></dgm:alg>", "rect", "", "list"),
    }
}

/// A sample data model (doc + `n` placeholder nodes), for sampData / styleData / clrData.
fn sample_model(n: usize) -> String {
    let pts: String = (1..=n).map(|i| format!("<dgm:pt modelId=\"{i}\"><dgm:prSet phldr=\"1\"/></dgm:pt>")).collect();
    let cx: String = (1..=n).map(|i| format!("<dgm:cxn modelId=\"{}\" srcId=\"0\" destId=\"{i}\" srcOrd=\"{}\" destOrd=\"0\"/>", 10 + i, i - 1)).collect();
    format!("<dgm:dataModel><dgm:ptLst><dgm:pt modelId=\"0\" type=\"doc\"/>{pts}</dgm:ptLst><dgm:cxnLst>{cx}</dgm:cxnLst><dgm:bg/><dgm:whole/></dgm:dataModel>")
}

/// The text node of one data point.
fn text_node(name: &str, shape: &str, style: &str) -> String {
    format!("<dgm:layoutNode name=\"{name}\" styleLbl=\"{style}\"><dgm:varLst><dgm:bulletEnabled val=\"1\"/></dgm:varLst><dgm:alg type=\"tx\"/><dgm:shape {SHAPE_NS} type=\"{shape}\" r:blip=\"\"><dgm:adjLst/></dgm:shape><dgm:presOf axis=\"desOrSelf\" ptType=\"node\"/><dgm:constrLst><dgm:constr type=\"tMarg\" refType=\"primFontSz\" fact=\"0.3\"/><dgm:constr type=\"bMarg\" refType=\"primFontSz\" fact=\"0.3\"/><dgm:constr type=\"lMarg\" refType=\"primFontSz\" fact=\"0.3\"/><dgm:constr type=\"rMarg\" refType=\"primFontSz\" fact=\"0.3\"/></dgm:constrLst><dgm:ruleLst><dgm:rule type=\"primFontSz\" val=\"5\" fact=\"NaN\" max=\"NaN\"/></dgm:ruleLst></dgm:layoutNode>")
}

/// The sibling transition after each point: a connector arrow with its text, or a space.
fn sib_trans(shape: &str) -> String {
    let inner = if shape.is_empty() {
        format!("<dgm:layoutNode name=\"sibTrans\"><dgm:alg type=\"sp\"/><dgm:shape {SHAPE_NS} r:blip=\"\"><dgm:adjLst/></dgm:shape><dgm:presOf/><dgm:constrLst/><dgm:ruleLst/></dgm:layoutNode>")
    } else {
        format!("<dgm:layoutNode name=\"sibTrans\" styleLbl=\"sibTrans2D1\"><dgm:alg type=\"conn\"/><dgm:shape {SHAPE_NS} type=\"conn\" r:blip=\"\"><dgm:adjLst/></dgm:shape><dgm:presOf axis=\"self\"/><dgm:constrLst><dgm:constr type=\"h\" refType=\"w\" fact=\"0.62\"/><dgm:constr type=\"connDist\"/><dgm:constr type=\"begPad\" refType=\"connDist\" fact=\"0.25\"/><dgm:constr type=\"endPad\" refType=\"connDist\" fact=\"0.22\"/></dgm:constrLst><dgm:ruleLst/><dgm:layoutNode name=\"connectorText\"><dgm:alg type=\"tx\"><dgm:param type=\"autoTxRot\" val=\"grav\"/></dgm:alg><dgm:shape {SHAPE_NS} type=\"conn\" r:blip=\"\" hideGeom=\"1\"><dgm:adjLst/></dgm:shape><dgm:presOf axis=\"self\"/><dgm:constrLst><dgm:constr type=\"lMarg\"/><dgm:constr type=\"rMarg\"/><dgm:constr type=\"tMarg\"/><dgm:constr type=\"bMarg\"/></dgm:constrLst><dgm:ruleLst/></dgm:layoutNode></dgm:layoutNode>")
    };
    format!("<dgm:forEach name=\"sibTransForEach\" axis=\"followSib\" ptType=\"sibTrans\" cnt=\"1\">{inner}</dgm:forEach>")
}

/// The complete layout definition part for a layout URN.
fn layout_def(urn: &str) -> String {
    let office = urn.strip_prefix(URN).unwrap_or("2005/8/layout/default");
    let (alg, shape, trans, cat) = family(office);
    let empty_shape = format!("<dgm:shape {SHAPE_NS} r:blip=\"\"><dgm:adjLst/></dgm:shape>");
    let head = format!("{DECL}<dgm:layoutDef xmlns:dgm=\"{NS_DGM}\" xmlns:a=\"{NS_A}\" uniqueId=\"{}\"><dgm:title val=\"\"/><dgm:desc val=\"\"/><dgm:catLst><dgm:cat type=\"{cat}\" pri=\"1000\"/></dgm:catLst><dgm:sampData>{}</dgm:sampData><dgm:styleData>{}</dgm:styleData><dgm:clrData>{}</dgm:clrData>",
        attr(urn), sample_model(3), sample_model(2), sample_model(4));
    let body = if cat == "hierarchy" {
        // hierChild / hierRoot: each point is a root with its text and a child hierarchy that
        // reuses the same forEach; parent-child connectors are drawn per parTrans.
        let horiz = office.ends_with("hierarchy2");
        let dir = if horiz { "<dgm:param type=\"linDir\" val=\"fromT\"/><dgm:param type=\"chAlign\" val=\"l\"/>" } else { "<dgm:param type=\"chAlign\" val=\"t\"/><dgm:param type=\"linDir\" val=\"fromL\"/>" };
        let root_alg = if horiz { "<dgm:alg type=\"hierRoot\"><dgm:param type=\"hierAlign\" val=\"lCtrCh\"/></dgm:alg>" } else { "<dgm:alg type=\"hierRoot\"><dgm:param type=\"hierAlign\" val=\"tCtrCh\"/></dgm:alg>" };
        format!("<dgm:layoutNode name=\"diagram\"><dgm:varLst><dgm:chPref val=\"1\"/><dgm:dir/><dgm:animOne val=\"branch\"/><dgm:animLvl val=\"lvl\"/><dgm:resizeHandles/></dgm:varLst><dgm:alg type=\"hierChild\">{dir}</dgm:alg>{empty_shape}<dgm:presOf/>\
<dgm:constrLst><dgm:constr type=\"w\" for=\"des\" forName=\"nodeText\" refType=\"w\"/><dgm:constr type=\"h\" for=\"des\" forName=\"nodeText\" refType=\"w\" refFor=\"des\" refForName=\"nodeText\" fact=\"0.5\"/><dgm:constr type=\"primFontSz\" for=\"des\" forName=\"nodeText\" op=\"equ\" val=\"65\"/><dgm:constr type=\"sibSp\" refType=\"w\" refFor=\"des\" refForName=\"nodeText\" fact=\"0.2\"/><dgm:constr type=\"sp\" for=\"des\" forName=\"hierChild\" refType=\"w\" refFor=\"des\" refForName=\"nodeText\" fact=\"0.3\"/></dgm:constrLst><dgm:ruleLst/>\
<dgm:forEach name=\"hierForEach\" axis=\"ch\" ptType=\"node\"><dgm:layoutNode name=\"hierRoot\">{root_alg}{empty_shape}<dgm:presOf/><dgm:constrLst/><dgm:ruleLst/>{}\
<dgm:layoutNode name=\"hierChild\"><dgm:alg type=\"hierChild\">{dir}</dgm:alg>{empty_shape}<dgm:presOf/><dgm:constrLst/><dgm:ruleLst/>\
<dgm:forEach name=\"parTransForEach\" axis=\"ch\" ptType=\"parTrans\" cnt=\"1\"><dgm:layoutNode name=\"parTrans\" styleLbl=\"parChTrans1D1\"><dgm:alg type=\"conn\"><dgm:param type=\"dim\" val=\"1D\"/><dgm:param type=\"endSty\" val=\"noArr\"/><dgm:param type=\"connRout\" val=\"bend\"/></dgm:alg><dgm:shape {SHAPE_NS} type=\"conn\" r:blip=\"\"><dgm:adjLst/></dgm:shape><dgm:presOf axis=\"self\"/><dgm:constrLst><dgm:constr type=\"begPad\"/><dgm:constr type=\"endPad\"/></dgm:constrLst><dgm:ruleLst/></dgm:layoutNode></dgm:forEach>\
<dgm:forEach name=\"childForEach\" ref=\"hierForEach\"/></dgm:layoutNode></dgm:layoutNode></dgm:forEach></dgm:layoutNode>",
            text_node("nodeText", shape, "node1").replace("axis=\"desOrSelf\"", "axis=\"self\""))
    } else {
        let (w_fact, h_fact) = match cat { "pyramid" => ("1", "0.35"), "cycle" | "relationship" => ("0.6", "0.6"), _ => ("1", "0.6") };
        let sp = if trans.is_empty() { "<dgm:constr type=\"w\" for=\"ch\" forName=\"sibTrans\" refType=\"w\" refFor=\"ch\" refForName=\"node\" fact=\"0.1\"/>" } else { "<dgm:constr type=\"w\" for=\"ch\" forName=\"sibTrans\" refType=\"w\" refFor=\"ch\" refForName=\"node\" fact=\"0.3\"/><dgm:constr type=\"h\" for=\"ch\" forName=\"sibTrans\" refType=\"h\" refFor=\"ch\" refForName=\"node\" op=\"lte\" fact=\"0.5\"/><dgm:constr type=\"primFontSz\" for=\"des\" forName=\"connectorText\" op=\"equ\" val=\"55\"/>" };
        let extra = match cat { "cycle" | "relationship" if alg.contains("cycle") => "<dgm:constr type=\"diam\" for=\"ch\" refType=\"w\" fact=\"-1\"/>", _ => "" };
        format!("<dgm:layoutNode name=\"diagram\"><dgm:varLst><dgm:dir/><dgm:animLvl val=\"lvl\"/><dgm:resizeHandles val=\"exact\"/></dgm:varLst>{alg}{empty_shape}<dgm:presOf/>\
<dgm:constrLst><dgm:constr type=\"w\" for=\"ch\" forName=\"node\" refType=\"w\" fact=\"{w_fact}\"/><dgm:constr type=\"h\" for=\"ch\" forName=\"node\" refType=\"w\" refFor=\"ch\" refForName=\"node\" fact=\"{h_fact}\"/>{sp}{extra}<dgm:constr type=\"primFontSz\" for=\"ch\" forName=\"node\" op=\"equ\" val=\"65\"/></dgm:constrLst>\
<dgm:ruleLst><dgm:rule type=\"w\" for=\"ch\" forName=\"node\" val=\"0\" fact=\"NaN\" max=\"NaN\"/></dgm:ruleLst>\
<dgm:forEach name=\"nodesForEach\" axis=\"ch\" ptType=\"node\">{}{}</dgm:forEach></dgm:layoutNode>",
            text_node("node", shape, "node1"), sib_trans(trans))
    };
    format!("{head}{body}</dgm:layoutDef>")
}

/// The diagram parts and frame for a SmartArt group.
pub fn write(w: &mut Workbook, si: usize, o: &Object, g: &Group, sink: &mut Sink) -> Option<String> {
    let layout = o.descr.strip_prefix(TAG)?.trim().to_string();
    let outer = crate::group::sheet_rect(&w.sheets[si], o);
    let (fw, fh) = (outer[2] - outer[0], outer[3] - outer[1]);
    let frame = [0.0, 0.0, fw, fh];
    let shapes: Vec<(&Shape, [f32; 4])> = g.children.iter().filter_map(|c| match &c.kind { ObjKind::Shape(s) => Some((s, g.child_rect(c, frame))), _ => None }).collect();
    if shapes.is_empty() { return None; }
    let seed = o.id;
    let doc = guid(seed, 0);
    // Parts and relationships first: the data model names the drawing's relationship.
    let paths: Vec<String> = ["data", "layout", "quickStyle", "colors", "drawing"].iter().map(|n| (sink.free)(&format!("xl/diagrams/{n}"), ".xml")).collect();
    let kinds = [format!("{RT}diagramData"), format!("{RT}diagramLayout"), format!("{RT}diagramQuickStyle"), format!("{RT}diagramColors"), RT_DRAWING.to_string()];
    let ids: Vec<String> = paths.iter().zip(kinds.iter()).map(|(p, k)| (sink.add_rel)(k, p.clone(), false)).collect();
    // Data model: the document point, one node per box with text (top level, in order).
    let mut pts = format!("<dgm:pt modelId=\"{doc}\" type=\"doc\"><dgm:prSet loTypeId=\"{}\" loCatId=\"{}\" qsTypeId=\"{URN}2005/8/quickstyle/simple1\" qsCatId=\"simple\" csTypeId=\"{URN}2005/8/colors/accent1_2\" csCatId=\"accent1\"/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\"/></a:p></dgm:t></dgm:pt>", attr(&layout_urn(&layout)), family(layout_urn(&layout).strip_prefix(URN).unwrap_or("")).3);
    let mut cxns = String::new();
    let mut model_of = vec![];
    let mut ord = 0;
    for (k, (s, _)) in shapes.iter().enumerate() {
        let id = guid(seed, 100 + k);
        model_of.push(id.clone());
        if s.text.trim().is_empty() { continue; }
        let (par, sib) = (guid(seed, 1000 + k), guid(seed, 2000 + k));
        pts.push_str(&format!("<dgm:pt modelId=\"{id}\"><dgm:prSet phldrT=\"[Text]\"/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/>{}</dgm:t></dgm:pt>", s.text.split('\n').map(|l| format!("<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p>", text(l))).collect::<String>()));
        pts.push_str(&format!("<dgm:pt modelId=\"{par}\" type=\"parTrans\" cxnId=\"{}\"><dgm:prSet/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\"/></a:p></dgm:t></dgm:pt>", guid(seed, 3000 + k)));
        pts.push_str(&format!("<dgm:pt modelId=\"{sib}\" type=\"sibTrans\" cxnId=\"{}\"><dgm:prSet/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\"/></a:p></dgm:t></dgm:pt>", guid(seed, 3000 + k)));
        cxns.push_str(&format!("<dgm:cxn modelId=\"{}\" srcId=\"{doc}\" destId=\"{id}\" srcOrd=\"{ord}\" destOrd=\"0\" parTransId=\"{par}\" sibTransId=\"{sib}\"/>", guid(seed, 3000 + k)));
        ord += 1;
    }
    let data = format!("{DECL}<dgm:dataModel xmlns:dgm=\"{NS_DGM}\" xmlns:a=\"{NS_A}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><dgm:ptLst>{pts}</dgm:ptLst><dgm:cxnLst>{cxns}</dgm:cxnLst><dgm:bg/><dgm:whole/><dgm:extLst><a:ext uri=\"http://schemas.microsoft.com/office/drawing/2008/diagram\"><dsp:dataModelExt xmlns:dsp=\"{NS_DSP}\" relId=\"{}\" minVer=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/></a:ext></dgm:extLst></dgm:dataModel>", ids[4]);
    let lo = layout_def(&layout_urn(&layout));
    let style = |lbl: &str| format!("<dgm:styleLbl name=\"{lbl}\"><dgm:scene3d><a:camera prst=\"orthographicFront\"/><a:lightRig rig=\"threePt\" dir=\"t\"/></dgm:scene3d><dgm:sp3d/><dgm:txPr/><dgm:style><a:lnRef idx=\"2\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:lnRef><a:fillRef idx=\"1\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:fillRef><a:effectRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/></a:fontRef></dgm:style></dgm:styleLbl>");
    let qs = format!("{DECL}<dgm:styleDef xmlns:dgm=\"{NS_DGM}\" xmlns:a=\"{NS_A}\" uniqueId=\"{URN}2005/8/quickstyle/simple1\"><dgm:title val=\"\"/><dgm:desc val=\"\"/><dgm:catLst><dgm:cat type=\"simple\" pri=\"10100\"/></dgm:catLst><dgm:scene3d><a:camera prst=\"orthographicFront\"/><a:lightRig rig=\"threePt\" dir=\"t\"/></dgm:scene3d><dgm:styleLbl name=\"node0\"><dgm:scene3d><a:camera prst=\"orthographicFront\"/><a:lightRig rig=\"threePt\" dir=\"t\"/></dgm:scene3d><dgm:sp3d/><dgm:txPr/><dgm:style><a:lnRef idx=\"2\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:lnRef><a:fillRef idx=\"1\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:fillRef><a:effectRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/></a:fontRef></dgm:style></dgm:styleLbl>{}{}{}</dgm:styleDef>", style("node1"), style("sibTrans2D1"), style("parChTrans1D1").replace("<a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/>", "<a:fontRef idx=\"minor\"><a:schemeClr val=\"tx1\"/>"));
    let cs = format!("{DECL}<dgm:colorsDef xmlns:dgm=\"{NS_DGM}\" xmlns:a=\"{NS_A}\" uniqueId=\"{URN}2005/8/colors/accent1_2\"><dgm:title val=\"\"/><dgm:desc val=\"\"/><dgm:catLst><dgm:cat type=\"accent1\" pri=\"11200\"/></dgm:catLst><dgm:styleLbl name=\"node1\"><dgm:fillClrLst meth=\"repeat\"><a:schemeClr val=\"accent1\"/></dgm:fillClrLst><dgm:linClrLst meth=\"repeat\"><a:schemeClr val=\"lt1\"/></dgm:linClrLst><dgm:effectClrLst/><dgm:txLinClrLst/><dgm:txFillClrLst meth=\"repeat\"><a:schemeClr val=\"lt1\"/></dgm:txFillClrLst><dgm:txEffectClrLst/></dgm:styleLbl><dgm:styleLbl name=\"sibTrans2D1\"><dgm:fillClrLst meth=\"repeat\"><a:schemeClr val=\"accent1\"><a:tint val=\"60000\"/></a:schemeClr></dgm:fillClrLst><dgm:linClrLst meth=\"repeat\"><a:schemeClr val=\"accent1\"><a:tint val=\"60000\"/></a:schemeClr></dgm:linClrLst><dgm:effectClrLst/><dgm:txLinClrLst/><dgm:txFillClrLst meth=\"repeat\"><a:schemeClr val=\"lt1\"/></dgm:txFillClrLst><dgm:txEffectClrLst/></dgm:styleLbl><dgm:styleLbl name=\"parChTrans1D1\"><dgm:fillClrLst meth=\"repeat\"><a:schemeClr val=\"accent1\"/></dgm:fillClrLst><dgm:linClrLst meth=\"repeat\"><a:schemeClr val=\"accent1\"><a:shade val=\"60000\"/></a:schemeClr></dgm:linClrLst><dgm:effectClrLst/><dgm:txLinClrLst/><dgm:txFillClrLst meth=\"repeat\"><a:schemeClr val=\"tx1\"/></dgm:txFillClrLst><dgm:txEffectClrLst/></dgm:styleLbl></dgm:colorsDef>");
    // The drawing: the group's members as rendered shapes, in the frame's space.
    let mut d = format!("{DECL}<dsp:drawing xmlns:dgm=\"{NS_DGM}\" xmlns:dsp=\"{NS_DSP}\" xmlns:a=\"{NS_A}\"><dsp:spTree><dsp:nvGrpSpPr><dsp:cNvPr id=\"0\" name=\"\"/><dsp:cNvGrpSpPr/></dsp:nvGrpSpPr><dsp:grpSpPr/>");
    for (k, (s, r)) in shapes.iter().enumerate() {
        let fill = s.fill.map(|c| format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hexs(c))).unwrap_or_else(|| "<a:noFill/>".into());
        let line = s.line.map(|c| format!("<a:ln w=\"12700\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>", hexs(c))).unwrap_or_else(|| "<a:ln><a:noFill/></a:ln>".into());
        let av: String = s.adjust.iter().map(|(n, v)| format!("<a:gd name=\"{}\" fmla=\"val {}\"/>", attr(n), v.round() as i64)).collect();
        let xf = format!("<a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/>", emu(r[0]), emu(r[1]), emu(r[2] - r[0]), emu(r[3] - r[1]));
        let body = if s.text.is_empty() { String::new() } else { format!("<dsp:txBody><a:bodyPr spcFirstLastPara=\"0\" vert=\"horz\" wrap=\"square\" lIns=\"38100\" tIns=\"38100\" rIns=\"38100\" bIns=\"38100\" numCol=\"1\" spcCol=\"1270\" anchor=\"ctr\" anchorCtr=\"0\"><a:noAutofit/></a:bodyPr><a:lstStyle/>{}</dsp:txBody><dsp:txXfrm>{xf}</dsp:txXfrm>", paras(&s.text, s)) };
        d.push_str(&format!("<dsp:sp modelId=\"{}\"><dsp:nvSpPr><dsp:cNvPr id=\"0\" name=\"\"/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr><a:xfrm>{xf}</a:xfrm><a:prstGeom prst=\"{}\"><a:avLst>{av}</a:avLst></a:prstGeom>{fill}{line}</dsp:spPr>{body}</dsp:sp>",
            model_of[k], attr(if s.preset.is_empty() { "rect" } else { &s.preset })));
    }
    d.push_str("</dsp:spTree></dsp:drawing>");
    let types = [format!("{CT}diagramData+xml"), format!("{CT}diagramLayout+xml"), format!("{CT}diagramStyle+xml"), format!("{CT}diagramColors+xml"), CT_DRAWING.to_string()];
    for ((p, x), t) in paths.iter().zip([data, lo, qs, cs, d]).zip(types) { w.parts.insert(p.clone(), x.into_bytes()); sink.overrides.push((p.clone(), t)); }
    Some(format!("<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{}\" name=\"{}\" descr=\"{}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{NS_DGM}\"><dgm:relIds xmlns:dgm=\"{NS_DGM}\" r:dm=\"{}\" r:lo=\"{}\" r:qs=\"{}\" r:cs=\"{}\"/></a:graphicData></a:graphic></xdr:graphicFrame>",
        o.id, attr(&o.name), attr(&o.descr), emu(fw), emu(fh), ids[0], ids[1], ids[2], ids[3]))
}

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }
fn parse(wb: &Workbook, p: &str) -> Option<Element> { wb.parts.get(p).and_then(|b| Element::parse(&part_text(b)).ok()) }

/// The diagram drawing part of a data model: the relationship its dataModelExt names, looked up
/// in the drawing part whose relationships also lead to that data model.
fn drawing_part(wb: &Workbook, data_part: &str, rel_id: &str) -> Option<String> {
    for (k, v) in &wb.parts {
        if !(k.starts_with("xl/drawings/_rels/") && k.ends_with(".rels")) { continue; }
        let Ok(e) = Element::parse(&part_text(v)) else { continue };
        let owner = format!("xl/drawings/{}", k.trim_start_matches("xl/drawings/_rels/").trim_end_matches(".rels"));
        let rs: Vec<(&str, &str, String)> = e.all("Relationship").filter_map(|r| Some((r.attr("Id")?, r.attr("Type")?, super::read::resolve(&owner, r.attr("Target")?)))).collect();
        if !rs.iter().any(|r| r.2 == data_part) { continue; }
        if let Some(r) = rs.iter().find(|r| r.0 == rel_id && r.1.ends_with("/diagramDrawing")) { return Some(r.2.clone()); }
    }
    None
}

/// A SmartArt graphic frame → a group of its rendered shapes.
pub fn read(wb: &Workbook, _si: usize, content: &Element, o: &mut Object) -> bool {
    if content.local() != "graphicFrame" { return false; }
    let Some(ids) = find(content, "relIds") else { return false };
    let get = |l: &str| ids.attrs.iter().find(|(k, _)| k.ends_with(&format!(":{l}"))).and_then(|(_, v)| o.rels.iter().find(|r| &r.id == v && !r.external)).map(|r| r.target.clone());
    let Some(dm) = get("dm") else { return false };
    let Some(data) = parse(wb, &dm) else { return false };
    let layout = get("lo").and_then(|p| parse(wb, &p)).and_then(|l| l.attr("uniqueId").map(String::from))
        .or_else(|| find(&data, "prSet").and_then(|p| p.attr("loTypeId")).map(String::from)).unwrap_or_default();
    let rel = find(&data, "dataModelExt").and_then(|e| e.attr("relId")).unwrap_or("");
    let Some(sp_tree) = drawing_part(wb, &dm, rel).and_then(|p| parse(wb, &p)) else { return false };
    let mut items = vec![];
    for (k, sp) in sp_tree.child("spTree").into_iter().flat_map(|t| t.all("sp")).enumerate() {
        let x = sp.path("spPr/xfrm");
        let v = |e: Option<&Element>, a: &str| e.and_then(|e| e.attr_f64(a)).unwrap_or(0.0) as f32 / EMU;
        let (off, ext) = (x.and_then(|x| x.child("off")), x.and_then(|x| x.child("ext")));
        let r = [v(off, "x"), v(off, "y"), v(off, "x") + v(ext, "cx"), v(off, "y") + v(ext, "cy")];
        let mut s = crate::drawing::parse_shape(sp, &wb.theme_colors);
        s.center = s.center || !s.text.is_empty();
        let mut c = Object::new(ObjKind::Shape(s), Anchor::Abs { x: r[0], y: r[1], w: r[2] - r[0], h: r[3] - r[1] });
        c.id = o.id * 100 + k as u32 + 1;
        c.name = format!("SmartArt Shape {}", k + 1);
        c.descr = format!("{TAG}{}", layout_id(&layout));
        items.push((c, r));
    }
    if items.is_empty() { return false; }
    let (mut g, _) = crate::group::make(items);
    // The frame's own size is the child space when known (shapes needn't fill it).
    let fx = content.path("xfrm/ext");
    let (fw, fh) = (fx.and_then(|e| e.attr_f64("cx")).unwrap_or(0.0) as f32 / EMU, fx.and_then(|e| e.attr_f64("cy")).unwrap_or(0.0) as f32 / EMU);
    if fw > 0.0 && fh > 0.0 { g.ch = [0.0, 0.0, fw, fh]; }
    o.kind = ObjKind::Group(g);
    o.descr = format!("{TAG}{}", layout_id(&layout));
    o.raw = None;
    o.rels.clear();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{add_object, Mark};

    #[test]
    fn every_layout_writes_a_full_definition() {
        for (id, office) in LAYOUTS {
            let x = layout_def(&layout_urn(id));
            let errs = super::super::schemacheck::check_xml(id, &x);
            assert!(errs.is_empty(), "{errs:?}");
            let e = Element::parse(&x).unwrap();
            assert_eq!(e.attr("uniqueId"), Some(format!("{URN}{office}").as_str()));
            let root = e.child("layoutNode").expect("root layout node");
            assert!(find(root, "alg").is_some() && find(root, "forEach").is_some() && find(root, "constrLst").is_some_and(|c| c.elements().count() > 0), "{id}");
            assert!(x.contains("<dgm:alg type=\"tx\""), "{id}: no text algorithm");
        }
    }

    #[test]
    fn smartart_group_round_trips() {
        let mut wb = Workbook::new();
        let tag = format!("{TAG}basicProcess");
        let items: Vec<(Object, [f32; 4])> = ["Plan", "Build", "Test"].iter().enumerate().map(|(k, t)| {
            let mut c = Object::new(ObjKind::Shape(Shape { text: t.to_string(), preset: "roundRect".into(), fill: Some([0x44, 0x72, 0xC4]), color: Some([255, 255, 255]), size: 14.0, center: true, ..Default::default() }), Anchor::default());
            c.descr = tag.clone();
            (c, [k as f32 * 160.0, 0.0, k as f32 * 160.0 + 140.0, 90.0])
        }).collect();
        let (g, _) = crate::group::make(items);
        let mut o = Object::new(ObjKind::Group(g), Anchor::Two { from: Mark { col: 1, row: 1, ..Default::default() }, to: Mark { col: 8, row: 6, dy: 10.0, ..Default::default() }, edit_as: None });
        o.descr = tag.clone();
        add_object(&mut wb.sheets[0].objects, o, "SmartArt");
        let bytes = crate::xlsx::write(&wb).unwrap();
        let files = super::super::pkgcheck::unzip(&bytes);
        super::super::pkgcheck::check_package(&files);
        for p in ["data1", "layout1", "quickStyle1", "colors1", "drawing1"] { assert!(files.contains_key(&format!("xl/diagrams/{p}.xml")), "{p}"); }
        let lo = &files["xl/diagrams/layout1.xml"];
        assert!(lo.contains("layout/process1") && lo.contains("<dgm:alg type=\"lin\"/>") && lo.contains("<dgm:alg type=\"tx\"/>") && lo.contains("type=\"rightArrow\"") == false && lo.contains("ptType=\"sibTrans\"") && lo.contains("<dgm:sampData>"), "{lo}");
        assert_eq!(files["xl/diagrams/data1.xml"].matches("<dgm:cxn ").count(), 3);
        let d = files.iter().find(|(k, _)| k.starts_with("xl/drawings/drawing")).unwrap().1;
        assert!(d.contains("<dgm:relIds"));
        if let Ok(dir) = std::env::var("JADWAL_DUMP") { std::fs::write(format!("{dir}/smartart.xlsx"), &bytes).unwrap(); }
        let back = crate::xlsx::read(&bytes).unwrap();
        let o = &back.sheets[0].objects[0];
        assert_eq!(o.descr, tag);
        let ObjKind::Group(g) = &o.kind else { panic!("{:?}", o.kind) };
        let texts: Vec<String> = g.children.iter().filter_map(|c| match &c.kind { ObjKind::Shape(s) => Some(s.text.clone()), _ => None }).collect();
        assert_eq!(texts, ["Plan", "Build", "Test"]);
        // Saved again: one set of diagram parts.
        let again = super::super::pkgcheck::unzip(&crate::xlsx::write(&back).unwrap());
        super::super::pkgcheck::check_package(&again);
        assert_eq!(again.keys().filter(|k| k.starts_with("xl/diagrams/data")).count(), 1);
    }
}
