//! Export as SVG, PNG and PDF (`export*.js`), import of text files as notes, and the system
//! clipboard.

use crate::app::BoardApp;
use crate::i18n::tr;
use saboura_engine::model::Item;
use std::io::Write;

/// The objects exported: the selection if there is one, else the whole board.
fn chosen(app: &BoardApp) -> Vec<Item> {
    if app.selection.is_empty() { app.board.items.clone() } else { app.selected_items().into_iter().cloned().collect() }
}

/// Fonts for drawing exported text: the bundled faces.
fn fontdb() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static DB: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> = std::sync::OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        // The bundled fonts are shared, not copied.
        for f in word_engine::fonts::BUNDLED { db.load_font_source(resvg::usvg::fontdb::Source::Binary(std::sync::Arc::new(f.bytes()))); }
        db.set_sans_serif_family("Noto Sans");
        std::sync::Arc::new(db)
    }).clone()
}

/// The SVG drawn into pixels, `scale` pixels per board unit (at most 8000 px across).
pub fn rasterize(svg: &str, scale: f32) -> Result<image::RgbaImage, String> {
    let mut opt = resvg::usvg::Options { fontdb: fontdb(), ..Default::default() };
    opt.font_family = "Noto Sans".into();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    let scale = scale.min(8000.0 / size.width().max(size.height()));
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let mut pix = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or("The board is too large to export.")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pix.as_mut());
    image::RgbaImage::from_raw(pix.width(), pix.height(), pix.take_demultiplied()).ok_or_else(|| "export".to_string())
}

/// A one-page PDF holding a picture (RGB, deflated) at `w` × `h` points.
pub fn pdf_from_image(img: &image::RgbaImage, w: f64, h: f64) -> Vec<u8> {
    let rgb: Vec<u8> = img.pixels().flat_map(|p| {
        // Transparent parts become white paper.
        let a = p[3] as u32;
        [0, 1, 2].map(|k| ((p[k] as u32 * a + 255 * (255 - a)) / 255) as u8)
    }).collect();
    let data = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
    let content = format!("q {w:.2} 0 0 {h:.2} 0 0 cm /Im0 Do Q");
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.2} {h:.2}] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>").into_bytes(),
        [format!("<< /Length {} >>\nstream\n", content.len()).into_bytes(), content.into_bytes(), b"\nendstream".to_vec()].concat(),
        [format!("<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", img.width(), img.height(), data.len()).into_bytes(), data, b"\nendstream".to_vec()].concat(),
    ];
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (n, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n", n + 1);
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets { let _ = write!(out, "{o:010} 00000 n \n"); }
    let _ = write!(out, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1);
    out
}

/// The board (or the selection) as bytes in a format: "svg", "png" or "pdf".
pub fn render(items: &[Item], format: &str, dark: bool) -> Result<Vec<u8>, String> {
    // Lines attached to objects are routed to them first.
    let mut b = saboura_engine::Board::new("", "blank");
    b.items = items.to_vec();
    b.update_connectors();
    let (svg, area) = saboura_engine::export::to_svg(&b.items, dark, format != "svg");
    match format {
        "svg" => Ok(svg.into_bytes()),
        "png" => {
            let img = rasterize(&svg, 2.0)?;
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
            Ok(out.into_inner())
        }
        "pdf-image" => {
            // A page the board's size in points (at most 200 inches), drawn at 2× for print.
            let s = (14400.0 / area.w.max(area.h)).min(1.0);
            let img = rasterize(&svg, (2.0 * s) as f32)?;
            Ok(pdf_from_image(&img, area.w * s, area.h * s))
        }
        "pptx" => pptx(&b.items, dark),
        "json" => { let mut board = saboura_engine::Board::new("", "blank"); board.items = b.items; Ok(board.to_json().into_bytes()) }
        "xlsx" => xlsx(&b.items),
        _ => vector_pdf(&svg),
    }
}

/// A PDF of vector shapes and real, selectable text with the fonts embedded (svg2pdf).
fn vector_pdf(svg: &str) -> Result<Vec<u8>, String> {
    use svg2pdf::usvg;
    static DB: std::sync::OnceLock<std::sync::Arc<usvg::fontdb::Database>> = std::sync::OnceLock::new();
    let db = DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        for f in word_engine::fonts::BUNDLED { db.load_font_source(usvg::fontdb::Source::Binary(std::sync::Arc::new(f.bytes()))); }
        db.set_sans_serif_family("Noto Sans");
        std::sync::Arc::new(db)
    }).clone();
    let opt = usvg::Options { fontdb: db, font_family: "Noto Sans".into(), ..Default::default() };
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default()).map_err(|e| format!("{e:?}"))
}

