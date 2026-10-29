//! The slide show (show/session.js): full screen, one slide at a time, in the order the Set Up
//! Slide Show settings give (all, a range, or a custom show; hidden slides skipped); next and
//! previous by keys, click or wheel; B/W blank the screen; a slide number then Enter jumps;
//! Escape ends. Animations play per click, transitions and timings run, and the pen,
//! highlighter, laser and eraser draw over the slide. Presenter View puts the audience's
//! slide in its own full-screen window and shows the speaker the next slide, notes and timer.
//! Rehearse Timings and Record keep each slide's time; Kiosk mode loops and ignores clicks;
//! Browsed individually runs in a window.

use crate::app::SlidesApp;
use crate::i18n::tr;
use egui::{pos2, vec2, Color32, Painter, Pos2, Rect};
use slides_engine::animation::Anim;
use slides_engine::render::{self, Options};
use slides_engine::show_setup::{ShowSetup, ShowType};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool { Arrow, Pen, Highlighter, Laser, Eraser }

#[derive(Clone)]
pub struct Ink { pub slide: usize, pub pts: Vec<Pos2>, pub color: Color32, pub width: f32, pub highlighter: bool }

pub struct Rehearse {
    /// Seconds spent on each slide (by index), and the running slide's start.
    pub times: std::collections::HashMap<usize, f64>,
    pub slide_start: f64,
    pub paused: Option<f64>,
    pub record: bool,
}

pub struct Show {
    pub order: Vec<usize>,
    pub pos: usize,
    blank: Option<Color32>,
    typed: String,
    textures: std::collections::HashMap<(usize, u32), egui::TextureHandle>,
    /// When the current slide appeared (for the transition) and the one before it.
    since: f64,
    previous: Option<usize>,
    started: f64,
    /// The slide whose sound has played.
    sounded: Option<usize>,
    /// Click steps of the current slide's animations played so far, and when the last began.
    pub step: usize,
    step_since: f64,
    /// Triggered sequences started: (trigger shape id, when).
    triggered: Vec<(u32, f64)>,
    layers: Option<(usize, u32, crate::anim_play::Layers)>,
    morph: Option<(usize, usize, u32, crate::morph::Morph)>,
    pub setup: ShowSetup,
    pub tool: Tool,
    pub ink: Vec<Ink>,
    drawing: Option<Ink>,
    pub rehearse: Option<Rehearse>,
    /// Where the slide was drawn last frame (clicks map through it).
    slide_rect: Rect,
    /// Presenter View: its timer can pause, and the notes' size.
    timer_paused: Option<f64>,
    timer_offset: f64,
    notes_size: f32,
    /// See All Slides (a grid to jump from).
    grid: bool,
    /// Video or audio playing in the slide.
    pub player: Option<crate::media::Player>,
}

impl Show {
    pub fn new(app: &SlidesApp, from: usize, order: Option<Vec<usize>>, rehearse: Option<bool>) -> Show {
        let setup = app.pres.show_setup();
        let mut order = order.unwrap_or_else(|| app.pres.show_order(&setup));
        // Starting from a slide the range leaves out (or a hidden one) still shows it.
        if !order.contains(&from) && from < app.slides().len() && from > 0 { order.push(from); order.sort(); }
        let pos = order.iter().position(|&i| i >= from).unwrap_or(0);
        let now = app.ctx.input(|i| i.time);
        Show {
            order, pos, blank: None, typed: String::new(), textures: Default::default(), since: now, previous: None, started: now, sounded: None, step: 0, step_since: now, triggered: vec![], layers: None, morph: None,
            setup, tool: Tool::Arrow, ink: vec![], drawing: None,
            rehearse: rehearse.map(|record| Rehearse { times: Default::default(), slide_start: now, paused: None, record }),
            slide_rect: Rect::NOTHING, timer_paused: None, timer_offset: 0.0, notes_size: 16.0, grid: false, player: None,
        }
    }
    fn go(&mut self, pos: usize, now: f64) {
        if pos == self.pos || pos >= self.order.len() { return; }
        if let Some(r) = &mut self.rehearse {
            let spent = now - r.slide_start - r.paused.map(|p| now - p).unwrap_or(0.0);
            *r.times.entry(self.order[self.pos]).or_default() += spent.max(0.0);
            r.slide_start = now;
            r.paused = None;
        }
        self.previous = Some(self.order[self.pos]);
        self.player = None;
        self.pos = pos;
        self.since = now;
        self.step = 0;
        self.triggered.clear();
    }
    /// Next: the slide's next animation step if any remain, else the next slide (or the
    /// first again when looping). False at the end.
    fn next(&mut self, steps: usize, now: f64) -> bool {
        if self.step < steps && self.setup.animation { self.step += 1; self.step_since = now; return true; }
        if self.pos + 1 >= self.order.len() {
            if self.setup.loop_until_esc && self.rehearse.is_none() && self.order.len() > 1 { self.go(0, now); return true; }
            return false;
        }
        let p = self.pos + 1;
        self.go(p, now);
        true
    }
    fn prev(&mut self, now: f64) { self.blank = None; let p = self.pos.saturating_sub(1); self.go(p, now); }
    /// Close the rehearsal clock (the last slide's time).
    fn finish(&mut self, now: f64) {
        if let Some(r) = &mut self.rehearse {
            let spent = now - r.slide_start - r.paused.map(|p| now - p).unwrap_or(0.0);
            *r.times.entry(self.order[self.pos]).or_default() += spent.max(0.0);
        }
    }
    fn elapsed(&self, now: f64) -> f64 { self.timer_paused.unwrap_or(now) - self.started - self.timer_offset }
}

