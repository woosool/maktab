//! Drawing slide transitions: each effect blends the previous slide into the next over
//! `t` = 0…1, with egui meshes (textured quads, polygons and tiles), as the slide show and
//! the Transitions tab's preview play them.

use egui::{pos2, vec2, Color32, Mesh, Painter, Pos2, Rect, TextureId};

fn ease(t: f32) -> f32 { t * t * (3.0 - 2.0 * t) }
fn uv_full() -> Rect { Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)) }

/// A texture drawn into a screen rectangle (uv a sub-rectangle), tinted.
fn image(p: &Painter, tex: TextureId, r: Rect, uv: Rect, tint: Color32) { p.image(tex, r, uv, tint); }

/// A texture mapped onto a screen quad (corners TL, TR, BR, BL), subdivided so perspective
/// distortions look smooth.
pub(crate) fn quad(p: &Painter, tex: TextureId, c: [Pos2; 4], uv: Rect, tint: Color32) {
    let n = 12;
    let mut m = Mesh::with_texture(tex);
    let lerp = |a: Pos2, b: Pos2, t: f32| a + (b - a) * t;
    for j in 0..=n {
        let v = j as f32 / n as f32;
        let (l, r) = (lerp(c[0], c[3], v), lerp(c[1], c[2], v));
        for i in 0..=n {
            let u = i as f32 / n as f32;
            m.vertices.push(egui::epaint::Vertex { pos: lerp(l, r, u), uv: pos2(uv.min.x + uv.width() * u, uv.min.y + uv.height() * v), color: tint });
        }
    }
    for j in 0..n { for i in 0..n { let a = (j * (n + 1) + i) as u32; m.add_triangle(a, a + 1, a + n as u32 + 1); m.add_triangle(a + 1, a + n as u32 + 2, a + n as u32 + 1); } }
    p.add(egui::Shape::mesh(m));
}

/// A texture shown through a convex polygon (points in screen space, mapped to `rect`'s uv).
pub(crate) fn poly(p: &Painter, tex: TextureId, rect: Rect, pts: &[Pos2], tint: Color32) {
    if pts.len() < 3 { return; }
    let mut m = Mesh::with_texture(tex);
    let uv = |q: Pos2| pos2((q.x - rect.min.x) / rect.width(), (q.y - rect.min.y) / rect.height());
    let c = pts.iter().fold(Pos2::ZERO, |a, q| a + q.to_vec2()) / pts.len() as f32;
    m.vertices.push(egui::epaint::Vertex { pos: c, uv: uv(c), color: tint });
    for q in pts { m.vertices.push(egui::epaint::Vertex { pos: *q, uv: uv(*q), color: tint }); }
    for k in 0..pts.len() as u32 { m.add_triangle(0, 1 + k, 1 + (k + 1) % pts.len() as u32); }
    p.add(egui::Shape::mesh(m));
}

/// A tile (sub-rectangle) of the slide at a screen position, scaled about its center.
fn tile(p: &Painter, tex: TextureId, rect: Rect, cell: Rect, at: Rect, tint: Color32) {
    let uv = Rect::from_min_max(pos2((cell.min.x - rect.min.x) / rect.width(), (cell.min.y - rect.min.y) / rect.height()), pos2((cell.max.x - rect.min.x) / rect.width(), (cell.max.y - rect.min.y) / rect.height()));
    image(p, tex, at, uv, tint);
}

/// A rectangle rotated in perspective about a vertical (y-axis) or horizontal hinge; angle in
/// radians (0 = flat), hinge 0 = left/top edge, 1 = right/bottom. Returns the corners.
fn hinged(r: Rect, angle: f32, hinge: f32, vertical_axis: bool) -> [Pos2; 4] {
    let (s, c) = angle.sin_cos();
    let depth = 0.35;
    if vertical_axis {
        let hx = r.min.x + r.width() * hinge;
        let edge = |x: f32| { let d = x - hx; let z = d * s; let k = 1.0 / (1.0 + z / r.width() * depth); (hx + d * c * k, k) };
        let (x0, k0) = edge(r.min.x);
        let (x1, k1) = edge(r.max.x);
        let cy = r.center().y;
        let h = r.height() / 2.0;
        [pos2(x0, cy - h * k0), pos2(x1, cy - h * k1), pos2(x1, cy + h * k1), pos2(x0, cy + h * k0)]
    } else {
        let hy = r.min.y + r.height() * hinge;
        let edge = |y: f32| { let d = y - hy; let z = d * s; let k = 1.0 / (1.0 + z / r.height() * depth); (hy + d * c * k, k) };
        let (y0, k0) = edge(r.min.y);
        let (y1, k1) = edge(r.max.y);
        let cx = r.center().x;
        let w = r.width() / 2.0;
        [pos2(cx - w * k0, y0), pos2(cx + w * k0, y0), pos2(cx + w * k1, y1), pos2(cx - w * k1, y1)]
    }
}

