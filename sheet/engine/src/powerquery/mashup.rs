//! The DataMashup part ([MS-QDEFF]): a custom XML part (customXml/itemN.xml) whose root
//! `DataMashup` element holds a base64 binary stream of four length-prefixed fields after a
//! version: the package parts (an OPC zip with [Content_Types].xml, Config/Package.xml and
//! Formulas/Section1.m), the permissions XML, the metadata (version, metadata XML, content
//! package) and the permission bindings (a DPAPI checksum, or the single null byte the spec
//! allows, which makes Excel fall back to the default permissions).

use super::{mlang, Load, Query};
use crate::book::Workbook;
use crate::xlsx::xml::{attr, part_text, text, Element};
use std::io::{Read, Write};

pub const NS: &str = "http://schemas.microsoft.com/DataMashup";
const RT_CUSTOM_XML_PROPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps";
pub const CT_ITEM_PROPS: &str = "application/vnd.openxmlformats-officedocument.customXmlProperties+xml";

// ---- base64 -------------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut o = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 { if k <= c.len() { o.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char); } else { o.push('='); } }
    }
    o
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c { b'A'..=b'Z' => c - b'A', b'a'..=b'z' => c - b'a' + 26, b'0'..=b'9' => c - b'0' + 52, b'+' => 62, b'/' => 63, b'=' => break, c if c.is_ascii_whitespace() => continue, _ => return Err("The DataMashup isn't valid base64.".into()) };
        acc = acc << 6 | v as u32; bits += 6;
        if bits >= 8 { bits -= 8; out.push((acc >> bits) as u8); acc &= (1 << bits) - 1; }
    }
    Ok(out)
}

// ---- the binary stream ------------------------------------------------------------------------

/// The decoded DataMashup stream.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stream {
    /// The package parts: (path, bytes), in package order.
    pub parts: Vec<(String, Vec<u8>)>,
    pub permissions: Vec<u8>,
    pub metadata_xml: String,
    pub metadata_content: Vec<u8>,
    pub bindings: Vec<u8>,
}

fn take<'a>(b: &'a [u8], at: &mut usize) -> Result<&'a [u8], String> {
    let n = b.get(*at..*at + 4).ok_or("The DataMashup stream is truncated.")?;
    let n = u32::from_le_bytes([n[0], n[1], n[2], n[3]]) as usize;
    let v = b.get(*at + 4..*at + 4 + n).ok_or("The DataMashup stream is truncated.")?;
    *at += 4 + n;
    Ok(v)
}

fn unzip(b: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(b)).map_err(|e| format!("The query package can't be read: {e}"))?;
    (0..z.len()).map(|i| { let mut f = z.by_index(i).map_err(|e| e.to_string())?; let mut v = vec![]; f.read_to_end(&mut v).map_err(|e| e.to_string())?; Ok((f.name().to_string(), v)) }).collect()
}

fn zip_parts(parts: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, d) in parts { z.start_file(n.as_str(), o).map_err(|e| e.to_string())?; z.write_all(d).map_err(|e| e.to_string())?; }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

pub fn decode(bytes: &[u8]) -> Result<Stream, String> {
    let ver = bytes.get(..4).ok_or("The DataMashup stream is empty.")?;
    if ver != [0, 0, 0, 0] { return Err("The DataMashup version isn't 0.".into()); }
    let mut at = 4;
    let parts = unzip(take(bytes, &mut at)?)?;
    let permissions = take(bytes, &mut at)?.to_vec();
    let meta = take(bytes, &mut at)?;
    let bindings = if at < bytes.len() { take(bytes, &mut at)?.to_vec() } else { vec![] };
    let mut m = 4;
    if meta.get(..4).is_some_and(|v| v != [0, 0, 0, 0]) { return Err("The DataMashup metadata version isn't 0.".into()); }
    let xml = take(meta, &mut m)?;
    let metadata_xml = part_text(xml);
    let metadata_content = if m < meta.len() { take(meta, &mut m)?.to_vec() } else { vec![] };
    Ok(Stream { parts, permissions, metadata_xml, metadata_content, bindings })
}

fn push(out: &mut Vec<u8>, b: &[u8]) { out.extend_from_slice(&(b.len() as u32).to_le_bytes()); out.extend_from_slice(b); }

pub fn encode(s: &Stream) -> Result<Vec<u8>, String> {
    let mut out = vec![0, 0, 0, 0];
    push(&mut out, &zip_parts(&s.parts)?);
    push(&mut out, &s.permissions);
    let mut meta = vec![0, 0, 0, 0];
    push(&mut meta, &[BOM.as_bytes(), s.metadata_xml.as_bytes()].concat());
    push(&mut meta, &s.metadata_content);
    push(&mut out, &meta);
    push(&mut out, &s.bindings);
    Ok(out)
}