fn texture(show: &mut Show, app: &mut SlidesApp, ctx: &egui::Context, index: usize, width: u32, entrances_hidden: bool) -> Option<egui::TextureHandle> {
    let key = (index, width * 2 + entrances_hidden as u32);
    if let Some(t) = show.textures.get(&key) { return Some(t.clone()); }
    let part = app.slides().get(index)?.clone();
    let scene = app.pres.scene(&part);
    let opts = Options { scale: width as f64 / scene.w, prompts: false, skip_text_of: None, translate: None };
    // Objects that enter by an animation aren't there yet when the slide arrives.
    let anims = if entrances_hidden && show.setup.animation { app.pres.animations(&part) } else { vec![] };
    let mut hidden: Vec<usize> = vec![];
    let mut seen = vec![];
    for a in &anims {
        if seen.contains(&a.shape) { continue; }
        seen.push(a.shape);
        if a.class() == slides_engine::animation::Class::Entrance { if let Some(l) = app.pres.shape_by_id(&part, a.shape) { hidden.push(l.path[0]); } }
    }
    let pm = if hidden.is_empty() { render::render(&app.pres, &scene, &mut app.images, opts)? } else { render::render_layers(&app.pres, &scene, &mut app.images, opts, &hidden)?.0 };
    let img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
    let t = ctx.load_texture(format!("show-{index}-{}", key.1), img, egui::TextureOptions::LINEAR);
    show.textures.insert(key, t.clone());
    Some(t)
}

fn fit(screen: Rect, app: &SlidesApp) -> Rect {
    let (sw, sh) = app.pres.size();
    let k = (screen.width() / sw as f32).min(screen.height() / sh as f32);
    Rect::from_center_size(screen.center(), vec2(sw as f32 * k, sh as f32 * k))
}

