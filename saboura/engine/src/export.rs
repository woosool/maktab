//! Board export as SVG (`export-svg.js`): every object drawn as vector shapes and text. The
//! app rasterises the same SVG for PNG and PDF export.

use crate::edit::table_layout;
use crate::geometry::{self, Box2, P};
use crate::model::Item;
use std::fmt::Write;

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }

fn path_d(pts: &[P], closed: bool) -> String {
    let mut d = String::new();
    for (n, p) in pts.iter().enumerate() { let _ = write!(d, "{}{:.2} {:.2} ", if n == 0 { "M" } else { "L" }, p.0, p.1); }
    if closed { d.push('Z'); }
    d
}

fn family(i: &Item) -> &'static str {
    match i.str("fontFamily") { Some("eb-garamond") => "Liberation Serif, serif", Some("roboto-mono") => "Liberation Mono, monospace", _ => "Noto Sans, Noto Sans Arabic, sans-serif" }
}

/// Wraps text to `width` by an estimate of glyph widths (no font metrics here).
pub fn wrap(text: &str, size: f64, width: f64) -> Vec<String> {
    let per = size * 0.56;
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if candidate.chars().count() as f64 * per > width && !line.is_empty() { out.push(std::mem::take(&mut line)); line = word.to_string(); } else { line = candidate; }
        }
        out.push(line);
    }
    out
}

