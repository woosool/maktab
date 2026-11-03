//! Creating, listing and removing annotations.
//!
//! Every annotation gets a normal appearance stream so viewers that do not
//! synthesize appearances (and our own renderer) show it. Appearance content is
//! written in view points; the form's /Matrix maps it back to PDF user space, so
//! annotations look right on rotated and cropped pages too.

use hayro::kurbo::{Affine, Point as KPoint};
use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat, dictionary};

use crate::document::{Error, Result};
use crate::geometry::{Point, Rect};

/// An RGB colour with components in 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
        }
    }
    fn array(self) -> Object {
        vec![self.r.into(), self.g.into(), self.b.into()].into()
    }
    fn op(self, op: &str) -> String {
        format!("{} {} {} {op}", num(self.r), num(self.g), num(self.b))
    }
}

/// The kinds of annotation the editor understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotKind {
    Highlight,
    Underline,
    StrikeOut,
    FreeText,
    Note,
    Square,
    Circle,
    Ink,
    Link,
    Widget,
    Other,
}

impl AnnotKind {
    fn from_subtype(name: &[u8]) -> Self {
        match name {
            b"Highlight" => Self::Highlight,
            b"Underline" => Self::Underline,
            b"StrikeOut" => Self::StrikeOut,
            b"FreeText" => Self::FreeText,
            b"Text" => Self::Note,
            b"Square" => Self::Square,
            b"Circle" => Self::Circle,
            b"Ink" => Self::Ink,
            b"Link" => Self::Link,
            b"Widget" => Self::Widget,
            _ => Self::Other,
        }
    }
    /// A short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Highlight => "Highlight",
            Self::Underline => "Underline",
            Self::StrikeOut => "Strikethrough",
            Self::FreeText => "Text box",
            Self::Note => "Sticky note",
            Self::Square => "Rectangle",
            Self::Circle => "Ellipse",
            Self::Ink => "Drawing",
            Self::Link => "Link",
            Self::Widget => "Form field",
            Self::Other => "Comment",
        }
    }
    /// Comments a reviewer would see in a comments list (not links or fields).
    pub fn is_comment(self) -> bool {
        !matches!(self, Self::Link | Self::Widget | Self::Other)
    }
}

/// An annotation to add, in view points.
#[derive(Clone, Debug, PartialEq)]
pub enum NewAnnot {
    /// `quads` are one rectangle per line of marked text.
    Highlight {
        quads: Vec<Rect>,
        color: Color,
    },
    Underline {
        quads: Vec<Rect>,
        color: Color,
    },
    StrikeOut {
        quads: Vec<Rect>,
        color: Color,
    },
    FreeText {
        rect: Rect,
        text: String,
        color: Color,
        size: f32,
        /// Background behind the text (`None` = transparent).
        fill: Option<Color>,
    },
    Note {
        at: Point,
        text: String,
        color: Color,
    },
    Square {
        rect: Rect,
        color: Color,
        width: f32,
        /// Interior colour (`None` = no fill).
        fill: Option<Color>,
        /// Opacity of the whole shape, 0..=1.
        opacity: f32,
    },
    Circle {
        rect: Rect,
        color: Color,
        width: f32,
        fill: Option<Color>,
        opacity: f32,
    },
    Ink {
        strokes: Vec<Vec<Point>>,
        color: Color,
        width: f32,
    },
}

/// An existing annotation as listed for the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct AnnotSummary {
    /// Position in the page's /Annots array (used to delete it).
    pub slot: usize,
    pub kind: AnnotKind,
    /// Bounds in view points.
    pub rect: Rect,
    pub contents: String,
    pub author: String,
    pub color: Option<Color>,
}

const AUTHOR: &str = "Waraq";

