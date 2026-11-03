//! The Morph transition: objects on both slides are matched (same name — PowerPoint's "!!"
//! names first — then same text, then same kind of shape) and glide from their old place,
//! size and turn to the new ones while their looks blend; the rest fade out or in.

use crate::app::SlidesApp;
use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, TextureHandle};
use slides_engine::render::{self, Options};
use slides_engine::scene::Kind;

pub struct Obj { tex: TextureHandle, bounds: Rect, rot: f32, name: String, text: String, kind: String }
pub struct Side { bg: TextureHandle, objs: Vec<Obj> }
pub struct Morph { a: Side, b: Side, pairs: Vec<(usize, usize)> }

fn side(app: &mut SlidesApp, ctx: &egui::Context, part: &str, px: u32, tag: &str) -> Option<Side> {
    let scene = app.pres.scene(part);
    let opts = Options { scale: px as f64 / scene.w, prompts: false, skip_text_of: None, translate: None };
    let mut tops: Vec<usize> = scene.items.iter().filter(|i| i.own && !i.hidden).filter_map(|i| i.loc.path.first().copied()).collect();
    tops.dedup();
    let (bg, layers) = render::render_layers(&app.pres, &scene, &mut app.images, opts, &tops)?;
    let tex = |name: String, pm: &tiny_skia::Pixmap| ctx.load_texture(name, egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data()), egui::TextureOptions::LINEAR);
    let names = crate::animations_tab::shape_names(app, part);
    let mut objs = vec![];
    for (top, pm) in &layers {
        let items: Vec<_> = scene.items.iter().filter(|i| i.own && i.loc.path.first() == Some(top)).collect();
        let Some(first) = items.first() else { continue };
        // Bounds of the unrotated frame (the turn is kept apart so it can be eased).
        let (cx, cy) = first.transform.apply(first.w / 2.0, first.h / 2.0);
        let rot = first.transform.b.atan2(first.transform.a) as f32;
        let (sx, sy) = ((first.transform.a.powi(2) + first.transform.b.powi(2)).sqrt() * first.w, (first.transform.c.powi(2) + first.transform.d.powi(2)).sqrt() * first.h);
        let mut bounds = Rect::from_center_size(pos2((cx / scene.w) as f32, (cy / scene.h) as f32), vec2((sx / scene.w) as f32, (sy / scene.h) as f32));
        if items.len() > 1 {
            // Groups: the union of their parts, unturned.
            bounds = Rect::NOTHING;
            for it in &items { for (x, y) in [(0.0, 0.0), (it.w, 0.0), (it.w, it.h), (0.0, it.h)] { let (a, b) = it.transform.apply(x, y); bounds.extend_with(pos2((a / scene.w) as f32, (b / scene.h) as f32)); } }
        }
        let text = items.iter().filter_map(|i| match &i.kind { Kind::Shape { text: Some(t), .. } => Some(t.paras.iter().flat_map(|p| p.runs.iter().map(|r| r.text.clone())).collect::<String>()), _ => None }).collect::<String>();
        let kind = match &first.kind { Kind::Shape { .. } => "shape", Kind::Picture { .. } => "picture", Kind::Table { .. } => "table", Kind::Chart { .. } => "chart", _ => "other" }.to_string();
        let name = app.pres.shape_id(&slides_engine::scene::Loc { part: part.into(), path: vec![*top] }).and_then(|id| names.iter().find(|n| n.0 == id)).map(|n| n.1.clone()).unwrap_or_default();
        objs.push(Obj { tex: tex(format!("morph-{tag}-{top}"), pm), bounds, rot: if items.len() > 1 { 0.0 } else { rot }, name, text, kind });
    }
    Some(Side { bg: tex(format!("morph-{tag}-bg"), &bg), objs })
}

pub fn build(app: &mut SlidesApp, ctx: &egui::Context, from: &str, to: &str, px: u32) -> Option<Morph> {
    let a = side(app, ctx, from, px, "a")?;
    let b = side(app, ctx, to, px, "b")?;
    let mut pairs = vec![];
    let mut used_a = vec![false; a.objs.len()];
    let mut used_b = vec![false; b.objs.len()];
    let mut pass = |f: &dyn Fn(&Obj, &Obj) -> bool, pairs: &mut Vec<(usize, usize)>| {
        for (j, ob) in b.objs.iter().enumerate() {
            if used_b[j] { continue; }
            // The nearest unused match.
            let best = a.objs.iter().enumerate().filter(|(i, oa)| !used_a[*i] && f(oa, ob)).min_by(|x, y| x.1.bounds.center().distance(ob.bounds.center()).total_cmp(&y.1.bounds.center().distance(ob.bounds.center()))).map(|(i, _)| i);
            if let Some(i) = best { used_a[i] = true; used_b[j] = true; pairs.push((i, j)); }
        }
    };
    pass(&|x, y| x.name.starts_with("!!") && x.name == y.name, &mut pairs);
    pass(&|x, y| !x.name.is_empty() && x.name == y.name && x.kind == y.kind, &mut pairs);
    pass(&|x, y| !x.text.trim().is_empty() && x.text == y.text, &mut pairs);
    pass(&|x, y| x.kind == y.kind && x.kind != "shape" , &mut pairs);
    Some(Morph { a, b, pairs })
}

