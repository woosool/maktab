//! Inserted objects: charts (real chart parts, drawn with Qalam's chart renderer), SmartArt
//! (editable grouped shapes from indented text), equations (typeset by Qalam's equation
//! engine), photo albums, slide zooms, slides reused from another presentation, audio and
//! video, comments, and embedded files.

use crate::opc;
use crate::pres::{Presentation, NS_A, NS_P, NS_R};
use crate::scene::{Loc, Xfrm};
use crate::xml::{Element, Node};
use word_engine::layout::DrawItem;
use word_engine::objects::{Chart, ChartKind, Diagram, DiagramKind};

pub const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
pub const RT_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const URI_CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const RT_VIDEO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/video";
const RT_AUDIO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/audio";
const RT_MEDIA: &str = "http://schemas.microsoft.com/office/2007/relationships/media";
const RT_PACKAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";
const RT_COMMENTS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const RT_AUTHORS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/commentAuthors";

/// One CSS pixel in EMU (the unit of Qalam's object renderers).
pub const PX: f64 = 9_525.0;

/// Draw Qalam draw items (CSS px) with transform `t` (px → target pixels).
pub fn draw_items(pm: &mut tiny_skia::Pixmap, items: &[DrawItem], t: tiny_skia::Transform) {
    use tiny_skia::{FillRule, Paint, PathBuilder, Stroke};
    let col = |c: word_engine::Color| tiny_skia::Color::from_rgba8(c.0, c.1, c.2, 255);
    for it in items {
        match it {
            DrawItem::Rect { x, y, w, h, color } => {
                if let Some(r) = tiny_skia::Rect::from_xywh(*x, *y, w.max(0.01), h.max(0.01)) { let mut p = Paint::default(); p.set_color(col(*color)); pm.fill_rect(r, &p, t, None); }
            }
            DrawItem::Line { x1, y1, x2, y2, width, color } => {
                let mut pb = PathBuilder::new();
                pb.move_to(*x1, *y1);
                pb.line_to(*x2, *y2);
                if let Some(path) = pb.finish() { let mut p = Paint { anti_alias: true, ..Default::default() }; p.set_color(col(*color)); pm.stroke_path(&path, &p, &Stroke { width: width.max(0.5), ..Default::default() }, t, None); }
            }
            DrawItem::Path { points, closed, fill, stroke, .. } => {
                let mut pb = PathBuilder::new();
                for (i, (x, y)) in points.iter().enumerate() { if i == 0 { pb.move_to(*x, *y); } else { pb.line_to(*x, *y); } }
                if *closed { pb.close(); }
                if let Some(path) = pb.finish() {
                    if let Some(f) = fill { let mut p = Paint { anti_alias: true, ..Default::default() }; p.set_color(col(*f)); pm.fill_path(&path, &p, FillRule::Winding, t, None); }
                    if let Some((c, w)) = stroke { let mut p = Paint { anti_alias: true, ..Default::default() }; p.set_color(col(*c)); pm.stroke_path(&path, &p, &Stroke { width: w.max(0.5), line_join: tiny_skia::LineJoin::Round, ..Default::default() }, t, None); }
                }
            }
            DrawItem::Glyphs { glyphs, baseline, font, size, color, .. } => {
                let face = word_engine::fonts::ttf(font.face);
                let s = *size / face.units_per_em() as f32;
                let mut pb = PathBuilder::new();
                struct O<'a> { pb: &'a mut PathBuilder, x: f32, y: f32, s: f32 }
                impl ttf_parser::OutlineBuilder for O<'_> {
                    fn move_to(&mut self, x: f32, y: f32) { self.pb.move_to(self.x + x * self.s, self.y - y * self.s); }
                    fn line_to(&mut self, x: f32, y: f32) { self.pb.line_to(self.x + x * self.s, self.y - y * self.s); }
                    fn quad_to(&mut self, a: f32, b: f32, x: f32, y: f32) { self.pb.quad_to(self.x + a * self.s, self.y - b * self.s, self.x + x * self.s, self.y - y * self.s); }
                    fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) { self.pb.cubic_to(self.x + a * self.s, self.y - b * self.s, self.x + c * self.s, self.y - d * self.s, self.x + x * self.s, self.y - y * self.s); }
                    fn close(&mut self) { self.pb.close(); }
                }
                for g in glyphs { let mut o = O { pb: &mut pb, x: g.x, y: baseline - g.dy, s }; face.outline_glyph(ttf_parser::GlyphId(g.gid), &mut o); }
                if let Some(path) = pb.finish() { let mut p = Paint { anti_alias: true, ..Default::default() }; p.set_color(col(*color)); pm.fill_path(&path, &p, FillRule::Winding, t, None); }
            }
            _ => {}
        }
    }
}

