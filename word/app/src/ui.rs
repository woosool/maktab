//! Menus, toolbar, panels and dialogs (views/editor.js, ui/toolbar.js, ui/menu.js).

use crate::app::{ParagraphDraft, Pending, WordApp};
use crate::ribbon;
use egui::{Color32, ComboBox, DragValue, Key, KeyboardShortcut, Modifiers, RichText, Ui};
use word_engine::numbering::ListKind;
use word_engine::page::PAPER_SIZES;
use word_engine::{Alignment, LineRule, NoteKind, VertAlign};

const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers { shift: true, ..Modifiers::COMMAND };

fn sc(m: Modifiers, k: Key) -> KeyboardShortcut { KeyboardShortcut::new(m, k) }

/// Global shortcuts (consumed before the canvas sees them).
pub fn shortcuts(app: &mut WordApp, ctx: &egui::Context) {
    let fire = |m, k| ctx.input_mut(|i| i.consume_shortcut(&sc(m, k)));
    // Alt variants first: logical matching would let Cmd+F claim Cmd+Alt+F.
    if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers { alt: true, ..CMD }, Key::F))) { app.insert_note(NoteKind::Foot); }
    if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers { alt: true, ..CMD }, Key::D))) { app.insert_note(NoteKind::End); }
    if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers { alt: true, ..CMD }, Key::M))) { app.new_comment(); }
    if fire(CMD, Key::N) { app.guarded(ctx, Pending::New); }
    if fire(CMD, Key::O) { app.guarded(ctx, Pending::Open); }
    if fire(CMD_SHIFT, Key::S) { app.save(ctx, true); }
    if fire(CMD, Key::S) { app.save(ctx, false); }
    if fire(CMD, Key::P) { app.open_print(); }
    if fire(CMD, Key::F) || fire(CMD, Key::H) { app.find.open = true; app.find.focus = true; }
    if fire(CMD_SHIFT, Key::E) { let on = !app.doc.track_changes; app.doc.set_track_changes(on); }
    if fire(CMD_SHIFT, Key::Z) || fire(CMD, Key::Y) { app.doc.redo(); app.scroll_to_caret = true; }
    if fire(CMD, Key::Z) { app.doc.undo(); app.scroll_to_caret = true; }
    // Formatting shortcuts apply only while the document has focus.
    if !app.canvas_focus { return; }
    if fire(CMD, Key::A) { app.doc.select_all(); }
    if fire(CMD, Key::D) { app.open_font_dialog(); }
    if fire(CMD, Key::B) { app.doc.toggle_bold(); }
    if fire(CMD, Key::I) { app.doc.toggle_italic(); }
    if fire(CMD, Key::U) { app.doc.toggle_underline(); }
    if fire(CMD, Key::E) { app.doc.set_alignment(Alignment::Center); }
    if fire(CMD, Key::L) { app.doc.set_alignment(Alignment::Left); }
    if fire(CMD, Key::R) { app.doc.set_alignment(Alignment::Right); }
    if fire(CMD, Key::J) { app.doc.set_alignment(Alignment::Justify); }
    if fire(CMD_SHIFT, Key::Period) || fire(CMD, Key::CloseBracket) { app.doc.grow_font(true); }
    if fire(CMD_SHIFT, Key::Comma) || fire(CMD, Key::OpenBracket) { app.doc.grow_font(false); }
    if fire(CMD_SHIFT, Key::Equals) || fire(CMD_SHIFT, Key::Plus) { app.doc.toggle_vert_align(VertAlign::Superscript); }
    if fire(CMD, Key::Equals) && !ctx.input(|i| i.modifiers.alt) { app.doc.toggle_vert_align(VertAlign::Subscript); }
    if fire(CMD, Key::Space) || fire(CMD, Key::Backslash) { app.doc.clear_run_formatting(); }
    if fire(CMD, Key::K) { app.dialogs.link = Some(app.doc.link_at(app.doc.range().0).unwrap_or_default()); }
    if fire(CMD_SHIFT, Key::L) { app.doc.toggle_list(ListKind::Bullet); }
    if fire(CMD_SHIFT, Key::Num7) { app.doc.toggle_list(ListKind::Number); }
    if fire(CMD_SHIFT, Key::Num8) { app.show_marks = !app.show_marks; }
    for (k, style) in [(Key::Num1, "Heading1"), (Key::Num2, "Heading2"), (Key::Num3, "Heading3"), (Key::Num0, "Normal")] {
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers { alt: true, ..CMD }, k))) { app.doc.set_paragraph_style(style); }
    }
    if fire(CMD, Key::Plus) { app.set_zoom(app.zoom + 0.1); }
    if fire(CMD, Key::Minus) { app.set_zoom(app.zoom - 0.1); }
    if fire(CMD, Key::Num0) { app.set_zoom(1.0); }
}


pub fn open_paragraph_dialog(app: &mut WordApp) {
    let f = app.doc.current_para_format();
    let rtl = app.doc.paragraph_rtl(app.doc.current_paragraph());
    app.dialogs.paragraph = Some(ParagraphDraft {
        left: f.indent_left as f32 / 1440.0, right: f.indent_right as f32 / 1440.0, first: f.first_line as f32 / 1440.0,
        before: f.space_before, after: f.space_after, rule: f.line_rule, spacing: f.line_spacing,
        alignment: f.alignment, rtl, outline: f.outline_level, contextual: f.contextual_spacing,
        widow: f.widow_control, keep_next: f.keep_next, keep_lines: f.keep_lines, page_break_before: f.page_break_before,
    });
}

