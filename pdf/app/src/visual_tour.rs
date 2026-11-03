//! Renders the main screens to PNG for visual review, plus smoke tests that
//! drive the real UI through egui_kittest.
//!
//! Tour: `PDF_SHOTS=/some/dir cargo test -p pdf-app visual_tour -- --ignored`

use eframe::egui::{self, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdf_engine::{Color, Document, NewAnnot, NewItem, Point, Rect};

use crate::app::{App, SidePanel, ThemeChoice, Tool, ToolSet};
use crate::theme;

fn sample(pages: usize) -> Document {
    Document::from_bytes(pdf_engine::sample::sample_pdf(pages)).expect("sample")
}

fn harness(size: Vec2, setup: impl FnOnce(&mut App) + 'static) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_eframe(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::install(&cc.egui_ctx);
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            let mut app = App::with_context(&cc.egui_ctx, false);
            app.theme = ThemeChoice::Light;
            setup(&mut app);
            app
        });
    h.run_steps(8);
    h
}

fn shoot(name: &str, size: Vec2, setup: impl FnOnce(&mut App) + 'static) {
    let Ok(dir) = std::env::var("PDF_SHOTS") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = harness(size, setup);
    let image = h.render().expect("render");
    image
        .save(std::path::Path::new(&dir).join(format!("{name}.png")))
        .unwrap();
}

fn annotated() -> Document {
    let mut d = sample(4);
    let t = d.page_text(0);
    let a = t.glyph_at(Point::new(80.0, 177.0), 4.0).unwrap();
    let quads = t.line_rects(a, a + 30);
    let red = Color::rgb(200, 45, 45);
    for annot in [
        NewAnnot::Highlight {
            quads,
            color: Color::rgb(255, 214, 10),
        },
        NewAnnot::FreeText {
            rect: Rect::new(320.0, 420.0, 540.0, 470.0),
            text: "Looks good —\nship it.".into(),
            color: red,
            size: 16.0,
            fill: None,
        },
        NewAnnot::Note {
            at: Point::new(540.0, 60.0),
            text: "Check the title".into(),
            color: Color::rgb(255, 205, 70),
        },
        NewAnnot::Circle {
            rect: Rect::new(300.0, 300.0, 420.0, 380.0),
            color: red,
            width: 2.0,
            fill: None,
            opacity: 1.0,
        },
        NewAnnot::Ink {
            strokes: vec![
                (0..40)
                    .map(|i| {
                        Point::new(80.0 + i as f32 * 6.0, 560.0 + (i as f32 * 0.5).sin() * 12.0)
                    })
                    .collect(),
            ],
            color: Color::rgb(40, 90, 200),
            width: 2.5,
        },
    ] {
        d.add_annotation(0, &annot).unwrap();
    }
    d
}

