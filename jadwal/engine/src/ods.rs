//! OpenDocument spreadsheets (.ods), as io.js reads and writes them: sheets, values (float,
//! percentage, currency, date, time, boolean, string), formulas (OpenFormula `of:=` with
//! `[.A1]` references, converted to and from Excel syntax), cell styles (font, size, bold,
//! italic, underline, strike, color, fill, alignment, wrap, number format), column widths, row
//! heights, hidden rows, columns and sheets, merged cells, notes (office:annotation) and the
//! active sheet.

use crate::addr::{Pos, Range};
use crate::book::{Input, Sheet, Workbook};
use crate::formula::lexer::{tokenize, Tok};
use crate::style::{HAlign, Style, Underline, VAlign};
use crate::value::Value;
use crate::xlsx::xml::{attr, text, Element, Node};
use std::collections::BTreeMap;
use std::io::{Read, Write};

const MIME: &str = "application/vnd.oasis.opendocument.spreadsheet";

// ---------------------------------------------------------------------------------------------
// Formulas

/// `of:=SUM([.A1:.B2];[$Sheet2.C3])` → `SUM(A1:B2,Sheet2!C3)`.
pub fn from_of(src: &str) -> String {
    let s = src.strip_prefix("of:").or_else(|| src.strip_prefix("msoxl:")).unwrap_or(src);
    let s = s.strip_prefix('=').unwrap_or(s);
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c); i += 1;
            while i < chars.len() { out.push(chars[i]); if chars[i] == '"' { if chars.get(i + 1) == Some(&'"') { out.push('"'); i += 2; continue; } i += 1; break; } i += 1; }
            continue;
        }
        if c == '[' {
            let end = chars[i..].iter().position(|x| *x == ']').map(|e| i + e).unwrap_or(chars.len());
            let body: String = chars[i + 1..end].iter().collect();
            out.push_str(&ref_from_of(&body));
            i = end + 1;
            continue;
        }
        out.push(if c == ';' { ',' } else { c });
        i += 1;
    }
    out
}

fn ref_from_of(body: &str) -> String {
    let parts: Vec<String> = body.split(':').map(|p| {
        let p = p.trim();
        match p.rfind('.') {
            Some(0) => p[1..].to_string(),
            Some(k) => {
                let sheet = p[..k].trim_start_matches('$').trim_matches('\'').replace("''", "'");
                let quoted = if sheet.chars().all(|c| c.is_alphanumeric() || c == '_') { sheet } else { format!("'{}'", sheet.replace('\'', "''")) };
                format!("{quoted}!{}", &p[k + 1..])
            }
            None => p.to_string(),
        }
    }).collect();
    // A2:Sheet!B3 keeps one sheet name (Excel's form).
    if parts.len() == 2 { if let (Some((s1, a)), Some((s2, b))) = (parts[0].split_once('!'), parts[1].split_once('!')) { if s1 == s2 { return format!("{s1}!{a}:{b}"); } } }
    parts.join(":")
}

/// `SUM(A1:B2,Sheet2!C3)` → `of:=SUM([.A1:.B2];[$Sheet2.C3])`.
pub fn to_of(src: &str) -> String {
    let Ok(tokens) = tokenize(src) else { return format!("of:={src}") };
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::from("of:=");
    let mut at = 0;
    for t in &tokens {
        out.extend(&chars[at..t.start]);
        let orig: String = chars[t.start..t.end].iter().collect();
        match &t.tok {
            Tok::Ref { sheet, body, .. } => {
                let pre = match sheet { Some(s) => format!("${}", if s.chars().all(|c| c.is_alphanumeric() || c == '_') { s.clone() } else { format!("'{}'", s.replace('\'', "''")) }), None => String::new() };
                let parts: Vec<String> = body.split(':').enumerate().map(|(k, p)| if k == 0 { format!("{pre}.{p}") } else { format!(".{p}") }).collect();
                out.push_str(&format!("[{}]", parts.join(":")));
            }
            Tok::Sep(',') => out.push(';'),
            _ => out.push_str(&orig),
        }
        at = t.end;
    }
    out.extend(&chars[at.min(chars.len())..]);
    out
}

// ---------------------------------------------------------------------------------------------
// Lengths and colors

fn length_px(s: &str) -> Option<f32> {
    let s = s.trim();
    let k = s.find(|c: char| c.is_alphabetic())?;
    let v: f32 = s[..k].parse().ok()?;
    Some(match &s[k..] { "cm" => v * 96.0 / 2.54, "mm" => v * 96.0 / 25.4, "in" => v * 96.0, "pt" => v * 4.0 / 3.0, "pc" => v * 16.0, "px" => v, _ => return None })
}
fn px_cm(px: f32) -> String { format!("{:.3}cm", px * 2.54 / 96.0) }
fn color(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 { return None; }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}
fn hex(c: [u8; 3]) -> String { format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]) }

// ---------------------------------------------------------------------------------------------
// Reading

#[derive(Default, Clone)]
struct OdsStyle { style: Style, data: Option<String>, width: Option<f32>, height: Option<f32>, hidden_table: bool }

