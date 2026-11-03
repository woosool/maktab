//! Editing existing page text: find the text-showing operators that draw a
//! line, remove them from the content stream, and write the new text as a
//! movable text item at the same place.
//!
//! The walker tracks the graphics and text state of the page's own content
//! stream (not Form XObjects) closely enough to know where every show operator
//! starts and how far it advances. Anything it cannot account for exactly is
//! refused with an explanation rather than risking a damaged page.

use std::collections::HashMap;

use hayro::kurbo::{Affine, Point as KPoint, Vec2};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::annot::Color;
use crate::fonts::FontFace;
use crate::geometry::{Point, Rect};
use crate::items;
use crate::text::PageText;

/// A line of existing text the user can edit.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    /// Bounds in view points.
    pub rect: Rect,
    pub text: String,
    /// Font size as displayed (view points).
    pub size: f32,
    pub color: Color,
    /// The PDF font name of the original text (e.g. "ABCDEF+Calibri").
    pub font_name: String,
    /// The bundled face the edited text will use.
    pub face: FontFace,
    /// Start of the baseline (view points).
    pub baseline: Point,
}

#[derive(Clone)]
pub(crate) struct Metrics {
    pub(crate) two_byte: bool,
    widths: Option<WidthTable>,
    pub(crate) type3: bool,
    pub(crate) name: String,
}

#[derive(Clone)]
enum WidthTable {
    Simple {
        first: u32,
        widths: Vec<f32>,
        missing: f32,
    },
    Cid {
        dw: f32,
        w: HashMap<u32, f32>,
    },
    /// A standard 14 font without /Widths: metrics from the metric-compatible bundled font.
    Standard(crate::fonts::FontFace),
    Fixed(f32),
}

impl Metrics {
    fn width(&self, code: u32) -> Option<f32> {
        Some(match self.widths.as_ref()? {
            WidthTable::Simple {
                first,
                widths,
                missing,
            } => code
                .checked_sub(*first)
                .and_then(|i| widths.get(i as usize))
                .copied()
                .unwrap_or(*missing),
            WidthTable::Cid { dw, w } => w.get(&code).copied().unwrap_or(*dw),
            WidthTable::Standard(face) => crate::fonts::win_ansi_width(*face, code)?,
            WidthTable::Fixed(w) => *w,
        })
    }
}

pub(crate) fn deref<'a>(doc: &'a lopdf::Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        _ => o,
    }
}

fn floats(doc: &lopdf::Document, o: &Object) -> Vec<f32> {
    match deref(doc, o) {
        Object::Array(a) => a
            .iter()
            .map(|x| deref(doc, x).as_float().unwrap_or(0.0))
            .collect(),
        _ => Vec::new(),
    }
}

