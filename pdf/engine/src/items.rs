//! Page objects the editor adds to the page content itself (not annotations):
//! text, images and drawn strokes (signatures, check marks).
//!
//! Each item is one content stream appended to the page's /Contents that paints
//! one Form XObject (`q /MkXn Do Q`). The form carries the item's description in
//! a private `/MkItem` dictionary so the editor can move, resize and edit it
//! later; other viewers simply see ordinary page content. The original content
//! is wrapped in `q … Q` once so its graphics state cannot leak into items.

use hayro::kurbo::Affine;
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::annot::Color;
use crate::document::{Error, Result};
use crate::fonts::{self, FontFace, TextPainter, num};
use crate::geometry::{Point, Rect};

/// An item to add, in view points.
#[derive(Clone, Debug, PartialEq)]
pub enum NewItem {
    /// Text whose first line's top-left corner is at `at`.
    Text {
        at: Point,
        text: String,
        face: FontFace,
        size: f32,
        color: Color,
    },
    /// A PNG or JPEG placed in `rect`.
    Image { rect: Rect, data: Vec<u8> },
    /// Strokes in view points (a drawn signature, check mark, …).
    Ink {
        strokes: Vec<Vec<Point>>,
        color: Color,
        width: f32,
    },
}

/// What an existing item is.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    Text {
        text: String,
        face: FontFace,
        size: f32,
        color: Color,
    },
    Image,
    Ink {
        color: Color,
        width: f32,
    },
}

/// An item on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct PageItem {
    /// Stable id (object number of the item's content stream).
    pub id: u32,
    pub rect: Rect,
    pub kind: ItemKind,
}

/// Changes to an item; `None` keeps the current value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemEdit {
    pub rect: Option<Rect>,
    pub text: Option<String>,
    pub color: Option<Color>,
    pub size: Option<f32>,
    pub width: Option<f32>,
    pub face: Option<FontFace>,
}

/// Full description stored in the form's /MkItem dictionary.
#[derive(Clone, Debug)]
enum Spec {
    Text {
        text: String,
        face: FontFace,
        size: f32,
        color: Color,
        rect: Rect,
    },
    Image {
        image: ObjectId,
        rect: Rect,
    },
    Ink {
        strokes: Vec<Vec<Point>>,
        color: Color,
        width: f32,
        rect: Rect,
    },
}

impl Spec {
    fn rect(&self) -> Rect {
        match self {
            Spec::Text { rect, .. } | Spec::Image { rect, .. } | Spec::Ink { rect, .. } => *rect,
        }
    }
}

const TEXT_PAD: f32 = 1.0;

fn text_rect(at: Point, text: &str, face: FontFace, size: f32) -> Rect {
    let (w, lines) = fonts::measure(text, face, size);
    let h = (lines.max(1) - 1) as f32 * fonts::leading(size) + size * 1.2;
    Rect::new(at.x, at.y, at.x + w + 2.0 * TEXT_PAD, at.y + h)
}

fn ink_bounds(strokes: &[Vec<Point>], width: f32) -> Rect {
    let mut r: Option<Rect> = None;
    for p in strokes.iter().flatten() {
        let q = Rect::new(p.x, p.y, p.x, p.y);
        r = Some(r.map_or(q, |r| r.union(q)));
    }
    r.unwrap_or_default().expand(width / 2.0)
}

/// Maps points from one rectangle onto another.
pub(crate) fn remap(p: Point, from: Rect, to: Rect) -> Point {
    let sx = if from.width().abs() > 1e-3 {
        to.width() / from.width()
    } else {
        1.0
    };
    let sy = if from.height().abs() > 1e-3 {
        to.height() / from.height()
    } else {
        1.0
    };
    Point::new(to.x0 + (p.x - from.x0) * sx, to.y0 + (p.y - from.y0) * sy)
}

// ----- page content plumbing ----------------------------------------------

