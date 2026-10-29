//! Review and view state Excel keeps in the file (comments.js, protect.js, customviews.js):
//! threaded comments with replies, @mentions and resolve (xl/threadedComments/*, the people in
//! xl/persons/person.xml; a legacy note is written next to each thread, as Excel does, so older
//! readers still see it); sheet protection with Excel's password hash (SHA-512 of salt +
//! UTF-16LE password, then `spinCount` rounds of SHA-512(hash + round index)), the allowed
//! actions and Allow Edit Ranges; workbook structure protection; custom views (a
//! customWorkbookView plus each sheet's customSheetView and its Z_{guid}_.wvu.Rows/Cols names);
//! and split panes (a sheetView pane without state="frozen", splits in twentieths of a point).

use crate::addr::{parse_range, Pos, Range};
use crate::book::{DefinedName, Sheet, Workbook};
use crate::xlsx::xml::{attr, text, Element};
use sha2::{Digest, Sha512};
use std::collections::{BTreeMap, BTreeSet};

pub const RT_THREADED: &str = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
pub const RT_PERSON: &str = "http://schemas.microsoft.com/office/2017/10/relationships/person";
pub const CT_THREADED: &str = "application/vnd.ms-excel.threadedcomments+xml";
pub const CT_PERSON: &str = "application/vnd.ms-excel.person+xml";
const NS_TC: &str = "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

// ---------------------------------------------------------------------------------------------
// Model

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Person { pub id: String, pub name: String, pub user_id: String, pub provider: String }

/// One comment of a thread (the first is the thread's start, the rest replies).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Comment { pub id: String, pub person: String, pub date: String, pub text: String }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Thread { pub comments: Vec<Comment>, pub done: bool }

/// A stored password: Excel's agile hash, or the legacy 16-bit `password` attribute.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PwHash { pub algorithm: String, pub hash: String, pub salt: String, pub spin: u32, pub legacy: Option<String> }

/// The actions a protected sheet still allows, in the Protect Sheet dialog's order.
pub const ACTIONS: [(&str, &str); 15] = [
    ("selectLockedCells", "Select locked cells"), ("selectUnlockedCells", "Select unlocked cells"), ("formatCells", "Format cells"),
    ("formatColumns", "Format columns"), ("formatRows", "Format rows"), ("insertColumns", "Insert columns"), ("insertRows", "Insert rows"),
    ("insertHyperlinks", "Insert hyperlinks"), ("deleteColumns", "Delete columns"), ("deleteRows", "Delete rows"), ("sort", "Sort"),
    ("autoFilter", "Use AutoFilter"), ("pivotTables", "Use PivotTable & PivotChart"), ("objects", "Edit objects"), ("scenarios", "Edit scenarios"),
];
/// Whether each attribute means "protected" when absent (the schema's defaults).
const PROTECTED_BY_DEFAULT: [bool; 15] = [false, false, true, true, true, true, true, true, true, true, true, true, true, false, false];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { SelectLocked, SelectUnlocked, FormatCells, FormatColumns, FormatRows, InsertColumns, InsertRows, InsertHyperlinks, DeleteColumns, DeleteRows, Sort, AutoFilter, PivotTables, Objects, Scenarios }

#[derive(Clone, Debug, PartialEq)]
pub struct SheetProtection { pub password: Option<PwHash>, pub allow: [bool; 15] }

impl Default for SheetProtection {
    /// Excel's dialog defaults: only selecting cells is allowed.
    fn default() -> Self { let mut allow = [false; 15]; allow[0] = true; allow[1] = true; SheetProtection { password: None, allow } }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct EditRange { pub name: String, pub ranges: Vec<Range>, pub password: Option<PwHash> }

/// One sheet's state in a custom view.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ViewSheet { pub zoom: u32, pub gridlines: bool, pub headers: bool, pub selection: Option<Range>, pub frozen: (u32, u32), pub hidden: bool, pub hidden_rows: BTreeSet<u32>, pub hidden_cols: BTreeSet<u32> }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct CustomView { pub name: String, pub guid: String, pub active: usize, pub sheets: Vec<ViewSheet>, pub hidden_rows: bool }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SheetReview {
    pub threads: BTreeMap<Pos, Thread>,
    pub protection: SheetProtection,
    pub edit_ranges: Vec<EditRange>,
    /// Split panes: where the bars sit, in pixels from the grid's top-left (after the headings).
    pub split: Option<(f32, f32)>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BookReview {
    pub persons: Vec<Person>,
    pub structure: bool,
    pub windows: bool,
    pub structure_password: Option<PwHash>,
    pub views: Vec<CustomView>,
}

// ---------------------------------------------------------------------------------------------
// Passwords

pub fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 { if k <= c.len() { out.push(T[(n >> (18 - 6 * k) & 63) as usize] as char); } else { out.push('='); } }
    }
    out
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> { Some(match c { b'A'..=b'Z' => c - b'A', b'a'..=b'z' => c - b'a' + 26, b'0'..=b'9' => c - b'0' + 52, b'+' => 62, b'/' => 63, _ => return None } as u32) };
    let bytes: Vec<u8> = s.bytes().filter(|c| *c != b'=' && !c.is_ascii_whitespace()).collect();
    let mut out = vec![];
    for c in bytes.chunks(4) {
        let mut n = 0u32;
        for (k, b) in c.iter().enumerate() { n |= val(*b)? << (18 - 6 * k); }
        for k in 0..c.len().saturating_sub(1) { out.push((n >> (16 - 8 * k)) as u8); }
    }
    Some(out)
}