fn metrics(doc: &lopdf::Document, font: &Dictionary) -> Metrics {
    let name = font
        .get(b"BaseFont")
        .and_then(Object::as_name)
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_else(|_| "unknown".into());
    let subtype = font
        .get(b"Subtype")
        .and_then(Object::as_name)
        .unwrap_or(b"");
    if subtype == b"Type0" {
        let identity = matches!(
            font.get(b"Encoding").and_then(Object::as_name),
            Ok(b"Identity-H")
        );
        let desc = font
            .get(b"DescendantFonts")
            .ok()
            .map(|d| deref(doc, d))
            .and_then(|d| d.as_array().ok())
            .and_then(|a| a.first())
            .map(|d| deref(doc, d))
            .and_then(|d| d.as_dict().ok());
        let widths = desc.map(|d| {
            let dw = d
                .get(b"DW")
                .map(|v| deref(doc, v).as_float().unwrap_or(1000.0))
                .unwrap_or(1000.0);
            let mut w = HashMap::new();
            if let Ok(arr) = d
                .get(b"W")
                .map(|v| deref(doc, v))
                .and_then(Object::as_array)
            {
                let mut i = 0;
                while i < arr.len() {
                    let first = deref(doc, &arr[i]).as_i64().unwrap_or(0) as u32;
                    match arr.get(i + 1).map(|o| deref(doc, o)) {
                        Some(Object::Array(ws)) => {
                            for (k, x) in ws.iter().enumerate() {
                                w.insert(first + k as u32, deref(doc, x).as_float().unwrap_or(dw));
                            }
                            i += 2;
                        }
                        Some(last) => {
                            let last = last.as_i64().unwrap_or(0) as u32;
                            let v = arr
                                .get(i + 2)
                                .map(|o| deref(doc, o).as_float().unwrap_or(dw))
                                .unwrap_or(dw);
                            for c in first..=last.min(first + 65_535) {
                                w.insert(c, v);
                            }
                            i += 3;
                        }
                        None => break,
                    }
                }
            }
            WidthTable::Cid { dw, w }
        });
        return Metrics {
            two_byte: identity,
            widths: if identity { widths } else { None },
            type3: false,
            name,
        };
    }
    let widths = font
        .get(b"Widths")
        .ok()
        .map(|w| floats(doc, w))
        .filter(|w| !w.is_empty())
        .map(|widths| {
            let missing = font
                .get(b"FontDescriptor")
                .ok()
                .map(|d| deref(doc, d))
                .and_then(|d| d.as_dict().ok())
                .and_then(|d| d.get(b"MissingWidth").ok())
                .and_then(|m| m.as_float().ok())
                .unwrap_or(0.0);
            WidthTable::Simple {
                first: font
                    .get(b"FirstChar")
                    .map(|v| deref(doc, v).as_i64().unwrap_or(0))
                    .unwrap_or(0) as u32,
                widths,
                missing,
            }
        });
    // Standard 14 fonts may omit /Widths; Liberation fonts share their metrics.
    let widths = widths.or_else(|| {
        let n = name.to_ascii_lowercase();
        let n = n.split_once('+').map_or(n.as_str(), |(_, b)| b).to_string();
        if n.starts_with("courier") {
            Some(WidthTable::Fixed(600.0))
        } else if n.starts_with("times") {
            Some(WidthTable::Standard(crate::fonts::FontFace::Serif))
        } else if n.starts_with("helvetica") || n.starts_with("arial") {
            Some(WidthTable::Standard(if n.contains("bold") {
                crate::fonts::FontFace::SansBold
            } else {
                crate::fonts::FontFace::Sans
            }))
        } else {
            None
        }
    });
    Metrics {
        two_byte: false,
        widths,
        type3: subtype == b"Type3",
        name,
    }
}

#[derive(Clone)]
struct State {
    ctm: Affine,
    color: Color,
    tc: f32,
    tw: f32,
    th: f32,
    tl: f32,
    font: Option<Vec<u8>>,
    size: f32,
}

/// One text-showing operator and where it draws.
#[derive(Clone, Debug)]
pub(crate) struct Show {
    pub(crate) index: usize,
    /// Each code drawn: which part of the operator (TJ element), byte range, box, advance.
    pub(crate) glyphs: Vec<GlyphBox>,
    /// Font size × horizontal scaling (to express advances as TJ numbers).
    pub(crate) size_th: f32,
    /// Baseline start in view points; `None` when an earlier advance was unknown.
    start: Option<Point>,
    /// Baseline end in view points, when widths are known.
    end: Option<Point>,
    /// Horizontal displacement in unscaled text space, when known.
    advance: Option<f32>,
    size: f32,
    pub(crate) font: Vec<u8>,
    color: Color,
    codes: usize,
    /// The text x axis in view space (to reject rotated text).
    dir: Vec2,
}

fn color_from(ops: &[Object]) -> Option<Color> {
    let v: Vec<f32> = ops.iter().filter_map(|o| o.as_float().ok()).collect();
    match v.len() {
        1 => Some(Color {
            r: v[0],
            g: v[0],
            b: v[0],
        }),
        3 => Some(Color {
            r: v[0],
            g: v[1],
            b: v[2],
        }),
        4 => Some(Color {
            r: (1.0 - v[0]) * (1.0 - v[3]),
            g: (1.0 - v[1]) * (1.0 - v[3]),
            b: (1.0 - v[2]) * (1.0 - v[3]),
        }),
        _ => None,
    }
}

pub(crate) fn affine(ops: &[Object]) -> Option<Affine> {
    let v: Vec<f64> = ops
        .iter()
        .filter_map(|o| o.as_float().ok())
        .map(f64::from)
        .collect();
    (v.len() == 6).then(|| Affine::new([v[0], v[1], v[2], v[3], v[4], v[5]]))
}

