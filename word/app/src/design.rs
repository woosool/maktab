//! The Design tab's dialogs and defaults: custom watermarks, page borders, and Set as Default
//! (the document defaults and theme choices new documents start with).

use crate::app::WordApp;
use egui::{Color32};
use word_engine::design::Watermark;
use word_engine::page::{BorderStyle, PageBorders};
use word_engine::Color;

pub struct WatermarkDraft { pub kind: u8, pub text: String, pub font: String, pub size: f32, pub color: Color, pub semi: bool, pub diagonal: bool, pub picture: Option<(Vec<u8>, f32, f32)>, pub washout: bool }
pub struct PageBorderDraft { pub setting: u8, pub style: BorderStyle, pub color: Color, pub width: f32, pub space: f32, pub from_text: bool, pub sides: [bool; 4] }

/// Defaults saved by Set as Default: body font and size, heading font, paragraph spacing.
#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
pub struct Defaults { pub body_font: Option<String>, pub size: Option<f32>, pub heading_font: Option<String>, pub after: Option<f32>, pub line: Option<f32> }

fn defaults_path() -> Option<std::path::PathBuf> { Some(crate::config_dir().join("defaults.json")) }

pub fn load_defaults() -> Option<Defaults> { defaults_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()) }

/// Apply saved defaults to a new document (without an undo step).
pub fn apply_defaults(doc: &mut word_engine::Document, d: &Defaults) {
    if let Some(f) = &d.body_font { doc.styles.doc_run.font = Some(f.clone()); }
    if let Some(s) = d.size { doc.styles.doc_run.size = Some(s); }
    if let Some(a) = d.after { doc.styles.doc_para.space_after = Some(a); }
    if let Some(l) = d.line { doc.styles.doc_para.line_spacing = Some(l); }
    if d.body_font.is_some() || d.size.is_some() || d.after.is_some() || d.line.is_some() { doc.style_edits.insert(String::new()); doc.styles_changed = true; }
    if let Some(h) = &d.heading_font {
        for id in ["Title", "Heading1", "Heading2", "Heading3"] {
            if let Some(s) = doc.styles.styles.get_mut(id) { s.run.font = Some(h.clone()); doc.style_edits.insert(id.into()); doc.styles_changed = true; }
        }
    }
    doc.format_epoch += 1;
    doc.dirty = false;
}

impl WordApp {
    pub fn open_watermark(&mut self) {
        let existing = self.doc.watermark().and_then(|r| if let Some(word_engine::objects::Object::Shape(s)) = &r.object { Some(s.text.clone()) } else { None });
        self.extras.watermark = Some(WatermarkDraft { kind: if existing.is_some() { 2 } else { 2 }, text: existing.unwrap_or_else(|| "CONFIDENTIAL".into()), font: "Calibri".into(), size: 0.0, color: Color(0xC0, 0xC0, 0xC0), semi: true, diagonal: true, picture: None, washout: true });
    }
    pub fn open_page_borders(&mut self) {
        let b = self.doc.page.borders.clone();
        self.extras.page_borders = Some(match b {
            Some(b) => PageBorderDraft { setting: 1, style: b.style, color: b.color, width: b.width, space: b.space, from_text: b.from_text, sides: b.sides },
            None => PageBorderDraft { setting: 1, style: BorderStyle::Single, color: Color(0, 0, 0), width: 0.5, space: 24.0, from_text: false, sides: [true; 4] },
        });
    }
    pub fn text_watermark(&mut self, text: &str, diagonal: bool) {
        self.doc.set_watermark(Some(Watermark::Text { text: text.into(), font: "Calibri".into(), size: 0.0, color: Color(0xC0, 0xC0, 0xC0), diagonal }));
    }
    /// Save the current document defaults as the defaults for new documents.
    pub fn set_as_default(&mut self, ctx: &egui::Context) {
        let s = &self.doc.styles;
        let d = Defaults {
            body_font: s.doc_run.font.clone(), size: s.doc_run.size, after: s.doc_para.space_after, line: s.doc_para.line_spacing,
            heading_font: s.styles.get("Heading1").and_then(|h| h.run.font.clone()),
        };
        if let (Some(p), Ok(json)) = (defaults_path(), serde_json::to_string_pretty(&d)) {
            if let Some(dir) = p.parent() { let _ = std::fs::create_dir_all(dir); }
            match std::fs::write(&p, json) { Ok(()) => self.notify(ctx, "New documents will use these settings"), Err(e) => self.dialogs.error = Some(crate::i18n::trf("Couldn't save the defaults.\n\n{}", &[&e])) }
        }
    }
}

