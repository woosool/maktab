//! Formatting commands of the Home and Insert tabs as package edits: shape effects and fills,
//! Quick Styles, WordArt and text effects, change case, character spacing, text direction,
//! columns, paragraph spacing and indents, links and actions, grouping, distributing,
//! sections, header and footer, and date and slide-number fields.

use crate::opc;
use crate::pres::Presentation;
use crate::scene::{Loc, Xfrm};
use crate::xml::{Element, Node};

const SPPR_AFTER_FILL: [&str; 6] = ["a:ln", "a:effectLst", "a:effectDag", "a:scene3d", "a:sp3d", "a:extLst"];
const SPPR_AFTER_EFFECT: [&str; 4] = ["a:scene3d", "a:sp3d", "a:extLst", "a:effectDag"];

fn srgb(hex: &str) -> Element { Element::new("a:srgbClr").with_attr("val", hex) }
fn srgb_alpha(hex: &str, alpha: u32) -> Element { srgb(hex).with_child(Element::new("a:alpha").with_attr("val", alpha.to_string())) }

/// Shape effect presets (the Shape Effects gallery).
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    None,
    /// Outer shadow: direction (degrees), distance and blur (points), color hex, alpha (0–100).
    Shadow { dir: f64, dist: f64, blur: f64, color: String, alpha: u32 },
    InnerShadow { dir: f64, dist: f64, blur: f64 },
    /// Reflection: how far it shows (0–1) and its gap (points).
    Reflection { size: f64, dist: f64 },
    /// Glow: radius (points), color hex.
    Glow { radius: f64, color: String },
    SoftEdge { radius: f64 },
    /// Bevel: the top bevel preset ("circle", "relaxedInset", …) and its size in points.
    Bevel { kind: String, size: f64 },
    /// 3-D rotation camera preset ("isometricTopUp", "perspectiveFront", …).
    Rotation3d { camera: String },
}

/// Which family of effect an edit replaces.
fn effect_tag(e: &Effect) -> &'static [&'static str] {
    match e {
        Effect::None => &["a:outerShdw", "a:innerShdw", "a:prstShdw", "a:reflection", "a:glow", "a:softEdge"],
        Effect::Shadow { .. } => &["a:outerShdw", "a:prstShdw"],
        Effect::InnerShadow { .. } => &["a:innerShdw"],
        Effect::Reflection { .. } => &["a:reflection"],
        Effect::Glow { .. } => &["a:glow"],
        Effect::SoftEdge { .. } => &["a:softEdge"],
        Effect::Bevel { .. } | Effect::Rotation3d { .. } => &[],
    }
}

fn pt(v: f64) -> String { ((v * 12_700.0).round() as i64).to_string() }
fn deg(v: f64) -> String { ((v.rem_euclid(360.0) * 60_000.0).round() as i64).to_string() }

fn effect_element(e: &Effect) -> Option<Element> {
    Some(match e {
        Effect::Shadow { dir, dist, blur, color, alpha } => Element::new("a:outerShdw").with_attr("blurRad", pt(*blur)).with_attr("dist", pt(*dist)).with_attr("dir", deg(*dir)).with_attr("algn", "ctr").with_attr("rotWithShape", "0").with_child(srgb_alpha(color, alpha * 1000)),
        Effect::InnerShadow { dir, dist, blur } => Element::new("a:innerShdw").with_attr("blurRad", pt(*blur)).with_attr("dist", pt(*dist)).with_attr("dir", deg(*dir)).with_child(srgb_alpha("000000", 50_000)),
        Effect::Reflection { size, dist } => Element::new("a:reflection").with_attr("blurRad", "6350").with_attr("stA", "50000").with_attr("endA", "300").with_attr("endPos", ((size * 100_000.0) as i64).to_string()).with_attr("dist", pt(*dist)).with_attr("dir", "5400000").with_attr("sy", "-100000").with_attr("algn", "bl").with_attr("rotWithShape", "0"),
        Effect::Glow { radius, color } => Element::new("a:glow").with_attr("rad", pt(*radius)).with_child(srgb_alpha(color, 60_000)),
        Effect::SoftEdge { radius } => Element::new("a:softEdge").with_attr("rad", pt(*radius)),
        _ => return None,
    })
}

/// Schema order inside a:effectLst.
const EFFECT_ORDER: [&str; 8] = ["a:blur", "a:fillOverlay", "a:glow", "a:innerShdw", "a:outerShdw", "a:prstShdw", "a:reflection", "a:softEdge"];

fn put_effect(lst: &mut Element, el: Element) {
    let rank = |n: &str| EFFECT_ORDER.iter().position(|x| *x == n).unwrap_or(9);
    let r = rank(&el.name);
    let pos = lst.children.iter().position(|n| matches!(n, Node::Element(e) if rank(&e.name) > r)).unwrap_or(lst.children.len());
    lst.children.insert(pos, Node::Element(el));
}

/// Apply an effect to an spPr (shape) or an rPr (text, for shadow/glow/reflection).
fn apply_effect(pr: &mut Element, e: &Effect, text: bool) {
    match e {
        Effect::Bevel { kind, size } => {
            let sp3d = pr.ensure_at("a:sp3d", &["a:extLst"]);
            sp3d.remove_children("a:bevelT");
            if kind != "none" { sp3d.children.insert(0, Node::Element(Element::new("a:bevelT").with_attr("w", pt(*size)).with_attr("h", pt(*size)).with_attr("prst", kind.clone()))); }
            return;
        }
        Effect::Rotation3d { camera } => {
            pr.remove_children("a:scene3d");
            if camera != "none" {
                let s = Element::new("a:scene3d").with_child(Element::new("a:camera").with_attr("prst", camera.clone())).with_child(Element::new("a:lightRig").with_attr("rig", "threePt").with_attr("dir", "t"));
                let pos = pr.children.iter().position(|n| matches!(n, Node::Element(x) if matches!(x.name.as_str(), "a:sp3d" | "a:extLst"))).unwrap_or(pr.children.len());
                pr.children.insert(pos, Node::Element(s));
            }
            return;
        }
        _ => {}
    }
    let before: &[&str] = if text { &["a:highlight", "a:uLnTx", "a:uLn", "a:uFillTx", "a:uFill", "a:latin", "a:ea", "a:cs", "a:sym", "a:hlinkClick", "a:hlinkMouseOver", "a:rtl", "a:extLst"] } else { &SPPR_AFTER_EFFECT };
    let lst = pr.ensure_at("a:effectLst", before);
    for t in effect_tag(e) { lst.remove_children(t); }
    if let Some(el) = effect_element(e) { put_effect(lst, el); }
}

/// Preset gradients from a base color: light and dark variations (Shape Fill ▸ Gradient).
pub fn gradient_variation(hex: &str, variation: usize) -> (Vec<(f64, String)>, f64, bool) {
    let c = crate::Rgba::hex(hex).unwrap_or(crate::Rgba::rgb(0x44, 0x72, 0xC4));
    let mix = |t: f64| { let m = |v: u8| ((v as f64) + (255.0 - v as f64) * t).round().clamp(0.0, 255.0) as u8; crate::Rgba::rgb(m(c.r), m(c.g), m(c.b)).to_hex() };
    let dark = |t: f64| { let m = |v: u8| (v as f64 * (1.0 - t)).round() as u8; crate::Rgba::rgb(m(c.r), m(c.g), m(c.b)).to_hex() };
    // 0–3: light variations (linear down, diagonal, from center, linear right); 4–7: dark.
    let light = vec![(0.0, mix(0.65)), (1.0, c.to_hex())];
    let darkv = vec![(0.0, c.to_hex()), (1.0, dark(0.45))];
    match variation {
        0 => (light, 90.0, false), 1 => (light, 45.0, false), 2 => (light, 0.0, true), 3 => (light, 0.0, false),
        4 => (darkv, 90.0, false), 5 => (darkv, 45.0, false), 6 => (darkv, 0.0, true), _ => (darkv, 0.0, false),
    }
}

