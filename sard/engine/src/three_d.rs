//! 3-D on shapes (a:scene3d and a:sp3d): the camera (presets or a free X/Y/Z rotation, with
//! perspective), the extrusion depth and its color, the contour, the top and bottom bevels,
//! the material and the lighting — read, written, and drawn (the shape is shaded by its bevel,
//! extruded and projected onto the slide).

use crate::color::{self, Ctx, Rgba};
use crate::pres::Presentation;
use crate::scene::Loc;
use crate::xml::{Element, Node};
use tiny_skia::Pixmap;

#[derive(Clone, Debug, PartialEq)]
pub struct Bevel { pub kind: String, pub w: f64, pub h: f64 }

#[derive(Clone, Debug, PartialEq)]
pub struct ThreeD {
    /// The camera preset (its rotation is used unless `rot` is set).
    pub camera: String,
    /// Free rotation in degrees: X (turning about the vertical axis), Y (tilting), Z.
    pub rot: Option<(f64, f64, f64)>,
    /// Field of view in degrees for perspective cameras (0 = none).
    pub fov: f64,
    /// Extrusion depth and contour width (EMU); their colors (None = the shape's).
    pub depth: f64,
    pub depth_color: Option<Rgba>,
    pub contour: f64,
    pub contour_color: Option<Rgba>,
    pub top: Option<Bevel>,
    pub bottom: Option<Bevel>,
    pub material: String,
    pub light: String,
    /// The light's angle around the shape (degrees).
    pub light_angle: f64,
    /// Distance from the ground (EMU).
    pub z: f64,
}

impl Default for ThreeD {
    fn default() -> Self { ThreeD { camera: "orthographicFront".into(), rot: None, fov: 0.0, depth: 0.0, depth_color: None, contour: 0.0, contour_color: None, top: None, bottom: None, material: "warmMatte".into(), light: "threePt".into(), light_angle: 0.0, z: 0.0 } }
}

/// Camera presets: (id, name, X, Y, Z rotation, field of view, oblique direction).
pub const CAMERAS: &[(&str, &str, f64, f64, f64, f64)] = &[
    ("orthographicFront", "No Rotation", 0.0, 0.0, 0.0, 0.0),
    ("isometricLeftDown", "Isometric: Left Down", 45.0, 35.0, 0.0, 0.0),
    ("isometricRightUp", "Isometric: Right Up", 315.0, 35.0, 0.0, 0.0),
    ("isometricTopUp", "Isometric: Top Up", 314.7, 324.6, 60.2, 0.0),
    ("isometricBottomDown", "Isometric: Bottom Down", 314.7, 35.4, 299.8, 0.0),
    ("isometricTopDown", "Isometric: Top Down", 45.3, 35.4, 299.8, 0.0),
    ("isometricBottomUp", "Isometric: Bottom Up", 45.3, 324.6, 60.2, 0.0),
    ("isometricLeftUp", "Isometric: Left Up", 45.0, 325.0, 0.0, 0.0),
    ("isometricRightDown", "Isometric: Right Down", 315.0, 325.0, 0.0, 0.0),
    ("isometricOffAxis1Left", "Off Axis 1: Left", 64.0, 18.0, 0.0, 0.0),
    ("isometricOffAxis1Right", "Off Axis 1: Right", 334.0, 18.0, 0.0, 0.0),
    ("isometricOffAxis1Top", "Off Axis 1: Top", 306.5, 301.3, 57.6, 0.0),
    ("isometricOffAxis2Left", "Off Axis 2: Left", 26.0, 18.0, 0.0, 0.0),
    ("isometricOffAxis2Right", "Off Axis 2: Right", 296.0, 18.0, 0.0, 0.0),
    ("isometricOffAxis2Top", "Off Axis 2: Top", 53.5, 301.3, 302.4, 0.0),
    ("perspectiveFront", "Perspective: Front", 0.0, 0.0, 0.0, 45.0),
    ("perspectiveLeft", "Perspective: Left", 20.0, 0.0, 0.0, 45.0),
    ("perspectiveRight", "Perspective: Right", 340.0, 0.0, 0.0, 45.0),
    ("perspectiveAbove", "Perspective: Above", 0.0, 340.0, 0.0, 45.0),
    ("perspectiveBelow", "Perspective: Below", 0.0, 20.0, 0.0, 45.0),
    ("perspectiveAboveLeftFacing", "Perspective: Above, Left Facing", 14.8, 338.0, 0.0, 45.0),
    ("perspectiveAboveRightFacing", "Perspective: Above, Right Facing", 345.2, 338.0, 0.0, 45.0),
    ("perspectiveContrastingLeftFacing", "Perspective: Contrasting Left", 43.9, 10.4, 356.4, 45.0),
    ("perspectiveContrastingRightFacing", "Perspective: Contrasting Right", 316.1, 10.4, 3.6, 45.0),
    ("perspectiveHeroicExtremeLeftFacing", "Perspective: Heroic Extreme Left", 34.5, 8.1, 357.1, 80.0),
    ("perspectiveHeroicExtremeRightFacing", "Perspective: Heroic Extreme Right", 325.5, 8.1, 2.9, 80.0),
    ("perspectiveRelaxed", "Perspective: Relaxed", 0.0, 309.6, 0.0, 45.0),
    ("perspectiveRelaxedModerately", "Perspective: Relaxed Moderately", 0.0, 324.8, 0.0, 45.0),
    ("obliqueTopLeft", "Oblique: Top Left", 0.0, 0.0, 0.0, 0.0),
    ("obliqueTopRight", "Oblique: Top Right", 0.0, 0.0, 0.0, 0.0),
    ("obliqueBottomLeft", "Oblique: Bottom Left", 0.0, 0.0, 0.0, 0.0),
    ("obliqueBottomRight", "Oblique: Bottom Right", 0.0, 0.0, 0.0, 0.0),
    ("obliqueTop", "Oblique: Top", 0.0, 0.0, 0.0, 0.0),
    ("obliqueBottom", "Oblique: Bottom", 0.0, 0.0, 0.0, 0.0),
    ("obliqueLeft", "Oblique: Left", 0.0, 0.0, 0.0, 0.0),
    ("obliqueRight", "Oblique: Right", 0.0, 0.0, 0.0, 0.0),
];

