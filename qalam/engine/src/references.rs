//! References: tables of contents and figures, captions (SEQ), citations and bibliographies
//! (sources in customXml), the index (XE) and the table of authorities (TA). Generated tables
//! are field blocks — a FieldBegin marker in their first paragraph, FieldEnd in their last — so
//! Word sees real TOC/INDEX/TOA/BIBLIOGRAPHY fields and they can be updated.

use crate::format::{Leader, ParaProps, RunProps, TabKind, TabStop};
use crate::model::{Document, EditKind, Field, Marker, Paragraph, Run};
use crate::styles::{Style, StyleKind};
use crate::Color;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Sources and citation styles

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SourceKind { #[default] Book, JournalArticle, Website, Report, Case, Misc }

impl SourceKind {
    pub fn ooxml(&self) -> &'static str { match self { SourceKind::Book => "Book", SourceKind::JournalArticle => "JournalArticle", SourceKind::Website => "InternetSite", SourceKind::Report => "Report", SourceKind::Case => "Case", SourceKind::Misc => "Misc" } }
    pub fn from_ooxml(v: &str) -> SourceKind { match v { "JournalArticle" | "ArticleInAPeriodical" => SourceKind::JournalArticle, "InternetSite" | "DocumentFromInternetSite" => SourceKind::Website, "Report" => SourceKind::Report, "Case" => SourceKind::Case, "Book" | "BookSection" => SourceKind::Book, _ => SourceKind::Misc } }
    pub const ALL: [(SourceKind, &'static str); 6] = [(SourceKind::Book, "Book"), (SourceKind::JournalArticle, "Journal Article"), (SourceKind::Website, "Web Site"), (SourceKind::Report, "Report"), (SourceKind::Case, "Case"), (SourceKind::Misc, "Miscellaneous")];
}

/// A bibliography source. Authors are "Last, First" entries separated by ";".
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Source {
    pub tag: String, pub kind: SourceKind, pub authors: String, pub title: String, pub year: String,
    pub publisher: String, pub city: String, pub journal: String, pub volume: String, pub issue: String, pub pages: String, pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CitationStyle { #[default] Apa, Mla, Chicago, Ieee, Harvard }

impl CitationStyle {
    pub const ALL: [(CitationStyle, &'static str); 5] = [(CitationStyle::Apa, "APA"), (CitationStyle::Mla, "MLA"), (CitationStyle::Chicago, "Chicago"), (CitationStyle::Ieee, "IEEE"), (CitationStyle::Harvard, "Harvard - Anglia")];
    pub fn name(&self) -> &'static str { Self::ALL.iter().find(|s| s.0 == *self).map(|s| s.1).unwrap_or("APA") }
    pub fn from_name(n: &str) -> CitationStyle { Self::ALL.iter().find(|s| n.to_ascii_uppercase().starts_with(&s.1.to_ascii_uppercase()[..3.min(s.1.len())])).map(|s| s.0).unwrap_or_default() }
    /// The bibliography's heading in this style.
    pub fn heading(&self) -> &'static str { match self { CitationStyle::Mla => "Works Cited", CitationStyle::Apa | CitationStyle::Ieee => "References", _ => "Bibliography" } }
}

impl Source {
    /// (last, first) names.
    pub fn names(&self) -> Vec<(String, String)> {
        self.authors.split(';').map(str::trim).filter(|a| !a.is_empty()).map(|a| match a.split_once(',') { Some((l, f)) => (l.trim().to_string(), f.trim().to_string()), None => match a.rsplit_once(' ') { Some((f, l)) => (l.to_string(), f.to_string()), None => (a.to_string(), String::new()) } }).collect()
    }
    fn initials(first: &str) -> String { first.split_whitespace().map(|w| format!("{}.", w.chars().next().unwrap_or(' '))).collect::<Vec<_>>().join(" ") }
    /// Authors as an in-text citation names them.
    fn cite_names(&self, style: CitationStyle) -> String {
        let n = self.names();
        let and = if style == CitationStyle::Apa { "&" } else { "and" };
        match n.len() {
            0 => self.title.clone(),
            1 => n[0].0.clone(),
            2 => format!("{} {and} {}", n[0].0, n[1].0),
            _ => format!("{} et al.", n[0].0),
        }
    }
}

/// The text of a citation in the document's style; IEEE numbers by first citation.
pub fn citation_text(doc: &Document, tag: &str, numbers: &HashMap<String, usize>) -> String {
    let Some(s) = doc.sources.iter().find(|s| s.tag == tag) else { return format!("({tag})") };
    let style = doc.citation_style;
    let year = if s.year.is_empty() { "n.d.".to_string() } else { s.year.clone() };
    match style {
        CitationStyle::Apa => format!("({}, {year})", s.cite_names(style)),
        CitationStyle::Mla => format!("({})", s.cite_names(style)),
        CitationStyle::Chicago | CitationStyle::Harvard => format!("({} {year})", s.cite_names(style)),
        CitationStyle::Ieee => format!("[{}]", numbers.get(tag).copied().unwrap_or(1)),
    }
}

/// Citation order: each tag's number by first citation in the document (IEEE).
pub fn citation_numbers(doc: &Document) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for r in doc.paragraphs.iter().flat_map(|p| p.runs.iter()) {
        if let Some(Field::Citation(t)) = &r.field { let n = out.len() + 1; out.entry(t.clone()).or_insert(n); }
    }
    out
}

/// A bibliography entry as runs (titles in italics), in the document's style.
fn bibliography_runs(style: CitationStyle, s: &Source, n: usize) -> Vec<Run> {
    let names = s.names();
    let year = if s.year.is_empty() { "n.d.".to_string() } else { s.year.clone() };
    let text = |t: String| Run::text(t);
    let italic = |t: String| Run { text: t, props: RunProps { italic: Some(true), ..Default::default() }, ..Default::default() };
    let join = |v: Vec<String>, last: &str| match v.len() { 0 => String::new(), 1 => v[0].clone(), _ => format!("{}, {last} {}", v[..v.len() - 1].join(", "), v[v.len() - 1]) };
    let place = [s.city.as_str(), s.publisher.as_str()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(": ");
    let mut runs = vec![];
    match style {
        CitationStyle::Apa | CitationStyle::Harvard => {
            let authors = join(names.iter().map(|(l, f)| format!("{l}, {}", Source::initials(f))).collect(), "&");
            runs.push(text(if style == CitationStyle::Apa { format!("{authors} ({year}). ") } else { format!("{authors} ({year}) ") }));
            match s.kind {
                SourceKind::JournalArticle => {
                    runs.push(text(format!("{}. ", s.title)));
                    runs.push(italic(format!("{}{}", s.journal, if s.volume.is_empty() { String::new() } else { format!(", {}", s.volume) })));
                    runs.push(text(format!("{}{}.", if s.issue.is_empty() { String::new() } else { format!("({})", s.issue) }, if s.pages.is_empty() { String::new() } else { format!(", {}", s.pages) })));
                }
                SourceKind::Website => { runs.push(italic(format!("{}. ", s.title))); if !s.url.is_empty() { runs.push(text(format!("Retrieved from {}", s.url))); } }
                _ => { runs.push(italic(format!("{}. ", s.title))); if !place.is_empty() { runs.push(text(format!("{place}."))); } }
            }
        }
        CitationStyle::Mla => {
            let authors = match names.len() { 0 => String::new(), 1 => format!("{}, {}", names[0].0, names[0].1), 2 => format!("{}, {}, and {} {}", names[0].0, names[0].1, names[1].1, names[1].0), _ => format!("{}, {}, et al", names[0].0, names[0].1) };
            if !authors.is_empty() { runs.push(text(format!("{authors}. "))); }
            match s.kind {
                SourceKind::JournalArticle => { runs.push(text(format!("\u{201C}{}.\u{201D} ", s.title))); runs.push(italic(s.journal.clone())); runs.push(text(format!(", vol. {}, {year}, pp. {}.", s.volume, s.pages))); }
                SourceKind::Website => { runs.push(italic(format!("{}. ", s.title))); runs.push(text(format!("{year}, {}.", s.url))); }
                _ => { runs.push(italic(format!("{}. ", s.title))); runs.push(text(format!("{}, {year}.", s.publisher))); }
            }
        }
        CitationStyle::Chicago => {
            let authors = join(names.iter().enumerate().map(|(k, (l, f))| if k == 0 { format!("{l}, {f}") } else { format!("{f} {l}") }).collect(), "and");
            runs.push(text(format!("{authors}. {year}. ")));
            match s.kind {
                SourceKind::JournalArticle => { runs.push(text(format!("\u{201C}{}.\u{201D} ", s.title))); runs.push(italic(s.journal.clone())); runs.push(text(format!(" {}: {}.", s.volume, s.pages))); }
                _ => { runs.push(italic(format!("{}. ", s.title))); if !place.is_empty() { runs.push(text(format!("{place}."))); } }
            }
        }
        CitationStyle::Ieee => {
            let authors = join(names.iter().map(|(l, f)| format!("{} {l}", Source::initials(f))).collect(), "and");
            runs.push(text(format!("[{n}]\t{authors}, ")));
            match s.kind {
                SourceKind::JournalArticle => { runs.push(text(format!("\u{201C}{},\u{201D} ", s.title))); runs.push(italic(s.journal.clone())); runs.push(text(format!(", vol. {}, pp. {}, {year}.", s.volume, s.pages))); }
                _ => { runs.push(italic(format!("{}. ", s.title))); runs.push(text(format!("{}{year}.", if place.is_empty() { String::new() } else { format!("{place}, ") }))); }
            }
        }
    }
    runs
}

// ---------------------------------------------------------------------------
// SEQ numbering

/// Each SEQ field's number by document position: captions count per label in document order.
pub fn seq_numbers(doc: &Document) -> HashMap<usize, u32> {
    let mut counters: HashMap<String, u32> = HashMap::new();
    let mut out = HashMap::new();
    let mut pos = 0;
    for p in &doc.paragraphs {
        for r in &p.runs {
            if let Some(Field::Seq(label)) = &r.field { let c = counters.entry(label.to_lowercase()).or_insert(0); *c += 1; out.insert(pos, *c); }
            pos += r.len();
        }
        pos += 1;
    }
    out
}

pub const CAPTION_LABELS: &[&str] = &["Figure", "Table", "Equation"];

pub const AUTHORITY_CATEGORIES: &[&str] = &["Cases", "Statutes", "Other Authorities", "Rules", "Treatises", "Regulations", "Constitutional Provisions"];

/// Options for Insert ▸ Table of Contents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TocOptions { pub levels: u8, pub page_numbers: bool, pub right_align: bool, pub leader: Leader, pub hyperlinks: bool, pub title: bool, pub manual: bool }

impl Default for TocOptions { fn default() -> Self { TocOptions { levels: 3, page_numbers: true, right_align: true, leader: Leader::Dot, hyperlinks: true, title: true, manual: false } } }

/// A field block's paragraphs: (first, last, instruction).
pub fn blocks(doc: &Document) -> Vec<(usize, usize, String)> {
    let mut out = vec![];
    let mut open: Option<(usize, String)> = None;
    for (i, p) in doc.paragraphs.iter().enumerate() {
        for r in &p.runs {
            match &r.marker {
                Some(Marker::FieldBegin(instr)) => open = Some((i, instr.clone())),
                Some(Marker::FieldEnd) => if let Some((s, instr)) = open.take() { out.push((s, i, instr)); },
                _ => {}
            }
        }
    }
    out
}

fn kind_of(instr: &str) -> String { instr.split_whitespace().next().unwrap_or("").to_uppercase() }

/// The text a SEQ-numbered caption shows (for tables of figures).
fn shown_text(doc: &Document, i: usize, seq: &HashMap<usize, u32>) -> String {
    let mut out = String::new();
    let mut pos = doc.paragraph_start(i);
    for r in &doc.paragraphs[i].runs {
        match &r.field { Some(Field::Seq(_)) => out += &seq.get(&pos).map(|n| n.to_string()).unwrap_or_default(), Some(_) => {}, None => if r.marker.is_none() && r.object.is_none() && r.image.is_none() { out += &r.text } }
        pos += r.len();
    }
    out.trim().to_string()
}

impl Document {
    fn content_width_twips(&self) -> i32 { self.page.width - self.page.left - self.page.right - self.page.gutter }

    /// Create a built-in paragraph style when the document lacks it.
    fn ensure_style(&mut self, id: &str, name: &str, para: ParaProps, run: RunProps) {
        if self.styles.styles.contains_key(id) { return; }
        self.styles.styles.insert(id.into(), Style { id: id.into(), name: name.into(), kind: StyleKind::Paragraph, based_on: Some("Normal".into()), next: Some("Normal".into()), para, run, table_borders: false });
        self.styles.order.push(id.into());
        self.style_edits.insert(id.into());
        self.styles_changed = true;
        self.format_epoch += 1;
    }

    /// A paragraph's hidden bookmark with this prefix (_Toc for tables, _Ref for references).
    pub fn ensure_bookmark(&mut self, index: usize, prefix: &str) -> String {
        let s = self.paragraph_start(index);
        let e = s + self.paragraphs[index].len();
        if let Some(b) = self.bookmarks().into_iter().find(|b| b.0.starts_with(prefix) && b.1 == s && b.2 == e) { return b.0; }
        self.range_bookmark(s, e, prefix)
    }

    /// A hidden bookmark around [s, e).
    pub fn range_bookmark(&mut self, s: usize, e: usize, prefix: &str) -> String {
        if let Some(b) = self.bookmarks().into_iter().find(|b| b.0.starts_with(prefix) && b.1 == s && b.2 == e) { return b.0; }
        let names: Vec<String> = self.bookmarks().into_iter().map(|b| b.0).collect();
        let mut n = 200_000 + (s as u64 * 13 + e as u64) % 700_000;
        while names.contains(&format!("{prefix}{n}")) { n += 1; }
        let name = format!("{prefix}{n}");
        self.insert_marker_pair(s, e, Marker::BookmarkStart(name.clone()), Marker::BookmarkEnd(name.clone()));
        name
    }

    /// Paragraphs (excluding field blocks) that a TOC lists: (index, level 0-based, text).
    fn toc_sources(&self, levels: u8) -> Vec<(usize, u8, String)> {
        let inside: Vec<(usize, usize)> = blocks(self).into_iter().map(|b| (b.0, b.1)).collect();
        self.outline().into_iter().filter(|(i, l, _)| *l < levels && !inside.iter().any(|(a, b)| i >= a && i <= b) && self.paragraphs[*i].style.as_deref() != Some("TOCHeading")).collect()
    }

    /// Entry paragraphs for a TOC or table of figures: text, leader tab, page number field,
    /// all linked to a bookmark on the source paragraph.
    fn entry_paragraph(&mut self, source: usize, text: String, style: &str, o: &TocOptions) -> Paragraph {
        let bm = self.ensure_bookmark(source, "_Toc");
        let link = o.hyperlinks.then(|| format!("#{bm}"));
        let mut runs = vec![Run { text, link: link.clone(), ..Default::default() }];
        if o.page_numbers {
            runs.push(Run { text: if o.right_align { "\t".into() } else { " ".into() }, link: link.clone(), ..Default::default() });
            runs.push(Run { text: " ".into(), field: Some(Field::PageRef(bm)), link, ..Default::default() });
        }
        let mut p = Paragraph { runs, style: Some(style.into()), ..Default::default() };
        if o.page_numbers && o.right_align { p.props.tabs = Some(vec![TabStop { pos: self.content_width_twips(), kind: TabKind::Right, leader: o.leader }]); }
        p
    }

    /// Wrap entries in a field block.
    fn block(instr: &str, mut paras: Vec<Paragraph>, empty: &str) -> Vec<Paragraph> {
        if paras.is_empty() { paras.push(Paragraph::new(empty)); }
        paras[0].runs.insert(0, Run::marker(Marker::FieldBegin(instr.to_string())));
        paras.last_mut().unwrap().runs.push(Run::marker(Marker::FieldEnd));
        paras
    }

    /// Replace paragraphs [a, b] (or insert at a when b < a) with `paras`.
    fn splice_paragraphs(&mut self, a: usize, b: Option<usize>, paras: Vec<Paragraph>) {
        let cell = None;
        let paras: Vec<Paragraph> = paras.into_iter().map(|mut p| { p.cell = cell; p }).collect();
        match b { Some(b) => { self.paragraphs.splice(a..=b, paras); } None => { self.paragraphs.splice(a..a, paras); } }
        self.format_epoch += 1;
    }

    /// The paragraph index where a block is inserted: before the caret's paragraph, or after
    /// it when that paragraph has text and the caret is at its end.
    fn insertion_index(&mut self) -> usize {
        let (i, o) = self.locate(self.range().0);
        if self.paragraphs[i].cell.is_some() { return self.paragraphs.iter().rposition(|p| p.cell.is_none()).map(|k| k + 1).unwrap_or(self.paragraphs.len()); }
        if self.paragraphs[i].is_empty() { return i; }
        if o >= self.paragraphs[i].len() { i + 1 } else { i }
    }

    fn toc_styles(&mut self, levels: u8) {
        for l in 1..=levels.max(1) {
            self.ensure_style(&format!("TOC{l}"), &format!("toc {l}"), ParaProps { indent_left: Some((l as i32 - 1) * 220), space_after: Some(5.0), ..Default::default() }, RunProps::default());
        }
        self.ensure_style("TOCHeading", "TOC Heading", ParaProps { space_before: Some(12.0), space_after: Some(6.0), outline_level: Some(9), keep_next: Some(true), ..Default::default() }, RunProps { size: Some(16.0), color: Some(Some(Color(0x2F, 0x54, 0x96))), ..Default::default() });
    }

    fn toc_entries(&mut self, o: &TocOptions) -> Vec<Paragraph> {
        let sources = self.toc_sources(o.levels);
        let mut out = vec![];
        for (i, level, text) in sources {
            if o.manual {
                let mut p = Paragraph { runs: vec![Run::text(text), Run::text("\t"), Run::text("1")], style: Some(format!("TOC{}", level + 1)), ..Default::default() };
                p.props.tabs = Some(vec![TabStop { pos: self.content_width_twips(), kind: TabKind::Right, leader: o.leader }]);
                out.push(p);
            } else {
                let style = format!("TOC{}", level + 1);
                out.push(self.entry_paragraph(i, text, &style, o));
            }
        }
        out
    }

    fn toc_instr(o: &TocOptions) -> String {
        format!("TOC \\o \"1-{}\"{}{} \\u{}", o.levels, if o.hyperlinks { " \\h" } else { "" }, " \\z", if o.page_numbers { "" } else { " \\n" })
    }

    /// Insert a table of contents at the caret (a title, then the TOC field).
    pub fn insert_toc(&mut self, o: TocOptions) {
        self.checkpoint(EditKind::Other);
        self.toc_styles(o.levels);
        let at = self.insertion_index();
        let entries = self.toc_entries(&o);
        let mut paras = if o.manual { entries } else { Self::block(&Self::toc_instr(&o), entries, "No table of contents entries found.") };
        if o.title { paras.insert(0, Paragraph { runs: vec![Run::text("Contents")], style: Some("TOCHeading".into()), ..Default::default() }); }
        let n = paras.len();
        self.splice_paragraphs(at, None, paras);
        let s = self.paragraph_start((at + n).min(self.paragraphs.len() - 1));
        self.select(s, s);
        self.changed();
    }

    /// Remove the table of contents (and its title).
    pub fn remove_toc(&mut self) {
        let Some((a, b, _)) = blocks(self).into_iter().find(|b| kind_of(&b.2) == "TOC" && !b.2.contains("\\c")) else { return };
        self.checkpoint(EditKind::Other);
        let a = if a > 0 && self.paragraphs[a - 1].style.as_deref() == Some("TOCHeading") { a - 1 } else { a };
        self.paragraphs.drain(a..=b);
        if self.paragraphs.is_empty() { self.paragraphs.push(Paragraph::new("")); }
        let s = self.paragraph_start(a.min(self.paragraphs.len() - 1));
        self.select(s, s);
        self.format_epoch += 1;
        self.changed();
    }

    /// Insert a caption: "Figure N" (a SEQ field) and text, below or above the selected
    /// picture, object, equation or table, or as its own paragraph at the caret.
    pub fn insert_caption(&mut self, label: &str, text: &str, above: bool) {
        self.checkpoint(EditKind::Other);
        self.ensure_style("Caption", "caption", ParaProps { space_after: Some(10.0), ..Default::default() }, RunProps { italic: Some(true), size: Some(9.0), color: Some(Some(Color(0x44, 0x54, 0x6A))), ..Default::default() });
        let (i, _) = self.locate(self.range().0);
        // In a table: next to the whole table.
        let (first, last) = match self.paragraphs[i].cell.and_then(|c| self.table_of_cell(c)) {
            Some(t) => {
                let ids: Vec<u64> = self.tables[t].rows.iter().flat_map(|r| r.cells.iter().map(|c| c.id)).collect();
                let f = self.paragraphs.iter().position(|p| p.cell.is_some_and(|c| ids.contains(&c))).unwrap_or(i);
                let l = self.paragraphs.iter().rposition(|p| p.cell.is_some_and(|c| ids.contains(&c))).unwrap_or(i);
                (f, l)
            }
            None => (i, i),
        };
        let at = if above { first } else { last + 1 };
        let mut runs = vec![Run::text(format!("{label} ")), Run { text: " ".into(), field: Some(Field::Seq(label.to_string())), ..Default::default() }];
        if !text.is_empty() { runs.push(Run::text(text.to_string())); }
        let p = Paragraph { runs, style: Some("Caption".into()), props: ParaProps { keep_next: above.then_some(true), ..Default::default() }, ..Default::default() };
        self.paragraphs.insert(at, p);
        let e = self.paragraph_start(at) + self.paragraphs[at].len();
        self.select(e, e);
        self.format_epoch += 1;
        self.changed();
    }

    fn figure_entries(&mut self, label: &str, o: &TocOptions) -> Vec<Paragraph> {
        let seq = seq_numbers(self);
        let inside: Vec<(usize, usize)> = blocks(self).into_iter().map(|b| (b.0, b.1)).collect();
        let captions: Vec<usize> = (0..self.paragraphs.len()).filter(|&i| !inside.iter().any(|(a, b)| i >= *a && i <= *b) && self.paragraphs[i].runs.iter().any(|r| matches!(&r.field, Some(Field::Seq(l)) if l.eq_ignore_ascii_case(label)))).collect();
        let mut out = vec![];
        for i in captions { let text = shown_text(self, i, &seq); out.push(self.entry_paragraph(i, text, "TableofFigures", o)); }
        out
    }

    /// Insert a table of figures (or tables, equations) for a caption label.
    pub fn insert_table_of_figures(&mut self, label: &str, o: TocOptions) {
        self.checkpoint(EditKind::Other);
        self.ensure_style("TableofFigures", "table of figures", ParaProps { space_after: Some(0.0), ..Default::default() }, RunProps::default());
        let at = self.insertion_index();
        let entries = self.figure_entries(label, &o);
        let paras = Self::block(&format!("TOC \\h \\z \\c \"{label}\""), entries, "No table of figures entries found.");
        self.splice_paragraphs(at, None, paras);
        self.changed();
    }

    // -- citations and bibliography --

    pub fn add_source(&mut self, s: Source) {
        self.checkpoint(EditKind::Other);
        match self.sources.iter_mut().find(|x| x.tag == s.tag) { Some(x) => *x = s, None => self.sources.push(s) }
        self.sources_changed = true;
        self.format_epoch += 1;
        self.changed();
    }
    pub fn remove_source(&mut self, tag: &str) {
        self.checkpoint(EditKind::Other);
        self.sources.retain(|s| s.tag != tag);
        self.sources_changed = true;
        self.format_epoch += 1;
        self.changed();
    }
    /// A tag for a new source: author's last name, two-digit year, then a letter if taken.
    pub fn new_source_tag(&self, s: &Source) -> String {
        let last: String = s.names().first().map(|n| n.0.clone()).unwrap_or_else(|| s.title.clone()).chars().filter(|c| c.is_alphanumeric()).take(3).collect();
        let year: String = s.year.chars().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect();
        let base = format!("{last}{year}");
        let mut tag = if base.is_empty() { "Src".into() } else { base.clone() };
        let mut k = 0u8;
        while self.sources.iter().any(|x| x.tag == tag) { tag = format!("{base}{}", (b'a' + k) as char); k += 1; }
        tag
    }
    pub fn set_citation_style(&mut self, style: CitationStyle) {
        self.checkpoint(EditKind::Other);
        self.citation_style = style;
        self.sources_changed = true;
        self.format_epoch += 1;
        self.update_blocks_except_index();
        self.changed();
    }

    fn bibliography_entries(&self) -> Vec<Paragraph> {
        let style = self.citation_style;
        let numbers = citation_numbers(self);
        let mut list: Vec<&Source> = self.sources.iter().collect();
        if style == CitationStyle::Ieee { list.sort_by_key(|s| numbers.get(&s.tag).copied().unwrap_or(usize::MAX)); }
        else { list.sort_by_key(|s| s.names().first().map(|n| n.0.to_lowercase()).unwrap_or_else(|| s.title.to_lowercase())); }
        list.iter().enumerate().map(|(k, s)| {
            let mut p = Paragraph { runs: bibliography_runs(style, s, numbers.get(&s.tag).copied().unwrap_or(k + 1)), style: Some("Bibliography".into()), ..Default::default() };
            if style == CitationStyle::Ieee { p.props.tabs = Some(vec![TabStop { pos: 567, kind: TabKind::Left, leader: Leader::None }]); p.props.indent_left = Some(567); p.props.first_line = Some(-567); }
            p
        }).collect()
    }

    /// Insert the bibliography (a heading named for the style, then the BIBLIOGRAPHY field).
    pub fn insert_bibliography(&mut self, heading: Option<&str>) {
        self.checkpoint(EditKind::Other);
        self.ensure_style("Bibliography", "Bibliography", ParaProps { indent_left: Some(720), first_line: Some(-720), ..Default::default() }, RunProps::default());
        let at = self.insertion_index();
        let mut paras = Self::block("BIBLIOGRAPHY", self.bibliography_entries(), "There are no sources in the current document.");
        if let Some(h) = heading { paras.insert(0, Paragraph { runs: vec![Run::text(h)], style: Some("Heading1".into()), ..Default::default() }); }
        self.splice_paragraphs(at, None, paras);
        self.changed();
    }

    // -- index --

    /// Mark an index entry ("Main" or "Main:Sub") after the selection.
    pub fn mark_index_entry(&mut self, entry: &str) {
        let e = self.range().1;
        self.select(e, e);
        self.insert_field(Field::IndexEntry(entry.to_string()));
    }

    /// Mark every occurrence of `text` (whole words, case-sensitive) as an index entry.
    pub fn mark_all_index_entries(&mut self, text: &str, entry: &str) -> usize {
        if text.is_empty() { return 0; }
        let mut marks = vec![];
        let mut pos = 0;
        let inside: Vec<(usize, usize)> = blocks(self).into_iter().map(|b| (b.0, b.1)).collect();
        let saved = self.selection;
        for (i, p) in self.paragraphs.iter().enumerate() {
            // Generated tables (TOC, index…) are skipped.
            if inside.iter().any(|(a, b)| i >= *a && i <= *b) { pos += p.len() + 1; continue; }
            let t: Vec<char> = p.text().chars().collect();
            let w: Vec<char> = text.chars().collect();
            let mut k = 0;
            while k + w.len() <= t.len() {
                let before = k == 0 || !t[k - 1].is_alphanumeric();
                let after = k + w.len() == t.len() || !t[k + w.len()].is_alphanumeric();
                if before && after && t[k..k + w.len()] == w[..] { marks.push(pos + k + w.len()); k += w.len(); } else { k += 1; }
            }
            pos += p.len() + 1;
        }
        let n = marks.len();
        let before = marks.iter().filter(|m| **m <= saved.focus.min(saved.anchor)).count();
        for at in marks.into_iter().rev() { self.select(at, at); self.insert_field(Field::IndexEntry(entry.to_string())); }
        // The selection stays on the same text (each mark before it adds one position).
        self.selection = crate::model::Selection { anchor: saved.anchor + before, focus: saved.focus + before };
        n
    }

    /// Page numbers (1-based) of fields matching `f`, from a fresh layout.
    fn field_pages(&self, mut f: impl FnMut(&Field) -> Option<String>) -> Vec<(String, usize)> {
        let layout = crate::layout_document(self);
        let mut out = vec![];
        let mut pos = 0;
        for p in &self.paragraphs {
            for r in &p.runs {
                if let Some(key) = r.field.as_ref().and_then(&mut f) { out.push((key, layout.page_of(pos) + 1)); }
                pos += r.len();
            }
            pos += 1;
        }
        out
    }

    fn index_entries(&self) -> Vec<Paragraph> {
        let found = self.field_pages(|f| if let Field::IndexEntry(e) = f { Some(e.clone()) } else { None });
        let mut map: std::collections::BTreeMap<(String, String), (String, String, Vec<usize>)> = Default::default();
        for (e, page) in found {
            let (main, sub) = match e.split_once(':') { Some((m, s)) => (m.trim().to_string(), s.trim().to_string()), None => (e.trim().to_string(), String::new()) };
            let ent = map.entry((main.to_lowercase(), sub.to_lowercase())).or_insert((main, sub, vec![]));
            if !ent.2.contains(&page) { ent.2.push(page); }
        }
        let mut out = vec![];
        let mut last_main = String::new();
        let mut last_letter = None;
        let pages = |v: &Vec<usize>| { let mut v = v.clone(); v.sort(); v.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ") };
        for ((key, _), (main, sub, pg)) in &map {
            let letter = key.chars().next().map(|c| c.to_ascii_uppercase());
            if letter != last_letter {
                last_letter = letter;
                out.push(Paragraph { runs: vec![Run::text(letter.map(|c| c.to_string()).unwrap_or_default())], style: Some("IndexHeading".into()), ..Default::default() });
            }
            if sub.is_empty() {
                out.push(Paragraph { runs: vec![Run::text(format!("{main}, {}", pages(pg)))], style: Some("Index1".into()), ..Default::default() });
            } else {
                if !main.eq_ignore_ascii_case(&last_main) { out.push(Paragraph { runs: vec![Run::text(main.clone())], style: Some("Index1".into()), ..Default::default() }); }
                out.push(Paragraph { runs: vec![Run::text(format!("{sub}, {}", pages(pg)))], style: Some("Index2".into()), ..Default::default() });
            }
            last_main = main.clone();
        }
        out
    }

    /// Insert the index in its own continuous section with `columns` columns.
    pub fn insert_index(&mut self, columns: u8) {
        self.checkpoint(EditKind::Other);
        self.ensure_style("Index1", "index 1", ParaProps { indent_left: Some(220), first_line: Some(-220), space_after: Some(0.0), ..Default::default() }, RunProps::default());
        self.ensure_style("Index2", "index 2", ParaProps { indent_left: Some(440), first_line: Some(-220), space_after: Some(0.0), ..Default::default() }, RunProps::default());
        self.ensure_style("IndexHeading", "index heading", ParaProps { space_before: Some(6.0), space_after: Some(0.0), keep_next: Some(true), ..Default::default() }, RunProps { bold: Some(true), ..Default::default() });
        let at = self.insertion_index();
        let mut paras = Self::block(&format!("INDEX \\e \"\t\" \\c \"{columns}\" \\z \"1033\""), self.index_entries(), "No index entries found.");
        if columns > 1 {
            use crate::page::{Columns, Section, SectionKind};
            // The section before the index ends here; the index is a continuous section in columns;
            // what follows continues on the same page.
            let k = self.section_index(at.min(self.paragraphs.len() - 1));
            let current = self.section_at(k).clone();
            if at > 0 && self.paragraphs[at - 1].section.is_none() && self.paragraphs[at - 1].cell.is_none() { self.paragraphs[at - 1].section = Some(current.clone()); }
            paras.last_mut().unwrap().section = Some(Section { kind: SectionKind::Continuous, columns: Columns { count: columns, ..Default::default() }, line_numbers: None });
            self.section_at_mut(k).kind = SectionKind::Continuous;
        }
        self.splice_paragraphs(at, None, paras);
        self.changed();
    }

    // -- table of authorities --

    pub fn mark_authority(&mut self, long: &str, short: &str, category: u8) {
        let e = self.range().1;
        self.select(e, e);
        self.insert_field(Field::Authority { long: long.into(), short: short.into(), category });
    }

    fn authority_entries(&self) -> Vec<Paragraph> {
        let found = self.field_pages(|f| if let Field::Authority { long, category, .. } = f { Some(format!("{category}\u{1}{long}")) } else { None });
        let mut by_cat: std::collections::BTreeMap<u8, std::collections::BTreeMap<String, Vec<usize>>> = Default::default();
        for (key, page) in found {
            let (cat, long) = key.split_once('\u{1}').unwrap_or(("1", &key));
            let pages = by_cat.entry(cat.parse().unwrap_or(1)).or_default().entry(long.to_string()).or_default();
            if !pages.contains(&page) { pages.push(page); }
        }
        let mut out = vec![];
        let w = self.content_width_twips();
        for (cat, entries) in by_cat {
            out.push(Paragraph { runs: vec![Run::text(AUTHORITY_CATEGORIES.get(cat.saturating_sub(1) as usize).copied().unwrap_or("Other Authorities"))], style: Some("TOAHeading".into()), ..Default::default() });
            for (long, mut pages) in entries {
                pages.sort();
                let shown = if pages.len() >= 5 { "passim".to_string() } else { pages.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ") };
                let mut p = Paragraph { runs: vec![Run::text(long), Run::text("\t"), Run::text(shown)], style: Some("TableofAuthorities".into()), ..Default::default() };
                p.props.tabs = Some(vec![TabStop { pos: w, kind: TabKind::Right, leader: Leader::Dot }]);
                out.push(p);
            }
        }
        out
    }

    pub fn insert_toa(&mut self) {
        self.checkpoint(EditKind::Other);
        self.ensure_style("TableofAuthorities", "table of authorities", ParaProps { indent_left: Some(220), first_line: Some(-220), ..Default::default() }, RunProps::default());
        self.ensure_style("TOAHeading", "toa heading", ParaProps { space_before: Some(12.0), keep_next: Some(true), ..Default::default() }, RunProps { bold: Some(true), size: Some(12.0), ..Default::default() });
        let at = self.insertion_index();
        let paras = Self::block("TOA \\h \\c \"1\" \\p", self.authority_entries(), "No table of authorities entries found.");
        self.splice_paragraphs(at, None, paras);
        self.changed();
    }

    // -- updating --

    fn regenerate(&mut self, instr: &str) -> Option<Vec<Paragraph>> {
        let k = kind_of(instr);
        let o = TocOptions { levels: instr.split("\\o \"1-").nth(1).and_then(|r| r.chars().next()).and_then(|c| c.to_digit(10)).map(|d| d as u8).unwrap_or(3), page_numbers: !instr.contains("\\n"), hyperlinks: instr.contains("\\h"), ..Default::default() };
        Some(match k.as_str() {
            "TOC" if instr.contains("\\c") => {
                let label = instr.split("\\c").nth(1).map(|r| r.trim().trim_matches('"').split('"').next().unwrap_or("").to_string()).unwrap_or_default();
                self.figure_entries(&label, &o)
            }
            "TOC" => { self.toc_styles(o.levels); self.toc_entries(&o) }
            "BIBLIOGRAPHY" => self.bibliography_entries(),
            "INDEX" => self.index_entries(),
            "TOA" => self.authority_entries(),
            _ => return None,
        })
    }

    fn update_matching(&mut self, keep: impl Fn(&str) -> bool) {
        // Last block first, so indices stay valid.
        for (a, b, instr) in blocks(self).into_iter().rev() {
            if !keep(&kind_of(&instr)) { continue; }
            let section = self.paragraphs[b].section.clone();
            if let Some(entries) = self.regenerate(&instr) {
                let mut paras = Self::block(&instr, entries, "No entries found.");
                paras.last_mut().unwrap().section = section;
                // Blocks may have moved while regenerating (bookmarks add markers, not paragraphs).
                self.splice_paragraphs(a, Some(b), paras);
            }
        }
    }

    fn update_blocks_except_index(&mut self) { self.update_matching(|k| k == "BIBLIOGRAPHY"); }

    /// Update every generated table: contents, figures, bibliography, index, authorities.
    pub fn update_tables(&mut self) {
        self.checkpoint(EditKind::Other);
        self.update_matching(|_| true);
        // A second pass settles page numbers that moved because the tables changed length.
        self.update_matching(|k| k == "INDEX" || k == "TOA");
        let e = self.range().0.min(self.len());
        self.select(e, e);
        self.changed();
    }

    /// Update only tables of one kind ("TOC", "INDEX", "TOA", "BIBLIOGRAPHY").
    pub fn update_tables_of(&mut self, kind: &str) {
        self.checkpoint(EditKind::Other);
        let k = kind.to_string();
        self.update_matching(|x| x == k);
        let e = self.range().0.min(self.len());
        self.select(e, e);
        self.changed();
    }

    /// Add Text: make the paragraph a heading of `level` (1–3) or remove it from the TOC (0).
    pub fn set_toc_level(&mut self, level: u8) {
        if level == 0 { self.set_paragraph_style("Normal"); self.set_para_props(ParaProps { outline_level: Some(9), ..Default::default() }); }
        else { self.set_paragraph_style(&format!("Heading{level}")); }
    }
}

// ---------------------------------------------------------------------------
// customXml sources (b:Sources)

fn esc(s: &str) -> String { let mut o = String::new(); crate::xml::escape_into(s, &mut o, false); o }

/// The sources part (customXml/itemN.xml) as Word writes it.
pub fn sources_xml(doc: &Document) -> String {
    let style = match doc.citation_style { CitationStyle::Apa => ("\\APASixthEditionOfficeOnline.xsl", "APA"), CitationStyle::Mla => ("\\MLASeventhEditionOfficeOnline.xsl", "MLA"), CitationStyle::Chicago => ("\\Chicago.xsl", "Chicago"), CitationStyle::Ieee => ("\\IEEE2006OfficeOnline.xsl", "IEEE"), CitationStyle::Harvard => ("\\HarvardAnglia2008OfficeOnline.xsl", "Harvard - Anglia") };
    let mut out = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><b:Sources SelectedStyle="{}" StyleName="{}" xmlns:b="http://schemas.openxmlformats.org/officeDocument/2006/bibliography" xmlns="http://schemas.openxmlformats.org/officeDocument/2006/bibliography">"#, esc(style.0), esc(style.1));
    for s in &doc.sources {
        let people: String = s.names().iter().map(|(l, f)| format!("<b:Person><b:Last>{}</b:Last><b:First>{}</b:First></b:Person>", esc(l), esc(f))).collect();
        let field = |tag: &str, v: &str| if v.is_empty() { String::new() } else { format!("<b:{tag}>{}</b:{tag}>", esc(v)) };
        out += &format!("<b:Source><b:Tag>{}</b:Tag><b:SourceType>{}</b:SourceType>{}{}{}{}{}{}{}{}{}{}</b:Source>",
            esc(&s.tag), s.kind.ooxml(),
            if people.is_empty() { String::new() } else { format!("<b:Author><b:Author><b:NameList>{people}</b:NameList></b:Author></b:Author>") },
            field("Title", &s.title), field("Year", &s.year), field("City", &s.city), field("Publisher", &s.publisher),
            field(if s.kind == SourceKind::JournalArticle { "JournalName" } else { "BookTitle" }, &s.journal), field("Volume", &s.volume), field("Issue", &s.issue), field("Pages", &s.pages), field("URL", &s.url));
    }
    out += "</b:Sources>";
    out
}

fn local<'a>(e: &'a crate::xml::Element) -> &'a str { e.name.rsplit(':').next().unwrap_or(&e.name) }
fn child_text(e: &crate::xml::Element, name: &str) -> String { e.elements().find(|c| local(c) == name).map(|c| c.text()).unwrap_or_default() }

/// Read sources from a customXml part (None when it isn't a b:Sources part).
pub fn read_sources(text: &str) -> Option<(Vec<Source>, CitationStyle)> {
    let root = crate::xml::parse(text).ok()?;
    if local(&root) != "Sources" { return None; }
    let style = CitationStyle::from_name(root.attr("StyleName").unwrap_or("APA"));
    let mut out = vec![];
    for s in root.elements().filter(|e| local(e) == "Source") {
        let mut authors = vec![];
        if let Some(a) = s.elements().find(|e| local(e) == "Author") {
            fn people(e: &crate::xml::Element, out: &mut Vec<String>) {
                for c in e.elements() { if local(c) == "Person" { out.push(format!("{}, {}", child_text(c, "Last"), child_text(c, "First")).trim_end_matches(", ").to_string()); } else { people(c, out); } }
            }
            people(a, &mut authors);
        }
        out.push(Source {
            tag: child_text(s, "Tag"), kind: SourceKind::from_ooxml(&child_text(s, "SourceType")), authors: authors.join("; "), title: child_text(s, "Title"), year: child_text(s, "Year"),
            publisher: child_text(s, "Publisher"), city: child_text(s, "City"), journal: { let j = child_text(s, "JournalName"); if j.is_empty() { child_text(s, "BookTitle") } else { j } },
            volume: child_text(s, "Volume"), issue: child_text(s, "Issue"), pages: child_text(s, "Pages"), url: child_text(s, "URL"),
        });
    }
    Some((out, style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Paragraph;

    fn para(text: &str, style: Option<&str>) -> Paragraph { Paragraph { runs: vec![Run::text(text)], style: style.map(String::from), ..Default::default() } }

    #[test]
    fn toc_captions_citations_index_and_authorities() {
        let body = "Body text that goes on for a while so the document has some length to it. ".repeat(30);
        let mut d = Document::new(vec![para("Introduction", Some("Heading1")), para(&body, None), para("Methods", Some("Heading1")), para("Setup", Some("Heading2")), para(&body, None), para("Results", Some("Heading1")), para(&body, None)]);
        // Table of contents at the top.
        d.select(0, 0);
        d.insert_toc(TocOptions::default());
        let bl = blocks(&d);
        assert_eq!(bl.len(), 1);
        assert!(bl[0].2.starts_with("TOC"));
        let toc: Vec<String> = (bl[0].0..=bl[0].1).map(|i| d.paragraphs[i].text()).collect();
        assert_eq!(toc.len(), 4, "{toc:?}");
        assert!(toc[2].starts_with("Setup") && d.paragraphs[bl[0].0 + 2].style.as_deref() == Some("TOC2"));
        // Captions number themselves; a table of figures lists them.
        let end = d.len();
        d.select(end, end);
        d.insert_caption("Figure", ": First chart", false);
        d.insert_caption("Figure", ": Second chart", false);
        let seq = seq_numbers(&d);
        let mut nums: Vec<u32> = seq.values().copied().collect();
        nums.sort();
        assert_eq!(nums, vec![1, 2]);
        let end = d.len();
        d.select(end, end);
        d.insert_table_of_figures("Figure", TocOptions::default());
        let tof = blocks(&d).into_iter().find(|b| b.2.contains("\\c")).unwrap();
        assert_eq!(d.paragraphs[tof.0].text().trim_start(), "Figure 1: First chart\t ");
        // Citations and a bibliography.
        d.add_source(Source { tag: "Smi20".into(), kind: SourceKind::Book, authors: "Smith, John; Lee, Ann".into(), title: "Writing Engines".into(), year: "2020".into(), publisher: "Acme".into(), city: "Boston".into(), ..Default::default() });
        d.select(5, 5);
        d.insert_field(Field::Citation("Smi20".into()));
        let nums = citation_numbers(&d);
        assert_eq!(citation_text(&d, "Smi20", &nums), "(Smith & Lee, 2020)");
        d.citation_style = CitationStyle::Ieee;
        assert_eq!(citation_text(&d, "Smi20", &nums), "[1]");
        d.citation_style = CitationStyle::Apa;
        let end = d.len();
        d.select(end, end);
        d.insert_bibliography(Some("References"));
        let bib = blocks(&d).into_iter().find(|b| b.2 == "BIBLIOGRAPHY").unwrap();
        assert!(d.paragraphs[bib.0].text().starts_with("Smith, J., & Lee, A. (2020). Writing Engines."), "{}", d.paragraphs[bib.0].text());
        // Index and table of authorities.
        d.select(3, 3);
        d.mark_index_entry("Engines");
        assert!(d.mark_all_index_entries("Methods", "Methods:Overview") >= 1);
        d.mark_authority("Smith v. Jones, 123 U.S. 456 (1990)", "Smith v. Jones", 1);
        let end = d.len();
        d.select(end, end);
        d.insert_index(2);
        let idx = blocks(&d).into_iter().find(|b| b.2.starts_with("INDEX")).unwrap();
        let idx_text: Vec<String> = (idx.0..=idx.1).map(|i| d.paragraphs[i].text()).collect();
        assert!(idx_text.iter().any(|t| t.starts_with("Engines, 1")) && idx_text.iter().any(|t| t.starts_with("Overview, ")), "{idx_text:?}");
        assert_eq!(d.paragraphs[idx.1].section.as_ref().map(|s| s.columns.count), Some(2), "the index is a two-column section");
        let end = d.len();
        d.select(end, end);
        d.insert_toa();
        let toa = blocks(&d).into_iter().find(|b| b.2.starts_with("TOA")).unwrap();
        assert_eq!(d.paragraphs[toa.0].text().trim(), "Cases");
        // A new heading appears after updating.
        d.paragraphs.push(para("Conclusion", Some("Heading1")));
        d.update_tables();
        let bl = blocks(&d).into_iter().find(|b| b.2.starts_with("TOC") && !b.2.contains("\\c")).unwrap();
        assert!(d.paragraphs[bl.1].text().starts_with("Conclusion"));
        // Everything survives saving as real fields.
        assert!(d.paragraphs.iter().flat_map(|p| p.runs.iter()).any(|r| matches!(&r.field, Some(Field::Authority { .. }))), "TA before saving");
        let bytes = crate::docx::write(&d).unwrap();
        let e = crate::docx::read(&bytes).unwrap();
        let kinds: Vec<String> = blocks(&e).into_iter().map(|b| kind_of(&b.2)).collect();
        assert_eq!(kinds, vec!["TOC", "TOC", "BIBLIOGRAPHY", "INDEX", "TOA"]);
        assert_eq!(e.sources.len(), 1);
        assert_eq!(e.sources[0].names().len(), 2);
        assert!(e.paragraphs.iter().flat_map(|p| p.runs.iter()).any(|r| matches!(&r.field, Some(Field::IndexEntry(x)) if x == "Engines")));
        assert!(e.paragraphs.iter().flat_map(|p| p.runs.iter()).any(|r| matches!(&r.field, Some(Field::Authority { category: 1, .. }))));
        assert_eq!(seq_numbers(&e).len(), 2);
        // Layout shows the citation and the caption numbers.
        let l = crate::layout_document(&e);
        let text: String = l.pages.iter().flat_map(|p| p.items.iter()).filter_map(|i| match i { crate::layout::DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| g.ch).collect::<String>()), _ => None }).collect();
        assert!(text.contains("(Smith & Lee, 2020)") && text.contains("Figure 2"));
    }
}
