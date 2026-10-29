//! A small XML tree for the package's little parts (workbook, rels, styles, comments, theme) and
//! the escaping both directions share. Worksheet cell data never goes through it: it streams.
//! Lookups go by local name, so prefixed (x:row) and strict-namespace files read the same.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Element {
    /// The qualified name as written ("x:sheet").
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node { Element(Element), Text(String) }

/// Resolves the five entities and character references.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') { return s.to_string(); }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';') else { out.push_str(tail); return out };
        let ent = &tail[1..end];
        let ch = match ent {
            "amp" => Some('&'), "lt" => Some('<'), "gt" => Some('>'), "quot" => Some('"'), "apos" => Some('\''),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch { Some(c) => out.push(c), None => out.push_str(&tail[..=end]) }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn xml_char(c: char) -> bool { matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{FFFE}' && c != '\u{FFFF}') }

/// Escapes text for an attribute value (characters XML can't hold are dropped).
pub fn attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c { '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"), '\r' => out.push_str("&#13;"), '\t' => out.push_str("&#9;"), c if xml_char(c) => out.push(c), _ => {} }
    }
    out
}

/// Escapes element text; control characters become _xHHHH_ (and a literal _xHHHH_ protects its
/// underscore as _x005F_), as SpreadsheetML spells them.
pub fn text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let cs: Vec<char> = s.chars().collect();
    for (i, &c) in cs.iter().enumerate() {
        match c {
            '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), '\r' => out.push_str("&#13;"),
            '_' if is_xescape(&cs[i..]) => out.push_str("_x005F_"),
            c if xml_char(c) => out.push(c),
            c => out.push_str(&format!("_x{:04X}_", c as u32)),
        }
    }
    out
}

fn is_xescape(c: &[char]) -> bool { c.len() >= 7 && c[1] == 'x' && c[2..6].iter().all(|h| h.is_ascii_hexdigit()) && c[6] == '_' }

/// Decodes _xHHHH_ escapes in cell text.
pub fn decode_x(s: &str) -> String {
    if !s.contains("_x") { return s.to_string(); }
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '_' && is_xescape(&cs[i..]) {
            let h: String = cs[i + 2..i + 6].iter().collect();
            if let Some(c) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) { out.push(c); i += 7; continue; }
        }
        out.push(cs[i]); i += 1;
    }
    out
}

fn start(e: &BytesStart) -> Element {
    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let attrs = e.attributes().with_checks(false).flatten()
        .map(|a| (String::from_utf8_lossy(a.key.as_ref()).into_owned(), unescape(&String::from_utf8_lossy(&a.value)))).collect();
    Element { name, attrs, children: Vec::new() }
}

/// The text of a part, BOM and all, as UTF-8 (UTF-16 parts are decoded too).
pub fn part_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let u: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return String::from_utf16_lossy(&u);
    }
    String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned()
}