#[test]
#[ignore]
fn visual_tour() {
    let big = Vec2::new(1360.0, 860.0);
    let small = Vec2::new(960.0, 640.0);
    shoot("01_welcome", big, |_| {});
    shoot("02_view", big, |a| {
        a.load_document(sample(5), None, "Quarterly report.pdf".into())
    });
    shoot("03_view_dark", big, |a| {
        a.load_document(sample(5), None, "Quarterly report.pdf".into());
        a.theme = ThemeChoice::Dark;
    });
    shoot("04_organize", big, |a| {
        a.load_document(sample(9), None, "Quarterly report.pdf".into());
        a.toolset = ToolSet::Organize;
        a.selection = [1, 2].into();
        a.rotate(90);
    });
    shoot("05_organize_dark", big, |a| {
        a.load_document(sample(9), None, "Quarterly report.pdf".into());
        a.toolset = ToolSet::Organize;
        a.selection = [4].into();
        a.theme = ThemeChoice::Dark;
    });
    shoot("06_comments", big, |a| {
        a.load_document(annotated(), None, "Reviewed.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::Highlight;
        a.side = Some(SidePanel::Comments);
        a.picked = Some(crate::app::Picked::Annot { page: 0, slot: 1 });
    });
    shoot("07_search", big, |a| {
        a.load_document(sample(3), None, "Quarterly report.pdf".into());
        a.search.query = "pages".into();
        a.run_search(true);
    });
    shoot("08_text_draft", big, |a| {
        a.load_document(sample(2), None, "Quarterly report.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::TextBox;
        a.draft = Some(crate::app::TextDraft {
            page: 0,
            rect: Rect::new(320.0, 300.0, 520.0, 330.0),
            target: crate::app::DraftTarget::NewTextBox,
            text: "Please double-check".into(),
            focus: true,
        });
    });
    shoot("09_comments_dark", big, |a| {
        a.load_document(annotated(), None, "Reviewed.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::Ink;
        a.side = Some(SidePanel::Comments);
        a.theme = ThemeChoice::Dark;
    });
    if let Ok(path) = std::env::var("PDF_SAMPLE") {
        shoot("11_real_file", big, move |a| {
            a.open_path(std::path::Path::new(&path))
        });
    }
    shoot("12_comment_selected", big, |a| {
        a.load_document(annotated(), None, "Reviewed.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::Select;
        a.picked = Some(crate::app::Picked::Annot { page: 0, slot: 3 });
    });
    shoot("13_edit_line", big, |a| {
        a.load_document(sample(2), None, "Quarterly report.pdf".into());
        a.toolset = ToolSet::Edit;
        a.tool = Tool::EditText;
        let at = Point::new(100.0, 152.0);
        let line = a.doc.as_ref().unwrap().text_line_at(0, at).unwrap();
        a.draft = Some(crate::app::TextDraft {
            page: 0,
            rect: line.rect,
            text: "Waraq rewrites this line in place – بالعربية أيضاً".into(),
            target: crate::app::DraftTarget::Line { at, line },
            focus: true,
        });
    });
    shoot("14_edited_text", big, |a| {
        let mut d = sample(2);
        d.replace_text_line(
            0,
            Point::new(100.0, 152.0),
            "Waraq rewrites this line in place – بالعربية أيضاً",
            None,
        )
        .unwrap();
        let png = {
            let mut buf = std::io::Cursor::new(Vec::new());
            image::RgbaImage::from_fn(120, 80, |x, y| {
                image::Rgba([(x * 2) as u8, 90, (y * 3) as u8, 255])
            })
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
            buf.into_inner()
        };
        d.add_item(
            0,
            &NewItem::Image {
                rect: Rect::new(360.0, 250.0, 480.0, 330.0),
                data: png,
            },
        )
        .unwrap();
        a.load_document(d, None, "Quarterly report.pdf".into());
        a.toolset = ToolSet::Edit;
        a.tool = Tool::Select;
        let id = a.doc.as_ref().unwrap().items(0)[1].id;
        a.picked = Some(crate::app::Picked::Item { page: 0, id });
    });
    shoot("15_sign", big, |a| {
        let mut d = sample(1);
        let ink: Vec<Vec<[f32; 2]>> = vec![
            (0..60)
                .map(|i| {
                    let t = i as f32 / 10.0;
                    [t * 0.6, 0.5 + (t * 2.2).sin() * 0.35 * (1.0 - t / 7.0)]
                })
                .collect(),
        ];
        a.signatures
            .add(crate::signatures::SigData::Ink(ink), false)
            .unwrap();
        a.signatures
            .add(
                crate::signatures::SigData::Typed("Salih Maoui".into()),
                false,
            )
            .unwrap();
        a.signatures
            .add(crate::signatures::SigData::Typed("SM".into()), true)
            .unwrap();
        for (stamp, at) in [
            (crate::app::Stamp::Signature(1), Point::new(140.0, 330.0)),
            (crate::app::Stamp::Date, Point::new(380.0, 330.0)),
            (crate::app::Stamp::Check, Point::new(90.0, 380.0)),
        ] {
            let item = crate::signatures::stamp_item(&a.signatures, stamp, at).unwrap();
            d.add_item(0, &item).unwrap();
        }
        a.load_document(d, None, "Contract.pdf".into());
        a.toolset = ToolSet::Sign;
        a.tool = Tool::Place(crate::app::Stamp::Signature(0));
    });
    shoot("16_sign_dialog", big, |a| {
        a.load_document(sample(1), None, "Contract.pdf".into());
        a.toolset = ToolSet::Sign;
        let mut d = crate::signatures::Dialog::new(false);
        d.tab = 1;
        d.typed = "Salih Maoui".into();
        a.sign_dialog = Some(d);
    });
    shoot("17_edit_dark", big, |a| {
        a.load_document(annotated(), None, "Reviewed.pdf".into());
        a.toolset = ToolSet::Edit;
        a.tool = Tool::AddText;
        a.theme = ThemeChoice::Dark;
    });
    shoot("18_arabic_ui", big, |a| {
        a.settings.lang = crate::i18n::Lang::Ar;
        a.load_document(annotated(), None, "عقد.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::Rectangle;
        a.side = Some(SidePanel::Comments);
    });
    shoot("19_french_forms", big, |a| {
        a.settings.lang = crate::i18n::Lang::Fr;
        let mut d = with_fields();
        d.set_field_value("first", "Salma").unwrap();
        d.set_field_value("second", "سلمى بن علي 2026").unwrap();
        d.set_field_value("agree", "Yes").unwrap();
        a.load_document(d, None, "Formulaire.pdf".into());
        a.toolset = ToolSet::Forms;
        a.tool = Tool::Fill;
        a.forms.focused = Some("second".into());
    });
    shoot("20_prepare_form", big, |a| {
        let mut d = with_fields();
        d.add_field(
            0,
            &pdf_engine::NewField {
                kind: pdf_engine::FieldKind::ComboBox,
                name: "country".into(),
                rect: Rect::new(320.0, 300.0, 480.0, 322.0),
                options: vec!["Morocco".into(), "France".into()],
            },
        )
        .unwrap();
        a.load_document(d, None, "form.pdf".into());
        a.toolset = ToolSet::Forms;
        a.tool = Tool::Select;
        let slot = a
            .doc
            .as_ref()
            .unwrap()
            .fields()
            .into_iter()
            .find(|f| f.name == "country")
            .unwrap()
            .widgets[0]
            .slot;
        a.picked = Some(crate::app::Picked::Field { page: 0, slot });
    });
    shoot("21_redact_marks", big, |a| {
        a.load_document(sample(2), None, "Report.pdf".into());
        a.toolset = ToolSet::Redact;
        a.tool = Tool::RedactText;
        a.redact.words = "quick brown".into();
        let hits = a
            .doc
            .as_ref()
            .unwrap()
            .find_redactions(&["quick brown".into(), "Waraq".into()], &[]);
        for h in hits {
            for r in h.rects {
                a.redact.add(h.page, r, h.text.clone());
            }
        }
        a.redact
            .add(0, Rect::new(72.0, 380.0, 280.0, 520.0), String::new());
    });
    shoot("22_redacted", big, |a| {
        let mut d = sample(2);
        let marks: Vec<(usize, Rect)> = d
            .find_redactions(&["quick brown".into()], &[pdf_engine::Pattern::Email])
            .into_iter()
            .flat_map(|h| h.rects.into_iter().map(move |r| (h.page, r)))
            .collect();
        let report = d.redact(&marks, true).unwrap();
        a.load_document(d, None, "Report.pdf".into());
        a.toolset = ToolSet::Redact;
        a.redact.report = Some(report);
    });
    shoot("23_about", big, |a| {
        a.load_document(sample(1), None, "Report.pdf".into());
        a.about_open = true;
    });
    shoot("23b_about_dark", big, |a| {
        a.load_document(sample(1), None, "Report.pdf".into());
        a.theme = ThemeChoice::Dark;
        a.about_open = true;
    });
    shoot("23c_about_ar", big, |a| {
        a.settings.lang = crate::i18n::Lang::Ar;
        a.load_document(sample(1), None, "Report.pdf".into());
        a.about_open = true;
    });
    shoot("24_settings_ar", big, |a| {
        a.settings.lang = crate::i18n::Lang::Ar;
        a.load_document(sample(1), None, "Report.pdf".into());
        a.settings_open = true;
    });
    shoot("25_fill_colour", big, |a| {
        let mut d = sample(1);
        d.add_annotation(
            0,
            &NewAnnot::Square {
                rect: Rect::new(320.0, 300.0, 520.0, 380.0),
                color: Color::rgb(40, 90, 200),
                width: 2.0,
                fill: Some(Color::rgb(205, 228, 252)),
                opacity: 0.8,
            },
        )
        .unwrap();
        d.add_annotation(
            0,
            &NewAnnot::FreeText {
                rect: Rect::new(320.0, 420.0, 540.0, 470.0),
                text: "ملاحظة مهمة: راجع الصفحة 3".into(),
                color: Color::rgb(21, 21, 21),
                size: 16.0,
                fill: Some(Color::rgb(255, 241, 168)),
            },
        )
        .unwrap();
        a.load_document(d, None, "Report.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::Select;
        a.picked = Some(crate::app::Picked::Annot { page: 0, slot: 0 });
    });
    shoot("10_small", small, |a| {
        a.load_document(
            sample(3),
            None,
            "A document with a rather long file name.pdf".into(),
        );
        a.zoom = 1.5;
        a.fit_width = false;
    });
}

/// Drives the real UI: tool-set tabs, organize actions and undo through the toolbar.
#[test]
fn ui_smoke() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(sample(3), None, "t.pdf".into())
    });
    h.get_by_role_and_label(egui::Role::Tab, "Organize pages")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().toolset, ToolSet::Organize);
    h.get_by_label("Page 2").click();
    h.run_steps(3);
    assert_eq!(h.state().selection.iter().copied().collect::<Vec<_>>(), [1]);
    h.get_by_label("Delete").click();
    h.run_steps(3);
    assert_eq!(h.state().doc.as_ref().unwrap().page_count(), 2);
    h.get_by_label("Undo").click();
    h.run_steps(3);
    assert_eq!(h.state().doc.as_ref().unwrap().page_count(), 3);
    h.get_by_role_and_label(egui::Role::Tab, "Comment").click();
    h.run_steps(2);
    h.get_by_label("Highlight").click();
    h.run_steps(2);
    assert_eq!(h.state().tool, Tool::Highlight);
}

/// Dragging across text with the highlight tool creates a highlight annotation.
#[test]
fn highlight_by_drag() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(sample(1), None, "t.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::Highlight;
    });
    let page = h.get_by_label("Page 1").rect();
    let zoom = h.state().zoom;
    // First body line: 132 pt from the top, starting at x = 72 pt.
    let at = |x: f32, y: f32| page.min + egui::vec2(x, y) * zoom;
    h.drag_at(at(75.0, 128.0));
    h.run_steps(2);
    h.hover_at(at(150.0, 128.0));
    h.run_steps(2);
    h.hover_at(at(250.0, 129.0));
    h.run_steps(2);
    h.drop_at(at(250.0, 129.0));
    h.run_steps(3);
    let doc = h.state().doc.as_ref().unwrap();
    let list = doc.annotations(0);
    assert_eq!(list.len(), 1, "one highlight");
    assert_eq!(list[0].kind, pdf_engine::AnnotKind::Highlight);
    assert!(
        list[0].rect.x0 < 80.0 && list[0].rect.x1 > 200.0,
        "{:?}",
        list[0].rect
    );
}

