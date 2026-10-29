//! Document design and page furniture: headers and footers (galleries, page numbers), styles
//! (style sets, theme fonts and colors, paragraph spacing), page color and borders,
//! watermarks, and drop caps.

use crate::format::{Alignment, Border, DropCap, LineRule, ParaBorders, ParaProps, RunProps, TabKind, TabStop, Leader};
use crate::model::{Anchor, AnchorAlign, Document, EditKind, Field, Paragraph, RelFrom, Run, Story, Wrap};
use crate::objects::{Object, Shape, ShapeKind};
use crate::styles::{Style, StyleKind};
use crate::Color;

// ---------------------------------------------------------------------------
// Headers and footers

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoryDesign { Blank, ThreeColumns, Line, Banded, Title }

pub const HEADER_DESIGNS: &[(StoryDesign, &str)] = &[(StoryDesign::Blank, "Blank"), (StoryDesign::ThreeColumns, "Blank (Three Columns)"), (StoryDesign::Line, "Austin"), (StoryDesign::Banded, "Banded"), (StoryDesign::Title, "Facet")];

/// Page-number designs: (name, text before, text after, with "of NUMPAGES").
pub const PAGE_NUMBER_STYLES: &[(&str, &str, &str, bool)] = &[("Plain Number", "", "", false), ("Page X of Y", "Page ", "", true), ("Accent Bar", "", " | Page", false), ("Bracketed", "[", "]", false), ("Dashes", "– ", " –", false)];

fn field_run(f: Field, props: &RunProps) -> Run { Run { text: " ".into(), field: Some(f), props: props.clone(), ..Default::default() } }

impl Document {
    pub fn story(&self, header: bool) -> Option<&Story> { if header { self.header.as_ref() } else { self.footer.as_ref() } }

    /// Replace a header or footer (None removes it).
    pub fn set_story(&mut self, header: bool, paragraphs: Option<Vec<Paragraph>>) {
        self.checkpoint(EditKind::Other);
        let s = paragraphs.map(|mut p| { for x in &mut p { x.cell = None; } if p.is_empty() { p.push(Paragraph::new("")); } Story { paragraphs: p } });
        if header { self.header = s; } else { self.footer = s; }
        self.stories_changed = true;
        self.format_epoch += 1;
        self.changed();
    }

    /// The content width in twips (for right tab stops in headers).
    fn text_width_twips(&self) -> i32 { self.page.width - self.page.left - self.page.right - self.page.gutter }

    /// Paragraphs of a built-in header or footer design.
    pub fn story_design(&self, design: StoryDesign, header: bool) -> Vec<Paragraph> {
        let w = self.text_width_twips();
        let accent = Color(0x44, 0x72, 0xC4);
        let title = if self.properties.title.is_empty() { "Document title".to_string() } else { self.properties.title.clone() };
        let placeholder = |t: &str| Run { text: t.into(), props: RunProps { color: Some(Some(Color(0x7F, 0x7F, 0x7F))), ..Default::default() }, ..Default::default() };
        match design {
            StoryDesign::Blank => vec![Paragraph { runs: vec![placeholder("[Type here]")], ..Default::default() }],
            StoryDesign::ThreeColumns => {
                let mut p = Paragraph { runs: vec![placeholder("[Type here]\t[Type here]\t[Type here]")], ..Default::default() };
                p.props.tabs = Some(vec![TabStop { pos: w / 2, kind: TabKind::Center, leader: Leader::None }, TabStop { pos: w, kind: TabKind::Right, leader: Leader::None }]);
                vec![p]
            }
            StoryDesign::Line => {
                let line = Some(Border { width: 1.0, space: 4.0, color: Some(accent) });
                let mut p = if header { Paragraph { runs: vec![Run { text: title.to_uppercase(), props: RunProps { color: Some(Some(accent)), spacing: Some(1.0), ..Default::default() }, ..Default::default() }], ..Default::default() } }
                    else { Paragraph { runs: vec![field_run(Field::Page, &RunProps { color: Some(Some(accent)), ..Default::default() })], ..Default::default() } };
                p.props.alignment = Some(Alignment::Center);
                p.props.borders = Some(if header { ParaBorders { bottom: line, ..Default::default() } } else { ParaBorders { top: line, ..Default::default() } });
                vec![p]
            }
            StoryDesign::Banded => {
                let white = RunProps { color: Some(Some(Color::WHITE)), bold: Some(true), ..Default::default() };
                let runs = if header { vec![Run { text: title.to_uppercase(), props: white.clone(), ..Default::default() }] } else { vec![Run { text: "Page ".into(), props: white.clone(), ..Default::default() }, field_run(Field::Page, &white), Run { text: " of ".into(), props: white.clone(), ..Default::default() }, field_run(Field::NumPages, &white)] };
                let mut p = Paragraph { runs, ..Default::default() };
                p.props.shading = Some(Some(accent));
                p.props.alignment = Some(Alignment::Center);
                p.props.space_after = Some(0.0);
                vec![p]
            }
            StoryDesign::Title => {
                let mut p = if header {
                    Paragraph { runs: vec![Run { text: title, props: RunProps { color: Some(Some(Color(0x40, 0x40, 0x40))), ..Default::default() }, ..Default::default() }, Run::text("\t"), field_run(Field::Date("MMMM d, yyyy".into()), &RunProps::default())], ..Default::default() }
                } else {
                    Paragraph { runs: vec![field_run(Field::Prop("AUTHOR".into()), &RunProps::default()), Run::text("\t"), field_run(Field::Page, &RunProps { bold: Some(true), color: Some(Some(accent)), ..Default::default() })], ..Default::default() }
                };
                p.props.tabs = Some(vec![TabStop { pos: w, kind: TabKind::Right, leader: Leader::None }]);
                vec![p]
            }
        }
    }

