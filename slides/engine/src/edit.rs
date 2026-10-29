//! Editing: slides (add from a layout, duplicate, delete, move, hide), shapes (insert,
//! delete, move/resize/rotate, order, fill and outline), text (replace, run formatting,
//! alignment, bullets) and notes. Every edit is an XML edit on the package.

use crate::opc::{self, CT_SLIDE};
use crate::pres::{Presentation, NS_A, NS_P, NS_R};
use crate::scene::{Loc, Xfrm};
use crate::xml::{Element, Node};

/// Run formatting to apply (None = leave as is).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunFormat {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    /// Points.
    pub size: Option<f64>,
    pub font: Option<String>,
    /// Hex RRGGBB, or "" to remove the direct color.
    pub color: Option<String>,
    pub baseline: Option<i32>,
    pub highlight: Option<Option<String>>,
}

const SP_TREE: &str = "p:cSld/p:spTree";

fn el_at<'a>(root: &'a mut Element, path: &[usize]) -> Option<&'a mut Element> {
    let mut cur = root.path_mut(SP_TREE)?;
    for &i in path { cur = cur.elements_mut().nth(i)?; }
    Some(cur)
}

/// The shape's spPr (or grpSpPr), created if missing.
fn sp_pr(sp: &mut Element) -> &mut Element {
    match sp.local() {
        "grpSp" => sp.ensure_at("p:grpSpPr", &["p:style", "p:txBody"]),
        _ => sp.ensure_at("p:spPr", &["p:style", "p:txBody"]),
    }
}

fn rpr_apply(rpr: &mut Element, f: &RunFormat) {
    let flag = |b: bool| if b { "1" } else { "0" };
    if let Some(b) = f.bold { rpr.set_attr("b", flag(b)); }
    if let Some(b) = f.italic { rpr.set_attr("i", flag(b)); }
    if let Some(b) = f.underline { rpr.set_attr("u", if b { "sng" } else { "none" }); }
    if let Some(b) = f.strike { rpr.set_attr("strike", if b { "sngStrike" } else { "noStrike" }); }
    if let Some(s) = f.size { rpr.set_attr("sz", ((s * 100.0).round() as i64).clamp(100, 400_000).to_string()); }
    if let Some(b) = f.baseline { if b == 0 { rpr.remove_attr("baseline"); } else { rpr.set_attr("baseline", (b * 1000).to_string()); } }
    // Child order: ln, fill, effect, highlight, uLnTx…, latin, ea, cs, sym, hlinkClick…
    const ORDER: [&str; 12] = ["a:ln", "a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill", "a:effectLst", "a:highlight", "a:latin", "a:ea", "a:cs"];
    let after = |name: &str| -> Vec<&'static str> { let i = ORDER.iter().position(|n| *n == name).unwrap_or(0); ORDER[i + 1..].iter().copied().chain(["a:sym", "a:hlinkClick", "a:hlinkMouseOver", "a:rtl", "a:extLst"]).collect() };
    if let Some(c) = &f.color {
        for n in ["a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill"] { rpr.remove_children(n); }
        if !c.is_empty() {
            let fill = rpr.ensure_at("a:solidFill", &after("a:solidFill"));
            fill.children = vec![Node::Element(Element::new("a:srgbClr").with_attr("val", c.clone()))];
        }
    }
    if let Some(h) = &f.highlight {
        rpr.remove_children("a:highlight");
        if let Some(h) = h { rpr.ensure_at("a:highlight", &after("a:highlight")).children = vec![Node::Element(Element::new("a:srgbClr").with_attr("val", h.clone()))]; }
    }
    if let Some(font) = &f.font {
        rpr.ensure_at("a:latin", &after("a:latin")).set_attr("typeface", font.clone());
        rpr.ensure_at("a:cs", &after("a:cs")).set_attr("typeface", font.clone());
    }
}

fn new_paragraph(text: &str, rpr: Option<&Element>, ppr: Option<&Element>) -> Element {
    let mut p = Element::new("a:p");
    if let Some(ppr) = ppr { p.children.push(Node::Element(ppr.clone())); }
    if !text.is_empty() {
        let mut r = Element::new("a:r");
        if let Some(rpr) = rpr { r.children.push(Node::Element(rpr.clone())); }
        r.children.push(Node::Element(Element::new("a:t").with_text(text)));
        p.children.push(Node::Element(r));
    }
    let mut end = rpr.cloned().unwrap_or_else(|| Element::new("a:rPr"));
    end.name = "a:endParaRPr".into();
    end.children.retain(|c| !matches!(c, Node::Element(e) if e.name == "a:hlinkClick"));
    if !end.attrs.is_empty() || !end.children.is_empty() { p.children.push(Node::Element(end)); }
    p
}

impl Presentation {
    fn next_slide_id(&self) -> u32 {
        self.root().child("p:sldIdLst").map(|l| l.elements().filter_map(|e| e.attr_i64("id")).max().unwrap_or(255) as u32 + 1).unwrap_or(256).max(256)
    }

    /// A free shape id on a part.
    pub fn next_shape_id(&self, part: &str) -> u32 {
        let mut ids = vec![];
        if let Some(root) = self.pkg.xml(part) { root.find_all("p:cNvPr", &mut ids); }
        ids.iter().filter_map(|e| e.attr_i64("id")).max().unwrap_or(1) as u32 + 1
    }

