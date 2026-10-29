//! Native Excel forms for objects Jadwal models in its own way (timelines, filled maps,
//! equations, signature lines, 3D models, embedded packages, SmartArt...). The drawing writer
//! (drawing::prepare) asks `write_object` first for every object; reading (drawing::load) lets
//! `read_object` turn recognized drawing content back into Jadwal's objects. Each kind lives in
//! its own module; this one only dispatches.

use crate::book::Workbook;
use crate::drawing::Object;
use super::xml::Element;

/// What a native writer may add while the drawing part is being written.
pub struct Sink<'a> {
    /// A relationship of the drawing part: (type, absolute target or URL, external) → its id.
    pub add_rel: &'a mut dyn FnMut(&str, String, bool) -> String,
    /// A free part name: (prefix, extension with dot) → "prefixN.ext".
    pub free: &'a mut dyn FnMut(&str, &str) -> String,
    /// Content type overrides (part name, type) and defaults (extension, type).
    pub overrides: &'a mut Vec<(String, String)>,
    pub defaults: &'a mut Vec<(String, String)>,
}

/// The drawing content (what goes inside the anchor) of object `oi` of sheet `si` in its native
/// form, adding parts to `w`; None: the drawing writer's usual form.
#[allow(unused_variables)]
pub fn write_object(w: &mut Workbook, si: usize, oi: usize, o: &Object, sink: &mut Sink) -> Option<String> {
    if matches!(o.kind, crate::drawing::ObjKind::Picture { .. }) && o.descr.starts_with(super::equation_parts::EQ_TAG) { if let Some(x) = super::equation_parts::write(w, o, sink) { return Some(x); } }
    if matches!(o.kind, crate::drawing::ObjKind::Picture { .. }) && o.descr.starts_with(crate::model3d::TAG) { if let Some(x) = super::model3d_parts::write(w, o, sink) { return Some(x); } }
    if let crate::drawing::ObjKind::Timeline(t) = &o.kind { return super::timeline_parts::frame_xml(w, si, oi, o, t); }
    if matches!(o.kind, crate::drawing::ObjKind::Picture { .. }) && crate::geo::FilledMap::from_tag(&o.descr).is_some() { if let Some(x) = super::regionmap_parts::write(w, si, o, sink) { return Some(x); } }
    if let crate::drawing::ObjKind::Group(g) = &o.kind { if let Some(x) = super::smartart_parts::write(w, si, o, g, sink) { return Some(x); } }
    None
}

/// Turns the drawing content of a just-read object (kind as the drawing reader saw it, raw and
/// rels set) into the Jadwal object it is natively. Returns whether it did.
#[allow(unused_variables)]
pub fn read_object(wb: &Workbook, si: usize, content: &Element, o: &mut Object) -> bool {
    if content.local() == "AlternateContent" && matches!(o.kind, crate::drawing::ObjKind::Other) {
        if let Some(k) = super::timeline_parts::kind(wb, si, content) { o.kind = k; return true; }
    }
    if super::model3d_parts::is_model(content) && super::model3d_parts::read(wb, content, o) { return true; }
    if super::equation_parts::is_equation(content) && super::equation_parts::read(wb, content, o) { return true; }
    if super::regionmap_parts::read(wb, si, content, o) { return true; }
    if super::smartart_parts::read(wb, si, content, o) { return true; }
    false
}

/// For a top-level `mc:AlternateContent` of a drawing (Excel wraps whole anchors for equations
/// and 3D models): the anchor element to read, when its content is a kind read natively.
/// None keeps the whole drawing carried as it was.
#[allow(unused_variables)]
pub fn top_level_anchor<'a>(wb: &Workbook, ac: &'a Element) -> Option<&'a Element> {
    // Equations and 3D models: the Choice's anchor when its content is one.
    for ch in ac.all("Choice") {
        let req = ch.attr("Requires").unwrap_or("");
        if !req.split_whitespace().any(|r| r == "a14" || r == "am3d") { continue; }
        let Some(a) = ch.elements().find(|e| matches!(e.local(), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor")) else { continue };
        if a.elements().any(|c| super::model3d_parts::is_model(c) || super::equation_parts::is_equation(c)) { return Some(a); }
    }
    None
}
