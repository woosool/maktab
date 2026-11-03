//! Layout of the chartex kinds (chart/ex.rs): waterfall, histogram, Pareto, box & whisker,
//! funnel on value axes; treemap (squarified) and sunburst (rings by level).

use super::{nice_scale, ticks, tick_text, value_text, Out, Prim, Rgb, GRID, INK, WHITE};
use crate::chart::ex::{bins, box_stats, LEVEL_SEP};
use crate::chart::{Chart, ChartKind, SeriesData};

/// Excel's waterfall colors come from the accents: increase, decrease, total.
fn wf_colors(c: &Chart, theme: &[[u8; 3]]) -> (Rgb, Rgb, Rgb) { (c.color(0, theme), crate::chart::palette_color(c.palette, 1, theme), crate::chart::palette_color(c.palette, 2, theme)) }

pub(super) fn draw(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    match c.kind {
        ChartKind::Treemap => treemap(o, c, data, area, theme),
        ChartKind::Sunburst => sunburst(o, c, data, area, theme),
        ChartKind::Funnel => funnel(o, c, data, area, theme),
        ChartKind::BoxWhisker => boxes(o, c, data, area, theme),
        _ => bars(o, c, data, area, theme),
    }
}

/// A value axis on the left (and optional percent axis on the right) with category labels
/// below; returns the plot rectangle and the value → y map's (min, max).
fn frame(o: &mut Out, c: &Chart, area: [f32; 4], lo: f64, hi: f64, cats: &[String], right_pct: bool) -> ([f32; 4], f64, f64) {
    let z = o.z;
    let small = 11.0 * z;
    let (min, max, step) = nice_scale(lo, hi, 5);
    let tv = ticks(min, max, step);
    let lw = tv.iter().map(|v| o.w(&tick_text(*v, step, false), small, false)).fold(0.0, f32::max) + 6.0 * z;
    let rw = if right_pct { o.w("100%", small, false) + 6.0 * z } else { 0.0 };
    let lh = o.h(small);
    let plot = [area[0] + lw, area[1] + lh / 2.0, area[2] - rw - 4.0 * z, area[3] - lh - 6.0 * z];
    if plot[2] <= plot[0] || plot[3] <= plot[1] { return (plot, min, max); }
    let y = |v: f64| plot[3] - ((v - min) / (max - min)) as f32 * (plot[3] - plot[1]);
    for v in &tv {
        let yy = y(*v);
        if c.gridlines { o.line([plot[0], yy], [plot[2], yy], o.hair(), GRID); }
        o.text(&tick_text(*v, step, false), [plot[0] - 4.0 * z, yy], small, false, INK, 2, 1);
    }
    if right_pct { for k in 0..=4 { let t = k as f64 / 4.0; let yy = plot[3] - t as f32 * (plot[3] - plot[1]); o.text(&format!("{}%", k * 25), [plot[2] + 4.0 * z, yy], small, false, INK, 0, 1); } }
    let n = cats.len().max(1);
    let slot = (plot[2] - plot[0]) / n as f32;
    let every = (cats.iter().map(|t| o.w(t, small, false)).fold(0.0, f32::max) / slot.max(1.0)).ceil().max(1.0) as usize;
    for (i, t) in cats.iter().enumerate() { if i % every == 0 { o.text(t, [plot[0] + (i as f32 + 0.5) * slot, plot[3] + 3.0 * z], small, false, INK, 1, 0); } }
    o.line([plot[0], y(min.max(0.0).min(max))], [plot[2], y(min.max(0.0).min(max))], o.hair(), GRID);
    (plot, min, max)
}

fn leaf(t: &str) -> &str { t.rsplit(LEVEL_SEP).next().unwrap_or(t) }