fn click_at(h: &mut Harness<'static, App>, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        });
        h.run_steps(1);
    }
}

fn drag(h: &mut Harness<'static, App>, from: egui::Pos2, to: egui::Pos2) {
    h.drag_at(from);
    h.run_steps(2);
    let mid = from + (to - from) * 0.5;
    h.hover_at(mid);
    h.run_steps(2);
    h.hover_at(to);
    h.run_steps(2);
    h.drop_at(to);
    h.run_steps(3);
}

fn page_point(h: &Harness<'static, App>, x: f32, y: f32) -> egui::Pos2 {
    let page = h.get_by_label("Page 1").rect();
    page.min + egui::vec2(x, y) * h.state().zoom
}

fn with_square_and_text_box() -> Document {
    let mut d = sample(1);
    let red = Color::rgb(200, 45, 45);
    d.add_annotation(
        0,
        &NewAnnot::Square {
            rect: Rect::new(100.0, 400.0, 200.0, 480.0),
            color: red,
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
            text: "Draft".into(),
            color: red,
            size: 14.0,
            fill: None,
        },
    )
    .unwrap();
    // Reopen so these are "existing" comments, as from another app.
    Document::from_bytes(d.bytes().to_vec()).unwrap()
}

/// Selecting an existing rectangle and dragging it moves it; the saved /Rect changes.
#[test]
fn drag_moves_rectangle() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(with_square_and_text_box(), None, "t.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::Select;
    });
    let from = page_point(&h, 150.0, 405.0);
    let to = page_point(&h, 200.0, 425.0);
    drag(&mut h, from, to);
    let bytes = h.state().doc.as_ref().unwrap().bytes().to_vec();
    let saved = Document::from_bytes(bytes).unwrap();
    let r = saved.annotations(0)[0].rect;
    assert!(
        (r.x0 - 149.0).abs() < 1.5 && (r.y0 - 419.0).abs() < 1.5,
        "{r:?}"
    );
    assert_eq!(
        h.state().picked,
        Some(crate::app::Picked::Annot { page: 0, slot: 0 })
    );
    // Resize from the bottom-right handle.
    let corner = page_point(&h, 250.0, 500.0);
    drag(&mut h, corner, corner + egui::vec2(40.0, 40.0));
    let r2 = h.state().doc.as_ref().unwrap().annotations(0)[0].rect;
    assert!(r2.x1 > r.x1 + 10.0 && (r2.x0 - r.x0).abs() < 1.0, "{r2:?}");
    // Undo restores the previous size.
    h.state_mut().undo();
    assert!((h.state().doc.as_ref().unwrap().annotations(0)[0].rect.x1 - r.x1).abs() < 0.5);
}