    /// A page-number paragraph in a style, aligned left/center/right.
    pub fn page_number_paragraph(&self, style: usize, align: Alignment) -> Paragraph {
        let (_, before, after, of) = PAGE_NUMBER_STYLES[style.min(PAGE_NUMBER_STYLES.len() - 1)];
        let props = RunProps::default();
        let mut runs = vec![];
        if !before.is_empty() { runs.push(Run::text(before)); }
        runs.push(field_run(Field::Page, &props));
        if of { runs.push(Run::text(" of ")); runs.push(field_run(Field::NumPages, &props)); }
        if !after.is_empty() { runs.push(Run::text(after)); }
        let mut p = Paragraph { runs, ..Default::default() };
        p.props.alignment = Some(align);
        p
    }

    /// Insert page numbers at the top or bottom of the page; other content of that header or
    /// footer stays (the number gets its own paragraph).
    pub fn insert_page_numbers(&mut self, top: bool, style: usize, align: Alignment) {
        let p = self.page_number_paragraph(style, align);
        let mut paras = self.story(top).map(|s| s.paragraphs.clone()).unwrap_or_default();
        // Replace an earlier page-number paragraph rather than stacking them.
        paras.retain(|x| !x.runs.iter().any(|r| r.field == Some(Field::Page)) && !x.is_empty());
        if top { paras.push(p); } else { paras.insert(0, p); }
        self.set_story(top, Some(paras));
    }

    pub fn remove_page_numbers(&mut self) {
        for top in [true, false] {
            let Some(s) = self.story(top) else { continue };
            if !s.paragraphs.iter().any(|p| p.runs.iter().any(|r| matches!(r.field, Some(Field::Page | Field::NumPages)))) { continue; }
            let paras: Vec<Paragraph> = s.paragraphs.iter().filter(|p| !p.runs.iter().any(|r| matches!(r.field, Some(Field::Page | Field::NumPages)))).cloned().collect();
            self.set_story(top, if paras.iter().all(|p| p.is_empty()) { None } else { Some(paras) });
        }
    }

    // -----------------------------------------------------------------------
    // Styles

