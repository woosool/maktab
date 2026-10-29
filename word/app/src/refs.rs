//! Layout and References dialogs: Columns, Line Numbers, Hyphenation, Footnote and Endnote,
//! Table of Contents, Caption, Table of Figures, Source, Manage Sources, Mark Entry, Index,
//! Mark Citation; and the Selection Pane.

use crate::app::WordApp;
use egui::{ComboBox, DragValue, RichText, Ui};
use word_engine::numbering::{NumFmt, NUMBER_FORMATS};
use word_engine::page::{Columns, Hyphenation, LineNumbering, LineRestart};
use word_engine::references::{Source, SourceKind, TocOptions, AUTHORITY_CATEGORIES, CAPTION_LABELS};
use word_engine::Leader;

/// Where More Columns applies.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ApplyTo { Section, Forward, Document }

#[derive(Clone)]
pub struct ColumnsDraft { pub columns: Columns, pub apply: ApplyTo }

#[derive(Clone)]
pub struct NotesDraft { pub foot: bool, pub fmt: NumFmt, pub start: u32 }

#[derive(Clone)]
pub struct CaptionDraft { pub label: String, pub text: String, pub above: bool }

#[derive(Clone)]
pub struct MarkEntryDraft { pub main: String, pub sub: String }

#[derive(Clone)]
pub struct MarkCitationDraft { pub long: String, pub short: String, pub category: u8 }

/// The source being added or edited; `cite` inserts a citation after saving.
#[derive(Clone)]
pub struct SourceDraft { pub source: Source, pub editing: Option<String>, pub cite: bool }

#[derive(Default)]
pub struct RefsState {
    pub columns: Option<ColumnsDraft>,
    pub line_numbers: Option<LineNumbering>,
    pub line_numbers_on: bool,
    pub hyphenation: Option<Hyphenation>,
    pub notes: Option<NotesDraft>,
    pub toc: Option<TocOptions>,
    pub caption: Option<CaptionDraft>,
    pub figures: Option<(String, TocOptions)>,
    pub source: Option<SourceDraft>,
    pub manage_sources: bool,
    pub mark_entry: Option<MarkEntryDraft>,
    pub index: Option<u8>,
    pub mark_citation: Option<MarkCitationDraft>,
    pub selection_pane: bool,
    pub status: Option<String>,
}

impl WordApp {
    pub fn open_columns_dialog(&mut self) {
        let k = self.doc.current_section();
        self.extras.refs.columns = Some(ColumnsDraft { columns: self.doc.section_at(k).columns.clone(), apply: ApplyTo::Section });
    }
    pub fn open_line_numbers_dialog(&mut self) {
        let k = self.doc.current_section();
        let cur = self.doc.section_at(k).line_numbers;
        self.extras.refs.line_numbers_on = cur.is_some();
        self.extras.refs.line_numbers = Some(cur.unwrap_or(LineNumbering { start: 1, count_by: 1, restart: LineRestart::Page, distance: 0 }));
    }
    pub fn open_mark_entry(&mut self) {
        let t = self.doc.selected_text().trim().to_string();
        self.extras.refs.mark_entry = Some(MarkEntryDraft { main: t, sub: String::new() });
    }
    pub fn open_mark_citation(&mut self) {
        let t = self.doc.selected_text().trim().to_string();
        self.extras.refs.mark_citation = Some(MarkCitationDraft { short: t.clone(), long: t, category: 1 });
    }
    pub fn new_source(&mut self, cite: bool) {
        self.extras.refs.source = Some(SourceDraft { source: Source::default(), editing: None, cite });
    }
    pub fn has_block(&self, kind: &str, figures: bool) -> bool {
        word_engine::references::blocks(&self.doc).iter().any(|b| b.2.split_whitespace().next().is_some_and(|k| k.eq_ignore_ascii_case(kind)) && b.2.contains("\\c \"") == figures || (kind == "TOA" && b.2.starts_with("TOA")))
    }
}

/// Run a dialog window; returns (ok, keep open).
fn window(app: &mut WordApp, ctx: &egui::Context, title: &str, body: impl FnOnce(&mut Ui), ok_label: &str) -> (bool, bool) {
    let (mut open, mut ok, mut cancel) = (true, false, false);
    crate::theme::dialog(ctx, title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        body(ui);
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, ok_label).clicked() { ok = true; }
            if crate::theme::secondary(ui, "Cancel").clicked() { cancel = true; }
        });
    });
    if ok || cancel || !open { app.canvas_focus = true; }
    (ok, open && !ok && !cancel)
}

