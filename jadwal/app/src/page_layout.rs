//! The Page Layout tab (ui/page-layout.js, pagebreaks.js): margins, orientation, paper size,
//! print area, page breaks, print titles, scale to fit and sheet options; and the Page Setup
//! dialog (Page, Margins, Header/Footer and Sheet tabs).

use crate::app::SheetApp;
use crate::i18n::tr;
use crate::print::edit_setup;
use egui::{vec2, Ui};
use qalam_kit::widgets::*;
use sheet_engine::addr::{parse_range, MAX_COLS, MAX_ROWS};
use sheet_engine::print::{parse_title_span, title_span_text, Margins, PageSetup, PAPERS};

pub fn margins_name(m: &Margins) -> &'static str {
    if *m == Margins::NORMAL { "Normal Margins" } else if *m == Margins::WIDE { "Wide Margins" } else if *m == Margins::NARROW { "Narrow Margins" } else { "Custom Margins" }
}

pub const SCALINGS: [&str; 5] = ["No Scaling", "Fit Sheet on One Page", "Fit All Columns on One Page", "Fit All Rows on One Page", "Custom Scaling"];

pub fn scaling_name(p: &PageSetup) -> &'static str {
    match (p.fit, p.fit_width, p.fit_height) {
        (false, ..) if p.scale == 100 => SCALINGS[0],
        (true, 1, 1) => SCALINGS[1],
        (true, 1, 0) => SCALINGS[2],
        (true, 0, 1) => SCALINGS[3],
        _ => SCALINGS[4],
    }
}

pub fn apply_scaling(p: &mut PageSetup, i: usize) {
    match i {
        0 => { p.fit = false; p.scale = 100; }
        1 => { p.fit = true; p.fit_width = 1; p.fit_height = 1; }
        2 => { p.fit = true; p.fit_width = 1; p.fit_height = 0; }
        3 => { p.fit = true; p.fit_width = 0; p.fit_height = 1; }
        _ => {}
    }
}

