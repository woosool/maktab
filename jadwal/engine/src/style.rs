//! Cell formatting (format.js, xlsx-styles.js): font, fill, alignment, borders and number format,
//! interned per workbook so a cell holds a small index.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum HAlign { #[default] General, Left, Center, Right, Fill, Justify, CenterAcross, Distributed }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum VAlign { Top, Center, #[default] Bottom, Justify, Distributed }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum LineStyle { #[default] None, Thin, Medium, Thick, Dashed, Dotted, Double, Hair, MediumDashed, DashDot, MediumDashDot, DashDotDot, MediumDashDotDot, SlantDashDot }

impl LineStyle {
    pub const ALL: [(LineStyle, &'static str); 14] = [
        (LineStyle::None, "none"), (LineStyle::Thin, "thin"), (LineStyle::Medium, "medium"), (LineStyle::Thick, "thick"), (LineStyle::Dashed, "dashed"), (LineStyle::Dotted, "dotted"),
        (LineStyle::Double, "double"), (LineStyle::Hair, "hair"), (LineStyle::MediumDashed, "mediumDashed"), (LineStyle::DashDot, "dashDot"), (LineStyle::MediumDashDot, "mediumDashDot"),
        (LineStyle::DashDotDot, "dashDotDot"), (LineStyle::MediumDashDotDot, "mediumDashDotDot"), (LineStyle::SlantDashDot, "slantDashDot"),
    ];
    pub fn name(&self) -> &'static str { Self::ALL.iter().find(|x| x.0 == *self).unwrap().1 }
    pub fn parse(s: &str) -> LineStyle { Self::ALL.iter().find(|x| x.1 == s).map_or(LineStyle::None, |x| x.0) }
    /// Line width in pixels.
    pub fn width(&self) -> f32 { match self { LineStyle::None => 0.0, LineStyle::Hair => 0.5, LineStyle::Medium | LineStyle::MediumDashed | LineStyle::MediumDashDot | LineStyle::MediumDashDotDot | LineStyle::SlantDashDot => 2.0, LineStyle::Thick => 3.0, LineStyle::Double => 3.0, _ => 1.0 } }
}

/// An RGB color.
pub type Rgb = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Border { pub style: LineStyle, pub color: Option<Rgb> }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Underline { #[default] None, Single, Double, SingleAccounting, DoubleAccounting }

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Style {
    pub font: String,
    /// Font size in tenths of a point.
    pub size: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: Underline,
    pub strike: bool,
    pub color: Option<Rgb>,
    /// Superscript (1) or subscript (-1).
    pub script: i8,
    pub fill: Option<Rgb>,
    pub h: HAlign,
    pub v: VAlign,
    pub wrap: bool,
    pub shrink: bool,
    pub indent: u8,
    /// Text rotation in degrees (-90..90), or 255 for stacked letters.
    pub rotation: i16,
    pub num_fmt: String,
    pub left: Border,
    pub right: Border,
    pub top: Border,
    pub bottom: Border,
    pub diag_up: bool,
    pub diag_down: bool,
    pub diagonal: Border,
    pub locked: bool,
    pub hidden: bool,
    /// Right-to-left reading order (2), left-to-right (1), or context (0).
    pub reading: u8,
    /// The theme colors `color` and `fill` were picked as (kept while they still match; see theme.rs).
    pub color_th: Option<crate::theme::ThemeColor>,
    pub fill_th: Option<crate::theme::ThemeColor>,
    /// The font is the theme's body (1) or headings (2) font while it still matches; 0 = a plain font.
    pub font_scheme: u8,
}

pub const DEFAULT_FONT: &str = "Calibri";

impl Default for Style {
    fn default() -> Self {
        Style { font: DEFAULT_FONT.into(), size: 110, bold: false, italic: false, underline: Underline::None, strike: false, color: None, script: 0, fill: None,
            h: HAlign::General, v: VAlign::Bottom, wrap: false, shrink: false, indent: 0, rotation: 0, num_fmt: "General".into(),
            left: Border::default(), right: Border::default(), top: Border::default(), bottom: Border::default(), diag_up: false, diag_down: false, diagonal: Border::default(),
            locked: true, hidden: false, reading: 0, color_th: None, fill_th: None, font_scheme: 1 }
    }
}

impl Style {
    pub fn size_pt(&self) -> f32 { self.size as f32 / 10.0 }
}

/// The workbook's table of distinct styles; index 0 is the default.
#[derive(Clone, Debug)]
pub struct Styles { list: Vec<Style>, index: std::collections::HashMap<Style, u32>, theme: (Vec<Rgb>, String, String) }

impl Default for Styles {
    fn default() -> Self { let mut s = Styles { list: vec![], index: Default::default(), theme: (crate::numfmt::OFFICE_THEME.to_vec(), "Calibri Light".into(), DEFAULT_FONT.into()) }; s.intern(Style::default()); s }
}

impl Styles {
    pub fn get(&self, i: u32) -> &Style { self.list.get(i as usize).unwrap_or(&self.list[0]) }
    /// The theme references are checked against (colors, headings font, body font).
    pub fn set_theme(&mut self, colors: Vec<Rgb>, major: String, minor: String) { self.theme = (colors, major, minor); }
    /// Theme references that no longer describe the style (its color or font was changed by hand) dropped.
    fn normalize(&self, s: &mut Style) {
        let (c, major, minor) = &self.theme;
        if s.color_th.is_some_and(|t| s.color != Some(t.resolve(c))) { s.color_th = None; }
        if s.fill_th.is_some_and(|t| s.fill != Some(t.resolve(c))) { s.fill_th = None; }
        if (s.font_scheme == 1 && s.font != *minor) || (s.font_scheme == 2 && s.font != *major) { s.font_scheme = 0; }
    }
    pub fn intern(&mut self, mut s: Style) -> u32 {
        self.normalize(&mut s);
        if let Some(&i) = self.index.get(&s) { return i; }
        let i = self.list.len() as u32;
        self.index.insert(s.clone(), i);
        self.list.push(s);
        i
    }
    pub fn len(&self) -> usize { self.list.len() }
    pub fn is_empty(&self) -> bool { self.list.is_empty() }
    pub fn all(&self) -> &[Style] { &self.list }
    /// Replace every style in place (indexes keep their meaning), as applying a theme does.
    pub fn replace_all(&mut self, mut list: Vec<Style>) {
        if list.len() != self.list.len() { return; }
        for s in &mut list { self.normalize(s); }
        self.index.clear();
        for (i, s) in list.iter().enumerate().rev() { self.index.insert(s.clone(), i as u32); }
        self.list = list;
    }
    /// Replace the default style (index 0), as a file's Normal style does.
    pub fn set_default(&mut self, mut s: Style) {
        self.normalize(&mut s);
        self.index.remove(&self.list[0]);
        self.list[0] = s.clone();
        self.index.insert(s, 0);
    }
}
