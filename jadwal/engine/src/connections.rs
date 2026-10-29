//! Workbook connections (queries.js, external-workbooks.js, editlinks.js): the data sources Get
//! Data created (a text/CSV file, another workbook's sheet or table, a web page's table, a range
//! of this workbook), stored as Excel stores them in `xl/connections.xml` (textPr, webPr, dbPr),
//! with each one's destination in a sheet-scoped `ExternalData_n` name. Connections Jadwal does
//! not model (ODBC, OLAP …) are kept as they were and listed, but not refreshed.
//!
//! External links (Edit Links) are the workbook's `xl/externalLinks/` parts: listed, changed to
//! another source, or broken (formulas pointing at them replaced by their values).

use crate::addr::{parse_range, quote_sheet, Pos, Range};
use crate::book::{DefinedName, Workbook};
use crate::import::{self, TextOptions};
use crate::value::Value;
use crate::xlsx::xml::{attr, part_text, Element};

pub const PART: &str = "xl/connections.xml";

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A delimited text file and its delimiter.
    Text { path: String, delimiter: char, encoding: import::Encoding },
    /// A sheet or table of another workbook.
    Workbook { path: String, item: String },
    /// A table (index from 1) of a web page.
    Web { url: String, table: usize },
    /// A Power Query query of this workbook (Microsoft.Mashup.OleDb.1, Location = the query's
    /// name; crate::powerquery holds its formula).
    Mashup { query: String },
    /// A connection kept as read (not refreshed here).
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub source: Source,
    pub refresh_on_open: bool,
    /// The original element, for connections not modelled.
    pub raw: Option<String>,
}

impl Connection {
    pub fn kind(&self) -> &'static str {
        match self.source { Source::Text { .. } => "Text", Source::Workbook { .. } => "Workbook", Source::Web { .. } => "Web", Source::Mashup { .. } => "Query", Source::Other => "Connection" }
    }
    pub fn location(&self) -> String {
        match &self.source { Source::Text { path, .. } | Source::Workbook { path, .. } => path.clone(), Source::Web { url, .. } => url.clone(), Source::Mashup { query } => query.clone(), Source::Other => String::new() }
    }
}

/// The workbook's connections.
pub fn list(wb: &Workbook) -> Vec<Connection> {
    let Some(b) = wb.parts.get(PART) else { return vec![] };
    let Ok(root) = Element::parse(&part_text(b)) else { return vec![] };
    root.all("connection").map(|e| {
        let id = e.attr_u32("id").unwrap_or(0);
        let name = e.attr("name").unwrap_or("Connection").to_string();
        let description = e.attr("description").unwrap_or("").to_string();
        let source = if let Some(t) = e.child("textPr") {
            let delimiter = if t.flag("comma", false) { ',' } else if t.flag("semicolon", false) { ';' } else if t.flag("space", false) { ' ' } else if let Some(c) = t.attr("delimiter").and_then(|d| d.chars().next()) { c } else { '\t' };
            let encoding = match t.attr_u32("codePage") { Some(1200) => import::Encoding::Utf16Le, Some(1201) => import::Encoding::Utf16Be, Some(28591) => import::Encoding::Latin1, Some(1252) | Some(437) => import::Encoding::Windows1252, _ => import::Encoding::Utf8 };
            Source::Text { path: t.attr("sourceFile").unwrap_or("").to_string(), delimiter, encoding }
        } else if let Some(w) = e.child("webPr") {
            let table = w.child("tables").and_then(|t| t.elements().next()).and_then(|x| x.attr_u32("v")).unwrap_or(1) as usize;
            Source::Web { url: w.attr("url").unwrap_or("").to_string(), table }
        } else if let Some(d) = e.child("dbPr") {
            let conn = d.attr("connection").unwrap_or("");
            let field = |k: &str| conn.split(';').find_map(|p| p.trim().strip_prefix(k).map(|v| v.trim_start_matches('=').to_string()));
            match (field("Data Source"), field("Location")) {
                (_, Some(loc)) if conn.contains("Microsoft.Mashup.OleDb") => Source::Mashup { query: loc.trim_matches('"').to_string() },
                (Some(ds), _) if conn.contains("Jadwal") || conn.contains("ACE.OLEDB") => Source::Workbook { path: ds, item: d.attr("command").unwrap_or("").trim_end_matches('$').to_string() },
                _ => Source::Other,
            }
        } else { Source::Other };
        let raw = matches!(source, Source::Other).then(|| e.to_xml());
        Connection { id, name, description, source, refresh_on_open: e.flag("refreshOnLoad", false), raw }
    }).collect()
}