    pub fn add_slide_entry(&mut self, part: &str, at: usize) {
        let main = self.main.clone();
        let rid = self.pkg.add_rel(&main, opc::RT_SLIDE, part, false);
        let id = self.next_slide_id();
        let list = self.root_mut().ensure_at("p:sldIdLst", &["p:sldSz", "p:notesSz", "p:smartTags", "p:embeddedFontLst", "p:custShowLst", "p:photoAlbum", "p:custDataLst", "p:kinsoku", "p:defaultTextStyle", "p:modifyVerifier", "p:extLst"]);
        let entry = Node::Element(Element::new("p:sldId").with_attr("id", id.to_string()).with_attr("r:id", rid));
        let positions: Vec<usize> = list.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(_))).map(|(i, _)| i).collect();
        let pos = positions.get(at).copied().unwrap_or(list.children.len());
        list.children.insert(pos, entry);
    }

    /// Add a slide from a layout at position `at`; its placeholders come from the layout.
    pub fn add_slide(&mut self, layout: &str, at: usize) -> String {
        let part = self.pkg.free_name("ppt/slides/slide", ".xml");
        let mut tree = Element::new("p:spTree")
            .with_child(Element::new("p:nvGrpSpPr").with_child(Element::new("p:cNvPr").with_attr("id", "1").with_attr("name", "")).with_child(Element::new("p:cNvGrpSpPr")).with_child(Element::new("p:nvPr")))
            .with_child(Element::new("p:grpSpPr").with_child(Element::new("a:xfrm").with_child(Element::new("a:off").with_attr("x", "0").with_attr("y", "0")).with_child(Element::new("a:ext").with_attr("cx", "0").with_attr("cy", "0")).with_child(Element::new("a:chOff").with_attr("x", "0").with_attr("y", "0")).with_child(Element::new("a:chExt").with_attr("cx", "0").with_attr("cy", "0"))));
        let mut next_id = 2;
        if let Some(ltree) = self.pkg.xml(layout).and_then(|l| l.path(SP_TREE)) {
            for sp in ltree.children_named("p:sp") {
                let Some(ph) = sp.path("p:nvSpPr/p:nvPr/p:ph") else { continue };
                let t = ph.attr("type").unwrap_or("obj");
                if matches!(t, "dt" | "ftr" | "sldNum" | "hdr") { continue; }
                let name = sp.path("p:nvSpPr/p:cNvPr").and_then(|c| c.attr("name")).unwrap_or("Placeholder").to_string();
                let mut new_ph = Element::new("p:ph");
                for k in ["type", "idx", "orient", "sz"] { if let Some(v) = ph.attr(k) { new_ph.set_attr(k, v); } }
                let shape = Element::new("p:sp")
                    .with_child(Element::new("p:nvSpPr").with_child(Element::new("p:cNvPr").with_attr("id", next_id.to_string()).with_attr("name", name)).with_child(Element::new("p:cNvSpPr").with_child(Element::new("a:spLocks").with_attr("noGrp", "1"))).with_child(Element::new("p:nvPr").with_child(new_ph)))
                    .with_child(Element::new("p:spPr"))
                    .with_child(Element::new("p:txBody").with_child(Element::new("a:bodyPr")).with_child(Element::new("a:lstStyle")).with_child(Element::new("a:p")));
                // Picture, chart and table placeholders get no text body.
                let shape = if matches!(t, "pic" | "chart" | "tbl" | "media" | "clipArt" | "dgm") { let mut s = shape; s.remove_children("p:txBody"); s } else { shape };
                tree.children.push(Node::Element(shape));
                next_id += 1;
            }
        }
        let root = Element::new("p:sld").with_attr("xmlns:a", NS_A).with_attr("xmlns:r", NS_R).with_attr("xmlns:p", NS_P)
            .with_child(Element::new("p:cSld").with_child(tree))
            .with_child(Element::new("p:clrMapOvr").with_child(Element::new("a:masterClrMapping")));
        self.pkg.put_xml(&part, root, Some(CT_SLIDE));
        self.pkg.add_rel(&part, opc::RT_LAYOUT, layout, false);
        self.add_slide_entry(&part, at);
        part
    }

    pub fn duplicate_slide(&mut self, slide: &str) -> Option<String> {
        let at = self.slides().iter().position(|s| s == slide)? + 1;
        let root = self.pkg.xml(slide)?.clone();
        let part = self.pkg.free_name("ppt/slides/slide", ".xml");
        self.pkg.put_xml(&part, root, Some(CT_SLIDE));
        // Same relationships (layout, images, links), except notes.
        let rels = self.pkg.rels(slide);
        let rn = opc::rels_name(&part);
        if let Some(r) = self.pkg.xml(&opc::rels_name(slide)).cloned() {
            let mut r = r;
            r.children.retain(|n| !matches!(n, Node::Element(e) if e.attr("Type") == Some(opc::RT_NOTES)));
            self.pkg.put_xml(&rn, r, None);
        }
        let _ = rels;
        self.add_slide_entry(&part, at);
        Some(part)
    }

    pub fn delete_slide(&mut self, slide: &str) {
        let main = self.main.clone();
        let rid = self.pkg.rels(&main).into_iter().find(|r| r.kind == opc::RT_SLIDE && opc::resolve(&main, &r.target) == slide).map(|r| r.id);
        if let Some(rid) = rid {
            if let Some(list) = self.root_mut().child_mut("p:sldIdLst") { list.children.retain(|n| !matches!(n, Node::Element(e) if e.attr("r:id") == Some(rid.as_str()))); }
            self.pkg.remove_rel(&main, &rid);
        }
        if let Some(n) = self.notes_of(slide) { self.pkg.remove(&n); }
        self.pkg.remove(slide);
    }

    /// Move a slide to position `to` (in the order after removal).
    pub fn move_slide(&mut self, from: usize, to: usize) {
        let Some(list) = self.root_mut().child_mut("p:sldIdLst") else { return };
        let mut entries: Vec<Node> = list.children.drain(..).filter(|n| matches!(n, Node::Element(_))).collect();
        if from >= entries.len() { list.children = entries; return; }
        let e = entries.remove(from);
        entries.insert(to.min(entries.len()), e);
        list.children = entries;
    }

    pub fn set_hidden(&mut self, slide: &str, hidden: bool) {
        if let Some(s) = self.pkg.xml_mut(slide) { if hidden { s.set_attr("show", "0"); } else { s.remove_attr("show"); } }
    }

    /// Change a slide's layout (placeholders keep their text; they re-inherit position).
    pub fn set_layout(&mut self, slide: &str, layout: &str) {
        if let Some(old) = self.pkg.rels(slide).into_iter().find(|r| r.kind == opc::RT_LAYOUT) { self.pkg.remove_rel(slide, &old.id); }
        self.pkg.add_rel(slide, opc::RT_LAYOUT, layout, false);
    }

    // ---- shapes -----------------------------------------------------------------

    pub fn element(&self, loc: &Loc) -> Option<&Element> {
        let mut cur = self.pkg.xml(&loc.part)?.path(SP_TREE)?;
        for &i in &loc.path { cur = cur.elements().nth(i)?; }
        Some(cur)
    }
    pub fn element_mut(&mut self, loc: &Loc) -> Option<&mut Element> { el_at(self.pkg.xml_mut(&loc.part)?, &loc.path) }

    /// Set a shape's frame (written on the slide even when it was inherited).
    pub fn set_xfrm(&mut self, loc: &Loc, x: &Xfrm) {
        let Some(sp) = self.element_mut(loc) else { return };
        let xfrm = if sp.local() == "graphicFrame" { sp.ensure_at("p:xfrm", &["a:graphic"]) } else { sp_pr(sp).ensure_at("a:xfrm", &["a:custGeom", "a:prstGeom", "a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill", "a:ln", "a:effectLst", "a:effectDag", "a:scene3d", "a:sp3d", "a:extLst"]) };
        if x.rot.abs() > 1e-6 { xfrm.set_attr("rot", ((x.rot.rem_euclid(360.0)) * 60_000.0).round().to_string()); } else { xfrm.remove_attr("rot"); }
        if x.flip_h { xfrm.set_attr("flipH", "1"); } else { xfrm.remove_attr("flipH"); }
        if x.flip_v { xfrm.set_attr("flipV", "1"); } else { xfrm.remove_attr("flipV"); }
        let off = xfrm.ensure_at("a:off", &["a:ext", "a:chOff", "a:chExt"]);
        off.set_attr("x", (x.x.round() as i64).to_string());
        off.set_attr("y", (x.y.round() as i64).to_string());
        let ext = xfrm.ensure_at("a:ext", &["a:chOff", "a:chExt"]);
        ext.set_attr("cx", (x.w.round().max(0.0) as i64).to_string());
        ext.set_attr("cy", (x.h.round().max(0.0) as i64).to_string());
    }

    /// The frame as written (or inherited, via the scene) for a top-level shape.
    pub fn xfrm(&self, loc: &Loc) -> Option<Xfrm> {
        let sp = self.element(loc)?;
        let x = match sp.local() { "grpSp" => sp.path("p:grpSpPr/a:xfrm"), "graphicFrame" => sp.child("p:xfrm"), _ => sp.path("p:spPr/a:xfrm") };
        x.map(Xfrm::read)
    }

    /// Insert a shape element at the top of the z-order; returns its location.
    pub fn insert_element(&mut self, part: &str, sp: Element) -> Option<Loc> {
        let tree = self.pkg.xml_mut(part)?.path_mut(SP_TREE)?;
        // Before extLst, if any.
        let pos = tree.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "p:extLst")).unwrap_or(tree.children.len());
        tree.children.insert(pos, Node::Element(sp));
        let idx = tree.children[..=pos].iter().filter(|n| matches!(n, Node::Element(_))).count() - 1;
        Some(Loc { part: part.to_string(), path: vec![idx] })
    }

    /// A preset shape (or text box when `text_box`).
    pub fn insert_shape(&mut self, part: &str, preset: &str, x: &Xfrm, text_box: bool) -> Option<Loc> {
        let id = self.next_shape_id(part);
        let name = if text_box { format!("TextBox {}", id - 1) } else { format!("{} {}", preset_label(preset), id - 1) };
        let mut cnv = Element::new("p:cNvSpPr");
        if text_box { cnv.set_attr("txBox", "1"); }
        let mut sp_pr = Element::new("p:spPr").with_child(Element::new("a:prstGeom").with_attr("prst", preset).with_child(Element::new("a:avLst")));
        if text_box { sp_pr.children.push(Node::Element(Element::new("a:noFill"))); }
        let mut body_pr = Element::new("a:bodyPr").with_attr("rtlCol", "0").with_attr("anchor", if text_box { "t" } else { "ctr" });
        if text_box { body_pr.set_attr("wrap", "square"); body_pr.children.push(Node::Element(Element::new("a:spAutoFit"))); }
        let mut sp = Element::new("p:sp")
            .with_child(Element::new("p:nvSpPr").with_child(Element::new("p:cNvPr").with_attr("id", id.to_string()).with_attr("name", name)).with_child(cnv).with_child(Element::new("p:nvPr")))
            .with_child(sp_pr);
        if !text_box {
            // Theme style: accent fill, darker outline, light text (as PowerPoint inserts shapes).
            let r = |n: &str, idx: &str, clr: &str| Element::new(n).with_attr("idx", idx).with_child(Element::new("a:schemeClr").with_attr("val", clr));
            let mut lnref = r("a:lnRef", "2", "accent1");
            lnref.child_mut("a:schemeClr").unwrap().children.push(Node::Element(Element::new("a:shade").with_attr("val", "50000")));
            sp.children.push(Node::Element(Element::new("p:style").with_child(lnref).with_child(r("a:fillRef", "1", "accent1")).with_child(r("a:effectRef", "0", "accent1")).with_child(Element::new("a:fontRef").with_attr("idx", "minor").with_child(Element::new("a:schemeClr").with_attr("val", "lt1")))));
        }
        let mut p = Element::new("a:p");
        if !text_box { p.children.push(Node::Element(Element::new("a:pPr").with_attr("algn", "ctr"))); }
        sp.children.push(Node::Element(Element::new("p:txBody").with_child(body_pr).with_child(Element::new("a:lstStyle")).with_child(p)));
        let loc = self.insert_element(part, sp)?;
        self.set_xfrm(&loc, x);
        // Text in a theme-styled shape is light (fontRef lt1).
        if !text_box {
            if let Some(sp) = self.element_mut(&loc) {
                if let Some(p) = sp.path_mut("p:txBody/a:p") {
                    p.children.push(Node::Element(Element::new("a:endParaRPr").with_attr("lang", "en-US").with_child(Element::new("a:solidFill").with_child(Element::new("a:schemeClr").with_attr("val", "lt1")))));
                }
            }
        }
        Some(loc)
    }

    /// Insert a picture from image bytes (PNG, JPEG, GIF, BMP, SVG).
    pub fn insert_picture(&mut self, part: &str, bytes: Vec<u8>, ext: &str, x: &Xfrm, name: &str) -> Option<Loc> {
        let media = self.pkg.free_name("ppt/media/image", &format!(".{}", ext.to_ascii_lowercase()));
        self.pkg.put_bytes(&media, bytes);
        let rid = self.pkg.add_rel(part, opc::RT_IMAGE, &media, false);
        let id = self.next_shape_id(part);
        let pic = Element::new("p:pic")
            .with_child(Element::new("p:nvPicPr").with_child(Element::new("p:cNvPr").with_attr("id", id.to_string()).with_attr("name", format!("Picture {}", id - 1)).with_attr("descr", name)).with_child(Element::new("p:cNvPicPr").with_child(Element::new("a:picLocks").with_attr("noChangeAspect", "1"))).with_child(Element::new("p:nvPr")))
            .with_child(Element::new("p:blipFill").with_child(Element::new("a:blip").with_attr("r:embed", rid)).with_child(Element::new("a:stretch").with_child(Element::new("a:fillRect"))))
            .with_child(Element::new("p:spPr").with_child(Element::new("a:prstGeom").with_attr("prst", "rect").with_child(Element::new("a:avLst"))));
        let loc = self.insert_element(part, pic)?;
        self.set_xfrm(&loc, x);
        Some(loc)
    }

    /// Insert a table of rows × cols filling the frame.
    pub fn insert_table(&mut self, part: &str, rows: usize, cols: usize, x: &Xfrm) -> Option<Loc> {
        let id = self.next_shape_id(part);
        let cw = (x.w / cols.max(1) as f64).round() as i64;
        let rh = 370_840i64;
        let mut grid = Element::new("a:tblGrid");
        for _ in 0..cols { grid.children.push(Node::Element(Element::new("a:gridCol").with_attr("w", cw.to_string()))); }
        let mut tbl = Element::new("a:tbl").with_child(Element::new("a:tblPr").with_attr("firstRow", "1").with_attr("bandRow", "1").with_child(Element::new("a:tableStyleId").with_text("{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}"))).with_child(grid);
        for _ in 0..rows {
            let mut tr = Element::new("a:tr").with_attr("h", rh.to_string());
            for _ in 0..cols {
                tr.children.push(Node::Element(Element::new("a:tc").with_child(Element::new("a:txBody").with_child(Element::new("a:bodyPr")).with_child(Element::new("a:lstStyle")).with_child(Element::new("a:p").with_child(Element::new("a:endParaRPr").with_attr("lang", "en-US")))).with_child(Element::new("a:tcPr"))));
            }
            tbl.children.push(Node::Element(tr));
        }
        let frame = Element::new("p:graphicFrame")
            .with_child(Element::new("p:nvGraphicFramePr").with_child(Element::new("p:cNvPr").with_attr("id", id.to_string()).with_attr("name", format!("Table {}", id - 1))).with_child(Element::new("p:cNvGraphicFramePr").with_child(Element::new("a:graphicFrameLocks").with_attr("noGrp", "1"))).with_child(Element::new("p:nvPr")))
            .with_child(Element::new("p:xfrm"))
            .with_child(Element::new("a:graphic").with_child(Element::new("a:graphicData").with_attr("uri", "http://schemas.openxmlformats.org/drawingml/2006/table").with_child(tbl)));
        let loc = self.insert_element(part, frame)?;
        self.set_xfrm(&loc, &Xfrm { h: rh as f64 * rows as f64, ..*x });
        Some(loc)
    }

    /// Delete top-level shapes (locations on the same part; any order).
    pub fn delete_shapes(&mut self, locs: &[Loc]) {
        let mut by_part: std::collections::HashMap<String, Vec<usize>> = Default::default();
        for l in locs { if let Some(&i) = l.path.first() { by_part.entry(l.part.clone()).or_default().push(i); } }
        for (part, mut idx) in by_part {
            idx.sort_unstable();
            idx.dedup();
            let Some(tree) = self.pkg.xml_mut(&part).and_then(|r| r.path_mut(SP_TREE)) else { continue };
            let mut k = 0;
            let keep: Vec<bool> = tree.children.iter().map(|n| if let Node::Element(_) = n { let d = idx.contains(&k); k += 1; !d } else { true }).collect();
            let mut it = keep.iter();
            tree.children.retain(|_| *it.next().unwrap());
        }
    }

    /// Insert a copy of a shape element taken from `src_part` (same presentation) onto
    /// `part`: its relationships (pictures, links, charts) are re-pointed and its ids renewed.
    pub fn insert_copy(&mut self, part: &str, src_part: &str, el: &Element, dx: f64, dy: f64) -> Option<Loc> {
        let mut el = el.clone();
        if src_part != part {
            let rels = self.pkg.rels(src_part);
            let mut map: Vec<(String, String)> = vec![];
            fn collect(e: &Element, out: &mut Vec<String>) { for (k, v) in &e.attrs { if k.starts_with("r:") && !v.is_empty() { out.push(v.clone()); } } for c in e.elements() { collect(c, out); } }
            let mut ids = vec![];
            collect(&el, &mut ids);
            for id in ids {
                if map.iter().any(|m| m.0 == id) { continue; }
                let Some(r) = rels.iter().find(|r| r.id == id) else { continue };
                let new = if r.external { self.pkg.add_rel(part, &r.kind, &r.target, true) } else { let t = opc::resolve(src_part, &r.target); self.pkg.add_rel(part, &r.kind, &t, false) };
                map.push((id, new));
            }
            fn remap(e: &mut Element, map: &[(String, String)]) { for (k, v) in e.attrs.iter_mut() { if k.starts_with("r:") { if let Some((_, n)) = map.iter().find(|(o, _)| o == v) { *v = n.clone(); } } } for c in e.elements_mut() { remap(c, map); } }
            remap(&mut el, &map);
        }
        let mut next = self.next_shape_id(part);
        fn renumber(e: &mut Element, next: &mut u32) { if e.name == "p:cNvPr" { e.set_attr("id", next.to_string()); *next += 1; } for c in e.elements_mut() { renumber(c, next); } }
        renumber(&mut el, &mut next);
        let loc = self.insert_element(part, el)?;
        if dx != 0.0 || dy != 0.0 { if let Some(mut x) = self.xfrm(&loc) { x.x += dx; x.y += dy; self.set_xfrm(&loc, &x); } }
        Some(loc)
    }

    /// Duplicate a top-level shape, offset a little; returns the copy's location.
    pub fn duplicate_shape(&mut self, loc: &Loc, dx: f64, dy: f64) -> Option<Loc> {
        let mut sp = self.element(loc)?.clone();
        let id = self.next_shape_id(&loc.part);
        let mut ids: Vec<&Element> = vec![];
        let snapshot = sp.clone();
        snapshot.find_all("p:cNvPr", &mut ids);
        let n = ids.len() as u32;
        let mut k = 0;
        fn renumber(e: &mut Element, next: &mut u32) { if e.name == "p:cNvPr" { e.set_attr("id", next.to_string()); *next += 1; } for c in e.elements_mut() { renumber(c, next); } }
        let mut next = id;
        renumber(&mut sp, &mut next);
        k += n;
        let _ = k;
        let new = self.insert_element(&loc.part, sp)?;
        if let Some(mut x) = self.xfrm(&new) { x.x += dx; x.y += dy; self.set_xfrm(&new, &x); }
        Some(new)
    }

    /// Move a top-level shape in the z-order: to the front/back, or one step.
    pub fn reorder(&mut self, loc: &Loc, how: Order) -> Option<Loc> {
        let i = *loc.path.first()?;
        let tree = self.pkg.xml_mut(&loc.part)?.path_mut(SP_TREE)?;
        // Shapes are the element children after nvGrpSpPr and grpSpPr.
        let elems: Vec<usize> = tree.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(e) if !matches!(e.name.as_str(), "p:nvGrpSpPr" | "p:grpSpPr" | "p:extLst"))).map(|(k, _)| k).collect();
        let all: Vec<usize> = tree.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(_))).map(|(k, _)| k).collect();
        let raw = *all.get(i)?;
        let pos = elems.iter().position(|&k| k == raw)?;
        let target = match how { Order::Front => elems.len() - 1, Order::Back => 0, Order::Forward => (pos + 1).min(elems.len() - 1), Order::Backward => pos.saturating_sub(1) };
        if target == pos { return Some(loc.clone()); }
        let node = tree.children.remove(raw);
        let dest = elems[target];
        tree.children.insert(dest, node);
        let new_i = tree.children[..=dest].iter().filter(|n| matches!(n, Node::Element(_))).count() - 1;
        Some(Loc { part: loc.part.clone(), path: vec![new_i] })
    }

    /// Solid fill (hex), no fill (""), or back to the style (None).
    pub fn set_fill(&mut self, loc: &Loc, fill: Option<&str>) {
        let Some(sp) = self.element_mut(loc) else { return };
        let pr = sp_pr(sp);
        for n in ["a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill"] { pr.remove_children(n); }
        let Some(f) = fill else { return };
        let el = if f.is_empty() { Element::new("a:noFill") } else { Element::new("a:solidFill").with_child(Element::new("a:srgbClr").with_attr("val", f)) };
        let pos = pr.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "a:ln" | "a:effectLst" | "a:effectDag" | "a:scene3d" | "a:sp3d" | "a:extLst"))).unwrap_or(pr.children.len());
        pr.children.insert(pos, Node::Element(el));
    }

    /// Outline color (hex, "" for none) and width in points.
    pub fn set_line(&mut self, loc: &Loc, color: Option<&str>, width_pt: Option<f64>, dash: Option<&str>) {
        let Some(sp) = self.element_mut(loc) else { return };
        let ln = sp_pr(sp).ensure_at("a:ln", &["a:effectLst", "a:effectDag", "a:scene3d", "a:sp3d", "a:extLst"]);
        if let Some(c) = color {
            for n in ["a:noFill", "a:solidFill", "a:gradFill", "a:pattFill"] { ln.remove_children(n); }
            let el = if c.is_empty() { Element::new("a:noFill") } else { Element::new("a:solidFill").with_child(Element::new("a:srgbClr").with_attr("val", c)) };
            ln.children.insert(0, Node::Element(el));
        }
        if let Some(w) = width_pt { ln.set_attr("w", ((w * 12_700.0).round() as i64).to_string()); }
        if let Some(d) = dash {
            ln.remove_children("a:prstDash");
            let pos = ln.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "a:round" | "a:bevel" | "a:miter" | "a:headEnd" | "a:tailEnd" | "a:extLst"))).unwrap_or(ln.children.len());
            ln.children.insert(pos, Node::Element(Element::new("a:prstDash").with_attr("val", d)));
        }
    }

    pub fn set_arrows(&mut self, loc: &Loc, head: Option<&str>, tail: Option<&str>) {
        let Some(sp) = self.element_mut(loc) else { return };
        let ln = sp_pr(sp).ensure_at("a:ln", &["a:effectLst", "a:effectDag", "a:scene3d", "a:sp3d", "a:extLst"]);
        ln.remove_children("a:headEnd");
        ln.remove_children("a:tailEnd");
        if let Some(h) = head { ln.children.push(Node::Element(Element::new("a:headEnd").with_attr("type", h))); }
        if let Some(t) = tail { ln.children.push(Node::Element(Element::new("a:tailEnd").with_attr("type", t))); }
    }

    pub fn set_name(&mut self, loc: &Loc, name: &str) {
        if let Some(c) = self.element_mut(loc).and_then(|sp| sp.elements_mut().find(|e| e.local().starts_with("nv"))).and_then(|nv| nv.child_mut("p:cNvPr")) { c.set_attr("name", name); }
    }
    pub fn set_alt_text(&mut self, loc: &Loc, text: &str) {
        if let Some(c) = self.element_mut(loc).and_then(|sp| sp.elements_mut().find(|e| e.local().starts_with("nv"))).and_then(|nv| nv.child_mut("p:cNvPr")) { c.set_attr("descr", text); }
    }

    // ---- text ---------------------------------------------------------------------

    fn tx_body_mut(&mut self, loc: &Loc, cell: Option<(usize, usize)>) -> Option<&mut Element> {
        let sp = self.element_mut(loc)?;
        if let Some((r, c)) = cell {
            let tbl = sp.path_mut("a:graphic/a:graphicData/a:tbl")?;
            let tr = tbl.elements_mut().filter(|e| e.name == "a:tr").nth(r)?;
            let tc = tr.elements_mut().filter(|e| e.name == "a:tc").nth(c)?;
            return Some(tc.ensure_at("a:txBody", &["a:tcPr", "a:extLst"]));
        }
        if sp.local() != "sp" { return None; }
        if sp.child("p:txBody").is_none() {
            sp.children.push(Node::Element(Element::new("p:txBody").with_child(Element::new("a:bodyPr")).with_child(Element::new("a:lstStyle")).with_child(Element::new("a:p"))));
        }
        sp.child_mut("p:txBody")
    }

    /// The plain text of a shape (paragraphs joined by \n; line breaks as \u{b}).
    pub fn shape_text(&self, loc: &Loc, cell: Option<(usize, usize)>) -> String {
        let Some(sp) = self.element(loc) else { return String::new() };
        let body = match cell {
            Some((r, c)) => sp.path("a:graphic/a:graphicData/a:tbl").and_then(|t| t.children_named("a:tr").nth(r)).and_then(|tr| tr.children_named("a:tc").nth(c)).and_then(|tc| tc.child("a:txBody")),
            None => sp.child("p:txBody"),
        };
        let Some(body) = body else { return String::new() };
        body.children_named("a:p").map(|p| p.elements().map(|e| match e.local() { "r" | "fld" => e.child("a:t").map(|t| t.text()).unwrap_or_default(), "br" => "\u{b}".to_string(), _ => String::new() }).collect::<String>()).collect::<Vec<_>>().join("\n")
    }

    /// Replace a shape's text, keeping each paragraph's properties and its first run's
    /// formatting where the paragraph still exists.
    pub fn set_shape_text(&mut self, loc: &Loc, cell: Option<(usize, usize)>, text: &str) {
        let Some(body) = self.tx_body_mut(loc, cell) else { return };
        let old: Vec<Element> = body.children_named("a:p").cloned().collect();
        let last = old.last().cloned();
        body.remove_children("a:p");
        for (i, line) in text.split('\n').enumerate() {
            let src = old.get(i).or(last.as_ref());
            let ppr = src.and_then(|p| p.child("a:pPr"));
            let rpr = src.and_then(|p| p.child("a:r").and_then(|r| r.child("a:rPr")).or_else(|| p.child("a:endParaRPr")).map(|e| { let mut e = e.clone(); e.name = "a:rPr".into(); e }));
            let mut p = Element::new("a:p");
            if let Some(ppr) = ppr { p.children.push(Node::Element(ppr.clone())); }
            for (k, piece) in line.split('\u{b}').enumerate() {
                if k > 0 { let mut br = Element::new("a:br"); if let Some(r) = &rpr { br.children.push(Node::Element(r.clone())); } p.children.push(Node::Element(br)); }
                if !piece.is_empty() {
                    let mut r = Element::new("a:r");
                    if let Some(rpr) = &rpr { r.children.push(Node::Element(rpr.clone())); }
                    r.children.push(Node::Element(Element::new("a:t").with_text(piece)));
                    p.children.push(Node::Element(r));
                }
            }
            if let Some(rpr) = &rpr { let mut end = rpr.clone(); end.name = "a:endParaRPr".into(); end.children.retain(|c| !matches!(c, Node::Element(e) if e.name == "a:hlinkClick")); p.children.push(Node::Element(end)); }
            body.children.push(Node::Element(p));
        }
        let _ = new_paragraph;
    }

    /// Insert text at a character position (paragraphs joined by \n, as in `shape_text`);
    /// "\n" starts a new paragraph. New text takes the formatting of the text before it.
    pub fn insert_text(&mut self, loc: &Loc, cell: Option<(usize, usize)>, pos: usize, text: &str) {
        let Some(body) = self.tx_body_mut(loc, cell) else { return };
        let mut pos = pos;
        for (k, piece) in text.split('\n').enumerate() {
            if k > 0 { split_paragraph(body, pos); pos += 1; }
            if !piece.is_empty() { insert_in_paragraphs(body, pos, piece); pos += piece.chars().count(); }
        }
    }

    /// Delete the characters [a, b) (a paragraph break counts as one character).
    pub fn delete_text(&mut self, loc: &Loc, cell: Option<(usize, usize)>, a: usize, b: usize) {
        if a >= b { return; }
        let Some(body) = self.tx_body_mut(loc, cell) else { return };
        let paras: Vec<usize> = body.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(e) if e.name == "a:p")).map(|(i, _)| i).collect();
        // Paragraph spans in characters.
        let mut spans = vec![];
        let mut off = 0;
        for &i in &paras { let Node::Element(p) = &body.children[i] else { continue }; let n = para_len(p); spans.push((off, off + n)); off += n + 1; }
        let Some(pa) = spans.iter().position(|&(s, e)| a >= s && a <= e) else { return };
        let pb = spans.iter().position(|&(s, e)| b >= s && b <= e).unwrap_or(spans.len() - 1);
        if pa == pb {
            if let Node::Element(p) = &mut body.children[paras[pa]] { cut_runs(p, a - spans[pa].0, b - spans[pa].0); }
            return;
        }
        // Across paragraphs: keep the head of the first and the tail of the last, joined.
        let tail: Vec<Node> = if let Node::Element(p) = &mut body.children[paras[pb]] { cut_runs(p, 0, b - spans[pb].0); p.children.iter().filter(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "a:r" | "a:br" | "a:fld"))).cloned().collect() } else { vec![] };
        if let Node::Element(p) = &mut body.children[paras[pa]] {
            let len = para_len(p);
            cut_runs(p, a - spans[pa].0, len);
            let at = p.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "a:endParaRPr")).unwrap_or(p.children.len());
            for (k, n) in tail.into_iter().enumerate() { p.children.insert(at + k, n); }
        }
        for &i in paras[pa + 1..=pb].iter().rev() { body.children.remove(i); }
    }

    /// Apply run formatting to the whole text of a shape (or table cell).
    pub fn format_shape_text(&mut self, loc: &Loc, cell: Option<(usize, usize)>, f: &RunFormat) {
        let Some(body) = self.tx_body_mut(loc, cell) else { return };
        for p in body.elements_mut().filter(|e| e.name == "a:p") {
            for r in p.elements_mut() {
                match r.name.as_str() {
                    "a:r" | "a:fld" | "a:br" => { let rpr = r.ensure_at("a:rPr", &["a:t"]); rpr_apply(rpr, f); }
                    "a:endParaRPr" => rpr_apply(r, f),
                    _ => {}
                }
            }
            if p.child("a:endParaRPr").is_none() { let mut end = Element::new("a:endParaRPr"); rpr_apply(&mut end, f); p.children.push(Node::Element(end)); }
        }
    }

    /// Apply run formatting to a character range [start, end) of the shape's text (the
    /// same indexing as `shape_text`, paragraphs joined by \n). Runs are split as needed.
    pub fn format_range(&mut self, loc: &Loc, cell: Option<(usize, usize)>, start: usize, end: usize, f: &RunFormat) {
        if start >= end { return; }
        let Some(body) = self.tx_body_mut(loc, cell) else { return };
        let mut pos = 0usize;
        for p in body.elements_mut().filter(|e| e.name == "a:p") {
            let mut out: Vec<Node> = vec![];
            for n in p.children.drain(..) {
                let Node::Element(r) = n else { out.push(n); continue };
                let len = match r.name.as_str() { "a:r" | "a:fld" => r.child("a:t").map(|t| t.text().chars().count()).unwrap_or(0), "a:br" => 1, _ => { out.push(Node::Element(r)); continue } };
                let (a, b) = (pos, pos + len);
                pos = b;
                if b <= start || a >= end || len == 0 || r.name != "a:r" {
                    let mut r = r;
                    if r.name == "a:br" && a >= start && b <= end { rpr_apply(r.ensure_at("a:rPr", &[]), f); }
                    out.push(Node::Element(r));
                    continue;
                }
                let text: Vec<char> = r.child("a:t").map(|t| t.text()).unwrap_or_default().chars().collect();
                let cut = [start.saturating_sub(a).min(len), end.saturating_sub(a).min(len)];
                for (k, (s, e)) in [(0, cut[0]), (cut[0], cut[1]), (cut[1], len)].into_iter().enumerate() {
                    if s >= e { continue; }
                    let mut piece = r.clone();
                    piece.child_mut("a:t").unwrap().set_text(text[s..e].iter().collect::<String>());
                    if k == 1 { rpr_apply(piece.ensure_at("a:rPr", &["a:t"]), f); }
                    out.push(Node::Element(piece));
                }
            }
            p.children = out;
            pos += 1; // the paragraph break
        }
    }

    /// Paragraph alignment ("l", "ctr", "r", "just") for all paragraphs, or a range of them.
    pub fn set_align(&mut self, loc: &Loc, cell: Option<(usize, usize)>, paras: Option<(usize, usize)>, algn: &str) {
        self.with_paragraphs(loc, cell, paras, |ppr| ppr.set_attr("algn", algn));
    }
    pub fn set_level(&mut self, loc: &Loc, cell: Option<(usize, usize)>, paras: Option<(usize, usize)>, delta: i32) {
        self.with_paragraphs(loc, cell, paras, |ppr| {
            let l = (ppr.attr_i64("lvl").unwrap_or(0) as i32 + delta).clamp(0, 8);
            if l == 0 { ppr.remove_attr("lvl"); } else { ppr.set_attr("lvl", l.to_string()); }
        });
    }
    /// Bullets: "none", "char:•", or an autonumber scheme ("arabicPeriod").
    pub fn set_bullets(&mut self, loc: &Loc, cell: Option<(usize, usize)>, paras: Option<(usize, usize)>, kind: &str) {
        self.with_paragraphs(loc, cell, paras, |ppr| {
            for n in ["a:buNone", "a:buChar", "a:buAutoNum", "a:buBlip", "a:buFont"] { ppr.remove_children(n); }
            let before = ["a:tabLst", "a:defRPr", "a:extLst"];
            if kind == "none" { ppr.ensure_at("a:buNone", &before); }
            else if let Some(ch) = kind.strip_prefix("char:") {
                ppr.ensure_at("a:buFont", &["a:buNone", "a:buAutoNum", "a:buChar", "a:buBlip", "a:tabLst", "a:defRPr", "a:extLst"]).set_attr("typeface", "Arial");
                ppr.ensure_at("a:buChar", &before).set_attr("char", ch);
                if ppr.attr("marL").is_none() { ppr.set_attr("marL", "342900"); ppr.set_attr("indent", "-342900"); }
            } else {
                ppr.ensure_at("a:buAutoNum", &before).set_attr("type", kind);
                if ppr.attr("marL").is_none() { ppr.set_attr("marL", "457200"); ppr.set_attr("indent", "-457200"); }
            }
        });
    }
    /// Line spacing as a multiple (1.0, 1.5, 2.0).
    pub fn set_line_spacing(&mut self, loc: &Loc, cell: Option<(usize, usize)>, paras: Option<(usize, usize)>, multiple: f64) {
        self.with_paragraphs(loc, cell, paras, |ppr| {
            ppr.remove_children("a:lnSpc");
            let ln = Element::new("a:lnSpc").with_child(Element::new("a:spcPct").with_attr("val", ((multiple * 100_000.0).round() as i64).to_string()));
            ppr.children.insert(0, Node::Element(ln));
        });
    }

    fn with_paragraphs(&mut self, loc: &Loc, cell: Option<(usize, usize)>, paras: Option<(usize, usize)>, mut f: impl FnMut(&mut Element)) {
        let Some(body) = self.tx_body_mut(loc, cell) else { return };
        for (i, p) in body.elements_mut().filter(|e| e.name == "a:p").enumerate() {
            if let Some((a, b)) = paras { if i < a || i > b { continue; } }
            let ppr = if p.child("a:pPr").is_some() { p.child_mut("a:pPr").unwrap() } else { p.children.insert(0, Node::Element(Element::new("a:pPr"))); p.child_mut("a:pPr").unwrap() };
            f(ppr);
        }
    }

    /// Vertical anchor of the text ("t", "ctr", "b").
    pub fn set_anchor(&mut self, loc: &Loc, anchor: &str) {
        if let Some(b) = self.tx_body_mut(loc, None) { b.ensure_at("a:bodyPr", &["a:lstStyle", "a:p"]).set_attr("anchor", anchor); }
    }

    // ---- notes and background -------------------------------------------------------

    /// Set the speaker notes, creating the notes slide (and notes master) when needed.
    pub fn set_notes(&mut self, slide: &str, text: &str) {
        let notes = match self.notes_of(slide) { Some(n) => n, None => { if text.is_empty() { return; } self.create_notes(slide) } };
        let Some(tree) = self.pkg.xml_mut(&notes).and_then(|n| n.path_mut(SP_TREE)) else { return };
        let body = tree.elements_mut().find(|sp| sp.path("p:nvSpPr/p:nvPr/p:ph").and_then(|p| p.attr("type")) == Some("body"));
        let Some(sp) = body else { return };
        let tx = sp.ensure("p:txBody");
        if tx.child("a:bodyPr").is_none() { tx.children.insert(0, Node::Element(Element::new("a:bodyPr"))); }
        tx.remove_children("a:p");
        for line in text.split('\n') { tx.children.push(Node::Element(new_paragraph(line, None, None))); }
    }

    fn create_notes(&mut self, slide: &str) -> String {
        let main = self.main.clone();
        let master = match self.pkg.related(&main, opc::RT_NOTES_MASTER) {
            Some(m) => m,
            None => {
                let m = "ppt/notesMasters/notesMaster1.xml".to_string();
                let theme = self.pkg.free_name("ppt/theme/theme", ".xml");
                let main_theme = self.masters().first().and_then(|ms| self.theme_part(ms));
                if let Some(t) = main_theme.and_then(|t| self.pkg.xml(&t).cloned()) { self.pkg.put_xml(&theme, t, Some("application/vnd.openxmlformats-officedocument.theme+xml")); }
                let root = Element::parse(&format!(r#"<p:notesMaster xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id="2" name="Notes Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="685800" y="4400550"/><a:ext cx="5486400" cy="3600450"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:notesMaster>"#)).unwrap();
                self.pkg.put_xml(&m, root, Some("application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"));
                if self.pkg.has(&theme) { self.pkg.add_rel(&m, opc::RT_THEME, &theme, false); }
                let rid = self.pkg.add_rel(&main, opc::RT_NOTES_MASTER, &m, false);
                let lst = self.root_mut().ensure_at("p:notesMasterIdLst", &["p:handoutMasterIdLst", "p:sldIdLst", "p:sldSz", "p:notesSz"]);
                lst.children = vec![Node::Element(Element::new("p:notesMasterId").with_attr("r:id", rid))];
                m
            }
        };
        let notes = self.pkg.free_name("ppt/notesSlides/notesSlide", ".xml");
        let root = Element::parse(&format!(r#"<p:notes xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#)).unwrap();
        self.pkg.put_xml(&notes, root, Some(opc::CT_NOTES));
        self.pkg.add_rel(&notes, opc::RT_NOTES_MASTER, &master, false);
        self.pkg.add_rel(&notes, opc::RT_SLIDE, slide, false);
        self.pkg.add_rel(slide, opc::RT_NOTES, &notes, false);
        notes
    }

    /// A solid background color for a slide (hex), or back to the layout's (None).
    pub fn set_background(&mut self, slide: &str, color: Option<&str>) {
        let Some(csld) = self.pkg.xml_mut(slide).and_then(|s| s.child_mut("p:cSld")) else { return };
        csld.remove_children("p:bg");
        if let Some(c) = color {
            let bg = Element::new("p:bg").with_child(Element::new("p:bgPr").with_child(Element::new("a:solidFill").with_child(Element::new("a:srgbClr").with_attr("val", c))).with_child(Element::new("a:effectLst")));
            csld.children.insert(0, Node::Element(bg));
        }
    }

    /// The slide's transition effect ("fade", "push", …), if any.
    pub fn transition(&self, slide: &str) -> Option<String> {
        let root = self.pkg.xml(slide)?;
        let t = root.child("p:transition").or_else(|| root.children_named("mc:AlternateContent").find_map(|ac| ac.child("mc:Fallback").and_then(|f| f.child("p:transition")).or_else(|| ac.child("mc:Choice").and_then(|c| c.child("p:transition")))))?;
        Some(t.first_element().map(|e| e.local().to_string()).unwrap_or_else(|| "fade".into()))
    }

    /// Set (or remove) the slide's transition.
    pub fn set_transition(&mut self, slide: &str, effect: Option<&str>) {
        let Some(root) = self.pkg.xml_mut(slide) else { return };
        root.remove_children("p:transition");
        root.children.retain(|n| !matches!(n, Node::Element(e) if e.name == "mc:AlternateContent" && e.find("p:transition").is_some()));
        let Some(effect) = effect else { return };
        let mut t = Element::new("p:transition").with_attr("spd", "med");
        let mut fx = Element::new(&format!("p:{effect}"));
        if effect == "push" || effect == "cover" { fx.set_attr("dir", "l"); }
        t.children.push(Node::Element(fx));
        let pos = root.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "p:timing" | "p:extLst"))).unwrap_or(root.children.len());
        root.children.insert(pos, Node::Element(t));
    }

    /// Replace every occurrence of `find` in the slides' text; returns how many.
    pub fn replace_all(&mut self, find: &str, with: &str, match_case: bool) -> usize {
        if find.is_empty() { return 0; }
        let mut n = 0;
        for s in self.slides() {
            let Some(root) = self.pkg.xml_mut(&s) else { continue };
            // Matches may span runs: work on each paragraph's text, then give each run back
            // its share (a replacement lands in the run where its match starts).
            fn walk(e: &mut Element, find: &str, with: &str, mc: bool, n: &mut usize) {
                if e.name == "a:p" {
                    let runs: Vec<usize> = e.children.iter().enumerate().filter(|(_, c)| matches!(c, Node::Element(r) if r.name == "a:r")).map(|(i, _)| i).collect();
                    let texts: Vec<Vec<char>> = runs.iter().map(|&i| if let Node::Element(r) = &e.children[i] { r.child("a:t").map(|t| t.text()).unwrap_or_default().chars().collect() } else { vec![] }).collect();
                    let all: Vec<char> = texts.iter().flatten().copied().collect();
                    let norm = |c: &char| if mc { *c } else { c.to_lowercase().next().unwrap_or(*c) };
                    let hay: Vec<char> = all.iter().map(norm).collect();
                    let needle: Vec<char> = find.chars().map(|c| norm(&c)).collect();
                    let mut owner = vec![0usize; all.len()];
                    let mut k = 0;
                    for (ri, t) in texts.iter().enumerate() { for _ in t { owner[k] = ri; k += 1; } }
                    let mut out: Vec<String> = vec![String::new(); texts.len()];
                    let mut i = 0;
                    let mut found = false;
                    while i < all.len() {
                        if i + needle.len() <= all.len() && hay[i..i + needle.len()] == needle[..] {
                            out[owner[i]].push_str(with);
                            i += needle.len();
                            *n += 1;
                            found = true;
                        } else { out[owner[i]].push(all[i]); i += 1; }
                    }
                    if found {
                        for (ri, &ci) in runs.iter().enumerate() {
                            if let Node::Element(r) = &mut e.children[ci] { if let Some(t) = r.child_mut("a:t") { t.set_text(out[ri].clone()); } }
                        }
                    }
                    return;
                }
                for c in e.elements_mut() { walk(c, find, with, mc, n); }
            }
            walk(root, find, with, match_case, &mut n);
        }
        n
    }
}

