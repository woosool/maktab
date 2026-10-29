//! The ribbon (ui/ribbon.js): a title bar with quick-access commands, a row of tabs, and one white
//! card of command groups. Icons are the JS editor's command icons (icon_data.rs), rasterized by
//! resvg at the display's pixel density so they stay sharp at any scale.

use crate::app::{Pending, WordApp};
pub use qalam_kit::colors::*;
pub use qalam_kit::widgets::*;
use egui::{pos2, vec2, Align2, Color32, ComboBox, FontFamily, FontId, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextureHandle, Ui};
use std::cell::RefCell;
use std::collections::HashMap;
use word_engine::numbering::ListKind;
use word_engine::{Alignment, Color, LineRule, Markup, NoteKind, RunProps, VertAlign};
use word_engine::objects::Object;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab { #[default] Home, Custom, Insert, Draw, Design, Layout, References, Mailings, Review, View, Table, ShapeFormat, ChartDesign, SmartArt, PictureFormat, HeaderFooter, Equation }

/// The paragraph direction buttons: a pilcrow with an arrow pointing the way text runs.
fn direction_button(ui: &mut Ui, rtl: bool, on: bool, tip: &str) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
    button_bg(ui, rect, &r, on);
    let c = rect.center();
    let p = ui.painter();
    let ink = Stroke::new(1.3, c_ink2());
    let (stem, bowl) = if rtl { (c.x + 3.0, c.x + 1.0) } else { (c.x - 3.0, c.x - 1.0) };
    // ¶ drawn as a filled bowl and two stems.
    p.circle_filled(pos2(bowl + if rtl { -2.0 } else { 2.0 }, c.y - 4.0), 2.6, c_ink2());
    p.line_segment([pos2(stem + 1.0, c.y - 7.0), pos2(stem + 1.0, c.y + 2.0)], ink);
    p.line_segment([pos2(stem + 4.0, c.y - 7.0), pos2(stem + 4.0, c.y + 2.0)], ink);
    p.line_segment([pos2(stem - 2.0, c.y - 7.0), pos2(stem + 5.0, c.y - 7.0)], ink);
    let (a, b) = if rtl { (c.x + 8.0, c.x - 8.0) } else { (c.x - 8.0, c.x + 8.0) };
    let y = c.y + 6.0;
    p.line_segment([pos2(a, y), pos2(b, y)], Stroke::new(1.3, ACCENT));
    let d = if rtl { 3.0 } else { -3.0 };
    p.line_segment([pos2(b, y), pos2(b + d, y - 2.5)], Stroke::new(1.3, ACCENT));
    p.line_segment([pos2(b, y), pos2(b + d, y + 2.5)], Stroke::new(1.3, ACCENT));
    r.on_hover_text(crate::i18n::tr(&tip))
}

pub fn ribbon(app: &mut WordApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let in_table = app.doc.current_cell().is_some();
    if app.tab == Tab::Table && !in_table { app.tab = Tab::Home; }
    // Contextual tabs for the selected object.
    let object = app.selected_object().and_then(|(_, o)| match o { Object::Shape(_) => Some(Tab::ShapeFormat), Object::Chart(_) => Some(Tab::ChartDesign), Object::Diagram(_) => Some(Tab::SmartArt), Object::Equation(_) | Object::Ink(_) => None })
        .or(app.selected_picture().map(|_| Tab::PictureFormat));
    if matches!(app.tab, Tab::ShapeFormat | Tab::ChartDesign | Tab::SmartArt | Tab::PictureFormat) && object != Some(app.tab) { app.tab = Tab::Insert; }
    if app.extras.story.is_none() && app.tab == Tab::HeaderFooter { app.tab = Tab::Insert; }
    let equation = matches!(app.selected_object(), Some((_, Object::Equation(_))));
    if app.tab == Tab::Equation && !equation { app.tab = Tab::Insert; }
    egui::Panel::top("ribbon").show_separator_line(false).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 6 })).show(ui, |ui| {
        // Tabs, Tell Me, quick access and the collapse button on one row.
        let mut active_tab = Rect::NOTHING;
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 30.0), row_layout(), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let file = ui.add(egui::Button::new(RichText::new(crate::i18n::tr("File")).size(14.0).color(c_ink()).family(FontFamily::Name("ui-bold".into()))).frame(false).min_size(vec2(44.0, 30.0)));
            crate::theme::menu(&file, |ui| file_menu(app, ui, &ctx));
            let custom_name = app.extras.options.settings.custom_tab_name.clone();
            let mut tabs = vec![(Tab::Home, "Home"), (Tab::Insert, "Insert"), (Tab::Draw, "Draw"), (Tab::Design, "Design"), (Tab::Layout, "Layout"), (Tab::References, "References"), (Tab::Mailings, "Mailings"), (Tab::Review, "Review"), (Tab::View, "View")];
            // Options ▸ Customize Ribbon: hidden tabs, and the custom tab after View.
            tabs.retain(|t| !app.tab_hidden(t.1));
            if !app.extras.options.settings.custom_tab.is_empty() { tabs.push((Tab::Custom, custom_name.as_str())); }
            if in_table { tabs.push((Tab::Table, "Table")); }
            match object { Some(Tab::ShapeFormat) => tabs.push((Tab::ShapeFormat, "Shape Format")), Some(Tab::ChartDesign) => tabs.push((Tab::ChartDesign, "Chart Design")), Some(Tab::SmartArt) => tabs.push((Tab::SmartArt, "SmartArt Design")), Some(Tab::PictureFormat) => tabs.push((Tab::PictureFormat, "Picture Format")), _ => {} }
            if app.extras.story.is_some() { tabs.push((Tab::HeaderFooter, "Header & Footer")); }
            if equation { tabs.push((Tab::Equation, "Equation")); }
            for (tab, label) in tabs {
                let label_t = if tab == Tab::Custom { crate::i18n::shape(label) } else { crate::i18n::tr(label) };
                let label = label_t.as_str();
                let selected = app.tab == tab;
                let contextual = matches!(tab, Tab::Table | Tab::ShapeFormat | Tab::ChartDesign | Tab::SmartArt | Tab::PictureFormat | Tab::HeaderFooter | Tab::Equation);
                let font = FontId::new(14.0, if selected { FontFamily::Name("ui-bold".into()) } else { FontFamily::Proportional });
                let g = ui.painter().layout_no_wrap(label.to_string(), font, if selected || contextual { ACCENT } else { c_ink2() });
                let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 22.0, 30.0), Sense::click());
                if selected {
                    // The active tab: the ribbon's color, bordered on top and sides, open at the
                    // bottom where it joins the ribbon (painted again after the ribbon).
                    let t = Rect::from_min_max(rect.min + vec2(0.0, 2.0), rect.max + vec2(0.0, 1.0));
                    ui.painter().rect_filled(t, 0.0, c_card());
                    let s = Stroke::new(1.0, c_line());
                    ui.painter().line_segment([t.left_bottom(), t.left_top()], s);
                    ui.painter().line_segment([t.left_top(), t.right_top()], s);
                    ui.painter().line_segment([t.right_top(), t.right_bottom()], s);
                    active_tab = t;
                } else if r.hovered() { ui.painter().rect_filled(rect.shrink2(vec2(0.0, 3.0)), crate::theme::RADIUS as f32, c_hover()); }
                ui.painter().galley(pos2(rect.center().x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0), g, c_ink());
                if r.clicked() { app.tab = tab; if app.extras.ribbon_collapsed && r.double_clicked() { app.extras.ribbon_collapsed = false; } }
                if r.double_clicked() { app.extras.ribbon_collapsed = !app.extras.ribbon_collapsed; }
            }
            ui.with_layout(far_layout(), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let (crect, collapse) = ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
                button_bg(ui, crect, &collapse, false);
                // A chevron: up collapses the ribbon, down shows it again.
                let c = crect.center();
                let d = if app.extras.ribbon_collapsed { 1.0 } else { -1.0 };
                ui.painter().line_segment([c + vec2(-5.0, -2.5 * d), c + vec2(0.0, 2.5 * d)], Stroke::new(1.6, c_ink2()));
                ui.painter().line_segment([c + vec2(0.0, 2.5 * d), c + vec2(5.0, -2.5 * d)], Stroke::new(1.6, c_ink2()));
                if collapse.on_hover_text(if app.extras.ribbon_collapsed { "Show the ribbon" } else { "Collapse the ribbon" }).clicked() { app.extras.ribbon_collapsed = !app.extras.ribbon_collapsed; }
                let more = small(ui, "more", None, false, true, "More");
                crate::theme::menu(&more, |ui| file_menu(app, ui, &ctx));
                ui.add_space(10.0);
                if small(ui, "save", None, false, true, "Save (⌘S)").clicked() { app.save(&ctx, false); }
                if small(ui, "redo", None, false, app.doc.can_redo(), "Redo (⇧⌘Z)").clicked() { app.doc.redo(); }
                let (arect, ar) = ui.allocate_exact_size(vec2(12.0, 24.0), Sense::click());
                button_bg(ui, arect, &ar, false);
                caret(ui, arect.center());
                crate::theme::menu(&ar, |ui| {
                    let depth = app.doc.history_len();
                    if depth == 0 { ui.weak(crate::i18n::tr("Nothing to undo")); }
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for n in 1..=depth.min(30) { if ui.button(crate::i18n::trf(if n == 1 { "Undo {} action" } else { "Undo {} actions" }, &[&n])).clicked() { app.doc.undo_steps(n); ui.close(); } }
                    });
                });
                if small(ui, "undo", None, false, app.doc.can_undo(), "Undo (⌘Z)").clicked() { app.doc.undo(); }
                ui.add_space(10.0);
                tell_me(app, ui, &ctx);
            });
        });
        if app.extras.ribbon_collapsed { return; }
        // Card.
        let painter = ui.painter().clone();
        egui::Frame::NONE.fill(c_card()).corner_radius(crate::theme::RADIUS).inner_margin(egui::Margin::symmetric(8, 2))
            .stroke(Stroke::new(1.0, c_line()))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // A tab wider than the window draws compactly; its full width is remembered.
                let full_id = egui::Id::unique(("ribbon-full-width", app.tab as usize));
                let full = ui.data(|d| d.get_temp::<f32>(full_id)).unwrap_or(0.0);
                let is_compact = full > ui.available_width();
                COMPACT.with(|c| c.set(is_compact));
                let out = egui::ScrollArea::horizontal().id_salt("ribbon-scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded).show(ui, |ui| {
                    ui.with_layout(row_layout(), |ui| {
                        ui.set_height(GROUP_H + 6.0);
                        let tab_name = match app.tab { Tab::Home => "Home", Tab::Insert => "Insert", Tab::Draw => "Draw", Tab::Design => "Design", Tab::Layout => "Layout", Tab::References => "References", Tab::Mailings => "Mailings", Tab::Review => "Review", Tab::View => "View", _ => "" };
                        let hidden: std::collections::HashSet<String> = app.extras.options.settings.hidden_groups.iter().filter_map(|k| k.strip_prefix(&format!("{tab_name}/")).map(String::from)).collect();
                        HIDDEN_GROUPS.with(|h| *h.borrow_mut() = hidden);
                        if (app.tab == Tab::Custom && app.extras.options.settings.custom_tab.is_empty()) || app.tab_hidden(tab_name) { app.tab = Tab::Home; }
                        match app.tab {
                            Tab::Custom => custom_tab(app, ui, &ctx),
                            Tab::Home => home(app, ui, &ctx),
                            Tab::Insert => insert(app, ui),
                            Tab::Layout => layout(app, ui),
                            Tab::References => references(app, ui),
                            Tab::Mailings => crate::mailings::tab(app, ui),
                            Tab::Review => review(app, ui),
                            Tab::View => view(app, ui),
                            Tab::Table => table(app, ui),
                            Tab::ShapeFormat => shape_format(app, ui),
                            Tab::ChartDesign => chart_design(app, ui),
                            Tab::SmartArt => smartart_design(app, ui),
                            Tab::PictureFormat => picture_format(app, ui),
                            Tab::Draw => draw_tab(app, ui),
                            Tab::Design => design_tab(app, ui),
                            Tab::HeaderFooter => header_footer_tab(app, ui),
                            Tab::Equation => equation_tab(app, ui),
                        }
                    });
                });
                COMPACT.with(|c| c.set(false));
                if !is_compact { ui.data_mut(|d| d.insert_temp(full_id, out.content_size.x)); }
            });
        // Join the active tab to the ribbon: cover the ribbon's top edge beneath it.
        if active_tab.is_positive() {
            painter.rect_filled(Rect::from_min_max(pos2(active_tab.min.x + 1.0, active_tab.max.y - 2.0), pos2(active_tab.max.x - 1.0, active_tab.max.y + 1.5)), 0.0, c_card());
        }
    });
}

fn file_menu(app: &mut WordApp, ui: &mut Ui, ctx: &egui::Context) {
    ui.set_width(260.0);
    let item = qalam_kit::widgets::file_item;
    if item(ui, "filePlus", "New", "⌘N") { app.guarded(ctx, Pending::New); ui.close(); }
    if item(ui, "folderOpen", "Open…", "⌘O") { app.guarded(ctx, Pending::Open); ui.close(); }
    ui.separator();
    if item(ui, "save", "Save", "⌘S") { app.save(ctx, false); ui.close(); }
    if item(ui, "rename", "Save As…", "⇧⌘S") { app.save(ctx, true); ui.close(); }
    ui.separator();
    if item(ui, "print", "Print…", "⌘P") { app.open_print(); ui.close(); }
    if item(ui, "pdf", "Export PDF…", "") { app.export_pdf(ctx); ui.close(); }
    if item(ui, "download", "Export Markdown…", "") { app.export(ctx, true); ui.close(); }
    if item(ui, "pasteText", "Export Plain Text…", "") { app.export(ctx, false); ui.close(); }
    ui.separator();
    if item(ui, "pageSetup", "Page Setup…", "") { app.dialogs.page_setup = Some(app.doc.page.clone()); ui.close(); }
    if item(ui, "preferences", "Options…", "") { app.extras.options.dialog = Some((0, app.extras.options.settings.clone())); ui.close(); }
    if item(ui, "keyboard", "Keyboard Shortcuts", "") { app.dialogs.shortcuts = true; ui.close(); }
    if item(ui, "info", &crate::i18n::trf("About {}", &[&crate::APP_NAME]), "") { app.dialogs.about = true; ui.close(); }
    if item(ui, "download", "Check for Updates…", "") { crate::update::check(ui.ctx(), true, None); ui.close(); }
    ui.separator();
    if item(ui, "close", "Quit", "⌘Q") { app.guarded(ctx, Pending::Quit); ui.close(); }
}

