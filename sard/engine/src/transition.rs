//! Slide transitions (Transitions tab): the PowerPoint gallery — Subtle, Exciting and Dynamic
//! Content — with each effect's options, duration, sound and advance timing, read from and
//! written to the slide's p:transition (standard effects directly, PowerPoint 2010/2013
//! effects and Morph inside mc:AlternateContent with a fade fallback, as PowerPoint saves them).

use crate::opc;
use crate::pres::Presentation;
use crate::xml::{Element, Node};

/// One effect: id, name, category, options (id, label).
pub struct Effect { pub id: &'static str, pub name: &'static str, pub category: &'static str, pub options: &'static [(&'static str, &'static str)] }

const DIR4: &[(&str, &str)] = &[("r", "From Right"), ("l", "From Left"), ("u", "From Top"), ("d", "From Bottom")];
const DIR8: &[(&str, &str)] = &[("r", "From Right"), ("u", "From Top"), ("l", "From Left"), ("d", "From Bottom"), ("ru", "From Top-Right"), ("rd", "From Bottom-Right"), ("lu", "From Top-Left"), ("ld", "From Bottom-Left")];
const LR: &[(&str, &str)] = &[("r", "From Right"), ("l", "From Left")];
const HV: &[(&str, &str)] = &[("vert", "Vertical"), ("horz", "Horizontal")];
const INOUT: &[(&str, &str)] = &[("in", "In"), ("out", "Out")];

