//! Saved signatures and initials (kept between runs) and the dialog that makes them.

use crate::i18n::{n_, tr, trf};
use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui::{self, Color32, Pos2, Stroke, Vec2, pos2, vec2};
use pdf_engine::{Color, FontFace, NewItem, Point, Rect};

use crate::app::{App, Stamp, Tool};
use crate::theme::{self, palette};

/// How a signature was made.
#[derive(Clone, Debug, PartialEq)]
pub enum SigData {
    /// Strokes with coordinates in units of the signature's height.
    Ink(Vec<Vec<[f32; 2]>>),
    Typed(String),
    /// PNG or JPEG bytes.
    Image(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Signature {
    pub data: SigData,
    pub initials: bool,
    file: Option<PathBuf>,
}

/// Signatures on disk (one file each) plus preview textures.
#[derive(Default)]
pub struct Store {
    pub items: Vec<Signature>,
    dir: Option<PathBuf>,
    textures: HashMap<usize, egui::TextureHandle>,
}

/// `<config dir>/signatures`, e.g. ~/Library/Application Support/Waraq/signatures.
pub fn default_dir() -> Option<PathBuf> {
    Some(crate::config_dir().join("signatures"))
}

fn ink_to_text(strokes: &[Vec<[f32; 2]>]) -> String {
    strokes
        .iter()
        .map(|s| {
            s.iter()
                .map(|p| format!("{:.4},{:.4}", p[0], p[1]))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn ink_from_text(text: &str) -> Vec<Vec<[f32; 2]>> {
    text.lines()
        .map(|l| {
            l.split_whitespace()
                .filter_map(|p| {
                    let (x, y) = p.split_once(',')?;
                    Some([x.parse().ok()?, y.parse().ok()?])
                })
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

impl Store {
    /// Loads every saved signature from `dir` (created on first save).
    pub fn open(dir: PathBuf) -> Self {
        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            // Oldest first: names are <kind>-<millis>-<n>.<ext>.
            let key = |p: &PathBuf| {
                let name = p
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut parts = name
                    .splitn(3, '-')
                    .skip(1)
                    .map(|x| x.parse::<u128>().unwrap_or(0));
                (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
            };
            paths.sort_by_key(key);
            for path in paths {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let initials = name.starts_with("ini-");
                if !initials && !name.starts_with("sig-") {
                    continue;
                }
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let data = match ext.as_str() {
                    "ink" => std::fs::read_to_string(&path)
                        .ok()
                        .map(|t| SigData::Ink(ink_from_text(&t))),
                    "txt" => std::fs::read_to_string(&path).ok().map(SigData::Typed),
                    "png" | "jpg" => std::fs::read(&path).ok().map(SigData::Image),
                    _ => None,
                };
                if let Some(data) = data {
                    items.push(Signature {
                        data,
                        initials,
                        file: Some(path),
                    });
                }
            }
        }
        Self {
            items,
            dir: Some(dir),
            textures: HashMap::new(),
        }
    }

    /// Adds a signature, writing it to disk when the store has a folder.
    pub fn add(&mut self, data: SigData, initials: bool) -> Result<usize, String> {
        let mut file = None;
        if let Some(dir) = &self.dir {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let (ext, bytes): (&str, Vec<u8>) = match &data {
                SigData::Ink(s) => ("ink", ink_to_text(s).into_bytes()),
                SigData::Typed(t) => ("txt", t.clone().into_bytes()),
                SigData::Image(b) => (
                    if b.starts_with(&[0xFF, 0xD8]) {
                        "jpg"
                    } else {
                        "png"
                    },
                    b.clone(),
                ),
            };
            let n = self.items.len();
            let path = dir.join(format!(
                "{}-{stamp}-{n}.{ext}",
                if initials { "ini" } else { "sig" }
            ));
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            file = Some(path);
        }
        self.items.push(Signature {
            data,
            initials,
            file,
        });
        Ok(self.items.len() - 1)
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            let sig = self.items.remove(index);
            if let Some(f) = sig.file {
                let _ = std::fs::remove_file(f);
            }
            self.textures.clear();
        }
    }

    fn texture(&mut self, ctx: &egui::Context, index: usize) -> Option<egui::TextureId> {
        if let Some(t) = self.textures.get(&index) {
            return Some(t.id());
        }
        let SigData::Image(bytes) = &self.items.get(index)?.data else {
            return None;
        };
        let tex = load_texture(ctx, bytes, &format!("sig-{index}"))?;
        let id = tex.id();
        self.textures.insert(index, tex);
        Some(id)
    }
}

fn load_texture(ctx: &egui::Context, bytes: &[u8], name: &str) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes)
        .ok()?
        .thumbnail(400, 200)
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

fn image_aspect(bytes: &[u8]) -> f32 {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .map_or(2.0, |(w, h)| w as f32 / h.max(1) as f32)
}

const INK: Color = Color {
    r: 0.08,
    g: 0.14,
    b: 0.45,
};

/// Today's date as YYYY-MM-DD (UTC).
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The page object a stamp becomes when clicked at `at` (view points).
pub fn stamp_item(store: &Store, stamp: Stamp, at: Point) -> Option<NewItem> {
    let sig = |i: usize| store.items.get(i);
    let (sig, height) = match stamp {
        Stamp::Date => {
            return Some(NewItem::Text {
                at: Point::new(at.x, at.y - 8.0),
                text: today(),
                face: FontFace::Sans,
                size: 12.0,
                color: Color::rgb(21, 21, 21),
            });
        }
        Stamp::Check => {
            let s = |x: f32, y: f32| Point::new(at.x - 7.0 + x, at.y - 7.0 + y);
            return Some(NewItem::Ink {
                strokes: vec![vec![s(0.0, 7.5), s(5.0, 13.0), s(14.0, 1.0)]],
                color: Color::rgb(21, 21, 21),
                width: 2.0,
            });
        }
        Stamp::Signature(i) => (sig(i)?, 42.0),
        Stamp::Initials(i) => (sig(i)?, 30.0),
    };
    Some(match &sig.data {
        SigData::Ink(strokes) => {
            let w = strokes.iter().flatten().map(|p| p[0]).fold(0.0, f32::max) * height;
            NewItem::Ink {
                strokes: strokes
                    .iter()
                    .map(|s| {
                        s.iter()
                            .map(|p| {
                                Point::new(
                                    at.x - w / 2.0 + p[0] * height,
                                    at.y - height / 2.0 + p[1] * height,
                                )
                            })
                            .collect()
                    })
                    .collect(),
                color: INK,
                width: 1.6,
            }
        }
        SigData::Typed(text) => {
            let size = height * 0.75;
            NewItem::Text {
                at: Point::new(at.x, at.y - size * 0.7),
                text: text.clone(),
                face: FontFace::Script,
                size,
                color: INK,
            }
        }
        SigData::Image(bytes) => {
            let w = height * image_aspect(bytes);
            NewItem::Image {
                rect: Rect::new(
                    at.x - w / 2.0,
                    at.y - height / 2.0,
                    at.x + w / 2.0,
                    at.y + height / 2.0,
                ),
                data: bytes.clone(),
            }
        }
    })
}

/// Draws a signature preview fitted into `rect`.
pub fn preview(ui: &mut egui::Ui, store: &mut Store, index: usize, rect: egui::Rect) {
    let p = palette(ui.visuals().dark_mode);
    let ink = if ui.visuals().dark_mode {
        Color32::from_rgb(170, 190, 255)
    } else {
        Color32::from_rgb(20, 36, 115)
    };
    let Some(sig) = store.items.get(index) else {
        return;
    };
    match &sig.data {
        SigData::Ink(strokes) => {
            let w = strokes.iter().flatten().map(|q| q[0]).fold(0.01, f32::max);
            let s = (rect.width() / w).min(rect.height());
            let origin = rect.center() - vec2(w * s / 2.0, s / 2.0);
            for stroke in strokes {
                let pts: Vec<Pos2> = stroke
                    .iter()
                    .map(|q| origin + vec2(q[0], q[1]) * s)
                    .collect();
                ui.painter()
                    .add(egui::Shape::line(pts, Stroke::new(1.5, ink)));
            }
        }
        SigData::Typed(text) => {
            ui.painter().text(
                rect.left_center(),
                egui::Align2::LEFT_CENTER,
                crate::i18n::shape(text),
                script_font(rect.height() * 0.7),
                ink,
            );
        }
        SigData::Image(_) => {
            if let Some(tex) = store.texture(ui.ctx(), index) {
                let size = ui.ctx().tex_manager().read().meta(tex).map(|m| m.size);
                let aspect = size.map_or(2.0, |s| s[0] as f32 / s[1].max(1) as f32);
                let h = rect.height().min(rect.width() / aspect);
                let r = egui::Rect::from_min_size(
                    rect.left_center() - vec2(0.0, h / 2.0),
                    vec2(h * aspect, h),
                );
                ui.painter().image(
                    tex,
                    r,
                    egui::Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    tr("picture"),
                    egui::FontId::proportional(11.0),
                    p.muted,
                );
            }
        }
    }
}

pub fn script_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(theme::SCRIPT_FAMILY.into()))
}

/// The Create signature / initials dialog.
pub struct Dialog {
    pub initials: bool,
    pub tab: usize,
    pub strokes: Vec<Vec<[f32; 2]>>,
    pub typed: String,
    image: Option<(Vec<u8>, egui::TextureHandle)>,
}

impl Dialog {
    pub fn new(initials: bool) -> Self {
        Self {
            initials,
            tab: 0,
            strokes: Vec::new(),
            typed: String::new(),
            image: None,
        }
    }

    fn data(&self) -> Option<SigData> {
        match self.tab {
            0 => {
                // Normalize to the drawing's own bounds, height 1.
                let pts: Vec<&[f32; 2]> = self.strokes.iter().flatten().collect();
                let (x0, y0) = pts
                    .iter()
                    .fold((f32::MAX, f32::MAX), |a, p| (a.0.min(p[0]), a.1.min(p[1])));
                let y1 = pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
                let h = (y1 - y0).max(10.0);
                (!pts.is_empty()).then(|| {
                    SigData::Ink(
                        self.strokes
                            .iter()
                            .map(|s| {
                                s.iter()
                                    .map(|p| [(p[0] - x0) / h, (p[1] - y0) / h])
                                    .collect()
                            })
                            .collect(),
                    )
                })
            }
            1 => (!self.typed.trim().is_empty())
                .then(|| SigData::Typed(self.typed.trim().to_string())),
            _ => self.image.as_ref().map(|(b, _)| SigData::Image(b.clone())),
        }
    }
}

pub fn dialog(app: &mut App, ctx: &egui::Context) {
    let Some(d) = app.sign_dialog.as_mut() else {
        return;
    };
    let p = palette(ctx.global_style().visuals.dark_mode);
    let mut close = false;
    let mut save = None;
    egui::Modal::new(egui::Id::unique("sign_dialog")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.with_layout(theme::column_layout(), |ui| {
            ui.label(
                egui::RichText::new(if d.initials {
                    tr("Create initials")
                } else {
                    tr("Create signature")
                })
                .font(theme::bold(16.0)),
            );
            ui.add_space(4.0);
            theme::row(ui, |ui| {
                for (i, label) in [n_("Draw"), n_("Type"), n_("Image")].iter().enumerate() {
                    if ui.selectable_label(d.tab == i, tr(label)).clicked() {
                        d.tab = i;
                    }
                }
            });
            ui.add_space(6.0);
            let (rect, response) = ui.allocate_exact_size(
                vec2(460.0, 170.0),
                if d.tab == 0 {
                    egui::Sense::drag()
                } else {
                    egui::Sense::hover()
                },
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Canvas, true, tr("Signature area"))
            });
            ui.painter().rect(
                rect,
                8.0,
                Color32::WHITE,
                Stroke::new(1.0, p.border),
                egui::StrokeKind::Inside,
            );
            let base_y = rect.bottom() - 40.0;
            ui.painter().hline(
                rect.left() + 24.0..=rect.right() - 24.0,
                base_y,
                Stroke::new(1.0, Color32::from_gray(200)),
            );
            let ink = Color32::from_rgb(20, 36, 115);
            match d.tab {
                0 => {
                    if response.drag_started() {
                        d.strokes.push(Vec::new());
                    }
                    if response.dragged()
                        && let Some(pos) = response.interact_pointer_pos()
                        && let Some(s) = d.strokes.last_mut()
                    {
                        let q = pos - rect.min;
                        if s.last()
                            .is_none_or(|l| (l[0] - q.x).abs() + (l[1] - q.y).abs() > 1.0)
                        {
                            s.push([q.x, q.y]);
                        }
                    }
                    for s in &d.strokes {
                        let pts: Vec<Pos2> =
                            s.iter().map(|q| rect.min + vec2(q[0], q[1])).collect();
                        ui.painter()
                            .add(egui::Shape::line(pts, Stroke::new(2.2, ink)));
                    }
                    if d.strokes.is_empty() {
                        ui.painter().text(
                            rect.center() - vec2(0.0, 20.0),
                            egui::Align2::CENTER_CENTER,
                            tr("Draw here with the mouse or trackpad"),
                            egui::FontId::proportional(13.0),
                            Color32::from_gray(150),
                        );
                    }
                }
                1 => {
                    let shown = if d.typed.is_empty() {
                        tr("Your name")
                    } else {
                        d.typed.clone()
                    };
                    let color = if d.typed.is_empty() {
                        Color32::from_gray(190)
                    } else {
                        ink
                    };
                    ui.painter().with_clip_rect(rect).text(
                        pos2(rect.left() + 28.0, base_y - 6.0),
                        egui::Align2::LEFT_BOTTOM,
                        crate::i18n::shape(&shown),
                        script_font(52.0),
                        color,
                    );
                }
                _ => {
                    if let Some((_, tex)) = &d.image {
                        let size = tex.size_vec2();
                        let s =
                            ((rect.width() - 40.0) / size.x).min((rect.height() - 30.0) / size.y);
                        let r = egui::Rect::from_center_size(rect.center(), size * s);
                        ui.painter().image(
                            tex.id(),
                            r,
                            egui::Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    } else {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            tr("PNG or JPEG of your signature"),
                            egui::FontId::proportional(13.0),
                            Color32::from_gray(150),
                        );
                    }
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| match d.tab {
                0 => {
                    if ui.button(tr("Clear")).clicked() {
                        d.strokes.clear();
                    }
                }
                1 => {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut d.typed)
                            .hint_text(tr("Type your name"))
                            .desired_width(300.0),
                    );
                    r.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("Name"))
                    });
                }
                _ => {
                    if ui.button(tr("Choose picture…")).clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Pictures", &["png", "jpg", "jpeg"])
                            .pick_file()
                        && let Ok(bytes) = std::fs::read(path)
                        && let Some(tex) = load_texture(ui.ctx(), &bytes, "sig-import")
                    {
                        d.image = Some((bytes, tex));
                    }
                }
            });
            ui.add_space(8.0);
            theme::row(ui, |ui| {
                ui.label(
                    egui::RichText::new(tr("Saved on this computer for next time."))
                        .size(11.5)
                        .color(p.muted),
                );
                ui.with_layout(theme::far_layout(), |ui| {
                    let data = d.data();
                    let ok = ui
                        .add_enabled_ui(data.is_some(), |ui| theme::primary_button(ui, &tr("Save")))
                        .inner;
                    if ok.clicked() {
                        save = data;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });
    if let Some(data) = save {
        let initials = d.initials;
        match app.signatures.add(data, initials) {
            Ok(i) => {
                app.tool = Tool::Place(if initials {
                    Stamp::Initials(i)
                } else {
                    Stamp::Signature(i)
                });
                app.notify(tr("Saved. Click on the page to place it."));
            }
            Err(e) => app.notify(trf("Could not save the signature: {}", &[&e])),
        }
        close = true;
    }
    if close {
        app.sign_dialog = None;
    }
}

/// Fill & Sign tools pane.
pub fn tools(app: &mut App, ui: &mut egui::Ui) {
    let p = palette(ui.visuals().dark_mode);
    theme::caption(ui, &tr("Fill & Sign"));
    if theme::tool_row(
        ui,
        crate::theme::Icon::Select,
        &tr("Select"),
        app.tool == Tool::Select,
    )
    .clicked()
    {
        app.tool = Tool::Select;
    }
    for initials in [false, true] {
        ui.add_space(8.0);
        theme::caption(
            ui,
            &if initials {
                tr("Initials")
            } else {
                tr("Signatures")
            },
        );
        let list: Vec<usize> = (0..app.signatures.items.len())
            .filter(|&i| app.signatures.items[i].initials == initials)
            .collect();
        for i in list {
            let stamp = if initials {
                Stamp::Initials(i)
            } else {
                Stamp::Signature(i)
            };
            let on = app.tool == Tool::Place(stamp);
            let (rect, r) =
                ui.allocate_exact_size(vec2(ui.available_width(), 46.0), egui::Sense::click());
            r.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    on,
                    if initials {
                        trf("Place initials {}", &[&(i + 1)])
                    } else {
                        trf("Place signature {}", &[&(i + 1)])
                    },
                )
            });
            ui.painter().rect(
                rect,
                6.0,
                if on { p.accent_soft } else { p.surface },
                Stroke::new(1.0, if on { p.accent } else { p.line }),
                egui::StrokeKind::Inside,
            );
            preview(
                ui,
                &mut app.signatures,
                i,
                rect.shrink2(vec2(10.0, 7.0))
                    .with_max_x(rect.right() - 30.0),
            );
            let del = egui::Rect::from_center_size(
                pos2(rect.right() - 15.0, rect.center().y),
                Vec2::splat(22.0),
            );
            let dr = ui.interact(del, r.id.with("del"), egui::Sense::click());
            dr.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, tr("Delete saved signature"))
            });
            theme::draw_icon(
                ui.painter(),
                del.center(),
                crate::theme::Icon::Trash,
                if dr.hovered() { p.accent } else { p.muted },
                14.0,
            );
            if dr.clicked() {
                app.signatures.remove(i);
                app.tool = Tool::Select;
                return;
            } else if r.clicked() {
                app.tool = Tool::Place(stamp);
            }
        }
        let label = if initials {
            tr("Create initials…")
        } else {
            tr("Create signature…")
        };
        if theme::tool_row(ui, crate::theme::Icon::Signature, &label, false).clicked() {
            app.sign_dialog = Some(Dialog::new(initials));
        }
    }
    ui.add_space(8.0);
    theme::caption(ui, &tr("Fill in"));
    for stamp in [Stamp::Date, Stamp::Check] {
        let tool = Tool::Place(stamp);
        if theme::tool_row(ui, tool.icon(), &tr(tool.label()), app.tool == tool).clicked() {
            app.tool = tool;
        }
    }
    if theme::tool_row(
        ui,
        crate::theme::Icon::TextBox,
        &tr("Add text"),
        app.tool == Tool::AddText,
    )
    .clicked()
    {
        app.tool = Tool::AddText;
    }
    ui.add_space(10.0);
    theme::para(ui, app.tool.hint(), 12.0, Some(p.muted));
    theme::para(
        ui,
        n_("Signatures are written into the page content, so they print and show in every viewer."),
        11.5,
        Some(p.muted),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_round_trips_on_disk() {
        let dir = std::env::temp_dir().join(format!("waraq-sig-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Store::open(dir.clone());
        s.add(SigData::Ink(vec![vec![[0.0, 0.0], [1.5, 0.5]]]), false)
            .unwrap();
        s.add(SigData::Typed("S. M.".into()), true).unwrap();
        let back = Store::open(dir.clone());
        assert_eq!(back.items.len(), 2);
        assert_eq!(
            back.items[0].data,
            SigData::Ink(vec![vec![[0.0, 0.0], [1.5, 0.5]]])
        );
        assert!(back.items[1].initials);
        let mut back = back;
        back.remove(0);
        assert_eq!(Store::open(dir.clone()).items.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn date_shape() {
        let d = today();
        assert_eq!(d.len(), 10);
        assert_eq!(&d[4..5], "-");
    }
}
