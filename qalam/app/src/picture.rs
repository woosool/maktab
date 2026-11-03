//! Picture and object editing: Picture Format commands, rotate/flip for any selected picture or
//! shape, the crop and alt-text dialogs, and Save as Picture.

use crate::app::WordApp;
use egui::{vec2, Color32, DragValue, Pos2, Rect, Sense, Stroke, TextureHandle};
use word_engine::model::{Anchor, AnchorAlign, Image, RelFrom, Wrap};
use word_engine::objects::Object;
use word_engine::picture::PictureOp;

pub struct CropDraft { pub pos: usize, pub left: f32, pub top: f32, pub right: f32, pub bottom: f32, pub texture: Option<TextureHandle> }

/// The Layout dialog's working copy (Position, Text Wrapping and Size tabs in one).
pub struct LayoutDraft { pub pos: usize, pub wrap: Option<Wrap>, pub anchor: Anchor, pub w: f32, pub h: f32, pub lock: bool, pub rotation: f32 }

/// Wrap styles with their icons, in the order Word lists them; None is in line with text.
pub const WRAPS: [(Option<Wrap>, &str, &str); 6] = [
    (None, "wrapInline", "In Line with Text"), (Some(Wrap::Square), "wrapSquare", "Square"), (Some(Wrap::Tight), "wrapTight", "Tight"),
    (Some(Wrap::TopBottom), "wrapTopBottom", "Top and Bottom"), (Some(Wrap::Behind), "behindText", "Behind Text"), (Some(Wrap::InFront), "inFront", "In Front of Text"),
];

#[derive(Clone, Copy, PartialEq)]
pub enum Turn { Right, Left, FlipH, FlipV }

impl WordApp {
    /// The selected picture: a one-character selection over a picture run.
    pub fn selected_picture(&self) -> Option<(usize, Image)> {
        let (s, e) = self.doc.range();
        if e != s + 1 { return None; }
        self.doc.image_at(s).cloned().map(|i| (s, i))
    }

    pub fn picture_op(&mut self, op: PictureOp) {
        let Some((pos, _)) = self.selected_picture() else { return };
        if !self.doc.apply_picture_op(pos, op) { self.dialogs.error = Some("This picture's format can't be edited.".into()); }
    }

    /// Rotate or flip the selected picture (its pixels) or shape (its transform).
    pub fn turn_selection(&mut self, turn: Turn) {
        if let Some((pos, img)) = self.selected_picture() {
            // Pictures turn and mirror as Word does: a:xfrm rot/flipH/flipV, pixels unchanged.
            match turn {
                Turn::Right => self.doc.set_image_rotation(pos, img.rotation + 90.0),
                Turn::Left => self.doc.set_image_rotation(pos, img.rotation - 90.0),
                Turn::FlipH => self.doc.flip_image(pos, true),
                Turn::FlipV => self.doc.flip_image(pos, false),
            }
            return;
        }
        if let Some((pos, Object::Shape(mut s))) = self.selected_object() {
            match turn {
                Turn::Right => s.rotation = (s.rotation + 90.0).rem_euclid(360.0),
                Turn::Left => s.rotation = (s.rotation - 90.0).rem_euclid(360.0),
                Turn::FlipH => s.flip_h = !s.flip_h,
                Turn::FlipV => s.flip_v = !s.flip_v,
            }
            s.raw = None;
            self.doc.set_object(pos, Object::Shape(s));
            self.doc.select(pos, pos + 1);
        }
    }

    pub fn change_picture(&mut self) {
        let Some((pos, _)) = self.selected_picture() else { return };
        let Some(p) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() else { return };
        match std::fs::read(&p) {
            Ok(bytes) => { if !self.doc.change_picture(pos, bytes) { self.dialogs.error = Some("Couldn't read that picture.".into()); } }
            Err(e) => self.dialogs.error = Some(crate::i18n::trf("Couldn't read the picture.\n\n{}", &[&e])),
        }
    }

    /// Save the selected picture's pixels, or a shape, chart or diagram rendered as PNG.
    pub fn save_as_picture(&mut self) {
        let (bytes, ext) = if let Some((_, img)) = self.selected_picture() {
            let Some(a) = self.doc.assets.get(&img.asset) else { return };
            (a.bytes.clone(), img.asset.rsplit('.').next().unwrap_or("png").to_string())
        } else if let Some((_, o)) = self.selected_object() {
            let Some(png) = word_engine::objects::object_png(&o) else { return };
            (png, "png".into())
        } else { return };
        let Some(path) = rfd::FileDialog::new().set_file_name(format!("Picture.{ext}")).save_file() else { return };
        if let Err(e) = std::fs::write(&path, bytes) { self.dialogs.error = Some(crate::i18n::trf("Couldn't save the picture.\n\n{}", &[&e])); }
    }

