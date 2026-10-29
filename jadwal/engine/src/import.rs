//! Getting data in and reshaping it (importdialog.js, get-data.js, dedupedialog.js,
//! advanced-filter.js): the text import wizard's parsing (encodings, delimiters, text qualifier,
//! fixed widths, column data types), HTML tables for From Web, sheets and tables of another
//! workbook, Text to Columns, Remove Duplicates by chosen columns and Advanced Filter.

use crate::addr::{Pos, Range};
use crate::book::{Input, Workbook};
use crate::value::Value;

// ---- text ------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Encoding { #[default] Utf8, Utf16Le, Utf16Be, Latin1, Windows1252 }

impl Encoding {
    pub const ALL: [(Encoding, &'static str); 5] = [(Encoding::Utf8, "65001 : Unicode (UTF-8)"), (Encoding::Utf16Le, "1200 : Unicode"), (Encoding::Utf16Be, "1201 : Unicode (Big-Endian)"), (Encoding::Latin1, "28591 : Western European (ISO)"), (Encoding::Windows1252, "1252 : Western European (Windows)")];
    /// The likely encoding of a file's bytes (BOMs, then UTF-8 validity).
    pub fn detect(b: &[u8]) -> Encoding {
        if b.starts_with(&[0xFF, 0xFE]) { Encoding::Utf16Le } else if b.starts_with(&[0xFE, 0xFF]) { Encoding::Utf16Be } else if std::str::from_utf8(b).is_ok() { Encoding::Utf8 } else { Encoding::Windows1252 }
    }
}

const CP1252: [u16; 32] = [0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178];

/// Decodes bytes in an encoding (BOMs dropped).
pub fn decode(b: &[u8], e: Encoding) -> String {
    match e {
        Encoding::Utf8 => String::from_utf8_lossy(b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b)).into_owned(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let b = b.strip_prefix(if e == Encoding::Utf16Le { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] }).unwrap_or(b);
            let u: Vec<u16> = b.chunks_exact(2).map(|c| if e == Encoding::Utf16Le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
            String::from_utf16_lossy(&u)
        }
        Encoding::Latin1 => b.iter().map(|c| *c as char).collect(),
        Encoding::Windows1252 => b.iter().map(|&c| if (0x80..0xA0).contains(&c) { char::from_u32(CP1252[(c - 0x80) as usize] as u32).unwrap_or('?') } else { c as char }).collect(),
    }
}

/// What a column becomes: General (numbers and dates recognized), Text, a date in an order, or
/// skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColType { #[default] General, Text, Date(DateOrder), Skip }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrder { Mdy, Dmy, Ymd, Myd, Dym, Ydm }

impl DateOrder {
    pub const ALL: [(DateOrder, &'static str); 6] = [(DateOrder::Mdy, "MDY"), (DateOrder::Dmy, "DMY"), (DateOrder::Ymd, "YMD"), (DateOrder::Myd, "MYD"), (DateOrder::Dym, "DYM"), (DateOrder::Ydm, "YDM")];
}

/// The wizard's choices.
#[derive(Clone, Debug, PartialEq)]
pub struct TextOptions {
    pub encoding: Encoding,
    /// Delimited (tab, semicolon, comma, space, other) or fixed width (break positions).
    pub fixed: Option<Vec<usize>>,
    pub tab: bool,
    pub semicolon: bool,
    pub comma: bool,
    pub space: bool,
    pub other: Option<char>,
    pub consecutive: bool,
    pub qualifier: Option<char>,
    /// Rows to skip at the start ("Start import at row").
    pub start_row: usize,
    pub header: bool,
    pub types: Vec<ColType>,
    pub decimal: char,
    pub thousands: char,
}

impl Default for TextOptions {
    fn default() -> Self { TextOptions { encoding: Encoding::Utf8, fixed: None, tab: true, semicolon: false, comma: false, space: false, other: None, consecutive: false, qualifier: Some('"'), start_row: 0, header: true, types: vec![], decimal: '.', thousands: ',' } }
}

impl TextOptions {
    /// Options guessed from the text: the delimiter seen most consistently.
    pub fn detect(text: &str) -> TextOptions {
        let d = crate::csv::detect(text, "");
        TextOptions { tab: d == '\t', comma: d == ',', semicolon: d == ';', space: false, other: (![',', '\t', ';'].contains(&d)).then_some(d), ..Default::default() }
    }
    fn delims(&self) -> Vec<char> {
        let mut v = vec![];
        if self.tab { v.push('\t'); } if self.semicolon { v.push(';'); } if self.comma { v.push(','); } if self.space { v.push(' '); }
        if let Some(c) = self.other { v.push(c); }
        v
    }
}

/// Splits text into rows of fields by the wizard's options (before data types).
pub fn split(text: &str, o: &TextOptions) -> Vec<Vec<String>> {
    let lines = text.split_inclusive('\n');
    if let Some(breaks) = &o.fixed {
        return lines.skip(o.start_row).map(|l| l.trim_end_matches(['\r', '\n'])).filter(|l| !l.is_empty()).map(|l| fixed_fields(l, breaks)).collect();
    }
    let delims = o.delims();
    let mut rows = vec![];
    let mut row: Vec<String> = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut in_q = false;
    let mut chars = text.chars().peekable();
    let mut line = 0usize;
    let mut last_delim = false;
    let push_row = |rows: &mut Vec<Vec<String>>, row: &mut Vec<String>, line: usize| { if line >= o.start_row && !(row.len() == 1 && row[0].is_empty()) { rows.push(std::mem::take(row)); } else { row.clear(); } };
    while let Some(c) = chars.next() {
        if in_q {
            if Some(c) == o.qualifier { if chars.peek() == Some(&c) { field.push(c); chars.next(); } else { in_q = false; } } else { field.push(c); }
            continue;
        }
        if Some(c) == o.qualifier && field.is_empty() && !quoted { in_q = true; quoted = true; last_delim = false; continue; }
        if delims.contains(&c) {
            if o.consecutive && last_delim { continue; }
            row.push(std::mem::take(&mut field)); quoted = false; last_delim = true;
            continue;
        }
        if c == '\r' { continue; }
        if c == '\n' {
            row.push(std::mem::take(&mut field)); quoted = false; last_delim = false;
            push_row(&mut rows, &mut row, line);
            line += 1;
            continue;
        }
        last_delim = false;
        field.push(c);
    }
    if !field.is_empty() || !row.is_empty() { row.push(field); push_row(&mut rows, &mut row, line); }
    rows
}

fn fixed_fields(l: &str, breaks: &[usize]) -> Vec<String> {
    let chars: Vec<char> = l.chars().collect();
    let mut out = vec![];
    let mut at = 0;
    for &b in breaks.iter().chain(std::iter::once(&usize::MAX)) {
        let end = b.min(chars.len());
        if at >= chars.len() && b != usize::MAX { out.push(String::new()); continue; }
        out.push(chars[at.min(chars.len())..end.max(at.min(chars.len()))].iter().collect::<String>().trim().to_string());
        at = end;
    }
    out
}

/// Suggested break positions for fixed-width text: columns that are blank on every line.
pub fn guess_breaks(text: &str) -> Vec<usize> {
    let lines: Vec<Vec<char>> = text.lines().take(50).filter(|l| !l.trim().is_empty()).map(|l| l.chars().collect()).collect();
    let w = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let blank = |i: usize| lines.iter().all(|l| l.get(i).is_none_or(|c| *c == ' '));
    (1..w).filter(|&i| blank(i - 1) && !blank(i)).collect()
}

/// One field as a value by its column type.
pub fn convert(wb: &Workbook, t: &str, ty: ColType, o: &TextOptions) -> Option<(Input, Option<String>)> {
    match ty {
        ColType::Skip => None,
        ColType::Text => Some((if t.is_empty() { Input::Empty } else { Input::Value(Value::str(t)) }, Some("@".into()))),
        ColType::Date(ord) => Some(match parse_date(t, ord) { Some(n) => (Input::Value(Value::Num(n)), Some("m/d/yyyy".into())), None => (if t.is_empty() { Input::Empty } else { Input::Value(Value::str(t)) }, None) }),
        ColType::General => {
            if t.starts_with('=') { return Some((Input::Value(Value::str(t)), None)); }
            let t2 = if o.decimal != '.' || o.thousands != ',' { t.replace(o.thousands, "\u{1}").replace(o.decimal, ".").replace('\u{1}', ",") } else { t.to_string() };
            Some(wb.parse_input(&t2))
        }
    }
}

fn parse_date(t: &str, ord: DateOrder) -> Option<f64> {
    let parts: Vec<i64> = t.split(['/', '-', '.', ' ']).filter(|p| !p.is_empty()).map(|p| p.parse().ok()).collect::<Option<_>>()?;
    if parts.len() != 3 { return None; }
    let (y, m, d) = match ord { DateOrder::Mdy => (parts[2], parts[0], parts[1]), DateOrder::Dmy => (parts[2], parts[1], parts[0]), DateOrder::Ymd => (parts[0], parts[1], parts[2]), DateOrder::Myd => (parts[1], parts[0], parts[2]), DateOrder::Dym => (parts[1], parts[2], parts[0]), DateOrder::Ydm => (parts[0], parts[2], parts[1]) };
    let y = if y < 30 { 2000 + y } else if y < 100 { 1900 + y } else { y };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    Some(crate::date::serial(y, m, d, false))
}

/// Puts rows of fields on a sheet from `at` with the column types; returns the area written.
pub fn place(wb: &mut Workbook, sheet: usize, at: Pos, rows: &[Vec<String>], o: &TextOptions) -> Range {
    let mut end = at;
    for (i, row) in rows.iter().enumerate() {
        let mut c = 0u32;
        for (j, t) in row.iter().enumerate() {
            let ty = o.types.get(j).copied().unwrap_or_default();
            let Some((input, fmt)) = convert(wb, t, ty, o) else { continue };
            let p = Pos::new(at.row + i as u32, at.col + c);
            wb.set_cell_input(sheet, p, input, fmt);
            end = Pos::new(end.row.max(p.row), end.col.max(p.col));
            c += 1;
        }
    }
    Range::new(at, end)
}

/// Data ▸ Text to Columns over a one-column selection: each cell's text split into the cells to
/// its right (the source column takes the first field).
pub fn text_to_columns(wb: &mut Workbook, sheet: usize, r: Range, o: &TextOptions, dest: Option<Pos>) -> usize {
    let dest = dest.unwrap_or(r.start());
    let mut n = 0;
    for row in r.r0..=r.r1 {
        let t = match wb.sheets[sheet].value(Pos::new(row, r.c0)) { Value::Str(s) => s.to_string(), Value::Empty => continue, v => v.display() };
        let fields = split(&t, &TextOptions { start_row: 0, ..o.clone() }).into_iter().next().unwrap_or_default();
        let mut c = 0;
        for (j, f) in fields.iter().enumerate() {
            let ty = o.types.get(j).copied().unwrap_or_default();
            let Some((input, fmt)) = convert(wb, f, ty, o) else { continue };
            wb.set_cell_input(sheet, Pos::new(dest.row + row - r.r0, dest.col + c), input, fmt);
            c += 1;
        }
        n += 1;
    }
    n
}

// ---- HTML ------------------------------------------------------------------------------------

/// A table found on a web page: its caption (or "Table n") and its rows of cell texts.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlTable { pub name: String, pub rows: Vec<Vec<String>> }

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() { match c { '<' => in_tag = true, '>' => { in_tag = false; out.push(' '); } _ if !in_tag => out.push(c), _ => {} } }
    let t = crate::xlsx::xml::unescape(&out.replace("&nbsp;", " "));
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The `<table>`s of an HTML page (nested tables are read on their own).
pub fn html_tables(html: &str) -> Vec<HtmlTable> {
    let low = html.to_ascii_lowercase();
    let mut out = vec![];
    let mut from = 0;
    while let Some(s) = low[from..].find("<table").map(|i| i + from) {
        // The matching end, counting nested tables.
        let mut depth = 0; let mut i = s; let mut end = low.len();
        while i < low.len() {
            let nt = low[i..].find("<table").map(|k| k + i); let ne = low[i..].find("</table").map(|k| k + i);
            match (nt, ne) {
                (Some(a), Some(b)) if a < b => { depth += 1; i = a + 6; }
                (_, Some(b)) => { depth -= 1; i = b + 7; if depth == 0 { end = b; break; } }
                _ => break,
            }
        }
        let body = &html[s..end];
        let lb = &low[s..end];
        let name = lb.find("<caption").and_then(|a| lb[a..].find("</caption").map(|b| strip_tags(&body[a..a + b]))).filter(|t| !t.is_empty()).unwrap_or_else(|| format!("Table {}", out.len() + 1));
        let mut rows = vec![];
        let mut k = 0;
        while let Some(a) = lb[k..].find("<tr").map(|x| x + k) {
            let b = lb[a + 3..].find("</tr").map(|x| x + a + 3).or_else(|| lb[a + 3..].find("<tr").map(|x| x + a + 3)).unwrap_or(lb.len());
            let rl = &lb[a..b]; let rb = &body[a..b];
            let mut cells = vec![];
            let mut q = 0;
            while let Some(c) = rl[q..].find("<t").map(|x| x + q) {
                let tag = rl.get(c + 2..c + 3).unwrap_or("");
                if tag != "d" && tag != "h" { q = c + 2; continue; }
                let open_end = rl[c..].find('>').map(|x| x + c + 1).unwrap_or(rl.len());
                let close = rl[open_end..].find("</t").map(|x| x + open_end).or_else(|| rl[open_end..].find("<t").map(|x| x + open_end)).unwrap_or(rl.len());
                let span = rl[c..open_end].split("colspan=").nth(1).and_then(|t| t.trim_start_matches(['"', '\'']).split(|ch: char| !ch.is_ascii_digit()).next()?.parse::<usize>().ok()).unwrap_or(1);
                cells.push(strip_tags(&rb[open_end..close]));
                for _ in 1..span.min(50) { cells.push(String::new()); }
                q = close.max(c + 2);
            }
            if !cells.is_empty() { rows.push(cells); }
            k = b.max(a + 3);
        }
        if !rows.is_empty() { out.push(HtmlTable { name, rows }); }
        from = s + 6;
    }
    out
}

// ---- another workbook --------------------------------------------------------------------------

/// What another workbook offers: (name, is a table, rows of values).
pub fn workbook_items(wb: &Workbook) -> Vec<(String, bool, Vec<Vec<Value>>)> {
    let mut v = vec![];
    for (si, s) in wb.sheets.iter().enumerate() {
        for t in &s.tables { v.push((t.name.clone(), true, crate::consolidate::source(wb, si, t.range))); }
    }
    for (si, s) in wb.sheets.iter().enumerate() {
        let Some(e) = s.extent() else { continue };
        v.push((s.name.clone(), false, crate::consolidate::source(wb, si, Range::new(Pos::new(0, 0), e))));
    }
    v
}

// ---- Remove Duplicates and Advanced Filter ----------------------------------------------------

/// Remove Duplicates: deletes the rows of `r` (after the header with `header`) whose values in
/// `cols` (absolute columns) repeat an earlier row; the rows below move up inside the range.
/// Returns (removed, remaining).
pub fn remove_duplicates(wb: &mut Workbook, sheet: usize, r: Range, cols: &[u32], header: bool) -> (usize, usize) {
    let start = if header { r.r0 + 1 } else { r.r0 };
    if start > r.r1 || cols.is_empty() { return (0, 0); }
    let mut seen = std::collections::HashSet::new();
    let mut keep = vec![];
    for row in start..=r.r1 {
        let key: Vec<String> = cols.iter().map(|c| wb.sheets[sheet].value(Pos::new(row, *c)).display().to_lowercase()).collect();
        if seen.insert(key) { keep.push(row); }
    }
    let removed = (r.r1 - start + 1) as usize - keep.len();
    if removed == 0 { return (0, keep.len()); }
    let clips: Vec<crate::book::Clip> = keep.iter().map(|row| wb.copy(sheet, Range { r0: *row, r1: *row, ..r })).collect();
    wb.clear(sheet, Range { r0: start, ..r }, true, true, false);
    for (i, c) in clips.iter().enumerate() { wb.paste(sheet, Range::cell(Pos::new(start + i as u32, r.c0)), c, Default::default()); }
    (removed, keep.len())
}

fn crit_match(v: &Value, c: &str) -> bool {
    let c = c.trim();
    if c.is_empty() { return true; }
    let (op, rest) = ["<>", ">=", "<=", "=", ">", "<"].iter().find_map(|o| c.strip_prefix(o).map(|r| (*o, r.trim()))).unwrap_or(("", c));
    let num = rest.parse::<f64>().ok();
    let vn = v.to_num().ok().filter(|_| !matches!(v, Value::Str(_) | Value::Empty));
    let vs = v.display();
    let cmp = |o: std::cmp::Ordering| match op { ">" => o.is_gt(), "<" => o.is_lt(), ">=" => o.is_ge(), "<=" => o.is_le(), "<>" => o.is_ne(), _ => o.is_eq() };
    match (num, vn) {
        (Some(n), Some(x)) => cmp(x.partial_cmp(&n).unwrap_or(std::cmp::Ordering::Less)),
        _ if op.is_empty() => vs.to_lowercase().starts_with(&rest.to_lowercase()) || crate::tables::wildcard(&format!("{}*", rest.to_lowercase()), &vs.to_lowercase()),
        _ if op == "=" && rest.is_empty() => vs.is_empty(),
        _ if op == "<>" && rest.is_empty() => !vs.is_empty(),
        _ if op == "=" || op == "<>" => { let m = crate::tables::wildcard(&rest.to_lowercase(), &vs.to_lowercase()); if op == "=" { m } else { !m } }
        _ => cmp(vs.to_lowercase().cmp(&rest.to_lowercase())),
    }
}

/// Advanced Filter: the data rows of `list` (header first) matching the criteria range (a header
/// row of column names, then rows of conditions: AND across a row, OR between rows), optionally
/// unique. Returns the matching rows (absolute).
pub fn advanced_rows(wb: &Workbook, sheet: usize, list: Range, criteria: Option<(usize, Range)>, unique: bool) -> Vec<u32> {
    let s = &wb.sheets[sheet];
    let headers: Vec<String> = (list.c0..=list.c1).map(|c| s.value(Pos::new(list.r0, c)).display().to_lowercase()).collect();
    let conds: Vec<Vec<(u32, String)>> = criteria.map(|(cs, cr)| {
        let cs = &wb.sheets[cs];
        (cr.r0 + 1..=cr.r1).map(|row| (cr.c0..=cr.c1).filter_map(|c| {
            let h = cs.value(Pos::new(cr.r0, c)).display().to_lowercase();
            let col = headers.iter().position(|x| *x == h)? as u32 + list.c0;
            let v = cs.value(Pos::new(row, c));
            let t = match &v { Value::Empty => return None, Value::Num(n) => format!("={}", crate::value::fmt_num(*n)), v => v.display() };
            Some((col, t))
        }).collect()).filter(|r: &Vec<(u32, String)>| !r.is_empty()).collect()
    }).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    (list.r0 + 1..=list.r1).filter(|&row| {
        let ok = conds.is_empty() || conds.iter().any(|and| and.iter().all(|(c, t)| crit_match(&s.value(Pos::new(row, *c)), t)));
        ok && (!unique || seen.insert((list.c0..=list.c1).map(|c| s.value(Pos::new(row, c)).display().to_lowercase()).collect::<Vec<_>>()))
    }).collect()
}

/// Filter the list in place: hides the rows that don't match (kept as filtered rows so Clear
/// shows them again).
pub fn advanced_in_place(wb: &mut Workbook, sheet: usize, list: Range, criteria: Option<(usize, Range)>, unique: bool) -> usize {
    let keep: std::collections::BTreeSet<u32> = advanced_rows(wb, sheet, list, criteria, unique).into_iter().collect();
    let s = &mut wb.sheets[sheet];
    for row in list.r0 + 1..=list.r1 { if !keep.contains(&row) { s.hidden_rows.insert(row); s.filtered_rows.insert(row); } else if s.filtered_rows.remove(&row) { s.hidden_rows.remove(&row); } }
    keep.len()
}

/// Copy to another location: the header and the matching rows from `to`.
pub fn advanced_copy(wb: &mut Workbook, sheet: usize, list: Range, criteria: Option<(usize, Range)>, unique: bool, to_sheet: usize, to: Pos) -> Range {
    let rows = advanced_rows(wb, sheet, list, criteria, unique);
    let mut out = vec![wb.copy(sheet, Range { r1: list.r0, ..list })];
    out.extend(rows.iter().map(|r| wb.copy(sheet, Range { r0: *r, r1: *r, ..list })));
    for (i, c) in out.iter().enumerate() { wb.paste(to_sheet, Range::cell(Pos::new(to.row + i as u32, to.col)), c, Default::default()); }
    Range { r0: to.row, c0: to.col, r1: to.row + rows.len() as u32, c1: to.col + list.c1 - list.c0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::{parse_a1, parse_range};

    #[test]
    fn delimited_fixed_and_types() {
        let o = TextOptions { comma: true, tab: false, ..Default::default() };
        let rows = split("a,\"b,c\",\"say \"\"hi\"\"\"\r\n1,2,3\n\n", &o);
        assert_eq!(rows, vec![vec!["a", "b,c", "say \"hi\""], vec!["1", "2", "3"]]);
        let o2 = TextOptions { space: true, tab: false, consecutive: true, start_row: 1, ..Default::default() };
        assert_eq!(split("skip me\nx   y  z\n", &o2), vec![vec!["x", "y", "z"]]);
        assert_eq!(TextOptions::detect("a;b;c\n1;2;3").semicolon, true);
        let f = TextOptions { fixed: Some(vec![3, 7]), ..Default::default() };
        assert_eq!(split("abcdefghij\nxy\n", &f), vec![vec!["abc", "defg", "hij"], vec!["xy", "", ""]]);
        assert_eq!(guess_breaks("ab  cd\nef  gh\n"), vec![4]);
        assert_eq!(decode(&[0xFF, 0xFE, b'h', 0, b'i', 0], Encoding::Utf16Le), "hi");
        assert_eq!(decode(&[0x80, b'5'], Encoding::Windows1252), "€5");
        assert_eq!(Encoding::detect(&[0xE9]), Encoding::Windows1252);
        let mut wb = Workbook::new();
        let o = TextOptions { comma: true, tab: false, types: vec![ColType::Text, ColType::General, ColType::Date(DateOrder::Dmy), ColType::Skip], ..Default::default() };
        let r = place(&mut wb, 0, Pos::new(0, 0), &split("007,1.5,31/12/2020,zzz", &o), &o);
        assert_eq!(r, parse_range("A1:C1").unwrap());
        assert_eq!(wb.display(0, parse_a1("A1").unwrap()), "007");
        assert_eq!(wb.sheets[0].value(parse_a1("B1").unwrap()), Value::Num(1.5));
        assert_eq!(wb.display(0, parse_a1("C1").unwrap()), "12/31/2020");
        assert!(wb.display(0, parse_a1("D1").unwrap()).is_empty());
    }

    #[test]
    fn text_to_columns_and_html() {
        let mut wb = Workbook::new();
        wb.set_input(0, parse_a1("A1").unwrap(), "Smith|John|42");
        let o = TextOptions { tab: false, other: Some('|'), ..Default::default() };
        text_to_columns(&mut wb, 0, parse_range("A1:A1").unwrap(), &o, None);
        assert_eq!(wb.display(0, parse_a1("B1").unwrap()), "John");
        assert_eq!(wb.sheets[0].value(parse_a1("C1").unwrap()), Value::Num(42.0));
        let t = html_tables("<p>x</p><table><caption>Pop</caption><tr><th>City</th><th>N</th></tr><tr><td><a href=x>Oslo</a></td><td>1&amp;2</td></tr></table><table><tr><td colspan=2>a</td><td>b</td></tr></table>");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].name, "Pop");
        assert_eq!(t[0].rows, vec![vec!["City", "N"], vec!["Oslo", "1&2"]]);
        assert_eq!(t[1].rows[0], vec!["a", "", "b"]);
    }

    #[test]
    fn dedupe_and_advanced_filter() {
        let mut wb = Workbook::new();
        for (r, row) in [["Name", "City", "N"], ["a", "X", "1"], ["b", "X", "2"], ["a", "X", "3"], ["c", "Y", "5"]].iter().enumerate() {
            for (c, t) in row.iter().enumerate() { wb.set_input(0, Pos::new(r as u32, c as u32), t); }
        }
        wb.recalc();
        let list = parse_range("A1:C5").unwrap();
        // Criteria: City = X and N > 1, or Name = c.
        for (p, t) in [("E1", "City"), ("F1", "N"), ("G1", "Name"), ("E2", "X"), ("F2", ">1"), ("G3", "c")] { wb.set_input(0, parse_a1(p).unwrap(), t); }
        wb.recalc();
        assert_eq!(advanced_rows(&wb, 0, list, Some((0, parse_range("E1:G3").unwrap())), false), vec![2, 3, 4]);
        advanced_in_place(&mut wb, 0, list, Some((0, parse_range("E1:G2").unwrap())), false);
        assert!(wb.sheets[0].hidden_rows.contains(&1) && !wb.sheets[0].hidden_rows.contains(&2));
        let r = advanced_copy(&mut wb, 0, parse_range("B1:B5").unwrap(), None, true, 0, parse_a1("I1").unwrap());
        assert_eq!(r, parse_range("I1:I3").unwrap());
        assert_eq!(wb.display(0, parse_a1("I3").unwrap()), "Y");
        let (removed, left) = remove_duplicates(&mut wb, 0, list, &[0], true);
        assert_eq!((removed, left), (1, 3));
        assert_eq!(wb.display(0, parse_a1("A4").unwrap()), "c");
    }
}