pub const BEVELS: &[(&str, &str)] = &[("circle", "Circle"), ("relaxedInset", "Relaxed Inset"), ("cross", "Cross"), ("coolSlant", "Cool Slant"), ("angle", "Angle"), ("softRound", "Soft Round"), ("convex", "Convex"), ("slope", "Slope"), ("divot", "Divot"), ("riblet", "Riblet"), ("hardEdge", "Hard Edge"), ("artDeco", "Art Deco")];
pub const MATERIALS: &[(&str, &str)] = &[("matte", "Matte"), ("warmMatte", "Warm Matte"), ("plastic", "Plastic"), ("metal", "Metal"), ("dkEdge", "Dark Edge"), ("softEdge", "Soft Edge"), ("flat", "Flat"), ("legacyWireframe", "Wireframe"), ("powder", "Powder"), ("translucentPowder", "Translucent Powder"), ("clear", "Clear"), ("softmetal", "Soft Metal")];
pub const LIGHTS: &[(&str, &str)] = &[("threePt", "Three Point"), ("balanced", "Balanced"), ("soft", "Soft"), ("harsh", "Harsh"), ("flood", "Flood"), ("contrasting", "Contrasting"), ("morning", "Morning"), ("sunrise", "Sunrise"), ("sunset", "Sunset"), ("chilly", "Chilly"), ("freezing", "Freezing"), ("flat", "Flat"), ("twoPt", "Two Point"), ("glow", "Glow"), ("brightRoom", "Bright Room")];

impl ThreeD {
    /// The rotation in effect (degrees).
    pub fn rotation(&self) -> (f64, f64, f64) {
        self.rot.unwrap_or_else(|| CAMERAS.iter().find(|c| c.0 == self.camera).map(|c| (c.2, c.3, c.4)).unwrap_or((0.0, 0.0, 0.0)))
    }
    pub fn perspective(&self) -> f64 {
        if self.camera.starts_with("perspective") { if self.fov > 0.0 { self.fov } else { CAMERAS.iter().find(|c| c.0 == self.camera).map(|c| c.5).unwrap_or(45.0) } } else { 0.0 }
    }
    /// Whether it changes how the shape looks at all.
    pub fn visible(&self) -> bool {
        let (x, y, z) = self.rotation();
        self.top.is_some() || self.bottom.is_some() || self.depth > 0.0 || self.contour > 0.0 || x != 0.0 || y != 0.0 || z != 0.0 || self.camera.starts_with("oblique") || self.material == "legacyWireframe"
    }

    /// Read from a shape's spPr (falling back to a theme effect style's).
    pub fn read(sp_pr: Option<&Element>, style: Option<&Element>, ctx: &Ctx) -> Option<ThreeD> {
        let pick = |n: &str| sp_pr.and_then(|p| p.child(n)).or_else(|| style.and_then(|s| s.child(n)));
        let (scene, sp3d) = (pick("a:scene3d"), pick("a:sp3d"));
        if scene.is_none() && sp3d.is_none() { return None; }
        let mut t = ThreeD::default();
        if let Some(s) = scene {
            if let Some(c) = s.child("a:camera") {
                t.camera = c.attr("prst").unwrap_or("orthographicFront").to_string();
                t.fov = c.attr_f64("fov").map(|v| v / 60_000.0).unwrap_or(0.0);
                if let Some(r) = c.child("a:rot") { t.rot = Some((r.attr_f64("lon").unwrap_or(0.0) / 60_000.0, r.attr_f64("lat").unwrap_or(0.0) / 60_000.0, r.attr_f64("rev").unwrap_or(0.0) / 60_000.0)); }
            }
            if let Some(l) = s.child("a:lightRig") {
                t.light = l.attr("rig").unwrap_or("threePt").to_string();
                t.light_angle = l.child("a:rot").and_then(|r| r.attr_f64("rev")).map(|v| v / 60_000.0).unwrap_or(0.0);
            }
        }
        if let Some(s) = sp3d {
            t.depth = s.attr_f64("extrusionH").unwrap_or(0.0);
            t.contour = s.attr_f64("contourW").unwrap_or(0.0);
            t.z = s.attr_f64("z").unwrap_or(0.0);
            t.material = s.attr("prstMaterial").unwrap_or("warmMatte").to_string();
            let bevel = |b: &Element| Bevel { kind: b.attr("prst").unwrap_or("circle").to_string(), w: b.attr_f64("w").unwrap_or(76_200.0), h: b.attr_f64("h").unwrap_or(76_200.0) };
            t.top = s.child("a:bevelT").map(bevel);
            t.bottom = s.child("a:bevelB").map(bevel);
            t.depth_color = s.child("a:extrusionClr").and_then(|c| color::resolve_child(c, ctx));
            t.contour_color = s.child("a:contourClr").and_then(|c| color::resolve_child(c, ctx));
        }
        Some(t)
    }
}

fn hex_el(name: &str, c: &str) -> Element { Element::new(name).with_child(Element::new("a:srgbClr").with_attr("val", c)) }
fn hex_of(c: Rgba) -> String { format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b) }

impl Presentation {
    /// The shape's own 3-D settings (as stored on it).
    pub fn three_d(&self, loc: &Loc) -> ThreeD {
        let ctx = Ctx::default();
        self.element(loc).and_then(|sp| ThreeD::read(sp.child("p:spPr"), None, &ctx)).unwrap_or_default()
    }

