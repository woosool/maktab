//! The drawing layer (xlsx-export-drawings.js, pictures.js): each sheet's floating objects —
//! charts, pictures, shapes and text boxes, slicers — anchored to cells (twoCellAnchor),
//! to a cell with a size (oneCellAnchor) or to the sheet (absoluteAnchor).
//!
//! Reading moves a sheet's drawing part into `Sheet.objects`: every anchor becomes an object
//! that keeps its original XML (`raw`) and the relationships it uses, so writing reproduces
//! what the model doesn't understand and only rewrites the anchors. Charts stay in their chart
//! parts while unchanged; edited and new charts are generated from the model. Writing builds a
//! fresh drawing part per sheet, adds parts, relationships and content types for new objects,
//! and drops chart, image and drawing parts nothing refers to any more.

use crate::book::Workbook;
use crate::chart::Chart;
use crate::xlsx::xml::{attr, part_text, text, Element, Node};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// EMUs per pixel (96 dpi).
pub const EMU: f32 = 9525.0;

/// A point in the grid: a cell and an offset into it, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Mark { pub col: u32, pub dx: f32, pub row: u32, pub dy: f32 }

#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    /// Moves and sizes with cells; `edit_as` is the file's editAs (oneCell, absolute).
    Two { from: Mark, to: Mark, edit_as: Option<String> },
    One { from: Mark, w: f32, h: f32 },
    Abs { x: f32, y: f32, w: f32, h: f32 },
}

impl Default for Anchor { fn default() -> Self { Anchor::Two { from: Mark::default(), to: Mark { col: 5, row: 10, ..Default::default() }, edit_as: None } } }

impl Anchor {
    /// The rectangle in sheet pixels (x0, y0, x1, y1), given where columns and rows start.
    pub fn rect(&self, col_pos: impl Fn(u32) -> f32, row_pos: impl Fn(u32) -> f32) -> [f32; 4] {
        let pt = |m: &Mark| (col_pos(m.col) + m.dx, row_pos(m.row) + m.dy);
        match self {
            Anchor::Two { from, to, .. } => { let (a, b) = (pt(from), pt(to)); [a.0, a.1, b.0.max(a.0), b.1.max(a.1)] }
            Anchor::One { from, w, h } => { let a = pt(from); [a.0, a.1, a.0 + w, a.1 + h] }
            Anchor::Abs { x, y, w, h } => [*x, *y, x + w, y + h],
        }
    }
    /// Places the anchor at a rectangle in sheet pixels, keeping its kind.
    pub fn set_rect(&mut self, r: [f32; 4], col_at: impl Fn(f32) -> u32, col_pos: impl Fn(u32) -> f32, row_at: impl Fn(f32) -> u32, row_pos: impl Fn(u32) -> f32) {
        let (x0, y0) = (r[0].max(0.0), r[1].max(0.0));
        let (w, h) = ((r[2] - r[0]).max(1.0), (r[3] - r[1]).max(1.0));
        let mark = |x: f32, y: f32| { let c = col_at(x); let rw = row_at(y); Mark { col: c, dx: (x - col_pos(c)).max(0.0), row: rw, dy: (y - row_pos(rw)).max(0.0) } };
        *self = match self {
            Anchor::Two { edit_as, .. } => Anchor::Two { from: mark(x0, y0), to: mark(x0 + w, y0 + h), edit_as: edit_as.clone() },
            Anchor::One { .. } => Anchor::One { from: mark(x0, y0), w, h },
            Anchor::Abs { .. } => Anchor::Abs { x: x0, y: y0, w, h },
        };
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shape {
    pub text: String,
    pub preset: String,
    pub text_box: bool,
    pub fill: Option<[u8; 3]>,
    pub line: Option<[u8; 3]>,
    pub color: Option<[u8; 3]>,
    /// Font size in points.
    pub size: f32,
    pub bold: bool,
    pub center: bool,
    /// Preset adjust values (guide name, value), as in `<a:avLst>`.
    pub adjust: Vec<(String, f64)>,
    /// WordArt text effects: an outline color, a shadow, a glow color.
    pub text_outline: Option<[u8; 3]>,
    pub shadow: bool,
    pub glow: Option<[u8; 3]>,
}

/// A slicer: buttons for the items of one field of a PivotTable, which filter it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Slicer {
    /// The PivotTable it filters (by name), or empty for a table's.
    pub pivot: String,
    /// The table it filters (by name) when `pivot` is empty.
    pub table: String,
    /// The field: a pivot source column, or a table column, by offset.
    pub field: usize,
    pub caption: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObjKind {
    Chart(Box<Chart>),
    Picture { data: Arc<Vec<u8>>, ext: String },
    Shape(Shape),
    Slicer(Slicer),
    /// A timeline: a date filter for a PivotTable or table (timeline.rs).
    Timeline(crate::timeline::Timeline),
    /// A group of objects (group.rs).
    Group(crate::group::Group),
    /// Connectors, ink, Office-only content: carried as they were.
    Other,
}

/// A relationship an object uses: id, full type, target (an absolute part name unless external).
#[derive(Clone, Debug, PartialEq)]
pub struct Rel { pub id: String, pub kind: String, pub target: String, pub external: bool }

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub id: u32,
    pub name: String,
    pub descr: String,
    pub anchor: Anchor,
    pub kind: ObjKind,
    /// The content element as read (xdr:sp, xdr:pic, xdr:graphicFrame...), written back while
    /// set; editing what it shows clears it.
    pub raw: Option<Arc<Element>>,
    /// The drawing part's namespace declarations the raw XML relies on.
    pub ns: Arc<Vec<(String, String)>>,
    pub rels: Vec<Rel>,
    /// Clockwise rotation in degrees and flips (a:xfrm), and hidden (the Selection Pane's eye).
    pub rot: f32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub hidden: bool,
}

impl Object {
    pub fn new(kind: ObjKind, anchor: Anchor) -> Object {
        Object { id: 0, name: String::new(), descr: String::new(), anchor, kind, raw: None, ns: Arc::new(vec![]), rels: vec![], rot: 0.0, flip_h: false, flip_v: false, hidden: false }
    }
    pub fn is_chart(&self) -> bool { matches!(self.kind, ObjKind::Chart(_)) }
    pub fn chart(&self) -> Option<&Chart> { if let ObjKind::Chart(c) = &self.kind { Some(c) } else { None } }
    pub fn chart_mut(&mut self) -> Option<&mut Chart> { if let ObjKind::Chart(c) = &mut self.kind { c.dirty = true; Some(c) } else { None } }
    /// What it shows changed: regenerate its XML on writing.
    pub fn touch(&mut self) { if !matches!(self.kind, ObjKind::Chart(_) | ObjKind::Other | ObjKind::Group(_)) { self.raw = None; } }
}

/// The next free object id and a name such as "Chart 3" for a sheet.
pub fn add_object(objects: &mut Vec<Object>, mut o: Object, base: &str) -> usize {
    let id = objects.iter().map(|x| x.id).max().unwrap_or(1) + 1;
    o.id = id;
    if o.name.is_empty() { o.name = format!("{base} {}", id - 1); }
    objects.push(o);
    objects.len() - 1
}

// ---- paths ---------------------------------------------------------------------------------------

fn rels_name(part: &str) -> String {
    match part.rfind('/') { Some(i) => format!("{}/_rels/{}.rels", &part[..i], &part[i + 1..]), None => format!("_rels/{part}.rels") }
}
fn resolve(source: &str, target: &str) -> String {
    let target = target.replace('\\', "/");
    if let Some(abs) = target.strip_prefix('/') { return abs.to_string(); }
    let mut segs: Vec<&str> = source.split('/').collect();
    segs.pop();
    for s in target.split('/') { match s { ".." => { segs.pop(); } "." | "" => {} s => segs.push(s) } }
    segs.join("/")
}
fn relative(from: &str, to: &str) -> String {
    let a: Vec<&str> = from.split('/').collect();
    let b: Vec<&str> = to.split('/').collect();
    let dir = &a[..a.len() - 1];
    let common = dir.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let mut out: Vec<String> = std::iter::repeat_n("..".to_string(), dir.len() - common).collect();
    out.extend(b[common..].iter().map(|s| s.to_string()));
    out.join("/")
}

fn read_rels(wb: &Workbook, part: &str) -> Vec<Rel> {
    let Some(e) = wb.parts.get(&rels_name(part)).and_then(|b| Element::parse(&part_text(b)).ok()) else { return vec![] };
    e.all("Relationship").filter_map(|r| {
        let external = r.attr("TargetMode") == Some("External");
        let t = r.attr("Target")?;
        Some(Rel { id: r.attr("Id")?.into(), kind: r.attr("Type")?.into(), target: if external { t.into() } else { resolve(part, t) }, external })
    }).collect()
}

const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const NS_XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const CT_DRAWING: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";
const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const SLICER_TAG: &str = "jadwal:slicer";

// ---- reading -------------------------------------------------------------------------------------

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }

fn emu(e: Option<&Element>) -> f32 { e.map(|x| x.text().trim().parse::<f64>().unwrap_or(0.0) as f32 / EMU).unwrap_or(0.0) }
fn mark(e: &Element) -> Mark {
    let n = |l: &str| e.child(l).and_then(|x| x.text().trim().parse::<u32>().ok()).unwrap_or(0);
    Mark { col: n("col"), dx: emu(e.child("colOff")), row: n("row"), dy: emu(e.child("rowOff")) }
}
fn ext(e: Option<&Element>) -> (f32, f32) { e.map(|x| (x.attr_f64("cx").unwrap_or(0.0) as f32 / EMU, x.attr_f64("cy").unwrap_or(0.0) as f32 / EMU)).unwrap_or((0.0, 0.0)) }

fn rel_attr(k: &str) -> bool { k.contains(':') && !k.starts_with("xmlns") && matches!(crate::xlsx::xml::local(k), "embed" | "id" | "link" | "pict" | "dm" | "lo" | "qs" | "cs") }

pub(crate) fn used_ids(e: &Element, ids: &HashSet<&str>, out: &mut Vec<String>) {
    for (k, v) in &e.attrs { if rel_attr(k) && ids.contains(v.as_str()) && !out.contains(v) { out.push(v.clone()); } }
    for c in e.elements() { used_ids(c, ids, out); }
}

fn hex(s: &str) -> Option<[u8; 3]> {
    if s.len() != 6 { return None; }
    let p = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([p(0)?, p(2)?, p(4)?])
}
fn solid(e: Option<&Element>, theme: &[[u8; 3]]) -> Option<[u8; 3]> {
    let f = e?.child("solidFill")?;
    if let Some(v) = f.child("srgbClr").and_then(|c| c.attr("val")) { return hex(v); }
    let k = match f.child("schemeClr")?.attr("val")? { "accent1" => 4, "accent2" => 5, "accent3" => 6, "accent4" => 7, "accent5" => 8, "accent6" => 9, "tx1" | "dk1" => 0, "bg1" | "lt1" => 1, "tx2" | "dk2" => 2, "bg2" | "lt2" => 3, _ => return None };
    theme.get(k).copied()
}

