//! Write a .docx exercising every serialized feature (for schema validation):
//! `cargo run --example features -- out.docx [roundtrip-source.docx]`.
use word_engine::numbering::ListKind;
use word_engine::*;

fn main() {
    let out = std::env::args().nth(1).expect("output path");
    let mut d = match std::env::args().nth(2) {
        Some(src) => docx::read(&std::fs::read(src).unwrap()).unwrap(),
        None => Document::new(vec![Paragraph::new("Title line"), Paragraph::new("Body text with a link and formatting."), Paragraph::new("item one"), Paragraph::new("item two")]),
    };
    d.clock = || "2026-01-01T00:00:00Z".into();
    d.revision_author = "Tester".into();
    d.select(0, 0);
    d.set_paragraph_style("Heading1");
    d.select(11, 15);
    d.toggle_bold();
    d.set_link(Some("https://example.com".into()));
    d.select(20, 24);
    d.set_run_props(RunProps { color: Some(Some(Color(200, 0, 0))), highlight: Some(Some("yellow".into())), underline: Some("double".into()), strike: Some(Strike::Double), ..Default::default() });
    d.add_comment("Tester", "A comment\nwith two lines", "2026-01-01T00:00:00Z");
    let root = d.comments[0].id.clone();
    d.reply_to_comment(&root, "Other", "Reply", "2026-01-02T00:00:00Z");
    d.set_para_props(ParaProps { alignment: Some(Alignment::Justify), indent_left: Some(360), first_line: Some(-360), space_before: Some(6.0), line_rule: Some(LineRule::AtLeast), line_spacing: Some(14.0), tabs: Some(vec![TabStop { pos: 4320, kind: TabKind::Right, leader: Leader::Dot }]), shading: Some(Some(Color(240, 240, 240))), borders: Some(ParaBorders { bottom: Some(Border { width: 0.5, space: 1.0, color: None }), ..Default::default() }), keep_next: Some(true), ..Default::default() });
    let n = d.len();
    d.select(n - 8, n);
    d.toggle_list(ListKind::Number);
    d.select(15, 15);
    let f = d.insert_note(NoteKind::Foot);
    d.set_note_text(NoteKind::Foot, &f, "Footnote text");
    let e = d.insert_note(NoteKind::End);
    d.set_note_text(NoteKind::End, &e, "Endnote text");
    d.set_track_changes(true);
    d.select(2, 5);
    d.replace("NEW");
    d.set_track_changes(false);
    let n = d.len();
    d.select(n, n);
    d.insert_table(3, 3);
    d.replace("cell");
    d.insert_table_row(true);
    d.set_cell_shading(Some(Color(200, 220, 255)));
    d.set_cell_alignment(word_engine::model::CellVAlign::Center, Alignment::Center);
    d.set_header_rows(true);
    d.set_row_height(600, true);
    d.move_cell(true);
    let (a, _) = d.range();
    d.select(a, a);
    d.move_cell(true);
    let (_, e) = d.range();
    d.select(a, e);
    d.merge_cells();
    d.apply_border_preset(word_engine::tables::BorderPreset::Outside, Border { width: 1.5, space: 0.0, color: Some(Color(200, 0, 0)) });
    d.apply_border_preset(word_engine::tables::BorderPreset::InsideH, Border { width: 0.0, space: 0.0, color: None });
    d.set_cell_margins(Some([60, 144, 60, 144]));
    d.set_table_cell_margins([0, 72, 0, 72]);
    d.insert_cells();
    d.apply_table_style(word_engine::tables::TableStyle::Banded, Color(0x44, 0x72, 0xC4));
    d.autofit(word_engine::tables::AutoFit::Contents);
    let n = d.len();
    d.select(n, n);
    let png = { let mut b = std::io::Cursor::new(vec![]); image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 128, 255, 255])).write_to(&mut b, image::ImageFormat::Png).unwrap(); b.into_inner() };
    d.insert_image(png, "image/png", 40.0, 40.0);
    d.insert_page_break();
    d.replace("أول");
    d.toggle_list(word_engine::numbering::ListKind::Number);
    d.apply_numbering(word_engine::numbering::NumFmt::ArabicAbjad, "%1-");
    d.insert_paragraph();
    d.toggle_list(word_engine::numbering::ListKind::Number);
    d.replace("Glow");
    let n = d.len();
    d.select(n - 4, n);
    d.set_run_props(RunProps { glow: Some(Some(Glow { color: Color(255, 192, 0), radius: 6.0 })), reflection: Some(true), gradient: Some(Some((Color(0, 0, 255), Color(255, 0, 0)))), ..Default::default() });
    d.select(n, n);
    d.insert_paragraph();
    d.replace("نص عربي مع English");
    d.set_paragraph_direction(true);
    let n = d.len();
    d.select(n - 18, n);
    d.set_run_props(RunProps { cs_font: Some("Traditional Arabic".into()), size_cs: Some(14.0), bold_cs: Some(true), rtl: Some(true), ..Default::default() });
    // Floating and rotated objects in every wrap style, an edited picture, and a chart.
    use word_engine::model::{Anchor, Wrap};
    use word_engine::objects::{Chart, ChartKind, Object, Shape, ShapeKind};
    d.select(0, 0);
    let mut shape = Shape::new(ShapeKind::Star5);
    shape.rotation = 20.0; shape.flip_h = true; shape.text = "Hi".into();
    for (k, wrap) in [Wrap::Square, Wrap::Tight, Wrap::TopBottom, Wrap::Behind, Wrap::InFront].into_iter().enumerate() {
        d.select(0, 0);
        d.insert_object(Object::Shape(shape.clone()));
        d.set_anchor(0, Some(Anchor::new(wrap, 40.0 * k as f32, 10.0)));
    }
    d.select(0, 0);
    d.insert_object(Object::Chart(Chart::sample(ChartKind::Column)));
    let n = d.len();
    d.select(n, n);
    let pic = { let mut b = std::io::Cursor::new(vec![]); image::RgbaImage::from_pixel(8, 4, image::Rgba([200, 30, 30, 255])).write_to(&mut b, image::ImageFormat::Png).unwrap(); b.into_inner() };
    d.insert_image(pic, "image/png", 80.0, 40.0);
    let pos = d.len() - 1;
    for op in [word_engine::picture::PictureOp::Sepia, word_engine::picture::PictureOp::Corrections { brightness: 10, contrast: 20 }, word_engine::picture::PictureOp::Transparency(20), word_engine::picture::PictureOp::Crop { left: 0.1, top: 0.0, right: 0.1, bottom: 0.0 }, word_engine::picture::PictureOp::Oval, word_engine::picture::PictureOp::SoftEdges, word_engine::picture::PictureOp::Frame(word_engine::Color(0, 0, 0))] {
        d.apply_picture_op(pos, op);
    }
    d.flip_image(pos, true);
    d.set_image_rotation(pos, 15.0);
    d.set_anchor(pos, Some(Anchor::new(Wrap::Square, 300.0, 0.0)));
    // Headers, footers, page numbers, equations, ink, page design, a drop cap and fields.
    d.properties.title = "Feature Report".into();
    let hdr = d.story_design(word_engine::design::StoryDesign::Line, true);
    d.set_story(true, Some(hdr));
    d.insert_page_numbers(false, 1, word_engine::Alignment::Center);
    d.set_watermark(Some(word_engine::design::Watermark::Text { text: "DRAFT".into(), font: "Calibri".into(), size: 0.0, color: Color(0xC0, 0xC0, 0xC0), diagonal: true }));
    d.set_page_color(Some(Color(0xFF, 0xFB, 0xF0)));
    d.set_page_borders(Some(word_engine::page::PageBorders { style: word_engine::page::BorderStyle::Double, width: 0.75, color: Color(0x44, 0x72, 0xC4), space: 24.0, from_text: false, sides: [true; 4] }));
    d.apply_style_set(1);
    d.apply_fonts("Cambria", "Calibri");
    d.apply_paragraph_spacing(0.0, 8.0, 1.15);
    let n = d.len();
    d.select(n, n);
    d.insert_paragraph();
    for (_, src) in word_engine::equation::GALLERY {
        let mut eq = word_engine::equation::Equation::new(src);
        eq.display = true;
        d.insert_object(Object::Equation(eq));
        let e = d.range().1;
        d.select(e, e);
        d.insert_paragraph();
    }
    d.replace("Inline math ");
    d.insert_object(Object::Equation(word_engine::equation::Equation::new("e^{i\\pi}+1=0")));
    let e = d.range().1;
    d.select(e, e);
    d.replace(" and the date ");
    d.insert_field(word_engine::Field::Date("dddd, MMMM d, yyyy".into()));
    d.replace(" by ");
    d.insert_field(word_engine::Field::Prop("AUTHOR".into()));
    d.insert_paragraph();
    d.replace("Dropped capitals start this paragraph, which wraps beside the large letter for three lines of text and then continues at the margin as usual.");
    let pi = d.paragraphs.len() - 1;
    d.set_drop_cap(pi, Some((word_engine::format::DropCap { margin: false, lines: 3, distance: 0 }, None)));
    let ink = word_engine::objects::Ink { width: 200.0, height: 100.0, canvas: true, strokes: vec![word_engine::objects::InkStroke { points: vec![(10.0, 10.0), (60.0, 80.0), (150.0, 20.0)], color: Color(0xE0, 0x30, 0x30), width: 2.0, highlighter: false, seq: 1 }] };
    let n = d.len();
    d.select(n, n);
    d.insert_paragraph();
    d.insert_object(Object::Ink(ink));
    let mut tb = Shape::text_box("A text box with some text.");
    tb.font_size = 12.0;
    let n = d.len();
    d.select(n, n);
    d.insert_object(Object::Shape(tb));
    // References and layout: TOC, captions, citations, bibliography, index, authorities,
    // columns, line numbers, hyphenation, note formats.
    use word_engine::references::{Source, SourceKind, TocOptions};
    d.select(0, 0);
    d.insert_toc(TocOptions::default());
    d.add_source(Source { tag: "Knu84".into(), kind: SourceKind::Book, authors: "Knuth, Donald E.".into(), title: "The TeXbook".into(), year: "1984".into(), publisher: "Addison-Wesley".into(), city: "Reading".into(), ..Default::default() });
    let n = d.len();
    d.select(n, n);
    d.insert_paragraph();
    d.replace("Typesetting follows Knuth ");
    d.insert_field(word_engine::Field::Citation("Knu84".into()));
    d.mark_index_entry("Typesetting");
    d.mark_authority("Marbury v. Madison, 5 U.S. 137 (1803)", "Marbury", 1);
    d.insert_caption("Figure", ": A canvas", false);
    let n = d.len();
    d.select(n, n);
    d.columns_from_here(word_engine::page::Columns { count: 2, separator: true, ..Default::default() });
    d.replace(&"Hyphenation and columns make newspaper-style typography possible for extraordinarily long words. ".repeat(20));
    d.insert_section_break(word_engine::page::SectionKind::Continuous);
    d.set_columns(word_engine::page::Columns::default());
    d.set_line_numbers(Some(word_engine::page::LineNumbering { start: 1, count_by: 5, restart: word_engine::page::LineRestart::Page, distance: 0 }));
    d.set_hyphenation(word_engine::page::Hyphenation { auto: true, ..Default::default() });
    d.set_note_format(true, word_engine::numbering::NumFmt::LowerRoman, 1);
    d.insert_table_of_figures("Figure", TocOptions::default());
    d.insert_bibliography(Some("References"));
    d.insert_index(2);
    let n = d.len();
    d.select(n, n);
    d.insert_toa();
    d.update_tables();
    // Mail merge fields, a rule, a nested table, and editing protection.
    let n = d.len();
    d.select(n, n);
    d.insert_field(word_engine::Field::GreetingLine { greeting: "Dear".into(), name: word_engine::mailmerge::NameFormat::FirstLast, punctuation: ",".into() });
    d.insert_field(word_engine::Field::Merge("City".into()));
    d.insert_field(word_engine::Field::MergeIf { field: "City".into(), op: word_engine::mailmerge::Compare::Equal, value: "Paris".into(), then: "Bonjour".into(), otherwise: "Hello".into() });
    d.insert_field(word_engine::Field::AddressBlock);
    d.insert_table(2, 2);
    d.insert_table(1, 2);
    d.protection = Some(word_engine::review::Protection::new(word_engine::review::Restriction::Comments, "pw"));
    std::fs::write(&out, docx::write(&d).unwrap()).unwrap();
    eprintln!("wrote {out}");
}