fn element(c: &Connection) -> String {
    if let Some(r) = &c.raw { return r.clone(); }
    let mut x = format!("<connection id=\"{}\" name=\"{}\"", c.id, attr(&c.name));
    if !c.description.is_empty() { x.push_str(&format!(" description=\"{}\"", attr(&c.description))); }
    let ty = match c.source { Source::Text { .. } => 6, Source::Web { .. } => 4, _ => 5 };
    if matches!(c.source, Source::Mashup { .. }) { x.push_str(" keepAlive=\"1\""); }
    x.push_str(&format!(" type=\"{ty}\""));
    if c.refresh_on_open { x.push_str(" refreshOnLoad=\"1\""); }
    x.push_str(" refreshedVersion=\"8\" background=\"1\" saveData=\"1\">");
    match &c.source {
        Source::Text { path, delimiter, encoding } => {
            let cp = match encoding { import::Encoding::Utf8 => 65001, import::Encoding::Utf16Le => 1200, import::Encoding::Utf16Be => 1201, import::Encoding::Latin1 => 28591, import::Encoding::Windows1252 => 1252 };
            let flags = match delimiter { ',' => " tab=\"0\" comma=\"1\"".to_string(), ';' => " tab=\"0\" semicolon=\"1\"".into(), ' ' => " tab=\"0\" space=\"1\"".into(), '\t' => String::new(), d => format!(" tab=\"0\" delimiter=\"{}\"", attr(&d.to_string())) };
            x.push_str(&format!("<textPr prompt=\"0\" codePage=\"{cp}\" sourceFile=\"{}\" delimited=\"1\"{flags}><textFields count=\"1\"><textField/></textFields></textPr>", attr(path)));
        }
        Source::Web { url, table } => x.push_str(&format!("<webPr sourceData=\"1\" parsePre=\"1\" consecutive=\"1\" xl2000=\"1\" url=\"{}\" htmlTables=\"1\"><tables count=\"1\"><x v=\"{table}\"/></tables></webPr>", attr(url))),
        Source::Workbook { path, item } => x.push_str(&format!("<dbPr connection=\"Provider=Microsoft.ACE.OLEDB.12.0;Data Source={};Extended Properties=Jadwal\" command=\"{}$\" commandType=\"3\"/>", attr(path), attr(item))),
        Source::Mashup { query } => {
            let loc = if query.chars().all(|c| c.is_alphanumeric() || c == '_') { query.clone() } else { format!("\"{query}\"") };
            x.push_str(&format!("<dbPr connection=\"{}\" command=\"SELECT * FROM [{}]\"/>", attr(&format!("Provider=Microsoft.Mashup.OleDb.1;Data Source=$Workbook$;Location={loc};Extended Properties=\"\"")), attr(query)));
        }
        Source::Other => {}
    }
    x.push_str("</connection>");
    x
}

/// Replaces the workbook's connections.
pub fn save(wb: &mut Workbook, list: &[Connection]) {
    if list.is_empty() { wb.parts.remove(PART); return; }
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<connections xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">");
    for c in list { x.push_str(&element(c)); }
    x.push_str("</connections>");
    wb.parts.insert(PART.into(), x.into_bytes());
}

pub const RT_QUERY_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable";
pub const CT_QUERY_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml";