fn fmt_combo(ui: &mut Ui, id: &str, fmt: &mut NumFmt) {
    let name = NUMBER_FORMATS.iter().chain([(NumFmt::Chicago, "*, †, ‡, §")].iter()).find(|f| f.0 == *fmt).map(|f| f.1).unwrap_or("1, 2, 3");
    crate::theme::Combo::from_id_salt(id).selected_text(crate::i18n::tr(&name)).show_ui(ui, |ui| {
        for (f, n) in [(NumFmt::Decimal, "1, 2, 3"), (NumFmt::LowerLetter, "a, b, c"), (NumFmt::UpperLetter, "A, B, C"), (NumFmt::LowerRoman, "i, ii, iii"), (NumFmt::UpperRoman, "I, II, III"), (NumFmt::Chicago, "*, †, ‡, §")] { ui.selectable_value(fmt, f, crate::i18n::tr(&(n))); }
    });
}

fn toc_options(ui: &mut Ui, o: &mut TocOptions, levels: bool) {
    crate::theme::check(ui, &mut o.page_numbers, crate::i18n::tr("Show page numbers"));
    ui.add_enabled_ui(o.page_numbers, |ui| {
        crate::theme::check(ui, &mut o.right_align, crate::i18n::tr("Right align page numbers"));
        ui.add_enabled_ui(o.right_align, |ui| {
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Tab leader:"));
                crate::theme::Combo::from_id_salt("toc-leader").selected_text(crate::i18n::tr(&(match o.leader { Leader::Dot => "........", Leader::Hyphen => "--------", Leader::Underscore => "________", _ => "(none)" }))).show_ui(ui, |ui| {
                    for (l, n) in [(Leader::None, "(none)"), (Leader::Dot, "........"), (Leader::Hyphen, "--------"), (Leader::Underscore, "________")] { ui.selectable_value(&mut o.leader, l, crate::i18n::tr(&n)); }
                });
            });
        });
    });
    crate::theme::check(ui, &mut o.hyperlinks, crate::i18n::tr("Use hyperlinks instead of page numbers online"));
    if levels { crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Show levels:")); ui.add(DragValue::new(&mut o.levels).range(1..=9)); }); }
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    if let Some(mut d) = app.extras.refs.columns.take() {
        let (ok, keep) = window(app, ctx, "Columns", |ui| {
            crate::theme::hrow(ui, |ui| {
                for (n, name) in [(1u8, "One"), (2, "Two"), (3, "Three")] { if ui.selectable_label(d.columns.count == n && d.columns.widths.is_empty(), crate::i18n::tr(&name)).clicked() { d.columns.count = n; d.columns.widths.clear(); } }
                if ui.selectable_label(!d.columns.widths.is_empty() && d.columns.widths[0].0 < d.columns.widths[1].0, "Left").clicked() { d.columns = preset(true); }
                if ui.selectable_label(!d.columns.widths.is_empty() && d.columns.widths[0].0 > d.columns.widths.get(1).map(|w| w.0).unwrap_or(0), "Right").clicked() { d.columns = preset(false); }
            });
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Number of columns:")); if ui.add(DragValue::new(&mut d.columns.count).range(1..=12)).changed() { d.columns.widths.clear(); } });
            let mut sp = d.columns.space as f32 / 1440.0;
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Spacing:")); if ui.add(DragValue::new(&mut sp).range(0.0..=3.0).speed(0.01).suffix("\"")).changed() { d.columns.space = (sp * 1440.0).round() as i32; } });
            crate::theme::check(ui, &mut d.columns.separator, crate::i18n::tr("Line between"));
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Apply to:"));
                crate::theme::Combo::from_id_salt("cols-apply").selected_text(crate::i18n::tr(&(match d.apply { ApplyTo::Section => "This section", ApplyTo::Forward => "This point forward", ApplyTo::Document => "Whole document" }))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.apply, ApplyTo::Section, crate::i18n::tr("This section"));
                    ui.selectable_value(&mut d.apply, ApplyTo::Forward, crate::i18n::tr("This point forward"));
                    ui.selectable_value(&mut d.apply, ApplyTo::Document, crate::i18n::tr("Whole document"));
                });
            });
        }, "OK");
        if ok { apply_columns(app, d.columns.clone(), d.apply); }
        if keep { app.extras.refs.columns = Some(d); }
    }
    if let Some(mut ln) = app.extras.refs.line_numbers.take() {
        let mut on = app.extras.refs.line_numbers_on;
        let (ok, keep) = window(app, ctx, "Line Numbers", |ui| {
            crate::theme::check(ui, &mut on, crate::i18n::tr("Add line numbering"));
            ui.add_enabled_ui(on, |ui| {
                crate::theme::grid(ui, egui::Grid::new("ln").num_columns(2), |ui| {
                    ui.label(crate::i18n::tr("Start at:")); ui.add(DragValue::new(&mut ln.start).range(1..=32767)); ui.end_row();
                    let mut dist = ln.distance as f32 / 1440.0;
                    ui.label(crate::i18n::tr("From text:")); if ui.add(DragValue::new(&mut dist).range(0.0..=2.0).speed(0.01).custom_formatter(|v, _| if v == 0.0 { "Auto".into() } else { format!("{v:.2}\"") })).changed() { ln.distance = (dist * 1440.0).round() as i32; } ui.end_row();
                    ui.label(crate::i18n::tr("Count by:")); ui.add(DragValue::new(&mut ln.count_by).range(1..=100)); ui.end_row();
                });
                crate::theme::radio_value(ui, &mut ln.restart, LineRestart::Page, crate::i18n::tr("Restart each page"));
                crate::theme::radio_value(ui, &mut ln.restart, LineRestart::Section, crate::i18n::tr("Restart each section"));
                crate::theme::radio_value(ui, &mut ln.restart, LineRestart::Continuous, crate::i18n::tr("Continuous"));
            });
        }, "OK");
        app.extras.refs.line_numbers_on = on;
        if ok { app.doc.set_line_numbers(on.then_some(ln)); }
        if keep { app.extras.refs.line_numbers = Some(ln); }
    }
    if let Some(mut h) = app.extras.refs.hyphenation.take() {
        let (ok, keep) = window(app, ctx, "Hyphenation", |ui| {
            crate::theme::check(ui, &mut h.auto, crate::i18n::tr("Automatically hyphenate document"));
            crate::theme::check(ui, &mut h.caps, crate::i18n::tr("Hyphenate words in CAPS"));
            let mut z = h.zone as f32 / 1440.0;
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Hyphenation zone:")); if ui.add(DragValue::new(&mut z).range(0.05..=2.0).speed(0.01).suffix("\"")).changed() { h.zone = (z * 1440.0).round() as i32; } });
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Limit consecutive hyphens to:")); ui.add(DragValue::new(&mut h.limit).range(0..=20).custom_formatter(|v, _| if v == 0.0 { "No limit".into() } else { format!("{v}") })); });
        }, "OK");
        if ok { app.doc.set_hyphenation(h); }
        if keep { app.extras.refs.hyphenation = Some(h); }
    }
    if let Some(mut d) = app.extras.refs.notes.take() {
        let (mut ok, keep) = (false, true);
        let mut insert = false;
        let mut open = true;
        let mut close = false;
        crate::theme::dialog(ctx, "Footnote and Endnote").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::hrow(ui, |ui| {
                if crate::theme::radio(ui, d.foot, crate::i18n::tr("Footnotes")).clicked() { d.foot = true; (d.fmt, d.start) = app.doc.page.footnote_format; }
                if crate::theme::radio(ui, !d.foot, crate::i18n::tr("Endnotes")).clicked() { d.foot = false; (d.fmt, d.start) = app.doc.page.endnote_format; }
            });
            crate::theme::grid(ui, egui::Grid::new("notes").num_columns(2), |ui| {
                ui.label(crate::i18n::tr("Number format:")); fmt_combo(ui, "note-fmt", &mut d.fmt); ui.end_row();
                ui.label(crate::i18n::tr("Start at:")); ui.add(DragValue::new(&mut d.start).range(1..=9999)); ui.end_row();
            });
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "Insert").clicked() { ok = true; insert = true; }
                if crate::theme::secondary(ui, "Cancel").clicked() { close = true; }
                if crate::theme::secondary(ui, "Apply").clicked() { ok = true; }
            });
        });
        if ok { app.doc.set_note_format(d.foot, d.fmt, d.start); }
        if insert { app.insert_note(if d.foot { word_engine::NoteKind::Foot } else { word_engine::NoteKind::End }); }
        if keep && open && !ok && !close { app.extras.refs.notes = Some(d); } else { app.canvas_focus = true; }
    }
    if let Some(mut o) = app.extras.refs.toc.take() {
        let (ok, keep) = window(app, ctx, "Table of Contents", |ui| {
            toc_options(ui, &mut o, true);
            crate::theme::check(ui, &mut o.title, crate::i18n::tr("Include a \"Contents\" heading"));
        }, "OK");
        if ok { if app.has_block("TOC", false) { app.doc.remove_toc(); } app.doc.insert_toc(o); }
        if keep { app.extras.refs.toc = Some(o); }
    }
    if let Some(mut d) = app.extras.refs.caption.take() {
        let (ok, keep) = window(app, ctx, "Caption", |ui| {
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Caption:")); ui.label(RichText::new(format!("{} 1", d.label)).strong()); ui.text_edit_singleline(&mut d.text); });
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Label:"));
                crate::theme::Combo::from_id_salt("cap-label").selected_text(crate::i18n::tr(&d.label.clone())).show_ui(ui, |ui| { for l in CAPTION_LABELS { ui.selectable_value(&mut d.label, l.to_string(), crate::i18n::tr(&*l)); } });
            });
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Position:")); crate::theme::radio_value(ui, &mut d.above, false, crate::i18n::tr("Below selected item")); crate::theme::radio_value(ui, &mut d.above, true, crate::i18n::tr("Above selected item")); });
        }, "OK");
        if ok { app.doc.insert_caption(&d.label, &d.text, d.above); }
        if keep { app.extras.refs.caption = Some(d); }
    }
    if let Some((mut label, mut o)) = app.extras.refs.figures.take() {
        let (ok, keep) = window(app, ctx, "Table of Figures", |ui| {
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Caption label:"));
                crate::theme::Combo::from_id_salt("tof-label").selected_text(crate::i18n::tr(&label.clone())).show_ui(ui, |ui| { for l in CAPTION_LABELS { ui.selectable_value(&mut label, l.to_string(), crate::i18n::tr(&*l)); } });
            });
            toc_options(ui, &mut o, false);
        }, "OK");
        if ok { app.doc.insert_table_of_figures(&label, o); }
        if keep { app.extras.refs.figures = Some((label, o)); }
    }
    source_dialog(app, ctx);
    manage_sources(app, ctx);
    if let Some(mut d) = app.extras.refs.mark_entry.take() {
        let mut open = true;
        let (mut mark, mut all, mut close) = (false, false, false);
        crate::theme::dialog(ctx, "Mark Index Entry").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::grid(ui, egui::Grid::new("xe").num_columns(2), |ui| {
                ui.label(crate::i18n::tr("Main entry:")); ui.text_edit_singleline(&mut d.main); ui.end_row();
                ui.label(crate::i18n::tr("Subentry:")); ui.text_edit_singleline(&mut d.sub); ui.end_row();
            });
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "Mark").clicked() { mark = true; }
                if crate::theme::secondary(ui, "Mark All").clicked() { all = true; }
                if crate::theme::secondary(ui, "Close").clicked() { close = true; }
            });
            if let Some(s) = &app.extras.refs.status { ui.label(RichText::new(s).small()); }
        });
        let entry = if d.sub.trim().is_empty() { d.main.trim().to_string() } else { format!("{}:{}", d.main.trim(), d.sub.trim()) };
        if mark && !d.main.trim().is_empty() { app.doc.mark_index_entry(&entry); app.extras.refs.status = Some("Entry marked.".into()); }
        if all && !d.main.trim().is_empty() {
            let text = app.doc.selected_text().trim().to_string();
            let text = if text.is_empty() { d.main.trim().to_string() } else { text };
            let n = app.doc.mark_all_index_entries(&text, &entry);
            app.extras.refs.status = Some(crate::i18n::trf("{} entries marked.", &[&n]));
        }
        // Word keeps this dialog open while you mark more entries.
        if open && !close { app.extras.refs.mark_entry = Some(d); } else { app.extras.refs.status = None; app.canvas_focus = true; }
    }
    if let Some(mut cols) = app.extras.refs.index.take() {
        let (ok, keep) = window(app, ctx, "Index", |ui| {
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Columns:")); ui.add(DragValue::new(&mut cols).range(1..=4)); });
            ui.label(RichText::new(crate::i18n::tr("Mark entries first with Mark Entry; the index lists them with their pages.")).small());
        }, "OK");
        if ok { app.doc.insert_index(cols); }
        if keep { app.extras.refs.index = Some(cols); }
    }
    if let Some(mut d) = app.extras.refs.mark_citation.take() {
        let (ok, keep) = window(app, ctx, "Mark Citation", |ui| {
            crate::theme::grid(ui, egui::Grid::new("ta").num_columns(2), |ui| {
                ui.label(crate::i18n::tr("Long citation:")); ui.text_edit_multiline(&mut d.long); ui.end_row();
                ui.label(crate::i18n::tr("Short citation:")); ui.text_edit_singleline(&mut d.short); ui.end_row();
                ui.label(crate::i18n::tr("Category:"));
                crate::theme::Combo::from_id_salt("ta-cat").selected_text(crate::i18n::tr(&(AUTHORITY_CATEGORIES[(d.category.max(1) - 1) as usize]))).show_ui(ui, |ui| { for (k, c) in AUTHORITY_CATEGORIES.iter().enumerate() { ui.selectable_value(&mut d.category, k as u8 + 1, crate::i18n::tr(&*c)); } });
                ui.end_row();
            });
        }, "Mark");
        if ok && !d.long.trim().is_empty() { app.doc.mark_authority(d.long.trim(), d.short.trim(), d.category); }
        if keep { app.extras.refs.mark_citation = Some(d); }
    }
}