fn rand(seed: u32) -> f32 { let mut x = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453); x ^= x >> 13; x = x.wrapping_mul(1_274_126_177); (x % 10_000) as f32 / 10_000.0 }

/// Direction vector for an option ("r" from right → moves left, etc.).
fn dirv(opt: &str) -> egui::Vec2 {
    match opt { "r" => vec2(1.0, 0.0), "l" => vec2(-1.0, 0.0), "u" => vec2(0.0, -1.0), "d" => vec2(0.0, 1.0), "ru" => vec2(1.0, -1.0), "rd" => vec2(1.0, 1.0), "lu" => vec2(-1.0, -1.0), "ld" => vec2(-1.0, 1.0), _ => vec2(1.0, 0.0) }
}

/// Draw the transition at progress `t`. `from` is the previous slide (None: black).
pub fn draw(p: &Painter, effect: &str, option: &str, t: f32, rect: Rect, from: Option<TextureId>, to: TextureId) {
    let t = t.clamp(0.0, 1.0);
    let e = ease(t);
    let w = Color32::WHITE;
    let (rw, rh) = (rect.width(), rect.height());
    let black = |p: &Painter| { p.rect_filled(rect, 0.0, Color32::BLACK); };
    let old = |p: &Painter, tint: Color32| match from { Some(f) => image(p, f, rect, uv_full(), tint), None => { p.rect_filled(rect, 0.0, Color32::BLACK.gamma_multiply(tint.a() as f32 / 255.0)); } };
    let new = |p: &Painter, tint: Color32| image(p, to, rect, uv_full(), tint);
    let clip = |r: Rect| p.with_clip_rect(r.intersect(rect));
    // Random picks one effect per slide change (the caller seeds it through `option`).
    let fx = if effect == "random" { let list = ["fade", "push", "wipe", "split", "cover", "uncover", "shape", "dissolve", "blinds", "checkerboard", "clock", "honeycomb", "cube", "flip", "gallery"]; list[(rand(option.bytes().map(|b| b as u32).sum::<u32>() + 7) * list.len() as f32) as usize % list.len()] } else { effect };
    match fx {
        "none" => new(p, w),
        "cut" => { if option == "black" && t < 0.5 { black(p) } else { new(p, w) } }
        "fade" | "morph" => {
            if option == "black" { if t < 0.5 { old(p, w); p.rect_filled(rect, 0.0, Color32::BLACK.gamma_multiply(t * 2.0)); } else { black(p); new(p, w.gamma_multiply((t - 0.5) * 2.0)); } }
            else { old(p, w); let s = if fx == "morph" { 1.0 + 0.02 * (1.0 - e) } else { 1.0 }; image(p, to, Rect::from_center_size(rect.center(), rect.size() * s), uv_full(), w.gamma_multiply(e)); }
        }
        "push" | "pan" => {
            let d = -dirv(option) * vec2(rw, rh);
            if let Some(f) = from { image(&clip(rect), f, rect.translate(d * e), uv_full(), w); }
            image(&clip(rect), to, rect.translate(-d * (1.0 - e)), uv_full(), w);
        }
        "wipe" => {
            old(p, w);
            let d = dirv(option);
            let r = Rect::from_min_max(pos2(if d.x > 0.0 { rect.max.x - rw * e } else { rect.min.x }, if d.y > 0.0 { rect.max.y - rh * e } else { rect.min.y }), pos2(if d.x < 0.0 { rect.min.x + rw * e } else { rect.max.x }, if d.y < 0.0 { rect.min.y + rh * e } else { rect.max.y }));
            let r = if d.x == 0.0 { Rect::from_x_y_ranges(rect.x_range(), r.y_range()) } else if d.y == 0.0 { Rect::from_x_y_ranges(r.x_range(), rect.y_range()) } else { r };
            new(&clip(r), w);
        }
        "split" => {
            old(p, w);
            let vert = option.starts_with("vert") || option.is_empty();
            let out = !option.ends_with("In");
            let c = rect.center();
            if out {
                let r = if vert { Rect::from_center_size(c, vec2(rw * e, rh)) } else { Rect::from_center_size(c, vec2(rw, rh * e)) };
                new(&clip(r), w);
            } else {
                let (a, b) = if vert { (Rect::from_min_size(rect.min, vec2(rw * e / 2.0, rh)), Rect::from_min_size(pos2(rect.max.x - rw * e / 2.0, rect.min.y), vec2(rw * e / 2.0, rh))) } else { (Rect::from_min_size(rect.min, vec2(rw, rh * e / 2.0)), Rect::from_min_size(pos2(rect.min.x, rect.max.y - rh * e / 2.0), vec2(rw, rh * e / 2.0))) };
                new(&clip(a), w); new(&clip(b), w);
            }
        }
        "reveal" => {
            let black_mid = option.starts_with("black");
            let left = !option.ends_with('R');
            if black_mid { black(p); }
            if let Some(f) = from { image(&clip(rect), f, rect.translate(vec2(if left { -rw } else { rw } * e * 0.3, 0.0)), uv_full(), w.gamma_multiply(1.0 - e)); }
            new(p, w.gamma_multiply(if black_mid { ((t - 0.4) / 0.6).clamp(0.0, 1.0) } else { e }));
        }
        "randomBars" => {
            old(p, w);
            let n = 72;
            for k in 0..n {
                if rand(k as u32 * 31 + 3) > t { continue; }
                let r = if option == "horz" { Rect::from_min_size(pos2(rect.min.x, rect.min.y + rh * k as f32 / n as f32), vec2(rw, rh / n as f32 + 0.5)) } else { Rect::from_min_size(pos2(rect.min.x + rw * k as f32 / n as f32, rect.min.y), vec2(rw / n as f32 + 0.5, rh)) };
                new(&clip(r), w);
            }
        }
        "shape" => {
            old(p, w);
            let c = rect.center();
            let rad = (rw * rw + rh * rh).sqrt() / 2.0 * e;
            if option == "plus" {
                let (a, b) = (rad * 1.4, rad * 0.45);
                for r in [Rect::from_center_size(c, vec2(a * 2.0, b * 2.0)), Rect::from_center_size(c, vec2(b * 2.0, a * 2.0))] { new(&clip(r), w); }
                return;
            }
            let pts: Vec<Pos2> = match option {
                "diamond" => vec![c + vec2(0.0, -rad * 1.42), c + vec2(rad * 1.42, 0.0), c + vec2(0.0, rad * 1.42), c + vec2(-rad * 1.42, 0.0)],
                "in" | "out" => { let r = Rect::from_center_size(c, rect.size() * e); vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()] }
                _ => (0..64).map(|k| { let a = k as f32 / 64.0 * std::f32::consts::TAU; c + vec2(a.cos(), a.sin()) * rad }).collect(),
            };
            let pts: Vec<Pos2> = pts.into_iter().map(|q| pos2(q.x.clamp(rect.min.x - rw, rect.max.x + rw), q.y.clamp(rect.min.y - rh, rect.max.y + rh))).collect();
            poly(&clip(rect), to, rect, &pts, w);
        }
        "uncover" => { new(p, w); if let Some(f) = from { image(&clip(rect), f, rect.translate(-dirv(option) * vec2(rw, rh) * e), uv_full(), w); } }
        "cover" => { old(p, w); image(&clip(rect), to, rect.translate(dirv(option) * vec2(rw, rh) * (1.0 - e)), uv_full(), w); }
        "flash" => { if t < 0.5 { old(p, w); p.rect_filled(rect, 0.0, Color32::WHITE.gamma_multiply(t * 2.0)); } else { new(p, w); p.rect_filled(rect, 0.0, Color32::WHITE.gamma_multiply((1.0 - t) * 2.0)); } }
        "fallOver" | "flip" | "switch" | "gallery" | "cube" | "box" | "rotate" | "orbit" | "doors" | "window" | "ferris" | "conveyor" | "prestige" | "pageCurl" | "peelOff" | "drape" => {
            three_d(p, fx, option, t, rect, from, to);
        }
        "curtains" | "wind" | "shred" | "comb" | "blinds" | "airplane" | "origami" | "crush" | "fracture" => strips(p, fx, option, t, rect, from, to),
        "dissolve" | "checkerboard" | "honeycomb" | "glitter" | "vortex" => tiles(p, fx, option, t, rect, from, to),
        "clock" => {
            old(p, w);
            let c = rect.center();
            let r = (rw * rw + rh * rh).sqrt();
            let n = 96;
            let sweep = e * std::f32::consts::TAU;
            let start = -std::f32::consts::FRAC_PI_2;
            let mut pts = vec![c];
            match option {
                "wedge" => { for k in 0..=n { let a = start - sweep / 2.0 + sweep * k as f32 / n as f32; pts.push(c + vec2(a.cos(), a.sin()) * r); } }
                "ccw" => { for k in 0..=n { let a = start - sweep * k as f32 / n as f32; pts.push(c + vec2(a.cos(), a.sin()) * r); } }
                _ => { for k in 0..=n { let a = start + sweep * k as f32 / n as f32; pts.push(c + vec2(a.cos(), a.sin()) * r); } }
            }
            // A fan from the center (the sector is not convex past half a turn: draw slices).
            for k in 1..pts.len() - 1 { poly(&clip(rect), to, rect, &[pts[0], pts[k], pts[k + 1]], w); }
        }
        "ripple" => {
            old(p, w);
            let c = match option { "lu" => rect.left_top(), "ru" => rect.right_top(), "ld" => rect.left_bottom(), "rd" => rect.right_bottom(), _ => rect.center() };
            let maxr = (rw * rw + rh * rh).sqrt();
            let rad = maxr * e;
            let circle: Vec<Pos2> = (0..72).map(|k| { let a = k as f32 / 72.0 * std::f32::consts::TAU; c + vec2(a.cos(), a.sin()) * rad }).collect();
            poly(&clip(rect), to, rect, &circle, w);
            for ring in 0..4 { let rr = rad - ring as f32 * 18.0; if rr > 0.0 { p.with_clip_rect(rect).circle_stroke(c, rr, egui::Stroke::new(3.0, Color32::from_white_alpha((60.0 * (1.0 - t)) as u8))); } }
        }
        "zoom" => {
            match option {
                "out" => { new(p, w); if let Some(f) = from { image(&clip(rect), f, Rect::from_center_size(rect.center(), rect.size() * (1.0 + e * 1.5)), uv_full(), w.gamma_multiply(1.0 - e)); } }
                "inRound" => { black(p); let r = Rect::from_center_size(rect.center(), rect.size() * e.max(0.01)); let a = (1.0 - e) * std::f32::consts::TAU; let c = r.center(); let rot = |q: Pos2| { let d = q - c; c + vec2(d.x * a.cos() - d.y * a.sin(), d.x * a.sin() + d.y * a.cos()) }; quad(p, to, [rot(r.left_top()), rot(r.right_top()), rot(r.right_bottom()), rot(r.left_bottom())], uv_full(), w); }
                _ => { old(p, w.gamma_multiply(1.0 - e)); image(p, to, Rect::from_center_size(rect.center(), rect.size() * (0.3 + 0.7 * e)), uv_full(), w.gamma_multiply(e)); }
            }
        }
        "flythrough" => {
            let into = !option.starts_with("out");
            let bounce = option.ends_with("Bounce");
            let mut k = if into { 0.1 + 0.9 * e } else { 2.5 - 1.5 * e };
            if bounce && t > 0.75 { k *= 1.0 + 0.06 * ((t - 0.75) / 0.25 * std::f32::consts::PI).sin(); }
            if let Some(f) = from { image(&clip(rect), f, Rect::from_center_size(rect.center(), rect.size() * if into { 1.0 + e } else { 1.0 - 0.7 * e }), uv_full(), w.gamma_multiply(1.0 - e)); }
            image(&clip(rect), to, Rect::from_center_size(rect.center(), rect.size() * k), uv_full(), w.gamma_multiply(e.max(0.15)));
        }
        _ => { old(p, w); new(p, w.gamma_multiply(e)); }
    }
}

