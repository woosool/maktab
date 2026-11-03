use crate::sample::sample_pdf;
use crate::*;

fn doc(pages: usize) -> Document {
    Document::from_bytes(sample_pdf(pages)).expect("sample opens")
}

/// Heading text of every page ("Page n"), read back through text extraction.
fn headings(d: &Document) -> Vec<String> {
    (0..d.page_count())
        .map(|i| {
            let t = d.page_text(i).text();
            t.lines().next().unwrap_or("").trim().to_string()
        })
        .collect()
}

fn reopen(d: &Document) -> Document {
    Document::from_bytes(d.bytes().to_vec()).expect("saved bytes reopen")
}

#[test]
fn opens_and_reads_text() {
    let d = doc(3);
    assert_eq!(d.page_count(), 3);
    assert_eq!(d.page_size(0), (612.0, 792.0));
    assert_eq!(headings(&d), ["Page 1", "Page 2", "Page 3"]);
    assert!(d.page_text(0).text().contains("quick brown fox"));
}

#[test]
fn glyph_boxes_sit_on_the_text() {
    let d = doc(1);
    let t = d.page_text(0);
    let p = t.glyphs.iter().find(|g| g.ch == 'P').unwrap();
    // Heading baseline is 72 pt from the top of the 792 pt page at y = 720.
    assert!((p.rect.x0 - 72.0).abs() < 1.0, "{:?}", p.rect);
    assert!(
        p.rect.y1 > 72.0 && p.rect.y0 < 72.0 && p.rect.y0 > 40.0,
        "{:?}",
        p.rect
    );
}

#[test]
fn rejects_garbage() {
    assert!(Document::from_bytes(b"not a pdf".to_vec()).is_err());
}

#[test]
fn reorder_helper() {
    assert_eq!(document_reorder(4, &[0], 4), [1, 2, 3, 0]);
    assert_eq!(document_reorder(4, &[3], 0), [3, 0, 1, 2]);
    assert_eq!(document_reorder(5, &[1, 3], 0), [1, 3, 0, 2, 4]);
    assert_eq!(document_reorder(5, &[0, 1], 3), [2, 0, 1, 3, 4]);
    assert_eq!(document_reorder(3, &[1], 1), [0, 1, 2]);
}

fn document_reorder(len: usize, moved: &[usize], to: usize) -> Vec<usize> {
    crate::document::reorder(len, moved, to)
}

#[test]
fn move_round_trips_through_save() {
    let mut d = doc(4);
    d.move_pages(&[0], 4).unwrap();
    assert_eq!(
        headings(&reopen(&d)),
        ["Page 2", "Page 3", "Page 4", "Page 1"]
    );
}

#[test]
fn rotate_round_trips() {
    let mut d = doc(2);
    d.rotate_pages(&[1], 90).unwrap();
    let r = reopen(&d);
    assert_eq!(r.page_rotation(1), 90);
    assert_eq!(r.page_size(1), (792.0, 612.0));
    d.rotate_pages(&[1], -180).unwrap();
    assert_eq!(reopen(&d).page_rotation(1), 270);
}

#[test]
fn delete_duplicate_insert() {
    let mut d = doc(3);
    d.delete_pages(&[1]).unwrap();
    assert_eq!(headings(&reopen(&d)), ["Page 1", "Page 3"]);
    d.duplicate_pages(&[0]).unwrap();
    assert_eq!(headings(&reopen(&d)), ["Page 1", "Page 1", "Page 3"]);
    d.insert_blank_page(1).unwrap();
    let r = reopen(&d);
    assert_eq!(headings(&r), ["Page 1", "", "Page 1", "Page 3"]);
    assert_eq!(r.page_size(1), (612.0, 792.0));
    assert!(
        d.delete_pages(&[0, 1, 2, 3]).is_err(),
        "deleting every page is refused"
    );
    assert_eq!(d.page_count(), 4, "failed edit leaves the document intact");
}

#[test]
fn extract_and_merge() {
    let d = doc(3);
    let out = d.extract_pages(&[2, 0]).unwrap();
    let e = Document::from_bytes(out.clone()).unwrap();
    assert_eq!(headings(&e), ["Page 3", "Page 1"]);

    let mut m = doc(2);
    m.insert_pdf(&out, 1).unwrap();
    assert_eq!(
        headings(&reopen(&m)),
        ["Page 1", "Page 3", "Page 1", "Page 2"]
    );
}

#[test]
fn undo_redo() {
    let mut d = doc(3);
    assert!(!d.can_undo() && !d.is_dirty());
    d.delete_pages(&[0]).unwrap();
    d.rotate_pages(&[0], 90).unwrap();
    assert!(d.is_dirty());
    d.undo().unwrap();
    assert_eq!(d.page_rotation(0), 0);
    d.undo().unwrap();
    assert_eq!(headings(&d), ["Page 1", "Page 2", "Page 3"]);
    assert!(!d.can_undo());
    d.redo().unwrap();
    assert_eq!(headings(&d), ["Page 2", "Page 3"]);
    d.mark_saved();
    assert!(!d.is_dirty());
    d.undo().unwrap();
    assert!(d.is_dirty());
}