/// Word's Left/Right column presets: a narrow column beside a wide one.
fn preset(left: bool) -> Columns {
    let (narrow, wide) = (2_880, 5_760);
    Columns { count: 2, space: 720, separator: false, widths: if left { vec![(narrow, 720), (wide, 0)] } else { vec![(wide, 720), (narrow, 0)] } }
}

/// Left/Right presets sized to the page's writing width.
pub fn sized_preset(app: &WordApp, left: bool) -> Columns {
    let total = app.doc.page.width - app.doc.page.left - app.doc.page.right - 720;
    let narrow = total / 3;
    let mut c = preset(left);
    c.widths = if left { vec![(narrow, 720), (total - narrow, 0)] } else { vec![(total - narrow, 720), (narrow, 0)] };
    c
}

pub fn apply_columns(app: &mut WordApp, mut c: Columns, to: ApplyTo) {
    if !c.widths.is_empty() && c.widths.len() == 2 { let left = c.widths[0].0 < c.widths[1].0; let sep = c.separator; c = sized_preset(app, left); c.separator = sep; }
    match to { ApplyTo::Section => app.doc.set_columns(c), ApplyTo::Forward => app.doc.columns_from_here(c), ApplyTo::Document => app.doc.set_columns_everywhere(c) }
}

fn source_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.refs.source.take() else { return };
    let title = if d.editing.is_some() { "Edit Source" } else { "Create Source" };
    let (ok, keep) = window(app, ctx, title, |ui| {
        let s = &mut d.source;
        crate::theme::hrow(ui, |ui| {
            ui.label(crate::i18n::tr("Type of source:"));
            crate::theme::Combo::from_id_salt("src-kind").selected_text(crate::i18n::tr(&(SourceKind::ALL.iter().find(|k| k.0 == s.kind).map(|k| k.1).unwrap_or("Book")))).show_ui(ui, |ui| { for (k, n) in SourceKind::ALL { ui.selectable_value(&mut s.kind, k, crate::i18n::tr(&n)); } });
        });
        crate::theme::grid(ui, egui::Grid::new("src").num_columns(2), |ui| {
            ui.label(crate::i18n::tr("Author:")); ui.add(egui::TextEdit::singleline(&mut s.authors).hint_text(crate::i18n::tr("Last, First; Last, First"))); ui.end_row();
            ui.label(crate::i18n::tr("Title:")); ui.text_edit_singleline(&mut s.title); ui.end_row();
            ui.label(crate::i18n::tr("Year:")); ui.text_edit_singleline(&mut s.year); ui.end_row();
            match s.kind {
                SourceKind::JournalArticle => {
                    ui.label(crate::i18n::tr("Journal Name:")); ui.text_edit_singleline(&mut s.journal); ui.end_row();
                    ui.label(crate::i18n::tr("Volume:")); ui.text_edit_singleline(&mut s.volume); ui.end_row();
                    ui.label(crate::i18n::tr("Issue:")); ui.text_edit_singleline(&mut s.issue); ui.end_row();
                    ui.label(crate::i18n::tr("Pages:")); ui.text_edit_singleline(&mut s.pages); ui.end_row();
                }
                SourceKind::Website => { ui.label(crate::i18n::tr("URL:")); ui.text_edit_singleline(&mut s.url); ui.end_row(); }
                _ => {
                    ui.label(crate::i18n::tr("City:")); ui.text_edit_singleline(&mut s.city); ui.end_row();
                    ui.label(crate::i18n::tr("Publisher:")); ui.text_edit_singleline(&mut s.publisher); ui.end_row();
                }
            }
            ui.label(crate::i18n::tr("Tag name:")); ui.add(egui::TextEdit::singleline(&mut s.tag).hint_text(crate::i18n::tr("automatic"))); ui.end_row();
        });
    }, "OK");
    if ok {
        let mut s = d.source.clone();
        if s.tag.trim().is_empty() { s.tag = app.doc.new_source_tag(&s); }
        if let Some(old) = &d.editing { if *old != s.tag { app.doc.remove_source(old); } }
        let tag = s.tag.clone();
        app.doc.add_source(s);
        if d.cite { app.doc.insert_field(word_engine::Field::Citation(tag)); }
    }
    if keep { app.extras.refs.source = Some(d); }
}

