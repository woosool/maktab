//! Reading a package (io.js's import side): workbook, styles, theme, shared strings, then each
//! worksheet streamed with quick-xml; unmodelled parts and worksheet elements are carried.

use super::styles::{self, Palette};
use super::xml::{decode_x, part_text, rich_text, Element};
use super::{base_width_px, from_file_formula, width_to_px};
use crate::addr::{parse_range, Pos, Range, MAX_COLS};
use crate::book::{Cell, DefinedName, Formula, Hyperlink, Input, Sheet, Workbook};
use crate::value::{ErrKind, Value};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;

use std::sync::Arc;

/// A relationship, its type shortened to the last path segment ("worksheet", "hyperlink") so
/// transitional and strict files match alike.
#[derive(Clone, Debug)]
pub struct Rel { pub id: String, pub kind: String, pub full_kind: String, pub target: String, pub external: bool }

pub fn rels_name(part: &str) -> String {
    match part.rfind('/') { Some(i) => format!("{}/_rels/{}.rels", &part[..i], &part[i + 1..]), None => format!("_rels/{part}.rels") }
}

/// A relationship target resolved against the part holding it.
pub fn resolve(source: &str, target: &str) -> String {
    let target = target.replace('\\', "/");
    if let Some(abs) = target.strip_prefix('/') { return abs.to_string(); }
    let mut segs: Vec<&str> = source.split('/').collect();
    segs.pop();
    for s in target.split('/') { match s { ".." => { segs.pop(); } "." | "" => {} s => segs.push(s) } }
    segs.join("/")
}

struct Pkg { files: BTreeMap<String, Vec<u8>>, used: HashSet<String> }

impl Pkg {
    /// A part by name (exact, then case-insensitively, as some writers vary case).
    fn key(&self, name: &str) -> Option<String> {
        if self.files.contains_key(name) { return Some(name.to_string()); }
        self.files.keys().find(|k| k.eq_ignore_ascii_case(name)).cloned()
    }
    fn take(&mut self, name: &str) -> Option<String> { let k = self.key(name)?; self.used.insert(k.clone()); Some(part_text(&self.files[&k])) }
    fn peek(&self, name: &str) -> Option<String> { self.key(name).map(|k| part_text(&self.files[&k])) }
    fn rels(&mut self, part: &str) -> Vec<Rel> {
        let Some(t) = self.take(&rels_name(part)) else { return vec![] };
        let Ok(root) = Element::parse(&t) else { return vec![] };
        root.all("Relationship").map(|e| {
            let external = e.attr("TargetMode").is_some_and(|m| m.eq_ignore_ascii_case("External"));
            let raw = e.attr("Target").unwrap_or("");
            let full = e.attr("Type").unwrap_or("").to_string();
            Rel { id: e.attr("Id").unwrap_or("").into(), kind: full.rsplit('/').next().unwrap_or("").into(), full_kind: full, target: if external { raw.into() } else { resolve(part, raw) }, external }
        }).collect()
    }
}

/// The theme's twelve colors, dk1 first.
fn theme_colors(src: &str) -> Option<Vec<[u8; 3]>> {
    let root = Element::parse(src).ok()?;
    fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }
    let scheme = find(&root, "clrScheme")?;
    let names = ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];
    let mut out = crate::numfmt::OFFICE_THEME.to_vec();
    for (i, n) in names.iter().enumerate() {
        let Some(c) = scheme.child(n).and_then(|e| e.first_el()) else { continue };
        if let Some(rgb) = c.attr("val").filter(|_| c.local() == "srgbClr").or(c.attr("lastClr")).and_then(styles::hex) { out[i] = rgb; }
    }
    Some(out)
}

impl Element { fn first_el(&self) -> Option<&Element> { self.elements().next() } }

/// The shared string table, streamed.
fn shared_strings(src: &str) -> Vec<Arc<str>> {
    let mut out = vec![];
    let mut r = Reader::from_str(src);
    let (mut in_si, mut in_t, mut skip) = (false, false, 0);
    let mut cur = String::new();
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"si" => { in_si = true; cur.clear(); }
                b"t" if in_si && skip == 0 => in_t = true,
                b"rPh" => skip += 1,
                _ => {}
            },
            Ok(Event::Empty(e)) if e.local_name().as_ref() == b"si" => out.push(Arc::from("")),
            Ok(Event::Text(t)) if in_t => cur.push_str(&t.unescape().map(|c| c.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned())),
            Ok(Event::CData(t)) if in_t => cur.push_str(&String::from_utf8_lossy(&t)),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"si" => { in_si = false; out.push(Arc::from(decode_x(&cur).as_str())); }
                b"t" => in_t = false,
                b"rPh" => skip -= 1,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// "2024-01-15T10:30:00Z" as a serial.