/// Double-clicking a text box opens the editor; Apply rewrites /Contents.
#[test]
fn edits_text_box_text() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(with_square_and_text_box(), None, "t.pdf".into());
        a.toolset = ToolSet::Comment;
        a.tool = Tool::Select;
    });
    // Let the clock run past the double-click window from start-up.
    h.run_steps(60);
    let at = page_point(&h, 360.0, 315.0);
    // Harness frames are ~0.25 s apart; widen the double-click window to match.
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 1.5);
    click_at(&mut h, at);
    click_at(&mut h, at);
    h.run_steps(2);
    assert!(
        h.state().draft.is_some(),
        "double-click opens the editor: {:?} {:?}",
        h.state().picked,
        h.state().toast
    );
    h.state_mut().draft.as_mut().unwrap().text = "Approved – موافق".into();
    h.run_steps(1);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    let doc = h.state().doc.as_ref().unwrap();
    let list = doc.annotations(0);
    assert_eq!(list.len(), 2);
    assert_eq!(list[1].contents, "Approved – موافق");
}

/// Edit PDF: click a line with Edit text, change it, apply.
#[test]
fn edits_page_text_line() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(sample(1), None, "t.pdf".into());
        a.toolset = ToolSet::Edit;
        a.tool = Tool::EditText;
    });
    let at = page_point(&h, 100.0, 152.0);
    click_at(&mut h, at);
    h.run_steps(2);
    let draft = h.state_mut().draft.as_mut().expect("line editor opens");
    assert!(draft.text.starts_with("Waraq is a native editor"));
    draft.text = "A different second line".into();
    h.run_steps(1);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    let text = h.state().doc.as_ref().unwrap().page_text(0).text();
    assert!(
        text.contains("A different second line") && !text.contains("native editor"),
        "{text}"
    );
    assert!(
        matches!(h.state().picked, Some(crate::app::Picked::Item { .. })),
        "new text is selected"
    );
}