    /// Edit a style (created from Normal when missing) as one undo step.
    pub fn update_style(&mut self, id: &str, f: impl FnOnce(&mut Style)) {
        if !self.styles.styles.contains_key(id) {
            let name = match id { "Heading1" => "heading 1", "Heading2" => "heading 2", "Heading3" => "heading 3", other => other }.to_string();
            let outline = match id { "Heading1" => Some(0), "Heading2" => Some(1), "Heading3" => Some(2), _ => None };
            self.styles.styles.insert(id.into(), Style { id: id.into(), name, kind: StyleKind::Paragraph, based_on: Some("Normal".into()), next: Some("Normal".into()), para: ParaProps { outline_level: outline, keep_next: outline.map(|_| true), ..Default::default() }, run: RunProps::default(), table_borders: false });
            self.styles.order.push(id.into());
        }
        if let Some(s) = self.styles.styles.get_mut(id) { f(s); }
        self.style_edits.insert(id.into());
        self.styles_changed = true;
        self.format_epoch += 1;
        self.changed();
    }

    /// Edit the document defaults (docDefaults).
    pub fn update_defaults(&mut self, f: impl FnOnce(&mut RunProps, &mut ParaProps)) {
        let sheet = &mut self.styles;
        f(&mut sheet.doc_run, &mut sheet.doc_para);
        self.style_edits.insert(String::new());
        self.styles_changed = true;
        self.format_epoch += 1;
        self.changed();
    }

    /// Apply a style set (Design ▸ Document Formatting).
    pub fn apply_style_set(&mut self, set: usize) {
        let s = &STYLE_SETS[set.min(STYLE_SETS.len() - 1)];
        self.checkpoint(EditKind::Other);
        for (id, look) in [("Title", &s.title), ("Heading1", &s.h1), ("Heading2", &s.h2)] {
            let look = look.clone();
            self.update_style(id, |st| {
                st.run.size = Some(look.size);
                st.run.bold = Some(look.bold);
                st.run.caps = Some(if look.caps { crate::format::Caps::AllCaps } else { crate::format::Caps::Normal });
                st.run.color = Some(look.color);
                st.run.font = look.font.map(String::from).or(st.run.font.take());
                st.para.borders = Some(if look.rule { ParaBorders { bottom: Some(Border { width: 0.75, space: 1.0, color: look.color }), ..Default::default() } } else { ParaBorders::default() });
                st.para.alignment = Some(if look.center { Alignment::Center } else { Alignment::Left });
            });
        }
    }

    /// Apply theme fonts (headings, body).
    pub fn apply_fonts(&mut self, heading: &str, body: &str) {
        self.checkpoint(EditKind::Other);
        let body = body.to_string();
        self.update_defaults(|r, _| { r.font = Some(body.clone()); r.cs_font = r.cs_font.take(); });
        if self.styles.styles.get("Normal").is_some_and(|n| n.run.font.is_some()) { let b = body.clone(); self.update_style("Normal", |s| s.run.font = Some(b)); }
        for id in ["Title", "Heading1", "Heading2", "Heading3"] { let h = heading.to_string(); self.update_style(id, |s| s.run.font = Some(h)); }
    }

    /// Apply theme colors: headings take the accent, the title a dark text color.
    pub fn apply_colors(&mut self, palette: &[Color; 6]) {
        self.checkpoint(EditKind::Other);
        let shade = |c: Color, k: f32| Color((c.0 as f32 * k) as u8, (c.1 as f32 * k) as u8, (c.2 as f32 * k) as u8);
        let (a, b) = (palette[0], palette[1]);
        self.update_style("Heading1", |s| s.run.color = Some(Some(shade(a, 0.75))));
        self.update_style("Heading2", |s| s.run.color = Some(Some(a)));
        self.update_style("Heading3", |s| s.run.color = Some(Some(shade(b, 0.6))));
        self.update_style("Title", |s| s.run.color = Some(Some(shade(a, 0.45))));
    }

    /// Paragraph spacing presets: (before, after, line multiple) applied to the defaults and Normal.
    pub fn apply_paragraph_spacing(&mut self, before: f32, after: f32, line: f32) {
        self.checkpoint(EditKind::Other);
        let set = |p: &mut ParaProps| { p.space_before = Some(before); p.space_after = Some(after); p.line_rule = Some(LineRule::Auto); p.line_spacing = Some(line); };
        self.update_defaults(|_, p| set(p));
        if self.styles.styles.contains_key("Normal") { self.update_style("Normal", |s| set(&mut s.para)); }
    }

    // -----------------------------------------------------------------------
    // Page background

    pub fn set_page_color(&mut self, color: Option<Color>) {
        self.checkpoint(EditKind::Other);
        self.page.color = color;
        self.changed();
    }