pub(crate) fn parse_shape(sp: &Element, theme: &[[u8; 3]]) -> Shape {
    let pr = sp.child("spPr");
    let mut s = Shape { preset: pr.and_then(|p| p.child("prstGeom")).and_then(|g| g.attr("prst")).unwrap_or("rect").into(), size: 11.0, ..Default::default() };
    s.text_box = sp.path("nvSpPr/cNvSpPr").is_some_and(|c| c.flag("txBox", false));
    if let Some(av) = pr.and_then(|p| p.child("prstGeom")).and_then(|g| g.child("avLst")) {
        s.adjust = av.elements().filter_map(|g| Some((g.attr("name")?.to_string(), g.attr("fmla")?.strip_prefix("val ")?.trim().parse().ok()?))).collect();
    }
    s.fill = solid(pr, theme);
    s.line = solid(pr.and_then(|p| p.child("ln")), theme);
    if pr.and_then(|p| p.child("ln")).is_some_and(|l| l.child("noFill").is_some()) { s.line = None; }
    // Shapes styled by the theme (no explicit fill) take accent 1 with a darker outline.
    if s.fill.is_none() && !s.text_box && pr.is_some_and(|p| p.child("noFill").is_none()) && sp.child("style").is_some() { s.fill = theme.get(4).copied(); }
    if let Some(tb) = sp.child("txBody") {
        let paras: Vec<String> = tb.all("p").map(|p| { let mut t = String::new(); for r in p.elements() { if r.local() == "r" || r.local() == "fld" { t.push_str(&r.child("t").map(|x| x.text()).unwrap_or_default()); } else if r.local() == "br" { t.push('\n'); } } t }).collect();
        s.text = paras.join("\n");
        if let Some(rp) = find(tb, "rPr") {
            if let Some(sz) = rp.attr_f64("sz") { s.size = (sz / 100.0) as f32; }
            s.bold = rp.flag("b", false);
            s.color = solid(Some(rp), theme);
            s.text_outline = rp.child("ln").and_then(|l| solid(Some(l), theme));
            if let Some(fx) = rp.child("effectLst") { s.shadow = fx.child("outerShdw").is_some(); s.glow = fx.child("glow").and_then(|g| solid(Some(g), theme).or_else(|| g.child("srgbClr").and_then(|c| c.attr("val")).and_then(hex))); }
        }
        s.center = find(tb, "pPr").and_then(|p| p.attr("algn")) == Some("ctr");
    }
    s
}