/// Excel's agile password hash (SHA-512).
pub fn hash_password(password: &str, salt: &[u8], spin: u32) -> String {
    let utf16: Vec<u8> = password.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut h = Sha512::new();
    h.update(salt);
    h.update(&utf16);
    let mut hash = h.finalize().to_vec();
    for i in 0..spin {
        let mut h = Sha512::new();
        h.update(&hash);
        h.update(i.to_le_bytes());
        hash = h.finalize().to_vec();
    }
    b64(&hash)
}

/// The legacy 16-bit hash (the `password` attribute of old files), as 4 hex digits.
pub fn legacy_hash(password: &str) -> String {
    let bytes: Vec<u8> = password.chars().map(|c| c as u32 as u8).collect();
    let mut h: u16 = 0;
    for &b in bytes.iter().rev() { h = ((h >> 14) & 1) | ((h << 1) & 0x7fff); h ^= b as u16; }
    h = ((h >> 14) & 1) | ((h << 1) & 0x7fff);
    h ^= bytes.len() as u16;
    h ^= 0xCE4B;
    format!("{h:04X}")
}

fn random_bytes(n: usize) -> Vec<u8> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
    let mut x = seed ^ COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed) ^ (&seed as *const u64 as u64);
    (0..n).map(|_| { x ^= x << 13; x ^= x >> 7; x ^= x << 17; (x >> 24) as u8 }).collect()
}