/// Fill & Sign: place a date and a check mark, then move the date.
#[test]
fn places_date_and_check() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(sample(1), None, "t.pdf".into());
        a.toolset = ToolSet::Sign;
        a.tool = Tool::Place(crate::app::Stamp::Date);
    });
    let at = page_point(&h, 400.0, 300.0);
    click_at(&mut h, at);
    h.state_mut().tool = Tool::Place(crate::app::Stamp::Check);
    let check = page_point(&h, 400.0, 350.0);
    click_at(&mut h, check);
    let items = h.state().doc.as_ref().unwrap().items(0);
    assert_eq!(items.len(), 2, "{:?}", h.state().toast);
    assert!(
        matches!(&items[0].kind, pdf_engine::ItemKind::Text { text, .. } if text == &crate::signatures::today())
    );
    h.state_mut().tool = Tool::Select;
    let from = page_point(&h, items[0].rect.x0 + 10.0, items[0].rect.y0 + 6.0);
    drag(&mut h, from, from + egui::vec2(0.0, -60.0));
    let moved = &h.state().doc.as_ref().unwrap().items(0)[0];
    assert!(
        moved.rect.y0 < items[0].rect.y0 - 20.0,
        "{:?} vs {:?}",
        moved.rect,
        items[0].rect
    );
}

/// Redact: drag across text, review, apply; the text is gone from the saved file.
#[test]
fn redacts_marked_text() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(sample(1), None, "t.pdf".into());
        a.toolset = ToolSet::Redact;
        a.tool = Tool::RedactText;
    });
    // "quick brown" on the first body line (baseline 132 pt from the top).
    let (from, to) = (page_point(&h, 100.0, 128.0), page_point(&h, 162.0, 128.0));
    drag(&mut h, from, to);
    assert_eq!(h.state().redact.marks.len(), 1, "one line marked");
    h.get_by_label("Apply redactions…").click();
    h.run_steps(2);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    assert!(h.state().redact.report.is_some(), "report shown");
    let saved = Document::from_bytes(h.state().doc.as_ref().unwrap().bytes().to_vec()).unwrap();
    let text = saved.page_text(0).text();
    assert!(
        !text.contains("quick") && text.contains("fox jumps"),
        "{text}"
    );
}

fn with_fields() -> Document {
    use pdf_engine::{FieldKind, NewField};
    let mut d = sample(1);
    let f = |kind, name: &str, rect| NewField {
        kind,
        name: name.into(),
        rect,
        options: vec![],
    };
    d.add_field(
        0,
        &f(
            FieldKind::Text,
            "first",
            Rect::new(72.0, 300.0, 300.0, 322.0),
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::Text,
            "second",
            Rect::new(72.0, 340.0, 300.0, 362.0),
        ),
    )
    .unwrap();
    d.add_field(
        0,
        &f(
            FieldKind::CheckBox,
            "agree",
            Rect::new(72.0, 380.0, 86.0, 394.0),
        ),
    )
    .unwrap();
    Document::from_bytes(d.bytes().to_vec()).unwrap()
}

/// Forms: click a text field, type, Tab to the next one; click the check box.
#[test]
fn fills_fields_with_tab() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(with_fields(), None, "form.pdf".into());
        a.toolset = ToolSet::Forms;
        a.tool = Tool::Fill;
    });
    let pt = page_point(&h, 150.0, 310.0);
    click_at(&mut h, pt);
    assert!(
        matches!(h.state().draft.as_ref().map(|d| &d.target), Some(crate::app::DraftTarget::Field(n)) if n == "first")
    );
    h.state_mut().draft.as_mut().unwrap().text = "Salma".into();
    h.run_steps(1);
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
    assert!(
        matches!(h.state().draft.as_ref().map(|d| &d.target), Some(crate::app::DraftTarget::Field(n)) if n == "second"),
        "Tab opens the next field"
    );
    h.state_mut().draft.as_mut().unwrap().text = "سلمى".into();
    h.run_steps(1);
    h.get_by_label("Apply").click();
    h.run_steps(2);
    let pt = page_point(&h, 79.0, 387.0);
    click_at(&mut h, pt);
    let fields = Document::from_bytes(h.state().doc.as_ref().unwrap().bytes().to_vec())
        .unwrap()
        .fields();
    let v = |n: &str| fields.iter().find(|f| f.name == n).unwrap().value.clone();
    assert_eq!(
        (v("first"), v("second"), v("agree")),
        ("Salma".into(), "سلمى".into(), "Yes".into())
    );
}

