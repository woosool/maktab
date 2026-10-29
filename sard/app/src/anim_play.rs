//! Playing object animations: the slide is rendered once without its animated shapes and
//! once per animated shape (a transparent layer each); each frame the layers are drawn with
//! the pose the animations give them at that moment (slide show and the editor's Preview).

use crate::app::SlidesApp;
use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, TextureHandle};
use slides_engine::animation::{self, Anim};
use slides_engine::render::{self, Options};

pub struct Layers {
    pub base: TextureHandle,
    /// (top-level shape index, its layer, its bounds as slide fractions).
    pub shapes: Vec<(usize, TextureHandle, Rect)>,
    pub anims: Vec<Anim>,
    /// For each animation, the shape layer it moves.
    pub target: Vec<Option<usize>>,
}

pub fn build(app: &mut SlidesApp, ctx: &egui::Context, part: &str, width_px: u32) -> Option<Layers> {
    let anims = app.pres.animations(part);
    let tops: Vec<Option<usize>> = anims.iter().map(|a| app.pres.shape_by_id(part, a.shape).map(|l| l.path[0])).collect();
    let mut animated: Vec<usize> = tops.iter().flatten().copied().collect();
    animated.sort();
    animated.dedup();
    let scene = app.pres.scene(part);
    let opts = Options { scale: width_px as f64 / scene.w, prompts: false, skip_text_of: None, translate: None };
    let (base, layers) = render::render_layers(&app.pres, &scene, &mut app.images, opts, &animated)?;
    let tex = |name: String, pm: &tiny_skia::Pixmap| ctx.load_texture(name, egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR);
    let mut shapes = vec![];
    for (top, pm) in &layers {
        // The shape's bounds: the union of its items' corners.
        let mut b = Rect::NOTHING;
        for it in scene.items.iter().filter(|i| i.own && i.loc.path.first() == Some(top)) {
            for (x, y) in [(0.0, 0.0), (it.w, 0.0), (it.w, it.h), (0.0, it.h)] {
                let (sx, sy) = it.transform.apply(x, y);
                b.extend_with(pos2((sx / scene.w) as f32, (sy / scene.h) as f32));
            }
        }
        if !b.is_positive() { b = Rect::from_min_size(Pos2::ZERO, vec2(1.0, 1.0)); }
        shapes.push((*top, tex(format!("anim-{part}-{top}"), pm), b));
    }
    let target = tops.iter().map(|t| t.and_then(|t| shapes.iter().position(|s| s.0 == t))).collect();
    Some(Layers { base: tex(format!("anim-{part}-base"), &base), shapes, anims, target })
}

/// Draw the slide in `rect` with each animation's elapsed time (ms; None = not started).
pub fn paint(p: &Painter, rect: Rect, l: &Layers, elapsed: &dyn Fn(usize) -> Option<f64>) {
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    p.image(l.base.id(), rect, full, Color32::WHITE);
    for (k, (_, tex, b)) in l.shapes.iter().enumerate() {
        let list: Vec<(&Anim, Option<f64>)> = l.anims.iter().enumerate().filter(|(i, _)| l.target[*i] == Some(k)).map(|(i, a)| (a, elapsed(i))).collect();
        let pose = animation::pose(&list, b.width() as f64, b.height() as f64);
        draw_posed(p, rect, tex, *b, &pose);
    }
}

fn draw_posed(p: &Painter, rect: Rect, tex: &TextureHandle, b: Rect, pose: &animation::Pose) {
    if !pose.visible || pose.opacity <= 0.0 { return; }
    let to_screen = |q: Pos2| pos2(rect.min.x + q.x * rect.width(), rect.min.y + q.y * rect.height());
    // Shadows and glows reach a little past the bounds.
    let pad = 0.03;
    let src = b.expand(pad).intersect(Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
    let tint = Color32::WHITE.gamma_multiply(pose.opacity.clamp(0.0, 1.0));
    let c = b.center();
    let (sin, cos) = (pose.rot.to_radians() as f32).sin_cos();
    let aspect = rect.width() / rect.height();
    let place = |q: Pos2| {
        // Scale and rotate about the shape's center (in screen proportions), then offset.
        let (x, y) = ((q.x - c.x) * pose.sx as f32 * aspect, (q.y - c.y) * pose.sy as f32);
        let (x, y) = (x * cos - y * sin, x * sin + y * cos);
        to_screen(pos2(c.x + x / aspect + pose.dx as f32, c.y + y + pose.dy as f32))
    };
    let corners = [place(src.left_top()), place(src.right_top()), place(src.right_bottom()), place(src.left_bottom())];
    match &pose.reveal {
        Some((effect, option, t, _out)) => reveal(p, rect, tex, b, effect, option, *t, tint),
        None => crate::transition_fx::quad(p, tex.id(), corners, src, tint),
    }
    if let Some((hex, amount)) = &pose.color {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            let col = Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
            crate::transition_fx::quad(p, tex.id(), corners, src, col.gamma_multiply(amount.clamp(0.0, 1.0) * pose.opacity));
        }
    }
}

