//! Slide Show settings and the masters behind the View tab: Set Up Slide Show (presProps'
//! p:showPr: show type, loop, narration, animation, slides, advance, pen color), custom
//! shows (p:custShowLst), rehearsed timings, and layout management for Slide Master view.

use crate::opc;
use crate::pres::Presentation;
use crate::xml::{Element, Node};

const RT_PRES_PROPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps";
const CT_PRES_PROPS: &str = "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml";
const CT_LAYOUT: &str = "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowType { Speaker, Browsed, Kiosk }

#[derive(Clone, Debug, PartialEq)]
pub enum Range { All, From(usize, usize), Custom(String) }

#[derive(Clone, Debug, PartialEq)]
pub struct ShowSetup {
    pub kind: ShowType,
    pub loop_until_esc: bool,
    pub narration: bool,
    pub animation: bool,
    pub range: Range,
    pub use_timings: bool,
    pub pen_color: String,
    pub laser_color: String,
    pub media_controls: bool,
    pub presenter_view: bool,
    /// Browsed individually: show the scroll bar.
    pub scrollbar: bool,
    /// The monitor the show plays on (its name), empty for automatic.
    pub monitor: String,
}

impl Default for ShowSetup {
    fn default() -> Self { ShowSetup { kind: ShowType::Speaker, loop_until_esc: false, narration: true, animation: true, range: Range::All, use_timings: true, pen_color: "FF0000".into(), laser_color: "FF0000".into(), media_controls: true, presenter_view: false, scrollbar: true, monitor: String::new() } }
}

impl Presentation {
    fn props_part(&self) -> Option<String> { self.pkg.related(&self.main, RT_PRES_PROPS) }
    fn props_mut(&mut self) -> &mut Element {
        let part = match self.props_part() {
            Some(p) => p,
            None => {
                let p = "ppt/presProps.xml".to_string();
                self.pkg.put_xml(&p, Element::new("p:presentationPr").with_attr("xmlns:a", NS_A).with_attr("xmlns:r", NS_R).with_attr("xmlns:p", NS_P), Some(CT_PRES_PROPS));
                let main = self.main.clone();
                self.pkg.add_rel(&main, RT_PRES_PROPS, &p, false);
                p
            }
        };
        if self.pkg.xml(&part).is_none() { self.pkg.put_xml(&part, Element::new("p:presentationPr").with_attr("xmlns:p", NS_P).with_attr("xmlns:a", NS_A), Some(CT_PRES_PROPS)); }
        self.pkg.xml_mut(&part).unwrap()
    }

    pub fn show_setup(&self) -> ShowSetup {
        let mut s = ShowSetup::default();
        let Some(pr) = self.props_part().and_then(|p| self.pkg.xml(&p)).and_then(|r| r.child("p:showPr")) else { return s };
        let flag = |k: &str, d: bool| pr.attr(k).map(|v| v == "1" || v == "true").unwrap_or(d);
        s.loop_until_esc = flag("loop", false);
        s.narration = flag("showNarration", false);
        s.animation = flag("showAnimation", true);
        s.use_timings = flag("useTimings", true);
        if let Some(b) = pr.child("p:browse") { s.kind = ShowType::Browsed; s.scrollbar = b.attr("showScrollbar") != Some("0"); }
        if pr.child("p:kiosk").is_some() { s.kind = ShowType::Kiosk; }
        if let Some(r) = pr.child("p:sldRg") { s.range = Range::From(r.attr_i64("st").unwrap_or(1).max(1) as usize, r.attr_i64("end").unwrap_or(1).max(1) as usize); }
        if let Some(c) = pr.child("p:custShow") { let id = c.attr("id").unwrap_or("0"); if let Some(n) = self.custom_shows().into_iter().find(|x| x.1 == id) { s.range = Range::Custom(n.0); } }
        if let Some(c) = pr.path("p:penClr").and_then(|c| c.find("a:srgbClr")).and_then(|c| c.attr("val")) { s.pen_color = c.to_string(); }
        // Ours (PowerPoint keeps these per user): in an extension attribute set.
        s.media_controls = pr.attr("qalamMedia") != Some("0");
        s.presenter_view = pr.attr("qalamPresenter") == Some("1");
        if let Some(l) = pr.attr("qalamLaser") { s.laser_color = l.to_string(); }
        if let Some(m) = pr.attr("qalamMonitor") { s.monitor = m.to_string(); }
        s
    }

