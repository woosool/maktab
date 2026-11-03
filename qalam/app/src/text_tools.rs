//! Insert ▸ Text: text boxes, WordArt, drop caps, Date & Time, the signature line, fields and
//! Quick Parts (AutoText, document properties), icons, and Text from File.

use crate::app::WordApp;
use egui::{vec2, Color32, RichText, Sense, Ui};
use word_engine::format::DropCap;
use word_engine::model::{Anchor, AnchorAlign, Field, Wrap};
use word_engine::objects::{Object, Shape};
use word_engine::{Color, RunProps};

pub struct DropCapDraft { pub position: u8, pub font: String, pub lines: u8, pub distance: f32 }
pub struct DateTimeDraft { pub selected: usize, pub update: bool }
pub struct SignatureDraft { pub signer: String, pub title: String, pub email: String, pub instructions: String, pub show_date: bool }
pub struct FieldDraft { pub selected: usize, pub format: usize }
pub struct IconsDialog { pub query: String, pub selected: Option<&'static str> }

/// Text box designs: (name, fill, outline, text color, italic, size).
pub const TEXT_BOXES: &[(&str, Option<Color>, Option<Color>, Color, bool, f32)] = &[
    ("Simple Text Box", Some(Color::WHITE), Some(Color(0, 0, 0)), Color(0, 0, 0), false, 11.0),
    ("Sidebar", Some(Color(0xDE, 0xEA, 0xF6)), None, Color(0x1F, 0x38, 0x64), false, 11.0),
    ("Quote", None, None, Color(0x44, 0x72, 0xC4), true, 14.0),
    ("Banded", Some(Color(0x44, 0x72, 0xC4)), None, Color::WHITE, false, 12.0),
    ("Outlined", Some(Color::WHITE), Some(Color(0x44, 0x72, 0xC4)), Color(0x26, 0x26, 0x26), false, 11.0),
];

/// WordArt styles: (name, run properties).
pub fn word_art_styles() -> Vec<(&'static str, RunProps)> {
    let base = |c: Color| RunProps { size: Some(36.0), color: Some(Some(c)), ..Default::default() };
    vec![
        ("Fill: Black, Shadow", RunProps { shadow: Some(true), ..base(Color(0, 0, 0)) }),
        ("Fill: Blue, Accent 1; Shadow", RunProps { shadow: Some(true), bold: Some(true), ..base(Color(0x44, 0x72, 0xC4)) }),
        ("Fill: White; Outline: Blue", RunProps { outline: Some(true), ..base(Color(0x44, 0x72, 0xC4)) }),
        ("Gradient Fill: Blue to Purple", RunProps { gradient: Some(Some((Color(0x2F, 0x6F, 0xE0), Color(0x8E, 0x3C, 0xC9)))), bold: Some(true), ..base(Color(0x2F, 0x6F, 0xE0)) }),
        ("Fill: Orange; Glow", RunProps { glow: Some(Some(word_engine::format::Glow { color: Color(0xFF, 0xC0, 0x00), radius: 8.0 })), ..base(Color(0xED, 0x7D, 0x31)) }),
        ("Fill: Gray; Reflection", RunProps { reflection: Some(true), ..base(Color(0x59, 0x59, 0x59)) }),
        ("Gradient Fill: Gold", RunProps { gradient: Some(Some((Color(0xFF, 0xD9, 0x66), Color(0xBF, 0x8F, 0x00)))), bold: Some(true), shadow: Some(true), ..base(Color(0xBF, 0x8F, 0x00)) }),
        ("Fill: Red; Bold Caps", RunProps { bold: Some(true), caps: Some(word_engine::format::Caps::AllCaps), ..base(Color(0xC0, 0, 0)) }),
    ]
}

/// Field kinds for Quick Parts ▸ Field: (name, description, field).
pub fn field_kinds() -> Vec<(&'static str, &'static str, Field)> {
    vec![
        ("Page", "The current page number", Field::Page),
        ("NumPages", "The number of pages", Field::NumPages),
        ("Date", "Today's date", Field::Date("M/d/yyyy".into())),
        ("Time", "The current time", Field::Date("TIME:h:mm am/pm".into())),
        ("Author", "The document's author", Field::Prop("AUTHOR".into())),
        ("Title", "The document's title", Field::Prop("TITLE".into())),
        ("Subject", "The document's subject", Field::Prop("SUBJECT".into())),
        ("Keywords", "The document's keywords", Field::Prop("KEYWORDS".into())),
        ("FileName", "The document's file name", Field::Prop("FILENAME".into())),
        ("NumWords", "The number of words", Field::Prop("NUMWORDS".into())),
        ("NumChars", "The number of characters", Field::Prop("NUMCHARS".into())),
    ]
}

fn quick_parts_path() -> Option<std::path::PathBuf> { Some(crate::config_dir().join("quickparts.json")) }

pub fn load_quick_parts() -> Vec<(String, String)> {
    quick_parts_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Vec<(String, String)>>(&t).ok()).unwrap_or_default()
}