/// A GUID in braces, upper case (as Excel writes ids).
pub fn guid() -> String {
    let b = random_bytes(16);
    let h: String = b.iter().map(|x| format!("{x:02X}")).collect();
    format!("{{{}-{}-4{}-A{}-{}}}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}

impl PwHash {
    /// A new hash for a password (None when it's empty), with a random salt and 100,000 rounds.
    pub fn new(password: &str) -> Option<PwHash> {
        if password.is_empty() { return None; }
        let salt = random_bytes(16);
        Some(PwHash { algorithm: "SHA-512".into(), hash: hash_password(password, &salt, 100_000), salt: b64(&salt), spin: 100_000, legacy: None })
    }
    pub fn matches(&self, password: &str) -> bool {
        if let Some(l) = &self.legacy { if self.hash.is_empty() { return l.eq_ignore_ascii_case(&legacy_hash(password)); } }
        if !self.algorithm.eq_ignore_ascii_case("SHA-512") { return false; }
        b64_decode(&self.salt).is_some_and(|s| hash_password(password, &s, self.spin) == self.hash)
    }
    fn read(e: &Element, prefix: &str) -> Option<PwHash> {
        let k = |n: &str| if prefix.is_empty() { n.to_string() } else { format!("{prefix}{}{}", n[..1].to_uppercase(), &n[1..]) };
        let hash = e.attr(&k("hashValue")).map(String::from);
        let legacy = e.attr(if prefix.is_empty() { "password" } else { "workbookPassword" }).map(String::from);
        if hash.is_none() && legacy.is_none() { return None; }
        Some(PwHash { algorithm: e.attr(&k("algorithmName")).unwrap_or("SHA-512").into(), hash: hash.unwrap_or_default(), salt: e.attr(&k("saltValue")).unwrap_or("").into(), spin: e.attr_u32(&k("spinCount")).unwrap_or(100_000), legacy })
    }
    fn xml(&self, prefix: &str) -> String {
        let k = |n: &str| if prefix.is_empty() { n.to_string() } else { format!("{prefix}{}{}", n[..1].to_uppercase(), &n[1..]) };
        let mut s = String::new();
        if let Some(l) = &self.legacy { s.push_str(&format!(" {}=\"{}\"", if prefix.is_empty() { "password" } else { "workbookPassword" }, attr(l))); }
        if !self.hash.is_empty() { s.push_str(&format!(" {}=\"{}\" {}=\"{}\" {}=\"{}\" {}=\"{}\"", k("algorithmName"), attr(&self.algorithm), k("hashValue"), attr(&self.hash), k("saltValue"), attr(&self.salt), k("spinCount"), self.spin)); }
        s
    }
}

// ---------------------------------------------------------------------------------------------
// Protection

impl SheetProtection {
    pub fn allows(&self, a: Action) -> bool { self.allow[a as usize] }
    pub fn read(raw: &str) -> SheetProtection {
        let Ok(e) = Element::parse(raw) else { return Default::default() };
        let mut allow = [false; 15];
        for (i, (k, _)) in ACTIONS.iter().enumerate() { allow[i] = !e.flag(k, PROTECTED_BY_DEFAULT[i]); }
        SheetProtection { password: PwHash::read(&e, ""), allow }
    }
    pub fn xml(&self) -> String {
        let mut s = String::from("<sheetProtection");
        if let Some(p) = &self.password { s.push_str(&p.xml("")); }
        s.push_str(" sheet=\"1\"");
        for (i, (k, _)) in ACTIONS.iter().enumerate() {
            let protected = !self.allow[i];
            if protected != PROTECTED_BY_DEFAULT[i] || i >= 13 { s.push_str(&format!(" {k}=\"{}\"", protected as u8)); }
        }
        s + "/>"
    }
}

fn set_extra(sheet: &mut Sheet, name: &str, xml: Option<String>) {
    sheet.extra.retain(|(k, _)| k != name);
    if let Some(x) = xml { sheet.extra.push((name.into(), x)); }
}

/// Protects a sheet with these allowed actions (the XML Excel reads goes with the sheet).
pub fn protect_sheet(sheet: &mut Sheet, p: SheetProtection) {
    set_extra(sheet, "sheetProtection", Some(p.xml()));
    sheet.review.protection = p;
    sheet.protected = true;
}

/// Unprotects a sheet if the password matches (or there is none).
pub fn unprotect_sheet(sheet: &mut Sheet, password: &str) -> bool {
    if sheet.review.protection.password.as_ref().is_some_and(|h| !h.matches(password)) { return false; }
    set_extra(sheet, "sheetProtection", None);
    sheet.review.protection = Default::default();
    sheet.protected = false;
    true
}

/// Whether the sheet's protection lets this happen.
pub fn allowed(sheet: &Sheet, a: Action) -> bool { !sheet.protected || sheet.review.protection.allows(a) }

/// Whether a cell can be edited: unlocked, or inside an Allow Edit Range (one with a password
/// asks for it first; `unlocked` lists the ranges already opened this session).
pub fn editable(wb: &Workbook, si: usize, p: Pos, unlocked: &[String]) -> Result<(), Option<String>> {
    let s = &wb.sheets[si];
    if !s.protected || !wb.style_at(si, p).locked { return Ok(()); }
    for r in &s.review.edit_ranges {
        if r.ranges.iter().any(|x| x.contains(p)) { return if r.password.is_none() || unlocked.contains(&r.name) { Ok(()) } else { Err(Some(r.name.clone())) }; }
    }
    Err(None)
}

pub fn set_edit_ranges(sheet: &mut Sheet, ranges: Vec<EditRange>) {
    let xml = (!ranges.is_empty()).then(|| {
        let mut s = String::from("<protectedRanges>");
        for r in &ranges {
            s.push_str("<protectedRange");
            if let Some(p) = &r.password { s.push_str(&p.xml("")); }
            s.push_str(&format!(" sqref=\"{}\" name=\"{}\"/>", r.ranges.iter().map(|x| x.a1()).collect::<Vec<_>>().join(" "), attr(&r.name)));
        }
        s + "</protectedRanges>"
    });
    set_extra(sheet, "protectedRanges", xml);
    sheet.review.edit_ranges = ranges;
}

pub fn protect_workbook(wb: &mut Workbook, password: &str) { wb.review.structure = true; wb.review.structure_password = PwHash::new(password); }
pub fn unprotect_workbook(wb: &mut Workbook, password: &str) -> bool {
    if wb.review.structure_password.as_ref().is_some_and(|h| !h.matches(password)) { return false; }
    wb.review.structure = false; wb.review.windows = false; wb.review.structure_password = None;
    true
}

/// The workbookProtection element (empty when the structure isn't locked).
pub fn workbook_protection_xml(wb: &Workbook) -> String {
    if !wb.review.structure && !wb.review.windows { return String::new(); }
    let mut s = String::from("<workbookProtection");
    if let Some(p) = &wb.review.structure_password { s.push_str(&p.xml("workbook")); }
    if wb.review.structure { s.push_str(" lockStructure=\"1\""); }
    if wb.review.windows { s.push_str(" lockWindows=\"1\""); }
    s + "/>"
}

// ---------------------------------------------------------------------------------------------
// Threaded comments

fn now_iso() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let secs = ms / 1000;
    let (y, m, d) = crate::date::civil_from_days(secs.div_euclid(86400));
    let t = secs.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:02}", t / 3600, t / 60 % 60, t % 60, (ms % 1000) / 10)
}