/// Builds a complete annotation dictionary (with its appearance stream).
fn make(
    doc: &mut lopdf::Document,
    page: ObjectId,
    to_user: Affine,
    annot: &NewAnnot,
) -> Result<Dictionary> {
    let (subtype, color, view_rect, content, extra, resources) = build(doc, annot)?;
    let view_rect = view_rect.expand(1.0);
    let user_rect = bbox(to_user, view_rect);

    let mut ap_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => rect_array(view_rect),
        "Matrix" => to_user.as_coeffs().iter().map(|&c| Object::Real(c as f32)).collect::<Vec<_>>(),
    };
    if let Some(res) = resources {
        ap_dict.set("Resources", res);
    }
    let ap = doc.add_object(Stream::new(ap_dict, content.into_bytes()));

    let mut dict = dictionary! {
        "Type" => "Annot",
        "Subtype" => subtype,
        "Rect" => rect_array(user_rect),
        "P" => page,
        "F" => 4, // Print
        "C" => color.array(),
        "T" => text_string(AUTHOR),
        "M" => Object::string_literal(pdf_date()),
        "AP" => dictionary! { "N" => ap },
    };
    for (k, v) in extra(to_user) {
        dict.set(k, v);
    }
    Ok(dict)
}

fn annots_of(doc: &lopdf::Document, page: ObjectId) -> Result<Vec<Object>> {
    Ok(match doc.get_dictionary(page)?.get(b"Annots") {
        Ok(Object::Reference(id)) => doc.get_object(*id)?.as_array()?.clone(),
        Ok(Object::Array(a)) => a.clone(),
        _ => Vec::new(),
    })
}

pub(crate) fn add(
    doc: &mut lopdf::Document,
    page: ObjectId,
    to_user: Affine,
    annot: &NewAnnot,
) -> Result<()> {
    let dict = make(doc, page, to_user, annot)?;
    let annot_id = doc.add_object(dict);
    let mut annots = annots_of(doc, page)?;
    annots.push(Object::Reference(annot_id));
    doc.get_dictionary_mut(page)?.set("Annots", annots);
    Ok(())
}

/// Rewrites the annotation in `slot` as `annot`, keeping its identity (author,
/// name, creation date, popup) and its place in the page's /Annots.
pub(crate) fn replace(
    doc: &mut lopdf::Document,
    page: ObjectId,
    slot: usize,
    to_user: Affine,
    annot: &NewAnnot,
) -> Result<()> {
    let mut annots = annots_of(doc, page)?;
    let old_obj = annots
        .get(slot)
        .cloned()
        .ok_or(Error::Invalid("That comment no longer exists."))?;
    let old = match &old_obj {
        Object::Reference(id) => doc.get_dictionary(*id)?.clone(),
        Object::Dictionary(d) => d.clone(),
        _ => return Err(Error::Invalid("That comment can't be edited.")),
    };
    let mut dict = make(doc, page, to_user, annot)?;
    for key in [&b"T"[..], b"NM", b"CreationDate", b"Popup", b"F", b"Subj"] {
        if let Ok(v) = old.get(key) {
            dict.set(key.to_vec(), v.clone());
        }
    }
    match old_obj {
        Object::Reference(id) => {
            doc.objects.insert(id, Object::Dictionary(dict));
        }
        _ => {
            annots[slot] = Object::Reference(doc.add_object(dict));
            doc.get_dictionary_mut(page)?.set("Annots", annots);
        }
    }
    doc.prune_objects();
    Ok(())
}

