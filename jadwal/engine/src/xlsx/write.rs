//! Writing a package (xlsx-export.js): every part Excel needs, generated from the model, plus
//! the parts and worksheet elements carried from the file that was read.

use super::read::{rels_name, resolve};
use super::xml::{attr, text, Element};
use super::{px_to_width, styles, to_file_formula};
use crate::addr::{Pos, Range, MAX_COLS};
use crate::book::{Input, Workbook, DEFAULT_COL_WIDTH};
use crate::value::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const CT: &str = "application/vnd.openxmlformats-officedocument.";
const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

/// CT_Worksheet's element order.
const ORDER: &[&str] = &["sheetPr", "dimension", "sheetViews", "sheetFormatPr", "cols", "sheetData", "sheetCalcPr", "sheetProtection", "protectedRanges", "scenarios", "autoFilter",
    "sortState", "dataConsolidate", "customSheetViews", "mergeCells", "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions", "pageMargins",
    "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF",
    "drawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"];

fn order(name: &str) -> usize { let l = super::xml::local(name); ORDER.iter().position(|n| *n == l).unwrap_or(ORDER.len() - 1) }

/// A relative target from the part `from` to the part `to`.
fn relative(from: &str, to: &str) -> String {
    let a: Vec<&str> = from.split('/').collect();
    let b: Vec<&str> = to.split('/').collect();
    let dir = &a[..a.len() - 1];
    let common = dir.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let mut out: Vec<String> = std::iter::repeat_n("..".to_string(), dir.len() - common).collect();
    out.extend(b[common..].iter().map(|s| s.to_string()));
    out.join("/")
}

/// A relationships part being built; new ids skip the ones carried over.
#[derive(Default)]
struct Rels { items: Vec<(String, String, String, bool)>, used: HashSet<String> }

impl Rels {
    fn keep(&mut self, id: &str, kind: &str, target: &str, external: bool) { self.used.insert(id.to_string()); self.items.push((id.into(), kind.into(), target.into(), external)); }
    fn add(&mut self, kind: &str, target: &str, external: bool) -> String {
        let id = (1..).map(|i| format!("rId{i}")).find(|i| !self.used.contains(i)).unwrap();
        self.keep(&id, kind, target, external);
        id
    }
    fn xml(&self) -> String {
        let mut o = format!("{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        for (id, kind, target, ext) in &self.items {
            let kind = if kind.contains('/') { kind.clone() } else { format!("{RT}{kind}") };
            o.push_str(&format!("<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>", attr(id), attr(&kind), attr(target), if *ext { " TargetMode=\"External\"" } else { "" }));
        }
        o.push_str("</Relationships>");
        o
    }
}

/// The shared string table being built.
#[derive(Default)]
struct Sst { list: Vec<String>, index: HashMap<String, usize>, count: usize }
impl Sst {
    fn add(&mut self, s: &str) -> usize {
        self.count += 1;
        if let Some(&i) = self.index.get(s) { return i; }
        self.list.push(s.to_string()); self.index.insert(s.to_string(), self.list.len() - 1); self.list.len() - 1
    }
}

fn num(n: f64) -> String {
    let a = n.abs();
    if a != 0.0 && !(1e-7..1e21).contains(&a) { format!("{n:e}") } else { format!("{n}") }
}

/// A <c> element.
fn cell_xml(o: &mut String, p: Pos, style: u32, formula: Option<(&str, Option<(Range, bool)>)>, v: &Value, sst: &mut Sst) {
    // A rich value (picture in cell, xlsx/richdata.rs): an error cell naming its metadata.
    if let Some(vm) = super::richdata::marker_vm(v) {
        o.push_str(&format!("<c r=\"{}\"{} t=\"e\" vm=\"{vm}\"><v>#VALUE!</v></c>", p.a1(), if style != 0 { format!(" s=\"{style}\"") } else { String::new() }));
        return;
    }
    let v = v.scalar();
    let v = match v { Value::Num(n) if !n.is_finite() => Value::Err(crate::value::ErrKind::Num), v => v };
    o.push_str("<c r=\""); o.push_str(&p.a1()); o.push('"');
    if style != 0 { o.push_str(&format!(" s=\"{style}\"")); }
    let t = match (&v, formula.is_some()) {
        (Value::Str(_), true) => " t=\"str\"", (Value::Str(_), false) => " t=\"s\"", (Value::Bool(_), _) => " t=\"b\"", (Value::Err(_), _) => " t=\"e\"", _ => "",
    };
    o.push_str(t);
    if let Some((_, Some((_, true)))) = formula { o.push_str(" cm=\"1\""); }
    let body = !matches!(v, Value::Empty | Value::Array(_)) || formula.is_some();
    if !body { o.push_str("/>"); return; }
    o.push('>');
    if let Some((f, arr)) = formula {
        match (arr, crate::whatif::table_xml(f)) {
            (Some((r, _)), Some(dt)) => o.push_str(&format!("<f t=\"dataTable\" ref=\"{}\"{dt}/>", r.a1())),
            (Some((r, _)), None) => { o.push_str(&format!("<f t=\"array\" ref=\"{}\">", r.a1())); o.push_str(&text(&to_file_formula(f))); o.push_str("</f>"); }
            (None, _) => { o.push_str("<f>"); o.push_str(&text(&to_file_formula(f))); o.push_str("</f>"); }
        }
    }
    match &v {
        Value::Num(n) => o.push_str(&format!("<v>{}</v>", num(*n))),
        Value::Str(s) if formula.is_some() => o.push_str(&format!("<v>{}</v>", text(s))),
        Value::Str(s) => o.push_str(&format!("<v>{}</v>", sst.add(s))),
        Value::Bool(b) => o.push_str(&format!("<v>{}</v>", *b as u8)),
        Value::Err(e) => o.push_str(&format!("<v>{}</v>", e.text())),
        _ => {}
    }
    o.push_str("</c>");
}