impl BookReview {
    /// The person with this display name, added if new.
    pub fn person(&mut self, name: &str) -> String {
        if let Some(p) = self.persons.iter().find(|p| p.name == name) { return p.id.clone(); }
        let id = guid();
        self.persons.push(Person { id: id.clone(), name: name.into(), user_id: name.into(), provider: "None".into() });
        id
    }
    pub fn name_of(&self, id: &str) -> String { self.persons.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default() }
}

/// Starts a thread on a cell, or replies to the one there.
pub fn add_comment(wb: &mut Workbook, si: usize, p: Pos, author: &str, body: &str) {
    let person = wb.review.person(author);
    let c = Comment { id: guid(), person, date: now_iso(), text: body.to_string() };
    wb.sheets[si].review.threads.entry(p).or_default().comments.push(c);
}

/// The @Name mentions in a comment that name known people: (person id, char start, char length).
pub fn mentions(wb: &Workbook, body: &str) -> Vec<(String, usize, usize)> {
    let chars: Vec<char> = body.chars().collect();
    let mut out = vec![];
    for (i, c) in chars.iter().enumerate() {
        if *c != '@' || (i > 0 && chars[i - 1].is_alphanumeric()) { continue; }
        let rest: String = chars[i + 1..].iter().collect();
        if let Some(p) = wb.review.persons.iter().filter(|p| !p.name.is_empty() && rest.starts_with(&p.name)).max_by_key(|p| p.name.len()) {
            out.push((p.id.clone(), i, p.name.chars().count() + 1));
        }
    }
    out
}

pub fn read_threads(sheet: &mut Sheet, xml: &str) {
    let Ok(root) = Element::parse(xml) else { return };
    let mut by_id: BTreeMap<String, Pos> = BTreeMap::new();
    for e in root.all("threadedComment") {
        let Some(p) = e.attr("ref").and_then(parse_range).map(|r| r.start()) else { continue };
        let c = Comment { id: e.attr("id").unwrap_or("").into(), person: e.attr("personId").unwrap_or("").into(), date: e.attr("dT").unwrap_or("").into(), text: e.child("text").map(|t| t.text()).unwrap_or_default() };
        let at = e.attr("parentId").and_then(|pid| by_id.get(pid).copied()).unwrap_or(p);
        let t = sheet.review.threads.entry(at).or_default();
        if e.attr("parentId").is_none() { t.done = e.flag("done", false); }
        by_id.insert(c.id.clone(), at);
        t.comments.push(c);
    }
    // The legacy notes Excel writes beside threads are only a copy.
    for p in sheet.review.threads.keys() { sheet.notes.remove(p); }
}

pub fn read_persons(wb: &mut Workbook, xml: &str) {
    let Ok(root) = Element::parse(xml) else { return };
    for e in root.all("person") {
        wb.review.persons.push(Person { id: e.attr("id").unwrap_or("").into(), name: e.attr("displayName").unwrap_or("").into(), user_id: e.attr("userId").unwrap_or("").into(), provider: e.attr("providerId").unwrap_or("None").into() });
    }
}

pub fn threads_xml(wb: &Workbook, si: usize) -> Option<String> {
    let s = &wb.sheets[si];
    if s.review.threads.is_empty() { return None; }
    let mut o = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<ThreadedComments xmlns=\"{NS_TC}\" xmlns:x=\"{NS_MAIN}\">");
    for (p, t) in &s.review.threads {
        let root_id = t.comments.first().map(|c| c.id.clone()).unwrap_or_default();
        for (k, c) in t.comments.iter().enumerate() {
            o.push_str(&format!("<threadedComment ref=\"{}\" dT=\"{}\" personId=\"{}\" id=\"{}\"", p.a1(), attr(&c.date), attr(&c.person), attr(&c.id)));
            if k > 0 { o.push_str(&format!(" parentId=\"{}\"", attr(&root_id))); } else if t.done { o.push_str(" done=\"1\""); }
            o.push_str(&format!("><text>{}</text>", text(&c.text)));
            let m = mentions(wb, &c.text);
            if !m.is_empty() {
                o.push_str("<mentions>");
                for (pid, start, len) in m { o.push_str(&format!("<mention mentionpersonId=\"{}\" mentionId=\"{}\" startIndex=\"{start}\" length=\"{len}\"/>", attr(&pid), guid())); }
                o.push_str("</mentions>");
            }
            o.push_str("</threadedComment>");
        }
    }
    Some(o + "</ThreadedComments>")
}

pub fn persons_xml(wb: &Workbook) -> Option<String> {
    if wb.review.persons.is_empty() || !wb.sheets.iter().any(|s| !s.review.threads.is_empty()) { return None; }
    let mut o = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<personList xmlns=\"{NS_TC}\" xmlns:x=\"{NS_MAIN}\">");
    for p in &wb.review.persons { o.push_str(&format!("<person displayName=\"{}\" id=\"{}\" userId=\"{}\" providerId=\"{}\"/>", attr(&p.name), attr(&p.id), attr(&p.user_id), attr(&p.provider))); }
    Some(o + "</personList>")
}