/// A copy of the workbook with a query table part (xl/queryTables) for each text, web or
/// workbook connection with a destination, related from the destination's sheet: Excel ties a
/// connection to its cells through the query table (named like the ExternalData_n name), not the
/// name alone. Query tables of these connections read from the file are replaced. Connections
/// to this workbook's ranges (Power Query, which needs the DataMashup part) get none.
pub fn stage(wb: &Workbook) -> Option<Workbook> {
    let conns: Vec<(Connection, usize, Range)> = list(wb).into_iter()
        .filter(|c| matches!(c.source, Source::Text { .. } | Source::Web { .. } | Source::Workbook { .. }))
        .filter_map(|c| { let (si, r) = destination(wb, c.id)?; Some((c, si, r)) })
        .filter(|(_, si, r)| !wb.sheets[*si].tables.iter().any(|t| t.range.intersects(r)))
        .collect();
    if conns.is_empty() { return None; }
    let mut out = wb.clone();
    let ids: Vec<u32> = conns.iter().map(|c| c.0.id).collect();
    for s in &mut out.sheets {
        let mut drop = vec![];
        s.extra.retain(|(k, v)| {
            let f: Vec<&str> = v.split('\t').collect();
            if k != "rel" || f.len() < 3 || f[1] != RT_QUERY_TABLE { return true; }
            let ours = wb.parts.get(f[2]).and_then(|b| Element::parse(&part_text(b)).ok()).and_then(|e| e.attr_u32("connectionId")).is_some_and(|id| ids.contains(&id));
            if ours { drop.push(f[2].to_string()); }
            !ours
        });
        for p in drop { out.parts.remove(&p); out.parts.remove(&crate::xlsx::rels_name_of(&p)); }
    }
    for (c, si, r) in conns {
        let path = (1..).map(|i| format!("xl/queryTables/queryTable{i}.xml")).find(|p| !out.parts.contains_key(p)).unwrap();
        let n = (r.c1 - r.c0 + 1) as usize;
        let mut seen = std::collections::HashSet::new();
        let fields: String = (0..n).map(|k| {
            let mut h = wb.display(si, Pos::new(r.r0, r.c0 + k as u32));
            if h.trim().is_empty() { h = format!("Column{}", k + 1); }
            while !seen.insert(h.to_lowercase()) { h.push('_'); }
            format!("<queryTableField id=\"{}\" name=\"{}\"/>", k + 1, attr(&h))
        }).collect();
        let x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<queryTable xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" name=\"{}\" connectionId=\"{}\" autoFormatId=\"16\" applyNumberFormats=\"0\" applyBorderFormats=\"0\" applyFontFormats=\"0\" applyPatternFormats=\"0\" applyAlignmentFormats=\"0\" applyWidthHeightFormats=\"0\"><queryTableRefresh nextId=\"{}\"><queryTableFields count=\"{n}\">{fields}</queryTableFields></queryTableRefresh></queryTable>",
            dest_name(c.id), c.id, n + 1);
        out.parts.insert(path.clone(), x.into_bytes());
        let s = &mut out.sheets[si];
        let used: std::collections::HashSet<String> = s.extra.iter().filter(|(k, _)| k == "rel").filter_map(|(_, v)| v.split('\t').next().map(String::from)).collect();
        let id = (1..).map(|i| format!("rIdQt{i}")).find(|i| !used.contains(i)).unwrap();
        s.extra.push(("rel".into(), format!("{id}\t{RT_QUERY_TABLE}\t{path}\t0")));
    }
    Some(out)
}

fn dest_name(id: u32) -> String { format!("ExternalData_{id}") }

/// Where a connection's data sits: (sheet, range).
pub fn destination(wb: &Workbook, id: u32) -> Option<(usize, Range)> {
    if let Some(Source::Mashup { query }) = list(wb).into_iter().find(|c| c.id == id).map(|c| c.source) { return crate::powerquery::destination(wb, &query); }
    let n = wb.names.iter().find(|d| d.name.eq_ignore_ascii_case(&dest_name(id)))?;
    let (sheet, r) = n.formula.trim_start_matches('=').rsplit_once('!')?;
    let si = wb.sheet_index(sheet.trim_matches('\''))?;
    Some((si, parse_range(&r.replace('$', ""))?))
}