const BOM: &str = "\u{feff}";
/// An empty OPC package (just the end of central directory), as Excel writes for no content.
const EMPTY_ZIP: [u8; 22] = [0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn new_stream() -> Stream {
    Stream {
        parts: vec![
            ("[Content_Types].xml".into(), format!("{BOM}<?xml version=\"1.0\" encoding=\"utf-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"text/xml\" /><Default Extension=\"m\" ContentType=\"application/x-ms-m\" /></Types>").into_bytes()),
            ("Config/Package.xml".into(), format!("{BOM}<?xml version=\"1.0\" encoding=\"utf-8\"?><Package xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\"><Version>2.24.4065.1401</Version><MinVersion>2.21.0.0</MinVersion><Culture>en-US</Culture><SafeCombine>true</SafeCombine><Items /></Package>").into_bytes()),
            ("Formulas/Section1.m".into(), b"section Section1;".to_vec()),
        ],
        permissions: format!("{BOM}<?xml version=\"1.0\" encoding=\"utf-8\"?><PermissionList xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\"><CanEvaluateFuturePackages>false</CanEvaluateFuturePackages><FirewallEnabled>true</FirewallEnabled><WorkbookGroupType xsi:nil=\"true\" /></PermissionList>").into_bytes(),
        metadata_xml: String::new(),
        metadata_content: EMPTY_ZIP.to_vec(),
        bindings: vec![0],
    }
}

// ---- the custom XML part ----------------------------------------------------------------------

/// The custom XML part holding the DataMashup, if any.
pub fn part_name(wb: &Workbook) -> Option<String> {
    let mut keys: Vec<&String> = wb.parts.keys().filter(|k| k.starts_with("customXml/item") && !k.contains("Props") && k.ends_with(".xml")).collect();
    keys.sort();
    keys.into_iter().find(|k| Element::parse(&part_text(&wb.parts[*k])).is_ok_and(|e| e.local() == "DataMashup")).cloned()
}

/// The queries read from the workbook.
#[derive(Clone, Debug, Default)]
pub struct Mashup {
    pub queries: Vec<Query>,
    /// The stream as read (kept parts: permissions, metadata content, embedded contents).
    pub stream: Option<Stream>,
    /// The DataMashup root's attributes (sqmid).
    pub root_attrs: Vec<(String, String)>,
    /// The AllFormulas item's entries and other items not about a query.
    pub all_entries: Vec<(String, String)>,
    /// Whether Section1.m could be split into its members (else the part is never rewritten).
    pub section_ok: bool,
}

/// The DataMashup's text: base64 of the stream.
pub fn read_queries(wb: &Workbook) -> Result<Mashup, String> {
    let Some(p) = part_name(wb) else { return Ok(Mashup { section_ok: true, ..Default::default() }) };
    let root = Element::parse(&part_text(&wb.parts[&p]))?;
    let stream = decode(&b64_decode(&root.text())?)?;
    let mut m = from_stream(&stream)?;
    m.root_attrs = root.attrs.iter().filter(|(k, _)| !k.starts_with("xmlns")).cloned().collect();
    m.stream = Some(stream);
    Ok(m)
}

/// Unescapes an ItemPath ("Section1/My%20Query").
fn unescape_path(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() { if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) { out.push(v); i += 3; continue; } }
        out.push(b[i]); i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn escape_path(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() { if c.is_ascii_alphanumeric() || "-_.~()".contains(c) { o.push(c); } else { let mut b = [0u8; 4]; for x in c.encode_utf8(&mut b).bytes() { o.push_str(&format!("%{x:02X}")); } } }
    o
}

pub fn from_stream(s: &Stream) -> Result<Mashup, String> {
    let section = s.parts.iter().find(|(n, _)| n.trim_start_matches('/') == "Formulas/Section1.m").map(|(_, b)| part_text(b)).unwrap_or_default();
    let mut m = Mashup::default();
    let members = match mlang::parse_section(&section) { Ok(v) => { m.section_ok = true; v } Err(_) => vec![] };
    // The metadata of each query.
    let mut metas: Vec<(String, Vec<(String, String)>)> = vec![];
    if let Ok(root) = Element::parse(&s.metadata_xml) {
        if let Some(items) = root.child("Items") {
            for it in items.all("Item") {
                let loc = it.child("ItemLocation");
                let ty = loc.and_then(|l| l.child("ItemType")).map(|t| t.text()).unwrap_or_default();
                let path = loc.and_then(|l| l.child("ItemPath")).map(|t| unescape_path(&t.text())).unwrap_or_default();
                let entries: Vec<(String, String)> = it.child("StableEntries").map(|se| se.all("Entry").filter_map(|e| Some((e.attr("Type")?.to_string(), e.attr("Value").unwrap_or("").to_string()))).collect()).unwrap_or_default();
                if ty == "AllFormulas" { m.all_entries = entries; } else if let Some(name) = path.strip_prefix("Section1/") { if !name.contains('/') { metas.push((name.to_string(), entries)); } }
            }
        }
    }
    for mem in members {
        let mut q = match &mem.expr { Ok(e) => match super::steps_from(e) { Ok((src, steps)) => Query { source: Some(src), steps, ..Query::from_formula(&mem.name, "") }, Err(_) => Query::from_formula(&mem.name, &mem.text) }, Err(_) => Query::from_formula(&mem.name, &mem.text) };
        q.raw = if q.source.is_none() { Some(mem.text.clone()) } else { None };
        q.meta = metas.iter().find(|x| x.0 == mem.name).map(|x| x.1.clone()).unwrap_or_default();
        let get = |k: &str| q.meta.iter().find(|e| e.0 == k).map(|e| e.1.clone());
        let fill = get("FillEnabled").is_some_and(|v| v == "l1");
        q.load = match get("FillTarget").and_then(|v| v.strip_prefix('s').map(String::from)) { Some(t) if fill && !t.is_empty() => Load::Table(t), _ => Load::ConnectionOnly };
        q.description = get("Description").and_then(|v| v.strip_prefix('s').map(String::from)).unwrap_or_default();
        m.queries.push(q);
    }
    Ok(m)
}

/// The metadata XML for the queries.
fn metadata_xml(queries: &[Query], all: &[(String, String)]) -> String {
    let entry = |k: &str, v: &str| format!("<Entry Type=\"{}\" Value=\"{}\" />", attr(k), attr(v));
    let mut x = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><LocalPackageMetadataFile xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\"><Items><Item><ItemLocation><ItemType>AllFormulas</ItemType><ItemPath /></ItemLocation>");
    if all.is_empty() { x.push_str("<StableEntries />"); } else { x.push_str("<StableEntries>"); for (k, v) in all { x.push_str(&entry(k, v)); } x.push_str("</StableEntries>"); }
    x.push_str("</Item>");
    for q in queries {
        // Entries Jadwal sets; the rest are kept as read (stale column types and relationship
        // information, which Excel recomputes on refresh, are dropped).
        let table = match &q.load { Load::Table(t) => Some(t.clone()), Load::ConnectionOnly => None };
        let mut e: Vec<(String, String)> = vec![
            ("IsPrivate".into(), "l0".into()),
            ("FillEnabled".into(), if table.is_some() { "l1" } else { "l0" }.into()),
            ("FillObjectType".into(), if table.is_some() { "sTable" } else { "sConnectionOnly" }.into()),
            ("FillToDataModelEnabled".into(), "l0".into()),
            ("ResultType".into(), "sTable".into()),
            ("NameUpdatedAfterFill".into(), "l0".into()),
        ];
        if let Some(t) = &table { e.push(("FillTarget".into(), format!("s{t}"))); e.push(("FilledCompleteResultToWorksheet".into(), "l1".into())); }
        e.push(("AddedToDataModel".into(), "l0".into()));
        e.push(("LoadedToDataModel".into(), "l0".into()));
        if !q.description.is_empty() { e.push(("Description".into(), format!("s{}", q.description))); }
        let set: Vec<String> = e.iter().map(|x| x.0.clone()).collect();
        for (k, v) in &q.meta {
            if set.contains(k) || matches!(k.as_str(), "FillColumnTypes" | "RelationshipInfoContainer" | "FillTarget" | "FilledCompleteResultToWorksheet") { continue; }
            e.push((k.clone(), v.clone()));
        }
        if !e.iter().any(|x| x.0 == "QueryID") { e.push(("QueryID".into(), format!("s{}", guid(&q.name)))); }
        if !e.iter().any(|x| x.0 == "FillLastUpdated") { e.push(("FillLastUpdated".into(), format!("d{}", now_stamp()))); }
        if !e.iter().any(|x| x.0 == "FillStatus") { e.push(("FillStatus".into(), "sComplete".into())); }
        x.push_str(&format!("<Item><ItemLocation><ItemType>Formula</ItemType><ItemPath>Section1/{}</ItemPath></ItemLocation><StableEntries>", text(&escape_path(&q.name))));
        for (k, v) in &e { x.push_str(&entry(k, v)); }
        x.push_str("</StableEntries></Item>");
        // An item per step, as Excel writes them.
        for s in q.source_steps().iter().chain(q.steps.iter().map(|s| &s.name)) {
            x.push_str(&format!("<Item><ItemLocation><ItemType>Formula</ItemType><ItemPath>Section1/{}/{}</ItemPath></ItemLocation><StableEntries /></Item>", text(&escape_path(&q.name)), text(&escape_path(s))));
        }
    }
    x.push_str("</Items></LocalPackageMetadataFile>");
    x
}

