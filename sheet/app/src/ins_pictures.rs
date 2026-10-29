//! The Insert tab's illustration and text commands that make pictures or insert text: Icons,
//! Online Pictures (Openverse), Screenshot, 3D Models, Signature Line, Object, Equation and
//! Symbol, plus SmartArt's dialog. Their dialogs live here (in egui memory) and are shown from
//! the objects layer's frame hook; double-clicking one of their pictures reopens its editor.

use crate::app::SheetApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use qalam_kit::colors::*;
use sheet_engine::drawing::{Anchor, ObjKind, Object, Shape};
use sheet_engine::embed::{self, Extra};
use std::sync::{Arc, Mutex};

pub const EQ_TAG: &str = "qalam-equation:";
pub const SIG_TAG: &str = "jadwal-signature:";
pub const OBJ_TAG: &str = "jadwal-object:";

// ---- dialog state ----------------------------------------------------------------------------------

#[derive(Clone)]
pub enum Dlg {
    Icons { cat: usize, query: String },
    Online(Online),
    Model { si: usize, idx: usize, mesh: Arc<sheet_engine::model3d::Mesh>, yaw: f32, pitch: f32 },
    Signature { signer: String, title: String, email: String, instructions: String, show_date: bool },
    Equation { target: Option<(usize, usize)>, linear: String },
    Symbol { tab: u8, subset: usize, font: usize, selected: Option<char> },
    Object { icon: bool },
    SmartArt(crate::ins_smartart::Editor),
}

fn did() -> egui::Id { egui::Id::new("jadwal-insert-dialog") }
pub fn open(ctx: &egui::Context, d: Dlg) { ctx.data_mut(|m| m.insert_temp(did(), d)); }
pub fn current(ctx: &egui::Context) -> Option<Dlg> { ctx.data(|m| m.get_temp::<Dlg>(did())) }

/// Shows the open dialog (called each frame by the objects layer).
pub fn frame(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = current(ctx) else { return };
    ctx.data_mut(|m| m.remove::<Dlg>(did()));
    let keep = match &mut d {
        Dlg::Icons { cat, query } => icons_dialog(app, ctx, cat, query),
        Dlg::Online(o) => online_dialog(app, ctx, o),
        Dlg::Model { si, idx, mesh, yaw, pitch } => model_dialog(app, ctx, *si, *idx, mesh, yaw, pitch),
        Dlg::Signature { signer, title, email, instructions, show_date } => signature_dialog(app, ctx, signer, title, email, instructions, show_date),
        Dlg::Equation { target, linear } => equation_dialog(app, ctx, *target, linear),
        Dlg::Symbol { tab, subset, font, selected } => symbol_dialog(app, ctx, tab, subset, font, selected),
        Dlg::Object { icon } => object_dialog(app, ctx, icon),
        Dlg::SmartArt(ed) => crate::ins_smartart::dialog(app, ctx, ed),
    };
    // A dialog opened meanwhile (by this one) wins.
    if keep && current(ctx).is_none() { open(ctx, d); }
}

// ---- inserting pictures ------------------------------------------------------------------------------

/// The size a picture of w × h pixels gets on the sheet (fits 480 × 360).
fn fit(w: f32, h: f32, maxw: f32, maxh: f32) -> (f32, f32) { let k = (maxw / w.max(1.0)).min(maxh / h.max(1.0)).min(1.0); (w * k, h * k) }

/// Inserts image bytes (PNG, JPEG...) as a picture at the active cell.
pub fn insert_image(app: &mut SheetApp, data: Vec<u8>, descr: &str, base: &str, label: &'static str, size: Option<(f32, f32)>) -> Option<usize> {
    let img = image::load_from_memory(&data).ok()?;
    let ext = match image::guess_format(&data) { Ok(image::ImageFormat::Jpeg) => "jpeg", Ok(image::ImageFormat::Gif) => "gif", Ok(image::ImageFormat::Bmp) => "bmp", Ok(image::ImageFormat::Png) => "png", _ => "" };
    // Other formats (WebP...) become PNG.
    let (data, ext) = if ext.is_empty() { let mut c = std::io::Cursor::new(vec![]); img.write_to(&mut c, image::ImageFormat::Png).ok()?; (c.into_inner(), "png") } else { (data, ext) };
    let (w, h) = size.unwrap_or_else(|| fit(img.width() as f32, img.height() as f32, 480.0, 360.0));
    let mut o = Object::new(ObjKind::Picture { data: Arc::new(data), ext: ext.into() }, crate::objects::anchor_at_cell(app, w, h));
    if let Anchor::Two { edit_as, .. } = &mut o.anchor { *edit_as = Some("oneCell".into()); }
    o.descr = descr.into();
    Some(crate::objects::insert(app, o, base, label))
}

/// Inserts a PNG picture that carries extra parts (SVG, model, embedded file).
fn insert_with_parts(app: &mut SheetApp, png: Vec<u8>, size: (f32, f32), base: &str, descr: &str, label: &'static str, extras: Vec<Extra>) -> usize {
    let si = app.sheet();
    let mut a = crate::objects::anchor_at_cell(app, size.0, size.1);
    if let Anchor::Two { edit_as, .. } = &mut a { *edit_as = Some("oneCell".into()); }
    let o = embed::picture_with_parts(&mut app.book, si, png, a, size, base, descr, extras);
    crate::objects::insert(app, o, base, label)
}

/// Replaces a picture's image (keeping its place), e.g. after editing an equation.
fn replace_picture(app: &mut SheetApp, si: usize, i: usize, png: Vec<u8>, descr: &str, size: Option<(f32, f32)>) {
    let s = &app.book.sheets[si];
    let (cols, rows) = (crate::grid::Axis::of_cols(s), crate::grid::Axis::of_rows(s));
    let Some(o) = app.book.sheets[si].objects.get_mut(i) else { return };
    o.kind = ObjKind::Picture { data: Arc::new(png), ext: "png".into() };
    o.descr = descr.into();
    o.raw = None;
    o.rels.clear();
    if let Some((w, h)) = size { let r = o.anchor.rect(|c| cols.pos(c), |r| rows.pos(r)); o.anchor.set_rect([r[0], r[1], r[0] + w, r[1] + h], |x| cols.at(x), |c| cols.pos(c), |y| rows.at(y), |r| rows.pos(r)); }
    app.dirty = true;
}

