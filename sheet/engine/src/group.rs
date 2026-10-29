//! Grouped objects (`xdr:grpSp`): a group is one floating object whose children sit in its own
//! coordinate space (chOff/chExt), so moving, resizing or rotating the group moves them all.
//! Also the rotation and flips any object can have (`a:xfrm rot/flipH/flipV`) and the
//! Selection Pane's hidden flag: reading them, writing them into generated and carried XML,
//! and the geometry the screen and print renderers share.

use crate::book::{Sheet, Workbook};
use crate::drawing::{Anchor, ObjKind, Object, Rel, EMU};
use crate::xlsx::xml::{attr, Element, Node};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Group {
    /// The members, each anchored `Anchor::Abs` in the group's child space.
    pub children: Vec<Object>,
    /// The child space: x, y, width, height (chOff, chExt) in pixels.
    pub ch: [f32; 4],
}

fn abs_rect(o: &Object) -> [f32; 4] {
    match o.anchor { Anchor::Abs { x, y, w, h } => [x, y, x + w, y + h], _ => [0.0, 0.0, 0.0, 0.0] }
}

impl Group {
    /// Where a member lands when the group fills `outer` (before the group's own rotation).
    pub fn child_rect(&self, c: &Object, outer: [f32; 4]) -> [f32; 4] {
        let r = abs_rect(c);
        let sx = if self.ch[2] > 0.0 { (outer[2] - outer[0]) / self.ch[2] } else { 1.0 };
        let sy = if self.ch[3] > 0.0 { (outer[3] - outer[1]) / self.ch[3] } else { 1.0 };
        [outer[0] + (r[0] - self.ch[0]) * sx, outer[1] + (r[1] - self.ch[1]) * sy, outer[0] + (r[2] - self.ch[0]) * sx, outer[1] + (r[3] - self.ch[1]) * sy]
    }
}

/// A point turned `deg` clockwise about `c`, after mirroring for the flips.
pub fn turn(p: (f32, f32), c: (f32, f32), deg: f32, fh: bool, fv: bool) -> (f32, f32) {
    let (mut x, mut y) = (p.0 - c.0, p.1 - c.1);
    if fh { x = -x; }
    if fv { y = -y; }
    let (s, co) = deg.to_radians().sin_cos();
    (c.0 + x * co - y * s, c.1 + x * s + y * co)
}

/// Groups objects placed at the given sheet rectangles; returns the group and its rectangle.
pub fn make(items: Vec<(Object, [f32; 4])>) -> (Group, [f32; 4]) {
    let mut bb = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (_, r) in &items { bb = [bb[0].min(r[0]), bb[1].min(r[1]), bb[2].max(r[2]), bb[3].max(r[3])]; }
    let children = items.into_iter().map(|(mut o, r)| { o.anchor = Anchor::Abs { x: r[0], y: r[1], w: r[2] - r[0], h: r[3] - r[1] }; o.touch(); o }).collect();
    (Group { children, ch: [bb[0], bb[1], bb[2] - bb[0], bb[3] - bb[1]] }, bb)
}