/// Built-in textures (Shape Fill ▸ Texture), generated as small tiles.
pub const TEXTURES: &[&str] = &["Canvas", "Denim", "Woven Mat", "Water Droplets", "Paper Bag", "Recycled Paper", "Granite", "Wood", "Cork", "Stationery", "Sand", "Marble"];

/// A texture tile (PNG) by name.
pub fn texture_png(name: &str) -> Vec<u8> {
    let (base, alt, kind): ((u8, u8, u8), (u8, u8, u8), u8) = match name {
        "Canvas" => ((222, 214, 196), (205, 196, 176), 0), "Denim" => ((70, 96, 140), (54, 78, 118), 1), "Woven Mat" => ((214, 196, 150), (182, 160, 112), 2),
        "Water Droplets" => ((150, 190, 220), (200, 225, 240), 3), "Paper Bag" => ((196, 160, 112), (184, 148, 100), 0), "Recycled Paper" => ((228, 222, 206), (210, 202, 186), 3),
        "Granite" => ((150, 150, 150), (90, 90, 90), 3), "Wood" => ((176, 124, 74), (150, 100, 56), 4), "Cork" => ((196, 150, 96), (160, 114, 68), 3),
        "Stationery" => ((246, 244, 236), (210, 220, 240), 5), "Sand" => ((226, 206, 160), (206, 184, 136), 3), _ => ((236, 236, 236), (200, 200, 205), 4),
    };
    let n = 64u32;
    let mut img = image::RgbaImage::new(n, n);
    let mut seed: u32 = name.bytes().fold(7, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; (seed % 1000) as f32 / 1000.0 };
    for y in 0..n {
        for x in 0..n {
            let t = match kind {
                0 => (if (x + y) % 4 < 2 { 0.3 } else { 0.0 }) + rnd() * 0.2,
                1 => (if (x + 2 * y) % 5 < 2 { 0.7 } else { 0.1 }) + rnd() * 0.15,
                2 => if (x / 8 + y / 8) % 2 == 0 { if y % 8 < 4 { 0.8 } else { 0.2 } } else if x % 8 < 4 { 0.8 } else { 0.2 },
                3 => rnd().powf(2.5),
                4 => ((y as f32 / 3.0 + (x as f32 / 9.0).sin() * 2.0).sin() * 0.5 + 0.5) * 0.8 + rnd() * 0.1,
                _ => if y % 16 == 0 { 1.0 } else { 0.0 },
            }.clamp(0.0, 1.0);
            let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
            img.put_pixel(x, y, image::Rgba([m(base.0, alt.0), m(base.1, alt.1), m(base.2, alt.2), 255]));
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

/// WordArt styles: (name, fill hex, outline hex, shadow, gradient, glow).
pub const WORDART: &[(&str, &str, Option<&str>, bool, bool, bool)] = &[
    ("Fill: Black, Text color 1; Shadow", "000000", None, true, false, false),
    ("Fill: Blue, Accent color 1; Shadow", "4472C4", None, true, false, false),
    ("Fill: Orange, Accent color 2; Outline: Orange", "ED7D31", Some("C55A11"), false, false, false),
    ("Fill: White; Outline: Blue, Accent color 1; Glow", "FFFFFF", Some("4472C4"), false, false, true),
    ("Fill: Gold, Accent color 4; Soft Bevel", "FFC000", None, true, false, false),
    ("Gradient Fill: Blue, Accent color 5; Reflection", "5B9BD5", None, false, true, false),
    ("Fill: Gray, Background color 2; Inner Shadow", "A5A5A5", None, true, false, false),
    ("Fill: White; Outline: Black; Shadow", "FFFFFF", Some("000000"), true, false, false),
    ("Gradient Fill: Gold; Outline", "FFC000", Some("BF9000"), false, true, false),
    ("Fill: Green, Accent color 6; Glow", "70AD47", None, false, false, true),
];

impl Presentation {
    fn sp_pr_mut(&mut self, loc: &Loc) -> Option<&mut Element> {
        let sp = self.element_mut(loc)?;
        Some(match sp.local() { "grpSp" => sp.ensure_at("p:grpSpPr", &["p:style"]), "graphicFrame" => return None, _ => sp.ensure_at("p:spPr", &["p:style", "p:txBody"]) })
    }

    /// Apply (or clear) a shape effect.
    pub fn set_effect(&mut self, loc: &Loc, e: &Effect) {
        let Some(pr) = self.sp_pr_mut(loc) else { return };
        apply_effect(pr, e, false);
        if *e == Effect::None { pr.remove_children("a:effectLst"); pr.remove_children("a:scene3d"); pr.remove_children("a:sp3d"); pr.children.push(Node::Element(Element::new("a:effectLst"))); let order = pr.children.len() - 1; let el = pr.children.remove(order); let pos = pr.children.iter().position(|n| matches!(n, Node::Element(x) if SPPR_AFTER_EFFECT.contains(&x.name.as_str()))).unwrap_or(pr.children.len()); pr.children.insert(pos, el); }
    }

    /// A gradient fill: stops (0–1, hex), angle in degrees (or radial).
    pub fn set_gradient_fill(&mut self, loc: &Loc, stops: &[(f64, String)], angle: f64, radial: bool) {
        let Some(pr) = self.sp_pr_mut(loc) else { return };
        for n in ["a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill"] { pr.remove_children(n); }
        let mut lst = Element::new("a:gsLst");
        for (p, c) in stops { lst.children.push(Node::Element(Element::new("a:gs").with_attr("pos", ((p * 100_000.0) as i64).to_string()).with_child(srgb(c)))); }
        let mut g = Element::new("a:gradFill").with_attr("rotWithShape", "1").with_child(lst);
        if radial { g.children.push(Node::Element(Element::new("a:path").with_attr("path", "circle").with_child(Element::new("a:fillToRect").with_attr("l", "50000").with_attr("t", "50000").with_attr("r", "50000").with_attr("b", "50000")))); }
        else { g.children.push(Node::Element(Element::new("a:lin").with_attr("ang", deg(angle)).with_attr("scaled", "1"))); }
        let pos = pr.children.iter().position(|n| matches!(n, Node::Element(x) if SPPR_AFTER_FILL.contains(&x.name.as_str()))).unwrap_or(pr.children.len());
        pr.children.insert(pos, Node::Element(g));
    }

    /// A picture (or tiled texture) fill from image bytes.
    pub fn set_picture_fill(&mut self, loc: &Loc, bytes: Vec<u8>, ext: &str, tile: bool) {
        let media = self.pkg.free_name("ppt/media/image", &format!(".{ext}"));
        self.pkg.put_bytes(&media, bytes);
        let rid = self.pkg.add_rel(&loc.part, opc::RT_IMAGE, &media, false);
        let Some(pr) = self.sp_pr_mut(loc) else { return };
        for n in ["a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill"] { pr.remove_children(n); }
        let mut f = Element::new("a:blipFill").with_attr("dpi", "0").with_attr("rotWithShape", "1").with_child(Element::new("a:blip").with_attr("r:embed", rid));
        if tile { f.children.push(Node::Element(Element::new("a:tile").with_attr("tx", "0").with_attr("ty", "0").with_attr("sx", "100000").with_attr("sy", "100000").with_attr("flip", "none").with_attr("algn", "tl"))); }
        else { f.children.push(Node::Element(Element::new("a:stretch").with_child(Element::new("a:fillRect")))); }
        let pos = pr.children.iter().position(|n| matches!(n, Node::Element(x) if SPPR_AFTER_FILL.contains(&x.name.as_str()))).unwrap_or(pr.children.len());
        pr.children.insert(pos, Node::Element(f));
    }

    /// A pattern fill (pattFill preset like "pct20", "dkHorz", "smGrid").
    pub fn set_pattern_fill(&mut self, loc: &Loc, preset: &str, fg: &str, bg: &str) {
        let Some(pr) = self.sp_pr_mut(loc) else { return };
        for n in ["a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill"] { pr.remove_children(n); }
        let f = Element::new("a:pattFill").with_attr("prst", preset).with_child(Element::new("a:fgClr").with_child(srgb(fg))).with_child(Element::new("a:bgClr").with_child(srgb(bg)));
        let pos = pr.children.iter().position(|n| matches!(n, Node::Element(x) if SPPR_AFTER_FILL.contains(&x.name.as_str()))).unwrap_or(pr.children.len());
        pr.children.insert(pos, Node::Element(f));
    }

    /// A Quick Style: theme fill/line/effect references in an accent color, with direct fill,
    /// outline and effects removed so the style shows (style 0–5 = subtle … intense).
    pub fn set_quick_style(&mut self, loc: &Loc, accent: &str, style: usize) {
        let (fill_idx, ln_idx, eff_idx, outline_only, light_text) = match style { 0 => (0, 2, 0, true, false), 1 => (1, 0, 0, false, true), 2 => (1, 2, 0, false, true), 3 => (2, 1, 1, false, true), 4 => (3, 0, 2, false, true), _ => (3, 0, 3, false, true) };
        let Some(sp) = self.element_mut(loc) else { return };
        if sp.local() != "sp" && sp.local() != "cxnSp" { return; }
        if let Some(pr) = sp.child_mut("p:spPr") { for n in ["a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill", "a:ln", "a:effectLst"] { pr.remove_children(n); } }
        let clr = |base: &str| Element::new("a:schemeClr").with_attr("val", base);
        let mut lnref = Element::new("a:lnRef").with_attr("idx", ln_idx.to_string()).with_child(clr(accent));
        if !outline_only { lnref.child_mut("a:schemeClr").unwrap().children.push(Node::Element(Element::new("a:shade").with_attr("val", "50000"))); }
        let style_el = Element::new("p:style").with_child(lnref)
            .with_child(Element::new("a:fillRef").with_attr("idx", fill_idx.to_string()).with_child(clr(accent)))
            .with_child(Element::new("a:effectRef").with_attr("idx", eff_idx.to_string()).with_child(clr(accent)))
            .with_child(Element::new("a:fontRef").with_attr("idx", "minor").with_child(clr(if light_text { "lt1" } else { "tx1" })));
        sp.remove_children("p:style");
        let pos = sp.children.iter().position(|n| matches!(n, Node::Element(x) if x.name == "p:txBody")).unwrap_or(sp.children.len());
        sp.children.insert(pos, Node::Element(style_el));
        // The text color follows the style.
        let tc = if light_text { "lt1" } else { "tx1" };
        if let Some(tx) = sp.child_mut("p:txBody") {
            for p in tx.elements_mut().filter(|e| e.name == "a:p") {
                for r in p.elements_mut() {
                    let rpr = match r.name.as_str() { "a:r" | "a:fld" => r.ensure_at("a:rPr", &["a:t"]), "a:endParaRPr" => r, _ => continue };
                    for n in ["a:solidFill", "a:gradFill", "a:noFill"] { rpr.remove_children(n); }
                    rpr.children.insert(if rpr.child("a:ln").is_some() { 1 } else { 0 }, Node::Element(Element::new("a:solidFill").with_child(clr(tc))));
                }
            }
        }
    }

    /// The shape's adjust values (its own avLst over the preset's defaults).
    pub fn adjusts(&self, loc: &Loc) -> Vec<(String, f64)> {
        let Some(pg) = self.element(loc).and_then(|e| e.path("p:spPr/a:prstGeom")) else { return vec![] };
        let mut v = crate::geom::preset_adjusts(pg.attr("prst").unwrap_or("rect"));
        if let Some(av) = pg.child("a:avLst") {
            for g in av.elements() { if let (Some(n), Some(f)) = (g.attr("name"), g.attr("fmla")) { if let Some(x) = f.strip_prefix("val ").and_then(|x| x.trim().parse().ok()) { if let Some(e) = v.iter_mut().find(|e| e.0 == n) { e.1 = x; } else { v.push((n.to_string(), x)); } } } }
        }
        v
    }
    /// Set a preset shape's adjust value (Edit Points ▸ the yellow handles).
    pub fn set_adjust(&mut self, loc: &Loc, name: &str, value: f64) {
        let Some(av) = self.element_mut(loc).and_then(|e| e.path_mut("p:spPr/a:prstGeom")).map(|p| p.ensure("a:avLst")) else { return };
        let fmla = format!("val {}", value.round() as i64);
        if let Some(g) = av.elements_mut().find(|g| g.attr("name") == Some(name)) { g.set_attr("fmla", fmla); }
        else { av.children.push(Node::Element(Element::new("a:gd").with_attr("name", name).with_attr("fmla", fmla))); }
    }

    // ---- text: effects, case, spacing --------------------------------------------------

    fn each_rpr(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, mut f: impl FnMut(&mut Element)) {
        if let Some((a, b)) = range { if a < b { self.split_runs_at(loc, cell, a, b); } }
        let Some(body) = self.text_body_mut(loc, cell) else { return };
        let mut pos = 0usize;
        for p in body.elements_mut().filter(|e| e.name == "a:p") {
            let mut touched = false;
            for r in p.elements_mut() {
                let len = match r.name.as_str() { "a:r" | "a:fld" => r.child("a:t").map(|t| t.text().chars().count()).unwrap_or(0), "a:br" => 1, _ => 0 };
                let inside = range.is_none_or(|(a, b)| a >= b || (pos >= a && pos + len <= b && len > 0));
                if inside && matches!(r.name.as_str(), "a:r" | "a:fld" | "a:br") { f(r.ensure_at("a:rPr", &["a:t"])); touched = true; }
                pos += len;
            }
            if range.is_none() || !touched && range.is_some_and(|(a, b)| a >= b) {
                if let Some(end) = p.child_mut("a:endParaRPr") { f(end); } else if range.is_none() { let mut e = Element::new("a:endParaRPr"); f(&mut e); p.children.push(Node::Element(e)); }
            }
            pos += 1;
        }
    }

    fn text_body_mut(&mut self, loc: &Loc, cell: Option<(usize, usize)>) -> Option<&mut Element> {
        let sp = self.element_mut(loc)?;
        if let Some((r, c)) = cell {
            let tbl = sp.path_mut("a:graphic/a:graphicData/a:tbl")?;
            let tr = tbl.elements_mut().filter(|e| e.name == "a:tr").nth(r)?;
            let tc = tr.elements_mut().filter(|e| e.name == "a:tc").nth(c)?;
            return tc.child_mut("a:txBody");
        }
        sp.child_mut("p:txBody")
    }

    /// Split runs so [a, b) starts and ends on run boundaries (formatting nothing).
    fn split_runs_at(&mut self, loc: &Loc, cell: Option<(usize, usize)>, a: usize, b: usize) {
        self.format_range(loc, cell, a, b, &crate::edit::RunFormat::default());
    }

    /// Text effects on the selection (or whole text): shadow, glow, reflection, outline, gradient.
    pub fn set_text_effect(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, e: &Effect) {
        self.each_rpr(loc, cell, range, |rpr| apply_effect(rpr, e, true));
    }
    /// Text outline (hex, None to remove) and width in points.
    pub fn set_text_outline(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, color: Option<&str>, width: f64) {
        self.each_rpr(loc, cell, range, |rpr| {
            rpr.remove_children("a:ln");
            if let Some(c) = color { rpr.children.insert(0, Node::Element(Element::new("a:ln").with_attr("w", pt(width)).with_child(Element::new("a:solidFill").with_child(srgb(c))))); }
        });
    }
    /// Gradient text fill (two hex colors, top to bottom), or a solid color when `to` is None.
    pub fn set_text_fill(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, from: &str, to: Option<&str>) {
        self.each_rpr(loc, cell, range, |rpr| {
            for n in ["a:noFill", "a:solidFill", "a:gradFill"] { rpr.remove_children(n); }
            let fill = match to {
                Some(t) => Element::new("a:gradFill").with_child(Element::new("a:gsLst").with_child(Element::new("a:gs").with_attr("pos", "0").with_child(srgb(from))).with_child(Element::new("a:gs").with_attr("pos", "100000").with_child(srgb(t)))).with_child(Element::new("a:lin").with_attr("ang", "5400000").with_attr("scaled", "1")),
                None => Element::new("a:solidFill").with_child(srgb(from)),
            };
            let pos = if rpr.child("a:ln").is_some() { 1 } else { 0 };
            rpr.children.insert(pos.min(rpr.children.len()), Node::Element(fill));
        });
    }
    /// Apply a WordArt style to the text.
    pub fn set_wordart(&mut self, loc: &Loc, style: usize) {
        let Some(&(_, fill, outline, shadow, gradient, glow)) = WORDART.get(style) else { return };
        let dark = darken(fill);
        self.set_text_fill(loc, None, None, fill, if gradient { Some(&dark) } else { None });
        self.set_text_outline(loc, None, None, outline, 1.0);
        self.set_text_effect(loc, None, None, &if shadow { Effect::Shadow { dir: 45.0, dist: 3.0, blur: 4.0, color: "000000".into(), alpha: 40 } } else { Effect::None });
        if glow { self.set_text_effect(loc, None, None, &Effect::Glow { radius: 8.0, color: "FFC000".into() }); }
    }

    /// Underline style ("sng", "dbl", "heavy", "dotted", "dash", "wavy", "none").
    pub fn set_underline_style(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, style: &str) {
        self.each_rpr(loc, cell, range, |rpr| rpr.set_attr("u", style));
    }
    /// Character spacing in points (0 = normal).
    pub fn set_char_spacing(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, pts: f64) {
        self.each_rpr(loc, cell, range, |rpr| if pts == 0.0 { rpr.remove_attr("spc"); } else { rpr.set_attr("spc", ((pts * 100.0).round() as i64).to_string()); });
    }
    /// Small caps / all caps / normal ("small", "all", "none"), and double strikethrough.
    pub fn set_caps(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, caps: &str) {
        self.each_rpr(loc, cell, range, |rpr| if caps == "none" { rpr.remove_attr("cap"); } else { rpr.set_attr("cap", caps); });
    }
    pub fn set_strike_kind(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, kind: &str) {
        self.each_rpr(loc, cell, range, |rpr| rpr.set_attr("strike", kind));
    }
    /// Highlight (hex) or none.
    pub fn set_highlight(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, color: Option<&str>) {
        self.format_range_or_all(loc, cell, range, &crate::edit::RunFormat { highlight: Some(color.map(String::from)), ..Default::default() });
    }
    fn format_range_or_all(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, f: &crate::edit::RunFormat) {
        match range { Some((a, b)) if a < b => self.format_range(loc, cell, a, b, f), _ => self.format_shape_text(loc, cell, f) }
    }

    /// Change case of the text in a range (or all): "sentence", "lower", "upper", "title", "toggle".
    pub fn change_case(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, mode: &str) {
        let text: Vec<char> = self.shape_text(loc, cell).chars().collect();
        let (a, b) = match range { Some((a, b)) if a < b => (a, b.min(text.len())), _ => (0, text.len()) };
        let mut out: Vec<char> = text[a..b].to_vec();
        let mut start_of_sentence = a == 0 || text[..a].iter().rev().find(|c| !c.is_whitespace()).is_none_or(|c| matches!(c, '.' | '!' | '?' | '\n'));
        let mut start_of_word = a == 0 || text[a - 1].is_whitespace();
        for c in out.iter_mut() {
            let ch = *c;
            *c = match mode {
                "lower" => ch.to_lowercase().next().unwrap_or(ch),
                "upper" => ch.to_uppercase().next().unwrap_or(ch),
                "title" => if start_of_word { ch.to_uppercase().next().unwrap_or(ch) } else { ch.to_lowercase().next().unwrap_or(ch) },
                "toggle" => if ch.is_uppercase() { ch.to_lowercase().next().unwrap_or(ch) } else { ch.to_uppercase().next().unwrap_or(ch) },
                _ => if start_of_sentence && ch.is_alphabetic() { ch.to_uppercase().next().unwrap_or(ch) } else { ch.to_lowercase().next().unwrap_or(ch) },
            };
            if ch.is_alphabetic() { start_of_sentence = false; }
            if matches!(ch, '.' | '!' | '?' | '\n') { start_of_sentence = true; }
            start_of_word = ch.is_whitespace();
        }
        // Rewrite character by character so each run keeps its formatting.
        let Some(body) = self.text_body_mut(loc, cell) else { return };
        let mut pos = 0usize;
        for p in body.elements_mut().filter(|e| e.name == "a:p") {
            for r in p.elements_mut() {
                match r.name.as_str() {
                    "a:r" => {
                        let t: Vec<char> = r.child("a:t").map(|t| t.text()).unwrap_or_default().chars().collect();
                        let new: String = t.iter().enumerate().map(|(i, c)| { let k = pos + i; if k >= a && k < b { out[k - a] } else { *c } }).collect();
                        if let Some(te) = r.child_mut("a:t") { te.set_text(new); }
                        pos += t.len();
                    }
                    "a:fld" => pos += r.child("a:t").map(|t| t.text().chars().count()).unwrap_or(0),
                    "a:br" => pos += 1,
                    _ => {}
                }
            }
            pos += 1;
        }
    }

    // ---- paragraph and text box ------------------------------------------------------

    fn body_pr_mut(&mut self, loc: &Loc) -> Option<&mut Element> {
        let sp = self.element_mut(loc)?;
        let tx = sp.child_mut("p:txBody")?;
        Some(tx.ensure_at("a:bodyPr", &["a:lstStyle", "a:p"]))
    }
    /// Text direction: "horz", "vert" (90°), "vert270", "wordArtVert" (stacked).
    pub fn set_text_direction(&mut self, loc: &Loc, dir: &str) {
        if let Some(b) = self.body_pr_mut(loc) { if dir == "horz" { b.remove_attr("vert"); } else { b.set_attr("vert", dir); } }
    }
    /// Columns: count and spacing (points).
    pub fn set_columns(&mut self, loc: &Loc, count: u32, spacing_pt: f64) {
        if let Some(b) = self.body_pr_mut(loc) {
            if count <= 1 { b.remove_attr("numCol"); b.remove_attr("spcCol"); } else { b.set_attr("numCol", count.to_string()); b.set_attr("spcCol", pt(spacing_pt)); }
        }
    }
    /// Text box insets (points), wrap and autofit ("none", "shrink", "resize").
    pub fn set_text_box(&mut self, loc: &Loc, insets: Option<[f64; 4]>, wrap: Option<bool>, autofit: Option<&str>) {
        let Some(b) = self.body_pr_mut(loc) else { return };
        if let Some([l, t, r, bt]) = insets { for (k, v) in [("lIns", l), ("tIns", t), ("rIns", r), ("bIns", bt)] { b.set_attr(k, pt(v)); } }
        if let Some(w) = wrap { b.set_attr("wrap", if w { "square" } else { "none" }); }
        if let Some(a) = autofit {
            for n in ["a:noAutofit", "a:normAutofit", "a:spAutoFit"] { b.remove_children(n); }
            let el = match a { "shrink" => "a:normAutofit", "resize" => "a:spAutoFit", _ => "a:noAutofit" };
            let pos = b.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "a:scene3d" | "a:sp3d" | "a:flatTx" | "a:extLst"))).unwrap_or(b.children.len());
            b.children.insert(pos, Node::Element(Element::new(el)));
        }
    }

    /// Paragraph spacing and indents (Paragraph dialog). Values in points; None leaves them.
    #[allow(clippy::too_many_arguments)]
    pub fn set_paragraph(&mut self, loc: &Loc, cell: Option<(usize, usize)>, paras: Option<(usize, usize)>, before: Option<f64>, after: Option<f64>, margin: Option<f64>, indent: Option<f64>, rtl: Option<bool>) {
        let Some(body) = self.text_body_mut(loc, cell) else { return };
        for (i, p) in body.elements_mut().filter(|e| e.name == "a:p").enumerate() {
            if let Some((a, b)) = paras { if i < a || i > b { continue; } }
            if p.child("a:pPr").is_none() { p.children.insert(0, Node::Element(Element::new("a:pPr"))); }
            let ppr = p.child_mut("a:pPr").unwrap();
            let sp = |name: &str, v: f64| Element::new(name).with_child(Element::new("a:spcPts").with_attr("val", ((v * 100.0).round() as i64).to_string()));
            if let Some(v) = before { ppr.remove_children("a:spcBef"); let pos = ppr.children.iter().position(|n| matches!(n, Node::Element(e) if e.name != "a:lnSpc")).unwrap_or(ppr.children.len()); ppr.children.insert(pos, Node::Element(sp("a:spcBef", v))); }
            if let Some(v) = after { ppr.remove_children("a:spcAft"); let pos = ppr.children.iter().position(|n| matches!(n, Node::Element(e) if !matches!(e.name.as_str(), "a:lnSpc" | "a:spcBef"))).unwrap_or(ppr.children.len()); ppr.children.insert(pos, Node::Element(sp("a:spcAft", v))); }
            if let Some(v) = margin { ppr.set_attr("marL", pt(v)); }
            if let Some(v) = indent { ppr.set_attr("indent", pt(v)); }
            if let Some(r) = rtl { if r { ppr.set_attr("rtl", "1"); } else { ppr.remove_attr("rtl"); } }
        }
    }

    /// Bullet size (percent of text) and color (hex) for the paragraphs.
    pub fn set_bullet_look(&mut self, loc: &Loc, cell: Option<(usize, usize)>, paras: Option<(usize, usize)>, size_pct: Option<f64>, color: Option<&str>) {
        let Some(body) = self.text_body_mut(loc, cell) else { return };
        for (i, p) in body.elements_mut().filter(|e| e.name == "a:p").enumerate() {
            if let Some((a, b)) = paras { if i < a || i > b { continue; } }
            if p.child("a:pPr").is_none() { p.children.insert(0, Node::Element(Element::new("a:pPr"))); }
            let ppr = p.child_mut("a:pPr").unwrap();
            let before = ["a:buFontTx", "a:buFont", "a:buNone", "a:buAutoNum", "a:buChar", "a:buBlip", "a:tabLst", "a:defRPr", "a:extLst"];
            if let Some(c) = color { ppr.remove_children("a:buClr"); let pos = ppr.children.iter().position(|n| matches!(n, Node::Element(e) if before.contains(&e.name.as_str()) || e.name == "a:buSzPct")).unwrap_or(ppr.children.len()); ppr.children.insert(pos, Node::Element(Element::new("a:buClr").with_child(srgb(c)))); }
            if let Some(s) = size_pct { ppr.remove_children("a:buSzPct"); let pos = ppr.children.iter().position(|n| matches!(n, Node::Element(e) if before.contains(&e.name.as_str()))).unwrap_or(ppr.children.len()); ppr.children.insert(pos, Node::Element(Element::new("a:buSzPct").with_attr("val", ((s * 1000.0) as i64).to_string()))); }
        }
    }

    /// Replace one font with another everywhere (Replace Fonts); returns runs changed.
    pub fn replace_font(&mut self, from: &str, to: &str) -> usize {
        let mut n = 0;
        let parts: Vec<String> = self.slides().into_iter().chain(self.all_layouts()).chain(self.masters()).collect();
        for p in parts {
            let Some(root) = self.pkg.xml_mut(&p) else { continue };
            fn walk(e: &mut Element, from: &str, to: &str, n: &mut usize) {
                if matches!(e.name.as_str(), "a:latin" | "a:ea" | "a:cs") && e.attr("typeface").is_some_and(|t| t.eq_ignore_ascii_case(from)) { e.set_attr("typeface", to); *n += 1; }
                for c in e.elements_mut() { walk(c, from, to, n); }
            }
            walk(root, from, to, &mut n);
        }
        // Theme fonts too.
        for m in self.masters() {
            if let Some(t) = self.theme_part(&m) { if let Some(root) = self.pkg.xml_mut(&t) { fn walk(e: &mut Element, from: &str, to: &str, n: &mut usize) { if e.name == "a:latin" && e.attr("typeface").is_some_and(|t| t.eq_ignore_ascii_case(from)) { e.set_attr("typeface", to); *n += 1; } for c in e.elements_mut() { walk(c, from, to, n); } } walk(root, from, to, &mut n); } }
        }
        n
    }

    /// Every font the presentation uses (for Replace Fonts), resolved through the theme.
    pub fn fonts_used(&self) -> Vec<String> {
        let mut out = std::collections::BTreeSet::new();
        for p in self.slides() {
            let scene = self.scene(&p);
            for it in &scene.items { if let crate::scene::Kind::Shape { text: Some(t), .. } = &it.kind { for para in &t.paras { for r in &para.runs { if !r.font.is_empty() { out.insert(r.font.clone()); } } } } }
        }
        out.into_iter().collect()
    }

    // ---- links and actions -------------------------------------------------------------

    /// A link on a shape: a web address, or a slide ("#3" = slide 3), or an action
    /// ("next", "previous", "first", "last", "end"); None removes it.
    pub fn set_shape_link(&mut self, loc: &Loc, target: Option<&str>) {
        let link = target.map(|t| self.link_element(&loc.part, t));
        let Some(sp) = self.element_mut(loc) else { return };
        let Some(c) = sp.elements_mut().find(|e| e.local().starts_with("nv")).and_then(|nv| nv.child_mut("p:cNvPr")) else { return };
        c.remove_children("a:hlinkClick");
        if let Some(l) = link { c.children.insert(0, Node::Element(l)); }
    }
    /// A link on text (the range, or all of it).
    pub fn set_text_link(&mut self, loc: &Loc, cell: Option<(usize, usize)>, range: Option<(usize, usize)>, target: Option<&str>) {
        let link = target.map(|t| self.link_element(&loc.part, t));
        self.each_rpr(loc, cell, range, |rpr| {
            rpr.remove_children("a:hlinkClick");
            if let Some(l) = &link {
                let pos = rpr.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "a:hlinkMouseOver" | "a:rtl" | "a:extLst"))).unwrap_or(rpr.children.len());
                rpr.children.insert(pos, Node::Element(l.clone()));
            }
        });
    }
    fn link_element(&mut self, part: &str, target: &str) -> Element {
        let action = |a: &str| Element::new("a:hlinkClick").with_attr("r:id", "").with_attr("action", format!("ppaction://hlinkshowjump?jump={a}"));
        match target {
            "next" => action("nextslide"), "previous" => action("previousslide"), "first" => action("firstslide"), "last" => action("lastslide"), "end" => action("endshow"),
            t if t.starts_with('#') => {
                let n: usize = t[1..].parse().unwrap_or(1);
                let slide = self.slides().get(n.saturating_sub(1)).cloned().unwrap_or_default();
                let rid = self.pkg.add_rel(part, opc::RT_SLIDE, &slide, false);
                Element::new("a:hlinkClick").with_attr("r:id", rid).with_attr("action", "ppaction://hlinksldjump")
            }
            url => {
                let url = if url.contains("://") || url.starts_with("mailto:") || url.starts_with("tel:") { url.to_string() } else if url.contains('@') { format!("mailto:{url}") } else { format!("https://{url}") };
                let rid = self.pkg.add_rel(part, opc::RT_HYPERLINK, &url, true);
                Element::new("a:hlinkClick").with_attr("r:id", rid)
            }
        }
    }
    /// Where a link element points: ("url", address), ("slide", index) or ("action", name).
    pub fn link_target(&self, part: &str, link: &Element) -> Option<(String, String)> {
        let action = link.attr("action").unwrap_or("");
        if let Some(j) = action.strip_prefix("ppaction://hlinkshowjump?jump=") { return Some(("action".into(), j.into())); }
        let rid = link.attr("r:id").filter(|r| !r.is_empty())?;
        if action == "ppaction://hlinksldjump" {
            let target = self.pkg.rel_target(part, rid)?;
            return self.slides().iter().position(|s| *s == target).map(|i| ("slide".into(), i.to_string()));
        }
        self.pkg.rel_external(part, rid).map(|u| ("url".into(), u))
    }
    /// The click link of a shape, if any.
    pub fn shape_link(&self, loc: &Loc) -> Option<(String, String)> {
        let sp = self.element(loc)?;
        let l = sp.elements().find(|e| e.local().starts_with("nv")).and_then(|nv| nv.child("p:cNvPr")).and_then(|c| c.child("a:hlinkClick"))?;
        self.link_target(&loc.part, l)
    }

    // ---- grouping and distributing -------------------------------------------------------

    /// Group top-level shapes (on one part) into a new group; returns its location.
    pub fn group(&mut self, locs: &[Loc], frames: &[Xfrm]) -> Option<Loc> {
        if locs.len() < 2 { return None; }
        let part = locs[0].part.clone();
        let x0 = frames.iter().map(|f| f.x).fold(f64::MAX, f64::min);
        let y0 = frames.iter().map(|f| f.y).fold(f64::MAX, f64::min);
        let x1 = frames.iter().map(|f| f.x + f.w).fold(f64::MIN, f64::max);
        let y1 = frames.iter().map(|f| f.y + f.h).fold(f64::MIN, f64::max);
        // Members keep their frames (written explicitly, as group children need them).
        for (l, f) in locs.iter().zip(frames) { self.set_xfrm(l, f); }
        let mut order: Vec<(usize, Element)> = locs.iter().filter_map(|l| Some((l.path[0], self.element(l)?.clone()))).collect();
        order.sort_by_key(|o| o.0);
        let id = self.next_shape_id(&part);
        let off = |n: &str, x: f64, y: f64| Element::new(n).with_attr("x", (x.round() as i64).to_string()).with_attr("y", (y.round() as i64).to_string());
        let ext = |n: &str, w: f64, h: f64| Element::new(n).with_attr("cx", (w.round() as i64).to_string()).with_attr("cy", (h.round() as i64).to_string());
        let mut grp = Element::new("p:grpSp")
            .with_child(Element::new("p:nvGrpSpPr").with_child(Element::new("p:cNvPr").with_attr("id", id.to_string()).with_attr("name", format!("Group {}", id - 1))).with_child(Element::new("p:cNvGrpSpPr")).with_child(Element::new("p:nvPr")))
            .with_child(Element::new("p:grpSpPr").with_child(Element::new("a:xfrm").with_child(off("a:off", x0, y0)).with_child(ext("a:ext", x1 - x0, y1 - y0)).with_child(off("a:chOff", x0, y0)).with_child(ext("a:chExt", x1 - x0, y1 - y0))));
        for (_, e) in &order { grp.children.push(Node::Element(e.clone())); }
        // The group takes the place of the topmost member.
        let top = order.last()?.0;
        let tree = self.pkg.xml_mut(&part)?.path_mut("p:cSld/p:spTree")?;
        let positions: Vec<usize> = tree.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(_))).map(|(i, _)| i).collect();
        let raw_top = *positions.get(top)?;
        tree.children[raw_top] = Node::Element(grp);
        let mut remove: Vec<usize> = order.iter().filter(|o| o.0 != top).filter_map(|o| positions.get(o.0).copied()).collect();
        remove.sort_unstable();
        for r in remove.iter().rev() { tree.children.remove(*r); }
        let idx = tree.children[..tree.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "p:grpSp" && e.path("p:nvGrpSpPr/p:cNvPr").and_then(|c| c.attr_i64("id")) == Some(id as i64))).unwrap_or(0) + 1].iter().filter(|n| matches!(n, Node::Element(_))).count() - 1;
        Some(Loc { part, path: vec![idx] })
    }

    /// Ungroup: the group's members back on the slide, with their frames mapped out of the
    /// group's child space; returns their locations.
    pub fn ungroup(&mut self, loc: &Loc) -> Vec<Loc> {
        let Some(grp) = self.element(loc).cloned() else { return vec![] };
        if grp.local() != "grpSp" { return vec![]; }
        let x = grp.path("p:grpSpPr/a:xfrm").map(crate::scene::Xfrm::read).unwrap_or_default();
        let xf = grp.path("p:grpSpPr/a:xfrm");
        let ch_off = xf.and_then(|x| x.child("a:chOff"));
        let ch_ext = xf.and_then(|x| x.child("a:chExt"));
        let (cx, cy) = (ch_off.and_then(|o| o.attr_f64("x")).unwrap_or(0.0), ch_off.and_then(|o| o.attr_f64("y")).unwrap_or(0.0));
        let (cw, chh) = (ch_ext.and_then(|o| o.attr_f64("cx")).unwrap_or(x.w), ch_ext.and_then(|o| o.attr_f64("cy")).unwrap_or(x.h));
        let (sx, sy) = (if cw > 0.0 { x.w / cw } else { 1.0 }, if chh > 0.0 { x.h / chh } else { 1.0 });
        let members: Vec<Element> = grp.elements().filter(|e| matches!(e.local(), "sp" | "pic" | "grpSp" | "graphicFrame" | "cxnSp")).cloned().collect();
        let i = loc.path[0];
        let Some(tree) = self.pkg.xml_mut(&loc.part).and_then(|r| r.path_mut("p:cSld/p:spTree")) else { return vec![] };
        let positions: Vec<usize> = tree.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(_))).map(|(k, _)| k).collect();
        let Some(&raw) = positions.get(i) else { return vec![] };
        tree.children.remove(raw);
        for (k, m) in members.iter().enumerate() { tree.children.insert(raw + k, Node::Element(m.clone())); }
        let locs: Vec<Loc> = (0..members.len()).map(|k| Loc { part: loc.part.clone(), path: vec![i + k] }).collect();
        // Map each member's frame from child space onto the slide (group rotation added).
        for l in &locs {
            if let Some(mut f) = self.xfrm(l) {
                let nx = x.x + (f.x - cx) * sx;
                let ny = x.y + (f.y - cy) * sy;
                f.w *= sx; f.h *= sy; f.x = nx; f.y = ny;
                if x.rot != 0.0 {
                    let (gcx, gcy) = (x.x + x.w / 2.0, x.y + x.h / 2.0);
                    let (mcx, mcy) = (f.x + f.w / 2.0, f.y + f.h / 2.0);
                    let (s, c) = x.rot.to_radians().sin_cos();
                    let (dx, dy) = (mcx - gcx, mcy - gcy);
                    let (rx, ry) = (gcx + dx * c - dy * s, gcy + dx * s + dy * c);
                    f.x = rx - f.w / 2.0; f.y = ry - f.h / 2.0; f.rot = (f.rot + x.rot).rem_euclid(360.0);
                }
                self.set_xfrm(l, &f);
            }
        }
        locs
    }

    // ---- sections ------------------------------------------------------------------------

    /// Sections: (name, first slide index) in order (p14:sectionLst).
    pub fn sections(&self) -> Vec<(String, usize)> {
        let Some(lst) = self.root().find("p14:sectionLst") else { return vec![] };
        let ids: Vec<i64> = self.root().child("p:sldIdLst").map(|l| l.elements().filter_map(|e| e.attr_i64("id")).collect()).unwrap_or_default();
        lst.elements().filter_map(|s| {
            let first = s.child("p14:sldIdLst").and_then(|l| l.first_element()).and_then(|e| e.attr_i64("id"));
            let idx = first.and_then(|f| ids.iter().position(|i| *i == f)).unwrap_or(ids.len());
            Some((s.attr("name")?.to_string(), idx))
        }).collect()
    }
    /// Replace the sections (name, first slide index), recomputing their slide lists.
    pub fn set_sections(&mut self, mut sections: Vec<(String, usize)>) {
        sections.sort_by_key(|s| s.1);
        let ids: Vec<String> = self.root().child("p:sldIdLst").map(|l| l.elements().filter_map(|e| e.attr("id").map(String::from)).collect()).unwrap_or_default();
        let root = self.root_mut();
        // Remove an existing section list (inside p:extLst/p:ext).
        if let Some(ext) = root.child_mut("p:extLst") { ext.children.retain(|n| !matches!(n, Node::Element(e) if e.find("p14:sectionLst").is_some())); }
        if sections.is_empty() { return; }
        let mut lst = Element::new("p14:sectionLst").with_attr("xmlns:p14", "http://schemas.microsoft.com/office/powerpoint/2010/main");
        for (k, (name, first)) in sections.iter().enumerate() {
            let end = sections.get(k + 1).map(|s| s.1).unwrap_or(ids.len());
            let mut sl = Element::new("p14:sldIdLst");
            for id in ids.iter().take(end).skip(*first) { sl.children.push(Node::Element(Element::new("p14:sldId").with_attr("id", id.clone()))); }
            let guid = format!("{{{:08X}-{:04X}-4{:03X}-8{:03X}-{:012X}}}", 0x1000_0000u32 + k as u32, k, k, k, k as u64 + 1);
            lst.children.push(Node::Element(Element::new("p14:section").with_attr("name", name.clone()).with_attr("id", guid).with_child(sl)));
        }
        let ext = Element::new("p:ext").with_attr("uri", "{521415D9-36F7-43E2-AB2F-B90AF26B5E84}").with_child(lst);
        let ext_lst = root.ensure_at("p:extLst", &[]);
        ext_lst.children.insert(0, Node::Element(ext));
    }

    // ---- header and footer, fields --------------------------------------------------------

    /// Header & Footer: date (None = off; Some("") = updates automatically), slide number,
    /// footer text; on every slide (`all`) or the current one; skipping title slides.
    pub fn set_header_footer(&mut self, slides: &[String], date: Option<&str>, number: bool, footer: Option<&str>, skip_title: bool) {
        for s in slides {
            let is_title = self.layout_of(s).is_some_and(|l| self.layout_type(&l) == "title");
            for (kind, on, text) in [("dt", date.is_some(), date.unwrap_or("")), ("sldNum", number, ""), ("ftr", footer.is_some(), footer.unwrap_or(""))] {
                let want = on && !(skip_title && is_title);
                self.set_footer_placeholder(s, kind, want, text);
            }
        }
    }

    fn set_footer_placeholder(&mut self, slide: &str, kind: &str, on: bool, text: &str) {
        // Remove any existing one.
        if let Some(tree) = self.pkg.xml_mut(slide).and_then(|r| r.path_mut("p:cSld/p:spTree")) {
            tree.children.retain(|n| !matches!(n, Node::Element(e) if e.path("p:nvSpPr/p:nvPr/p:ph").and_then(|p| p.attr("type")) == Some(kind)));
        }
        if !on { return; }
        let Some(layout) = self.layout_of(slide) else { return };
        let idx = self.pkg.xml(&layout).and_then(|l| l.path("p:cSld/p:spTree")).and_then(|t| t.elements().find(|e| e.path("p:nvSpPr/p:nvPr/p:ph").and_then(|p| p.attr("type")) == Some(kind)).and_then(|e| e.path("p:nvSpPr/p:nvPr/p:ph")).and_then(|p| p.attr("idx").map(String::from)));
        let id = self.next_shape_id(slide);
        let mut ph = Element::new("p:ph").with_attr("type", kind);
        if let Some(i) = idx { ph.set_attr("sz", "quarter"); ph.set_attr("idx", i); }
        let name = match kind { "dt" => "Date Placeholder", "sldNum" => "Slide Number Placeholder", _ => "Footer Placeholder" };
        let para = match kind {
            "sldNum" => Element::new("a:p").with_child(Element::new("a:fld").with_attr("id", "{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}").with_attr("type", "slidenum").with_child(Element::new("a:rPr").with_attr("lang", "en-US")).with_child(Element::new("a:t").with_text("‹#›"))),
            "dt" if text.is_empty() => Element::new("a:p").with_child(Element::new("a:fld").with_attr("id", "{2E4E1F6B-1F29-4B6A-9C6B-1A1B3E3D1F11}").with_attr("type", "datetime1").with_child(Element::new("a:rPr").with_attr("lang", "en-US")).with_child(Element::new("a:t").with_text(today()))),
            _ => Element::new("a:p").with_child(Element::new("a:r").with_child(Element::new("a:rPr").with_attr("lang", "en-US")).with_child(Element::new("a:t").with_text(text))),
        };
        let sp = Element::new("p:sp")
            .with_child(Element::new("p:nvSpPr").with_child(Element::new("p:cNvPr").with_attr("id", id.to_string()).with_attr("name", format!("{name} {}", id - 1))).with_child(Element::new("p:cNvSpPr").with_child(Element::new("a:spLocks").with_attr("noGrp", "1"))).with_child(Element::new("p:nvPr").with_child(ph)))
            .with_child(Element::new("p:spPr"))
            .with_child(Element::new("p:txBody").with_child(Element::new("a:bodyPr")).with_child(Element::new("a:lstStyle")).with_child(para));
        self.insert_element(slide, sp);
    }

    /// Insert a field (date "datetime1"… or "slidenum") at a character position.
    pub fn insert_field(&mut self, loc: &Loc, cell: Option<(usize, usize)>, pos: usize, kind: &str, number: usize) {
        let text = if kind == "slidenum" { number.to_string() } else { today_as(kind) };
        // Inserted as text first (so it's positioned and formatted), then turned into a field.
        self.insert_text(loc, cell, pos, "\u{E0F0}");
        let Some(body) = self.text_body_mut(loc, cell) else { return };
        for p in body.elements_mut().filter(|e| e.name == "a:p") {
            let Some(k) = p.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "a:r" && e.child("a:t").is_some_and(|t| t.text().contains('\u{E0F0}')))) else { continue };
            let Node::Element(r) = p.children.remove(k) else { continue };
            let t = r.child("a:t").map(|t| t.text()).unwrap_or_default();
            let (before, after) = t.split_once('\u{E0F0}').unwrap_or((&t, ""));
            let rpr = r.child("a:rPr").cloned();
            let mk_run = |s: &str| { let mut e = Element::new("a:r"); if let Some(p) = &rpr { e.children.push(Node::Element(p.clone())); } e.children.push(Node::Element(Element::new("a:t").with_text(s))); e };
            let mut fld = Element::new("a:fld").with_attr("id", format!("{{{:08X}-0000-4000-8000-000000000001}}", pos)).with_attr("type", kind);
            if let Some(p) = &rpr { fld.children.push(Node::Element(p.clone())); }
            fld.children.push(Node::Element(Element::new("a:t").with_text(text.clone())));
            let mut new = vec![];
            if !before.is_empty() { new.push(Node::Element(mk_run(before))); }
            new.push(Node::Element(fld));
            if !after.is_empty() { new.push(Node::Element(mk_run(after))); }
            for (j, n) in new.into_iter().enumerate() { p.children.insert(k + j, n); }
            break;
        }
    }

    /// Distribute shapes evenly between the outermost ones (horizontal or vertical).
    pub fn distribute(&mut self, items: &[(Loc, Xfrm)], horizontal: bool, slide_w: f64, slide_h: f64) {
        let mut v: Vec<(Loc, Xfrm)> = items.to_vec();
        if v.len() < 2 { return; }
        v.sort_by(|a, b| if horizontal { a.1.x.total_cmp(&b.1.x) } else { a.1.y.total_cmp(&b.1.y) });
        let (start, end) = if v.len() == 2 || items.len() < 3 { (0.0, if horizontal { slide_w } else { slide_h }) } else {
            (if horizontal { v[0].1.x } else { v[0].1.y }, if horizontal { v.last().unwrap().1.x + v.last().unwrap().1.w } else { v.last().unwrap().1.y + v.last().unwrap().1.h })
        };
        let total: f64 = v.iter().map(|f| if horizontal { f.1.w } else { f.1.h }).sum();
        let gap = (end - start - total) / (v.len() as f64 - if items.len() < 3 { -1.0 } else { 1.0 });
        let mut pos = if items.len() < 3 { start + gap } else { start };
        for (l, mut f) in v {
            if horizontal { f.x = pos; pos += f.w + gap; } else { f.y = pos; pos += f.h + gap; }
            self.set_xfrm(&l, &f);
        }
    }
}

