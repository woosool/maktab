//! Pictures that carry more than an image: an icon's SVG (svgBlip), a 3D model, an embedded
//! file (OLE package). Each is a picture object whose XML (`raw`) references the extra parts,
//! so the drawing writer keeps them and reading brings them back; Excel shows the picture.
//! Also renders shapes to a PNG (signature lines and other generated pictures).

use crate::book::Workbook;
use crate::drawing::{Anchor, ObjKind, Object, Rel, Shape};
use crate::xlsx::xml::{attr, part_text, Element, Node};
use std::sync::Arc;

pub const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub const RT_PACKAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";
pub const RT_MODEL3D: &str = "http://schemas.microsoft.com/office/2017/06/relationships/model3d";
const NS_SVG: &str = "http://schemas.microsoft.com/office/drawing/2016/SVG/main";
const URI_SVG: &str = "{96DAC541-7B7A-43D3-8B79-37D633B846F1}";
/// Our own extension for parts Excel has no picture markup for (models, packages).
const URI_JADWAL: &str = "{6D3B8A52-0E7C-4C3B-9A3F-4A6164776C01}";

/// A part to carry with a picture.
pub struct Extra { pub rel: &'static str, pub prefix: &'static str, pub ext: String, pub mime: String, pub bytes: Vec<u8> }

/// Adds a Default content type for an extension (creating [Content_Types].xml when missing).
pub fn ensure_content_type(wb: &mut Workbook, ext: &str, mime: &str) {
    let mut ct = wb.parts.get("[Content_Types].xml").and_then(|b| Element::parse(&part_text(b)).ok()).unwrap_or_else(|| Element { name: "Types".into(), attrs: vec![("xmlns".into(), "http://schemas.openxmlformats.org/package/2006/content-types".into())], children: vec![] });
    if ct.all("Default").any(|e| e.attr("Extension").is_some_and(|x| x.eq_ignore_ascii_case(ext))) { return; }
    ct.children.insert(0, Node::Element(Element { name: "Default".into(), attrs: vec![("Extension".into(), ext.into()), ("ContentType".into(), mime.into())], children: vec![] }));
    wb.parts.insert("[Content_Types].xml".into(), ct.to_xml().into_bytes());
}

fn free_part(wb: &Workbook, prefix: &str, ext: &str) -> String { (1..).map(|i| format!("{prefix}{i}.{ext}")).find(|n| !wb.parts.contains_key(n)).unwrap() }

/// The id `drawing::add_object` will give the next object.
pub fn next_id(objects: &[Object]) -> u32 { objects.iter().map(|x| x.id).max().unwrap_or(1) + 1 }

/// A PNG picture carrying extra parts, ready for `drawing::add_object` on sheet `si`.
pub fn picture_with_parts(wb: &mut Workbook, si: usize, png: Vec<u8>, anchor: Anchor, size: (f32, f32), base: &str, descr: &str, extras: Vec<Extra>) -> Object {
    let id = next_id(&wb.sheets[si].objects);
    let name = format!("{base} {}", id - 1);
    ensure_content_type(wb, "png", "image/png");
    let img = free_part(wb, "xl/media/image", "png");
    wb.parts.insert(img.clone(), png.clone());
    let mut rels = vec![Rel { id: "rId1".into(), kind: RT_IMAGE.into(), target: img, external: false }];
    let mut exts = String::new();
    let mut own = String::new();
    for (k, e) in extras.into_iter().enumerate() {
        let rid = format!("rId{}", k + 2);
        ensure_content_type(wb, &e.ext, &e.mime);
        let p = free_part(wb, e.prefix, &e.ext);
        wb.parts.insert(p.clone(), e.bytes);
        if e.ext == "svg" { exts.push_str(&format!("<a:ext uri=\"{URI_SVG}\"><asvg:svgBlip xmlns:asvg=\"{NS_SVG}\" r:embed=\"{rid}\"/></a:ext>")); }
        else { own.push_str(&format!("<jd:part xmlns:jd=\"urn:jadwal:parts\" kind=\"{}\" r:id=\"{rid}\"/>", attr(e.rel))); }
        rels.push(Rel { id: rid, kind: e.rel.into(), target: p, external: false });
    }
    if !own.is_empty() { exts.push_str(&format!("<a:ext uri=\"{URI_JADWAL}\">{own}</a:ext>")); }
    let ext_lst = if exts.is_empty() { String::new() } else { format!("<a:extLst>{exts}</a:extLst>") };
    let (w, h) = size;
    let emu = |v: f32| (v as f64 * crate::drawing::EMU as f64).round() as i64;
    let xml = format!("<xdr:pic xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"{}\"{}/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"rId1\">{ext_lst}</a:blip><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>",
        attr(&name), if descr.is_empty() { String::new() } else { format!(" descr=\"{}\"", attr(descr)) }, emu(w), emu(h));
    let raw = Element::parse(&xml).ok().map(Arc::new);
    let mut o = Object::new(ObjKind::Picture { data: Arc::new(png), ext: "png".into() }, anchor);
    o.name = name;
    o.descr = descr.into();
    o.raw = raw;
    o.rels = rels;
    o
}