    /// The selected picture or object (either kind).
    pub fn selected_box(&self) -> Option<usize> {
        let (s, e) = self.doc.range();
        (e == s + 1 && (self.doc.image_at(s).is_some() || self.doc.object_at(s).is_some())).then_some(s)
    }

    /// Change how text wraps the selection. Floating it keeps it where it is on the page.
    pub fn set_wrap(&mut self, wrap: Option<Wrap>) {
        let Some(pos) = self.selected_box() else { return };
        let anchor = match (wrap, self.doc.anchor_at(pos)) {
            (None, _) => None,
            (Some(w), Some(mut a)) => { a.wrap = w; Some(a) }
            (Some(w), None) => {
                let g = self.doc.page.geometry();
                let (pg, x, y) = self.layout.object_box(pos).map(|b| (b.0, b.1, b.2)).unwrap_or((0, g.left, g.top));
                let pi = self.doc.locate(pos).0;
                let top = self.layout.lines.iter().find(|l| l.paragraph == pi && l.page == pg).map(|l| l.y).unwrap_or(y);
                Some(Anchor::new(w, x - g.left, y - top))
            }
        };
        self.doc.set_anchor(pos, anchor);
    }

    /// Place a floating selection at one of nine spots in the margins (Position gallery).
    pub fn set_position(&mut self, h: AnchorAlign, v: AnchorAlign) {
        let Some(pos) = self.selected_box() else { return };
        let mut a = self.doc.anchor_at(pos).unwrap_or(Anchor::new(Wrap::Square, 0.0, 0.0));
        a.x_from = RelFrom::Margin; a.y_from = RelFrom::Margin;
        a.x_align = Some(h); a.y_align = Some(v);
        self.doc.set_anchor(pos, Some(a));
    }

    pub fn open_layout(&mut self) {
        let Some(pos) = self.selected_box() else { return };
        let (w, h) = self.selection_size().unwrap_or((96.0, 96.0));
        let rotation = match (self.doc.image_at(pos), self.doc.object_at(pos)) { (Some(i), _) => i.rotation, (_, Some(Object::Shape(s))) => s.rotation, _ => 0.0 };
        let wrap = self.doc.anchor_at(pos).map(|a| a.wrap);
        let anchor = self.doc.anchor_at(pos).unwrap_or(Anchor::new(Wrap::Square, 0.0, 0.0));
        self.insert.layout = Some(LayoutDraft { pos, wrap, anchor, w, h, lock: self.doc.image_at(pos).is_some(), rotation });
    }


    pub fn open_crop(&mut self, ctx: &egui::Context) {
        let Some((pos, img)) = self.selected_picture() else { return };
        let texture = self.doc.assets.get(&img.asset).and_then(|a| crate::insert::texture(ctx, "crop-preview", &a.bytes));
        self.insert.crop = Some(CropDraft { pos, left: 0.0, top: 0.0, right: 0.0, bottom: 0.0, texture });
    }

    pub fn open_alt_text(&mut self) {
        if let Some((pos, img)) = self.selected_picture() { self.insert.alt = Some((pos, img.alt)); }
    }

    /// Set the selected picture's or object's size in px, optionally keeping its aspect ratio.
    pub fn set_selection_size(&mut self, w: f32, h: f32) {
        let (s, e) = self.doc.range();
        if e == s + 1 && (self.doc.image_at(s).is_some() || self.doc.object_at(s).is_some()) {
            self.doc.resize_image(s, w, h);
            self.doc.select(s, s + 1);
        }
    }

    pub fn selection_size(&self) -> Option<(f32, f32)> {
        if let Some((_, i)) = self.selected_picture() { return Some((i.width, i.height)); }
        self.selected_object().map(|(_, o)| o.size())
    }
}

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    crop_dialog(app, ctx);
    alt_dialog(app, ctx);
    layout_dialog(app, ctx);
}

