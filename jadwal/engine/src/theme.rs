//! The workbook theme (Page Layout ▸ Themes): its twelve colors, its heading and body fonts and
//! its effects, as `xl/theme/theme1.xml` holds them. Cell styles can refer to theme colors (with
//! a tint) and to the theme fonts, so applying another theme recolors and refonts them; the
//! built-in galleries (themes, color palettes, font pairs, effects) and .thmx files.

use crate::book::Workbook;
use crate::style::Rgb;
use crate::xlsx::xml::{attr, part_text, Element, Node};

/// A reference to a theme color: the slot in scheme order (0 dk1, 1 lt1, 2 dk2, 3 lt2,
/// 4–9 accent1–6, 10 hlink, 11 folHlink) and a tint in 1/100000 (−100000 darkest … 100000 lightest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ThemeColor { pub idx: u8, pub tint: i32 }

impl ThemeColor {
    pub fn new(idx: u8, tint: f64) -> ThemeColor { ThemeColor { idx, tint: (tint * 100000.0).round() as i32 } }
    pub fn tint_f(&self) -> f64 { self.tint as f64 / 100000.0 }
    /// Its color in a theme's twelve.
    pub fn resolve(&self, theme: &[Rgb]) -> Rgb {
        let base = theme.get(self.idx as usize).copied().unwrap_or(crate::numfmt::OFFICE_THEME[(self.idx as usize).min(11)]);
        if self.tint == 0 { base } else { tint(base, self.tint_f()) }
    }
    /// The index styles.xml uses (lt1/dk1 and lt2/dk2 swapped).
    pub fn xlsx_index(&self) -> u32 { match self.idx { 0 => 1, 1 => 0, 2 => 3, 3 => 2, i => i as u32 } }
    pub fn from_xlsx(i: u32, tint: f64) -> ThemeColor { ThemeColor::new(match i { 0 => 1, 1 => 0, 2 => 3, 3 => 2, i => i.min(11) } as u8, tint) }
}

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

/// The tints of the color picker's theme columns, top to bottom below the base color (Excel's).
pub fn picker_tints(base: Rgb) -> [f64; 5] {
    let l = (base.iter().copied().max().unwrap() as f64 + base.iter().copied().min().unwrap() as f64) / 510.0;
    if l < 0.05 { [0.5, 0.35, 0.25, 0.15, 0.05] }
    else if l > 0.95 { [-0.05, -0.15, -0.25, -0.35, -0.5] }
    else if l < 0.2 { [0.9, 0.75, 0.5, 0.25, 0.1] }
    else if l > 0.8 { [-0.1, -0.25, -0.5, -0.75, -0.9] }
    else { [0.8, 0.6, 0.4, -0.25, -0.5] }
}

/// The picker's ten columns in Excel's order: lt1, dk1, lt2, dk2, accent1–6.
pub const PICKER_SLOTS: [u8; 10] = [1, 0, 3, 2, 4, 5, 6, 7, 8, 9];

/// Theme effects: the fmtScheme's fill, line and effect style lists (inner XML, `a:` prefix).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Effects { pub name: String, pub fills: String, pub lines: String, pub effects: String }

/// What the theme part says beyond its colors.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub colors_name: String,
    pub fonts_name: String,
    /// Headings and body fonts (latin).
    pub major: String,
    pub minor: String,
    pub effects_name: String,
    /// Effects chosen here (None: the part's own, or Office's).
    pub effects: Option<Effects>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme { name: "Office Theme".into(), colors_name: "Office".into(), fonts_name: "Office".into(), major: "Calibri Light".into(), minor: crate::style::DEFAULT_FONT.into(), effects_name: "Office".into(), effects: None }
    }
}

// ---- the built-in galleries -------------------------------------------------------------------

