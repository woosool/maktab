//! Wrapping, pagination and source-position geometry (layout.js,
//! pagination.js, line-builder.js, tables.js).
//!
//! `layout_document` turns the model into pages of positioned `Line`s (for
//! carets, hit testing and selection) and `DrawItem`s (for painting).

use crate::chars;
use crate::fonts::{self, FontKey};
use crate::format::*;
use crate::model::{CellVAlign, Document, Field, NoteKind, Paragraph, Run, VMerge};
use std::collections::HashMap;
use crate::numbering::MarkerCounter;
use crate::page::{twips, PageGeometry};

const PT: f32 = 4.0 / 3.0; // px per point
/// A cell's margins in px (top, right, bottom, left): its own, else the table's.
fn cell_margins(t: &crate::model::Table, c: &crate::model::Cell) -> [f32; 4] {
    let mut out = [0.0; 4];
    for i in 0..4 { let v = c.margins.map(|m| m[i]).filter(|v| *v >= 0).unwrap_or(t.cell_margins[i]); out[i] = twips(v); }
    out
}

/// `spans`: each cell's range of the table's paragraphs (its own, then tables nested in it).
struct TableGeo { spans: HashMap<u64, (usize, usize)>, col_x: Vec<f32>, borders: bool }

/// Where each of a table's cells starts and ends in its paragraphs (in document order, so a
/// cell runs up to the next cell's first paragraph). Rows then only look at their own cells'
/// paragraphs, keeping long tables linear.
fn cell_spans(table: &crate::model::Table, paragraphs: &[(usize, usize)], doc: &Document) -> HashMap<u64, (usize, usize)> {
    let own: std::collections::HashSet<u64> = table.rows.iter().flat_map(|r| r.cells.iter().map(|c| c.id)).collect();
    let mut starts: Vec<(u64, usize)> = vec![];
    let mut seen = std::collections::HashSet::new();
    // A nested table's paragraphs belong to the cell of this table that holds it.
    let mut owner: HashMap<u64, Option<u64>> = HashMap::new();
    for (k, &(pi, _)) in paragraphs.iter().enumerate() {
        let Some(c) = doc.paragraphs[pi].cell else { continue };
        let o = *owner.entry(c).or_insert_with(|| {
            let mut cur = c;
            for _ in 0..32 {
                if own.contains(&cur) { return Some(cur); }
                cur = doc.table_of_cell(cur).and_then(|t| doc.tables[t].parent)?;
            }
            None
        });
        if let Some(o) = o { if seen.insert(o) { starts.push((o, k)); } }
    }
    (0..starts.len()).map(|i| (starts[i].0, (starts[i].1, starts.get(i + 1).map(|n| n.1).unwrap_or(paragraphs.len())))).collect()
}

/// `Glyph::ch` of a cluster's second and later glyphs (Arabic dots, marks drawn apart): they
/// carry no text of their own, so text extraction doesn't repeat the letter.
pub const CONTINUATION: char = '\u{200B}';

#[derive(Clone, Debug)]
pub struct Glyph {
    /// The character the glyph renders (the first of its cluster), for text extraction.
    pub ch: char,
    pub gid: u16,
    pub x: f32,
    /// Vertical offset from shaping (marks), px up.
    pub dy: f32,
}

/// Legacy text effects drawn with glyphs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Effects {
    pub outline: bool, pub shadow: bool, pub glow: Option<crate::format::Glow>, pub reflection: bool, pub gradient: Option<(Color, Color)>,
    /// Clockwise rotation in degrees about (cx, cy): text in a rotated shape.
    pub rotate: Option<(f32, f32, f32)>,
}

#[derive(Clone, Debug)]
pub enum DrawItem {
    Glyphs { glyphs: Vec<Glyph>, baseline: f32, font: FontKey, size: f32, color: Color, effects: Effects },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Color },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color },
    /// A picture; `rot` is its clockwise rotation in degrees about its center.
    Image { asset: String, x: f32, y: f32, w: f32, h: f32, pos: usize, rot: f32, flip: (bool, bool) },
    /// A filled and/or stroked polygon or polyline (shapes, charts, diagrams).
    /// `ink` is a pen stroke's sequence number (for hiding and replaying ink).
    Path { points: Vec<(f32, f32)>, closed: bool, fill: Option<Color>, stroke: Option<(Color, f32)>, ink: Option<u32> },
    /// An inline object's box, for selecting and resizing it (paints nothing).
    Frame { x: f32, y: f32, w: f32, h: f32, pos: usize },
}

#[derive(Clone, Debug)]
pub struct Line {
    pub page: usize,
    pub y: f32,
    pub height: f32,
    pub baseline: f32,
    /// Content box of the line's paragraph (for hit testing).
    pub left: f32,
    pub right: f32,
    pub start: usize,
    pub end: usize,
    /// x of every caret position start..=end.
    pub stops: Vec<f32>,
    pub paragraph: usize,
    pub last_in_paragraph: bool,
    pub cell: Option<u64>,
    /// Each position's character box (x0, x1), for selections in mixed-direction text.
    pub boxes: Vec<(f32, f32)>,
    /// The paragraph runs right to left.
    pub rtl: bool,
    /// The line mixes directions (or is right to left): arrow keys move visually.
    pub bidi: bool,
}

impl Line {
    /// x where the paragraph mark goes: the line's visual end for its paragraph direction
    /// (rightmost edge for left-to-right, leftmost for right-to-left), not the caret after
    /// the logically last character, which lands inside a run of the other direction.
    pub fn visual_end_x(&self) -> f32 {
        let edges = self.boxes.iter().flat_map(|b| [b.0, b.1]).chain(self.stops.iter().copied());
        let fallback = *self.stops.last().unwrap_or(if self.rtl { &self.right } else { &self.left });
        if self.rtl { edges.fold(fallback, f32::min) } else { edges.fold(fallback, f32::max) }
    }
}

#[derive(Clone, Debug)]
pub struct PageBox {
    pub width: f32,
    pub height: f32,
    pub items: Vec<DrawItem>,
    /// Items painted behind the text (fills, highlights).
    pub under: Vec<DrawItem>,
    /// Hyperlink areas (x, y, w, h, target).
    pub links: Vec<(f32, f32, f32, f32, String)>,
    /// Items painted over the text (objects in front of text).
    pub over: Vec<DrawItem>,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub pages: Vec<PageBox>,
    pub lines: Vec<Line>,
    pub revision: u64,
    /// Every table cell fragment as painted (for cell selection, gridlines and resizing).
    pub cells: Vec<CellBox>,
    /// Each floating object's reference point: (page, x its horizontal offset counts from, its
    /// paragraph's top), so moving it can compute new offsets.
    pub float_origins: HashMap<usize, (usize, f32, f32)>,
}

#[derive(Clone, Debug)]
pub struct CellBox { pub page: usize, pub x0: f32, pub y0: f32, pub x1: f32, pub y1: f32, pub table: usize, pub row: usize, pub col: usize, pub span: usize, pub cell: u64 }

// ---------------------------------------------------------------------------
// Measurement

#[derive(Clone, Debug)]
struct Fmt {
    key: FontKey,
    size: f32, // px, after script scaling
    ascent: f32,
    descent: f32,
    shift: f32, // baseline shift, px (positive = up)
    color: Color,
    underline: bool,
    strike: Strike,
    highlight: Option<Color>,
    hidden: bool,
    spacing: f32,
    ul_offset: f32,
    ul_thickness: f32,
    strike_offset: f32,
    link: Option<String>,
    /// Part of a tracked change (draws a change bar).
    revision: bool,
    effects: Effects,
}

/// Author colors for tracked changes (REVISION_COLORS in revisions.js).
const REVISION_COLORS: [Color; 8] = [Color(0xB5, 0x08, 0x2E), Color(0x2E, 0x6E, 0xB5), Color(0x2F, 0x8A, 0x3A), Color(0x8A, 0x3F, 0xB0), Color(0xC2, 0x5E, 0x00), Color(0x00, 0x83, 0x8F), Color(0xA0, 0x30, 0x6E), Color(0x5B, 0x6B, 0x00)];

pub fn revision_color(author: &str) -> Color {
    let mut hash: u32 = 0;
    for c in author.chars() { hash = hash.wrapping_mul(31).wrapping_add(c as u32); }
    REVISION_COLORS[hash as usize % REVISION_COLORS.len()]
}

/// Markup views (revisionDisplay): All shows insertions underlined and deletions struck in the
/// author's color; Simple and None show the result; Original shows the text before the changes.
fn apply_revision_display(f: &mut Fmt, rv: &crate::model::Revision, markup: crate::model::Markup) {
    use crate::model::{Markup, RevKind};
    let ins = rv.kind == RevKind::Ins;
    match markup {
        Markup::All => {
            f.color = revision_color(&rv.author);
            if ins { f.underline = true; } else { f.strike = Strike::Single; }
            f.revision = true;
        }
        Markup::Simple => { if ins { f.revision = true; } else { f.hidden = true; } }
        Markup::None => { if !ins { f.hidden = true; } }
        Markup::Original => { if ins { f.hidden = true; } }
    }
}

fn make_fmt(f: &RunFormat) -> Fmt {
    let key = fonts::resolve(&f.font, f.bold, f.italic);
    let m = fonts::metrics(key);
    let full = f.size * PT;
    let (size, shift) = match f.vert_align {
        VertAlign::Superscript => (full * 0.65, full * 0.33),
        VertAlign::Subscript => (full * 0.65, -full * 0.14),
        VertAlign::Baseline => (full, 0.0),
    };
    Fmt {
        key, size, ascent: m.ascent * full, descent: m.descent * full, shift,
        color: f.color.unwrap_or(Color::BLACK), underline: f.underline, strike: f.strike,
        highlight: f.highlight.or(f.shading), hidden: f.vanish, spacing: f.spacing * PT,
        ul_offset: m.underline_offset * size, ul_thickness: (m.underline_thickness * size).max(1.0), strike_offset: m.strike_offset * size, link: None, revision: false, effects: Effects { outline: f.outline, shadow: f.shadow, glow: f.glow, reflection: f.reflection, gradient: f.gradient, rotate: None },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind { Char, Space, Tab, LineBreak, PageBreak, Image, Field, Note }

#[derive(Clone, Debug)]
struct Atom {
    ch: char,
    pos: Option<usize>,
    width: f32,
    fmt: usize,
    kind: Kind,
    /// A picture or object box: (asset, width, height, rotation).
    image: Option<(String, f32, f32, f32, (bool, bool))>,
    leader: Leader,
    note: Option<(NoteKind, String)>,
    /// The field whose result this atom shows (its first atom carries the width).
    field: Option<Field>,
    /// The face drawing this character (script fallback), when it differs from the format's.
    key: Option<FontKey>,
    /// Bidi embedding level (odd = right to left).
    level: u8,
    /// The shaped glyphs of the cluster this character starts, as (first, count) in the
    /// paragraph's glyph array, positioned from the character's left edge.
    glyphs: (u32, u16),
    /// A shape, chart or diagram drawn in this atom's box (`image` holds its size).
    object: Option<std::sync::Arc<crate::objects::Object>>,
}

#[derive(Clone, Copy, Debug)]
struct PGlyph { gid: u16, dx: f32, dy: f32 }

/// One measured paragraph line before placement.
#[derive(Clone, Debug)]
struct MLine {
    atoms: Vec<Atom>,
    x: Vec<f32>, // x of each atom, relative to the content box left
    height: f32,
    baseline: f32, // from line top
    start: usize,
    end: usize,
    page_break_after: bool,
    /// The line ends with a column break.
    column_break_after: bool,
    last: bool,
    left: f32,
    right: f32,
    /// Atom indices in visual (left-to-right) order.
    order: Vec<usize>,
    /// Atoms before this index are content; the rest are trailing spaces and breaks.
    trail: usize,
    rtl: bool,
    /// The line ends inside a word hyphenated here: a hyphen is drawn after it.
    hyphen: bool,
    /// Space left above the line to clear a floating object (wrap top and bottom, no room beside).
    skip: f32,
}

#[derive(Clone)]
struct Measured { lines: Vec<MLine>, format: ParaFormat, fmts: Vec<Fmt>, paragraph: usize, style: Option<String>, glyphs: std::sync::Arc<Vec<PGlyph>>, floats: Vec<FloatSpec>, start: usize, marker: Option<String> }

/// A floating picture or object anchored in a paragraph.
#[derive(Clone, Debug)]
struct FloatSpec { pos: usize, w: f32, h: f32, anchor: crate::model::Anchor, asset: String, rot: f32, flip: (bool, bool), object: Option<std::sync::Arc<crate::objects::Object>>, contour: Option<Vec<(f32, f32)>> }

/// Space a wrapping float takes from lines, in content-box x and paragraph-relative y.
#[derive(Clone, Debug)]
struct Band { x0: f32, x1: f32, y0: f32, y1: f32, full: bool, poly: Option<std::sync::Arc<Vec<(f32, f32)>>>, dist: f32 }

/// A float placed on a page (body coordinates).
#[derive(Clone, Debug)]
struct PlacedFloat { page: usize, pos: usize, x0: f32, y0: f32, x1: f32, y1: f32, wrap: crate::model::Wrap, dist: f32, poly: Option<std::sync::Arc<Vec<(f32, f32)>>> }

struct Ctx<'a> {
    doc: &'a Document,
    page: PageGeometry,
    markers: MarkerCounter,
    page_number: usize,
    page_count: usize,
    cache: MeasureCache,
    note_labels: HashMap<(NoteKind, String), String>,
    /// Pages of bookmarks, from a first layout pass (PAGEREF results).
    bookmark_pages: HashMap<String, usize>,
    /// Pages of body PAGE fields by position, from a first layout pass.
    field_pages: HashMap<usize, usize>,
    /// SEQ numbers by position and citation numbers by tag, computed on first use.
    refs: std::sync::OnceLock<(HashMap<usize, u32>, HashMap<String, usize>)>,
}

struct CacheEntry { paragraph: Paragraph, marker: Option<String>, notes: Vec<String>, width: f32, epoch: (u64, i32), start: usize, measured: Measured }

/// Measured paragraphs from the previous layout (paragraph-measure-cache.js). An entry is reused
/// when the paragraph, its list marker, the available width and the formatting epoch all match;
/// its document positions shift to the paragraph's new start.
#[derive(Default)]
pub struct MeasureCache { entries: std::collections::HashMap<u64, CacheEntry>, seen: std::collections::HashSet<u64>, pub hits: usize, pub misses: usize }

fn shift_measured(m: &mut Measured, delta: i64) {
    if delta == 0 { return; }
    let mv = |v: &mut usize| *v = (*v as i64 + delta) as usize;
    for l in &mut m.lines {
        mv(&mut l.start); mv(&mut l.end);
        for a in &mut l.atoms { if let Some(p) = &mut a.pos { mv(p); } }
    }
    mv(&mut m.start);
    for f in &mut m.floats { mv(&mut f.pos); }
}

/// Han, kana and hangul letters, which Word spaces apart from Latin letters and digits.
fn ideograph(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF)
}

fn display_char(c: char, f: &RunFormat) -> char {
    if f.caps != Caps::Normal { c.to_uppercase().next().unwrap_or(c) } else { c }
}

impl<'a> Ctx<'a> {
    /// A field's displayed result.
    fn field_text(&self, field: &Field, pos: Option<usize>) -> String {
        let refs = || self.refs.get_or_init(|| (crate::references::seq_numbers(self.doc), crate::references::citation_numbers(self.doc)));
        match field {
            Field::Seq(_) => pos.and_then(|p| refs().0.get(&p)).map(|n| n.to_string()).unwrap_or_else(|| "1".into()),
            Field::Citation(tag) => crate::references::citation_text(self.doc, tag, &refs().1),
            Field::IndexEntry(_) | Field::Authority { .. } => String::new(),
            Field::Merge(_) | Field::AddressBlock | Field::GreetingLine { .. } | Field::MergeIf { .. } | Field::MergeRec | Field::MergeSeq | Field::NextRecord | Field::NextIf { .. } | Field::SkipIf { .. } => self.doc.mail_merge.field_text(field),
            Field::Page => pos.and_then(|p| self.field_pages.get(&p)).map(|p| p + 1).unwrap_or(self.page_number).to_string(),
            Field::NumPages => self.page_count.max(1).to_string(),
            Field::Ref(name) => self.doc.bookmark_text(name).unwrap_or_else(|| "Error! Reference source not found.".into()),
            Field::PageRef(name) => self.bookmark_pages.get(name).map(|p| (p + 1).to_string()).unwrap_or_else(|| "?".into()),
            Field::Date(pic) => crate::fields::date_value(self.doc, pic),
            Field::Prop(name) => crate::fields::property(self.doc, name),
        }
    }

    /// Measure through the cache. Stories (headers/footers) and paragraphs with page fields are
    /// measured afresh, since their text depends on the page.
    fn measure(&mut self, index: usize, p: &Paragraph, start: usize, width: f32) -> Measured {
        let doc = self.doc;
        let marker = doc.para_format(p).numbering.and_then(|(num, ilvl)| self.markers.next(&doc.numbering, num, ilvl));
        let cacheable = index < doc.paragraphs.len() && p.runs.iter().all(|r| r.field.is_none());
        if !cacheable { return self.measure_with(index, p, start, width, marker, &[], None); }
        let epoch = (doc.format_epoch, doc.page.default_tab_stop);
        let notes: Vec<String> = p.runs.iter().filter_map(|r| r.note.as_ref()).map(|n| self.note_labels.get(&(n.kind, n.id.clone())).cloned().unwrap_or_default()).collect();
        self.cache.seen.insert(p.id);
        if let Some(e) = self.cache.entries.get(&p.id) {
            if e.width == width && e.epoch == epoch && e.marker == marker && e.notes == notes && e.paragraph == *p {
                let mut m = e.measured.clone();
                shift_measured(&mut m, start as i64 - e.start as i64);
                m.paragraph = index;
                self.cache.hits += 1;
                return m;
            }
        }
        self.cache.misses += 1;
        let m = self.measure_with(index, p, start, width, marker.clone(), &[], None);
        self.cache.entries.insert(p.id, CacheEntry { paragraph: p.clone(), marker, notes, width, epoch, start, measured: m.clone() });
        m
    }

