//! Insert ▸ Symbol (a quick gallery and the Symbol dialog) and equations (the gallery, the
//! equation editor with structure templates and a live preview).

use crate::app::WordApp;
use egui::{vec2, Color32, RichText, Sense, Stroke, Ui};
use word_engine::equation::Equation;
use word_engine::objects::Object;

pub use qalam_kit::symbols::{COMMON, SPECIAL, SUBSETS};

pub struct SymbolDialog { pub subset: usize, pub selected: Option<char>, pub code: String, pub tab: u8 }

pub struct EquationDialog { pub pos: Option<usize>, pub linear: String, pub display: bool, pub size: f32, pub cursor: Option<usize> }

pub use qalam_kit::symbols::{glyph_font, MATH_SYMBOLS, STRUCTURES};

impl WordApp {
    pub fn insert_symbol(&mut self, c: char) {
        self.doc.replace(&c.to_string());
        self.extras.recent_symbols.retain(|x| *x != c);
        self.extras.recent_symbols.insert(0, c);
        self.extras.recent_symbols.truncate(20);
        self.canvas_focus = true;
    }
    pub fn open_symbols(&mut self) { self.extras.symbol = Some(SymbolDialog { subset: 10, selected: None, code: String::new(), tab: 0 }); }

    /// Open the equation editor: on the selected equation, or for a new one.
    pub fn open_equation(&mut self, linear: String) {
        if let Some((pos, Object::Equation(eq))) = self.selected_object() {
            let text = if linear.is_empty() { eq.linear.clone() } else { format!("{}{}", eq.linear, linear) };
            self.extras.equation = Some(EquationDialog { pos: Some(pos), cursor: Some(text.chars().count()), linear: text, display: eq.display, size: eq.size });
            return;
        }
        let empty = self.doc.current_paragraph().is_empty();
        self.extras.equation = Some(EquationDialog { pos: None, cursor: Some(linear.chars().count()), linear, display: empty, size: self.doc.current_format().size });
    }

    /// Insert a built-in or edited equation; an equation alone in its paragraph is a centered
    /// display equation.
    pub fn insert_equation(&mut self, linear: &str) {
        let empty = self.doc.current_paragraph().is_empty();
        let mut eq = Equation::new(linear);
        eq.display = empty;
        eq.size = self.doc.current_format().size;
        self.doc.insert_object(Object::Equation(eq));
        if empty { self.doc.set_alignment(word_engine::Alignment::Center); }
        self.canvas_focus = true;
    }
}

/// The Symbol drop-down: recent and common symbols, then More Symbols.
pub fn symbol_menu(app: &mut WordApp, ui: &mut Ui) {
    let mut chars: Vec<char> = app.extras.recent_symbols.clone();
    for c in COMMON.chars() { if !chars.contains(&c) { chars.push(c); } }
    chars.truncate(20);
    crate::theme::grid(ui, egui::Grid::new("symbol-quick").spacing(vec2(2.0, 2.0)), |ui| {
        for (k, c) in chars.into_iter().enumerate() {
            if symbol_cell(ui, c, false).clicked() { app.insert_symbol(c); ui.close(); }
            if k % 5 == 4 { ui.end_row(); }
        }
    });
    ui.separator();
    if crate::ribbon::small(ui, "symbol", Some("More Symbols…"), false, true, "All symbols and special characters").clicked() { app.open_symbols(); ui.close(); }
}

fn symbol_cell(ui: &mut Ui, c: char, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    let bg = if selected { Color32::from_rgb(0xCC, 0xE0, 0xFF) } else if r.hovered() { Color32::from_gray(235) } else { Color32::WHITE };
    ui.painter().rect(rect, 2.0, bg, Stroke::new(1.0, Color32::from_gray(220)), egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, c.to_string(), glyph_font(c, 17.0), Color32::from_gray(30));
    r.on_hover_text(format!("U+{:04X}", c as u32))
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    symbol_dialog(app, ctx);
    equation_dialog(app, ctx);
}