/// Moves every sheet's drawing into `Sheet.objects` (after reading a file).
pub fn load(wb: &mut Workbook) {
    for si in 0..wb.sheets.len() {
        let Some(di) = wb.sheets[si].extra.iter().position(|(k, _)| k == "drawing") else { continue };
        let Some(rid) = Element::parse(&wb.sheets[si].extra[di].1).ok().and_then(|e| e.attrs.iter().find(|(k, _)| k.ends_with(":id")).map(|(_, v)| v.clone())) else { continue };
        let Some(ri) = wb.sheets[si].extra.iter().position(|(k, v)| k == "rel" && v.split('\t').next() == Some(rid.as_str())) else { continue };
        let f: Vec<String> = wb.sheets[si].extra[ri].1.split('\t').map(String::from).collect();
        if f.len() < 4 || f[3] == "1" { continue; }
        let path = f[2].clone();
        let Some(root) = wb.parts.get(&path).and_then(|b| Element::parse(&part_text(b)).ok()) else { continue };
        // Anchors wrapped in markup compatibility (chartex charts...) stay carried as they are.
        if root.local() != "wsDr" || root.elements().any(|e| !matches!(e.local(), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") && !(e.local() == "AlternateContent" && crate::xlsx::native_objects::top_level_anchor(wb, e).is_some())) { continue; }
        let rels = read_rels(wb, &path);
        let ids: HashSet<&str> = rels.iter().map(|r| r.id.as_str()).collect();
        let ns = Arc::new(root.attrs.iter().filter(|(k, _)| k.starts_with("xmlns")).cloned().collect::<Vec<_>>());
        let mut objects = vec![];
        let mut media: HashMap<String, Arc<Vec<u8>>> = HashMap::new();
        for a in root.elements() {
            let a = if a.local() == "AlternateContent" { match crate::xlsx::native_objects::top_level_anchor(wb, a) { Some(x) => x, None => continue } } else { a };
            let anchor = match a.local() {
                "twoCellAnchor" => Anchor::Two { from: a.child("from").map(mark).unwrap_or_default(), to: a.child("to").map(mark).unwrap_or_default(), edit_as: a.attr("editAs").map(String::from) },
                "oneCellAnchor" => { let (w, h) = ext(a.child("ext")); Anchor::One { from: a.child("from").map(mark).unwrap_or_default(), w, h } }
                "absoluteAnchor" => { let p = a.child("pos"); let (w, h) = ext(a.child("ext")); Anchor::Abs { x: p.and_then(|p| p.attr_f64("x")).unwrap_or(0.0) as f32 / EMU, y: p.and_then(|p| p.attr_f64("y")).unwrap_or(0.0) as f32 / EMU, w, h } }
                _ => continue,
            };
            let Some(content) = a.elements().find(|c| !matches!(c.local(), "from" | "to" | "ext" | "pos" | "clientData")) else { continue };
            let mut used = vec![];
            used_ids(content, &ids, &mut used);
            let orels: Vec<Rel> = rels.iter().filter(|r| used.contains(&r.id)).cloned().collect();
            let rel_target = |id: &str| orels.iter().find(|r| r.id == id && !r.external).map(|r| r.target.clone());
            let cnv = find(content, "cNvPr");
            let kind = match content.local() {
                "graphicFrame" => {
                    let c = find(content, "chart").filter(|c| c.name.contains("chart") || c.attrs.iter().any(|(_, v)| v == NS_C));
                    let part = c.and_then(|c| c.attrs.iter().find(|(k, _)| rel_attr(k)).map(|(_, v)| v.clone())).and_then(|id| rel_target(&id));
                    match part.as_ref().and_then(|p| wb.parts.get(p)).and_then(|b| crate::chart::parse(&part_text(b), &wb.theme_colors)) {
                        Some(mut ch) => { ch.part = part; ObjKind::Chart(Box::new(ch)) }
                        None => ObjKind::Other,
                    }
                }
                "pic" => {
                    let target = find(content, "blip").and_then(|b| b.attrs.iter().find(|(k, _)| k.ends_with(":embed")).map(|(_, v)| v.clone())).and_then(|id| rel_target(&id));
                    match target.and_then(|t| { let d = media.entry(t.clone()).or_insert_with(|| Arc::new(wb.parts.get(&t).cloned().unwrap_or_default())).clone(); (!d.is_empty()).then_some((t, d)) }) {
                        Some((t, data)) => ObjKind::Picture { data, ext: t.rsplit('.').next().unwrap_or("png").to_ascii_lowercase() },
                        None => ObjKind::Other,
                    }
                }
                "sp" => {
                    let descr = cnv.and_then(|c| c.attr("descr")).unwrap_or("");
                    if let Some(t) = crate::timeline::from_tag(descr) { ObjKind::Timeline(t) } else {
                    match descr.strip_prefix(SLICER_TAG) {
                        Some(rest) => { let f: Vec<&str> = rest.trim_start_matches('\t').split('\t').collect(); ObjKind::Slicer(Slicer { pivot: f.first().copied().unwrap_or("").into(), field: f.get(1).and_then(|x| x.parse().ok()).unwrap_or(0), caption: f.get(2).copied().unwrap_or("").into(), table: f.get(3).copied().unwrap_or("").into() }) }
                        None => ObjKind::Shape(parse_shape(content, &wb.theme_colors)),
                    } }
                }
                "grpSp" => ObjKind::Group(crate::group::read(wb, content, &orels, &ns)),
                "AlternateContent" => crate::xlsx::slicer_parts::kind(wb, si, content).or_else(|| crate::chart::ex::kind_of_frame(wb, content, &rel_target)).unwrap_or(ObjKind::Other),
                _ => ObjKind::Other,
            };
            objects.push(Object {
                id: cnv.and_then(|c| c.attr_u32("id")).unwrap_or(0),
                name: cnv.and_then(|c| c.attr("name")).unwrap_or("").into(),
                descr: cnv.and_then(|c| c.attr("descr")).unwrap_or("").into(),
                anchor, kind, raw: Some(Arc::new(content.clone())), ns: ns.clone(), rels: orels,
                rot: 0.0, flip_h: false, flip_v: false, hidden: false,
            });
            if let Some(o) = objects.last_mut() {
                (o.rot, o.flip_h, o.flip_v, o.hidden) = crate::group::read_transform(content);
                crate::xlsx::native_objects::read_object(wb, si, content, o);
                if matches!(o.kind, ObjKind::Group(_)) { o.raw = None; }
            }
        }
        let s = &mut wb.sheets[si];
        s.extra.remove(di.max(ri));
        s.extra.remove(di.min(ri));
        s.objects = objects;
        wb.parts.remove(&rels_name(&path));
        wb.parts.remove(&path);
    }
}

// ---- writing -------------------------------------------------------------------------------------

fn emu_s(px: f32) -> i64 { (px as f64 * EMU as f64).round() as i64 }
fn mark_xml(tag: &str, m: &Mark) -> String {
    format!("<xdr:{tag}><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:{tag}>", m.col, emu_s(m.dx), m.row, emu_s(m.dy))
}
fn hexs(c: [u8; 3]) -> String { format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }

pub(crate) fn remap(e: &mut Element, map: &HashMap<String, String>) {
    for (k, v) in e.attrs.iter_mut() { if rel_attr(k) { if let Some(n) = map.get(v.as_str()) { *v = n.clone(); } } }
    for c in e.children.iter_mut() { if let Node::Element(x) = c { remap(x, map); } }
}

pub(crate) fn shape_xml(o: &Object, s: &Shape, descr: &str, wh: (f32, f32)) -> String {
    let fill = s.fill.map(|c| format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hexs(c))).unwrap_or_else(|| "<a:noFill/>".into());
    let line = s.line.map(|c| format!("<a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>", hexs(c))).unwrap_or_else(|| "<a:ln><a:noFill/></a:ln>".into());
    let mut color = s.color.map(|c| format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hexs(c))).unwrap_or_default();
    if let Some(c) = s.text_outline { color = format!("<a:ln w=\"12700\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>{color}", hexs(c)); }
    if s.shadow || s.glow.is_some() {
        color.push_str("<a:effectLst>");
        if let Some(c) = s.glow { color.push_str(&format!("<a:glow rad=\"63500\"><a:srgbClr val=\"{}\"><a:alpha val=\"60000\"/></a:srgbClr></a:glow>", hexs(c))); }
        if s.shadow { color.push_str("<a:outerShdw blurRad=\"38100\" dist=\"38100\" dir=\"2700000\" algn=\"tl\"><a:srgbClr val=\"000000\"><a:alpha val=\"43137\"/></a:srgbClr></a:outerShdw>"); }
        color.push_str("</a:effectLst>");
    }
    let av: String = s.adjust.iter().map(|(n, v)| format!("<a:gd name=\"{}\" fmla=\"val {}\"/>", attr(n), v.round() as i64)).collect();
    let paras: String = s.text.split('\n').map(|l| format!("<a:p>{}<a:r><a:rPr lang=\"en-US\" sz=\"{}\" b=\"{}\">{color}</a:rPr><a:t>{}</a:t></a:r></a:p>", if s.center { "<a:pPr algn=\"ctr\"/>" } else { "" }, (s.size * 100.0).round() as i64, s.bold as u8, text(l))).collect();
    format!("<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{}\" name=\"{}\"{}/><xdr:cNvSpPr{}/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"{}\"><a:avLst>{av}</a:avLst></a:prstGeom>{fill}{line}</xdr:spPr><xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\" wrap=\"square\" rtlCol=\"0\" anchor=\"t\"/><a:lstStyle/>{paras}</xdr:txBody></xdr:sp>",
        o.id, attr(&o.name), if descr.is_empty() { String::new() } else { format!(" descr=\"{}\"", attr(descr)) }, if s.text_box { " txBox=\"1\"" } else { "" }, emu_s(wh.0), emu_s(wh.1), attr(if s.preset.is_empty() { "rect" } else { &s.preset }))
}

