//! The ribbon (ui/ribbon.js), drawn with Qalam's components: File, the tabs and quick access,
//! and one card of command groups for the active tab.

use crate::app::{Dialog, SheetApp, Tab};
use crate::i18n::tr;
use egui::{pos2, vec2, FontFamily, FontId, Rect, RichText, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use sheet_engine::book::BorderKind;
use sheet_engine::style::{Border, HAlign, LineStyle, Underline, VAlign};

const GROUP_H: f32 = 76.0;

pub fn ribbon(app: &mut SheetApp, ui: &mut Ui) {
    egui::Panel::top("ribbon").show_separator_line(false).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 6 })).show(ui, |ui| {
        let mut active = Rect::NOTHING;
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 30.0), row_layout(), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let file = ui.add(egui::Button::new(RichText::new(tr("File")).size(14.0).color(c_ink()).family(FontFamily::Name("ui-bold".into()))).frame(false).min_size(vec2(44.0, 30.0)));
            crate::theme::menu(&file, |ui| crate::filemenu::file_menu(app, ui));
            for (tab, label) in [(Tab::Home, "Home"), (Tab::Insert, "Insert"), (Tab::PageLayout, "Page Layout"), (Tab::Formulas, "Formulas"), (Tab::Data, "Data"), (Tab::Review, "Review"), (Tab::View, "View")].into_iter().chain(crate::table_tab::contextual(app)).chain(crate::insert_tab::contextual(app)).chain(crate::page_view::contextual(app)) {
                let label = tr(label);
                let selected = app.tab == tab;
                let font = FontId::new(14.0, if selected { FontFamily::Name("ui-bold".into()) } else { FontFamily::Proportional });
                let g = ui.painter().layout_no_wrap(label, font, if selected { crate::grid::SELECT } else { c_ink2() });
                let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 22.0, 30.0), Sense::click());
                if selected {
                    let t = Rect::from_min_max(rect.min + vec2(0.0, 2.0), rect.max + vec2(0.0, 1.0));
                    ui.painter().rect_filled(t, 0.0, c_card());
                    let s = Stroke::new(1.0, c_line());
                    ui.painter().line_segment([t.left_bottom(), t.left_top()], s);
                    ui.painter().line_segment([t.left_top(), t.right_top()], s);
                    ui.painter().line_segment([t.right_top(), t.right_bottom()], s);
                    active = t;
                } else if r.hovered() { ui.painter().rect_filled(rect.shrink2(vec2(0.0, 3.0)), 0.0, c_hover()); }
                ui.painter().galley(pos2(rect.center().x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0), g, c_ink());
                if r.clicked() { app.tab = tab; }
            }
            ui.with_layout(far_layout(), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                crate::view_tab::quick_access(app, ui);
            });
        });
        let painter = ui.painter().clone();
        egui::Frame::NONE.fill(c_card()).inner_margin(egui::Margin::symmetric(8, 2)).stroke(Stroke::new(1.0, c_line())).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let full_id = egui::Id::new(("ribbon-full-width", app.tab as usize));
            let full = ui.data(|d| d.get_temp::<f32>(full_id)).unwrap_or(0.0);
            let is_compact = full > ui.available_width();
            qalam_kit::widgets::COMPACT.with(|c| c.set(is_compact));
            // Excel shows each group's name under it.
            qalam_kit::widgets::CAPTIONS.with(|c| c.set(true));
            let out = egui::ScrollArea::horizontal().id_salt("ribbon-scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded).show(ui, |ui| {
                ui.with_layout(row_layout(), |ui| {
                    ui.set_height(GROUP_H + 6.0 + if is_compact { 0.0 } else { qalam_kit::widgets::CAPTION_H });
                    match app.tab {
                        Tab::Home => home(app, ui),
                        Tab::Insert => insert(app, ui),
                        Tab::PageLayout => page_layout(app, ui),
                        Tab::Formulas => formulas(app, ui),
                        Tab::Data => data(app, ui),
                        Tab::Review => review(app, ui),
                        Tab::View => view(app, ui),
                        Tab::TableDesign => crate::table_tab::table_design(app, ui),
                        Tab::ChartDesign | Tab::ObjectFormat | Tab::PivotAnalyze | Tab::PivotDesign | Tab::Sparkline => crate::insert_tab::contextual_body(app, ui),
                        Tab::HeaderFooter => crate::page_view::hf_tab(app, ui),
                    }
                });
            });
            qalam_kit::widgets::COMPACT.with(|c| c.set(false));
            qalam_kit::widgets::CAPTIONS.with(|c| c.set(false));
            if !is_compact { ui.data_mut(|d| d.insert_temp(full_id, out.content_size.x)); }
        });
        if active.is_positive() {
            painter.rect_filled(Rect::from_min_max(pos2(active.min.x + 1.0, active.max.y - 2.0), pos2(active.max.x - 1.0, active.max.y + 1.5)), 0.0, c_card());
        }
    });
}


const SIZES: [f32; 16] = [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0];

