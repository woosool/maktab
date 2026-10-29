//! The right-click menu, as in Word: a mini toolbar (font, size, styles, bold/italic/underline,
//! highlight, color, lists, comment, alignment, spacing) above context items — spelling
//! suggestions, cut/copy/paste options, Font and Paragraph, Search, Synonyms, Translate, Link,
//! New Comment, and table or picture commands. Items can be hidden in Options ▸ Context Menu.

use crate::app::WordApp;
use crate::ribbon::{c_ink, c_ink3, menu_item, small};
use egui::{vec2, Color32, RichText, Ui};
use word_engine::{Alignment, RunProps};

/// Context menu items that can be hidden: (id, label).
pub const ITEMS: &[(&str, &str)] = &[
    ("toolbar", "Mini toolbar"), ("cut", "Cut"), ("copy", "Copy"), ("paste", "Paste Options"), ("font", "Font…"), ("paragraph", "Paragraph…"),
    ("search", "Search"), ("synonyms", "Synonyms"), ("translate", "Translate"), ("link", "Link"), ("comment", "New Comment"), ("table", "Table commands"),
];

fn shown(app: &WordApp, id: &str) -> bool { !app.extras.options.hidden_context.contains(id) }

fn row(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> bool { small(ui, icon, Some(label), false, enabled, "").clicked() }

/// The mini toolbar: two rows of formatting buttons.
fn toolbar(app: &mut WordApp, ui: &mut Ui) {
    let f = app.doc.current_format();
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 4)).show(ui, |ui| {
        crate::theme::IN_MENU.with(|m| m.set(false));
        ui.spacing_mut().item_spacing = vec2(2.0, 4.0);
        crate::theme::hrow(ui, |ui| {
            crate::ribbon::font_picker(app, ui, &f.font);
            crate::ribbon::size_picker(app, ui, f.size);
            if small(ui, "fontGrow", None, false, true, "Increase Font Size").clicked() { app.doc.grow_font(true); }
            if small(ui, "fontShrink", None, false, true, "Decrease Font Size").clicked() { app.doc.grow_font(false); }
            let painting = app.format_painter.is_some();
            if small(ui, "formatPainter", None, painting, true, "Format Painter").clicked() { app.format_painter = if painting { None } else { Some(app.doc.current_run_props()) }; ui.close(); }
            let s = small(ui, "styleGallery", Some("Styles"), false, true, "Styles");
            crate::theme::menu(&s, |ui| {
                for id in ["Normal", "Title", "Heading1", "Heading2", "Heading3", "Quote"] {
                    let name = app.doc.styles.get(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string());
                    if ui.selectable_label(app.doc.paragraph_style() == id, crate::i18n::tr(&(name))).clicked() { app.doc.set_paragraph_style(id); ui.close(); }
                }
            });
        });
        crate::theme::hrow(ui, |ui| {
            if small(ui, "bold", None, f.bold, true, "Bold").clicked() { app.doc.toggle_bold(); }
            if small(ui, "italic", None, f.italic, true, "Italic").clicked() { app.doc.toggle_italic(); }
            if small(ui, "underline", None, f.underline, true, "Underline").clicked() { app.doc.toggle_underline(); }
            if small(ui, "highlight", None, false, true, "Text Highlight Color").clicked() { let h = app.last_highlight.clone(); app.doc.set_run_props(RunProps { highlight: Some(Some(h)), ..Default::default() }); }
            let c = app.last_color;
            let r = small(ui, "textColor", None, false, true, "Font Color");
            ui.painter().rect_filled(egui::Rect::from_min_size(r.rect.left_bottom() + vec2(5.0, -5.0), vec2(16.0, 3.0)), 0.0, Color32::from_rgb(c.0, c.1, c.2));
            if r.clicked() { app.doc.set_run_props(RunProps { color: Some(Some(c)), ..Default::default() }); }
            if small(ui, "bullets", None, false, true, "Bullets").clicked() { app.doc.toggle_list(word_engine::numbering::ListKind::Bullet); }
            if small(ui, "numbers", None, false, true, "Numbering").clicked() { app.doc.toggle_list(word_engine::numbering::ListKind::Number); }
            if small(ui, "newComment", None, false, true, "New Comment").clicked() { app.new_comment(); ui.close(); }
            let center = app.doc.current_para_format().alignment == Alignment::Center;
            if small(ui, "alignCenter", None, center, true, "Center").clicked() { app.doc.set_alignment(if center { Alignment::Left } else { Alignment::Center }); }
            let ls = small(ui, "lineSpacing", None, false, true, "Line and Paragraph Spacing");
            crate::theme::menu(&ls, |ui| crate::ribbon::line_spacing_menu(app, ui));
        });
        crate::theme::IN_MENU.with(|m| m.set(true));
    });
}

