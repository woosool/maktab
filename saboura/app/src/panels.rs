//! Panels around the board: the frames list (`frames-panel.js`), comments (`comments-ui.js`),
//! find on board (`board-search.js`), the timer (`timer-ui.js`) and presenting frames.

use crate::app::{BoardApp, Side};
use crate::i18n::tr;
use egui::{Align2, Color32, Rect, RichText, Ui, vec2};
use qalam_kit::colors::*;
use saboura_engine::geometry::{self, Box2};
use saboura_engine::model::{Item, now_ms, uid};
use saboura_engine::View;
use serde_json::{Value, json};

/// Frames in reading order: top to bottom, then start to end.
pub fn frames(app: &BoardApp) -> Vec<Item> {
    let mut f: Vec<Item> = app.board.items.iter().filter(|i| i.kind == "frame").cloned().collect();
    f.sort_by(|a, b| ((a.y / 40.0).round(), a.x).partial_cmp(&((b.y / 40.0).round(), b.x)).unwrap_or(std::cmp::Ordering::Equal));
    f
}

/// Shows a part of the board in the view, with a margin.
pub fn go_to(app: &mut BoardApp, b: Box2) {
    let size = (app.canvas_rect.width() as f64, app.canvas_rect.height() as f64);
    app.view = View::fit(b, size, 60.0);
}

pub fn side_panel(app: &mut BoardApp, ui: &mut Ui) {
    if app.side == Side::None { return; }
    let panel = if crate::i18n::rtl() { egui::Panel::left("side") } else { egui::Panel::right("side") };
    panel.resizable(true).default_size(280.0).frame(egui::Frame::NONE.fill(c_card()).inner_margin(12).stroke(egui::Stroke::new(1.0, c_line()))).show(ui, |ui| {
        crate::theme::hrow(ui, |ui| {
            ui.label(RichText::new(tr(match app.side { Side::Frames => "Frames", Side::Votes => "Voting", _ => "Comments" })).size(15.0).strong());
            ui.with_layout(qalam_kit::widgets::far_layout(), |ui| { if ui.small_button("✕").clicked() { app.side = Side::None; } });
        });
        ui.separator();
        let align = if crate::i18n::rtl() { egui::Align::Max } else { egui::Align::Min };
        ui.with_layout(egui::Layout::top_down(align), |ui| match app.side { Side::Frames => frames_list(app, ui), Side::Comments => comments(app, ui), Side::Votes => votes_panel(app, ui), Side::None => {} });
    });
}

fn frames_list(app: &mut BoardApp, ui: &mut Ui) {
    let list = frames(app);
    if list.is_empty() { ui.weak(tr("Add a frame (F) to group a part of the board. Frames can be presented one by one.")); return; }
    if crate::theme::primary(ui, tr("Present")).clicked() { start_presenting(app); }
    ui.add_space(6.0);
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (n, f) in list.iter().enumerate() {
            let selected = app.is_selected(&f.id);
            let name = if f.text.is_empty() { format!("{} {}", tr("Frame"), n + 1) } else { f.text.clone() };
            let r = ui.add(egui::Button::new(format!("{}.  {}", n + 1, crate::i18n::shape(&name))).selected(selected).frame_when_inactive(false).min_size(vec2(ui.available_width(), 30.0)));
            if r.clicked() { app.selection = vec![f.id.clone()]; go_to(app, geometry::object_bounds(f)); }
            if r.double_clicked() { start_presenting_at(app, n); }
        }
    });
}

// ---- Comments ----

fn threads(app: &BoardApp) -> Vec<Value> { app.board.extra.get("comments").and_then(|c| c.as_array()).cloned().unwrap_or_default() }

fn set_threads(app: &mut BoardApp, t: Vec<Value>) {
    if t.is_empty() { app.board.extra.remove("comments"); } else { app.board.extra.insert("comments".into(), Value::Array(t)); }
    app.changed();
}

fn message(app: &BoardApp, text: &str) -> Value {
    json!({ "id": uid(), "authorId": format!("local:{}", app.prefs.author), "authorName": app.prefs.author, "text": text, "createdAt": now_ms() as i64 })
}