fn darken(hex: &str) -> String {
    let c = crate::Rgba::hex(hex).unwrap_or(crate::Rgba::BLACK);
    crate::Rgba::rgb((c.r as f64 * 0.6) as u8, (c.g as f64 * 0.6) as u8, (c.b as f64 * 0.6) as u8).to_hex()
}

/// Days since 1970 → (year, month, day).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn now_days() -> i64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64 / 86_400).unwrap_or(0) }
pub fn today() -> String { today_as("datetime1") }

/// Today in a field's format (datetime1 = 10/26/2026, datetime2 = Monday, October 26, 2026, …).
pub fn today_as(kind: &str) -> String {
    let days = now_days();
    let (y, m, d) = civil(days);
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    const DAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
    let wd = DAYS[days.rem_euclid(7) as usize];
    let mn = MONTHS[(m - 1) as usize];
    match kind {
        "iso" => format!("{y}-{m:02}-{d:02}"),
        "datetime2" => format!("{wd}, {mn} {d}, {y}"),
        "datetime3" => format!("{d} {mn} {y}"),
        "datetime4" => format!("{mn} {d}, {y}"),
        "datetime5" => format!("{d}-{}-{:02}", &mn[..3], y % 100),
        "datetime6" => format!("{mn} {:02}", y % 100),
        "datetime7" => format!("{}-{:02}", &mn[..3], y % 100),
        _ => format!("{m}/{d}/{y}"),
    }
}

