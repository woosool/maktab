//! Objects Excel keeps in a sheet's legacy VML drawing: embedded files (OLE Package objects,
//! `<oleObjects>` with an x14 `objectPr`, the file in xl/embeddings/oleObjectN.bin as a compound
//! file with an `\x01Ole10Native` stream) and signature lines (a VML picture shape with
//! `o:signatureline`).
//!
//! In Jadwal both are pictures (embed.rs): an embedded file carries the file's bytes in a
//! `RT_PACKAGE` part and is described `OBJ_TAG` + file name; a signature line is described
//! `SIG_TAG` + signer, title, e-mail and instructions (tab-separated). `stage` (writing) moves such
//! pictures of a sheet out of the drawing into a VML part (merged by write.rs with the comments'
//! VML when the sheet has notes), the oleObjects element and the package parts; `after_read`
//! turns Excel's packages and signature lines back into those pictures. Other OLE objects
//! (Word documents, equations of old...) stay carried as they were.

use super::pivot_parts::sheet_rel;
use super::read::{rels_name, resolve};
use super::xml::{attr, part_text, Element, Node};
use crate::book::Workbook;
use crate::drawing::{Anchor, Mark, ObjKind, Object};
use crate::embed::{carried_part, picture_with_parts, Extra, RT_IMAGE, RT_PACKAGE};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The descriptions the app gives these pictures (app/src/ins_pictures.rs).
pub const OBJ_TAG: &str = "jadwal-object:";
pub const SIG_TAG: &str = "jadwal-signature:";
pub const RT_OLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject";
const RT_VML: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
pub const CT_OLE: &str = "application/vnd.openxmlformats-officedocument.oleObject";
const CT_VML: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
/// {0003000C-0000-0000-C000-000000000046}: the Packager's class.
const CLSID_PACKAGE: [u8; 16] = [0x0C, 0x00, 0x03, 0x00, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
/// The picture shape type the VML shapes use.
const SHAPETYPE_75: &str = "<v:shapetype id=\"_x0000_t75\" coordsize=\"21600,21600\" o:spt=\"75\" o:preferrelative=\"t\" path=\"m@4@5l@4@11@9@11@9@5xe\" filled=\"f\" stroked=\"f\"><v:stroke joinstyle=\"miter\"/><v:formulas><v:f eqn=\"if lineDrawn pixelLineWidth 0\"/><v:f eqn=\"sum @0 1 0\"/><v:f eqn=\"sum 0 0 @1\"/><v:f eqn=\"prod @2 1 2\"/><v:f eqn=\"prod @3 21600 pixelWidth\"/><v:f eqn=\"prod @3 21600 pixelHeight\"/><v:f eqn=\"sum @0 0 1\"/><v:f eqn=\"prod @6 1 2\"/><v:f eqn=\"prod @7 21600 pixelWidth\"/><v:f eqn=\"sum @8 21600 0\"/><v:f eqn=\"prod @7 21600 pixelHeight\"/><v:f eqn=\"sum @10 21600 0\"/></v:formulas><v:path o:extrusionok=\"f\" gradientshapeok=\"t\" o:connecttype=\"rect\"/><o:lock v:ext=\"edit\" aspectratio=\"t\"/></v:shapetype>";

// ---- the package stream -------------------------------------------------------------------------

fn cstr(s: &str) -> Vec<u8> { let mut v: Vec<u8> = s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'_' }).collect(); v.push(0); v }

/// An OLE Package object (compound file) embedding a file.
pub fn package_bin(name: &str, data: &[u8]) -> Vec<u8> {
    let name = if name.is_empty() { "file" } else { name };
    let temp = format!("C:\\Users\\Public\\{name}");
    let mut n = vec![];
    n.extend_from_slice(&2u16.to_le_bytes());
    n.extend(cstr(name));
    n.extend(cstr(name));
    n.extend_from_slice(&0x0003_0000u32.to_le_bytes());
    n.extend_from_slice(&(cstr(&temp).len() as u32).to_le_bytes());
    n.extend(cstr(&temp));
    n.extend_from_slice(&(data.len() as u32).to_le_bytes());
    n.extend_from_slice(data);
    // The Unicode forms of the temporary path, label and path.
    for s in [&temp, &name.to_string(), &name.to_string()] {
        let u: Vec<u16> = s.encode_utf16().collect();
        n.extend_from_slice(&(u.len() as u32).to_le_bytes());
        for c in u { n.extend_from_slice(&c.to_le_bytes()); }
    }
    let mut native = (n.len() as u32).to_le_bytes().to_vec();
    native.extend(n);
    let mut comp = vec![1, 0, 0xFE, 0xFF, 3, 0x0A, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF];
    comp.extend_from_slice(&CLSID_PACKAGE);
    for s in ["OLE Package"] { comp.extend_from_slice(&(cstr(s).len() as u32).to_le_bytes()); comp.extend(cstr(s)); }
    comp.extend_from_slice(&0u32.to_le_bytes()); // no clipboard format
    comp.extend_from_slice(&(cstr("Package").len() as u32).to_le_bytes());
    comp.extend(cstr("Package"));
    comp.extend_from_slice(&0x71B2_39F4u32.to_le_bytes());
    comp.extend_from_slice(&[0; 12]);
    crate::cfb::write(CLSID_PACKAGE, &[("\u{1}Ole10Native", &native), ("\u{1}CompObj", &comp), ("\u{3}ObjInfo", &[0, 0, 3, 0, 0x0D, 0])])
}