/// A presentation (`export-pptx.js`): a slide per frame in reading order (or one of the whole
/// board when it has none), each the frame's picture, centred on a 16:9 slide.
fn pptx(items: &[Item], dark: bool) -> Result<Vec<u8>, String> {
    let mut deck = slides_engine::Presentation::new_blank(true);
    for s in deck.slides() { deck.delete_slide(&s); }
    let layouts = deck.all_layouts();
    let layout = layouts.iter().find(|l| deck.layout_type(l) == "blank").or(layouts.last()).cloned().ok_or("No slide layout")?;
    let (sw, sh) = deck.size();
    let mut frames: Vec<&Item> = items.iter().filter(|i| i.kind == "frame").collect();
    frames.sort_by(|a, b| ((a.y / 40.0).round(), a.x).partial_cmp(&((b.y / 40.0).round(), b.x)).unwrap_or(std::cmp::Ordering::Equal));
    let groups: Vec<Vec<Item>> = if frames.is_empty() { vec![items.to_vec()] } else {
        frames.iter().map(|f| {
            let fb = saboura_engine::geometry::object_bounds(f);
            items.iter().filter(|i| i.id == f.id || saboura_engine::geometry::object_bounds(i).within(&fb)).cloned().collect()
        }).collect()
    };
    for (n, group) in groups.iter().enumerate() {
        let (svg, area) = saboura_engine::export::to_svg(group, dark, true);
        let img = rasterize(&svg, (2400.0 / area.w.max(area.h)).clamp(0.5, 3.0) as f32)?;
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        let part = deck.add_slide(&layout, n);
        let s = (sw * 0.94 / area.w).min(sh * 0.94 / area.h);
        let (w, h) = (area.w * s, area.h * s);
        let x = slides_engine::scene::Xfrm { x: (sw - w) / 2.0, y: (sh - h) / 2.0, w, h, rot: 0.0, flip_h: false, flip_v: false };
        let name = group.iter().find(|i| i.kind == "frame").map(|f| f.text.clone()).unwrap_or_else(|| "Board".into());
        deck.insert_picture(&part, png.into_inner(), "png", &x, &name);
    }
    deck.save()
}

/// A workbook (`export-sheet.js`): a sheet listing every object, and a sheet per table.
fn xlsx(items: &[Item]) -> Result<Vec<u8>, String> {
    use sheet_engine::{Input, Pos, Value, Workbook};
    let mut wb = Workbook::new();
    wb.sheets[0].name = "Board".into();
    let put = |wb: &mut Workbook, sheet: usize, row: u32, col: u32, v: Value| wb.set_cell_input(sheet, Pos { row, col }, Input::Value(v), None);
    let text = |s: &str| Value::Str(s.into());
    for (c, h) in ["Type", "Text", "Color", "X", "Y", "Width", "Height", "Frame", "Link"].iter().enumerate() { put(&mut wb, 0, 0, c as u32, text(h)); }
    let frames: Vec<&Item> = items.iter().filter(|i| i.kind == "frame").collect();
    let mut row = 1;
    for i in saboura_engine::geometry::stacked(items) {
        let ib = saboura_engine::geometry::object_bounds(i);
        let frame = frames.iter().find(|f| f.id != i.id && ib.within(&saboura_engine::geometry::object_bounds(f))).map(|f| f.text.clone()).unwrap_or_default();
        let kind = match i.kind.as_str() { "rect" => i.shape.clone().unwrap_or_else(|| "rectangle".into()), "arrow" => "connector".into(), k => k.to_string() };
        let body = if let Some(cells) = i.cells() { cells.iter().map(|r| r.join("\t")).collect::<Vec<_>>().join("\n") } else { i.text.clone() };
        let vals = [text(&kind), text(&body), text(i.color.as_deref().unwrap_or("")), Value::Num(i.x.round()), Value::Num(i.y.round()), Value::Num(i.w.round()), Value::Num(i.h.round()), text(&frame), text(i.str("link").unwrap_or(""))];
        for (c, v) in vals.into_iter().enumerate() { put(&mut wb, 0, row, c as u32, v); }
        row += 1;
    }
    for (n, t) in items.iter().filter(|i| i.kind == "table").enumerate() {
        let si = wb.add_sheet(wb.sheets.len(), Some(format!("Table {}", n + 1)));
        for (r, cells) in t.cells().unwrap_or_default().iter().enumerate() {
            for (c, v) in cells.iter().enumerate() {
                let value = v.trim().parse::<f64>().map(Value::Num).unwrap_or_else(|_| text(v));
                put(&mut wb, si, r as u32, c as u32, value);
            }
        }
    }
    sheet_engine::xlsx::write(&wb)
}