fn atom_len(e: &Element) -> usize {
    match e.name.as_str() { "a:r" | "a:fld" => e.child("a:t").map(|t| t.text().chars().count()).unwrap_or(0), "a:br" => 1, _ => 0 }
}
fn para_len(p: &Element) -> usize { p.elements().map(atom_len).sum() }

/// Remove characters [a, b) of one paragraph; fields go whole when touched.
fn cut_runs(p: &mut Element, a: usize, b: usize) {
    let mut pos = 0;
    let mut first_rpr: Option<Element> = None;
    let mut out = vec![];
    for n in p.children.drain(..) {
        let Node::Element(mut e) = n else { out.push(n); continue };
        let len = atom_len(&e);
        let (s, t) = (pos, pos + len);
        pos = t;
        if len == 0 || t <= a || s >= b { out.push(Node::Element(e)); continue; }
        if first_rpr.is_none() { first_rpr = e.child("a:rPr").cloned(); }
        if e.name == "a:r" {
            let chars: Vec<char> = e.child("a:t").map(|x| x.text()).unwrap_or_default().chars().collect();
            let keep: String = chars.iter().enumerate().filter(|(i, _)| s + i < a || s + i >= b).map(|(_, c)| *c).collect();
            if !keep.is_empty() { e.child_mut("a:t").unwrap().set_text(keep); out.push(Node::Element(e)); }
        }
    }
    p.children = out;
    // An emptied paragraph keeps the formatting for what's typed next.
    if para_len(p) == 0 && p.child("a:endParaRPr").is_none() {
        if let Some(mut r) = first_rpr { r.name = "a:endParaRPr".into(); p.children.push(Node::Element(r)); }
    }
}