    /// Store the shape's 3-D settings (removing them when they change nothing).
    pub fn set_three_d(&mut self, loc: &Loc, t: &ThreeD) {
        let Some(sp) = self.element_mut(loc) else { return };
        let Some(pr) = sp.child_mut("p:spPr") else { return };
        pr.remove_children("a:scene3d");
        pr.remove_children("a:sp3d");
        if !t.visible() && t.material == "warmMatte" { return; }
        let mut cam = Element::new("a:camera").with_attr("prst", t.camera.clone());
        if t.fov > 0.0 && t.camera.starts_with("perspective") { cam.set_attr("fov", ((t.fov * 60_000.0).round() as i64).to_string()); }
        if let Some((x, y, z)) = t.rot {
            let a = |v: f64| (((v.rem_euclid(360.0)) * 60_000.0).round() as i64).to_string();
            cam.children.push(Node::Element(Element::new("a:rot").with_attr("lat", a(y)).with_attr("lon", a(x)).with_attr("rev", a(z))));
        }
        let mut rig = Element::new("a:lightRig").with_attr("rig", t.light.clone()).with_attr("dir", "t");
        if t.light_angle != 0.0 { rig.children.push(Node::Element(Element::new("a:rot").with_attr("lat", "0").with_attr("lon", "0").with_attr("rev", ((t.light_angle.rem_euclid(360.0) * 60_000.0).round() as i64).to_string()))); }
        let scene = Element::new("a:scene3d").with_child(cam).with_child(rig);
        let mut sp3d = Element::new("a:sp3d");
        if t.z != 0.0 { sp3d.set_attr("z", (t.z.round() as i64).to_string()); }
        if t.depth > 0.0 { sp3d.set_attr("extrusionH", (t.depth.round() as i64).to_string()); }
        if t.contour > 0.0 { sp3d.set_attr("contourW", (t.contour.round() as i64).to_string()); }
        if t.material != "warmMatte" { sp3d.set_attr("prstMaterial", t.material.clone()); }
        let bevel = |n: &str, b: &Bevel| Element::new(n).with_attr("w", (b.w.round() as i64).to_string()).with_attr("h", (b.h.round() as i64).to_string()).with_attr("prst", b.kind.clone());
        if let Some(b) = &t.top { sp3d.children.push(Node::Element(bevel("a:bevelT", b))); }
        if let Some(b) = &t.bottom { sp3d.children.push(Node::Element(bevel("a:bevelB", b))); }
        if let Some(c) = t.depth_color { sp3d.children.push(Node::Element(hex_el("a:extrusionClr", &hex_of(c)))); }
        if let Some(c) = t.contour_color { sp3d.children.push(Node::Element(hex_el("a:contourClr", &hex_of(c)))); }
        // scene3d and sp3d sit after the effects, before extLst.
        let pos = pr.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == "a:extLst")).unwrap_or(pr.children.len());
        pr.children.insert(pos, Node::Element(sp3d));
        pr.children.insert(pos, Node::Element(scene));
    }
}