fn home(app: &mut WordApp, ui: &mut Ui, ctx: &egui::Context) {
    let f = app.doc.current_format();
    let pf = app.doc.current_para_format();
    group(ui, "Clipboard", |ui| {
        let (paste, options) = large_split(ui, "paste", "Paste", "Paste (⌘V)");
        if paste.clicked() { app.paste_from_system(); }
        crate::theme::menu(&options, |ui| {
            ui.label(RichText::new(crate::i18n::tr("Paste Options")).strong());
            if small(ui, "paste", Some("Keep Source Formatting"), false, true, "").clicked() { app.paste_from_system(); ui.close(); }
            if small(ui, "formatPainter", Some("Merge Formatting"), false, true, "Use the formatting at the caret").clicked() { app.paste_merged(); ui.close(); }
            if small(ui, "pasteText", Some("Keep Text Only"), false, true, "").clicked() { app.paste_plain(); ui.close(); }
        });
        rows(ui, |ui| {
            let has = !app.doc.selection.is_caret();
            if small(ui, "cut", Some("Cut"), false, has, "Cut (⌘X)").clicked() { app.cut(ctx); }
            if small(ui, "copy", Some("Copy"), false, has, "Copy (⌘C)").clicked() { app.copy(ctx); }
            let painting = app.format_painter.is_some();
            if small(ui, "formatPainter", Some("Format Painter"), painting, true, "Copy formatting, then select text to apply it").clicked() {
                app.format_painter = if painting { None } else { Some(app.doc.current_run_props()) };
            }
        });
    });
    let font_dialog = group_l(ui, "Font", "Font options (⌘D)", |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                font_picker(app, ui, &f.font);
                size_picker(app, ui, f.size);
                if small(ui, "fontGrow", None, false, true, "Increase font size (⌘])").clicked() { app.doc.grow_font(true); }
                if small(ui, "fontShrink", None, false, true, "Decrease font size (⌘[)").clicked() { app.doc.grow_font(false); }
                let case = small(ui, "changeCase", None, false, true, "Change case");
                crate::theme::menu(&case, |ui| {
                    use word_engine::editing::CaseMode as C;
                    for (mode, label) in [(C::Sentence, "Sentence case."), (C::Lower, "lowercase"), (C::Upper, "UPPERCASE"), (C::Capitalize, "Capitalize Each Word"), (C::Toggle, "tOGGLE cASE")] {
                        if ui.button(crate::i18n::tr(&label)).clicked() { app.doc.change_case(mode); ui.close(); }
                    }
                });
                if small(ui, "clearFormat", None, false, true, "Clear formatting (⌘Space)").clicked() { app.doc.clear_run_formatting(); }
            });
            ui.horizontal(|ui| {
                if small(ui, "bold", None, f.bold, true, "Bold (⌘B)").clicked() { app.doc.toggle_bold(); }
                if small(ui, "italic", None, f.italic, true, "Italic (⌘I)").clicked() { app.doc.toggle_italic(); }
                if small(ui, "underline", None, f.underline, true, "Underline (⌘U)").clicked() { app.doc.toggle_underline(); }
                if small(ui, "strike", None, f.strike != word_engine::Strike::None, true, "Strikethrough").clicked() { app.doc.toggle_strike(); }
                if small(ui, "sub", None, f.vert_align == VertAlign::Subscript, true, "Subscript (⌘=)").clicked() { app.doc.toggle_vert_align(VertAlign::Subscript); }
                if small(ui, "sup", None, f.vert_align == VertAlign::Superscript, true, "Superscript (⇧⌘=)").clicked() { app.doc.toggle_vert_align(VertAlign::Superscript); }
                ui.add_space(4.0);
                let hl = f.highlight.map(|c| Color32::from_rgb(c.0, c.1, c.2)).unwrap_or(Color32::from_rgb(255, 255, 0));
                let fx = small(ui, "textEffects", None, f.outline || f.shadow || f.glow.is_some() || f.reflection || f.gradient.is_some(), true, "Text effects");
                crate::theme::menu(&fx, |ui| {
                    let mut outline = f.outline;
                    let mut shadow = f.shadow;
                    if ui.checkbox(&mut outline, crate::i18n::tr("Outline")).changed() { app.doc.set_run_props(RunProps { outline: Some(outline), ..Default::default() }); }
                    if ui.checkbox(&mut shadow, crate::i18n::tr("Shadow")).changed() { app.doc.set_run_props(RunProps { shadow: Some(shadow), ..Default::default() }); }
                    let mut reflection = f.reflection;
                    if ui.checkbox(&mut reflection, crate::i18n::tr("Reflection")).changed() { app.doc.set_run_props(RunProps { reflection: Some(reflection), ..Default::default() }); }
                    crate::theme::submenu(ui, crate::i18n::tr("Glow"), |ui| {
                        if none_row(ui, "No Glow") { app.doc.set_run_props(RunProps { glow: Some(None), ..Default::default() }); ui.close(); }
                        for radius in [5.0, 10.0] {
                            menu_heading(ui, &format!("{radius} pt"));
                            ui.allocate_ui_with_layout(vec2(6.0 * 30.0, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                for c in [Color(0x44, 0x72, 0xC4), Color(0xED, 0x7D, 0x31), Color(0xA5, 0xA5, 0xA5), Color(0xFF, 0xC0, 0x00), Color(0x5B, 0x9B, 0xD5), Color(0x70, 0xAD, 0x47)] {
                                    let col = Color32::from_rgb(c.0, c.1, c.2);
                                    let (rect, r) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                    // A soft halo around the swatch, as the glow looks.
                                    let halo = if radius > 6.0 { 4.0 } else { 2.0 };
                                    ui.painter().rect_filled(rect, 8.0, col.gamma_multiply(0.35));
                                    ui.painter().rect(rect.shrink(halo), 5.0, col, Stroke::NONE, StrokeKind::Inside);
                                    if r.hovered() { ui.painter().rect_stroke(rect.expand(2.0), 9.0, Stroke::new(1.5, ACCENT), StrokeKind::Inside); }
                                    if r.clicked() { app.doc.set_run_props(RunProps { glow: Some(Some(word_engine::Glow { color: c, radius })), ..Default::default() }); ui.close(); }
                                }
                            });
                        }
                    });
                    crate::theme::submenu(ui, crate::i18n::tr("Gradient Fill"), |ui| {
                        if ui.button(crate::i18n::tr("No Gradient")).clicked() { app.doc.set_run_props(RunProps { gradient: Some(None), ..Default::default() }); ui.close(); }
                        for (name, a, b) in [("Blue", Color(0x5B, 0x9B, 0xD5), Color(0x1F, 0x3B, 0x73)), ("Sunset", Color(0xFF, 0xC0, 0x00), Color(0xC0, 0x00, 0x00)), ("Forest", Color(0xA9, 0xD1, 0x8E), Color(0x37, 0x56, 0x23)), ("Silver", Color(0xD9, 0xD9, 0xD9), Color(0x40, 0x40, 0x40))] {
                            ui.horizontal(|ui| {
                                let (rect, r) = ui.allocate_exact_size(vec2(36.0, 16.0), Sense::click());
                                let mut mesh = egui::Mesh::default();
                                mesh.colored_vertex(rect.left_top(), Color32::from_rgb(a.0, a.1, a.2)); mesh.colored_vertex(rect.right_top(), Color32::from_rgb(a.0, a.1, a.2));
                                mesh.colored_vertex(rect.left_bottom(), Color32::from_rgb(b.0, b.1, b.2)); mesh.colored_vertex(rect.right_bottom(), Color32::from_rgb(b.0, b.1, b.2));
                                mesh.add_triangle(0, 1, 2); mesh.add_triangle(1, 3, 2);
                                ui.painter().add(egui::Shape::mesh(mesh));
                                if r.clicked() | ui.button(crate::i18n::tr(&name)).clicked() { app.doc.set_run_props(RunProps { gradient: Some(Some((a, b))), ..Default::default() }); ui.close(); }
                            });
                        }
                    });
                    ui.separator();
                    if ui.button(crate::i18n::tr("Clear Text Effects")).clicked() { app.doc.set_run_props(RunProps { outline: Some(false), shadow: Some(false), glow: Some(None), reflection: Some(false), gradient: Some(None), ..Default::default() }); ui.close(); }
                });
                let (main, arrow) = color_split(ui, "highlight", hl, "Text highlight color");
                if main.clicked() { let name = app.last_highlight.clone(); app.doc.set_run_props(RunProps { highlight: Some(Some(name)), ..Default::default() }); }
                crate::theme::menu(&arrow, |ui| {
                    if none_row(ui, "No Color") { app.doc.set_run_props(RunProps { highlight: Some(None), shading: Some(None), ..Default::default() }); ui.close(); }
                    let names: Vec<(&str, Color)> = word_engine::HIGHLIGHTS.iter().map(|(n, c)| (&**n, *c)).collect();
                    for row in names.chunks(5) {
                        ui.allocate_ui_with_layout(vec2(5.0 * 30.0, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for (name, c) in row {
                                let sel = app.last_highlight == *name;
                                if swatch(ui, Color32::from_rgb(c.0, c.1, c.2), 24.0, sel).on_hover_text(crate::i18n::tr(name)).clicked() { app.last_highlight = name.to_string(); app.doc.set_run_props(RunProps { highlight: Some(Some(name.to_string())), ..Default::default() }); ui.close(); }
                            }
                        });
                    }
                });
                let tc = app.last_color;
                let (main, arrow) = color_split(ui, "textColor", Color32::from_rgb(tc.0, tc.1, tc.2), "Font color");
                if main.clicked() { app.doc.set_run_props(RunProps { color: Some(Some(tc)), ..Default::default() }); }
                crate::theme::menu(&arrow, |ui| {
                    if let Some(c) = color_menu(ui, "Automatic") {
                        if let Some(c) = c { app.last_color = c; }
                        app.doc.set_run_props(RunProps { color: Some(c), ..Default::default() });
                    }
                });
            });
        });
    });
    if font_dialog { app.open_font_dialog(); }
    let para_dialog = group_l(ui, "Paragraph", "Paragraph options", |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                let kind = pf.numbering.and_then(|(n, _)| app.doc.numbering.kind(n));
                let (b, b_more) = small_split(ui, "bullets", kind == Some(ListKind::Bullet), "Bullets");
                if b.clicked() { app.doc.toggle_list(ListKind::Bullet); }
                crate::theme::menu(&b_more, |ui| list_gallery(app, ui, true));
                let (n, n_more) = small_split(ui, "numbers", kind == Some(ListKind::Number), "Numbering");
                if n.clicked() { app.doc.toggle_list(ListKind::Number); }
                crate::theme::menu(&n_more, |ui| list_gallery(app, ui, false));
                let ml = small(ui, "multilevel", None, false, true, "Multilevel list");
                crate::theme::menu(&ml, |ui| multilevel_gallery(app, ui));
                if small(ui, "outdent", None, false, true, "Decrease indent").clicked() { app.doc.adjust_indent(false); }
                if small(ui, "indent", None, false, true, "Increase indent").clicked() { app.doc.adjust_indent(true); }
                let sort = small(ui, "sort", None, false, true, "Sort the selected paragraphs");
                crate::theme::menu(&sort, |ui| {
                    if ui.button(crate::i18n::tr("Sort A → Z (0 → 9)")).clicked() { if !app.doc.sort_paragraphs(false) { app.notify(ui.ctx(), "Select two or more paragraphs to sort"); } ui.close(); }
                    if ui.button(crate::i18n::tr("Sort Z → A (9 → 0)")).clicked() { if !app.doc.sort_paragraphs(true) { app.notify(ui.ctx(), "Select two or more paragraphs to sort"); } ui.close(); }
                });
                if small(ui, "pilcrow", None, app.show_marks, true, "Show formatting marks (⇧⌘8)").clicked() { app.show_marks = !app.show_marks; }
            });
            ui.horizontal(|ui| {
                // The buttons name the side of the page, also in right-to-left paragraphs.
                let visual = app.doc.visual_alignment();
                for (a, icon, tip) in [(Alignment::Left, "alignLeft", "Align left (⌘L)"), (Alignment::Center, "alignCenter", "Center (⌘E)"), (Alignment::Right, "alignRight", "Align right (⌘R)"), (Alignment::Justify, "alignJustify", "Justify (⌘J)")] {
                    if small(ui, icon, None, visual == a, true, tip).clicked() { app.doc.set_visual_alignment(a); }
                }
                let rtl = app.doc.paragraph_rtl(app.doc.current_paragraph());
                if direction_button(ui, false, !rtl, "Left-to-right text direction").clicked() { app.doc.set_paragraph_direction(false); }
                if direction_button(ui, true, rtl, "Right-to-left text direction").clicked() { app.doc.set_paragraph_direction(true); }
                let ls = small(ui, "lineSpacing", None, false, true, "Line and paragraph spacing");
                crate::theme::menu(&ls, |ui| line_spacing_menu(app, ui));
                let sh = small(ui, "shading", None, pf.shading.is_some(), true, "Shading");
                crate::theme::menu(&sh, |ui| {
                    if let Some(c) = color_menu(ui, "No Color") { app.doc.set_para_props(word_engine::ParaProps { shading: Some(c), ..Default::default() }); }
                });
                let bd = small(ui, "borders", None, false, true, "Borders");
                crate::theme::menu(&bd, |ui| borders_menu(app, ui));
            });
        });
    });
    if para_dialog { crate::ui::open_paragraph_dialog(app); }
    group(ui, "Styles", |ui| style_gallery(app, ui));
    group(ui, "Panes", |ui| {
        if large(ui, "stylesPane", "Styles Pane", app.extras.styles_pane, true, "All styles: apply, see and compare").clicked() { app.extras.styles_pane = !app.extras.styles_pane; }
        if large(ui, "formattingPane", "Formatting Pane", app.extras.formatting_pane, true, "The selection's formatting").clicked() { app.extras.formatting_pane = !app.extras.formatting_pane; }
    });
    group(ui, "Voice", |ui| {
        if large(ui, "dictate", "Dictate", false, true, "Type with your voice").clicked() {
            let how = if cfg!(target_os = "macos") { "press the Fn (🌐) key twice" } else if cfg!(target_os = "windows") { "press Windows + H" } else { "use your desktop's dictation tool" };
            app.notify(ctx, crate::i18n::trf("Dictation uses your system's speech input: {}, then speak — the text is typed at the caret.", &[&how]));
        }
    });
    group(ui, "Editing", |ui| {
        rows(ui, |ui| {
            if small(ui, "find", Some("Find"), app.find.open, true, "Find (⌘F)").clicked() { app.find.open = true; app.find.focus = true; }
            if small(ui, "replace", Some("Replace"), false, true, "Replace (⌘H)").clicked() { app.find.open = true; app.find.focus = true; }
            if small(ui, "selectAll", Some("Select All"), false, true, "Select all (⌘A)").clicked() { app.doc.select_all(); }
        });
    });
}

/// Bullet or Numbering Library: previews drawn with the list's own marker text.
fn list_gallery(app: &mut WordApp, ui: &mut Ui, bullets: bool) {
    use word_engine::editing::{BULLET_LIBRARY, NUMBER_LIBRARY};
    ui.label(RichText::new(if bullets { "Bullet Library" } else { "Numbering Library" }).strong());
    let tile = |ui: &mut Ui, lines: [String; 3]| -> Response {
        let (rect, r) = ui.allocate_exact_size(vec2(if bullets { 40.0 } else { 74.0 }, if bullets { 40.0 } else { 74.0 }), Sense::click());
        ui.painter().rect(rect, crate::theme::RADIUS as f32, if r.hovered() { c_hover() } else { c_card() }, Stroke::new(1.0, c_line()), StrokeKind::Inside);
        if bullets {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, &lines[0], FontId::proportional(20.0), c_ink());
        } else {
            for (k, t) in lines.iter().enumerate() {
                let y = rect.min.y + 16.0 + k as f32 * 21.0;
                ui.painter().text(pos2(rect.min.x + 8.0, y), Align2::LEFT_CENTER, t, FontId::proportional(11.0), c_ink());
                ui.painter().line_segment([pos2(rect.min.x + 30.0, y), pos2(rect.max.x - 6.0, y)], Stroke::new(1.0, c_line()));
            }
        }
        r
    };
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(if bullets { 200.0 } else { 320.0 });
        if bullets {
            for &b in BULLET_LIBRARY {
                if tile(ui, [b.to_string(), String::new(), String::new()]).clicked() { app.doc.apply_bullet(b); ui.close(); }
            }
        } else {
            let rtl = app.doc.paragraph_rtl(app.doc.current_paragraph());
            let library = if rtl { word_engine::editing::RTL_NUMBER_LIBRARY } else { NUMBER_LIBRARY };
            for &(fmt, pattern) in library {
                let label = |n: u32| pattern.replace("%1", &word_engine::numbering::format_number(n, fmt));
                if tile(ui, [label(1), label(2), label(3)]).clicked() { app.doc.apply_numbering(fmt, pattern); ui.close(); }
            }
        }
    });
    // Named custom lists of this kind.
    let mine: Vec<word_engine::editing::CustomList> = app.custom_lists.iter().filter(|l| (l.levels[0].fmt == word_engine::numbering::NumFmt::Bullet) == bullets).cloned().collect();
    if !mine.is_empty() {
        ui.separator();
        ui.label(RichText::new(crate::i18n::tr("My Lists")).strong());
        for l in mine {
            let preview = l.preview();
            if ui.button(format!("{}   {}", preview[0], l.name)).on_hover_text(preview.iter().take(3).cloned().collect::<Vec<_>>().join("  ·  ")).clicked() { app.doc.apply_custom_list(&l); ui.close(); }
        }
    }
    ui.separator();
    if ui.button(if bullets { "Define New Bullet…" } else { "Define New Number Format…" }).clicked() { app.open_list_dialog(if bullets { crate::app::ListDialogKind::Bullet } else { crate::app::ListDialogKind::Number }); ui.close(); }
    let in_list = app.doc.current_para_format().numbering.is_some();
    ui.add_enabled_ui(in_list, |ui| {
        if ui.button(crate::i18n::tr("Change List Level ▸")).on_hover_text(crate::i18n::tr("Tab / Shift+Tab")).clicked() { app.doc.change_list_level(1); ui.close(); }
        if !bullets {
            if ui.button(crate::i18n::tr("Restart at 1")).clicked() { app.doc.restart_numbering(1); ui.close(); }
            if ui.button(crate::i18n::tr("Continue Numbering")).clicked() { app.doc.continue_numbering(); ui.close(); }
            ui.horizontal(|ui| {
                let id = ui.make_persistent_id("set-value");
                let mut n: u32 = ui.data(|d| d.get_temp(id)).unwrap_or(1);
                ui.label(crate::i18n::tr("Set value"));
                ui.add(egui::DragValue::new(&mut n).range(0..=32767));
                ui.data_mut(|d| d.insert_temp(id, n));
                if ui.button(crate::i18n::tr("Set")).clicked() { app.doc.restart_numbering(n); ui.close(); }
            });
        }
        if ui.button(crate::i18n::tr("None (remove list)")).clicked() {
            let kind = app.doc.current_para_format().numbering.and_then(|(n, _)| app.doc.numbering.kind(n));
            if let Some(k) = kind { app.doc.toggle_list(k); }
            ui.close();
        }
    });
}

fn multilevel_gallery(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::editing::MULTILEVEL_LIBRARY;
    ui.label(RichText::new(crate::i18n::tr("List Library")).strong());
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(340.0);
        for &(kind, sample) in MULTILEVEL_LIBRARY {
            let (rect, r) = ui.allocate_exact_size(vec2(78.0, 78.0), Sense::click());
            ui.painter().rect(rect, crate::theme::RADIUS as f32, if r.hovered() { c_hover() } else { c_card() }, Stroke::new(1.0, c_line()), StrokeKind::Inside);
            for (k, t) in sample.split(' ').enumerate() {
                let y = rect.min.y + 16.0 + k as f32 * 22.0;
                let x = rect.min.x + 6.0 + k as f32 * 10.0;
                ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, t, FontId::proportional(10.5), c_ink());
                ui.painter().line_segment([pos2(x + 26.0, y), pos2(rect.max.x - 5.0, y)], Stroke::new(1.0, c_line()));
            }
            if r.on_hover_text(crate::i18n::tr(&sample)).clicked() { app.doc.apply_multilevel(kind); ui.close(); }
        }
    });
    let mine: Vec<word_engine::editing::CustomList> = app.custom_lists.iter().filter(|l| l.levels.len() > 1 && l.levels[1..].iter().any(|v| v.text.contains("%1"))).cloned().collect();
    if !mine.is_empty() {
        ui.separator();
        ui.label(RichText::new(crate::i18n::tr("My Lists")).strong());
        for l in mine { if ui.button(format!("{}   {}", l.preview().iter().take(3).cloned().collect::<Vec<_>>().join(" "), l.name)).clicked() { app.doc.apply_custom_list(&l); ui.close(); } }
    }
    ui.separator();
    if ui.button(crate::i18n::tr("Define New Multilevel List…")).clicked() { app.open_list_dialog(crate::app::ListDialogKind::Multilevel); ui.close(); }
}

pub fn font_picker(app: &mut WordApp, ui: &mut Ui, current: &str) {
    crate::theme::Combo::from_id_salt("rb-font").width(132.0).height(400.0).selected_text(RichText::new(current).size(12.5)).show_ui(ui, |ui| {
        for display in word_engine::fonts::family_names() {
            let display = display.as_str();
            let key = word_engine::fonts::resolve(display, false, false);
            // Bundled faces preview in their own font; installed ones in the interface font.
            let family = if (key.face as usize) < word_engine::fonts::BUNDLED.len() { FontFamily::Name(crate::app::face_name(key.face as usize).into()) } else { FontFamily::Proportional };
            let text = RichText::new(display).font(FontId::new(14.0, family));
            if ui.selectable_label(current.eq_ignore_ascii_case(display), text).clicked() {
                app.doc.set_run_props(RunProps { font: Some(display.to_string()), ..Default::default() });
            }
        }
    });
}

pub fn size_picker(app: &mut WordApp, ui: &mut Ui, size: f32) {
    let id = ui.make_persistent_id("rb-size-text");
    let mut text = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| format!("{size}"));
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(44.0).font(FontId::proportional(12.5)));
    if r.has_focus() { app.canvas_focus = false; ui.data_mut(|d| d.insert_temp(id, text.clone())); }
    else { ui.data_mut(|d| d.remove::<String>(id)); }
    if r.lost_focus() {
        if let Ok(v) = text.trim().parse::<f32>() { if v >= 1.0 && v <= 1638.0 && (v - size).abs() > 0.01 { app.doc.set_font_size(v); } }
        ui.data_mut(|d| d.remove::<String>(id));
        app.canvas_focus = true;
    }
    let (arect, ar) = ui.allocate_exact_size(vec2(14.0, 22.0), Sense::click());
    button_bg(ui, arect, &ar, false);
    caret(ui, arect.center());
    crate::theme::menu(&ar, |ui| {
        for s in [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0] {
            if ui.selectable_label((size - s).abs() < 0.01, format!("{s}")).clicked() { app.doc.set_font_size(s); ui.close(); }
        }
    });
}