/// Starts a comment at a board point: its thread is made when the first message is sent.
pub fn new_comment(app: &mut BoardApp, at: (f64, f64)) {
    app.side = Side::Comments;
    app.reply = Some((format!("new:{}:{}", at.0, at.1), String::new()));
}

fn send(app: &mut BoardApp, id: &str, text: &str) {
    let text = text.trim();
    if text.is_empty() { return; }
    let mut t = threads(app);
    if let Some(pos) = id.strip_prefix("new:") {
        let mut it = pos.split(':').filter_map(|v| v.parse::<f64>().ok());
        let (x, y) = (it.next().unwrap_or(0.0), it.next().unwrap_or(0.0));
        let mut thread = json!({ "id": uid(), "x": x, "y": y, "resolved": false, "messages": [message(app, text)] });
        // A comment placed on an object moves with it.
        if let Some(o) = geometry::hit_test(&app.board.items, (x, y), 0.0).filter(|o| !o.is_connector()) {
            thread["anchorId"] = o.id.clone().into();
            thread["dx"] = (x - o.x).into();
            thread["dy"] = (y - o.y).into();
        }
        let new_id = thread["id"].as_str().unwrap_or_default().to_string();
        t.push(thread);
        app.reply = Some((new_id, String::new()));
    } else if let Some(th) = t.iter_mut().find(|th| th["id"].as_str() == Some(id)) {
        let m = message(app, text);
        if let Some(ms) = th["messages"].as_array_mut() { ms.push(m); }
        app.reply = Some((id.to_string(), String::new()));
    }
    set_threads(app, t);
}

fn comments(app: &mut BoardApp, ui: &mut Ui) {
    if ui.button(tr("Add a comment (C)")).clicked() { app.tool = crate::app::Tool::Comment; }
    ui.weak(tr("Click on the board to place it."));
    ui.add_space(6.0);
    // A new comment being written.
    if let Some((id, mut text)) = app.reply.clone() && id.starts_with("new:") {
        ui.group(|ui| {
            ui.label(RichText::new(tr("New comment")).strong());
            let r = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(f32::INFINITY).hint_text(tr("Write a comment")));
            r.request_focus();
            crate::rtl::row(ui, |ui| {
                if crate::theme::primary(ui, tr("Comment")).clicked() { send(app, &id, &text); text.clear(); }
                if crate::theme::secondary(ui, &tr("Cancel")).clicked() { app.reply = None; }
            });
        });
        if app.reply.as_ref().is_some_and(|r| r.0 == id) { app.reply = Some((id, text)); }
    }
    let list = threads(app);
    if list.is_empty() { ui.weak(tr("No comments yet.")); }
    let mut action: Option<(String, &str)> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for t in &list {
            let id = t["id"].as_str().unwrap_or_default().to_string();
            let resolved = t["resolved"].as_bool() == Some(true);
            let open = app.reply.as_ref().is_some_and(|r| r.0 == id);
            egui::Frame::group(ui.style()).fill(if open { ACCENT.gamma_multiply(0.08) } else { Color32::TRANSPARENT }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for m in t["messages"].as_array().into_iter().flatten() {
                    if m["deleted"].as_bool() == Some(true) { continue; }
                    ui.label(RichText::new(m["authorName"].as_str().unwrap_or("")).strong().size(12.0));
                    ui.label(crate::i18n::shape(m["text"].as_str().unwrap_or("")));
                }
                crate::rtl::row(ui, |ui| {
                    if ui.small_button(tr("Show")).clicked() { action = Some((id.clone(), "show")); }
                    if ui.small_button(tr(if resolved { "Reopen" } else { "Resolve" })).clicked() { action = Some((id.clone(), "resolve")); }
                    if ui.small_button(tr("Delete")).clicked() { action = Some((id.clone(), "delete")); }
                    if !open && ui.small_button(tr("Reply")).clicked() { action = Some((id.clone(), "reply")); }
                });
                if open && let Some((_, mut text)) = app.reply.clone() {
                    let r = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2).desired_width(f32::INFINITY).hint_text(tr("Reply")));
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
                    if crate::theme::primary(ui, tr("Send")).clicked() || enter { send(app, &id, &text); } else { app.reply = Some((id.clone(), text)); }
                }
            });
        }
    });
    if let Some((id, what)) = action {
        let mut t = threads(app);
        match what {
            "resolve" => { for th in t.iter_mut().filter(|th| th["id"].as_str() == Some(&id)) { let r = th["resolved"].as_bool() == Some(true); th["resolved"] = (!r).into(); } set_threads(app, t); }
            "delete" => { t.retain(|th| th["id"].as_str() != Some(&id)); set_threads(app, t); app.reply = None; }
            "reply" => app.reply = Some((id, String::new())),
            _ => {
                if let Some(p) = t.iter().find(|th| th["id"].as_str() == Some(&id)).and_then(|th| crate::canvas::pin_point(app, th)) {
                    let c = (app.canvas_rect.width() as f64 / 2.0, app.canvas_rect.height() as f64 / 2.0);
                    app.view.x = c.0 - p.0 * app.view.z;
                    app.view.y = c.1 - p.1 * app.view.z;
                }
                app.reply = Some((id, String::new()));
            }
        }
    }
}

