//! Mail merge (Mailings tab): recipient lists from CSV, tab-separated text or Excel, merge
//! fields, Address Block and Greeting Line built from matched columns, rules (If…Then…Else,
//! Next Record, Skip Record If, record and sequence numbers), previewing a record, checking for
//! errors, and finishing into one document with a section per recipient. Envelopes and labels
//! are created as new documents.

use crate::model::{Document, Field, Paragraph, Run};
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MergeKind { #[default] Normal, Letters, Email, Envelopes, Labels, Directory }

impl MergeKind {
    pub const ALL: [(MergeKind, &'static str); 6] = [(MergeKind::Letters, "Letters"), (MergeKind::Email, "E-mail Messages"), (MergeKind::Envelopes, "Envelopes…"), (MergeKind::Labels, "Labels…"), (MergeKind::Directory, "Directory"), (MergeKind::Normal, "Normal Word Document")];
}

/// How Greeting Line names the recipient.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NameFormat { #[default] FirstLast, TitleLast, TitleFirstLast, First, Full }

impl NameFormat {
    pub const ALL: [(NameFormat, &'static str); 5] = [(NameFormat::FirstLast, "Joshua Randall Jr."), (NameFormat::TitleLast, "Mr. Randall"), (NameFormat::TitleFirstLast, "Mr. Joshua Randall Jr."), (NameFormat::First, "Joshua"), (NameFormat::Full, "Joshua Randall Jr. (full name)")];
    fn code(&self) -> &'static str { match self { NameFormat::FirstLast => "<<_FIRST0_ _LAST0_>>", NameFormat::TitleLast => "<<_TITLE0_ _LAST0_>>", NameFormat::TitleFirstLast => "<<_TITLE0_ _FIRST0_ _LAST0_>>", NameFormat::First => "<<_FIRST0_>>", NameFormat::Full => "<<_FIRST0_ _LAST0_ _SUFFIX0_>>" } }
    fn from_code(c: &str) -> NameFormat {
        if c.contains("_TITLE0_ _FIRST0_") { NameFormat::TitleFirstLast } else if c.contains("_TITLE0_") { NameFormat::TitleLast } else if c.contains("_SUFFIX0_") { NameFormat::Full } else if c.contains("_LAST0_") { NameFormat::FirstLast } else { NameFormat::First }
    }
}

/// Comparisons in rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Compare { #[default] Equal, NotEqual, Less, Greater, LessEq, GreaterEq, IsBlank, IsNotBlank }

impl Compare {
    pub const ALL: [(Compare, &'static str); 8] = [(Compare::Equal, "Equal to"), (Compare::NotEqual, "Not equal to"), (Compare::Less, "Less than"), (Compare::Greater, "Greater than"), (Compare::LessEq, "Less than or equal"), (Compare::GreaterEq, "Greater than or equal"), (Compare::IsBlank, "Is blank"), (Compare::IsNotBlank, "Is not blank")];
    pub fn op(&self) -> &'static str { match self { Compare::Equal | Compare::IsBlank => "=", Compare::NotEqual | Compare::IsNotBlank => "<>", Compare::Less => "<", Compare::Greater => ">", Compare::LessEq => "<=", Compare::GreaterEq => ">=" } }
    pub fn from_op(op: &str, value: &str) -> Compare {
        match (op, value.is_empty()) { ("=", true) => Compare::IsBlank, ("<>", true) => Compare::IsNotBlank, ("=", _) => Compare::Equal, ("<>", _) => Compare::NotEqual, ("<", _) => Compare::Less, (">", _) => Compare::Greater, ("<=", _) => Compare::LessEq, (">=", _) => Compare::GreaterEq, _ => Compare::Equal }
    }
    /// Numbers compare as numbers, everything else as text (case-insensitive), as Word does.
    pub fn test(&self, a: &str, b: &str) -> bool {
        let ord = match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
            (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            _ => a.trim().to_lowercase().cmp(&b.trim().to_lowercase()),
        };
        use std::cmp::Ordering::*;
        match self { Compare::Equal => ord == Equal, Compare::NotEqual => ord != Equal, Compare::Less => ord == Less, Compare::Greater => ord == Greater, Compare::LessEq => ord != Greater, Compare::GreaterEq => ord != Less, Compare::IsBlank => a.trim().is_empty(), Compare::IsNotBlank => !a.trim().is_empty() }
    }
}

/// A recipient list: column names and rows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DataSource { pub name: String, pub headers: Vec<String>, pub rows: Vec<Vec<String>> }

impl DataSource {
    pub fn get(&self, row: usize, column: &str) -> Option<&str> {
        let c = self.headers.iter().position(|h| h.eq_ignore_ascii_case(column))?;
        self.rows.get(row).and_then(|r| r.get(c)).map(String::as_str)
    }

    /// Read CSV or tab-separated text (the delimiter is whichever the header line uses most).
    pub fn parse_delimited(name: &str, text: &str) -> Result<DataSource, String> {
        let text = text.trim_start_matches('\u{FEFF}');
        let first = text.lines().next().unwrap_or("");
        let delim = [',', '\t', ';'].into_iter().max_by_key(|d| first.matches(*d).count()).unwrap_or(',');
        let mut rows: Vec<Vec<String>> = vec![];
        let mut row = vec![];
        let mut cell = String::new();
        let mut quoted = false;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' if quoted && chars.peek() == Some(&'"') => { cell.push('"'); chars.next(); }
                '"' if cell.is_empty() && !quoted => quoted = true,
                '"' if quoted => quoted = false,
                c if c == delim && !quoted => row.push(std::mem::take(&mut cell)),
                '\r' if !quoted => {}
                '\n' if !quoted => { row.push(std::mem::take(&mut cell)); rows.push(std::mem::take(&mut row)); }
                c => cell.push(c),
            }
        }
        if !cell.is_empty() || !row.is_empty() { row.push(cell); rows.push(row); }
        rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
        if rows.is_empty() { return Err("The recipient list is empty.".into()); }
        let headers: Vec<String> = rows.remove(0).into_iter().map(|h| h.trim().to_string()).collect();
        let n = headers.len();
        for r in &mut rows { r.resize(n, String::new()); }
        Ok(DataSource { name: name.into(), headers, rows })
    }

    /// Read the first worksheet of an .xlsx workbook.
    pub fn read_xlsx(name: &str, bytes: &[u8]) -> Result<DataSource, String> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let mut read = |path: &str| -> Option<String> { use std::io::Read; let mut f = zip.by_name(path).ok()?; let mut s = String::new(); f.read_to_string(&mut s).ok()?; Some(s) };
        let shared: Vec<String> = read("xl/sharedStrings.xml").and_then(|t| crate::xml::parse(&t).ok()).map(|root| root.children_named("si").map(|si| {
            fn text(e: &crate::xml::Element, out: &mut String) { for c in e.elements() { if c.name == "t" { out.push_str(&c.text()); } else if c.name != "rPh" { text(c, out); } } }
            let mut s = String::new(); text(si, &mut s); s
        }).collect()).unwrap_or_default();
        let sheet = read("xl/worksheets/sheet1.xml").ok_or("The workbook has no first sheet.")?;
        let root = crate::xml::parse(&sheet).map_err(|e| format!("{e:?}"))?;
        let data = root.child("sheetData").ok_or("The sheet has no data.")?;
        let col_index = |r: &str| -> usize { r.chars().take_while(|c| c.is_ascii_alphabetic()).fold(0usize, |a, c| a * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1)).saturating_sub(1) };
        let mut rows: Vec<Vec<String>> = vec![];
        for row in data.children_named("row") {
            let mut out: Vec<String> = vec![];
            for (k, c) in row.children_named("c").enumerate() {
                let col = c.attr("r").map(col_index).unwrap_or(k);
                let v = c.child("v").map(|v| v.text()).unwrap_or_default();
                let value = match c.attr("t") {
                    Some("s") => v.parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default(),
                    Some("inlineStr") => c.child("is").and_then(|i| i.child("t")).map(|t| t.text()).unwrap_or_default(),
                    _ => v,
                };
                if out.len() <= col { out.resize(col + 1, String::new()); }
                out[col] = value;
            }
            rows.push(out);
        }
        rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
        if rows.is_empty() { return Err("The first sheet is empty.".into()); }
        let headers: Vec<String> = rows.remove(0).into_iter().map(|h| h.trim().to_string()).collect();
        let n = headers.len();
        for r in &mut rows { r.resize(n, String::new()); }
        Ok(DataSource { name: name.into(), headers, rows })
    }

    /// The list as CSV (Type a New List saves this way).
    pub fn to_csv(&self) -> String {
        let q = |s: &str| if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
        let mut out = self.headers.iter().map(|h| q(h)).collect::<Vec<_>>().join(",") + "\n";
        for r in &self.rows { out += &(r.iter().map(|c| q(c)).collect::<Vec<_>>().join(",") + "\n"); }
        out
    }
}

/// The address parts Address Block and Greeting Line use, matched to recipient columns.
pub const ADDRESS_FIELDS: &[(&str, &[&str])] = &[
    ("Courtesy Title", &["title", "courtesy title", "salutation", "mr/ms"]),
    ("First Name", &["first name", "firstname", "first", "given name"]),
    ("Last Name", &["last name", "lastname", "last", "surname", "family name"]),
    ("Suffix", &["suffix"]),
    ("Company", &["company", "company name", "organization", "organisation"]),
    ("Address 1", &["address 1", "address1", "address line 1", "address", "street"]),
    ("Address 2", &["address 2", "address2", "address line 2"]),
    ("City", &["city", "town"]),
    ("State", &["state", "province", "region"]),
    ("Postal Code", &["postal code", "zip", "zip code", "postcode"]),
    ("Country or Region", &["country", "country or region", "country/region"]),
    ("E-mail Address", &["e-mail address", "email", "e-mail", "email address"]),
];

/// The document's mail merge state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MailMerge {
    pub kind: MergeKind,
    pub source: Option<DataSource>,
    /// Where the list came from (shown in the UI; not saved in the document).
    pub path: Option<String>,
    /// Rows left out in Edit Recipient List.
    pub excluded: BTreeSet<usize>,
    /// Match Fields: address part → column.
    pub map: HashMap<String, String>,
    /// Preview Results shows this included recipient (0-based) instead of «field» names.
    pub preview: bool,
    pub record: usize,
    pub highlight: bool,
}

impl MailMerge {
    /// Included row indices in order.
    pub fn included(&self) -> Vec<usize> {
        self.source.as_ref().map(|s| (0..s.rows.len()).filter(|r| !self.excluded.contains(r)).collect()).unwrap_or_default()
    }

    /// The column an address part reads: the matched one, else a column whose name fits.
    pub fn column_for(&self, part: &str) -> Option<String> {
        let src = self.source.as_ref()?;
        if let Some(c) = self.map.get(part) { return (!c.is_empty()).then(|| c.clone()); }
        let names = ADDRESS_FIELDS.iter().find(|f| f.0 == part).map(|f| f.1).unwrap_or(&[]);
        src.headers.iter().find(|h| names.iter().any(|n| h.trim().eq_ignore_ascii_case(n)) || h.trim().eq_ignore_ascii_case(part)).cloned()
    }

    fn part(&self, row: usize, part: &str) -> String {
        self.column_for(part).and_then(|c| self.source.as_ref()?.get(row, &c).map(|v| v.trim().to_string())).unwrap_or_default()
    }

    /// A recipient's address block lines.
    pub fn address(&self, row: usize) -> String {
        let p = |n: &str| self.part(row, n);
        let name = [p("Courtesy Title"), p("First Name"), p("Last Name"), p("Suffix")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
        let city_line = {
            let mut s = p("City");
            let (st, zip) = (p("State"), p("Postal Code"));
            if !st.is_empty() { if !s.is_empty() { s += ", "; } s += &st; }
            if !zip.is_empty() { if !s.is_empty() { s += " "; } s += &zip; }
            s
        };
        [name, p("Company"), p("Address 1"), p("Address 2"), city_line, p("Country or Region")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n")
    }

    pub fn greeting(&self, row: Option<usize>, greeting: &str, name: NameFormat, punct: &str) -> String {
        let Some(row) = row else { return format!("{greeting} Sir or Madam{punct}") };
        let p = |n: &str| self.part(row, n);
        let parts: Vec<String> = match name {
            NameFormat::FirstLast => vec![p("First Name"), p("Last Name")],
            NameFormat::TitleLast => vec![p("Courtesy Title"), p("Last Name")],
            NameFormat::TitleFirstLast => vec![p("Courtesy Title"), p("First Name"), p("Last Name")],
            NameFormat::First => vec![p("First Name")],
            NameFormat::Full => vec![p("First Name"), p("Last Name"), p("Suffix")],
        };
        let n = parts.into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
        if n.is_empty() { format!("{greeting} Sir or Madam{punct}") } else { format!("{greeting} {n}{punct}") }
    }

    fn value(&self, row: Option<usize>, column: &str) -> Option<String> {
        row.and_then(|r| self.source.as_ref()?.get(r, column).map(String::from))
    }

    /// What a merge field shows: the previewed record's value, or «Name» placeholders.
    pub fn field_text(&self, f: &Field) -> String {
        let row = if self.preview { self.included().get(self.record).copied() } else { None };
        self.text_for(f, row, self.record + 1)
    }

    pub fn text_for(&self, f: &Field, row: Option<usize>, seq: usize) -> String {
        let live = row.is_some();
        match f {
            Field::Merge(name) => if live { self.value(row, name).unwrap_or_default() } else { format!("«{name}»") },
            Field::AddressBlock => if live { self.address(row.unwrap()) } else { "«AddressBlock»".into() },
            Field::GreetingLine { greeting, name, punctuation } => if live { self.greeting(row, greeting, *name, punctuation) } else { "«GreetingLine»".into() },
            Field::MergeIf { field, op, value, then, otherwise } => {
                if !live { return String::new(); }
                if op.test(&self.value(row, field).unwrap_or_default(), value) { then.clone() } else { otherwise.clone() }
            }
            Field::MergeRec => if live { (row.unwrap() + 1).to_string() } else { "«Rec»".into() },
            Field::MergeSeq => if live { seq.to_string() } else { "«Seq»".into() },
            Field::NextRecord => if live { String::new() } else { "«Next Record»".into() },
            Field::NextIf { .. } => if live { String::new() } else { "«Next Record If»".into() },
            Field::SkipIf { .. } => if live { String::new() } else { "«Skip Record If»".into() },
            _ => String::new(),
        }
    }
}

pub fn is_merge_field(f: &Field) -> bool {
    matches!(f, Field::Merge(_) | Field::AddressBlock | Field::GreetingLine { .. } | Field::MergeIf { .. } | Field::MergeRec | Field::MergeSeq | Field::NextRecord | Field::NextIf { .. } | Field::SkipIf { .. })
}

/// A field's instruction text as written to .docx (IF is written with a nested MERGEFIELD).
pub fn instruction(f: &Field) -> Option<String> {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "'"));
    Some(match f {
        Field::Merge(n) => format!(" MERGEFIELD {} ", if n.contains(' ') { q(n) } else { n.clone() }),
        Field::AddressBlock => " ADDRESSBLOCK \\f \"<<_TITLE0_ >><<_FIRST0_>><< _LAST0_>><< _SUFFIX0_>>\n<<_COMPANY_\n>><<_STREET1_\n>><<_STREET2_\n>><<_CITY_>><<, _STATE_>><< _POSTAL_>><<\n_COUNTRY_>>\" \\l 1033 \\c 2 \\e \"United States\" \\d ".into(),
        Field::GreetingLine { greeting, name, punctuation } => format!(" GREETINGLINE \\f \"<<_GREETING_ {}{}>>\" \\l 1033 \\e \"{} Sir or Madam{}\" ", name.code(), punctuation, greeting, punctuation).replacen("_GREETING_", &format!("{greeting} _GREETING_"), 0),
        Field::MergeRec => " MERGEREC ".into(),
        Field::MergeSeq => " MERGESEQ ".into(),
        Field::NextRecord => " NEXT ".into(),
        Field::NextIf { field, op, value } => format!(" NEXTIF {{MERGEFIELD {field}}} {} {} ", op.op(), q(value)),
        Field::SkipIf { field, op, value } => format!(" SKIPIF {{MERGEFIELD {field}}} {} {} ", op.op(), q(value)),
        Field::MergeIf { field, op, value, then, otherwise } => format!(" IF {{MERGEFIELD {field}}} {} {} {} {} ", op.op(), q(value), q(then), q(otherwise)),
        _ => return None,
    })
}

/// Split an instruction into words, keeping "quoted strings" and {nested fields} whole.
fn tokens(instr: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let (mut quoted, mut depth) = (false, 0);
    for c in instr.chars() {
        match c {
            '"' if depth == 0 => { quoted = !quoted; if !quoted { out.push(std::mem::take(&mut cur)); } }
            '{' if !quoted => { depth += 1; cur.push(c); }
            '}' if !quoted => { depth -= 1; cur.push(c); if depth == 0 { out.push(std::mem::take(&mut cur)); } }
            c if c.is_whitespace() && !quoted && depth == 0 => if !cur.is_empty() { out.push(std::mem::take(&mut cur)); },
            c => cur.push(c),
        }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

fn field_name(tok: &str) -> String {
    let t = tok.trim_matches(|c| c == '{' || c == '}').trim();
    t.strip_prefix("MERGEFIELD").map(|r| r.trim().trim_matches('"').split(" \\").next().unwrap_or("").trim().to_string()).unwrap_or_else(|| t.trim_matches('"').to_string())
}

/// Parse a merge field instruction (the reader passes nested fields as `{MERGEFIELD X}`).
pub fn parse(instr: &str) -> Option<Field> {
    let t = tokens(instr);
    let word = t.first()?.to_uppercase();
    let cond = |k: usize| -> Option<(String, Compare, String)> {
        let field = field_name(t.get(k)?);
        let op = t.get(k + 1).map(String::as_str).unwrap_or("=");
        let value = t.get(k + 2).cloned().unwrap_or_default();
        Some((field, Compare::from_op(op, &value), value))
    };
    Some(match word.as_str() {
        "MERGEFIELD" => Field::Merge(field_name(&format!("MERGEFIELD {}", t.get(1)?))),
        "ADDRESSBLOCK" => Field::AddressBlock,
        "GREETINGLINE" => {
            let f = t.iter().position(|x| x == "\\f").and_then(|k| t.get(k + 1)).cloned().unwrap_or_default();
            let e = t.iter().position(|x| x == "\\e").and_then(|k| t.get(k + 1)).cloned().unwrap_or_else(|| "Dear Sir or Madam,".into());
            let greeting = e.split_whitespace().next().unwrap_or("Dear").to_string();
            let punctuation = e.chars().last().filter(|c| !c.is_alphanumeric()).map(String::from).unwrap_or_default();
            Field::GreetingLine { greeting, name: NameFormat::from_code(&f), punctuation }
        }
        "MERGEREC" => Field::MergeRec,
        "MERGESEQ" => Field::MergeSeq,
        "NEXT" => Field::NextRecord,
        "NEXTIF" => { let (field, op, value) = cond(1)?; Field::NextIf { field, op, value } }
        "SKIPIF" => { let (field, op, value) = cond(1)?; Field::SkipIf { field, op, value } }
        "IF" if t.get(1).is_some_and(|x| x.contains("MERGEFIELD")) => {
            let (field, op, value) = cond(1)?;
            Field::MergeIf { field, op, value, then: t.get(4).cloned().unwrap_or_default(), otherwise: t.get(5).cloned().unwrap_or_default() }
        }
        _ => return None,
    })
}

impl Document {
    pub fn set_merge_source(&mut self, source: Option<DataSource>, path: Option<String>) {
        let mm = &mut self.mail_merge;
        mm.source = source;
        mm.path = path;
        mm.excluded.clear();
        mm.record = 0;
        if mm.kind == MergeKind::Normal { mm.kind = MergeKind::Letters; }
        self.format_epoch += 1;
        self.revision += 1;
    }

    /// Change preview/highlight/record: these only change what fields show.
    pub fn merge_view(&mut self, f: impl FnOnce(&mut MailMerge)) {
        f(&mut self.mail_merge);
        let n = self.mail_merge.included().len();
        if self.mail_merge.record >= n { self.mail_merge.record = n.saturating_sub(1); }
        self.format_epoch += 1;
        self.revision += 1;
    }

    /// Fields that name columns the recipient list doesn't have.
    pub fn merge_errors(&self) -> Vec<String> {
        let Some(src) = &self.mail_merge.source else { return vec!["No recipient list is selected.".into()] };
        let mut out = vec![];
        for r in self.paragraphs.iter().flat_map(|p| p.runs.iter()) {
            let name = match &r.field { Some(Field::Merge(n)) => n, Some(Field::MergeIf { field, .. } | Field::NextIf { field, .. } | Field::SkipIf { field, .. }) => field, _ => continue };
            if !src.headers.iter().any(|h| h.eq_ignore_ascii_case(name)) { let m = format!("The field «{name}» is not in the recipient list."); if !out.contains(&m) { out.push(m); } }
        }
        out
    }

    pub fn has_merge_fields(&self) -> bool { self.paragraphs.iter().flat_map(|p| p.runs.iter()).any(|r| r.field.as_ref().is_some_and(is_merge_field)) }

    /// Replace merge fields with their values, starting at recipient `order[start]`. Returns how
    /// many recipients the copy used, or None when a Skip Record If rule skips it.
    fn freeze(&mut self, mm: &MailMerge, order: &[usize], start: usize, seq: usize) -> Option<usize> {
        let mut cur = start;
        for p in &mut self.paragraphs {
            for r in &mut p.runs {
                let Some(f) = r.field.clone().filter(is_merge_field) else { continue };
                let row = order.get(cur).copied();
                match &f {
                    Field::NextRecord => cur += 1,
                    Field::NextIf { field, op, value } => if op.test(&mm.value(row, field).unwrap_or_default(), value) { cur += 1 },
                    Field::SkipIf { field, op, value } => if op.test(&mm.value(row, field).unwrap_or_default(), value) { return None },
                    _ => {}
                }
                let text = if row.is_some() || !matches!(f, Field::Merge(_)) { mm.text_for(&f, row, seq) } else { String::new() };
                r.field = None;
                r.text = text.replace('\n', &crate::chars::LINE_BREAK.to_string());
            }
            p.runs.retain(|r| !r.text.is_empty() || r.marker.is_some() || r.field.is_some());
        }
        Some(cur - start + 1)
    }

    /// Finish & Merge ▸ Edit Individual Documents: one copy per recipient (labels use several
    /// recipients per page), each starting on a new page.
    pub fn merged_document(&self, from: usize, to: usize) -> Result<Document, String> {
        let mm = self.mail_merge.clone();
        let src = mm.source.as_ref().ok_or("Select a recipient list first.")?;
        let order: Vec<usize> = mm.included().into_iter().filter(|r| *r >= from && *r <= to.min(src.rows.len().saturating_sub(1))).collect();
        if order.is_empty() { return Err("No recipients are included.".into()); }
        let template = crate::docx::write(self)?;
        let mut out: Option<Document> = None;
        let (mut at, mut seq) = (0, 1);
        while at < order.len() {
            let mut copy = crate::docx::read(&template)?;
            match copy.freeze(&mm, &order, at, seq) {
                None => { at += 1; continue; }
                Some(used) => { at += used; seq += 1; }
            }
            match &mut out {
                None => out = Some(copy),
                Some(d) => d.append_document(copy),
            }
        }
        let mut d = out.ok_or("Every recipient was skipped.")?;
        d.mail_merge = MailMerge::default();
        d.dirty = true;
        Ok(d)
    }

    /// Append another document's body on a new page (its tables and notes come along).
    pub fn append_document(&mut self, mut other: Document) {
        if let Some(first) = other.paragraphs.first_mut() {
            if first.cell.is_none() { first.props.page_break_before = Some(true); }
            else { let mut p = Paragraph::new(""); p.runs.push(Run::text(crate::chars::PAGE_BREAK.to_string())); self.paragraphs.push(p); }
        }
        self.paragraphs.append(&mut other.paragraphs);
        self.tables.append(&mut other.tables);
        self.notes.append(&mut other.notes);
        self.sort_tables();
        self.format_epoch += 1;
        self.changed();
    }

    /// Update Labels: copy the first label's content into every other label, after a Next
    /// Record field (labels are the cells of the document's first table).
    pub fn update_labels(&mut self) {
        let Some(t) = self.tables.first() else { return };
        let ids: Vec<u64> = t.cells().map(|c| c.id).collect();
        let Some(&first) = ids.first() else { return };
        self.checkpoint(crate::model::EditKind::Other);
        let template: Vec<Paragraph> = self.paragraphs.iter().filter(|p| p.cell == Some(first)).cloned().collect();
        for &c in &ids[1..] {
            let at = self.paragraphs.iter().position(|p| p.cell == Some(c));
            self.paragraphs.retain(|p| p.cell != Some(c));
            let Some(at) = at else { continue };
            let mut copy: Vec<Paragraph> = template.iter().map(|p| { let mut q = p.clone(); q.id = crate::model::new_id(); q.cell = Some(c); q }).collect();
            if let Some(p0) = copy.first_mut() { p0.runs.insert(0, Run { text: " ".into(), field: Some(Field::NextRecord), ..Default::default() }); }
            let at = at.min(self.paragraphs.len());
            self.paragraphs.splice(at..at, copy);
        }
        self.changed();
    }
}

/// Envelope sizes (name, width, height in twips).
pub const ENVELOPES: &[(&str, i32, i32)] = &[("Size 10 (4 1/8 × 9 1/2 in)", 13680, 5940), ("DL (110 × 220 mm)", 12474, 6237), ("C5 (162 × 229 mm)", 12984, 9185), ("C6 (114 × 162 mm)", 9185, 6464), ("Monarch (3 7/8 × 7 1/2 in)", 10800, 5580)];

/// Label sheets: name, columns, rows, label width, height, top margin, left margin,
/// horizontal pitch (twips).
pub const LABELS: &[(&str, u8, u8, i32, i32, i32, i32, i32)] = &[
    ("Avery 5160 / 8160 Address (30 per sheet)", 3, 10, 3780, 1440, 720, 270, 3960),
    ("Avery 5163 Shipping (10 per sheet)", 2, 5, 5760, 2880, 720, 225, 6030),
    ("Avery 5162 Address (14 per sheet)", 2, 7, 5760, 1920, 1260, 225, 6030),
    ("Avery 5167 Return Address (80 per sheet)", 4, 20, 2520, 720, 720, 432, 2952),
    ("A4 21 labels (63.5 × 38.1 mm)", 3, 7, 3600, 2160, 1101, 340, 3600),
];

impl Document {
    /// A new document set up as an envelope: return address top left, delivery address in the
    /// middle.
    pub fn envelope(size: usize, delivery: &str, ret: &str) -> Document {
        let (_, w, h) = ENVELOPES.get(size).copied().unwrap_or(ENVELOPES[0]);
        let mut paras: Vec<Paragraph> = ret.lines().map(|l| { let mut p = Paragraph::new(l); p.props.space_after = Some(0.0); p }).collect();
        let lead = (h / 2 - 720 - ret.lines().count() as i32 * 280).max(0);
        for (k, l) in delivery.lines().enumerate() {
            let mut p = Paragraph::new(l);
            p.props.indent_left = Some(w / 2 - 1440);
            p.props.space_after = Some(0.0);
            if k == 0 { p.props.space_before = Some(lead as f32 / 20.0); }
            paras.push(p);
        }
        if paras.is_empty() { paras.push(Paragraph::new("")); }
        let mut d = Document::new(paras);
        d.page.width = w; d.page.height = h; d.page.landscape = true;
        d.page.top = 360; d.page.left = 360; d.page.right = 360; d.page.bottom = 360;
        d.mail_merge.kind = MergeKind::Envelopes;
        d
    }

    /// A new document of labels: a table the size of the sheet, each label holding `text` (or
    /// an empty label to merge into).
    pub fn labels(spec: usize, text: &str) -> Document {
        let (_, cols, rows, w, h, top, left, pitch) = LABELS.get(spec).copied().unwrap_or(LABELS[0]);
        let mut d = Document::new(vec![Paragraph::new("")]);
        d.page.top = top; d.page.left = left; d.page.right = 0; d.page.bottom = 0;
        d.select(0, 0);
        d.insert_table(rows as usize, cols as usize);
        let gap = pitch - w;
        if let Some(t) = d.tables.first_mut() {
            t.borders = false;
            t.style = None;
            t.fixed = true;
            t.cell_margins = [0, 86, 0, 86];
            t.grid = (0..cols).map(|k| if k + 1 < cols { w + gap } else { w }).collect();
            for r in &mut t.rows { r.min_height = h; r.exact = true; r.cant_split = true; }
            t.sync_cell_widths();
        }
        if !text.is_empty() {
            let ids: Vec<u64> = d.tables[0].cells().map(|c| c.id).collect();
            for p in d.paragraphs.iter_mut().filter(|p| p.cell.is_some_and(|c| ids.contains(&c))) { p.runs = vec![Run::text(text.replace('\n', &crate::chars::LINE_BREAK.to_string()))]; }
        }
        d.mail_merge.kind = MergeKind::Labels;
        d.dirty = true;
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> DataSource { DataSource::parse_delimited("people.csv", "First Name,Last Name,City,Amount\nAda,Lovelace,London,10\n\"Grace\",\"Hopper, Jr\",Arlington,5\nAlan,Turing,Manchester,0\n").unwrap() }

    #[test]
    fn csv_fields_preview_rules_and_merge() {
        let s = list();
        assert_eq!(s.headers, vec!["First Name", "Last Name", "City", "Amount"]);
        assert_eq!(s.rows[1][1], "Hopper, Jr");
        let mut d = Document::new(vec![Paragraph::new("")]);
        d.set_merge_source(Some(s), None);
        d.insert_field(Field::GreetingLine { greeting: "Dear".into(), name: NameFormat::FirstLast, punctuation: ",".into() });
        d.replace(" from ");
        d.insert_field(Field::Merge("City".into()));
        d.insert_field(Field::SkipIf { field: "Amount".into(), op: Compare::Equal, value: "0".into() });
        d.insert_field(Field::MergeIf { field: "Amount".into(), op: Compare::Greater, value: "6".into(), then: " (gold)".into(), otherwise: String::new() });
        assert_eq!(d.mail_merge.field_text(&Field::Merge("City".into())), "«City»");
        d.merge_view(|m| { m.preview = true; m.record = 1; });
        assert_eq!(d.mail_merge.field_text(&Field::Merge("City".into())), "Arlington");
        assert!(d.merge_errors().is_empty());
        // Round trip keeps the fields.
        let e = crate::docx::read(&crate::docx::write(&d).unwrap()).unwrap();
        let kinds: Vec<Field> = e.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| r.field.clone()).collect();
        assert_eq!(kinds.len(), 4, "{kinds:?}");
        assert!(matches!(&kinds[3], Field::MergeIf { field, then, .. } if field == "Amount" && then == " (gold)"));
        // Merge: Turing is skipped (Amount = 0).
        let m = d.merged_document(0, usize::MAX).unwrap();
        let text = m.text();
        assert!(text.contains("Dear Ada Lovelace, from London (gold)") && text.contains("Dear Grace Hopper, Jr, from Arlington") && !text.contains("Turing"), "{text}");
        assert_eq!(m.paragraphs.iter().filter(|p| p.props.page_break_before == Some(true)).count(), 1);
    }

    #[test]
    fn labels_and_envelopes() {
        let mut d = Document::labels(0, "");
        assert_eq!(d.tables[0].cells().count(), 30);
        d.set_merge_source(Some(list()), None);
        let first = d.tables[0].rows[0].cells[0].id;
        let p = d.paragraph_start(d.paragraphs.iter().position(|p| p.cell == Some(first)).unwrap());
        d.select(p, p);
        d.insert_field(Field::AddressBlock);
        d.update_labels();
        let m = d.merged_document(0, usize::MAX).unwrap();
        let t = m.text();
        assert!(t.contains("Ada Lovelace") && t.contains("Grace Hopper") && t.contains("Alan Turing"), "{t}");
        let e = Document::envelope(0, "Ada Lovelace\n1 Analytical Way", "Me\nHome");
        assert!(e.page.width > e.page.height);
        assert!(e.text().contains("Analytical"));
    }
}
