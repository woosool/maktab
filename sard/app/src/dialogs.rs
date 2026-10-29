//! Dialogs of the Home and Insert tabs: Font, Paragraph, Bullets and Numbering, Columns,
//! Line Spacing, Insert Table, Header & Footer, Date & Time, Symbol, Equation, Chart Data,
//! SmartArt text, Link, Action, Photo Album, Replace Fonts, Find and Replace.

use crate::app::SlidesApp;
use crate::i18n::tr;
use egui::{vec2, Color32, RichText, Sense, Stroke};
use qalam_kit::colors::*;
use slides_engine::edit::RunFormat;
use slides_engine::scene::Loc;
use word_engine::objects::{Chart, DiagramKind};

#[derive(Clone, Debug)]
pub struct FontDialog { pub font: String, pub size: f64, pub bold: bool, pub italic: bool, pub color: word_engine::Color, pub underline: String, pub strike: u8, pub offset: i32, pub small_caps: bool, pub all_caps: bool, pub spacing: f64 }

#[derive(Clone, Debug)]
pub struct ParaDialog { pub align: String, pub before_text: f64, pub hanging: f64, pub before: f64, pub after: f64, pub line: f64, pub rtl: bool }

#[derive(Clone, Debug)]
pub enum Dialog {
    Font(FontDialog),
    Paragraph(ParaDialog),
    Bullets { numbered: bool, kind: String, size: f64, color: word_engine::Color, start: u32 },
    Columns { count: u32, gap: f64 },
    InsertTable { rows: usize, cols: usize },
    HeaderFooter { date: bool, auto: bool, fixed: String, format: usize, number: bool, footer: bool, footer_text: String, skip_title: bool },
    DateTime { format: usize, auto: bool },
    Symbol { subset: usize, selected: Option<char> },
    Equation { loc: Option<Loc>, linear: String },
    ChartData { loc: Loc, chart: Chart },
    SmartArt { loc: Option<Loc>, kind: DiagramKind, text: String },
    Link { target: u8, address: String, slide: usize, text_only: bool },
    Action { kind: u8, slide: usize, address: String },
    PhotoAlbum { files: Vec<std::path::PathBuf>, per: usize, captions: bool },
    ReplaceFonts { from: String, to: String },
    Find { query: String, with: String, match_case: bool, whole: bool, replace: bool, last: Option<(usize, usize)> },
    /// Slide size in inches; `fit`: Some(true) Ensure Fit, Some(false) Maximize, None don't scale.
    SlideSize { w: f64, h: f64, fit: Option<bool> },
}

fn grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) { crate::theme::grid(ui, egui::Grid::new(id).num_columns(2).spacing(vec2(12.0, 8.0)), add); }

