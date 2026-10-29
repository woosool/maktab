//! Headless GUI tests: the real app driven by egui_kittest and rendered with wgpu.

use crate::app::WordApp;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

fn out_dir() -> std::path::PathBuf {
    let dir = std::env::var_os("WORD_SNAPSHOTS").map(std::path::PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("word-snapshots"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn harness(path: Option<std::path::PathBuf>) -> Harness<'static, WordApp> {
    // Tests never read or write the user's settings, macros or dictionary.
    static CONFIG: std::sync::Once = std::sync::Once::new();
    CONFIG.call_once(|| {
        let dir = std::env::temp_dir().join(format!("word-test-config-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // SAFETY: set once, before any test reads the environment.
        unsafe { std::env::set_var("WORD_CONFIG_DIR", dir) };
    });
    // Document text may contain characters (emoji) no bundled font covers; the app draws tofu for
    // those. The UI chrome is checked strictly by the blank-document test.
    let mut builder = Harness::builder().with_size([1500.0, 800.0]).with_step_dt(1.0 / 60.0).wgpu();
    if path.is_some() { builder = builder.allow_missing_glyphs(); }
    builder.build_eframe(move |cc| WordApp::new(cc, path))
}

fn save(h: &mut Harness<'_, WordApp>, name: &str) {
    match h.render() {
        Ok(img) => { img.save(out_dir().join(format!("{name}.png"))).unwrap(); }
        Err(e) => eprintln!("render unavailable: {e}"),
    }
}

fn click(h: &mut Harness<'_, WordApp>, pos: Pos2) {
    h.event(Event::PointerMoved(pos));
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn typing_formatting_and_undo() {
    let mut h = harness(None);
    h.run_steps(3);
    for word in ["Hello", " ", "world"] { h.event(Event::Text(word.into())); }
    h.run_steps(2);
    assert_eq!(h.state().doc.text(), "Hello world");
    // Select "world" with Shift+Left ×5 and make it bold.
    for _ in 0..5 { h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft); }
    h.run_steps(2);
    assert_eq!(h.state().doc.range(), (6, 11));
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    assert!(h.state().doc.current_format().bold);
    h.key_press(Key::End);
    h.key_press(Key::Enter);
    h.event(Event::Text("Second paragraph".into()));
    h.run_steps(2);
    assert_eq!(h.state().doc.paragraphs.len(), 2);
    save(&mut h, "typing");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().doc.text(), "Hello world\n");
}

#[test]
fn opens_document_and_clicks_place_caret() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../documents/public/fixtures/tables.docx");
    if !fixture.exists() { return; }
    let mut h = harness(Some(fixture));
    h.run_steps(4);
    assert!(h.state().layout.page_count() >= 1);
    save(&mut h, "tables");
    // Clicking in the first page's body moves the caret off the document start.
    let before = h.state().doc.selection.focus;
    let last = h.state().layout.lines.iter().filter(|l| l.page == 0).last().unwrap().clone();
    let _ = before;
    click(&mut h, Pos2::new(560.0, 300.0));
    let after = h.state().doc.selection.focus;
    assert!(after <= h.state().doc.len() && last.end <= h.state().doc.len());
}

#[test]
fn comment_on_selection() {
    let mut h = harness(None);
    h.run_steps(3);
    h.event(Event::Text("Please review this sentence.".into()));
    h.run_steps(2);
    for _ in 0..9 { h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft); }
    h.key_press_modifiers(Modifiers { alt: true, ..Modifiers::COMMAND }, Key::M);
    h.run_steps(3);
    assert!(h.state().show_comments && h.state().comment_edit.is_some());
    h.event(Event::Text("Shorter, please.".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(3);
    let doc = &h.state().doc;
    assert_eq!(doc.comments.len(), 1);
    assert_eq!(doc.comments[0].text, "Shorter, please.");
    assert_eq!(doc.comment_anchors()[0].1..doc.comment_anchors()[0].2, 19..28);
    assert_eq!(doc.text(), "Please review this sentence.", "typing went to the comment, not the document");
    save(&mut h, "comments");
}

#[test]
fn track_changes_markup() {
    let mut h = harness(None);
    h.run_steps(3);
    h.event(Event::Text("The old plan works.".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers { shift: true, ..Modifiers::COMMAND }, Key::E);
    h.run_steps(2);
    assert!(h.state().doc.track_changes);
    // Select "old" and type over it.
    h.state_mut().doc.select(4, 7);
    h.event(Event::Text("new".into()));
    h.run_steps(3);
    assert_eq!(h.state().doc.text(), "The oldnew plan works.");
    assert_eq!(h.state().doc.revisions().len(), 2);
    save(&mut h, "track-changes");
    let n = h.state().doc.len();
    h.state_mut().doc.resolve_revisions(true, 0, n, true);
    h.run_steps(2);
    assert_eq!(h.state().doc.text(), "The new plan works.");
}

#[test]
fn footnote_insert_and_edit() {
    let mut h = harness(None);
    h.run_steps(3);
    h.event(Event::Text("A sourced claim.".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers { alt: true, ..Modifiers::COMMAND }, Key::F);
    h.run_steps(3);
    assert!(h.state().show_notes && h.state().note_edit.is_some());
    h.event(Event::Text("Smith 2024, p. 12.".into()));
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(3);
    let doc = &h.state().doc;
    assert_eq!(doc.notes.len(), 1);
    assert_eq!(doc.note_text(word_engine::NoteKind::Foot, &doc.notes[0].id), "Smith 2024, p. 12.");
    save(&mut h, "footnote");
}

#[test]
fn ribbon_tabs_render() {
    use crate::ribbon::Tab;
    let mut h = harness(None);
    h.event(Event::Text("Ribbon".into()));
    for (tab, name) in [(Tab::Home, "home"), (Tab::Insert, "insert"), (Tab::Draw, "draw"), (Tab::Design, "design"), (Tab::Layout, "layout"), (Tab::References, "references"), (Tab::Mailings, "mailings"), (Tab::Review, "review"), (Tab::View, "view")] {
        h.state_mut().tab = tab;
        h.run_steps(3);
        save(&mut h, &format!("tab-{name}"));
    }
    h.state_mut().doc.insert_table(2, 2);
    h.state_mut().tab = Tab::Table;
    h.run_steps(3);
    assert!(h.state().tab == Tab::Table, "the Table tab appears in a table");
    save(&mut h, "tab-table");
}

fn press(h: &mut Harness<'_, WordApp>, pos: Pos2, modifiers: Modifiers) {
    h.event(Event::PointerMoved(pos));
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers });
    h.step();
}
fn release(h: &mut Harness<'_, WordApp>, pos: Pos2, modifiers: Modifiers) {
    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers });
    h.run_steps(2);
}
fn drag_to(h: &mut Harness<'_, WordApp>, from: Pos2, to: Pos2, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    press(h, from, modifiers);
    for k in 1..=6 { let p = from + (to - from) * (k as f32 / 6.0); h.event(Event::PointerMoved(p)); h.step(); }
    release(h, to, modifiers);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
}

fn typed(text: &str) -> Harness<'static, WordApp> {
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text(text.into()));
    h.run_steps(3);
    h
}

#[test]
fn drag_and_drop_moves_and_copies_text() {
    let mut h = typed("alpha beta gamma");
    h.state_mut().doc.select(0, 6);
    h.run_steps(2);
    // Drag "alpha " to the end.
    let (from, to) = (h.state().screen_of(2), h.state().screen_of(16) + egui::vec2(2.0, 0.0));
    drag_to(&mut h, from, to, Modifiers::NONE);
    assert_eq!(h.state().doc.text(), "beta gammaalpha ");
    assert_eq!(h.state().doc.selected_text(), "alpha ");
    // Option/Ctrl-drag copies.
    let copy = if cfg!(target_os = "macos") { Modifiers::ALT } else { Modifiers::CTRL };
    let (from, to) = (h.state().screen_of(12), h.state().screen_of(0));
    drag_to(&mut h, from, to, copy);
    assert_eq!(h.state().doc.text(), "alpha beta gammaalpha ");
    // A plain click inside a selection just places the caret.
    let p = h.state().screen_of(3);
    press(&mut h, p, Modifiers::NONE);
    release(&mut h, p, Modifiers::NONE);
    assert!(h.state().doc.selection.is_caret());
}

#[test]
fn word_drag_margin_and_paragraph_moves() {
    let mut h = typed("one two three four");
    h.key_press(Key::Enter);
    h.event(Event::Text("second line".into()));
    h.run_steps(3);
    // Double-click "two", then drag into "four": whole words are selected.
    let p = h.state().screen_of(5);
    press(&mut h, p, Modifiers::NONE); release(&mut h, p, Modifiers::NONE);
    press(&mut h, p, Modifiers::NONE);
    for k in 1..=4 { let q = p + (h.state().screen_of(15) - p) * (k as f32 / 4.0); h.event(Event::PointerMoved(q)); h.step(); }
    let q = h.state().screen_of(15);
    release(&mut h, q, Modifiers::NONE);
    assert_eq!(h.state().doc.selected_text(), "two three four");
    // A click in the left margin selects the line.
    let line = h.state().screen_of(20);
    let margin = egui::pos2(line.x - 40.0, line.y);
    h.run_steps(40); // let the double-click window pass
    press(&mut h, margin, Modifiers::NONE); release(&mut h, margin, Modifiers::NONE);
    assert_eq!(h.state().doc.selected_text(), "second line");
    // Move the paragraph up.
    let mods = if cfg!(target_os = "macos") { Modifiers { ctrl: true, shift: true, ..Default::default() } } else { Modifiers { alt: true, shift: true, ..Default::default() } };
    h.key_press_modifiers(mods, Key::ArrowUp);
    h.run_steps(2);
    assert_eq!(h.state().doc.text(), "second line\none two three four");
}

