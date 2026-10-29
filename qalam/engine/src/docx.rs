//! DOCX package reading and writing (docx.js).
//!
//! Every package part is kept. Saving rewrites `word/document.xml` from the
//! model (plus numbering, media and relationships when they changed) and
//! copies all other parts byte for byte. Unrecognized paragraph/run
//! properties, original drawings and imported numbering definitions are
//! written back verbatim.

use crate::chars;
use crate::format::*;
use crate::model::*;
use crate::numbering::{AbstractNum, Level, NumFmt, NumInstance, Numbering};
use crate::page::PageSettings;
use crate::styles::{Style, StyleKind, StyleSheet};
use crate::xml::{self, Element, Node};
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Write};

const DOC_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const REL_DOC: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_HEADER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
const REL_FOOTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const REL_LINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const REL_NUMBERING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
const REL_COMMENTS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const REL_COMMENTS_EX: &str = "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const REL_CUSTOM_XML: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml";
const REL_CUSTOM_XML_PROPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps";
const REL_SETTINGS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
const REL_FOOTNOTES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";
const REL_ENDNOTES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes";
const REL_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const REL_STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const EMU_PER_PX: f32 = 9525.0;

#[derive(Clone, Debug)]
pub struct Rel { pub id: String, pub kind: String, pub target: String, pub external: bool }

/// The original package, kept for preservation on save.
#[derive(Clone, Debug, Default)]
pub struct Package {
    pub parts: BTreeMap<String, Vec<u8>>,
    pub main: String,
    /// Root start tag attributes of the main document (namespace declarations, mc:Ignorable).
    pub root_attrs: Vec<(String, String)>,
    pub body_sect: Option<Element>,
    pub rels: Vec<Rel>,
}

fn rels_path(part: &str) -> String {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") }
}

fn resolve_target(base: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') { return abs.to_string(); }
    let mut parts: Vec<&str> = base.rsplit_once('/').map(|(d, _)| d.split('/').collect()).unwrap_or_default();
    for seg in target.split('/') {
        match seg { ".." => { parts.pop(); } "." | "" => {} s => parts.push(s) }
    }
    parts.join("/")
}

fn read_rels(parts: &BTreeMap<String, Vec<u8>>, part: &str) -> Vec<Rel> {
    let Some(bytes) = parts.get(&rels_path(part)) else { return vec![] };
    let Ok(root) = xml::parse(&String::from_utf8_lossy(bytes)) else { return vec![] };
    root.elements().filter(|e| e.name.ends_with("Relationship")).map(|e| Rel {
        id: e.attr("Id").unwrap_or_default().into(),
        kind: e.attr("Type").unwrap_or_default().into(),
        target: e.attr("Target").unwrap_or_default().into(),
        external: e.attr("TargetMode") == Some("External"),
    }).collect()
}

fn part_text(parts: &BTreeMap<String, Vec<u8>>, path: &str) -> Option<String> {
    parts.get(path).map(|b| {
        let s = String::from_utf8_lossy(b).to_string();
        s.trim_start_matches('\u{FEFF}').to_string()
    })
}

// ---------------------------------------------------------------------------
// Reading

struct Reader<'a> {
    parts: &'a BTreeMap<String, Vec<u8>>,
    theme_minor: Option<String>,
    theme_major: Option<String>,
    /// The theme's complex-script fonts (<a:cs>), for w:cstheme.
    theme_minor_cs: Option<String>,
    theme_major_cs: Option<String>,
}

fn twip_attr(e: &Element, name: &str) -> Option<i32> { e.attr(name).and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v.round() as i32) }

impl<'a> Reader<'a> {
    fn run_props(&self, rpr: &Element) -> (RunProps, Option<String>, Vec<String>) {
        let mut p = RunProps::default();
        let mut style = None;
        let mut extra = vec![];
        for c in rpr.elements() {
            let val = c.attr("w:val");
            let on = !matches!(val, Some("0" | "false" | "off"));
            match c.name.as_str() {
                "w:rStyle" => style = val.map(String::from),
                "w:rFonts" => {
                    let theme = c.attr("w:asciiTheme").map(|t| if t.starts_with("major") { self.theme_major.clone() } else { self.theme_minor.clone() }).flatten();
                    p.font = c.attr("w:ascii").or(c.attr("w:hAnsi")).map(String::from).or(theme);
                    let cs_theme = c.attr("w:cstheme").and_then(|t| if t.starts_with("major") { self.theme_major_cs.clone() } else { self.theme_minor_cs.clone() });
                    p.cs_font = c.attr("w:cs").map(String::from).or(cs_theme);
                    if p.font.is_none() && p.cs_font.is_none() { extra.push(c.to_xml()); }
                }
                "w:b" => p.bold = Some(on),
                "w:i" => p.italic = Some(on),
                "w:u" => p.underline = Some(val.unwrap_or("single").into()),
                "w:strike" => p.strike = Some(if on { Strike::Single } else { Strike::None }),
                "w:dstrike" => { if on { p.strike = Some(Strike::Double); } else if p.strike.is_none() { p.strike = Some(Strike::None); } }
                "w:color" => p.color = Some(match val { Some("auto") | None => None, Some(v) => Color::from_hex(v) }),
                "w:sz" => p.size = val.and_then(|v| v.parse::<f32>().ok()).map(|v| v / 2.0),
                "w:highlight" => p.highlight = Some(val.filter(|v| *v != "none").map(String::from)),
                "w:shd" => p.shading = Some(c.attr("w:fill").filter(|f| *f != "auto").and_then(Color::from_hex)),
                "w:vertAlign" => p.vert_align = Some(match val { Some("superscript") => VertAlign::Superscript, Some("subscript") => VertAlign::Subscript, _ => VertAlign::Baseline }),
                "w:caps" => p.caps = Some(if on { Caps::AllCaps } else { Caps::Normal }),
                "w:smallCaps" => p.caps = Some(if on { Caps::SmallCaps } else { Caps::Normal }),
                "w:spacing" => p.spacing = val.and_then(|v| v.parse::<f32>().ok()).map(|v| v / 20.0),
                "w:vanish" => p.vanish = Some(on),
                // Written back by the model (or deliberately dropped).
                "w:szCs" => p.size_cs = val.and_then(|v| v.parse::<f32>().ok()).map(|v| v / 2.0),
                "w:bCs" => p.bold_cs = Some(on),
                "w:iCs" => p.italic_cs = Some(on),
                "w:rtl" => p.rtl = Some(on),
                "w:outline" => p.outline = Some(on),
                "w14:glow" => {
                    let color = c.find("w14:srgbClr").and_then(|x| x.attr("w14:val")).and_then(Color::from_hex).unwrap_or(Color(0xFF, 0xC0, 0));
                    let radius = c.attr("w14:rad").and_then(|r| r.parse::<f32>().ok()).unwrap_or(63500.0) / 12700.0;
                    p.glow = Some(Some(Glow { color, radius }));
                }
                "w14:reflection" => p.reflection = Some(true),
                "w14:textFill" => {
                    let mut stops = vec![];
                    c.find_all("w14:gs", &mut stops);
                    let col = |g: &&Element| g.find("w14:srgbClr").and_then(|x| x.attr("w14:val")).and_then(Color::from_hex);
                    match (stops.first().and_then(col), stops.last().and_then(col)) {
                        (Some(a), Some(b)) => p.gradient = Some(Some((a, b))),
                        _ => extra.push(c.to_xml()),
                    }
                }
                "w:shadow" => p.shadow = Some(on),
                "w:rPrChange" => {}
                _ => extra.push(c.to_xml()),
            }
        }
        (p, style, extra)
    }

    fn para_props(&self, ppr: &Element) -> (ParaProps, Option<String>, RunProps, Vec<String>) {
        let mut p = ParaProps::default();
        let mut style = None;
        let mut mark = RunProps::default();
        let mut extra = vec![];
        for c in ppr.elements() {
            match c.name.as_str() {
                "w:pStyle" => style = c.attr("w:val").map(String::from),
                "w:framePr" if matches!(c.attr("w:dropCap"), Some("drop" | "margin")) => {
                    p.drop_cap = Some(crate::format::DropCap { margin: c.attr("w:dropCap") == Some("margin"), lines: c.attr("w:lines").and_then(|v| v.parse().ok()).unwrap_or(3).clamp(1, 10), distance: twip_attr(c, "w:hSpace").unwrap_or(0) });
                }
                "w:jc" => p.alignment = c.attr("w:val").map(Alignment::from_ooxml),
                "w:spacing" => {
                    let auto = |side: &str| matches!(c.attr(&format!("w:{side}Autospacing")), Some("1" | "true" | "on"));
                    let lines = |side: &str| c.attr(&format!("w:{side}Lines")).and_then(|v| v.parse::<f32>().ok()).filter(|v| *v != 0.0).map(|v| v / 100.0 * 12.0);
                    if auto("before") { p.space_before = Some(14.0) } else if let Some(v) = lines("before") { p.space_before = Some(v) } else if let Some(v) = twip_attr(c, "w:before") { p.space_before = Some(v as f32 / 20.0) }
                    if auto("after") { p.space_after = Some(14.0) } else if let Some(v) = lines("after") { p.space_after = Some(v) } else if let Some(v) = twip_attr(c, "w:after") { p.space_after = Some(v as f32 / 20.0) }
                    if let Some(line) = twip_attr(c, "w:line") {
                        let rule = match c.attr("w:lineRule") { Some("exact") => LineRule::Exact, Some("atLeast") => LineRule::AtLeast, _ => LineRule::Auto };
                        p.line_rule = Some(rule);
                        p.line_spacing = Some(if rule == LineRule::Auto { line as f32 / 240.0 } else { line as f32 / 20.0 });
                    }
                }
                "w:ind" => {
                    p.indent_left = twip_attr(c, "w:left").or(twip_attr(c, "w:start")).or(p.indent_left);
                    p.indent_right = twip_attr(c, "w:right").or(twip_attr(c, "w:end")).or(p.indent_right);
                    if let Some(h) = twip_attr(c, "w:hanging") { p.first_line = Some(-h); } else if let Some(f) = twip_attr(c, "w:firstLine") { p.first_line = Some(f); }
                }
                "w:keepNext" => p.keep_next = ppr.flag("w:keepNext"),
                "w:bidi" => p.rtl = ppr.flag("w:bidi"),
                "w:keepLines" => p.keep_lines = ppr.flag("w:keepLines"),
                "w:pageBreakBefore" => p.page_break_before = ppr.flag("w:pageBreakBefore"),
                "w:widowControl" => p.widow_control = ppr.flag("w:widowControl"),
                "w:contextualSpacing" => p.contextual_spacing = ppr.flag("w:contextualSpacing"),
                "w:outlineLvl" => p.outline_level = c.attr("w:val").and_then(|v| v.parse().ok()),
                "w:tabs" => p.tabs = Some(c.children_named("w:tab").filter_map(|t| Some(TabStop {
                    pos: twip_attr(t, "w:pos")?,
                    kind: match t.attr("w:val") { Some("center") => TabKind::Center, Some("right" | "end") => TabKind::Right, Some("decimal") => TabKind::Decimal, Some("clear") => TabKind::Clear, _ => TabKind::Left },
                    leader: match t.attr("w:leader") { Some("dot") => Leader::Dot, Some("hyphen") => Leader::Hyphen, Some("underscore" | "heavy") => Leader::Underscore, Some("middleDot") => Leader::MiddleDot, _ => Leader::None },
                })).collect()),
                "w:shd" => p.shading = Some(c.attr("w:fill").filter(|f| *f != "auto").and_then(Color::from_hex)),
                "w:pBdr" => {
                    let side = |n: &str| c.child(n).filter(|b| !matches!(b.attr("w:val"), Some("nil" | "none"))).map(|b| Border {
                        width: b.attr("w:sz").and_then(|v| v.parse::<f32>().ok()).unwrap_or(4.0) / 8.0,
                        space: b.attr("w:space").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0),
                        color: b.attr("w:color").and_then(Color::from_hex),
                    });
                    p.borders = Some(ParaBorders { top: side("w:top"), bottom: side("w:bottom"), left: side("w:left"), right: side("w:right") });
                }
                "w:numPr" => {
                    let id = c.val("w:numId").and_then(|v| v.parse().ok());
                    let lvl = c.val("w:ilvl").and_then(|v| v.parse().ok()).unwrap_or(0);
                    if let Some(id) = id { p.numbering = Some((id, lvl)); }
                }
                "w:rPr" => mark = self.run_props(c).0,
                "w:pPrChange" | "w:sectPr" => {}
                "w:suppressLineNumbers" => p.suppress_line_numbers = ppr.flag("w:suppressLineNumbers"),
                "w:suppressAutoHyphens" => p.suppress_hyphens = ppr.flag("w:suppressAutoHyphens"),
                _ => extra.push(c.to_xml()),
            }
        }
        (p, style, mark, extra)
    }

    fn styles(&self, root: &Element) -> StyleSheet {
        let mut s = StyleSheet::empty();
        if let Some(d) = root.child("w:docDefaults") {
            if let Some(r) = d.child("w:rPrDefault").and_then(|x| x.child("w:rPr")) { s.doc_run = self.run_props(r).0; }
            if let Some(p) = d.child("w:pPrDefault").and_then(|x| x.child("w:pPr")) { s.doc_para = self.para_props(p).0; }
        }
        // Absent docDefaults spacing means 0 after, single line (§17.7.5).
        if s.doc_para.space_after.is_none() { s.doc_para.space_after = Some(0.0); }
        if s.doc_run.size.is_none() { s.doc_run.size = Some(10.0); }
        if s.doc_run.font.is_none() { s.doc_run.font = self.theme_minor.clone().or(Some("Times New Roman".into())); }
        for st in root.children_named("w:style") {
            let kind = match st.attr("w:type") { Some("character") => StyleKind::Character, Some("table") => StyleKind::Table, Some("numbering") => StyleKind::Numbering, _ => StyleKind::Paragraph };
            let id = st.attr("w:styleId").unwrap_or_default().to_string();
            let default = matches!(st.attr("w:default"), Some("1" | "true" | "on"));
            if default && kind == StyleKind::Paragraph { s.default_paragraph = Some(id.clone()); }
            if default && kind == StyleKind::Table { s.default_table = Some(id.clone()); }
            let para = st.child("w:pPr").map(|p| self.para_props(p).0).unwrap_or_default();
            let run = st.child("w:rPr").map(|r| self.run_props(r).0).unwrap_or_default();
            let table_borders = st.child("w:tblPr").and_then(|t| t.child("w:tblBorders")).is_some_and(|b| b.elements().any(|e| !matches!(e.attr("w:val"), Some("nil" | "none"))));
            s.add(Style { name: st.val("w:name").map(String::from).unwrap_or_else(|| id.clone()), id, kind, based_on: st.val("w:basedOn").map(String::from), next: st.val("w:next").map(String::from), para, run, table_borders });
        }
        s
    }

    fn numbering(&self, root: &Element) -> Numbering {
        let mut n = Numbering::default();
        for a in root.children_named("w:abstractNum") {
            let Some(id) = a.attr("w:abstractNumId").and_then(|v| v.parse().ok()) else { continue };
            let mut levels: Vec<Level> = (0..9).map(|i| Level { fmt: NumFmt::Decimal, text: format!("%{}.", i + 1), start: 1, indent_left: 720 * (i + 1), hanging: 360, font: None }).collect();
            for l in a.children_named("w:lvl") {
                let Some(i) = l.attr("w:ilvl").and_then(|v| v.parse::<usize>().ok()).filter(|i| *i < 9) else { continue };
                let lv = &mut levels[i];
                lv.fmt = l.val("w:numFmt").map(NumFmt::from_ooxml).unwrap_or(NumFmt::Decimal);
                lv.text = l.val("w:lvlText").unwrap_or("").to_string();
                lv.start = l.val("w:start").and_then(|v| v.parse().ok()).unwrap_or(1);
                if let Some(ind) = l.child("w:pPr").and_then(|p| p.child("w:ind")) {
                    lv.indent_left = twip_attr(ind, "w:left").or(twip_attr(ind, "w:start")).unwrap_or(lv.indent_left);
                    lv.hanging = twip_attr(ind, "w:hanging").unwrap_or(0);
                }
                lv.font = l.child("w:rPr").and_then(|r| r.child("w:rFonts")).and_then(|f| f.attr("w:ascii")).map(String::from);
            }
            n.abstracts.push(AbstractNum { id, levels, raw: Some(a.to_xml()) });
        }
        for num in root.children_named("w:num") {
            let (Some(id), Some(abs)) = (num.attr("w:numId").and_then(|v| v.parse().ok()), num.val("w:abstractNumId").and_then(|v| v.parse().ok())) else { continue };
            let mut overrides = HashMap::new();
            for o in num.children_named("w:lvlOverride") {
                if let (Some(l), Some(s)) = (o.attr("w:ilvl").and_then(|v| v.parse().ok()), o.child("w:startOverride").and_then(|s| s.attr("w:val")).and_then(|v| v.parse().ok())) { overrides.insert(l, s); }
            }
            n.nums.push(NumInstance { id, abstract_id: abs, start_overrides: overrides, raw: Some(num.to_xml()) });
        }
        n
    }
}

struct BodyReader<'a, 'b> {
    r: &'a Reader<'b>,
    rels: &'a [Rel],
    part: &'a str,
    paragraphs: Vec<Paragraph>,
    tables: Vec<Table>,
    assets: HashMap<String, Asset>,
    /// Complex field state: Some(instruction) while inside a field's result.
    field: Vec<(String, bool)>,
    /// The tracked change enclosing the runs being read.
    revision: Option<Revision>,
    /// Bookmark ids seen so far → names (bookmarkEnd refers to the id).
    bookmark_ids: HashMap<String, String>,
}

impl<'a, 'b> BodyReader<'a, 'b> {
    fn rel(&self, id: &str) -> Option<&Rel> { self.rels.iter().find(|r| r.id == id) }

    fn block(&mut self, e: &Element, cell: Option<u64>) {
        match e.name.as_str() {
            "w:p" => { let p = self.paragraph(e, cell); self.paragraphs.push(p); }
            "w:tbl" => {
                self.table(e, cell);
            }
            "w:sdt" => if let Some(c) = e.child("w:sdtContent") { for b in c.elements() { self.block(b, cell); } },
            "w:customXml" | "w:ins" | "w:moveTo" => for b in e.elements() { self.block(b, cell); },
            _ => {}
        }
    }