// ---- Find, timer ----

/// Objects whose text (or table cells) contain the words, in board order.
pub fn search_matches(app: &BoardApp, q: &str) -> Vec<String> {
    let q = q.trim().to_lowercase();
    if q.is_empty() { return Vec::new(); }
    app.board.items.iter().filter(|i| i.text.to_lowercase().contains(&q) || i.cells().is_some_and(|c| c.iter().flatten().any(|t| t.to_lowercase().contains(&q))))
        .map(|i| i.id.clone()).collect()
}

/// The search bar and the timer, floating over the board.
pub fn overlays(app: &mut BoardApp, ui: &mut Ui) {
    let board = app.canvas_rect;
    if let Some((mut q, mut at)) = app.search.clone() {
        let mut close = false;
        egui::Area::new(egui::Id::unique("search")).pivot(Align2::CENTER_TOP).fixed_pos(board.center_top() + vec2(0.0, 8.0)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).corner_radius(10).inner_margin(6).show(ui, |ui| {
                crate::rtl::row(ui, |ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Find on board")).desired_width(220.0));
                    if r.changed() { at = 0; }
                    let found = search_matches(app, &q);
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if !r.lost_focus() || enter { r.request_focus(); }
                    ui.label(if found.is_empty() { tr(if q.trim().is_empty() { "" } else { "No results" }) } else { format!("{} / {}", at.min(found.len() - 1) + 1, found.len()) });
                    let prev = ui.small_button("↑").clicked();
                    let next = ui.small_button("↓").clicked() || enter;
                    if !found.is_empty() {
                        if next { at = (at + 1) % found.len(); }
                        if prev { at = (at + found.len() - 1) % found.len(); }
                        if r.changed() || next || prev {
                            let id = found[at.min(found.len() - 1)].clone();
                            if let Some(i) = app.board.find(&id) {
                                let b = geometry::object_bounds(i);
                                let c = (app.canvas_rect.width() as f64 / 2.0, app.canvas_rect.height() as f64 / 2.0);
                                if b.w * app.view.z > c.0 * 1.6 || b.w * app.view.z < 40.0 { go_to(app, b); }
                                else { app.view.x = c.0 - (b.x + b.w / 2.0) * app.view.z; app.view.y = c.1 - (b.y + b.h / 2.0) * app.view.z; }
                            }
                            app.selection = vec![id];
                        }
                    }
                    if ui.small_button("✕").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) { close = true; }
                });
            });
        });
        app.search = if close { None } else { Some((q, at)) };
    }
    if app.timer_shown || app.board.extra.get("timer").is_some_and(|t| t["state"] == "running") { timer(app, ui, board); }
}

const MS: i64 = 1000;