fn frame_xml(id: u32, name: &str) -> Element {
    Element::new("p:nvGraphicFramePr").with_child(Element::new("p:cNvPr").with_attr("id", id.to_string()).with_attr("name", name)).with_child(Element::new("p:cNvGraphicFramePr").with_child(Element::new("a:graphicFrameLocks").with_attr("noGrp", "1"))).with_child(Element::new("p:nvPr"))
}

/// SmartArt layouts offered (kind, name).
pub const SMARTART: &[(DiagramKind, &str, &str)] = &[
    (DiagramKind::List, "List", "Basic Block List"), (DiagramKind::Process, "Process", "Basic Process"), (DiagramKind::Cycle, "Cycle", "Basic Cycle"),
    (DiagramKind::Hierarchy, "Hierarchy", "Organization Chart"), (DiagramKind::Pyramid, "Pyramid", "Basic Pyramid"),
];

impl Presentation {
    // ---- charts -------------------------------------------------------------------------

    /// The chart of a graphic frame, if it holds one.
    pub fn chart(&self, loc: &Loc) -> Option<Chart> {
        let sp = self.element(loc)?;
        let rid = sp.find("c:chart").and_then(|c| c.attr("r:id"))?;
        let part = self.pkg.rel_target(&loc.part, rid)?;
        let root = self.pkg.xml(&part)?;
        let x = self.xfrm(loc).unwrap_or_default();
        let w = word_engine::xml::parse(&root.to_xml()).ok()?;
        word_engine::objects::parse_chart(&w, (x.w / PX) as f32, (x.h / PX) as f32)
    }
    /// The chart in a chart part (for drawing frames inherited from layouts too).
    pub fn chart_part(&self, part: &str, rid: &str, w: f64, h: f64) -> Option<Chart> {
        let target = self.pkg.rel_target(part, rid)?;
        let root = word_engine::xml::parse(&self.pkg.xml(&target)?.to_xml()).ok()?;
        word_engine::objects::parse_chart(&root, (w / PX) as f32, (h / PX) as f32)
    }

    /// Insert a chart with sample data.
    pub fn insert_chart(&mut self, part: &str, kind: ChartKind, x: &Xfrm) -> Option<Loc> {
        let chart = Chart::sample(kind);
        let name = self.pkg.free_name("ppt/charts/chart", ".xml");
        let root = Element::parse(&word_engine::objects::chart_part_xml(&chart)).ok()?;
        self.pkg.put_xml(&name, root, Some(CT_CHART));
        let rid = self.pkg.add_rel(part, RT_CHART, &name, false);
        let id = self.next_shape_id(part);
        let frame = Element::new("p:graphicFrame").with_child(frame_xml(id, &format!("Chart {}", id - 1))).with_child(Element::new("p:xfrm"))
            .with_child(Element::new("a:graphic").with_child(Element::new("a:graphicData").with_attr("uri", URI_CHART).with_child(Element::new("c:chart").with_attr("xmlns:c", URI_CHART).with_attr("xmlns:r", NS_R).with_attr("r:id", rid))));
        let loc = self.insert_element(part, frame)?;
        self.set_xfrm(&loc, x);
        Some(loc)
    }
    /// Replace a chart's data, type and title.
    pub fn set_chart(&mut self, loc: &Loc, chart: &Chart) {
        let Some(rid) = self.element(loc).and_then(|sp| sp.find("c:chart")).and_then(|c| c.attr("r:id")).map(String::from) else { return };
        let Some(part) = self.pkg.rel_target(&loc.part, &rid) else { return };
        if let Ok(root) = Element::parse(&word_engine::objects::chart_part_xml(chart)) { self.pkg.put_xml(&part, root, Some(CT_CHART)); }
    }

    // ---- SmartArt -------------------------------------------------------------------------