/// Paragraph styles as live previews (styleGallery).
fn style_gallery(app: &mut WordApp, ui: &mut Ui) {
    let current = app.doc.paragraph_style();
    let mut styles: Vec<(String, String)> = app.doc.styles.paragraph_styles().iter().map(|s| (s.id.clone(), s.name.clone())).collect();
    // Common styles first, as Word's gallery shows them.
    let rank = |id: &str| ["Normal", "NoSpacing", "Heading1", "Heading2", "Heading3", "Title", "Subtitle", "Quote"].iter().position(|x| *x == id).unwrap_or(99);
    styles.sort_by_key(|(id, _)| rank(id));
    styles.truncate(6);
    for (id, name) in styles {
        let props = app.doc.styles.run_props(Some(&id), None);
        let mut rf = word_engine::RunFormat::default();
        rf.apply(&props);
        let key = word_engine::fonts::resolve(&rf.font, rf.bold, rf.italic);
        let color = rf.color.map(|c| Color32::from_rgb(c.0, c.1, c.2)).unwrap_or(c_ink());
        let (rect, r) = ui.allocate_exact_size(vec2(74.0, 64.0), Sense::click());
        let on = id == current;
        ui.painter().rect(rect.shrink(1.0), crate::theme::RADIUS as f32, if r.hovered() { c_hover() } else { c_card() }, Stroke::new(if on { 1.5 } else { 1.0 }, if on { ACCENT } else { c_line() }), StrokeKind::Inside);
        let preview = FontId::new((rf.size * 1.0).clamp(10.0, 17.0), FontFamily::Name(crate::app::face_name(key.face as usize).into()));
        ui.painter().with_clip_rect(rect.shrink(3.0)).text(pos2(rect.min.x + 7.0, rect.min.y + 24.0), Align2::LEFT_CENTER, "AaBbCc", preview, color);
        ui.painter().text(pos2(rect.min.x + 7.0, rect.max.y - 12.0), Align2::LEFT_CENTER, &name, FontId::proportional(10.5), c_ink3());
        if r.on_hover_text(crate::i18n::trf("Apply the {} style", &[&name])).clicked() { app.doc.set_paragraph_style(&id); }
    }
}

pub fn line_spacing_menu(app: &mut WordApp, ui: &mut Ui) {
    let cur = app.doc.current_para_format();
    for v in [1.0, 1.15, 1.5, 2.0, 2.5, 3.0] {
        let on = cur.line_rule == LineRule::Auto && (cur.line_spacing - v).abs() < 0.01;
        if ui.selectable_label(on, format!("{v}")).clicked() { app.doc.set_line_spacing(LineRule::Auto, v); ui.close(); }
    }
    ui.separator();
    if ui.button(if cur.space_before > 0.0 { "Remove Space Before Paragraph" } else { "Add Space Before Paragraph" }).clicked() { app.doc.set_paragraph_spacing(Some(if cur.space_before > 0.0 { 0.0 } else { 12.0 }), None); ui.close(); }
    if ui.button(if cur.space_after > 0.0 { "Remove Space After Paragraph" } else { "Add Space After Paragraph" }).clicked() { app.doc.set_paragraph_spacing(None, Some(if cur.space_after > 0.0 { 0.0 } else { 9.0 })); ui.close(); }
    ui.separator();
    if ui.button(crate::i18n::tr("Line Spacing Options…")).clicked() { crate::ui::open_paragraph_dialog(app); ui.close(); }
}

fn borders_menu(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::{Border, ParaBorders};
    let b = Some(Border { width: 0.5, space: 1.0, color: None });
    for (icon, label, borders) in [
        ("borderBottom", "Bottom Border", ParaBorders { bottom: b, ..Default::default() }),
        ("borderTop", "Top Border", ParaBorders { top: b, ..Default::default() }),
        ("borderOuter", "Box Border", ParaBorders { top: b, bottom: b, left: b, right: b }),
        ("borderNone", "No Border", ParaBorders::default()),
    ] {
        if small(ui, icon, Some(label), false, true, "").clicked() { app.doc.set_para_props(word_engine::ParaProps { borders: Some(borders), ..Default::default() }); ui.close(); }
    }
}

fn cover_gallery(app: &mut WordApp, ui: &mut Ui) {
    ui.label(RichText::new(crate::i18n::tr("Built-in")).strong());
    ui.horizontal(|ui| {
        for (design, name) in word_engine::editing::COVER_DESIGNS {
            let (rect, r) = ui.allocate_exact_size(vec2(70.0, 92.0), Sense::click());
            crate::insert::cover_thumbnail(ui, rect, *design, word_engine::objects::PALETTE[0], false);
            ui.painter().text(rect.center_bottom() + vec2(0.0, -8.0), Align2::CENTER_CENTER, *name, FontId::proportional(10.0), c_ink3());
            if r.clicked() { app.open_cover_page(*design); ui.close(); }
        }
    });
}

/// Header ▾ / Footer ▾ galleries.
fn story_gallery(app: &mut WordApp, ui: &mut Ui, header: bool) {
    use word_engine::design::{StoryDesign, HEADER_DESIGNS};
    ui.label(RichText::new(crate::i18n::tr("Built-in")).strong());
    for (design, name) in HEADER_DESIGNS {
        let (rect, r) = ui.allocate_exact_size(vec2(260.0, 40.0), Sense::click());
        ui.painter().rect(rect.shrink(1.0), crate::theme::RADIUS as f32, if r.hovered() { c_hover() } else { c_card() }, Stroke::new(1.0, c_line()), StrokeKind::Inside);
        let accent = Color32::from_rgb(0x44, 0x72, 0xC4);
        let band = Rect::from_min_size(rect.min + vec2(8.0, if header { 6.0 } else { 18.0 }), vec2(244.0, 14.0));
        match design {
            StoryDesign::Banded => { ui.painter().rect_filled(band, crate::theme::RADIUS as f32, accent); ui.painter().text(band.center(), Align2::CENTER_CENTER, if header { "TITLE" } else { "Page 1 of 3" }, FontId::proportional(8.0), Color32::WHITE); }
            StoryDesign::Line => { ui.painter().text(band.center(), Align2::CENTER_CENTER, if header { "DOCUMENT TITLE" } else { "1" }, FontId::proportional(8.0), accent); let y = if header { band.max.y } else { band.min.y }; ui.painter().line_segment([pos2(band.min.x, y), pos2(band.max.x, y)], Stroke::new(1.0, accent)); }
            StoryDesign::ThreeColumns => for (k, al) in [Align2::LEFT_CENTER, Align2::CENTER_CENTER, Align2::RIGHT_CENTER].into_iter().enumerate() { let x = [band.min.x, band.center().x, band.max.x][k]; ui.painter().text(pos2(x, band.center().y), al, "[Type here]", FontId::proportional(8.0), c_ink3()); },
            StoryDesign::Title => { ui.painter().text(band.left_center(), Align2::LEFT_CENTER, if header { "Title" } else { "Author" }, FontId::proportional(8.0), c_ink2()); ui.painter().text(band.right_center(), Align2::RIGHT_CENTER, if header { "Date" } else { "1" }, FontId::proportional(8.0), accent); }
            StoryDesign::Blank => { ui.painter().text(band.left_center(), Align2::LEFT_CENTER, "[Type here]", FontId::proportional(8.0), c_ink3()); }
        }
        ui.painter().text(rect.right_bottom() - vec2(6.0, 4.0), Align2::RIGHT_BOTTOM, *name, FontId::proportional(9.0), c_ink3());
        if r.clicked() {
            if app.extras.story.is_some() { app.end_story(); }
            let paras = app.doc.story_design(*design, header);
            app.doc.set_story(header, Some(paras));
            app.edit_story(header, None);
            ui.close();
        }
    }
    ui.separator();
    let what = if header { "Header" } else { "Footer" };
    if ui.button(crate::i18n::trf("Edit {}", &[&what])).clicked() { app.edit_story(header, None); ui.close(); }
    if ui.add_enabled(app.doc.story(header).is_some() || app.extras.story.as_ref().is_some_and(|s| s.header == header), egui::Button::new(format!("Remove {what}"))).clicked() {
        if app.extras.story.is_some() { app.end_story(); }
        app.doc.set_story(header, None);
        ui.close();
    }
}

/// Page Number ▾.
fn page_number_menu(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::design::PAGE_NUMBER_STYLES;
    for (top, name) in [(true, "Top of Page"), (false, "Bottom of Page")] {
        crate::theme::submenu(ui, crate::i18n::tr(&name), |ui| {
            for (align, an) in [(Alignment::Left, "Left"), (Alignment::Center, "Center"), (Alignment::Right, "Right")] {
                ui.label(RichText::new(crate::i18n::tr(&an)).small().strong());
                for (k, (sn, ..)) in PAGE_NUMBER_STYLES.iter().enumerate() {
                    if ui.button(format!("  {sn}")).clicked() {
                        if app.extras.story.is_some() { app.end_story(); }
                        app.doc.insert_page_numbers(top, k, align);
                        ui.close();
                    }
                }
            }
        });
    }
    crate::theme::submenu(ui, crate::i18n::tr("Current Position"), |ui| {
        if ui.button(crate::i18n::tr("Plain Number")).clicked() { app.doc.insert_field(word_engine::Field::Page); ui.close(); }
        if ui.button(crate::i18n::tr("Page X of Y")).clicked() { app.doc.replace("Page "); app.doc.insert_field(word_engine::Field::Page); app.doc.replace(" of "); app.doc.insert_field(word_engine::Field::NumPages); ui.close(); }
    });
    ui.separator();
    if ui.button(crate::i18n::tr("Remove Page Numbers")).clicked() { if app.extras.story.is_some() { app.end_story(); } app.doc.remove_page_numbers(); ui.close(); }
}

fn insert(app: &mut WordApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    group(ui, "Pages", |ui| {
        rows(ui, |ui| {
            let r = small_menu(ui, "coverPage", "Cover Page", "Insert a designed cover page");
            crate::theme::menu(&r, |ui| cover_gallery(app, ui));
            if small(ui, "blankPage", Some("Blank Page"), false, true, "Insert a blank page at the caret").clicked() { app.doc.insert_blank_page(); app.scroll_to_caret = true; }
            if small(ui, "pageBreak", Some("Page Break"), false, true, "Start the next page (⌘Enter)").clicked() { app.doc.insert_page_break(); }
        });
    });
    group(ui, "Tables", |ui| {
        let r = large_menu(ui, "table", "Table", "Insert a table");
        crate::theme::menu(&r, |ui| table_grid_picker(app, ui));
    });
    group(ui, "Illustrations", |ui| {
        let r = large_menu(ui, "pictures", "Pictures", "Insert pictures");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "pictures", "This Device…") { app.insert_picture(); ui.close(); }
            if menu_item(ui, "changePicture", "Online Pictures…") { app.open_online_pictures(); ui.close(); }
            if menu_item(ui, "icons", "Stock Icons…") { app.open_icons(&ctx); ui.close(); }
        });
        let shapes = large_menu(ui, "shapes", "Shapes", "Insert a shape");
        crate::theme::menu(&shapes, |ui| {
            shape_gallery(app, ui);
            ui.separator();
            if menu_item(ui, "drawingCanvas", "New Drawing Canvas") { app.insert_drawing_canvas(); ui.close(); }
        });
        rows(ui, |ui| {
            let r = small_menu(ui, "screenshot", "Screenshot", "Insert a picture of a window or screen");
            if r.clicked() { app.open_screenshot(&ctx); }
            if small(ui, "icons", Some("Icons"), false, true, "Insert an icon").clicked() { app.open_icons(&ctx); }
            let r = small_menu(ui, "smartArt", "Diagram", "SmartArt diagrams and charts");
            crate::theme::menu(&r, |ui| {
                ui.label(RichText::new(crate::i18n::tr("SmartArt")).strong());
                for (k, _, n) in word_engine::objects::DIAGRAMS { if ui.button(crate::i18n::tr(&*n)).clicked() { app.open_diagram(*k); ui.close(); } }
                ui.separator();
                crate::theme::submenu(ui, crate::i18n::tr("Chart"), |ui| { for (k, n) in word_engine::objects::CHARTS { if ui.button(crate::i18n::tr(&*n)).clicked() { app.open_chart(*k); ui.close(); } } });
            });
        });
    });
    group(ui, "Links", |ui| {
        let r = large(ui, "link", "Link", false, true, "Insert a link (⌘K); right-click for more");
        if r.clicked() { app.dialogs.link = Some(app.doc.link_at(app.doc.range().0).unwrap_or_default()); }
        r.context_menu(|ui| {
            if ui.button(crate::i18n::tr("Cross-reference…")).clicked() { app.insert.xref = Some(crate::insert::XrefDraft { headings: true, page: false, selected: None }); ui.close(); }
            if ui.button(crate::i18n::tr("Online Video…")).clicked() { app.insert.video = Some(crate::insert::VideoDialog { url: String::new(), job: None, info: None, status: String::new() }); ui.close(); }
        });
        if large(ui, "bookmark", "Bookmark", false, true, "Name a place in the document").clicked() { app.insert.bookmark = Some(crate::insert::BookmarkDraft { name: String::new(), hidden: false }); }
    });
    group(ui, "Comments", |ui| {
        if large(ui, "comment", "Comment", false, true, "New comment (⌥⌘M)").clicked() { app.new_comment(); }
    });
    group(ui, "Header & Footer", |ui| {
        let r = large_menu(ui, "header", "Header", "Edit the page header");
        crate::theme::menu(&r, |ui| story_gallery(app, ui, true));
        let r = large_menu(ui, "footer", "Footer", "Edit the page footer");
        crate::theme::menu(&r, |ui| story_gallery(app, ui, false));
        let r = large_menu(ui, "pageNumber", "Page Number", "Number the pages");
        crate::theme::menu(&r, |ui| page_number_menu(app, ui));
    });
    group(ui, "Text", |ui| {
        let r = large_menu(ui, "textBox", "Text Box", "Insert a text box");
        crate::theme::menu(&r, |ui| crate::text_tools::text_box_menu(app, ui));
        rows(ui, |ui| {
            let r = small_menu(ui, "quickParts", "Quick Parts", "AutoText, document properties and fields");
            crate::theme::menu(&r, |ui| crate::text_tools::quick_parts_menu(app, ui));
            let r = small_menu(ui, "wordArt", "WordArt", "Decorative text");
            crate::theme::menu(&r, |ui| crate::text_tools::word_art_menu(app, ui));
            let r = small_menu(ui, "dropCap", "Drop Cap", "A large initial letter");
            crate::theme::menu(&r, |ui| crate::text_tools::drop_cap_menu(app, ui));
        });
        rows(ui, |ui| {
            if small(ui, "signatureLine", Some("Signature Line"), false, true, "Insert a signature line").clicked() { app.open_signature(); }
            if small(ui, "dateTime", Some("Date & Time"), false, true, "Insert the date or time").clicked() { app.open_date_time(); }
            let r = small_menu(ui, "object", "Object", "Text from a file, video, add-ins");
            crate::theme::menu(&r, |ui| {
                if menu_item(ui, "filePlus", "Text from File…") { app.insert_text_from_file(); ui.close(); }
                if menu_item(ui, "webLayout", "Online Video…") { app.insert.video = Some(crate::insert::VideoDialog { url: String::new(), job: None, info: None, status: String::new() }); ui.close(); }
                if menu_item(ui, "footnote", "Footnote") { app.insert_note(NoteKind::Foot); ui.close(); }
                if menu_item(ui, "smartLookup", "Wikipedia…") { app.open_wikipedia(); ui.close(); }
                if menu_item(ui, "thesaurus", "Dictionary…") { app.open_dictionary(); ui.close(); }
                if menu_item(ui, "object", "Add-ins…") { app.insert.addins = true; ui.close(); }
            });
        });
    });
    group(ui, "Symbols", |ui| {
        let r = large_menu(ui, "equation", "Equation", "Insert an equation");
        crate::theme::menu(&r, |ui| crate::symbols::equation_menu(app, ui));
        let r = small_menu(ui, "symbol", "Symbol", "Insert a symbol");
        crate::theme::menu(&r, |ui| crate::symbols::symbol_menu(app, ui));
    });
}

/// The Shapes gallery: each preset drawn by the engine's own geometry.
fn shape_gallery(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::objects::*;
    ui.label(RichText::new(crate::i18n::tr("Shapes")).strong());
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(260.0);
        for (kind, _, name) in SHAPES {
            let (rect, r) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
            if r.hovered() { ui.painter().rect_filled(rect, crate::theme::RADIUS as f32, c_hover()); }
            let inner = rect.shrink(6.0);
            let stroke = Stroke::new(1.2, c_ink2());
            if kind.is_line() {
                ui.painter().line_segment([inner.left_bottom(), inner.right_top()], stroke);
                if *kind == ShapeKind::Arrow { ui.painter().line_segment([inner.right_top(), inner.right_top() + vec2(-6.0, 1.0)], stroke); ui.painter().line_segment([inner.right_top(), inner.right_top() + vec2(-1.0, 6.0)], stroke); }
            } else {
                let pts: Vec<egui::Pos2> = geometry(*kind, inner.width(), inner.height()).into_iter().map(|(x, y)| inner.min + vec2(x, y)).collect();
                ui.painter().add(egui::Shape::closed_line(pts, stroke));
            }
            if r.on_hover_text(crate::i18n::tr(&*name)).clicked() { app.insert_shape(*kind); ui.close(); }
        }
    });
}

fn shape_format(app: &mut WordApp, ui: &mut Ui) {
    let Some((pos, Object::Shape(mut shape))) = app.selected_object() else { return };
    let mut changed = false;
    group(ui, "Shape Styles", |ui| {
        for (i, c) in word_engine::objects::PALETTE.iter().enumerate() {
            let (rect, r) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
            ui.painter().rect(rect.shrink(4.0), crate::theme::RADIUS as f32, Color32::from_rgb(c.0, c.1, c.2), Stroke::new(1.0, Color32::from_rgb(c.0 / 2, c.1 / 2, c.2 / 2)), StrokeKind::Inside);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Abc", FontId::proportional(10.0), Color32::WHITE);
            if r.on_hover_text(crate::i18n::trf("Style {}", &[&(i + 1)])).clicked() { shape.fill = Some(*c); shape.outline = Some(Color(c.0 / 2, c.1 / 2, c.2 / 2)); shape.text_color = Color::WHITE; changed = true; }
        }
    });
    group(ui, "Format", |ui| { rows(ui, |ui| { changed |= crate::insert::shape_controls(ui, &mut shape, 3); }); });
    group(ui, "Text", |ui| {
        if large(ui, "addText", "Edit Text", false, true, "Type text in the shape").clicked() { app.edit_object(pos); }
    });
    group(ui, "Rotation", |ui| {
        rows(ui, |ui| {
            changed |= ui.add(egui::DragValue::new(&mut shape.rotation).range(0.0..=359.9).speed(1.0).suffix("°")).changed();
            ui.horizontal(|ui| {
                if small(ui, "flipHorizontal", None, shape.flip_h, true, "Flip horizontal").clicked() { shape.flip_h = !shape.flip_h; changed = true; }
                if small(ui, "flipVertical", None, shape.flip_v, true, "Flip vertical").clicked() { shape.flip_v = !shape.flip_v; changed = true; }
            });
        });
    });
    if changed { shape.raw = None; app.doc.set_object(pos, Object::Shape(shape)); app.doc.select(pos, pos + 1); }
    arrange_and_size(app, ui);
}



