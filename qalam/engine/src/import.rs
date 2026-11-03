//! Plain-text and Markdown import (importers in the JS app).

use crate::format::*;
use crate::model::{Document, Paragraph, Run};
use crate::numbering::ListKind;

/// One paragraph per line.
pub fn from_text(text: &str) -> Document {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    Document::new(text.split('\n').map(Paragraph::new).collect())
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
            if let (Some(close), Some(open)) = (rest.find("]("), rest.find(')')) {
                if open > close {
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
pub fn to_text(doc: &Document) -> String { doc.text().replace(crate::chars::OBJECT, "") }

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
            if let Some(l) = &r.link { t = format!("[{t}]({l})"); }
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
    fn markdown_round_trip() {
        let d = from_markdown("# Title\n\nSome **bold** and *it* [link](https://x.y)\n\n- one\n- two\n");
        assert_eq!(d.paragraphs.len(), 4);
        assert_eq!(d.paragraphs[0].style.as_deref(), Some("Heading1"));
        assert!(d.run_format(&d.paragraphs[1], &d.paragraphs[1].runs[1]).bold);
        let md = to_markdown(&d);
        assert!(md.contains("# Title") && md.contains("**bold**") && md.contains("- two"));
    }
}