pub const EFFECTS: &[Effect] = &[
    Effect { id: "none", name: "None", category: "Subtle", options: &[] },
    Effect { id: "morph", name: "Morph", category: "Subtle", options: &[("byObject", "Objects"), ("byWord", "Words"), ("byChar", "Characters")] },
    Effect { id: "fade", name: "Fade", category: "Subtle", options: &[("smooth", "Smoothly"), ("black", "Through Black")] },
    Effect { id: "push", name: "Push", category: "Subtle", options: &[("d", "From Bottom"), ("l", "From Left"), ("r", "From Right"), ("u", "From Top")] },
    Effect { id: "wipe", name: "Wipe", category: "Subtle", options: DIR8 },
    Effect { id: "split", name: "Split", category: "Subtle", options: &[("vertOut", "Vertical Out"), ("vertIn", "Vertical In"), ("horzOut", "Horizontal Out"), ("horzIn", "Horizontal In")] },
    Effect { id: "reveal", name: "Reveal", category: "Subtle", options: &[("smoothL", "Smoothly From Right"), ("smoothR", "Smoothly From Left"), ("blackL", "Through Black From Right"), ("blackR", "Through Black From Left")] },
    Effect { id: "cut", name: "Cut", category: "Subtle", options: &[("plain", "Cut"), ("black", "Through Black")] },
    Effect { id: "randomBars", name: "Random Bars", category: "Subtle", options: &[("vert", "Vertical"), ("horz", "Horizontal")] },
    Effect { id: "shape", name: "Shape", category: "Subtle", options: &[("circle", "Circle"), ("diamond", "Diamond"), ("plus", "Plus"), ("in", "In"), ("out", "Out")] },
    Effect { id: "uncover", name: "Uncover", category: "Subtle", options: DIR8 },
    Effect { id: "cover", name: "Cover", category: "Subtle", options: DIR8 },
    Effect { id: "flash", name: "Flash", category: "Subtle", options: &[] },
    Effect { id: "fallOver", name: "Fall Over", category: "Exciting", options: &[("l", "Left"), ("r", "Right")] },
    Effect { id: "drape", name: "Drape", category: "Exciting", options: &[("l", "Left"), ("r", "Right")] },
    Effect { id: "curtains", name: "Curtains", category: "Exciting", options: &[] },
    Effect { id: "wind", name: "Wind", category: "Exciting", options: &[("r", "Right"), ("l", "Left")] },
    Effect { id: "prestige", name: "Prestige", category: "Exciting", options: &[] },
    Effect { id: "fracture", name: "Fracture", category: "Exciting", options: &[] },
    Effect { id: "crush", name: "Crush", category: "Exciting", options: &[] },
    Effect { id: "peelOff", name: "Peel Off", category: "Exciting", options: &[("l", "Left"), ("r", "Right")] },
    Effect { id: "pageCurl", name: "Page Curl", category: "Exciting", options: &[("doubleL", "Double Left"), ("doubleR", "Double Right"), ("singleL", "Single Left"), ("singleR", "Single Right")] },
    Effect { id: "airplane", name: "Airplane", category: "Exciting", options: &[("l", "Left"), ("r", "Right")] },
    Effect { id: "origami", name: "Origami", category: "Exciting", options: &[("l", "Left"), ("r", "Right")] },
    Effect { id: "dissolve", name: "Dissolve", category: "Exciting", options: &[] },
    Effect { id: "checkerboard", name: "Checkerboard", category: "Exciting", options: &[("horz", "From Left"), ("vert", "From Top")] },
    Effect { id: "blinds", name: "Blinds", category: "Exciting", options: HV },
    Effect { id: "clock", name: "Clock", category: "Exciting", options: &[("cw", "Clockwise"), ("ccw", "Counterclockwise"), ("wedge", "Wedge")] },
    Effect { id: "ripple", name: "Ripple", category: "Exciting", options: &[("center", "Center"), ("lu", "From Top-Left"), ("ru", "From Top-Right"), ("ld", "From Bottom-Left"), ("rd", "From Bottom-Right")] },
    Effect { id: "honeycomb", name: "Honeycomb", category: "Exciting", options: &[] },
    Effect { id: "glitter", name: "Glitter", category: "Exciting", options: &[("diamondL", "Diamonds From Left"), ("diamondR", "Diamonds From Right"), ("hexagonL", "Hexagons From Left"), ("hexagonR", "Hexagons From Right")] },
    Effect { id: "vortex", name: "Vortex", category: "Exciting", options: DIR4 },
    Effect { id: "shred", name: "Shred", category: "Exciting", options: &[("stripIn", "Strips In"), ("stripOut", "Strips Out"), ("crushIn", "Particles In"), ("crushOut", "Particles Out")] },
    Effect { id: "switch", name: "Switch", category: "Exciting", options: LR },
    Effect { id: "flip", name: "Flip", category: "Exciting", options: LR },
    Effect { id: "gallery", name: "Gallery", category: "Exciting", options: LR },
    Effect { id: "cube", name: "Cube", category: "Exciting", options: DIR4 },
    Effect { id: "doors", name: "Doors", category: "Exciting", options: HV },
    Effect { id: "box", name: "Box", category: "Exciting", options: DIR4 },
    Effect { id: "comb", name: "Comb", category: "Exciting", options: HV },
    Effect { id: "zoom", name: "Zoom", category: "Exciting", options: &[("in", "In"), ("out", "Out"), ("inRound", "Zoom and Rotate")] },
    Effect { id: "random", name: "Random", category: "Exciting", options: &[] },
    Effect { id: "pan", name: "Pan", category: "Dynamic Content", options: DIR4 },
    Effect { id: "ferris", name: "Ferris Wheel", category: "Dynamic Content", options: LR },
    Effect { id: "conveyor", name: "Conveyor", category: "Dynamic Content", options: LR },
    Effect { id: "rotate", name: "Rotate", category: "Dynamic Content", options: DIR4 },
    Effect { id: "window", name: "Window", category: "Dynamic Content", options: HV },
    Effect { id: "orbit", name: "Orbit", category: "Dynamic Content", options: DIR4 },
    Effect { id: "flythrough", name: "Fly Through", category: "Dynamic Content", options: &[("in", "In"), ("out", "Out"), ("inBounce", "In with Bounce"), ("outBounce", "Out with Bounce")] },
];

pub fn effect(id: &str) -> Option<&'static Effect> { EFFECTS.iter().find(|e| e.id == id) }

/// Transition sounds offered (name; generated as short WAV tones when chosen).
pub const SOUNDS: &[&str] = &["Applause", "Arrow", "Bomb", "Breeze", "Camera", "Cash Register", "Chime", "Click", "Coin", "Drum Roll", "Explosion", "Hammer", "Laser", "Push", "Suction", "Typewriter", "Voltage", "Whoosh", "Wind"];

/// A slide's transition settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub effect: String,
    pub option: String,
    pub duration_ms: u32,
    pub on_click: bool,
    pub after_ms: Option<u32>,
    pub sound: Option<String>,
}