/// The current slide as the audience sees it, in `r`: transition, animations, ink, laser.
fn paint_slide(show: &mut Show, app: &mut SlidesApp, ctx: &egui::Context, p: &Painter, r: Rect, now: f64, anims: &[Anim], laser: Option<Pos2>) {
    let px = (r.width() * ctx.pixels_per_point()).round().max(1.0) as u32;
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let index = show.order[show.pos];
    let part = app.slides().get(index).cloned().unwrap_or_default();
    let tr = app.pres.slide_transition(&part);
    let dur = tr.duration_ms.max(1) as f64 / 1000.0;
    let t = if tr.effect == "none" { 1.0 } else { ((now - show.since) / dur).clamp(0.0, 1.0) as f32 };
    let from = show.previous.and_then(|prev| texture(show, app, ctx, prev, px, false));
    let p = p.with_clip_rect(r);
    if let Some(to) = texture(show, app, ctx, index, px, true) {
        if t < 1.0 && tr.effect == "morph" && show.previous.is_some() {
            let prev = show.previous.unwrap();
            if show.morph.as_ref().is_none_or(|m| (m.0, m.1, m.2) != (prev, index, px)) {
                let slides = app.slides();
                show.morph = match (slides.get(prev), slides.get(index)) { (Some(a), Some(b)) => crate::morph::build(app, ctx, &a.clone(), &b.clone(), px).map(|m| (prev, index, px, m)), _ => None };
            }
            if let Some((_, _, _, m)) = &show.morph { crate::morph::draw(&p, m, t, r); }
            ctx.request_repaint();
        }
        else if t < 1.0 { crate::transition_fx::draw(&p, &tr.effect, &format!("{}{}", tr.option, if tr.effect == "random" { index.to_string() } else { String::new() }), t, r, from.as_ref().map(|f| f.id()), to.id()); ctx.request_repaint(); }
        else if anims.is_empty() || !show.setup.animation { p.image(to.id(), r, uv, Color32::WHITE); }
        else {
            if show.layers.as_ref().is_none_or(|l| l.0 != index || l.1 != px) { show.layers = crate::anim_play::build(app, ctx, &part, px).map(|l| (index, px, l)); }
            let (sched, _) = slides_engine::animation::schedule(anims);
            let arrived = show.since + dur;
            let (step, step_since) = (show.step, show.step_since);
            let trig = show.triggered.clone();
            let el = |i: usize| -> Option<f64> {
                let a = anims.get(i)?;
                if let Some(id) = a.trigger {
                    // Triggered effects run one after another in their own little sequence.
                    let started = trig.iter().find(|t| t.0 == id)?.1;
                    let before: u32 = anims.iter().take(i).filter(|b| b.trigger == Some(id) && a.start == slides_engine::animation::Start::AfterPrevious).map(|b| b.delay_ms + b.duration_ms).sum();
                    let e = (now - started) * 1000.0 - (before + a.delay_ms) as f64;
                    return (e >= 0.0).then_some(e);
                }
                let t = sched.iter().find(|t| t.index == i)?;
                if t.step < step { return Some(1e12); }
                if t.step > step { return None; }
                let base = if t.step == 0 { arrived } else { step_since };
                let e = (now - base) * 1000.0 - t.at_ms as f64;
                (e >= 0.0).then_some(e)
            };
            if let Some((_, _, l)) = &show.layers { crate::anim_play::paint(&p, r, l, &el); }
            ctx.request_repaint();
        }
    }
    // Ink on this slide (stored in slide fractions).
    let to_screen = |q: Pos2| pos2(r.min.x + q.x * r.width(), r.min.y + q.y * r.height());
    for s in show.ink.iter().filter(|s| s.slide == index).chain(show.drawing.iter()) {
        let pts: Vec<Pos2> = s.pts.iter().map(|q| to_screen(*q)).collect();
        let w = s.width * r.width() / 1000.0;
        if pts.len() == 1 { p.circle_filled(pts[0], w / 2.0, s.color); }
        else { p.add(egui::Shape::line(pts, egui::Stroke::new(w, s.color))); }
    }
    if let Some(l) = laser {
        if r.contains(l) {
            let c = hex_color(&show.setup.laser_color);
            p.circle_filled(l, 9.0, c.gamma_multiply(0.35));
            p.circle_filled(l, 5.0, c);
            p.circle_filled(l, 2.0, Color32::from_white_alpha(200));
        }
    }
    if let Some(c) = show.blank { p.rect_filled(r, 0.0, c); }
}

pub fn hex_color(h: &str) -> Color32 {
    let v = u32::from_str_radix(h.trim_start_matches('#'), 16).unwrap_or(0xFF0000);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// What ended the show: nothing, or the user (Escape, End Show, past the last slide).
fn end_show(app: &mut SlidesApp, mut show: Show, ctx: &egui::Context, now: f64) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    if let Some(r) = app.before_show.take() { ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(r.min)); }
    show.finish(now);
    if let Some(&i) = show.order.get(show.pos) { app.go_to(i); }
    let slides = app.slides();
    if let Some(r) = show.rehearse.take() {
        let total: f64 = r.times.values().sum();
        let times: Vec<(String, u32)> = r.times.iter().filter_map(|(i, t)| slides.get(*i).map(|s| (s.clone(), (t * 1000.0).round() as u32))).collect();
        app.view_dialog = Some(crate::views::ViewDialog::KeepTimings { times, total });
    } else if !show.ink.is_empty() {
        let (sw, sh) = app.pres.size();
        let strokes = show.ink.iter().filter_map(|s| {
            let part = slides.get(s.slide)?.clone();
            let pts = s.pts.iter().map(|q| (q.x as f64 * sw, q.y as f64 * sh)).collect();
            Some((part, pts, format!("{:02X}{:02X}{:02X}", s.color.r(), s.color.g(), s.color.b()), (s.width as f64 / 1000.0 * sw / 12_700.0), if s.highlighter { 0.4 } else { 1.0 }))
        }).collect();
        app.view_dialog = Some(crate::views::ViewDialog::KeepInk { strokes });
    }
}