/// Arrange (position, rotate/flip, save, delete) and Size groups shared by pictures and shapes.
fn arrange_and_size(app: &mut WordApp, ui: &mut Ui) {
    use crate::picture::Turn;
    group(ui, "Arrange", |ui| {
        let pos = large(ui, "position", "Position", false, true, "Place the object on the page");
        crate::theme::menu(&pos, |ui| crate::picture::position_menu(app, ui));
        let wrap = large(ui, "wrapText", "Wrap Text", false, true, "How text flows around the object");
        crate::theme::menu(&wrap, |ui| crate::picture::wrap_menu(app, ui));
        rows(ui, |ui| {
            if small(ui, "bringToFront", Some("Bring to Front"), false, true, "In front of text").clicked() { app.set_wrap(Some(word_engine::model::Wrap::InFront)); }
            if small(ui, "sendToBack", Some("Send Behind Text"), false, true, "Behind text").clicked() { app.set_wrap(Some(word_engine::model::Wrap::Behind)); }
            let al = small(ui, "objAlignCenter", Some("Align"), false, true, "Align the object");
            crate::theme::menu(&al, |ui| {
                for (icon, name, a) in [("objAlignLeft", "Align Left", Alignment::Left), ("objAlignCenter", "Align Center", Alignment::Center), ("objAlignRight", "Align Right", Alignment::Right)] {
                    if menu_item(ui, icon, name) {
                        match app.selected_box().filter(|p| app.doc.anchor_at(*p).is_some()) {
                            Some(_) => { let h = match a { Alignment::Left => word_engine::model::AnchorAlign::Start, Alignment::Right => word_engine::model::AnchorAlign::End, _ => word_engine::model::AnchorAlign::Center };
                                if let Some(p) = app.selected_box() { if let Some(mut an) = app.doc.anchor_at(p) { an.x_align = Some(h); an.x_from = word_engine::model::RelFrom::Margin; app.doc.set_anchor(p, Some(an)); } } }
                            None => app.doc.set_alignment(a),
                        }
                        ui.close();
                    }
                }
            });
        });
        let rot = large(ui, "rotate", "Rotate", false, true, "Rotate or flip");
        crate::theme::menu(&rot, |ui| {
            for (icon, name, t) in [("rotateRight", "Rotate Right 90°", Turn::Right), ("rotateLeft", "Rotate Left 90°", Turn::Left), ("flipVertical", "Flip Vertical", Turn::FlipV), ("flipHorizontal", "Flip Horizontal", Turn::FlipH)] {
                if menu_item(ui, icon, name) { app.turn_selection(t); ui.close(); }
            }
        });
        rows(ui, |ui| {
            if small(ui, "download", Some("Save as Picture"), false, true, "Save as a picture file").clicked() { app.save_as_picture(); }
            if small(ui, "deletePicture", Some("Delete"), false, true, "Delete").clicked() { app.doc.delete(true, false); }
        });
    });
    if let Some((w, h)) = app.selection_size() {
        group(ui, "Size", |ui| {
            rows(ui, |ui| {
                let id = egui::Id::unique("lock-aspect");
                let mut lock = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
                let (mut nw, mut nh) = (w / 96.0, h / 96.0);
                let mut changed = None;
                ui.horizontal(|ui| { let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover()); paint_icon(ui, "shapeHeight", r, 16, true); if ui.add(egui::DragValue::new(&mut nh).range(0.05..=22.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { changed = Some(if lock && h > 0.0 { (nh * w / h, nh) } else { (nw, nh) }); } });
                ui.horizontal(|ui| { let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover()); paint_icon(ui, "shapeWidth", r, 16, true); if ui.add(egui::DragValue::new(&mut nw).range(0.05..=22.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { changed = Some(if lock && w > 0.0 { (nw, nw * h / w) } else { (nw, nh) }); } });
                if ui.checkbox(&mut lock, crate::i18n::tr("Lock aspect ratio")).changed() { ui.data_mut(|d| d.insert_temp(id, lock)); }
                if let Some((a, b)) = changed { app.set_selection_size(a * 96.0, b * 96.0); }
            });
        });
    }
}

fn picture_format(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::picture::PictureOp;
    if app.selected_picture().is_none() { return; }
    let ctx = ui.ctx().clone();
    group(ui, "Adjust", |ui| {
        if large(ui, "removeBackground", "Remove Background", false, true, "Make the picture's background color transparent").clicked() { app.picture_op(PictureOp::RemoveBackground); }
        let c = large(ui, "corrections", "Corrections", false, true, "Brightness, contrast and sharpness");
        crate::theme::menu(&c, |ui| {
            ui.label(RichText::new(crate::i18n::tr("Brightness / Contrast")).strong());
            crate::theme::grid(ui, egui::Grid::new("corrections").spacing(vec2(4.0, 4.0)), |ui| {
                for b in [-40, -20, 0, 20, 40] {
                    for k in [-40, -20, 0, 20, 40] {
                        let r = ui.add_sized(vec2(44.0, 30.0), egui::Button::new(RichText::new(format!("{b:+}/{k:+}")).size(9.0)).fill(Color32::from_gray((200.0 + b as f32 * 1.2).clamp(60.0, 255.0) as u8)));
                        if r.on_hover_text(format!("Brightness {b:+}%  Contrast {k:+}%")).clicked() && (b, k) != (0, 0) { app.picture_op(PictureOp::Corrections { brightness: b, contrast: k }); ui.close(); }
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            if ui.button(crate::i18n::tr("Sharpen")).clicked() { app.picture_op(PictureOp::Sharpen); ui.close(); }
            if ui.button(crate::i18n::tr("Soften")).clicked() { app.picture_op(PictureOp::Blur(1.2)); ui.close(); }
        });
        let col = large(ui, "pictureColor", "Color", false, true, "Recolor the picture");
        crate::theme::menu(&col, |ui| {
            for (name, op) in [("Grayscale", PictureOp::Grayscale), ("Sepia", PictureOp::Sepia), ("Washout", PictureOp::Washout)] {
                if ui.button(crate::i18n::tr(&name)).clicked() { app.picture_op(op); ui.close(); }
            }
            ui.label(crate::i18n::tr("Recolor"));
            if let Some(Some(c)) = color_menu(ui, "") { app.picture_op(PictureOp::Tint(c)); }
        });
        let art = large(ui, "artisticEffects", "Artistic Effects", false, true, "Blur, sharpen and more");
        crate::theme::menu(&art, |ui| {
            for (name, op) in [("Blur", PictureOp::Blur(3.0)), ("Strong blur", PictureOp::Blur(6.0)), ("Sharpen", PictureOp::Sharpen), ("Pencil sketch", PictureOp::Sketch), ("Invert", PictureOp::Invert), ("Posterize", PictureOp::Posterize)] {
                if ui.button(crate::i18n::tr(&name)).clicked() { app.picture_op(op); ui.close(); }
            }
        });
        let tr = large(ui, "transparency", "Transparency", false, true, "Make the picture see-through");
        crate::theme::menu(&tr, |ui| {
            for t in [15u8, 30, 50, 65, 80] { if ui.button(format!("{t}%")).clicked() { app.picture_op(PictureOp::Transparency(t)); ui.close(); } }
        });
        rows(ui, |ui| {
            if small(ui, "changePicture", Some("Change Picture"), false, true, "Replace with another picture, keeping the size").clicked() { app.change_picture(); }
            if small(ui, "resetPicture", Some("Reset Picture"), false, true, "Discard all formatting changes").clicked() { if let Some((pos, _)) = app.selected_picture() { app.doc.reset_picture(pos); } }
            if small(ui, "compressPictures", Some("Compress"), false, true, "Downsample to 150 ppi at the shown size").clicked() { app.picture_op(PictureOp::Compress); }
        });
    });
    group(ui, "Picture Styles", |ui| {
        let r = large(ui, "styleGallery", "Quick Styles", false, true, "Frame or shape the picture");
        crate::theme::menu(&r, |ui| for (name, op) in [("Simple Frame", PictureOp::Frame(word_engine::Color(255, 255, 255))), ("Black Frame", PictureOp::Frame(word_engine::Color(0, 0, 0))), ("Rounded", PictureOp::Rounded), ("Oval", PictureOp::Oval), ("Soft Edges", PictureOp::SoftEdges)] {
            if menu_item(ui, "pictureEffects", name) { app.picture_op(op); ui.close(); }
        });
    });
    group(ui, "Accessibility", |ui| {
        if large(ui, "altText", "Alt Text", false, true, "Describe the picture for screen readers").clicked() { app.open_alt_text(); }
    });
    group(ui, "Crop", |ui| {
        if large(ui, "crop", "Crop", false, true, "Trim the picture's edges").clicked() { app.open_crop(&ctx); }
    });
    arrange_and_size(app, ui);
}

fn chart_design(app: &mut WordApp, ui: &mut Ui) {
    let Some((pos, Object::Chart(mut chart))) = app.selected_object() else { return };
    let mut changed = false;
    group(ui, "Data", |ui| {
        if large(ui, "chart", "Edit Data", false, true, "Edit the chart's data").clicked() { app.edit_object(pos); }
    });
    group(ui, "Type", |ui| {
        for (k, n) in word_engine::objects::CHARTS {
            if small(ui, "chart", Some(n), chart.kind == *k, true, "Change chart type").clicked() && chart.kind != *k { chart.kind = *k; changed = true; }
        }
    });
    group(ui, "Chart Layouts", |ui| {
        rows(ui, |ui| {
            changed |= ui.checkbox(&mut chart.legend, crate::i18n::tr("Legend")).changed();
            ui.horizontal(|ui| { ui.label(crate::i18n::tr("Title")); changed |= ui.add(egui::TextEdit::singleline(&mut chart.title).desired_width(140.0)).changed(); });
        });
    });
    if changed { chart.raw = None; app.doc.set_object(pos, Object::Chart(chart)); app.doc.select(pos, pos + 1); }
}

fn smartart_design(app: &mut WordApp, ui: &mut Ui) {
    let Some((pos, Object::Diagram(mut d))) = app.selected_object() else { return };
    let mut changed = false;
    group(ui, "Create Graphic", |ui| {
        if large(ui, "smartArt", "Text Pane", false, true, "Edit the diagram's text").clicked() { app.edit_object(pos); }
    });
    group(ui, "Layouts", |ui| {
        for (k, _, n) in word_engine::objects::DIAGRAMS { if small(ui, "smartArt", Some(n), d.kind == *k, true, "Change layout").clicked() && d.kind != *k { d.kind = *k; changed = true; } }
    });
    group(ui, "Colors", |ui| {
        rows(ui, |ui| { changed |= swatch_choice(ui, &word_engine::objects::PALETTE, &mut d.color); });
    });
    if changed { app.doc.set_object(pos, Object::Diagram(d)); app.doc.select(pos, pos + 1); }
}

/// Hover a grid to choose a table size (Insert → Table).
fn table_grid_picker(app: &mut WordApp, ui: &mut Ui) {
    let id = ui.make_persistent_id("grid-pick");
    let (mut rows, mut cols): (usize, usize) = ui.data(|d| d.get_temp(id)).unwrap_or((0, 0));
    ui.label(if rows > 0 { crate::i18n::trf("{} × {} Table", &[&cols, &rows]) } else { "Insert Table".into() });
    let cell = 16.0;
    let (rect, r) = ui.allocate_exact_size(vec2(10.0 * (cell + 2.0), 8.0 * (cell + 2.0)), Sense::click());
    if let Some(p) = r.hover_pos() {
        cols = (((p.x - rect.min.x) / (cell + 2.0)).floor() as usize + 1).min(10);
        rows = (((p.y - rect.min.y) / (cell + 2.0)).floor() as usize + 1).min(8);
        ui.data_mut(|d| d.insert_temp(id, (rows, cols)));
    }
    for rr in 0..8 {
        for cc in 0..10 {
            let c = Rect::from_min_size(rect.min + vec2(cc as f32 * (cell + 2.0), rr as f32 * (cell + 2.0)), vec2(cell, cell));
            let on = rr < rows && cc < cols;
            ui.painter().rect(c, crate::theme::RADIUS as f32, if on { Color32::from_rgb(0xDE, 0xEB, 0xFA) } else { c_card() }, Stroke::new(1.0, if on { ACCENT } else { c_line() }), StrokeKind::Inside);
        }
    }
    if r.clicked() && rows > 0 { app.doc.insert_table(rows, cols); app.scroll_to_caret = true; app.canvas_focus = true; ui.data_mut(|d| d.remove::<(usize, usize)>(id)); ui.close(); }
    ui.separator();
    if ui.button(crate::i18n::tr("Insert Table…")).clicked() { app.dialogs.table = Some((3, 3)); ui.close(); }
    crate::theme::submenu(ui, crate::i18n::tr("Convert Text to Table"), |ui| {
        use word_engine::tables::Separator;
        for (sep, label) in [(Separator::Tab, "Split at tabs"), (Separator::Comma, "Split at commas"), (Separator::Paragraph, "One cell per paragraph")] {
            if ui.button(crate::i18n::tr(&label)).clicked() { if !app.doc.convert_text_to_table(sep) { app.notify(ui.ctx(), "Select paragraphs outside a table"); } ui.close(); }
        }
    });
}

fn layout(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::page::{Columns, LineNumbering, LineRestart, SectionKind};
    let launch = group_l(ui, "Page Setup", "Page Setup", |ui| {
        let m = large_menu(ui, "margins", "Margins", "Page margins");
        crate::theme::menu(&m, |ui| {
            for (icon, name, t, b, l, r) in [("marginsNormal", "Normal", 1440, 1440, 1440, 1440), ("marginsNarrow", "Narrow", 720, 720, 720, 720), ("marginsModerate", "Moderate", 1440, 1440, 1080, 1080), ("marginsWide", "Wide", 1440, 1440, 2880, 2880)] {
                if small(ui, icon, Some(name), false, true, "").clicked() {
                    let mut p = app.doc.page.clone();
                    (p.top, p.bottom, p.left, p.right) = (t, b, l, r);
                    if let Err(e) = app.doc.set_page_settings(p) { app.dialogs.error = Some(e); }
                    ui.close();
                }
            }
            if small(ui, "marginsCustom", Some("Custom Margins…"), false, true, "").clicked() { app.dialogs.page_setup = Some(app.doc.page.clone()); ui.close(); }
        });
        let o = large_menu(ui, "orientation", "Orientation", "Page orientation");
        crate::theme::menu(&o, |ui| {
            for (icon, name, land) in [("portrait", "Portrait", false), ("landscape", "Landscape", true)] {
                if small(ui, icon, Some(name), app.doc.page.landscape == land, true, "").clicked() {
                    let mut p = app.doc.page.clone();
                    p.set_landscape(land);
                    let _ = app.doc.set_page_settings(p);
                    ui.close();
                }
            }
        });
        let s = large_menu(ui, "size", "Size", "Paper size");
        crate::theme::menu(&s, |ui| {
            for (name, w, h) in word_engine::page::PAPER_SIZES {
                if small(ui, "paperSize", Some(name), false, true, "").clicked() {
                    let mut p = app.doc.page.clone();
                    (p.width, p.height) = if p.landscape { (*h, *w) } else { (*w, *h) };
                    if let Err(e) = app.doc.set_page_settings(p) { app.dialogs.error = Some(e); }
                    ui.close();
                }
            }
            if small(ui, "pageSetup", Some("More Paper Sizes…"), false, true, "").clicked() { app.dialogs.page_setup = Some(app.doc.page.clone()); ui.close(); }
        });
        let c = large_menu(ui, "columns", "Columns", "Split text into columns");
        crate::theme::menu(&c, |ui| {
            let cur = app.doc.section_at(app.doc.current_section()).columns.clone();
            for (icon, name, n) in [("columnsOne", "One", 1u8), ("columnsTwo", "Two", 2), ("columnsThree", "Three", 3)] {
                if small(ui, icon, Some(name), cur.count == n && cur.widths.is_empty(), true, "").clicked() { app.doc.set_columns(Columns { count: n, separator: cur.separator, space: cur.space, widths: vec![] }); ui.close(); }
            }
            for (icon, name, left) in [("columnsLeft", "Left", true), ("columnsRight", "Right", false)] {
                if menu_item(ui, icon, name) { let c = crate::refs::sized_preset(app, left); app.doc.set_columns(c); ui.close(); }
            }
            ui.separator();
            if menu_item(ui, "columnsDialog", "More Columns…") { app.open_columns_dialog(); ui.close(); }
        });
        rows(ui, |ui| {
            let b = small_menu(ui, "breaks", "Breaks", "Page, column and section breaks");
            crate::theme::menu(&b, |ui| {
                ui.label(RichText::new(crate::i18n::tr("Page Breaks")).strong());
                if menu_item(ui, "breakPage", "Page") { app.doc.insert_page_break(); ui.close(); }
                if menu_item(ui, "breakColumn", "Column") { app.doc.insert_column_break(); ui.close(); }
                if menu_item(ui, "breakTextWrapping", "Text Wrapping") { app.doc.insert_line_break(); ui.close(); }
                ui.separator();
                ui.label(RichText::new(crate::i18n::tr("Section Breaks")).strong());
                for (icon, name, k) in [("breakNextPage", "Next Page", SectionKind::NextPage), ("breakContinuous", "Continuous", SectionKind::Continuous), ("breakEvenPage", "Even Page", SectionKind::EvenPage), ("breakOddPage", "Odd Page", SectionKind::OddPage)] {
                    if menu_item(ui, icon, name) { app.doc.insert_section_break(k); ui.close(); }
                }
            });
            let l = small_menu(ui, "lineNumbers", "Line Numbers", "Number the lines in the margin");
            crate::theme::menu(&l, |ui| {
                let cur = app.doc.section_at(app.doc.current_section()).line_numbers;
                let set = |r: Option<LineRestart>| r.map(|restart| LineNumbering { start: 1, count_by: 1, restart, distance: 0 });
                for (name, r) in [("None", None), ("Continuous", Some(LineRestart::Continuous)), ("Restart Each Page", Some(LineRestart::Page)), ("Restart Each Section", Some(LineRestart::Section))] {
                    if ui.selectable_label(cur.map(|c| c.restart) == r, crate::i18n::tr(&(name))).clicked() { app.doc.set_line_numbers(set(r)); ui.close(); }
                }
                let suppressed = app.doc.current_para_format().suppress_line_numbers;
                if ui.selectable_label(suppressed, crate::i18n::tr("Suppress for Current Paragraph")).clicked() { app.doc.set_para_props(word_engine::ParaProps { suppress_line_numbers: Some(!suppressed), ..Default::default() }); ui.close(); }
                ui.separator();
                if ui.button(crate::i18n::tr("Line Numbering Options…")).clicked() { app.open_line_numbers_dialog(); ui.close(); }
            });
            let h = small_menu(ui, "hyphenation", "Hyphenation", "Break words between syllables");
            crate::theme::menu(&h, |ui| {
                let mut hy = app.doc.page.hyphenation;
                if ui.selectable_label(!hy.auto, crate::i18n::tr("None")).clicked() { hy.auto = false; app.doc.set_hyphenation(hy); ui.close(); }
                if ui.selectable_label(hy.auto, crate::i18n::tr("Automatic")).clicked() { hy.auto = true; app.doc.set_hyphenation(hy); ui.close(); }
                ui.separator();
                if ui.button(crate::i18n::tr("Hyphenation Options…")).clicked() { app.extras.refs.hyphenation = Some(app.doc.page.hyphenation); ui.close(); }
            });
        });
    });
    if launch { app.dialogs.page_setup = Some(app.doc.page.clone()); }
    let pf = app.doc.current_para_format();
    let launch = group_l(ui, "Paragraph", "Paragraph Settings", |ui| {
        let mut changed = None;
        rows(ui, |ui| {
            ui.spacing_mut().interact_size.y = 20.0;
            let line = |ui: &mut Ui, f: &mut dyn FnMut(&mut Ui)| { ui.allocate_ui_with_layout(vec2(250.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| f(ui)); };
            line(ui, &mut |ui| {
                ui.add_sized(vec2(118.0, 18.0), egui::Label::new(RichText::new(crate::i18n::tr("Indent")).size(11.5).color(c_ink3())).halign(egui::Align::LEFT));
                ui.label(RichText::new(crate::i18n::tr("Spacing")).size(11.5).color(c_ink3()));
            });
            let fields: [[(&str, &str, bool, bool); 2]; 2] = [[("indentLeft", "Left", true, true), ("spaceBefore", "Before", false, true)], [("indentRight", "Right", true, false), ("spaceAfter", "After", false, false)]];
            for row in fields {
                line(ui, &mut |ui| {
                    for (icon, label, indent, first) in row {
                        ui.allocate_ui_with_layout(vec2(54.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| paint_label_icon(ui, icon, label));
                        let w = vec2(58.0, 20.0);
                        if indent {
                            let u = crate::view::units();
                            let mut v = u.from_twips(if first { pf.indent_left } else { pf.indent_right });
                            if ui.add_sized(w, egui::DragValue::new(&mut v).speed(u.speed()).max_decimals(2).suffix(u.suffix())).changed() {
                                let t = Some(u.to_twips(v).clamp(-31680, 31680));
                                changed = Some(if first { word_engine::ParaProps { indent_left: t, ..Default::default() } } else { word_engine::ParaProps { indent_right: t, ..Default::default() } });
                            }
                            ui.add_space(4.0);
                        } else {
                            let mut v = if first { pf.space_before } else { pf.space_after };
                            if ui.add_sized(w, egui::DragValue::new(&mut v).speed(0.5).range(0.0..=600.0).suffix(" pt")).changed() {
                                changed = Some(if first { word_engine::ParaProps { space_before: Some(v), ..Default::default() } } else { word_engine::ParaProps { space_after: Some(v), ..Default::default() } });
                            }
                        }
                    }
                });
            }
        });
        if let Some(p) = changed { app.doc.set_para_props(p); }
    });
    if launch { crate::ui::open_paragraph_dialog(app); }
    arrange_group(app, ui);
}

/// Layout ▸ Arrange: enabled for a selected picture or object, like Word.
fn arrange_group(app: &mut WordApp, ui: &mut Ui) {
    use crate::picture::Turn;
    use word_engine::arrange::Stack;
    use word_engine::model::Wrap;
    let sel = app.selected_box();
    let floating = sel.is_some_and(|p| app.doc.anchor_at(p).is_some());
    group(ui, "Arrange", |ui| {
        let pos = large(ui, "position", "Position", false, sel.is_some(), "Place the object on the page");
        if sel.is_some() { crate::theme::menu(&pos, |ui| crate::picture::position_menu(app, ui)); }
        let wrap = large(ui, "wrapText", "Wrap Text", false, sel.is_some(), "How text flows around the object");
        if sel.is_some() { crate::theme::menu(&wrap, |ui| crate::picture::wrap_menu(app, ui)); }
        rows(ui, |ui| {
            let f = small(ui, "bringForward", Some("Bring Forward"), false, sel.is_some(), "Bring the object forward");
            if let Some(p) = sel {
                crate::theme::menu(&f, |ui| {
                    if floating && menu_item(ui, "bringForward", "Bring Forward") { app.doc.restack(p, Stack::Forward); app.doc.select(p, p + 1); ui.close(); }
                    if floating && menu_item(ui, "bringToFront", "Bring to Front") { app.doc.restack(p, Stack::ToFront); app.doc.select(p, p + 1); ui.close(); }
                    if menu_item(ui, "inFront", "Bring in Front of Text") { app.set_wrap(Some(Wrap::InFront)); ui.close(); }
                });
            }
            let b = small(ui, "sendBackward", Some("Send Backward"), false, sel.is_some(), "Send the object backward");
            if let Some(p) = sel {
                crate::theme::menu(&b, |ui| {
                    if floating && menu_item(ui, "sendBackward", "Send Backward") { app.doc.restack(p, Stack::Backward); app.doc.select(p, p + 1); ui.close(); }
                    if floating && menu_item(ui, "sendToBack", "Send to Back") { app.doc.restack(p, Stack::ToBack); app.doc.select(p, p + 1); ui.close(); }
                    if menu_item(ui, "behindText", "Send Behind Text") { app.set_wrap(Some(Wrap::Behind)); ui.close(); }
                });
            }
            if small(ui, "selectionPane", Some("Selection Pane"), app.extras.refs.selection_pane, true, "List the objects on the document").clicked() { app.extras.refs.selection_pane = !app.extras.refs.selection_pane; }
        });
        rows(ui, |ui| {
            let al = small(ui, "objAlignLeft", Some("Align"), false, sel.is_some(), "Align the object");
            if sel.is_some() {
                crate::theme::menu(&al, |ui| {
                    use word_engine::model::{AnchorAlign, RelFrom};
                    for (icon, name, h, horiz) in [("objAlignLeft", "Align Left", AnchorAlign::Start, true), ("objAlignCenter", "Align Center", AnchorAlign::Center, true), ("objAlignRight", "Align Right", AnchorAlign::End, true), ("objAlignTop", "Align Top", AnchorAlign::Start, false), ("objAlignMiddle", "Align Middle", AnchorAlign::Center, false), ("objAlignBottom", "Align Bottom", AnchorAlign::End, false)] {
                        if menu_item(ui, icon, name) {
                            if let Some(p) = sel {
                                match app.doc.anchor_at(p) {
                                    Some(mut an) => { if horiz { an.x_align = Some(h); an.x_from = RelFrom::Margin; } else { an.y_align = Some(h); an.y_from = RelFrom::Margin; } app.doc.set_anchor(p, Some(an)); }
                                    None if horiz => app.doc.set_alignment(match h { AnchorAlign::Start => Alignment::Left, AnchorAlign::End => Alignment::Right, _ => Alignment::Center }),
                                    None => {}
                                }
                            }
                            ui.close();
                        }
                    }
                });
            }
            small(ui, "groupObjects", Some("Group"), false, false, "Grouping needs several selected objects");
            let rot = small(ui, "rotate", Some("Rotate"), false, sel.is_some(), "Rotate or flip");
            if sel.is_some() {
                crate::theme::menu(&rot, |ui| {
                    for (icon, name, t) in [("rotateRight", "Rotate Right 90°", Turn::Right), ("rotateLeft", "Rotate Left 90°", Turn::Left), ("flipVertical", "Flip Vertical", Turn::FlipV), ("flipHorizontal", "Flip Horizontal", Turn::FlipH)] {
                        if menu_item(ui, icon, name) { app.turn_selection(t); ui.close(); }
                    }
                });
            }
        });
    });
}

fn paint_label_icon(ui: &mut Ui, icon: &str, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        paint_icon(ui, icon, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), 16, true);
        ui.label(RichText::new(label).size(12.0).color(c_ink2()));
    });
}

fn references(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::references::{CitationStyle, TocOptions};
    let has_toc = app.has_block("TOC", false);
    group(ui, "Table of Contents", |ui| {
        let t = large_menu(ui, "toc", "Table of Contents", "Insert a table of contents");
        crate::theme::menu(&t, |ui| {
            ui.label(RichText::new(crate::i18n::tr("Built-In")).strong());
            for (name, o) in [("Automatic Table 1", TocOptions::default()), ("Automatic Table 2", TocOptions { leader: word_engine::Leader::None, ..Default::default() }), ("Manual Table", TocOptions { manual: true, hyperlinks: false, ..Default::default() })] {
                if ui.button(crate::i18n::tr(&name)).clicked() { if has_toc && !o.manual { app.doc.remove_toc(); } app.doc.insert_toc(o); ui.close(); }
            }
            ui.separator();
            if ui.button(crate::i18n::tr("Custom Table of Contents…")).clicked() { app.extras.refs.toc = Some(TocOptions::default()); ui.close(); }
            if ui.add_enabled(has_toc, egui::Button::new(crate::i18n::tr("Remove Table of Contents"))).clicked() { app.doc.remove_toc(); ui.close(); }
        });
        rows(ui, |ui| {
            let a = small_menu(ui, "addText", "Add Text", "Add the paragraph to the table of contents");
            crate::theme::menu(&a, |ui| {
                let level = app.doc.current_para_format().outline_level;
                for (name, l) in [("Do Not Show in Table of Contents", 0u8), ("Level 1", 1), ("Level 2", 2), ("Level 3", 3)] {
                    let on = if l == 0 { level >= 9 } else { level + 1 == l };
                    if ui.selectable_label(on, crate::i18n::tr(&name)).clicked() { app.doc.set_toc_level(l); ui.close(); }
                }
            });
            if small(ui, "updateTable", Some("Update Table"), false, has_toc, "Update the table of contents").clicked() { app.doc.update_tables_of("TOC"); }
            if small(ui, "updateFields", Some("Update Fields"), false, true, "Update every table, index and field").clicked() { app.doc.update_tables(); }
        });
    });
    let launch = group_l(ui, "Footnotes", "Footnote and Endnote", |ui| {
        if large(ui, "footnote", "Insert Footnote", false, true, "⌥⌘F").clicked() { app.insert_note(NoteKind::Foot); }
        rows(ui, |ui| {
            if small(ui, "endnote", Some("Insert Endnote"), false, true, "⌥⌘D").clicked() { app.insert_note(NoteKind::End); }
            let n = small_menu(ui, "nextFootnote", "Next Footnote", "Go to a footnote or endnote");
            crate::theme::menu(&n, |ui| {
                for (name, kind, fwd) in [("Next Footnote", NoteKind::Foot, true), ("Previous Footnote", NoteKind::Foot, false), ("Next Endnote", NoteKind::End, true), ("Previous Endnote", NoteKind::End, false)] {
                    if ui.button(crate::i18n::tr(&name)).clicked() { go_to_note(app, kind, fwd); ui.close(); }
                }
            });
            if small(ui, "showNotes", Some("Show Notes"), app.show_notes, true, "").clicked() { app.show_notes = !app.show_notes; }
        });
    });
    if launch { let (fmt, start) = app.doc.page.footnote_format; app.extras.refs.notes = Some(crate::refs::NotesDraft { foot: true, fmt, start }); }
    group(ui, "Research", |ui| {
        if large(ui, "smartLookup", "Smart Lookup", false, true, "Look up the selection").clicked() { app.open_wikipedia(); }
    });
    group(ui, "Citations & Bibliography", |ui| {
        let c = large_menu(ui, "citation", "Insert Citation", "Cite a source");
        crate::theme::menu(&c, |ui| {
            let numbers = word_engine::references::citation_numbers(&app.doc);
            let mut pick = None;
            for s in &app.doc.sources {
                if ui.button(format!("{}\n{}", word_engine::references::citation_text(&app.doc, &s.tag, &numbers), s.title)).clicked() { pick = Some(s.tag.clone()); }
            }
            if let Some(t) = pick { app.doc.insert_field(word_engine::Field::Citation(t)); ui.close(); }
            if !app.doc.sources.is_empty() { ui.separator(); }
            if ui.button(crate::i18n::tr("Add New Source…")).clicked() { app.new_source(true); ui.close(); }
            if ui.button(crate::i18n::tr("Add New Placeholder…")).clicked() {
                let mut s = word_engine::references::Source { title: "Placeholder".into(), ..Default::default() };
                s.tag = app.doc.new_source_tag(&s);
                let tag = s.tag.clone();
                app.doc.add_source(s);
                app.doc.insert_field(word_engine::Field::Citation(tag));
                ui.close();
            }
        });
        rows(ui, |ui| {
            if small(ui, "manageSources", Some("Manage Sources"), false, true, "Add, edit and delete sources").clicked() { app.extras.refs.manage_sources = true; }
            ui.horizontal(|ui| {
                ui.label(RichText::new(crate::i18n::tr("Style")).size(12.0).color(c_ink2()));
                let cur = app.doc.citation_style;
                let mut pick = cur;
                crate::theme::Combo::from_id_salt("cite-style").width(110.0).selected_text(crate::i18n::tr(&cur.name())).show_ui(ui, |ui| { for (s, n) in CitationStyle::ALL { ui.selectable_value(&mut pick, s, crate::i18n::tr(&n)); } });
                if pick != cur { app.doc.set_citation_style(pick); }
            });
            let b = small_menu(ui, "bibliography", "Bibliography", "Insert a bibliography");
            crate::theme::menu(&b, |ui| {
                for h in ["Bibliography", "References", "Works Cited"] { if ui.button(crate::i18n::tr(&h)).clicked() { app.doc.insert_bibliography(Some(h)); ui.close(); } }
                ui.separator();
                if ui.button(crate::i18n::tr("Insert Bibliography")).clicked() { app.doc.insert_bibliography(None); ui.close(); }
            });
        });
    });
    let has_tof = app.has_block("TOC", true);
    group(ui, "Captions", |ui| {
        if large(ui, "caption", "Insert Caption", false, true, "Label a figure, table or equation").clicked() {
            let label = if app.doc.locate(app.doc.range().0).0 < app.doc.paragraphs.len() && app.doc.paragraphs[app.doc.locate(app.doc.range().0).0].cell.is_some() { "Table" } else if app.selected_box().is_some_and(|p| matches!(app.doc.object_at(p), Some(word_engine::objects::Object::Equation(_)))) { "Equation" } else { "Figure" };
            app.extras.refs.caption = Some(crate::refs::CaptionDraft { label: label.into(), text: String::new(), above: label == "Table" });
        }
        rows(ui, |ui| {
            let r = small_menu(ui, "tableOfFigures", "Insert Table of Figures", "List the captions with their pages");
            crate::theme::menu(&r, |ui| {
                for l in word_engine::references::CAPTION_LABELS { if ui.button(crate::i18n::tr(&format!("Table of {l}s"))).clicked() { app.doc.insert_table_of_figures(l, TocOptions::default()); ui.close(); } }
                ui.separator();
                if ui.button(crate::i18n::tr("Options…")).clicked() { app.extras.refs.figures = Some(("Figure".into(), TocOptions::default())); ui.close(); }
            });
            if small(ui, "updateTable", Some("Update Table"), false, has_tof, "Update the table of figures").clicked() { app.doc.update_tables_of("TOC"); }
            if small(ui, "crossReference", Some("Cross-reference"), false, true, "Refer to a heading, bookmark or caption").clicked() { app.insert.xref = Some(crate::insert::XrefDraft { headings: true, page: false, selected: None }); }
        });
    });
    let has_index = app.has_block("INDEX", false);
    group(ui, "Index", |ui| {
        if large(ui, "markEntry", "Mark Entry", false, true, "Mark the selection for the index").clicked() { app.open_mark_entry(); }
        rows(ui, |ui| {
            if small(ui, "index", Some("Insert Index"), false, true, "Insert the index").clicked() { app.extras.refs.index = Some(2); }
            if small(ui, "updateIndex", Some("Update Index"), false, has_index, "Update the index").clicked() { app.doc.update_tables_of("INDEX"); }
        });
    });
    let has_toa = app.has_block("TOA", false);
    group(ui, "Table of Authorities", |ui| {
        if large(ui, "markCitation", "Mark Citation", false, true, "Mark a legal citation for the table of authorities").clicked() { app.open_mark_citation(); }
        rows(ui, |ui| {
            if small(ui, "tableOfAuthorities", Some("Insert Table"), false, true, "Insert the table of authorities").clicked() { app.doc.insert_toa(); }
            if small(ui, "updateAuthorities", Some("Update Table"), false, has_toa, "Update the table of authorities").clicked() { app.doc.update_tables_of("TOA"); }
        });
    });
}

/// Move to the next or previous footnote or endnote reference.
fn go_to_note(app: &mut WordApp, kind: NoteKind, forward: bool) {
    let at = app.doc.range().1;
    let refs: Vec<usize> = app.doc.note_refs().into_iter().filter(|r| r.0 == kind).map(|r| r.2).collect();
    let target = if forward { refs.iter().find(|p| **p >= at).or(refs.first()) } else { refs.iter().rev().find(|p| **p + 1 < at).or(refs.last()) };
    if let Some(&p) = target { app.doc.select(p, p + 1); app.scroll_to_caret = true; }
}

fn review(app: &mut WordApp, ui: &mut Ui) {
    group(ui, "Proofing", |ui| {
        if large(ui, "spelling", "Spelling & Grammar", app.extras.review.editor_open, true, "Check spelling and grammar (F7)").clicked() { if app.extras.review.editor_open { app.extras.review.editor_open = false; } else { app.open_editor(); } }
        if large(ui, "thesaurus", "Thesaurus", app.extras.review.thesaurus.is_some(), true, "Synonyms for the selected word (⇧F7)").clicked() { if app.extras.review.thesaurus.is_some() { app.extras.review.thesaurus = None; } else { app.open_thesaurus(); } }
        let r = large(ui, "wordCount", "Word Count", false, true, "Words, characters, paragraphs and pages");
        crate::theme::menu(&r, |ui| {
            crate::theme::grid(ui, egui::Grid::new("word-count").num_columns(2).spacing(vec2(24.0, 6.0)), |ui| {
                let text = app.doc.text();
                for (k, v) in [("Pages", app.layout.page_count()), ("Words", app.doc.word_count()), ("Characters (no spaces)", text.chars().filter(|c| !c.is_whitespace()).count()), ("Characters (with spaces)", app.doc.character_count()), ("Paragraphs", app.doc.paragraphs.iter().filter(|p| !p.is_empty()).count()), ("Lines", app.layout.lines.len())] { ui.label(crate::i18n::tr(k)); ui.label(RichText::new(v.to_string()).strong()); ui.end_row(); }
            });
        });
    });
    group(ui, "Speech", |ui| {
        let speaking = app.reading_aloud();
        if large(ui, "readAloud", if speaking { "Stop Reading" } else { "Read Aloud" }, speaking, true, "Read the text aloud from the caret").clicked() { app.toggle_read_aloud(); }
    });
    group(ui, "Accessibility", |ui| {
        if large(ui, "accessibility", "Check Accessibility", app.extras.review.accessibility, true, "Find content that's hard to read for people with disabilities").clicked() { app.extras.review.accessibility = !app.extras.review.accessibility; }
    });
    group(ui, "Language", |ui| {
        let l = large_menu(ui, "language", "Language", "Translate, and proofing options");
        crate::theme::menu(&l, |ui| {
            crate::theme::submenu(ui, crate::i18n::tr("Translate Selection"), |ui| { for (code, name) in crate::review::LANGUAGES { if ui.button(crate::i18n::tr(&*name)).clicked() { app.start_translate(code, false); ui.close(); } } });
            crate::theme::submenu(ui, crate::i18n::tr("Translate Document"), |ui| { for (code, name) in crate::review::LANGUAGES { if ui.button(crate::i18n::tr(&*name)).clicked() { app.start_translate(code, true); ui.close(); } } });
            if ui.button(crate::i18n::tr("Translation Models…")).clicked() { app.extras.review.models_open = true; ui.close(); }
            ui.separator();
            ui.label(RichText::new(crate::i18n::tr("Proofing language: English (United States)")).small().weak());
            ui.checkbox(&mut app.extras.review.check_as_you_type, crate::i18n::tr("Check spelling as you type"));
            if ui.checkbox(&mut app.extras.review.grammar, crate::i18n::tr("Mark grammar errors as you type")).changed() { app.extras.review.issues_rev = u64::MAX; }
        });
    });
    let has_comments = !app.doc.comments.is_empty();
    group(ui, "Comments", |ui| {
        if large(ui, "newComment", "New Comment", false, app.doc.protection.as_ref().is_none_or(|p| p.kind == word_engine::review::Restriction::Comments), "⌥⌘M").clicked() { app.new_comment(); }
        rows(ui, |ui| {
            let d = small(ui, "deleteComment", None, false, has_comments, "Delete comment");
            if has_comments {
                crate::theme::menu(&d, |ui| {
                    if ui.button(crate::i18n::tr("Delete")).clicked() { if let Some(id) = app.doc.comments_at(app.doc.selection.focus).first().cloned() { app.doc.delete_comment(&id); } ui.close(); }
                    if ui.button(crate::i18n::tr("Delete All Comments in Document")).clicked() { let roots: Vec<String> = app.doc.comments.iter().filter(|c| c.parent.is_none()).map(|c| c.id.clone()).collect(); for id in roots { app.doc.delete_comment(&id); } ui.close(); }
                });
            }
            if small(ui, "previous", None, false, has_comments, "Previous comment").clicked() { crate::ui::goto_comment(app, false); }
            if small(ui, "next", None, false, has_comments, "Next comment").clicked() { crate::ui::goto_comment(app, true); }
        });
        if large(ui, "showComments", "Show Comments", app.show_comments, true, "Show the comments pane").clicked() { app.show_comments = !app.show_comments; }
    });
    let locked_tracking = app.doc.protection.as_ref().is_some_and(|p| p.kind == word_engine::review::Restriction::TrackedChanges);
    group(ui, "Tracking", |ui| {
        if large(ui, "trackChanges", "Track Changes", app.doc.track_changes, !locked_tracking, "⇧⌘E").clicked() { let on = !app.doc.track_changes; app.doc.set_track_changes(on); }
        rows(ui, |ui| {
            let r = small(ui, "markup", None, false, true, "Display for Review");
            crate::theme::menu(&r, |ui| {
                for (m, l) in [(Markup::Simple, "Simple Markup"), (Markup::All, "All Markup"), (Markup::None, "No Markup"), (Markup::Original, "Original")] {
                    if ui.selectable_label(app.doc.markup == m, crate::i18n::tr(&l)).clicked() { app.doc.set_markup(m); ui.close(); }
                }
            });
            let m = small(ui, "showMarkup", None, false, true, "Show Markup");
            crate::theme::menu(&m, |ui| {
                ui.checkbox(&mut app.extras.review.show_comment_marks, crate::i18n::tr("Comments"));
                let mut insdel = app.doc.markup != Markup::None;
                if ui.checkbox(&mut insdel, crate::i18n::tr("Insertions and Deletions")).changed() { app.doc.set_markup(if insdel { Markup::All } else { Markup::None }); }
                ui.checkbox(&mut app.show_comments, crate::i18n::tr("Balloons (comments pane)"));
            });
            if small(ui, "reviewingPane", None, app.extras.review.reviewing_pane, true, "Reviewing Pane: every revision and comment").clicked() { app.extras.review.reviewing_pane = !app.extras.review.reviewing_pane; }
        });
    });
    let any = !app.doc.revisions().is_empty();
    group(ui, "Changes", |ui| {
        for (accept, icon, label) in [(true, "accept", "Accept"), (false, "reject", "Reject")] {
            let r = large_menu(ui, icon, label, if accept { "Accept changes" } else { "Reject changes" });
            if any {
                crate::theme::menu(&r, |ui| {
                    let n = app.doc.len();
                    if ui.button(crate::i18n::tr(&format!("{} and Move to Next", if accept { "Accept" } else { "Reject" }))).clicked() { let (s, e) = app.doc.range(); app.doc.resolve_revisions(accept, s, e, false); crate::ui::goto_change(app, true); ui.close(); }
                    if ui.button(crate::i18n::tr(&format!("{} This Change", if accept { "Accept" } else { "Reject" }))).clicked() { let (s, e) = app.doc.range(); app.doc.resolve_revisions(accept, s, e, false); ui.close(); }
                    if ui.button(crate::i18n::tr(&format!("{} All Changes", if accept { "Accept" } else { "Reject" }))).clicked() { app.doc.resolve_revisions(accept, 0, n, true); ui.close(); }
                    if ui.button(crate::i18n::tr(&format!("{} All Changes and Stop Tracking", if accept { "Accept" } else { "Reject" }))).clicked() { app.doc.resolve_revisions(accept, 0, n, true); if !locked_tracking { app.doc.set_track_changes(false); } ui.close(); }
                });
            }
        }
        rows(ui, |ui| {
            if small(ui, "previousArrow", None, false, any, "Previous change").clicked() { crate::ui::goto_change(app, false); }
            if small(ui, "nextArrow", None, false, any, "Next change").clicked() { crate::ui::goto_change(app, true); }
        });
    });
    group(ui, "Compare", |ui| {
        let c = large_menu(ui, "compare", "Compare", "Compare two versions of a document");
        crate::theme::menu(&c, |ui| {
            if ui.button(crate::i18n::tr("Compare…")).clicked() { app.extras.review.compare = Some(crate::review::CompareDraft { use_current: true, ..Default::default() }); ui.close(); }
            if ui.button(crate::i18n::tr("Combine…")).on_hover_text(crate::i18n::tr("Shows another version's changes in this document")).clicked() { app.extras.review.compare = Some(crate::review::CompareDraft { use_current: true, ..Default::default() }); ui.close(); }
        });
    });
    group(ui, "Protect", |ui| {
        let on = app.doc.protection.is_some();
        if large_menu(ui, "protect", "Restrict Editing", "Limit what others can change").clicked() {
            if on { app.extras.review.unlock = Some(String::new()); } else { app.extras.review.restrict = Some(crate::review::RestrictDraft { kind: word_engine::review::Restriction::ReadOnly, password: String::new(), confirm: String::new() }); }
        }
    });
    group(ui, "Ink", |ui| {
        if large(ui, "hideInk", "Hide Ink", app.extras.draw.hide_ink, true, "Hide or show ink").clicked() { app.extras.draw.hide_ink = !app.extras.draw.hide_ink; }
    });
}

fn view(app: &mut WordApp, ui: &mut Ui) {
    use crate::view::{Immersive, PageTint, Units, ViewMode};
    let mode = app.extras.view.mode;
    group(ui, "Views", |ui| {
        if large(ui, "readMode", "Read Mode", app.read_mode, true, "Read the document full screen").clicked() { app.set_view(ViewMode::Print); app.read_mode = true; }
        if large(ui, "printLayout", "Print Layout", mode == ViewMode::Print && !app.read_mode, true, "The document as it will print").clicked() { app.set_view(ViewMode::Print); }
        if large(ui, "webLayout", "Web Layout", mode == ViewMode::Web, true, "The document as a web page: text fits the window").clicked() { app.set_view(ViewMode::Web); }
        rows(ui, |ui| {
            if small(ui, "outlineView", Some("Outline"), mode == ViewMode::Outline, true, "Headings as an outline you can rearrange").clicked() { app.set_view(ViewMode::Outline); }
            if small(ui, "draft", Some("Draft"), mode == ViewMode::Draft, true, "Text only, without page layout").clicked() { app.set_view(ViewMode::Draft); }
        });
    });
    group(ui, "Immersive", |ui| {
        if large(ui, "focus", "Focus", app.focus_mode, true, "Hide the ribbon and status bar (Esc to return)").clicked() { app.focus_mode = true; }
        if large(ui, "readAloud", "Immersive Reader", app.extras.view.immersive.is_some(), true, "Easier reading: column width, page color, line focus, syllables").clicked() {
            app.extras.view.immersive = if app.extras.view.immersive.is_some() { None } else { Some(Immersive { width: 800.0, tint: PageTint::None, spacing: false, syllables: false, focus: None, current: 0 }) };
        }
    });
    group(ui, "Show", |ui| {
        rows(ui, |ui| {
            ui.checkbox(&mut app.show_ruler, crate::i18n::tr("Ruler"));
            ui.checkbox(&mut app.extras.view.page_grid, crate::i18n::tr("Gridlines"));
            ui.checkbox(&mut app.show_outline, crate::i18n::tr("Navigation Pane"));
        });
        rows(ui, |ui| {
            ui.add_space(26.0);
            ui.allocate_ui_with_layout(vec2(140.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(RichText::new(crate::i18n::tr("Units")).size(12.0).color(c_ink2()));
                let cur = app.extras.view.units;
                crate::theme::Combo::from_id_salt("units").width(92.0).selected_text(crate::i18n::tr(&cur.name())).show_ui(ui, |ui| { for (u, n) in Units::ALL { ui.selectable_value(&mut app.extras.view.units, u, crate::i18n::tr(&n)); } });
            });
        });
    });
    group(ui, "Zoom", |ui| {
        if large(ui, "zoom", "Zoom", false, true, "Choose a zoom level").clicked() { app.extras.view.zoom_dialog = Some((app.zoom, 0)); }
        if large(ui, "zoom100", "100%", (app.zoom - 1.0).abs() < 0.01, true, "Actual size (⌘0)").clicked() { app.extras.view.multi_page = false; app.set_zoom(1.0); }
        rows(ui, |ui| {
            if small(ui, "onePage", Some("One Page"), false, true, "A whole page in the window").clicked() { app.fit_page(1); }
            if small(ui, "multiplePages", Some("Multiple Pages"), app.extras.view.multi_page, true, "Pages side by side").clicked() { app.fit_page(2); }
            if small(ui, "pageWidth", Some("Page Width"), false, true, "Fit the page width to the window").clicked() { app.extras.view.multi_page = false; app.fit_width = true; }
        });
    });
    group(ui, "Window", |ui| {
        if large(ui, "newWindow", "New Window", false, true, "Open another window on this document").clicked() { app.new_window(); }
        rows(ui, |ui| {
            if small(ui, "split", Some(if app.extras.view.split { "Remove Split" } else { "Split" }), app.extras.view.split, true, "Two views of this document").clicked() { app.extras.view.split = !app.extras.view.split; }
            if small(ui, "sideToSide", Some("View Side by Side"), app.extras.view.side.is_some(), true, "Another document beside this one").clicked() { if app.extras.view.side.is_some() { app.extras.view.side = None; } else { app.open_side_by_side(); app.extras.view.sync_side = true; } }
            let v = &app.extras.view;
            let (avail, on) = (v.split || v.side.is_some(), if v.side.is_some() { v.sync_side } else { v.sync_split });
            if small(ui, "verticalMovement", Some("Synchronous Scrolling"), on && avail, avail, "Scroll both views together").clicked() {
                let v = &mut app.extras.view;
                if v.side.is_some() { v.sync_side = !v.sync_side; } else { v.sync_split = !v.sync_split; }
            }
        });
    });
    group(ui, "Macros", |ui| {
        let m = large_menu(ui, "field", "Macros", "View, record and run macros");
        crate::theme::menu(&m, |ui| crate::macros::menu(app, ui));
    });
    group(ui, "Properties", |ui| {
        if large(ui, "info", "Properties", false, true, "Title, author, keywords and statistics").clicked() { app.open_properties(); }
    });
    group(ui, "Dark Mode", |ui| {
        let dark = app.extras.view.dark;
        if large(ui, "darkPage", "Dark Mode", dark, true, "Dark pages and chrome").clicked() { let ctx = ui.ctx().clone(); app.set_dark(&ctx, !dark); }
    });
}

fn table(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::model::{CellVAlign, TablePart};
    use word_engine::tables::{AutoFit, Separator, TableStyle};
    let block = app.doc.cell_block();
    let multi = block.is_some_and(|b| app.doc.block_cells(b).len() > 1);
    group(ui, "Table", |ui| {
        let sel = large(ui, "selectTable", "Select", false, true, "Select cells, rows, columns or the table");
        crate::theme::menu(&sel, |ui| {
            if small(ui, "selectCell", Some("Select Cell"), false, true, "").clicked() { if let Some(b) = app.doc.cell_block() { app.doc.select_block(b); } ui.close(); }
            if small(ui, "selectColumn", Some("Select Column"), false, true, "").clicked() { app.doc.select_table_part(TablePart::Column); ui.close(); }
            if small(ui, "selectRow", Some("Select Row"), false, true, "").clicked() { app.doc.select_table_part(TablePart::Row); ui.close(); }
            if small(ui, "selectTable", Some("Select Table"), false, true, "").clicked() { app.doc.select_table_part(TablePart::Table); ui.close(); }
        });
        rows(ui, |ui| {
            if small(ui, "gridlines", Some("View Gridlines"), app.show_gridlines, true, "Show cell edges of tables without borders").clicked() { app.show_gridlines = !app.show_gridlines; }
            let header = block.is_some_and(|b| app.doc.tables[b.table].rows.first().is_some_and(|r| r.header));
            if small(ui, "repeatHeader", Some("Repeat Header Rows"), header, true, "Repeat the top rows on every page").clicked() { app.doc.set_header_rows(!header); }
            let keep = block.is_some_and(|b| app.doc.tables[b.table].rows[b.r0].cant_split);
            if small(ui, "keepRow", Some("Keep Rows Together"), keep, true, "Don't let rows break across pages").clicked() { app.doc.set_rows_cant_split(!keep); }
        });
    });
    group(ui, "Draw", |ui| {
        use crate::app::TableTool;
        if large(ui, "drawTable", "Draw Table", app.table_tool == Some(TableTool::Pen), true, "Draw cell borders: drag across a cell to split it (Esc to stop)").clicked() { app.table_tool = if app.table_tool == Some(TableTool::Pen) { None } else { Some(TableTool::Pen) }; }
        if large(ui, "tableEraser", "Eraser", app.table_tool == Some(TableTool::Eraser), true, "Click a line between cells to merge them (Esc to stop)").clicked() { app.table_tool = if app.table_tool == Some(TableTool::Eraser) { None } else { Some(TableTool::Eraser) }; }
    });
    group(ui, "Rows & Columns", |ui| {
        let d = large(ui, "deleteTable", "Delete", false, true, "Delete rows, columns or the table");
        crate::theme::menu(&d, |ui| {
            if small(ui, "deleteCells", Some("Delete Cell (Shift Left)"), false, true, "").clicked() { app.doc.delete_cell(); ui.close(); }
            if small(ui, "deleteRow", Some("Delete Rows"), false, true, "").clicked() { app.doc.delete_table_part(TablePart::Row); ui.close(); }
            if small(ui, "deleteColumn", Some("Delete Columns"), false, true, "").clicked() { app.doc.delete_table_part(TablePart::Column); ui.close(); }
            if small(ui, "deleteTable", Some("Delete Table"), false, true, "").clicked() { app.doc.delete_table_part(TablePart::Table); ui.close(); }
        });
        if large(ui, "insertAbove", "Insert Above", false, true, "Insert a row above").clicked() { app.doc.insert_table_row(false); }
        if large(ui, "insertBelow", "Insert Below", false, true, "Insert a row below").clicked() { app.doc.insert_table_row(true); }
        rows(ui, |ui| {
            if small(ui, "insertLeft", Some("Insert Left"), false, true, "Insert a column to the left").clicked() { app.doc.insert_table_column(false); }
            if small(ui, "insertRight", Some("Insert Right"), false, true, "Insert a column to the right").clicked() { app.doc.insert_table_column(true); }
            if small(ui, "insertCells", Some("Insert Cell"), false, true, "Insert a cell, shifting cells right").clicked() { app.doc.insert_cells(); }
        });
    });
    group(ui, "Merge", |ui| {
        if large(ui, "mergeCells", "Merge Cells", false, multi, "Merge the selected cells").clicked() { app.doc.merge_cells(); }
        rows(ui, |ui| {
            let sp = small(ui, "splitCells", Some("Split Cells…"), false, true, "Split the cell into columns");
            crate::theme::menu(&sp, |ui| {
                let id = ui.make_persistent_id("split-cols");
                let mut n: usize = ui.data(|d| d.get_temp(id)).unwrap_or(2);
                ui.horizontal(|ui| { ui.label(crate::i18n::tr("Columns")); ui.add(egui::DragValue::new(&mut n).range(1..=20)); });
                ui.data_mut(|d| d.insert_temp(id, n));
                if ui.button(crate::i18n::tr("Split")).clicked() { app.doc.split_cell(n); ui.close(); }
            });
            if small(ui, "splitTable", Some("Split Table"), false, true, "Split the table above this row").clicked() { app.doc.split_table(); }
        });
    });
    group(ui, "Cell Size", |ui| {
        rows(ui, |ui| {
            if let Some(b) = block {
                let t = &app.doc.tables[b.table];
                let mut h = t.rows[b.r0].min_height as f32 / 1440.0;
                let mut w = t.grid.get(b.c0).copied().unwrap_or(0) as f32 / 1440.0;
                let mut set_h = None;
                let mut set_w = None;
                ui.horizontal(|ui| {
                    paint_label_icon(ui, "rowHeight", "Height");
                    if ui.add(egui::DragValue::new(&mut h).speed(0.01).range(0.0..=22.0).max_decimals(2).custom_formatter(|v, _| if v == 0.0 { "Auto".into() } else { format!("{v:.2}\"") })).changed() { set_h = Some(h); }
                });
                ui.horizontal(|ui| {
                    paint_label_icon(ui, "columnWidth", "Width");
                    if ui.add(egui::DragValue::new(&mut w).speed(0.01).range(0.15..=22.0).max_decimals(2).suffix("\"")).changed() { set_w = Some(w); }
                });
                if let Some(h) = set_h { app.doc.set_row_height((h * 1440.0).round() as i32, false); }
                if let Some(w) = set_w { app.doc.set_column_width(b.table, b.c0, (w * 1440.0).round() as i32, false); }
            }
            ui.horizontal(|ui| {
                if small(ui, "distributeRows", None, false, true, "Distribute rows evenly").clicked() {
                    let heights: Vec<f32> = block.map(|b| { let t = &app.doc.tables[b.table]; (0..t.rows.len()).map(|r| app.layout.cells.iter().filter(|c| c.table == b.table && c.row == r).map(|c| c.y1 - c.y0).fold(0.0, f32::max)).collect() }).unwrap_or_default();
                    app.doc.distribute_rows(&heights);
                }
                if small(ui, "distributeColumns", None, false, true, "Distribute columns evenly").clicked() { app.doc.distribute_columns(); }
                let af = small(ui, "autofit", Some("AutoFit"), false, true, "Fit the table to its contents or the window");
                crate::theme::menu(&af, |ui| {
                    if ui.button(crate::i18n::tr("AutoFit Contents")).clicked() { app.doc.autofit(AutoFit::Contents); ui.close(); }
                    if ui.button(crate::i18n::tr("AutoFit Window")).clicked() { app.doc.autofit(AutoFit::Window); ui.close(); }
                    if ui.button(crate::i18n::tr("Fixed Column Width")).clicked() { app.doc.autofit(AutoFit::Fixed); ui.close(); }
                });
            });
        });
    });
    group(ui, "Table Position", |ui| {
        rows(ui, |ui| {
            if let Some(b) = block {
                let cur = app.doc.tables[b.table].alignment;
                ui.horizontal(|ui| {
                    for (a, icon, tip) in [(Alignment::Left, "alignLeft", "Table at the left margin"), (Alignment::Center, "alignCenter", "Center the table"), (Alignment::Right, "alignRight", "Table at the right margin")] {
                        if small(ui, icon, None, cur == a, true, tip).clicked() { let ind = app.doc.tables[b.table].indent; app.doc.set_table_alignment(b.table, a, ind); }
                    }
                });
                let mut ind = app.doc.tables[b.table].indent as f32 / 1440.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::i18n::tr("Indent")).size(11.5).color(c_ink3()));
                    if ui.add_enabled(cur == Alignment::Left, egui::DragValue::new(&mut ind).speed(0.01).range(-5.0..=10.0).max_decimals(2).suffix("\"")).changed() { app.doc.set_table_alignment(b.table, Alignment::Left, (ind * 1440.0).round() as i32); }
                });
                if small(ui, "tableProperties", Some("Properties…"), false, true, "Table properties").clicked() { app.open_table_properties(); }
            }
        });
    });
    group(ui, "Alignment", |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
            let cur = block.map(|b| { let t = &app.doc.tables[b.table]; t.cell_at_grid(b.r0, b.c0).map(|(_, c)| c.v_align).unwrap_or_default() });
            let h = app.doc.current_para_format().alignment;
            for (v, vname) in [(CellVAlign::Top, "Top"), (CellVAlign::Center, "Center"), (CellVAlign::Bottom, "Bottom")] {
                ui.horizontal(|ui| {
                    for (a, aname) in [(Alignment::Left, "Left"), (Alignment::Center, "Center"), (Alignment::Right, "Right")] {
                        let icon = format!("cellAlign{vname}{aname}");
                        if small(ui, &icon, None, cur == Some(v) && h == a, true, &format!("Align {} {}", vname.to_lowercase(), aname.to_lowercase())).clicked() { app.doc.set_cell_alignment(v, a); }
                    }
                });
            }
        });
    });
    group(ui, "Table Styles", |ui| {
        let accent = Color(0x44, 0x72, 0xC4);
        for (style, name) in [(TableStyle::Plain, "Plain"), (TableStyle::Grid, "Grid"), (TableStyle::HeaderShaded, "Header"), (TableStyle::Banded, "Banded")] {
            let (rect, r) = ui.allocate_exact_size(vec2(58.0, 64.0), Sense::click());
            ui.painter().rect(rect.shrink(1.0), crate::theme::RADIUS as f32, if r.hovered() { c_hover() } else { c_card() }, Stroke::new(1.0, c_line()), StrokeKind::Inside);
            // A 4×4 preview of the look.
            let g = Rect::from_min_size(rect.min + vec2(9.0, 7.0), vec2(40.0, 34.0));
            let ac = Color32::from_rgb(accent.0, accent.1, accent.2);
            let light = Color32::from_rgb(0xDA, 0xE3, 0xF3);
            for row in 0..4 {
                let y = g.min.y + row as f32 * g.height() / 4.0;
                let cell = Rect::from_min_size(pos2(g.min.x, y), vec2(g.width(), g.height() / 4.0));
                let fill = match style { TableStyle::HeaderShaded | TableStyle::Banded if row == 0 => Some(ac), TableStyle::Banded if row % 2 == 0 => Some(light), _ => None };
                if let Some(f) = fill { ui.painter().rect_filled(cell, crate::theme::RADIUS as f32, f); }
                if style != TableStyle::Plain { ui.painter().line_segment([cell.left_bottom(), cell.right_bottom()], Stroke::new(0.8, c_ink3())); }
            }
            if style != TableStyle::Plain {
                ui.painter().rect_stroke(g, crate::theme::RADIUS as f32, Stroke::new(0.8, c_ink3()), StrokeKind::Inside);
                for k in 1..4 { let x = g.min.x + k as f32 * g.width() / 4.0; ui.painter().line_segment([pos2(x, g.min.y), pos2(x, g.max.y)], Stroke::new(0.8, c_ink3())); }
            } else {
                for k in 0..4 { let y = g.min.y + (k as f32 + 0.5) * g.height() / 4.0; ui.painter().line_segment([pos2(g.min.x + 3.0, y), pos2(g.max.x - 3.0, y)], Stroke::new(1.0, c_line())); }
            }
            ui.painter().text(pos2(rect.center().x, rect.max.y - 11.0), Align2::CENTER_CENTER, name, FontId::proportional(10.5), c_ink3());
            if r.on_hover_text(crate::i18n::trf("Apply the {} look", &[&name])).clicked() { app.doc.apply_table_style(style, accent); }
        }
        rows(ui, |ui| {
            let sh = small(ui, "cellShading", Some("Shading"), false, true, "Cell shading");
            crate::theme::menu(&sh, |ui| { if let Some(c) = color_menu(ui, "No Color") { app.doc.set_cell_shading(c); } });
            let bd = small(ui, "cellBorders", Some("Borders"), false, true, "Borders of the selected cells, with the pen below");
            crate::theme::menu(&bd, |ui| {
                use word_engine::tables::BorderPreset as B;
                for (preset, icon, label) in [(B::All, "borderAll", "All Borders"), (B::Outside, "borderOuter", "Outside Borders"), (B::Inside, "borderInside", "Inside Borders"), (B::InsideH, "borderInsideH", "Inside Horizontal"), (B::InsideV, "borderInsideV", "Inside Vertical"), (B::Top, "borderTop", "Top Border"), (B::Bottom, "borderBottom", "Bottom Border"), (B::Left, "borderLeft", "Left Border"), (B::Right, "borderRight", "Right Border"), (B::None, "borderNone", "No Border")] {
                    if small(ui, icon, Some(label), false, true, "").clicked() { let pen = app.border_pen; app.doc.apply_border_preset(preset, pen); ui.close(); }
                }
                ui.separator();
                if ui.button(crate::i18n::tr("Table Grid (all cells)")).clicked() { app.doc.set_table_borders(true); ui.close(); }
                if ui.button(crate::i18n::tr("Remove Table Borders")).clicked() { app.doc.set_table_borders(false); ui.close(); }
            });
            ui.horizontal(|ui| {
                let pc = app.border_pen.color.unwrap_or(Color::BLACK);
                let (main, arrow) = color_split(ui, "penColor", Color32::from_rgb(pc.0, pc.1, pc.2), "Pen color");
                let _ = main;
                crate::theme::menu(&arrow, |ui| { if let Some(c) = color_menu(ui, "Automatic") { app.border_pen.color = c; } });
                crate::theme::Combo::from_id_salt("pen-width").width(56.0).selected_text(crate::i18n::tr(&(format!("{} pt", app.border_pen.width)))).show_ui(ui, |ui| {
                    for w in [0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.0, 4.5, 6.0] { ui.selectable_value(&mut app.border_pen.width, w, format!("{w} pt")); }
                });
            });
        });
    });
    group(ui, "Cell Margins", |ui| {
        let m = large(ui, "cellMargins", "Margins", false, block.is_some(), "Space between cell borders and text");
        egui::Popup::menu(&m).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            let Some(b) = app.doc.cell_block() else { return };
            let (table_margins, cell) = { let t = &app.doc.tables[b.table]; (t.cell_margins, t.cell_at_grid(b.r0, b.c0).and_then(|(_, c)| c.margins)) };
            ui.label(RichText::new(crate::i18n::tr("Default for the table")).strong());
            let mut m = table_margins.map(|v| v as f32 / 1440.0);
            let mut changed = false;
            crate::theme::grid(ui, egui::Grid::new("tbl-mar"), |ui| {
                for (i, name) in [(0, "Top"), (2, "Bottom"), (3, "Left"), (1, "Right")] {
                    ui.label(crate::i18n::tr(name));
                    changed |= ui.add(egui::DragValue::new(&mut m[i]).speed(0.005).range(0.0..=5.0).max_decimals(2).suffix("\"")).changed();
                    if i == 2 || i == 1 { ui.end_row(); }
                }
            });
            if changed { app.doc.set_table_cell_margins(m.map(|v| (v * 1440.0).round() as i32)); }
            ui.separator();
            let mut own = cell.is_some();
            if ui.checkbox(&mut own, crate::i18n::tr("Different margins for the selected cells")).changed() {
                let base = app.doc.tables[b.table].cell_margins;
                app.doc.set_cell_margins(if own { Some(base) } else { None });
            }
            if let Some(cm) = cell {
                let base = app.doc.tables[b.table].cell_margins;
                let mut m = [0, 1, 2, 3].map(|i| if cm[i] >= 0 { cm[i] } else { base[i] } as f32 / 1440.0);
                let mut changed = false;
                crate::theme::grid(ui, egui::Grid::new("cell-mar"), |ui| {
                    for (i, name) in [(0, "Top"), (2, "Bottom"), (3, "Left"), (1, "Right")] {
                        ui.label(crate::i18n::tr(name));
                        changed |= ui.add(egui::DragValue::new(&mut m[i]).speed(0.005).range(0.0..=5.0).max_decimals(2).suffix("\"")).changed();
                        if i == 2 || i == 1 { ui.end_row(); }
                    }
                });
                if changed { app.doc.set_cell_margins(Some(m.map(|v| (v * 1440.0).round() as i32))); }
            }
        });
    });
    group(ui, "Data", |ui| {
        let s = large(ui, "sortTable", "Sort", false, true, "Sort rows by a column");
        crate::theme::menu(&s, |ui| {
            let col = block.map(|b| b.c0).unwrap_or(0);
            ui.label(crate::i18n::trf("By column {}", &[&(col + 1)]));
            let id = ui.make_persistent_id("sort-header");
            let mut header: bool = ui.data(|d| d.get_temp(id)).unwrap_or(true);
            ui.checkbox(&mut header, crate::i18n::tr("My table has a header row"));
            ui.data_mut(|d| d.insert_temp(id, header));
            if ui.button(crate::i18n::tr("Ascending (A → Z, 0 → 9)")).clicked() { if !app.doc.sort_table(col, false, header) { app.notify(ui.ctx(), "Tables with vertically merged cells can't be sorted"); } ui.close(); }
            if ui.button(crate::i18n::tr("Descending (Z → A, 9 → 0)")).clicked() { if !app.doc.sort_table(col, true, header) { app.notify(ui.ctx(), "Tables with vertically merged cells can't be sorted"); } ui.close(); }
        });
        if large(ui, "formula", "Formula", false, true, "Calculate a value from the table (=SUM(ABOVE))").clicked() { app.formula = Some((app.doc.suggested_formula(), String::new())); }
        let c = large(ui, "convertToText", "Convert to Text", false, true, "Replace the table with text");
        crate::theme::menu(&c, |ui| {
            for (sep, label) in [(Separator::Tab, "Separate with tabs"), (Separator::Comma, "Separate with commas"), (Separator::Paragraph, "One paragraph per cell")] {
                if ui.button(crate::i18n::tr(&label)).clicked() { app.doc.convert_table_to_text(sep); ui.close(); }
            }
        });
    });
}


