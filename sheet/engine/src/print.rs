//! Printing (sheet-pdf.js, page-layout.js, print-pagination.js, pagebreaks.js, print-header.js,
//! print-borders.js): a sheet's page setup, cutting what prints into pages (automatic and manual
//! breaks, print titles, scaling and fit-to-pages), and drawing each page as Qalam's draw items so
//! word-engine's PDF writer (which embeds the bundled fonts) produces the file.
//!
//! Units: sheet sizes are pixels at 96 dpi (a column of 64 px is 2/3 inch on paper at 100%); page
//! setup lengths are inches, as SpreadsheetML stores them. Print areas and titles are the sheet's
//! `_xlnm.Print_Area` / `_xlnm.Print_Titles` defined names, as in Excel, so they move with
//! inserted rows and columns and save as they were read.

use crate::addr::{col_name, parse_range, quote_sheet, Pos, Range, MAX_COLS, MAX_ROWS};
use crate::book::{DefinedName, Sheet, Workbook};
use crate::style::{HAlign, LineStyle, Style, Underline, VAlign};
use crate::value::Value;
use crate::xlsx::xml::Element;
use std::collections::BTreeSet;
use word_engine::fonts::FontKey;
use word_engine::layout::{DrawItem, Effects, Glyph, Layout, PageBox};
use word_engine::Color;

#[path = "print_objects.rs"]
pub mod objects;

pub const PX_PER_INCH: f32 = 96.0;

/// Paper sizes by SpreadsheetML `paperSize` code: (code, name, width × height in inches, portrait).
pub const PAPERS: &[(u32, &str, f32, f32)] = &[
    (1, "Letter", 8.5, 11.0), (5, "Legal", 8.5, 14.0), (3, "Tabloid", 11.0, 17.0), (7, "Executive", 7.25, 10.5),
    (8, "A3", 297.0 / 25.4, 420.0 / 25.4), (9, "A4", 210.0 / 25.4, 297.0 / 25.4), (11, "A5", 148.0 / 25.4, 210.0 / 25.4),
    (12, "B4 (JIS)", 257.0 / 25.4, 364.0 / 25.4), (13, "B5 (JIS)", 182.0 / 25.4, 257.0 / 25.4),
    (20, "Envelope #10", 4.125, 9.5), (27, "Envelope DL", 110.0 / 25.4, 220.0 / 25.4),
];

/// Page margins in inches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Margins { pub left: f32, pub right: f32, pub top: f32, pub bottom: f32, pub header: f32, pub footer: f32 }

impl Margins {
    pub const NORMAL: Margins = Margins { left: 0.7, right: 0.7, top: 0.75, bottom: 0.75, header: 0.3, footer: 0.3 };
    pub const WIDE: Margins = Margins { left: 1.0, right: 1.0, top: 1.0, bottom: 1.0, header: 0.5, footer: 0.5 };
    pub const NARROW: Margins = Margins { left: 0.25, right: 0.25, top: 0.75, bottom: 0.75, header: 0.3, footer: 0.3 };
}

impl Default for Margins { fn default() -> Self { Margins::NORMAL } }

/// Header and footer text with Excel's codes (&L &C &R sections, &P &N &D &T &F &A &Z fields,
/// &B &I &U &S toggles, &"Font,Style", &nn size, &KRRGGBB color, && for &).
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderFooter {
    pub odd_header: String, pub odd_footer: String,
    pub even_header: String, pub even_footer: String,
    pub first_header: String, pub first_footer: String,
    pub different_odd_even: bool, pub different_first: bool,
    pub scale_with_doc: bool, pub align_with_margins: bool,
    /// Pictures shown by &G (hfpic.rs), saved in the sheet's legacyDrawingHF part.
    pub pictures: Vec<crate::hfpic::HfPicture>,
}

impl Default for HeaderFooter {
    fn default() -> Self {
        HeaderFooter { odd_header: String::new(), odd_footer: String::new(), even_header: String::new(), even_footer: String::new(), first_header: String::new(), first_footer: String::new(),
            different_odd_even: false, different_first: false, scale_with_doc: true, align_with_margins: true, pictures: vec![] }
    }
}

impl HeaderFooter {
    fn is_default(&self) -> bool { *self == HeaderFooter::default() }
    /// The header and footer for page `n` (1-based within the sheet).
    pub fn for_page(&self, n: u32) -> (&str, &str) {
        if self.different_first && n == 1 { (&self.first_header, &self.first_footer) }
        else if self.different_odd_even && n % 2 == 0 { (&self.even_header, &self.even_footer) }
        else { (&self.odd_header, &self.odd_footer) }
    }
}

/// How a sheet prints (pageSetup, pageMargins, printOptions, headerFooter, rowBreaks, colBreaks).
#[derive(Clone, Debug, PartialEq)]
pub struct PageSetup {
    /// SpreadsheetML paper size code (1 = Letter, 9 = A4).
    pub paper: u32,
    pub landscape: bool,
    /// Print scale in percent (10–400), used when not fitting to pages.
    pub scale: u32,
    /// Fit to pages: `fit_width` × `fit_height` pages (0 = as many as needed).
    pub fit: bool,
    pub fit_width: u32,
    pub fit_height: u32,
    pub first_page: Option<u32>,
    pub over_then_down: bool,
    pub black_and_white: bool,
    pub draft: bool,
    pub margins: Margins,
    pub gridlines: bool,
    pub headings: bool,
    pub center_h: bool,
    pub center_v: bool,
    pub header_footer: HeaderFooter,
    /// Manual breaks: a page starts at each of these rows / columns (0-based).
    pub row_breaks: BTreeSet<u32>,
    pub col_breaks: BTreeSet<u32>,
    /// The printer settings part's relationship id (kept from the file).
    pub printer_rel: Option<String>,
    /// pageSetup attributes this model doesn't edit (kept from the file).
    pub other: Vec<(String, String)>,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup { paper: 1, landscape: false, scale: 100, fit: false, fit_width: 1, fit_height: 1, first_page: None, over_then_down: false, black_and_white: false, draft: false,
            margins: Margins::NORMAL, gridlines: false, headings: false, center_h: false, center_v: false, header_footer: HeaderFooter::default(),
            row_breaks: BTreeSet::new(), col_breaks: BTreeSet::new(), printer_rel: None, other: vec![] }
    }
}

impl PageSetup {
    /// Paper width and height in inches, as oriented.
    pub fn paper_inches(&self) -> (f32, f32) {
        let (w, h) = PAPERS.iter().find(|p| p.0 == self.paper).map(|p| (p.2, p.3)).unwrap_or((8.5, 11.0));
        if self.landscape { (h, w) } else { (w, h) }
    }
    pub fn paper_name(&self) -> &'static str { PAPERS.iter().find(|p| p.0 == self.paper).map_or("Letter", |p| p.1) }

    // ---- SpreadsheetML ----

    /// Takes one worksheet element this model owns; false for others.
    pub fn read_xml(&mut self, name: &str, raw: &str) -> bool {
        if !matches!(name, "pageMargins" | "pageSetup" | "printOptions" | "headerFooter" | "rowBreaks" | "colBreaks") { return false; }
        let Ok(e) = Element::parse(raw) else { return true };
        let f = |k: &str, d: f32| e.attr_f64(k).map_or(d, |v| v as f32);
        match name {
            "pageMargins" => self.margins = Margins { left: f("left", 0.7), right: f("right", 0.7), top: f("top", 0.75), bottom: f("bottom", 0.75), header: f("header", 0.3), footer: f("footer", 0.3) },
            "printOptions" => {
                self.gridlines = e.flag("gridLines", false) && e.flag("gridLinesSet", true);
                self.headings = e.flag("headings", false);
                self.center_h = e.flag("horizontalCentered", false);
                self.center_v = e.flag("verticalCentered", false);
            }
            "pageSetup" => for (k, v) in &e.attrs {
                let n = || v.trim().parse::<u32>().ok();
                match crate::xlsx::xml::local(k) {
                    "paperSize" => self.paper = n().unwrap_or(1),
                    "scale" => self.scale = n().unwrap_or(100).clamp(10, 400),
                    "fitToWidth" => self.fit_width = n().unwrap_or(1),
                    "fitToHeight" => self.fit_height = n().unwrap_or(1),
                    "firstPageNumber" => self.first_page = n().or(self.first_page),
                    "useFirstPageNumber" => {}
                    "orientation" => self.landscape = v == "landscape",
                    "pageOrder" => self.over_then_down = v == "overThenDown",
                    "blackAndWhite" => self.black_and_white = e.flag(k, false),
                    "draft" => self.draft = e.flag(k, false),
                    "id" if k.contains(':') => self.printer_rel = Some(v.clone()),
                    _ => self.other.push((k.clone(), v.clone())),
                }
            },
            "headerFooter" => {
                let hf = &mut self.header_footer;
                hf.different_odd_even = e.flag("differentOddEven", false);
                hf.different_first = e.flag("differentFirst", false);
                hf.scale_with_doc = e.flag("scaleWithDoc", true);
                hf.align_with_margins = e.flag("alignWithMargins", true);
                let t = |n: &str| e.child(n).map(|c| c.text()).unwrap_or_default();
                hf.odd_header = t("oddHeader"); hf.odd_footer = t("oddFooter");
                hf.even_header = t("evenHeader"); hf.even_footer = t("evenFooter");
                hf.first_header = t("firstHeader"); hf.first_footer = t("firstFooter");
            }
            _ => {
                let set = if name == "rowBreaks" { &mut self.row_breaks } else { &mut self.col_breaks };
                set.extend(e.all("brk").filter_map(|b| b.attr_u32("id")).filter(|i| *i > 0));
            }
        }
        if name == "pageSetup" && !e.flag("useFirstPageNumber", false) { self.first_page = None; }
        true
    }