    pub fn set_page_borders(&mut self, borders: Option<crate::page::PageBorders>) {
        self.checkpoint(EditKind::Other);
        self.page.borders = borders;
        self.changed();
    }

    /// The watermark in the header, if any.
    pub fn watermark(&self) -> Option<&Run> {
        self.header.as_ref()?.paragraphs.iter().flat_map(|p| p.runs.iter()).find(|r| is_watermark(r))
    }

    /// Put a text or picture watermark behind every page (in the header), or remove it.
    pub fn set_watermark(&mut self, mark: Option<Watermark>) {
        let mut paras = self.header.as_ref().map(|s| s.paragraphs.clone()).unwrap_or_default();
        for p in &mut paras { p.runs.retain(|r| !is_watermark(r)); }
        if let Some(m) = mark {
            let g = self.page.geometry();
            let run = match m {
                Watermark::Text { text, font, size, color, diagonal } => {
                    let mut s = Shape::new(ShapeKind::Rect);
                    s.fill = None; s.outline = None; s.text = text.clone(); s.text_color = color; s.bold = false;
                    let size = if size <= 0.0 { 66.0 } else { size };
                    s.font_size = size;
                    let w = crate::objects::text_width(&text, &font, false, size) + 24.0;
                    s.width = w.min(g.width * 1.1); s.height = size * 4.0 / 3.0 * 1.4;
                    s.rotation = if diagonal { 315.0 } else { 0.0 };
                    s.name = "PowerPlusWaterMarkObject".into();
                    Run { text: crate::chars::OBJECT.to_string(), object: Some(Object::Shape(s)), anchor: Some(centered_anchor()), ..Default::default() }
                }
                Watermark::Picture { asset, width, height, washout } => {
                    let max_w = g.width - g.left - g.right;
                    let k = if width > max_w { max_w / width } else { 1.0 };
                    let mut img = crate::model::Image { asset: asset.clone(), width: width * k, height: height * k, alt: "Watermark".into(), ..Default::default() };
                    if washout {
                        if let Some((png, _)) = self.assets.get(&asset).and_then(|a| crate::picture::apply(&a.bytes, crate::picture::PictureOp::Washout, (width, height))) {
                            let baked = self.add_png_asset(png);
                            img.original = Some(asset);
                            img.asset = baked;
                            img.effects = vec![crate::picture::PictureOp::Washout];
                        }
                    }
                    Run { text: crate::chars::OBJECT.to_string(), image: Some(img), anchor: Some(centered_anchor()), ..Default::default() }
                }
            };
            if paras.is_empty() { paras.push(Paragraph::new("")); }
            paras[0].runs.insert(0, run);
        }
        let empty = paras.iter().all(|p| p.runs.is_empty());
        self.set_story(true, if empty { None } else { Some(paras) });
    }

    // -----------------------------------------------------------------------
    // Drop caps

    /// The drop cap of paragraph `index` (the paragraph before it, when that is its cap).
    pub fn drop_cap_of(&self, index: usize) -> Option<(usize, DropCap)> {
        let cap = |i: usize| self.paragraphs.get(i).and_then(|p| p.props.drop_cap).map(|d| (i, d));
        cap(index).or_else(|| index.checked_sub(1).and_then(cap))
    }

    /// Make the first letter of paragraph `index` a drop cap (or change or remove one). The
    /// letter moves into its own paragraph with a frame, as Word stores it.
    pub fn set_drop_cap(&mut self, index: usize, cap: Option<(DropCap, Option<String>)>) {
        let existing = self.drop_cap_of(index);
        let target = match existing { Some((i, _)) => i + 1, None => index };
        if target >= self.paragraphs.len() || self.paragraphs[target].cell.is_some() { return; }
        self.checkpoint(EditKind::Other);
        // Remove an existing cap first: its letter goes back to the paragraph.
        if let Some((ci, _)) = existing {
            let cap_para = self.paragraphs.remove(ci);
            let letter: String = cap_para.runs.iter().map(|r| r.text.as_str()).collect();
            let t = ci;
            if let Some(first) = self.paragraphs[t].runs.first_mut() { first.text.insert_str(0, &letter); } else { self.paragraphs[t].runs.push(Run::text(letter)); }
            return self.finish_drop_cap(t, cap);
        }
        self.finish_drop_cap(target, cap);
    }