impl WordApp {
    /// Insert a text box design floating at the caret's paragraph; its text is edited in the
    /// Format Shape dialog.
    pub fn insert_text_box(&mut self, design: usize) {
        let (name, fill, outline, color, italic, size) = TEXT_BOXES[design.min(TEXT_BOXES.len() - 1)];
        let mut s = Shape::text_box(if design == 2 { "“Grab your reader’s attention with a great quote from the document.”" } else { "Type your text here." });
        s.fill = fill; s.outline = outline; s.text_color = color; s.italic = italic; s.font_size = size;
        let g = self.doc.page.geometry();
        let content = g.width - g.left - g.right;
        let mut a = Anchor::new(Wrap::Square, 0.0, 0.0);
        match name {
            "Sidebar" => { s.width = 168.0; s.height = 384.0; a.x_align = Some(AnchorAlign::End); }
            "Quote" => { s.width = content * 0.6; s.height = 96.0; a.x_align = Some(AnchorAlign::Center); a.wrap = Wrap::TopBottom; }
            "Banded" => { s.width = content; s.height = 72.0; a.wrap = Wrap::TopBottom; }
            _ => { a.x = (content - s.width) / 2.0; }
        }
        self.doc.insert_object(Object::Shape(s));
        let pos = self.doc.range().0;
        self.doc.set_anchor(pos, Some(a));
        self.edit_object(pos);
    }

    /// Insert WordArt: styled text, selected so typing replaces it.
    pub fn insert_word_art(&mut self, style: usize) {
        let styles = word_art_styles();
        let (_, props) = styles[style.min(styles.len() - 1)].clone();
        let text = if self.doc.selected_text().trim().is_empty() { "Your text here".to_string() } else { self.doc.selected_text() };
        let s = self.doc.range().0;
        self.doc.replace(&text);
        let e = s + text.chars().count();
        self.doc.select(s, e);
        self.doc.set_run_props(props);
        self.canvas_focus = true;
    }

    pub fn open_drop_cap(&mut self) {
        let pi = self.doc.locate(self.doc.range().0).0;
        let existing = self.doc.drop_cap_of(pi);
        self.extras.drop_cap = Some(DropCapDraft { position: existing.map(|c| if c.1.margin { 2 } else { 1 }).unwrap_or(1), font: self.doc.current_format().font.clone(), lines: existing.map(|c| c.1.lines).unwrap_or(3), distance: existing.map(|c| c.1.distance as f32 / 1440.0).unwrap_or(0.0) });
    }

    /// Drop Cap gallery choice: 0 none, 1 dropped, 2 in margin.
    pub fn apply_drop_cap(&mut self, position: u8, lines: u8, distance_in: f32, font: Option<String>) {
        let pi = self.doc.locate(self.doc.range().0).0;
        let cap = (position > 0).then(|| (DropCap { margin: position == 2, lines: lines.clamp(1, 10), distance: (distance_in * 1440.0).round() as i32 }, font));
        self.doc.set_drop_cap(pi, cap);
    }

    pub fn open_date_time(&mut self) { self.extras.date_time = Some(DateTimeDraft { selected: 1, update: false }); }
    pub fn open_signature(&mut self) { self.extras.signature = Some(SignatureDraft { signer: String::new(), title: String::new(), email: String::new(), instructions: "Before signing this document, verify that the content you are signing is correct.".into(), show_date: true }); }
    pub fn open_field(&mut self) { self.extras.field = Some(FieldDraft { selected: 0, format: 0 }); }
    pub fn open_icons(&mut self, _ctx: &egui::Context) { self.extras.icons = Some(IconsDialog { query: String::new(), selected: None }); }