/// Prepare form: drag out a text field, then move it.
#[test]
fn prepares_a_field() {
    let mut h = harness(Vec2::new(1280.0, 800.0), |a| {
        a.load_document(sample(1), None, "form.pdf".into());
        a.toolset = ToolSet::Forms;
        a.tool = Tool::Field(pdf_engine::FieldKind::Text);
    });
    let (pa, pb) = (page_point(&h, 100.0, 300.0), page_point(&h, 260.0, 322.0));
    drag(&mut h, pa, pb);
    let fields = h.state().doc.as_ref().unwrap().fields();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "Text1");
    h.state_mut().tool = Tool::Select;
    let (pa, pb) = (page_point(&h, 180.0, 310.0), page_point(&h, 180.0, 410.0));
    drag(&mut h, pa, pb);
    let r = h.state().doc.as_ref().unwrap().fields()[0].widgets[0].rect;
    assert!((r.y0 - 400.0).abs() < 2.0, "{r:?}");
}

/// Where egui puts the first word of an Arabic phrase: true when it is on the right
/// (correct for right-to-left reading).
#[test]
fn arabic_phrase_reads_right_to_left() {
    // "ب" (one short letter) then a long word: in RTL reading the short word is rightmost.
    // The same holds with a Latin word: "ب PDFPDFPDF".
    for phrase in ["ب ممممممممم", "ب PDFPDFPDF"] {
        let (left, right) = word_boxes(phrase);
        assert!(
            right.1 - right.0 < left.1 - left.0,
            "{phrase}: short first word must be on the right: {left:?} {right:?}"
        );
    }
}

fn word_boxes(phrase: &'static str) -> ((usize, usize), (usize, usize)) {
    let mut h = Harness::builder()
        .with_size(Vec2::new(400.0, 60.0))
        .wgpu()
        .build_ui(move |ui| {
            theme::install_fonts(ui.ctx());
            ui.painter()
                .rect_filled(ui.max_rect().expand(20.0), 0.0, egui::Color32::WHITE);
            ui.label(
                egui::RichText::new(crate::i18n::shape(phrase))
                    .size(30.0)
                    .color(egui::Color32::BLACK),
            );
        });
    h.run_steps(3);
    let img = h.render().unwrap();
    // Ink columns.
    let cols: Vec<bool> = (0..img.width())
        .map(|x| (0..img.height()).any(|y| img.get_pixel(x, y)[0] < 100))
        .collect();
    let mut runs = Vec::new();
    let mut x = 0;
    while x < cols.len() {
        if cols[x] {
            let s = x;
            while x < cols.len() && cols[x] {
                x += 1;
            }
            runs.push((s, x));
        }
        x += 1;
    }
    // Merge runs closer than 6 px (letters of one word).
    let mut words: Vec<(usize, usize)> = Vec::new();
    for r in runs {
        match words.last_mut() {
            Some(w) if r.0 <= w.1 + 6 => w.1 = r.1,
            _ => words.push(r),
        }
    }
    assert_eq!(words.len(), 2, "{phrase}: {words:?}");
    (words[0], words[1])
}

/// The splash card shows first; a file named on the command line opens once it is gone.
#[test]
fn splash_then_opens_pending_file() {
    let path = std::env::temp_dir().join(format!("waraq-splash-{}.pdf", std::process::id()));
    std::fs::write(&path, pdf_engine::sample::sample_pdf(2)).unwrap();
    let p2 = path.clone();
    let mut h = Harness::builder()
        .with_size(Vec2::new(440.0, 340.0))
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_eframe(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::install(&cc.egui_ctx);
            let mut app = App::with_context(&cc.egui_ctx, false);
            app.splash = Some(0.0);
            app.pending_open = Some(p2);
            app
        });
    // Restart the card's clock now that the harness is built (each harness frame is ~0.25 s).
    h.state_mut().splash = Some(0.0);
    h.state_mut().pending_open = Some(path.clone());
    h.run_steps(2);
    assert!(h.state().splash.is_some(), "the splash shows first");
    assert!(
        h.state().doc.is_none() && h.query_by_label("File").is_none(),
        "the window isn't drawn behind it"
    );
    assert!(h.query_by_label("Waraq").is_some());
    h.run_steps(8); // past the splash's minimum time
    assert!(h.state().splash.is_none());
    assert_eq!(
        h.state().doc.as_ref().map(|d| d.page_count()),
        Some(2),
        "the pending file opened"
    );
    assert!(h.query_by_label("File").is_some());
    let _ = std::fs::remove_file(&path);
}