/// Double-click on a picture this module made: reopen its editor. True when handled.
pub fn open_object(app: &mut SheetApp, si: usize, i: usize) -> bool {
    let Some(o) = app.book.sheets[si].objects.get(i) else { return false };
    if let Some(l) = o.descr.strip_prefix(EQ_TAG) { open(&app.ctx, Dlg::Equation { target: Some((si, i)), linear: l.to_string() }); return true; }
    if o.descr.starts_with(sheet_engine::model3d::TAG) {
        let Some((_, bytes)) = embed::carried_part(&app.book, o, embed::RT_MODEL3D) else { return false };
        let ext = o.descr.rsplit('.').next().unwrap_or("glb").to_string();
        let (yaw, pitch) = sheet_engine::model3d::view_of(&o.descr).unwrap_or((30.0, 20.0));
        if let Ok(m) = sheet_engine::model3d::load(&bytes, &ext) { open(&app.ctx, Dlg::Model { si, idx: i, mesh: Arc::new(m), yaw, pitch }); return true; }
        return false;
    }
    if let Some(name) = o.descr.strip_prefix(OBJ_TAG) {
        let Some((_, bytes)) = embed::carried_part(&app.book, o, embed::RT_PACKAGE) else { return false };
        let dir = std::env::temp_dir().join(format!("jadwal-objects-{}", std::process::id()));
        let path = dir.join(std::path::Path::new(name).file_name().unwrap_or_default());
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &bytes)) {
            Ok(()) => { if let Err(e) = open_with_os(&path) { app.error = Some(format!("{}\n{e}", tr("The object couldn't be opened."))); } }
            Err(e) => app.error = Some(format!("{}\n{e}", tr("The object couldn't be opened."))),
        }
        return true;
    }
    false
}

fn open_with_os(p: &std::path::Path) -> std::io::Result<()> {
    if cfg!(test) { return Ok(()); }
    #[cfg(target_os = "macos")] { std::process::Command::new("open").arg(p).spawn().map(|_| ()) }
    #[cfg(target_os = "windows")] { std::process::Command::new("cmd").args(["/C", "start", ""]).arg(p).spawn().map(|_| ()) }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))] { std::process::Command::new("xdg-open").arg(p).spawn().map(|_| ()) }
}

// ---- Icons ---------------------------------------------------------------------------------------------

/// Icon categories: (name, words in an icon's name that put it there).
pub const ICON_CATS: &[(&str, &[&str])] = &[
    ("All", &[]),
    ("Arrows and Navigation", &["arrow", "undo", "redo", "move", "back", "forward", "next", "previous", "goTo", "expand", "collapse", "up", "down", "flip", "rotate"]),
    ("Charts and Data", &["chart", "pivot", "sparkline", "sum", "sort", "filter", "table", "function", "formula", "math", "lookup", "financial", "logical", "percent", "decimal", "number"]),
    ("Text and Writing", &["text", "font", "bold", "italic", "underline", "strike", "align", "indent", "bullet", "numbering", "paragraph", "caps", "spelling", "dictate", "word", "letter", "case", "translate", "language", "symbol", "equation"]),
    ("Files and Documents", &["file", "folder", "save", "print", "pdf", "page", "document", "export", "download", "open", "copy", "paste", "cut", "header", "footer", "cover", "envelope", "label"]),
    ("Drawing and Shapes", &["shape", "draw", "pen", "ink", "eraser", "brush", "paint", "color", "fill", "outline", "border", "highlight", "effect", "bevel", "glow", "gradient", "smartArt", "wordArt"]),
    ("Media", &["picture", "image", "photo", "video", "audio", "media", "screenshot", "camera", "record", "volume", "mute", "play", "model3d", "icons"]),
    ("People and Communication", &["comment", "share", "people", "invite", "coauthor", "user", "mail", "recipient", "greeting", "accessibility", "help", "info"]),
    ("Layout and View", &["layout", "view", "zoom", "grid", "ruler", "column", "row", "cell", "merge", "split", "margin", "orientation", "size", "window", "slide", "section", "freeze", "group", "arrange", "select"]),
];

/// Every icon in the kit's sets, by name (deduplicated), with its SVG body and defs.
pub fn all_icons() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut v: Vec<(&str, &str, &str)> = vec![];
    let mut seen = std::collections::HashSet::new();
    for (set, defs) in [(qalam_kit::icon_data::ICONS, qalam_kit::icon_data::DEFS), (qalam_kit::slide_icon_data::ICONS, qalam_kit::slide_icon_data::DEFS), (qalam_kit::sheet_icon_data::ICONS, qalam_kit::sheet_icon_data::DEFS)] {
        for e in set.iter() { if seen.insert(e.0) { v.push((e.0, e.2, defs)); } }
    }
    v.sort_by_key(|e| e.0.to_ascii_lowercase());
    v
}

fn icon_matches(name: &str, cat: usize, query: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let in_cat = cat == 0 || ICON_CATS[cat].1.iter().any(|w| lower.contains(&w.to_ascii_lowercase()));
    in_cat && (query.trim().is_empty() || lower.contains(&query.trim().to_ascii_lowercase()) || crate::i18n::tr(&split_name(name)).to_lowercase().contains(&query.trim().to_lowercase()))
}

/// "chartColumn" → "Chart Column".
fn split_name(n: &str) -> String {
    let mut s = String::new();
    for (i, c) in n.chars().enumerate() { if i == 0 { s.extend(c.to_uppercase()); } else { if c.is_uppercase() || (c.is_ascii_digit() && !s.ends_with(|x: char| x.is_ascii_digit())) { s.push(' '); } s.push(c); } }
    s
}

