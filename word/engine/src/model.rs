//! Document model and editing transactions (model.js).
//!
//! Like the JS engine, body and table-cell paragraphs live in one flat list;
//! a paragraph's `cell` names the table cell that owns it. Positions are
//! character offsets in paragraphs joined by '\n'.

use crate::chars;
use crate::format::*;
use crate::numbering::{ListKind, Numbering};
use crate::page::PageSettings;
use crate::styles::StyleSheet;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub fn new_id() -> u64 { NEXT_ID.fetch_add(1, Ordering::Relaxed) }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Image {
    /// Package part path of the media, e.g. `word/media/image1.png`.
    pub asset: String,
    pub width: f32, // px
    pub height: f32,
    pub alt: String,
    /// Original `w:drawing` XML, written back when the picture is unchanged.
    pub raw: Option<String>,
    /// The asset before any picture edits (Reset Picture), saved in an extension element.
    pub original: Option<String>,
    /// Clockwise rotation in degrees (a:xfrm rot).
    pub rotation: f32,
    /// Mirroring (a:xfrm flipH/flipV).
    pub flip_h: bool,
    pub flip_v: bool,
    /// Picture Format edits since `original`, in order. Those Word can express are saved as
    /// DrawingML over the original pixels; `asset` holds them baked for display.
    pub effects: Vec<crate::picture::PictureOp>,
}

/// Fields whose result the layout computes (body-fields.js): page numbers, and cross-references
/// to a bookmark's text (REF) or page (PAGEREF).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    Page, NumPages, Ref(String), PageRef(String),
    /// DATE/TIME with a Word date-time picture (`\@ "dddd, MMMM d, yyyy"`); "TIME:" prefixes TIME.
    Date(String),
    /// A document property field: AUTHOR, TITLE, SUBJECT, KEYWORDS, FILENAME, NUMWORDS, NUMCHARS.
    Prop(String),
    /// A sequence number (SEQ Figure): captions.
    Seq(String),
    /// A citation of a source by tag (CITATION).
    Citation(String),
    /// An index entry, "Main" or "Main:Sub" (XE); shows nothing.
    IndexEntry(String),
    /// A table-of-authorities entry (TA): long and short citation, category 1–16; shows nothing.
    Authority { long: String, short: String, category: u8 },
    /// Mail merge: a recipient column (MERGEFIELD).
    Merge(String),
    /// Mail merge: the recipient's address, composed from matched columns (ADDRESSBLOCK).
    AddressBlock,
    /// Mail merge: "Dear First Last," — greeting word, name format, punctuation (GREETINGLINE).
    GreetingLine { greeting: String, name: crate::mailmerge::NameFormat, punctuation: String },
    /// Mail merge rules: IF column op value "then" "else"; the record and sequence numbers;
    /// move to the next record (NEXT), when a condition holds (NEXTIF), or skip it (SKIPIF).
    MergeIf { field: String, op: crate::mailmerge::Compare, value: String, then: String, otherwise: String },
    MergeRec, MergeSeq,
    NextRecord,
    NextIf { field: String, op: crate::mailmerge::Compare, value: String },
    SkipIf { field: String, op: crate::mailmerge::Compare, value: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevKind { Ins, Del }

/// A tracked change on a run or paragraph mark (revisions.js, §17.13.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision { pub kind: RevKind, pub author: String, pub date: String }

/// How tracked changes display (Review → Display for Review).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Markup { #[default] All, Simple, None, Original }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteKind { Foot, End }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRef { pub kind: NoteKind, pub id: String }

/// A footnote or endnote body (footnotes.js).
#[derive(Clone, Debug, PartialEq)]
pub struct Note { pub kind: NoteKind, pub id: String, pub paragraphs: Vec<Paragraph> }

/// Zero-width anchors carried as empty runs, so every edit that moves text moves them too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Marker {
    CommentStart(String), CommentEnd(String), BookmarkStart(String), BookmarkEnd(String),
    /// The start (with its instruction) and end of a field whose result is ordinary paragraphs:
    /// TOC, INDEX, TOA, BIBLIOGRAPHY.
    FieldBegin(String), FieldEnd,
}

/// A comment body (comments.js). Replies name their parent; the thread shares the parent's anchor.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub initials: String,
    pub date: String,
    pub text: String,
    pub parent: Option<String>,
    pub done: bool,
    /// w14:paraId of the comment's last paragraph (links commentsExtended entries).
    pub para_id: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Run {
    pub text: String,
    pub props: RunProps,
    pub style: Option<String>,
    /// Hyperlink target (URL) or `#bookmark`.
    pub link: Option<String>,
    pub image: Option<Image>,
    pub field: Option<Field>,
    /// Unrecognized `w:rPr` children, preserved verbatim.
    pub extra: Vec<String>,
    /// Zero-width anchor (text is empty).
    pub marker: Option<Marker>,
    /// Tracked insertion or deletion.
    pub revision: Option<Revision>,
    /// Footnote/endnote reference (text is `chars::NOTE`).
    pub note: Option<NoteRef>,
    /// A shape, chart or diagram (text is `chars::OBJECT`).
    pub object: Option<crate::objects::Object>,
    /// Floating placement of a picture or object (wp:anchor); None is inline.
    pub anchor: Option<Anchor>,
}

/// How text flows around a floating picture or object (§20.4.2.15–20.4.2.18).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Wrap { #[default] Square, Tight, TopBottom, Behind, InFront }

/// What a floating object's offset is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RelFrom { #[default] Margin, Page, Paragraph }

/// Alignment within the reference box, instead of an offset (wp:align).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorAlign { Start, Center, End }

/// A floating object's position: offsets in px from `x_from`/`y_from` (or an alignment), the
/// wrap style and the gap kept between it and the text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub wrap: Wrap,
    pub x: f32,
    pub y: f32,
    pub x_from: RelFrom,
    pub y_from: RelFrom,
    pub x_align: Option<AnchorAlign>,
    pub y_align: Option<AnchorAlign>,
    pub dist: f32,
    /// Stacking order among floats (wp:anchor relativeHeight); higher is in front.
    pub z: i64,
}

impl Anchor {
    /// Float at offset (x, y) from the margin and the anchor paragraph's top.
    pub fn new(wrap: Wrap, x: f32, y: f32) -> Anchor {
        Anchor { wrap, x, y, x_from: RelFrom::Margin, y_from: RelFrom::Paragraph, x_align: None, y_align: None, dist: 12.0, z: 0 }
    }
}

impl Run {
    pub fn marker(m: Marker) -> Run { Run { marker: Some(m), ..Default::default() } }
    pub fn text(text: impl Into<String>) -> Run { Run { text: text.into(), ..Default::default() } }
    pub fn len(&self) -> usize { self.text.chars().count() }
    pub fn is_empty(&self) -> bool { self.text.is_empty() }
    /// Whether two runs can merge without losing formatting.
    fn same_format(&self, o: &Run) -> bool {
        self.props == o.props && self.style == o.style && self.link == o.link && self.extra == o.extra && self.image.is_none() && o.image.is_none() && self.field.is_none() && o.field.is_none() && self.marker.is_none() && o.marker.is_none() && self.revision == o.revision && self.note.is_none() && o.note.is_none() && self.object.is_none() && o.object.is_none()
    }
    fn with_text(&self, text: String) -> Run { Run { text, ..self.clone() } }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub id: u64,
    pub runs: Vec<Run>,
    pub props: ParaProps,
    pub style: Option<String>,
    pub cell: Option<u64>,
    /// Paragraph-mark run properties (empty paragraph formatting).
    pub mark: RunProps,
    /// Tracked insertion/deletion of this paragraph's mark.
    pub mark_revision: Option<Revision>,
    /// Unrecognized `w:pPr` children, preserved verbatim.
    pub extra: Vec<String>,
    /// This paragraph ends a section with these settings (a w:sectPr in its w:pPr).
    pub section: Option<crate::page::Section>,
}

impl Default for Paragraph {
    fn default() -> Self { Paragraph { id: new_id(), runs: vec![], props: ParaProps::default(), style: None, cell: None, mark: RunProps::default(), mark_revision: None, extra: vec![], section: None } }
}

impl Paragraph {
    pub fn new(text: &str) -> Paragraph {
        let mut p = Paragraph::default();
        if !text.is_empty() { p.runs.push(Run::text(text)); }
        p
    }
    pub fn text(&self) -> String { self.runs.iter().map(|r| r.text.as_str()).collect() }
    pub fn len(&self) -> usize { self.runs.iter().map(Run::len).sum() }
    pub fn is_empty(&self) -> bool { self.runs.iter().all(Run::is_empty) }
    /// An empty copy that keeps paragraph-level formatting.
    pub fn sibling(&self) -> Paragraph {
        Paragraph { id: new_id(), runs: vec![], props: self.props.clone(), style: self.style.clone(), cell: self.cell, mark: self.mark.clone(), mark_revision: None, extra: self.extra.clone(), section: None }
    }
    /// Split runs so a boundary falls at `offset`; returns the index of the first run at/after it.
    fn split_at(&mut self, offset: usize) -> usize {
        let mut at = 0;
        for i in 0..self.runs.len() {
            let len = self.runs[i].len();
            if offset == at { return i; }
            if offset < at + len {
                let byte = char_byte(&self.runs[i].text, offset - at);
                let tail = self.runs[i].text.split_off(byte);
                let next = self.runs[i].with_text(tail);
                self.runs.insert(i + 1, next);
                return i + 1;
            }
            at += len;
        }
        self.runs.len()
    }
    /// Remove the runs between two offsets, returning them.
    fn take(&mut self, start: usize, end: usize) -> Vec<Run> {
        self.split_at(end);
        let a = self.split_at(start);
        let b = self.split_at(end);
        let taken: Vec<Run> = self.runs.drain(a..b).collect();
        // Anchors survive deletion of the text around them.
        let (markers, text): (Vec<Run>, Vec<Run>) = taken.into_iter().partition(|r| r.marker.is_some());
        self.runs.splice(a..a, markers);
        text
    }
    pub fn normalize_public(&mut self) { self.normalize(); }
    fn normalize(&mut self) {
        let mut out: Vec<Run> = Vec::with_capacity(self.runs.len());
        for r in self.runs.drain(..) {
            if r.text.is_empty() && r.marker.is_none() { continue; }
            if let Some(last) = out.last_mut() { if last.same_format(&r) { last.text.push_str(&r.text); continue; } }
            out.push(r);
        }
        self.runs = out;
    }
    fn link_ends_at(&self, offset: usize) -> bool {
        let mut at = 0;
        for (k, r) in self.runs.iter().enumerate() {
            at += r.len();
            if at == offset { return r.link.is_some() && self.runs.get(k + 1).is_none_or(|n| n.link != r.link); }
        }
        false
    }
    /// The run supplying formatting at `offset` (the run before it, unless at the start).
    pub fn run_at(&self, offset: usize) -> Option<&Run> {
        let mut at = 0;
        let mut found = None;
        for r in &self.runs {
            let len = r.len();
            if offset > at && offset <= at + len { return Some(r); }
            if found.is_none() && offset == at && len > 0 { found = Some(r); }
            at += len;
        }
        found
    }
}

