//! 3D models as Excel saves them ([MS-ODRAWXML] 2.6 model3d): an `mc:AlternateContent`
//! Requires="am3d" graphic frame holding `am3d:model3d` (the .glb part, camera, transform with
//! the rotation, the rendered raster, viewport and light), with a picture fallback. In Jadwal a
//! 3D model is a picture carrying the model part (embed::RT_MODEL3D) whose description is
//! `model3d::TAG` + "yaw,pitch name"; the rotation is am3d:rot (ay = yaw, ax = pitch, in
//! 60000ths of a degree). Only glTF binary models are written natively (Excel reads no other).

use super::native_objects::Sink;
use super::xml::{attr, Element, Node};
use crate::book::Workbook;
use crate::drawing::{ObjKind, Object, Rel};
use crate::embed::RT_MODEL3D;
use std::sync::Arc;

pub const NS_AM3D: &str = "http://schemas.microsoft.com/office/drawing/2017/model3d";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const CT_GLB: &str = "model/gltf.binary";

fn emu(px: f32) -> i64 { (px as f64 * crate::drawing::EMU as f64).round() as i64 }

/// The yaw, pitch and file name of a model picture's description.
fn view(descr: &str) -> Option<(f32, f32, String)> {
    let (y, p) = crate::model3d::view_of(descr)?;
    let name = descr.strip_prefix(crate::model3d::TAG)?.split_once(' ').map(|x| x.1.to_string()).unwrap_or_default();
    Some((y, p, name))
}

/// Makes .glb parts typed as Excel types them.
fn glb_type(w: &mut Workbook) {
    let Some(mut ct) = w.parts.get("[Content_Types].xml").and_then(|b| Element::parse(&super::xml::part_text(b)).ok()) else { return };
    for c in ct.children.iter_mut() {
        if let Node::Element(e) = c {
            if e.local() == "Default" && e.attr("Extension").is_some_and(|x| x.eq_ignore_ascii_case("glb")) {
                for (k, v) in e.attrs.iter_mut() { if k == "ContentType" { *v = CT_GLB.into(); } }
            }
        }
    }
    w.parts.insert("[Content_Types].xml".into(), ct.to_xml().into_bytes());
}

