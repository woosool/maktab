//! Round trips through write → read, the fixture files, and checks that every written part is
//! well-formed XML.

use super::*;
use crate::addr::{parse_range, Pos};
use crate::book::{DefinedName, Hyperlink, Input, Workbook};
use crate::style::{Border, HAlign, LineStyle, Style};
use crate::value::Value;
use std::io::Read;

fn p(a: &str) -> Pos { crate::addr::parse_a1(a).unwrap() }

/// Every part of a package, by name.
fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len()).map(|i| { let mut f = z.by_index(i).unwrap(); let mut d = vec![]; f.read_to_end(&mut d).unwrap(); (f.name().to_string(), d) }).collect()
}

/// Parses every XML part to the end, failing on any well-formedness error.
fn check_well_formed(bytes: &[u8]) {
    for (name, data) in unzip(bytes) {
        if !(name.ends_with(".xml") || name.ends_with(".rels") || name.ends_with(".vml")) { continue; }
        let text = super::xml::part_text(&data);
        let mut r = quick_xml::Reader::from_str(&text);
        r.config_mut().check_end_names = true;
        let mut depth = 0i32;
        loop {
            match r.read_event() {
                Ok(quick_xml::events::Event::Start(_)) => depth += 1,
                Ok(quick_xml::events::Event::End(_)) => depth -= 1,
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(quick_xml::events::Event::Text(t)) => { t.unescape().unwrap_or_else(|e| panic!("{name}: {e}")); }
                Ok(_) => {}
                Err(e) => panic!("{name}: {e} at {}", r.buffer_position()),
            }
        }
        assert_eq!(depth, 0, "{name} unbalanced");
    }
}

fn sample() -> Workbook {
    let mut wb = Workbook::new();
    wb.add_sheet(1, Some("Data Two".into()));
    wb.add_sheet(2, Some("Hidden".into()));
    wb.sheets[2].hidden = true;
    for (a, t) in [("A1", "Name"), ("B1", "Qty"), ("A2", "apple"), ("B2", "3"), ("A3", "pear"), ("B3", "4.5"), ("C1", "TRUE"), ("C2", "#N/A"), ("D1", "  padded\nline  "), ("D2", "ctl\u{1}x_x0041_")] { wb.set_input(0, p(a), t); }
    wb.set_input(0, p("B4"), "=SUM(B2:B3)");
    wb.set_input(0, p("B5"), "=XLOOKUP(\"pear\",A2:A3,B2:B3)");
    wb.set_input(0, p("B6"), "=LET(x,2,y,3,x*y)");
    wb.set_input(0, p("B7"), "=A2&\"!\"");
    wb.set_input(0, p("E1"), "=SEQUENCE(3)");
    wb.set_input(1, p("A1"), "='Sheet1'!B4*2");
    wb.set_input(0, p("C5"), "1/2/2024");
    let mut st = Style::default();
    st.bold = true; st.italic = true; st.color = Some([200, 10, 10]); st.fill = Some([255, 255, 0]); st.h = HAlign::Center; st.wrap = true; st.rotation = -45;
    st.bottom = Border { style: LineStyle::Double, color: Some([0, 0, 255]) }; st.num_fmt = "0.000\"x\"".into(); st.locked = false; st.size = 140; st.font = "Arial".into();
    let si = wb.styles.intern(st);
    wb.sheets[0].cells.get_mut(&p("B2")).unwrap().style = si;
    let s = &mut wb.sheets[0];
    s.merges.push(parse_range("F1:G2").unwrap());
    s.frozen_rows = 1; s.frozen_cols = 2;
    s.col_widths.insert(0, 120.0); s.col_widths.insert(3, 33.0);
    s.row_heights.insert(1, 40.0);
    s.hidden_rows.insert(9); s.hidden_cols.insert(7);
    s.notes.insert(p("A1"), "first note\nline two".into());
    s.links.insert(p("A2"), Hyperlink { target: "https://example.com/a?b=1&c=2".into(), tooltip: Some("tip".into()) });
    s.links.insert(p("A3"), Hyperlink { target: "#'Data Two'!A1".into(), tooltip: None });
    s.tab_color = Some([1, 2, 3]);
    s.zoom = 125; s.right_to_left = true; s.gridlines = false; s.show_zeros = false;
    s.selection = parse_range("B2:C3");
    s.autofilter = parse_range("A1:B3");
    s.col_styles.insert(10, si);
    wb.sheets[1].protected = true;
    wb.names.push(DefinedName { name: "Total".into(), sheet: None, formula: "Sheet1!$B$4".into(), hidden: false, comment: "the total".into() });
    wb.names.push(DefinedName { name: "Local".into(), sheet: Some(1), formula: "'Data Two'!$A$1".into(), hidden: true, comment: String::new() });
    wb.active = 1;
    wb.recalc();
    wb
}

