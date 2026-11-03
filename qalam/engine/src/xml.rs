//! A small XML DOM over quick-xml (docx-xml.js). Element names are rewritten
//! to canonical prefixes for known OOXML namespaces so lookups like `w:p`
//! work whatever prefixes a producer chose; preserved fragments are written
//! with the same canonical prefixes.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashMap;

pub const NAMESPACES: &[(&str, &str)] = &[
    ("w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main"),
    ("r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships"),
    ("wp", "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"),
    ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    ("pic", "http://schemas.openxmlformats.org/drawingml/2006/picture"),
    ("mc", "http://schemas.openxmlformats.org/markup-compatibility/2006"),
    ("w14", "http://schemas.microsoft.com/office/word/2010/wordml"),
    ("wp14", "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing"),
    ("v", "urn:schemas-microsoft-com:vml"),
    ("o", "urn:schemas-microsoft-com:office:office"),
    ("m", "http://schemas.openxmlformats.org/officeDocument/2006/math"),
    ("wps", "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"),
    ("wpg", "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"),
    ("w15", "http://schemas.microsoft.com/office/word/2012/wordml"),
];

#[derive(Clone, Debug, PartialEq)]
pub enum Node { Element(Element), Text(String) }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn new(name: &str) -> Element { Element { name: name.into(), ..Default::default() } }
    pub fn attr(&self, name: &str) -> Option<&str> { self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str()) }
    pub fn set_attr(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.attrs.iter_mut().find(|(k, _)| k == name) { Some(a) => a.1 = value, None => self.attrs.push((name.into(), value)) }
    }
    pub fn with_attr(mut self, name: &str, value: impl Into<String>) -> Element { self.set_attr(name, value); self }
    pub fn with_child(mut self, child: Element) -> Element { self.children.push(Node::Element(child)); self }
    pub fn push(&mut self, child: Element) { self.children.push(Node::Element(child)); }
    pub fn elements(&self) -> impl Iterator<Item = &Element> { self.children.iter().filter_map(|n| if let Node::Element(e) = n { Some(e) } else { None }) }
    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> { self.children.iter_mut().filter_map(|n| if let Node::Element(e) = n { Some(e) } else { None }) }
    pub fn child(&self, name: &str) -> Option<&Element> { self.elements().find(|e| e.name == name) }
    pub fn child_mut(&mut self, name: &str) -> Option<&mut Element> { self.elements_mut().find(|e| e.name == name) }
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a { self.elements().filter(move |e| e.name == name) }
    /// Depth-first search for the first descendant with a name.
    pub fn find(&self, name: &str) -> Option<&Element> {
        for e in self.elements() { if e.name == name { return Some(e); } if let Some(f) = e.find(name) { return Some(f); } }
        None
    }
    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a Element>) {
        for e in self.elements() { if e.name == name { out.push(e); } e.find_all(name, out); }
    }
    pub fn text(&self) -> String {
        let mut s = String::new();
        for c in &self.children { match c { Node::Text(t) => s.push_str(t), Node::Element(e) => s.push_str(&e.text()) } }
        s
    }
    /// `w:val` of a child, e.g. `<w:jc w:val="center"/>`.
    pub fn val(&self, child: &str) -> Option<&str> { self.child(child).and_then(|c| c.attr("w:val")) }
    /// OOXML on/off (§17.17.4): present without val, or val true/1/on.
    pub fn flag(&self, child: &str) -> Option<bool> {
        self.child(child).map(|c| !matches!(c.attr("w:val"), Some("0" | "false" | "off" | "none")))
    }
    /// Get or create a child, keeping it in place if it exists.
    pub fn ensure(&mut self, name: &str) -> &mut Element {
        if self.child(name).is_none() { self.push(Element::new(name)); }
        self.child_mut(name).unwrap()
    }
    pub fn remove(&mut self, name: &str) { self.children.retain(|c| !matches!(c, Node::Element(e) if e.name == name)); }

    pub fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (k, v) in &self.attrs { out.push(' '); out.push_str(k); out.push_str("=\""); escape_into(v, out, true); out.push('"'); }
        if self.children.is_empty() { out.push_str("/>"); return; }
        out.push('>');
        for c in &self.children { match c { Node::Text(t) => escape_into(t, out, false), Node::Element(e) => e.write(out) } }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }
    pub fn to_xml(&self) -> String { let mut s = String::new(); self.write(&mut s); s }
}