#[test]
fn table_cells_select_merge_and_resize() {
    let mut h = harness(None);
    h.run_steps(2);
    h.state_mut().doc.insert_table(3, 3);
    for k in 0..9 {
        h.event(Event::Text(format!("c{k}")));
        if k < 8 { h.key_press(Key::Tab); }
    }
    h.run_steps(3);
    // Drag from cell c0 to c4: the 2×2 block is selected.
    let pos = |h: &Harness<'_, WordApp>, t: &str| { let d = &h.state().doc; d.paragraph_start(d.paragraphs.iter().position(|p| p.text() == t).unwrap()) + 1 };
    let (a, b) = (h.state().screen_of(pos(&h, "c0")), h.state().screen_of(pos(&h, "c4")));
    drag_to(&mut h, a, b, Modifiers::NONE);
    let block = h.state().doc.cell_block().unwrap();
    assert_eq!((block.r0, block.r1, block.c0, block.c1), (0, 1, 0, 1));
    h.state_mut().tab = crate::ribbon::Tab::Table;
    h.run_steps(3);
    save(&mut h, "table-select");
    assert!(h.state_mut().doc.merge_cells());
    h.run_steps(3);
    // Drag the first column's right border 60px to the right.
    let cells = h.state().layout.cells.clone();
    let c = cells.iter().find(|c| c.row == 2 && c.col == 0).unwrap();
    let before = h.state().doc.tables[0].grid[0];
    let from = h.state().screen_pos(c.page, c.x1, (c.y0 + c.y1) / 2.0);
    drag_to(&mut h, from, from + egui::vec2(60.0, 0.0), Modifiers::NONE);
    let after = h.state().doc.tables[0].grid[0];
    assert!((after - before - 900).abs() < 40, "60px = 900 twips wider ({before} → {after})");
    save(&mut h, "table-merged");
}

#[test]
fn draw_erase_borders_and_formula() {
    use crate::app::TableTool;
    let mut h = harness(None);
    h.run_steps(2);
    h.state_mut().doc.insert_table(3, 2);
    for v in ["Item", "Cost", "A", "120", "B", "80"] { h.event(Event::Text(v.into())); h.key_press(Key::Tab); }
    h.run_steps(3);
    assert_eq!(h.state().doc.tables[0].rows.len(), 4, "Tab past the last cell adds a row");
    // Formula in the new row's second cell.
    let cell = h.state().doc.tables[0].rows[3].cells[1].id;
    let p = { let d = &h.state().doc; d.paragraph_start(d.paragraphs.iter().position(|p| p.cell == Some(cell)).unwrap()) };
    h.state_mut().doc.select(p, p);
    assert_eq!(h.state().doc.suggested_formula(), "=SUM(ABOVE)");
    h.state_mut().doc.insert_formula("=SUM(ABOVE)", "#,##0.00").unwrap();
    // Pen: a vertical stroke through row 0's first cell splits it into two columns.
    h.state_mut().table_tool = Some(TableTool::Pen);
    h.run_steps(2);
    let c = h.state().layout.cells.iter().find(|c| c.row == 1 && c.col == 0).unwrap().clone();
    let top = h.state().screen_pos(c.page, (c.x0 + c.x1) / 2.0, c.y0 + 2.0);
    let bottom = h.state().screen_pos(c.page, (c.x0 + c.x1) / 2.0, c.y1 - 2.0);
    drag_to(&mut h, top, bottom, Modifiers::NONE);
    assert_eq!(h.state().doc.tables[0].rows[1].cells.len(), 3);
    // A horizontal stroke through the same row's last cell splits it into two rows.
    let c = h.state().layout.cells.iter().find(|c| c.row == 1 && c.col == 2).unwrap().clone();
    let (l, r) = (h.state().screen_pos(c.page, c.x0 + 3.0, (c.y0 + c.y1) / 2.0), h.state().screen_pos(c.page, c.x1 - 3.0, (c.y0 + c.y1) / 2.0));
    drag_to(&mut h, l, r, Modifiers::NONE);
    assert_eq!(h.state().doc.tables[0].rows.len(), 5);
    // Eraser on the new vertical line merges the two halves back.
    h.state_mut().table_tool = Some(TableTool::Eraser);
    h.run_steps(2);
    let a = h.state().layout.cells.iter().find(|c| c.row == 1 && c.col == 0).unwrap().clone();
    let on_line = h.state().screen_pos(a.page, a.x1, (a.y0 + a.y1) / 2.0);
    press(&mut h, on_line, Modifiers::NONE);
    release(&mut h, on_line, Modifiers::NONE);
    assert_eq!(h.state().doc.tables[0].rows[1].cells.len(), 2);
    h.state_mut().table_tool = None;
    // Red outside border on the whole table.
    h.state_mut().doc.select_table_part(word_engine::model::TablePart::Table);
    h.state_mut().doc.apply_border_preset(word_engine::tables::BorderPreset::Outside, word_engine::Border { width: 2.25, space: 0.0, color: Some(word_engine::Color(200, 30, 30)) });
    h.state_mut().tab = crate::ribbon::Tab::Table;
    h.run_steps(3);
    let text = h.state().doc.text();
    assert!(text.contains("200.00"), "formula result inserted: {text:?}");
    save(&mut h, "table-draw");
}

#[test]
fn arabic_typing_rtl_and_arrows() {
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("مرحبا بالعالم".into()));
    h.key_press(Key::Enter);
    h.event(Event::Text("English with عربي inside, 2026.".into()));
    h.key_press(Key::Enter);
    h.event(Event::Text("Emoji 😀 and café".into()));
    h.run_steps(3);
    let first = h.state().layout.lines[0].clone();
    assert!(first.rtl, "Arabic text makes its paragraph right to left");
    // → in an RTL paragraph moves toward its start (logically backward), ← toward its end.
    h.state_mut().doc.select(3, 3);
    h.key_press(Key::ArrowRight);
    h.run_steps(1);
    assert_eq!(h.state().doc.selection.focus, 2);
    h.key_press(Key::ArrowLeft);
    h.key_press(Key::ArrowLeft);
    h.run_steps(1);
    assert_eq!(h.state().doc.selection.focus, 4);
    // Making the English paragraph right to left moves it to the right margin.
    let p = h.state().doc.paragraph_start(1) + 2;
    h.state_mut().doc.select(p, p);
    let before = h.state().layout.lines.iter().find(|l| l.paragraph == 1).unwrap().stops[0];
    h.state_mut().doc.set_paragraph_direction(true);
    h.run_steps(3);
    let after = h.state().layout.lines.iter().find(|l| l.paragraph == 1).unwrap().stops[0];
    assert!(after > before + 100.0, "the paragraph starts at the right margin ({before} → {after})");
    save(&mut h, "arabic");
}

#[test]
fn lists_ruler_and_table_handles() {
    use word_engine::numbering::NumFmt;
    let mut h = harness(None);
    h.run_steps(2);
    for (k, t) in ["First point", "Second point", "Third point"].iter().enumerate() {
        if k > 0 { h.key_press(Key::Enter); }
        h.event(Event::Text(t.to_string()));
    }
    h.run_steps(2);
    h.state_mut().doc.select_all();
    h.state_mut().doc.apply_numbering(NumFmt::UpperRoman, "%1.");
    // Tab at the start of a list item demotes it.
    let p = h.state().doc.paragraph_start(1);
    h.state_mut().doc.select(p, p);
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert_eq!(h.state().doc.para_format(&h.state().doc.paragraphs[1]).numbering.unwrap().1, 1, "Tab demotes a list item");
    h.state_mut().doc.apply_multilevel(word_engine::editing::Multilevel::Legal);
    h.run_steps(3);
    // Drag the left-indent box on the ruler 1 inch to the right (snapped to 1/16").
    h.state_mut().doc.select(p, p);
    h.run_steps(2);
    let before = h.state().doc.current_para_format().indent_left;
    let (ox, _, _, zoom) = h.state().canvas_map.clone();
    let g = h.state().doc.page.geometry();
    let ruler = h.state().ruler_rect;
    let x = ox + (crate::canvas::GAP + g.left + before as f32 / 15.0) * zoom;
    let from = egui::pos2(x, ruler.max.y - 5.0);
    drag_to(&mut h, from, from + egui::vec2(96.0 * zoom, 0.0), Modifiers::NONE);
    let after = h.state().doc.current_para_format().indent_left;
    assert!((after - before - 1440).abs() <= 90, "left indent moved one inch ({before} → {after})");
    // Click the ruler to add a tab stop at about 4".
    let tx = ox + (crate::canvas::GAP + g.left + 384.0) * zoom;
    let at = egui::pos2(tx, ruler.center().y);
    press(&mut h, at, Modifiers::NONE);
    release(&mut h, at, Modifiers::NONE);
    assert!(h.state().doc.current_para_format().tabs.iter().any(|t| (t.pos - 5760).abs() <= 90), "a left tab at 4\"");
    // A centered table, moved above the list with its handle.
    let end = h.state().doc.len();
    h.state_mut().doc.select(end, end);
    h.state_mut().doc.insert_table(2, 2);
    h.state_mut().doc.scale_table(0, 0.5);
    h.state_mut().doc.set_table_alignment(0, word_engine::Alignment::Center, 0);
    h.run_steps(3);
    let c = h.state().layout.cells.iter().find(|c| c.row == 0 && c.col == 0).unwrap().clone();
    let handle = h.state().screen_pos(c.page, c.x0, c.y0) - egui::vec2(9.0, 9.0);
    let first_line = h.state().screen_of(0);
    h.event(Event::PointerMoved(handle));
    h.run_steps(2);
    drag_to(&mut h, handle, first_line, Modifiers::NONE);
    assert_eq!(h.state().doc.paragraphs.iter().position(|p| p.cell.is_some()), Some(0), "the table moved to the top");
    save(&mut h, "home-lists-ruler");
}

