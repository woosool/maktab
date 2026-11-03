//! Snapping while moving, alignment and distribution (`snapping.js`, `spacing.js`), and which
//! mind-map branches a collapsed node hides (`mind-visibility.js`).

use crate::geometry::{Box2, object_bounds};
use crate::model::Item;
use std::collections::HashSet;

/// A guide line drawn while snapping: vertical at x (`true`) or horizontal at y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide { pub vertical: bool, pub at: f64, pub from: f64, pub to: f64 }

/// How far to shift a moving box so one of its edges or its centre lines up with another
/// object's, within `tolerance` board units, and the guides that show it.
pub fn snap(moving: Box2, others: &[Box2], tolerance: f64) -> (f64, f64, Vec<Guide>) {
    let xs = |b: &Box2| [b.x, b.x + b.w / 2.0, b.x + b.w];
    let ys = |b: &Box2| [b.y, b.y + b.h / 2.0, b.y + b.h];
    let best = |mine: [f64; 3], theirs: &dyn Fn(&Box2) -> [f64; 3]| {
        let mut found: Option<(f64, f64)> = None;
        for o in others {
            for t in theirs(o) {
                for m in mine {
                    let d = t - m;
                    if d.abs() <= tolerance && found.is_none_or(|(fd, _)| d.abs() < fd.abs()) { found = Some((d, t)); }
                }
            }
        }
        found
    };
    let (mut dx, mut dy, mut guides) = (0.0, 0.0, Vec::new());
    if let Some((d, at)) = best(xs(&moving), &xs) {
        dx = d;
        let span = others.iter().filter(|o| xs(o).iter().any(|v| (v - at).abs() < 1e-6));
        let (lo, hi) = span.fold((moving.y, moving.y + moving.h), |(lo, hi), o| (lo.min(o.y), hi.max(o.y + o.h)));
        guides.push(Guide { vertical: true, at, from: lo, to: hi });
    }
    if let Some((d, at)) = best(ys(&moving), &ys) {
        dy = d;
        let span = others.iter().filter(|o| ys(o).iter().any(|v| (v - at).abs() < 1e-6));
        let (lo, hi) = span.fold((moving.x, moving.x + moving.w), |(lo, hi), o| (lo.min(o.x), hi.max(o.x + o.w)));
        guides.push(Guide { vertical: false, at, from: lo, to: hi });
    }
    (dx, dy, guides)
}

/// Lines objects up: "left", "center", "right", "top", "middle" or "bottom" of their box.
pub fn align(items: &mut [&mut Item], how: &str) {
    let boxes: Vec<Box2> = items.iter().map(|i| object_bounds(i)).collect();
    if boxes.len() < 2 { return; }
    let x0 = boxes.iter().map(|b| b.x).fold(f64::MAX, f64::min);
    let x1 = boxes.iter().map(|b| b.x + b.w).fold(f64::MIN, f64::max);
    let y0 = boxes.iter().map(|b| b.y).fold(f64::MAX, f64::min);
    let y1 = boxes.iter().map(|b| b.y + b.h).fold(f64::MIN, f64::max);
    for (i, b) in items.iter_mut().zip(&boxes) {
        match how {
            "left" => i.x += x0 - b.x,
            "center" => i.x += (x0 + x1) / 2.0 - (b.x + b.w / 2.0),
            "right" => i.x += x1 - (b.x + b.w),
            "top" => i.y += y0 - b.y,
            "middle" => i.y += (y0 + y1) / 2.0 - (b.y + b.h / 2.0),
            "bottom" => i.y += y1 - (b.y + b.h),
            _ => {}
        }
    }
}

/// Spaces objects evenly between the first and last, across (`horizontal`) or down.
pub fn distribute(items: &mut [&mut Item], horizontal: bool) {
    if items.len() < 3 { return; }
    let key = |b: &Box2| if horizontal { b.x } else { b.y };
    let size = |b: &Box2| if horizontal { b.w } else { b.h };
    let mut order: Vec<(usize, Box2)> = items.iter().map(|i| object_bounds(i)).enumerate().collect();
    order.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
    let start = key(&order[0].1);
    let end = order.iter().map(|(_, b)| key(b) + size(b)).fold(f64::MIN, f64::max);
    let total: f64 = order.iter().map(|(_, b)| size(b)).sum();
    let gap = (end - start - total) / (order.len() - 1) as f64;
    let mut at = start;
    for (n, b) in order {
        let d = at - key(&b);
        if horizontal { items[n].x += d; } else { items[n].y += d; }
        at += size(&b) + gap;
    }
}

/// Objects hidden by collapsed mind-map nodes (`mindCollapsed`): every branch under them.
pub fn collapsed_hidden(items: &[Item]) -> HashSet<String> {
    let mut hidden: HashSet<String> = HashSet::new();
    let mut roots: Vec<String> = items.iter().filter(|i| i.flag("mindCollapsed")).map(|i| i.id.clone()).collect();
    while let Some(r) = roots.pop() {
        for i in items.iter().filter(|i| i.mind_parent() == Some(r.as_str())) {
            if hidden.insert(i.id.clone()) { roots.push(i.id.clone()); }
        }
    }
    hidden
}
