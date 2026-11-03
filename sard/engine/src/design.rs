//! The Design tab: themes (a designed master with its layouts, theme and artwork, applied as
//! PowerPoint does: the slides move onto its layouts and keep their content), variants, color
//! schemes, theme fonts, effects, background styles, Format Background, hiding background
//! graphics, and slide size with content scaling.
//!
//! The 20 themes and their variants are decks made by the JS editor's theme engine
//! (themes/<id>-<variant>.pptx in the resources, with themes/index.json listing them).

use crate::opc;
use crate::pres::Presentation;
use crate::xml::{Element, Node};

#[derive(Clone, Debug)]
pub struct ThemeInfo { pub id: String, pub name: String, pub fonts: (String, String), pub variants: Vec<Variant> }
#[derive(Clone, Debug)]
pub struct Variant { pub name: String, pub dark: bool, pub colors: Vec<(String, String)> }
#[derive(Clone, Debug)]
pub struct EffectScheme { pub id: String, pub name: String, pub fills: String, pub lines: String, pub effects: String }

/// The theme catalog (from themes/index.json).
#[derive(Clone, Debug, Default)]
pub struct Catalog { pub themes: Vec<ThemeInfo>, pub color_schemes: Vec<Variant>, pub font_pairs: Vec<(String, String, String)>, pub effects: Vec<EffectScheme> }

const SLOTS: [&str; 12] = ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];

fn colors_of(v: &serde_like::Value) -> Vec<(String, String)> {
    SLOTS.iter().filter_map(|s| v.get(s).and_then(|c| c.as_str()).map(|c| (s.to_string(), c.trim_start_matches('#').to_uppercase()))).collect()
}

/// A tiny JSON reader (the catalog is our own file; no dependency needed).
mod serde_like {
    #[derive(Clone, Debug)]
    pub enum Value { Null, Bool(bool), Num(f64), Str(String), Arr(Vec<Value>), Obj(Vec<(String, Value)>) }
    impl Value {
        pub fn get(&self, k: &str) -> Option<&Value> { if let Value::Obj(v) = self { v.iter().find(|e| e.0 == k).map(|e| &e.1) } else { None } }
        pub fn as_str(&self) -> Option<&str> { if let Value::Str(s) = self { Some(s) } else { None } }
        pub fn as_arr(&self) -> &[Value] { if let Value::Arr(a) = self { a } else { &[] } }
        pub fn as_bool(&self) -> bool { matches!(self, Value::Bool(true)) }
    }
    pub fn parse(s: &str) -> Option<Value> { let c: Vec<char> = s.chars().collect(); let mut i = 0; let v = val(&c, &mut i)?; Some(v) }
    fn ws(c: &[char], i: &mut usize) { while *i < c.len() && c[*i].is_whitespace() { *i += 1; } }
    fn val(c: &[char], i: &mut usize) -> Option<Value> {
        ws(c, i);
        match *c.get(*i)? {
            '{' => { *i += 1; let mut v = vec![]; loop { ws(c, i); if c.get(*i) == Some(&'}') { *i += 1; break; } let Value::Str(k) = val(c, i)? else { return None }; ws(c, i); *i += 1; let x = val(c, i)?; v.push((k, x)); ws(c, i); if c.get(*i) == Some(&',') { *i += 1; } } Some(Value::Obj(v)) }
            '[' => { *i += 1; let mut v = vec![]; loop { ws(c, i); if c.get(*i) == Some(&']') { *i += 1; break; } v.push(val(c, i)?); ws(c, i); if c.get(*i) == Some(&',') { *i += 1; } } Some(Value::Arr(v)) }
            '"' => { *i += 1; let mut s = String::new(); while *i < c.len() && c[*i] != '"' { if c[*i] == '\\' { *i += 1; match c.get(*i)? { 'n' => s.push('\n'), 't' => s.push('\t'), 'u' => { let h: String = c[*i + 1..*i + 5].iter().collect(); s.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?).unwrap_or('?')); *i += 4; } x => s.push(*x) } } else { s.push(c[*i]); } *i += 1; } *i += 1; Some(Value::Str(s)) }
            't' => { *i += 4; Some(Value::Bool(true)) } 'f' => { *i += 5; Some(Value::Bool(false)) } 'n' => { *i += 4; Some(Value::Null) }
            _ => { let st = *i; while *i < c.len() && (c[*i].is_ascii_digit() || "+-.eE".contains(c[*i])) { *i += 1; } c[st..*i].iter().collect::<String>().parse().ok().map(Value::Num) }
        }
    }
}

