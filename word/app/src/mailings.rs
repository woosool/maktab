//! Mailings tab: envelopes and labels, recipients (new list, CSV/Excel), Edit Recipient List,
//! merge fields, Address Block, Greeting Line, rules, Match Fields, previewing records, Find
//! Recipient, Check for Errors, and Finish & Merge (new document, PDF, e-mail files).

use crate::app::WordApp;
use egui::{ComboBox, RichText, Ui};
use word_engine::mailmerge::{Compare, DataSource, MergeKind, NameFormat, ADDRESS_FIELDS, ENVELOPES, LABELS};
use word_engine::Field;

#[derive(Default)]
pub struct MailingsState {
    pub envelope: Option<EnvelopeDraft>,
    pub labels: Option<LabelsDraft>,
    pub recipients: Option<RecipientsDraft>,
    pub greeting: Option<GreetingDraft>,
    pub address_block: bool,
    pub rule: Option<RuleDraft>,
    pub match_fields: bool,
    pub find: Option<String>,
    pub errors: Option<Vec<String>>,
    pub finish: Option<FinishDraft>,
}

pub struct EnvelopeDraft { pub delivery: String, pub ret: String, pub size: usize }
pub struct LabelsDraft { pub text: String, pub spec: usize, pub merge: bool }
pub struct RecipientsDraft { pub filter: String, pub sort: Option<(usize, bool)>, pub new_list: bool }
pub struct GreetingDraft { pub greeting: String, pub name: NameFormat, pub punctuation: String }
pub struct RuleDraft { pub kind: u8, pub field: String, pub op: Compare, pub value: String, pub then: String, pub otherwise: String }
pub struct FinishDraft { pub target: u8, pub range: u8, pub from: usize, pub to: usize, pub email_field: String, pub subject: String }

pub const RULES: &[&str] = &["If…Then…Else…", "Merge Record #", "Merge Sequence #", "Next Record", "Next Record If…", "Skip Record If…"];

impl WordApp {
    fn has_recipients(&self) -> bool { self.doc.mail_merge.source.is_some() }

    pub fn use_existing_list(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Recipient lists", &["csv", "txt", "tsv", "xlsx"]).pick_file() else { return };
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let res = std::fs::read(&p).map_err(|e| e.to_string()).and_then(|bytes| if name.to_lowercase().ends_with(".xlsx") { DataSource::read_xlsx(&name, &bytes) } else { DataSource::parse_delimited(&name, &String::from_utf8_lossy(&bytes)) });
        match res {
            Ok(src) => { self.doc.set_merge_source(Some(src), Some(p.display().to_string())); self.extras.mailings.recipients = Some(RecipientsDraft { filter: String::new(), sort: None, new_list: false }); }
            Err(e) => self.dialogs.error = Some(crate::i18n::trf("Couldn't read the recipient list: {}", &[&e])),
        }
    }

    pub fn type_new_list(&mut self) {
        let headers: Vec<String> = ["Title", "First Name", "Last Name", "Company", "Address 1", "Address 2", "City", "State", "Postal Code", "Country or Region", "E-mail Address"].iter().map(|s| s.to_string()).collect();
        let n = headers.len();
        self.doc.set_merge_source(Some(DataSource { name: "New list".into(), headers, rows: vec![vec![String::new(); n]] }), None);
        self.extras.mailings.recipients = Some(RecipientsDraft { filter: String::new(), sort: None, new_list: true });
    }

    pub fn insert_merge(&mut self, f: Field) {
        if self.doc.mail_merge.kind == MergeKind::Normal { self.doc.mail_merge.kind = MergeKind::Letters; }
        self.doc.insert_field(f);
        self.canvas_focus = true;
    }

