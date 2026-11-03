//! AcroForm fields: reading, filling (with generated appearances), creating, editing and
//! deleting text, check box, radio, dropdown, list and signature fields.
//!
//! Appearances are generated for every value so viewers that don't build their own show
//! the filled form; text is set with the bundled fonts, so any script works (including
//! Arabic, right aligned). `NeedAppearances` is left off because the appearances are complete.

use std::collections::HashMap;

use hayro::kurbo::{Affine, Point as KPoint};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::annot::{decode_text, text_string};
use crate::document::{Error, Result};
use crate::fonts::{self, FontFace, TextPainter, num};
use crate::geometry::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    CheckBox,
    Radio,
    ComboBox,
    ListBox,
    Signature,
    PushButton,
}

impl FieldKind {
    pub fn label(self) -> &'static str {
        match self {
            FieldKind::Text => "Text field",
            FieldKind::CheckBox => "Check box",
            FieldKind::Radio => "Radio button",
            FieldKind::ComboBox => "Dropdown",
            FieldKind::ListBox => "List box",
            FieldKind::Signature => "Signature field",
            FieldKind::PushButton => "Button",
        }
    }
}

/// One widget (the visible box) of a field.
#[derive(Clone, Debug, PartialEq)]
pub struct Widget {
    pub page: usize,
    /// Position in the page's /Annots.
    pub slot: usize,
    /// Bounds in view points.
    pub rect: Rect,
    /// The "on" state of a check box or radio button (its export value).
    pub on_state: Option<String>,
}

/// A form field as the UI sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldInfo {
    /// Fully qualified name ("address.city").
    pub name: String,
    pub kind: FieldKind,
    /// Text or selected choice; for check boxes and radios the selected export value or "Off".
    pub value: String,
    pub default: String,
    /// Choices (dropdown/list) or the radio group's export values.
    pub options: Vec<String>,
    pub multiline: bool,
    pub read_only: bool,
    pub widgets: Vec<Widget>,
}

impl FieldInfo {
    pub fn is_on(&self, widget: &Widget) -> bool {
        widget.on_state.as_deref().is_some_and(|s| s == self.value)
    }
}

/// A field to create (Prepare form), in view points.
#[derive(Clone, Debug, PartialEq)]
pub struct NewField {
    pub kind: FieldKind,
    pub name: String,
    pub rect: Rect,
    /// Choices for dropdowns and lists; for a radio button, its export value (first item).
    pub options: Vec<String>,
}

const FF_READ_ONLY: i64 = 1;
const FF_MULTILINE: i64 = 1 << 12;
const FF_RADIO: i64 = 1 << 15;
const FF_PUSH: i64 = 1 << 16;
const FF_COMBO: i64 = 1 << 17;

fn deref<'a>(doc: &'a lopdf::Document, o: &'a Object) -> &'a Object {
    crate::textedit::deref(doc, o)
}

fn acroform_id(doc: &mut lopdf::Document, create: bool) -> Option<ObjectId> {
    let cat = doc.catalog().ok()?;
    match cat.get(b"AcroForm") {
        Ok(Object::Reference(id)) => Some(*id),
        Ok(Object::Dictionary(d)) => {
            let d = d.clone();
            let id = doc.add_object(d);
            doc.catalog_mut().ok()?.set("AcroForm", id);
            Some(id)
        }
        _ if create => {
            let id = doc.add_object(dictionary! {
                "Fields" => Vec::<Object>::new(),
                "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
                "DR" => dictionary! { "Font" => dictionary! { "Helv" => dictionary! {
                    "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
                } } },
            });
            doc.catalog_mut().ok()?.set("AcroForm", id);
            Some(id)
        }
        _ => None,
    }
}