fn iso_serial(s: &str, date1904: bool) -> Option<f64> {
    let (d, t) = s.split_once('T').unwrap_or((s, ""));
    let mut p = d.split('-').map(|x| x.parse::<i64>());
    let (y, m, dd) = (p.next()?.ok()?, p.next().unwrap_or(Ok(1)).ok()?, p.next().unwrap_or(Ok(1)).ok()?);
    let day = if y < 1900 && !date1904 { 0.0 } else { crate::date::serial(y, m, dd, date1904) };
    let t = t.trim_end_matches('Z');
    let t = t.split(['+']).next().unwrap_or("");
    let mut secs = 0.0;
    for (i, part) in t.split(':').enumerate() { if let Ok(v) = part.parse::<f64>() { secs += v * [3600.0, 60.0, 1.0].get(i).copied().unwrap_or(0.0); } }
    Some(day + secs / 86400.0)
}

fn attrs(e: &BytesStart) -> Vec<(Vec<u8>, String)> {
    e.attributes().with_checks(false).flatten().map(|a| (a.key.local_name().as_ref().to_vec(), a.unescape_value().map(|v| v.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned()))).collect()
}
fn get<'a>(a: &'a [(Vec<u8>, String)], k: &str) -> Option<&'a str> { a.iter().find(|(n, _)| n == k.as_bytes()).map(|(_, v)| v.as_str()) }

/// Shared context for a sheet's cells.
struct Ctx<'a> { sst: &'a [Arc<str>], xf: &'a [u32], date1904: bool, pal: &'a Palette }

/// A cached value written in the file, with the cell holding it, for formulas the engine can't
/// evaluate (and manual-calculation files).
type Cached = Vec<(usize, Pos, Value)>;

/// One cell being read.
#[derive(Default)]
struct RawCell { pos: Pos, s: u32, t: String, cm: bool, v: String, f: Option<String>, ft: String, fsi: Option<String>, fref: Option<String>, inline: String, vm: Option<u32> }

fn finish(c: RawCell, sheet: &mut Sheet, ctx: &Ctx, shared: &mut HashMap<String, (Pos, String)>, arrays: &mut Vec<(Pos, Range, bool)>) -> bool {
    let style = ctx.xf.get(c.s as usize).copied().unwrap_or(0);
    let value = match c.t.as_str() {
        "s" => c.v.trim().parse::<usize>().ok().and_then(|i| ctx.sst.get(i)).map(|s| Value::Str(s.clone())).unwrap_or(Value::Empty),
        "str" => if c.v.is_empty() && c.f.is_none() { Value::Empty } else { Value::str(decode_x(&c.v)) },
        "inlineStr" => Value::str(decode_x(if c.inline.is_empty() { &c.v } else { &c.inline })),
        "b" => Value::Bool(matches!(c.v.trim(), "1" | "true" | "TRUE")),
        "e" => ErrKind::parse(c.v.trim()).map(Value::Err).unwrap_or_else(|| if c.v.trim() == "#GETTING_DATA" { Value::Err(ErrKind::NA) } else { Value::str(&c.v) }),
        "d" => iso_serial(c.v.trim(), ctx.date1904).map(Value::Num).unwrap_or(Value::Empty),
        _ => c.v.trim().parse::<f64>().map(Value::Num).unwrap_or(Value::Empty),
    };
    // A rich value (picture in cell): a marker resolved after reading (richdata::load).
    let value = match c.vm { Some(vm) if c.f.is_none() => super::richdata::marker(vm), _ => value };
    // The formula: its own text, or a shared formula's master text moved here.
    let mut text = c.f.clone().filter(|t| !t.trim().is_empty());
    if c.ft == "shared" {
        if let Some(si) = &c.fsi {
            match &text {
                Some(t) => { shared.insert(si.clone(), (c.pos, t.clone())); }
                None => if let Some((at, t)) = shared.get(si) { text = Some(crate::formula::adjust::shift(t, c.pos.row as i64 - at.row as i64, c.pos.col as i64 - at.col as i64)); },
            }
        }
    }
    if c.ft == "dataTable" { text = None; }
    let mut cell = Cell { style, value: value.clone(), ..Default::default() };
    let formula = text.is_some();
    match text {
        Some(t) => {
            let t = from_file_formula(&decode_x(&t));
            cell.input = Input::Formula(Arc::new(Formula::new(&t)));
            if c.ft == "array" {
                if let Some(r) = c.fref.as_deref().and_then(parse_range) {
                    if c.cm { arrays.push((c.pos, r, true)); } else { cell.array = Some(r); arrays.push((c.pos, r, false)); }
                }
            }
        }
        None => {
            if value.is_empty() && style == 0 { return false; }
            cell.input = if value.is_empty() { Input::Empty } else { Input::Value(value) };
        }
    }
    sheet.cells.insert(c.pos, cell);
    formula
}