/// "Tell me what you want to do": type to find a command; Enter runs the first match.
fn tell_me(app: &mut WordApp, ui: &mut Ui, ctx: &egui::Context) {
    let id = egui::Id::unique("tell-me");
    let r = ui.add(egui::TextEdit::singleline(&mut app.extras.tell_me).id(id).hint_text(crate::i18n::tr("🔍  Tell me what you want to do")).desired_width(260.0).margin(egui::Margin::symmetric(10, 5)));
    if r.gained_focus() { app.canvas_focus = false; }
    let found = crate::commands::matches(&app.extras.tell_me);
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let mut run: Option<fn(&mut WordApp, &egui::Context)> = None;
    if enter { run = found.first().map(|c| c.run); }
    // The results stay open while the box has focus or the pointer is over them.
    let hover_id = id.with("hovered");
    let over = ui.data(|d| d.get_temp::<bool>(hover_id)).unwrap_or(false);
    if !found.is_empty() && (r.has_focus() || over) {
        let area = egui::Area::new(id.with("popup")).order(egui::Order::Foreground).fixed_pos(r.rect.left_bottom() + vec2(0.0, 4.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(r.rect.width());
                crate::theme::full_width(ui, |ui| {
                    for c in &found {
                        if small(ui, c.icon, Some(c.name), false, true, c.name).clicked() { run = Some(c.run); }
                    }
                });
            });
        });
        let hovered = area.response.contains_pointer();
        ui.data_mut(|d| d.insert_temp(hover_id, hovered));
    } else { ui.data_mut(|d| d.insert_temp(hover_id, false)); }
    if let Some(f) = run { app.extras.tell_me.clear(); app.canvas_focus = true; f(app, ctx); }
}