pub fn status_bar(app: &mut WordApp, ui: &mut Ui) {
    let now = ui.input(|i| i.time);
    egui::Panel::bottom("status").frame(egui::Frame::NONE.fill(ribbon::c_frame()).inner_margin(egui::Margin::symmetric(10, 3))).show(ui, |ui| {
        ui.with_layout(ribbon::row_layout(), |ui| {
            let muted = |t: String| RichText::new(t).size(12.0).color(ribbon::c_ink3());
            let page = app.layout.page_of(app.doc.selection.focus) + 1;
            ui.label(muted(crate::i18n::tr("Section: 1")));
            ui.add_space(10.0);
            ui.label(muted(crate::i18n::trf("Page {} of {}", &[&page, &app.layout.page_count()])));
            ui.add_space(10.0);
            ui.label(muted(crate::i18n::trf("{} words", &[&app.doc.word_count()])));
            crate::macros::status(app, ui);
            if app.doc.track_changes {
                ui.add_space(10.0);
                if ui.add(egui::Button::new(RichText::new(crate::i18n::tr("Track Changes: On")).size(12.0).color(ribbon::ACCENT)).frame(false)).on_hover_text(crate::i18n::tr("Click to turn off")).clicked() { app.doc.set_track_changes(false); }
            }
            if let Some((_, until)) = &mut app.status { if until.is_infinite() { *until = now + 4.0; } }
            if let Some((msg, until)) = &app.status { if *until > now { ui.add_space(10.0); ui.label(muted(msg.clone())); } }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(muted(format!("{:.0}%", app.zoom * 100.0)));
                if ribbon::small(ui, "zoomIn", None, false, true, "Zoom in").clicked() { app.set_zoom(app.zoom + 0.1); }
                let mut pct = app.zoom * 100.0;
                ui.spacing_mut().slider_width = 110.0;
                if ui.add(egui::Slider::new(&mut pct, 25.0..=400.0).show_value(false).step_by(5.0)).changed() { app.set_zoom(pct / 100.0); }
                if ribbon::small(ui, "zoomOut", None, false, true, "Zoom out").clicked() { app.set_zoom(app.zoom - 0.1); }
                ui.add_space(8.0);
                if ribbon::small(ui, "pilcrow", None, app.show_marks, true, "Formatting marks").clicked() { app.show_marks = !app.show_marks; }
                if ribbon::small(ui, "navigationPane", None, app.show_outline, true, "Navigation pane").clicked() { app.show_outline = !app.show_outline; }
                if ribbon::small(ui, "webLayout", None, false, true, "Web layout: fit the page to the window").clicked() { app.fit_width = true; }
                if ribbon::small(ui, "printLayout", None, true, true, "Print layout").clicked() {}
                if ribbon::small(ui, "readMode", None, false, true, "Read Mode (Esc to return)").clicked() { app.read_mode = true; app.read_row = app.layout.page_of(app.doc.selection.focus) / 2; app.read_scroll = true; }
                if ribbon::small(ui, "focus", Some("Focus"), app.focus_mode, true, "Focus: hide the ribbon (Esc to return)").clicked() { app.focus_mode = true; }
            });
        });
    });
}