/// A small picture of a 3-D look on a rounded square (the presets galleries), PNG-free:
/// premultiplied RGBA and its size.
pub fn preview(t: &ThreeD, px: u32) -> Option<(Vec<u8>, u32, u32)> {
    let mut p = Presentation::new_blank(false);
    let l = p.all_layouts().last().cloned()?;
    let s = p.add_slide(&l, 0);
    let (sw, sh) = p.size();
    let side = sh * 0.5;
    let loc = p.insert_shape(&s, "roundRect", &crate::scene::Xfrm { x: (sw - side) / 2.0, y: (sh - side) / 2.0, w: side, h: side, ..Default::default() }, false)?;
    p.set_fill(&loc, Some("DCE6F2"));
    p.set_line(&loc, Some("A6A6A6"), Some(0.75), None);
    let mut look = t.clone();
    // Depth and a soft edge so turns read clearly, unless the look brings its own.
    if look.depth == 0.0 && look.top.is_none() { look.depth = side * 0.18; look.top = Some(Bevel { kind: "softRound".into(), w: side * 0.04, h: side * 0.04 }); }
    p.set_three_d(&loc, &look);
    let mut scene = p.scene(&s);
    scene.background = crate::scene::Fill::None;
    // Just the square's neighbourhood.
    let mut images = crate::render::Images::default();
    let k = px as f64 / (side * 1.45);
    let pm = crate::render::render(&p, &scene, &mut images, crate::render::Options { scale: k, prompts: false, skip_text_of: None, translate: None })?;
    let (cx, cy) = ((sw / 2.0 * k) as i32, (sh / 2.0 * k) as i32);
    let half = px as i32 / 2;
    let mut out = Pixmap::new(px, px)?;
    out.draw_pixmap(half - cx, half - cy, pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
    Some((out.data().to_vec(), px, px))
}

// ---- ready-made 3-D models -----------------------------------------------------------------

/// The 3-D Models gallery: (id, name, preset geometry, width, height in EMU).
pub const MODELS: &[(&str, &str, &str, f64, f64)] = &[
    ("cube", "Cube", "rect", 1_600_000.0, 1_600_000.0),
    ("box", "Box", "rect", 2_400_000.0, 1_500_000.0),
    ("slab", "Slab", "roundRect", 2_600_000.0, 1_600_000.0),
    ("sphere", "Sphere", "ellipse", 1_800_000.0, 1_800_000.0),
    ("cylinder", "Cylinder", "ellipse", 1_500_000.0, 1_500_000.0),
    ("cone", "Cone", "ellipse", 1_800_000.0, 1_800_000.0),
    ("pyramid", "Pyramid", "rect", 1_800_000.0, 1_800_000.0),
    ("prism", "Prism", "triangle", 1_800_000.0, 1_600_000.0),
    ("torus", "Torus", "donut", 1_900_000.0, 1_900_000.0),
    ("disc", "Coin", "ellipse", 1_700_000.0, 1_700_000.0),
    ("star", "Star", "star5", 1_900_000.0, 1_800_000.0),
    ("heart", "Heart", "heart", 1_800_000.0, 1_700_000.0),
    ("arrow", "Arrow", "rightArrow", 2_400_000.0, 1_300_000.0),
    ("hexagon", "Hexagon", "hexagon", 1_900_000.0, 1_650_000.0),
    ("ring", "Ring", "donut", 1_900_000.0, 1_900_000.0),
    ("button", "Button", "roundRect", 2_200_000.0, 900_000.0),
];

/// The 3-D look of a gallery model of size `w`×`h`.
pub fn model(id: &str, w: f64, h: f64) -> ThreeD {
    let m = w.min(h);
    let bevel = |kind: &str, k: f64| Some(Bevel { kind: kind.into(), w: m * k, h: m * k });
    let base = ThreeD { material: "plastic".into(), light: "threePt".into(), ..ThreeD::default() };
    match id {
        "cube" => ThreeD { camera: "isometricOffAxis2Left".into(), depth: m, top: bevel("angle", 0.02), ..base },
        "box" => ThreeD { camera: "isometricOffAxis1Right".into(), depth: m * 0.8, ..base },
        "slab" => ThreeD { camera: "perspectiveContrastingRightFacing".into(), depth: m * 0.18, top: bevel("softRound", 0.05), ..base },
        "sphere" => ThreeD { top: bevel("circle", 0.5), bottom: bevel("circle", 0.5), material: "plastic".into(), ..base },
        "cylinder" => ThreeD { camera: "perspectiveFront".into(), rot: Some((0.0, 300.0, 0.0)), fov: 30.0, depth: m * 1.2, ..base },
        "cone" => ThreeD { camera: "perspectiveFront".into(), rot: Some((0.0, 325.0, 0.0)), fov: 30.0, top: bevel("angle", 0.5), ..base },
        "pyramid" => ThreeD { camera: "isometricOffAxis2Left".into(), top: bevel("angle", 0.5), ..base },
        "prism" => ThreeD { camera: "isometricOffAxis1Right".into(), depth: m * 0.7, ..base },
        "torus" => ThreeD { camera: "perspectiveFront".into(), rot: Some((0.0, 320.0, 0.0)), fov: 30.0, top: bevel("circle", 0.12), bottom: bevel("circle", 0.12), ..base },
        "disc" => ThreeD { camera: "perspectiveFront".into(), rot: Some((0.0, 300.0, 0.0)), fov: 30.0, depth: m * 0.12, top: bevel("circle", 0.04), material: "metal".into(), ..base },
        "ring" => ThreeD { camera: "isometricOffAxis2Left".into(), depth: m * 0.15, top: bevel("circle", 0.05), material: "metal".into(), ..base },
        "button" => ThreeD { top: bevel("softRound", 0.12), material: "plastic".into(), ..base },
        _ => ThreeD { camera: "isometricOffAxis2Left".into(), depth: m * 0.25, top: bevel("circle", 0.03), ..base },
    }
}

impl Presentation {
    /// Insert a 3-D model from the gallery at (x, y) (its top-left, EMU).
    pub fn insert_3d_model(&mut self, slide: &str, id: &str, x: f64, y: f64) -> Option<Loc> {
        let &(_, name, preset, w, h) = MODELS.iter().find(|m| m.0 == id)?;
        let loc = self.insert_shape(slide, preset, &crate::scene::Xfrm { x, y, w, h, ..Default::default() }, false)?;
        if let Some(c) = self.element_mut(&loc).and_then(|sp| sp.child_mut("p:nvSpPr")).and_then(|nv| nv.child_mut("p:cNvPr")) { c.set_attr("name", format!("3D {name}")); }
        if id == "ring" { self.set_adjust(&loc, "adj", 12_000.0); }
        self.set_three_d(&loc, &model(id, w, h));
        Some(loc)
    }
}

// ---- drawing ------------------------------------------------------------------------------

/// The bevel's height (0–1) at a fraction `d` (0 at the edge, 1 at the bevel's inner end).
fn profile(kind: &str, d: f32) -> f32 {
    let d = d.clamp(0.0, 1.0);
    match kind {
        "angle" | "slope" => d,
        "coolSlant" => (d * 1.6).min(1.0),
        "convex" => (1.0 - (1.0 - d).powi(2)).sqrt(),
        "softRound" => d * d * (3.0 - 2.0 * d),
        "relaxedInset" => 1.0 - (1.0 - d).powi(3),
        "cross" => if d < 0.5 { d * 2.0 } else { 1.0 - (d - 0.5) },
        "divot" => if d < 0.6 { d / 0.6 } else { 1.0 - (d - 0.6) * 1.2 },
        "riblet" => (d * std::f32::consts::PI * 3.0).sin().abs() * 0.6 + d * 0.4,
        "hardEdge" => (d * 3.0).min(1.0),
        "artDeco" => ((d * 4.0).floor() / 4.0 + (d * 4.0).fract() * 0.25).min(1.0),
        _ => (1.0 - (1.0 - d).powi(2)).sqrt(), // circle
    }
}

/// The distance (px, up to `max`) from each covered pixel to the shape's edge (chamfer).
fn edge_distance(pm: &Pixmap, max: f32) -> Vec<f32> {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let big = max + 2.0;
    let px = pm.pixels();
    let mut d: Vec<f32> = (0..w * h).map(|i| if px[i].alpha() > 127 { big } else { 0.0 }).collect();
    let s2 = std::f32::consts::SQRT_2;
    for y in 0..h { for x in 0..w {
        let i = y * w + x;
        if d[i] == 0.0 { continue; }
        let mut v = d[i];
        if x == 0 || y == 0 { v = v.min(1.0); }
        if x > 0 { v = v.min(d[i - 1] + 1.0); }
        if y > 0 { v = v.min(d[i - w] + 1.0); if x > 0 { v = v.min(d[i - w - 1] + s2); } if x + 1 < w { v = v.min(d[i - w + 1] + s2); } }
        d[i] = v;
    } }
    for y in (0..h).rev() { for x in (0..w).rev() {
        let i = y * w + x;
        if d[i] == 0.0 { continue; }
        let mut v = d[i];
        if x + 1 == w || y + 1 == h { v = v.min(1.0); }
        if x + 1 < w { v = v.min(d[i + 1] + 1.0); }
        if y + 1 < h { v = v.min(d[i + w] + 1.0); if x + 1 < w { v = v.min(d[i + w + 1] + s2); } if x > 0 { v = v.min(d[i + w - 1] + s2); } }
        d[i] = v;
    } }
    d
}

/// The light direction (toward the light) for a rig and angle.
fn light_dir(t: &ThreeD) -> (f32, f32, f32) {
    let a = (-135.0f32 + t.light_angle as f32).to_radians();
    let elev: f32 = match t.light.as_str() { "harsh" | "contrasting" => 35.0, "flat" | "flood" | "brightRoom" => 70.0, "soft" | "balanced" => 55.0, _ => 45.0 };
    let (e, c) = (elev.to_radians().sin(), elev.to_radians().cos());
    (a.cos() * c, a.sin() * c, e)
}

/// Shade the shape's pixels by its bevel and material (in place).
pub fn shade(pm: &mut Pixmap, t: &ThreeD, px_per_emu: f64) {
    if t.material == "flat" { return; }
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let l = light_dir(t);
    let flat = l.2;
    let (spec, contrast): (f32, f32) = match t.material.as_str() { "metal" | "softmetal" => (0.9, 1.6), "plastic" | "clear" => (0.6, 1.0), "dkEdge" => (0.2, 1.4), "matte" | "powder" | "translucentPowder" => (0.0, 0.9), _ => (0.25, 1.0) };
    let Some(b) = &t.top else {
        // No bevel: a gentle overall light for shiny materials only.
        if spec < 0.5 { return; }
        for (i, p) in pm.pixels_mut().iter_mut().enumerate() {
            let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
            let k = 1.0 + 0.25 * (1.0 - (x + y)) * spec;
            *p = scale_px(*p, k, 0.0);
        }
        return;
    };
    let bw = ((b.w * px_per_emu) as f32).max(1.0);
    let bh = (b.h / b.w.max(1.0)) as f32;
    let raw = edge_distance(pm, bw);
    // Smoothed, so curved edges don't band.
    let mut dist = raw.clone();
    // Wide bevels need more smoothing (the chamfer distance is faceted).
    let passes = ((bw / 6.0) as usize).clamp(2, 8);
    for _ in 0..passes {
        let src = dist.clone();
        for y in 1..h.saturating_sub(1) { for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            // Only the bevel's band needs smoothing; inside it the height is flat.
            if raw[i] == 0.0 || raw[i] > bw + 2.0 { continue; }
            let mut sum = 0.0;
            for dy in [-1i64, 0, 1] { for dx in [-1i64, 0, 1] { sum += src[(i as i64 + dy * w as i64 + dx) as usize]; } }
            dist[i] = sum / 9.0;
        } }
    }
    let height = |x: usize, y: usize| profile(&b.kind, dist[y * w + x] / bw) * bw * bh;
    let wire = t.material == "legacyWireframe";
    let px = pm.pixels_mut();
    for y in 0..h { for x in 0..w {
        let i = y * w + x;
        if px[i].alpha() == 0 || raw[i] >= bw + 1.0 { continue; }
        let (x0, x1, y0, y1) = (x.saturating_sub(1), (x + 1).min(w - 1), y.saturating_sub(1), (y + 1).min(h - 1));
        let gx = (height(x1, y) - height(x0, y)) / (x1 - x0).max(1) as f32;
        let gy = (height(x, y1) - height(x, y0)) / (y1 - y0).max(1) as f32;
        let n = { let (a, b, c) = (-gx, -gy, 1.0f32); let m = (a * a + b * b + c * c).sqrt(); (a / m, b / m, c / m) };
        let diff = (n.0 * l.0 + n.1 * l.1 + n.2 * l.2).max(0.0);
        let k = (1.0 + (diff / flat - 1.0) * contrast * 0.8).clamp(0.55, 1.6);
        // Specular: the light reflected toward the viewer.
        let hv = { let (a, b, c) = (l.0, l.1, l.2 + 1.0); let m = (a * a + b * b + c * c).sqrt(); (a / m, b / m, c / m) };
        let s = (n.0 * hv.0 + n.1 * hv.1 + n.2 * hv.2).max(0.0).powf(24.0) * spec;
        if wire { let edge = dist[i] < 1.5 || (dist[i] - bw).abs() < 1.0; if !edge { px[i] = tiny_skia::PremultipliedColorU8::TRANSPARENT; } continue; }
        px[i] = scale_px(px[i], k, s);
    } }
}