/// A slicer's items and whether each is selected (shown).
pub fn slicer_items(wb: &Workbook, sl: &Slicer) -> Vec<(String, bool)> {
    if sl.pivot.is_empty() {
        let Some((si, ti)) = wb.find_table(&sl.table) else { return vec![] };
        let scope = crate::tables::Scope::Table(ti);
        let (mut vals, blanks) = wb.filter_values(si, scope, sl.field as u32);
        if blanks { vals.push("(blank)".into()); }
        let crit = wb.filter_of(si, scope).and_then(|f| f.get(sl.field as u32));
        return vals.into_iter().map(|v| {
            let on = match crit { Some(crate::tables::Criteria::Values { values, blanks }) => if v == "(blank)" { *blanks } else { values.iter().any(|x| x.eq_ignore_ascii_case(&v)) }, _ => true };
            (v, on)
        }).collect();
    }
    let Some(p) = wb.sheets.iter().flat_map(|s| s.pivots.iter()).find(|p| p.name == sl.pivot) else { return vec![] };
    let h = p.hidden.get(&sl.field);
    p.items(wb, sl.field).into_iter().map(|i| { let on = !h.is_some_and(|h| h.contains(&i)); (i, on) }).collect()
}

/// What a slicer saves as: a text box listing its items, tagged so reading restores the slicer.
pub fn slicer_shape(wb: &Workbook, sl: &Slicer) -> Shape {
    let pivot = wb.sheets.iter().flat_map(|s| s.pivots.iter()).find(|p| p.name == sl.pivot);
    let items = slicer_items(wb, sl).into_iter().map(|(i, on)| format!("{} {i}", if on { "☑" } else { "☐" })).collect::<Vec<_>>();
    let _ = pivot;
    Shape { text: std::iter::once(sl.caption.clone()).chain(items).collect::<Vec<_>>().join("\n"), preset: "rect".into(), text_box: true, fill: Some([255, 255, 255]), line: Some([0xBF, 0xBF, 0xBF]), size: 10.0, ..Default::default() }
}