    /// The elements to write, as (element name, xml), for ordering among the worksheet's parts.
    pub fn write_xml(&self) -> Vec<(&'static str, String)> {
        use crate::xlsx::xml::{attr, text};
        let mut out = vec![];
        if self.gridlines || self.headings || self.center_h || self.center_v {
            let mut o = String::from("<printOptions");
            if self.center_h { o.push_str(" horizontalCentered=\"1\""); }
            if self.center_v { o.push_str(" verticalCentered=\"1\""); }
            if self.headings { o.push_str(" headings=\"1\""); }
            if self.gridlines { o.push_str(" gridLines=\"1\""); }
            o.push_str("/>");
            out.push(("printOptions", o));
        }
        let m = &self.margins;
        out.push(("pageMargins", format!("<pageMargins left=\"{}\" right=\"{}\" top=\"{}\" bottom=\"{}\" header=\"{}\" footer=\"{}\"/>", m.left, m.right, m.top, m.bottom, m.header, m.footer)));
        let mut p = String::new();
        if self.paper != 1 { p.push_str(&format!(" paperSize=\"{}\"", self.paper)); }
        if self.scale != 100 { p.push_str(&format!(" scale=\"{}\"", self.scale)); }
        if let Some(n) = self.first_page { p.push_str(&format!(" firstPageNumber=\"{n}\"")); }
        if self.fit_width != 1 { p.push_str(&format!(" fitToWidth=\"{}\"", self.fit_width)); }
        if self.fit_height != 1 { p.push_str(&format!(" fitToHeight=\"{}\"", self.fit_height)); }
        if self.over_then_down { p.push_str(" pageOrder=\"overThenDown\""); }
        if self.landscape { p.push_str(" orientation=\"landscape\""); }
        if self.black_and_white { p.push_str(" blackAndWhite=\"1\""); }
        if self.draft { p.push_str(" draft=\"1\""); }
        if self.first_page.is_some() { p.push_str(" useFirstPageNumber=\"1\""); }
        for (k, v) in &self.other { p.push_str(&format!(" {k}=\"{}\"", attr(v))); }
        if let Some(r) = &self.printer_rel { p.push_str(&format!(" r:id=\"{}\"", attr(r))); }
        if !p.is_empty() { out.push(("pageSetup", format!("<pageSetup{p}/>"))); }
        let hf = &self.header_footer;
        if !hf.is_default() {
            let mut h = String::from("<headerFooter");
            if hf.different_odd_even { h.push_str(" differentOddEven=\"1\""); }
            if hf.different_first { h.push_str(" differentFirst=\"1\""); }
            if !hf.scale_with_doc { h.push_str(" scaleWithDoc=\"0\""); }
            if !hf.align_with_margins { h.push_str(" alignWithMargins=\"0\""); }
            h.push('>');
            for (n, v) in [("oddHeader", &hf.odd_header), ("oddFooter", &hf.odd_footer), ("evenHeader", &hf.even_header), ("evenFooter", &hf.even_footer), ("firstHeader", &hf.first_header), ("firstFooter", &hf.first_footer)] {
                if !v.is_empty() { h.push_str(&format!("<{n}>{}</{n}>", text(v))); }
            }
            h.push_str("</headerFooter>");
            out.push(("headerFooter", h));
        }
        for (name, set, max) in [("rowBreaks", &self.row_breaks, MAX_COLS - 1), ("colBreaks", &self.col_breaks, MAX_ROWS - 1)] {
            if set.is_empty() { continue; }
            let mut b = format!("<{name} count=\"{0}\" manualBreakCount=\"{0}\">", set.len());
            for i in set { b.push_str(&format!("<brk id=\"{i}\" max=\"{max}\" man=\"1\"/>")); }
            b.push_str(&format!("</{name}>"));
            out.push((name, b));
        }
        out
    }

    /// Sets sheetPr's pageSetUpPr fitToPage to match `fit`.
    pub fn write_sheet_pr(&self, pr: &mut Element) {
        use crate::xlsx::xml::Node;
        let i = pr.children.iter().position(|c| matches!(c, Node::Element(e) if e.local() == "pageSetUpPr"));
        match (i, self.fit) {
            (Some(i), fit) => {
                if let Node::Element(e) = &mut pr.children[i] {
                    e.attrs.retain(|(k, _)| k != "fitToPage");
                    if fit { e.attrs.push(("fitToPage".into(), "1".into())); }
                    if e.attrs.is_empty() { pr.children.remove(i); }
                }
            }
            (None, true) => pr.children.push(Node::Element(Element { name: "pageSetUpPr".into(), attrs: vec![("fitToPage".into(), "1".into())], children: vec![] })),
            _ => {}
        }
    }
}

// ---- print areas and titles (defined names) ----

const AREA: &str = "_xlnm.Print_Area";
const TITLES: &str = "_xlnm.Print_Titles";

fn split_refs(f: &str) -> Vec<String> {
    let mut out = vec![]; let mut cur = String::new(); let mut q = false;
    for c in f.chars() {
        if c == '\'' { q = !q; }
        if c == ',' && !q { out.push(std::mem::take(&mut cur)); } else { cur.push(c); }
    }
    out.push(cur);
    out.into_iter().map(|s| { let s = s.trim(); let s = s.rsplit_once('!').map_or(s, |x| x.1); s.replace('$', "") }).filter(|s| !s.is_empty()).collect()
}

fn abs(r: &Range) -> String {
    let full_cols = r.r0 == 0 && r.r1 == MAX_ROWS - 1;
    let full_rows = r.c0 == 0 && r.c1 == MAX_COLS - 1;
    if full_cols { format!("${}:${}", col_name(r.c0), col_name(r.c1)) }
    else if full_rows { format!("${}:${}", r.r0 + 1, r.r1 + 1) }
    else if r.is_cell() { format!("${}${}", col_name(r.c0), r.r0 + 1) }
    else { format!("${}${}:${}${}", col_name(r.c0), r.r0 + 1, col_name(r.c1), r.r1 + 1) }
}

impl Workbook {
    fn sheet_name_index(&self, sheet: usize, name: &str) -> Option<usize> { self.names.iter().position(|n| n.sheet == Some(sheet) && n.name.eq_ignore_ascii_case(name)) }
    fn set_sheet_name(&mut self, sheet: usize, name: &str, formula: Option<String>) {
        let i = self.sheet_name_index(sheet, name);
        match (i, formula) {
            (Some(i), Some(f)) => self.names[i].formula = f,
            (Some(i), None) => { self.names.remove(i); }
            (None, Some(f)) => self.names.push(DefinedName { name: name.into(), sheet: Some(sheet), formula: f, hidden: false, comment: String::new() }),
            (None, None) => {}
        }
    }
    /// The sheet's print area (one or more ranges), if set.
    pub fn print_area(&self, sheet: usize) -> Option<Vec<Range>> {
        let n = &self.names[self.sheet_name_index(sheet, AREA)?];
        let v: Vec<Range> = split_refs(&n.formula).iter().filter_map(|s| parse_range(s)).collect();
        (!v.is_empty()).then_some(v)
    }
    pub fn set_print_area(&mut self, sheet: usize, area: Option<&[Range]>) {
        let q = quote_sheet(&self.sheets[sheet].name);
        let f = area.filter(|a| !a.is_empty()).map(|a| a.iter().map(|r| format!("{q}!{}", abs(r))).collect::<Vec<_>>().join(","));
        self.set_sheet_name(sheet, AREA, f);
    }
    /// Rows and columns repeated on every page: (first, last) of each.
    pub fn print_titles(&self, sheet: usize) -> (Option<(u32, u32)>, Option<(u32, u32)>) {
        let Some(i) = self.sheet_name_index(sheet, TITLES) else { return (None, None) };
        let (mut rows, mut cols) = (None, None);
        for s in split_refs(&self.names[i].formula) {
            let Some(r) = parse_range(&s) else { continue };
            if r.c0 == 0 && r.c1 == MAX_COLS - 1 { rows = Some((r.r0, r.r1)); } else if r.r0 == 0 && r.r1 == MAX_ROWS - 1 { cols = Some((r.c0, r.c1)); }
        }
        (rows, cols)
    }
    pub fn set_print_titles(&mut self, sheet: usize, rows: Option<(u32, u32)>, cols: Option<(u32, u32)>) {
        let q = quote_sheet(&self.sheets[sheet].name);
        let mut parts = vec![];
        if let Some((a, b)) = cols { parts.push(format!("{q}!{}", abs(&Range { r0: 0, r1: MAX_ROWS - 1, c0: a, c1: b }))); }
        if let Some((a, b)) = rows { parts.push(format!("{q}!{}", abs(&Range { r0: a, r1: b, c0: 0, c1: MAX_COLS - 1 }))); }
        self.set_sheet_name(sheet, TITLES, (!parts.is_empty()).then(|| parts.join(",")));
    }
}