fn home(app: &mut SheetApp, ui: &mut Ui) {
    let st = app.style();
    group(ui, "Clipboard", |ui| {
        let (main, arrow) = large_split(ui, "paste", "Paste", "Paste (⌘V)");
        if main.clicked() { app.paste(); }
        crate::theme::menu(&arrow, |ui| {
            use sheet_engine::book::{Paste, PasteWhat};
            menu_heading(ui, "Paste");
            for (icon, label, how) in [("paste", "Paste", Paste::default()), ("formula", "Formulas", Paste { what: PasteWhat::Formulas, ..Default::default() }), ("borderNone", "No Borders", Paste { what: PasteWhat::NoBorders, ..Default::default() }), ("columnWidth", "Keep Source Column Widths", Paste { widths: true, ..Default::default() }), ("rotate", "Transpose", Paste { transpose: true, ..Default::default() })] {
                if menu_item(ui, icon, label) { app.paste_with(how); ui.close(); }
            }
            menu_heading(ui, "Paste Values");
            if menu_item(ui, "pasteText", "Values") { app.paste_with(Paste { what: PasteWhat::Values, ..Default::default() }); ui.close(); }
            menu_heading(ui, "Other Paste Options");
            if menu_item(ui, "formatPainter", "Formatting") { app.paste_with(Paste { what: PasteWhat::Formats, ..Default::default() }); ui.close(); }
            ui.separator();
            if menu_item(ui, "pasteText", "Paste Special…") { app.dialog = Some(Dialog::PasteSpecial { how: Default::default(), op: 0 }); ui.close(); }
        });
        rows(ui, |ui| {
            if small(ui, "cut", Some("Cut"), false, true, "Cut (⌘X)").clicked() { app.cut(); }
            let (c, ca) = small_split_label(ui, "copy", Some("Copy"), false, "Copy (⌘C)");
            if c.clicked() { app.copy(); }
            crate::theme::menu(&ca, |ui| {
                if menu_item(ui, "copy", "Copy") { app.copy(); ui.close(); }
                if menu_item(ui, "picture", "Copy as Text") { app.copy(); ui.close(); }
            });
            let on = app.painter.is_some();
            let r = small(ui, "formatPainter", Some("Format Painter"), on, true, "Format Painter: click to paint once, double-click to keep painting");
            if r.clicked() || r.double_clicked() {
                if on && !r.double_clicked() { app.painter = None; } else { let c = app.book.copy(app.sheet(), app.sel.range()); app.painter = Some((c, r.double_clicked())); }
            }
        });
    });
    let font_launch = group_l(ui, "Font", "Format Cells: Font (⌘1)", |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                crate::theme::Combo::from_id_salt("rb-font").width(132.0).height(400.0).selected_text(RichText::new(&st.font).size(12.5)).show_ui(ui, |ui| {
                    menu_heading(ui, "Theme Fonts");
                    for (f, k, tag) in [(app.book.theme.major.clone(), 2u8, "(Headings)"), (app.book.theme.minor.clone(), 1u8, "(Body)")] { let shown = [f.as_str(), tr(tag).as_str()].join("  "); if ui.selectable_label(st.font == f, RichText::new(shown).size(13.0)).clicked() { app.apply_style("Font", move |s| { s.font = f.clone(); s.font_scheme = k; }); } }
                    menu_heading(ui, "All Fonts");
                    for name in word_engine::fonts::family_names() {
                        if ui.selectable_label(st.font.eq_ignore_ascii_case(&name), RichText::new(&name).size(13.0)).clicked() { let n = name.clone(); app.apply_style("Font", move |s| s.font = n.clone()); }
                    }
                });
                let size_id = egui::Id::new("rb-size");
                let mut text: String = ui.data(|d| d.get_temp(size_id)).unwrap_or_else(|| format!("{}", st.size_pt()));
                if !ui.memory(|m| m.has_focus(size_id)) { text = format!("{}", st.size_pt()); }
                let r = ui.add(egui::TextEdit::singleline(&mut text).id(size_id).desired_width(30.0).font(FontId::proportional(12.5)));
                ui.data_mut(|d| d.insert_temp(size_id, text.clone()));
                if r.lost_focus() { if let Ok(v) = text.trim().parse::<f32>() { if (1.0..=409.0).contains(&v) { app.apply_style("Font Size", move |s| s.size = (v * 10.0).round() as u32); } } }
                let arrow = ui.add(egui::Button::new("▾").frame(false));
                crate::theme::menu(&arrow, |ui| for s in SIZES { if ui.selectable_label(st.size_pt() == s, format!("{s}")).clicked() { app.apply_style("Font Size", move |x| x.size = (s * 10.0) as u32); ui.close(); } });
                if small(ui, "fontGrow", None, false, true, "Increase Font Size (⇧⌘>)").clicked() { let n = SIZES.iter().copied().find(|x| *x > st.size_pt()).unwrap_or(st.size_pt() + 2.0); app.apply_style("Font Size", move |s| s.size = (n * 10.0) as u32); }
                if small(ui, "fontShrink", None, false, true, "Decrease Font Size (⇧⌘<)").clicked() { let n = SIZES.iter().rev().copied().find(|x| *x < st.size_pt()).unwrap_or(1.0); app.apply_style("Font Size", move |s| s.size = (n * 10.0) as u32); }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if small(ui, "bold", None, st.bold, true, "Bold (⌘B)").clicked() { let b = !st.bold; app.apply_style("Bold", move |s| s.bold = b); }
                if small(ui, "italic", None, st.italic, true, "Italic (⌘I)").clicked() { let b = !st.italic; app.apply_style("Italic", move |s| s.italic = b); }
                let (u, ua) = small_split(ui, "underline", st.underline != Underline::None, "Underline (⌘U)");
                if u.clicked() { let on = st.underline == Underline::None; app.apply_style("Underline", move |s| s.underline = if on { Underline::Single } else { Underline::None }); }
                crate::theme::menu(&ua, |ui| {
                    if menu_item(ui, "underline", "Underline") { app.apply_style("Underline", |s| s.underline = Underline::Single); ui.close(); }
                    if menu_item(ui, "underline", "Double Underline") { app.apply_style("Underline", |s| s.underline = Underline::Double); ui.close(); }
                });
                ui.add_space(4.0);
                borders_button(app, ui);
                ui.add_space(4.0);
                let last_fill = ui.data(|d| d.get_temp::<Option<[u8; 3]>>(egui::Id::new("last-fill"))).unwrap_or(Some([255, 255, 0]));
                let (f, fa) = color_split(ui, "shading", last_fill.map(crate::app::rgb).unwrap_or(egui::Color32::TRANSPARENT), "Fill Color");
                if f.clicked() { app.apply_style("Fill Color", move |s| s.fill = last_fill); }
                crate::theme::menu(&fa, |ui| { let th = app.book.theme_colors.clone(); if let Some(c) = crate::themes_ui::color_menu(ui, "No Fill", &th) { let (c, t) = (c.map(|x| x.0), c.and_then(|x| x.1)); ui.data_mut(|d| d.insert_temp(egui::Id::new("last-fill"), c)); app.apply_style("Fill Color", move |s| { s.fill = c; s.fill_th = t; }); ui.close(); } });
                let last_color = ui.data(|d| d.get_temp::<Option<[u8; 3]>>(egui::Id::new("last-color"))).unwrap_or(Some([0xC0, 0, 0]));
                let (t, ta) = color_split(ui, "textColor", last_color.map(crate::app::rgb).unwrap_or(egui::Color32::BLACK), "Font Color");
                if t.clicked() { app.apply_style("Font Color", move |s| s.color = last_color); }
                crate::theme::menu(&ta, |ui| { let th = app.book.theme_colors.clone(); if let Some(c) = crate::themes_ui::color_menu(ui, "Automatic", &th) { let (c, t) = (c.map(|x| x.0), c.and_then(|x| x.1)); ui.data_mut(|d| d.insert_temp(egui::Id::new("last-color"), c)); app.apply_style("Font Color", move |s| { s.color = c; s.color_th = t; }); ui.close(); } });
            });
        });
    });
    if font_launch { crate::dialogs::open_format_cells(app, 2); }
    let align_launch = group_l(ui, "Alignment", "Format Cells: Alignment", |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (icon, v, tip) in [("alignTop", VAlign::Top, "Top Align"), ("alignMiddle", VAlign::Center, "Middle Align"), ("alignBottom", VAlign::Bottom, "Bottom Align")] {
                    if small(ui, icon, None, st.v == v, true, tip).clicked() { app.apply_style("Vertical Alignment", move |s| s.v = v); }
                }
                ui.add_space(4.0);
                let (o, oa) = small_split(ui, "orientation", st.rotation != 0, "Orientation");
                if o.clicked() { let r = if st.rotation == 0 { 45 } else { 0 }; app.apply_style("Orientation", move |s| s.rotation = r); }
                crate::theme::menu(&oa, |ui| {
                    for (label, deg) in [("Angle Counterclockwise", 45i16), ("Angle Clockwise", -45), ("Vertical Text", 255), ("Rotate Text Up", 90), ("Rotate Text Down", -90)] {
                        if ui.selectable_label(st.rotation == deg, tr(label)).clicked() { let d = if st.rotation == deg { 0 } else { deg }; app.apply_style("Orientation", move |s| s.rotation = d); ui.close(); }
                    }
                    ui.separator();
                    if ui.button(tr("Format Cell Alignment")).clicked() { crate::dialogs::open_format_cells(app, 1); ui.close(); }
                });
                ui.add_space(4.0);
                if small(ui, "wrapText", Some("Wrap Text"), st.wrap, true, "Wrap Text").clicked() { let w = !st.wrap; app.apply_style("Wrap Text", move |s| s.wrap = w); }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (icon, h, tip) in [("alignLeft", HAlign::Left, "Align Left"), ("alignCenter", HAlign::Center, "Center"), ("alignRight", HAlign::Right, "Align Right")] {
                    if small(ui, icon, None, st.h == h, true, tip).clicked() { let h2 = if st.h == h { HAlign::General } else { h }; app.apply_style("Alignment", move |s| s.h = h2); }
                }
                ui.add_space(4.0);
                if small(ui, "outdent", None, false, st.indent > 0, "Decrease Indent").clicked() { app.apply_style("Indent", |s| s.indent = s.indent.saturating_sub(1)); }
                if small(ui, "indent", None, false, true, "Increase Indent").clicked() { app.apply_style("Indent", |s| { s.indent = (s.indent + 1).min(15); if matches!(s.h, HAlign::General | HAlign::Center) { s.h = HAlign::Left; } }); }
                ui.add_space(4.0);
                let merged = app.book.sheets[app.sheet()].merge_at(app.sel.active).is_some();
                let m = small(ui, "mergeCenter", Some("Merge & Center"), merged, true, "Merge & Center");
                let (ar, aresp) = ui.allocate_exact_size(vec2(12.0, 24.0), Sense::click());
                button_bg(ui, ar, &aresp, false); caret(ui, ar.center());
                if m.clicked() { app.merge(if merged { 3 } else { 0 }); }
                crate::theme::menu(&aresp, |ui| {
                    for (label, k) in [("Merge & Center", 0u8), ("Merge Across", 1), ("Merge Cells", 2), ("Unmerge Cells", 3)] { if menu_item(ui, "mergeCenter", label) { app.merge(k); ui.close(); } }
                });
            });
        });
    });
    if align_launch { crate::dialogs::open_format_cells(app, 1); }
    let num_launch = group_l(ui, "Number", "Format Cells: Number", |ui| {
        rows(ui, |ui| {
            let cat = category_of(&st.num_fmt);
            crate::theme::Combo::from_id_salt("rb-numfmt").width(150.0).height(460.0).selected_text(RichText::new(tr(cat)).size(12.5)).show_ui(ui, |ui| {
                let v = app.book.sheets[app.sheet()].value(app.sel.active);
                for (label, fmt) in [("General", "General"), ("Number", "0.00"), ("Currency", "\"$\"#,##0.00"), ("Accounting", ACCOUNTING), ("Short Date", "m/d/yyyy"), ("Long Date", "dddd, mmmm d, yyyy"), ("Time", "h:mm:ss AM/PM"), ("Percentage", "0.00%"), ("Fraction", "# ?/?"), ("Scientific", "0.00E+00"), ("Text", "@")] {
                    let sample = sheet_engine::numfmt::format(&v, fmt, app.book.date1904).text;
                    let r = ui.selectable_label(st.num_fmt == fmt, RichText::new(tr(label)).size(13.0));
                    ui.painter().text(r.rect.right_center() - vec2(4.0, 0.0), egui::Align2::RIGHT_CENTER, sample, FontId::proportional(11.0), c_ink3());
                    if r.clicked() { let f = fmt.to_string(); app.apply_style("Number Format", move |s| s.num_fmt = f.clone()); }
                }
                ui.separator();
                if ui.button(tr("More Number Formats…")).clicked() { crate::dialogs::open_format_cells(app, 0); ui.close(); }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let (a, aa) = small_split(ui, "accountingFormat", false, "Accounting Number Format");
                if a.clicked() { app.apply_style("Number Format", |s| s.num_fmt = ACCOUNTING.into()); }
                crate::theme::menu(&aa, |ui| {
                    for (label, f) in [("$ English (United States)", ACCOUNTING), ("£ English (United Kingdom)", r#"_-[$£-809]* #,##0.00_-;-[$£-809]* #,##0.00_-;_-[$£-809]* "-"??_-;_-@_-"#), ("€ Euro (€ 123)", r#"_-* #,##0.00 [$€-x-euro2]_-;-* #,##0.00 [$€-x-euro2]_-;_-* "-"?? [$€-x-euro2]_-;_-@_-"#), ("¥ Chinese (PRC)", r#"_ [$¥-804]* #,##0.00_ ;_ [$¥-804]* -#,##0.00_ ;_ [$¥-804]* "-"??_ ;_ @_ "#), ("د.إ.‏ Arabic (U.A.E.)", r#"_-* #,##0.00 [$د.إ.‏-3801]_-;-* #,##0.00 [$د.إ.‏-3801]_-;_-* "-"?? [$د.إ.‏-3801]_-;_-@_-"#)] {
                        if ui.button(label).clicked() { let f = f.to_string(); app.apply_style("Number Format", move |s| s.num_fmt = f.clone()); ui.close(); }
                    }
                    ui.separator();
                    if ui.button(tr("More Accounting Formats…")).clicked() { crate::dialogs::open_format_cells(app, 0); ui.close(); }
                });
                if small(ui, "percentStyle", None, false, true, "Percent Style (⇧⌘%)").clicked() { app.apply_style("Number Format", |s| s.num_fmt = "0%".into()); }
                if small(ui, "commaStyle", None, false, true, "Comma Style").clicked() { app.apply_style("Number Format", |s| s.num_fmt = r#"_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)"#.into()); }
                ui.add_space(4.0);
                if small(ui, "increaseDecimal", None, false, true, "Increase Decimal").clicked() { let v = app.book.sheets[app.sheet()].value(app.sel.active); app.apply_style("Number Format", move |s| s.num_fmt = decimals(&s.num_fmt, 1, &v)); }
                if small(ui, "decreaseDecimal", None, false, true, "Decrease Decimal").clicked() { let v = app.book.sheets[app.sheet()].value(app.sel.active); app.apply_style("Number Format", move |s| s.num_fmt = decimals(&s.num_fmt, -1, &v)); }
            });
        });
    });
    if num_launch { crate::dialogs::open_format_cells(app, 0); }
    group(ui, "Styles", |ui| {
        let r = large_menu(ui, "conditionalFormatting", "Conditional Formatting", "Highlight cells, data bars, color scales and icon sets");
        crate::theme::menu(&r, |ui| crate::home_hooks::cond_format_menu(app, ui));
        let r = large_menu(ui, "formatAsTable", "Format as Table", "Turn the range into a table with a style");
        crate::theme::menu(&r, |ui| crate::home_hooks::format_as_table_menu(app, ui));
        let r = large_menu(ui, "cellStyles", "Cell Styles", "Apply a ready-made cell style");
        crate::theme::menu(&r, |ui| cell_styles_menu(app, ui));
    });
    group(ui, "Cells", |ui| {
        let r = large_menu(ui, "insertCells", "Insert", "Insert cells, rows, columns or sheets");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "insertCells", "Insert Cells…") { app.dialog = Some(Dialog::Insert { delete: false, choice: 1 }); ui.close(); }
            if menu_item(ui, "insertAbove", "Insert Sheet Rows") { app.insert_rows_cols(true, false); ui.close(); }
            if menu_item(ui, "insertLeft", "Insert Sheet Columns") { app.insert_rows_cols(false, false); ui.close(); }
            if menu_item(ui, "newSheet", "Insert Sheet") { crate::tabs::add_sheet(app, app.sheet()); ui.close(); }
        });
        let r = large_menu(ui, "deleteCells", "Delete", "Delete cells, rows, columns or the sheet");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "deleteCells", "Delete Cells…") { app.dialog = Some(Dialog::Insert { delete: true, choice: 1 }); ui.close(); }
            if menu_item(ui, "deleteRow", "Delete Sheet Rows") { app.insert_rows_cols(true, true); ui.close(); }
            if menu_item(ui, "deleteColumn", "Delete Sheet Columns") { app.insert_rows_cols(false, true); ui.close(); }
            if menu_item(ui, "delete", "Delete Sheet") { let i = app.sheet(); app.checkpoint("Delete Sheet"); match app.book.delete_sheet(i) { Ok(()) => { let a = app.book.active; app.set_sheet(a); app.changed(); } Err(e) => { app.undo(); app.error = Some(tr(&e)); } } ui.close(); }
        });
        let r = large_menu(ui, "formatCells", "Format", "Row height, column width, visibility and sheets");
        crate::theme::menu(&r, |ui| format_menu(app, ui));
    });
    group(ui, "Editing", |ui| {
        rows(ui, |ui| {
            let (s, sa) = small_split_label(ui, "autoSum", Some("AutoSum"), false, "AutoSum (⌥=)");
            if s.clicked() { app.autosum("SUM"); }
            crate::theme::menu(&sa, |ui| {
                for (f, label) in [("SUM", "Sum"), ("AVERAGE", "Average"), ("COUNT", "Count Numbers"), ("MAX", "Max"), ("MIN", "Min")] { if ui.button(tr(label)).clicked() { app.autosum(f); ui.close(); } }
                ui.separator();
                if ui.button(tr("More Functions…")).clicked() { app.dialog = Some(Dialog::Function { query: String::new(), picked: None }); ui.close(); }
            });
            let r = small_menu(ui, "fillDown", "Fill", "Fill cells down, right, up or left, or with a series");
            crate::theme::menu(&r, |ui| {
                if menu_item(ui, "moveDown", "Down") { app.fill_dir(0); ui.close(); }
                if menu_item(ui, "nextArrow", "Right") { app.fill_dir(1); ui.close(); }
                if menu_item(ui, "moveUp", "Up") { app.fill_dir(2); ui.close(); }
                if menu_item(ui, "previousArrow", "Left") { app.fill_dir(3); ui.close(); }
                if menu_item(ui, "copy", "Across Worksheets…") { fill_across(app); ui.close(); }
                if menu_item(ui, "numbers", "Series…") { app.dialog = Some(Dialog::Series { rows: app.sel.range().cols() > app.sel.range().rows(), kind: 0, step: 1.0, stop: String::new(), unit: 0, trend: false }); ui.close(); }
                if menu_item(ui, "alignJustify", "Justify") { justify(app); ui.close(); }
                if menu_item(ui, "fillDown", "Flash Fill") { flash_fill(app); ui.close(); }
            });
            let r = small_menu(ui, "clear", "Clear", "Clear contents, formats, notes or all");
            crate::theme::menu(&r, |ui| {
                if menu_item(ui, "clear", "Clear All") { app.clear(true, true, true); ui.close(); }
                if menu_item(ui, "clearFormat", "Clear Formats") { app.clear(false, true, false); ui.close(); }
                if menu_item(ui, "clear", "Clear Contents") { app.clear(true, false, false); ui.close(); }
                if menu_item(ui, "deleteComment", "Clear Comments and Notes") { app.checkpoint("Clear Notes"); let s = app.sheet(); let rs = app.sel.ranges(); app.book.sheets[s].notes.retain(|p, _| !rs.iter().any(|r| r.contains(*p))); app.dirty = true; ui.close(); }
                if menu_item(ui, "unlink", "Clear Hyperlinks") { app.checkpoint("Clear Hyperlinks"); let s = app.sheet(); let rs = app.sel.ranges(); app.book.sheets[s].links.retain(|p, _| !rs.iter().any(|r| r.contains(*p))); app.dirty = true; ui.close(); }
                if menu_item(ui, "unlink", "Remove Hyperlinks") { app.checkpoint("Remove Hyperlinks"); let s = app.sheet(); let rs = app.sel.ranges(); app.book.sheets[s].links.retain(|p, _| !rs.iter().any(|r| r.contains(*p))); app.book.apply_style(s, &rs, &|st| { st.underline = Underline::None; st.color = None; }); app.dirty = true; ui.close(); }
            });
        });
        let r = large_menu(ui, "sortFilter", "Sort & Filter", "Sort and filter the data");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "sort", "Sort A to Z") { app.sort(true); ui.close(); }
            if menu_item(ui, "sort", "Sort Z to A") { app.sort(false); ui.close(); }
            if menu_item(ui, "sort", "Custom Sort…") { crate::dialogs::open_sort(app); ui.close(); }
            ui.separator();
            if menu_item(ui, "filter", "Filter") { crate::home_hooks::toggle_filter(app); ui.close(); }
            if menu_item(ui, "clearFilter", "Clear") { crate::home_hooks::clear_filters(app); ui.close(); }
            if menu_item(ui, "reapply", "Reapply") { crate::home_hooks::reapply_filters(app); ui.close(); }
        });
        let r = large_menu(ui, "findSelect", "Find & Select", "Find, replace and go to");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "find", "Find…") { crate::dialogs::open_find(app, false); ui.close(); }
            if menu_item(ui, "replace", "Replace…") { crate::dialogs::open_find(app, true); ui.close(); }
            if menu_item(ui, "goTo", "Go To…") { app.dialog = Some(Dialog::GoTo { text: String::new() }); ui.close(); }
            ui.separator();
            for (label, kind) in [("Formulas", 0u8), ("Notes", 1), ("Constants", 2), ("Blanks", 3), ("Conditional Formatting", 4), ("Data Validation", 5)] { if ui.button(tr(label)).clicked() { select_special(app, kind); ui.close(); } }
            ui.separator();
            if ui.selectable_label(crate::objects::state(&app.ctx).select_mode, tr("Select Objects")).clicked() { crate::home_hooks::select_objects(app); ui.close(); }
            if menu_item(ui, "selectionPane", "Selection Pane…") { crate::home_hooks::selection_pane(app); ui.close(); }
        });
    });
}