/// The Page Layout tab's groups.
pub fn tab(app: &mut SheetApp, ui: &mut Ui) {
    let s = app.sheet();
    let p = app.book.sheets[s].page.clone();
    crate::themes_ui::group(app, ui);
    let launch = group_l(ui, "Page Setup", "Page Setup", |ui| {
        let r = large_menu(ui, "margins", "Margins", "Margins");
        crate::theme::menu(&r, |ui| {
            for (n, m) in [("Normal", Margins::NORMAL), ("Wide", Margins::WIDE), ("Narrow", Margins::NARROW)] {
                let label = format!("{}  ({}: {}\"  {}: {}\")", tr(n), tr("Top"), m.top, tr("Left"), m.left);
                if ui.selectable_label(p.margins == m, label).clicked() { edit_setup(app, "Margins", |p| p.margins = m); ui.close(); }
            }
            ui.separator();
            if ui.button(tr("Custom Margins…")).clicked() { open_setup(app, 1); ui.close(); }
        });
        let r = large_menu(ui, "orientation", "Orientation", "Orientation");
        crate::theme::menu(&r, |ui| {
            if menu_item(ui, "portrait", "Portrait") { edit_setup(app, "Orientation", |p| p.landscape = false); ui.close(); }
            if menu_item(ui, "landscape", "Landscape") { edit_setup(app, "Orientation", |p| p.landscape = true); ui.close(); }
        });
        let r = large_menu(ui, "paperSize", "Size", "Paper size");
        crate::theme::menu(&r, |ui| {
            for (code, name, w, h) in PAPERS {
                if ui.selectable_label(p.paper == *code, format!("{}  {:.2}\" × {:.2}\"", tr(name), w, h)).clicked() { let c = *code; edit_setup(app, "Size", |p| p.paper = c); ui.close(); }
            }
            ui.separator();
            if ui.button(tr("More Paper Sizes…")).clicked() { open_setup(app, 0); ui.close(); }
        });
        let r = large_menu(ui, "printArea", "Print Area", "Print Area");
        crate::theme::menu(&r, |ui| {
            if ui.button(tr("Set Print Area")).clicked() {
                app.checkpoint("Set Print Area");
                let sel = app.sel.ranges();
                app.book.set_print_area(s, Some(&sel));
                app.dirty = true; app.printing.show_breaks = true; ui.close();
            }
            if let Some(mut area) = app.book.print_area(s) {
                if ui.button(tr("Add to Print Area")).clicked() {
                    app.checkpoint("Add to Print Area");
                    area.extend(app.sel.ranges());
                    app.book.set_print_area(s, Some(&area));
                    app.dirty = true; ui.close();
                }
                if ui.button(tr("Clear Print Area")).clicked() { app.checkpoint("Clear Print Area"); app.book.set_print_area(s, None); app.dirty = true; ui.close(); }
            }
        });
        let r = large_menu(ui, "breaks", "Breaks", "Page breaks");
        crate::theme::menu(&r, |ui| {
            let a = app.sel.active;
            let r = app.sel.range();
            let whole_rows = r.c0 == 0 && r.c1 == MAX_COLS - 1;
            let whole_cols = r.r0 == 0 && r.r1 == MAX_ROWS - 1;
            if ui.button(tr("Insert Page Break")).clicked() {
                edit_setup(app, "Insert Page Break", |p| {
                    if !whole_cols && a.row > 0 { p.row_breaks.insert(a.row); }
                    if !whole_rows && a.col > 0 { p.col_breaks.insert(a.col); }
                });
                app.printing.show_breaks = true; ui.close();
            }
            let has = p.row_breaks.contains(&a.row) || p.col_breaks.contains(&a.col);
            if ui.add_enabled(has, egui::Button::new(tr("Remove Page Break"))).clicked() {
                edit_setup(app, "Remove Page Break", |p| { p.row_breaks.remove(&a.row); p.col_breaks.remove(&a.col); });
                ui.close();
            }
            if ui.add_enabled(!p.row_breaks.is_empty() || !p.col_breaks.is_empty(), egui::Button::new(tr("Reset All Page Breaks"))).clicked() {
                edit_setup(app, "Reset All Page Breaks", |p| { p.row_breaks.clear(); p.col_breaks.clear(); });
                ui.close();
            }
            ui.separator();
            let mut show = app.printing.show_breaks;
            if crate::theme::check(ui, &mut show, tr("Show Page Breaks")).changed() { app.printing.show_breaks = show; }
        });
        if sheet_engine::sheet_background::get(&app.book, s).is_some() {
            if large(ui, "deletePicture", "Delete Background", false, true, "Remove the sheet's background picture").clicked() { app.checkpoint("Delete Background"); sheet_engine::sheet_background::remove(&mut app.book, s); app.dirty = true; }
        } else if large(ui, "backgroundStyles", "Background", false, true, "Show a picture tiled behind the sheet's cells").clicked() { pick_background(app); }
        if large(ui, "printTitles", "Print Titles", false, true, "Rows and columns to repeat on every page").clicked() { open_setup(app, 3); }
    });
    if launch { open_setup(app, 0); }
    let launch = group_l(ui, "Scale to Fit", "Page Setup", |ui| {
        rows(ui, |ui| {
            let pages = |n: u32| if n == 0 { tr("Automatic") } else if n == 1 { tr("1 page") } else { crate::i18n::trf("{} pages", &[&n]) };
            for (label, height) in [("Width:", false), ("Height:", true)] {
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(50.0, 20.0), egui::Label::new(tr(label)));
                    let cur = if !p.fit { 0 } else if height { p.fit_height } else { p.fit_width };
                    crate::theme::Combo::from_id_salt(("fit", height)).width(96.0).selected_text(pages(cur)).show_ui(ui, |ui| {
                        for n in 0..=9u32 {
                            if ui.selectable_label(cur == n, pages(n)).clicked() {
                                edit_setup(app, "Scale to Fit", |p| {
                                    if !p.fit { p.fit_width = 0; p.fit_height = 0; }
                                    if height { p.fit_height = n; } else { p.fit_width = n; }
                                    p.fit = p.fit_width > 0 || p.fit_height > 0;
                                });
                            }
                        }
                    });
                });
            }
            ui.horizontal(|ui| {
                ui.add_sized(vec2(50.0, 20.0), egui::Label::new(tr("Scale:")));
                let mut sc = p.scale;
                if ui.add_enabled(!p.fit, egui::DragValue::new(&mut sc).range(10..=400).suffix("%")).changed() { edit_setup(app, "Scale", |p| p.scale = sc); }
            });
        });
    });
    if launch { open_setup(app, 0); }
    let launch = group_l(ui, "Sheet Options", "Page Setup", |ui| {
        rows(ui, |ui| {
            ui.set_min_width(64.0);
            ui.label(tr("Gridlines"));
            let mut g = app.book.sheets[s].gridlines;
            if crate::theme::check(ui, &mut g, tr("View")).changed() { app.checkpoint("Gridlines"); app.book.sheets[s].gridlines = g; app.dirty = true; }
            let mut pg = p.gridlines;
            if crate::theme::check(ui, &mut pg, tr("Print")).changed() { edit_setup(app, "Print Gridlines", |p| p.gridlines = pg); }
        });
        rows(ui, |ui| {
            ui.set_min_width(64.0);
            ui.label(tr("Headings"));
            let mut h = app.book.sheets[s].headers;
            if crate::theme::check(ui, &mut h, tr("View")).changed() { app.checkpoint("Headings"); app.book.sheets[s].headers = h; app.dirty = true; }
            let mut ph = p.headings;
            if crate::theme::check(ui, &mut ph, tr("Print")).changed() { edit_setup(app, "Print Headings", |p| p.headings = ph); }
        });
        rows(ui, |ui| {
            let mut rtl = app.book.sheets[s].right_to_left;
            if crate::theme::check(ui, &mut rtl, tr("Sheet Right-to-Left")).changed() { app.checkpoint("Sheet Right-to-Left"); app.book.sheets[s].right_to_left = rtl; app.dirty = true; }
        });
    });
    if launch { open_setup(app, 3); }
    crate::arrange::ribbon_group(app, ui);
}