fn fields_array(doc: &lopdf::Document, form: ObjectId) -> Vec<ObjectId> {
    doc.get_dictionary(form)
        .ok()
        .and_then(|f| f.get(b"Fields").ok())
        .map(|f| deref(doc, f))
        .and_then(|f| f.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default()
}

/// Inherited attribute lookup up the field tree.
fn inherited<'a>(doc: &'a lopdf::Document, mut id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    for _ in 0..32 {
        let d = doc.get_dictionary(id).ok()?;
        if let Ok(v) = d.get(key) {
            return Some(deref(doc, v));
        }
        id = d.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn kids(doc: &lopdf::Document, id: ObjectId) -> Vec<ObjectId> {
    doc.get_dictionary(id)
        .ok()
        .and_then(|d| d.get(b"Kids").ok())
        .map(|k| deref(doc, k))
        .and_then(|k| k.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default()
}

fn is_widget(doc: &lopdf::Document, id: ObjectId) -> bool {
    doc.get_dictionary(id)
        .ok()
        .and_then(|d| d.get(b"Subtype").ok())
        .and_then(|s| s.as_name().ok())
        == Some(b"Widget")
}

/// Terminal fields with their widget ids.
fn terminals(doc: &lopdf::Document, form: ObjectId) -> Vec<(ObjectId, String, Vec<ObjectId>)> {
    let mut out = Vec::new();
    let mut stack: Vec<(ObjectId, String)> = fields_array(doc, form)
        .into_iter()
        .rev()
        .map(|id| (id, String::new()))
        .collect();
    let mut guard = 0;
    while let Some((id, prefix)) = stack.pop() {
        guard += 1;
        if guard > 100_000 {
            break;
        }
        let Ok(d) = doc.get_dictionary(id) else {
            continue;
        };
        let part = d.get(b"T").map(decode_text).unwrap_or_default();
        let name = match (prefix.is_empty(), part.is_empty()) {
            (true, _) => part,
            (false, true) => prefix,
            (false, false) => format!("{prefix}.{part}"),
        };
        let ks = kids(doc, id);
        let field_kids: Vec<ObjectId> = ks
            .iter()
            .copied()
            .filter(|k| doc.get_dictionary(*k).is_ok_and(|d| d.has(b"T")))
            .collect();
        if !field_kids.is_empty() {
            for k in field_kids.into_iter().rev() {
                stack.push((k, name.clone()));
            }
            continue;
        }
        let widgets = if ks.is_empty() {
            if is_widget(doc, id) {
                vec![id]
            } else {
                Vec::new()
            }
        } else {
            ks
        };
        out.push((id, name, widgets));
    }
    out
}

fn kind_of(doc: &lopdf::Document, id: ObjectId) -> FieldKind {
    let ft = inherited(doc, id, b"FT")
        .and_then(|o| o.as_name().ok())
        .unwrap_or(b"Tx");
    let ff = inherited(doc, id, b"Ff")
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);
    match ft {
        b"Btn" if ff & FF_PUSH != 0 => FieldKind::PushButton,
        b"Btn" if ff & FF_RADIO != 0 => FieldKind::Radio,
        b"Btn" => FieldKind::CheckBox,
        b"Ch" if ff & FF_COMBO != 0 => FieldKind::ComboBox,
        b"Ch" => FieldKind::ListBox,
        b"Sig" => FieldKind::Signature,
        _ => FieldKind::Text,
    }
}

fn value_string(o: Option<&Object>) -> String {
    match o {
        Some(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
        Some(Object::String(..)) => decode_text(o.unwrap()),
        Some(Object::Array(a)) => a.first().map(|x| value_string(Some(x))).unwrap_or_default(),
        _ => String::new(),
    }
}

fn on_state(doc: &lopdf::Document, widget: ObjectId) -> Option<String> {
    let ap = doc
        .get_dictionary(widget)
        .ok()?
        .get(b"AP")
        .ok()
        .map(|a| deref(doc, a))?
        .as_dict()
        .ok()?;
    let n = deref(doc, ap.get(b"N").ok()?).as_dict().ok()?;
    n.iter()
        .map(|(k, _)| k)
        .find(|k| k.as_slice() != b"Off")
        .map(|k| String::from_utf8_lossy(k).into_owned())
}

fn options(doc: &lopdf::Document, id: ObjectId) -> Vec<String> {
    let Some(Object::Array(a)) = inherited(doc, id, b"Opt") else {
        return Vec::new();
    };
    a.iter()
        .map(|o| match deref(doc, o) {
            Object::Array(pair) => pair
                .get(1)
                .or(pair.first())
                .map(|x| decode_text(deref(doc, x)))
                .unwrap_or_default(),
            other => decode_text(other),
        })
        .collect()
}

fn user_rect(doc: &lopdf::Document, id: ObjectId) -> Option<Rect> {
    let v: Vec<f32> = doc
        .get_dictionary(id)
        .ok()?
        .get(b"Rect")
        .ok()?
        .as_array()
        .ok()?
        .iter()
        .filter_map(|x| x.as_float().ok())
        .collect();
    (v.len() == 4).then(|| Rect::new(v[0], v[1], v[2], v[3]).normalized())
}

fn bbox(m: Affine, r: Rect) -> Rect {
    let pts = [(r.x0, r.y0), (r.x1, r.y1), (r.x0, r.y1), (r.x1, r.y0)]
        .map(|(x, y)| m * KPoint::new(f64::from(x), f64::from(y)));
    Rect::new(
        pts.iter().map(|p| p.x as f32).fold(f32::MAX, f32::min),
        pts.iter().map(|p| p.y as f32).fold(f32::MAX, f32::min),
        pts.iter().map(|p| p.x as f32).fold(f32::MIN, f32::max),
        pts.iter().map(|p| p.y as f32).fold(f32::MIN, f32::max),
    )
}

/// Where each widget sits: widget id -> (page index, slot).
fn widget_places(doc: &lopdf::Document, pages: &[ObjectId]) -> HashMap<ObjectId, (usize, usize)> {
    let mut map = HashMap::new();
    for (pi, &page) in pages.iter().enumerate() {
        let annots = match doc.get_dictionary(page).and_then(|p| p.get(b"Annots")) {
            Ok(Object::Reference(id)) => doc
                .get_object(*id)
                .and_then(Object::as_array)
                .cloned()
                .unwrap_or_default(),
            Ok(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        for (slot, a) in annots.iter().enumerate() {
            if let Ok(id) = a.as_reference() {
                map.insert(id, (pi, slot));
            }
        }
    }
    map
}

/// The AcroForm object (documents are normalized on open so it is always indirect).
fn form_id(doc: &lopdf::Document) -> Option<ObjectId> {
    doc.catalog()
        .ok()?
        .get(b"AcroForm")
        .ok()?
        .as_reference()
        .ok()
}

/// Makes a direct /AcroForm dictionary indirect (done once when a document is opened).
pub(crate) fn normalize(doc: &mut lopdf::Document) {
    let _ = acroform_id(doc, false);
}

pub(crate) fn list(
    doc: &lopdf::Document,
    pages: &[ObjectId],
    to_view: &dyn Fn(usize) -> Affine,
) -> Vec<FieldInfo> {
    let Some(form) = form_id(doc) else {
        return Vec::new();
    };
    let places = widget_places(doc, pages);
    let mut out = Vec::new();
    for (id, name, widgets) in terminals(doc, form) {
        let kind = kind_of(doc, id);
        let ff = inherited(doc, id, b"Ff")
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        let ws: Vec<Widget> = widgets
            .iter()
            .filter_map(|w| {
                let (page, slot) = *places.get(w)?;
                Some(Widget {
                    page,
                    slot,
                    rect: bbox(to_view(page), user_rect(doc, *w)?),
                    on_state: on_state(doc, *w),
                })
            })
            .collect();
        let mut opts = options(doc, id);
        if kind == FieldKind::Radio {
            opts = ws.iter().filter_map(|w| w.on_state.clone()).collect();
        }
        let mut value = value_string(inherited(doc, id, b"V"));
        if matches!(kind, FieldKind::CheckBox | FieldKind::Radio) && value.is_empty() {
            value = "Off".into();
        }
        out.push(FieldInfo {
            name,
            kind,
            value,
            default: value_string(inherited(doc, id, b"DV")),
            options: opts,
            multiline: ff & FF_MULTILINE != 0,
            read_only: ff & FF_READ_ONLY != 0,
            widgets: ws,
        });
    }
    out
}

fn find_field(doc: &mut lopdf::Document, name: &str) -> Result<(ObjectId, Vec<ObjectId>)> {
    let form = acroform_id(doc, false).ok_or(Error::Invalid("This PDF has no form."))?;
    terminals(doc, form)
        .into_iter()
        .find(|(_, n, _)| n == name)
        .map(|(id, _, w)| (id, w))
        .ok_or(Error::Invalid("That form field no longer exists."))
}

fn da_size(doc: &lopdf::Document, id: ObjectId) -> f32 {
    let da = inherited(doc, id, b"DA")
        .and_then(|o| o.as_str().ok())
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let t: Vec<&str> = da.split_whitespace().collect();
    t.iter()
        .position(|x| *x == "Tf")
        .and_then(|i| t.get(i.checked_sub(1)?)?.parse::<f32>().ok())
        .unwrap_or(0.0)
}

const BORDER: &str = "0.55 0.55 0.6 RG 0.75 w";

/// Appearance stream for a widget of `w` × `h` points.
#[allow(clippy::too_many_arguments)]
fn appearance(
    doc: &mut lopdf::Document,
    kind: FieldKind,
    w: f32,
    h: f32,
    value: &str,
    options: &[String],
    size: f32,
    multiline: bool,
    on: bool,
) -> Stream {
    let mut res = Dictionary::new();
    let mut s = String::new();
    let border = format!("{BORDER} 0.5 0.5 {} {} re S\n", num(w - 1.0), num(h - 1.0));
    match kind {
        FieldKind::Text | FieldKind::ComboBox | FieldKind::ListBox => {
            s += "1 1 1 rg 0 0 ";
            s += &format!(
                "{} {} re f\n{border}/Tx BMC\nq 1 1 {} {} re W n\n",
                num(w),
                num(h),
                num(w - 2.0),
                num(h - 2.0)
            );
            let size = if size > 0.0 {
                size
            } else if multiline || kind == FieldKind::ListBox {
                11.0
            } else {
                (h * 0.62).clamp(6.0, 14.0)
            };
            let mut p = TextPainter::default();
            let mut highlight = String::new();
            if kind == FieldKind::ListBox {
                let lead = fonts::leading(size);
                let mut text = String::new();
                for (i, o) in options.iter().enumerate() {
                    if o == value {
                        let top = 2.0 + i as f32 * lead;
                        highlight = format!(
                            "0.6 0.75 0.95 rg 1 {} {} {} re f\n",
                            num(h - top - lead),
                            num(w - 2.0),
                            num(lead)
                        );
                    }
                    text += o;
                    text.push('\n');
                }
                p.text_in_box(
                    text.trim_end(),
                    FontFace::Sans,
                    size,
                    3.0,
                    w - 3.0,
                    2.0 + size * 0.9,
                    [0.0, 0.0, 0.0],
                );
            } else {
                let y = if multiline {
                    2.0 + size * 0.95
                } else {
                    h / 2.0 + size * 0.35
                };
                let right = if kind == FieldKind::ComboBox {
                    w - 14.0
                } else {
                    w - 3.0
                };
                p.text_in_box(value, FontFace::Sans, size, 3.0, right, y, [0.0, 0.0, 0.0]);
            }
            s += &highlight;
            // The painter works in a y-down space.
            s += &format!("q 1 0 0 -1 0 {} cm\n{}Q\n", num(h), p.take_content());
            s += "Q\nEMC\n";
            if kind == FieldKind::ComboBox {
                let (cx, cy) = (w - 7.5, h / 2.0);
                s += &format!(
                    "0.35 g {} {} m {} {} l {} {} l f\n",
                    num(cx - 3.0),
                    num(cy + 1.5),
                    num(cx + 3.0),
                    num(cy + 1.5),
                    num(cx),
                    num(cy - 2.0)
                );
            }
            res.set("Font", p.fonts(doc));
        }
        FieldKind::CheckBox => {
            s += &format!("1 1 1 rg 0 0 {} {} re f\n{border}", num(w), num(h));
            if on {
                let (u, v) = (w / 20.0, h / 20.0);
                s += &format!(
                    "0 g 1.6 w 1 J 1 j {} {} m {} {} l {} {} l S\n",
                    num(4.0 * u),
                    num(10.0 * v),
                    num(8.5 * u),
                    num(5.0 * v),
                    num(16.0 * u),
                    num(15.5 * v)
                );
            }
        }
        FieldKind::Radio => {
            let r = w.min(h) / 2.0 - 0.75;
            let (cx, cy) = (w / 2.0, h / 2.0);
            s += &format!("1 1 1 rg {BORDER} {} B\n", circle(cx, cy, r));
            if on {
                s += &format!("0 g {} f\n", circle(cx, cy, r * 0.45));
            }
        }
        FieldKind::Signature => {
            s += &format!(
                "0.97 0.97 1 rg 0 0 {} {} re f\n0.4 0.45 0.7 RG 0.75 w [3 2] 0 d 0.5 0.5 {} {} re S\n",
                num(w),
                num(h),
                num(w - 1.0),
                num(h - 1.0)
            );
        }
        FieldKind::PushButton => {
            s += &format!("0.9 g 0 0 {} {} re f\n{border}", num(w), num(h));
        }
    }
    Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)], "Resources" => res },
        s.into_bytes(),
    )
}

fn circle(cx: f32, cy: f32, r: f32) -> String {
    const K: f32 = 0.552_284_8;
    let o = r * K;
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h",
        num(cx + r),
        num(cy),
        num(cx + r),
        num(cy + o),
        num(cx + o),
        num(cy + r),
        num(cx),
        num(cy + r),
        num(cx - o),
        num(cy + r),
        num(cx - r),
        num(cy + o),
        num(cx - r),
        num(cy),
        num(cx - r),
        num(cy - o),
        num(cx - o),
        num(cy - r),
        num(cx),
        num(cy - r),
        num(cx + o),
        num(cy - r),
        num(cx + r),
        num(cy - o),
        num(cx + r),
        num(cy)
    )
}

