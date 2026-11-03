//! Redact tool set: marking, Find & redact, review, apply and the result report.

use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use pdf_engine::{Pattern, Rect, RedactReport};

use crate::app::{App, Tool};
use crate::i18n::{n_, tr, trf};
use crate::theme::{self, Icon, palette};

/// A pending redaction mark.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub page: usize,
    pub rect: Rect,
    /// What is under it, for the review list.
    pub label: String,
}

#[derive(Default)]
pub struct RedactState {
    pub marks: Vec<Mark>,
    pub words: String,
    pub patterns: [bool; 4],
    pub remove_hidden: bool,
    pub review: bool,
    pub report: Option<RedactReport>,
}

impl RedactState {
    pub fn add(&mut self, page: usize, rect: Rect, label: String) {
        let rect = rect.normalized();
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return;
        }
        if !self.marks.iter().any(|m| {
            m.page == page
                && (m.rect.x0 - rect.x0).abs() < 0.5
                && (m.rect.y0 - rect.y0).abs() < 0.5
                && (m.rect.x1 - rect.x1).abs() < 0.5
        }) {
            self.marks.push(Mark { page, rect, label });
        }
    }
}

fn pattern_label(p: Pattern) -> &'static str {
    match p {
        Pattern::Email => n_("Email addresses"),
        Pattern::Phone => n_("Phone numbers"),
        Pattern::CardNumber => n_("Card numbers"),
        Pattern::Date => n_("Dates"),
    }
}

/// Marks the text glyphs `a..=b` of a page (one mark per line).
pub fn mark_text(app: &mut App, page: usize, a: usize, b: usize) {
    let Some(doc) = &app.doc else { return };
    let text = doc.page_text(page);
    let label = text.slice(a, b).trim().to_string();
    for r in text.line_rects(a, b) {
        app.redact.add(page, r.expand(0.5), label.clone());
    }
}

/// Draws the marks of one page.
pub fn paint_marks(app: &App, painter: &egui::Painter, page: usize, screen: egui::Rect, zoom: f32) {
    for m in app.redact.marks.iter().filter(|m| m.page == page) {
        let r = egui::Rect::from_min_max(
            screen.min + egui::vec2(m.rect.x0, m.rect.y0) * zoom,
            screen.min + egui::vec2(m.rect.x1, m.rect.y1) * zoom,
        );
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(200, 30, 30, 50));
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.5, Color32::from_rgb(200, 30, 30)),
            egui::StrokeKind::Middle,
        );
    }
}

pub fn tools(app: &mut App, ui: &mut egui::Ui) {
    let p = palette(ui.visuals().dark_mode);
    theme::caption(ui, &tr("Redact"));
    for tool in [Tool::RedactText, Tool::RedactArea] {
        if theme::tool_row(ui, tool.icon(), &tr(tool.label()), app.tool == tool).clicked() {
            app.tool = tool;
        }
    }
    if theme::tool_row(ui, Icon::BlankPage, &tr("Mark current page"), false).clicked()
        && let Some(doc) = &app.doc
    {
        let page = app.current_page;
        let (w, h) = doc.page_size(page);
        app.redact.add(
            page,
            Rect::new(0.0, 0.0, w, h),
            trf("Page {}", &[&(page + 1)]),
        );
    }
    ui.add_space(8.0);
    theme::caption(ui, &tr("Find & redact"));
    let r = ui.add(
        egui::TextEdit::singleline(&mut app.redact.words)
            .hint_text(tr("Words, separated by commas"))
            .desired_width(f32::INFINITY),
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("Words to redact")));
    for (i, pat) in Pattern::ALL.iter().enumerate() {
        theme::check(ui, &mut app.redact.patterns[i], &tr(pattern_label(*pat)));
    }
    if ui.button(tr("Find and mark")).clicked() {
        let words: Vec<String> = app
            .redact
            .words
            .split(',')
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
            .collect();
        let patterns: Vec<Pattern> = Pattern::ALL
            .iter()
            .zip(app.redact.patterns)
            .filter(|(_, on)| *on)
            .map(|(p, _)| *p)
            .collect();
        if let Some(doc) = &app.doc {
            let hits = doc.find_redactions(&words, &patterns);
            let n = hits.len();
            for h in hits {
                for r in h.rects {
                    app.redact.add(h.page, r.expand(0.5), h.text.clone());
                }
            }
            app.notify(trf("{} matches marked", &[&n]));
        }
    }
    ui.add_space(8.0);
    theme::caption(ui, &trf("Marks ({})", &[&app.redact.marks.len()]));
    let mut remove = None;
    let mut go = None;
    egui::ScrollArea::vertical()
        .id_salt("marks")
        .max_height(220.0)
        .show(ui, |ui| {
            for (i, m) in app.redact.marks.iter().enumerate() {
                theme::row(ui, |ui| {
                    if theme::icon_button_sized(ui, Icon::Trash, &tr("Remove mark"), false, 22.0)
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    let label = if m.label.is_empty() {
                        tr("Area")
                    } else {
                        crate::i18n::shape(&m.label)
                    };
                    let text = format!("{} · {}", trf("Page {}", &[&(m.page + 1)]), label);
                    if ui
                        .add(
                            egui::Label::new(RichText::new(text).size(12.0))
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        go = Some((m.page, m.rect));
                    }
                });
            }
        });
    if let Some(i) = remove {
        app.redact.marks.remove(i);
    }
    if let Some((page, rect)) = go {
        app.canvas.reveal = Some((page, rect));
        app.current_page = page;
    }
    ui.add_space(6.0);
    theme::check(
        ui,
        &mut app.redact.remove_hidden,
        &tr("Also remove hidden information"),
    )
    .on_hover_text(tr(
        "Metadata, document information, JavaScript, attached files and page actions.",
    ));
    ui.add_space(6.0);
    let ready = !app.redact.marks.is_empty() || app.redact.remove_hidden;
    if ui
        .add_enabled_ui(ready, |ui| {
            theme::primary_button(ui, &tr("Apply redactions…"))
        })
        .inner
        .clicked()
    {
        app.redact.review = true;
    }
    if !app.redact.marks.is_empty() && ui.button(tr("Clear marks")).clicked() {
        app.redact.marks.clear();
    }
    ui.add_space(8.0);
    theme::para(
        ui,
        n_(
            "Redaction removes the text, image pixels, drawings, links and comments under each mark from the file, then covers the marks with black boxes.",
        ),
        11.5,
        Some(p.muted),
    );
}