    fn finish(&mut self, d: &FinishDraft) {
        let n = self.doc.mail_merge.source.as_ref().map(|s| s.rows.len()).unwrap_or(0);
        let (from, to) = match d.range { 1 => { let r = self.doc.mail_merge.included().get(self.doc.mail_merge.record).copied().unwrap_or(0); (r, r) } 2 => (d.from.saturating_sub(1), d.to.saturating_sub(1)), _ => (0, n) };
        let merged = match self.doc.merged_document(from, to) { Ok(m) => m, Err(e) => { self.dialogs.error = Some(e); return; } };
        match d.target {
            // Print: a PDF of the merged documents.
            1 => {
                let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name("Merged.pdf").save_file() else { return };
                let layout = word_engine::layout_document(&merged);
                if let Err(e) = std::fs::write(&path, word_engine::pdf::export(&merged, &layout, "Merged")) { self.dialogs.error = Some(e.to_string()); }
                else { self.notify(&self.ctx.clone(), crate::i18n::trf("Saved {}", &[&path.display()])); }
            }
            // E-mail: one .eml message per recipient, ready for any mail app.
            2 => {
                let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
                let mm = self.doc.mail_merge.clone();
                let order: Vec<usize> = mm.included().into_iter().filter(|r| *r >= from && *r <= to).collect();
                let mut count = 0;
                for (k, row) in order.iter().enumerate() {
                    let to_addr = mm.source.as_ref().and_then(|s| s.get(*row, &d.email_field)).unwrap_or("").trim().to_string();
                    if to_addr.is_empty() { continue; }
                    let one = self.doc.merged_document(*row, *row).map(|m| m.text()).unwrap_or_default();
                    let eml = format!("To: {to_addr}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nX-Unsent: 1\r\n\r\n{}\r\n", d.subject, one.replace('\n', "\r\n"));
                    if std::fs::write(dir.join(format!("message-{:03}.eml", k + 1)), eml).is_ok() { count += 1; }
                }
                self.notify(&self.ctx.clone(), crate::i18n::trf("{} messages saved in {}", &[&count, &dir.display()]));
            }
            _ => { self.set_document(merged, None); self.doc.dirty = true; }
        }
    }
}