#[test]
fn rtl_lists_custom_lists_effects_and_read_mode() {
    use word_engine::numbering::{ListKind, NumFmt, Script};
    let mut h = harness(None);
    h.run_steps(2);
    for (k, t) in ["البند الأول", "فرعي", "البند الثاني"].iter().enumerate() {
        if k > 0 { h.key_press(Key::Enter); }
        h.event(Event::Text(t.to_string()));
    }
    h.run_steps(2);
    h.state_mut().doc.select_all();
    h.state_mut().doc.toggle_list(ListKind::Number);
    let p = h.state().doc.paragraph_start(1);
    h.state_mut().doc.select(p, p);
    h.key_press(Key::Tab);
    h.run_steps(3);
    // The sub-item's marker is an Arabic letter, drawn with Arabic glyphs at the right.
    let doc = &h.state().doc;
    let (n, lvl) = doc.para_format(&doc.paragraphs[1]).numbering.unwrap();
    assert_eq!(doc.numbering.level(n, lvl).unwrap().fmt, NumFmt::ArabicAlpha);
    // A named custom list.
    let custom = word_engine::editing::CustomList::number("Steps", NumFmt::Decimal, "Step %1:", 1, Script::Latin);
    let end = h.state().doc.len();
    h.state_mut().doc.select(end, end);
    h.key_press(Key::Enter);
    h.event(Event::Text("Mix the colors".into()));
    h.run_steps(2);
    h.state_mut().doc.set_paragraph_direction(false);
    h.state_mut().doc.apply_custom_list(&custom);
    h.key_press(Key::Enter);
    h.event(Event::Text("Glowing heading".into()));
    h.run_steps(2);
    let s = h.state().doc.paragraph_start(h.state().doc.paragraphs.len() - 1);
    h.state_mut().doc.select(s, s + 15);
    h.state_mut().doc.set_run_props(word_engine::RunProps { size: Some(26.0), glow: Some(Some(word_engine::Glow { color: word_engine::Color(0xFF, 0xC0, 0), radius: 8.0 })), reflection: Some(true), gradient: Some(Some((word_engine::Color(0x44, 0x72, 0xC4), word_engine::Color(0xC0, 0, 0)))), ..Default::default() });
    h.run_steps(3);
    save(&mut h, "rtl-lists-effects");
    // Ruler inside a table cell: indents are measured from the cell.
    let end = h.state().doc.len();
    h.state_mut().doc.select(end, end);
    h.state_mut().doc.insert_table(1, 3);
    h.run_steps(4);
    save(&mut h, "ruler-in-cell");
    // Read Mode lays pages out side by side and ignores typing.
    let text = h.state().doc.text();
    h.state_mut().read_mode = true;
    h.run_steps(3);
    h.event(Event::Text("x".into()));
    h.run_steps(2);
    assert_eq!(h.state().doc.text(), text, "Read Mode is read-only");
    save(&mut h, "read-mode");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(!h.state().read_mode);
}

#[test]
fn define_new_number_format_dialog() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("Prepare".into()));
    h.key_press(Key::Enter);
    h.event(Event::Text("Bake".into()));
    h.run_steps(2);
    h.state_mut().doc.select_all();
    h.state_mut().open_list_dialog(crate::app::ListDialogKind::Number);
    {
        let d = h.state_mut().dialogs.list.as_mut().unwrap();
        d.list.name = "Recipe steps".into();
        d.list.levels[0].text = "Step %1 —".into();
        d.list.levels[0].fmt = word_engine::numbering::NumFmt::CardinalText;
        d.save = false;
    }
    h.run_steps(3);
    save(&mut h, "define-number-format");
    h.get_by_label("Apply").click();
    h.run_steps(3);
    assert!(h.state().dialogs.list.is_none());
    let doc = &h.state().doc;
    let (n, _) = doc.para_format(&doc.paragraphs[1]).numbering.unwrap();
    let mut c = word_engine::numbering::MarkerCounter::default();
    c.next(&doc.numbering, n, 0);
    assert_eq!(c.next(&doc.numbering, n, 0).unwrap(), "Step Two —");
}

#[test]
fn insert_objects_show_contextual_tabs() {
    use crate::ribbon::Tab;
    use word_engine::objects::{ShapeKind, Object};
    let mut h = harness(None);
    h.state_mut().tab = Tab::Insert;
    h.run_steps(3);
    save(&mut h, "insert-tab");
    h.state_mut().insert_shape(ShapeKind::Rect);
    let at = h.state().doc.range().1;
    h.run_steps(3);
    assert!(matches!(h.state().selected_object(), Some((_, Object::Shape(_)))), "the shape is selected");
    h.state_mut().tab = Tab::ShapeFormat;
    h.run_steps(3);
    assert!(h.state().tab == Tab::ShapeFormat, "Shape Format stays while a shape is selected");
    save(&mut h, "shape-format");
    h.state_mut().doc.select(at, at);
    h.run_steps(3);
    assert!(h.state().tab == Tab::Insert, "the tab closes when the shape is deselected");
}

fn object_box(h: &Harness<'_, WordApp>, pos: usize) -> (f32, f32, f32, f32) {
    h.state().layout.pages[0].items.iter().find_map(|i| match i {
        word_engine::layout::DrawItem::Image { x, y, w, h, pos: p, .. } | word_engine::layout::DrawItem::Frame { x, y, w, h, pos: p } if *p == pos => Some((*x, *y, *w, *h)),
        _ => None,
    }).expect("object on page 1")
}

#[test]
fn picture_and_shape_handles_resize_rotate_and_format() {
    use crate::ribbon::Tab;
    use word_engine::objects::{Object, ShapeKind};
    let mut h = harness(None);
    let mut img = image::RgbaImage::new(200, 100);
    for (x, _, p) in img.enumerate_pixels_mut() { *p = image::Rgba([(x % 255) as u8, 120, 200, 255]); }
    let mut png = std::io::Cursor::new(vec![]);
    image::DynamicImage::ImageRgba8(img).write_to(&mut png, image::ImageFormat::Png).unwrap();
    h.state_mut().doc.insert_image(png.into_inner(), "image/png", 200.0, 100.0);
    h.state_mut().doc.select(0, 1);
    h.state_mut().tab = Tab::PictureFormat;
    h.run_steps(3);
    assert!(h.state().tab == Tab::PictureFormat, "Picture Format shows for a selected picture");
    let zoom = h.state().zoom;

    // The right edge handle stretches the width only.
    let (x, y, w, ht) = object_box(&h, 0);
    let from = h.state().screen_pos(0, x + w, y + ht / 2.0);
    drag_to(&mut h, from, from + egui::vec2(50.0 * zoom, 0.0), Modifiers::NONE);
    let i = h.state().doc.image_at(0).unwrap().clone();
    assert!((i.width - 250.0).abs() < 1.5 && (i.height - 100.0).abs() < 0.5, "side handle: {}×{}", i.width, i.height);

    // A corner keeps the picture's aspect ratio.
    h.state_mut().doc.select(0, 1);
    h.run_steps(2);
    let (x, y, w, ht) = object_box(&h, 0);
    let from = h.state().screen_pos(0, x + w, y + ht);
    drag_to(&mut h, from, from + egui::vec2(-125.0 * zoom, 0.0), Modifiers::NONE);
    let i = h.state().doc.image_at(0).unwrap().clone();
    assert!((i.width - 125.0).abs() < 1.5 && (i.height - 50.0).abs() < 1.5, "corner handle: {}×{}", i.width, i.height);

    // Picture edits change the pixels; Reset brings the original back.
    h.state_mut().doc.select(0, 1);
    h.state_mut().picture_op(word_engine::picture::PictureOp::Grayscale);
    h.state_mut().turn_selection(crate::picture::Turn::Right);
    let i = h.state().doc.image_at(0).unwrap().clone();
    assert_eq!(i.rotation, 90.0, "Rotate Right turns the picture's frame");
    assert!((i.width - 125.0).abs() < 1.5 && (i.height - 50.0).abs() < 1.5, "its size is unchanged: {}×{}", i.width, i.height);
    assert_eq!(i.effects, vec![word_engine::picture::PictureOp::Grayscale]);
    h.run_steps(3);
    save(&mut h, "picture-format");
    h.state_mut().doc.reset_picture(0);
    assert_eq!(h.state().doc.image_at(0).unwrap().width, 200.0);

    // A shape's rotation handle turns it; Shift snaps to 15°.
    let end = h.state().doc.len();
    h.state_mut().doc.select(end, end);
    h.state_mut().insert_shape(ShapeKind::RightArrow);
    let pos = h.state().doc.range().0;
    h.run_steps(3);
    let (x, y, w, ht) = object_box(&h, pos);
    let handle = h.state().screen_pos(0, x + w / 2.0, y) - egui::vec2(0.0, 22.0);
    let center = h.state().screen_pos(0, x + w / 2.0, y + ht / 2.0);
    let to = center + egui::vec2(120.0, 3.0);
    drag_to(&mut h, handle, to, Modifiers::SHIFT);
    match h.state().doc.object_at(pos) { Some(Object::Shape(s)) => assert_eq!(s.rotation, 90.0), _ => panic!("shape") }
    // Its right-edge handle now sits below the center; dragging it down widens the shape.
    h.state_mut().doc.select(pos, pos + 1);
    h.run_steps(2);
    let (_, x, y, w, ht) = h.state().layout.object_box(pos).unwrap();
    let handle = h.state().screen_pos(0, x + w / 2.0, y + ht / 2.0 + w / 2.0);
    let zoom = h.state().zoom;
    drag_to(&mut h, handle, handle + egui::vec2(0.0, 30.0 * zoom), Modifiers::NONE);
    match h.state().doc.object_at(pos) { Some(Object::Shape(s)) => assert!((s.width - w - 30.0).abs() < 1.5 && (s.height - ht).abs() < 0.5, "{}×{} from {w}×{ht}", s.width, s.height), _ => panic!("shape") }
    h.state_mut().doc.select(pos, pos + 1);
    h.state_mut().tab = Tab::ShapeFormat;
    h.run_steps(3);
    save(&mut h, "shape-rotated");
}