/// An icon as a standalone SVG document.
pub fn icon_svg(name: &str) -> Option<String> {
    let (_, body, defs) = all_icons().into_iter().find(|e| e.0 == name)?;
    Some(format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="96" height="96" color="#2c2c2e"><defs>{defs}</defs>{body}</svg>"##))
}

/// Inserts an icon: a PNG rendering with the SVG alongside (svgBlip), 96 × 96 px.
pub fn insert_icon(app: &mut SheetApp, name: &str) -> Option<usize> {
    let svg = icon_svg(name)?;
    let png = embed::rasterize_svg(svg.as_bytes(), 192, 192)?;
    Some(insert_with_parts(app, png, (96.0, 96.0), "Graphic", &split_name(name), "Insert Icon", vec![Extra { rel: embed::RT_IMAGE, prefix: "xl/media/image", ext: "svg".into(), mime: "image/svg+xml".into(), bytes: svg.into_bytes() }]))
}

fn icons_dialog(app: &mut SheetApp, ctx: &egui::Context, cat: &mut usize, query: &mut String) -> bool {
    let mut open = true;
    let mut pick: Option<&'static str> = None;
    crate::theme::dialog(ctx, "Insert Icons").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_width(620.0);
        ui.add(egui::TextEdit::singleline(query).hint_text(tr("Search icons")).desired_width(600.0));
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(170.0);
                for (k, (n, _)) in ICON_CATS.iter().enumerate() { if ui.selectable_label(*cat == k, tr(n)).clicked() { *cat = k; } }
            });
            egui::ScrollArea::vertical().id_salt("icon-grid").max_height(380.0).show(ui, |ui| {
                ui.set_width(430.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    let list: Vec<_> = all_icons().into_iter().filter(|e| icon_matches(e.0, *cat, query)).collect();
                    if list.is_empty() { ui.label(RichText::new(tr("No icons match.")).color(c_ink3())); }
                    for (name, ..) in list {
                        let (r, resp) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
                        if !ui.is_rect_visible(r) { continue; }
                        ui.painter().rect(r, 4.0, if resp.hovered() { c_hover() } else { c_card() }, Stroke::new(1.0, c_line()), StrokeKind::Inside);
                        qalam_kit::widgets::paint_icon(ui, name, Rect::from_center_size(r.center(), vec2(32.0, 32.0)), 32, true);
                        if resp.on_hover_text(split_name(name)).clicked() { pick = Some(name); }
                    }
                });
            });
        });
        ui.label(RichText::new(tr("Click an icon to insert it.")).size(11.0).color(c_ink3()));
    });
    if let Some(n) = pick { insert_icon(app, n); return false; }
    open
}

// ---- Online Pictures (Openverse) ---------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct Hit { pub title: String, pub creator: String, pub license: String, pub url: String, pub thumb_url: String, pub landing: String, pub thumb: Option<Arc<image::RgbaImage>> }

#[derive(Clone, Default)]
pub enum Status { #[default] Idle, Busy, Done(Vec<Hit>), Failed(String) }

#[derive(Clone, Default)]
pub struct Online { pub query: String, pub url: String, pub search: Arc<Mutex<Status>>, pub download: Arc<Mutex<Option<Result<(Vec<u8>, String), String>>>>, pub selected: Option<usize>, pub downloading: bool }

const OFFLINE: &str = "Online pictures need an internet connection. Check your connection and try again.";

fn agent() -> ureq::Agent { ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(20))).build().into() }

fn get(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut r = agent().get(url).header("User-Agent", "Jadwal/0.1 (spreadsheet; +https://openverse.org)").call().map_err(|e| e.to_string())?;
    r.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| e.to_string())
}

fn enc(q: &str) -> String { q.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else if b == b' ' { "+".into() } else { format!("%{b:02X}") }).collect() }

/// Openverse's JSON results as hits (without thumbnails).
pub fn parse_results(json: &[u8]) -> Result<Vec<Hit>, String> {
    let v: serde_json::Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let s = |x: &serde_json::Value| x.as_str().unwrap_or("").to_string();
    Ok(v["results"].as_array().cloned().unwrap_or_default().iter().map(|r| Hit {
        title: s(&r["title"]), creator: s(&r["creator"]),
        license: format!("CC {} {}", s(&r["license"]).to_uppercase(), s(&r["license_version"])).replace("CC CC0", "CC0").replace("CC PDM", "Public Domain Mark").trim().to_string(),
        url: s(&r["url"]), thumb_url: if s(&r["thumbnail"]).is_empty() { s(&r["url"]) } else { s(&r["thumbnail"]) }, landing: s(&r["foreign_landing_url"]), thumb: None,
    }).filter(|h| !h.url.is_empty()).collect())
}

fn start_search(ctx: &egui::Context, o: &mut Online) {
    let q = o.query.trim().to_string();
    if q.is_empty() { return; }
    o.selected = None;
    let st = o.search.clone();
    *st.lock().unwrap() = Status::Busy;
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let url = format!("https://api.openverse.org/v1/images/?q={}&page_size=20&mature=false", enc(&q));
        let res = get(&url, 4_000_000).and_then(|b| parse_results(&b));
        match res {
            Err(_) => *st.lock().unwrap() = Status::Failed(OFFLINE.into()),
            Ok(mut hits) => {
                *st.lock().unwrap() = Status::Done(hits.clone());
                ctx.request_repaint();
                for k in 0..hits.len() {
                    if let Ok(b) = get(&hits[k].thumb_url, 8_000_000) {
                        if let Ok(img) = image::load_from_memory(&b) { hits[k].thumb = Some(Arc::new(img.thumbnail(200, 200).to_rgba8())); }
                    }
                    *st.lock().unwrap() = Status::Done(hits.clone());
                    ctx.request_repaint();
                }
            }
        }
        ctx.request_repaint();
    });
}

fn start_download(ctx: &egui::Context, o: &mut Online, url: String, credit: String) {
    o.downloading = true;
    let slot = o.download.clone();
    *slot.lock().unwrap() = None;
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let r = get(&url, 40_000_000).map_err(|_| OFFLINE.to_string()).and_then(|b| if image::load_from_memory(&b).is_ok() { Ok((b, credit)) } else { Err("That address isn't a picture Jadwal can show.".to_string()) });
        *slot.lock().unwrap() = Some(r);
        ctx.request_repaint();
    });
}