pub fn menu(app: &mut WordApp, ui: &mut Ui) {
    crate::theme::full_width(ui, |ui| {
        ui.set_width(330.0);
        let picture = app.selected_picture().map(|p| p.0).is_some();
        let object = app.selected_object().map(|o| o.0);
        if shown(app, "toolbar") && !picture && object.is_none() { toolbar(app, ui); ui.separator(); }
        if crate::review::context_items(app, ui) {}
        let ctx = ui.ctx().clone();
        let has = !app.doc.selection.is_caret();
        if shown(app, "cut") && row(ui, "cut", "Cut", has) { app.cut(&ctx); ui.close(); }
        if shown(app, "copy") && row(ui, "copy", "Copy", has) { app.copy(&ctx); ui.close(); }
        if shown(app, "paste") {
            ui.label(RichText::new(crate::i18n::tr("Paste Options:")).strong().color(c_ink()));
            crate::theme::hrow(ui, |ui| {
                crate::theme::IN_MENU.with(|m| m.set(false));
                ui.add_space(26.0);
                if small(ui, "paste", None, false, true, "Keep Source Formatting").clicked() { app.paste_from_system(); ui.close(); }
                if small(ui, "pasteText", None, false, true, "Merge Formatting").clicked() { app.paste_merged(); ui.close(); }
                if small(ui, "clearFormat", None, false, true, "Keep Text Only").clicked() { app.paste_plain(); ui.close(); }
                crate::theme::IN_MENU.with(|m| m.set(true));
            });
        }
        ui.separator();
        if picture || object.is_some() { picture_items(app, ui, object); return; }
        if shown(app, "font") && row(ui, "fontDialog", "Font…", true) { app.open_font_dialog(); ui.close(); }
        if shown(app, "paragraph") && row(ui, "paragraphDialog", "Paragraph…", true) { crate::ui::open_paragraph_dialog(app); ui.close(); }
        // The word or selection under the caret, for Search and Synonyms.
        let word = { let t = app.doc.selected_text(); if t.trim().is_empty() { let (a, b) = app.doc.word_at(app.doc.range().0); app.doc.text_between(a, b) } else { t } };
        let word = word.trim().chars().take(40).collect::<String>();
        if shown(app, "search") || shown(app, "synonyms") { ui.separator(); }
        if shown(app, "search") && !word.is_empty() && row(ui, "find", &format!("Search \"{word}\""), true) {
            app.show_outline = true;
            app.extras.nav.query = word.clone();
            app.extras.nav.tab = crate::nav::NavTab::Results;
            ui.close();
        }
        if shown(app, "synonyms") && !word.is_empty() {
            crate::theme::submenu(ui, RichText::new(crate::i18n::tr("Synonyms")).color(c_ink()), |ui| synonyms(app, ui, &word));
        }
        if shown(app, "translate") {
            crate::theme::submenu(ui, RichText::new(crate::i18n::tr("Translate")).color(c_ink()), |ui| {
                for (code, name) in crate::review::LANGUAGES { if ui.button(crate::i18n::tr(&*name)).clicked() { app.start_translate(code, false); ui.close(); } }
            });
        }
        if shown(app, "link") {
            ui.separator();
            let link = app.doc.link_at(app.doc.range().0);
            crate::theme::submenu(ui, RichText::new(crate::i18n::tr("Link")).color(c_ink()), |ui| {
                if ui.button(if link.is_some() { "Edit Link…" } else { "Insert Link…" }).clicked() { app.dialogs.link = Some(link.clone().unwrap_or_default()); ui.close(); }
                if let Some(l) = &link {
                    if ui.button(crate::i18n::tr("Open Link")).clicked() { open_url(l); ui.close(); }
                    if ui.button(crate::i18n::tr("Copy Link")).clicked() { ui.ctx().copy_text(l.clone()); ui.close(); }
                    if ui.button(crate::i18n::tr("Remove Link")).clicked() { app.doc.set_link(None); ui.close(); }
                }
            });
        }
        if shown(app, "comment") && row(ui, "newComment", "New Comment", true) { app.new_comment(); ui.close(); }
        if shown(app, "table") && app.doc.current_cell().is_some() {
            ui.separator();
            crate::theme::submenu(ui, RichText::new(crate::i18n::tr("Insert")).color(c_ink()), |ui| {
                if menu_item(ui, "insertAbove", "Insert Rows Above") { app.doc.insert_table_row(false); ui.close(); }
                if menu_item(ui, "insertBelow", "Insert Rows Below") { app.doc.insert_table_row(true); ui.close(); }
                if menu_item(ui, "insertLeft", "Insert Columns to the Left") { app.doc.insert_table_column(false); ui.close(); }
                if menu_item(ui, "insertRight", "Insert Columns to the Right") { app.doc.insert_table_column(true); ui.close(); }
                if menu_item(ui, "table", "Nested Table…") { app.dialogs.table = Some((2, 2)); ui.close(); }
            });
            crate::theme::submenu(ui, RichText::new(crate::i18n::tr("Delete")).color(c_ink()), |ui| {
                use word_engine::model::TablePart;
                if menu_item(ui, "deleteRow", "Delete Rows") { app.doc.delete_table_part(TablePart::Row); ui.close(); }
                if menu_item(ui, "deleteColumn", "Delete Columns") { app.doc.delete_table_part(TablePart::Column); ui.close(); }
                if menu_item(ui, "deleteTable", "Delete Table") { app.doc.delete_table_part(TablePart::Table); ui.close(); }
            });
            if row(ui, "mergeCells", "Merge Cells", true) { app.doc.merge_cells(); ui.close(); }
            if row(ui, "splitCells", "Split Cells", true) { app.doc.split_cell(2); ui.close(); }
            crate::theme::submenu(ui, RichText::new(crate::i18n::tr("AutoFit")).color(c_ink()), |ui| {
                use word_engine::tables::AutoFit;
                for (m, n) in [(AutoFit::Contents, "AutoFit to Contents"), (AutoFit::Window, "AutoFit to Window"), (AutoFit::Fixed, "Fixed Column Width")] { if ui.button(crate::i18n::tr(&n)).clicked() { app.doc.autofit(m); ui.close(); } }
            });
            if row(ui, "tableProperties", "Table Properties…", true) { app.open_table_properties(); ui.close(); }
        }
        ui.add_space(2.0);
        ui.label(RichText::new(crate::i18n::tr("Customize this menu in File ▸ Options")).small().color(c_ink3()));
    });
}

