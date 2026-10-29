//! Sparklines on screen (sparklinedialog.js, sparklinecreatedialog.js, native-sparkline.js): the
//! Create Sparklines dialog, drawing sparklines and pictures in cells in the grid, the
//! contextual Sparkline tab; also Insert ▸ Pictures ▸ Picture in Cell and the Hierarchy,
//! Statistic and Waterfall chart menu.

use crate::app::{SheetApp, Tab};
use crate::i18n::tr;
use egui::{Color32, Rect, RichText, TextureHandle, Ui};
use qalam_kit::colors::c_ink2;
use qalam_kit::widgets::*;
use sheet_engine::addr::parse_range;
use sheet_engine::chart::ChartKind;
use sheet_engine::drawing::{Anchor, ObjKind, Object};
use sheet_engine::sparkline::{self as sl, Bound, Empty, Group, Kind};
use sheet_engine::{cellimage, Pos, Range};
use std::cell::RefCell;
use std::collections::HashMap;

/// What the small dialog asks for.
#[derive(Clone, Debug, PartialEq)]
enum Ask {
    /// Create (edit: replacing group `Some(i)`): data and location ranges.
    Ranges { kind: Kind, edit: Option<usize> },
    /// The date range of a date axis.
    DateAxis,
    /// A custom vertical axis bound.
    Bound { max: bool },
}

#[derive(Clone, Debug)]
struct Dlg { ask: Ask, data: String, loc: String, error: Option<String> }

thread_local! {
    static DIALOG: RefCell<Option<Dlg>> = const { RefCell::new(None) };
    static TEXTURES: RefCell<HashMap<(usize, usize), Option<(TextureHandle, [f32; 2])>>> = RefCell::new(HashMap::new());
}

fn set_dialog(d: Option<Dlg>) { DIALOG.with(|x| *x.borrow_mut() = d); }

/// Whether the Create Sparklines (or another sparkline) dialog is open.
#[cfg_attr(not(test), allow(dead_code))]
pub fn dialog_open() -> bool { DIALOG.with(|d| d.borrow().is_some()) }

/// Insert ▸ Sparklines ▸ Line, Column or Win/Loss: the Create Sparklines dialog, with the
/// selection as the data and the column (or row) beside it as the location.
pub fn open_create(app: &mut SheetApp, kind: Kind) {
    app.commit(None);
    let r = app.sel.range();
    let (data, loc) = if r.is_cell() { (String::new(), r.a1()) }
        else if r.cols() >= r.rows() { (r.a1(), Range { c0: r.c1 + 1, c1: r.c1 + 1, ..r }.a1()) }
        else { (r.a1(), Range { r0: r.r1 + 1, r1: r.r1 + 1, ..r }.a1()) };
    set_dialog(Some(Dlg { ask: Ask::Ranges { kind, edit: None }, data, loc, error: None }));
}

/// Creates sparklines directly (what the dialog's OK does).
pub fn create(app: &mut SheetApp, kind: Kind, data: &str, loc: &str, edit: Option<usize>) -> Result<(), String> {
    let si = app.sheet();
    let loc = parse_range(loc.trim().trim_start_matches('=').rsplit('!').next().unwrap_or("")).ok_or_else(|| tr("The location range isn't valid."))?;
    if sheet_engine::chart::parse_ref(data).is_none() { return Err(tr("The data range isn't valid.")); }
    let mut book = app.book.clone();
    let old = edit.and_then(|g| book.sheets[si].sparklines.get(g).cloned());
    if let Some(g) = edit { if g < book.sheets[si].sparklines.len() { book.sheets[si].sparklines.remove(g); } }
    sl::create(&mut book, si, kind, data.trim(), loc).map_err(tr)?;
    if let Some(o) = old { let n = book.sheets[si].sparklines.last_mut().unwrap(); let sp = std::mem::take(&mut n.sparklines); *n = Group { sparklines: sp, ..o }; }
    app.checkpoint(if edit.is_some() { "Edit Sparklines" } else { "Insert Sparklines" });
    let parts = std::mem::take(&mut app.book.parts);
    app.book = book;
    app.book.parts = parts;
    app.dirty = true;
    Ok(())
}