/// Page Layout ▸ Background: a picture file tiled behind the cells.
fn pick_background(app: &mut SheetApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() else { return };
    let Ok(data) = std::fs::read(&path) else { app.error = Some(tr("The picture couldn't be read.")); return };
    set_background(app, data);
}

pub fn set_background(app: &mut SheetApp, data: Vec<u8>) {
    if image::load_from_memory(&data).is_err() { app.error = Some(tr("That file isn't a picture Jadwal can show (PNG, JPEG).")); return; }
    let ext = match image::guess_format(&data) { Ok(image::ImageFormat::Jpeg) => "jpeg", Ok(image::ImageFormat::Gif) => "gif", Ok(image::ImageFormat::Bmp) => "bmp", _ => "png" };
    let s = app.sheet();
    app.checkpoint("Background");
    sheet_engine::sheet_background::set(&mut app.book, s, data, ext);
    app.dirty = true;
}

thread_local! { static BG: std::cell::RefCell<Option<(usize, usize, Option<egui::TextureHandle>)>> = const { std::cell::RefCell::new(None) }; }

/// The sheet's background picture, tiled from A1 behind the cells of one pane.
pub fn paint_background(app: &SheetApp, p: &egui::Painter, lay: &crate::grid::Layout) {
    let Some(data) = sheet_engine::sheet_background::get(&app.book, app.sheet()) else { return };
    let key = (data.as_ptr() as usize, data.len());
    let tex = BG.with(|b| {
        let mut b = b.borrow_mut();
        if b.as_ref().map(|x| (x.0, x.1)) != Some(key) {
            let t = image::load_from_memory(data).ok().map(|i| i.to_rgba8()).map(|img| {
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                app.ctx.load_texture("sheet-background", ci, egui::TextureOptions::LINEAR)
            });
            *b = Some((key.0, key.1, t));
        }
        b.as_ref().and_then(|x| x.2.clone())
    });
    let Some(t) = tex else { return };
    let size = t.size_vec2() * lay.z;
    if size.x < 1.0 || size.y < 1.0 { return; }
    let clip = p.clip_rect();
    // Tiles start at the sheet's origin, wherever it has scrolled to.
    let o = crate::objects::to_screen(lay, 0.0, 0.0);
    let origin = egui::pos2(o.x - lay.cols.pos(lay.cols.at(0.0)) * 0.0, o.y);
    let (x0, y0) = (origin.x + ((clip.min.x - origin.x) / size.x).floor() * size.x, origin.y + ((clip.min.y - origin.y) / size.y).floor() * size.y);
    let mut y = y0;
    while y < clip.max.y {
        let mut x = x0;
        while x < clip.max.x { p.image(t.id(), egui::Rect::from_min_size(egui::pos2(x, y), size), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE); x += size.x; }
        y += size.y;
    }
}

// ---- the Page Setup dialog ----

pub struct SetupDialog {
    pub tab: u8,
    pub page: PageSetup,
    pub area: String,
    pub rows: String,
    pub cols: String,
    pub error: Option<String>,
}

pub fn open_setup(app: &mut SheetApp, tab: u8) {
    let s = app.sheet();
    let area = app.book.print_area(s).map(|a| a.iter().map(|r| r.a1()).collect::<Vec<_>>().join(",")).unwrap_or_default();
    let (r, c) = app.book.print_titles(s);
    app.printing.setup = Some(SetupDialog { tab, page: app.book.sheets[s].page.clone(), area, rows: title_span_text(r, true), cols: title_span_text(c, false), error: None });
}