fn hx(s: &str) -> Rgb {
    let s = s.trim_start_matches('#');
    let b = |i: usize| u8::from_str_radix(s.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    [b(0), b(2), b(4)]
}

/// A named set of twelve colors.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette { pub name: String, pub colors: [Rgb; 12] }

/// A built-in theme: colors, fonts and (optionally) effects.
#[derive(Clone, Debug, PartialEq)]
pub struct Builtin { pub name: String, pub colors: Palette, pub major: String, pub minor: String, pub effects: Option<Effects> }

fn pal(name: &str, dk2: &str, lt2: &str, acc: [&str; 6], hl: &str, fh: &str) -> Palette {
    let mut c = [[0, 0, 0], [255, 255, 255], hx(dk2), hx(lt2), [0; 3], [0; 3], [0; 3], [0; 3], [0; 3], [0; 3], hx(hl), hx(fh)];
    for (i, a) in acc.iter().enumerate() { c[4 + i] = hx(a); }
    Palette { name: name.into(), colors: c }
}

/// Excel's color palettes (Colors ▾), then the shared catalog's schemes.
pub fn palettes() -> Vec<Palette> {
    let mut v = vec![
        Palette { name: "Office".into(), colors: crate::numfmt::OFFICE_THEME },
        pal("Office 2013 - 2022", "44546A", "E7E6E6", ["4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5", "70AD47"], "0563C1", "954F72"),
        pal("Office 2007 - 2010", "1F497D", "EEECE1", ["4F81BD", "C0504D", "9BBB59", "8064A2", "4BACC6", "F79646"], "0000FF", "800080"),
        pal("Grayscale", "000000", "F8F8F8", ["DDDDDD", "B2B2B2", "969696", "808080", "5F5F5F", "4D4D4D"], "5F5F5F", "919191"),
        pal("Blue Warm", "242852", "ACCBF9", ["4A66AC", "629DD1", "297FD5", "7F8FA9", "5AA2AE", "9D90A0"], "9454C3", "3EBBF0"),
        pal("Blue", "17406D", "DBEFF9", ["0F6FC6", "009DD9", "0BD0D9", "10CF9B", "7CCA62", "A5C249"], "F49100", "85DFD0"),
        pal("Blue Green", "373545", "CEDBE6", ["3494BA", "58B6C0", "75BDA7", "7A8C8E", "84ACB6", "2683C6"], "6B9F25", "9F6715"),
        pal("Green", "455F51", "E3DED1", ["549E39", "8AB833", "C0CF3A", "029676", "4AB5C4", "0989B1"], "6B9F25", "BA6906"),
        pal("Green Yellow", "455F51", "E3DED1", ["99CB38", "63A537", "37A76F", "44C1A3", "4EB3CF", "51C3F9"], "EE7B08", "977B2D"),
        pal("Yellow", "39302A", "E5DEDB", ["FFCA08", "F8931D", "CE8D3E", "EC7016", "E64823", "9C6A6A"], "2998E3", "7F723D"),
        pal("Yellow Orange", "4E3B30", "FBEEC9", ["F0A22E", "A5644E", "B58B80", "C3986D", "A19574", "C17529"], "AD1F1F", "FFC42F"),
        pal("Orange", "637052", "CCDDEA", ["E48312", "BD582C", "865640", "9B8357", "C2BC80", "94A088"], "2998E3", "8C8C8C"),
        pal("Orange Red", "696464", "E9E5DC", ["D34817", "9B2D1F", "A28E6A", "956251", "918485", "855D5D"], "CC9900", "96A9A9"),
        pal("Red Orange", "505046", "EEECE1", ["E84C22", "FFBD47", "B64926", "FF8427", "CC9900", "B22600"], "CC9900", "666699"),
        pal("Red", "323232", "E5C243", ["A5300F", "D55816", "E19825", "B19C7D", "7F5F52", "B27D49"], "6B9F25", "B26B02"),
        pal("Red Violet", "454551", "D8D9DC", ["E32D91", "C830CC", "4EA6DC", "4775E7", "8971E1", "D54773"], "6B9F25", "8C8C8C"),
        pal("Violet", "373545", "DCD8DC", ["AD84C6", "8784C7", "5D739A", "6997AF", "84ACB6", "6F8183"], "69A020", "8C8C8C"),
        pal("Violet II", "632E62", "EAE5EB", ["92278F", "9B57D3", "755DD9", "665EB8", "45A5ED", "5982DB"], "0066FF", "666699"),
        pal("Median", "775F55", "EBDDC3", ["94B6D2", "DD8047", "A5AB81", "D8B25C", "7BA79D", "968C8C"], "F7B615", "704404"),
        pal("Paper", "444D26", "FEFAC9", ["A5B592", "F3A447", "E7BC29", "D092A7", "9C85C0", "809EC2"], "8E58B6", "7F6F6F"),
        pal("Marquee", "5E5E5E", "DDDDDD", ["418AB3", "A6B727", "F69200", "838383", "FEC306", "DF5327"], "F59E00", "B2B2B2"),
        pal("Slipstream", "212745", "B4DCFA", ["4E67C8", "5ECCF3", "A7EA52", "5DCEAF", "FF8021", "F14124"], "56C7AA", "59A8D1"),
        pal("Aspect", "323232", "E3DED1", ["F07F09", "9F2936", "1B587C", "4E8542", "604878", "C19859"], "6B9F25", "B26B02"),
    ];
    for s in &slides_engine::design::catalog().color_schemes {
        if v.iter().any(|p| p.name == s.name) { continue; }
        let mut c = crate::numfmt::OFFICE_THEME;
        let slots = ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];
        for (k, hex) in &s.colors { if let Some(i) = slots.iter().position(|x| x == k) { c[i] = hx(hex); } }
        v.push(Palette { name: s.name.clone(), colors: c });
    }
    v
}