/// What one worksheet writes besides its own part.
struct SheetOut { xml: String, rels: Rels, comments: Option<String>, vml: Option<String>, dynamic: bool, tables: Vec<(String, String)>, hf: Vec<(String, Vec<u8>)>, qtables: Vec<(String, String)> }

fn sheet_xml(wb: &Workbook, si: usize, sst: &mut Sst, path: &str, comments_path: &str, vml_path: &str, dxfs: &mut Vec<String>) -> SheetOut {
    let s = &wb.sheets[si];
    let mut rels = Rels::default();
    let mut parts: Vec<(usize, String)> = vec![];
    let extra = |n: &str| s.extra.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
    // A carried VML drawing with objects joins the notes' VML (vml_objects.rs).
    let merged = if s.notes.is_empty() { None } else { super::vml_objects::carried_vml(wb, si) };
    // Carried relationships keep their ids (the carried elements name them).
    for (k, v) in &s.extra {
        if k != "rel" { continue; }
        let f: Vec<&str> = v.split('\t').collect();
        if f.len() < 4 || merged.as_deref() == Some(f[2]) { continue; }
        let ext = f[3] == "1";
        if !ext && !wb.parts.contains_key(f[2]) { continue; }
        rels.keep(f[0], f[1], &if ext { f[2].to_string() } else { relative(path, f[2]) }, ext);
    }
    // sheetPr: the file's, with the tab color replaced.
    let mut pr = extra("sheetPr").and_then(|r| Element::parse(r).ok()).map(|e| styles::strip_prefix(&e)).unwrap_or_else(|| Element { name: "sheetPr".into(), ..Default::default() });
    pr.children.retain(|c| !matches!(c, super::xml::Node::Element(e) if e.local() == "tabColor"));
    if let Some(c) = s.tab_color { pr.children.insert(0, super::xml::Node::Element(Element { name: "tabColor".into(), attrs: vec![("rgb".into(), styles::rgb_hex(c))], children: vec![] })); }
    s.page.write_sheet_pr(&mut pr);
    crate::outline::write_sheet_pr(&s.outline, &mut pr);
    if !pr.children.is_empty() || !pr.attrs.is_empty() { parts.push((0, pr.to_xml())); }
    let ext = s.extent();
    parts.push((1, format!("<dimension ref=\"{}\"/>", ext.map_or("A1".into(), |e| if e == Pos::new(0, 0) { "A1".into() } else { Range::new(Pos::new(0, 0), e).a1() }))));
    // View.
    let mut v = String::from("<sheetViews><sheetView");
    if !s.gridlines { v.push_str(" showGridLines=\"0\""); } if !s.headers { v.push_str(" showRowColHeaders=\"0\""); } if !s.show_zeros { v.push_str(" showZeros=\"0\""); }
    if s.right_to_left { v.push_str(" rightToLeft=\"1\""); } if si == wb.active { v.push_str(" tabSelected=\"1\""); }
    if s.zoom != 100 && s.zoom > 0 { v.push_str(&format!(" zoomScale=\"{0}\" zoomScaleNormal=\"{0}\"", s.zoom.clamp(10, 400))); }
    let o = Pos::new(s.frozen_origin.row.min(s.frozen_rows), s.frozen_origin.col.min(s.frozen_cols));
    if (s.frozen_rows > 0 || s.frozen_cols > 0) && o != Pos::default() { v.push_str(&format!(" topLeftCell=\"{}\"", o.a1())); }
    v.push_str(" workbookViewId=\"0\">");
    let mut pane = None;
    if s.frozen_rows > 0 || s.frozen_cols > 0 {
        let p = match (s.frozen_rows > 0, s.frozen_cols > 0) { (true, true) => "bottomRight", (true, false) => "bottomLeft", _ => "topRight" };
        v.push_str("<pane");
        if s.frozen_cols > 0 { v.push_str(&format!(" xSplit=\"{}\"", s.frozen_cols - o.col)); } if s.frozen_rows > 0 { v.push_str(&format!(" ySplit=\"{}\"", s.frozen_rows - o.row)); }
        v.push_str(&format!(" topLeftCell=\"{}\" activePane=\"{p}\" state=\"frozen\"/>", Pos::new(s.frozen_rows, s.frozen_cols).a1()));
        pane = Some(p);
    } else if let Some((x, p)) = crate::review::split_pane_xml(s) { v.push_str(&x); pane = Some(p); }
    if let Some(sel) = s.selection {
        v.push_str("<selection");
        if let Some(p) = pane { v.push_str(&format!(" pane=\"{p}\"")); }
        v.push_str(&format!(" activeCell=\"{}\" sqref=\"{}\"/>", sel.start().a1(), sel.a1()));
    } else if let Some(p) = pane { v.push_str(&format!("<selection pane=\"{p}\"/>")); }
    v.push_str("</sheetView></sheetViews>");
    parts.push((2, v));
    let mut fp = format!("<sheetFormatPr defaultRowHeight=\"{}\"", crate::value::fmt_num((s.default_row_height as f64 * 0.75 * 100.0).round() / 100.0));
    if (s.default_row_height - crate::book::DEFAULT_ROW_HEIGHT).abs() > 0.01 { fp.push_str(" customHeight=\"1\""); }
    if (s.default_col_width - DEFAULT_COL_WIDTH).abs() > 0.01 { fp.push_str(&format!(" defaultColWidth=\"{}\"", px_to_width(s.default_col_width))); }
    fp.push_str(&crate::outline::format_attrs(&s.outline));
    fp.push_str("/>");
    parts.push((3, fp));
    // Columns, runs of equal settings as one <col>.
    let cols: BTreeSet<u32> = s.col_widths.keys().chain(s.hidden_cols.iter()).chain(s.col_styles.keys()).chain(s.outline.cols.keys()).chain(s.outline.collapsed_cols.iter()).copied().filter(|c| *c < MAX_COLS).collect();
    if !cols.is_empty() {
        let key = |c: u32| (s.col_widths.get(&c).map(|w| w.round() as i64), s.hidden_cols.contains(&c), s.col_styles.get(&c).copied().unwrap_or(0), crate::outline::line_attrs(&s.outline, false, c));
        let mut o = String::from("<cols>");
        let list: Vec<u32> = cols.into_iter().collect();
        let mut i = 0;
        while i < list.len() {
            let mut j = i;
            while j + 1 < list.len() && list[j + 1] == list[j] + 1 && key(list[j + 1]) == key(list[i]) { j += 1; }
            let (w, hidden, st, ol) = key(list[i]);
            o.push_str(&format!("<col min=\"{}\" max=\"{}\" width=\"{}\"", list[i] + 1, list[j] + 1, px_to_width(w.map_or(s.default_col_width, |w| w as f32))));
            if st != 0 { o.push_str(&format!(" style=\"{st}\"")); } if hidden { o.push_str(" hidden=\"1\""); } if w.is_some() { o.push_str(" customWidth=\"1\""); }
            o.push_str(&ol);
            o.push_str("/>");
            i = j + 1;
        }
        o.push_str("</cols>");
        parts.push((4, o));
    }
    // Arrays: legacy ones from the cell, dynamic ones from the area their values spill over.
    let mut areas: HashMap<Pos, Range> = HashMap::new();
    for (p, a) in &s.spill_anchor { let e = areas.entry(*a).or_insert(Range::cell(*a)); *e = e.union(&Range::cell(*p)); }
    let mut dynamic = false;
    let spills: BTreeMap<Pos, &Value> = s.spill.iter().map(|(p, v)| (*p, v)).collect();
    let mut rows: BTreeSet<u32> = s.row_heights.keys().chain(s.hidden_rows.iter()).chain(s.row_styles.keys()).chain(s.outline.rows.keys()).chain(s.outline.collapsed_rows.iter()).copied().collect();
    let mut last = None;
    for p in s.cells.keys().chain(spills.keys()) { if last != Some(p.row) { rows.insert(p.row); last = Some(p.row); } }
    let mut d = String::with_capacity(64 * s.cells.len() + 32);
    d.push_str("<sheetData>");
    for r in rows {
        d.push_str(&format!("<row r=\"{}\"", r + 1));
        if let Some(h) = s.row_heights.get(&r) { d.push_str(&format!(" ht=\"{}\" customHeight=\"1\"", crate::value::fmt_num((*h as f64 * 0.75 * 100.0).round() / 100.0))); }
        if s.hidden_rows.contains(&r) { d.push_str(" hidden=\"1\""); }
        d.push_str(&crate::outline::line_attrs(&s.outline, true, r));
        if let Some(st) = s.row_styles.get(&r).filter(|x| **x != 0) { d.push_str(&format!(" s=\"{st}\" customFormat=\"1\"")); }
        let lo = Pos::new(r, 0); let hi = Pos::new(r, MAX_COLS);
        let mut a = s.cells.range(lo..=hi).peekable();
        let mut b = spills.range(lo..=hi).peekable();
        let mut body = String::new();
        loop {
            let next_a = a.peek().map(|x| *x.0); let next_b = b.peek().map(|x| *x.0);
            match (next_a, next_b) {
                (None, None) => break,
                (Some(pa), pb) if pb.is_none_or(|pb| pa <= pb) => {
                    let (p, c) = a.next().unwrap();
                    if pb == Some(pa) { b.next(); }
                    match &c.input {
                        Input::Formula(f) => {
                            let arr = c.array.map(|r| (r, false)).or_else(|| areas.get(p).map(|r| (*r, true)));
                            if arr.is_some_and(|x| x.1) { dynamic = true; }
                            cell_xml(&mut body, *p, c.style, Some((&f.text, arr)), &c.value, sst);
                        }
                        Input::Value(v) => cell_xml(&mut body, *p, c.style, None, v, sst),
                        Input::Empty => cell_xml(&mut body, *p, c.style, None, s.spill.get(p).unwrap_or(&Value::Empty), sst),
                    }
                }
                _ => { let (p, v) = b.next().unwrap(); cell_xml(&mut body, *p, s.style_index(*p), None, v, sst); }
            }
        }
        if body.is_empty() { d.push_str("/>"); } else { d.push('>'); d.push_str(&body); d.push_str("</row>"); }
    }
    d.push_str("</sheetData>");
    parts.push((5, d));
    if s.protected { parts.push((order("sheetProtection"), extra("sheetProtection").and_then(|r| Element::parse(r).ok()).filter(|e| e.flag("sheet", false)).map(|e| styles::strip_prefix(&e).to_xml()).unwrap_or_else(|| "<sheetProtection sheet=\"1\" objects=\"1\" scenarios=\"1\"/>".into()))); }
    if let Some(af) = s.autofilter {
        let raw = extra("autoFilter").and_then(|r| Element::parse(r).ok()).filter(|e| e.attr("ref").and_then(crate::addr::parse_range) == Some(af)).map(|e| styles::strip_prefix(&e).to_xml());
        parts.push((order("autoFilter"), raw.filter(|_| !s.filter.is_active()).unwrap_or_else(|| crate::tables::filter_xml(af, &s.filter))));
    }
    if !s.merges.is_empty() {
        let mut m = format!("<mergeCells count=\"{}\">", s.merges.len());
        for r in &s.merges { m.push_str(&format!("<mergeCell ref=\"{}\"/>", r.a1())); }
        m.push_str("</mergeCells>");
        parts.push((order("mergeCells"), m));
    }
    if !s.links.is_empty() {
        let mut h = String::from("<hyperlinks>");
        for (p, l) in &s.links {
            h.push_str(&format!("<hyperlink ref=\"{}\"", p.a1()));
            if let Some(loc) = l.target.strip_prefix('#') { h.push_str(&format!(" location=\"{}\"", attr(loc))); }
            else { let id = rels.add("hyperlink", &l.target, true); h.push_str(&format!(" r:id=\"{id}\"")); }
            if let Some(t) = &l.tooltip { h.push_str(&format!(" tooltip=\"{}\"", attr(t))); }
            h.push_str("/>");
        }
        h.push_str("</hyperlinks>");
        parts.push((order("hyperlinks"), h));
    }
    let (mut comments, mut vml) = (None, None);
    let mut merged_rels: Option<(String, Vec<u8>)> = None;
    if !s.notes.is_empty() {
        rels.add("comments", &relative(path, comments_path), false);
        let id = rels.add("vmlDrawing", &relative(path, vml_path), false);
        parts.push((order("legacyDrawing"), format!("<legacyDrawing r:id=\"{id}\"/>")));
        // A thread's note names it as Excel does: author "tc={id}" and xr:uid = the thread's id,
        // which is how Excel pairs the legacy note with the threaded comment.
        let tc = |p: &crate::Pos| s.review.threads.get(p).and_then(|t| t.comments.first()).map(|c| c.id.clone()).filter(|i| !i.is_empty());
        let mut authors: Vec<String> = vec![String::new()];
        for p in s.notes.keys() { if let Some(id) = tc(p) { authors.push(format!("tc={id}")); } }
        let any_tc = authors.len() > 1;
        let mut c = format!("{DECL}<comments xmlns=\"{NS}\"{}><authors>{}</authors><commentList>",
            if any_tc { " xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"xr\" xmlns:xr=\"http://schemas.microsoft.com/office/spreadsheetml/2014/revision\"" } else { "" },
            authors.iter().map(|a| format!("<author>{}</author>", text(a))).collect::<String>());
        let mut x = String::from("<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">");
        x.push_str(&format!("<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"{}\"/></o:shapelayout>", si + 1));
        x.push_str("<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>");
        let (mshapes, mrels, mmax) = merged.as_deref().map(|p| super::vml_objects::merge_shapes(wb, p)).unwrap_or_default();
        let base = (1024 * (si + 1)).max(mmax);
        for (k, (p, t)) in s.notes.iter().enumerate() {
            let (aid, uid) = match tc(p) { Some(id) => (authors.iter().position(|a| *a == format!("tc={id}")).unwrap_or(0), format!(" shapeId=\"0\" xr:uid=\"{}\"", attr(&id))), None => (0, String::new()) };
            c.push_str(&format!("<comment ref=\"{}\" authorId=\"{aid}\"{uid}><text><t xml:space=\"preserve\">{}</t></text></comment>", p.a1(), text(t)));
            x.push_str(&format!("<v:shape id=\"_x0000_s{}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:{};visibility:hidden\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\">", base + k + 1, k + 1));
            x.push_str("<v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox>");
            x.push_str(&format!("<x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{}, 15, {}, 10, {}, 15, {}, 4</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{}</x:Row><x:Column>{}</x:Column></x:ClientData></v:shape>",
                p.col + 1, p.row, p.col + 3, p.row + 4, p.row, p.col));
        }
        c.push_str("</commentList></comments>");
        x.push_str(&mshapes);
        x.push_str("</xml>");
        if !mrels.is_empty() {
            let mut vr = Rels::default();
            for (id, kind, target) in &mrels { if wb.parts.contains_key(target) { vr.keep(id, kind, &relative(vml_path, target), false); } }
            merged_rels = Some((rels_name(vml_path), vr.xml().into_bytes()));
        }
        comments = Some(c); vml = Some(x);
    }
    // Header/footer pictures (hfpic.rs): their own VML part, with the pictures as media.
    let mut hf = vec![];
    hf.extend(merged_rels);
    let pics = &s.page.header_footer.pictures;
    if !pics.is_empty() {
        let vp = format!("xl/drawings/vmlDrawingHF{}.vml", si + 1);
        let id = rels.add("vmlDrawing", &relative(path, &vp), false);
        parts.push((order("legacyDrawingHF"), format!("<legacyDrawingHF r:id=\"{id}\"/>")));
        let mut vr = Rels::default();
        for (k, p) in pics.iter().enumerate() {
            let mp = format!("xl/media/hfimage{}_{}.{}", si + 1, k + 1, if p.ext.is_empty() { "png" } else { &p.ext });
            vr.keep(&format!("rId{}", k + 1), "image", &relative(&vp, &mp), false);
            hf.push((mp, p.data.to_vec()));
        }
        hf.push((rels_name(&vp), vr.xml().into_bytes()));
        hf.push((vp, crate::hfpic::write_vml(pics, si).into_bytes()));
    }
    // Tables (numbered across the workbook), written from the model.
    let mut tables = vec![];
    let mut qtables = vec![];
    if !s.tables.is_empty() {
        let first = wb.sheets[..si].iter().map(|x| x.tables.len()).sum::<usize>() + 1;
        let mut tp = format!("<tableParts count=\"{}\">", s.tables.len());
        for (k, t) in s.tables.iter().enumerate() {
            let tpath = format!("xl/tables/table{}.xml", first + k);
            let id = rels.add("table", &relative(path, &tpath), false);
            tp.push_str(&format!("<tablePart r:id=\"{id}\"/>"));
            let mut tx = crate::tables::table_xml(t, (first + k) as u32);
            // A Power Query result table is tied to its connection by a query table.
            if let Some(qt) = crate::powerquery::query_table_xml(wb, t, &mut tx) { qtables.push((tpath.clone(), qt)); }
            tables.push((tpath, tx));
        }
        tp.push_str("</tableParts>");
        parts.push((order("tableParts"), tp));
    }
    // Conditional formats and validations; their x14 extension joins the carried extLst.
    let mut rules = super::rules::write(s, si, dxfs);
    crate::sparkline::add_ext(&mut rules.ext, s);
    for c in &rules.cond { parts.push((order("conditionalFormatting"), c.clone())); }
    if let Some(v) = &rules.validations { parts.push((order("dataValidations"), v.clone())); }
    if let Some(x) = &rules.ext { parts.push((order("extLst"), super::rules::merge_ext(extra("extLst"), x))); }
    // Carried elements (those whose relationship is gone are dropped).
    for (k, raw) in &s.extra {
        if matches!(k.as_str(), "rel" | "xmlns" | "sheetPr" | "sheetProtection" | "autoFilter" | "tableParts") { continue; }
        if k == "extLst" && rules.ext.is_some() { continue; }
        if (k == "legacyDrawing" && !s.notes.is_empty()) || (k == "legacyDrawingHF" && !s.page.header_footer.pictures.is_empty()) || order(k) <= 5 { continue; }
        if let Some(id) = rel_id(raw) { if !rels.used.contains(&id) { continue; } }
        parts.push((order(k), raw.clone()));
    }
    // Page setup (printOptions, pageMargins, pageSetup, headerFooter, rowBreaks, colBreaks).
    for (k, xml) in s.page.write_xml() { parts.push((order(k), xml)); }
    parts.sort_by_key(|p| p.0);
    let mut ns = format!(" xmlns=\"{NS}\" xmlns:r=\"{NS_R}\"");
    if let Some(x) = extra("xmlns") {
        // The file's other declarations, for the carried XML (skipping ones that clash).
        if let Ok(e) = Element::parse(&format!("<a{x}/>")) {
            for (k, v) in &e.attrs { if !ns.contains(&format!(" {k}=")) { ns.push_str(&format!(" {k}=\"{}\"", attr(v))); } }
        }
    }
    let mut xml = format!("{DECL}<worksheet{ns}>");
    for (_, p) in parts { xml.push_str(&p); }
    xml.push_str("</worksheet>");
    SheetOut { xml, rels, comments, vml, dynamic, tables, hf, qtables }
}

/// The relationship id a carried element names (r:id), if any.
fn rel_id(raw: &str) -> Option<String> {
    let e = Element::parse(raw).ok()?;
    fn walk(e: &Element) -> Option<String> {
        if let Some((_, v)) = e.attrs.iter().find(|(k, _)| k.ends_with(":id") && !k.starts_with("xmlns")) { return Some(v.clone()); }
        e.elements().find_map(walk)
    }
    // Only the elements that are themselves references (drawing, tableParts...) are checked.
    if matches!(e.local(), "drawing" | "legacyDrawing" | "legacyDrawingHF" | "drawingHF" | "picture" | "tableParts" | "oleObjects" | "controls") { walk(&e) } else { None }
}

fn now_w3c() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (y, m, d) = crate::date::civil_from_days(secs.div_euclid(86400));
    let t = secs.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
}