    /// Build atoms and break a paragraph into lines for a content width (measureParagraph).
    /// `bands` are the floats lines must avoid, relative to the paragraph's first line top.
    /// `resume` measures only from that document position on (the rest of a paragraph after a
    /// page break, rewrapped for the new page's floats).
    fn measure_with(&mut self, index: usize, p: &Paragraph, start: usize, width: f32, marker: Option<String>, bands: &[Band], resume: Option<usize>) -> Measured {
        let doc = self.doc;
        let marker_text = marker.clone();
        let mut floats: Vec<FloatSpec> = vec![];
        let can_float = true;
        let mut format = doc.para_format(p);
        // A display equation (m:oMathPara) is centered and left-to-right, whatever the paragraph says.
        if p.runs.iter().any(|r| matches!(&r.object, Some(crate::objects::Object::Equation(e)) if e.display)) && p.runs.iter().all(|r| r.object.is_some() || r.text.trim().is_empty()) {
            format.alignment = crate::format::Alignment::Center;
            format.rtl = Some(false);
        }
        let mut fmts: Vec<Fmt> = vec![];
        let mark = make_fmt(&doc.mark_format(p));
        fmts.push(mark.clone());
        let mut atoms: Vec<Atom> = vec![];

        // List marker followed by a tab to the hanging indent.
        if let Some((num, ilvl)) = format.numbering {
            if let Some(text) = marker {
                let level = doc.numbering.level(num, ilvl).cloned();
                let mut mf = doc.mark_format(p);
                if let Some(Some(font)) = level.as_ref().map(|l| l.font.clone()) {
                    // Symbol bullet fonts map to Noto's bullet glyphs.
                    if !font.eq_ignore_ascii_case("symbol") && !font.eq_ignore_ascii_case("wingdings") { mf.font = font; }
                }
                mf.underline = false;
                let f = make_fmt(&mf);
                fmts.push(f.clone());
                let fi = fmts.len() - 1;
                for c in text.chars() {
                    let c = match c {
                        '\u{F0B7}' | '\u{F0A7}' | '\u{F06C}' => '\u{2022}', '\u{F0D8}' => '\u{27A2}', '\u{F0FC}' => '\u{2713}', 'o' if text.len() == 1 => '\u{25E6}',
                        // Any other symbol-font code would be drawn from whatever font has that
                        // private-use code (often an icon font): the level's default bullet instead.
                        '\u{F000}'..='\u{F0FF}' => ['\u{2022}', '\u{25E6}', '\u{25AA}'][ilvl as usize % 3],
                        c => c,
                    };
                    atoms.push(Atom { ch: c, pos: None, width: fonts::advance(f.key, c, f.size), fmt: fi, kind: Kind::Char, image: None, leader: Leader::None, note: None, field: None, key: None, level: 0, glyphs: (0, 0), object: None });
                }
                atoms.push(Atom { ch: '\t', pos: None, width: 0.0, fmt: fi, kind: Kind::Tab, image: None, leader: Leader::None, note: None, field: None, key: None, level: 0, glyphs: (0, 0), object: None });
            }
        }

        let mut pos = start;
        for run in &p.runs {
            let rf = doc.run_format(p, run);
            // Latin text uses the run's format; Arabic/Hebrew (and rtl runs) its complex-script one.
            let mut f = make_fmt(&rf);
            let mut fcs = make_fmt(&rf.complex());
            for x in [&mut f, &mut fcs] {
                x.link = run.link.clone();
                if let Some(rv) = &run.revision { apply_revision_display(x, rv, doc.markup); }
            }
            let fi = intern(&mut fmts, f.clone());
            let fi_cs = intern(&mut fmts, fcs);
            if let (Some(anchor), true) = (run.anchor, can_float && (run.object.is_some() || run.image.is_some())) {
                // A float is a zero-width atom in the text; the placer draws it at its offset.
                let (w, h, asset, rot, flip) = match (&run.object, &run.image) { (Some(o), _) => { let (w, h) = o.size(); (w, h, String::new(), 0.0, (false, false)) } (_, Some(i)) => (i.width, i.height, i.asset.clone(), i.rotation, (i.flip_h, i.flip_v)), _ => (0.0, 0.0, String::new(), 0.0, (false, false)) };
                let contour = if anchor.wrap == crate::model::Wrap::Tight && run.image.is_some() { doc.assets.get(&asset).and_then(|a| crate::picture::contour(&asset, &a.bytes)).map(|c| c.iter().map(|&(u, v)| (u * w, v * h)).collect()) } else { None };
                floats.push(FloatSpec { pos, w, h, anchor, asset, rot, flip, object: run.object.clone().map(std::sync::Arc::new), contour });
                atoms.push(Atom { ch: chars::OBJECT, pos: Some(pos), width: 0.0, fmt: fi, kind: Kind::Image, image: None, leader: Leader::None, note: None, field: None, key: None, level: 0, glyphs: (0, 0), object: None });
                pos += run.len();
                continue;
            }
            if let Some(obj) = &run.object {
                let (w, h) = obj.size();
                let rot = match obj { crate::objects::Object::Shape(s) => s.rotation, _ => 0.0 };
                let (bw, _) = rotated_bbox(w, h, rot);
                atoms.push(Atom { ch: chars::OBJECT, pos: Some(pos), width: bw, fmt: fi, kind: Kind::Image, image: Some((String::new(), w, h, rot, (false, false))), leader: Leader::None, note: None, field: None, key: None, level: 0, glyphs: (0, 0), object: Some(std::sync::Arc::new(obj.clone())) });
                pos += run.len();
                continue;
            }
            if let Some(img) = &run.image {
                let (bw, _) = rotated_bbox(img.width, img.height, img.rotation);
                atoms.push(Atom { ch: chars::OBJECT, pos: Some(pos), width: bw, fmt: fi, kind: Kind::Image, image: Some((img.asset.clone(), img.width, img.height, img.rotation, (img.flip_h, img.flip_v))), leader: Leader::None, note: None, field: None, key: None, level: 0, glyphs: (0, 0), object: None });
                pos += run.len();
                continue;
            }
            if let Some(n) = &run.note {
                let label = self.note_labels.get(&(n.kind, n.id.clone())).cloned().unwrap_or_else(|| "?".into());
                let mut nf = f.clone();
                if nf.shift == 0.0 { let full = nf.size; nf.size = full * 0.65; nf.shift = full * 0.33; }
                let ni = match fmts.iter().position(|x| x.key == nf.key && x.size == nf.size && x.shift == nf.shift && x.color == nf.color && x.revision == nf.revision && x.link.is_none()) { Some(i) => i, None => { fmts.push(nf.clone()); fmts.len() - 1 } };
                let w: f32 = label.chars().map(|c| fonts::advance(nf.key, c, nf.size)).sum();
                atoms.push(Atom { ch: chars::NOTE, pos: Some(pos), width: if nf.hidden { 0.0 } else { w }, fmt: ni, kind: Kind::Note, image: None, leader: Leader::None, note: Some((n.kind, n.id.clone())), field: None, key: None, level: 0, glyphs: (0, 0), object: None });
                pos += run.len();
                continue;
            }
            if let Some(field) = run.field.clone() {
                let text = self.field_text(&field, Some(pos));
                let w = crate::objects::shape_bidi(&text, false, f.key, f.size, 0.0).1;
                let n = run.len();
                // A field's result is one unit for editing purposes: the first atom carries its width.
                for (k, _) in run.text.chars().enumerate() {
                    atoms.push(Atom { ch: ' ', pos: Some(pos + k), width: if k == 0 { w } else { 0.0 }, fmt: fi, kind: Kind::Field, image: None, leader: Leader::None, note: None, field: if k == 0 { Some(field.clone()) } else { None }, key: None, level: 0, glyphs: (0, 0), object: None });
                }
                pos += n;
                continue;
            }
            // Word gives every character of a right-to-left run (w:rtl) its complex-script
            // properties, spaces and punctuation included; elsewhere neutrals between Arabic
            // letters follow the letter before them, so a title's spaces match its words.
            let mut prev_cs = false;
            for c in run.text.chars() {
                let kind = match c {
                    '\t' => Kind::Tab, c if chars::ptab_kind(c).is_some() => Kind::Tab, chars::LINE_BREAK => Kind::LineBreak,
                    chars::PAGE_BREAK | chars::COLUMN_BREAK => Kind::PageBreak,
                    ' ' | '\u{00A0}' => Kind::Space, _ => Kind::Char,
                };
                let shown = display_char(c, &rf);
                let strong = c.is_alphanumeric() && !fonts::is_complex(c);
                let cs = fonts::is_complex(c) || rf.rtl && matches!(kind, Kind::Char | Kind::Space) || prev_cs && !strong && matches!(kind, Kind::Char | Kind::Space);
                if fonts::is_complex(c) { prev_cs = true; } else if strong { prev_cs = false; }
                let afi = if cs { fi_cs } else { fi };
                let af = &fmts[afi];
                let width = if af.hidden { 0.0 } else { match kind {
                    Kind::Char if c == '\u{AD}' => 0.0,
                    Kind::Char | Kind::Space => fonts::advance(af.key, shown, af.size) + af.spacing,
                    _ => 0.0,
                } };
                atoms.push(Atom { ch: shown, pos: Some(pos), width, fmt: afi, kind, image: None, leader: Leader::None, note: None, field: None, key: None, level: 0, glyphs: (0, 0), object: None });
                // Capitals of ß are two letters (SS, as Word shows); the extra one has no document position.
                if shown != c { for e in c.to_uppercase().skip(1) { let width = if af.hidden { 0.0 } else { fonts::advance(af.key, e, af.size) + af.spacing }; atoms.push(Atom { ch: e, pos: None, width, fmt: afi, kind, image: None, leader: Leader::None, note: None, field: None, key: None, level: 0, glyphs: (0, 0), object: None }); } }
                pos += 1;
            }
        }
        let end = pos;
        let rtl = format.rtl.unwrap_or_else(|| unicode_bidi::get_base_direction(p.text().as_str()) == unicode_bidi::Direction::Rtl);
        let glyphs = std::sync::Arc::new(shape_atoms(&mut atoms, &fmts, rtl));
        // Word's automatic spacing (autoSpaceDE/autoSpaceDN, on by default) puts a quarter em
        // between ideographs and Latin letters or digits.
        for k in 1..atoms.len() {
            let (a, b) = (&atoms[k - 1], &atoms[k]);
            if a.kind == Kind::Char && b.kind == Kind::Char && !fmts[a.fmt].hidden && !fmts[b.fmt].hidden
                && (ideograph(a.ch) && b.ch.is_ascii_alphanumeric() || a.ch.is_ascii_alphanumeric() && ideograph(b.ch)) {
                let add = fmts[a.fmt].size.max(fmts[b.fmt].size) * 0.25;
                atoms[k - 1].width += add;
            }
        }

        // indent_left/right are the start/end indents (§17.3.1.12); in RTL paragraphs start is on the right.
        let indent_left = twips(format.indent_left);
        let indent_right = twips(format.indent_right);
        let first_line = twips(format.first_line);
        let mut lines: Vec<MLine> = vec![];
        let mut i = 0;
        let mut first = true;
        if let Some(r) = resume {
            i = atoms.iter().position(|a| a.pos.is_some_and(|p| p >= r)).unwrap_or(atoms.len());
            first = i == 0;
        }
        let mut y = 0.0f32;
        // Free spans left on the current row beside a float (text flows on both sides), and
        // that row's top.
        let mut pending: Vec<(f32, f32)> = vec![];
        let mut row_top = 0.0f32;
        let est = (fmts[0].ascent + fmts[0].descent) * if format.line_rule == LineRule::Auto { format.line_spacing.max(0.06) } else { 1.0 };
        loop {
            let line_left = indent_left + if first { first_line } else { 0.0 };
            let mut avail = (width - indent_right - line_left).max(8.0);
            let mut phys_left = if rtl { indent_right } else { line_left };
            // Floats: take the widest free span beside them, or move below them.
            let mut skip = 0.0f32;
            let mut banded = false;
            if let Some((a, c)) = (!pending.is_empty()).then(|| pending.remove(0)) {
                // The rest of this row: back up to its top, right of (or, in RTL, left of) the float.
                phys_left = a; avail = c - a; banded = true;
                skip = row_top - y;
            } else if !bands.is_empty() {
                let h = lines.last().map(|l| l.height).unwrap_or(est);
                for _ in 0..64 {
                    let (y0, y1) = (y + skip, y + skip + h);
                    let hits: Vec<&Band> = bands.iter().filter(|b| b.y0 < y1 && b.y1 > y0).collect();
                    if hits.is_empty() { break; }
                    let (lo, hi) = (phys_left, phys_left + avail);
                    let mut spans = vec![(lo, hi)];
                    for b in &hits {
                        if b.full { spans.clear(); break; }
                        // The x range the float covers within this line's height: its box, or
                        // for tight wrap its outline there, widened by the wrap distance.
                        let Some((bx0, bx1)) = (match &b.poly { Some(p) => poly_extent(p, y0 - b.dist, y1 + b.dist).map(|(a, c)| (a - b.dist, c + b.dist)), None => Some((b.x0, b.x1)) }) else { continue };
                        spans = spans.into_iter().flat_map(|(a, c)| {
                            let mut out = vec![];
                            if bx0 > a { out.push((a, bx0.min(c))); }
                            if bx1 < c { out.push((bx1.max(a), c)); }
                            out
                        }).filter(|(a, c)| c > a).collect();
                    }
                    // Spans wide enough for text, in reading order; the first takes this line.
                    let mut usable: Vec<(f32, f32)> = spans.into_iter().filter(|(a, c)| c - a >= 48.0_f32.min(avail)).collect();
                    if rtl { usable.reverse(); }
                    match (!usable.is_empty()).then(|| usable.remove(0)) {
                        Some((a, c)) => { phys_left = a; avail = c - a; banded = true; pending = usable; row_top = y + skip; break; }
                        None => {
                            // No room beside the float: continue under the nearest one.
                            let next = hits.iter().map(|b| b.y1).fold(f32::MAX, f32::min);
                            skip = next - y;
                        }
                    }
                }
            }
            let line_left = if banded && !rtl { phys_left } else { line_left };
            let line_start_atom = i;
            let mut x = 0.0f32;
            let mut xs: Vec<f32> = vec![];
            let mut last_break: Option<usize> = None; // atom index after which we may break
            let mut j = i;
            let mut ended_by: Option<Kind> = None;
            // A right, center or decimal stop past the right margin lets its text reach the
            // stop, as in Word (a resume's dates at a stop beyond the margin).
            let mut limit = avail;
            while j < atoms.len() {
                let kind = atoms[j].kind;
                if kind == Kind::Tab {
                    // Tab stops are relative to the text margin (content box left).
                    let abs = line_left + x;
                    let seg: f32 = atoms[j + 1..].iter().take_while(|a| !matches!(a.kind, Kind::Tab | Kind::LineBreak | Kind::PageBreak)).map(|a| a.width).sum();
                    let (w, leader, aligned) = match chars::ptab_kind(atoms[j].ch) {
                        // A positional tab aligns against the margins or indents, whatever the stops.
                        Some((align, indent)) => {
                            let (lo, hi) = if indent { (indent_left, width - indent_right) } else { (0.0, width) };
                            let pos = match align { "center" => (lo + hi) / 2.0 - seg / 2.0, "right" => hi - seg, _ => lo };
                            ((pos - abs).max(0.0), Leader::None, align != "left")
                        }
                        None => self.tab_width(&format, abs, seg, first && atoms[j].pos.is_none(), indent_left, &atoms[j + 1..]),
                    };
                    if aligned { limit = limit.max(x + w + seg + 0.5); }
                    atoms[j].width = w;
                    atoms[j].leader = leader;
                }
                let w = atoms[j].width;
                // A no-break space (Ctrl+Shift+Space) joins its neighbors onto one line.
                let breakable_space = kind == Kind::Space && atoms[j].ch != '\u{00A0}';
                if !breakable_space && x + w > limit && j > line_start_atom && !matches!(kind, Kind::LineBreak | Kind::PageBreak) {
                    break;
                }
                xs.push(x);
                x += w;
                if matches!(kind, Kind::LineBreak | Kind::PageBreak) { ended_by = Some(kind); j += 1; break; }
                if breakable_space || kind == Kind::Tab || (kind == Kind::Char && matches!(atoms[j].ch, '-' | '\u{2013}' | '\u{2014}' | '/')) || kind == Kind::Image {
                    last_break = Some(j + 1);
                }
                // Ideographs break anywhere.
                if kind == Kind::Char && (atoms[j].ch as u32) >= 0x2E80 { last_break = Some(j + 1); }
                j += 1;
            }
            let mut stop = j;
            let mut hyphen_w = 0.0;
            if ended_by.is_none() && j < atoms.len() {
                // Hyphenate the overflowing word when that fits more on the line (soft hyphens,
                // or the dictionary with automatic hyphenation).
                let consecutive = lines.iter().rev().take_while(|l: &&MLine| l.hyphen).count();
                let limit = doc.page.hyphenation.limit as usize;
                if limit == 0 || consecutive < limit {
                    if let Some((at, w)) = self.hyphenate(&atoms, &fmts, &format, line_start_atom, j, &xs, x, avail) { stop = at; hyphen_w = w; }
                }
                // Overflow: back up to the last break opportunity; trailing spaces stay on the line.
                if hyphen_w == 0.0 { if let Some(b) = last_break.filter(|&b| b > line_start_atom) { stop = b; } }
                // Text after a run of leading tabs that doesn't fit takes its tab to the next
                // line, where the tab aligns it at a stop again (as Word does).
                if hyphen_w == 0.0 && stop > line_start_atom + 1 && stop < atoms.len() && atoms[line_start_atom..stop].iter().all(|a| a.kind == Kind::Tab) && atoms[stop].kind != Kind::Tab { stop -= 1; }
                while stop < atoms.len() && atoms[stop].kind == Kind::Space { stop += 1; }
                if stop == line_start_atom { stop = line_start_atom + 1; }
                xs.truncate(stop - line_start_atom);
                while xs.len() < stop - line_start_atom { let k = line_start_atom + xs.len(); let px = xs.last().map(|v| *v + atoms[k - 1].width).unwrap_or(0.0); xs.push(px); }
            }
            let line_atoms: Vec<Atom> = atoms[line_start_atom..stop].to_vec();
            let is_last = stop >= atoms.len() && ended_by != Some(Kind::LineBreak) && ended_by != Some(Kind::PageBreak) || stop >= atoms.len() && ended_by.is_none();
            let _ = xs;
            let mut ml = self.finish_line(line_atoms, &fmts, &format, phys_left, avail - hyphen_w, is_last, ended_by == Some(Kind::LineBreak), phys_left + avail, rtl);
            ml.hyphen = hyphen_w > 0.0;
            let column = ended_by == Some(Kind::PageBreak) && atoms[..stop].last().is_some_and(|a| a.ch == chars::COLUMN_BREAK);
            ml.page_break_after = ended_by == Some(Kind::PageBreak) && !column;
            ml.column_break_after = column;
            ml.start = ml.atoms.iter().find_map(|a| a.pos).unwrap_or(lines.last().map(|l: &MLine| l.end).unwrap_or(start));
            ml.end = ml.atoms.iter().rev().find_map(|a| a.pos.map(|p| p + 1)).unwrap_or(ml.start);
            ml.left = if banded { phys_left } else if rtl { indent_right } else { indent_left.min(line_left) };
            ml.skip = skip;
            y += skip + ml.height;
            if matches!(ended_by, Some(Kind::LineBreak) | Some(Kind::PageBreak)) { pending.clear(); }
            lines.push(ml);
            i = stop;
            first = false;
            if i >= atoms.len() {
                // A trailing line/page break leaves an empty final line for the caret.
                if matches!(ended_by, Some(Kind::LineBreak) | Some(Kind::PageBreak)) {
                    let left = if rtl { indent_right } else { indent_left };
                    let mut empty = self.finish_line(vec![], &fmts, &format, left, avail, true, false, left + avail, rtl);
                    empty.start = end; empty.end = end; empty.left = left;
                    lines.push(empty);
                }
                break;
            }
        }
        if let Some(l) = lines.last_mut() { l.last = true; l.end = end; }
        Measured { lines, format, fmts, paragraph: index, style: p.style.clone(), glyphs, floats, start, marker: marker_text }
    }

    /// A hyphenation point for the word overflowing at atom `j`: (break before this atom, the
    /// hyphen's width). Soft hyphens win over the dictionary; Word's hyphenation zone, two
    /// letters on each side, and the caps option apply.
    #[allow(clippy::too_many_arguments)]
    fn hyphenate(&self, atoms: &[Atom], fmts: &[Fmt], format: &ParaFormat, line_start: usize, j: usize, xs: &[f32], x: f32, avail: f32) -> Option<(usize, f32)> {
        use hyphenation::{Hyphenator, Language, Load, Standard};
        static DICT: std::sync::OnceLock<Option<Standard>> = std::sync::OnceLock::new();
        let is_letter = |a: &Atom| a.kind == Kind::Char && (a.ch.is_alphabetic() || a.ch == '\u{AD}');
        if !is_letter(&atoms[j]) { return None; }
        let mut ws = j;
        while ws > line_start && is_letter(&atoms[ws - 1]) { ws -= 1; }
        let mut we = j;
        while we < atoms.len() && is_letter(&atoms[we]) { we += 1; }
        let word: Vec<char> = atoms[ws..we].iter().map(|a| a.ch).collect();
        let soft: Vec<usize> = word.iter().enumerate().filter(|(_, c)| **c == '\u{AD}').map(|(k, _)| k + 1).collect();
        let hy = self.doc.page.hyphenation;
        let auto = hy.auto && !format.suppress_hyphens;
        if soft.is_empty() && !auto { return None; }
        let x_ws = if ws - line_start < xs.len() { xs[ws - line_start] } else { x };
        // The hyphenation zone: only words starting before the zone at the line end break.
        if soft.is_empty() && avail - x_ws < twips(hy.zone) * 0.5 { return None; }
        let candidates: Vec<usize> = if !soft.is_empty() { soft } else {
            let letters: String = word.iter().collect();
            if letters.chars().count() < 5 || (!hy.caps && letters.chars().all(|c| c.is_uppercase())) || !letters.chars().all(|c| c.is_ascii_alphabetic()) { return None; }
            let dict = DICT.get_or_init(|| Standard::from_embedded(Language::EnglishUS).ok()).as_ref()?;
            let lower = letters.to_lowercase();
            dict.hyphenate(&lower).breaks.iter().map(|b| lower[..*b].chars().count()).filter(|k| *k >= 2 && word.len() - k >= 2).collect()
        };
        let f = &fmts[atoms[j].fmt];
        let hyphen_w = fonts::advance(f.key, '-', f.size);
        let room = avail - x_ws - hyphen_w;
        candidates.into_iter().rev().find(|&k| atoms[ws..ws + k].iter().map(|a| a.width).sum::<f32>() <= room).filter(|&k| ws + k > line_start).map(|k| (ws + k, hyphen_w))
    }

    /// Width of a tab starting at `abs` (px from the content box left), its leader, and whether
    /// it reached a right, center or decimal stop.
    fn tab_width(&self, f: &ParaFormat, abs: f32, seg: f32, marker: bool, indent_left: f32, following: &[Atom]) -> (f32, Leader, bool) {
        let mut stops: Vec<(f32, TabKind, Leader)> = f.tabs.iter().filter(|t| t.kind != TabKind::Clear).map(|t| (twips(t.pos), t.kind, t.leader)).collect();
        // The hanging indent acts as an implicit left stop (list markers, §17.3.1.37).
        if f.first_line < 0 { stops.push((indent_left, TabKind::Left, Leader::None)); }
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        let eps = 0.01;
        for (pos, kind, leader) in stops {
            let w = match kind {
                TabKind::Left | TabKind::Clear => pos - abs,
                TabKind::Center => pos - seg / 2.0 - abs,
                TabKind::Right => pos - seg - abs,
                TabKind::Decimal => {
                    let before: f32 = following.iter().take_while(|a| !matches!(a.kind, Kind::Tab | Kind::LineBreak | Kind::PageBreak) && a.ch != '.' && a.ch != ',').map(|a| a.width).sum();
                    pos - before - abs
                }
            };
            if pos > abs + eps && w > eps { return (w, leader, !matches!(kind, TabKind::Left | TabKind::Clear)); }
        }
        if marker && abs < indent_left - eps { return (indent_left - abs, Leader::None, false); }
        let d = self.page.default_tab.max(1.0);
        let next = ((abs + eps) / d).floor() * d + d;
        (next - abs, Leader::None, false)
    }

