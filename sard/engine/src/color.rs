//! DrawingML colors: sRGB, scheme, system, preset, HSL and scRGB colors, with their transforms
//! (lumMod, tint, shade, satMod, alpha, …) applied the way PowerPoint renders them
//! (shape-color.ts): tint, shade and inv in linear light, the rest in HSL.

use crate::xml::Element;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: f32 }

impl Rgba {
    pub const BLACK: Rgba = Rgba { r: 0, g: 0, b: 0, a: 1.0 };
    pub const WHITE: Rgba = Rgba { r: 255, g: 255, b: 255, a: 1.0 };
    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba { Rgba { r, g, b, a: 1.0 } }
    pub fn hex(s: &str) -> Option<Rgba> {
        let s = s.trim_start_matches('#');
        let s = if s.len() == 8 { &s[2..] } else { s };
        if s.len() != 6 { return None; }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Rgba::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
    pub fn to_hex(&self) -> String { format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b) }
    pub fn with_alpha(self, a: f32) -> Rgba { Rgba { a, ..self } }
    fn unit(&self) -> [f64; 3] { [self.r as f64 / 255.0, self.g as f64 / 255.0, self.b as f64 / 255.0] }
    fn from_unit(c: [f64; 3], a: f32) -> Rgba {
        let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgba { r: q(c[0]), g: q(c[1]), b: q(c[2]), a }
    }
    /// Relative luminance (for choosing readable text on a fill).
    pub fn luminance(&self) -> f64 { let [r, g, b] = self.unit().map(to_linear); 0.2126 * r + 0.7152 * g + 0.0722 * b }
}

fn to_linear(c: f64) -> f64 { if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } }
fn to_srgb(c: f64) -> f64 { if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.max(0.0).powf(1.0 / 2.4) - 0.055 } }

fn rgb_to_hsl([r, g, b]: [f64; 3]) -> [f64; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min { return [0.0, 0.0, l]; }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r { (g - b) / d + if g < b { 6.0 } else { 0.0 } } else if max == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
    [h / 6.0, s, l]
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 0.5 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    if s == 0.0 { return [l, l, l]; }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    [hue_to_rgb(p, q, h + 1.0 / 3.0), hue_to_rgb(p, q, h), hue_to_rgb(p, q, h - 1.0 / 3.0)]
}

/// The theme's twelve colors by slot name (dk1, lt1, dk2, lt2, accent1–6, hlink, folHlink).
#[derive(Clone, Debug, Default)]
pub struct Scheme(pub HashMap<String, Rgba>);

impl Scheme {
    pub fn from_xml(clr_scheme: &Element) -> Scheme {
        let mut m = HashMap::new();
        for slot in clr_scheme.elements() {
            if let Some(c) = slot.first_element().and_then(|c| base_color(c, &Ctx::default())) { m.insert(slot.local().to_string(), c); }
        }
        Scheme(m)
    }
    pub fn get(&self, slot: &str) -> Option<Rgba> { self.0.get(slot).copied() }
}

/// The color map: slide tokens (bg1, tx1, …) to theme slots (lt1, dk1, …).
#[derive(Clone, Debug)]
pub struct ColorMap(pub HashMap<String, String>);

