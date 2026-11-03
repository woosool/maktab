//! Redaction: removing content under marks for real, then covering the marks.
//!
//! For every marked page the page's own content stream is walked (see `textedit`):
//! - text: each glyph whose centre lies under a mark is cut out of its show operator and
//!   replaced by an equal displacement, so the text around it keeps its position;
//! - images: pixels under a mark are set to black in the image data itself (JPEG and 8-bit
//!   RGB/grey images); other image formats under a mark are removed entirely;
//! - drawings: painted paths overlapping a mark are not painted any more (large drawings,
//!   such as page backgrounds, are kept and reported);
//! - annotations (comments, links, form fields) overlapping a mark are deleted;
//! - objects added by the editor (text, pictures, signatures) under a mark are deleted.
//!
//! Finally a black box is drawn over each mark, the result is re-read and any text still found
//! under a mark (for example inside a Form XObject) is reported rather than silently kept.

use std::collections::{BTreeSet, HashMap};

use hayro::kurbo::{Affine, Point as KPoint};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::{Error, Result};
use crate::geometry::{Point, Rect};
use crate::items;
use crate::text::PageText;
use crate::textedit::{self, Show};

/// What a redaction did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RedactReport {
    pub glyphs_removed: usize,
    pub images_edited: usize,
    pub images_removed: usize,
    pub paths_removed: usize,
    pub annotations_removed: usize,
    pub objects_removed: usize,
    /// Hidden information removed (metadata, scripts, attachments…).
    pub hidden_removed: Vec<String>,
    /// Things that could not be handled as usual, for the user to check.
    pub unhandled: Vec<String>,
}

/// Search patterns for Find & redact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Email,
    Phone,
    CardNumber,
    Date,
}

impl Pattern {
    pub const ALL: [Pattern; 4] = [
        Pattern::Email,
        Pattern::Phone,
        Pattern::CardNumber,
        Pattern::Date,
    ];
    fn regex(self) -> &'static regex::Regex {
        use std::sync::OnceLock;
        static RE: [OnceLock<regex::Regex>; 4] = [
            OnceLock::new(),
            OnceLock::new(),
            OnceLock::new(),
            OnceLock::new(),
        ];
        let (i, src) = match self {
            Pattern::Email => (0, r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}"),
            Pattern::Phone => (1, r"\+?\(?\d[\d\s().\-]{6,}\d"),
            Pattern::CardNumber => (2, r"\b(?:\d[ \-]?){12,18}\d\b"),
            Pattern::Date => (
                3,
                r"(?i)\b(?:\d{1,2}[/.\-]\d{1,2}[/.\-]\d{2,4}|\d{4}-\d{2}-\d{2}|\d{1,2} (?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]* \d{4}|(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]* \d{1,2},? \d{4})\b",
            ),
        };
        RE[i].get_or_init(|| regex::Regex::new(src).expect("pattern compiles"))
    }

    fn accepts(self, m: &str) -> bool {
        let digits: Vec<u32> = m.chars().filter_map(|c| c.to_digit(10)).collect();
        match self {
            Pattern::Phone => digits.len() >= 7 && digits.len() <= 15,
            Pattern::CardNumber => (13..=19).contains(&digits.len()) && luhn(&digits),
            _ => true,
        }
    }
}

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// A match found for redaction: its page, one rectangle per line, and the text.
#[derive(Clone, Debug, PartialEq)]
pub struct RedactHit {
    pub page: usize,
    pub rects: Vec<Rect>,
    pub text: String,
}