#[test]
fn search_finds_matches_with_boxes() {
    let d = doc(3);
    let hits = d.search("QUICK brown");
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[1].page, 1);
    let r = hits[0].rects[0];
    assert!(r.x0 > 72.0 && r.width() > 40.0 && r.y0 > 100.0, "{r:?}");
    assert!(d.search("   ").is_empty());
    assert!(d.search("zebra").is_empty());
}

fn red() -> Color {
    Color::rgb(220, 50, 40)
}

#[test]
fn annotations_round_trip() {
    let mut d = doc(1);
    let t = d.page_text(0);
    let a = t
        .glyph_at(Point::new(80.0, 125.0), 4.0)
        .expect("glyph under point");
    let quads = t.line_rects(a, a + 8);
    let annots = [
        NewAnnot::Highlight {
            quads: quads.clone(),
            color: Color::rgb(255, 220, 0),
        },
        NewAnnot::Underline {
            quads: quads.clone(),
            color: red(),
        },
        NewAnnot::StrikeOut {
            quads,
            color: red(),
        },
        NewAnnot::FreeText {
            rect: Rect::new(300.0, 300.0, 500.0, 340.0),
            text: "Hello (world)\nsecond line".into(),
            color: red(),
            size: 14.0,
            fill: None,
        },
        NewAnnot::Note {
            at: Point::new(500.0, 80.0),
            text: "Note – ünïcode".into(),
            color: Color::rgb(255, 210, 60),
        },
        NewAnnot::Square {
            rect: Rect::new(80.0, 400.0, 200.0, 480.0),
            color: red(),
            width: 2.0,
            fill: None,
            opacity: 1.0,
        },
        NewAnnot::Circle {
            rect: Rect::new(220.0, 400.0, 340.0, 480.0),
            color: red(),
            width: 2.0,
            fill: None,
            opacity: 1.0,
        },
        NewAnnot::Ink {
            strokes: vec![vec![
                Point::new(400.0, 400.0),
                Point::new(450.0, 450.0),
                Point::new(500.0, 400.0),
            ]],
            color: red(),
            width: 3.0,
        },
    ];
    for a in &annots {
        d.add_annotation(0, a).unwrap();
    }
    let r = reopen(&d);
    let list = r.annotations(0);
    let kinds: Vec<AnnotKind> = list.iter().map(|a| a.kind).collect();
    use AnnotKind::*;
    assert_eq!(
        kinds,
        [
            Highlight, Underline, StrikeOut, FreeText, Note, Square, Circle, Ink
        ]
    );
    let ft = &list[3];
    assert_eq!(ft.contents, "Hello (world)\nsecond line");
    assert!(
        (ft.rect.x0 - 299.0).abs() < 0.6 && (ft.rect.y0 - 299.0).abs() < 0.6,
        "{:?}",
        ft.rect
    );
    assert_eq!(list[4].contents, "Note – ünïcode");

    // The appearance streams render: the highlight area turns yellow-ish.
    let img = r.render(0, 1.0).unwrap();
    let q = list[0].rect;
    let (x, y) = ((q.x0 + 2.0) as usize, (q.y0 + 2.0) as usize);
    let px = &img.rgba[(y * img.width as usize + x) * 4..][..4];
    assert!(px[2] < 120 && px[0] > 150, "highlight pixel {px:?}");

    // And deleting one removes it.
    let mut r = r;
    r.delete_annotation(0, 0).unwrap();
    assert_eq!(r.annotations(0).len(), 7);
}

#[test]
fn annotations_on_rotated_pages_map_back() {
    let mut d = doc(1);
    d.rotate_pages(&[0], 90).unwrap();
    let rect = Rect::new(100.0, 50.0, 200.0, 90.0);
    d.add_annotation(
        0,
        &NewAnnot::Square {
            rect,
            color: red(),
            width: 1.0,
            fill: None,
            opacity: 1.0,
        },
    )
    .unwrap();
    let got = reopen(&d).annotations(0)[0].rect;
    assert!(
        (got.x0 - 99.0).abs() < 0.6 && (got.y1 - 91.0).abs() < 0.6,
        "{got:?}"
    );
}

#[test]
fn renders_pixels() {
    let d = doc(1);
    let img = d.render(0, 0.5).unwrap();
    assert_eq!((img.width, img.height), (306, 396));
    // The red rule under the heading.
    let (x, y) = (100usize, ((792.0 - 702.0) * 0.5) as usize);
    let px = &img.rgba[(y * 306 + x) * 4..][..4];
    assert!(px[0] > 180 && px[1] < 100, "{px:?}");
}

#[test]
fn blank_document() {
    let d = Document::blank();
    assert_eq!(d.page_count(), 1);
    assert!(d.page_text(0).glyphs.is_empty());
}