    fn finish_drop_cap(&mut self, target: usize, cap: Option<(DropCap, Option<String>)>) {
        if let Some((dc, font)) = cap {
            let p = &self.paragraphs[target];
            let Some(first_run) = p.runs.iter().position(|r| !r.text.is_empty() && r.field.is_none() && r.object.is_none() && r.image.is_none()) else { self.changed(); return };
            let fmt = self.para_format(p);
            let rf = self.run_format(p, &p.runs[first_run]);
            let letter: String = p.runs[first_run].text.chars().take(1).collect();
            if letter.trim().is_empty() { self.changed(); return; }
            // Word sizes the cap so its capital letter reaches from the first line's top to the
            // last dropped line's baseline.
            let body = crate::fonts::metrics(crate::fonts::resolve(&rf.font, rf.bold, rf.italic));
            let line_pt = match fmt.line_rule { LineRule::Auto => rf.size * (body.ascent + body.descent) * fmt.line_spacing.max(0.06), _ => fmt.line_spacing };
            let cap_font = font.clone().unwrap_or(rf.font.clone());
            let cap = crate::fonts::metrics(crate::fonts::resolve(&cap_font, rf.bold, rf.italic));
            let span = (dc.lines as f32 - 1.0) * line_pt + rf.size * body.cap_height;
            let size = (span / cap.cap_height.max(0.5)).clamp(rf.size, 400.0);
            let mut props = p.runs[first_run].props.clone();
            props.size = Some(size);
            props.size_cs = Some(size);
            if let Some(f) = font { props.font = Some(f); }
            let mut cap_para = Paragraph { runs: vec![Run { text: letter.clone(), props, ..Default::default() }], style: p.style.clone(), ..Default::default() };
            cap_para.props.drop_cap = Some(dc);
            cap_para.props.space_before = Some(0.0);
            cap_para.props.space_after = Some(0.0);
            let r = &mut self.paragraphs[target].runs[first_run];
            r.text = r.text.chars().skip(1).collect();
            self.paragraphs.insert(target, cap_para);
            let start = self.paragraph_start(target + 1);
            self.select(start, start);
        }
        self.format_epoch += 1;
        self.changed();
    }
}

fn centered_anchor() -> Anchor {
    Anchor { wrap: Wrap::Behind, x: 0.0, y: 0.0, x_from: RelFrom::Margin, y_from: RelFrom::Margin, x_align: Some(AnchorAlign::Center), y_align: Some(AnchorAlign::Center), dist: 0.0, z: 0 }
}

fn is_watermark(r: &Run) -> bool {
    matches!(&r.object, Some(Object::Shape(s)) if s.name.starts_with("PowerPlusWaterMarkObject")) || r.image.as_ref().is_some_and(|i| i.alt == "Watermark" && r.anchor.is_some_and(|a| a.wrap == Wrap::Behind))
}

/// A watermark: diagonal or horizontal text, or a (washed-out) picture.
#[derive(Clone, Debug)]
pub enum Watermark {
    Text { text: String, font: String, size: f32, color: Color, diagonal: bool },
    Picture { asset: String, width: f32, height: f32, washout: bool },
}

pub const WATERMARKS: &[&str] = &["CONFIDENTIAL", "DO NOT COPY", "DRAFT", "SAMPLE", "URGENT", "ASAP"];

// ---------------------------------------------------------------------------
// Style sets and themes