/// The <sheetData> stream, up to its end tag.
fn sheet_data(r: &mut Reader<&[u8]>, sheet: &mut Sheet, ctx: &Ctx) -> Result<Vec<(Pos, Range, bool)>, String> {
    let mut shared: HashMap<String, (Pos, String)> = HashMap::new();
    let mut arrays = vec![];
    let (mut row, mut col): (i64, i64) = (-1, -1);
    let mut cell: Option<RawCell> = None;
    #[derive(PartialEq)] enum In { None, V, F, T }
    let mut inn = In::None;
    let mut in_is = false;
    let mut rph = 0;
    let row_start = |a: &[(Vec<u8>, String)], row: &mut i64, col: &mut i64, sheet: &mut Sheet| {
        *row = get(a, "r").and_then(|v| v.trim().parse::<i64>().ok()).map(|v| v - 1).unwrap_or(*row + 1);
        *col = -1;
        let rr = (*row).max(0) as u32;
        if let Some(ht) = get(a, "ht").and_then(|v| v.parse::<f64>().ok()) {
            let px = (ht * 4.0 / 3.0) as f32;
            let px = (px * 100.0).round() / 100.0;
            if get(a, "customHeight").is_some_and(|v| v == "1" || v == "true") || (px - sheet.default_row_height).abs() > 0.5 { sheet.row_heights.insert(rr, px); }
        }
        if get(a, "hidden").is_some_and(|v| v == "1" || v == "true") { sheet.hidden_rows.insert(rr); }
        crate::outline::read_line(&mut sheet.outline, true, rr, get(a, "outlineLevel").and_then(|v| v.trim().parse().ok()), get(a, "collapsed").is_some_and(|v| v == "1" || v == "true"));
        if get(a, "customFormat").is_some_and(|v| v == "1" || v == "true") {
            if let Some(s) = get(a, "s").and_then(|v| v.parse::<usize>().ok()) { let st = ctx.xf.get(s).copied().unwrap_or(0); if st != 0 { sheet.row_styles.insert(rr, st); } }
        }
    };
    let mut buf = Vec::new();
    loop {
        let ev = r.read_event_into(&mut buf).map_err(|e| format!("worksheet XML error at {}: {e}", r.buffer_position()))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                match e.local_name().as_ref() {
                    b"row" => { let a = attrs(e); row_start(&a, &mut row, &mut col, sheet); }
                    b"c" => {
                        let a = attrs(e);
                        let pos = match get(&a, "r").and_then(|v| crate::addr::parse_a1(v)) {
                            Some(p) => { row = p.row as i64; col = p.col as i64; p }
                            None => { col += 1; if row < 0 { row = 0; } Pos::new(row as u32, (col as u32).min(MAX_COLS - 1)) }
                        };
                        let c = RawCell { pos, s: get(&a, "s").and_then(|v| v.parse().ok()).unwrap_or(0), t: get(&a, "t").unwrap_or("n").to_string(), cm: get(&a, "cm").is_some(), vm: get(&a, "vm").and_then(|v| v.parse().ok()), ..Default::default() };
                        if empty { finish(c, sheet, ctx, &mut shared, &mut arrays); } else { cell = Some(c); }
                    }
                    b"v" if !empty => inn = In::V,
                    b"f" => if let Some(c) = cell.as_mut() {
                        let a = attrs(e);
                        c.ft = get(&a, "t").unwrap_or("normal").to_string();
                        c.fsi = get(&a, "si").map(String::from);
                        c.fref = get(&a, "ref").map(String::from);
                        if c.ft == "dataTable" { c.f = Some(crate::whatif::table_formula_from_xml(&a.iter().map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), v.clone())).collect::<Vec<_>>())); c.ft = "array".into(); }
                        else if !empty { inn = In::F; c.f = Some(String::new()); }
                    },
                    b"is" if !empty => in_is = true,
                    b"t" if in_is && rph == 0 && !empty => inn = In::T,
                    b"rPh" if !empty => rph += 1,
                    _ => {}
                }
            }
            Event::Text(t) => if let Some(c) = cell.as_mut() {
                if inn == In::None { continue; }
                let s = t.unescape().map(|x| x.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned());
                match inn { In::V => c.v.push_str(&s), In::F => c.f.get_or_insert_with(String::new).push_str(&s), In::T => c.inline.push_str(&s), In::None => {} }
            },
            Event::CData(t) => if let Some(c) = cell.as_mut() {
                let s = String::from_utf8_lossy(&t);
                match inn { In::V => c.v.push_str(&s), In::F => c.f.get_or_insert_with(String::new).push_str(&s), In::T => c.inline.push_str(&s), In::None => {} }
            },
            Event::End(e) => match e.local_name().as_ref() {
                b"v" | b"f" | b"t" => inn = In::None,
                b"is" => in_is = false,
                b"rPh" => rph -= 1,
                b"c" => if let Some(c) = cell.take() { finish(c, sheet, ctx, &mut shared, &mut arrays); },
                b"sheetData" => return Ok(arrays),
                _ => {}
            },
            Event::Eof => return Ok(arrays),
            _ => {}
        }
        buf.clear();
    }
}