/// Smoke-tests real files: `PDF_CORPUS=/dir cargo test -p pdf-engine corpus -- --ignored --nocapture`
#[test]
#[ignore]
fn corpus() {
    let Ok(dir) = std::env::var("PDF_CORPUS") else {
        return;
    };
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "pdf") {
            continue;
        }
        let t = std::time::Instant::now();
        let mut d = match Document::open(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("{}: open failed: {e}", path.display());
                continue;
            }
        };
        let open = t.elapsed();
        let r = std::time::Instant::now();
        let img = d.render(0, 1.5).expect("render");
        let render = r.elapsed();
        let chars = d.page_text(0).glyphs.len();
        // Try editing up to 12 lines of page 1.
        let t = d.page_text(0);
        let mut starts: Vec<Point> = Vec::new();
        let mut prev_nl = true;
        for g in &t.glyphs {
            if prev_nl && !g.synthetic && starts.len() < 12 {
                starts.push(Point::new(g.rect.x0 + 1.0, g.rect.center_y()));
            }
            prev_nl = g.ch == '\n';
        }
        let mut ok = 0;
        let mut reasons = std::collections::BTreeMap::new();
        for at in &starts {
            match d.text_line_at(0, *at) {
                Ok(_) => ok += 1,
                Err(e) => *reasons.entry(e).or_insert(0) += 1,
            }
        }
        if let Some(at) = starts.iter().find(|a| d.text_line_at(0, **a).is_ok()) {
            let before = d.page_count();
            d.replace_text_line(0, *at, "EDITED LINE", None).unwrap();
            let back = reopen(&d);
            assert_eq!(back.page_count(), before);
            assert!(back.page_text(0).text().contains("EDITED LINE"));
        }
        println!("  editable lines {ok}/{}: {reasons:?}", starts.len());
        // Redact every "the" on page 1 and check it's gone after saving.
        let mut rd = Document::from_bytes(d.bytes().to_vec()).unwrap();
        let hits: Vec<(usize, Rect)> = rd
            .find_redactions(&["the".into()], &[])
            .into_iter()
            .filter(|h| h.page == 0)
            .flat_map(|h| h.rects.into_iter().map(|r| (0, r)))
            .collect();
        if !hits.is_empty() {
            let before = rd.page_text(0).text().to_lowercase().matches("the").count();
            let rep = rd.redact(&hits, false).unwrap();
            let after = reopen(&rd)
                .page_text(0)
                .text()
                .to_lowercase()
                .matches("the")
                .count();
            println!(
                "  redacted {} marks: \"the\" {before} -> {after}; removed {} glyphs, unhandled {:?}",
                hits.len(),
                rep.glyphs_removed,
                rep.unhandled
            );
        }
        d.rotate_pages(&[0], 90).unwrap();
        d.add_annotation(
            0,
            &NewAnnot::Square {
                rect: Rect::new(10.0, 10.0, 60.0, 60.0),
                color: red(),
                width: 1.0,
                fill: None,
                opacity: 1.0,
            },
        )
        .unwrap();
        let back = reopen(&d);
        assert_eq!(back.page_count(), d.page_count());
        println!(
            "{}: {} pages, open {open:?}, render p1 {}x{} {render:?}, {chars} chars, {} bytes",
            path.file_name().unwrap().to_string_lossy(),
            d.page_count(),
            img.width,
            img.height,
            d.bytes().len()
        );
    }
}

#[test]
fn annotations_are_editable_after_reopen() {
    let mut d = doc(1);
    d.add_annotation(
        0,
        &NewAnnot::Square {
            rect: Rect::new(100.0, 400.0, 200.0, 480.0),
            color: red(),
            width: 2.0,
            fill: None,
            opacity: 1.0,
        },
    )
    .unwrap();
    d.add_annotation(
        0,
        &NewAnnot::FreeText {
            rect: Rect::new(300.0, 300.0, 450.0, 330.0),
            text: "Old".into(),
            color: red(),
            size: 12.0,
            fill: None,
        },
    )
    .unwrap();
    let mut r = reopen(&d);
    let sq = r.annotation_shape(0, 0).expect("square reads back");
    assert!((sq.bounds().x0 - 100.0).abs() < 0.6, "{:?}", sq.bounds());
    let moved = sq.transformed(sq.bounds(), Rect::new(150.0, 420.0, 300.0, 520.0));
    r.replace_annotation(0, 0, &moved).unwrap();
    let mut ft = r.annotation_shape(0, 1).unwrap();
    ft.set_text("Nouveau texte – مرحبا".into());
    ft.set_color(Color::rgb(20, 90, 200));
    r.replace_annotation(0, 1, &ft).unwrap();
    let back = reopen(&r);
    let list = back.annotations(0);
    assert_eq!(list.len(), 2, "replaced in place, not added");
    assert!(
        (list[0].rect.x0 - 149.0).abs() < 0.6 && (list[0].rect.x1 - 301.0).abs() < 0.6,
        "{:?}",
        list[0].rect
    );
    assert_eq!(list[1].contents, "Nouveau texte – مرحبا");
    assert_eq!(list[0].author, "Waraq");
    r.undo().unwrap();
    assert_eq!(r.annotations(0)[1].contents, "Old");
}

