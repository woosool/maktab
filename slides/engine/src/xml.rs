//! A small XML tree that keeps everything it doesn't understand: names keep their prefixes,
//! attributes keep their order, and comments, processing instructions and text survive a
//! parse → serialize round trip, so a .pptx part PowerPoint wrote comes back the same.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
    /// Comments, processing instructions, CDATA and doctype, kept verbatim.
    Raw(String),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Element {
    /// The qualified name as written ("p:sp").
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Document {
    /// The XML declaration, if any (written back as is).
    pub decl: Option<String>,
    /// Comments or processing instructions before the root.
    pub prolog: Vec<Node>,
    pub root: Element,
}

fn unescape(s: &str) -> String {
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

fn escape(s: &str, attr: bool, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            '\n' if attr => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' if attr => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
}

fn start(e: &BytesStart) -> Result<Element, String> {
    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let mut attrs = Vec::new();
    for a in e.attributes().with_checks(false) {
        let a = a.map_err(|x| x.to_string())?;
        let k = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let v = unescape(&String::from_utf8_lossy(&a.value));
        attrs.push((k, v));
    }
    Ok(Element { name, attrs, children: Vec::new() })
}

impl Document {
    pub fn parse(bytes: &[u8]) -> Result<Document, String> {
        let text = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).map_err(|e| e.to_string())?;
        let mut reader = Reader::from_str(text);
        reader.config_mut().trim_text(false);
        let mut doc = Document::default();
        let mut stack: Vec<Element> = Vec::new();
        let mut root: Option<Element> = None;
        let push = |stack: &mut Vec<Element>, prolog: &mut Vec<Node>, n: Node| {
            if let Some(top) = stack.last_mut() { top.children.push(n); } else if !matches!(&n, Node::Text(t) if t.trim().is_empty()) { prolog.push(n); }
        };
        loop {
            match reader.read_event().map_err(|e| format!("XML error at {}: {e}", reader.buffer_position()))? {
                Event::Decl(d) => doc.decl = Some(format!("<?{}?>", String::from_utf8_lossy(&d))),
                Event::Start(e) => stack.push(start(&e)?),
                Event::Empty(e) => {
                    let el = start(&e)?;
                    if stack.is_empty() { root = Some(el); } else { stack.last_mut().unwrap().children.push(Node::Element(el)); }
                }
                Event::End(_) => {
                    let el = stack.pop().ok_or("unbalanced end tag")?;
                    if let Some(top) = stack.last_mut() { top.children.push(Node::Element(el)); } else { root = Some(el); }
                }
                Event::Text(t) => {
                    let s = unescape(&String::from_utf8_lossy(&t));
                    if let Some(top) = stack.last_mut() {
                        // Adjacent text (split around entities) is one node.
                        if let Some(Node::Text(prev)) = top.children.last_mut() { prev.push_str(&s); } else { top.children.push(Node::Text(s)); }
                    }
                }
                Event::CData(c) => {
                    let s = String::from_utf8_lossy(&c).into_owned();
                    if let Some(top) = stack.last_mut() { top.children.push(Node::Text(s)); }
                }
                Event::Comment(c) => push(&mut stack, &mut doc.prolog, Node::Raw(format!("<!--{}-->", String::from_utf8_lossy(&c)))),
                Event::PI(p) => push(&mut stack, &mut doc.prolog, Node::Raw(format!("<?{}?>", String::from_utf8_lossy(&p)))),
                Event::DocType(d) => push(&mut stack, &mut doc.prolog, Node::Raw(format!("<!DOCTYPE {}>", String::from_utf8_lossy(&d)))),
                Event::Eof => break,
            }
        }
        doc.root = root.ok_or("no root element")?;
        Ok(doc)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(self.decl.as_deref().unwrap_or(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#));
        out.push_str("\r\n");
        for n in &self.prolog { write_node(n, &mut out); }
        write_element(&self.root, &mut out);
        out.into_bytes()
    }

    pub fn new(root: Element) -> Document { Document { decl: None, prolog: vec![], root } }
}

fn write_node(n: &Node, out: &mut String) {
    match n {
        Node::Element(e) => write_element(e, out),
        Node::Text(t) => escape(t, false, out),
        Node::Raw(r) => out.push_str(r),
    }
}

fn write_element(e: &Element, out: &mut String) {
    out.push('<');
    out.push_str(&e.name);
    for (k, v) in &e.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape(v, true, out);
        out.push('"');
    }
    if e.children.is_empty() { out.push_str("/>"); return; }
    out.push('>');
    for c in &e.children { write_node(c, out); }
    out.push_str("</");
    out.push_str(&e.name);
    out.push('>');
}

impl Element {
    pub fn new(name: &str) -> Element { Element { name: name.to_string(), attrs: vec![], children: vec![] } }
    pub fn with_attr(mut self, k: &str, v: impl Into<String>) -> Element { self.set_attr(k, v); self }
    pub fn with_child(mut self, c: Element) -> Element { self.children.push(Node::Element(c)); self }
    pub fn with_text(mut self, t: impl Into<String>) -> Element { self.children.push(Node::Text(t.into())); self }

    /// Parse an element from a fragment (namespace prefixes are taken as written).
    pub fn parse(xml: &str) -> Result<Element, String> { Document::parse(xml.as_bytes()).map(|d| d.root) }
    pub fn to_xml(&self) -> String { let mut s = String::new(); write_element(self, &mut s); s }

    /// The name without its prefix ("sp" for "p:sp").
    pub fn local(&self) -> &str { self.name.rsplit(':').next().unwrap_or(&self.name) }
    pub fn is(&self, name: &str) -> bool { self.name == name }

    pub fn attr(&self, k: &str) -> Option<&str> { self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str()) }
    pub fn attr_i64(&self, k: &str) -> Option<i64> { self.attr(k).and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v.round() as i64) }
    pub fn attr_f64(&self, k: &str) -> Option<f64> { self.attr(k).and_then(|v| v.trim().parse().ok()) }
    pub fn attr_bool(&self, k: &str) -> Option<bool> { self.attr(k).map(|v| v == "1" || v == "true" || v == "on") }
    pub fn set_attr(&mut self, k: &str, v: impl Into<String>) {
        let v = v.into();
        if let Some(a) = self.attrs.iter_mut().find(|(a, _)| a == k) { a.1 = v; } else { self.attrs.push((k.to_string(), v)); }
    }
    pub fn remove_attr(&mut self, k: &str) { self.attrs.retain(|(a, _)| a != k); }

    pub fn elements(&self) -> impl Iterator<Item = &Element> { self.children.iter().filter_map(|n| if let Node::Element(e) = n { Some(e) } else { None }) }
    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> { self.children.iter_mut().filter_map(|n| if let Node::Element(e) = n { Some(e) } else { None }) }
    pub fn child(&self, name: &str) -> Option<&Element> { self.elements().find(|e| e.name == name) }
    pub fn child_mut(&mut self, name: &str) -> Option<&mut Element> { self.elements_mut().find(|e| e.name == name) }
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a { self.elements().filter(move |e| e.name == name) }
    /// Follow a path of child names ("p:cSld/p:spTree").
    pub fn path(&self, path: &str) -> Option<&Element> { path.split('/').try_fold(self, |e, n| e.child(n)) }
    pub fn path_mut(&mut self, path: &str) -> Option<&mut Element> { path.split('/').try_fold(self, |e, n| e.child_mut(n)) }
    /// The child, created (appended) if missing.
    pub fn ensure(&mut self, name: &str) -> &mut Element {
        if self.child(name).is_none() { self.children.push(Node::Element(Element::new(name))); }
        self.child_mut(name).unwrap()
    }
    /// The child, created at `index` among the element children if missing (schema order).
    pub fn ensure_at(&mut self, name: &str, before: &[&str]) -> &mut Element {
        if self.child(name).is_none() {
            let pos = self.children.iter().position(|n| matches!(n, Node::Element(e) if before.contains(&e.name.as_str()))).unwrap_or(self.children.len());
            self.children.insert(pos, Node::Element(Element::new(name)));
        }
        self.child_mut(name).unwrap()
    }
    pub fn remove_children(&mut self, name: &str) { self.children.retain(|n| !matches!(n, Node::Element(e) if e.name == name)); }
    pub fn first_element(&self) -> Option<&Element> { self.elements().next() }

    /// All text inside, in order.
    pub fn text(&self) -> String {
        let mut s = String::new();
        fn walk(e: &Element, s: &mut String) { for c in &e.children { match c { Node::Text(t) => s.push_str(t), Node::Element(e) => walk(e, s), Node::Raw(_) => {} } } }
        walk(self, &mut s);
        s
    }
    pub fn set_text(&mut self, t: impl Into<String>) { self.children = vec![Node::Text(t.into())]; }

    /// Depth-first search for the first descendant with this name.
    pub fn find(&self, name: &str) -> Option<&Element> {
        for e in self.elements() { if e.name == name { return Some(e); } if let Some(f) = e.find(name) { return Some(f); } }
        None
    }
    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a Element>) {
        for e in self.elements() { if e.name == name { out.push(e); } e.find_all(name, out); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_everything() {
        let src = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<p:sld xmlns:p=\"urn:p\" xmlns:a=\"urn:a\"><!-- c --><p:cSld name=\"A &amp; B\"><a:t>x &lt; y</a:t><a:t xml:space=\"preserve\"> two </a:t><a:br/></p:cSld></p:sld>";
        let d = Document::parse(src.as_bytes()).unwrap();
        assert_eq!(String::from_utf8(d.to_bytes()).unwrap(), src);
        assert_eq!(d.root.path("p:cSld").unwrap().attr("name"), Some("A & B"));
        assert_eq!(d.root.path("p:cSld/a:t").unwrap().text(), "x < y");
    }
}