pub fn catalog() -> &'static Catalog {
    static C: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let Some(bytes) = word_engine::resources::load("themes/index.json") else { return Catalog::default() };
        let Some(v) = std::str::from_utf8(bytes).ok().and_then(serde_like::parse) else { return Catalog::default() };
        let variant = |x: &serde_like::Value| Variant { name: x.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(), dark: x.get("dark").is_some_and(|d| d.as_bool()), colors: x.get("colors").map(colors_of).unwrap_or_default() };
        Catalog {
            themes: v.get("themes").map(|t| t.as_arr()).unwrap_or(&[]).iter().map(|t| {
                let f = t.get("fonts").map(|f| f.as_arr()).unwrap_or(&[]);
                ThemeInfo {
                    id: t.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    name: t.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    fonts: (f.first().and_then(|x| x.as_str()).unwrap_or("Calibri").to_string(), f.get(1).and_then(|x| x.as_str()).unwrap_or("Calibri").to_string()),
                    variants: t.get("variants").map(|v| v.as_arr()).unwrap_or(&[]).iter().map(variant).collect(),
                }
            }).collect(),
            color_schemes: v.get("colorSchemes").map(|t| t.as_arr()).unwrap_or(&[]).iter().map(variant).collect(),
            font_pairs: v.get("fontPairs").map(|t| t.as_arr()).unwrap_or(&[]).iter().filter_map(|p| Some((p.get("name")?.as_str()?.to_string(), p.get("major")?.as_str()?.to_string(), p.get("minor")?.as_str()?.to_string()))).collect(),
            effects: v.get("effects").map(|t| t.as_arr()).unwrap_or(&[]).iter().filter_map(|e| Some(EffectScheme { id: e.get("id")?.as_str()?.to_string(), name: e.get("name")?.as_str()?.to_string(), fills: e.get("fills")?.as_str()?.to_string(), lines: e.get("lines")?.as_str()?.to_string(), effects: e.get("effects")?.as_str()?.to_string() })).collect(),
        }
    })
}

/// A theme's deck for a variant.
pub fn theme_deck(id: &str, variant: usize) -> Option<Presentation> {
    Presentation::open(word_engine::resources::load(&format!("themes/{id}-{variant}.pptx"))?).ok()
}

const RT_LAYOUT: &str = opc::RT_LAYOUT;

impl Presentation {
    /// The ids used by masters and layouts (they share one number space).
    fn master_layout_ids(&self) -> u32 {
        let mut max = 2_147_483_648u32;
        if let Some(l) = self.root().child("p:sldMasterIdLst") { for e in l.elements() { if let Some(i) = e.attr_i64("id") { max = max.max(i as u32); } } }
        for m in self.masters() { if let Some(l) = self.pkg.xml(&m).and_then(|x| x.child("p:sldLayoutIdLst")) { for e in l.elements() { if let Some(i) = e.attr_i64("id") { max = max.max(i as u32); } } } }
        max
    }