/// A copy of the workbook ready for the package writer: pivots and slicers as native parts and
/// each sheet's objects as drawing, chart and media parts. None when nothing needs doing.
pub fn prepare(wb: &Workbook) -> Option<Workbook> {
    if wb.sheets.iter().all(|s| s.objects.is_empty() && s.pivots.is_empty()) && !crate::xlsx::stale_pivots(wb) { return None; }
    let mut w = wb.clone();
    crate::xlsx::store_pivots(&mut w);
    let mut overrides: Vec<(String, String)> = vec![];
    let mut defaults: Vec<(String, String)> = vec![];
    let mut taken: HashSet<String> = w.parts.keys().cloned().collect();
    let mut free = |prefix: &str, ext: &str| -> String { let n = (1..).map(|i| format!("{prefix}{i}{ext}")).find(|n| !taken.contains(n)).unwrap(); taken.insert(n.clone()); n };
    let mut written_charts: HashSet<String> = HashSet::new();
    let mut media_paths: HashMap<usize, String> = HashMap::new();
    let mut pending_media: Vec<(String, Arc<Vec<u8>>)> = vec![];
    for si in 0..w.sheets.len() {
        // A sheet whose drawing wasn't read into objects keeps that drawing as it was.
        if w.sheets[si].objects.is_empty() || w.sheets[si].extra.iter().any(|(k, _)| k == "drawing") { continue; }
        let dpath = free("xl/drawings/drawing", ".xml");
        let mut rels: Vec<(String, String, String, bool)> = vec![];
        let add_rel = |rels: &mut Vec<(String, String, String, bool)>, kind: &str, target: String, external: bool| -> String {
            let id = format!("rId{}", rels.len() + 1);
            rels.push((id.clone(), kind.to_string(), target, external));
            id
        };
        // The root declares what the carried XML uses plus what generated XML needs.
        let mut ns: BTreeMap<String, String> = BTreeMap::new();
        for o in &w.sheets[si].objects { if o.raw.is_some() { for (k, v) in o.ns.iter() { ns.entry(k.clone()).or_insert(v.clone()); } } }
        for (k, v) in [("xmlns:xdr", NS_XDR), ("xmlns:a", NS_A), ("xmlns:r", NS_R)] { ns.entry(k.into()).or_insert(v.into()); }
        let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr");
        for (k, v) in &ns { x.push_str(&format!(" {k}=\"{}\"", attr(v))); }
        x.push('>');
        let objects = w.sheets[si].objects.clone();
        for (oi, o) in objects.iter().enumerate() {
            let wh = match &o.anchor { Anchor::One { w, h, .. } | Anchor::Abs { w, h, .. } => (*w, *h), Anchor::Two { .. } => (0.0, 0.0) };
            match &o.anchor {
                Anchor::Two { from, to, edit_as } => { x.push_str(&format!("<xdr:twoCellAnchor{}>", edit_as.as_ref().map(|e| format!(" editAs=\"{}\"", attr(e))).unwrap_or_default())); x.push_str(&mark_xml("from", from)); x.push_str(&mark_xml("to", to)); }
                Anchor::One { from, w, h } => { x.push_str("<xdr:oneCellAnchor>"); x.push_str(&mark_xml("from", from)); x.push_str(&format!("<xdr:ext cx=\"{}\" cy=\"{}\"/>", emu_s(*w), emu_s(*h))); }
                Anchor::Abs { x: ax, y, w, h } => { x.push_str(&format!("<xdr:absoluteAnchor><xdr:pos x=\"{}\" y=\"{}\"/><xdr:ext cx=\"{}\" cy=\"{}\"/>", emu_s(*ax), emu_s(*y), emu_s(*w), emu_s(*h))); }
            }
            let mut map: HashMap<String, String> = HashMap::new();
            let native = {
                let mut add = |kind: &str, target: String, ext: bool| add_rel(&mut rels, kind, target, ext);
                let mut fr = |p: &str, e: &str| free(p, e);
                crate::xlsx::native_objects::write_object(&mut w, si, oi, o, &mut crate::xlsx::native_objects::Sink { add_rel: &mut add, free: &mut fr, overrides: &mut overrides, defaults: &mut defaults })
            };
            let content = if let Some(x) = native { x } else { match &o.kind {
                ObjKind::Chart(c) if c.kind.is_ex() => {
                    // Chartex charts are written from the model each time (chart/ex.rs).
                    let path = free("xl/charts/chartEx", ".xml");
                    let x = crate::chart::ex::to_xml(c, &w, si);
                    let x = crate::xlsx::chartex_keep::carry(&mut w, c.part.as_deref(), &path, x);
                    w.parts.insert(path.clone(), x.into_bytes());
                    overrides.push((path.clone(), crate::chart::ex::CT.into()));
                    let id = add_rel(&mut rels, crate::chart::ex::RT, path, false);
                    crate::chart::ex::frame_xml(o.id, &o.name, &id, c.kind)
                }
                ObjKind::Chart(c) => {
                    // The part: the original while unchanged (copied when another object took it).
                    let orig = c.part.clone().filter(|p| w.parts.contains_key(p));
                    let path = match &orig { Some(p) if !written_charts.contains(p) => p.clone(), _ => free("xl/charts/chart", ".xml") };
                    if c.dirty || orig.is_none() { w.parts.insert(path.clone(), c.to_xml(&w, si).into_bytes()); }
                    else if orig.as_deref() != Some(path.as_str()) {
                        let src = orig.clone().unwrap();
                        let b = w.parts[&src].clone();
                        w.parts.insert(path.clone(), b);
                        if let Some(r) = w.parts.get(&rels_name(&src)).cloned() { w.parts.insert(rels_name(&path), r); }
                    }
                    if orig.as_deref() != Some(path.as_str()) || c.dirty { overrides.push((path.clone(), CT_CHART.into())); }
                    written_charts.insert(path.clone());
                    let id = add_rel(&mut rels, &format!("{RT}chart"), path, false);
                    match (&o.raw, o.rels.iter().find(|r| r.kind.ends_with("/chart"))) {
                        (Some(raw), Some(old)) => { map.insert(old.id.clone(), id); let mut e = (**raw).clone(); remap(&mut e, &map); e.to_xml() }
                        _ => format!("<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{}\" name=\"{}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{NS_C}\"><c:chart xmlns:c=\"{NS_C}\" r:id=\"{id}\"/></a:graphicData></a:graphic></xdr:graphicFrame>", o.id, attr(&o.name)),
                    }
                }
                ObjKind::Picture { data, ext } => {
                    let old = o.rels.iter().find(|r| r.kind.ends_with("/image") && !r.external && w.parts.get(&r.target).is_some_and(|b| b.len() == data.len()));
                    let target = match old {
                        Some(r) => r.target.clone(),
                        None => {
                            let key = Arc::as_ptr(data) as usize;
                            media_paths.entry(key).or_insert_with(|| {
                                let p = free("xl/media/image", &format!(".{ext}"));
                                w.parts.insert(p.clone(), (**data).clone());
                                let mime = match ext.as_str() { "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", _ => "image/png" };
                                defaults.push((ext.clone(), mime.into()));
                                p
                            }).clone()
                        }
                    };
                    let id = add_rel(&mut rels, &format!("{RT}image"), target, false);
                    match (&o.raw, o.rels.iter().find(|r| r.kind.ends_with("/image"))) {
                        (Some(raw), Some(old)) => {
                            map.insert(old.id.clone(), id);
                            for r in o.rels.iter().filter(|r| r.id != old.id) { if r.external || w.parts.contains_key(&r.target) { let n = add_rel(&mut rels, &r.kind, r.target.clone(), r.external); map.insert(r.id.clone(), n); } }
                            let mut e = (**raw).clone(); remap(&mut e, &map); e.to_xml()
                        }
                        _ => format!("<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{}\" name=\"{}\"{}/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{id}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>",
                            o.id, attr(&o.name), if o.descr.is_empty() { String::new() } else { format!(" descr=\"{}\"", attr(&o.descr)) }, emu_s(wh.0), emu_s(wh.1)),
                    }
                }
                ObjKind::Slicer(sl) => crate::xlsx::slicer_parts::frame_xml(&w, si, oi, o, sl).unwrap_or_else(|| shape_xml(o, &slicer_shape(&w, sl), &format!("{SLICER_TAG}\t{}\t{}\t{}\t{}", sl.pivot, sl.field, sl.caption, sl.table), wh)),
                ObjKind::Timeline(t) => shape_xml(o, &crate::timeline::fallback_shape(&w, t), &crate::timeline::tag(t), wh),
                ObjKind::Shape(s) if o.raw.is_none() => shape_xml(o, s, &o.descr, wh),
                ObjKind::Group(g) => {
                    let mut add = |kind: &str, target: String, ext: bool| add_rel(&mut rels, kind, target, ext);
                    let mut media = |w: &Workbook, data: &Arc<Vec<u8>>, ext: &str| -> String {
                        if let Some((p, _)) = w.parts.iter().find(|(p, b)| p.starts_with("xl/media/") && b.len() == data.len() && b.as_slice() == data.as_slice()) { return p.clone(); }
                        let p = free("xl/media/image", &format!(".{ext}"));
                        let mime = match ext { "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", _ => "image/png" };
                        defaults.push((ext.to_string(), mime.into()));
                        pending_media.push((p.clone(), data.clone()));
                        p
                    };
                    let x = crate::group::xml(&w, si, o, g, &mut crate::group::Sink { add_rel: &mut add, new_media: &mut media });
                    for (p, d) in pending_media.drain(..) { w.parts.insert(p, (*d).clone()); }
                    x
                }
                _ => {
                    let Some(raw) = &o.raw else { continue };
                    for r in &o.rels { if r.external || w.parts.contains_key(&r.target) { let n = add_rel(&mut rels, &r.kind, r.target.clone(), r.external); map.insert(r.id.clone(), n); } }
                    let mut e = (**raw).clone(); remap(&mut e, &map); e.to_xml()
                }
            } };
            let content = crate::group::patch_top(content, o);
            x.push_str(&content);
            x.push_str("<xdr:clientData/>");
            x.push_str(match o.anchor { Anchor::Two { .. } => "</xdr:twoCellAnchor>", Anchor::One { .. } => "</xdr:oneCellAnchor>", Anchor::Abs { .. } => "</xdr:absoluteAnchor>" });
        }
        x.push_str("</xdr:wsDr>");
        w.parts.insert(dpath.clone(), x.into_bytes());
        if !rels.is_empty() {
            let mut r = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
            for (id, kind, target, ext) in &rels { r.push_str(&format!("<Relationship Id=\"{id}\" Type=\"{}\" Target=\"{}\"{}/>", attr(kind), attr(&if *ext { target.clone() } else { relative(&dpath, target) }), if *ext { " TargetMode=\"External\"" } else { "" })); }
            r.push_str("</Relationships>");
            w.parts.insert(rels_name(&dpath), r.into_bytes());
        }
        overrides.push((dpath.clone(), CT_DRAWING.into()));
        let s = &mut w.sheets[si];
        let used: HashSet<String> = s.extra.iter().filter(|(k, _)| k == "rel").filter_map(|(_, v)| v.split('\t').next().map(String::from)).collect();
        let id = (1..).map(|i| format!("rIdDr{i}")).find(|i| !used.contains(i)).unwrap();
        s.extra.retain(|(k, _)| k != "drawing");
        s.extra.push(("drawing".into(), format!("<drawing r:id=\"{id}\"/>")));
        s.extra.push(("rel".into(), format!("{id}\t{RT}drawing\t{dpath}\t0")));
    }
    sweep(&mut w);
    // Content types for the new parts.
    let mut ct = w.parts.get("[Content_Types].xml").and_then(|b| Element::parse(&part_text(b)).ok()).unwrap_or_else(|| Element { name: "Types".into(), attrs: vec![("xmlns".into(), "http://schemas.openxmlformats.org/package/2006/content-types".into())], children: vec![] });
    let has_default: HashSet<String> = ct.all("Default").filter_map(|e| e.attr("Extension").map(|x| x.to_ascii_lowercase())).collect();
    let mut seen = HashSet::new();
    for (x, t) in defaults { if !has_default.contains(&x) && seen.insert(x.clone()) { ct.children.insert(0, Node::Element(Element { name: "Default".into(), attrs: vec![("Extension".into(), x), ("ContentType".into(), t)], children: vec![] })); } }
    for (p, t) in overrides {
        let pn = format!("/{p}");
        ct.children.retain(|n| !matches!(n, Node::Element(e) if e.local() == "Override" && e.attr("PartName") == Some(pn.as_str())));
        ct.children.push(Node::Element(Element { name: "Override".into(), attrs: vec![("PartName".into(), pn), ("ContentType".into(), t)], children: vec![] }));
    }
    w.parts.insert("[Content_Types].xml".into(), ct.to_xml().into_bytes());
    Some(w)
}

