//! Run and paragraph formatting. `RunProps`/`ParaProps` hold declared
//! (possibly absent) properties as read from styles and direct formatting;
//! `RunFormat`/`ParaFormat` are the fully resolved values used by layout.
//! Spacing is in points; indents in twips, as in the JS engine.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub const BLACK: Color = Color(0, 0, 0);
    pub const WHITE: Color = Color(255, 255, 255);
    pub fn from_hex(s: &str) -> Option<Color> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 { return None; }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Color((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
    pub fn hex(&self) -> String { format!("{:02X}{:02X}{:02X}", self.0, self.1, self.2) }
}

/// The OOXML highlight palette (§17.18.40).
pub const HIGHLIGHTS: &[(&str, Color)] = &[
    ("yellow", Color(255, 255, 0)), ("green", Color(0, 255, 0)), ("cyan", Color(0, 255, 255)),
    ("magenta", Color(255, 0, 255)), ("blue", Color(0, 0, 255)), ("red", Color(255, 0, 0)),
    ("darkBlue", Color(0, 0, 139)), ("darkCyan", Color(0, 139, 139)), ("darkGreen", Color(0, 100, 0)),
    ("darkMagenta", Color(128, 0, 128)), ("darkRed", Color(139, 0, 0)), ("darkYellow", Color(128, 128, 0)),
    ("darkGray", Color(169, 169, 169)), ("lightGray", Color(211, 211, 211)), ("black", Color(0, 0, 0)),
    ("white", Color(255, 255, 255)),
];
pub fn highlight_color(name: &str) -> Option<Color> {
    HIGHLIGHTS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// A soft glow around text: color and radius in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow { pub color: Color, pub radius: f32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Strike { #[default] None, Single, Double }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VertAlign { #[default] Baseline, Superscript, Subscript }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Caps { #[default] Normal, AllCaps, SmallCaps }

/// Declared run properties. `None` means "inherit".
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RunProps {
    pub font: Option<String>,
    pub size: Option<f32>, // points
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<String>, // OOXML ST_Underline value; "none" clears
    pub strike: Option<Strike>,
    pub color: Option<Option<Color>>, // Some(None) = auto
    pub highlight: Option<Option<String>>, // Some(None) = none
    pub shading: Option<Option<Color>>,
    pub vert_align: Option<VertAlign>,
    pub caps: Option<Caps>,
    pub spacing: Option<f32>, // character spacing, points
    pub vanish: Option<bool>,
    /// Right-to-left run (w:rtl) and complex-script font, size and emphasis (w:rFonts/@cs, szCs, bCs, iCs).
    pub rtl: Option<bool>,
    /// Legacy text effects (w:outline, w:shadow).
    pub outline: Option<bool>,
    pub shadow: Option<bool>,
    /// Word 2010 effects (w14:glow, w14:reflection, w14:textFill gradient). Some(None) clears.
    pub glow: Option<Option<Glow>>,
    pub reflection: Option<bool>,
    pub gradient: Option<Option<(Color, Color)>>,
    pub cs_font: Option<String>,
    pub size_cs: Option<f32>,
    pub bold_cs: Option<bool>,
    pub italic_cs: Option<bool>,
}

macro_rules! overlay {
    ($self:ident, $other:ident; $($f:ident),*) => { $( if $other.$f.is_some() { $self.$f = $other.$f.clone(); } )* };
}

impl RunProps {
    pub fn overlay(&mut self, o: &RunProps) {
        overlay!(self, o; font, size, bold, italic, underline, strike, color, highlight, shading, vert_align, caps, spacing, vanish, rtl, outline, shadow, glow, reflection, gradient, cs_font, size_cs, bold_cs, italic_cs);
    }
    pub fn is_empty(&self) -> bool { *self == RunProps::default() }
}

/// Fully resolved run formatting used by layout and painting.
#[derive(Clone, Debug, PartialEq)]
pub struct RunFormat {
    pub font: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: Strike,
    pub color: Option<Color>,
    pub highlight: Option<Color>,
    pub shading: Option<Color>,
    pub vert_align: VertAlign,
    pub caps: Caps,
    pub spacing: f32,
    pub vanish: bool,
    pub rtl: bool,
    pub outline: bool,
    pub shadow: bool,
    pub glow: Option<Glow>,
    pub reflection: bool,
    pub gradient: Option<(Color, Color)>,
    /// Complex-script font (None: the run's font) and size/emphasis.
    pub cs_font: Option<String>,
    pub size_cs: f32,
    pub bold_cs: bool,
    pub italic_cs: bool,
}

impl Default for RunFormat {
    fn default() -> Self {
        RunFormat { font: "Noto Sans".into(), size: 12.0, bold: false, italic: false, underline: false, strike: Strike::None, color: None, highlight: None, shading: None, vert_align: VertAlign::Baseline, caps: Caps::Normal, spacing: 0.0, vanish: false, rtl: false, outline: false, shadow: false, glow: None, reflection: false, gradient: None, cs_font: None, size_cs: 12.0, bold_cs: false, italic_cs: false }
    }
}

impl RunFormat {
    pub fn apply(&mut self, p: &RunProps) {
        if let Some(v) = &p.font { self.font = v.clone(); }
        if let Some(v) = p.size { self.size = v; }
        if let Some(v) = p.bold { self.bold = v; }
        if let Some(v) = p.italic { self.italic = v; }
        if let Some(v) = &p.underline { self.underline = v != "none"; }
        if let Some(v) = p.strike { self.strike = v; }
        if let Some(v) = p.color { self.color = v; }
        if let Some(v) = &p.highlight { self.highlight = v.as_deref().and_then(highlight_color); }
        if let Some(v) = p.shading { self.shading = v; }
        if let Some(v) = p.vert_align { self.vert_align = v; }
        if let Some(v) = p.caps { self.caps = v; }
        if let Some(v) = p.spacing { self.spacing = v; }
        if let Some(v) = p.vanish { self.vanish = v; }
        if let Some(v) = p.rtl { self.rtl = v; }
        if let Some(v) = p.outline { self.outline = v; }
        if let Some(v) = p.shadow { self.shadow = v; }
        if let Some(v) = p.glow { self.glow = v; }
        if let Some(v) = p.reflection { self.reflection = v; }
        if let Some(v) = p.gradient { self.gradient = v; }
        if let Some(v) = &p.cs_font { self.cs_font = Some(v.clone()); }
        if let Some(v) = p.size_cs { self.size_cs = v; }
        if let Some(v) = p.bold_cs { self.bold_cs = v; }
        if let Some(v) = p.italic_cs { self.italic_cs = v; }
    }

    /// The formatting complex-script text uses (Arabic, Hebrew, or any run marked rtl).
    pub fn complex(&self) -> RunFormat {
        RunFormat { font: self.cs_font.clone().unwrap_or_else(|| self.font.clone()), size: self.size_cs, bold: self.bold_cs, italic: self.italic_cs, ..self.clone() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Alignment { #[default] Left, Center, Right, Justify }

impl Alignment {
    pub fn from_ooxml(v: &str) -> Alignment {
        match v { "center" => Alignment::Center, "right" | "end" => Alignment::Right, "both" | "distribute" => Alignment::Justify, _ => Alignment::Left }
    }
    pub fn ooxml(&self) -> &'static str {
        match self { Alignment::Left => "left", Alignment::Center => "center", Alignment::Right => "right", Alignment::Justify => "both" }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineRule { #[default] Auto, Exact, AtLeast }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind { Left, Center, Right, Decimal, Clear }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leader { None, Dot, Hyphen, Underscore, MiddleDot }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabStop { pub pos: i32, pub kind: TabKind, pub leader: Leader } // pos in twips

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Border { pub width: f32, pub space: f32, pub color: Option<Color> } // width/space points

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ParaBorders { pub top: Option<Border>, pub bottom: Option<Border>, pub left: Option<Border>, pub right: Option<Border> }

/// Declared paragraph properties.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ParaProps {
    pub alignment: Option<Alignment>,
    pub space_before: Option<f32>,
    pub space_after: Option<f32>,
    pub line_rule: Option<LineRule>,
    /// Multiple for `Auto`, points otherwise.
    pub line_spacing: Option<f32>,
    pub indent_left: Option<i32>,
    pub indent_right: Option<i32>,
    /// Negative values are hanging indents.
    pub first_line: Option<i32>,
    pub keep_next: Option<bool>,
    pub keep_lines: Option<bool>,
    pub page_break_before: Option<bool>,
    pub widow_control: Option<bool>,
    pub contextual_spacing: Option<bool>,
    pub outline_level: Option<u8>,
    pub tabs: Option<Vec<TabStop>>,
    pub shading: Option<Option<Color>>,
    pub borders: Option<ParaBorders>,
    /// (numId, ilvl). numId 0 removes numbering.
    pub numbering: Option<(u32, u8)>,
    /// Right-to-left paragraph (w:bidi). Unset paragraphs take the direction of their first strong character.
    pub rtl: Option<bool>,
    /// This paragraph is a drop cap for the next one (w:framePr w:dropCap).
    pub drop_cap: Option<DropCap>,
    pub suppress_line_numbers: Option<bool>,
    pub suppress_hyphens: Option<bool>,
}

/// A drop cap: dropped into the text or in the margin, spanning `lines` lines, `distance`
/// twips from the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropCap { pub margin: bool, pub lines: u8, pub distance: i32 }

impl ParaProps {
    pub fn overlay(&mut self, o: &ParaProps) {
        overlay!(self, o; alignment, space_before, space_after, line_rule, line_spacing, indent_left, indent_right, first_line, keep_next, keep_lines, page_break_before, widow_control, contextual_spacing, outline_level, shading, borders, numbering, rtl, drop_cap, suppress_line_numbers, suppress_hyphens);
        if let Some(t) = &o.tabs {
            // Tab stops accumulate through inheritance; `Clear` removes an inherited stop.
            let mut merged = self.tabs.clone().unwrap_or_default();
            for stop in t {
                merged.retain(|s| s.pos != stop.pos);
                if stop.kind != TabKind::Clear { merged.push(*stop); }
            }
            merged.sort_by_key(|s| s.pos);
            self.tabs = Some(merged);
        }
    }
    pub fn is_empty(&self) -> bool { *self == ParaProps::default() }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParaFormat {
    pub alignment: Alignment,
    pub space_before: f32,
    pub space_after: f32,
    pub line_rule: LineRule,
    pub line_spacing: f32,
    pub indent_left: i32,
    pub indent_right: i32,
    pub first_line: i32,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub widow_control: bool,
    pub contextual_spacing: bool,
    pub outline_level: u8,
    pub tabs: Vec<TabStop>,
    pub shading: Option<Color>,
    pub borders: ParaBorders,
    pub numbering: Option<(u32, u8)>,
    pub rtl: Option<bool>,
    pub drop_cap: Option<DropCap>,
    pub suppress_line_numbers: bool,
    pub suppress_hyphens: bool,
}

impl Default for ParaFormat {
    // PARAGRAPH_FORMAT in formatting.js.
    fn default() -> Self {
        ParaFormat { alignment: Alignment::Left, space_before: 0.0, space_after: 9.0, line_rule: LineRule::Auto, line_spacing: 1.0, indent_left: 0, indent_right: 0, first_line: 0, keep_next: false, keep_lines: false, page_break_before: false, widow_control: true, contextual_spacing: false, outline_level: 9, tabs: vec![], shading: None, borders: ParaBorders::default(), numbering: None, rtl: None, drop_cap: None, suppress_line_numbers: false, suppress_hyphens: false }
    }
}

impl ParaFormat {
    pub fn apply(&mut self, p: &ParaProps) {
        if let Some(v) = p.alignment { self.alignment = v; }
        if let Some(v) = p.space_before { self.space_before = v; }
        if let Some(v) = p.space_after { self.space_after = v; }
        if let Some(v) = p.line_rule { self.line_rule = v; }
        if let Some(v) = p.line_spacing { self.line_spacing = v; }
        if let Some(v) = p.indent_left { self.indent_left = v; }
        if let Some(v) = p.indent_right { self.indent_right = v; }
        if let Some(v) = p.first_line { self.first_line = v; }
        if let Some(v) = p.keep_next { self.keep_next = v; }
        if let Some(v) = p.keep_lines { self.keep_lines = v; }
        if let Some(v) = p.page_break_before { self.page_break_before = v; }
        if let Some(v) = p.widow_control { self.widow_control = v; }
        if let Some(v) = p.contextual_spacing { self.contextual_spacing = v; }
        if let Some(v) = p.outline_level { self.outline_level = v; }
        if let Some(v) = &p.tabs { self.tabs = v.clone(); }
        if let Some(v) = p.shading { self.shading = v; }
        if let Some(v) = &p.borders { self.borders = v.clone(); }
        if let Some(v) = p.numbering { self.numbering = if v.0 == 0 { None } else { Some(v) }; }
        if let Some(v) = p.rtl { self.rtl = Some(v); }
        if let Some(v) = p.drop_cap { self.drop_cap = Some(v); }
        if let Some(v) = p.suppress_line_numbers { self.suppress_line_numbers = v; }
        if let Some(v) = p.suppress_hyphens { self.suppress_hyphens = v; }
    }
}