const ACCOUNTING: &str = r#"_("$"* #,##0.00_);_("$"* \(#,##0.00\);_("$"* "-"??_);_(@_)"#;

/// The built-in cell styles (Good, Bad and Neutral; Data and Model; Titles and Headings;
/// Themed Cell Styles; Number Format), as Excel's gallery offers them.
fn cell_styles_menu(app: &mut SheetApp, ui: &mut Ui) {
    use sheet_engine::style::Style;
    ui.set_width(520.0);
    let mut accents = [[68u8, 114, 196]; 6];
    for (i, a) in accents.iter_mut().enumerate() { if let Some(c) = app.book.theme_colors.get(4 + i) { *a = *c; } }
    let acc = move |i: usize| accents[i % 6];
    let tint = |c: [u8; 3], t: f32| [(c[0] as f32 + (255.0 - c[0] as f32) * t) as u8, (c[1] as f32 + (255.0 - c[1] as f32) * t) as u8, (c[2] as f32 + (255.0 - c[2] as f32) * t) as u8];
    type F = Box<dyn Fn(&mut Style)>;
    let line = |c: Option<[u8; 3]>, s: LineStyle| Border { style: s, color: c };
    let mut sections: Vec<(&str, Vec<(String, F)>)> = vec![
        ("Good, Bad and Neutral", vec![
            ("Normal".into(), Box::new(|s: &mut Style| *s = Style::default()) as F),
            ("Bad".into(), Box::new(|s: &mut Style| { s.fill = Some([0xFF, 0xC7, 0xCE]); s.color = Some([0x9C, 0x00, 0x06]); })),
            ("Good".into(), Box::new(|s: &mut Style| { s.fill = Some([0xC6, 0xEF, 0xCE]); s.color = Some([0x00, 0x61, 0x00]); })),
            ("Neutral".into(), Box::new(|s: &mut Style| { s.fill = Some([0xFF, 0xEB, 0x9C]); s.color = Some([0x9C, 0x57, 0x00]); })),
        ]),
        ("Data and Model", vec![
            ("Calculation".into(), Box::new(move |s: &mut Style| { s.fill = Some([0xF2, 0xF2, 0xF2]); s.color = Some([0xFA, 0x7D, 0x00]); s.bold = true; let b = line(Some([0x7F, 0x7F, 0x7F]), LineStyle::Thin); s.left = b; s.right = b; s.top = b; s.bottom = b; })),
            ("Check Cell".into(), Box::new(move |s: &mut Style| { s.fill = Some([0xA5, 0xA5, 0xA5]); s.color = Some([255, 255, 255]); s.bold = true; let b = line(Some([0x3F, 0x3F, 0x3F]), LineStyle::Double); s.left = b; s.right = b; s.top = b; s.bottom = b; })),
            ("Explanatory Text".into(), Box::new(|s: &mut Style| { s.italic = true; s.color = Some([0x7F, 0x7F, 0x7F]); })),
            ("Input".into(), Box::new(move |s: &mut Style| { s.fill = Some([0xFF, 0xCC, 0x99]); s.color = Some([0x3F, 0x3F, 0x76]); let b = line(Some([0x7F, 0x7F, 0x7F]), LineStyle::Thin); s.left = b; s.right = b; s.top = b; s.bottom = b; })),
            ("Linked Cell".into(), Box::new(move |s: &mut Style| { s.color = Some([0xFA, 0x7D, 0x00]); s.bottom = line(Some([0xFF, 0x80, 0x01]), LineStyle::Double); })),
            ("Note".into(), Box::new(move |s: &mut Style| { s.fill = Some([0xFF, 0xFF, 0xCC]); let b = line(Some([0xB2, 0xB2, 0xB2]), LineStyle::Thin); s.left = b; s.right = b; s.top = b; s.bottom = b; })),
            ("Output".into(), Box::new(move |s: &mut Style| { s.fill = Some([0xF2, 0xF2, 0xF2]); s.color = Some([0x3F, 0x3F, 0x3F]); s.bold = true; let b = line(Some([0x3F, 0x3F, 0x3F]), LineStyle::Thin); s.left = b; s.right = b; s.top = b; s.bottom = b; })),
            ("Warning Text".into(), Box::new(|s: &mut Style| s.color = Some([0xFF, 0, 0]))),
        ]),
        ("Titles and Headings", vec![
            ("Heading 1".into(), Box::new(move |s: &mut Style| { s.bold = true; s.size = 150; s.color = Some([0x44, 0x54, 0x6A]); s.bottom = line(Some(acc(0)), LineStyle::Thick); })),
            ("Heading 2".into(), Box::new(move |s: &mut Style| { s.bold = true; s.size = 130; s.color = Some([0x44, 0x54, 0x6A]); s.bottom = line(Some(tint(acc(0), 0.5)), LineStyle::Thick); })),
            ("Heading 3".into(), Box::new(move |s: &mut Style| { s.bold = true; s.color = Some([0x44, 0x54, 0x6A]); s.bottom = line(Some(tint(acc(0), 0.6)), LineStyle::Medium); })),
            ("Heading 4".into(), Box::new(|s: &mut Style| { s.bold = true; s.color = Some([0x44, 0x54, 0x6A]); })),
            ("Title".into(), Box::new(|s: &mut Style| { s.size = 180; s.font = "Calibri Light".into(); s.color = Some([0x44, 0x54, 0x6A]); })),
            ("Total".into(), Box::new(move |s: &mut Style| { s.bold = true; s.top = line(Some(acc(0)), LineStyle::Thin); s.bottom = line(Some(acc(0)), LineStyle::Double); })),
        ]),
    ];
    let mut themed: Vec<(String, F)> = vec![];
    // Themed styles refer to the theme's colors (with tints), so they follow a theme change.
    let thc = app.book.theme_colors.clone();
    let white = sheet_engine::theme::ThemeColor::new(1, 0.0);
    let white_c = white.resolve(&thc);
    for (pct, t) in [("20%", 0.8f64), ("40%", 0.6), ("60%", 0.4)] { for i in 0..6 { let tc = sheet_engine::theme::ThemeColor::new(4 + i as u8, t); let c = tc.resolve(&thc); let dark = t < 0.5; themed.push((format!("{pct} - Accent{}", i + 1), Box::new(move |s: &mut Style| { s.fill = Some(c); s.fill_th = Some(tc); (s.color, s.color_th) = if dark { (Some(white_c), Some(white)) } else { (None, None) }; }))); } }
    for i in 0..6 { let tc = sheet_engine::theme::ThemeColor::new(4 + i as u8, 0.0); let c = tc.resolve(&thc); themed.push((format!("Accent{}", i + 1), Box::new(move |s: &mut Style| { s.fill = Some(c); s.fill_th = Some(tc); s.color = Some(white_c); s.color_th = Some(white); }))); }
    sections.push(("Themed Cell Styles", themed));
    sections.push(("Number Format", vec![
        ("Comma".into(), Box::new(|s: &mut Style| s.num_fmt = r#"_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)"#.into()) as F),
        ("Comma [0]".into(), Box::new(|s: &mut Style| s.num_fmt = r#"_(* #,##0_);_(* \(#,##0\);_(* "-"_);_(@_)"#.into())),
        ("Currency".into(), Box::new(|s: &mut Style| s.num_fmt = ACCOUNTING.into())),
        ("Currency [0]".into(), Box::new(|s: &mut Style| s.num_fmt = r#"_("$"* #,##0_);_("$"* \(#,##0\);_("$"* "-"_);_(@_)"#.into())),
        ("Percent".into(), Box::new(|s: &mut Style| s.num_fmt = "0%".into())),
    ]));
    let mut chosen: Option<F> = None;
    for (title, items) in sections {
        menu_heading(ui, title);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (name, f) in items {
                let mut preview = Style::default();
                f(&mut preview);
                let (rect, r) = ui.allocate_exact_size(vec2(80.0, 26.0), Sense::click());
                ui.painter().rect_filled(rect, 0.0, preview.fill.map(crate::app::rgb).unwrap_or(c_card()));
                if preview.bottom.style != LineStyle::None { ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(preview.bottom.style.width().max(1.0), preview.bottom.color.map(crate::app::rgb).unwrap_or(egui::Color32::BLACK))); }
                if preview.left.style != LineStyle::None { ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, preview.left.color.map(crate::app::rgb).unwrap_or(egui::Color32::BLACK)), egui::StrokeKind::Inside); }
                if r.hovered() { ui.painter().rect_stroke(rect, 0.0, Stroke::new(2.0, crate::grid::SELECT), egui::StrokeKind::Inside); }
                let font = crate::grid::font_for(&preview, (preview.size_pt() * 1.1).min(15.0));
                ui.painter().with_clip_rect(rect).text(rect.left_center() + vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, tr(&name), font, preview.color.map(crate::app::rgb).unwrap_or(c_ink()));
                if r.on_hover_text(tr(&name)).clicked() { chosen = Some(f); }
            }
        });
    }
    if let Some(f) = chosen { app.apply_style("Cell Style", move |s| f(s)); ui.close(); }
}

/// Fill ▸ Across Worksheets: copies the selection to the same place on every other sheet.
fn fill_across(app: &mut SheetApp) {
    let s = app.sheet(); let r = app.sel.range();
    let clip = app.book.copy(s, r);
    app.checkpoint("Fill Across Worksheets");
    for i in 0..app.book.sheets.len() { if i != s { app.book.paste(i, r, &clip, Default::default()); } }
    app.changed();
}

/// Fill ▸ Justify: joins the selected column's text into the first cell, wrapping at its width.
fn justify(app: &mut SheetApp) {
    let s = app.sheet(); let r = app.sel.range();
    let words: Vec<String> = (r.r0..=r.r1.min(r.r0 + 10_000)).map(|row| app.book.display(s, sheet_engine::Pos::new(row, r.c0))).filter(|t| !t.is_empty()).collect();
    if words.is_empty() { return; }
    let width = (app.book.sheets[s].col_width(r.c0) / 7.0).max(4.0) as usize;
    let mut lines = vec![String::new()];
    for w in words.join(" ").split_whitespace() { let l = lines.last_mut().unwrap(); if !l.is_empty() && l.len() + 1 + w.len() > width { lines.push(w.to_string()); } else { if !l.is_empty() { l.push(' '); } l.push_str(w); } }
    app.checkpoint("Justify");
    app.book.clear(s, sheet_engine::Range { c1: r.c0, ..r }, true, false, false);
    for (i, l) in lines.iter().enumerate() { app.book.set_input(s, sheet_engine::Pos::new(r.r0 + i as u32, r.c0), l); }
    app.changed();
}

/// Flash Fill: fills the rest of the active column by repeating the pattern of the examples typed
/// above it — a piece of the cell to the left (a word, the text before or after a separator) or
/// that text's case changed.
pub fn flash_fill(app: &mut SheetApp) {
    let s = app.sheet();
    let col = app.sel.active.col;
    if col == 0 { app.error = Some(tr("Flash Fill needs a column of examples next to the data.")); return; }
    let sh = &app.book.sheets[s];
    let src = |row: u32| sh.value(sheet_engine::Pos::new(row, col - 1)).display();
    let mut top = app.sel.active.row; while top > 0 && !src(top - 1).is_empty() { top -= 1; }
    let examples: Vec<(String, String)> = (top..).take_while(|r| !src(*r).is_empty()).map(|r| (src(r), sh.value(sheet_engine::Pos::new(r, col)).display())).take_while(|(_, out)| !out.is_empty()).collect();
    if examples.is_empty() { app.error = Some(tr("Type an example of the result in the first cell, then try again.")); return; }
    let rules: Vec<Box<dyn Fn(&str) -> String>> = {
        let mut v: Vec<Box<dyn Fn(&str) -> String>> = vec![];
        for i in 0..4usize { v.push(Box::new(move |t: &str| t.split_whitespace().nth(i).unwrap_or("").to_string())); }
        v.push(Box::new(|t: &str| t.split_whitespace().last().unwrap_or("").to_string()));
        for sep in ['@', ',', '-', '/', '.', '_', ':'] {
            v.push(Box::new(move |t: &str| t.split(sep).next().unwrap_or("").trim().to_string()));
            v.push(Box::new(move |t: &str| t.rsplit(sep).next().unwrap_or("").trim().to_string()));
        }
        v.push(Box::new(|t: &str| t.split_whitespace().filter_map(|w| w.chars().next()).collect()));
        v
    };
    let cases: [fn(&str) -> String; 4] = [|t| t.to_string(), |t| t.to_uppercase(), |t| t.to_lowercase(), |t| { let mut c = t.chars(); c.next().map(|f| f.to_uppercase().chain(c.flat_map(|x| x.to_lowercase())).collect()).unwrap_or_default() }];
    let found = rules.iter().flat_map(|r| cases.iter().map(move |c| (r, c))).find(|(r, c)| examples.iter().all(|(i, o)| c(&r(i)) == *o));
    let Some((rule, case)) = found else { app.error = Some(tr("Flash Fill couldn't find a pattern. Type another example and try again.")); return; };
    let start = top + examples.len() as u32;
    let fills: Vec<(u32, String)> = (start..).take_while(|r| !src(*r).is_empty()).map(|r| (r, case(&rule(&src(r))))).collect();
    app.checkpoint("Flash Fill");
    let n = fills.len();
    for (r, t) in fills { app.book.set_input(s, sheet_engine::Pos::new(r, col), &format!("'{t}")); }
    app.changed();
    app.notify(&crate::i18n::trf("Flash Fill changed {} cells.", &[&n]));
}

/// Selects the cells of the current selection (or sheet) that hold formulas, notes, constants or blanks.
fn select_special(app: &mut SheetApp, kind: u8) {
    let s = &app.book.sheets[app.sheet()];
    let area = if app.sel.range().is_cell() { s.extent().map(|e| sheet_engine::Range::new(sheet_engine::Pos::new(0, 0), e)).unwrap_or_default() } else { app.sel.range() };
    let hits: Vec<sheet_engine::Pos> = match kind {
        0 => s.cells.iter().filter(|(p, c)| area.contains(**p) && c.is_formula()).map(|(p, _)| *p).collect(),
        1 => s.notes.keys().copied().filter(|p| area.contains(*p)).collect(),
        2 => s.cells.iter().filter(|(p, c)| area.contains(**p) && matches!(c.input, sheet_engine::Input::Value(_))).map(|(p, _)| *p).collect(),
        3 => area.iter().filter(|p| s.value(*p).is_empty()).take(10_000).collect(),
        _ => crate::home_hooks::special_cells(&app.book, app.sheet(), area, kind),
    };
    if hits.is_empty() { app.error = Some(tr("No cells were found.")); return; }
    let mut sel = crate::app::Selection::at(hits[0]);
    sel.extra = hits[1..].iter().map(|p| sheet_engine::Range::cell(*p)).collect();
    // The active range goes last, as ranges() puts it.
    app.sel = sel;
}

fn category_of(fmt: &str) -> &'static str {
    let f = fmt.to_ascii_lowercase();
    if f == "general" { "General" } else if f == "@" { "Text" } else if f.contains('%') { "Percentage" } else if f.contains("e+") { "Scientific" } else if f.contains('/') && f.contains('?') { "Fraction" }
    else if sheet_engine::numfmt::is_date_format(fmt) { if f.contains('h') || f.contains('s') && !f.contains('y') { "Time" } else { "Date" } } else if f.contains("_(") { "Accounting" } else if f.contains('$') || f.contains('€') || f.contains('£') { "Currency" } else { "Number" }
}

