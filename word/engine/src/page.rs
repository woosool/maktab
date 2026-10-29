//! Page settings in integer twips (page-settings.js) and their pixel geometry.

#[derive(Clone, Debug, PartialEq)]
pub struct PageSettings {
    pub width: i32,
    pub height: i32,
    pub landscape: bool,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
    pub header: i32,
    pub footer: i32,
    pub gutter: i32,
    pub default_tab_stop: i32,
    pub title_page: bool,
    /// Page background color (w:background, Design ▸ Page Color).
    pub color: Option<crate::Color>,
    /// A border around every page (w:pgBorders).
    pub borders: Option<PageBorders>,
    /// The last section's columns, line numbers and start (the body's w:sectPr).
    pub section: Section,
    /// Automatic hyphenation (settings.xml).
    pub hyphenation: Hyphenation,
    /// Footnote and endnote numbering: (format, start).
    pub footnote_format: (crate::numbering::NumFmt, u32),
    pub endnote_format: (crate::numbering::NumFmt, u32),
}

/// How a section starts (w:type).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SectionKind { #[default] NextPage, Continuous, EvenPage, OddPage }

impl SectionKind {
    pub fn val(&self) -> &'static str { match self { SectionKind::NextPage => "nextPage", SectionKind::Continuous => "continuous", SectionKind::EvenPage => "evenPage", SectionKind::OddPage => "oddPage" } }
    pub fn from_val(v: &str) -> SectionKind { match v { "continuous" => SectionKind::Continuous, "evenPage" => SectionKind::EvenPage, "oddPage" => SectionKind::OddPage, _ => SectionKind::NextPage } }
}

/// Text columns (w:cols): equal columns `space` twips apart, or explicit widths and spaces.
#[derive(Clone, Debug, PartialEq)]
pub struct Columns { pub count: u8, pub space: i32, pub separator: bool, pub widths: Vec<(i32, i32)> }

impl Default for Columns { fn default() -> Self { Columns { count: 1, space: 720, separator: false, widths: vec![] } } }

/// Line numbering (w:lnNumType).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineNumbering { pub start: u32, pub count_by: u32, pub restart: LineRestart, pub distance: i32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineRestart { Page, Section, Continuous }

/// One section's settings; the page size and margins are shared by the document.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Section { pub kind: SectionKind, pub columns: Columns, pub line_numbers: Option<LineNumbering> }

/// Automatic hyphenation settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hyphenation { pub auto: bool, pub zone: i32, pub limit: u8, pub caps: bool }

impl Default for Hyphenation { fn default() -> Self { Hyphenation { auto: false, zone: 360, limit: 0, caps: true } } }

/// Page borders: one line style on the chosen sides, `space` pt from the page edge or the text.
#[derive(Clone, Debug, PartialEq)]
pub struct PageBorders { pub style: BorderStyle, pub width: f32, pub color: crate::Color, pub space: f32, pub from_text: bool, pub sides: [bool; 4] }

/// Line styles for page borders (w:val).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderStyle { Single, Double, Dotted, Dashed, Thick, ThinThick }

impl BorderStyle {
    pub fn val(&self) -> &'static str { match self { BorderStyle::Single => "single", BorderStyle::Double => "double", BorderStyle::Dotted => "dotted", BorderStyle::Dashed => "dashed", BorderStyle::Thick => "thick", BorderStyle::ThinThick => "thinThickSmallGap" } }
    pub fn from_val(v: &str) -> BorderStyle { match v { "double" => BorderStyle::Double, "dotted" => BorderStyle::Dotted, "dashed" | "dashSmallGap" => BorderStyle::Dashed, "thick" => BorderStyle::Thick, v if v.starts_with("thinThick") || v.starts_with("thickThin") => BorderStyle::ThinThick, _ => BorderStyle::Single } }
}

impl Default for PageSettings {
    fn default() -> Self {
        PageSettings { width: 12240, height: 15840, landscape: false, top: 1440, right: 1440, bottom: 1440, left: 1440, header: 720, footer: 720, gutter: 0, default_tab_stop: 720, title_page: false, color: None, borders: None, section: Section::default(), hyphenation: Hyphenation::default(), footnote_format: (crate::numbering::NumFmt::Decimal, 1), endnote_format: (crate::numbering::NumFmt::LowerRoman, 1) }
    }
}

pub const PAPER_SIZES: &[(&str, i32, i32)] = &[("Letter", 12240, 15840), ("A4", 11906, 16838), ("Legal", 12240, 20160)];

/// Page geometry in px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeometry {
    pub width: f32,
    pub height: f32,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    pub header: f32,
    pub footer: f32,
    pub default_tab: f32,
}

pub fn twips(v: i32) -> f32 { v as f32 / 15.0 }

impl PageSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.width < 1440 || self.height < 1440 { return Err("Pages must be at least one inch wide and high.".into()); }
        if self.default_tab_stop < 1 { return Err("Default tab spacing must be greater than zero.".into()); }
        let h = self.left + self.right + self.gutter;
        let v = self.top.abs() + self.bottom.abs();
        if self.width - h < 360 || self.height - v < 360 { return Err("Margins must leave at least a quarter inch of writing space.".into()); }
        Ok(())
    }
    pub fn geometry(&self) -> PageGeometry {
        PageGeometry {
            width: twips(self.width), height: twips(self.height),
            top: twips(self.top.abs()), bottom: twips(self.bottom.abs()),
            left: twips(self.left + self.gutter), right: twips(self.right),
            header: twips(self.header), footer: twips(self.footer),
            default_tab: twips(self.default_tab_stop),
        }
    }
    /// Swap width and height when the orientation changes.
    pub fn set_landscape(&mut self, landscape: bool) {
        if landscape != self.landscape {
            std::mem::swap(&mut self.width, &mut self.height);
            self.landscape = landscape;
        }
    }
}