/// Waterfall, histogram and Pareto: columns on a value axis.
fn bars(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    let z = o.z;
    let Some(d) = data.first() else { return };
    // (label, from, to, color) per column, and Pareto's cumulative share.
    let mut cols: Vec<(String, f64, f64, Rgb)> = vec![];
    let mut cum: Vec<f64> = vec![];
    match c.kind {
        ChartKind::Waterfall => {
            let (up, down, total) = wf_colors(c, theme);
            let mut run = 0.0;
            for (i, v) in d.vals.iter().enumerate() {
                let v = v.unwrap_or(0.0);
                let label = d.cats.get(i).map(|t| leaf(t).to_string()).unwrap_or_else(|| format!("{}", i + 1));
                if c.ex.subtotals.contains(&i) { run = v; cols.push((label, 0.0, v, total)); }
                else { cols.push((label, run, run + v, if v >= 0.0 { up } else { down })); run += v; }
            }
        }
        ChartKind::Histogram => {
            let v: Vec<f64> = d.vals.iter().flatten().copied().collect();
            for (k, (a, b, n)) in bins(&v, c.ex.bin_width, c.ex.bin_count).into_iter().enumerate() {
                let l = if k == 0 { format!("[{}, {}]", value_text(round(a)), value_text(round(b))) } else { format!("({}, {}]", value_text(round(a)), value_text(round(b))) };
                cols.push((l, 0.0, n as f64, c.color(0, theme)));
            }
        }
        _ => {
            // Pareto: categories summed, sorted descending, with the running share.
            let mut agg: Vec<(String, f64)> = vec![];
            for (i, v) in d.vals.iter().enumerate() {
                let k = d.cats.get(i).map(|t| leaf(t).to_string()).unwrap_or_else(|| format!("{}", i + 1));
                match agg.iter_mut().find(|x| x.0 == k) { Some(x) => x.1 += v.unwrap_or(0.0), None => agg.push((k, v.unwrap_or(0.0))) }
            }
            agg.sort_by(|a, b| b.1.total_cmp(&a.1));
            let total: f64 = agg.iter().map(|x| x.1.max(0.0)).sum();
            let mut acc = 0.0;
            for (k, v) in agg { acc += v.max(0.0); cum.push(if total > 0.0 { acc / total } else { 0.0 }); cols.push((k, 0.0, v, c.color(0, theme))); }
        }
    }
    if cols.is_empty() { return; }
    let lo = cols.iter().map(|x| x.1.min(x.2)).fold(0.0, f64::min);
    let hi = cols.iter().map(|x| x.1.max(x.2)).fold(0.0, f64::max);
    let cats: Vec<String> = cols.iter().map(|x| x.0.clone()).collect();
    let (plot, min, max) = frame(o, c, area, lo, hi, &cats, !cum.is_empty());
    if plot[2] <= plot[0] || plot[3] <= plot[1] { return; }
    let y = |v: f64| plot[3] - ((v - min) / (max - min)) as f32 * (plot[3] - plot[1]);
    let slot = (plot[2] - plot[0]) / cols.len() as f32;
    let gap = if c.kind == ChartKind::Waterfall { slot * 0.25 } else { 0.5 * z };
    o.series(Some(0));
    for (i, (_, a, b, col)) in cols.iter().enumerate() {
        let x0 = plot[0] + i as f32 * slot + gap / 2.0;
        let r = [x0, y(a.max(*b)), x0 + slot - gap, y(a.min(*b))];
        o.v.push(Prim::Rect { r, fill: Some(*col), stroke: (c.kind != ChartKind::Waterfall).then_some((WHITE, o.hair())) });
        if c.data_labels { let v = if c.kind == ChartKind::Waterfall && !c.ex.subtotals.contains(&i) { b - a } else { *b }; o.text(&value_text(v), [x0 + (slot - gap) / 2.0, r[1] - 2.0 * z], 10.0 * z, false, INK, 1, 2); }
        // Waterfall connectors.
        if c.kind == ChartKind::Waterfall && i + 1 < cols.len() { o.line([x0 + slot - gap, y(*b)], [x0 + slot + gap / 2.0 + gap / 2.0, y(*b)], o.hair(), GRID); }
    }
    if !cum.is_empty() {
        let col = crate::chart::palette_color(c.palette, 1, theme);
        let pts: Vec<[f32; 2]> = cum.iter().enumerate().map(|(i, p)| [plot[0] + (i as f32 + 0.5) * slot, plot[3] - *p as f32 * (plot[3] - plot[1])]).collect();
        o.series(Some(1));
        o.v.push(Prim::Line { pts: pts.clone(), color: col, width: 2.25 * z });
        for p in pts { o.v.push(Prim::Poly { pts: super::ellipse(p, 2.5 * z, 2.5 * z, 12), fill: Some(col), stroke: None, opacity: 1.0, strip: false }); }
    }
    o.series(None);
}

fn round(v: f64) -> f64 { (v * 1e6).round() / 1e6 }