pub fn escape_into(s: &str, out: &mut String, attr: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            // Attribute values keep line breaks and tabs only as character references.
            '\n' if attr => out.push_str("&#10;"),
            '\t' if attr => out.push_str("&#9;"),
            '\r' if attr => out.push_str("&#13;"),
            // XML 1.0 forbids most control characters.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
}

struct NsScope { prefixes: HashMap<String, String> }

fn canonical(uri: &str) -> Option<&'static str> { NAMESPACES.iter().find(|(_, u)| *u == uri).map(|(p, _)| *p) }

fn rename(name: &str, scopes: &[NsScope]) -> String {
    let (prefix, local) = match name.split_once(':') { Some((p, l)) => (p, l), None => ("", name) };
    if prefix == "xmlns" || name == "xmlns" || prefix == "xml" { return name.to_string(); }
    for s in scopes.iter().rev() {
        if let Some(uri) = s.prefixes.get(prefix) {
            return match canonical(uri) { Some(c) => format!("{c}:{local}"), None => name.to_string() };
        }
    }
    name.to_string()
}

fn start_element(e: &BytesStart, scopes: &mut Vec<NsScope>) -> Result<Element, String> {
    let mut prefixes = HashMap::new();
    let mut raw_attrs = vec![];
    for a in e.attributes().with_checks(false) {
        let a = a.map_err(|e| e.to_string())?;
        let key = String::from_utf8_lossy(a.key.as_ref()).to_string();
        let value = a.unescape_value().map(|v| v.to_string()).unwrap_or_else(|_| String::from_utf8_lossy(&a.value).to_string());
        if key == "xmlns" { prefixes.insert(String::new(), value.clone()); }
        else if let Some(p) = key.strip_prefix("xmlns:") { prefixes.insert(p.to_string(), value.clone()); }
        raw_attrs.push((key, value));
    }
    scopes.push(NsScope { prefixes });
    let name = rename(&String::from_utf8_lossy(e.name().as_ref()), scopes);
    let attrs = raw_attrs.into_iter().map(|(k, v)| {
        if k.starts_with("xmlns") {
            // Rewrite declarations of known namespaces to the canonical prefix.
            if let Some(p) = k.strip_prefix("xmlns:") { if let Some(c) = canonical(&v) { if c != p { return (format!("xmlns:{c}"), v); } } }
            (k, v)
        } else if k.contains(':') { (rename(&k, scopes), v) } else { (k, v) }
    }).collect();
    Ok(Element { name, attrs, children: vec![] })
}

pub fn parse(xml: &str) -> Result<Element, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = vec![Element::new("#document")];
    let mut scopes: Vec<NsScope> = vec![];
    loop {
        match reader.read_event().map_err(|e| format!("XML error at {}: {e}", reader.buffer_position()))? {
            Event::Start(e) => { let el = start_element(&e, &mut scopes)?; stack.push(el); }
            Event::Empty(e) => { let el = start_element(&e, &mut scopes)?; scopes.pop(); stack.last_mut().unwrap().push(el); }
            Event::End(_) => {
                scopes.pop();
                let el = stack.pop().ok_or("unbalanced XML")?;
                stack.last_mut().ok_or("unbalanced XML")?.push(el);
            }
            Event::Text(t) => {
                let s = t.unescape().map(|v| v.to_string()).unwrap_or_else(|_| String::from_utf8_lossy(&t).to_string());
                if !s.is_empty() { stack.last_mut().unwrap().children.push(Node::Text(s)); }
            }
            Event::CData(t) => stack.last_mut().unwrap().children.push(Node::Text(String::from_utf8_lossy(&t).to_string())),
            Event::Eof => break,
            _ => {}
        }
    }
    let doc = stack.pop().ok_or("empty XML")?;
    doc.elements().next().cloned().ok_or_else(|| "XML has no root element".to_string())
}

pub const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_prefixes() {
        let x = parse(r#"<x:document xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><x:body><x:p><x:r><x:t xml:space="preserve"> a &amp; b</x:t></x:r></x:p></x:body></x:document>"#).unwrap();
        assert_eq!(x.name, "w:document");
        assert_eq!(x.find("w:t").unwrap().text(), " a & b");
        assert!(x.to_xml().contains("xmlns:w="));
    }
}