/// The page's /Contents as a list of stream ids (made into an array).
pub(crate) fn contents(doc: &lopdf::Document, page: ObjectId) -> Result<Vec<ObjectId>> {
    let list: Vec<ObjectId> = match doc.get_dictionary(page)?.get(b"Contents") {
        Ok(Object::Reference(id)) => match doc.get_object(*id)? {
            Object::Array(a) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
            _ => vec![*id],
        },
        Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
        _ => Vec::new(),
    };
    Ok(list)
}

pub(crate) fn set_contents(
    doc: &mut lopdf::Document,
    page: ObjectId,
    list: &[ObjectId],
) -> Result<()> {
    doc.get_dictionary_mut(page)?.set(
        "Contents",
        list.iter()
            .map(|&id| Object::Reference(id))
            .collect::<Vec<_>>(),
    );
    Ok(())
}

pub(crate) fn is_item(doc: &lopdf::Document, id: ObjectId) -> bool {
    matches!(doc.get_object(id), Ok(Object::Stream(s)) if s.dict.has(b"MkForm"))
}

fn is_wrap(doc: &lopdf::Document, id: ObjectId) -> bool {
    matches!(doc.get_object(id), Ok(Object::Stream(s)) if s.dict.has(b"MkWrap"))
}

/// Wraps the original content in q … Q (once) so items start from a clean state.
pub(crate) fn ensure_wrapped(doc: &mut lopdf::Document, page: ObjectId) -> Result<()> {
    let list = contents(doc, page)?;
    if list.iter().any(|&id| is_wrap(doc, id)) {
        return Ok(());
    }
    let open = doc.add_object(Stream::new(
        dictionary! { "MkWrap" => true },
        b"q\n".to_vec(),
    ));
    let close = doc.add_object(Stream::new(
        dictionary! { "MkWrap" => true },
        b"\nQ\n".to_vec(),
    ));
    let (items, original): (Vec<ObjectId>, Vec<ObjectId>) =
        list.into_iter().partition(|&id| is_item(doc, id));
    let mut new = vec![open];
    new.extend(original);
    new.push(close);
    new.extend(items);
    set_contents(doc, page, &new)
}

/// Makes the page's Resources (and its /XObject) direct, unshared dictionaries.
fn own_xobjects(doc: &mut lopdf::Document, page: ObjectId) -> Result<()> {
    let res = match doc.get_dictionary(page)?.get(b"Resources") {
        Ok(Object::Reference(id)) => doc.get_dictionary(*id)?.clone(),
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    let mut res = res;
    let xo = match res.get(b"XObject") {
        Ok(Object::Reference(id)) => doc.get_dictionary(*id)?.clone(),
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    res.set("XObject", xo);
    doc.get_dictionary_mut(page)?.set("Resources", res);
    Ok(())
}

pub(crate) fn page_xobjects(doc: &mut lopdf::Document, page: ObjectId) -> Result<&mut Dictionary> {
    own_xobjects(doc, page)?;
    let res = doc
        .get_dictionary_mut(page)?
        .get_mut(b"Resources")?
        .as_dict_mut()?;
    Ok(res.get_mut(b"XObject")?.as_dict_mut()?)
}

// ----- building -----------------------------------------------------------

fn color_obj(c: Color) -> Object {
    vec![c.r.into(), c.g.into(), c.b.into()].into()
}

fn read_color(o: Option<&Object>) -> Color {
    let v: Vec<f32> = o
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|x| x.as_float().ok()).collect())
        .unwrap_or_default();
    if v.len() == 3 {
        Color {
            r: v[0],
            g: v[1],
            b: v[2],
        }
    } else {
        Color::rgb(0, 0, 0)
    }
}

fn rect_obj(r: Rect) -> Object {
    vec![
        Object::Real(r.x0),
        Object::Real(r.y0),
        Object::Real(r.x1),
        Object::Real(r.y1),
    ]
    .into()
}