#[test]
#[ignore]
fn splash_snapshots() {
    let Ok(dir) = std::env::var("PDF_SHOTS") else {
        return;
    };
    for (name, theme, lang) in [
        ("26_splash_light", ThemeChoice::Light, crate::i18n::Lang::En),
        ("27_splash_dark", ThemeChoice::Dark, crate::i18n::Lang::En),
        ("28_splash_ar", ThemeChoice::Light, crate::i18n::Lang::Ar),
    ] {
        let mut h = Harness::builder()
            .with_size(Vec2::new(440.0, 340.0))
            .with_pixels_per_point(1.0)
            .wgpu()
            .build_eframe(move |cc| {
                theme::install_fonts(&cc.egui_ctx);
                theme::install(&cc.egui_ctx);
                let mut app = App::with_context(&cc.egui_ctx, false);
                app.theme = theme;
                app.settings.lang = lang;
                app.splash = Some(0.0);
                app.pending_open = Some("Contract.pdf".into());
                app
            });
        h.state_mut().splash = Some(0.0);
        h.state_mut().pending_open = Some("Contract.pdf".into());
        h.run_steps(3);
        assert!(h.state().splash.is_some());
        h.render()
            .unwrap()
            .save(std::path::Path::new(&dir).join(format!("{name}.png")))
            .unwrap();
    }
}

fn lang_harness(lang: crate::i18n::Lang, theme: ThemeChoice) -> Harness<'static, App> {
    harness(Vec2::new(1280.0, 800.0), move |a| {
        a.settings.lang = lang;
        a.theme = theme;
        a.load_document(sample(2), None, "Report.pdf".into());
    })
}

/// In Arabic the menu bar and the About dialog are mirrored; in English they are not.
#[test]
fn chrome_mirrors_for_arabic() {
    for (lang, rtl) in [
        (crate::i18n::Lang::En, false),
        (crate::i18n::Lang::Ar, true),
    ] {
        let mut h = lang_harness(lang, ThemeChoice::Light);
        crate::i18n::set(lang);
        let file = h.get_by_label(&crate::i18n::tr("File")).rect();
        let help = h.get_by_label(&crate::i18n::tr("Help")).rect();
        let logo = h.get_by_label("Waraq").rect();
        assert_eq!(
            file.center().x > help.center().x,
            rtl,
            "{lang:?}: File vs Help"
        );
        assert_eq!(
            logo.center().x > file.center().x,
            rtl,
            "{lang:?}: logo at the reading start"
        );
        // The File menu opens aligned to its button's reading-start edge.
        h.get_by_label(&crate::i18n::tr("File")).click();
        h.run_steps(3);
        crate::i18n::set(lang);
        let open = h.get_by_label(&crate::i18n::tr("Open…")).rect();
        if rtl {
            assert!(
                (open.right() - file.right()).abs() < 12.0,
                "menu hugs the button's right edge: {open:?} {file:?}"
            );
        } else {
            assert!(
                (open.left() - file.left()).abs() < 12.0,
                "menu hugs the button's left edge: {open:?} {file:?}"
            );
        }
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        h.state_mut().about_open = true;
        h.run_steps(3);
        crate::i18n::set(lang);
        let icon = h.get_by_label(&crate::i18n::tr("Waraq icon")).rect();
        let made = h
            .get_by_label(&crate::i18n::trf("Version {}", &[&crate::update::CURRENT]))
            .rect();
        let close = h.get_by_label(&crate::i18n::tr("Close")).rect();
        let license = h.get_by_label(&crate::i18n::tr("License")).rect();
        assert_eq!(
            icon.center().x > made.center().x,
            rtl,
            "{lang:?}: About icon side"
        );
        assert_eq!(
            close.center().x < license.center().x,
            rtl,
            "{lang:?}: close button side"
        );
    }
    crate::i18n::set(crate::i18n::Lang::En);
}