/// Parts regenerated on every save, never copied from `Workbook.parts`.
fn generated(name: &str) -> bool {
    matches!(name, "[Content_Types].xml" | "_rels/.rels" | "xl/workbook.xml" | "xl/_rels/workbook.xml.rels" | "xl/styles.xml" | "xl/sharedStrings.xml" | "docProps/app.xml" | "xl/metadata.xml" | "xl/calcChain.xml" | "xl/theme/theme1.xml")
        || name.starts_with("xl/worksheets/sheet") && !name.contains("/_rels/") || name.starts_with("xl/tables/")
}

pub fn write(wb: &Workbook) -> Result<Vec<u8>, String> {
    // Objects and pivot definitions become parts and names on a copy (drawing.rs).
    let staged = crate::cellimage::stage(wb);
    let wb = staged.as_ref().unwrap_or(wb);
    let vml_staged = super::vml_objects::stage(wb);
    let wb = vml_staged.as_ref().unwrap_or(wb);
    let queried = crate::connections::stage(wb);
    let wb = queried.as_ref().unwrap_or(wb);
    let mashed = crate::powerquery::stage(wb);
    let wb = mashed.as_ref().unwrap_or(wb);
    let reviewed = crate::review::stage(wb);
    let wb = reviewed.as_ref().unwrap_or(wb);
    let prepared = crate::drawing::prepare(wb);
    let wb = prepared.as_ref().unwrap_or(wb);
    let mut out: Vec<(String, Vec<u8>)> = vec![];
    let mut overrides: Vec<(String, String)> = vec![];
    let mut defaults: BTreeMap<String, String> = BTreeMap::new();
    defaults.insert("rels".into(), "application/vnd.openxmlformats-package.relationships+xml".into());
    defaults.insert("xml".into(), "application/xml".into());
    // The original content types, for the carried parts.
    let orig_ct = wb.parts.get("[Content_Types].xml").and_then(|b| Element::parse(&super::xml::part_text(b)).ok());
    let mut orig_over: HashMap<String, String> = HashMap::new();
    if let Some(ct) = &orig_ct {
        for e in ct.elements() {
            match e.local() {
                "Default" => if let (Some(x), Some(t)) = (e.attr("Extension"), e.attr("ContentType")) { defaults.entry(x.to_ascii_lowercase()).or_insert(t.into()); },
                "Override" => if let (Some(p), Some(t)) = (e.attr("PartName"), e.attr("ContentType")) { orig_over.insert(p.trim_start_matches('/').into(), t.into()); },
                _ => {}
            }
        }
    }
    let merged_vml: HashSet<String> = (0..wb.sheets.len()).filter(|&i| !wb.sheets[i].notes.is_empty()).filter_map(|i| super::vml_objects::carried_vml(wb, i)).flat_map(|p| [rels_name(&p), p]).collect();
    let carried: Vec<&String> = wb.parts.keys().filter(|k| !generated(k) && !merged_vml.contains(*k)).collect();
    let taken: HashSet<&str> = carried.iter().map(|s| s.as_str()).collect();
    let free = |prefix: &str, ext: &str, used: &mut HashSet<String>| -> String {
        let n = (1..).map(|i| format!("{prefix}{i}{ext}")).find(|n| !taken.contains(n.as_str()) && !used.contains(n)).unwrap();
        used.insert(n.clone()); n
    };
    let mut used_names: HashSet<String> = HashSet::new();
    let mut sst = Sst::default();
    // Differential formats: the file's, then the ones conditional formats add.
    let mut dxfs = super::rules::dxf_table(wb.parts.get("xl/styles.xml").and_then(|b| Element::parse(&super::xml::part_text(b)).ok()).as_ref());
    let mut wrels = Rels::default();
    // Workbook relationships carried from the file (external links, pivot caches, connections).
    let orig_wb = wb.parts.get("xl/workbook.xml").and_then(|b| Element::parse(&super::xml::part_text(b)).ok());
    if let Some(r) = wb.parts.get("xl/_rels/workbook.xml.rels").and_then(|b| Element::parse(&super::xml::part_text(b)).ok()) {
        for e in r.all("Relationship") {
            let (Some(id), Some(kind), Some(target)) = (e.attr("Id"), e.attr("Type"), e.attr("Target")) else { continue };
            let short = kind.rsplit('/').next().unwrap_or("");
            let ext = e.attr("TargetMode") == Some("External");
            if !matches!(short, "externalLink" | "pivotCacheDefinition" | "slicerCache" | "timelineCache" | "connections" | "customXml" | "xmlMaps" | "volatileDependencies") && !super::richdata::WB_KINDS.contains(&short) { continue; }
            if !ext && !wb.parts.contains_key(&resolve("xl/workbook.xml", target)) { continue; }
            wrels.keep(id, kind, target, ext);
        }
    }
    // Connections added in Jadwal (Data ▸ Get Data) need their relationship.
    if wb.parts.contains_key("xl/connections.xml") && !wrels.items.iter().any(|r| r.1.ends_with("/connections")) {
        wrels.add("http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections", "connections.xml", false);
    }
    // Custom XML parts added in Jadwal (the Power Query DataMashup) need theirs.
    for k in wb.parts.keys().filter(|k| k.starts_with("customXml/item") && !k.contains("Props") && k.ends_with(".xml")) {
        let target = format!("../{k}");
        if !wrels.items.iter().any(|r| r.2 == target || resolve("xl/workbook.xml", &r.2) == *k) { wrels.add("http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml", &target, false); }
    }
    let mut sheets_xml = String::from("<sheets>");
    let mut any_dynamic = false;
    for (i, s) in wb.sheets.iter().enumerate() {
        let path = format!("xl/worksheets/sheet{}.xml", i + 1);
        let (cp, vp) = if s.notes.is_empty() { (String::new(), String::new()) } else { (free("xl/comments", ".xml", &mut used_names), free("xl/drawings/vmlDrawing", ".vml", &mut used_names)) };
        let mut so = sheet_xml(wb, i, &mut sst, &path, &cp, &vp, &mut dxfs);
        if let Some(x) = crate::review::threads_xml(wb, i) {
            let tp = free("xl/threadedComments/threadedComment", ".xml", &mut used_names);
            so.rels.add(crate::review::RT_THREADED, &relative(&path, &tp), false);
            overrides.push((tp.clone(), crate::review::CT_THREADED.into()));
            out.push((tp, x.into_bytes()));
        }
        any_dynamic |= so.dynamic;
        let id = wrels.add("worksheet", &format!("worksheets/sheet{}.xml", i + 1), false);
        sheets_xml.push_str(&format!("<sheet name=\"{}\" sheetId=\"{}\"{} r:id=\"{id}\"/>", attr(&s.name), i + 1, if s.hidden { " state=\"hidden\"" } else { "" }));
        overrides.push((path.clone(), format!("{CT}spreadsheetml.worksheet+xml")));
        if !so.rels.items.is_empty() { out.push((rels_name(&path), so.rels.xml().into_bytes())); }
        out.push((path, so.xml.into_bytes()));
        for (name, bytes) in so.hf {
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            let mime = match ext.as_str() { "vml" => format!("{CT}vmlDrawing"), "rels" => "application/vnd.openxmlformats-package.relationships+xml".into(), "jpg" | "jpeg" => "image/jpeg".into(), "gif" => "image/gif".into(), "bmp" => "image/bmp".into(), _ => "image/png".into() };
            defaults.entry(ext).or_insert(mime);
            out.push((name, bytes));
        }
        for (tp, tx) in so.tables { overrides.push((tp.clone(), format!("{CT}spreadsheetml.table+xml"))); out.push((tp, tx.into_bytes())); }
        for (tp, qx) in so.qtables {
            let qp = free("xl/queryTables/queryTable", ".xml", &mut used_names);
            let mut tr = Rels::default();
            tr.add(crate::connections::RT_QUERY_TABLE, &relative(&tp, &qp), false);
            out.push((rels_name(&tp), tr.xml().into_bytes()));
            overrides.push((qp.clone(), crate::connections::CT_QUERY_TABLE.into()));
            out.push((qp, qx.into_bytes()));
        }
        if let (Some(c), Some(v)) = (so.comments, so.vml) {
            overrides.push((cp.clone(), format!("{CT}spreadsheetml.comments+xml")));
            defaults.insert("vml".into(), format!("{CT}vmlDrawing"));
            out.push((cp, c.into_bytes()));
            out.push((vp, v.into_bytes()));
        }
    }
    sheets_xml.push_str("</sheets>");
    if let Some(x) = crate::review::persons_xml(wb) {
        wrels.add(crate::review::RT_PERSON, "persons/person.xml", false);
        overrides.push(("xl/persons/person.xml".into(), crate::review::CT_PERSON.into()));
        out.push(("xl/persons/person.xml".into(), x.into_bytes()));
    }
    // Styles, strings, theme, metadata.
    let styles_xml = styles::write(wb.styles.all(), wb.parts.get("xl/styles.xml").and_then(|b| Element::parse(&super::xml::part_text(b)).ok()).as_ref(), &dxfs, &styles::ThemeCtx { colors: &wb.theme_colors, major: &wb.theme.major, minor: &wb.theme.minor });
    out.push(("xl/styles.xml".into(), styles_xml.into_bytes()));
    wrels.add("styles", "styles.xml", false);
    overrides.push(("xl/styles.xml".into(), format!("{CT}spreadsheetml.styles+xml")));
    let theme = crate::theme::part_for(wb);
    out.push(("xl/theme/theme1.xml".into(), theme));
    wrels.add("theme", "theme/theme1.xml", false);
    overrides.push(("xl/theme/theme1.xml".into(), format!("{CT}theme+xml")));
    let mut sx = format!("{DECL}<sst xmlns=\"{NS}\" count=\"{}\" uniqueCount=\"{}\">", sst.count, sst.list.len());
    for s in &sst.list {
        let sp = s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) || s.contains('\n');
        sx.push_str(&format!("<si><t{}>{}</t></si>", if sp { " xml:space=\"preserve\"" } else { "" }, text(s)));
    }
    sx.push_str("</sst>");
    out.push(("xl/sharedStrings.xml".into(), sx.into_bytes()));
    wrels.add("sharedStrings", "sharedStrings.xml", false);
    overrides.push(("xl/sharedStrings.xml".into(), format!("{CT}spreadsheetml.sharedStrings+xml")));
    if let Some(md) = super::richdata::metadata(wb, any_dynamic) {
        out.push(("xl/metadata.xml".into(), md.into_bytes()));
        wrels.add("sheetMetadata", "metadata.xml", false);
        overrides.push(("xl/metadata.xml".into(), format!("{CT}spreadsheetml.sheetMetadata+xml")));
    }
    // The workbook part.
    let carry = |n: &str| orig_wb.as_ref().and_then(|w| w.child(n)).filter(|e| { let mut ok = true; check_ids(e, &wrels.used, &mut ok); ok }).map(|e| styles::strip_prefix(e).to_xml()).unwrap_or_default();
    let mut w = format!("{DECL}<workbook xmlns=\"{NS}\" xmlns:r=\"{NS_R}\">");
    w.push_str(&format!("<workbookPr{} defaultThemeVersion=\"166925\"/>", if wb.date1904 { " date1904=\"1\"" } else { "" }));
    w.push_str(&crate::review::workbook_protection_xml(wb));
    w.push_str(&format!("<bookViews><workbookView xWindow=\"0\" yWindow=\"0\" windowWidth=\"28800\" windowHeight=\"12300\"{}/></bookViews>", if wb.active > 0 { format!(" activeTab=\"{}\"", wb.active) } else { String::new() }));
    w.push_str(&sheets_xml);
    w.push_str(&carry("externalReferences"));
    if !wb.names.is_empty() {
        w.push_str("<definedNames>");
        for n in &wb.names {
            if n.sheet.is_some_and(|s| s >= wb.sheets.len()) { continue; }
            w.push_str(&format!("<definedName name=\"{}\"", attr(&n.name)));
            if let Some(s) = n.sheet { w.push_str(&format!(" localSheetId=\"{s}\"")); }
            if n.hidden { w.push_str(" hidden=\"1\""); }
            if !n.comment.is_empty() { w.push_str(&format!(" comment=\"{}\"", attr(&n.comment))); }
            w.push_str(&format!(">{}</definedName>", text(&to_file_formula(n.formula.trim_start_matches('=')))));
        }
        w.push_str("</definedNames>");
    }
    w.push_str("<calcPr calcId=\"191029\" fullCalcOnLoad=\"1\"");
    if wb.manual { w.push_str(" calcMode=\"manual\""); } else if wb.calc_no_tables { w.push_str(" calcMode=\"autoNoTable\""); }
    if let Some((n, d)) = wb.iterate { w.push_str(&format!(" iterate=\"1\" iterateCount=\"{n}\" iterateDelta=\"{d}\"")); }
    w.push_str("/>");
    w.push_str(&crate::review::custom_views_xml(wb));
    w.push_str(&carry("pivotCaches"));
    w.push_str(&super::slicer_parts::workbook_ext(orig_wb.as_ref(), &wrels.used));
    w.push_str("</workbook>");
    out.push(("xl/workbook.xml".into(), w.into_bytes()));
    overrides.push(("xl/workbook.xml".into(), format!("{CT}spreadsheetml.sheet.main+xml")));
    out.push(("xl/_rels/workbook.xml.rels".into(), wrels.xml().into_bytes()));
    // Document properties and the package relationships.
    let mut prels = Rels::default();
    prels.add("officeDocument", "xl/workbook.xml", false);
    if !wb.parts.contains_key("docProps/core.xml") {
        let now = now_w3c();
        out.push(("docProps/core.xml".into(), format!("{DECL}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dcterms:created xsi:type=\"dcterms:W3CDTF\">{now}</dcterms:created><dcterms:modified xsi:type=\"dcterms:W3CDTF\">{now}</dcterms:modified></cp:coreProperties>").into_bytes()));
    }
    prels.add("http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties", "docProps/core.xml", false);
    overrides.push(("docProps/core.xml".into(), "application/vnd.openxmlformats-package.core-properties+xml".into()));
    let mut titles = String::new();
    for s in &wb.sheets { titles.push_str(&format!("<vt:lpstr>{}</vt:lpstr>", text(&s.name))); }
    out.push(("docProps/app.xml".into(), format!("{DECL}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\"><Application>Jadwal</Application><DocSecurity>0</DocSecurity><ScaleCrop>false</ScaleCrop><HeadingPairs><vt:vector size=\"2\" baseType=\"variant\"><vt:variant><vt:lpstr>Worksheets</vt:lpstr></vt:variant><vt:variant><vt:i4>{}</vt:i4></vt:variant></vt:vector></HeadingPairs><TitlesOfParts><vt:vector size=\"{0}\" baseType=\"lpstr\">{titles}</vt:vector></TitlesOfParts><AppVersion>16.0300</AppVersion></Properties>", wb.sheets.len()).into_bytes()));
    prels.add("extended-properties", "docProps/app.xml", false);
    overrides.push(("docProps/app.xml".into(), format!("{CT}extended-properties+xml")));
    if wb.parts.contains_key("docProps/custom.xml") { prels.add("custom-properties", "docProps/custom.xml", false); }
    out.push(("_rels/.rels".into(), prels.xml().into_bytes()));
    // Carried parts, typed as the original file typed them.
    for k in carried {
        if k == "docProps/core.xml" { out.push((k.clone(), wb.parts[k].clone())); continue; }
        let ext = k.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if let Some(t) = orig_over.get(k.as_str()) { overrides.push((k.clone(), t.clone())); }
        else if k == "xl/connections.xml" { overrides.push((k.clone(), format!("{CT}spreadsheetml.connections+xml"))); }
        else if k.starts_with("xl/queryTables/") && k.ends_with(".xml") { overrides.push((k.clone(), crate::connections::CT_QUERY_TABLE.into())); }
        else if k.starts_with("customXml/itemProps") && k.ends_with(".xml") { overrides.push((k.clone(), crate::powerquery::mashup::CT_ITEM_PROPS.into())); }
        else if k.starts_with("customXml/item") && k.ends_with(".xml") { overrides.push((k.clone(), "application/xml".into())); }
        else if !defaults.contains_key(&ext) {
            let mime = match ext.as_str() { "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", "svg" => "image/svg+xml", "tif" | "tiff" => "image/tiff", "emf" => "image/x-emf", "wmf" => "image/x-wmf", "vml" => "application/vnd.openxmlformats-officedocument.vmlDrawing", "bin" => "application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings", _ => "application/octet-stream" };
            defaults.insert(ext, mime.into());
        }
        out.push((k.clone(), wb.parts[k].clone()));
    }
    let mut ct = format!("{DECL}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    for (x, t) in &defaults { ct.push_str(&format!("<Default Extension=\"{}\" ContentType=\"{}\"/>", attr(x), attr(t))); }
    let mut seen = HashSet::new();
    for (p, t) in &overrides { if seen.insert(p.clone()) { ct.push_str(&format!("<Override PartName=\"/{}\" ContentType=\"{}\"/>", attr(p), attr(t))); } }
    ct.push_str("</Types>");
    // The zip, content types first.
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("[Content_Types].xml", opts).map_err(|e| e.to_string())?;
        zip.write_all(ct.as_bytes()).map_err(|e| e.to_string())?;
        let mut done = HashSet::new();
        for (name, data) in &out {
            if !done.insert(name.clone()) { continue; }
            let o = if name.starts_with("xl/media/") { opts.compression_method(zip::CompressionMethod::Stored) } else { opts };
            zip.start_file(name.as_str(), o).map_err(|e| e.to_string())?;
            zip.write_all(data).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    let bytes = buf.into_inner();
    #[cfg(test)]
    super::pkgcheck::on_write(&bytes);
    Ok(bytes)
}

/// Whether every r:id inside a carried workbook element still has its relationship.
fn check_ids(e: &Element, used: &HashSet<String>, ok: &mut bool) {
    for (k, v) in &e.attrs { if k.ends_with(":id") && !k.starts_with("xmlns") && !used.contains(v) { *ok = false; } }
    for c in e.elements() { check_ids(c, used, ok); }
}

