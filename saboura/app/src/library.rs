//! The template library (`templates.js`, `template-library/`) and the icon and sticker
//! library (`icon-library.js`), generated from the JS builders by `tools/gen-libraries.mjs`.

use crate::app::{BoardApp, Panel, Pending};
use crate::i18n::tr;
use egui::{Color32, Rect, Sense, Ui, vec2};
use qalam_kit::colors::*;
use saboura_engine::model::Item;
use saboura_engine::{Board, geometry};
use serde_json::Value;
use std::sync::OnceLock;

fn templates() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("../../assets/library/templates.json")).unwrap_or_default())
}
fn icons() -> &'static Value {
    static I: OnceLock<Value> = OnceLock::new();
    I.get_or_init(|| serde_json::from_str(include_str!("../../assets/library/icons.json")).unwrap_or_default())
}

/// English or French, the two languages the libraries are written in.
fn lang_index() -> usize { if crate::i18n::lang() == crate::i18n::Lang::Fr { 1 } else { 0 } }
fn pick(v: &Value) -> String { v[lang_index()].as_str().or(v[0].as_str()).unwrap_or("").to_string() }

/// Whether every word of the query is in the entry's name, description or keywords.
fn matches(q: &str, fields: &[&Value]) -> bool {
    let hay: String = fields.iter().flat_map(|f| f.as_array().cloned().unwrap_or_else(|| vec![(*f).clone()])).filter_map(|v| v.as_str().map(str::to_lowercase)).collect::<Vec<_>>().join(" ");
    q.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}

/// A template's board in the interface language, with fresh IDs.
pub fn template_board(id: &str) -> Option<Board> {
    let t = templates()["templates"].as_array()?.iter().find(|t| t["id"] == id)?;
    let b = &t["boards"][if lang_index() == 1 { "fr" } else { "en" }];
    let mut board: Board = serde_json::from_value(b.clone()).ok()?;
    board.id = saboura_engine::model::uid();
    board.update_connectors();
    Some(board)
}

pub fn template_dialog(app: &mut BoardApp, ui: &mut Ui, query: String, category: String) {
    let mut open = true;
    let (mut q, mut cat) = (query, category);
    let mut chosen: Option<(String, bool)> = None;
    crate::theme::dialog(ui.ctx(), tr("Templates")).open(&mut open).default_width(760.0).show(ui.ctx(), |ui| {
        // A width that fits the window, three cards across at most.
        let w = (ui.ctx().content_rect().width() - 120.0).clamp(300.0, 760.0);
        ui.set_width(w);
        crate::theme::hrow(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Search templates")).desired_width(w.min(320.0)));
        });
        ui.add_space(4.0);
        // Categories wrap onto more lines rather than widening the dialog.
        ui.allocate_ui_with_layout(vec2(w, 0.0), card_layout(), |ui| {
            ui.set_max_width(w);
            ui.selectable_value(&mut cat, String::new(), tr("All"));
            for c in templates()["categories"].as_array().into_iter().flatten() {
                let id = c["id"].as_str().unwrap_or("").to_string();
                ui.selectable_value(&mut cat, id, pick(&c["name"]));
            }
        });
        ui.separator();
        let h = (ui.ctx().content_rect().height() - 300.0).clamp(220.0, 540.0);
        egui::ScrollArea::vertical().max_height(h).min_scrolled_height(h).auto_shrink([false, false]).show(ui, |ui| {
            ui.allocate_ui_with_layout(vec2(w, 0.0), card_layout().with_cross_align(egui::Align::Min), |ui| {
                ui.set_max_width(w);
                ui.spacing_mut().item_spacing = vec2(12.0, 14.0);
                let list: Vec<&Value> = templates()["templates"].as_array().into_iter().flatten()
                    .filter(|t| (cat.is_empty() || t["category"] == cat.as_str()) && matches(&q, &[&t["name"], &t["description"], &t["keywords"]])).collect();
                if list.is_empty() { ui.weak(tr("No templates match.")); }
                let card = ((w - 24.0) / 3.0).floor().clamp(200.0, 236.0);
                for t in list {
                    let id = t["id"].as_str().unwrap_or("").to_string();
                    ui.allocate_ui_with_layout(vec2(card, 236.0), crate::rtl_column(), |ui| {
                        ui.set_width(card);
                        let (r, resp) = ui.allocate_exact_size(vec2(card, 120.0), Sense::click());
                        thumbnail(ui, &t["boards"]["en"]["items"], r, resp.hovered());
                        if resp.on_hover_text(pick(&t["description"])).clicked() { chosen = Some((id.clone(), false)); }
                        ui.label(egui::RichText::new(pick(&t["name"])).strong());
                        ui.add(egui::Label::new(egui::RichText::new(pick(&t["description"])).small().color(c_ink3())).truncate());
                        crate::theme::hrow(ui, |ui| {
                            if ui.small_button(tr("New board")).clicked() { chosen = Some((id.clone(), false)); }
                            if ui.small_button(tr("Add to this board")).clicked() { chosen = Some((id.clone(), true)); }
                        });
                    });
                }
            });
        });
    });
    match chosen {
        Some((id, add)) => {
            app.panel = Panel::None;
            let Some(board) = template_board(&id) else { return };
            if add { add_items(app, board.items); } else { app.guard(Pending::Template(Box::new(board))); }
        }
        None => app.panel = if open { Panel::Templates(q, cat) } else { Panel::None },
    }
}