/// Section1.m with every query.
pub fn section_text(queries: &[Query]) -> String {
    let mut s = String::from("section Section1;");
    for q in queries {
        if !q.description.is_empty() { s.push_str(&format!("\r\n\r\n[ Description = {} ]", mlang::quote(&q.description))); s.push_str(&format!("\r\nshared {} = {};", mlang::ident(&q.name), q.formula())); }
        else { s.push_str(&format!("\r\n\r\nshared {} = {};", mlang::ident(&q.name), q.formula())); }
    }
    s
}

/// The stream for the queries, building on the one read (its other parts kept).
pub fn stream_for(old: Option<&Stream>, all: &[(String, String)], queries: &[Query]) -> Stream {
    let mut s = old.cloned().unwrap_or_else(new_stream);
    let sec = section_text(queries).into_bytes();
    match s.parts.iter_mut().find(|(n, _)| n.trim_start_matches('/') == "Formulas/Section1.m") { Some(p) => p.1 = sec, None => s.parts.push(("Formulas/Section1.m".into(), sec)) }
    s.metadata_xml = metadata_xml(queries, all);
    if s.metadata_content.is_empty() { s.metadata_content = EMPTY_ZIP.to_vec(); }
    // The package changed, so a DPAPI checksum read from the file no longer holds.
    s.bindings = vec![0];
    s
}