fn online_dialog(app: &mut SheetApp, ctx: &egui::Context, o: &mut Online) -> bool {
    // A finished download goes in.
    if o.downloading {
        let r = o.download.lock().unwrap().take();
        if let Some(r) = r {
            o.downloading = false;
            match r {
                Ok((bytes, credit)) => { if insert_image(app, bytes, &credit, "Picture", "Insert Online Picture", None).is_some() { app.notify(&tr("Picture inserted. Credit the creator as its license asks.")); return false; } }
                Err(e) => app.error = Some(tr(&e)),
            }
        }
    }
    let mut open = true;
    crate::theme::dialog(ctx, "Online Pictures").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_width(640.0);
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut o.query).hint_text(tr("Search Openverse")).desired_width(520.0));
            if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) | ui.button(tr("Search")).clicked() { start_search(ui.ctx(), o); }
        });
        let status = o.search.lock().unwrap().clone();
        let (r, _) = ui.allocate_exact_size(vec2(640.0, 330.0), Sense::hover());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
        match status {
            Status::Idle => { child.label(RichText::new(tr("Search millions of openly licensed pictures from Openverse.")).color(c_ink3())); }
            Status::Busy => { child.horizontal(|ui| { ui.spinner(); ui.label(tr("Searching…")); }); }
            Status::Failed(e) => { child.label(RichText::new(tr(&e)).color(c_ink3())); }
            Status::Done(hits) if hits.is_empty() => { child.label(RichText::new(tr("No pictures match.")).color(c_ink3())); }
            Status::Done(hits) => {
                egui::ScrollArea::vertical().id_salt("ov-grid").max_height(330.0).show(&mut child, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        for (k, h) in hits.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(vec2(146.0, 110.0), Sense::click());
                            let on = o.selected == Some(k);
                            ui.painter().rect(r, 3.0, c_card(), Stroke::new(if on { 2.0 } else { 1.0 }, if on { ACCENT } else { c_line() }), StrokeKind::Inside);
                            match &h.thumb {
                                Some(img) => {
                                    let key = egui::Id::new(("ov-thumb", &h.thumb_url));
                                    let tex = ui.data(|d| d.get_temp::<egui::TextureHandle>(key)).unwrap_or_else(|| {
                                        let t = ui.ctx().load_texture(format!("ov-{k}-{}", h.thumb_url), egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR);
                                        ui.data_mut(|d| d.insert_temp(key, t.clone()));
                                        t
                                    });
                                    let s = tex.size_vec2();
                                    let inner = r.shrink(4.0);
                                    let kk = (inner.width() / s.x).min(inner.height() / s.y);
                                    ui.painter().image(tex.id(), Rect::from_center_size(inner.center(), s * kk), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                                }
                                None => { ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(16.0), c_ink3()); }
                            }
                            if resp.on_hover_text(format!("{}\n{} {}\n{}", h.title, tr("by"), h.creator, h.license)).clicked() { o.selected = Some(k); }
                        }
                    });
                });
                if let Some(h) = o.selected.and_then(|k| hits.get(k)) {
                    ui.label(RichText::new(format!("“{}” {} {} — {}", h.title, tr("by"), if h.creator.is_empty() { tr("unknown") } else { h.creator.clone() }, h.license)).size(12.0));
                    if !h.landing.is_empty() { ui.hyperlink_to(RichText::new(tr("Source and license")).size(11.5), &h.landing); }
                }
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(tr("From a URL:"));
            ui.add(egui::TextEdit::singleline(&mut o.url).hint_text("https://…").desired_width(400.0));
            if ui.add_enabled(!o.url.trim().is_empty() && !o.downloading, egui::Button::new(tr("Insert from URL"))).clicked() {
                let u = o.url.trim().to_string();
                start_download(ui.ctx(), o, u.clone(), u);
            }
        });
        let pick = match &*o.search.lock().unwrap() { Status::Done(h) => o.selected.and_then(|k| h.get(k).cloned()), _ => None };
        let go = crate::theme::footer(ui, |ui| { if o.downloading { ui.spinner(); } crate::theme::primary_enabled(ui, pick.is_some() && !o.downloading, "Insert").clicked() });
        if go { if let Some(h) = pick { let credit = format!("\"{}\" by {} — {} ({})", h.title, h.creator, h.license, h.landing); start_download(ui.ctx(), o, h.url.clone(), credit); } }
    });
    open
}

// ---- Screenshot ---------------------------------------------------------------------------------------

type Shots = Arc<Mutex<Option<Result<Vec<(String, Arc<image::RgbaImage>)>, String>>>>;
fn shots_id() -> egui::Id { egui::Id::new("jadwal-shots") }

/// The Screenshot menu: Available Windows (captured when it opens) and Screen Clipping.
pub fn screenshot_menu(app: &mut SheetApp, ui: &mut Ui) {
    let shots: Shots = match ui.data(|d| d.get_temp::<Shots>(shots_id())) {
        Some(s) => s,
        None => {
            let shots: Shots = Arc::new(Mutex::new(None));
            let s = shots.clone();
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let mut out = vec![];
                if let Ok(ws) = xcap::Window::all() {
                    for w in ws.into_iter().filter(|w| !w.is_minimized().unwrap_or(true)).take(12) {
                        let title = w.title().unwrap_or_default();
                        if title.trim().is_empty() || title.contains(crate::APP_NAME) { continue; }
                        if let Ok(img) = w.capture_image() { if img.width() > 40 && img.height() > 40 { out.push((format!("{} — {}", w.app_name().unwrap_or_default(), title), Arc::new(img))); } }
                    }
                }
                *s.lock().unwrap() = Some(if out.is_empty() { Err("No windows could be captured. On macOS, allow screen recording for Jadwal in System Settings → Privacy & Security.".into()) } else { Ok(out) });
                ctx.request_repaint();
            });
            ui.data_mut(|d| d.insert_temp(shots_id(), shots.clone()));
            shots
        }
    };
    qalam_kit::widgets::menu_heading(ui, "Available Windows");
    let state = shots.lock().ok().and_then(|g| g.clone());
    match state {
        None => { ui.horizontal(|ui| { ui.spinner(); ui.label(tr("Capturing…")); }); }
        Some(Err(e)) => { ui.label(RichText::new(tr(&e)).color(c_ink3())); }
        Some(Ok(list)) => {
            ui.set_width(420.0);
            ui.horizontal_wrapped(|ui| for (k, (title, img)) in list.iter().enumerate() {
                let key = egui::Id::new(("shot-tex", k, Arc::as_ptr(img) as usize));
                let tex = ui.data(|d| d.get_temp::<egui::TextureHandle>(key)).unwrap_or_else(|| {
                    let small = image::imageops::thumbnail(&**img, 180, (180 * img.height() / img.width().max(1)).max(1));
                    let t = ui.ctx().load_texture(format!("shot-{k}"), egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw()), egui::TextureOptions::LINEAR);
                    ui.data_mut(|d| d.insert_temp(key, t.clone()));
                    t
                });
                let (r, resp) = ui.allocate_exact_size(vec2(130.0, 90.0), Sense::click());
                let s = tex.size_vec2();
                let k2 = (r.width() / s.x).min(r.height() / s.y);
                ui.painter().image(tex.id(), Rect::from_center_size(r.center(), s * k2), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                if resp.hovered() { ui.painter().rect_stroke(r, 2.0, Stroke::new(2.0, ACCENT), StrokeKind::Inside); }
                if resp.on_hover_text(title).clicked() {
                    insert_rgba(app, img, title);
                    ui.data_mut(|d| d.remove::<Shots>(shots_id()));
                    ui.close();
                }
            });
        }
    }
    ui.separator();
    if qalam_kit::widgets::menu_item(ui, "screenshot", "Screen Clipping") {
        ui.data_mut(|d| d.remove::<Shots>(shots_id()));
        ui.close();
        screen_clipping(app);
    }
}