/// The bytes and part name of a picture's carried part of a relationship type (svg: the svgBlip).
pub fn carried_part(wb: &Workbook, o: &Object, rel: &str) -> Option<(String, Vec<u8>)> {
    o.rels.iter().filter(|r| !r.external && (r.kind == rel || (rel == "svg" && r.kind == RT_IMAGE && r.target.ends_with(".svg")))).find_map(|r| wb.parts.get(&r.target).map(|b| (r.target.clone(), b.clone())))
}

/// Shapes laid out in a w × h picture, rendered to PNG at 2× (signature lines...).
pub fn shapes_png(shapes: &[(Shape, [f32; 4])], w: f32, h: f32) -> Option<Vec<u8>> {
    let wb = Workbook::new();
    let mut assets = crate::print::objects::Assets::new();
    let mut items = vec![];
    for (s, r) in shapes {
        let o = Object::new(ObjKind::Shape(s.clone()), Anchor::Abs { x: r[0], y: r[1], w: r[2] - r[0], h: r[3] - r[1] });
        crate::print::objects::draw_object(&wb, 0, &o, *r, 1.0, &mut assets, &mut items);
    }
    svg_png(&items, w, h)
}

/// Draw items rendered to a PNG at 2×.
pub fn svg_png(items: &[word_engine::layout::DrawItem], w: f32, h: f32) -> Option<Vec<u8>> {
    let refs: Vec<&word_engine::layout::DrawItem> = items.iter().collect();
    let svg = word_engine::svg::items_svg(None, &refs, w, h, 0.0, 0.0);
    rasterize_svg(svg.as_bytes(), (w * 2.0).ceil() as u32, (h * 2.0).ceil() as u32)
}

/// An SVG rasterized to a PNG of the given pixel size (resvg).
pub fn rasterize_svg(svg: &[u8], pw: u32, ph: u32) -> Option<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let mut pm = resvg::tiny_skia::Pixmap::new(pw.max(1), ph.max(1))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(pw as f32 / size.width(), ph as f32 / size.height()), &mut pm.as_mut());
    pm.encode_png().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_picture_round_trips() {
        let mut wb = Workbook::new();
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="red"/></svg>"#.to_vec();
        let png = rasterize_svg(&svg, 20, 20).unwrap();
        let anchor = Anchor::One { from: Default::default(), w: 40.0, h: 40.0 };
        let o = picture_with_parts(&mut wb, 0, png, anchor.clone(), (40.0, 40.0), "Graphic", "icon", vec![Extra { rel: RT_IMAGE, prefix: "xl/media/image", ext: "svg".into(), mime: "image/svg+xml".into(), bytes: svg.clone() }]);
        let m = picture_with_parts(&mut wb, 0, rasterize_svg(&svg, 8, 8).unwrap(), anchor, (40.0, 40.0), "3D Model", "m", vec![Extra { rel: RT_MODEL3D, prefix: "xl/models/model3d", ext: "glb".into(), mime: "model/gltf.binary".into(), bytes: vec![1, 2, 3] }]);
        crate::drawing::add_object(&mut wb.sheets[0].objects, o, "Graphic");
        crate::drawing::add_object(&mut wb.sheets[0].objects, m, "3D Model");
        let bytes = crate::xlsx::write(&wb).unwrap();
        let back = crate::xlsx::read(&bytes).unwrap();
        let objs = &back.sheets[0].objects;
        assert_eq!(objs.len(), 2);
        assert!(matches!(objs[0].kind, ObjKind::Picture { .. }));
        assert_eq!(carried_part(&back, &objs[0], "svg").unwrap().1, svg);
        assert_eq!(carried_part(&back, &objs[1], RT_MODEL3D).unwrap().1, vec![1, 2, 3]);
        let ct = String::from_utf8_lossy(&back.parts["[Content_Types].xml"]).to_string();
        assert!(ct.contains("svg"), "{ct}");
    }

    #[test]
    fn shapes_render_to_png() {
        let s = Shape { preset: "star5".into(), fill: Some([200, 0, 0]), text: "Hi".into(), size: 11.0, ..Default::default() };
        let png = shapes_png(&[(s, [0.0, 0.0, 50.0, 50.0])], 50.0, 50.0).unwrap();
        assert!(image::load_from_memory(&png).is_ok());
    }
}
