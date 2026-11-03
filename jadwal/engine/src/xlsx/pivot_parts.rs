//! Native PivotTable parts: each `Sheet.pivots` entry is written as a pivot cache definition
//! (with its records) and a pivot table definition that Excel opens as a working, refreshable
//! PivotTable (the cache refreshes on load); the computed report stays in the cells as well.
//! Reading turns pivot tables the model can represent (fields in rows, columns, filters and
//! values summed, counted, averaged, max or min, over a worksheet range or table) back into
//! `Pivot`s and drops their parts; others are carried untouched.
//!
//! Writing happens on the copy `drawing::prepare` makes: the generated parts go into
//! `Workbook.parts` with their relationships parts, the sheets get "rel" entries, and the
//! reference copies of xl/workbook.xml, its rels and [Content_Types].xml are updated so the
//! package writer emits `<pivotCaches>`, the workbook relationships and the content types.

use super::read::{rels_name, resolve};
use super::xml::{attr, part_text, Element, Node};
use crate::addr::{Pos, Range};
use crate::book::Workbook;
use crate::pivot::{cmp_item, key_text, Agg, BaseItem, CalcField, DateLevel, ExtraField, Grouping, Layout, Pivot, PivotStyle, Role, ShowAs, Subtotals, ValueField};
use crate::value::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(super) const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub(super) const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(super) const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
pub(super) const NS_X15: &str = "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main";
const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.";
const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
pub(super) const RT_SLICER: &str = "http://schemas.microsoft.com/office/2007/relationships/slicer";
pub(super) const RT_SLICER_CACHE: &str = "http://schemas.microsoft.com/office/2007/relationships/slicerCache";
/// Workbook extensions listing slicer caches: pivot (x14) and table (x15) ones.
pub(super) const EXT_WB_SLICERS: &str = "{BBE1A952-AA13-448e-AADC-164F8A28A991}";
pub(super) const EXT_WB_TABLE_SLICERS: &str = "{46BE6895-7355-4a93-B00E-2C351335B9C9}";

/// A relative target from the part `from` to the part `to`.
pub(super) fn relative(from: &str, to: &str) -> String {
    let a: Vec<&str> = from.split('/').collect();
    let b: Vec<&str> = to.split('/').collect();
    let dir = &a[..a.len() - 1];
    let common = dir.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let mut out: Vec<String> = std::iter::repeat_n("..".to_string(), dir.len() - common).collect();
    out.extend(b[common..].iter().map(|s| s.to_string()));
    out.join("/")
}

/// A relationships part with the given (id, type, relative target) entries.
pub(super) fn rels_xml(items: &[(String, String, String)]) -> String {
    let mut o = format!("{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for (id, kind, target) in items { o.push_str(&format!("<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"/>", attr(id), attr(kind), attr(target))); }
    o.push_str("</Relationships>");
    o
}

/// Relationships of a part, targets resolved: (id, full type, target).
pub(super) fn part_rels(wb: &Workbook, part: &str) -> Vec<(String, String, String)> {
    let Some(e) = wb.parts.get(&rels_name(part)).and_then(|b| Element::parse(&part_text(b)).ok()) else { return vec![] };
    e.all("Relationship").filter(|r| r.attr("TargetMode") != Some("External")).filter_map(|r| Some((r.attr("Id")?.to_string(), r.attr("Type")?.to_string(), resolve(part, r.attr("Target")?)))).collect()
}

pub(super) fn parse_part(wb: &Workbook, part: &str) -> Option<Element> { wb.parts.get(part).and_then(|b| Element::parse(&part_text(b)).ok()) }

/// A fresh relationship id for a sheet's carried relationships.
pub(super) fn sheet_rel(wb: &mut Workbook, si: usize, kind: &str, target: &str, base: &str) -> String {
    let s = &mut wb.sheets[si];
    let used: HashSet<String> = s.extra.iter().filter(|(k, _)| k == "rel").filter_map(|(_, v)| v.split('\t').next().map(String::from)).collect();
    let id = (1..).map(|i| format!("{base}{i}")).find(|i| !used.contains(i)).unwrap();
    s.extra.push(("rel".into(), format!("{id}\t{kind}\t{target}\t0")));
    id
}

// ---- the model's view of a pivot's source ----------------------------------------------------------

/// One field's distinct items: values in order of first appearance (the cache's shared items, or
/// a grouped field's group items), their key texts, and the indexes in the order the report shows them.
pub(crate) struct FieldItems { pub values: Vec<Value>, pub texts: Vec<String>, pub sorted: Vec<usize> }

impl FieldItems {
    pub fn index(&self, text: &str) -> Option<usize> { self.texts.iter().position(|t| t == text) }
    /// The position of an item in report order.
    fn pos(&self, text: &str) -> usize { let s = self.index(text).unwrap_or(0); self.sorted.iter().position(|x| *x == s).unwrap_or(0) }
}

/// The source records (every row under the header).
fn records(wb: &Workbook, p: &Pivot) -> Option<Vec<Vec<Value>>> {
    let si = wb.sheet_index(&p.source_sheet)?;
    let s = &wb.sheets[si];
    let src = p.source;
    Some((src.r0 + 1..=src.r1).map(|r| (src.c0..=src.c1).map(|c| s.value(Pos::new(r, c))).collect()).collect())
}

/// A source field's raw items (the cache's shared items).
fn shared_items(wb: &Workbook, p: &Pivot, recs: &[Vec<Value>], f: usize) -> FieldItems {
    let mut values = vec![];
    let mut texts: Vec<String> = vec![];
    let mut seen: HashMap<String, usize> = HashMap::new();
    for r in recs {
        let v = r.get(f).cloned().unwrap_or_default();
        let t = key_text(&v);
        if !seen.contains_key(&t) { seen.insert(t.clone(), texts.len()); texts.push(t); values.push(v); }
    }
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|a, b| cmp_item(&(texts[*a].clone(), values[*a].clone(), None), &(texts[*b].clone(), values[*b].clone(), None)));
    let _ = wb;
    let _ = p;
    FieldItems { values, texts, sorted: order }
}

/// A field's items on an axis: a grouped field's group items (in order), else its shared items.
fn axis_items(wb: &Workbook, p: &Pivot, recs: &[Vec<Value>], f: usize) -> FieldItems {
    match p.group_of(f) {
        Some(Grouping::Discrete(groups)) => {
            let base = shared_items(wb, p, recs, p.base_of(f));
            let mut texts: Vec<String> = vec![];
            for s in &base.sorted {
                let t = &base.texts[*s];
                let l = groups.iter().find(|g| g.1.contains(t)).map(|g| g.0.clone()).unwrap_or(t.clone());
                if !texts.contains(&l) { texts.push(l); }
            }
            for g in groups { if !texts.contains(&g.0) { texts.push(g.0.clone()); } }
            let sorted = (0..texts.len()).collect();
            FieldItems { values: texts.iter().map(Value::str).collect(), texts, sorted }
        }
        Some(g) => { let texts = g.all_items(wb.date1904); FieldItems { values: texts.iter().map(Value::str).collect(), sorted: (0..texts.len()).collect(), texts } }
        None => shared_items(wb, p, recs, f),
    }
}

/// A field's items as the native parts number them (None when the pivot isn't written natively).
pub(crate) fn field_items(wb: &Workbook, p: &Pivot, f: usize) -> Option<FieldItems> {
    if !writable(wb, p) || p.calc_index(f).is_some() || f >= p.n_fields() { return None; }
    let p = p.refreshed(wb);
    let recs = records(wb, &p)?;
    Some(axis_items(wb, &p, &recs, f))
}

/// Whether a pivot is written as native parts: its source exists and it has a field placed.
pub(crate) fn writable(wb: &Workbook, p: &Pivot) -> bool {
    let n = p.n_fields();
    wb.sheet_index(&p.source_sheet).is_some() && p.source.r1 > p.source.r0
        && !(p.rows.is_empty() && p.cols.is_empty() && p.values.is_empty() && p.filters.is_empty())
        && p.rows.iter().chain(&p.cols).chain(&p.filters).chain(p.values.iter().map(|v| &v.field)).all(|f| *f < n)
        && p.extra.iter().all(|e| e.base < p.n_source())
}

fn num(n: f64) -> String { let a = n.abs(); if a != 0.0 && !(1e-7..1e21).contains(&a) { format!("{n:e}") } else { format!("{n}") } }

fn item_xml(v: &Value) -> String {
    match v {
        Value::Num(n) if n.is_finite() => format!("<n v=\"{}\"/>", num(*n)),
        Value::Str(s) => format!("<s v=\"{}\"/>", attr(s)),
        Value::Bool(b) => format!("<b v=\"{}\"/>", *b as u8),
        Value::Err(e) => format!("<e v=\"{}\"/>", attr(e.text())),
        Value::Num(_) => "<e v=\"#NUM!\"/>".into(),
        _ => "<m/>".into(),
    }
}

/// A serial as the file's date-time text.
pub(super) fn iso(s: f64, d1904: bool) -> String {
    let (y, m, d) = crate::date::ymd(s, d1904);
    let (h, mi, se, _) = crate::date::hms(s);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{se:02}")
}

pub(super) fn parse_iso(t: &str, d1904: bool) -> Option<f64> {
    let y: i64 = t.get(0..4)?.parse().ok()?;
    let m: i64 = t.get(5..7)?.parse().ok()?;
    let d: i64 = t.get(8..10)?.parse().ok()?;
    let mut s = crate::date::serial(y, m, d, d1904);
    if let Some(tm) = t.get(11..19) {
        let p: Vec<f64> = tm.split(':').filter_map(|x| x.parse().ok()).collect();
        if p.len() == 3 { s += (p[0] * 3600.0 + p[1] * 60.0 + p[2]) / 86400.0; }
    }
    Some(s)
}