/// Finds words (case-insensitive, whole matches) and patterns on one page.
pub(crate) fn find(
    text: &PageText,
    page: usize,
    words: &[String],
    patterns: &[Pattern],
) -> Vec<RedactHit> {
    let s: String = text.glyphs.iter().map(|g| g.ch).collect();
    // Byte offset -> glyph index.
    let mut byte_to_glyph = vec![0usize; s.len() + 1];
    for (gi, (bi, c)) in s.char_indices().enumerate() {
        byte_to_glyph[bi..bi + c.len_utf8()].fill(gi);
    }
    byte_to_glyph[s.len()] = text.glyphs.len();
    let mut out = Vec::new();
    let mut push = |a: usize, b: usize| {
        if b <= a {
            return;
        }
        let (ga, gb) = (byte_to_glyph[a], byte_to_glyph[b].saturating_sub(1));
        let rects = text.line_rects(ga, gb);
        if !rects.is_empty() {
            out.push(RedactHit {
                page,
                rects,
                text: s[a..b].to_string(),
            });
        }
    };
    for w in words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
        let re = regex::RegexBuilder::new(&regex::escape(w))
            .case_insensitive(true)
            .build();
        if let Ok(re) = re {
            for m in re.find_iter(&s) {
                push(m.start(), m.end());
            }
        }
    }
    for p in patterns {
        for m in p.regex().find_iter(&s) {
            if p.accepts(m.as_str()) {
                push(m.start(), m.end());
            }
        }
    }
    out
}