fn picture_items(app: &mut WordApp, ui: &mut Ui, object: Option<usize>) {
    use crate::picture::Turn;
    let picture = app.selected_picture().map(|p| p.0);
    if let Some(pos) = object { if row(ui, "editShape", "Edit…", true) { app.edit_object(pos); ui.close(); } }
    if let Some(pos) = picture {
        if row(ui, "changePicture", "Change Picture…", true) { app.change_picture(); ui.close(); }
        if row(ui, "crop", "Crop…", true) { app.open_crop(&ui.ctx().clone()); ui.close(); }
        if row(ui, "altText", "Edit Alt Text…", true) { app.open_alt_text(); ui.close(); }
        if row(ui, "resetPicture", "Reset Picture", true) { app.doc.reset_picture(pos); ui.close(); }
    }
    crate::theme::submenu(ui, RichText::new(crate::i18n::tr("Wrap Text")).color(c_ink()), |ui| crate::picture::wrap_menu(app, ui));
    if row(ui, "layoutOptions", "Size and Position…", true) { app.open_layout(); ui.close(); }
    crate::theme::submenu(ui, RichText::new(crate::i18n::tr("Rotate")).color(c_ink()), |ui| {
        for (name, t) in [("Rotate Right 90°", Turn::Right), ("Rotate Left 90°", Turn::Left), ("Flip Vertical", Turn::FlipV), ("Flip Horizontal", Turn::FlipH)] { if ui.button(crate::i18n::tr(&name)).clicked() { app.turn_selection(t); ui.close(); } }
    });
    if row(ui, "caption", "Insert Caption…", true) { app.extras.refs.caption = Some(crate::refs::CaptionDraft { label: "Figure".into(), text: String::new(), above: false }); ui.close(); }
    if row(ui, "download", "Save as Picture…", true) { app.save_as_picture(); ui.close(); }
    if row(ui, "shapeEffects", if picture.is_some() { "Format Picture" } else { "Format Shape" }, true) {
        app.tab = match app.selected_object() { Some((_, word_engine::objects::Object::Chart(_))) => crate::ribbon::Tab::ChartDesign, Some((_, word_engine::objects::Object::Diagram(_))) => crate::ribbon::Tab::SmartArt, Some(_) => crate::ribbon::Tab::ShapeFormat, None => crate::ribbon::Tab::PictureFormat };
        ui.close();
    }
}