fn number_format(e: &Element) -> String {
    let mut code = String::new();
    for c in e.elements() {
        match c.local() {
            "number" => {
                let d = c.attr_u32("number:decimal-places").unwrap_or(0) as usize;
                code.push_str(if c.flag("number:grouping", false) { "#,##0" } else { "0" });
                if d > 0 { code.push('.'); code.push_str(&"0".repeat(d)); }
            }
            "scientific-number" => { let d = c.attr_u32("number:decimal-places").unwrap_or(2) as usize; code.push_str(&format!("0.{}E+00", "0".repeat(d))); }
            "text" => { let t = c.text(); if t == "%" { code.push('%'); } else if !t.is_empty() { code.push_str(&format!("\"{}\"", t.replace('"', ""))); } }
            "currency-symbol" => code.push_str(&format!("\"{}\"", c.text())),
            "year" => code.push_str(if c.attr("number:style") == Some("long") { "yyyy" } else { "yy" }),
            "month" => code.push_str(if c.flag("number:textual", false) { if c.attr("number:style") == Some("long") { "mmmm" } else { "mmm" } } else if c.attr("number:style") == Some("long") { "mm" } else { "m" }),
            "day" => code.push_str(if c.attr("number:style") == Some("long") { "dd" } else { "d" }),
            "day-of-week" => code.push_str(if c.attr("number:style") == Some("long") { "dddd" } else { "ddd" }),
            "hours" => code.push_str(if c.attr("number:style") == Some("long") { "hh" } else { "h" }),
            "minutes" => code.push_str("mm"),
            "seconds" => code.push_str("ss"),
            "am-pm" => code.push_str(" AM/PM"),
            "text-content" => code.push('@'),
            _ => {}
        }
    }
    let code = code.replace("\"/\"", "/").replace("\":\"", ":").replace("\"-\"", "-").replace("\" \"", " ").replace("\".\"", ".");
    if code.is_empty() { "General".into() } else { code }
}

fn read_styles(root: &Element, into: &mut BTreeMap<String, OdsStyle>, formats: &mut BTreeMap<String, String>) {
    for sec in root.elements().filter(|e| matches!(e.local(), "automatic-styles" | "styles")) {
        for e in sec.elements() {
            let Some(name) = e.attr("style:name") else { continue };
            match e.local() {
                "number-style" | "percentage-style" | "currency-style" | "date-style" | "time-style" | "text-style" => { formats.insert(name.into(), number_format(e)); }
                "style" => {
                    let mut o = into.get(e.attr("style:parent-style-name").unwrap_or("")).cloned().unwrap_or_default();
                    o.data = e.attr("style:data-style-name").map(String::from).or(o.data);
                    for c in e.elements() {
                        let a = |k: &str| c.attr(k);
                        match c.local() {
                            "text-properties" => {
                                if let Some(w) = a("fo:font-weight") { o.style.bold = w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600); }
                                if let Some(w) = a("fo:font-style") { o.style.italic = w == "italic" || w == "oblique"; }
                                if let Some(f) = a("style:font-name").or(a("fo:font-family")) { o.style.font = f.trim_matches('\'').into(); }
                                if let Some(sz) = a("fo:font-size").and_then(length_px) { o.style.size = (sz * 0.75 * 10.0).round() as u32; }
                                if let Some(c) = a("fo:color").and_then(color) { o.style.color = Some(c); }
                                if a("style:text-underline-style").is_some_and(|u| u != "none") { o.style.underline = if a("style:text-underline-type") == Some("double") { Underline::Double } else { Underline::Single }; }
                                if a("style:text-line-through-style").is_some_and(|u| u != "none") { o.style.strike = true; }
                            }
                            "table-cell-properties" => {
                                if let Some(c) = a("fo:background-color").and_then(color) { o.style.fill = Some(c); }
                                if a("fo:wrap-option") == Some("wrap") { o.style.wrap = true; }
                                match a("style:vertical-align") { Some("top") => o.style.v = VAlign::Top, Some("middle") => o.style.v = VAlign::Center, _ => {} }
                                if a("style:cell-protect") == Some("none") { o.style.locked = false; }
                            }
                            "paragraph-properties" => match a("fo:text-align") { Some("center") => o.style.h = HAlign::Center, Some("end" | "right") => o.style.h = HAlign::Right, Some("start" | "left") => o.style.h = HAlign::Left, Some("justify") => o.style.h = HAlign::Justify, _ => {} },
                            "table-column-properties" => o.width = a("style:column-width").and_then(length_px),
                            "table-row-properties" => o.height = a("style:row-height").and_then(length_px),
                            "table-properties" => o.hidden_table = a("table:display") == Some("false"),
                            _ => {}
                        }
                    }
                    into.insert(name.into(), o);
                }
                _ => {}
            }
        }
    }
}