fn read_rect(o: Option<&Object>) -> Option<Rect> {
    let v: Vec<f32> = o?
        .as_array()
        .ok()?
        .iter()
        .filter_map(|x| x.as_float().ok())
        .collect();
    (v.len() == 4).then(|| Rect::new(v[0], v[1], v[2], v[3]))
}

/// Builds the Form XObject for `spec` (content in view points, mapped by `to_user`).
fn build_form(doc: &mut lopdf::Document, spec: &Spec, to_user: Affine) -> Stream {
    let rect = spec.rect();
    let mut resources = Dictionary::new();
    let mut meta = Dictionary::new();
    let content = match spec {
        Spec::Text {
            text,
            face,
            size,
            color,
            rect,
        } => {
            let mut p = TextPainter::default();
            p.text(
                text,
                *face,
                *size,
                rect.x0 + TEXT_PAD,
                rect.y0 + size * 0.95,
                [color.r, color.g, color.b],
            );
            let content = p.take_content();
            resources.set("Font", p.fonts(doc));
            meta.set("Kind", "Text");
            meta.set("Text", crate::annot::text_string(text));
            meta.set("Face", Object::Name(face.key().as_bytes().to_vec()));
            meta.set("Size", Object::Real(*size));
            meta.set("C", color_obj(*color));
            content
        }
        Spec::Image { image, rect } => {
            resources.set("XObject", dictionary! { "Im0" => *image });
            meta.set("Kind", "Image");
            meta.set("Image", *image);
            format!(
                "q {} 0 0 {} {} {} cm /Im0 Do Q\n",
                num(rect.width()),
                num(-rect.height()),
                num(rect.x0),
                num(rect.y1)
            )
        }
        Spec::Ink {
            strokes,
            color,
            width,
            ..
        } => {
            let mut s = format!(
                "{} {} {} RG {} w 1 J 1 j\n",
                num(color.r),
                num(color.g),
                num(color.b),
                num(*width)
            );
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
            meta.set("Kind", "Ink");
            meta.set(
                "Strokes",
                strokes
                    .iter()
                    .map(|st| {
                        Object::Array(
                            st.iter()
                                .flat_map(|p| [Object::Real(p.x), Object::Real(p.y)])
                                .collect(),
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            meta.set("C", color_obj(*color));
            meta.set("W", Object::Real(*width));
            s
        }
    };
    meta.set("Rect", rect_obj(rect));
    let bbox = rect.expand(4.0);
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => rect_obj(bbox),
            "Matrix" => to_user.as_coeffs().iter().map(|&c| Object::Real(c as f32)).collect::<Vec<_>>(),
            "Resources" => resources,
            "MkItem" => meta,
        },
        content.into_bytes(),
    );
    let _ = stream.compress();
    stream
}

fn read_spec(doc: &lopdf::Document, form: ObjectId) -> Option<Spec> {
    let Ok(Object::Stream(s)) = doc.get_object(form) else {
        return None;
    };
    let meta = s.dict.get(b"MkItem").ok()?.as_dict().ok()?;
    let rect = read_rect(meta.get(b"Rect").ok())?;
    let color = read_color(meta.get(b"C").ok());
    Some(match meta.get(b"Kind").ok()?.as_name().ok()? {
        b"Text" => Spec::Text {
            text: meta
                .get(b"Text")
                .map(crate::annot::decode_text)
                .unwrap_or_default(),
            face: FontFace::from_key(
                meta.get(b"Face")
                    .and_then(Object::as_name)
                    .unwrap_or(b"Sans"),
            ),
            size: meta.get(b"Size").and_then(Object::as_float).unwrap_or(12.0),
            color,
            rect,
        },
        b"Image" => Spec::Image {
            image: meta.get(b"Image").ok()?.as_reference().ok()?,
            rect,
        },
        b"Ink" => Spec::Ink {
            strokes: meta
                .get(b"Strokes")
                .ok()?
                .as_array()
                .ok()?
                .iter()
                .filter_map(|st| {
                    let v: Vec<f32> = st
                        .as_array()
                        .ok()?
                        .iter()
                        .filter_map(|x| x.as_float().ok())
                        .collect();
                    Some(v.chunks_exact(2).map(|c| Point::new(c[0], c[1])).collect())
                })
                .collect(),
            color,
            width: meta.get(b"W").and_then(Object::as_float).unwrap_or(2.0),
            rect,
        },
        _ => return None,
    })
}

/// Decodes PNG or JPEG into an image XObject (JPEG passes through untouched).
fn image_xobject(doc: &mut lopdf::Document, data: &[u8]) -> Result<(ObjectId, u32, u32)> {
    let format =
        image::guess_format(data).map_err(|_| Error::Invalid("Choose a PNG or JPEG picture."))?;
    let img = image::load_from_memory(data)
        .map_err(|_| Error::Invalid("This picture could not be read."))?;
    let (w, h) = (img.width(), img.height());
    if format == image::ImageFormat::Jpeg {
        let cs = if img.color().channel_count() == 1 {
            "DeviceGray"
        } else {
            "DeviceRGB"
        };
        let jpeg_cmyk = img.color().channel_count() == 4;
        if !jpeg_cmyk {
            let stream = Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
                    "ColorSpace" => cs, "BitsPerComponent" => 8, "Filter" => "DCTDecode",
                },
                data.to_vec(),
            )
            .with_compression(false);
            return Ok((doc.add_object(stream), w, h));
        }
    }
    let rgba = img.to_rgba8();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for px in rgba.pixels() {
        rgb.extend_from_slice(&px.0[..3]);
        alpha.push(px.0[3]);
    }
    let mut dict = dictionary! {
        "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
        "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
    };
    if alpha.iter().any(|&a| a != 255) {
        let mut mask = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
            },
            alpha,
        );
        let _ = mask.compress();
        dict.set("SMask", doc.add_object(mask));
    }
    let mut stream = Stream::new(dict, rgb);
    let _ = stream.compress();
    Ok((doc.add_object(stream), w, h))
}