/// The native frame of a 3D model picture.
pub fn write(w: &mut Workbook, o: &Object, sink: &mut Sink) -> Option<String> {
    let (yaw, pitch, _) = view(&o.descr)?;
    let ObjKind::Picture { data, ext } = &o.kind else { return None };
    let model = o.rels.iter().find(|r| r.kind == RT_MODEL3D && !r.external && r.target.to_ascii_lowercase().ends_with(".glb") && w.parts.contains_key(&r.target))?.target.clone();
    let img = match o.rels.iter().find(|r| r.kind == RT_IMAGE && !r.external && w.parts.get(&r.target).is_some_and(|b| b.as_slice() == data.as_slice())) {
        Some(r) => r.target.clone(),
        None => {
            let p = (sink.free)("xl/media/image", &format!(".{ext}"));
            w.parts.insert(p.clone(), (**data).clone());
            sink.defaults.push((ext.clone(), if ext == "png" { "image/png".into() } else { format!("image/{ext}") }));
            p
        }
    };
    if !w.parts.get("[Content_Types].xml").is_some_and(|b| String::from_utf8_lossy(b).contains(CT_GLB)) { glb_type(w); }
    sink.defaults.push(("glb".into(), CT_GLB.into()));
    let mid = (sink.add_rel)(RT_MODEL3D, model, false);
    let pid = (sink.add_rel)(RT_IMAGE, img, false);
    let (cx, cy) = match &o.anchor { crate::drawing::Anchor::One { w, h, .. } | crate::drawing::Anchor::Abs { w, h, .. } => (emu(*w), emu(*h)), _ => (2286000, 2286000) };
    let ang = |d: f32| ((d as f64).rem_euclid(360.0) * 60000.0).round() as i64;
    let descr = if o.descr.is_empty() { String::new() } else { format!(" descr=\"{}\"", attr(&o.descr)) };
    let cnv = format!("<xdr:cNvPr id=\"{}\" name=\"{}\"{descr}/>", o.id, attr(&o.name));
    let xfrm = format!("<a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>");
    Some(format!("<mc:AlternateContent xmlns:mc=\"{NS_MC}\"><mc:Choice xmlns:am3d=\"{NS_AM3D}\" Requires=\"am3d\"><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr>{cnv}<xdr:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></xdr:cNvGraphicFramePr></xdr:nvGraphicFramePr>\
<xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{NS_AM3D}\"><am3d:model3d r:embed=\"{mid}\"><am3d:spPr>{xfrm}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></am3d:spPr>\
<am3d:camera><am3d:pos x=\"0\" y=\"0\" z=\"67740115\"/><am3d:up dx=\"0\" dy=\"36000000\" dz=\"0\"/><am3d:lookAt x=\"0\" y=\"0\" z=\"0\"/><am3d:perspective fov=\"2700000\"/></am3d:camera>\
<am3d:trans><am3d:meterPerModelUnit n=\"1000000\" d=\"1000000\"/><am3d:preTrans dx=\"0\" dy=\"0\" dz=\"0\"/><am3d:scale><am3d:sx n=\"1000000\" d=\"1000000\"/><am3d:sy n=\"1000000\" d=\"1000000\"/><am3d:sz n=\"1000000\" d=\"1000000\"/></am3d:scale><am3d:rot ax=\"{}\" ay=\"{}\" az=\"0\"/><am3d:postTrans dx=\"0\" dy=\"0\" dz=\"0\"/></am3d:trans>\
<am3d:raster rName=\"Office3DRenderer\" rVer=\"16.0.8326\"><am3d:blip r:embed=\"{pid}\"/></am3d:raster><am3d:objViewport viewportSz=\"{}\"/>\
<am3d:ambientLight><am3d:clr><a:scrgbClr r=\"50000\" g=\"50000\" b=\"50000\"/></am3d:clr><am3d:illuminance n=\"500000\" d=\"1000000\"/></am3d:ambientLight></am3d:model3d></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice>\
<mc:Fallback><xdr:pic><xdr:nvPicPr>{cnv}<xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{pid}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr>{xfrm}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic></mc:Fallback></mc:AlternateContent>",
        ang(pitch), ang(yaw), cx.max(cy)))
}

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }
fn rid(e: &Element) -> Option<&str> { e.attrs.iter().find(|(k, _)| k.ends_with(":embed")).map(|(_, v)| v.as_str()) }

/// Whether drawing content is a 3D model frame.
pub fn is_model(content: &Element) -> bool { find(content, "model3d").is_some() }