/// The (file name, bytes) an OLE Package object embeds.
pub fn package_of(bin: &[u8]) -> Option<(String, Vec<u8>)> {
    let (_, streams) = crate::cfb::read(bin)?;
    let s = &streams.iter().find(|(n, _)| n == "\u{1}Ole10Native")?.1;
    let mut i = 4 + 2;
    let cs = |b: &[u8], i: &mut usize| -> Option<String> { let e = b.get(*i..)?.iter().position(|c| *c == 0)? + *i; let t = b[*i..e].iter().map(|c| *c as char).collect(); *i = e + 1; Some(t) };
    let label = cs(s, &mut i)?;
    let _path = cs(s, &mut i)?;
    i += 4;
    let tl = u32::from_le_bytes(s.get(i..i + 4)?.try_into().ok()?) as usize;
    i += 4 + tl;
    let dl = u32::from_le_bytes(s.get(i..i + 4)?.try_into().ok()?) as usize;
    i += 4;
    let data = s.get(i..i + dl)?.to_vec();
    // The Unicode label, when present, keeps non-ANSI names.
    let mut j = i + dl;
    let mut uni = vec![];
    for _ in 0..3 {
        let Some(n) = s.get(j..j + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize) else { break };
        let Some(b) = s.get(j + 4..j + 4 + n * 2) else { break };
        uni.push(String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()));
        j += 4 + n * 2;
    }
    let name = uni.get(1).filter(|u| !u.is_empty()).cloned().unwrap_or(label);
    Some((name, data))
}

// ---- geometry -----------------------------------------------------------------------------------

/// An object's anchor as cell marks (offsets in pixels).
fn marks(wb: &Workbook, si: usize, a: &Anchor) -> (Mark, Mark) {
    let s = &wb.sheets[si];
    let at = |mut px: f32, size: &dyn Fn(u32) -> f32| -> (u32, f32) { let mut i = 0; while i < 16383 && px >= size(i) && size(i) > 0.0 || i < 16383 && size(i) == 0.0 { px -= size(i); i += 1; } (i, px.max(0.0)) };
    let pos = |i: u32, size: &dyn Fn(u32) -> f32| -> f32 { (0..i).map(size).sum() };
    let cw = |c: u32| s.col_width(c);
    let rh = |r: u32| s.row_height(r);
    let rect = match a {
        Anchor::Two { from, to, .. } => return (from.clone(), to.clone()),
        Anchor::One { from, w, h } => { let x = pos(from.col, &cw) + from.dx; let y = pos(from.row, &rh) + from.dy; [x, y, x + w, y + h] }
        Anchor::Abs { x, y, w, h } => [*x, *y, x + w, y + h],
    };
    let (c0, dx0) = at(rect[0], &cw);
    let (r0, dy0) = at(rect[1], &rh);
    let (c1, dx1) = at(rect[2], &cw);
    let (r1, dy1) = at(rect[3], &rh);
    (Mark { col: c0, dx: dx0, row: r0, dy: dy0 }, Mark { col: c1, dx: dx1, row: r1, dy: dy1 })
}

fn size_of(wb: &Workbook, si: usize, f: &Mark, t: &Mark) -> (f32, f32) {
    let s = &wb.sheets[si];
    let w: f32 = (f.col..t.col).map(|c| s.col_width(c)).sum::<f32>() - f.dx + t.dx;
    let h: f32 = (f.row..t.row).map(|r| s.row_height(r)).sum::<f32>() - f.dy + t.dy;
    (w.max(1.0), h.max(1.0))
}

fn vml_anchor(f: &Mark, t: &Mark) -> String { format!("{}, {}, {}, {}, {}, {}, {}, {}", f.col, f.dx.round() as i64, f.row, f.dy.round() as i64, t.col, t.dx.round() as i64, t.row, t.dy.round() as i64) }

fn xdr_mark(tag: &str, m: &Mark) -> String {
    let e = |v: f32| (v as f64 * crate::drawing::EMU as f64).round() as i64;
    format!("<{tag}><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></{tag}>", m.col, e(m.dx), m.row, e(m.dy))
}

fn guid(seed: &str) -> String {
    use sha2::Digest;
    let h = sha2::Sha256::digest(seed.as_bytes());
    let x: String = h.iter().take(16).map(|b| format!("{b:02X}")).collect();
    format!("{{{}-{}-4{}-8{}-{}}}", &x[..8], &x[8..12], &x[13..16], &x[17..20], &x[20..32])
}