pub fn frame(app: &mut SlidesApp, ui: &mut egui::Ui) {
    let Some(mut show) = app.show.take() else { return };
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let mut end = false;
    let cur_part = app.slides().get(show.order.get(show.pos).copied().unwrap_or(0)).cloned().unwrap_or_default();
    let anims = app.pres.animations(&cur_part);
    let steps = slides_engine::animation::schedule(&anims).1;
    let kiosk = show.setup.kind == ShowType::Kiosk;
    let presenter = show.setup.presenter_view;
    let mut clicked_at: Option<Pos2> = None;
    let drawing_tool = matches!(show.tool, Tool::Pen | Tool::Highlighter | Tool::Eraser);
    ctx.input(|i| {
        let cmd = i.modifiers.command;
        for ev in &i.events {
            match ev {
                egui::Event::Key { key, pressed: true, .. } if cmd => match key {
                    egui::Key::P => show.tool = if show.tool == Tool::Pen { Tool::Arrow } else { Tool::Pen },
                    egui::Key::L => show.tool = if show.tool == Tool::Laser { Tool::Arrow } else { Tool::Laser },
                    egui::Key::I => show.tool = if show.tool == Tool::Highlighter { Tool::Arrow } else { Tool::Highlighter },
                    egui::Key::E => show.tool = if show.tool == Tool::Eraser { Tool::Arrow } else { Tool::Eraser },
                    egui::Key::A => show.tool = Tool::Arrow,
                    _ => {}
                },
                egui::Event::Key { key, pressed: true, .. } => match key {
                    egui::Key::Escape => { if show.grid { show.grid = false; } else if show.tool != Tool::Arrow { show.tool = Tool::Arrow; } else { end = true; } }
                    egui::Key::ArrowRight | egui::Key::ArrowDown | egui::Key::Space | egui::Key::PageDown | egui::Key::N if !kiosk => {
                        if show.blank.take().is_none() && !show.next(steps, now) { end = true; }
                    }
                    egui::Key::Enter if !kiosk => {
                        if let Ok(n) = show.typed.parse::<usize>() {
                            if let Some(p) = show.order.iter().position(|&s| s + 1 == n) { show.go(p, now); }
                            show.typed.clear();
                        } else if !show.next(steps, now) { end = true; }
                    }
                    egui::Key::ArrowLeft | egui::Key::ArrowUp | egui::Key::Backspace | egui::Key::PageUp | egui::Key::P if !kiosk => show.prev(now),
                    egui::Key::Home if !kiosk => show.go(0, now),
                    egui::Key::End if !kiosk => { let p = show.order.len().saturating_sub(1); show.go(p, now); }
                    egui::Key::B => show.blank = if show.blank == Some(Color32::BLACK) { None } else { Some(Color32::BLACK) },
                    egui::Key::W => show.blank = if show.blank == Some(Color32::WHITE) { None } else { Some(Color32::WHITE) },
                    // E erases the slide's ink.
                    egui::Key::E => { let s = show.order[show.pos]; show.ink.retain(|k| k.slide != s); }
                    egui::Key::G => show.grid = !show.grid,
                    _ => {}
                },
                egui::Event::Text(t) if t.chars().all(|c| c.is_ascii_digit()) => show.typed.push_str(t),
                egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed: true, pos, .. } if !drawing_tool && !show.grid => clicked_at = Some(*pos),
                egui::Event::PointerButton { button: egui::PointerButton::Secondary, pressed: true, .. } if !kiosk && !presenter => show.prev(now),
                egui::Event::MouseWheel { delta, .. } if !kiosk && !presenter && !show.grid => { if delta.y < -0.5 { if !show.next(steps, now) { end = true; } } else if delta.y > 0.5 { show.prev(now); } }
                _ => {}
            }
        }
    });
    // Pen and highlighter: a drag draws; the eraser removes strokes it touches.
    let rect = show.slide_rect;
    let to_slide = |p: Pos2| pos2((p.x - rect.min.x) / rect.width().max(1.0), (p.y - rect.min.y) / rect.height().max(1.0));
    let (down, pointer) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
    let index = show.order.get(show.pos).copied().unwrap_or(0);
    match (show.tool, down, pointer) {
        (Tool::Pen | Tool::Highlighter, true, Some(p)) if rect.contains(p) => {
            let hi = show.tool == Tool::Highlighter;
            let q = to_slide(p);
            match &mut show.drawing {
                Some(d) => { if d.pts.last().is_none_or(|l| l.distance(q) > 0.001) { d.pts.push(q); } }
                None => show.drawing = Some(Ink { slide: index, pts: vec![q], color: if hi { Color32::from_rgba_unmultiplied(255, 230, 0, 110) } else { hex_color(&show.setup.pen_color) }, width: if hi { 14.0 } else { 3.0 }, highlighter: hi }),
            }
        }
        (Tool::Eraser, true, Some(p)) => { let q = to_slide(p); show.ink.retain(|s| s.slide != index || s.pts.iter().all(|k| k.distance(q) > 0.015)); }
        _ => { if let Some(d) = show.drawing.take() { show.ink.push(d); } }
    }
    // A click follows the object's link or action (or plays its media, or a trigger), else
    // advances (not in kiosk mode).
    if let Some(p) = clicked_at.filter(|p| show.slide_rect.contains(*p)) {
        let (sw, sh) = app.pres.size();
        let r = show.slide_rect;
        let (x, y) = (((p.x - r.min.x) / r.width()) as f64 * sw, ((p.y - r.min.y) / r.height()) as f64 * sh);
        let mut handled = false;
        if let Some(part) = app.slides().get(show.order[show.pos]).cloned() {
            let scene = app.pres.scene(&part);
            for it in scene.items.iter().rev().filter(|i| i.own) {
                let Some(inv) = it.transform.invert() else { continue };
                let (lx, ly) = inv.apply(x, y);
                if lx < 0.0 || ly < 0.0 || lx > it.w || ly > it.h { continue; }
                let loc = slides_engine::scene::Loc { part: part.clone(), path: vec![it.loc.path[0]] };
                // A trigger plays its effects.
                if let Some(id) = app.pres.shape_id(&loc) { if anims.iter().any(|a| a.trigger == Some(id)) { show.triggered.retain(|t| t.0 != id); show.triggered.push((id, now)); handled = true; break; } }
                // Media plays (or pauses) right in the slide; without the controls, a click on
                // a playing one pauses it.
                if let Some(m) = app.pres.media_part(&loc) {
                    let video = app.pres.element(&loc).is_some_and(|e| e.find("a:videoFile").is_some());
                    crate::media::toggle(&mut show.player, &app.pres, &m, video);
                    handled = true;
                    break;
                }
                if let Some((kind, target)) = app.pres.shape_link(&loc) {
                    handled = true;
                    match (kind.as_str(), target.as_str()) {
                        ("url", u) => crate::canvas::open_path(u),
                        ("slide", n) => { if let Ok(i) = n.parse::<usize>() { if let Some(pos) = show.order.iter().position(|&s| s == i) { show.go(pos, now); } } }
                        ("action", "nextslide") => { if show.pos + 1 < show.order.len() { let q = show.pos + 1; show.go(q, now); } }
                        ("action", "previousslide") => show.prev(now),
                        ("action", "firstslide") => show.go(0, now),
                        ("action", "lastslide") => { let q = show.order.len() - 1; show.go(q, now); }
                        ("action", "endshow") => end = true,
                        _ => handled = false,
                    }
                    break;
                }
                // Links on text: the run under the pointer.
                if let slides_engine::scene::Kind::Shape { text: Some(t), .. } = &it.kind {
                    if let Some(url) = t.paras.iter().flat_map(|p| p.runs.iter()).find_map(|r| r.link.clone()) { crate::canvas::open_path(&url); handled = true; break; }
                }
            }
        }
        let on_click = app.slides().get(show.order[show.pos]).map(|s| app.pres.slide_transition(s).on_click).unwrap_or(true);
        if !handled && !kiosk && !presenter && show.tool == Tool::Arrow && (on_click || show.step < steps) && !show.next(steps, now) { end = true; }
    }
    // Automatic advance after the slide's time (when timings are used), and the sound.
    let part = app.slides().get(index).cloned().unwrap_or_default();
    let trans = app.pres.slide_transition(&part);
    let dur = trans.duration_ms.max(1) as f64 / 1000.0;
    if show.sounded != Some(show.pos) { show.sounded = Some(show.pos); if let Some(w) = app.pres.transition_sound(&part) { crate::transitions_tab::play_sound(&w); } }
    if let (Some(after), true, None) = (trans.after_ms, show.setup.use_timings, &show.rehearse) {
        let wait = dur.max(0.0) + after as f64 / 1000.0;
        if now - show.since >= wait && show.blank.is_none() && show.step >= steps.min(if show.setup.animation { steps } else { 0 }) {
            if !show.next(0, now) && show.setup.kind == ShowType::Kiosk { show.go(0, now); }
        } else { ctx.request_repaint_after(std::time::Duration::from_millis(100)); }
        // Timed slides with click animations: the steps follow the time, too.
        if now - show.since >= wait && show.step < steps { show.step += 1; show.step_since = now; }
    }
    if kiosk && show.pos + 1 >= show.order.len() && trans.after_ms.is_none() { /* kiosk without timings waits on the last slide */ }
    if end || show.order.is_empty() { end_show(app, show, &ctx, now); return; }
    let laser = if show.tool == Tool::Laser { ctx.input(|i| i.pointer.latest_pos()) } else { None };
    if laser.is_some() || show.tool != Tool::Arrow { ctx.set_cursor_icon(if show.tool == Tool::Laser { egui::CursorIcon::None } else { egui::CursorIcon::Crosshair }); }

    let mut ended = false;
    if presenter {
        // The audience's slide in its own (full-screen) window.
        let mut aud_end = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("audience"), { let mut b = egui::ViewportBuilder::default().with_title(tr("Slide Show")).with_fullscreen(true); if let Some((p, sz)) = crate::views::show_monitor(&show.setup) { b = b.with_position(p).with_inner_size(sz); } b }, |vctx, class| {
            let mut draw = |ui: &mut egui::Ui| {
                let r = fit(ui.max_rect(), app);
                ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::BLACK);
                let c = ui.ctx().clone();
                paint_slide(&mut show, app, &c, &ui.painter().clone(), r, now, &anims, None);
                let cur = app.slides().get(show.order[show.pos]).cloned().unwrap_or_default();
                crate::media::overlay(&mut show.player, &app.pres, ui, &cur, r, false);
            };
            if class == egui::ViewportClass::EmbeddedWindow {
                egui::Window::new(tr("Audience")).default_size(vec2(480.0, 270.0)).anchor(egui::Align2::RIGHT_BOTTOM, vec2(-16.0, -64.0)).resizable(true).show(vctx, |ui| { let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().max(320.0), 200.0), egui::Sense::hover()); let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r)); draw(&mut c); });
            } else {
                egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(vctx, |ui| draw(ui));
                if vctx.input(|i| i.key_pressed(egui::Key::Escape) || i.viewport().close_requested()) { aud_end = true; }
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_gray(0x1E))).show(ui, |ui| { ended = presenter_ui(&mut show, app, ui, now, &anims, steps, laser); });
        ended |= aud_end;
    } else {
        let browsed = show.setup.kind == ShowType::Browsed;
        if browsed {
            egui::Panel::bottom("show-status").frame(egui::Frame::NONE.fill(qalam_kit::colors::c_frame()).inner_margin(egui::Margin::symmetric(10, 4))).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("◀").clicked() { show.prev(now); }
                    if ui.button("▶").clicked() && !show.next(steps, now) { ended = true; }
                    ui.label(crate::i18n::trf("Slide {} of {}", &[&(show.pos + 1), &show.order.len()]));
                    if ui.button(tr("End Show")).clicked() { ended = true; }
                });
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ui, |ui| {
            let screen = ui.max_rect();
            let r = fit(screen, app);
            show.slide_rect = r;
            if show.grid { grid_ui(&mut show, app, ui, now); return; }
            paint_slide(&mut show, app, &ctx, ui.painter(), r, now, &anims, laser);
            let controls = show.setup.media_controls;
            crate::media::overlay(&mut show.player, &app.pres, ui, &part, r, controls);
            if show.blank.is_some() { ui.painter().rect_filled(screen, 0.0, show.blank.unwrap()); }
            // The show toolbar shows while the pointer moves (not in kiosk mode).
            let still = ctx.input(|i| i.pointer.time_since_last_movement()) > 2.0;
            if !kiosk && (!still || show.tool != Tool::Arrow) { if toolbar(&mut show, ui, screen, now, steps) { ended = true; } }
            if still && show.tool == Tool::Arrow { ctx.set_cursor_icon(egui::CursorIcon::None); } else { ctx.request_repaint_after(std::time::Duration::from_millis(500)); }
            if show.rehearse.is_some() && rehearse_bar(&mut show, ui, screen, now, steps) { ended = true; }
        });
    }
    if ended { end_show(app, show, &ctx, now); return; }
    app.show = Some(show);
}