fn cell_text(e: &Element) -> String {
    fn walk(e: &Element, out: &mut String) {
        for n in &e.children {
            match n {
                Node::Text(t) => out.push_str(t),
                Node::Element(c) => match c.local() {
                    "s" => out.push_str(&" ".repeat(c.attr_u32("text:c").unwrap_or(1) as usize)),
                    "tab" => out.push('\t'),
                    "line-break" => out.push('\n'),
                    "annotation" => {}
                    _ => walk(c, out),
                },
            }
        }
    }
    let mut out = String::new();
    for (k, p) in e.all("p").enumerate() { if k > 0 { out.push('\n'); } walk(p, &mut out); }
    out
}

fn date_serial(s: &str, date1904: bool) -> Option<f64> {
    let (d, t) = s.split_once('T').unwrap_or((s, ""));
    let mut it = d.split('-');
    let (y, m, dd): (i64, u32, u32) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    let days = crate::date::days_from_civil(y, m, dd) - crate::date::days_from_civil(if date1904 { 1904 } else { 1899 }, if date1904 { 1 } else { 12 }, if date1904 { 1 } else { 30 });
    let mut frac = 0.0;
    if !t.is_empty() { let v: Vec<f64> = t.split(':').filter_map(|x| x.parse().ok()).collect(); frac = (v.first().unwrap_or(&0.0) * 3600.0 + v.get(1).unwrap_or(&0.0) * 60.0 + v.get(2).unwrap_or(&0.0)) / 86400.0; }
    Some(days as f64 + frac)
}

fn duration_days(s: &str) -> Option<f64> {
    // PT12H30M00S
    let s = s.strip_prefix('-').unwrap_or(s).strip_prefix('P')?;
    let (days, t) = s.split_once('T').unwrap_or((s, ""));
    let mut total = days.trim_end_matches('D').parse::<f64>().unwrap_or(0.0) * 86400.0;
    let mut num = String::new();
    for c in t.chars() { if c.is_ascii_digit() || c == '.' { num.push(c); } else { let v: f64 = num.parse().unwrap_or(0.0); num.clear(); total += v * match c { 'H' => 3600.0, 'M' => 60.0, _ => 1.0 }; } }
    Some(total / 86400.0)
}