    /// Line metrics, bidi reordering (UAX #9 L1–L2) and alignment. `line_left` is the line's
    /// physical left edge; in right-to-left paragraphs the start side is the right.
    #[allow(clippy::too_many_arguments)]
    fn finish_line(&self, mut atoms: Vec<Atom>, fmts: &[Fmt], f: &ParaFormat, line_left: f32, avail: f32, last: bool, soft_break: bool, right_edge: f32, rtl: bool) -> MLine {
        let mut ascent = 0.0f32;
        let mut descent = 0.0f32;
        let mut image_h = 0.0f32;
        for a in &atoms {
            let fm = &fmts[a.fmt];
            if fm.hidden { continue; }
            if let Some((_, w, h, rot, _)) = &a.image {
                // Equations sit on the baseline like text: part above, part below.
                let d = a.object.as_ref().map(|o| o.descent()).unwrap_or(0.0);
                if d > 0.0 { ascent = ascent.max(h - d); descent = descent.max(d); continue; }
                image_h = image_h.max(rotated_bbox(*w, *h, *rot).1);
                continue;
            }
            ascent = ascent.max(fm.ascent + fm.shift.max(0.0));
            descent = descent.max(fm.descent - fm.shift.min(0.0));
        }
        if ascent == 0.0 && descent == 0.0 {
            // Empty lines use the paragraph mark's font (fmts[0]).
            ascent = fmts[0].ascent; descent = fmts[0].descent;
        }
        ascent = ascent.max(image_h);
        let natural = ascent + descent;
        let (height, baseline) = match f.line_rule {
            LineRule::Auto => (natural * f.line_spacing.max(0.06), ascent * f.line_spacing.min(1.0)),
            LineRule::Exact => { let h = f.line_spacing * PT; (h, h - descent) }
            LineRule::AtLeast => { let h = (f.line_spacing * PT).max(natural); (h, h - descent) }
        };
        let baseline = if f.line_rule == LineRule::Auto && f.line_spacing < 1.0 { height - descent } else { baseline };

        // L1: trailing whitespace, tabs and breaks take the paragraph level; trailing spaces hang.
        let n = atoms.len();
        let base = u8::from(rtl);
        let ws = |k: Kind| matches!(k, Kind::Space | Kind::LineBreak | Kind::PageBreak | Kind::Tab);
        let mut trail = n;
        while trail > 0 && ws(atoms[trail - 1].kind) && atoms[trail - 1].kind != Kind::Tab { trail -= 1; }
        let mut levels: Vec<u8> = atoms.iter().map(|a| a.level).collect();
        for k in 0..n { if k >= trail || matches!(atoms[k].kind, Kind::Tab | Kind::LineBreak | Kind::PageBreak) { levels[k] = base; } }
        let content: Vec<usize> = visual_order(&levels[..trail]);

        // Alignment: left/right are start/end, mirrored in right-to-left paragraphs.
        let natural_w: f32 = atoms[..trail].iter().map(|a| a.width).sum();
        let extra = (avail - natural_w).max(0.0);
        let justify = f.alignment == Alignment::Justify && !last && !soft_break;
        let align = match f.alignment { Alignment::Justify => Alignment::Left, a => a };
        let align = if rtl { match align { Alignment::Left => Alignment::Right, Alignment::Right => Alignment::Left, a => a } } else { align };
        // A line wider than its room overflows on its end side: past the left edge in RTL.
        let mut offset = line_left + if rtl { (avail - natural_w).min(0.0) } else { 0.0 };
        if !justify { match align { Alignment::Center => offset += extra / 2.0, Alignment::Right => offset += extra, _ => {} } }
        if justify {
            // Spaces after the last tab expand; the final line keeps natural spacing.
            let from = atoms[..trail].iter().rposition(|a| a.kind == Kind::Tab).map(|t| t + 1).unwrap_or(0);
            let spaces = atoms[from..trail].iter().filter(|a| a.kind == Kind::Space).count();
            if spaces > 0 { let add = extra / spaces as f32; for a in &mut atoms[from..trail] { if a.kind == Kind::Space { a.width += add; } } }
            else if rtl { offset += extra; }
        }
        let mut xs = vec![0.0f32; n];
        let mut cursor = offset;
        for &k in &content { xs[k] = cursor; cursor += atoms[k].width; }
        // Trailing spaces hang past the end side: right of LTR text, left of RTL text.
        let mut order = content;
        if rtl {
            let mut c = offset;
            for k in trail..n { c -= atoms[k].width; xs[k] = c; }
            order.splice(0..0, (trail..n).rev());
        } else {
            for k in trail..n { xs[k] = cursor; cursor += atoms[k].width; }
            order.extend(trail..n);
        }
        MLine { atoms, x: xs, height, baseline, start: 0, end: 0, page_break_after: false, column_break_after: false, last, left: line_left, right: right_edge, order, trail, rtl, hyphen: false, skip: 0.0 }
    }
}

/// The bounding box of a w×h box turned by `deg`: what a rotated inline object takes in its line.
fn rotated_bbox(w: f32, h: f32, deg: f32) -> (f32, f32) {
    if deg.rem_euclid(360.0) == 0.0 { return (w, h); }
    let (sin, cos) = deg.to_radians().sin_cos();
    (w * cos.abs() + h * sin.abs(), w * sin.abs() + h * cos.abs())
}

/// The x extent of a closed polygon between heights y0 and y1 (edges clipped to the band).
fn poly_extent(p: &[(f32, f32)], y0: f32, y1: f32) -> Option<(f32, f32)> {
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    let n = p.len();
    for k in 0..n {
        let (a, b) = (p[k], p[(k + 1) % n]);
        if a.1.max(b.1) < y0 || a.1.min(b.1) > y1 { continue; }
        // Clip the edge to [y0, y1] and take its end points.
        let at = |y: f32| if (b.1 - a.1).abs() < 1e-6 { a.0 } else { a.0 + (b.0 - a.0) * ((y - a.1) / (b.1 - a.1)).clamp(0.0, 1.0) };
        let ys = [a.1.clamp(y0, y1), b.1.clamp(y0, y1)];
        for y in ys { let x = at(y); lo = lo.min(x); hi = hi.max(x); }
    }
    (lo <= hi).then_some((lo, hi))
}

/// A float's outline for tight wrapping, in page coordinates: a shape's own geometry, or a
/// picture's opaque area (its box when fully opaque), turned with the object.
fn wrap_polygon(f: &FloatSpec, x: f32, y: f32) -> Option<std::sync::Arc<Vec<(f32, f32)>>> {
    use crate::objects::Object;
    if f.anchor.wrap != crate::model::Wrap::Tight { return None; }
    let local: Vec<(f32, f32)> = match f.object.as_deref() {
        Some(Object::Shape(s)) if !s.kind.is_line() => crate::objects::geometry(s.kind, s.width, s.height).into_iter().map(|p| s.transform(p)).collect(),
        Some(_) => return None,
        None => {
            let base = f.contour.clone().unwrap_or_else(|| vec![(0.0, 0.0), (f.w, 0.0), (f.w, f.h), (0.0, f.h)]);
            let (sin, cos) = f.rot.to_radians().sin_cos();
            let (cx, cy) = (f.w / 2.0, f.h / 2.0);
            base.into_iter().map(|(px, py)| { let (dx, dy) = (px - cx, py - cy); (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos) }).collect()
        }
    };
    Some(std::sync::Arc::new(local.into_iter().map(|(a, b)| (a + x, b + y)).collect()))
}

/// Visual order of atoms with the given bidi levels (UAX #9 L2): from the highest level down to
/// the lowest odd level, reverse every run at or above that level.
fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let (Some(&max), Some(min_odd)) = (levels.iter().max(), levels.iter().copied().filter(|l| l % 2 == 1).min()) else { return order };
    for lvl in (min_odd..=max).rev() {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= lvl {
                let s = i;
                while i < order.len() && levels[order[i]] >= lvl { i += 1; }
                order[s..i].reverse();
            } else { i += 1; }
        }
    }
    order
}

/// Resolve each character's face (script fallback) and bidi level, then shape words: runs of
/// characters sharing format, face and level go through rustybuzz together, so Arabic letters
/// join and marks sit on their bases. A cluster's glyphs and width belong to its first character.
fn shape_atoms(atoms: &mut [Atom], fmts: &[Fmt], rtl: bool) -> Vec<PGlyph> {
    let mut flat: Vec<PGlyph> = vec![];
    // Plain left-to-right text (no character from Hebrew onward) needs no bidi resolution.
    let needs_bidi = rtl || atoms.iter().any(|a| a.ch as u32 >= 0x0590 && !matches!(a.kind, Kind::Image | Kind::Note | Kind::Field));
    if needs_bidi {
        let text: String = atoms.iter().map(|a| match a.kind { Kind::Char | Kind::Space => a.ch, Kind::Tab => '\t', Kind::LineBreak => '\u{2028}', _ => '\u{FFFC}' }).collect();
        let info = unicode_bidi::BidiInfo::new(&text, Some(if rtl { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() }));
        for (k, (byte, _)) in text.char_indices().enumerate() { atoms[k].level = info.levels[byte].number(); }
        // A list marker with Latin letters ("Step 1:") reads left to right as a unit, as if
        // isolated (UAX #9 LRI…PDI). Digits and punctuation alone ("1.") follow the paragraph,
        // so Word shows ".1" in a right-to-left list.
        let marker = atoms.iter().take_while(|a| a.pos.is_none() && a.kind != Kind::Tab).count();
        if rtl && marker > 0 && !atoms[..marker].iter().any(|a| fonts::is_complex(a.ch))
            && atoms[..marker].iter().any(|a| a.ch.is_alphabetic() && !a.ch.is_numeric()) {
            for a in &mut atoms[..marker] { a.level = 2; }
        }
    }
    // Invisible format characters (RLM/LRM, ZWJ/ZWNJ, bidi embeddings) stay in the face of
    // the letter before them: a fallback face would split the word, breaking Arabic joining,
    // and draw them as visible glyphs.
    let mut prev_face: Option<FontKey> = None;
    for a in atoms.iter_mut() {
        if matches!(a.kind, Kind::Char | Kind::Space) {
            let base = fmts[a.fmt].key;
            let k = if is_format_char(a.ch) { prev_face.unwrap_or(base) } else { fonts::fallback(base, a.ch) };
            if k != base { a.key = Some(k); }
            prev_face = Some(k);
        } else { prev_face = None; }
    }
    let key = |a: &Atom| a.key.unwrap_or(fmts[a.fmt].key);
    let mut k = 0;
    while k < atoms.len() {
        if atoms[k].kind != Kind::Char || fmts[atoms[k].fmt].hidden { k += 1; continue; }
        // Runs split mid-word (a colored letter, Word's revision-session run splits) still
        // shape together when face, size and level agree, so letters keep their joined forms.
        let (fmt, face, level) = (atoms[k].fmt, key(&atoms[k]), atoms[k].level);
        let same = |a: &Atom| a.kind == Kind::Char && key(a) == face && a.level == level && !fmts[a.fmt].hidden && fmts[a.fmt].size == fmts[fmt].size;
        let mut e = k;
        while e < atoms.len() && same(&atoms[e]) { e += 1; }
        let word: String = atoms[k..e].iter().map(|a| a.ch).collect();
        // Byte offset → character index within the word.
        let mut index_of = vec![0usize; word.len() + 1];
        for (i, (b, _)) in word.char_indices().enumerate() { index_of[b] = i; }
        let f = &fmts[fmt];
        let mut pen = vec![0.0f32; e - k];
        // A cluster's glyphs are contiguous in the output, so each atom gets one range.
        for g in fonts::shape(face, &word, level % 2 == 1, f.size).iter() {
            let i = index_of.get(g.cluster).copied().unwrap_or(0);
            let a = &mut atoms[k + i];
            if a.glyphs.1 == 0 { a.glyphs = (flat.len() as u32, 0); }
            flat.push(PGlyph { gid: g.gid, dx: pen[i] + g.dx, dy: g.dy });
            a.glyphs.1 += 1;
            pen[i] += g.advance;
        }
        for (i, a) in atoms[k..e].iter_mut().enumerate() { a.width = if a.glyphs.1 == 0 { 0.0 } else { pen[i] + fmts[a.fmt].spacing }; }
        k = e;
    }
    flat
}

/// Default-ignorable format characters: bidi marks and embeddings, zero-width (non-)joiners.
fn is_format_char(c: char) -> bool {
    matches!(c as u32, 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2069 | 0x061C | 0xFEFF)
}

fn intern(fmts: &mut Vec<Fmt>, f: Fmt) -> usize {
    match fmts.iter().position(|x| x.key == f.key && x.size == f.size && x.color == f.color && x.underline == f.underline && x.strike == f.strike && x.highlight == f.highlight && x.shift == f.shift && x.hidden == f.hidden && x.spacing == f.spacing && x.link == f.link && x.revision == f.revision && x.effects == f.effects) {
        Some(i) => i,
        None => { fmts.push(f); fmts.len() - 1 }
    }
}

// ---------------------------------------------------------------------------
// Placement

struct Placer<'a> {
    ctx: Ctx<'a>,
    pages: Vec<PageBox>,
    lines: Vec<Line>,
    cells: Vec<CellBox>,
    page: usize,
    y: f32,
    body_top: f32,
    body_bottom: f32,
    /// Space reserved at the current page's foot for footnotes.
    foot_space: f32,
    /// Footnote ids shown at each page's foot, in order.
    page_notes: Vec<Vec<String>>,
    note_heights: HashMap<String, f32>,
    /// Content ends of vertically merged cells, by (table, grid column), awaiting their last row.
    merge_ends: HashMap<(usize, usize), (usize, f32)>,
    /// Floating objects placed so far, which later lines wrap around.
    floats: Vec<PlacedFloat>,
    /// Floats to paint once the pages are done: (page, layer: 0 behind text, 1 with text,
    /// 2 in front, z-order, items).
    front: Vec<(usize, u8, i64, Vec<DrawItem>)>,
    float_origins: HashMap<usize, (usize, f32, f32)>,
    /// The current section's columns on this page: (left, width) each, the one being filled,
    /// where they start, a balancing limit for all but the last, and the lowest point reached.
    cols: Vec<(f32, f32)>,
    col: usize,
    col_top: f32,
    col_bottom: Option<f32>,
    col_max_y: f32,
    col_separator: bool,
    /// Line numbering of the current section and the running count.
    line_numbers: Option<crate::page::LineNumbering>,
    line_no: u32,
    /// Floats from the previous pass, by page: text above a float's anchor paragraph (or
    /// placed before it on the page) wraps around them too.
    seeded: Vec<PlacedFloat>,
    /// The last paragraph box painted: (paragraph, page, bottom), for boxes that join.
    box_end: Option<(usize, usize, f32)>,
}

/// Gap above the footnote separator plus the separator itself.
const NOTE_SEPARATOR: f32 = 14.0;

impl<'a> Placer<'a> {
    /// A new empty page, filled with the page color.
    fn blank_page(&self) -> PageBox {
        let g = self.ctx.page;
        let under = self.ctx.doc.page.color.map(|c| vec![DrawItem::Rect { x: 0.0, y: 0.0, w: g.width, h: g.height, color: c }]).unwrap_or_default();
        PageBox { width: g.width, height: g.height, items: vec![], under, links: vec![], over: vec![] }
    }

    fn new_page(&mut self) {
        if !self.pages.is_empty() { self.close_columns(); }
        let blank = self.blank_page();
        self.pages.push(blank);
        self.page = self.pages.len() - 1;
        self.y = self.body_top;
        self.foot_space = 0.0;
        self.col = 0;
        self.col_top = self.body_top;
        self.col_bottom = None;
        self.col_max_y = self.body_top;
        if self.line_numbers.is_some_and(|l| l.restart == crate::page::LineRestart::Page) { self.line_no = 0; }
    }

    /// The writing area's bottom in the current column, above reserved footnotes (balanced
    /// columns stop earlier, except the last).
    fn bottom(&self) -> f32 {
        let page = self.body_bottom - self.foot_space;
        match self.col_bottom { Some(b) if self.col + 1 < self.cols.len() => b.min(page), _ => page }
    }

    /// The current column's left edge and width.
    fn col_geom(&self) -> (f32, f32) { self.cols.get(self.col).copied().unwrap_or((self.ctx.page.left, self.ctx.page.width - self.ctx.page.left - self.ctx.page.right)) }

    /// Is the cursor at the top of its column (nothing placed above in it)?
    fn at_column_top(&self) -> bool { self.y <= self.col_top + 0.5 }

    /// Continue in the next column, or on a new page after the last one.
    fn break_column(&mut self) {
        self.col_max_y = self.col_max_y.max(self.y);
        if self.col + 1 < self.cols.len() {
            self.col += 1;
            self.y = self.col_top;
        } else {
            self.new_page();
        }
    }

    /// Draw the lines between this page's columns (w:sep) and note how far they reached.
    fn close_columns(&mut self) {
        self.col_max_y = self.col_max_y.max(self.y);
        if self.col_separator && self.cols.len() > 1 {
            let (top, bottom) = (self.col_top, self.col_max_y);
            let page = self.page;
            self.ensure_page(page);
            for k in 1..self.cols.len() {
                let x = (self.cols[k - 1].0 + self.cols[k - 1].1 + self.cols[k].0) / 2.0;
                self.pages[page].items.push(DrawItem::Line { x1: x, y1: top, x2: x, y2: bottom, width: 0.75, color: Color(0, 0, 0) });
            }
        }
    }

    /// Column positions for a section on this page.
    fn section_columns(&self, s: &crate::page::Section) -> Vec<(f32, f32)> { self.ctx.page.columns(&s.columns) }

    /// Start a section: its break (new page, even/odd page, or continuing below the previous
    /// section), then its columns and line numbering. `height` (an estimate of the section's
    /// content) balances its columns when it ends at a continuous break.
    fn start_section(&mut self, s: &crate::page::Section, first: bool, height: Option<f32>) {
        use crate::page::SectionKind;
        if !first {
            match s.kind {
                SectionKind::Continuous => {
                    self.close_columns();
                    self.y = self.col_max_y.max(self.y);
                }
                SectionKind::NextPage => self.new_page(),
                SectionKind::EvenPage | SectionKind::OddPage => {
                    self.new_page();
                    // Page numbers are 1-based: even pages have odd indices.
                    let even = (self.page + 1) % 2 == 0;
                    if even != (s.kind == SectionKind::EvenPage) { self.new_page(); }
                }
            }
        }
        self.cols = self.section_columns(s);
        self.col = 0;
        self.col_top = self.y;
        self.col_max_y = self.y;
        self.col_separator = s.columns.separator;
        self.col_bottom = match height { Some(h) if self.cols.len() > 1 => Some(self.y + h / self.cols.len() as f32 + 2.0), _ => None };
        self.line_numbers = s.line_numbers;
        if first || s.line_numbers.is_some_and(|l| l.restart == crate::page::LineRestart::Section) { self.line_no = 0; }
    }

    /// A note's paragraphs as displayed: its number first, at footnote size unless styled.
    fn note_paragraphs(&self, kind: NoteKind, id: &str) -> Vec<Paragraph> {
        let doc = self.ctx.doc;
        let mut paras = doc.note(kind, id).map(|n| n.paragraphs.clone()).unwrap_or_else(|| vec![Paragraph::default()]);
        let label = self.ctx.note_labels.get(&(kind, id.to_string())).cloned().unwrap_or_default();
        for p in &mut paras {
            if p.style.is_none() {
                for r in &mut p.runs { if r.props.size.is_none() { r.props.size = Some(10.0); } }
                if p.mark.size.is_none() { p.mark.size = Some(10.0); }
                if p.props.space_after.is_none() { p.props.space_after = Some(0.0); }
            }
            p.cell = None;
        }
        if let Some(first) = paras.first_mut() {
            let props = first.runs.first().map(|r| r.props.clone()).unwrap_or_else(|| first.mark.clone());
            first.runs.insert(0, Run { text: format!("{label} "), props: crate::format::RunProps { vert_align: Some(VertAlign::Superscript), ..props }, ..Default::default() });
            first.runs[1..].iter_mut().take(1).for_each(|r| if r.text.starts_with(' ') { r.text.remove(0); });
        }
        paras
    }

    fn note_height(&mut self, id: &str) -> f32 {
        if let Some(h) = self.note_heights.get(id) { return *h; }
        let width = self.ctx.page.width - self.ctx.page.left - self.ctx.page.right;
        let paras = self.note_paragraphs(NoteKind::Foot, id);
        let h = paras.iter().map(|p| self.ctx.measure(usize::MAX, p, 0, width).lines.iter().map(|l| l.height).sum::<f32>()).sum();
        self.note_heights.insert(id.to_string(), h);
        h
    }