pub fn tab(app: &mut WordApp, ui: &mut Ui) {
    use crate::ribbon::{group, large, large_menu, rows, small, menu_item};
    let has = app.has_recipients();
    let kind = app.doc.mail_merge.kind;
    group(ui, "Create", |ui| {
        if large(ui, "envelope", "Envelopes", false, true, "Create and print envelopes").clicked() { app.extras.mailings.envelope = Some(EnvelopeDraft { delivery: app.doc.selected_text(), ret: app.author.clone(), size: 0 }); }
        if large(ui, "labels", "Labels", false, true, "Create and print labels").clicked() { app.extras.mailings.labels = Some(LabelsDraft { text: app.doc.selected_text(), spec: 0, merge: false }); }
    });
    group(ui, "Start Mail Merge", |ui| {
        let s = large_menu(ui, "startMerge", "Start Mail Merge", "Choose the kind of document to merge");
        crate::theme::menu(&s, |ui| {
            for (k, n) in MergeKind::ALL {
                if ui.selectable_label(kind == k, crate::i18n::tr(&n)).clicked() {
                    match k {
                        MergeKind::Envelopes => app.extras.mailings.envelope = Some(EnvelopeDraft { delivery: String::new(), ret: app.author.clone(), size: 0 }),
                        MergeKind::Labels => app.extras.mailings.labels = Some(LabelsDraft { text: String::new(), spec: 0, merge: true }),
                        _ => app.doc.mail_merge.kind = k,
                    }
                    ui.close();
                }
            }
        });
        let r = large_menu(ui, "selectRecipients", "Select Recipients", "Choose the people to merge");
        crate::theme::menu(&r, |ui| {
            if ui.button(crate::i18n::tr("Type a New List…")).clicked() { app.type_new_list(); ui.close(); }
            if ui.button(crate::i18n::tr("Use an Existing List…")).clicked() { app.use_existing_list(); ui.close(); }
        });
        if large(ui, "editRecipients", "Edit Recipient List", false, has, "Choose, sort, filter and edit recipients").clicked() { app.extras.mailings.recipients = Some(RecipientsDraft { filter: String::new(), sort: None, new_list: false }); }
    });
    group(ui, "Write & Insert Fields", |ui| {
        if large(ui, "highlightFields", "Highlight Merge Fields", app.doc.mail_merge.highlight, has, "Shade merge fields").clicked() { app.doc.merge_view(|m| m.highlight = !m.highlight); }
        if large(ui, "addressBlock", "Address Block", false, has, "Insert the recipient's address").clicked() { app.extras.mailings.address_block = true; }
        if large(ui, "greetingLine", "Greeting Line", false, has, "Insert a greeting").clicked() { app.extras.mailings.greeting = Some(GreetingDraft { greeting: "Dear".into(), name: NameFormat::FirstLast, punctuation: ",".into() }); }
        let f = large_menu(ui, "mergeField", "Insert Merge Field", "Insert a recipient column");
        if has {
            crate::theme::menu(&f, |ui| {
                let headers = app.doc.mail_merge.source.as_ref().map(|s| s.headers.clone()).unwrap_or_default();
                for h in headers { if ui.button(crate::i18n::tr(&h)).clicked() { app.insert_merge(Field::Merge(h)); ui.close(); } }
            });
        }
        rows(ui, |ui| {
            let r = small(ui, "rules", Some("Rules"), false, has, "Merge rules");
            if has {
                crate::theme::menu(&r, |ui| {
                    for (k, n) in RULES.iter().enumerate() {
                        if ui.button(crate::i18n::tr(&*n)).clicked() {
                            match k {
                                1 => app.insert_merge(Field::MergeRec),
                                2 => app.insert_merge(Field::MergeSeq),
                                3 => app.insert_merge(Field::NextRecord),
                                _ => { let field = app.doc.mail_merge.source.as_ref().and_then(|s| s.headers.first().cloned()).unwrap_or_default(); app.extras.mailings.rule = Some(RuleDraft { kind: k as u8, field, op: Compare::Equal, value: String::new(), then: String::new(), otherwise: String::new() }); }
                            }
                            ui.close();
                        }
                    }
                });
            }
            if small(ui, "matchFields", Some("Match Fields"), false, has, "Match address parts to your columns").clicked() { app.extras.mailings.match_fields = true; }
            if small(ui, "updateLabels", Some("Update Labels"), false, has && kind == MergeKind::Labels, "Copy the first label to all labels").clicked() { app.doc.update_labels(); }
        });
    });
    let n = app.doc.mail_merge.included().len();
    let preview = app.doc.mail_merge.preview;
    group(ui, "Preview Results", |ui| {
        if large(ui, "previewResults", "Preview Results", preview, has, "Show a recipient's values in the fields").clicked() { app.doc.merge_view(|m| m.preview = !m.preview); }
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                let at = app.doc.mail_merge.record;
                if small(ui, "firstRecord", None, false, preview && at > 0, "First record").clicked() { app.doc.merge_view(|m| m.record = 0); }
                if small(ui, "previousRecord", None, false, preview && at > 0, "Previous record").clicked() { app.doc.merge_view(|m| m.record -= 1); }
                let mut v = at + 1;
                if ui.add_enabled(preview && n > 0, egui::DragValue::new(&mut v).range(1..=n.max(1))).changed() { app.doc.merge_view(|m| m.record = v - 1); }
                if small(ui, "nextRecord", None, false, preview && at + 1 < n, "Next record").clicked() { app.doc.merge_view(|m| m.record += 1); }
                if small(ui, "lastRecord", None, false, preview && at + 1 < n, "Last record").clicked() { app.doc.merge_view(|m| m.record = n.saturating_sub(1)); }
            });
            if small(ui, "findRecipient", Some("Find Recipient"), false, has, "Find a recipient by any value").clicked() { app.extras.mailings.find = Some(String::new()); }
            if small(ui, "checkErrors", Some("Check for Errors"), false, has || app.doc.has_merge_fields(), "Find fields the list doesn't have").clicked() { app.extras.mailings.errors = Some(app.doc.merge_errors()); }
        });
    });
    group(ui, "Finish", |ui| {
        let f = large_menu(ui, "finishMerge", "Finish & Merge", "Merge into documents, a PDF or e-mail");
        if has {
            crate::theme::menu(&f, |ui| {
                let email = app.doc.mail_merge.column_for("E-mail Address").unwrap_or_default();
                for (k, name) in [(0u8, "Edit Individual Documents…"), (1, "Print Documents… (PDF)"), (2, "Send E-mail Messages…")] {
                    if menu_item(ui, if k == 2 { "envelope" } else { "filePlus" }, name) { app.extras.mailings.finish = Some(FinishDraft { target: k, range: 0, from: 1, to: n, email_field: email.clone(), subject: String::new() }); ui.close(); }
                }
            });
        }
    });
}