#[test]
fn floating_pictures_wrap_drag_nudge_and_inline() {
    use crate::ribbon::Tab;
    use word_engine::model::Wrap;
    let mut h = typed(&"Text flows around floating pictures. ".repeat(40));
    let mut img = image::RgbaImage::new(160, 120);
    for (x, y, p) in img.enumerate_pixels_mut() { *p = image::Rgba([(x * 2 % 255) as u8, (y * 2 % 255) as u8, 180, 255]); }
    let mut png = std::io::Cursor::new(vec![]);
    image::DynamicImage::ImageRgba8(img).write_to(&mut png, image::ImageFormat::Png).unwrap();
    h.state_mut().doc.select(0, 0);
    h.state_mut().doc.insert_image(png.into_inner(), "image/png", 160.0, 120.0);
    h.state_mut().doc.select(0, 1);
    h.run_steps(2);
    h.state_mut().set_wrap(Some(Wrap::Square));
    h.state_mut().tab = Tab::PictureFormat;
    h.run_steps(3);
    let g = h.state().doc.page.geometry();
    assert_eq!(h.state().doc.anchor_at(0).map(|a| a.wrap), Some(Wrap::Square));
    let first = h.state().layout.lines[0].stops[1];
    assert!(first > g.left + 160.0, "text starts right of the picture: {first}");

    // Drag it 1.5" right and 0.5" down; text then flows on its left.
    let (_, x, y, w, ht) = h.state().layout.object_box(0).unwrap();
    let zoom = h.state().zoom;
    let from = h.state().screen_pos(0, x + w / 2.0, y + ht / 2.0);
    drag_to(&mut h, from, from + egui::vec2(144.0 * zoom, 48.0 * zoom), Modifiers::NONE);
    h.run_steps(2);
    let pos = h.state().doc.range().0;
    let (_, x2, y2, ..) = h.state().layout.object_box(pos).unwrap();
    assert!((x2 - x - 144.0).abs() < 2.0 && (y2 - y - 48.0).abs() < 2.0, "moved by the drag: ({x}, {y}) → ({x2}, {y2})");
    let beside = h.state().layout.lines.iter().find(|l| l.y > y2 + 4.0 && l.y < y2 + 60.0).unwrap();
    assert!(beside.stops.iter().all(|&s| s < x2 || s > x2 + w), "lines skip the picture's box");

    // Arrow keys nudge it.
    h.state_mut().canvas_focus = true;
    h.run_steps(1);
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    let pos = h.state().doc.range().0;
    let (_, x3, ..) = h.state().layout.object_box(pos).unwrap();
    assert!((x3 - x2 - 6.0).abs() < 1.0, "nudged 6px: {x2} → {x3}");
    save(&mut h, "float-square");

    h.state_mut().set_wrap(Some(Wrap::Behind));
    h.state_mut().doc.set_image_rotation(pos, 20.0);
    let end = h.state().doc.len();
    h.state_mut().doc.select(end, end);
    let mut shape = word_engine::objects::Shape::new(word_engine::objects::ShapeKind::RoundRect);
    shape.text = "Rotated".into(); shape.rotation = 30.0;
    h.state_mut().doc.insert_object(word_engine::objects::Object::Shape(shape));
    let sp = h.state().doc.range().0;
    h.state_mut().doc.set_anchor(sp, Some(word_engine::model::Anchor::new(Wrap::InFront, 430.0, 330.0)));
    h.state_mut().doc.select(pos, pos + 1);
    h.run_steps(3);
    save(&mut h, "float-behind");
    h.state_mut().set_wrap(None);
    h.run_steps(2);
    assert!(h.state().doc.anchor_at(pos).is_none(), "back in line with text");
}

#[test]
fn headers_footers_equations_symbols_and_text_tools() {
    use crate::ribbon::Tab;
    let mut h = typed("Once upon a time a document grew a header, a footer, equations and a drop cap. ");
    h.run_steps(2);
    let g = h.state().doc.page.geometry();
    // Double-click in the header area edits the header in place.
    let at = h.state().screen_pos(0, g.left + 60.0, g.header + 6.0);
    click(&mut h, at);
    click(&mut h, at);
    h.run_steps(3);
    assert!(h.state().extras.story.as_ref().is_some_and(|s| s.header), "double-click opens the header");
    assert!(h.state().tab == Tab::HeaderFooter);
    h.event(Event::Text("Quarterly Report".into()));
    h.run_steps(3);
    save(&mut h, "header-editing");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().extras.story.is_none());
    assert_eq!(h.state().doc.header.as_ref().map(|s| s.paragraphs[0].text()), Some("Quarterly Report".to_string()));
    let header_glyphs: String = h.state().layout.pages[0].items.iter().filter_map(|i| match i { word_engine::layout::DrawItem::Glyphs { glyphs, baseline, .. } if *baseline < g.top => Some(glyphs.iter().map(|x| x.ch).collect::<String>()), _ => None }).collect();
    assert!(header_glyphs.contains("Quarterly"), "the header shows on the page: {header_glyphs}");
    // Page numbers in the footer.
    h.state_mut().doc.insert_page_numbers(false, 1, word_engine::Alignment::Right);
    assert!(h.state().doc.footer.as_ref().unwrap().paragraphs[0].runs.iter().any(|r| r.field == Some(word_engine::Field::NumPages)));

    // An equation on its own line, a symbol, a date field.
    let end = h.state().doc.len();
    h.state_mut().doc.select(end, end);
    h.state_mut().doc.insert_paragraph();
    h.state_mut().insert_equation("x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}");
    let pos = h.state().doc.range().0;
    assert!(matches!(h.state().doc.object_at(pos), Some(word_engine::objects::Object::Equation(e)) if e.display));
    h.state_mut().doc.select(pos, pos + 1);
    h.state_mut().tab = Tab::Equation;
    h.run_steps(3);
    assert!(h.state().tab == Tab::Equation, "the Equation tab shows for a selected equation");
    save(&mut h, "equation-tab");
    let end = h.state().doc.len();
    h.state_mut().doc.select(end, end);
    h.state_mut().doc.insert_paragraph();
    h.state_mut().insert_symbol('π');
    h.state_mut().doc.replace(" — today is ");
    h.state_mut().doc.insert_field(word_engine::Field::Date("MMMM d, yyyy".into()));
    // WordArt, a text box and a drop cap on the first paragraph.
    h.state_mut().doc.insert_paragraph();
    h.state_mut().insert_word_art(3);
    h.state_mut().doc.select(0, 0);
    h.state_mut().insert_text_box(1);
    h.state_mut().insert.shape = None;
    h.state_mut().doc.select(2, 2);
    let paras = h.state().doc.paragraphs.len();
    h.state_mut().apply_drop_cap(1, 3, 0.0, None);
    assert_eq!(h.state().doc.paragraphs.len(), paras + 1, "the drop cap letter gets its own paragraph");
    h.state_mut().tab = Tab::Insert;
    h.run_steps(4);
    save(&mut h, "insert-features");
    let text: String = h.state().layout.pages[0].items.iter().filter_map(|i| match i { word_engine::layout::DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|x| x.ch).collect::<String>()), _ => None }).collect();
    let year = &crate::app::local_iso()[..4];
    assert!(text.contains(year), "the date field shows this year");
    // Everything survives saving.
    let bytes = word_engine::docx::write(&h.state().doc).unwrap();
    let back = word_engine::docx::read(&bytes).unwrap();
    assert!(back.header.is_some() && back.footer.is_some());
    assert!(back.paragraphs.iter().flat_map(|p| p.runs.iter()).any(|r| matches!(r.object, Some(word_engine::objects::Object::Equation(_)))));
    assert!(back.paragraphs.iter().any(|p| p.props.drop_cap.is_some()));
    // Tell Me finds and runs commands.
    let found = crate::commands::matches("equat");
    assert_eq!(found.first().map(|c| c.name), Some("Equation"));
    let ctx = h.ctx.clone();
    (found[0].run)(h.state_mut(), &ctx);
    assert!(h.state().extras.equation.is_some(), "Tell Me opened the equation editor");
}