/// Wrap Text choices (ribbon, Layout Options button and context menu).
pub fn wrap_menu(app: &mut WordApp, ui: &mut egui::Ui) {
    let current = app.selected_box().map(|p| app.doc.anchor_at(p).map(|a| a.wrap));
    for (wrap, icon, name) in WRAPS {
        let on = current == Some(wrap);
        if crate::ribbon::small(ui, icon, Some(name), on, true, name).clicked() { app.set_wrap(wrap); ui.close(); }
    }
    ui.separator();
    if ui.button(crate::i18n::tr("More Layout Options…")).clicked() { app.open_layout(); ui.close(); }
}

/// Position gallery: in line with text, or nine spots with square wrapping.
pub fn position_menu(app: &mut WordApp, ui: &mut egui::Ui) {
    if crate::ribbon::small(ui, "wrapInline", Some("In Line with Text"), false, true, "In line with text").clicked() { app.set_wrap(None); ui.close(); }
    ui.label(egui::RichText::new(crate::i18n::tr("With Text Wrapping")).strong());
    crate::theme::grid(ui, egui::Grid::new("position-grid").spacing(vec2(4.0, 4.0)), |ui| {
        for (v, vn) in [(AnchorAlign::Start, "Top"), (AnchorAlign::Center, "Middle"), (AnchorAlign::End, "Bottom")] {
            for (h, hn) in [(AnchorAlign::Start, "Left"), (AnchorAlign::Center, "Center"), (AnchorAlign::End, "Right")] {
                let (rect, r) = ui.allocate_exact_size(vec2(40.0, 48.0), Sense::click());
                let page = rect.shrink(3.0);
                ui.painter().rect(page, 2.0, Color32::WHITE, Stroke::new(1.0, if r.hovered() { Color32::from_rgb(40, 110, 220) } else { Color32::from_gray(170) }), egui::StrokeKind::Inside);
                for k in 0..7 { let y = page.min.y + 5.0 + k as f32 * 5.5; ui.painter().line_segment([egui::pos2(page.min.x + 4.0, y), egui::pos2(page.max.x - 4.0, y)], Stroke::new(1.0, Color32::from_gray(200))); }
                let bw = vec2(12.0, 10.0);
                let fx = match h { AnchorAlign::Start => 0.0, AnchorAlign::Center => 0.5, AnchorAlign::End => 1.0 };
                let fy = match v { AnchorAlign::Start => 0.0, AnchorAlign::Center => 0.5, AnchorAlign::End => 1.0 };
                let inner = page.shrink(4.0);
                let b = Rect::from_min_size(inner.min + vec2((inner.width() - bw.x) * fx, (inner.height() - bw.y) * fy), bw);
                ui.painter().rect_filled(b, 1.0, Color32::from_rgb(68, 114, 196));
                if r.on_hover_text(crate::i18n::trf("Position in {} {}", &[&vn, &hn])).clicked() { app.set_position(h, v); ui.close(); }
            }
            ui.end_row();
        }
    });
    ui.separator();
    if ui.button(crate::i18n::tr("More Layout Options…")).clicked() { app.open_layout(); ui.close(); }
}