fn is_native(wb: &Workbook, o: &Object) -> bool {
    if !matches!(o.kind, ObjKind::Picture { .. }) { return false; }
    o.descr.starts_with(SIG_TAG) || o.descr.starts_with(OBJ_TAG) && carried_part(wb, o, RT_PACKAGE).is_some()
}

/// A picture's image as a PNG (EMF and other formats Jadwal can't decode give None).
fn to_png(data: &[u8]) -> Option<Vec<u8>> {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) { return Some(data.to_vec()); }
    let img = image::load_from_memory(data).ok()?;
    let mut c = std::io::Cursor::new(vec![]);
    img.write_to(&mut c, image::ImageFormat::Png).ok()?;
    Some(c.into_inner())
}

// ---- writing ------------------------------------------------------------------------------------

fn set_override(w: &mut Workbook, part: &str, ct: &str) {
    let mut t = w.parts.get("[Content_Types].xml").and_then(|b| Element::parse(&part_text(b)).ok()).unwrap_or_else(|| Element { name: "Types".into(), attrs: vec![("xmlns".into(), "http://schemas.openxmlformats.org/package/2006/content-types".into())], children: vec![] });
    let pn = format!("/{part}");
    t.children.retain(|n| !matches!(n, Node::Element(e) if e.local() == "Override" && e.attr("PartName") == Some(pn.as_str())));
    t.children.push(Node::Element(Element { name: "Override".into(), attrs: vec![("PartName".into(), pn), ("ContentType".into(), ct.into())], children: vec![] }));
    w.parts.insert("[Content_Types].xml".into(), t.to_xml().into_bytes());
}

fn free(w: &Workbook, prefix: &str, ext: &str) -> String { (1..).map(|i| format!("{prefix}{i}{ext}")).find(|n| !w.parts.contains_key(n)).unwrap() }