/// Font pairs (Fonts ▾): name, headings, body.
pub fn font_pairs() -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> = [
        ("Office", "Calibri Light", "Calibri"), ("Office 2023", "Aptos Display", "Aptos"), ("Office 2007 - 2010", "Cambria", "Calibri"), ("Calibri", "Calibri", "Calibri"),
        ("Arial", "Arial", "Arial"), ("Corbel", "Corbel", "Corbel"), ("Candara", "Candara", "Candara"), ("Franklin Gothic", "Franklin Gothic Medium", "Franklin Gothic Book"),
        ("Century Gothic", "Century Gothic", "Century Gothic"), ("Tw Cen MT", "Tw Cen MT", "Tw Cen MT"), ("Cambria", "Cambria", "Cambria"), ("Garamond", "Garamond", "Garamond"),
        ("Georgia", "Georgia", "Georgia"), ("Trebuchet MS", "Trebuchet MS", "Trebuchet MS"), ("Times New Roman-Arial", "Times New Roman", "Arial"), ("Verdana", "Verdana", "Verdana"),
    ].iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect();
    for p in &slides_engine::design::catalog().font_pairs { if !v.iter().any(|x| x.0 == p.0) { v.push(p.clone()); } }
    v
}

/// Effects (Effects ▾): Office first (no explicit effects), then the catalog's.
pub fn effects() -> Vec<Effects> {
    let mut v = vec![Effects { name: "Office".into(), ..Default::default() }];
    for e in &slides_engine::design::catalog().effects { v.push(Effects { name: e.name.clone(), fills: e.fills.clone(), lines: e.lines.clone(), effects: e.effects.clone() }); }
    v
}

/// The Themes gallery: Office, the earlier Office themes, then the catalog's designed themes
/// (the same ones Sard's Design tab offers).
pub fn builtins() -> Vec<Builtin> {
    let p = palettes();
    let get = |n: &str| p.iter().find(|x| x.name == n).cloned().unwrap();
    let mut v = vec![
        Builtin { name: "Office Theme".into(), colors: get("Office"), major: "Calibri Light".into(), minor: "Calibri".into(), effects: None },
        Builtin { name: "Office 2013 - 2022 Theme".into(), colors: get("Office 2013 - 2022"), major: "Calibri Light".into(), minor: "Calibri".into(), effects: None },
        Builtin { name: "Office 2007 - 2010 Theme".into(), colors: get("Office 2007 - 2010"), major: "Cambria".into(), minor: "Calibri".into(), effects: None },
    ];
    let ef = effects();
    for (k, t) in slides_engine::design::catalog().themes.iter().enumerate() {
        let Some(var) = t.variants.first() else { continue };
        let mut c = crate::numfmt::OFFICE_THEME;
        let slots = ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];
        for (key, hex) in &var.colors { if let Some(i) = slots.iter().position(|x| x == key) { c[i] = hx(hex); } }
        // Each designed theme gets one of the effect schemes, in turn.
        let e = ef.get(k % ef.len().max(1)).filter(|e| !e.fills.is_empty()).cloned();
        v.push(Builtin { name: t.name.clone(), colors: Palette { name: var.name.clone(), colors: c }, major: t.fonts.0.clone(), minor: t.fonts.1.clone(), effects: e });
    }
    v
}