#[test]
fn page_items_round_trip() {
    let mut d = doc(1);
    let id = d
        .add_item(
            0,
            &NewItem::Text {
                at: Point::new(72.0, 300.0),
                text: "Hello مرحبا".into(),
                face: FontFace::Sans,
                size: 18.0,
                color: red(),
            },
        )
        .unwrap();
    let r = reopen(&d);
    let items = r.items(0);
    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0].kind, ItemKind::Text { text, .. } if text == "Hello مرحبا"));
    // The new text is real page text: extractable and searchable, Arabic included.
    assert_eq!(r.search("hello").len(), 1);
    assert!(
        r.page_text(0).text().contains('م'),
        "{}",
        r.page_text(0).text()
    );
    // Move and re-word it.
    let mut d = r;
    d.update_item(
        0,
        id,
        &ItemEdit {
            rect: Some(Rect::new(100.0, 500.0, 200.0, 520.0)),
            text: Some("Bye".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let it = &reopen(&d).items(0)[0];
    assert!((it.rect.x0 - 100.0).abs() < 0.01 && (it.rect.y0 - 500.0).abs() < 0.01);
    assert!(reopen(&d).page_text(0).text().contains("Bye"));
    // Ink and image items.
    let ink = d
        .add_item(
            0,
            &NewItem::Ink {
                strokes: vec![vec![Point::new(300.0, 600.0), Point::new(340.0, 640.0)]],
                color: red(),
                width: 2.0,
            },
        )
        .unwrap();
    let png = {
        let mut buf = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 255, 255]))
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    d.add_item(
        0,
        &NewItem::Image {
            rect: Rect::new(400.0, 600.0, 440.0, 640.0),
            data: png,
        },
    )
    .unwrap();
    let img = d.render(0, 1.0).unwrap();
    let px = &img.rgba[(620 * img.width as usize + 420) * 4..][..4];
    assert!(px[2] > 200 && px[0] < 40, "image pixel {px:?}");
    d.update_item(
        0,
        ink,
        &ItemEdit {
            rect: Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
            ..Default::default()
        },
    )
    .unwrap();
    d.delete_item(0, ink).unwrap();
    assert_eq!(reopen(&d).items(0).len(), 2);
}

#[test]
fn edits_an_existing_text_line() {
    let mut d = doc(1);
    // Second body line: "Waraq is a native editor …" at baseline y = 792 - 636.
    let at = Point::new(100.0, 152.0);
    let line = d.text_line_at(0, at).expect("editable");
    assert!(
        line.text.starts_with("Waraq is a native editor"),
        "{}",
        line.text
    );
    assert!((line.size - 13.0).abs() < 0.1);
    d.replace_text_line(0, at, "Waraq edits text in place", None)
        .unwrap();
    let r = reopen(&d);
    let text = r.page_text(0).text();
    assert!(text.contains("Waraq edits text in place"), "{text}");
    assert!(
        !text.contains("native editor"),
        "old glyphs removed: {text}"
    );
    assert!(
        text.contains("quick brown fox") && text.contains("Highlight this"),
        "neighbours intact"
    );
    let new = r.search("Waraq edits").remove(0).rects[0];
    assert!(
        (new.y0 - line.rect.y0).abs() < 3.0 && (new.x0 - line.rect.x0).abs() < 3.0,
        "{new:?} vs {:?}",
        line.rect
    );
}

#[test]
fn refuses_to_edit_where_there_is_no_text() {
    let d = doc(1);
    assert!(d.text_line_at(0, Point::new(500.0, 700.0)).is_err());
}

#[test]
fn fill_and_opacity_round_trip() {
    let mut d = doc(1);
    let blue = Color::rgb(40, 90, 200);
    d.add_annotation(
        0,
        &NewAnnot::Square {
            rect: Rect::new(100.0, 500.0, 200.0, 560.0),
            color: red(),
            width: 2.0,
            fill: Some(blue),
            opacity: 0.5,
        },
    )
    .unwrap();
    d.add_annotation(
        0,
        &NewAnnot::FreeText {
            rect: Rect::new(300.0, 500.0, 450.0, 530.0),
            text: "Note".into(),
            color: red(),
            size: 12.0,
            fill: Some(Color::rgb(255, 241, 168)),
        },
    )
    .unwrap();
    let mut r = reopen(&d);
    let sq = r.annotation_shape(0, 0).unwrap();
    assert_eq!(sq.fill().map(|c| (c.b * 255.0).round() as u8), Some(200));
    assert!((sq.opacity().unwrap() - 0.5).abs() < 0.01);
    assert!(r.annotation_shape(0, 1).unwrap().fill().is_some());
    // Half-transparent blue over white renders as a light blue.
    let img = r.render(0, 1.0).unwrap();
    let px = &img.rgba[(530 * img.width as usize + 150) * 4..][..4];
    assert!(px[2] > 200 && px[0] < 180 && px[0] > 100, "{px:?}");
    // Changing it is one undo step.
    let mut sq2 = sq.clone();
    sq2.set_fill(None);
    r.replace_annotation(0, 0, &sq2).unwrap();
    assert!(r.annotation_shape(0, 0).unwrap().fill().is_none());
    r.undo().unwrap();
    assert!(r.annotation_shape(0, 0).unwrap().fill().is_some());
}

