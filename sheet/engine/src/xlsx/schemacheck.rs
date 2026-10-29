//! Test helper: a structural check of written parts against the schemas (ECMA-376 Part 1
//! schemas sml/dml/dgm, [MS-XLSX] x14/x15/threaded comments/rich data, [MS-ODRAWXML] chartex,
//! diagram drawing and 3D models). For each element it knows, it checks the required attributes
//! and that the children are ones the schema allows, in the schema's sequence order. It also
//! checks each part's content type and the type of every relationship naming it against the
//! values Excel writes. Elements it has no rule for are not checked (their children are).

use super::xml::{Element, Node};
use std::collections::BTreeMap;

pub const S: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const X15: &str = "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main";
const TC: &str = "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";
const RD: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
const RD2: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata2";
const RVR: &str = "http://schemas.microsoft.com/office/spreadsheetml/2022/richvaluerel";
const CX: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";
const DGM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const DSP: &str = "http://schemas.microsoft.com/office/drawing/2008/diagram";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const AM3D: &str = "http://schemas.microsoft.com/office/drawing/2017/model3d";
const PR: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CTS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const TSLE: &str = "http://schemas.microsoft.com/office/drawing/2012/timeslicer";
const SLE: &str = "http://schemas.microsoft.com/office/drawing/2010/slicer";
const M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const DS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/customXml";
const DM: &str = "http://schemas.microsoft.com/DataMashup";

/// (namespace, element, child sequence, required attributes). A sequence slot "a|b" allows
/// either; a slot may repeat. A sequence of ["*"] allows any children in any order.
type Rule = (&'static str, &'static str, &'static [&'static str], &'static [&'static str]);

const FILL: &str = "noFill|solidFill|gradFill|blipFill|pattFill|grpFill";
const SP_PR: &[&str] = &["xfrm", "custGeom|prstGeom", FILL, "ln", "effectLst|effectDag", "scene3d", "sp3d", "extLst"];
const COLOR: &[&str] = &["scrgbClr|srgbClr|hslClr|sysClr|schemeClr|prstClr"];