/// Effects that turn the slides in space: hinged faces drawn with perspective.
fn three_d(p: &Painter, fx: &str, option: &str, t: f32, rect: Rect, from: Option<TextureId>, to: TextureId) {
    let e = ease(t);
    let w = Color32::WHITE;
    let pi = std::f32::consts::PI;
    let rev = matches!(option, "l" | "u" | "doubleR" | "singleR");
    let horiz_axis = matches!(option, "u" | "d");
    let shade = |k: f32| w.gamma_multiply(k.clamp(0.25, 1.0));
    p.rect_filled(rect, 0.0, Color32::BLACK);
    match fx {
        "fallOver" => { image(p, to, rect, uv_full(), w); if let Some(f) = from { quad(p, f, hinged(rect, -e * pi / 2.0, 1.0, false), uv_full(), shade(1.0 - e * 0.8)); } }
        "flip" | "switch" => {
            let a = e * pi;
            let shrink = 1.0 - 0.15 * (a.sin());
            let r = Rect::from_center_size(rect.center(), rect.size() * shrink);
            let dir = if rev { -1.0 } else { 1.0 };
            if a < pi / 2.0 { if let Some(f) = from { quad(p, f, hinged(r, a * dir, 0.5, true), uv_full(), shade(1.0 - a / pi)); } }
            else { quad(p, to, hinged(r, (a - pi) * dir, 0.5, true), uv_full(), shade(a / pi)); }
        }
        "gallery" | "conveyor" => {
            let d = if rev { -1.0 } else { 1.0 };
            let off = rect.width() * e * d;
            if let Some(f) = from { quad(p, f, hinged(rect.translate(vec2(-off, 0.0)), -0.5 * e * d, 0.5, true), uv_full(), shade(1.0 - e * 0.5)); }
            quad(p, to, hinged(rect.translate(vec2(rect.width() * d - off, 0.0)), 0.5 * (1.0 - e) * d, 0.5, true), uv_full(), shade(0.5 + e * 0.5));
        }
        "cube" | "box" | "rotate" | "orbit" => {
            // Two faces of a box turning about the shared edge.
            let a = e * pi / 2.0;
            let s = if rev { -1.0 } else { 1.0 };
            let inv = fx == "box" || fx == "orbit";
            if horiz_axis {
                let cut = rect.min.y + rect.height() * if s > 0.0 { 1.0 - e } else { e };
                let (ra, rb) = (Rect::from_min_max(rect.min, pos2(rect.max.x, cut)), Rect::from_min_max(pos2(rect.min.x, cut), rect.max));
                let (fa, fb) = if s > 0.0 { (from, Some(to)) } else { (Some(to), from) };
                if let Some(f) = fa { quad(p, f, hinged(ra, if inv { a } else { -a } * 0.6, if s > 0.0 { 1.0 } else { 0.0 }, false), uv_full(), shade(1.0 - e * 0.6)); }
                if let Some(f) = fb { quad(p, f, hinged(rb, if inv { -a } else { a } * 0.6, if s > 0.0 { 0.0 } else { 1.0 }, false), uv_full(), shade(0.4 + e * 0.6)); }
            } else {
                let cut = rect.min.x + rect.width() * if s > 0.0 { 1.0 - e } else { e };
                let (ra, rb) = (Rect::from_min_max(rect.min, pos2(cut, rect.max.y)), Rect::from_min_max(pos2(cut, rect.min.y), rect.max));
                let (fa, fb) = if s > 0.0 { (from, Some(to)) } else { (Some(to), from) };
                if let Some(f) = fa { quad(p, f, hinged(ra, if inv { a } else { -a } * 0.6, if s > 0.0 { 1.0 } else { 0.0 }, true), uv_full(), shade(if s > 0.0 { 1.0 - e * 0.6 } else { 0.4 + e * 0.6 })); }
                if let Some(f) = fb { quad(p, f, hinged(rb, if inv { -a } else { a } * 0.6, if s > 0.0 { 0.0 } else { 1.0 }, true), uv_full(), shade(if s > 0.0 { 0.4 + e * 0.6 } else { 1.0 - e * 0.6 })); }
            }
        }
        "doors" | "window" => {
            image(p, to, Rect::from_center_size(rect.center(), rect.size() * (0.85 + 0.15 * e)), uv_full(), w.gamma_multiply(e.max(0.3)));
            let Some(f) = from else { return };
            let vert = option != "horz";
            let a = e * pi / 2.0 * 0.95;
            if vert {
                let (l, r) = (Rect::from_min_max(rect.min, pos2(rect.center().x, rect.max.y)), Rect::from_min_max(pos2(rect.center().x, rect.min.y), rect.max));
                quad(p, f, hinged(l, -a, 0.0, true), Rect::from_min_max(pos2(0.0, 0.0), pos2(0.5, 1.0)), shade(1.0 - e));
                quad(p, f, hinged(r, a, 1.0, true), Rect::from_min_max(pos2(0.5, 0.0), pos2(1.0, 1.0)), shade(1.0 - e));
            } else {
                let (t_, b) = (Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y)), Rect::from_min_max(pos2(rect.min.x, rect.center().y), rect.max));
                quad(p, f, hinged(t_, -a, 0.0, false), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 0.5)), shade(1.0 - e));
                quad(p, f, hinged(b, a, 1.0, false), Rect::from_min_max(pos2(0.0, 0.5), pos2(1.0, 1.0)), shade(1.0 - e));
            }
        }
        "ferris" => {
            image(p, to, rect, uv_full(), w.gamma_multiply(e));
            if let Some(f) = from {
                let a = e * 1.2 * if rev { -1.0 } else { 1.0 };
                let pivot = rect.center() + vec2(0.0, rect.height() * 1.5);
                let rot = |q: Pos2| { let d = q - pivot; pivot + vec2(d.x * a.cos() - d.y * a.sin(), d.x * a.sin() + d.y * a.cos()) };
                quad(&p.with_clip_rect(rect), f, [rot(rect.left_top()), rot(rect.right_top()), rot(rect.right_bottom()), rot(rect.left_bottom())], uv_full(), w);
            }
        }
        "prestige" => {
            if let Some(f) = from { image(p, f, Rect::from_center_size(rect.center(), rect.size() * (1.0 - 0.3 * e)), uv_full(), w.gamma_multiply(1.0 - e)); }
            quad(p, to, hinged(rect, (1.0 - e) * 0.9, 1.0, false), uv_full(), w.gamma_multiply(e));
        }
        _ => {
            // Page curl, peel off, drape: the old slide's corner lifts across the new one.
            image(p, to, rect, uv_full(), w);
            let Some(f) = from else { return };
            let right = !rev;
            let x = if right { rect.max.x - rect.width() * e * 1.1 } else { rect.min.x + rect.width() * e * 1.1 };
            let lift = rect.height() * 0.15 * (e * pi).sin();
            let c = if right { [rect.left_top(), pos2(x, rect.min.y - lift), pos2(x - lift, rect.max.y), rect.left_bottom()] } else { [pos2(x, rect.min.y - lift), rect.right_top(), rect.right_bottom(), pos2(x + lift, rect.max.y)] };
            let uv = if right { Rect::from_min_max(pos2(0.0, 0.0), pos2(((x - rect.min.x) / rect.width()).max(0.0), 1.0)) } else { Rect::from_min_max(pos2(((x - rect.min.x) / rect.width()).min(1.0), 0.0), pos2(1.0, 1.0)) };
            if (right && x > rect.min.x) || (!right && x < rect.max.x) {
                quad(p, f, c, uv, w);
                // The curled-back edge: a lighter strip with a shadow.
                let edge = if right { [pos2(x, rect.min.y - lift), pos2(x + 30.0 * (1.0 - e), rect.min.y), pos2(x + 30.0 * (1.0 - e) - lift, rect.max.y), pos2(x - lift, rect.max.y)] } else { [pos2(x - 30.0 * (1.0 - e), rect.min.y), pos2(x, rect.min.y - lift), pos2(x + lift, rect.max.y), pos2(x - 30.0 * (1.0 - e) + lift, rect.max.y)] };
                p.add(egui::Shape::convex_polygon(edge.to_vec(), Color32::from_gray(235), egui::Stroke::new(1.0, Color32::from_black_alpha(60))));
            }
        }
    }
}