fn tool_button(ui: &mut egui::Ui, icon: &str, tip: &str, on: bool) -> bool {
    qalam_kit::widgets::small(ui, icon, None, on, true, tip).clicked()
}

/// The pen and pointer tools.
fn tools(show: &mut Show, ui: &mut egui::Ui) {
    for (t, icon, tip) in [(Tool::Laser, "laser", "Laser Pointer (Ctrl+L)"), (Tool::Pen, "penTool", "Pen (Ctrl+P)"), (Tool::Highlighter, "highlighterTool", "Highlighter (Ctrl+I)"), (Tool::Eraser, "eraserTool", "Eraser (Ctrl+E)")] {
        if tool_button(ui, icon, tip, show.tool == t) { show.tool = if show.tool == t { Tool::Arrow } else { t }; }
    }
    ui.menu_button("🎨", |ui| for (hex, name) in [("FF0000", "Red"), ("FFC000", "Orange"), ("FFFF00", "Yellow"), ("00B050", "Green"), ("0070C0", "Blue"), ("7030A0", "Purple"), ("000000", "Black"), ("FFFFFF", "White")] {
        if ui.selectable_label(show.setup.pen_color == hex, tr(name)).clicked() { show.setup.pen_color = hex.into(); show.tool = Tool::Pen; ui.close(); }
    });
}