    /// Footnotes on a line that aren't yet on `page`.
    fn new_notes(&self, l: &MLine, page: usize) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for a in &l.atoms {
            if let Some((NoteKind::Foot, id)) = &a.note {
                if !self.page_notes.get(page).is_some_and(|n| n.contains(id)) && !out.contains(id) { out.push(id.clone()); }
            }
        }
        out
    }

    /// Height a line's new footnotes would add to the current page's foot.
    fn notes_need(&mut self, l: &MLine) -> f32 {
        let new = self.new_notes(l, self.page);
        if new.is_empty() { return 0.0; }
        let sep = if self.foot_space == 0.0 { NOTE_SEPARATOR } else { 0.0 };
        sep + new.iter().map(|id| self.note_height(id)).sum::<f32>()
    }

    /// Paint each page's footnotes above its bottom margin.
    fn paint_notes(&mut self) {
        let geo = self.ctx.page;
        let width = geo.width - geo.left - geo.right;
        for page in 0..self.page_notes.len().min(self.pages.len()) {
            let ids = self.page_notes[page].clone();
            if ids.is_empty() { continue; }
            let total: f32 = ids.iter().map(|id| self.note_height(id)).sum();
            let mut y = self.body_bottom - total;
            let sep_y = y - NOTE_SEPARATOR / 2.0;
            self.pages[page].items.push(DrawItem::Line { x1: geo.left, y1: sep_y, x2: geo.left + 144.0, y2: sep_y, width: 0.75, color: Color::BLACK });
            let saved = self.lines.len();
            for id in ids {
                for p in self.note_paragraphs(NoteKind::Foot, &id) {
                    let m = self.ctx.measure(usize::MAX, &p, 0, width);
                    for l in &m.lines { self.emit(&m, l, page, geo.left, y, None); y += l.height; }
                }
            }
            // Note text is edited in the notes pane, not on the page.
            self.lines.truncate(saved);
        }
    }

    /// Endnotes follow the body after a short separator.
    fn place_endnotes(&mut self) {
        let doc = self.ctx.doc;
        let refs: Vec<String> = doc.note_refs().into_iter().filter(|r| r.0 == NoteKind::End).map(|r| r.1).collect();
        if refs.is_empty() { return; }
        let geo = self.ctx.page;
        let width = geo.width - geo.left - geo.right;
        if self.y + NOTE_SEPARATOR * 2.0 > self.bottom() { self.new_page(); }
        self.y += NOTE_SEPARATOR;
        let sep_y = self.y - NOTE_SEPARATOR / 2.0;
        let page = self.page;
        self.pages[page].items.push(DrawItem::Line { x1: geo.left, y1: sep_y, x2: geo.left + 144.0, y2: sep_y, width: 0.75, color: Color::BLACK });
        let saved = self.lines.len();
        let mut seen = std::collections::HashSet::new();
        for id in refs {
            if !seen.insert(id.clone()) { continue; }
            for p in self.note_paragraphs(NoteKind::End, &id) {
                let m = self.ctx.measure(usize::MAX, &p, 0, width);
                self.place_paragraph(&m, None, 0.0);
            }
        }
        self.lines.truncate(saved);
    }
    fn ensure_page(&mut self, page: usize) { while self.pages.len() <= page { let b = self.blank_page(); self.pages.push(b); } }

    /// Page borders (w:pgBorders) on every page, measured from the page edge or the text.
    fn paint_page_borders(&mut self) {
        let Some(b) = self.ctx.doc.page.borders.clone() else { return };
        let g = self.ctx.page;
        let sp = b.space * PT;
        let (x0, y0, x1, y1) = if b.from_text { (g.left - sp, self.body_top - sp, g.width - g.right + sp, self.body_bottom + sp) } else { (sp, sp, g.width - sp, g.height - sp) };
        let w = (b.width * PT).max(0.5);
        use crate::page::BorderStyle;
        for pb in &mut self.pages {
            let sides = [((x0, y0), (x1, y0), 1.0), ((x0, y0), (x0, y1), 1.0), ((x0, y1), (x1, y1), -1.0), ((x1, y0), (x1, y1), -1.0)];
            for (k, &((ax, ay), (bx, by), inward)) in [sides[0], sides[1], sides[2], sides[3]].iter().enumerate() {
                if !b.sides[k] { continue; }
                let horizontal = ay == by;
                let off = |d: f32| if horizontal { (ax, ay + d * inward, bx, by + d * inward) } else { (ax + d * inward, ay, bx + d * inward, by) };
                let mut line = |d: f32, width: f32| { let (a, c, e, f) = off(d); pb.items.push(DrawItem::Line { x1: a, y1: c, x2: e, y2: f, width, color: b.color }); };
                match b.style {
                    BorderStyle::Single => line(0.0, w),
                    BorderStyle::Thick => line(0.0, w * 2.5),
                    BorderStyle::Double => { line(0.0, w); line(w * 2.5, w); }
                    BorderStyle::ThinThick => { line(0.0, w * 2.5); line(w * 3.0, w * 0.6); }
                    BorderStyle::Dotted | BorderStyle::Dashed => {
                        let (dash, gap) = if b.style == BorderStyle::Dotted { (w, w * 2.0) } else { (w * 6.0, w * 3.0) };
                        let len = if horizontal { bx - ax } else { by - ay };
                        let mut t = 0.0;
                        while t < len {
                            let e = (t + dash).min(len);
                            let (a, c, _, _) = off(0.0);
                            let (p0, p1) = if horizontal { ((a + t, c), (a + e, c)) } else { ((a, c + t), (a, c + e)) };
                            pb.items.push(DrawItem::Line { x1: p0.0, y1: p0.1, x2: p1.0, y2: p1.1, width: w, color: b.color });
                            t += dash + gap;
                        }
                    }
                }
            }
        }
    }

    /// A drop cap paragraph: its letter drops beside the next paragraph's first lines (or into
    /// the margin) without taking vertical space; dropped caps wrap the next lines around them.
    fn place_drop_cap(&mut self, index: usize, start: usize, next_start: usize, dc: crate::format::DropCap, width: f32, prev: Option<&(ParaFormat, Option<String>)>) {
        let doc = self.ctx.doc;
        let p = &doc.paragraphs[index];
        let m = self.ctx.measure_with(index, p, start, width, None, &[], None);
        let next = self.ctx.measure_with(index + 1, &doc.paragraphs[index + 1], next_start, width, None, &[], None);
        let Some(l) = m.lines.first() else { return };
        let (lh, bl) = next.lines.first().map(|x| (x.height, x.baseline)).unwrap_or((l.height, l.baseline));
        let mut gap = next.format.space_before * PT;
        if let Some((pf, _)) = prev { gap = gap.max(pf.space_after * PT); }
        if self.y <= self.body_top + 0.5 { gap = next.format.space_before * PT; }
        let lines = dc.lines.max(1) as f32;
        let mut top = self.y + gap;
        if top + lines * lh > self.bottom() && !self.at_column_top() { self.break_column(); top = self.y + next.format.space_before * PT; }
        let cap_w: f32 = l.atoms[..l.trail].iter().map(|a| a.width).sum();
        let dist = twips(dc.distance);
        let (left, width) = self.col_geom();
        // In right-to-left paragraphs the cap drops on the right.
        let rtl = next.lines.first().is_some_and(|l| l.rtl);
        let right = left + width;
        let x = match (dc.margin, rtl) { (true, false) => left - cap_w - dist.max(4.0), (false, false) => left, (true, true) => right + dist.max(4.0), (false, true) => right - cap_w };
        // The letter's baseline sits on the last dropped line's baseline.
        let y = top + (lines - 1.0) * lh + bl - l.baseline;
        let page = self.page;
        self.emit(&m, l, page, x, y, None);
        if !dc.margin {
            let (x0, x1) = if rtl { (right - cap_w - dist - 2.0, right) } else { (left, left + cap_w + dist + 2.0) };
            self.floats.push(PlacedFloat { page, pos: usize::MAX - index, x0, y0: top, x1, y1: top + lines * lh - 0.5, wrap: crate::model::Wrap::Square, dist: 0.0, poly: None });
        }
    }

    /// Emit a measured line at (page, left, top).
    fn emit(&mut self, m: &Measured, l: &MLine, page: usize, left: f32, top: f32, cell: Option<u64>) {
        self.ensure_page(page);
        // Line numbers in the margin beside body lines (w:lnNumType), every `count_by` lines.
        if let (None, Some(ln), true) = (cell, self.line_numbers, m.paragraph < self.ctx.doc.paragraphs.len() && m.format.drop_cap.is_none() && !m.format.suppress_line_numbers) {
            self.line_no += 1;
            let n = ln.start.max(1) - 1 + self.line_no;
            if n % ln.count_by.max(1) == 0 {
                let doc = self.ctx.doc;
                let size = doc.styles.doc_run.size.unwrap_or(11.0);
                let font = doc.styles.doc_run.font.clone().unwrap_or_else(|| "Calibri".into());
                let text = n.to_string();
                let w = crate::objects::text_width(&text, &font, false, size);
                let dist = if ln.distance > 0 { twips(ln.distance) } else { 18.0 };
                let col_left = self.col_geom().0;
                let (items, _) = crate::objects::text_line(&text, &font, false, size, Color(0, 0, 0), col_left - dist - w, top + l.baseline);
                self.pages[page].items.extend(items);
            }
        }
        if m.paragraph != usize::MAX {
            let new = self.new_notes(l, page);
            if !new.is_empty() {
                if page == self.page { let need = self.notes_need(l); self.foot_space += need; }
                while self.page_notes.len() <= page { self.page_notes.push(vec![]); }
                self.page_notes[page].extend(new);
            }
        }
        let base = top + l.baseline;
        let n = l.end.saturating_sub(l.start);
        // Caret stops: before a character is its leading edge (left for LTR, right for RTL).
        let mut by_pos: Vec<Option<f32>> = vec![None; n + 1];
        let mut boxes: Vec<(f32, f32)> = vec![(0.0, 0.0); n];
        let mut last: Option<(f32, f32, bool)> = None;
        // Spaces hanging past the margin keep the caret at the margin (or at the end of text that
        // already overflows it), as in Word, instead of running off the page.
        let hang = l.atoms.get(l.trail).map(|a| if l.rtl { (left + l.left).min(left + l.x[l.trail] + a.width) } else { (left + l.right).max(left + l.x[l.trail]) });
        for (k, a) in l.atoms.iter().enumerate() {
            let (mut ax, mut aw) = (left + l.x[k], a.width);
            if let (Some(edge), true) = (hang, k >= l.trail && a.kind == Kind::Space) {
                let (x0, x1) = if l.rtl { (ax.max(edge), (ax + aw).max(edge)) } else { (ax.min(edge), (ax + aw).min(edge)) };
                ax = x0; aw = x1 - x0;
            }
            let odd = a.level % 2 == 1 || k >= l.trail && l.rtl;
            if let Some(p) = a.pos {
                if p >= l.start && p - l.start < n {
                    by_pos[p - l.start] = Some(if odd { ax + aw } else { ax });
                    boxes[p - l.start] = (ax, ax + aw);
                }
                last = Some((ax, ax + aw, odd));
            }
        }
        // A ligature (Arabic lam-alef, "ffi") gives its whole width to its first character; the
        // caret between its letters goes to an even share of the width, as in Word.
        let mut k = 0;
        while k < l.atoms.len() {
            let a = &l.atoms[k];
            let mut e = k + 1;
            if let (Some(p), Kind::Char, true) = (a.pos, a.kind, a.glyphs.1 > 0) {
                while e < l.atoms.len() && { let b = &l.atoms[e]; b.kind == Kind::Char && b.glyphs.1 == 0 && b.width == 0.0 && b.level == a.level && !m.fmts[b.fmt].hidden && b.pos == Some(p + e - k) } { e += 1; }
                let parts = (e - k) as f32;
                let (ax, odd) = (left + l.x[k], a.level % 2 == 1 || k >= l.trail && l.rtl);
                for j in 1..e - k {
                    let i = p + j - l.start;
                    if p + j >= l.start && i < n { let d = a.width * j as f32 / parts; by_pos[i] = Some(if odd { ax + a.width - d } else { ax + d }); }
                }
            }
            k = e;
        }
        // After the last character: its trailing edge.
        if by_pos[n].is_none() { if let Some((x0, x1, odd)) = last { by_pos[n] = Some(if odd { x0 } else { x1 }); } }
        let aligned_start = left + if l.rtl { l.right } else { l.left };
        let empty_x = left + match (m.format.alignment, l.rtl) {
            (Alignment::Center, _) => (l.left + l.right) / 2.0,
            (Alignment::Right, false) | (Alignment::Left | Alignment::Justify, true) => l.right,
            _ => l.left,
        };
        let mut prev = if l.atoms.is_empty() { empty_x } else { aligned_start };
        let mut stops = Vec::with_capacity(n + 1);
        for v in by_pos { let v = v.unwrap_or(prev); stops.push(v); prev = v; }
        for (i, b) in boxes.iter_mut().enumerate() { if b.0 == 0.0 && b.1 == 0.0 { *b = (stops[i], stops[i]); } }
        let bidi = l.rtl || l.atoms.iter().any(|a| a.level % 2 == 1);
        self.lines.push(Line { page, y: top, height: l.height, baseline: base, left: left + l.left.min(0.0).max(-left) , right: left + l.right, start: l.start, end: l.end, stops, paragraph: m.paragraph, last_in_paragraph: l.last, cell, boxes, rtl: l.rtl, bidi });
        self.paint_line(m, l, page, left, top);
    }

    fn paint_line(&mut self, m: &Measured, l: &MLine, page: usize, left: f32, top: f32) {
        let base = top + l.baseline;
        let bar_x = self.ctx.page.left - 14.0;
        let pb = &mut self.pages[page];
        // Change bar in the left margin for lines with tracked changes.
        if l.atoms.iter().any(|a| m.fmts[a.fmt].revision) {
            pb.items.push(DrawItem::Line { x1: bar_x, y1: top, x2: bar_x, y2: top + l.height, width: 1.0, color: Color(0x80, 0x80, 0x80) });
        }
        // One underline position per line, as in Word: the lowest and thickest of the
        // underlined text on the baseline, so runs in different fonts and sizes (Arabic words
        // and the Latin-sized spaces between them) join into one unbroken rule.
        let (mut ul_off, mut ul_th) = (f32::MIN, 0.0f32);
        for a in &l.atoms[..l.trail] {
            let f = &m.fmts[a.fmt];
            if f.underline && !f.hidden && f.shift == 0.0 && a.image.is_none() { ul_off = ul_off.max(f.ul_offset); ul_th = ul_th.max(f.ul_thickness); }
        }
        let mut underlines: Vec<(f32, f32, f32, f32, Color)> = vec![];
        // Visual groups of atoms sharing a format and face.
        let key_of = |a: &Atom| a.key.unwrap_or(m.fmts[a.fmt].key);
        let mut v = 0;
        while v < l.order.len() {
            let k = l.order[v];
            let a = &l.atoms[k];
            let f = &m.fmts[a.fmt];
            let x = left + l.x[k];
            if let Some((asset, w, h, rot, flip)) = &a.image {
                // A rotated object sits centered in its rotated bounding box.
                let (bw, bh) = rotated_bbox(*w, *h, *rot);
                let d = a.object.as_ref().map(|o| o.descent()).unwrap_or(0.0);
                let (ox, oy) = (x + (bw - w) / 2.0, base - bh + (bh - h) / 2.0 + d);
                match &a.object {
                    Some(obj) => {
                        pb.items.extend(obj.render(ox, oy));
                        pb.items.push(DrawItem::Frame { x: ox, y: oy, w: *w, h: *h, pos: a.pos.unwrap_or(0) });
                    }
                    None => pb.items.push(DrawItem::Image { asset: asset.clone(), x: ox, y: oy, w: *w, h: *h, pos: a.pos.unwrap_or(0), rot: *rot, flip: *flip }),
                }
                v += 1;
                continue;
            }
            let face = key_of(a);
            let mut glyphs = vec![];
            let (mut start_x, mut end_x) = (f32::MAX, f32::MIN);
            let (mut deco0, mut deco1) = (f32::MAX, f32::MIN);
            let mut w = v;
            let mut field_pieces: Vec<(FontKey, Vec<Glyph>)> = vec![];
            while w < l.order.len() {
                let j = l.order[w];
                let at = &l.atoms[j];
                if at.fmt != a.fmt || at.image.is_some() || key_of(at) != face { break; }
                let gx = left + l.x[j];
                let text_glyphs = |text: &str, glyphs: &mut Vec<Glyph>| {
                    let mut fx = gx;
                    for c in text.chars() { glyphs.push(Glyph { ch: c, gid: fonts::glyph_id(face.face, c), x: fx, dy: 0.0 }); fx += fonts::advance(face, c, f.size); }
                };
                match at.kind {
                    Kind::Char if !f.hidden && at.ch != '\u{AD}' => for (n, g) in m.glyphs[at.glyphs.0 as usize..at.glyphs.0 as usize + at.glyphs.1 as usize].iter().enumerate() { glyphs.push(Glyph { ch: if n == 0 { at.ch } else { CONTINUATION }, gid: g.gid, x: gx + g.dx, dy: g.dy }); },
                    // Spaces paint nothing but keep word boundaries for PDF text extraction.
                    Kind::Space if !f.hidden => glyphs.push(Glyph { ch: ' ', gid: fonts::glyph_id(face.face, ' '), x: gx, dy: 0.0 }),
                    Kind::Note if !f.hidden => text_glyphs(&at.note.as_ref().and_then(|n| self.ctx.note_labels.get(n)).cloned().unwrap_or_default(), &mut glyphs),
                    // A field's result (a citation's Arabic author, say) is shaped with bidi and font
                    // fallback; pieces in other faces are drawn after this run's glyphs.
                    Kind::Field => if let Some(field) = &at.field {
                        let (pieces, _) = crate::objects::shape_bidi(&self.ctx.field_text(field, at.pos), l.rtl, face, f.size, gx);
                        for (key, gs) in pieces { if key == face { glyphs.extend(gs); } else { field_pieces.push((key, gs)); } }
                    },
                    Kind::Tab if at.leader != Leader::None => {
                        let c = match at.leader { Leader::Dot => '.', Leader::Hyphen => '-', Leader::Underscore => '_', Leader::MiddleDot => '\u{00B7}', Leader::None => ' ' };
                        let cw = fonts::advance(face, c, f.size).max(1.0);
                        let mut fx = (gx / cw).ceil() * cw;
                        while fx + cw <= gx + at.width - cw * 0.5 { glyphs.push(Glyph { ch: c, gid: fonts::glyph_id(face.face, c), x: fx, dy: 0.0 }); fx += cw; }
                    }
                    _ => {}
                }
                start_x = start_x.min(gx);
                end_x = end_x.max(gx + at.width);
                // Trailing spaces are not underlined (ulTrailSpace off).
                if j < l.trail { deco0 = deco0.min(gx); deco1 = deco1.max(gx + at.width); }
                w += 1;
            }
            if let Some(link) = f.link.clone().filter(|_| end_x > start_x) {
                pb.links.push((start_x, top, end_x - start_x, l.height, link));
            }
            if !f.hidden {
                if let Some(h) = f.highlight { pb.under.push(DrawItem::Rect { x: start_x, y: top, w: end_x - start_x, h: l.height, color: h }); }
                let gb = base - f.shift;
                if !glyphs.is_empty() { pb.items.push(DrawItem::Glyphs { glyphs, baseline: gb, font: face, size: f.size, color: f.color, effects: f.effects }); }
                for (key, gs) in field_pieces.drain(..) { pb.items.push(DrawItem::Glyphs { glyphs: gs, baseline: gb, font: key, size: f.size, color: f.color, effects: f.effects }); }
                if deco1 > deco0 {
                    if f.underline {
                        let (off, th) = if f.shift == 0.0 && ul_th > 0.0 { (ul_off, ul_th) } else { (f.ul_offset, f.ul_thickness) };
                        let y = gb + off + th / 2.0;
                        match underlines.last_mut() {
                            Some(u) if (u.1 - deco0).abs() < 0.5 && u.2 == y && u.4 == f.color => u.1 = deco1,
                            _ => underlines.push((deco0, deco1, y, th, f.color)),
                        }
                    }
                    match f.strike {
                        Strike::None => {}
                        Strike::Single => { let y = gb - f.strike_offset; pb.items.push(DrawItem::Line { x1: deco0, y1: y, x2: deco1, y2: y, width: f.ul_thickness, color: f.color }); }
                        Strike::Double => for d in [-1.0, 1.0] { let y = gb - f.strike_offset + d * f.ul_thickness * 1.2; pb.items.push(DrawItem::Line { x1: deco0, y1: y, x2: deco1, y2: y, width: f.ul_thickness * 0.8, color: f.color }); },
                    }
                }
            }
            v = w;
        }
        for (x1, x2, y, width, color) in underlines { pb.items.push(DrawItem::Line { x1, y1: y, x2, y2: y, width, color }); }
        // A hyphenated line ends with a hyphen after its last letter.
        if l.hyphen {
            if let Some(k) = (0..l.trail).rev().find(|&k| l.atoms[k].kind == Kind::Char) {
                let a = &l.atoms[k];
                let f = &m.fmts[a.fmt];
                let face = a.key.unwrap_or(f.key);
                let x = left + l.x[k] + a.width;
                let g = Glyph { ch: '-', gid: fonts::glyph_id(face.face, '-'), x, dy: 0.0 };
                self.pages[page].items.push(DrawItem::Glyphs { glyphs: vec![g], baseline: top + l.baseline - f.shift, font: face, size: f.size, color: f.color, effects: f.effects });
            }
        }
    }

    /// Paragraph fill/borders around lines placed on one page.
    fn paint_paragraph_box(&mut self, m: &Measured, page: usize, left: f32, width: f32, top: f32, bottom: f32, first: bool, last: bool) {
        self.ensure_page(page);
        let f = &m.format;
        let x0 = left + twips(f.indent_left);
        let x1 = left + width - twips(f.indent_right);
        let b = &f.borders;
        // Neighbouring paragraphs with the same borders and indents share one box, as in Word:
        // no line between them and the sides run through the spacing.
        let doc = self.ctx.doc;
        let joins = |i: usize, j: usize| j < doc.paragraphs.len() && doc.paragraphs[i].cell == doc.paragraphs[j].cell
            && { let g = doc.para_format(&doc.paragraphs[j]); g.borders == f.borders && g.indent_left == f.indent_left && g.indent_right == f.indent_right };
        let i = m.paragraph;
        let body = i < doc.paragraphs.len();
        let joined_above = match self.box_end { Some((q, pg, y)) if first && body && i > 0 && q == i - 1 && pg == page && joins(i, i - 1) => Some(y), _ => None };
        let joined_below = last && body && joins(i, i + 1);
        let (first, last) = (first && joined_above.is_none(), last && !joined_below);
        let pb = &mut self.pages[page];
        let pad = |o: &Option<Border>| o.map(|b| b.space * PT).unwrap_or(0.0);
        let (bx0, bx1) = (x0 - pad(&b.left), x1 + pad(&b.right));
        let (by0, by1) = (joined_above.unwrap_or(top - if first { pad(&b.top) } else { 0.0 }), bottom + if last { pad(&b.bottom) } else { 0.0 });
        self.box_end = body.then_some((i, page, by1));
        if let Some(c) = f.shading { pb.under.push(DrawItem::Rect { x: bx0, y: by0, w: bx1 - bx0, h: by1 - by0, color: c }); }
        let line = |pb: &mut PageBox, b: &Option<Border>, x1: f32, y1: f32, x2: f32, y2: f32| if let Some(b) = b { pb.items.push(DrawItem::Line { x1, y1, x2, y2, width: (b.width * PT).max(0.75), color: b.color.unwrap_or(Color::BLACK) }); };
        if first { line(pb, &b.top, bx0, by0, bx1, by0); }
        if last { line(pb, &b.bottom, bx0, by1, bx1, by1); }
        line(pb, &b.left, bx0, by0, bx0, by1);
        line(pb, &b.right, bx1, by0, bx1, by1);
    }

    /// Position and paint a paragraph's floating objects on the current page.
    /// `wrap` registers body floats for text wrapping; floats in cells and stories only paint.
    fn place_floats(&mut self, m: &Measured, page: usize, left: f32, width: f32, para_top: f32, wrap: bool) {
        use crate::model::{AnchorAlign, RelFrom, Wrap};
        let g = self.ctx.page;
        for f in &m.floats {
            let a = f.anchor;
            let (x_base, x_span) = match a.x_from { RelFrom::Page => (0.0, g.width), _ => (left, width) };
            let (y_base, y_span) = match a.y_from { RelFrom::Page => (0.0, g.height), RelFrom::Margin => (self.body_top, self.body_bottom - self.body_top), RelFrom::Paragraph => (para_top, 0.0) };
            let place = |base: f32, span: f32, size: f32, off: f32, al: Option<AnchorAlign>| match al {
                Some(AnchorAlign::Start | AnchorAlign::Inside) => base, Some(AnchorAlign::Center) => base + (span - size) / 2.0, Some(AnchorAlign::End | AnchorAlign::Outside) => base + span - size, None => base + off,
            };
            // Inside/outside swap sides on even (left-hand) pages.
            let even = page % 2 == 1;
            let x_align = match a.x_align { Some(AnchorAlign::Inside) if even => Some(AnchorAlign::End), Some(AnchorAlign::Outside) if even => Some(AnchorAlign::Start), al => al };
            let x = place(x_base, x_span, f.w, a.x, x_align);
            let y = place(y_base, y_span, f.h, a.y, a.y_align);
            self.ensure_page(page);
            // Header/footer objects aren't selectable from the body.
            let story = m.paragraph >= self.ctx.doc.paragraphs.len();
            if !story { self.float_origins.insert(f.pos, (page, left, para_top)); }
            let pos = if story { usize::MAX } else { f.pos };
            let mut items: Vec<DrawItem> = match &f.object { Some(o) => o.render(x, y), None => vec![DrawItem::Image { asset: f.asset.clone(), x, y, w: f.w, h: f.h, pos, rot: f.rot, flip: f.flip }] };
            if f.object.is_some() && !story { items.push(DrawItem::Frame { x, y, w: f.w, h: f.h, pos }); }
            // Floats are painted in z-order within their layer once the page is complete.
            let layer = match a.wrap {
                Wrap::Behind => {
                    // Behind text: its frame stays in `under` too, so clicks reach the text.
                    items.retain(|i| !matches!(i, DrawItem::Frame { .. }));
                    if !story { items.push(DrawItem::Frame { x, y, w: f.w, h: f.h, pos }); }
                    0
                }
                Wrap::InFront => 2,
                _ => 1,
            };
            self.front.push((page, layer, a.z, items));
            if wrap {
                // This float replaces its seed from the previous pass.
                self.seeded.retain(|s| s.pos != f.pos);
                self.floats.push(PlacedFloat { page, pos: f.pos, x0: x, y0: y, x1: x + f.w, y1: y + f.h, wrap: a.wrap, dist: a.dist, poly: wrap_polygon(f, x, y) });
            }
        }
    }

    /// The current page's wrapping floats as bands relative to a paragraph at `para_top`.
    fn bands(&self, left: f32, para_top: f32) -> Vec<Band> {
        use crate::model::Wrap;
        self.floats.iter().chain(self.seeded.iter()).filter(|f| f.page == self.page && !matches!(f.wrap, Wrap::Behind | Wrap::InFront) && f.y1 + f.dist > para_top).map(|f| {
            let d = if f.wrap == Wrap::Tight { f.dist / 2.0 } else { f.dist };
            // Tight wrap follows the outline, shifted into paragraph coordinates.
            let poly = (f.wrap == Wrap::Tight).then(|| f.poly.clone()).flatten().map(|p| std::sync::Arc::new(p.iter().map(|&(x, y)| (x - left, y - para_top)).collect::<Vec<_>>()));
            Band { x0: f.x0 - d - left, x1: f.x1 + d - left, y0: f.y0 - d - para_top, y1: f.y1 + d - para_top, full: f.wrap == Wrap::TopBottom, poly, dist: d }
        }).collect()
    }

    /// Place a paragraph's lines in the body flow, breaking pages as needed.
    fn place_paragraph(&mut self, m: &Measured, prev: Option<&(ParaFormat, Option<String>)>, keep_with: f32) {
        let f = &m.format;
        // Spacing between paragraphs collapses to the larger value (paragraphGap).
        let mut gap = f.space_before * PT;
        if let Some((pf, pstyle)) = prev {
            let after = pf.space_after * PT;
            gap = gap.max(after);
            if *pstyle == m.style {
                gap -= if pf.contextual_spacing { after } else { 0.0 } + if f.contextual_spacing { f.space_before * PT } else { 0.0 };
            }
            gap = gap.max(0.0);
        }
        if f.page_break_before && self.y > self.body_top + 0.5 { self.new_page(); }
        if !self.at_column_top() { self.y += gap; } else if prev.is_none() || f.page_break_before { self.y += f.space_before * PT; }
        let total: f32 = m.lines.iter().map(|l| l.height).sum();
        let capacity = self.bottom() - self.col_top;
        let remaining = self.bottom() - self.y;
        // Keep lines together / with next when the paragraph fits in a fresh column.
        if !self.at_column_top() && (f.keep_lines || f.keep_next) && total + keep_with > remaining && total + keep_with <= capacity {
            self.break_column();
        } else if !self.at_column_top() && f.widow_control && m.lines.len() > 1 && m.lines[0].height <= remaining && m.lines[0].height + m.lines[1].height > remaining && total > remaining {
            // Orphan control: never leave a paragraph's first line alone at a column bottom.
            self.break_column();
        }
        // A first line that won't fit moves the paragraph on before its floats are placed, so
        // they land on the page the paragraph starts on.
        if let Some(l0) = m.lines.first() {
            if !self.at_column_top() && self.y + l0.skip + l0.height + self.notes_need(l0) > self.bottom() + 0.01 { self.break_column(); }
        }
        let (mut left, mut width) = self.col_geom();
        // Floats anchored here land relative to this paragraph's top; lines then wrap around any
        // float on the page.
        let para_top = self.y;
        let page = self.page;
        // Page ink anchored inside the paragraph, past its first line, waits for the page that
        // line lands on (ink drawn on a page the paragraph continues onto); other floats go with
        // the paragraph's first page, as in Word.
        let first_end = m.lines.first().map_or(usize::MAX, |l| l.end);
        let later = |f: &FloatSpec| f.pos >= first_end && f.anchor.y_from == crate::model::RelFrom::Page && matches!(f.object.as_deref(), Some(crate::objects::Object::Ink(i)) if !i.canvas);
        let mut pending: Vec<FloatSpec> = m.floats.iter().filter(|f| later(f)).cloned().collect();
        if pending.is_empty() { self.place_floats(m, page, left, width, para_top, true); }
        else { let now = Measured { floats: m.floats.iter().filter(|f| !later(f)).cloned().collect(), ..m.clone() }; self.place_floats(&now, page, left, width, para_top, true); }
        // Lines wrap around the page's floats; after a column or page break the rest of the
        // paragraph is measured again for the new place's floats (or none) and width.
        let measured_width = m.lines.first().map(|_| width).unwrap_or(width);
        let mut cur: std::borrow::Cow<Measured> = match self.remeasure_rest(m, left, width, None, para_top, false) { Some(r) => std::borrow::Cow::Owned(r), None => std::borrow::Cow::Borrowed(m) };
        let mut wrapped = matches!(cur, std::borrow::Cow::Owned(_));
        let mut cur_width = measured_width;
        let mut frag_top = self.y;
        let mut frag_first = true;
        let mut k = 0;
        while k < cur.lines.len() {
            let n = cur.lines.len();
            self.y += cur.lines[k].skip;
            let need = self.notes_need(&cur.lines[k]);
            let lh = cur.lines[k].height;
            // Above footnotes a paragraph's last line also needs its space after clear of the separator.
            let notes = self.foot_space + need > 0.0;
            let after = if notes { f.space_after * PT } else { 0.0 };
            let mut must_break = self.y + lh + need + if k + 1 == n { after } else { 0.0 } > self.bottom() + 0.01 && !self.at_column_top();
            // Widow control: carry one more line so the last column holds two.
            if !must_break && f.widow_control && n > 2 && k == n - 2 && k > 0 {
                let need = lh + cur.lines[n - 1].height + after;
                if self.y + need > self.bottom() + 0.01 && !self.at_column_top() { must_break = true; }
            }
            if must_break {
                self.paint_paragraph_box(&cur, self.page, left, width, frag_top, self.y, frag_first, false);
                self.break_column();
                (left, width) = self.col_geom();
                frag_top = self.y; frag_first = false;
                let resized = (width - cur_width).abs() > 0.5;
                if wrapped || resized || !self.bands(left, self.y).is_empty() {
                    let from = cur.lines[k].start;
                    if let Some(r) = self.remeasure_rest(m, left, width, Some(from), self.y, resized) {
                        wrapped = true;
                        cur_width = width;
                        cur = std::borrow::Cow::Owned(r);
                        k = 0;
                        self.y += cur.lines[0].skip;
                    }
                }
            }
            let l = &cur.lines[k];
            let (page, y) = (self.page, self.y);
            if !pending.is_empty() {
                // An anchor that ends a line (it takes no room, so wrapping leaves it there) goes
                // with the next line.
                let next = cur.lines.get(k + 1).map_or(usize::MAX, |n| n.start);
                let (here, rest): (Vec<FloatSpec>, Vec<FloatSpec>) = pending.drain(..).partition(|f| f.pos + 1 < next || next == usize::MAX);
                pending = rest;
                if !here.is_empty() { let now = Measured { floats: here, ..m.clone() }; self.place_floats(&now, page, left, width, para_top, false); }
            }
            let l = &cur.lines[k];
            self.emit(&cur, l, page, left, y, None);
            // A page break that ends its paragraph keeps the mark (and space after) on the old page: the
            // empty line after it holds the caret but takes no room, so the next paragraph starts at the top.
            let mark_after_break = k + 1 == n && k > 0 && cur.lines[k - 1].page_break_after && l.start == l.end;
            if !mark_after_break { self.y += l.height; }
            let (page_break, column_break) = (l.page_break_after, l.column_break_after);
            if page_break || column_break {
                self.paint_paragraph_box(&cur, self.page, left, width, frag_top, self.y, frag_first, false);
                if column_break { self.break_column(); } else { self.new_page(); }
                (left, width) = self.col_geom();
                frag_top = self.y; frag_first = false;
                let resized = (width - cur_width).abs() > 0.5;
                if wrapped || resized || !self.bands(left, self.y).is_empty() {
                    if let Some(next) = cur.lines.get(k + 1).map(|l| l.start) {
                        if let Some(r) = self.remeasure_rest(m, left, width, Some(next), self.y, resized) { wrapped = true; cur_width = width; cur = std::borrow::Cow::Owned(r); k = 0; continue; }
                    }
                }
            }
            k += 1;
        }
        if f.shading.is_some() || f.borders != ParaBorders::default() {
            self.paint_paragraph_box(&cur, self.page, left, width, frag_top, self.y, frag_first, true);
        }
    }

    /// Measure a paragraph again (from a position on) for this place's floats and width.
    #[allow(clippy::too_many_arguments)]
    fn remeasure_rest(&mut self, m: &Measured, left: f32, width: f32, from: Option<usize>, top: f32, force: bool) -> Option<Measured> {
        let bands = self.bands(left, top);
        let body = m.paragraph < self.ctx.doc.paragraphs.len();
        if !body || (bands.is_empty() && from.is_none() && !force) { return None; }
        let p = &self.ctx.doc.paragraphs[m.paragraph];
        let mut r = self.ctx.measure_with(m.paragraph, p, m.start, width, m.marker.clone(), &bands, from);
        r.floats = vec![];
        Some(r)
    }

    /// Lay out one table: rows flow down pages; cells split across pages; header rows repeat.
    fn place_table(&mut self, ti: usize, paragraphs: &[(usize, usize)]) {
        let doc = self.ctx.doc;
        let table = &doc.tables[ti];
        let (col_left, col_width) = self.col_geom();
        let max_cols = table.rows.iter().map(|r| (r.grid_before + r.grid_after) as usize + r.cells.iter().map(|c| c.span.max(1) as usize).sum::<usize>()).max().unwrap_or(0);
        let mut grid: Vec<f32> = table.grid.iter().map(|g| twips(*g)).collect();
        let writing = col_width;
        if grid.len() < max_cols || grid.iter().sum::<f32>() <= 0.0 {
            // Missing grids share the writing width.
            grid = vec![writing / max_cols.max(1) as f32; max_cols];
        }
        // Nested and autofit tables fit the space they're in, unless the table has a width of its
        // own (w:tblW in twips): like Word, that is kept and may run past the margin.
        let sum: f32 = grid.iter().sum();
        if (table.parent.is_some() || !table.fixed) && !has_own_width(table) && sum > writing + 0.5 { for g in grid.iter_mut() { *g *= writing / sum; } }
        let spans = cell_spans(table, paragraphs, doc);
        if !table.fixed { self.autofit_grid(ti, paragraphs, &spans, &mut grid, writing); }
        let table_w: f32 = grid.iter().sum();
        let shift = match table.alignment { Alignment::Center => (writing - table_w) / 2.0, Alignment::Right => writing - table_w, _ => 0.0 };
        // A right-to-left table (w:bidiVisual) is mirrored: its first column is on the right,
        // its indent and alignment count from the right margin.
        let col_x = if table.rtl() {
            let mut xs = vec![col_left + writing - twips(table.indent) - shift];
            for g in &grid { xs.push(xs.last().unwrap() - g); }
            xs
        } else {
            let mut xs = vec![col_left + twips(table.indent) + shift];
            for g in &grid { xs.push(xs.last().unwrap() + g); }
            xs
        };
        let t = TableGeo {
            spans,
            col_x,
            borders: table.borders || table.style.as_deref().is_some_and(|s| doc.styles.table_style_borders(s)),
        };
        let header_rows = table.rows.iter().take_while(|r| r.header).count();
        let start = (self.lines.len(), self.page, self.y, self.cells.len());
        for r in 0..table.rows.len() {
            let mut mark = (self.lines.len(), self.page, self.y, self.cells.len());
            if !self.place_row(ti, r, paragraphs, &t, true) {
                // Repeated header rows are never left alone at the foot of a page: when the
                // first body row moves on, they move with it.
                if r == header_rows && header_rows > 0 && start.1 == mark.1 && start.2 > self.body_top + 0.5 { mark = start; }
                // The row moves whole to a fresh page, after repeated header rows.
                self.lines.truncate(mark.0);
                self.cells.truncate(mark.3);
                self.retract_items(mark.1, mark.2);
                self.page = mark.1;
                self.break_column();
                if r >= header_rows { for h in 0..header_rows { self.place_row(ti, h, paragraphs, &t, false); } }
                self.place_row(ti, r, paragraphs, &t, false);
            }
        }
    }

    /// Autofit layout: a column narrower than its widest unbreakable content (a long word, an
    /// equation, a picture) widens to fit it, taking the room from columns with space to spare;
    /// the table grows up to the writing width when that isn't enough.
    fn autofit_grid(&mut self, ti: usize, paragraphs: &[(usize, usize)], spans: &HashMap<u64, (usize, usize)>, grid: &mut [f32], writing: f32) {
        let doc = self.ctx.doc;
        let table = &doc.tables[ti];
        let n = grid.len();
        if n == 0 { return; }
        let mut need = vec![0.0f32; n];
        for (r, row) in table.rows.iter().enumerate() {
            let mut g = row.grid_before as usize;
            for cell in &row.cells {
                let span = cell.span.max(1) as usize;
                if span == 1 && g < n {
                    let m4 = cell_margins(table, cell);
                    let mut widest = 0.0f32;
                    let own = spans.get(&cell.id).map(|&(a, b)| &paragraphs[a..b]).unwrap_or(&[]);
                    for &(pi, start) in own.iter().filter(|(pi, _)| doc.paragraphs[*pi].cell == Some(cell.id)) {
                        let m = self.ctx.measure(pi, &doc.paragraphs[pi], start, 1.0);
                        for l in &m.lines {
                            let w: f32 = l.atoms[..l.trail.min(l.atoms.len())].iter().map(|a| a.width).sum();
                            widest = widest.max(w + l.left.max(0.0));
                        }
                    }
                    // A nested table needs at least a narrow column per its own columns.
                    let nested = doc.tables.iter().filter(|t| t.parent == Some(cell.id)).map(|t| t.grid.len() as f32 * 24.0).fold(0.0, f32::max);
                    need[g] = need[g].max(widest.max(nested) + m4[1] + m4[3] + 1.0);
                }
                let _ = r;
                g += span;
            }
        }
        let total: f32 = grid.iter().sum();
        let deficit: f32 = (0..n).map(|k| (need[k] - grid[k]).max(0.0)).sum();
        if deficit <= 0.5 { return; }
        // Take from columns wider than their needs, in proportion to their slack.
        let slack: f32 = (0..n).map(|k| (grid[k] - need[k]).max(0.0)).sum();
        let room = (writing - total).max(0.0);
        let take = deficit.min(slack);
        for k in 0..n {
            if need[k] > grid[k] { grid[k] = need[k]; }
            else if slack > 0.0 { grid[k] -= (grid[k] - need[k]).max(0.0) / slack * take; }
        }
        // What slack couldn't cover widens the table, up to the writing width; beyond that the
        // widened columns share the shortfall.
        let over = deficit - take;
        if over > room + 0.5 {
            let excess = over - room;
            let widened: f32 = (0..n).filter(|&k| need[k] > 0.0).map(|k| grid[k]).sum();
            if widened > 0.0 { for g in grid.iter_mut() { *g -= *g / widened * excess; } }
        }
    }

    /// Place one row at the cursor. Returns false (having placed it) when the row
    /// should rather start on a new page and `may_move` allows it.
    fn place_row(&mut self, ti: usize, row_index: usize, paragraphs: &[(usize, usize)], t: &TableGeo, may_move: bool) -> bool {
        let doc = self.ctx.doc;
        let table = &doc.tables[ti];
        let row = &table.rows[row_index];
        let row_top_page = self.page;
        let row_top = self.y;
        let mut results: Vec<(&crate::model::Cell, f32, f32, usize, f32, Vec<usize>)> = vec![];
        // Cells holding a nested table keep their content at the top (it isn't shifted).
        let mut nested: Vec<u64> = vec![];
        let mut g = row.grid_before as usize;
        let last_x = t.col_x.len() - 1;
        // Each cell's grid column (§17.4.14 gridBefore, then the spans before it).
        let gcols: HashMap<u64, usize> = row.cells.iter().scan(row.grid_before as usize, |at, c| { let here = *at; *at += c.span as usize; Some((c.id, here)) }).collect();
        for cell in &row.cells {
            // Only this cell's own paragraphs (and tables nested in it).
            let paragraphs = t.spans.get(&cell.id).map(|&(a, b)| &paragraphs[a..b]).unwrap_or(&[]);
            let span = cell.span.max(1) as usize;
            let (ea, eb) = (t.col_x[g.min(last_x)], t.col_x[(g + span).min(last_x)]);
            let (cx0, cx1) = (ea.min(eb), ea.max(eb));
            g += span;
            if cell.v_merge == VMerge::Continue { results.push((cell, cx0, cx1, row_top_page, row_top, vec![])); continue; }
            let m4 = cell_margins(table, cell);
            let inner = (cx1 - cx0 - m4[1] - m4[3]).max(4.0);
            let (mut page, mut y) = (row_top_page, row_top + m4[0]);
            let mut prev: Option<(ParaFormat, Option<String>)> = None;
            let mut ids = vec![];
            let mut k = 0;
            while k < paragraphs.len() {
                let (pi, start) = paragraphs[k];
                let pcell = doc.paragraphs[pi].cell;
                if pcell != Some(cell.id) {
                    // A table nested in this cell: placed in the cell's content box.
                    if let Some(child) = pcell.and_then(|c| doc.child_table_in_cell(cell.id, c)) {
                        let from = k;
                        while k < paragraphs.len() && doc.paragraphs[paragraphs[k].0].cell.is_some_and(|c| doc.table_contains_cell(child, c)) { k += 1; }
                        if let Some((pf, _)) = &prev { y += pf.space_after * PT; }
                        let saved = (std::mem::take(&mut self.cols), self.col, self.page, self.y, self.col_top, self.col_bottom, self.col_max_y, self.col_separator);
                        self.cols = vec![(cx0 + m4[3], inner)];
                        (self.col, self.page, self.y, self.col_top, self.col_bottom, self.col_max_y, self.col_separator) = (0, page, y, y, None, y, false);
                        self.place_table(child, &paragraphs[from..k]);
                        (page, y) = (self.page, self.y);
                        (self.cols, self.col, self.page, self.y, self.col_top, self.col_bottom, self.col_max_y, self.col_separator) = saved;
                        nested.push(cell.id);
                        prev = None;
                        continue;
                    }
                    k += 1;
                    continue;
                }
                k += 1;
                let m = self.ctx.measure(pi, &doc.paragraphs[pi], start, inner);
                let mut gap = m.format.space_before * PT;
                if let Some((pf, ps)) = &prev {
                    gap = gap.max(pf.space_after * PT);
                    if *ps == m.style && (pf.contextual_spacing || m.format.contextual_spacing) { gap = 0.0; }
                }
                y += gap;
                // Floats in a cell position from the cell's content box (layoutInCell).
                self.place_floats(&m, page, cx0 + m4[3], inner, y, false);
                for l in &m.lines {
                    if y + l.height > (if page == self.page { self.bottom() } else { self.body_bottom }) + 0.01 && y > self.body_top + 0.5 { page += 1; y = self.body_top; }
                    self.emit(&m, l, page, cx0 + m4[3], y, Some(cell.id));
                    ids.push(self.lines.len() - 1);
                    y += l.height;
                }
                prev = Some((m.format.clone(), m.style.clone()));
            }
            if let Some((pf, _)) = &prev { y += pf.space_after * PT; }
            y += m4[2];
            results.push((cell, cx0, cx1, page, y, ids));
        }
        let (mut end_page, mut end_y) = (row_top_page, row_top + twips(row.min_height));
        // A vertically merged cell's content spans its rows: it sets no height until its last row,
        // which grows to fit whatever the earlier rows didn't.
        let continues_below = |cell: &crate::model::Cell| -> Option<usize> {
            let gcol = *gcols.get(&cell.id)?;
            let next = table.rows.get(row_index + 1)?;
            let mut at = next.grid_before as usize;
            next.cells.iter().any(|c| { let hit = at == gcol && c.v_merge == VMerge::Continue; at += c.span.max(1) as usize; hit }).then_some(gcol)
        };
        for r in &results {
            let gcol = gcols.get(&r.0.id).copied().unwrap_or(0);
            if r.0.v_merge == VMerge::Restart && continues_below(r.0).is_some() { self.merge_ends.insert((ti, gcol), (r.3, r.4)); continue; }
            let (p, y) = if r.0.v_merge == VMerge::Continue {
                match (continues_below(r.0), self.merge_ends.get(&(ti, gcol)).copied()) {
                    (None, Some(end)) => { self.merge_ends.remove(&(ti, gcol)); end }
                    _ => (r.3, r.4),
                }
            } else { (r.3, r.4) };
            if p > end_page || p == end_page && y > end_y { end_page = p; end_y = y; }
        }
        if row.exact && row.min_height > 0 && end_page == row_top_page { end_y = row_top + twips(row.min_height); }
        // A row that continues on the next page moves there whole when it must not split
        // (cantSplit) or when no cell could place its first line on this page; otherwise it
        // splits across the page boundary, as rows do by default.
        // A row's minimum (or exact) height is reserved space: when it overflows what is left
        // of the page, the row starts on the next one (and splits from there if taller than a page).
        let min_h = twips(row.min_height);
        if may_move && row.min_height > 0 && row_top > self.body_top + 0.5 && row_top + min_h > self.bottom() + 0.01 && min_h <= self.body_bottom - self.body_top { return false; }
        if may_move && end_page > row_top_page && row_top > self.body_top + 0.5 {
            let height: f32 = results.iter().map(|r| r.5.iter().map(|&i| self.lines[i].height).sum::<f32>()).fold(0.0, f32::max);
            let nothing_here = results.iter().all(|r| r.5.first().is_none_or(|&i| self.lines[i].page > row_top_page));
            if (row.cant_split && height <= self.body_bottom - self.body_top) || nothing_here { return false; }
        }
        for (cell, cx0, cx1, _, _, _) in &results {
            let mut spans_down = false;
            if cell.v_merge == VMerge::Restart || cell.v_merge == VMerge::Continue {
                let gcol = gcols.get(&cell.id).copied().unwrap_or(0);
                spans_down = table.rows.get(row_index + 1).is_some_and(|next| {
                    let mut at = next.grid_before as usize;
                    next.cells.iter().any(|c| { let hit = at == gcol && c.v_merge == VMerge::Continue; at += c.span.max(1) as usize; hit })
                });
            }
            let (gcol, gspan) = gcols.get(&cell.id).map(|&g| (g, cell.span.max(1) as usize)).unwrap_or((0, 1));
            for page in row_top_page..=end_page {
                let top = if page == row_top_page { row_top } else { self.body_top };
                let bottom = if page == end_page { end_y } else { self.body_bottom };
                self.cells.push(CellBox { page, x0: *cx0, y0: top, x1: *cx1, y1: bottom, table: ti, row: row_index, col: gcol, span: gspan, cell: cell.id });
                self.ensure_page(page);
                let pb = &mut self.pages[page];
                if let Some(c) = cell.shading { pb.under.push(DrawItem::Rect { x: *cx0, y: top, w: cx1 - cx0, h: bottom - top, color: c }); }
                // Each side: the cell's own border, else the table's (§17.4.38 / §17.4.39).
                let table_side = t.borders.then_some(Border { width: 0.5, space: 0.0, color: None });
                let b = &cell.borders;
                let side = |own: Option<Border>| own.or(table_side).filter(|x| x.width > 0.0).map(|x| ((x.width * PT).max(0.75), x.color.unwrap_or(Color::BLACK)));
                let mut line = |s: Option<(f32, Color)>, x1: f32, y1: f32, x2: f32, y2: f32| if let Some((w, c)) = s { pb.items.push(DrawItem::Line { x1, y1, x2, y2, width: w, color: c }); };
                // Horizontal rules reach the outer sides of the vertical ones, so thick borders
                // meet in closed square corners.
                let half = |s: Option<(f32, Color)>| s.map(|(w, _)| w / 2.0).unwrap_or(0.0);
                let (ex0, ex1) = (*cx0 - half(side(b.left)), *cx1 + half(side(b.right)));
                if cell.v_merge != VMerge::Continue || page != row_top_page { line(side(b.top), ex0, top, ex1, top); }
                if !spans_down || page != end_page { line(side(b.bottom), ex0, bottom, ex1, bottom); }
                line(side(b.left), *cx0, top, *cx0, bottom);
                line(side(b.right), *cx1, top, *cx1, bottom);
            }
        }
        if end_page == row_top_page {
            for (cell, _, _, _, y, ids) in &results {
                if nested.contains(&cell.id) { continue; }
                let slack = end_y - y;
                let shift = match cell.v_align { CellVAlign::Top => 0.0, CellVAlign::Center => slack / 2.0, CellVAlign::Bottom => slack };
                if shift > 0.5 { self.shift_lines(ids, shift, row_top_page); }
            }
        }
        self.page = end_page;
        self.y = end_y;
        true
    }

    fn shift_lines(&mut self, ids: &[usize], dy: f32, page: usize) {
        let mut ranges = vec![];
        for &i in ids {
            let l = &mut self.lines[i];
            ranges.push((l.y, l.y + l.height, l.stops.first().copied().unwrap_or(0.0), l.right));
            l.y += dy; l.baseline += dy;
        }
        let inside = |x: f32, y: f32| ranges.iter().any(|(t, b, l, r)| y >= *t - 0.01 && y <= *b + 0.01 && x >= *l - 1.0 && x <= *r + 1.0);
        for item in &mut self.pages[page].items {
            match item {
                DrawItem::Glyphs { glyphs, baseline, .. } if glyphs.first().is_some_and(|g| inside(g.x, *baseline)) => *baseline += dy,
                DrawItem::Line { x1, y1, y2, .. } if inside(*x1, *y1) && (*y1 - *y2).abs() < 0.01 => { *y1 += dy; *y2 += dy; }
                DrawItem::Image { x, y, h, .. } if inside(*x, *y + *h) => *y += dy,
                DrawItem::Frame { x, y, h, .. } if inside(*x, *y + *h) => *y += dy,
                DrawItem::Path { points, .. } if points.first().is_some_and(|p| inside(p.0, p.1)) => for p in points.iter_mut() { p.1 += dy; },
                _ => {}
            }
        }
    }

    /// Remove items painted on `page` at or below `y` (a retried table row).
    fn retract_items(&mut self, page: usize, y: f32) {
        let Some(pb) = self.pages.get_mut(page) else { return };
        let below = |item: &DrawItem| match item {
            DrawItem::Glyphs { baseline, .. } => *baseline > y,
            DrawItem::Rect { y: ry, .. } => *ry >= y - 0.01,
            DrawItem::Line { y1, y2, .. } => y1.min(*y2) >= y - 0.01,
            DrawItem::Image { y: iy, .. } | DrawItem::Frame { y: iy, .. } => *iy >= y - 0.01,
            DrawItem::Path { points, .. } => points.iter().all(|p| p.1 >= y - 0.01),
        };
        pb.items.retain(|i| !below(i));
        pb.under.retain(|i| !below(i));
        for p in self.pages.drain(page + 1..) { drop(p); }
    }

    /// Header/footer stories, painted per page with that page's number.
    fn paint_stories(&mut self) {
        let doc = self.ctx.doc;
        let count = self.pages.len();
        self.ctx.page_count = count;
        let geo = self.ctx.page;
        let width = geo.width - geo.left - geo.right;
        // Story positions are not body positions: their PAGE fields follow the page painted.
        let field_pages = std::mem::take(&mut self.ctx.field_pages);
        for page in 0..count {
            if page == 0 && doc.page.title_page { continue; }
            self.ctx.page_number = page + 1;
            for (story, is_header) in [(&doc.header, true), (&doc.footer, false)] {
                let Some(story) = story else { continue };
                let saved = self.lines.len();
                let mut measured = vec![];
                for (k, p) in story.paragraphs.iter().enumerate() { measured.push(self.ctx.measure(usize::MAX - k, p, 0, width)); }
                let total: f32 = measured.iter().map(|m| m.lines.iter().map(|l| l.height).sum::<f32>()).sum();
                let mut y = if is_header { geo.header } else { geo.height - geo.footer - total };
                for m in &measured {
                    // Header and footer floats (logos, watermarks) position from their paragraph.
                    self.place_floats(m, page, geo.left, width, y, false);
                    for l in &m.lines { self.emit(m, l, page, geo.left, y, None); y += l.height; }
                }
                // Stories are not editable here: drop their caret lines.
                self.lines.truncate(saved);
            }
        }
        self.ctx.field_pages = field_pages;
    }
}