/// Every menu open and every dialog, in English, French and Arabic:
/// `PDF_SHOTS=dir cargo test -p pdf-app chrome_snapshots -- --ignored`
#[test]
#[ignore]
fn chrome_snapshots() {
    let Ok(dir) = std::env::var("PDF_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let save = |h: &mut Harness<'static, App>, name: String| {
        h.render()
            .unwrap()
            .save(dir.join(format!("{name}.png")))
            .unwrap()
    };
    for (lang, code) in [
        (crate::i18n::Lang::En, "en"),
        (crate::i18n::Lang::Fr, "fr"),
        (crate::i18n::Lang::Ar, "ar"),
    ] {
        for (menu, key) in [
            ("File", "file"),
            ("Edit", "edit"),
            ("View", "view"),
            ("Page", "page"),
            ("Help", "help"),
        ] {
            let mut h = lang_harness(lang, ThemeChoice::Light);
            crate::i18n::set(lang);
            h.get_by_role_and_label(egui::Role::Button, &crate::i18n::tr(menu))
                .click();
            h.run_steps(3);
            save(&mut h, format!("m_{code}_{key}"));
        }
        for (theme, t) in [(ThemeChoice::Light, "light"), (ThemeChoice::Dark, "dark")] {
            let mut h = lang_harness(lang, theme);
            h.state_mut().about_open = true;
            h.run_steps(3);
            crate::i18n::set(lang);
            h.get_by_label(&crate::i18n::tr("License")).click();
            h.run_steps(8);
            save(&mut h, format!("d_{code}_about_{t}"));
        }
        let mut h = lang_harness(lang, ThemeChoice::Light);
        h.state_mut().settings_open = true;
        h.run_steps(3);
        save(&mut h, format!("d_{code}_settings"));
        let mut h = lang_harness(lang, ThemeChoice::Light);
        h.state_mut().toolset = ToolSet::Sign;
        h.state_mut().sign_dialog = Some(crate::signatures::Dialog::new(false));
        h.run_steps(3);
        save(&mut h, format!("d_{code}_signature"));
        let mut h = lang_harness(lang, ThemeChoice::Light);
        h.state_mut().toolset = ToolSet::Redact;
        h.state_mut().redact.add(
            0,
            Rect::new(72.0, 120.0, 200.0, 136.0),
            "quick brown".into(),
        );
        h.state_mut().redact.review = true;
        h.run_steps(3);
        save(&mut h, format!("d_{code}_redact_review"));
        let mut h = harness(Vec2::new(1280.0, 800.0), move |a| {
            a.settings.lang = lang;
            a.load_document(with_fields(), None, "form.pdf".into());
            a.toolset = ToolSet::Forms;
            a.tool = Tool::Select;
            let slot = a.doc.as_ref().unwrap().fields()[0].widgets[0].slot;
            a.picked = Some(crate::app::Picked::Field { page: 0, slot });
        });
        h.run_steps(2);
        save(&mut h, format!("d_{code}_field_properties"));
    }
    crate::i18n::set(crate::i18n::Lang::En);
}

/// Licence texts are shown verbatim: the Arabic About shows exactly the English bytes.
#[test]
fn licence_text_is_verbatim_in_every_language() {
    let mut texts = Vec::new();
    for lang in [crate::i18n::Lang::En, crate::i18n::Lang::Ar] {
        let mut h = lang_harness(lang, ThemeChoice::Light);
        h.state_mut().about_open = true;
        h.run_steps(3);
        crate::i18n::set(lang);
        h.get_by_label(&crate::i18n::tr("License")).click();
        h.run_steps(4);
        let node = h
            .query_by_label_contains("Permission is hereby granted")
            .expect("licence shown");
        let ak = node.accesskit_node();
        let text = ak
            .label()
            .filter(|l| !l.is_empty())
            .or_else(|| ak.value())
            .unwrap_or_default()
            .to_string();
        assert_eq!(
            text,
            crate::about::LICENSE,
            "{lang:?}: licence rendered verbatim"
        );
        // Left-aligned: the block starts at the dialog's left content edge even in Arabic.
        let left = node.rect().left();
        let close = h.get_by_label(&crate::i18n::tr("Close")).rect();
        texts.push((text, left, close));
    }
    crate::i18n::set(crate::i18n::Lang::En);
    assert_eq!(texts[0].0.as_bytes(), texts[1].0.as_bytes());
    assert!(
        (texts[0].1 - texts[1].1).abs() < 1.0,
        "same left edge in both languages: {} vs {}",
        texts[0].1,
        texts[1].1
    );
}

/// README screenshots of the app with a PDF open, in English, French and Arabic.
/// `MAKTAB_SCREENSHOTS=/dir cargo test -p pdf-app readme_screenshots -- --ignored`
#[test]
#[ignore]
fn readme_screenshots() {
    let Ok(dir) = std::env::var("MAKTAB_SCREENSHOTS") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for (code, lang) in [
        ("en", crate::i18n::Lang::En),
        ("fr", crate::i18n::Lang::Fr),
        ("ar", crate::i18n::Lang::Ar),
    ] {
        let mut h = harness(Vec2::new(1440.0, 860.0), move |a| {
            a.settings.lang = lang;
            a.load_document(annotated(), None, "Quarterly report.pdf".into());
            a.side = Some(SidePanel::Comments);
        });
        h.state_mut().splash = None;
        h.run_steps(4);
        let image = h.render().expect("render");
        image
            .save(std::path::Path::new(&dir).join(format!("waraq-{code}.png")))
            .unwrap();
    }
    crate::i18n::set(crate::i18n::Lang::En);
}
