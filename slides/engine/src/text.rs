//! Text layout for shapes and table cells (text-layout.ts): runs are shaped with the word
//! engine's fonts (rustybuzz, with per-character fallback), broken into lines at spaces,
//! given bullets, indents, spacing and alignment, then placed in the box by its anchor and
//! autofit. Units are points; (0, 0) is the box's top-left corner.

use crate::scene::{Align, Anchor, Autofit, Bullet, Para, Spacing, TextBody};
use crate::Rgba;
use word_engine::fonts::{self, FontKey};

/// The face to resolve for a typeface name (the word engine substitutes metric-compatible
/// faces for Office fonts that aren't installed).
pub fn map_font(name: &str) -> &str { if name.is_empty() { "Calibri" } else { name } }

pub fn font_key(name: &str, bold: bool, italic: bool) -> FontKey { fonts::resolve(map_font(name), bold, italic) }

#[derive(Clone, Debug, PartialEq)]
pub struct Glyph { pub gid: u16, pub x: f64, pub y: f64 }

/// Glyphs of one face, size and color, positioned on a line.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    pub face: u16,
    pub synthetic_bold: bool,
    pub italic_skew: bool,
    /// Font size in points.
    pub size: f64,
    pub color: Rgba,
    pub glyphs: Vec<Glyph>,
    /// Decoration: (x0, x1) span at the baseline.
    pub underline: bool,
    pub strike: bool,
    pub span: (f64, f64),
    pub baseline: f64,
    pub highlight: Option<Rgba>,
    pub fx: RunFx,
}

/// A run's text effects (WordArt): outline, shadow, glow, gradient fill, double strike.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RunFx {
    pub outline: Option<(Rgba, f64)>,
    pub shadow: Option<crate::scene::Shadow>,
    pub glow: Option<(Rgba, f64)>,
    pub gradient: Option<Vec<(f32, Rgba)>>,
    pub double_strike: bool,
    pub double_underline: bool,
}

/// Where each character of the body's text (paragraphs joined by \n) is: line index and x.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CharBox { pub line: usize, pub x0: f64, pub x1: f64 }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Line { pub top: f64, pub height: f64, pub baseline: f64, pub x0: f64, pub x1: f64, pub para: usize, pub first_char: usize, pub end_char: usize }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Layout {
    pub runs: Vec<GlyphRun>,
    pub lines: Vec<Line>,
    /// One per character of the text (including the \n between paragraphs).
    pub chars: Vec<CharBox>,
    /// Height of the laid-out text (points).
    pub height: f64,
    /// The scale autofit applied (1 = none).
    pub scale: f64,
}

impl Layout {
    /// The character index nearest a point (box coordinates, points).
    pub fn hit(&self, x: f64, y: f64) -> usize {
        if self.lines.is_empty() { return 0; }
        let li = self.lines.iter().position(|l| y < l.top + l.height).unwrap_or(self.lines.len() - 1);
        let line = &self.lines[li];
        let mut best = line.first_char;
        let mut best_d = f64::MAX;
        for i in line.first_char..=line.end_char {
            let cx = self.caret_x(i, li);
            let d = (cx - x).abs();
            if d < best_d { best_d = d; best = i; }
        }
        best
    }
    /// The caret's x for a character index on a line.
    pub fn caret_x(&self, i: usize, line: usize) -> f64 {
        let l = &self.lines[line];
        if i < self.chars.len() && self.chars[i].line == line && i < l.end_char { self.chars[i].x0 }
        else if i > 0 && i - 1 < self.chars.len() && self.chars[i - 1].line == line { self.chars[i - 1].x1 }
        else { l.x0 }
    }
    /// The caret position (x, top, height) for a character index.
    pub fn caret(&self, i: usize) -> (f64, f64, f64) {
        if self.lines.is_empty() { return (0.0, 0.0, 14.0); }
        let li = if i < self.chars.len() { self.chars[i].line } else { self.lines.len() - 1 };
        // At a line end that wraps, the caret belongs to the next line's start unless it's the paragraph end.
        let li = self.lines.iter().position(|l| i >= l.first_char && i <= l.end_char).unwrap_or(li);
        let l = &self.lines[li];
        (self.caret_x(i, li), l.top, l.height)
    }
}