/// A read 3D model frame as the model picture.
pub fn read(wb: &Workbook, content: &Element, o: &mut Object) -> bool {
    let Some(m) = find(content, "model3d") else { return false };
    let rel = |id: Option<&str>| id.and_then(|id| o.rels.iter().find(|r| r.id == id && !r.external).cloned());
    let Some(mr) = rel(rid(m)).filter(|r| wb.parts.contains_key(&r.target)) else { return false };
    // The raster, else the fallback picture.
    let pic = rel(m.child("raster").and_then(|r| r.child("blip")).and_then(rid)).or_else(|| rel(content.child("Fallback").and_then(|f| find(f, "blip")).and_then(rid)));
    let Some((pr, data)) = pic.and_then(|r| wb.parts.get(&r.target).filter(|b| !b.is_empty()).map(|b| (r, b.clone()))) else { return false };
    let rot = m.child("trans").and_then(|t| t.child("rot"));
    let a = |k: &str| rot.and_then(|r| r.attr_f64(k)).unwrap_or(0.0) / 60000.0;
    let norm = |d: f64| { let d = d.rem_euclid(360.0); if d > 180.0 { d - 360.0 } else { d } };
    let (yaw, pitch) = (norm(a("ay")), norm(a("ax")));
    let name = mr.target.rsplit('/').next().unwrap_or("model.glb").to_string();
    o.descr = match view(&o.descr) { Some((_, _, n)) if !n.is_empty() => format!("{}{yaw},{pitch} {n}", crate::model3d::TAG), _ => format!("{}{yaw},{pitch} {name}", crate::model3d::TAG) };
    o.kind = ObjKind::Picture { data: Arc::new(data), ext: pr.target.rsplit('.').next().unwrap_or("png").to_ascii_lowercase() };
    o.rels = vec![Rel { id: pr.id.clone(), kind: RT_IMAGE.into(), target: pr.target.clone(), external: false }, Rel { id: mr.id.clone(), kind: RT_MODEL3D.into(), target: mr.target.clone(), external: false }];
    o.raw = None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{add_object, Anchor, Mark};
    use crate::embed::{carried_part, picture_with_parts, Extra};
    use crate::xlsx::pkgcheck::{check_package, part, unzip};

    #[test]
    fn model_round_trips() {
        let mut wb = Workbook::new();
        let glb = b"glTF\x02\x00\x00\x00fake".to_vec();
        let png = crate::model3d::render_png(&crate::model3d::cube(), 32, 32, 30.0, 20.0).unwrap();
        let o = picture_with_parts(&mut wb, 0, png.clone(), Anchor::One { from: Mark { col: 2, row: 2, ..Default::default() }, w: 240.0, h: 240.0 }, (240.0, 240.0), "3D Model", &format!("{}30,20 cube.glb", crate::model3d::TAG), vec![Extra { rel: RT_MODEL3D, prefix: "xl/models/model3d", ext: "glb".into(), mime: "model/gltf-binary".into(), bytes: glb.clone() }]);
        add_object(&mut wb.sheets[0].objects, o, "3D Model");
        let bytes = crate::xlsx::write(&wb).unwrap();
        if let Ok(d) = std::env::var("JADWAL_DUMP") { std::fs::write(format!("{d}/model3d_parts.xlsx"), &bytes).unwrap(); }
        let files = unzip(&bytes);
        check_package(&files);
        let d = &files["xl/drawings/drawing1.xml"];
        assert!(d.contains("Requires=\"am3d\"") && d.contains("<am3d:rot ax=\"1200000\" ay=\"1800000\"") && d.contains("<mc:Fallback><xdr:pic>"), "{d}");
        assert!(files["[Content_Types].xml"].contains("model/gltf.binary"));
        let back = crate::xlsx::read(&bytes).unwrap();
        let ob = &back.sheets[0].objects[0];
        assert_eq!(crate::model3d::view_of(&ob.descr), Some((30.0, 20.0)));
        assert!(ob.descr.ends_with("cube.glb"));
        assert_eq!(carried_part(&back, ob, RT_MODEL3D).unwrap().1, glb);
        assert!(matches!(&ob.kind, ObjKind::Picture { data, .. } if data.as_slice() == png.as_slice()));
        let again = crate::xlsx::write(&back).unwrap();
        check_package(&unzip(&again));
        let _ = part(&again, "xl/drawings/drawing1.xml");
    }

    /// A drawing as Excel writes it: the alternate content wraps whole anchors.
    #[test]
    fn reads_excel_top_level_frames() {
        let mut wb = Workbook::new();
        wb.set_input(0, crate::Pos::new(0, 0), "x");
        let bytes = crate::xlsx::write(&wb).unwrap();
        let mut files: std::collections::BTreeMap<String, Vec<u8>> = { let z = unzip(&bytes); z.keys().map(|k| (k.clone(), part(&bytes, k))).collect() };
        let png = crate::model3d::render_png(&crate::model3d::cube(), 16, 16, 0.0, 0.0).unwrap();
        files.insert("xl/media/image1.png".into(), png.clone());
        files.insert("xl/model3d/model3d1.glb".into(), b"glTF".to_vec());
        let anchor = |inner: &str| format!("<xdr:twoCellAnchor editAs=\"oneCell\"><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>5</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>10</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{inner}<xdr:clientData/></xdr:twoCellAnchor>");
        let model = anchor("<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"2\" name=\"3D Model 1\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2017/model3d\"><am3d:model3d r:embed=\"rId2\"><am3d:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"10\" cy=\"10\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></am3d:spPr><am3d:camera><am3d:pos x=\"0\" y=\"0\" z=\"1\"/><am3d:up dx=\"0\" dy=\"1\" dz=\"0\"/><am3d:lookAt x=\"0\" y=\"0\" z=\"0\"/><am3d:perspective fov=\"2700000\"/></am3d:camera><am3d:trans><am3d:meterPerModelUnit n=\"1\" d=\"1\"/><am3d:preTrans dx=\"0\" dy=\"0\" dz=\"0\"/><am3d:scale><am3d:sx n=\"1\" d=\"1\"/><am3d:sy n=\"1\" d=\"1\"/><am3d:sz n=\"1\" d=\"1\"/></am3d:scale><am3d:rot ax=\"21000000\" ay=\"2700000\" az=\"0\"/><am3d:postTrans dx=\"0\" dy=\"0\" dz=\"0\"/></am3d:trans><am3d:raster rName=\"Office3DRenderer\" rVer=\"16.0.8326\"><am3d:blip r:embed=\"rId1\"/></am3d:raster></am3d:model3d></a:graphicData></a:graphic></xdr:graphicFrame>");
        let fallback = anchor("<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"2\" name=\"3D Model 1\"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"rId1\"/></xdr:blipFill><xdr:spPr/></xdr:pic>");
        let eq = anchor("<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"3\" name=\"TextBox 2\"/><xdr:cNvSpPr txBox=\"1\"/></xdr:nvSpPr><xdr:spPr/><xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a14:m><m:oMathPara xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:oMath><m:sSup><m:e><m:r><a:rPr lang=\"en-US\" i=\"1\"/><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath></m:oMathPara></a14:m><a:endParaRPr lang=\"en-US\"/></a:p></xdr:txBody></xdr:sp>");
        let eqfb = anchor("<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"3\" name=\"TextBox 2\"/><xdr:cNvSpPr txBox=\"1\"/></xdr:nvSpPr><xdr:spPr/><xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>x^2</a:t></a:r></a:p></xdr:txBody></xdr:sp>");
        let drawing = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:am3d=\"{NS_AM3D}\" Requires=\"am3d\">{model}</mc:Choice><mc:Fallback>{fallback}</mc:Fallback></mc:AlternateContent><mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\" Requires=\"a14\">{eq}</mc:Choice><mc:Fallback>{eqfb}</mc:Fallback></mc:AlternateContent></xdr:wsDr>");
        files.insert("xl/drawings/drawing1.xml".into(), drawing.into_bytes());
        files.insert("xl/drawings/_rels/drawing1.xml.rels".into(), format!("<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{RT_IMAGE}\" Target=\"../media/image1.png\"/><Relationship Id=\"rId2\" Type=\"{RT_MODEL3D}\" Target=\"../model3d/model3d1.glb\"/></Relationships>").into_bytes());
        files.insert("xl/worksheets/_rels/sheet1.xml.rels".into(), b"<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdD\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>".to_vec());
        let s = String::from_utf8(files["xl/worksheets/sheet1.xml"].clone()).unwrap();
        let s = s.replacen("</worksheet>", "<drawing r:id=\"rIdD\"/></worksheet>", 1);
        let s = if s.contains("xmlns:r=") { s } else { s.replacen("<worksheet ", "<worksheet xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ", 1) };
        files.insert("xl/worksheets/sheet1.xml".into(), s.into_bytes());
        let ct = String::from_utf8(files["[Content_Types].xml"].clone()).unwrap().replacen("<Default ", "<Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"glb\" ContentType=\"model/gltf.binary\"/><Override PartName=\"/xl/drawings/drawing1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/><Default ", 1);
        files.insert("[Content_Types].xml".into(), ct.into_bytes());
        let mut buf = std::io::Cursor::new(Vec::new());
        { use std::io::Write; let mut z = zip::ZipWriter::new(&mut buf); for (k, v) in &files { z.start_file(k.as_str(), zip::write::SimpleFileOptions::default()).unwrap(); z.write_all(v).unwrap(); } z.finish().unwrap(); }
        let back = crate::xlsx::read(&buf.into_inner()).unwrap();
        let objs = &back.sheets[0].objects;
        assert_eq!(objs.len(), 2, "{objs:?}");
        assert_eq!(crate::model3d::view_of(&objs[0].descr), Some((45.0, -10.0)));
        assert_eq!(carried_part(&back, &objs[0], RT_MODEL3D).unwrap().1, b"glTF".to_vec());
        assert_eq!(objs[1].descr, format!("{}x^2", super::super::equation_parts::EQ_TAG));
        assert!(matches!(objs[1].kind, ObjKind::Picture { .. }));
        // Written back natively.
        let files2 = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&files2);
        let d = &files2["xl/drawings/drawing1.xml"];
        assert!(d.contains("am3d:model3d") && d.contains("<a14:m>"));
    }
}