/// The workbook as saved: embedded files and signature lines become VML objects (see the module
/// comment). None when no sheet has any.
pub fn stage(wb: &Workbook) -> Option<Workbook> {
    if !wb.sheets.iter().any(|s| s.objects.iter().any(|o| is_native(wb, o))) { return None; }
    let mut w = wb.clone();
    for si in 0..w.sheets.len() {
        // A sheet keeping a VML drawing or OLE objects of its own is left as it is.
        if w.sheets[si].extra.iter().any(|(k, _)| k == "oleObjects" || k == "legacyDrawing") { continue; }
        let picks: Vec<usize> = (0..w.sheets[si].objects.len()).filter(|&i| is_native(&w, &w.sheets[si].objects[i])).collect();
        if picks.is_empty() { continue; }
        let vpath = free(&w, "xl/drawings/vmlDrawing", ".vml");
        let mut vrels: Vec<(String, String)> = vec![];
        let mut shapes = String::new();
        let mut oles = String::new();
        for (k, &oi) in picks.iter().enumerate() {
            let o = w.sheets[si].objects[oi].clone();
            let ObjKind::Picture { data, ext } = &o.kind else { continue };
            let spid = 1024 * (si + 1) + k + 1;
            let (f, t) = marks(&w, si, &o.anchor);
            let (pw, ph) = size_of(&w, si, &f, &t);
            // The image part: the picture's own when unchanged, else a new one.
            let img = match o.rels.iter().find(|r| r.kind == RT_IMAGE && !r.external && !r.target.ends_with(".svg") && w.parts.get(&r.target).is_some_and(|b| b.as_slice() == data.as_slice())) {
                Some(r) => r.target.clone(),
                None => { let p = free(&w, "xl/media/image", &format!(".{ext}")); w.parts.insert(p.clone(), (**data).clone()); p }
            };
            crate::embed::ensure_content_type(&mut w, ext, match ext.as_str() { "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "bmp" => "image/bmp", "emf" => "image/x-emf", "wmf" => "image/x-wmf", _ => "image/png" });
            vrels.push((format!("rId{}", vrels.len() + 1), img.clone()));
            let vid = vrels.last().unwrap().0.clone();
            let style = format!("position:absolute;margin-left:0;margin-top:0;width:{:.2}pt;height:{:.2}pt;z-index:{}", pw * 0.75, ph * 0.75, k + 1);
            let client = format!("<x:ClientData ObjectType=\"Pict\"><x:SizeWithCells/><x:Anchor>{}</x:Anchor><x:CF>Pict</x:CF><x:AutoPict/></x:ClientData>", vml_anchor(&f, &t));
            if let Some(rest) = o.descr.strip_prefix(SIG_TAG) {
                let fl: Vec<&str> = rest.split('\t').collect();
                let g = |i: usize| attr(fl.get(i).copied().unwrap_or(""));
                shapes.push_str(&format!("<v:shape id=\"_x0000_s{spid}\" type=\"#_x0000_t75\" alt=\"{}\" style=\"{style}\" filled=\"t\" fillcolor=\"window [65]\" stroked=\"t\" strokecolor=\"windowText [64]\" o:insetmode=\"auto\"><v:fill color2=\"window [65]\"/><v:imagedata o:relid=\"{vid}\" o:title=\"\"/><o:lock v:ext=\"edit\" rotation=\"t\" cropping=\"t\" verticies=\"t\" text=\"t\" grouping=\"t\"/><o:signatureline v:ext=\"edit\" id=\"{}\" provid=\"{{00000000-0000-0000-0000-000000000000}}\" o:suggestedsigner=\"{}\" o:suggestedsigner2=\"{}\" o:suggestedsigneremail=\"{}\" signinginstructionsset=\"t\" o:signinginstructions=\"{}\" issignatureline=\"t\" showsigndate=\"t\"/>{client}</v:shape>",
                    attr(&o.name), guid(&format!("{si}/{oi}/{}", o.descr)), g(0), g(1), g(2), g(3)));
                continue;
            }
            // An embedded file: the package part becomes the OLE object.
            let Some((ppath, bytes)) = carried_part(&w, &o, RT_PACKAGE) else { continue };
            let name = o.descr.strip_prefix(OBJ_TAG).unwrap_or("").to_string();
            let bin = if ppath.ends_with(".bin") { ppath.clone() } else { free(&w, "xl/embeddings/oleObject", ".bin") };
            w.parts.insert(bin.clone(), package_bin(&name, &bytes));
            set_override(&mut w, &bin, CT_OLE);
            let rid = sheet_rel(&mut w, si, RT_OLE, &bin, "rIdOle");
            let iid = sheet_rel(&mut w, si, RT_IMAGE, &img, "rIdOle");
            shapes.push_str(&format!("<v:shape id=\"_x0000_s{spid}\" type=\"#_x0000_t75\" style=\"{style}\" filled=\"t\" fillcolor=\"window [65]\" stroked=\"t\" strokecolor=\"windowText [64]\" o:insetmode=\"auto\"><v:fill color2=\"window [65]\"/><v:imagedata o:relid=\"{vid}\" o:title=\"\"/>{client}</v:shape>"));
            oles.push_str(&format!("<mc:AlternateContent xmlns:mc=\"{NS_MC}\"><mc:Choice Requires=\"x14\" xmlns:x14=\"{NS_X14}\"><oleObject progId=\"Package\" dvAspect=\"DVASPECT_ICON\" shapeId=\"{spid}\" r:id=\"{rid}\"><objectPr defaultSize=\"0\" autoPict=\"0\" r:id=\"{iid}\"><anchor moveWithCells=\"1\">{}{}</anchor></objectPr></oleObject></mc:Choice><mc:Fallback><oleObject progId=\"Package\" dvAspect=\"DVASPECT_ICON\" shapeId=\"{spid}\" r:id=\"{rid}\"/></mc:Fallback></mc:AlternateContent>",
                xdr_mark("from", &f), xdr_mark("to", &t)));
        }
        let mut x = format!("<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\"><o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"{}\"/></o:shapelayout>", si + 1);
        x.push_str(SHAPETYPE_75);
        x.push_str(&shapes);
        x.push_str("</xml>");
        w.parts.insert(vpath.clone(), x.into_bytes());
        let mut r = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        for (id, t) in &vrels { r.push_str(&format!("<Relationship Id=\"{id}\" Type=\"{RT_IMAGE}\" Target=\"{}\"/>", attr(&super::pivot_parts::relative(&vpath, t)))); }
        r.push_str("</Relationships>");
        w.parts.insert(rels_name(&vpath), r.into_bytes());
        crate::embed::ensure_content_type(&mut w, "vml", CT_VML);
        let vid = sheet_rel(&mut w, si, RT_VML, &vpath, "rIdVml");
        let s = &mut w.sheets[si];
        s.extra.push(("legacyDrawing".into(), format!("<legacyDrawing r:id=\"{vid}\"/>")));
        if !oles.is_empty() { s.extra.push(("oleObjects".into(), format!("<oleObjects xmlns:xdr=\"{NS_XDR}\">{oles}</oleObjects>"))); }
        for oi in picks.into_iter().rev() { s.objects.remove(oi); }
    }
    Some(w)
}

/// The VML part of a sheet's carried legacyDrawing, if any.
pub fn carried_vml(wb: &Workbook, si: usize) -> Option<String> {
    let s = &wb.sheets[si];
    let raw = &s.extra.iter().find(|(k, _)| k == "legacyDrawing")?.1;
    let id = Element::parse(raw).ok()?.attrs.iter().find(|(k, _)| k.ends_with(":id")).map(|(_, v)| v.clone())?;
    let t = s.extra.iter().filter(|(k, _)| k == "rel").find_map(|(_, v)| { let f: Vec<&str> = v.split('\t').collect(); (f.len() >= 4 && f[0] == id && f[3] != "1").then(|| f[2].to_string()) })?;
    wb.parts.contains_key(&t).then_some(t)
}

fn is_note(e: &Element) -> bool {
    (e.local() == "shape" && e.child("ClientData").and_then(|c| c.attr("ObjectType")) == Some("Note")) || (e.local() == "shapetype" && e.attr("id") == Some("_x0000_t202"))
}

/// What a carried VML part adds to the comments' VML write.rs generates: its shapes other than
/// notes (XML), its relationships (id, type, absolute target) and its highest shape number.
pub fn merge_shapes(wb: &Workbook, part: &str) -> (String, Vec<(String, String, String)>, usize) {
    let Some(e) = wb.parts.get(part).and_then(|b| Element::parse(&part_text(b)).ok()) else { return (String::new(), vec![], 0) };
    let mut xml = String::new();
    let mut max = 0;
    for c in e.elements() {
        if c.local() == "shapelayout" || is_note(c) { continue; }
        if let Some(n) = c.attr("id").and_then(|i| i.strip_prefix("_x0000_s")).and_then(|n| n.parse::<usize>().ok()) { max = max.max(n); }
        xml.push_str(&c.to_xml());
    }
    let rels = wb.parts.get(&rels_name(part)).and_then(|b| Element::parse(&part_text(b)).ok()).map(|r| r.all("Relationship").filter(|x| x.attr("TargetMode") != Some("External")).filter_map(|x| Some((x.attr("Id")?.to_string(), x.attr("Type")?.to_string(), resolve(part, x.attr("Target")?)))).collect()).unwrap_or_default();
    (xml, rels, max)
}

/// Whether a VML drawing holds shapes other than notes (read.rs keeps it when it does).
pub fn has_objects(vml: &str) -> bool {
    Element::parse(vml).is_ok_and(|e| e.all("shape").any(|s| !is_note(s)))
}

// ---- reading ------------------------------------------------------------------------------------

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }

fn rid(e: &Element) -> Option<String> { e.attrs.iter().find(|(k, _)| k.ends_with(":id") && !k.starts_with("xmlns")).map(|(_, v)| v.clone()) }

fn xdr_mark_of(e: &Element) -> Mark {
    let n = |l: &str| e.child(l).map(|x| x.text().trim().parse::<f64>().unwrap_or(0.0)).unwrap_or(0.0);
    Mark { col: n("col") as u32, dx: n("colOff") as f32 / crate::drawing::EMU, row: n("row") as u32, dy: n("rowOff") as f32 / crate::drawing::EMU }
}

fn anchor_of_vml(shape: &Element) -> Option<(Mark, Mark)> {
    let a: Vec<f32> = find(shape, "Anchor")?.text().split(',').filter_map(|v| v.trim().parse().ok()).collect();
    (a.len() == 8).then(|| (Mark { col: a[0] as u32, dx: a[1], row: a[2] as u32, dy: a[3] }, Mark { col: a[4] as u32, dx: a[5], row: a[6] as u32, dy: a[7] }))
}

/// A generic picture for an image Jadwal can't show (EMF icons...): a page with the name.
fn placeholder(label: &str, w: f32, h: f32) -> Option<Vec<u8>> {
    use crate::drawing::Shape;
    let base = Shape { preset: "rect".into(), text_box: true, center: true, size: 8.0, color: Some([0x20, 0x20, 0x20]), ..Default::default() };
    let list = vec![
        (Shape { preset: "foldedCorner".into(), fill: Some([0xF4, 0xF6, 0xFA]), line: Some([0x70, 0x78, 0x88]), ..Default::default() }, [w * 0.3, h * 0.05, w * 0.7, h * 0.6]),
        (Shape { text: label.to_string(), ..base }, [0.0, h * 0.62, w, h]),
    ];
    crate::embed::shapes_png(&list, w, h)
}

fn parse_vml(wb: &Workbook, part: &str) -> Option<Element> { wb.parts.get(part).and_then(|b| Element::parse(&part_text(b)).ok()) }

/// After drawings are read: Excel's embedded packages and signature lines become pictures.
pub fn after_read(wb: &mut Workbook) {
    for si in 0..wb.sheets.len() {
        let rels: HashMap<String, String> = wb.sheets[si].extra.iter().filter(|(k, _)| k == "rel").filter_map(|(_, v)| { let f: Vec<&str> = v.split('\t').collect(); (f.len() >= 4 && f[3] != "1").then(|| (f[0].to_string(), f[2].to_string())) }).collect();
        let vpath = carried_vml(wb, si);
        let vml = vpath.as_ref().and_then(|p| parse_vml(wb, p));
        let vrels: HashMap<String, String> = vpath.as_ref().map(|p| merge_shapes(wb, p).1.into_iter().map(|(i, _, t)| (i, t)).collect()).unwrap_or_default();
        let vshape = |spid: &str| vml.as_ref().and_then(|v| v.all("shape").find(|s| s.attr("id").and_then(|i| i.strip_prefix("_x0000_s")) == Some(spid)).cloned());
        let vimage = |s: &Element| s.child("imagedata").and_then(|i| i.attrs.iter().find(|(k, _)| k.ends_with("relid")).map(|(_, v)| v.clone())).and_then(|i| vrels.get(&i).cloned());
        // (anchor, image part, descr, package bytes) per object; the shape ids and sheet rels used.
        let mut found: Vec<(Mark, Mark, Option<String>, String, Option<(String, Vec<u8>)>)> = vec![];
        let mut gone_shapes: HashSet<String> = HashSet::new();
        let mut gone_rels: HashSet<String> = HashSet::new();
        let mut gone_parts: Vec<String> = vec![];
        let mut ole_done = false;
        if let Some(raw) = wb.sheets[si].extra.iter().find(|(k, _)| k == "oleObjects").map(|(_, v)| v.clone()) {
            if let Ok(e) = Element::parse(&raw) {
                // Each object: Choice's oleObject inside markup compatibility, else the plain one.
                let objs: Vec<Element> = e.elements().filter_map(|c| if c.local() == "AlternateContent" { c.all("Choice").find_map(|ch| ch.child("oleObject")).or_else(|| c.child("Fallback").and_then(|f| f.child("oleObject"))).cloned() } else if c.local() == "oleObject" { Some(c.clone()) } else { None }).collect();
                let mut ok = !objs.is_empty();
                let mut list = vec![];
                for ob in &objs {
                    let pkg = ob.attr("progId").is_some_and(|p| p.starts_with("Package"));
                    let r = rid(ob);
                    let bin = r.as_ref().and_then(|r| rels.get(r)).cloned();
                    let content = bin.as_ref().and_then(|b| wb.parts.get(b)).and_then(|b| package_of(b));
                    let spid = ob.attr("shapeId").unwrap_or("").to_string();
                    let shape = vshape(&spid);
                    let pr = ob.child("objectPr");
                    let ir = pr.and_then(rid);
                    let img = ir.as_ref().and_then(|i| rels.get(i)).cloned().or_else(|| shape.as_ref().and_then(&vimage));
                    let anchor = pr.and_then(|p| p.child("anchor")).and_then(|a| Some((xdr_mark_of(a.child("from")?), xdr_mark_of(a.child("to")?)))).or_else(|| shape.as_ref().and_then(anchor_of_vml));
                    match (pkg, content, anchor) {
                        (true, Some(c), Some((f, t))) => {
                            list.push((f, t, img, format!("{OBJ_TAG}{}", c.0), Some(c)));
                            gone_shapes.insert(spid);
                            gone_rels.extend(r.into_iter().chain(ir));
                            gone_parts.extend(bin);
                        }
                        _ => { ok = false; break; }
                    }
                }
                if ok { found.extend(list); ole_done = true; }
            }
        }
        if !ole_done { gone_shapes.clear(); gone_rels.clear(); gone_parts.clear(); }
        // Signature lines.
        if let Some(v) = &vml {
            for s in v.all("shape") {
                let Some(sl) = s.child("signatureline") else { continue };
                if !sl.attr("issignatureline").is_some_and(|x| x == "t" || x == "true") { continue; }
                let Some((f, t)) = anchor_of_vml(s) else { continue };
                let a = |k: &str| sl.attrs.iter().find(|(n, _)| super::xml::local(n) == k).map(|(_, v)| v.replace(['\t', '\n'], " ")).unwrap_or_default();
                let descr = format!("{SIG_TAG}{}\t{}\t{}\t{}", a("suggestedsigner"), a("suggestedsigner2"), a("suggestedsigneremail"), a("signinginstructions"));
                found.push((f, t, vimage(s), descr, None));
                if let Some(id) = s.attr("id").and_then(|i| i.strip_prefix("_x0000_s")) { gone_shapes.insert(id.to_string()); }
            }
        }
        if found.is_empty() { continue; }
        for p in &gone_parts { wb.parts.remove(p); }
        for (f, t, img, descr, pkg) in found {
            let (w, h) = size_of(wb, si, &f, &t);
            let label = match &pkg { Some((n, _)) => n.clone(), None => "Signature".into() };
            let png = img.as_ref().and_then(|p| wb.parts.get(p)).and_then(|b| to_png(b)).or_else(|| if pkg.is_none() { signature_png(&descr, w, h) } else { placeholder(&label, w, h) }).unwrap_or_default();
            let anchor = Anchor::Two { from: f, to: t, edit_as: Some("oneCell".into()) };
            let extras = pkg.map(|(_, bytes)| vec![Extra { rel: RT_PACKAGE, prefix: "xl/embeddings/oleObject", ext: "bin".into(), mime: CT_OLE.into(), bytes }]).unwrap_or_default();
            let base = if descr.starts_with(SIG_TAG) { "Signature Line" } else { "Object" };
            let o = picture_with_parts(wb, si, png, anchor, (w, h), base, &descr, extras);
            crate::drawing::add_object(&mut wb.sheets[si].objects, o, base);
        }
        // What's left of the VML drawing and the OLE objects.
        let s = &mut wb.sheets[si];
        if ole_done { s.extra.retain(|(k, _)| k != "oleObjects"); }
        s.extra.retain(|(k, v)| !(k == "rel" && v.split('\t').next().is_some_and(|i| gone_rels.contains(i))));
        let (Some(vp), Some(mut v)) = (vpath, vml) else { continue };
        v.children.retain(|n| !matches!(n, Node::Element(e) if e.local() == "shape" && e.attr("id").and_then(|i| i.strip_prefix("_x0000_s")).is_some_and(|i| gone_shapes.contains(i))));
        let kept = v.all("shape").filter(|x| !is_note(x)).count();
        if kept == 0 {
            let id = carried_vml_rel(s);
            s.extra.retain(|(k, _)| k != "legacyDrawing");
            if let Some(id) = id { s.extra.retain(|(k, x)| !(k == "rel" && x.split('\t').next() == Some(id.as_str()))); }
            wb.parts.remove(&rels_name(&vp));
            wb.parts.remove(&vp);
        } else {
            wb.parts.insert(vp, v.to_xml().into_bytes());
        }
    }
    let _ = Arc::new(());
}

fn carried_vml_rel(s: &crate::book::Sheet) -> Option<String> {
    let raw = &s.extra.iter().find(|(k, _)| k == "legacyDrawing")?.1;
    Element::parse(raw).ok()?.attrs.iter().find(|(k, _)| k.ends_with(":id")).map(|(_, v)| v.clone())
}

/// A signature line picture (for files whose image Jadwal can't show): an X, a line, the
/// signer's name and title.
fn signature_png(descr: &str, w: f32, h: f32) -> Option<Vec<u8>> {
    use crate::drawing::Shape;
    let f: Vec<&str> = descr.strip_prefix(SIG_TAG).unwrap_or("").split('\t').collect();
    let t = |s: &str, size: f32, r: [f32; 4]| (Shape { text: s.into(), preset: "rect".into(), text_box: true, size, color: Some([0x20, 0x20, 0x20]), ..Default::default() }, r);
    let list = vec![
        t("X", 20.0, [w * 0.04, h * 0.25, w * 0.2, h * 0.55]),
        (Shape { preset: "rect".into(), fill: Some([0x40, 0x40, 0x40]), ..Default::default() }, [w * 0.04, h * 0.56, w * 0.96, h * 0.57]),
        t(f.first().copied().unwrap_or(""), 10.0, [w * 0.04, h * 0.6, w * 0.96, h * 0.78]),
        t(f.get(1).copied().unwrap_or(""), 9.0, [w * 0.04, h * 0.78, w * 0.96, h * 0.96]),
    ];
    crate::embed::shapes_png(&list, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::Pos;
    use crate::xlsx::pkgcheck::{check_package, part, unzip};

    const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x2D, 0x7A, 0x9C, 0x0F, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82];

    fn anchor(c: u32, r: u32) -> Anchor { Anchor::Two { from: Mark { col: c, row: r, ..Default::default() }, to: Mark { col: c + 2, row: r + 4, ..Default::default() }, edit_as: Some("oneCell".into()) } }

    fn book(note: bool) -> Workbook {
        let mut wb = Workbook::new();
        wb.set_input(0, Pos::new(0, 0), "x");
        if note { wb.sheets[0].notes.insert(Pos::new(0, 0), "A note".into()); }
        let file = b"hello, world\nsecond line".to_vec();
        let o = picture_with_parts(&mut wb, 0, PNG.to_vec(), anchor(1, 1), (128.0, 80.0), "Object", &format!("{OBJ_TAG}notes.txt"), vec![Extra { rel: RT_PACKAGE, prefix: "xl/embeddings/oleObject", ext: "bin".into(), mime: CT_OLE.into(), bytes: file }]);
        crate::drawing::add_object(&mut wb.sheets[0].objects, o, "Object");
        let s = picture_with_parts(&mut wb, 0, PNG.to_vec(), anchor(5, 2), (128.0, 80.0), "Signature Line", &format!("{SIG_TAG}Ann Lee\tCEO\tann@example.com\tSign here"), vec![]);
        crate::drawing::add_object(&mut wb.sheets[0].objects, s, "Signature Line");
        wb
    }

    fn check(wb: &Workbook, note: bool) {
        let bytes = crate::xlsx::write(wb).unwrap();
        let files = unzip(&bytes);
        check_package(&files);
        let sheet = &files["xl/worksheets/sheet1.xml"];
        assert!(sheet.contains("<oleObjects") && sheet.contains("progId=\"Package\"") && sheet.contains("<legacyDrawing"), "{sheet}");
        assert_eq!(sheet.matches("<legacyDrawing").count(), 1);
        assert!(!files.keys().any(|k| k.starts_with("xl/drawings/drawing")), "the objects left the drawing");
        let vml: Vec<&String> = files.iter().filter(|(k, _)| k.ends_with(".vml")).map(|(_, v)| v).collect();
        assert_eq!(vml.len(), 1);
        assert!(vml[0].contains("o:signatureline") && vml[0].contains("Ann Lee") && vml[0].contains("ObjectType=\"Pict\""));
        assert_eq!(vml[0].contains("ObjectType=\"Note\""), note);
        let ct = &files["[Content_Types].xml"];
        assert!(ct.contains("/xl/embeddings/oleObject1.bin\" ContentType=\"application/vnd.openxmlformats-officedocument.oleObject\""), "{ct}");
        let bin = part(&bytes, "xl/embeddings/oleObject1.bin");
        assert_eq!(package_of(&bin), Some(("notes.txt".into(), b"hello, world\nsecond line".to_vec())));
        // Read back: the same pictures, and saving again gives the same objects.
        let back = crate::xlsx::read(&bytes).unwrap();
        let objs = &back.sheets[0].objects;
        assert_eq!(objs.len(), 2, "{:?}", objs.iter().map(|o| &o.descr).collect::<Vec<_>>());
        let ob = objs.iter().find(|o| o.descr == format!("{OBJ_TAG}notes.txt")).expect("object");
        assert_eq!(carried_part(&back, ob, RT_PACKAGE).unwrap().1, b"hello, world\nsecond line");
        assert!(matches!(&ob.kind, ObjKind::Picture { data, .. } if data.as_slice() == PNG));
        assert_eq!(ob.anchor, anchor(1, 1));
        let sg = objs.iter().find(|o| o.descr.starts_with(SIG_TAG)).expect("signature");
        assert_eq!(sg.descr, format!("{SIG_TAG}Ann Lee\tCEO\tann@example.com\tSign here"));
        assert_eq!(back.sheets[0].notes.len(), note as usize);
        assert!(!back.sheets[0].extra.iter().any(|(k, _)| k == "oleObjects" || k == "legacyDrawing"));
        let again = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&again);
        assert_eq!(again["xl/worksheets/sheet1.xml"].matches("<oleObject ").count(), 2);
        if let Ok(dir) = std::env::var("JADWAL_OLE_OUT") { std::fs::write(format!("{dir}/ole{}.xlsx", note as u8), &bytes).unwrap(); }
    }

    #[test]
    fn ole_and_signature_round_trip() { check(&book(false), false); }

    #[test]
    fn ole_and_signature_share_vml_with_notes() { check(&book(true), true); }

    /// Another OLE object (not a package) stays carried, and its VML shape joins the notes' VML.
    #[test]
    fn other_ole_objects_stay_carried_beside_notes() {
        use std::io::{Read, Write};
        let bytes = crate::xlsx::write(&book(true)).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            for i in 0..z.len() {
                let mut f = z.by_index(i).unwrap();
                let mut v = vec![]; f.read_to_end(&mut v).unwrap();
                if f.name() == "xl/worksheets/sheet1.xml" { v = String::from_utf8(v).unwrap().replace("progId=\"Package\"", "progId=\"Word.Document.12\"").into_bytes(); }
                w.start_file(f.name(), zip::write::SimpleFileOptions::default()).unwrap();
                w.write_all(&v).unwrap();
            }
            w.finish().unwrap();
        }
        let back = crate::xlsx::read(&buf.into_inner()).unwrap();
        assert_eq!(back.sheets[0].objects.len(), 1, "only the signature line became a picture");
        assert!(back.sheets[0].extra.iter().any(|(k, _)| k == "oleObjects"));
        let again = crate::xlsx::write(&back).unwrap();
        let files = unzip(&again);
        check_package(&files);
        let sheet = &files["xl/worksheets/sheet1.xml"];
        assert_eq!(sheet.matches("<legacyDrawing ").count(), 1);
        assert!(sheet.contains("Word.Document.12"));
        let vml: Vec<&String> = files.iter().filter(|(k, _)| k.ends_with(".vml")).map(|(_, v)| v).collect();
        assert_eq!(vml.len(), 1, "one VML part per sheet");
        assert!(vml[0].contains("ObjectType=\"Note\"") && vml[0].contains("_x0000_s1025"));
        // A sheet keeping OLE objects of its own saves new signature lines as plain pictures.
        assert!(files.keys().any(|k| k.starts_with("xl/drawings/drawing")));
        assert_eq!(vml[0].matches("ObjectType=\"Note\"").count(), 1);
        let back2 = crate::xlsx::read(&again).unwrap();
        assert_eq!(back2.sheets[0].notes.len(), 1);
        assert_eq!(back2.sheets[0].objects.len(), 1);
    }

    #[test]
    fn package_stream() {
        let b = package_bin("ünï.txt", b"abc");
        assert_eq!(package_of(&b), Some(("ünï.txt".into(), b"abc".to_vec())));
    }
}