/// Parses "1:3" (rows) or "A:B" (columns) as (first, last), 0-based.
pub fn parse_title_span(s: &str, rows: bool) -> Option<(u32, u32)> {
    let s = s.replace('$', "");
    let s = s.trim();
    if s.is_empty() { return None; }
    let (a, b) = s.split_once(':').unwrap_or((s, s));
    if rows {
        let (a, b) = (a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?);
        (a >= 1 && b >= 1).then(|| (a.min(b) - 1, a.max(b) - 1))
    } else {
        let (a, b) = (crate::addr::col_index(&a.trim().to_ascii_uppercase())?, crate::addr::col_index(&b.trim().to_ascii_uppercase())?);
        Some((a.min(b), a.max(b)))
    }
}

pub fn title_span_text(span: Option<(u32, u32)>, rows: bool) -> String {
    match span { None => String::new(), Some((a, b)) if rows => format!("${}:${}", a + 1, b + 1), Some((a, b)) => format!("${}:${}", col_name(a), col_name(b)) }
}

// ---- pagination ----

/// What a print job covers.
#[derive(Clone, Debug, PartialEq)]
pub enum What { ActiveSheet, Workbook, Selection(Vec<Range>) }

/// One printed page: a block of rows and columns of a sheet, plus the titles repeated on it.
#[derive(Clone, Debug, PartialEq)]
pub struct PrintPage {
    pub sheet: usize,
    pub rows: (u32, u32),
    pub cols: (u32, u32),
    pub title_rows: Option<(u32, u32)>,
    pub title_cols: Option<(u32, u32)>,
    /// The page number printed by &P, and the sheet's page count for &N.
    pub number: u32,
    pub count: u32,
    /// Print scale (1.0 = 100%).
    pub scale: f32,
}

/// The cells a sheet prints when no print area is set: everything with content, a fill or a border.
pub fn used_range(wb: &Workbook, si: usize) -> Option<Range> {
    let s = &wb.sheets[si];
    let mut r: Option<Range> = s.extent().map(|e| Range::new(Pos::new(0, 0), e));
    let mut add = |x: Range| r = Some(r.map_or(Range::new(Pos::new(0, 0), x.end()), |r| r.union(&Range::new(Pos::new(0, 0), x.end()))));
    for (p, c) in &s.cells {
        if c.style == 0 { continue; }
        let st = wb.styles.get(c.style);
        if st.fill.is_some() || [st.left, st.right, st.top, st.bottom].iter().any(|b| b.style != LineStyle::None) { add(Range::cell(*p)); }
    }
    for m in &s.merges { add(*m); }
    // Floating objects print too.
    for o in &s.objects { if let Some(e) = objects::last_cell(s, o) { add(Range::cell(e)); } }
    for g in &s.sparklines { for sp in &g.sparklines { add(Range::cell(sp.loc)); } }
    r
}

/// Cuts [lo, hi] into runs whose sizes fit `avail`, starting new runs at manual breaks.
fn cut(size: impl Fn(u32) -> f32, lo: u32, hi: u32, avail: f32, breaks: &BTreeSet<u32>) -> Vec<(u32, u32)> {
    let mut out = vec![];
    let (mut start, mut acc) = (lo, 0.0);
    for i in lo..=hi {
        let s = size(i);
        if i > start && (breaks.contains(&i) || (acc + s > avail && acc > 0.0)) { out.push((start, i - 1)); start = i; acc = 0.0; }
        acc += s;
    }
    out.push((start, hi));
    out.retain(|&(a, b)| (a..=b).any(|i| size(i) > 0.0));
    out
}

fn span_size(s: &Sheet, span: Option<(u32, u32)>, rows: bool) -> f32 {
    span.map_or(0.0, |(a, b)| (a..=b).map(|i| if rows { s.row_height(i) } else { s.col_width(i) }).sum())
}

/// Heading sizes (row-number column width, column-letter row height) in sheet pixels.
fn heading_size(s: &Sheet, last_row: u32) -> (f32, f32) { ((last_row + 1).to_string().len().max(2) as f32 * 7.0 + 10.0, s.default_row_height) }

/// The usable page area in sheet pixels at scale 1.
fn page_area(p: &PageSetup) -> (f32, f32) {
    let (w, h) = p.paper_inches();
    let m = &p.margins;
    (((w - m.left - m.right) * PX_PER_INCH).max(24.0), ((h - m.top - m.bottom) * PX_PER_INCH).max(24.0))
}

/// Pages for one range of a sheet at `scale`.
fn pages_at(wb: &Workbook, si: usize, area: Range, scale: f32) -> Vec<PrintPage> {
    let s = &wb.sheets[si];
    let p = &s.page;
    let (tr, tc) = wb.print_titles(si);
    let (mut aw, mut ah) = page_area(p);
    aw /= scale; ah /= scale;
    if p.headings { let (hw, hh) = heading_size(s, area.r1); aw -= hw; ah -= hh; }
    // Titles take room on every page that doesn't already show them.
    let (tw, th) = (span_size(s, tc.filter(|t| t.1 < area.c1), false), span_size(s, tr.filter(|t| t.1 < area.r1), true));
    let col_bands = cut(|c| s.col_width(c), area.c0, area.c1, (aw - tw).max(aw * 0.25), &p.col_breaks);
    let row_bands = cut(|r| s.row_height(r), area.r0, area.r1, (ah - th).max(ah * 0.25), &p.row_breaks);
    let mut out = vec![];
    let mut push = |rb: (u32, u32), cb: (u32, u32)| out.push(PrintPage {
        sheet: si, rows: rb, cols: cb, title_rows: tr.filter(|t| rb.0 > t.1), title_cols: tc.filter(|t| cb.0 > t.1), number: 0, count: 0, scale,
    });
    if p.over_then_down { for rb in &row_bands { for cb in &col_bands { push(*rb, *cb); } } }
    else { for cb in &col_bands { for rb in &row_bands { push(*rb, *cb); } } }
    out
}