    /// Insert a signature line: space to sign over a line, then the signer's name and title.
    pub fn insert_signature(&mut self, d: &SignatureDraft) {
        use word_engine::format::{Border, ParaBorders, ParaProps};
        if !self.doc.current_paragraph().is_empty() { self.doc.insert_paragraph(); }
        self.doc.insert_paragraph();
        let g = self.doc.page.geometry();
        let right = (((g.width - g.left - g.right) - 288.0) * 15.0).max(0.0) as i32;
        self.doc.replace(if d.show_date { "X\t" } else { "X" });
        self.doc.set_para_props(ParaProps { borders: Some(ParaBorders { bottom: Some(Border { width: 0.75, space: 1.0, color: Some(Color(0, 0, 0)) }), ..Default::default() }), indent_right: Some(right), space_after: Some(0.0), ..Default::default() });
        for line in [d.signer.as_str(), d.title.as_str(), d.email.as_str()] {
            if line.trim().is_empty() { continue; }
            self.doc.insert_paragraph();
            self.doc.set_para_props(ParaProps { borders: Some(ParaBorders::default()), indent_right: Some(0), space_after: Some(0.0), ..Default::default() });
            self.doc.replace(line);
        }
        self.doc.insert_paragraph();
        self.doc.set_para_props(ParaProps { borders: Some(ParaBorders::default()), indent_right: Some(0), space_after: None, ..Default::default() });
    }

    /// Insert another document's content at the caret (Object ▸ Text from File).
    pub fn insert_text_from_file(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Documents", &["docx", "dotx", "docm", "md", "markdown", "txt"]).pick_file() else { return };
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let read = std::fs::read(&p).map_err(|e| e.to_string()).and_then(|bytes| match ext.as_str() {
            "md" | "markdown" => Ok(word_engine::import::from_markdown(&String::from_utf8_lossy(&bytes))),
            "txt" => Ok(word_engine::import::from_text(&String::from_utf8_lossy(&bytes))),
            _ => word_engine::docx::read(&bytes),
        });
        match read {
            Ok(other) => {
                for (k, a) in other.assets { self.doc.assets.entry(k).or_insert(a); }
                let paras: Vec<_> = other.paragraphs.into_iter().filter(|p| p.cell.is_none()).collect();
                self.doc.paste_fragment(paras);
                self.scroll_to_caret = true;
            }
            Err(e) => self.dialogs.error = Some(crate::i18n::trf("Couldn't insert the file.\n\n{}", &[&e])),
        }
    }

    /// Save the selection as an AutoText entry (Quick Parts gallery).
    pub fn save_quick_part(&mut self) {
        let text = self.doc.selected_text();
        if text.trim().is_empty() { return; }
        let name: String = text.split_whitespace().take(4).collect::<Vec<_>>().join(" ");
        self.extras.quick_parts.retain(|q| q.0 != name);
        self.extras.quick_parts.push((name, text));
        if let (Some(p), Ok(json)) = (quick_parts_path(), serde_json::to_string(&self.extras.quick_parts)) {
            if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
            let _ = std::fs::write(p, json);
        }
    }
}

/// Text Box ▾ gallery.
pub fn text_box_menu(app: &mut WordApp, ui: &mut Ui) {
    crate::theme::section(ui, "Built-In");
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(330.0);
        for (k, (name, fill, outline, color, italic, _)) in TEXT_BOXES.iter().enumerate() {
            let (rect, r) = ui.allocate_exact_size(vec2(100.0, 78.0), Sense::click());
            ui.painter().rect_filled(rect, 2.0, if r.hovered() { Color32::from_gray(232) } else { Color32::WHITE });
            let page = rect.shrink2(vec2(18.0, 6.0));
            ui.painter().rect_stroke(page, 0.0, egui::Stroke::new(1.0, Color32::from_gray(200)), egui::StrokeKind::Inside);
            let b = egui::Rect::from_center_size(page.center(), vec2(page.width() * 0.75, page.height() * 0.5));
            let c = |c: Color| Color32::from_rgb(c.0, c.1, c.2);
            ui.painter().rect(b, 1.0, fill.map(c).unwrap_or(Color32::TRANSPARENT), egui::Stroke::new(1.0, outline.map(c).unwrap_or(Color32::TRANSPARENT)), egui::StrokeKind::Inside);
            ui.painter().text(b.center(), egui::Align2::CENTER_CENTER, if *italic { "“Quote”" } else { "Text" }, egui::FontId::proportional(9.0), c(*color));
            if r.on_hover_text(crate::i18n::tr(&*name)).clicked() { app.insert_text_box(k); ui.close(); }
        }
    });
    ui.separator();
    if crate::ribbon::small(ui, "textBox", Some("Draw Text Box"), false, true, "Insert a simple text box").clicked() { app.insert_text_box(0); ui.close(); }
}