fn set_destination(wb: &mut Workbook, id: u32, sheet: usize, r: Range) {
    let name = dest_name(id);
    wb.names.retain(|d| !d.name.eq_ignore_ascii_case(&name));
    let a = |p: Pos| format!("${}${}", crate::addr::col_name(p.col), p.row + 1);
    wb.names.push(DefinedName { name, sheet: Some(sheet), formula: format!("{}!{}:{}", quote_sheet(&wb.sheets[sheet].name), a(r.start()), a(r.end())), hidden: true, comment: String::new() });
}

/// Fetches a web page's text (the app passes one that uses the network).
pub type Fetch<'a> = &'a dyn Fn(&str) -> Result<String, String>;

/// The rows a connection's source gives now.
pub fn load(wb: &Workbook, c: &Connection, fetch: Fetch) -> Result<Vec<Vec<Value>>, String> {
    let strings = |rows: Vec<Vec<String>>| -> Vec<Vec<Value>> {
        rows.into_iter().map(|r| r.into_iter().map(|t| match wb.parse_input(&t).0 { crate::book::Input::Value(v) => v, _ => if t.is_empty() { Value::Empty } else { Value::str(t) } }).collect()).collect()
    };
    match &c.source {
        Source::Text { path, delimiter, encoding } => {
            let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let text = import::decode(&b, *encoding);
            let o = TextOptions { tab: *delimiter == '\t', comma: *delimiter == ',', semicolon: *delimiter == ';', space: *delimiter == ' ', other: (!['\t', ',', ';', ' '].contains(delimiter)).then_some(*delimiter), ..Default::default() };
            Ok(strings(import::split(&text, &o)))
        }
        Source::Workbook { path, item } => {
            let other = crate::open(std::path::Path::new(path))?;
            import::workbook_items(&other).into_iter().find(|(n, _, _)| n.eq_ignore_ascii_case(item)).map(|x| x.2).ok_or_else(|| format!("'{item}' was not found in {path}."))
        }
        Source::Web { url, table } => {
            let html = fetch(url)?;
            let t = import::html_tables(&html);
            t.get(table.saturating_sub(1)).map(|t| strings(t.rows.clone())).ok_or_else(|| "The web page has no table at that position.".to_string())
        }
        Source::Mashup { query } => {
            let t = crate::powerquery::evaluate(wb, query, fetch)?;
            let mut rows: Vec<Vec<Value>> = vec![t.columns.iter().map(Value::str).collect()];
            rows.extend(t.rows.iter().map(|r| r.iter().zip(&t.types).map(|(v, ty)| v.to_cell(*ty)).collect()));
            Ok(rows)
        }
        Source::Other => Err("This connection type can't be refreshed in Jadwal.".into()),
    }
}

/// Adds a connection whose data goes to (sheet, at); loads it now. Returns its id.
pub fn add(wb: &mut Workbook, name: &str, source: Source, sheet: usize, at: Pos, fetch: Fetch) -> Result<u32, String> {
    let mut all = list(wb);
    let id = all.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let base = if name.trim().is_empty() { "Query".to_string() } else { name.trim().to_string() };
    let mut nm = base.clone();
    let mut k = 1;
    while all.iter().any(|c| c.name.eq_ignore_ascii_case(&nm)) { k += 1; nm = format!("{base} ({k})"); }
    let c = Connection { id, name: nm, description: String::new(), source, refresh_on_open: false, raw: None };
    let rows = load(wb, &c, fetch)?;
    all.push(c);
    save(wb, &all);
    let r = crate::consolidate::put(wb, sheet, at, &rows);
    set_destination(wb, id, sheet, r);
    Ok(id)
}