impl Default for Transition {
    fn default() -> Self { Transition { effect: "none".into(), option: String::new(), duration_ms: 0, on_click: true, after_ms: None, sound: None } }
}

/// The default duration of an effect (ms), as PowerPoint gives them.
pub fn default_duration(effect: &str) -> u32 {
    match effect {
        "none" => 0, "cut" => 100, "fade" | "push" | "wipe" | "split" | "cover" | "uncover" | "randomBars" | "shape" | "flash" => 700,
        "morph" | "dissolve" | "checkerboard" | "blinds" | "clock" | "comb" | "zoom" | "reveal" => 1000,
        "ripple" | "glitter" | "vortex" | "shred" | "honeycomb" | "crush" | "fracture" | "prestige" | "curtains" => 2000,
        _ => 1600,
    }
}

const NS_P14: &str = "http://schemas.microsoft.com/office/powerpoint/2010/main";
const NS_P15: &str = "http://schemas.microsoft.com/office/powerpoint/2012/main";
const NS_P159: &str = "http://schemas.microsoft.com/office/powerpoint/2015/09/main";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

fn el(name: &str, attrs: &[(&str, &str)]) -> Element { let mut e = Element::new(name); for (k, v) in attrs { e.set_attr(k, *v); } e }

/// The effect element, and the namespace prefix it needs (None = standard).
fn effect_element(effect: &str, option: &str) -> Option<(Element, Option<&'static str>)> {
    let d = |o: &str, dflt: &str| if o.is_empty() { dflt.to_string() } else { o.to_string() };
    let dir_from = |o: &str| match o { "r" => "l", "l" => "r", "u" => "d", "d" => "u", "ru" => "ld", "rd" => "lu", "lu" => "rd", "ld" => "ru", x => x }.to_string();
    let prst = |name: &str, inv: bool| { let mut e = el("p15:prstTrans", &[("prst", name)]); if inv { e.set_attr("invX", "1"); } (e, Some("p15")) };
    Some(match effect {
        "none" => return None,
        "cut" => (if option == "black" { el("p:cut", &[("thruBlk", "1")]) } else { el("p:cut", &[]) }, None),
        "fade" => (if option == "black" { el("p:fade", &[("thruBlk", "1")]) } else { el("p:fade", &[]) }, None),
        "push" => (el("p:push", &[("dir", &dir_from(&d(option, "d")))]), None),
        "wipe" => (el("p:wipe", &[("dir", &dir_from(&d(option, "r")))]), None),
        "split" => { let o = d(option, "vertOut"); (el("p:split", &[("orient", if o.starts_with("vert") { "vert" } else { "horz" }), ("dir", if o.ends_with("In") { "in" } else { "out" })]), None) }
        "reveal" => { let o = d(option, "smoothL"); let mut e = el("p14:reveal", &[("dir", if o.ends_with('L') { "l" } else { "r" })]); if o.starts_with("black") { e.set_attr("thruBlk", "1"); } (e, Some("p14")) }
        "randomBars" => (el("p:randomBar", &[("dir", &d(option, "vert"))]), None),
        "shape" => match d(option, "circle").as_str() { "diamond" => (el("p:diamond", &[]), None), "plus" => (el("p:plus", &[]), None), "in" => (el("p:zoom", &[("dir", "in")]), None), "out" => (el("p:zoom", &[("dir", "out")]), None), _ => (el("p:circle", &[]), None) },
        "uncover" => (el("p:pull", &[("dir", &dir_from(&d(option, "l")))]), None),
        "cover" => (el("p:cover", &[("dir", &dir_from(&d(option, "r")))]), None),
        "flash" => (el("p14:flash", &[]), Some("p14")),
        "morph" => (el("p159:morph", &[("option", &d(option, "byObject"))]), Some("p159")),
        "fallOver" => prst("fallOver", option == "r"),
        "drape" => prst("drape", option == "r"),
        "curtains" => prst("curtains", false),
        "wind" => prst("wind", option == "l"),
        "prestige" => prst("prestige", false),
        "fracture" => prst("fracture", false),
        "crush" => prst("crush", false),
        "peelOff" => prst("peelOff", option == "r"),
        "pageCurl" => prst(if option.starts_with("single") { "pageCurlSingle" } else { "pageCurlDouble" }, option.ends_with('R')),
        "airplane" => prst("airplane", option == "r"),
        "origami" => prst("origami", option == "r"),
        "dissolve" => (el("p:dissolve", &[]), None),
        "checkerboard" => (el("p:checker", &[("dir", &d(option, "horz"))]), None),
        "blinds" => (el("p:blinds", &[("dir", &d(option, "vert"))]), None),
        "clock" => match d(option, "cw").as_str() { "ccw" => (el("p14:wheelReverse", &[("spokes", "1")]), Some("p14")), "wedge" => (el("p:wedge", &[]), None), _ => (el("p:wheel", &[("spokes", "1")]), None) },
        "ripple" => (el("p14:ripple", &[("dir", &d(option, "center"))]), Some("p14")),
        "honeycomb" => (el("p14:honeycomb", &[]), Some("p14")),
        "glitter" => { let o = d(option, "diamondL"); (el("p14:glitter", &[("pattern", if o.starts_with("hex") { "hexagon" } else { "diamond" }), ("dir", if o.ends_with('R') { "r" } else { "l" })]), Some("p14")) }
        "vortex" => (el("p14:vortex", &[("dir", &d(option, "l"))]), Some("p14")),
        "shred" => { let o = d(option, "stripIn"); (el("p14:shred", &[("pattern", if o.starts_with("crush") { "crush" } else { "strip" }), ("dir", if o.ends_with("Out") { "out" } else { "in" })]), Some("p14")) }
        "switch" => (el("p14:switch", &[("dir", &d(option, "r"))]), Some("p14")),
        "flip" => (el("p14:flip", &[("dir", &d(option, "r"))]), Some("p14")),
        "gallery" => (el("p14:gallery", &[("dir", &d(option, "r"))]), Some("p14")),
        "cube" => (el("p14:prism", &[("dir", &d(option, "l"))]), Some("p14")),
        "box" => (el("p14:prism", &[("dir", &d(option, "l")), ("isInverted", "1")]), Some("p14")),
        "rotate" => (el("p14:prism", &[("dir", &d(option, "l")), ("isContent", "1")]), Some("p14")),
        "orbit" => (el("p14:prism", &[("dir", &d(option, "l")), ("isContent", "1"), ("isInverted", "1")]), Some("p14")),
        "doors" => (el("p14:doors", &[("dir", &d(option, "vert"))]), Some("p14")),
        "comb" => (el("p:comb", &[("dir", &d(option, "horz"))]), None),
        "zoom" if option == "inRound" => (el("p:newsflash", &[]), None),
        "zoom" => (el("p14:warp", &[("dir", if option == "out" { "out" } else { "in" })]), Some("p14")),
        "random" => (el("p:random", &[]), None),
        "pan" => (el("p14:pan", &[("dir", &d(option, "u"))]), Some("p14")),
        "ferris" => (el("p14:ferris", &[("dir", &d(option, "l"))]), Some("p14")),
        "conveyor" => (el("p14:conveyor", &[("dir", &d(option, "l"))]), Some("p14")),
        "window" => (el("p14:window", &[("dir", &d(option, "vert"))]), Some("p14")),
        "flythrough" => { let o = d(option, "in"); let mut e = el("p14:flythrough", &[("dir", if o.starts_with("out") { "out" } else { "in" })]); if o.ends_with("Bounce") { e.set_attr("hasBounce", "1"); } (e, Some("p14")) }
        _ => (el("p:fade", &[]), None),
    })
}