#[test]
fn redaction_removes_text_for_real() {
    let mut d = doc(2);
    // "quick brown" on the first body line of page 1.
    let hit = d.find_redactions(&["quick brown".into()], &[]);
    assert_eq!(hit.len(), 2, "one per page");
    let marks: Vec<(usize, Rect)> = hit
        .iter()
        .filter(|h| h.page == 0)
        .flat_map(|h| h.rects.iter().map(|r| (h.page, *r)))
        .collect();
    let report = d.redact(&marks, false).unwrap();
    assert!(report.glyphs_removed >= 11, "{report:?}");
    assert!(report.unhandled.is_empty(), "{report:?}");
    let r = reopen(&d);
    let text = r.page_text(0).text();
    assert!(
        !text.to_lowercase().contains("quick") && !text.contains("brown"),
        "{text}"
    );
    assert!(
        text.contains("The") && text.contains("fox jumps over the lazy dog"),
        "neighbours survive: {text}"
    );
    assert!(text.contains("Waraq is a native editor"), "{text}");
    assert_eq!(r.search("quick brown").len(), 1, "page 2 untouched");
    // Surviving text keeps its position.
    let fox = r.search("fox jumps").remove(0).rects[0];
    let orig = doc(1).search("fox jumps").remove(0).rects[0];
    assert!((fox.x0 - orig.x0).abs() < 0.5, "{fox:?} vs {orig:?}");
    // A black box covers the mark.
    let img = r.render(0, 1.0).unwrap();
    let m = marks[0].1;
    let (x, y) = (
        ((m.x0 + m.x1) / 2.0) as usize,
        ((m.y0 + m.y1) / 2.0) as usize,
    );
    let px = &img.rgba[(y * img.width as usize + x) * 4..][..3];
    assert!(px.iter().all(|&c| c < 30), "{px:?}");
}