/// Records a connection whose data the caller already placed at (sheet, r). Returns its id.
pub fn register(wb: &mut Workbook, name: &str, source: Source, sheet: usize, r: Range) -> u32 {
    let mut all = list(wb);
    let id = all.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let base = if name.trim().is_empty() { "Query".to_string() } else { name.trim().to_string() };
    let mut nm = base.clone();
    let mut k = 1;
    while all.iter().any(|c| c.name.eq_ignore_ascii_case(&nm)) { k += 1; nm = format!("{base} ({k})"); }
    all.push(Connection { id, name: nm, description: String::new(), source, refresh_on_open: false, raw: None });
    save(wb, &all);
    set_destination(wb, id, sheet, r);
    id
}

/// Refreshes one connection into its destination (the old area cleared first).
pub fn refresh(wb: &mut Workbook, id: u32, fetch: Fetch) -> Result<Range, String> {
    let c = list(wb).into_iter().find(|c| c.id == id).ok_or("The connection is gone.")?;
    if let Source::Mashup { query } = &c.source {
        let (si, ti) = crate::powerquery::refresh(wb, query, fetch)?;
        return Ok(wb.sheets[si].tables[ti].range);
    }
    let (si, old) = destination(wb, id).ok_or("The connection has no destination on a sheet.")?;
    let rows = load(wb, &c, fetch)?;
    wb.clear(si, old, true, false, false);
    let r = crate::consolidate::put(wb, si, old.start(), &rows);
    set_destination(wb, id, si, r);
    Ok(r)
}

/// Refresh All: every connection that can be refreshed; returns the errors by name.
pub fn refresh_all(wb: &mut Workbook, fetch: Fetch) -> Vec<(String, String)> {
    let mut errs = vec![];
    for c in list(wb) {
        if matches!(c.source, Source::Other) || destination(wb, c.id).is_none() { continue; }
        if let Err(e) = refresh(wb, c.id, fetch) { errs.push((c.name, e)); }
    }
    errs
}

/// Deletes a connection (its data stays as plain values).
pub fn remove(wb: &mut Workbook, id: u32) {
    let all: Vec<Connection> = list(wb).into_iter().filter(|c| c.id != id).collect();
    save(wb, &all);
    let n = dest_name(id);
    wb.names.retain(|d| !d.name.eq_ignore_ascii_case(&n));
}

/// Renames a connection or changes its properties.
pub fn update(wb: &mut Workbook, c: &Connection) {
    let mut all = list(wb);
    if let Some(x) = all.iter_mut().find(|x| x.id == c.id) { *x = c.clone(); }
    save(wb, &all);
}

// ---- external links (Edit Links) ---------------------------------------------------------------

/// An external link: its index ([1] in formulas), part, and the file it points to.
#[derive(Clone, Debug, PartialEq)]
pub struct Link { pub index: usize, pub part: String, pub target: String }

/// The workbook's external links, from its externalLink parts and their relationships.
pub fn links(wb: &Workbook) -> Vec<Link> {
    let mut v: Vec<Link> = vec![];
    let mut parts: Vec<&String> = wb.parts.keys().filter(|k| k.starts_with("xl/externalLinks/externalLink") && k.ends_with(".xml")).collect();
    parts.sort_by_key(|k| k.trim_start_matches("xl/externalLinks/externalLink").trim_end_matches(".xml").parse::<usize>().unwrap_or(0));
    for p in parts {
        let rels = format!("xl/externalLinks/_rels/{}.rels", p.rsplit('/').next().unwrap_or(""));
        let target = wb.parts.get(&rels).and_then(|b| Element::parse(&part_text(b)).ok()).and_then(|e| e.all("Relationship").find(|r| r.attr("Type").is_some_and(|t| t.ends_with("externalLinkPath"))).and_then(|r| r.attr("Target").map(String::from))).unwrap_or_default();
        let index = p.trim_start_matches("xl/externalLinks/externalLink").trim_end_matches(".xml").parse().unwrap_or(v.len() + 1);
        v.push(Link { index, part: p.clone(), target });
    }
    v
}

/// Change Source: points a link at another file.
pub fn change_link(wb: &mut Workbook, link: &Link, to: &str) {
    let rels = format!("xl/externalLinks/_rels/{}.rels", link.part.rsplit('/').next().unwrap_or(""));
    let Some(b) = wb.parts.get(&rels) else { return };
    let t = part_text(b);
    let new = t.replace(&format!("Target=\"{}\"", attr(&link.target)), &format!("Target=\"{}\"", attr(to)));
    wb.parts.insert(rels, new.into_bytes());
}