fn insert_in_paragraphs(body: &mut Element, pos: usize, text: &str) {
    let mut off = 0;
    let count = body.children_named("a:p").count();
    for (pi, p) in body.elements_mut().filter(|e| e.name == "a:p").enumerate() {
        let n = para_len(p);
        if pos <= off + n || pi + 1 == count {
            let local = pos.saturating_sub(off).min(n);
            insert_in_paragraph(p, local, text);
            return;
        }
        off += n + 1;
    }
}

fn insert_in_paragraph(p: &mut Element, at: usize, text: &str) {
    // The run ending at (or containing) the position takes the text.
    let mut pos = 0;
    let mut target: Option<(usize, usize)> = None; // (child index, offset in run)
    let mut after_field: Option<usize> = None;
    for (i, n) in p.children.iter().enumerate() {
        let Node::Element(e) = n else { continue };
        let len = atom_len(e);
        if e.name == "a:r" && at >= pos && at <= pos + len { target = Some((i, at - pos)); if at < pos + len || at == pos + len { break; } }
        if e.name != "a:r" && len > 0 && at == pos + len { after_field = Some(i); }
        pos += len;
    }
    if let Some((i, k)) = target {
        if let Node::Element(r) = &mut p.children[i] {
            let mut chars: Vec<char> = r.child("a:t").map(|t| t.text()).unwrap_or_default().chars().collect();
            for (j, c) in text.chars().enumerate() { chars.insert(k + j, c); }
            r.ensure("a:t").set_text(chars.into_iter().collect::<String>());
        }
        return;
    }
    // No run there: a new run, formatted like the paragraph end (or the run before).
    let rpr = p.child("a:endParaRPr").map(|e| { let mut e = e.clone(); e.name = "a:rPr".into(); e })
        .or_else(|| p.elements().filter(|e| e.name == "a:r").last().and_then(|r| r.child("a:rPr").cloned()));
    let mut r = Element::new("a:r");
    if let Some(rpr) = rpr { r.children.push(Node::Element(rpr)); }
    r.children.push(Node::Element(Element::new("a:t").with_text(text)));
    let idx = match after_field {
        Some(i) => i + 1,
        None => {
            // Before the atom at `at`, else before endParaRPr.
            let mut pos = 0;
            let mut found = None;
            for (i, n) in p.children.iter().enumerate() { if let Node::Element(e) = n { let len = atom_len(e); if len > 0 && pos >= at { found = Some(i); break; } pos += len; } }
            found.unwrap_or_else(|| p.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "a:endParaRPr")).unwrap_or(p.children.len()))
        }
    };
    p.children.insert(idx, Node::Element(r));
}