/// Effects made of strips or pieces of the old slide moving away.
fn strips(p: &Painter, fx: &str, option: &str, t: f32, rect: Rect, from: Option<TextureId>, to: TextureId) {
    let e = ease(t);
    let w = Color32::WHITE;
    image(p, to, rect, uv_full(), w);
    let Some(f) = from else { return };
    let clip = p.with_clip_rect(rect);
    match fx {
        "curtains" => {
            // Two halves drawing aside, gathering as they go.
            for side in [0.0f32, 1.0] {
                let n = 10;
                for k in 0..n {
                    let u0 = side * 0.5 + 0.5 * k as f32 / n as f32;
                    let u1 = u0 + 0.5 / n as f32;
                    let squeeze = 1.0 - e * 0.85;
                    let anchor = if side == 0.0 { rect.min.x } else { rect.max.x };
                    let x0 = anchor + (rect.min.x + rect.width() * u0 - anchor) * squeeze;
                    let x1 = anchor + (rect.min.x + rect.width() * u1 - anchor) * squeeze;
                    let wave = (k as f32 * 1.7).sin() * 6.0 * e;
                    image(&clip, f, Rect::from_min_max(pos2(x0, rect.min.y + wave), pos2(x1.max(x0 + 1.0), rect.max.y - wave)), Rect::from_min_max(pos2(u0, 0.0), pos2(u1, 1.0)), w.gamma_multiply(1.0 - e * 0.5));
                }
            }
        }
        "wind" | "airplane" | "origami" => {
            let d = if option == "l" { -1.0 } else { 1.0 };
            let n = 14;
            for k in 0..n {
                let v0 = k as f32 / n as f32;
                let delay = rand(k as u32 + 11) * 0.4;
                let q = ((t - delay) / 0.6).clamp(0.0, 1.0);
                let dx = rect.width() * 1.3 * ease(q) * d;
                let dy = if fx == "airplane" { -rect.height() * 0.6 * ease(q) } else { 0.0 };
                let shrink = if fx == "origami" { 1.0 - 0.8 * ease(q) } else { 1.0 };
                let r = Rect::from_min_size(pos2(rect.min.x + dx, rect.min.y + rect.height() * v0 + dy), vec2(rect.width() * shrink, rect.height() / n as f32 + 0.5));
                image(&clip, f, r, Rect::from_min_max(pos2(0.0, v0), pos2(1.0, v0 + 1.0 / n as f32)), w.gamma_multiply(1.0 - q * 0.6));
            }
        }
        "shred" => {
            let out = option.ends_with("Out");
            let particles = option.starts_with("crush");
            let n = if particles { 24 } else { 18 };
            for k in 0..n {
                for j in 0..(if particles { 14 } else { 1 }) {
                    let (u0, v0) = (k as f32 / n as f32, if particles { j as f32 / 14.0 } else { 0.0 });
                    let (du, dv) = (1.0 / n as f32, if particles { 1.0 / 14.0 } else { 1.0 });
                    let dir = if k % 2 == 0 { -1.0 } else { 1.0 };
                    let q = ((t - rand((k * 31 + j) as u32) * 0.3) / 0.7).clamp(0.0, 1.0);
                    let cell = Rect::from_min_size(pos2(rect.min.x + rect.width() * u0, rect.min.y + rect.height() * v0), vec2(rect.width() * du + 0.5, rect.height() * dv + 0.5));
                    let at = cell.translate(vec2(0.0, dir * rect.height() * ease(q) * if out { 1.0 } else { 1.2 })).expand(if particles { -cell.width() * 0.3 * q } else { 0.0 });
                    tile(&clip, f, rect, cell, at, w.gamma_multiply(1.0 - q));
                }
            }
        }
        "comb" | "blinds" => {
            let horz = option == "horz" || (fx == "comb" && option.is_empty());
            let n = if fx == "blinds" { 8 } else { 12 };
            for k in 0..n {
                let (a, b) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
                if fx == "blinds" {
                    // Each slat turns: the old slide's band narrows.
                    let keep = 1.0 - e;
                    let cell = if horz { Rect::from_min_max(pos2(rect.min.x, rect.min.y + rect.height() * a), pos2(rect.max.x, rect.min.y + rect.height() * (a + (b - a) * keep))) } else { Rect::from_min_max(pos2(rect.min.x + rect.width() * a, rect.min.y), pos2(rect.min.x + rect.width() * (a + (b - a) * keep), rect.max.y)) };
                    image(&p.with_clip_rect(cell), f, rect, uv_full(), w);
                } else {
                    let dir = if k % 2 == 0 { 1.0 } else { -1.0 };
                    let cell = if horz { Rect::from_min_max(pos2(rect.min.x, rect.min.y + rect.height() * a), pos2(rect.max.x, rect.min.y + rect.height() * b)) } else { Rect::from_min_max(pos2(rect.min.x + rect.width() * a, rect.min.y), pos2(rect.min.x + rect.width() * b, rect.max.y)) };
                    let off = if horz { vec2(rect.width() * e * dir, 0.0) } else { vec2(0.0, rect.height() * e * dir) };
                    tile(&clip, f, rect, cell, cell.translate(off), w);
                }
            }
        }
        _ => {
            // Crush and fracture: pieces collapse toward the center or fall.
            let n = 10;
            for k in 0..n { for j in 0..6 {
                let cell = Rect::from_min_size(pos2(rect.min.x + rect.width() * k as f32 / n as f32, rect.min.y + rect.height() * j as f32 / 6.0), vec2(rect.width() / n as f32 + 0.5, rect.height() / 6.0 + 0.5));
                let q = ((t - rand((k * 7 + j) as u32) * 0.35) / 0.65).clamp(0.0, 1.0);
                let at = if fx == "crush" {
                    let c = rect.center();
                    let center = cell.center() + (c - cell.center()) * ease(q);
                    Rect::from_center_size(center, cell.size() * (1.0 - 0.9 * q))
                } else { cell.translate(vec2((rand(k as u32 * 3 + j as u32) - 0.5) * 80.0 * q, rect.height() * 1.2 * ease(q) * ease(q))) };
                tile(&clip, f, rect, cell, at, w.gamma_multiply(1.0 - q * 0.7));
            } }
        }
    }
}