/// A pen button: the pen drawn in its ink color; the caret under it opens color and thickness.
fn pen_button(app: &mut WordApp, ui: &mut Ui, k: usize) {
    use crate::draw::{PenKind, Tool};
    let pen = app.extras.draw.pens[k];
    let on = app.extras.draw.tool == Tool::Pen(k);
    let (rect, _) = ui.allocate_exact_size(vec2(44.0, 70.0), Sense::hover());
    let top = Rect::from_min_size(rect.min, vec2(44.0, 52.0));
    let bottom = Rect::from_min_max(pos2(rect.min.x, top.max.y), rect.max);
    let r = ui.interact(top, egui::Id::unique(("pen", k)), Sense::click());
    let more = ui.interact(bottom, egui::Id::unique(("pen-more", k)), Sense::click());
    button_bg(ui, rect, &r, on);
    if more.hovered() { ui.painter().rect_filled(bottom, crate::theme::RADIUS as f32, c_hover()); }
    let icon = match pen.kind { PenKind::Pen => "inkPen", PenKind::Pencil => "penColor", PenKind::Highlighter => "highlight" };
    paint_icon(ui, icon, Rect::from_center_size(pos2(rect.center().x, rect.min.y + 22.0), vec2(30.0, 30.0)), 32, true);
    let c = Color32::from_rgb(pen.color.0, pen.color.1, pen.color.2);
    let bar = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 43.0), vec2(26.0, (pen.width * 1.5).clamp(2.0, 6.0)));
    ui.painter().rect(bar, crate::theme::RADIUS as f32, c, Stroke::new(0.5, Color32::from_gray(150)), StrokeKind::Outside);
    caret(ui, bottom.center());
    let name = match pen.kind { PenKind::Pen => "Pen", PenKind::Pencil => "Pencil", PenKind::Highlighter => "Highlighter" };
    if r.on_hover_text(format!("{name}: {:.2} mm", pen.width)).clicked() { app.extras.draw.tool = if on { Tool::Select } else { Tool::Pen(k) }; }
    crate::theme::menu(&more, |ui| pen_menu(app, ui, k));
}