/// Break Link: formulas that refer to the link become their values.
pub fn break_link(wb: &mut Workbook, link: &Link) -> usize {
    let tag = format!("[{}]", link.index);
    let mut n = 0;
    for s in &mut wb.sheets {
        for c in s.cells.values_mut() {
            if c.formula().is_some_and(|f| f.contains(&tag)) { c.input = crate::book::Input::Value(c.value.scalar()); c.array = None; n += 1; }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_tables_tie_connections_to_cells() {
        let dir = std::env::temp_dir().join(format!("jadwal-qt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("q.csv");
        std::fs::write(&path, "a,b\n1,2\n").unwrap();
        let mut wb = Workbook::new();
        let no_web: Fetch = &|_| Err("offline".into());
        let id = add(&mut wb, "q", Source::Text { path: path.to_string_lossy().into(), delimiter: ',', encoding: import::Encoding::Utf8 }, 0, Pos::new(0, 0), no_web).unwrap();
        let bytes = crate::xlsx::write(&wb).unwrap();
        let files = crate::xlsx::pkgcheck::unzip(&bytes);
        crate::xlsx::pkgcheck::check_package(&files);
        let qt = &files["xl/queryTables/queryTable1.xml"];
        assert!(qt.contains(&format!("name=\"ExternalData_{id}\" connectionId=\"{id}\"")) && qt.contains("<queryTableField id=\"2\" name=\"b\"/>"), "{qt}");
        assert!(files["xl/connections.xml"].contains("prompt=\"0\""));
        // Read and saved again: still one query table.
        let again = crate::xlsx::pkgcheck::unzip(&crate::xlsx::write(&crate::xlsx::read(&bytes).unwrap()).unwrap());
        assert_eq!(again.keys().filter(|k| k.starts_with("xl/queryTables/") && !k.contains("_rels")).count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn text_connection_refreshes_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("jadwal-conn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("d.csv");
        std::fs::write(&path, "a,b\n1,2\n").unwrap();
        let mut wb = Workbook::new();
        let none: Fetch = &|_| Err("offline".into());
        let id = add(&mut wb, "d", Source::Text { path: path.to_string_lossy().into(), delimiter: ',', encoding: import::Encoding::Utf8 }, 0, Pos::new(2, 1), none).unwrap();
        assert_eq!(wb.display(0, Pos::new(3, 2)), "2");
        std::fs::write(&path, "a,b\n1,5\n3,4\n").unwrap();
        let r = refresh(&mut wb, id, none).unwrap();
        assert_eq!(r, parse_range("B3:C5").unwrap());
        assert_eq!(wb.display(0, Pos::new(3, 2)), "5");
        // Range and web sources.
        let web: Fetch = &|_| Ok("<table><tr><td>x</td><td>9</td></tr></table>".into());
        let w = add(&mut wb, "", Source::Web { url: "http://example.com".into(), table: 1 }, 0, Pos::new(10, 0), web).unwrap();
        assert_eq!(wb.display(0, Pos::new(10, 1)), "9");
        add(&mut wb, "copy", Source::Web { url: "http://example.com/2".into(), table: 1 }, 0, Pos::new(20, 0), web).unwrap();
        assert_eq!(wb.display(0, Pos::new(20, 1)), "9");
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let l = list(&back);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].source, Source::Text { path: path.to_string_lossy().into(), delimiter: ',', encoding: import::Encoding::Utf8 });
        assert_eq!(l[1].source, Source::Web { url: "http://example.com".into(), table: 1 });
        assert_eq!(l[1].name, "Query");
        assert_eq!(destination(&back, id), Some((0, parse_range("B3:C5").unwrap())));
        assert!(refresh_all(&mut wb, web).is_empty());
        remove(&mut wb, w);
        assert_eq!(list(&wb).len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