/// Writes the queries into the DataMashup part (made, with its item properties, when missing;
/// removed when there are no queries).
pub fn write_queries(wb: &mut Workbook, queries: &[Query]) -> Result<(), String> {
    let old = read_queries(wb)?;
    if !old.section_ok { return Err("The workbook's queries are written in a form Jadwal can't change; they're kept as they are.".into()); }
    let existing = part_name(wb);
    if queries.is_empty() {
        if let Some(p) = existing {
            let props = wb.parts.get(&crate::xlsx::rels_name_of(&p)).and_then(|b| Element::parse(&part_text(b)).ok()).and_then(|e| e.all("Relationship").find_map(|r| r.attr("Target").map(|t| crate::xlsx::resolve_part(&p, t))));
            wb.parts.remove(&p);
            wb.parts.remove(&crate::xlsx::rels_name_of(&p));
            if let Some(pp) = props { wb.parts.remove(&pp); }
        }
        return Ok(());
    }
    let stream = stream_for(old.stream.as_ref(), &old.all_entries, queries);
    let b64 = b64_encode(&encode(&stream)?);
    let attrs: String = old.root_attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", attr(v))).collect();
    let xml = format!("<?xml version=\"1.0\" encoding=\"utf-16\"?><DataMashup{attrs} xmlns=\"{NS}\">{b64}</DataMashup>");
    // Excel writes the part in UTF-16 (little endian, with a byte order mark).
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() { bytes.extend_from_slice(&u.to_le_bytes()); }
    let p = existing.unwrap_or_else(|| {
        let n = (1..).find(|i| !wb.parts.contains_key(&format!("customXml/item{i}.xml")) && !wb.parts.contains_key(&format!("customXml/itemProps{i}.xml"))).unwrap();
        let props = format!("customXml/itemProps{n}.xml");
        wb.parts.insert(props, format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\r\n<ds:datastoreItem ds:itemID=\"{{{}}}\" xmlns:ds=\"http://schemas.openxmlformats.org/officeDocument/2006/customXml\"><ds:schemaRefs/></ds:datastoreItem>", guid(&format!("item{n}")).to_uppercase()).into_bytes());
        wb.parts.insert(format!("customXml/_rels/item{n}.xml.rels"), format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{RT_CUSTOM_XML_PROPS}\" Target=\"itemProps{n}.xml\"/></Relationships>").into_bytes());
        format!("customXml/item{n}.xml")
    });
    wb.parts.insert(p, bytes);
    Ok(())
}

/// A GUID derived from a name and the time (no random source is needed for these ids).
pub fn guid(seed: &str) -> String {
    use sha2::{Digest, Sha256};
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let h = Sha256::digest(format!("{seed}{t}").as_bytes());
    let x: String = h.iter().take(16).map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-4{}-a{}-{}", &x[..8], &x[8..12], &x[13..16], &x[17..20], &x[20..32])
}

/// Now as metadata writes a date: 2015-09-10T04:45:41.9275900Z.
pub fn now_stamp() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (y, m, day) = crate::date::civil_from_days(secs.div_euclid(86400));
    let t = secs.rem_euclid(86400);
    format!("{y:04}-{m:02}-{day:02}T{:02}:{:02}:{:02}.{:07}Z", t / 3600, t / 60 % 60, t % 60, d.subsec_nanos() / 100)
}