#[test]
fn draw_tab_ink_eraser_lasso_and_ink_to_shape() {
    use crate::draw::Tool;
    use word_engine::objects::Object;
    let mut h = typed("Ink on the page.");
    h.state_mut().tab = crate::ribbon::Tab::Draw;
    h.state_mut().extras.draw.tool = Tool::Pen(0);
    h.run_steps(2);
    let g = h.state().doc.page.geometry();
    let p = |h: &Harness<'_, WordApp>, x: f32, y: f32| h.state().screen_pos(0, x, y);
    let (a, b) = (p(&h, g.left + 40.0, g.top + 120.0), p(&h, g.left + 300.0, g.top + 160.0));
    drag_to(&mut h, a, b, Modifiers::NONE);
    let (c, d) = (p(&h, g.left + 40.0, g.top + 260.0), p(&h, g.left + 300.0, g.top + 260.0));
    drag_to(&mut h, c, d, Modifiers::NONE);
    h.state_mut().extras.draw.tool = Tool::Pen(4);
    h.run_steps(1);
    let (e, f) = (p(&h, g.left, g.top + 10.0), p(&h, g.left + 120.0, g.top + 10.0));
    drag_to(&mut h, e, f, Modifiers::NONE);
    h.run_steps(2);
    let inks: Vec<(usize, word_engine::objects::Ink)> = {
        let mut out = vec![]; let mut pos = 0;
        for p in &h.state().doc.paragraphs { for r in &p.runs { if let Some(Object::Ink(i)) = &r.object { out.push((pos, i.clone())); } pos += r.len(); } pos += 1; }
        out
    };
    assert_eq!(inks.len(), 2, "pen ink in front, highlighter ink behind");
    assert_eq!(inks.iter().map(|i| i.1.strokes.len()).sum::<usize>(), 3);
    save(&mut h, "draw-ink");
    // The eraser removes the stroke it touches.
    h.state_mut().extras.draw.tool = Tool::Eraser;
    h.run_steps(1);
    let mid = p(&h, g.left + 170.0, g.top + 140.0);
    drag_to(&mut h, mid - egui::vec2(0.0, 10.0), mid + egui::vec2(0.0, 10.0), Modifiers::NONE);
    let strokes = |h: &Harness<'_, WordApp>| h.state().doc.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| if let Some(Object::Ink(i)) = &r.object { Some(i.strokes.len()) } else { None }).sum::<usize>();
    assert_eq!(strokes(&h), 2, "erased one stroke");
    // Lasso around the straight stroke selects it; Delete removes it.
    h.state_mut().extras.draw.tool = Tool::Lasso;
    h.run_steps(1);
    let pts = [(20.0, 230.0), (340.0, 230.0), (340.0, 290.0), (20.0, 290.0), (20.0, 232.0)];
    let at = p(&h, g.left + pts[0].0, g.top + pts[0].1);
    press(&mut h, at, Modifiers::NONE);
    for q in &pts[1..] { let at = p(&h, g.left + q.0, g.top + q.1); h.event(Event::PointerMoved(at)); h.step(); }
    let at = p(&h, g.left + pts[4].0, g.top + pts[4].1);
    release(&mut h, at, Modifiers::NONE);
    assert_eq!(h.state().extras.draw.selected.len(), 1, "the lasso selected the stroke");
    h.key_press(Key::Delete);
    h.run_steps(1);
    assert_eq!(strokes(&h), 1);
    // Ink to Shape turns a drawn box into a rectangle.
    h.state_mut().extras.draw.tool = Tool::Pen(0);
    h.state_mut().extras.draw.ink_to_shape = true;
    h.run_steps(1);
    let corners = [(100.0, 400.0), (300.0, 400.0), (300.0, 520.0), (100.0, 520.0), (101.0, 402.0)];
    let at = p(&h, g.left + corners[0].0, g.top + corners[0].1);
    press(&mut h, at, Modifiers::NONE);
    for w in corners.windows(2) { for k in 1..=8 { let t = k as f32 / 8.0; let (x, y) = (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t); let at = p(&h, g.left + x, g.top + y); h.event(Event::PointerMoved(at)); h.step(); } }
    let at = p(&h, g.left + corners[4].0, g.top + corners[4].1);
    release(&mut h, at, Modifiers::NONE);
    h.run_steps(2);
    assert!(h.state().doc.paragraphs.iter().flat_map(|p| p.runs.iter()).any(|r| matches!(&r.object, Some(Object::Shape(s)) if s.kind == word_engine::objects::ShapeKind::Rect)), "a rectangle shape");
    save(&mut h, "draw-shape");
    // Ink is saved as drawing groups and read back.
    let back = word_engine::docx::read(&word_engine::docx::write(&h.state().doc).unwrap()).unwrap();
    assert!(back.paragraphs.iter().flat_map(|p| p.runs.iter()).any(|r| matches!(&r.object, Some(Object::Ink(i)) if i.strokes.len() == 1)));
}

#[test]
fn design_tab_styles_page_color_borders_and_watermark() {
    let mut h = typed("Design");
    h.state_mut().doc.set_paragraph_style("Title");
    h.state_mut().doc.insert_paragraph();
    h.state_mut().doc.set_paragraph_style("Heading1");
    h.state_mut().doc.replace("A heading");
    h.state_mut().doc.insert_paragraph();
    h.state_mut().doc.set_paragraph_style("Normal");
    h.state_mut().doc.replace(&"Body text for the design tab. ".repeat(12));
    h.state_mut().doc.apply_style_set(3);
    h.state_mut().doc.apply_colors(&word_engine::design::COLOR_SETS[2].1);
    h.state_mut().doc.set_page_color(Some(word_engine::Color(0xFF, 0xF8, 0xE7)));
    h.state_mut().doc.set_page_borders(Some(word_engine::page::PageBorders { style: word_engine::page::BorderStyle::Double, width: 1.0, color: word_engine::Color(0x54, 0x9E, 0x39), space: 24.0, from_text: false, sides: [true; 4] }));
    h.state_mut().text_watermark("DRAFT", true);
    h.state_mut().tab = crate::ribbon::Tab::Design;
    h.state_mut().extras.styles_pane = true;
    h.run_steps(4);
    assert!(h.state().doc.watermark().is_some());
    assert!(h.state().layout.pages[0].under.iter().any(|i| matches!(i, word_engine::layout::DrawItem::Rect { color, .. } if *color == word_engine::Color(0xFF, 0xF8, 0xE7))), "the page color fills the page");
    save(&mut h, "design");
}

#[test]
fn dialogs_render() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(None);
    h.run_steps(2);
    let shots: Vec<(&str, Box<dyn Fn(&mut WordApp)>)> = vec![
        ("dialog-page-setup", Box::new(|a: &mut WordApp| a.dialogs.page_setup = Some(a.doc.page.clone()))),
        ("dialog-paragraph", Box::new(|a: &mut WordApp| crate::ui::open_paragraph_dialog(a))),
        ("dialog-columns", Box::new(|a: &mut WordApp| a.open_columns_dialog())),
        ("dialog-source", Box::new(|a: &mut WordApp| a.new_source(false))),
        ("dialog-caption", Box::new(|a: &mut WordApp| a.extras.refs.caption = Some(crate::refs::CaptionDraft { label: "Figure".into(), text: String::new(), above: false }))),
    ];
    for (name, open) in shots {
        open(h.state_mut());
        h.run_steps(4);
        save(&mut h, name);
        if name == "dialog-columns" {
            let at = Pos2::new(717.0, 404.0);
            h.event(Event::PointerMoved(at));
            h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
            h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            h.run_steps(3);
            save(&mut h, "dialog-dropdown");
            h.key_press(egui::Key::Escape);
            h.run_steps(2);
        }
        h.key_press(egui::Key::Escape);
        let a = h.state_mut();
        a.dialogs.page_setup = None; a.dialogs.paragraph = None; a.extras.refs.columns = None; a.extras.refs.source = None; a.extras.refs.caption = None;
        h.run_steps(2);
    }
}