/// Adds or removes a decimal place in a number format.
pub fn decimals(fmt: &str, d: i32, v: &sheet_engine::Value) -> String {
    let base = if fmt.eq_ignore_ascii_case("general") {
        // Start from the decimals the value shows.
        let shown = v.display();
        let n = shown.split_once('.').map(|(_, f)| f.len()).unwrap_or(0);
        if n == 0 { "0".to_string() } else { format!("0.{}", "0".repeat(n)) }
    } else { fmt.to_string() };
    base.split(';').map(|sec| {
        let mut out = String::new(); let mut done = false; let mut quoted = false;
        let chars: Vec<char> = sec.chars().collect();
        // The last digit placeholder of the number part.
        let last = chars.iter().enumerate().filter(|(i, c)| { let q = chars[..*i].iter().filter(|x| **x == '"').count() % 2 == 1; !q && matches!(c, '0' | '#' | '?') }).map(|(i, _)| i).last();
        for (i, c) in chars.iter().enumerate() {
            if *c == '"' { quoted = !quoted; }
            out.push(*c);
            if Some(i) == last && !done && !quoted {
                done = true;
                if d > 0 { if chars[..=i].contains(&'.') { out.push('0'); } else { out.push_str(".0"); } }
                else if chars[..=i].contains(&'.') { out.pop(); if out.ends_with('.') { out.pop(); } }
            }
        }
        out
    }).collect::<Vec<_>>().join(";")
}

