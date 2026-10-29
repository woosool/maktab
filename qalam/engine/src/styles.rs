//! Paragraph and character styles with basedOn inheritance (styles.js).

use crate::format::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleKind { Paragraph, Character, Table, Numbering }

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub id: String,
    pub name: String,
    pub kind: StyleKind,
    pub based_on: Option<String>,
    pub next: Option<String>,
    pub para: ParaProps,
    pub run: RunProps,
    /// Table styles: whether any table border is declared.
    pub table_borders: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleSheet {
    pub doc_run: RunProps,
    pub doc_para: ParaProps,
    pub styles: HashMap<String, Style>,
    pub order: Vec<String>,
    pub default_paragraph: Option<String>,
    pub default_table: Option<String>,
}

impl Default for StyleSheet {
    /// BASIC_STYLES in docx.js: Noto Sans 12pt, 9pt after, Normal / Heading 1–2 / Emphasis / Strong.
    fn default() -> Self {
        let mut s = StyleSheet { doc_run: RunProps { font: Some("Noto Sans".into()), size: Some(12.0), ..Default::default() }, doc_para: ParaProps { space_after: Some(9.0), line_spacing: Some(1.0), line_rule: Some(LineRule::Auto), ..Default::default() }, styles: HashMap::new(), order: vec![], default_paragraph: Some("Normal".into()), default_table: None };
        s.add(Style { id: "Normal".into(), name: "Normal".into(), kind: StyleKind::Paragraph, based_on: None, next: None, para: ParaProps::default(), run: RunProps::default(), table_borders: false });
        for (level, size, before, after) in [(0u8, 20.0, 12.0, 6.0), (1, 16.0, 9.0, 4.5), (2, 13.0, 8.0, 4.0)] {
            s.add(Style {
                id: format!("Heading{}", level + 1), name: format!("Heading {}", level + 1), kind: StyleKind::Paragraph,
                based_on: Some("Normal".into()), next: Some("Normal".into()),
                para: ParaProps { keep_next: Some(true), keep_lines: Some(true), space_before: Some(before), space_after: Some(after), outline_level: Some(level), ..Default::default() },
                run: RunProps { bold: Some(true), size: Some(size), ..Default::default() }, table_borders: false,
            });
        }
        s.add(Style { id: "Title".into(), name: "Title".into(), kind: StyleKind::Paragraph, based_on: Some("Normal".into()), next: Some("Normal".into()), para: ParaProps { space_after: Some(12.0), ..Default::default() }, run: RunProps { size: Some(28.0), ..Default::default() }, table_borders: false });
        s.add(Style { id: "Emphasis".into(), name: "Emphasis".into(), kind: StyleKind::Character, based_on: None, next: None, para: ParaProps::default(), run: RunProps { italic: Some(true), ..Default::default() }, table_borders: false });
        s.add(Style { id: "Strong".into(), name: "Strong".into(), kind: StyleKind::Character, based_on: None, next: None, para: ParaProps::default(), run: RunProps { bold: Some(true), ..Default::default() }, table_borders: false });
        s.add(Style { id: "TableGrid".into(), name: "Table Grid".into(), kind: StyleKind::Table, based_on: None, next: None, para: ParaProps::default(), run: RunProps::default(), table_borders: true });
        s
    }
}

impl StyleSheet {
    pub fn empty() -> Self {
        StyleSheet { doc_run: RunProps::default(), doc_para: ParaProps::default(), styles: HashMap::new(), order: vec![], default_paragraph: None, default_table: None }
    }
    pub fn add(&mut self, style: Style) {
        if !self.styles.contains_key(&style.id) { self.order.push(style.id.clone()); }
        self.styles.insert(style.id.clone(), style);
    }
    pub fn get(&self, id: &str) -> Option<&Style> { self.styles.get(id) }

    /// The style chain from the root ancestor down to `id`, guarding against cycles.
    pub fn chain(&self, id: &str) -> Vec<&Style> {
        let mut out = vec![];
        let mut cur = self.styles.get(id);
        while let Some(s) = cur {
            if out.iter().any(|o: &&Style| o.id == s.id) || out.len() > 32 { break; }
            out.push(s);
            cur = s.based_on.as_deref().and_then(|b| self.styles.get(b));
        }
        out.reverse();
        out
    }

    pub fn paragraph_style_id<'a>(&'a self, id: Option<&'a str>) -> Option<&'a str> {
        id.filter(|i| self.styles.contains_key(*i)).or(self.default_paragraph.as_deref())
    }

    /// Declared paragraph properties: docDefaults ⊕ paragraph style chain.
    pub fn para_props(&self, style: Option<&str>) -> ParaProps {
        let mut p = self.doc_para.clone();
        if let Some(id) = self.paragraph_style_id(style) {
            for s in self.chain(id) { p.overlay(&s.para); }
        }
        p
    }

    /// Run properties for text in a paragraph of `para_style` with character style `char_style`.
    pub fn run_props(&self, para_style: Option<&str>, char_style: Option<&str>) -> RunProps {
        let mut r = self.doc_run.clone();
        if let Some(id) = self.paragraph_style_id(para_style) {
            for s in self.chain(id) { r.overlay(&s.run); }
        }
        if let Some(id) = char_style {
            for s in self.chain(id) {
                // ECMA-376 §17.7.3 toggle properties: a character style's bold/italic
                // flips the inherited value instead of replacing it.
                let mut run = s.run.clone();
                if s.kind == StyleKind::Character {
                    if run.bold == Some(true) && r.bold == Some(true) { run.bold = Some(false); }
                    if run.italic == Some(true) && r.italic == Some(true) { run.italic = Some(false); }
                }
                r.overlay(&run);
            }
        }
        r
    }

    pub fn paragraph_styles(&self) -> Vec<&Style> {
        self.order.iter().filter_map(|id| self.styles.get(id)).filter(|s| s.kind == StyleKind::Paragraph).collect()
    }
    pub fn character_styles(&self) -> Vec<&Style> {
        self.order.iter().filter_map(|id| self.styles.get(id)).filter(|s| s.kind == StyleKind::Character).collect()
    }
    /// Look a style up by id or display name (built-in names are case-insensitive).
    pub fn find(&self, id_or_name: &str) -> Option<&Style> {
        self.styles.get(id_or_name).or_else(|| self.styles.values().find(|s| s.name.eq_ignore_ascii_case(id_or_name)))
    }
}