    fn table(&mut self, e: &Element, parent: Option<u64>) {
        let tblpr = e.child("w:tblPr");
        let grid: Vec<i32> = e.child("w:tblGrid").map(|g| g.children_named("w:gridCol").map(|c| twip_attr(c, "w:w").unwrap_or(0)).collect()).unwrap_or_default();
        let mut table = Table {
            id: new_id(), grid, rows: vec![],
            borders: tblpr.and_then(|p| p.child("w:tblBorders")).is_some_and(|b| b.elements().any(|x| !matches!(x.attr("w:val"), Some("nil" | "none")))),
            style: tblpr.and_then(|p| p.val("w:tblStyle")).map(String::from),
            indent: tblpr.and_then(|p| p.child("w:tblInd")).filter(|i| i.attr("w:type").is_none_or(|t| t == "dxa")).and_then(|i| twip_attr(i, "w:w")).unwrap_or(0),
            alignment: tblpr.and_then(|p| p.val("w:jc")).map(Alignment::from_ooxml).unwrap_or_default(),
            cell_margins: tblpr.and_then(|p| p.child("w:tblCellMar")).map(|m| read_margins(m, DEFAULT_CELL_MARGINS)).unwrap_or(DEFAULT_CELL_MARGINS),
            raw_props: tblpr.map(|p| p.to_xml()),
            parent,
            fixed: tblpr.and_then(|p| p.child("w:tblLayout")).and_then(|l| l.attr("w:type")) == Some("fixed"),
        };
        let start = self.paragraphs.len();
        for tr in e.children_named("w:tr") {
            let trpr = tr.child("w:trPr");
            let height = trpr.and_then(|t| t.child("w:trHeight"));
            let mut row = Row {
                cells: vec![], min_height: height.and_then(|h| twip_attr(h, "w:val")).unwrap_or(0),
                exact: height.and_then(|h| h.attr("w:hRule")) == Some("exact"),
                header: trpr.and_then(|t| t.flag("w:tblHeader")).unwrap_or(false),
                cant_split: trpr.and_then(|t| t.flag("w:cantSplit")).unwrap_or(false),
                grid_before: trpr.and_then(|t| t.val("w:gridBefore")).and_then(|v| v.parse().ok()).unwrap_or(0),
                grid_after: trpr.and_then(|t| t.val("w:gridAfter")).and_then(|v| v.parse().ok()).unwrap_or(0),
                extra: trpr.map(|t| t.elements().filter(|c| !matches!(c.name.as_str(), "w:trHeight" | "w:tblHeader" | "w:cantSplit" | "w:gridBefore" | "w:gridAfter")).map(|c| c.to_xml()).collect()).unwrap_or_default(),
            };
            let mut cells: Vec<&Element> = vec![];
            for c in tr.elements() {
                match c.name.as_str() {
                    "w:tc" => cells.push(c),
                    "w:sdt" => if let Some(x) = c.child("w:sdtContent") { cells.extend(x.children_named("w:tc")); },
                    _ => {}
                }
            }
            for tc in cells {
                let tcpr = tc.child("w:tcPr");
                let mut cell = Cell::new(tcpr.and_then(|p| p.child("w:tcW")).filter(|w| w.attr("w:type").is_none_or(|t| t == "dxa")).and_then(|w| twip_attr(w, "w:w")).unwrap_or(0));
                cell.span = tcpr.and_then(|p| p.val("w:gridSpan")).and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
                cell.v_merge = match tcpr.and_then(|p| p.child("w:vMerge")) { None => VMerge::None, Some(v) if v.attr("w:val") == Some("restart") => VMerge::Restart, Some(_) => VMerge::Continue };
                cell.shading = tcpr.and_then(|p| p.child("w:shd")).and_then(|s| s.attr("w:fill")).filter(|f| *f != "auto").and_then(Color::from_hex);
                cell.v_align = match tcpr.and_then(|p| p.val("w:vAlign")) { Some("center") => CellVAlign::Center, Some("bottom") => CellVAlign::Bottom, _ => CellVAlign::Top };
                if let Some(b) = tcpr.and_then(|p| p.child("w:tcBorders")) {
                    let side = |names: &[&str]| names.iter().find_map(|n| b.child(n)).map(read_border);
                    cell.borders = CellBorders { top: side(&["w:top"]), right: side(&["w:right", "w:end"]), bottom: side(&["w:bottom"]), left: side(&["w:left", "w:start"]) };
                }
                cell.margins = tcpr.and_then(|p| p.child("w:tcMar")).map(|m| read_margins(m, [-1; 4]));
                if let Some(p) = tcpr { cell.extra = p.elements().filter(|c| !matches!(c.name.as_str(), "w:tcW" | "w:gridSpan" | "w:vMerge" | "w:shd" | "w:vAlign" | "w:tcBorders" | "w:tcMar")).map(|c| c.to_xml()).collect(); }
                let before = self.paragraphs.len();
                for b in tc.elements() { self.block(b, Some(cell.id)); }
                if self.paragraphs.len() == before { let mut p = Paragraph::default(); p.cell = Some(cell.id); self.paragraphs.push(p); }
                row.cells.push(cell);
            }
            if row.cells.is_empty() { continue; }
            table.rows.push(row);
        }
        if table.rows.is_empty() { self.paragraphs.truncate(start); return; }
        if table.grid.is_empty() {
            // Derive a grid from the first row's widths.
            table.grid = table.rows[0].cells.iter().flat_map(|c| std::iter::repeat_n(c.width / c.span as i32, c.span as usize)).collect();
        }
        self.tables.push(table);
    }

    fn paragraph(&mut self, e: &Element, cell: Option<u64>) -> Paragraph {
        let mut p = Paragraph::default();
        p.cell = cell;
        if let Some(ppr) = e.child("w:pPr") {
            let (props, style, mark, extra) = self.r.para_props(ppr);
            p.props = props; p.style = style; p.mark = mark; p.extra = extra;
            p.section = ppr.child("w:sectPr").map(read_section);
            if let Some(rpr) = ppr.child("w:rPr") {
                p.mark_revision = rpr.child("w:ins").map(|x| revision_from(x, RevKind::Ins)).or_else(|| rpr.child("w:del").map(|x| revision_from(x, RevKind::Del)));
            }
        }
        self.inline(e, &mut p.runs, None);
        p.runs.retain(|r| !r.text.is_empty() || r.marker.is_some());
        p
    }

    fn inline(&mut self, e: &Element, runs: &mut Vec<Run>, link: Option<&str>) {
        for c in e.elements() {
            match c.name.as_str() {
                "w:r" => self.run(c, runs, link),
                "w:hyperlink" => {
                    let target = c.attr("r:id").and_then(|id| self.rel(id)).map(|r| r.target.clone())
                        .or_else(|| c.attr("w:anchor").map(|a| format!("#{a}")));
                    let t = target.or(link.map(String::from));
                    self.inline(c, runs, t.as_deref());
                }
                "w:fldSimple" => {
                    let instr = c.attr("w:instr").unwrap_or_default().trim().to_string();
                    let field = page_field(&instr);
                    let mut inner = vec![];
                    self.inline(c, &mut inner, link);
                    if let Some(f) = field {
                        let props = inner.first().map(|r| r.props.clone()).unwrap_or_default();
                        runs.push(Run { text: " ".into(), props, field: Some(f), ..Default::default() });
                    } else { runs.extend(inner); }
                }
                "w:ins" | "w:moveTo" | "w:del" | "w:moveFrom" => {
                    let kind = if matches!(c.name.as_str(), "w:ins" | "w:moveTo") { RevKind::Ins } else { RevKind::Del };
                    let outer = self.revision.replace(revision_from(c, kind));
                    self.inline(c, runs, link);
                    self.revision = outer;
                }
                "w:smartTag" | "w:customXml" | "w:bdo" | "w:dir" => self.inline(c, runs, link),
                "w:sdt" => if let Some(x) = c.child("w:sdtContent") { self.inline(x, runs, link) },
                "w:commentRangeStart" => if let Some(id) = c.attr("w:id") { runs.push(Run::marker(Marker::CommentStart(id.into()))) },
                "w:bookmarkStart" => if let (Some(id), Some(name)) = (c.attr("w:id"), c.attr("w:name")) {
                    // Word's internal _GoBack bookmark only marks the last edit position.
                    if name != "_GoBack" { self.bookmark_ids.insert(id.to_string(), name.to_string()); runs.push(Run::marker(Marker::BookmarkStart(name.into()))); }
                },
                "w:bookmarkEnd" => if let Some(name) = c.attr("w:id").and_then(|id| self.bookmark_ids.get(id)).cloned() { runs.push(Run::marker(Marker::BookmarkEnd(name))) },
                "w:commentRangeEnd" => if let Some(id) = c.attr("w:id") { runs.push(Run::marker(Marker::CommentEnd(id.into()))) },
                "m:oMath" => runs.push(Run { text: chars::OBJECT.to_string(), object: Some(crate::objects::Object::Equation(crate::equation::read_omml(c, false))), ..Default::default() }),
                "m:oMathPara" => for m in c.children_named("m:oMath") { runs.push(Run { text: chars::OBJECT.to_string(), object: Some(crate::objects::Object::Equation(crate::equation::read_omml(m, true))), ..Default::default() }); },
                _ => {}
            }
        }
    }

    fn run(&mut self, e: &Element, runs: &mut Vec<Run>, link: Option<&str>) {
        let (props, style, extra) = e.child("w:rPr").map(|r| self.r.run_props(r)).unwrap_or_default();
        let base = Run { text: String::new(), props, style, link: link.map(String::from), image: None, field: None, extra, marker: None, revision: self.revision.clone(), note: None, object: None, anchor: None };
        let mut text = String::new();
        let flush = |text: &mut String, runs: &mut Vec<Run>| if !text.is_empty() { runs.push(base.with(std::mem::take(text))); };
        for c in e.elements() {
            // Inside a complex field's instruction, collect nothing; in its result, keep text unless it's a PAGE field.
            // Inside any field's instruction (including a field nested in one), collect nothing.
            let in_instr = self.field.iter().any(|f| !f.1);
            match c.name.as_str() {
                "w:fldChar" => match c.attr("w:fldCharType") {
                    Some("begin") => self.field.push((String::new(), false)),
                    Some("separate") => {
                        let nested_in_instr = self.field.len() >= 2 && !self.field[self.field.len() - 2].1;
                        if nested_in_instr { if let Some(f) = self.field.last_mut() { f.1 = true; } }
                        else if let Some(f) = self.field.last_mut() {
                            f.1 = true;
                            if let Some(field) = page_field(f.0.trim()) {
                                flush(&mut text, runs);
                                runs.push(Run { text: " ".into(), field: Some(field), ..base.with(String::new()) });
                                f.0 = "#done".into();
                            } else if block_field(f.0.trim()) {
                                flush(&mut text, runs);
                                runs.push(Run::marker(Marker::FieldBegin(f.0.trim().to_string())));
                                f.0 = "#block".into();
                            }
                        }
                    }
                    Some("end") => {
                        if let Some((instr, separated)) = self.field.pop() {
                            // A field inside another's instruction (IF {MERGEFIELD X} = …) joins it.
                            if let Some(parent) = self.field.last_mut().filter(|p| !p.1) { parent.0.push_str(&format!(" {{{}}} ", instr.trim())); }
                            else if instr == "#block" { flush(&mut text, runs); runs.push(Run::marker(Marker::FieldEnd)); }
                            // XE and TA have no result: they end without a separator.
                            else if !separated { if let Some(field @ (Field::IndexEntry(_) | Field::Authority { .. } | Field::NextRecord | Field::NextIf { .. } | Field::SkipIf { .. } | Field::MergeRec | Field::MergeSeq)) = page_field(instr.trim()) { flush(&mut text, runs); runs.push(Run { text: " ".into(), field: Some(field), ..base.with(String::new()) }); } }
                        }
                    }
                    _ => {}
                },
                "w:instrText" => if let Some(f) = self.field.last_mut() { if !f.1 { f.0.push_str(&c.text()); } },
                _ if in_instr => {}
                _ if self.field.last().is_some_and(|f| f.0 == "#done") => {}
                "w:t" | "w:delText" => text.push_str(&c.text().replace(['\u{000B}', '\u{000C}', '\u{000E}', '\u{FFFC}'], "")),
                "w:tab" | "w:ptab" => text.push('\t'),
                "w:br" | "w:cr" => text.push(match c.attr("w:type") { Some("page") => chars::PAGE_BREAK, Some("column") => chars::COLUMN_BREAK, _ => chars::LINE_BREAK }),
                "w:noBreakHyphen" => text.push('\u{2011}'),
                "w:softHyphen" => text.push('\u{AD}'),
                "w:sym" => text.push(c.attr("w:char").and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(|v| char::from_u32(if v >= 0xF000 { v - 0xF000 } else { v })).filter(|c| !c.is_control()).unwrap_or('\u{25A1}')),
                "w:drawing" => {
                    flush(&mut text, runs);
                    if let Some(obj) = self.drawing_object(c) { runs.push(Run { text: chars::OBJECT.to_string(), object: Some(obj), anchor: read_anchor(c), ..base.with(String::new()) }); }
                    else if let Some(img) = self.drawing(c) { runs.push(Run { text: chars::OBJECT.to_string(), image: Some(img), anchor: read_anchor(c), ..base.with(String::new()) }); }
                }
                "w:pict" | "mc:AlternateContent" => {
                    // Prefer the DrawingML choice of alternate content.
                    if let Some(d) = c.find("w:drawing") {
                        flush(&mut text, runs);
                        if let Some(obj) = self.drawing_object(d) { runs.push(Run { text: chars::OBJECT.to_string(), object: Some(obj), anchor: read_anchor(d), ..base.with(String::new()) }); }
                        else if let Some(img) = self.drawing(d) { runs.push(Run { text: chars::OBJECT.to_string(), image: Some(img), anchor: read_anchor(d), ..base.with(String::new()) }); }
                    }
                }
                "w:footnoteReference" | "w:endnoteReference" => {
                    flush(&mut text, runs);
                    let kind = if c.name == "w:footnoteReference" { NoteKind::Foot } else { NoteKind::End };
                    let mut r = base.with(chars::NOTE.to_string());
                    r.note = Some(NoteRef { kind, id: c.attr("w:id").unwrap_or("0").into() });
                    runs.push(r);
                }
                _ => {}
            }
        }
        flush(&mut text, runs);
    }

    /// Shapes (wps), charts (c:chart parts) and this app's diagrams (pictures with a diagram spec).
    fn drawing_object(&mut self, d: &Element) -> Option<crate::objects::Object> {
        use crate::objects::*;
        let container = d.child("wp:inline").or(d.child("wp:anchor"))?;
        let ext = container.child("wp:extent")?;
        let w = ext.attr("cx")?.parse::<f32>().ok()? / EMU_PER_PX;
        let h = ext.attr("cy")?.parse::<f32>().ok()? / EMU_PER_PX;
        let data = container.find("a:graphicData")?;
        let uri = data.attr("uri").unwrap_or_default();
        if uri.ends_with("wordprocessingShape") {
            let mut shape = parse_shape(data.child("wps:wsp")?, w, h)?;
            shape.name = container.child("wp:docPr").and_then(|p| p.attr("name")).filter(|n| !n.starts_with("Shape ") && !n.starts_with("Text Box ")).unwrap_or_default().to_string();
            shape.raw = Some(d.to_xml());
            return Some(Object::Shape(shape));
        }
        if uri.ends_with("wordprocessingGroup") {
            let descr = container.child("wp:docPr").and_then(|p| p.attr("descr")).unwrap_or_default();
            if descr.starts_with("word-ink") { return Some(Object::Ink(parse_ink(data.child("wpg:wgp")?, w, h, descr))); }
            return None;
        }
        if uri.ends_with("/chart") {
            let rel = self.rel(data.child("c:chart")?.attr("r:id")?)?;
            let path = resolve_target(self.part, &rel.target);
            let root = part_text(self.r.parts, &path).and_then(|t| xml::parse(&t).ok())?;
            let mut chart = parse_chart(&root, w, h)?;
            chart.part = Some(path);
            chart.raw = Some(d.to_xml());
            return Some(Object::Chart(chart));
        }
        let descr = container.child("wp:docPr").and_then(|p| p.attr("descr")).unwrap_or_default();
        Diagram::from_spec(descr, w, h).map(Object::Diagram)
    }

    fn drawing(&mut self, d: &Element) -> Option<Image> {
        let container = d.child("wp:inline").or(d.child("wp:anchor"))?;
        let ext = container.child("wp:extent")?;
        let cx: f32 = ext.attr("cx")?.parse().ok()?;
        let cy: f32 = ext.attr("cy")?.parse().ok()?;
        let blip = container.find("a:blip")?;
        let rel = self.rel(blip.attr("r:embed")?)?;
        let path = resolve_target(self.part, &rel.target);
        let bytes = self.r.parts.get(&path)?.clone();
        let ct = content_type_for(&path);
        self.assets.insert(path.clone(), Asset { bytes, content_type: ct });
        let alt = container.child("wp:docPr").and_then(|p| p.attr("descr")).unwrap_or_default().to_string();
        let rotation = container.find("pic:spPr").and_then(|s| s.child("a:xfrm")).and_then(|x| x.attr("rot")).and_then(|r| r.parse::<f32>().ok()).map(|r| r / 60000.0).unwrap_or(0.0);
        // The unedited picture, kept by Reset Picture across saves (an a:ext of pic:cNvPr).
        let original_part = container.find("wo:original").and_then(|o| o.attr("r:embed")).and_then(|id| self.rel(id)).map(|rel| resolve_target(self.part, &rel.target));
        let original = original_part.and_then(|p| {
            let bytes = self.r.parts.get(&p)?.clone();
            self.assets.insert(p.clone(), Asset { bytes, content_type: content_type_for(&p) });
            Some(p)
        });
        let xfrm = container.find("pic:spPr").and_then(|s| s.child("a:xfrm"));
        let flag = |a: &str| xfrm.and_then(|x| x.attr(a)).is_some_and(|v| v == "1" || v == "true");
        let (flip_h, flip_v) = (flag("flipH"), flag("flipV"));
        // DrawingML picture effects become edits over the embedded (original) picture.
        let effects = picture_effects(container);
        let (asset, original) = match (&original, effects.is_empty()) {
            (None, false) => match self.assets.get(&path).and_then(|a| crate::picture::bake(&a.bytes, &effects)) {
                Some(png) => {
                    let mut n = 1;
                    while self.assets.contains_key(&format!("word/media/edited{n}.png")) || self.r.parts.contains_key(&format!("word/media/edited{n}.png")) { n += 1; }
                    let baked = format!("word/media/edited{n}.png");
                    self.assets.insert(baked.clone(), Asset { bytes: png, content_type: "image/png".into() });
                    (baked, Some(path.clone()))
                }
                None => (path.clone(), original),
            },
            _ => (path.clone(), original),
        };
        let effects = if asset == path { vec![] } else { effects };
        Some(Image { asset, width: cx / EMU_PER_PX, height: cy / EMU_PER_PX, alt, raw: Some(d.to_xml()), original, rotation, flip_h, flip_v, effects })
    }
}