fn center(r: &Rect) -> Point {
    Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

fn under(marks: &[Rect], r: &Rect) -> bool {
    let c = center(r);
    marks.iter().any(|m| m.contains(c))
}

fn overlaps(marks: &[Rect], r: &Rect) -> bool {
    marks.iter().any(|m| m.intersects(r))
}

/// Rebuilds a show operator without the glyphs under the marks. Returns the replacement
/// operators and the number of glyphs removed, or `None` when a removed glyph's width is
/// unknown (the caller then drops the whole operator).
fn cut_show(op: &Operation, show: &Show, marks: &[Rect]) -> Option<(Vec<Operation>, usize)> {
    let doomed: BTreeSet<(usize, usize)> = show
        .glyphs
        .iter()
        .filter(|g| g.rect.is_some_and(|r| under(marks, &r)))
        .map(|g| (g.part, g.byte))
        .collect();
    if doomed.is_empty() {
        return Some((vec![op.clone()], 0));
    }
    let parts: Vec<Object> = match op.operator.as_str() {
        "TJ" => op.operands.first()?.as_array().ok()?.clone(),
        _ => vec![op.operands.last()?.clone()],
    };
    let mut array: Vec<Object> = Vec::new();
    let mut removed = 0;
    for (pi, part) in parts.iter().enumerate() {
        let Object::String(bytes, format) = part else {
            array.push(part.clone());
            continue;
        };
        let mut current: Vec<u8> = Vec::new();
        for g in show.glyphs.iter().filter(|g| g.part == pi) {
            let code = &bytes[g.byte..(g.byte + g.len).min(bytes.len())];
            if doomed.contains(&(pi, g.byte)) {
                let adv = g.advance?;
                if !current.is_empty() {
                    array.push(Object::String(std::mem::take(&mut current), *format));
                }
                if show.size_th.abs() < 1e-6 {
                    return None;
                }
                array.push(Object::Real(-adv / show.size_th * 1000.0));
                removed += 1;
            } else {
                current.extend_from_slice(code);
            }
        }
        if !current.is_empty() {
            array.push(Object::String(current, *format));
        }
    }
    let mut ops = Vec::new();
    if op.operator == "\"" {
        ops.push(Operation::new("Tw", vec![op.operands[0].clone()]));
        ops.push(Operation::new("Tc", vec![op.operands[1].clone()]));
    }
    if op.operator == "'" || op.operator == "\"" {
        ops.push(Operation::new("T*", vec![]));
    }
    ops.push(Operation::new("TJ", vec![Object::Array(array)]));
    Some((ops, removed))
}

/// Blackens the pixels of an image under the marks; returns the new image object.
fn black_out(
    doc: &mut lopdf::Document,
    image: ObjectId,
    unit_to_view: Affine,
    marks: &[Rect],
) -> Option<ObjectId> {
    let Ok(Object::Stream(s)) = doc.get_object(image) else {
        return None;
    };
    let dict = s.dict.clone();
    let w = dict.get(b"Width").ok()?.as_i64().ok()? as u32;
    let h = dict.get(b"Height").ok()?.as_i64().ok()? as u32;
    let filters: Vec<Vec<u8>> = s
        .filters()
        .map(|f| f.into_iter().map(<[u8]>::to_vec).collect())
        .unwrap_or_default();
    let (mut pixels, channels, cs): (Vec<u8>, usize, Object) = if filters
        .iter()
        .any(|f| f == b"DCTDecode")
    {
        let img = image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg).ok()?;
        if img.color().channel_count() == 1 {
            (
                img.to_luma8().into_raw(),
                1,
                Object::Name(b"DeviceGray".to_vec()),
            )
        } else if img.color().channel_count() == 3 {
            (
                img.to_rgb8().into_raw(),
                3,
                Object::Name(b"DeviceRGB".to_vec()),
            )
        } else {
            return None;
        }
    } else {
        if dict.get(b"BitsPerComponent").ok()?.as_i64().ok()? != 8
            || filters.iter().any(|f| f != b"FlateDecode")
        {
            return None;
        }
        let cs = dict.get(b"ColorSpace").ok()?.clone();
        let channels = match textedit::deref(doc, &cs) {
            Object::Name(n) if n == b"DeviceRGB" => 3,
            Object::Name(n) if n == b"DeviceGray" => 1,
            Object::Array(a) if a.first().and_then(|n| n.as_name().ok()) == Some(b"ICCBased") => {
                let n = a
                    .get(1)
                    .map(|o| textedit::deref(doc, o))
                    .and_then(|o| o.as_stream().ok())
                    .and_then(|s| s.dict.get(b"N").ok()?.as_i64().ok())?;
                if n == 3 || n == 1 {
                    n as usize
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        let data = if filters.is_empty() {
            s.content.clone()
        } else {
            s.decompressed_content().ok()?
        };
        if data.len() < (w * h) as usize * channels {
            return None;
        }
        (data, channels, cs)
    };
    let to_unit = unit_to_view.inverse();
    for m in marks {
        let pts = [(m.x0, m.y0), (m.x1, m.y0), (m.x0, m.y1), (m.x1, m.y1)]
            .map(|(x, y)| to_unit * KPoint::new(f64::from(x), f64::from(y)));
        let (u0, u1) = (
            pts.iter().map(|p| p.x).fold(f64::MAX, f64::min),
            pts.iter().map(|p| p.x).fold(f64::MIN, f64::max),
        );
        let (v0, v1) = (
            pts.iter().map(|p| p.y).fold(f64::MAX, f64::min),
            pts.iter().map(|p| p.y).fold(f64::MIN, f64::max),
        );
        // Image space: u to the right, v up; row 0 is the top (v = 1).
        let x0 = ((u0 * f64::from(w)).floor().max(0.0) as u32).min(w);
        let x1 = ((u1 * f64::from(w)).ceil().max(0.0) as u32).min(w);
        let y0 = (((1.0 - v1) * f64::from(h)).floor().max(0.0) as u32).min(h);
        let y1 = (((1.0 - v0) * f64::from(h)).ceil().max(0.0) as u32).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) as usize) * channels;
                for c in 0..channels {
                    pixels[i + c] = 0;
                }
            }
        }
    }
    let mut new = dict;
    new.remove(b"Filter");
    new.remove(b"DecodeParms");
    new.remove(b"Decode");
    new.set("ColorSpace", cs);
    new.set("BitsPerComponent", 8);
    let mut stream = Stream::new(new, pixels);
    let _ = stream.compress();
    Some(doc.add_object(stream))
}

