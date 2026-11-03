//! Text positions, gathered by running the page through a recording device.

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::hayro_syntax::page::Page;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, RenderContent, SoftMask, interpret_page,
};
use hayro::kurbo::{Point as KPoint, Rect as KRect};

use crate::geometry::{Point, Rect};

/// One character on the page, in reading (content-stream) order.
#[derive(Clone, Debug, PartialEq)]
pub struct TextGlyph {
    pub ch: char,
    /// Bounds in view points.
    pub rect: Rect,
    /// True for spaces and line breaks the engine inferred from gaps.
    pub synthetic: bool,
}

/// All text of one page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageText {
    pub glyphs: Vec<TextGlyph>,
}

/// One search match: the page and one rectangle per line it spans.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub page: usize,
    pub rects: Vec<Rect>,
}

impl PageText {
    pub fn text(&self) -> String {
        self.glyphs.iter().map(|g| g.ch).collect()
    }

    /// Index of the glyph closest to `p` (within `slack` points), for text selection.
    pub fn glyph_at(&self, p: Point, slack: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, g) in self.glyphs.iter().enumerate() {
            if g.synthetic && g.ch == '\n' {
                continue;
            }
            let r = g.rect;
            let dx = (r.x0 - p.x).max(p.x - r.x1).max(0.0);
            let dy = (r.y0 - p.y).max(p.y - r.y1).max(0.0);
            // Prefer glyphs on the same line: vertical distance weighs more.
            let d = dx + dy * 3.0;
            if d <= slack && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// One rectangle per line for glyphs `a..=b` (in either order).
    pub fn line_rects(&self, a: usize, b: usize) -> Vec<Rect> {
        let (a, b) = (a.min(b), a.max(b).min(self.glyphs.len().saturating_sub(1)));
        let mut out: Vec<Rect> = Vec::new();
        for g in self.glyphs.get(a..=b).unwrap_or(&[]) {
            if g.ch == '\n' || (g.synthetic && g.rect.width() <= 0.0) {
                continue;
            }
            match out.last_mut() {
                Some(last) if same_line(last, &g.rect) => *last = last.union(g.rect),
                _ => out.push(g.rect),
            }
        }
        out
    }

    /// The text of glyphs `a..=b`.
    pub fn slice(&self, a: usize, b: usize) -> String {
        let (a, b) = (a.min(b), a.max(b));
        self.glyphs
            .get(a..=b.min(self.glyphs.len().saturating_sub(1)))
            .unwrap_or(&[])
            .iter()
            .map(|g| g.ch)
            .collect()
    }
}

fn same_line(a: &Rect, b: &Rect) -> bool {
    let overlap = a.y1.min(b.y1) - a.y0.max(b.y0);
    overlap > 0.5 * a.height().min(b.height())
}

/// Glyph outlines are in a 1000-unit em square.
const EM: f64 = 1000.0;

struct Collector {
    raw: Vec<(String, Rect, f32)>,
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, _: &hayro::kurbo::BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for g in run.glyphs() {
            let text = match g.as_unicode() {
                Some(BfString::Char(c)) => c.to_string(),
                Some(BfString::String(s)) => s,
                None => continue,
            };
            let advance = match &**g {
                Glyph::Outline(o) => o.advance_width(),
                Glyph::Type3(t) => t.advance_width(),
            }
            .filter(|w| *w > 0.0)
            .unwrap_or(500.0) as f64;
            let m = props.transform * g.transform();
            let r = m.transform_rect_bbox(KRect::new(0.0, -0.22 * EM, advance, 0.9 * EM));
            // Size of one em on screen, to judge gaps between glyphs.
            let em = (m * KPoint::new(0.0, EM)).distance(m * KPoint::ORIGIN) as f32;
            self.raw.push((
                text,
                Rect::new(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32),
                em,
            ));
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

pub(crate) fn extract(page: &Page<'_>) -> PageText {
    let settings = InterpreterSettings {
        render_content: RenderContent::PAGE,
        ..Default::default()
    };
    let (w, h) = page.render_dimensions();
    let cache = InterpreterCache::new();
    let mut ctx = Context::new(
        page.initial_transform(true).to_kurbo(),
        KRect::new(0.0, 0.0, w as f64, h as f64),
        &cache,
        page.xref(),
        settings,
    );
    let mut dev = Collector { raw: Vec::new() };
    interpret_page(page, &mut ctx, &mut dev);
    assemble(dev.raw)
}

fn is_rtl_char(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

fn presentation_form(c: char) -> bool {
    matches!(c as u32, 0xFB50..=0xFDFF | 0xFE70..=0xFEFF)
}

#[derive(Clone, Copy, PartialEq)]
enum Dir {
    Rtl,
    Ltr,
    Neutral,
}

fn dir_of(text: &str) -> Dir {
    match text.chars().next() {
        Some(c) if is_rtl_char(c) => Dir::Rtl,
        Some(c) if c.is_alphanumeric() => Dir::Ltr,
        _ => Dir::Neutral,
    }
}

/// A piece of text with its box: one glyph's Unicode text, or an inferred space.
struct Unit {
    text: String,
    rect: Rect,
    synthetic: bool,
}

/// Reverses each maximal run of units of direction `d` (neutrals inside a run move with it).
fn reverse_runs(units: &mut [Unit], d: Dir) {
    let idx: Vec<usize> = (0..units.len())
        .filter(|&i| dir_of(&units[i].text) == d)
        .collect();
    let mut k = 0;
    while k < idx.len() {
        let start = idx[k];
        let mut end = start;
        while k + 1 < idx.len()
            && (end + 1..idx[k + 1]).all(|j| dir_of(&units[j].text) == Dir::Neutral)
        {
            k += 1;
            end = idx[k];
        }
        units[start..=end].reverse();
        k += 1;
    }
}

/// Puts a line given in visual order (left to right) into reading order.
fn to_logical(mut units: Vec<Unit>) -> Vec<Unit> {
    let rtl = units.iter().filter(|u| dir_of(&u.text) == Dir::Rtl).count();
    let ltr = units.iter().filter(|u| dir_of(&u.text) == Dir::Ltr).count();
    if rtl >= ltr {
        // Right-to-left paragraph: read from the right; numbers and Latin words stay LTR.
        units.reverse();
        reverse_runs(&mut units, Dir::Ltr);
    } else {
        reverse_runs(&mut units, Dir::Rtl);
    }
    units
}

/// Turns raw glyph boxes into text, inferring spaces and line breaks. Lines with
/// right-to-left script are put into reading order whatever order the PDF drew them in,
/// and Arabic presentation forms are normalized to the letters people type.
fn assemble(raw: Vec<(String, Rect, f32)>) -> PageText {
    // Consecutive glyphs on the same row form a line.
    let mut lines: Vec<Vec<(String, Rect, f32)>> = Vec::new();
    for item in raw {
        let joins = lines
            .last()
            .and_then(|l| l.last())
            .is_some_and(|(_, p, pem)| {
                same_line(p, &item.1)
                    || (item.1.center_y() - p.center_y()).abs() < 0.3 * pem.max(item.2)
            });
        if joins {
            lines.last_mut().expect("line").push(item);
        } else {
            lines.push(vec![item]);
        }
    }
    let mut glyphs: Vec<TextGlyph> = Vec::new();
    for line in lines {
        if let Some(last) = glyphs.last()
            && !last.ch.is_whitespace()
        {
            glyphs.push(TextGlyph {
                ch: '\n',
                rect: last.rect,
                synthetic: true,
            });
        }
        let rtl = line.iter().any(|(t, _, _)| t.chars().any(is_rtl_char));
        let mut line = line;
        if rtl {
            line.sort_by(|a, b| a.1.x0.total_cmp(&b.1.x0));
        }
        let mut units: Vec<Unit> = Vec::new();
        let mut prev: Option<(Rect, f32)> = None;
        for (text, rect, em) in line {
            if let Some((p, pem)) = prev {
                let gap = (rect.x0 - p.x1).max(p.x0 - rect.x1);
                let last_space = units
                    .last()
                    .is_some_and(|u| u.text.ends_with(char::is_whitespace));
                if gap > 0.15 * em.max(pem) && !last_space && !text.starts_with(' ') {
                    let (x0, x1) = if rect.x0 >= p.x1 {
                        (p.x1, rect.x0)
                    } else {
                        (rect.x1, p.x0)
                    };
                    units.push(Unit {
                        text: " ".into(),
                        rect: Rect::new(x0, p.y0, x1, p.y1),
                        synthetic: true,
                    });
                }
            }
            prev = Some((rect, em));
            units.push(Unit {
                text,
                rect,
                synthetic: false,
            });
        }
        if rtl {
            units = to_logical(units);
        }
        for u in units {
            let text: String = if u.text.chars().any(presentation_form) {
                use unicode_normalization::UnicodeNormalization;
                u.text
                    .chars()
                    .flat_map(|c| {
                        if presentation_form(c) {
                            c.nfkc().collect::<Vec<_>>()
                        } else {
                            vec![c]
                        }
                    })
                    .collect()
            } else {
                u.text
            };
            let chars: Vec<char> = text.chars().collect();
            let n = chars.len().max(1) as f32;
            let w = u.rect.width() / n;
            let rtl_unit = chars.first().is_some_and(|c| is_rtl_char(*c));
            for (i, ch) in chars.into_iter().enumerate() {
                // Arabic ligatures read from the right.
                let k = if rtl_unit { n as usize - 1 - i } else { i };
                let x0 = u.rect.x0 + w * k as f32;
                glyphs.push(TextGlyph {
                    ch,
                    rect: Rect::new(x0, u.rect.y0, x0 + w, u.rect.y1),
                    synthetic: u.synthetic,
                });
            }
        }
    }
    PageText { glyphs }
}

fn fold(c: char) -> char {
    if c.is_whitespace() {
        ' '
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

pub(crate) fn search(text: &PageText, page: usize, query: &str) -> Vec<SearchHit> {
    let needle: Vec<char> = query.trim().chars().map(fold).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let hay: Vec<char> = text.glyphs.iter().map(|g| fold(g.ch)).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()] == needle[..] {
            out.push(SearchHit {
                page,
                rects: text.line_rects(i, i + needle.len() - 1),
            });
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(t: &str, x: f32) -> (String, Rect, f32) {
        (t.to_string(), Rect::new(x, 0.0, x + 8.0, 10.0), 10.0)
    }

    #[test]
    fn visual_order_arabic_reads_logically() {
        // "مرحبا 2026" drawn left to right in visual order: 2026 first, then the word reversed.
        let raw = vec![
            unit("2", 0.0),
            unit("0", 8.0),
            unit("2", 16.0),
            unit("6", 24.0),
            unit("ا", 40.0),
            unit("ب", 48.0),
            unit("ح", 56.0),
            unit("ر", 64.0),
            unit("م", 72.0),
        ];
        assert_eq!(assemble(raw).text(), "مرحبا 2026");
    }

    #[test]
    fn presentation_forms_are_normalized() {
        // Isolated/final forms as some producers map them: سلام (with lam-alef ligature).
        let raw = vec![
            unit("\u{FEE1}", 0.0),
            unit("\u{FEFC}", 8.0),
            unit("\u{FEB3}", 16.0),
        ];
        assert_eq!(assemble(raw).text(), "سلام");
    }

    #[test]
    fn mixed_latin_inside_arabic() {
        // "اسم PDF هنا" in visual order: "انه FDP مسا" left to right → reversed runs.
        let raw = vec![
            unit("ا", 0.0),
            unit("ن", 8.0),
            unit("ه", 16.0),
            unit("P", 32.0),
            unit("D", 40.0),
            unit("F", 48.0),
            unit("م", 64.0),
            unit("س", 72.0),
            unit("ا", 80.0),
        ];
        assert_eq!(assemble(raw).text(), "اسم PDF هنا");
    }
}