/// Text in a box: wrapped, aligned, centred down for notes and shapes, shrunk to fit if asked.
fn text_block(out: &mut String, i: &Item, text: &str, b: Box2, color: &str, vcenter: bool) {
    if text.is_empty() { return; }
    let mut size = i.font_size();
    let mut lines = wrap(text, size, b.w);
    if i.auto_font_size == Some(true) {
        while (lines.len() as f64 * size * 1.35 > b.h) && size > 8.0 { size = (size * 0.9).max(8.0); lines = wrap(text, size, b.w); }
    }
    let lh = size * 1.35;
    let top = if vcenter { b.y + ((b.h - lines.len() as f64 * lh) / 2.0).max(0.0) } else { b.y };
    // Right-to-left text starts at the right unless the object says otherwise.
    let rtl = text.chars().find(|c| c.is_alphabetic()).is_some_and(|c| ('\u{0590}'..='\u{08FF}').contains(&c));
    let align = if rtl && i.text_align.is_none() { "right" } else { i.align() };
    let (anchor, x) = match align { "center" => ("middle", b.x + b.w / 2.0), "right" => ("end", b.x + b.w), _ => ("start", b.x) };
    let deco = [(i.flag("underline") || i.str("link").is_some()).then_some("underline"), i.flag("strike").then_some("line-through")].into_iter().flatten().collect::<Vec<_>>().join(" ");
    let _ = write!(out, r#"<text font-family="{}" font-size="{size:.1}" fill="{color}" text-anchor="{anchor}"{}{}{}>"#, family(i),
        if i.is_bold() { r#" font-weight="700""# } else { "" }, if i.flag("italic") { r#" font-style="italic""# } else { "" },
        if deco.is_empty() { String::new() } else { format!(r#" text-decoration="{deco}""#) });
    // Styled words are tspans within each line (lines hold the text's chars in order).
    let runs = crate::rich::read(i.get("textRuns"));
    let spans = if runs.is_empty() || text != i.text { Vec::new() } else { crate::rich::spans(text, &runs) };
    let mut at = 0usize;
    for (n, l) in lines.iter().enumerate() {
        let _ = write!(out, r#"<tspan x="{x:.2}" y="{:.2}"{}>"#, top + n as f64 * lh + size * 1.05, if rtl { r#" direction="rtl""# } else { "" });
        let count = l.chars().count();
        if spans.is_empty() { out.push_str(&esc(l)); } else {
            let chars: Vec<char> = l.chars().collect();
            for (r, st) in &spans {
                let (a, b) = (r.start.max(at), r.end.min(at + count));
                if a >= b { continue; }
                let piece: String = chars[a - at..b - at].iter().collect();
                let mut attrs = String::new();
                if st.get("bold").and_then(|v| v.as_bool()) == Some(true) { attrs.push_str(r#" font-weight="700""#); }
                if st.get("italic").and_then(|v| v.as_bool()) == Some(true) { attrs.push_str(r#" font-style="italic""#); }
                let deco: Vec<&str> = [(st.get("underline").and_then(|v| v.as_bool()) == Some(true) || st.contains_key("link")).then_some("underline"), (st.get("strike").and_then(|v| v.as_bool()) == Some(true)).then_some("line-through")].into_iter().flatten().collect();
                if !deco.is_empty() { let _ = write!(attrs, r#" text-decoration="{}""#, deco.join(" ")); }
                if let Some(c) = st.get("textColor").and_then(|v| v.as_str()) { let _ = write!(attrs, r#" fill="{c}""#); }
                else if st.contains_key("link") { attrs.push_str(r##" fill="#2f6fd6""##); }
                let _ = write!(out, "<tspan{attrs}>{}</tspan>", esc(&piece));
            }
        }
        out.push_str("</tspan>");
        at += count + 1;
    }
    out.push_str("</text>");
}

fn rotate_attr(i: &Item) -> String {
    let r = i.rotation();
    if r == 0.0 { return String::new(); }
    let c = geometry::center(i);
    format!(r#" transform="rotate({r:.3} {:.2} {:.2})""#, c.0, c.1)
}

fn cap(out: &mut String, kind: &str, tip: P, from: P, width: f64, color: &str) {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let l = dx.hypot(dy);
    if l == 0.0 || kind == "none" { return; }
    let (d, n) = ((dx / l, dy / l), (-dy / l, dx / l));
    let len = 8.0 + width * 2.5;
    let at = |a: f64, b: f64| (tip.0 - d.0 * len * a + n.0 * len * b, tip.1 - d.1 * len * a + n.1 * len * b);
    let filled = !kind.ends_with("-open");
    let fill = if filled { color } else { "#ffffff" };
    match kind {
        "arrow" => { let _ = write!(out, r#"<path d="{}" fill="none" stroke="{color}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"/>"#, path_d(&[at(1.0, 0.55), tip, at(1.0, -0.55)], false)); }
        "triangle" | "triangle-open" => { let _ = write!(out, r#"<path d="{}" fill="{fill}" stroke="{color}" stroke-width="{width}"/>"#, path_d(&[tip, at(1.0, 0.55), at(1.0, -0.55)], true)); }
        "diamond" | "diamond-open" => { let _ = write!(out, r#"<path d="{}" fill="{fill}" stroke="{color}" stroke-width="{width}"/>"#, path_d(&[tip, at(0.75, 0.4), at(1.5, 0.0), at(0.75, -0.4)], true)); }
        "circle" | "circle-open" => { let c = at(0.35, 0.0); let _ = write!(out, r#"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="{fill}" stroke="{color}" stroke-width="{width}"/>"#, c.0, c.1, len * 0.35); }
        "bar" => { let _ = write!(out, r#"<path d="{}" stroke="{color}" stroke-width="{width}"/>"#, path_d(&[at(0.6, 0.5), at(0.6, -0.5)], false)); }
        "crowfoot" => { for s in [-0.55, 0.0, 0.55] { let _ = write!(out, r#"<path d="{}" stroke="{color}" stroke-width="{width}"/>"#, path_d(&[at(1.0, 0.0), at(0.0, s)], false)); } }
        _ => {}
    }
}

/// One object as SVG.
pub fn item_svg(out: &mut String, i: &Item, dark: bool) {
    let ink = if dark && matches!(i.kind.as_str(), "text" | "frame" | "arrow") { "#e8ebe6" } else { "#30342f" };
    let text_color = i.str("textColor").unwrap_or(if i.str("link").is_some() { "#2f6fd6" } else { ink }).to_string();
    let fill = i.color.clone().unwrap_or_else(|| "#f6e7a4".into());
    let rot = rotate_attr(i);
    let local = |pts: Vec<P>| -> Vec<P> { pts.into_iter().map(|(x, y)| (i.x + x, i.y + y)).collect() };
    let tb = |b: Box2| Box2 { x: i.x + b.x, y: i.y + b.y, ..b };
    match i.kind.as_str() {
        "pen" => {
            let Some(pts) = &i.points else { return };
            let p: Vec<P> = pts.iter().map(|q| (i.x + q[0], i.y + q[1])).collect();
            let _ = write!(out, r#"<path d="{}" fill="none" stroke="{fill}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"{rot}/>"#, path_d(&p, false), i.line_width.unwrap_or(3.0));
        }
        "arrow" => {
            let color = i.color.clone().unwrap_or_else(|| "#878f86".into());
            let w = i.line_width.unwrap_or(2.0);
            let pts = geometry::connector_segments(i);
            let dash = match i.str("lineStyle") { Some("dashed") => r#" stroke-dasharray="8 6""#, Some("dotted") => r#" stroke-dasharray="1 5" stroke-linecap="round""#, _ => "" };
            let _ = write!(out, r#"<path d="{}" fill="none" stroke="{color}" stroke-width="{w}" stroke-linejoin="round"{dash}/>"#, path_d(&pts, false));
            let n = pts.len();
            cap(out, i.end_cap.as_deref().unwrap_or("arrow"), pts[n - 1], pts[n - 2], w, &color);
            cap(out, i.start_cap.as_deref().unwrap_or("none"), pts[0], pts[1], w, &color);
            if !i.text.is_empty() {
                let m = pts[n / 2];
                let lines = wrap(&i.text, i.font_size(), 240.0);
                let lw = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * i.font_size() * 0.56 + 12.0;
                let lh = lines.len() as f64 * i.font_size() * 1.35 + 6.0;
                let b = Box2 { x: m.0 - lw / 2.0, y: m.1 - lh / 2.0, w: lw, h: lh };
                let _ = write!(out, r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="4" fill="{}"/>"#, b.x, b.y, b.w, b.h, if dark { "#161717" } else { "#ffffff" });
                let mut c = i.clone();
                c.text_align = Some("center".into());
                text_block(out, &c, &i.text, Box2 { x: b.x + 6.0, y: b.y + 3.0, w: b.w - 12.0, h: b.h - 6.0 }, &text_color, false);
            }
        }
        "frame" => {
            let _ = write!(out, r##"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="6" fill="{fill}" fill-opacity="0.45" stroke="#a2aaa0" stroke-dasharray="6 5"/>"##, i.x, i.y, i.w, i.h);
            let _ = write!(out, r#"<text x="{:.2}" y="{:.2}" font-family="Noto Sans, sans-serif" font-size="13" font-weight="700" fill="{}">{}</text>"#, i.x, i.y - 8.0, if dark { "#b8b8b8" } else { "#555555" }, esc(&i.text));
        }
        "image" => {
            let Some(src) = i.str("src") else { return };
            let crop = i.get("crop");
            let c = |k: &str, d: f64| crop.and_then(|c| c[k].as_f64()).unwrap_or(d);
            let (cx, cy, cw, ch) = (c("x", 0.0), c("y", 0.0), c("w", 1.0), c("h", 1.0));
            let (fw, fh) = (i.w / cw, i.h / ch);
            let clip = if crop.and_then(|c| c["mask"].as_str()) == Some("ellipse") { format!(r#"<clipPath id="c{0}"><ellipse cx="{1:.2}" cy="{2:.2}" rx="{3:.2}" ry="{4:.2}"/></clipPath>"#, i.id, i.x + i.w / 2.0, i.y + i.h / 2.0, i.w / 2.0, i.h / 2.0) }
                else { format!(r#"<clipPath id="c{}"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"/></clipPath>"#, i.id, i.x, i.y, i.w, i.h) };
            let _ = write!(out, r#"<g{rot}>{clip}<image clip-path="url(#c{})" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" preserveAspectRatio="none" href="{}"/></g>"#, i.id, i.x - cx * fw, i.y - cy * fh, fw, fh, esc(src));
        }
        "table" => {
            let (cols, rows) = table_layout(i);
            let _ = write!(out, r##"<g{rot}><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{fill}" stroke="#bfbfbf"/>"##, i.x, i.y, i.w, i.h);
            for x in &cols { let _ = write!(out, r##"<path d="M{0:.2} {1:.2}V{2:.2}" stroke="#bfbfbf"/>"##, i.x + x, i.y, i.y + i.h); }
            for y in &rows { let _ = write!(out, r##"<path d="M{0:.2} {1:.2}H{2:.2}" stroke="#bfbfbf"/>"##, i.x, i.y + y, i.x + i.w); }
            let mut c = i.clone();
            c.font_size = Some(i.font_size.unwrap_or(15.0));
            c.auto_font_size = None;
            for (r, row) in i.cells().unwrap_or_default().iter().enumerate() {
                for (k, t) in row.iter().enumerate() {
                    if k + 1 >= cols.len() || r + 1 >= rows.len() { continue; }
                    c.bold = Some(r == 0);
                    text_block(out, &c, t, Box2 { x: i.x + cols[k] + 6.0, y: i.y + rows[r] + 4.0, w: cols[k + 1] - cols[k] - 12.0, h: rows[r + 1] - rows[r] - 8.0 }, "#30342f", true);
                }
            }
            out.push_str("</g>");
        }
        kind => {
            out.push_str(&format!("<g{rot}>"));
            match kind {
                "note" => { let _ = write!(out, r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="2" fill="{fill}"/>"#, i.x, i.y, i.w, i.h); }
                "rect" | "ellipse" => {
                    let line = i.str("borderColor").unwrap_or("#878f86");
                    let dash = match i.str("lineStyle") { Some("dashed") => r#" stroke-dasharray="8 6""#, Some("dotted") => r#" stroke-dasharray="1 4""#, _ => "" };
                    let _ = write!(out, r#"<path d="{}" fill="{fill}" stroke="{line}" stroke-width="{}"{dash}/>"#, path_d(&local(geometry::item_outline(i)), true), i.line_width.unwrap_or(1.0));
                    for d in geometry::shape_details(i.shape.as_deref().unwrap_or(""), i.w, i.h) { let _ = write!(out, r#"<path d="{}" fill="none" stroke="{line}"/>"#, path_d(&local(d), false)); }
                }
                _ => {}
            }
            let b = match kind {
                "text" => Box2 { x: 4.0, y: 4.0, w: i.w - 8.0, h: i.h - 8.0 },
                "ellipse" => Box2 { x: i.w * 0.15, y: i.h * 0.15, w: i.w * 0.7, h: i.h * 0.7 },
                _ => Box2 { x: 14.0, y: 12.0, w: i.w - 28.0, h: i.h - 24.0 },
            };
            text_block(out, i, &i.text, tb(b), &text_color, kind != "text");
            out.push_str("</g>");
        }
    }
}

/// The board (or the objects given) as an SVG document with a margin around them.
pub fn to_svg(items: &[Item], dark: bool, background: bool) -> (String, Box2) {
    let b = geometry::bounds(items);
    let m = 40.0;
    let area = Box2 { x: b.x - m, y: b.y - m, w: b.w + m * 2.0, h: b.h + m * 2.0 };
    let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{:.0}" height="{:.0}" viewBox="{:.2} {:.2} {:.2} {:.2}">"#, area.w, area.h, area.x, area.y, area.w, area.h);
    if background { let _ = write!(out, r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}"/>"#, area.x, area.y, area.w, area.h, if dark { "#161717" } else { "#ffffff" }); }
    for i in geometry::stacked(items) { item_svg(&mut out, i, dark); }
    out.push_str("</svg>");
    (out, area)
}