/// Rebuilds the appearances of every widget of a field from its current value.
fn refresh(doc: &mut lopdf::Document, field: ObjectId, widgets: &[ObjectId]) {
    let kind = kind_of(doc, field);
    let value = value_string(inherited(doc, field, b"V"));
    let opts = options(doc, field);
    let size = da_size(doc, field);
    let ff = inherited(doc, field, b"Ff")
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);
    for &w in widgets {
        let Some(r) = user_rect(doc, w) else { continue };
        let (width, height) = (r.width().max(1.0), r.height().max(1.0));
        match kind {
            FieldKind::CheckBox | FieldKind::Radio => {
                let on = on_state(doc, w).unwrap_or_else(|| "Yes".into());
                let on_ap = appearance(doc, kind, width, height, "", &[], 0.0, false, true);
                let off_ap = appearance(doc, kind, width, height, "", &[], 0.0, false, false);
                let on_id = doc.add_object(on_ap);
                let off_id = doc.add_object(off_ap);
                let mut n = Dictionary::new();
                n.set(on.clone(), on_id);
                n.set("Off", off_id);
                let selected = if value == on { on } else { "Off".into() };
                if let Ok(d) = doc.get_dictionary_mut(w) {
                    d.set("AP", dictionary! { "N" => n });
                    d.set("AS", Object::Name(selected.into_bytes()));
                }
            }
            _ => {
                let ap = appearance(
                    doc,
                    kind,
                    width,
                    height,
                    &value,
                    &opts,
                    size,
                    ff & FF_MULTILINE != 0,
                    false,
                );
                let id = doc.add_object(ap);
                if let Ok(d) = doc.get_dictionary_mut(w) {
                    d.set("AP", dictionary! { "N" => id });
                }
            }
        }
    }
}