/// Opens a Miro board saved as JSON from Miro's REST API (items, connectors, mind-map nodes).
pub fn import_miro(app: &mut BoardApp) {
    let Some(path) = rfd::FileDialog::new().add_filter("Miro (JSON)", &["json"]).pick_file() else { return };
    let parsed = std::fs::read_to_string(&path).map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string()))
        .and_then(|v| saboura_engine::miro::convert(&v));
    match parsed {
        Ok((board, report)) => {
            app.new_board("blank");
            app.board = board;
            app.dirty = true;
            app.fit_pending = true;
            let mut msg = crate::i18n::trf("Imported {} objects from Miro.", &[&report.converted]);
            if !report.skipped.is_empty() { msg.push_str(&format!(" {}", crate::i18n::trf("{} were left out.", &[&report.skipped.len()]))); }
            app.notify(&msg);
        }
        Err(e) => app.notify(&format!("{}: {}", tr("Couldn't import"), tr(&e))),
    }
}

pub fn export(app: &mut BoardApp, format: &str) {
    let items = chosen(app);
    if items.is_empty() { app.notify(&tr("The board is empty.")); return; }
    let name: String = app.board.title.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let ext = if format == "pdf-image" { "pdf" } else { format };
    let Some(path) = rfd::FileDialog::new().add_filter(ext.to_uppercase(), &[ext]).set_file_name(format!("{name}.{ext}")).save_file() else { return };
    match render(&items, format, false).and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string())) {
        Ok(()) => app.notify(&tr("Exported")),
        Err(e) => app.notify(&format!("{}: {e}", tr("Couldn't export"))),
    }
}

/// Text files become a grid of notes: one note per paragraph (or per line of a CSV).
pub fn import_text(app: &mut BoardApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("Text"), &["txt", "md", "csv"]).pick_file() else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { app.notify(&tr("This file can't be read.")); return };
    let csv = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    let parts: Vec<String> = if csv { text.lines().map(|l| l.split(',').map(|c| c.trim().trim_matches('"')).collect::<Vec<_>>().join(" · ")).filter(|l| !l.trim().is_empty()).collect() }
        else { text.split("\n\n").map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect() };
    if parts.is_empty() { return; }
    let c = app.view_center();
    let cols = (parts.len() as f64).sqrt().ceil() as usize;
    app.checkpoint();
    let mut ids = Vec::new();
    for (n, t) in parts.iter().take(500).enumerate() {
        let (x, y) = (c.0 + (n % cols) as f64 * 220.0 - cols as f64 * 110.0, c.1 + (n / cols) as f64 * 220.0 - 110.0);
        let mut i = Item::new("note", x, y);
        i.w = 200.0; i.h = 200.0;
        i.text = t.chars().take(20000).collect();
        i.color = Some(saboura_engine::model::COLORS[n % 5].into());
        ids.push(app.add_item(i));
    }
    app.selection = ids;
}

pub fn clipboard_text() -> Option<String> { arboard::Clipboard::new().ok()?.get_text().ok() }

/// A picture on the system clipboard, as PNG bytes.
pub fn clipboard_image() -> Option<Vec<u8>> {
    let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
    let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())?;
    let mut out = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// A small preview of the board for `.sab` files (at most 512 px across).
pub fn thumbnail(items: &[Item]) -> Option<Vec<u8>> {
    if items.is_empty() { return None; }
    let (svg, area) = saboura_engine::export::to_svg(items, false, true);
    let img = rasterize(&svg, (512.0 / area.w.max(area.h)).min(1.0) as f32).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}