/// Effects of tiles revealing the new slide.
fn tiles(p: &Painter, fx: &str, option: &str, t: f32, rect: Rect, from: Option<TextureId>, to: TextureId) {
    let w = Color32::WHITE;
    match from { Some(f) => image(p, f, rect, uv_full(), w), None => { p.rect_filled(rect, 0.0, Color32::BLACK); } }
    let clip = p.with_clip_rect(rect);
    let (cols, rows) = match fx { "dissolve" => (64, 36), "checkerboard" => (16, 9), _ => (12, 7) };
    let (cw, ch) = (rect.width() / cols as f32, rect.height() / rows as f32);
    for j in 0..rows { for i in 0..cols {
        let cell = Rect::from_min_size(pos2(rect.min.x + cw * i as f32, rect.min.y + ch * j as f32), vec2(cw + 0.5, ch + 0.5));
        let order = match fx {
            "dissolve" => rand((j * cols + i) as u32 * 17 + 5),
            "checkerboard" => { let base = if option == "vert" { j as f32 / rows as f32 } else { i as f32 / cols as f32 }; base * 0.5 + if (i + j) % 2 == 0 { 0.0 } else { 0.5 } * 0.5 }
            "glitter" => { let base = i as f32 / cols as f32; let b = if option.ends_with('R') { 1.0 - base } else { base }; b * 0.7 + rand((j * cols + i) as u32) * 0.3 }
            "vortex" => { let c = rect.center(); let d = (cell.center() - c).length() / (rect.size().length() / 2.0); 1.0 - d * 0.8 + rand((j * cols + i) as u32) * 0.2 }
            _ => rand((j * cols + i) as u32 * 13 + 1),
        } * 0.75;
        let q = ((t - order) / 0.25).clamp(0.0, 1.0);
        if q <= 0.0 { continue; }
        match fx {
            "honeycomb" => {
                // A hexagon growing at the tile.
                let c = cell.center();
                let r = cw.max(ch) * 0.75 * q;
                let hex: Vec<Pos2> = (0..6).map(|k| { let a = k as f32 / 6.0 * std::f32::consts::TAU + 0.5; c + vec2(a.cos(), a.sin()) * r }).collect();
                poly(&clip, to, rect, &hex, w);
            }
            "glitter" => {
                let c = cell.center();
                let s = q;
                let shape: Vec<Pos2> = if option.starts_with("hex") { (0..6).map(|k| { let a = k as f32 / 6.0 * std::f32::consts::TAU; c + vec2(a.cos() * cw * 0.7, a.sin() * ch * 0.7) * s }).collect() } else { vec![c + vec2(0.0, -ch * s), c + vec2(cw * s, 0.0), c + vec2(0.0, ch * s), c + vec2(-cw * s, 0.0)] };
                poly(&clip, to, rect, &shape, w.gamma_multiply(0.6 + 0.4 * q));
            }
            "vortex" => {
                let a = (1.0 - q) * 2.5;
                let c = cell.center() + (cell.center() - rect.center()) * (1.0 - q) * 0.3;
                let rot = |dx: f32, dy: f32| c + vec2(dx * a.cos() - dy * a.sin(), dx * a.sin() + dy * a.cos());
                let uv = Rect::from_min_max(pos2((cell.min.x - rect.min.x) / rect.width(), (cell.min.y - rect.min.y) / rect.height()), pos2((cell.max.x - rect.min.x) / rect.width(), (cell.max.y - rect.min.y) / rect.height()));
                let (hw, hh) = (cw / 2.0, ch / 2.0);
                quad(&clip, to, [rot(-hw, -hh), rot(hw, -hh), rot(hw, hh), rot(-hw, hh)], uv, w.gamma_multiply(q));
            }
            _ => tile(&clip, to, rect, cell, if fx == "checkerboard" { Rect::from_min_size(cell.min, vec2(cell.width() * q, cell.height())) } else { cell }, w),
        }
    } }
}