pub(crate) fn set_value(doc: &mut lopdf::Document, name: &str, value: &str) -> Result<()> {
    let (id, widgets) = find_field(doc, name)?;
    let kind = kind_of(doc, id);
    let v = match kind {
        FieldKind::CheckBox | FieldKind::Radio => Object::Name(if value.is_empty() {
            b"Off".to_vec()
        } else {
            value.as_bytes().to_vec()
        }),
        _ => text_string(value),
    };
    doc.get_dictionary_mut(id)?.set("V", v);
    refresh(doc, id, &widgets);
    if let Some(form) = acroform_id(doc, false)
        && let Ok(f) = doc.get_dictionary_mut(form)
    {
        f.set("NeedAppearances", false);
    }
    doc.prune_objects();
    Ok(())
}

pub(crate) fn add(
    doc: &mut lopdf::Document,
    page: ObjectId,
    to_user: Affine,
    field: &NewField,
) -> Result<String> {
    let form = acroform_id(doc, true).ok_or(Error::Invalid("The form could not be created."))?;
    let name = field.name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("Give the field a name."));
    }
    let r = bbox(to_user, field.rect.normalized());
    let rect: Vec<Object> = vec![
        Object::Real(r.x0),
        Object::Real(r.y0),
        Object::Real(r.x1),
        Object::Real(r.y1),
    ];
    let mut widget = dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "Rect" => rect, "P" => page, "F" => 4,
        "MK" => dictionary! { "BC" => vec![Object::Real(0.55), Object::Real(0.55), Object::Real(0.6)], "BG" => vec![Object::Real(1.0), Object::Real(1.0), Object::Real(1.0)] },
    };
    let existing = terminals(doc, form).into_iter().find(|(_, n, _)| n == name);
    let field_id;
    let widget_id;
    if field.kind == FieldKind::Radio {
        let export = field
            .options
            .first()
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Choice1".into());
        let export = export.replace(' ', "_");
        // Placeholder on-state so `refresh` knows the export name.
        widget.set("AP", dictionary! { "N" => dictionary! { export.clone() => Object::Null, "Off" => Object::Null } });
        match existing {
            Some((id, _, _)) if kind_of(doc, id) == FieldKind::Radio => {
                widget.set("Parent", id);
                widget_id = doc.add_object(widget);
                let mut k = kids(doc, id);
                k.push(widget_id);
                doc.get_dictionary_mut(id)?.set(
                    "Kids",
                    k.into_iter().map(Object::Reference).collect::<Vec<_>>(),
                );
                field_id = id;
            }
            Some(_) => return Err(Error::Invalid("A field with that name already exists.")),
            None => {
                let parent = doc.add_object(dictionary! {
                    "FT" => "Btn", "Ff" => FF_RADIO | (1 << 14), "T" => text_string(name), "V" => "Off",
                });
                widget.set("Parent", parent);
                widget_id = doc.add_object(widget);
                doc.get_dictionary_mut(parent)?
                    .set("Kids", vec![Object::Reference(widget_id)]);
                let mut f = fields_array(doc, form);
                f.push(parent);
                doc.get_dictionary_mut(form)?.set(
                    "Fields",
                    f.into_iter().map(Object::Reference).collect::<Vec<_>>(),
                );
                field_id = parent;
            }
        }
    } else {
        if existing.is_some() {
            return Err(Error::Invalid("A field with that name already exists."));
        }
        widget.set("T", text_string(name));
        match field.kind {
            FieldKind::Text => {
                widget.set("FT", "Tx");
                widget.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
                widget.set("V", text_string(""));
            }
            FieldKind::CheckBox => {
                widget.set("FT", "Btn");
                widget.set("V", "Off");
                widget.set("AP", dictionary! { "N" => dictionary! { "Yes" => Object::Null, "Off" => Object::Null } });
            }
            FieldKind::ComboBox | FieldKind::ListBox => {
                widget.set("FT", "Ch");
                if field.kind == FieldKind::ComboBox {
                    widget.set("Ff", FF_COMBO);
                }
                widget.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
                widget.set(
                    "Opt",
                    field
                        .options
                        .iter()
                        .map(|o| text_string(o))
                        .collect::<Vec<_>>(),
                );
                widget.set(
                    "V",
                    text_string(field.options.first().map_or("", String::as_str)),
                );
            }
            FieldKind::Signature => widget.set("FT", "Sig"),
            FieldKind::PushButton => {
                widget.set("FT", "Btn");
                widget.set("Ff", FF_PUSH);
            }
            FieldKind::Radio => unreachable!(),
        }
        widget_id = doc.add_object(widget);
        let mut f = fields_array(doc, form);
        f.push(widget_id);
        doc.get_dictionary_mut(form)?.set(
            "Fields",
            f.into_iter().map(Object::Reference).collect::<Vec<_>>(),
        );
        field_id = widget_id;
    }
    let annots = match doc.get_dictionary(page)?.get(b"Annots") {
        Ok(Object::Reference(id)) => doc.get_object(*id)?.as_array()?.clone(),
        Ok(Object::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    let mut annots = annots;
    annots.push(Object::Reference(widget_id));
    doc.get_dictionary_mut(page)?.set("Annots", annots);
    let widgets = if field_id == widget_id {
        vec![widget_id]
    } else {
        kids(doc, field_id)
    };
    refresh(doc, field_id, &widgets);
    Ok(name.to_string())
}

/// Renames a field and sets its default value and options.
pub(crate) fn set_properties(
    doc: &mut lopdf::Document,
    name: &str,
    new_name: &str,
    default: &str,
    options: &[String],
) -> Result<()> {
    let (id, widgets) = find_field(doc, name)?;
    if new_name.trim().is_empty() {
        return Err(Error::Invalid("Give the field a name."));
    }
    if new_name != name {
        let form = acroform_id(doc, false).ok_or(Error::Invalid("This PDF has no form."))?;
        if terminals(doc, form).iter().any(|(_, n, _)| n == new_name) {
            return Err(Error::Invalid("A field with that name already exists."));
        }
    }
    let kind = kind_of(doc, id);
    let d = doc.get_dictionary_mut(id)?;
    // Only the last part of a dotted name belongs to this field.
    let part = new_name.rsplit('.').next().unwrap_or(new_name);
    d.set("T", text_string(part));
    match kind {
        FieldKind::CheckBox | FieldKind::Radio => {
            if !default.is_empty() {
                d.set("DV", Object::Name(default.as_bytes().to_vec()));
            }
        }
        _ => d.set("DV", text_string(default)),
    }
    if matches!(kind, FieldKind::ComboBox | FieldKind::ListBox) {
        d.set(
            "Opt",
            options.iter().map(|o| text_string(o)).collect::<Vec<_>>(),
        );
    }
    refresh(doc, id, &widgets);
    doc.prune_objects();
    Ok(())
}

/// Moves or resizes one widget.
pub(crate) fn set_widget_rect(
    doc: &mut lopdf::Document,
    page: ObjectId,
    slot: usize,
    to_user: Affine,
    rect: Rect,
) -> Result<()> {
    let widget = widget_at(doc, page, slot)?;
    let r = bbox(to_user, rect.normalized());
    doc.get_dictionary_mut(widget)?.set(
        "Rect",
        vec![
            Object::Real(r.x0),
            Object::Real(r.y0),
            Object::Real(r.x1),
            Object::Real(r.y1),
        ],
    );
    let field = field_of(doc, widget);
    let widgets = if field == widget {
        vec![widget]
    } else {
        kids(doc, field)
    };
    refresh(doc, field, &widgets);
    doc.prune_objects();
    Ok(())
}

fn widget_at(doc: &lopdf::Document, page: ObjectId, slot: usize) -> Result<ObjectId> {
    let annots = match doc.get_dictionary(page)?.get(b"Annots") {
        Ok(Object::Reference(id)) => doc.get_object(*id)?.as_array()?.clone(),
        Ok(Object::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    annots
        .get(slot)
        .and_then(|o| o.as_reference().ok())
        .filter(|id| is_widget(doc, *id))
        .ok_or(Error::Invalid("That form field no longer exists."))
}

/// The field a widget belongs to (itself when merged).
fn field_of(doc: &lopdf::Document, widget: ObjectId) -> ObjectId {
    let d = doc.get_dictionary(widget).ok();
    if d.is_some_and(|d| d.has(b"T")) {
        return widget;
    }
    d.and_then(|d| d.get(b"Parent").ok())
        .and_then(|p| p.as_reference().ok())
        .unwrap_or(widget)
}

/// Deletes the field a widget belongs to (all its widgets).
pub(crate) fn delete_field(
    doc: &mut lopdf::Document,
    pages: &[ObjectId],
    page: ObjectId,
    slot: usize,
) -> Result<()> {
    let widget = widget_at(doc, page, slot)?;
    let field = field_of(doc, widget);
    let mut widgets = if field == widget {
        vec![widget]
    } else {
        kids(doc, field)
    };
    widgets.push(widget);
    for &p in pages {
        let annots = match doc.get_dictionary(p).and_then(|d| d.get(b"Annots")) {
            Ok(Object::Reference(id)) => doc
                .get_object(*id)
                .and_then(Object::as_array)
                .cloned()
                .unwrap_or_default(),
            Ok(Object::Array(a)) => a.clone(),
            _ => continue,
        };
        let kept: Vec<Object> = annots
            .into_iter()
            .filter(|a| a.as_reference().map_or(true, |id| !widgets.contains(&id)))
            .collect();
        doc.get_dictionary_mut(p)?.set("Annots", kept);
    }
    forget_widgets(doc, &widgets);
    Ok(())
}

/// Removes widgets (deleted with their annotations) from the field tree; fields left with
/// no widgets are removed too.
pub(crate) fn forget_widgets(doc: &mut lopdf::Document, gone: &[ObjectId]) {
    let Some(form) = acroform_id(doc, false) else {
        return;
    };
    fn prune(
        doc: &mut lopdf::Document,
        list: Vec<ObjectId>,
        gone: &[ObjectId],
        depth: usize,
    ) -> Vec<ObjectId> {
        let mut out = Vec::new();
        for id in list {
            if gone.contains(&id) {
                continue;
            }
            let ks = kids(doc, id);
            if depth < 32 && !ks.is_empty() {
                let left = prune(doc, ks, gone, depth + 1);
                if left.is_empty() {
                    continue;
                }
                if let Ok(d) = doc.get_dictionary_mut(id) {
                    d.set(
                        "Kids",
                        left.into_iter().map(Object::Reference).collect::<Vec<_>>(),
                    );
                }
            }
            out.push(id);
        }
        out
    }
    let fields = fields_array(doc, form);
    let left = prune(doc, fields, gone, 0);
    if let Ok(f) = doc.get_dictionary_mut(form) {
        f.set(
            "Fields",
            left.into_iter().map(Object::Reference).collect::<Vec<_>>(),
        );
    }
}