/// Removes drawing, chart and media parts no relationship points at (deleted objects).
fn sweep(w: &mut Workbook) {
    for _ in 0..4 {
        let mut refd: HashSet<String> = HashSet::new();
        for (k, v) in &w.parts {
            let Some(i) = k.rfind("/_rels/").map(|i| i as isize).or(if k.starts_with("_rels/") { Some(-1) } else { None }) else { continue };
            let owner = if i < 0 { k.trim_start_matches("_rels/").trim_end_matches(".rels").to_string() } else { let i = i as usize; format!("{}/{}", &k[..i], k[i + 7..].trim_end_matches(".rels")) };
            if let Ok(e) = Element::parse(&part_text(v)) {
                for r in e.all("Relationship") { if r.attr("TargetMode") != Some("External") { if let Some(t) = r.attr("Target") { refd.insert(resolve(&owner, t)); } } }
            }
        }
        for s in &w.sheets { for (k, v) in &s.extra { if k == "rel" { if let Some(t) = v.split('\t').nth(2) { refd.insert(t.to_string()); } } } }
        let gone: Vec<String> = w.parts.keys().filter(|k| !k.contains("/_rels/") && (k.starts_with("xl/charts/") || k.starts_with("xl/media/") || k.starts_with("xl/diagrams/") || (k.starts_with("xl/drawings/") && k.ends_with(".xml"))) && !refd.contains(*k)).cloned().collect();
        if gone.is_empty() { break; }
        for g in gone { w.parts.remove(&rels_name(&g)); w.parts.remove(&g); }
    }
}