/// The sparkline dialogs (called each frame by the window).
pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = DIALOG.with(|x| x.borrow().clone()) else { return };
    let title = match d.ask { Ask::Ranges { edit: None, .. } => "Create Sparklines", Ask::Ranges { .. } => "Edit Sparklines", Ask::DateAxis => "Sparkline Date Range", Ask::Bound { max: false } => "Sparkline Vertical Axis Setting (Minimum)", Ask::Bound { max: true } => "Sparkline Vertical Axis Setting (Maximum)" };
    let mut open = true;
    let mut keep = true;
    crate::theme::dialog(ctx, tr(title)).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.set_min_width(320.0);
        match &d.ask {
            Ask::Ranges { .. } => {
                ui.label(RichText::new(tr("Choose the data that you want")).strong());
                ui.horizontal(|ui| { ui.label(tr("Data Range:")); crate::refedit::field(ui, &mut d.data, "sparkline_ui.rs:84", 200.0); });
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Choose where you want the sparklines to be placed")).strong());
                ui.horizontal(|ui| { ui.label(tr("Location Range:")); crate::refedit::field(ui, &mut d.loc, "sparkline_ui.rs:87", 200.0); });
            }
            Ask::DateAxis => { ui.label(tr("Select the range that contains the date values for the sparkline data.")); crate::refedit::field(ui, &mut d.data, "sparkline_ui.rs:89", 260.0); }
            Ask::Bound { .. } => { ui.label(tr("Enter the vertical axis value:")); ui.add(egui::TextEdit::singleline(&mut d.data).desired_width(120.0)); }
        }
        if let Some(e) = &d.error { ui.label(RichText::new(e).color(Color32::from_rgb(0xC0, 0x20, 0x20))); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let res = match d.ask.clone() {
                    Ask::Ranges { kind, edit } => create(app, kind, &d.data, &d.loc, edit),
                    Ask::DateAxis => if sheet_engine::chart::parse_ref(&d.data).is_some() { let f = d.data.trim().to_string(); edit_group(app, "Date Axis", move |g| g.date_axis = Some(f)); Ok(()) } else { Err(tr("The date range isn't valid.")) },
                    Ask::Bound { max } => match d.data.trim().parse::<f64>() { Ok(v) => { edit_group(app, "Axis", move |g| if max { g.max = Bound::Custom(v) } else { g.min = Bound::Custom(v) }); Ok(()) } Err(_) => Err(tr("Enter a number.")) },
                };
                match res { Ok(()) => keep = false, Err(e) => d.error = Some(e) }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    set_dialog((keep && open).then_some(d));
}

// ---- drawing in the grid -----------------------------------------------------------------

fn texture(ctx: &egui::Context, v: &sheet_engine::Value) -> Option<(TextureHandle, [f32; 2])> {
    let sheet_engine::Value::Str(s) = v else { return None };
    let key = (s.as_ptr() as usize, s.len());
    TEXTURES.with(|t| t.borrow_mut().entry(key).or_insert_with(|| {
        let (src, _) = cellimage::parse(v)?;
        let (data, _) = cellimage::bytes(src)?;
        let img = image::load_from_memory(&data).ok()?.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        Some((ctx.load_texture(format!("cellpic-{}-{}", key.0, key.1), ci, egui::TextureOptions::LINEAR), [img.width() as f32, img.height() as f32]))
    }).clone())
}

