//! The formula bar (ui/formulabar.js): the Name Box (the selection's address; typing an address
//! or a name goes there), the cancel / enter / Insert Function buttons, and the cell's contents
//! for editing, with function suggestions while a formula is typed.

use crate::app::{Editor, Selection, SheetApp};
use crate::i18n::tr;
use egui::{vec2, Color32, FontId, Key, Modifiers, RichText, Stroke, Ui};
use qalam_kit::colors::*;
use sheet_engine::addr::{Range, MAX_COLS, MAX_ROWS};

pub fn bar(app: &mut SheetApp, ui: &mut Ui) {
    egui::Panel::top("formula-bar").show_separator_line(false).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin { left: 8, right: 8, top: 2, bottom: 4 })).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            name_box(app, ui);
            ui.add_space(4.0);
            let editing = app.edit.is_some();
            if ui.add_enabled(editing, egui::Button::new(RichText::new("✕").size(14.0)).frame(false)).on_hover_text(tr("Cancel")).clicked() { app.cancel_edit(); }
            if ui.add_enabled(editing, egui::Button::new(RichText::new("✓").size(14.0)).frame(false)).on_hover_text(tr("Enter")).clicked() { app.commit(None); }
            if ui.add(egui::Button::new(RichText::new("fx").italics().size(14.0)).frame(false)).on_hover_text(tr("Insert Function")).clicked() {
                app.dialog = Some(crate::app::Dialog::Function { query: String::new(), picked: None });
            }
            ui.add_space(4.0);
            contents(app, ui);
        });
    });
}

fn name_box(app: &mut SheetApp, ui: &mut Ui) {
    let id = egui::Id::new("name-box");
    let focused = ui.memory(|m| m.has_focus(id));
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    if !focused {
        let r = app.sel.range();
        text = if r.is_cell() { r.start().a1() } else if r.r0 == 0 && r.r1 == MAX_ROWS - 1 && r.c0 == 0 && r.c1 == MAX_COLS - 1 { "A:XFD".into() } else {
            // A selection that matches a defined name shows the name.
            let a = r.a1();
            let sheet = &app.book.sheets[app.sheet()].name;
            app.book.names.iter().find(|n| { let f = n.formula.trim_start_matches('='); f.eq_ignore_ascii_case(&format!("{}!{}", sheet_engine::addr::quote_sheet(sheet), r.a1().replace(|c: char| c == '$', ""))) || f.replace('$', "").eq_ignore_ascii_case(&format!("{}!{}", sheet_engine::addr::quote_sheet(sheet), a)) }).map(|n| n.name.clone()).unwrap_or(a)
        };
    }
    let resp = ui.add(egui::TextEdit::singleline(&mut text).id(id).desired_width(90.0).font(FontId::proportional(13.0)));
    if resp.has_focus() { ui.data_mut(|d| d.insert_temp(id, text.clone())); }
    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { go_to(app, &text); }
}

/// Selects an address ("B5", "A1:C9", "Sheet2!A1") or a defined name; an unknown name typed
/// with a range selected defines that name.
pub fn go_to(app: &mut SheetApp, text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() { return false; }
    let (sheet, body) = match t.rsplit_once('!') { Some((s, b)) => (app.book.sheet_index(s.trim_matches('\'')), b), None => (Some(app.sheet()), t) };
    if let (Some(si), Some(r)) = (sheet, sheet_engine::addr::parse_range(body)) {
        if si != app.sheet() { app.set_sheet(si); }
        app.sel = Selection { anchor: r.start(), active: r.end(), extra: vec![] };
        if r.is_cell() { app.sel.anchor = r.start(); }
        app.sel.active = if r.is_cell() { r.start() } else { r.end() };
        app.sel.anchor = r.start();
        app.reveal = true;
        return true;
    }
    if let Some(i) = app.book.name_index(t, Some(app.sheet())) {
        let f = app.book.names[i].formula.trim_start_matches('=').to_string();
        return go_to(app, &f.replace('$', ""));
    }
    // A new name for the selection.
    if app.book.valid_name(t).is_ok() {
        app.checkpoint("Define Name");
        let r = app.sel.range();
        let sheet = app.book.sheets[app.sheet()].name.clone();
        app.book.names.push(sheet_engine::book::DefinedName { name: t.to_string(), sheet: None, formula: format!("{}!{}", sheet_engine::addr::quote_sheet(&sheet), absolute(r)), hidden: false, comment: String::new() });
        app.changed();
        return true;
    }
    app.error = Some(tr("Enter a cell reference like A1, a range like A1:C9, or a valid name."));
    false
}

pub fn absolute(r: Range) -> String {
    let c = |p: sheet_engine::Pos| format!("${}${}", sheet_engine::addr::col_name(p.col), p.row + 1);
    if r.is_cell() { c(r.start()) } else { format!("{}:{}", c(r.start()), c(r.end())) }
}