/// Split the paragraph containing `pos` there (Enter).
fn split_paragraph(body: &mut Element, pos: usize) {
    let idx: Vec<usize> = body.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(e) if e.name == "a:p")).map(|(i, _)| i).collect();
    let mut off = 0;
    for (k, &i) in idx.iter().enumerate() {
        let Node::Element(p) = &body.children[i] else { continue };
        let n = para_len(p);
        if pos <= off + n || k + 1 == idx.len() {
            let local = pos.saturating_sub(off).min(n);
            let mut head = p.clone();
            let mut tail = p.clone();
            cut_runs(&mut head, local, n);
            cut_runs(&mut tail, 0, local);
            // The new paragraph keeps the formatting at the split for typing.
            if para_len(&tail) == 0 && tail.child("a:endParaRPr").is_none() {
                if let Some(r) = p.elements().filter(|e| e.name == "a:r").last().and_then(|r| r.child("a:rPr")) { let mut r = r.clone(); r.name = "a:endParaRPr".into(); tail.children.push(Node::Element(r)); }
            }
            body.children[i] = Node::Element(head);
            body.children.insert(i + 1, Node::Element(tail));
            return;
        }
        off += n + 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order { Front, Back, Forward, Backward }

/// A readable name for a preset ("roundRect" → "Rounded Rectangle").
pub fn preset_label(p: &str) -> String {
    match p {
        "rect" => "Rectangle".into(), "roundRect" => "Rounded Rectangle".into(), "ellipse" => "Oval".into(), "triangle" => "Isosceles Triangle".into(),
        "rtTriangle" => "Right Triangle".into(), "diamond" => "Diamond".into(), "line" => "Straight Connector".into(), "rightArrow" => "Right Arrow".into(),
        other => {
            let mut s = String::new();
            for (i, c) in other.chars().enumerate() { if i == 0 { s.extend(c.to_uppercase()); } else if c.is_uppercase() { s.push(' '); s.push(c); } else { s.push(c); } }
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Kind;

    #[test]
    fn build_a_deck() {
        let mut p = Presentation::new_blank(true);
        let layouts = p.all_layouts();
        let title_layout = layouts.iter().find(|l| p.layout_type(l) == "title").unwrap().clone();
        let s1 = p.add_slide(&title_layout, 0);
        assert_eq!(p.slides(), vec![s1.clone()]);
        let scene = p.scene(&s1);
        let title = scene.items.iter().find(|i| i.placeholder.as_ref().is_some_and(|p| p.0 == "ctrTitle")).unwrap();
        p.set_shape_text(&title.loc, None, "Hello\nWorld");
        assert_eq!(p.shape_text(&title.loc, None), "Hello\nWorld");
        p.format_shape_text(&title.loc, None, &RunFormat { bold: Some(true), color: Some("FF0000".into()), ..Default::default() });
        let scene = p.scene(&s1);
        let t = scene.items.iter().find(|i| i.loc == title.loc).unwrap();
        let Kind::Shape { text: Some(tb), .. } = &t.kind else { panic!() };
        assert!(tb.paras[0].runs[0].bold);
        assert_eq!(tb.paras[1].runs[0].color, crate::Rgba::rgb(255, 0, 0));
        // Partial formatting splits runs.
        p.format_range(&title.loc, None, 1, 3, &RunFormat { italic: Some(true), ..Default::default() });
        let scene = p.scene(&s1);
        let Kind::Shape { text: Some(tb), .. } = &scene.items.iter().find(|i| i.loc == title.loc).unwrap().kind else { panic!() };
        assert_eq!(tb.paras[0].runs.iter().map(|r| (r.text.as_str(), r.italic)).collect::<Vec<_>>(), vec![("H", false), ("el", true), ("lo", false)]);

        // Shapes, pictures, tables.
        let x = Xfrm { x: 914_400.0, y: 914_400.0, w: 1_828_800.0, h: 914_400.0, ..Default::default() };
        let shape = p.insert_shape(&s1, "roundRect", &x, false).unwrap();
        p.set_shape_text(&shape, None, "Box");
        let png = { let mut v = vec![]; v.extend_from_slice(b"\x89PNG\r\n\x1a\n"); v };
        let pic = p.insert_picture(&s1, png, "png", &x, "test").unwrap();
        let tbl = p.insert_table(&s1, 2, 3, &x).unwrap();
        let scene = p.scene(&s1);
        assert!(scene.items.iter().any(|i| i.loc == pic && matches!(i.kind, Kind::Picture { .. })));
        assert!(scene.items.iter().any(|i| i.loc == tbl && matches!(&i.kind, Kind::Table(t) if t.cols.len() == 3 && t.rows.len() == 2)));
        let moved = p.reorder(&shape, Order::Front).unwrap();
        assert_eq!(p.shape_text(&moved, None), "Box");
        p.delete_shapes(&[moved]);

        // Slides: duplicate, move, delete; notes.
        let s2 = p.duplicate_slide(&s1).unwrap();
        assert_eq!(p.slides(), vec![s1.clone(), s2.clone()]);
        p.move_slide(1, 0);
        assert_eq!(p.slides(), vec![s2.clone(), s1.clone()]);
        p.set_notes(&s1, "Speaker notes\nline two");
        assert_eq!(p.notes_text(&s1), "Speaker notes\nline two");
        p.delete_slide(&s2);
        assert_eq!(p.slides(), vec![s1.clone()]);
        assert_eq!(p.replace_all("hello", "Hi", false), 1);

        // Typing keeps formatting; Enter splits; deleting across paragraphs merges.
        let t2 = p.scene(&s1).items.iter().find(|i| i.placeholder.as_ref().is_some_and(|p| p.0 == "ctrTitle")).unwrap().loc.clone();
        p.set_shape_text(&t2, None, "Hi World");
        p.insert_text(&t2, None, 2, " there");
        assert_eq!(p.shape_text(&t2, None), "Hi there World");
        p.insert_text(&t2, None, 8, "\n");
        assert_eq!(p.shape_text(&t2, None), "Hi there\n World");
        p.delete_text(&t2, None, 8, 10);
        assert_eq!(p.shape_text(&t2, None), "Hi thereWorld");
        p.delete_text(&t2, None, 0, 13);
        assert_eq!(p.shape_text(&t2, None), "");
        p.insert_text(&t2, None, 0, "Hi World");

        // Everything survives saving and reopening.
        let again = Presentation::open(&p.save().unwrap()).unwrap();
        let s = again.slides();
        assert_eq!(s.len(), 1);
        assert_eq!(again.notes_text(&s[0]), "Speaker notes\nline two");
        assert_eq!(again.slide_title(&s[0]), "Hi World");
    }
}