// ---- applying ---------------------------------------------------------------------------------

/// The workbook's styles with theme colors and fonts re-resolved: `f` maps each style.
fn restyle(wb: &mut Workbook, f: impl Fn(&mut crate::style::Style)) {
    let mut list = wb.styles.all().to_vec();
    for s in &mut list { f(s); }
    wb.styles.replace_all(list);
}

/// New theme colors: cells, borders and shapes that use theme colors follow.
pub fn apply_colors(wb: &mut Workbook, p: &Palette) {
    let old = wb.theme_colors.clone();
    let new = p.colors.to_vec();
    let follow = |c: &mut Option<Rgb>, t: &mut Option<ThemeColor>| {
        if let Some(r) = *t {
            if c.is_some_and(|x| x == r.resolve(&old)) { *c = Some(r.resolve(&new)); } else { *t = None; }
        }
    };
    wb.styles.set_theme(new.clone(), wb.theme.major.clone(), wb.theme.minor.clone());
    restyle(wb, |s| { follow(&mut s.color, &mut s.color_th); follow(&mut s.fill, &mut s.fill_th); });
    // Shapes keep plain colors: those that were exactly a theme accent take the new accent.
    fn walk(objs: &mut [crate::drawing::Object], old: &[Rgb], new: &[Rgb]) {
        for o in objs {
            match &mut o.kind {
                crate::drawing::ObjKind::Shape(s) => {
                    let mut changed = false;
                    for c in [&mut s.fill, &mut s.line, &mut s.color].into_iter().flatten() {
                        if let Some(k) = (2..12).find(|&k| old.get(k) == Some(c)) { if let Some(n) = new.get(k) { *c = *n; changed = true; } }
                    }
                    if changed { o.raw = None; }
                }
                crate::drawing::ObjKind::Group(g) => walk(&mut g.children, old, new),
                _ => {}
            }
        }
    }
    for sh in &mut wb.sheets { walk(&mut sh.objects, &old, &new); }
    wb.theme_colors = new;
    wb.theme.colors_name = p.name.clone();
    store(wb);
}

/// New theme fonts: cells in the body (or heading) font follow.
pub fn apply_fonts(wb: &mut Workbook, name: &str, major: &str, minor: &str) {
    let (old_major, old_minor) = (wb.theme.major.clone(), wb.theme.minor.clone());
    let (major_s, minor_s) = (major.to_string(), minor.to_string());
    wb.styles.set_theme(wb.theme_colors.clone(), major_s.clone(), minor_s.clone());
    restyle(wb, |s| {
        match s.font_scheme {
            1 if s.font == old_minor => s.font = minor_s.clone(),
            2 if s.font == old_major => s.font = major_s.clone(),
            0 => {}
            _ => s.font_scheme = 0,
        }
    });
    wb.theme.major = major.into();
    wb.theme.minor = minor.into();
    wb.theme.fonts_name = name.into();
    store(wb);
}

pub fn apply_effects(wb: &mut Workbook, e: &Effects) {
    wb.theme.effects_name = e.name.clone();
    wb.theme.effects = if e.fills.is_empty() { None } else { Some(e.clone()) };
    store(wb);
}

/// A whole theme: colors, fonts and effects.
pub fn apply(wb: &mut Workbook, t: &Builtin) {
    apply_colors(wb, &t.colors);
    apply_fonts(wb, &t.name, &t.major, &t.minor);
    apply_effects(wb, &t.effects.clone().unwrap_or(Effects { name: "Office".into(), ..Default::default() }));
    wb.theme.name = t.name.clone();
    store(wb);
}

