//! Home ▸ Styles Pane and Formatting Pane: side panels listing the document's paragraph styles
//! (applied with a click) and describing the selection's formatting.

use crate::app::WordApp;
use egui::{Color32, RichText, Ui};
use word_engine::styles::StyleKind;
use word_engine::{Alignment, LineRule};

fn title_case(name: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in name.chars() { if up { out.extend(c.to_uppercase()); } else { out.push(c); } up = c == ' '; }
    out
}

pub fn panes(app: &mut WordApp, ui: &mut Ui) {
    if app.extras.styles_pane { styles_pane(app, ui); }
    if app.extras.formatting_pane { formatting_pane(app, ui); }
}

fn styles_pane(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::right("styles-pane").resizable(true).default_size(240.0).show(ui, |ui| {
        ui.add_space(6.0);
        crate::theme::hrow(ui, |ui| {
            ui.heading(crate::i18n::tr("Styles"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if crate::ribbon::close_button(ui, "Close the Styles pane").clicked() { app.extras.styles_pane = false; } });
        });
        let current = app.doc.paragraph_style();
        ui.label(RichText::new(crate::i18n::trf("Current style: {}", &[&(title_case(&app.doc.styles.styles.get(&current).map(|s| s.name.clone()).unwrap_or(current.clone())))])).weak());
        if ui.button(crate::i18n::tr("Clear All")).on_hover_text(crate::i18n::tr("Normal style, no direct formatting")).clicked() { app.doc.set_paragraph_style("Normal"); app.doc.clear_run_formatting(); }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut ids: Vec<String> = app.doc.styles.order.iter().filter(|id| app.doc.styles.styles.get(*id).is_some_and(|s| s.kind == StyleKind::Paragraph)).cloned().collect();
            for id in app.doc.styles.styles.keys() { if !ids.contains(id) && app.doc.styles.styles[id].kind == StyleKind::Paragraph { ids.push(id.clone()); } }
            for id in ids {
                // Each style's name drawn in (a reduced form of) its own formatting.
                let mut p = word_engine::Paragraph::new("x");
                p.style = Some(id.clone());
                let f = app.doc.run_format(&p, &p.runs[0]);
                let name = title_case(&app.doc.styles.styles[&id].name);
                let face = crate::app::face_name(word_engine::fonts::resolve(&f.font, f.bold, f.italic).face as usize);
                let color = f.color.map(|c| Color32::from_rgb(c.0, c.1, c.2)).unwrap_or(Color32::from_gray(30));
                let text = RichText::new(if f.caps == word_engine::Caps::AllCaps { name.to_uppercase() } else { name.clone() }).family(egui::FontFamily::Name(face.into())).size(f.size.clamp(9.0, 18.0)).color(color);
                let r = ui.selectable_label(id == current, text).on_hover_text(format!("{name}: {} {} pt{}{}", f.font, f.size, if f.bold { ", bold" } else { "" }, if f.italic { ", italic" } else { "" }));
                if r.clicked() { app.doc.set_paragraph_style(&id); app.canvas_focus = true; }
            }
        });
    });
}

fn formatting_pane(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::right("formatting-pane").resizable(true).default_size(240.0).show(ui, |ui| {
        ui.add_space(6.0);
        crate::theme::hrow(ui, |ui| {
            ui.heading(crate::i18n::tr("Formatting"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if crate::ribbon::close_button(ui, "Close the Formatting pane").clicked() { app.extras.formatting_pane = false; } });
        });
        let f = app.doc.current_format();
        let p = app.doc.current_para_format();
        let sel = app.doc.selected_text();
        ui.label(RichText::new(if sel.is_empty() { "At the caret".to_string() } else { crate::i18n::trf("Selected: “{}”", &[&sel.chars().take(30).collect::<String>()]) }).weak());
        ui.separator();
        ui.label(RichText::new(crate::i18n::tr("Font")).strong());
        crate::theme::grid(ui, egui::Grid::new("fmt-font").num_columns(2), |ui| {
            let mut row = |k: &str, v: String| { ui.label(crate::i18n::tr(k)); ui.label(v); ui.end_row(); };
            row("Font:", f.font.clone());
            row("Size:", format!("{} pt", f.size));
            let styles: Vec<&str> = [(f.bold, "Bold"), (f.italic, "Italic"), (f.underline, "Underline"), (f.strike != word_engine::Strike::None, "Strikethrough")].iter().filter(|x| x.0).map(|x| x.1).collect();
            row("Style:", if styles.is_empty() { "Regular".into() } else { styles.join(", ") });
            row("Color:", f.color.map(|c| format!("#{}", c.hex())).unwrap_or("Automatic".into()));
            if let Some(h) = f.highlight { row("Highlight:", format!("#{}", h.hex())); }
            if f.spacing != 0.0 { row("Spacing:", format!("{:+} pt", f.spacing)); }
        });
        ui.separator();
        ui.label(RichText::new(crate::i18n::tr("Paragraph")).strong());
        crate::theme::grid(ui, egui::Grid::new("fmt-para").num_columns(2), |ui| {
            let mut row = |k: &str, v: String| { ui.label(crate::i18n::tr(k)); ui.label(v); ui.end_row(); };
            row("Style:", title_case(&app.doc.paragraph_style()));
            row("Alignment:", match p.alignment { Alignment::Left => "Left", Alignment::Center => "Centered", Alignment::Right => "Right", Alignment::Justify => "Justified" }.into());
            row("Indent:", format!("Left {:.2}\", Right {:.2}\"", p.indent_left as f32 / 1440.0, p.indent_right as f32 / 1440.0));
            if p.first_line != 0 { row(if p.first_line > 0 { "First line:" } else { "Hanging:" }, format!("{:.2}\"", p.first_line.abs() as f32 / 1440.0)); }
            row("Spacing:", crate::i18n::trf("Before {} pt, After {} pt", &[&p.space_before, &p.space_after]));
            row("Line spacing:", match p.line_rule { LineRule::Auto => crate::i18n::trf("Multiple {}", &[&format!("{:.2}", p.line_spacing)]), LineRule::Exact => crate::i18n::trf("Exactly {} pt", &[&p.line_spacing]), LineRule::AtLeast => crate::i18n::trf("At least {} pt", &[&p.line_spacing]) });
            if p.rtl == Some(true) { row("Direction:", "Right-to-left".into()); }
            if let Some(d) = p.drop_cap { row("Drop cap:", crate::i18n::trf(if d.margin { "{} lines, in margin" } else { "{} lines" }, &[&d.lines])); }
        });
        ui.separator();
        if ui.button(crate::i18n::tr("Clear Formatting")).clicked() { app.doc.clear_run_formatting(); }
    });
}