fn annots_list(doc: &lopdf::Document, page: ObjectId) -> Vec<Object> {
    match doc.get_dictionary(page).and_then(|p| p.get(b"Annots")) {
        Ok(Object::Reference(id)) => doc
            .get_object(*id)
            .and_then(Object::as_array)
            .cloned()
            .unwrap_or_default(),
        Ok(Object::Array(a)) => a.clone(),
        _ => Vec::new(),
    }
}

/// Shared state of one page's redaction.
struct Ctx<'a> {
    marks: &'a [Rect],
    page_no: usize,
    page_area: f32,
}

/// The rewritten operators and new XObject resources, or `None` when nothing changed.
type Rewritten = Option<(Vec<Operation>, Vec<(Vec<u8>, ObjectId)>)>;

/// Rewrites one content stream (page or form) without what lies under the marks.
fn rewrite(
    doc: &mut lopdf::Document,
    ctx: &Ctx<'_>,
    ops: Vec<Operation>,
    res: &Dictionary,
    to_view: Affine,
    report: &mut RedactReport,
    depth: usize,
) -> Result<Rewritten> {
    let marks = ctx.marks;
    let page_no = ctx.page_no;
    let page_area = ctx.page_area;
    let fonts = textedit::resource_fonts(doc, res);
    let kinds = textedit::xobject_kinds(doc, res);
    let walk = textedit::walk(&ops, &fonts, &kinds, to_view);
    // Replacements by operator index.
    let mut replace: HashMap<usize, Vec<Operation>> = HashMap::new();
    let mut unknown_fonts = BTreeSet::new();
    for show in &walk.shows {
        let any = show
            .glyphs
            .iter()
            .any(|g| g.rect.is_some_and(|r| under(marks, &r)));
        if !any {
            continue;
        }
        let op = &ops[show.index];
        match cut_show(op, show, marks) {
            Some((new, n)) => {
                report.glyphs_removed += n;
                replace.insert(show.index, new);
            }
            None => {
                // Widths unknown: drop the whole run (it is under a mark at least partly).
                report.glyphs_removed += show.glyphs.len();
                let name = fonts
                    .get(&show.font)
                    .map(|m| m.name.clone())
                    .unwrap_or_default();
                unknown_fonts.insert(name);
                let mut new = Vec::new();
                if op.operator == "'" || op.operator == "\"" {
                    new.push(Operation::new("T*", vec![]));
                }
                replace.insert(show.index, new);
            }
        }
    }
    for name in unknown_fonts {
        report.unhandled.push(format!(
            "Page {}: text in the font “{name}” has no width information, so whole runs of it that touch a mark were removed.",
            page_no + 1
        ));
    }
    // Images.
    let mut renamed: Vec<(Vec<u8>, ObjectId)> = Vec::new();
    for (index, name, m) in &walk.images {
        let pts =
            [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].map(|(x, y)| *m * KPoint::new(x, y));
        let bounds = Rect::new(
            pts.iter().map(|p| p.x as f32).fold(f32::MAX, f32::min),
            pts.iter().map(|p| p.y as f32).fold(f32::MAX, f32::min),
            pts.iter().map(|p| p.x as f32).fold(f32::MIN, f32::max),
            pts.iter().map(|p| p.y as f32).fold(f32::MIN, f32::max),
        );
        if !overlaps(marks, &bounds) {
            continue;
        }
        let image_id = res
            .get(b"XObject")
            .map(|x| textedit::deref(doc, x))
            .and_then(Object::as_dict)
            .and_then(|x| x.get(name))
            .and_then(Object::as_reference)
            .ok();
        match image_id.and_then(|id| black_out(doc, id, *m, marks)) {
            Some(new) => {
                report.images_edited += 1;
                // A fresh resource name so other pages using the image keep the original.
                let mut fresh = name.clone();
                fresh.extend_from_slice(format!("_R{}", new.0).as_bytes());
                renamed.push((fresh.clone(), new));
                replace.insert(
                    *index,
                    vec![Operation::new("Do", vec![Object::Name(fresh)])],
                );
            }
            None => {
                report.images_removed += 1;
                report.unhandled.push(format!(
                    "Page {}: an image under a mark is in a format that can't be edited, so it was removed entirely.",
                    page_no + 1
                ));
                replace.insert(*index, Vec::new());
            }
        }
    }
    // Drawings.
    let mark_area: f32 = marks.iter().map(|m| m.width() * m.height()).sum();
    for (index, bounds) in &walk.paths {
        if !overlaps(marks, bounds) {
            continue;
        }
        let area = bounds.width().max(0.5) * bounds.height().max(0.5);
        if area <= (4.0 * mark_area).max(page_area * 0.02)
            || marks.iter().any(|m| {
                m.expand(1.0).contains(Point::new(bounds.x0, bounds.y0))
                    && m.expand(1.0).contains(Point::new(bounds.x1, bounds.y1))
            })
        {
            report.paths_removed += 1;
            replace.insert(*index, vec![Operation::new("n", vec![])]);
        } else {
            report.unhandled.push(format!("Page {}: a large drawing (such as a background) overlaps a mark and was kept; the black box covers it.", page_no + 1));
        }
    }
    if walk.inline_images {
        report.unhandled.push(format!(
            "Page {}: inline images are not checked; review the page.",
            page_no + 1
        ));
    }
    // Form XObjects under a mark: rewrite a copy of the form the same way.
    for (index, name, m) in &walk.forms {
        let Some(form_id) = res
            .get(b"XObject")
            .map(|x| textedit::deref(doc, x))
            .and_then(Object::as_dict)
            .and_then(|x| x.get(name))
            .and_then(Object::as_reference)
            .ok()
        else {
            continue;
        };
        let Ok(Object::Stream(form)) = doc.get_object(form_id) else {
            continue;
        };
        let form = form.clone();
        let matrix = form
            .dict
            .get(b"Matrix")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|a| a.to_vec())
            .and_then(|a| textedit::affine(&a))
            .unwrap_or(Affine::IDENTITY);
        let full = *m * matrix;
        let bbox = form
            .dict
            .get(b"BBox")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_float().ok())
                    .collect::<Vec<f32>>()
            });
        if let Some(b) = bbox.filter(|b| b.len() == 4) {
            let pts = [(b[0], b[1]), (b[2], b[1]), (b[0], b[3]), (b[2], b[3])]
                .map(|(x, y)| full * KPoint::new(f64::from(x), f64::from(y)));
            let bounds = Rect::new(
                pts.iter().map(|p| p.x as f32).fold(f32::MAX, f32::min),
                pts.iter().map(|p| p.y as f32).fold(f32::MAX, f32::min),
                pts.iter().map(|p| p.x as f32).fold(f32::MIN, f32::max),
                pts.iter().map(|p| p.y as f32).fold(f32::MIN, f32::max),
            );
            if !overlaps(marks, &bounds) {
                continue;
            }
        }
        if depth >= 4 {
            report.unhandled.push(format!(
                "Page {}: content nested too deeply in form objects was not changed.",
                page_no + 1
            ));
            continue;
        }
        let data = form
            .decompressed_content()
            .unwrap_or_else(|_| form.content.clone());
        let Ok(content) = Content::decode(&data) else {
            report.unhandled.push(format!(
                "Page {}: a form object under a mark could not be read and was left as is.",
                page_no + 1
            ));
            continue;
        };
        let form_res = form
            .dict
            .get(b"Resources")
            .ok()
            .map(|r| textedit::deref(doc, r))
            .and_then(|r| r.as_dict().ok())
            .cloned()
            .unwrap_or_else(|| res.clone());
        if let Some((out, inner)) = rewrite(
            doc,
            ctx,
            content.operations,
            &form_res,
            full,
            report,
            depth + 1,
        )? {
            let bytes = Content { operations: out }
                .encode()
                .map_err(|_| Error::Invalid("The redacted content could not be written."))?;
            let mut dict = form.dict.clone();
            dict.remove(b"Filter");
            dict.remove(b"DecodeParms");
            if !inner.is_empty() {
                let mut r = form_res.clone();
                let mut xo = r
                    .get(b"XObject")
                    .map(|x| textedit::deref(doc, x))
                    .and_then(Object::as_dict)
                    .cloned()
                    .unwrap_or_default();
                for (n, id) in inner {
                    xo.set(n, id);
                }
                r.set("XObject", xo);
                dict.set("Resources", r);
            } else if !dict.has(b"Resources") {
                dict.set("Resources", form_res.clone());
            }
            let mut stream = Stream::new(dict, bytes);
            let _ = stream.compress();
            let new = doc.add_object(stream);
            let mut fresh = name.clone();
            fresh.extend_from_slice(format!("_R{}", new.0).as_bytes());
            renamed.push((fresh.clone(), new));
            replace.insert(
                *index,
                vec![Operation::new("Do", vec![Object::Name(fresh)])],
            );
        }
    }
    if replace.is_empty() && renamed.is_empty() {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(ops.len());
    for (i, op) in ops.into_iter().enumerate() {
        match replace.remove(&i) {
            Some(new) => out.extend(new),
            None => out.push(op),
        }
    }
    Ok(Some((out, renamed)))
}