pub fn find_panel(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::left("find").resizable(true).default_size(260.0).show(ui, |ui| {
        ui.add_space(6.0);
        crate::theme::hrow(ui, |ui| { ui.heading(crate::i18n::tr("Find")); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| if ribbon::close_button(ui, "Close").clicked() { app.find.open = false; app.canvas_focus = true; }); });
        let r = ui.add(egui::TextEdit::singleline(&mut app.find.query).hint_text(crate::i18n::tr("Search document")).desired_width(f32::INFINITY));
        if app.find.focus { r.request_focus(); app.find.focus = false; }
        if r.has_focus() { app.canvas_focus = false; }
        if r.changed() { app.find.revision = u64::MAX; }
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if enter { let back = ui.input(|i| i.modifiers.shift); app.find_next(!back); r.request_focus(); }
        if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) { app.find.open = false; app.canvas_focus = true; }
        crate::theme::hrow(ui, |ui| {
            if crate::theme::check(ui, &mut app.find.match_case, crate::i18n::tr("Match case")).changed() { app.find.revision = u64::MAX; }
            if crate::theme::check(ui, &mut app.find.whole_words, crate::i18n::tr("Whole words")).changed() { app.find.revision = u64::MAX; }
        });
        app.refresh_find();
        crate::theme::hrow(ui, |ui| {
            if ribbon::small(ui, "previousArrow", None, false, true, "Previous match (Shift+Enter)").clicked() { app.find_next(false); }
            if ribbon::small(ui, "nextArrow", None, false, true, "Next match (Enter)").clicked() { app.find_next(true); }
            let n = app.find.matches.len();
            ui.label(match (n, app.find.current) { (0, _) if !app.find.query.is_empty() => "No results".to_string(), (0, _) => String::new(), (n, Some(i)) => format!("{} of {n}", i + 1), (n, None) => crate::i18n::trf("{} results", &[&n]) });
        });
        ui.separator();
        ui.label(crate::i18n::tr("Replace with"));
        let r2 = ui.add(egui::TextEdit::singleline(&mut app.find.replace).desired_width(f32::INFINITY));
        if r2.has_focus() { app.canvas_focus = false; }
        crate::theme::hrow(ui, |ui| {
            if ui.button(crate::i18n::tr("Replace")).clicked() { app.replace_current(); }
            if ui.button(crate::i18n::tr("Replace all")).clicked() { let ctx = ui.ctx().clone(); app.replace_all(&ctx); }
        });
    });
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    // Unsaved changes.
    if let Some(action) = app.dialogs.confirm.clone() {
        let mut close = false;
        egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(crate::i18n::tr("Save changes?"));
            ui.label(crate::i18n::tr("Your changes will be lost if you don't save them."));
            ui.add_space(8.0);
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "Save").clicked() { close = true; if app.save(ctx, false) { app.run_pending(ctx, action.clone()); } }
                if crate::theme::secondary(ui, "Cancel").clicked() { close = true; }
                if crate::theme::secondary(ui, "Don't Save").clicked() { close = true; app.doc.dirty = false; app.run_pending(ctx, action.clone()); }
            });
        });
        if close { app.dialogs.confirm = None; }
    }

    if let Some(msg) = app.dialogs.error.clone() {
        let mut close = false;
        egui::Modal::new(egui::Id::unique("error")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(crate::i18n::tr(&msg));
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "OK").clicked() { close = true; });
        });
        if close { app.dialogs.error = None; }
    }

    if let Some(mut page) = app.dialogs.page_setup.clone() {
        let mut open = true;
        let mut apply = false;
        crate::theme::dialog(ctx, "Page Setup").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            let inch = |v: i32| v as f32 / 1440.0;
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Paper"));
                let current = PAPER_SIZES.iter().find(|(_, w, h)| (page.width.min(page.height), page.width.max(page.height)) == (*w, *h)).map(|p| p.0).unwrap_or("Custom");
                crate::theme::Combo::from_id_salt("paper").selected_text(crate::i18n::tr(&current)).show_ui(ui, |ui| {
                    for (name, w, h) in PAPER_SIZES {
                        if ui.selectable_label(current == *name, crate::i18n::tr(&*name)).clicked() { if page.landscape { page.width = *h; page.height = *w; } else { page.width = *w; page.height = *h; } }
                    }
                });
            });
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Orientation"));
                let mut land = page.landscape;
                crate::theme::radio_value(ui, &mut land, false, crate::i18n::tr("Portrait"));
                crate::theme::radio_value(ui, &mut land, true, crate::i18n::tr("Landscape"));
                page.set_landscape(land);
            });
            let field = |ui: &mut Ui, label: &str, v: &mut i32| {
                let mut x = inch(*v);
                ui.label(crate::i18n::tr(label));
                if ui.add(DragValue::new(&mut x).range(0.0..=22.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { *v = (x * 1440.0).round() as i32; }
            };
            crate::theme::grid(ui, egui::Grid::new("margins").num_columns(4), |ui| {
                field(ui, "Width", &mut page.width); field(ui, "Height", &mut page.height); ui.end_row();
                field(ui, "Top", &mut page.top); field(ui, "Bottom", &mut page.bottom); ui.end_row();
                field(ui, "Left", &mut page.left); field(ui, "Right", &mut page.right); ui.end_row();
                field(ui, "Header", &mut page.header); field(ui, "Footer", &mut page.footer); ui.end_row();
                field(ui, "Gutter", &mut page.gutter); ui.end_row();
            });
            if let Err(e) = page.validate() { ui.colored_label(Color32::RED, e); }
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Apply").clicked() { apply = true; });
        });
        if apply {
            match app.doc.set_page_settings(page.clone()) { Ok(()) => app.dialogs.page_setup = None, Err(e) => app.dialogs.error = Some(e) }
        } else if open { app.dialogs.page_setup = Some(page); } else { app.dialogs.page_setup = None; }
    }

    if let Some(mut d) = app.dialogs.paragraph.clone() {
        let mut open = true;
        let mut apply = false;
        crate::theme::dialog(ctx, "Paragraph").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            crate::theme::section(ui, "General");
            crate::theme::grid(ui, egui::Grid::new("para-general").num_columns(2), |ui| {
                ui.label(crate::i18n::tr("Alignment"));
                crate::theme::Combo::from_id_salt("p-align").selected_text(crate::i18n::tr(&(match d.alignment { Alignment::Left => if d.rtl { "Right (start)" } else { "Left" }, Alignment::Center => "Centered", Alignment::Right => if d.rtl { "Left (end)" } else { "Right" }, Alignment::Justify => "Justified" }))).show_ui(ui, |ui| {
                    for (a, n) in [(Alignment::Left, "Start (left)"), (Alignment::Center, "Centered"), (Alignment::Right, "End (right)"), (Alignment::Justify, "Justified")] { ui.selectable_value(&mut d.alignment, a, crate::i18n::tr(&n)); }
                });
                ui.end_row();
                ui.label(crate::i18n::tr("Outline level"));
                crate::theme::Combo::from_id_salt("p-outline").selected_text(crate::i18n::tr(&(if d.outline >= 9 { "Body Text".to_string() } else { crate::i18n::trf("Level {}", &[&(d.outline + 1)]) }))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.outline, 9, crate::i18n::tr("Body Text"));
                    for l in 0..9u8 { ui.selectable_value(&mut d.outline, l, crate::i18n::trf("Level {}", &[&(l + 1)])); }
                });
                ui.end_row();
                ui.label(crate::i18n::tr("Direction"));
                crate::theme::hrow(ui, |ui| { crate::theme::radio_value(ui, &mut d.rtl, false, crate::i18n::tr("Left-to-right")); crate::theme::radio_value(ui, &mut d.rtl, true, crate::i18n::tr("Right-to-left")); });
                ui.end_row();
            });
            ui.separator();
            crate::theme::section(ui, "Indents and Spacing");
            crate::theme::grid(ui, egui::Grid::new("para").num_columns(2), |ui| {
                ui.label(crate::i18n::tr("Indent left")); ui.add(DragValue::new(&mut d.left).range(-22.0..=22.0).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
                ui.label(crate::i18n::tr("Indent right")); ui.add(DragValue::new(&mut d.right).range(-22.0..=22.0).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
                ui.label(crate::i18n::tr("First line (− hanging)")); ui.add(DragValue::new(&mut d.first).range(-22.0..=22.0).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
                ui.label(crate::i18n::tr("Space before")); ui.add(DragValue::new(&mut d.before).range(0.0..=600.0).speed(0.5).suffix(" pt")); ui.end_row();
                ui.label(crate::i18n::tr("Space after")); ui.add(DragValue::new(&mut d.after).range(0.0..=600.0).speed(0.5).suffix(" pt")); ui.end_row();
                ui.label(crate::i18n::tr("Line spacing"));
                crate::theme::hrow(ui, |ui| {
                    crate::theme::Combo::from_id_salt("rule").selected_text(crate::i18n::tr(&(match d.rule { LineRule::Auto => "Multiple", LineRule::Exact => "Exactly", LineRule::AtLeast => "At least" }))).show_ui(ui, |ui| {
                        for (r, n) in [(LineRule::Auto, "Multiple"), (LineRule::Exact, "Exactly"), (LineRule::AtLeast, "At least")] {
                            if ui.selectable_label(d.rule == r, crate::i18n::tr(&n)).clicked() && d.rule != r { d.rule = r; d.spacing = if r == LineRule::Auto { 1.0 } else { 12.0 }; }
                        }
                    });
                    if d.rule == LineRule::Auto { ui.add(DragValue::new(&mut d.spacing).range(0.06..=5.0).speed(0.01).max_decimals(2)); }
                    else { ui.add(DragValue::new(&mut d.spacing).range(0.7..=600.0).speed(0.25).suffix(" pt")); }
                });
                ui.end_row();
            });
            crate::theme::check(ui, &mut d.contextual, crate::i18n::tr("Don't add space between paragraphs of the same style"));
            ui.separator();
            crate::theme::section(ui, "Line and Page Breaks");
            crate::theme::check(ui, &mut d.widow, crate::i18n::tr("Widow/Orphan control"));
            crate::theme::check(ui, &mut d.keep_next, crate::i18n::tr("Keep with next"));
            crate::theme::check(ui, &mut d.keep_lines, crate::i18n::tr("Keep lines together"));
            crate::theme::check(ui, &mut d.page_break_before, crate::i18n::tr("Page break before"));
            ui.add_space(4.0);
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Apply").clicked() { apply = true; });
        });
        if apply {
            app.doc.set_para_props(word_engine::ParaProps {
                indent_left: Some((d.left * 1440.0).round() as i32), indent_right: Some((d.right * 1440.0).round() as i32), first_line: Some((d.first * 1440.0).round() as i32),
                space_before: Some(d.before), space_after: Some(d.after), line_rule: Some(d.rule), line_spacing: Some(d.spacing),
                alignment: Some(d.alignment), rtl: Some(d.rtl), outline_level: Some(d.outline), contextual_spacing: Some(d.contextual),
                widow_control: Some(d.widow), keep_next: Some(d.keep_next), keep_lines: Some(d.keep_lines), page_break_before: Some(d.page_break_before), ..Default::default()
            });
            app.dialogs.paragraph = None;
        } else if open { app.dialogs.paragraph = Some(d); } else { app.dialogs.paragraph = None; }
    }

    if let Some(mut d) = app.dialogs.list.clone() {
        use word_engine::numbering::{NumFmt, NUMBER_FORMATS};
        use crate::app::ListDialogKind as K;
        let mut open = true;
        let mut apply = false;
        let title = match d.kind { K::Number => "Define New Number Format", K::Bullet => "Define New Bullet", K::Multilevel => "Define New Multilevel List" };
        crate::theme::dialog(ctx, title).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            app.canvas_focus = false;
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Name")); ui.add(egui::TextEdit::singleline(&mut d.list.name).desired_width(220.0)); });
            if d.kind == K::Multilevel {
                crate::theme::hrow(ui, |ui| {
                    ui.label(crate::i18n::tr("Level"));
                    for l in 0..9 { ui.selectable_value(&mut d.level, l, crate::i18n::tr(&((l + 1).to_string()))); }
                });
            }
            let lv = d.level;
            let level = &mut d.list.levels[lv];
            crate::theme::grid(ui, egui::Grid::new("list-def").num_columns(2), |ui| {
                if d.kind == K::Bullet {
                    ui.label(crate::i18n::tr("Bullet character"));
                    let mut s = level.text.clone();
                    if ui.add(egui::TextEdit::singleline(&mut s).desired_width(60.0)).changed() { if let Some(c) = s.chars().last() { level.text = c.to_string(); } }
                    ui.end_row();
                    ui.label(crate::i18n::tr("Symbols"));
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(260.0);
                        for c in ['•', '◦', '▪', '■', '□', '◆', '◇', '►', '▶', '➢', '➤', '✓', '✔', '✗', '★', '☆', '♦', '–', '—', '→', '⇒', '❖', '◉', '○'] {
                            if ui.selectable_label(level.text == c.to_string(), crate::i18n::tr(&c.to_string())).clicked() { level.text = c.to_string(); }
                        }
                    });
                    ui.end_row();
                } else {
                    ui.label(crate::i18n::tr("Number style"));
                    crate::theme::Combo::from_id_salt("num-style").selected_text(crate::i18n::tr(&(NUMBER_FORMATS.iter().find(|f| f.0 == level.fmt).map(|f| f.1).unwrap_or("1, 2, 3")))).show_ui(ui, |ui| {
                        for (f, n) in NUMBER_FORMATS { ui.selectable_value(&mut level.fmt, *f, crate::i18n::tr(&*n)); }
                    });
                    ui.end_row();
                    ui.label(crate::i18n::tr("Number format"));
                    ui.add(egui::TextEdit::singleline(&mut level.text).desired_width(220.0)).on_hover_text(crate::i18n::tr("Write the text around the number: %1 is this list's first level, %2 the second, … e.g. \"Step %1:\", \"Article %1 –\", \"%1.%2\""));
                    ui.end_row();
                    ui.label(""); ui.weak(crate::i18n::trf("Use %{} for this level's number", &[&(lv + 1)])); ui.end_row();
                    ui.label(crate::i18n::tr("Start at")); ui.add(DragValue::new(&mut level.start).range(0..=32767)); ui.end_row();
                }
                ui.label(crate::i18n::tr("Indent (inches)"));
                let mut ind = level.indent_left as f32 / 1440.0;
                if ui.add(DragValue::new(&mut ind).speed(0.01).range(0.0..=10.0).max_decimals(2)).changed() { level.indent_left = (ind * 1440.0).round() as i32; }
                ui.end_row();
                ui.label(crate::i18n::tr("Number width (hanging)"));
                let mut h = level.hanging as f32 / 1440.0;
                if ui.add(DragValue::new(&mut h).speed(0.01).range(0.0..=5.0).max_decimals(2)).changed() { level.hanging = (h * 1440.0).round() as i32; }
                ui.end_row();
            });
            if level.fmt == NumFmt::Bullet && d.kind != K::Bullet { level.fmt = NumFmt::Decimal; }
            ui.separator();
            crate::theme::section(ui, "Preview");
            egui::Frame::group(ui.style()).fill(egui::Color32::WHITE).show(ui, |ui| {
                ui.set_min_width(300.0);
                let preview = d.list.preview();
                let shown = if d.kind == K::Multilevel { 4 } else { 1 };
                for (i, p) in preview.iter().take(shown).enumerate() {
                    for k in 0..if d.kind == K::Multilevel { 1 } else { 3 } {
                        let mut text = p.clone();
                        if k > 0 { let mut l = d.list.clone(); l.levels[0].start += k as u32; text = l.preview()[0].clone(); }
                        crate::theme::hrow(ui, |ui| { ui.add_space(i as f32 * 16.0); ui.label(RichText::new(text).color(egui::Color32::BLACK)); ui.label(RichText::new("────────────").color(egui::Color32::from_gray(190))); });
                    }
                }
            });
            crate::theme::check(ui, &mut d.save, crate::i18n::tr("Keep in My Lists"));
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Apply").clicked() { apply = true; });
        });
        if apply {
            if d.list.name.trim().is_empty() { d.list.name = "Custom list".into(); }
            app.doc.apply_custom_list(&d.list);
            if d.save {
                app.custom_lists.retain(|l| l.name != d.list.name);
                app.custom_lists.push(d.list.clone());
                app.save_custom_lists();
            }
            app.dialogs.list = None;
            app.canvas_focus = true;
        } else if open { app.dialogs.list = Some(d); } else { app.dialogs.list = None; app.canvas_focus = true; }
    }

    if let Some(mut d) = app.dialogs.font.clone() {
        let mut open = true;
        let mut apply = false;
        crate::theme::dialog(ctx, "Font").open(&mut open).resizable(false).collapsible(false).default_width(380.0).show(ctx, |ui| {
            ui.set_width(380.0);
            crate::theme::grid(ui, egui::Grid::new("font-grid").num_columns(2).spacing(egui::vec2(12.0, 6.0)), |ui| {
                ui.label(crate::i18n::tr("Font"));
                crate::theme::Combo::from_id_salt("fd-font").width(180.0).selected_text(crate::i18n::tr(&d.font.clone())).show_ui(ui, |ui| {
                    for name in word_engine::fonts::family_names() { ui.selectable_value(&mut d.font, name.clone(), name.as_str()); }
                });
                ui.end_row();
                ui.label(crate::i18n::tr("Style"));
                crate::theme::Combo::from_id_salt("fd-style").selected_text(crate::i18n::tr(&(match (d.bold, d.italic) { (false, false) => "Regular", (false, true) => "Italic", (true, false) => "Bold", (true, true) => "Bold Italic" }))).show_ui(ui, |ui| {
                    for (b, i, n) in [(false, false, "Regular"), (false, true, "Italic"), (true, false, "Bold"), (true, true, "Bold Italic")] {
                        if ui.selectable_label(d.bold == b && d.italic == i, crate::i18n::tr(&n)).clicked() { d.bold = b; d.italic = i; }
                    }
                });
                ui.end_row();
                ui.label(crate::i18n::tr("Size")); ui.add(DragValue::new(&mut d.size).range(1.0..=1638.0).speed(0.5).max_decimals(1).suffix(" pt")); ui.end_row();
                ui.label(crate::i18n::tr("Font color"));
                crate::theme::hrow(ui, |ui| {
                    let mut c = d.color.unwrap_or(word_engine::Color::BLACK);
                    if crate::ribbon::color_button(ui, &mut c) { d.color = Some(c); }
                    if ui.selectable_label(d.color.is_none(), crate::i18n::tr("Automatic")).clicked() { d.color = None; }
                });
                ui.end_row();
                ui.label(crate::i18n::tr("Character spacing")); ui.add(DragValue::new(&mut d.spacing).range(-20.0..=100.0).speed(0.1).max_decimals(1).suffix(" pt")); ui.end_row();
            });
            ui.separator();
            crate::theme::section(ui, "Effects");
            crate::theme::grid(ui, egui::Grid::new("font-fx").num_columns(2), |ui| {
                crate::theme::check(ui, &mut d.underline, crate::i18n::tr("Underline")); crate::theme::check(ui, &mut d.small_caps, crate::i18n::tr("Small caps")); ui.end_row();
                if crate::theme::check(ui, &mut d.strike, crate::i18n::tr("Strikethrough")).changed() && d.strike { d.double_strike = false; }
                if crate::theme::check(ui, &mut d.all_caps, crate::i18n::tr("All caps")).changed() && d.all_caps { d.small_caps = false; }
                ui.end_row();
                if crate::theme::check(ui, &mut d.double_strike, crate::i18n::tr("Double strikethrough")).changed() && d.double_strike { d.strike = false; }
                crate::theme::check(ui, &mut d.hidden, crate::i18n::tr("Hidden")); ui.end_row();
                if crate::theme::check(ui, &mut d.superscript, crate::i18n::tr("Superscript")).changed() && d.superscript { d.subscript = false; }
                crate::theme::check(ui, &mut d.outline, crate::i18n::tr("Outline")); ui.end_row();
                if crate::theme::check(ui, &mut d.subscript, crate::i18n::tr("Subscript")).changed() && d.subscript { d.superscript = false; }
                crate::theme::check(ui, &mut d.shadow, crate::i18n::tr("Shadow")); ui.end_row();
            });
            ui.separator();
            // Preview in the chosen face.
            let key = word_engine::fonts::resolve(&d.font, d.bold, d.italic);
            let mut text = RichText::new(if d.all_caps { "THE QUICK BROWN FOX" } else { "The quick brown fox" }).font(egui::FontId::new(d.size.clamp(8.0, 28.0) * 4.0 / 3.0, egui::FontFamily::Name(crate::app::face_name(key.face as usize).into())));
            let c = d.color.unwrap_or(word_engine::Color::BLACK);
            text = text.color(egui::Color32::from_rgb(c.0, c.1, c.2));
            if d.underline { text = text.underline(); }
            if d.strike || d.double_strike { text = text.strikethrough(); }
            // A fixed-size preview box (centring in free space balloons in right-to-left layouts).
            let (rect, _) = ui.allocate_exact_size(egui::vec2(380.0, 56.0), egui::Sense::hover());
            ui.painter().rect(rect, 4.0, egui::Color32::WHITE, ui.visuals().widgets.noninteractive.bg_stroke, egui::StrokeKind::Inside);
            let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Truncate), rect.width() - 16.0, egui::TextStyle::Body);
            ui.painter().with_clip_rect(rect).galley(rect.center() - galley.size() / 2.0, galley, egui::Color32::BLACK);
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Apply").clicked() { apply = true; });
        });
        if apply {
            use word_engine::{Caps, Strike, VertAlign};
            app.doc.set_run_props(word_engine::RunProps {
                font: Some(d.font.clone()), cs_font: Some(d.font.clone()), bold: Some(d.bold), bold_cs: Some(d.bold), italic: Some(d.italic), italic_cs: Some(d.italic),
                size: Some(d.size), size_cs: Some(d.size), color: Some(d.color), underline: Some(if d.underline { "single" } else { "none" }.into()),
                strike: Some(if d.double_strike { Strike::Double } else if d.strike { Strike::Single } else { Strike::None }),
                vert_align: Some(if d.superscript { VertAlign::Superscript } else if d.subscript { VertAlign::Subscript } else { VertAlign::Baseline }),
                caps: Some(if d.all_caps { Caps::AllCaps } else if d.small_caps { Caps::SmallCaps } else { Caps::Normal }),
                vanish: Some(d.hidden), outline: Some(d.outline), shadow: Some(d.shadow), spacing: Some(d.spacing), ..Default::default()
            });
            app.dialogs.font = None;
            app.canvas_focus = true;
        } else if open { app.dialogs.font = Some(d); } else { app.dialogs.font = None; }
    }

    if let Some(mut d) = app.dialogs.table_props.clone() {
        let mut open = true;
        let mut apply = false;
        crate::theme::dialog(ctx, "Table Properties").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            use word_engine::model::CellVAlign;
            crate::theme::section(ui, "Table");
            crate::theme::grid(ui, egui::Grid::new("tp-table").num_columns(2), |ui| {
                ui.label(crate::i18n::tr("Width")); ui.add(DragValue::new(&mut d.width).range(0.5..=22.0).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
                ui.label(crate::i18n::tr("Alignment"));
                crate::theme::hrow(ui, |ui| { for (a, n) in [(Alignment::Left, "Left"), (Alignment::Center, "Center"), (Alignment::Right, "Right")] { crate::theme::radio_value(ui, &mut d.alignment, a, crate::i18n::tr(&n)); } });
                ui.end_row();
                ui.label(crate::i18n::tr("Indent from left")); ui.add_enabled(d.alignment == Alignment::Left, DragValue::new(&mut d.indent).range(-5.0..=10.0).speed(0.01).max_decimals(2).suffix("\"")); ui.end_row();
                ui.label(crate::i18n::tr("Default cell margins"));
                crate::theme::hrow(ui, |ui| {
                    for (i, n) in [(0, "T"), (2, "B"), (3, "L"), (1, "R")] { ui.label(crate::i18n::tr(n)); ui.add(DragValue::new(&mut d.margins[i]).range(0.0..=5.0).speed(0.005).max_decimals(2)); }
                });
                ui.end_row();
            });
            ui.separator();
            crate::theme::section(ui, "Row (at the caret)");
            crate::theme::grid(ui, egui::Grid::new("tp-row").num_columns(2), |ui| {
                ui.label(crate::i18n::tr("Height"));
                crate::theme::hrow(ui, |ui| { ui.add(DragValue::new(&mut d.row_height).range(0.0..=22.0).speed(0.01).max_decimals(2).custom_formatter(|v, _| if v == 0.0 { "Auto".into() } else { format!("{v:.2}\"") })); crate::theme::check(ui, &mut d.exact, crate::i18n::tr("Exactly")); });
                ui.end_row();
                ui.label(""); crate::theme::check(ui, &mut d.cant_split, crate::i18n::tr("Don't allow the row to break across pages")); ui.end_row();
                ui.label(""); crate::theme::check(ui, &mut d.header, crate::i18n::tr("Repeat as header row at the top of each page")); ui.end_row();
            });
            ui.separator();
            crate::theme::section(ui, "Cell");
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Vertical alignment"));
                for (v, n) in [(CellVAlign::Top, "Top"), (CellVAlign::Center, "Center"), (CellVAlign::Bottom, "Bottom")] { crate::theme::radio_value(ui, &mut d.v_align, v, crate::i18n::tr(&n)); }
            });
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Apply").clicked() { apply = true; });
        });
        if apply {
            let t = d.table;
            let total: i32 = app.doc.tables[t].grid.iter().sum();
            let target = (d.width * 1440.0).round() as i32;
            if (target - total).abs() > 10 && total > 0 { app.doc.scale_table(t, target as f32 / total as f32); }
            app.doc.set_table_alignment(t, d.alignment, (d.indent * 1440.0).round() as i32);
            app.doc.set_table_cell_margins(d.margins.map(|m| (m * 1440.0).round() as i32));
            app.doc.set_row_height((d.row_height * 1440.0).round() as i32, d.exact);
            app.doc.set_rows_cant_split(d.cant_split);
            app.doc.set_header_rows(d.header);
            app.doc.for_selected_cells(|c| c.v_align = d.v_align);
            app.dialogs.table_props = None;
        } else if open { app.dialogs.table_props = Some(d); } else { app.dialogs.table_props = None; }
    }

    if let Some(mut target) = app.dialogs.link.clone() {
        let mut open = true;
        let mut result: Option<Option<String>> = None;
        crate::theme::dialog(ctx, "Link").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            if app.doc.selection.is_caret() { ui.weak(crate::i18n::tr("Select the text to link first.")); }
            let r = ui.add(egui::TextEdit::singleline(&mut target).hint_text(crate::i18n::tr("https://example.com")).desired_width(320.0));
            r.request_focus();
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "Apply").clicked() || r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    let t = target.trim().to_string();
                    // Bare domains get a scheme; fragments and mailto stay as typed.
                    let t = if t.is_empty() || t.contains("://") || t.starts_with('#') || t.starts_with("mailto:") { t } else { format!("https://{t}") };
                    result = Some((!t.is_empty()).then_some(t));
                }
                if ui.button(crate::i18n::tr("Remove link")).clicked() { result = Some(None); }
            });
        });
        if let Some(r) = result { app.doc.set_link(r); app.dialogs.link = None; app.canvas_focus = true; }
        else if open { app.dialogs.link = Some(target); } else { app.dialogs.link = None; }
    }

    if let Some((mut rows, mut cols)) = app.dialogs.table {
        let mut open = true;
        let mut insert = false;
        crate::theme::dialog(ctx, "Insert Table").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Columns")); ui.add(DragValue::new(&mut cols).range(1..=63)); ui.label(crate::i18n::tr("Rows")); ui.add(DragValue::new(&mut rows).range(1..=500)); });
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Insert").clicked() { insert = true; });
        });
        if insert { app.doc.insert_table(rows, cols); app.dialogs.table = None; app.scroll_to_caret = true; app.canvas_focus = true; }
        else if open { app.dialogs.table = Some((rows, cols)); } else { app.dialogs.table = None; }
    }

    if let Some((mut formula, mut picture)) = app.formula.clone() {
        let mut open = true;
        let mut insert = false;
        crate::theme::dialog(ctx, "Formula").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.label(crate::i18n::tr("Formula"));
            let r = ui.add(egui::TextEdit::singleline(&mut formula).desired_width(280.0).font(egui::TextStyle::Monospace));
            app.canvas_focus = false;
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Number format"));
                crate::theme::Combo::from_id_salt("fmt").selected_text(crate::i18n::tr(&(if picture.is_empty() { "General".to_string() } else { picture.clone() }))).show_ui(ui, |ui| {
                    ui.selectable_value(&mut picture, String::new(), crate::i18n::tr("General"));
                    for f in word_engine::formula::NUMBER_FORMATS { ui.selectable_value(&mut picture, f.to_string(), crate::i18n::tr(&*f)); }
                });
            });
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Paste function"));
                crate::theme::Combo::from_id_salt("fn").selected_text(crate::i18n::tr(&("…"))).show_ui(ui, |ui| {
                    for f in word_engine::formula::FUNCTIONS { if ui.button(crate::i18n::tr(&*f)).clicked() { formula.push_str(&format!("{f}()")); } }
                });
            });
            match app.doc.evaluate_formula(&formula, &picture) {
                Ok(v) => { ui.label(RichText::new(crate::i18n::trf("Result: {}", &[&v])).strong()); }
                Err(e) => { ui.colored_label(egui::Color32::from_rgb(200, 40, 40), e); }
            }
            ui.weak(crate::i18n::tr("ABOVE, BELOW, LEFT, RIGHT, A1, A1:B3 — SUM AVERAGE COUNT MAX MIN PRODUCT ROUND IF …"));
            if crate::theme::footer(ui, |ui| crate::theme::primary(ui, "Insert").clicked()) || r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { insert = true; }
        });
        if insert {
            match app.doc.insert_formula(&formula, &picture) { Ok(()) => { app.formula = None; app.canvas_focus = true; } Err(e) => app.dialogs.error = Some(e) }
        } else if open { app.formula = Some((formula, picture)); } else { app.formula = None; app.canvas_focus = true; }
    }

    if app.dialogs.shortcuts {
        let mut open = true;
        crate::theme::dialog(ctx, "Keyboard Shortcuts").open(&mut open).collapsible(false).default_width(520.0).show(ctx, |ui| {
            crate::theme::grid(ui, egui::Grid::new("keys").striped(true), |ui| {
                for (k, v) in [
                    ("Bold / Italic / Underline", "Cmd/Ctrl+B / I / U"), ("Align left / center / right / justify", "Cmd/Ctrl+L / E / R / J"),
                    ("Grow / shrink font", "Cmd/Ctrl+] / ["), ("Superscript / subscript", "Cmd/Ctrl+Shift+= / Cmd/Ctrl+="),
                    ("Clear text formatting", "Cmd/Ctrl+Space"), ("Heading 1–3 / Normal", "Cmd/Ctrl+Alt+1–3 / 0"),
                    ("Bullets / numbering", "Cmd/Ctrl+Shift+L / Shift+7"), ("Link", "Cmd/Ctrl+K"), ("Find and replace", "Cmd/Ctrl+F"),
                    ("Page break / line break", "Cmd/Ctrl+Enter / Shift+Enter"), ("Next / previous cell", "Tab / Shift+Tab"),
                    ("Formatting marks", "Cmd/Ctrl+Shift+8"), ("Zoom", "Cmd/Ctrl+ + / − / 0"), ("Word movement", "Alt (macOS) / Ctrl + arrows"),
                ] { ui.label(crate::i18n::tr(k)); ui.monospace(v); ui.end_row(); }
            });
        });
        app.dialogs.shortcuts = open;
    }

    crate::update::dialog(app, ctx);

    if app.dialogs.about {
        let mut open = true;
        crate::theme::dialog(ctx, crate::i18n::trf("About {}", &[&crate::APP_NAME])).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            // The icon, name and maker; links to the product page; license.
            static ICON: std::sync::OnceLock<Option<egui::TextureHandle>> = std::sync::OnceLock::new();
            let icon = ICON.get_or_init(|| image::load_from_memory(include_bytes!("../../assets/icon.png")).ok().map(|i| {
                let i = i.resize(128, 128, image::imageops::FilterType::Lanczos3).to_rgba8();
                ui.ctx().load_texture("about-icon", egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw()), egui::TextureOptions::LINEAR)
            }));
            crate::theme::hrow(ui, |ui| {
                if let Some(t) = icon { ui.add(egui::Image::new((t.id(), egui::vec2(72.0, 72.0)))); }
                crate::theme::vcol(ui, |ui| {
                    ui.label(RichText::new(format!("{}  قلم", crate::APP_NAME)).size(22.0).family(egui::FontFamily::Name("ui-bold".into())));
                    ui.label(crate::i18n::trf("Version {}", &[&env!("CARGO_PKG_VERSION")]));
                    ui.label(crate::i18n::tr("A free desktop word processor for creating, editing, and formatting .docx documents offline."));
                });
            });
            ui.add_space(8.0);
            crate::theme::hrow(ui, |ui| {
                ui.label(crate::i18n::tr("Made by"));
                ui.hyperlink_to("Woosool", "https://woosool.com");
            });
            ui.hyperlink_to(crate::i18n::tr("Qalam on woosool.com"), "https://woosool.com/en/products/qalam");
            if crate::theme::secondary(ui, "Check for Updates…").clicked() { crate::update::check(ui.ctx(), true, None); }
            ui.add_space(6.0);
            ui.label(RichText::new(crate::i18n::tr("Released under the MIT License.")).strong());
            ui.label(RichText::new("Copyright © 2026 Woosool").small());
            egui::CollapsingHeader::new(crate::i18n::tr("License")).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| ui.label(RichText::new(include_str!("../../LICENSE")).small().monospace()));
            });
            ui.add_space(6.0);
            ui.weak(crate::i18n::tr("Fonts: Noto Sans, Carlito, Caladea (OFL / Apache-2.0), Liberation (OFL)."));
        });
        app.dialogs.about = open;
    }
}