fn place(p: &Painter, rect: Rect, tex: &TextureHandle, src: Rect, src_rot: f32, at: Rect, rot: f32, tint: Color32) {
    // The object's own picture, unturned by its old angle, stretched onto the new frame and
    // turned by the new one.
    let to_screen = |q: Pos2| pos2(rect.min.x + q.x * rect.width(), rect.min.y + q.y * rect.height());
    let aspect = rect.width() / rect.height();
    let pad = 0.25;
    let s = src.expand2(src.size() * pad);
    let corners_src = [s.left_top(), s.right_top(), s.right_bottom(), s.left_bottom()];
    let (kx, ky) = (at.width() / src.width().max(1e-4), at.height() / src.height().max(1e-4));
    let (sin0, cos0) = (-src_rot).sin_cos();
    let (sin1, cos1) = rot.sin_cos();
    let c0 = src.center();
    let c1 = at.center();
    let corners: Vec<Pos2> = corners_src.iter().map(|q| {
        // Into the object's frame (unturn about the old center), scale, turn, move.
        let (x, y) = ((q.x - c0.x) * aspect, q.y - c0.y);
        let (x, y) = (x * cos0 - y * sin0, x * sin0 + y * cos0);
        let (x, y) = (x * kx, y * ky);
        let (x, y) = (x * cos1 - y * sin1, x * sin1 + y * cos1);
        to_screen(pos2(c1.x + x / aspect, c1.y + y))
    }).collect();
    // The texture region is turned with the object, so sample it through the same turn.
    if src_rot.abs() < 1e-3 {
        let uv = s.intersect(Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
        if uv != s {
            // Clipped at the slide's edge: map only the visible part.
            let f = |q: Pos2| pos2((q.x - s.min.x) / s.width(), (q.y - s.min.y) / s.height());
            let lerp = |a: Pos2, b: Pos2, t: f32| a + (b - a) * t;
            let at = |u: Pos2| { let top = lerp(corners[0], corners[1], u.x); let bot = lerp(corners[3], corners[2], u.x); lerp(top, bot, u.y) };
            let (m0, m1) = (f(uv.min), f(uv.max));
            crate::transition_fx::quad(p, tex.id(), [at(m0), at(pos2(m1.x, m0.y)), at(m1), at(pos2(m0.x, m1.y))], uv, tint);
            return;
        }
        crate::transition_fx::quad(p, tex.id(), [corners[0], corners[1], corners[2], corners[3]], s, tint);
    } else {
        // Turned objects: map the turned source quad.
        let to_uv = |q: Pos2| q;
        let src_quad: Vec<Pos2> = corners_src.iter().map(|q| {
            let (x, y) = ((q.x - c0.x) * aspect, q.y - c0.y);
            let (sn, cs) = src_rot.sin_cos();
            let (x, y) = (x * cs - y * sn, x * sn + y * cs);
            to_uv(pos2(c0.x + x / aspect, c0.y + y))
        }).collect();
        let dst: Vec<Pos2> = corners_src.iter().map(|q| {
            let (x, y) = ((q.x - c0.x) * aspect * kx, (q.y - c0.y) * ky);
            let (x, y) = (x * cos1 - y * sin1, x * sin1 + y * cos1);
            to_screen(pos2(c1.x + x / aspect, c1.y + y))
        }).collect();
        let mut m = egui::Mesh::with_texture(tex.id());
        for k in 0..4 { m.vertices.push(egui::epaint::Vertex { pos: dst[k], uv: src_quad[k], color: tint }); }
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        p.add(egui::Shape::mesh(m));
    }
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect { Rect::from_min_max(a.min + (b.min - a.min) * t, a.max + (b.max - a.max) * t) }

fn ease(t: f32) -> f32 { if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 } }

pub fn draw(p: &Painter, m: &Morph, t: f32, rect: Rect) {
    let t = t.clamp(0.0, 1.0);
    let e = ease(t);
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    p.image(m.a.bg.id(), rect, full, Color32::WHITE);
    p.image(m.b.bg.id(), rect, full, Color32::WHITE.gamma_multiply(e));
    // Unmatched old objects fade out quickly, new ones fade in late.
    for (i, o) in m.a.objs.iter().enumerate() {
        if m.pairs.iter().any(|p| p.0 == i) { continue; }
        let k = (1.0 - t * 2.0).max(0.0);
        if k > 0.0 { p.image(o.tex.id(), rect, full, Color32::WHITE.gamma_multiply(k)); }
    }
    for &(i, j) in &m.pairs {
        let (oa, ob) = (&m.a.objs[i], &m.b.objs[j]);
        let at = lerp_rect(oa.bounds, ob.bounds, e);
        let mut dr = ob.rot - oa.rot;
        if dr > std::f32::consts::PI { dr -= std::f32::consts::TAU; } else if dr < -std::f32::consts::PI { dr += std::f32::consts::TAU; }
        let rot = oa.rot + dr * e;
        // The looks blend through the middle of the move.
        let k = ((t - 0.25) / 0.5).clamp(0.0, 1.0);
        if k < 1.0 { place(p, rect, &oa.tex, oa.bounds, oa.rot, at, rot, Color32::WHITE.gamma_multiply(1.0 - k)); }
        if k > 0.0 { place(p, rect, &ob.tex, ob.bounds, ob.rot, at, rot, Color32::WHITE.gamma_multiply(k)); }
    }
    for (j, o) in m.b.objs.iter().enumerate() {
        if m.pairs.iter().any(|p| p.1 == j) { continue; }
        let k = ((t - 0.5) * 2.0).max(0.0);
        if k > 0.0 { p.image(o.tex.id(), rect, full, Color32::WHITE.gamma_multiply(k)); }
    }
}