fn color_button(ui: &mut egui::Ui, c: &mut Color) -> bool {
    crate::ribbon::color_button(ui, c)
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    watermark_dialog(app, ctx);
    borders_dialog(app, ctx);
}

fn watermark_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.watermark.take() else { return };
    let (mut open, mut ok, cancel) = (true, false, std::cell::Cell::new(false));
    crate::theme::dialog(ctx, "Printed Watermark").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::radio_value(ui, &mut d.kind, 0, crate::i18n::tr("No watermark"));
        crate::theme::radio_value(ui, &mut d.kind, 1, crate::i18n::tr("Picture watermark"));
        ui.add_enabled_ui(d.kind == 1, |ui| {
            crate::theme::hrow(ui, |ui| {
                if ui.button(crate::i18n::tr("Select Picture…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() {
                        if let Some((bytes, img)) = std::fs::read(&p).ok().and_then(|b| image::load_from_memory(&b).ok().map(|i| (b, i))) { d.picture = Some((bytes, img.width() as f32, img.height() as f32)); }
                    }
                }
                ui.label(if d.picture.is_some() { "Picture selected" } else { "No picture" });
                crate::theme::check(ui, &mut d.washout, crate::i18n::tr("Washout"));
            });
        });
        crate::theme::radio_value(ui, &mut d.kind, 2, crate::i18n::tr("Text watermark"));
        ui.add_enabled_ui(d.kind == 2, |ui| {
            crate::theme::grid(ui, egui::Grid::new("wm").num_columns(2), |ui| {
                ui.label(crate::i18n::tr("Text:"));
                crate::theme::Combo::from_id_salt("wm-text").selected_text(crate::i18n::tr(&d.text.clone())).show_ui(ui, |ui| { for t in word_engine::design::WATERMARKS { ui.selectable_value(&mut d.text, t.to_string(), crate::i18n::tr(&(*t))); } });
                ui.end_row();
                ui.label(""); ui.text_edit_singleline(&mut d.text); ui.end_row();
                ui.label(crate::i18n::tr("Font:"));
                crate::theme::Combo::from_id_salt("wm-font").selected_text(crate::i18n::tr(&d.font.clone())).show_ui(ui, |ui| { for n in word_engine::fonts::family_names() { ui.selectable_value(&mut d.font, n.clone(), n.as_str()); } });
                ui.end_row();
                ui.label(crate::i18n::tr("Size:")); ui.add(egui::DragValue::new(&mut d.size).range(0.0..=200.0).custom_formatter(|v, _| if v == 0.0 { "Auto".into() } else { format!("{v:.0}") })); ui.end_row();
                ui.label(crate::i18n::tr("Color:")); crate::theme::hrow(ui, |ui| { color_button(ui, &mut d.color); crate::theme::check(ui, &mut d.semi, crate::i18n::tr("Semitransparent")); }); ui.end_row();
                ui.label(crate::i18n::tr("Layout:")); crate::theme::hrow(ui, |ui| { crate::theme::radio_value(ui, &mut d.diagonal, true, crate::i18n::tr("Diagonal")); crate::theme::radio_value(ui, &mut d.diagonal, false, crate::i18n::tr("Horizontal")); }); ui.end_row();
            });
        });
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel.set(true); } });
    });
    if ok {
        let mark = match d.kind {
            1 => d.picture.as_ref().map(|(bytes, w, h)| {
                let asset = app.doc.add_png_asset(word_engine::picture::to_png(bytes).unwrap_or_else(|| bytes.clone()));
                Watermark::Picture { asset, width: *w, height: *h, washout: d.washout }
            }),
            2 => {
                // Semitransparent text is drawn lighter.
                let c = if d.semi { Color(((d.color.0 as u16 + 255 * 2) / 3) as u8, ((d.color.1 as u16 + 255 * 2) / 3) as u8, ((d.color.2 as u16 + 255 * 2) / 3) as u8) } else { d.color };
                Some(Watermark::Text { text: d.text.clone(), font: d.font.clone(), size: d.size, color: c, diagonal: d.diagonal })
            }
            _ => None,
        };
        if d.kind == 0 || mark.is_some() { app.doc.set_watermark(mark); }
        app.canvas_focus = true;
    } else if open && !cancel.get() { app.extras.watermark = Some(d); } else { app.canvas_focus = true; }
}