fn scale_px(p: tiny_skia::PremultipliedColorU8, k: f32, spec: f32) -> tiny_skia::PremultipliedColorU8 {
    let a = p.alpha() as f32;
    let f = |c: u8| ((c as f32 * k + spec * a).clamp(0.0, a)) as u8;
    tiny_skia::PremultipliedColorU8::from_rgba(f(p.red()), f(p.green()), f(p.blue()), p.alpha()).unwrap_or(p)
}

/// A projective map from layer pixels to target pixels: (row-major 3×3).
type H = [f64; 9];

fn invert(m: &H) -> Option<H> {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 { return None; }
    let k = 1.0 / det;
    Some([(e * i - f * h) * k, (c * h - b * i) * k, (b * f - c * e) * k, (f * g - d * i) * k, (a * i - c * g) * k, (c * d - a * f) * k, (d * h - e * g) * k, (b * g - a * h) * k, (a * e - b * d) * k])
}

/// The homography of the layer's plane, pushed back by `z` px, seen through the camera
/// about `center` (target px).
fn plane_map(t: &ThreeD, ox: f64, oy: f64, center: (f64, f64), z: f64, size: f64) -> H {
    let (rx, ry, rz) = t.rotation();
    let (rx, ry, rz) = (rx.to_radians(), ry.to_radians(), rz.to_radians());
    // R = Rz · Rx(tilt) · Ry(turn)
    let (sy, cy) = rx.sin_cos(); // turn about the vertical axis
    let (sx, cx) = ry.sin_cos(); // tilt about the horizontal axis
    let (sz, cz) = rz.sin_cos();
    let ry_m = [cy, 0.0, -sy, 0.0, 1.0, 0.0, sy, 0.0, cy];
    let rx_m = [1.0, 0.0, 0.0, 0.0, cx, sx, 0.0, -sx, cx];
    let rz_m = [cz, -sz, 0.0, sz, cz, 0.0, 0.0, 0.0, 1.0];
    let mul = |a: &[f64; 9], b: &[f64; 9]| { let mut r = [0.0; 9]; for i in 0..3 { for j in 0..3 { r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum(); } } r };
    let r = mul(&rz_m, &mul(&rx_m, &ry_m));
    // Oblique cameras shear the depth instead of turning.
    let (shx, shy) = match t.camera.as_str() { "obliqueTopLeft" => (-0.5, -0.5), "obliqueTopRight" => (0.5, -0.5), "obliqueBottomLeft" => (-0.5, 0.5), "obliqueBottomRight" => (0.5, 0.5), "obliqueTop" => (0.0, -0.7), "obliqueBottom" => (0.0, 0.7), "obliqueLeft" => (-0.7, 0.0), "obliqueRight" => (0.7, 0.0), _ => (0.0, 0.0) };
    let fov = t.perspective();
    let focal = if fov > 0.0 { size / (fov.to_radians() / 2.0).tan() * 0.9 } else { 0.0 };
    // A layer pixel (u, v) is the point p = (u + ox - cx, v + oy - cy, -z) in camera space.
    // Rotated q = R p; screen = c + q.xy / (1 - q.z / focal) (no division without perspective).
    let (cxp, cyp) = center;
    let (bx, by) = (ox - cxp, oy - cyp);
    // q = R·(u, v, 0) + R·(bx, by, -z)
    let tq = [r[0] * bx + r[1] * by - r[2] * z, r[3] * bx + r[4] * by - r[5] * z, r[6] * bx + r[7] * by - r[8] * z];
    let (qx, qy, qz) = ([r[0], r[1], tq[0]], [r[3], r[4], tq[1]], [r[6], r[7], tq[2]]);
    let qx = [qx[0] + shx * 0.0, qx[1], qx[2] + shx * z];
    let qy = [qy[0], qy[1], qy[2] + shy * z];
    let w = if focal > 0.0 { [-qz[0] / focal, -qz[1] / focal, 1.0 - qz[2] / focal] } else { [0.0, 0.0, 1.0] };
    // screen.x * w = qx + cx * w
    [qx[0] + cxp * w[0], qx[1] + cxp * w[1], qx[2] + cxp * w[2], qy[0] + cyp * w[0], qy[1] + cyp * w[1], qy[2] + cyp * w[2], w[0], w[1], w[2]]
}