/// Ungroup: the members with their sheet rectangles (the group's rotation and flips passed on).
pub fn ungroup(o: &Object, outer: [f32; 4]) -> Vec<(Object, [f32; 4])> {
    let ObjKind::Group(g) = &o.kind else { return vec![] };
    let c = ((outer[0] + outer[2]) / 2.0, (outer[1] + outer[3]) / 2.0);
    g.children.iter().map(|k| {
        let r = g.child_rect(k, outer);
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let m = turn(((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0), c, o.rot, o.flip_h, o.flip_v);
        let mut k = k.clone();
        k.rot = (k.rot + o.rot).rem_euclid(360.0);
        k.flip_h ^= o.flip_h; k.flip_v ^= o.flip_v;
        if o.flip_h != o.flip_v && k.rot != 0.0 { k.rot = (360.0 - k.rot).rem_euclid(360.0); }
        (k, [m.0 - w / 2.0, m.1 - h / 2.0, m.0 + w / 2.0, m.1 + h / 2.0])
    }).collect()
}

/// An object's rectangle in sheet pixels, from the sheet's column widths and row heights.
pub fn sheet_rect(s: &Sheet, o: &Object) -> [f32; 4] {
    let cp = |c: u32| (0..c).map(|i| s.col_width(i)).sum::<f32>();
    let rp = |r: u32| (0..r).map(|i| s.row_height(i)).sum::<f32>();
    o.anchor.rect(cp, rp)
}

// ---- reading ------------------------------------------------------------------------------------

/// The xfrm of a drawing element (sp, pic, cxnSp, graphicFrame, grpSp).
pub fn xfrm(e: &Element) -> Option<&Element> {
    match e.local() { "grpSp" => e.path("grpSpPr/xfrm"), "graphicFrame" => e.child("xfrm"), _ => e.path("spPr/xfrm") }
}

/// Rotation (degrees), flips and hidden flag of a drawing element.
pub fn read_transform(e: &Element) -> (f32, bool, bool, bool) {
    let x = xfrm(e);
    let rot = x.and_then(|x| x.attr_f64("rot")).map(|r| (r / 60000.0) as f32).unwrap_or(0.0);
    let hidden = e.elements().find(|c| c.local().starts_with("nv")).and_then(|nv| nv.child("cNvPr")).is_some_and(|c| c.flag("hidden", false));
    (rot, x.is_some_and(|x| x.flag("flipH", false)), x.is_some_and(|x| x.flag("flipV", false)), hidden)
}

fn emu_attr(e: Option<&Element>, k: &str) -> f32 { e.and_then(|e| e.attr_f64(k)).unwrap_or(0.0) as f32 / EMU }

fn used(e: &Element, rels: &[Rel]) -> Vec<Rel> {
    let ids: std::collections::HashSet<&str> = rels.iter().map(|r| r.id.as_str()).collect();
    let mut out = vec![];
    crate::drawing::used_ids(e, &ids, &mut out);
    rels.iter().filter(|r| out.contains(&r.id)).cloned().collect()
}

/// A grpSp element as a group.
pub fn read(wb: &Workbook, e: &Element, rels: &[Rel], ns: &Arc<Vec<(String, String)>>) -> Group {
    let x = e.path("grpSpPr/xfrm");
    let (cx, cy) = (emu_attr(x.and_then(|x| x.child("chOff")), "x"), emu_attr(x.and_then(|x| x.child("chOff")), "y"));
    let (cw, chh) = (emu_attr(x.and_then(|x| x.child("chExt")), "cx"), emu_attr(x.and_then(|x| x.child("chExt")), "cy"));
    let mut children = vec![];
    for c in e.elements() {
        if matches!(c.local(), "nvGrpSpPr" | "grpSpPr") { continue; }
        let cx_ = xfrm(c);
        let off = cx_.and_then(|x| x.child("off"));
        let ext = cx_.and_then(|x| x.child("ext"));
        let anchor = Anchor::Abs { x: emu_attr(off, "x"), y: emu_attr(off, "y"), w: emu_attr(ext, "cx"), h: emu_attr(ext, "cy") };
        let orels = used(c, rels);
        let kind = match c.local() {
            "sp" => ObjKind::Shape(crate::drawing::parse_shape(c, &wb.theme_colors)),
            "pic" => {
                let id = find(c, "blip").and_then(|b| b.attrs.iter().find(|(k, _)| k.ends_with(":embed")).map(|(_, v)| v.clone()));
                match id.and_then(|id| orels.iter().find(|r| r.id == id && !r.external)).and_then(|r| wb.parts.get(&r.target).map(|d| (r.target.clone(), d.clone()))) {
                    Some((t, d)) if !d.is_empty() => ObjKind::Picture { data: Arc::new(d), ext: t.rsplit('.').next().unwrap_or("png").to_ascii_lowercase() },
                    _ => ObjKind::Other,
                }
            }
            "grpSp" => ObjKind::Group(read(wb, c, rels, ns)),
            _ => ObjKind::Other,
        };
        let cnv = find(c, "cNvPr");
        let (rot, fh, fv, hidden) = read_transform(c);
        let mut o = Object::new(kind, anchor);
        o.id = cnv.and_then(|c| c.attr_u32("id")).unwrap_or(0);
        o.name = cnv.and_then(|c| c.attr("name")).unwrap_or("").into();
        o.descr = cnv.and_then(|c| c.attr("descr")).unwrap_or("").into();
        (o.rot, o.flip_h, o.flip_v, o.hidden) = (rot, fh, fv, hidden);
        if !matches!(o.kind, ObjKind::Group(_)) { o.raw = Some(Arc::new(c.clone())); }
        o.ns = ns.clone();
        o.rels = orels;
        children.push(o);
    }
    Group { children, ch: [cx, cy, cw, chh] }
}

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }

// ---- writing ------------------------------------------------------------------------------------

fn emu_s(px: f32) -> i64 { (px as f64 * EMU as f64).round() as i64 }

fn child_mut<'a>(e: &'a mut Element, l: &str) -> Option<&'a mut Element> {
    e.children.iter_mut().find_map(|n| match n { Node::Element(x) if x.local() == l => Some(x), _ => None })
}
fn pre(e: &Element) -> String { e.name.split_once(':').map(|p| format!("{}:", p.0)).unwrap_or_default() }

fn el(name: &str, attrs: Vec<(&str, String)>) -> Element { Element { name: name.into(), attrs: attrs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(), children: vec![] } }

/// Puts an object's rotation, flips, hidden flag (and, for members, its box in the parent's
/// child space) into its drawing element.
fn patch(e: &mut Element, o: &Object, bx: Option<[f32; 4]>, id: Option<u32>) {
    let p = pre(e);
    // cNvPr: hidden, and a fresh id for members.
    if let Some(nv) = e.children.iter_mut().find_map(|n| match n { Node::Element(x) if x.local().starts_with("nv") => Some(x), _ => None }) {
        if let Some(c) = child_mut(nv, "cNvPr") {
            if o.hidden { c.set_attr("hidden", "1"); } else { c.attrs.retain(|(k, _)| k != "hidden"); }
            if let Some(id) = id { c.set_attr("id", id.to_string()); }
        }
    }
    let local = e.local().to_string();
    let host: &mut Element = match local.as_str() {
        "graphicFrame" => e,
        "grpSp" => match child_mut(e, "grpSpPr") { Some(x) => x, None => return },
        _ => match child_mut(e, "spPr") { Some(x) => x, None => return },
    };
    let xname = if local == "graphicFrame" { format!("{p}xfrm") } else { "a:xfrm".to_string() };
    if child_mut(host, "xfrm").is_none() {
        let at = if local == "graphicFrame" { host.children.iter().position(|n| matches!(n, Node::Element(x) if x.local() == "graphic")).unwrap_or(host.children.len()) } else { 0 };
        host.children.insert(at, Node::Element(el(&xname, vec![])));
    }
    let x = child_mut(host, "xfrm").unwrap();
    x.attrs.retain(|(k, _)| !matches!(k.as_str(), "rot" | "flipH" | "flipV"));
    if o.rot != 0.0 { x.set_attr("rot", ((o.rot as f64 * 60000.0).round() as i64).to_string()); }
    if o.flip_h { x.set_attr("flipH", "1"); }
    if o.flip_v { x.set_attr("flipV", "1"); }
    if let Some(b) = bx {
        let a = if x.name.contains(':') { x.name.split(':').next().unwrap().to_string() } else { "a".into() };
        let _ = a;
        let set = |x: &mut Element, l: &str, k: [(&str, i64); 2]| {
            if child_mut(x, l).is_none() { let pos = match l { "off" => 0, "ext" => 1.min(x.children.len()), _ => x.children.len() }; x.children.insert(pos, Node::Element(el(&format!("a:{l}"), vec![]))); }
            let c = child_mut(x, l).unwrap();
            for (n, v) in k { c.set_attr(n, v.to_string()); }
        };
        set(x, "off", [("x", emu_s(b[0])), ("y", emu_s(b[1]))]);
        set(x, "ext", [("cx", emu_s(b[2] - b[0])), ("cy", emu_s(b[3] - b[1]))]);
    }
}