/// The pages of one sheet (its print area, or `ranges` when printing a selection).
pub fn sheet_pages(wb: &Workbook, si: usize, ranges: Option<&[Range]>) -> Vec<PrintPage> {
    let s = &wb.sheets[si];
    let used = used_range(wb, si);
    let clamp = |r: Range| -> Option<Range> {
        // Whole rows or columns print only as far as the sheet is used.
        let u = used?;
        let r = Range { r1: if r.r1 == MAX_ROWS - 1 { u.r1.max(r.r0) } else { r.r1 }, c1: if r.c1 == MAX_COLS - 1 { u.c1.max(r.c0) } else { r.c1 }, ..r };
        Some(r)
    };
    let areas: Vec<Range> = match ranges {
        Some(r) => r.iter().filter_map(|r| clamp(*r).or(Some(*r))).collect(),
        None => match wb.print_area(si) { Some(a) => a.into_iter().filter_map(clamp).collect(), None => used.into_iter().collect() },
    };
    let p = &s.page;
    let mut scale = if p.fit { 1.0 } else { p.scale.clamp(10, 400) as f32 / 100.0 };
    if p.fit && (p.fit_width > 0 || p.fit_height > 0) {
        // Shrink until the widest area fits the requested pages.
        let (aw, ah) = page_area(p);
        for a in &areas {
            let w: f32 = (a.c0..=a.c1).map(|c| s.col_width(c)).sum::<f32>() + if p.headings { heading_size(s, a.r1).0 } else { 0.0 };
            let h: f32 = (a.r0..=a.r1).map(|r| s.row_height(r)).sum::<f32>() + if p.headings { heading_size(s, a.r1).1 } else { 0.0 };
            if p.fit_width > 0 { scale = scale.min(aw * p.fit_width as f32 / w.max(1.0)); }
            if p.fit_height > 0 { scale = scale.min(ah * p.fit_height as f32 / h.max(1.0)); }
        }
        scale = (scale * 100.0).floor().clamp(10.0, 100.0) / 100.0;
        for _ in 0..40 {
            let ok = areas.iter().all(|a| {
                let pg = pages_at(wb, si, *a, scale);
                let cols = pg.iter().map(|x| x.cols).collect::<BTreeSet<_>>().len() as u32;
                let rows = pg.iter().map(|x| x.rows).collect::<BTreeSet<_>>().len() as u32;
                (p.fit_width == 0 || cols <= p.fit_width) && (p.fit_height == 0 || rows <= p.fit_height)
            });
            if ok || scale <= 0.1 { break; }
            scale = ((scale * 100.0 - 1.0).max(10.0)).round() / 100.0;
        }
    }
    let mut out: Vec<PrintPage> = areas.iter().flat_map(|a| pages_at(wb, si, *a, scale)).collect();
    let count = out.len() as u32;
    let first = p.first_page.unwrap_or(1);
    for (i, pg) in out.iter_mut().enumerate() { pg.number = first + i as u32; pg.count = count; }
    out
}

/// Every page a print job produces, in order.
pub fn pages(wb: &Workbook, what: &What) -> Vec<PrintPage> {
    match what {
        What::ActiveSheet => sheet_pages(wb, wb.active, None),
        What::Selection(r) => sheet_pages(wb, wb.active, Some(r)),
        What::Workbook => (0..wb.sheets.len()).filter(|i| !wb.sheets[*i].hidden).flat_map(|i| sheet_pages(wb, i, None)).collect(),
    }
}

/// Where automatic and manual page breaks fall on a sheet: (rows that start a page, columns
/// that start a page), excluding the first — for drawing dashed break lines on the grid.
pub fn break_lines(wb: &Workbook, si: usize) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let pg = sheet_pages(wb, si, None);
    let rows: BTreeSet<u32> = pg.iter().map(|p| p.rows.0).collect();
    let cols: BTreeSet<u32> = pg.iter().map(|p| p.cols.0).collect();
    let (r0, c0) = (rows.iter().next().copied(), cols.iter().next().copied());
    (rows.into_iter().filter(|r| Some(*r) != r0).collect(), cols.into_iter().filter(|c| Some(*c) != c0).collect())
}

// ---- drawing ----

/// Values for header and footer fields.
#[derive(Clone, Debug, Default)]
pub struct JobInfo {
    /// The file's name (&F) and folder (&Z).
    pub file_name: String,
    pub folder: String,
    /// The date and time printed (&D, &T), as a serial date; None = now (UTC).
    pub now: Option<f64>,
}

fn col(c: [u8; 3]) -> Color { Color(c[0], c[1], c[2]) }
const BLACK: Color = Color(0, 0, 0);

fn is_rtl(c: char) -> bool { matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF) }
fn is_strong(c: char) -> bool { c.is_alphabetic() }

/// A line of text shaped into glyph runs, x from 0.
struct Shaped { runs: Vec<(FontKey, Vec<Glyph>)>, width: f32 }

fn shape_text(text: &str, key: FontKey, size: f32) -> Shaped {
    // Runs of one face (script fallback), each shaped in its direction.
    let mut groups: Vec<(FontKey, String)> = vec![];
    for c in text.chars() {
        let k = if c.is_whitespace() { groups.last().map_or(key, |g| g.0) } else { word_engine::fonts::fallback(key, c) };
        match groups.last_mut() { Some(g) if g.0 == k => g.1.push(c), _ => groups.push((k, c.to_string())) }
    }
    let para_rtl = text.chars().find(|c| is_strong(*c)).is_some_and(is_rtl);
    if para_rtl { groups.reverse(); }
    let mut runs = vec![];
    let mut pen = 0.0;
    for (k, s) in groups {
        let rtl = s.chars().any(is_rtl);
        let shaped = word_engine::fonts::shape(k, &s, rtl, size);
        let mut glyphs = vec![];
        for g in shaped.iter() {
            let ch = s.get(g.cluster..).and_then(|t| t.chars().next()).unwrap_or(' ');
            glyphs.push(Glyph { ch, gid: g.gid, x: pen + g.dx, dy: g.dy });
            pen += g.advance + if k.synthetic_bold && !ch.is_whitespace() { size / 32.0 } else { 0.0 };
        }
        runs.push((k, glyphs));
    }
    Shaped { runs, width: pen }
}

fn measure(text: &str, key: FontKey, size: f32) -> f32 { shape_text(text, key, size).width }

/// Greedy word wrap into lines no wider than `w`.
fn wrap_lines(text: &str, key: FontKey, size: f32, w: f32) -> Vec<String> {
    let mut out = vec![];
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && measure(&cand, key, size) > w { out.push(std::mem::replace(&mut line, word.to_string())); } else { line = cand; }
        }
        out.push(line);
    }
    out
}

/// A run of text drawn with one format.
#[derive(Clone)]
struct Seg { text: String, font: String, bold: bool, italic: bool, underline: bool, strike: bool, size: f32, color: Color }

struct Canvas { under: Vec<DrawItem>, items: Vec<DrawItem>, over: Vec<DrawItem> }

impl Canvas {
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) { if w > 0.0 && h > 0.0 { self.under.push(DrawItem::Rect { x, y, w, h, color }); } }
    fn line(to: &mut Vec<DrawItem>, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color) { to.push(DrawItem::Line { x1, y1, x2, y2, width, color }); }
    /// Draws a shaped line at (x, baseline), dropping glyphs outside [clip0, clip1].
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, sh: &Shaped, x: f32, baseline: f32, size: f32, color: Color, clip: (f32, f32), rotate: Option<(f32, f32, f32)>) {
        for (k, glyphs) in &sh.runs {
            let adv = size * 0.6;
            let g: Vec<Glyph> = glyphs.iter().filter(|g| { let gx = x + g.x; gx >= clip.0 - 0.5 && gx + adv * 0.5 <= clip.1 + 0.5 || rotate.is_some() }).map(|g| Glyph { x: x + g.x, ..g.clone() }).collect();
            if g.is_empty() { continue; }
            self.items.push(DrawItem::Glyphs { glyphs: g, baseline, font: *k, size, color, effects: Effects { rotate, ..Default::default() } });
        }
    }
}

fn line_pattern(style: LineStyle) -> &'static [f32] {
    match style {
        LineStyle::Dashed | LineStyle::MediumDashed => &[3.0, 1.5],
        LineStyle::Dotted => &[1.0, 1.0],
        LineStyle::Hair => &[1.0, 1.0],
        LineStyle::DashDot | LineStyle::MediumDashDot | LineStyle::SlantDashDot => &[3.0, 1.0, 1.0, 1.0],
        LineStyle::DashDotDot | LineStyle::MediumDashDotDot => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        _ => &[],
    }
}

/// One border edge from (x1, y1) to (x2, y2), with dashes and double lines.
fn border(to: &mut Vec<DrawItem>, b: &crate::style::Border, x1: f32, y1: f32, x2: f32, y2: f32, scale: f32) {
    if b.style == LineStyle::None { return; }
    let color = b.color.map(col).unwrap_or(BLACK);
    let w = (b.style.width() * scale).max(0.4) * 0.75;
    if b.style == LineStyle::Double {
        let (dx, dy) = if y1 == y2 { (0.0, scale) } else { (scale, 0.0) };
        Canvas::line(to, x1 - dx, y1 - dy, x2 - dx, y2 - dy, 0.6 * scale, color);
        Canvas::line(to, x1 + dx, y1 + dy, x2 + dx, y2 + dy, 0.6 * scale, color);
        return;
    }
    let pat = line_pattern(b.style);
    if pat.is_empty() { Canvas::line(to, x1, y1, x2, y2, w, color); return; }
    let len = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
    if len <= 0.0 { return; }
    let (ux, uy) = ((x2 - x1) / len, (y2 - y1) / len);
    let unit = (b.style.width() * scale).max(1.0) * 1.5;
    let (mut t, mut k) = (0.0, 0);
    while t < len {
        let d = pat[k % pat.len()] * unit;
        if k % 2 == 0 { let e = (t + d).min(len); Canvas::line(to, x1 + ux * t, y1 + uy * t, x1 + ux * e, y1 + uy * e, w, color); }
        t += d; k += 1;
    }
}