/// Read an effect element back to (effect id, option).
fn read_effect(e: &Element) -> (String, String) {
    let a = |k: &str| e.attr(k).unwrap_or("").to_string();
    let from_dir = |o: &str| match o { "l" => "r", "r" => "l", "u" => "d", "d" => "u", "ld" => "ru", "lu" => "rd", "rd" => "lu", "ru" => "ld", x => x }.to_string();
    let inv = e.attr_bool("invX").unwrap_or(false);
    let (id, opt): (&str, String) = match e.local() {
        "cut" => ("cut", if e.attr_bool("thruBlk") == Some(true) { "black".into() } else { "plain".into() }),
        "fade" => ("fade", if e.attr_bool("thruBlk") == Some(true) { "black".into() } else { "smooth".into() }),
        "push" => ("push", from_dir(&a("dir"))), "wipe" => ("wipe", from_dir(&a("dir"))),
        "split" => ("split", format!("{}{}", if a("orient") == "horz" { "horz" } else { "vert" }, if a("dir") == "in" { "In" } else { "Out" })),
        "reveal" => ("reveal", format!("{}{}", if e.attr_bool("thruBlk") == Some(true) { "black" } else { "smooth" }, if a("dir") == "r" { "R" } else { "L" })),
        "randomBar" => ("randomBars", a("dir")),
        "circle" => ("shape", "circle".into()), "diamond" => ("shape", "diamond".into()), "plus" => ("shape", "plus".into()),
        "zoom" => ("shape", a("dir")),
        "pull" => ("uncover", from_dir(&a("dir"))), "cover" => ("cover", from_dir(&a("dir"))),
        "flash" => ("flash", String::new()), "morph" => ("morph", a("option")),
        "prstTrans" => {
            let p = a("prst");
            let id = match p.as_str() { "fallOver" => "fallOver", "drape" => "drape", "curtains" => "curtains", "wind" => "wind", "prestige" => "prestige", "fracture" => "fracture", "crush" => "crush", "peelOff" => "peelOff", "pageCurlDouble" | "pageCurlSingle" => "pageCurl", "airplane" => "airplane", "origami" => "origami", _ => "fade" };
            let opt = if id == "pageCurl" { format!("{}{}", if p.ends_with("Single") { "single" } else { "double" }, if inv { "R" } else { "L" }) } else if id == "wind" { if inv { "l".into() } else { "r".into() } } else if inv { "r".into() } else { "l".into() };
            (id, opt)
        }
        "dissolve" => ("dissolve", String::new()), "checker" => ("checkerboard", a("dir")), "blinds" => ("blinds", a("dir")),
        "wheel" => ("clock", "cw".into()), "wheelReverse" => ("clock", "ccw".into()), "wedge" => ("clock", "wedge".into()),
        "ripple" => ("ripple", a("dir")), "honeycomb" => ("honeycomb", String::new()),
        "glitter" => ("glitter", format!("{}{}", if a("pattern") == "hexagon" { "hexagon" } else { "diamond" }, if a("dir") == "r" { "R" } else { "L" })),
        "vortex" => ("vortex", a("dir")),
        "shred" => ("shred", format!("{}{}", if a("pattern") == "crush" { "crush" } else { "strip" }, if a("dir") == "out" { "Out" } else { "In" })),
        "switch" => ("switch", a("dir")), "flip" => ("flip", a("dir")), "gallery" => ("gallery", a("dir")),
        "prism" => { let (c, i) = (e.attr_bool("isContent").unwrap_or(false), e.attr_bool("isInverted").unwrap_or(false)); (match (c, i) { (false, false) => "cube", (false, true) => "box", (true, false) => "rotate", _ => "orbit" }, a("dir")) }
        "doors" => ("doors", a("dir")), "comb" => ("comb", a("dir")), "warp" => ("zoom", a("dir")), "newsflash" => ("zoom", "inRound".into()), "random" => ("random", String::new()),
        "pan" => ("pan", a("dir")), "ferris" => ("ferris", a("dir")), "conveyor" => ("conveyor", a("dir")), "window" => ("window", a("dir")),
        "flythrough" => ("flythrough", format!("{}{}", a("dir"), if e.attr_bool("hasBounce") == Some(true) { "Bounce" } else { "" })),
        _ => ("fade", String::new()),
    };
    // Effects without options carry none.
    let opt = if effect(id).is_some_and(|e| e.options.is_empty()) { String::new() } else { opt };
    (id.to_string(), opt)
}