pub fn read(bytes: &[u8]) -> Result<Workbook, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("This isn't an .ods file ({e})."))?;
    let mut part = |n: &str| -> Option<String> { let mut f = zip.by_name(n).ok()?; let mut s = String::new(); f.read_to_string(&mut s).ok()?; Some(s) };
    let content = part("content.xml").ok_or("This file has no content.xml.")?;
    let styles_xml = part("styles.xml");
    let settings = part("settings.xml");
    let root = Element::parse(&content)?;
    let mut styles = BTreeMap::new();
    let mut formats = BTreeMap::new();
    if let Some(s) = styles_xml.as_deref().and_then(|s| Element::parse(s).ok()) { read_styles(&s, &mut styles, &mut formats); }
    read_styles(&root, &mut styles, &mut formats);
    let mut wb = Workbook::new();
    wb.sheets.clear();
    let body = root.path("body/spreadsheet").ok_or("This file has no spreadsheet.")?;
    let mut interned: BTreeMap<String, u32> = BTreeMap::new();
    for t in body.all("table") {
        let mut sheet = Sheet::new(t.attr("table:name").unwrap_or("Sheet"));
        if let Some(st) = t.attr("table:style-name").and_then(|n| styles.get(n)) { sheet.hidden = st.hidden_table; }
        let si = wb.sheets.len();
        wb.sheets.push(sheet);
        let mut col = 0u32;
        let mut cols_default: BTreeMap<u32, String> = BTreeMap::new();
        let mut columns: Vec<&Element> = vec![];
        for e in t.elements() { if e.local() == "table-column" { columns.push(e); } else if e.local() == "table-columns" || e.local() == "table-header-columns" { columns.extend(e.all("table-column")); } }
        for c in columns {
            let n = c.attr_u32("table:number-columns-repeated").unwrap_or(1).min(16384);
            let w = c.attr("table:style-name").and_then(|s| styles.get(s)).and_then(|s| s.width);
            for k in col..(col + n).min(16384) {
                if let Some(w) = w { if n < 1000 { wb.sheets[si].col_widths.insert(k, w.round()); } }
                if c.attr("table:visibility").is_some_and(|v| v != "visible") { wb.sheets[si].hidden_cols.insert(k); }
                if let Some(d) = c.attr("table:default-cell-style-name") { cols_default.insert(k, d.to_string()); }
            }
            col += n;
        }
        let mut rows: Vec<&Element> = vec![];
        for e in t.elements() { match e.local() { "table-row" => rows.push(e), "table-rows" | "table-header-rows" | "table-row-group" => rows.extend(e.all("table-row")), _ => {} } }
        let mut r = 0u32;
        for row in rows {
            let rn = row.attr_u32("table:number-rows-repeated").unwrap_or(1);
            let has_content = row.elements().any(|c| !c.children.is_empty() || c.attr("office:value-type").is_some());
            let reps = if has_content { rn.min(1000) } else { rn };
            if !has_content {
                if row.attr("table:visibility").is_some_and(|v| v != "visible") && rn < 10_000 { for k in r..r + rn { wb.sheets[si].hidden_rows.insert(k); } }
                if let Some(h) = row.attr("table:style-name").and_then(|s| styles.get(s)).and_then(|s| s.height) { if rn < 10_000 { for k in r..r + rn { if (h - 20.0).abs() > 0.6 { wb.sheets[si].row_heights.insert(k, h.round()); } } } }
                r = r.saturating_add(rn);
                if r >= crate::addr::MAX_ROWS { break; }
                continue;
            }
            for rep in 0..reps {
                let rr = r + rep;
                if row.attr("table:visibility").is_some_and(|v| v != "visible") { wb.sheets[si].hidden_rows.insert(rr); }
                if let Some(h) = row.attr("table:style-name").and_then(|s| styles.get(s)).and_then(|s| s.height) { if (h - 20.0).abs() > 0.6 { wb.sheets[si].row_heights.insert(rr, h.round()); } }
                let mut c = 0u32;
                for cell in row.elements().filter(|e| matches!(e.local(), "table-cell" | "covered-table-cell")) {
                    let cn = cell.attr_u32("table:number-columns-repeated").unwrap_or(1);
                    let covered = cell.local() == "covered-table-cell";
                    let style_name = cell.attr("table:style-name").map(String::from).or_else(|| cols_default.get(&c).cloned());
                    let vt = cell.attr("office:value-type");
                    let formula = cell.attr("table:formula");
                    let txt = cell_text(cell);
                    let note = cell.child("annotation").map(|a| a.all("p").map(|p| p.text()).collect::<Vec<_>>().join("\n"));
                    let empty = vt.is_none() && formula.is_none() && txt.is_empty() && note.is_none();
                    let reps_c = if empty && style_name.is_none() { 0 } else { cn.min(1024) };
                    for k in 0..reps_c {
                        let p = Pos::new(rr, c + k);
                        if covered { break; }
                        let mut fmt: Option<String> = None;
                        let mut sidx = 0u32;
                        if let Some(sn) = &style_name {
                            if let Some(i) = interned.get(sn) { sidx = *i; } else if let Some(o) = styles.get(sn) {
                                let mut st = o.style.clone();
                                if st.font.is_empty() { st.font = wb.styles.get(0).font.clone(); }
                                if st.size == 0 { st.size = wb.styles.get(0).size; }
                                st.num_fmt = o.data.as_ref().and_then(|d| formats.get(d)).cloned().unwrap_or_else(|| "General".into());
                                sidx = wb.styles.intern(st); interned.insert(sn.clone(), sidx);
                            }
                        }
                        let value = match vt {
                            Some("float") => cell.attr_f64("office:value").map(Value::Num),
                            Some("percentage") => { fmt = Some("0%".into()); cell.attr_f64("office:value").map(Value::Num) }
                            Some("currency") => { fmt = Some("#,##0.00".into()); cell.attr_f64("office:value").map(Value::Num) }
                            Some("date") => { fmt = Some("yyyy-mm-dd".into()); cell.attr("office:date-value").and_then(|d| date_serial(d, wb.date1904)).map(Value::Num) }
                            Some("time") => { fmt = Some("h:mm:ss".into()); cell.attr("office:time-value").and_then(duration_days).map(Value::Num) }
                            Some("boolean") => Some(Value::Bool(cell.attr("office:boolean-value").is_some_and(|b| b == "true" || b == "1"))),
                            Some("string") | None if !txt.is_empty() => Some(Value::str(cell.attr("office:string-value").map(String::from).unwrap_or(txt.clone()))),
                            _ => None,
                        };
                        if sidx != 0 { wb.sheets[si].cells.entry(p).or_default().style = sidx; }
                        if let Some(f) = formula {
                            wb.set_input(si, p, &format!("={}", from_of(f)));
                            if let (Some(v), Some(c)) = (value, wb.sheets[si].cells.get_mut(&p)) { c.value = v; }
                        } else if let Some(v) = value {
                            let has_fmt = wb.styles.get(sidx).num_fmt != "General";
                            wb.set_cell_input(si, p, Input::Value(v), if has_fmt { None } else { fmt.clone() });
                        }
                        if let Some(n) = &note { wb.sheets[si].notes.insert(p, n.clone()); }
                        let cs = cell.attr_u32("table:number-columns-spanned").unwrap_or(1);
                        let rs = cell.attr_u32("table:number-rows-spanned").unwrap_or(1);
                        if cs > 1 || rs > 1 { wb.sheets[si].merges.push(Range { r0: rr, c0: c + k, r1: rr + rs - 1, c1: c + k + cs - 1 }); }
                    }
                    c = c.saturating_add(cn);
                }
            }
            r = r.saturating_add(rn);
            if r >= crate::addr::MAX_ROWS { break; }
        }
    }
    if wb.sheets.is_empty() { wb.sheets.push(Sheet::new("Sheet1")); }
    // The active sheet from settings.xml.
    if let Some(e) = settings.as_deref().and_then(|s| Element::parse(s).ok()) {
        fn find<'a>(e: &'a Element, name: &str) -> Option<&'a Element> { if e.attr("config:name") == Some(name) { return Some(e); } e.elements().find_map(|c| find(c, name)) }
        if let Some(a) = find(&e, "ActiveTable").map(|a| a.text()) { if let Some(i) = wb.sheet_index(a.trim()) { wb.active = i; } }
    }
    if wb.sheets[wb.active].hidden { wb.active = wb.sheets.iter().position(|s| !s.hidden).unwrap_or(0); }
    wb.recalc();
    Ok(wb)
}

