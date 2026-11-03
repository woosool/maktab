//! A small generated PDF for tests, demos and the visual tour.

use lopdf::{Dictionary, Object, Stream, dictionary};

/// Builds a Letter-size PDF with `pages` pages of Helvetica text.
/// Page *n* starts with the heading "Page n" and has a few paragraphs.
pub fn sample_pdf(pages: usize) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let bold = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font, "F2" => bold },
    });
    const LINES: [&str; 6] = [
        "The quick brown fox jumps over the lazy dog.",
        "Waraq is a native editor for reading and marking up PDFs.",
        "Highlight this sentence to try the comment tools.",
        "Pages can be rotated, reordered, duplicated and deleted.",
        "Search finds every match and outlines it on the page.",
        "Everything you add is saved as a real PDF annotation.",
    ];
    let mut kids = Vec::new();
    for n in 1..=pages {
        let mut s = format!(
            "0.85 0.2 0.15 rg 72 700 468 4 re f\n0 0 0 rg BT /F2 28 Tf 72 720 Td (Page {n}) Tj ET\n"
        );
        for (i, line) in LINES.iter().enumerate() {
            s += &format!(
                "BT /F1 13 Tf 72 {} Td ({line}) Tj ET\n",
                660 - i as i32 * 24
            );
        }
        s += &format!(
            "0.2 0.4 0.8 RG 2 w 72 {} 200 120 re S\n",
            260 + n as i32 * 4
        );
        let content = doc.add_object(Stream::new(Dictionary::new(), s.into_bytes()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => resources,
            "Contents" => content,
        });
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => kids.len() as i64, "Kids" => kids,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("sample serializes");
    out
}