/// sharedItems' type flags for a list of values.
fn shared_attrs<'a>(vals: impl Iterator<Item = &'a Value>) -> String {
    let (mut s, mut n, mut b, mut e, mut m, mut int) = (false, false, false, false, false, true);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in vals {
        match v {
            Value::Num(x) if x.is_finite() => { n = true; lo = lo.min(*x); hi = hi.max(*x); if x.fract() != 0.0 { int = false; } }
            Value::Str(_) => s = true, Value::Bool(_) => b = true, Value::Err(_) | Value::Num(_) => e = true, _ => m = true,
        }
    }
    let mut a = String::new();
    if !(s || b || e) { if !m { a.push_str(" containsSemiMixedTypes=\"0\""); } a.push_str(" containsString=\"0\""); }
    if m { a.push_str(" containsBlank=\"1\""); }
    if [s || b || e, n].iter().filter(|x| **x).count() > 1 { a.push_str(" containsMixedTypes=\"1\""); }
    if n { a.push_str(" containsNumber=\"1\""); if int { a.push_str(" containsInteger=\"1\""); } a.push_str(&format!(" minValue=\"{}\" maxValue=\"{}\"", num(lo), num(hi))); }
    a
}

fn x(v: usize) -> String { if v == 0 { "<x/>".into() } else { format!("<x v=\"{v}\"/>") } }

/// Unique cache field names (Excel requires them distinct).
pub(super) fn unique_names(names: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    names.into_iter().map(|n| {
        let mut c = n.clone();
        let mut k = 2;
        while !seen.insert(c.to_lowercase()) { c = format!("{n}{k}"); k += 1; }
        c
    }).collect()
}

const PREV_ITEM: u32 = 1048828;
const NEXT_ITEM: u32 = 1048829;
const EXT_X14_PIVOT_FIELD: &str = "{2946ED86-A175-432a-8AC1-64E0C546D7DE}";
const EXT_X14_DATA_FIELD: &str = "{E15A36E0-9728-4e99-A89B-3F7291B0FE68}";

/// The field a grouped field rolls up into (the file's `par`): the next coarser date level, or a discrete grouping.
fn parent_field(p: &Pivot, f: usize) -> Option<usize> {
    let n = p.n_source();
    let b = p.base_of(f);
    let level = |g: &Grouping| if let Grouping::Date { level, .. } = g { Some(*level) } else { None };
    let mine = p.group_of(f).and_then(level);
    let mut sib = p.extra.iter().enumerate().filter(|(_, e)| e.base == b);
    match mine {
        Some(l) => sib.filter_map(|(j, e)| level(&e.group).filter(|x| *x > l).map(|x| (x, n + j))).min().map(|x| x.1),
        None if f < n => sib.find(|(_, e)| matches!(e.group, Grouping::Discrete(_))).map(|(j, _)| n + j),
        None => None,
    }
}

fn group_xml(p: &Pivot, f: usize, d1904: bool, items: &FieldItems, base_shared: Option<&FieldItems>) -> String {
    let par = parent_field(p, f).map(|x| format!(" par=\"{x}\"")).unwrap_or_default();
    let Some(g) = p.group_of(f) else { return if par.is_empty() { String::new() } else { format!("<fieldGroup{par}/>") } };
    let mut o = format!("<fieldGroup{par} base=\"{}\">", p.base_of(f));
    let auto = |a: bool, b: bool| format!("{}{}", if a { "" } else { " autoStart=\"0\"" }, if b { "" } else { " autoEnd=\"0\"" });
    match g {
        Grouping::Range { start, end, by, auto_start, auto_end } => o.push_str(&format!("<rangePr{} startNum=\"{}\" endNum=\"{}\" groupInterval=\"{}\"/>", auto(*auto_start, *auto_end), num(*start), num(*end), num(*by))),
        Grouping::Date { level, start, end, by, auto_start, auto_end } => {
            o.push_str(&format!("<rangePr{} groupBy=\"{}\" startDate=\"{}\" endDate=\"{}\"", auto(*auto_start, *auto_end), level.code(), iso(*start, d1904), iso(end.floor() + 1.0, d1904)));
            if *by > 1.0 { o.push_str(&format!(" groupInterval=\"{}\"", num(*by))); }
            o.push_str("/>");
        }
        Grouping::Discrete(groups) => {
            let base = base_shared.map(|b| b.texts.as_slice()).unwrap_or(&[]);
            o.push_str(&format!("<discretePr count=\"{}\">", base.len()));
            for t in base { let l = groups.iter().find(|g| g.1.contains(t)).map(|g| g.0.as_str()).unwrap_or(t); o.push_str(&x(items.index(l).unwrap_or(0))); }
            o.push_str("</discretePr>");
        }
    }
    o.push_str(&format!("<groupItems count=\"{}\">", items.texts.len()));
    for t in &items.texts { o.push_str(&format!("<s v=\"{}\"/>", attr(t))); }
    o.push_str("</groupItems></fieldGroup>");
    o
}