/// A short WAV tone for a transition sound (synthesized; each name has its own character).
pub fn sound_wav(name: &str) -> Vec<u8> {
    let rate = 22_050u32;
    let seed = name.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let (dur, f0, f1, noise) = match name {
        "Applause" | "Breeze" | "Wind" | "Whoosh" | "Suction" | "Explosion" | "Drum Roll" => (0.6, 0.0, 0.0, 1.0),
        "Chime" | "Coin" | "Cash Register" => (0.5, 1320.0, 1760.0, 0.0),
        "Click" | "Camera" | "Typewriter" | "Hammer" => (0.08, 1800.0, 900.0, 0.4),
        "Laser" | "Voltage" => (0.35, 2200.0, 300.0, 0.0),
        _ => (0.3, 600.0 + (seed % 600) as f64, 300.0, 0.1),
    };
    let n = (rate as f64 * dur) as usize;
    let mut s = seed;
    let mut data = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f64 / rate as f64;
        let env = (1.0 - t / dur).max(0.0).powf(1.5) * (t * 200.0).min(1.0);
        let f = f0 + (f1 - f0) * t / dur;
        s ^= s << 13; s ^= s >> 17; s ^= s << 5;
        let nz = (s % 2000) as f64 / 1000.0 - 1.0;
        let v = ((2.0 * std::f64::consts::PI * f * t).sin() * (1.0 - noise) + nz * noise) * env * 0.5;
        data.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    let mut w = Vec::with_capacity(44 + data.len());
    w.extend_from_slice(b"RIFF"); w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes()); w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes()); w.extend_from_slice(&1u16.to_le_bytes()); w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes()); w.extend_from_slice(&(rate * 2).to_le_bytes()); w.extend_from_slice(&2u16.to_le_bytes()); w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data"); w.extend_from_slice(&(data.len() as u32).to_le_bytes()); w.extend_from_slice(&data);
    w
}