// ----- operations -----------------------------------------------------------

pub(crate) fn add(
    doc: &mut lopdf::Document,
    page: ObjectId,
    to_user: Affine,
    item: &NewItem,
) -> Result<u32> {
    let spec = match item {
        NewItem::Text {
            at,
            text,
            face,
            size,
            color,
        } => {
            if text.trim().is_empty() {
                return Err(Error::Invalid("Type some text first."));
            }
            Spec::Text {
                text: text.clone(),
                face: *face,
                size: *size,
                color: *color,
                rect: text_rect(*at, text, *face, *size),
            }
        }
        NewItem::Image { rect, data } => {
            let (image, _, _) = image_xobject(doc, data)?;
            Spec::Image {
                image,
                rect: rect.normalized(),
            }
        }
        NewItem::Ink {
            strokes,
            color,
            width,
        } => {
            if strokes.iter().all(|s| s.is_empty()) {
                return Err(Error::Invalid("Nothing drawn."));
            }
            Spec::Ink {
                strokes: strokes.clone(),
                color: *color,
                width: *width,
                rect: ink_bounds(strokes, *width),
            }
        }
    };
    ensure_wrapped(doc, page)?;
    let form = build_form(doc, &spec, to_user);
    let form = doc.add_object(form);
    let item = doc.add_object(Stream::new(dictionary! { "MkForm" => form }, Vec::new()));
    let name = format!("MkX{}", item.0);
    if let Ok(Object::Stream(s)) = doc.get_object_mut(item) {
        s.set_plain_content(format!("q /{name} Do Q\n").into_bytes());
    }
    page_xobjects(doc, page)?.set(name, form);
    let mut list = contents(doc, page)?;
    list.push(item);
    set_contents(doc, page, &list)?;
    Ok(item.0)
}