fn contents(app: &mut SheetApp, ui: &mut Ui) {
    let id = egui::Id::new("formula-bar");
    let si = app.sheet();
    let width = ui.available_width();
    if let Some(e) = app.edit.as_mut() {
        if !e.focus_bar && ui.memory(|m| m.has_focus(id)) { e.focus_bar = true; }
    }
    let editing_here = app.edit.as_ref().is_some_and(|e| e.focus_bar);
    if editing_here { crate::grid::edit_keys(app, ui, true); }
    match app.edit.as_mut() {
        Some(e) => {
            let lines = e.text.lines().count().clamp(1, 6);
            let out = egui::TextEdit::multiline(&mut e.text).id(id).font(FontId::proportional(13.0)).desired_width(width).desired_rows(lines).lock_focus(true).show(ui);
            let pending = e.request_focus;
            if e.focus_bar && e.request_focus { out.response.request_focus(); e.request_focus = false; crate::grid::set_cursor(ui.ctx(), id, e.cursor); }
            if out.response.has_focus() && !pending { if let Some(c) = out.cursor_range { if usize::from(c.primary.index) != e.cursor { e.cursor = usize::from(c.primary.index); e.pointing = None; } } }
            if out.response.changed() && !pending { e.pointing = None; }
            let anchor = out.response.rect;
            suggestions(app, ui, anchor);
        }
        None => {
            let mut text = app.book.edit_text(si, app.sel.active);
            let resp = ui.add(egui::TextEdit::singleline(&mut text).id(id).font(FontId::proportional(13.0)).desired_width(width));
            if resp.gained_focus() || resp.changed() {
                let p = app.sel.active;
                let cursor = text.chars().count();
                app.edit = Some(Editor { pos: p, sheet: si, text, typed: false, cursor, pointing: None, focus_bar: true, request_focus: true });
            }
        }
    }
}

/// The function name being typed at the cursor, and where it starts.
fn partial_name(e: &Editor) -> Option<(String, usize)> {
    if !e.text.starts_with('=') { return None; }
    let chars: Vec<char> = e.text.chars().collect();
    let end = e.cursor.min(chars.len());
    let mut start = end;
    while start > 1 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '.' || chars[start - 1] == '_') { start -= 1; }
    if start == end { return None; }
    // Not inside quotes.
    if chars[..start].iter().filter(|c| **c == '"').count() % 2 == 1 { return None; }
    let word: String = chars[start..end].iter().collect();
    if word.chars().next().is_some_and(|c| c.is_ascii_digit()) { return None; }
    Some((word.to_uppercase(), start))
}

/// The function the cursor is inside the parentheses of, and which argument.
fn current_call(e: &Editor) -> Option<(String, usize)> {
    let chars: Vec<char> = e.text.chars().take(e.cursor).collect();
    let mut depth = 0i32; let mut arg = 0usize; let mut quoted = false;
    for i in (0..chars.len()).rev() {
        match chars[i] {
            '"' => quoted = !quoted,
            ')' if !quoted => depth += 1,
            ',' | ';' if !quoted && depth == 0 => arg += 1,
            '(' if !quoted => {
                if depth == 0 {
                    let mut s = i;
                    while s > 0 && (chars[s - 1].is_alphanumeric() || chars[s - 1] == '.' || chars[s - 1] == '_') { s -= 1; }
                    let name: String = chars[s..i].iter().collect::<String>().to_uppercase();
                    return (!name.is_empty()).then_some((name, arg));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Function names matching what is typed, and the argument hint of the function being filled in.
pub fn suggestions(app: &mut SheetApp, ui: &mut Ui, anchor: egui::Rect) {
    let Some(e) = app.edit.as_ref() else { return };
    let pos = anchor.left_bottom() + vec2(0.0, 2.0);
    if let Some((word, start)) = partial_name(e) {
        let names: Vec<&'static str> = sheet_engine::formula::functions::names().into_iter().filter(|n| n.starts_with(&word)).take(10).collect();
        if !names.is_empty() && !(names.len() == 1 && names[0] == word) {
            let sel_id = egui::Id::new("fn-suggest");
            let mut sel: usize = ui.data(|d| d.get_temp(sel_id)).unwrap_or(0).min(names.len() - 1);
            let (down, up, tab) = ui.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::Tab)));
            if down { sel = (sel + 1) % names.len(); }
            if up { sel = (sel + names.len() - 1) % names.len(); }
            ui.data_mut(|d| d.insert_temp(sel_id, sel));
            let mut pick = tab.then_some(names[sel]);
            egui::Area::new(egui::Id::new("fn-suggest-area")).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(c_card()).stroke(Stroke::new(1.0, c_line())).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    for (i, n) in names.iter().enumerate() {
                        let r = ui.selectable_label(i == sel, RichText::new(*n).size(13.0));
                        if r.clicked() { pick = Some(n); }
                    }
                    if let Some(h) = crate::dialogs::function_help(names[sel]) { ui.separator(); ui.label(RichText::new(h.1).size(11.5).color(c_ink2())); }
                });
            });
            if let Some(n) = pick {
                let e = app.edit.as_mut().unwrap();
                let chars: Vec<char> = e.text.chars().collect();
                let end = e.cursor.min(chars.len());
                let mut out: String = chars[..start].iter().collect();
                out.push_str(n); out.push('(');
                out.extend(&chars[end..]);
                e.text = out;
                e.cursor = start + n.len() + 1;
                e.request_focus = true;
            }
            return;
        }
    }
    if let Some((name, arg)) = current_call(e) {
        if let Some((sig, _)) = crate::dialogs::function_help(&name) {
            egui::Area::new(egui::Id::new("fn-hint")).order(egui::Order::Foreground).fixed_pos(pos).interactable(false).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(c_card()).stroke(Stroke::new(1.0, c_line())).show(ui, |ui| {
                    // The argument being typed in bold.
                    let inner = sig.split_once('(').map(|(_, r)| r.trim_end_matches(')')).unwrap_or("");
                    let args: Vec<&str> = inner.split(", ").collect();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(RichText::new(format!("{name}(")).size(12.0));
                        for (i, a) in args.iter().enumerate() {
                            let rt = RichText::new(*a).size(12.0);
                            ui.label(if i == arg.min(args.len() - 1) { rt.strong().color(Color32::from_rgb(0x21, 0x73, 0x46)) } else { rt });
                            if i + 1 < args.len() { ui.label(RichText::new(", ").size(12.0)); }
                        }
                        ui.label(RichText::new(")").size(12.0));
                    });
                });
            });
        }
    }
}