fn layout_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(d) = &mut app.insert.layout else { return };
    let (mut open, mut apply, mut cancel) = (true, false, false);
    crate::theme::dialog(ctx, "Layout").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label(egui::RichText::new(crate::i18n::tr("Text Wrapping")).strong());
        ui.horizontal_wrapped(|ui| {
            for (wrap, icon, name) in WRAPS {
                if crate::ribbon::small(ui, icon, Some(name), d.wrap == wrap, true, name).clicked() { d.wrap = wrap; }
            }
        });
        crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Distance from text")); let mut v = d.anchor.dist / 96.0; if ui.add(DragValue::new(&mut v).range(0.0..=2.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { d.anchor.dist = v * 96.0; } });
        ui.add_enabled_ui(d.wrap.is_some(), |ui| {
            ui.separator();
            ui.label(egui::RichText::new(crate::i18n::tr("Position")).strong());
            let names = |a: Option<AnchorAlign>, h: bool| match a { None => "Absolute", Some(AnchorAlign::Start) => if h { "Left" } else { "Top" }, Some(AnchorAlign::Center) => if h { "Centered" } else { "Middle" }, Some(AnchorAlign::End) => if h { "Right" } else { "Bottom" } };
            crate::theme::grid(ui, egui::Grid::new("layout-pos").num_columns(4), |ui| {
                for horizontal in [true, false] {
                    ui.label(if horizontal { "Horizontal" } else { "Vertical" });
                    let (align, off, from) = if horizontal { (&mut d.anchor.x_align, &mut d.anchor.x, &mut d.anchor.x_from) } else { (&mut d.anchor.y_align, &mut d.anchor.y, &mut d.anchor.y_from) };
                    crate::theme::Combo::from_id_salt(("align", horizontal)).selected_text(crate::i18n::tr(&(names(*align, horizontal)))).show_ui(ui, |ui| {
                        for a in [None, Some(AnchorAlign::Start), Some(AnchorAlign::Center), Some(AnchorAlign::End)] { ui.selectable_value(align, a, crate::i18n::tr(&(names(a, horizontal)))); }
                    });
                    if align.is_none() { let mut v = *off / 96.0; if ui.add(DragValue::new(&mut v).speed(0.01).max_decimals(2).suffix("\"")).changed() { *off = v * 96.0; } } else { ui.label(""); }
                    let froms: &[(RelFrom, &str)] = if horizontal { &[(RelFrom::Margin, "relative to Margin"), (RelFrom::Page, "relative to Page")] } else { &[(RelFrom::Paragraph, "below Paragraph"), (RelFrom::Margin, "relative to Margin"), (RelFrom::Page, "relative to Page")] };
                    crate::theme::Combo::from_id_salt(("from", horizontal)).selected_text(crate::i18n::tr(&(froms.iter().find(|f| f.0 == *from).map(|f| f.1).unwrap_or("")))).show_ui(ui, |ui| {
                        for (f, n) in froms { ui.selectable_value(from, *f, crate::i18n::tr(&(*n))); }
                    });
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.label(egui::RichText::new(crate::i18n::tr("Size")).strong());
        crate::theme::hrow(ui, |ui| {
            let (w0, h0) = (d.w, d.h);
            let mut w = d.w / 96.0; let mut h = d.h / 96.0;
            ui.label(crate::i18n::tr("Height"));
            if ui.add(DragValue::new(&mut h).range(0.05..=22.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { d.h = h * 96.0; if d.lock && h0 > 0.0 { d.w = w0 * d.h / h0; } }
            ui.label(crate::i18n::tr("Width"));
            if ui.add(DragValue::new(&mut w).range(0.05..=22.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { d.w = w * 96.0; if d.lock && w0 > 0.0 { d.h = h0 * d.w / w0; } }
            crate::theme::check(ui, &mut d.lock, crate::i18n::tr("Lock aspect ratio"));
        });
        crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Rotation")); ui.add(DragValue::new(&mut d.rotation).range(0.0..=359.9).speed(1.0).suffix("°")); });
        ui.separator();
        crate::theme::footer(ui, |ui| { if crate::theme::primary(ui, "OK").clicked() { apply = true; } if crate::theme::secondary(ui, "Cancel").clicked() { cancel = true; } });
    });
    if apply {
        let Some(d) = app.insert.layout.take() else { return };
        let pos = d.pos;
        if app.doc.image_at(pos).is_none() && app.doc.object_at(pos).is_none() { return; }
        app.doc.resize_image(pos, d.w, d.h);
        match app.doc.object_at(pos).cloned() {
            Some(Object::Shape(mut s)) if s.rotation != d.rotation => { s.rotation = d.rotation; s.raw = None; app.doc.set_object(pos, Object::Shape(s)); }
            _ => if app.doc.image_at(pos).is_some_and(|i| i.rotation != d.rotation) { app.doc.set_image_rotation(pos, d.rotation); },
        }
        app.doc.set_anchor(pos, d.wrap.map(|w| Anchor { wrap: w, ..d.anchor }));
        app.doc.select(pos, pos + 1);
    } else if !open || cancel { app.insert.layout = None; }
}

fn crop_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(d) = &mut app.insert.crop else { return };
    let mut open = true;
    let mut apply = false;
    crate::theme::dialog(ctx, "Crop Picture").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        if let Some(tex) = &d.texture {
            let size = tex.size_vec2();
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
            ui.painter().image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
            let keep = Rect::from_min_max(rect.min + vec2(d.left * size.x, d.top * size.y), rect.max - vec2(d.right * size.x, d.bottom * size.y));
            // Shade what the crop removes.
            let shade = Color32::from_black_alpha(140);
            for r in [Rect::from_min_max(rect.min, egui::pos2(rect.max.x, keep.min.y)), Rect::from_min_max(egui::pos2(rect.min.x, keep.max.y), rect.max), Rect::from_min_max(egui::pos2(rect.min.x, keep.min.y), egui::pos2(keep.min.x, keep.max.y)), Rect::from_min_max(egui::pos2(keep.max.x, keep.min.y), egui::pos2(rect.max.x, keep.max.y))] { ui.painter().rect_filled(r, 0.0, shade); }
            ui.painter().rect_stroke(keep, 0.0, Stroke::new(2.0, Color32::BLACK), egui::StrokeKind::Inside);
            // Dragging moves the nearest edge(s) toward the pointer.
            if let (true, Some(p)) = (resp.dragged(), resp.interact_pointer_pos()) {
                let fx = ((p.x - rect.min.x) / size.x).clamp(0.0, 1.0);
                let fy = ((p.y - rect.min.y) / size.y).clamp(0.0, 1.0);
                let tol = 0.2;
                if (fx - d.left).abs() < tol && fx < 1.0 - d.right - 0.05 { d.left = fx; }
                else if (1.0 - fx - d.right).abs() < tol && fx > d.left + 0.05 { d.right = 1.0 - fx; }
                if (fy - d.top).abs() < tol && fy < 1.0 - d.bottom - 0.05 { d.top = fy; }
                else if (1.0 - fy - d.bottom).abs() < tol && fy > d.top + 0.05 { d.bottom = 1.0 - fy; }
            }
        }
        crate::theme::grid(ui, egui::Grid::new("crop-sides").num_columns(4), |ui| {
            let side = |ui: &mut egui::Ui, name: &str, v: &mut f32| { ui.label(crate::i18n::tr(name)); let mut pct = *v * 100.0; if ui.add(DragValue::new(&mut pct).range(0.0..=90.0).suffix(" %")).changed() { *v = pct / 100.0; } };
            side(ui, "Left", &mut d.left); side(ui, "Right", &mut d.right); ui.end_row();
            side(ui, "Top", &mut d.top); side(ui, "Bottom", &mut d.bottom); ui.end_row();
        });
        crate::theme::hrow(ui, |ui| {
            for (name, ratio) in [("Square 1:1", 1.0), ("4:3", 4.0 / 3.0), ("16:9", 16.0 / 9.0), ("3:4", 0.75)] {
                if ui.button(crate::i18n::tr(&name)).clicked() {
                    // Center crop to the ratio.
                    if let Some(tex) = &d.texture {
                        let s = tex.size_vec2();
                        let cur = s.x / s.y;
                        let (mut l, mut t) = (0.0, 0.0);
                        if cur > ratio { l = (1.0 - ratio / cur) / 2.0; } else { t = (1.0 - cur / ratio) / 2.0; }
                        (d.left, d.right, d.top, d.bottom) = (l, l, t, t);
                    }
                }
            }
        });
        ui.separator();
        crate::theme::hrow(ui, |ui| {
            if ui.button(crate::i18n::tr("Reset")).clicked() { (d.left, d.right, d.top, d.bottom) = (0.0, 0.0, 0.0, 0.0); }
            if ui.add(egui::Button::new(crate::i18n::tr("Crop")).fill(Color32::from_rgb(0x2B, 0x57, 0x9A))).clicked() { apply = true; }
        });
    });
    if apply {
        let (pos, op) = (d.pos, PictureOp::Crop { left: d.left, top: d.top, right: d.right, bottom: d.bottom });
        app.insert.crop = None;
        if app.doc.image_at(pos).is_some() && !app.doc.apply_picture_op(pos, op) { app.dialogs.error = Some("Couldn't crop the picture.".into()); }
    } else if !open { app.insert.crop = None; }
}

fn alt_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some((pos, text)) = &mut app.insert.alt else { return };
    let (mut open, mut done) = (true, false);
    crate::theme::dialog(ctx, "Alt Text").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label(crate::i18n::tr("How would you describe this picture to someone who is blind or has low vision?"));
        ui.add(egui::TextEdit::multiline(text).desired_rows(4).desired_width(320.0));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { done = true; }
            if crate::theme::secondary(ui, "Mark as decorative").clicked() { text.clear(); done = true; }
        });
    });
    if done {
        let (pos, text) = (*pos, text.clone());
        app.insert.alt = None;
        app.doc.edit_image(pos, |i| i.alt = text);
    } else if !open { app.insert.alt = None; }
}