/// WordArt ▾ gallery: each style's "A" drawn in its colors.
pub fn word_art_menu(app: &mut WordApp, ui: &mut Ui) {
    crate::theme::grid(ui, egui::Grid::new("wordart").spacing(vec2(4.0, 4.0)), |ui| {
        for (k, (name, p)) in word_art_styles().into_iter().enumerate() {
            let (rect, r) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
            if r.hovered() { ui.painter().rect_filled(rect, 3.0, Color32::from_gray(232)); }
            let color = p.gradient.flatten().map(|g| g.0).or(p.color.flatten()).unwrap_or(Color(0, 0, 0));
            let c = Color32::from_rgb(color.0, color.1, color.2);
            if p.shadow == Some(true) { ui.painter().text(rect.center() + vec2(1.5, 1.5), egui::Align2::CENTER_CENTER, "A", egui::FontId::new(30.0, egui::FontFamily::Name("ui-bold".into())), Color32::from_gray(160)); }
            if let Some(Some(g)) = p.glow { ui.painter().circle_filled(rect.center(), 16.0, Color32::from_rgba_unmultiplied(g.color.0, g.color.1, g.color.2, 70)); }
            if p.outline == Some(true) {
                for d in [vec2(-1.0, 0.0), vec2(1.0, 0.0), vec2(0.0, -1.0), vec2(0.0, 1.0)] { ui.painter().text(rect.center() + d, egui::Align2::CENTER_CENTER, "A", egui::FontId::new(30.0, egui::FontFamily::Name("ui-bold".into())), c); }
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "A", egui::FontId::new(30.0, egui::FontFamily::Name("ui-bold".into())), Color32::WHITE);
            } else {
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "A", egui::FontId::new(30.0, egui::FontFamily::Name("ui-bold".into())), c);
            }
            if r.on_hover_text(crate::i18n::tr(&name)).clicked() { app.insert_word_art(k); ui.close(); }
            if k % 4 == 3 { ui.end_row(); }
        }
    });
}

/// Drop Cap ▾.
pub fn drop_cap_menu(app: &mut WordApp, ui: &mut Ui) {
    let pi = app.doc.locate(app.doc.range().0).0;
    let cur = app.doc.drop_cap_of(pi).map(|c| if c.1.margin { 2 } else { 1 }).unwrap_or(0);
    for (k, (icon, name)) in [("dropCap", "None"), ("dropCap", "Dropped"), ("dropCap", "In Margin")].iter().enumerate() {
        if crate::ribbon::small(ui, icon, Some(name), cur == k as u8, true, name).clicked() { app.apply_drop_cap(k as u8, 3, 0.0, None); ui.close(); }
    }
    ui.separator();
    if ui.button(crate::i18n::tr("Drop Cap Options…")).clicked() { app.open_drop_cap(); ui.close(); }
}

/// Quick Parts ▾.
pub fn quick_parts_menu(app: &mut WordApp, ui: &mut Ui) {
    if !app.extras.quick_parts.is_empty() {
        crate::theme::section(ui, "AutoText");
        for (name, text) in app.extras.quick_parts.clone() {
            if ui.button(crate::i18n::tr(&name)).on_hover_text(&text).clicked() { app.doc.replace(&text); ui.close(); }
        }
        ui.separator();
    }
    crate::theme::submenu(ui, crate::i18n::tr("Document Property"), |ui| {
        for (name, key) in [("Author", "AUTHOR"), ("Title", "TITLE"), ("Subject", "SUBJECT"), ("Keywords", "KEYWORDS"), ("File Name", "FILENAME")] {
            if ui.button(crate::i18n::tr(&name)).clicked() { app.doc.insert_field(Field::Prop(key.into())); ui.close(); }
        }
    });
    if ui.button(crate::i18n::tr("Field…")).clicked() { app.open_field(); ui.close(); }
    ui.separator();
    if ui.add_enabled(!app.doc.selected_text().trim().is_empty(), egui::Button::new(crate::i18n::tr("Save Selection to Quick Part Gallery…"))).clicked() { app.save_quick_part(); ui.close(); }
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    drop_cap_dialog(app, ctx);
    date_time_dialog(app, ctx);
    signature_dialog(app, ctx);
    field_dialog(app, ctx);
    icons_dialog(app, ctx);
}

