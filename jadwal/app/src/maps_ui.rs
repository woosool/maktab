//! Maps on screen: Insert ▸ Maps ▸ Filled Map (a picture that redraws from its data; settings
//! for its title and colors), and Insert ▸ 3D Map — Jadwal's map view: rows with a place
//! (country or US state names, or latitude/longitude columns) plotted as columns, bubbles or a
//! heat map on a flat world map, with a tour of scenes to step through or play, and Capture to
//! put the current scene into the sheet as a picture. It's drawn locally from bundled Natural
//! Earth outlines: no globe, terrain or online imagery.

use crate::app::SheetApp;
use crate::i18n::tr;
use sheet_engine::drawing::{ObjKind, Object};
use sheet_engine::geo::{self, FilledMap, PlotKind};
use sheet_engine::Range;
use std::sync::Arc;

const MAP_W: u32 = 480;
const MAP_H: u32 = 300;

pub fn menu(app: &mut SheetApp, ui: &mut egui::Ui) {
    if qalam_kit::widgets::menu_item(ui, "chart", "Filled Map") { insert_filled_map(app); ui.close(); }
    if selected_map(app).is_some() && qalam_kit::widgets::menu_item(ui, "preferences", "Map Settings…") { open_settings(app); ui.close(); }
}

fn source_range(app: &SheetApp) -> Range {
    let r = app.sel.range();
    if r.is_cell() { app.book.current_region(app.sheet(), app.sel.active) } else { r }
}

/// Insert ▸ Maps ▸ Filled Map, from the selection (names in its first column, values in its last).
pub fn insert_filled_map(app: &mut SheetApp) {
    let si = app.sheet();
    let r = source_range(app);
    if r.c1 == r.c0 { app.notify(&tr("Select a column of countries or states and a column of values.")); return; }
    let head = app.book.display(si, r.start());
    let has_head = !matches!(app.book.sheets[si].value(sheet_engine::Pos::new(r.r0, r.c1)), sheet_engine::Value::Num(_));
    let title = if has_head { app.book.display(si, sheet_engine::Pos::new(r.r0, r.c1)) } else { String::new() };
    let _ = head;
    let m = FilledMap { source: sheet_engine::chart::make_ref(&app.book.sheets[si].name, r), title, ..Default::default() };
    if m.rows(&app.book, si).iter().all(|(n, _)| geo::find(n, false).is_none()) { app.notify(&tr("None of the selected names is a country or US state Jadwal knows.")); return; }
    let Some(png) = m.png(&app.book, si, MAP_W, MAP_H) else { return };
    let mut o = Object::new(ObjKind::Picture { data: Arc::new(png), ext: "png".into() }, crate::objects::anchor_at_cell(app, MAP_W as f32, MAP_H as f32));
    o.descr = m.tag();
    let i = crate::objects::insert(app, o, "Map", "Insert Map");
    remember(si, i, &app.book);
    let (_, unknown) = m.svg(&app.book, si, 10.0, 10.0);
    if unknown > 0 { app.notify(&format!("{} {unknown}", tr("Names not found on the map:"))); }
}

fn map_of(o: &Object) -> Option<FilledMap> { if matches!(o.kind, ObjKind::Picture { .. }) { FilledMap::from_tag(&o.descr) } else { None } }

fn selected_map(app: &SheetApp) -> Option<(usize, FilledMap)> {
    let i = crate::objects::selected(app)?;
    map_of(&app.book.sheets[app.sheet()].objects[i]).map(|m| (i, m))
}

thread_local! { static SEEN: std::cell::RefCell<std::collections::HashMap<(usize, usize), String>> = Default::default(); }

fn signature(wb: &sheet_engine::Workbook, si: usize, m: &FilledMap) -> String { format!("{:?}{:?}", m, m.rows(wb, si)) }
fn remember(si: usize, i: usize, wb: &sheet_engine::Workbook) {
    if let Some(m) = map_of(&wb.sheets[si].objects[i]) { let s = signature(wb, si, &m); SEEN.with(|x| x.borrow_mut().insert((si, i), s)); }
}