/// The visible lines of a page: (index, start, size) in page pixels from 0.
fn tracks(s: &Sheet, title: Option<(u32, u32)>, body: (u32, u32), rows: bool, scale: f32) -> Vec<(u32, f32, f32)> {
    let mut out = vec![];
    let mut at = 0.0;
    let idx = title.map(|(a, b)| a..=b).into_iter().flatten().chain(body.0..=body.1);
    for i in idx {
        let sz = if rows { s.row_height(i) } else { s.col_width(i) } * scale;
        if sz <= 0.0 { continue; }
        out.push((i, at, sz));
        at += sz;
    }
    out
}

/// The shown text of a cell, its color and effective alignment.
fn shown(wb: &Workbook, si: usize, p: Pos, st: &Style) -> Option<(String, Option<Color>, HAlign, bool)> {
    let s = &wb.sheets[si];
    let v = s.value(p);
    if v.is_empty() || (!s.show_zeros && v == Value::Num(0.0)) || crate::cellimage::is_image(&v) { return None; }
    let f = crate::numfmt::format(&v, &st.num_fmt, wb.date1904);
    let align = match st.h { HAlign::General => match v { Value::Num(_) => HAlign::Right, Value::Bool(_) | Value::Err(_) => HAlign::Center, _ => HAlign::Left }, a => a };
    Some((f.text, f.color.map(col), align, matches!(v, Value::Num(_))))
}

fn font_key(st: &Style) -> FontKey { word_engine::fonts::resolve(&st.font, st.bold, st.italic) }
fn font_px(st: &Style, scale: f32) -> f32 { st.size_pt() * PX_PER_INCH / 72.0 * scale }

/// Draws page `pg` into a page box.
pub fn draw_page(wb: &Workbook, pg: &PrintPage, info: &JobInfo) -> PageBox { draw_page_with(wb, pg, info, &mut objects::Assets::new()) }