/// Synonyms submenu: looked up once per word, shown when they arrive.
fn synonyms(app: &mut WordApp, ui: &mut Ui, word: &str) {
    let st = &mut app.extras.options.synonyms;
    if st.as_ref().is_none_or(|s| s.0 != word) {
        let q = crate::insert::encode(word);
        *st = Some((word.to_string(), crate::insert::Job::spawn(&app.ctx, move || {
            let v = crate::insert::get_json(&format!("https://api.datamuse.com/words?rel_syn={q}&max=10"))?;
            Ok(v.as_array().into_iter().flatten().filter_map(|x| x["word"].as_str().map(String::from)).collect())
        }), None));
    }
    let Some((_, job, result)) = st else { return };
    if let Some(r) = job.take() { *result = Some(r); }
    match result {
        None => { crate::theme::hrow(ui, |ui| { ui.spinner(); ui.label(crate::i18n::tr("Looking up…")); }); ui.ctx().request_repaint_after(std::time::Duration::from_millis(150)); }
        Some(Err(_)) => { ui.label(RichText::new(crate::i18n::tr("Synonyms need an internet connection.")).weak()); }
        Some(Ok(list)) if list.is_empty() => { ui.label(RichText::new(crate::i18n::tr("No suggestions")).weak()); }
        Some(Ok(list)) => {
            let list = list.clone();
            for w in list {
                if ui.button(crate::i18n::tr(&w)).clicked() {
                    let (s, e) = app.doc.range();
                    if s == e { let (a, b) = app.doc.word_at(s); app.doc.select(a, b); }
                    app.doc.replace(&w);
                    ui.close();
                }
            }
        }
    }
    ui.separator();
    if ui.button(crate::i18n::tr("Thesaurus…")).clicked() { app.open_thesaurus(); ui.close(); }
}

pub fn open_url(url: &str) {
    let cmd = if cfg!(target_os = "macos") { "open" } else if cfg!(target_os = "windows") { "explorer" } else { "xdg-open" };
    let _ = std::process::Command::new(cmd).arg(url).spawn();
}