fn drop_cap_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.drop_cap.take() else { return };
    let (mut open, mut ok, cancel) = (true, false, std::cell::Cell::new(false));
    crate::theme::dialog(ctx, "Drop Cap").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::section(ui, "Position");
        crate::theme::hrow(ui, |ui| { for (k, n) in ["None", "Dropped", "In margin"].iter().enumerate() { ui.selectable_value(&mut d.position, k as u8, crate::i18n::tr(&*n)); } });
        ui.separator();
        crate::theme::grid(ui, egui::Grid::new("dropcap").num_columns(2), |ui| {
            ui.label(crate::i18n::tr("Font:"));
            crate::theme::Combo::from_id_salt("dc-font").selected_text(crate::i18n::tr(&d.font.clone())).show_ui(ui, |ui| { for n in word_engine::fonts::family_names() { ui.selectable_value(&mut d.font, n.clone(), n.as_str()); } });
            ui.end_row();
            ui.label(crate::i18n::tr("Lines to drop:")); ui.add(egui::DragValue::new(&mut d.lines).range(1..=10)); ui.end_row();
            ui.label(crate::i18n::tr("Distance from text:")); ui.add(egui::DragValue::new(&mut d.distance).range(0.0..=1.0).speed(0.01).suffix("\"")); ui.end_row();
        });
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel.set(true); } });
    });
    if ok { app.apply_drop_cap(d.position, d.lines, d.distance, Some(d.font.clone())); app.canvas_focus = true; }
    else if open { app.extras.drop_cap = Some(d); } else { app.canvas_focus = true; }
}

fn date_time_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.date_time.take() else { return };
    let (mut open, mut ok, cancel) = (true, false, std::cell::Cell::new(false));
    let now = (app.doc.local_clock)();
    crate::theme::dialog(ctx, "Date and Time").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        ui.label(crate::i18n::tr("Available formats:"));
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (k, pic) in word_engine::fields::DATE_FORMATS.iter().enumerate() {
                let r = ui.selectable_label(d.selected == k, crate::i18n::tr(&(word_engine::fields::format_date(&now, pic))));
                if r.clicked() { d.selected = k; }
                if r.double_clicked() { d.selected = k; ok = true; }
            }
        });
        crate::theme::check(ui, &mut d.update, crate::i18n::tr("Update automatically"));
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel.set(true); } });
    });
    if ok {
        let pic = word_engine::fields::DATE_FORMATS[d.selected];
        // Time-only pictures are TIME fields.
        let is_time = !pic.contains('d') && !pic.contains('y') && !pic.contains('M');
        if d.update { app.doc.insert_field(Field::Date(if is_time { format!("TIME:{pic}") } else { pic.to_string() })); }
        else { app.doc.replace(&word_engine::fields::format_date(&now, pic)); }
        app.canvas_focus = true;
    } else if open && !cancel.get() { app.extras.date_time = Some(d); } else { app.canvas_focus = true; }
}

fn signature_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.signature.take() else { return };
    let (mut open, mut ok, cancel) = (true, false, std::cell::Cell::new(false));
    crate::theme::dialog(ctx, "Signature Setup").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::grid(ui, egui::Grid::new("sig").num_columns(2), |ui| {
            ui.label(crate::i18n::tr("Suggested signer:")); ui.text_edit_singleline(&mut d.signer); ui.end_row();
            ui.label(crate::i18n::tr("Suggested signer's title:")); ui.text_edit_singleline(&mut d.title); ui.end_row();
            ui.label(crate::i18n::tr("Suggested signer's e-mail:")); ui.text_edit_singleline(&mut d.email); ui.end_row();
            ui.label(crate::i18n::tr("Instructions to the signer:")); ui.add(egui::TextEdit::multiline(&mut d.instructions).desired_rows(2)); ui.end_row();
        });
        crate::theme::check(ui, &mut d.show_date, crate::i18n::tr("Show sign date in signature line"));
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel.set(true); } });
    });
    if ok { app.insert_signature(&d); app.canvas_focus = true; }
    else if open { app.extras.signature = Some(d); } else { app.canvas_focus = true; }
}