/// The cache definition, records and table definition of one pivot.
fn pivot_xml(wb: &Workbook, p0: &Pivot, cache_id: u32) -> Option<(String, String, String)> {
    // The report rows follow the timelines' selection even when the model wasn't synced.
    let mut p1 = p0.clone();
    if p1.date_filter.is_none() { p1.date_filter = super::timeline_parts::pivot_filters(wb, &p0.name).first().copied(); }
    let p = &p1.refreshed(wb);
    let d1904 = wb.date1904;
    let recs = records(wb, p)?;
    let n = p.n_source();
    let nx = p.extra.len();
    let nf = p.n_fields();
    let names = unique_names(p.fields(wb));
    // Fields that list items: on an axis, filtered, used by a slicer, or grouped.
    let mut itemized: BTreeSet<usize> = p.rows.iter().chain(&p.cols).chain(&p.filters).copied().collect();
    itemized.extend(p.hidden.iter().filter(|(_, h)| !h.is_empty()).map(|(f, _)| *f));
    itemized.extend(super::slicer_parts::pivot_slicer_fields(wb, &p.name));
    itemized.extend(p.groups.keys().copied());
    itemized.extend(n..n + nx);
    itemized.retain(|f| *f < n + nx);
    let items: BTreeMap<usize, FieldItems> = itemized.iter().map(|f| (*f, axis_items(wb, p, &recs, *f))).collect();
    // Source fields whose raw values are shared items (not range or date grouped).
    let mut shared_set: BTreeSet<usize> = itemized.iter().copied().filter(|f| *f < n && !p.groups.contains_key(f)).collect();
    shared_set.extend(p.extra.iter().filter(|e| matches!(e.group, Grouping::Discrete(_))).map(|e| e.base));
    let shared: BTreeMap<usize, FieldItems> = shared_set.iter().map(|f| (*f, shared_items(wb, p, &recs, *f))).collect();
    let tl_fields = super::timeline_parts::pivot_timeline_fields(wb, &p.name);
    let as_date = |f: usize| (matches!(p.groups.get(&f), Some(Grouping::Date { .. })) || tl_fields.contains(&f)) && recs.iter().all(|r| matches!(r.get(f), Some(Value::Num(_)) | Some(Value::Empty) | None));
    // Cache definition.
    let mut cd = format!("{DECL}<pivotCacheDefinition xmlns=\"{NS}\" xmlns:r=\"{NS_R}\" r:id=\"rId1\" refreshOnLoad=\"1\" refreshedBy=\"Jadwal\" createdVersion=\"6\" refreshedVersion=\"6\" minRefreshableVersion=\"3\" recordCount=\"{}\">", recs.len());
    cd.push_str(&format!("<cacheSource type=\"worksheet\"><worksheetSource ref=\"{}\" sheet=\"{}\"/></cacheSource>", p.source.a1(), attr(&p.source_sheet)));
    cd.push_str(&format!("<cacheFields count=\"{nf}\">"));
    for f in 0..n {
        cd.push_str(&format!("<cacheField name=\"{}\" numFmtId=\"{}\">", attr(&names[f]), if as_date(f) { 14 } else { 0 }));
        if let Some(it) = shared.get(&f) {
            cd.push_str(&format!("<sharedItems{} count=\"{}\">", shared_attrs(it.values.iter()), it.values.len()));
            for v in &it.values { cd.push_str(&item_xml(v)); }
            cd.push_str("</sharedItems>");
        } else if as_date(f) {
            let nums: Vec<f64> = recs.iter().filter_map(|r| if let Some(Value::Num(v)) = r.get(f) { Some(*v) } else { None }).collect();
            let blank = nums.len() < recs.len();
            let (lo, hi) = (nums.iter().cloned().fold(f64::INFINITY, f64::min), nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
            cd.push_str(&format!("<sharedItems containsSemiMixedTypes=\"0\" containsNonDate=\"0\" containsDate=\"1\" containsString=\"0\"{}", if blank { " containsBlank=\"1\"" } else { "" }));
            if !nums.is_empty() { cd.push_str(&format!(" minDate=\"{}\" maxDate=\"{}\"", iso(lo, d1904), iso(hi + 1.0, d1904))); }
            cd.push_str("/>");
        } else {
            let a = shared_attrs(recs.iter().filter_map(|r| r.get(f)));
            cd.push_str(&format!("<sharedItems{a}/>"));
        }
        if let Some(it) = items.get(&f) { cd.push_str(&group_xml(p, f, d1904, it, shared.get(&f))); }
        cd.push_str("</cacheField>");
    }
    for (j, e) in p.extra.iter().enumerate() {
        let f = n + j;
        cd.push_str(&format!("<cacheField name=\"{}\" numFmtId=\"0\" databaseField=\"0\">", attr(&names[f])));
        cd.push_str(&group_xml(p, f, d1904, &items[&f], shared.get(&e.base)));
        cd.push_str("</cacheField>");
    }
    for (c, calc) in p.calcs.iter().enumerate() {
        cd.push_str(&format!("<cacheField name=\"{}\" numFmtId=\"0\" formula=\"{}\" databaseField=\"0\"/>", attr(&names[n + nx + c]), attr(&calc.formula)));
    }
    cd.push_str("</cacheFields>");
    cd.push_str(&format!("<extLst><ext uri=\"{{725AE2AE-9491-48be-B2B4-4EB974FC3084}}\" xmlns:x14=\"{NS_X14}\"><x14:pivotCacheDefinition pivotCacheId=\"{cache_id}\"/></ext></extLst>"));
    cd.push_str("</pivotCacheDefinition>");
    // Records.
    let mut rc = format!("{DECL}<pivotCacheRecords xmlns=\"{NS}\" xmlns:r=\"{NS_R}\" count=\"{}\">", recs.len());
    for r in &recs {
        rc.push_str("<r>");
        for f in 0..n {
            let v = r.get(f).cloned().unwrap_or_default();
            match shared.get(&f) {
                Some(it) => rc.push_str(&format!("<x v=\"{}\"/>", it.index(&key_text(&v)).unwrap_or(0))),
                None => match &v { Value::Num(d) if as_date(f) => rc.push_str(&format!("<d v=\"{}\"/>", iso(*d, d1904))), _ => rc.push_str(&item_xml(&v)) },
            }
        }
        rc.push_str("</r>");
    }
    rc.push_str("</pivotCacheRecords>");
    // Table definition: layout as the model computes it.
    let rep = p.compute(wb);
    let pages = if p.filters.is_empty() { 0 } else { p.filters.len() + 1 };
    let body = &rep.cells[pages.min(rep.cells.len())..];
    let width = body.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let r0 = p.anchor.row + pages as u32;
    let loc = Range { r0, c0: p.anchor.col, r1: r0 + body.len().max(1) as u32 - 1, c1: p.anchor.col + width as u32 - 1 };
    let multi = p.values.len() > 1;
    let lead = rep.lead.max(1);
    let (fhr, fdr) = if p.cols.is_empty() { (if multi { 0 } else { 1 }, 1) } else { (1, 1 + p.cols.len() + multi as usize) };
    let hidden = |f: usize| p.hidden.get(&f).cloned().unwrap_or_default();
    let (table_layout, field_layout) = match p.layout {
        Layout::Compact => (" outline=\"1\" outlineData=\"1\"", ""),
        Layout::Outline => (" compact=\"0\" compactData=\"0\" outline=\"1\" outlineData=\"1\"", " compact=\"0\""),
        Layout::Tabular => (" compact=\"0\" compactData=\"0\" gridDropZones=\"0\"", " compact=\"0\" outline=\"0\""),
    };
    let mut t = format!("{DECL}<pivotTableDefinition xmlns=\"{NS}\" name=\"{}\" cacheId=\"{cache_id}\" applyNumberFormats=\"0\" applyBorderFormats=\"0\" applyFontFormats=\"0\" applyPatternFormats=\"0\" applyAlignmentFormats=\"0\" applyWidthHeightFormats=\"1\" dataCaption=\"Values\"{}{} updatedVersion=\"6\" minRefreshableVersion=\"3\" useAutoFormatting=\"1\" itemPrintTitles=\"1\" createdVersion=\"6\" indent=\"0\"{table_layout} multipleFieldFilters=\"0\">",
        attr(&p.name), if p.row_grand { "" } else { " rowGrandTotals=\"0\"" }, if p.col_grand { "" } else { " colGrandTotals=\"0\"" });
    t.push_str(&format!("<location ref=\"{}\" firstHeaderRow=\"{fhr}\" firstDataRow=\"{fdr}\" firstDataCol=\"{lead}\"", loc.a1()));
    if !p.filters.is_empty() { t.push_str(&format!(" rowPageCount=\"{}\" colPageCount=\"1\"", p.filters.len())); }
    t.push_str("/>");
    t.push_str(&format!("<pivotFields count=\"{nf}\">"));
    for f in 0..nf {
        if p.calc_index(f).is_some() { t.push_str("<pivotField dataField=\"1\" dragToRow=\"0\" dragToCol=\"0\" dragToPage=\"0\" showAll=\"0\" defaultSubtotal=\"0\"/>"); continue; }
        t.push_str("<pivotField");
        let (row, col, page) = (p.rows.contains(&f), p.cols.contains(&f), p.filters.contains(&f));
        if row { t.push_str(" axis=\"axisRow\""); } else if col { t.push_str(" axis=\"axisCol\""); } else if page { t.push_str(" axis=\"axisPage\""); }
        if p.values.iter().any(|v| v.field == f) { t.push_str(" dataField=\"1\""); }
        t.push_str(field_layout);
        if as_date(f) { t.push_str(" numFmtId=\"14\""); }
        t.push_str(" showAll=\"0\"");
        let h = hidden(f);
        let single = page.then(|| items.get(&f)).flatten().and_then(|it| { let on: Vec<usize> = it.sorted.iter().copied().filter(|s| !h.contains(&it.texts[*s])).collect(); (!h.is_empty() && on.len() == 1).then(|| on[0]) });
        if page && !h.is_empty() && single.is_none() { t.push_str(" multipleItemSelectionAllowed=\"1\""); }
        let sub = !col && !(row && p.subtotals == Subtotals::Off);
        if !sub { t.push_str(" defaultSubtotal=\"0\""); }
        if row && sub && (p.subtotals == Subtotals::Bottom || p.layout == Layout::Tabular) { t.push_str(" subtotalTop=\"0\""); }
        if row && p.blank_rows { t.push_str(" insertBlankRow=\"1\""); }
        let ext = if row && p.repeat_labels { format!("<extLst><ext uri=\"{EXT_X14_PIVOT_FIELD}\" xmlns:x14=\"{NS_X14}\"><x14:pivotField fillDownLabels=\"1\"/></ext></extLst>") } else { String::new() };
        match items.get(&f) {
            Some(it) => {
                t.push_str(&format!("><items count=\"{}\">", it.sorted.len() + sub as usize));
                for s in &it.sorted { t.push_str(&format!("<item{} x=\"{s}\"/>", if single.is_none() && h.contains(&it.texts[*s]) { " h=\"1\"" } else { "" })); }
                if sub { t.push_str("<item t=\"default\"/>"); }
                t.push_str(&format!("</items>{ext}</pivotField>"));
            }
            None if !ext.is_empty() => t.push_str(&format!(">{ext}</pivotField>")),
            None => t.push_str("/>"),
        }
    }
    t.push_str("</pivotFields>");
    // Row items from the report's rows.
    if !p.rows.is_empty() {
        t.push_str(&format!("<rowFields count=\"{}\">", p.rows.len()));
        for f in &p.rows { t.push_str(&format!("<field x=\"{f}\"/>")); }
        t.push_str("</rowFields>");
    }
    let row_pos = |key: &[String]| -> Vec<usize> { key.iter().enumerate().map(|(d, s)| items.get(&p.rows[d]).map_or(0, |it| it.pos(s))).collect() };
    let mut ri: Vec<String> = vec![];
    if !p.rows.is_empty() {
        let mut prev: Vec<usize> = vec![];
        for (i, role) in rep.roles.iter().enumerate() {
            let key = row_pos(&rep.keys[i]);
            match role {
                Role::Data | Role::Group if !key.is_empty() => {
                    let r = prev.iter().zip(&key).take_while(|(a, b)| a == b).count().min(key.len() - 1);
                    ri.push(format!("<i{}>{}</i>", if r > 0 { format!(" r=\"{r}\"") } else { String::new() }, key[r..].iter().map(|v| x(*v)).collect::<String>()));
                    prev = key;
                }
                Role::Subtotal => ri.push(format!("<i t=\"default\">{}</i>", key.iter().map(|v| x(*v)).collect::<String>())),
                Role::Blank if !key.is_empty() => ri.push(format!("<i t=\"blank\">{}</i>", key.iter().map(|v| x(*v)).collect::<String>())),
                Role::Grand => ri.push("<i t=\"grand\"><x/></i>".into()),
                _ => {}
            }
        }
    } else { ri.push("<i/>".into()); }
    t.push_str(&format!("<rowItems count=\"{}\">{}</rowItems>", ri.len(), ri.concat()));
    let mut cf: Vec<i64> = p.cols.iter().map(|f| *f as i64).collect();
    if multi { cf.push(-2); }
    if !cf.is_empty() {
        t.push_str(&format!("<colFields count=\"{}\">", cf.len()));
        for f in &cf { t.push_str(&format!("<field x=\"{f}\"/>")); }
        t.push_str("</colFields>");
    }
    let nv = p.values.len().max(1);
    let mut ci: Vec<String> = vec![];
    let ia = |v: usize| if v > 0 { format!(" i=\"{v}\"") } else { String::new() };
    if p.cols.is_empty() {
        if multi { for v in 0..nv { ci.push(format!("<i{}>{}</i>", ia(v), x(v))); } } else { ci.push("<i/>".into()); }
    } else {
        let mut prev: Vec<usize> = vec![];
        for ck in &rep.col_keys {
            let key: Vec<usize> = ck.iter().enumerate().map(|(d, s)| items.get(&p.cols[d]).map_or(0, |it| it.pos(s))).collect();
            for v in 0..if multi { nv } else { 1 } {
                let mut m = key.clone();
                if multi { m.push(v); }
                let r = prev.iter().zip(&m).take_while(|(a, b)| a == b).count().min(m.len() - 1);
                ci.push(format!("<i{}{}>{}</i>", if r > 0 { format!(" r=\"{r}\"") } else { String::new() }, if multi { ia(v) } else { String::new() }, m[r..].iter().map(|v| x(*v)).collect::<String>()));
                prev = m;
            }
        }
        if p.row_grand { for v in 0..if multi { nv } else { 1 } { ci.push(format!("<i t=\"grand\"{}>{}</i>", ia(v), x(v))); } }
    }
    t.push_str(&format!("<colItems count=\"{}\">{}</colItems>", ci.len(), ci.concat()));
    if !p.filters.is_empty() {
        t.push_str(&format!("<pageFields count=\"{}\">", p.filters.len()));
        for f in &p.filters {
            let h = hidden(*f);
            let one = items.get(f).and_then(|it| { let on: Vec<usize> = (0..it.sorted.len()).filter(|k| !h.contains(&it.texts[it.sorted[*k]])).collect(); (!h.is_empty() && on.len() == 1).then(|| on[0]) });
            t.push_str(&format!("<pageField fld=\"{f}\"{} hier=\"-1\"/>", one.map(|k| format!(" item=\"{k}\"")).unwrap_or_default()));
        }
        t.push_str("</pageFields>");
    }
    if !p.values.is_empty() {
        t.push_str(&format!("<dataFields count=\"{}\">", p.values.len()));
        let mut used: HashSet<String> = HashSet::new();
        for v in &p.values {
            let mut name = p.value_name(wb, v);
            // Data field names must differ from each other and from the cache field names.
            while names.iter().any(|n| n.eq_ignore_ascii_case(&name)) || !used.insert(name.to_lowercase()) { name.push(' '); }
            let agg = if p.calc_index(v.field).is_some() { Agg::Sum } else { v.agg };
            t.push_str(&format!("<dataField name=\"{}\" fld=\"{}\" subtotal=\"{}\"", attr(&name), v.field, agg.code()));
            let show = match v.show { ShowAs::PctGrand => "percentOfTotal", ShowAs::PctRow => "percentOfRow", ShowAs::PctCol => "percentOfCol", ShowAs::Difference => "difference", ShowAs::RunTotal => "runTotal", _ => "" };
            if !show.is_empty() { t.push_str(&format!(" showDataAs=\"{show}\"")); }
            let base_item = match &v.base_item { BaseItem::Prev => PREV_ITEM as usize, BaseItem::Next => NEXT_ITEM as usize, BaseItem::Item(s) => items.get(&v.base_field).map_or(0, |it| it.pos(s)) };
            t.push_str(&format!(" baseField=\"{}\" baseItem=\"{}\"", if v.base_field < nf { v.base_field } else { 0 }, if v.show == ShowAs::Difference { base_item } else { 0 }));
            if let Some(id) = v.format().and_then(|fm| (1..50).find(|id| crate::numfmt::builtin(*id) == Some(fm.as_str()))) { t.push_str(&format!(" numFmtId=\"{id}\"")); }
            let x14 = match v.show { ShowAs::PctParentRow => "percentOfParentRow", ShowAs::RankAsc => "rankAscending", ShowAs::RankDesc => "rankDescending", _ => "" };
            if x14.is_empty() { t.push_str("/>"); } else { t.push_str(&format!("><extLst><ext uri=\"{EXT_X14_DATA_FIELD}\" xmlns:x14=\"{NS_X14}\"><x14:dataField pivotShowAs=\"{x14}\"/></ext></extLst></dataField>")); }
        }
        t.push_str("</dataFields>");
    }
    let st = &p.style;
    if !st.name.is_empty() { t.push_str(&format!("<pivotTableStyleInfo name=\"{}\" showRowHeaders=\"{}\" showColHeaders=\"{}\" showRowStripes=\"{}\" showColStripes=\"{}\" showLastColumn=\"1\"/>", attr(&st.name), st.row_headers as u8, st.col_headers as u8, st.banded_rows as u8, st.banded_cols as u8)); }
    // Timeline selections: the pivot's date filters (Excel applies them without a refresh).
    let tf = super::timeline_parts::pivot_filters(wb, &p.name);
    if !tf.is_empty() {
        t.push_str(&format!("<filters count=\"{}\">", tf.len()));
        for (k, (f, a, b)) in tf.iter().enumerate() { t.push_str(&super::timeline_parts::filter_xml(k + 1, *f, *a, *b, &super::timeline_parts::cache_base(&names[*f]))); }
        t.push_str("</filters>");
    }
    t.push_str("</pivotTableDefinition>");
    Some((cd, rc, t))
}

// ---- writing ---------------------------------------------------------------------------------------

/// The workbook-level state of a save in progress: the reference workbook part and rels being
/// rebuilt, new content types, and the part names taken.
pub(super) struct Ctx {
    pub rels: Vec<(String, String, String)>,
    pub overrides: Vec<(String, String)>,
    pub taken: HashSet<String>,
    /// Workbook pivot caches (cacheId, relationship id), carried ones first.
    pub caches: Vec<(u32, String)>,
    /// The pivot cache id of each written pivot, by (sheet, index).
    pub cache_ids: HashMap<(usize, usize), u32>,
    /// Slicer cache relationship ids for the workbook extensions (pivot, table).
    pub x14: Vec<String>,
    pub x15: Vec<String>,
    /// Timeline cache relationship ids for the workbook's x15 timelineCacheRefs.
    pub timelines: Vec<String>,
}

impl Ctx {
    pub fn free(&mut self, prefix: &str, ext: &str) -> String {
        let n = (1..).map(|i| format!("{prefix}{i}{ext}")).find(|n| !self.taken.contains(n)).unwrap();
        self.taken.insert(n.clone());
        n
    }
    /// A workbook relationship to the part `target` (an absolute part name).
    pub fn wb_rel(&mut self, kind: &str, target: &str) -> String {
        let id = (1..).map(|i| format!("rId{i}")).find(|i| !self.rels.iter().any(|r| &r.0 == i)).unwrap();
        self.rels.push((id.clone(), kind.into(), relative("xl/workbook.xml", target)));
        id
    }
}

fn short(kind: &str) -> &str { kind.rsplit('/').next().unwrap_or("") }

/// Whether the reference workbook part lists pivot or slicer caches whose parts are gone (so a
/// save must rebuild the lists even without pivots or objects).
pub fn stale(wb: &Workbook) -> bool {
    part_rels(wb, "xl/workbook.xml").iter().any(|(_, k, t)| matches!(short(k), "pivotCacheDefinition" | "slicerCache" | "timelineCache") && !wb.parts.contains_key(t))
}

/// Writes every pivot (and slicer) of the copy `w` as native parts; see the module comment.
pub fn store(w: &mut Workbook) {
    let mut wbx = parse_part(w, "xl/workbook.xml").unwrap_or_else(|| Element { name: "workbook".into(), attrs: vec![("xmlns".into(), NS.into()), ("xmlns:r".into(), NS_R.into())], children: vec![] });
    // Carried workbook relationships, minus pivot and slicer caches whose parts are gone.
    let mut rels: Vec<(String, String, String)> = vec![];
    if let Some(r) = parse_part(w, "xl/_rels/workbook.xml.rels") {
        for e in r.all("Relationship") {
            let (Some(id), Some(kind), Some(target)) = (e.attr("Id"), e.attr("Type"), e.attr("Target")) else { continue };
            if matches!(short(kind), "pivotCacheDefinition" | "slicerCache" | "timelineCache") && !w.parts.contains_key(&resolve("xl/workbook.xml", target)) { continue; }
            if e.attr("TargetMode") == Some("External") { rels.push((id.into(), kind.into(), target.into())); continue; }
            rels.push((id.into(), kind.into(), target.into()));
        }
    }
    let ids: HashSet<String> = rels.iter().map(|r| r.0.clone()).collect();
    let rid = |e: &Element| e.attrs.iter().find(|(k, _)| k.ends_with(":id") && !k.starts_with("xmlns")).map(|(_, v)| v.clone());
    let caches: Vec<(u32, String)> = wbx.child("pivotCaches").map(|pc| pc.all("pivotCache").filter_map(|c| Some((c.attr_u32("cacheId")?, rid(c)?))).filter(|(_, r)| ids.contains(r)).collect()).unwrap_or_default();
    let mut x14 = vec![];
    let mut x15 = vec![];
    let mut timelines = vec![];
    if let Some(el) = wbx.child("extLst") {
        for ext in el.all("ext") {
            if ext.attr("uri") == Some(super::timeline_parts::EXT_WB_TIMELINES) {
                for tc in ext.elements().flat_map(|l| l.all("timelineCacheRef")) { if let Some(r) = rid(tc).filter(|r| ids.contains(r)) { timelines.push(r); } }
                continue;
            }
            let list = match ext.attr("uri") { Some(EXT_WB_SLICERS) => &mut x14, Some(EXT_WB_TABLE_SLICERS) => &mut x15, _ => continue };
            for sc in ext.elements().flat_map(|l| l.all("slicerCache")) { if let Some(r) = rid(sc).filter(|r| ids.contains(r)) { list.push(r); } }
        }
    }
    let taken: HashSet<String> = w.parts.keys().cloned().collect();
    let mut ctx = Ctx { rels, overrides: vec![], taken, caches, cache_ids: HashMap::new(), x14, x15, timelines };
    // Pivots.
    let mut next_cache = ctx.caches.iter().map(|c| c.0).max().map_or(1, |m| m + 1);
    for si in 0..w.sheets.len() {
        for k in 0..w.sheets[si].pivots.len() {
            let p = w.sheets[si].pivots[k].clone();
            if !writable(w, &p) { continue; }
            let Some((cd, rc, t)) = pivot_xml(w, &p, next_cache) else { continue };
            let cpath = ctx.free("xl/pivotCache/pivotCacheDefinition", ".xml");
            let rpath = cpath.replace("pivotCacheDefinition", "pivotCacheRecords");
            ctx.taken.insert(rpath.clone());
            let tpath = ctx.free("xl/pivotTables/pivotTable", ".xml");
            w.parts.insert(cpath.clone(), cd.into_bytes());
            w.parts.insert(rels_name(&cpath), rels_xml(&[("rId1".into(), format!("{RT}pivotCacheRecords"), relative(&cpath, &rpath))]).into_bytes());
            w.parts.insert(rpath.clone(), rc.into_bytes());
            w.parts.insert(tpath.clone(), t.into_bytes());
            w.parts.insert(rels_name(&tpath), rels_xml(&[("rId1".into(), format!("{RT}pivotCacheDefinition"), relative(&tpath, &cpath))]).into_bytes());
            ctx.overrides.push((cpath.clone(), format!("{CT}pivotCacheDefinition+xml")));
            ctx.overrides.push((rpath, format!("{CT}pivotCacheRecords+xml")));
            ctx.overrides.push((tpath.clone(), format!("{CT}pivotTable+xml")));
            let id = ctx.wb_rel(&format!("{RT}pivotCacheDefinition"), &cpath);
            ctx.caches.push((next_cache, id));
            ctx.cache_ids.insert((si, k), next_cache);
            sheet_rel(w, si, &format!("{RT}pivotTable"), &tpath, "rIdPv");
            next_cache += 1;
        }
    }
    super::slicer_parts::store(w, &mut ctx);
    super::timeline_parts::store(w, &mut ctx);
    // The reference workbook part: pivotCaches and the slicer cache extensions rebuilt.
    wbx.children.retain(|c| !matches!(c, Node::Element(e) if matches!(e.local(), "pivotCaches" | "extLst")));
    let mut extra = String::new();
    if !ctx.caches.is_empty() {
        extra.push_str("<pivotCaches>");
        for (c, r) in &ctx.caches { extra.push_str(&format!("<pivotCache cacheId=\"{c}\" r:id=\"{r}\"/>")); }
        extra.push_str("</pivotCaches>");
    }
    if !ctx.x14.is_empty() || !ctx.x15.is_empty() || !ctx.timelines.is_empty() {
        extra.push_str(&format!("<extLst xmlns:r=\"{NS_R}\">"));
        if !ctx.x14.is_empty() {
            extra.push_str(&format!("<ext uri=\"{EXT_WB_SLICERS}\" xmlns:x14=\"{NS_X14}\"><x14:slicerCaches>"));
            for r in &ctx.x14 { extra.push_str(&format!("<x14:slicerCache r:id=\"{r}\"/>")); }
            extra.push_str("</x14:slicerCaches></ext>");
        }
        if !ctx.x15.is_empty() {
            extra.push_str(&format!("<ext uri=\"{EXT_WB_TABLE_SLICERS}\" xmlns:x15=\"{NS_X15}\"><x15:slicerCaches xmlns:x14=\"{NS_X14}\">"));
            for r in &ctx.x15 { extra.push_str(&format!("<x14:slicerCache r:id=\"{r}\"/>")); }
            extra.push_str("</x15:slicerCaches></ext>");
        }
        if !ctx.timelines.is_empty() {
            extra.push_str(&format!("<ext uri=\"{}\" xmlns:x15=\"{NS_X15}\"><x15:timelineCacheRefs>", super::timeline_parts::EXT_WB_TIMELINES));
            for r in &ctx.timelines { extra.push_str(&format!("<x15:timelineCacheRef r:id=\"{r}\"/>")); }
            extra.push_str("</x15:timelineCacheRefs></ext>");
        }
        extra.push_str("</extLst>");
    }
    if !extra.is_empty() {
        if let Ok(e) = Element::parse(&format!("<a xmlns=\"{NS}\" xmlns:r=\"{NS_R}\">{extra}</a>")) { wbx.children.extend(e.children); }
    }
    w.parts.insert("xl/workbook.xml".into(), wbx.to_xml().into_bytes());
    w.parts.insert("xl/_rels/workbook.xml.rels".into(), rels_xml(&ctx.rels).into_bytes());
    // Content types.
    let mut ct = parse_part(w, "[Content_Types].xml").unwrap_or_else(|| Element { name: "Types".into(), attrs: vec![("xmlns".into(), "http://schemas.openxmlformats.org/package/2006/content-types".into())], children: vec![] });
    for (p, t) in ctx.overrides {
        let pn = format!("/{p}");
        ct.children.retain(|n| !matches!(n, Node::Element(e) if e.local() == "Override" && e.attr("PartName") == Some(pn.as_str())));
        ct.children.push(Node::Element(Element { name: "Override".into(), attrs: vec![("PartName".into(), pn), ("ContentType".into(), t)], children: vec![] }));
    }
    w.parts.insert("[Content_Types].xml".into(), ct.to_xml().into_bytes());
}

// ---- reading ---------------------------------------------------------------------------------------

/// The item value a shared or group item element holds.
fn item_value(it: &Element) -> Option<Value> {
    Some(match it.local() {
        "s" => Value::str(it.attr("v").unwrap_or("")),
        "n" => Value::Num(it.attr_f64("v")?),
        "b" => Value::Bool(it.flag("v", false)),
        "m" => Value::Empty,
        "e" => Value::Err(crate::value::ErrKind::parse(it.attr("v")?)?),
        "d" => Value::str(it.attr("v").unwrap_or("")),
        _ => return None,
    })
}

/// A cache field's range grouping.
fn read_range(rp: &Element, d1904: bool) -> Option<Grouping> {
    let (auto_start, auto_end) = (rp.flag("autoStart", true), rp.flag("autoEnd", true));
    let by = rp.attr_f64("groupInterval").unwrap_or(1.0);
    match rp.attr("groupBy").unwrap_or("range") {
        "range" => Some(Grouping::Range { start: rp.attr_f64("startNum").unwrap_or(0.0), end: rp.attr_f64("endNum").unwrap_or(0.0), by, auto_start, auto_end }),
        g => {
            let level = DateLevel::parse(g)?;
            let start = rp.attr("startDate").and_then(|t| parse_iso(t, d1904)).unwrap_or(0.0);
            // The end date is the day after the last.
            let end = rp.attr("endDate").and_then(|t| parse_iso(t, d1904)).map(|e| if e.fract() == 0.0 { e - 1.0 } else { e }).unwrap_or(start);
            Some(Grouping::Date { level, start, end, by: if level == DateLevel::Days { by } else { 1.0 }, auto_start, auto_end })
        }
    }
}

/// A pivot table definition and its cache as a `Pivot`, when the model can represent it.
fn read_pivot(wb: &Workbook, table: &Element, cache: &Element) -> Option<Pivot> {
    let d1904 = wb.date1904;
    let src = cache.child("cacheSource")?;
    if src.attr("type").is_some_and(|t| t != "worksheet") { return None; }
    let ws = src.child("worksheetSource")?;
    if ws.attrs.iter().any(|(k, _)| k.ends_with(":id")) { return None; }
    let (sheet, range) = match (ws.attr("ref"), ws.attr("sheet"), ws.attr("name")) {
        (Some(r), Some(s), _) => (wb.sheets[wb.sheet_index(s)?].name.clone(), crate::addr::parse_range(r)?),
        (_, _, Some(n)) => { let (si, ti) = wb.find_table(n)?; (wb.sheets[si].name.clone(), wb.sheets[si].tables[ti].range) }
        _ => return None,
    };
    if table.child("calculatedItems").is_some() || table.flag("dataOnRows", false) { return None; }
    let fields: Vec<&Element> = cache.child("cacheFields")?.all("cacheField").collect();
    let n = (range.c1 - range.c0 + 1) as usize;
    // Cache fields: source (database) fields, grouping fields, calculated fields.
    let (mut db, mut grp, mut calc) = (vec![], vec![], vec![]);
    for (i, f) in fields.iter().enumerate() {
        if f.attr("formula").is_some() { calc.push(i); } else if !f.flag("databaseField", true) { if f.child("fieldGroup").is_none() { return None; } grp.push(i); } else { db.push(i); }
    }
    if db.len() != n { return None; }
    let mut map = vec![0usize; fields.len()];
    for (k, i) in db.iter().enumerate() { map[*i] = k; }
    for (j, i) in grp.iter().enumerate() { map[*i] = n + j; }
    for (c, i) in calc.iter().enumerate() { map[*i] = n + grp.len() + c; }
    let nf = fields.len();
    // Item texts: a grouped field's group items, else its shared items.
    let item_text = |f: usize, i: usize| -> Option<String> {
        let fe = fields.get(f)?;
        let list = fe.child("fieldGroup").and_then(|g| g.child("groupItems")).or_else(|| fe.child("sharedItems"))?;
        Some(key_text(&item_value(list.elements().nth(i)?)?))
    };
    let mut groups = BTreeMap::new();
    for (k, i) in db.iter().enumerate() {
        let Some(fg) = fields[*i].child("fieldGroup") else { continue };
        if fg.child("discretePr").is_some() { return None; }
        if let Some(rp) = fg.child("rangePr") { groups.insert(k, read_range(rp, d1904)?); }
    }
    let mut extra = vec![];
    for i in &grp {
        let fg = fields[*i].child("fieldGroup")?;
        let b = fg.attr_u32("base")? as usize;
        if !db.contains(&b) { return None; }
        let group = if let Some(rp) = fg.child("rangePr") {
            let g = read_range(rp, d1904)?;
            if !matches!(g, Grouping::Date { .. }) { return None; }
            g
        } else if let Some(dp) = fg.child("discretePr") {
            let gi: Vec<String> = fg.child("groupItems")?.elements().map(|e| item_value(e).map(|v| key_text(&v))).collect::<Option<_>>()?;
            let shared: Vec<String> = fields[b].child("sharedItems")?.elements().map(|e| item_value(e).map(|v| key_text(&v))).collect::<Option<_>>()?;
            let mut by: Vec<(String, Vec<String>)> = gi.iter().map(|g| (g.clone(), vec![])).collect();
            for (s, xe) in dp.all("x").enumerate() { let g = xe.attr_u32("v").unwrap_or(0) as usize; if let (Some(t), Some(slot)) = (shared.get(s), by.get_mut(g)) { slot.1.push(t.clone()); } }
            by.retain(|(name, items)| !items.is_empty() && !(items.len() == 1 && &items[0] == name));
            Grouping::Discrete(by)
        } else { return None };
        extra.push(ExtraField { name: fields[*i].attr("name").unwrap_or("Group").into(), base: map[b], group });
    }
    let calcs: Vec<CalcField> = calc.iter().map(|i| { let f = fields[*i].attr("formula").unwrap_or("").trim(); CalcField { name: fields[*i].attr("name").unwrap_or("Field").into(), formula: f.strip_prefix('=').unwrap_or(f).into() } }).collect();
    let loc = table.child("location").and_then(|l| l.attr("ref")).and_then(crate::addr::parse_range)?;
    let list = |name: &str, sub: &str, key: &str| -> Option<Vec<i64>> {
        Some(match table.child(name) { Some(e) => e.all(sub).map(|f| f.attr(key).and_then(|v| v.parse().ok())).collect::<Option<Vec<i64>>>()?, None => vec![] })
    };
    let rows = list("rowFields", "field", "x")?;
    if rows.contains(&-2) { return None; }
    let cols: Vec<i64> = list("colFields", "field", "x")?.into_iter().filter(|f| *f != -2).collect();
    let pages = list("pageFields", "pageField", "fld")?;
    let ok = |v: &[i64]| v.iter().all(|f| *f >= 0 && (*f as usize) < nf && !calc.contains(&(*f as usize)));
    if !ok(&rows) || !ok(&cols) || !ok(&pages) { return None; }
    let pf: Vec<&Element> = table.child("pivotFields").map(|e| e.all("pivotField").collect()).unwrap_or_default();
    if pf.len() != nf { return None; }
    // The item text at a position of a pivot field's items.
    let pf_item = |f: usize, k: usize| -> Option<String> {
        let it = pf[f].child("items")?.all("item").filter(|i| i.attr("t").is_none_or(|t| t == "data")).nth(k)?;
        item_text(f, it.attr_u32("x")? as usize)
    };
    let mut values = vec![];
    if let Some(df) = table.child("dataFields") {
        for d in df.all("dataField") {
            let f = d.attr_u32("fld")? as usize;
            if f >= nf { return None; }
            let x14 = d.child("extLst").into_iter().flat_map(|l| l.all("ext")).flat_map(|e| e.elements()).find(|e| e.local() == "dataField").and_then(|e| e.attr("pivotShowAs"));
            let show = match (d.attr("showDataAs").unwrap_or("normal"), x14) {
                (_, Some("percentOfParentRow")) => ShowAs::PctParentRow,
                (_, Some("rankAscending")) => ShowAs::RankAsc,
                (_, Some("rankDescending")) => ShowAs::RankDesc,
                (_, Some(_)) => return None,
                ("normal", None) => ShowAs::Normal,
                ("percentOfTotal", None) => ShowAs::PctGrand,
                ("percentOfRow", None) => ShowAs::PctRow,
                ("percentOfCol", None) => ShowAs::PctCol,
                ("difference", None) => ShowAs::Difference,
                ("runTotal", None) => ShowAs::RunTotal,
                _ => return None,
            };
            let bf = d.attr_u32("baseField").map(|b| b as usize).filter(|b| *b < nf).unwrap_or(0);
            let base_item = match d.attr_u32("baseItem").unwrap_or(0) {
                _ if show != ShowAs::Difference => BaseItem::Prev,
                PREV_ITEM => BaseItem::Prev, NEXT_ITEM => BaseItem::Next,
                k => pf_item(bf, k as usize).map(BaseItem::Item).unwrap_or_default(),
            };
            let mut num_fmt = d.attr_u32("numFmtId").filter(|id| *id != 0).and_then(crate::numfmt::builtin).unwrap_or("").to_string();
            if show.percent() && num_fmt == "0.00%" { num_fmt.clear(); }
            let agg = if calc.contains(&f) { Agg::Sum } else { Agg::parse(d.attr("subtotal").unwrap_or("sum"))? };
            values.push((ValueField { field: map[f], agg, show, base_field: map[bf], base_item, name: String::new(), num_fmt }, d.attr("name").unwrap_or("").trim_end().to_string()));
        }
    }
    let mut hidden: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for (f, e) in pf.iter().enumerate() {
        let data: Vec<&Element> = e.child("items").map(|it| it.all("item").filter(|i| i.attr("t").is_none_or(|t| t == "data")).collect()).unwrap_or_default();
        let single = table.child("pageFields").and_then(|pfs| pfs.all("pageField").find(|x| x.attr_u32("fld") == Some(f as u32))).and_then(|x| x.attr_u32("item"));
        let mut h = BTreeSet::new();
        for (k, item) in data.iter().enumerate() {
            let off = match single { Some(s) => k as u32 != s, None => item.flag("h", false) };
            if !off { continue; }
            h.insert(item_text(f, item.attr_u32("x")? as usize)?);
        }
        if !h.is_empty() { hidden.insert(map[f], h); }
    }
    let page_rows = if pages.is_empty() { 0 } else { table.child("location").and_then(|l| l.attr_u32("rowPageCount")).unwrap_or(pages.len() as u32) + 1 };
    let anchor = Pos::new(loc.r0.saturating_sub(page_rows), loc.c0);
    let c1 = if pages.is_empty() { loc.c1 } else { loc.c1.max(loc.c0 + 1) };
    let u = |v: Vec<i64>| v.into_iter().map(|f| map[f as usize]).collect::<Vec<_>>();
    // Layout: from the outer row field (or the table's defaults).
    let outer = rows.first().map(|f| pf[*f as usize]);
    let (compact, outline) = match outer { Some(e) => (e.flag("compact", true) && table.flag("compact", true), e.flag("outline", true) && table.flag("outline", false)), None => (table.flag("compact", true), table.flag("outline", false)) };
    let layout = if table.flag("gridDropZones", false) { Layout::Tabular } else if compact && outline { Layout::Compact } else if outline { Layout::Outline } else { Layout::Tabular };
    let subtotals = match outer { Some(e) if !e.flag("defaultSubtotal", true) => Subtotals::Off, Some(e) if !e.flag("subtotalTop", true) || layout == Layout::Tabular => Subtotals::Bottom, _ => if layout == Layout::Tabular { Subtotals::Bottom } else { Subtotals::Top } };
    let row_pf: Vec<&Element> = rows.iter().map(|f| pf[*f as usize]).collect();
    let fill_down = |e: &Element| e.child("extLst").into_iter().flat_map(|l| l.all("ext")).flat_map(|x| x.elements()).any(|x| x.flag("fillDownLabels", false) || x.flag("fillDownLabelsDefault", false));
    let style = match table.child("pivotTableStyleInfo") {
        Some(s) => PivotStyle { name: s.attr("name").unwrap_or("").into(), row_headers: s.flag("showRowHeaders", false), col_headers: s.flag("showColHeaders", false), banded_rows: s.flag("showRowStripes", false), banded_cols: s.flag("showColStripes", false) },
        None => PivotStyle { name: String::new(), ..Default::default() },
    };
    let mut p = Pivot {
        name: table.attr("name").unwrap_or("PivotTable1").into(), source_sheet: sheet, source: range,
        rows: u(rows), cols: u(cols), filters: u(pages), values: vec![], hidden, anchor,
        output: Some(Range { r0: anchor.row, c0: loc.c0, r1: loc.r1, c1 }),
        groups, extra, calcs, layout, subtotals,
        row_grand: table.flag("rowGrandTotals", true), col_grand: table.flag("colGrandTotals", true),
        repeat_labels: row_pf.iter().any(|e| fill_down(e)) || fill_down(table),
        blank_rows: row_pf.iter().any(|e| e.flag("insertBlankRow", false)),
        style, date_filter: None,
    };
    for (mut v, name) in values {
        if !name.is_empty() && name != p.value_name(wb, &v) { v.name = name; }
        p.values.push(v);
    }
    Some(p)
}

/// Turns the native pivot tables the model can represent into `Sheet.pivots` and drops their
/// parts (the cache too once no carried pivot table uses it).
pub fn load(wb: &mut Workbook) {
    let blocked = super::slicer_parts::blocked_pivots(wb);
    let mut users: HashMap<String, usize> = HashMap::new();
    let mut found: Vec<(usize, String, String, String)> = vec![];
    for (si, s) in wb.sheets.iter().enumerate() {
        for (k, v) in &s.extra {
            if k != "rel" { continue; }
            let f: Vec<&str> = v.split('\t').collect();
            if f.len() < 4 || f[3] == "1" || short(f[1]) != "pivotTable" { continue; }
            let Some(cache) = part_rels(wb, f[2]).into_iter().find(|r| short(&r.1) == "pivotCacheDefinition").map(|r| r.2) else { continue };
            *users.entry(cache.clone()).or_default() += 1;
            found.push((si, f[0].to_string(), f[2].to_string(), cache));
        }
    }
    for (si, rid, tpath, cpath) in found {
        let (Some(t), Some(c)) = (parse_part(wb, &tpath), parse_part(wb, &cpath)) else { continue };
        if t.attr("name").is_some_and(|n| blocked.contains(n)) { continue; }
        let Some(p) = read_pivot(wb, &t, &c) else { continue };
        wb.sheets[si].pivots.push(p);
        wb.sheets[si].extra.retain(|(k, v)| !(k == "rel" && v.split('\t').next() == Some(rid.as_str())));
        wb.parts.remove(&tpath);
        wb.parts.remove(&rels_name(&tpath));
        let left = users.get_mut(&cpath).unwrap();
        *left -= 1;
        if *left == 0 {
            for (_, _, r) in part_rels(wb, &cpath) { wb.parts.remove(&r); wb.parts.remove(&rels_name(&r)); }
            wb.parts.remove(&rels_name(&cpath));
            wb.parts.remove(&cpath);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::addr::{parse_range, Pos};
    use crate::book::Workbook;
    use crate::drawing::{add_object, Anchor, ObjKind, Object, Slicer};
    use crate::pivot::{Agg, Pivot, ValueField};
    use crate::xlsx::xml::Element;
    use std::collections::{BTreeMap, HashSet};
    use std::io::{Read, Write};

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        let rows = [["Region", "Product", "Sales", "Qty"], ["East", "A", "10", "1"], ["West", "A", "20", "2"], ["East", "B", "5", "3"], ["East", "A", "7", "4"], ["West", "B", "", "5"]];
        for (i, row) in rows.iter().enumerate() { for (j, t) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, j as u32), t); } }
        wb.add_sheet(1, Some("Report".into()));
        wb.recalc();
        wb
    }

    fn unzip(bytes: &[u8]) -> BTreeMap<String, String> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..z.len()).map(|i| { let mut f = z.by_index(i).unwrap(); let mut s = String::new(); f.read_to_string(&mut s).unwrap_or_default(); (f.name().to_string(), s) }).collect()
    }

    fn rezip(files: &BTreeMap<String, String>) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut buf);
        for (k, v) in files { z.start_file(k.as_str(), zip::write::SimpleFileOptions::default()).unwrap(); z.write_all(v.as_bytes()).unwrap(); }
        z.finish().unwrap();
        buf.into_inner()
    }

    /// Every relationship target exists, every part has a content type, and every override names a part.
    fn check_package(files: &BTreeMap<String, String>) {
        let ct = Element::parse(&files["[Content_Types].xml"]).unwrap();
        let defaults: HashSet<String> = ct.all("Default").filter_map(|e| e.attr("Extension").map(|x| x.to_ascii_lowercase())).collect();
        let overrides: HashSet<String> = ct.all("Override").filter_map(|e| e.attr("PartName").map(|p| p.trim_start_matches('/').to_string())).collect();
        for o in &overrides { assert!(files.contains_key(o), "override for missing part {o}"); }
        for k in files.keys() {
            if k == "[Content_Types].xml" { continue; }
            let ext = k.rsplit('.').next().unwrap().to_ascii_lowercase();
            assert!(overrides.contains(k) || (defaults.contains(&ext) && ext != "xml") || k.ends_with(".rels"), "no content type for {k}");
            if (k.starts_with("xl/pivot") || k.starts_with("xl/slicer")) && !k.ends_with(".rels") { assert!(overrides.contains(k), "no override for {k}"); }
            if let Some(i) = k.rfind("_rels/") {
                let owner = format!("{}{}", &k[..i], k[i + 6..].trim_end_matches(".rels"));
                let rels = Element::parse(&files[k]).unwrap();
                for r in rels.all("Relationship") {
                    if r.attr("TargetMode") == Some("External") { continue; }
                    let t = super::resolve(&owner, r.attr("Target").unwrap());
                    assert!(files.contains_key(&t), "{k}: missing target {t}");
                }
            }
        }
        // r:ids used by the workbook part exist in its rels.
        let rels = Element::parse(&files["xl/_rels/workbook.xml.rels"]).unwrap();
        let ids: HashSet<&str> = rels.all("Relationship").filter_map(|r| r.attr("Id")).collect();
        let w = &files["xl/workbook.xml"];
        for part in w.split("r:id=\"").skip(1) { let id = &part[..part.find('"').unwrap()]; assert!(ids.contains(id), "workbook r:id {id} missing"); }
    }

    fn count(files: &BTreeMap<String, String>, prefix: &str) -> usize { files.keys().filter(|k| k.starts_with(prefix) && !k.contains("_rels")).count() }

    fn pivot() -> Pivot {
        let mut p = Pivot::new("PivotTable1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(0, 0));
        p.rows = vec![0, 1];
        p.cols = vec![];
        p.filters = vec![3];
        p.values = vec![ValueField::new(2, Agg::Sum), ValueField::new(3, Agg::Average)];
        p.hidden.insert(1, ["B".to_string()].into());
        p
    }

    #[test]
    fn pivot_writes_native_parts_and_reads_back() {
        let mut wb = book();
        let mut p = pivot();
        p.apply(&mut wb, 1);
        wb.sheets[1].pivots.push(p.clone());
        let mut q = Pivot::new("PivotTable2", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(20, 0));
        q.rows = vec![0];
        q.cols = vec![1];
        q.values = vec![ValueField::new(2, Agg::Max)];
        q.apply(&mut wb, 1);
        wb.sheets[1].pivots.push(q.clone());
        let bytes = crate::xlsx::write(&wb).unwrap();
        let files = unzip(&bytes);
        check_package(&files);
        assert_eq!(count(&files, "xl/pivotTables/pivotTable"), 2);
        assert_eq!(count(&files, "xl/pivotCache/pivotCacheDefinition"), 2);
        assert_eq!(count(&files, "xl/pivotCache/pivotCacheRecords"), 2);
        let w = &files["xl/workbook.xml"];
        assert!(w.contains("<pivotCaches><pivotCache cacheId=\"1\"") && !w.contains("_JadwalPivot"));
        let t = Element::parse(&files["xl/pivotTables/pivotTable1.xml"]).unwrap();
        assert_eq!(t.child("location").unwrap().attr("ref"), Some("A3:C8"));
        assert_eq!(t.path("pageFields/pageField").unwrap().attr("fld"), Some("3"));
        assert_eq!(t.path("dataFields").unwrap().all("dataField").map(|d| d.attr("subtotal").unwrap().to_string()).collect::<Vec<_>>(), ["sum", "average"]);
        assert_eq!(t.path("colFields/field").unwrap().attr("x"), Some("-2"));
        let pf: Vec<&Element> = t.path("pivotFields").unwrap().all("pivotField").collect();
        assert_eq!(pf[0].attr("axis"), Some("axisRow"));
        assert!(pf[1].path("items").unwrap().all("item").any(|i| i.attr("h") == Some("1")));
        let c = Element::parse(&files["xl/pivotCache/pivotCacheDefinition1.xml"]).unwrap();
        assert_eq!(c.attr("refreshOnLoad"), Some("1"));
        assert_eq!(c.path("cacheSource/worksheetSource").unwrap().attr("ref"), Some("A1:D6"));
        assert!(files["xl/worksheets/_rels/sheet2.xml.rels"].contains("pivotTable"));
        // The computed cells are kept.
        assert!(files["xl/worksheets/sheet2.xml"].contains("<c r=\"A3\""));
        let back = crate::xlsx::read(&bytes).unwrap();
        assert_eq!(back.sheets[1].pivots, vec![p, q]);
        assert!(back.names.is_empty());
        assert!(!back.parts.keys().any(|k| k.starts_with("xl/pivot")));
        // Saving again regenerates the parts instead of adding to them.
        let files2 = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&files2);
        assert_eq!(count(&files2, "xl/pivotTables/"), 2);
        assert_eq!(count(&files2, "xl/pivotCache/"), 4);
        assert_eq!(files2["xl/workbook.xml"].matches("<pivotCache ").count(), 2);
    }

    #[test]
    fn unrepresentable_pivot_is_carried() {
        let mut wb = book();
        let mut p = pivot();
        p.apply(&mut wb, 1);
        wb.sheets[1].pivots.push(p);
        let mut files = unzip(&crate::xlsx::write(&wb).unwrap());
        let c = files["xl/pivotCache/pivotCacheDefinition1.xml"].replace("<cacheSource type=\"worksheet\">", "<cacheSource type=\"external\" connectionId=\"1\">");
        files.insert("xl/pivotCache/pivotCacheDefinition1.xml".into(), c);
        let back = crate::xlsx::read(&rezip(&files)).unwrap();
        assert!(back.sheets[1].pivots.is_empty());
        let again = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&again);
        assert_eq!(count(&again, "xl/pivotTables/pivotTable"), 1);
        assert!(again["xl/workbook.xml"].contains("<pivotCache "));
        assert!(again["xl/pivotCache/pivotCacheDefinition1.xml"].contains("connectionId="));
    }

    #[test]
    fn legacy_pivot_names_still_load() {
        let mut wb = book();
        let mut p = pivot();
        p.layout = crate::pivot::Layout::Tabular;
        p.subtotals = crate::pivot::Subtotals::Bottom;
        p.apply(&mut wb, 1);
        wb.names.push(crate::book::DefinedName { name: format!("{}1", crate::pivot::NAME_PREFIX), sheet: Some(1), formula: crate::pivot::spec_formula(&p.to_spec()), hidden: true, comment: String::new() });
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        assert_eq!(back.sheets[1].pivots, vec![p]);
    }

    #[test]
    fn slicers_write_native_parts_and_read_back() {
        let mut wb = book();
        let mut p = pivot();
        p.apply(&mut wb, 1);
        wb.sheets[1].pivots.push(p);
        for (i, row) in [["Name", "N"], ["a", "1"], ["b", "2"]].iter().enumerate() { for (j, v) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, 5 + j as u32), v); } }
        wb.recalc();
        let t = crate::tables::Table { name: "Table1".into(), display_name: "Table1".into(), range: parse_range("F1:G3").unwrap(), header_row: true, totals_row: false,
            columns: vec![crate::tables::TableColumn { name: "Name".into(), ..Default::default() }, crate::tables::TableColumn { name: "N".into(), ..Default::default() }],
            style: "TableStyleMedium2".into(), banded_rows: true, banded_cols: false, first_col: false, last_col: false, filter: Some(Default::default()) };
        wb.sheets[0].tables.push(t);
        add_object(&mut wb.sheets[1].objects, Object::new(ObjKind::Slicer(Slicer { pivot: "PivotTable1".into(), table: String::new(), field: 1, caption: "Product".into() }), Anchor::default()), "Slicer");
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Slicer(Slicer { pivot: String::new(), table: "Table1".into(), field: 0, caption: "Name".into() }), Anchor::default()), "Slicer");
        let bytes = crate::xlsx::write(&wb).unwrap();
        // For external validation (openpyxl, LibreOffice): JADWAL_PIVOT_OUT=path.xlsx.
        if let Ok(out) = std::env::var("JADWAL_PIVOT_OUT") { std::fs::write(out, &bytes).unwrap(); }
        let files = unzip(&bytes);
        check_package(&files);
        assert_eq!(count(&files, "xl/slicerCaches/slicerCache"), 2);
        assert_eq!(count(&files, "xl/slicers/slicer"), 2);
        let w = &files["xl/workbook.xml"];
        assert!(w.contains("x14:slicerCaches") && w.contains("x15:slicerCaches") && w.contains("Slicer_Product") && w.contains("Slicer_Name"));
        let sc = files.iter().find(|(k, v)| k.starts_with("xl/slicerCaches/") && v.contains("tabular")).unwrap().1;
        assert!(sc.contains("<i x=\"1\" s=\"0\"/>") && sc.contains("pivotCacheId=\"1\""));
        assert!(files.values().any(|v| v.contains("tableSlicerCache tableId=\"1\" column=\"1\"")));
        assert!(files["xl/worksheets/sheet2.xml"].contains("x14:slicerList") && files["xl/worksheets/sheet1.xml"].contains("x14:slicerList"));
        let drawing = files.iter().filter(|(k, _)| k.starts_with("xl/drawings/drawing")).map(|(_, v)| v.as_str()).collect::<String>();
        assert_eq!(drawing.matches("<sle:slicer ").count(), 2);
        let back = crate::xlsx::read(&bytes).unwrap();
        assert!(matches!(&back.sheets[1].objects[0].kind, ObjKind::Slicer(s) if s.pivot == "PivotTable1" && s.field == 1 && s.caption == "Product"));
        assert!(matches!(&back.sheets[0].objects[0].kind, ObjKind::Slicer(s) if s.table == "Table1" && s.field == 0 && s.caption == "Name"));
        assert!(back.names.is_empty());
        assert!(!back.parts.keys().any(|k| k.starts_with("xl/slicer")));
        let again = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&again);
        assert_eq!(count(&again, "xl/slicerCaches/"), 2);
        assert_eq!(again["xl/workbook.xml"].matches("<x14:slicerCache ").count(), 2);
        assert_eq!(again["xl/workbook.xml"].matches("<definedName ").count(), 2);
    }

    #[test]
    fn grouping_calcs_and_layout_round_trip() {
        use crate::pivot::{BaseItem, DateLevel, Layout, ShowAs, Subtotals};
        let mut wb = Workbook::new();
        let rows = [["Date", "Region", "Sales", "Qty"], ["2024-01-15", "East", "10", "1"], ["2024-02-03", "West", "20", "2"], ["2024-05-20", "East", "5", "3"], ["2025-01-02", "West", "7", "4"], ["2025-11-30", "North", "1", "5"]];
        for (i, row) in rows.iter().enumerate() { for (j, t) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, j as u32), t); } }
        wb.add_sheet(1, Some("Report".into()));
        wb.recalc();
        let mut p = Pivot::new("PivotTable1", "Sheet1", parse_range("A1:D6").unwrap(), Pos::new(2, 0));
        p.rows = vec![0];
        p.cols = vec![1];
        let (lo, hi) = p.field_bounds(&wb, 0).unwrap();
        p.group_dates(&wb, 0, &[DateLevel::Years, DateLevel::Months], lo, hi, 1.0, false, false);
        p.group_items(&wb, 1, "Coasts", vec!["East".into(), "West".into()]);
        assert_eq!((p.rows.clone(), p.cols.clone()), (vec![4, 0], vec![5, 1]));
        let c = p.add_calc("Bonus", "=Sales*0.1");
        let mut diff = ValueField::new(2, Agg::Sum);
        diff.show = ShowAs::Difference; diff.base_field = 1; diff.base_item = BaseItem::Item("East".into()); diff.name = "Diff".into(); diff.num_fmt = "#,##0.00".into();
        let mut rank = ValueField::new(2, Agg::Sum); rank.show = ShowAs::RankDesc; rank.base_field = 0;
        let mut share = ValueField::new(3, Agg::Average); share.show = ShowAs::PctParentRow;
        let mut run = ValueField::new(2, Agg::StdDev); run.show = ShowAs::RunTotal; run.base_field = 2; run.base_item = BaseItem::Prev;
        p.values = vec![ValueField::new(2, Agg::Sum), ValueField::new(c, Agg::Sum), diff, rank, share, run];
        p.layout = Layout::Outline;
        p.subtotals = Subtotals::Bottom;
        p.row_grand = false;
        p.repeat_labels = true;
        p.blank_rows = true;
        p.style.name = "PivotStyleMedium9".into();
        p.style.banded_rows = true;
        p.hidden.insert(0, ["Feb".to_string()].into());
        p.apply(&mut wb, 1);
        wb.sheets[1].pivots.push(p.clone());
        let bytes = crate::xlsx::write(&wb).unwrap();
        if let Ok(out) = std::env::var("JADWAL_PIVOT_GROUP_OUT") { std::fs::write(out, &bytes).unwrap(); }
        let files = unzip(&bytes);
        check_package(&files);
        let cd = &files["xl/pivotCache/pivotCacheDefinition1.xml"];
        assert!(Element::parse(cd).is_ok());
        assert!(cd.contains("groupBy=\"months\"") && cd.contains("groupBy=\"years\"") && cd.contains("databaseField=\"0\"") && cd.contains("<discretePr") && cd.contains("formula=\"Sales*0.1\""));
        assert!(cd.contains("<fieldGroup par=\"4\" base=\"0\">") && cd.contains("<fieldGroup par=\"5\"/>"));
        assert!(files["xl/pivotCache/pivotCacheRecords1.xml"].contains("<d v=\"2024-01-15T00:00:00\"/>"));
        let t = &files["xl/pivotTables/pivotTable1.xml"];
        assert!(Element::parse(t).is_ok());
        assert!(t.contains("showDataAs=\"difference\"") && t.contains("pivotShowAs=\"rankDescending\"") && t.contains("pivotShowAs=\"percentOfParentRow\"") && t.contains("showDataAs=\"runTotal\""));
        assert!(t.contains("rowGrandTotals=\"0\"") && t.contains("PivotStyleMedium9") && t.contains("showRowStripes=\"1\"") && t.contains("fillDownLabels=\"1\"") && t.contains("insertBlankRow=\"1\"") && t.contains("subtotalTop=\"0\""));
        assert!(t.contains("numFmtId=\"4\"") && t.contains("subtotal=\"stdDev\""));
        let back = crate::xlsx::read(&bytes).unwrap();
        assert_eq!(back.sheets[1].pivots, vec![p.clone()]);
        assert_eq!(back.sheets[1].pivots[0].compute(&back), p.compute(&wb));
        // Each layout survives a save.
        for (layout, sub) in [(Layout::Compact, Subtotals::Top), (Layout::Tabular, Subtotals::Bottom), (Layout::Outline, Subtotals::Off)] {
            let mut q = p.clone();
            q.layout = layout; q.subtotals = sub; q.repeat_labels = false; q.blank_rows = false; q.row_grand = true; q.col_grand = false;
            let mut w2 = wb.clone();
            w2.sheets[1].pivots = vec![q.clone()];
            let back = crate::xlsx::read(&crate::xlsx::write(&w2).unwrap()).unwrap();
            let mut b = back.sheets[1].pivots[0].clone();
            b.output = q.output;
            assert_eq!(b, q, "{layout:?}");
        }
        let again = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&again);
    }
}