const RULES: &[Rule] = &[
    // ---- package
    (PR, "Relationships", &["Relationship"], &[]),
    (PR, "Relationship", &[], &["Id", "Type", "Target"]),
    (CTS, "Types", &["Default|Override"], &[]),
    (CTS, "Default", &[], &["Extension", "ContentType"]),
    (CTS, "Override", &[], &["PartName", "ContentType"]),
    (MC, "Choice", &["*"], &["Requires"]),
    // ---- SpreadsheetML (sml.xsd)
    (S, "workbook", &["fileVersion", "fileSharing", "workbookPr", "workbookProtection", "bookViews", "sheets", "functionGroups", "externalReferences", "definedNames", "calcPr", "oleSize", "customWorkbookViews", "pivotCaches", "smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"], &[]),
    (S, "sheets", &["sheet"], &[]),
    (S, "sheet", &[], &["name", "sheetId", "id"]),
    (S, "definedName", &[], &["name"]),
    (S, "pivotCache", &[], &["cacheId", "id"]),
    (S, "customWorkbookView", &["extLst"], &["name", "guid", "windowWidth", "windowHeight", "activeSheetId"]),
    (S, "worksheet", &["sheetPr", "dimension", "sheetViews", "sheetFormatPr", "cols", "sheetData", "sheetCalcPr", "sheetProtection", "protectedRanges", "scenarios", "autoFilter", "sortState", "dataConsolidate", "customSheetViews", "mergeCells", "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "drawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"], &[]),
    (S, "sheetPr", &["tabColor", "outlinePr", "pageSetUpPr"], &[]),
    (S, "sheetView", &["pane", "selection", "pivotSelection", "extLst"], &["workbookViewId"]),
    (S, "sheetFormatPr", &[], &["defaultRowHeight"]),
    (S, "col", &[], &["min", "max"]),
    (S, "row", &["c", "extLst"], &[]),
    (S, "c", &["f", "v", "is", "extLst"], &[]),
    (S, "protectedRanges", &["protectedRange"], &[]),
    (S, "protectedRange", &["securityDescriptor"], &["sqref", "name"]),
    (S, "scenarios", &["scenario"], &[]),
    (S, "scenario", &["inputCells"], &["name"]),
    (S, "inputCells", &[], &["r", "val"]),
    (S, "customSheetViews", &["customSheetView"], &[]),
    (S, "customSheetView", &["pane", "selection", "rowBreaks", "colBreaks", "pageMargins", "printOptions", "pageSetup", "headerFooter", "autoFilter", "extLst"], &["guid"]),
    (S, "headerFooter", &["oddHeader", "oddFooter", "evenHeader", "evenFooter", "firstHeader", "firstFooter"], &[]),
    (S, "oleObjects", &["oleObject"], &[]),
    (S, "oleObject", &["objectPr"], &["shapeId"]),
    (S, "objectPr", &["anchor"], &[]),
    (S, "anchor", &["from", "to"], &[]),
    (S, "autoFilter", &["filterColumn", "sortState", "extLst"], &[]),
    (S, "filterColumn", &["filters|top10|customFilters|dynamicFilter|colorFilter|iconFilter", "extLst"], &["colId"]),
    (S, "customFilters", &["customFilter"], &[]),
    (S, "customFilter", &[], &["val"]),
    (S, "comments", &["authors", "commentList", "extLst"], &[]),
    (S, "comment", &["text", "commentPr"], &["ref", "authorId"]),
    (S, "connections", &["connection"], &[]),
    (S, "connection", &["dbPr", "olapPr", "webPr", "textPr", "parameters", "extLst"], &["id"]),
    (S, "textPr", &["textFields"], &[]),
    (S, "webPr", &["tables"], &[]),
    (S, "dbPr", &[], &["connection"]),
    (S, "queryTable", &["queryTableRefresh", "extLst"], &["name", "connectionId"]),
    // ---- custom XML item properties (the Power Query DataMashup's) and the DataMashup root
    (DS, "datastoreItem", &["schemaRefs"], &["itemID"]),
    (DS, "schemaRefs", &["schemaRef"], &[]),
    (DM, "DataMashup", &[], &[]),
    (S, "queryTableRefresh", &["queryTableFields", "queryTableDeletedFields", "sortState", "extLst"], &[]),
    (S, "queryTableFields", &["queryTableField"], &[]),
    (S, "queryTableField", &["extLst"], &["id"]),
    (S, "table", &["autoFilter", "sortState", "tableColumns", "tableStyleInfo", "extLst"], &["id", "name", "displayName", "ref"]),
    (S, "tableColumn", &["calculatedColumnFormula", "totalsRowFormula", "xmlColumnPr", "extLst"], &["id", "name"]),
    (S, "metadata", &["metadataTypes", "metadataStrings", "mdxMetadata", "futureMetadata", "cellMetadata", "valueMetadata", "extLst"], &[]),
    (S, "metadataType", &[], &["name", "minSupportedVersion"]),
    (S, "futureMetadata", &["bk", "extLst"], &["name"]),
    (S, "rc", &[], &["t", "v"]),
    (S, "styleSheet", &["numFmts", "fonts", "fills", "borders", "cellStyleXfs", "cellXfs", "cellStyles", "dxfs", "tableStyles", "colors", "extLst"], &[]),
    (S, "pivotCacheDefinition", &["cacheSource", "cacheFields", "cacheHierarchies", "kpis", "calculatedItems", "tupleCache", "calculatedMembers", "dimensions", "measureGroups", "maps", "extLst"], &[]),
    (S, "cacheSource", &["consolidation|worksheetSource", "extLst"], &["type"]),
    (S, "cacheField", &["sharedItems", "fieldGroup", "mpMap", "extLst"], &["name"]),
    (S, "sharedItems", &["m|n|b|e|s|d"], &[]),
    (S, "fieldGroup", &["rangePr", "discretePr", "groupItems"], &[]),
    (S, "pivotCacheRecords", &["r", "extLst"], &[]),
    (S, "pivotTableDefinition", &["location", "pivotFields", "rowFields", "rowItems", "colFields", "colItems", "pageFields", "dataFields", "formats", "conditionalFormats", "chartFormats", "pivotHierarchies", "pivotTableStyleInfo", "filters", "rowHierarchiesUsage", "colHierarchiesUsage", "extLst"], &["name", "cacheId", "dataCaption"]),
    (S, "location", &[], &["ref", "firstHeaderRow", "firstDataRow", "firstDataCol"]),
    (S, "pivotField", &["items", "autoSortScope", "extLst"], &[]),
    (S, "pageField", &["extLst"], &["fld"]),
    (S, "dataField", &["extLst"], &["fld"]),
    (S, "filters", &["filter|blank|dateGroupItem"], &[]),
    (S, "filter", &["autoFilter", "extLst"], &[]),
    // ---- x14 (sparklines, slicers)
    (X14, "sparklineGroup", &["colorSeries", "colorNegative", "colorAxis", "colorMarkers", "colorFirst", "colorLast", "colorHigh", "colorLow", "f", "sparklines"], &[]),
    (X14, "sparklines", &["sparkline"], &[]),
    (X14, "sparkline", &["f", "sqref"], &[]),
    (X14, "slicerCacheDefinition", &["pivotTables", "data", "extLst"], &["name", "sourceName"]),
    (X14, "tabular", &["items", "extLst"], &["pivotCacheId"]),
    (X14, "slicers", &["slicer"], &[]),
    (X14, "slicers>slicer", &["extLst"], &["name", "cache", "rowHeight"]),
    (X14, "pivotCacheDefinition", &[], &[]),
    // ---- x15 (timelines)
    (X15, "timelineCacheDefinition", &["pivotTables", "state", "extLst"], &["name", "sourceName"]),
    (X15, "state", &["selection", "bounds", "extLst"], &["pivotCacheId"]),
    (X15, "selection", &[], &["startDate", "endDate"]),
    (X15, "bounds", &[], &["startDate", "endDate"]),
    (X15, "timelines", &["timeline"], &[]),
    (X15, "timeline", &["extLst"], &["name", "cache", "level", "selectionLevel"]),
    (X15, "pivotFilter", &[], &[]),
    // ---- threaded comments, persons
    (TC, "ThreadedComments", &["threadedComment", "extLst"], &[]),
    (TC, "threadedComment", &["text", "mentions", "extLst"], &["ref", "dT", "personId", "id"]),
    (TC, "mention", &[], &["mentionpersonId", "mentionId", "startIndex", "length"]),
    (TC, "personList", &["person", "extLst"], &[]),
    (TC, "person", &["extLst"], &["displayName", "id"]),
    // ---- rich data (in-cell pictures)
    (RD, "rvData", &["rv", "extLst"], &[]),
    (RD, "rv", &["v", "fb"], &["s"]),
    (RD, "rvStructures", &["s", "extLst"], &[]),
    (RD, "s", &["k"], &["t"]),
    (RD, "k", &[], &["n"]),
    (RD2, "rvTypesInfo", &["global", "types", "extLst"], &[]),
    (RVR, "richValueRels", &["rel", "extLst"], &[]),
    (RVR, "rel", &[], &["id"]),
    // ---- chartex ([MS-ODRAWXML] 2.24)
    (CX, "chartSpace", &["chartData", "chart", "spPr", "txPr", "clrMapOvr", "fmtOvrs", "printSettings", "extLst"], &[]),
    (CX, "chartData", &["externalData", "data"], &[]),
    (CX, "data", &["numDim|strDim", "extLst"], &["id"]),
    (CX, "numDim", &["f", "nf", "lvl"], &["type"]),
    (CX, "strDim", &["f", "nf", "lvl"], &["type"]),
    (CX, "lvl", &["pt"], &["ptCount"]),
    (CX, "chart", &["title", "plotArea", "legend", "extLst"], &[]),
    (CX, "title", &["tx", "spPr", "txPr", "extLst"], &[]),
    (CX, "plotArea", &["plotAreaRegion", "axis", "spPr", "extLst"], &[]),
    (CX, "plotAreaRegion", &["plotSurface", "series", "extLst"], &[]),
    (CX, "series", &["tx", "spPr", "valueColors", "valueColorPositions", "dataPt", "dataLabels", "dataId", "layoutPr", "axisId", "extLst"], &["layoutId"]),
    (CX, "valueColors", &["minColor", "midColor", "maxColor"], &[]),
    (CX, "valueColorPositions", &["min", "mid", "max"], &[]),
    (CX, "layoutPr", &["parentLabelLayout", "regionLabelLayout", "visibility", "aggregation", "binning", "geography", "statistics", "subtotals", "extLst"], &[]),
    (CX, "geography", &["geoCache", "extLst"], &["cultureLanguage", "cultureRegion", "attribution"]),
    (CX, "geoCache", &["binary|clear"], &["provider"]),
    (CX, "clear", &["geoLocationQueryResults", "geoDataEntityQueryResults", "geoDataPointToEntityQueryResults", "geoChildEntitiesQueryResults", "geoParentEntitiesQueryResults", "geoChildTypesQueryResults"], &[]),
    (CX, "geoLocationQueryResults", &["geoLocationQueryResult"], &[]),
    (CX, "geoLocationQueryResult", &["geoLocationQuery", "geoLocations"], &[]),
    (CX, "geoLocationQuery", &[], &["entityType"]),
    (CX, "geoLocations", &["geoLocation"], &[]),
    (CX, "geoLocation", &["address"], &["latitude", "longitude", "entityName", "entityType"]),
    (CX, "geoDataEntityQueryResults", &["geoDataEntityQueryResult"], &[]),
    (CX, "geoDataEntityQueryResult", &["geoDataEntityQuery", "geoData"], &[]),
    (CX, "geoDataEntityQuery", &[], &["entityType", "entityId"]),
    (CX, "geoData", &["geoPolygons", "copyrights"], &["entityName", "entityId", "east", "west", "north", "south"]),
    (CX, "geoPolygons", &["geoPolygon"], &[]),
    (CX, "geoPolygon", &[], &["polygonId", "numPoints", "pcaRings"]),
    (CX, "copyrights", &["copyright"], &[]),
    (CX, "geoDataPointToEntityQueryResults", &["geoDataPointToEntityQueryResult"], &[]),
    (CX, "geoDataPointToEntityQueryResult", &["geoDataPointQuery", "geoDataPointToEntityQuery"], &[]),
    (CX, "geoDataPointQuery", &[], &["entityType", "latitude", "longitude"]),
    (CX, "geoDataPointToEntityQuery", &[], &["entityType", "entityId"]),
    (CX, "dataId", &[], &["val"]),
    (CX, "axis", &["catScaling|valScaling", "title", "units", "majorGridlines", "minorGridlines", "majorTickMarks", "minorTickMarks", "tickLabels", "numFmt", "spPr", "txPr", "extLst"], &["id"]),
    (CX, "legend", &["spPr", "txPr", "extLst"], &[]),
    (CX, "dataLabels", &["numFmt", "spPr", "txPr", "visibility", "separator", "dataLabel", "dataLabelHidden", "extLst"], &[]),
    (CX, "dataPt", &["spPr", "extLst"], &["idx"]),
    (CX, "spPr", SP_PR, &[]),
    // ---- diagrams (dml-diagram.xsd)
    (DGM, "dataModel", &["ptLst", "cxnLst", "bg", "whole", "extLst"], &[]),
    (DGM, "pt", &["prSet", "spPr", "t", "extLst"], &["modelId"]),
    (DGM, "cxn", &["extLst"], &["modelId", "srcId", "destId"]),
    (DGM, "layoutDef", &["title", "desc", "catLst", "sampData", "styleData", "clrData", "layoutNode", "extLst"], &[]),
    (DGM, "layoutNode", &["*"], &[]),
    (DGM, "forEach", &["*"], &[]),
    (DGM, "alg", &["param", "extLst"], &["type"]),
    (DGM, "param", &[], &["type", "val"]),
    (DGM, "shape", &["adjLst", "extLst"], &[]),
    (DGM, "constrLst", &["constr"], &[]),
    (DGM, "constr", &["extLst"], &["type"]),
    (DGM, "ruleLst", &["rule"], &[]),
    (DGM, "rule", &["extLst"], &["type"]),
    (DGM, "choose", &["if", "else"], &[]),
    (DGM, "if", &["*"], &["func", "op", "val"]),
    (DGM, "styleDef", &["title", "desc", "catLst", "scene3d", "styleLbl", "extLst"], &["uniqueId"]),
    (DGM, "colorsDef", &["title", "desc", "catLst", "styleLbl", "extLst"], &["uniqueId"]),
    (DGM, "relIds", &[], &["dm", "lo", "qs", "cs"]),
    (DGM, "cat", &[], &["type", "pri"]),
    (DSP, "drawing", &["spTree"], &[]),
    (DSP, "spTree", &["nvGrpSpPr", "grpSpPr", "sp|grpSp", "extLst"], &[]),
    (DSP, "sp", &["nvSpPr", "spPr", "style", "txBody", "txXfrm", "extLst"], &["modelId"]),
    (DSP, "spPr", SP_PR, &[]),
    // ---- spreadsheet drawing
    (XDR, "wsDr", &["twoCellAnchor|oneCellAnchor|absoluteAnchor"], &[]),
    (XDR, "twoCellAnchor", &["from", "to", "sp|grpSp|graphicFrame|cxnSp|pic|contentPart", "clientData"], &[]),
    (XDR, "oneCellAnchor", &["from", "ext", "sp|grpSp|graphicFrame|cxnSp|pic|contentPart", "clientData"], &[]),
    (XDR, "absoluteAnchor", &["pos", "ext", "sp|grpSp|graphicFrame|cxnSp|pic|contentPart", "clientData"], &[]),
    (XDR, "from", &["col", "colOff", "row", "rowOff"], &[]),
    (XDR, "to", &["col", "colOff", "row", "rowOff"], &[]),
    (XDR, "sp", &["nvSpPr", "spPr", "style", "txBody"], &[]),
    (XDR, "pic", &["nvPicPr", "blipFill", "spPr", "style"], &[]),
    (XDR, "graphicFrame", &["nvGraphicFramePr", "xfrm", "graphic"], &[]),
    (XDR, "grpSp", &["nvGrpSpPr", "grpSpPr", "sp|grpSp|graphicFrame|cxnSp|pic|contentPart"], &[]),
    (XDR, "cxnSp", &["nvCxnSpPr", "spPr", "style"], &[]),
    (XDR, "cNvPr", &["hlinkClick", "hlinkHover", "extLst"], &["id", "name"]),
    (XDR, "spPr", SP_PR, &[]),
    (XDR, "blipFill", &["blip", "srcRect", "tile|stretch"], &[]),
    (A, "graphicData", &["*"], &["uri"]),
    (A, "xfrm", &["off", "ext", "chOff", "chExt"], &[]),
    (A, "prstGeom", &["avLst"], &["prst"]),
    (A, "gd", &[], &["name", "fmla"]),
    (A, "ln", &[FILL, "prstDash|custDash", "round|bevel|miter", "headEnd", "tailEnd", "extLst"], &[]),
    (A, "solidFill", COLOR, &[]),
    (A, "srgbClr", &["*"], &["val"]),
    (A, "schemeClr", &["*"], &["val"]),
    (A, "blip", &["*"], &[]),
    (A, "bodyPr", &["prstTxWarp", "noAutofit|normAutofit|spAutoFit", "scene3d", "sp3d", "flatTx", "extLst"], &[]),
    (A, "p", &["pPr", "r|br|fld|m", "endParaRPr"], &[]),
    (A, "r", &["rPr", "t"], &[]),
    (A, "rPr", &["ln", FILL, "effectLst|effectDag", "highlight", "uLnTx|uLn", "uFillTx|uFill", "latin", "ea", "cs", "sym", "hlinkClick", "hlinkMouseOver", "rtl", "extLst"], &[]),
    (A, "latin", &[], &["typeface"]),
    // ---- theme (dml-main.xsd)
    (A, "theme", &["themeElements", "objectDefaults", "extraClrSchemeLst", "custClrLst", "extLst"], &[]),
    (A, "themeElements", &["clrScheme", "fontScheme", "fmtScheme", "extLst"], &[]),
    (A, "clrScheme", &["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink", "extLst"], &["name"]),
    (A, "fontScheme", &["majorFont", "minorFont", "extLst"], &["name"]),
    (A, "majorFont", &["latin", "ea", "cs", "font", "extLst"], &[]),
    (A, "minorFont", &["latin", "ea", "cs", "font", "extLst"], &[]),
    (A, "font", &[], &["script", "typeface"]),
    (A, "fmtScheme", &["fillStyleLst", "lnStyleLst", "effectStyleLst", "bgFillStyleLst"], &[]),
    // ---- 3D models
    (AM3D, "model3d", &["spPr", "camera", "trans", "attrSrcUrl", "raster", "objViewport", "ambientLight", "ptLight", "extLst"], &["embed"]),
    (AM3D, "camera", &["pos", "up", "lookAt", "perspective|orthographic"], &[]),
    (AM3D, "trans", &["meterPerModelUnit", "preTrans", "scale", "rot", "postTrans"], &[]),
    (AM3D, "scale", &["sx", "sy", "sz"], &[]),
    (AM3D, "raster", &["blip"], &["rName", "rVer"]),
    (AM3D, "spPr", SP_PR, &[]),
    // ---- frames naming other parts
    (TSLE, "timeslicer", &["extLst"], &["name"]),
    (SLE, "slicer", &["extLst"], &["name"]),
    (M, "oMathPara", &["oMathParaPr", "oMath"], &[]),
];

/// The rule for an element; a rule named "parent>name" applies only under that parent.
fn rule(ns: &str, parent: &str, name: &str) -> Option<&'static Rule> {
    let scoped = format!("{parent}>{name}");
    RULES.iter().find(|r| r.0 == ns && r.1 == scoped).or_else(|| RULES.iter().find(|r| r.0 == ns && r.1 == name))
}

fn local(n: &str) -> &str { n.rsplit(':').next().unwrap_or(n) }
fn prefix(n: &str) -> &str { n.find(':').map_or("", |i| &n[..i]) }

/// The name a child counts as for its parent's sequence: an `mc:AlternateContent` counts as
/// the first element of its first Choice.
fn slot_name(c: &Element) -> Option<String> {
    if local(&c.name) == "AlternateContent" { return c.elements().next().and_then(|ch| ch.elements().next()).map(|e| local(&e.name).to_string()); }
    Some(local(&c.name).to_string())
}

fn walk(e: &Element, scope: &mut Vec<(String, String)>, path: &str, errs: &mut Vec<String>) {
    let mark = scope.len();
    for (k, v) in &e.attrs {
        if k == "xmlns" { scope.push((String::new(), v.clone())); } else if let Some(p) = k.strip_prefix("xmlns:") { scope.push((p.into(), v.clone())); }
    }
    let ns = scope.iter().rev().find(|(p, _)| p == prefix(&e.name)).map(|(_, u)| u.clone()).unwrap_or_default();
    let name = local(&e.name);
    let here = format!("{path}/{name}");
    let parent = path.rsplit('/').next().unwrap_or("");
    if let Some(&(_, _, seq, req)) = rule(&ns, parent, name) {
        for r in req { if !e.attrs.iter().any(|(k, _)| !k.starts_with("xmlns") && local(k) == *r) { errs.push(format!("{here}: missing required attribute {r}")); } }
        if seq != ["*"] {
            let mut last = 0usize;
            for c in e.elements() {
                let Some(cn) = slot_name(c) else { continue };
                match seq.iter().position(|s| s.split('|').any(|x| x == cn)) {
                    None => errs.push(format!("{here}: child {cn} not allowed")),
                    Some(i) if i < last => errs.push(format!("{here}: child {cn} out of order (after {})", seq[last])),
                    Some(i) => last = i,
                }
            }
        }
    }
    for c in e.children.iter() { if let Node::Element(c) = c { walk(c, scope, &here, errs); } }
    scope.truncate(mark);
}

/// Structural errors of one XML part.
pub fn check_xml(name: &str, src: &str) -> Vec<String> {
    let Ok(root) = Element::parse(src) else { return vec![format!("{name}: not well-formed")] };
    let mut errs = vec![];
    walk(&root, &mut vec![("xml".into(), "http://www.w3.org/XML/1998/namespace".into())], "", &mut errs);
    errs.into_iter().map(|e| format!("{name}: {e}")).collect()
}

const CT: &str = "application/vnd.openxmlformats-officedocument.";
const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

/// (part name prefix, content type, relationship type) as Excel writes them.
fn expected(part: &str) -> Option<(String, String)> {
    let t: &[(&str, &str, &str)] = &[
        ("xl/worksheets/sheet", "{CT}spreadsheetml.worksheet+xml", "{RT}worksheet"),
        ("xl/drawings/drawing", "{CT}drawing+xml", "{RT}drawing"),
        ("xl/charts/chartEx", "application/vnd.ms-office.chartex+xml", "http://schemas.microsoft.com/office/2014/relationships/chartEx"),
        ("xl/charts/chart", "{CT}drawingml.chart+xml", "{RT}chart"),
        ("xl/charts/style", "application/vnd.ms-office.chartstyle+xml", "http://schemas.microsoft.com/office/2011/relationships/chartStyle"),
        ("xl/charts/colors", "application/vnd.ms-office.chartcolorstyle+xml", "http://schemas.microsoft.com/office/2011/relationships/chartColorStyle"),
        ("xl/diagrams/data", "{CT}drawingml.diagramData+xml", "{RT}diagramData"),
        ("xl/diagrams/layout", "{CT}drawingml.diagramLayout+xml", "{RT}diagramLayout"),
        ("xl/diagrams/quickStyle", "{CT}drawingml.diagramStyle+xml", "{RT}diagramQuickStyle"),
        ("xl/diagrams/colors", "{CT}drawingml.diagramColors+xml", "{RT}diagramColors"),
        ("xl/diagrams/drawing", "application/vnd.ms-office.drawingml.diagramDrawing+xml", "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing"),
        ("xl/pivotCache/pivotCacheDefinition", "{CT}spreadsheetml.pivotCacheDefinition+xml", "{RT}pivotCacheDefinition"),
        ("xl/pivotCache/pivotCacheRecords", "{CT}spreadsheetml.pivotCacheRecords+xml", "{RT}pivotCacheRecords"),
        ("xl/pivotTables/pivotTable", "{CT}spreadsheetml.pivotTable+xml", "{RT}pivotTable"),
        ("xl/slicerCaches/slicerCache", "application/vnd.ms-excel.slicerCache+xml", "http://schemas.microsoft.com/office/2007/relationships/slicerCache"),
        ("xl/slicers/slicer", "application/vnd.ms-excel.slicer+xml", "http://schemas.microsoft.com/office/2007/relationships/slicer"),
        ("xl/timelineCaches/timelineCache", "application/vnd.ms-excel.timelineCache+xml", "http://schemas.microsoft.com/office/2011/relationships/timelineCache"),
        ("xl/timelines/timeline", "application/vnd.ms-excel.timeline+xml", "http://schemas.microsoft.com/office/2011/relationships/timeline"),
        ("xl/threadedComments/threadedComment", "application/vnd.ms-excel.threadedcomments+xml", "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment"),
        ("xl/persons/person", "application/vnd.ms-excel.person+xml", "http://schemas.microsoft.com/office/2017/10/relationships/person"),
        ("xl/comments", "{CT}spreadsheetml.comments+xml", "{RT}comments"),
        ("xl/richData/rdrichvaluestructure", "application/vnd.ms-excel.rdrichvaluestructure+xml", "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueStructure"),
        ("xl/richData/rdrichvalue", "application/vnd.ms-excel.rdrichvalue+xml", "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValue"),
        ("xl/richData/rdRichValueTypes", "application/vnd.ms-excel.rdrichvaluetypes+xml", "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueTypes"),
        ("xl/richData/richValueRel", "application/vnd.ms-excel.richvaluerel+xml", "http://schemas.microsoft.com/office/2022/10/relationships/richValueRel"),
        ("xl/metadata", "{CT}spreadsheetml.sheetMetadata+xml", "{RT}sheetMetadata"),
        ("xl/connections", "{CT}spreadsheetml.connections+xml", "{RT}connections"),
        ("xl/queryTables/queryTable", "{CT}spreadsheetml.queryTable+xml", "{RT}queryTable"),
        ("customXml/itemProps", "application/vnd.openxmlformats-officedocument.customXmlProperties+xml", "{RT}customXmlProps"),
        ("customXml/item", "application/xml", "{RT}customXml"),
        ("xl/tables/table", "{CT}spreadsheetml.table+xml", "{RT}table"),
        ("xl/theme/theme", "{CT}theme+xml", "{RT}theme"),
        ("xl/styles", "{CT}spreadsheetml.styles+xml", "{RT}styles"),
        ("xl/sharedStrings", "{CT}spreadsheetml.sharedStrings+xml", "{RT}sharedStrings"),
        ("xl/embeddings/oleObject", "{CT}oleObject", "{RT}oleObject"),
        ("xl/models/", "model/gltf.binary", "http://schemas.microsoft.com/office/2017/06/relationships/model3d"),
        ("xl/model3d/", "model/gltf.binary", "http://schemas.microsoft.com/office/2017/06/relationships/model3d"),
        ("xl/drawings/vmlDrawing", "{CT}vmlDrawing", "{RT}vmlDrawing"),
    ];
    t.iter().find(|(p, _, _)| part.starts_with(p)).map(|(_, c, r)| (c.replace("{CT}", CT), r.replace("{RT}", RT)))
}

/// Checks every part of a package: structure, content types and relationship types.
pub fn check_files(files: &BTreeMap<String, String>) -> Vec<String> {
    let mut errs = vec![];
    for (k, v) in files {
        if k.ends_with(".xml") || k.ends_with(".rels") { errs.extend(check_xml(k, v)); }
    }
    let Some(ct) = files.get("[Content_Types].xml").and_then(|c| Element::parse(c).ok()) else { return errs };
    let ctype = |part: &str| -> Option<String> {
        ct.all("Override").find(|o| o.attr("PartName").map(|p| p.trim_start_matches('/')) == Some(part)).and_then(|o| o.attr("ContentType").map(String::from))
            .or_else(|| { let ext = part.rsplit('.').next()?.to_ascii_lowercase(); ct.all("Default").find(|d| d.attr("Extension").map(|x| x.to_ascii_lowercase()) == Some(ext.clone())).and_then(|d| d.attr("ContentType").map(String::from)) })
    };
    for k in files.keys() {
        if k.ends_with(".rels") || k.contains("vmlDrawingHF") { continue; }
        if let Some((c, _)) = expected(k) { if ctype(k).as_deref() != Some(c.as_str()) { errs.push(format!("{k}: content type {:?}, expected {c}", ctype(k))); } }
    }
    for (k, v) in files {
        let Some(i) = k.rfind("_rels/") else { continue };
        let owner = format!("{}{}", &k[..i], k[i + 6..].trim_end_matches(".rels"));
        let Ok(rels) = Element::parse(v) else { continue };
        for r in rels.all("Relationship") {
            if r.attr("TargetMode") == Some("External") { continue; }
            let t = super::read::resolve(&owner, r.attr("Target").unwrap_or(""));
            if t.contains("vmlDrawingHF") { continue; }
            if let Some((_, rt)) = expected(&t) { if r.attr("Type") != Some(rt.as_str()) { errs.push(format!("{k}: relationship to {t} has type {:?}, expected {rt}", r.attr("Type"))); } }
        }
    }
    errs
}