fn manage_sources(app: &mut WordApp, ctx: &egui::Context) {
    if !app.extras.refs.manage_sources { return; }
    let mut open = true;
    let mut action: Option<(u8, String)> = None;
    crate::theme::dialog(ctx, "Source Manager").open(&mut open).collapsible(false).default_width(460.0).show(ctx, |ui| {
        app.canvas_focus = false;
        ui.label(RichText::new(crate::i18n::trf("Sources in this document — style: {}", &[&app.doc.citation_style.name()])).strong());
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            if app.doc.sources.is_empty() { ui.label(crate::i18n::tr("No sources yet.")); }
            for s in &app.doc.sources {
                crate::theme::hrow(ui, |ui| {
                    ui.label(format!("{} — {} ({})", s.authors, s.title, s.year));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(crate::i18n::tr("Delete")).clicked() { action = Some((2, s.tag.clone())); }
                        if ui.small_button(crate::i18n::tr("Edit…")).clicked() { action = Some((1, s.tag.clone())); }
                        if ui.small_button(crate::i18n::tr("Cite")).clicked() { action = Some((3, s.tag.clone())); }
                    });
                });
            }
        });
        if ui.button(crate::i18n::tr("New…")).clicked() { action = Some((0, String::new())); }
    });
    match action {
        Some((0, _)) => app.new_source(false),
        Some((1, tag)) => if let Some(s) = app.doc.sources.iter().find(|s| s.tag == tag).cloned() { app.extras.refs.source = Some(SourceDraft { source: s, editing: Some(tag), cite: false }); },
        Some((2, tag)) => app.doc.remove_source(&tag),
        Some((_, tag)) => app.doc.insert_field(word_engine::Field::Citation(tag)),
        None => {}
    }
    if !open { app.extras.refs.manage_sources = false; app.canvas_focus = true; }
}