/// Picture edits from a pic:pic's DrawingML: blip effects, crop, preset shape, border, soft edges.
fn picture_effects(container: &Element) -> Vec<crate::picture::PictureOp> {
    use crate::picture::PictureOp;
    let mut ops = vec![];
    let num = |e: &Element, a: &str| e.attr(a).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    let color = |e: &Element| e.attr("val").and_then(Color::from_hex).or_else(|| match e.attr("val") { Some("black") => Some(Color::BLACK), Some("white") => Some(Color::WHITE), _ => None });
    if let Some(fill) = container.find("pic:blipFill") {
        if let Some(r) = fill.child("a:srcRect") {
            let f = |a: &str| num(r, a) / 100000.0;
            if f("l") + f("t") + f("r") + f("b") > 0.0 { ops.push(PictureOp::Crop { left: f("l"), top: f("t"), right: f("r"), bottom: f("b") }); }
        }
        if let Some(blip) = fill.child("a:blip") {
            for c in blip.children.iter().filter_map(|n| if let Node::Element(e) = n { Some(e) } else { None }) {
                match c.name.as_str() {
                    "a:grayscl" => ops.push(PictureOp::Grayscale),
                    "a:lum" => { let (b, k) = ((num(c, "bright") / 1000.0).round() as i32, (num(c, "contrast") / 1000.0).round() as i32); if b != 0 || k != 0 { ops.push(PictureOp::Corrections { brightness: b, contrast: k }); } }
                    "a:alphaModFix" => { let amt = c.attr("amt").and_then(|v| v.parse::<f32>().ok()).unwrap_or(100000.0); if amt < 100000.0 { ops.push(PictureOp::Transparency((100.0 - amt / 1000.0).round().clamp(0.0, 100.0) as u8)); } }
                    "a:duotone" => {
                        let cs: Vec<Color> = c.children.iter().filter_map(|n| if let Node::Element(e) = n { color(e) } else { None }).collect();
                        if cs.len() == 2 { ops.push(PictureOp::Duotone(cs[0], cs[1])); }
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(sp) = container.find("pic:spPr") {
        match sp.child("a:prstGeom").and_then(|g| g.attr("prst")) { Some("roundRect") => ops.push(PictureOp::Rounded), Some("ellipse") => ops.push(PictureOp::Oval), _ => {} }
        if let Some(c) = sp.child("a:ln").filter(|l| l.child("a:noFill").is_none()).and_then(|l| l.find("a:srgbClr")).and_then(|c| c.attr("val")).and_then(Color::from_hex) { ops.push(PictureOp::Frame(c)); }
        if sp.child("a:effectLst").and_then(|e| e.child("a:softEdge")).is_some() { ops.push(PictureOp::SoftEdges); }
    }
    ops
}

/// A section's start, columns and line numbering (w:sectPr).
fn read_section(sect: &Element) -> crate::page::Section {
    use crate::page::{Columns, LineNumbering, LineRestart, Section, SectionKind};
    let kind = sect.child("w:type").and_then(|t| t.attr("w:val")).map(SectionKind::from_val).unwrap_or_default();
    let columns = sect.child("w:cols").map(|c| {
        let n = |a: &str| c.attr(a).and_then(|v| v.parse::<i32>().ok());
        let widths: Vec<(i32, i32)> = if c.attr("w:equalWidth").is_some_and(|v| v == "0" || v == "false") { c.children_named("w:col").map(|x| (x.attr("w:w").and_then(|v| v.parse().ok()).unwrap_or(0), x.attr("w:space").and_then(|v| v.parse().ok()).unwrap_or(0))).collect() } else { vec![] };
        let count = if widths.is_empty() { n("w:num").unwrap_or(1) } else { widths.len() as i32 };
        Columns { count: count.clamp(1, 45) as u8, space: n("w:space").unwrap_or(720), separator: c.attr("w:sep").is_some_and(|v| v == "1" || v == "true"), widths }
    }).unwrap_or_default();
    let line_numbers = sect.child("w:lnNumType").map(|l| {
        let n = |a: &str| l.attr(a).and_then(|v| v.parse::<i64>().ok());
        LineNumbering { start: n("w:start").unwrap_or(0).max(0) as u32 + 1, count_by: n("w:countBy").unwrap_or(1).max(1) as u32, restart: match l.attr("w:restart") { Some("newPage") => LineRestart::Page, Some("continuous") => LineRestart::Continuous, _ => LineRestart::Section }, distance: n("w:distance").unwrap_or(0) as i32 }
    }).filter(|l| l.count_by > 0);
    Section { kind, columns, line_numbers }
}

/// Write a section's start, columns and line numbering into a w:sectPr.
fn write_section(s: &mut Element, sec: &crate::page::Section) {
    s.remove("w:type");
    s.remove("w:cols");
    s.remove("w:lnNumType");
    if sec.kind != crate::page::SectionKind::NextPage { s.push(val("w:type", sec.kind.val())); }
    if let Some(l) = sec.line_numbers {
        let mut e = el("w:lnNumType").with_attr("w:countBy", l.count_by.to_string());
        if l.start > 1 { e.set_attr("w:start", (l.start - 1).to_string()); }
        if l.distance > 0 { e.set_attr("w:distance", l.distance.to_string()); }
        e.set_attr("w:restart", match l.restart { crate::page::LineRestart::Page => "newPage", crate::page::LineRestart::Section => "newSection", crate::page::LineRestart::Continuous => "continuous" });
        s.push(e);
    }
    let c = &sec.columns;
    let mut cols = el("w:cols").with_attr("w:space", c.space.to_string());
    if c.count > 1 || !c.widths.is_empty() {
        if c.separator { cols.set_attr("w:sep", "1"); }
        if c.widths.is_empty() { cols.set_attr("w:num", c.count.to_string()); }
        else {
            cols.set_attr("w:num", c.widths.len().to_string());
            cols.set_attr("w:equalWidth", "0");
            for (w, sp) in &c.widths { cols.push(el("w:col").with_attr("w:w", w.to_string()).with_attr("w:space", sp.to_string())); }
        }
    }
    s.push(cols);
    sort_children(s, SECTPR_ORDER);
}

/// A run's floating placement from a `w:drawing`'s wp:anchor (§20.4.2.3).
fn read_anchor(d: &Element) -> Option<crate::model::Anchor> {
    use crate::model::{Anchor, AnchorAlign, RelFrom, Wrap};
    let a = d.child("wp:anchor")?;
    let wrap = if a.child("wp:wrapSquare").is_some() { Wrap::Square }
        else if a.child("wp:wrapTight").is_some() || a.child("wp:wrapThrough").is_some() { Wrap::Tight }
        else if a.child("wp:wrapTopAndBottom").is_some() { Wrap::TopBottom }
        else if a.attr("behindDoc").is_some_and(|v| v == "1" || v == "true") { Wrap::Behind } else { Wrap::InFront };
    let axis = |name: &str| -> (f32, RelFrom, Option<AnchorAlign>) {
        let Some(p) = a.child(name) else { return (0.0, RelFrom::Margin, None) };
        let from = match p.attr("relativeFrom").unwrap_or("margin") { "page" => RelFrom::Page, "paragraph" | "line" => RelFrom::Paragraph, _ => RelFrom::Margin };
        let off = p.child("wp:posOffset").and_then(|o| o.text().trim().parse::<f32>().ok()).map(|v| v / EMU_PER_PX).unwrap_or(0.0);
        let align = p.child("wp:align").map(|al| match al.text().trim() { "center" => AnchorAlign::Center, "right" | "bottom" | "outside" => AnchorAlign::End, _ => AnchorAlign::Start });
        (off, from, align)
    };
    let (x, x_from, x_align) = axis("wp:positionH");
    let (y, y_from, y_align) = axis("wp:positionV");
    // Horizontal offsets from a paragraph or character count from the margin here.
    let x_from = if x_from == RelFrom::Paragraph { RelFrom::Margin } else { x_from };
    let dist = a.attr("distL").and_then(|v| v.parse::<f32>().ok()).map(|v| v / EMU_PER_PX).unwrap_or(12.0);
    let z = a.attr("relativeHeight").and_then(|v| v.parse::<i64>().ok()).map(|v| (v - 251_658_240).div_euclid(1024)).unwrap_or(0);
    Some(Anchor { wrap, x, y, x_from, y_from, x_align, y_align, dist, z })
}

/// Turn a generated wp:inline drawing into wp:anchor for a floating run. Imported anchors kept
/// verbatim are already wp:anchor and pass through.
fn anchorize(mut d: Element, anchor: Option<crate::model::Anchor>, id: u32) -> Element {
    use crate::model::{AnchorAlign, RelFrom, Wrap};
    let Some(a) = anchor else { return d };
    let Some(k) = d.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "wp:inline")) else { return d };
    let Node::Element(inline) = d.children.remove(k) else { return d };
    let dist = ((a.dist * EMU_PER_PX).round() as i64).to_string();
    let mut out = el("wp:anchor").with_attr("distT", "0").with_attr("distB", "0").with_attr("distL", dist.clone()).with_attr("distR", dist)
        .with_attr("simplePos", "0").with_attr("relativeHeight", (251_658_240 + a.z.max(0) * 1024 + (id as i64 % 1024)).to_string()).with_attr("behindDoc", if a.wrap == Wrap::Behind { "1" } else { "0" })
        .with_attr("locked", "0").with_attr("layoutInCell", "1").with_attr("allowOverlap", "1");
    out.push(el("wp:simplePos").with_attr("x", "0").with_attr("y", "0"));
    let axis = |name: &str, from: RelFrom, off: f32, align: Option<AnchorAlign>, h: bool| {
        let rel = match (from, h) { (RelFrom::Page, _) => "page", (RelFrom::Paragraph, false) => "paragraph", _ => "margin" };
        let mut p = el(name).with_attr("relativeFrom", rel);
        match align {
            Some(al) => { let mut e = el("wp:align"); e.children.push(Node::Text(match (al, h) { (AnchorAlign::Start, true) => "left", (AnchorAlign::Start, false) => "top", (AnchorAlign::Center, _) => "center", (AnchorAlign::End, true) => "right", (AnchorAlign::End, false) => "bottom" }.into())); p.push(e); }
            None => { let mut e = el("wp:posOffset"); e.children.push(Node::Text(((off * EMU_PER_PX).round() as i64).to_string())); p.push(e); }
        }
        p
    };
    out.push(axis("wp:positionH", a.x_from, a.x, a.x_align, true));
    out.push(axis("wp:positionV", a.y_from, a.y, a.y_align, false));
    let wrap = match a.wrap {
        Wrap::Square => el("wp:wrapSquare").with_attr("wrapText", "bothSides"),
        Wrap::Tight => {
            let mut w = el("wp:wrapTight").with_attr("wrapText", "bothSides");
            let mut poly = el("wp:wrapPolygon").with_attr("edited", "0");
            poly.push(el("wp:start").with_attr("x", "0").with_attr("y", "0"));
            for (x, y) in [("0", "21600"), ("21600", "21600"), ("21600", "0"), ("0", "0")] { poly.push(el("wp:lineTo").with_attr("x", x).with_attr("y", y)); }
            w.push(poly);
            w
        }
        Wrap::TopBottom => el("wp:wrapTopAndBottom"),
        Wrap::Behind | Wrap::InFront => el("wp:wrapNone"),
    };
    // extent and effectExtent come first, then the wrap, then docPr and the graphic.
    let mut placed = false;
    for n in inline.children {
        let is_lead = matches!(&n, Node::Element(e) if e.name == "wp:extent" || e.name == "wp:effectExtent");
        if !is_lead && !placed { out.push(wrap.clone()); placed = true; }
        if let Node::Element(e) = n { out.push(e); } else { out.children.push(n); }
    }
    if !placed { out.push(wrap); }
    d.children.insert(k, Node::Element(out));
    d
}

/// A border side (§17.4.38): nil/none read as width 0, "no border".
fn read_border(b: &Element) -> Border {
    if matches!(b.attr("w:val"), Some("nil" | "none")) { return Border { width: 0.0, space: 0.0, color: None }; }
    Border {
        width: (b.attr("w:sz").and_then(|v| v.parse::<f32>().ok()).unwrap_or(4.0) / 8.0).max(0.25),
        space: b.attr("w:space").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0),
        color: b.attr("w:color").and_then(Color::from_hex),
    }
}

/// Margins as top, right, bottom, left twips; absent sides keep `fallback` (-1 = inherit).
fn read_margins(m: &Element, fallback: [i32; 4]) -> [i32; 4] {
    let side = |names: &[&str], i: usize| names.iter().find_map(|n| m.child(n)).filter(|x| x.attr("w:type").is_none_or(|t| t == "dxa")).and_then(|x| twip_attr(x, "w:w")).unwrap_or(fallback[i]);
    [side(&["w:top"], 0), side(&["w:right", "w:end"], 1), side(&["w:bottom"], 2), side(&["w:left", "w:start"], 3)]
}

fn border_element(name: &str, b: &Border) -> Element {
    if b.width <= 0.0 { return val(name, "nil"); }
    val(name, "single").with_attr("w:sz", ((b.width * 8.0).round() as i32).clamp(2, 96).to_string()).with_attr("w:space", "0").with_attr("w:color", b.color.map(|c| c.hex()).unwrap_or("auto".into()))
}

fn margins_element(name: &str, m: [i32; 4]) -> Element {
    let mut e = el(name);
    for (side, v) in [("w:top", m[0]), ("w:left", m[3]), ("w:bottom", m[2]), ("w:right", m[1])] {
        if v >= 0 { e.push(el(side).with_attr("w:w", v.to_string()).with_attr("w:type", "dxa")); }
    }
    e
}

fn revision_from(e: &Element, kind: RevKind) -> Revision {
    Revision { kind, author: e.attr("w:author").unwrap_or_default().into(), date: e.attr("w:date").unwrap_or_default().into() }
}

/// Deleted runs use w:delText / w:delInstrText (§17.13.5.14).
fn mark_deleted(e: &mut Element) {
    match e.name.as_str() { "w:t" => e.name = "w:delText".into(), "w:instrText" => e.name = "w:delInstrText".into(), _ => {} }
    for c in e.elements_mut() { mark_deleted(c); }
}

trait WithText { fn with(&self, text: String) -> Run; }
impl WithText for Run { fn with(&self, text: String) -> Run { Run { text, ..self.clone() } } }

/// The quoted text after a switch or keyword in a field instruction.
fn quoted_after(instr: &str, key: &str) -> Option<String> {
    let rest = instr.split_once(key)?.1.trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(rest.split('"').next().unwrap_or("").to_string())
}

/// Fields whose result is whole paragraphs, kept as field blocks.
fn block_field(instr: &str) -> bool { matches!(instr.split_whitespace().next().unwrap_or("").to_uppercase().as_str(), "TOC" | "INDEX" | "TOA" | "BIBLIOGRAPHY") }

/// Fields the layout computes: PAGE, NUMPAGES, and REF/PAGEREF to a bookmark (case of the name kept).
fn page_field(instr: &str) -> Option<Field> {
    if let Some(f) = crate::mailmerge::parse(instr) { return Some(f); }
    let mut words = instr.split_whitespace();
    let word = words.next().unwrap_or("").to_uppercase();
    match word.as_str() {
        "PAGE" => Some(Field::Page), "NUMPAGES" | "SECTIONPAGES" => Some(Field::NumPages),
        "DATE" | "TIME" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" => {
            // The picture after \@, quoted or not.
            let pic = instr.split_once("\\@").map(|(_, r)| { let r = r.trim_start(); if let Some(q) = r.strip_prefix('"') { q.split('"').next().unwrap_or("").to_string() } else { r.split_whitespace().next().unwrap_or("").to_string() } }).unwrap_or_default();
            Some(Field::Date(if word == "TIME" { format!("TIME:{pic}") } else { pic }))
        }
        "AUTHOR" | "TITLE" | "SUBJECT" | "KEYWORDS" | "COMMENTS" | "FILENAME" | "NUMWORDS" | "NUMCHARS" => Some(Field::Prop(word.clone())),
        "SEQ" => words.next().map(|l| Field::Seq(l.to_string())),
        "CITATION" => words.next().map(|t| Field::Citation(t.to_string())),
        "XE" => Some(Field::IndexEntry(quoted_after(instr, "XE").unwrap_or_default())),
        "TA" => Some(Field::Authority { long: quoted_after(instr, "\\l").unwrap_or_default(), short: quoted_after(instr, "\\s").unwrap_or_default(), category: instr.split("\\c").nth(1).and_then(|r| r.split_whitespace().next()).and_then(|v| v.trim_matches('"').parse().ok()).unwrap_or(1) }),
        "REF" => words.next().filter(|n| !n.starts_with('\\')).map(|n| Field::Ref(n.into())),
        "PAGEREF" => words.next().filter(|n| !n.starts_with('\\')).map(|n| Field::PageRef(n.into())),
        _ => None,
    }
}

fn content_type_for(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() { "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", "tif" | "tiff" => "image/tiff", "emf" => "image/x-emf", "wmf" => "image/x-wmf", "svg" => "image/svg+xml", _ => "application/octet-stream" }.into()
}

fn read_page(sect: &Element) -> PageSettings {
    let mut s = PageSettings::default();
    if let Some(sz) = sect.child("w:pgSz") {
        s.width = twip_attr(sz, "w:w").unwrap_or(s.width);
        s.height = twip_attr(sz, "w:h").unwrap_or(s.height);
        s.landscape = sz.attr("w:orient") == Some("landscape");
    }
    if let Some(m) = sect.child("w:pgMar") {
        s.top = twip_attr(m, "w:top").unwrap_or(s.top);
        s.bottom = twip_attr(m, "w:bottom").unwrap_or(s.bottom);
        s.left = twip_attr(m, "w:left").unwrap_or(s.left);
        s.right = twip_attr(m, "w:right").unwrap_or(s.right);
        s.header = twip_attr(m, "w:header").unwrap_or(s.header);
        s.footer = twip_attr(m, "w:footer").unwrap_or(s.footer);
        s.gutter = twip_attr(m, "w:gutter").unwrap_or(0);
    }
    s.title_page = sect.flag("w:titlePg").unwrap_or(false);
    s.section = read_section(sect);
    for (tag, slot) in [("w:footnotePr", &mut s.footnote_format), ("w:endnotePr", &mut s.endnote_format)] {
        if let Some(pr) = sect.child(tag) {
            if let Some(f) = pr.child("w:numFmt").and_then(|f| f.attr("w:val")) { slot.0 = crate::numbering::NumFmt::from_ooxml(f); }
            if let Some(n) = pr.child("w:numStart").and_then(|f| f.attr("w:val")).and_then(|v| v.parse().ok()) { slot.1 = n; }
        }
    }
    if let Some(b) = sect.child("w:pgBorders") {
        let sides = ["w:top", "w:left", "w:bottom", "w:right"].map(|n| b.child(n).filter(|x| !matches!(x.attr("w:val"), Some("nil" | "none"))));
        if let Some(first) = sides.iter().flatten().next() {
            s.borders = Some(crate::page::PageBorders {
                style: crate::page::BorderStyle::from_val(first.attr("w:val").unwrap_or("single")),
                width: first.attr("w:sz").and_then(|v| v.parse::<f32>().ok()).map(|v| v / 8.0).unwrap_or(0.5),
                color: first.attr("w:color").and_then(Color::from_hex).unwrap_or(Color::BLACK),
                space: first.attr("w:space").and_then(|v| v.parse::<f32>().ok()).unwrap_or(24.0),
                from_text: b.attr("w:offsetFrom") == Some("text"),
                sides: sides.map(|x| x.is_some()),
            });
        }
    }
    s
}

/// Open a .docx (or .dotx/.docm) file.
pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("Not a DOCX package: {e}"))?;
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() { continue; }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        parts.insert(f.name().trim_start_matches('/').to_string(), buf);
    }
    let main = read_rels(&parts, "").into_iter().find(|r| r.kind == REL_DOC || r.kind.ends_with("/officeDocument")).map(|r| r.target.trim_start_matches('/').to_string()).unwrap_or_else(|| "word/document.xml".into());
    let doc_xml = part_text(&parts, &main).ok_or("The package has no main document part.")?;
    let root = xml::parse(&doc_xml)?;
    let body = root.child("w:body").ok_or("The main document has no body.")?;
    let rels = read_rels(&parts, &main);

    let theme = rels.iter().find(|r| r.kind.ends_with("/theme")).and_then(|r| part_text(&parts, &resolve_target(&main, &r.target))).and_then(|t| xml::parse(&t).ok());
    let font_of = |which: &str, script: &str| theme.as_ref().and_then(|t| t.find(which)).and_then(|f| f.child(script)).and_then(|l| l.attr("typeface")).filter(|t| !t.is_empty()).map(String::from);
    let reader = Reader {
        parts: &parts, theme_minor: font_of("a:minorFont", "a:latin"), theme_major: font_of("a:majorFont", "a:latin"),
        theme_minor_cs: font_of("a:minorFont", "a:cs"), theme_major_cs: font_of("a:majorFont", "a:cs"),
    };

    let styles = rels.iter().find(|r| r.kind == REL_STYLES).and_then(|r| part_text(&parts, &resolve_target(&main, &r.target))).and_then(|t| xml::parse(&t).ok())
        .map(|s| reader.styles(&s)).unwrap_or_default();
    let numbering = rels.iter().find(|r| r.kind == REL_NUMBERING).and_then(|r| part_text(&parts, &resolve_target(&main, &r.target))).and_then(|t| xml::parse(&t).ok())
        .map(|n| reader.numbering(&n)).unwrap_or_default();

    let mut br = BodyReader { r: &reader, rels: &rels, part: &main, paragraphs: vec![], tables: vec![], assets: HashMap::new(), field: vec![], revision: None, bookmark_ids: HashMap::new() };
    for b in body.elements() { br.block(b, None); }
    let sect = body.child("w:sectPr").cloned();
    let page = sect.as_ref().map(read_page).unwrap_or_default();

    // Default header/footer stories.
    let mut stories: [Option<Story>; 2] = [None, None];
    if let Some(sect) = &sect {
        for (k, name) in ["w:headerReference", "w:footerReference"].iter().enumerate() {
            let refs: Vec<&Element> = sect.children_named(name).collect();
            let chosen = refs.iter().find(|r| r.attr("w:type") == Some("default")).or(refs.first());
            if let Some(path) = chosen.and_then(|r| r.attr("r:id")).and_then(|id| rels.iter().find(|r| r.id == id)).map(|r| resolve_target(&main, &r.target)) {
                if let Some(x) = part_text(&parts, &path).and_then(|t| xml::parse(&t).ok()) {
                    let story_rels = read_rels(&parts, &path);
                    let mut sr = BodyReader { r: &reader, rels: &story_rels, part: &path, paragraphs: vec![], tables: vec![], assets: HashMap::new(), field: vec![], revision: None, bookmark_ids: HashMap::new() };
                    for b in x.elements() { sr.block(b, None); }
                    // Story tables are flattened to their paragraphs.
                    for p in &mut sr.paragraphs { p.cell = None; }
                    br.assets.extend(sr.assets);
                    if sr.paragraphs.iter().any(|p| !p.is_empty()) { stories[k] = Some(Story { paragraphs: sr.paragraphs }); }
                }
            }
        }
    }

    // Bibliography sources live in a customXml part (b:Sources).
    let mut sources = None;
    let mut macros = None;
    for r in rels.iter().filter(|r| r.kind == REL_CUSTOM_XML) {
        let path = resolve_target(&main, &r.target);
        let Some(text) = part_text(&parts, &path) else { continue };
        if sources.is_none() { if let Some(found) = crate::references::read_sources(&text) { sources = Some((path.clone(), found)); continue; } }
        if macros.is_none() { if let Some(found) = crate::macros::read_macros(&text) { macros = Some((path, found)); } }
    }
    let comments = read_comments(&parts, &main, &rels);
    let mut notes = vec![];
    let mut note_specials = vec![];
    for (kind, rel, tag) in [(NoteKind::Foot, "/footnotes", "w:footnote"), (NoteKind::End, "/endnotes", "w:endnote")] {
        let Some(path) = rels.iter().find(|r| r.kind.ends_with(rel)).map(|r| resolve_target(&main, &r.target)) else { continue };
        let Some(root) = part_text(&parts, &path).and_then(|t| xml::parse(&t).ok()) else { continue };
        let note_rels = read_rels(&parts, &path);
        for n in root.children_named(tag) {
            if n.attr("w:type").is_some_and(|t| t != "normal") { note_specials.push((kind, n.to_xml())); continue; }
            let mut nr = BodyReader { r: &reader, rels: &note_rels, part: &path, paragraphs: vec![], tables: vec![], assets: HashMap::new(), field: vec![], revision: None, bookmark_ids: HashMap::new() };
            for b in n.elements() { nr.block(b, None); }
            for p in &mut nr.paragraphs { p.cell = None; }
            // The space after the note's own number belongs to the number.
            if let Some(r) = nr.paragraphs.first_mut().and_then(|p| p.runs.first_mut()) { if r.text.starts_with(' ') { r.text.remove(0); } }
            for p in &mut nr.paragraphs { p.runs.retain(|r| !r.text.is_empty() || r.marker.is_some()); }
            br.assets.extend(nr.assets);
            notes.push(Note { kind, id: n.attr("w:id").unwrap_or_default().into(), paragraphs: nr.paragraphs });
        }
    }
    let BodyReader { paragraphs, tables, assets, .. } = br;
    let mut doc = Document::new(paragraphs);
    // Document properties (docProps/core.xml).
    if let Some(core) = part_text(&parts, "docProps/core.xml").and_then(|t| xml::parse(&t).ok()) {
        let get = |local: &str| core.elements().find(|e| e.name.rsplit(':').next() == Some(local)).map(|e| e.text()).unwrap_or_default();
        doc.properties = crate::model::Properties { title: get("title"), subject: get("subject"), author: get("creator"), keywords: get("keywords"), description: get("description") };
    }
    doc.tables = tables;
    doc.styles = styles;
    doc.numbering = numbering;
    if page.validate().is_ok() { doc.page = page; }
    if let Some((path, list)) = macros { doc.macros = list; doc.macros_part = Some(path); }
    if let Some((path, (list, style))) = sources { doc.sources = list; doc.citation_style = style; doc.sources_part = Some(path); }
    doc.page.color = root.child("w:background").and_then(|b| b.attr("w:color")).and_then(Color::from_hex);
    doc.assets = assets;
    let [header, footer] = stories;
    doc.header = header;
    doc.footer = footer;
    if let Some(d) = parts.get("word/settings.xml").and_then(|b| xml::parse(&String::from_utf8_lossy(b)).ok()) {
        doc.track_changes = d.flag("w:trackRevisions").unwrap_or(false);
        if let Some(t) = d.child("w:defaultTabStop").and_then(|t| twip_attr(t, "w:val")).filter(|v| *v > 0) { doc.page.default_tab_stop = t; }
        let h = &mut doc.page.hyphenation;
        h.auto = d.flag("w:autoHyphenation").unwrap_or(false);
        if let Some(z) = d.child("w:hyphenationZone").and_then(|t| twip_attr(t, "w:val")) { h.zone = z; }
        if let Some(n) = d.child("w:consecutiveHyphenLimit").and_then(|t| t.attr("w:val")).and_then(|v| v.parse().ok()) { h.limit = n; }
        h.caps = !d.flag("w:doNotHyphenateCaps").unwrap_or(false);
        doc.protection = d.child("w:documentProtection").and_then(crate::review::read_protection);
    }
    doc.comments = comments;
    doc.notes = notes;
    doc.note_specials = note_specials;
    doc.package = Some(Package { root_attrs: root.attrs.clone(), body_sect: sect, rels, main, parts });
    Ok(doc)
}