pub(crate) struct Walk {
    pub(crate) shows: Vec<Show>,
    /// Image XObjects drawn (operator index, resource name, unit square → view).
    pub(crate) images: Vec<(usize, Vec<u8>, Affine)>,
    /// Form XObjects drawn (operator index, resource name, form space → view).
    pub(crate) forms: Vec<(usize, Vec<u8>, Affine)>,
    /// Painted paths: (index of the painting operator, bounds in view points).
    pub(crate) paths: Vec<(usize, Rect)>,
    /// Inline images (BI … EI) were present.
    pub(crate) inline_images: bool,
}

/// One drawn code of a text-showing operator.
#[derive(Clone, Debug)]
pub(crate) struct GlyphBox {
    pub(crate) part: usize,
    pub(crate) byte: usize,
    pub(crate) len: usize,
    /// Bounds in view points (only when the position is known).
    pub(crate) rect: Option<Rect>,
    /// Displacement in text space, when known.
    pub(crate) advance: Option<f32>,
}

pub(crate) fn walk(
    ops: &[Operation],
    fonts: &HashMap<Vec<u8>, Metrics>,
    xobjects: &HashMap<Vec<u8>, bool>,
    to_view: Affine,
) -> Walk {
    let mut st = State {
        ctm: Affine::IDENTITY,
        color: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        },
        tc: 0.0,
        tw: 0.0,
        th: 1.0,
        tl: 0.0,
        font: None,
        size: 0.0,
    };
    let mut stack: Vec<State> = Vec::new();
    let mut tm = Affine::IDENTITY;
    let mut tlm = Affine::IDENTITY;
    let mut known = true;
    let mut shows = Vec::new();
    let mut images = Vec::new();
    let mut forms = Vec::new();
    let mut paths = Vec::new();
    let mut inline_images = false;
    let mut path_pts: Vec<KPoint> = Vec::new();
    let f = |o: &Operation, i: usize| {
        o.operands
            .get(i)
            .and_then(|x| x.as_float().ok())
            .unwrap_or(0.0)
    };
    for (index, op) in ops.iter().enumerate() {
        match op.operator.as_str() {
            "q" => stack.push(st.clone()),
            "Q" => {
                if let Some(s) = stack.pop() {
                    st = s;
                }
            }
            "cm" => {
                if let Some(m) = affine(&op.operands) {
                    st.ctm *= m;
                }
            }
            "BT" => {
                tm = Affine::IDENTITY;
                tlm = Affine::IDENTITY;
                known = true;
            }
            "Td" | "TD" => {
                let (tx, ty) = (f(op, 0), f(op, 1));
                if op.operator == "TD" {
                    st.tl = -ty;
                }
                tlm *= Affine::translate((f64::from(tx), f64::from(ty)));
                tm = tlm;
                known = true;
            }
            "Tm" => {
                if let Some(m) = affine(&op.operands) {
                    tlm = m;
                    tm = m;
                    known = true;
                }
            }
            "T*" => {
                tlm *= Affine::translate((0.0, -f64::from(st.tl)));
                tm = tlm;
                known = true;
            }
            "Tc" => st.tc = f(op, 0),
            "Tw" => st.tw = f(op, 0),
            "Tz" => st.th = f(op, 0) / 100.0,
            "TL" => st.tl = f(op, 0),
            "Tf" => {
                st.font = op
                    .operands
                    .first()
                    .and_then(|n| n.as_name().ok())
                    .map(<[u8]>::to_vec);
                st.size = f(op, 1);
            }
            "rg" | "g" | "k" | "sc" | "scn" => {
                if let Some(c) = color_from(&op.operands) {
                    st.color = c;
                }
            }
            "Tj" | "TJ" | "'" | "\"" => {
                if op.operator == "\"" {
                    st.tw = f(op, 0);
                    st.tc = f(op, 1);
                }
                if op.operator == "'" || op.operator == "\"" {
                    tlm *= Affine::translate((0.0, -f64::from(st.tl)));
                    tm = tlm;
                    known = true;
                }
                let Some(font) = st.font.clone() else {
                    continue;
                };
                let metrics = fonts.get(&font);
                let full = to_view * st.ctm * tm;
                let start = known.then(|| {
                    let p = full * KPoint::ORIGIN;
                    Point::new(p.x as f32, p.y as f32)
                });
                let dir = (full * KPoint::new(1.0, 0.0)) - (full * KPoint::ORIGIN);
                let size_vec =
                    (full * KPoint::new(0.0, f64::from(st.size))) - (full * KPoint::ORIGIN);
                // Collect strings and TJ adjustments.
                let parts: Vec<&Object> = match op.operator.as_str() {
                    "TJ" => op
                        .operands
                        .first()
                        .and_then(|a| a.as_array().ok())
                        .map(|a| a.iter().collect())
                        .unwrap_or_default(),
                    _ => op.operands.last().into_iter().collect(),
                };
                let mut advance = Some(0.0f32);
                let mut codes = 0usize;
                let mut glyphs = Vec::new();
                // "Up" on screen, whatever the matrices' orientation (y grows downwards in view
                // space; some producers flip the text matrix).
                let up = if size_vec.y > 0.0 {
                    -size_vec
                } else {
                    size_vec
                };
                for (part_index, part) in parts.into_iter().enumerate() {
                    match part {
                        Object::String(bytes, _) => {
                            let two = metrics.is_some_and(|m| m.two_byte);
                            let step = if two { 2 } else { 1 };
                            let list: Vec<u32> = if two {
                                bytes
                                    .chunks(2)
                                    .map(|c| {
                                        u32::from(c[0]) << 8 | u32::from(*c.get(1).unwrap_or(&0))
                                    })
                                    .collect()
                            } else {
                                bytes.iter().map(|&b| u32::from(b)).collect()
                            };
                            codes += list.len();
                            for (k, code) in list.into_iter().enumerate() {
                                let w = metrics.and_then(|m| m.width(code));
                                let before = advance;
                                let own = w.map(|w| {
                                    let space = if !two && code == 32 { st.tw } else { 0.0 };
                                    (w / 1000.0 * st.size + st.tc + space) * st.th
                                });
                                advance = match (advance, own) {
                                    (Some(a), Some(o)) => Some(a + o),
                                    _ => None,
                                };
                                let rect = (known && start.is_some())
                                    .then_some(())
                                    .and(before.zip(advance))
                                    .map(|(a, b)| {
                                        let p0 = full * KPoint::new(f64::from(a), 0.0);
                                        let p1 = full * KPoint::new(f64::from(b), 0.0);
                                        let pts = [
                                            p0 + up * 0.85,
                                            p1 + up * 0.85,
                                            p0 - up * 0.2,
                                            p1 - up * 0.2,
                                        ];
                                        let xs = pts.map(|p| p.x as f32);
                                        let ys = pts.map(|p| p.y as f32);
                                        Rect::new(
                                            xs.iter().copied().fold(f32::MAX, f32::min),
                                            ys.iter().copied().fold(f32::MAX, f32::min),
                                            xs.iter().copied().fold(f32::MIN, f32::max),
                                            ys.iter().copied().fold(f32::MIN, f32::max),
                                        )
                                    });
                                glyphs.push(GlyphBox {
                                    part: part_index,
                                    byte: k * step,
                                    len: step,
                                    rect,
                                    advance: own,
                                });
                            }
                        }
                        other => {
                            if let (Some(a), Ok(n)) = (advance, other.as_float()) {
                                advance = Some(a - n / 1000.0 * st.size * st.th);
                            }
                        }
                    }
                }
                let end = advance.zip(start).map(|(a, _)| {
                    let p = full * KPoint::new(f64::from(a), 0.0);
                    Point::new(p.x as f32, p.y as f32)
                });
                shows.push(Show {
                    index,
                    glyphs,
                    size_th: st.size * st.th,
                    start,
                    end,
                    advance,
                    size: size_vec.length() as f32,
                    font,
                    color: st.color,
                    codes,
                    dir,
                });
                match advance {
                    Some(a) => tm *= Affine::translate((f64::from(a), 0.0)),
                    None => known = false,
                }
            }
            "m" | "l" => {
                path_pts
                    .push(to_view * st.ctm * KPoint::new(f64::from(f(op, 0)), f64::from(f(op, 1))));
            }
            "c" => {
                for k in 0..3 {
                    path_pts.push(
                        to_view
                            * st.ctm
                            * KPoint::new(f64::from(f(op, 2 * k)), f64::from(f(op, 2 * k + 1))),
                    );
                }
            }
            "v" | "y" => {
                for k in 0..2 {
                    path_pts.push(
                        to_view
                            * st.ctm
                            * KPoint::new(f64::from(f(op, 2 * k)), f64::from(f(op, 2 * k + 1))),
                    );
                }
            }
            "re" => {
                let (x, y, w, h) = (
                    f64::from(f(op, 0)),
                    f64::from(f(op, 1)),
                    f64::from(f(op, 2)),
                    f64::from(f(op, 3)),
                );
                for (px, py) in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)] {
                    path_pts.push(to_view * st.ctm * KPoint::new(px, py));
                }
            }
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                if !path_pts.is_empty() {
                    let xs: Vec<f32> = path_pts.iter().map(|p| p.x as f32).collect();
                    let ys: Vec<f32> = path_pts.iter().map(|p| p.y as f32).collect();
                    paths.push((
                        index,
                        Rect::new(
                            xs.iter().copied().fold(f32::MAX, f32::min),
                            ys.iter().copied().fold(f32::MAX, f32::min),
                            xs.iter().copied().fold(f32::MIN, f32::max),
                            ys.iter().copied().fold(f32::MIN, f32::max),
                        ),
                    ));
                }
                path_pts.clear();
            }
            "n" => path_pts.clear(),
            "Do" => {
                if let Some(name) = op.operands.first().and_then(|n| n.as_name().ok()) {
                    let m = to_view * st.ctm;
                    match xobjects.get(name) {
                        Some(true) => images.push((index, name.to_vec(), m)),
                        Some(false) => forms.push((index, name.to_vec(), m)),
                        None => {}
                    }
                }
            }
            "BI" | "ID" | "EI" => inline_images = true,
            _ => {}
        }
    }
    Walk {
        shows,
        images,
        forms,
        paths,
        inline_images,
    }
}

