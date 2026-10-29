//! Object animations (Animations tab): entrance, emphasis, exit and motion-path effects on a
//! slide's shapes, read from and written to its p:timing tree (the main sequence of click
//! groups, with PowerPoint's preset ids and behaviors so PowerPoint plays them too), and the
//! timeline the slide show plays.

use crate::pres::Presentation;
use crate::scene::Loc;
use crate::xml::{Element, Node};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class { Entrance, Emphasis, Exit, Path }

impl Class {
    pub fn xml(&self) -> &'static str { match self { Class::Entrance => "entr", Class::Emphasis => "emph", Class::Exit => "exit", Class::Path => "path" } }
    fn from_xml(s: &str) -> Class { match s { "emph" => Class::Emphasis, "exit" => Class::Exit, "path" => Class::Path, _ => Class::Entrance } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start { OnClick, WithPrevious, AfterPrevious }

/// One effect in the library: id, name, class, PowerPoint preset id, options (id, label).
pub struct Preset { pub id: &'static str, pub name: &'static str, pub class: Class, pub preset: u32, pub options: &'static [(&'static str, &'static str)] }

const DIRS: &[(&str, &str)] = &[("d", "From Bottom"), ("l", "From Left"), ("r", "From Right"), ("u", "From Top"), ("ld", "From Bottom-Left"), ("rd", "From Bottom-Right"), ("lu", "From Top-Left"), ("ru", "From Top-Right")];
const DIR4: &[(&str, &str)] = &[("d", "From Bottom"), ("l", "From Left"), ("r", "From Right"), ("u", "From Top")];
const SPLIT: &[(&str, &str)] = &[("vertIn", "Vertical In"), ("vertOut", "Vertical Out"), ("horzIn", "Horizontal In"), ("horzOut", "Horizontal Out")];
const SHAPES: &[(&str, &str)] = &[("circleIn", "Circle In"), ("circleOut", "Circle Out"), ("boxIn", "Box In"), ("boxOut", "Box Out"), ("diamondIn", "Diamond In"), ("plusIn", "Plus In")];
const SPOKES: &[(&str, &str)] = &[("1", "1 Spoke"), ("2", "2 Spokes"), ("3", "3 Spokes"), ("4", "4 Spokes"), ("8", "8 Spokes")];
const BARS: &[(&str, &str)] = &[("vert", "Vertical"), ("horz", "Horizontal")];
const AMOUNT: &[(&str, &str)] = &[("150", "Larger"), ("110", "Slightly Larger"), ("75", "Smaller"), ("50", "Tiny")];
const SPIN: &[(&str, &str)] = &[("360", "Full Spin"), ("180", "Half Spin"), ("90", "Quarter Spin"), ("720", "Two Spins"), ("-360", "Counterclockwise")];
const PATHS: &[(&str, &str)] = &[("d", "Down"), ("u", "Up"), ("r", "Right"), ("l", "Left")];

pub const PRESETS: &[Preset] = &[
    Preset { id: "appear", name: "Appear", class: Class::Entrance, preset: 1, options: &[] },
    Preset { id: "fade", name: "Fade", class: Class::Entrance, preset: 10, options: &[] },
    Preset { id: "flyIn", name: "Fly In", class: Class::Entrance, preset: 2, options: DIRS },
    Preset { id: "floatIn", name: "Float In", class: Class::Entrance, preset: 42, options: &[("u", "Float Up"), ("d", "Float Down")] },
    Preset { id: "split", name: "Split", class: Class::Entrance, preset: 16, options: SPLIT },
    Preset { id: "wipe", name: "Wipe", class: Class::Entrance, preset: 22, options: DIR4 },
    Preset { id: "shape", name: "Shape", class: Class::Entrance, preset: 6, options: SHAPES },
    Preset { id: "wheel", name: "Wheel", class: Class::Entrance, preset: 21, options: SPOKES },
    Preset { id: "randomBars", name: "Random Bars", class: Class::Entrance, preset: 14, options: BARS },
    Preset { id: "growTurn", name: "Grow & Turn", class: Class::Entrance, preset: 31, options: &[] },
    Preset { id: "zoom", name: "Zoom", class: Class::Entrance, preset: 53, options: &[("center", "Object Center"), ("slide", "Slide Center")] },
    Preset { id: "swivel", name: "Swivel", class: Class::Entrance, preset: 45, options: &[("horz", "Horizontal"), ("vert", "Vertical")] },
    Preset { id: "bounce", name: "Bounce", class: Class::Entrance, preset: 26, options: &[] },
    Preset { id: "pulse", name: "Pulse", class: Class::Emphasis, preset: 26, options: &[] },
    Preset { id: "colorPulse", name: "Color Pulse", class: Class::Emphasis, preset: 27, options: &[] },
    Preset { id: "teeter", name: "Teeter", class: Class::Emphasis, preset: 32, options: &[] },
    Preset { id: "spin", name: "Spin", class: Class::Emphasis, preset: 8, options: SPIN },
    Preset { id: "growShrink", name: "Grow/Shrink", class: Class::Emphasis, preset: 6, options: AMOUNT },
    Preset { id: "desaturate", name: "Desaturate", class: Class::Emphasis, preset: 35, options: &[] },
    Preset { id: "darken", name: "Darken", class: Class::Emphasis, preset: 36, options: &[] },
    Preset { id: "lighten", name: "Lighten", class: Class::Emphasis, preset: 30, options: &[] },
    Preset { id: "transparency", name: "Transparency", class: Class::Emphasis, preset: 9, options: &[("50", "50%"), ("25", "25%"), ("75", "75%"), ("100", "100%")] },
    Preset { id: "objectColor", name: "Object Color", class: Class::Emphasis, preset: 19, options: &[] },
    Preset { id: "complementary", name: "Complementary Color", class: Class::Emphasis, preset: 21, options: &[] },
    Preset { id: "lineColor", name: "Line Color", class: Class::Emphasis, preset: 7, options: &[] },
    Preset { id: "fillColor", name: "Fill Color", class: Class::Emphasis, preset: 1, options: &[] },
    Preset { id: "fontColor", name: "Font Color", class: Class::Emphasis, preset: 3, options: &[] },
    Preset { id: "underline", name: "Underline", class: Class::Emphasis, preset: 18, options: &[] },
    Preset { id: "boldFlash", name: "Bold Flash", class: Class::Emphasis, preset: 10, options: &[] },
    Preset { id: "boldReveal", name: "Bold Reveal", class: Class::Emphasis, preset: 15, options: &[] },
    Preset { id: "wave", name: "Wave", class: Class::Emphasis, preset: 34, options: &[] },
    Preset { id: "disappear", name: "Disappear", class: Class::Exit, preset: 1, options: &[] },
    Preset { id: "fadeOut", name: "Fade", class: Class::Exit, preset: 10, options: &[] },
    Preset { id: "flyOut", name: "Fly Out", class: Class::Exit, preset: 2, options: &[("d", "To Bottom"), ("l", "To Left"), ("r", "To Right"), ("u", "To Top"), ("ld", "To Bottom-Left"), ("rd", "To Bottom-Right"), ("lu", "To Top-Left"), ("ru", "To Top-Right")] },
    Preset { id: "floatOut", name: "Float Out", class: Class::Exit, preset: 42, options: &[("d", "Float Down"), ("u", "Float Up")] },
    Preset { id: "splitOut", name: "Split", class: Class::Exit, preset: 16, options: SPLIT },
    Preset { id: "wipeOut", name: "Wipe", class: Class::Exit, preset: 22, options: DIR4 },
    Preset { id: "shapeOut", name: "Shape", class: Class::Exit, preset: 6, options: SHAPES },
    Preset { id: "wheelOut", name: "Wheel", class: Class::Exit, preset: 21, options: SPOKES },
    Preset { id: "randomBarsOut", name: "Random Bars", class: Class::Exit, preset: 14, options: BARS },
    Preset { id: "shrinkTurn", name: "Shrink & Turn", class: Class::Exit, preset: 31, options: &[] },
    Preset { id: "zoomOut", name: "Zoom", class: Class::Exit, preset: 53, options: &[("center", "Object Center"), ("slide", "Slide Center")] },
    Preset { id: "swivelOut", name: "Swivel", class: Class::Exit, preset: 45, options: &[("horz", "Horizontal"), ("vert", "Vertical")] },
    Preset { id: "bounceOut", name: "Bounce", class: Class::Exit, preset: 26, options: &[] },
    Preset { id: "lines", name: "Lines", class: Class::Path, preset: 42, options: PATHS },
    Preset { id: "arcs", name: "Arcs", class: Class::Path, preset: 37, options: PATHS },
    Preset { id: "turns", name: "Turns", class: Class::Path, preset: 41, options: PATHS },
    Preset { id: "shapes", name: "Shapes", class: Class::Path, preset: 64, options: &[("circle", "Circle"), ("diamond", "Diamond"), ("triangle", "Triangle"), ("square", "Square")] },
    Preset { id: "loops", name: "Loops", class: Class::Path, preset: 52, options: &[] },
];

pub fn preset(id: &str) -> Option<&'static Preset> { PRESETS.iter().find(|p| p.id == id) }

/// One animation on a shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Anim {
    /// The shape's id (cNvPr id).
    pub shape: u32,
    pub effect: String,
    pub option: String,
    pub start: Start,
    pub duration_ms: u32,
    pub delay_ms: u32,
    /// 0 = once; else the count (u32::MAX = until next click).
    pub repeat: u32,
    pub rewind: bool,
    /// Plays when this shape is clicked (a trigger), instead of in the main sequence.
    pub trigger: Option<u32>,
    /// A color for color effects (hex).
    pub color: Option<String>,
}

impl Anim {
    pub fn class(&self) -> Class { preset(&self.effect).map(|p| p.class).unwrap_or(Class::Entrance) }
    pub fn name(&self) -> &'static str { preset(&self.effect).map(|p| p.name).unwrap_or("Custom") }
    pub fn new(shape: u32, effect: &str) -> Anim {
        let p = preset(effect);
        Anim { shape, effect: effect.into(), option: p.and_then(|p| p.options.first()).map(|o| o.0.to_string()).unwrap_or_default(), start: Start::OnClick, duration_ms: default_duration(effect), delay_ms: 0, repeat: 0, rewind: false, trigger: None, color: None }
    }
}

pub fn default_duration(effect: &str) -> u32 {
    match effect { "appear" | "disappear" => 0, "flyIn" | "flyOut" | "wipe" | "wipeOut" | "split" | "splitOut" | "randomBars" | "randomBarsOut" => 500, "spin" | "growShrink" | "teeter" | "wave" | "bounce" | "bounceOut" | "lines" | "arcs" | "turns" | "shapes" | "loops" => 2000, _ => 500 }
}

// ---- writing the timing tree -------------------------------------------------------------

struct Ids(u32);
impl Ids { fn next(&mut self) -> String { self.0 += 1; self.0.to_string() } }

fn cond(delay: &str) -> Element { Element::new("p:stCondLst").with_child(Element::new("p:cond").with_attr("delay", delay)) }
fn target(spid: u32) -> Element { Element::new("p:tgtEl").with_child(Element::new("p:spTgt").with_attr("spid", spid.to_string())) }
fn behavior(ids: &mut Ids, dur: &str, spid: u32, attr: Option<&str>) -> Element {
    let mut c = Element::new("p:cBhvr").with_child(Element::new("p:cTn").with_attr("id", ids.next()).with_attr("dur", dur).with_attr("fill", "hold")).with_child(target(spid));
    if let Some(a) = attr { c.children.push(Node::Element(Element::new("p:attrNameLst").with_child(Element::new("p:attrName").with_text(a)))); }
    c
}
fn set_vis(ids: &mut Ids, spid: u32, visible: bool, delay: u32) -> Element {
    let mut b = behavior(ids, "1", spid, Some("style.visibility"));
    if delay > 0 { if let Some(ctn) = b.child_mut("p:cTn") { ctn.children.push(Node::Element(cond(&delay.to_string()))); } }
    Element::new("p:set").with_child(b).with_child(Element::new("p:to").with_child(Element::new("p:strVal").with_attr("val", if visible { "visible" } else { "hidden" })))
}
fn anim_prop(ids: &mut Ids, dur: u32, spid: u32, attr: &str, from: &str, to: &str) -> Element {
    let mut b = behavior(ids, &dur.to_string(), spid, Some(attr));
    b.set_attr("additive", "base");
    Element::new("p:anim").with_attr("calcmode", "lin").with_attr("valueType", "num").with_child(b)
        .with_child(Element::new("p:tavLst")
            .with_child(Element::new("p:tav").with_attr("tm", "0").with_child(Element::new("p:val").with_child(Element::new("p:strVal").with_attr("val", from))))
            .with_child(Element::new("p:tav").with_attr("tm", "100000").with_child(Element::new("p:val").with_child(Element::new("p:strVal").with_attr("val", to)))))
}
fn filter(ids: &mut Ids, dur: u32, spid: u32, transition: &str, f: &str) -> Element {
    Element::new("p:animEffect").with_attr("transition", transition).with_attr("filter", f).with_child(behavior(ids, &dur.to_string(), spid, None))
}

/// The behaviors of an effect (what PowerPoint actually plays).
fn behaviors(a: &Anim, ids: &mut Ids) -> Vec<Element> {
    let (d, s) = (a.duration_ms.max(1), a.shape);
    let o = a.option.as_str();
    let fly = |dir: &str, out: bool| -> (String, String, String, String) {
        let (x, y) = ("#ppt_x".to_string(), "#ppt_y".to_string());
        let (fx, fy) = match dir { "l" => ("0-#ppt_w/2".into(), y.clone()), "r" => ("1+#ppt_w/2".into(), y.clone()), "u" => (x.clone(), "0-#ppt_h/2".into()), "ld" => ("0-#ppt_w/2".into(), "1+#ppt_h/2".into()), "rd" => ("1+#ppt_w/2".into(), "1+#ppt_h/2".into()), "lu" => ("0-#ppt_w/2".into(), "0-#ppt_h/2".into()), "ru" => ("1+#ppt_w/2".into(), "0-#ppt_h/2".into()), _ => (x.clone(), "1+#ppt_h/2".into()) };
        if out { (x, y, fx, fy) } else { (fx, fy, x, y) }
    };
    let filt = |kind: &str, out: bool| -> String {
        match kind {
            "wipe" => format!("wipe({})", match o { "l" => "left", "r" => "right", "u" => "up", _ => "down" }),
            "split" => format!("barn({}{})", if o.ends_with("Out") { "out" } else { "in" }, if o.starts_with("horz") { "Horizontal" } else { "Vertical" }),
            "shape" => { let (sh, dir) = o.split_at(o.find(|c: char| c.is_uppercase()).unwrap_or(o.len())); format!("{}({})", if sh.is_empty() { "circle" } else { sh }, if dir == "Out" { "out" } else { "in" }) }
            "wheel" => format!("wheel({})", if o.is_empty() { "1" } else { o }),
            "randomBars" => format!("randombar({})", if o == "vert" { "vertical" } else { "horizontal" }),
            _ => { let _ = out; "fade".into() }
        }
    };
    let mut v = vec![];
    match a.effect.as_str() {
        "appear" => v.push(set_vis(ids, s, true, 0)),
        "disappear" => v.push(set_vis(ids, s, false, 0)),
        "fade" => { v.push(set_vis(ids, s, true, 0)); v.push(filter(ids, d, s, "in", "fade")); }
        "fadeOut" => { v.push(filter(ids, d, s, "out", "fade")); v.push(set_vis(ids, s, false, d)); }
        "flyIn" | "flyOut" => {
            let out = a.effect == "flyOut";
            let (fx, fy, tx, ty) = fly(if o.is_empty() { "d" } else { o }, out);
            if !out { v.push(set_vis(ids, s, true, 0)); }
            v.push(anim_prop(ids, d, s, "ppt_x", &fx, &tx));
            v.push(anim_prop(ids, d, s, "ppt_y", &fy, &ty));
            if out { v.push(set_vis(ids, s, false, d)); }
        }
        "floatIn" | "floatOut" => {
            let out = a.effect == "floatOut";
            let off = if o == "d" { "-0.1" } else { "+0.1" };
            if !out { v.push(set_vis(ids, s, true, 0)); v.push(filter(ids, d, s, "in", "fade")); v.push(anim_prop(ids, d, s, "ppt_y", &format!("#ppt_y{off}"), "#ppt_y")); }
            else { v.push(filter(ids, d, s, "out", "fade")); v.push(anim_prop(ids, d, s, "ppt_y", "#ppt_y", &format!("#ppt_y{off}"))); v.push(set_vis(ids, s, false, d)); }
        }
        "split" | "wipe" | "shape" | "wheel" | "randomBars" => { v.push(set_vis(ids, s, true, 0)); let f = filt(&a.effect, false); v.push(filter(ids, d, s, "in", &f)); }
        "splitOut" | "wipeOut" | "shapeOut" | "wheelOut" | "randomBarsOut" => { let f = filt(a.effect.trim_end_matches("Out"), true); v.push(filter(ids, d, s, "out", &f)); v.push(set_vis(ids, s, false, d)); }
        "zoom" | "growTurn" | "swivel" | "bounce" => {
            v.push(set_vis(ids, s, true, 0));
            v.push(anim_prop(ids, d, s, "ppt_w", "0", "#ppt_w"));
            v.push(anim_prop(ids, d, s, "ppt_h", if a.effect == "swivel" { "#ppt_h" } else { "0" }, "#ppt_h"));
            if a.effect == "growTurn" { v.push(anim_prop(ids, d, s, "style.rotation", "90", "0")); }
            v.push(filter(ids, d, s, "in", "fade"));
        }
        "zoomOut" | "shrinkTurn" | "swivelOut" | "bounceOut" => {
            v.push(anim_prop(ids, d, s, "ppt_w", "#ppt_w", "0"));
            v.push(anim_prop(ids, d, s, "ppt_h", "#ppt_h", if a.effect == "swivelOut" { "#ppt_h" } else { "0" }));
            if a.effect == "shrinkTurn" { v.push(anim_prop(ids, d, s, "style.rotation", "0", "90")); }
            v.push(filter(ids, d, s, "out", "fade"));
            v.push(set_vis(ids, s, false, d));
        }
        "spin" | "teeter" | "wave" => {
            let by: i64 = if a.effect == "spin" { o.parse::<i64>().unwrap_or(360) * 60_000 } else { 240_000 };
            let mut b = behavior(ids, &if a.effect == "spin" { d } else { d / 4 }.to_string(), s, Some("r"));
            if a.effect != "spin" { if let Some(c) = b.child_mut("p:cTn") { c.set_attr("autoRev", "1"); c.set_attr("repeatCount", "2000"); } }
            v.push(Element::new("p:animRot").with_attr("by", by.to_string()).with_child(b));
        }
        "pulse" | "growShrink" => {
            let pct: i64 = if a.effect == "pulse" { 105 } else { o.parse().unwrap_or(150) };
            let mut b = behavior(ids, &d.to_string(), s, None);
            if a.effect == "pulse" { if let Some(c) = b.child_mut("p:cTn") { c.set_attr("autoRev", "1"); } }
            v.push(Element::new("p:animScale").with_child(b).with_child(Element::new("p:by").with_attr("x", (pct * 1000).to_string()).with_attr("y", (pct * 1000).to_string())));
        }
        "transparency" => {
            let op = 1.0 - o.parse::<f64>().unwrap_or(50.0) / 100.0;
            v.push(Element::new("p:set").with_child(behavior(ids, &d.to_string(), s, Some("style.opacity"))).with_child(Element::new("p:to").with_child(Element::new("p:strVal").with_attr("val", format!("{op}")))));
        }
        "objectColor" | "fillColor" | "lineColor" | "fontColor" | "colorPulse" | "complementary" | "desaturate" | "darken" | "lighten" => {
            let attr = match a.effect.as_str() { "lineColor" => "stroke.color", "fontColor" => "style.color", _ => "fillcolor" };
            let color = a.color.clone().unwrap_or_else(|| match a.effect.as_str() { "darken" => "404040".into(), "lighten" => "D9D9D9".into(), "desaturate" => "A5A5A5".into(), _ => "FF0000".into() });
            let mut b = behavior(ids, &d.to_string(), s, Some(attr));
            if a.effect == "colorPulse" { if let Some(c) = b.child_mut("p:cTn") { c.set_attr("autoRev", "1"); } }
            v.push(Element::new("p:animClr").with_attr("clrSpc", "rgb").with_attr("dir", "cw").with_child(b).with_child(Element::new("p:to").with_child(Element::new("a:srgbClr").with_attr("val", color))));
        }
        "underline" | "boldFlash" | "boldReveal" => {
            let (attr, val) = if a.effect == "underline" { ("style.textDecorationUnderline", "true") } else { ("style.fontWeight", "bold") };
            let mut b = behavior(ids, &d.to_string(), s, Some(attr));
            if a.effect == "boldFlash" { if let Some(c) = b.child_mut("p:cTn") { c.set_attr("autoRev", "1"); } }
            v.push(Element::new("p:set").with_child(b).with_child(Element::new("p:to").with_child(Element::new("p:strVal").with_attr("val", val))));
        }
        "lines" | "arcs" | "turns" | "shapes" | "loops" => {
            let path = motion_path(&a.effect, o);
            let mut b = behavior(ids, &d.to_string(), s, Some("ppt_x"));
            if let Some(l) = b.child_mut("p:attrNameLst") { l.children.push(Node::Element(Element::new("p:attrName").with_text("ppt_y"))); }
            v.push(Element::new("p:animMotion").with_attr("origin", "layout").with_attr("path", path).with_attr("pathEditMode", "relative").with_child(b));
        }
        _ => { v.push(set_vis(ids, s, true, 0)); v.push(filter(ids, d, s, "in", "fade")); }
    }
    v
}

/// A motion path in slide fractions (PowerPoint's path syntax: M x y L x y C … E).
pub fn motion_path(effect: &str, option: &str) -> String {
    let (dx, dy): (f64, f64) = match option { "u" => (0.0, -0.25), "r" => (0.25, 0.0), "l" => (-0.25, 0.0), _ => (0.0, 0.25) };
    match effect {
        "arcs" => format!("M 0 0 C {} {} {} {} {} {} E", dx * 0.5 - dy * 0.5, dy * 0.5 + dx * 0.5, dx - dy * 0.5, dy + dx * 0.5, dx, dy),
        "turns" => format!("M 0 0 L {} {} L {} {} E", dx * 0.6, dy * 0.6, dx * 0.6 + dy.abs() * 0.4, dy * 0.6 + dx.abs() * 0.4),
        "shapes" => match option {
            "diamond" => "M 0 0 L 0.1 0.1 L 0 0.2 L -0.1 0.1 Z E".into(),
            "triangle" => "M 0 0 L 0.1 0.17 L -0.1 0.17 Z E".into(),
            "square" => "M 0 0 L 0.15 0 L 0.15 0.15 L 0 0.15 Z E".into(),
            _ => "M 0 0 C 0.055 0 0.1 0.045 0.1 0.1 C 0.1 0.155 0.055 0.2 0 0.2 C -0.055 0.2 -0.1 0.155 -0.1 0.1 C -0.1 0.045 -0.055 0 0 0 Z E".into(),
        },
        "loops" => "M 0 0 C 0.1 0 0.15 0.1 0.08 0.15 C 0 0.2 -0.05 0.1 0.02 0.06 C 0.1 0.02 0.2 0.1 0.25 0 E".into(),
        _ => format!("M 0 0 L {dx} {dy} E"),
    }
}

/// Points of a motion path (slide fractions), flattened.
pub fn path_points(path: &str) -> Vec<(f64, f64)> {
    let toks: Vec<&str> = path.split_whitespace().collect();
    let mut out = vec![(0.0, 0.0)];
    let mut cur = (0.0, 0.0);
    let mut i = 0;
    let num = |s: &str| s.parse::<f64>().unwrap_or(0.0);
    while i < toks.len() {
        match toks[i] {
            "M" | "m" if i + 2 < toks.len() => { cur = (num(toks[i + 1]), num(toks[i + 2])); out = vec![cur]; i += 3; }
            "L" | "l" if i + 2 < toks.len() => { cur = (num(toks[i + 1]), num(toks[i + 2])); out.push(cur); i += 3; }
            "C" | "c" if i + 6 < toks.len() => {
                let (a, b, e) = ((num(toks[i + 1]), num(toks[i + 2])), (num(toks[i + 3]), num(toks[i + 4])), (num(toks[i + 5]), num(toks[i + 6])));
                for k in 1..=12 {
                    let t = k as f64 / 12.0;
                    let m = 1.0 - t;
                    out.push((m * m * m * cur.0 + 3.0 * m * m * t * a.0 + 3.0 * m * t * t * b.0 + t * t * t * e.0, m * m * m * cur.1 + 3.0 * m * m * t * a.1 + 3.0 * m * t * t * b.1 + t * t * t * e.1));
                }
                cur = e;
                i += 7;
            }
            "Z" | "z" => { if let Some(f) = out.first().copied() { out.push(f); cur = f; } i += 1; }
            _ => i += 1,
        }
    }
    out
}

fn effect_par(a: &Anim, ids: &mut Ids, node_type: &str, grp: usize) -> Element {
    let p = preset(&a.effect);
    let mut ctn = Element::new("p:cTn").with_attr("id", ids.next())
        .with_attr("presetID", p.map(|p| p.preset).unwrap_or(10).to_string())
        .with_attr("presetClass", a.class().xml())
        .with_attr("presetSubtype", subtype(&a.option).to_string())
        .with_attr("fill", "hold").with_attr("grpId", grp.to_string()).with_attr("nodeType", node_type);
    if a.repeat > 0 { ctn.set_attr("repeatCount", if a.repeat == u32::MAX { "indefinite".to_string() } else { (a.repeat * 1000).to_string() }); }
    if a.rewind { ctn.set_attr("fill", "remove"); }
    ctn.children.push(Node::Element(cond(&a.delay_ms.to_string())));
    let mut kids = Element::new("p:childTnLst");
    for b in behaviors(a, ids) { kids.children.push(Node::Element(b)); }
    ctn.children.push(Node::Element(kids));
    // Remember our effect and option for exact round trips (PowerPoint ignores it).
    ctn.set_attr("qalamFx", format!("{}|{}|{}", a.effect, a.option, a.color.clone().unwrap_or_default()));
    Element::new("p:par").with_child(ctn)
}

fn subtype(option: &str) -> u32 {
    match option { "u" => 1, "r" => 2, "ru" => 3, "d" => 4, "rd" => 6, "ld" => 12, "l" => 8, "lu" => 9, "in" | "vertIn" => 21, "out" | "vertOut" => 42, "horzIn" => 26, "horzOut" => 37, "vert" => 5, "horz" => 10, _ => 0 }
}

impl Presentation {
    fn shape_ids(&self, slide: &str) -> Vec<u32> {
        let Some(tree) = self.slide(slide).and_then(|r| r.path("p:cSld/p:spTree")) else { return vec![] };
        tree.elements().filter_map(|e| e.elements().find(|c| c.local().starts_with("nv")).and_then(|nv| nv.child("p:cNvPr")).and_then(|c| c.attr_i64("id")).map(|i| i as u32)).collect()
    }
    /// The cNvPr id of a top-level shape.
    pub fn shape_id(&self, loc: &Loc) -> Option<u32> {
        self.element(loc)?.elements().find(|c| c.local().starts_with("nv"))?.child("p:cNvPr")?.attr_i64("id").map(|i| i as u32)
    }
    /// The location of the top-level shape with this id.
    pub fn shape_by_id(&self, slide: &str, id: u32) -> Option<Loc> {
        let tree = self.slide(slide)?.path("p:cSld/p:spTree")?;
        tree.elements().position(|e| e.elements().find(|c| c.local().starts_with("nv")).and_then(|nv| nv.child("p:cNvPr")).and_then(|c| c.attr_i64("id")) == Some(id as i64)).map(|i| Loc { part: slide.to_string(), path: vec![i] })
    }

    /// The slide's animations in playing order.
    pub fn animations(&self, slide: &str) -> Vec<Anim> {
        let Some(timing) = self.slide(slide).and_then(|r| r.child("p:timing")) else { return vec![] };
        let mut out = vec![];
        let mut effects = vec![];
        fn walk<'a>(e: &'a Element, out: &mut Vec<(&'a Element, Option<u32>)>, trigger: Option<u32>) {
            for c in e.elements() {
                if c.name == "p:cTn" && c.attr("presetClass").is_some() { out.push((c, trigger)); continue; }
                // Interactive sequences carry their trigger shape.
                let trig = if c.name == "p:seq" { c.find("p:triggerEvt").or_else(|| c.path("p:cTn/p:stCondLst")).and_then(|t| t.find("p:spTgt")).and_then(|s| s.attr_i64("spid")).map(|i| i as u32).filter(|_| c.path("p:cTn").and_then(|t| t.attr("nodeType")) == Some("interactiveSeq")) } else { None };
                walk(c, out, trig.or(trigger));
            }
        }
        walk(timing, &mut effects, None);
        for (ctn, trigger) in effects {
            let shape = ctn.find("p:spTgt").and_then(|s| s.attr_i64("spid")).unwrap_or(0) as u32;
            let class = Class::from_xml(ctn.attr("presetClass").unwrap_or("entr"));
            let pid = ctn.attr_i64("presetID").unwrap_or(10) as u32;
            let (effect, option, color) = match ctn.attr("qalamFx") {
                Some(q) => { let mut it = q.split('|'); (it.next().unwrap_or("fade").to_string(), it.next().unwrap_or("").to_string(), it.next().filter(|c| !c.is_empty()).map(String::from)) }
                None => {
                    let p = PRESETS.iter().find(|p| p.class == class && p.preset == pid).or_else(|| PRESETS.iter().find(|p| p.class == class));
                    let sub = ctn.attr_i64("presetSubtype").unwrap_or(0) as u32;
                    let opt = p.and_then(|p| p.options.iter().find(|o| subtype(o.0) == sub).or(p.options.first())).map(|o| o.0.to_string()).unwrap_or_default();
                    (p.map(|p| p.id).unwrap_or("fade").to_string(), opt, ctn.find("a:srgbClr").and_then(|c| c.attr("val")).map(String::from))
                }
            };
            let start = match ctn.attr("nodeType") { Some("withEffect") => Start::WithPrevious, Some("afterEffect") => Start::AfterPrevious, _ => Start::OnClick };
            let delay = ctn.path("p:stCondLst/p:cond").and_then(|c| c.attr_i64("delay")).unwrap_or(0).max(0) as u32;
            let mut durs = vec![];
            ctn.find_all("p:cTn", &mut durs);
            let duration = durs.iter().filter_map(|c| c.attr_i64("dur")).max().unwrap_or(500).max(0) as u32;
            let repeat = match ctn.attr("repeatCount") { Some("indefinite") => u32::MAX, Some(v) => (v.parse::<u32>().unwrap_or(1000) / 1000).max(1), None => 0 };
            out.push(Anim { shape, effect, option, start, duration_ms: if duration == 1 { 0 } else { duration }, delay_ms: delay, repeat, rewind: ctn.attr("fill") == Some("remove"), trigger, color });
        }
        out
    }

    /// Replace the slide's animations (in playing order).
    pub fn set_animations(&mut self, slide: &str, anims: &[Anim]) {
        let ids_on_slide = self.shape_ids(slide);
        let anims: Vec<&Anim> = anims.iter().filter(|a| ids_on_slide.contains(&a.shape)).collect();
        let Some(root) = self.pkg.xml_mut(slide) else { return };
        root.remove_children("p:timing");
        if anims.is_empty() { return; }
        let mut ids = Ids(2);
        let main: Vec<&&Anim> = anims.iter().filter(|a| a.trigger.is_none()).collect();
        // Click groups: each OnClick starts one; With/After join the current group.
        let mut groups: Vec<Vec<&Anim>> = vec![];
        for a in &main { if a.start == Start::OnClick || groups.is_empty() { groups.push(vec![]); } groups.last_mut().unwrap().push(a); }
        let mut seq_kids = Element::new("p:childTnLst");
        let build_group = |g: &[&Anim], ids: &mut Ids, click: bool| -> Element {
            let mut outer = Element::new("p:cTn").with_attr("id", ids.next()).with_attr("fill", "hold");
            outer.children.push(Node::Element(cond(if click { "indefinite" } else { "0" })));
            let mut outer_kids = Element::new("p:childTnLst");
            // Sub-groups by "after previous": each starts when the previous ends.
            let mut t = 0u32;
            let mut sub: Vec<(u32, Vec<&Anim>)> = vec![];
            let mut end = 0u32;
            for a in g {
                if a.start == Start::AfterPrevious || sub.is_empty() { if a.start == Start::AfterPrevious { t = end; } sub.push((t, vec![])); }
                sub.last_mut().unwrap().1.push(a);
                end = end.max(t + a.delay_ms + a.duration_ms);
            }
            for (at, list) in sub {
                let mut ctn = Element::new("p:cTn").with_attr("id", ids.next()).with_attr("fill", "hold");
                ctn.children.push(Node::Element(cond(&at.to_string())));
                let mut kids = Element::new("p:childTnLst");
                for a in list {
                    let nt = match a.start { Start::OnClick => "clickEffect", Start::WithPrevious => "withEffect", Start::AfterPrevious => "afterEffect" };
                    kids.children.push(Node::Element(effect_par(a, ids, nt, 0)));
                }
                ctn.children.push(Node::Element(kids));
                outer_kids.children.push(Node::Element(Element::new("p:par").with_child(ctn)));
            }
            outer.children.push(Node::Element(outer_kids));
            Element::new("p:par").with_child(outer)
        };
        for (k, g) in groups.iter().enumerate() {
            let click = g.first().is_some_and(|a| a.start == Start::OnClick) || k > 0;
            seq_kids.children.push(Node::Element(build_group(g, &mut ids, click)));
        }
        let main_seq = Element::new("p:seq").with_attr("concurrent", "1").with_attr("nextAc", "seek")
            .with_child(Element::new("p:cTn").with_attr("id", ids.next()).with_attr("dur", "indefinite").with_attr("nodeType", "mainSeq").with_child(seq_kids))
            .with_child(Element::new("p:prevCondLst").with_child(Element::new("p:cond").with_attr("evt", "onPrev").with_attr("delay", "0").with_child(Element::new("p:tgtEl").with_child(Element::new("p:sldTgt")))))
            .with_child(Element::new("p:nextCondLst").with_child(Element::new("p:cond").with_attr("evt", "onNext").with_attr("delay", "0").with_child(Element::new("p:tgtEl").with_child(Element::new("p:sldTgt")))));
        let mut root_kids = Element::new("p:childTnLst").with_child(main_seq);
        // Triggered animations: one interactive sequence per trigger shape.
        let mut triggers: Vec<u32> = anims.iter().filter_map(|a| a.trigger).collect();
        triggers.dedup();
        for trig in triggers {
            let list: Vec<&Anim> = anims.iter().filter(|a| a.trigger == Some(trig)).copied().collect();
            let mut kids = Element::new("p:childTnLst");
            kids.children.push(Node::Element(build_group(&list, &mut ids, false)));
            let mut ctn = Element::new("p:cTn").with_attr("id", ids.next()).with_attr("restart", "whenNotActive").with_attr("fill", "hold").with_attr("evtFilter", "cancelBubble").with_attr("nodeType", "interactiveSeq");
            ctn.children.push(Node::Element(Element::new("p:stCondLst").with_child(Element::new("p:cond").with_attr("evt", "onClick").with_attr("delay", "0").with_child(target(trig)))));
            ctn.children.push(Node::Element(kids));
            ctn.children.push(Node::Element(Element::new("p:endSync").with_attr("evt", "end").with_attr("delay", "0").with_child(Element::new("p:rtn").with_attr("val", "all"))));
            root_kids.children.push(Node::Element(Element::new("p:seq").with_attr("concurrent", "1").with_attr("nextAc", "seek").with_child(ctn).with_child(Element::new("p:nextCondLst").with_child(Element::new("p:cond").with_attr("evt", "onClick").with_attr("delay", "0").with_child(target(trig))))));
        }
        let tm_root = Element::new("p:cTn").with_attr("id", "1").with_attr("dur", "indefinite").with_attr("restart", "never").with_attr("nodeType", "tmRoot").with_child(root_kids);
        let mut bld = Element::new("p:bldLst");
        let mut seen = vec![];
        for a in &anims { if !seen.contains(&a.shape) { seen.push(a.shape); bld.children.push(Node::Element(Element::new("p:bldP").with_attr("spid", a.shape.to_string()).with_attr("grpId", "0").with_attr("animBg", "1"))); } }
        let timing = Element::new("p:timing").with_child(Element::new("p:tnLst").with_child(Element::new("p:par").with_child(tm_root))).with_child(bld);
        // p:timing comes after p:transition (and its AlternateContent), before extLst.
        let pos = root.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "p:extLst")).unwrap_or(root.children.len());
        root.children.insert(pos, Node::Element(timing));
    }
}