/// Random clicks, drags, typing and shortcuts over the whole window; reports every panic the
/// app recovered from. Run with `cargo test gui_fuzz -- --ignored --nocapture`.
#[test]
#[ignore]
fn gui_fuzz() {
    use std::sync::{Arc, Mutex};
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let sink = seen.clone();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture().to_string();
        let ours: Vec<&str> = bt.lines().filter(|l| l.contains("qalam::") || l.contains("word_engine::") || l.contains("at ./") || l.contains("src/")).take(60).collect();
        sink.lock().unwrap().push(format!("{info}\n{}", ours.join("\n")));
    }));
    let seeds: u64 = std::env::var("FUZZ_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(6);
    let steps: usize = std::env::var("FUZZ_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(400);
    for seed in 1..=seeds {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut rnd = move |n: u64| { s ^= s << 13; s ^= s >> 7; s ^= s << 17; s % n.max(1) };
        let mut h = harness(None);
        if std::env::var("FUZZ_LANG").is_ok_and(|l| l == "ar") { h.state_mut().extras.options.settings.lang = crate::i18n::Lang::Ar; }
        h.run_steps(2);
        h.event(Event::Text("Fuzzing text with several words to click on.".into()));
        let mut trail = vec![];
        for _ in 0..steps {
            let kind = rnd(100);
            // Skip the File tab (it opens native pickers).
            let mut pos = Pos2::new(60.0 + rnd(1440) as f32, 4.0 + rnd(790) as f32);
            // …and the Save button (a native picker for untitled documents).
            if pos.y < 40.0 && pos.x > 1380.0 { pos.x -= 400.0; }
            match kind {
                0..=49 => { trail.push(format!("click {pos:?}")); click(&mut h, pos); }
                50..=57 => {
                    let to = Pos2::new(pos.x + rnd(200) as f32 - 100.0, pos.y + rnd(200) as f32 - 100.0);
                    trail.push(format!("drag {pos:?}->{to:?}"));
                    h.event(Event::PointerMoved(pos));
                    h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
                    h.step();
                    h.event(Event::PointerMoved(to));
                    h.step();
                    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
                    h.run_steps(2);
                }
                58..=61 => {
                    trail.push(format!("double-click {pos:?}"));
                    click(&mut h, pos); click(&mut h, pos);
                }
                62..=75 => { let t = ["a", "Hello ", "\n", "x", "مرحبا", "1"][rnd(6) as usize]; trail.push(format!("type {t:?}")); h.event(Event::Text(t.into())); h.run_steps(1); }
                76..=89 => {
                    let k = [Key::Backspace, Key::Delete, Key::Enter, Key::Tab, Key::ArrowLeft, Key::ArrowRight, Key::ArrowUp, Key::ArrowDown, Key::Escape, Key::Home, Key::End][rnd(11) as usize];
                    let shift = rnd(4) == 0;
                    trail.push(format!("key {k:?} shift={shift}"));
                    h.key_press_modifiers(if shift { Modifiers::SHIFT } else { Modifiers::NONE }, k);
                    h.run_steps(1);
                }
                _ => {
                    let k = [Key::B, Key::I, Key::U, Key::Z, Key::Y, Key::A, Key::C, Key::V, Key::X][rnd(9) as usize];
                    trail.push(format!("cmd+{k:?}"));
                    h.key_press_modifiers(Modifiers::COMMAND, k);
                    h.run_steps(1);
                }
            }
            if !seen.lock().unwrap().is_empty() {
                let p = seen.lock().unwrap().remove(0);
                println!("=== seed {seed}: PANIC\n{p}\n--- last steps:\n{}", trail.iter().rev().take(8).rev().cloned().collect::<Vec<_>>().join("\n"));
                seen.lock().unwrap().clear();
            }
        }
        println!("seed {seed} done");
    }
}

/// Post and Cancel in the comments pane respond to real pointer clicks.
#[test]
fn comment_post_and_cancel_click() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("Commented words here".into()));
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.state_mut().new_comment();
    h.run_steps(3);
    h.event(Event::Text("Looks good".into()));
    h.run_steps(2);
    let post = h.get_by_label("Post").rect().center();
    click(&mut h, post);
    h.run_steps(2);
    assert!(h.state().comment_edit.is_none(), "Post closed the editor");
    assert_eq!(h.state().doc.comments[0].text, "Looks good");
    // A second comment, cancelled while empty, disappears.
    h.state_mut().new_comment();
    h.run_steps(3);
    let cancel = h.get_by_label("Cancel").rect().center();
    click(&mut h, cancel);
    h.run_steps(2);
    assert!(h.state().comment_edit.is_none(), "Cancel closed the editor");
    assert_eq!(h.state().doc.comments.len(), 1);
    save(&mut h, "comments");
}

/// Review, View and Mailings features in the running app, with snapshots.
#[test]
fn review_view_and_mailings() {
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("Heading text".into()));
    h.run_steps(1);
    h.state_mut().doc.set_paragraph_style("Heading1");
    h.key_press(Key::Enter);
    h.event(Event::Text("We recieve teh mail and and read it every morning before work.".into()));
    h.run_steps(2);
    // Editor pane lists the issues; squiggles are drawn.
    h.state_mut().open_editor();
    h.run_steps(4);
    let words: Vec<String> = h.state().extras.review.issues.iter().map(|i| i.word.clone()).collect();
    assert!(words.contains(&"recieve".to_string()) && words.contains(&"teh".to_string()), "{words:?}");
    save(&mut h, "review-editor");
    h.state_mut().extras.review.editor_open = false;
    // A nested table with an equation inside autofit columns.
    h.state_mut().doc.insert_table(2, 3);
    h.state_mut().doc.insert_object(word_engine::objects::Object::Equation(word_engine::equation::Equation::new("(x+a)^n=\\sum_{k=0}^{n}\\binom{n}{k}x^k a^{n-k}")));
    h.state_mut().doc.move_cell(true);
    h.state_mut().doc.insert_table(2, 2);
    h.state_mut().doc.replace("Inner");
    h.run_steps(3);
    save(&mut h, "table-nested");
    // Views.
    h.state_mut().set_view(crate::view::ViewMode::Web);
    h.run_steps(4);
    assert!(h.state().doc.view_override.is_some());
    save(&mut h, "view-web");
    h.state_mut().set_view(crate::view::ViewMode::Outline);
    h.run_steps(3);
    save(&mut h, "view-outline");
    h.state_mut().set_view(crate::view::ViewMode::Print);
    h.run_steps(3);
    assert!(h.state().doc.view_override.is_none());
    // Mail merge preview.
    let src = word_engine::mailmerge::DataSource::parse_delimited("people.csv", "First Name,Last Name,City\nAda,Lovelace,London\nAlan,Turing,Manchester\n").unwrap();
    h.state_mut().doc.set_merge_source(Some(src), None);
    let end = h.state().doc.paragraph_start(0);
    h.state_mut().doc.select(end, end);
    h.state_mut().insert_merge(word_engine::Field::GreetingLine { greeting: "Dear".into(), name: word_engine::mailmerge::NameFormat::FirstLast, punctuation: ",".into() });
    h.state_mut().doc.merge_view(|m| { m.preview = true; m.record = 1; m.highlight = true; });
    h.state_mut().tab = crate::ribbon::Tab::Mailings;
    h.run_steps(4);
    save(&mut h, "mailings-preview");
}

#[test]
fn dark_mode_snapshot() {
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("Dark mode text on a dark page.".into()));
    let ctx = h.ctx.clone();
    h.state_mut().set_dark(&ctx, true);
    for (tab, name) in [(crate::ribbon::Tab::Home, "dark-home"), (crate::ribbon::Tab::View, "dark-view")] {
        h.state_mut().tab = tab;
        h.run_steps(4);
        save(&mut h, name);
    }
    h.state_mut().set_dark(&ctx, false);
}

#[test]
fn print_dialog() {
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("Printed page one.".into()));
    h.run_steps(1);
    h.state_mut().open_print();
    h.run_steps(8);
    assert!(h.state().extras.print.is_some());
    save(&mut h, "print-dialog");
    // A subset of pages (with a repeat, as Windows copies are) makes a PDF of that many pages.
    let app = h.state();
    let pdf = word_engine::pdf::export_pages(&app.doc, &app.layout, "t", &[0, 0]);
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 2"));
}

/// Record a macro by typing, then run it with its shortcut; the editor shows errors by line.
#[test]
fn macros_record_run_and_edit() {
    let mut h = harness(None);
    h.run_steps(2);
    h.state_mut().extras.macros.loaded = true; // don't touch the user's saved macros
    h.state_mut().start_macro_recording(crate::macros::RecordDraft { name: "Sign".into(), description: "Signature".into(), store: crate::macros::Store::Document, shortcut: Some(1) });
    h.event(Event::Text("Best regards".into()));
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.event(Event::Text("Ada".into()));
    h.run_steps(2);
    h.state_mut().stop_macro_recording();
    let code = h.state().doc.macros[0].code.clone();
    assert!(code.contains("type_text(\"Best regards\")") && code.contains("new_paragraph();") && code.contains("bold();"), "{code}");
    // Run it again with ⌘⌥⇧1.
    h.state_mut().doc.select_all();
    h.state_mut().doc.delete(false, false);
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT, Key::Num1);
    h.run_steps(3);
    assert_eq!(h.state().doc.text(), "Best regards\nAda");
    // A broken macro opens the editor at its line.
    let bad = word_engine::macros::Macro { name: "Broken".into(), code: "bold();\nalign(\"diagonal\");".into(), ..Default::default() };
    h.state_mut().run_macro(crate::macros::Store::Document, &bad);
    h.run_steps(4);
    assert_eq!(h.state().extras.macros.editor.as_ref().and_then(|e| e.error.as_ref()).and_then(|e| e.1), Some(2));
    save(&mut h, "macro-editor");
}