    /// Insert a SmartArt graphic as a group of editable shapes; its text (one item per
    /// line, tabs for levels) is kept in the group's description so it can be edited again.
    pub fn insert_smartart(&mut self, part: &str, kind: DiagramKind, items: &[(u8, String)], x: &Xfrm, accent: &str) -> Option<Loc> {
        let shapes = smartart_shapes(kind, items, x);
        if shapes.is_empty() { return None; }
        let mut locs = vec![];
        let mut frames = vec![];
        for (preset, f, text, level) in &shapes {
            let loc = self.insert_shape(part, preset, f, false)?;
            if !text.is_empty() { self.set_shape_text(&loc, None, text); }
            if preset == "line" || preset == "straightConnector1" { self.set_line(&loc, Some(&shade(accent, 0.7)), Some(1.5), None); }
            else {
                let fill = if *level == 0 { accent.to_string() } else { tint(accent, 0.25 * *level as f64) };
                self.set_fill(&loc, Some(&fill));
                self.set_line(&loc, Some("FFFFFF"), Some(1.0), None);
                self.format_shape_text(&loc, None, &crate::edit::RunFormat { color: Some("FFFFFF".into()), ..Default::default() });
            }
            locs.push(loc);
            frames.push(*f);
        }
        let g = self.group(&locs, &frames)?;
        let spec = Diagram { kind, items: items.to_vec(), width: 0.0, height: 0.0, color: word_engine::Color::from_hex(accent).unwrap_or(word_engine::Color::BLACK) }.to_spec();
        if let Some(c) = self.element_mut(&g).and_then(|e| e.path_mut("p:nvGrpSpPr/p:cNvPr")) { c.set_attr("descr", spec); c.set_attr("name", "SmartArt"); }
        Some(g)
    }
    /// The SmartArt spec of a group made by `insert_smartart`.
    pub fn smartart(&self, loc: &Loc) -> Option<Diagram> {
        let d = self.element(loc)?.path("p:nvGrpSpPr/p:cNvPr")?.attr("descr")?;
        Diagram::from_spec(d, 0.0, 0.0)
    }
    /// Rebuild a SmartArt group from new items or layout, in the same place.
    pub fn update_smartart(&mut self, loc: &Loc, kind: DiagramKind, items: &[(u8, String)], accent: &str) -> Option<Loc> {
        let x = self.xfrm(loc)?;
        self.delete_shapes(std::slice::from_ref(loc));
        let g = self.insert_smartart(&loc.part, kind, items, &x, accent)?;
        // Back to the same z position.
        let mut cur = g;
        while cur.path[0] > loc.path[0] { match self.reorder(&cur, crate::edit::Order::Backward) { Some(n) if n != cur => cur = n, _ => break } }
        Some(cur)
    }

    // ---- equations, pictures of objects --------------------------------------------------

    /// Insert an equation (linear format, as Qalam types them) as a picture that remembers
    /// its source for editing.
    pub fn insert_equation(&mut self, part: &str, linear: &str, size_pt: f32, center: (f64, f64)) -> Option<Loc> {
        let mut eq = word_engine::equation::Equation::new(linear);
        eq.size = size_pt;
        eq.display = true;
        let png = word_engine::objects::object_png(&word_engine::objects::Object::Equation(eq.clone()))?;
        let (w, h, _) = eq.metrics();
        let (w, h) = (w as f64 * PX, h as f64 * PX);
        let loc = self.insert_picture(part, png, "png", &Xfrm { x: center.0 - w / 2.0, y: center.1 - h / 2.0, w, h, ..Default::default() }, &format!("qalam-equation:{linear}"))?;
        Some(loc)
    }
    /// The equation source of a picture inserted by `insert_equation`.
    pub fn equation(&self, loc: &Loc) -> Option<String> {
        self.element(loc)?.path("p:nvPicPr/p:cNvPr")?.attr("descr")?.strip_prefix("qalam-equation:").map(String::from)
    }

    // ---- photo album, zoom, reuse ---------------------------------------------------------

    /// A photo album: a title slide, then the pictures (1, 2 or 4 per slide), fitted.
    pub fn photo_album(&mut self, pictures: &[(Vec<u8>, String, u32, u32)], per_slide: usize, captions: bool) -> usize {
        let layouts = self.all_layouts();
        let blank = layouts.iter().find(|l| self.layout_type(l) == "blank").or(layouts.last()).cloned();
        let title = layouts.iter().find(|l| self.layout_type(l) == "title").cloned();
        let mut count = 0;
        let at = self.slides().len();
        if let Some(t) = &title {
            let s = self.add_slide(t, at);
            let scene = self.scene(&s);
            for it in scene.items.iter().filter(|i| i.own) {
                if let Some(ph) = &it.placeholder { if ph.0 == "ctrTitle" { self.set_shape_text(&it.loc, None, "Photo Album"); } }
            }
            count += 1;
        }
        let Some(blank) = blank else { return count };
        let (sw, sh) = self.size();
        let per = per_slide.clamp(1, 4);
        let cols = if per == 1 { 1 } else { 2 };
        let rows = if per <= 2 { 1 } else { 2 };
        for chunk in pictures.chunks(per) {
            let s = self.add_slide(&blank, self.slides().len());
            count += 1;
            let (cw, ch) = (sw / cols as f64, sh / rows as f64);
            for (k, (bytes, ext, iw, ih)) in chunk.iter().enumerate() {
                let (cx, cy) = ((k % cols) as f64 * cw, (k / cols) as f64 * ch);
                let cap_h = if captions { 450_000.0 } else { 0.0 };
                let (bw, bh) = (cw * 0.88, (ch - cap_h) * 0.88);
                let scale = (bw / (*iw as f64 * PX)).min(bh / (*ih as f64 * PX));
                let (w, h) = (*iw as f64 * PX * scale, *ih as f64 * PX * scale);
                let x = Xfrm { x: cx + (cw - w) / 2.0, y: cy + (ch - cap_h - h) / 2.0, w, h, ..Default::default() };
                self.insert_picture(&s, bytes.clone(), ext, &x, "Photo");
                if captions {
                    if let Some(tb) = self.insert_shape(&s, "rect", &Xfrm { x: cx, y: cy + ch - cap_h, w: cw, h: cap_h, ..Default::default() }, true) {
                        self.set_shape_text(&tb, None, "Caption");
                        self.set_align(&tb, None, None, "ctr");
                    }
                }
            }
        }
        count
    }