/// The original (non-item) content of a page, decoded.
pub(crate) fn page_ops(
    doc: &lopdf::Document,
    page: ObjectId,
) -> Result<(Vec<Operation>, Vec<ObjectId>), String> {
    let list = items::contents(doc, page).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let mut keep = Vec::new();
    for id in list {
        if items::is_item(doc, id) {
            keep.push(id);
            continue;
        }
        let Ok(Object::Stream(s)) = doc.get_object(id) else {
            continue;
        };
        let data = s
            .decompressed_content()
            .unwrap_or_else(|_| s.content.clone());
        bytes.extend_from_slice(&data);
        bytes.push(b'\n');
    }
    let content = Content::decode(&bytes).map_err(|_| {
        "The page content could not be read, so it can't be edited safely.".to_string()
    })?;
    Ok((content.operations, keep))
}

/// A page's resources dictionary (resolved).
pub(crate) fn page_resources(doc: &lopdf::Document, page: ObjectId) -> Dictionary {
    doc.get_dictionary(page)
        .ok()
        .and_then(|p| p.get(b"Resources").ok())
        .map(|r| deref(doc, r))
        .and_then(|r| r.as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

/// XObject names of a resources dictionary and whether each is an image (true) or a form.
pub(crate) fn xobject_kinds(doc: &lopdf::Document, res: &Dictionary) -> HashMap<Vec<u8>, bool> {
    let mut out = HashMap::new();
    let Ok(xo) = res
        .get(b"XObject")
        .map(|f| deref(doc, f))
        .and_then(Object::as_dict)
    else {
        return out;
    };
    for (name, o) in xo.iter() {
        if let Object::Stream(s) = deref(doc, o) {
            out.insert(
                name.clone(),
                s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"),
            );
        }
    }
    out
}

/// Font metrics for the fonts of a resources dictionary.
pub(crate) fn resource_fonts(doc: &lopdf::Document, res: &Dictionary) -> HashMap<Vec<u8>, Metrics> {
    let mut out = HashMap::new();
    let Ok(fonts) = res
        .get(b"Font")
        .map(|f| deref(doc, f))
        .and_then(Object::as_dict)
    else {
        return out;
    };
    for (name, f) in fonts.iter() {
        if let Ok(d) = deref(doc, f).as_dict() {
            out.insert(name.clone(), metrics(doc, d));
        }
    }
    out
}

pub(crate) fn page_fonts(doc: &lopdf::Document, page: ObjectId) -> HashMap<Vec<u8>, Metrics> {
    let mut out = HashMap::new();
    let Ok(page) = doc.get_dictionary(page) else {
        return out;
    };
    let res = page
        .get(b"Resources")
        .map(|r| deref(doc, r))
        .and_then(Object::as_dict);
    let Ok(fonts) = res
        .and_then(|r| r.get(b"Font"))
        .map(|f| deref(doc, f))
        .and_then(Object::as_dict)
    else {
        return out;
    };
    for (name, f) in fonts.iter() {
        if let Ok(d) = deref(doc, f).as_dict() {
            out.insert(name.clone(), metrics(doc, d));
        }
    }
    out
}

fn pick_face(name: &str) -> FontFace {
    let n = name.to_ascii_lowercase();
    if n.contains("times")
        || n.contains("serif") && !n.contains("sans")
        || n.contains("roman")
        || n.contains("georgia")
        || n.contains("garamond")
    {
        FontFace::Serif
    } else if n.contains("bold") || n.contains("black") || n.contains("heavy") {
        FontFace::SansBold
    } else {
        FontFace::Sans
    }
}

struct Found {
    line: TextLine,
    shows: Vec<Show>,
}

fn find_line(
    doc: &lopdf::Document,
    page: ObjectId,
    to_view: Affine,
    text: &PageText,
    at: Point,
) -> Result<Found, String> {
    let g = text
        .glyph_at(at, 4.0)
        .ok_or("Click directly on a line of text to edit it.")?;
    let glyphs = &text.glyphs;
    let mut a = g;
    while a > 0 && glyphs[a - 1].ch != '\n' {
        a -= 1;
    }
    let mut b = g;
    while b + 1 < glyphs.len() && glyphs[b + 1].ch != '\n' {
        b += 1;
    }
    let rects = text.line_rects(a, b);
    let rect = rects.iter().skip(1).fold(rects[0], |r, q| r.union(*q));
    let line_text = text.slice(a, b).trim().to_string();
    let glyph_count = glyphs[a..=b].iter().filter(|g| !g.synthetic).count();

    let fonts = page_fonts(doc, page);
    let (ops, _) = page_ops(doc, page)?;
    let walk = walk(&ops, &fonts, &HashMap::new(), to_view);
    let slack = rect.height() * 0.35;
    let shows: Vec<Show> = walk
        .shows
        .into_iter()
        .filter(|s| {
            s.start.is_some_and(|p| {
                p.y >= rect.y0 - slack
                    && p.y <= rect.y1 + slack
                    && p.x >= rect.x0 - slack
                    && p.x <= rect.x1 + slack
            })
        })
        .collect();
    let Some(first) = shows
        .iter()
        .min_by(|x, y| x.start.unwrap().x.total_cmp(&y.start.unwrap().x))
        .cloned()
    else {
        return Err("This text is drawn inside a form object or as outlines, so it can't be edited in place.".into());
    };
    if shows
        .iter()
        .any(|s| fonts.get(&s.font).is_some_and(|m| m.type3))
    {
        return Err(
            "This line uses a Type 3 font (glyphs drawn as pictures), which can't be edited."
                .into(),
        );
    }
    if shows
        .iter()
        .any(|s| s.dir.x < 0.5 || s.dir.y.abs() > 0.01 * s.dir.x.abs().max(1.0))
    {
        return Err("Rotated or slanted text can't be edited yet.".into());
    }
    let start = first.start.unwrap();
    if (start.x - rect.x0).abs() > first.size.max(4.0) {
        return Err(
            "This line shares a text run with other text, so it can't be edited cleanly.".into(),
        );
    }
    if shows
        .iter()
        .any(|s| s.end.is_some_and(|e| e.x > rect.x1 + first.size))
    {
        return Err(
            "This line continues into other text in the same run, so it can't be edited cleanly."
                .into(),
        );
    }
    let codes: usize = shows.iter().map(|s| s.codes).sum();
    if codes.abs_diff(glyph_count) > (glyph_count / 10).max(1) {
        return Err("This line's text could not be matched to the page content (it may be drawn in several layers).".into());
    }
    let font_name = fonts
        .get(&first.font)
        .map(|m| m.name.clone())
        .unwrap_or_default();
    let line = TextLine {
        rect,
        text: line_text,
        size: first.size,
        color: first.color,
        face: pick_face(&font_name),
        font_name,
        baseline: start,
    };
    Ok(Found { line, shows })
}

pub(crate) fn line_at(
    doc: &lopdf::Document,
    page: ObjectId,
    to_view: Affine,
    text: &PageText,
    at: Point,
) -> Result<TextLine, String> {
    find_line(doc, page, to_view, text, at).map(|f| f.line)
}

/// Removes the line at `at` from the page content and writes `new_text` in its
/// place as a text item. Returns the line that was replaced.
pub(crate) fn replace_line(
    doc: &mut lopdf::Document,
    page: ObjectId,
    to_view: Affine,
    text: &PageText,
    at: Point,
    new_text: &str,
    style: Option<(FontFace, f32, Color)>,
) -> Result<TextLine, String> {
    let found = find_line(doc, page, to_view, text, at)?;
    let (mut ops, keep) = page_ops(doc, page)?;
    // Replace from the back so indices stay valid.
    let mut targets: Vec<&Show> = found.shows.iter().collect();
    targets.sort_by_key(|s| std::cmp::Reverse(s.index));
    for show in targets {
        // Does a later show operator rely on this one's advance?
        let needs_advance = ops[show.index + 1..]
            .iter()
            .map(|o| o.operator.as_str())
            .find(|o| {
                matches!(
                    *o,
                    "Tj" | "TJ" | "'" | "\"" | "Td" | "TD" | "Tm" | "T*" | "ET" | "BT"
                )
            })
            .is_some_and(|o| o == "Tj" || o == "TJ");
        let op = ops[show.index].clone();
        let mut replacement = Vec::new();
        if op.operator == "\"" {
            replacement.push(Operation::new("Tw", vec![op.operands[0].clone()]));
            replacement.push(Operation::new("Tc", vec![op.operands[1].clone()]));
        }
        if op.operator == "'" || op.operator == "\"" {
            replacement.push(Operation::new("T*", vec![]));
        }
        if needs_advance {
            let Some(adv) = show.advance else {
                return Err("This line's font has no width information, so removing it would shift other text.".into());
            };
            // Same displacement, no glyphs: [-(adv / size * 1000)] TJ with Th folded in.
            let size_th = text_size(&ops[..show.index]);
            if size_th.abs() < 1e-6 {
                return Err("The text size of this line could not be determined.".into());
            }
            replacement.push(Operation::new(
                "TJ",
                vec![Object::Array(vec![Object::Real(-adv / size_th * 1000.0)])],
            ));
        }
        ops.splice(show.index..=show.index, replacement);
    }
    let bytes = Content { operations: ops }
        .encode()
        .map_err(|_| "The edited page content could not be written.".to_string())?;
    let mut stream = Stream::new(Dictionary::new(), bytes);
    let _ = stream.compress();
    let id = doc.add_object(stream);
    let mut list = vec![id];
    list.extend(keep);
    items::set_contents(doc, page, &list).map_err(|e| e.to_string())?;

    let line = found.line;
    if !new_text.trim().is_empty() {
        let (face, size, color) = style.unwrap_or((line.face, line.size, line.color));
        let item = items::NewItem::Text {
            at: Point::new(line.baseline.x - 1.0, line.baseline.y - size * 0.95),
            text: new_text.to_string(),
            face,
            size,
            color,
        };
        items::add(doc, page, to_view.inverse(), &item).map_err(|e| e.to_string())?;
    }
    doc.prune_objects();
    Ok(line)
}

/// Font size × horizontal scaling in effect after `ops` (for TJ adjustments).
fn text_size(ops: &[Operation]) -> f32 {
    let mut size = 0.0;
    let mut th = 1.0;
    let mut stack = Vec::new();
    for op in ops {
        match op.operator.as_str() {
            "q" => stack.push((size, th)),
            "Q" => {
                if let Some((s, t)) = stack.pop() {
                    size = s;
                    th = t;
                }
            }
            "Tf" => {
                size = op
                    .operands
                    .get(1)
                    .and_then(|o| o.as_float().ok())
                    .unwrap_or(size)
            }
            "Tz" => {
                th = op
                    .operands
                    .first()
                    .and_then(|o| o.as_float().ok())
                    .unwrap_or(100.0)
                    / 100.0
            }
            _ => {}
        }
    }
    size * th
}