// ---- the timeline ---------------------------------------------------------------------------

/// When each animation plays: click step (0 = automatically on entering the slide), and
/// start time (ms) within that step.
#[derive(Clone, Debug, PartialEq)]
pub struct Timed { pub index: usize, pub step: usize, pub at_ms: u32 }

/// The schedule of the main sequence (triggers excluded) and the number of click steps.
pub fn schedule(anims: &[Anim]) -> (Vec<Timed>, usize) {
    let mut out = vec![];
    let mut step = 0usize;
    let mut group_start_end = (0u32, 0u32); // (start of current sub-group, end of everything so far)
    let mut first = true;
    for (i, a) in anims.iter().enumerate() {
        if a.trigger.is_some() { continue; }
        let at = match a.start {
            Start::OnClick => { step += 1; group_start_end = (0, 0); 0 }
            Start::WithPrevious => { if first { group_start_end = (0, 0); } group_start_end.0 }
            Start::AfterPrevious => { let s = group_start_end.1; group_start_end.0 = s; s }
        };
        let at = at + a.delay_ms;
        group_start_end.1 = group_start_end.1.max(at + a.duration_ms.max(1));
        if a.start == Start::WithPrevious || a.start == Start::OnClick { group_start_end.0 = at.saturating_sub(a.delay_ms); }
        out.push(Timed { index: i, step, at_ms: at });
        first = false;
    }
    (out, step)
}