fn field_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.field.take() else { return };
    let (mut open, mut ok, cancel) = (true, false, std::cell::Cell::new(false));
    let kinds = field_kinds();
    crate::theme::dialog(ctx, "Field").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hcols(ui, |ui| {
            crate::theme::vcol(ui, |ui| {
                ui.label(crate::i18n::tr("Field names:"));
                egui::ScrollArea::vertical().id_salt("fields").max_height(260.0).show(ui, |ui| {
                    for (k, (name, _, _)) in kinds.iter().enumerate() {
                        let r = ui.selectable_label(d.selected == k, crate::i18n::tr(&*name));
                        if r.clicked() { d.selected = k; }
                        if r.double_clicked() { ok = true; }
                    }
                });
            });
            ui.separator();
            crate::theme::vcol(ui, |ui| {
                let (_, desc, f) = &kinds[d.selected];
                ui.label(RichText::new(crate::i18n::tr(&*desc)).weak());
                if matches!(f, Field::Date(_)) {
                    ui.label(crate::i18n::tr("Date formats:"));
                    let now = (app.doc.local_clock)();
                    for (k, pic) in word_engine::fields::DATE_FORMATS.iter().enumerate().take(10) { crate::theme::radio_value(ui, &mut d.format, k, word_engine::fields::format_date(&now, pic)); }
                }
            });
        });
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel.set(true); } });
    });
    if ok {
        let mut f = kinds[d.selected].2.clone();
        if let Field::Date(p) = &mut f { let pic = word_engine::fields::DATE_FORMATS[d.format]; *p = if p.starts_with("TIME:") { format!("TIME:{pic}") } else { pic.to_string() }; }
        app.doc.insert_field(f);
        app.canvas_focus = true;
    } else if open && !cancel.get() { app.extras.field = Some(d); } else { app.canvas_focus = true; }
}

/// Rasterize a command icon to PNG for inserting as a picture.
fn icon_png(name: &str, px: u32) -> Option<Vec<u8>> {
    let (_, _, large) = crate::icon_data::ICONS.iter().find(|e| e.0 == name)?;
    let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="{px}" height="{px}" color="#2c2c2e"><defs>{}</defs>{large}</svg>"##, crate::icon_data::DEFS);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

fn icons_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.icons.take() else { return };
    let (mut open, mut ok, cancel) = (true, false, std::cell::Cell::new(false));
    crate::theme::dialog(ctx, "Insert Icons").open(&mut open).collapsible(false).resizable(false).default_width(560.0).show(ctx, |ui| {
        app.canvas_focus = false;
        ui.add(egui::TextEdit::singleline(&mut d.query).hint_text(crate::i18n::tr("Search icons")).desired_width(f32::INFINITY));
        let q = d.query.to_lowercase();
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            crate::theme::grid(ui, egui::Grid::new("icons").spacing(vec2(4.0, 4.0)), |ui| {
                let mut k = 0;
                for (name, _, _) in crate::icon_data::ICONS.iter().filter(|e| q.is_empty() || e.0.to_lowercase().contains(&q)) {
                    let (rect, r) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
                    let sel = d.selected == Some(*name);
                    if sel || r.hovered() { ui.painter().rect_filled(rect, 4.0, if sel { Color32::from_rgb(0xCC, 0xE0, 0xFF) } else { Color32::from_gray(236) }); }
                    crate::ribbon::paint_icon(ui, name, rect.shrink(8.0), 32, true);
                    if r.clicked() { d.selected = Some(name); }
                    if r.double_clicked() { d.selected = Some(name); ok = true; }
                    k += 1;
                    if k % 10 == 0 { ui.end_row(); }
                }
            });
        });
        crate::theme::footer(ui, |ui| { if crate::theme::primary_enabled(ui, d.selected.is_some(), "Insert").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel.set(true); } });
    });
    if let (true, Some(name)) = (ok, d.selected) {
        if let Some(png) = icon_png(name, 192) { app.doc.insert_image(png, "image/png", 96.0, 96.0); }
        app.canvas_focus = true;
    } else if open && !cancel.get() { app.extras.icons = Some(d); } else { app.canvas_focus = true; }
}