impl Default for ColorMap {
    fn default() -> Self {
        ColorMap([("bg1", "lt1"), ("tx1", "dk1"), ("bg2", "lt2"), ("tx2", "dk2")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect())
    }
}

impl ColorMap {
    pub fn from_xml(el: &Element) -> ColorMap {
        let mut m = ColorMap::default();
        for (k, v) in &el.attrs { m.0.insert(k.clone(), v.clone()); }
        m
    }
    /// A slide's override on top of the master's map.
    pub fn overlay(&self, el: Option<&Element>) -> ColorMap {
        match el.and_then(|o| o.child("a:overrideClrMapping")) { Some(o) => ColorMap::from_xml(o), None => self.clone() }
    }
}

/// What a color reference resolves against.
#[derive(Clone, Debug, Default)]
pub struct Ctx {
    pub scheme: Scheme,
    pub map: ColorMap,
    /// The placeholder color (phClr) from a style reference.
    pub ph: Option<Rgba>,
}

impl Ctx {
    pub fn scheme_color(&self, token: &str) -> Option<Rgba> {
        if token == "phClr" { return self.ph.or(self.scheme.get("dk1")); }
        let slot = self.map.0.get(token).map(String::as_str).unwrap_or(token);
        self.scheme.get(slot).or_else(|| match token {
            "tx1" | "dk1" => Some(Rgba::BLACK),
            "bg1" | "lt1" => Some(Rgba::WHITE),
            "tx2" | "dk2" => Some(Rgba::rgb(0x44, 0x54, 0x6A)),
            "bg2" | "lt2" => Some(Rgba::rgb(0xE7, 0xE6, 0xE6)),
            _ => None,
        })
    }
}

fn preset(name: &str) -> Option<Rgba> {
    let v: u32 = match name {
        "black" => 0x000000, "white" => 0xFFFFFF, "red" => 0xFF0000, "green" => 0x008000, "blue" => 0x0000FF, "yellow" => 0xFFFF00,
        "cyan" | "aqua" => 0x00FFFF, "magenta" | "fuchsia" => 0xFF00FF, "gray" | "grey" => 0x808080, "silver" => 0xC0C0C0,
        "maroon" => 0x800000, "navy" => 0x000080, "olive" => 0x808000, "purple" => 0x800080, "teal" => 0x008080, "lime" => 0x00FF00,
        "orange" => 0xFFA500, "pink" => 0xFFC0CB, "brown" => 0xA52A2A, "gold" => 0xFFD700, "ltGray" | "lightGray" => 0xD3D3D3,
        "dkGray" | "darkGray" => 0xA9A9A9, "dkBlue" | "darkBlue" => 0x00008B, "dkRed" | "darkRed" => 0x8B0000, "dkGreen" | "darkGreen" => 0x006400,
        "ltBlue" | "lightBlue" => 0xADD8E6, "violet" => 0xEE82EE, "indigo" => 0x4B0082, "coral" => 0xFF7F50, "salmon" => 0xFA8072,
        "tan" => 0xD2B48C, "beige" => 0xF5F5DC, "khaki" => 0xF0E68C, "crimson" => 0xDC143C, "skyBlue" => 0x87CEEB, "steelBlue" => 0x4682B4,
        "tomato" => 0xFF6347, "turquoise" => 0x40E0D0, "wheat" => 0xF5DEB3, "orchid" => 0xDA70D6, "plum" => 0xDDA0DD, "lavender" => 0xE6E6FA,
        _ => return None,
    };
    Some(Rgba::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn base_color(el: &Element, ctx: &Ctx) -> Option<Rgba> {
    match el.local() {
        "srgbClr" => el.attr("val").and_then(Rgba::hex),
        "sysClr" => el.attr("lastClr").and_then(Rgba::hex).or_else(|| match el.attr("val") { Some("window") => Some(Rgba::WHITE), _ => Some(Rgba::BLACK) }),
        "schemeClr" => ctx.scheme_color(el.attr("val")?),
        "prstClr" => preset(el.attr("val")?),
        "scrgbClr" => {
            let p = |k: &str| to_srgb(el.attr_f64(k).unwrap_or(0.0) / 100_000.0);
            Some(Rgba::from_unit([p("r"), p("g"), p("b")], 1.0))
        }
        "hslClr" => {
            let h = el.attr_f64("hue").unwrap_or(0.0) / 60_000.0 / 360.0;
            let s = el.attr_f64("sat").unwrap_or(0.0) / 100_000.0;
            let l = el.attr_f64("lum").unwrap_or(0.0) / 100_000.0;
            Some(Rgba::from_unit(hsl_to_rgb(h, s, l), 1.0))
        }
        _ => None,
    }
}

/// Resolve a color element (srgbClr, schemeClr, …) with its transforms.
pub fn resolve(el: &Element, ctx: &Ctx) -> Option<Rgba> {
    let base = base_color(el, ctx)?;
    let mut c = base.unit();
    let mut alpha: f64 = 1.0;
    for t in el.elements() {
        let v = t.attr_f64("val").unwrap_or(0.0) / 100_000.0;
        match t.local() {
            "alpha" => alpha = v,
            "alphaMod" => alpha *= v,
            "alphaOff" => alpha += v,
            "inv" => c = c.map(|x| to_srgb(1.0 - to_linear(x))),
            "gray" => { let y = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]; c = [y, y, y]; }
            "comp" => { let [h, s, l] = rgb_to_hsl(c); c = hsl_to_rgb((h + 0.5) % 1.0, s, l); }
            "shade" => c = c.map(|x| to_srgb(to_linear(x) * v)),
            "tint" => c = c.map(|x| to_srgb(to_linear(x) * v + (1.0 - v))),
            "lumMod" | "lumOff" => {
                let [h, s, l] = rgb_to_hsl(c);
                let nl = if t.local() == "lumMod" { l * v } else { l + v };
                c = hsl_to_rgb(h, s, nl.clamp(0.0, 1.0));
            }
            "satMod" | "satOff" => {
                let [h, s, l] = rgb_to_hsl(c);
                let ns = if t.local() == "satMod" { s * v } else { s + v };
                c = if l == 0.0 || l == 1.0 { [l, l, l] } else { hsl_to_rgb(h, ns.clamp(0.0, 1.0), l) }.map(|x| x.clamp(0.0, 1.0));
            }
            "hueMod" | "hueOff" => {
                let [h, s, l] = rgb_to_hsl(c);
                // hueOff is in 60000ths of a degree.
                let nh = if t.local() == "hueMod" { h * v } else { h + t.attr_f64("val").unwrap_or(0.0) / 60_000.0 / 360.0 };
                c = hsl_to_rgb(nh.rem_euclid(1.0), s, l);
            }
            "red" => c[0] = to_srgb(v), "green" => c[1] = to_srgb(v), "blue" => c[2] = to_srgb(v),
            "redMod" => c[0] = to_srgb(to_linear(c[0]) * v), "greenMod" => c[1] = to_srgb(to_linear(c[1]) * v), "blueMod" => c[2] = to_srgb(to_linear(c[2]) * v),
            "redOff" => c[0] = to_srgb(to_linear(c[0]) + v), "greenOff" => c[1] = to_srgb(to_linear(c[1]) + v), "blueOff" => c[2] = to_srgb(to_linear(c[2]) + v),
            "gamma" => c = c.map(to_srgb), "invGamma" => c = c.map(to_linear),
            _ => {}
        }
        alpha = alpha.clamp(0.0, 1.0);
    }
    Some(Rgba::from_unit(c, alpha as f32))
}

/// The color child of a fill-like element (solidFill, gs, …), resolved.
pub fn resolve_child(parent: &Element, ctx: &Ctx) -> Option<Rgba> {
    parent.elements().find(|e| matches!(e.local(), "srgbClr" | "schemeClr" | "sysClr" | "prstClr" | "scrgbClr" | "hslClr")).and_then(|e| resolve(e, ctx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(x: &str) -> Element { Element::parse(x).unwrap() }

    #[test]
    fn transforms() {
        let mut ctx = Ctx::default();
        ctx.scheme.0.insert("accent1".into(), Rgba::rgb(0x44, 0x72, 0xC4));
        ctx.scheme.0.insert("lt1".into(), Rgba::WHITE);
        assert_eq!(resolve(&el(r#"<a:schemeClr val="accent1"/>"#), &ctx), Some(Rgba::rgb(0x44, 0x72, 0xC4)));
        // 25% darker accent, as PowerPoint's palette writes it.
        let c = resolve(&el(r#"<a:schemeClr val="accent1"><a:lumMod val="75000"/></a:schemeClr>"#), &ctx).unwrap();
        assert_eq!(c.to_hex(), "2F5597");
        // bg1 maps to lt1.
        assert_eq!(resolve(&el(r#"<a:schemeClr val="bg1"><a:lumMod val="85000"/></a:schemeClr>"#), &ctx).unwrap().to_hex(), "D9D9D9");
        let a = resolve(&el(r#"<a:srgbClr val="FF0000"><a:alpha val="50000"/></a:srgbClr>"#), &ctx).unwrap();
        assert_eq!((a.r, a.a), (255, 0.5));
    }
}