fn insert_rgba(app: &mut SheetApp, img: &image::RgbaImage, title: &str) {
    let mut png = std::io::Cursor::new(Vec::new());
    if image::DynamicImage::ImageRgba8(img.clone()).write_to(&mut png, image::ImageFormat::Png).is_ok() { insert_image(app, png.into_inner(), title, "Picture", "Insert Screenshot", None); }
}

/// Screen Clipping: captures the main screen, then lets a rectangle of it be picked.
fn screen_clipping(app: &mut SheetApp) {
    let cap = xcap::Monitor::all().ok().and_then(|ms| ms.into_iter().find(|m| m.is_primary().unwrap_or(false)).or(None)).or_else(|| xcap::Monitor::all().ok().and_then(|m| m.into_iter().next())).and_then(|m| m.capture_image().ok());
    match cap {
        Some(img) => { app.ctx.data_mut(|d| { d.insert_temp(clip_id(), Clip { img: Arc::new(img), start: None, tex: None }); }); }
        None => app.error = Some(tr("The screen couldn't be captured. On macOS, allow screen recording for Jadwal in System Settings → Privacy & Security.")),
    }
}

#[derive(Clone)]
struct Clip { img: Arc<image::RgbaImage>, start: Option<egui::Pos2>, tex: Option<egui::TextureHandle> }
fn clip_id() -> egui::Id { egui::Id::new("jadwal-screen-clip") }

/// The screen clipping overlay (a frozen picture of the screen to drag a rectangle on).
pub fn clip_overlay(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut c) = ctx.data(|d| d.get_temp::<Clip>(clip_id())) else { return };
    let tex = c.tex.get_or_insert_with(|| ctx.load_texture("screen-clip", egui::ColorImage::from_rgba_unmultiplied([c.img.width() as usize, c.img.height() as usize], c.img.as_raw()), egui::TextureOptions::LINEAR)).clone();
    let mut keep = true;
    egui::Area::new(egui::Id::new("screen-clip-area")).order(egui::Order::Foreground).fixed_pos(pos2(0.0, 0.0)).show(ctx, |ui| {
        let screen = ctx.content_rect();
        let s = tex.size_vec2();
        let k = (screen.width() / s.x).min(screen.height() / s.y);
        let pic = Rect::from_min_size(screen.min, s * k);
        let resp = ui.allocate_rect(screen, Sense::click_and_drag());
        ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(200));
        ui.painter().image(tex.id(), pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_white_alpha(170));
        ui.painter().text(screen.center_top() + vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, tr("Drag to clip part of the screen (Esc cancels)"), egui::FontId::proportional(15.0), Color32::WHITE);
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        if resp.drag_started() { c.start = resp.interact_pointer_pos(); }
        if let (Some(a), Some(b)) = (c.start, ui.input(|i| i.pointer.interact_pos())) {
            let r = Rect::from_two_pos(a, b).intersect(pic);
            ui.painter().image(tex.id(), r, Rect::from_min_max(((r.min - pic.min) / pic.size()).to_pos2(), ((r.max - pic.min) / pic.size()).to_pos2()), Color32::WHITE);
            ui.painter().rect_stroke(r, 0.0, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
            if resp.drag_stopped() {
                let (x0, y0) = (((r.min.x - pic.min.x) / k).max(0.0) as u32, ((r.min.y - pic.min.y) / k).max(0.0) as u32);
                let (w, h) = ((r.width() / k) as u32, (r.height() / k) as u32);
                if w > 2 && h > 2 { let sub = image::imageops::crop_imm(&*c.img, x0, y0, w.min(c.img.width() - x0), h.min(c.img.height() - y0)).to_image(); insert_rgba(app, &sub, "Screen Clipping"); }
                keep = false;
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { keep = false; }
    });
    ctx.data_mut(|d| if keep { d.insert_temp(clip_id(), c); } else { d.remove::<Clip>(clip_id()); });
}

// ---- 3D Models -----------------------------------------------------------------------------------------

/// Insert ▸ 3D Models: picks a .glb or .obj file and inserts it turned to a default view.
pub fn insert_model(app: &mut SheetApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("3D Models"), &["glb", "obj"]).pick_file() else { return };
    let bytes = match std::fs::read(&path) { Ok(b) => b, Err(e) => { app.error = Some(format!("{}\n{e}", tr("The model couldn't be read."))); return } };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "model.glb".into());
    if let Err(e) = insert_model_bytes(app, bytes, &name) { app.error = Some(tr(&e)); }
}