/// The slide show's toolbar (bottom-left): previous, next, tools, see all slides, end.
fn toolbar(show: &mut Show, ui: &mut egui::Ui, screen: Rect, now: f64, steps: usize) -> bool {
    let mut end = false;
    let r = Rect::from_min_size(pos2(screen.min.x + 16.0, screen.max.y - 50.0), vec2(330.0, 36.0));
    ui.painter().rect_filled(r, 18.0, Color32::from_black_alpha(140));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(12.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    if c.add(egui::Button::new(egui::RichText::new("◀").color(Color32::WHITE)).frame(false)).on_hover_text(tr("Previous")).clicked() { show.prev(now); }
    if c.add(egui::Button::new(egui::RichText::new("▶").color(Color32::WHITE)).frame(false)).on_hover_text(tr("Next")).clicked() && !show.next(steps, now) { end = true; }
    tools(show, &mut c);
    if tool_button(&mut c, "seeAllSlides", "See All Slides (G)", show.grid) { show.grid = !show.grid; }
    if tool_button(&mut c, "blackScreen", "Black Screen (B)", show.blank.is_some()) { show.blank = if show.blank.is_some() { None } else { Some(Color32::BLACK) }; }
    if c.add(egui::Button::new(egui::RichText::new("✕").color(Color32::WHITE)).frame(false)).on_hover_text(tr("End Show")).clicked() { end = true; }
    end
}

/// Rehearse Timings / Record: the slide's time and the total, with Next, Pause and Repeat.
fn rehearse_bar(show: &mut Show, ui: &mut egui::Ui, screen: Rect, now: f64, steps: usize) -> bool {
    let mut end = false;
    let Some(r) = &show.rehearse else { return false };
    let paused = r.paused.is_some();
    let slide_t = r.paused.unwrap_or(now) - r.slide_start;
    let total: f64 = r.times.values().sum::<f64>() + slide_t;
    let record = r.record;
    let fmt = |s: f64| format!("{:01}:{:02}:{:02}", (s / 3600.0) as u32, (s / 60.0) as u32 % 60, s as u32 % 60);
    let bar = Rect::from_min_size(screen.min + vec2(16.0, 16.0), vec2(340.0, 34.0));
    ui.painter().rect_filled(bar, 4.0, Color32::from_gray(245));
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(8.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    if record { c.label(egui::RichText::new("● REC").color(Color32::from_rgb(220, 40, 40)).strong()); }
    if c.button("▶").on_hover_text(tr("Next")).clicked() && !show.next(steps, now) { end = true; }
    if c.button(if paused { "⏵" } else { "⏸" }).on_hover_text(tr("Pause Recording")).clicked() {
        if let Some(r) = &mut show.rehearse { match r.paused.take() { Some(p) => r.slide_start += now - p, None => r.paused = Some(now) } }
    }
    c.label(egui::RichText::new(fmt(slide_t)).monospace().color(Color32::BLACK));
    if c.button("↺").on_hover_text(tr("Repeat")).clicked() { if let Some(r) = &mut show.rehearse { r.slide_start = now; r.paused = None; } }
    c.label(egui::RichText::new(fmt(total)).monospace().color(Color32::from_gray(80)));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    end
}

/// See All Slides: pick one to jump to.
fn grid_ui(show: &mut Show, app: &mut SlidesApp, ui: &mut egui::Ui, now: f64) {
    let ctx = ui.ctx().clone();
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(24.0);
        ui.horizontal_wrapped(|ui| {
            for (k, &i) in show.order.clone().iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(240.0, 150.0), egui::Sense::click());
                let s = fit(r.shrink(6.0), app);
                if let Some(t) = texture(show, app, &ctx, i, 240, false) { ui.painter().image(t.id(), s, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
                ui.painter().rect_stroke(s, 0.0, egui::Stroke::new(if k == show.pos { 3.0 } else if resp.hovered() { 2.0 } else { 0.5 }, if k == show.pos || resp.hovered() { qalam_kit::colors::ACCENT } else { Color32::GRAY }), egui::StrokeKind::Outside);
                ui.painter().text(s.left_bottom() + vec2(0.0, 2.0), egui::Align2::LEFT_TOP, (i + 1).to_string(), egui::FontId::proportional(12.0), Color32::WHITE);
                if resp.clicked() { show.go(k, now); show.grid = false; }
            }
        });
    });
}