/// The note Excel writes for a thread, for readers without threaded comments.
pub fn thread_note(wb: &Workbook, t: &Thread) -> String {
    let mut s = String::from("[Threaded comment]\n\nYour version of Excel allows you to read this threaded comment; however, any edits to it will get removed if the file is opened in a newer version of Excel. Learn more: https://go.microsoft.com/fwlink/?linkid=870924\n\n");
    for (k, c) in t.comments.iter().enumerate() { s.push_str(if k == 0 { "Comment:\n    " } else { "\nReply:\n    " }); s.push_str(&c.text); let _ = wb.review.name_of(&c.person); }
    s
}

// ---------------------------------------------------------------------------------------------
// Custom views

impl CustomView {
    pub fn capture(wb: &Workbook, name: &str, hidden_rows: bool) -> CustomView {
        let sheets = wb.sheets.iter().map(|s| ViewSheet { zoom: s.zoom, gridlines: s.gridlines, headers: s.headers, selection: s.selection, frozen: (s.frozen_rows, s.frozen_cols), hidden: s.hidden, hidden_rows: if hidden_rows { s.hidden_rows.clone() } else { Default::default() }, hidden_cols: if hidden_rows { s.hidden_cols.clone() } else { Default::default() } }).collect();
        CustomView { name: name.into(), guid: guid(), active: wb.active, sheets, hidden_rows }
    }
    pub fn apply(&self, wb: &mut Workbook) {
        for (s, v) in wb.sheets.iter_mut().zip(&self.sheets) {
            s.zoom = v.zoom; s.gridlines = v.gridlines; s.headers = v.headers; s.selection = v.selection; s.frozen_rows = v.frozen.0; s.frozen_cols = v.frozen.1; s.hidden = v.hidden;
            if self.hidden_rows { s.hidden_rows = v.hidden_rows.clone(); s.hidden_cols = v.hidden_cols.clone(); }
        }
        if self.active < wb.sheets.len() && !wb.sheets[self.active].hidden { wb.active = self.active; }
    }
}

fn runs(set: &BTreeSet<u32>) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = vec![];
    for &i in set { match out.last_mut() { Some(l) if l.1 + 1 == i => l.1 = i, _ => out.push((i, i)) } }
    out
}

fn wvu_name(guid: &str, kind: &str) -> String { format!("Z_{}_.wvu.{kind}", guid.trim_matches(|c| c == '{' || c == '}').replace('-', "_")) }

pub fn custom_views_xml(wb: &Workbook) -> String {
    if wb.review.views.is_empty() { return String::new(); }
    let mut s = String::from("<customWorkbookViews>");
    for v in &wb.review.views {
        s.push_str(&format!("<customWorkbookView name=\"{}\" guid=\"{}\" includePrintSettings=\"0\" windowWidth=\"1280\" windowHeight=\"800\" activeSheetId=\"{}\"/>", attr(&v.name), attr(&v.guid), v.active + 1));
    }
    s + "</customWorkbookViews>"
}