/// Draws a page, adding the pictures it shows to `assets`.
pub fn draw_page_with(wb: &Workbook, pg: &PrintPage, info: &JobInfo, assets: &mut objects::Assets) -> PageBox {
    let s = &wb.sheets[pg.sheet];
    let p = &s.page;
    let (pw, ph) = p.paper_inches();
    let (pw, ph) = (pw * PX_PER_INCH, ph * PX_PER_INCH);
    let m = &p.margins;
    let k = pg.scale;
    let mut cv = Canvas { under: vec![], items: vec![], over: vec![] };
    let cols = tracks(s, pg.title_cols, pg.cols, false, k);
    let rows = tracks(s, pg.title_rows, pg.rows, true, k);
    let (hw, hh) = if p.headings { let (a, b) = heading_size(s, pg.rows.1); (a * k, b * k) } else { (0.0, 0.0) };
    let cw: f32 = cols.last().map_or(0.0, |c| c.1 + c.2);
    let rh: f32 = rows.last().map_or(0.0, |r| r.1 + r.2);
    let (aw, ah) = ((pw - (m.left + m.right) * PX_PER_INCH).max(1.0), (ph - (m.top + m.bottom) * PX_PER_INCH).max(1.0));
    let mut ox = m.left * PX_PER_INCH + if p.center_h { ((aw - cw - hw) / 2.0).max(0.0) } else { 0.0 };
    let oy = m.top * PX_PER_INCH + if p.center_v { ((ah - rh - hh) / 2.0).max(0.0) } else { 0.0 };
    // Right-to-left sheets print with column A at the right.
    let rtl = s.right_to_left;
    if rtl { ox = pw - ox - cw - hw; }
    let (gx, gy) = (if rtl { ox } else { ox + hw }, oy + hh);
    let xpos = |i: usize| -> f32 { let c = &cols[i]; if rtl { gx + cw - c.1 - c.2 } else { gx + c.1 } };
    let cell_rect = |ri: usize, ci: usize| -> (f32, f32, f32, f32) { (xpos(ci), gy + rows[ri].1, cols[ci].2, rows[ri].2) };
    // Merges, as visible track spans.
    let merge_span = |mr: &Range| -> Option<(usize, usize, usize, usize)> {
        let rs: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].0 >= mr.r0 && rows[i].0 <= mr.r1).collect();
        let cs: Vec<usize> = (0..cols.len()).filter(|&i| cols[i].0 >= mr.c0 && cols[i].0 <= mr.c1).collect();
        Some((*rs.first()?, *rs.last()?, *cs.first()?, *cs.last()?))
    };
    let span_rect = |(r0, r1, c0, c1): (usize, usize, usize, usize)| -> (f32, f32, f32, f32) {
        let (a, b) = (cell_rect(r0, c0), cell_rect(r1, c1));
        let (x0, x1) = (a.0.min(b.0), (a.0 + a.2).max(b.0 + b.2));
        (x0, a.1, x1 - x0, b.1 + b.3 - a.1)
    };
    let merges: Vec<(Range, (usize, usize, usize, usize))> = s.merges.iter().filter_map(|mr| merge_span(mr).map(|sp| (*mr, sp))).collect();
    let merge_of = |ri: usize, ci: usize| merges.iter().find(|(_, (r0, r1, c0, c1))| ri >= *r0 && ri <= *r1 && ci >= *c0 && ci <= *c1);
    let in_merge = |p: Pos| s.merges.iter().any(|m| m.contains(p));
    let default = wb.styles.get(0).clone();
    let ink = |st: &Style| if p.black_and_white { BLACK } else { st.color.map(col).unwrap_or(BLACK) };

    // Headings.
    if p.headings {
        let hs = Style { size: default.size.min(110), ..default.clone() };
        let (key, sz) = (font_key(&hs), font_px(&hs, k));
        let met = word_engine::fonts::metrics(key);
        let grey = Color(0xE8, 0xE8, 0xE8);
        let line = Color(0xA0, 0xA0, 0xA0);
        let hx = if rtl { ox + cw } else { ox };
        cv.rect(gx.min(hx), oy, cw + hw, hh, grey);
        cv.rect(hx, gy, hw, rh, grey);
        for (i, c) in cols.iter().enumerate() {
            let x = xpos(i);
            let sh = shape_text(&col_name(c.0), key, sz);
            cv.text(&sh, x + (c.2 - sh.width) / 2.0, oy + (hh + (met.ascent - met.descent) * sz) / 2.0, sz, BLACK, (x, x + c.2), None);
            Canvas::line(&mut cv.under, x, oy, x, oy + hh, 0.5, line);
            Canvas::line(&mut cv.under, x + c.2, oy, x + c.2, oy + hh, 0.5, line);
        }
        for r in &rows {
            let y = gy + r.1;
            let sh = shape_text(&(r.0 + 1).to_string(), key, sz);
            cv.text(&sh, hx + (hw - sh.width) / 2.0, y + (r.2 + (met.ascent - met.descent) * sz) / 2.0, sz, BLACK, (hx, hx + hw), None);
            Canvas::line(&mut cv.under, hx, y + r.2, hx + hw, y + r.2, 0.5, line);
        }
        Canvas::line(&mut cv.under, gx.min(hx), oy + hh, gx.min(hx) + cw + hw, oy + hh, 0.5, line);
    }

    // Fills.
    for (ri, r) in rows.iter().enumerate() {
        for (ci, c) in cols.iter().enumerate() {
            let q = Pos::new(r.0, c.0);
            if merge_of(ri, ci).is_some() { continue; }
            if let Some(f) = wb.styles.get(s.style_index(q)).fill { let (x, y, w, h) = cell_rect(ri, ci); cv.rect(x, y, w, h, if p.black_and_white { Color(255, 255, 255) } else { col(f) }); }
        }
    }
    for (mr, sp) in &merges {
        if let Some(f) = wb.styles.get(s.style_index(mr.start())).fill { let (x, y, w, h) = span_rect(*sp); cv.rect(x, y, w, h, if p.black_and_white { Color(255, 255, 255) } else { col(f) }); }
    }
    // Gridlines (not inside merged cells).
    if p.gridlines && !rows.is_empty() && !cols.is_empty() {
        let gc = Color(0xC0, 0xC0, 0xC0);
        let x0 = xpos(if rtl { cols.len() - 1 } else { 0 });
        Canvas::line(&mut cv.under, x0, gy, x0 + cw, gy, 0.5, gc);
        Canvas::line(&mut cv.under, if rtl { x0 + cw } else { x0 }, gy, if rtl { x0 + cw } else { x0 }, gy + rh, 0.5, gc);
        for ri in 0..rows.len() {
            for ci in 0..cols.len() {
                let (x, y, w, h) = cell_rect(ri, ci);
                let mg = merge_of(ri, ci).map(|m| m.1);
                let edge_x = if rtl { x } else { x + w };
                if mg.is_none_or(|(_, _, _, c1)| ci == c1) { Canvas::line(&mut cv.under, edge_x, y, edge_x, y + h, 0.5, gc); }
                if mg.is_none_or(|(_, r1, _, _)| ri == r1) { Canvas::line(&mut cv.under, x, y + h, x + w, y + h, 0.5, gc); }
            }
        }
    }

    // Text.
    let empty = |ri: usize, ci: usize| { let q = Pos::new(rows[ri].0, cols[ci].0); s.value(q).is_empty() && !in_merge(q) };
    let draw_cell = |cv: &mut Canvas, q: Pos, rect: (f32, f32, f32, f32), ri: usize, ci: usize, merged: bool| {
        let st = wb.styles.get(s.style_index(q));
        let Some((mut text, fcolor, align, num)) = shown(wb, pg.sheet, q, st) else { return };
        let key = font_key(st);
        let sz = font_px(st, k) * if st.script != 0 { 0.7 } else { 1.0 };
        let met = word_engine::fonts::metrics(key);
        let color = if p.black_and_white { BLACK } else { fcolor.unwrap_or_else(|| ink(st)) };
        let pad = 2.0 * k + st.indent as f32 * 9.0 * k;
        let (x, y, w, h) = rect;
        let lh = (met.ascent + met.descent) * sz;
        let wrap = (st.wrap || st.h == HAlign::Justify || st.v == VAlign::Justify) && !num;
        // Rotated text turns about the cell's center.
        let rot = match st.rotation { r @ 1..=90 => Some(-(r as f32)), r @ 91..=180 => Some((r - 90) as f32), _ => None };
        if let Some(deg) = rot {
            let sh = shape_text(&text.replace('\n', " "), key, sz);
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            cv.text(&sh, cx - sh.width / 2.0, cy + (met.ascent - met.descent) * sz / 2.0, sz, color, (f32::MIN, f32::MAX), Some((deg, cx, cy)));
            return;
        }
        let lines: Vec<String> = if wrap { wrap_lines(&text, key, sz, (w - 2.0 * pad).max(4.0)) } else { vec![text.replace('\n', " ")] };
        let mut clip = (x, x + w);
        if !wrap && lines.len() == 1 {
            let tw = measure(&lines[0], key, sz);
            if num && tw > w - 2.0 * pad {
                // Numbers too wide for the column show ###.
                let hash = measure("#", key, sz).max(1.0);
                text = "#".repeat(((w - 2.0 * pad) / hash).floor().max(1.0) as usize);
                let sh = shape_text(&text, key, sz);
                let bl = match st.v { VAlign::Top => y + met.ascent * sz + k, VAlign::Bottom => y + h - met.descent * sz - k, _ => y + (h + (met.ascent - met.descent) * sz) / 2.0 };
                cv.text(&sh, x + pad, bl, sz, color, clip, None);
                return;
            }
            if !merged && tw > w - 2.0 * pad {
                // Long text runs into empty neighbours on the page.
                let need = tw + 2.0 * pad;
                let (mut l, mut r) = (ci, ci);
                let width = |l: usize, r: usize| cols[l..=r].iter().map(|c| c.2).sum::<f32>();
                let grow_r = matches!(align, HAlign::Left | HAlign::Center | HAlign::CenterAcross | HAlign::General | HAlign::Fill | HAlign::Justify | HAlign::Distributed);
                let grow_l = matches!(align, HAlign::Right | HAlign::Center | HAlign::CenterAcross);
                while width(l, r) < need {
                    let mut grew = false;
                    if grow_r && r + 1 < cols.len() && empty(ri, r + 1) { r += 1; grew = true; }
                    if grow_l && l > 0 && empty(ri, l - 1) { l -= 1; grew = true; }
                    if !grew { break; }
                }
                let (a, b) = (xpos(l), xpos(r));
                clip = (a.min(b), (a + cols[l].2).max(b + cols[r].2));
            }
        }
        if st.h == HAlign::Fill && !text.is_empty() {
            let one = measure(&text, key, sz).max(1.0);
            let n = ((w - 2.0 * pad) / one).floor().max(1.0) as usize;
            let t = text.repeat(n);
            let sh = shape_text(&t, key, sz);
            let bl = y + h - met.descent * sz - k;
            cv.text(&sh, x + pad, bl, sz, color, clip, None);
            return;
        }
        let total = lh * lines.len() as f32;
        let top = match st.v { VAlign::Top => y + k, VAlign::Bottom => y + h - total - k, _ => y + (h - total) / 2.0 };
        let mut ul_items = vec![];
        for (i, line) in lines.iter().enumerate() {
            let sh = shape_text(line, key, sz);
            let mut bl = top + met.ascent * sz + lh * i as f32;
            if st.script > 0 { bl -= sz * 0.4; } else if st.script < 0 { bl += sz * 0.2; }
            let lx = match align {
                HAlign::Right => x + w - pad - sh.width,
                HAlign::Center | HAlign::CenterAcross | HAlign::Distributed => x + (w - sh.width) / 2.0,
                _ => x + pad,
            };
            cv.text(&sh, lx, bl, sz, color, clip, None);
            let (lx0, lx1) = (lx.max(clip.0), (lx + sh.width).min(clip.1));
            if st.underline != Underline::None && lx1 > lx0 {
                let uy = bl + met.underline_offset * sz;
                let t = met.underline_thickness * sz;
                ul_items.push((lx0, uy, lx1, t));
                if matches!(st.underline, Underline::Double | Underline::DoubleAccounting) { ul_items.push((lx0, uy + t * 2.0, lx1, t)); }
            }
            if st.strike && lx1 > lx0 { let sy = bl - met.strike_offset * sz; ul_items.push((lx0, sy, lx1, met.underline_thickness * sz)); }
        }
        for (a, yy, b, t) in ul_items { Canvas::line(&mut cv.items, a, yy, b, yy, t, color); }
    };
    for (ri, r) in rows.iter().enumerate() {
        for (ci, c) in cols.iter().enumerate() {
            if merge_of(ri, ci).is_some() { continue; }
            draw_cell(&mut cv, Pos::new(r.0, c.0), cell_rect(ri, ci), ri, ci, false);
        }
    }
    for (mr, sp) in &merges {
        let rect = span_rect(*sp);
        draw_cell(&mut cv, mr.start(), rect, sp.0, sp.2, true);
    }

    // Borders (each cell's own edges; shared edges also take the neighbour's).
    for (ri, r) in rows.iter().enumerate() {
        for (ci, c) in cols.iter().enumerate() {
            let q = Pos::new(r.0, c.0);
            let st = wb.styles.get(s.style_index(q));
            let (x, y, w, h) = cell_rect(ri, ci);
            let (lx, rx) = if rtl { (x + w, x) } else { (x, x + w) };
            border(&mut cv.over, &st.top, x, y, x + w, y, k);
            border(&mut cv.over, &st.bottom, x, y + h, x + w, y + h, k);
            border(&mut cv.over, &st.left, lx, y, lx, y + h, k);
            border(&mut cv.over, &st.right, rx, y, rx, y + h, k);
            if st.diag_down { border(&mut cv.over, &st.diagonal, lx, y, rx, y + h, k); }
            if st.diag_up { border(&mut cv.over, &st.diagonal, lx, y + h, rx, y, k); }
        }
    }
    // Sparklines and pictures in cells (sparkline.rs).
    if !s.sparklines.is_empty() || s.cells.values().any(|c| crate::cellimage::is_image(&c.value)) {
        for (ri, r) in rows.iter().enumerate() { for (ci, c) in cols.iter().enumerate() { let (x, y, w, h) = cell_rect(ri, ci); crate::sparkline::print_cell(wb, pg.sheet, Pos::new(r.0, c.0), [x, y, x + w, y + h], k, assets, &mut cv.items); } }
    }
    // Floating objects, above the cells, in z-order.
    objects::draw(wb, pg, gx, gy, cw, rtl, assets, &mut cv.over);

    draw_hf(wb, pg, info, assets, &mut cv);
    PageBox { width: pw, height: ph, items: cv.items, under: cv.under, links: vec![], over: cv.over }
}


/// A page with only its header and footer (Page Layout view draws the cells live).
pub fn header_footer_page(wb: &Workbook, pg: &PrintPage, info: &JobInfo, assets: &mut objects::Assets) -> PageBox {
    let (pw, ph) = wb.sheets[pg.sheet].page.paper_inches();
    let mut cv = Canvas { under: vec![], items: vec![], over: vec![] };
    draw_hf(wb, pg, info, assets, &mut cv);
    PageBox { width: pw * PX_PER_INCH, height: ph * PX_PER_INCH, items: cv.items, under: cv.under, links: vec![], over: cv.over }
}