/// An appearing (or vanishing) shape seen through a growing window.
fn reveal(p: &Painter, rect: Rect, tex: &TextureHandle, b: Rect, effect: &str, option: &str, t: f32, tint: Color32) {
    let s = |q: Pos2| pos2(rect.min.x + q.x * rect.width(), rect.min.y + q.y * rect.height());
    let b = Rect::from_min_max(s(b.min), s(b.max)).expand(rect.width() * 0.02);
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let clip = |r: Rect| { p.with_clip_rect(r.intersect(p.clip_rect())).image(tex.id(), rect, full, tint); };
    let c = b.center();
    let (w, h) = (b.width(), b.height());
    match effect {
        "wipe" => clip(match option {
            "l" => Rect::from_min_size(b.min, vec2(w * t, h)),
            "r" => Rect::from_min_max(pos2(b.max.x - w * t, b.min.y), b.max),
            "u" => Rect::from_min_size(b.min, vec2(w, h * t)),
            _ => Rect::from_min_max(pos2(b.min.x, b.max.y - h * t), b.max),
        }),
        "split" => {
            let horz = option.starts_with("horz");
            let out = option.ends_with("Out");
            if out {
                if horz { clip(Rect::from_center_size(c, vec2(w, h * t))); } else { clip(Rect::from_center_size(c, vec2(w * t, h))); }
            } else if horz {
                clip(Rect::from_min_size(b.min, vec2(w, h * t / 2.0)));
                clip(Rect::from_min_max(pos2(b.min.x, b.max.y - h * t / 2.0), b.max));
            } else {
                clip(Rect::from_min_size(b.min, vec2(w * t / 2.0, h)));
                clip(Rect::from_min_max(pos2(b.max.x - w * t / 2.0, b.min.y), b.max));
            }
        }
        "randomBars" => {
            let n = 24;
            for i in 0..n {
                let k = ((i * 7919) % 97) as f32 / 97.0;
                if k > t { continue; }
                let f = i as f32 / n as f32;
                clip(if option == "vert" { Rect::from_min_size(pos2(b.min.x + w * f, b.min.y), vec2(w / n as f32 + 1.0, h)) } else { Rect::from_min_size(pos2(b.min.x, b.min.y + h * f), vec2(w, h / n as f32 + 1.0)) });
            }
        }
        "wheel" => {
            let spokes = option.parse::<usize>().unwrap_or(1).max(1);
            let r = (w * w + h * h).sqrt() / 2.0;
            for sp in 0..spokes {
                let a0 = -std::f32::consts::FRAC_PI_2 + sp as f32 * std::f32::consts::TAU / spokes as f32;
                let sweep = std::f32::consts::TAU / spokes as f32 * t;
                let steps = 24;
                // Convex slices of the fan (at most a quarter turn each).
                let mut a = a0;
                while a < a0 + sweep - 1e-4 {
                    let end = (a + std::f32::consts::FRAC_PI_2).min(a0 + sweep);
                    let mut pts = vec![c];
                    for k in 0..=steps { let u = a + (end - a) * k as f32 / steps as f32; pts.push(c + vec2(u.cos(), u.sin()) * r); }
                    crate::transition_fx::poly(&p.with_clip_rect(b), tex.id(), rect, &pts, tint);
                    a = end;
                }
            }
        }
        _ => {
            // Shape: circle, box, diamond or plus, from the center (In) or toward it (Out).
            let out = option.ends_with("Out");
            let k = if out { 1.0 - t } else { t };
            let kind = option.trim_end_matches("In").trim_end_matches("Out");
            let (rx, ry) = (w * 0.75 * k, h * 0.75 * k);
            let pts: Vec<Pos2> = match kind {
                "box" => vec![c + vec2(-rx, -ry), c + vec2(rx, -ry), c + vec2(rx, ry), c + vec2(-rx, ry)],
                "diamond" => vec![c + vec2(0.0, -ry * 1.4), c + vec2(rx * 1.4, 0.0), c + vec2(0.0, ry * 1.4), c + vec2(-rx * 1.4, 0.0)],
                "plus" => { clip(Rect::from_center_size(c, vec2(rx * 2.0, h))); clip(Rect::from_center_size(c, vec2(w, ry * 2.0))); vec![] }
                _ => (0..48).map(|i| { let a = i as f32 / 48.0 * std::f32::consts::TAU; c + vec2(a.cos() * rx * 1.1, a.sin() * ry * 1.1) }).collect(),
            };
            crate::transition_fx::poly(&p.with_clip_rect(b), tex.id(), rect, &pts, tint);
        }
    };
}

/// The editor's Preview: every click step plays in turn, each after the one before ends.
pub fn preview_elapsed(anims: &[Anim], since_ms: f64) -> Vec<Option<f64>> {
    let (sched, steps) = animation::schedule(anims);
    let mut step_start = vec![0.0f64; steps + 2];
    for s in 0..=steps {
        let end = sched.iter().filter(|t| t.step == s).map(|t| (t.at_ms + anims[t.index].duration_ms.max(1)) as f64 * (anims[t.index].repeat.clamp(1, 3) as f64)).fold(0.0, f64::max);
        step_start[s + 1] = step_start[s] + end + if s > 0 || end > 0.0 { 150.0 } else { 0.0 };
    }
    let mut out = vec![None; anims.len()];
    for t in &sched {
        let e = since_ms - step_start[t.step] - t.at_ms as f64;
        out[t.index] = if e >= 0.0 { Some(e) } else { None };
    }
    // Triggered effects play last.
    let tail = step_start[steps + 1];
    for (i, a) in anims.iter().enumerate() { if a.trigger.is_some() { let e = since_ms - tail - a.delay_ms as f64; out[i] = if e >= 0.0 { Some(e) } else { None }; } }
    out
}

/// How long the whole preview lasts (ms).
pub fn preview_length(anims: &[Anim]) -> f64 {
    let mut lo = 0.0;
    let mut hi = 600_000.0;
    // The first moment every effect has finished.
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        let done = preview_elapsed(anims, mid).iter().zip(anims).all(|(e, a)| e.is_some_and(|e| e >= a.duration_ms.max(1) as f64 * a.repeat.clamp(1, 3) as f64));
        if done { hi = mid; } else { lo = mid; }
    }
    hi
}