/// Box & whisker: per category (or one), a box per series.
fn boxes(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    let z = o.z;
    // Categories: distinct labels in order (none: one group).
    let mut cats: Vec<String> = vec![];
    for d in data { for t in &d.cats { if !cats.contains(t) { cats.push(t.clone()); } } }
    if cats.is_empty() { cats.push(String::new()); }
    let sample = |d: &SeriesData, cat: &str| -> Vec<f64> { d.vals.iter().enumerate().filter(|(i, _)| d.cats.is_empty() || d.cats.get(*i).map(|s| s.as_str()) == Some(cat)).filter_map(|(_, v)| *v).collect() };
    let all: Vec<f64> = data.iter().flat_map(|d| d.vals.iter().flatten().copied()).collect();
    if all.is_empty() { return; }
    let (lo, hi) = all.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    let shown: Vec<String> = cats.iter().map(|t| leaf(t).to_string()).collect();
    let (plot, min, max) = frame(o, c, area, lo.min(0.0), hi, &shown, false);
    if plot[2] <= plot[0] || plot[3] <= plot[1] { return; }
    let y = |v: f64| plot[3] - ((v - min) / (max - min)) as f32 * (plot[3] - plot[1]);
    let slot = (plot[2] - plot[0]) / cats.len() as f32;
    let n = data.len().max(1) as f32;
    let bw = slot * 0.7 / n;
    for (si, d) in data.iter().enumerate() {
        let col = c.color(si, theme);
        o.series(Some(si));
        for (ci, cat) in cats.iter().enumerate() {
            let Some((wl, q1, med, q3, wh, mean, out)) = box_stats(&sample(d, cat), c.ex.inclusive) else { continue };
            let x0 = plot[0] + ci as f32 * slot + slot * 0.15 + si as f32 * bw;
            let xm = x0 + bw / 2.0;
            o.line([xm, y(wh)], [xm, y(q3)], o.hair() * 1.5, INK);
            o.line([xm, y(q1)], [xm, y(wl)], o.hair() * 1.5, INK);
            o.line([xm - bw / 4.0, y(wh)], [xm + bw / 4.0, y(wh)], o.hair() * 1.5, INK);
            o.line([xm - bw / 4.0, y(wl)], [xm + bw / 4.0, y(wl)], o.hair() * 1.5, INK);
            o.v.push(Prim::Rect { r: [x0 + bw * 0.1, y(q3), x0 + bw * 0.9, y(q1)], fill: Some(col), stroke: Some((INK, o.hair())) });
            o.line([x0 + bw * 0.1, y(med)], [x0 + bw * 0.9, y(med)], 1.5 * z, WHITE);
            if !c.ex.hide_mean { let (my, d2) = (y(mean), 3.0 * z); o.line([xm - d2, my - d2], [xm + d2, my + d2], 1.5 * z, WHITE); o.line([xm - d2, my + d2], [xm + d2, my - d2], 1.5 * z, WHITE); }
            if !c.ex.hide_outliers { for v in out { o.v.push(Prim::Poly { pts: super::ellipse([xm, y(v)], 2.5 * z, 2.5 * z, 12), fill: None, stroke: Some((col, z)), opacity: 1.0, strip: false }); } }
        }
    }
    o.series(None);
}

/// Funnel: centered horizontal bars, widest value full width, labels inside.
fn funnel(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    let z = o.z;
    let Some(d) = data.first() else { return };
    let n = d.vals.len();
    if n == 0 { return; }
    let small = 11.0 * z;
    let labels: Vec<String> = (0..n).map(|i| d.cats.get(i).map(|t| leaf(t).to_string()).unwrap_or_default()).collect();
    let lw = labels.iter().map(|t| o.w(t, small, false)).fold(0.0, f32::max) + 8.0 * z;
    let plot = [area[0] + lw, area[1], area[2], area[3]];
    let maxv = d.vals.iter().flatten().fold(0.0f64, |a, &b| a.max(b.abs()));
    if maxv <= 0.0 || plot[2] <= plot[0] { return; }
    let row = (plot[3] - plot[1]) / n as f32;
    let cx = (plot[0] + plot[2]) / 2.0;
    o.series(Some(0));
    for (i, v) in d.vals.iter().enumerate() {
        let v = v.unwrap_or(0.0).abs();
        let w = (v / maxv) as f32 * (plot[2] - plot[0]);
        let y0 = plot[1] + i as f32 * row + row * 0.04;
        o.rect([cx - w / 2.0, y0, cx + w / 2.0, y0 + row * 0.92], c.color(0, theme));
        o.text(&value_text(v), [cx, y0 + row * 0.46], small, false, WHITE, 1, 1);
        o.text(&labels[i], [plot[0] - 4.0 * z, y0 + row * 0.46], small, false, INK, 2, 1);
    }
    o.series(None);
}