/// What reading does after the package parts are in: drawings into objects, pivots onto sheets.
pub fn after_read(wb: &mut Workbook) {
    crate::xlsx::load_pivots(wb);
    crate::pivot::load(wb);
    load(wb);
    crate::xlsx::slicer_parts::after_load(wb);
    crate::xlsx::timeline_parts::after_load(wb);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::{parse_range, Pos};
    use crate::chart::ChartKind;

    // A 1×1 PNG.
    const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x2D, 0x7A, 0x9C, 0x0F, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82];

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        for (i, row) in [["", "A", "B"], ["x", "1", "2"], ["y", "3", "4"]].iter().enumerate() { for (j, t) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, j as u32), t); } }
        wb.recalc();
        wb
    }

    #[test]
    fn objects_round_trip() {
        let mut wb = book();
        let chart = Chart::from_range(&wb, 0, parse_range("A1:C3").unwrap(), ChartKind::Line, None);
        let anchor = Anchor::Two { from: Mark { col: 4, dx: 3.0, row: 1, dy: 0.0 }, to: Mark { col: 10, dx: 0.0, row: 15, dy: 5.0 }, edit_as: None };
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Chart(Box::new(chart)), anchor.clone()), "Chart");
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Picture { data: Arc::new(PNG.to_vec()), ext: "png".into() }, Anchor::One { from: Mark { col: 1, row: 20, ..Default::default() }, w: 50.0, h: 40.0 }), "Picture");
        let mut tb = Object::new(ObjKind::Shape(Shape { text: "Hello\nthere".into(), text_box: true, size: 12.0, ..Default::default() }), Anchor::Abs { x: 10.0, y: 400.0, w: 100.0, h: 30.0 });
        tb.name = "TextBox 9".into();
        add_object(&mut wb.sheets[0].objects, tb, "TextBox");
        let bytes = crate::xlsx::write(&wb).unwrap();
        let back = crate::xlsx::read(&bytes).unwrap();
        let objs = &back.sheets[0].objects;
        assert_eq!(objs.len(), 3);
        let c = objs[0].chart().unwrap();
        assert_eq!(c.kind, ChartKind::Line);
        assert_eq!(c.series.len(), 2);
        assert_eq!(objs[0].anchor, anchor);
        assert!(matches!(&objs[1].kind, ObjKind::Picture { data, .. } if data.as_slice() == PNG));
        assert!(matches!(&objs[2].kind, ObjKind::Shape(s) if s.text == "Hello\nthere"));
        assert_eq!(objs[2].anchor, Anchor::Abs { x: 10.0, y: 400.0, w: 100.0, h: 30.0 });
        // Unchanged objects keep their parts; moving one rewrites only its anchor.
        let mut b2 = back.clone();
        b2.sheets[0].objects[0].anchor.set_rect([0.0, 0.0, 128.0, 100.0], |x| (x / 64.0) as u32, |c| c as f32 * 64.0, |y| (y / 20.0) as u32, |r| r as f32 * 20.0);
        b2.sheets[0].objects.remove(1);
        let bytes2 = crate::xlsx::write(&b2).unwrap();
        let back2 = crate::xlsx::read(&bytes2).unwrap();
        assert_eq!(back2.sheets[0].objects.len(), 2);
        assert_eq!(back2.sheets[0].objects[0].anchor, Anchor::Two { from: Mark::default(), to: Mark { col: 2, dx: 0.0, row: 5, dy: 0.0 }, edit_as: None });
        // The deleted picture's media part is gone.
        assert!(!back2.parts.keys().any(|k| k.starts_with("xl/media/")));
        let ct = String::from_utf8(read_zip(&bytes2, "[Content_Types].xml")).unwrap();
        assert!(ct.contains("drawingml.chart+xml") && ct.contains("drawing+xml"));
    }

    fn read_zip(bytes: &[u8], name: &str) -> Vec<u8> {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut f = z.by_name(name).unwrap();
        let mut v = vec![]; f.read_to_end(&mut v).unwrap(); v
    }

    #[test]
    fn duplicated_sheet_gets_its_own_parts() {
        let mut wb = book();
        let chart = Chart::from_range(&wb, 0, parse_range("A1:C3").unwrap(), ChartKind::Pie, None);
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Chart(Box::new(chart)), Anchor::default()), "Chart");
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let mut b = back.clone();
        b.duplicate_sheet(0);
        let again = crate::xlsx::read(&crate::xlsx::write(&b).unwrap()).unwrap();
        let parts: Vec<_> = again.sheets.iter().map(|s| s.objects[0].chart().unwrap().part.clone().unwrap()).collect();
        assert_eq!(parts.len(), 2);
        assert_ne!(parts[0], parts[1]);
        assert_eq!(again.sheets[1].objects[0].chart().unwrap().kind, ChartKind::Pie);
    }

    #[test]
    fn unchanged_chart_part_is_kept_byte_for_byte() {
        let mut wb = book();
        let chart = Chart::from_range(&wb, 0, parse_range("A1:C3").unwrap(), ChartKind::Column, None);
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Chart(Box::new(chart)), Anchor::default()), "Chart");
        let mut back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let part = back.sheets[0].objects[0].chart().unwrap().part.clone().unwrap();
        let custom = format!("{}<!-- kept -->", String::from_utf8(back.parts[&part].clone()).unwrap());
        back.parts.insert(part.clone(), custom.clone().into_bytes());
        let again = crate::xlsx::read(&crate::xlsx::write(&back).unwrap()).unwrap();
        let p2 = again.sheets[0].objects[0].chart().unwrap().part.clone().unwrap();
        assert_eq!(String::from_utf8(again.parts[&p2].clone()).unwrap(), custom);
    }

    #[test]
    fn slicer_and_pivot_round_trip() {
        let mut wb = book();
        let mut p = crate::pivot::Pivot::new("PivotTable1", "Sheet1", parse_range("A1:C3").unwrap(), Pos::new(5, 0));
        p.rows = vec![0];
        p.values = vec![crate::pivot::ValueField::new(1, crate::pivot::Agg::Sum)];
        p.hidden.insert(0, ["y".to_string()].into());
        p.apply(&mut wb, 0);
        wb.sheets[0].pivots.push(p.clone());
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Slicer(Slicer { pivot: "PivotTable1".into(), table: String::new(), field: 0, caption: "Col".into() }), Anchor::default()), "Slicer");
        assert_eq!(slicer_items(&wb, match &wb.sheets[0].objects[0].kind { ObjKind::Slicer(s) => s, _ => unreachable!() }), vec![("x".to_string(), true), ("y".to_string(), false)]);
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        assert_eq!(back.sheets[0].pivots, vec![p]);
        assert!(matches!(&back.sheets[0].objects[0].kind, ObjKind::Slicer(s) if s.pivot == "PivotTable1" && s.caption == "Col"));
        assert!(back.names.is_empty());
    }

    /// Open → save → open for the files in $JADWAL_ROUND_TRIP (separated by ':'), comparing objects.
    #[test]
    #[ignore]
    fn files_round_trip() {
        let Ok(list) = std::env::var("JADWAL_ROUND_TRIP") else { return };
        for f in list.split(':').filter(|f| !f.is_empty()) {
            let a = crate::xlsx::read(&std::fs::read(f).unwrap()).unwrap();
            let bytes = crate::xlsx::write(&a).unwrap();
            let b = crate::xlsx::read(&bytes).unwrap();
            let count = |w: &Workbook| w.sheets.iter().map(|s| s.objects.len()).sum::<usize>();
            let charts = |w: &Workbook| w.sheets.iter().flat_map(|s| s.objects.iter()).filter(|o| o.is_chart()).count();
            eprintln!("{f}: {} objects ({} charts) -> {} ({})", count(&a), charts(&a), count(&b), charts(&b));
            assert_eq!(count(&a), count(&b), "{f}");
            assert_eq!(charts(&a), charts(&b), "{f}");
            for (x, y) in a.sheets.iter().zip(&b.sheets) { for (o, p) in x.objects.iter().zip(&y.objects) { assert_eq!(o.anchor, p.anchor); assert_eq!(std::mem::discriminant(&o.kind), std::mem::discriminant(&p.kind)); } }
        }
    }
}
