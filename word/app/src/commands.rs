//! Commands for "Tell me what you want to do": a name, search words and an action.

use crate::app::WordApp;

pub struct Command { pub name: &'static str, pub words: &'static str, pub icon: &'static str, pub run: fn(&mut WordApp, &egui::Context) }

macro_rules! cmd { ($name:expr, $words:expr, $icon:expr, $f:expr) => { Command { name: $name, words: $words, icon: $icon, run: $f } }; }

pub fn all() -> Vec<Command> {
    use crate::ribbon::Tab;
    use word_engine::NoteKind;
    vec![
        cmd!("Save", "save document file", "save", |a, c| { a.save(c, false); }),
        cmd!("Save As", "save copy rename", "save", |a, c| { a.save(c, true); }),
        cmd!("Export to PDF", "pdf export print", "pdf", |a, c| a.export_pdf(c)),
        cmd!("Undo", "undo back revert", "undo", |a, _| { a.doc.undo(); }),
        cmd!("Redo", "redo again", "redo", |a, _| { a.doc.redo(); }),
        cmd!("Bold", "bold strong heavy", "bold", |a, _| a.doc.toggle_bold()),
        cmd!("Italic", "italic slant", "italic", |a, _| a.doc.toggle_italic()),
        cmd!("Underline", "underline", "underline", |a, _| a.doc.toggle_underline()),
        cmd!("Font…", "font dialog typeface size", "fontDialog", |a, _| a.open_font_dialog()),
        cmd!("Find", "find search look", "find", |a, _| { a.find.open = true; a.find.focus = true; }),
        cmd!("Insert Picture", "picture image photo insert", "pictures", |a, _| a.insert_picture()),
        cmd!("Online Pictures", "online picture image web search", "changePicture", |a, _| a.open_online_pictures()),
        cmd!("Insert Table", "table grid insert", "table", |a, _| a.doc.insert_table(3, 3)),
        cmd!("Page Break", "page break new page", "pageBreak", |a, _| a.doc.insert_page_break()),
        cmd!("Blank Page", "blank page insert", "blankPage", |a, _| a.doc.insert_blank_page()),
        cmd!("Cover Page", "cover title page", "coverPage", |a, _| a.open_cover_page(word_engine::editing::CoverDesign::Banded)),
        cmd!("New Comment", "comment note review", "newComment", |a, _| a.new_comment()),
        cmd!("Footnote", "footnote note", "footnote", |a, _| a.insert_note(NoteKind::Foot)),
        cmd!("Endnote", "endnote note", "endnote", |a, _| a.insert_note(NoteKind::End)),
        cmd!("Link", "hyperlink link url", "link", |a, _| a.dialogs.link = Some(a.doc.link_at(a.doc.range().0).unwrap_or_default())),
        cmd!("Bookmark", "bookmark anchor", "bookmark", |a, _| a.insert.bookmark = Some(crate::insert::BookmarkDraft { name: String::new(), hidden: false })),
        cmd!("Edit Header", "header top page", "header", |a, _| a.edit_story(true, None)),
        cmd!("Edit Footer", "footer bottom page", "footer", |a, _| a.edit_story(false, None)),
        cmd!("Page Number", "page number numbering", "pageNumber", |a, _| a.doc.insert_page_numbers(false, 0, word_engine::Alignment::Center)),
        cmd!("Text Box", "text box frame", "textBox", |a, _| a.insert_text_box(0)),
        cmd!("WordArt", "wordart fancy text", "wordArt", |a, _| a.insert_word_art(0)),
        cmd!("Drop Cap", "drop cap initial letter", "dropCap", |a, _| a.open_drop_cap()),
        cmd!("Date & Time", "date time today", "dateTime", |a, _| a.open_date_time()),
        cmd!("Signature Line", "signature sign", "signatureLine", |a, _| a.open_signature()),
        cmd!("Equation", "equation math formula", "equation", |a, _| a.open_equation(String::new())),
        cmd!("Symbol", "symbol character special", "symbol", |a, _| a.open_symbols()),
        cmd!("Icons", "icon pictogram", "icons", |a, c| a.open_icons(c)),
        cmd!("Screenshot", "screenshot screen capture", "screenshot", |a, c| a.open_screenshot(c)),
        cmd!("Field", "field quick parts", "field", |a, _| a.open_field()),
        cmd!("Watermark", "watermark confidential draft", "watermark", |a, _| a.open_watermark()),
        cmd!("Page Color", "page color background", "pageColor", |a, _| { a.tab = Tab::Design; }),
        cmd!("Page Borders", "page border frame", "pageBorders", |a, _| a.open_page_borders()),
        cmd!("Draw", "draw ink pen", "drawTouch", |a, _| { a.tab = Tab::Draw; a.extras.draw.tool = crate::draw::Tool::Pen(0); }),
        cmd!("Zoom In", "zoom bigger", "zoomIn", |a, _| a.set_zoom(a.zoom + 0.1)),
        cmd!("Zoom Out", "zoom smaller", "zoomOut", |a, _| a.set_zoom(a.zoom - 0.1)),
        cmd!("Read Mode", "read mode view", "readMode", |a, _| a.read_mode = true),
        cmd!("Track Changes", "track changes revisions review", "trackChanges", |a, _| a.doc.track_changes = !a.doc.track_changes),
    ]
}

/// Commands whose name or search words contain every word of the query.
pub fn matches(query: &str) -> Vec<Command> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
    if words.is_empty() { return vec![]; }
    all().into_iter().filter(|c| { let hay = format!("{} {}", c.name.to_lowercase(), c.words); words.iter().all(|w| hay.contains(w.as_str())) }).take(8).collect()
}