fn symbol_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.symbol.take() else { return };
    let (mut open, mut insert) = (true, None);
    crate::theme::dialog(ctx, "Symbol").open(&mut open).collapsible(false).resizable(false).default_width(560.0).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hrow(ui, |ui| {
            ui.selectable_value(&mut d.tab, 0, crate::i18n::tr("Symbols"));
            ui.selectable_value(&mut d.tab, 1, crate::i18n::tr("Special Characters"));
        });
        ui.separator();
        if d.tab == 0 {
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Subset:"));
                crate::theme::Combo::from_id_salt("subset").width(220.0).selected_text(crate::i18n::tr(&(SUBSETS[d.subset].0))).show_ui(ui, |ui| {
                    for (k, s) in SUBSETS.iter().enumerate() { ui.selectable_value(&mut d.subset, k, crate::i18n::tr(&(s.0))); }
                });
                ui.label(crate::i18n::tr("Character code:"));
                if ui.add(egui::TextEdit::singleline(&mut d.code).desired_width(60.0)).changed() {
                    if let Some(c) = u32::from_str_radix(d.code.trim().trim_start_matches("U+"), 16).ok().and_then(char::from_u32) {
                        d.selected = Some(c);
                        if let Some(k) = SUBSETS.iter().position(|s| (s.1..=s.2).contains(&(c as u32))) { d.subset = k; }
                    }
                }
            });
            let (_, a, b) = SUBSETS[d.subset];
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                crate::theme::grid(ui, egui::Grid::new("symbol-grid").spacing(vec2(1.0, 1.0)), |ui| {
                    let mut k = 0;
                    for cp in a..=b {
                        let Some(c) = char::from_u32(cp) else { continue };
                        if c.is_control() { continue; }
                        let r = symbol_cell(ui, c, d.selected == Some(c));
                        if r.clicked() { d.selected = Some(c); d.code = format!("{:04X}", cp); }
                        if r.double_clicked() { insert = Some(c); }
                        k += 1;
                        if k % 16 == 0 { ui.end_row(); }
                    }
                });
            });
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Recently used:"));
                for c in app.extras.recent_symbols.clone().into_iter().take(16) { if symbol_cell(ui, c, false).clicked() { d.selected = Some(c); } }
            });
        } else {
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (c, name) in SPECIAL {
                    let r = ui.selectable_label(d.selected == Some(*c), RichText::new(format!("{:<3} {name}", if c.is_whitespace() || (*c as u32) < 0x2000 && *c == '\u{AD}' { ' ' } else { *c })));
                    if r.clicked() { d.selected = Some(*c); }
                    if r.double_clicked() { insert = Some(*c); }
                }
            });
        }
        ui.separator();
        crate::theme::hrow(ui, |ui| {
            if let Some(c) = d.selected { ui.label(RichText::new(c.to_string()).font(glyph_font(c, 28.0))); ui.label(format!("U+{:04X}", c as u32)); }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(d.selected.is_some(), egui::Button::new(crate::i18n::tr("Insert"))).clicked() { insert = d.selected; }
            });
        });
    });
    if let Some(c) = insert { app.insert_symbol(c); }
    if open { app.extras.symbol = Some(d); } else { app.canvas_focus = true; }
}

/// Insert `text` at the editor's cursor (char index) and move the cursor past it.
fn insert_at(d: &mut EquationDialog, text: &str) {
    let at = d.cursor.unwrap_or(d.linear.chars().count()).min(d.linear.chars().count());
    let byte = d.linear.char_indices().nth(at).map(|(b, _)| b).unwrap_or(d.linear.len());
    d.linear.insert_str(byte, text);
    d.cursor = Some(at + text.chars().count());
}