struct Piece { text: Vec<char>, key: FontKey, size: f64, color: Rgba, underline: bool, strike: bool, baseline: f64, highlight: Option<Rgba>, spacing: f64, italic: bool, bold: bool, first: usize, level: u8, fx: RunFx }

fn shape_piece(p: &Piece, scale: f64) -> Vec<(u16, f64, f64, f64, usize)> {
    // (gid, advance, dx, dy, char index) per glyph, in points.
    let text: String = p.text.iter().collect();
    let size = p.size * scale * if p.baseline != 0.0 { 0.65 } else { 1.0 };
    let rtl = p.level % 2 == 1;
    let shaped = fonts::shape(p.key, &text, rtl, size as f32);
    let offsets: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let mut out: Vec<(usize, (u16, f64, f64, f64, usize))> = shaped.iter().enumerate().map(|(k, g)| {
        let ci = offsets.iter().position(|&b| b == g.cluster).unwrap_or(0);
        (k, (g.gid, g.advance as f64 + p.spacing * scale, g.dx as f64, g.dy as f64, p.first + ci))
    }).collect();
    // Glyphs in reading (logical) order for line breaking; a right-to-left run comes back in
    // visual order, so within a cluster keep the reverse of the shaper's order (placing the
    // run reversed restores it).
    out.sort_by(|a, b| a.1.4.cmp(&b.1.4).then(if rtl { b.0.cmp(&a.0) } else { a.0.cmp(&b.0) }));
    out.into_iter().map(|(_, g)| g).collect()
}

fn spacing_pts(s: Spacing, size: f64) -> f64 { match s { Spacing::Auto => 0.0, Spacing::Points(p) => p, Spacing::Percent(f) => f * size * 1.2 } }

/// Lay out a body in a box of `w` × `h` points.
pub fn layout(body: &TextBody, w: f64, h: f64) -> Layout {
    let (scale, reduction) = match body.autofit { Autofit::Shrink { font_scale, line_reduction } => (font_scale, line_reduction), _ => (1.0, 0.0) };
    let mut out = layout_scaled(body, w, scale, reduction);
    // Shrink-on-overflow: when the text no longer fits, shrink until it does (as PowerPoint
    // recomputes fontScale while you type).
    if matches!(body.autofit, Autofit::Shrink { .. }) && out.height > h && h > 0.0 {
        let mut s = scale;
        while out.height > h && s > 0.25 {
            s -= 0.05;
            out = layout_scaled(body, w, s, (reduction + 0.1).min(0.2));
        }
    }
    let _ = h;
    out
}

fn layout_scaled(body: &TextBody, w: f64, scale: f64, reduction: f64) -> Layout {
    let mut out = Layout { scale, ..Default::default() };
    let mut y = 0.0;
    let mut char_base = 0usize;
    for (pi, para) in body.paras.iter().enumerate() {
        let para_len: usize = para.runs.iter().map(|r| if r.br { 1 } else { r.text.chars().count() }).sum();
        let first_size = para.runs.iter().find(|r| !r.br && !r.text.is_empty()).map(|r| r.size).unwrap_or(para.end_size) * scale;
        if pi > 0 { y += spacing_pts(para.before, first_size); } else if !matches!(para.before, Spacing::Auto) { y += spacing_pts(para.before, first_size); }
        lay_para(para, pi, w, scale, reduction, &mut y, char_base, &mut out);
        y += spacing_pts(para.after, first_size);
        char_base += para_len;
        // The paragraph break character.
        if pi + 1 < body.paras.len() {
            let li = out.lines.len().saturating_sub(1);
            let x = out.lines.last().map(|l| l.x1).unwrap_or(0.0);
            out.chars.push(CharBox { line: li, x0: x, x1: x });
            char_base += 1;
        }
    }
    out.height = y;
    out
}