/// The Selection Pane: every picture and object, top of the stack first; click to select.
pub fn selection_pane(app: &mut WordApp, ui: &mut Ui) {
    if !app.extras.refs.selection_pane { return; }
    egui::Panel::right("selection-pane").resizable(true).default_size(240.0).show(ui, |ui| {
        crate::theme::hrow(ui, |ui| {
            ui.heading(crate::i18n::tr("Selection"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if crate::ribbon::close_button(ui, "Close").clicked() { app.extras.refs.selection_pane = false; } });
        });
        let mut list = app.doc.drawings();
        list.sort_by_key(|d| std::cmp::Reverse(d.z.unwrap_or(i64::MIN)));
        let selected = app.selected_box();
        let mut act = None;
        crate::theme::hrow(ui, |ui| {
            ui.add_enabled_ui(selected.is_some_and(|p| app.doc.anchor_at(p).is_some()), |ui| {
                if ui.button("▲").on_hover_text(crate::i18n::tr("Bring Forward")).clicked() { act = Some(word_engine::arrange::Stack::Forward); }
                if ui.button("▼").on_hover_text(crate::i18n::tr("Send Backward")).clicked() { act = Some(word_engine::arrange::Stack::Backward); }
            });
        });
        if let (Some(a), Some(p)) = (act, selected) { app.doc.restack(p, a); app.doc.select(p, p + 1); }
        ui.separator();
        if list.is_empty() { ui.label(RichText::new(crate::i18n::tr("No shapes or pictures on this document.")).weak()); }
        for d in list {
            let label = if d.z.is_some() { d.name.clone() } else { crate::i18n::trf("{} (in line)", &[&d.name]) };
            if ui.selectable_label(selected == Some(d.pos), crate::i18n::tr(&label)).clicked() { app.doc.select(d.pos, d.pos + 1); app.scroll_to_caret = true; }
        }
    });
}