pub fn show(app: &mut SlidesApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let mut open = true;
    let mut done = false;
    let title = match &d {
        Dialog::Font(_) => "Font", Dialog::Paragraph(_) => "Paragraph", Dialog::Bullets { .. } => "Bullets and Numbering", Dialog::Columns { .. } => "Columns",
        Dialog::InsertTable { .. } => "Insert Table", Dialog::HeaderFooter { .. } => "Header and Footer", Dialog::DateTime { .. } => "Date and Time", Dialog::Symbol { .. } => "Symbol",
        Dialog::Equation { .. } => "Equation", Dialog::ChartData { .. } => "Edit Data", Dialog::SmartArt { .. } => "SmartArt", Dialog::Link { .. } => "Insert Hyperlink",
        Dialog::Action { .. } => "Action Settings", Dialog::PhotoAlbum { .. } => "Photo Album", Dialog::ReplaceFonts { .. } => "Replace Font", Dialog::Find { replace: true, .. } => "Replace", Dialog::Find { .. } => "Find", Dialog::SlideSize { .. } => "Slide Size",
    };
    crate::theme::dialog(ctx, title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_min_width(380.0);
        match &mut d {
            Dialog::Font(f) => {
                grid(ui, "font-d", |ui| {
                    ui.label(tr("Latin text font"));
                    crate::theme::Combo::from_id_salt("fd-font").width(200.0).selected_text(f.font.clone()).show_ui(ui, |ui| for n in word_engine::fonts::family_names() { if ui.selectable_label(f.font == n, &n).clicked() { f.font = n.clone(); } });
                    ui.end_row();
                    ui.label(tr("Font style"));
                    crate::theme::hrow(ui, |ui| { crate::theme::check(ui, &mut f.bold, tr("Bold")); crate::theme::check(ui, &mut f.italic, tr("Italic")); });
                    ui.end_row();
                    ui.label(tr("Size"));
                    ui.add(egui::DragValue::new(&mut f.size).range(1.0..=4000.0).speed(0.5).suffix(" pt"));
                    ui.end_row();
                    ui.label(tr("Font color"));
                    qalam_kit::widgets::color_button(ui, &mut f.color);
                    ui.end_row();
                    ui.label(tr("Underline style"));
                    crate::theme::Combo::from_id_salt("fd-u").selected_text(tr(UNDERLINES.iter().find(|u| u.0 == f.underline).map(|u| u.1).unwrap_or("(none)"))).show_ui(ui, |ui| for (k, n) in UNDERLINES { if ui.selectable_label(f.underline == *k, tr(n)).clicked() { f.underline = k.to_string(); } });
                    ui.end_row();
                });
                crate::theme::section(ui, "Effects");
                grid(ui, "font-fx", |ui| {
                    let mut s = f.strike == 1; if crate::theme::check(ui, &mut s, tr("Strikethrough")).changed() { f.strike = if s { 1 } else { 0 }; }
                    crate::theme::check(ui, &mut f.small_caps, tr("Small Caps")); ui.end_row();
                    let mut ds = f.strike == 2; if crate::theme::check(ui, &mut ds, tr("Double Strikethrough")).changed() { f.strike = if ds { 2 } else { 0 }; }
                    crate::theme::check(ui, &mut f.all_caps, tr("All Caps")); ui.end_row();
                    let mut sup = f.offset > 0; if crate::theme::check(ui, &mut sup, tr("Superscript")).changed() { f.offset = if sup { 30 } else { 0 }; }
                    let mut sub = f.offset < 0; if crate::theme::check(ui, &mut sub, tr("Subscript")).changed() { f.offset = if sub { -25 } else { 0 }; }
                    ui.end_row();
                    ui.label(tr("Offset"));
                    ui.add(egui::DragValue::new(&mut f.offset).range(-100..=100).suffix("%"));
                    ui.end_row();
                });
                crate::theme::section(ui, "Character Spacing");
                ui.add(egui::DragValue::new(&mut f.spacing).range(-20.0..=200.0).speed(0.1).suffix(" pt"));
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done {
                    let f = f.clone();
                    app.format(RunFormat { font: Some(f.font.clone()), size: Some(f.size), bold: Some(f.bold), italic: Some(f.italic), underline: Some(f.underline != "none"), color: Some(format!("{:02X}{:02X}{:02X}", f.color.0, f.color.1, f.color.2)), baseline: Some(f.offset), strike: Some(f.strike > 0), ..Default::default() });
                    let u = f.underline.clone();
                    app.text_cmd(move |p, l, c, r| {
                        p.set_underline_style(l, c, r, &u);
                        p.set_strike_kind(l, c, r, match f.strike { 1 => "sngStrike", 2 => "dblStrike", _ => "noStrike" });
                        p.set_caps(l, c, r, if f.all_caps { "all" } else if f.small_caps { "small" } else { "none" });
                        p.set_char_spacing(l, c, r, f.spacing);
                    });
                }
            }
            Dialog::Paragraph(p) => {
                crate::theme::section(ui, "General");
                grid(ui, "para-g", |ui| {
                    ui.label(tr("Alignment"));
                    crate::theme::Combo::from_id_salt("pd-al").selected_text(tr(ALIGNS.iter().find(|a| a.0 == p.align).map(|a| a.1).unwrap_or("Left"))).show_ui(ui, |ui| for (k, n) in ALIGNS { if ui.selectable_label(p.align == *k, tr(n)).clicked() { p.align = k.to_string(); } });
                    ui.end_row();
                    ui.label(tr("Direction"));
                    crate::theme::hrow(ui, |ui| { crate::theme::radio_value(ui, &mut p.rtl, false, tr("Left-to-right")); crate::theme::radio_value(ui, &mut p.rtl, true, tr("Right-to-left")); });
                    ui.end_row();
                });
                crate::theme::section(ui, "Indentation");
                grid(ui, "para-i", |ui| {
                    ui.label(tr("Before text")); ui.add(egui::DragValue::new(&mut p.before_text).range(0.0..=400.0).speed(0.5).suffix(" pt")); ui.end_row();
                    ui.label(tr("Hanging")); ui.add(egui::DragValue::new(&mut p.hanging).range(-400.0..=400.0).speed(0.5).suffix(" pt")); ui.end_row();
                });
                crate::theme::section(ui, "Spacing");
                grid(ui, "para-s", |ui| {
                    ui.label(tr("Before")); ui.add(egui::DragValue::new(&mut p.before).range(0.0..=400.0).speed(0.5).suffix(" pt")); ui.end_row();
                    ui.label(tr("After")); ui.add(egui::DragValue::new(&mut p.after).range(0.0..=400.0).speed(0.5).suffix(" pt")); ui.end_row();
                    ui.label(tr("Line Spacing")); ui.add(egui::DragValue::new(&mut p.line).range(0.5..=10.0).speed(0.05).suffix("×")); ui.end_row();
                });
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done {
                    let p = p.clone();
                    app.paragraphs(move |pr, l, c, r| {
                        pr.set_align(l, c, r, &p.align);
                        pr.set_paragraph(l, c, r, Some(p.before), Some(p.after), Some(p.before_text + p.hanging.max(0.0)), Some(-p.hanging), Some(p.rtl));
                        pr.set_line_spacing(l, c, r, p.line);
                    });
                }
            }
            Dialog::Bullets { numbered, kind, size, color, start } => {
                crate::theme::hrow(ui, |ui| { crate::theme::radio_value(ui, numbered, false, tr("Bulleted")); crate::theme::radio_value(ui, numbered, true, tr("Numbered")); });
                ui.horizontal_wrapped(|ui| {
                    ui.set_max_width(380.0);
                    let choices: Vec<(String, String)> = if *numbered { NUMBERS.iter().map(|(k, n)| (k.to_string(), n.to_string())).collect() } else { BULLET_CHARS.chars().map(|c| (format!("char:{c}"), c.to_string())).collect() };
                    for (k, n) in choices {
                        let (r, resp) = ui.allocate_exact_size(vec2(70.0, 54.0), Sense::click());
                        ui.painter().rect(r, 4.0, if *kind == k { ACCENT.gamma_multiply(0.15) } else { c_card() }, Stroke::new(1.0, if *kind == k { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, n, egui::FontId::proportional(16.0), c_ink());
                        if resp.clicked() { *kind = k; }
                    }
                });
                grid(ui, "bul", |ui| {
                    ui.label(tr("Size")); ui.add(egui::DragValue::new(size).range(25.0..=400.0).suffix("% of text")); ui.end_row();
                    ui.label(tr("Color")); qalam_kit::widgets::color_button(ui, color); ui.end_row();
                    if *numbered { ui.label(tr("Start at")); ui.add(egui::DragValue::new(start).range(1..=9999)); ui.end_row(); }
                });
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done {
                    let (k, sz, col) = (kind.clone(), *size, format!("{:02X}{:02X}{:02X}", color.0, color.1, color.2));
                    app.paragraphs(move |p, l, c, r| { p.set_bullets(l, c, r, &k); p.set_bullet_look(l, c, r, Some(sz), Some(&col)); });
                }
            }
            Dialog::Columns { count, gap } => {
                grid(ui, "cols", |ui| {
                    ui.label(tr("Number")); ui.add(egui::DragValue::new(count).range(1..=16)); ui.end_row();
                    ui.label(tr("Spacing")); ui.add(egui::DragValue::new(gap).range(0.0..=200.0).speed(0.5).suffix(" pt")); ui.end_row();
                });
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done { let (n, g) = (*count, *gap); app.shapes_cmd(move |p, l| p.set_columns(l, n, g)); }
            }
            Dialog::InsertTable { rows, cols } => {
                grid(ui, "it", |ui| {
                    ui.label(tr("Number of columns")); ui.add(egui::DragValue::new(cols).range(1..=75)); ui.end_row();
                    ui.label(tr("Number of rows")); ui.add(egui::DragValue::new(rows).range(1..=75)); ui.end_row();
                });
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done { app.insert_table(*rows, *cols); }
            }
            Dialog::HeaderFooter { date, auto, fixed, format, number, footer, footer_text, skip_title } => {
                crate::theme::check(ui, date, tr("Date and time"));
                ui.add_enabled_ui(*date, |ui| {
                    ui.indent("dt", |ui| {
                        crate::theme::radio_value(ui, auto, true, tr("Update automatically"));
                        let fmts = slides_engine::format::date_formats();
                        crate::theme::Combo::from_id_salt("hf-fmt").width(240.0).selected_text(fmts.get(*format).map(|f| f.1.clone()).unwrap_or_default()).show_ui(ui, |ui| for (k, (_, ex)) in fmts.iter().enumerate() { if ui.selectable_label(*format == k, ex).clicked() { *format = k; } });
                        crate::theme::radio_value(ui, auto, false, tr("Fixed"));
                        ui.add_enabled(!*auto, egui::TextEdit::singleline(fixed).desired_width(240.0));
                    });
                });
                crate::theme::check(ui, number, tr("Slide number"));
                crate::theme::check(ui, footer, tr("Footer"));
                ui.add_enabled(*footer, egui::TextEdit::singleline(footer_text).desired_width(300.0));
                crate::theme::check(ui, skip_title, tr("Don't show on title slide"));
                let (apply, all) = crate::theme::footer(ui, |ui| (crate::theme::secondary(ui, "Apply").clicked(), crate::theme::primary(ui, "Apply to All").clicked()));
                if apply || all {
                    done = true;
                    let slides = if all { app.slides() } else { app.slide().into_iter().collect() };
                    let d = if *date { Some(if *auto { String::new() } else { fixed.clone() }) } else { None };
                    app.checkpoint("header-footer");
                    app.pres.set_header_footer(&slides, d.as_deref(), *number, if *footer { Some(footer_text.as_str()) } else { None }, *skip_title);
                }
            }
            Dialog::DateTime { format, auto } => {
                let fmts = slides_engine::format::date_formats();
                for (k, (_, ex)) in fmts.iter().enumerate() { if ui.selectable_label(*format == k, ex).clicked() { *format = k; } }
                crate::theme::check(ui, auto, tr("Update automatically"));
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done {
                    let (kind, text) = fmts[*format].clone();
                    if *auto { app.insert_field(kind); } else { app.insert_text_here(&text); }
                }
            }
            Dialog::Symbol { subset, selected } => {
                use qalam_kit::symbols::{SPECIAL, SUBSETS};
                crate::theme::Combo::from_id_salt("sym-subset").width(260.0).selected_text(tr(SUBSETS[*subset].0)).show_ui(ui, |ui| for (k, s) in SUBSETS.iter().enumerate() { if ui.selectable_label(*subset == k, tr(s.0)).clicked() { *subset = k; } });
                let (_, a, b) = SUBSETS[*subset];
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(420.0);
                        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                        for cp in a..=b {
                            let Some(c) = char::from_u32(cp) else { continue };
                            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            let on = *selected == Some(c);
                            ui.painter().rect(r, 2.0, if on { ACCENT.gamma_multiply(0.2) } else if resp.hovered() { c_hover() } else { c_card() }, Stroke::new(1.0, if on { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
                            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, c.to_string(), qalam_kit::symbols::glyph_font(c, 18.0), c_ink());
                            if resp.clicked() { *selected = Some(c); }
                            if resp.double_clicked() { app.insert_text_here(&c.to_string()); }
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| { ui.set_max_width(420.0); for (c, n) in SPECIAL.iter().take(14) { if ui.small_button(c.to_string()).on_hover_text(tr(n)).clicked() { *selected = Some(*c); } } });
                if let Some(c) = selected { ui.label(format!("U+{:04X}", *c as u32)); }
                done = crate::theme::footer(ui, |ui| crate::theme::primary_enabled(ui, selected.is_some(), "Insert").clicked());
                if done { if let Some(c) = selected { app.insert_text_here(&c.to_string()); } }
            }
            Dialog::Equation { loc, linear } => {
                use qalam_kit::symbols::{MATH_SYMBOLS, STRUCTURES};
                egui::ScrollArea::horizontal().id_salt("eq-structs").show(ui, |ui| ui.horizontal(|ui| for (group, _, items) in STRUCTURES {
                    crate::theme::submenu(ui, tr(group), |ui| for (n, src) in *items { if ui.button(tr(n)).clicked() { linear.push_str(src); ui.close(); } });
                }));
                ui.horizontal_wrapped(|ui| { ui.set_max_width(440.0); for c in MATH_SYMBOLS.chars() { if ui.small_button(RichText::new(c.to_string()).font(qalam_kit::symbols::glyph_font(c, 14.0))).clicked() { linear.push(c); } } });
                ui.add(egui::TextEdit::multiline(linear).desired_width(440.0).desired_rows(3).font(egui::FontId::monospace(13.0)).hint_text(tr("Type an equation, e.g. x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}")));
                // A live preview.
                let eq = word_engine::equation::Equation { linear: linear.clone(), size: 20.0, color: word_engine::Color::BLACK, display: true };
                if let Some(png) = word_engine::objects::object_png(&word_engine::objects::Object::Equation(eq)) {
                    if let Ok(img) = image::load_from_memory(&png) {
                        let img = img.to_rgba8();
                        let tex = ui.ctx().load_texture("eq-preview", egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR);
                        let s = vec2(img.width() as f32 / 2.0, img.height() as f32 / 2.0);
                        egui::Frame::NONE.fill(Color32::WHITE).inner_margin(8).show(ui, |ui| { ui.set_min_width(440.0); ui.image((tex.id(), s)); });
                    }
                }
                crate::theme::hrow(ui, |ui| for (n, src) in word_engine::equation::GALLERY.iter().take(6) { if ui.small_button(tr(n)).clicked() { *linear = src.to_string(); } });
                done = crate::theme::footer(ui, |ui| crate::theme::primary_enabled(ui, !linear.trim().is_empty(), "Insert").clicked());
                if done { app.insert_equation(loc.clone(), linear.clone()); }
            }
            Dialog::ChartData { loc, chart } => {
                grid(ui, "ch-top", |ui| {
                    ui.label(tr("Chart type"));
                    crate::theme::Combo::from_id_salt("ch-kind").selected_text(tr(word_engine::objects::CHARTS.iter().find(|c| c.0 == chart.kind).map(|c| c.1).unwrap_or("Column"))).show_ui(ui, |ui| for (k, n) in word_engine::objects::CHARTS { if ui.selectable_label(chart.kind == *k, tr(n)).clicked() { chart.kind = *k; } });
                    ui.end_row();
                    ui.label(tr("Chart title")); ui.text_edit_singleline(&mut chart.title); ui.end_row();
                    ui.label(tr("Legend")); crate::theme::check(ui, &mut chart.legend, ""); ui.end_row();
                });
                egui::ScrollArea::both().max_height(260.0).show(ui, |ui| {
                    egui::Grid::new("ch-data").striped(true).show(ui, |ui| {
                        ui.label("");
                        for s in chart.series.iter_mut() { ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(80.0)); }
                        ui.end_row();
                        for (ci, cat) in chart.categories.iter_mut().enumerate() {
                            ui.add(egui::TextEdit::singleline(cat).desired_width(90.0));
                            for s in chart.series.iter_mut() { if let Some(v) = s.values.get_mut(ci) { ui.add(egui::DragValue::new(v).speed(0.1)); } }
                            ui.end_row();
                        }
                    });
                });
                crate::theme::hrow(ui, |ui| {
                    if ui.button(tr("Add Category")).clicked() { chart.categories.push(format!("Category {}", chart.categories.len() + 1)); for s in chart.series.iter_mut() { s.values.push(0.0); } }
                    if ui.button(tr("Add Series")).clicked() { let n = chart.categories.len(); chart.series.push(word_engine::objects::Series { name: format!("Series {}", chart.series.len() + 1), values: vec![0.0; n], color: None }); }
                    if chart.categories.len() > 1 && ui.button(tr("Remove Category")).clicked() { chart.categories.pop(); for s in chart.series.iter_mut() { s.values.pop(); } }
                    if chart.series.len() > 1 && ui.button(tr("Remove Series")).clicked() { chart.series.pop(); }
                });
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done { app.checkpoint("chart"); app.pres.set_chart(loc, chart); }
            }
            Dialog::SmartArt { loc, kind, text } => {
                ui.horizontal_wrapped(|ui| for (k, cat, name) in slides_engine::objects::SMARTART {
                    let (r, resp) = ui.allocate_exact_size(vec2(92.0, 70.0), Sense::click());
                    ui.painter().rect(r, 4.0, if *kind == *k { ACCENT.gamma_multiply(0.15) } else { c_card() }, Stroke::new(1.0, if *kind == *k { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
                    smartart_icon(ui, *k, r.shrink2(vec2(14.0, 14.0)).translate(vec2(0.0, -6.0)));
                    ui.painter().text(egui::pos2(r.center().x, r.max.y - 9.0), egui::Align2::CENTER_CENTER, tr(cat), egui::FontId::proportional(10.5), c_ink2());
                    if resp.on_hover_text(tr(name)).clicked() { *kind = *k; }
                });
                ui.label(RichText::new(tr("Type your text here (Tab for a lower level):")).size(12.0).color(c_ink3()));
                ui.add(egui::TextEdit::multiline(text).desired_width(400.0).desired_rows(7));
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done { app.insert_smartart(loc.clone(), *kind, text); }
            }
            Dialog::Link { target, address, slide, text_only } => {
                crate::theme::hrow(ui, |ui| { crate::theme::radio_value(ui, target, 0, tr("Existing File or Web Page")); crate::theme::radio_value(ui, target, 1, tr("Place in This Document")); crate::theme::radio_value(ui, target, 2, tr("E-mail Address")); });
                match *target {
                    1 => { let n = app.slides().len(); egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| for i in 0..n { let t = app.pres.slide_title(&app.slides()[i]); if ui.selectable_label(*slide == i, format!("{}. {}", i + 1, if t.is_empty() { tr("Slide") } else { t })).clicked() { *slide = i; } }); }
                    _ => { ui.add(egui::TextEdit::singleline(address).desired_width(380.0).hint_text(if *target == 2 { "name@example.com" } else { "https://" })); }
                }
                let _ = text_only;
                let (remove, ok) = crate::theme::footer(ui, |ui| (crate::theme::secondary(ui, "Remove Link").clicked(), crate::theme::primary(ui, "OK").clicked()));
                if ok || remove {
                    done = true;
                    let t = if remove { None } else { Some(match *target { 1 => format!("#{}", *slide + 1), 2 => format!("mailto:{}", address.trim_start_matches("mailto:")), _ => address.clone() }) };
                    app.set_link(t.as_deref());
                }
            }
            Dialog::Action { kind, slide, address } => {
                for (k, n) in [(0u8, "None"), (1, "Next Slide"), (2, "Previous Slide"), (3, "First Slide"), (4, "Last Slide"), (5, "End Show"), (6, "Slide…"), (7, "URL…")] { crate::theme::radio_value(ui, kind, k, tr(n)); }
                if *kind == 6 { let n = app.slides().len(); crate::theme::Combo::from_id_salt("act-sl").selected_text(format!("{}", *slide + 1)).show_ui(ui, |ui| for i in 0..n { if ui.selectable_label(*slide == i, format!("{}", i + 1)).clicked() { *slide = i; } }); }
                if *kind == 7 { ui.text_edit_singleline(address); }
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done {
                    let t = match *kind { 1 => Some("next".to_string()), 2 => Some("previous".into()), 3 => Some("first".into()), 4 => Some("last".into()), 5 => Some("end".into()), 6 => Some(format!("#{}", *slide + 1)), 7 => Some(address.clone()), _ => None };
                    app.set_link(t.as_deref());
                }
            }
            Dialog::PhotoAlbum { files, per, captions } => {
                if ui.button(tr("Insert picture from File/Disk…")).clicked() { if let Some(f) = rfd::FileDialog::new().add_filter(&tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_files() { files.extend(f); } }
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| { let mut remove = None; for (i, f) in files.iter().enumerate() { crate::theme::hrow(ui, |ui| { ui.label(f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()); if ui.small_button("✕").clicked() { remove = Some(i); } }); } if let Some(i) = remove { files.remove(i); } });
                grid(ui, "pa", |ui| {
                    ui.label(tr("Picture layout"));
                    crate::theme::Combo::from_id_salt("pa-per").selected_text(tr(match *per { 1 => "1 picture", 2 => "2 pictures", _ => "4 pictures" })).show_ui(ui, |ui| for (k, n) in [(1, "1 picture"), (2, "2 pictures"), (4, "4 pictures")] { if ui.selectable_label(*per == k, tr(n)).clicked() { *per = k; } });
                    ui.end_row();
                    ui.label(tr("Captions below all pictures")); crate::theme::check(ui, captions, ""); ui.end_row();
                });
                done = crate::theme::footer(ui, |ui| crate::theme::primary_enabled(ui, !files.is_empty(), "Create").clicked());
                if done { app.photo_album(files, *per, *captions); }
            }
            Dialog::ReplaceFonts { from, to } => {
                let used = app.pres.fonts_used();
                if from.is_empty() { if let Some(f) = used.first() { *from = f.clone(); } }
                grid(ui, "rf", |ui| {
                    ui.label(tr("Replace"));
                    crate::theme::Combo::from_id_salt("rf-from").width(200.0).selected_text(from.clone()).show_ui(ui, |ui| for f in &used { if ui.selectable_label(from == f, f).clicked() { *from = f.clone(); } });
                    ui.end_row();
                    ui.label(tr("With"));
                    crate::theme::Combo::from_id_salt("rf-to").width(200.0).selected_text(to.clone()).show_ui(ui, |ui| for f in word_engine::fonts::family_names() { if ui.selectable_label(*to == f, &f).clicked() { *to = f.clone(); } });
                    ui.end_row();
                });
                done = crate::theme::footer(ui, |ui| crate::theme::primary_enabled(ui, !from.is_empty() && !to.is_empty(), "Replace").clicked());
                if done { app.checkpoint("replace-fonts"); let n = app.pres.replace_font(from, to); app.notify(&crate::i18n::trf("Replaced {} occurrences", &[&n])); }
            }
            Dialog::SlideSize { w, h, fit } => {
                grid(ui, "ss", |ui| {
                    ui.label(tr("Slides sized for"));
                    crate::theme::Combo::from_id_salt("ss-preset").width(220.0).selected_text(tr("Custom")).show_ui(ui, |ui| for (n, pw, ph) in [("Widescreen", 13.333, 7.5), ("Standard (4:3)", 10.0, 7.5), ("On-screen Show (16:10)", 10.0, 6.25), ("Letter Paper", 10.0, 7.5), ("A4 Paper", 10.83, 7.5), ("35mm Slides", 11.25, 7.5), ("Banner", 8.0, 1.0)] { if ui.selectable_label(false, tr(n)).clicked() { *w = pw; *h = ph; } });
                    ui.end_row();
                    ui.label(tr("Width")); ui.add(egui::DragValue::new(w).range(1.0..=56.0).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
                    ui.label(tr("Height")); ui.add(egui::DragValue::new(h).range(1.0..=56.0).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
                    ui.label(tr("Content"));
                    crate::theme::Combo::from_id_salt("ss-fit").selected_text(tr(match fit { Some(true) => "Ensure Fit", Some(false) => "Maximize", None => "Don't Scale" })).show_ui(ui, |ui| for (v, n) in [(Some(true), "Ensure Fit"), (Some(false), "Maximize"), (None, "Don't Scale")] { if ui.selectable_label(*fit == v, tr(n)).clicked() { *fit = v; } });
                    ui.end_row();
                });
                done = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
                if done { app.checkpoint("size"); app.pres.resize_slides((*w * 914_400.0).round() as i64, (*h * 914_400.0).round() as i64, *fit); app.thumbs.clear(); }
            }
            Dialog::Find { query, with, match_case, whole, replace, last } => {
                grid(ui, "find", |ui| {
                    ui.label(tr("Find what")); ui.add(egui::TextEdit::singleline(query).desired_width(260.0)); ui.end_row();
                    if *replace { ui.label(tr("Replace with")); ui.add(egui::TextEdit::singleline(with).desired_width(260.0)); ui.end_row(); }
                });
                crate::theme::hrow(ui, |ui| { crate::theme::check(ui, match_case, tr("Match case")); crate::theme::check(ui, whole, tr("Find whole words only")); });
                let (next, rep, all, toggle) = crate::theme::footer(ui, |ui| (
                    crate::theme::primary(ui, "Find Next").clicked(),
                    *replace && crate::theme::secondary(ui, "Replace").clicked(),
                    *replace && crate::theme::secondary(ui, "Replace All").clicked(),
                    !*replace && crate::theme::secondary(ui, "Replace…").clicked(),
                ));
                if toggle { *replace = true; }
                if next { *last = app.find_next(query, *match_case, *whole, *last); }
                if rep { app.replace_current(query, with, *match_case); *last = app.find_next(query, *match_case, *whole, *last); }
                if all { app.checkpoint("replace"); let n = app.pres.replace_all(query, with, *match_case); app.notify(&crate::i18n::trf("Replaced {} occurrences", &[&n])); }
            }
        }
    });
    if open && !done { app.dialog = Some(d); }
}

pub const UNDERLINES: &[(&str, &str)] = &[("none", "(none)"), ("sng", "Single"), ("dbl", "Double"), ("heavy", "Heavy"), ("dotted", "Dotted"), ("dash", "Dashed"), ("dotDash", "Dot-Dash"), ("wavy", "Wavy")];
pub const ALIGNS: &[(&str, &str)] = &[("l", "Left"), ("ctr", "Centered"), ("r", "Right"), ("just", "Justified"), ("dist", "Distributed")];
pub const BULLET_CHARS: &str = "•○■□◆❖➢➤✓✔★–";
pub const NUMBERS: &[(&str, &str)] = &[("arabicPeriod", "1. 2. 3."), ("arabicParenR", "1) 2) 3)"), ("romanUcPeriod", "I. II. III."), ("alphaUcPeriod", "A. B. C."), ("alphaLcParenR", "a) b) c)"), ("alphaLcPeriod", "a. b. c."), ("romanLcPeriod", "i. ii. iii."), ("arabicPlain", "1 2 3")];

/// A small drawing of a SmartArt layout.
pub fn smartart_icon(ui: &egui::Ui, kind: DiagramKind, r: egui::Rect) {
    let x = slides_engine::scene::Xfrm { x: 0.0, y: 0.0, w: r.width() as f64, h: r.height() as f64, ..Default::default() };
    let items: Vec<(u8, String)> = match kind { DiagramKind::Hierarchy => vec![(0, "".into()), (1, "".into()), (1, "".into())], _ => vec![(0, "".into()), (0, "".into()), (0, "".into())] };
    for (preset, f, _, _) in slides_engine::objects::smartart_shapes(kind, &items, &x) {
        let g = slides_engine::geom::preset(&preset, f.w, f.h, &[]);
        for p in &g.paths {
            for poly in slides_engine::geom::flatten(&p.cmds, 6) {
                let (s, c) = f.rot.to_radians().sin_cos();
                let pts: Vec<egui::Pos2> = poly.iter().map(|q| { let (dx, dy) = (q.x - f.w / 2.0, q.y - f.h / 2.0); egui::pos2(r.min.x + (f.x + f.w / 2.0 + dx * c - dy * s) as f32, r.min.y + (f.y + f.h / 2.0 + dx * s + dy * c) as f32) }).collect();
                if pts.len() > 2 && preset != "line" { ui.painter().add(egui::Shape::convex_polygon(pts.clone(), ACCENT.gamma_multiply(0.8), Stroke::NONE)); }
                if pts.len() > 1 { ui.painter().add(egui::Shape::line(pts, Stroke::new(1.0, ACCENT))); }
            }
        }
    }
}