#[test]
fn redaction_blacks_out_images_paths_and_annotations() {
    let mut d = doc(1);
    let png = {
        let mut buf = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(20, 20, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    d.add_item(
        0,
        &NewItem::Image {
            rect: Rect::new(300.0, 600.0, 400.0, 700.0),
            data: png,
        },
    )
    .unwrap();
    d.add_annotation(
        0,
        &NewAnnot::Note {
            at: Point::new(100.0, 100.0),
            text: "secret note".into(),
            color: red(),
        },
    )
    .unwrap();
    // The blue rectangle drawn in the sample content sits at y = 792 - 384.. on page 1.
    let marks = vec![
        (0, Rect::new(60.0, 90.0, 130.0, 130.0)),
        (0, Rect::new(60.0, 400.0, 280.0, 540.0)),
        (0, Rect::new(300.0, 600.0, 350.0, 700.0)),
    ];
    let report = d.redact(&marks, true).unwrap();
    assert_eq!(report.annotations_removed, 1, "{report:?}");
    assert!(report.paths_removed >= 1, "{report:?}");
    assert_eq!(report.objects_removed, 1, "{report:?}");
    let r = reopen(&d);
    assert!(r.annotations(0).is_empty());
    assert!(r.items(0).is_empty());
    assert!(!r.page_text(0).text().contains("secret"));
}

#[test]
fn redaction_edits_image_pixels() {
    // An image in the page content itself (not an editor item): copy the item's image into a
    // plain content stream by extracting a page with it drawn through a normal Do.
    let mut d = doc(1);
    let png = {
        let mut buf = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(40, 40, image::Rgba([0, 160, 0, 255]))
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    d.add_item(
        0,
        &NewItem::Image {
            rect: Rect::new(300.0, 600.0, 400.0, 700.0),
            data: png,
        },
    )
    .unwrap();
    // Re-open and flatten: replace the item form by its image drawn directly.
    let bytes = flatten_items(&d.bytes());
    let mut d = Document::from_bytes(bytes).unwrap();
    assert!(d.items(0).is_empty());
    let report = d
        .redact(&[(0, Rect::new(300.0, 600.0, 350.0, 700.0))], false)
        .unwrap();
    assert_eq!(report.images_edited, 1, "{report:?}");
    let img = reopen(&d).render(0, 1.0).unwrap();
    let at = |x: usize, y: usize| img.rgba[(y * img.width as usize + x) * 4..][..3].to_vec();
    assert!(
        at(375, 650)[1] > 120,
        "right half still green: {:?}",
        at(375, 650)
    );
    // Under the black box the image data itself is black too: check the stored image.
    let doc = lopdf::Document::load_mem(&d.bytes()).unwrap();
    let blacked = doc.objects.values().any(|o| {
        o.as_stream().ok().is_some_and(|s| {
            s.dict.get(b"Subtype").and_then(lopdf::Object::as_name).ok() == Some(b"Image")
                && s.dict.get(b"Width").and_then(lopdf::Object::as_i64).ok() == Some(40)
                && s.decompressed_content().is_ok_and(|px| {
                    px.len() >= 3 * 40 * 40
                        && px[..3] == [0, 0, 0]
                        && px[3 * 39..3 * 40] == [0, 160, 0]
                })
        })
    });
    assert!(
        blacked,
        "left half of the image data is black, right half untouched"
    );
}

/// Test helper: turns editor items into plain page content (`Do` of the image directly).
fn flatten_items(bytes: &[u8]) -> Vec<u8> {
    use lopdf::Object;
    let mut doc = lopdf::Document::load_mem(bytes).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let image = doc
        .objects
        .iter()
        .find(|(_, o)| {
            o.as_stream().ok().is_some_and(|s| {
                s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image")
            })
        })
        .map(|(id, _)| *id)
        .unwrap();
    let content = b"q 100 0 0 100 300 92 cm /ImX Do Q".to_vec();
    let c = doc.add_object(lopdf::Stream::new(lopdf::Dictionary::new(), content));
    let res = lopdf::dictionary! { "XObject" => lopdf::dictionary! { "ImX" => image } };
    let p = doc.get_dictionary_mut(page).unwrap();
    p.set("Contents", c);
    p.set("Resources", res);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn finds_patterns() {
    let mut d = doc(1);
    d.add_item(0, &NewItem::Text {
        at: Point::new(72.0, 300.0),
        text: "Mail jane.doe@example.com or call +1 (555) 123-4567 by 2026-11-03; card 4111 1111 1111 1111".into(),
        face: FontFace::Sans,
        size: 9.0,
        color: red(),
    })
    .unwrap();
    let hits = d.find_redactions(&[], &Pattern::ALL);
    let texts: Vec<&str> = hits.iter().map(|h| h.text.as_str()).collect();
    assert!(texts.contains(&"jane.doe@example.com"), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|t| t.contains("555") && t.contains("4567")),
        "{texts:?}"
    );
    assert!(texts.contains(&"2026-11-03"), "{texts:?}");
    assert!(texts.contains(&"4111 1111 1111 1111"), "{texts:?}");
}

#[test]
fn forms_create_fill_and_read_back() {
    let mut d = doc(1);
    let f = |kind, name: &str, rect, options: &[&str]| NewField {
        kind,
        name: name.into(),
        rect,
        options: options.iter().map(|s| s.to_string()).collect(),
    };
    d.add_field(
        0,
        &f(
            FieldKind::Text,
            "full_name",
            Rect::new(72.0, 300.0, 300.0, 322.0),
            &[],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::CheckBox,
            "agree",
            Rect::new(72.0, 340.0, 86.0, 354.0),
            &[],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::Radio,
            "size",
            Rect::new(72.0, 370.0, 86.0, 384.0),
            &["Small"],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::Radio,
            "size",
            Rect::new(100.0, 370.0, 114.0, 384.0),
            &["Large"],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::ComboBox,
            "country",
            Rect::new(72.0, 400.0, 220.0, 420.0),
            &["Morocco", "France", "Egypt"],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::ListBox,
            "colour",
            Rect::new(72.0, 430.0, 220.0, 480.0),
            &["Red", "Green", "Blue"],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::Signature,
            "signature",
            Rect::new(300.0, 430.0, 500.0, 480.0),
            &[],
        ),
    )
    .unwrap();
    assert!(
        d.add_field(
            0,
            &f(
                FieldKind::Text,
                "agree",
                Rect::new(0.0, 0.0, 10.0, 10.0),
                &[]
            )
        )
        .is_err(),
        "names are unique"
    );

    let mut r = reopen(&d);
    r.set_field_value("full_name", "سالم Ben 42").unwrap();
    r.set_field_value("agree", "Yes").unwrap();
    r.set_field_value("size", "Large").unwrap();
    r.set_field_value("country", "France").unwrap();
    r.set_field_value("colour", "Blue").unwrap();
    let back = reopen(&r);
    let fields = back.fields();
    let get = |n: &str| fields.iter().find(|f| f.name == n).unwrap().clone();
    assert_eq!(
        fields.len(),
        6,
        "{:?}",
        fields
            .iter()
            .map(|f| (&f.name, f.kind, f.widgets.len()))
            .collect::<Vec<_>>()
    );
    assert_eq!(get("full_name").value, "سالم Ben 42");
    assert_eq!(get("agree").value, "Yes");
    assert_eq!(get("size").value, "Large");
    assert_eq!(get("size").options, ["Small", "Large"]);
    assert!(
        get("size").is_on(&get("size").widgets[1]) && !get("size").is_on(&get("size").widgets[0])
    );
    assert_eq!(get("country").kind, FieldKind::ComboBox);
    assert_eq!(get("country").value, "France");
    assert_eq!(get("colour").options, ["Red", "Green", "Blue"]);
    assert_eq!(get("signature").kind, FieldKind::Signature);
    // Appearances are real: the filled value is drawn (and extractable) on the page.
    let full = lopdf::Document::load_mem(&back.bytes()).unwrap();
    let widget_has_ap = full
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Subtype").and_then(lopdf::Object::as_name).ok() == Some(b"Widget"))
        .all(|d| d.has(b"AP"));
    assert!(widget_has_ap);
    let img = back.render(0, 1.0).unwrap();
    let dark = (300..322)
        .flat_map(|y| (75..250).map(move |x| (x, y)))
        .filter(|&(x, y)| img.rgba[(y * img.width as usize + x) * 4] < 100)
        .count();
    assert!(dark > 30, "text drawn in the field ({dark} dark pixels)");
    // Edit properties, move, delete.
    let mut e = back;
    e.set_field_properties(
        "country",
        "country_of_birth",
        "Morocco",
        &["Morocco".into(), "Tunisia".into()],
    )
    .unwrap();
    let c = e
        .fields()
        .into_iter()
        .find(|f| f.name == "country_of_birth")
        .unwrap();
    assert_eq!(c.options, ["Morocco", "Tunisia"]);
    assert_eq!(c.default, "Morocco");
    let w = c.widgets[0].clone();
    e.set_widget_rect(0, w.slot, Rect::new(300.0, 300.0, 450.0, 320.0))
        .unwrap();
    let c = e
        .fields()
        .into_iter()
        .find(|f| f.name == "country_of_birth")
        .unwrap();
    assert!((c.widgets[0].rect.x0 - 300.0).abs() < 0.5);
    e.delete_field(0, c.widgets[0].slot).unwrap();
    let names: Vec<String> = reopen(&e).fields().into_iter().map(|f| f.name).collect();
    assert!(
        !names.contains(&"country_of_birth".to_string()) && names.len() == 5,
        "{names:?}"
    );
}

/// Writes sample outputs for checking in other software:
/// `PDF_EXPORT=/dir cargo test -p pdf-engine export_samples -- --ignored`
#[test]
#[ignore]
fn export_samples() {
    let Ok(dir) = std::env::var("PDF_EXPORT") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let mut d = doc(1);
    let f = |kind, name: &str, rect, options: &[&str]| NewField {
        kind,
        name: name.into(),
        rect,
        options: options.iter().map(|s| s.to_string()).collect(),
    };
    d.add_field(
        0,
        &f(
            FieldKind::Text,
            "full_name",
            Rect::new(72.0, 300.0, 300.0, 322.0),
            &[],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::CheckBox,
            "agree",
            Rect::new(72.0, 340.0, 86.0, 354.0),
            &[],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::Radio,
            "size",
            Rect::new(72.0, 370.0, 86.0, 384.0),
            &["Small"],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::Radio,
            "size",
            Rect::new(100.0, 370.0, 114.0, 384.0),
            &["Large"],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::ComboBox,
            "country",
            Rect::new(72.0, 400.0, 220.0, 420.0),
            &["Morocco", "France"],
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::ListBox,
            "colour",
            Rect::new(72.0, 430.0, 220.0, 480.0),
            &["Red", "Green", "Blue"],
        ),
    )
    .unwrap();
    d.set_field_value("full_name", "Salma سلمى 2026").unwrap();
    d.set_field_value("agree", "Yes").unwrap();
    d.set_field_value("size", "Large").unwrap();
    d.set_field_value("country", "France").unwrap();
    d.set_field_value("colour", "Green").unwrap();
    std::fs::write(dir.join("form.pdf"), d.bytes().as_slice()).unwrap();
    let mut r = doc(1);
    let marks: Vec<(usize, Rect)> = r
        .find_redactions(&["quick brown".into()], &[])
        .into_iter()
        .flat_map(|h| h.rects.into_iter().map(move |x| (h.page, x)))
        .collect();
    r.redact(&marks, true).unwrap();
    std::fs::write(dir.join("redacted.pdf"), r.bytes().as_slice()).unwrap();
}

const ARABIC_MIX: &str = "مرحبا بالعالم 2026 مع PDF";

#[test]
fn arabic_added_text_extracts_and_searches_logically() {
    let mut d = doc(1);
    d.add_item(
        0,
        &NewItem::Text {
            at: Point::new(72.0, 300.0),
            text: ARABIC_MIX.into(),
            face: FontFace::Sans,
            size: 20.0,
            color: red(),
        },
    )
    .unwrap();
    let r = reopen(&d);
    let text = r.page_text(0).text();
    assert!(text.contains(ARABIC_MIX), "{text}");
    for q in ["بالعالم", "2026", "مع PDF", "مرحبا بالعالم"] {
        assert_eq!(r.search(q).len(), 1, "{q}");
    }
    // The first logical word sits at the right (RTL paragraph), the number left of it.
    let hello = r.search("مرحبا").remove(0).rects[0];
    let year = r.search("2026").remove(0).rects[0];
    assert!(hello.x0 > year.x1, "{hello:?} {year:?}");
}

#[test]
fn arabic_in_visual_order_from_other_producers_is_searchable() {
    // Build a page the way many producers write Arabic: glyphs left to right.
    let mut raw = lopdf::Document::load_mem(&sample_pdf(1)).unwrap();
    let page = *raw.get_pages().values().next().unwrap();
    let mut p = crate::fonts::TextPainter::default();
    p.visual_order = true;
    p.text(
        ARABIC_MIX,
        FontFace::Sans,
        20.0,
        72.0,
        320.0,
        [0.0, 0.0, 0.0],
    );
    let content = format!("q 1 0 0 -1 0 792 cm\n{}Q\n", p.take_content());
    let fonts = p.fonts(&mut raw);
    let c = raw.add_object(lopdf::Stream::new(
        lopdf::Dictionary::new(),
        content.into_bytes(),
    ));
    let pd = raw.get_dictionary_mut(page).unwrap();
    pd.set("Contents", c);
    pd.set("Resources", lopdf::dictionary! { "Font" => fonts });
    let mut bytes = Vec::new();
    raw.save_to(&mut bytes).unwrap();
    let d = Document::from_bytes(bytes).unwrap();
    assert!(
        d.page_text(0).text().contains(ARABIC_MIX),
        "{}",
        d.page_text(0).text()
    );
    assert_eq!(d.search("بالعالم").len(), 1);
}

#[test]
fn arabic_text_box_is_shaped_and_right_aligned() {
    let mut d = doc(1);
    let rect = Rect::new(100.0, 600.0, 400.0, 640.0);
    d.add_annotation(
        0,
        &NewAnnot::FreeText {
            rect,
            text: "سلام".into(),
            color: Color::rgb(0, 0, 0),
            size: 24.0,
            fill: None,
        },
    )
    .unwrap();
    let img = reopen(&d).render(0, 1.0).unwrap();
    let ink = |x0: usize, x1: usize| {
        (605..635)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| img.rgba[(y * img.width as usize + x) * 4] < 128)
            .count()
    };
    assert!(ink(300, 400) > 40, "text sits at the right of the box");
    assert_eq!(ink(100, 250), 0, "nothing on the left");
    // Joined letters: shaping picks contextual forms, not the isolated ones.
    let shaped: Vec<u16> = crate::fonts::shaped_gids("سلام", FontFace::Sans);
    let isolated: Vec<u16> = "سلام"
        .chars()
        .rev()
        .map(|c| crate::fonts::shaped_gids(&c.to_string(), FontFace::Sans)[0])
        .collect();
    assert_ne!(shaped, isolated, "contextual forms are used");
}

#[test]
fn highlight_follows_rtl_text() {
    let mut d = doc(1);
    d.add_item(
        0,
        &NewItem::Text {
            at: Point::new(72.0, 300.0),
            text: ARABIC_MIX.into(),
            face: FontFace::Sans,
            size: 20.0,
            color: red(),
        },
    )
    .unwrap();
    let t = d.page_text(0);
    let s: Vec<char> = t.glyphs.iter().map(|g| g.ch).collect();
    let start = s
        .iter()
        .collect::<String>()
        .find("بالعالم")
        .map(|b| s.iter().collect::<String>()[..b].chars().count())
        .unwrap();
    let rects = t.line_rects(start, start + "بالعالم".chars().count() - 1);
    assert_eq!(rects.len(), 1, "one box for one word");
    let word = d.search("بالعالم").remove(0).rects[0];
    assert!((rects[0].x0 - word.x0).abs() < 0.5 && (rects[0].x1 - word.x1).abs() < 0.5);
    let hello = d.search("مرحبا").remove(0).rects[0];
    assert!(
        rects[0].x1 <= hello.x0 + 0.5,
        "the second word is left of the first"
    );
}

#[test]
fn redaction_reaches_into_form_xobjects() {
    // Move the page's whole content into a Form XObject, as some producers do.
    let mut raw = lopdf::Document::load_mem(&sample_pdf(1)).unwrap();
    let page = *raw.get_pages().values().next().unwrap();
    let content = raw.get_page_content(page);
    let res = raw
        .get_dictionary(page)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .clone();
    let form = raw.add_object(lopdf::Stream::new(
        lopdf::dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()], "Resources" => res },
        content,
    ));
    let c = raw.add_object(lopdf::Stream::new(
        lopdf::Dictionary::new(),
        b"q /Fm0 Do Q".to_vec(),
    ));
    let pd = raw.get_dictionary_mut(page).unwrap();
    pd.set("Contents", c);
    pd.set(
        "Resources",
        lopdf::dictionary! { "XObject" => lopdf::dictionary! { "Fm0" => form } },
    );
    let mut bytes = Vec::new();
    raw.save_to(&mut bytes).unwrap();
    let mut d = Document::from_bytes(bytes).unwrap();
    let marks: Vec<(usize, Rect)> = d
        .find_redactions(&["quick brown".into()], &[])
        .into_iter()
        .flat_map(|h| h.rects.into_iter().map(move |r| (h.page, r)))
        .collect();
    let report = d.redact(&marks, false).unwrap();
    assert!(
        report.glyphs_removed >= 11 && report.unhandled.is_empty(),
        "{report:?}"
    );
    let text = reopen(&d).page_text(0).text();
    assert!(
        !text.contains("quick") && text.contains("fox jumps over"),
        "{text}"
    );
}