/// Leaves (path, value) grouped by their top level, in first-seen order.
fn tops(d: &SeriesData) -> Vec<(String, Vec<(String, f64)>)> {
    let mut out: Vec<(String, Vec<(String, f64)>)> = vec![];
    for (i, v) in d.vals.iter().enumerate() {
        let v = v.unwrap_or(0.0);
        if v <= 0.0 { continue; }
        let p = d.cats.get(i).cloned().unwrap_or_else(|| format!("{}", i + 1));
        let top = p.split(LEVEL_SEP).next().unwrap_or("").to_string();
        match out.iter_mut().find(|x| x.0 == top) { Some(x) => x.1.push((p, v)), None => out.push((top, vec![(p, v)])) }
    }
    out
}

/// Squarified treemap: rectangles for `vals` (descending) filling `r`.
fn squarify(vals: &[f64], r: [f32; 4]) -> Vec<[f32; 4]> {
    let total: f64 = vals.iter().sum();
    let mut out = vec![[0.0; 4]; vals.len()];
    if total <= 0.0 { return out; }
    let scale = ((r[2] - r[0]) * (r[3] - r[1])) as f64 / total;
    let mut rect = r;
    let mut i = 0;
    let worst = |row: &[f64], side: f64| -> f64 { let s: f64 = row.iter().sum(); let (mx, mn) = row.iter().fold((0.0f64, f64::MAX), |(a, b), &x| (a.max(x), b.min(x))); (side * side * mx / (s * s)).max(s * s / (side * side * mn)) };
    while i < vals.len() {
        let (w, h) = ((rect[2] - rect[0]) as f64, (rect[3] - rect[1]) as f64);
        let side = w.min(h).max(1e-6);
        let mut j = i + 1;
        while j < vals.len() {
            let a: Vec<f64> = vals[i..j].iter().map(|v| v * scale).collect();
            let b: Vec<f64> = vals[i..=j].iter().map(|v| v * scale).collect();
            if worst(&b, side) > worst(&a, side) { break; }
            j += 1;
        }
        let row: f64 = vals[i..j].iter().map(|v| v * scale).sum();
        let thick = (row / side) as f32;
        let mut at = 0.0f32;
        for k in i..j {
            let len = (vals[k] * scale / row * side) as f32;
            out[k] = if w >= h { [rect[0], rect[1] + at, rect[0] + thick, rect[1] + at + len] } else { [rect[0] + at, rect[1], rect[0] + at + len, rect[1] + thick] };
            at += len;
        }
        if w >= h { rect[0] += thick; } else { rect[1] += thick; }
        i = j;
    }
    out
}

fn treemap(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    let z = o.z;
    let Some(d) = data.first() else { return };
    let mut groups = tops(d);
    groups.sort_by(|a, b| b.1.iter().map(|x| x.1).sum::<f64>().total_cmp(&a.1.iter().map(|x| x.1).sum::<f64>()));
    let sums: Vec<f64> = groups.iter().map(|g| g.1.iter().map(|x| x.1).sum()).collect();
    let small = 10.0 * z;
    o.series(Some(0));
    for (gi, (g, r)) in groups.iter_mut().zip(squarify(&sums, area)).enumerate() {
        let col = c.color(gi, theme);
        g.1.sort_by(|a, b| b.1.total_cmp(&a.1));
        let vals: Vec<f64> = g.1.iter().map(|x| x.1).collect();
        let depth = g.1.iter().any(|x| x.0.contains(LEVEL_SEP));
        for ((p, v), lr) in g.1.iter().zip(squarify(&vals, r)) {
            o.v.push(Prim::Rect { r: lr, fill: Some(col), stroke: Some((WHITE, z)) });
            if lr[2] - lr[0] > 20.0 * z && lr[3] - lr[1] > 14.0 * z {
                let label = if c.data_labels { format!("{}\n{}", leaf(p), value_text(*v)) } else { leaf(p).to_string() };
                let mut yy = lr[1] + 3.0 * z + if depth && lr[1] == r[1] { o.h(small) } else { 0.0 };
                for l in label.lines() { o.text(l, [lr[0] + 3.0 * z, yy], small, false, WHITE, 0, 0); yy += o.h(small); }
            }
        }
        if depth { o.text(&g.0, [r[0] + 3.0 * z, r[1] + 2.0 * z], small, true, WHITE, 0, 0); }
    }
    o.series(None);
}