// ---------------------------------------------------------------------------------------------
// Writing

fn number_style_xml(name: &str, code: &str) -> (String, &'static str) {
    let c = code.to_ascii_lowercase();
    if crate::numfmt::is_date_format(code) {
        let mut o = format!("<number:date-style style:name=\"{name}\">");
        let mut i = 0;
        let ch: Vec<char> = c.chars().collect();
        while i < ch.len() {
            let run = ch[i..].iter().take_while(|x| **x == ch[i]).count();
            let time_ctx = ch[..i].iter().rev().find(|x| x.is_alphabetic()).is_some_and(|x| *x == 'h') || ch[i + run..].iter().find(|x| x.is_alphabetic()).is_some_and(|x| *x == 's');
            match ch[i] {
                'y' => o.push_str(&format!("<number:year{}/>", if run > 2 { " number:style=\"long\"" } else { "" })),
                'm' if time_ctx && run <= 2 => o.push_str("<number:minutes number:style=\"long\"/>"),
                'm' => o.push_str(&match run { 1 => "<number:month/>".to_string(), 2 => "<number:month number:style=\"long\"/>".into(), 3 => "<number:month number:textual=\"true\"/>".into(), _ => "<number:month number:style=\"long\" number:textual=\"true\"/>".into() }),
                'd' => o.push_str(match run { 1 => "<number:day/>", 2 => "<number:day number:style=\"long\"/>", 3 => "<number:day-of-week/>", _ => "<number:day-of-week number:style=\"long\"/>" }),
                'h' => o.push_str(if run > 1 { "<number:hours number:style=\"long\"/>" } else { "<number:hours/>" }),
                's' => o.push_str("<number:seconds number:style=\"long\"/>"),
                'a' if c[i..].starts_with("am/pm") => { o.push_str("<number:am-pm/>"); i += 5; continue; }
                x if !x.is_alphanumeric() && x != '"' && x != '\\' => o.push_str(&format!("<number:text>{}</number:text>", text(&x.to_string().repeat(run)))),
                _ => {}
            }
            i += run;
        }
        return (o + "</number:date-style>", "date");
    }
    let first = code.split(';').next().unwrap_or(code);
    let decimals = first.split_once('.').map(|(_, d)| d.chars().take_while(|c| *c == '0' || *c == '#').count()).unwrap_or(0);
    let grouping = first.contains(',');
    let num = format!("<number:number number:decimal-places=\"{decimals}\" number:min-decimal-places=\"{decimals}\" number:min-integer-digits=\"1\"{}/>", if grouping { " number:grouping=\"true\"" } else { "" });
    if first.contains('%') { return (format!("<number:percentage-style style:name=\"{name}\">{num}<number:text>%</number:text></number:percentage-style>"), "percentage"); }
    if first.contains('E') || first.contains('e') { return (format!("<number:number-style style:name=\"{name}\"><number:scientific-number number:decimal-places=\"{decimals}\" number:min-integer-digits=\"1\" number:min-exponent-digits=\"2\"/></number:number-style>"), "float"); }
    if first == "@" { return (format!("<number:text-style style:name=\"{name}\"><number:text-content/></number:text-style>"), "string"); }
    let cur: String = first.split('"').skip(1).step_by(2).collect::<Vec<_>>().join("");
    let pre = first.trim_start().starts_with(['$', '€', '£', '¥', '"']);
    let lit = if cur.is_empty() { first.chars().filter(|c| matches!(c, '$' | '€' | '£' | '¥')).collect::<String>() } else { cur };
    let t = if lit.is_empty() { String::new() } else { format!("<number:text>{}</number:text>", text(&lit)) };
    let body = if pre { format!("{t}{num}") } else { format!("{num}{t}") };
    (format!("<number:number-style style:name=\"{name}\">{body}</number:number-style>"), "float")
}