/// A copy of the workbook with what the writer needs added: a note beside each thread, each
/// sheet's customSheetViews and the hidden names that record a view's hidden rows and columns.
pub fn stage(wb: &Workbook) -> Option<Workbook> {
    if wb.review.views.is_empty() && wb.sheets.iter().all(|s| s.review.threads.is_empty()) { return None; }
    let mut out = wb.clone();
    for si in 0..out.sheets.len() {
        let notes: Vec<(Pos, String)> = out.sheets[si].review.threads.iter().map(|(p, t)| (*p, thread_note(wb, t))).collect();
        for (p, n) in notes { out.sheets[si].notes.insert(p, n); }
        if wb.review.views.is_empty() { continue; }
        let mut x = String::from("<customSheetViews>");
        for v in &wb.review.views {
            let Some(vs) = v.sheets.get(si) else { continue };
            x.push_str(&format!("<customSheetView guid=\"{}\"", attr(&v.guid)));
            if vs.zoom != 100 { x.push_str(&format!(" scale=\"{}\"", vs.zoom)); }
            if !vs.gridlines { x.push_str(" showGridLines=\"0\""); }
            if !vs.headers { x.push_str(" showRowCol=\"0\""); }
            if !vs.hidden_rows.is_empty() { x.push_str(" hiddenRows=\"1\""); }
            if !vs.hidden_cols.is_empty() { x.push_str(" hiddenColumns=\"1\""); }
            if vs.hidden { x.push_str(" state=\"hidden\""); }
            x.push('>');
            if vs.frozen != (0, 0) {
                x.push_str("<pane");
                if vs.frozen.1 > 0 { x.push_str(&format!(" xSplit=\"{}\"", vs.frozen.1)); }
                if vs.frozen.0 > 0 { x.push_str(&format!(" ySplit=\"{}\"", vs.frozen.0)); }
                x.push_str(&format!(" topLeftCell=\"{}\" activePane=\"bottomRight\" state=\"frozen\"/>", Pos::new(vs.frozen.0, vs.frozen.1).a1()));
            }
            if let Some(r) = vs.selection { x.push_str(&format!("<selection activeCell=\"{}\" sqref=\"{}\"/>", r.start().a1(), r.a1())); }
            x.push_str("</customSheetView>");
            let quote = |n: &str| if n.chars().all(|c| c.is_alphanumeric() || c == '_') { n.to_string() } else { format!("'{}'", n.replace('\'', "''")) };
            let name = quote(&out.sheets[si].name);
            if v.hidden_rows {
                let rows: Vec<String> = runs(&vs.hidden_rows).iter().map(|(a, b)| format!("{name}!${}:${}", a + 1, b + 1)).collect();
                if !rows.is_empty() { out.names.push(DefinedName { name: wvu_name(&v.guid, "Rows"), sheet: Some(si), formula: rows.join(","), hidden: true, comment: String::new() }); }
                let cols: Vec<String> = runs(&vs.hidden_cols).iter().map(|(a, b)| format!("{name}!${}:${}", crate::addr::col_name(*a), crate::addr::col_name(*b))).collect();
                if !cols.is_empty() { out.names.push(DefinedName { name: wvu_name(&v.guid, "Cols"), sheet: Some(si), formula: cols.join(","), hidden: true, comment: String::new() }); }
            }
        }
        x.push_str("</customSheetViews>");
        set_extra(&mut out.sheets[si], "customSheetViews", Some(x));
    }
    Some(out)
}

// ---------------------------------------------------------------------------------------------
// Split panes

/// Reads a split (not frozen) pane: xSplit/ySplit are twentieths of a point.
pub fn read_split(view: &Element) -> Option<(f32, f32)> {
    let p = view.child("pane")?;
    if matches!(p.attr("state"), Some("frozen" | "frozenSplit")) { return None; }
    let (x, y) = (p.attr_f64("xSplit").unwrap_or(0.0), p.attr_f64("ySplit").unwrap_or(0.0));
    (x > 0.0 || y > 0.0).then(|| ((x / 15.0) as f32, (y / 15.0) as f32))
}

/// The pane element for a split sheet, and the pane its selection belongs to.
pub fn split_pane_xml(s: &Sheet) -> Option<(String, &'static str)> {
    if s.frozen_rows > 0 || s.frozen_cols > 0 { return None; }
    let (x, y) = s.review.split?;
    let pane = match (x > 0.0, y > 0.0) { (true, true) => "bottomRight", (false, true) => "bottomLeft", (true, false) => "topRight", _ => return None };
    let mut o = String::from("<pane");
    if x > 0.0 { o.push_str(&format!(" xSplit=\"{}\"", (x * 15.0).round())); }
    if y > 0.0 { o.push_str(&format!(" ySplit=\"{}\"", (y * 15.0).round())); }
    // The bottom-right pane starts at the cell under the bars.
    let col = { let mut acc = 0.0; let mut c = 0; while acc < x && c < 16_000 { acc += s.col_width(c); c += 1; } c };
    let row = { let mut acc = 0.0; let mut r = 0; while acc < y && r < 1_000_000 { acc += s.row_height(r); r += 1; } r };
    o.push_str(&format!(" topLeftCell=\"{}\" activePane=\"{pane}\"/>", Pos::new(row, col).a1()));
    Some((o, pane))
}

// ---------------------------------------------------------------------------------------------
// After reading