fn inches(ui: &mut Ui, label: &str, v: &mut f32) {
    ui.label(tr(label));
    ui.add(egui::DragValue::new(v).range(0.0..=10.0).speed(0.01).fixed_decimals(2).suffix("\""));
}

pub fn setup_dialog(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = app.printing.setup.take() else { return };
    let mut open = true;
    let mut done: Option<bool> = None;
    let mut cancel = false;
    crate::theme::dialog(ctx, "Page Setup").open(&mut open).default_width(480.0).show(ctx, |ui| {
        ui.horizontal(|ui| for (i, t) in ["Page", "Margins", "Header/Footer", "Sheet"].iter().enumerate() { ui.selectable_value(&mut d.tab, i as u8, tr(t)); });
        ui.separator();
        let p = &mut d.page;
        match d.tab {
            0 => {
                ui.label(egui::RichText::new(tr("Orientation")).strong());
                ui.horizontal(|ui| { crate::theme::radio_value(ui, &mut p.landscape, false, tr("Portrait")); crate::theme::radio_value(ui, &mut p.landscape, true, tr("Landscape")); });
                ui.add_space(6.0);
                ui.label(egui::RichText::new(tr("Scaling")).strong());
                ui.horizontal(|ui| {
                    crate::theme::radio_value(ui, &mut p.fit, false, tr("Adjust to:"));
                    ui.add_enabled(!p.fit, egui::DragValue::new(&mut p.scale).range(10..=400).suffix("%"));
                    ui.label(tr("normal size"));
                });
                ui.horizontal(|ui| {
                    crate::theme::radio_value(ui, &mut p.fit, true, tr("Fit to:"));
                    ui.add_enabled(p.fit, egui::DragValue::new(&mut p.fit_width).range(0..=99));
                    ui.label(tr("page(s) wide by"));
                    ui.add_enabled(p.fit, egui::DragValue::new(&mut p.fit_height).range(0..=99));
                    ui.label(tr("tall"));
                });
                ui.add_space(6.0);
                egui::Grid::new("ps-page").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
                    ui.label(tr("Paper size:"));
                    crate::theme::Combo::from_id_salt("ps-paper").width(220.0).selected_text(tr(p.paper_name())).show_ui(ui, |ui| for (code, name, _, _) in PAPERS { ui.selectable_value(&mut p.paper, *code, tr(name)); });
                    ui.end_row();
                    ui.label(tr("First page number:"));
                    ui.horizontal(|ui| {
                        let mut auto = p.first_page.is_none();
                        if crate::theme::check(ui, &mut auto, tr("Auto")).changed() { p.first_page = if auto { None } else { Some(1) }; }
                        if let Some(n) = &mut p.first_page { ui.add(egui::DragValue::new(n).range(1..=99999)); }
                    });
                    ui.end_row();
                });
            }
            1 => {
                egui::Grid::new("ps-margins").num_columns(4).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
                    inches(ui, "Top:", &mut p.margins.top); inches(ui, "Header:", &mut p.margins.header); ui.end_row();
                    inches(ui, "Left:", &mut p.margins.left); inches(ui, "Right:", &mut p.margins.right); ui.end_row();
                    inches(ui, "Bottom:", &mut p.margins.bottom); inches(ui, "Footer:", &mut p.margins.footer); ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(egui::RichText::new(tr("Center on page")).strong());
                crate::theme::check(ui, &mut p.center_h, tr("Horizontally"));
                crate::theme::check(ui, &mut p.center_v, tr("Vertically"));
            }
            2 => {
                let hf = &mut p.header_footer;
                ui.label(tr("Codes: &L &C &R sections; &P page, &N pages, &D date, &T time, &F file, &A sheet; &B bold, &I italic."));
                ui.add_space(4.0);
                let presets = ["", "&CPage &P", "&CPage &P of &N", "&C&A", "&C&F", "&L&A&RPage &P", "&L&F&C&A&RPage &P"];
                egui::Grid::new("ps-hf").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
                    for (label, v) in [("Header:", &mut hf.odd_header), ("Footer:", &mut hf.odd_footer)] {
                        ui.label(tr(label));
                        ui.vertical(|ui| {
                            crate::theme::Combo::from_id_salt(("ps-hf", label)).width(300.0).selected_text(if v.is_empty() { tr("(none)") } else { v.clone() }).show_ui(ui, |ui| {
                                for pr in presets { if ui.selectable_label(*v == pr, if pr.is_empty() { tr("(none)") } else { pr.to_string() }).clicked() { *v = pr.to_string(); } }
                            });
                            ui.add(egui::TextEdit::singleline(v).desired_width(300.0));
                        });
                        ui.end_row();
                    }
                });
                crate::theme::check(ui, &mut hf.different_odd_even, tr("Different odd and even pages"));
                if hf.different_odd_even {
                    egui::Grid::new("ps-hf-even").num_columns(2).show(ui, |ui| {
                        ui.label(tr("Even header:")); ui.add(egui::TextEdit::singleline(&mut hf.even_header).desired_width(300.0)); ui.end_row();
                        ui.label(tr("Even footer:")); ui.add(egui::TextEdit::singleline(&mut hf.even_footer).desired_width(300.0)); ui.end_row();
                    });
                }
                crate::theme::check(ui, &mut hf.different_first, tr("Different first page"));
                if hf.different_first {
                    egui::Grid::new("ps-hf-first").num_columns(2).show(ui, |ui| {
                        ui.label(tr("First header:")); ui.add(egui::TextEdit::singleline(&mut hf.first_header).desired_width(300.0)); ui.end_row();
                        ui.label(tr("First footer:")); ui.add(egui::TextEdit::singleline(&mut hf.first_footer).desired_width(300.0)); ui.end_row();
                    });
                }
                crate::theme::check(ui, &mut hf.scale_with_doc, tr("Scale with document"));
                crate::theme::check(ui, &mut hf.align_with_margins, tr("Align with page margins"));
            }
            _ => {
                egui::Grid::new("ps-sheet").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
                    ui.label(tr("Print area:")); crate::refedit::field(ui, &mut d.area, "page_layout.rs:329", 240.0); ui.end_row();
                    ui.label(tr("Rows to repeat at top:")); crate::refedit::field(ui, &mut d.rows, "page_layout.rs:330", 240.0); ui.end_row();
                    ui.label(tr("Columns to repeat at left:")); crate::refedit::field(ui, &mut d.cols, "page_layout.rs:331", 240.0); ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(egui::RichText::new(tr("Print")).strong());
                crate::theme::check(ui, &mut p.gridlines, tr("Gridlines"));
                crate::theme::check(ui, &mut p.headings, tr("Row and column headings"));
                crate::theme::check(ui, &mut p.black_and_white, tr("Black and white"));
                ui.add_space(6.0);
                ui.label(egui::RichText::new(tr("Page order")).strong());
                crate::theme::radio_value(ui, &mut p.over_then_down, false, tr("Down, then over"));
                crate::theme::radio_value(ui, &mut p.over_then_down, true, tr("Over, then down"));
            }
        }
        if let Some(e) = &d.error { ui.colored_label(egui::Color32::from_rgb(0xC0, 0x30, 0x30), e); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { done = Some(true); }
            if crate::theme::secondary(ui, "Print Preview").clicked() { done = Some(false); }
            if crate::theme::secondary(ui, "Cancel").clicked() { cancel = true; }
        });
    });
    if let Some(preview) = done.map(|ok| !ok) {
        // Validate the references before changing anything.
        let area: Option<Vec<_>> = d.area.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| parse_range(&s.replace('$', "").rsplit('!').next().unwrap_or("").to_string())).collect();
        let rows = parse_title_span(d.rows.rsplit('!').next().unwrap_or(""), true);
        let cols = parse_title_span(d.cols.rsplit('!').next().unwrap_or(""), false);
        if area.is_none() { d.error = Some(tr("The print area isn't a valid reference.")); }
        else if rows.is_none() && !d.rows.trim().is_empty() { d.error = Some(tr("Rows to repeat isn't a valid reference (for example $1:$2).")); }
        else if cols.is_none() && !d.cols.trim().is_empty() { d.error = Some(tr("Columns to repeat isn't a valid reference (for example $A:$B).")); }
        else {
            let s = app.sheet();
            let area = area.unwrap_or_default();
            app.checkpoint("Page Setup");
            app.book.sheets[s].page = d.page.clone();
            app.book.set_print_area(s, (!area.is_empty()).then_some(&area[..]));
            app.book.set_print_titles(s, rows, cols);
            app.dirty = true;
            if preview { crate::print::open(app); }
            return;
        }
    }
    if open && !cancel { app.printing.setup = Some(d); }
}