/// Draws a page's header and footer (with their &G pictures).
fn draw_hf(wb: &Workbook, pg: &PrintPage, info: &JobInfo, assets: &mut objects::Assets, cv: &mut Canvas) {
    let s = &wb.sheets[pg.sheet];
    let p = &s.page;
    let (pw, ph) = p.paper_inches();
    let (pw, ph) = (pw * PX_PER_INCH, ph * PX_PER_INCH);
    let m = &p.margins;
    let k = pg.scale;
    let default = wb.styles.get(0).clone();
    let kind = crate::hfpic::page_kind(&p.header_footer, pg.number);
    let (head, foot) = p.header_footer.for_page(pg.number);
    let hk = if p.header_footer.scale_with_doc { k } else { 1.0 };
    let (lm, rm) = if p.header_footer.align_with_margins { (m.left * PX_PER_INCH, pw - m.right * PX_PER_INCH) } else { (0.5 * PX_PER_INCH, pw - 0.5 * PX_PER_INCH) };
    let fields = Fields { page: pg.number, pages: pg.count, sheet: &s.name, info };
    for (src, is_header) in [(head, true), (foot, false)] {
        if src.is_empty() { continue; }
        let sections = parse_header(src, &default, &fields);
        for (si, lines) in sections.iter().enumerate() {
            let shaped: Vec<(Vec<(Shaped, f32, Color, bool, bool, Option<(String, f32, f32)>)>, f32, f32, f32)> = lines.iter().map(|segs| {
                let mut parts = vec![]; let (mut w, mut asc, mut desc) = (0.0f32, 0.0f32, 0.0f32);
                for sg in segs {
                    if sg.text == PICTURE {
                        let slot = crate::hfpic::slot(si, !is_header, kind);
                        let Some(pic) = p.header_footer.pictures.iter().find(|x| x.slot == slot) else { continue };
                        let (pw2, ph2) = (pic.width * PX_PER_INCH / 72.0 * hk, pic.height * PX_PER_INCH / 72.0 * hk);
                        let name = format!("jadwal-hf-{}-{slot}-{:x}-{:?}", pg.sheet, std::sync::Arc::as_ptr(&pic.data) as usize, pic.crop);
                        let ct = if pic.crop.iter().any(|c| *c > 0.0) { "image/png" } else { pic.content_type() };
                        assets.entry(name.clone()).or_insert_with(|| word_engine::model::Asset { bytes: pic.cropped(), content_type: ct.into() });
                        asc = asc.max(ph2); w += pw2;
                        parts.push((shape_text("", font_key(&default), 1.0), 0.0, sg.color, false, false, Some((name, pw2, ph2))));
                        continue;
                    }
                    let key = word_engine::fonts::resolve(&sg.font, sg.bold, sg.italic);
                    let sz = sg.size * PX_PER_INCH / 72.0 * hk;
                    let met = word_engine::fonts::metrics(key);
                    asc = asc.max(met.ascent * sz); desc = desc.max(met.descent * sz);
                    let sh = shape_text(&sg.text, key, sz);
                    w += sh.width;
                    parts.push((sh, sz, sg.color, sg.underline, sg.strike, None));
                }
                if segs.is_empty() { let met = word_engine::fonts::metrics(font_key(&default)); let sz = font_px(&default, hk); asc = met.ascent * sz; desc = met.descent * sz; }
                (parts, w, asc, desc)
            }).collect();
            let total: f32 = shaped.iter().map(|l| l.2 + l.3).sum();
            let mut y = if is_header { m.header * PX_PER_INCH } else { ph - m.footer * PX_PER_INCH - total };
            for (parts, w, asc, desc) in shaped {
                let bl = y + asc;
                let mut x = match si { 0 => lm, 1 => (pw - w) / 2.0, _ => rm - w };
                for (sh, sz, color, ul, strike, pic) in parts {
                    if let Some((asset, iw, ih)) = pic { cv.items.push(DrawItem::Image { asset, x, y: bl - ih, w: iw, h: ih, pos: 0, rot: 0.0, flip: (false, false) }); x += iw; continue; }
                    cv.text(&sh, x, bl, sz, color, (f32::MIN, f32::MAX), None);
                    if ul { Canvas::line(&mut cv.items, x, bl + sz * 0.12, x + sh.width, bl + sz * 0.12, sz * 0.05, color); }
                    if strike { Canvas::line(&mut cv.items, x, bl - sz * 0.3, x + sh.width, bl - sz * 0.3, sz * 0.05, color); }
                    x += sh.width;
                }
                y += asc + desc;
            }
        }
    }
}

/// Stands for a &G picture among a header's segments.
const PICTURE: &str = "\u{FFFC}";

struct Fields<'a> { page: u32, pages: u32, sheet: &'a str, info: &'a JobInfo }

fn now_serial() -> f64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    secs / 86400.0 + 25569.0
}

/// Splits header/footer text into its left, center and right sections, each as lines of
/// formatted segments, with fields filled in.
fn parse_header(src: &str, base: &Style, f: &Fields) -> [Vec<Vec<Seg>>; 3] {
    let mut out: [Vec<Vec<Seg>>; 3] = [vec![vec![]], vec![vec![]], vec![vec![]]];
    let mut sec = 1usize;
    let mut cur = Seg { text: String::new(), font: base.font.clone(), bold: false, italic: false, underline: false, strike: false, size: base.size_pt(), color: BLACK };
    let flush = |out: &mut [Vec<Vec<Seg>>; 3], sec: usize, cur: &mut Seg| {
        if !cur.text.is_empty() { let s = cur.clone(); out[sec].last_mut().unwrap().push(s); cur.text.clear(); }
    };
    let now = f.info.now.unwrap_or_else(now_serial);
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' { flush(&mut out, sec, &mut cur); out[sec].push(vec![]); i += 1; continue; }
        if c != '&' || i + 1 >= chars.len() { cur.text.push(c); i += 1; continue; }
        let d = chars[i + 1];
        i += 2;
        match d {
            'L' | 'C' | 'R' => { flush(&mut out, sec, &mut cur); sec = match d { 'L' => 0, 'C' => 1, _ => 2 }; }
            'P' => cur.text.push_str(&f.page.to_string()),
            'N' => cur.text.push_str(&f.pages.to_string()),
            'D' => cur.text.push_str(&crate::numfmt::format(&Value::Num(now.floor()), "m/d/yyyy", false).text),
            'T' => cur.text.push_str(&crate::numfmt::format(&Value::Num(now), "h:mm AM/PM", false).text),
            'F' => cur.text.push_str(&f.info.file_name),
            'Z' => cur.text.push_str(&f.info.folder),
            'A' => cur.text.push_str(f.sheet),
            '&' => cur.text.push('&'),
            'B' | 'I' | 'U' | 'E' | 'S' => {
                flush(&mut out, sec, &mut cur);
                match d { 'B' => cur.bold = !cur.bold, 'I' => cur.italic = !cur.italic, 'S' => cur.strike = !cur.strike, _ => cur.underline = !cur.underline }
            }
            '"' => {
                flush(&mut out, sec, &mut cur);
                let mut spec = String::new();
                while i < chars.len() && chars[i] != '"' { spec.push(chars[i]); i += 1; }
                i += 1;
                let (name, style) = spec.split_once(',').unwrap_or((&spec, ""));
                if name.trim() != "-" && !name.trim().is_empty() { cur.font = name.trim().to_string(); }
                let st = style.to_ascii_lowercase();
                if !st.is_empty() { cur.bold = st.contains("bold"); cur.italic = st.contains("italic") || st.contains("oblique"); }
            }
            'K' => {
                flush(&mut out, sec, &mut cur);
                let hex: String = chars[i..].iter().take(6).collect();
                if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                    let v = u32::from_str_radix(&hex, 16).unwrap_or(0);
                    cur.color = Color((v >> 16) as u8, (v >> 8) as u8, v as u8);
                }
                i += 6.min(chars.len() - i);
            }
            '0'..='9' => {
                flush(&mut out, sec, &mut cur);
                let mut n = d.to_string();
                while i < chars.len() && chars[i].is_ascii_digit() { n.push(chars[i]); i += 1; }
                if let Ok(v) = n.parse::<f32>() { if v > 0.0 { cur.size = v; } }
            }
            'G' => { flush(&mut out, sec, &mut cur); let pic = Seg { text: PICTURE.into(), ..cur.clone() }; out[sec].last_mut().unwrap().push(pic); }
            _ => {} // &O outline, &H shadow, &X &Y scripts: not drawn.
        }
    }
    flush(&mut out, sec, &mut cur);
    out
}