fn borders_button(app: &mut SheetApp, ui: &mut Ui) {
    let last: BorderKind = ui.data(|d| d.get_temp(egui::Id::new("last-border"))).unwrap_or(BorderKind::Bottom);
    let (b, ba) = small_split(ui, "borderBottom", false, "Borders");
    let line: LineStyle = ui.data(|d| d.get_temp(egui::Id::new("border-line"))).unwrap_or(LineStyle::Thin);
    let color: Option<[u8; 3]> = ui.data(|d| d.get_temp(egui::Id::new("border-color"))).unwrap_or(None);
    let apply = |app: &mut SheetApp, k: BorderKind| {
        app.checkpoint("Borders");
        let s = app.sheet();
        for r in app.sel.ranges() { app.book.apply_borders(s, r, k, Border { style: line, color }); }
        app.dirty = true;
    };
    if b.clicked() { apply(app, last); }
    crate::theme::menu(&ba, |ui| {
        for (icon, label, k) in [("borderBottom", "Bottom Border", BorderKind::Bottom), ("borderTop", "Top Border", BorderKind::Top), ("borderLeft", "Left Border", BorderKind::Left), ("borderRight", "Right Border", BorderKind::Right),
            ("borderNone", "No Border", BorderKind::None), ("borderAll", "All Borders", BorderKind::All), ("borderOuter", "Outside Borders", BorderKind::Outside), ("borderOuter", "Thick Box Border", BorderKind::ThickBox),
            ("borderInside", "Inside Borders", BorderKind::Inside), ("borderInsideH", "Inside Horizontal", BorderKind::InsideH), ("borderInsideV", "Inside Vertical", BorderKind::InsideV),
            ("borderDiagonalDown", "Diagonal Down", BorderKind::DiagDown), ("borderDiagonalUp", "Diagonal Up", BorderKind::DiagUp)] {
            if menu_item(ui, icon, label) { ui.data_mut(|d| d.insert_temp(egui::Id::new("last-border"), k)); apply(app, k); ui.close(); }
        }
        ui.separator();
        crate::theme::submenu(ui, tr("Line Style"), |ui| {
            for (l, name) in [(LineStyle::Thin, "Thin"), (LineStyle::Medium, "Medium"), (LineStyle::Thick, "Thick"), (LineStyle::Dashed, "Dashed"), (LineStyle::Dotted, "Dotted"), (LineStyle::Double, "Double"), (LineStyle::Hair, "Hair")] {
                if ui.selectable_label(line == l, tr(name)).clicked() { ui.data_mut(|d| d.insert_temp(egui::Id::new("border-line"), l)); ui.close(); }
            }
        });
        crate::theme::submenu(ui, tr("Line Color"), |ui| { let th = app.book.theme_colors.clone(); if let Some(c) = crate::themes_ui::color_menu(ui, "Automatic", &th) { ui.data_mut(|d| d.insert_temp(egui::Id::new("border-color"), c.map(|x| x.0))); ui.close(); } });
        if ui.button(tr("More Borders…")).clicked() { crate::dialogs::open_format_cells(app, 3); ui.close(); }
    });
}