/// How an animated object looks at a moment: shown, opacity, offset (slide fractions),
/// scale, rotation (degrees), a reveal (effect name, progress), a color change (hex, amount).
#[derive(Clone, Debug, PartialEq)]
pub struct Pose { pub visible: bool, pub opacity: f32, pub dx: f64, pub dy: f64, pub sx: f64, pub sy: f64, pub rot: f64, pub reveal: Option<(String, String, f32, bool)>, pub color: Option<(String, f32)>, pub bold: bool, pub underline: bool }

impl Default for Pose { fn default() -> Self { Pose { visible: true, opacity: 1.0, dx: 0.0, dy: 0.0, sx: 1.0, sy: 1.0, rot: 0.0, reveal: None, color: None, bold: false, underline: false } } }

fn ease(t: f64) -> f64 { t * t * (3.0 - 2.0 * t) }

/// The pose of a shape after the animations `anims` (with their elapsed ms; None = not yet
/// started) have run, given `w`, `h` (the shape's size as slide fractions).
pub fn pose(anims: &[(&Anim, Option<f64>)], w: f64, h: f64) -> Pose {
    let mut p = Pose::default();
    // Before its entrance starts, an object with an entrance effect is hidden.
    if anims.first().is_some_and(|(a, _)| a.class() == Class::Entrance) { p.visible = false; }
    for (a, elapsed) in anims {
        let Some(el) = elapsed else { continue };
        let dur = a.duration_ms.max(1) as f64;
        let mut t = (el / dur).clamp(0.0, 1.0);
        if a.repeat > 0 && *el > 0.0 { let reps = if a.repeat == u32::MAX { 1e9 } else { a.repeat as f64 }; if el / dur < reps { t = (el / dur).fract(); if el / dur >= reps - 1e-9 { t = 1.0; } } }
        let e = ease(t);
        let o = a.option.as_str();
        match a.effect.as_str() {
            "appear" => p.visible = true,
            "disappear" => { if t >= 1.0 { p.visible = false; } }
            "fade" => { p.visible = true; p.opacity = e as f32; }
            "fadeOut" => { p.opacity = (1.0 - e) as f32; if t >= 1.0 { p.visible = false; } }
            "flyIn" | "flyOut" => {
                let out = a.effect == "flyOut";
                p.visible = !(out && t >= 1.0) || !out;
                let k = if out { e } else { 1.0 - e };
                let (fx, fy) = match o { "l" => (-1.0 - w, 0.0), "r" => (1.0 + w, 0.0), "u" => (0.0, -1.0 - h), "ld" => (-1.0 - w, 1.0 + h), "rd" => (1.0 + w, 1.0 + h), "lu" => (-1.0 - w, -1.0 - h), "ru" => (1.0 + w, -1.0 - h), _ => (0.0, 1.0 + h) };
                p.dx += fx * k; p.dy += fy * k;
                if out && t >= 1.0 { p.visible = false; }
            }
            "floatIn" => { p.visible = true; p.opacity = e as f32; p.dy += if o == "d" { -0.1 } else { 0.1 } * (1.0 - e); }
            "floatOut" => { p.opacity = (1.0 - e) as f32; p.dy += if o == "u" { -0.1 } else { 0.1 } * e; if t >= 1.0 { p.visible = false; } }
            "split" | "wipe" | "shape" | "wheel" | "randomBars" => { p.visible = true; p.reveal = if t < 1.0 { Some((a.effect.clone(), o.to_string(), t as f32, false)) } else { None }; }
            "splitOut" | "wipeOut" | "shapeOut" | "wheelOut" | "randomBarsOut" => { p.reveal = if t < 1.0 { Some((a.effect.trim_end_matches("Out").to_string(), o.to_string(), (1.0 - t) as f32, true)) } else { None }; if t >= 1.0 { p.visible = false; } }
            "zoom" => { p.visible = true; p.sx = e; p.sy = e; p.opacity = e as f32; }
            "zoomOut" => { p.sx = 1.0 - e; p.sy = 1.0 - e; p.opacity = (1.0 - e) as f32; if t >= 1.0 { p.visible = false; } }
            "growTurn" => { p.visible = true; p.sx = e; p.sy = e; p.rot += 90.0 * (1.0 - e); p.opacity = e as f32; }
            "shrinkTurn" => { p.sx = 1.0 - e; p.sy = 1.0 - e; p.rot += 90.0 * e; p.opacity = (1.0 - e) as f32; if t >= 1.0 { p.visible = false; } }
            "swivel" => { p.visible = true; let s = (e * std::f64::consts::PI * 2.5).cos().abs().max(0.02) * e + (1.0 - e) * 0.02; if o == "vert" { p.sy = s.max(e); } else { p.sx = s.max(e); } p.opacity = e as f32; }
            "swivelOut" => { let s = 1.0 - e; if o == "vert" { p.sy = s; } else { p.sx = s; } p.opacity = (1.0 - e) as f32; if t >= 1.0 { p.visible = false; } }
            "bounce" => { p.visible = true; p.opacity = (t * 3.0).min(1.0) as f32; let b = (1.0 - t).powi(2) * (t * std::f64::consts::PI * 3.5).cos().abs(); p.dy -= 0.3 * b; }
            "bounceOut" => { let b = t.powi(2) * (t * std::f64::consts::PI * 3.5).cos().abs(); p.dy -= 0.3 * b; p.opacity = (1.0 - t) as f32; if t >= 1.0 { p.visible = false; } }
            "pulse" => { let s = 1.0 + 0.05 * (t * std::f64::consts::PI).sin(); p.sx *= s; p.sy *= s; }
            "growShrink" => { let k = o.parse::<f64>().unwrap_or(150.0) / 100.0; let s = 1.0 + (k - 1.0) * e; p.sx *= s; p.sy *= s; }
            "spin" => p.rot += o.parse::<f64>().unwrap_or(360.0) * e,
            "teeter" => p.rot += 4.0 * (t * std::f64::consts::PI * 6.0).sin() * (1.0 - t),
            "wave" => { p.dy += 0.02 * (t * std::f64::consts::PI * 2.0).sin(); p.rot += 3.0 * (t * std::f64::consts::PI * 4.0).sin() * (1.0 - t); }
            "transparency" => p.opacity = (1.0 - (o.parse::<f64>().unwrap_or(50.0) / 100.0) * e) as f32,
            "objectColor" | "fillColor" | "lineColor" | "fontColor" | "complementary" | "desaturate" | "darken" | "lighten" => {
                let c = a.color.clone().unwrap_or_else(|| match a.effect.as_str() { "darken" => "404040".into(), "lighten" => "D9D9D9".into(), "desaturate" => "A5A5A5".into(), "complementary" => "00B0F0".into(), _ => "FF0000".into() });
                p.color = Some((c, e as f32));
            }
            "colorPulse" => p.color = Some((a.color.clone().unwrap_or_else(|| "FF0000".into()), (t * std::f64::consts::PI).sin() as f32)),
            "underline" => p.underline = t > 0.0,
            "boldFlash" => p.bold = t > 0.0 && t < 1.0,
            "boldReveal" => p.bold = t > 0.0,
            "lines" | "arcs" | "turns" | "shapes" | "loops" => {
                let pts = path_points(&motion_path(&a.effect, o));
                if pts.len() > 1 {
                    let f = e * (pts.len() - 1) as f64;
                    let k = (f.floor() as usize).min(pts.len() - 2);
                    let u = f - k as f64;
                    p.dx += pts[k].0 + (pts[k + 1].0 - pts[k].0) * u;
                    p.dy += pts[k].1 + (pts[k + 1].1 - pts[k].1) * u;
                }
            }
            _ => { p.visible = true; p.opacity = e as f32; }
        }
        if a.rewind && t >= 1.0 { p = Pose { visible: p.visible, ..Pose::default() }; }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Xfrm;

    #[test]
    fn animations_round_trip_and_schedule() {
        let mut p = Presentation::new_blank(true);
        let l = p.all_layouts()[6].clone();
        let s = p.add_slide(&l, 0);
        let x = Xfrm { x: 1e6, y: 1e6, w: 2e6, h: 1e6, ..Default::default() };
        let a = p.insert_shape(&s, "rect", &x, false).unwrap();
        let b = p.insert_shape(&s, "ellipse", &x, false).unwrap();
        let (ia, ib) = (p.shape_id(&a).unwrap(), p.shape_id(&b).unwrap());
        let mut list = vec![];
        for pr in PRESETS { let mut an = Anim::new(ia, pr.id); an.start = Start::OnClick; list.push(an); }
        let mut with = Anim::new(ib, "spin"); with.start = Start::WithPrevious; with.delay_ms = 250; list.push(with);
        let mut after = Anim::new(ib, "fadeOut"); after.start = Start::AfterPrevious; list.push(after);
        let mut trig = Anim::new(ib, "pulse"); trig.trigger = Some(ia); list.push(trig);
        p.set_animations(&s, &list);
        let back = p.animations(&s);
        assert_eq!(back.len(), list.len());
        for (x, y) in list.iter().zip(&back) {
            assert_eq!((x.effect.as_str(), x.option.as_str(), x.start, x.delay_ms, x.trigger), (y.effect.as_str(), y.option.as_str(), y.start, y.delay_ms, y.trigger), "{}", x.effect);
        }
        let (sched, steps) = schedule(&back);
        assert_eq!(steps, PRESETS.len());
        let w = sched.iter().rfind(|t| back[t.index].effect == "spin").unwrap();
        assert_eq!((w.step, w.at_ms), (steps, 250));
        let again = Presentation::open(&p.save().unwrap()).unwrap();
        assert_eq!(again.animations(&again.slides()[0]).len(), list.len());
        // Poses: a fly-in starts off the slide and ends in place.
        let fly = Anim::new(ia, "flyIn");
        assert!(pose(&[(&fly, Some(0.0))], 0.2, 0.2).dy > 1.0);
        assert_eq!(pose(&[(&fly, Some(10_000.0))], 0.2, 0.2).dy, 0.0);
        assert!(!pose(&[(&fly, None)], 0.2, 0.2).visible);
    }
}