/// Presenter View: the current slide, the next one, the notes, the timer and the tools.
fn presenter_ui(show: &mut Show, app: &mut SlidesApp, ui: &mut egui::Ui, now: f64, anims: &[Anim], steps: usize, laser: Option<Pos2>) -> bool {
    let mut end = false;
    let ctx = ui.ctx().clone();
    let white = Color32::from_gray(230);
    // The top bar: timer (pause, restart) and the clock.
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let e = show.elapsed(now).max(0.0);
        ui.label(egui::RichText::new(format!("{:02}:{:02}:{:02}", (e / 3600.0) as u32, (e / 60.0) as u32 % 60, e as u32 % 60)).size(22.0).monospace().color(white));
        if ui.button(if show.timer_paused.is_some() { "⏵" } else { "⏸" }).on_hover_text(tr("Pause the timer")).clicked() {
            match show.timer_paused.take() { Some(p) => show.timer_offset += now - p, None => show.timer_paused = Some(now) }
        }
        if ui.button("↺").on_hover_text(tr("Restart the timer")).clicked() { show.started = now; show.timer_offset = 0.0; show.timer_paused = None; }
        ui.add_space(24.0);
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        ui.label(egui::RichText::new(format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)).size(16.0).color(Color32::from_gray(160))).on_hover_text(tr("Time (UTC)"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            if ui.button(tr("End Slide Show")).clicked() { end = true; }
        });
    });
    ui.add_space(6.0);
    let area = ui.available_rect_before_wrap();
    let left = Rect::from_min_max(area.min + vec2(12.0, 0.0), pos2(area.min.x + area.width() * 0.62, area.max.y - 56.0));
    let right = Rect::from_min_max(pos2(left.max.x + 16.0, area.min.y), pos2(area.max.x - 12.0, area.max.y - 56.0));
    // The current slide.
    let r = fit(left, app);
    let r = Rect::from_min_size(pos2(r.min.x, left.min.y), r.size());
    show.slide_rect = r;
    paint_slide(show, app, &ctx, ui.painter(), r, now, anims, laser);
    let cur = app.slides().get(show.order[show.pos]).cloned().unwrap_or_default();
    crate::media::overlay(&mut show.player, &app.pres, ui, &cur, r, true);
    // The slide and step under it.
    ui.painter().text(pos2(r.center().x, r.max.y + 8.0), egui::Align2::CENTER_TOP, crate::i18n::trf("Slide {} of {}", &[&(show.pos + 1), &show.order.len()]), egui::FontId::proportional(14.0), white);
    // The next slide (or the end).
    ui.painter().text(right.min, egui::Align2::LEFT_TOP, if show.step < steps { tr("Next animation") } else { tr("Next slide") }, egui::FontId::proportional(13.0), Color32::from_gray(160));
    let next_box = Rect::from_min_size(right.min + vec2(0.0, 20.0), vec2(right.width(), right.width() * 9.0 / 16.0));
    let nr = fit(next_box, app);
    let nr = Rect::from_min_size(next_box.min, nr.size());
    let next = if show.step < steps { Some(show.order[show.pos]) } else { show.order.get(show.pos + 1).copied() };
    match next.and_then(|i| texture(show, app, &ctx, i, (nr.width() * ctx.pixels_per_point()) as u32, show.step >= steps)) {
        Some(t) => { ui.painter().image(t.id(), nr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE); }
        None => { ui.painter().rect_filled(nr, 0.0, Color32::BLACK); ui.painter().text(nr.center(), egui::Align2::CENTER_CENTER, tr("End of slide show"), egui::FontId::proportional(14.0), white); }
    }
    // The speaker notes, with their size.
    let notes = app.slides().get(show.order[show.pos]).map(|s| app.pres.notes_text(s)).unwrap_or_default();
    let nb = Rect::from_min_max(pos2(right.min.x, nr.max.y + 16.0), right.max);
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(nb));
    c.horizontal(|ui| {
        ui.label(egui::RichText::new(tr("Notes")).color(Color32::from_gray(160)));
        if ui.small_button("A+").on_hover_text(tr("Make the text larger")).clicked() { show.notes_size = (show.notes_size + 2.0).min(48.0); }
        if ui.small_button("A−").on_hover_text(tr("Make the text smaller")).clicked() { show.notes_size = (show.notes_size - 2.0).max(10.0); }
    });
    egui::ScrollArea::vertical().id_salt("presenter-notes").show(&mut c, |ui| { ui.label(egui::RichText::new(if notes.trim().is_empty() { tr("No notes.") } else { notes }).size(show.notes_size).color(white)); });
    // The bottom bar: tools, see all slides, black screen, previous / next.
    let bottom = Rect::from_min_max(pos2(area.min.x + 12.0, area.max.y - 48.0), pos2(area.max.x - 12.0, area.max.y - 6.0));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(bottom).layout(egui::Layout::left_to_right(egui::Align::Center)));
    tools(show, &mut b);
    if tool_button(&mut b, "seeAllSlides", "See All Slides", show.grid) { show.grid = !show.grid; }
    if tool_button(&mut b, "blackScreen", "Black or unblack slide show", show.blank.is_some()) { show.blank = if show.blank.is_some() { None } else { Some(Color32::BLACK) }; }
    b.add_space(24.0);
    if b.button(egui::RichText::new("◀").size(18.0)).on_hover_text(tr("Previous")).clicked() { show.prev(now); }
    b.label(egui::RichText::new(format!("{} / {}", show.pos + 1, show.order.len())).color(white));
    if b.button(egui::RichText::new("▶").size(18.0)).on_hover_text(tr("Next")).clicked() && !show.next(steps, now) { end = true; }
    if show.grid {
        egui::Window::new(tr("See All Slides")).collapsible(false).default_size(vec2(800.0, 500.0)).show(&ctx, |ui| grid_ui(show, app, ui, now));
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
    end
}
