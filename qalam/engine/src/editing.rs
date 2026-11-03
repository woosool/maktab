//! Home-tab editing commands: bullet/numbering libraries and multilevel lists (list-library.js),
//! Change Case (capitalization.js), paragraph sort, table move/alignment/size, Undo to a step.

use crate::format::*;
use crate::model::*;
use crate::numbering::{Level, NumFmt, Script};

/// Bullet characters offered in the Bullet Library.
pub const BULLET_LIBRARY: &[char] = &['\u{2022}', '\u{25E6}', '\u{25AA}', '\u{25C6}', '\u{2713}', '\u{27A2}', '\u{2013}', '\u{2605}'];

/// Numbering Library entries: (format, pattern for %1).
pub const NUMBER_LIBRARY: &[(NumFmt, &str)] = &[
    (NumFmt::Decimal, "%1."), (NumFmt::Decimal, "%1)"), (NumFmt::Decimal, "(%1)"), (NumFmt::UpperRoman, "%1."),
    (NumFmt::UpperLetter, "%1."), (NumFmt::LowerLetter, "%1)"), (NumFmt::LowerLetter, "%1."), (NumFmt::LowerRoman, "%1."),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Multilevel { Numbered, Legal, Bullets, Outline }

pub const MULTILEVEL_LIBRARY: &[(Multilevel, &str)] = &[
    (Multilevel::Numbered, "1. a. i."), (Multilevel::Legal, "1. 1.1. 1.1.1."), (Multilevel::Bullets, "● ○ ■"), (Multilevel::Outline, "I. A. 1. a)"),
];

/// Cover page designs (cover-pages.js).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverDesign { Banded, Simple, Sideline, Bold }

pub const COVER_DESIGNS: &[(CoverDesign, &str)] = &[(CoverDesign::Banded, "Banded"), (CoverDesign::Simple, "Simple"), (CoverDesign::Sideline, "Sideline"), (CoverDesign::Bold, "Bold")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseMode { Sentence, Lower, Upper, Capitalize, Toggle }

fn level(i: usize, fmt: NumFmt, text: String) -> Level {
    Level { fmt, text, start: 1, indent_left: 720 * (i as i32 + 1), hanging: 360, font: None }
}

/// The nine levels of a list whose first level is `fmt`/`pattern`. Deeper levels follow Word's
/// cycle in the paragraph's script: 1. a. i. — or 1. أ. أ. (abjad) in Arabic, 1. א. א. in Hebrew.
fn levels_for(first: Option<(NumFmt, String)>, bullet: Option<char>, script: Script) -> Vec<Level> {
    let cycle = match script {
        Script::Latin => [NumFmt::Decimal, NumFmt::LowerLetter, NumFmt::LowerRoman],
        Script::Arabic => [NumFmt::Decimal, NumFmt::ArabicAlpha, NumFmt::ArabicAbjad],
        Script::Hebrew => [NumFmt::Decimal, NumFmt::Hebrew2, NumFmt::Hebrew1],
    };
    (0..9).map(|i| match (bullet, &first) {
        (Some(b), _) => level(i, NumFmt::Bullet, if i == 0 { b.to_string() } else { ['\u{25E6}', '\u{25AA}', '\u{2022}'][i % 3].to_string() }),
        (None, Some((fmt, pattern))) if i == 0 => level(0, *fmt, pattern.clone()),
        _ => level(i, cycle[i % 3], format!("%{}.", i + 1)),
    }).collect()
}

/// Numbering Library entries for right-to-left paragraphs.
pub const RTL_NUMBER_LIBRARY: &[(NumFmt, &str)] = &[
    (NumFmt::Decimal, "%1."), (NumFmt::HindiNumbers, "%1."), (NumFmt::ArabicAlpha, "%1."), (NumFmt::ArabicAbjad, "%1-"),
    (NumFmt::HindiNumbers, "(%1)"), (NumFmt::ArabicAlpha, "%1)"), (NumFmt::Hebrew1, "%1."), (NumFmt::Hebrew2, "%1."),
];

/// A user-defined list (Define New Number Format / Bullet / Multilevel List), kept by name.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomList { pub name: String, pub levels: Vec<Level> }

impl CustomList {
    /// One line: name, then per level `format|text|start|indent|hanging|font`, tab-separated.
    pub fn to_line(&self) -> String {
        let mut out = self.name.replace(['\t', '\n'], " ");
        for l in &self.levels {
            out.push('\t');
            out.push_str(&format!("{}|{}|{}|{}|{}|{}", l.fmt.ooxml(), l.text.replace(['|', '\t', '\n'], " "), l.start, l.indent_left, l.hanging, l.font.clone().unwrap_or_default()));
        }
        out
    }
    pub fn from_line(line: &str) -> Option<CustomList> {
        let mut parts = line.split('\t');
        let name = parts.next()?.to_string();
        let levels: Vec<Level> = parts.filter_map(|p| {
            let f: Vec<&str> = p.split('|').collect();
            if f.len() < 6 { return None; }
            Some(Level { fmt: NumFmt::from_ooxml(f[0]), text: f[1].to_string(), start: f[2].parse().ok()?, indent_left: f[3].parse().ok()?, hanging: f[4].parse().ok()?, font: (!f[5].is_empty()).then(|| f[5].to_string()) })
        }).collect();
        (!name.is_empty() && !levels.is_empty()).then_some(CustomList { name, levels })
    }
    /// A one-level number format; deeper levels follow the usual cycle.
    pub fn number(name: &str, fmt: NumFmt, pattern: &str, start: u32, script: Script) -> CustomList {
        let mut levels = levels_for(Some((fmt, pattern.to_string())), None, script);
        levels[0].start = start;
        CustomList { name: name.into(), levels }
    }
    pub fn bullet(name: &str, symbol: char, font: Option<String>) -> CustomList {
        let mut levels = levels_for(None, Some(symbol), Script::Latin);
        levels[0].font = font;
        CustomList { name: name.into(), levels }
    }
    /// The marker each level shows for its first item, e.g. ["Step 1:", "1.a", …].
    pub fn preview(&self) -> Vec<String> {
        self.levels.iter().enumerate().map(|(i, l)| {
            if l.fmt == NumFmt::Bullet { return l.text.clone(); }
            let mut t = l.text.clone();
            for k in 0..=i { let lv = &self.levels[k]; t = t.replace(&format!("%{}", k + 1), &crate::numbering::format_number(lv.start, lv.fmt)); }
            t
        }).collect()
    }
}

fn multilevel_levels(kind: Multilevel) -> Vec<Level> {
    (0..9).map(|i| match kind {
        Multilevel::Numbered => level(i, [NumFmt::Decimal, NumFmt::LowerLetter, NumFmt::LowerRoman][i % 3], format!("%{}.", i + 1)),
        Multilevel::Legal => level(i, NumFmt::Decimal, (1..=i + 1).map(|k| format!("%{k}.")).collect::<String>()),
        Multilevel::Bullets => level(i, NumFmt::Bullet, ['\u{25CF}', '\u{25CB}', '\u{25A0}'][i % 3].to_string()),
        Multilevel::Outline => {
            let (fmt, text) = [(NumFmt::UpperRoman, "%1."), (NumFmt::UpperLetter, "%2."), (NumFmt::Decimal, "%3."), (NumFmt::LowerLetter, "%4)"), (NumFmt::Decimal, "(%5)"), (NumFmt::LowerLetter, "(%6)"), (NumFmt::LowerRoman, "(%7)"), (NumFmt::LowerLetter, "(%8)"), (NumFmt::LowerRoman, "(%9)")][i];
            level(i, fmt, text.to_string())
        }
    }).map(|mut l| { if kind == Multilevel::Legal { l.hanging = 360 + 180 * (l.text.len() as i32 / 3); } l }).collect()
}

impl Document {
    /// Apply a new list definition: a list already at the caret switches as a whole (keeping each
    /// paragraph's level), otherwise the selected paragraphs start the list at level 1.
    pub fn apply_list_levels(&mut self, levels: Vec<Level>) {
        self.checkpoint(EditKind::Other);
        let num = self.numbering.new_list_levels(levels);
        let current = self.current_para_format().numbering.map(|(n, _)| n);
        let selected: Vec<usize> = self.selected_paragraph_indices().collect();
        for i in 0..self.paragraphs.len() {
            let f = self.para_format(&self.paragraphs[i]).numbering;
            let in_list = current.is_some() && f.map(|x| x.0) == current;
            if in_list || current.is_none() && selected.contains(&i) {
                let lvl = if in_list { f.unwrap().1 } else { 0 };
                let p = &mut self.paragraphs[i];
                p.props.numbering = Some((num, lvl));
                p.props.indent_left = None;
                p.props.first_line = None;
            }
        }
        self.changed();
    }
    fn current_script(&self) -> Script { self.list_script(self.locate(self.range().0).0) }
    pub fn apply_bullet(&mut self, bullet: char) { self.apply_list_levels(levels_for(None, Some(bullet), Script::Latin)); }
    pub fn apply_numbering(&mut self, fmt: NumFmt, pattern: &str) { let s = self.current_script(); self.apply_list_levels(levels_for(Some((fmt, pattern.to_string())), None, s)); }
    pub fn apply_custom_list(&mut self, list: &CustomList) { self.apply_list_levels(list.levels.clone()); }
    pub fn apply_multilevel(&mut self, kind: Multilevel) { self.apply_list_levels(multilevel_levels(kind)); }
    /// Continue numbering from the previous list of the same kind (Continue Numbering).
    pub fn continue_numbering(&mut self) {
        let Some((n, _)) = self.current_para_format().numbering else { return };
        let kind = self.numbering.kind(n);
        let first = self.locate(self.range().0).0;
        let prev = self.paragraphs[..first].iter().rev().filter_map(|p| self.para_format(p).numbering).find(|(m, _)| *m != n && self.numbering.kind(*m) == kind);
        let Some((target, _)) = prev else { return };
        self.checkpoint(EditKind::Other);
        for p in &mut self.paragraphs[first..] {
            if let Some((m, l)) = p.props.numbering { if m == n { p.props.numbering = Some((target, l)); } }
        }
        self.changed();
    }

    /// Change Case on the selection (or the word at the caret).
    pub fn change_case(&mut self, mode: CaseMode) {
        let (mut s, mut e) = self.range();
        if s == e { (s, e) = self.word_at(s); }
        if s == e { return; }
        let text: Vec<char> = self.text().chars().collect();
        let mut out = String::new();
        let mut start_of_sentence = s == 0 || text[..s].iter().rev().find(|c| !c.is_whitespace()).is_none_or(|c| matches!(c, '.' | '!' | '?' | '\n'));
        let mut start_of_word = s == 0 || !text[s - 1].is_alphanumeric();
        for &c in &text[s..e] {
            let mapped: String = match mode {
                CaseMode::Lower => c.to_lowercase().collect(),
                CaseMode::Upper => c.to_uppercase().collect(),
                CaseMode::Toggle => if c.is_uppercase() { c.to_lowercase().collect() } else { c.to_uppercase().collect() },
                CaseMode::Capitalize => if start_of_word { c.to_uppercase().collect() } else { c.to_lowercase().collect() },
                CaseMode::Sentence => if start_of_sentence && c.is_alphabetic() { c.to_uppercase().collect() } else { c.to_lowercase().collect() },
            };
            // Case mappings that change the length (ß → SS) keep the original character.
            out.push_str(if mapped.chars().count() == 1 { &mapped } else { "" });
            if mapped.chars().count() != 1 { out.push(c); }
            if c.is_alphabetic() { start_of_sentence = false; }
            if matches!(c, '.' | '!' | '?' | '\n') { start_of_sentence = true; }
            start_of_word = !c.is_alphanumeric() && c != '\'';
        }
        // Replace character by character so every run keeps its formatting.
        self.checkpoint(EditKind::Other);
        let new: Vec<char> = out.chars().collect();
        let mut pos = s;
        let starts = self.paragraph_starts();
        for (i, p) in self.paragraphs.iter_mut().enumerate() {
            let ps = starts[i];
            let mut at = ps;
            for r in &mut p.runs {
                let len = r.len();
                if r.image.is_none() && r.note.is_none() && r.field.is_none() && at + len > s && at < e {
                    r.text = r.text.chars().enumerate().map(|(k, c)| { let g = at + k; if g >= s && g < e { new[g - s] } else { c } }).collect();
                }
                at += len;
            }
            pos = pos.max(at);
        }
        let _ = pos;
        self.select(s, e);
        self.changed();
    }

    /// Sort the selected paragraphs by their text (numbers numerically) (Sort Text).
    pub fn sort_paragraphs(&mut self, descending: bool) -> bool {
        let r = self.selected_paragraph_indices();
        let (first, last) = (*r.start(), *r.end());
        if last <= first || self.paragraphs[first..=last].iter().any(|p| p.cell != self.paragraphs[first].cell) { return false; }
        self.checkpoint(EditKind::Other);
        let mut block: Vec<Paragraph> = self.paragraphs.drain(first..=last).collect();
        let num = |p: &Paragraph| crate::formula::cell_number(&p.text());
        block.sort_by(|a, b| {
            let o = match (num(a), num(b)) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                _ => (a.is_empty(), a.text().to_lowercase()).cmp(&(b.is_empty(), b.text().to_lowercase())),
            };
            if descending { o.reverse() } else { o }
        });
        self.paragraphs.splice(first..first, block);
        let s = self.paragraph_start(first);
        let e = self.paragraph_start(last) + self.paragraphs[last].len();
        self.select(s, e);
        self.changed();
        true
    }

    // ---- tables ----------------------------------------------------------------------------

    /// Horizontal placement of a table (§17.4.28 jc, §17.4.51 tblInd).
    pub fn set_table_alignment(&mut self, table: usize, alignment: Alignment, indent: i32) {
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[table];
        t.alignment = alignment;
        t.indent = if alignment == Alignment::Left { indent } else { 0 };
        t.raw_props = None;
        self.changed();
    }

    /// Scale every column (the table's resize handle). Returns the new total width in twips.
    pub fn scale_table(&mut self, table: usize, factor: f32) -> i32 {
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[table];
        for g in &mut t.grid { *g = ((*g as f32 * factor).round() as i32).max(216); }
        let grid = t.grid.clone();
        for row in &mut t.rows {
            let mut g = row.grid_before as usize;
            for c in &mut row.cells { let span = c.span.max(1) as usize; c.width = grid[g.min(grid.len())..(g + span).min(grid.len())].iter().sum(); g += span; }
        }
        self.changed();
        grid.iter().sum()
    }

    /// Move a whole table so it sits before the body paragraph containing `pos` (the move handle).
    pub fn move_table(&mut self, table: usize, pos: usize) -> bool {
        // The table's paragraphs, with any tables nested inside it.
        let Some(first) = self.paragraphs.iter().position(|p| p.cell.is_some_and(|c| self.table_contains_cell(table, c))) else { return false };
        let count = self.paragraphs[first..].iter().take_while(|p| p.cell.is_some_and(|c| self.table_contains_cell(table, c))).count();
        let (target, _) = self.locate(pos);
        if self.paragraphs[target].cell.is_some() || (target >= first && target <= first + count) { return false; }
        self.checkpoint(EditKind::Other);
        let old_parent = self.tables[table].parent;
        let block: Vec<Paragraph> = self.paragraphs.drain(first..first + count).collect();
        let at = if target > first { target - count } else { target };
        self.paragraphs.splice(at..at, block);
        // Moved into the body, a nested table is no longer inside its old cell, which keeps a
        // paragraph of its own.
        self.tables[table].parent = None;
        if let Some(cell) = old_parent {
            if !self.paragraphs.iter().any(|p| p.cell == Some(cell)) {
                if let Some(k) = self.paragraphs.iter().position(|p| p.cell.is_some_and(|c| self.table_of_cell(c) == self.table_of_cell(cell) && self.table_of_cell(c).is_some())) {
                    let mut p = Paragraph::default(); p.cell = Some(cell);
                    self.paragraphs.insert(k, p);
                }
            }
        }
        let at = self.paragraphs.iter().position(|p| p.cell.is_some_and(|c| self.table_contains_cell(table, c))).unwrap_or(at);
        // A table must be followed by a paragraph: the one it left keeps the gap closed.
        self.sort_tables();
        let p = self.paragraph_start(at);
        self.select(p, p);
        self.changed();
        true
    }

    /// Make the selected paragraphs' tab stops exactly `stops`: inherited style stops that are
    /// gone get a clear entry (§17.3.1.37), the rest are direct.
    pub fn set_tab_stops(&mut self, stops: Vec<TabStop>) {
        self.checkpoint(EditKind::Other);
        for i in self.selected_paragraph_indices() {
            let inherited = self.styles.para_props(self.paragraphs[i].style.as_deref()).tabs.unwrap_or_default();
            let mut direct: Vec<TabStop> = stops.clone();
            for t in inherited.iter().filter(|t| t.kind != TabKind::Clear && !stops.iter().any(|s| s.pos == t.pos)) {
                direct.push(TabStop { pos: t.pos, kind: TabKind::Clear, leader: Leader::None });
            }
            direct.sort_by_key(|t| t.pos);
            self.paragraphs[i].props.tabs = Some(direct);
        }
        self.changed();
    }

    /// Insert a cover page before the document's first paragraph (Insert → Cover Page).
    pub fn insert_cover_page(&mut self, design: CoverDesign, title: &str, subtitle: &str, author: &str, date: &str, accent: Color) {
        self.checkpoint(EditKind::Other);
        let run = |text: &str, size: f32, color: Color, bold: bool| Run { text: text.into(), props: RunProps { size: Some(size), size_cs: Some(size), color: Some(Some(color)), bold: Some(bold), bold_cs: Some(bold), ..Default::default() }, ..Default::default() };
        let para = |runs: Vec<Run>, props: ParaProps| { let mut p = Paragraph::default(); p.runs = runs.into_iter().filter(|r| !r.text.is_empty()).collect(); p.props = props; p };
        let grey = Color(0x59, 0x59, 0x59);
        let thick = |c: Color| Some(Border { width: 6.0, space: 8.0, color: Some(c) });
        let mut paras = match design {
            CoverDesign::Banded => vec![
                para(vec![run(title, 40.0, Color::WHITE, false)], ParaProps { space_before: Some(180.0), space_after: Some(0.0), shading: Some(Some(accent)), ..Default::default() }),
                para(vec![run(subtitle, 18.0, Color::WHITE, false)], ParaProps { space_after: Some(0.0), shading: Some(Some(accent)), ..Default::default() }),
            ],
            CoverDesign::Simple => vec![
                para(vec![run(title, 44.0, accent, false)], ParaProps { space_before: Some(200.0), space_after: Some(6.0), borders: Some(ParaBorders { bottom: Some(Border { width: 1.5, space: 4.0, color: Some(accent) }), ..Default::default() }), ..Default::default() }),
                para(vec![run(subtitle, 16.0, grey, false)], ParaProps::default()),
            ],
            CoverDesign::Sideline => vec![
                para(vec![run(title, 36.0, Color::BLACK, true)], ParaProps { space_before: Some(160.0), space_after: Some(4.0), indent_left: Some(360), borders: Some(ParaBorders { left: thick(accent), ..Default::default() }), ..Default::default() }),
                para(vec![run(subtitle, 16.0, accent, false)], ParaProps { indent_left: Some(360), borders: Some(ParaBorders { left: thick(accent), ..Default::default() }), ..Default::default() }),
            ],
            CoverDesign::Bold => vec![
                para(vec![run(&title.to_uppercase(), 48.0, Color(0x26, 0x26, 0x26), true)], ParaProps { space_before: Some(220.0), space_after: Some(0.0), alignment: Some(Alignment::Center), ..Default::default() }),
                para(vec![run(subtitle, 18.0, accent, false)], ParaProps { alignment: Some(Alignment::Center), borders: Some(ParaBorders { top: Some(Border { width: 3.0, space: 6.0, color: Some(accent) }), ..Default::default() }), ..Default::default() }),
            ],
        };
        let align = if design == CoverDesign::Bold { Some(Alignment::Center) } else { None };
        paras.push(para(vec![run(author, 14.0, grey, true)], ParaProps { space_before: Some(220.0), space_after: Some(0.0), alignment: align, ..Default::default() }));
        let mut last = para(vec![run(date, 12.0, grey, false), run(&crate::chars::PAGE_BREAK.to_string(), 12.0, grey, false)], ParaProps { alignment: align, ..Default::default() });
        last.mark = RunProps { size: Some(12.0), ..Default::default() };
        paras.push(last);
        self.paragraphs.splice(0..0, paras.clone());
        let n: usize = paras.iter().map(|p| p.len() + 1).sum();
        let (a, f) = (self.selection.anchor + n, self.selection.focus + n);
        self.selection = Selection { anchor: a, focus: f };
        self.changed();
    }

    /// A page break, an empty paragraph and another page break (Insert → Blank Page).
    pub fn insert_blank_page(&mut self) {
        let pb = crate::chars::PAGE_BREAK;
        self.replace(&format!("{pb}\n{pb}"));
    }

    /// Undo several steps at once (the Undo drop-down).
    pub fn undo_steps(&mut self, n: usize) { for _ in 0..n { if !self.undo() { break; } } }
    pub fn undo_depth(&self) -> usize { self.history_len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc(t: &str) -> Document { Document::new(t.split('\n').map(Paragraph::new).collect()) }

    #[test]
    fn list_libraries() {
        let mut d = doc("a\nb\nc");
        d.select(0, 5);
        d.apply_numbering(NumFmt::UpperRoman, "%1)");
        let mut c = crate::numbering::MarkerCounter::default();
        let (n, _) = d.para_format(&d.paragraphs[0]).numbering.unwrap();
        assert_eq!(c.next(&d.numbering, n, 0).unwrap(), "I)");
        // Switching the bullet changes the whole list, keeping levels.
        d.select(2, 2);
        d.change_list_level(1);
        d.apply_bullet('\u{2713}');
        let f: Vec<_> = d.paragraphs.iter().map(|p| d.para_format(p).numbering.unwrap()).collect();
        assert!(f.iter().all(|x| x.0 == f[0].0) && f[1].1 == 1 && f[0].1 == 0);
        assert_eq!(d.numbering.level(f[0].0, 0).unwrap().text, "\u{2713}");
        d.apply_multilevel(Multilevel::Legal);
        let (n, _) = d.para_format(&d.paragraphs[0]).numbering.unwrap();
        let mut c = crate::numbering::MarkerCounter::default();
        assert_eq!(c.next(&d.numbering, n, 0).unwrap(), "1.");
        assert_eq!(c.next(&d.numbering, n, 1).unwrap(), "1.1.");
        let bytes = crate::docx::write(&d).unwrap();
        assert!(crate::docx::read(&bytes).is_ok());
    }

    #[test]
    fn rtl_lists_and_custom_lists() {
        let mut d = doc("أول\nثاني\nثالث");
        d.select_all();
        d.toggle_list(crate::numbering::ListKind::Number);
        d.select(4, 4);
        d.change_list_level(1);
        let (n, _) = d.para_format(&d.paragraphs[0]).numbering.unwrap();
        let mut c = crate::numbering::MarkerCounter::default();
        assert_eq!(c.next(&d.numbering, n, 0).unwrap(), "1.");
        assert_eq!(c.next(&d.numbering, n, 1).unwrap(), "أ.", "the second level uses Arabic letters, not a b c");
        let custom = CustomList::number("Steps", NumFmt::Decimal, "Step %1:", 3, Script::Latin);
        assert_eq!(custom.preview()[0], "Step 3:");
        let back = CustomList::from_line(&custom.to_line()).unwrap();
        assert_eq!(back, custom);
        let mut d = doc("a\nb");
        d.select_all();
        d.apply_custom_list(&custom);
        let (n, _) = d.para_format(&d.paragraphs[1]).numbering.unwrap();
        let mut c = crate::numbering::MarkerCounter::default();
        c.next(&d.numbering, n, 0);
        assert_eq!(c.next(&d.numbering, n, 0).unwrap(), "Step 4:");
        let bytes = crate::docx::write(&d).unwrap();
        assert!(String::from_utf8_lossy(&bytes).len() > 0 && crate::docx::read(&bytes).is_ok());
    }

    #[test]
    fn cover_and_blank_pages() {
        let mut d = doc("Body text");
        d.select(4, 4);
        d.insert_cover_page(CoverDesign::Banded, "Annual Report", "2026", "Ada", "October 2026", Color(0x44, 0x72, 0xC4));
        assert_eq!(d.paragraphs.len(), 5);
        assert_eq!(d.paragraphs[4].text(), "Body text");
        assert_eq!(d.selected_text(), "", "the caret stays in the body text");
        assert_eq!(d.locate(d.range().0), (4, 4));
        assert_eq!(crate::layout_document(&d).page_count(), 2);
        let mut d = doc("one two");
        d.select(3, 3);
        d.insert_blank_page();
        assert_eq!(crate::layout_document(&d).page_count(), 3);
    }

    #[test]
    fn case_sort_and_tables() {
        let mut d = doc("hello WORLD. second sentence");
        d.select(0, 28);
        d.change_case(CaseMode::Sentence);
        assert_eq!(d.text(), "Hello world. Second sentence");
        d.change_case(CaseMode::Capitalize);
        assert_eq!(d.text(), "Hello World. Second Sentence");
        d.change_case(CaseMode::Toggle);
        assert_eq!(d.text(), "hELLO wORLD. sECOND sENTENCE");
        let mut d = doc("pear\n10\napple\n9");
        d.select_all();
        assert!(d.sort_paragraphs(false));
        assert_eq!(d.text(), "9\n10\napple\npear");
        let mut d = doc("top\nmiddle\nend");
        d.select(0, 0);
        d.insert_table(2, 2);
        d.set_table_alignment(0, Alignment::Center, 0);
        let w = d.scale_table(0, 0.5);
        assert!(w < 5000);
        let end = d.len();
        assert!(d.move_table(0, end));
        assert_eq!(d.paragraphs.iter().position(|p| p.cell.is_some()).unwrap(), 3, "the table now follows 'middle'");
        assert_eq!(d.text().lines().next().unwrap(), "top");
        let l = crate::layout_document(&d);
        assert!(l.cells.iter().all(|c| c.x0 > d.page.geometry().left + 50.0), "a centered table is indented");
        d.undo_steps(2);
        assert_eq!(d.paragraphs.iter().position(|p| p.cell.is_some()).unwrap(), 1);
    }
}