/// What a worksheet's elements other than cells say, gathered for after the cells are read.
#[derive(Default)]
struct SheetMeta { links: Vec<Element>, selected: bool }

fn top_level(name: &str, raw: &str, sheet: &mut Sheet, ctx: &Ctx, meta: &mut SheetMeta) {
    let el = || Element::parse(raw).ok();
    match name {
        "dimension" | "sheetCalcPr" => {}
        "sheetPr" => {
            if let Some(e) = el() {
                sheet.tab_color = e.child("tabColor").and_then(|c| ctx.pal.color(c));
                sheet.page.fit = e.child("pageSetUpPr").is_some_and(|p| p.flag("fitToPage", false));
                crate::outline::read_sheet_pr(&e, &mut sheet.outline);
            }
            sheet.extra.push(("sheetPr".into(), raw.into()));
        }
        "sheetViews" => if let Some(v) = el().and_then(|e| e.child("sheetView").cloned()) {
            sheet.gridlines = v.flag("showGridLines", true);
            sheet.headers = v.flag("showRowColHeaders", true);
            sheet.show_zeros = v.flag("showZeros", true);
            sheet.right_to_left = v.flag("rightToLeft", false);
            if let Some(z) = v.attr_u32("zoomScale").filter(|z| *z > 0) { sheet.zoom = z.clamp(10, 400); }
            meta.selected = v.flag("tabSelected", false);
            sheet.review.split = crate::review::read_split(&v);
            if let Some(p) = v.child("pane").filter(|p| matches!(p.attr("state"), Some("frozen" | "frozenSplit"))) {
                // The splits count the rows and columns shown frozen, from the view's top-left cell.
                let top = v.attr("topLeftCell").and_then(crate::addr::parse_a1).unwrap_or_default();
                let (xs, ys) = (p.attr_f64("xSplit").unwrap_or(0.0) as u32, p.attr_f64("ySplit").unwrap_or(0.0) as u32);
                sheet.frozen_cols = if xs > 0 { top.col + xs } else { 0 };
                sheet.frozen_rows = if ys > 0 { top.row + ys } else { 0 };
                sheet.frozen_origin = Pos::new(if ys > 0 { top.row } else { 0 }, if xs > 0 { top.col } else { 0 });
            }
            if let Some(s) = v.all("selection").last() {
                sheet.selection = s.attr("sqref").and_then(|q| q.split_whitespace().next()).and_then(parse_range).or_else(|| s.attr("activeCell").and_then(parse_range));
            }
        },
        "sheetFormatPr" => if let Some(e) = el() {
            if let Some(h) = e.attr_f64("defaultRowHeight").filter(|h| *h > 0.0) { sheet.default_row_height = ((h * 4.0 / 3.0 * 100.0).round() / 100.0) as f32; }
            if let Some(w) = e.attr_f64("defaultColWidth").filter(|w| *w > 0.0) { sheet.default_col_width = width_to_px(w); }
            else if let Some(b) = e.attr_f64("baseColWidth") { sheet.default_col_width = base_width_px(b); }
        },
        "cols" => if let Some(e) = el() {
            for c in e.all("col") {
                let (lo, hi) = (c.attr_u32("min").unwrap_or(1).max(1) - 1, c.attr_u32("max").unwrap_or(1).clamp(1, MAX_COLS) - 1);
                let w = c.attr_f64("width").map(width_to_px);
                let hidden = c.flag("hidden", false);
                let st = c.attr_u32("style").and_then(|s| ctx.xf.get(s as usize).copied()).unwrap_or(0);
                for i in lo..=hi.max(lo) {
                    if let Some(w) = w { if w != sheet.default_col_width || c.flag("customWidth", false) { sheet.col_widths.insert(i, w); } }
                    if hidden || w == Some(0.0) { sheet.hidden_cols.insert(i); }
                    if st != 0 { sheet.col_styles.insert(i, st); }
                    crate::outline::read_line(&mut sheet.outline, false, i, c.attr_u32("outlineLevel"), c.flag("collapsed", false));
                }
            }
        },
        "mergeCells" => if let Some(e) = el() { sheet.merges.extend(e.all("mergeCell").filter_map(|m| m.attr("ref").and_then(parse_range)).filter(|r| !r.is_cell())); },
        "hyperlinks" => if let Some(e) = el() { meta.links.extend(e.all("hyperlink").cloned()); },
        "sheetProtection" => {
            sheet.protected = el().is_some_and(|e| e.flag("sheet", false));
            sheet.extra.push(("sheetProtection".into(), raw.into()));
        }
        "autoFilter" => {
            sheet.autofilter = el().and_then(|e| e.attr("ref").and_then(parse_range));
            sheet.extra.push(("autoFilter".into(), raw.into()));
        }
        n if sheet.page.read_xml(n, raw) => {}
        n => sheet.extra.push((n.to_string(), raw.into())),
    }
}