pub fn insert_model_bytes(app: &mut SheetApp, bytes: Vec<u8>, name: &str) -> Result<usize, String> {
    let ext = name.rsplit('.').next().unwrap_or("glb").to_ascii_lowercase();
    let m = sheet_engine::model3d::load(&bytes, &ext)?;
    let (yaw, pitch) = (30.0, 20.0);
    let png = sheet_engine::model3d::render_png(&m, 480, 480, yaw, pitch).ok_or("The model couldn't be drawn.")?;
    let (mime, ext) = if ext == "obj" { ("model/obj", "obj") } else { ("model/gltf-binary", "glb") };
    let descr = format!("{}{yaw},{pitch} {name}", sheet_engine::model3d::TAG);
    let i = insert_with_parts(app, png, (240.0, 240.0), "3D Model", &descr, "Insert 3D Model", vec![Extra { rel: embed::RT_MODEL3D, prefix: "xl/models/model3d", ext: ext.into(), mime: mime.into(), bytes }]);
    app.notify(&tr("Double-click the model to turn it."));
    Ok(i)
}

fn model_dialog(app: &mut SheetApp, ctx: &egui::Context, si: usize, idx: usize, mesh: &Arc<sheet_engine::model3d::Mesh>, yaw: &mut f32, pitch: &mut f32) -> bool {
    let mut open = true;
    let mut apply = false;
    crate::theme::dialog(ctx, "3D Model View").open(&mut open).resizable(false).show(ctx, |ui| {
        let px = sheet_engine::model3d::render(mesh, 240, 240, *yaw, *pitch);
        let t = ui.ctx().load_texture("model-preview", egui::ColorImage::from_rgba_unmultiplied([240, 240], &px), egui::TextureOptions::LINEAR);
        let (r, resp) = ui.allocate_exact_size(vec2(240.0, 240.0), Sense::drag());
        ui.painter().rect_filled(r, 4.0, Color32::WHITE);
        ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        // Dragging the preview turns the model (the rotate handle).
        if resp.dragged() { let d = resp.drag_delta(); *yaw = (*yaw + d.x).rem_euclid(360.0); *pitch = (*pitch - d.y).clamp(-89.0, 89.0); }
        if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::Grab); }
        ui.add(egui::Slider::new(yaw, 0.0..=360.0).text(tr("Turn")));
        ui.add(egui::Slider::new(pitch, -89.0..=89.0).text(tr("Tilt")));
        apply = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
    });
    if apply {
        if let Some(png) = sheet_engine::model3d::render_png(mesh, 480, 480, *yaw, *pitch) {
            app.checkpoint("Turn 3D Model");
            if let Some(o) = app.book.sheets[si].objects.get_mut(idx) {
                let name = o.descr.split_once(' ').map(|x| x.1.to_string()).unwrap_or_default();
                o.descr = format!("{}{},{} {name}", sheet_engine::model3d::TAG, yaw.round(), pitch.round());
                // The new rendering replaces the image part; the model part stays.
                if let Some(r) = o.rels.iter().find(|r| r.kind == embed::RT_IMAGE).cloned() { app.book.parts.insert(r.target.clone(), png.clone()); }
                o.kind = ObjKind::Picture { data: Arc::new(png), ext: "png".into() };
                if let Some(raw) = o.raw.as_mut() { let mut e = (**raw).clone(); set_descr(&mut e, &o.descr); *raw = Arc::new(e); }
                app.dirty = true;
            }
        }
        return false;
    }
    open
}

fn set_descr(e: &mut sheet_engine::xlsx::xml::Element, d: &str) {
    if e.local() == "cNvPr" { e.attrs.retain(|(k, _)| k != "descr"); e.attrs.push(("descr".into(), d.into())); return; }
    for c in e.children.iter_mut() { if let sheet_engine::xlsx::xml::Node::Element(x) = c { set_descr(x, d); } }
}

// ---- Signature Line ------------------------------------------------------------------------------------

/// The signature line picture: an X, a line, the signer's name and title below.
pub fn signature_png(signer: &str, title: &str, date: bool) -> Option<Vec<u8>> {
    let (w, h) = (288.0, 144.0);
    let base = Shape { preset: "rect".into(), text_box: true, size: 11.0, color: Some([0x20, 0x20, 0x20]), ..Default::default() };
    let mut list = vec![
        (Shape { preset: "rect".into(), fill: Some([255, 255, 255]), ..Default::default() }, [0.0, 0.0, w, h]),
        (Shape { text: "X".into(), size: 28.0, ..base.clone() }, [8.0, 30.0, 60.0, 84.0]),
        (Shape { preset: "line".into(), line: Some([0x40, 0x40, 0x40]), ..Default::default() }, [10.0, 82.0, w - 10.0, 82.0]),
        (Shape { text: [signer, title].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("\n"), ..base.clone() }, [10.0, 86.0, w - 10.0, h - 4.0]),
    ];
    if date { list.push((Shape { text: tr("Date"), size: 9.0, color: Some([0x70, 0x70, 0x70]), ..base.clone() }, [w - 70.0, 62.0, w - 8.0, 82.0])); }
    sheet_engine::embed::shapes_png(&list, w, h)
}

fn signature_dialog(app: &mut SheetApp, ctx: &egui::Context, signer: &mut String, title: &mut String, email: &mut String, instructions: &mut String, show_date: &mut bool) -> bool {
    let mut open = true;
    let mut ok = false;
    crate::theme::dialog(ctx, "Signature Setup").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_width(360.0);
        for (label, v) in [("Suggested signer (for example, John Doe):", &mut *signer), ("Suggested signer's title (for example, Manager):", &mut *title), ("Suggested signer's e-mail address:", &mut *email)] {
            ui.label(tr(label));
            ui.add(egui::TextEdit::singleline(v).desired_width(340.0));
        }
        ui.label(tr("Instructions to the signer:"));
        ui.add(egui::TextEdit::multiline(instructions).desired_rows(2).desired_width(340.0));
        crate::theme::check(ui, show_date, tr("Show sign date in signature line"));
        ok = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "OK").clicked());
    });
    if ok {
        if let Some(png) = signature_png(signer, title, *show_date) {
            let descr = format!("{SIG_TAG}{}\t{}\t{}\t{}", signer, title, email, instructions.replace('\n', " "));
            insert_image(app, png, &descr, "Signature Line", "Insert Signature Line", Some((288.0, 144.0)));
        }
        return false;
    }
    open
}