impl Element {
    pub fn parse(src: &str) -> Result<Element, String> {
        let mut reader = Reader::from_str(src);
        let mut stack: Vec<Element> = Vec::new();
        loop {
            match reader.read_event().map_err(|e| format!("XML error at {}: {e}", reader.buffer_position()))? {
                Event::Start(e) => stack.push(start(&e)),
                Event::Empty(e) => { let el = start(&e); match stack.last_mut() { Some(t) => t.children.push(Node::Element(el)), None => return Ok(el) } }
                Event::End(_) => {
                    let el = stack.pop().ok_or("unbalanced end tag")?;
                    match stack.last_mut() { Some(t) => t.children.push(Node::Element(el)), None => return Ok(el) }
                }
                Event::Text(t) => if let Some(top) = stack.last_mut() {
                    let s = unescape(&String::from_utf8_lossy(&t));
                    if let Some(Node::Text(p)) = top.children.last_mut() { p.push_str(&s); } else { top.children.push(Node::Text(s)); }
                }
                Event::CData(c) => if let Some(top) = stack.last_mut() { top.children.push(Node::Text(String::from_utf8_lossy(&c).into_owned())); }
                Event::Eof => return Err("no root element".into()),
                _ => {}
            }
        }
    }
    pub fn to_xml(&self) -> String { let mut s = String::new(); self.write(&mut s); s }
    fn write(&self, out: &mut String) {
        out.push('<'); out.push_str(&self.name);
        for (k, v) in &self.attrs { out.push_str(&format!(" {k}=\"{}\"", attr(v))); }
        if self.children.is_empty() { out.push_str("/>"); return; }
        out.push('>');
        for c in &self.children { match c { Node::Element(e) => e.write(out), Node::Text(t) => out.push_str(&attr_text(t)) } }
        out.push_str(&format!("</{}>", self.name));
    }
    pub fn local(&self) -> &str { local(&self.name) }
    /// An attribute by local name (r:id and id both match "id" only when asked as written).
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).or_else(|| self.attrs.iter().find(|(a, _)| !k.contains(':') && local(a) == k && !a.starts_with("xmlns"))).map(|(_, v)| v.as_str())
    }
    pub fn attr_f64(&self, k: &str) -> Option<f64> { self.attr(k).and_then(|v| v.trim().parse().ok()) }
    pub fn attr_u32(&self, k: &str) -> Option<u32> { self.attr(k).and_then(|v| v.trim().parse().ok()) }
    /// A boolean attribute; `default` when absent.
    pub fn flag(&self, k: &str, default: bool) -> bool { self.attr(k).map_or(default, |v| v == "1" || v.eq_ignore_ascii_case("true") || v == "on") }
    pub fn set_attr(&mut self, k: &str, v: impl Into<String>) {
        let v = v.into();
        if let Some(a) = self.attrs.iter_mut().find(|(a, _)| a == k) { a.1 = v; } else { self.attrs.push((k.to_string(), v)); }
    }
    pub fn elements(&self) -> impl Iterator<Item = &Element> { self.children.iter().filter_map(|n| if let Node::Element(e) = n { Some(e) } else { None }) }
    pub fn child(&self, l: &str) -> Option<&Element> { self.elements().find(|e| e.local() == l) }
    pub fn all<'a>(&'a self, l: &'a str) -> impl Iterator<Item = &'a Element> + 'a { self.elements().filter(move |e| e.local() == l) }
    pub fn path(&self, p: &str) -> Option<&Element> { p.split('/').try_fold(self, |e, n| e.child(n)) }
    /// All text inside, in order.
    pub fn text(&self) -> String {
        let mut s = String::new();
        fn walk(e: &Element, s: &mut String) { for c in &e.children { match c { Node::Text(t) => s.push_str(t), Node::Element(e) => walk(e, s) } } }
        walk(self, &mut s);
        s
    }
}

fn attr_text(t: &str) -> String { t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

pub fn local(name: &str) -> &str { name.rsplit(':').next().unwrap_or(name) }

/// Rich or plain cell text (<si>, <is>, a comment's <text>): the <t> runs, phonetic runs skipped.
pub fn rich_text(e: &Element) -> String {
    let mut s = String::new();
    fn walk(e: &Element, s: &mut String) {
        for c in e.elements() {
            match c.local() { "t" => s.push_str(&c.text()), "rPh" | "phoneticPr" | "rPr" => {}, _ => walk(c, s) }
        }
    }
    walk(e, &mut s);
    decode_x(&s)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escapes() {
        assert_eq!(text("a\u{1}b_x0041_"), "a_x0001_b_x005F_x0041_");
        assert_eq!(decode_x(&"a_x0001_b_x005F_x0041_".to_string()), "a\u{1}b_x0041_");
        let e = Element::parse("<x:a xmlns:x='u'><x:b r:id='r1'>1 &amp; 2</x:b></x:a>").unwrap();
        assert_eq!(e.child("b").unwrap().text(), "1 & 2");
        assert_eq!(e.child("b").unwrap().attr("id"), Some("r1"));
    }
}