fn sunburst(o: &mut Out, c: &Chart, data: &[SeriesData], area: [f32; 4], theme: &[[u8; 3]]) {
    let z = o.z;
    let Some(d) = data.first() else { return };
    let groups = tops(d);
    let total: f64 = groups.iter().flat_map(|g| g.1.iter().map(|x| x.1)).sum();
    if total <= 0.0 { return; }
    let depth = groups.iter().flat_map(|g| g.1.iter().map(|x| x.0.split(LEVEL_SEP).count())).max().unwrap_or(1);
    let (cx, cy) = ((area[0] + area[2]) / 2.0, (area[1] + area[3]) / 2.0);
    let rad = (area[2] - area[0]).min(area[3] - area[1]) / 2.0 * 0.95;
    let hole = rad * 0.12;
    let ring = (rad - hole) / depth as f32;
    let at = |a: f32, r: f32| [cx + a.cos() * r, cy + a.sin() * r];
    // Segments per level: (level, start angle, sweep, label, color).
    let mut segs: Vec<(usize, f32, f32, String, Rgb)> = vec![];
    let mut a0 = -std::f32::consts::FRAC_PI_2;
    for (gi, (_, leaves)) in groups.iter().enumerate() {
        let col = c.color(gi, theme);
        for lv in 0..depth {
            // Consecutive leaves sharing the path prefix up to this level merge into one segment.
            let mut a = a0;
            let mut k = 0;
            while k < leaves.len() {
                let parts: Vec<&str> = leaves[k].0.split(LEVEL_SEP).collect();
                if parts.len() <= lv { a += (leaves[k].1 / total) as f32 * std::f32::consts::TAU; k += 1; continue; }
                let key = parts[..=lv].join("/");
                let mut sum = 0.0;
                while k < leaves.len() && { let p: Vec<&str> = leaves[k].0.split(LEVEL_SEP).collect(); p.len() > lv && p[..=lv].join("/") == key } { sum += leaves[k].1; k += 1; }
                let sweep = (sum / total) as f32 * std::f32::consts::TAU;
                let shade = 1.0 - 0.18 * lv as f32;
                segs.push((lv, a, sweep, parts[lv].to_string(), col.map(|x| (x as f32 * shade + 255.0 * (1.0 - shade)) as u8)));
                a += sweep;
            }
        }
        a0 += (leaves.iter().map(|x| x.1).sum::<f64>() / total) as f32 * std::f32::consts::TAU;
    }
    o.series(Some(0));
    let small = 10.0 * z;
    for (lv, a, sweep, label, col) in segs {
        let (r0, r1) = (hole + lv as f32 * ring, hole + (lv + 1) as f32 * ring);
        let steps = ((sweep / 0.06).ceil() as usize).max(1);
        let mut pts: Vec<[f32; 2]> = (0..=steps).map(|k| at(a + sweep * k as f32 / steps as f32, r1)).collect();
        pts.extend((0..=steps).rev().map(|k| at(a + sweep * k as f32 / steps as f32, r0)));
        o.v.push(Prim::Poly { pts, fill: Some(col), stroke: None, opacity: 1.0, strip: true });
        o.line(at(a, r0), at(a, r1), 1.5 * z, WHITE);
        if sweep * (r0 + r1) / 2.0 > o.w(&label, small, false) * 0.8 { o.text(&label, at(a + sweep / 2.0, (r0 + r1) / 2.0), small, false, WHITE, 1, 1); }
    }
    o.series(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    struct M;
    impl crate::chart::Measure for M { fn size(&self, t: &str, px: f32, _: bool) -> [f32; 2] { [t.chars().count() as f32 * px * 0.5, px * 1.2] } }

    #[test]
    fn each_kind_lays_out() {
        let d = SeriesData { name: "S".into(), cats: vec![format!("A{LEVEL_SEP}x"), format!("A{LEVEL_SEP}y"), format!("B{LEVEL_SEP}z"), "C".into()], vals: vec![Some(5.0), Some(3.0), Some(-2.0), Some(4.0)], ..Default::default() };
        for k in [ChartKind::Waterfall, ChartKind::Histogram, ChartKind::Pareto, ChartKind::BoxWhisker, ChartKind::Treemap, ChartKind::Sunburst, ChartKind::Funnel] {
            let c = Chart { kind: k, gridlines: true, series: vec![Default::default()], ..Default::default() };
            let p = crate::chart::layout(&c, std::slice::from_ref(&d), [0.0, 0.0, 400.0, 300.0], &[], 1.0, &M);
            assert!(p.iter().filter(|x| matches!(x, Prim::Rect { .. } | Prim::Poly { .. })).count() >= 3, "{k:?} draws");
        }
        let r = squarify(&[6.0, 3.0, 1.0], [0.0, 0.0, 100.0, 100.0]);
        let area: f32 = r.iter().map(|x| (x[2] - x[0]) * (x[3] - x[1])).sum();
        assert!((area - 10000.0).abs() < 1.0);
    }
}