/// Reads an annotation back as an editable shape in view points, for the
/// kinds the editor can rewrite (from this app or any other).
pub(crate) fn read(
    doc: &lopdf::Document,
    page: ObjectId,
    slot: usize,
    to_view: Affine,
) -> Option<NewAnnot> {
    let annots = annots_of(doc, page).ok()?;
    let dict = match annots.get(slot)? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    let kind = AnnotKind::from_subtype(dict.get(b"Subtype").ok()?.as_name().ok()?);
    let color = dict
        .get(b"C")
        .ok()
        .and_then(|c| c.as_array().ok())
        .and_then(|c| {
            let v: Vec<f32> = c.iter().filter_map(|x| x.as_float().ok()).collect();
            match v.len() {
                3 => Some(Color {
                    r: v[0],
                    g: v[1],
                    b: v[2],
                }),
                1 => Some(Color {
                    r: v[0],
                    g: v[0],
                    b: v[0],
                }),
                _ => None,
            }
        });
    let rect = bbox(to_view, read_rect(dict.get(b"Rect").ok()?)?);
    let contents = dict.get(b"Contents").map(decode_text).unwrap_or_default();
    let width = dict
        .get(b"BS")
        .ok()
        .and_then(|b| b.as_dict().ok())
        .and_then(|b| b.get(b"W").ok())
        .and_then(|w| w.as_float().ok())
        .or_else(|| {
            dict.get(b"Border")
                .ok()?
                .as_array()
                .ok()?
                .get(2)?
                .as_float()
                .ok()
        })
        .unwrap_or(1.0);
    let map = |x: f32, y: f32| {
        let p = to_view * KPoint::new(x as f64, y as f64);
        Point::new(p.x as f32, p.y as f32)
    };
    let red = Color::rgb(200, 45, 45);
    let interior = dict
        .get(b"IC")
        .ok()
        .and_then(|c| c.as_array().ok())
        .and_then(|c| {
            let v: Vec<f32> = c.iter().filter_map(|x| x.as_float().ok()).collect();
            (v.len() == 3).then(|| Color {
                r: v[0],
                g: v[1],
                b: v[2],
            })
        });
    let opacity = dict
        .get(b"CA")
        .ok()
        .and_then(|o| o.as_float().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    Some(match kind {
        AnnotKind::Highlight | AnnotKind::Underline | AnnotKind::StrikeOut => {
            let q: Vec<f32> = dict
                .get(b"QuadPoints")
                .ok()?
                .as_array()
                .ok()?
                .iter()
                .filter_map(|x| x.as_float().ok())
                .collect();
            let quads: Vec<Rect> = q
                .chunks_exact(8)
                .map(|c| {
                    let pts: Vec<Point> = c.chunks_exact(2).map(|p| map(p[0], p[1])).collect();
                    pts.iter()
                        .skip(1)
                        .fold(Rect::new(pts[0].x, pts[0].y, pts[0].x, pts[0].y), |r, p| {
                            r.union(Rect::new(p.x, p.y, p.x, p.y))
                        })
                })
                .collect();
            let color = color.unwrap_or(Color::rgb(255, 214, 10));
            match kind {
                AnnotKind::Highlight => NewAnnot::Highlight { quads, color },
                AnnotKind::Underline => NewAnnot::Underline { quads, color },
                _ => NewAnnot::StrikeOut { quads, color },
            }
        }
        AnnotKind::FreeText => {
            let da = dict
                .get(b"DA")
                .ok()
                .and_then(|d| d.as_str().ok())
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            let tokens: Vec<&str> = da.split_whitespace().collect();
            let size = tokens
                .iter()
                .position(|t| *t == "Tf")
                .and_then(|i| tokens.get(i.checked_sub(1)?)?.parse::<f32>().ok())
                .filter(|s| *s > 0.0)
                .unwrap_or(12.0);
            let da_color = tokens.iter().position(|t| *t == "rg").and_then(|i| {
                let v: Vec<f32> = tokens
                    .get(i.checked_sub(3)?..i)?
                    .iter()
                    .filter_map(|t| t.parse().ok())
                    .collect();
                (v.len() == 3).then(|| Color {
                    r: v[0],
                    g: v[1],
                    b: v[2],
                })
            });
            NewAnnot::FreeText {
                rect: rect.expand(-1.0),
                text: contents,
                color: da_color.or(color).unwrap_or(Color::rgb(0, 0, 0)),
                size,
                fill: interior,
            }
        }
        AnnotKind::Note => NewAnnot::Note {
            at: Point::new(rect.x0, rect.y0),
            text: contents,
            color: color.unwrap_or(Color::rgb(255, 205, 70)),
        },
        AnnotKind::Square => NewAnnot::Square {
            rect: rect.expand(-1.0),
            color: color.unwrap_or(red),
            width,
            fill: interior,
            opacity,
        },
        AnnotKind::Circle => NewAnnot::Circle {
            rect: rect.expand(-1.0),
            color: color.unwrap_or(red),
            width,
            fill: interior,
            opacity,
        },
        AnnotKind::Ink => {
            let list = dict.get(b"InkList").ok()?.as_array().ok()?;
            let strokes = list
                .iter()
                .filter_map(|s| {
                    let v: Vec<f32> = s
                        .as_array()
                        .ok()?
                        .iter()
                        .filter_map(|x| x.as_float().ok())
                        .collect();
                    Some(v.chunks_exact(2).map(|c| map(c[0], c[1])).collect())
                })
                .collect();
            NewAnnot::Ink {
                strokes,
                color: color.unwrap_or(red),
                width,
            }
        }
        _ => return None,
    })
}

impl NewAnnot {
    /// The area the annotation covers (view points).
    pub fn bounds(&self) -> Rect {
        match self {
            NewAnnot::Highlight { quads, .. }
            | NewAnnot::Underline { quads, .. }
            | NewAnnot::StrikeOut { quads, .. } => quads
                .iter()
                .skip(1)
                .fold(quads.first().copied().unwrap_or_default(), |a, q| {
                    a.union(*q)
                }),
            NewAnnot::FreeText { rect, .. }
            | NewAnnot::Square { rect, .. }
            | NewAnnot::Circle { rect, .. } => rect.normalized(),
            NewAnnot::Note { at, .. } => Rect::new(at.x, at.y, at.x + 20.0, at.y + 20.0),
            NewAnnot::Ink { strokes, width, .. } => {
                let mut r: Option<Rect> = None;
                for p in strokes.iter().flatten() {
                    let q = Rect::new(p.x, p.y, p.x, p.y);
                    r = Some(r.map_or(q, |r| r.union(q)));
                }
                r.unwrap_or_default().expand(width / 2.0)
            }
        }
    }

    /// Whether handles can resize it (markup and notes only move).
    pub fn resizable(&self) -> bool {
        matches!(
            self,
            NewAnnot::FreeText { .. }
                | NewAnnot::Square { .. }
                | NewAnnot::Circle { .. }
                | NewAnnot::Ink { .. }
        )
    }

    /// The same annotation moved/scaled so that `from` lands on `to`.
    pub fn transformed(&self, from: Rect, to: Rect) -> NewAnnot {
        let m = |p: Point| crate::items::remap(p, from, to);
        let mr = |r: Rect| {
            let a = m(Point::new(r.x0, r.y0));
            let b = m(Point::new(r.x1, r.y1));
            Rect::from_points(a, b)
        };
        // Notes and text markup keep their size; they only move.
        let (dx, dy) = (to.x0 - from.x0, to.y0 - from.y0);
        let mv = |r: Rect| Rect::new(r.x0 + dx, r.y0 + dy, r.x1 + dx, r.y1 + dy);
        let mut out = self.clone();
        match &mut out {
            NewAnnot::Highlight { quads, .. }
            | NewAnnot::Underline { quads, .. }
            | NewAnnot::StrikeOut { quads, .. } => {
                for q in quads.iter_mut() {
                    *q = mv(*q);
                }
            }
            NewAnnot::Note { at, .. } => *at = Point::new(at.x + dx, at.y + dy),
            NewAnnot::FreeText { rect, .. }
            | NewAnnot::Square { rect, .. }
            | NewAnnot::Circle { rect, .. } => *rect = mr(*rect),
            NewAnnot::Ink { strokes, .. } => {
                for p in strokes.iter_mut().flatten() {
                    *p = m(*p);
                }
            }
        }
        out
    }

    pub fn color(&self) -> Color {
        match self {
            NewAnnot::Highlight { color, .. }
            | NewAnnot::Underline { color, .. }
            | NewAnnot::StrikeOut { color, .. }
            | NewAnnot::FreeText { color, .. }
            | NewAnnot::Note { color, .. }
            | NewAnnot::Square { color, .. }
            | NewAnnot::Circle { color, .. }
            | NewAnnot::Ink { color, .. } => *color,
        }
    }

    pub fn set_color(&mut self, c: Color) {
        match self {
            NewAnnot::Highlight { color, .. }
            | NewAnnot::Underline { color, .. }
            | NewAnnot::StrikeOut { color, .. }
            | NewAnnot::FreeText { color, .. }
            | NewAnnot::Note { color, .. }
            | NewAnnot::Square { color, .. }
            | NewAnnot::Circle { color, .. }
            | NewAnnot::Ink { color, .. } => *color = c,
        }
    }

    /// Whether the annotation has an interior/background colour setting.
    pub fn fillable(&self) -> bool {
        matches!(
            self,
            NewAnnot::Square { .. } | NewAnnot::Circle { .. } | NewAnnot::FreeText { .. }
        )
    }

    pub fn fill(&self) -> Option<Color> {
        match self {
            NewAnnot::Square { fill, .. }
            | NewAnnot::Circle { fill, .. }
            | NewAnnot::FreeText { fill, .. } => *fill,
            _ => None,
        }
    }

    pub fn set_fill(&mut self, c: Option<Color>) {
        if let NewAnnot::Square { fill, .. }
        | NewAnnot::Circle { fill, .. }
        | NewAnnot::FreeText { fill, .. } = self
        {
            *fill = c;
        }
    }

    /// Opacity of shapes (1 for other kinds).
    pub fn opacity(&self) -> Option<f32> {
        match self {
            NewAnnot::Square { opacity, .. } | NewAnnot::Circle { opacity, .. } => Some(*opacity),
            _ => None,
        }
    }

    pub fn set_opacity(&mut self, o: f32) {
        if let NewAnnot::Square { opacity, .. } | NewAnnot::Circle { opacity, .. } = self {
            *opacity = o.clamp(0.05, 1.0);
        }
    }

    /// Line width, for shapes and drawings.
    pub fn line_width(&self) -> Option<f32> {
        match self {
            NewAnnot::Square { width, .. }
            | NewAnnot::Circle { width, .. }
            | NewAnnot::Ink { width, .. } => Some(*width),
            _ => None,
        }
    }

    pub fn set_line_width(&mut self, w: f32) {
        if let NewAnnot::Square { width, .. }
        | NewAnnot::Circle { width, .. }
        | NewAnnot::Ink { width, .. } = self
        {
            *width = w;
        }
    }

    /// Text of text boxes and notes.
    pub fn text(&self) -> Option<&str> {
        match self {
            NewAnnot::FreeText { text, .. } | NewAnnot::Note { text, .. } => Some(text),
            _ => None,
        }
    }

    pub fn set_text(&mut self, t: String) {
        if let NewAnnot::FreeText { text, .. } | NewAnnot::Note { text, .. } = self {
            *text = t;
        }
    }

    pub fn font_size(&self) -> Option<f32> {
        match self {
            NewAnnot::FreeText { size, .. } => Some(*size),
            _ => None,
        }
    }

    /// Sets the text-box font size, growing the box to fit the text.
    pub fn set_font_size(&mut self, s: f32) {
        if let NewAnnot::FreeText {
            size, rect, text, ..
        } = self
        {
            *size = s;
            let (w, lines) = crate::fonts::measure(text, crate::fonts::FontFace::Sans, s);
            let h = lines as f32 * crate::fonts::leading(s) + 6.0;
            *rect = Rect::new(
                rect.x0,
                rect.y0,
                rect.x0 + rect.width().max(w + 6.0),
                rect.y0 + rect.height().max(h),
            );
        }
    }
}

type Extra = Box<dyn Fn(Affine) -> Vec<(&'static str, Object)>>;
type Built = (&'static str, Color, Rect, String, Extra, Option<Dictionary>);

fn build(doc: &mut lopdf::Document, annot: &NewAnnot) -> Result<Built> {
    Ok(match annot {
        NewAnnot::Highlight { quads, color }
        | NewAnnot::Underline { quads, color }
        | NewAnnot::StrikeOut { quads, color } => {
            let Some(first) = quads.first() else {
                return Err(Error::Invalid("Nothing to mark."));
            };
            let rect = quads.iter().fold(*first, |a, q| a.union(*q));
            let mut s = String::new();
            let mut res = None;
            let subtype = match annot {
                NewAnnot::Highlight { .. } => {
                    s += "/GS0 gs\n";
                    s += &color.op("rg");
                    s += "\n";
                    for q in quads {
                        s += &format!("{} re f\n", rect_ops(*q));
                    }
                    res = Some(dictionary! {
                        "ExtGState" => dictionary! {
                            "GS0" => dictionary! { "Type" => "ExtGState", "BM" => "Multiply" },
                        },
                    });
                    "Highlight"
                }
                NewAnnot::Underline { .. } | NewAnnot::StrikeOut { .. } => {
                    let under = matches!(annot, NewAnnot::Underline { .. });
                    s += &color.op("RG");
                    s += "\n";
                    for q in quads {
                        let w = (q.height() * 0.07).max(0.8);
                        let y = if under { q.y1 - w } else { q.center_y() };
                        s += &format!(
                            "{} w {} {} m {} {} l S\n",
                            num(w),
                            num(q.x0),
                            num(y),
                            num(q.x1),
                            num(y)
                        );
                    }
                    if under { "Underline" } else { "StrikeOut" }
                }
                _ => unreachable!(),
            };
            let quads = quads.clone();
            let extra: Extra = Box::new(move |m| {
                let mut pts = Vec::new();
                for q in &quads {
                    // Order: upper-left, upper-right, lower-left, lower-right (as displayed).
                    for (x, y) in [(q.x0, q.y0), (q.x1, q.y0), (q.x0, q.y1), (q.x1, q.y1)] {
                        let p = m * KPoint::new(x as f64, y as f64);
                        pts.push(Object::Real(p.x as f32));
                        pts.push(Object::Real(p.y as f32));
                    }
                }
                vec![("QuadPoints", Object::Array(pts))]
            });
            (subtype, *color, rect, s, extra, res)
        }
        NewAnnot::FreeText {
            rect,
            text,
            color,
            size,
            fill,
        } => {
            let size = size.max(4.0);
            let rect = rect.normalized();
            let mut painter = crate::fonts::TextPainter::default();
            painter.text_in_box(
                text,
                crate::fonts::FontFace::Sans,
                size,
                rect.x0 + 2.0,
                rect.x1 - 2.0,
                rect.y0 + 2.0 + size * 0.9,
                [color.r, color.g, color.b],
            );
            let background = fill
                .map(|f| format!("{} {} re f\n", f.op("rg"), rect_ops(rect)))
                .unwrap_or_default();
            let s = format!(
                "q {background}{} re W n\n{}Q\n",
                rect_ops(rect),
                painter.take_content()
            );
            let fill = *fill;
            let res = dictionary! { "Font" => painter.fonts(doc) };
            let da = format!("/Helv {} Tf {}", num(size), color.op("rg"));
            let text = text.clone();
            let extra: Extra = Box::new(move |_| {
                vec![
                    ("Contents", text_string(&text)),
                    ("DA", Object::string_literal(da.clone())),
                    (
                        "DS",
                        Object::string_literal(format!("font: Helvetica {}pt", num(size))),
                    ),
                ]
                .into_iter()
                .chain(fill.map(|f| ("IC", f.array())))
                .collect()
            });
            ("FreeText", *color, rect, s, extra, Some(res))
        }
        NewAnnot::Note { at, text, color } => {
            let rect = Rect::new(at.x, at.y, at.x + 20.0, at.y + 20.0);
            let inner = rect.expand(-1.5);
            let mut s = format!(
                "{} {} 1 w {} re B\n",
                color.op("rg"),
                Color::rgb(90, 70, 20).op("RG"),
                rect_ops(inner)
            );
            for i in 0..3 {
                let y = inner.y0 + 5.5 + i as f32 * 4.0;
                s += &format!(
                    "{} {} m {} {} l S\n",
                    num(inner.x0 + 4.0),
                    num(y),
                    num(inner.x1 - 4.0),
                    num(y)
                );
            }
            let text = text.clone();
            let extra: Extra = Box::new(move |_| {
                vec![
                    ("Contents", text_string(&text)),
                    ("Name", Object::Name(b"Comment".to_vec())),
                    ("Open", Object::Boolean(false)),
                ]
            });
            ("Text", *color, rect, s, extra, None)
        }
        NewAnnot::Square {
            rect,
            color,
            width,
            fill,
            opacity,
        }
        | NewAnnot::Circle {
            rect,
            color,
            width,
            fill,
            opacity,
        } => {
            let rect = rect.normalized();
            let w = width.max(0.5);
            let r = rect.expand(-w / 2.0);
            let square = matches!(annot, NewAnnot::Square { .. });
            let path = if square {
                format!("{} re", rect_ops(r))
            } else {
                ellipse(r)
            };
            let opacity = opacity.clamp(0.05, 1.0);
            let mut s = String::new();
            let mut res = None;
            if opacity < 1.0 {
                s += "/GS0 gs\n";
                res = Some(dictionary! {
                    "ExtGState" => dictionary! {
                        "GS0" => dictionary! { "Type" => "ExtGState", "ca" => Object::Real(opacity), "CA" => Object::Real(opacity) },
                    },
                });
            }
            if let Some(f) = fill {
                s += &format!("{} ", f.op("rg"));
            }
            s += &format!(
                "{} {} w {} {}\n",
                color.op("RG"),
                num(w),
                path,
                if fill.is_some() { "B" } else { "S" }
            );
            let fill = *fill;
            let extra: Extra = Box::new(move |_| {
                let mut v: Vec<(&'static str, Object)> =
                    vec![("BS", dictionary! { "W" => w, "S" => "S" }.into())];
                if let Some(f) = fill {
                    v.push(("IC", f.array()));
                }
                if opacity < 1.0 {
                    v.push(("CA", Object::Real(opacity)));
                }
                v
            });
            (
                if square { "Square" } else { "Circle" },
                *color,
                rect,
                s,
                extra,
                res,
            )
        }
        NewAnnot::Ink {
            strokes,
            color,
            width,
        } => {
            let pts: Vec<&Point> = strokes.iter().flatten().collect();
            let Some(first) = pts.first() else {
                return Err(Error::Invalid("Nothing drawn."));
            };
            let w = width.max(0.5);
            let rect = pts
                .iter()
                .fold(Rect::new(first.x, first.y, first.x, first.y), |r, p| {
                    r.union(Rect::new(p.x, p.y, p.x, p.y))
                })
                .expand(w);
            let mut s = format!("{} {} w 1 J 1 j\n", color.op("RG"), num(w));
            for stroke in strokes {
                for (i, p) in stroke.iter().enumerate() {
                    s += &format!(
                        "{} {} {} ",
                        num(p.x),
                        num(p.y),
                        if i == 0 { "m" } else { "l" }
                    );
                }
                if stroke.len() == 1 {
                    s += &format!("{} {} l ", num(stroke[0].x + 0.01), num(stroke[0].y));
                }
                s += "S\n";
            }
            let strokes = strokes.clone();
            let extra: Extra = Box::new(move |m| {
                let list = strokes
                    .iter()
                    .map(|stroke| {
                        Object::Array(
                            stroke
                                .iter()
                                .flat_map(|p| {
                                    let u = m * KPoint::new(p.x as f64, p.y as f64);
                                    [Object::Real(u.x as f32), Object::Real(u.y as f32)]
                                })
                                .collect(),
                        )
                    })
                    .collect::<Vec<_>>();
                vec![
                    ("InkList", Object::Array(list)),
                    ("BS", dictionary! { "W" => w }.into()),
                ]
            });
            ("Ink", *color, rect, s, extra, None)
        }
    })
}

pub(crate) fn list(doc: &lopdf::Document, page: ObjectId, to_view: Affine) -> Vec<AnnotSummary> {
    let Ok(page) = doc.get_dictionary(page) else {
        return Vec::new();
    };
    let annots = match page.get(b"Annots") {
        Ok(Object::Reference(id)) => doc.get_object(*id).and_then(Object::as_array).cloned(),
        Ok(Object::Array(a)) => Ok(a.clone()),
        _ => return Vec::new(),
    }
    .unwrap_or_default();
    let mut out = Vec::new();
    for (slot, a) in annots.iter().enumerate() {
        let dict = match a {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        };
        let Some(dict) = dict else { continue };
        let Ok(subtype) = dict.get(b"Subtype").and_then(Object::as_name) else {
            continue;
        };
        if subtype == b"Popup" {
            continue;
        }
        let Some(r) = dict.get(b"Rect").ok().and_then(read_rect) else {
            continue;
        };
        let color = dict
            .get(b"C")
            .ok()
            .and_then(|c| c.as_array().ok())
            .and_then(|c| {
                let v: Vec<f32> = c.iter().filter_map(|x| x.as_float().ok()).collect();
                (v.len() == 3).then(|| Color {
                    r: v[0],
                    g: v[1],
                    b: v[2],
                })
            });
        out.push(AnnotSummary {
            slot,
            kind: AnnotKind::from_subtype(subtype),
            rect: bbox(to_view, r),
            contents: dict
                .get(b"Contents")
                .ok()
                .map(decode_text)
                .unwrap_or_default(),
            author: dict.get(b"T").ok().map(decode_text).unwrap_or_default(),
            color,
        });
    }
    out
}

pub(crate) fn remove(doc: &mut lopdf::Document, page: ObjectId, slot: usize) -> Result<()> {
    let page_dict = doc.get_dictionary(page)?;
    let mut annots = match page_dict.get(b"Annots") {
        Ok(Object::Reference(id)) => doc.get_object(*id)?.as_array()?.clone(),
        Ok(Object::Array(a)) => a.clone(),
        _ => return Err(Error::Invalid("This page has no comments.")),
    };
    if slot >= annots.len() {
        return Err(Error::Invalid("That comment no longer exists."));
    }
    let removed = annots.remove(slot);
    // Drop its popup too, if it has one.
    if let Ok(id) = removed.as_reference()
        && let Ok(Object::Reference(popup)) = doc.get_dictionary(id).and_then(|d| d.get(b"Popup"))
    {
        let popup = *popup;
        annots.retain(|a| a.as_reference().ok() != Some(popup));
    }
    doc.get_dictionary_mut(page)?.set("Annots", annots);
    doc.prune_objects();
    Ok(())
}

// ----- helpers -----------------------------------------------------------

fn bbox(m: Affine, r: Rect) -> Rect {
    let pts = [(r.x0, r.y0), (r.x1, r.y0), (r.x0, r.y1), (r.x1, r.y1)]
        .map(|(x, y)| m * KPoint::new(x as f64, y as f64));
    let xs = pts.map(|p| p.x as f32);
    let ys = pts.map(|p| p.y as f32);
    Rect::new(
        xs.iter().copied().fold(f32::MAX, f32::min),
        ys.iter().copied().fold(f32::MAX, f32::min),
        xs.iter().copied().fold(f32::MIN, f32::max),
        ys.iter().copied().fold(f32::MIN, f32::max),
    )
}

fn read_rect(o: &Object) -> Option<Rect> {
    let a = o.as_array().ok()?;
    let v: Vec<f32> = a.iter().filter_map(|x| x.as_float().ok()).collect();
    (v.len() == 4).then(|| Rect::new(v[0], v[1], v[2], v[3]).normalized())
}

fn rect_array(r: Rect) -> Object {
    vec![
        Object::Real(r.x0),
        Object::Real(r.y0),
        Object::Real(r.x1),
        Object::Real(r.y1),
    ]
    .into()
}

fn rect_ops(r: Rect) -> String {
    format!(
        "{} {} {} {}",
        num(r.x0),
        num(r.y0),
        num(r.width()),
        num(r.height())
    )
}

/// Four Bézier arcs approximating the ellipse inscribed in `r`.
fn ellipse(r: Rect) -> String {
    const K: f32 = 0.552_284_8;
    let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    let (ox, oy) = (rx * K, ry * K);
    let c = |a: f32, b: f32, c2: f32, d: f32, e: f32, f: f32| {
        format!(
            "{} {} {} {} {} {} c ",
            num(a),
            num(b),
            num(c2),
            num(d),
            num(e),
            num(f)
        )
    };
    let mut s = format!("{} {} m ", num(cx + rx), num(cy));
    s += &c(cx + rx, cy + oy, cx + ox, cy + ry, cx, cy + ry);
    s += &c(cx - ox, cy + ry, cx - rx, cy + oy, cx - rx, cy);
    s += &c(cx - rx, cy - oy, cx - ox, cy - ry, cx, cy - ry);
    s += &c(cx + ox, cy - ry, cx + rx, cy - oy, cx + rx, cy);
    s + "h"
}

fn num(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// A PDF text string: plain if ASCII, otherwise UTF-16BE with a byte-order mark.
pub(crate) fn text_string(s: &str) -> Object {
    if s.is_ascii() {
        Object::string_literal(s)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            bytes.extend(u.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

pub(crate) fn decode_text(o: &Object) -> String {
    let Ok(b) = o.as_str() else {
        return String::new();
    };
    if b.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = b[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&b[3..]).into_owned()
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}

/// The current UTC time as a PDF date string.
fn pdf_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "D:{y:04}{m:02}{d:02}{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_compact() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(-0.0001), "0");
    }

    #[test]
    fn text_strings_round_trip() {
        assert_eq!(decode_text(&text_string("Hello")), "Hello");
        assert_eq!(decode_text(&text_string("مرحبا")), "مرحبا");
    }

    #[test]
    fn date_has_pdf_shape() {
        let d = pdf_date();
        assert!(
            d.starts_with("D:20") && d.ends_with('Z') && d.len() == 17,
            "{d}"
        );
    }
}