fn equation_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.extras.equation.take() else { return };
    let (mut open, mut done, cancel) = (true, false, std::cell::Cell::new(false));
    crate::theme::dialog(ctx, "Equation").open(&mut open).collapsible(false).resizable(true).default_width(640.0).show(ctx, |ui| {
        app.canvas_focus = false;
        // Structures and symbols.
        ui.horizontal_wrapped(|ui| {
            for (group, icon, items) in STRUCTURES {
                let r = crate::ribbon::small(ui, icon, Some(group), false, true, group);
                crate::theme::menu(&r, |ui| {
                    for (label, src) in *items {
                        crate::theme::hrow(ui, |ui| {
                            let eq = Equation::new(src);
                            let (w, h, _) = eq.metrics();
                            let (rect, resp) = ui.allocate_exact_size(vec2(w.max(40.0) + 12.0, h + 8.0), Sense::click());
                            if resp.hovered() { ui.painter().rect_filled(rect, 3.0, Color32::from_gray(238)); }
                            crate::canvas::paint_preview(app, ui, &eq.render(0.0, 0.0), rect.min + vec2(6.0, 4.0), 1.0);
                            if resp.on_hover_text(crate::i18n::tr(&*label)).clicked() { insert_at(&mut d, src); ui.close(); }
                        });
                    }
                });
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
            for c in MATH_SYMBOLS.chars() {
                let (rect, r) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                if r.hovered() { ui.painter().rect_filled(rect, 2.0, Color32::from_gray(232)); }
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, c.to_string(), glyph_font(c, 15.0), Color32::from_gray(30));
                if r.clicked() { insert_at(&mut d, &c.to_string()); }
            }
        });
        ui.separator();
        ui.label(RichText::new(crate::i18n::tr("Type the equation in linear form: x^2, a_i, \\frac{a}{b} or a/b, \\sqrt{x}, \\sum_{i=1}^n, \\alpha, \\to…")).small().weak());
        let id = ui.make_persistent_id("equation-linear");
        let out = egui::TextEdit::multiline(&mut d.linear).id(id).font(egui::TextStyle::Monospace).desired_rows(3).desired_width(f32::INFINITY).show(ui);
        if let Some(c) = out.cursor_range { d.cursor = Some(c.primary.index.0); }
        if let Some(c) = d.cursor.filter(|_| !out.response.has_focus()) {
            // Keep the cursor where structures were inserted.
            let mut st = out.state.clone();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(c))));
            st.store(ui.ctx(), id);
        }
        crate::theme::hrow(ui, |ui| {
            crate::theme::check(ui, &mut d.display, crate::i18n::tr("Display (own line, centered)"));
            ui.label(crate::i18n::tr("Size"));
            ui.add(egui::DragValue::new(&mut d.size).range(6.0..=72.0).speed(0.5).suffix(" pt"));
        });
        // Live preview.
        let mut eq = Equation::new(&d.linear);
        eq.size = d.size;
        eq.display = d.display;
        let (w, h, _) = eq.metrics();
        let avail = ui.available_width();
        let scale = ((avail - 24.0) / w).clamp(0.3, 1.6);
        let (rect, _) = ui.allocate_exact_size(vec2(avail, h * scale + 24.0), Sense::hover());
        ui.painter().rect(rect, 4.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(210)), egui::StrokeKind::Inside);
        crate::canvas::paint_preview(app, ui, &eq.render(0.0, 0.0), egui::pos2(rect.center().x - w * scale / 2.0, rect.min.y + 12.0), scale);
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, if d.pos.is_some() { "Update" } else { "Insert" }).clicked() { done = true; }
            if crate::theme::secondary(ui, "Cancel").clicked() { cancel.set(true); }
        });
    });
    if done && !d.linear.trim().is_empty() {
        let mut eq = Equation::new(d.linear.trim());
        eq.size = d.size;
        eq.display = d.display;
        match d.pos {
            Some(pos) => { app.doc.set_object(pos, Object::Equation(eq)); app.doc.select(pos, pos + 1); }
            None => {
                let display = d.display;
                app.doc.insert_object(Object::Equation(eq));
                if display { app.doc.set_alignment(word_engine::Alignment::Center); }
            }
        }
        app.canvas_focus = true;
    } else if open && !done && !cancel.get() { app.extras.equation = Some(d); } else { app.canvas_focus = true; }
}

/// The Equation ▾ gallery.
pub fn equation_menu(app: &mut WordApp, ui: &mut Ui) {
    crate::theme::section(ui, "Built-In");
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        for (name, src) in word_engine::equation::GALLERY {
            ui.label(RichText::new(crate::i18n::tr(&*name)).small().strong());
            let eq = Equation::new(src);
            let (w, h, _) = eq.metrics();
            let (rect, r) = ui.allocate_exact_size(vec2(w.max(200.0) + 16.0, h + 10.0), Sense::click());
            if r.hovered() { ui.painter().rect_filled(rect, 3.0, Color32::from_gray(238)); }
            crate::canvas::paint_preview(app, ui, &eq.render(0.0, 0.0), rect.min + vec2(8.0, 5.0), 1.0);
            if r.clicked() { app.insert_equation(src); ui.close(); }
        }
    });
    ui.separator();
    if crate::ribbon::small(ui, "equation", Some("Insert New Equation"), false, true, "Type a new equation").clicked() { app.open_equation(String::new()); ui.close(); }
}