/// Lay out a document into pages.
pub fn layout_document(doc: &Document) -> Layout { layout_with_cache(doc, &mut MeasureCache::default()) }

/// Lay out a document, reusing measurements from `cache` (kept by the editor between edits).
/// With PAGEREF fields, a first pass finds the bookmarks' pages and a second shows them.
pub fn layout_with_cache(doc: &Document, cache: &mut MeasureCache) -> Layout {
    let (mut layout, mut floats) = layout_pass(doc, cache, &HashMap::new(), &HashMap::new(), 1, &[]);
    // Wrapping floats are known everywhere on their page only after a pass: lay out again with
    // them in place, until they stop moving (at most three more passes).
    // Drop caps (pos from the top of the range) are placed exactly in one pass.
    let wraps = |f: &Vec<PlacedFloat>| f.iter().any(|p| p.pos < usize::MAX / 2 && !matches!(p.wrap, crate::model::Wrap::Behind | crate::model::Wrap::InFront));
    for _ in 0..3 {
        if !wraps(&floats) { break; }
        let (next, next_floats) = layout_pass(doc, cache, &HashMap::new(), &HashMap::new(), 1, &floats);
        let settled = next_floats.len() == floats.len() && next_floats.iter().zip(&floats).all(|(a, b)| a.page == b.page && (a.y0 - b.y0).abs() < 0.5 && (a.x0 - b.x0).abs() < 0.5);
        layout = next; floats = next_floats;
        if settled { break; }
    }
    let has = |f: fn(&Field) -> bool| doc.paragraphs.iter().any(|p| p.runs.iter().any(|r| r.field.as_ref().is_some_and(f)));
    let page_refs = has(|f| matches!(f, Field::PageRef(_)));
    // NUMPAGES in the body shows the page count only a first pass knows.
    let num_pages = has(|f| matches!(f, Field::NumPages)) && layout.page_count() > 1;
    // PAGE in the body shows the page each field lands on, which a first pass places.
    let page_fields = has(|f| matches!(f, Field::Page)) && layout.page_count() > 1;
    if !page_refs && !num_pages && !page_fields { return layout; }
    let pages: HashMap<String, usize> = if page_refs { doc.bookmarks().into_iter().map(|(n, s, _)| (n, layout.page_of(s))).collect() } else { HashMap::new() };
    let mut field_pages = HashMap::new();
    if page_fields {
        let mut pos = 0;
        for p in &doc.paragraphs {
            for r in &p.runs {
                if r.field == Some(Field::Page) { field_pages.insert(pos, layout.page_of(pos)); }
                pos += r.len();
            }
            pos += 1;
        }
    }
    layout_pass(doc, cache, &pages, &field_pages, layout.page_count(), &floats).0
}