#[test]
fn navigation_pane() {
    let mut h = harness(None);
    h.run_steps(2);
    for (t, s) in [("Introduction", "Heading1"), ("Body text here.", "Normal"), ("Details", "Heading2"), ("More text.", "Normal"), ("Summary", "Heading1")] {
        h.event(Event::Text(t.into()));
        h.run_steps(1);
        h.state_mut().doc.set_paragraph_style(s);
        h.key_press(Key::Enter);
    }
    h.state_mut().tab = crate::ribbon::Tab::View;
    h.state_mut().show_outline = true;
    h.run_steps(4);
    save(&mut h, "navigation-pane");
}

#[test]
fn split_scroll_sync_and_nav_pane() {
    let mut h = harness(None);
    h.run_steps(2);
    for k in 0..40 {
        h.event(Event::Text(format!("Heading {k}")));
        h.state_mut().doc.set_paragraph_style(if k % 8 == 0 { "Heading1" } else { "Normal" });
        h.key_press(Key::Enter);
    }
    h.run_steps(2);
    let wheel = |h: &mut Harness<'_, WordApp>, at: Pos2| {
        h.event(Event::PointerMoved(at));
        h.event(Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -300.0), modifiers: Modifiers::NONE, phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel });
        h.run_steps(6);
    };
    h.state_mut().extras.view.split = true;
    h.run_steps(4);
    // Without sync, scrolling the lower view leaves the upper one.
    wheel(&mut h, Pos2::new(760.0, 650.0));
    let o = h.state().extras.view.offsets;
    assert!(o[0].y > 50.0 && o[1].y < 1.0, "{o:?}");
    // With sync, the upper view follows by the same amount.
    h.state_mut().extras.view.sync_split = true;
    let before = h.state().extras.view.offsets;
    wheel(&mut h, Pos2::new(760.0, 650.0));
    let after = h.state().extras.view.offsets;
    let (d0, d1) = (after[0].y - before[0].y, after[1].y - before[1].y);
    assert!(d0 > 50.0 && (d0 - d1).abs() < 2.0, "lower moved {d0}, upper {d1}");
    h.state_mut().extras.view.split = false;
    h.state_mut().show_outline = true;
    let ctx = h.ctx.clone();
    h.state_mut().set_dark(&ctx, true);
    h.run_steps(4);
    save(&mut h, "nav-pane-dark");
    h.state_mut().extras.nav.tab = crate::nav::NavTab::Pages;
    h.run_steps(6);
    save(&mut h, "nav-pages");
    h.state_mut().set_dark(&ctx, false);
}

#[test]
fn menus_full_width() {
    let mut h = harness(None);
    h.run_steps(2);
    click(&mut h, Pos2::new(1170.0, 21.0));
    h.event(Event::Text("s".into()));
    h.run_steps(4);
    save(&mut h, "menu-tellme");
    h.key_press(Key::Escape);
    h.state_mut().canvas_focus = true;
    h.run_steps(2);
    click(&mut h, Pos2::new(30.0, 21.0));
    h.run_steps(3);
    save(&mut h, "menu-file");
}

#[test]
fn context_menu_options_and_view_reset() {
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("Make your document look professional.".into()));
    h.run_steps(2);
    // Right-click a word: the Word-style menu with its mini toolbar.
    let at = Pos2::new(560.0, 286.0);
    h.event(Event::PointerMoved(at));
    h.event(Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(4);
    save(&mut h, "context-menu");
    h.key_press(Key::Escape);
    h.run_steps(2);
    // Options dialog, Customize Ribbon page.
    h.state_mut().extras.options.dialog = Some((2, h.state().extras.options.settings.clone()));
    h.run_steps(4);
    save(&mut h, "options-ribbon");
    h.state_mut().extras.options.dialog = None;
    // Hidden tabs and a custom tab.
    let s = &mut h.state_mut().extras.options.settings;
    s.hidden_tabs.insert("Mailings".into());
    s.custom_tab = vec!["Bold".into(), "Insert Picture".into()];
    s.custom_tab_name = "Quick".into();
    h.state_mut().tab = crate::ribbon::Tab::Custom;
    h.run_steps(4);
    save(&mut h, "custom-tab");
    // Settings round trip.
    let mut st = crate::options::Settings::default();
    st.hidden_groups.insert("Home/Voice".into());
    st.window = Some(([1000.0, 700.0], Some([40.0, 50.0]), false));
    st.save();
    let back = crate::options::Settings::load();
    assert_eq!(back.hidden_groups, st.hidden_groups);
    assert_eq!(back.window, st.window);
    // Opening a document resets Split and Web Layout.
    h.state_mut().extras.view.split = true;
    h.state_mut().set_view(crate::view::ViewMode::Web);
    let doc = word_engine::Document::new(vec![word_engine::Paragraph::new("Another file")]);
    h.state_mut().set_document(doc, None);
    h.run_steps(2);
    assert!(!h.state().extras.view.split && h.state().extras.view.mode == crate::view::ViewMode::Print && h.state().doc.view_override.is_none());
}

#[test]
fn interface_languages() {
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("Bonjour — مرحبا".into()));
    for (lang, name) in [(crate::i18n::Lang::Fr, "lang-fr"), (crate::i18n::Lang::Ar, "lang-ar")] {
        h.state_mut().extras.options.settings.lang = lang;
        h.state_mut().tab = crate::ribbon::Tab::Home;
        h.run_steps(4);
        save(&mut h, name);
    }
    h.state_mut().tab = crate::ribbon::Tab::Insert;
    h.run_steps(4);
    save(&mut h, "lang-ar-insert");
    h.state_mut().extras.options.dialog = Some((0, h.state().extras.options.settings.clone()));
    h.run_steps(4);
    save(&mut h, "lang-ar-options");
    h.state_mut().extras.options.dialog = None;
    h.state_mut().extras.options.settings.lang = crate::i18n::Lang::En;
    h.run_steps(2);
}

/// Drive the Arabic interface through its tabs, menus, panes and dialogs; list the strings
/// that have no translation. Run with `cargo test arabic_coverage -- --ignored --nocapture`.
#[test]
#[ignore]
fn arabic_coverage() {
    use crate::ribbon::Tab;
    let mut h = harness(None);
    h.run_steps(2);
    h.event(Event::Text("Some text to work with.".into()));
    h.state_mut().extras.options.settings.lang = crate::i18n::Lang::Ar;
    h.run_steps(2);
    let shot = |h: &mut Harness<'_, WordApp>, name: &str| { h.run_steps(4); if std::env::var("COVERAGE_SHOTS").is_ok() { save(h, name); } };
    for t in [Tab::Home, Tab::Insert, Tab::Draw, Tab::Design, Tab::Layout, Tab::References, Tab::Mailings, Tab::Review, Tab::View] { h.state_mut().tab = t; h.run_steps(3); }
    click(&mut h, Pos2::new(1470.0, 21.0)); shot(&mut h, "ar-file-menu"); h.key_press(Key::Escape);
    // Panes.
    { let a = h.state_mut(); a.show_outline = true; a.show_comments = true; a.extras.review.accessibility = true; a.extras.review.reviewing_pane = true; a.extras.refs.selection_pane = true; }
    h.state_mut().new_comment();
    shot(&mut h, "ar-panes");
    { let a = h.state_mut(); a.show_outline = false; a.show_comments = false; a.comment_edit = None; a.extras.review.accessibility = false; a.extras.review.reviewing_pane = false; a.extras.refs.selection_pane = false; }
    h.state_mut().open_editor(); h.run_steps(3); h.state_mut().extras.review.editor_open = false;
    // Dialogs, one at a time.
    let dialogs: Vec<Box<dyn Fn(&mut WordApp)>> = vec![
        Box::new(|a| a.dialogs.page_setup = Some(a.doc.page.clone())),
        Box::new(|a| crate::ui::open_paragraph_dialog(a)),
        Box::new(|a| a.open_font_dialog()),
        Box::new(|a| a.dialogs.link = Some(String::new())),
        Box::new(|a| a.dialogs.table = Some((3, 3))),
        Box::new(|a| a.dialogs.about = true),
        Box::new(|a| a.dialogs.shortcuts = true),
        Box::new(|a| a.extras.options.dialog = Some((0, a.extras.options.settings.clone()))),
        Box::new(|a| a.extras.options.dialog = Some((1, a.extras.options.settings.clone()))),
        Box::new(|a| a.extras.options.dialog = Some((2, a.extras.options.settings.clone()))),
        Box::new(|a| a.extras.options.dialog = Some((3, a.extras.options.settings.clone()))),
        Box::new(|a| a.open_print()),
        Box::new(|a| { a.extras.macros.loaded = true; a.extras.macros.list = true; }),
        Box::new(|a| a.extras.macros.record_dialog = Some(crate::macros::RecordDraft { name: "M".into(), description: String::new(), store: crate::macros::Store::Global, shortcut: None })),
        Box::new(|a| a.open_columns_dialog()),
        Box::new(|a| a.open_line_numbers_dialog()),
        Box::new(|a| a.extras.refs.hyphenation = Some(Default::default())),
        Box::new(|a| a.extras.refs.toc = Some(Default::default())),
        Box::new(|a| a.extras.refs.caption = Some(crate::refs::CaptionDraft { label: "Figure".into(), text: String::new(), above: false })),
        Box::new(|a| a.new_source(false)),
        Box::new(|a| a.extras.refs.manage_sources = true),
        Box::new(|a| a.open_mark_entry()),
        Box::new(|a| a.extras.refs.index = Some(2)),
        Box::new(|a| a.open_mark_citation()),
        Box::new(|a| a.extras.mailings.envelope = Some(crate::mailings::EnvelopeDraft { delivery: String::new(), ret: String::new(), size: 0 })),
        Box::new(|a| a.extras.mailings.labels = Some(crate::mailings::LabelsDraft { text: String::new(), spec: 0, merge: false })),
        Box::new(|a| a.type_new_list()),
        Box::new(|a| a.extras.mailings.greeting = Some(crate::mailings::GreetingDraft { greeting: "Dear".into(), name: Default::default(), punctuation: ",".into() })),
        Box::new(|a| a.extras.mailings.match_fields = true),
        Box::new(|a| a.extras.mailings.errors = Some(vec![])),
        Box::new(|a| a.extras.review.compare = Some(Default::default())),
        Box::new(|a| a.extras.review.restrict = Some(crate::review::RestrictDraft { kind: Default::default(), password: String::new(), confirm: String::new() })),
        Box::new(|a| a.open_properties()),
        Box::new(|a| a.extras.view.zoom_dialog = Some((1.0, 2))),
        Box::new(|a| a.insert.xref = Some(crate::insert::XrefDraft { headings: true, page: false, selected: None })),
    ];
    for (k, open) in dialogs.iter().enumerate() {
        open(h.state_mut());
        shot(&mut h, &format!("ar-dialog-{k}"));
        h.key_press(Key::Escape);
        let a = h.state_mut();
        a.dialogs = Default::default();
        a.extras.options.dialog = None; a.extras.print = None; a.extras.macros.list = false; a.extras.macros.record_dialog = None;
        a.extras.refs = Default::default(); a.extras.mailings = Default::default(); a.extras.review.compare = None; a.extras.review.restrict = None;
        a.extras.view.properties = None; a.extras.view.zoom_dialog = None; a.insert.xref = None;
        h.run_steps(2);
    }
    // Views.
    for m in [crate::view::ViewMode::Outline, crate::view::ViewMode::Web, crate::view::ViewMode::Print] { h.state_mut().set_view(m); h.run_steps(3); }
    let missing = crate::i18n::missing();
    println!("MISSING {}", missing.len());
    std::fs::write(std::env::temp_dir().join("qalam-missing.json"), serde_json::to_string_pretty(&missing).unwrap()).unwrap();
    h.state_mut().extras.options.settings.lang = crate::i18n::Lang::En;
    h.run_steps(2);
}