pub fn signature_menu(app: &mut SheetApp, ui: &mut Ui) {
    if ui.button(tr("Signature Line…")).clicked() { open(&app.ctx, Dlg::Signature { signer: String::new(), title: String::new(), email: String::new(), instructions: tr("Before signing this document, verify that the content you are signing is correct."), show_date: true }); ui.close(); }
    if ui.button(tr("Add Signature Services…")).clicked() { app.notify(&tr("Digital signing services aren't available; the signature line prints for signing on paper.")); ui.close(); }
}

// ---- Object --------------------------------------------------------------------------------------------

fn object_dialog(app: &mut SheetApp, ctx: &egui::Context, icon: &mut bool) -> bool {
    let mut open = true;
    let mut browse = false;
    crate::theme::dialog(ctx, "Object").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.label(RichText::new(tr("Create from File")).strong());
        ui.label(RichText::new(tr("Inserts the contents of the file into your document so that you can open it later with the program that made it.")).size(11.5).color(c_ink3()));
        crate::theme::check(ui, icon, tr("Display as icon"));
        browse = crate::theme::footer(ui, |ui| crate::theme::primary(ui, "Browse…").clicked());
    });
    if browse {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            match std::fs::read(&path) {
                Ok(b) => { let n = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(); insert_object_bytes(app, b, &n, *icon); }
                Err(e) => app.error = Some(format!("{}\n{e}", tr("The file couldn't be read."))),
            }
        }
        return false;
    }
    open
}

/// Embeds a file (an OLE package part) shown as an icon with its name, or as the picture itself
/// for image files not shown as icons. Double-click opens it with its program.
pub fn insert_object_bytes(app: &mut SheetApp, bytes: Vec<u8>, name: &str, as_icon: bool) -> usize {
    let picture = (!as_icon).then(|| image::load_from_memory(&bytes).ok()).flatten();
    let (png, size) = match picture {
        Some(img) => {
            let mut c = std::io::Cursor::new(vec![]);
            let _ = img.write_to(&mut c, image::ImageFormat::Png);
            (c.into_inner(), fit(img.width() as f32, img.height() as f32, 480.0, 360.0))
        }
        None => (object_icon_png(name).unwrap_or_default(), (96.0, 80.0)),
    };
    insert_with_parts(app, png, size, "Object", &format!("{OBJ_TAG}{name}"), "Insert Object", vec![Extra { rel: embed::RT_PACKAGE, prefix: "xl/embeddings/oleObject", ext: "bin".into(), mime: "application/vnd.openxmlformats-officedocument.oleObject".into(), bytes }])
}

/// A file's icon picture: a page with its extension, the name below.
fn object_icon_png(name: &str) -> Option<Vec<u8>> {
    let ext = name.rsplit_once('.').map(|x| x.1.to_uppercase()).unwrap_or_default();
    let base = Shape { preset: "rect".into(), text_box: true, center: true, size: 8.0, color: Some([0x20, 0x20, 0x20]), ..Default::default() };
    let list = vec![
        (Shape { preset: "foldedCorner".into(), fill: Some([0xF4, 0xF6, 0xFA]), line: Some([0x70, 0x78, 0x88]), ..Default::default() }, [32.0, 4.0, 64.0, 46.0]),
        (Shape { text: ext, bold: true, size: 7.0, color: Some([0x2B, 0x57, 0x9A]), ..base.clone() }, [28.0, 16.0, 68.0, 34.0]),
        (Shape { text: name.to_string(), ..base.clone() }, [0.0, 50.0, 96.0, 80.0]),
    ];
    sheet_engine::embed::shapes_png(&list, 96.0, 80.0)
}

// ---- Equation ------------------------------------------------------------------------------------------

/// An equation rendered as a picture (word-engine math): PNG at 2× and its size in pixels.
pub fn equation_png(linear: &str) -> Option<(Vec<u8>, (f32, f32))> {
    let eq = word_engine::equation::Equation { linear: linear.into(), size: 16.0, color: word_engine::Color::BLACK, display: true };
    let png = word_engine::objects::object_png(&word_engine::objects::Object::Equation(eq.clone()))?;
    let (w, h, _) = eq.metrics();
    Some((png, (w.max(4.0), h.max(4.0))))
}

/// Inserts (or, with a target, replaces) an equation picture.
pub fn insert_equation(app: &mut SheetApp, target: Option<(usize, usize)>, linear: &str) -> Option<usize> {
    let (png, size) = equation_png(linear)?;
    let descr = format!("{EQ_TAG}{linear}");
    match target {
        Some((si, i)) if i < app.book.sheets[si].objects.len() => { app.checkpoint("Edit Equation"); replace_picture(app, si, i, png, &descr, Some(size)); Some(i) }
        _ => insert_image(app, png, &descr, "Equation", "Insert Equation", Some(size)),
    }
}

fn equation_dialog(app: &mut SheetApp, ctx: &egui::Context, target: Option<(usize, usize)>, linear: &mut String) -> bool {
    use qalam_kit::symbols::{MATH_SYMBOLS, STRUCTURES};
    let mut open = true;
    let mut ok = false;
    crate::theme::dialog(ctx, "Equation").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_width(460.0);
        egui::ScrollArea::horizontal().id_salt("eq-structs").show(ui, |ui| ui.horizontal(|ui| for (group, _, items) in STRUCTURES {
            crate::theme::submenu(ui, tr(group), |ui| for (n, src) in *items { if ui.button(tr(n)).clicked() { linear.push_str(src); ui.close(); } });
        }));
        ui.horizontal_wrapped(|ui| { ui.set_max_width(440.0); for c in MATH_SYMBOLS.chars() { if ui.small_button(RichText::new(c.to_string()).font(qalam_kit::symbols::glyph_font(c, 14.0))).clicked() { linear.push(c); } } });
        ui.add(egui::TextEdit::multiline(linear).desired_width(440.0).desired_rows(3).font(egui::FontId::monospace(13.0)).hint_text(tr("Type an equation, e.g. x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}")));
        if let Some((png, _)) = equation_png(linear) {
            if let Ok(img) = image::load_from_memory(&png) {
                let img = img.to_rgba8();
                let t = ui.ctx().load_texture("eq-preview", egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR);
                let s = t.size_vec2() / 2.0;
                let k = (440.0 / s.x).min(1.0);
                egui::Frame::new().fill(Color32::WHITE).inner_margin(6.0).show(ui, |ui| { ui.image((t.id(), s * k)); });
            }
        }
        ok = crate::theme::footer(ui, |ui| crate::theme::primary_enabled(ui, !linear.trim().is_empty(), "Insert").clicked());
    });
    if ok { insert_equation(app, target, linear.trim()); return false; }
    open
}