fn record_nav(app: &mut WordApp, ui: &mut Ui) {
    let n = app.doc.mail_merge.included().len();
    crate::theme::hrow(ui, |ui| {
        let at = app.doc.mail_merge.record;
        if ui.add_enabled(at > 0, egui::Button::new("◀")).clicked() { app.doc.merge_view(|m| m.record -= 1); }
        ui.label(crate::i18n::trf("Recipient {} of {}", &[&(at + 1), &n]));
        if ui.add_enabled(at + 1 < n, egui::Button::new("▶")).clicked() { app.doc.merge_view(|m| m.record += 1); }
    });
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    use crate::theme::{dialog, footer, primary, secondary, section};
    if let Some(mut d) = app.extras.mailings.envelope.take() {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        dialog(ctx, "Envelopes").open(&mut open).default_width(460.0).show(ctx, |ui| {
            app.canvas_focus = false;
            section(ui, "Delivery address");
            ui.add(egui::TextEdit::multiline(&mut d.delivery).desired_rows(4).desired_width(f32::INFINITY).hint_text(crate::i18n::tr("Name\nStreet\nCity, State ZIP")));
            section(ui, "Return address");
            ui.add(egui::TextEdit::multiline(&mut d.ret).desired_rows(3).desired_width(f32::INFINITY));
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Envelope size:")); crate::theme::Combo::from_id_salt("env-size").width(240.0).selected_text(crate::i18n::tr(&(ENVELOPES[d.size].0))).show_ui(ui, |ui| { for (k, e) in ENVELOPES.iter().enumerate() { ui.selectable_value(&mut d.size, k, crate::i18n::tr(&(e.0))); } }); });
            ui.label(RichText::new(crate::i18n::tr("Leave the delivery address empty to merge addresses into it.")).small().weak());
            footer(ui, |ui| { if primary(ui, "New Document").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok {
            let src = app.doc.mail_merge.source.clone();
            let mut env = word_engine::Document::envelope(d.size, &d.delivery, &d.ret);
            if d.delivery.trim().is_empty() {
                // A merge envelope: an Address Block where the delivery address goes.
                let last = env.paragraphs.len();
                let mut p = word_engine::Paragraph::new("");
                p.props.indent_left = Some(ENVELOPES[d.size].1 / 2 - 1440);
                p.props.space_before = Some(60.0);
                env.paragraphs.push(p);
                let at = env.paragraph_start(last);
                env.select(at, at);
                env.insert_field(Field::AddressBlock);
                if let Some(s) = src { env.set_merge_source(Some(s), None); }
                env.mail_merge.kind = MergeKind::Envelopes;
            }
            app.set_document(env, None);
            app.doc.dirty = true;
        } else if open && !cancel { app.extras.mailings.envelope = Some(d); } else { app.canvas_focus = true; }
    }
    if let Some(mut d) = app.extras.mailings.labels.take() {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        dialog(ctx, "Labels").open(&mut open).default_width(460.0).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::check(ui, &mut d.merge, crate::i18n::tr("Merge recipients into the labels (Mail Merge)"));
            ui.add_enabled_ui(!d.merge, |ui| {
                section(ui, "Address");
                ui.add(egui::TextEdit::multiline(&mut d.text).desired_rows(4).desired_width(f32::INFINITY));
            });
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Label:")); crate::theme::Combo::from_id_salt("label-spec").width(300.0).selected_text(crate::i18n::tr(&(LABELS[d.spec].0))).show_ui(ui, |ui| { for (k, l) in LABELS.iter().enumerate() { ui.selectable_value(&mut d.spec, k, crate::i18n::tr(&l.0)); } }); });
            let l = LABELS[d.spec];
            ui.label(RichText::new(format!("{} × {} labels, {:.2}\" × {:.2}\"", l.1, l.2, l.3 as f32 / 1440.0, l.4 as f32 / 1440.0)).small().weak());
            footer(ui, |ui| { if primary(ui, "New Document").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok {
            let src = app.doc.mail_merge.source.clone();
            let mut doc = word_engine::Document::labels(d.spec, if d.merge { "" } else { &d.text });
            if d.merge {
                if let Some(s) = src { doc.set_merge_source(Some(s), None); }
                let first = doc.tables[0].rows[0].cells[0].id;
                let p = doc.paragraph_start(doc.paragraphs.iter().position(|p| p.cell == Some(first)).unwrap_or(0));
                doc.select(p, p);
                doc.insert_field(Field::AddressBlock);
                doc.update_labels();
                doc.mail_merge.kind = MergeKind::Labels;
            }
            app.set_document(doc, None);
            app.doc.dirty = true;
        } else if open && !cancel { app.extras.mailings.labels = Some(d); } else { app.canvas_focus = true; }
    }
    recipients_dialog(app, ctx);
    if app.extras.mailings.address_block {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        dialog(ctx, "Insert Address Block").open(&mut open).default_width(440.0).show(ctx, |ui| {
            app.canvas_focus = false;
            section(ui, "Preview");
            let row = app.doc.mail_merge.included().get(app.doc.mail_merge.record).copied();
            egui::Frame::group(ui.style()).show(ui, |ui| { ui.set_min_width(380.0); ui.label(row.map(|r| app.doc.mail_merge.address(r)).unwrap_or_else(|| "(no recipients)".into())); });
            record_nav(app, ui);
            ui.label(RichText::new(crate::i18n::tr("Parts come from your columns; fix mismatches with Match Fields.")).small().weak());
            if ui.button(crate::i18n::tr("Match Fields…")).clicked() { app.extras.mailings.match_fields = true; }
            footer(ui, |ui| { if primary(ui, "OK").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok { app.insert_merge(Field::AddressBlock); }
        if !open || ok || cancel { app.extras.mailings.address_block = false; app.canvas_focus = true; }
    }
    if let Some(mut d) = app.extras.mailings.greeting.take() {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        dialog(ctx, "Insert Greeting Line").open(&mut open).default_width(440.0).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::hrow(ui, |ui| {
                crate::theme::Combo::from_id_salt("greet").width(80.0).selected_text(crate::i18n::tr(&d.greeting.clone())).show_ui(ui, |ui| { for g in ["Dear", "To", "Hello", "Hi"] { ui.selectable_value(&mut d.greeting, g.to_string(), crate::i18n::tr(&(g))); } });
                crate::theme::Combo::from_id_salt("greet-name").width(200.0).selected_text(crate::i18n::tr(&(NameFormat::ALL.iter().find(|n| n.0 == d.name).map(|n| n.1).unwrap_or("")))).show_ui(ui, |ui| { for (f, n) in NameFormat::ALL { ui.selectable_value(&mut d.name, f, crate::i18n::tr(&n)); } });
                crate::theme::Combo::from_id_salt("greet-punct").width(50.0).selected_text(crate::i18n::tr(&(if d.punctuation.is_empty() { "(none)".into() } else { d.punctuation.clone() }))).show_ui(ui, |ui| { for p in [",", ":", "!", ""] { ui.selectable_value(&mut d.punctuation, p.to_string(), crate::i18n::tr(&(if p.is_empty() { "(none)" } else { p }))); } });
            });
            section(ui, "Preview");
            let row = app.doc.mail_merge.included().get(app.doc.mail_merge.record).copied();
            ui.label(RichText::new(app.doc.mail_merge.greeting(row, &d.greeting, d.name, &d.punctuation)).size(15.0));
            record_nav(app, ui);
            footer(ui, |ui| { if primary(ui, "OK").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok { app.insert_merge(Field::GreetingLine { greeting: d.greeting.clone(), name: d.name, punctuation: d.punctuation.clone() }); }
        if open && !ok && !cancel { app.extras.mailings.greeting = Some(d); } else { app.canvas_focus = true; }
    }
    if let Some(mut d) = app.extras.mailings.rule.take() {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        let headers = app.doc.mail_merge.source.as_ref().map(|s| s.headers.clone()).unwrap_or_default();
        dialog(ctx, RULES[d.kind as usize].trim_end_matches('…')).open(&mut open).default_width(460.0).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::grid(ui, egui::Grid::new("rule").num_columns(2).spacing(egui::vec2(10.0, 8.0)), |ui| {
                ui.label(crate::i18n::tr("Field name:"));
                crate::theme::Combo::from_id_salt("rule-field").width(200.0).selected_text(crate::i18n::tr(&d.field.clone())).show_ui(ui, |ui| { for h in &headers { ui.selectable_value(&mut d.field, h.clone(), crate::i18n::tr(&h)); } });
                ui.end_row();
                ui.label(crate::i18n::tr("Comparison:"));
                crate::theme::Combo::from_id_salt("rule-op").width(200.0).selected_text(crate::i18n::tr(&(Compare::ALL.iter().find(|c| c.0 == d.op).map(|c| c.1).unwrap_or("")))).show_ui(ui, |ui| { for (c, n) in Compare::ALL { ui.selectable_value(&mut d.op, c, crate::i18n::tr(&n)); } });
                ui.end_row();
                if !matches!(d.op, Compare::IsBlank | Compare::IsNotBlank) { ui.label(crate::i18n::tr("Compare to:")); ui.text_edit_singleline(&mut d.value); ui.end_row(); }
                if d.kind == 0 {
                    ui.label(crate::i18n::tr("Insert this text:")); ui.text_edit_singleline(&mut d.then); ui.end_row();
                    ui.label(crate::i18n::tr("Otherwise insert:")); ui.text_edit_singleline(&mut d.otherwise); ui.end_row();
                }
            });
            footer(ui, |ui| { if primary(ui, "OK").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok {
            let value = if matches!(d.op, Compare::IsBlank | Compare::IsNotBlank) { String::new() } else { d.value.clone() };
            let f = match d.kind {
                0 => Field::MergeIf { field: d.field.clone(), op: d.op, value, then: d.then.clone(), otherwise: d.otherwise.clone() },
                4 => Field::NextIf { field: d.field.clone(), op: d.op, value },
                _ => Field::SkipIf { field: d.field.clone(), op: d.op, value },
            };
            app.insert_merge(f);
        }
        if open && !ok && !cancel { app.extras.mailings.rule = Some(d); } else { app.canvas_focus = true; }
    }
    if app.extras.mailings.match_fields {
        let (mut open, mut ok) = (true, false);
        let headers = app.doc.mail_merge.source.as_ref().map(|s| s.headers.clone()).unwrap_or_default();
        dialog(ctx, "Match Fields").open(&mut open).default_width(440.0).show(ctx, |ui| {
            app.canvas_focus = false;
            ui.label(RichText::new(crate::i18n::tr("Choose the column that holds each address part.")).weak());
            crate::theme::grid(ui, egui::Grid::new("match").num_columns(2).spacing(egui::vec2(12.0, 6.0)), |ui| {
                for (part, _) in ADDRESS_FIELDS {
                    ui.label(crate::i18n::tr(&*part));
                    let cur = app.doc.mail_merge.column_for(part).unwrap_or_default();
                    let mut pick = cur.clone();
                    crate::theme::Combo::from_id_salt(("match", *part)).width(200.0).selected_text(crate::i18n::tr(&(if cur.is_empty() { "(not matched)".to_string() } else { cur.clone() }))).show_ui(ui, |ui| {
                        ui.selectable_value(&mut pick, String::new(), crate::i18n::tr("(not matched)"));
                        for h in &headers { ui.selectable_value(&mut pick, h.clone(), crate::i18n::tr(&h)); }
                    });
                    if pick != cur { let p = part.to_string(); app.doc.merge_view(move |m| { m.map.insert(p, pick); }); }
                    ui.end_row();
                }
            });
            footer(ui, |ui| if primary(ui, "OK").clicked() { ok = true; });
        });
        if !open || ok { app.extras.mailings.match_fields = false; app.canvas_focus = true; }
    }
    if let Some(mut q) = app.extras.mailings.find.take() {
        let (mut open, mut next, mut close) = (true, false, false);
        dialog(ctx, "Find Entry").open(&mut open).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Find:")); let r = ui.text_edit_singleline(&mut q); if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { next = true; } });
            footer(ui, |ui| { if primary(ui, "Find Next").clicked() { next = true; } if secondary(ui, "Close").clicked() { close = true; } });
        });
        if next && !q.trim().is_empty() {
            let order = app.doc.mail_merge.included();
            let src = app.doc.mail_merge.source.clone().unwrap_or_default();
            let start = app.doc.mail_merge.record + 1;
            let ql = q.to_lowercase();
            let hit = (0..order.len()).map(|k| (start + k) % order.len().max(1)).find(|&k| src.rows.get(order[k]).is_some_and(|r| r.iter().any(|c| c.to_lowercase().contains(&ql))));
            match hit { Some(k) => app.doc.merge_view(|m| { m.preview = true; m.record = k; }), None => app.notify(ctx, "No recipient matches.") }
        }
        if open && !close { app.extras.mailings.find = Some(q); } else { app.canvas_focus = true; }
    }
    if let Some(errs) = app.extras.mailings.errors.take() {
        let (mut open, mut ok) = (true, false);
        dialog(ctx, "Check for Errors").open(&mut open).default_width(420.0).show(ctx, |ui| {
            if errs.is_empty() { ui.label(crate::i18n::tr("No mail merge errors have been found.")); }
            for e in &errs { ui.label(format!("• {e}")); }
            footer(ui, |ui| if primary(ui, "OK").clicked() { ok = true; });
        });
        if open && !ok { app.extras.mailings.errors = Some(errs); }
    }
    if let Some(mut d) = app.extras.mailings.finish.take() {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        let headers = app.doc.mail_merge.source.as_ref().map(|s| s.headers.clone()).unwrap_or_default();
        let title = ["Merge to New Document", "Merge to PDF", "Merge to E-mail"][d.target as usize];
        dialog(ctx, title).open(&mut open).default_width(420.0).show(ctx, |ui| {
            app.canvas_focus = false;
            if d.target == 2 {
                crate::theme::grid(ui, egui::Grid::new("email").num_columns(2), |ui| {
                    ui.label(crate::i18n::tr("To:"));
                    crate::theme::Combo::from_id_salt("email-to").width(200.0).selected_text(crate::i18n::tr(&d.email_field.clone())).show_ui(ui, |ui| { for h in &headers { ui.selectable_value(&mut d.email_field, h.clone(), crate::i18n::tr(&h)); } });
                    ui.end_row();
                    ui.label(crate::i18n::tr("Subject line:")); ui.text_edit_singleline(&mut d.subject); ui.end_row();
                });
                ui.label(RichText::new(crate::i18n::tr("Messages are saved as .eml files you can open and send from your mail app.")).small().weak());
            }
            section(ui, "Merge records");
            crate::theme::radio_value(ui, &mut d.range, 0, crate::i18n::tr("All"));
            crate::theme::radio_value(ui, &mut d.range, 1, crate::i18n::tr("Current record"));
            crate::theme::hrow(ui, |ui| { crate::theme::radio_value(ui, &mut d.range, 2, crate::i18n::tr("From:")); ui.add(egui::DragValue::new(&mut d.from).range(1..=9999)); ui.label(crate::i18n::tr("To:")); ui.add(egui::DragValue::new(&mut d.to).range(1..=9999)); });
            footer(ui, |ui| { if primary(ui, "OK").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
        });
        if ok { app.finish(&d); }
        if open && !ok && !cancel { app.extras.mailings.finish = Some(d); } else { app.canvas_focus = true; }
    }
}

fn recipients_dialog(app: &mut WordApp, ctx: &egui::Context) {
    use crate::theme::{dialog, footer, primary, secondary};
    let Some(mut d) = app.extras.mailings.recipients.take() else { return };
    let Some(mut src) = app.doc.mail_merge.source.clone() else { return };
    let mut excluded = app.doc.mail_merge.excluded.clone();
    let (mut open, mut ok, mut save, mut cancel) = (true, false, false, false);
    let title = if d.new_list { "New Address List" } else { "Mail Merge Recipients" };
    dialog(ctx, title).open(&mut open).resizable(true).default_width(760.0).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hrow(ui, |ui| {
            ui.label(RichText::new(crate::i18n::tr(&src.name)).strong());
            ui.label(RichText::new(crate::i18n::trf("{} recipients, {} selected", &[&src.rows.len(), &(src.rows.len() - excluded.len())])).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.add(egui::TextEdit::singleline(&mut d.filter).hint_text(crate::i18n::tr("Filter")).desired_width(160.0)); });
        });
        let mut order: Vec<usize> = (0..src.rows.len()).filter(|&r| d.filter.trim().is_empty() || src.rows[r].iter().any(|c| c.to_lowercase().contains(&d.filter.to_lowercase()))).collect();
        if let Some((c, asc)) = d.sort { order.sort_by(|a, b| { let o = src.rows[*a].get(c).cmp(&src.rows[*b].get(c)); if asc { o } else { o.reverse() } }); }
        let mut remove = None;
        egui::ScrollArea::both().max_height(340.0).show(ui, |ui| {
            crate::theme::grid(ui, egui::Grid::new("recipients").striped(true).spacing(egui::vec2(6.0, 4.0)), |ui| {
                let mut all = excluded.is_empty();
                if crate::theme::check(ui, &mut all, "").changed() { if all { excluded.clear(); } else { excluded = (0..src.rows.len()).collect(); } }
                for (k, h) in src.headers.iter().enumerate() {
                    let arrow = match d.sort { Some((c, true)) if c == k => " ▲", Some((c, false)) if c == k => " ▼", _ => "" };
                    if ui.add(egui::Button::new(RichText::new(format!("{h}{arrow}")).strong()).frame(false)).on_hover_text(crate::i18n::tr("Sort")).clicked() { d.sort = Some((k, d.sort != Some((k, true)))); }
                }
                ui.label("");
                ui.end_row();
                for &r in &order {
                    let mut on = !excluded.contains(&r);
                    if crate::theme::check(ui, &mut on, "").changed() { if on { excluded.remove(&r); } else { excluded.insert(r); } }
                    for c in 0..src.headers.len() { ui.add(egui::TextEdit::singleline(&mut src.rows[r][c]).desired_width(110.0)); }
                    if ui.small_button("✕").on_hover_text(crate::i18n::tr("Delete entry")).clicked() { remove = Some(r); }
                    ui.end_row();
                }
            });
        });
        if let Some(r) = remove { src.rows.remove(r); excluded = std::mem::take(&mut excluded).into_iter().filter(|x| *x != r).map(|x| if x > r { x - 1 } else { x }).collect(); }
        crate::theme::hrow(ui, |ui| {
            if ui.button(crate::i18n::tr("New Entry")).clicked() { src.rows.push(vec![String::new(); src.headers.len()]); }
            if ui.button(crate::i18n::tr("Save List as CSV…")).clicked() { save = true; }
        });
        footer(ui, |ui| { if primary(ui, "OK").clicked() { ok = true; } if secondary(ui, "Cancel").clicked() { cancel = true; } });
    });
    if save {
        if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("Recipients.csv").save_file() {
            if let Err(e) = std::fs::write(&p, src.to_csv()) { app.dialogs.error = Some(e.to_string()); } else { src.name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(); app.doc.mail_merge.path = Some(p.display().to_string()); }
        }
    }
    // Edits apply as they're made; the list stays with the document while it's open.
    let path = app.doc.mail_merge.path.clone();
    app.doc.merge_view(|m| { m.source = Some(src); m.path = path; m.excluded = excluded; });
    if open && !ok && !cancel { app.extras.mailings.recipients = Some(d); } else { app.canvas_focus = true; }
}