#[allow(clippy::too_many_arguments)]
fn lay_para(para: &Para, pi: usize, w: f64, scale: f64, reduction: f64, y: &mut f64, char_base: usize, out: &mut Layout) {
    let margin = para.margin / 12_700.0;
    let indent = para.indent / 12_700.0;
    // Pieces: runs split by font fallback; line breaks as their own markers.
    let mut pieces: Vec<Piece> = vec![];
    let mut breaks: Vec<usize> = vec![]; // char indices of a:br
    // Bidi embedding levels for the paragraph (line breaks as line separators).
    let plain: String = para.runs.iter().map(|r| if r.br { "\u{2028}".to_string() } else if r.caps { r.text.to_uppercase() } else { r.text.clone() }).collect();
    let levels: Vec<u8> = {
        let base = if para.rtl { Some(unicode_bidi::Level::rtl()) } else { None };
        let info = unicode_bidi::BidiInfo::new(&plain, base);
        plain.char_indices().map(|(b, _)| info.levels[b].number()).collect()
    };
    let base_level: u8 = if para.rtl { 1 } else { levels.iter().copied().min().unwrap_or(0).min(1) };
    let level_at = |c: usize| levels.get(c - char_base).copied().unwrap_or(base_level);
    let mut ci = char_base;
    for r in &para.runs {
        if r.br { breaks.push(ci); ci += 1; continue; }
        let text: Vec<char> = if r.caps { r.text.to_uppercase().chars().collect() } else { r.text.chars().collect() };
        let base = font_key(&r.font, r.bold, r.italic);
        let mut start = 0;
        while start < text.len() {
            let k0 = fonts::fallback(base, text[start]);
            let lv = level_at(ci + start);
            let mut end = start + 1;
            // Spaces stay with the run around them when its face has them (so a phrase in one
            // script shapes as one run); runs split where the face or direction changes.
            while end < text.len() && level_at(ci + end) == lv && (fonts::fallback(base, text[end]).face == k0.face || (text[end].is_whitespace() && fonts::has_glyph(k0.face, text[end]))) { end += 1; }
            pieces.push(Piece { text: text[start..end].to_vec(), key: k0, size: r.size, color: r.color, underline: r.underline, strike: r.strike, baseline: r.baseline, highlight: r.highlight, spacing: r.spacing, italic: r.italic, bold: r.bold, first: ci + start, level: lv, fx: RunFx { outline: r.outline, shadow: r.shadow.clone(), glow: r.glow, gradient: r.gradient.clone(), double_strike: r.double_strike, double_underline: r.underline_style == "dbl" } });
            start = end;
        }
        ci += text.len();
    }
    let para_end = ci;
    // Glyphs with their char index and piece.
    struct G { gid: u16, adv: f64, dx: f64, dy: f64, ch: usize, piece: usize }
    let mut glyphs: Vec<G> = vec![];
    for (k, p) in pieces.iter().enumerate() {
        for (gid, adv, dx, dy, ch) in shape_piece(p, scale) { glyphs.push(G { gid, adv, dx, dy, ch, piece: k }); }
    }
    let char_of = |c: usize| -> char {
        let mut i = char_base;
        for r in &para.runs { if r.br { if i == c { return '\n'; } i += 1; continue; } let n = r.text.chars().count(); if c < i + n { return r.text.chars().nth(c - i).unwrap_or(' '); } i += n; }
        ' '
    };
    // Bullet.

    let bullet: Option<(String, FontKey, f64, Rgba)> = para.bullet.as_ref().map(|b| match b {
        Bullet::Char { ch, font, color, size } => (ch.clone(), font_key(font.as_deref().unwrap_or("Arial"), false, false), size * scale, color.unwrap_or(Rgba::BLACK)),
        Bullet::Number { scheme, value, color, size } => (crate::scene::number_text(scheme, *value), pieces.first().map(|p| p.key).unwrap_or(font_key("", false, false)), size * scale, color.unwrap_or(Rgba::BLACK)),
    });
    // With a bullet, the first line's text starts at the margin (a hanging indent), or just
    // after the bullet when that is further right.
    let bullet_w = bullet.as_ref().map(|(t, k, sz, _)| fonts::shape(*k, t, false, *sz as f32).iter().map(|g| g.advance as f64).sum::<f64>()).unwrap_or(0.0);
    let first_x = if bullet.is_some() && !pieces.is_empty() { (margin + indent + bullet_w + 2.0).max(margin) } else { margin + indent };
    // Break into lines.
    let line_mult = match para.line { Spacing::Percent(f) => (f - reduction).max(0.5), _ => 1.0 - reduction };
    let mut lines: Vec<(usize, usize)> = vec![]; // glyph ranges
    let mut i = 0;
    while i < glyphs.len() || lines.is_empty() {
        let first_line = lines.is_empty();
        let start_x = if first_line { first_x } else { margin };
        let avail = (w - start_x).max(1.0);
        let mut x = 0.0;
        let mut j = i;
        let mut last_space: Option<usize> = None;
        while j < glyphs.len() {
            let g = &glyphs[j];
            if breaks.iter().any(|&b| b < g.ch && (j == i || glyphs[j - 1].ch < b)) && j > i { break; }
            let c = char_of(g.ch);
            if x + g.adv > avail && j > i && !c.is_whitespace() {
                if let Some(s) = last_space { j = s + 1; }
                break;
            }
            if c.is_whitespace() { last_space = Some(j); }
            x += g.adv;
            j += 1;
        }
        if j == i && i < glyphs.len() { j = i + 1; }
        lines.push((i, j));
        if j >= glyphs.len() { break; }
        i = j;
    }
    let ascent_of = |p: &Piece| { let m = fonts::metrics(p.key); (m.ascent as f64 * p.size * scale, m.descent.abs() as f64 * p.size * scale) };
    let empty_size = para.end_size * scale;
    let empty_key = font_key(&para.end_font, false, false);
    let em = fonts::metrics(empty_key);
    for (li, &(a, b)) in lines.iter().enumerate() {
        let (mut asc, mut desc) = (em.ascent as f64 * empty_size, em.descent.abs() as f64 * empty_size);
        if b > a { asc = 0.0; desc = 0.0; for g in &glyphs[a..b] { let (x, d) = ascent_of(&pieces[g.piece]); asc = asc.max(x); desc = desc.max(d); } }
        let natural = (asc + desc) * 1.2 / 1.0;
        let height = match para.line { Spacing::Points(p) => p * scale, _ => natural * line_mult.max(0.1) * 0.9 + (asc + desc) * 0.1 * line_mult };
        let baseline = *y + asc + (height - (asc + desc) * 1.0).max(0.0) * 0.35;
        let start_x = if li == 0 { first_x } else { margin };
        // Trailing spaces don't count for alignment.
        let mut width: f64 = glyphs[a..b].iter().map(|g| g.adv).sum();
        let mut k = b;
        while k > a && char_of(glyphs[k - 1].ch).is_whitespace() { k -= 1; width -= glyphs[k].adv; }
        let avail = (w - start_x).max(0.0);
        let mut x = start_x + match para.align { Align::Center => (avail - width) / 2.0, Align::Right => avail - width, _ => 0.0 };
        if para.rtl && para.align == Align::Left { x = start_x + avail - width; }
        let justify_gap = if para.align == Align::Justify && li + 1 < lines.len() { let spaces = glyphs[a..k].iter().filter(|g| char_of(g.ch) == ' ').count(); if spaces > 0 { (avail - width) / spaces as f64 } else { 0.0 } } else { 0.0 };
        let line_index = out.lines.len();
        // Bullet on the first line, hanging at the margin + indent.
        if li == 0 && b > a {
            if let Some((text, key, size, color)) = &bullet {
                let shaped = fonts::shape(*key, text, false, *size as f32);
                let mut bx = margin + indent;
                let mut gl = vec![];
                for g in shaped.iter() { gl.push(Glyph { gid: g.gid, x: bx + g.dx as f64, y: baseline - g.dy as f64 }); bx += g.advance as f64; }
                out.runs.push(GlyphRun { face: key.face, synthetic_bold: key.synthetic_bold, italic_skew: false, size: *size, color: *color, glyphs: gl, underline: false, strike: false, span: (0.0, 0.0), baseline, highlight: None, fx: RunFx::default() });
            }
        }
        let first_char = if b > a { glyphs[a].ch.min(glyphs[a..b].iter().map(|g| g.ch).min().unwrap_or(glyphs[a].ch)) } else if li == 0 { char_base } else { out.lines.last().map(|l| l.end_char).unwrap_or(char_base) };
        let mut current: Option<GlyphRun> = None;
        let mut char_boxes: Vec<(usize, f64, f64)> = vec![];
        // Visual order (UAX #9, L2): segments by piece, reversed from the highest level down
        // to the lowest odd one; right-to-left segments run backwards.
        let mut segs: Vec<(usize, usize, u8)> = vec![];
        for (k, g) in glyphs[a..b].iter().enumerate() {
            let lv = pieces[g.piece].level;
            match segs.last_mut() { Some(s) if glyphs[a + s.0].piece == g.piece => s.1 = k + 1, _ => segs.push((k, k + 1, lv)) }
        }
        if let Some(max) = segs.iter().map(|s| s.2).max() {
            let min_odd = segs.iter().map(|s| s.2).filter(|l| l % 2 == 1).min().unwrap_or(max + 1);
            let mut lvl = max;
            while lvl >= min_odd && lvl > 0 {
                let mut i = 0;
                while i < segs.len() {
                    if segs[i].2 >= lvl { let j0 = i; while i < segs.len() && segs[i].2 >= lvl { i += 1; } segs[j0..i].reverse(); } else { i += 1; }
                }
                lvl -= 1;
            }
        }
        let order: Vec<usize> = segs.iter().flat_map(|&(s, e, lv)| { let r: Vec<usize> = (a + s..a + e).collect(); if lv % 2 == 1 { r.into_iter().rev().collect::<Vec<_>>() } else { r } }).collect();
        for g in order.iter().map(|&k| &glyphs[k]) {
            let p = &pieces[g.piece];
            let shift = if p.baseline != 0.0 { -p.baseline * p.size * scale } else { 0.0 };
            if current.as_ref().is_none_or(|r| r.face != p.key.face || r.color != p.color || r.size != p.size * scale * if p.baseline != 0.0 { 0.65 } else { 1.0 } || r.underline != p.underline || r.strike != p.strike || r.baseline != baseline + shift || r.highlight != p.highlight || r.fx != p.fx) {
                if let Some(r) = current.take() { out.runs.push(r); }
                current = Some(GlyphRun { face: p.key.face, synthetic_bold: p.key.synthetic_bold || (p.bold && !fonts_bold(p.key)), italic_skew: p.italic && !fonts_italic(p.key), size: p.size * scale * if p.baseline != 0.0 { 0.65 } else { 1.0 }, color: p.color, glyphs: vec![], underline: p.underline, strike: p.strike, span: (x, x), baseline: baseline + shift, highlight: p.highlight, fx: p.fx.clone() });
            }
            let r = current.as_mut().unwrap();
            r.glyphs.push(Glyph { gid: g.gid, x: x + g.dx, y: baseline + shift - g.dy });
            let adv = g.adv + if char_of(g.ch) == ' ' { justify_gap } else { 0.0 };
            char_boxes.push((g.ch, x, x + adv));
            x += adv;
            r.span.1 = x;
        }
        if let Some(r) = current.take() { out.runs.push(r); }
        let end_char = if li + 1 < lines.len() { glyphs[lines[li + 1].0].ch } else { para_end };
        // Character boxes in order (glyph clusters may cover several characters).
        char_boxes.sort_by_key(|c| c.0);
        for c in first_char..end_char {
            let bx = char_boxes.iter().rev().find(|b| b.0 <= c).map(|b| (b.1, b.2)).unwrap_or((x, x));
            let is_break = breaks.contains(&c);
            let entry = if is_break { CharBox { line: line_index, x0: x, x1: x } } else { CharBox { line: line_index, x0: bx.0, x1: bx.1 } };
            if c - char_base + (char_base) >= out.chars.len() { out.chars.push(entry); } else { out.chars[c] = entry; }
        }
        out.lines.push(Line { top: *y, height, baseline, x0: start_x + if para.align == Align::Center { (avail - width) / 2.0 } else if para.align == Align::Right { avail - width } else { 0.0 }, x1: x, para: pi, first_char, end_char });
        *y += height;
    }
}