impl Presentation {
    fn transition_element(&self, slide: &str) -> Option<Element> {
        let root = self.pkg.xml(slide)?;
        // The richest form: an AlternateContent choice, else the plain transition.
        root.children_named("mc:AlternateContent").find_map(|ac| ac.child("mc:Choice").and_then(|c| c.child("p:transition")).cloned()).or_else(|| root.child("p:transition").cloned())
    }

    /// The slide's transition settings.
    pub fn slide_transition(&self, slide: &str) -> Transition {
        let Some(t) = self.transition_element(slide) else { return Transition::default() };
        let fx = t.elements().find(|e| !matches!(e.local(), "sndAc" | "extLst"));
        let (effect, option) = fx.map(read_effect).unwrap_or(("none".into(), String::new()));
        let duration = t.attr_i64("p14:dur").map(|d| d as u32).unwrap_or_else(|| match t.attr("spd") { Some("fast") => 500, Some("slow") => 1000, Some(_) => 750, None => default_duration(&effect) });
        let sound = t.path("p:sndAc/p:stSnd/p:snd").and_then(|s| s.attr("name")).map(|n| n.trim_end_matches(".wav").to_string());
        Transition { effect, option, duration_ms: duration, on_click: t.attr("advClick") != Some("0"), after_ms: t.attr_i64("advTm").map(|v| v as u32), sound }
    }