fn format_menu(app: &mut SheetApp, ui: &mut Ui) {
    let r = app.sel.range();
    let s = app.sheet();
    menu_heading(ui, "Cell Size");
    if menu_item(ui, "rowHeight", "Row Height…") { let h = app.book.sheets[s].row_height(r.r0); app.dialog = Some(Dialog::Size { cols: false, value: h }); ui.close(); }
    if menu_item(ui, "autofit", "AutoFit Row Height") { let rows: Vec<u32> = (r.r0..=r.r1.min(r.r0 + 5000)).collect(); crate::grid::autofit_rows(app, &rows); ui.close(); }
    if menu_item(ui, "columnWidth", "Column Width…") { let w = app.book.sheets[s].col_width(r.c0); app.dialog = Some(Dialog::Size { cols: true, value: w }); ui.close(); }
    if menu_item(ui, "autofit", "AutoFit Column Width") { let cols: Vec<u32> = (r.c0..=r.c1.min(r.c0 + 500)).collect(); crate::grid::autofit_cols(app, &cols); ui.close(); }
    if menu_item(ui, "columnWidth", "Default Width…") { let w = app.book.sheets[s].default_col_width; app.dialog = Some(Dialog::Size { cols: true, value: -w }); ui.close(); }
    menu_heading(ui, "Visibility");
    for (label, rows, hide) in [("Hide Rows", true, true), ("Hide Columns", false, true), ("Unhide Rows", true, false), ("Unhide Columns", false, false)] {
        if ui.button(tr(label)).clicked() {
            app.checkpoint(if hide { "Hide" } else { "Unhide" });
            let sh = &mut app.book.sheets[s];
            if rows { for x in r.r0..=r.r1.min(r.r0 + 100_000) { if hide { sh.hidden_rows.insert(x); } else { sh.hidden_rows.remove(&x); } } }
            else { for x in r.c0..=r.c1 { if hide { sh.hidden_cols.insert(x); } else { sh.hidden_cols.remove(&x); } } }
            app.dirty = true;
            ui.close();
        }
    }
    menu_heading(ui, "Organize Sheets");
    if menu_item(ui, "rename", "Rename Sheet") { app.dialog = Some(Dialog::Rename { sheet: s, name: app.book.sheets[s].name.clone() }); ui.close(); }
    if menu_item(ui, "copy", "Duplicate Sheet") { app.checkpoint("Duplicate Sheet"); let n = app.book.duplicate_sheet(s); app.set_sheet(n); app.changed(); ui.close(); }
    menu_heading(ui, "Protection");
    let locked = app.style().locked;
    if menu_item(ui, "lockTracking", if locked { "Unlock Cell" } else { "Lock Cell" }) { app.apply_style("Lock Cell", move |st| st.locked = !locked); ui.close(); }
    let prot = app.book.sheets[s].protected;
    if menu_item(ui, "protect", if prot { "Unprotect Sheet" } else { "Protect Sheet" }) { app.checkpoint("Protect Sheet"); app.book.sheets[s].protected = !prot; app.dirty = true; ui.close(); }
    ui.separator();
    if menu_item(ui, "formatPainter", "Format Cells…") { crate::dialogs::open_format_cells(app, 0); ui.close(); }
}