fn cell_style_xml(name: &str, st: &Style, data: Option<&str>) -> String {
    let mut o = format!("<style:style style:name=\"{name}\" style:family=\"table-cell\" style:parent-style-name=\"Default\"");
    if let Some(d) = data { o.push_str(&format!(" style:data-style-name=\"{d}\"")); }
    o.push('>');
    let mut cp = String::new();
    if let Some(f) = st.fill { cp.push_str(&format!(" fo:background-color=\"{}\"", hex(f))); }
    if st.wrap { cp.push_str(" fo:wrap-option=\"wrap\""); }
    match st.v { VAlign::Top => cp.push_str(" style:vertical-align=\"top\""), VAlign::Center => cp.push_str(" style:vertical-align=\"middle\""), _ => {} }
    if !st.locked { cp.push_str(" style:cell-protect=\"none\""); }
    if !cp.is_empty() { o.push_str(&format!("<style:table-cell-properties{cp}/>")); }
    let align = match st.h { HAlign::Center | HAlign::CenterAcross => Some("center"), HAlign::Right => Some("end"), HAlign::Left => Some("start"), HAlign::Justify => Some("justify"), _ => None };
    if let Some(a) = align { o.push_str(&format!("<style:paragraph-properties fo:text-align=\"{a}\"/>")); }
    let mut tp = format!(" style:font-name=\"{}\" fo:font-size=\"{}pt\"", attr(&st.font), st.size as f32 / 10.0);
    if st.bold { tp.push_str(" fo:font-weight=\"bold\""); }
    if st.italic { tp.push_str(" fo:font-style=\"italic\""); }
    if let Some(c) = st.color { tp.push_str(&format!(" fo:color=\"{}\"", hex(c))); }
    if st.underline != Underline::None { tp.push_str(&format!(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"{}", if matches!(st.underline, Underline::Double | Underline::DoubleAccounting) { " style:text-underline-type=\"double\"" } else { "" })); }
    if st.strike { tp.push_str(" style:text-line-through-style=\"solid\""); }
    o.push_str(&format!("<style:text-properties{tp}/>"));
    o + "</style:style>"
}

fn iso_date(serial: f64, date1904: bool) -> String {
    let base = crate::date::days_from_civil(if date1904 { 1904 } else { 1899 }, if date1904 { 1 } else { 12 }, if date1904 { 1 } else { 30 });
    let (y, m, d) = crate::date::civil_from_days(base + serial.floor() as i64);
    let secs = ((serial - serial.floor()) * 86400.0).round() as i64;
    if secs == 0 { format!("{y:04}-{m:02}-{d:02}") } else { format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60) }
}