/// Places objects to the right of the board's content, as a new selection.
pub fn add_items(app: &mut BoardApp, mut items: Vec<Item>) {
    if items.is_empty() { return; }
    let here = geometry::bounds(&app.board.items);
    let b = geometry::bounds(&items);
    let (dx, dy) = if app.board.items.is_empty() { (-b.x, -b.y) } else { (here.x + here.w + 200.0 - b.x, here.y - b.y) };
    // New IDs, links between the objects kept.
    let ids: std::collections::HashMap<String, String> = items.iter().map(|i| (i.id.clone(), saboura_engine::model::uid())).collect();
    for i in items.iter_mut() {
        i.id = ids[&i.id].clone();
        i.x += dx; i.y += dy;
        i.start_id = i.start_id.as_ref().and_then(|s| ids.get(s).cloned());
        i.end_id = i.end_id.as_ref().and_then(|s| ids.get(s).cloned());
        if let Some(p) = i.mind_parent().and_then(|p| ids.get(p)).cloned() { let mut m = i.get("mindMap").cloned().unwrap_or_default(); m["parent"] = p.into(); i.set("mindMap", m); }
        if let Some(p) = i.str("parentId").and_then(|p| ids.get(p)).cloned() { i.set("parentId", p); }
    }
    app.checkpoint();
    app.selection = items.iter().map(|i| i.id.clone()).collect();
    let nb = geometry::bounds(&items);
    app.board.items.extend(items);
    app.board.update_connectors();
    app.changed();
    crate::panels::go_to(app, nb);
}