    /// Copy a part (and the parts it relates to, except `skip` kinds) from another
    /// presentation; returns its new name here. `done` maps source names already copied.
    fn import_part(&mut self, src: &Presentation, part: &str, done: &mut Vec<(String, String)>, skip: &[&str]) -> Option<String> {
        if let Some((_, n)) = done.iter().find(|d| d.0 == part) { return Some(n.clone()); }
        let (dir, file) = part.rsplit_once('/').unwrap_or(("ppt", part));
        let ext = file.rsplit('.').next().unwrap_or("xml");
        let stem = file.trim_end_matches(&format!(".{ext}")).trim_end_matches(|c: char| c.is_ascii_digit());
        let name = self.pkg.free_name(&format!("{dir}/{stem}"), &format!(".{ext}"));
        done.push((part.to_string(), name.clone()));
        match src.pkg.parts.get(part)? {
            opc::PartData::Xml(d) => { let ct = src.pkg.content_type(part); self.pkg.put_xml(&name, d.root.clone(), ct.as_deref()); }
            opc::PartData::Bytes(b) => self.pkg.put_bytes(&name, b.to_vec()),
        }
        // Relationships keep their ids, pointing at copies.
        if let Some(rels) = src.pkg.xml(&opc::rels_name(part)).cloned() {
            let mut rels = rels;
            for r in rels.elements_mut() {
                if r.attr("TargetMode") == Some("External") { continue; }
                let kind = r.attr("Type").unwrap_or("").to_string();
                let target = opc::resolve(part, r.attr("Target").unwrap_or(""));
                let new = if skip.contains(&kind.as_str()) { done.iter().find(|d| d.0 == target).map(|d| d.1.clone()) } else { self.import_part(src, &target, done, skip) };
                if let Some(n) = new { r.set_attr("Target", opc::relative(&name, &n)); }
            }
            self.pkg.put_xml(&opc::rels_name(&name), rels, None);
        }
        Some(name)
    }

    /// Apply a theme deck's master (with its layouts, theme and artwork) to the slides given
    /// (all when `slides` is empty): they move onto its layouts of the same kind.
    pub fn apply_theme_deck(&mut self, deck: &Presentation, slides: &[String]) -> Option<String> {
        let src_master = deck.masters().first()?.clone();
        let mut done = vec![];
        // Layouts point back to the master: copy the master first, then its layouts (which
        // see the master already copied).
        let new_master = self.import_part(deck, &src_master, &mut done, &[RT_LAYOUT, opc::RT_MASTER])?;
        for l in deck.layouts(&src_master) {
            let nl = self.import_part(deck, &l, &mut done, &[opc::RT_MASTER])?;
            // The master's rels to layouts were skipped: point them at the copies.
            if let Some(rels) = self.pkg.xml_mut(&opc::rels_name(&new_master)) {
                for r in rels.elements_mut() {
                    if opc::resolve(&src_master, r.attr("Target").unwrap_or("")) == l { r.set_attr("Target", opc::relative(&new_master, &nl)); }
                }
            }
        }
        // Fresh ids for the master and its layouts.
        let mut next = self.master_layout_ids() + 1;
        if let Some(lst) = self.pkg.xml_mut(&new_master).and_then(|m| m.child_mut("p:sldLayoutIdLst")) {
            for e in lst.elements_mut() { e.set_attr("id", next.to_string()); next += 1; }
        }
        let main = self.main.clone();
        let rid = self.pkg.add_rel(&main, opc::RT_MASTER, &new_master, false);
        let lst = self.root_mut().ensure_at("p:sldMasterIdLst", &["p:notesMasterIdLst", "p:handoutMasterIdLst", "p:sldIdLst", "p:sldSz", "p:notesSz"]);
        lst.children.push(Node::Element(Element::new("p:sldMasterId").with_attr("id", next.to_string()).with_attr("r:id", rid)));
        // Move the slides.
        let targets: Vec<String> = if slides.is_empty() { self.slides() } else { slides.to_vec() };
        let old_masters = self.masters();
        let new_layouts = self.layouts(&new_master);
        for s in &targets {
            let (kind, name) = self.layout_of(s).map(|l| (self.layout_type(&l), self.layout_name(&l))).unwrap_or_default();
            let pick = new_layouts.iter().find(|l| self.layout_type(l) == kind && kind != "cust")
                .or_else(|| new_layouts.iter().find(|l| self.layout_name(l) == name))
                .or_else(|| new_layouts.iter().find(|l| self.layout_type(l) == "obj"))
                .or(new_layouts.first()).cloned();
            if let Some(l) = pick { self.set_layout(s, &l); }
        }
        // Masters no slide uses any more go (keep at least one).
        for m in old_masters { if m != new_master { self.drop_master_if_unused(&m); } }
        Some(new_master)
    }