/// Redacts one page. `to_view` maps user space to view points.
pub(crate) fn redact_page(
    doc: &mut lopdf::Document,
    page: ObjectId,
    to_view: Affine,
    marks: &[Rect],
    page_no: usize,
    report: &mut RedactReport,
) -> Result<()> {
    let res = textedit::page_resources(doc, page);
    let (ops, keep) = textedit::page_ops(doc, page).map_err(Error::Refused)?;
    let page_area = {
        let (mut w, mut h) = (0.0f32, 0.0f32);
        if let Ok(mb) = doc
            .get_dictionary(page)
            .and_then(|p| p.get(b"MediaBox"))
            .and_then(Object::as_array)
        {
            let v: Vec<f32> = mb.iter().filter_map(|x| x.as_float().ok()).collect();
            if v.len() == 4 {
                w = (v[2] - v[0]).abs();
                h = (v[3] - v[1]).abs();
            }
        }
        (w * h).max(1.0)
    };
    let ctx = Ctx {
        marks,
        page_no,
        page_area,
    };
    if let Some((out, renamed)) = rewrite(doc, &ctx, ops, &res, to_view, report, 0)? {
        let bytes = Content { operations: out }
            .encode()
            .map_err(|_| Error::Invalid("The redacted page content could not be written."))?;
        let mut stream = Stream::new(Dictionary::new(), bytes);
        let _ = stream.compress();
        let id = doc.add_object(stream);
        let mut list = vec![id];
        list.extend(keep);
        items::set_contents(doc, page, &list)?;
        if !renamed.is_empty() {
            let xo = items::page_xobjects(doc, page)?;
            for (name, id) in renamed {
                xo.set(name, id);
            }
        }
    }
    // Objects added by the editor.
    for item in items::list(doc, page) {
        if overlaps(marks, &item.rect) {
            items::delete(doc, page, item.id)?;
            report.objects_removed += 1;
        }
    }
    // Annotations, links and form fields.
    let annots = annots_list(doc, page);
    let mut kept = Vec::new();
    let mut gone: Vec<ObjectId> = Vec::new();
    for a in annots {
        let dict = match &a {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        };
        let rect = dict
            .and_then(|d| d.get(b"Rect").ok())
            .and_then(|r| r.as_array().ok())
            .and_then(|v| {
                let v: Vec<f32> = v.iter().filter_map(|x| x.as_float().ok()).collect();
                (v.len() == 4).then(|| Rect::new(v[0], v[1], v[2], v[3]).normalized())
            });
        let hit = rect.is_some_and(|r| {
            let pts = [(r.x0, r.y0), (r.x1, r.y1)]
                .map(|(x, y)| to_view * KPoint::new(f64::from(x), f64::from(y)));
            let v = Rect::new(
                pts[0].x as f32,
                pts[0].y as f32,
                pts[1].x as f32,
                pts[1].y as f32,
            )
            .normalized();
            overlaps(marks, &v)
        });
        if hit {
            report.annotations_removed += 1;
            if let Object::Reference(id) = a {
                gone.push(id);
            }
        } else {
            kept.push(a);
        }
    }
    if !gone.is_empty() {
        doc.get_dictionary_mut(page)?.set("Annots", kept);
        crate::forms::forget_widgets(doc, &gone);
    }
    // Cover each mark.
    let to_user = to_view.inverse();
    let mut boxes = format!(
        "q {} cm 0 0 0 rg\n",
        to_user
            .as_coeffs()
            .map(|c| crate::fonts::num(c as f32))
            .join(" ")
    );
    for m in marks {
        boxes += &format!(
            "{} {} {} {} re f\n",
            crate::fonts::num(m.x0),
            crate::fonts::num(m.y0),
            crate::fonts::num(m.width()),
            crate::fonts::num(m.height())
        );
    }
    boxes += "Q\n";
    items::ensure_wrapped(doc, page)?;
    let cover = doc.add_object(Stream::new(
        dictionary! { "MkRedaction" => true },
        boxes.into_bytes(),
    ));
    let mut list = items::contents(doc, page)?;
    list.push(cover);
    items::set_contents(doc, page, &list)?;
    Ok(())
}