/// A template's objects as coloured blocks.
fn thumbnail(ui: &Ui, items: &Value, r: Rect, hot: bool) {
    let p = ui.painter_at(r);
    p.rect_filled(r, 8.0, if hot { c_hover() } else { c_card() });
    p.rect_stroke(r, 8.0, egui::Stroke::new(1.0, if hot { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
    // Each template's objects are read once, not on every frame.
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<usize, std::sync::Arc<Vec<Item>>>>> = std::sync::Mutex::new(None);
    let key = items as *const Value as usize;
    let list = {
        let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        c.get_or_insert_with(Default::default).entry(key).or_insert_with(|| std::sync::Arc::new(items.as_array().into_iter().flatten().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())).clone()
    };
    if list.is_empty() { return; }
    let b = geometry::bounds(list.iter());
    let s = ((r.width() - 16.0) as f64 / b.w.max(1.0)).min((r.height() - 16.0) as f64 / b.h.max(1.0));
    let o = r.center() - vec2((b.w * s) as f32 / 2.0, (b.h * s) as f32 / 2.0);
    let to = |x: f64, y: f64| o + vec2(((x - b.x) * s) as f32, ((y - b.y) * s) as f32);
    for i in geometry::stacked(&list) {
        let rr = Rect::from_min_max(to(i.x, i.y), to(i.x + i.w, i.y + i.h));
        match i.kind.as_str() {
            "frame" => { p.rect_stroke(rr, 2.0, egui::Stroke::new(0.8, c_line()), egui::StrokeKind::Inside); }
            "arrow" | "pen" => { p.line_segment([rr.min, rr.max], egui::Stroke::new(0.8, c_ink3())); }
            "text" => { p.rect_filled(Rect::from_min_size(rr.min, vec2(rr.width().min(60.0), 2.0)), 1.0, c_ink3()); }
            _ => { p.rect_filled(rr, 1.5, crate::canvas::hex(i.color.as_deref(), Color32::from_gray(0xdd))); }
        }
    }
}

// ---- Icons and stickers ----

/// An SVG of a lucide outline in a colour; a sticker on its round badge.
fn icon_svg(entry: &Value, color: &str, sticker: bool) -> String {
    let body = entry["svg"].as_str().unwrap_or("");
    if sticker {
        let glyph = entry["glyph"].as_str().unwrap_or("#ffffff");
        let inner = if body.is_empty() {
            format!(r#"<text x="12" y="16.2" text-anchor="middle" font-family="Noto Sans" font-weight="700" font-size="10" fill="{glyph}">{}</text>"#, entry["label"].as_str().unwrap_or(""))
        } else {
            format!(r#"<g transform="translate(5.4 5.4) scale(0.55)" fill="none" stroke="{glyph}" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round">{body}</g>"#)
        };
        return format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="11.5" fill="{}"/>{inner}</svg>"#, entry["background"].as_str().unwrap_or("#888888"));
    }
    // A thin light outline keeps dark icons visible on the dark board, as in the JS.
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-1 -1 26 26" fill="none" stroke-linecap="round" stroke-linejoin="round"><g stroke="#f3f4f6" stroke-width="3">{body}</g><g stroke="{color}" stroke-width="2">{body}</g></svg>"##)
}

fn render_png(svg: &str, size: u32) -> Option<Vec<u8>> {
    let img = crate::export::rasterize(svg, 1.0).ok()?;
    let img = image::imageops::resize(&img, size, size, image::imageops::FilterType::Lanczos3);
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

fn icon_texture(app: &mut BoardApp, key: &str, svg: &str) -> Option<egui::TextureHandle> {
    let k = format!("icon:{key}");
    if let Some((_, t)) = app.textures.get(&k) { return t.clone(); }
    // Drawn large enough for the grid: 26 board units at 4×.
    let tex = crate::export::rasterize(svg, 4.0).ok().map(|img| {
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img);
        app.ctx.load_texture(k.clone(), ci, egui::TextureOptions::LINEAR)
    });
    app.textures.insert(k, (0, tex.clone()));
    tex
}

const ICON_COLORS: [&str; 6] = ["#30342f", "#2f6fd6", "#26894a", "#c86b0a", "#d03a3a", "#8a5bd6"];

pub fn icon_dialog(app: &mut BoardApp, ui: &mut Ui, query: String, category: String, color: usize) {
    let mut open = true;
    let (mut q, mut cat, mut col) = (query, category, color);
    let mut chosen: Option<(Value, bool)> = None;
    crate::theme::dialog(ui.ctx(), tr("Icons and stickers")).open(&mut open).default_width(620.0).show(ui.ctx(), |ui| {
        crate::rtl::row(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Search icons")).desired_width(220.0));
            for (n, c) in ICON_COLORS.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                ui.painter().circle(r.center(), 8.0, crate::canvas::hex(Some(c), Color32::GRAY), egui::Stroke::new(if n == col { 2.0 } else { 0.0 }, ACCENT));
                if resp.clicked() { col = n; }
            }
        });
        crate::rtl::wrap(ui, |ui| {
            ui.selectable_value(&mut cat, String::new(), tr("All"));
            ui.selectable_value(&mut cat, "stickers".to_string(), tr("Stickers"));
            for c in icons()["categories"].as_array().into_iter().flatten() {
                ui.selectable_value(&mut cat, c["id"].as_str().unwrap_or("").to_string(), pick(&c["name"]));
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            crate::rtl::wrap(ui, |ui| {
                let stickers = icons()["stickers"].as_array().cloned().unwrap_or_default();
                let list = icons()["icons"].as_array().cloned().unwrap_or_default();
                let mut show: Vec<(Value, bool)> = Vec::new();
                if cat.is_empty() || cat == "stickers" { show.extend(stickers.into_iter().filter(|s| matches(&q, &[&s["name"], &s["keywords"]])).map(|s| (s, true))); }
                if cat != "stickers" { show.extend(list.into_iter().filter(|i| (cat.is_empty() || i["category"] == cat.as_str()) && matches(&q, &[&i["name"], &i["keywords"]])).map(|i| (i, false))); }
                for (entry, sticker) in show {
                    let key = format!("{}{}", entry["id"].as_str().unwrap_or(""), if sticker { String::new() } else { format!("-{col}") });
                    let svg = icon_svg(&entry, ICON_COLORS[col], sticker);
                    let (r, resp) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::click());
                    if resp.hovered() { ui.painter().rect_filled(r, 8.0, c_hover()); }
                    if let Some(t) = icon_texture(app, &key, &svg) { ui.painter().image(t.id(), r.shrink(10.0), Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE); }
                    if resp.on_hover_text(pick(&entry["name"])).clicked() { chosen = Some((entry, sticker)); }
                }
            });
        });
    });
    if let Some((entry, sticker)) = chosen {
        let name = pick(&entry["name"]);
        if let Some(png) = render_png(&icon_svg(&entry, ICON_COLORS[col], sticker), 192) {
            let c = app.view_center();
            app.insert_image(&png, c);
            let size = if sticker { 64.0 } else { 96.0 };
            app.edit_selected(|i| { i.x += i.w / 2.0 - size / 2.0; i.y += i.h / 2.0 - size / 2.0; i.w = size; i.h = size; i.text = name.clone(); i.set("alt", name.clone()); });
        }
    }
    app.panel = if open { Panel::Icons(q, cat, col) } else { Panel::None };
}

/// Cards and chips flow from the reading side and wrap onto more lines.
fn card_layout() -> egui::Layout {
    if crate::i18n::rtl() { egui::Layout::right_to_left(egui::Align::Min).with_main_wrap(true) } else { egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true) }
}
