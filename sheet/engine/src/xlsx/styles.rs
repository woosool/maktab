//! styles.xml (xlsx-styles.js, xlsx-export-styles.js): cell formats read into interned `Style`s
//! (fonts, fills, borders, number formats, alignment, protection; colors resolved through the
//! palette and theme) and written back as deduplicated font/fill/border/numFmt tables.

use super::xml::{attr, Element};
use crate::style::{Border, HAlign, LineStyle, Rgb, Style, Underline, VAlign};
use std::collections::HashMap;

/// Colors the file's styles resolve against: the theme's twelve and the indexed palette.
pub struct Palette { pub theme: Vec<Rgb>, pub indexed: Vec<Rgb> }

impl Palette {
    pub fn new(theme: Vec<Rgb>, styles: Option<&Element>) -> Palette {
        let mut indexed: Vec<Rgb> = (0..64).map(|i| crate::numfmt::INDEXED[if i < 8 { i } else { i - 8 }]).collect();
        if let Some(list) = styles.and_then(|s| s.path("colors/indexedColors")) {
            for (i, c) in list.all("rgbColor").enumerate() { if let (Some(slot), Some(rgb)) = (indexed.get_mut(i), c.attr("rgb").and_then(hex)) { *slot = rgb; } }
        }
        Palette { theme, indexed }
    }
    /// A <color>/<fgColor>-like element; None for automatic (and the system colors 64/65).
    pub fn color(&self, e: &Element) -> Option<Rgb> {
        if e.flag("auto", false) { return None; }
        let base = if let Some(rgb) = e.attr("rgb") { hex(rgb)? }
            else if let Some(i) = e.attr_u32("indexed") { *self.indexed.get(i as usize)? }
            else if let Some(t) = e.attr_u32("theme") {
                // Index 0/1 and 2/3 name lt1/dk1 and lt2/dk2, the reverse of the scheme's order.
                let t = match t { 0 => 1, 1 => 0, 2 => 3, 3 => 2, t => t } as usize;
                *self.theme.get(t)?
            } else { return None };
        Some(match e.attr_f64("tint") { Some(t) if t != 0.0 && t.abs() <= 1.0 => tint(base, t), _ => base })
    }
    /// The theme color a <color>-like element names, if it names one.
    pub fn theme_ref(e: &Element) -> Option<crate::theme::ThemeColor> {
        if e.attr("rgb").is_some() || e.attr("indexed").is_some() { return None; }
        let t = e.attr_u32("theme")?;
        Some(crate::theme::ThemeColor::from_xlsx(t, e.attr_f64("tint").filter(|t| t.abs() <= 1.0).unwrap_or(0.0)))
    }
}

/// "FFRRGGBB" or "RRGGBB".
pub fn hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    let s = if s.len() == 8 { &s[2..] } else { s };
    if s.len() != 6 { return None; }
    let b = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