pub fn goto_comment(app: &mut WordApp, forward: bool) {
    let anchors = app.doc.comment_anchors();
    let at = app.doc.range().0;
    let target = if forward { anchors.iter().find(|a| a.1 > at).or(anchors.first()) } else { anchors.iter().rev().find(|a| a.1 < at).or(anchors.last()) };
    if let Some((_, s, e)) = target.cloned() { app.doc.select(s, e); app.scroll_to_caret = true; app.show_comments = true; }
}

/// Short local-looking date from an OOXML timestamp.
fn short_date(iso: &str) -> String { iso.get(..10).map(|d| format!("{d} {}", iso.get(11..16).unwrap_or(""))).unwrap_or_default() }

/// Comment threads in document order (review.js margin column, as a side pane).
pub fn comments_panel(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::right("comments").resizable(true).default_size(280.0).show(ui, |ui| {
        ui.add_space(6.0);
        crate::theme::hrow(ui, |ui| {
            ui.heading(crate::i18n::tr("Comments"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ribbon::close_button(ui, "Hide comments").clicked() { app.show_comments = false; }
                if ui.button(crate::i18n::tr("New")).on_hover_text(crate::i18n::tr("New comment on the selection")).clicked() { app.new_comment(); }
            });
        });
        ui.separator();
        let anchors = app.doc.comment_anchors();
        let active: Vec<String> = app.doc.comments_at(app.doc.selection.focus);
        let roots: Vec<(String, usize, usize)> = anchors.into_iter().filter(|(id, ..)| app.doc.comments.iter().any(|c| c.id == *id && c.parent.is_none())).collect();
        if roots.is_empty() { ui.weak(crate::i18n::tr("Select text and choose New to add a comment.")); }
        let mut action: Option<Box<dyn FnOnce(&mut WordApp)>> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (root, s, e) in roots {
                let thread: Vec<word_engine::Comment> = app.doc.comments.iter().filter(|c| c.id == root || c.parent.as_deref() == Some(&root)).cloned().collect();
                let done = thread.first().is_some_and(|c| c.done);
                let is_active = active.contains(&root);
                let active_fill = if ui.visuals().dark_mode { Color32::from_rgb(74, 60, 22) } else { Color32::from_rgb(255, 246, 214) };
                let frame = egui::Frame::group(ui.style()).fill(if is_active { active_fill } else if done { ui.visuals().faint_bg_color } else { ui.visuals().panel_fill })
                    .stroke(egui::Stroke::new(if is_active { 1.5 } else { 1.0 }, if is_active { Color32::from_rgb(230, 170, 40) } else { ui.visuals().widgets.noninteractive.bg_stroke.color }));
                let r = frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for c in &thread {
                        crate::theme::hrow(ui, |ui| {
                            ui.label(RichText::new(&c.author).strong());
                            ui.weak(short_date(&c.date));
                        });
                        match &mut app.comment_edit {
                            Some((id, draft, is_new)) if *id == c.id => {
                                let is_new = *is_new;
                                let te = ui.add(egui::TextEdit::multiline(draft).desired_rows(2).desired_width(f32::INFINITY).hint_text(crate::i18n::tr("Comment")));
                                te.request_focus();
                                app.canvas_focus = false;
                                crate::theme::hrow(ui, |ui| {
                                    let save = crate::theme::primary(ui, "Post").clicked() || te.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && i.modifiers.command);
                                    let cancel = crate::theme::secondary(ui, "Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape));
                                    if save { let (id, text) = (id.clone(), draft.clone()); action = Some(Box::new(move |app| { app.doc.edit_comment(&id, &text); app.comment_edit = None; app.canvas_focus = true; })); }
                                    else if cancel {
                                        let id = c.id.clone();
                                        // Cancelling a brand-new, empty comment removes it.
                                        action = Some(Box::new(move |app| { if is_new && app.doc.comments.iter().any(|c| c.id == id && c.text.is_empty()) { app.doc.undo(); } app.comment_edit = None; app.canvas_focus = true; }));
                                    }
                                });
                            }
                            _ => {
                                let text = if done { RichText::new(&c.text).weak() } else { RichText::new(&c.text) };
                                ui.label(text);
                            }
                        }
                        ui.add_space(4.0);
                    }
                    if app.comment_edit.is_none() {
                        match &mut app.reply {
                            Some((id, draft)) if *id == root => {
                                let te = ui.add(egui::TextEdit::multiline(draft).desired_rows(1).desired_width(f32::INFINITY).hint_text(crate::i18n::tr("Reply")));
                                te.request_focus();
                                app.canvas_focus = false;
                                crate::theme::hrow(ui, |ui| {
                                    if crate::theme::primary(ui, "Reply").clicked() && !draft.trim().is_empty() {
                                        let (id, text) = (id.clone(), draft.clone());
                                        action = Some(Box::new(move |app| { let author = app.author.clone(); app.doc.reply_to_comment(&id, &author, &text, &crate::app::now_iso()); app.reply = None; app.canvas_focus = true; }));
                                    }
                                    if crate::theme::secondary(ui, "Cancel").clicked() { action = Some(Box::new(|app| { app.reply = None; app.canvas_focus = true; })); }
                                });
                            }
                            _ => {
                                crate::theme::hrow(ui, |ui| {
                                    let root2 = root.clone();
                                    if !done && ui.small_button(crate::i18n::tr("Reply")).clicked() { action = Some(Box::new(move |app| app.reply = Some((root2, String::new())))); }
                                    let root2 = root.clone();
                                    if ui.small_button(if done { "Reopen" } else { "Resolve" }).clicked() { action = Some(Box::new(move |app| app.doc.set_comment_done(&root2, !done))); }
                                    let (root2, text) = (root.clone(), thread[0].text.clone());
                                    if ui.small_button(crate::i18n::tr("Edit")).clicked() { action = Some(Box::new(move |app| app.comment_edit = Some((root2, text, false)))); }
                                    let root2 = root.clone();
                                    if ui.small_button(crate::i18n::tr("Delete")).clicked() { action = Some(Box::new(move |app| app.doc.delete_comment(&root2))); }
                                });
                            }
                        }
                    }
                }).response;
                // Clicking a card's background jumps to its text. A click area over the card would
                // sit on top of its buttons and swallow their clicks, so read the pointer instead.
                let editing = app.comment_edit.as_ref().is_some_and(|(id, ..)| thread.iter().any(|c| c.id == *id)) || app.reply.as_ref().is_some_and(|(id, _)| *id == root);
                let card_click = r.contains_pointer() && ui.input(|i| i.pointer.primary_clicked());
                if card_click && action.is_none() && !editing {
                    action = Some(Box::new(move |app| { app.doc.select(s, e); app.scroll_to_caret = true; app.canvas_focus = true; }));
                }
                ui.add_space(6.0);
            }
        });
        if let Some(a) = action { a(app); }
    });
}