/// Flow laid-out lines into `n` columns of `col_w` points with `gap` between them, filling
/// each to `box_h` before moving on (bodyPr numCol / spcCol).
pub fn flow_columns(laid: &mut Layout, n: u32, col_w: f64, gap: f64, box_h: f64) {
    if n <= 1 || laid.lines.is_empty() { return; }
    let mut col = 0u32;
    let mut y0 = 0.0; // top of the current column's first line in the single-column layout
    let mut shifts: Vec<(f64, f64)> = vec![]; // per line (dx, dy)
    for l in &laid.lines {
        if l.top + l.height - y0 > box_h + 0.01 && l.top > y0 && col + 1 < n { col += 1; y0 = l.top; }
        shifts.push((col as f64 * (col_w + gap), -y0));
    }
    for r in laid.runs.iter_mut() {
        let Some(li) = laid.lines.iter().position(|l| r.baseline >= l.top && r.baseline <= l.top + l.height + 0.01) else { continue };
        let (dx, dy) = shifts[li];
        r.baseline += dy; r.span.0 += dx; r.span.1 += dx;
        for g in r.glyphs.iter_mut() { g.x += dx; g.y += dy; }
    }
    for c in laid.chars.iter_mut() { if let Some(&(dx, _)) = shifts.get(c.line) { c.x0 += dx; c.x1 += dx; } }
    for (l, (dx, dy)) in laid.lines.iter_mut().zip(&shifts) { l.top += dy; l.baseline += dy; l.x0 += dx; l.x1 += dx; }
    laid.height = laid.lines.iter().map(|l| l.top + l.height).fold(0.0, f64::max);
}