/// Draws pages as a layout for word-engine's PDF writer and SVG renderer.
pub fn layout(wb: &Workbook, pages: &[PrintPage], info: &JobInfo) -> Layout {
    render(wb, pages, info).0
}

/// The pages as a layout plus a document holding the pictures they show (for the PDF writer and
/// SVG renderer).
pub fn render(wb: &Workbook, pages: &[PrintPage], info: &JobInfo) -> (Layout, word_engine::Document) {
    let mut assets = objects::Assets::new();
    let lay = Layout { pages: pages.iter().map(|p| draw_page_with(wb, p, info, &mut assets)).collect(), ..Default::default() };
    let mut doc = word_engine::Document::default();
    doc.assets.extend(assets);
    (lay, doc)
}

/// A PDF of the pages (repeated `copies` times), with embedded fonts and searchable text.
pub fn pdf(wb: &Workbook, pages: &[PrintPage], info: &JobInfo, title: &str, copies: usize) -> Vec<u8> {
    let (lay, doc) = render(wb, pages, info);
    let order: Vec<usize> = (0..copies.max(1)).flat_map(|_| 0..lay.pages.len()).collect();
    word_engine::pdf::export_pages(&doc, &lay, title, &order)
}

/// A PDF of what a job prints; None when there is nothing to print.
pub fn pdf_of(wb: &Workbook, what: &What, info: &JobInfo, title: &str) -> Option<Vec<u8>> {
    let pg = pages(wb, what);
    (!pg.is_empty()).then(|| pdf(wb, &pg, info, title, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(rows: u32, cols: u32) -> Workbook {
        let mut wb = Workbook::new();
        for r in 0..rows { for c in 0..cols { wb.set_input(0, Pos::new(r, c), &format!("{}", r * cols + c)); } }
        wb
    }

    #[test]
    fn paginates_down_then_over() {
        let wb = book(100, 20);
        let pg = sheet_pages(&wb, 0, None);
        // Letter portrait: 7.1 × 9.5 inches usable = 681.6 × 912 px: 10 columns of 64 px, 45 rows of 20 px.
        assert_eq!(pg[0].cols, (0, 9));
        assert_eq!(pg[0].rows, (0, 44));
        assert_eq!(pg[1].rows, (45, 89));
        assert_eq!(pg[1].cols, (0, 9));
        assert_eq!(pg.len(), 6);
        assert_eq!((pg[5].number, pg[5].count), (6, 6));
    }

    #[test]
    fn manual_breaks_titles_and_order() {
        let mut wb = book(60, 3);
        wb.sheets[0].page.row_breaks.insert(10);
        wb.sheets[0].page.over_then_down = true;
        wb.set_print_titles(0, Some((0, 0)), None);
        assert_eq!(wb.print_titles(0), (Some((0, 0)), None));
        let pg = sheet_pages(&wb, 0, None);
        assert_eq!(pg[0].rows, (0, 9));
        assert_eq!(pg[1].rows.0, 10);
        assert_eq!(pg[1].title_rows, Some((0, 0)));
        assert_eq!(pg[0].title_rows, None);
        // Print area limits what prints.
        wb.set_print_area(0, Some(&[parse_range("A1:B5").unwrap()]));
        assert_eq!(wb.print_area(0), Some(vec![parse_range("A1:B5").unwrap()]));
        assert_eq!(wb.names.iter().find(|n| n.name == AREA).unwrap().formula, "Sheet1!$A$1:$B$5");
        let pg = sheet_pages(&wb, 0, None);
        assert_eq!((pg.len(), pg[0].rows, pg[0].cols), (1, (0, 4), (0, 1)));
        wb.set_print_area(0, None);
        assert_eq!(wb.print_area(0), None);
    }

    #[test]
    fn fit_to_one_page() {
        let mut wb = book(200, 30);
        wb.sheets[0].page.fit = true;
        wb.sheets[0].page.fit_width = 1;
        wb.sheets[0].page.fit_height = 1;
        let pg = sheet_pages(&wb, 0, None);
        assert_eq!(pg.len(), 1);
        assert!(pg[0].scale < 0.3);
        wb.sheets[0].page.fit_height = 0;
        let pg = sheet_pages(&wb, 0, None);
        assert!(pg.iter().all(|p| p.cols == (0, 29)) && pg.len() > 1);
    }

    #[test]
    fn empty_sheet_prints_nothing() {
        assert!(pdf_of(&Workbook::new(), &What::ActiveSheet, &JobInfo::default(), "x").is_none());
    }

    #[test]
    fn header_codes() {
        let info = JobInfo { file_name: "Book.xlsx".into(), now: Some(45292.5), ..Default::default() };
        let f = Fields { page: 2, pages: 5, sheet: "Data", info: &info };
        let s = parse_header("&LPage &P of &N&C&\"Arial,Bold\"&14&A&R&D &T &F &&", &Style::default(), &f);
        let text = |v: &Vec<Vec<Seg>>| v[0].iter().map(|s| s.text.clone()).collect::<String>();
        assert_eq!(text(&s[0]), "Page 2 of 5");
        assert_eq!(s[1][0][0].text, "Data");
        assert!(s[1][0][0].bold && s[1][0][0].size == 14.0 && s[1][0][0].font == "Arial");
        assert_eq!(text(&s[2]), "1/1/2024 12:00 PM Book.xlsx &");
    }

    #[test]
    fn writes_pdf() {
        let mut wb = book(80, 12);
        wb.set_input(0, Pos::new(0, 0), "مرحبا Hello, a long text that overflows into its neighbours");
        wb.set_input(0, Pos::new(0, 1), "");
        wb.apply_style(0, &[parse_range("A1:C3").unwrap()], &|s| { s.bold = true; s.fill = Some([255, 255, 0]); s.top.style = LineStyle::Dashed; s.bottom.style = LineStyle::Double; });
        wb.merge(0, parse_range("D2:F4").unwrap(), false);
        let p = &mut wb.sheets[0].page;
        p.gridlines = true; p.headings = true; p.landscape = true;
        p.header_footer.odd_footer = "&CPage &P of &N".into();
        let bytes = pdf_of(&wb, &What::ActiveSheet, &JobInfo::default(), "Test").unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-") && text.trim_end().ends_with("%%EOF"));
        let n = text.matches("/Type /Page ").count();
        assert_eq!(n, sheet_pages(&wb, 0, None).len());
        assert!(text.contains("/FontFile2") && text.contains("/ToUnicode"));
        // Landscape pages are wider than tall: 11 × 8.5 inches.
        assert!(text.contains("/MediaBox [0 0 792 612]"));
        let xref_at: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        assert!(bytes[xref_at..].starts_with(b"xref"));
        let (rows, cols) = break_lines(&wb, 0);
        assert!(!rows.is_empty() && cols.is_empty());
    }

    #[test]
    fn page_setup_xml_round_trip() {
        let mut p = PageSetup { paper: 9, landscape: true, scale: 80, fit: true, fit_width: 1, fit_height: 0, first_page: Some(3), over_then_down: true,
            gridlines: true, headings: true, center_h: true, margins: Margins::WIDE, printer_rel: Some("rId1".into()), ..Default::default() };
        p.header_footer.odd_header = "&L&A & <x>".into();
        p.header_footer.different_first = true;
        p.row_breaks.insert(20);
        p.col_breaks.extend([5, 9]);
        let mut back = PageSetup { fit: true, ..Default::default() };
        for (n, x) in p.write_xml() {
            assert!(back.read_xml(n, &x), "{n}");
        }
        assert_eq!(back, p);
    }

    #[test]
    fn xlsx_round_trip() {
        let mut wb = book(30, 4);
        wb.add_sheet(1, Some("My Data".into()));
        let p = &mut wb.sheets[0].page;
        p.paper = 9; p.landscape = true; p.fit = true; p.fit_height = 0; p.gridlines = true; p.center_v = true;
        p.margins = Margins::NARROW;
        p.header_footer.odd_footer = "&LPage &P&R&D".into();
        p.row_breaks.insert(12);
        wb.set_print_area(0, Some(&[parse_range("A1:C20").unwrap()]));
        wb.set_print_titles(1, Some((0, 1)), Some((0, 0)));
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        assert_eq!(back.sheets[0].page, wb.sheets[0].page);
        assert_eq!(back.print_area(0), wb.print_area(0));
        assert_eq!(back.print_titles(1), (Some((0, 1)), Some((0, 0))));
        assert!(back.sheets[0].extra.iter().all(|(n, _)| !matches!(n.as_str(), "pageSetup" | "pageMargins" | "rowBreaks")));
    }
}