pub fn goto_change(app: &mut WordApp, forward: bool) {
    if let Some((s, e)) = app.doc.next_revision(forward) { app.doc.select(s, e); app.scroll_to_caret = true; }
}

/// Footnotes and endnotes in document order, each editable as plain text.
pub fn notes_panel(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::right("notes").resizable(true).default_size(280.0).show(ui, |ui| {
        ui.add_space(6.0);
        crate::theme::hrow(ui, |ui| {
            ui.heading(crate::i18n::tr("Notes"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ribbon::close_button(ui, "Hide notes").clicked() { app.show_notes = false; }
            });
        });
        ui.separator();
        let labels = app.doc.note_labels();
        let refs = app.doc.note_refs();
        if refs.is_empty() { ui.weak(crate::i18n::tr("Insert → Footnote or Endnote adds a note at the caret.")); }
        let mut commit: Option<(NoteKind, String, String)> = None;
        let mut jump = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for kind in [NoteKind::Foot, NoteKind::End] {
                let these: Vec<_> = refs.iter().filter(|r| r.0 == kind).collect();
                if these.is_empty() { continue; }
                ui.label(RichText::new(if kind == NoteKind::Foot { "Footnotes" } else { "Endnotes" }).strong());
                for (k, id, pos) in these {
                    let label = labels.get(&(*k, id.clone())).cloned().unwrap_or_default();
                    crate::theme::hcols(ui, |ui| {
                        if ui.link(RichText::new(&label).strong()).on_hover_text(crate::i18n::tr("Go to reference")).clicked() { jump = Some(*pos); }
                        let editing = app.note_edit.as_ref().is_some_and(|(nk, nid, _)| nk == k && nid == id);
                        if editing {
                            let (_, _, draft) = app.note_edit.as_mut().unwrap();
                            let r = ui.add(egui::TextEdit::multiline(draft).desired_rows(2).desired_width(f32::INFINITY));
                            if !r.has_focus() && !r.lost_focus() { r.request_focus(); }
                            app.canvas_focus = false;
                            if r.lost_focus() || ui.input(|i| i.key_pressed(Key::Escape)) { commit = Some((*k, id.clone(), draft.clone())); }
                        } else {
                            let text = app.doc.note_text(*k, id);
                            let shown = if text.is_empty() { RichText::new(crate::i18n::tr("(empty — click to write)")).weak() } else { RichText::new(text.clone()) };
                            if ui.add(egui::Label::new(shown).sense(egui::Sense::click())).on_hover_text(crate::i18n::tr("Click to edit")).clicked() {
                                app.note_edit = Some((*k, id.clone(), text));
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            }
        });
        if let Some((k, id, text)) = commit { app.doc.set_note_text(k, &id, &text); app.note_edit = None; app.canvas_focus = true; }
        if let Some(pos) = jump { app.doc.select(pos, pos + 1); app.scroll_to_caret = true; app.canvas_focus = true; }
    });
}

/// Read Mode's slim bar: title, screen navigation and the way back.
pub fn read_bar(app: &mut WordApp, ui: &mut Ui) {
    egui::Panel::top("readbar").show_separator_line(false).frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(0x2B, 0x2B, 0x2B)).inner_margin(egui::Margin::symmetric(10, 6))).show(ui, |ui| {
        crate::theme::hrow(ui, |ui| {
            let white = |t: &str| RichText::new(t).color(egui::Color32::from_gray(235));
            if ui.add(egui::Button::new(white("×  Exit Read Mode")).frame(false)).clicked() { app.read_mode = false; }
            let title = app.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pages = app.layout.page_count();
                if ui.add(egui::Button::new(white("Next ►")).frame(false)).clicked() { app.read_row += 1; app.read_scroll = true; }
                ui.label(white(&crate::i18n::trf("Screen {} — {} pages", &[&(app.read_row + 1), &pages])));
                if ui.add(egui::Button::new(white("◄ Previous")).frame(false)).clicked() { app.read_row = app.read_row.saturating_sub(1); app.read_scroll = true; }
                ui.add_space(ui.available_width() / 2.0 - 120.0);
                ui.label(white(&title));
            });
        });
    });
    if ui.input(|i| i.key_pressed(Key::Escape)) { app.read_mode = false; }
}