fn apply(m: &H, u: f64, v: f64) -> (f64, f64) {
    let w = m[6] * u + m[7] * v + m[8];
    ((m[0] * u + m[1] * v + m[2]) / w, (m[3] * u + m[4] * v + m[5]) / w)
}

/// Draw `src` (sitting at `ox`, `oy` on the target) through the map `m`; `tint` replaces
/// the colors (keeping coverage) when given.
fn warp(target: &mut Pixmap, src: &Pixmap, m: &H, tint: Option<(u8, u8, u8)>) {
    let Some(inv) = invert(m) else { return };
    let (sw, sh) = (src.width() as f64, src.height() as f64);
    let corners = [(0.0, 0.0), (sw, 0.0), (sw, sh), (0.0, sh)].map(|(u, v)| apply(m, u, v));
    if corners.iter().any(|c| !c.0.is_finite() || !c.1.is_finite()) { return; }
    let (tw, th) = (target.width() as i64, target.height() as i64);
    let x0 = corners.iter().map(|c| c.0).fold(f64::MAX, f64::min).floor().max(0.0) as i64;
    let x1 = (corners.iter().map(|c| c.0).fold(f64::MIN, f64::max).ceil() as i64).min(tw);
    let y0 = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min).floor().max(0.0) as i64;
    let y1 = (corners.iter().map(|c| c.1).fold(f64::MIN, f64::max).ceil() as i64).min(th);
    if x1 <= x0 || y1 <= y0 || (x1 - x0) * (y1 - y0) > 40_000_000 { return; }
    let sp = src.pixels();
    let swu = src.width() as usize;
    let tpx = target.pixels_mut();
    for y in y0..y1 {
        // Only the span of this row inside the (convex) projected quad.
        let yc = y as f64 + 0.5;
        let (mut sx0, mut sx1) = (f64::MAX, f64::MIN);
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            if (a.1 <= yc && b.1 >= yc) || (b.1 <= yc && a.1 >= yc) {
                let x = if (b.1 - a.1).abs() < 1e-9 { a.0.min(b.0) } else { a.0 + (yc - a.1) / (b.1 - a.1) * (b.0 - a.0) };
                let x2 = if (b.1 - a.1).abs() < 1e-9 { a.0.max(b.0) } else { x };
                sx0 = sx0.min(x); sx1 = sx1.max(x2);
            }
        }
        if sx0 > sx1 { continue; }
        let (rx0, rx1) = (((sx0 - 1.0).floor() as i64).max(x0), ((sx1 + 1.0).ceil() as i64).min(x1));
        for x in rx0..rx1 {
        let (u, v) = apply(&inv, x as f64 + 0.5, y as f64 + 0.5);
        if u < 0.0 || v < 0.0 || u >= sw || v >= sh { continue; }
        // Flat-colored planes (sides, contour) only need coverage: the nearest pixel will do.
        if let Some((r, g, b)) = tint {
            let a = sp[(v as usize).min(src.height() as usize - 1) * swu + (u as usize).min(swu - 1)].alpha() as f32;
            if a < 0.5 { continue; }
            let d = &mut tpx[(y * tw + x) as usize];
            let k = 1.0 - a / 255.0;
            let o = |s: f32, d: u8| (s + d as f32 * k).clamp(0.0, 255.0) as u8;
            let na = o(a, d.alpha());
            *d = tiny_skia::PremultipliedColorU8::from_rgba(o(r as f32 * a / 255.0, d.red()).min(na), o(g as f32 * a / 255.0, d.green()).min(na), o(b as f32 * a / 255.0, d.blue()).min(na), na).unwrap_or(*d);
            continue;
        }
        // Bilinear sampling.
        let (fu, fv) = (u - 0.5, v - 0.5);
        let (iu, iv) = (fu.floor().max(0.0) as usize, fv.floor().max(0.0) as usize);
        let (iu1, iv1) = ((iu + 1).min(swu - 1), (iv + 1).min(src.height() as usize - 1));
        let (au, av) = ((fu - iu as f64).clamp(0.0, 1.0) as f32, (fv - iv as f64).clamp(0.0, 1.0) as f32);
        let s = |i: usize, j: usize| sp[j * swu + i];
        let mix = |f: fn(&tiny_skia::PremultipliedColorU8) -> u8| {
            let a = f(&s(iu, iv)) as f32 * (1.0 - au) + f(&s(iu1, iv)) as f32 * au;
            let b = f(&s(iu, iv1)) as f32 * (1.0 - au) + f(&s(iu1, iv1)) as f32 * au;
            a * (1.0 - av) + b * av
        };
        let a = mix(|p| p.alpha());
        if a < 0.5 { continue; }
        let (r, g, b) = match tint { Some((r, g, b)) => (r as f32 * a / 255.0, g as f32 * a / 255.0, b as f32 * a / 255.0), None => (mix(|p| p.red()), mix(|p| p.green()), mix(|p| p.blue())) };
        let d = &mut tpx[(y * tw + x) as usize];
        let k = 1.0 - a / 255.0;
        let o = |s: f32, d: u8| (s + d as f32 * k).clamp(0.0, 255.0) as u8;
        let na = o(a, d.alpha());
        *d = tiny_skia::PremultipliedColorU8::from_rgba(o(r, d.red()).min(na), o(g, d.green()).min(na), o(b, d.blue()).min(na), na).unwrap_or(*d);
    } }
}