    pub fn set_show_setup(&mut self, s: &ShowSetup) {
        let cust = match &s.range { Range::Custom(n) => self.custom_shows().into_iter().find(|x| &x.0 == n).map(|x| x.1), _ => None };
        let root = self.props_mut();
        root.remove_children("p:showPr");
        let b = |v: bool| if v { "1" } else { "0" };
        let mut pr = Element::new("p:showPr").with_attr("loop", b(s.loop_until_esc || s.kind == ShowType::Kiosk)).with_attr("showNarration", b(s.narration)).with_attr("showAnimation", b(s.animation)).with_attr("useTimings", b(s.use_timings));
        pr.set_attr("qalamMedia", b(s.media_controls));
        pr.set_attr("qalamPresenter", b(s.presenter_view));
        pr.set_attr("qalamLaser", s.laser_color.clone());
        if !s.monitor.is_empty() { pr.set_attr("qalamMonitor", s.monitor.clone()); }
        pr.children.push(Node::Element(match s.kind { ShowType::Speaker => Element::new("p:present"), ShowType::Browsed => Element::new("p:browse").with_attr("showScrollbar", b(s.scrollbar)), ShowType::Kiosk => Element::new("p:kiosk") }));
        pr.children.push(Node::Element(match (&s.range, cust) {
            (Range::From(a, z), _) => Element::new("p:sldRg").with_attr("st", a.to_string()).with_attr("end", z.to_string()),
            (Range::Custom(_), Some(id)) => Element::new("p:custShow").with_attr("id", id),
            _ => Element::new("p:sldAll"),
        }));
        pr.children.push(Node::Element(Element::new("p:penClr").with_child(Element::new("a:srgbClr").with_attr("val", s.pen_color.clone()))));
        // showPr comes first in presentationPr.
        root.children.insert(0, Node::Element(pr));
    }

    /// Handout or notes master settings ("handout" or "notes"): stored as text on the
    /// presentation's properties, and as the master part's header/footer flags when it has one.
    pub fn page_master(&self, kind: &str) -> Option<String> {
        let root = self.props_part().and_then(|p| self.pkg.xml(&p))?;
        root.attr(if kind == "handout" { "qalamHandout" } else { "qalamNotesMaster" }).map(String::from)
    }