fn timer(app: &mut BoardApp, ui: &mut Ui, board: Rect) {
    let now = now_ms() as i64;
    let t = app.board.extra.get("timer").cloned().unwrap_or_else(|| json!({ "id": "timer", "state": "idle", "duration": 5 * 60 * MS, "remaining": 5 * 60 * MS, "endsAt": 0 }));
    let running = t["state"] == "running";
    let duration = t["duration"].as_i64().unwrap_or(300_000);
    let remaining = if running { (t["endsAt"].as_i64().unwrap_or(now) - now).max(0) } else { t["remaining"].as_i64().unwrap_or(duration) };
    if running && remaining == 0 {
        app.board.extra.insert("timer".into(), json!({ "id": "timer", "state": "idle", "duration": duration, "remaining": duration, "endsAt": 0 }));
        app.notify(&tr("Time's up!"));
        app.changed();
        return;
    }
    let mut set = None;
    let (at, pivot) = crate::rtl::corner(board, false, true, vec2(8.0, 8.0));
    egui::Area::new(egui::Id::unique("timer")).pivot(pivot).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).corner_radius(10).inner_margin(8).show(ui, |ui| {
            crate::rtl::row(ui, |ui| {
                let secs = (remaining + 999) / MS;
                ui.label(RichText::new(format!("{:02}:{:02}", secs / 60, secs % 60)).size(22.0).monospace().color(if running && secs <= 10 { Color32::from_rgb(0xd0, 0x3a, 0x3a) } else { c_ink() }));
                if running {
                    if ui.button(tr("Pause")).clicked() { set = Some(json!({ "id": "timer", "state": "paused", "duration": duration, "remaining": remaining, "endsAt": 0 })); }
                } else if ui.button(tr("Start")).clicked() { set = Some(json!({ "id": "timer", "state": "running", "duration": duration, "remaining": remaining, "endsAt": now + remaining })); }
                if ui.button(tr("Reset")).clicked() { set = Some(json!({ "id": "timer", "state": "idle", "duration": duration, "remaining": duration, "endsAt": 0 })); }
                if ui.small_button("✕").on_hover_text(tr("Close timer")).clicked() { app.timer_shown = false; set = Some(Value::Null); }
            });
            if !running {
                crate::rtl::row(ui, |ui| {
                    for m in [1, 3, 5, 10, 15, 30] {
                        if ui.selectable_label(duration == m * 60 * MS, format!("{m} {}", tr("min"))).clicked() { set = Some(json!({ "id": "timer", "state": "idle", "duration": m * 60 * MS, "remaining": m * 60 * MS, "endsAt": 0 })); }
                    }
                });
            }
        });
    });
    if let Some(v) = set { if v.is_null() { app.board.extra.remove("timer"); } else { app.board.extra.insert("timer".into(), v); } app.changed(); }
    if running { ui.ctx().request_repaint_after(std::time::Duration::from_millis(250)); }
}

// ---- Presenting ----

pub fn start_presenting(app: &mut BoardApp) { start_presenting_at(app, 0); }

pub fn start_presenting_at(app: &mut BoardApp, n: usize) {
    if frames(app).is_empty() { app.notify(&tr("Add frames to present them one by one.")); return; }
    app.stop_editing();
    app.presenting = Some(n);
    app.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
}

/// Frames one at a time, filling the screen: arrows or Space move, Escape ends.
pub fn present(app: &mut BoardApp, ui: &mut Ui) {
    let list = frames(app);
    let Some(mut n) = app.presenting else { return };
    let (next, prev, end) = ui.input(|i| (
        i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::PageDown),
        i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::PageUp),
        i.key_pressed(egui::Key::Escape)));
    if next { n += 1; }
    if prev { n = n.saturating_sub(1); }
    if end || n >= list.len() || list.is_empty() {
        app.presenting = None;
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        return;
    }
    app.presenting = Some(n);
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ui, |ui| {
        let rect = ui.max_rect();
        let resp = ui.allocate_rect(rect, egui::Sense::click());
        if resp.clicked() { app.presenting = Some(n + 1); }
        let f = &list[n];
        let v = View::fit(Box2 { x: f.x, y: f.y, w: f.w, h: f.h }, (rect.width() as f64, rect.height() as f64), 24.0);
        let xf = crate::canvas::Xf { v, origin: rect.min };
        let fr = Rect::from_two_pos(xf.s((f.x, f.y)), xf.s((f.x + f.w, f.y + f.h)));
        let painter = ui.painter_at(fr);
        painter.rect_filled(fr, 0.0, crate::canvas::board_bg(app.prefs.dark));
        let images: Vec<String> = app.board.items.iter().filter(|i| i.kind == "image").map(|i| i.id.clone()).collect();
        let tex: std::collections::HashMap<String, egui::TextureId> = images.into_iter().filter_map(|id| app.texture_of(&id).map(|t| (id, t.id()))).collect();
        for i in geometry::stacked(&app.board.items) {
            if i.id == f.id || !geometry::object_bounds(i).intersects(&Box2 { x: f.x, y: f.y, w: f.w, h: f.h }) { continue; }
            crate::canvas::draw_item(&painter, i, &xf, app.prefs.dark, false, tex.get(&i.id).copied(), None);
        }
        ui.painter().text(rect.right_bottom() - vec2(16.0, 12.0), Align2::RIGHT_BOTTOM, format!("{} / {}", n + 1, list.len()), egui::FontId::proportional(13.0), Color32::from_gray(150));
    });
}