/// The theme part brought in line with the model (the file's part patched, so what the model
/// doesn't hold — script fonts, background fills, object defaults — is kept).
fn store(wb: &mut Workbook) {
    let key = "xl/theme/theme1.xml";
    let patched = wb.parts.get(key).and_then(|b| Element::parse(&part_text(b)).ok()).and_then(|mut root| { patch(&mut root, wb).then_some(root) });
    let xml = match patched { Some(r) => format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n{}", r.to_xml()), None => theme_xml(wb) };
    wb.parts.insert(key.into(), xml.into_bytes());
}

fn child_mut<'a>(e: &'a mut Element, l: &str) -> Option<&'a mut Element> {
    e.children.iter_mut().find_map(|n| match n { Node::Element(x) if x.local() == l => Some(x), _ => None })
}

const SLOTS: [&str; 12] = ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];

fn hexs(c: Rgb) -> String { format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }

fn patch(root: &mut Element, wb: &Workbook) -> bool {
    if root.local() != "theme" { return false; }
    let pre = root.name.split_once(':').map(|p| format!("{}:", p.0)).unwrap_or_default();
    root.set_attr("name", wb.theme.name.clone());
    let Some(te) = child_mut(root, "themeElements") else { return false };
    let Some(cs) = child_mut(te, "clrScheme") else { return false };
    cs.set_attr("name", wb.theme.colors_name.clone());
    cs.children = SLOTS.iter().enumerate().map(|(i, s)| {
        let c = wb.theme_colors.get(i).copied().unwrap_or(crate::numfmt::OFFICE_THEME[i]);
        let inner = if i < 2 && c == [[0, 0, 0], [255, 255, 255]][i] {
            Element { name: format!("{pre}sysClr"), attrs: vec![("val".into(), if i == 0 { "windowText" } else { "window" }.into()), ("lastClr".into(), hexs(c))], children: vec![] }
        } else { Element { name: format!("{pre}srgbClr"), attrs: vec![("val".into(), hexs(c))], children: vec![] } };
        Node::Element(Element { name: format!("{pre}{s}"), attrs: vec![], children: vec![Node::Element(inner)] })
    }).collect();
    if let Some(fs) = child_mut(te, "fontScheme") {
        fs.set_attr("name", wb.theme.fonts_name.clone());
        for (which, face) in [("majorFont", &wb.theme.major), ("minorFont", &wb.theme.minor)] {
            if let Some(f) = child_mut(fs, which) {
                match child_mut(f, "latin") {
                    Some(l) => { l.attrs.retain(|(k, _)| k != "panose"); l.set_attr("typeface", face.clone()); }
                    None => f.children.insert(0, Node::Element(Element { name: format!("{pre}latin"), attrs: vec![("typeface".into(), face.clone())], children: vec![] })),
                }
            }
        }
    }
    if let Some(fm) = child_mut(te, "fmtScheme") {
        fm.set_attr("name", wb.theme.effects_name.clone());
        if let Some(e) = &wb.theme.effects {
            for (tag, inner) in [("fillStyleLst", &e.fills), ("lnStyleLst", &e.lines), ("effectStyleLst", &e.effects)] {
                let Ok(mut el) = Element::parse(&format!("<a:{tag} xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">{inner}</a:{tag}>")) else { continue };
                el.attrs.retain(|(k, _)| k != "xmlns:a");
                if let Some(slot) = fm.children.iter_mut().find(|n| matches!(n, Node::Element(x) if x.local() == tag)) { *slot = Node::Element(el); }
            }
        }
    }
    true
}

/// The theme part to save: the stored one while it agrees with the model, else brought in line.
pub fn part_for(wb: &Workbook) -> Vec<u8> {
    let key = "xl/theme/theme1.xml";
    let Some(b) = wb.parts.get(key) else { return theme_xml(wb).into_bytes() };
    let text = part_text(b);
    let same = parse(&text).is_some_and(|t| t.name == wb.theme.name && t.major == wb.theme.major && t.minor == wb.theme.minor && t.colors_name == wb.theme.colors_name && t.effects_name == wb.theme.effects_name)
        && parse_colors(&text).is_some_and(|c| c.as_slice() == wb.theme_colors.as_slice());
    if same { return b.clone(); }
    match Element::parse(&text).ok().and_then(|mut root| patch(&mut root, wb).then_some(root)) {
        Some(root) => format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n{}", root.to_xml()).into_bytes(),
        None => theme_xml(wb).into_bytes(),
    }
}

/// A complete theme part from the model.
pub fn theme_xml(wb: &Workbook) -> String {
    let c = &wb.theme_colors;
    let hex = |i: usize| hexs(c.get(i).copied().unwrap_or(crate::numfmt::OFFICE_THEME[i]));
    let mut clr = String::new();
    for (i, n) in SLOTS.iter().enumerate() {
        if i < 2 { clr.push_str(&format!("<a:{n}><a:sysClr val=\"{}\" lastClr=\"{}\"/></a:{n}>", if i == 0 { "windowText" } else { "window" }, hex(i))); }
        else { clr.push_str(&format!("<a:{n}><a:srgbClr val=\"{}\"/></a:{n}>", hex(i))); }
    }
    let fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
    let ln = |w: u32| format!("<a:ln w=\"{w}\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/><a:miter lim=\"800000\"/></a:ln>");
    let (fills, lines, effs) = match &wb.theme.effects {
        Some(e) => (e.fills.clone(), e.lines.clone(), e.effects.clone()),
        None => (format!("{fill}{fill}{fill}"), format!("{}{}{}", ln(6350), ln(12700), ln(19050)), "<a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle>".to_string()),
    };
    let t = &wb.theme;
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"{}\"><a:themeElements><a:clrScheme name=\"{}\">{clr}</a:clrScheme>\
<a:fontScheme name=\"{}\"><a:majorFont><a:latin typeface=\"{}\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>\
<a:minorFont><a:latin typeface=\"{}\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme>\
<a:fmtScheme name=\"{}\"><a:fillStyleLst>{fills}</a:fillStyleLst><a:lnStyleLst>{lines}</a:lnStyleLst>\
<a:effectStyleLst>{effs}</a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",
        attr(&t.name), attr(&t.colors_name), attr(&t.fonts_name), attr(&t.major), attr(&t.minor), attr(&t.effects_name))
}

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }

/// The names and fonts of a theme part (its colors are read with the workbook's).
pub fn parse(xml: &str) -> Option<Theme> {
    let root = Element::parse(xml).ok()?;
    let mut t = Theme { name: root.attr("name").unwrap_or("Office Theme").into(), effects: None, ..Default::default() };
    if let Some(cs) = find(&root, "clrScheme") { t.colors_name = cs.attr("name").unwrap_or("").into(); }
    if let Some(fs) = find(&root, "fontScheme") {
        t.fonts_name = fs.attr("name").unwrap_or("").into();
        if let Some(f) = fs.child("majorFont").and_then(|f| f.child("latin")).and_then(|l| l.attr("typeface")) { t.major = f.into(); }
        if let Some(f) = fs.child("minorFont").and_then(|f| f.child("latin")).and_then(|l| l.attr("typeface")) { t.minor = f.into(); }
    }
    if let Some(fm) = find(&root, "fmtScheme") { t.effects_name = fm.attr("name").unwrap_or("").into(); }
    Some(t)
}

/// The twelve colors of a theme part, dk1 first.
pub fn parse_colors(xml: &str) -> Option<[Rgb; 12]> {
    let root = Element::parse(xml).ok()?;
    let scheme = find(&root, "clrScheme")?;
    let mut out = crate::numfmt::OFFICE_THEME;
    for (i, n) in SLOTS.iter().enumerate() {
        let Some(c) = scheme.child(n).and_then(|e| e.elements().next()) else { continue };
        if let Some(v) = c.attr("val").filter(|_| c.local() == "srgbClr").or(c.attr("lastClr")) { if v.len() == 6 { out[i] = hx(v); } }
    }
    Some(out)
}

// ---- .thmx files ------------------------------------------------------------------------------

/// Save Current Theme: a .thmx package holding the theme part.
pub fn thmx(wb: &Workbook) -> Vec<u8> {
    use std::io::Write;
    let theme = wb.parts.get("xl/theme/theme1.xml").cloned().unwrap_or_else(|| theme_xml(wb).into_bytes());
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let files: [(&str, Vec<u8>); 3] = [
        ("[Content_Types].xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/theme/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/></Types>".to_vec()),
        ("_rels/.rels", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme/theme1.xml\"/></Relationships>".to_vec()),
        ("theme/theme/theme1.xml", theme),
    ];
    for (n, b) in files { let _ = z.start_file(n, o); let _ = z.write_all(&b); }
    z.finish().map(|c| c.into_inner()).unwrap_or_default()
}