    /// A slide zoom: a picture of slide `target` that jumps to it when clicked.
    pub fn insert_slide_zoom(&mut self, part: &str, target: usize, png: Vec<u8>, x: &Xfrm) -> Option<Loc> {
        let loc = self.insert_picture(part, png, "png", x, &format!("Slide Zoom {}", target + 1))?;
        self.set_shape_link(&loc, Some(&format!("#{}", target + 1)));
        self.set_line(&loc, Some("BFBFBF"), Some(0.75), None);
        Some(loc)
    }

    /// Copy slide `index` of another presentation into this one at `at`, with its pictures
    /// and media; it takes this presentation's layout of the same type (or name), else the
    /// first. `keep_source` keeps its look by copying its background onto the slide.
    pub fn reuse_slide(&mut self, other: &Presentation, index: usize, at: usize, keep_source: bool) -> Option<String> {
        let src = other.slides().get(index)?.clone();
        let mut root = other.pkg.xml(&src)?.clone();
        let src_layout = other.layout_of(&src);
        let (lt, ln) = src_layout.as_deref().map(|l| (other.layout_type(l), other.layout_name(l))).unwrap_or_default();
        let layouts = self.all_layouts();
        let layout = layouts.iter().find(|l| self.layout_type(l) == lt && lt != "cust").or_else(|| layouts.iter().find(|l| self.layout_name(l) == ln)).or(layouts.first())?.clone();
        if keep_source && root.path("p:cSld/p:bg").is_none() {
            // The source's resolved background, as a solid color when it was one.
            if let crate::scene::Fill::Solid(c) = other.scene(&src).background {
                if let Some(csld) = root.child_mut("p:cSld") { csld.children.insert(0, Node::Element(Element::parse(&format!(r#"<p:bg><p:bgPr><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>"#, c.to_hex())).ok()?)); }
            }
        }
        let part = self.pkg.free_name("ppt/slides/slide", ".xml");
        // Relationships: pictures, media and charts are copied; slide links dropped.
        let mut rel_map: Vec<(String, String)> = vec![];
        for r in other.pkg.rels(&src) {
            if r.kind == opc::RT_LAYOUT || r.kind == opc::RT_NOTES || r.kind == opc::RT_SLIDE { continue; }
            let new_id = if r.external { self.pkg.add_rel(&part, &r.kind, &r.target, true) } else {
                let target = opc::resolve(&src, &r.target);
                let Some(bytes) = other.pkg.bytes(&target) else { continue };
                let ext = target.rsplit('.').next().unwrap_or("bin").to_string();
                let dir = target.rsplit_once('/').map(|(d, _)| d).unwrap_or("ppt/media");
                let file = target.rsplit('/').next().unwrap_or("part");
                let stem = file.trim_end_matches(&format!(".{ext}")).trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
                let name = self.pkg.free_name(&format!("{dir}/{stem}"), &format!(".{ext}"));
                match other.pkg.parts.get(&target) {
                    Some(opc::PartData::Xml(d)) => { let ct = other.pkg.content_type(&target); self.pkg.put_xml(&name, d.root.clone(), ct.as_deref()); }
                    _ => self.pkg.put_bytes(&name, bytes.to_vec()),
                }
                self.pkg.add_rel(&part, &r.kind, &name, false)
            };
            rel_map.push((r.id.clone(), new_id));
        }
        // Rewrite r:id/r:embed/r:link attributes to the new ids.
        fn remap(e: &mut Element, map: &[(String, String)]) {
            for (k, v) in e.attrs.iter_mut() { if k.starts_with("r:") { if let Some((_, n)) = map.iter().find(|(o, _)| o == v) { *v = n.clone(); } } }
            for c in e.elements_mut() { remap(c, map); }
        }
        remap(&mut root, &rel_map);
        self.pkg.put_xml(&part, root, Some(opc::CT_SLIDE));
        self.pkg.add_rel(&part, opc::RT_LAYOUT, &layout, false);
        self.add_slide_entry(&part, at);
        Some(part)
    }

    // ---- media ------------------------------------------------------------------------------

    /// Embed a video or audio file, shown by a poster picture (PNG bytes).
    pub fn insert_media(&mut self, part: &str, bytes: Vec<u8>, ext: &str, poster: Vec<u8>, x: &Xfrm, video: bool) -> Option<Loc> {
        let media = self.pkg.free_name("ppt/media/media", &format!(".{}", ext.to_ascii_lowercase()));
        self.pkg.put_bytes(&media, bytes);
        let mime = match ext.to_ascii_lowercase().as_str() { "mp4" | "m4v" => "video/mp4", "mov" => "video/quicktime", "wmv" => "video/x-ms-wmv", "avi" => "video/x-msvideo", "mp3" => "audio/mpeg", "m4a" => "audio/mp4", "wav" => "audio/wav", "wma" => "audio/x-ms-wma", _ => if video { "video/unknown" } else { "audio/unknown" } };
        if let Some(ct) = self.pkg.xml_mut("[Content_Types].xml") {
            let e = ext.to_ascii_lowercase();
            if !ct.elements().any(|d| d.attr("Extension").is_some_and(|x| x.eq_ignore_ascii_case(&e))) { ct.children.insert(0, Node::Element(Element::new("Default").with_attr("Extension", e).with_attr("ContentType", mime))); }
        }
        let link = self.pkg.add_rel(part, if video { RT_VIDEO } else { RT_AUDIO }, &media, false);
        let embed = self.pkg.add_rel(part, RT_MEDIA, &media, false);
        let img = self.pkg.free_name("ppt/media/image", ".png");
        self.pkg.put_bytes(&img, poster);
        let pic_rid = self.pkg.add_rel(part, opc::RT_IMAGE, &img, false);
        let id = self.next_shape_id(part);
        let file = Element::new(if video { "a:videoFile" } else { "a:audioFile" }).with_attr("r:link", link);
        let ext_el = Element::parse(&format!(r#"<p:extLst><p:ext uri="{{DAA4B4D4-6D71-4841-9C94-3DE7FCFB9230}}"><p14:media xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main" r:embed="{embed}"/></p:ext></p:extLst>"#)).ok()?;
        let pic = Element::new("p:pic")
            .with_child(Element::new("p:nvPicPr").with_child(Element::new("p:cNvPr").with_attr("id", id.to_string()).with_attr("name", if video { "Video" } else { "Audio" }).with_child(Element::new("a:hlinkClick").with_attr("r:id", "").with_attr("action", "ppaction://media"))).with_child(Element::new("p:cNvPicPr").with_child(Element::new("a:picLocks").with_attr("noChangeAspect", "1"))).with_child(Element::new("p:nvPr").with_child(file).with_child(ext_el)))
            .with_child(Element::new("p:blipFill").with_child(Element::new("a:blip").with_attr("r:embed", pic_rid)).with_child(Element::new("a:stretch").with_child(Element::new("a:fillRect"))))
            .with_child(Element::new("p:spPr").with_child(Element::new("a:prstGeom").with_attr("prst", "rect").with_child(Element::new("a:avLst"))));
        let loc = self.insert_element(part, pic)?;
        self.set_xfrm(&loc, x);
        Some(loc)
    }
    /// The media part a picture plays, if it's audio or video.
    pub fn media_part(&self, loc: &Loc) -> Option<String> {
        let sp = self.element(loc)?;
        let f = sp.find("a:videoFile").or_else(|| sp.find("a:audioFile"))?;
        self.pkg.rel_target(&loc.part, f.attr("r:link")?)
    }

    /// Embed a file (Insert ▸ Object ▸ Create from file), shown by an icon picture.
    pub fn insert_object_file(&mut self, part: &str, bytes: Vec<u8>, file_name: &str, icon_png: Vec<u8>, x: &Xfrm) -> Option<Loc> {
        let ext = file_name.rsplit('.').next().unwrap_or("bin").to_ascii_lowercase();
        let emb = self.pkg.free_name("ppt/embeddings/oleObject", &format!(".{ext}"));
        self.pkg.put_bytes(&emb, bytes);
        if let Some(ct) = self.pkg.xml_mut("[Content_Types].xml") {
            if !ct.elements().any(|d| d.attr("Extension").is_some_and(|x| x.eq_ignore_ascii_case(&ext))) { ct.children.insert(0, Node::Element(Element::new("Default").with_attr("Extension", ext.clone()).with_attr("ContentType", "application/octet-stream"))); }
        }
        let rid = self.pkg.add_rel(part, RT_PACKAGE, &emb, false);
        let loc = self.insert_picture(part, icon_png, "png", x, &format!("qalam-object:{rid}:{file_name}"))?;
        Some(loc)
    }
    /// An embedded file's name and bytes (pictures inserted by `insert_object_file`).
    pub fn object_file(&self, loc: &Loc) -> Option<(String, std::sync::Arc<Vec<u8>>)> {
        let d = self.element(loc)?.path("p:nvPicPr/p:cNvPr")?.attr("descr")?.strip_prefix("qalam-object:")?.to_string();
        let (rid, name) = d.split_once(':')?;
        let part = self.pkg.rel_target(&loc.part, rid)?;
        Some((name.to_string(), self.pkg.bytes(&part)?))
    }

    // ---- comments ---------------------------------------------------------------------------

    /// Comments on a slide: (author, text, date, x, y in EMU).
    pub fn comments(&self, slide: &str) -> Vec<(String, String, String, f64, f64)> {
        let Some(part) = self.pkg.related(slide, RT_COMMENTS) else { return vec![] };
        let authors = self.comment_authors();
        let Some(root) = self.pkg.xml(&part) else { return vec![] };
        root.children_named("p:cm").map(|c| {
            let author = c.attr("authorId").and_then(|a| authors.iter().find(|x| x.0 == a)).map(|x| x.1.clone()).unwrap_or_default();
            let pos = c.child("p:pos");
            // Legacy comment positions are in 1/8 points? They're in "master units" (576 per inch).
            let k = 914_400.0 / 576.0;
            (author, c.child("p:text").map(|t| t.text()).unwrap_or_default(), c.attr("dt").unwrap_or("").to_string(), pos.and_then(|p| p.attr_f64("x")).unwrap_or(0.0) * k, pos.and_then(|p| p.attr_f64("y")).unwrap_or(0.0) * k)
        }).collect()
    }
    fn comment_authors(&self) -> Vec<(String, String)> {
        self.pkg.related(&self.main, RT_AUTHORS).and_then(|p| self.pkg.xml(&p)).map(|r| r.children_named("p:cmAuthor").filter_map(|a| Some((a.attr("id")?.to_string(), a.attr("name")?.to_string()))).collect()).unwrap_or_default()
    }
    /// Add a comment to a slide at (x, y) EMU.
    pub fn add_comment(&mut self, slide: &str, author: &str, text: &str, x: f64, y: f64) {
        let main = self.main.clone();
        let authors_part = match self.pkg.related(&main, RT_AUTHORS) {
            Some(p) => p,
            None => {
                let p = "ppt/commentAuthors.xml".to_string();
                self.pkg.put_xml(&p, Element::new("p:cmAuthorLst").with_attr("xmlns:a", NS_A).with_attr("xmlns:r", NS_R).with_attr("xmlns:p", NS_P), Some("application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml"));
                self.pkg.add_rel(&main, RT_AUTHORS, &p, false);
                p
            }
        };
        let (aid, idx) = {
            let lst = self.pkg.xml_mut(&authors_part).unwrap();
            let found = lst.children_named("p:cmAuthor").find(|a| a.attr("name") == Some(author)).map(|a| a.attr("id").unwrap_or("0").to_string());
            match found {
                Some(id) => {
                    let a = lst.elements_mut().find(|a| a.attr("id") == Some(id.as_str())).unwrap();
                    let n = a.attr_i64("lastIdx").unwrap_or(0) + 1;
                    a.set_attr("lastIdx", n.to_string());
                    (id, n)
                }
                None => {
                    let id = lst.elements().count().to_string();
                    let initials: String = author.split_whitespace().filter_map(|w| w.chars().next()).collect();
                    lst.children.push(Node::Element(Element::new("p:cmAuthor").with_attr("id", id.clone()).with_attr("name", author).with_attr("initials", initials).with_attr("lastIdx", "1").with_attr("clrIdx", id.clone())));
                    (id, 1)
                }
            }
        };
        let part = match self.pkg.related(slide, RT_COMMENTS) {
            Some(p) => p,
            None => {
                let p = self.pkg.free_name("ppt/comments/comment", ".xml");
                self.pkg.put_xml(&p, Element::new("p:cmLst").with_attr("xmlns:a", NS_A).with_attr("xmlns:r", NS_R).with_attr("xmlns:p", NS_P), Some("application/vnd.openxmlformats-officedocument.presentationml.comments+xml"));
                self.pkg.add_rel(slide, RT_COMMENTS, &p, false);
                p
            }
        };
        let k = 576.0 / 914_400.0;
        let dt = format!("{}T12:00:00.000", crate::format::today_as("iso"));
        let cm = Element::new("p:cm").with_attr("authorId", aid).with_attr("dt", dt).with_attr("idx", idx.to_string())
            .with_child(Element::new("p:pos").with_attr("x", ((x * k) as i64).to_string()).with_attr("y", ((y * k) as i64).to_string()))
            .with_child(Element::new("p:text").with_text(text));
        self.pkg.xml_mut(&part).unwrap().children.push(Node::Element(cm));
    }
    /// Delete a slide's comment by index.
    pub fn delete_comment(&mut self, slide: &str, index: usize) {
        let Some(part) = self.pkg.related(slide, RT_COMMENTS) else { return };
        if let Some(root) = self.pkg.xml_mut(&part) {
            let mut k = 0;
            root.children.retain(|n| if let Node::Element(e) = n { if e.name == "p:cm" { k += 1; return k - 1 != index; } true } else { true });
        }
    }
}

fn tint(hex: &str, t: f64) -> String {
    let c = crate::Rgba::hex(hex).unwrap_or(crate::Rgba::BLACK);
    let m = |v: u8| ((v as f64) + (255.0 - v as f64) * t.clamp(0.0, 0.9)).round() as u8;
    crate::Rgba::rgb(m(c.r), m(c.g), m(c.b)).to_hex()
}
fn shade(hex: &str, k: f64) -> String {
    let c = crate::Rgba::hex(hex).unwrap_or(crate::Rgba::BLACK);
    crate::Rgba::rgb((c.r as f64 * k) as u8, (c.g as f64 * k) as u8, (c.b as f64 * k) as u8).to_hex()
}

/// The shapes of a SmartArt layout in a frame: (preset, frame, text, level).
pub fn smartart_shapes(kind: DiagramKind, items: &[(u8, String)], x: &Xfrm) -> Vec<(String, Xfrm, String, u8)> {
    let tops: Vec<(String, Vec<String>)> = {
        let mut v: Vec<(String, Vec<String>)> = vec![];
        for (l, t) in items { if *l == 0 || v.is_empty() { v.push((t.clone(), vec![])); } else { v.last_mut().unwrap().1.push(t.clone()); } }
        v
    };
    let n = tops.len().max(1) as f64;
    let mut out = vec![];
    let f = |x0: f64, y0: f64, w: f64, h: f64| Xfrm { x: x0, y: y0, w, h, ..Default::default() };
    match kind {
        DiagramKind::List => {
            let cols = n.min(3.0).max(1.0);
            let rows = (n / cols).ceil();
            let (gw, gh) = (x.w / cols, x.h / rows);
            for (i, (t, subs)) in tops.iter().enumerate() {
                let (c, r) = ((i as f64 % cols), (i as f64 / cols).floor());
                let text = if subs.is_empty() { t.clone() } else { format!("{t}\n{}", subs.join("\n")) };
                out.push(("rect".into(), f(x.x + c * gw + gw * 0.05, x.y + r * gh + gh * 0.05, gw * 0.9, gh * 0.9), text, 0));
            }
        }
        DiagramKind::Process => {
            let gap = x.w * 0.06 / n.max(2.0);
            let bw = (x.w - gap * (n - 1.0) * 2.0) / n;
            for (i, (t, subs)) in tops.iter().enumerate() {
                let x0 = x.x + i as f64 * (bw + gap * 2.0);
                let text = if subs.is_empty() { t.clone() } else { format!("{t}\n{}", subs.join("\n")) };
                out.push(("roundRect".into(), f(x0, x.y + x.h * 0.25, bw, x.h * 0.5), text, 0));
                if i + 1 < tops.len() { out.push(("rightArrow".into(), f(x0 + bw + gap * 0.25, x.y + x.h * 0.42, gap * 1.5, x.h * 0.16), String::new(), 1)); }
            }
        }
        DiagramKind::Cycle => {
            let (cx, cy) = (x.x + x.w / 2.0, x.y + x.h / 2.0);
            let r = x.w.min(x.h) * 0.36;
            let s = x.w.min(x.h) * 0.26;
            for (i, (t, _)) in tops.iter().enumerate() {
                let a = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / n;
                out.push(("ellipse".into(), f(cx + r * a.cos() - s / 2.0, cy + r * a.sin() - s / 2.0, s, s), t.clone(), 0));
                let b = a + std::f64::consts::PI / n;
                let mut arrow = f(cx + r * 1.02 * b.cos() - s * 0.15, cy + r * 1.02 * b.sin() - s * 0.08, s * 0.3, s * 0.16);
                arrow.rot = (b.to_degrees() + 90.0).rem_euclid(360.0);
                out.push(("rightArrow".into(), arrow, String::new(), 1));
            }
        }
        DiagramKind::Pyramid => {
            let rows = n;
            let h = x.h / rows;
            for (i, (t, _)) in tops.iter().enumerate() {
                let w = x.w * (i as f64 + 1.0) / rows;
                out.push(("trapezoid".into(), f(x.x + (x.w - w) / 2.0, x.y + i as f64 * h, w, h * 0.96), t.clone(), (i as f64 / rows * 3.0) as u8));
            }
            if let Some(first) = out.first_mut() { first.0 = "triangle".into(); }
        }
        DiagramKind::Hierarchy => {
            // Root on top, its children below, grandchildren in a third row.
            let mut levels: Vec<Vec<(usize, String, Option<usize>)>> = vec![vec![], vec![], vec![]];
            let mut parents: [Option<usize>; 3] = [None; 3];
            for (k, (l, t)) in items.iter().enumerate() {
                let l = (*l as usize).min(2);
                levels[l].push((k, t.clone(), if l > 0 { parents[l - 1] } else { None }));
                parents[l] = Some(levels[l].len() - 1);
                for p in parents.iter_mut().skip(l + 1) { *p = None; }
            }
            let rows = levels.iter().filter(|r| !r.is_empty()).count().max(1) as f64;
            let rh = x.h / rows;
            let mut centers: Vec<Vec<(f64, f64)>> = vec![];
            for (li, row) in levels.iter().enumerate() {
                if row.is_empty() { centers.push(vec![]); continue; }
                let cw = x.w / row.len() as f64;
                let mut cs = vec![];
                for (k, (_, t, _)) in row.iter().enumerate() {
                    let (bw, bh) = (cw * 0.8, rh * 0.55);
                    let x0 = x.x + k as f64 * cw + (cw - bw) / 2.0;
                    let y0 = x.y + li as f64 * rh + (rh - bh) / 2.0;
                    out.push(("roundRect".into(), f(x0, y0, bw, bh), t.clone(), li as u8));
                    cs.push((x0 + bw / 2.0, y0));
                    let _ = bh;
                }
                centers.push(cs);
            }
            // Connectors from each box to its parent.
            for li in 1..levels.len() {
                for (k, (_, _, parent)) in levels[li].iter().enumerate() {
                    let (Some(p), Some(&(cx, top))) = (parent, centers[li].get(k)) else { continue };
                    let Some(&(px, ptop)) = centers[li - 1].get(*p) else { continue };
                    let pb = ptop + rh * 0.55;
                    out.insert(0, ("line".into(), f(px.min(cx), pb, (cx - px).abs().max(1.0), (top - pb).max(1.0)), String::new(), 9));
                    if cx < px { if let Some(l) = out.first_mut() { l.1.flip_h = true; } }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects() {
        let mut p = Presentation::new_blank(true);
        let blank = p.all_layouts().into_iter().find(|l| p.layout_type(l) == "blank").unwrap();
        let s = p.add_slide(&blank, 0);
        let x = Xfrm { x: 1e6, y: 1e6, w: 6e6, h: 4e6, ..Default::default() };
        let c = p.insert_chart(&s, ChartKind::Column, &x).unwrap();
        let mut chart = p.chart(&c).unwrap();
        assert_eq!(chart.series.len(), 3);
        chart.title = "Sales".into();
        p.set_chart(&c, &chart);
        assert_eq!(p.chart(&c).unwrap().title, "Sales");
        let items = Diagram::items_from_text("Plan\nBuild\nShip");
        let g = p.insert_smartart(&s, DiagramKind::Process, &items, &x, "4472C4").unwrap();
        assert_eq!(p.smartart(&g).unwrap().items.len(), 3);
        let e = p.insert_equation(&s, "a^2+b^2=c^2", 28.0, (6e6, 3e6)).unwrap();
        assert_eq!(p.equation(&e).as_deref(), Some("a^2+b^2=c^2"));
        p.add_comment(&s, "Salah", "Check this", 1e6, 1e6);
        assert_eq!(p.comments(&s).len(), 1);
        // Reuse a slide from the sample deck.
        let other = Presentation::open(include_bytes!("../tests/fixtures/sample.pptx")).unwrap();
        let r = p.reuse_slide(&other, 1, 1, false).unwrap();
        assert_eq!(p.slide_title(&r), "Agenda");
        let again = Presentation::open(&p.save().unwrap()).unwrap();
        assert_eq!(again.slides().len(), 2);
        assert!(again.chart(&c).is_some());
    }
}