// ---- Dot voting (`facilitation/model.js`: `workshop.vote` and a `v:` ballot per voter) ----

pub fn workshop(app: &BoardApp) -> serde_json::Map<String, Value> {
    app.board.extra.get("workshop").and_then(|w| w.as_object()).cloned().unwrap_or_default()
}
fn set_workshop(app: &mut BoardApp, w: serde_json::Map<String, Value>) {
    if w.is_empty() { app.board.extra.remove("workshop"); } else { app.board.extra.insert("workshop".into(), Value::Object(w)); }
    app.changed();
}
fn ballot_key(vote: &str, by: &str) -> String { format!("v:{}", serde_json::to_string(&json!([vote, by])).unwrap_or_default()) }

/// The vote under way or last held.
pub fn vote(app: &BoardApp) -> Option<Value> { workshop(app).get("vote").cloned() }
fn voter(app: &BoardApp) -> String { format!("local:{}", app.prefs.author) }

/// Votes per object: everyone's, and this voter's.
pub fn tally(app: &BoardApp) -> (std::collections::HashMap<String, usize>, std::collections::HashMap<String, usize>) {
    let (mut all, mut mine) = (std::collections::HashMap::new(), std::collections::HashMap::new());
    let Some(v) = vote(app) else { return (all, mine) };
    let id = v["id"].as_str().unwrap_or("");
    let me = voter(app);
    for (k, b) in workshop(app) {
        if !k.starts_with("v:") || b["vote"] != id { continue; }
        for t in b["ids"].as_array().into_iter().flatten().filter_map(|t| t.as_str()) {
            *all.entry(t.to_string()).or_insert(0) += 1;
            if b["by"] == me.as_str() { *mine.entry(t.to_string()).or_insert(0) += 1; }
        }
    }
    (all, mine)
}

/// A vote for (or, `remove`, taken back from) an object by the current voter.
pub fn cast(app: &mut BoardApp, target: &str, remove: bool) {
    let Some(v) = vote(app).filter(|v| v["state"] == "active") else { return };
    if !v["targets"].as_array().is_some_and(|t| t.iter().any(|t| t == target)) { return; }
    let (id, max, multiple) = (v["id"].as_str().unwrap_or("").to_string(), v["votes"].as_u64().unwrap_or(3) as usize, v["multiple"] == true);
    let me = voter(app);
    let key = ballot_key(&id, &me);
    let mut w = workshop(app);
    let mut ids: Vec<String> = w.get(&key).and_then(|b| serde_json::from_value(b["ids"].clone()).ok()).unwrap_or_default();
    if remove { if let Some(p) = ids.iter().rposition(|t| t == target) { ids.remove(p); } }
    else if ids.len() >= max { app.notify(&tr("You have used all your votes.")); return; }
    else if !multiple && ids.iter().any(|t| t == target) { ids.retain(|t| t != target); }
    else { ids.push(target.to_string()); }
    if ids.is_empty() { w.remove(&key); } else {
        w.insert(key, json!({ "vote": id, "by": me, "ids": ids, "at": now_ms() as i64, "name": app.prefs.author }));
    }
    set_workshop(app, w);
}