/// The color of the extrusion's sides: given, else the shape's average color.
fn side_color(src: &Pixmap, given: Option<Rgba>) -> (u8, u8, u8) {
    if let Some(c) = given { return (c.r, c.g, c.b); }
    let (mut r, mut g, mut b, mut n) = (0f64, 0f64, 0f64, 0f64);
    for p in src.pixels().iter().step_by(7) { let a = p.alpha() as f64; if a > 200.0 { r += p.red() as f64 * 255.0 / a; g += p.green() as f64 * 255.0 / a; b += p.blue() as f64 * 255.0 / a; n += 1.0; } }
    if n == 0.0 { (128, 128, 128) } else { ((r / n) as u8, (g / n) as u8, (b / n) as u8) }
}

/// Draw a shape's layer (`src`, at `ox`, `oy` on the target) in 3-D: shaded by its bevel and
/// material, extruded by its depth, turned about `center` (target px).
/// A shape's layer (`src`, sitting at `ox`, `oy` on a `tw`×`th` target) in 3-D: shaded by
/// its bevel and material, extruded by its depth, turned about `center`. Returns the drawn
/// layer and where it goes on the target (only the area it covers is allocated).
#[allow(clippy::too_many_arguments)]
pub fn compose_layer(mut src: Pixmap, ox: i32, oy: i32, center: (f64, f64), t: &ThreeD, px_per_emu: f64, tw: u32, th: u32) -> Option<(Pixmap, i32, i32)> {
    shade(&mut src, t, px_per_emu);
    let size = (src.width().max(src.height())) as f64 * 1.6;
    let depth = t.depth * px_per_emu;
    // Every plane to draw, back to front: (map, flat color for sides and contour).
    let mut planes: Vec<(H, Option<(u8, u8, u8)>)> = vec![];
    if depth > 0.5 {
        let (r, g, b) = side_color(&src, t.depth_color);
        // One slice every couple of pixels is enough for a solid side.
        let n = ((depth / 1.5).ceil() as usize).clamp(2, 40);
        for k in (1..=n).rev() {
            let z = depth * k as f64 / n as f64;
            let f = 0.55 + 0.3 * (1.0 - k as f64 / n as f64);
            planes.push((plane_map(t, ox as f64, oy as f64, center, z, size), Some(((r as f64 * f) as u8, (g as f64 * f) as u8, (b as f64 * f) as u8))));
        }
    }
    if t.contour > 0.0 {
        let c = t.contour_color.map(|c| (c.r, c.g, c.b)).unwrap_or((0, 0, 0));
        let cw = (t.contour * px_per_emu).max(1.0);
        for k in 0..8 {
            let a = k as f64 / 8.0 * std::f64::consts::TAU;
            planes.push((plane_map(t, ox as f64 + a.cos() * cw, oy as f64 + a.sin() * cw, (center.0 + a.cos() * cw, center.1 + a.sin() * cw), 0.0, size), Some(c)));
        }
    }
    planes.push((plane_map(t, ox as f64, oy as f64, center, 0.0, size), None));
    // The area all planes cover, within the target.
    let (sw, sh) = (src.width() as f64, src.height() as f64);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (m, _) in &planes {
        for (u, v) in [(0.0, 0.0), (sw, 0.0), (sw, sh), (0.0, sh)] {
            let (x, y) = apply(m, u, v);
            if !x.is_finite() || !y.is_finite() { continue; }
            x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y);
        }
    }
    let (lx, ly) = (x0.floor().max(0.0) as i32, y0.floor().max(0.0) as i32);
    let (rx, ry) = ((x1.ceil() as i64).min(tw as i64) as i32, (y1.ceil() as i64).min(th as i64) as i32);
    if rx <= lx || ry <= ly { return None; }
    let mut layer = Pixmap::new((rx - lx) as u32, (ry - ly) as u32)?;
    let (fx, fy) = (lx as f64, ly as f64);
    let local = |m: &H| -> H { [m[0] - fx * m[6], m[1] - fx * m[7], m[2] - fx * m[8], m[3] - fy * m[6], m[4] - fy * m[7], m[5] - fy * m[8], m[6], m[7], m[8]] };
    for (m, tint) in &planes { warp(&mut layer, &src, &local(m), *tint); }
    Some((layer, lx, ly))
}

// ---- a cache of drawn 3-D shapes --------------------------------------------------------------

thread_local! {
    /// (key, layer, x, y), oldest first; kept under a memory budget.
    static CACHE: std::cell::RefCell<Vec<(u64, std::rc::Rc<Pixmap>, i32, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
}
const CACHE_BYTES: usize = 160 * 1024 * 1024;

pub fn cached(key: u64) -> Option<(std::rc::Rc<Pixmap>, i32, i32)> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let i = c.iter().position(|e| e.0 == key)?;
        // Most recently used goes last.
        let e = c.remove(i);
        let out = (e.1.clone(), e.2, e.3);
        c.push(e);
        Some(out)
    })
}