pub fn char_byte(s: &str, chars: usize) -> usize { s.char_indices().nth(chars).map(|(b, _)| b).unwrap_or(s.len()) }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VMerge { #[default] None, Restart, Continue }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CellVAlign { #[default] Top, Center, Bottom }

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub id: u64,
    pub width: i32, // twips
    pub span: u32,
    pub v_merge: VMerge,
    pub shading: Option<Color>,
    pub v_align: CellVAlign,
    /// Direct borders per side (tcBorders); `None` inherits the table's, width 0 is "no border".
    pub borders: CellBorders,
    /// Direct margins (tcMar) in twips: top, right, bottom, left.
    pub margins: Option<[i32; 4]>,
    pub extra: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct CellBorders { pub top: Option<Border>, pub right: Option<Border>, pub bottom: Option<Border>, pub left: Option<Border> }

impl Cell {
    pub fn new(width: i32) -> Cell { Cell { id: new_id(), width, span: 1, v_merge: VMerge::None, shading: None, v_align: CellVAlign::Top, borders: CellBorders::default(), margins: None, extra: vec![] } }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub min_height: i32,
    pub exact: bool,
    pub header: bool,
    pub cant_split: bool,
    /// Grid columns skipped before the first cell and after the last (w:gridBefore/After).
    pub grid_before: u32,
    pub grid_after: u32,
    /// Unrecognized `w:trPr` children, preserved verbatim.
    pub extra: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub id: u64,
    pub grid: Vec<i32>,
    pub rows: Vec<Row>,
    pub borders: bool,
    pub style: Option<String>,
    pub indent: i32,
    pub alignment: Alignment,
    /// Default cell margins (tblCellMar) in twips: top, right, bottom, left.
    pub cell_margins: [i32; 4],
    /// Original `w:tblPr` XML, written back unchanged.
    pub raw_props: Option<String>,
    /// The cell this table sits in (a nested table), if any.
    pub parent: Option<u64>,
    /// Fixed column widths (w:tblLayout fixed); otherwise columns widen to fit content that
    /// can't wrap, as Word's autofit layout does.
    pub fixed: bool,
}

impl Table {
    pub fn cells(&self) -> impl Iterator<Item = &Cell> { self.rows.iter().flat_map(|r| r.cells.iter()) }
    pub fn has_cell(&self, id: u64) -> bool { self.cells().any(|c| c.id == id) }
    /// (row, column index in row) of a cell.
    pub fn find_cell(&self, id: u64) -> Option<(usize, usize)> {
        self.rows.iter().enumerate().find_map(|(r, row)| row.cells.iter().position(|c| c.id == id).map(|c| (r, c)))
    }
    /// The grid column at which a cell starts.
    pub fn grid_col(&self, row: usize, cell: usize) -> usize { self.rows[row].grid_before as usize + self.rows[row].cells[..cell].iter().map(|c| c.span as usize).sum::<usize>() }
}

#[derive(Clone, Debug)]
pub struct Asset { pub bytes: Vec<u8>, pub content_type: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Selection { pub anchor: usize, pub focus: usize }

impl Selection {
    pub fn range(&self) -> (usize, usize) { (self.anchor.min(self.focus), self.anchor.max(self.focus)) }
    pub fn is_caret(&self) -> bool { self.anchor == self.focus }
}

#[derive(Clone)]
struct Snapshot { paragraphs: Vec<Paragraph>, tables: Vec<Table>, comments: Vec<Comment>, notes: Vec<Note>, numbering: Numbering, styles: StyleSheet, page: PageSettings, selection: Selection, header: Option<Story>, footer: Option<Story> }

/// Document properties (docProps/core.xml) shown by property fields and Quick Parts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Properties { pub title: String, pub subject: String, pub author: String, pub keywords: String, pub description: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind { Typing, Deleting, Other }

/// A header or footer story.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Story { pub paragraphs: Vec<Paragraph> }

pub struct Document {
    pub paragraphs: Vec<Paragraph>,
    pub tables: Vec<Table>,
    pub styles: StyleSheet,
    pub numbering: Numbering,
    pub page: PageSettings,
    pub assets: HashMap<String, Asset>,
    pub header: Option<Story>,
    pub footer: Option<Story>,
    pub selection: Selection,
    /// Formatting for the next typed text at a caret.
    pub typing: Option<RunProps>,
    /// Bumps on every content change; layout caches key on it.
    pub revision: u64,
    pub dirty: bool,
    pub styles_changed: bool,
    /// Bumps when styles, numbering or other inputs to every paragraph's measurement may change.
    pub format_epoch: u64,
    pub comments: Vec<Comment>,
    /// Comments were edited, so comments.xml must be rewritten on save.
    pub comments_changed: bool,
    /// Record edits as tracked changes (w:trackRevisions).
    pub track_changes: bool,
    pub track_changes_changed: bool,
    pub revision_author: String,
    pub markup: Markup,
    /// Supplies dates for new comments and revisions (set by the app).
    pub clock: fn() -> String,
    /// Local date and time (ISO, no zone) for DATE/TIME fields.
    pub local_clock: fn() -> String,
    pub properties: Properties,
    /// The file name for FILENAME fields.
    pub file_name: String,
    /// Headers or footers were edited (their parts are written anew).
    pub stories_changed: bool,
    /// Macros stored in this document, and the recorder while Record Macro is on.
    pub macros: Vec<crate::macros::Macro>,
    pub recorder: Option<crate::macros::Recorder>,
    /// The customXml part the macros were read from.
    pub macros_part: Option<String>,
    /// Web Layout and Draft lay the document out on this page instead (never saved); `true`
    /// leaves out headers, footers and page decorations.
    pub view_override: Option<(crate::page::PageSettings, bool)>,
    /// Restrict Editing (w:documentProtection), when enforced.
    pub protection: Option<crate::review::Protection>,
    /// Mail merge: document type, recipients and the record being previewed.
    pub mail_merge: crate::mailmerge::MailMerge,
    /// Bibliography sources (customXml b:Sources) and the citation style.
    pub sources: Vec<crate::references::Source>,
    pub citation_style: crate::references::CitationStyle,
    pub sources_changed: bool,
    /// The customXml part the sources were read from.
    pub sources_part: Option<String>,
    /// Style ids edited this session ("" is the document defaults), rewritten in styles.xml.
    pub style_edits: std::collections::BTreeSet<String>,
    /// Footnote and endnote bodies, plus the parts' special entries (separators) kept verbatim.
    pub notes: Vec<Note>,
    pub note_specials: Vec<(NoteKind, String)>,
    pub notes_changed: bool,
    pub package: Option<crate::docx::Package>,
    history: Vec<Snapshot>,
    future: Vec<Snapshot>,
    last_edit: Option<EditKind>,
}

impl Default for Document { fn default() -> Self { Document::new(vec![Paragraph::default()]) } }

impl Document {
    pub fn new(paragraphs: Vec<Paragraph>) -> Document {
        Document {
            paragraphs: if paragraphs.is_empty() { vec![Paragraph::default()] } else { paragraphs },
            tables: vec![], styles: StyleSheet::default(), numbering: Numbering::default(), page: PageSettings::default(),
            assets: HashMap::new(), header: None, footer: None, selection: Selection::default(), typing: None,
            revision: 0, dirty: false, styles_changed: false, format_epoch: 0, comments: vec![], comments_changed: false, track_changes: false, track_changes_changed: false, revision_author: "Author".into(), markup: Markup::All, clock: || String::new(), local_clock: || String::new(), properties: Properties::default(), file_name: String::new(), stories_changed: false, style_edits: Default::default(), macros: vec![], recorder: None, macros_part: None, view_override: None, protection: None, mail_merge: Default::default(), sources: vec![], citation_style: Default::default(), sources_changed: false, sources_part: None, notes: vec![], note_specials: vec![], notes_changed: false, package: None, history: vec![], future: vec![], last_edit: None,
        }
    }

    // ---- text and positions -------------------------------------------------

    pub fn text(&self) -> String { self.paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>().join("\n") }
    pub fn len(&self) -> usize { self.paragraphs.iter().map(|p| p.len() + 1).sum::<usize>() - 1 }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
    pub fn paragraph_start(&self, index: usize) -> usize { self.paragraphs[..index].iter().map(|p| p.len() + 1).sum() }
    pub fn paragraph_starts(&self) -> Vec<usize> {
        let mut at = 0;
        self.paragraphs.iter().map(|p| { let s = at; at += p.len() + 1; s }).collect()
    }
    /// (paragraph index, offset within it).
    pub fn locate(&self, pos: usize) -> (usize, usize) {
        let mut at = 0;
        for (i, p) in self.paragraphs.iter().enumerate() {
            let len = p.len();
            if pos <= at + len { return (i, pos - at); }
            at += len + 1;
        }
        let last = self.paragraphs.len() - 1;
        (last, self.paragraphs[last].len())
    }
    pub fn range(&self) -> (usize, usize) { self.selection.range() }
    pub fn select(&mut self, anchor: usize, focus: usize) {
        let n = self.len();
        self.selection = Selection { anchor: anchor.min(n), focus: focus.min(n) };
        self.typing = None;
        self.last_edit = None;
    }
    pub fn select_all(&mut self) {
        let rec = self.rec_enter(|| "select_all();".to_string());
        let r = self.select_all__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn select_all__impl(&mut self) { let n = self.len(); self.select(0, n); }
    pub fn selected_text(&self) -> String {
        let (s, e) = self.range();
        self.text_between(s, e)
    }
    pub fn text_between(&self, start: usize, end: usize) -> String {
        let mut out = String::new();
        let mut at = 0;
        for (i, p) in self.paragraphs.iter().enumerate() {
            let len = p.len();
            if at + len >= start && at <= end {
                let a = start.saturating_sub(at).min(len);
                let b = end.saturating_sub(at).min(len);
                let t: String = p.text().chars().skip(a).take(b - a).filter(|c| *c != chars::NOTE).map(|c| match c { chars::OBJECT => ' ', chars::LINE_BREAK | chars::PAGE_BREAK | chars::COLUMN_BREAK => '\n', c => c }).collect();
                out.push_str(&t);
                if end > at + len && i + 1 < self.paragraphs.len() { out.push('\n'); }
            }
            at += len + 1;
            if at > end { break; }
        }
        out
    }
    /// The character at a position ('\n' between paragraphs).
    pub fn char_at(&self, pos: usize) -> Option<char> {
        let (i, o) = self.locate(pos);
        let p = &self.paragraphs[i];
        if o == p.len() { return if i + 1 < self.paragraphs.len() { Some('\n') } else { None }; }
        p.text().chars().nth(o)
    }

    // ---- resolved formatting ------------------------------------------------

    pub fn para_format(&self, p: &Paragraph) -> ParaFormat {
        let mut f = ParaFormat::default();
        let mut props = self.styles.para_props(p.style.as_deref());
        props.overlay(&p.props);
        f.apply(&props);
        // List level geometry supplies indents unless declared directly (listGeometry).
        if let Some((num, ilvl)) = f.numbering {
            if let Some(level) = self.numbering.level(num, ilvl) {
                if p.props.indent_left.is_none() { f.indent_left = level.indent_left; }
                if p.props.first_line.is_none() { f.first_line = -level.hanging; }
            }
        }
        f
    }
    pub fn run_format(&self, p: &Paragraph, r: &Run) -> RunFormat {
        let mut f = RunFormat::default();
        let mut props = self.styles.run_props(p.style.as_deref(), r.style.as_deref());
        props.overlay(&r.props);
        f.apply(&props);
        f
    }
    pub fn mark_format(&self, p: &Paragraph) -> RunFormat {
        let mut f = RunFormat::default();
        let mut props = self.styles.run_props(p.style.as_deref(), None);
        props.overlay(&p.mark);
        f.apply(&props);
        f
    }
    /// The formatting shown in the toolbar for the current selection/caret.
    pub fn current_format(&self) -> RunFormat {
        let (s, _) = self.range();
        let (i, o) = self.locate(s);
        let p = &self.paragraphs[i];
        let mut f = match p.run_at(if self.selection.is_caret() { o } else { o + 1 }) {
            Some(r) => self.run_format(p, r),
            None => self.mark_format(p),
        };
        if let Some(t) = &self.typing { f.apply(t); }
        f
    }
    /// The selection's character formatting as explicit properties (Format Painter's copy).
    pub fn current_run_props(&self) -> RunProps {
        let f = self.current_format();
        let highlight = HIGHLIGHTS.iter().find(|(_, c)| Some(*c) == f.highlight).map(|(n, _)| n.to_string());
        RunProps {
            font: Some(f.font), size: Some(f.size), bold: Some(f.bold), italic: Some(f.italic),
            underline: Some(if f.underline { "single" } else { "none" }.into()), strike: Some(f.strike), color: Some(f.color),
            highlight: Some(highlight), shading: Some(None), vert_align: Some(f.vert_align), caps: Some(f.caps), spacing: Some(f.spacing), vanish: Some(f.vanish),
            rtl: Some(f.rtl), outline: Some(f.outline), shadow: Some(f.shadow), glow: Some(f.glow), reflection: Some(f.reflection), gradient: Some(f.gradient), cs_font: f.cs_font, size_cs: Some(f.size_cs), bold_cs: Some(f.bold_cs), italic_cs: Some(f.italic_cs),
        }
    }
    pub fn current_paragraph(&self) -> &Paragraph { &self.paragraphs[self.locate(self.range().0).0] }
    pub fn current_para_format(&self) -> ParaFormat { self.para_format(self.current_paragraph()) }
    pub fn selected_paragraph_indices(&self) -> std::ops::RangeInclusive<usize> {
        let (s, e) = self.range();
        let first = self.locate(s).0;
        let (last, off) = self.locate(e);
        // A selection ending at the next paragraph's start does not format that paragraph.
        let last = if e > s && off == 0 && last > first { last - 1 } else { last };
        first..=last
    }

    // ---- history -------------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot { paragraphs: self.paragraphs.clone(), tables: self.tables.clone(), comments: self.comments.clone(), notes: self.notes.clone(), numbering: self.numbering.clone(), styles: self.styles.clone(), page: self.page.clone(), selection: self.selection, header: self.header.clone(), footer: self.footer.clone() }
    }
    fn restore(&mut self, s: Snapshot) {
        if self.comments != s.comments { self.comments_changed = true; }
        if self.notes != s.notes { self.notes_changed = true; }
        self.notes = s.notes;
        self.paragraphs = s.paragraphs; self.tables = s.tables; self.numbering = s.numbering; self.comments = s.comments;
        if self.header != s.header || self.footer != s.footer { self.stories_changed = true; }
        if self.styles != s.styles { self.styles_changed = true; }
        self.header = s.header; self.footer = s.footer;
        self.styles = s.styles; self.page = s.page; self.selection = s.selection;
        self.format_epoch += 1;
        self.changed();
    }
    /// Record an undo step. Consecutive typing (or deleting) coalesces into one step.
    pub fn checkpoint(&mut self, kind: EditKind) {
        if kind != EditKind::Other && self.last_edit == Some(kind) { return; }
        self.history.push(self.snapshot());
        if self.history.len() > 500 { self.history.remove(0); }
        self.future.clear();
        self.last_edit = Some(kind);
    }
    /// Break typing coalescing (e.g. at word boundaries or caret moves).
    pub fn seal(&mut self) { self.last_edit = None; }
    pub fn can_undo(&self) -> bool { !self.history.is_empty() }
    pub fn history_len(&self) -> usize { self.history.len() }
    pub fn can_redo(&self) -> bool { !self.future.is_empty() }
    pub fn undo(&mut self) -> bool {
        let rec = self.rec_enter(|| "undo();".to_string());
        let r = self.undo__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn undo__impl(&mut self) -> bool {
        let Some(s) = self.history.pop() else { return false };
        self.future.push(self.snapshot());
        self.restore(s);
        self.last_edit = None;
        true
    }
    pub fn redo(&mut self) -> bool {
        let rec = self.rec_enter(|| "redo();".to_string());
        let r = self.redo__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn redo__impl(&mut self) -> bool {
        let Some(s) = self.future.pop() else { return false };
        self.history.push(self.snapshot());
        self.restore(s);
        self.last_edit = None;
        true
    }
    pub(crate) fn changed(&mut self) {
        if self.tables.iter().any(|t| t.parent.is_some()) { self.prune_nested_tables(); }
        self.revision += 1;
        self.dirty = true;
    }

    /// Drop nested tables whose cell was deleted (with a row, column or the outer table), along
    /// with their paragraphs.
    fn prune_nested_tables(&mut self) {
        loop {
            let live: std::collections::HashSet<u64> = self.tables.iter().flat_map(|t| t.cells().map(|c| c.id)).collect();
            let Some(k) = self.tables.iter().position(|t| t.parent.is_some_and(|p| !live.contains(&p))) else { break };
            let cells: std::collections::HashSet<u64> = self.tables[k].cells().map(|c| c.id).collect();
            self.tables.remove(k);
            self.paragraphs.retain(|p| !p.cell.is_some_and(|c| cells.contains(&c)));
        }
        if self.paragraphs.is_empty() { self.paragraphs.push(Paragraph::new("")); }
        let n = self.len();
        if self.selection.anchor > n || self.selection.focus > n { self.selection = Selection { anchor: n, focus: n }; }
    }

    // ---- text editing ----------------------------------------------------------

    /// Delete [start, end). Paragraphs merge only within the same container
    /// (body or one table cell); cells always keep at least one paragraph.
    fn delete_range(&mut self, start: usize, end: usize) {
        if start >= end { return; }
        let (fi, fo) = self.locate(start);
        let (li, lo) = self.locate(end);
        if fi == li { self.paragraphs[fi].take(fo, lo); self.paragraphs[fi].normalize(); return; }
        let same_container = self.paragraphs[fi..=li].iter().all(|p| p.cell == self.paragraphs[fi].cell);
        if same_container {
            let len = self.paragraphs[fi].len();
            self.paragraphs[fi].take(fo, len);
            let mut last = self.paragraphs[li].clone();
            last.take(0, lo);
            self.paragraphs[fi].runs.extend(last.runs);
            self.paragraphs[fi].normalize();
            self.paragraphs.drain(fi + 1..=li);
            return;
        }
        // Tables the selection covers completely go entirely (with tables nested in them); the
        // rest is then deleted as text, joining what's left before and after.
        let starts = self.paragraph_starts();
        let mut removed: Vec<usize> = vec![];
        let mut gone_tables: Vec<usize> = vec![];
        for (ti, t) in self.tables.iter().enumerate() {
            if t.parent.is_some_and(|p| self.paragraphs[fi..=li].iter().any(|q| q.cell == Some(p))) { continue; }
            let Some(a) = (fi..=li).find(|&i| self.paragraphs[i].cell.is_some_and(|c| self.table_contains_cell(ti, c))) else { continue };
            let b = a + self.paragraphs[a..].iter().take_while(|p| p.cell.is_some_and(|c| self.table_contains_cell(ti, c))).count() - 1;
            let from_start = a > fi || fo == 0;
            let to_end = b < li || lo >= self.paragraphs[li].len();
            if b <= li && from_start && to_end { removed.extend(a..=b); gone_tables.push(ti); }
        }
        if !removed.is_empty() {
            removed.sort_unstable();
            removed.dedup();
            let shift: usize = removed.iter().map(|&i| { let (s0, s1) = (starts[i], starts[i] + self.paragraphs[i].len() + 1); s1.min(end).saturating_sub(s0.min(end)) }).sum();
            for &i in removed.iter().rev() { self.paragraphs.remove(i); }
            let ids: Vec<u64> = gone_tables.iter().map(|&t| self.tables[t].id).collect();
            self.tables.retain(|t| !ids.contains(&t.id));
            if self.paragraphs.is_empty() { self.paragraphs.push(Paragraph::new("")); }
            self.remove_orphaned_tables();
            let n = self.len();
            let new_end = end.saturating_sub(shift).min(n);
            if new_end > start.min(n) { self.delete_range(start.min(n), new_end); }
            return;
        }
        // Across containers: clear covered text, then drop fully covered paragraphs
        // whose container keeps another paragraph.
        let mut covered = vec![];
        for i in fi..=li {
            let len = self.paragraphs[i].len();
            let (a, b) = (if i == fi { fo } else { 0 }, if i == li { lo } else { len });
            self.paragraphs[i].take(a, b);
            self.paragraphs[i].normalize();
            if i != fi && i != li { covered.push(i); }
        }
        for &i in covered.iter().rev() {
            let cell = self.paragraphs[i].cell;
            let others = self.paragraphs.iter().enumerate().filter(|(j, p)| *j != i && p.cell == cell).count();
            if others > 0 { self.paragraphs.remove(i); }
        }
        // Tables whose every cell lost its paragraphs are gone; keep others intact.
        self.remove_orphaned_tables();
    }

    fn remove_orphaned_tables(&mut self) {
        let paragraphs = &self.paragraphs;
        self.tables.retain(|t| t.cells().any(|c| paragraphs.iter().any(|p| p.cell == Some(c.id))));
        // Fill cells that lost all paragraphs.
        for t in &self.tables {
            let mut insert_at = None;
            for c in t.cells() {
                if let Some(i) = self.paragraphs.iter().position(|p| p.cell == Some(c.id)) { insert_at = Some(i + 1); }
                else if let Some(at) = insert_at {
                    let mut p = Paragraph::default(); p.cell = Some(c.id);
                    self.paragraphs.insert(at, p);
                    insert_at = Some(at + 1);
                }
            }
        }
    }

    /// Insert runs (which may contain '\n') at a position, returning the end position.
    pub(crate) fn insert_runs(&mut self, pos: usize, runs: Vec<Run>) -> usize {
        let (i, o) = self.locate(pos);
        let mut lines: Vec<Vec<Run>> = vec![vec![]];
        let mut count = 0;
        for run in runs {
            let mut parts = run.text.split('\n').peekable();
            while let Some(part) = parts.next() {
                if !part.is_empty() { count += part.chars().count(); lines.last_mut().unwrap().push(run.with_text(part.to_string())); }
                if parts.peek().is_some() { lines.push(vec![]); count += 1; }
            }
        }
        let p = &mut self.paragraphs[i];
        let at = p.split_at(o);
        let tail: Vec<Run> = p.runs.drain(at..).collect();
        let template = p.sibling();
        let n = lines.len();
        let mut new_paragraphs = vec![];
        for (k, line) in lines.into_iter().enumerate() {
            if k == 0 { self.paragraphs[i].runs.extend(line); }
            else { let mut q = template.clone(); q.id = new_id(); q.runs = line; new_paragraphs.push(q); }
        }
        if n == 1 { self.paragraphs[i].runs.extend(tail); self.paragraphs[i].normalize(); }
        else {
            new_paragraphs.last_mut().unwrap().runs.extend(tail);
            for q in &mut new_paragraphs { q.normalize(); }
            self.paragraphs[i].normalize();
            let next_style = self.paragraphs[i].style.as_deref().and_then(|s| self.styles.get(s)).and_then(|s| s.next.clone());
            // A new paragraph after a heading takes the style's `next` style when split at the end.
            if let Some(next) = next_style {
                if new_paragraphs.len() == 1 && new_paragraphs[0].is_empty() { new_paragraphs[0].style = Some(next); new_paragraphs[0].props = ParaProps::default(); }
            }
            self.paragraphs.splice(i + 1..i + 1, new_paragraphs);
        }
        pos + count
    }

    /// The run used for newly typed text at a position (typingRun).
    fn typing_run(&self, pos: usize) -> Run {
        let (i, o) = self.locate(pos);
        let p = &self.paragraphs[i];
        let mut run = match p.run_at(o) {
            Some(r) if r.image.is_none() && r.field.is_none() && r.note.is_none() && r.object.is_none() => Run { text: String::new(), image: None, field: None, marker: None, revision: None, ..r.clone() },
            // Text typed after a note reference doesn't continue its superscript mark style.
            Some(r) if r.note.is_some() => Run { props: RunProps { vert_align: None, ..r.props.clone() }, ..Default::default() },
            Some(r) => Run { props: r.props.clone(), style: r.style.clone(), ..Default::default() },
            None => Run { props: p.mark.clone(), ..Default::default() },
        };
        // Typing at the end of a link does not extend it.
        if run.link.is_some() && p.link_ends_at(o) { run.link = None; }
        if let Some(t) = &self.typing { run.props.overlay(t); }
        run
    }

    /// Replace the selection with text (paragraph marks as '\n').
    pub fn replace(&mut self, text: &str) {
        let rec = self.rec_enter(|| crate::macros::text_line(text));
        let r = self.replace__impl(text);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn replace__impl(&mut self, text: &str) {
        let kind = if text.chars().count() == 1 && !text.contains('\n') { EditKind::Typing } else { EditKind::Other };
        if kind == EditKind::Typing && text.chars().all(char::is_whitespace) { self.seal(); }
        self.checkpoint(kind);
        let (s, e) = self.range();
        if self.track_changes { return self.tracked_replace(s, e, text); }
        let mut run = self.typing_run(s);
        self.delete_range(s, e);
        run.text = text.to_string();
        let end = self.insert_runs(s, vec![run]);
        let typing = self.typing.take();
        self.selection = Selection { anchor: end, focus: end };
        if text.is_empty() { self.typing = typing; }
        self.changed();
    }

    pub fn insert_paragraph(&mut self) {
        let rec = self.rec_enter(|| "new_paragraph();".to_string());
        let r = self.insert_paragraph__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn insert_paragraph__impl(&mut self) {
        // Formatting picked at the caret (a font chosen before typing) carries into the new paragraph.
        // So does the formatting of the text before the caret (a font applied to a selection):
        // the new paragraph continues it, as in Word.
        let typing = self.typing.clone();
        let carried = { let (s, _) = self.range(); let mut p = self.typing_run(s).props; p.vert_align = None; p };
        self.seal(); self.replace("\n"); self.seal();
        let (i, _) = self.locate(self.selection.focus);
        if self.paragraphs[i].is_empty() {
            self.paragraphs[i].mark.overlay(&carried);
            self.changed();
        }
        if let Some(t) = typing { self.typing = Some(t); }
    }
    pub fn insert_line_break(&mut self) {
        let rec = self.rec_enter(|| "line_break();".to_string());
        let r = self.insert_line_break__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn insert_line_break__impl(&mut self) { self.replace(&chars::LINE_BREAK.to_string()); }
    pub fn insert_page_break(&mut self) {
        let rec = self.rec_enter(|| "page_break();".to_string());
        let r = self.insert_page_break__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn insert_page_break__impl(&mut self) { self.replace(&chars::PAGE_BREAK.to_string()); }
    pub fn insert_tab(&mut self) {
        let rec = self.rec_enter(|| "tab();".to_string());
        let r = self.insert_tab__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn insert_tab__impl(&mut self) { self.replace("\t"); }

    /// Backspace (`forward == false`) or Delete. `word` deletes to a word boundary.
    pub fn delete(&mut self, forward: bool, word: bool) {
        let rec = self.rec_enter(|| format!("{}();", match (forward, word) { (false, false) => "delete_back", (true, false) => "delete_forward", (false, true) => "delete_word_back", (true, true) => "delete_word_forward" }));
        let r = self.delete__impl(forward, word);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn delete__impl(&mut self, forward: bool, word: bool) {
        let (s, e) = self.range();
        let (s, e) = if s != e { (s, e) } else if forward {
            if s >= self.len() { return; }
            (s, if word { self.word_boundary(s, true) } else { self.next_grapheme(s, true) })
        } else {
            if s == 0 {
                // Backspace at the document start removes list formatting / indentation first.
                self.backspace_paragraph_start();
                return;
            }
            let (_i, o) = self.locate(s);
            if o == 0 && !word {
                if self.backspace_paragraph_start() { return; }
            }
            (if word { self.word_boundary(s, false) } else { self.backspace_target(s) }, s)
        };
        // Never merge a paragraph into a different table cell or across a table edge.
        let (fi, _) = self.locate(s);
        let (li, _) = self.locate(e);
        if fi != li && e - s == 1 && self.paragraphs[fi].cell != self.paragraphs[li].cell {
            self.selection = Selection { anchor: if forward { e } else { s }, focus: if forward { e } else { s } };
            return;
        }
        self.checkpoint(EditKind::Deleting);
        if self.track_changes {
            // Deleted text stays in place: Backspace leaves the caret before it, Delete after it.
            let end = self.tracked_delete(s, e);
            let at = if forward { end } else { s };
            self.selection = Selection { anchor: at, focus: at };
            self.changed();
            return;
        }
        self.delete_range(s, e);
        self.selection = Selection { anchor: s, focus: s };
        self.changed();
    }

    // ---- tracked changes (revisions.js) ------------------------------------------------

    pub fn new_revision(&self, kind: RevKind) -> Revision { Revision { kind, author: self.revision_author.clone(), date: (self.clock)() } }

    pub fn set_track_changes(&mut self, on: bool) {
        if self.track_changes != on { self.track_changes = on; self.track_changes_changed = true; self.dirty = true; }
    }
    pub fn set_markup(&mut self, markup: Markup) {
        if self.markup != markup { self.markup = markup; self.format_epoch += 1; self.revision += 1; }
    }

    /// Mark [s, e) deleted. The author's own insertions are removed outright. Returns the end of
    /// the range after those removals.
    fn tracked_delete(&mut self, s: usize, e: usize) -> usize {
        if s >= e { return e; }
        let mine = self.revision_author.clone();
        let del = self.new_revision(RevKind::Del);
        let (fi, _) = self.locate(s);
        let (li, _) = self.locate(e);
        let starts = self.paragraph_starts();
        let mut removed = 0;
        for i in (fi..=li).rev() {
            let ps = starts[i];
            let len = self.paragraphs[i].len();
            let (a, b) = (s.saturating_sub(ps).min(len), e.saturating_sub(ps).min(len));
            // The paragraph mark lies in the range when the range continues past it.
            if i < li && e > ps + len && self.paragraphs.get(i + 1).is_some_and(|n| n.cell == self.paragraphs[i].cell) {
                let own = self.paragraphs[i].mark_revision.as_ref().is_some_and(|r| r.kind == RevKind::Ins && r.author == mine);
                if own {
                    let next = self.paragraphs.remove(i + 1);
                    self.paragraphs[i].runs.extend(next.runs);
                    self.paragraphs[i].mark_revision = next.mark_revision;
                    removed += 1;
                } else if self.paragraphs[i].mark_revision.is_none() {
                    self.paragraphs[i].mark_revision = Some(del.clone());
                }
            }
            let p = &mut self.paragraphs[i];
            if a < b {
                p.split_at(b);
                let ia = p.split_at(a);
                let ib = p.split_at(b);
                let mut k = ia;
                let mut end = ib;
                while k < end {
                    let r = &mut p.runs[k];
                    match &r.revision {
                        Some(rv) if rv.kind == RevKind::Ins && rv.author == mine => { removed += r.len(); p.runs.remove(k); end -= 1; continue; }
                        Some(rv) if rv.kind == RevKind::Del => {}
                        _ if r.marker.is_some() => {}
                        _ => r.revision = Some(del.clone()),
                    }
                    k += 1;
                }
            }
            p.normalize();
        }
        e - removed
    }

    /// Tracked replacement: the old text is marked deleted and the new text inserted after it.
    fn tracked_replace(&mut self, s: usize, e: usize, text: &str) {
        let mut run = self.typing_run(s);
        let at = self.tracked_delete(s, e);
        if text.is_empty() { self.selection = Selection { anchor: s, focus: s }; self.changed(); return; }
        run.revision = Some(self.new_revision(RevKind::Ins));
        run.text = text.to_string();
        let (first, _) = self.locate(at);
        let end = self.insert_runs(at, vec![run]);
        // New paragraph marks created by the insertion are tracked too.
        let breaks = text.matches('\n').count();
        for i in first..first + breaks { self.paragraphs[i].mark_revision = Some(self.new_revision(RevKind::Ins)); }
        self.typing = None;
        self.selection = Selection { anchor: end, focus: end };
        self.changed();
    }

    /// Tracked changes as (start, end, kind, author), in document order. A mark's range covers
    /// its paragraph separator.
    pub fn revisions(&self) -> Vec<(usize, usize, RevKind, String)> {
        let mut out = vec![];
        let mut at = 0;
        for p in &self.paragraphs {
            for r in &p.runs {
                if let Some(rv) = &r.revision {
                    let len = r.len();
                    match out.last_mut() {
                        Some((_, e, k, a)) if *e == at && *k == rv.kind && *a == rv.author => *e += len,
                        _ => out.push((at, at + len, rv.kind, rv.author.clone())),
                    }
                }
                at += r.len();
            }
            if let Some(rv) = &p.mark_revision { out.push((at, at + 1, rv.kind, rv.author.clone())); }
            at += 1;
        }
        out
    }

    /// Accept or reject every change intersecting [s, e] (all changes when `all`).
    pub fn resolve_revisions(&mut self, accept: bool, s: usize, e: usize, all: bool) -> usize {
        let hits = |a: usize, b: usize| all || (a < e.max(s + 1) && b > s) || (s == e && a <= s && s <= b);
        let starts = self.paragraph_starts();
        let mut touched = 0;
        let mut changed = false;
        let snapshot = self.snapshot();
        for i in (0..self.paragraphs.len()).rev() {
            let ps = starts[i];
            let len = self.paragraphs[i].len();
            if let Some(rv) = self.paragraphs[i].mark_revision.clone() {
                if hits(ps + len, ps + len + 1) {
                    touched += 1; changed = true;
                    let merge = (rv.kind == RevKind::Ins) != accept;
                    self.paragraphs[i].mark_revision = None;
                    if merge && i + 1 < self.paragraphs.len() && self.paragraphs[i + 1].cell == self.paragraphs[i].cell {
                        let next = self.paragraphs.remove(i + 1);
                        self.paragraphs[i].runs.extend(next.runs);
                        self.paragraphs[i].mark_revision = next.mark_revision;
                    }
                }
            }
            let p = &mut self.paragraphs[i];
            let mut at = ps;
            let mut k = 0;
            while k < p.runs.len() {
                let len = p.runs[k].len();
                if let Some(rv) = p.runs[k].revision.clone() {
                    if hits(at, at + len) {
                        touched += 1; changed = true;
                        if (rv.kind == RevKind::Ins) == accept { p.runs[k].revision = None; } else { p.runs.remove(k); at += len; continue; }
                    }
                }
                at += len;
                k += 1;
            }
            p.normalize();
        }
        if changed {
            self.history.push(snapshot);
            self.future.clear();
            let n = self.len();
            self.selection = Selection { anchor: s.min(n), focus: s.min(n) };
            self.changed();
        }
        touched
    }

    /// The next/previous change after/before the selection.
    pub fn next_revision(&self, forward: bool) -> Option<(usize, usize)> {
        let (s, e) = self.range();
        let all = self.revisions();
        let found = if forward { all.iter().find(|r| r.0 >= e && (r.0, r.1) != (s, e)).or(all.first()) } else { all.iter().rev().find(|r| r.1 <= s && (r.0, r.1) != (s, e)).or(all.last()) };
        found.map(|r| (r.0, r.1))
    }

    /// At a paragraph start, Backspace first ends a list (as in Word). Returns whether it acted.
    fn backspace_paragraph_start(&mut self) -> bool {
        let (i, o) = self.locate(self.range().0);
        if o != 0 { return false; }
        if self.para_format(&self.paragraphs[i]).numbering.is_some() {
            self.checkpoint(EditKind::Other);
            self.paragraphs[i].props.numbering = Some((0, 0));
            self.paragraphs[i].props.indent_left = None;
            self.paragraphs[i].props.first_line = None;
            self.changed();
            return true;
        }
        false
    }

    // ---- navigation -----------------------------------------------------------

    pub fn word_boundary(&self, pos: usize, forward: bool) -> usize {
        let text: Vec<char> = self.text().chars().collect();
        let class = |c: char| if c.is_alphanumeric() || c == '_' { 2 } else if c.is_whitespace() { 0 } else { 1 };
        let mut p = pos;
        if forward {
            if p < text.len() && text[p] == '\n' { return p + 1; }
            let start = text.get(p).map(|&c| class(c));
            while p < text.len() && Some(class(text[p])) == start && text[p] != '\n' { p += 1; }
            while p < text.len() && class(text[p]) == 0 && text[p] != '\n' { p += 1; }
        } else {
            if p > 0 && text[p - 1] == '\n' { return p - 1; }
            while p > 0 && class(text[p - 1]) == 0 && text[p - 1] != '\n' { p -= 1; }
            let c = if p > 0 { Some(class(text[p - 1])) } else { None };
            while p > 0 && Some(class(text[p - 1])) == c && text[p - 1] != '\n' { p -= 1; }
        }
        p
    }
    pub fn word_at(&self, pos: usize) -> (usize, usize) {
        let text: Vec<char> = self.text().chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
        let (mut a, mut b) = (pos, pos);
        if pos < text.len() && is_word(text[pos]) || pos > 0 && text.get(pos - 1).copied().is_some_and(is_word) {
            while a > 0 && is_word(text[a - 1]) { a -= 1; }
            while b < text.len() && is_word(text[b]) { b += 1; }
        } else if pos < text.len() && text[pos] != '\n' { b = pos + 1; }
        (a, b)
    }
    pub fn paragraph_bounds(&self, pos: usize) -> (usize, usize) {
        let (i, _) = self.locate(pos);
        let s = self.paragraph_start(i);
        (s, s + self.paragraphs[i].len())
    }

    // ---- run formatting ---------------------------------------------------------

    /// Apply run properties to the selection, or to future typing at a caret (setRunFormat).
    pub fn set_run_props(&mut self, patch: RunProps) {
        let rec = self.rec_enter(|| crate::macros::run_props_line(&patch));
        let r = self.set_run_props__impl(patch);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn set_run_props__impl(&mut self, patch: RunProps) {
        let (s, e) = self.range();
        if s == e {
            let (i, _) = self.locate(s);
            if self.paragraphs[i].is_empty() {
                // An empty paragraph's mark carries its formatting.
                self.checkpoint(EditKind::Other);
                self.paragraphs[i].mark.overlay(&patch);
                self.changed();
            }
            let mut t = self.typing.take().unwrap_or_default();
            t.overlay(&patch);
            self.typing = Some(t);
            return;
        }
        self.checkpoint(EditKind::Other);
        let starts = self.paragraph_starts();
        for i in self.selected_paragraph_indices() {
            let ps = starts[i];
            let len = self.paragraphs[i].len();
            let a = s.saturating_sub(ps).min(len);
            let b = e.saturating_sub(ps).min(len);
            let p = &mut self.paragraphs[i];
            if a < b {
                let ia = p.split_at(a);
                let ib = p.split_at(b);
                for r in &mut p.runs[ia..ib] {
                    r.props.overlay(&patch);
                    // A direct value equal to the inherited one is dropped, so runs merge again.
                    let inherited = self.styles.run_props(p.style.as_deref(), r.style.as_deref());
                    drop_inherited(&mut r.props, &inherited);
                }
                p.normalize();
            }
            if e > ps + len { p.mark.overlay(&patch); }
        }
        self.changed();
    }

    /// Toggle a boolean property based on the current state (toggleBold etc.).
    /// Bold/italic apply to Latin and complex-script text alike (b and bCs), as Word's buttons do.
    pub fn toggle_bold(&mut self) {
        let rec = self.rec_enter(|| "bold();".to_string());
        let r = self.toggle_bold__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn toggle_bold__impl(&mut self) { let v = !self.current_format().bold; self.set_run_props(RunProps { bold: Some(v), bold_cs: Some(v), ..Default::default() }); }
    pub fn toggle_italic(&mut self) {
        let rec = self.rec_enter(|| "italic();".to_string());
        let r = self.toggle_italic__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn toggle_italic__impl(&mut self) { let v = !self.current_format().italic; self.set_run_props(RunProps { italic: Some(v), italic_cs: Some(v), ..Default::default() }); }
    pub fn toggle_underline(&mut self) {
        let rec = self.rec_enter(|| "underline();".to_string());
        let r = self.toggle_underline__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn toggle_underline__impl(&mut self) { let v = !self.current_format().underline; self.set_run_props(RunProps { underline: Some(if v { "single" } else { "none" }.into()), ..Default::default() }); }
    pub fn toggle_strike(&mut self) {
        let rec = self.rec_enter(|| "strikethrough();".to_string());
        let r = self.toggle_strike__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn toggle_strike__impl(&mut self) { let v = self.current_format().strike == Strike::None; self.set_run_props(RunProps { strike: Some(if v { Strike::Single } else { Strike::None }), ..Default::default() }); }
    pub fn toggle_vert_align(&mut self, va: VertAlign) {
        let v = if self.current_format().vert_align == va { VertAlign::Baseline } else { va };
        self.set_run_props(RunProps { vert_align: Some(v), ..Default::default() });
    }
    pub fn set_font_size(&mut self, size: f32) {
        let rec = self.rec_enter(|| format!("font_size({size});"));
        let r = self.set_font_size__impl(size);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn set_font_size__impl(&mut self, size: f32) {
        // Half-point steps from 1 to 1638 pt (font-size.js).
        let size = ((size * 2.0).round() / 2.0).clamp(1.0, 1638.0);
        self.set_run_props(RunProps { size: Some(size), size_cs: Some(size), ..Default::default() });
    }
    pub fn grow_font(&mut self, grow: bool) {
        let rec = self.rec_enter(|| if grow { "grow_font();".to_string() } else { "shrink_font();".to_string() });
        let r = self.grow_font__impl(grow);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn grow_font__impl(&mut self, grow: bool) {
        const SIZES: [f32; 16] = [8., 9., 10., 11., 12., 14., 16., 18., 20., 22., 24., 26., 28., 36., 48., 72.];
        let cur = self.current_format().size;
        let next = if grow { SIZES.iter().copied().find(|&s| s > cur).unwrap_or(cur + 10.0) } else { SIZES.iter().rev().copied().find(|&s| s < cur).unwrap_or((cur - 1.0).max(1.0)) };
        self.set_font_size(next);
    }

    /// Remove direct run formatting from the selection (clearDirectTextFormatting).
    pub fn clear_run_formatting(&mut self) {
        let rec = self.rec_enter(|| "clear_formatting();".to_string());
        let r = self.clear_run_formatting__impl();
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn clear_run_formatting__impl(&mut self) {
        let (s, e) = self.range();
        if s == e { self.typing = Some(RunProps { font: None, ..clear_all_props() }); return; }
        self.checkpoint(EditKind::Other);
        let starts = self.paragraph_starts();
        for i in self.selected_paragraph_indices() {
            let ps = starts[i];
            let len = self.paragraphs[i].len();
            let (a, b) = (s.saturating_sub(ps).min(len), e.saturating_sub(ps).min(len));
            let p = &mut self.paragraphs[i];
            let ia = p.split_at(a);
            let ib = p.split_at(b);
            for r in &mut p.runs[ia..ib] { r.props = RunProps::default(); r.style = None; r.extra.clear(); }
            p.normalize();
            if e > ps + len { p.mark = RunProps::default(); }
        }
        self.changed();
    }

    pub fn set_character_style(&mut self, id: Option<String>) {
        let (s, e) = self.range();
        if s == e { return; }
        self.checkpoint(EditKind::Other);
        let starts = self.paragraph_starts();
        for i in self.selected_paragraph_indices() {
            let ps = starts[i];
            let len = self.paragraphs[i].len();
            let (a, b) = (s.saturating_sub(ps).min(len), e.saturating_sub(ps).min(len));
            let p = &mut self.paragraphs[i];
            let ia = p.split_at(a);
            let ib = p.split_at(b);
            for r in &mut p.runs[ia..ib] { r.style = id.clone(); }
            p.normalize();
        }
        self.changed();
    }

    pub fn set_link(&mut self, target: Option<String>) {
        let (s, e) = self.range();
        if s == e { return; }
        self.checkpoint(EditKind::Other);
        let starts = self.paragraph_starts();
        for i in self.selected_paragraph_indices() {
            let ps = starts[i];
            let len = self.paragraphs[i].len();
            let (a, b) = (s.saturating_sub(ps).min(len), e.saturating_sub(ps).min(len));
            let p = &mut self.paragraphs[i];
            let ia = p.split_at(a);
            let ib = p.split_at(b);
            for r in &mut p.runs[ia..ib] {
                r.link = target.clone();
                if target.is_some() && r.style.is_none() { r.props.color = Some(Some(Color(0x05, 0x63, 0xC1))); r.props.underline = Some("single".into()); }
                if target.is_none() { r.props.color = None; r.props.underline = None; }
            }
            p.normalize();
        }
        self.changed();
    }
    pub fn link_at(&self, pos: usize) -> Option<String> {
        let (i, o) = self.locate(pos);
        let mut at = 0;
        for r in &self.paragraphs[i].runs {
            let len = r.len();
            if o >= at && o < at + len { return r.link.clone(); }
            at += len;
        }
        None
    }

    // ---- paragraph formatting ------------------------------------------------------

    pub fn set_para_props(&mut self, patch: ParaProps) {
        self.checkpoint(EditKind::Other);
        for i in self.selected_paragraph_indices() { self.paragraphs[i].props.overlay(&patch); }
        self.changed();
    }
    /// Whether a paragraph runs right to left: its w:bidi, else its first strong character.
    pub fn paragraph_rtl(&self, p: &Paragraph) -> bool {
        self.para_format(p).rtl.unwrap_or_else(|| unicode_bidi::get_base_direction(p.text().as_str()) == unicode_bidi::Direction::Rtl)
    }
    /// Set the selected paragraphs' direction. The text keeps its start alignment, so it moves
    /// to the other side, as Word's Right-to-Left button does.
    pub fn set_paragraph_direction(&mut self, rtl: bool) {
        self.set_para_props(ParaProps { rtl: Some(rtl), ..Default::default() });
        // New text typed in these paragraphs is marked to match.
        let mut t = self.typing.take().unwrap_or_default();
        t.rtl = Some(rtl);
        self.typing = Some(t);
    }
    /// Alignment as it appears on the page: left/right swap in right-to-left paragraphs, where
    /// the stored values mean start/end.
    pub fn visual_alignment(&self) -> Alignment {
        let p = self.current_paragraph();
        let a = self.para_format(p).alignment;
        if self.paragraph_rtl(p) { mirror(a) } else { a }
    }
    /// Apply an alignment chosen by its visual side (the Align Left/Right buttons).
    pub fn set_visual_alignment(&mut self, a: Alignment) {
        let rtl = self.paragraph_rtl(self.current_paragraph());
        self.set_alignment(if rtl { mirror(a) } else { a });
    }

    // ---- grapheme clusters (UTF-8 text) ------------------------------------------------------

    /// The next/previous grapheme cluster boundary, so the caret never lands inside a combining
    /// sequence or an emoji ZWJ sequence.
    pub fn next_grapheme(&self, pos: usize, forward: bool) -> usize {
        use unicode_segmentation::UnicodeSegmentation;
        let (i, o) = self.locate(pos);
        let text = self.paragraphs[i].text();
        if forward && o >= text.chars().count() { return (pos + 1).min(self.len()); }
        if !forward && o == 0 { return pos.saturating_sub(1); }
        // Cluster boundaries as character offsets: 0, then the end of each grapheme.
        let bounds: Vec<usize> = std::iter::once(0).chain(text.graphemes(true).scan(0, |acc, g| { *acc += g.chars().count(); Some(*acc) })).collect();
        let target = if forward { bounds.iter().copied().find(|&b| b > o) } else { bounds.iter().rev().copied().find(|&b| b < o) };
        pos - o + target.unwrap_or(o)
    }
    /// Backspace removes a whole emoji or accented cluster, but only the last mark of an Arabic
    /// or Hebrew letter with diacritics, so a vowel sign can be corrected on its own.
    pub fn backspace_target(&self, pos: usize) -> usize {
        let prev = self.next_grapheme(pos, false);
        let (i, o) = self.locate(pos);
        let chars: Vec<char> = self.paragraphs[i].text().chars().collect();
        let last = o.checked_sub(1).and_then(|k| chars.get(k)).copied();
        if pos - prev > 1 && last.is_some_and(|c| crate::fonts::is_complex(c)) { pos - 1 } else { prev }
    }

    pub fn set_alignment(&mut self, a: Alignment) {
        let rec = self.rec_enter(|| format!("align(\"{}\");", match a { Alignment::Left => "left", Alignment::Center => "center", Alignment::Right => "right", Alignment::Justify => "justify" }));
        let r = self.set_alignment__impl(a);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn set_alignment__impl(&mut self, a: Alignment) { self.set_para_props(ParaProps { alignment: Some(a), ..Default::default() }); }
    pub fn set_line_spacing(&mut self, rule: LineRule, value: f32) {
        self.set_para_props(ParaProps { line_rule: Some(rule), line_spacing: Some(value), ..Default::default() });
    }
    pub fn set_paragraph_spacing(&mut self, before: Option<f32>, after: Option<f32>) {
        self.set_para_props(ParaProps { space_before: before, space_after: after, ..Default::default() });
    }
    /// Increase/decrease the left indent by one default tab (adjustIndent), or change list level.
    pub fn adjust_indent(&mut self, increase: bool) {
        if self.current_para_format().numbering.is_some() { return self.change_list_level(if increase { 1 } else { -1 }); }
        self.checkpoint(EditKind::Other);
        let step = self.page.default_tab_stop;
        for i in self.selected_paragraph_indices() {
            let f = self.para_format(&self.paragraphs[i]);
            let next = if increase { (f.indent_left / step + 1) * step } else { ((f.indent_left + step - 1) / step - 1).max(0) * step };
            self.paragraphs[i].props.indent_left = Some(next.clamp(0, 31680));
        }
        self.changed();
    }
    pub fn clear_paragraph_formatting(&mut self) {
        self.checkpoint(EditKind::Other);
        for i in self.selected_paragraph_indices() { self.paragraphs[i].props = ParaProps::default(); }
        self.changed();
    }
    pub fn set_paragraph_style(&mut self, id: &str) {
        let rec = self.rec_enter(|| format!("style({id:?});"));
        let r = self.set_paragraph_style__impl(id);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn set_paragraph_style__impl(&mut self, id: &str) {
        self.checkpoint(EditKind::Other);
        let id = if self.styles.get(id).is_some() { Some(id.to_string()) } else { None };
        for i in self.selected_paragraph_indices() {
            self.paragraphs[i].style = id.clone();
            // A style change resets direct paragraph formatting that would hide the style (setParagraphStyle).
            let num = self.paragraphs[i].props.numbering;
            self.paragraphs[i].props = ParaProps { numbering: num, ..Default::default() };
        }
        self.changed();
    }
    pub fn paragraph_style(&self) -> String {
        let p = self.current_paragraph();
        self.styles.paragraph_style_id(p.style.as_deref()).unwrap_or("Normal").to_string()
    }

    // ---- lists -------------------------------------------------------------------------

    /// Toggle bullets/numbering on the selected paragraphs (toggleList).
    pub fn toggle_list(&mut self, kind: ListKind) {
        let rec = self.rec_enter(|| if kind == ListKind::Bullet { "bullets();".to_string() } else { "numbering();".to_string() });
        let r = self.toggle_list__impl(kind);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn toggle_list__impl(&mut self, kind: ListKind) {
        let current = self.current_para_format().numbering.and_then(|(n, _)| self.numbering.kind(n));
        self.checkpoint(EditKind::Other);
        let indices: Vec<usize> = self.selected_paragraph_indices().collect();
        if current == Some(kind) {
            for i in indices { let p = &mut self.paragraphs[i]; p.props.numbering = Some((0, 0)); p.props.indent_left = None; p.props.first_line = None; }
        } else {
            // Continue an adjacent list of the same kind, otherwise start a new one.
            let first = *indices.first().unwrap();
            let neighbor = [first.checked_sub(1), indices.last().map(|l| l + 1)].into_iter().flatten()
                .filter_map(|j| self.paragraphs.get(j)).filter_map(|p| self.para_format(p).numbering)
                .find(|(n, _)| self.numbering.kind(*n) == Some(kind)).map(|(n, _)| n);
            let script = self.list_script(first);
            let num = neighbor.unwrap_or_else(|| self.numbering.new_list_for(kind, script));
            for i in indices { let p = &mut self.paragraphs[i]; p.props.numbering = Some((num, 0)); p.props.indent_left = None; p.props.first_line = None; }
        }
        self.changed();
    }
    /// Which letters a new list in this paragraph uses: Arabic or Hebrew in right-to-left text.
    pub fn list_script(&self, index: usize) -> crate::numbering::Script {
        use crate::numbering::Script;
        let p = &self.paragraphs[index];
        if !self.paragraph_rtl(p) { return Script::Latin; }
        if p.text().chars().any(|c| matches!(c as u32, 0x0590..=0x05FF)) { Script::Hebrew } else { Script::Arabic }
    }
    pub fn change_list_level(&mut self, delta: i32) {
        self.checkpoint(EditKind::Other);
        for i in self.selected_paragraph_indices() {
            if let Some((n, l)) = self.para_format(&self.paragraphs[i]).numbering {
                self.paragraphs[i].props.numbering = Some((n, (l as i32 + delta).clamp(0, 8) as u8));
                self.paragraphs[i].props.indent_left = None; self.paragraphs[i].props.first_line = None;
            }
        }
        self.changed();
    }
    pub fn restart_numbering(&mut self, start: u32) {
        let Some((n, l)) = self.current_para_format().numbering else { return };
        self.checkpoint(EditKind::Other);
        let Some(new_num) = self.numbering.restart(n, l, start) else { return };
        let first = self.locate(self.range().0).0;
        for i in first..self.paragraphs.len() {
            let f = self.para_format(&self.paragraphs[i]);
            match f.numbering { Some((m, _)) if m == n => { let lvl = f.numbering.unwrap().1; self.paragraphs[i].props.numbering = Some((new_num, lvl)); } Some(_) => break, None if self.paragraphs[i].is_empty() => continue, None => break }
        }
        self.changed();
    }

    // ---- page setup -------------------------------------------------------------------------

    pub fn set_page_settings(&mut self, settings: PageSettings) -> Result<(), String> {
        settings.validate()?;
        self.checkpoint(EditKind::Other);
        self.page = settings;
        self.changed();
        Ok(())
    }

    // ---- tables ----------------------------------------------------------------------------------

    pub fn table_of_cell(&self, cell: u64) -> Option<usize> { self.tables.iter().position(|t| t.has_cell(cell)) }
    pub fn current_cell(&self) -> Option<u64> { self.current_paragraph().cell }

    /// Insert a rows×columns table after the current paragraph (insertTable).
    pub fn insert_table(&mut self, rows: usize, cols: usize) {
        let rec = self.rec_enter(|| format!("insert_table({rows}, {cols});"));
        let r = self.insert_table__impl(rows, cols);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn insert_table__impl(&mut self, rows: usize, cols: usize) {
        if rows == 0 || cols == 0 { return; }
        self.checkpoint(EditKind::Other);
        let (i, _) = self.locate(self.range().1);
        let container = self.paragraphs[i].cell;
        // Inside a cell the new table fills the cell (less its margins); otherwise the page width.
        let width = match container.and_then(|c| self.table_of_cell(c).and_then(|t| self.tables[t].cells().find(|x| x.id == c).map(|x| (t, x.width)))) {
            Some((t, w)) => { let m = self.tables[t].cell_margins; (w - m[1] - m[3]).max(360 * cols as i32) }
            None => self.page.width - self.page.left - self.page.right - self.page.gutter,
        };
        let col = width / cols as i32;
        let table = Table {
            id: new_id(), grid: vec![col; cols],
            rows: (0..rows).map(|_| Row { cells: (0..cols).map(|_| Cell::new(col)).collect(), min_height: 0, exact: false, header: false, cant_split: false, grid_before: 0, grid_after: 0, extra: vec![] }).collect(),
            borders: true, style: self.styles.get("TableGrid").map(|_| "TableGrid".to_string()), indent: 0, alignment: Alignment::Left, cell_margins: DEFAULT_CELL_MARGINS, raw_props: None, parent: container, fixed: false,
        };
        let mut new = vec![];
        for c in table.cells() {
            let mut p = Paragraph::default();
            p.cell = Some(c.id);
            p.props.space_after = Some(0.0);
            new.push(p);
        }
        let first_cell = new[0].clone();
        // Tables need a following paragraph in the same container.
        let mut after = self.paragraphs[i].sibling();
        after.cell = container;
        // The paragraph after a table is plain text, not another list item.
        after.props.numbering = if self.para_format(&self.paragraphs[i]).numbering.is_some() { Some((0, 0)) } else { after.props.numbering };
        after.props.indent_left = None;
        after.props.first_line = None;
        self.paragraphs.splice(i + 1..i + 1, new.into_iter().chain(std::iter::once(after)));
        self.tables.push(table);
        let pos = self.paragraph_start(self.paragraphs.iter().position(|p| p.id == first_cell.id).unwrap());
        self.selection = Selection { anchor: pos, focus: pos };
        self.sort_tables();
        self.changed();
    }

    /// The outermost table containing a cell (nested tables belong to their outer table).
    pub fn root_table_of_cell(&self, cell: u64) -> Option<usize> {
        let mut ti = self.table_of_cell(cell)?;
        let mut guard = 0;
        while let Some(pc) = self.tables[ti].parent {
            match self.table_of_cell(pc) { Some(t) if guard < 32 => { ti = t; guard += 1; } _ => break }
        }
        Some(ti)
    }

    /// Is the cell in table `ti` or in a table nested inside it?
    pub fn table_contains_cell(&self, ti: usize, cell: u64) -> bool {
        let mut cur = self.table_of_cell(cell);
        let mut guard = 0;
        while let Some(t) = cur {
            if t == ti { return true; }
            if guard > 32 { return false; }
            guard += 1;
            cur = self.tables[t].parent.and_then(|pc| self.table_of_cell(pc));
        }
        false
    }

    /// The table directly inside `cell` that contains `inner` (a cell at any depth below it).
    pub fn child_table_in_cell(&self, cell: u64, inner: u64) -> Option<usize> {
        let mut cur = self.table_of_cell(inner)?;
        for _ in 0..32 {
            match self.tables[cur].parent { Some(pc) if pc == cell => return Some(cur), Some(pc) => cur = self.table_of_cell(pc)?, None => return None }
        }
        None
    }

    pub(crate) fn sort_tables(&mut self) {
        let order: HashMap<u64, usize> = self.paragraphs.iter().enumerate().filter_map(|(i, p)| p.cell.map(|c| (c, i))).rev().collect();
        self.tables.sort_by_key(|t| t.cells().filter_map(|c| order.get(&c.id)).min().copied().unwrap_or(usize::MAX));
    }

    pub(crate) fn cell_paragraphs(&self, cell: u64) -> std::ops::Range<usize> {
        let first = self.paragraphs.iter().position(|p| p.cell == Some(cell)).unwrap_or(0);
        let count = self.paragraphs[first..].iter().take_while(|p| p.cell == Some(cell)).count();
        first..first + count
    }

    /// Insert a row below (or above) the current row (insertTableRow).
    pub fn insert_table_row(&mut self, below: bool) {
        let Some(cell) = self.current_cell() else { return };
        let Some(ti) = self.table_of_cell(cell) else { return };
        self.checkpoint(EditKind::Other);
        let (r, _) = self.tables[ti].find_cell(cell).unwrap();
        let template = self.tables[ti].rows[r].clone();
        let new_row = Row { cells: template.cells.iter().map(|c| Cell { id: new_id(), v_merge: if c.v_merge == VMerge::None { VMerge::None } else { VMerge::Continue }, ..c.clone() }).collect(), header: false, ..template.clone() };
        let anchor_row = &self.tables[ti].rows[r];
        let anchor_cell = if below { anchor_row.cells.last().unwrap().id } else { anchor_row.cells[0].id };
        let range = self.cell_paragraphs(anchor_cell);
        let at = if below { range.end } else { range.start };
        let template_p = self.paragraphs[range.start].sibling();
        let new_paragraphs: Vec<Paragraph> = new_row.cells.iter().map(|c| { let mut p = template_p.clone(); p.id = new_id(); p.cell = Some(c.id); p }).collect();
        let first_id = new_paragraphs[0].id;
        self.paragraphs.splice(at..at, new_paragraphs);
        self.tables[ti].rows.insert(if below { r + 1 } else { r }, new_row);
        let idx = self.paragraphs.iter().position(|p| p.id == first_id).unwrap();
        let pos = self.paragraph_start(idx);
        self.selection = Selection { anchor: pos, focus: pos };
        self.changed();
    }

    /// Insert a column right (or left) of the current cell (insertTableColumn).
    pub fn insert_table_column(&mut self, right: bool) {
        let Some(cell) = self.current_cell() else { return };
        let Some(ti) = self.table_of_cell(cell) else { return };
        self.checkpoint(EditKind::Other);
        let (r, c) = self.tables[ti].find_cell(cell).unwrap();
        let grid_col = self.tables[ti].grid_col(r, c) + if right { self.tables[ti].rows[r].cells[c].span as usize } else { 0 };
        // Scale existing columns so the table keeps its width.
        let t = &mut self.tables[ti];
        let total: i32 = t.grid.iter().sum();
        let cols = t.grid.len() as i32 + 1;
        let new_width = total / cols;
        for g in &mut t.grid { *g = (*g as i64 * (total - new_width) as i64 / total.max(1) as i64) as i32; }
        t.grid.insert(grid_col.min(t.grid.len()), new_width);
        let template = self.paragraphs[self.cell_paragraphs(cell).start].sibling();
        let mut inserts: Vec<(u64, bool, Paragraph)> = vec![]; // (neighbor cell, after?, paragraph)
        let t = &mut self.tables[ti];
        for row in &mut t.rows {
            let mut at = row.grid_before as usize; let mut index = row.cells.len();
            for (k, cl) in row.cells.iter().enumerate() { if at >= grid_col { index = k; break; } at += cl.span as usize; }
            let new_cell = Cell::new(new_width);
            let mut p = template.clone(); p.id = new_id(); p.cell = Some(new_cell.id);
            let (neighbor, after) = if index < row.cells.len() { (row.cells[index].id, false) } else { (row.cells.last().unwrap().id, true) };
            inserts.push((neighbor, after, p));
            row.cells.insert(index, new_cell);
        }
        let grid = t.grid.clone();
        for row in &mut t.rows { let mut g = row.grid_before as usize; for cl in &mut row.cells { cl.width = grid[g..(g + cl.span as usize).min(grid.len())].iter().sum(); g += cl.span as usize; } }
        for (neighbor, after, p) in inserts {
            let range = self.cell_paragraphs(neighbor);
            let at = if after { range.end } else { range.start };
            self.paragraphs.insert(at, p);
        }
        self.changed();
    }

    /// Delete the current row, column or whole table (deleteTablePart).
    pub fn delete_table_part(&mut self, part: TablePart) {
        let Some(cell) = self.current_cell() else { return };
        let Some(ti) = self.table_of_cell(cell) else { return };
        self.checkpoint(EditKind::Other);
        let (r, c) = self.tables[ti].find_cell(cell).unwrap();
        let mut removed: Vec<u64> = vec![];
        let whole = match part {
            TablePart::Table => true,
            TablePart::Row => self.tables[ti].rows.len() == 1,
            TablePart::Column => self.tables[ti].rows.iter().all(|row| row.cells.len() == 1),
        };
        if whole {
            removed = self.tables[ti].cells().map(|c| c.id).collect();
            self.tables.remove(ti);
        } else if part == TablePart::Row {
            removed = self.tables[ti].rows[r].cells.iter().map(|c| c.id).collect();
            self.tables[ti].rows.remove(r);
        } else {
            let g = self.tables[ti].grid_col(r, c);
            let t = &mut self.tables[ti];
            for row in &mut t.rows {
                let mut at = row.grid_before as usize;
                if let Some(k) = row.cells.iter().position(|cl| { let hit = at <= g && g < at + cl.span as usize; at += cl.span as usize; hit }) {
                    if row.cells[k].span > 1 { row.cells[k].span -= 1; row.cells[k].width -= t.grid[g]; }
                    else if row.cells.len() > 1 { removed.push(row.cells[k].id); row.cells.remove(k); }
                }
            }
            if g < t.grid.len() { t.grid.remove(g); }
        }
        let first = self.paragraphs.iter().position(|p| p.cell.is_some_and(|c| removed.contains(&c))).unwrap_or(0);
        self.paragraphs.retain(|p| !p.cell.is_some_and(|c| removed.contains(&c)));
        if self.paragraphs.is_empty() { self.paragraphs.push(Paragraph::default()); }
        let pos = self.paragraph_start(first.min(self.paragraphs.len() - 1));
        self.selection = Selection { anchor: pos, focus: pos };
        self.changed();
    }

    /// Move the caret to the next/previous cell (Tab / Shift+Tab); adds a row past the end.
    pub fn move_cell(&mut self, forward: bool) -> bool {
        let Some(cell) = self.current_cell() else { return false };
        let Some(ti) = self.table_of_cell(cell) else { return false };
        let ids: Vec<u64> = self.tables[ti].cells().filter(|c| c.v_merge != VMerge::Continue).map(|c| c.id).collect();
        let k = ids.iter().position(|&c| c == cell).unwrap_or(0);
        let target = if forward { ids.get(k + 1).copied() } else { k.checked_sub(1).map(|k| ids[k]) };
        match target {
            Some(t) => {
                let range = self.cell_paragraphs(t);
                let s = self.paragraph_start(range.start);
                let e = self.paragraph_start(range.end - 1) + self.paragraphs[range.end - 1].len();
                self.select(s, e);
            }
            None if forward => { self.insert_table_row(true); }
            None => {}
        }
        true
    }

    // ---- images ---------------------------------------------------------------------------------

    pub fn insert_image(&mut self, bytes: Vec<u8>, content_type: &str, width: f32, height: f32) {
        let ext = if content_type.contains("png") { "png" } else if content_type.contains("gif") { "gif" } else { "jpeg" };
        let mut n = self.assets.len() + 1;
        while self.assets.contains_key(&format!("word/media/image{n}.{ext}")) { n += 1; }
        let asset = format!("word/media/image{n}.{ext}");
        self.assets.insert(asset.clone(), Asset { bytes, content_type: content_type.into() });
        // Fit within the writing width.
        let max_w = crate::page::twips(self.page.width - self.page.left - self.page.right - self.page.gutter);
        let scale = if width > max_w { max_w / width } else { 1.0 };
        self.checkpoint(EditKind::Other);
        let (s, e) = self.range();
        self.delete_range(s, e);
        let run = Run { text: chars::OBJECT.to_string(), image: Some(Image { asset, width: width * scale, height: height * scale, ..Default::default() }), ..Default::default() };
        let end = self.insert_runs(s, vec![run]);
        self.selection = Selection { anchor: end, focus: end };
        self.changed();
    }
    /// The picture run at [pos, pos+1), if any.
    pub fn image_at(&self, pos: usize) -> Option<&Image> {
        let (i, o) = self.locate(pos);
        let mut at = 0;
        for r in &self.paragraphs[i].runs { let len = r.len(); if o >= at && o < at + len { return r.image.as_ref(); } at += len; }
        None
    }
    pub fn resize_image(&mut self, pos: usize, width: f32, height: f32) {
        let (i, o) = self.locate(pos);
        self.checkpoint(EditKind::Other);
        let mut at = 0;
        for r in &mut self.paragraphs[i].runs {
            let len = r.len();
            if o >= at && o < at + len {
                if let Some(img) = &mut r.image { img.width = width.max(4.0); img.height = height.max(4.0); img.raw = None; }
                if let Some(obj) = &mut r.object { obj.set_size(width.max(8.0), height.max(0.0)); }
            }
            at += len;
        }
        self.changed();
    }

    /// Apply `f` to the picture at `pos` (one undo step) and keep it selected.
    pub fn edit_image(&mut self, pos: usize, f: impl FnOnce(&mut Image)) {
        let (i, o) = self.locate(pos);
        self.checkpoint(EditKind::Other);
        let (mut at, mut f) = (0, Some(f));
        for r in &mut self.paragraphs[i].runs {
            let len = r.len();
            if o >= at && o < at + len { if let (Some(img), Some(f)) = (&mut r.image, f.take()) { f(img); img.raw = None; } }
            at += len;
        }
        self.selection = Selection { anchor: pos, focus: pos + 1 };
        self.changed();
    }
    /// Store new picture bytes as a fresh PNG asset and return its part name.
    pub fn add_png_asset(&mut self, bytes: Vec<u8>) -> String {
        let mut n = self.assets.len() + 1;
        while self.assets.contains_key(&format!("word/media/image{n}.png")) { n += 1; }
        let asset = format!("word/media/image{n}.png");
        self.assets.insert(asset.clone(), Asset { bytes, content_type: "image/png".into() });
        asset
    }
    /// Apply a picture edit (Picture Format tab): the pixels change, the old asset is kept for
    /// undo and Reset Picture. Returns false when the picture can't be decoded.
    pub fn apply_picture_op(&mut self, pos: usize, op: crate::picture::PictureOp) -> bool {
        let Some(img) = self.image_at(pos).cloned() else { return false };
        let Some(bytes) = self.assets.get(&img.asset).map(|a| a.bytes.clone()) else { return false };
        let Some((png, (w, h))) = crate::picture::apply(&bytes, op, (img.width, img.height)) else { return false };
        let asset = self.add_png_asset(png);
        self.edit_image(pos, |i| { if i.original.is_none() { i.original = Some(i.asset.clone()); } i.asset = asset; i.width = w; i.height = h; i.effects.push(op); });
        true
    }
    /// Reset Picture: the original pixels at their natural size (fit to the text width).
    pub fn reset_picture(&mut self, pos: usize) {
        let Some(img) = self.image_at(pos).cloned() else { return };
        let asset = img.original.clone().unwrap_or(img.asset.clone());
        let Some((w, h)) = self.assets.get(&asset).and_then(|a| crate::picture::pixel_size(&a.bytes)) else { return };
        let max_w = crate::page::twips(self.page.width - self.page.left - self.page.right - self.page.gutter);
        let scale = if w > max_w { max_w / w } else { 1.0 };
        self.edit_image(pos, |i| { i.asset = asset; i.original = None; i.width = w * scale; i.height = h * scale; i.effects.clear(); i.rotation = 0.0; i.flip_h = false; i.flip_v = false; });
    }
    /// Replace a picture's bytes (Change Picture), keeping its width.
    pub fn change_picture(&mut self, pos: usize, bytes: Vec<u8>) -> bool {
        let Some((w, h)) = crate::picture::pixel_size(&bytes) else { return false };
        let Some(png) = crate::picture::to_png(&bytes) else { return false };
        let asset = self.add_png_asset(png);
        self.edit_image(pos, |i| { let width = i.width; i.asset = asset; i.original = None; i.effects.clear(); i.width = width; i.height = width * h / w; });
        true
    }

    /// The floating placement of the picture or object at `pos`.
    pub fn anchor_at(&self, pos: usize) -> Option<Anchor> {
        let (i, o) = self.locate(pos);
        let mut at = 0;
        for r in &self.paragraphs[i].runs { let len = r.len(); if o >= at && o < at + len { return r.anchor; } at += len; }
        None
    }
    /// Make the picture or object at `pos` float (Some) or sit in line with text (None).
    pub fn set_anchor(&mut self, pos: usize, anchor: Option<Anchor>) {
        let (i, o) = self.locate(pos);
        self.checkpoint(EditKind::Other);
        let mut at = 0;
        for r in &mut self.paragraphs[i].runs {
            let len = r.len();
            if o >= at && o < at + len && (r.image.is_some() || r.object.is_some()) {
                r.anchor = anchor;
                if let Some(img) = &mut r.image { img.raw = None; }
                if let Some(obj) = &mut r.object { obj.clear_raw(); }
            }
            at += len;
        }
        self.selection = Selection { anchor: pos, focus: pos + 1 };
        self.changed();
    }
    /// Move a floating object to a new anchor paragraph and offset (dragging it on the page):
    /// the run moves to the start of the paragraph holding `target`.
    pub fn move_float(&mut self, pos: usize, target: usize, x: f32, y: f32) -> usize {
        let Some(mut anchor) = self.anchor_at(pos) else { return pos };
        anchor.x = x; anchor.y = y;
        anchor.x_from = RelFrom::Margin; anchor.y_from = RelFrom::Paragraph;
        anchor.x_align = None; anchor.y_align = None;
        let (pi, _) = self.locate(pos);
        let (ti, _) = self.locate(target.min(self.len()));
        let mut new_pos = pos;
        if ti != pi && self.paragraphs[ti].cell.is_none() {
            // Cut the run and insert it at the target paragraph's start.
            let run = { let (i, o) = self.locate(pos); let mut at = 0; let mut found = None; for r in &self.paragraphs[i].runs { if o >= at && o < at + r.len() { found = Some(r.clone()); break; } at += r.len(); } found };
            if let Some(run) = run {
                self.checkpoint(EditKind::Other);
                self.delete_range(pos, pos + 1);
                let (ti, _) = self.locate(target.min(self.len()).saturating_sub(usize::from(target > pos)));
                let start = self.paragraph_start(ti);
                self.insert_runs(start, vec![run]);
                new_pos = start;
            }
        }
        self.set_anchor(new_pos, Some(anchor));
        new_pos
    }
    /// Rotate a picture (free rotation, kept as a:xfrm rot).
    pub fn set_image_rotation(&mut self, pos: usize, degrees: f32) {
        self.edit_image(pos, |i| i.rotation = degrees.rem_euclid(360.0));
    }
    /// Mirror a picture horizontally or vertically (a:xfrm flipH/flipV, the pixels unchanged).
    pub fn flip_image(&mut self, pos: usize, horizontal: bool) {
        self.edit_image(pos, |i| if horizontal { i.flip_h = !i.flip_h } else { i.flip_v = !i.flip_v });
    }

    /// Insert a shape, chart or diagram at the caret (replacing the selection) and select it.
    pub fn insert_object(&mut self, object: crate::objects::Object) {
        self.checkpoint(EditKind::Other);
        let (s, e) = self.range();
        self.delete_range(s, e);
        let run = Run { text: chars::OBJECT.to_string(), object: Some(object), ..Default::default() };
        self.insert_runs(s, vec![run]);
        self.selection = Selection { anchor: s, focus: s + 1 };
        self.changed();
    }
    /// The object at [pos, pos+1).
    pub fn object_at(&self, pos: usize) -> Option<&crate::objects::Object> {
        let (i, o) = self.locate(pos);
        let mut at = 0;
        for r in &self.paragraphs[i].runs { let len = r.len(); if o >= at && o < at + len { return r.object.as_ref(); } at += len; }
        None
    }
    /// Replace the object at `pos` (edits from the object's ribbon tab or dialog).
    pub fn set_object(&mut self, pos: usize, object: crate::objects::Object) {
        let (i, o) = self.locate(pos);
        self.checkpoint(EditKind::Other);
        let mut at = 0;
        for r in &mut self.paragraphs[i].runs { let len = r.len(); if o >= at && o < at + len && r.object.is_some() { r.object = Some(object.clone()); } at += len; }
        self.changed();
    }

    // ---- search ----------------------------------------------------------------------------------

    /// Literal matches as (start, end) positions (search.js: paragraphs interrupt matches).
    pub fn find_all(&self, query: &str, match_case: bool, whole_words: bool) -> Vec<(usize, usize)> {
        if query.is_empty() || query.contains('\n') { return vec![]; }
        let fold = |c: char| if match_case { c } else { c.to_lowercase().next().unwrap_or(c) };
        let q: Vec<char> = query.chars().map(fold).collect();
        let text: Vec<char> = self.text().chars().collect();
        let folded: Vec<char> = text.iter().map(|&c| fold(c)).collect();
        let is_word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_');
        let mut out = vec![];
        let mut i = 0;
        while i + q.len() <= folded.len() {
            if folded[i..i + q.len()] == q[..] && (!whole_words || !is_word(text.get(i.wrapping_sub(1)).filter(|_| i > 0)) && !is_word(text.get(i + q.len()))) {
                out.push((i, i + q.len()));
                i += q.len();
            } else { i += 1; }
        }
        out
    }
    /// Replace all matches in one undo step; returns the count.
    pub fn replace_all(&mut self, query: &str, with: &str, match_case: bool, whole_words: bool) -> usize {
        let rec = self.rec_enter(|| format!("replace_all({query:?}, {with:?}, {match_case}, {whole_words});"));
        let r = self.replace_all__impl(query, with, match_case, whole_words);
        self.rec_exit(rec);
        r
    }
    #[allow(non_snake_case)]
    fn replace_all__impl(&mut self, query: &str, with: &str, match_case: bool, whole_words: bool) -> usize {
        let matches = self.find_all(query, match_case, whole_words);
        if matches.is_empty() { return 0; }
        self.checkpoint(EditKind::Other);
        for &(s, e) in matches.iter().rev() {
            // New text takes the first matched character's formatting.
            let mut run = self.typing_run(s + 1);
            run.link = None;
            self.delete_range(s, e);
            run.text = with.to_string();
            if !with.is_empty() { self.insert_runs(s, vec![run]); }
        }
        let n = self.len();
        self.selection = Selection { anchor: self.selection.anchor.min(n), focus: self.selection.anchor.min(n) };
        self.changed();
        matches.len()
    }

    // ---- clipboard fragments ---------------------------------------------------------------------

    /// Selected content as paragraphs (cell membership removed), for rich in-app paste.
    pub fn fragment(&self) -> Vec<Paragraph> {
        let (s, e) = self.range();
        let starts = self.paragraph_starts();
        let mut out = vec![];
        for i in self.selected_paragraph_indices() {
            let ps = starts[i];
            let len = self.paragraphs[i].len();
            let mut p = self.paragraphs[i].clone();
            let (a, b) = (s.saturating_sub(ps).min(len), e.saturating_sub(ps).min(len));
            let runs = p.take(a, b);
            p.runs = runs; p.cell = None; p.id = new_id();
            out.push(p);
        }
        if e > s && self.locate(e).1 == 0 && self.locate(e).0 > self.locate(s).0 { out.push(Paragraph::default()); }
        out
    }
    /// Paste paragraphs from `fragment`, keeping run formatting and paragraph styles.
    pub fn paste_fragment(&mut self, mut fragment: Vec<Paragraph>) {
        if fragment.is_empty() { return; }
        if self.track_changes {
            // Tracked paste keeps the fragment's text but not its paragraph formatting.
            let text: Vec<String> = fragment.iter().map(|p| p.runs.iter().filter(|r| r.revision.as_ref().is_none_or(|v| v.kind != RevKind::Del)).map(|r| r.text.as_str()).collect()).collect();
            return self.replace(&text.join("\n"));
        }
        for p in &mut fragment { p.runs.retain(|r| r.revision.as_ref().is_none_or(|v| v.kind != RevKind::Del)); for r in &mut p.runs { r.revision = None; r.marker = None; } p.runs.retain(|r| !r.text.is_empty()); p.mark_revision = None; }
        self.checkpoint(EditKind::Other);
        let (s, e) = self.range();
        self.delete_range(s, e);
        let mut runs = vec![];
        for (k, p) in fragment.iter().enumerate() {
            if k > 0 { runs.push(Run::text("\n")); }
            runs.extend(p.runs.iter().cloned());
        }
        // Copy images' bytes are shared through `assets`, which persist across paste in one document.
        let (first_index, first_offset) = self.locate(s);
        let end = self.insert_runs(s, runs);
        // Paragraph styles from the fragment apply to the pasted paragraphs after the first (or the first when it was empty).
        for (k, p) in fragment.iter().enumerate() {
            let idx = first_index + k;
            if idx >= self.paragraphs.len() { break; }
            if k == 0 && first_offset != 0 { continue; }
            if k == fragment.len() - 1 && fragment.len() > 1 && p.runs.is_empty() { continue; }
            self.paragraphs[idx].style = p.style.clone();
            let cell = self.paragraphs[idx].cell;
            self.paragraphs[idx].props = p.props.clone();
            self.paragraphs[idx].cell = cell;
        }
        self.selection = Selection { anchor: end, focus: end };
        self.changed();
    }

    // ---- comments (comments.js) ----------------------------------------------------------

    /// Comment anchors as (id, start, end), in document order.
    pub fn comment_anchors(&self) -> Vec<(String, usize, usize)> {
        let mut found: Vec<(String, Option<usize>, Option<usize>)> = vec![];
        let mut at = 0;
        for p in &self.paragraphs {
            for r in &p.runs {
                match &r.marker {
                    Some(Marker::CommentStart(id)) => match found.iter_mut().find(|f| f.0 == *id) { Some(f) => f.1 = Some(at), None => found.push((id.clone(), Some(at), None)) },
                    Some(Marker::CommentEnd(id)) => match found.iter_mut().find(|f| f.0 == *id) { Some(f) => f.2 = Some(at), None => found.push((id.clone(), None, Some(at))) },
                    _ => {}
                }
                at += r.len();
            }
            at += 1;
        }
        let mut out: Vec<_> = found.into_iter().map(|(id, s, e)| {
            let (s, e) = (s.or(e).unwrap(), e.or(s).unwrap());
            (id, s.min(e), s.max(e))
        }).collect();
        out.sort_by_key(|a| (a.1, a.2));
        out
    }
    /// The thread root id for a comment.
    pub fn comment_root<'a>(&'a self, id: &'a str) -> &'a str {
        let mut cur = id;
        for _ in 0..32 {
            match self.comments.iter().find(|c| c.id == cur).and_then(|c| c.parent.as_deref()) { Some(p) => cur = p, None => break }
        }
        cur
    }
    fn next_comment_id(&self) -> String {
        let mut n = 0u32;
        while self.comments.iter().any(|c| c.id == n.to_string()) { n += 1; }
        n.to_string()
    }
    fn new_para_id(&self) -> String {
        let mut x = (self.comments.len() as u32 + 1).wrapping_mul(2654435761) & 0x7FFF_FFFE;
        while self.comments.iter().any(|c| c.para_id == format!("{x:08X}")) { x = (x + 7) & 0x7FFF_FFFF; }
        format!("{x:08X}")
    }
    /// Add a comment on the selection (or the word at the caret). Returns its id.
    pub fn add_comment(&mut self, author: &str, text: &str, date: &str) -> String {
        let (mut s, mut e) = self.range();
        if s == e { (s, e) = self.word_at(s); }
        self.checkpoint(EditKind::Other);
        let id = self.next_comment_id();
        // End first so the start position stays valid.
        for (pos, m) in [(e, Marker::CommentEnd(id.clone())), (s, Marker::CommentStart(id.clone()))] {
            let (i, o) = self.locate(pos);
            let p = &mut self.paragraphs[i];
            let at = p.split_at(o);
            p.runs.insert(at, Run::marker(m));
        }
        let para_id = self.new_para_id();
        self.comments.push(Comment { id: id.clone(), author: author.into(), initials: initials(author), date: date.into(), text: text.into(), parent: None, done: false, para_id });
        self.comments_changed = true;
        self.changed();
        id
    }
    pub fn reply_to_comment(&mut self, parent: &str, author: &str, text: &str, date: &str) -> Option<String> {
        let root = self.comment_root(parent).to_string();
        if !self.comments.iter().any(|c| c.id == root) { return None; }
        self.checkpoint(EditKind::Other);
        let id = self.next_comment_id();
        let para_id = self.new_para_id();
        self.comments.push(Comment { id: id.clone(), author: author.into(), initials: initials(author), date: date.into(), text: text.into(), parent: Some(root), done: false, para_id });
        self.comments_changed = true;
        self.changed();
        Some(id)
    }
    pub fn edit_comment(&mut self, id: &str, text: &str) {
        if self.comments.iter().any(|c| c.id == id && c.text != text) {
            self.checkpoint(EditKind::Other);
            if let Some(c) = self.comments.iter_mut().find(|c| c.id == id) { c.text = text.into(); }
            self.comments_changed = true;
            self.changed();
        }
    }
    /// Resolve or reopen a thread (Word marks the whole thread).
    pub fn set_comment_done(&mut self, id: &str, done: bool) {
        let root = self.comment_root(id).to_string();
        self.checkpoint(EditKind::Other);
        for c in &mut self.comments { if c.id == root || c.parent.as_deref() == Some(&root) { c.done = done; } }
        self.comments_changed = true;
        self.changed();
    }
    /// Delete a comment; deleting a thread root deletes its replies and anchor.
    pub fn delete_comment(&mut self, id: &str) {
        let is_root = self.comments.iter().any(|c| c.id == id && c.parent.is_none());
        self.checkpoint(EditKind::Other);
        let doomed: Vec<String> = self.comments.iter().filter(|c| c.id == id || is_root && c.parent.as_deref() == Some(id)).map(|c| c.id.clone()).collect();
        self.comments.retain(|c| !doomed.contains(&c.id));
        for p in &mut self.paragraphs {
            p.runs.retain(|r| !matches!(&r.marker, Some(Marker::CommentStart(m) | Marker::CommentEnd(m)) if doomed.contains(m)));
        }
        self.comments_changed = true;
        self.changed();
    }
    /// Comment threads touching a position (for the caret's active comment).
    pub fn comments_at(&self, pos: usize) -> Vec<String> {
        self.comment_anchors().into_iter().filter(|(_, s, e)| *s <= pos && pos <= *e).map(|(id, ..)| id).collect()
    }

    // ---- footnotes and endnotes (footnotes.js) ---------------------------------------------

    /// Display numbers for note references, in document order: footnotes 1, 2, 3…; endnotes i, ii, iii….
    pub fn note_labels(&self) -> HashMap<(NoteKind, String), String> {
        let mut out = HashMap::new();
        let ((ff, fs), (ef, es)) = (self.page.footnote_format, self.page.endnote_format);
        let (mut foot, mut end) = (fs.saturating_sub(1), es.saturating_sub(1));
        for p in &self.paragraphs {
            for r in &p.runs {
                if let Some(n) = &r.note {
                    if out.contains_key(&(n.kind, n.id.clone())) { continue; }
                    let label = match n.kind {
                        NoteKind::Foot => { foot += 1; crate::numbering::format_number(foot, ff) }
                        NoteKind::End => { end += 1; crate::numbering::format_number(end, ef) }
                    };
                    out.insert((n.kind, n.id.clone()), label);
                }
            }
        }
        out
    }
    /// Insert a note reference at the caret with an empty body. Returns the note id.
    pub fn insert_note(&mut self, kind: NoteKind) -> String {
        let mut n = 1u32;
        while self.notes.iter().any(|x| x.kind == kind && x.id == n.to_string()) { n += 1; }
        let id = n.to_string();
        self.checkpoint(EditKind::Other);
        let (s, e) = self.range();
        self.delete_range(s, e);
        let mut run = self.typing_run(s);
        run.text = chars::NOTE.to_string();
        run.note = Some(NoteRef { kind, id: id.clone() });
        run.props.vert_align = None;
        let style = if kind == NoteKind::Foot { "FootnoteReference" } else { "EndnoteReference" };
        if self.styles.get(style).is_some() { run.style = Some(style.into()); } else { run.props.vert_align = Some(VertAlign::Superscript); }
        let end = self.insert_runs(s, vec![run]);
        let mut body = Paragraph::default();
        let text_style = if kind == NoteKind::Foot { "FootnoteText" } else { "EndnoteText" };
        if self.styles.get(text_style).is_some() { body.style = Some(text_style.into()); }
        self.notes.push(Note { kind, id: id.clone(), paragraphs: vec![body] });
        self.notes_changed = true;
        self.selection = Selection { anchor: end, focus: end };
        self.changed();
        id
    }
    pub fn note(&self, kind: NoteKind, id: &str) -> Option<&Note> { self.notes.iter().find(|n| n.kind == kind && n.id == id) }
    pub fn note_text(&self, kind: NoteKind, id: &str) -> String {
        self.note(kind, id).map(|n| n.paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
    }
    /// Replace a note's text, one paragraph per line, keeping the first paragraph's formatting.
    pub fn set_note_text(&mut self, kind: NoteKind, id: &str, text: &str) {
        if self.note_text(kind, id) == text { return; }
        self.checkpoint(EditKind::Other);
        if let Some(n) = self.notes.iter_mut().find(|n| n.kind == kind && n.id == id) {
            let template = n.paragraphs.first().cloned().unwrap_or_default();
            let run = template.runs.first().map(|r| Run { text: String::new(), props: r.props.clone(), style: r.style.clone(), ..Default::default() }).unwrap_or_default();
            n.paragraphs = text.split('\n').map(|line| {
                let mut p = template.sibling();
                if !line.is_empty() { p.runs.push(Run { text: line.into(), ..run.clone() }); }
                p
            }).collect();
        }
        self.notes_changed = true;
        self.changed();
    }
    /// Note references in document order: (kind, id, position).
    pub fn note_refs(&self) -> Vec<(NoteKind, String, usize)> {
        let mut out = vec![];
        let mut at = 0;
        for p in &self.paragraphs {
            for r in &p.runs { if let Some(n) = &r.note { out.push((n.kind, n.id.clone(), at)); } at += r.len(); }
            at += 1;
        }
        out
    }

    // ---- moving text (drag and drop, Move Paragraph) ----------------------------------------

    /// Move (or copy) [s, e) to `target` in one undo step; selects the moved text. A target inside
    /// the source range does nothing (dropping text onto itself).
    pub fn move_range(&mut self, s: usize, e: usize, target: usize, copy: bool) -> bool {
        if s >= e || (!copy && target >= s && target <= e) { return false; }
        self.select(s, e);
        let fragment = self.fragment();
        self.checkpoint(EditKind::Other);
        let len = e - s;
        let mut at = target;
        if !copy {
            self.delete_range(s, e);
            if target > e { at -= len; }
        }
        // The fragment's text, as it will be inserted, decides the new selection length.
        let inserted = fragment.iter().map(|p| p.len()).sum::<usize>() + fragment.len().saturating_sub(1);
        self.selection = Selection { anchor: at, focus: at };
        let history = self.history.len();
        self.paste_fragment(fragment);
        // paste_fragment checkpoints too; keep one undo step for the whole move.
        self.history.truncate(history);
        self.selection = Selection { anchor: at, focus: at + inserted };
        self.changed();
        true
    }

    /// Move the selected paragraphs above the previous (or below the next) paragraph in the same
    /// container (Word's Move Up/Down).
    pub fn move_paragraphs(&mut self, up: bool) -> bool {
        let range = self.selected_paragraph_indices();
        let (first, last) = (*range.start(), *range.end());
        let cell = self.paragraphs[first].cell;
        if self.paragraphs[first..=last].iter().any(|p| p.cell != cell) { return false; }
        let neighbor = if up { first.checked_sub(1) } else { Some(last + 1) };
        let Some(n) = neighbor.filter(|&n| n < self.paragraphs.len() && self.paragraphs[n].cell == cell) else { return false };
        let (s, e) = self.range();
        let start_before = self.paragraph_start(first);
        self.checkpoint(EditKind::Other);
        let block: Vec<Paragraph> = self.paragraphs.drain(first..=last).collect();
        let at = if up { n } else { n + 1 - block.len() };
        self.paragraphs.splice(at..at, block);
        let start_after = self.paragraph_start(at);
        let shift = |p: usize| (p as i64 + start_after as i64 - start_before as i64) as usize;
        self.selection = Selection { anchor: shift(s), focus: shift(e) };
        self.changed();
        true
    }

    /// The sentence containing `pos` (⌘-click).
    pub fn sentence_at(&self, pos: usize) -> (usize, usize) {
        let (ps, pe) = self.paragraph_bounds(pos);
        let text: Vec<char> = self.text().chars().skip(ps).take(pe - ps).collect();
        let at = pos - ps;
        let ends = |i: usize| matches!(text[i], '.' | '!' | '?');
        let mut a = at.min(text.len());
        while a > 0 && !(ends(a - 1) && text.get(a).is_some_and(|c| c.is_whitespace())) { a -= 1; }
        while a < text.len() && text[a].is_whitespace() { a += 1; }
        let mut b = at.min(text.len());
        while b < text.len() && !ends(b) { b += 1; }
        if b < text.len() { b += 1; }
        while b < text.len() && text[b] == ' ' { b += 1; }
        (ps + a, ps + b)
    }

    // ---- bookmarks and cross-references (bookmarks.js, body-fields.js) ---------------------

    /// Bookmarks as (name, start, end), in document order.
    pub fn bookmarks(&self) -> Vec<(String, usize, usize)> {
        let mut found: Vec<(String, Option<usize>, Option<usize>)> = vec![];
        let mut at = 0;
        for p in &self.paragraphs {
            for r in &p.runs {
                match &r.marker {
                    Some(Marker::BookmarkStart(n)) => match found.iter_mut().find(|f| f.0 == *n) { Some(f) => f.1 = Some(at), None => found.push((n.clone(), Some(at), None)) },
                    Some(Marker::BookmarkEnd(n)) => match found.iter_mut().find(|f| f.0 == *n) { Some(f) => f.2 = Some(at), None => found.push((n.clone(), None, Some(at))) },
                    _ => {}
                }
                at += r.len();
            }
            at += 1;
        }
        let mut out: Vec<_> = found.into_iter().filter_map(|(n, s, e)| { let (s, e) = (s.or(e)?, e.or(s)?); Some((n, s.min(e), s.max(e))) }).collect();
        out.sort_by_key(|b| b.1);
        out
    }
    /// Word's rule: a letter first, then letters, digits or underscores, at most 40 characters.
    pub fn valid_bookmark_name(name: &str) -> bool {
        let mut c = name.chars();
        c.next().is_some_and(|f| f.is_alphabetic() || f == '_') && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') && name.chars().count() <= 40
    }
    /// Add (or move) a bookmark around the selection.
    pub fn add_bookmark(&mut self, name: &str) -> Result<(), String> {
        if !Self::valid_bookmark_name(name) { return Err("Bookmark names start with a letter and use only letters, numbers and underscores.".into()); }
        let (s, e) = self.range();
        self.checkpoint(EditKind::Other);
        self.remove_bookmark_markers(name);
        self.insert_marker_pair(s, e, Marker::BookmarkStart(name.into()), Marker::BookmarkEnd(name.into()));
        self.changed();
        Ok(())
    }
    pub fn delete_bookmark(&mut self, name: &str) {
        self.checkpoint(EditKind::Other);
        self.remove_bookmark_markers(name);
        self.changed();
    }
    fn remove_bookmark_markers(&mut self, name: &str) {
        for p in &mut self.paragraphs { p.runs.retain(|r| !matches!(&r.marker, Some(Marker::BookmarkStart(n) | Marker::BookmarkEnd(n)) if n == name)); }
    }
    pub(crate) fn insert_marker_pair(&mut self, s: usize, e: usize, start: Marker, end: Marker) {
        for (pos, m) in [(e, end), (s, start)] {
            let (i, o) = self.locate(pos);
            let p = &mut self.paragraphs[i];
            let at = p.split_at(o);
            p.runs.insert(at, Run::marker(m));
        }
    }
    /// The text inside a bookmark (a REF field's result).
    pub fn bookmark_text(&self, name: &str) -> Option<String> {
        self.bookmarks().into_iter().find(|b| b.0 == name).map(|(_, s, e)| self.text_between(s, e).replace('\n', " "))
    }
    /// A heading's hidden bookmark (_RefNNN), created around its text when needed (Word does the same).
    pub fn heading_bookmark(&mut self, index: usize) -> String {
        let s = self.paragraph_start(index);
        let e = s + self.paragraphs[index].len();
        if let Some(b) = self.bookmarks().into_iter().find(|b| b.0.starts_with("_Ref") && b.1 == s && b.2 == e) { return b.0; }
        let mut n = 100_000 + index as u64 * 7 + self.revision % 1000;
        let names: Vec<String> = self.bookmarks().into_iter().map(|b| b.0).collect();
        while names.contains(&format!("_Ref{n}")) { n += 1; }
        let name = format!("_Ref{n}");
        self.insert_marker_pair(s, e, Marker::BookmarkStart(name.clone()), Marker::BookmarkEnd(name.clone()));
        name
    }
    /// Insert a cross-reference field at the caret: the bookmark's text or its page number,
    /// clickable to jump there (REF/PAGEREF … \h).
    /// Insert a field (Date & Time, Quick Parts ▸ Field, Document Property) at the caret.
    pub fn insert_field(&mut self, field: Field) {
        self.checkpoint(EditKind::Other);
        let (s, e) = self.range();
        self.delete_range(s, e);
        let mut run = self.typing_run(s);
        run.text = " ".into();
        run.field = Some(field);
        let end = self.insert_runs(s, vec![run]);
        self.selection = Selection { anchor: end, focus: end };
        self.changed();
    }
    pub fn insert_cross_reference(&mut self, bookmark: &str, page: bool) {
        self.checkpoint(EditKind::Other);
        let (s, e) = self.range();
        self.delete_range(s, e);
        let mut run = self.typing_run(s);
        run.text = " ".into();
        run.field = Some(if page { Field::PageRef(bookmark.into()) } else { Field::Ref(bookmark.into()) });
        run.link = Some(format!("#{bookmark}"));
        let end = self.insert_runs(s, vec![run]);
        self.selection = Selection { anchor: end, focus: end };
        self.changed();
    }
    pub fn has_reference_fields(&self) -> bool {
        self.paragraphs.iter().any(|p| p.runs.iter().any(|r| matches!(r.field, Some(Field::Ref(_) | Field::PageRef(_)))))
    }

    // ---- statistics ---------------------------------------------------------------------------

    /// Text as it reads with tracked deletions removed (for counts).
    fn visible_text(&self) -> String {
        self.paragraphs.iter().map(|p| p.runs.iter().filter(|r| r.revision.as_ref().is_none_or(|v| v.kind != RevKind::Del)).map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n")
    }
    pub fn word_count(&self) -> usize { self.visible_text().split(|c: char| c.is_whitespace() || c == chars::OBJECT || c == chars::NOTE).filter(|w| !w.is_empty()).count() }
    pub fn character_count(&self) -> usize { self.visible_text().chars().filter(|c| !c.is_control() && *c != chars::OBJECT && *c != chars::NOTE).count() }

    /// Outline entries (outline.js): (paragraph index, level, text).
    pub fn outline(&self) -> Vec<(usize, u8, String)> {
        self.paragraphs.iter().enumerate().filter(|(_, p)| p.cell.is_none()).filter_map(|(i, p)| {
            let level = self.para_format(p).outline_level;
            let text = p.text().trim().to_string();
            (level < 9 && !text.is_empty()).then_some((i, level, text))
        }).collect()
    }
}

fn mirror(a: Alignment) -> Alignment { match a { Alignment::Left => Alignment::Right, Alignment::Right => Alignment::Left, other => other } }

/// Initials from an author name ("Ada Lovelace" → "AL").
pub fn initials(author: &str) -> String {
    author.split_whitespace().filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).take(3).collect()
}

/// Word's default cell margins: 0.08" left and right.
pub const DEFAULT_CELL_MARGINS: [i32; 4] = [0, 108, 0, 108];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TablePart { Row, Column, Table }

fn drop_inherited(p: &mut RunProps, inherited: &RunProps) {
    if p.bold.is_some() && p.bold.unwrap_or(false) == inherited.bold.unwrap_or(false) { p.bold = None; }
    if p.italic.is_some() && p.italic.unwrap_or(false) == inherited.italic.unwrap_or(false) { p.italic = None; }
    if p.bold_cs.is_some() && p.bold_cs.unwrap_or(false) == inherited.bold_cs.unwrap_or(false) { p.bold_cs = None; }
    if p.italic_cs.is_some() && p.italic_cs.unwrap_or(false) == inherited.italic_cs.unwrap_or(false) { p.italic_cs = None; }
    let ul = |v: &Option<String>| v.as_deref().is_some_and(|u| u != "none");
    if p.underline.is_some() && ul(&p.underline) == ul(&inherited.underline) && inherited.underline.is_none() { p.underline = None; }
    if p.strike.is_some() && p.strike.unwrap_or_default() == inherited.strike.unwrap_or_default() { p.strike = None; }
    if p.vert_align.is_some() && p.vert_align.unwrap_or_default() == inherited.vert_align.unwrap_or_default() { p.vert_align = None; }
}

fn clear_all_props() -> RunProps {
    RunProps { rtl: None, outline: Some(false), shadow: Some(false), glow: Some(None), reflection: Some(false), gradient: Some(None), cs_font: None, size_cs: None, bold_cs: Some(false), italic_cs: Some(false), font: None, size: None, bold: Some(false), italic: Some(false), underline: Some("none".into()), strike: Some(Strike::None), color: Some(None), highlight: Some(None), shading: Some(None), vert_align: Some(VertAlign::Baseline), caps: Some(Caps::Normal), spacing: Some(0.0), vanish: Some(false) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document { Document::new(text.split('\n').map(Paragraph::new).collect()) }

    #[test]
    fn caret_font_survives_enter() {
        for text in ["", "Hello"] {
            let mut d = doc(text);
            let n = text.chars().count();
            d.select(n, n);
            d.set_run_props(RunProps { font: Some("Amiri".into()), ..Default::default() });
            d.insert_paragraph();
            assert_eq!(d.current_format().font, "Amiri", "after Enter in {text:?}");
            d.replace("x");
            assert_eq!(d.current_format().font, "Amiri");
            d.insert_paragraph();
            assert_eq!(d.current_format().font, "Amiri", "second Enter");
        }
    }

    #[test]
    fn selection_font_continues_after_enter() {
        let mut d = doc("Hello");
        d.select(0, 5);
        d.set_run_props(RunProps { font: Some("Amiri".into()), ..Default::default() });
        d.select(5, 5);
        d.insert_paragraph();
        assert_eq!(d.current_format().font, "Amiri");
        d.replace("x");
        d.insert_paragraph();
        d.replace("y");
        assert_eq!(d.current_format().font, "Amiri");
    }

    #[test]
    fn typing_splitting_and_merging() {
        let mut d = doc("Hello world");
        d.select(5, 5);
        d.insert_paragraph();
        assert_eq!(d.text(), "Hello\n world");
        assert_eq!(d.paragraphs.len(), 2);
        d.delete(false, false);
        assert_eq!(d.text(), "Hello world");
        d.select(11, 11);
        for c in "!!".chars() { d.replace(&c.to_string()); }
        assert_eq!(d.text(), "Hello world!!");
        d.undo();
        assert_eq!(d.text(), "Hello world", "typing coalesces into one undo step");
        d.redo();
        assert_eq!(d.text(), "Hello world!!");
    }

    #[test]
    fn formatting_splits_runs_and_normalizes() {
        let mut d = doc("abcdef");
        d.select(2, 4);
        d.toggle_bold();
        assert_eq!(d.paragraphs[0].runs.len(), 3);
        assert!(d.current_format().bold);
        d.toggle_bold();
        assert_eq!(d.paragraphs[0].runs.len(), 1, "runs with equal props merge");
    }

    #[test]
    fn cross_paragraph_delete_and_words() {
        let mut d = doc("one two\nthree");
        d.select(4, 10);
        d.delete(false, false);
        assert_eq!(d.text(), "one ree");
        assert_eq!(d.word_boundary(0, true), 4);
        assert_eq!(d.word_at(5), (4, 7));
    }

    #[test]
    fn lists_toggle_and_find() {
        let mut d = doc("a\nb\nc");
        d.select(0, 3);
        d.toggle_list(ListKind::Bullet);
        assert!(d.para_format(&d.paragraphs[1]).numbering.is_some());
        assert!(d.para_format(&d.paragraphs[2]).numbering.is_none());
        d.toggle_list(ListKind::Bullet);
        assert!(d.para_format(&d.paragraphs[0]).numbering.is_none());
        let mut d = doc("Cat cat CAT concat");
        assert_eq!(d.find_all("cat", false, true).len(), 3);
        assert_eq!(d.replace_all("cat", "dog", true, false), 2);
        assert_eq!(d.text(), "Cat dog CAT condog");
    }

    #[test]
    fn tables_insert_rows_and_columns() {
        let mut d = doc("x");
        d.insert_table(2, 2);
        assert_eq!(d.paragraphs.len(), 6);
        d.replace("A");
        d.move_cell(true);
        d.replace("B");
        d.insert_table_row(true);
        assert_eq!(d.tables[0].rows.len(), 3);
        d.insert_table_column(true);
        assert!(d.tables[0].rows.iter().all(|r| r.cells.len() == 3));
        assert_eq!(d.paragraphs.iter().filter(|p| p.cell.is_some()).count(), 9);
        d.delete_table_part(TablePart::Table);
        assert!(d.tables.is_empty());
        assert_eq!(d.text(), "x\n");
    }

    #[test]
    fn comments_anchor_reply_resolve_delete() {
        let mut d = doc("The quick brown fox");
        d.select(4, 9);
        let id = d.add_comment("Ada Lovelace", "Is it quick?", "2026-01-01T00:00:00Z");
        assert_eq!(d.comment_anchors(), vec![(id.clone(), 4, 9)]);
        assert_eq!(d.text(), "The quick brown fox", "markers take no positions");
        // Edits before and inside the anchor move and stretch it.
        d.select(0, 0); d.replace("Oh, ");
        d.select(10, 10); d.replace("!");
        assert_eq!(d.comment_anchors()[0].1..d.comment_anchors()[0].2, 8..14);
        // Deleting the anchored text collapses but keeps the comment.
        d.select(8, 14); d.delete(false, false);
        assert_eq!(d.comment_anchors()[0].1, 8);
        let reply = d.reply_to_comment(&id, "Grace", "Yes", "2026-01-02T00:00:00Z").unwrap();
        d.set_comment_done(&reply, true);
        assert!(d.comments.iter().all(|c| c.done));
        assert_eq!(d.comments[0].initials, "AL");
        d.delete_comment(&id);
        assert!(d.comments.is_empty() && d.comment_anchors().is_empty());
        d.undo();
        assert_eq!(d.comments.len(), 2);
    }

    #[test]
    fn tracked_typing_deleting_accept_reject() {
        let mut d = doc("one two three");
        d.revision_author = "Ada".into();
        d.set_track_changes(true);
        // Delete "two " (Backspace over a selection) and type "2 ".
        d.select(4, 8);
        d.replace("2 ");
        assert_eq!(d.text(), "one two 2 three", "deleted text stays until accepted");
        let revs = d.revisions();
        assert_eq!(revs.iter().map(|r| (r.0, r.1, r.2)).collect::<Vec<_>>(), vec![(4, 8, RevKind::Del), (8, 10, RevKind::Ins)]);
        // Backspace over one's own insertion removes it outright.
        d.delete(false, false);
        assert_eq!(d.text(), "one two 2three");
        // Backspace over original text marks it deleted and keeps the caret before it.
        d.select(3, 3);
        d.delete(false, false);
        assert_eq!((d.text().as_str(), d.range()), ("one two 2three", (2, 2)));
        // A tracked paragraph split, then rejecting everything restores the original.
        d.select(14, 14);
        d.insert_paragraph();
        assert_eq!(d.paragraphs.len(), 2);
        let n = d.len();
        d.resolve_revisions(false, 0, n, true);
        assert_eq!(d.text(), "one two three");
        assert!(d.revisions().is_empty());
        // Accepting applies them instead.
        d.select(0, 4);
        d.replace("");
        d.select(0, 0);
        d.replace("1 ");
        d.resolve_revisions(true, 0, 0, true);
        assert_eq!(d.text(), "1 two three");
        d.undo();
        assert_eq!(d.revisions().len(), 2, "accepting is one undo step");
    }

    #[test]
    fn notes_insert_number_and_edit() {
        let mut d = doc("First claim. Second claim.");
        d.select(12, 12);
        let a = d.insert_note(NoteKind::Foot);
        d.set_note_text(NoteKind::Foot, &a, "Source A");
        d.select(0, 0);
        let b = d.insert_note(NoteKind::Foot);
        let labels = d.note_labels();
        assert_eq!(labels[&(NoteKind::Foot, b.clone())], "1", "numbers follow document order");
        assert_eq!(labels[&(NoteKind::Foot, a.clone())], "2");
        assert_eq!(d.note_text(NoteKind::Foot, &a), "Source A");
        d.select(0, 0);
        let e = d.insert_note(NoteKind::End);
        assert_eq!(d.note_labels()[&(NoteKind::End, e)], "i");
        d.replace("x");
        assert!(!d.current_format().bold && d.current_format().vert_align == VertAlign::Baseline, "typing after a reference is normal text");
        d.undo();
        d.undo();
        assert_eq!(d.notes.len(), 2);
    }

    #[test]
    fn move_copy_and_reorder() {
        let mut d = doc("alpha beta gamma");
        assert!(d.move_range(0, 6, 16, false));
        assert_eq!(d.text(), "beta gammaalpha ");
        assert_eq!(d.selected_text(), "alpha ");
        d.undo();
        assert_eq!(d.text(), "alpha beta gamma", "a move is one undo step");
        assert!(!d.move_range(0, 6, 3, false), "dropping onto itself does nothing");
        assert!(d.move_range(6, 10, 0, true));
        assert_eq!(d.text(), "betaalpha beta gamma");
        let mut d = doc("one\ntwo\nthree");
        d.select(5, 5);
        assert!(d.move_paragraphs(true));
        assert_eq!(d.text(), "two\none\nthree");
        assert_eq!(d.range(), (1, 1), "the caret moves with its paragraph");
        assert!(!d.move_paragraphs(true));
        d.select(0, 5);
        assert!(d.move_paragraphs(false));
        assert_eq!(d.text(), "three\ntwo\none");
        let d = doc("First one. Second one! Third?");
        assert_eq!(d.sentence_at(14), (11, 23));
    }

    #[test]
    fn graphemes_direction_and_alignment() {
        // "e" + combining acute, a family emoji ZWJ sequence, then Arabic with a fatha.
        let mut d = doc("e\u{301}x\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}y\u{628}\u{64E}");
        assert_eq!(d.next_grapheme(0, true), 2);
        assert_eq!(d.next_grapheme(3, true), 8, "the ZWJ family is one cluster");
        assert_eq!(d.next_grapheme(8, false), 3);
        d.select(8, 8);
        d.delete(false, false);
        assert_eq!(d.text(), "e\u{301}xy\u{628}\u{64E}", "Backspace removes the whole emoji");
        let end = d.len();
        d.select(end, end);
        d.delete(false, false);
        assert_eq!(d.text(), "e\u{301}xy\u{628}", "only the Arabic vowel mark is removed");
        // Direction: auto from the first strong character, then explicit.
        let mut d = doc("مرحبا world\nhello");
        assert!(d.paragraph_rtl(&d.paragraphs[0]));
        assert!(!d.paragraph_rtl(&d.paragraphs[1]));
        d.select(13, 13);
        d.set_paragraph_direction(true);
        assert!(d.paragraph_rtl(&d.paragraphs[1]));
        // Align Left in a right-to-left paragraph stores "end" (Right).
        d.set_visual_alignment(Alignment::Left);
        assert_eq!(d.para_format(&d.paragraphs[1]).alignment, Alignment::Right);
        assert_eq!(d.visual_alignment(), Alignment::Left);
    }

    #[test]
    fn bookmarks_and_cross_references() {
        let mut d = doc("Introduction\nSee the section.");
        d.select(0, 12);
        assert!(d.add_bookmark("1bad").is_err());
        d.add_bookmark("Intro").unwrap();
        assert_eq!(d.bookmarks(), vec![("Intro".to_string(), 0, 12)]);
        assert_eq!(d.bookmark_text("Intro").as_deref(), Some("Introduction"));
        // Edits before move it.
        d.select(0, 0); d.replace("1. ");
        assert_eq!(d.bookmarks()[0].1..d.bookmarks()[0].2, 3..15);
        d.select(20, 20);
        d.insert_cross_reference("Intro", false);
        assert!(d.has_reference_fields());
        let h = d.heading_bookmark(0);
        assert!(h.starts_with("_Ref"));
        assert_eq!(d.heading_bookmark(0), h, "a heading's bookmark is reused");
        d.delete_bookmark("Intro");
        assert_eq!(d.bookmarks().len(), 1);
    }

    #[test]
    fn fragment_round_trip() {
        let mut d = doc("Hello\nWorld");
        d.select(3, 8);
        let f = d.fragment();
        assert_eq!(f.len(), 2);
        d.select(11, 11);
        d.paste_fragment(f);
        assert_eq!(d.text(), "Hello\nWorldlo\nWo");
    }
}