fn votes_panel(app: &mut BoardApp, ui: &mut Ui) {
    let current = vote(app);
    let active = current.as_ref().is_some_and(|v| v["state"] == "active");
    crate::rtl::row(ui, |ui| {
        ui.label(tr("Voter"));
        let mut name = app.prefs.author.clone();
        if ui.add(egui::TextEdit::singleline(&mut name).desired_width(150.0).char_limit(100)).changed() && !name.trim().is_empty() { app.prefs.author = name; app.prefs.save(); }
    });
    ui.weak(tr("Several people can vote on one computer: each types their name before voting."));
    ui.separator();
    if active {
        let v = current.clone().unwrap_or_default();
        let (_, mine) = tally(app);
        let used: usize = mine.values().sum();
        ui.label(RichText::new(v["title"].as_str().unwrap_or("")).strong().size(15.0));
        ui.label(crate::i18n::trf("{} of {} votes left", &[&(v["votes"].as_u64().unwrap_or(0) as usize).saturating_sub(used), &v["votes"].as_u64().unwrap_or(0)]));
        ui.weak(tr("Click an object to vote for it; click a dot to take the vote back."));
        if crate::theme::primary(ui, tr("End vote")).clicked() {
            let mut w = workshop(app);
            if let Some(v) = w.get_mut("vote") { v["state"] = "ended".into(); v["endedAt"] = (now_ms() as i64).into(); v["revealed"] = true.into(); }
            set_workshop(app, w);
        }
    } else {
        let id = egui::Id::unique("vote-form");
        let (mut title, mut votes): (String, u32) = ui.ctx().data_mut(|d| d.get_temp(id)).unwrap_or_else(|| (tr("Which ideas should we pursue?"), 3));
        crate::theme::section(ui, &tr("New vote"));
        ui.add(egui::TextEdit::singleline(&mut title).desired_width(f32::INFINITY).char_limit(200));
        ui.add(egui::Slider::new(&mut votes, 1..=20).text(tr("votes each")));
        let targets: Vec<String> = if app.selection.is_empty() {
            app.board.items.iter().filter(|i| matches!(i.kind.as_str(), "note" | "rect" | "ellipse" | "text" | "image" | "table")).map(|i| i.id.clone()).collect()
        } else { app.selection.clone() };
        ui.weak(crate::i18n::trf("On {} objects (the selection, or every note and shape).", &[&targets.len()]));
        if ui.add_enabled(!targets.is_empty() && !title.trim().is_empty(), egui::Button::new(tr("Start vote"))).clicked() {
            let mut w = workshop(app);
            w.retain(|k, _| !k.starts_with("v:"));
            w.insert("vote".into(), json!({ "id": format!("vote-{}", &uid()[..8]), "title": title.trim(), "votes": votes, "multiple": true, "anonymous": false, "hidden": false,
                "targets": targets, "state": "active", "revealed": true, "startedAt": now_ms() as i64, "endsAt": now_ms() as i64 + 24 * 3600 * 1000 }));
            set_workshop(app, w);
            app.selection.clear();
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, (title, votes)));
    }
    // Results, most votes first.
    if let Some(v) = current {
        let (all, _) = tally(app);
        if !all.is_empty() {
            crate::theme::section(ui, &tr(if active { "Votes so far" } else { "Results" }));
            ui.weak(v["title"].as_str().unwrap_or(""));
            let mut rows: Vec<(String, usize)> = all.into_iter().collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1));
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (id, n) in rows {
                    let Some(i) = app.board.find(&id).cloned() else { continue };
                    let name = if i.text.is_empty() { tr(&i.kind) } else { i.text.lines().next().unwrap_or("").chars().take(40).collect() };
                    if ui.add(egui::Button::new(format!("{n}  ·  {}", crate::i18n::shape(&name))).frame_when_inactive(false).min_size(vec2(ui.available_width(), 26.0))).clicked() {
                        app.selection = vec![id.clone()];
                        go_to(app, geometry::object_bounds(&i));
                    }
                }
            });
        }
        if !active && ui.button(tr("Clear vote")).clicked() { set_workshop(app, serde_json::Map::new()); }
    }
}