#[test]
fn round_trip() {
    let wb = sample();
    let bytes = write(&wb).unwrap();
    check_well_formed(&bytes);
    let back = read(&bytes).unwrap();
    assert_eq!(back.sheets.len(), 3);
    assert_eq!(back.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Sheet1", "Data Two", "Hidden"]);
    assert!(back.sheets[2].hidden);
    assert_eq!(back.active, 1);
    let (a, b) = (&wb.sheets[0], &back.sheets[0]);
    for (pos, c) in &a.cells {
        let d = &b.cells[pos];
        assert_eq!(c.input, d.input, "input at {}", pos.a1());
        assert_eq!(c.value, d.value, "value at {}", pos.a1());
        assert_eq!(wb.styles.get(c.style), back.styles.get(d.style), "style at {}", pos.a1());
    }
    assert_eq!(b.cells[&p("D2")].value, Value::str("ctl\u{1}x_x0041_"));
    assert_eq!(b.cells[&p("B5")].value, Value::Num(4.5));
    assert_eq!(b.cells[&p("B6")].formula(), Some("LET(x,2,y,3,x*y)"));
    assert_eq!(b.value(p("E3")), Value::Num(3.0));
    assert_eq!(b.spill_anchor.get(&p("E3")), Some(&p("E1")));
    for f in ["merges", "frozen", "widths", "heights", "hidden", "notes", "links", "view", "filter", "colstyle"] {
        match f {
            "merges" => assert_eq!(a.merges, b.merges),
            "frozen" => assert_eq!((a.frozen_rows, a.frozen_cols), (b.frozen_rows, b.frozen_cols)),
            "widths" => assert_eq!(a.col_widths, b.col_widths),
            "heights" => assert_eq!(a.row_heights, b.row_heights),
            "hidden" => assert_eq!((&a.hidden_rows, &a.hidden_cols), (&b.hidden_rows, &b.hidden_cols)),
            "notes" => assert_eq!(a.notes, b.notes),
            "links" => assert_eq!(a.links, b.links),
            "view" => assert_eq!((a.tab_color, a.zoom, a.right_to_left, a.gridlines, a.show_zeros, a.selection), (b.tab_color, b.zoom, b.right_to_left, b.gridlines, b.show_zeros, b.selection)),
            "filter" => assert_eq!(a.autofilter, b.autofilter),
            _ => assert_eq!(wb.styles.get(a.col_styles[&10]), back.styles.get(b.col_styles[&10])),
        }
    }
    assert!(back.sheets[1].protected);
    assert_eq!(back.sheets[1].cells[&p("A1")].value, Value::Num(15.0));
    assert_eq!(wb.names, back.names);
    // And once more: a second save of what was read is stable.
    let again = read(&write(&back).unwrap()).unwrap();
    assert_eq!(again.sheets[0].cells.len(), back.sheets[0].cells.len());
    assert_eq!(again.names, back.names);
}

#[test]
fn written_formulas_use_future_prefixes() {
    let bytes = write(&sample()).unwrap();
    let parts = unzip(&bytes);
    let sheet = String::from_utf8(parts.iter().find(|(n, _)| n == "xl/worksheets/sheet1.xml").unwrap().1.clone()).unwrap();
    assert!(sheet.contains("<f>_xlfn.XLOOKUP("), "{sheet}");
    assert!(sheet.contains("_xlfn.LET(_xlpm.x,2,_xlpm.y,3,_xlpm.x*_xlpm.y)"));
    assert!(sheet.contains("cm=\"1\"><f t=\"array\" ref=\"E1:E3\">_xlfn.SEQUENCE(3)</f>"));
    assert!(parts.iter().any(|(n, _)| n == "xl/metadata.xml"));
    assert!(parts.iter().any(|(n, _)| n.starts_with("xl/drawings/vmlDrawing")));
    assert_eq!(to_file_formula("FILTER(A1:A3,B1:B3>0)+sum(1)"), "_xlfn._xlws.FILTER(A1:A3,B1:B3>0)+sum(1)");
    assert_eq!(to_file_formula("\"XLOOKUP(\"&STDEV.S(A1:A2)"), "\"XLOOKUP(\"&_xlfn.STDEV.S(A1:A2)");
    assert_eq!(from_file_formula("_xlfn._xlws.SORT(A1:A3)+_xlfn.LAMBDA(_xlpm.a,_xlpm.a+1)(2)"), "SORT(A1:A3)+LAMBDA(a,a+1)(2)");
}

#[test]
fn sizes() {
    assert_eq!(width_to_px(9.140625), 64.0);
    assert_eq!(px_to_width(64.0), 9.140625);
    for px in 0..400 { assert_eq!(width_to_px(px_to_width(px as f32)), px as f32); }
    assert_eq!(base_width_px(8.0), 64.0);
}

#[test]
fn fixtures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spreadsheet/tests/fixtures");
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let mut n = 0;
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("xlsx") { continue; }
        let wb = read(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let bytes = write(&wb).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        check_well_formed(&bytes);
        let back = read(&bytes).unwrap_or_else(|e| panic!("{} again: {e}", path.display()));
        assert_eq!(back.sheets.len(), wb.sheets.len());
        for (x, y) in wb.sheets.iter().zip(&back.sheets) {
            for (pos, c) in &x.cells { if !c.is_blank() { assert_eq!(Some(&c.input), y.cells.get(pos).map(|d| &d.input), "{} {}!{}", path.display(), x.name, pos.a1()); } }
        }
        n += 1;
    }
    assert!(n > 0);
}