/// A top-level object's XML with its rotation, flips and hidden flag (unchanged when none apply).
pub fn patch_top(content: String, o: &Object) -> String {
    let plain = o.rot == 0.0 && !o.flip_h && !o.flip_v && !o.hidden;
    if plain && !(content.contains(" rot=") || content.contains("flipH") || content.contains("flipV") || content.contains("hidden=")) { return content; }
    match Element::parse(&content) { Ok(mut e) => { patch(&mut e, o, None, None); e.to_xml() } Err(_) => content }
}

/// What writing a group needs from the drawing writer: new relationships and media parts.
pub struct Sink<'a> {
    pub add_rel: &'a mut dyn FnMut(&str, String, bool) -> String,
    pub new_media: &'a mut dyn FnMut(&Workbook, &Arc<Vec<u8>>, &str) -> String,
}

const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

fn member_xml(w: &Workbook, o: &Object, bx: [f32; 4], next: &mut u32, sink: &mut Sink) -> Option<String> {
    *next += 1;
    let id = *next;
    let mut e = match &o.kind {
        ObjKind::Group(g) => return Some(group_xml(w, o, g, bx, Some(id), next, sink)),
        _ if o.raw.is_some() => {
            let mut e = (**o.raw.as_ref().unwrap()).clone();
            let mut map = HashMap::new();
            for r in &o.rels { if r.external || w.parts.contains_key(&r.target) || r.kind.ends_with("/image") { let n = (sink.add_rel)(&r.kind, r.target.clone(), r.external); map.insert(r.id.clone(), n); } }
            crate::drawing::remap(&mut e, &map);
            e
        }
        ObjKind::Shape(s) => Element::parse(&crate::drawing::shape_xml(o, s, &o.descr, (bx[2] - bx[0], bx[3] - bx[1]))).ok()?,
        ObjKind::Picture { data, ext } => {
            let target = (sink.new_media)(w, data, ext);
            let rid = (sink.add_rel)(RT_IMAGE, target, false);
            Element::parse(&format!("<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"{}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>", attr(&o.name))).ok()?
        }
        _ => return None,
    };
    patch(&mut e, o, Some(bx), Some(id));
    Some(e.to_xml())
}

fn group_xml(w: &Workbook, o: &Object, g: &Group, bx: [f32; 4], id: Option<u32>, next: &mut u32, sink: &mut Sink) -> String {
    let id = id.unwrap_or(o.id);
    let mut x = format!("<xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id=\"{id}\" name=\"{}\"{}{}/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr><xdr:grpSpPr><a:xfrm{}{}{}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/><a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/></a:xfrm></xdr:grpSpPr>",
        attr(&o.name), if o.descr.is_empty() { String::new() } else { format!(" descr=\"{}\"", attr(&o.descr)) }, if o.hidden { " hidden=\"1\"" } else { "" },
        if o.rot != 0.0 { format!(" rot=\"{}\"", (o.rot as f64 * 60000.0).round() as i64) } else { String::new() }, if o.flip_h { " flipH=\"1\"" } else { "" }, if o.flip_v { " flipV=\"1\"" } else { "" },
        emu_s(bx[0]), emu_s(bx[1]), emu_s(bx[2] - bx[0]), emu_s(bx[3] - bx[1]), emu_s(g.ch[0]), emu_s(g.ch[1]), emu_s(g.ch[2]), emu_s(g.ch[3]));
    for c in &g.children {
        if let Some(m) = member_xml(w, c, abs_rect(c), next, sink) { x.push_str(&m); }
    }
    x.push_str("</xdr:grpSp>");
    x
}

/// A top-level group's xdr:grpSp.
pub fn xml(w: &Workbook, si: usize, o: &Object, g: &Group, sink: &mut Sink) -> String {
    let bx = sheet_rect(&w.sheets[si], o);
    let mut next = o.id.max(1) * 1000;
    group_xml(w, o, g, bx, None, &mut next, sink)
}

// ---- printing -----------------------------------------------------------------------------------

use word_engine::layout::DrawItem;

fn turn_items(items: &mut [DrawItem], c: (f32, f32), deg: f32, fh: bool, fv: bool) {
    let t = |p: (f32, f32)| turn(p, c, deg, fh, fv);
    for it in items.iter_mut() {
        match it {
            DrawItem::Rect { x, y, w, h, color } => {
                let pts = [(*x, *y), (*x + *w, *y), (*x + *w, *y + *h), (*x, *y + *h)].map(t).to_vec();
                *it = DrawItem::Path { points: pts, closed: true, fill: Some(*color), stroke: None, ink: None };
            }
            DrawItem::Line { x1, y1, x2, y2, .. } => { let a = t((*x1, *y1)); let b = t((*x2, *y2)); (*x1, *y1, *x2, *y2) = (a.0, a.1, b.0, b.1); }
            DrawItem::Path { points, .. } => for p in points.iter_mut() { *p = t(*p); },
            DrawItem::Image { x, y, w, h, rot, flip, .. } => {
                let m = t((*x + *w / 2.0, *y + *h / 2.0));
                (*x, *y) = (m.0 - *w / 2.0, m.1 - *h / 2.0);
                flip.0 ^= fh; flip.1 ^= fv;
                *rot = (*rot + deg).rem_euclid(360.0);
            }
            DrawItem::Glyphs { glyphs, baseline, .. } => {
                // Text stays upright; its block moves with the rotated shape.
                if let (Some(f), Some(l)) = (glyphs.first(), glyphs.last()) {
                    let mid = ((f.x + l.x) / 2.0, *baseline);
                    let m = t(mid);
                    for g in glyphs.iter_mut() { g.x += m.0 - mid.0; }
                    *baseline = m.1;
                }
            }
            DrawItem::Frame { .. } => {}
        }
    }
}

/// Prints an object that is rotated, flipped or a group (None: draw it the plain way).
pub fn print(wb: &Workbook, si: usize, o: &Object, r: [f32; 4], z: f32, assets: &mut crate::print::objects::Assets, out: &mut Vec<DrawItem>) -> bool {
    if o.hidden { return true; }
    let group = matches!(o.kind, ObjKind::Group(_));
    if !group && o.rot == 0.0 && !o.flip_h && !o.flip_v { return false; }
    let mut items = vec![];
    match &o.kind {
        ObjKind::Group(g) => for c in &g.children { crate::print::objects::draw_object(wb, si, c, g.child_rect(c, r), z, assets, &mut items); },
        _ => { let mut plain = o.clone(); plain.rot = 0.0; plain.flip_h = false; plain.flip_v = false; crate::print::objects::draw_object(wb, si, &plain, r, z, assets, &mut items); }
    }
    if o.rot != 0.0 || o.flip_h || o.flip_v { turn_items(&mut items, ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0), o.rot, o.flip_h, o.flip_v); }
    out.extend(items);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{add_object, Shape};

    #[test]
    fn group_round_trip_and_ungroup() {
        let mut wb = Workbook::new();
        let a = Object::new(ObjKind::Shape(Shape { preset: "rect".into(), text: "A".into(), fill: Some([1, 2, 3]), size: 11.0, ..Default::default() }), Anchor::Abs { x: 0.0, y: 0.0, w: 10.0, h: 10.0 });
        let mut b = Object::new(ObjKind::Shape(Shape { preset: "ellipse".into(), text: "B".into(), size: 11.0, ..Default::default() }), Anchor::Abs { x: 0.0, y: 0.0, w: 10.0, h: 10.0 });
        b.rot = 30.0;
        let (g, bb) = make(vec![(a, [100.0, 100.0, 200.0, 150.0]), (b, [250.0, 120.0, 300.0, 200.0])]);
        assert_eq!(bb, [100.0, 100.0, 300.0, 200.0]);
        let mut go = Object::new(ObjKind::Group(g), Anchor::Abs { x: 100.0, y: 100.0, w: 200.0, h: 100.0 });
        go.flip_h = true;
        add_object(&mut wb.sheets[0].objects, go, "Group");
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let o = &back.sheets[0].objects[0];
        assert!(o.flip_h);
        let ObjKind::Group(g) = &o.kind else { panic!("not a group: {:?}", o.kind) };
        assert_eq!(g.children.len(), 2);
        assert!(matches!(&g.children[0].kind, ObjKind::Shape(s) if s.text == "A" && s.fill == Some([1, 2, 3])));
        assert_eq!(g.children[1].rot, 30.0);
        let r = g.child_rect(&g.children[1], [100.0, 100.0, 300.0, 200.0]);
        assert!((r[0] - 250.0).abs() < 0.1 && (r[3] - 200.0).abs() < 0.1);
        // Moving the group moves the members; ungrouping gives them back where they show.
        let parts = ungroup(o, [0.0, 0.0, 200.0, 100.0]);
        assert_eq!(parts.len(), 2);
        // Flipped horizontally: A (left) ends up on the right.
        assert!((parts[0].1[0] - 100.0).abs() < 0.1 && (parts[0].1[2] - 200.0).abs() < 0.1, "{:?}", parts[0].1);
        // A second write keeps it.
        let again = crate::xlsx::read(&crate::xlsx::write(&back).unwrap()).unwrap();
        assert!(matches!(&again.sheets[0].objects[0].kind, ObjKind::Group(g) if g.children.len() == 2));
    }

    #[test]
    fn groups_and_rotation_print() {
        let wb = Workbook::new();
        let sq = |fill| Object::new(ObjKind::Shape(Shape { preset: "rect".into(), fill: Some(fill), size: 11.0, ..Default::default() }), Anchor::default());
        let (g, _) = make(vec![(sq([255, 0, 0]), [0.0, 0.0, 10.0, 10.0]), (sq([0, 0, 255]), [90.0, 0.0, 100.0, 10.0])]);
        let mut o = Object::new(ObjKind::Group(g), Anchor::Abs { x: 0.0, y: 0.0, w: 100.0, h: 10.0 });
        let mut assets = Default::default();
        let mut items = vec![];
        crate::print::objects::draw_object(&wb, 0, &o, [0.0, 0.0, 200.0, 20.0], 1.0, &mut assets, &mut items);
        // Both members print, scaled with the group.
        let reds = items.iter().filter(|i| matches!(i, DrawItem::Rect { color, .. } | DrawItem::Path { fill: Some(color), .. } if color.0 == 255)).count();
        let blue_x: Vec<f32> = items.iter().filter_map(|i| match i { DrawItem::Rect { x, color, .. } if color.2 == 255 && color.0 == 0 => Some(*x), DrawItem::Path { points, fill: Some(c), .. } if c.2 == 255 && c.0 == 0 => Some(points[0].0), _ => None }).collect();
        assert!(reds > 0 && blue_x.iter().any(|x| (*x - 180.0).abs() < 0.5), "{items:?}");
        // Turned 180°: the blue member is now on the left.
        o.rot = 180.0;
        let mut items = vec![];
        crate::print::objects::draw_object(&wb, 0, &o, [0.0, 0.0, 200.0, 20.0], 1.0, &mut assets, &mut items);
        let xs: Vec<f32> = items.iter().filter_map(|i| match i { DrawItem::Path { points, fill: Some(c), .. } if c.2 == 255 && c.0 == 0 => Some(points.iter().map(|p| p.0).fold(f32::MAX, f32::min)), _ => None }).collect();
        assert!(xs.iter().any(|x| x.abs() < 0.5), "{xs:?}");
        // Hidden objects don't print.
        o.hidden = true;
        let mut items = vec![];
        crate::print::objects::draw_object(&wb, 0, &o, [0.0, 0.0, 200.0, 20.0], 1.0, &mut assets, &mut items);
        assert!(items.is_empty());
    }
}