#[test]
fn about_and_options_snapshots() {
    let mut h = harness(None);
    h.run_steps(2);
    h.state_mut().dialogs.about = true;
    h.run_steps(4);
    save(&mut h, "about");
    h.state_mut().dialogs.about = false;
    h.state_mut().extras.options.dialog = Some((2, h.state().extras.options.settings.clone()));
    h.run_steps(4);
    save(&mut h, "options-tabs");
}

/// The font color and highlight menus open without panicking.
#[test]
fn color_menus_open() {
    let mut h = harness(None);
    h.run_steps(2);
    for lang in [crate::i18n::Lang::En, crate::i18n::Lang::Ar] {
        h.state_mut().extras.options.settings.lang = lang;
        h.run_steps(3);
        // The font color arrow, wherever the (possibly mirrored) ribbon put it.
        let x = if lang == crate::i18n::Lang::Ar { 1500.0 - 388.0 } else { 388.0 };
        for dx in [-6.0f32, -3.0, 0.0, 3.0] {
            click(&mut h, Pos2::new(x + dx, 79.0));
            h.run_steps(4);
            assert!(h.state().dialogs.error.is_none(), "a frame panicked ({lang:?}): {:?}", h.state().dialogs.error);
            h.key_press(Key::Escape);
            h.run_steps(2);
        }
    }
    h.state_mut().extras.options.settings.lang = crate::i18n::Lang::En;
    click(&mut h, Pos2::new(388.0, 79.0));
    h.run_steps(4);
    save(&mut h, "color-menu");
    { use egui_kittest::kittest::Queryable; h.get_by_label("More Colors…").hover(); }
    h.run_steps(6);
    save(&mut h, "color-picker");
    assert!(h.state().dialogs.error.is_none());
}

#[test]
fn arabic_dropdowns_mirror() {
    let mut h = harness(None);
    h.run_steps(2);
    h.state_mut().extras.options.settings.lang = crate::i18n::Lang::Ar;
    h.run_steps(3);
    click(&mut h, Pos2::new(1500.0 - 388.0, 79.0));
    h.run_steps(4);
    save(&mut h, "ar-color-menu");
    { use egui_kittest::kittest::Queryable; h.get_by_label(&crate::i18n::shape("ألوان إضافية…")).hover(); }
    h.run_steps(6);
    save(&mut h, "ar-color-picker");
    assert!(h.state().dialogs.error.is_none());
    crate::i18n::set(crate::i18n::Lang::En);
}

/// Click every spot of the Arabic ribbon; report positions whose popup panics.
#[test]
#[ignore]
fn arabic_ribbon_sweep() {
    let mut h = harness(None);
    h.run_steps(2);
    h.state_mut().extras.options.settings.lang = crate::i18n::Lang::Ar;
    h.run_steps(3);
    let mut bad = vec![];
    let mut y = 50.0;
    while y <= 120.0 {
        let mut x = 20.0;
        while x <= 1480.0 {
            h.state_mut().dialogs.error = None;
            click(&mut h, Pos2::new(x, y));
            h.run_steps(3);
            if h.state().dialogs.error.is_some() { bad.push((x, y)); }
            h.key_press(Key::Escape);
            h.state_mut().dialogs = Default::default();
            h.state_mut().tab = crate::ribbon::Tab::Home;
            h.run_steps(2);
            x += 14.0;
        }
        y += 23.0;
    }
    println!("BAD {bad:?}");
}

#[test]
#[ignore]
fn arabic_replay() {
    let mut h = harness(None);
    h.state_mut().extras.options.settings.lang = crate::i18n::Lang::Ar;
    h.run_steps(2);
    h.event(Event::Text("Fuzzing text with several words to click on.".into()));
    let check = |h: &mut Harness<'_, WordApp>, what: &str| { if h.state().dialogs.error.is_some() { println!("PANIC after {what}"); h.state_mut().dialogs.error = None; } };
    click(&mut h, Pos2::new(869.0, 71.0)); h.run_steps(2); check(&mut h, "click 869,71");
    save(&mut h, "replay-1");
    h.event(Event::Text("\n".into())); h.run_steps(1); check(&mut h, "enter");
    h.key_press_modifiers(Modifiers::COMMAND, Key::X); h.run_steps(1); check(&mut h, "cut");
    h.event(Event::Text("a".into())); h.run_steps(1); check(&mut h, "a");
    click(&mut h, Pos2::new(386.0, 144.0)); h.run_steps(2); check(&mut h, "click 386,144");
    save(&mut h, "replay-2");
    h.key_press(Key::End); h.run_steps(1); check(&mut h, "end");
    click(&mut h, Pos2::new(1231.0, 116.0)); h.run_steps(3); check(&mut h, "click 1231,116");
}

/// A stand-in for the system's default-app settings: Word has the types until Qalam is made default.
struct FakeDefaults(std::sync::Mutex<bool>);
impl qalam_kit::default_apps::Backend for FakeDefaults {
    fn handler(&self, app: &qalam_kit::default_apps::AppId, _: &qalam_kit::default_apps::Format) -> Option<String> {
        Some(if *self.0.lock().unwrap() { app.bundle_id.to_string() } else { "com.microsoft.Word".into() })
    }
    fn make_default(&self, _: &qalam_kit::default_apps::AppId) -> Result<qalam_kit::default_apps::Outcome, String> { *self.0.lock().unwrap() = true; Ok(qalam_kit::default_apps::Outcome::Changed) }
}

#[test]
fn options_default_apps() {
    use egui_kittest::kittest::Queryable;
    qalam_kit::default_apps::set_backend(std::sync::Arc::new(FakeDefaults(std::sync::Mutex::new(false))));
    let mut h = harness(None);
    h.run_steps(3);
    h.state_mut().extras.options.dialog = Some((0, h.state().extras.options.settings.clone()));
    h.run_steps(4);
    assert!(h.query_by_label(".docx").is_some() && h.query_by_label(".md").is_some());
    assert!(h.query_all_by_label("Another app").count() > 0);
    h.get_by_label("Make Qalam the Default App").scroll_to_me(); h.run_steps(3);
    h.get_by_label("Make Qalam the Default App").click(); h.run_steps(3);
    assert!(h.query_by_label("Another app").is_none());
    assert!(h.query_by_label("Qalam is the default app for all of these file types.").is_some());
    h.state_mut().extras.options.dialog = None;
}

#[test]
fn opens_a_file_the_system_sent() {
    crate::app::TAKE_SENT_FILES.with(|t| t.set(true));
    let p = std::env::temp_dir().join(format!("qalam-sent-{}.txt", std::process::id()));
    std::fs::write(&p, "Sent from Finder").unwrap();
    let mut h = harness(None);
    h.run_steps(2);
    qalam_kit::open_files::push([p.clone()]);
    h.run_steps(3);
    assert!(h.state().doc.text().contains("Sent from Finder"));
    crate::app::TAKE_SENT_FILES.with(|t| t.set(false));
    let _ = std::fs::remove_file(&p);
}