#[derive(Clone)]
pub struct Look { pub size: f32, pub bold: bool, pub caps: bool, pub color: Option<Color>, pub rule: bool, pub center: bool, pub font: Option<&'static str> }

pub struct StyleSet { pub name: &'static str, pub title: Look, pub h1: Look, pub h2: Look }

const fn look(size: f32, bold: bool, caps: bool, color: Option<Color>, rule: bool, center: bool, font: Option<&'static str>) -> Look { Look { size, bold, caps, color, rule, center, font } }

pub const STYLE_SETS: &[StyleSet] = &[
    StyleSet { name: "Default", title: look(28.0, false, false, None, false, false, None), h1: look(16.0, false, false, Some(Color(0x2F, 0x54, 0x96)), false, false, None), h2: look(13.0, false, false, Some(Color(0x2F, 0x54, 0x96)), false, false, None) },
    StyleSet { name: "Crisp", title: look(28.0, false, false, None, true, false, None), h1: look(14.0, true, false, None, true, false, None), h2: look(12.0, true, false, None, false, false, None) },
    StyleSet { name: "Classic", title: look(26.0, true, false, None, false, true, Some("Times New Roman")), h1: look(14.0, true, false, None, false, false, Some("Times New Roman")), h2: look(12.0, true, false, None, false, false, Some("Times New Roman")) },
    StyleSet { name: "Banner", title: look(26.0, false, true, Some(Color(0x1F, 0x38, 0x64)), false, false, None), h1: look(14.0, true, true, Some(Color(0x1F, 0x38, 0x64)), true, false, None), h2: look(12.0, true, true, Some(Color(0x1F, 0x38, 0x64)), false, false, None) },
    StyleSet { name: "Minimal", title: look(24.0, false, false, Some(Color(0x40, 0x40, 0x40)), false, false, None), h1: look(14.0, false, false, Some(Color(0x40, 0x40, 0x40)), true, false, None), h2: look(12.0, false, false, Some(Color(0x59, 0x59, 0x59)), false, false, None) },
    StyleSet { name: "Lines (Simple)", title: look(26.0, false, false, None, true, false, None), h1: look(14.0, false, false, Some(Color(0x2F, 0x54, 0x96)), true, false, None), h2: look(12.0, true, false, None, false, false, None) },
    StyleSet { name: "Shaded", title: look(26.0, true, true, Some(Color(0x44, 0x72, 0xC4)), false, true, None), h1: look(14.0, true, true, Some(Color(0x44, 0x72, 0xC4)), false, false, None), h2: look(12.0, true, false, Some(Color(0x44, 0x72, 0xC4)), false, false, None) },
];

/// Theme color sets: six accents.
pub const COLOR_SETS: &[(&str, [Color; 6])] = &[
    ("Office", [Color(0x44, 0x72, 0xC4), Color(0xED, 0x7D, 0x31), Color(0xA5, 0xA5, 0xA5), Color(0xFF, 0xC0, 0x00), Color(0x5B, 0x9B, 0xD5), Color(0x70, 0xAD, 0x47)]),
    ("Blue", [Color(0x0F, 0x6F, 0xC6), Color(0x00, 0x9D, 0xD9), Color(0x0B, 0xD0, 0xD9), Color(0x10, 0xCF, 0x9B), Color(0x7C, 0xCA, 0x62), Color(0xA5, 0xC2, 0x49)]),
    ("Green", [Color(0x54, 0x9E, 0x39), Color(0x8A, 0xB8, 0x33), Color(0xC0, 0xCF, 0x3A), Color(0x02, 0x96, 0x76), Color(0x4A, 0xB5, 0xC4), Color(0x09, 0x89, 0xB1)]),
    ("Red", [Color(0xA5, 0x30, 0x0F), Color(0xD5, 0x58, 0x16), Color(0xE1, 0x9F, 0x2D), Color(0x92, 0x6E, 0x51), Color(0x8F, 0x4E, 0x3F), Color(0x7E, 0x5A, 0x40)]),
    ("Violet", [Color(0x8B, 0x3A, 0x9B), Color(0x9F, 0x5F, 0xCF), Color(0xC0, 0x6F, 0xAE), Color(0x6A, 0x5A, 0xB4), Color(0x42, 0x6A, 0xD0), Color(0x3E, 0x9B, 0xC7)]),
    ("Orange", [Color(0xE4, 0x8D, 0x2E), Color(0xE0, 0x6B, 0x1F), Color(0xD3, 0x47, 0x2E), Color(0xB9, 0x8B, 0x2E), Color(0xA9, 0x6D, 0x2A), Color(0x95, 0x5D, 0x2A)]),
    ("Grayscale", [Color(0x80, 0x80, 0x80), Color(0x59, 0x59, 0x59), Color(0x3F, 0x3F, 0x3F), Color(0xA5, 0xA5, 0xA5), Color(0x7F, 0x7F, 0x7F), Color(0x26, 0x26, 0x26)]),
];

/// Theme font pairs: (name, headings, body).
pub const FONT_SETS: &[(&str, &str, &str)] = &[
    ("Office", "Calibri Light", "Calibri"), ("Office 2007", "Cambria", "Calibri"), ("Calibri", "Calibri", "Calibri"), ("Arial", "Arial", "Arial"),
    ("Times New Roman", "Times New Roman", "Times New Roman"), ("Cambria", "Cambria", "Cambria"), ("Noto Sans", "Noto Sans", "Noto Sans"), ("Courier New", "Courier New", "Courier New"),
];

/// Themes: (name, color set, font set).
pub const THEMES: &[(&str, usize, usize)] = &[("Office", 0, 0), ("Facet", 2, 3), ("Ion", 1, 6), ("Retrospect", 5, 1), ("Slice", 4, 4), ("Wisp", 3, 5), ("Grayscale", 6, 2)];

/// Paragraph spacing presets: (name, before pt, after pt, line multiple).
pub const SPACING: &[(&str, f32, f32, f32)] = &[("No Paragraph Space", 0.0, 0.0, 1.0), ("Compact", 0.0, 4.0, 1.0), ("Tight", 0.0, 6.0, 1.15), ("Open", 0.0, 10.0, 1.15), ("Relaxed", 0.0, 6.0, 1.5), ("Double", 0.0, 8.0, 2.0)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stories_styles_page_and_drop_caps_round_trip() {
        let mut d = Document::new(vec![Paragraph::new("Once upon a time there was a document that wrapped around a large initial letter.")]);
        d.properties.title = "Report".into();
        d.set_story(true, Some(d.story_design(StoryDesign::Line, true)));
        d.insert_page_numbers(false, 1, Alignment::Right);
        d.apply_style_set(3);
        d.apply_fonts("Cambria", "Calibri");
        d.apply_paragraph_spacing(0.0, 6.0, 1.15);
        d.set_page_color(Some(Color(0xFF, 0xF2, 0xCC)));
        d.set_page_borders(Some(crate::page::PageBorders { style: crate::page::BorderStyle::Double, width: 1.5, color: Color(0x44, 0x72, 0xC4), space: 24.0, from_text: false, sides: [true; 4] }));
        d.set_watermark(Some(Watermark::Text { text: "DRAFT".into(), font: "Calibri".into(), size: 0.0, color: Color(0xC0, 0xC0, 0xC0), diagonal: true }));
        d.set_drop_cap(0, Some((DropCap { margin: false, lines: 3, distance: 0 }, None)));
        assert_eq!(d.paragraphs.len(), 2);
        assert_eq!(d.paragraphs[0].text(), "O");
        assert!(d.paragraphs[1].text().starts_with("nce"));
        let bytes = crate::docx::write(&d).unwrap();
        let e = crate::docx::read(&bytes).unwrap();
        assert_eq!(e.header.as_ref().unwrap().paragraphs.iter().map(|p| p.text()).collect::<String>().trim_start_matches(crate::chars::OBJECT), "REPORT");
        assert!(e.footer.as_ref().unwrap().paragraphs[0].runs.iter().any(|r| r.field == Some(Field::NumPages)));
        assert!(e.watermark().is_some(), "the watermark survives");
        assert_eq!(e.page.color, Some(Color(0xFF, 0xF2, 0xCC)));
        assert_eq!(e.page.borders.as_ref().map(|b| b.style), Some(crate::page::BorderStyle::Double));
        assert_eq!(e.styles.styles["Heading1"].run.caps, Some(crate::format::Caps::AllCaps));
        assert_eq!(e.styles.doc_run.font.as_deref(), Some("Calibri"));
        assert_eq!(e.paragraphs[0].props.drop_cap.map(|c| c.lines), Some(3));
        // Removing the drop cap puts the letter back.
        let mut f = e;
        f.set_drop_cap(1, None);
        assert_eq!(f.paragraphs.len(), 1);
        assert!(f.paragraphs[0].text().starts_with("Once"));
        // Undo restores a removed header.
        f.set_story(true, None);
        f.undo();
        assert!(f.header.is_some());
    }
}