pub fn rgb_hex(c: Rgb) -> String { format!("FF{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }

/// Excel's tint: lighten (t > 0) or darken (t < 0) the color's HSL luminance.
pub fn tint(c: Rgb, t: f64) -> Rgb {
    let [r, g, b] = c.map(|x| x as f64 / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let mut l = (mx + mn) / 2.0;
    let d = mx - mn;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let h = if d == 0.0 { 0.0 } else if mx == r { ((g - b) / d).rem_euclid(6.0) } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 } * 60.0;
    l = if t < 0.0 { l * (1.0 + t) } else { l * (1.0 - t) + t };
    let c2 = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c2 * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c2 / 2.0;
    let (r1, g1, b1) = match (h / 60.0) as i32 { 0 => (c2, x, 0.0), 1 => (x, c2, 0.0), 2 => (0.0, c2, x), 3 => (0.0, x, c2), 4 => (x, 0.0, c2), _ => (c2, 0.0, x) };
    [r1, g1, b1].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// Locale-dependent built-in formats (5–8, 27–36, 50–58) as the en-US build shows them.
fn builtin_fmt(id: u32) -> Option<&'static str> {
    crate::numfmt::builtin(id).or_else(|| Some(match id {
        5 => r##""$"#,##0_);\("$"#,##0\)"##, 6 => r##""$"#,##0_);[Red]\("$"#,##0\)"##, 7 => r##""$"#,##0.00_);\("$"#,##0.00\)"##, 8 => r##""$"#,##0.00_);[Red]\("$"#,##0.00\)"##,
        27..=36 | 50..=58 => "m/d/yyyy",
        _ => return None,
    }))
}

#[derive(Default, Clone)]
struct Font { name: Option<String>, size: Option<u32>, bold: bool, italic: bool, underline: Underline, strike: bool, color: Option<Rgb>, script: i8, color_th: Option<crate::theme::ThemeColor>, scheme: u8 }

fn read_font(f: &Element, pal: &Palette) -> Font {
    let mut o = Font::default();
    for e in f.elements() {
        match e.local() {
            "name" | "rFont" => o.name = e.attr("val").map(String::from),
            "sz" => o.size = e.attr_f64("val").map(|v| (v * 10.0).round() as u32),
            "b" => o.bold = e.flag("val", true), "i" => o.italic = e.flag("val", true), "strike" => o.strike = e.flag("val", true),
            "u" => o.underline = match e.attr("val").unwrap_or("single") { "double" => Underline::Double, "singleAccounting" => Underline::SingleAccounting, "doubleAccounting" => Underline::DoubleAccounting, "none" => Underline::None, _ => Underline::Single },
            // The theme's text color (tx1) is what "automatic" shows.
            "color" => { let auto = e.attr("theme") == Some("1") && e.attr("tint").is_none(); o.color = if auto { None } else { pal.color(e) }; o.color_th = if auto { None } else { Palette::theme_ref(e) }; }
            "scheme" => o.scheme = match e.attr("val") { Some("minor") => 1, Some("major") => 2, _ => 0 },
            "vertAlign" => o.script = match e.attr("val") { Some("superscript") => 1, Some("subscript") => -1, _ => 0 },
            _ => {}
        }
    }
    o
}

fn read_fill_th(f: &Element) -> Option<crate::theme::ThemeColor> {
    let p = f.child("patternFill")?;
    if p.attr("patternType").unwrap_or("solid") == "none" { return None; }
    p.child("fgColor").and_then(Palette::theme_ref)
}

fn read_fill(f: &Element, pal: &Palette) -> Option<Rgb> {
    if let Some(p) = f.child("patternFill") {
        let kind = p.attr("patternType").unwrap_or(if p.child("fgColor").is_some() { "solid" } else { "none" });
        if kind == "none" { return None; }
        return p.child("fgColor").and_then(|c| pal.color(c)).or_else(|| p.child("bgColor").and_then(|c| pal.color(c)));
    }
    f.child("gradientFill").and_then(|g| g.child("stop")).and_then(|s| s.child("color")).and_then(|c| pal.color(c))
}

struct BorderSet { left: Border, right: Border, top: Border, bottom: Border, diagonal: Border, up: bool, down: bool }

fn read_border(b: &Element, pal: &Palette) -> BorderSet {
    let side = |n: &str| b.child(n).map(|e| Border { style: LineStyle::parse(e.attr("style").unwrap_or("none")), color: e.child("color").and_then(|c| pal.color(c)) }).filter(|x| x.style != LineStyle::None).unwrap_or_default();
    BorderSet { left: { let l = side("left"); if l.style == LineStyle::None { side("start") } else { l } }, right: { let r = side("right"); if r.style == LineStyle::None { side("end") } else { r } },
        top: side("top"), bottom: side("bottom"), diagonal: side("diagonal"), up: b.flag("diagonalUp", false), down: b.flag("diagonalDown", false) }
}

/// Every cellXfs entry as a Style (index = the file's style id), plus the Normal style.
pub fn read(styles: &Element, pal: &Palette) -> (Vec<Style>, Style) {
    let mut fmts: HashMap<u32, String> = HashMap::new();
    if let Some(n) = styles.child("numFmts") { for f in n.all("numFmt") { if let (Some(id), Some(code)) = (f.attr_u32("numFmtId"), f.attr("formatCode")) { fmts.insert(id, code.to_string()); } } }
    let fonts: Vec<Font> = styles.child("fonts").map(|f| f.all("font").map(|x| read_font(x, pal)).collect()).unwrap_or_default();
    let fills: Vec<Option<Rgb>> = styles.child("fills").map(|f| f.all("fill").map(|x| read_fill(x, pal)).collect()).unwrap_or_default();
    let fill_ths: Vec<Option<crate::theme::ThemeColor>> = styles.child("fills").map(|f| f.all("fill").map(read_fill_th).collect()).unwrap_or_default();
    let borders: Vec<BorderSet> = styles.child("borders").map(|f| f.all("border").map(|x| read_border(x, pal)).collect()).unwrap_or_default();
    let base_font = fonts.first().cloned().unwrap_or_default();
    let xf = |x: &Element| -> Style {
        let mut s = Style::default();
        let f = x.attr_u32("fontId").and_then(|i| fonts.get(i as usize)).unwrap_or(&base_font);
        s.font = f.name.clone().or(base_font.name.clone()).unwrap_or_else(|| crate::style::DEFAULT_FONT.into());
        s.size = f.size.or(base_font.size).unwrap_or(110);
        (s.bold, s.italic, s.underline, s.strike, s.color, s.script) = (f.bold, f.italic, f.underline, f.strike, f.color, f.script);
        s.fill = x.attr_u32("fillId").and_then(|i| fills.get(i as usize)).copied().flatten();
        s.fill_th = x.attr_u32("fillId").and_then(|i| fill_ths.get(i as usize)).copied().flatten().filter(|_| s.fill.is_some());
        (s.color_th, s.font_scheme) = (f.color_th.filter(|_| s.color.is_some()), if f.name.is_some() { f.scheme } else { base_font.scheme });
        if let Some(b) = x.attr_u32("borderId").and_then(|i| borders.get(i as usize)) {
            (s.left, s.right, s.top, s.bottom, s.diagonal, s.diag_up, s.diag_down) = (b.left, b.right, b.top, b.bottom, b.diagonal, b.up, b.down);
        }
        let id = x.attr_u32("numFmtId").unwrap_or(0);
        s.num_fmt = fmts.get(&id).cloned().or_else(|| builtin_fmt(id).map(String::from)).unwrap_or_else(|| "General".into());
        if let Some(a) = x.child("alignment") {
            s.h = match a.attr("horizontal").unwrap_or("") { "left" => HAlign::Left, "center" => HAlign::Center, "right" => HAlign::Right, "fill" => HAlign::Fill, "justify" => HAlign::Justify, "centerContinuous" => HAlign::CenterAcross, "distributed" => HAlign::Distributed, _ => HAlign::General };
            s.v = match a.attr("vertical").unwrap_or("") { "top" => VAlign::Top, "center" => VAlign::Center, "justify" => VAlign::Justify, "distributed" => VAlign::Distributed, _ => VAlign::Bottom };
            s.wrap = a.flag("wrapText", false); s.shrink = a.flag("shrinkToFit", false);
            s.indent = a.attr_u32("indent").unwrap_or(0).min(250) as u8;
            // 91–180 are downward angles (−1…−90); 255 is stacked.
            s.rotation = match a.attr_u32("textRotation").unwrap_or(0) { r @ 0..=90 => r as i16, r @ 91..=180 => 90 - r as i16, _ => 255 };
            s.reading = a.attr_u32("readingOrder").unwrap_or(0).min(2) as u8;
        }
        if let Some(p) = x.child("protection") { s.locked = p.flag("locked", true); s.hidden = p.flag("hidden", false); }
        s
    };
    let list: Vec<Style> = styles.child("cellXfs").map(|c| c.all("xf").map(xf).collect()).unwrap_or_default();
    // Normal: the cell style xf of the builtin style 0, which blank cells show.
    let normal = styles.child("cellStyleXfs").and_then(|c| {
        let id = styles.child("cellStyles").and_then(|cs| cs.all("cellStyle").find(|s| s.attr("builtinId") == Some("0"))).and_then(|s| s.attr_u32("xfId")).unwrap_or(0);
        c.all("xf").nth(id as usize)
    }).map(xf).unwrap_or_else(|| list.first().cloned().unwrap_or_default());
    (list, normal)
}

// ---- writing ----

fn color_xml(tag: &str, c: Option<Rgb>, auto: bool) -> String {
    match c { Some(c) => format!("<{tag} rgb=\"{}\"/>", rgb_hex(c)), None if auto => format!("<{tag} theme=\"1\"/>"), None => String::new() }
}

/// What theme references are written against: the theme's colors and its fonts.
pub struct ThemeCtx<'a> { pub colors: &'a [Rgb], pub major: &'a str, pub minor: &'a str }

fn theme_color_xml(tag: &str, t: crate::theme::ThemeColor) -> String {
    if t.tint == 0 { format!("<{tag} theme=\"{}\"/>", t.xlsx_index()) } else { format!("<{tag} theme=\"{}\" tint=\"{}\"/>", t.xlsx_index(), t.tint_f()) }
}

fn font_xml(s: &Style, th: &ThemeCtx) -> String {
    let mut o = String::from("<font>");
    if s.bold { o.push_str("<b/>"); } if s.italic { o.push_str("<i/>"); } if s.strike { o.push_str("<strike/>"); }
    match s.underline { Underline::None => {}, Underline::Single => o.push_str("<u/>"), Underline::Double => o.push_str("<u val=\"double\"/>"), Underline::SingleAccounting => o.push_str("<u val=\"singleAccounting\"/>"), Underline::DoubleAccounting => o.push_str("<u val=\"doubleAccounting\"/>") }
    if s.script != 0 { o.push_str(&format!("<vertAlign val=\"{}\"/>", if s.script > 0 { "superscript" } else { "subscript" })); }
    o.push_str(&format!("<sz val=\"{}\"/>", crate::value::fmt_num(s.size as f64 / 10.0)));
    match (s.color, s.color_th) {
        (Some(c), Some(t)) if t.resolve(th.colors) == c => o.push_str(&theme_color_xml("color", t)),
        _ => o.push_str(&color_xml("color", s.color, true)),
    }
    o.push_str(&format!("<name val=\"{}\"/><family val=\"2\"/>", attr(&s.font)));
    let scheme = match s.font_scheme { 1 if s.font == th.minor => "minor", 2 if s.font == th.major => "major", _ => "" };
    if !scheme.is_empty() { o.push_str(&format!("<scheme val=\"{scheme}\"/>")); }
    o.push_str("</font>");
    o
}

fn fill_xml_th(c: Rgb, t: Option<crate::theme::ThemeColor>, th: &ThemeCtx) -> String {
    match t { Some(t) if t.resolve(th.colors) == c => format!("<fill><patternFill patternType=\"solid\">{}<bgColor indexed=\"64\"/></patternFill></fill>", theme_color_xml("fgColor", t)), _ => fill_xml(c) }
}

fn fill_xml(c: Rgb) -> String { format!("<fill><patternFill patternType=\"solid\"><fgColor rgb=\"{}\"/><bgColor indexed=\"64\"/></patternFill></fill>", rgb_hex(c)) }

fn border_xml(s: &Style) -> String {
    let mut o = String::from("<border");
    if s.diag_up { o.push_str(" diagonalUp=\"1\""); } if s.diag_down { o.push_str(" diagonalDown=\"1\""); }
    o.push('>');
    for (n, b) in [("left", &s.left), ("right", &s.right), ("top", &s.top), ("bottom", &s.bottom), ("diagonal", &s.diagonal)] {
        if b.style == LineStyle::None { o.push_str(&format!("<{n}/>")); continue; }
        let c = match b.color { Some(c) => format!("<color rgb=\"{}\"/>", rgb_hex(c)), None => "<color indexed=\"64\"/>".into() };
        o.push_str(&format!("<{n} style=\"{}\">{c}</{n}>", b.style.name()));
    }
    o.push_str("</border>");
    o
}

/// styles.xml for the workbook's style table: cellXfs[i] is style i. `dxfs` and the like from
/// the file that was read (conditional formats refer to them by index) are carried over.
pub fn write(styles: &[Style], original: Option<&Element>, dxfs: &[String], th: &ThemeCtx) -> String {
    fn idx(map: &mut HashMap<String, usize>, list: &mut Vec<String>, x: String) -> usize {
        if let Some(&i) = map.get(&x) { return i; }
        list.push(x.clone()); map.insert(x, list.len() - 1); list.len() - 1
    }
    let (mut fonts, mut fills, mut borders) = (vec![], vec!["<fill><patternFill patternType=\"none\"/></fill>".to_string(), "<fill><patternFill patternType=\"gray125\"/></fill>".to_string()], vec![]);
    let (mut fm, mut fl, mut bm) = (HashMap::new(), HashMap::new(), HashMap::new());
    for (i, f) in fills.iter().enumerate() { fl.insert(f.clone(), i); }
    let mut numfmts: Vec<(u32, String)> = vec![];
    let mut xfs = String::new();
    let base = styles.first().cloned().unwrap_or_default();
    idx(&mut fm, &mut fonts, font_xml(&base, th));
    idx(&mut bm, &mut borders, border_xml(&Style::default()));
    for s in styles {
        let font = idx(&mut fm, &mut fonts, font_xml(s, th));
        let fill = s.fill.map_or(0, |c| idx(&mut fl, &mut fills, fill_xml_th(c, s.fill_th, th)));
        let border = idx(&mut bm, &mut borders, border_xml(s));
        let nf = (0..50).find(|&id| crate::numfmt::builtin(id) == Some(s.num_fmt.as_str())).unwrap_or_else(|| {
            if let Some((id, _)) = numfmts.iter().find(|(_, c)| *c == s.num_fmt) { return *id; }
            let id = 164 + numfmts.len() as u32; numfmts.push((id, s.num_fmt.clone())); id
        });
        xfs.push_str(&format!("<xf numFmtId=\"{nf}\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" xfId=\"0\""));
        if nf != 0 { xfs.push_str(" applyNumberFormat=\"1\""); } if font != 0 { xfs.push_str(" applyFont=\"1\""); } if fill != 0 { xfs.push_str(" applyFill=\"1\""); } if border != 0 { xfs.push_str(" applyBorder=\"1\""); }
        let mut al = String::new();
        let h = match s.h { HAlign::General => "", HAlign::Left => "left", HAlign::Center => "center", HAlign::Right => "right", HAlign::Fill => "fill", HAlign::Justify => "justify", HAlign::CenterAcross => "centerContinuous", HAlign::Distributed => "distributed" };
        if !h.is_empty() { al.push_str(&format!(" horizontal=\"{h}\"")); }
        let v = match s.v { VAlign::Bottom => "", VAlign::Top => "top", VAlign::Center => "center", VAlign::Justify => "justify", VAlign::Distributed => "distributed" };
        if !v.is_empty() { al.push_str(&format!(" vertical=\"{v}\"")); }
        if s.rotation != 0 { al.push_str(&format!(" textRotation=\"{}\"", match s.rotation { 255 => 255, r if r < 0 => 90 - r as i32, r => r as i32 })); }
        if s.wrap { al.push_str(" wrapText=\"1\""); } if s.indent > 0 { al.push_str(&format!(" indent=\"{}\"", s.indent)); }
        if s.shrink { al.push_str(" shrinkToFit=\"1\""); } if s.reading > 0 { al.push_str(&format!(" readingOrder=\"{}\"", s.reading)); }
        let prot = !s.locked || s.hidden;
        if !al.is_empty() { xfs.push_str(" applyAlignment=\"1\""); } if prot { xfs.push_str(" applyProtection=\"1\""); }
        if al.is_empty() && !prot { xfs.push_str("/>"); continue; }
        xfs.push('>');
        if !al.is_empty() { xfs.push_str(&format!("<alignment{al}/>")); }
        if prot { xfs.push_str(&format!("<protection locked=\"{}\" hidden=\"{}\"/>", s.locked as u8, s.hidden as u8)); }
        xfs.push_str("</xf>");
    }
    let mut o = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#);
    if !numfmts.is_empty() {
        o.push_str(&format!("<numFmts count=\"{}\">", numfmts.len()));
        for (id, c) in &numfmts { o.push_str(&format!("<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>", attr(c))); }
        o.push_str("</numFmts>");
    }
    o.push_str(&format!("<fonts count=\"{}\">{}</fonts>", fonts.len(), fonts.concat()));
    o.push_str(&format!("<fills count=\"{}\">{}</fills>", fills.len(), fills.concat()));
    o.push_str(&format!("<borders count=\"{}\">{}</borders>", borders.len(), borders.concat()));
    o.push_str("<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>");
    o.push_str(&format!("<cellXfs count=\"{}\">{xfs}</cellXfs>", styles.len().max(1)));
    if styles.is_empty() { o = o.replace("<cellXfs count=\"1\"></cellXfs>", "<cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs>"); }
    o.push_str("<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>");
    // Carried from the file read: differential formats, table styles and the custom palette.
    for name in ["dxfs", "tableStyles", "colors"] {
        if name == "dxfs" { o.push_str(&super::rules::dxfs_xml(dxfs)); continue; }
        if let Some(e) = original.and_then(|s| s.child(name)) { o.push_str(&strip_prefix(e).to_xml()); }
    }
    o.push_str("</styleSheet>");
    o
}

/// The element with the default namespace's prefix (x:font) removed, for re-emitting.
pub fn strip_prefix(e: &Element) -> Element {
    let pre = e.name.split_once(':').map(|p| p.0.to_string());
    fn walk(e: &Element, pre: &Option<String>) -> Element {
        let name = match pre { Some(p) => e.name.strip_prefix(&format!("{p}:")).unwrap_or(&e.name).to_string(), None => e.name.clone() };
        Element { name, attrs: e.attrs.clone(), children: e.children.iter().map(|c| match c { super::xml::Node::Element(x) => super::xml::Node::Element(walk(x, pre)), t => t.clone() }).collect() }
    }
    walk(e, &pre)
}