/// Comment bodies from comments.xml, with replies and resolution from commentsExtended.xml.
fn read_comments(parts: &BTreeMap<String, Vec<u8>>, main: &str, rels: &[Rel]) -> Vec<Comment> {
    let part = |kind: &str| rels.iter().find(|r| r.kind == kind).and_then(|r| part_text(parts, &resolve_target(main, &r.target))).and_then(|t| xml::parse(&t).ok());
    let Some(root) = part(REL_COMMENTS) else { return vec![] };
    let mut out: Vec<Comment> = root.children_named("w:comment").map(|c| {
        let paras: Vec<&Element> = c.children_named("w:p").collect();
        let text = paras.iter().map(|p| {
            let mut t = String::new();
            let mut all = vec![];
            p.find_all("w:r", &mut all);
            for r in all { for x in r.elements() { match x.name.as_str() { "w:t" => t.push_str(&x.text()), "w:tab" => t.push('\t'), "w:br" => t.push('\n'), _ => {} } } }
            t
        }).collect::<Vec<_>>().join("\n");
        Comment {
            id: c.attr("w:id").unwrap_or_default().into(), author: c.attr("w:author").unwrap_or_default().into(),
            initials: c.attr("w:initials").unwrap_or_default().into(), date: c.attr("w:date").unwrap_or_default().into(),
            text, parent: None, done: false,
            para_id: paras.last().and_then(|p| p.attr("w14:paraId")).unwrap_or_default().into(),
        }
    }).collect();
    if let Some(ex) = part(REL_COMMENTS_EX) {
        for e in ex.children_named("w15:commentEx") {
            let Some(pid) = e.attr("w15:paraId") else { continue };
            let parent = e.attr("w15:paraIdParent").and_then(|pp| out.iter().find(|c| c.para_id == pp)).map(|c| c.id.clone());
            if let Some(c) = out.iter_mut().find(|c| c.para_id == pid) {
                c.done = matches!(e.attr("w15:done"), Some("1" | "true"));
                c.parent = parent;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Writing

struct Writer<'a> {
    doc: &'a Document,
    rels: Vec<Rel>,
    new_parts: BTreeMap<String, Vec<u8>>,
    main: String,
    next_rel: u32,
    next_drawing: u32,
    next_revision: std::cell::Cell<u32>,
    bookmark_ids: std::cell::RefCell<HashMap<String, usize>>,
    /// (w:headerReference / w:footerReference, relationship id) for stories written anew.
    story_refs: Vec<(String, String)>,
    /// SEQ results in document order, and how many have been written.
    seq_values: Vec<u32>,
    field_pos: std::cell::Cell<usize>,
}

fn el(name: &str) -> Element { Element::new(name) }
fn val(name: &str, v: impl Into<String>) -> Element { el(name).with_attr("w:val", v) }
fn raw_nodes(raw: &[String]) -> Vec<Node> {
    // Preserved fragments are parsed back with the canonical prefixes declared.
    let decls: String = xml::NAMESPACES.iter().map(|(p, u)| format!(" xmlns:{p}=\"{u}\"")).collect();
    raw.iter().filter_map(|r| xml::parse(&format!("<w:x{decls}>{r}</w:x>")).ok()).flat_map(|w| w.children).collect()
}

impl<'a> Writer<'a> {
    fn rel_id(&mut self, kind: &str, target: &str, external: bool) -> String {
        if let Some(r) = self.rels.iter().find(|r| r.kind == kind && r.target == target) { return r.id.clone(); }
        loop {
            let id = format!("rId{}", self.next_rel);
            self.next_rel += 1;
            if !self.rels.iter().any(|r| r.id == id) {
                self.rels.push(Rel { id: id.clone(), kind: kind.into(), target: target.into(), external });
                return id;
            }
        }
    }

    fn run_props(&self, p: &RunProps, style: &Option<String>, extra: &[String]) -> Option<Element> {
        let mut r = el("w:rPr");
        if let Some(s) = style { r.push(val("w:rStyle", s.clone())); }
        if p.font.is_some() || p.cs_font.is_some() {
            let mut fonts = el("w:rFonts");
            if let Some(f) = &p.font { fonts = fonts.with_attr("w:ascii", f.clone()).with_attr("w:hAnsi", f.clone()).with_attr("w:eastAsia", f.clone()); }
            if let Some(f) = p.cs_font.as_ref().or(p.font.as_ref()) { fonts = fonts.with_attr("w:cs", f.clone()); }
            r.push(fonts);
        }
        let mut nodes = raw_nodes(extra);
        nodes.retain(|n| !matches!(n, Node::Element(e) if (p.font.is_some() || p.cs_font.is_some()) && e.name == "w:rFonts"));
        let onoff = |r: &mut Element, name: &str, v: Option<bool>| if let Some(v) = v { r.push(if v { el(name) } else { val(name, "0") }); };
        onoff(&mut r, "w:b", p.bold);
        onoff(&mut r, "w:bCs", p.bold_cs);
        onoff(&mut r, "w:i", p.italic);
        onoff(&mut r, "w:iCs", p.italic_cs);
        onoff(&mut r, "w:rtl", p.rtl);
        onoff(&mut r, "w:outline", p.outline);
        onoff(&mut r, "w:shadow", p.shadow);
        if let Some(Some(g)) = p.glow {
            let clr = el("w14:srgbClr").with_attr("w14:val", g.color.hex()).with_child(el("w14:alpha").with_attr("w14:val", "40000"));
            r.push(el("w14:glow").with_attr("w14:rad", ((g.radius * 12700.0).round() as i64).to_string()).with_child(clr));
        }
        if p.reflection == Some(true) {
            let mut x = el("w14:reflection");
            for (k, v) in [("w14:blurRad", "6350"), ("w14:stA", "55000"), ("w14:stPos", "0"), ("w14:endA", "300"), ("w14:endPos", "45500"), ("w14:dist", "0"), ("w14:dir", "5400000"), ("w14:fadeDir", "5400000"), ("w14:sx", "100000"), ("w14:sy", "-100000"), ("w14:kx", "0"), ("w14:ky", "0"), ("w14:algn", "bl")] { x.set_attr(k, v); }
            r.push(x);
        }
        if let Some(Some((a, b))) = p.gradient {
            let gs = |pos: &str, c: Color| el("w14:gs").with_attr("w14:pos", pos).with_child(el("w14:srgbClr").with_attr("w14:val", c.hex()));
            let grad = el("w14:gradFill").with_child(el("w14:gsLst").with_child(gs("0", a)).with_child(gs("100000", b))).with_child(el("w14:lin").with_attr("w14:ang", "5400000").with_attr("w14:scaled", "0"));
            r.push(el("w14:textFill").with_child(grad));
        }
        match p.caps { Some(Caps::AllCaps) => r.push(el("w:caps")), Some(Caps::SmallCaps) => r.push(el("w:smallCaps")), Some(Caps::Normal) => r.push(val("w:caps", "0")), None => {} }
        match p.strike { Some(Strike::Single) => r.push(el("w:strike")), Some(Strike::Double) => r.push(el("w:dstrike")), Some(Strike::None) => { r.push(val("w:strike", "0")); r.push(val("w:dstrike", "0")); } None => {} }
        onoff(&mut r, "w:vanish", p.vanish);
        if let Some(c) = p.color { r.push(val("w:color", c.map(|c| c.hex()).unwrap_or("auto".into()))); }
        if let Some(s) = p.spacing { r.push(val("w:spacing", ((s * 20.0).round() as i32).to_string())); }
        if let Some(s) = p.size { r.push(val("w:sz", ((s * 2.0).round() as i32).to_string())); }
        if let Some(s) = p.size_cs { r.push(val("w:szCs", ((s * 2.0).round() as i32).to_string())); }
        if let Some(h) = &p.highlight { r.push(val("w:highlight", h.clone().unwrap_or("none".into()))); }
        if let Some(u) = &p.underline { r.push(val("w:u", u.clone())); }
        if let Some(s) = p.shading { r.push(el("w:shd").with_attr("w:val", "clear").with_attr("w:color", "auto").with_attr("w:fill", s.map(|c| c.hex()).unwrap_or("auto".into()))); }
        if let Some(v) = p.vert_align { r.push(val("w:vertAlign", match v { VertAlign::Superscript => "superscript", VertAlign::Subscript => "subscript", VertAlign::Baseline => "baseline" })); }
        r.children.extend(nodes);
        sort_children(&mut r, RPR_ORDER);
        (!r.children.is_empty()).then_some(r)
    }

    fn para_props(&self, p: &Paragraph) -> Option<Element> {
        let mut e = el("w:pPr");
        let pp = &p.props;
        if let Some(s) = &p.style { e.push(val("w:pStyle", s.clone())); }
        let onoff = |e: &mut Element, name: &str, v: Option<bool>| if let Some(v) = v { e.push(if v { el(name) } else { val(name, "0") }); };
        onoff(&mut e, "w:keepNext", pp.keep_next);
        onoff(&mut e, "w:keepLines", pp.keep_lines);
        onoff(&mut e, "w:pageBreakBefore", pp.page_break_before);
        if let Some(d) = pp.drop_cap {
            e.push(el("w:framePr").with_attr("w:dropCap", if d.margin { "margin" } else { "drop" }).with_attr("w:lines", d.lines.to_string()).with_attr("w:hSpace", d.distance.to_string()).with_attr("w:wrap", "around").with_attr("w:vAnchor", "text").with_attr("w:hAnchor", "text"));
        }
        onoff(&mut e, "w:widowControl", pp.widow_control);
        onoff(&mut e, "w:bidi", pp.rtl);
        onoff(&mut e, "w:suppressLineNumbers", pp.suppress_line_numbers);
        onoff(&mut e, "w:suppressAutoHyphens", pp.suppress_hyphens);
        if let Some(sec) = &p.section { e.push(self.paragraph_sect_pr(sec)); }
        if let Some((num, lvl)) = pp.numbering { e.push(el("w:numPr").with_child(val("w:ilvl", lvl.to_string())).with_child(val("w:numId", num.to_string()))); }
        if let Some(b) = &pp.borders {
            let mut x = el("w:pBdr");
            for (name, side) in [("w:top", &b.top), ("w:left", &b.left), ("w:bottom", &b.bottom), ("w:right", &b.right)] {
                if let Some(s) = side { x.push(val(name, "single").with_attr("w:sz", ((s.width * 8.0).round() as i32).to_string()).with_attr("w:space", (s.space.round() as i32).to_string()).with_attr("w:color", s.color.map(|c| c.hex()).unwrap_or("auto".into()))); }
            }
            e.push(x);
        }
        if let Some(s) = pp.shading { e.push(el("w:shd").with_attr("w:val", "clear").with_attr("w:color", "auto").with_attr("w:fill", s.map(|c| c.hex()).unwrap_or("auto".into()))); }
        if let Some(tabs) = &pp.tabs {
            let mut t = el("w:tabs");
            for s in tabs {
                let kind = match s.kind { TabKind::Left => "left", TabKind::Center => "center", TabKind::Right => "right", TabKind::Decimal => "decimal", TabKind::Clear => "clear" };
                let mut x = val("w:tab", kind).with_attr("w:pos", s.pos.to_string());
                let leader = match s.leader { Leader::Dot => Some("dot"), Leader::Hyphen => Some("hyphen"), Leader::Underscore => Some("underscore"), Leader::MiddleDot => Some("middleDot"), Leader::None => None };
                if let Some(l) = leader { x.set_attr("w:leader", l); }
                t.push(x);
            }
            e.push(t);
        }
        if pp.space_before.is_some() || pp.space_after.is_some() || pp.line_spacing.is_some() {
            let mut s = el("w:spacing");
            if let Some(v) = pp.space_before { s.set_attr("w:before", ((v * 20.0).round() as i32).to_string()); }
            if let Some(v) = pp.space_after { s.set_attr("w:after", ((v * 20.0).round() as i32).to_string()); }
            if let Some(v) = pp.line_spacing {
                let rule = pp.line_rule.unwrap_or(LineRule::Auto);
                let line = if rule == LineRule::Auto { v * 240.0 } else { v * 20.0 };
                s.set_attr("w:line", (line.round() as i32).to_string());
                s.set_attr("w:lineRule", match rule { LineRule::Auto => "auto", LineRule::Exact => "exact", LineRule::AtLeast => "atLeast" });
            }
            e.push(s);
        }
        if pp.indent_left.is_some() || pp.indent_right.is_some() || pp.first_line.is_some() {
            let mut i = el("w:ind");
            if let Some(v) = pp.indent_left { i.set_attr("w:left", v.to_string()); }
            if let Some(v) = pp.indent_right { i.set_attr("w:right", v.to_string()); }
            if let Some(v) = pp.first_line { if v < 0 { i.set_attr("w:hanging", (-v).to_string()); } else { i.set_attr("w:firstLine", v.to_string()); } }
            e.push(i);
        }
        onoff(&mut e, "w:contextualSpacing", pp.contextual_spacing);
        if let Some(a) = pp.alignment { e.push(val("w:jc", a.ooxml())); }
        if let Some(l) = pp.outline_level { e.push(val("w:outlineLvl", l.to_string())); }
        e.children.extend(raw_nodes(&p.extra));
        let mut mark = self.run_props(&p.mark, &None, &[]);
        if let Some(rv) = &p.mark_revision {
            let x = self.revision_element(rv);
            mark.get_or_insert_with(|| el("w:rPr")).children.insert(0, Node::Element(x));
        }
        if let Some(r) = mark { e.push(r); }
        sort_children(&mut e, PPR_ORDER);
        (!e.children.is_empty()).then_some(e)
    }

    fn revision_element(&self, rv: &Revision) -> Element {
        self.next_revision.set(self.next_revision.get() + 1);
        let mut w = el(if rv.kind == RevKind::Ins { "w:ins" } else { "w:del" }).with_attr("w:id", self.next_revision.get().to_string()).with_attr("w:author", rv.author.clone());
        if !rv.date.is_empty() { w.set_attr("w:date", rv.date.clone()); }
        w
    }

    fn run_elements(&mut self, run: &Run, out: &mut Vec<Element>) {
        if let Some(Marker::BookmarkStart(name) | Marker::BookmarkEnd(name)) = &run.marker {
            let n = self.bookmark_ids.borrow().len();
            let id = self.bookmark_ids.borrow_mut().entry(name.clone()).or_insert(n).to_string();
            out.push(match run.marker { Some(Marker::BookmarkStart(_)) => el("w:bookmarkStart").with_attr("w:id", id).with_attr("w:name", name.clone()), _ => el("w:bookmarkEnd").with_attr("w:id", id) });
            return;
        }
        if let Some(Marker::FieldBegin(instr)) = &run.marker {
            let mut r = el("w:r");
            r.push(el("w:fldChar").with_attr("w:fldCharType", "begin"));
            let mut i = el("w:instrText").with_attr("xml:space", "preserve");
            i.children.push(Node::Text(format!(" {instr} ")));
            r.push(i);
            r.push(el("w:fldChar").with_attr("w:fldCharType", "separate"));
            out.push(r);
            return;
        }
        if let Some(Marker::FieldEnd) = &run.marker {
            out.push(el("w:r").with_child(el("w:fldChar").with_attr("w:fldCharType", "end")));
            return;
        }
        if let Some(m) = &run.marker {
            let (start, id) = match m { Marker::CommentStart(id) => (true, id), Marker::CommentEnd(id) => (false, id), _ => unreachable!() };
            // Replies created here have no markers of their own; they share the root's range.
            let mut ids = vec![id.clone()];
            let has_own = |c: &Comment| self.doc.paragraphs.iter().any(|p| p.runs.iter().any(|r| matches!(&r.marker, Some(Marker::CommentStart(x)) if *x == c.id)));
            ids.extend(self.doc.comments.iter().filter(|c| c.parent.as_deref() == Some(id) && !has_own(c)).map(|c| c.id.clone()));
            for id in ids {
                if start { out.push(el("w:commentRangeStart").with_attr("w:id", id)); }
                else {
                    out.push(el("w:commentRangeEnd").with_attr("w:id", id.clone()));
                    out.push(el("w:r").with_child(el("w:commentReference").with_attr("w:id", id)));
                }
            }
            return;
        }
        if let Some(rv) = &run.revision {
            let mut inner = vec![];
            let plain = Run { revision: None, ..run.clone() };
            self.run_elements(&plain, &mut inner);
            if rv.kind == RevKind::Del { for e in &mut inner { mark_deleted(e); } }
            let mut w = self.revision_element(rv);
            for e in inner { w.push(e); }
            out.push(w);
            return;
        }
        let rpr = self.run_props(&run.props, &run.style, &run.extra);
        let mut r = el("w:r");
        if let Some(p) = &rpr { r.push(p.clone()); }
        if let Some(f) = &run.field {
            let code = match f {
                Field::Page => " PAGE ".to_string(), Field::NumPages => " NUMPAGES ".to_string(), Field::Ref(n) => format!(" REF {n} \\h "), Field::PageRef(n) => format!(" PAGEREF {n} \\h "),
                Field::Date(p) => match p.strip_prefix("TIME:") { Some(t) => format!(" TIME \\@ \"{t}\" "), None => format!(" DATE \\@ \"{p}\" ") },
                Field::Prop(n) => format!(" {n} "),
                Field::Seq(l) => format!(" SEQ {l} \\* ARABIC "),
                Field::Citation(t) => format!(" CITATION {t} \\l 1033 "),
                Field::IndexEntry(e) => format!(" XE \"{}\" ", e.replace('"', "'")),
                Field::Authority { long, short, category } => format!(" TA \\l \"{}\" \\s \"{}\" \\c {category} ", long.replace('"', "'"), short.replace('"', "'")),
                f => crate::mailmerge::instruction(f).unwrap_or_default(),
            };
            let result = match f {
                Field::Ref(n) => self.doc.bookmark_text(n).unwrap_or_default(), Field::Date(p) => crate::fields::date_value(self.doc, p), Field::Prop(n) => crate::fields::property(self.doc, n),
                Field::Citation(t) => crate::references::citation_text(self.doc, t, &crate::references::citation_numbers(self.doc)),
                f if crate::mailmerge::is_merge_field(f) => crate::mailmerge::MailMerge::default().text_for(f, None, 1),
                Field::Seq(_) => { let pos = self.field_pos.get(); self.field_pos.set(pos + 1); self.seq_values.get(pos).map(|n| n.to_string()).unwrap_or_else(|| "1".into()) }
                _ => "1".into(),
            };
            // XE and TA fields have no result.
            if matches!(f, Field::IndexEntry(_) | Field::Authority { .. } | Field::NextRecord | Field::NextIf { .. } | Field::SkipIf { .. }) {
                r.push(el("w:fldChar").with_attr("w:fldCharType", "begin"));
                let mut instr = el("w:instrText").with_attr("xml:space", "preserve");
                instr.children.push(Node::Text(code));
                r.push(instr);
                r.push(el("w:fldChar").with_attr("w:fldCharType", "end"));
                out.push(r);
                return;
            }
            // PAGEREF results depend on pagination: Word recalculates fields marked dirty on open.
            let mut begin = el("w:fldChar").with_attr("w:fldCharType", "begin");
            if matches!(f, Field::PageRef(_)) { begin.set_attr("w:dirty", "true"); }
            r.push(begin);
            // Nested fields in the instruction ({MERGEFIELD X}) are written as real nested fields.
            for (k, part) in code.split(['{', '}']).enumerate() {
                if k % 2 == 1 {
                    r.push(el("w:fldChar").with_attr("w:fldCharType", "begin"));
                    let mut i = el("w:instrText").with_attr("xml:space", "preserve"); i.children.push(Node::Text(format!(" {} ", part.trim()))); r.push(i);
                    r.push(el("w:fldChar").with_attr("w:fldCharType", "separate"));
                    let name = part.trim().strip_prefix("MERGEFIELD").unwrap_or(part).trim();
                    let mut t = el("w:t"); t.children.push(Node::Text(format!("«{name}»"))); r.push(t);
                    r.push(el("w:fldChar").with_attr("w:fldCharType", "end"));
                } else if !part.is_empty() {
                    let mut i = el("w:instrText").with_attr("xml:space", "preserve"); i.children.push(Node::Text(part.to_string())); r.push(i);
                }
            }
            r.push(el("w:fldChar").with_attr("w:fldCharType", "separate"));
            let mut t = el("w:t").with_attr("xml:space", "preserve"); t.children.push(Node::Text(result)); r.push(t);
            r.push(el("w:fldChar").with_attr("w:fldCharType", "end"));
            out.push(r);
            return;
        }
        if let Some(n) = &run.note {
            if run.style.is_none() && run.props.vert_align.is_none() {
                r.children.retain(|c| !matches!(c, Node::Element(e) if e.name == "w:rPr"));
                let mut rpr = self.run_props(&RunProps { vert_align: Some(VertAlign::Superscript), ..run.props.clone() }, &None, &run.extra).unwrap();
                sort_children(&mut rpr, RPR_ORDER);
                r.children.insert(0, Node::Element(rpr));
            }
            r.push(el(if n.kind == NoteKind::Foot { "w:footnoteReference" } else { "w:endnoteReference" }).with_attr("w:id", n.id.clone()));
            out.push(r);
            return;
        }
        if let Some(crate::objects::Object::Equation(eq)) = &run.object {
            let math = crate::equation::omml(eq);
            let xml = if eq.display { format!(r#"<m:oMathPara><m:oMathParaPr><m:jc m:val="center"/></m:oMathParaPr>{math}</m:oMathPara>"#) } else { math };
            if let Some(Node::Element(e)) = raw_nodes(&[xml]).into_iter().next() { out.push(e); }
            return;
        }
        if let Some(obj) = &run.object {
            let d = self.object_drawing(obj);
            r.push(anchorize(d, run.anchor, self.next_drawing));
            out.push(r);
            return;
        }
        if let Some(img) = &run.image {
            let d = self.drawing(img);
            r.push(anchorize(d, run.anchor, self.next_drawing));
            out.push(r);
            return;
        }
        let mut text = String::new();
        let flush = |text: &mut String, r: &mut Element| if !text.is_empty() {
            let mut t = el("w:t");
            if text.starts_with(' ') || text.ends_with(' ') || text.contains("  ") { t.set_attr("xml:space", "preserve"); }
            t.children.push(Node::Text(std::mem::take(text)));
            r.push(t);
        };
        for c in run.text.chars() {
            match c {
                '\t' => { flush(&mut text, &mut r); r.push(el("w:tab")); }
                '\u{AD}' => { flush(&mut text, &mut r); r.push(el("w:softHyphen")); }
                chars::LINE_BREAK => { flush(&mut text, &mut r); r.push(el("w:br")); }
                chars::PAGE_BREAK => { flush(&mut text, &mut r); r.push(el("w:br").with_attr("w:type", "page")); }
                chars::COLUMN_BREAK => { flush(&mut text, &mut r); r.push(el("w:br").with_attr("w:type", "column")); }
                '\u{2011}' => { flush(&mut text, &mut r); r.push(el("w:noBreakHyphen")); }
                chars::OBJECT => {}
                c => text.push(c),
            }
        }
        flush(&mut text, &mut r);
        out.push(r);
    }

    /// A free part path like `word/charts/chart3.xml`.
    fn new_part_name(&self, prefix: &str, ext: &str) -> String {
        let taken = |p: &String| self.new_parts.contains_key(p) || self.doc.package.as_ref().is_some_and(|k| k.parts.contains_key(p)) || self.doc.assets.contains_key(p);
        (1..).map(|n| format!("{prefix}{n}.{ext}")).find(|p| !taken(p)).unwrap()
    }

    fn object_drawing(&mut self, obj: &crate::objects::Object) -> Element {
        use crate::objects::*;
        self.next_drawing += 1;
        let id = self.next_drawing;
        let xml = match obj {
            Object::Shape(s) => s.raw.clone().unwrap_or_else(|| shape_drawing_xml(s, id)),
            Object::Chart(c) => match (&c.raw, &c.part) {
                (Some(raw), Some(_)) => raw.clone(),
                _ => {
                    let path = self.new_part_name("word/charts/chart", "xml");
                    self.new_parts.insert(path.clone(), chart_part_xml(c).into_bytes());
                    let rid = self.rel_id(REL_CHART, path.trim_start_matches("word/"), false);
                    chart_drawing_xml(c, &rid, id)
                }
            },
            Object::Ink(i) => ink_drawing_xml(i, id),
            Object::Equation(_) => return el("w:drawing"),
            Object::Diagram(d) => {
                let path = self.new_part_name("word/media/diagram", "png");
                if let Some(png) = diagram_png(d) { self.new_parts.insert(path.clone(), png); }
                let img = Image { asset: path, width: d.width, height: d.height, alt: d.to_spec(), ..Default::default() };
                return self.drawing(&img);
            }
        };
        match raw_nodes(&[xml]).into_iter().next() { Some(Node::Element(e)) => e, _ => el("w:drawing") }
    }

    fn drawing(&mut self, img: &Image) -> Element {
        if let Some(raw) = &img.raw {
            if let Some(Node::Element(e)) = raw_nodes(std::slice::from_ref(raw)).into_iter().next() { return e; }
        }
        // Edits Word can express are written as DrawingML over the original picture; otherwise
        // the edited pixels are saved and the original rides along for Reset Picture.
        let fx = img.original.as_ref().filter(|o| self.doc.assets.contains_key(*o)).and_then(|_| crate::picture::drawingml(&img.effects, img.width, img.height));
        let blip_asset = match (&fx, &img.original) { (Some(_), Some(o)) => o.clone(), _ => img.asset.clone() };
        let part_target = |main: &str, a: &str| a.strip_prefix(main.rsplit_once('/').map(|(d, _)| d).unwrap_or("")).unwrap_or(a).trim_start_matches('/').to_string();
        let target = part_target(&self.main, &blip_asset);
        if let Some(a) = self.doc.assets.get(&blip_asset) { self.new_parts.insert(blip_asset.clone(), a.bytes.clone()); }
        let rid = self.rel_id(REL_IMAGE, &target, false);
        self.next_drawing += 1;
        let id = self.next_drawing;
        let (cx, cy) = (((img.width * EMU_PER_PX).round() as i64).to_string(), ((img.height * EMU_PER_PX).round() as i64).to_string());
        let rot = (img.rotation.rem_euclid(360.0) * 60000.0).round() as i64;
        let xfrm = format!("{}{}{}", if rot != 0 { format!(r#" rot="{rot}""#) } else { String::new() }, if img.flip_h { r#" flipH="1""# } else { "" }, if img.flip_v { r#" flipV="1""# } else { "" });
        let original = match img.original.as_ref().filter(|o| **o != img.asset && fx.is_none()).and_then(|o| self.doc.assets.get(o).map(|a| (o.clone(), a.bytes.clone()))) {
            Some((o, bytes)) => {
                self.new_parts.insert(o.clone(), bytes);
                let t = part_target(&self.main, &o);
                let orid = self.rel_id(REL_IMAGE, &t, false);
                format!(r#"<a:extLst><a:ext uri="{{6F2C1D4E-8A3B-4C5D-9E7F-1A2B3C4D5E6F}}"><wo:original xmlns:wo="http://schemas.woffice.app/picture/2026" r:embed="{orid}"/></a:ext></a:extLst>"#)
            }
            None => String::new(),
        };
        let fx = fx.unwrap_or(crate::picture::PictureXml { blip: String::new(), src_rect: String::new(), geom: "rect", ln: String::new(), effects: String::new() });
        let blip = if fx.blip.is_empty() { format!(r#"<a:blip r:embed="{rid}"/>"#) } else { format!(r#"<a:blip r:embed="{rid}">{}</a:blip>"#, fx.blip) };
        let xml = format!(
            r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="Picture {id}" descr="{alt}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="Picture {id}">{original}</pic:cNvPr><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill>{blip}{src}<a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm{xfrm}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="{geom}"><a:avLst/></a:prstGeom>{ln}{eff}</pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#,
            alt = { let mut s = String::new(); xml::escape_into(&img.alt, &mut s, true); s }, src = fx.src_rect, geom = fx.geom, ln = fx.ln, eff = fx.effects);
        match raw_nodes(&[xml]).into_iter().next() { Some(Node::Element(e)) => e, _ => el("w:drawing") }
    }

    fn paragraph(&mut self, p: &Paragraph) -> Element {
        let mut e = el("w:p");
        if let Some(ppr) = self.para_props(p) { e.push(ppr); }
        let mut k = 0;
        while k < p.runs.len() {
            let link = p.runs[k].link.clone();
            let mut group = vec![];
            while k < p.runs.len() && p.runs[k].link == link { self.run_elements(&p.runs[k], &mut group); k += 1; }
            match link {
                Some(target) => {
                    let mut h = el("w:hyperlink");
                    if let Some(anchor) = target.strip_prefix('#') { h.set_attr("w:anchor", anchor); }
                    else { let id = self.rel_id(REL_LINK, &target, true); h.set_attr("r:id", id); }
                    for g in group { h.push(g); }
                    e.push(h);
                }
                None => for g in group { e.push(g); },
            }
        }
        e
    }

    fn table(&mut self, t: &Table, paragraphs: &[&Paragraph]) -> Element {
        let mut e = el("w:tbl");
        let tblpr = t.raw_props.as_ref().and_then(|r| match raw_nodes(std::slice::from_ref(r)).into_iter().next() { Some(Node::Element(e)) => Some(e), _ => None });
        let mut tblpr = tblpr.unwrap_or_else(|| {
            let mut p = el("w:tblPr");
            if let Some(s) = &t.style { p.push(val("w:tblStyle", s.clone())); }
            p.push(el("w:tblW").with_attr("w:w", "0").with_attr("w:type", "auto"));
            if t.borders && t.style.is_none() {
                let mut b = el("w:tblBorders");
                for side in ["w:top", "w:left", "w:bottom", "w:right", "w:insideH", "w:insideV"] { b.push(val(side, "single").with_attr("w:sz", "4").with_attr("w:space", "0").with_attr("w:color", "auto")); }
                p.push(b);
            }
            if t.fixed { p.push(el("w:tblLayout").with_attr("w:type", "fixed")); }
            if t.cell_margins != DEFAULT_CELL_MARGINS { p.push(margins_element("w:tblCellMar", t.cell_margins)); }
            p.push(val("w:tblLook", "04A0").with_attr("w:firstRow", "1").with_attr("w:lastRow", "0").with_attr("w:firstColumn", "1").with_attr("w:lastColumn", "0").with_attr("w:noHBand", "0").with_attr("w:noVBand", "1"));
            p
        });
        sort_children(&mut tblpr, TBLPR_ORDER);
        e.push(tblpr);
        let mut grid = el("w:tblGrid");
        for g in &t.grid { grid.push(el("w:gridCol").with_attr("w:w", g.to_string())); }
        e.push(grid);
        for row in &t.rows {
            let mut tr = el("w:tr");
            let mut trpr = el("w:trPr");
            if row.grid_before > 0 { trpr.push(val("w:gridBefore", row.grid_before.to_string())); }
            if row.grid_after > 0 { trpr.push(val("w:gridAfter", row.grid_after.to_string())); }
            trpr.children.extend(raw_nodes(&row.extra));
            if row.cant_split { trpr.push(el("w:cantSplit")); }
            if row.min_height > 0 { let mut h = val("w:trHeight", row.min_height.to_string()); if row.exact { h.set_attr("w:hRule", "exact"); } trpr.push(h); }
            if row.header { trpr.push(el("w:tblHeader")); }
            sort_children(&mut trpr, TRPR_ORDER);
            if !trpr.children.is_empty() { tr.push(trpr); }
            for cell in &row.cells {
                let mut tc = el("w:tc");
                let mut tcpr = el("w:tcPr");
                tcpr.push(el("w:tcW").with_attr("w:w", cell.width.to_string()).with_attr("w:type", "dxa"));
                if cell.span > 1 { tcpr.push(val("w:gridSpan", cell.span.to_string())); }
                match cell.v_merge { VMerge::Restart => tcpr.push(val("w:vMerge", "restart")), VMerge::Continue => tcpr.push(el("w:vMerge")), VMerge::None => {} }
                let b = &cell.borders;
                if [b.top, b.left, b.bottom, b.right].iter().any(Option::is_some) {
                    let mut x = el("w:tcBorders");
                    for (n, side) in [("w:top", b.top), ("w:left", b.left), ("w:bottom", b.bottom), ("w:right", b.right)] { if let Some(s) = side { x.push(border_element(n, &s)); } }
                    tcpr.push(x);
                }
                if let Some(m) = cell.margins { tcpr.push(margins_element("w:tcMar", m)); }
                if let Some(c) = cell.shading { tcpr.push(el("w:shd").with_attr("w:val", "clear").with_attr("w:color", "auto").with_attr("w:fill", c.hex())); }
                match cell.v_align { CellVAlign::Center => tcpr.push(val("w:vAlign", "center")), CellVAlign::Bottom => tcpr.push(val("w:vAlign", "bottom")), CellVAlign::Top => {} }
                tcpr.children.extend(raw_nodes(&cell.extra));
                sort_children(&mut tcpr, TCPR_ORDER);
                tc.push(tcpr);
                // The cell's paragraphs and the tables nested in it, in document order; a cell
                // always ends with a paragraph.
                let mut any = false;
                let mut last_is_table = false;
                let mut k = 0;
                while k < paragraphs.len() {
                    let p = paragraphs[k];
                    if p.cell == Some(cell.id) { let x = self.paragraph(p); tc.push(x); any = true; last_is_table = false; k += 1; continue; }
                    if let Some(child) = p.cell.and_then(|c| self.doc.child_table_in_cell(cell.id, c)) {
                        let mut members = vec![];
                        while k < paragraphs.len() && paragraphs[k].cell.is_some_and(|c| self.doc.table_contains_cell(child, c)) { members.push(paragraphs[k]); k += 1; }
                        let x = self.table(&self.doc.tables[child], &members);
                        tc.push(x);
                        last_is_table = true;
                        continue;
                    }
                    k += 1;
                }
                if !any || last_is_table { tc.push(el("w:p")); }
                tr.push(tc);
            }
            e.push(tr);
        }
        e
    }

    /// Rewrite the styles edited in this session (and the document defaults) in styles.xml,
    /// keeping everything else as it was.
    fn patch_styles(&self, root: &mut Element) {
        let doc = self.doc;
        let sheet = &doc.styles;
        let run = |r: &RunProps| self.run_props(r, &None, &[]);
        let para = |p: &ParaProps| { let mut tmp = Paragraph::new(""); tmp.props = p.clone(); self.para_props(&tmp) };
        if doc.style_edits.contains("") {
            let defaults = root.ensure("w:docDefaults");
            let r = defaults.ensure("w:rPrDefault");
            r.children.clear();
            if let Some(x) = run(&sheet.doc_run) { r.push(x); }
            let p = defaults.ensure("w:pPrDefault");
            p.children.clear();
            if let Some(x) = para(&sheet.doc_para) { p.push(x); }
        }
        for id in doc.style_edits.iter().filter(|i| !i.is_empty()) {
            let Some(st) = sheet.styles.get(id) else { continue };
            let pos = root.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "w:style" && e.attr("w:styleId") == Some(id.as_str())));
            let mut e = match pos {
                Some(k) => match root.children.remove(k) { Node::Element(e) => e, _ => continue },
                None => {
                    let kind = match st.kind { crate::styles::StyleKind::Character => "character", crate::styles::StyleKind::Table => "table", _ => "paragraph" };
                    let mut e = el("w:style").with_attr("w:type", kind).with_attr("w:styleId", id.clone()).with_child(val("w:name", st.name.clone()));
                    if let Some(b) = &st.based_on { e.push(val("w:basedOn", b.clone())); }
                    if let Some(n) = &st.next { e.push(val("w:next", n.clone())); }
                    e.push(el("w:qFormat"));
                    e
                }
            };
            e.remove("w:pPr");
            e.remove("w:rPr");
            let at = e.children.iter().position(|n| matches!(n, Node::Element(x) if matches!(x.name.as_str(), "w:tblPr" | "w:trPr" | "w:tcPr" | "w:tblStylePr"))).unwrap_or(e.children.len());
            if let Some(r) = run(&st.run) { e.children.insert(at, Node::Element(r)); }
            if let Some(p) = para(&st.para) { if !p.children.is_empty() { e.children.insert(at, Node::Element(p)); } }
            match pos { Some(k) => root.children.insert(k, Node::Element(e)), None => root.push(e) }
        }
    }

    /// The w:sectPr ending a section inside the body: the page settings and story references
    /// of the document, with this section's own start, columns and line numbers.
    fn paragraph_sect_pr(&self, sec: &crate::page::Section) -> Element {
        let mut s = self.sect_pr();
        write_section(&mut s, sec);
        s
    }

    fn sect_pr(&self) -> Element {
        let doc = self.doc;
        let mut s = doc.package.as_ref().and_then(|p| p.body_sect.clone()).unwrap_or_else(|| el("w:sectPr"));
        let pg = &doc.page;
        let sz = s.ensure("w:pgSz");
        sz.set_attr("w:w", pg.width.to_string());
        sz.set_attr("w:h", pg.height.to_string());
        if pg.landscape { sz.set_attr("w:orient", "landscape"); } else { sz.attrs.retain(|(k, _)| k != "w:orient"); }
        let m = s.ensure("w:pgMar");
        for (k, v) in [("w:top", pg.top), ("w:right", pg.right), ("w:bottom", pg.bottom), ("w:left", pg.left), ("w:header", pg.header), ("w:footer", pg.footer), ("w:gutter", pg.gutter)] { m.set_attr(k, v.to_string()); }
        s.remove("w:pgBorders");
        if let Some(b) = &pg.borders {
            let mut x = el("w:pgBorders").with_attr("w:offsetFrom", if b.from_text { "text" } else { "page" });
            for (k, name) in ["w:top", "w:left", "w:bottom", "w:right"].iter().enumerate() {
                if b.sides[k] { x.push(val(name, b.style.val()).with_attr("w:sz", ((b.width * 8.0).round() as i32).to_string()).with_attr("w:space", (b.space.round() as i32).to_string()).with_attr("w:color", b.color.hex())); }
            }
            s.push(x);
        }
        if pg.title_page { if s.child("w:titlePg").is_none() { s.push(el("w:titlePg")); } } else { s.remove("w:titlePg"); }
        for (tag, (fmt, start), default) in [("w:footnotePr", pg.footnote_format, crate::numbering::NumFmt::Decimal), ("w:endnotePr", pg.endnote_format, crate::numbering::NumFmt::LowerRoman)] {
            s.remove(tag);
            if fmt != default || start != 1 { s.push(el(tag).with_child(val("w:numFmt", fmt.ooxml())).with_child(val("w:numStart", start.to_string()))); }
        }
        // Edited headers and footers: references to their new parts replace the old ones.
        if doc.stories_changed {
            s.children.retain(|n| !matches!(n, Node::Element(e) if e.name == "w:headerReference" || e.name == "w:footerReference"));
            for (tag, rid) in self.story_refs.iter().rev() { s.children.insert(0, Node::Element(el(tag).with_attr("w:type", "default").with_attr("r:id", rid.clone()))); }
        }
        write_section(&mut s, &doc.page.section);
        s
    }

    fn document_xml(&mut self) -> String {
        let doc = self.doc;
        let mut root = el("w:document");
        let mut attrs = doc.package.as_ref().map(|p| p.root_attrs.clone()).unwrap_or_default();
        for (p, u) in xml::NAMESPACES {
            let k = format!("xmlns:{p}");
            if !attrs.iter().any(|(a, _)| *a == k) { attrs.push((k, u.to_string())); }
        }
        // Word 2010 effects (w14) are ignorable for older consumers.
        if !attrs.iter().any(|(k, _)| k == "mc:Ignorable") { attrs.push(("mc:Ignorable".into(), "w14 w15 wp14".into())); }
        root.attrs = attrs;
        if let Some(c) = doc.page.color { root.push(el("w:background").with_attr("w:color", c.hex())); }
        let mut body = el("w:body");
        let mut i = 0;
        while i < doc.paragraphs.len() {
            let p = &doc.paragraphs[i];
            match p.cell.and_then(|c| doc.root_table_of_cell(c)) {
                Some(ti) => {
                    let t = &doc.tables[ti];
                    let mut members = vec![];
                    while i < doc.paragraphs.len() && doc.paragraphs[i].cell.is_some_and(|c| doc.table_contains_cell(ti, c)) { members.push(&doc.paragraphs[i]); i += 1; }
                    let x = self.table(t, &members);
                    body.push(x);
                }
                None => { let x = self.paragraph(p); body.push(x); i += 1; }
            }
        }
        body.push(self.sect_pr());
        root.push(body);
        format!("{}{}", xml::DECL, root.to_xml())
    }

    fn numbering_xml(&self) -> String {
        let n = &self.doc.numbering;
        let mut out = format!("{}<w:numbering", xml::DECL);
        for (p, u) in xml::NAMESPACES { out.push_str(&format!(" xmlns:{p}=\"{u}\"")); }
        out.push('>');
        // Abstract definitions precede instances (§17.9.20).
        for a in &n.abstracts {
            if let Some(raw) = &a.raw { out.push_str(raw); continue; }
            let mut e = el("w:abstractNum").with_attr("w:abstractNumId", a.id.to_string());
            e.push(val("w:multiLevelType", "hybridMultilevel"));
            for (i, l) in a.levels.iter().enumerate() {
                let mut lv = el("w:lvl").with_attr("w:ilvl", i.to_string());
                lv.push(val("w:start", l.start.to_string()));
                lv.push(val("w:numFmt", l.fmt.ooxml()));
                lv.push(val("w:lvlText", l.text.clone()));
                lv.push(val("w:lvlJc", "left"));
                lv.push(el("w:pPr").with_child(el("w:ind").with_attr("w:left", l.indent_left.to_string()).with_attr("w:hanging", l.hanging.to_string())));
                if l.fmt == NumFmt::Bullet { lv.push(el("w:rPr").with_child(el("w:rFonts").with_attr("w:ascii", "Noto Sans").with_attr("w:hAnsi", "Noto Sans").with_attr("w:hint", "default"))); }
                e.push(lv);
            }
            out.push_str(&e.to_xml());
        }
        for num in &n.nums {
            if let Some(raw) = &num.raw { out.push_str(raw); continue; }
            let mut e = el("w:num").with_attr("w:numId", num.id.to_string());
            e.push(val("w:abstractNumId", num.abstract_id.to_string()));
            let mut levels: Vec<_> = num.start_overrides.iter().collect();
            levels.sort();
            for (l, s) in levels { e.push(el("w:lvlOverride").with_attr("w:ilvl", l.to_string()).with_child(val("w:startOverride", s.to_string()))); }
            out.push_str(&e.to_xml());
        }
        out.push_str("</w:numbering>");
        out
    }
}

const RPR_ORDER: &[&str] = &["w:rStyle", "w:rFonts", "w:b", "w:bCs", "w:i", "w:iCs", "w:caps", "w:smallCaps", "w:strike", "w:dstrike", "w:outline", "w:shadow", "w:emboss", "w:imprint", "w:noProof", "w:snapToGrid", "w:vanish", "w:webHidden", "w:color", "w:spacing", "w:w", "w:kern", "w:position", "w:sz", "w:szCs", "w:highlight", "w:u", "w:effect", "w:bdr", "w:shd", "w:fitText", "w:vertAlign", "w:rtl", "w:cs", "w:em", "w:lang", "w:eastAsianLayout", "w:specVanish", "w:oMath", "w14:glow", "w14:shadow", "w14:reflection", "w14:textOutline", "w14:textFill", "w14:scene3d", "w14:props3d", "w14:ligatures", "w14:numForm", "w14:numSpacing", "w14:stylisticSets", "w14:cntxtAlts"];
/// CT_Settings children that follow the hyphenation settings.
const SETTINGS_AFTER_HYPHENATION: &[&str] = &["w:showEnvelope", "w:summaryLength", "w:clickAndTypeStyle", "w:defaultTableStyle", "w:evenAndOddHeaders", "w:bookFoldRevPrinting", "w:bookFoldPrinting", "w:bookFoldPrintingSheets", "w:drawingGridHorizontalSpacing", "w:drawingGridVerticalSpacing", "w:displayHorizontalDrawingGridEvery", "w:displayVerticalDrawingGridEvery", "w:doNotUseMarginsForDrawingGridOrigin", "w:drawingGridHorizontalOrigin", "w:drawingGridVerticalOrigin", "w:doNotShadeFormData", "w:noPunctuationKerning", "w:characterSpacingControl", "w:printTwoOnOne", "w:strictFirstAndLastChars", "w:noLineBreaksAfter", "w:noLineBreaksBefore", "w:savePreviewPicture", "w:doNotValidateAgainstSchema", "w:saveInvalidXml", "w:ignoreMixedContent", "w:alwaysShowPlaceholderText", "w:doNotDemarcateInvalidXml", "w:saveXmlDataOnly", "w:useXSLTWhenSaving", "w:saveThroughXslt", "w:showXMLTags", "w:alwaysMergeEmptyNamespace", "w:updateFields", "w:hdrShapeDefaults", "w:footnotePr", "w:endnotePr", "w:compat", "w:docVars", "w:rsids", "m:mathPr", "w:attachedSchema", "w:themeFontLang", "w:clrSchemeMapping", "w:doNotIncludeSubdocsInStats", "w:doNotAutoCompressPictures", "w:forceUpgrade", "w:captions", "w:readModeInkLockDown", "w:smartTagType", "w:shapeDefaults", "w:doNotEmbedSmartTags", "w:decimalSymbol", "w:listSeparator"];

const PPR_ORDER: &[&str] = &["w:pStyle", "w:keepNext", "w:keepLines", "w:pageBreakBefore", "w:framePr", "w:widowControl", "w:numPr", "w:suppressLineNumbers", "w:pBdr", "w:shd", "w:tabs", "w:suppressAutoHyphens", "w:kinsoku", "w:wordWrap", "w:overflowPunct", "w:topLinePunct", "w:autoSpaceDE", "w:autoSpaceDN", "w:bidi", "w:adjustRightInd", "w:snapToGrid", "w:spacing", "w:ind", "w:contextualSpacing", "w:mirrorIndents", "w:suppressOverlap", "w:jc", "w:textDirection", "w:textAlignment", "w:textboxTightWrap", "w:outlineLvl", "w:divId", "w:cnfStyle", "w:rPr", "w:sectPr", "w:pPrChange"];
const TBLPR_ORDER: &[&str] = &["w:tblStyle", "w:tblpPr", "w:tblOverlap", "w:bidiVisual", "w:tblStyleRowBandSize", "w:tblStyleColBandSize", "w:tblW", "w:jc", "w:tblCellSpacing", "w:tblInd", "w:tblBorders", "w:shd", "w:tblLayout", "w:tblCellMar", "w:tblLook", "w:tblCaption", "w:tblDescription"];
/// CT_Settings children that follow w:trackRevisions (§17.15.1.78).
const SETTINGS_AFTER_TRACK: &[&str] = &["w:doNotTrackMoves", "w:doNotTrackFormatting", "w:documentProtection", "w:autoFormatOverride", "w:styleLockTheme", "w:styleLockQFSet", "w:defaultTabStop", "w:autoHyphenation", "w:consecutiveHyphenLimit", "w:hyphenationZone", "w:doNotHyphenateCaps", "w:showEnvelope", "w:summaryLength", "w:clickAndTypeStyle", "w:defaultTableStyle", "w:evenAndOddHeaders", "w:bookFoldRevPrinting", "w:bookFoldPrinting", "w:bookFoldPrintingSheets", "w:drawingGridHorizontalSpacing", "w:drawingGridVerticalSpacing", "w:displayHorizontalDrawingGridEvery", "w:displayVerticalDrawingGridEvery", "w:doNotUseMarginsForDrawingGridOrigin", "w:drawingGridHorizontalOrigin", "w:drawingGridVerticalOrigin", "w:doNotShadeFormData", "w:noPunctuationKerning", "w:characterSpacingControl", "w:printTwoOnOne", "w:strictFirstAndLastChars", "w:noLineBreaksAfter", "w:noLineBreaksBefore", "w:savePreviewPicture", "w:doNotValidateAgainstSchema", "w:saveInvalidXml", "w:ignoreMixedContent", "w:alwaysShowPlaceholderText", "w:doNotDemarcateInvalidXml", "w:saveXmlDataOnly", "w:useXSLTWhenSaving", "w:saveThroughXslt", "w:showXMLTags", "w:alwaysMergeEmptyNamespace", "w:updateFields", "w:hdrShapeDefaults", "w:footnotePr", "w:endnotePr", "w:compat", "w:docVars", "w:rsids", "m:mathPr", "w:attachedSchema", "w:themeFontLang", "w:clrSchemeMapping", "w:doNotIncludeSubdocsInStats", "w:doNotAutoCompressPictures", "w:forceUpgrade", "w:captions", "w:readModeInkLockDown", "w:smartTagType", "w:schemaLibrary", "w:shapeDefaults", "w:doNotEmbedSmartTags", "w:decimalSymbol", "w:listSeparator"];
const TRPR_ORDER: &[&str] = &["w:cnfStyle", "w:divId", "w:gridBefore", "w:gridAfter", "w:wBefore", "w:wAfter", "w:cantSplit", "w:trHeight", "w:tblHeader", "w:tblCellSpacing", "w:jc", "w:hidden", "w:ins", "w:del", "w:trPrChange"];
const TCPR_ORDER: &[&str] = &["w:cnfStyle", "w:tcW", "w:gridSpan", "w:hMerge", "w:vMerge", "w:tcBorders", "w:shd", "w:noWrap", "w:tcMar", "w:textDirection", "w:tcFitText", "w:vAlign", "w:hideMark"];
const SECTPR_ORDER: &[&str] = &["w:headerReference", "w:footerReference", "w:footnotePr", "w:endnotePr", "w:type", "w:pgSz", "w:pgMar", "w:paperSrc", "w:pgBorders", "w:lnNumType", "w:pgNumType", "w:cols", "w:formProt", "w:vAlign", "w:noEndnote", "w:titlePg", "w:textDirection", "w:bidi", "w:rtlGutter", "w:docGrid", "w:printerSettings", "w:sectPrChange"];

/// Order children by the schema sequence; unknown elements keep their relative order at the end.
fn sort_children(e: &mut Element, order: &[&str]) {
    let rank = |n: &Node| match n { Node::Element(x) => order.iter().position(|o| *o == x.name).unwrap_or(order.len()), Node::Text(_) => order.len() };
    let mut seen = std::collections::HashSet::new();
    // Drop duplicate singleton properties (direct values win over preserved copies, which come later).
    e.children.retain(|n| match n { Node::Element(x) if order.contains(&x.name.as_str()) && !matches!(x.name.as_str(), "w:headerReference" | "w:footerReference") => seen.insert(x.name.clone()), _ => true });
    e.children.sort_by_key(rank);
}

const BASIC_STYLES: &str = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Noto Sans" w:hAnsi="Noto Sans"/><w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="180" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="180" w:after="90"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="160" w:after="80"/><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:b/><w:sz w:val="26"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="240"/></w:pPr><w:rPr><w:sz w:val="56"/></w:rPr></w:style><w:style w:type="character" w:styleId="Emphasis"><w:name w:val="Emphasis"/><w:qFormat/><w:rPr><w:i/></w:rPr></w:style><w:style w:type="character" w:styleId="Strong"><w:name w:val="Strong"/><w:qFormat/><w:rPr><w:b/></w:rPr></w:style><w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style></w:styles>"#;

fn ns_decls() -> String { xml::NAMESPACES.iter().map(|(p, u)| format!(" xmlns:{p}=\"{u}\"")).collect() }

fn comments_xml(comments: &[Comment]) -> String {
    let mut root = el("w:comments");
    for c in comments {
        let mut e = el("w:comment").with_attr("w:id", c.id.clone()).with_attr("w:author", c.author.clone()).with_attr("w:initials", c.initials.clone());
        if !c.date.is_empty() { e.set_attr("w:date", c.date.clone()); }
        let lines: Vec<&str> = c.text.split('\n').collect();
        for (i, line) in lines.iter().enumerate() {
            let mut p = el("w:p");
            if i + 1 == lines.len() { p.set_attr("w14:paraId", c.para_id.clone()); p.set_attr("w14:textId", "77777777"); }
            p.push(el("w:pPr").with_child(val("w:pStyle", "CommentText")));
            if i == 0 { p.push(el("w:r").with_child(el("w:annotationRef"))); }
            let mut t = el("w:t").with_attr("xml:space", "preserve");
            t.children.push(Node::Text(line.to_string()));
            p.push(el("w:r").with_child(t));
            e.push(p);
        }
        root.push(e);
    }
    let x = root.to_xml();
    format!("{}<w:comments{}{}", xml::DECL, ns_decls(), &x["<w:comments".len()..])
}

fn comments_ex_xml(comments: &[Comment]) -> String {
    let mut s = format!("{}<w15:commentsEx{} mc:Ignorable=\"w14 w15\">", xml::DECL, ns_decls());
    for c in comments {
        let mut e = el("w15:commentEx").with_attr("w15:paraId", c.para_id.clone());
        if let Some(parent) = c.parent.as_ref().and_then(|p| comments.iter().find(|x| x.id == *p)) { e.set_attr("w15:paraIdParent", parent.para_id.clone()); }
        e.set_attr("w15:done", if c.done { "1" } else { "0" });
        s.push_str(&e.to_xml());
    }
    s.push_str("</w15:commentsEx>");
    s
}

fn rels_xml(rels: &[Rel]) -> String {
    let mut s = format!("{}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">", xml::DECL);
    for r in rels {
        let mut e = Element::new("Relationship").with_attr("Id", r.id.clone()).with_attr("Type", r.kind.clone()).with_attr("Target", r.target.clone());
        if r.external { e.set_attr("TargetMode", "External"); }
        s.push_str(&e.to_xml());
    }
    s.push_str("</Relationships>");
    s
}

fn ensure_content_types(parts: &mut BTreeMap<String, Vec<u8>>, main: &str) {
    let text = parts.get("[Content_Types].xml").map(|b| String::from_utf8_lossy(b).to_string());
    let mut root = text.and_then(|t| xml::parse(&t).ok()).unwrap_or_else(|| Element::new("Types").with_attr("xmlns", "http://schemas.openxmlformats.org/package/2006/content-types"));
    let has_default = |root: &Element, ext: &str| root.elements().any(|e| e.name == "Default" && e.attr("Extension").is_some_and(|x| x.eq_ignore_ascii_case(ext)));
    let has_override = |root: &Element, part: &str| root.elements().any(|e| e.name == "Override" && e.attr("PartName") == Some(part));
    for (ext, ct) in [("rels", "application/vnd.openxmlformats-package.relationships+xml"), ("xml", "application/xml")] {
        if !has_default(&root, ext) { root.children.insert(0, Node::Element(Element::new("Default").with_attr("Extension", ext).with_attr("ContentType", ct))); }
    }
    let paths: Vec<String> = parts.keys().cloned().collect();
    for p in paths {
        let ext = p.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "png" | "jpeg" | "jpg" | "gif" | "bmp") && !has_default(&root, &ext) {
            root.children.insert(0, Node::Element(Element::new("Default").with_attr("Extension", ext.clone()).with_attr("ContentType", content_type_for(&p))));
        }
    }
    let props: Vec<String> = parts.keys().filter(|p| p.starts_with("customXml/itemProps") && p.ends_with(".xml")).cloned().collect();
    for p in props {
        let name = format!("/{p}");
        if !has_override(&root, &name) { root.push(Element::new("Override").with_attr("PartName", name).with_attr("ContentType", "application/vnd.openxmlformats-officedocument.customXmlProperties+xml")); }
    }
    let stories: Vec<String> = parts.keys().filter(|p| (p.starts_with("word/header") || p.starts_with("word/footer")) && p.ends_with(".xml")).cloned().collect();
    for p in stories {
        let name = format!("/{p}");
        let kind = if p.starts_with("word/header") { "header" } else { "footer" };
        if !has_override(&root, &name) { root.push(Element::new("Override").with_attr("PartName", name).with_attr("ContentType", format!("application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"))); }
    }
    let charts: Vec<String> = parts.keys().filter(|p| p.starts_with("word/charts/") && p.ends_with(".xml") && !p.contains("/_rels/")).cloned().collect();
    for p in charts {
        let name = format!("/{p}");
        if !has_override(&root, &name) { root.push(Element::new("Override").with_attr("PartName", name).with_attr("ContentType", "application/vnd.openxmlformats-officedocument.drawingml.chart+xml")); }
    }
    for (part, ct) in [(format!("/{main}"), DOC_TYPE.to_string()), ("/word/styles.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml".into()), ("/word/numbering.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml".into()), ("/word/comments.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml".into()), ("/word/footnotes.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml".into()), ("/word/endnotes.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml".into()), ("/word/settings.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml".into()), ("/word/commentsExtended.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml".into())] {
        if parts.contains_key(&part[1..]) && !has_override(&root, &part) { root.push(Element::new("Override").with_attr("PartName", part).with_attr("ContentType", ct)); }
    }
    parts.insert("[Content_Types].xml".into(), format!("{}{}", xml::DECL, root.to_xml()).into_bytes());
}

/// Serialize a document to .docx bytes.
pub fn write(doc: &Document) -> Result<Vec<u8>, String> {
    let (mut parts, main, rels) = match &doc.package {
        Some(p) => (p.parts.clone(), p.main.clone(), p.rels.clone()),
        None => {
            let mut parts = BTreeMap::new();
            parts.insert("_rels/.rels".into(), rels_xml(&[Rel { id: "rId1".into(), kind: REL_DOC.into(), target: "word/document.xml".into(), external: false }]).into_bytes());
            parts.insert("word/styles.xml".into(), format!("{}{}", xml::DECL, BASIC_STYLES).into_bytes());
            (parts, "word/document.xml".to_string(), vec![Rel { id: "rId1".into(), kind: REL_STYLES.into(), target: "styles.xml".into(), external: false }])
        }
    };
    let next_rel = rels.iter().filter_map(|r| r.id.strip_prefix("rId").and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0) + 1;
    let next_drawing = 1000 + doc.paragraphs.len() as u32;
    let mut w = Writer { doc, rels, new_parts: BTreeMap::new(), main: main.clone(), next_rel, next_drawing, next_revision: std::cell::Cell::new(9000), bookmark_ids: Default::default(), story_refs: vec![], seq_values: {
        let seq = crate::references::seq_numbers(doc);
        let mut v: Vec<(usize, u32)> = seq.into_iter().collect();
        v.sort();
        v.into_iter().map(|x| x.1).collect()
    }, field_pos: std::cell::Cell::new(0) };
    if doc.stories_changed {
        for (story, tag, rel, file, root) in [(&doc.header, "w:headerReference", REL_HEADER, "header", "w:hdr"), (&doc.footer, "w:footerReference", REL_FOOTER, "footer", "w:ftr")] {
            let Some(story) = story else { continue };
            let path = w.new_part_name(&format!("word/{file}"), "xml");
            let target = path.trim_start_matches("word/").to_string();
            // Pictures in the story resolve against the story part's own relationships.
            let doc_rels = std::mem::take(&mut w.rels);
            let doc_next = w.next_rel;
            w.next_rel = 1;
            let body: String = story.paragraphs.iter().map(|p| w.paragraph(p).to_xml()).collect();
            let body = if body.is_empty() { "<w:p/>".to_string() } else { body };
            let part_rels = std::mem::replace(&mut w.rels, doc_rels);
            w.next_rel = doc_next;
            parts.insert(path.clone(), format!("{}<{root}{}>{body}</{root}>", xml::DECL, ns_decls()).into_bytes());
            if !part_rels.is_empty() { parts.insert(rels_path(&path), rels_xml(&part_rels).into_bytes()); }
            let new_parts = std::mem::take(&mut w.new_parts);
            parts.extend(new_parts);
            let rid = w.rel_id(rel, &target, false);
            w.story_refs.push((tag.to_string(), rid));
        }
    }
    let document = w.document_xml();
    parts.insert(main.clone(), document.into_bytes());
    // Macros: their own customXml part, rewritten each save (emptied when all are deleted).
    if !doc.macros.is_empty() || doc.macros_part.is_some() {
        let path = match &doc.macros_part {
            Some(p) => p.clone(),
            None => {
                let mut n = 1;
                while parts.contains_key(&format!("customXml/item{n}.xml")) { n += 1; }
                parts.insert(format!("customXml/itemProps{n}.xml"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?><ds:datastoreItem ds:itemID="{{6E1C5A10-7A4B-4C5D-9E2F-{:012X}}}" xmlns:ds="http://schemas.openxmlformats.org/officeDocument/2006/customXml"><ds:schemaRefs><ds:schemaRef ds:uri="{}"/></ds:schemaRefs></ds:datastoreItem>"#, n, crate::macros::NAMESPACE).into_bytes());
                parts.insert(format!("customXml/_rels/item{n}.xml.rels"), rels_xml(&[Rel { id: "rId1".into(), kind: REL_CUSTOM_XML_PROPS.into(), target: format!("itemProps{n}.xml"), external: false }]).into_bytes());
                w.rel_id(REL_CUSTOM_XML, &format!("../customXml/item{n}.xml"), false);
                format!("customXml/item{n}.xml")
            }
        };
        parts.insert(path, crate::macros::macros_xml(&doc.macros).into_bytes());
    }
    if doc.sources_changed || (!doc.sources.is_empty() && doc.sources_part.is_none()) {
        let path = match &doc.sources_part {
            Some(p) => p.clone(),
            None => {
                let mut n = 1;
                while parts.contains_key(&format!("customXml/item{n}.xml")) { n += 1; }
                let path = format!("customXml/item{n}.xml");
                let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
                let guid = format!("{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}", (nanos >> 32) as u32, (nanos >> 16) as u16, nanos as u16 | 0x4000, 0x8000 | (n as u16), nanos as u64 & 0xFFFF_FFFF_FFFF);
                parts.insert(format!("customXml/itemProps{n}.xml"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?><ds:datastoreItem ds:itemID="{guid}" xmlns:ds="http://schemas.openxmlformats.org/officeDocument/2006/customXml"><ds:schemaRefs><ds:schemaRef ds:uri="http://schemas.openxmlformats.org/officeDocument/2006/bibliography"/></ds:schemaRefs></ds:datastoreItem>"#).into_bytes());
                parts.insert(format!("customXml/_rels/item{n}.xml.rels"), rels_xml(&[Rel { id: "rId1".into(), kind: REL_CUSTOM_XML_PROPS.into(), target: format!("itemProps{n}.xml"), external: false }]).into_bytes());
                w.rel_id(REL_CUSTOM_XML, &format!("../customXml/item{n}.xml"), false);
                path
            }
        };
        parts.insert(path, crate::references::sources_xml(doc).into_bytes());
    }
    if doc.styles_changed {
        let path = w.rels.iter().find(|r| r.kind == REL_STYLES).map(|r| resolve_target(&main, &r.target)).unwrap_or_else(|| { w.rel_id(REL_STYLES, "styles.xml", false); resolve_target(&main, "styles.xml") });
        let text = part_text(&parts, &path).unwrap_or_else(|| BASIC_STYLES.to_string());
        let mut root = xml::parse(&text).unwrap_or_else(|_| Element::new("w:styles"));
        w.patch_styles(&mut root);
        let x = root.to_xml();
        let body = if root.attrs.iter().any(|(k, _)| k == "xmlns:w") { x } else { format!("<w:styles{}{}", ns_decls(), &x["<w:styles".len()..]) };
        parts.insert(path, format!("{}{}", xml::DECL, body).into_bytes());
    }
    let uses_numbering = doc.paragraphs.iter().any(|p| p.props.numbering.is_some_and(|n| n.0 != 0));
    let numbering_rel = w.rels.iter().find(|r| r.kind == REL_NUMBERING).map(|r| resolve_target(&main, &r.target));
    if doc.numbering.changed || (uses_numbering && numbering_rel.is_none()) {
        let path = numbering_rel.unwrap_or_else(|| { w.rel_id(REL_NUMBERING, "numbering.xml", false); resolve_target(&main, "numbering.xml") });
        parts.insert(path, w.numbering_xml().into_bytes());
    }
    if doc.comments_changed || (!doc.comments.is_empty() && !w.rels.iter().any(|r| r.kind == REL_COMMENTS)) {
        let path = w.rels.iter().find(|r| r.kind == REL_COMMENTS).map(|r| resolve_target(&main, &r.target)).unwrap_or_else(|| { w.rel_id(REL_COMMENTS, "comments.xml", false); resolve_target(&main, "comments.xml") });
        parts.insert(path, comments_xml(&doc.comments).into_bytes());
        let path = w.rels.iter().find(|r| r.kind == REL_COMMENTS_EX).map(|r| resolve_target(&main, &r.target)).unwrap_or_else(|| { w.rel_id(REL_COMMENTS_EX, "commentsExtended.xml", false); resolve_target(&main, "commentsExtended.xml") });
        parts.insert(path, comments_ex_xml(&doc.comments).into_bytes());
    }
    let has_notes = |kind: NoteKind| doc.notes.iter().any(|n| n.kind == kind);
    for (kind, rel_kind, file, tag) in [(NoteKind::Foot, REL_FOOTNOTES, "footnotes.xml", "footnote"), (NoteKind::End, REL_ENDNOTES, "endnotes.xml", "endnote")] {
        let existing = w.rels.iter().find(|r| r.kind == rel_kind).map(|r| resolve_target(&main, &r.target));
        if !(doc.notes_changed && (existing.is_some() || has_notes(kind))) && !(existing.is_none() && has_notes(kind)) { continue; }
        let path = existing.unwrap_or_else(|| { w.rel_id(rel_kind, file, false); resolve_target(&main, file) });
        // Hyperlinks in notes resolve against the notes part's own relationships.
        let part_rels = read_rels(&parts, &path);
        let doc_rels = std::mem::replace(&mut w.rels, part_rels);
        let doc_next = w.next_rel;
        w.next_rel = w.rels.iter().filter_map(|r| r.id.strip_prefix("rId").and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0) + 1;
        let mut body = String::new();
        let specials: Vec<&String> = doc.note_specials.iter().filter(|(k, _)| *k == kind).map(|(_, x)| x).collect();
        if specials.is_empty() {
            body.push_str(&format!(r#"<w:{tag} w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:separator/></w:r></w:p></w:{tag}><w:{tag} w:type="continuationSeparator" w:id="0"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:{tag}>"#));
        } else { for x in specials { body.push_str(x); } }
        let referenced: std::collections::HashSet<String> = doc.note_refs().into_iter().filter(|r| r.0 == kind).map(|r| r.1).collect();
        for n in doc.notes.iter().filter(|n| n.kind == kind && referenced.contains(&n.id)) {
            let mut e = el(&format!("w:{tag}")).with_attr("w:id", n.id.clone());
            for (i, p) in n.paragraphs.iter().enumerate() {
                let mut x = w.paragraph(p);
                if i == 0 {
                    // The note's own number, then a space.
                    let at = x.children.iter().position(|c| !matches!(c, Node::Element(e) if e.name == "w:pPr")).unwrap_or(x.children.len());
                    let reference = el("w:r").with_child(el("w:rPr").with_child(val("w:vertAlign", "superscript"))).with_child(el(&format!("w:{tag}Ref")));
                    let mut space = el("w:t").with_attr("xml:space", "preserve");
                    space.children.push(Node::Text(" ".into()));
                    x.children.insert(at, Node::Element(el("w:r").with_child(space)));
                    x.children.insert(at, Node::Element(reference));
                }
                e.push(x);
            }
            body.push_str(&e.to_xml());
        }
        parts.insert(path.clone(), format!("{}<w:{tag}s{}>{body}</w:{tag}s>", xml::DECL, ns_decls()).into_bytes());
        let part_rels = std::mem::replace(&mut w.rels, doc_rels);
        w.next_rel = doc_next;
        if !part_rels.is_empty() { parts.insert(rels_path(&path), rels_xml(&part_rels).into_bytes()); }
    }
    let wants_bg = doc.page.color.is_some();
    let has_bg = part_text(&parts, "word/settings.xml").is_some_and(|t| t.contains("displayBackgroundShape"));
    let hy = doc.page.hyphenation;
    let had_hyphenation = part_text(&parts, "word/settings.xml").is_some_and(|t| t.contains("autoHyphenation") || t.contains("hyphenationZone"));
    let wants_hyphenation = hy != crate::page::Hyphenation::default();
    let had_protection = part_text(&parts, "word/settings.xml").is_some_and(|t| t.contains("documentProtection"));
    if doc.track_changes_changed || wants_bg != has_bg || wants_hyphenation || had_hyphenation || had_protection || doc.protection.is_some() {
        let path = w.rels.iter().find(|r| r.kind.ends_with("/settings")).map(|r| resolve_target(&main, &r.target)).unwrap_or_else(|| { w.rel_id(REL_SETTINGS, "settings.xml", false); resolve_target(&main, "settings.xml") });
        let mut root = part_text(&parts, &path).and_then(|t| xml::parse(&t).ok()).unwrap_or_else(|| Element::new("w:settings"));
        root.remove("w:displayBackgroundShape");
        // Protection comes early in CT_Settings (after view/zoom/removePersonalInformation…).
        root.remove("w:documentProtection");
        if let Some(p) = &doc.protection {
            if let Ok(e) = xml::parse(&format!("<x{}>{}</x>", ns_decls(), p.xml())) {
                if let Some(Node::Element(pe)) = e.children.into_iter().find(|n| matches!(n, Node::Element(_))) {
                    let at = root.children.iter().position(|n| matches!(n, Node::Element(e) if !matches!(e.name.as_str(), "w:writeProtection" | "w:view" | "w:zoom" | "w:removePersonalInformation" | "w:removeDateAndTime" | "w:doNotDisplayPageBoundaries" | "w:displayBackgroundShape" | "w:printPostScriptOverText" | "w:printFractionalCharacterWidth" | "w:printFormsData" | "w:embedTrueTypeFonts" | "w:embedSystemFonts" | "w:saveSubsetFonts" | "w:saveFormsData" | "w:mirrorMargins" | "w:alignBordersAndEdges" | "w:bordersDoNotSurroundHeader" | "w:bordersDoNotSurroundFooter" | "w:gutterAtTop" | "w:hideSpellingErrors" | "w:hideGrammaticalErrors" | "w:activeWritingStyle" | "w:proofState" | "w:formsDesign" | "w:attachedTemplate" | "w:linkStyles" | "w:stylePaneFormatFilter" | "w:stylePaneSortMethod" | "w:documentType" | "w:mailMerge" | "w:revisionView" | "w:trackRevisions" | "w:doNotTrackMoves" | "w:doNotTrackFormatting"))).unwrap_or(root.children.len());
                    root.children.insert(at, Node::Element(pe));
                }
            }
        }
        if wants_bg {
            let at = root.children.iter().position(|n| matches!(n, Node::Element(e) if !matches!(e.name.as_str(), "w:writeProtection" | "w:view" | "w:zoom" | "w:removePersonalInformation" | "w:removeDateAndTime" | "w:doNotDisplayPageBoundaries"))).unwrap_or(root.children.len());
            root.children.insert(at, Node::Element(el("w:displayBackgroundShape")));
        }
        // Hyphenation settings, in their schema position (before showEnvelope and the rest).
        for tag in ["w:autoHyphenation", "w:consecutiveHyphenLimit", "w:hyphenationZone", "w:doNotHyphenateCaps"] { root.remove(tag); }
        let mut hyph = vec![];
        if hy.auto { hyph.push(el("w:autoHyphenation")); }
        if hy.limit > 0 { hyph.push(val("w:consecutiveHyphenLimit", hy.limit.to_string())); }
        if hy.zone != 360 { hyph.push(val("w:hyphenationZone", hy.zone.to_string())); }
        if !hy.caps { hyph.push(el("w:doNotHyphenateCaps")); }
        let at = root.children.iter().position(|n| matches!(n, Node::Element(e) if SETTINGS_AFTER_HYPHENATION.contains(&e.name.as_str()))).unwrap_or(root.children.len());
        for (k, h) in hyph.into_iter().enumerate() { root.children.insert(at + k, Node::Element(h)); }
        root.remove("w:trackRevisions");
        if doc.track_changes {
            let at = root.children.iter().position(|n| matches!(n, Node::Element(e) if SETTINGS_AFTER_TRACK.contains(&e.name.as_str()))).unwrap_or(root.children.len());
            root.children.insert(at, Node::Element(el("w:trackRevisions")));
        }
        let x = root.to_xml();
        let body = if root.attrs.iter().any(|(k, _)| k == "xmlns:w") { x } else { format!("<w:settings{}{}", ns_decls(), &x["<w:settings".len()..]) };
        parts.insert(path, format!("{}{}", xml::DECL, body).into_bytes());
    }
    let new_parts = std::mem::take(&mut w.new_parts);
    parts.extend(new_parts);
    parts.insert(rels_path(&main), rels_xml(&w.rels).into_bytes());
    ensure_content_types(&mut parts, &main);

    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        // [Content_Types].xml first, as producers conventionally write it.
        let mut names: Vec<&String> = parts.keys().collect();
        names.sort_by_key(|n| (n.as_str() != "[Content_Types].xml", n.as_str() != "_rels/.rels", n.to_string()));
        for name in names {
            zip.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
            zip.write_all(&parts[name]).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::numbering::ListKind;

    #[test]
    fn round_trip_new_document() {
        let mut d = Document::new(vec![Paragraph::new("Hello world"), Paragraph::new("Second")]);
        d.select(0, 5);
        d.toggle_bold();
        d.select(12, 12);
        d.toggle_list(ListKind::Number);
        d.select(0, 0);
        d.insert_table(2, 2);
        d.replace("cell");
        let bytes = write(&d).unwrap();
        let e = read(&bytes).unwrap();
        assert_eq!(e.text(), d.text());
        assert!(e.run_format(&e.paragraphs[0], &e.paragraphs[0].runs[0]).bold);
        assert_eq!(e.tables.len(), 1);
        assert!(e.para_format(e.paragraphs.last().unwrap()).numbering.is_some());
        // Saving again keeps every part.
        let again = read(&write(&e).unwrap()).unwrap();
        assert_eq!(again.text(), d.text());
    }

    #[test]
    fn revisions_round_trip() {
        let mut d = Document::new(vec![Paragraph::new("keep old text")]);
        d.revision_author = "Ada".into();
        d.set_track_changes(true);
        d.select(5, 8);
        d.replace("new");
        let end = d.len();
        d.select(end, end);
        d.insert_paragraph();
        d.replace("added");
        let bytes = write(&d).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let mut xml = String::new();
        zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
        assert!(xml.contains("<w:delText>old</w:delText>") && xml.contains("w:author=\"Ada\""));
        let e = read(&bytes).unwrap();
        assert!(e.track_changes, "trackRevisions setting round-trips");
        assert_eq!(e.text(), d.text());
        let kinds = |d: &Document| d.revisions().into_iter().map(|r| (r.0, r.1, r.2)).collect::<Vec<_>>();
        assert_eq!(kinds(&e), kinds(&d));
        let mut e = e;
        let n = e.len();
        e.resolve_revisions(true, 0, n, true);
        assert_eq!(e.text(), "keep new text\nadded");
    }

    #[test]
    fn notes_round_trip() {
        let mut d = Document::new(vec![Paragraph::new("Claim one. Claim two.")]);
        d.select(10, 10);
        let f = d.insert_note(NoteKind::Foot);
        d.set_note_text(NoteKind::Foot, &f, "Source, page 4\nSecond paragraph");
        let end = d.len();
        d.select(end, end);
        let e = d.insert_note(NoteKind::End);
        d.set_note_text(NoteKind::End, &e, "Endnote text");
        let r = read(&write(&d).unwrap()).unwrap();
        assert_eq!(r.text(), d.text());
        assert_eq!(r.note_text(NoteKind::Foot, &f), "Source, page 4\nSecond paragraph");
        assert_eq!(r.note_text(NoteKind::End, &e), "Endnote text");
        assert_eq!(r.note_specials.len(), 4, "separators are created");
        // Saving the reopened document again keeps the notes.
        let again = read(&write(&r).unwrap()).unwrap();
        assert_eq!(again.note_text(NoteKind::Foot, &f), "Source, page 4\nSecond paragraph");
    }

    #[test]
    fn direction_and_complex_script_round_trip() {
        let mut d = Document::new(vec![Paragraph::new("مرحبا"), Paragraph::new("hello")]);
        d.select(0, 0);
        d.set_paragraph_direction(true);
        d.select(0, 5);
        d.set_run_props(RunProps { cs_font: Some("Traditional Arabic".into()), size_cs: Some(16.0), bold_cs: Some(true), rtl: Some(true), ..Default::default() });
        let e = read(&write(&d).unwrap()).unwrap();
        assert_eq!(e.para_format(&e.paragraphs[0]).rtl, Some(true));
        assert_eq!(e.para_format(&e.paragraphs[1]).rtl, None);
        let f = e.run_format(&e.paragraphs[0], &e.paragraphs[0].runs[0]);
        assert_eq!((f.cs_font.as_deref(), f.size_cs, f.bold_cs, f.rtl), (Some("Traditional Arabic"), 16.0, true, true));
        assert_eq!(e.text(), "مرحبا\nhello");
    }

    #[test]
    fn text_effects_round_trip() {
        let mut d = Document::new(vec![Paragraph::new("Glowing text")]);
        d.select(0, 7);
        d.set_run_props(RunProps { glow: Some(Some(Glow { color: Color(255, 192, 0), radius: 8.0 })), reflection: Some(true), gradient: Some(Some((Color(0x44, 0x72, 0xC4), Color(0xC0, 0, 0)))), outline: Some(true), ..Default::default() });
        let e = read(&write(&d).unwrap()).unwrap();
        let f = e.run_format(&e.paragraphs[0], &e.paragraphs[0].runs[0]);
        assert_eq!(f.glow, Some(Glow { color: Color(255, 192, 0), radius: 8.0 }));
        assert!(f.reflection && f.outline);
        assert_eq!(f.gradient, Some((Color(0x44, 0x72, 0xC4), Color(0xC0, 0, 0))));
    }

    #[test]
    fn bookmarks_and_reference_fields_round_trip() {
        let mut d = Document::new(vec![Paragraph::new("Results"), Paragraph::new("As shown in ")]);
        d.select(0, 7);
        d.add_bookmark("Results").unwrap();
        let end = d.len();
        d.select(end, end);
        d.insert_cross_reference("Results", false);
        d.replace(" on page ");
        d.insert_cross_reference("Results", true);
        let bytes = write(&d).unwrap();
        let e = read(&bytes).unwrap();
        assert_eq!(e.bookmarks(), d.bookmarks());
        let fields: Vec<Field> = e.paragraphs[1].runs.iter().filter_map(|r| r.field.clone()).collect();
        assert_eq!(fields, vec![Field::Ref("Results".into()), Field::PageRef("Results".into())]);
        let l = crate::layout_document(&e);
        let text: String = l.pages[0].items.iter().filter_map(|i| match i { crate::layout::DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| g.ch).collect::<String>()), _ => None }).collect();
        assert!(text.contains("Results") && text.contains("1"), "fields show the bookmark's text and page: {text}");
    }

    #[test]
    fn floating_pictures_rotation_and_original_round_trip() {
        use crate::model::{Anchor, AnchorAlign, Wrap};
        let png = {
            let img = image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255]));
            let mut out = std::io::Cursor::new(vec![]);
            image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        let mut d = Document::new(vec![Paragraph::new("Text beside a picture.")]);
        d.select(0, 0);
        d.insert_image(png.clone(), "image/png", 40.0, 20.0);
        d.insert_object(crate::objects::Object::Shape(crate::objects::Shape::new(crate::objects::ShapeKind::Ellipse)));
        let shape_pos = d.range().0;
        assert!(d.apply_picture_op(0, crate::picture::PictureOp::Grayscale));
        d.set_image_rotation(0, 30.0);
        d.set_anchor(0, Some(Anchor::new(Wrap::Square, 24.0, 12.0)));
        let mut behind = Anchor::new(Wrap::Behind, 0.0, 0.0);
        behind.x_align = Some(AnchorAlign::Center);
        d.set_anchor(shape_pos, Some(behind));
        let bytes = write(&d).unwrap();
        let e = read(&bytes).unwrap();
        let a = e.anchor_at(0).expect("picture floats");
        assert_eq!((a.wrap, a.x.round(), a.y.round()), (Wrap::Square, 24.0, 12.0));
        let b = e.anchor_at(shape_pos).expect("shape floats");
        assert_eq!((b.wrap, b.x_align), (Wrap::Behind, Some(AnchorAlign::Center)));
        let img = e.image_at(0).unwrap();
        assert_eq!(img.rotation, 30.0);
        let original = img.original.clone().expect("the unedited picture is kept");
        assert_eq!(e.assets[&original].bytes, png);
        // Saving again (from the raw XML) keeps all of it.
        let f = read(&write(&e).unwrap()).unwrap();
        assert_eq!(f.anchor_at(0).map(|a| a.wrap), Some(Wrap::Square));
        assert!(f.image_at(0).unwrap().original.is_some());
    }

    fn read_part(docx: &[u8], name: &str) -> Vec<u8> {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
        let mut out = vec![];
        z.by_name(name).unwrap().read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn picture_edits_save_as_drawingml_over_the_original() {
        use crate::picture::PictureOp;
        let png = {
            let img = image::RgbaImage::from_fn(8, 4, |x, _| image::Rgba([x as u8 * 30, 90, 160, 255]));
            let mut out = std::io::Cursor::new(vec![]);
            image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        let mut d = Document::new(vec![Paragraph::new("")]);
        d.insert_image(png.clone(), "image/png", 80.0, 40.0);
        let ops = [PictureOp::Corrections { brightness: 20, contrast: -10 }, PictureOp::Grayscale, PictureOp::Transparency(40), PictureOp::Crop { left: 0.25, top: 0.0, right: 0.0, bottom: 0.0 }, PictureOp::Rounded, PictureOp::Frame(Color(0, 0, 0))];
        for op in ops { assert!(d.apply_picture_op(0, op)); }
        d.flip_image(0, true);
        let bytes = write(&d).unwrap();
        let xml = String::from_utf8(read_part(&bytes, "word/document.xml")).unwrap();
        for tag in [r#"<a:lum bright="20000" contrast="-10000"/>"#, "<a:grayscl/>", r#"<a:alphaModFix amt="60000"/>"#, r#"<a:srcRect l="25000""#, r#"prst="roundRect""#, "<a:ln ", r#"flipH="1""#] {
            assert!(xml.contains(tag), "{tag} in {xml}");
        }
        assert!(!xml.contains("wo:original"), "the blip is the original picture itself");
        let e = read(&bytes).unwrap();
        let img = e.image_at(0).unwrap();
        assert_eq!(e.assets[img.original.as_ref().unwrap()].bytes, png, "the original pixels are embedded");
        assert!(img.flip_h);
        assert_eq!(img.effects.len(), 6, "{:?}", img.effects);
        // An effect Word can't express keeps the baked pixels plus the original for Reset Picture.
        let mut d = Document::new(vec![Paragraph::new("")]);
        d.insert_image(png.clone(), "image/png", 80.0, 40.0);
        d.apply_picture_op(0, PictureOp::Posterize);
        let xml = String::from_utf8(read_part(&write(&d).unwrap(), "word/document.xml")).unwrap();
        assert!(xml.contains("wo:original") && !xml.contains("<a:lum"));
    }

    #[test]
    fn objects_round_trip() {
        use crate::objects::*;
        let mut d = Document::new(vec![Paragraph::new("Figure: ")]);
        d.select(8, 8);
        let mut shape = Shape::new(ShapeKind::Star5);
        shape.rotation = 30.0; shape.flip_h = true;
        shape.text = "Hi".into();
        d.insert_object(Object::Shape(shape));
        d.select(9, 9);
        d.insert_object(Object::Chart(Chart::sample(ChartKind::Bar)));
        d.select(10, 10);
        d.insert_object(Object::Diagram(Diagram::sample(DiagramKind::Hierarchy)));
        let bytes = write(&d).unwrap();
        let e = read(&bytes).unwrap();
        let objs: Vec<&Object> = e.paragraphs[0].runs.iter().filter_map(|r| r.object.as_ref()).collect();
        assert_eq!(objs.len(), 3);
        match objs[0] { Object::Shape(s) => { assert_eq!(s.kind, ShapeKind::Star5); assert_eq!(s.text, "Hi"); assert_eq!((s.rotation, s.flip_h, s.flip_v), (30.0, true, false)); } _ => panic!("shape") }
        match objs[1] { Object::Chart(c) => { assert_eq!(c.kind, ChartKind::Bar); assert_eq!(c.series.len(), 3); assert_eq!(c.categories.len(), 4); assert_eq!(c.title, "Chart Title"); } _ => panic!("chart") }
        match objs[2] { Object::Diagram(g) => assert_eq!(g.items.len(), 7), _ => panic!("diagram") }
        // Saving the reopened document keeps the original chart part.
        assert!(read(&write(&e).unwrap()).is_ok());
        crate::layout_document(&e);
    }

    #[test]
    fn comments_round_trip() {
        let mut d = Document::new(vec![Paragraph::new("Alpha beta gamma")]);
        d.select(6, 10);
        let id = d.add_comment("Ada Lovelace", "Check this\nsecond line", "2026-03-04T05:06:07Z");
        d.reply_to_comment(&id, "Grace Hopper", "Done & dusted", "2026-03-05T00:00:00Z");
        d.set_comment_done(&id, true);
        let e = read(&write(&d).unwrap()).unwrap();
        assert_eq!(e.comment_anchors()[0].1..e.comment_anchors()[0].2, 6..10);
        assert_eq!(e.comments.len(), 2);
        assert_eq!(e.comments[0].text, "Check this\nsecond line");
        assert_eq!(e.comments[1].parent.as_deref(), Some(id.as_str()));
        assert!(e.comments.iter().all(|c| c.done));
        assert_eq!(e.comments[0].author, "Ada Lovelace");
        // An unchanged reopened document keeps its original comments part.
        let bytes = write(&e).unwrap();
        assert_eq!(read(&bytes).unwrap().comments.len(), 2);
    }

    #[test]
    fn reads_repository_fixtures() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../documents/public/fixtures");
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let mut ok = 0;
        let mut failed = vec![];
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "docx") { continue; }
            let bytes = std::fs::read(&path).unwrap();
            match read(&bytes) {
                Ok(doc) => {
                    let layout = crate::layout::layout_document(&doc);
                    assert!(layout.page_count() >= 1);
                    let saved = read(&write(&doc).unwrap()).unwrap();
                    assert_eq!(saved.text(), doc.text(), "{} round-trips its text", path.display());
                    ok += 1;
                }
                Err(err) => failed.push(format!("{}: {err}", path.display())),
            }
        }
        assert!(failed.is_empty(), "{failed:#?}");
        eprintln!("{ok} fixtures opened, laid out and round-tripped");
    }
}
