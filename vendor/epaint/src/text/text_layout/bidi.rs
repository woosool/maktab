//! Right-to-left text (Arabic, Hebrew): the Unicode bidi algorithm on top of egui's layout.
//!
//! Glyphs stay in reading (logical) order, one per char, so cursor indices keep working; only
//! their x positions are moved. Shaping happens per directional run, then each laid-out row is
//! put in visual order (UAX #9, rules L1–L2).

use alloc::{string::String, vec::Vec};

use unicode_bidi::{bidi_class, BidiClass, BidiInfo, Level};

use super::{Paragraph, ShapingContext, TextRun};
use crate::text::{index::{ByteIndex, ByteRangeExt as _}, Glyph};
use crate::text::{glyph_atlas::GlyphAllocation, styled_metrics::StyledMetrics};

fn is_rtl_char(c: char) -> bool {
    matches!(bidi_class(c), BidiClass::R | BidiClass::AL)
}

/// Bidi level of every byte of `text`, or `None` if it has no right-to-left letters.
pub(crate) fn segment_levels(text: &str) -> Option<Vec<Level>> {
    if !text.chars().any(is_rtl_char) {
        return None;
    }
    Some(BidiInfo::new(text, None).levels)
}

/// Split shaping runs where the bidi level changes, so each is shaped in one direction.
pub(crate) fn split_runs_at_levels(runs: &mut Vec<TextRun>, levels: &[Level]) {
    let mut out = Vec::with_capacity(runs.len());
    for run in runs.drain(..) {
        if run.raster.is_some() {
            out.push(run);
            continue;
        }
        let range = run.byte_range.as_usize();
        let mut start = range.start;
        for i in range.start + 1..range.end {
            if levels[i] != levels[i - 1] {
                out.push(TextRun { font_key: run.font_key, byte_range: ByteIndex(start)..ByteIndex(i), raster: None });
                start = i;
            }
        }
        out.push(TextRun { font_key: run.font_key, byte_range: ByteIndex(start)..ByteIndex(range.end), raster: None });
    }
    *runs = out;
}

/// The glyphs of a right-to-left run (from `first` on) arrive in visual order. Put them in
/// reading order, each cluster keeping its shape, laid end to end from `start_px`.
pub(crate) fn rtl_run_to_logical(
    ctx: &ShapingContext,
    paragraph: &mut Paragraph,
    first: usize,
    clusters: &[u32],
    run_text: &str,
    face_metrics: &StyledMetrics,
    start_px: f32,
) {
    let visual: Vec<Glyph> = paragraph.glyphs.drain(first..).collect();
    let mut groups: Vec<(u32, Vec<Glyph>)> = Vec::new();
    for (glyph, &cluster) in visual.into_iter().zip(clusters) {
        match groups.iter_mut().find(|(c, _)| *c == cluster) {
            Some((_, v)) => v.push(glyph),
            None => groups.push((cluster, alloc::vec![glyph])),
        }
    }
    groups.sort_by_key(|(c, _)| *c);
    let starts: Vec<u32> = groups.iter().map(|(c, _)| *c).collect();
    let ppp = ctx.pixels_per_point;
    let mut pen = start_px;
    for (k, (cluster, mut glyphs)) in groups.into_iter().enumerate() {
        // The base glyph first; marks keep their offset from it.
        glyphs.sort_by_key(|g| g.advance_width == 0.0);
        let origin = glyphs[0].pos.x;
        let width_px: f32 = glyphs.iter().map(|g| g.advance_width).sum::<f32>() * ppp;
        let end = starts.get(k + 1).map_or(run_text.len(), |&s| s as usize);
        let text = run_text.get(cluster as usize..end).unwrap_or_default();
        let chars = text.chars().count().max(1);
        let owner = paragraph.glyphs.len();
        let n = glyphs.len().min(chars);
        let mut chars_iter = text.chars();
        for (j, mut glyph) in glyphs.into_iter().enumerate() {
            let x = (pen + (glyph.pos.x - origin) * ppp).round() / ppp;
            if j < chars {
                glyph.pos.x = x;
                glyph.chr = chars_iter.next().unwrap_or(glyph.chr);
                paragraph.glyphs.push(glyph);
            } else {
                // More glyphs than chars (a letter drawn in parts): draw it with the first.
                glyph.pos.x = x - paragraph.glyphs[owner].pos.x;
                glyph.advance_width = 0.0;
                paragraph.attached.push((owner, glyph));
            }
        }
        for chr in chars_iter.by_ref().take(chars.saturating_sub(n)) {
            let x = (pen + width_px).round() as i32;
            paragraph.glyphs.push(ctx.glyph(chr, x, 0.0, face_metrics, GlyphAllocation::default()));
        }
        pen += width_px;
    }
}

/// `Some(base is right to left)` when the paragraph has right-to-left letters.
pub(crate) fn paragraph_base(glyphs: &[Glyph]) -> Option<bool> {
    if !glyphs.iter().any(|g| is_rtl_char(g.chr)) {
        return None;
    }
    let first_strong = glyphs.iter().find_map(|g| match bidi_class(g.chr) {
        BidiClass::L => Some(false),
        BidiClass::R | BidiClass::AL => Some(true),
        _ => None,
    });
    Some(first_strong.unwrap_or(false))
}

fn row_levels(glyphs: &[Glyph], base_rtl: Option<bool>) -> Vec<Level> {
    let text: String = glyphs.iter().map(|g| g.chr).collect();
    let base = base_rtl.map(|rtl| if rtl { Level::rtl() } else { Level::ltr() });
    let info = BidiInfo::new(&text, base);
    match info.paragraphs.first() {
        Some(para) => {
            let levels = info.reordered_levels_per_char(para, 0..text.len());
            if levels.len() == glyphs.len() { levels } else { alloc::vec![Level::ltr(); glyphs.len()] }
        }
        None => alloc::vec![Level::ltr(); glyphs.len()],
    }
}

/// Move a row's glyphs into visual order.
pub(crate) fn reorder_row(row: &mut crate::text::Row, base_rtl: bool, pixels_per_point: f32) {
    if row.glyphs.is_empty() {
        return;
    }
    let levels = row_levels(&row.glyphs, Some(base_rtl));
    if !levels.iter().any(|l| l.is_rtl()) {
        return;
    }
    // Units: a glyph with its zero-width followers (marks, ligature continuations).
    let mut units: Vec<(usize, usize)> = Vec::new();
    for i in 0..row.glyphs.len() {
        if i == 0 || 0.0 < row.glyphs[i].advance_width || levels[i] != levels[i - 1] {
            units.push((i, i + 1));
        } else if let Some(u) = units.last_mut() {
            u.1 = i + 1;
        }
    }
    let unit_levels: Vec<Level> = units.iter().map(|u| levels[u.0]).collect();
    let order = BidiInfo::reorder_visual(&unit_levels);
    let old: Vec<f32> = row.glyphs.iter().map(|g| g.pos.x).collect();
    let mut pen = old[0];
    for u in order {
        let (s, e) = units[u];
        let width: f32 = row.glyphs[s..e].iter().map(|g| g.advance_width).sum();
        for i in s..e {
            row.glyphs[i].pos.x = ((pen + old[i] - old[s]) * pixels_per_point).round() / pixels_per_point;
        }
        pen += width;
    }
}

/// For a row with right-to-left letters: whether each glyph reads right to left.
pub(crate) fn row_rtl_flags(glyphs: &[Glyph]) -> Option<Vec<bool>> {
    paragraph_base(glyphs)?;
    Some(row_levels(glyphs, None).iter().map(|l| l.is_rtl()).collect())
}