/// Reads one worksheet part into `sheet`; returns its array formulas (anchor, range, dynamic).
fn worksheet(src: &str, sheet: &mut Sheet, ctx: &Ctx, meta: &mut SheetMeta) -> Result<Vec<(Pos, Range, bool)>, String> {
    let bytes = src.as_bytes();
    let mut r = Reader::from_reader(bytes);
    let mut depth = 0;
    let mut arrays = vec![];
    let mut buf = Vec::new();
    loop {
        let before = r.buffer_position() as usize;
        let ev = r.read_event_into(&mut buf).map_err(|e| format!("worksheet XML error at {}: {e}", r.buffer_position()))?;
        match ev {
            Event::Start(e) => {
                depth += 1;
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if depth == 1 {
                    let ns: String = e.attributes().with_checks(false).flatten().filter(|a| { let k = a.key.as_ref(); (k.starts_with(b"xmlns:") && k != b"xmlns:r") || k == b"mc:Ignorable" })
                        .map(|a| format!(" {}=\"{}\"", String::from_utf8_lossy(a.key.as_ref()), String::from_utf8_lossy(&a.value))).collect();
                    if !ns.is_empty() { sheet.extra.push(("xmlns".into(), ns)); }
                } else if depth == 2 {
                    if name == "sheetData" { arrays = sheet_data(&mut r, sheet, ctx)?; }
                    else {
                        let end = e.to_end().into_owned();
                        let mut skip = Vec::new();
                        r.read_to_end_into(end.name(), &mut skip).map_err(|e| e.to_string())?;
                        let after = r.buffer_position() as usize;
                        top_level(&name, src[before..after].trim(), sheet, ctx, meta);
                    }
                    depth -= 1;
                }
            }
            Event::Empty(e) => if depth == 1 {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let after = r.buffer_position() as usize;
                top_level(&name, src[before..after].trim(), sheet, ctx, meta);
            },
            Event::End(_) => depth -= 1,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(arrays)
}

/// A note's text without the "Author:" line Excel puts first (or the threaded-comment banner).
fn note_text(text: &str, author: &str) -> String {
    let mut t = text.replace("\r\n", "\n");
    if t.starts_with("[Threaded comment]") {
        if let Some(i) = t.find("Comment:\n") { t = t[i + 9..].to_string(); }
        return t.lines().map(str::trim).collect::<Vec<_>>().join("\n").trim().to_string();
    }
    if !author.is_empty() { if let Some(rest) = t.strip_prefix(author).and_then(|r| r.strip_prefix(':')) { t = rest.strip_prefix('\n').unwrap_or(rest).to_string(); } }
    t
}

pub fn read(bytes: &[u8]) -> Result<Workbook, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("This isn't an .xlsx file ({e})."))?;
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() { continue; }
        if f.encrypted() { return Err("This workbook is password-protected.".into()); }
        let name = f.name().trim_start_matches('/').replace('\\', "/");
        let mut data = Vec::with_capacity(f.size().min(1 << 28) as usize);
        f.read_to_end(&mut data).map_err(|e| e.to_string())?;
        files.insert(name, data);
    }
    if files.contains_key("EncryptedPackage") { return Err("This workbook is password-protected.".into()); }
    let mut pkg = Pkg { files, used: HashSet::new() };
    for n in ["_rels/.rels", "docProps/app.xml", "xl/calcChain.xml"] { if let Some(k) = pkg.key(n) { pkg.used.insert(k); } }
    let root_rels = pkg.rels("");
    let wb_path = root_rels.iter().find(|r| r.kind == "officeDocument" && !r.external).map(|r| r.target.clone())
        .filter(|t| pkg.key(t).is_some()).unwrap_or_else(|| "xl/workbook.xml".into());
    let wb_src = pkg.peek(&wb_path).ok_or("This file has no workbook part.")?;
    let wb_key = pkg.key(&wb_path).unwrap();
    pkg.used.insert(wb_key.clone());
    let wbx = Element::parse(&wb_src)?;
    let rels = pkg.rels(&wb_path);
    let canonical = wb_path == "xl/workbook.xml";
    let mut wb = Workbook::new();
    wb.sheets.clear();
    wb.date1904 = wbx.child("workbookPr").is_some_and(|p| p.flag("date1904", false));
    if let Some(c) = wbx.child("calcPr") {
        wb.manual = c.attr("calcMode") == Some("manual");
        wb.calc_no_tables = c.attr("calcMode") == Some("autoNoTable");
        if c.flag("iterate", false) { wb.iterate = Some((c.attr_u32("iterateCount").unwrap_or(100), c.attr_f64("iterateDelta").unwrap_or(0.001))); }
    }
    // Theme, then styles (which resolve theme colors).
    if let Some(t) = rels.iter().find(|r| r.kind == "theme") {
        if let Some(k) = pkg.key(&t.target) {
            if let Some(c) = theme_colors(&part_text(&pkg.files[&k])) { wb.theme_colors = c; }
            if let Some(t) = crate::theme::parse(&part_text(&pkg.files[&k])) { wb.theme = t; }
            pkg.used.insert(k.clone());
            let bytes = pkg.files[&k].clone();
            wb.parts.insert("xl/theme/theme1.xml".into(), bytes);
        }
    }
    let styles_el = rels.iter().find(|r| r.kind == "styles").and_then(|r| pkg.take(&r.target)).and_then(|t| Element::parse(&t).ok());
    let pal = Palette::new(wb.theme_colors.clone(), styles_el.as_ref());
    let dxfs: Vec<crate::condformat::Dxf> = styles_el.as_ref().and_then(|s| s.child("dxfs")).map(|d| d.all("dxf").map(|e| super::rules::read_dxf(e, &pal)).collect()).unwrap_or_default();
    let mut xf: Vec<u32> = vec![0];
    if let Some(s) = &styles_el {
        wb.styles.set_theme(wb.theme_colors.clone(), wb.theme.major.clone(), wb.theme.minor.clone());
        let (list, _normal) = styles::read(s, &pal);
        if let Some(first) = list.first() { wb.styles.set_default(first.clone()); }
        xf = list.into_iter().map(|st| wb.styles.intern(st)).collect();
        if xf.is_empty() { xf.push(0); }
        wb.parts.insert("xl/styles.xml".into(), s.to_xml().into_bytes());
    }
    let sst = rels.iter().find(|r| r.kind == "sharedStrings").and_then(|r| pkg.take(&r.target)).map(|t| shared_strings(&t)).unwrap_or_default();
    for r in &rels { if matches!(r.kind.as_str(), "calcChain" | "person") { if let Some(k) = pkg.key(&r.target) { pkg.used.insert(k); } } }
    if let Some(t) = rels.iter().find(|r| r.kind == "person").map(|r| r.target.clone()).and_then(|t| pkg.peek(&t)) { crate::review::read_persons(&mut wb, &t); }
    let ctx = Ctx { sst: &sst, xf: &xf, date1904: wb.date1904, pal: &pal };
    // Sheets in order; chart, dialog and macro sheets have no cells to show and are left out.
    let mut index_map: HashMap<usize, usize> = HashMap::new();
    let mut cached: Cached = vec![];
    
    let mut selected = None;
    let sheets: Vec<Element> = wbx.child("sheets").map(|s| s.all("sheet").cloned().collect()).unwrap_or_default();
    for (fi, se) in sheets.iter().enumerate() {
        let Some(rel) = se.attr("r:id").or(se.attr("id")).and_then(|id| rels.iter().find(|r| r.id == id)) else { continue };
        if rel.kind != "worksheet" { continue; }
        let Some(src) = pkg.take(&rel.target) else { continue };
        let name = se.attr("name").unwrap_or("Sheet").to_string();
        let mut sheet = Sheet::new(&name);
        sheet.hidden = matches!(se.attr("state"), Some("hidden" | "veryHidden"));
        let srels = pkg.rels(&rel.target);
        let mut meta = SheetMeta::default();
        let arrays = worksheet(&src, &mut sheet, &ctx, &mut meta)?;
        super::rules::take(&mut sheet, &dxfs, &pal);
        crate::sparkline::take(&mut sheet, &wb.theme_colors);
        let si = wb.sheets.len();
        if meta.selected && selected.is_none() { selected = Some(si); }
        // Array formulas: the other cells of the range hold cached results, which become the
        // spill values until recalculation (the engine owns that area).
        for (anchor, r, _dynamic) in arrays {
            let keys: Vec<Pos> = sheet.cells.range(Pos::new(r.r0, 0)..=Pos::new(r.r1, MAX_COLS)).map(|(p, _)| *p).filter(|p| r.contains(*p) && *p != anchor).collect();
            for p in keys {
                let c = sheet.cells.get_mut(&p).unwrap();
                if c.is_formula() { continue; }
                if !c.value.is_empty() { sheet.spill.insert(p, c.value.clone()); sheet.spill_anchor.insert(p, anchor); }
                c.input = Input::Empty; c.value = Value::Empty;
                if c.style == 0 { sheet.cells.remove(&p); }
            }
        }
        for (p, c) in &sheet.cells {
            if c.is_formula() && !c.value.is_empty() { cached.push((si, *p, c.value.clone())); }
        }
        // Links.
        for h in &meta.links {
            let Some(p) = h.attr("ref").and_then(parse_range).map(|r| r.start()) else { continue };
            let ext = h.attr("r:id").or(h.attr("id")).and_then(|id| srels.iter().find(|r| r.id == id)).map(|r| r.target.clone());
            let loc = h.attr("location").map(|l| format!("#{l}"));
            let target = match (ext, loc) { (Some(e), Some(l)) => format!("{e}{l}"), (Some(e), None) => e, (None, Some(l)) => l, _ => continue };
            sheet.links.insert(p, Hyperlink { target, tooltip: h.attr("tooltip").map(String::from) });
        }
        // Notes from the legacy comments part; the VML that draws them is regenerated.
        let mut has_notes = false;
        if let Some(c) = srels.iter().find(|r| r.kind == "comments").and_then(|r| pkg.take(&r.target)).and_then(|t| Element::parse(&t).ok()) {
            let authors: Vec<String> = c.child("authors").map(|a| a.all("author").map(|x| x.text()).collect()).unwrap_or_default();
            for cm in c.child("commentList").map(|l| l.all("comment").collect::<Vec<_>>()).unwrap_or_default() {
                let Some(p) = cm.attr("ref").and_then(parse_range).map(|r| r.start()) else { continue };
                let author = cm.attr_u32("authorId").and_then(|i| authors.get(i as usize)).map(String::as_str).unwrap_or("");
                let text = cm.child("text").map(rich_text).unwrap_or_default();
                sheet.notes.insert(p, note_text(&text, author));
                has_notes = true;
            }
        }
        let mut dropped: HashSet<String> = HashSet::new();
        // Header/footer pictures (hfpic.rs) from the legacyDrawingHF VML part.
        if let Some(i) = sheet.extra.iter().position(|(n, _)| n == "legacyDrawingHF") {
            let id = Element::parse(&sheet.extra[i].1).ok().and_then(|e| e.attr("r:id").or(e.attr("id")).map(String::from));
            if let Some(r) = id.as_ref().and_then(|id| srels.iter().find(|r| &r.id == id && !r.external)).cloned() {
                if let Some(vml) = pkg.take(&r.target) {
                    let images: Vec<(String, Vec<u8>, String)> = pkg.rels(&r.target).into_iter().filter_map(|vr| {
                        let k = pkg.key(&vr.target)?;
                        pkg.used.insert(k.clone());
                        Some((vr.id.clone(), pkg.files[&k].clone(), k.rsplit('.').next().unwrap_or("png").to_ascii_lowercase()))
                    }).collect();
                    sheet.page.header_footer.pictures = crate::hfpic::read_vml(&vml, &images);
                    sheet.extra.remove(i);
                    dropped.insert(r.id.clone());
                }
            }
        }
        if has_notes {
            if let Some(i) = sheet.extra.iter().position(|(n, _)| n == "legacyDrawing") {
                // Kept when it also holds objects (OLE, signature lines; vml_objects.rs).
                let id = Element::parse(&sheet.extra[i].1).ok().and_then(|e| e.attr("r:id").or(e.attr("id")).map(String::from));
                let objects = id.as_ref().and_then(|id| srels.iter().find(|r| &r.id == id && !r.external)).and_then(|r| pkg.peek(&r.target)).is_some_and(|t| super::vml_objects::has_objects(&t));
                if !objects { sheet.extra.remove(i); if let Some(id) = id { dropped.insert(id); } }
            }
        }
        for r in &srels {
            if r.kind == "threadedComment" && !r.external { if let Some(t) = pkg.take(&r.target) { crate::review::read_threads(&mut sheet, &t); } continue; }
            if matches!(r.kind.as_str(), "hyperlink" | "comments" | "threadedComment") { if !r.external { if let Some(k) = pkg.key(&r.target) { pkg.used.insert(k); } } continue; }
            if dropped.contains(&r.id) { if let Some(k) = pkg.key(&r.target) { pkg.used.insert(k); } continue; }
            // Tables are modelled (crate::tables) and written afresh.
            if r.kind == "table" && !r.external { if let Some(t) = pkg.take(&r.target).and_then(|x| crate::tables::table_from_xml(&x)) { sheet.tables.push(t); } continue; }
            let target = if r.external { r.target.clone() } else { pkg.key(&r.target).unwrap_or(r.target.clone()) };
            sheet.extra.push(("rel".into(), format!("{}\t{}\t{}\t{}", r.id, r.full_kind, target, r.external as u8)));
        }
        index_map.insert(fi, si);
        wb.sheets.push(sheet);
        crate::tables::after_read(&mut wb, si);
    }
    if wb.sheets.is_empty() { wb.sheets.push(Sheet::new("Sheet1")); }
    // Names, with sheet scopes renumbered past any left-out sheets.
    if let Some(dn) = wbx.child("definedNames") {
        for d in dn.all("definedName") {
            let Some(name) = d.attr("name") else { continue };
            let sheet = match d.attr_u32("localSheetId") { Some(i) => match index_map.get(&(i as usize)) { Some(&s) => Some(s), None => continue }, None => None };
            wb.names.push(DefinedName { name: name.into(), sheet, formula: from_file_formula(d.text().trim()), hidden: d.flag("hidden", false), comment: d.attr("comment").unwrap_or("").into() });
        }
    }
    crate::review::after_read(&mut wb, &wbx);
    let tab = wbx.path("bookViews/workbookView").and_then(|v| v.attr_u32("activeTab")).and_then(|t| index_map.get(&(t as usize)).copied());
    wb.active = tab.or(selected).unwrap_or(0).min(wb.sheets.len() - 1);
    if wb.sheets[wb.active].hidden { wb.active = wb.sheets.iter().position(|s| !s.hidden).unwrap_or(0); }
    // Everything not read into the model is carried; the content types, workbook and its rels
    // are kept for reference when the workbook part sits where writing puts it.
    if canonical {
        wb.parts.insert("xl/workbook.xml".into(), pkg.files[&wb_key].clone());
        if let Some(k) = pkg.key("xl/_rels/workbook.xml.rels") { wb.parts.insert("xl/_rels/workbook.xml.rels".into(), pkg.files[&k].clone()); }
    }
    for (k, v) in &pkg.files {
        if pkg.used.contains(k) || k.ends_with(".rels") && !keep_rels(k, &pkg) { continue; }
        if k.starts_with("xl/chartsheets/") || k.starts_with("xl/dialogsheets/") || k.starts_with("xl/macrosheets/") { continue; }
        wb.parts.entry(k.clone()).or_insert_with(|| v.clone());
    }
    crate::drawing::after_read(&mut wb);
    super::richdata::load(&mut wb);
    crate::cellimage::after_read(&mut wb);
    super::vml_objects::after_read(&mut wb);
    // Recalculation: always (it rebuilds the spill areas), except in manual mode where the
    // file's values stand. Where the engine can't evaluate a formula (#NAME? from a function
    // it doesn't have yet) the file's cached value is kept instead.
    if !wb.manual {
        wb.recalc();
        for (si, p, v) in cached {
            if let Some(c) = wb.sheets[si].cells.get_mut(&p) {
                let unknown = c.value == Value::Err(ErrKind::Name) || matches!(&c.input, Input::Formula(f) if f.ast.is_err());
                if unknown && v != Value::Err(ErrKind::Name) { c.value = v; }
            }
        }
    }
    Ok(wb)
}

/// Rels parts of carried parts (drawings, charts, pivot tables) are carried with them; rels of
/// parts the reader consumed are not.
fn keep_rels(k: &str, pkg: &Pkg) -> bool {
    let Some(i) = k.rfind("/_rels/") else { return false };
    let owner = format!("{}/{}", &k[..i], k[i + 7..].trim_end_matches(".rels"));
    pkg.files.contains_key(&owner) && !pkg.used.contains(&owner) && owner != "xl/workbook.xml"
}