pub fn dialogs(app: &mut App, ctx: &egui::Context) {
    let p = palette(ctx.global_style().visuals.dark_mode);
    if app.redact.review {
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("redact_review")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.with_layout(theme::column_layout(), |ui| {
                ui.label(RichText::new(tr("Apply redactions?")).font(theme::bold(16.0)));
                let pages: std::collections::BTreeSet<usize> =
                    app.redact.marks.iter().map(|m| m.page).collect();
                ui.label(trf(
                    "{} marks on {} pages will be removed permanently once you save.",
                    &[&app.redact.marks.len(), &pages.len()],
                ));
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for m in &app.redact.marks {
                            let label = if m.label.is_empty() {
                                tr("Area")
                            } else {
                                crate::i18n::shape(&m.label)
                            };
                            ui.label(
                                RichText::new(format!(
                                    "• {} · {}",
                                    trf("Page {}", &[&(m.page + 1)]),
                                    label
                                ))
                                .size(12.0),
                            );
                        }
                    });
                if app.redact.remove_hidden {
                    ui.label(
                        RichText::new(tr("Hidden information will also be removed."))
                            .color(p.muted),
                    );
                }
            });
            ui.add_space(8.0);
            theme::footer(ui, |ui| {
                if theme::primary_button(ui, &tr("Apply")).clicked() {
                    choice = Some(true);
                }
                if ui.button(tr("Cancel")).clicked() {
                    choice = Some(false);
                }
            });
        });
        match choice {
            Some(true) => {
                app.redact.review = false;
                let marks: Vec<(usize, Rect)> =
                    app.redact.marks.iter().map(|m| (m.page, m.rect)).collect();
                let hidden = app.redact.remove_hidden;
                let mut result = None;
                app.apply(false, |d| {
                    result = Some(d.redact(&marks, hidden)?);
                    Ok(())
                });
                if let Some(r) = result {
                    app.redact.marks.clear();
                    app.redact.report = Some(r);
                }
            }
            Some(false) => app.redact.review = false,
            None => {}
        }
    }
    if let Some(report) = app.redact.report.clone() {
        let mut close = false;
        egui::Modal::new(egui::Id::unique("redact_report")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.with_layout(theme::column_layout(), |ui| {
                ui.label(RichText::new(tr("Redaction applied")).font(theme::bold(16.0)));
                let line = |ui: &mut egui::Ui, label: &str, n: usize| {
                    if n > 0 {
                        ui.label(trf(label, &[&n]));
                    }
                };
                line(ui, n_("Characters removed: {}"), report.glyphs_removed);
                line(ui, n_("Images edited: {}"), report.images_edited);
                line(ui, n_("Images removed: {}"), report.images_removed);
                line(ui, n_("Drawings removed: {}"), report.paths_removed);
                line(
                    ui,
                    n_("Comments, links and fields removed: {}"),
                    report.annotations_removed,
                );
                line(ui, n_("Added objects removed: {}"), report.objects_removed);
                if !report.hidden_removed.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(tr("Hidden information removed")).strong());
                    for h in &report.hidden_removed {
                        ui.label(format!("• {}", tr(h)));
                    }
                }
                ui.add_space(4.0);
                if report.unhandled.is_empty() {
                    ui.label(
                        RichText::new(tr("Everything under the marks was handled.")).color(p.muted),
                    );
                } else {
                    ui.label(
                        RichText::new(tr("Please check"))
                            .strong()
                            .color(Color32::from_rgb(0xC0, 0x50, 0x00)),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for u in &report.unhandled {
                                ui.label(RichText::new(format!("• {u}")).size(12.0));
                            }
                        });
                }
                ui.label(
                    RichText::new(tr("Save the document to make the redaction permanent."))
                        .color(p.muted),
                );
            });
            ui.add_space(8.0);
            theme::footer(ui, |ui| {
                if theme::primary_button(ui, &tr("OK")).clicked() {
                    close = true;
                }
            });
        });
        if close {
            app.redact.report = None;
        }
    }
    let _ = Vec2::ZERO;
}