fn layout_pass(doc: &Document, cache: &mut MeasureCache, bookmark_pages: &HashMap<String, usize>, field_pages: &HashMap<usize, usize>, page_count: usize, seed: &[PlacedFloat]) -> (Layout, Vec<PlacedFloat>) {
    let page = doc.view_override.as_ref().map(|v| v.0.geometry()).unwrap_or_else(|| doc.page.geometry());
    let plain = doc.view_override.as_ref().is_some_and(|v| v.1);
    let starts = doc.paragraph_starts();
    // Measure header height to push the body down when the header is taller than the margin.
    let mut placer = Placer {
        ctx: Ctx { doc, page, markers: MarkerCounter::default(), page_number: 1, page_count, cache: std::mem::take(cache), note_labels: doc.note_labels(), bookmark_pages: bookmark_pages.clone(), field_pages: field_pages.clone(), refs: Default::default() },
        pages: vec![], lines: vec![], cells: vec![], page: 0, y: page.top, body_top: page.top, body_bottom: page.height - page.bottom, foot_space: 0.0, page_notes: vec![], note_heights: HashMap::new(), merge_ends: HashMap::new(), floats: vec![], front: vec![], seeded: seed.to_vec(), float_origins: HashMap::new(), cols: vec![(page.left, page.width - page.left - page.right)], col: 0, col_top: page.top, col_bottom: None, col_max_y: page.top, col_separator: false, line_numbers: None, line_no: 0, box_end: None,
    };
    if let Some(h) = doc.header.as_ref().filter(|_| !plain) {
        let width = page.width - page.left - page.right;
        let mut tmp = Ctx { doc, page, markers: MarkerCounter::default(), page_number: 1, page_count: 1, cache: MeasureCache::default(), note_labels: HashMap::new(), bookmark_pages: HashMap::new(), field_pages: HashMap::new(), refs: Default::default() };
        let hh: f32 = h.paragraphs.iter().map(|p| tmp.measure(0, p, 0, width).lines.iter().map(|l| l.height).sum::<f32>()).sum();
        placer.body_top = page.top.max(page.header + hh);
    }
    if let Some(f) = doc.footer.as_ref().filter(|_| !plain) {
        let width = page.width - page.left - page.right;
        let mut tmp = Ctx { doc, page, markers: MarkerCounter::default(), page_number: 1, page_count: 1, cache: MeasureCache::default(), note_labels: HashMap::new(), bookmark_pages: HashMap::new(), field_pages: HashMap::new(), refs: Default::default() };
        let fh: f32 = f.paragraphs.iter().map(|p| tmp.measure(0, p, 0, width).lines.iter().map(|l| l.height).sum::<f32>()).sum();
        placer.body_bottom = (page.height - page.bottom).min(page.height - page.footer - fh);
    }
    placer.new_page();
    // Sections: each paragraph's section index; a paragraph with section settings ends one.
    let mut sections: Vec<&crate::page::Section> = vec![];
    let mut sec_of = Vec::with_capacity(doc.paragraphs.len());
    for p in &doc.paragraphs { sec_of.push(sections.len()); if let Some(s) = &p.section { sections.push(s); } }
    sections.push(&doc.page.section);
    // Balanced columns: a multi-column section followed by a continuous one.
    let balance = |k: usize, placer: &mut Placer| -> Option<f32> {
        let s = sections[k];
        if s.columns.count <= 1 || sections.get(k + 1).is_none_or(|n| n.kind != crate::page::SectionKind::Continuous) { return None; }
        let w = placer.section_columns(s).first().map(|c| c.1).unwrap_or(100.0);
        let mut h = 0.0;
        for (i, p) in doc.paragraphs.iter().enumerate().filter(|(i, _)| sec_of[*i] == k) {
            if p.cell.is_some() { h += 20.0; continue; }
            let mut c = Ctx { doc, page, markers: MarkerCounter::default(), page_number: 1, page_count: 1, cache: MeasureCache::default(), note_labels: HashMap::new(), bookmark_pages: HashMap::new(), field_pages: HashMap::new(), refs: Default::default() };
            let m = c.measure_with(i, p, starts[i], w, None, &[], None);
            h += m.lines.iter().map(|l| l.height).sum::<f32>() + (m.format.space_before + m.format.space_after) * PT;
        }
        Some(h)
    };
    let mut cur_section = 0;
    let h0 = balance(0, &mut placer);
    placer.start_section(sections[0], true, h0);
    let mut prev: Option<(ParaFormat, Option<String>)> = None;
    let mut i = 0;
    while i < doc.paragraphs.len() {
        let p = &doc.paragraphs[i];
        if sec_of[i] != cur_section {
            cur_section = sec_of[i];
            let h = balance(cur_section, &mut placer);
            placer.start_section(sections[cur_section], false, h);
            if sections[cur_section].kind != crate::page::SectionKind::Continuous { prev = None; }
        }
        let width = placer.col_geom().1;
        if let Some(cell) = p.cell {
            // Collect the whole table's paragraphs.
            let ti = doc.root_table_of_cell(cell);
            let mut members = vec![];
            // The cells of the table and of tables nested in it, looked up once (long tables).
            let cells: std::collections::HashSet<u64> = ti.map(|t| doc.tables.iter().enumerate()
                .filter(|(k, x)| *k == t || x.parent.is_some_and(|pc| doc.table_contains_cell(t, pc)))
                .flat_map(|(_, x)| x.rows.iter().flat_map(|r| r.cells.iter().map(|c| c.id))).collect()).unwrap_or_default();
            while i < doc.paragraphs.len() && doc.paragraphs[i].cell.is_some_and(|c| cells.contains(&c)) {
                members.push((i, starts[i])); i += 1;
            }
            if members.is_empty() { members.push((i, starts[i])); i += 1; }
            if let Some(ti) = ti {
                if let Some((pf, _)) = &prev { placer.y += pf.space_after * PT; }
                placer.place_table(ti, &members);
            }
            prev = None;
            continue;
        }
        if let Some(dc) = doc.para_format(p).drop_cap {
            if doc.paragraphs.get(i + 1).is_some_and(|n| n.cell.is_none()) && placer.page < usize::MAX {
                placer.place_drop_cap(i, starts[i], starts[i + 1], dc, width, prev.as_ref());
                i += 1;
                continue;
            }
        }
        let m = placer.ctx.measure(i, p, starts[i], width);
        // Keep-with-next needs the next paragraph's first line height.
        let keep_with = if m.format.keep_next {
            doc.paragraphs.get(i + 1).filter(|n| n.cell.is_none()).map(|n| {
                if let Some(e) = placer.ctx.cache.entries.get(&n.id).filter(|e| e.paragraph == *n && e.width == width) {
                    return e.measured.lines.first().map(|l| l.height).unwrap_or(0.0);
                }
                let mut c = Ctx { doc, page, markers: MarkerCounter::default(), page_number: 1, page_count: 1, cache: MeasureCache::default(), note_labels: HashMap::new(), bookmark_pages: HashMap::new(), field_pages: HashMap::new(), refs: Default::default() };
                c.measure(i + 1, n, starts[i + 1], width).lines.first().map(|l| l.height).unwrap_or(0.0)
            }).unwrap_or(0.0)
        } else { 0.0 };
        placer.place_paragraph(&m, prev.as_ref(), keep_with);
        prev = Some((m.format.clone(), m.style.clone()));
        i += 1;
    }
    placer.place_endnotes();
    placer.paint_notes();
    if !plain { placer.paint_stories(); }
    placer.paint_page_borders();
    // Floats by z-order: behind text just above the page color, then with the text, then in front.
    let mut front = std::mem::take(&mut placer.front);
    front.sort_by_key(|f| f.2);
    let base = usize::from(doc.page.color.is_some());
    let mut behind_count: HashMap<usize, usize> = HashMap::new();
    for (page, layer, _, items) in front {
        placer.ensure_page(page);
        let pb = &mut placer.pages[page];
        match layer {
            0 => { let at = base + *behind_count.get(&page).unwrap_or(&0); let n = items.len(); pb.under.splice(at.min(pb.under.len())..at.min(pb.under.len()), items); *behind_count.entry(page).or_default() += n; }
            1 => pb.items.extend(items),
            _ => pb.over.extend(items),
        }
    }
    let Placer { pages, lines, ctx, cells, floats, float_origins, .. } = placer;
    *cache = ctx.cache;
    let seen = std::mem::take(&mut cache.seen);
    cache.entries.retain(|id, _| seen.contains(id));
    (Layout { pages, lines, revision: doc.revision, cells, float_origins }, floats)
}

// ---------------------------------------------------------------------------
// Geometry queries

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretRect { pub page: usize, pub x: f32, pub y: f32, pub height: f32 }

impl Layout {
    pub fn page_count(&self) -> usize { self.pages.len() }

    /// The line showing `pos`. At a soft-wrap boundary the caret belongs to the next line.
    pub fn line_index(&self, pos: usize) -> Option<usize> {
        let mut fallback = None;
        for (i, l) in self.lines.iter().enumerate() {
            if l.start <= pos && (pos < l.end || pos == l.end && l.last_in_paragraph) { return Some(i); }
            if l.start <= pos && pos <= l.end { fallback = Some(i); }
        }
        fallback.or_else(|| self.lines.iter().rposition(|l| l.start <= pos))
    }

    pub fn caret_rect(&self, pos: usize) -> Option<CaretRect> {
        let i = self.line_index(pos)?;
        let l = &self.lines[i];
        let x = l.stops.get(pos.saturating_sub(l.start)).copied().unwrap_or(*l.stops.last()?);
        Some(CaretRect { page: l.page, x, y: l.y, height: l.height })
    }

    fn pos_in_line(&self, l: &Line, x: f32) -> usize {
        let mut best = l.start;
        let mut best_d = f32::MAX;
        for (k, sx) in l.stops.iter().enumerate() {
            let d = (sx - x).abs();
            if d < best_d { best_d = d; best = l.start + k; }
        }
        // On wrapped lines the end position belongs to the next line.
        if !l.last_in_paragraph && best == l.end && l.end > l.start { best = l.end - 1; }
        best
    }

    /// Document position nearest a point on a page (hitTest).
    pub fn hit_test(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        let on_page: Vec<&Line> = self.lines.iter().filter(|l| l.page == page).collect();
        if on_page.is_empty() {
            // Before or after this page's content: snap to the nearest page with lines.
            let next = self.lines.iter().find(|l| l.page > page);
            return next.map(|l| l.start).or_else(|| self.lines.last().map(|l| l.end));
        }
        let vdist = |l: &Line| if y < l.y { l.y - y } else if y > l.y + l.height { y - l.y - l.height } else { 0.0 };
        let hdist = |l: &Line| { let a = l.stops.first().copied().unwrap_or(l.left).min(l.left); if x < a { a - x } else if x > l.right { x - l.right } else { 0.0 } };
        let best = on_page.iter().min_by(|a, b| {
            let ka = (vdist(a) + hdist(a) * 0.25, hdist(a));
            let kb = (vdist(b) + hdist(b) * 0.25, hdist(b));
            ka.partial_cmp(&kb).unwrap()
        })?;
        Some(self.pos_in_line(best, x))
    }

    /// Up/Down movement keeping a goal x (verticalLine).
    pub fn vertical(&self, pos: usize, down: bool, goal_x: f32) -> Option<usize> {
        let cur = &self.lines[self.line_index(pos)?];
        let key = |l: &Line| (l.page, l.y);
        let ahead = |l: &&Line| if down { key(l) > (cur.page, cur.y + cur.height - 0.5) } else { key(l) < (cur.page, cur.y - cur.height + 0.5) && (l.page < cur.page || l.y + l.height <= cur.y + 0.5) };
        let candidates: Vec<&Line> = self.lines.iter().filter(ahead).collect();
        let nearest = if down { candidates.iter().min_by(|a, b| key(a).partial_cmp(&key(b)).unwrap()) } else { candidates.iter().max_by(|a, b| key(a).partial_cmp(&key(b)).unwrap()) }?;
        // Stay on the nearest visual row (e.g. a table's first row), then prefer the line under goal_x there.
        let row: Vec<&Line> = candidates.iter().copied().filter(|l| l.page == nearest.page && l.y < nearest.y + nearest.height && nearest.y < l.y + l.height).collect();
        let gap = |l: &&Line| if goal_x < l.left { l.left - goal_x } else if goal_x > l.right { goal_x - l.right } else { 0.0 };
        let target = row.iter().min_by(|a, b| gap(a).total_cmp(&gap(b)).then(if down { key(a).partial_cmp(&key(b)).unwrap() } else { key(b).partial_cmp(&key(a)).unwrap() }))?;
        Some(self.pos_in_line(target, goal_x))
    }