fn find(doc: &mut lopdf::Document, page: ObjectId, id: u32) -> Result<(ObjectId, ObjectId)> {
    let item = contents(doc, page)?
        .into_iter()
        .find(|o| o.0 == id && is_item(doc, *o))
        .ok_or(Error::Invalid("That object is no longer on the page."))?;
    let Ok(Object::Stream(s)) = doc.get_object(item) else {
        unreachable!()
    };
    let form = s.dict.get(b"MkForm")?.as_reference()?;
    Ok((item, form))
}

pub(crate) fn list(doc: &lopdf::Document, page: ObjectId) -> Vec<PageItem> {
    let Ok(ids) = contents(doc, page) else {
        return Vec::new();
    };
    ids.into_iter()
        .filter(|id| is_item(doc, *id))
        .filter_map(|id| {
            let Ok(Object::Stream(s)) = doc.get_object(id) else {
                return None;
            };
            let form = s.dict.get(b"MkForm").ok()?.as_reference().ok()?;
            let spec = read_spec(doc, form)?;
            let rect = spec.rect();
            let kind = match spec {
                Spec::Text {
                    text,
                    face,
                    size,
                    color,
                    ..
                } => ItemKind::Text {
                    text,
                    face,
                    size,
                    color,
                },
                Spec::Image { .. } => ItemKind::Image,
                Spec::Ink { color, width, .. } => ItemKind::Ink { color, width },
            };
            Some(PageItem {
                id: id.0,
                rect,
                kind,
            })
        })
        .collect()
}

pub(crate) fn update(
    doc: &mut lopdf::Document,
    page: ObjectId,
    to_user: Affine,
    id: u32,
    edit: &ItemEdit,
) -> Result<()> {
    let (_, form) = find(doc, page, id)?;
    let spec = read_spec(doc, form).ok_or(Error::Invalid("This object can't be edited."))?;
    let new_rect = edit.rect.map(Rect::normalized);
    let spec = match spec {
        Spec::Text {
            text,
            face,
            size,
            color,
            rect,
        } => {
            let mut size = edit.size.unwrap_or(size);
            // Resizing the box scales the type.
            if let Some(r) = new_rect
                && edit.size.is_none()
                && (r.height() - rect.height()).abs() > 0.5
            {
                size = (size * r.height() / rect.height().max(1.0)).clamp(2.0, 400.0);
            }
            let text = edit.text.clone().unwrap_or(text);
            if text.trim().is_empty() {
                return delete(doc, page, id);
            }
            let face = edit.face.unwrap_or(face);
            let at = new_rect.map_or(Point::new(rect.x0, rect.y0), |r| Point::new(r.x0, r.y0));
            Spec::Text {
                rect: text_rect(at, &text, face, size),
                text,
                face,
                size,
                color: edit.color.unwrap_or(color),
            }
        }
        Spec::Image { image, rect } => Spec::Image {
            image,
            rect: new_rect.unwrap_or(rect),
        },
        Spec::Ink {
            strokes,
            color,
            width,
            rect,
        } => {
            let to = new_rect.unwrap_or(rect);
            let strokes = strokes
                .iter()
                .map(|s| s.iter().map(|p| remap(*p, rect, to)).collect())
                .collect();
            Spec::Ink {
                strokes,
                color: edit.color.unwrap_or(color),
                width: edit.width.unwrap_or(width),
                rect: to,
            }
        }
    };
    let stream = build_form(doc, &spec, to_user);
    doc.objects.insert(form, Object::Stream(stream));
    doc.prune_objects();
    Ok(())
}

pub(crate) fn delete(doc: &mut lopdf::Document, page: ObjectId, id: u32) -> Result<()> {
    let (item, _) = find(doc, page, id)?;
    let mut list = contents(doc, page)?;
    list.retain(|o| *o != item);
    set_contents(doc, page, &list)?;
    if let Ok(xo) = page_xobjects(doc, page) {
        xo.remove(format!("MkX{id}").as_bytes());
    }
    doc.prune_objects();
    Ok(())
}