/// Fills the model from what the reader kept: protection and edit ranges from the sheets'
/// carried XML, workbook protection and custom views from the workbook part.
pub fn after_read(wb: &mut Workbook, wbx: &Element) {
    if let Some(p) = wbx.child("workbookProtection") {
        wb.review.structure = p.flag("lockStructure", false);
        wb.review.windows = p.flag("lockWindows", false);
        wb.review.structure_password = PwHash::read(p, "workbook");
    }
    let views: Vec<(String, String, usize)> = wbx.child("customWorkbookViews").map(|c| c.all("customWorkbookView").map(|v| (v.attr("name").unwrap_or("").to_string(), v.attr("guid").unwrap_or("").to_string(), v.attr_u32("activeSheetId").unwrap_or(1).saturating_sub(1) as usize)).collect()).unwrap_or_default();
    for (name, g, active) in views {
        let mut v = CustomView { name, guid: g.clone(), active, sheets: vec![], hidden_rows: false };
        for (si, s) in wb.sheets.iter().enumerate() {
            let mut vs = ViewSheet { zoom: 100, gridlines: true, headers: true, ..Default::default() };
            if let Some(e) = s.extra.iter().find(|(k, _)| k == "customSheetViews").and_then(|(_, x)| Element::parse(x).ok()) {
                if let Some(c) = e.all("customSheetView").find(|c| c.attr("guid") == Some(g.as_str())) {
                    vs.zoom = c.attr_u32("scale").unwrap_or(100); vs.gridlines = c.flag("showGridLines", true); vs.headers = c.flag("showRowCol", true);
                    vs.hidden = c.attr("state").is_some_and(|x| x != "visible");
                    if let Some(p) = c.child("pane").filter(|p| p.attr("state") == Some("frozen")) { vs.frozen = (p.attr_f64("ySplit").unwrap_or(0.0) as u32, p.attr_f64("xSplit").unwrap_or(0.0) as u32); }
                    vs.selection = c.child("selection").and_then(|x| x.attr("sqref")).and_then(|q| q.split_whitespace().next()).and_then(parse_range);
                }
            }
            for (kind, rows) in [("Rows", true), ("Cols", false)] {
                let n = wvu_name(&g, kind);
                if let Some(d) = wb.names.iter().find(|d| d.name.eq_ignore_ascii_case(&n) && d.sheet == Some(si)) {
                    v.hidden_rows = true;
                    for part in d.formula.trim_start_matches('=').split(',') {
                        let body = part.rsplit('!').next().unwrap_or("").replace('$', "");
                        let (a, b) = body.split_once(':').unwrap_or((&body, &body));
                        let conv = |t: &str| if rows { t.parse::<u32>().ok().map(|x| x.saturating_sub(1)) } else { crate::addr::parse_a1(&format!("{t}1")).map(|p| p.col) };
                        if let (Some(a), Some(b)) = (conv(a), conv(b)) { for i in a..=b.min(a + 1_048_576) { if rows { vs.hidden_rows.insert(i); } else { vs.hidden_cols.insert(i); } } }
                    }
                }
            }
            v.sheets.push(vs);
        }
        wb.review.views.push(v);
    }
    wb.names.retain(|d| !(d.name.starts_with("Z_") && d.name.contains("_.wvu.")));
    for s in &mut wb.sheets {
        if s.protected { if let Some((_, x)) = s.extra.iter().find(|(k, _)| k == "sheetProtection") { s.review.protection = SheetProtection::read(x); } }
        if let Some(e) = s.extra.iter().find(|(k, _)| k == "protectedRanges").and_then(|(_, x)| Element::parse(x).ok()) {
            s.review.edit_ranges = e.all("protectedRange").map(|r| EditRange { name: r.attr("name").unwrap_or("").into(), ranges: r.attr("sqref").unwrap_or("").split_whitespace().filter_map(parse_range).collect(), password: PwHash::read(r, "") }).collect();
        }
        if !wb.review.views.is_empty() { s.extra.retain(|(k, _)| k != "customSheetViews"); }
    }
}

// ---------------------------------------------------------------------------------------------
// Workbook statistics

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Statistics { pub sheet_end: Option<Pos>, pub cells: usize, pub formulas: usize, pub tables: usize, pub pivots: usize, pub objects: usize, pub notes: usize, pub threads: usize, pub sheets: usize, pub book_cells: usize, pub book_formulas: usize, pub book_tables: usize, pub book_notes: usize, pub book_threads: usize }