    /// Start/end positions of the visual line containing `pos` (Home/End).
    pub fn line_bounds(&self, pos: usize) -> Option<(usize, usize)> {
        let l = &self.lines[self.line_index(pos)?];
        let mut end = l.end;
        if !l.last_in_paragraph && end > l.start { end -= 1; }
        Some((l.start, end))
    }

    /// Selection highlight rectangles (page, x, y, w, h) (selectionRects).
    pub fn selection_rects(&self, start: usize, end: usize) -> Vec<(usize, f32, f32, f32, f32)> {
        let mut out = vec![];
        if start >= end { return out; }
        for l in &self.lines {
            if l.end < start || l.start > end || (l.start == end && l.start != l.end) { continue; }
            // The union of the selected characters' boxes: mixed-direction selections can be split.
            let mut spans: Vec<(f32, f32)> = (start.max(l.start)..end.min(l.end)).filter_map(|p| l.boxes.get(p - l.start).copied()).filter(|b| b.1 > b.0).collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = vec![];
            for s in spans { match merged.last_mut() { Some(m) if s.0 <= m.1 + 0.5 => m.1 = m.1.max(s.1), _ => merged.push(s) } }
            // Selected paragraph marks show as a small extension at the line's end side.
            if end > l.end && l.last_in_paragraph {
                let edge = l.stops.last().copied().unwrap_or(l.left);
                merged.push(if l.rtl { (edge - 6.0, edge) } else { (edge, edge + 6.0) });
            }
            for (x0, x1) in merged { out.push((l.page, x0, l.y, x1 - x0, l.height)); }
        }
        out
    }

    /// Visual Left/Right movement in a mixed-direction line: the nearest caret stop to the
    /// right or left. None when the line is plain left to right or at its visual edge.
    pub fn visual_step(&self, pos: usize, right: bool) -> Option<usize> {
        let l = &self.lines[self.line_index(pos)?];
        if !l.bidi { return None; }
        let x = l.stops.get(pos - l.start).copied()?;
        let last = if l.last_in_paragraph { l.end } else { l.end.saturating_sub(1).max(l.start) };
        (l.start..=last).filter(|&p| p != pos).map(|p| (p, l.stops[p - l.start]))
            .filter(|&(_, sx)| if right { sx > x + 0.5 } else { sx < x - 0.5 })
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map(|(p, _)| p)
    }

    /// Whether the caret's line belongs to a right-to-left paragraph.
    pub fn line_rtl(&self, pos: usize) -> bool { self.line_index(pos).is_some_and(|i| self.lines[i].rtl) }

    pub fn page_of(&self, pos: usize) -> usize { self.caret_rect(pos).map(|c| c.page).unwrap_or(0) }

    /// Image draw item at a point, with its position.
    pub fn image_at(&self, page: usize, x: f32, y: f32) -> Option<(usize, f32, f32, f32, f32)> {
        let hit = |i: &DrawItem| match i {
            DrawItem::Image { x: ix, y: iy, w, h, pos, .. } | DrawItem::Frame { x: ix, y: iy, w, h, pos } if x >= *ix && x <= ix + w && y >= *iy && y <= iy + h => Some((*pos, *ix, *iy, *w, *h)),
            _ => None,
        };
        let p = self.pages.get(page)?;
        if let Some(found) = p.over.iter().rev().chain(p.items.iter().rev()).find_map(hit) { return Some(found); }
        // Behind-text objects take clicks only where no text covers them.
        let over_text = self.lines.iter().any(|l| l.page == page && y >= l.y && y < l.y + l.height && l.end > l.start && x >= l.stops.iter().copied().fold(f32::MAX, f32::min) && x <= l.stops.iter().copied().fold(f32::MIN, f32::max));
        if over_text { return None; }
        p.under.iter().rev().find_map(hit)
    }
    /// The box of the picture or object at `pos`, wherever it is drawn (inline, floating, behind).
    pub fn object_box(&self, pos: usize) -> Option<(usize, f32, f32, f32, f32)> {
        self.pages.iter().enumerate().find_map(|(pg, p)| p.over.iter().chain(p.items.iter()).chain(p.under.iter()).find_map(|i| match i {
            DrawItem::Image { x, y, w, h, pos: ip, .. } | DrawItem::Frame { x, y, w, h, pos: ip } if *ip == pos => Some((pg, *x, *y, *w, *h)),
            _ => None,
        }))
    }
}