    fn drop_master_if_unused(&mut self, master: &str) {
        let layouts = self.layouts(master);
        let used = self.slides().iter().any(|s| self.layout_of(s).is_some_and(|l| layouts.contains(&l)));
        if used || self.masters().len() <= 1 { return; }
        let theme = self.theme_part(master);
        for l in &layouts { self.pkg.remove(l); }
        let main = self.main.clone();
        if let Some(r) = self.pkg.rels(&main).into_iter().find(|r| r.kind == opc::RT_MASTER && opc::resolve(&main, &r.target) == master) {
            if let Some(lst) = self.root_mut().child_mut("p:sldMasterIdLst") { lst.children.retain(|n| !matches!(n, Node::Element(e) if e.attr("r:id") == Some(r.id.as_str()))); }
            self.pkg.remove_rel(&main, &r.id);
        }
        self.pkg.remove(master);
        // The theme too, unless the notes master or another master uses it.
        if let Some(t) = theme {
            let still = self.masters().iter().any(|m| self.theme_part(m).as_deref() == Some(t.as_str())) || self.pkg.parts.keys().any(|k| k.contains("notesMaster") && self.pkg.related(k, opc::RT_THEME).as_deref() == Some(t.as_str()));
            if !still { self.pkg.remove(&t); }
        }
    }

    /// Masters used by the given slides (all slides when empty).
    pub fn masters_of(&self, slides: &[String]) -> Vec<String> {
        let list: Vec<String> = if slides.is_empty() { self.slides() } else { slides.to_vec() };
        let mut out = vec![];
        for s in list { if let Some(m) = self.chain(&s).1 { if !out.contains(&m) { out.push(m); } } }
        if out.is_empty() { out = self.masters(); }
        out
    }

    fn theme_root_mut(&mut self, master: &str) -> Option<&mut Element> {
        let t = self.theme_part(master)?;
        self.pkg.xml_mut(&t)
    }