    /// Write the slide's transition.
    pub fn set_slide_transition(&mut self, slide: &str, t: &Transition) {
        // A sound is embedded as a WAV next to the slide.
        let sound_rel = t.sound.as_ref().map(|name| {
            let media = self.pkg.free_name("ppt/media/audio", ".wav");
            self.pkg.put_bytes(&media, sound_wav(name));
            if let Some(ct) = self.pkg.xml_mut("[Content_Types].xml") { if !ct.elements().any(|d| d.attr("Extension") == Some("wav")) { ct.children.insert(0, Node::Element(el("Default", &[("Extension", "wav"), ("ContentType", "audio/wav")]))); } }
            (self.pkg.add_rel(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/audio", &media, false), name.clone())
        });
        let Some(root) = self.pkg.xml_mut(slide) else { return };
        root.remove_children("p:transition");
        root.children.retain(|n| !matches!(n, Node::Element(e) if e.name == "mc:AlternateContent" && e.find("p:transition").is_some()));
        let fx = effect_element(&t.effect, &t.option);
        if fx.is_none() && t.on_click && t.after_ms.is_none() && t.sound.is_none() { return; }
        let build = |rich: bool| -> Element {
            let mut tr = Element::new("p:transition");
            let spd = if t.duration_ms <= 500 { "fast" } else if t.duration_ms >= 1000 { "slow" } else { "med" };
            tr.set_attr("spd", spd);
            if rich { tr.set_attr("p14:dur", t.duration_ms.to_string()); }
            if !t.on_click { tr.set_attr("advClick", "0"); }
            if let Some(a) = t.after_ms { tr.set_attr("advTm", a.to_string()); }
            match (&fx, rich) {
                (Some((e, _)), true) => tr.children.push(Node::Element(e.clone())),
                (Some((e, None)), false) => tr.children.push(Node::Element(e.clone())),
                (Some((_, Some(_))), false) => tr.children.push(Node::Element(el("p:fade", &[]))),
                _ => {}
            }
            if let Some((rid, name)) = &sound_rel { tr.children.push(Node::Element(Element::new("p:sndAc").with_child(Element::new("p:stSnd").with_child(el("p:snd", &[("r:embed", rid), ("name", &format!("{name}.wav"))]))))); }
            tr
        };
        let node = match fx.as_ref().and_then(|f| f.1) {
            // Newer effects: PowerPoint 2010+ reads the choice, older readers the fade.
            Some(ns) => {
                let uri = match ns { "p15" => NS_P15, "p159" => NS_P159, _ => NS_P14 };
                let mut choice = el("mc:Choice", &[("Requires", ns)]);
                choice.set_attr(&format!("xmlns:{ns}"), uri);
                if ns != "p14" { choice.set_attr("xmlns:p14", NS_P14); }
                choice.children.push(Node::Element(build(true)));
                Element::new("mc:AlternateContent").with_attr("xmlns:mc", NS_MC).with_child(choice).with_child(Element::new("mc:Fallback").with_child(build(false)))
            }
            None => {
                // Standard effects with a duration also use the p14 attribute.
                let mut choice = el("mc:Choice", &[("Requires", "p14"), ("xmlns:p14", NS_P14)]);
                choice.children.push(Node::Element(build(true)));
                Element::new("mc:AlternateContent").with_attr("xmlns:mc", NS_MC).with_child(choice).with_child(Element::new("mc:Fallback").with_child(build(false)))
            }
        };
        let pos = root.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "p:timing" | "p:extLst"))).unwrap_or(root.children.len());
        root.children.insert(pos, Node::Element(node));
    }

    /// The bytes of a slide's transition sound, if any.
    pub fn transition_sound(&self, slide: &str) -> Option<std::sync::Arc<Vec<u8>>> {
        let t = self.transition_element(slide)?;
        let rid = t.path("p:sndAc/p:stSnd/p:snd")?.attr("r:embed")?;
        let part = self.pkg.rel_target(slide, rid)?;
        let _ = opc::RT_IMAGE;
        self.pkg.bytes(&part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_round_trips() {
        let mut p = Presentation::new_blank(true);
        let l = p.all_layouts()[0].clone();
        let s = p.add_slide(&l, 0);
        for e in EFFECTS {
            let opts: Vec<&str> = if e.options.is_empty() { vec![""] } else { e.options.iter().map(|o| o.0).collect() };
            for o in opts {
                let t = Transition { effect: e.id.into(), option: o.into(), duration_ms: 1234, on_click: false, after_ms: Some(3000), sound: None };
                p.set_slide_transition(&s, &t);
                let back = p.slide_transition(&s);
                if e.id == "none" { assert_eq!(back.effect, "none"); continue; }
                assert_eq!((back.effect.as_str(), back.option.as_str()), (e.id, o), "{} {}", e.id, o);
                assert_eq!(back.duration_ms, 1234);
                assert!(!back.on_click && back.after_ms == Some(3000));
            }
        }
        p.set_slide_transition(&s, &Transition { effect: "fade".into(), option: "smooth".into(), duration_ms: 700, on_click: true, after_ms: None, sound: Some("Chime".into()) });
        assert_eq!(p.slide_transition(&s).sound.as_deref(), Some("Chime"));
        assert!(p.transition_sound(&s).is_some_and(|b| b.starts_with(b"RIFF")));
        let again = Presentation::open(&p.save().unwrap()).unwrap();
        assert_eq!(again.slide_transition(&again.slides()[0]).effect, "fade");
    }
}