/// Whether a table read from a file has its own preferred width in twips (`w:tblW w:type="dxa"`).
fn has_own_width(table: &crate::model::Table) -> bool {
    let Some(props) = table.raw_props.as_deref().and_then(|r| crate::xml::parse(r).ok()) else { return false };
    props.elements().find(|e| e.name == "w:tblW").is_some_and(|w| w.attr("w:type") == Some("dxa") && w.attr("w:w").and_then(|v| v.parse::<i32>().ok()).unwrap_or(0) > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, Paragraph};

    fn doc(text: &str) -> Document { Document::new(text.split('\n').map(Paragraph::new).collect()) }


    /// Consecutive paragraphs with the same outside borders share one box: no lines between them.
    #[test]
    fn matching_paragraph_borders_draw_one_box() {
        let mut d = doc("السطر الأول\nالسطر الثاني\nالسطر الثالث");
        let pen = Some(crate::format::Border { width: 0.5, space: 1.0, color: None });
        for p in &mut d.paragraphs { p.props.borders = Some(ParaBorders { top: pen, bottom: pen, left: pen, right: pen }); }
        let l = layout_document(&d);
        let flat = l.pages[0].items.iter().filter(|i| matches!(i, DrawItem::Line { y1, y2, .. } if y1 == y2)).count();
        assert_eq!(flat, 2, "one top and one bottom line around the three paragraphs");
        let sides: Vec<(f32, f32)> = l.pages[0].items.iter().filter_map(|i| match i { DrawItem::Line { x1, x2, y1, y2, .. } if x1 == x2 => Some((*y1, *y2)), _ => None }).collect();
        let mut tops: Vec<f32> = sides.iter().map(|s| s.0).collect(); tops.sort_by(f32::total_cmp);
        let mut ends: Vec<f32> = sides.iter().map(|s| s.1).collect(); ends.sort_by(f32::total_cmp);
        assert!(tops.iter().skip(2).all(|t| ends.iter().any(|e| (e - t).abs() < 0.01)), "side lines run on without gaps: {sides:?}");
    }

    /// PAGE in the body shows the page it lands on, not 1.
    #[test]
    fn body_page_field_shows_its_own_page() {
        let mut d = doc(&vec!["سطر line"; 120].join("\n"));
        let end = d.paragraph_start(d.paragraphs.len() - 1);
        d.select(end, end);
        d.insert_field(crate::model::Field::Page);
        d.select(0, 0);
        d.insert_field(crate::model::Field::Page);
        let l = layout_document(&d);
        let n = l.page_count();
        assert!(n > 2);
        let text = |k: usize| -> String { l.pages[k].items.iter().filter_map(|i| match i { DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| g.ch).collect::<String>()), _ => None }).collect() };
        assert!(text(0).contains("رطس1line"), "first page: {}", text(0));
        assert!(text(n - 1).contains(&n.to_string()), "last page shows its number: {}", text(n - 1));
    }

    /// NUMPAGES in a body paragraph shows the document's page count, not 1.
    #[test]
    fn body_numpages_field_shows_the_page_count() {
        let mut d = doc(&vec!["سطر line"; 120].join("\n"));
        d.select(0, 0);
        d.insert_field(crate::model::Field::NumPages);
        let l = layout_document(&d);
        assert!(l.page_count() > 2);
        let first: String = l.pages[0].items.iter().filter_map(|i| match i { DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| g.ch).collect::<String>()), _ => None }).collect();
        assert!(first.contains(&l.page_count().to_string()), "page count shown in the body: {first}");
    }

    #[test]
    fn ideographs_and_latin_or_digits_get_a_quarter_em_apart() {
        // "於83年": Word adds automatic spacing on both sides of the digits.
        let spaced = layout_document(&doc("於83年"));
        let plain = layout_document(&doc("於年83"));
        let end = |l: &Layout| l.caret_rect(4).unwrap().x;
        let gap = |l: &Layout| l.caret_rect(1).unwrap().x;
        assert!(end(&spaced) - end(&plain) > 1.0, "two gaps against one: {} vs {}", end(&spaced), end(&plain));
        assert!(gap(&spaced) - layout_document(&doc("於年")).caret_rect(1).unwrap().x > 1.0, "space after the ideograph before a digit");
        // Arabic next to digits gets no extra space.
        let a = layout_document(&doc("سنة 83"));
        assert!(a.caret_rect(6).is_some());
    }

    #[test]
    fn right_tab_past_the_margin_keeps_its_text_on_the_line() {
        // a resume sets 0.4" margins and a right stop at 8.06", past the right
        // margin (8.1" text width); its dates stay on the heading's line, as in Word.
        let mut d = doc("Florida Polytechnic University\tExpected Graduation: May 2028");
        d.page.left = 576; d.page.right = 576;
        d.paragraphs[0].props.tabs = Some(vec![TabStop { pos: 11610, kind: TabKind::Right, leader: Leader::None }]);
        let l = layout_document(&d);
        assert_eq!(l.lines.len(), 1, "the dated heading stays on one line");
        let end = l.caret_rect(d.paragraphs[0].text().chars().count()).unwrap();
        assert!((end.x - (twips(576) + twips(11610))).abs() < 1.5, "text ends at the stop: {}", end.x);
        // A left stop past the margin still wraps.
        d.paragraphs[0].props.tabs = Some(vec![TabStop { pos: 11610, kind: TabKind::Left, leader: Leader::None }]);
        assert!(layout_document(&d).lines.len() > 1);
    }

    #[test]
    fn text_after_leading_tabs_that_does_not_fit_takes_its_tab_to_the_next_line() {
        // two leading tabs reach a left stop near the margin; the greeting doesn't fit there,
        // so the second tab moves down with it and centres it at the centre stop.
        let mut d = doc("\t\tGreetings");
        d.page.width = 11906; d.page.left = 1440; d.page.right = 1440;
        d.paragraphs[0].props.tabs = Some(vec![TabStop { pos: 5018, kind: TabKind::Center, leader: Leader::None }, TabStop { pos: 8625, kind: TabKind::Left, leader: Leader::None }]);
        d.paragraphs[0].props.first_line = Some(539);
        let l = layout_document(&d);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.lines[1].start, 1, "the second tab starts the second line");
        let (a, b) = (l.caret_rect(2).unwrap(), l.caret_rect(11).unwrap());
        let centre = twips(d.page.left) + twips(5018);
        assert!(((a.x + b.x) / 2.0 - centre).abs() < 1.5, "greeting centred at the stop: {} {}", a.x, b.x);
    }

    #[test]
    fn all_caps_sharp_s_shows_two_letters() {
        let mut d = doc("Straße");
        d.select(0, 6);
        d.set_run_props(crate::format::RunProps { caps: Some(Caps::AllCaps), ..Default::default() });
        let l = layout_document(&d);
        let shown: String = l.pages[0].items.iter().filter_map(|i| match i { DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| g.ch).collect::<String>()), _ => None }).collect();
        assert_eq!(shown, "STRASSE");
        assert!(l.caret_rect(6).is_some());
    }

    #[test]
    fn wraps_and_paginates() {
        let long = "word ".repeat(4000);
        let d = doc(&long);
        let l = layout_document(&d);
        assert!(l.page_count() > 3, "long text spans pages: {}", l.page_count());
        for w in l.lines.windows(2) { assert!(w[0].end <= w[1].start + 1, "lines are in order"); }
        // Caret and hit-test agree.
        let c = l.caret_rect(100).unwrap();
        let hit = l.hit_test(c.page, c.x + 0.1, c.y + c.height / 2.0).unwrap();
        assert_eq!(hit, 100);
    }

    #[test]
    fn page_breaks_and_alignment() {
        let mut d = doc("a\u{000C}b");
        assert_eq!(layout_document(&d).page_count(), 2);
        d = doc("center me");
        d.select(0, 0);
        d.set_alignment(Alignment::Center);
        let l = layout_document(&d);
        let x = l.caret_rect(0).unwrap().x;
        assert!(x > 200.0, "centered text starts mid-page ({x})");
    }

    #[test]
    fn page_break_ending_a_paragraph_leaves_next_at_page_top() {
        // the space after a paragraph that ends in a page break stays on the old page.
        let mut d = Document::new(vec![Paragraph::new("a\u{000C}"), Paragraph::new("b")]);
        d.select(0, 4);
        d.set_paragraph_spacing(Some(30.0), Some(30.0));
        let l = layout_document(&d);
        let b = l.lines.iter().find(|x| x.paragraph == 1).unwrap();
        let mark = l.lines.iter().find(|x| x.paragraph == 0 && x.page == 1).unwrap();
        assert_eq!(b.page, 1);
        assert!((b.y - mark.y).abs() < 0.01, "next paragraph starts at the top ({} vs {})", b.y, mark.y);
        assert!(l.caret_rect(2).is_some_and(|c| c.page == 1));
    }

    #[test]
    fn vertical_movement_and_tables() {
        let mut d = doc("first\nsecond");
        let l = layout_document(&d);
        let down = l.vertical(2, true, l.caret_rect(2).unwrap().x).unwrap();
        assert!((7..=8).contains(&down), "moves into the second paragraph near the same x ({down})");
        assert_eq!(l.vertical(down, false, l.caret_rect(down).unwrap().x), Some(2));
        d.select(0, 0);
        d.insert_table(3, 3);
        let l = layout_document(&d);
        assert_eq!(l.lines.len(), 2 + 9 + 1);
        // Cells in one row share a top.
        let cells: Vec<&Line> = l.lines.iter().filter(|x| x.cell.is_some()).collect();
        assert!((cells[0].y - cells[1].y).abs() < 0.01 && cells[1].stops[0] > cells[0].stops[0]);
    }

    #[test]
    fn down_from_line_start_enters_table_below() {
        // From the start of the paragraph above a table, Down lands in the first cell, not past the table.
        let mut d = doc("above\nبعد الجدول");
        d.select(0, 0);
        d.insert_table(2, 2);
        let l = layout_document(&d);
        let above = l.lines.iter().position(|x| x.cell.is_some()).unwrap() - 1;
        let start = l.lines[above].start;
        let down = l.vertical(start, true, l.caret_rect(start).unwrap().x).unwrap();
        let first_cell = l.lines.iter().find(|x| x.cell.is_some()).unwrap();
        assert_eq!(down, first_cell.start);
        let up = l.vertical(down, false, l.caret_rect(down).unwrap().x).unwrap();
        assert_eq!(l.lines[l.line_index(up).unwrap()].start, start);
    }

    #[test]
    fn long_tables_lay_out_in_linear_time() {
        // 1,500 rows × 6 columns: each row looks only at its own cells' paragraphs.
        let mut d = doc("قبل الجدول");
        d.select(0, 0);
        d.insert_table(1500, 6);
        let t = std::time::Instant::now();
        let l = layout_document(&d);
        assert!(t.elapsed().as_secs_f32() < 5.0, "{:?}", t.elapsed());
        assert!(l.pages.len() > 30);
        assert_eq!(l.lines.iter().filter(|x| x.cell.is_some()).count(), 9000);
    }

    /// Twenty lines of Arabic text, then a two-row one-column table; `edit` sets its rows up.
    fn table_after_text(edit: impl Fn(&mut Document)) -> (Document, Layout) {
        let mut d = doc(&[vec!["سطر من النص"; 20], vec!["بعد"]].concat().join("\n"));
        d.select(d.paragraph_start(20), d.paragraph_start(20));
        d.insert_table(2, 1);
        edit(&mut d);
        let l = layout_document(&d);
        (d, l)
    }

    #[test]
    fn rtl_tables_put_the_first_column_on_the_right() {
        let mut d = doc("نص");
        d.select(0, 0);
        d.insert_table(1, 3);
        let ltr = layout_document(&d);
        d.tables[0].raw_props = Some(r#"<w:tblPr><w:bidiVisual/></w:tblPr>"#.into());
        let rtl = layout_document(&d);
        let col = |l: &Layout, c: usize| l.cells.iter().find(|x| x.col == c).map(|x| (x.x0, x.x1)).unwrap();
        assert!(col(&ltr, 0).0 < col(&ltr, 2).0, "left to right: first column on the left");
        assert!(col(&rtl, 0).0 > col(&rtl, 2).0, "right to left: first column on the right");
        let geo = d.page.geometry();
        assert!((col(&rtl, 0).1 - (geo.width - geo.right)).abs() < 1.0, "it starts at the right margin");
        assert!((col(&rtl, 0).1 - col(&rtl, 0).0 - (col(&ltr, 0).1 - col(&ltr, 0).0)).abs() < 0.5, "same widths");
    }

    #[test]
    fn rows_with_a_minimum_height_start_on_the_next_page() {
        // 9000 twips (450 pt) doesn't fit under twenty lines, but fits a page.
        let (_, l) = table_after_text(|d| d.tables[0].rows[0].min_height = 9000);
        let first: Vec<usize> = l.cells.iter().filter(|c| c.row == 0).map(|c| c.page).collect();
        assert_eq!(first, vec![1], "the row starts whole on page 2");
        // Taller than a page, it still starts where it is.
        let (_, l) = table_after_text(|d| d.tables[0].rows[0].min_height = 20000);
        assert_eq!(l.cells.iter().filter(|c| c.row == 0).map(|c| c.page).min(), Some(0));
    }

    #[test]
    fn repeated_header_row_moves_with_the_first_row() {
        let (_, l) = table_after_text(|d| { d.tables[0].rows[0].header = true; d.tables[0].rows[1].min_height = 9000; });
        let header: Vec<usize> = l.cells.iter().filter(|c| c.row == 0).map(|c| c.page).collect();
        assert_eq!(header, vec![1], "no header row alone at the foot of page 1: {header:?}");
        assert_eq!(l.cells.iter().filter(|c| c.row == 1).map(|c| c.page).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn a_table_with_its_own_width_keeps_it_past_the_margin() {
        let wide = |own: bool| {
            let (d, l) = table_after_text(|d| {
                d.tables[0].grid = vec![13608];
                if own { d.tables[0].raw_props = Some(r#"<w:tblPr><w:tblW w:w="13608" w:type="dxa"/></w:tblPr>"#.into()); }
            });
            let c = l.cells.iter().find(|c| c.row == 0).unwrap();
            (c.x1 - c.x0, d)
        };
        let (kept, d) = wide(true);
        assert!((kept - twips(13608)).abs() < 0.5, "{kept}");
        let geo = d.page.geometry();
        let (squeezed, _) = wide(false);
        assert!(squeezed <= geo.width - geo.left - geo.right + 0.5, "an automatic-width table still fits the margins: {squeezed}");
    }

    #[test]
    fn thick_table_borders_meet_in_closed_corners() {
        use crate::tables::BorderPreset;
        let mut d = doc("جدول\nبعد");
        d.select(0, 0);
        d.insert_table(2, 2);
        let cells: Vec<usize> = (0..d.paragraphs.len()).filter(|&i| d.paragraphs[i].cell.is_some()).collect();
        d.select(d.paragraph_start(cells[0]), d.paragraph_start(*cells.last().unwrap()));
        d.apply_border_preset(BorderPreset::All, crate::format::Border { width: 3.0, space: 0.0, color: None });
        let l = layout_document(&d);
        let lines: Vec<(f32, f32, f32, f32, f32)> = l.pages[0].items.iter().filter_map(|i| match i { DrawItem::Line { x1, y1, x2, y2, width, .. } if *width > 3.0 => Some((*x1, *y1, *x2, *y2, *width)), _ => None }).collect();
        let left = lines.iter().filter(|v| v.0 == v.2).map(|v| v.0).fold(f32::MAX, f32::min);
        let w = 3.0 * PT;
        let top = lines.iter().filter(|h| h.1 == h.3).map(|h| h.0.min(h.2)).fold(f32::MAX, f32::min);
        assert!((top - (left - w / 2.0)).abs() < 0.01, "the top rule starts at the left rule's outer side: {top} vs {left}");
    }

    #[test]
    fn cached_layout_matches_fresh_layout() {
        let mut d = doc(&(0..40).map(|i| format!("Paragraph {i} {}", "text ".repeat(i % 7 * 9))).collect::<Vec<_>>().join("\n"));
        d.select(0, 30);
        d.toggle_list(crate::numbering::ListKind::Number);
        let mut cache = MeasureCache::default();
        let _ = layout_with_cache(&d, &mut cache);
        let same = |a: &Layout, b: &Layout| a.lines.len() == b.lines.len() && a.lines.iter().zip(&b.lines).all(|(x, y)| (x.page, x.start, x.end, x.paragraph) == (y.page, y.start, y.end, y.paragraph) && (x.y - y.y).abs() < 0.01 && x.stops == y.stops);
        // Typing, splitting, joining, list changes and undo all keep the cached layout exact.
        let edits: Vec<Box<dyn Fn(&mut Document)>> = vec![
            Box::new(|d| { d.select(3, 3); d.replace("X"); }),
            Box::new(|d| { d.select(50, 50); d.insert_paragraph(); }),
            Box::new(|d| { d.select(12, 12); d.delete(false, false); }),
            Box::new(|d| { d.select(0, 0); d.toggle_list(crate::numbering::ListKind::Number); }),
            Box::new(|d| { d.undo(); }),
            Box::new(|d| { d.select(200, 260); d.toggle_bold(); }),
        ];
        for edit in edits {
            edit(&mut d);
            cache.hits = 0;
            let cached = layout_with_cache(&d, &mut cache);
            assert!(cache.hits > 0 || d.format_epoch > 0, "unchanged paragraphs are reused");
            assert!(same(&cached, &layout_document(&d)), "cached layout differs after an edit");
        }
    }

    #[test]
    fn footnotes_sit_at_the_page_foot() {
        let mut d = doc(&(0..60).map(|i| format!("Paragraph {i} with enough words to wrap across the line width at least once here.")).collect::<Vec<_>>().join("\n"));
        d.select(5, 5);
        let id = d.insert_note(NoteKind::Foot);
        d.set_note_text(NoteKind::Foot, &id, "The footnote text.");
        let l = layout_document(&d);
        let geo = d.page.geometry();
        let page = &l.pages[0];
        let note_glyph_y = page.items.iter().filter_map(|i| match i { DrawItem::Glyphs { glyphs, baseline, .. } if glyphs.iter().any(|g| g.ch == 'T') && *baseline > geo.height - geo.bottom - 40.0 => Some(*baseline), _ => None }).next();
        assert!(note_glyph_y.is_some(), "note text is painted at the foot of page 1");
        let last_body = l.lines.iter().filter(|x| x.page == 0).map(|x| x.y + x.height).fold(0.0, f32::max);
        assert!(last_body < note_glyph_y.unwrap() - 20.0, "body text stops above the notes");
        // The reference shows its number.
        assert!(page.items.iter().any(|i| matches!(i, DrawItem::Glyphs { glyphs, .. } if glyphs.len() == 1 && glyphs[0].ch == '1')));
    }

    #[test]
    fn space_after_clears_the_footnote_separator() {
        // A page with notes ends with a paragraph's last line only if its space after fits too.
        for after in [6.0, 9.0, 12.0, 15.0, 18.0] {
            let mut d = doc(&(0..80).map(|i| format!("Line {i}")).collect::<Vec<_>>().join("\n"));
            for p in &mut d.paragraphs { p.props.space_before = Some(0.0); p.props.space_after = Some(after); }
            d.select(3, 3);
            let id = d.insert_note(NoteKind::Foot);
            d.set_note_text(NoteKind::Foot, &id, "A note.");
            let l = layout_document(&d);
            let geo = d.page.geometry();
            let sep = l.pages[0].items.iter().filter_map(|i| match i { DrawItem::Line { x1, y1, x2, width, .. } if *x1 == geo.left && *x2 == geo.left + 144.0 && *width == 0.75 => Some(*y1), _ => None }).next().expect("separator");
            let last = l.lines.iter().filter(|x| x.page == 0).map(|x| x.y + x.height).fold(0.0, f32::max);
            // The separator sits half its band below the writing area's bottom.
            let gap = sep - NOTE_SEPARATOR / 2.0 - last;
            assert!(gap >= after * PT - 0.01, "space after {after} pt clears the separator: gap {gap}");
        }
    }

    #[test]
    fn caret_inside_a_ligature_sits_between_its_letters() {
        // "سلام": lam and alef draw as one lam-alef glyph.
        let d = doc("سلام");
        let l = layout_document(&d);
        let s = &l.lines[0].stops;
        assert!(s.windows(2).all(|w| w[1] < w[0] - 0.5), "every stop strictly leftward, none stuck at the ligature's end: {s:?}");
        assert!(s[2] < s[1] - 1.0 && s[2] > s[3] + 1.0, "between lam and alef is inside the ligature: {s:?}");
    }

    #[test]
    fn rtl_and_mixed_direction_lines() {
        let d = doc("مرحبا بالعالم\nabc عربي xyz\nالسعر 123 دولار");
        let l = layout_document(&d);
        let geo = d.page.geometry();
        let right = geo.width - geo.right;
        // RTL paragraph: starts at the right margin; stops move leftward as positions increase.
        let p0 = &l.lines[0];
        assert!(p0.rtl);
        assert!((p0.stops[0] - right).abs() < 1.0, "first stop at the right margin ({} vs {right})", p0.stops[0]);
        assert!(p0.stops.windows(2).all(|w| w[1] <= w[0] + 0.01), "stops decrease");
        // Mixed LTR paragraph: "abc " then the Arabic word reversed, then " xyz".
        let p1 = &l.lines[1];
        assert!(!p1.rtl && p1.bidi);
        let s = |pos: usize| p1.stops[pos - p1.start];
        let start = p1.start;
        assert!(s(start + 4) > s(start + 5) && s(start + 5) > s(start + 6), "Arabic characters run right to left");
        assert!(s(start + 4) > s(start + 2), "the Arabic run sits after 'abc '");
        // Numbers inside Arabic keep left-to-right order.
        let p2 = &l.lines[2];
        let n = p2.start + 6;
        assert!(p2.stops[n - p2.start] < p2.stops[n + 1 - p2.start], "digits run left to right");
        // A selection across the direction boundary is split into boxes.
        let rects = l.selection_rects(start + 2, start + 6);
        assert_eq!(rects.len(), 2, "logical selection over 'c ' + 'عر' is two visual pieces: {rects:?}");
        // Visual arrow movement inside the Arabic word moves left.
        let from = start + 5;
        let to = l.visual_step(from, false).unwrap();
        assert!(p1.stops[to - start] < p1.stops[from - start]);
    }

    #[test]
    fn paragraph_mark_sits_at_the_visual_end_of_mixed_direction_lines() {
        // LTR paragraph ending in Arabic, RTL paragraph ending in Latin.
        let d = doc("abc عربي\nمرحبا بالعالم abc");
        let l = layout_document(&d);
        let (p0, p1) = (&l.lines[0], &l.lines[1]);
        assert!(!p0.rtl && p1.rtl);
        let max_right = p0.boxes.iter().map(|b| b.1).fold(f32::MIN, f32::max);
        assert!(*p0.stops.last().unwrap() < max_right - 1.0, "logical end is inside the Arabic run");
        assert!(p0.visual_end_x() >= max_right - 0.01, "mark after the rightmost glyph");
        let min_left = p1.boxes.iter().map(|b| b.0).fold(f32::MAX, f32::min);
        assert!(*p1.stops.last().unwrap() > min_left + 1.0, "logical end is inside 'abc'");
        assert!(p1.visual_end_x() <= min_left + 0.01, "mark before the leftmost glyph");
        // Single-direction paragraphs are unchanged.
        let d = doc("hello\nمرحبا");
        let l = layout_document(&d);
        for x in &l.lines { assert!((x.visual_end_x() - x.stops.last().unwrap()).abs() < 0.01); }
    }

    #[test]
    fn rtl_underlined_title_is_one_rule_and_shapes_across_runs() {
        use crate::format::RunProps;
        let props = |color: Option<Color>| RunProps { underline: Some("single".into()), rtl: Some(true), size: Some(12.0), size_cs: Some(16.0), bold_cs: Some(true), color: color.map(Some), ..Default::default() };
        let run = |t: &str, c: Option<Color>| Run { text: t.into(), props: props(c), ..Default::default() };
        let mut p = Paragraph::new("");
        // A Word-style split: RLM marks, the space in its own run, a color change mid-word.
        p.runs = vec![run("\u{200F}خطة", None), run("\u{200F} ", None), run("الب", None), run("حث", Some(Color(0x20, 0, 0)))];
        p.props.alignment = Some(Alignment::Center);
        p.props.rtl = Some(true);
        let d = Document::new(vec![p]);
        let l = layout_document(&d);
        let page = &l.pages[0];
        let rules: Vec<_> = page.items.iter().filter(|i| matches!(i, DrawItem::Line { .. })).collect();
        assert_eq!(rules.len(), 2, "one underline per color, contiguous: {rules:?}");
        let ys: Vec<f32> = rules.iter().map(|r| match r { DrawItem::Line { y1, .. } => *y1, _ => 0.0 }).collect();
        assert!((ys[0] - ys[1]).abs() < 0.01, "same underline height");
        // Every glyph (the space too) is at the complex-script size.
        assert!(page.items.iter().all(|i| !matches!(i, DrawItem::Glyphs { size, .. } if (*size - 16.0 * PT).abs() > 0.01)));
        // The word split by color shapes like the whole word.
        let whole = Document::new(vec![{ let mut q = Paragraph::new(""); q.runs = vec![run("البحث", None)]; q.props.rtl = Some(true); q }]);
        let w = |l: &Layout| { let x = &l.lines[0]; x.stops.first().unwrap() - x.stops.last().unwrap() };
        let split = Document::new(vec![{ let mut q = Paragraph::new(""); q.runs = vec![run("الب", None), run("حث", Some(Color(0x20, 0, 0)))]; q.props.rtl = Some(true); q }]);
        assert!((w(&layout_document(&whole)) - w(&layout_document(&split))).abs() < 0.01);
    }

    #[test]
    fn no_break_space_keeps_words_together() {
        // Try every split point near a line end: "a" and "b" always share a line.
        for k in 0..40 {
            let d = doc(&format!("{}{}aaaa\u{00A0}bbbb", "word ".repeat(30), "x".repeat(k)));
            let l = layout_document(&d);
            let n = d.paragraphs[0].len();
            let line_of = |p: usize| l.lines.iter().position(|x| x.start <= p && p < x.end.max(x.start + 1)).unwrap();
            assert_eq!(line_of(n - 9), line_of(n - 1), "k={k}");
        }
    }

    #[test]
    fn overlong_rtl_line_overflows_to_the_left() {
        let d = doc(&"ب".repeat(400));
        let l = layout_document(&d);
        let geo = d.page.geometry();
        let right = geo.width - geo.right;
        for line in &l.lines { assert!(line.stops.iter().all(|s| *s <= right + 1.0), "nothing past the right margin"); }
    }

    #[test]
    fn latin_list_markers_stay_in_order_in_rtl() {
        let mut d = doc("أول\nثاني");
        d.select_all();
        let custom = crate::editing::CustomList::number("Steps", crate::numbering::NumFmt::Decimal, "Step %1:", 1, crate::numbering::Script::Latin);
        d.apply_custom_list(&custom);
        let l = layout_document(&d);
        let page = &l.pages[0];
        // The marker's glyphs in visual order spell "Step1:" (spaces aside), not ":Step 1".
        let mut marker: Vec<(f32, char)> = page.items.iter().filter_map(|i| match i { DrawItem::Glyphs { glyphs, baseline, .. } if *baseline < l.lines[0].baseline + 1.0 => Some(glyphs.iter().filter(|g| g.ch.is_ascii_graphic()).map(|g| (g.x, g.ch)).collect::<Vec<_>>()), _ => None }).flatten().collect();
        marker.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(marker.iter().map(|m| m.1).collect::<String>(), "Step1:");
    }

    #[test]
    fn unknown_symbol_font_bullets_draw_the_default_bullet() {
        // A private-use code no bundled font draws (here Wingdings 0x81) is not left to font
        // fallback, which can find an icon at that code: the level's default bullet is drawn.
        let mut d = doc("بند");
        d.select_all();
        d.apply_custom_list(&crate::editing::CustomList::bullet("Odd", '\u{F081}', Some("Wingdings".into())));
        let l = layout_document(&d);
        let chars: String = l.pages[0].items.iter().filter_map(|i| match i { DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| g.ch).collect::<String>()), _ => None }).collect();
        assert!(chars.contains('\u{2022}') && !chars.contains('\u{F081}'), "{chars:?}");
    }

    #[test]
    fn number_list_markers_follow_rtl_paragraph() {
        let mut d = doc("أول\nثاني");
        d.select_all();
        let custom = crate::editing::CustomList::number("Nums", crate::numbering::NumFmt::Decimal, "%1.", 1, crate::numbering::Script::Latin);
        d.apply_custom_list(&custom);
        let l = layout_document(&d);
        let page = &l.pages[0];
        // Like Word, "1." in a right-to-left paragraph reads ".1": the dot sits between number and text.
        let mut marker: Vec<(f32, char)> = page.items.iter().filter_map(|i| match i { DrawItem::Glyphs { glyphs, baseline, .. } if *baseline < l.lines[0].baseline + 1.0 => Some(glyphs.iter().filter(|g| g.ch.is_ascii_graphic()).map(|g| (g.x, g.ch)).collect::<Vec<_>>()), _ => None }).flatten().collect();
        marker.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(marker.iter().map(|m| m.1).collect::<String>(), ".1");
    }

    #[test]
    fn text_wraps_around_floats() {
        use crate::model::{Anchor, Wrap};
        use crate::objects::{Object, Shape, ShapeKind};
        let words = "word ".repeat(200);
        let float = |wrap: Wrap| {
            let mut d = doc(&format!("{words}\n{words}"));
            d.select(0, 0);
            let mut s = Shape::new(ShapeKind::Rect);
            s.width = 200.0; s.height = 100.0;
            d.insert_object(Object::Shape(s));
            d.set_anchor(0, Some(Anchor::new(wrap, 0.0, 0.0)));
            (d.page.geometry(), layout_document(&d))
        };
        let (g, l) = float(Wrap::Square);
        let first = &l.lines[0];
        assert!(first.stops[0] >= g.left + 200.0 + 11.0, "lines beside the float start right of it: {}", first.stops[0]);
        let below = l.lines.iter().find(|x| x.y > g.top + 120.0).unwrap();
        assert!((below.stops[0] - g.left).abs() < 1.0, "lines below the float use the full width");
        // A float in the middle: text on both sides of it, row by row.
        let mut d = doc(&words);
        d.select(0, 0);
        let mut s = Shape::new(ShapeKind::Rect);
        s.width = 120.0; s.height = 100.0;
        d.insert_object(Object::Shape(s));
        d.set_anchor(0, Some(Anchor::new(Wrap::Square, 250.0, 0.0)));
        let l2 = layout_document(&d);
        let row: Vec<&Line> = l2.lines.iter().filter(|x| (x.y - l2.lines[0].y).abs() < 0.5).collect();
        assert_eq!(row.len(), 2, "two pieces share the first row");
        assert!(row[0].stops.iter().all(|&x| x <= g.left + 250.0 - 11.0) && row[1].stops[0] >= g.left + 370.0 + 11.0, "pieces sit left and right of the float");
        assert_eq!(l2.hit_test(0, g.left + 500.0, row[1].y + 2.0).map(|p| p >= row[1].start), Some(true), "clicks on the right piece land in it");
        let (g, l) = float(Wrap::TopBottom);
        assert!(l.lines[0].y >= g.top + 100.0, "top-and-bottom wrap moves text below: {}", l.lines[0].y);
        let (g, l) = float(Wrap::Behind);
        assert!((l.lines[0].y - g.top).abs() < 0.5 && l.lines[0].stops[0] < g.left + 1.0, "behind text: lines are unaffected");
        assert!(l.pages[0].under.iter().any(|i| matches!(i, DrawItem::Path { .. })), "painted under the text");
        assert_eq!(l.object_box(0).map(|b| (b.3, b.4)), Some((200.0, 100.0)));
        assert!(l.image_at(0, g.left + 50.0, g.top + 5.0).is_none(), "clicks over text reach the text");
    }

    #[test]
    fn outside_aligned_floats_alternate_sides_on_facing_pages() {
        use crate::model::{Anchor, AnchorAlign, Wrap};
        use crate::objects::{Object, Shape, ShapeKind};
        let mut d = doc("اليمين\nاليسار");
        let shape = |d: &mut Document, pos: usize, al: AnchorAlign| {
            d.select(pos, pos);
            let mut s = Shape::new(ShapeKind::Rect);
            s.width = 100.0; s.height = 50.0;
            d.insert_object(Object::Shape(s));
            let mut a = Anchor::new(Wrap::Square, 0.0, 0.0);
            a.x_align = Some(al);
            d.set_anchor(pos, Some(a));
        };
        shape(&mut d, 0, AnchorAlign::Outside);
        d.paragraphs[1].props.page_break_before = Some(true);
        let p2 = d.paragraph_start(1);
        shape(&mut d, p2, AnchorAlign::Outside);
        let g = d.page.geometry();
        let l = layout_document(&d);
        let (pg1, x1, ..) = l.object_box(0).unwrap();
        let (pg2, x2, ..) = l.object_box(p2).unwrap();
        assert_eq!((pg1, pg2), (0, 1));
        assert!((x1 - (g.width - g.right - 100.0)).abs() < 1.0, "outside on page 1 is the right margin: {x1}");
        assert!((x2 - g.left).abs() < 1.0, "outside on page 2 is the left margin: {x2}");
        // Inside is the other way round, and both are kept on save.
        let mut a = d.anchor_at(0).unwrap();
        a.x_align = Some(AnchorAlign::Inside);
        d.set_anchor(0, Some(a));
        assert!((layout_document(&d).object_box(0).unwrap().1 - g.left).abs() < 1.0);
        let e = crate::docx::read(&crate::docx::write(&d).unwrap()).unwrap();
        assert_eq!(e.anchor_at(0).and_then(|a| a.x_align), Some(AnchorAlign::Inside));
        assert_eq!(e.anchor_at(p2).and_then(|a| a.x_align), Some(AnchorAlign::Outside));
    }

    #[test]
    fn floats_wrap_across_pages_above_anchors_tightly_and_in_stories() {
        use crate::model::{Anchor, Run, Story, Wrap};
        use crate::objects::{Object, Shape, ShapeKind};
        let words = "word ".repeat(900);
        let shape = |kind: ShapeKind, w: f32, h: f32| { let mut s = Shape::new(kind); s.width = w; s.height = h; Object::Shape(s) };
        // A float hanging off the bottom of page 1: the paragraph continues full width on page 2.
        let mut d = doc(&words);
        d.select(0, 0);
        d.insert_object(shape(ShapeKind::Rect, 200.0, 400.0));
        d.set_anchor(0, Some(Anchor::new(Wrap::Square, 0.0, 700.0)));
        let g = d.page.geometry();
        let l = layout_document(&d);
        let p2: Vec<&Line> = l.lines.iter().filter(|x| x.page == 1).collect();
        assert!(!p2.is_empty() && p2.iter().all(|x| x.stops[0] < g.left + 1.0), "page 2 lines use the full width");
        assert!(l.lines.iter().any(|x| x.page == 0 && x.y > g.top + 720.0 && x.stops[0] > g.left + 200.0), "page 1 lines beside the float are narrowed");

        // A float anchored in the second paragraph but raised above it: the first paragraph wraps too.
        let mut d = doc(&format!("{}\n{}", "first ".repeat(60), "second ".repeat(60)));
        let p2 = d.paragraph_start(1);
        d.select(p2, p2);
        d.insert_object(shape(ShapeKind::Rect, 150.0, 120.0));
        d.set_anchor(p2, Some(Anchor::new(Wrap::Square, 0.0, -60.0)));
        let l = layout_document(&d);
        let (_, _, fy, ..) = l.object_box(p2).unwrap();
        let beside: Vec<&Line> = l.lines.iter().filter(|x| x.paragraph == 0 && x.y + x.height > fy - 12.0).collect();
        assert!(!beside.is_empty() && beside.iter().all(|x| x.stops[0] > g.left + 150.0), "text above the anchor paragraph wraps");

        // Tight wrap follows an ellipse: near its top and bottom text comes closer than square wrap.
        let starts = |wrap: Wrap| {
            let mut d = doc(&words);
            d.select(0, 0);
            d.insert_object(shape(ShapeKind::Ellipse, 200.0, 200.0));
            d.set_anchor(0, Some(Anchor::new(wrap, 0.0, 0.0)));
            layout_document(&d).lines.iter().filter(|x| x.page == 0 && x.y < g.top + 200.0).map(|x| x.stops[0]).collect::<Vec<f32>>()
        };
        let (sq, tight) = (starts(Wrap::Square), starts(Wrap::Tight));
        assert!(sq.iter().all(|&x| x > g.left + 200.0));
        assert!(tight[0] < sq[0] - 20.0 && tight[tight.len() / 2] > g.left + 190.0, "tight: {tight:?} square: {sq:?}");

        // Header floats paint on every page, unselectable; a float in a table cell stays in the cell.
        let mut d = doc("Body");
        let mut hp = Paragraph::new("");
        let mut run = Run { text: crate::chars::OBJECT.to_string(), object: Some(shape(ShapeKind::Star5, 60.0, 60.0)), ..Default::default() };
        run.anchor = Some(Anchor::new(Wrap::InFront, 400.0, 0.0));
        hp.runs = vec![run];
        d.header = Some(Story { paragraphs: vec![hp] });
        d.select(4, 4);
        d.insert_table(1, 2);
        let cell_pos = d.paragraphs.iter().position(|p| p.cell.is_some()).map(|i| d.paragraph_start(i)).unwrap();
        d.select(cell_pos, cell_pos);
        d.insert_object(shape(ShapeKind::Rect, 40.0, 30.0));
        d.set_anchor(cell_pos, Some(Anchor::new(Wrap::Square, 10.0, 0.0)));
        let l = layout_document(&d);
        assert!(l.pages[0].over.iter().any(|i| matches!(i, DrawItem::Path { .. })), "the header star is drawn in front");
        assert!(l.image_at(0, g.left + 430.0, g.header + 30.0).is_none(), "header objects aren't selectable from the body");
        let (_, x, ..) = l.object_box(cell_pos).expect("the cell float is drawn");
        let cell = l.cells.iter().find(|c| c.col == 0).unwrap();
        assert!(x >= cell.x0 + 10.0 && x < cell.x1, "positioned from the cell: {x} in {}..{}", cell.x0, cell.x1);
    }

    #[test]
    fn justify_fills_width() {
        let mut d = doc(&"lorem ipsum dolor ".repeat(40));
        d.select(0, 0);
        d.set_alignment(Alignment::Justify);
        let l = layout_document(&d);
        let first = &l.lines[0];
        let geo = d.page.geometry();
        let last_char_end = first.stops[first.end - first.start - 1];
        assert!((last_char_end - (geo.width - geo.right)).abs() < 8.0);
    }

    #[test]
    fn caret_stays_at_the_margin_after_hanging_spaces() {
        // Held-down spacebar: the spaces hang past the margin, the caret stays at it.
        for (t, rtl) in [("Word", false), ("مرحبا", true)] {
            let d = Document::new(vec![Paragraph::new(&format!("{t}{}", " ".repeat(400)))]);
            let lay = layout_document(&d);
            assert_eq!(lay.lines.len(), 1);
            let l = &lay.lines[0];
            let x = lay.caret_rect(d.len()).unwrap().x;
            assert!(x >= l.left - 0.5 && x <= l.right + 0.5, "{t}: caret at {x} inside {}..{}", l.left, l.right);
            let edge = if rtl { l.left } else { l.right };
            assert!((x - edge).abs() < 0.5, "{t}: caret at the end-side margin");
        }
    }
}