/// After a change: redraw maps whose data or settings changed.
pub fn after_change(app: &mut SheetApp) {
    for si in 0..app.book.sheets.len() {
        for i in 0..app.book.sheets[si].objects.len() {
            let Some(m) = map_of(&app.book.sheets[si].objects[i]) else { continue };
            let sig = signature(&app.book, si, &m);
            let old = SEEN.with(|x| x.borrow().get(&(si, i)).cloned());
            if old.as_deref() == Some(sig.as_str()) { continue; }
            SEEN.with(|x| x.borrow_mut().insert((si, i), sig));
            if old.is_none() { continue; } // first sight (opened from a file): the saved picture stands
            if let Some(png) = m.png(&app.book, si, MAP_W, MAP_H) {
                let o = &mut app.book.sheets[si].objects[i];
                o.kind = ObjKind::Picture { data: Arc::new(png), ext: "png".into() };
                o.raw = None;
            }
        }
    }
}

// ---- Map Settings ----

#[derive(Clone)]
struct Settings { si: usize, i: usize, map: FilledMap }
fn settings_id() -> egui::Id { egui::Id::new("jadwal-map-settings") }

fn open_settings(app: &mut SheetApp) {
    if let Some((i, map)) = selected_map(app) { let si = app.sheet(); app.ctx.data_mut(|m| m.insert_temp(settings_id(), Settings { si, i, map })); }
}