/// The Equation menu: built-in equations and Insert New Equation.
pub fn equation_menu(app: &mut SheetApp, ui: &mut Ui) {
    ui.set_width(300.0);
    qalam_kit::widgets::menu_heading(ui, "Built-In");
    for (name, src) in word_engine::equation::GALLERY {
        if ui.button(tr(name)).on_hover_text(*src).clicked() { insert_equation(app, None, src); ui.close(); }
    }
    ui.separator();
    if qalam_kit::widgets::menu_item(ui, "equation", "Insert New Equation") { open(&app.ctx, Dlg::Equation { target: None, linear: String::new() }); ui.close(); }
}

// ---- Symbol --------------------------------------------------------------------------------------------

const SYMBOL_FONTS: &[&str] = &["(normal text)", "Noto Sans", "Noto Sans Math", "Noto Sans Symbols", "Noto Sans Symbols 2", "Noto Sans Arabic", "Noto Sans Hebrew"];
fn recent_id() -> egui::Id { egui::Id::new("jadwal-recent-symbols") }
pub fn recent_symbols(ctx: &egui::Context) -> Vec<char> { ctx.data(|d| d.get_temp::<Vec<char>>(recent_id())).unwrap_or_else(|| qalam_kit::symbols::COMMON.chars().take(16).collect()) }

/// Types a symbol into the cell being edited, or the active cell.
pub fn insert_symbol(app: &mut SheetApp, c: char) {
    crate::ribbon::insert_text(app, &c.to_string());
    let mut r = recent_symbols(&app.ctx);
    r.retain(|x| *x != c);
    r.insert(0, c);
    r.truncate(16);
    app.ctx.data_mut(|d| d.insert_temp(recent_id(), r));
}

fn symbol_dialog(app: &mut SheetApp, ctx: &egui::Context, tab: &mut u8, subset: &mut usize, font: &mut usize, selected: &mut Option<char>) -> bool {
    use qalam_kit::symbols::{SPECIAL, SUBSETS};
    let mut open = true;
    let mut ins: Option<char> = None;
    crate::theme::dialog(ctx, "Symbol").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.horizontal(|ui| {
            if ui.selectable_label(*tab == 0, tr("Symbols")).clicked() { *tab = 0; }
            if ui.selectable_label(*tab == 1, tr("Special Characters")).clicked() { *tab = 1; }
        });
        ui.separator();
        if *tab == 0 {
            ui.horizontal(|ui| {
                ui.label(tr("Font:"));
                crate::theme::Combo::from_id_salt("sym-font").width(150.0).selected_text(tr(SYMBOL_FONTS[*font])).show_ui(ui, |ui| for (k, f) in SYMBOL_FONTS.iter().enumerate() { if ui.selectable_label(*font == k, tr(f)).clicked() { *font = k; } });
                ui.label(tr("Subset:"));
                crate::theme::Combo::from_id_salt("sym-subset").width(170.0).selected_text(tr(SUBSETS[*subset].0)).show_ui(ui, |ui| for (k, s) in SUBSETS.iter().enumerate() { if ui.selectable_label(*subset == k, tr(s.0)).clicked() { *subset = k; } });
            });
            let (_, a, b) = SUBSETS[*subset];
            egui::ScrollArea::vertical().id_salt("sym-grid").max_height(250.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.set_max_width(430.0);
                    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                    for cp in a..=b {
                        let Some(c) = char::from_u32(cp) else { continue };
                        if sym_cell(ui, c, *selected == Some(c), *font).double_clicked() { ins = Some(c); }
                    }
                });
            });
            // Clicks select (handled in sym_cell via memory).
            if let Some(c) = ui.data(|d| d.get_temp::<char>(egui::Id::new("sym-click"))) { *selected = Some(c); ui.data_mut(|d| d.remove::<char>(egui::Id::new("sym-click"))); }
            ui.label(tr("Recently used symbols:"));
            ui.horizontal(|ui| { ui.spacing_mut().item_spacing = vec2(2.0, 2.0); for c in recent_symbols(ui.ctx()) { if sym_cell(ui, c, *selected == Some(c), *font).double_clicked() { ins = Some(c); } } });
            if let Some(c) = ui.data(|d| d.get_temp::<char>(egui::Id::new("sym-click"))) { *selected = Some(c); ui.data_mut(|d| d.remove::<char>(egui::Id::new("sym-click"))); }
            if let Some(c) = selected { ui.label(RichText::new(format!("U+{:04X}", *c as u32)).size(11.5).color(c_ink3())); }
        } else {
            egui::ScrollArea::vertical().id_salt("sym-special").max_height(300.0).show(ui, |ui| {
                for (c, n) in SPECIAL {
                    let on = *selected == Some(*c);
                    let r = ui.selectable_label(on, format!("{}    {}", if c.is_whitespace() || c.is_control() || matches!(*c, '\u{AD}' | '\u{200B}' | '\u{200C}') { ' ' } else { *c }, tr(n)));
                    if r.clicked() { *selected = Some(*c); }
                    if r.double_clicked() { ins = Some(*c); }
                }
            });
        }
        let go = crate::theme::footer(ui, |ui| crate::theme::primary_enabled(ui, selected.is_some(), "Insert").clicked());
        if go { ins = *selected; }
    });
    if let Some(c) = ins { insert_symbol(app, c); }
    open
}

fn sym_cell(ui: &mut Ui, c: char, on: bool, font: usize) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if ui.is_rect_visible(r) {
        ui.painter().rect(r, 2.0, if on { ACCENT.gamma_multiply(0.2) } else if resp.hovered() { c_hover() } else { c_card() }, Stroke::new(1.0, if on { ACCENT } else { c_line() }), StrokeKind::Inside);
        let _ = font;
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, c.to_string(), qalam_kit::symbols::glyph_font(c, 17.0), c_ink());
    }
    if resp.clicked() { ui.data_mut(|d| d.insert_temp(egui::Id::new("sym-click"), c)); }
    resp
}