fn insert(app: &mut SheetApp, ui: &mut Ui) { crate::insert_tab::groups(app, ui); }

pub fn insert_text(app: &mut SheetApp, t: &str) {
    match app.edit.as_mut() {
        Some(e) => { let mut ch: Vec<char> = e.text.chars().collect(); let at = e.cursor.min(ch.len()); for (k, c) in t.chars().enumerate() { ch.insert(at + k, c); } e.text = ch.into_iter().collect(); e.cursor = at + t.chars().count(); e.request_focus = true; }
        None => { let cur = app.book.edit_text(app.sheet(), app.sel.active); app.begin_edit(Some(cur + t), false); }
    }
}

fn page_layout(app: &mut SheetApp, ui: &mut Ui) { crate::page_layout::tab(app, ui); }

fn formulas(app: &mut SheetApp, ui: &mut Ui) { crate::formulas_tab::tab(app, ui); }

/// Starts a formula with the function, as the Function Library menus do.
pub fn insert_function(app: &mut SheetApp, name: &str) {
    match app.edit.as_mut() {
        Some(e) if e.text.starts_with('=') => { let mut ch: Vec<char> = e.text.chars().collect(); let at = e.cursor.min(ch.len()); let t = format!("{name}("); for (k, c) in t.chars().enumerate() { ch.insert(at + k, c); } e.text = ch.into_iter().collect(); e.cursor = at + t.len(); e.request_focus = true; }
        _ => { app.commit(None); app.begin_edit(Some(format!("={name}(")), false); }
    }
}