fn settings(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = ctx.data(|m| m.get_temp::<Settings>(settings_id())) else { return };
    let mut keep = true;
    let mut open = true;
    crate::theme::dialog(ctx, "Map Settings").open(&mut open).resizable(false).show(ctx, |ui| {
        egui::Grid::new("map-settings").num_columns(2).show(ui, |ui| {
            ui.label(tr("Title:")); ui.text_edit_singleline(&mut d.map.title); ui.end_row();
            ui.label(tr("Data:")); ui.text_edit_singleline(&mut d.map.source); ui.end_row();
            ui.label(tr("Lowest value:")); ui.color_edit_button_srgb(&mut d.map.low); ui.end_row();
            ui.label(tr("Highest value:")); ui.color_edit_button_srgb(&mut d.map.high); ui.end_row();
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                if d.si < app.book.sheets.len() && d.i < app.book.sheets[d.si].objects.len() {
                    app.checkpoint("Map Settings");
                    app.book.sheets[d.si].objects[d.i].descr = d.map.tag();
                    SEEN.with(|x| x.borrow_mut().insert((d.si, d.i), String::new()));
                    after_change(app);
                    app.dirty = true;
                }
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    if keep && open { ctx.data_mut(|m| m.insert_temp(settings_id(), d)); } else { ctx.data_mut(|m| m.remove::<Settings>(settings_id())); }
}

// ---- 3D Map ----

#[derive(Clone, PartialEq)]
pub struct Scene { pub title: String, pub kind: PlotKind, pub value_col: u32, pub lon: f32, pub lat: f32, pub zoom: f32 }

#[derive(Clone)]
pub struct MapView { pub si: usize, pub range: Range, pub loc_col: u32, pub scenes: Vec<Scene>, pub cur: usize, pub playing: Option<f64>, pub missing: usize }

fn view_id() -> egui::Id { egui::Id::new("jadwal-3d-map") }
pub fn view_state(ctx: &egui::Context) -> Option<MapView> { ctx.data(|m| m.get_temp::<MapView>(view_id())) }

/// Insert ▸ 3D Map: opens the map view on the selection's data.
pub fn open_3d(app: &mut SheetApp) {
    let si = app.sheet();
    let r = source_range(app);
    if r.r1 <= r.r0 { app.notify(&tr("Select data with a header row and a column of places (countries, states, or latitude and longitude).")); return; }
    let numeric = |c: u32| (r.r0 + 1..=r.r1).any(|row| matches!(app.book.sheets[si].value(sheet_engine::Pos::new(row, c)), sheet_engine::Value::Num(_)));
    let loc_col = (r.c0..=r.c1).find(|c| !numeric(*c)).unwrap_or(r.c0);
    let value_col = (r.c0..=r.c1).rev().find(|c| numeric(*c) && !["lat", "latitude", "lon", "long", "lng", "longitude"].contains(&app.book.display(si, sheet_engine::Pos::new(r.r0, *c)).to_lowercase().trim())).unwrap_or(r.c1);
    let title = app.book.display(si, sheet_engine::Pos::new(r.r0, value_col));
    let scene = Scene { title, kind: PlotKind::Columns, value_col, lon: 10.0, lat: 20.0, zoom: 1.0 };
    let (_, missing) = geo::points_from(&app.book, si, r, loc_col, value_col);
    app.ctx.data_mut(|m| m.insert_temp(view_id(), MapView { si, range: r, loc_col, scenes: vec![scene], cur: 0, playing: None, missing }));
}

fn scene_svg(app: &SheetApp, v: &MapView, s: &Scene, w: f32, h: f32) -> String {
    let (pts, _) = geo::points_from(&app.book, v.si, v.range, v.loc_col, s.value_col);
    geo::plot_svg(&pts, s.kind, s.lon, s.lat, s.zoom, &s.title, w, h)
}

thread_local! { static TEX: std::cell::RefCell<Option<(String, egui::TextureHandle)>> = const { std::cell::RefCell::new(None) }; }

fn texture(ctx: &egui::Context, svg: String) -> Option<egui::TextureHandle> {
    if let Some(t) = TEX.with(|t| t.borrow().as_ref().filter(|x| x.0 == svg).map(|x| x.1.clone())) { return Some(t); }
    let png = geo::svg_png(&svg, 1.0)?;
    let img = image::load_from_memory(&png).ok()?.to_rgba8();
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    let t = ctx.load_texture("jadwal-3d-map", ci, egui::TextureOptions::LINEAR);
    TEX.with(|x| *x.borrow_mut() = Some((svg, t.clone())));
    Some(t)
}

/// Puts the current scene into the sheet as a picture.
pub fn capture(app: &mut SheetApp, v: &MapView) {
    let s = &v.scenes[v.cur];
    let Some(png) = geo::svg_png(&scene_svg(app, v, s, 640.0, 400.0), 1.0) else { return };
    app.book.active = v.si.min(app.book.sheets.len() - 1);
    let mut o = Object::new(ObjKind::Picture { data: Arc::new(png), ext: "png".into() }, crate::objects::anchor_at_cell(app, 640.0, 400.0));
    o.descr = format!("{} — {}", tr("3D Map"), s.title);
    crate::objects::insert(app, o, "Picture", "Capture Screen");
}

fn view_window(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut v) = view_state(ctx) else { return };
    if v.si >= app.book.sheets.len() { ctx.data_mut(|m| m.remove::<MapView>(view_id())); return; }
    let mut open = true;
    let mut do_capture = false;
    let now = ctx.input(|i| i.time);
    if let Some(t0) = v.playing {
        if now - t0 > 3.0 { if v.cur + 1 < v.scenes.len() { v.cur += 1; v.playing = Some(now); } else { v.playing = None; } }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
    egui::Window::new(tr("3D Maps")).id(egui::Id::new("jadwal-3d-map-window")).open(&mut open).default_size([760.0, 560.0]).show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            let s = &mut v.scenes[v.cur];
            for (k, label) in [(PlotKind::Columns, "Stacked Column"), (PlotKind::Bubbles, "Bubble"), (PlotKind::Heat, "Heat Map")] { if ui.selectable_label(s.kind == k, tr(label)).clicked() { s.kind = k; } }
            ui.separator();
            let si = v.si;
            egui::ComboBox::from_id_salt("map-value").selected_text(app.book.display(si, sheet_engine::Pos::new(v.range.r0, s.value_col))).show_ui(ui, |ui| {
                for c in v.range.c0..=v.range.c1 { let n = app.book.display(si, sheet_engine::Pos::new(v.range.r0, c)); if ui.selectable_label(s.value_col == c, n).clicked() { s.value_col = c; } }
            });
            ui.separator();
            if ui.button("−").on_hover_text(tr("Zoom out")).clicked() { s.zoom = (s.zoom / 1.5).max(0.7); }
            if ui.button("+").on_hover_text(tr("Zoom in")).clicked() { s.zoom = (s.zoom * 1.5).min(24.0); }
            ui.separator();
            ui.label(tr("Scene title:"));
            ui.add(egui::TextEdit::singleline(&mut s.title).desired_width(160.0));
        });
        ui.horizontal(|ui| {
            ui.label(format!("{} {} / {}", tr("Scene"), v.cur + 1, v.scenes.len()));
            if ui.add_enabled(v.cur > 0, egui::Button::new("◀")).clicked() { v.cur -= 1; }
            if ui.add_enabled(v.cur + 1 < v.scenes.len(), egui::Button::new("▶")).clicked() { v.cur += 1; }
            if ui.button(tr("New Scene")).clicked() { let s = v.scenes[v.cur].clone(); v.scenes.insert(v.cur + 1, s); v.cur += 1; }
            if ui.add_enabled(v.scenes.len() > 1, egui::Button::new(tr("Delete Scene"))).clicked() { v.scenes.remove(v.cur); v.cur = v.cur.min(v.scenes.len() - 1); }
            if v.playing.is_some() { if ui.button(tr("Stop Tour")).clicked() { v.playing = None; } }
            else if ui.add_enabled(v.scenes.len() > 1, egui::Button::new(tr("Play Tour"))).clicked() { v.cur = 0; v.playing = Some(now); }
            if crate::theme::primary(ui, "Capture Screen").on_hover_text(tr("Put this scene into the sheet as a picture")).clicked() { do_capture = true; }
        });
        if v.missing > 0 { ui.label(egui::RichText::new(format!("{} {}", v.missing, tr("rows have places Jadwal couldn't find."))).small()); }
        let avail = ui.available_size();
        let (w, h) = (avail.x.max(200.0).round(), (avail.y - 4.0).max(150.0).round());
        let s = v.scenes[v.cur].clone();
        let svg = scene_svg(app, &v, &s, w, h);
        let resp = match texture(ctx, svg) {
            Some(t) => ui.add(egui::Image::new((t.id(), egui::vec2(w, h))).sense(egui::Sense::drag())),
            None => ui.allocate_response(egui::vec2(w, h), egui::Sense::drag()),
        };
        // Drag to pan; the wheel zooms.
        let s = &mut v.scenes[v.cur];
        if resp.dragged() { let d = resp.drag_delta(); let k = w / 330.0 * s.zoom; s.lon = (s.lon - d.x / k).clamp(-180.0, 180.0); s.lat = (s.lat + d.y / k).clamp(-70.0, 80.0); }
        if resp.hovered() { let sc = ui.input(|i| i.smooth_scroll_delta.y); if sc != 0.0 { s.zoom = (s.zoom * (1.0 + sc / 300.0)).clamp(0.7, 24.0); } }
    });
    if do_capture { capture(app, &v); }
    if open { ctx.data_mut(|m| m.insert_temp(view_id(), v)); } else { ctx.data_mut(|m| m.remove::<MapView>(view_id())); }
}

pub fn frame(app: &mut SheetApp, ctx: &egui::Context) {
    settings(app, ctx);
    view_window(app, ctx);
}