fn borders_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.page_borders.take() else { return };
    let (mut open, mut ok, cancel) = (true, false, std::cell::Cell::new(false));
    crate::theme::dialog(ctx, "Page Borders").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hcols(ui, |ui| {
            crate::theme::vcol(ui, |ui| {
                crate::theme::section(ui, "Setting");
                for (k, n) in ["None", "Box", "Custom"].iter().enumerate() { crate::theme::radio_value(ui, &mut d.setting, k as u8, crate::i18n::tr(&*n)); }
            });
            ui.separator();
            crate::theme::vcol(ui, |ui| {
                crate::theme::section(ui, "Style");
                for (s, n) in [(BorderStyle::Single, "Single"), (BorderStyle::Double, "Double"), (BorderStyle::Dotted, "Dotted"), (BorderStyle::Dashed, "Dashed"), (BorderStyle::Thick, "Thick"), (BorderStyle::ThinThick, "Thin–Thick")] {
                    crate::theme::radio_value(ui, &mut d.style, s, crate::i18n::tr(&n));
                }
                crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Color:")); color_button(ui, &mut d.color); });
                crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Width:")); ui.add(egui::DragValue::new(&mut d.width).range(0.25..=6.0).speed(0.05).suffix(" pt")); });
            });
            ui.separator();
            crate::theme::vcol(ui, |ui| {
                crate::theme::section(ui, "Preview");
                // Click the sides to turn them on or off (Custom).
                let (rect, _) = ui.allocate_exact_size(egui::vec2(110.0, 140.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                let inner = rect.shrink(12.0);
                let c = Color32::from_rgb(d.color.0, d.color.1, d.color.2);
                let sides = [[inner.left_top(), inner.right_top()], [inner.left_top(), inner.left_bottom()], [inner.left_bottom(), inner.right_bottom()], [inner.right_top(), inner.right_bottom()]];
                for (k, s) in sides.iter().enumerate() {
                    let hit = egui::Rect::from_two_pos(s[0], s[1]).expand(5.0);
                    let r = ui.interact(hit, egui::Id::unique(("page-border-side", k)), egui::Sense::click());
                    if r.clicked() { d.sides[k] = !d.sides[k]; d.setting = 2; }
                    if d.setting != 0 && d.sides[k] { ui.painter().line_segment(*s, egui::Stroke::new(d.width.max(0.5) * 1.3, c)); }
                    else { ui.painter().line_segment(*s, egui::Stroke::new(0.5, Color32::from_gray(210))); }
                }
                for k in 0..6 { let y = inner.min.y + 10.0 + k as f32 * 16.0; ui.painter().line_segment([egui::pos2(inner.min.x + 8.0, y), egui::pos2(inner.max.x - 8.0, y)], egui::Stroke::new(1.0, Color32::from_gray(200))); }
            });
        });
        ui.separator();
        crate::theme::hrow(ui, |ui| {
            ui.label(crate::i18n::tr("Margin:")); ui.add(egui::DragValue::new(&mut d.space).range(0.0..=31.0).suffix(" pt"));
            ui.label(crate::i18n::tr("Measure from:"));
            crate::theme::radio_value(ui, &mut d.from_text, false, crate::i18n::tr("Edge of page"));
            crate::theme::radio_value(ui, &mut d.from_text, true, crate::i18n::tr("Text"));
        });
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { ok = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel.set(true); } });
    });
    if ok {
        let b = (d.setting != 0 && d.sides.iter().any(|s| *s)).then(|| PageBorders { style: d.style, width: d.width, color: d.color, space: d.space, from_text: d.from_text, sides: if d.setting == 1 { [true; 4] } else { d.sides } });
        app.doc.set_page_borders(b);
        app.canvas_focus = true;
    } else if open && !cancel.get() { app.extras.page_borders = Some(d); } else { app.canvas_focus = true; }
}