    /// `flags`: (header, date, footer, page number).
    pub fn set_page_master(&mut self, kind: &str, value: &str, flags: (bool, bool, bool, bool)) {
        self.props_mut().set_attr(if kind == "handout" { "qalamHandout" } else { "qalamNotesMaster" }, value);
        let rt = if kind == "handout" { "http://schemas.openxmlformats.org/officeDocument/2006/relationships/handoutMaster" } else { opc::RT_NOTES_MASTER };
        let Some(part) = self.pkg.related(&self.main, rt) else { return };
        let Some(root) = self.pkg.xml_mut(&part) else { return };
        root.remove_children("p:hf");
        let b = |v: bool| if v { "1" } else { "0" };
        let hf = Element::new("p:hf").with_attr("hdr", b(flags.0)).with_attr("dt", b(flags.1)).with_attr("ftr", b(flags.2)).with_attr("sldNum", b(flags.3));
        // p:hf comes after cSld/clrMap (and notesStyle comes after it).
        let pos = root.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "p:notesStyle" | "p:extLst"))).unwrap_or(root.children.len());
        root.children.insert(pos, Node::Element(hf));
    }

    /// Custom shows: (name, id, slide parts).
    pub fn custom_shows(&self) -> Vec<(String, String, Vec<String>)> {
        let Some(list) = self.root().child("p:custShowLst") else { return vec![] };
        let main = self.main.clone();
        list.children_named("p:custShow").map(|c| {
            let slides = c.path("p:sldLst").map(|l| l.children_named("p:sld").filter_map(|s| s.attr("r:id").and_then(|id| self.pkg.rel_target(&main, id))).collect()).unwrap_or_default();
            (c.attr("name").unwrap_or("Custom Show").to_string(), c.attr("id").unwrap_or("0").to_string(), slides)
        }).collect()
    }

    pub fn set_custom_shows(&mut self, shows: &[(String, Vec<String>)]) {
        let main = self.main.clone();
        let mut list = Element::new("p:custShowLst");
        for (k, (name, slides)) in shows.iter().enumerate() {
            let mut sl = Element::new("p:sldLst");
            for s in slides {
                if let Some(r) = self.pkg.rels(&main).into_iter().find(|r| r.kind == opc::RT_SLIDE && opc::resolve(&main, &r.target) == *s) { sl.children.push(Node::Element(Element::new("p:sld").with_attr("r:id", r.id))); }
            }
            list.children.push(Node::Element(Element::new("p:custShow").with_attr("name", name.clone()).with_attr("id", k.to_string()).with_child(sl)));
        }
        let root = self.root_mut();
        root.remove_children("p:custShowLst");
        if shows.is_empty() { return; }
        let pos = root.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "p:photoAlbum" | "p:custDataLst" | "p:kinsoku" | "p:defaultTextStyle" | "p:modifyVerifier" | "p:extLst"))).unwrap_or(root.children.len());
        root.children.insert(pos, Node::Element(list));
    }

    /// The slides a show plays (indexes), by the setup's range.
    pub fn show_order(&self, s: &ShowSetup) -> Vec<usize> {
        let slides = self.slides();
        match &s.range {
            Range::All => (0..slides.len()).filter(|&i| !self.hidden(&slides[i])).collect(),
            Range::From(a, z) => (a.saturating_sub(1)..(*z).min(slides.len())).filter(|&i| !self.hidden(&slides[i])).collect(),
            Range::Custom(n) => self.custom_shows().into_iter().find(|c| &c.0 == n).map(|c| c.2.iter().filter_map(|p| slides.iter().position(|s| s == p)).collect()).unwrap_or_default(),
        }
    }

    /// Rehearsed (or cleared) timings: advance each slide after its time.
    pub fn set_timing(&mut self, slide: &str, ms: Option<u32>) {
        let mut t = self.slide_transition(slide);
        t.after_ms = ms;
        if t.effect == "none" && t.duration_ms == 0 { t.duration_ms = 0; }
        self.set_slide_transition(slide, &t);
    }

    // ---- Slide Master view: layouts -------------------------------------------------------

    /// A copy of `layout` (with `name`) after it in its master; returns the new part.
    pub fn duplicate_layout(&mut self, layout: &str, name: &str) -> Option<String> {
        let master = self.master_of(layout)?;
        let mut root = self.pkg.xml(layout)?.clone();
        if let Some(c) = root.child_mut("p:cSld") { c.set_attr("name", name); }
        root.set_attr("preserve", "1");
        root.remove_attr("type");
        let part = self.pkg.free_name("ppt/slideLayouts/slideLayout", ".xml");
        self.pkg.put_xml(&part, root, Some(CT_LAYOUT));
        if let Some(r) = self.pkg.xml(&opc::rels_name(layout)).cloned() { self.pkg.put_xml(&opc::rels_name(&part), r, None); }
        let rid = self.pkg.add_rel(&master, opc::RT_LAYOUT, &part, false);
        let next = self.pkg.xml(&master)?.path("p:sldLayoutIdLst").map(|l| l.elements().filter_map(|e| e.attr_i64("id")).max().unwrap_or(2147483648) + 1).unwrap_or(2147483649);
        // Ids must also stay clear of the masters' ids.
        let next = next.max(self.masters_max_id() + 1);
        let m = self.pkg.xml_mut(&master)?;
        let list = m.ensure_at("p:sldLayoutIdLst", &["p:transition", "p:timing", "p:hf", "p:txStyles", "p:extLst"]);
        list.children.push(Node::Element(Element::new("p:sldLayoutId").with_attr("id", next.to_string()).with_attr("r:id", rid)));
        Some(part)
    }

    fn masters_max_id(&self) -> i64 {
        let mut m = self.root().path("p:sldMasterIdLst").map(|l| l.elements().filter_map(|e| e.attr_i64("id")).max().unwrap_or(0)).unwrap_or(0);
        for ms in self.masters() { if let Some(l) = self.pkg.xml(&ms).and_then(|x| x.path("p:sldLayoutIdLst")) { m = m.max(l.elements().filter_map(|e| e.attr_i64("id")).max().unwrap_or(0)); } }
        m
    }

    /// A new layout with only a title (Insert Layout).
    pub fn insert_layout(&mut self, master: &str) -> Option<String> {
        let base = self.layouts(master).into_iter().find(|l| self.layout_type(l) == "titleOnly").or_else(|| self.layouts(master).into_iter().next())?;
        let n = self.layouts(master).len() + 1;
        self.duplicate_layout(&base, &format!("Custom Layout {n}"))
    }

    /// Slides using a layout.
    pub fn layout_users(&self, layout: &str) -> usize { self.slides().iter().filter(|s| self.layout_of(s).as_deref() == Some(layout)).count() }

    /// Delete a layout no slide uses; false when it's in use or the master's last.
    pub fn delete_layout(&mut self, layout: &str) -> bool {
        let Some(master) = self.master_of(layout) else { return false };
        if self.layout_users(layout) > 0 || self.layouts(&master).len() <= 1 { return false; }
        let rid = self.pkg.rels(&master).into_iter().find(|r| r.kind == opc::RT_LAYOUT && opc::resolve(&master, &r.target) == layout).map(|r| r.id);
        if let Some(rid) = rid {
            if let Some(l) = self.pkg.xml_mut(&master).and_then(|m| m.child_mut("p:sldLayoutIdLst")) { l.children.retain(|n| !matches!(n, Node::Element(e) if e.attr("r:id") == Some(rid.as_str()))); }
            self.pkg.remove_rel(&master, &rid);
        }
        self.pkg.remove(layout);
        true
    }

    pub fn rename_layout(&mut self, layout: &str, name: &str) {
        if let Some(c) = self.pkg.xml_mut(layout).and_then(|l| l.child_mut("p:cSld")) { c.set_attr("name", name); }
    }

    /// Whether a layout (or master) shows a kind of placeholder ("title", "dt", "ftr", "sldNum").
    pub fn has_placeholder(&self, part: &str, kind: &str) -> bool {
        self.pkg.xml(part).and_then(|r| r.path("p:cSld/p:spTree")).is_some_and(|t| t.elements().any(|e| e.find("p:ph").is_some_and(|p| p.attr("type").unwrap_or("obj") == kind)))
    }

    /// Turn a layout's title or footer placeholders on (copied from its master) or off.
    pub fn set_placeholder(&mut self, layout: &str, kind: &str, on: bool) {
        if on == self.has_placeholder(layout, kind) { return; }
        if !on {
            if let Some(t) = self.pkg.xml_mut(layout).and_then(|r| r.path_mut("p:cSld/p:spTree")) { t.children.retain(|n| !matches!(n, Node::Element(e) if e.find("p:ph").is_some_and(|p| p.attr("type").unwrap_or("obj") == kind))); }
            return;
        }
        let Some(master) = self.master_of(layout) else { return };
        let Some(sp) = self.pkg.xml(&master).and_then(|r| r.path("p:cSld/p:spTree")).and_then(|t| t.elements().find(|e| e.find("p:ph").is_some_and(|p| p.attr("type").unwrap_or("obj") == kind)).cloned()) else { return };
        let ids: Vec<i64> = self.pkg.xml(layout).and_then(|r| r.path("p:cSld/p:spTree")).map(|t| { let mut v = vec![]; t.find_all("p:cNvPr", &mut v); v.iter().filter_map(|c| c.attr_i64("id")).collect() }).unwrap_or_default();
        let mut sp = sp;
        if let Some(c) = sp.elements_mut().find(|e| e.local().starts_with("nv")).and_then(|nv| nv.child_mut("p:cNvPr")) { c.set_attr("id", (ids.iter().max().unwrap_or(&1) + 1).to_string()); }
        if let Some(t) = self.pkg.xml_mut(layout).and_then(|r| r.path_mut("p:cSld/p:spTree")) { t.children.push(Node::Element(sp)); }
    }

    /// Ink kept from a slide show: a freeform line through `pts` (EMU), its color (hex),
    /// width (pt) and opacity (highlighter).
    pub fn insert_ink(&mut self, slide: &str, pts: &[(f64, f64)], color: &str, width_pt: f64, alpha: f64) -> Option<crate::scene::Loc> {
        if pts.len() < 2 { return None; }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in pts { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); }
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let pt = |x: f64, y: f64| Element::new("a:pt").with_attr("x", ((x - x0).round() as i64).to_string()).with_attr("y", ((y - y0).round() as i64).to_string());
        let mut path = Element::new("a:path").with_attr("w", (w.round() as i64).to_string()).with_attr("h", (h.round() as i64).to_string());
        path.children.push(Node::Element(Element::new("a:moveTo").with_child(pt(pts[0].0, pts[0].1))));
        for &(x, y) in &pts[1..] { path.children.push(Node::Element(Element::new("a:lnTo").with_child(pt(x, y)))); }
        let id = self.next_shape_id(slide);
        let mut clr = Element::new("a:srgbClr").with_attr("val", color);
        if alpha < 1.0 { clr.children.push(Node::Element(Element::new("a:alpha").with_attr("val", ((alpha * 100_000.0).round() as i64).to_string()))); }
        let sp = Element::new("p:sp")
            .with_child(Element::new("p:nvSpPr").with_child(Element::new("p:cNvPr").with_attr("id", id.to_string()).with_attr("name", format!("Ink {id}"))).with_child(Element::new("p:cNvSpPr")).with_child(Element::new("p:nvPr")))
            .with_child(Element::new("p:spPr")
                .with_child(Element::new("a:xfrm").with_child(Element::new("a:off").with_attr("x", (x0.round() as i64).to_string()).with_attr("y", (y0.round() as i64).to_string())).with_child(Element::new("a:ext").with_attr("cx", (w.round() as i64).to_string()).with_attr("cy", (h.round() as i64).to_string())))
                .with_child(Element::new("a:custGeom").with_child(Element::new("a:avLst")).with_child(Element::new("a:gdLst")).with_child(Element::new("a:ahLst")).with_child(Element::new("a:cxnLst")).with_child(Element::new("a:rect").with_attr("l", "0").with_attr("t", "0").with_attr("r", "r").with_attr("b", "b")).with_child(Element::new("a:pathLst").with_child(path)))
                .with_child(Element::new("a:noFill"))
                .with_child(Element::new("a:ln").with_attr("w", ((width_pt * 12_700.0).round() as i64).to_string()).with_attr("cap", "rnd").with_child(Element::new("a:solidFill").with_child(clr)).with_child(Element::new("a:round"))));
        let tree = self.pkg.xml_mut(slide)?.path_mut("p:cSld/p:spTree")?;
        tree.children.push(Node::Element(sp));
        let n = tree.elements().count() - 1;
        Some(crate::scene::Loc { part: slide.to_string(), path: vec![n] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_custom_shows_and_layouts() {
        let mut p = Presentation::new_blank(true);
        let l = p.all_layouts()[1].clone();
        let a = p.add_slide(&l, 0);
        let b = p.add_slide(&l, 1);
        let _c = p.add_slide(&l, 2);
        p.set_custom_shows(&[("Short".into(), vec![b.clone(), a.clone()])]);
        let s = ShowSetup { kind: ShowType::Kiosk, range: Range::Custom("Short".into()), pen_color: "00FF00".into(), presenter_view: true, ..Default::default() };
        p.set_show_setup(&s);
        let p = Presentation::open(&p.save().unwrap()).unwrap();
        let back = p.show_setup();
        assert_eq!((back.kind, back.range.clone(), back.pen_color.as_str(), back.loop_until_esc, back.presenter_view), (ShowType::Kiosk, Range::Custom("Short".into()), "00FF00", true, true));
        assert_eq!(p.show_order(&back), vec![1, 0]);
        let mut p = p;
        let m = p.masters()[0].clone();
        let n = p.layouts(&m).len();
        let new = p.insert_layout(&m).unwrap();
        assert_eq!(p.layouts(&m).len(), n + 1);
        p.rename_layout(&new, "Mine");
        assert_eq!(p.layout_name(&new), "Mine");
        p.set_placeholder(&new, "title", false);
        assert!(!p.has_placeholder(&new, "title"));
        p.set_placeholder(&new, "title", true);
        assert!(p.has_placeholder(&new, "title"));
        assert!(p.delete_layout(&new));
        assert_eq!(p.layouts(&m).len(), n);
        let again = Presentation::open(&p.save().unwrap()).unwrap();
        assert_eq!(again.layouts(&again.masters()[0]).len(), n);
    }
}