pub fn write(wb: &Workbook) -> Result<Vec<u8>, String> {
    let mut auto = String::new();
    // Styles in use: one cell style each, with its number style.
    let mut used: BTreeMap<u32, String> = BTreeMap::new();
    let mut data_kind: BTreeMap<u32, &'static str> = BTreeMap::new();
    for s in &wb.sheets { for c in s.cells.values() { used.entry(c.style).or_default(); } for st in s.col_styles.values() { used.entry(*st).or_default(); } }
    let mut k = 0;
    for (i, name) in used.iter_mut() {
        let st = wb.styles.get(*i);
        k += 1;
        *name = format!("ce{k}");
        let data = (st.num_fmt != "General").then(|| { let (x, kind) = number_style_xml(&format!("N{k}"), &st.num_fmt); auto.push_str(&x); data_kind.insert(*i, kind); format!("N{k}") });
        auto.push_str(&cell_style_xml(name, st, data.as_deref()));
    }
    // Column and row sizes.
    let mut col_styles: BTreeMap<String, String> = BTreeMap::new();
    let mut row_styles: BTreeMap<String, String> = BTreeMap::new();
    let sizes = |map: &mut BTreeMap<String, String>, prefix: &str, px: f32, auto: &mut String, family: &str| -> String {
        let key = px_cm(px);
        if let Some(n) = map.get(&key) { return n.clone(); }
        let n = format!("{prefix}{}", map.len() + 1);
        if family == "table-column" { auto.push_str(&format!("<style:style style:name=\"{n}\" style:family=\"table-column\"><style:table-column-properties fo:break-before=\"auto\" style:column-width=\"{key}\"/></style:style>")); }
        else { auto.push_str(&format!("<style:style style:name=\"{n}\" style:family=\"table-row\"><style:table-row-properties style:row-height=\"{key}\" fo:break-before=\"auto\" style:use-optimal-row-height=\"false\"/></style:style>")); }
        map.insert(key, n.clone());
        n
    };
    auto.push_str("<style:style style:name=\"ta1\" style:family=\"table\" style:master-page-name=\"Default\"><style:table-properties table:display=\"true\" style:writing-mode=\"lr-tb\"/></style:style>");
    auto.push_str("<style:style style:name=\"ta2\" style:family=\"table\" style:master-page-name=\"Default\"><style:table-properties table:display=\"false\" style:writing-mode=\"lr-tb\"/></style:style>");
    let mut body = String::new();
    for (si, s) in wb.sheets.iter().enumerate() {
        body.push_str(&format!("<table:table table:name=\"{}\" table:style-name=\"{}\">", attr(&s.name), if s.hidden { "ta2" } else { "ta1" }));
        let ext = s.extent().unwrap_or_default();
        let last_col = ext.col.max(s.col_widths.keys().next_back().copied().unwrap_or(0)).max(s.merges.iter().map(|m| m.c1).max().unwrap_or(0));
        let mut c = 0;
        while c <= last_col {
            let w = s.col_width(c).max(1.0);
            let hidden = s.hidden_cols.contains(&c);
            let mut n = 1;
            while c + n <= last_col && (s.col_width(c + n).max(1.0) - w).abs() < 0.01 && s.hidden_cols.contains(&(c + n)) == hidden { n += 1; }
            let width = if hidden { s.col_widths.get(&c).copied().unwrap_or(s.default_col_width) } else { w };
            let sn = sizes(&mut col_styles, "co", width, &mut auto, "table-column");
            body.push_str(&format!("<table:table-column table:style-name=\"{sn}\"{}{} table:default-cell-style-name=\"Default\"/>", if n > 1 { format!(" table:number-columns-repeated=\"{n}\"") } else { String::new() }, if hidden { " table:visibility=\"collapse\"" } else { "" }));
            c += n;
        }
        let covered: std::collections::HashSet<Pos> = s.merges.iter().flat_map(|m| m.iter().filter(move |p| *p != m.start())).collect();
        let last_row = ext.row.max(s.merges.iter().map(|m| m.r1).max().unwrap_or(0)).max(s.hidden_rows.iter().next_back().copied().unwrap_or(0).min(100_000));
        for r in 0..=last_row {
            let h = s.row_heights.get(&r).copied().unwrap_or(s.default_row_height);
            let sn = sizes(&mut row_styles, "ro", h, &mut auto, "table-row");
            body.push_str(&format!("<table:table-row table:style-name=\"{sn}\"{}>", if s.hidden_rows.contains(&r) { " table:visibility=\"collapse\"" } else { "" }));
            let mut blank = 0u32;
            let flush = |body: &mut String, blank: &mut u32| { if *blank > 0 { body.push_str(&if *blank > 1 { format!("<table:table-cell table:number-columns-repeated=\"{blank}\"/>") } else { "<table:table-cell/>".into() }); *blank = 0; } };
            for c in 0..=last_col {
                let p = Pos::new(r, c);
                let cell = s.cells.get(&p);
                let v = s.value(p);
                let note = s.notes.get(&p);
                if covered.contains(&p) { flush(&mut body, &mut blank); body.push_str("<table:covered-table-cell/>"); continue; }
                if cell.is_none() && v.is_empty() && note.is_none() && s.merge_at(p).is_none() { blank += 1; continue; }
                flush(&mut body, &mut blank);
                let style = cell.map(|c| c.style).unwrap_or(0);
                body.push_str("<table:table-cell");
                if let Some(n) = used.get(&style).filter(|_| style != 0) { body.push_str(&format!(" table:style-name=\"{n}\"")); }
                if let Some(f) = cell.and_then(|c| c.formula()) { body.push_str(&format!(" table:formula=\"{}\"", attr(&to_of(f)))); }
                if let Some(m) = s.merges.iter().find(|m| m.start() == p) { body.push_str(&format!(" table:number-columns-spanned=\"{}\" table:number-rows-spanned=\"{}\"", m.cols(), m.rows())); }
                let shown = wb.display(si, p);
                match &v {
                    Value::Num(n) => {
                        match data_kind.get(&style).copied() {
                            Some("date") => body.push_str(&format!(" office:value-type=\"date\" office:date-value=\"{}\"", iso_date(*n, wb.date1904))),
                            Some("percentage") => body.push_str(&format!(" office:value-type=\"percentage\" office:value=\"{n}\"")),
                            _ => body.push_str(&format!(" office:value-type=\"float\" office:value=\"{n}\"")),
                        }
                    }
                    Value::Bool(b) => body.push_str(&format!(" office:value-type=\"boolean\" office:boolean-value=\"{b}\"")),
                    Value::Str(_) => body.push_str(" office:value-type=\"string\""),
                    Value::Err(_) => body.push_str(" office:value-type=\"string\" office:string-value=\"\""),
                    _ => {}
                }
                body.push('>');
                if let Some(n) = note { body.push_str("<office:annotation>"); for l in n.lines() { body.push_str(&format!("<text:p>{}</text:p>", text(l))); } body.push_str("</office:annotation>"); }
                if !v.is_empty() { for l in shown.split('\n') { body.push_str(&format!("<text:p>{}</text:p>", text(l))); } }
                body.push_str("</table:table-cell>");
            }
            body.push_str("</table:table-row>");
        }
        body.push_str("</table:table>");
    }
    let ns = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\" xmlns:of=\"urn:oasis:names:tc:opendocument:xmlns:of:1.2\" xmlns:config=\"urn:oasis:names:tc:opendocument:xmlns:config:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" office:version=\"1.3\"";
    let decl = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
    let def = wb.styles.get(0);
    let fonts = format!("<office:font-face-decls><style:font-face style:name=\"{0}\" svg:font-family=\"'{0}'\"/></office:font-face-decls>", attr(&def.font));
    let content = format!("{decl}<office:document-content {ns}>{fonts}<office:automatic-styles>{auto}</office:automatic-styles><office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>");
    let styles = format!("{decl}<office:document-styles {ns}>{fonts}<office:styles><style:default-style style:family=\"table-cell\"><style:text-properties style:font-name=\"{0}\" fo:font-size=\"{1}pt\"/></style:default-style><style:style style:name=\"Default\" style:family=\"table-cell\"/></office:styles><office:automatic-styles><style:page-layout style:name=\"pm1\"/></office:automatic-styles><office:master-styles><style:master-page style:name=\"Default\" style:page-layout-name=\"pm1\"/></office:master-styles></office:document-styles>", attr(&def.font), def.size as f32 / 10.0);
    let settings = format!("{decl}<office:document-settings {ns}><office:settings><config:config-item-set config:name=\"ooo:view-settings\"><config:config-item-map-indexed config:name=\"Views\"><config:config-item-map-entry><config:config-item config:name=\"ViewId\" config:type=\"string\">view1</config:config-item><config:config-item config:name=\"ActiveTable\" config:type=\"string\">{}</config:config-item></config:config-item-map-entry></config:config-item-map-indexed></config:config-item-set></office:settings></office:document-settings>", text(&wb.sheets[wb.active].name));
    let meta = format!("{decl}<office:document-meta {ns}><office:meta><meta:generator>Jadwal</meta:generator></office:meta></office:document-meta>");
    let manifest = format!("{decl}<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\"><manifest:file-entry manifest:full-path=\"/\" manifest:version=\"1.3\" manifest:media-type=\"{MIME}\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"settings.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/></manifest:manifest>");
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        // The mimetype comes first, uncompressed (ODF 1.3 §3.3).
        zip.start_file("mimetype", stored).map_err(|e| e.to_string())?;
        zip.write_all(MIME.as_bytes()).map_err(|e| e.to_string())?;
        for (n, x) in [("content.xml", &content), ("styles.xml", &styles), ("settings.xml", &settings), ("meta.xml", &meta), ("META-INF/manifest.xml", &manifest)] {
            zip.start_file(n, deflate).map_err(|e| e.to_string())?;
            zip.write_all(x.as_bytes()).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_convert() {
        assert_eq!(from_of("of:=SUM([.A1:.B2];[$Sheet2.C3])"), "SUM(A1:B2,Sheet2!C3)");
        assert_eq!(from_of("of:=[$'My Sheet'.A1:.A3]"), "'My Sheet'!A1:A3");
        assert_eq!(to_of("SUM(A1:B2,Sheet2!C3)"), "of:=SUM([.A1:.B2];[$Sheet2.C3])");
        assert_eq!(from_of(&to_of("IF(A1>0,\"a;b\",'My Sheet'!$B$2)")), "IF(A1>0,\"a;b\",'My Sheet'!$B$2)");
    }

    #[test]
    fn ods_round_trip() {
        let mut wb = Workbook::new();
        wb.add_sheet(1, Some("Data".into()));
        wb.set_input(0, Pos::new(0, 0), "Name");
        wb.set_input(0, Pos::new(1, 0), "12.5");
        wb.set_input(0, Pos::new(2, 0), "=A2*2");
        wb.set_input(0, Pos::new(3, 0), "=Data!B2+1");
        wb.set_input(0, Pos::new(4, 0), "TRUE");
        wb.set_input(0, Pos::new(5, 0), "2024-03-15");
        wb.set_input(0, Pos::new(6, 0), "25%");
        wb.set_input(1, Pos::new(1, 1), "41");
        wb.apply_style(0, &[Range::cell(Pos::new(0, 0))], &|s| { s.bold = true; s.fill = Some([255, 255, 0]); s.h = HAlign::Center; });
        wb.merge(0, Range { r0: 0, c0: 2, r1: 1, c1: 3 }, false);
        wb.sheets[0].col_widths.insert(0, 120.0);
        wb.sheets[0].hidden_rows.insert(8);
        wb.sheets[0].notes.insert(Pos::new(1, 0), "a note".into());
        wb.sheets[1].hidden = true;
        wb.recalc();
        let back = read(&write(&wb).unwrap()).unwrap();
        assert_eq!(back.sheets.len(), 2);
        assert_eq!(back.sheets[1].name, "Data");
        assert!(back.sheets[1].hidden);
        let s = &back.sheets[0];
        assert_eq!(s.value(Pos::new(0, 0)), Value::str("Name"));
        assert_eq!(s.value(Pos::new(2, 0)), Value::Num(25.0));
        assert_eq!(back.edit_text(0, Pos::new(3, 0)), "=Data!B2+1");
        assert_eq!(s.value(Pos::new(3, 0)), Value::Num(42.0));
        assert_eq!(s.value(Pos::new(4, 0)), Value::Bool(true));
        assert_eq!(back.display(0, Pos::new(5, 0)), wb.display(0, Pos::new(5, 0)));
        assert_eq!(s.value(Pos::new(6, 0)), Value::Num(0.25));
        let st = back.style_at(0, Pos::new(0, 0));
        assert!(st.bold && st.fill == Some([255, 255, 0]) && st.h == HAlign::Center);
        assert_eq!(s.merges, vec![Range { r0: 0, c0: 2, r1: 1, c1: 3 }]);
        assert!((s.col_width(0) - 120.0).abs() <= 1.0);
        assert!(s.hidden_rows.contains(&8));
        assert_eq!(s.notes[&Pos::new(1, 0)], "a note");
    }
}