fn data(app: &mut SheetApp, ui: &mut Ui) { crate::data_tab::tab(app, ui); }

pub fn trim_spaces(app: &mut SheetApp) {
    let s = app.sheet();
    app.checkpoint("Trim Spaces");
    for r in app.sel.ranges() {
        let cells: Vec<(sheet_engine::Pos, String)> = app.book.sheets[s].cells.iter().filter(|(p, c)| r.contains(**p) && matches!(c.input, sheet_engine::Input::Value(sheet_engine::Value::Str(_)))).map(|(p, c)| (*p, c.value.display())).collect();
        for (p, t) in cells { let n = t.split_whitespace().collect::<Vec<_>>().join(" "); if n != t { app.book.set_cell_input(s, p, sheet_engine::Input::Value(sheet_engine::Value::str(n)), None); } }
    }
    app.changed();
}

fn review(app: &mut SheetApp, ui: &mut Ui) { crate::review_tab::tab(app, ui); }

fn view(app: &mut SheetApp, ui: &mut Ui) { crate::view_tab::tab(app, ui); }

/// Data ▸ Filter, for the Home tab's Sort & Filter menu too.
pub fn toggle_filter(app: &mut SheetApp) { crate::filter_ui::toggle_filter(app); }
/// Data ▸ Clear: shows the rows the sheet's filters hid.
pub fn clear_filters(app: &mut SheetApp) { crate::filter_ui::clear_filters(app); }
/// Home ▸ Format as Table: the style gallery, then Create Table.
pub fn format_as_table_menu(app: &mut SheetApp, ui: &mut Ui) { crate::filter_ui::format_as_table_menu(app, ui); }