fn pen_menu(app: &mut WordApp, ui: &mut Ui, k: usize) {
    use crate::draw::{PenKind, HIGHLIGHTER_WIDTHS, PEN_COLORS, PEN_WIDTHS};
    let kind = app.extras.draw.pens[k].kind;
    ui.label(RichText::new(crate::i18n::tr("Thickness")).strong());
    ui.horizontal(|ui| {
        for w in if kind == PenKind::Highlighter { HIGHLIGHTER_WIDTHS } else { PEN_WIDTHS } {
            let (rect, r) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
            let sel = (app.extras.draw.pens[k].width - w).abs() < 0.01;
            if sel || r.hovered() { ui.painter().rect_filled(rect, crate::theme::RADIUS as f32, if sel { c_press() } else { c_hover() }); }
            ui.painter().circle_filled(rect.center(), (w * 1.6).clamp(1.0, 10.0), c_ink2());
            if r.on_hover_text(format!("{w} mm")).clicked() { app.extras.draw.pens[k].width = *w; }
        }
    });
    ui.label(RichText::new(crate::i18n::tr("Colors")).strong());
    for row in PEN_COLORS.chunks(8) {
        ui.allocate_ui_with_layout(vec2(8.0 * 24.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for c in row {
                let sel = app.extras.draw.pens[k].color == *c;
                if swatch(ui, Color32::from_rgb(c.0, c.1, c.2), 20.0, sel).clicked() { app.extras.draw.pens[k].color = *c; app.extras.draw.tool = crate::draw::Tool::Pen(k); }
            }
        });
    }
    let mut pc = app.extras.draw.pens[k].color;
    let pick_id = egui::Id::new("custom-color-value");
    let mut rgb: [u8; 3] = [pc.0, pc.1, pc.2];
    crate::theme::submenu(ui, crate::i18n::tr("More Colors…"), |ui| if let Some(c) = custom_picker(ui, &mut rgb) { pc = c; ui.close(); });
    ui.ctx().data_mut(|d| d.insert_persisted(pick_id, rgb));
    if pc != app.extras.draw.pens[k].color { app.extras.draw.pens[k].color = pc; app.extras.draw.tool = crate::draw::Tool::Pen(k); }
    ui.separator();
    if app.extras.draw.pens.len() > 1 && ui.button(crate::i18n::tr("Remove Pen")).clicked() {
        app.extras.draw.pens.remove(k);
        app.extras.draw.tool = crate::draw::Tool::Select;
        ui.close();
    }
}

fn draw_tab(app: &mut WordApp, ui: &mut Ui) {
    use crate::draw::{PenDef, PenKind, Tool};
    group(ui, "Tools", |ui| {
        let t = app.extras.draw.tool;
        if large(ui, "drawSelect", "Select", t == Tool::Select, true, "Select and type").clicked() { app.extras.draw.tool = Tool::Select; }
        if large(ui, "lassoSelect", "Lasso Select", t == Tool::Lasso, true, "Select ink by drawing around it; drag to move, Delete to remove").clicked() { app.extras.draw.tool = Tool::Lasso; }
        let r = large_menu(ui, "inkEraser", "Eraser", "Erase ink strokes");
        if r.clicked() { app.extras.draw.tool = if t == Tool::Eraser { Tool::Select } else { Tool::Eraser }; }
        if t == Tool::Eraser { ui.painter().rect_stroke(r.rect, crate::theme::RADIUS as f32, Stroke::new(1.5, ACCENT), StrokeKind::Inside); }
        r.context_menu(|ui| { if ui.button(crate::i18n::tr("Erase All Ink")).clicked() { erase_all_ink(app); ui.close(); } });
        for k in 0..app.extras.draw.pens.len() { pen_button(app, ui, k); }
        let r = large_menu(ui, "addPen", "Add Pen", "Add a pen, pencil or highlighter");
        crate::theme::menu(&r, |ui| {
            for (kind, name, color, width) in [(PenKind::Pen, "Pen", Color(0x2E, 0x8B, 0x57), 0.5), (PenKind::Pencil, "Pencil", Color(0x40, 0x40, 0x40), 1.0), (PenKind::Highlighter, "Highlighter", Color(0x00, 0xFF, 0x7F), 6.0)] {
                if ui.button(crate::i18n::tr(&name)).clicked() { app.extras.draw.pens.push(PenDef { kind, color, width }); app.extras.draw.tool = Tool::Pen(app.extras.draw.pens.len() - 1); ui.close(); }
            }
        });
    });
    group(ui, "Touch", |ui| {
        if large(ui, "drawTouch", "Draw with Touch", app.extras.draw.touch, true, "Draw with your finger on a touch screen").clicked() { app.extras.draw.touch = !app.extras.draw.touch; }
        if large(ui, "inkRuler", "Ruler", app.extras.draw.ruler, true, "Draw straight lines").clicked() { app.extras.draw.ruler = !app.extras.draw.ruler; }
    });
    group(ui, "Convert", |ui| {
        if large(ui, "inkToShape", "Ink to Shape", app.extras.draw.ink_to_shape, true, "Turn drawn shapes into shapes").clicked() { app.extras.draw.ink_to_shape = !app.extras.draw.ink_to_shape; }
    });
    group(ui, "Insert", |ui| {
        if large(ui, "drawingCanvas", "Drawing Canvas", false, true, "A canvas to draw on").clicked() { app.insert_drawing_canvas(); }
    });
    group(ui, "Replay", |ui| {
        if large(ui, "inkReplay", "Ink Replay", app.extras.draw.replay.is_some(), true, "Replay your ink strokes").clicked() {
            app.extras.draw.replay = if app.extras.draw.replay.is_some() { None } else { Some(ui.input(|i| i.time)) };
        }
        if large(ui, "hideInk", "Hide Ink", app.extras.draw.hide_ink, true, "Show or hide ink").clicked() { app.extras.draw.hide_ink = !app.extras.draw.hide_ink; }
    });
}