    /// Replace the color scheme of the masters' themes; `dark` swaps the background and text
    /// colors (the master's color map).
    pub fn set_theme_colors(&mut self, masters: &[String], name: &str, colors: &[(String, String)], dark: Option<bool>) {
        for m in masters {
            if let Some(cs) = self.theme_root_mut(m).and_then(|t| t.path_mut("a:themeElements/a:clrScheme")) {
                cs.set_attr("name", name);
                cs.children = SLOTS.iter().filter_map(|slot| colors.iter().find(|c| c.0 == *slot)).map(|(slot, hex)| Node::Element(Element::new(&format!("a:{slot}")).with_child(Element::new("a:srgbClr").with_attr("val", hex.clone())))).collect();
            }
            if let Some(d) = dark {
                if let Some(map) = self.pkg.xml_mut(m).and_then(|x| x.child_mut("p:clrMap")) {
                    let (bg1, tx1, bg2, tx2) = if d { ("dk1", "lt1", "dk2", "lt2") } else { ("lt1", "dk1", "lt2", "dk2") };
                    map.set_attr("bg1", bg1); map.set_attr("tx1", tx1); map.set_attr("bg2", bg2); map.set_attr("tx2", tx2);
                }
            }
        }
    }
    /// Theme fonts (headings, body).
    pub fn set_theme_fonts(&mut self, masters: &[String], name: &str, major: &str, minor: &str) {
        for m in masters {
            let Some(fs) = self.theme_root_mut(m).and_then(|t| t.path_mut("a:themeElements/a:fontScheme")) else { continue };
            fs.set_attr("name", name);
            for (which, face) in [("a:majorFont", major), ("a:minorFont", minor)] {
                if let Some(f) = fs.child_mut(which) { f.ensure_at("a:latin", &["a:ea", "a:cs", "a:font", "a:extLst"]).set_attr("typeface", face); }
            }
        }
    }
    /// Theme effects (fill, line and effect style lists).
    pub fn set_theme_effects(&mut self, masters: &[String], scheme: &EffectScheme) {
        let wrap = |tag: &str, inner: &str| Element::parse(&format!(r#"<a:{tag} xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">{inner}</a:{tag}>"#)).ok();
        for m in masters {
            let Some(fmt) = self.theme_root_mut(m).and_then(|t| t.path_mut("a:themeElements/a:fmtScheme")) else { continue };
            fmt.set_attr("name", scheme.name.clone());
            for (tag, inner) in [("fillStyleLst", &scheme.fills), ("lnStyleLst", &scheme.lines), ("effectStyleLst", &scheme.effects)] {
                if let Some(mut el) = wrap(tag, inner) {
                    el.remove_attr("xmlns:a");
                    let name = format!("a:{tag}");
                    if let Some(pos) = fmt.children.iter().position(|n| matches!(n, Node::Element(e) if e.name == name)) { fmt.children[pos] = Node::Element(el); }
                }
            }
        }
    }

    /// Background Styles gallery: 12 styles (solid, linear, radial × bg1, bg2, tx1, tx2).
    pub fn set_background_style(&mut self, slides: &[String], style: usize) {
        let token = ["bg1", "bg2", "tx2", "tx1"][style % 4];
        let fill = match style / 4 {
            0 => format!(r#"<a:solidFill><a:schemeClr val="{token}"/></a:solidFill>"#),
            1 => format!(r#"<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="{token}"><a:tint val="92000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="{token}"><a:shade val="88000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill>"#),
            _ => format!(r#"<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="{token}"><a:tint val="80000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="{token}"><a:shade val="72000"/></a:schemeClr></a:gs></a:gsLst><a:path path="circle"><a:fillToRect l="50000" t="30000" r="50000" b="70000"/></a:path></a:gradFill>"#),
        };
        for s in slides { self.set_background_fill(s, Some(&fill)); }
    }

    /// A slide's background fill from DrawingML fill XML (solid, gradient, picture, pattern),
    /// or back to the layout's (None).
    pub fn set_background_fill(&mut self, slide: &str, fill_xml: Option<&str>) {
        let el = fill_xml.and_then(|f| Element::parse(&format!(r#"<p:bg xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:bgPr>{f}<a:effectLst/></p:bgPr></p:bg>"#)).ok());
        let Some(csld) = self.pkg.xml_mut(slide).and_then(|s| s.child_mut("p:cSld")) else { return };
        csld.remove_children("p:bg");
        if let Some(mut bg) = el { for k in ["xmlns:p", "xmlns:a", "xmlns:r"] { bg.remove_attr(k); } csld.children.insert(0, Node::Element(bg)); }
    }
    /// A picture background (stretched or tiled), with transparency 0–1.
    pub fn set_background_picture(&mut self, slide: &str, bytes: Vec<u8>, ext: &str, tile: bool, transparency: f64) {
        let media = self.pkg.free_name("ppt/media/image", &format!(".{ext}"));
        self.pkg.put_bytes(&media, bytes);
        let rid = self.pkg.add_rel(slide, opc::RT_IMAGE, &media, false);
        let alpha = if transparency > 0.0 { format!(r#"<a:alphaModFix amt="{}"/>"#, ((1.0 - transparency) * 100_000.0) as i64) } else { String::new() };
        let mode = if tile { r#"<a:tile tx="0" ty="0" sx="100000" sy="100000" flip="none" algn="tl"/>"# } else { "<a:stretch><a:fillRect/></a:stretch>" };
        self.set_background_fill(slide, Some(&format!(r#"<a:blipFill dpi="0" rotWithShape="1"><a:blip r:embed="{rid}">{alpha}</a:blip><a:srcRect/>{mode}</a:blipFill>"#)));
    }
    /// Hide (or show) the master's and layout's artwork on a slide.
    pub fn set_hide_background_graphics(&mut self, slide: &str, hide: bool) {
        if let Some(s) = self.pkg.xml_mut(slide) { if hide { s.set_attr("showMasterSp", "0"); } else { s.remove_attr("showMasterSp"); } }
    }
    pub fn hides_background_graphics(&self, slide: &str) -> bool { self.pkg.xml(slide).and_then(|s| s.attr("showMasterSp")).is_some_and(|v| v == "0") }

    /// Change the slide size; `fit` scales every slide's, layout's and master's content
    /// (Ensure Fit keeps it all visible; Maximize fills the new size).
    pub fn resize_slides(&mut self, cx: i64, cy: i64, fit: Option<bool>) {
        let (ow, oh) = self.size();
        self.set_size(cx, cy);
        let Some(ensure) = fit else { return };
        let (sx, sy) = (cx as f64 / ow, cy as f64 / oh);
        let k = if ensure { sx.min(sy) } else { sx.max(sy) };
        let (dx, dy) = ((cx as f64 - ow * k) / 2.0, (cy as f64 - oh * k) / 2.0);
        let parts: Vec<String> = self.slides().into_iter().chain(self.all_layouts()).chain(self.masters()).collect();
        for p in parts {
            let Some(tree) = self.pkg.xml_mut(&p).and_then(|r| r.path_mut("p:cSld/p:spTree")) else { continue };
            for sp in tree.elements_mut() {
                let x = match sp.local() { "grpSp" => sp.path_mut("p:grpSpPr/a:xfrm"), "graphicFrame" => sp.child_mut("p:xfrm"), _ => sp.path_mut("p:spPr/a:xfrm") };
                let Some(x) = x else { continue };
                if let Some(o) = x.child_mut("a:off") {
                    let (ox, oy) = (o.attr_f64("x").unwrap_or(0.0), o.attr_f64("y").unwrap_or(0.0));
                    o.set_attr("x", ((ox * k + dx).round() as i64).to_string()); o.set_attr("y", ((oy * k + dy).round() as i64).to_string());
                }
                if let Some(e) = x.child_mut("a:ext") {
                    let (ex, ey) = (e.attr_f64("cx").unwrap_or(0.0), e.attr_f64("cy").unwrap_or(0.0));
                    e.set_attr("cx", ((ex * k).round() as i64).to_string()); e.set_attr("cy", ((ey * k).round() as i64).to_string());
                }
            }
        }
    }

    /// The theme name and variant colors in use (for the gallery's highlight).
    pub fn theme_name(&self, slide: &str) -> String { self.master_for(slide).map(|m| self.theme(&m).name).unwrap_or_default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_apply() {
        let cat = catalog();
        assert!(cat.themes.len() >= 20, "themes {}", cat.themes.len());
        let mut p = Presentation::open(include_bytes!("../tests/fixtures/sample.pptx")).unwrap();
        let t = &cat.themes[3];
        let deck = theme_deck(&t.id, 1).unwrap();
        let master = p.apply_theme_deck(&deck, &[]).unwrap();
        assert_eq!(p.masters(), vec![master.clone()], "old master dropped");
        for s in p.slides() { assert_eq!(p.chain(&s).1.as_deref(), Some(master.as_str())); }
        assert_eq!(p.slide_title(&p.slides()[1]), "Agenda");
        p.set_theme_colors(&[master.clone()], "X", &cat.color_schemes[5].colors, Some(true));
        p.set_theme_fonts(&[master.clone()], "Y", "Georgia", "Verdana");
        p.set_theme_effects(&[master.clone()], &cat.effects[2]);
        p.set_background_style(&p.slides(), 5);
        let again = Presentation::open(&p.save().unwrap()).unwrap();
        assert_eq!(again.slides().len(), 2);
        assert_eq!(again.theme(&again.masters()[0]).major.latin, "Georgia");
        // Slide size with content scaled to fit.
        p.resize_slides(9_144_000, 6_858_000, Some(true));
        assert_eq!(p.size(), (9_144_000.0, 6_858_000.0));
    }
}

#[cfg(test)]
mod look {
    use super::*;
    #[test]
    fn theme_snapshots() {
        let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") else { return };
        for (k, t) in catalog().themes.iter().enumerate().step_by(5) {
            let mut p = Presentation::open(include_bytes!("../tests/fixtures/sample.pptx")).unwrap();
            p.apply_theme_deck(&theme_deck(&t.id, 0).unwrap(), &[]).unwrap();
            for (i, s) in p.slides().iter().enumerate() { std::fs::write(format!("{dir}/theme{k}-{i}.png"), crate::render::slide_png(&p, s, 640).unwrap()).unwrap(); }
        }
    }
}