pub fn statistics(wb: &Workbook, si: usize) -> Statistics {
    let count = |s: &Sheet| (s.cells.values().filter(|c| !c.is_blank()).count(), s.cells.values().filter(|c| c.is_formula()).count());
    let s = &wb.sheets[si];
    let (cells, formulas) = count(s);
    let mut st = Statistics { sheet_end: s.extent(), cells, formulas, tables: s.tables.len(), pivots: s.pivots.len(), objects: s.objects.len(), notes: s.notes.len(), threads: s.review.threads.len(), sheets: wb.sheets.len(), ..Default::default() };
    for s in &wb.sheets { let (c, f) = count(s); st.book_cells += c; st.book_formulas += f; st.book_tables += s.tables.len(); st.book_notes += s.notes.len(); st.book_threads += s.review.threads.len(); }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excel_hash_vector() {
        // A known Excel sheetProtection: password "test".
        let salt = b64_decode("Fj0LUf6Kcg5bGZx2ss3l9g==").unwrap();
        let h = hash_password("test", &salt, 100_000);
        let p = PwHash { algorithm: "SHA-512".into(), hash: h.clone(), salt: "Fj0LUf6Kcg5bGZx2ss3l9g==".into(), spin: 100_000, legacy: None };
        assert!(p.matches("test"));
        assert!(!p.matches("Test"));
        assert_eq!(b64_decode(&h).unwrap().len(), 64);
        // The legacy 16-bit hash of "test" (as in old Excel files).
        assert_eq!(legacy_hash("test"), "CBEB");
        let l = PwHash { legacy: Some("CBEB".into()), ..Default::default() };
        assert!(l.matches("test") && !l.matches("x"));
    }

    #[test]
    fn hash_known_answer() {
        // SHA-512 spin with 0 rounds is SHA-512(salt || UTF-16LE(password)): "a" with empty salt.
        assert_eq!(hash_password("a", &[], 0), b64(&Sha512::digest([0x61u8, 0x00])));
    }

    #[test]
    fn protection_round_trip() {
        let mut wb = Workbook::new();
        let mut p = SheetProtection::default();
        p.allow[Action::FormatCells as usize] = true;
        p.password = PwHash::new("secret");
        protect_sheet(&mut wb.sheets[0], p.clone());
        set_edit_ranges(&mut wb.sheets[0], vec![EditRange { name: "Inputs".into(), ranges: vec![parse_range("B2:C5").unwrap()], password: None }]);
        protect_workbook(&mut wb, "book");
        let bytes = crate::xlsx::write(&wb).unwrap();
        let mut back = crate::xlsx::read(&bytes).unwrap();
        let s = &back.sheets[0];
        assert!(s.protected);
        assert_eq!(s.review.protection.allow, p.allow);
        assert!(s.review.protection.password.as_ref().unwrap().matches("secret"));
        assert_eq!(s.review.edit_ranges[0].ranges, vec![parse_range("B2:C5").unwrap()]);
        assert!(editable(&back, 0, Pos::new(2, 2), &[]).is_ok());
        assert!(editable(&back, 0, Pos::new(9, 9), &[]).is_err());
        assert!(back.review.structure);
        assert!(!unprotect_workbook(&mut back, "nope"));
        assert!(unprotect_workbook(&mut back, "book"));
        assert!(!unprotect_sheet(&mut back.sheets[0], "x"));
        assert!(unprotect_sheet(&mut back.sheets[0], "secret"));
    }

    #[test]
    fn threaded_comments_round_trip() {
        let mut wb = Workbook::new();
        wb.review.person("Bob");
        add_comment(&mut wb, 0, Pos::new(1, 1), "Ann", "Please check @Bob");
        add_comment(&mut wb, 0, Pos::new(1, 1), "Bob", "Done.");
        wb.sheets[0].review.threads.get_mut(&Pos::new(1, 1)).unwrap().done = true;
        wb.sheets[0].notes.insert(Pos::new(4, 0), "plain note".into());
        let bytes = crate::xlsx::write(&wb).unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        assert!(text.contains("threadedComments") && text.contains("persons/person.xml"));
        let back = crate::xlsx::read(&bytes).unwrap();
        let t = &back.sheets[0].review.threads[&Pos::new(1, 1)];
        assert_eq!(t.comments.len(), 2);
        assert!(t.done);
        assert_eq!(back.review.name_of(&t.comments[0].person), "Ann");
        assert_eq!(t.comments[1].text, "Done.");
        // The legacy copy isn't read back as a second note; the plain note stays.
        assert!(!back.sheets[0].notes.contains_key(&Pos::new(1, 1)));
        assert_eq!(back.sheets[0].notes[&Pos::new(4, 0)], "plain note");
        assert_eq!(mentions(&back, "Please check @Bob").len(), 1);
        // And again (ids are kept).
        let again = crate::xlsx::read(&crate::xlsx::write(&back).unwrap()).unwrap();
        assert_eq!(again.sheets[0].review.threads, back.sheets[0].review.threads);
    }

    #[test]
    fn custom_views_round_trip() {
        let mut wb = Workbook::new();
        wb.add_sheet(1, Some("Data".into()));
        wb.sheets[0].hidden_rows.insert(3); wb.sheets[0].hidden_rows.insert(4); wb.sheets[0].zoom = 150;
        let v = CustomView::capture(&wb, "Summary", true);
        wb.review.views.push(v);
        wb.sheets[0].hidden_rows.clear(); wb.sheets[0].zoom = 100;
        let mut back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        assert!(back.names.is_empty());
        assert_eq!(back.review.views.len(), 1);
        let v = back.review.views[0].clone();
        assert_eq!(v.name, "Summary");
        v.apply(&mut back);
        assert_eq!(back.sheets[0].zoom, 150);
        assert!(back.sheets[0].hidden_rows.contains(&3) && back.sheets[0].hidden_rows.contains(&4));
    }

    #[test]
    fn split_round_trip() {
        let mut wb = Workbook::new();
        wb.sheets[0].review.split = Some((192.0, 100.0));
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        let (x, y) = back.sheets[0].review.split.unwrap();
        assert!((x - 192.0).abs() < 0.5 && (y - 100.0).abs() < 0.5);
        assert_eq!(back.sheets[0].frozen_rows, 0);
    }
}