fn erase_all_ink(app: &mut WordApp) {
    loop {
        let pos = { let mut at = None; let mut pos = 0; 'o: for p in &app.doc.paragraphs { for r in &p.runs { if matches!(r.object, Some(Object::Ink(ref i)) if !i.canvas) { at = Some(pos); break 'o; } pos += r.len(); } pos += 1; } at };
        let Some(pos) = pos else { break };
        app.doc.select(pos, pos + 1);
        app.doc.delete(true, false);
    }
}

/// A style-set tile: "Title" and "Heading 1" drawn in the set's look.
fn style_set_tile(ui: &mut Ui, set: &word_engine::design::StyleSet) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(112.0, 92.0), Sense::click());
    ui.painter().rect(rect, crate::theme::RADIUS as f32, c_card(), Stroke::new(1.0, if r.hovered() { ACCENT } else { c_line() }), StrokeKind::Inside);
    let c = |c: Option<Color>| c.map(|c| Color32::from_rgb(c.0, c.1, c.2)).unwrap_or(c_ink());
    let serif = set.title.font.is_some_and(|f| f.contains("Times"));
    let fam = |bold: bool| if serif || bold { FontFamily::Name("ui-bold".into()) } else { FontFamily::Proportional };
    let cap = |t: &str, caps: bool| if caps { t.to_uppercase() } else { t.to_string() };
    let x = if set.title.center { Align2::CENTER_CENTER } else { Align2::LEFT_CENTER };
    let tx = if set.title.center { rect.center().x } else { rect.min.x + 12.0 };
    ui.painter().text(pos2(tx, rect.min.y + 20.0), x, cap("Title", set.title.caps), FontId::new(14.0, fam(set.title.bold)), c(set.title.color));
    ui.painter().text(pos2(rect.min.x + 12.0, rect.min.y + 38.0), Align2::LEFT_CENTER, cap("Heading 1", set.h1.caps), FontId::new(10.0, fam(set.h1.bold || serif)), c(set.h1.color));
    if set.h1.rule { ui.painter().line_segment([pos2(rect.min.x + 12.0, rect.min.y + 46.0), pos2(rect.max.x - 12.0, rect.min.y + 46.0)], Stroke::new(1.0, c(set.h1.color))); }
    for k in 0..2 { let y = rect.min.y + 52.0 + k as f32 * 5.0; ui.painter().line_segment([pos2(rect.min.x + 12.0, y), pos2(rect.max.x - 12.0, y)], Stroke::new(1.5, Color32::from_gray(200))); }
    ui.painter().text(pos2(rect.center().x, rect.max.y - 12.0), Align2::CENTER_CENTER, set.name, FontId::proportional(11.5), c_ink2());
    r.on_hover_text(crate::i18n::trf("Style set: {}", &[&set.name]))
}

fn palette_swatches(ui: &mut Ui, p: &[Color; 6]) {
    let (rect, _) = ui.allocate_exact_size(vec2(72.0, 14.0), Sense::hover());
    for (k, c) in p.iter().enumerate() { ui.painter().rect_filled(Rect::from_min_size(rect.min + vec2(k as f32 * 12.0, 0.0), vec2(11.0, 14.0)), 0.0, Color32::from_rgb(c.0, c.1, c.2)); }
}

fn design_tab(app: &mut WordApp, ui: &mut Ui) {
    use word_engine::design::{COLOR_SETS, FONT_SETS, SPACING, STYLE_SETS, THEMES, WATERMARKS};
    group(ui, "Themes", |ui| {
        let r = large_menu(ui, "themes", "Themes", "Theme colors and fonts");
        crate::theme::menu(&r, |ui| {
            ui.label(RichText::new(crate::i18n::tr("Office")).strong());
            crate::theme::grid(ui, egui::Grid::new("themes").spacing(vec2(6.0, 6.0)), |ui| {
                for (k, (name, colors, fonts)) in THEMES.iter().enumerate() {
                    ui.vertical(|ui| {
                        let (rect, r) = ui.allocate_exact_size(vec2(96.0, 64.0), Sense::click());
                        ui.painter().rect(rect, crate::theme::RADIUS as f32, c_card(), Stroke::new(1.0, if r.hovered() { ACCENT } else { c_line() }), StrokeKind::Inside);
                        ui.painter().text(rect.min + vec2(10.0, 22.0), Align2::LEFT_CENTER, "Aa", FontId::new(22.0, FontFamily::Proportional), c_ink());
                        let p = &COLOR_SETS[*colors].1;
                        for (i, c) in p.iter().enumerate() { ui.painter().rect_filled(Rect::from_min_size(rect.min + vec2(10.0 + i as f32 * 13.0, 44.0), vec2(12.0, 10.0)), 0.0, Color32::from_rgb(c.0, c.1, c.2)); }
                        ui.label(RichText::new(crate::i18n::tr(&*name)).small());
                        if r.clicked() { let (_, h, b) = FONT_SETS[*fonts]; app.doc.apply_fonts(h, b); app.doc.apply_colors(&COLOR_SETS[*colors].1); ui.close(); }
                    });
                    if k % 4 == 3 { ui.end_row(); }
                }
            });
        });
    });
    group(ui, "Document Formatting", |ui| {
        for set in STYLE_SETS.iter().take(5) {
            if style_set_tile(ui, set).clicked() { let k = STYLE_SETS.iter().position(|s| s.name == set.name).unwrap_or(0); app.doc.apply_style_set(k); }
        }
        let (more, _) = large_split(ui, "more", "", "More style sets");
        crate::theme::menu(&more, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(480.0);
                for (k, set) in STYLE_SETS.iter().enumerate() { if style_set_tile(ui, set).clicked() { app.doc.apply_style_set(k); ui.close(); } }
            });
        });
    });
    group(ui, "Colors & Fonts", |ui| {
        let r = large_menu(ui, "colors", "Colors", "Theme colors");
        crate::theme::menu(&r, |ui| {
            for (name, p) in COLOR_SETS {
                let resp = ui.horizontal(|ui| { palette_swatches(ui, p); ui.label(crate::i18n::tr(*name)); }).response.interact(Sense::click());
                if resp.clicked() { app.doc.apply_colors(p); ui.close(); }
            }
        });
        let r = large_menu(ui, "fonts", "Fonts", "Theme fonts");
        crate::theme::menu(&r, |ui| {
            for (name, h, b) in FONT_SETS {
                let face = |n: &str| FontFamily::Name(crate::app::face_name(word_engine::fonts::resolve(n, false, false).face as usize).into());
                let resp = ui.vertical(|ui| {
                    ui.label(RichText::new(crate::i18n::tr(&*name)).small().weak());
                    ui.label(RichText::new(crate::i18n::tr(&*h)).family(face(h)).size(15.0));
                    ui.label(RichText::new(crate::i18n::tr(&*b)).family(face(b)).size(11.0));
                    ui.separator();
                }).response.interact(Sense::click());
                if resp.clicked() { app.doc.apply_fonts(h, b); ui.close(); }
            }
        });
        rows(ui, |ui| {
            let r = small_menu(ui, "paragraphSpacing", "Paragraph Spacing", "Spacing for the whole document");
            crate::theme::menu(&r, |ui| {
                for (name, before, after, line) in SPACING {
                    if ui.button(format!("{name}   ({after:.0} pt after, {line} lines)")).clicked() { app.doc.apply_paragraph_spacing(*before, *after, *line); ui.close(); }
                }
            });
            let r = small_menu(ui, "effects", "Effects", "Shape effects for the whole document");
            crate::theme::menu(&r, |ui| {
                for (name, width) in [("Flat (no outlines)", 0.0), ("Subtle (thin outlines)", 0.5), ("Strong (thick outlines)", 2.25)] {
                    if ui.button(crate::i18n::tr(&name)).clicked() { apply_shape_effect(app, width); ui.close(); }
                }
            });
            if small(ui, "setDefault", Some("Set as Default"), false, true, "Use these settings for new documents").clicked() { let c = ui.ctx().clone(); app.set_as_default(&c); }
        });
    });
    group(ui, "Page Background", |ui| {
        let r = large_menu(ui, "watermark", "Watermark", "Text or a picture behind the page");
        crate::theme::menu(&r, |ui| {
            for diagonal in [true, false] {
                ui.label(RichText::new(if diagonal { "Diagonal" } else { "Horizontal" }).small().strong());
                ui.horizontal(|ui| {
                    for t in WATERMARKS.iter().take(4) {
                        let (rect, resp) = ui.allocate_exact_size(vec2(64.0, 82.0), Sense::click());
                        ui.painter().rect(rect, crate::theme::RADIUS as f32, c_card(), Stroke::new(1.0, if resp.hovered() { ACCENT } else { c_line() }), StrokeKind::Inside);
                        let g = ui.painter().layout_no_wrap(t.to_string(), FontId::proportional(8.0), Color32::from_gray(160));
                        let angle = if diagonal { -std::f32::consts::FRAC_PI_4 } else { 0.0 };
                        let pos = rect.center() - egui::Vec2::angled(angle) * g.size().x / 2.0 - egui::Vec2::angled(angle + std::f32::consts::FRAC_PI_2) * g.size().y / 2.0;
                        ui.painter().add(egui::epaint::TextShape::new(pos, g, Color32::from_gray(160)).with_angle(angle));
                        if resp.on_hover_text(*t).clicked() { app.text_watermark(t, diagonal); ui.close(); }
                    }
                });
            }
            ui.separator();
            if ui.button(crate::i18n::tr("Custom Watermark…")).clicked() { app.open_watermark(); ui.close(); }
            if ui.add_enabled(app.doc.watermark().is_some(), egui::Button::new(crate::i18n::tr("Remove Watermark"))).clicked() { app.doc.set_watermark(None); ui.close(); }
        });
        let r = large_menu(ui, "pageColor", "Page Color", "Color the page background");
        crate::theme::menu(&r, |ui| {
            if let Some(c) = color_menu(ui, "No Color") { app.doc.set_page_color(c); ui.close(); }
        });
        if large(ui, "pageBorders", "Page Borders", false, true, "Add a border around the page").clicked() { app.open_page_borders(); }
    });
}

/// Design ▸ Effects: outline weight for every shape in the document.
fn apply_shape_effect(app: &mut WordApp, width: f32) {
    let mut pos = 0;
    let mut targets = vec![];
    for p in &app.doc.paragraphs { for r in &p.runs { if let Some(Object::Shape(s)) = &r.object { if !s.textbox && !s.name.starts_with("PowerPlus") { targets.push((pos, s.clone())); } } pos += r.len(); } pos += 1; }
    for (at, mut s) in targets {
        if width == 0.0 { s.outline = None; } else { s.outline = Some(s.outline.unwrap_or(Color(0x2F, 0x52, 0x8F))); s.outline_width = width; }
        s.raw = None;
        app.doc.set_object(at, Object::Shape(s));
    }
}

fn header_footer_tab(app: &mut WordApp, ui: &mut Ui) {
    let Some(header) = app.extras.story.as_ref().map(|s| s.header) else { return };
    let ctx = ui.ctx().clone();
    group(ui, "Header & Footer", |ui| {
        let r = large_menu(ui, "header", "Header", "Header designs");
        crate::theme::menu(&r, |ui| story_gallery(app, ui, true));
        let r = large_menu(ui, "footer", "Footer", "Footer designs");
        crate::theme::menu(&r, |ui| story_gallery(app, ui, false));
        let r = large_menu(ui, "pageNumber", "Page Number", "Insert a page number at the caret");
        crate::theme::menu(&r, |ui| {
            if ui.button(crate::i18n::tr("Plain Number")).clicked() { app.doc.insert_field(word_engine::Field::Page); ui.close(); }
            if ui.button(crate::i18n::tr("Page X of Y")).clicked() { app.doc.replace("Page "); app.doc.insert_field(word_engine::Field::Page); app.doc.replace(" of "); app.doc.insert_field(word_engine::Field::NumPages); ui.close(); }
        });
    });
    group(ui, "Insert", |ui| {
        if large(ui, "dateTime", "Date & Time", false, true, "Insert the date or time").clicked() { app.open_date_time(); }
        let r = large_menu(ui, "info", "Document Info", "Author, title, file name");
        crate::theme::menu(&r, |ui| {
            for (name, key) in [("Author", "AUTHOR"), ("Title", "TITLE"), ("File Name", "FILENAME"), ("Subject", "SUBJECT")] {
                if ui.button(crate::i18n::tr(&name)).clicked() { app.doc.insert_field(word_engine::Field::Prop(key.into())); ui.close(); }
            }
        });
        let r = large_menu(ui, "quickParts", "Quick Parts", "AutoText and fields");
        crate::theme::menu(&r, |ui| crate::text_tools::quick_parts_menu(app, ui));
        if large(ui, "pictures", "Pictures", false, true, "Insert a picture").clicked() { app.insert_picture(); }
        if large(ui, "changePicture", "Online Pictures", false, true, "Search pictures online").clicked() { app.open_online_pictures(); }
    });
    group(ui, "Navigation", |ui| {
        if large(ui, "goToHeader", "Go to Header", header, !header, "Edit the header").clicked() { app.go_to_story(true); }
        if large(ui, "goToFooter", "Go to Footer", !header, header, "Edit the footer").clicked() { app.go_to_story(false); }
    });
    group(ui, "Options", |ui| {
        rows(ui, |ui| {
            let mut first = app.extras.story.as_ref().map(|s| s.host.page.title_page).unwrap_or(false);
            if ui.checkbox(&mut first, crate::i18n::tr("Different First Page")).changed() { if let Some(st) = &mut app.extras.story { st.host.checkpoint(word_engine::model::EditKind::Other); st.host.page.title_page = first; st.host.dirty = true; } }
        });
    });
    group(ui, "Position", |ui| {
        rows(ui, |ui| {
            if let Some(st) = &mut app.extras.story {
                let (mut h, mut f) = (st.host.page.header as f32 / 1440.0, st.host.page.footer as f32 / 1440.0);
                let mut changed = false;
                ui.horizontal(|ui| { paint_label_icon(ui, "header", "Header from Top:"); changed |= ui.add(egui::DragValue::new(&mut h).range(0.0..=3.0).speed(0.01).max_decimals(2).suffix("\"")).changed(); });
                ui.horizontal(|ui| { paint_label_icon(ui, "footer", "Footer from Bottom:"); changed |= ui.add(egui::DragValue::new(&mut f).range(0.0..=3.0).speed(0.01).max_decimals(2).suffix("\"")).changed(); });
                if changed {
                    st.host.page.header = (h * 1440.0).round() as i32;
                    st.host.page.footer = (f * 1440.0).round() as i32;
                    st.host.dirty = true;
                    if st.header { app.doc.page.top = st.host.page.header; }
                    app.doc.format_epoch += 1;
                    app.layout = Default::default();
                }
            }
        });
    });
    group(ui, "Close", |ui| {
        if large(ui, "closeView", "Close Header and Footer", false, true, "Back to the document (Esc)").clicked() { app.end_story(); }
    });
    let _ = ctx;
}

fn equation_tab(app: &mut WordApp, ui: &mut Ui) {
    group(ui, "Tools", |ui| {
        let r = large_menu(ui, "equation", "Equation", "Built-in equations");
        crate::theme::menu(&r, |ui| crate::symbols::equation_menu(app, ui));
        if large(ui, "editShape", "Edit", false, true, "Edit the equation (double-click)").clicked() { app.open_equation(String::new()); }
    });
    group(ui, "Symbols", |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
            let syms: Vec<char> = crate::symbols::MATH_SYMBOLS.chars().collect();
            for row in syms.chunks(14).take(3) {
                ui.horizontal(|ui| {
                    for c in row {
                        let (rect, r) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                        if r.hovered() { ui.painter().rect_filled(rect, crate::theme::RADIUS as f32, c_hover()); }
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, c.to_string(), crate::symbols::glyph_font(*c, 15.0), c_ink());
                        if r.clicked() { app.open_equation(c.to_string()); }
                    }
                });
            }
        });
    });
    group(ui, "Structures", |ui| {
        for (name, _, items) in crate::symbols::STRUCTURES {
            let r = structure_button(app, ui, name, items[0].1);
            crate::theme::menu(&r, |ui| {
                for (label, src) in *items {
                    let eq = word_engine::equation::Equation::new(src);
                    let (w, h, _) = eq.metrics();
                    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(60.0) + 16.0, h + 10.0), Sense::click());
                    if resp.hovered() { ui.painter().rect_filled(rect, crate::theme::RADIUS as f32, c_hover()); }
                    crate::canvas::paint_preview(app, ui, &eq.render(0.0, 0.0), rect.min + vec2(8.0, 5.0), 1.0);
                    if resp.on_hover_text(crate::i18n::tr(&*label)).clicked() { app.open_equation(src.to_string()); ui.close(); }
                }
            });
        }
    });
}

/// A Structures button drawn with a preview of its first template, like Word's.
fn structure_button(app: &mut WordApp, ui: &mut Ui, name: &str, src: &str) -> Response {
    let g = ui.painter().layout(name.to_string(), FontId::proportional(11.5), c_ink2(), 70.0);
    let w = g.size().x.max(40.0) + 16.0;
    let (rect, r) = ui.allocate_exact_size(vec2(w, 70.0), Sense::click());
    button_bg(ui, rect, &r, false);
    let mut eq = word_engine::equation::Equation::new(src);
    eq.size = 10.0;
    let (ew, eh, _) = eq.metrics();
    let scale = (30.0 / eh).min((w - 8.0) / ew).min(1.2);
    let at = pos2(rect.center().x - ew * scale / 2.0, rect.min.y + 20.0 - eh * scale / 2.0);
    crate::canvas::paint_preview(app, ui, &eq.render(0.0, 0.0), at, scale);
    let text_at = pos2(rect.center().x - g.size().x / 2.0, rect.min.y + 39.0);
    let last = g.rows.last().map(|row| row.rect()).unwrap_or(Rect::NOTHING);
    ui.painter().galley(text_at, g, c_ink2());
    caret(ui, pos2(text_at.x + last.max.x + 6.0, text_at.y + last.center().y));
    r.on_hover_text(crate::i18n::tr(&name))
}

/// The custom tab from Options ▸ Customize Ribbon: its commands as large buttons.
fn custom_tab(app: &mut WordApp, ui: &mut Ui, ctx: &egui::Context) {
    let all = crate::commands::all();
    let names = app.extras.options.settings.custom_tab.clone();
    let mut run = None;
    group(ui, "Custom", |ui| {
        for n in &names {
            if let Some(c) = all.iter().find(|c| c.name == *n) {
                if large(ui, c.icon, c.name, false, true, c.name).clicked() { run = Some(c.run); }
            }
        }
    });
    if let Some(f) = run { f(app, ctx); }
}