/// Removes metadata, scripts, attachments and similar hidden information.
pub(crate) fn remove_hidden(
    doc: &mut lopdf::Document,
    pages: &[ObjectId],
    report: &mut RedactReport,
) {
    let mut note = |s: &str| {
        if !report.hidden_removed.iter().any(|x| x == s) {
            report.hidden_removed.push(s.to_string());
        }
    };
    if doc.trailer.remove(b"Info").is_some() {
        note("Document information (title, author, dates)");
    }
    if let Ok(cat) = doc.catalog_mut() {
        if cat.remove(b"Metadata").is_some() {
            note("XMP metadata");
        }
        if cat.remove(b"OpenAction").is_some() {
            note("Open action");
        }
        if cat.remove(b"AA").is_some() {
            note("Document actions");
        }
        if cat.remove(b"PieceInfo").is_some() {
            note("Private application data");
        }
    }
    let names = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Names").ok().cloned());
    if let Some(names) = names {
        let id = names.as_reference().ok();
        let mut dict = match id {
            Some(id) => doc.get_dictionary(id).ok().cloned(),
            None => names.as_dict().ok().cloned(),
        }
        .unwrap_or_default();
        if dict.remove(b"JavaScript").is_some() {
            note("JavaScript");
        }
        if dict.remove(b"EmbeddedFiles").is_some() {
            note("Attached files");
        }
        match id {
            Some(id) => {
                doc.objects.insert(id, Object::Dictionary(dict));
            }
            None => {
                if let Ok(cat) = doc.catalog_mut() {
                    cat.set("Names", dict);
                }
            }
        }
    }
    for &page in pages {
        if let Ok(p) = doc.get_dictionary_mut(page) {
            if p.remove(b"AA").is_some() {
                note("Page actions");
            }
            p.remove(b"PieceInfo");
            p.remove(b"Metadata");
        }
        let annots = annots_list(doc, page);
        let before = annots.len();
        let kept: Vec<Object> = annots
            .into_iter()
            .filter(|a| {
                let d = match a {
                    Object::Reference(id) => doc.get_dictionary(*id).ok(),
                    Object::Dictionary(d) => Some(d),
                    _ => None,
                };
                !d.is_some_and(|d| {
                    d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"FileAttachment")
                })
            })
            .collect();
        if kept.len() != before {
            note("Attached files");
            if let Ok(p) = doc.get_dictionary_mut(page) {
                p.set("Annots", kept);
            }
        }
    }
    doc.prune_objects();
}
