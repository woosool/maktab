//! Sections (Layout ▸ Breaks, Columns, Line Numbers) and document-wide layout settings
//! (hyphenation, footnote and endnote numbering).

use crate::model::{Document, EditKind};
use crate::page::{Columns, Hyphenation, LineNumbering, Section, SectionKind};

impl Document {
    /// The section paragraph `para` belongs to (a section-ending paragraph belongs to its own).
    pub fn section_index(&self, para: usize) -> usize { self.paragraphs[..para.min(self.paragraphs.len())].iter().filter(|p| p.section.is_some()).count() }
    pub fn section_count(&self) -> usize { self.paragraphs.iter().filter(|p| p.section.is_some()).count() + 1 }
    /// The caret's section.
    pub fn current_section(&self) -> usize { self.section_index(self.locate(self.range().0).0) }

    pub fn section_at(&self, k: usize) -> &Section {
        self.paragraphs.iter().filter_map(|p| p.section.as_ref()).nth(k).unwrap_or(&self.page.section)
    }
    pub fn section_at_mut(&mut self, k: usize) -> &mut Section {
        let idx = self.paragraphs.iter().enumerate().filter(|(_, p)| p.section.is_some()).map(|(i, _)| i).nth(k);
        match idx { Some(i) => self.paragraphs[i].section.as_mut().unwrap(), None => &mut self.page.section }
    }

    /// Edit the caret's section as one undo step.
    pub fn edit_section(&mut self, f: impl FnOnce(&mut Section)) {
        self.checkpoint(EditKind::Other);
        let k = self.current_section();
        f(self.section_at_mut(k));
        self.format_epoch += 1;
        self.changed();
    }

    /// Columns for the caret's section.
    pub fn set_columns(&mut self, columns: Columns) { self.edit_section(|s| s.columns = columns); }

    /// Line numbering for the caret's section (None turns it off).
    pub fn set_line_numbers(&mut self, ln: Option<LineNumbering>) { self.edit_section(|s| s.line_numbers = ln); }

    /// Insert a section break at the caret: the text before it ends a section with the current
    /// settings, and what follows starts as `kind` says.
    pub fn insert_section_break(&mut self, kind: SectionKind) {
        let k = self.current_section();
        let current = self.section_at(k).clone();
        let (i, _) = self.locate(self.range().0);
        if self.paragraphs[i].cell.is_some() { return; }
        let ended_here = self.paragraphs[i].section.take();
        self.insert_paragraph();
        let (j, _) = self.locate(self.range().0);
        let first = j.saturating_sub(1);
        self.paragraphs[first].section = Some(current);
        match ended_here {
            Some(mut s) => { s.kind = kind; self.paragraphs[j].section = Some(s); }
            None => self.section_at_mut(k + 1).kind = kind,
        }
        self.format_epoch += 1;
        self.changed();
    }

    /// Text columns for a new section starting at the caret ("This point forward").
    pub fn columns_from_here(&mut self, columns: Columns) {
        self.insert_section_break(SectionKind::Continuous);
        let k = self.current_section();
        self.section_at_mut(k).columns = columns;
        self.format_epoch += 1;
        self.changed();
    }

    /// Apply columns to every section ("Whole document").
    pub fn set_columns_everywhere(&mut self, columns: Columns) {
        self.checkpoint(EditKind::Other);
        for p in &mut self.paragraphs { if let Some(s) = &mut p.section { s.columns = columns.clone(); } }
        self.page.section.columns = columns;
        self.format_epoch += 1;
        self.changed();
    }

    pub fn insert_column_break(&mut self) { self.replace(&crate::chars::COLUMN_BREAK.to_string()); }

    pub fn set_hyphenation(&mut self, h: Hyphenation) {
        self.checkpoint(EditKind::Other);
        self.page.hyphenation = h;
        self.format_epoch += 1;
        self.changed();
    }

    /// Footnote (or endnote) number format and first number.
    pub fn set_note_format(&mut self, foot: bool, fmt: crate::numbering::NumFmt, start: u32) {
        self.checkpoint(EditKind::Other);
        if foot { self.page.footnote_format = (fmt, start.max(1)); } else { self.page.endnote_format = (fmt, start.max(1)); }
        self.format_epoch += 1;
        self.changed();
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Document, Paragraph};
    use crate::page::{Columns, Hyphenation, LineNumbering, LineRestart, SectionKind};

    #[test]
    fn columns_sections_line_numbers_and_hyphenation() {
        let words = "Columns flow from one to the next before the page ends. ".repeat(60);
        let mut d = Document::new(vec![Paragraph::new("Title"), Paragraph::new(&words), Paragraph::new(&words), Paragraph::new("After")]);
        // Two columns from the second paragraph on (a continuous break), then a new page.
        let p1 = d.paragraph_start(1);
        d.select(p1, p1);
        d.columns_from_here(Columns { count: 2, separator: true, ..Default::default() });
        let p3 = d.paragraph_start(d.paragraphs.len() - 1);
        d.select(p3, p3);
        d.insert_section_break(SectionKind::NextPage);
        let k = d.section_count();
        assert_eq!(k, 3, "three sections");
        let last = d.paragraphs.len() - 1;
        d.select(d.paragraph_start(last), d.paragraph_start(last));
        d.set_columns(Columns::default());
        let g = d.page.geometry();
        let l = crate::layout_document(&d);
        let col_lines: Vec<&crate::layout::Line> = l.lines.iter().filter(|x| x.paragraph == 2 || x.paragraph == 3).collect();
        let right = col_lines.iter().filter(|x| x.stops[0] > g.left + (g.width - g.left - g.right) / 2.0).count();
        assert!(right > 5, "text flows into the second column");
        let last_line = l.lines.last().unwrap();
        assert!(last_line.page > col_lines.last().unwrap().page || last_line.y < col_lines.iter().map(|x| x.y).fold(f32::MAX, f32::min), "the next-page section starts on a new page");
        assert!(l.pages[0].items.iter().any(|i| matches!(i, crate::layout::DrawItem::Line { x1, x2, .. } if (x1 - x2).abs() < 0.1 && *x1 > g.left + 100.0)), "a line between the columns");
        // Line numbers and hyphenation.
        d.set_line_numbers(Some(LineNumbering { start: 1, count_by: 5, restart: LineRestart::Page, distance: 0 }));
        d.set_hyphenation(Hyphenation { auto: true, ..Default::default() });
        let l = crate::layout_document(&d);
        let glyph_text: String = l.pages[0].items.iter().filter_map(|i| match i { crate::layout::DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| g.ch).collect::<String>() + " "), _ => None }).collect();
        assert!(glyph_text.contains("- "), "a hyphenated line ends with a hyphen");
        // Everything round-trips.
        let e = crate::docx::read(&crate::docx::write(&d).unwrap()).unwrap();
        assert_eq!(e.section_count(), 3);
        assert_eq!(e.section_at(1).columns.count, 2);
        assert!(e.section_at(1).columns.separator);
        assert_eq!(e.section_at(1).kind, SectionKind::Continuous);
        assert_eq!(e.section_at(2).kind, SectionKind::NextPage);
        assert!(e.page.hyphenation.auto);
        assert_eq!(e.page.section.line_numbers.map(|l| (l.count_by, l.restart)), Some((5, LineRestart::Page)));
    }
}