/// Date formats offered by Date & Time (field type, example).
pub fn date_formats() -> Vec<(&'static str, String)> {
    ["datetime1", "datetime2", "datetime3", "datetime4", "datetime5", "datetime6", "datetime7"].iter().map(|k| (*k, today_as(k))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Kind;

    #[test]
    fn formatting_commands() {
        let mut p = Presentation::new_blank(true);
        let blank = p.all_layouts().into_iter().find(|l| p.layout_type(l) == "blank").unwrap();
        let s = p.add_slide(&blank, 0);
        let x = Xfrm { x: 1e6, y: 1e6, w: 3e6, h: 1.5e6, ..Default::default() };
        let a = p.insert_shape(&s, "rect", &x, false).unwrap();
        p.set_shape_text(&a, None, "hello world. second one");
        p.set_effect(&a, &Effect::Shadow { dir: 45.0, dist: 4.0, blur: 6.0, color: "000000".into(), alpha: 40 });
        p.set_effect(&a, &Effect::Glow { radius: 8.0, color: "FFC000".into() });
        p.set_effect(&a, &Effect::Reflection { size: 0.5, dist: 2.0 });
        let scene = p.scene(&s);
        let it = scene.items.iter().find(|i| i.loc == a).unwrap();
        assert!(it.effects.shadow.is_some() && it.effects.glow.is_some() && it.effects.reflection.is_some());
        p.change_case(&a, None, None, "sentence");
        assert_eq!(p.shape_text(&a, None), "Hello world. Second one");
        p.change_case(&a, None, Some((0, 5)), "upper");
        assert_eq!(p.shape_text(&a, None), "HELLO world. Second one");
        p.set_wordart(&a, 2);
        let scene = p.scene(&s);
        let Kind::Shape { text: Some(t), .. } = &scene.items.iter().find(|i| i.loc == a).unwrap().kind else { panic!() };
        assert!(t.paras[0].runs[0].outline.is_some());
        p.set_gradient_fill(&a, &[(0.0, "FFFFFF".into()), (1.0, "4472C4".into())], 90.0, false);
        p.set_quick_style(&a, "accent2", 4);
        p.set_shape_link(&a, Some("#1"));
        assert_eq!(p.shape_link(&a), Some(("slide".into(), "0".into())));
        p.set_shape_link(&a, Some("example.com"));
        assert_eq!(p.shape_link(&a), Some(("url".into(), "https://example.com".into())));
        // Group and ungroup keep positions.
        let b = p.insert_shape(&s, "ellipse", &Xfrm { x: 5e6, y: 3e6, w: 1e6, h: 1e6, ..Default::default() }, false).unwrap();
        let fa = p.xfrm(&a).unwrap();
        let fb = p.xfrm(&b).unwrap();
        let g = p.group(&[a.clone(), b.clone()], &[fa, fb]).unwrap();
        assert_eq!(p.element(&g).unwrap().local(), "grpSp");
        let members = p.ungroup(&g);
        assert_eq!(members.len(), 2);
        let back: Vec<Xfrm> = members.iter().map(|l| p.xfrm(l).unwrap()).collect();
        assert!((back[0].x - fa.x).abs() < 1.0 && (back[1].y - fb.y).abs() < 1.0);
        // Sections, header/footer, fields.
        let s2 = p.add_slide(&blank, 1);
        p.set_sections(vec![("Intro".into(), 0), ("Main".into(), 1)]);
        assert_eq!(p.sections(), vec![("Intro".into(), 0), ("Main".into(), 1)]);
        p.set_header_footer(&[s.clone(), s2.clone()], Some(""), true, Some("Woosool"), false);
        let n = p.scene(&s2).items.iter().filter(|i| i.placeholder.as_ref().is_some_and(|ph| matches!(ph.0.as_str(), "dt" | "ftr" | "sldNum"))).count();
        assert_eq!(n, 3);
        p.insert_field(&members[0], None, 0, "slidenum", 1);
        assert!(p.shape_text(&members[0], None).starts_with('1'));
        let again = Presentation::open(&p.save().unwrap()).unwrap();
        assert_eq!(again.sections().len(), 2);
    }
}