fn fonts_bold(k: FontKey) -> bool { word_engine::fonts::face(k.face).bold }
fn fonts_italic(k: FontKey) -> bool { word_engine::fonts::face(k.face).italic }

/// The box (left, top, width, height in points) text is placed in, from the shape size, the
/// geometry's text rectangle and the insets; and the vertical offset its anchor gives.
pub fn place(body: &TextBody, shape_w: f64, shape_h: f64, text_rect: Option<[f64; 4]>) -> (f64, f64, f64, f64) {
    let [l, t, r, b] = text_rect.unwrap_or([0.0, 0.0, shape_w, shape_h]);
    let [il, it, ir, ib] = body.insets;
    let e = 12_700.0;
    ((l + il) / e, (t + it) / e, ((r - l) - il - ir).max(0.0) / e, ((b - t) - it - ib).max(0.0) / e)
}

/// The vertical offset for the anchor.
pub fn anchor_offset(body: &TextBody, laid: &Layout, box_h: f64) -> f64 {
    match body.anchor { Anchor::Top => 0.0, Anchor::Middle => (box_h - laid.height) / 2.0, Anchor::Bottom => box_h - laid.height }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Run;

    fn body(text: &str, size: f64) -> TextBody {
        TextBody { paras: text.split('\n').map(|l| Para { runs: vec![Run { text: l.into(), font: "Calibri".into(), size, color: Rgba::BLACK, ..Default::default() }], end_size: size, ..Default::default() }).collect(), wrap: true, ..Default::default() }
    }

    #[test]
    fn wraps_and_maps_characters() {
        let b = body("The quick brown fox jumps over the lazy dog\nSecond", 18.0);
        let l = layout(&b, 120.0, 500.0);
        assert!(l.lines.len() >= 4, "lines {}", l.lines.len());
        let n = "The quick brown fox jumps over the lazy dog\nSecond".chars().count();
        assert_eq!(l.chars.len(), n);
        // Characters advance left to right on the first line.
        assert!(l.chars[1].x0 > l.chars[0].x0);
        // Hit testing finds the start and the last paragraph.
        assert_eq!(l.hit(-5.0, 0.0), 0);
        assert!(l.hit(500.0, l.height - 1.0) >= n - 1);
        let (x, _, h) = l.caret(n);
        assert!(x > 0.0 && h > 0.0);
    }

    #[test]
    fn shrinks_to_fit() {
        let mut b = body("one two three four five six seven eight nine ten eleven twelve", 40.0);
        b.autofit = Autofit::Shrink { font_scale: 1.0, line_reduction: 0.0 };
        let l = layout(&b, 200.0, 100.0);
        assert!(l.scale < 1.0 && l.height <= 100.0 + 1e-6, "scale {} height {}", l.scale, l.height);
    }
}

#[cfg(test)]
mod bidi_tests {
    use super::*;
    use crate::scene::{Para, Run};

    #[test]
    fn arabic_phrase_reads_right_to_left() {
        // Three Arabic words: the first word in reading order is drawn rightmost.
        let text = "انقر لإضافة عنوان";
        let body = TextBody { paras: vec![Para { runs: vec![Run { text: text.into(), font: "Calibri".into(), size: 24.0, color: Rgba::BLACK, ..Default::default() }], end_size: 24.0, ..Default::default() }], wrap: true, ..Default::default() };
        let l = layout(&body, 600.0, 200.0);
        // Character 0 (first letter of the first word) is right of the last word's letters.
        let first = l.chars[0].x0;
        let last = l.chars[text.chars().count() - 1].x0;
        assert!(first > last, "first {first} last {last}");
    }
}