pub fn remember(key: u64, layer: Pixmap, x: i32, y: i32) -> std::rc::Rc<Pixmap> {
    let rc = std::rc::Rc::new(layer);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.push((key, rc.clone(), x, y));
        let mut total: usize = c.iter().map(|e| e.1.data().len()).sum();
        while total > CACHE_BYTES && c.len() > 1 { total -= c[0].1.data().len(); c.remove(0); }
    });
    rc
}

/// Forget every drawn 3-D shape (frees their memory).
pub fn clear_cache() { CACHE.with(|c| c.borrow_mut().clear()); }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Xfrm;

    #[test]
    fn three_d_round_trips_and_draws() {
        let mut p = Presentation::new_blank(true);
        let l = p.all_layouts()[6].clone();
        let s = p.add_slide(&l, 0);
        let loc = p.insert_shape(&s, "rect", &Xfrm { x: 3e6, y: 2e6, w: 3e6, h: 2e6, ..Default::default() }, false).unwrap();
        let t = ThreeD { camera: "perspectiveLeft".into(), rot: Some((30.0, 10.0, 0.0)), fov: 45.0, depth: 300_000.0, contour: 12_700.0, top: Some(Bevel { kind: "circle".into(), w: 101_600.0, h: 101_600.0 }), material: "plastic".into(), ..Default::default() };
        p.set_three_d(&loc, &t);
        let back = p.three_d(&loc);
        assert_eq!((back.camera.as_str(), back.depth, back.top.clone(), back.material.as_str()), ("perspectiveLeft", 300_000.0, t.top.clone(), "plastic"));
        assert_eq!(back.rot.map(|r| (r.0.round(), r.1.round())), Some((30.0, 10.0)));
        let flat = crate::render::slide_png(&p, &s, 400).unwrap();
        if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") {
            // A sheet of looks for checking by eye.
            let mut q = Presentation::new_blank(true);
            let s2 = q.add_slide(&l, 0);
            let looks = [("isometricLeftDown", 200_000.0, Some("circle"), "warmMatte"), ("perspectiveRight", 0.0, Some("angle"), "metal"), ("obliqueTopLeft", 250_000.0, None, "plastic"), ("orthographicFront", 0.0, Some("convex"), "plastic"), ("perspectiveHeroicExtremeLeftFacing", 150_000.0, Some("softRound"), "matte"), ("isometricOffAxis1Top", 100_000.0, Some("relaxedInset"), "metal")];
            for (k, (cam, depth, bevel, mat)) in looks.iter().enumerate() {
                let x = Xfrm { x: 600_000.0 + (k % 3) as f64 * 3.8e6, y: 700_000.0 + (k / 3) as f64 * 3.2e6, w: 2.6e6, h: 1.8e6, ..Default::default() };
                let lc = q.insert_shape(&s2, if k % 2 == 0 { "roundRect" } else { "ellipse" }, &x, false).unwrap();
                q.set_three_d(&lc, &ThreeD { camera: cam.to_string(), depth: *depth, top: bevel.map(|b| Bevel { kind: b.into(), w: 120_000.0, h: 120_000.0 }), material: mat.to_string(), ..Default::default() });
            }
            let mut q2 = Presentation::new_blank(true);
            let s3 = q2.add_slide(&l, 0);
            for (k, m) in MODELS.iter().enumerate() { q2.insert_3d_model(&s3, m.0, 300_000.0 + (k % 6) as f64 * 1.95e6, 400_000.0 + (k / 6) as f64 * 2.1e6); }
            std::fs::write(format!("{dir}/three_d_models.png"), crate::render::slide_png(&q2, &s3, 1400).unwrap()).unwrap();
            std::fs::write(format!("{dir}/three_d.png"), crate::render::slide_png(&q, &s2, 1200).unwrap()).unwrap();
        }
        p.set_three_d(&loc, &ThreeD::default());
        let plain = crate::render::slide_png(&p, &s, 400).unwrap();
        assert_ne!(flat, plain, "3-D changes the picture");
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    #[test]
    #[ignore]
    fn heavy_slide_timing() {
        let mut p = Presentation::new_blank(true);
        let l = p.all_layouts().last().cloned().unwrap();
        let s = p.add_slide(&l, 0);
        for (k, m) in MODELS.iter().take(10).enumerate() { p.insert_3d_model(&s, m.0, (k % 5) as f64 * 2.3e6, (k / 5) as f64 * 2.5e6); }
        let spec = crate::diagram::Spec::new("orgChart", "A\n\tB\n\tC\n\t\tD\n\t\tE\n\tF");
        p.insert_diagram(&s, &spec, &crate::scene::Xfrm { x: 0.0, y: 5e6, w: 6e6, h: 1.8e6, ..Default::default() });
        p.insert_mermaid(&s, crate::mermaid::TEMPLATES[0].1, &crate::scene::Xfrm { x: 6e6, y: 5e6, w: 6e6, h: 1.8e6, ..Default::default() }).unwrap();
        let t = std::time::Instant::now();
        let scene = p.scene(&s);
        eprintln!("scene: {:?} ({} items)", t.elapsed(), scene.items.len());
        let mut images = crate::render::Images::default();
        for w in [1400.0, 2800.0, 168.0, 2800.0] {
            let t = std::time::Instant::now();
            crate::render::render(&p, &scene, &mut images, crate::render::Options { scale: w / scene.w, prompts: true, skip_text_of: None, translate: None });
            eprintln!("render {w}px: {:?}", t.elapsed());
        }
        let t = std::time::Instant::now();
        let big = crate::diagram::Spec::new("orgChart", &(0..40).map(|k| format!("{}Item {k}", if k == 0 { "" } else if k % 5 == 0 { "\t" } else { "\t\t" })).collect::<Vec<_>>().join("\n"));
        p.insert_diagram(&s, &big, &crate::scene::Xfrm { w: 1e7, h: 6e6, ..Default::default() });
        eprintln!("insert 40-item SmartArt: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        p.insert_mermaid(&s, crate::mermaid::TEMPLATES[9].1, &crate::scene::Xfrm { w: 1e7, h: 6e6, ..Default::default() }).unwrap();
        eprintln!("insert ER diagram: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        for _ in 0..10 { let _ = p.scene(&s); }
        eprintln!("10 scenes: {:?}", t.elapsed());
    }
}