#[test]
fn reads_hand_written_package() {
    // Prefixed names, no r attributes, shared and array formulas, inline and rich strings,
    // a theme color with tint, absolute rel targets.
    let files: &[(&str, &str)] = &[
        ("[Content_Types].xml", r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#),
        ("_rels/.rels", r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument" Target="/xl/workbook.xml"/></Relationships>"#),
        ("xl/workbook.xml", r#"<x:workbook xmlns:x="http://purl.oclc.org/ooxml/spreadsheetml/main" xmlns:r="http://purl.oclc.org/ooxml/officeDocument/relationships"><x:workbookPr date1904="1"/><x:sheets><x:sheet name="S" sheetId="1" r:id="rId1"/></x:sheets><x:definedNames><x:definedName name="N" localSheetId="0">S!$A$1</x:definedName></x:definedNames></x:workbook>"#),
        ("xl/_rels/workbook.xml.rels", r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet" Target="/xl/worksheets/s.xml"/><Relationship Id="rId2" Type="x/sharedStrings" Target="strings.xml"/><Relationship Id="rId3" Type="x/styles" Target="styles.xml"/></Relationships>"#),
        ("xl/strings.xml", r#"<sst xmlns="m"><si><r><rPr><b/></rPr><t>Rich </t></r><r><t xml:space="preserve">text</t></r><rPh><t>ignored</t></rPh></si></sst>"#),
        ("xl/styles.xml", r#"<styleSheet xmlns="m"><fonts><font><sz val="11"/><name val="Calibri"/></font><font><b val="0"/><color theme="4" tint="0.5"/><sz val="10"/><name val="Arial"/></font></fonts><fills><fill><patternFill/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor indexed="10"/></patternFill></fill></fills><borders><border/></borders><cellXfs><xf/><xf fontId="1" fillId="2" numFmtId="10"/></cellXfs></styleSheet>"#),
        ("xl/worksheets/s.xml", r#"<x:worksheet xmlns:x="m"><x:sheetData><x:row><x:c t="s"><x:v>0</x:v></x:c><x:c t="inlineStr"><x:is><x:t>in</x:t></x:is></x:c><x:c s="1"><x:v>0.25</x:v></x:c></x:row>
<x:row><x:c><x:f t="shared" ref="A2:A4" si="0">B2*2</x:f><x:v>0</x:v></x:c></x:row><x:row><x:c><x:f t="shared" si="0"/><x:v>0</x:v></x:c></x:row>
<x:row r="5"><x:c r="A5"><x:f t="array" ref="A5:B5">{1,2}</x:f><x:v>1</x:v></x:c><x:c r="B5"><x:v>2</x:v></x:c><x:c r="C5" t="d"><x:v>2024-01-01T12:00:00</x:v></x:c><x:c r="D5" t="e"><x:v>#DIV/0!</x:v></x:c><x:c r="E5" t="b"><x:v>1</x:v></x:c></x:row></x:sheetData><x:mergeCells><x:mergeCell ref="A1:B1"/></x:mergeCells><x:pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></x:worksheet>"#),
    ];
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(&mut buf);
        for (n, d) in files { z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap(); z.write_all(d.as_bytes()).unwrap(); }
        z.finish().unwrap();
    }
    let wb = read(&buf.into_inner()).unwrap();
    let s = &wb.sheets[0];
    assert!(wb.date1904);
    assert_eq!(s.cells[&p("A1")].value, Value::str("Rich text"));
    assert_eq!(s.cells[&p("B1")].value, Value::str("in"));
    let st = wb.style_at(0, p("C1"));
    assert_eq!((st.font.as_str(), st.size, st.bold, st.fill, st.num_fmt.as_str()), ("Arial", 100, false, Some([255, 0, 0]), "0.00%"));
    assert!(st.color.is_some() && st.color != Some(wb.theme_colors[4]));
    assert_eq!(s.cells[&p("A3")].formula(), Some("B3*2"));
    assert_eq!(s.cells[&p("A5")].array, parse_range("A5:B5"));
    assert!(matches!(s.cells.get(&p("B5")).map(|c| &c.input), None | Some(Input::Empty)));
    assert_eq!(s.value(p("B5")), Value::Num(2.0));
    assert_eq!(s.cells[&p("C5")].value, Value::Num(crate::date::serial(2024, 1, 1, true) + 0.5));
    assert_eq!(s.cells[&p("D5")].value, Value::Err(crate::value::ErrKind::Div0));
    assert_eq!(s.cells[&p("E5")].value, Value::Bool(true));
    assert_eq!(wb.names[0].sheet, Some(0));
    assert!(!s.extra.iter().any(|(n, _)| n == "pageMargins"), "page margins are modelled");
    let out = write(&wb).unwrap();
    check_well_formed(&out);
    let sheet = String::from_utf8(unzip(&out).into_iter().find(|(n, _)| n == "xl/worksheets/sheet1.xml").unwrap().1).unwrap();
    assert!(sheet.find("<mergeCells").unwrap() < sheet.find("<pageMargins").unwrap());
    assert!(sheet.contains("xmlns:x=\"m\""));
}

/// Manual smoke test over private files: `JADWAL_XLSX_DIR=~/Downloads cargo test smoke -- --ignored --nocapture`.
#[test]
#[ignore]
fn smoke() {
    let Ok(dir) = std::env::var("JADWAL_XLSX_DIR") else { return };
    let out = std::env::var("JADWAL_XLSX_OUT").ok();
    if let Some(o) = &out { std::fs::write(std::path::Path::new(o).join("sample.xlsx"), write(&sample()).unwrap()).unwrap(); }
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("xlsx") { continue; }
        let t = std::time::Instant::now();
        let wb = match read(&std::fs::read(&path).unwrap()) { Ok(w) => w, Err(e) => { println!("READ FAIL {}: {e}", path.display()); continue } };
        let cells: usize = wb.sheets.iter().map(|s| s.cells.len()).sum();
        let bytes = write(&wb).unwrap();
        check_well_formed(&bytes);
        let back = read(&bytes).unwrap();
        let cells2: usize = back.sheets.iter().map(|s| s.cells.len()).sum();
        println!("{}: {} sheets, {cells} cells → {cells2}, {} parts, {:?}", path.display(), wb.sheets.len(), wb.parts.len(), t.elapsed());
        if let Some(o) = &out { std::fs::write(std::path::Path::new(o).join(path.file_name().unwrap()), &bytes).unwrap(); }
    }
}

#[test]
fn tables_and_filters_round_trip() {
    use crate::tables::{CmpOp, Cond, Criteria, Scope};
    let mut wb = Workbook::new();
    wb.add_sheet(1, None);
    for (i, row) in [["Name", "Qty"], ["apple", "3"], ["pear", "5"], ["fig", "1"]].iter().enumerate() {
        for (j, v) in row.iter().enumerate() { wb.set_input(0, Pos::new(i as u32, j as u32), v); wb.set_input(1, Pos::new(i as u32, j as u32), v); }
    }
    wb.create_table(0, parse_range("A1:B4").unwrap(), true, "TableStyleLight9").unwrap();
    wb.set_table_totals(0, 0, true);
    wb.set_input(0, p("C2"), "=[@Qty]*2"); wb.table_after_input(0, p("C2"));
    wb.set_column_filter(0, Scope::Table(0), 1, Some(Criteria::Custom { a: Cond { op: CmpOp::Gt, val: "2".into() }, b: None, and: false }));
    wb.toggle_filter(1, parse_range("A1:B4").unwrap(), p("A1"));
    wb.set_column_filter(1, Scope::Auto, 0, Some(Criteria::Values { values: vec!["pear".into()], blanks: true }));
    wb.create_table(1, parse_range("D1:E3").unwrap(), true, "").unwrap();
    wb.recalc();
    let bytes = write(&wb).unwrap();
    check_well_formed(&bytes);
    let names: Vec<String> = unzip(&bytes).into_iter().map(|x| x.0).collect();
    assert!(names.contains(&"xl/tables/table1.xml".to_string()) && names.contains(&"xl/tables/table2.xml".to_string()));
    let back = read(&bytes).unwrap();
    let t = &back.sheets[0].tables[0];
    assert_eq!(t.name, "Table1");
    assert_eq!(t.range, parse_range("A1:C5").unwrap());
    assert!(t.totals_row);
    assert_eq!(t.style, "TableStyleLight9");
    assert_eq!(t.columns[1].totals_function, "sum");
    assert_eq!(t.columns[2].calculated_formula, "[@Qty]*2");
    assert_eq!(t.filter.as_ref().unwrap().columns.len(), 1);
    assert_eq!(back.sheets[0].filtered_rows, wb.sheets[0].filtered_rows);
    assert_eq!(back.sheets[1].autofilter, parse_range("A1:B4"));
    assert_eq!(back.sheets[1].filter, wb.sheets[1].filter);
    assert_eq!(back.sheets[1].tables[0].name, "Table2");
    // Clearing after reading shows the rows the file hid.
    let mut back = back;
    back.clear_filters(1);
    assert!(back.sheets[1].hidden_rows.is_empty());
    // A second save is stable.
    let again = read(&write(&back).unwrap()).unwrap();
    assert_eq!(again.sheets[0].tables, back.sheets[0].tables);
}