/// Draws the sparklines and pictures in the cells of `area` (grid.rs hook, after fills).
pub fn draw(app: &SheetApp, p: &egui::Painter, lay: &crate::grid::Layout, area: Range) {
    let si = app.sheet();
    let s = &app.book.sheets[si];
    for g in &s.sparklines {
        for sp in &g.sparklines {
            if !area.contains(sp.loc) || lay.cols.size(sp.loc.col) <= 0.0 || lay.rows.size(sp.loc.row) <= 0.0 { continue; }
            let rc = s.merge_at(sp.loc).map(|m| lay.range_rect(m)).unwrap_or_else(|| lay.cell_rect(sp.loc));
            let prims = sl::layout(&app.book, si, g, sp, [rc.min.x, rc.min.y, rc.max.x, rc.max.y], lay.z);
            crate::chart_ui::paint_prims(&p.with_clip_rect(rc.intersect(p.clip_rect())), &prims);
        }
    }
    // Pictures in cells: value cells and spilled IMAGE() results.
    let cells = s.cells.range(Pos::new(area.r0, 0)..=Pos::new(area.r1, u32::MAX)).filter(|(q, _)| q.col >= area.c0 && q.col <= area.c1).map(|(q, c)| (*q, &c.value));
    for (q, v) in cells.chain(s.spill.iter().filter(|(q, _)| area.contains(**q)).map(|(q, v)| (*q, v))) {
        let Some((src, alt)) = cellimage::parse(v) else { continue };
        if lay.cols.size(q.col) <= 0.0 || lay.rows.size(q.row) <= 0.0 { continue; }
        let rc = s.merge_at(q).map(|m| lay.range_rect(m)).unwrap_or_else(|| lay.cell_rect(q)).shrink(lay.z);
        match texture(p.ctx(), v) {
            Some((t, [w, h])) => {
                let f = cellimage::fit([rc.min.x, rc.min.y, rc.max.x, rc.max.y], w, h);
                p.image(t.id(), Rect::from_min_max(egui::pos2(f[0], f[1]), egui::pos2(f[2], f[3])), Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                // A web picture isn't fetched: a frame with its alt text (or address) instead.
                let cp = p.with_clip_rect(rc.intersect(p.clip_rect()));
                cp.rect_stroke(rc, 0.0, egui::Stroke::new(1.0, Color32::from_gray(0xB0)), egui::StrokeKind::Inside);
                cp.text(rc.center(), egui::Align2::CENTER_CENTER, if alt.is_empty() { src } else { alt }, egui::FontId::proportional(10.0 * lay.z), Color32::from_gray(0x70));
            }
        }
    }
}

// ---- Picture in Cell -------------------------------------------------------------------------

/// Insert ▸ Pictures ▸ Picture in Cell…: a picture file placed in the active cell.
pub fn picture_in_cell(app: &mut SheetApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() else { return };
    let data = match std::fs::read(&path) { Ok(d) => d, Err(e) => { app.error = Some(format!("{}\n{e}", tr("The picture couldn't be read."))); return } };
    let alt = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    place_picture(app, &data, &alt);
}

/// Places picture bytes in the active cell (what Picture in Cell does once the file is read).
pub fn place_picture(app: &mut SheetApp, data: &[u8], alt: &str) {
    if image::load_from_memory(data).is_err() { app.error = Some(tr("That file isn't a picture Jadwal can show (PNG, JPEG).")); return; }
    let ext = match image::guess_format(data) { Ok(image::ImageFormat::Jpeg) => "jpeg", Ok(image::ImageFormat::Gif) => "gif", Ok(image::ImageFormat::Bmp) => "bmp", _ => "png" };
    app.commit(None);
    app.checkpoint("Picture in Cell");
    let (si, p) = (app.sheet(), app.sel.active);
    cellimage::place(&mut app.book, si, p, data, ext, alt);
    app.changed();
}

// ---- Hierarchy, Statistic and Waterfall charts ----------------------------------------------

/// Insert ▸ Hierarchy, Statistic or Waterfall chart menu.
pub fn more_charts_menu(app: &mut SheetApp, ui: &mut Ui) {
    for (heading, kinds) in sheet_engine::chart::ex::MENU {
        menu_heading(ui, &tr(heading));
        for k in kinds.iter() { if menu_item(ui, k.icon(), k.label()) { insert_ex_chart(app, *k); ui.close(); } }
    }
}

/// Inserts a chartex chart of the selection (or the data around the active cell).
pub fn insert_ex_chart(app: &mut SheetApp, kind: ChartKind) {
    let si = app.sheet();
    let r = app.sel.range();
    let r = if r.is_cell() { app.book.current_region(si, app.sel.active) } else { r };
    if !r.iter().take(200_000).any(|p| matches!(app.book.sheets[si].value(p), sheet_engine::Value::Num(_))) { app.error = Some(tr("Select the cells with the data to chart (with some numbers) first.")); return; }
    let c = sheet_engine::chart::ex::from_range(&app.book, si, r, kind);
    let s = &app.book.sheets[si];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    let (x, y) = (cols.pos(r.c1 + 2), rows.pos(r.r0));
    let mut a = Anchor::default();
    a.set_rect([x, y, x + 480.0, y + 288.0], |x| cols.at(x), |c| cols.pos(c), |y| rows.at(y), |r| rows.pos(r));
    crate::objects::insert(app, Object::new(ObjKind::Chart(Box::new(c)), a), "Chart", "Insert Chart");
    app.tab = Tab::ChartDesign;
}

// ---- the Sparkline tab -------------------------------------------------------------------------

/// The group at the active cell.
pub fn group_at(app: &SheetApp) -> Option<usize> { sl::at(&app.book.sheets[app.sheet()], app.sel.active).map(|x| x.0) }

/// The contextual tab, while the active cell has a sparkline.
pub fn contextual(app: &SheetApp) -> Option<(Tab, &'static str)> { group_at(app).map(|_| (Tab::Sparkline, "Sparkline")) }

fn edit_group(app: &mut SheetApp, label: &'static str, f: impl FnOnce(&mut Group)) {
    let Some(g) = group_at(app) else { return };
    app.checkpoint(label);
    let si = app.sheet();
    f(&mut app.book.sheets[si].sparklines[g]);
    app.dirty = true;
}

fn to_rgb(c: word_engine::Color) -> [u8; 3] { [c.0, c.1, c.2] }

/// Groups the sparklines in the selection into the active cell's group.
pub fn group_selection(app: &mut SheetApp) {
    let Some(g) = group_at(app) else { return };
    let si = app.sheet();
    let rs = app.sel.ranges();
    app.checkpoint("Group Sparklines");
    let s = &mut app.book.sheets[si];
    let mut moved = vec![];
    for (i, x) in s.sparklines.iter_mut().enumerate() { if i == g { continue; } x.sparklines.retain(|sp| if rs.iter().any(|r| r.contains(sp.loc)) { moved.push(sp.clone()); false } else { true }); }
    s.sparklines[g].sparklines.extend(moved);
    s.sparklines.retain(|x| !x.sparklines.is_empty());
    app.dirty = true;
}

/// Splits the active cell's group into one group per sparkline.
pub fn ungroup(app: &mut SheetApp) {
    let Some(g) = group_at(app) else { return };
    let si = app.sheet();
    app.checkpoint("Ungroup Sparklines");
    let s = &mut app.book.sheets[si];
    let grp = s.sparklines.remove(g);
    for sp in &grp.sparklines { s.sparklines.insert(g, Group { sparklines: vec![sp.clone()], ..grp.clone() }); }
    app.dirty = true;
}

/// Clears the sparklines in the selection.
pub fn clear_selected(app: &mut SheetApp) {
    let si = app.sheet();
    app.checkpoint("Clear Sparklines");
    for r in app.sel.ranges() { sl::clear(&mut app.book.sheets[si], r); }
    app.dirty = true;
}

/// The Sparkline tab's body.
pub fn tab(app: &mut SheetApp, ui: &mut Ui) {
    let Some(gi) = group_at(app) else { return };
    let si = app.sheet();
    let g = app.book.sheets[si].sparklines[gi].clone();
    group(ui, "Sparkline", |ui| {
        let r = large_menu(ui, "editShape", "Edit Data", "Edit the group's data and location ranges");
        crate::theme::menu(&r, |ui| {
            if ui.button(tr("Edit Group Location & Data…")).clicked() {
                let data = g.sparklines.iter().filter_map(|sp| sheet_engine::chart::parse_ref(&sp.data)).fold(None, |a: Option<(Option<String>, Range)>, (n, r)| Some(match a { None => (n, r), Some((n0, u)) => (n0, u.union(&r)) }));
                let data = data.map(|(n, r)| sheet_engine::chart::make_ref(&n.unwrap_or_else(|| app.book.sheets[si].name.clone()), r)).unwrap_or_default();
                let loc = g.sparklines.iter().map(|sp| Range::cell(sp.loc)).reduce(|a, b| a.union(&b)).map(|r| r.a1()).unwrap_or_default();
                set_dialog(Some(Dlg { ask: Ask::Ranges { kind: g.kind, edit: Some(gi) }, data, loc, error: None }));
                ui.close();
            }
            ui.separator();
            menu_heading(ui, &tr("Hidden & Empty Cells"));
            for (e, t) in [(Empty::Gap, "Show empty cells as Gaps"), (Empty::Zero, "Show empty cells as Zero"), (Empty::Connect, "Connect data points with line")] {
                if ui.radio(g.empty == e, tr(t)).clicked() { edit_group(app, "Hidden & Empty Cells", |x| x.empty = e); ui.close(); }
            }
            if ui.checkbox(&mut g.show_hidden.clone(), tr("Show data in hidden rows and columns")).clicked() { edit_group(app, "Hidden & Empty Cells", |x| x.show_hidden = !x.show_hidden); ui.close(); }
        });
    });
    group(ui, "Type", |ui| {
        for (icon, k) in [("sparkline", Kind::Line), ("chartColumn", Kind::Column), ("chartBar", Kind::WinLoss)] {
            if large(ui, icon, k.label(), g.kind == k, true, "Change the type of the sparkline group").clicked() { edit_group(app, "Sparkline Type", |x| x.kind = k); }
        }
    });
    group(ui, "Show", |ui| {
        let flags: [(&str, fn(&mut Group) -> &mut bool); 6] = [("High Point", |x| &mut x.high), ("Low Point", |x| &mut x.low), ("Negative Points", |x| &mut x.negative), ("First Point", |x| &mut x.first), ("Last Point", |x| &mut x.last), ("Markers", |x| &mut x.markers)];
        for pair in flags.chunks(3) {
            ui.vertical(|ui| {
                for (label, f) in pair {
                    let mut gc = g.clone();
                    let mut on = *f(&mut gc);
                    let enabled = *label != "Markers" || g.kind == Kind::Line;
                    if ui.add_enabled(enabled, egui::Checkbox::new(&mut on, tr(label))).changed() { let f = *f; edit_group(app, "Show Points", move |x| *f(x) = on); }
                }
            });
        }
    });
    group(ui, "Style", |ui| {
        rows(ui, |ui| {
            let r = small_menu(ui, "colors", "Sparkline Color", "The color of the sparklines");
            crate::theme::menu(&r, |ui| { if let Some(Some(c)) = color_menu(ui, "") { edit_group(app, "Sparkline Color", move |x| x.colors.series = to_rgb(c)); } });
            let r = small_menu(ui, "penColor", "Marker Color", "The colors of the markers and singled-out points");
            crate::theme::menu(&r, |ui| {
                type Pick = fn(&mut Group) -> &mut [u8; 3];
                let items: [(&str, Pick); 6] = [("Negative Points", |x| &mut x.colors.negative), ("Markers", |x| &mut x.colors.markers), ("High Point", |x| &mut x.colors.high), ("Low Point", |x| &mut x.colors.low), ("First Point", |x| &mut x.colors.first), ("Last Point", |x| &mut x.colors.last)];
                for (label, f) in items {
                    crate::theme::submenu(ui, tr(label), |ui| { if let Some(Some(c)) = color_menu(ui, "") { edit_group(app, "Marker Color", move |x| *f(x) = to_rgb(c)); } });
                }
            });
        });
    });
    group(ui, "Group", |ui| {
        let r = large_menu(ui, "gridlines", "Axis", "Axis options");
        crate::theme::menu(&r, |ui| {
            menu_heading(ui, &tr("Horizontal Axis Options"));
            if ui.radio(g.date_axis.is_none(), tr("General Axis Type")).clicked() { edit_group(app, "Axis", |x| x.date_axis = None); ui.close(); }
            if ui.radio(g.date_axis.is_some(), tr("Date Axis Type…")).clicked() { set_dialog(Some(Dlg { ask: Ask::DateAxis, data: g.date_axis.clone().unwrap_or_default(), loc: String::new(), error: None })); ui.close(); }
            if ui.checkbox(&mut g.show_axis.clone(), tr("Show Axis")).clicked() { edit_group(app, "Axis", |x| x.show_axis = !x.show_axis); ui.close(); }
            if ui.checkbox(&mut g.right_to_left.clone(), tr("Plot Data Right-to-Left")).clicked() { edit_group(app, "Axis", |x| x.right_to_left = !x.right_to_left); ui.close(); }
            for (max, head) in [(false, "Vertical Axis Minimum Value Options"), (true, "Vertical Axis Maximum Value Options")] {
                ui.separator();
                menu_heading(ui, &tr(head));
                let cur = if max { g.max } else { g.min };
                let set = move |x: &mut Group, b: Bound| if max { x.max = b } else { x.min = b };
                if ui.radio(cur == Bound::Individual, tr("Automatic for Each Sparkline")).clicked() { edit_group(app, "Axis", move |x| set(x, Bound::Individual)); ui.close(); }
                if ui.radio(cur == Bound::Group, tr("Same for All Sparklines")).clicked() { edit_group(app, "Axis", move |x| set(x, Bound::Group)); ui.close(); }
                if ui.radio(matches!(cur, Bound::Custom(_)), tr("Custom Value…")).clicked() { let v = if let Bound::Custom(v) = cur { sheet_engine::value::fmt_num(v) } else { "0".into() }; set_dialog(Some(Dlg { ask: Ask::Bound { max }, data: v, loc: String::new(), error: None })); ui.close(); }
            }
        });
        rows(ui, |ui| {
            if small(ui, "groupObjects", Some("Group"), false, true, "Group the selected sparklines so they share formatting").clicked() { group_selection(app); }
            if small(ui, "ungroup", Some("Ungroup"), false, g.sparklines.len() > 1, "Ungroup the sparklines so each can be formatted on its own").clicked() { ungroup(app); }
            if small(ui, "clearFormat", Some("Clear"), false, true, "Clear the selected sparklines").clicked() { clear_selected(app); }
        });
    });
    ui.label(RichText::new(format!("{} {}", g.sparklines.len(), tr("sparklines in this group"))).size(11.0).color(c_ink2()));
}
