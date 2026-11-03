//! Toolbar icons, drawn as strokes in the style of the JS app's line icons.

use egui::{Color32, Pos2, Rect, Shape, Stroke, pos2, vec2};
use std::f32::consts::{PI, TAU};

/// Draws icon `name` into `r` (a square) in `ink`.
pub fn paint(p: &egui::Painter, name: &str, r: Rect, ink: Color32) {
    let s = Stroke::new(1.5, ink);
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let line = |pts: &[(f32, f32)]| { p.add(Shape::line(pts.iter().map(|(x, y)| at(*x, *y)).collect(), s)); };
    let closed = |pts: &[(f32, f32)]| { p.add(Shape::closed_line(pts.iter().map(|(x, y)| at(*x, *y)).collect(), s)); };
    let arc = |cx: f32, cy: f32, rr: f32, a0: f32, a1: f32| -> Vec<Pos2> { (0..=16).map(|k| { let a = a0 + (a1 - a0) * k as f32 / 16.0; at(cx + rr * a.cos(), cy + rr * a.sin()) }).collect() };
    match name {
        "select" => closed(&[(0.25, 0.1), (0.8, 0.55), (0.52, 0.6), (0.38, 0.9)]),
        "hand" => {
            for (k, top) in [0.3f32, 0.12, 0.15, 0.25].iter().enumerate() { let x = 0.28 + k as f32 * 0.15; line(&[(x, *top), (x, 0.55)]); }
            line(&[(0.13, 0.5), (0.3, 0.9), (0.73, 0.9), (0.73, 0.4)]);
        }
        "note" => { closed(&[(0.15, 0.15), (0.85, 0.15), (0.85, 0.6), (0.6, 0.85), (0.15, 0.85)]); line(&[(0.6, 0.85), (0.6, 0.6), (0.85, 0.6)]); }
        "text" => { line(&[(0.2, 0.2), (0.8, 0.2)]); line(&[(0.5, 0.2), (0.5, 0.85)]); line(&[(0.4, 0.85), (0.6, 0.85)]); }
        "shape" | "rectangle" => { p.rect_stroke(Rect::from_min_max(at(0.18, 0.18), at(0.82, 0.82)), 2.0, s, egui::StrokeKind::Middle); }
        "terminator" => { p.rect_stroke(Rect::from_min_max(at(0.1, 0.3), at(0.9, 0.7)), r.height() * 0.2, s, egui::StrokeKind::Middle); }
        "ellipse" => { p.circle_stroke(r.center(), r.width() * 0.36, s); }
        "diamond" => closed(&[(0.5, 0.12), (0.88, 0.5), (0.5, 0.88), (0.12, 0.5)]),
        "triangle" => closed(&[(0.5, 0.15), (0.88, 0.85), (0.12, 0.85)]),
        "hexagon" => closed(&[(0.3, 0.15), (0.7, 0.15), (0.9, 0.5), (0.7, 0.85), (0.3, 0.85), (0.1, 0.5)]),
        "parallelogram" => closed(&[(0.3, 0.25), (0.9, 0.25), (0.7, 0.75), (0.1, 0.75)]),
        "star" => {
            let pts: Vec<(f32, f32)> = (0..10).map(|n| { let a = -PI / 2.0 + n as f32 * PI / 5.0; let rr = if n % 2 == 1 { 0.17 } else { 0.4 }; (0.5 + rr * a.cos(), 0.52 + rr * a.sin()) }).collect();
            closed(&pts);
        }
        "cylinder" => {
            p.add(Shape::closed_line(arc(0.5, 0.25, 0.32, 0.0, TAU).into_iter().map(|q| pos2(q.x, r.min.y + (q.y - r.min.y - 0.25 * r.height()) * 0.35 + 0.25 * r.height())).collect(), s));
            line(&[(0.18, 0.25), (0.18, 0.75)]); line(&[(0.82, 0.25), (0.82, 0.75)]);
            p.add(Shape::line(arc(0.5, 0.75, 0.32, 0.0, PI).into_iter().map(|q| pos2(q.x, r.min.y + (q.y - r.min.y - 0.75 * r.height()) * 0.35 + 0.75 * r.height())).collect(), s));
        }
        "document" => { line(&[(0.85, 0.75), (0.85, 0.15), (0.15, 0.15), (0.15, 0.8)]); p.add(Shape::line((0..=12).map(|k| { let t = k as f32 / 12.0; at(0.15 + 0.7 * t, 0.8 - 0.07 * (t * TAU).sin()) }).collect(), s)); }
        "library" => { closed(&[(0.15, 0.55), (0.45, 0.55), (0.45, 0.85), (0.15, 0.85)]); p.circle_stroke(at(0.7, 0.7), r.width() * 0.16, s); closed(&[(0.5, 0.12), (0.7, 0.42), (0.3, 0.42)]); }
        "frame" => { line(&[(0.3, 0.1), (0.3, 0.9)]); line(&[(0.7, 0.1), (0.7, 0.9)]); line(&[(0.1, 0.3), (0.9, 0.3)]); line(&[(0.1, 0.7), (0.9, 0.7)]); }
        "table" => { p.rect_stroke(Rect::from_min_max(at(0.15, 0.18), at(0.85, 0.82)), 2.0, s, egui::StrokeKind::Middle); line(&[(0.15, 0.4), (0.85, 0.4)]); line(&[(0.42, 0.18), (0.42, 0.82)]); }
        "mindmap" => {
            p.rect_stroke(Rect::from_center_size(at(0.5, 0.22), vec2(r.width() * 0.25, r.height() * 0.2)), 1.5, s, egui::StrokeKind::Middle);
            for x in [0.22, 0.78] { p.rect_stroke(Rect::from_center_size(at(x, 0.78), vec2(r.width() * 0.25, r.height() * 0.2)), 1.5, s, egui::StrokeKind::Middle); }
            line(&[(0.5, 0.32), (0.5, 0.5), (0.22, 0.5), (0.22, 0.68)]); line(&[(0.5, 0.5), (0.78, 0.5), (0.78, 0.68)]);
        }
        "image" => { p.rect_stroke(Rect::from_min_max(at(0.12, 0.18), at(0.88, 0.82)), 2.0, s, egui::StrokeKind::Middle); line(&[(0.12, 0.72), (0.38, 0.48), (0.62, 0.7), (0.72, 0.6), (0.88, 0.74)]); p.circle_stroke(at(0.66, 0.36), r.width() * 0.06, s); }
        "pen" => { line(&[(0.2, 0.8), (0.25, 0.6), (0.7, 0.15), (0.85, 0.3), (0.4, 0.75), (0.2, 0.8)]); }
        "line" => { line(&[(0.15, 0.85), (0.85, 0.15)]); line(&[(0.5, 0.15), (0.85, 0.15), (0.85, 0.5)]); }
        "undo" => { p.add(Shape::line(arc(0.5, 0.55, 0.3, PI, TAU + 0.6), s)); line(&[(0.1, 0.35), (0.2, 0.58), (0.38, 0.45)]); }
        "redo" => { p.add(Shape::line(arc(0.5, 0.55, 0.3, -0.6 - PI, 0.0).into_iter().rev().collect(), s)); line(&[(0.9, 0.35), (0.8, 0.58), (0.62, 0.45)]); }
        "minus" => line(&[(0.25, 0.5), (0.75, 0.5)]),
        "plus" => { line(&[(0.25, 0.5), (0.75, 0.5)]); line(&[(0.5, 0.25), (0.5, 0.75)]); }
        "fit" => { for (a, b, c) in [((0.15, 0.35), (0.15, 0.15), (0.35, 0.15)), ((0.65, 0.15), (0.85, 0.15), (0.85, 0.35)), ((0.85, 0.65), (0.85, 0.85), (0.65, 0.85)), ((0.35, 0.85), (0.15, 0.85), (0.15, 0.65))] { line(&[a, b, c]); } }
        "map" => { closed(&[(0.12, 0.22), (0.37, 0.12), (0.63, 0.22), (0.88, 0.12), (0.88, 0.78), (0.63, 0.88), (0.37, 0.78), (0.12, 0.88)]); line(&[(0.37, 0.12), (0.37, 0.78)]); line(&[(0.63, 0.22), (0.63, 0.88)]); }
        "back" => { line(&[(0.85, 0.5), (0.15, 0.5)]); line(&[(0.4, 0.25), (0.15, 0.5), (0.4, 0.75)]); }
        "more" => { for x in [0.25, 0.5, 0.75] { p.circle_filled(at(x, 0.5), 1.6, ink); } }
        "command" => {
            line(&[(0.38, 0.38), (0.62, 0.38), (0.62, 0.62), (0.38, 0.62), (0.38, 0.38)]);
            for (cx, cy, a0) in [(0.27, 0.27, 0.0), (0.73, 0.27, PI / 2.0), (0.73, 0.73, PI), (0.27, 0.73, -PI / 2.0)] { p.add(Shape::line(arc(cx, cy, 0.11, a0, a0 + 1.5 * PI), s)); }
        }
        "align-left" => { line(&[(0.15, 0.25), (0.85, 0.25)]); line(&[(0.15, 0.5), (0.6, 0.5)]); line(&[(0.15, 0.75), (0.75, 0.75)]); }
        "align-center" => { line(&[(0.15, 0.25), (0.85, 0.25)]); line(&[(0.28, 0.5), (0.72, 0.5)]); line(&[(0.2, 0.75), (0.8, 0.75)]); }
        "align-right" => { line(&[(0.15, 0.25), (0.85, 0.25)]); line(&[(0.4, 0.5), (0.85, 0.5)]); line(&[(0.25, 0.75), (0.85, 0.75)]); }
        "list" => { for y in [0.25, 0.5, 0.75] { p.circle_filled(at(0.2, y), 1.6, ink); line(&[(0.35, y), (0.85, y)]); } }
        "link" => {
            p.rect_stroke(Rect::from_center_size(at(0.35, 0.62), vec2(r.width() * 0.4, r.height() * 0.22)).rotate_bb(egui::emath::Rot2::from_angle(-0.8)), r.height() * 0.11, s, egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_center_size(at(0.65, 0.38), vec2(r.width() * 0.4, r.height() * 0.22)).rotate_bb(egui::emath::Rot2::from_angle(-0.8)), r.height() * 0.11, s, egui::StrokeKind::Middle);
        }
        "lock" | "unlock" => {
            p.rect_stroke(Rect::from_min_max(at(0.22, 0.45), at(0.78, 0.88)), 2.0, s, egui::StrokeKind::Middle);
            let a1 = if name == "lock" { TAU } else { TAU - 0.5 };
            p.add(Shape::line(arc(0.5, 0.42, 0.2, PI, a1).into_iter().chain([at(if name == "lock" { 0.7 } else { 0.68 }, 0.45)]).collect(), s));
        }
        "trash" => { line(&[(0.15, 0.25), (0.85, 0.25)]); line(&[(0.38, 0.25), (0.4, 0.12), (0.6, 0.12), (0.62, 0.25)]); line(&[(0.22, 0.25), (0.28, 0.88), (0.72, 0.88), (0.78, 0.25)]); }
        "sun" => { p.circle_stroke(r.center(), r.width() * 0.18, s); for k in 0..8 { let a = k as f32 * TAU / 8.0; line(&[(0.5 + 0.3 * a.cos(), 0.5 + 0.3 * a.sin()), (0.5 + 0.4 * a.cos(), 0.5 + 0.4 * a.sin())]); } }
        "moon" => { p.add(Shape::line(arc(0.5, 0.5, 0.33, 0.9, TAU - 0.4), s)); p.add(Shape::line(arc(0.62, 0.38, 0.22, PI * 0.35, PI * 1.55).into_iter().rev().collect(), s)); }
        "grid" => { for x in [0.25, 0.5, 0.75] { for y in [0.25, 0.5, 0.75] { p.circle_filled(at(x, y), 1.4, ink); } } }
        "border" => { p.rect_stroke(Rect::from_min_max(at(0.18, 0.18), at(0.82, 0.82)), 2.0, Stroke::new(2.2, ink), egui::StrokeKind::Middle); }
        "comment" => { closed(&[(0.15, 0.2), (0.85, 0.2), (0.85, 0.68), (0.45, 0.68), (0.25, 0.86), (0.28, 0.68), (0.15, 0.68)]); }
        "tag" => { closed(&[(0.15, 0.15), (0.55, 0.15), (0.88, 0.5), (0.52, 0.86), (0.15, 0.5)]); p.circle_filled(at(0.33, 0.33), 1.8, ink); }
        "highlight" => { line(&[(0.3, 0.62), (0.62, 0.18), (0.8, 0.32), (0.48, 0.76), (0.3, 0.62)]); p.rect_filled(Rect::from_min_max(at(0.12, 0.8), at(0.88, 0.92)), 1.0, Color32::from_rgb(0xfa, 0xd2, 0x4b)); }
        "image-options" => { p.rect_stroke(Rect::from_min_max(at(0.12, 0.18), at(0.88, 0.82)), 2.0, s, egui::StrokeKind::Middle); line(&[(0.3, 0.05), (0.3, 0.95)]); line(&[(0.7, 0.05), (0.7, 0.95)]); }
        "collapse" => { p.circle_stroke(r.center(), r.width() * 0.36, s); line(&[(0.32, 0.5), (0.68, 0.5)]); }
        "expand" => { p.circle_stroke(r.center(), r.width() * 0.36, s); line(&[(0.32, 0.5), (0.68, 0.5)]); line(&[(0.5, 0.32), (0.5, 0.68)]); }
        "templates" => { p.rect_stroke(Rect::from_min_max(at(0.12, 0.12), at(0.88, 0.88)), 2.0, s, egui::StrokeKind::Middle); line(&[(0.12, 0.38), (0.88, 0.38)]); line(&[(0.42, 0.38), (0.42, 0.88)]); }
        "sticker" => { p.circle_stroke(r.center(), r.width() * 0.38, s); p.circle_filled(at(0.38, 0.42), 1.6, ink); p.circle_filled(at(0.62, 0.42), 1.6, ink); p.add(Shape::line(arc(0.5, 0.52, 0.18, 0.4, PI - 0.4), s)); }
        "vote" => { for (x, y) in [(0.3, 0.35), (0.62, 0.3), (0.45, 0.65), (0.72, 0.68)] { p.circle_filled(at(x, y), r.width() * 0.09, ink); } }
        "chevron" => line(&[(0.3, 0.4), (0.5, 0.6), (0.7, 0.4)]),
        _ => { p.rect_stroke(r.shrink(r.width() * 0.2), 2.0, s, egui::StrokeKind::Middle); }
    }
}

/// Whether `name` has its own icon (shapes without one show a plain square).
pub fn known(name: &str) -> bool {
    matches!(name, "rectangle" | "terminator" | "ellipse" | "diamond" | "triangle" | "hexagon" | "parallelogram" | "star" | "cylinder" | "document" | "library" | "line")
}