/// Browse for Themes: a .thmx (or a bare theme XML) as a built-in-like theme.
pub fn read_thmx(bytes: &[u8]) -> Option<Builtin> {
    use std::io::Read;
    let xml = match zip::ZipArchive::new(std::io::Cursor::new(bytes)) {
        Ok(mut z) => {
            let name = (0..z.len()).filter_map(|i| z.by_index(i).ok().map(|f| f.name().to_string())).find(|n| n.ends_with(".xml") && n.contains("theme/theme"))?;
            let mut s = String::new(); z.by_name(&name).ok()?.read_to_string(&mut s).ok()?; s
        }
        Err(_) => part_text(bytes),
    };
    let t = parse(&xml)?;
    let colors = parse_colors(&xml)?;
    Some(Builtin { name: t.name.clone(), colors: Palette { name: t.colors_name.clone(), colors }, major: t.major, minor: t.minor, effects: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::Pos;

    #[test]
    fn theme_colors_in_styles_follow_the_theme_and_round_trip() {
        let mut wb = Workbook::new();
        wb.set_input(0, Pos::new(0, 0), "x");
        let accent = ThemeColor::new(4, 0.4);
        let mut st = wb.style_at(0, Pos::new(0, 0)).clone();
        st.fill = Some(accent.resolve(&wb.theme_colors)); st.fill_th = Some(accent);
        st.color = Some(ThemeColor::new(5, 0.0).resolve(&wb.theme_colors)); st.color_th = Some(ThemeColor::new(5, 0.0));
        let i = wb.styles.intern(st);
        wb.sheets[0].cells.get_mut(&Pos::new(0, 0)).unwrap().style = i;
        // Round trip keeps the references.
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let s = back.style_at(0, Pos::new(0, 0));
        assert_eq!(s.fill_th, Some(accent));
        assert_eq!(s.color_th, Some(ThemeColor::new(5, 0.0)));
        assert_eq!(s.font_scheme, 1);
        // Another theme recolors the cell.
        let mut b = back.clone();
        let t = builtins().into_iter().find(|t| t.name == "Office 2007 - 2010 Theme").unwrap();
        apply(&mut b, &t);
        let s = b.style_at(0, Pos::new(0, 0));
        assert_eq!(s.fill, Some(tint([0x4F, 0x81, 0xBD], 0.4)));
        assert_eq!(s.color, Some([0xC0, 0x50, 0x4D]));
        assert_eq!(b.theme.major, "Cambria");
        let again = crate::xlsx::read(&crate::xlsx::write(&b).unwrap()).unwrap();
        assert_eq!(again.theme_colors[4], [0x4F, 0x81, 0xBD]);
        assert_eq!(again.theme.name, "Office 2007 - 2010 Theme");
        assert_eq!(again.style_at(0, Pos::new(0, 0)).fill, Some(tint([0x4F, 0x81, 0xBD], 0.4)));
        // Fonts follow the body font.
        let mut c = again.clone();
        apply_fonts(&mut c, "Georgia", "Georgia", "Georgia");
        assert_eq!(c.style_at(0, Pos::new(0, 0)).font, "Georgia");
        // .thmx round trip.
        let th = read_thmx(&thmx(&c)).unwrap();
        assert_eq!(th.minor, "Georgia");
        assert_eq!(th.colors.colors[4], [0x4F, 0x81, 0xBD]);
    }

    #[test]
    fn plain_colors_stay() {
        let mut wb = Workbook::new();
        wb.set_input(0, Pos::new(0, 0), "x");
        let mut st = wb.style_at(0, Pos::new(0, 0)).clone();
        st.fill = Some([1, 2, 3]);
        // A stale reference (the color was changed by hand) is dropped.
        st.fill_th = Some(ThemeColor::new(4, 0.0));
        let i = wb.styles.intern(st);
        wb.sheets[0].cells.get_mut(&Pos::new(0, 0)).unwrap().style = i;
        apply_colors(&mut wb, &palettes()[2]);
        assert_eq!(wb.style_at(0, Pos::new(0, 0)).fill, Some([1, 2, 3]));
    }
}
