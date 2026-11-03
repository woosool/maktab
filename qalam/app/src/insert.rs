//! Insert tab features (views/insert-tab.js): cover pages, online pictures, shapes, SmartArt,
//! charts, screenshots, add-ins (Wikipedia, Dictionary), online video, bookmarks and
//! cross-references. Network and capture work runs on background threads.

use crate::app::WordApp;
use egui::{vec2, Color32, ComboBox, DragValue, Key, RichText, Sense, TextureHandle, Ui};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use word_engine::editing::{CoverDesign, COVER_DESIGNS};
use word_engine::objects::*;
use word_engine::Color;

const USER_AGENT: &str = "WordRust/0.1 (desktop word processor)";

/// A result produced on a background thread.
pub struct Job<T>(Arc<Mutex<Option<Result<T, String>>>>);

impl<T: Send + 'static> Job<T> {
    pub fn spawn(ctx: &egui::Context, f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Job<T> {
        let slot = Arc::new(Mutex::new(None));
        let (out, ctx) = (slot.clone(), ctx.clone());
        std::thread::spawn(move || { let r = f(); *out.lock().unwrap() = Some(r); ctx.request_repaint(); });
        Job(slot)
    }
    pub fn take(&self) -> Option<Result<T, String>> { self.0.lock().unwrap().take() }
}

fn agent() -> ureq::Agent { ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(20))).build().into() }

pub fn get_text(url: &str) -> Result<String, String> {
    agent().get(url).header("User-Agent", USER_AGENT).call().map_err(|e| e.to_string())?.body_mut().read_to_string().map_err(|e| e.to_string())
}
pub fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    agent().get(url).header("User-Agent", USER_AGENT).call().map_err(|e| e.to_string())?.body_mut().with_config().limit(40 << 20).read_to_vec().map_err(|e| e.to_string())
}
pub fn get_json(url: &str) -> Result<serde_json::Value, String> { serde_json::from_str(&get_text(url)?).map_err(|e| e.to_string()) }

pub fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub fn texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.thumbnail(320, 320).to_rgba8();
    Some(ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR))
}

// ---------------------------------------------------------------------------
// State

#[derive(Clone)]
pub struct PictureHit { pub title: String, pub thumb: String, pub url: String, pub creator: String, pub license: String, pub landing: String }

pub struct OnlinePictures { pub query: String, pub url: String, pub search: Option<Job<Vec<PictureHit>>>, pub results: Vec<(PictureHit, Option<TextureHandle>)>, pub thumbs: Vec<Option<Job<Vec<u8>>>>, pub download: Option<(Job<Vec<u8>>, PictureHit)>, pub status: String }
pub struct WikiPane { pub query: String, pub lang: String, pub search: Option<Job<Vec<(String, String, String)>>>, pub results: Vec<(String, String, String)>, pub summary: Option<Job<(String, String, String)>>, pub article: Option<(String, String, String)>, pub status: String }
pub struct DictPane { pub word: String, pub job: Option<Job<Vec<(String, String)>>>, pub senses: Vec<(String, String)>, pub status: String }
pub struct VideoDialog { pub url: String, pub job: Option<Job<(String, Vec<u8>)>>, pub info: Option<(String, Vec<u8>, Option<TextureHandle>)>, pub status: String }
pub struct ScreenshotDialog { pub job: Option<Job<Vec<(String, image::RgbaImage)>>>, pub shots: Vec<(String, image::RgbaImage, TextureHandle)>, pub status: String }
pub struct CoverDraft { pub design: CoverDesign, pub title: String, pub subtitle: String, pub author: String, pub date: String, pub accent: Color }
pub struct ChartEdit { pub pos: Option<usize>, pub chart: Chart }
pub struct DiagramEdit { pub pos: Option<usize>, pub diagram: Diagram, pub text: String }
pub struct ShapeEdit { pub pos: usize, pub shape: Shape }
pub struct BookmarkDraft { pub name: String, pub hidden: bool }
pub struct XrefDraft { pub headings: bool, pub page: bool, pub selected: Option<usize> }

#[derive(Default)]
pub struct InsertState {
    pub online: Option<OnlinePictures>,
    pub wiki: Option<WikiPane>,
    pub dict: Option<DictPane>,
    pub video: Option<VideoDialog>,
    pub screenshot: Option<ScreenshotDialog>,
    pub cover: Option<CoverDraft>,
    pub chart: Option<ChartEdit>,
    pub diagram: Option<DiagramEdit>,
    pub shape: Option<ShapeEdit>,
    pub bookmark: Option<BookmarkDraft>,
    pub xref: Option<XrefDraft>,
    pub addins: bool,
    pub crop: Option<crate::picture::CropDraft>,
    pub alt: Option<(usize, String)>,
    pub layout: Option<crate::picture::LayoutDraft>,
}

// ---------------------------------------------------------------------------
// Commands

impl WordApp {
    pub fn open_online_pictures(&mut self) {
        let query = self.doc.selected_text();
        self.insert.online = Some(OnlinePictures { query: query.chars().take(60).collect(), url: String::new(), search: None, results: vec![], thumbs: vec![], download: None, status: "Search openly licensed images (Openverse), or paste an image address.".into() });
    }
    pub fn open_wikipedia(&mut self) {
        let query = self.doc.selected_text();
        let rtl = self.doc.paragraph_rtl(self.doc.current_paragraph());
        self.insert.wiki = Some(WikiPane { query: query.trim().chars().take(80).collect(), lang: if rtl { "ar".into() } else { "en".into() }, search: None, results: vec![], summary: None, article: None, status: String::new() });
    }
    pub fn open_dictionary(&mut self) {
        let (a, b) = if self.doc.selection.is_caret() { self.doc.word_at(self.doc.selection.focus) } else { self.doc.range() };
        self.insert.dict = Some(DictPane { word: self.doc.text_between(a, b).trim().to_string(), job: None, senses: vec![], status: String::new() });
    }
    pub fn open_cover_page(&mut self, design: CoverDesign) {
        let title = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Document Title".into());
        let date = crate::app::now_iso()[..10].to_string();
        self.insert.cover = Some(CoverDraft { design, title, subtitle: "Document subtitle".into(), author: self.author.clone(), date, accent: PALETTE[0] });
    }
    pub fn insert_shape(&mut self, kind: ShapeKind) { self.doc.insert_object(Object::Shape(Shape::new(kind))); self.canvas_focus = true; }
    pub fn open_chart(&mut self, kind: ChartKind) { self.insert.chart = Some(ChartEdit { pos: None, chart: Chart::sample(kind) }); }
    pub fn open_diagram(&mut self, kind: DiagramKind) {
        let d = Diagram::sample(kind);
        self.insert.diagram = Some(DiagramEdit { pos: None, text: d.text(), diagram: d });
    }
    pub fn open_screenshot(&mut self, ctx: &egui::Context) {
        let job = Job::spawn(ctx, || {
            let mut out = vec![];
            for m in xcap::Monitor::all().map_err(|e| e.to_string())? {
                if let Ok(img) = m.capture_image() { out.push((crate::i18n::trf("Screen: {}", &[&m.name().unwrap_or_default()]), img)); }
            }
            if let Ok(ws) = xcap::Window::all() {
                for w in ws.into_iter().filter(|w| !w.is_minimized().unwrap_or(true)).take(12) {
                    let title = w.title().unwrap_or_default();
                    if title.trim().is_empty() { continue; }
                    if let Ok(img) = w.capture_image() { if img.width() > 40 && img.height() > 40 { out.push((format!("{} — {}", w.app_name().unwrap_or_default(), title), img)); } }
                }
            }
            if out.is_empty() { Err("No screens or windows could be captured. On macOS, allow screen recording for this app in System Settings → Privacy & Security.".into()) } else { Ok(out) }
        });
        self.insert.screenshot = Some(ScreenshotDialog { job: Some(job), shots: vec![], status: "Capturing…".into() });
    }
    /// Double-click (or the contextual tab) edits a shape, chart or diagram.
    pub fn edit_object(&mut self, pos: usize) {
        match self.doc.object_at(pos).cloned() {
            Some(Object::Shape(shape)) => { self.doc.select(pos, pos + 1); self.insert.shape = Some(ShapeEdit { pos, shape }); }
            Some(Object::Chart(chart)) => self.insert.chart = Some(ChartEdit { pos: Some(pos), chart }),
            Some(Object::Diagram(d)) => self.insert.diagram = Some(DiagramEdit { pos: Some(pos), text: d.text(), diagram: d }),
            Some(Object::Equation(_)) => { self.doc.select(pos, pos + 1); self.open_equation(String::new()); }
            Some(Object::Ink(_)) => { self.doc.select(pos, pos + 1); self.tab = crate::ribbon::Tab::Draw; }
            None => {}
        }
    }
    /// The selected object, if the selection is exactly one.
    pub fn selected_object(&self) -> Option<(usize, Object)> {
        let (s, e) = self.doc.range();
        if e != s + 1 { return None; }
        self.doc.object_at(s).cloned().map(|o| (s, o))
    }
    /// Insert a downloaded picture with an attribution caption (online pictures, video thumbnails).
    fn insert_picture_bytes(&mut self, bytes: Vec<u8>, link: Option<String>, caption: Option<(String, String)>) -> Result<(), String> {
        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        let (bytes, ct) = match image::guess_format(&bytes) {
            Ok(image::ImageFormat::Png) => (bytes, "image/png"),
            Ok(image::ImageFormat::Jpeg) => (bytes, "image/jpeg"),
            Ok(image::ImageFormat::Gif) => (bytes, "image/gif"),
            _ => { let mut out = std::io::Cursor::new(vec![]); img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?; (out.into_inner(), "image/png") }
        };
        let start = self.doc.range().0;
        self.doc.insert_image(bytes, ct, img.width() as f32, img.height() as f32);
        if let Some(url) = link { self.doc.select(start, start + 1); self.doc.set_link(Some(url)); let e = start + 1; self.doc.select(e, e); }
        if let Some((text, url)) = caption {
            self.doc.insert_paragraph();
            let s = self.doc.range().0;
            self.doc.replace(&text);
            let e = self.doc.range().0;
            self.doc.select(s, e);
            self.doc.set_run_props(word_engine::RunProps { size: Some(8.0), size_cs: Some(8.0), italic: Some(true), ..Default::default() });
            if !url.is_empty() { self.doc.set_link(Some(url)); }
            self.doc.select(e, e);
            self.doc.insert_paragraph();
        }
        self.scroll_to_caret = true;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Panes

pub fn panes(app: &mut WordApp, ui: &mut Ui) {
    if app.insert.online.is_some() { online_pictures(app, ui); }
    if app.insert.wiki.is_some() { wikipedia(app, ui); }
    if app.insert.dict.is_some() { dictionary(app, ui); }
}

fn pane_header(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    ui.add_space(6.0);
    crate::theme::hrow(ui, |ui| {
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if crate::ribbon::close_button(ui, "Close").clicked() { close = true; } });
    });
    ui.separator();
    close
}

fn online_pictures(app: &mut WordApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut close = false;
    let mut insert: Option<PictureHit> = None;
    let mut from_url = false;
    egui::Panel::right("online-pictures").resizable(true).default_size(330.0).show(ui, |ui| {
        close = pane_header(ui, "Online Pictures");
        let st = app.insert.online.as_mut().unwrap();
        let r = ui.add(egui::TextEdit::singleline(&mut st.query).hint_text(crate::i18n::tr("Search images")).desired_width(f32::INFINITY));
        if r.has_focus() { app.canvas_focus = false; }
        let go = ui.button(crate::i18n::tr("Search")).clicked() || r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if go && !st.query.trim().is_empty() {
            let q = encode(st.query.trim());
            st.status = "Searching…".into();
            st.search = Some(Job::spawn(&ctx, move || {
                let v = get_json(&format!("https://api.openverse.org/v1/images/?q={q}&page_size=18&mature=false"))?;
                Ok(v["results"].as_array().cloned().unwrap_or_default().into_iter().map(|r| PictureHit {
                    title: r["title"].as_str().unwrap_or("Untitled").into(), thumb: r["thumbnail"].as_str().unwrap_or_default().into(), url: r["url"].as_str().unwrap_or_default().into(),
                    creator: r["creator"].as_str().unwrap_or("Unknown Author").into(),
                    license: format!("CC {} {}", r["license"].as_str().unwrap_or("").to_uppercase(), r["license_version"].as_str().unwrap_or("")).trim().into(),
                    landing: r["foreign_landing_url"].as_str().unwrap_or_default().into(),
                }).collect())
            }));
        }
        if let Some(res) = st.search.as_ref().and_then(|j| j.take()) {
            st.search = None;
            match res {
                Ok(hits) => {
                    st.status = crate::i18n::trf("{} pictures — click one to insert. Licenses are shown with each picture.", &[&hits.len()]);
                    st.thumbs = hits.iter().map(|h| { let u = h.thumb.clone(); (!u.is_empty()).then(|| Job::spawn(&ctx, move || get_bytes(&u))) }).collect();
                    st.results = hits.into_iter().map(|h| (h, None)).collect();
                }
                Err(e) => st.status = format!("Search failed: {e}"),
            }
        }
        for (i, slot) in st.thumbs.iter_mut().enumerate() {
            if let Some(Ok(bytes)) = slot.as_ref().and_then(|j| j.take()) { st.results[i].1 = texture(&ctx, &format!("ov-{i}"), &bytes); *slot = None; }
        }
        ui.weak(&st.status);
        egui::ScrollArea::vertical().max_height(ui.available_height() - 90.0).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (hit, tex) in &st.results {
                    let (rect, r) = ui.allocate_exact_size(vec2(96.0, 96.0), Sense::click());
                    ui.painter().rect_filled(rect, 3.0, Color32::from_gray(235));
                    if let Some(t) = tex {
                        let s = t.size_vec2();
                        let k = (96.0 / s.x).min(96.0 / s.y);
                        let img = egui::Rect::from_center_size(rect.center(), s * k);
                        ui.painter().image(t.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    if r.hovered() { ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(2.0, crate::ribbon::ACCENT), egui::StrokeKind::Inside); }
                    if r.on_hover_text(format!("{}\n{} — {}", hit.title, hit.creator, hit.license)).clicked() { insert = Some(hit.clone()); }
                }
            });
        });
        ui.separator();
        ui.label(crate::i18n::tr("Or insert from an address"));
        crate::theme::hrow(ui, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut st.url).hint_text(crate::i18n::tr("https://…/picture.jpg")).desired_width(220.0));
            if r.has_focus() { app.canvas_focus = false; }
            if ui.button(crate::i18n::tr("Insert")).clicked() { from_url = true; }
        });
        if let Some((job, hit)) = &st.download { if let Some(res) = job.take() {
            let hit = hit.clone();
            st.download = None;
            match res {
                Ok(bytes) => {
                    let caption = (!hit.license.is_empty()).then(|| (crate::i18n::trf("This photo by {} is licensed under {}", &[&hit.creator, &hit.license]), hit.landing.clone()));
                    if let Err(e) = app.insert_picture_bytes(bytes, None, caption) { app.insert.online.as_mut().unwrap().status = format!("Couldn't insert: {e}"); }
                    else { app.insert.online.as_mut().unwrap().status = "Inserted.".into(); }
                }
                Err(e) => app.insert.online.as_mut().unwrap().status = format!("Download failed: {e}"),
            }
        } }
    });
    let Some(st) = app.insert.online.as_mut() else { return };
    if let Some(hit) = insert {
        st.status = crate::i18n::trf("Downloading {}…", &[&hit.title]);
        let url = if hit.url.is_empty() { hit.thumb.clone() } else { hit.url.clone() };
        let thumb = hit.thumb.clone();
        st.download = Some((Job::spawn(&ctx, move || get_bytes(&url).or_else(|_| get_bytes(&thumb))), hit));
    }
    if from_url && !st.url.trim().is_empty() {
        let url = st.url.trim().to_string();
        st.status = "Downloading…".into();
        st.download = Some((Job::spawn(&ctx, move || get_bytes(&url)), PictureHit { title: String::new(), thumb: String::new(), url: String::new(), creator: String::new(), license: String::new(), landing: String::new() }));
    }
    if close { app.insert.online = None; app.canvas_focus = true; }
}

fn wikipedia(app: &mut WordApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut close = false;
    let mut insert: Option<(String, String, String, bool)> = None;
    egui::Panel::right("wikipedia").resizable(true).default_size(340.0).show(ui, |ui| {
        close = pane_header(ui, "Wikipedia");
        let st = app.insert.wiki.as_mut().unwrap();
        crate::theme::hrow(ui, |ui| {
            crate::theme::Combo::from_id_salt("wiki-lang").width(60.0).selected_text(crate::i18n::tr(&st.lang.clone())).show_ui(ui, |ui| {
                for (l, n) in [("en", "English"), ("ar", "العربية"), ("fr", "Français"), ("es", "Español"), ("de", "Deutsch"), ("he", "עברית")] { ui.selectable_value(&mut st.lang, l.to_string(), crate::i18n::tr(&n)); }
            });
            let r = ui.add(egui::TextEdit::singleline(&mut st.query).hint_text(crate::i18n::tr("Search Wikipedia")).desired_width(f32::INFINITY));
            if r.has_focus() { app.canvas_focus = false; }
            if (ui.input(|i| i.key_pressed(Key::Enter)) && r.lost_focus()) && !st.query.trim().is_empty() {
                let (q, lang) = (encode(st.query.trim()), st.lang.clone());
                st.status = "Searching…".into();
                st.search = Some(Job::spawn(&ctx, move || {
                    let v = get_json(&format!("https://{lang}.wikipedia.org/w/rest.php/v1/search/page?q={q}&limit=10"))?;
                    Ok(v["pages"].as_array().cloned().unwrap_or_default().into_iter().map(|p| (p["title"].as_str().unwrap_or("").to_string(), p["description"].as_str().unwrap_or("").to_string(), p["key"].as_str().unwrap_or("").to_string())).collect())
                }));
            }
        });
        if ui.button(crate::i18n::tr("Search")).clicked() && !st.query.trim().is_empty() {
            let (q, lang) = (encode(st.query.trim()), st.lang.clone());
            st.search = Some(Job::spawn(&ctx, move || {
                let v = get_json(&format!("https://{lang}.wikipedia.org/w/rest.php/v1/search/page?q={q}&limit=10"))?;
                Ok(v["pages"].as_array().cloned().unwrap_or_default().into_iter().map(|p| (p["title"].as_str().unwrap_or("").to_string(), p["description"].as_str().unwrap_or("").to_string(), p["key"].as_str().unwrap_or("").to_string())).collect())
            }));
        }
        if let Some(r) = st.search.as_ref().and_then(|j| j.take()) {
            st.search = None;
            match r { Ok(v) => { st.status = if v.is_empty() { "No articles found.".into() } else { String::new() }; st.results = v; } Err(e) => st.status = format!("Search failed: {e}") }
        }
        if let Some(r) = st.summary.as_ref().and_then(|j| j.take()) {
            st.summary = None;
            match r { Ok(a) => st.article = Some(a), Err(e) => st.status = format!("Couldn't load the article: {e}") }
        }
        if !st.status.is_empty() { ui.weak(&st.status); }
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some((title, extract, url)) = st.article.clone() {
                ui.label(RichText::new(&title).strong().size(15.0));
                ui.label(crate::i18n::tr(&extract));
                ui.hyperlink_to("Open on Wikipedia", &url);
                crate::theme::hrow(ui, |ui| {
                    if ui.button(crate::i18n::tr("Insert text and citation")).clicked() { insert = Some((title.clone(), extract.clone(), url.clone(), true)); }
                    if ui.button(crate::i18n::tr("Insert link")).clicked() { insert = Some((title.clone(), extract.clone(), url.clone(), false)); }
                    if ui.button(crate::i18n::tr("Back")).clicked() { st.article = None; }
                });
                ui.weak(crate::i18n::tr("Text from Wikipedia is available under CC BY-SA 4.0."));
            } else {
                for (title, desc, key) in st.results.clone() {
                    let r = ui.add(egui::Button::new(RichText::new(&title).strong()).frame(false));
                    if !desc.is_empty() { ui.weak(&desc); }
                    if r.clicked() {
                        let lang = st.lang.clone();
                        st.summary = Some(Job::spawn(&ctx, move || {
                            let v = get_json(&format!("https://{lang}.wikipedia.org/api/rest_v1/page/summary/{}", encode(&key)))?;
                            Ok((v["title"].as_str().unwrap_or(&title).to_string(), v["extract"].as_str().unwrap_or("").to_string(), v["content_urls"]["desktop"]["page"].as_str().unwrap_or("").to_string()))
                        }));
                    }
                    ui.add_space(4.0);
                }
            }
        });
    });
    if let Some((title, extract, url, with_text)) = insert {
        if with_text {
            app.doc.replace(&extract);
            app.doc.insert_paragraph();
        }
        let s = app.doc.range().0;
        let label = if with_text { crate::i18n::trf("Source: Wikipedia — {}", &[&title]) } else { title };
        app.doc.replace(&label);
        let e = app.doc.range().0;
        app.doc.select(s, e);
        app.doc.set_link(Some(url));
        app.doc.select(e, e);
        app.canvas_focus = true;
        app.scroll_to_caret = true;
    }
    if close { app.insert.wiki = None; app.canvas_focus = true; }
}

fn dictionary(app: &mut WordApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut close = false;
    let mut insert: Option<String> = None;
    egui::Panel::right("dictionary").resizable(true).default_size(300.0).show(ui, |ui| {
        close = pane_header(ui, "Dictionary");
        let st = app.insert.dict.as_mut().unwrap();
        let r = ui.add(egui::TextEdit::singleline(&mut st.word).hint_text(crate::i18n::tr("Look up a word")).desired_width(f32::INFINITY));
        if r.has_focus() { app.canvas_focus = false; }
        if (ui.button(crate::i18n::tr("Look up")).clicked() || r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) && !st.word.trim().is_empty() {
            let w = encode(&st.word.trim().to_lowercase());
            st.status = "Looking up…".into();
            st.job = Some(Job::spawn(&ctx, move || {
                let v = get_json(&format!("https://api.dictionaryapi.dev/api/v2/entries/en/{w}"))?;
                let mut out = vec![];
                for entry in v.as_array().cloned().unwrap_or_default() {
                    for m in entry["meanings"].as_array().cloned().unwrap_or_default() {
                        for d in m["definitions"].as_array().cloned().unwrap_or_default().into_iter().take(3) {
                            out.push((m["partOfSpeech"].as_str().unwrap_or("").to_string(), d["definition"].as_str().unwrap_or("").to_string()));
                        }
                    }
                }
                if out.is_empty() { Err("No definitions found.".into()) } else { Ok(out) }
            }));
        }
        if let Some(r) = st.job.as_ref().and_then(|j| j.take()) { st.job = None; match r { Ok(v) => { st.senses = v; st.status = String::new(); } Err(e) => { st.senses.clear(); st.status = e; } } }
        if !st.status.is_empty() { ui.weak(&st.status); }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (pos, def) in &st.senses {
                ui.horizontal_wrapped(|ui| { ui.label(RichText::new(crate::i18n::tr(&pos)).italics().color(crate::ribbon::c_ink3())); ui.label(crate::i18n::tr(&def)); });
                if ui.small_button(crate::i18n::tr("Insert")).clicked() { insert = Some(format!("{} ({pos}): {def}", st.word.trim())); }
                ui.add_space(4.0);
            }
        });
        ui.weak(crate::i18n::tr("Definitions from the Free Dictionary API (dictionaryapi.dev)."));
    });
    if let Some(t) = insert { app.doc.replace(&t); app.canvas_focus = true; }
    if close { app.insert.dict = None; app.canvas_focus = true; }
}

// ---------------------------------------------------------------------------
// Dialogs

pub fn dialogs(app: &mut WordApp, ctx: &egui::Context) {
    cover_dialog(app, ctx);
    video_dialog(app, ctx);
    screenshot_dialog(app, ctx);
    chart_dialog(app, ctx);
    diagram_dialog(app, ctx);
    shape_dialog(app, ctx);
    bookmark_dialog(app, ctx);
    xref_dialog(app, ctx);
    addins_dialog(app, ctx);
    crate::picture::dialogs(app, ctx);
}

fn cover_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.insert.cover.take() else { return };
    let mut open = true;
    let mut insert = false;
    crate::theme::dialog(ctx, "Cover Page").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hrow(ui, |ui| {
            for (design, name) in COVER_DESIGNS {
                let (rect, r) = ui.allocate_exact_size(vec2(70.0, 92.0), Sense::click());
                cover_thumbnail(ui, rect, *design, d.accent, d.design == *design);
                ui.painter().text(rect.center_bottom() + vec2(0.0, -8.0), egui::Align2::CENTER_CENTER, *name, egui::FontId::proportional(10.0), crate::ribbon::c_ink3());
                if r.clicked() { d.design = *design; }
            }
        });
        crate::theme::grid(ui, egui::Grid::new("cover").num_columns(2), |ui| {
            ui.label(crate::i18n::tr("Title")); ui.text_edit_singleline(&mut d.title); ui.end_row();
            ui.label(crate::i18n::tr("Subtitle")); ui.text_edit_singleline(&mut d.subtitle); ui.end_row();
            ui.label(crate::i18n::tr("Author")); ui.text_edit_singleline(&mut d.author); ui.end_row();
            ui.label(crate::i18n::tr("Date")); ui.text_edit_singleline(&mut d.date); ui.end_row();
            ui.label(crate::i18n::tr("Color"));
            crate::ribbon::swatch_choice(ui, &PALETTE, &mut d.accent);
            ui.end_row();
        });
        if ui.button(crate::i18n::tr("Insert Cover Page")).clicked() { insert = true; }
    });
    if insert {
        app.doc.insert_cover_page(d.design, &d.title, &d.subtitle, &d.author, &d.date, d.accent);
        app.canvas_focus = true;
    } else if open { app.insert.cover = Some(d); } else { app.canvas_focus = true; }
}

/// A miniature of a cover design (gallery tile).
pub fn cover_thumbnail(ui: &Ui, rect: egui::Rect, design: CoverDesign, accent: Color, on: bool) {
    let p = ui.painter();
    let page = rect.shrink2(vec2(6.0, 12.0)).translate(vec2(0.0, -6.0));
    p.rect(page, 2.0, Color32::WHITE, egui::Stroke::new(if on { 2.0 } else { 1.0 }, if on { crate::ribbon::ACCENT } else { crate::ribbon::c_line() }), egui::StrokeKind::Inside);
    let a = Color32::from_rgb(accent.0, accent.1, accent.2);
    let x0 = page.min.x + 6.0;
    let y = page.min.y + page.height() * 0.35;
    match design {
        CoverDesign::Banded => { p.rect_filled(egui::Rect::from_min_max(egui::pos2(page.min.x, y - 6.0), egui::pos2(page.max.x, y + 10.0)), 0.0, a); }
        CoverDesign::Simple => { p.line_segment([egui::pos2(x0, y + 8.0), egui::pos2(page.max.x - 6.0, y + 8.0)], egui::Stroke::new(1.0, a)); p.rect_filled(egui::Rect::from_min_size(egui::pos2(x0, y), vec2(30.0, 5.0)), 0.0, a); }
        CoverDesign::Sideline => { p.rect_filled(egui::Rect::from_min_size(egui::pos2(x0, y - 4.0), vec2(2.0, 16.0)), 0.0, a); p.rect_filled(egui::Rect::from_min_size(egui::pos2(x0 + 5.0, y), vec2(28.0, 5.0)), 0.0, Color32::from_gray(60)); }
        CoverDesign::Bold => { p.rect_filled(egui::Rect::from_center_size(egui::pos2(page.center().x, y + 2.0), vec2(36.0, 7.0)), 0.0, Color32::from_gray(40)); p.line_segment([egui::pos2(page.center().x - 18.0, y + 10.0), egui::pos2(page.center().x + 18.0, y + 10.0)], egui::Stroke::new(1.5, a)); }
    }
    p.rect_filled(egui::Rect::from_min_size(egui::pos2(x0, page.max.y - 14.0), vec2(20.0, 3.0)), 0.0, Color32::from_gray(150));
}

fn video_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.insert.video.take() else { return };
    let mut open = true;
    let mut insert = false;
    crate::theme::dialog(ctx, "Online Video").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        app.canvas_focus = false;
        ui.label(crate::i18n::tr("Paste the address of a YouTube or Vimeo video:"));
        let r = ui.add(egui::TextEdit::singleline(&mut d.url).hint_text(crate::i18n::tr("https://www.youtube.com/watch?v=…")).desired_width(360.0));
        if (ui.button(crate::i18n::tr("Find video")).clicked() || r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) && !d.url.trim().is_empty() {
            let url = d.url.trim().to_string();
            d.status = "Looking up the video…".into();
            d.job = Some(Job::spawn(ctx, move || {
                let api = if url.contains("vimeo.com") { format!("https://vimeo.com/api/oembed.json?url={}", encode(&url)) } else { format!("https://www.youtube.com/oembed?url={}&format=json", encode(&url)) };
                let v = get_json(&api)?;
                let title = v["title"].as_str().unwrap_or("Video").to_string();
                let thumb = v["thumbnail_url"].as_str().ok_or("This address has no video preview.")?.to_string();
                Ok((title, get_bytes(&thumb)?))
            }));
        }
        if let Some(r) = d.job.as_ref().and_then(|j| j.take()) {
            d.job = None;
            match r { Ok((t, b)) => { let tex = texture(ctx, "video-thumb", &b); d.info = Some((t, b, tex)); d.status = String::new(); } Err(e) => d.status = format!("Couldn't find that video: {e}") }
        }
        if !d.status.is_empty() { ui.weak(&d.status); }
        if let Some((title, _, tex)) = &d.info {
            if let Some(t) = tex { ui.image((t.id(), t.size_vec2() * (240.0 / t.size_vec2().x))); }
            ui.label(RichText::new(title).strong());
            crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Insert").clicked() { insert = true; });
        }
    });
    if insert {
        if let Some((title, bytes, _)) = d.info.clone() {
            let url = d.url.trim().to_string();
            let _ = app.insert_picture_bytes(bytes, Some(url.clone()), Some((format!("▶ {title}"), url)));
        }
        app.canvas_focus = true;
    } else if open { app.insert.video = Some(d); } else { app.canvas_focus = true; }
}

fn screenshot_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.insert.screenshot.take() else { return };
    let mut open = true;
    let mut chosen: Option<usize> = None;
    if let Some(r) = d.job.as_ref().and_then(|j| j.take()) {
        d.job = None;
        match r {
            Ok(shots) => {
                d.shots = shots.into_iter().enumerate().filter_map(|(i, (t, img))| {
                    let thumb = image::DynamicImage::ImageRgba8(img.clone()).thumbnail(240, 160).to_rgba8();
                    let tex = ctx.load_texture(format!("shot-{i}"), egui::ColorImage::from_rgba_unmultiplied([thumb.width() as usize, thumb.height() as usize], thumb.as_raw()), egui::TextureOptions::LINEAR);
                    Some((t, img, tex))
                }).collect();
                d.status = "Available windows and screens — click one to insert it.".into();
            }
            Err(e) => d.status = e,
        }
    }
    crate::theme::dialog(ctx, "Screenshot").open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
        ui.weak(&d.status);
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, (title, _, tex)) in d.shots.iter().enumerate() {
                    crate::theme::vcol(ui, |ui| {
                        let r = ui.add(egui::Button::image(egui::Image::new((tex.id(), tex.size_vec2()))));
                        ui.label(RichText::new(title.chars().take(36).collect::<String>()).size(10.5));
                        if r.clicked() { chosen = Some(i); }
                    });
                }
            });
        });
    });
    if let Some(i) = chosen {
        let img = &d.shots[i].1;
        let mut out = std::io::Cursor::new(vec![]);
        if image::DynamicImage::ImageRgba8(img.clone()).write_to(&mut out, image::ImageFormat::Png).is_ok() {
            app.doc.insert_image(out.into_inner(), "image/png", img.width() as f32 / 2.0, img.height() as f32 / 2.0);
        }
        app.canvas_focus = true;
    } else if open { app.insert.screenshot = Some(d); }
}

fn chart_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.insert.chart.take() else { return };
    let mut open = true;
    let mut done = false;
    crate::theme::dialog(ctx, if d.pos.is_some() { "Edit Chart Data" } else { "Insert Chart" }).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hrow(ui, |ui| {
            ui.label(crate::i18n::tr("Type"));
            for (k, n) in CHARTS { ui.selectable_value(&mut d.chart.kind, *k, crate::i18n::tr(&*n)); }
        });
        crate::theme::hrow(ui, |ui| { ui.label(crate::i18n::tr("Title")); ui.text_edit_singleline(&mut d.chart.title); crate::theme::check(ui, &mut d.chart.legend, crate::i18n::tr("Legend")); });
        ui.separator();
        // Data sheet: categories down, series across.
        let c = &mut d.chart;
        egui::ScrollArea::both().max_height(190.0).show(ui, |ui| {
            crate::theme::grid(ui, egui::Grid::new("chart-data").striped(true), |ui| {
                ui.label("");
                for s in c.series.iter_mut() { ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(80.0)); }
                ui.end_row();
                for row in 0..c.categories.len() {
                    ui.add(egui::TextEdit::singleline(&mut c.categories[row]).desired_width(90.0));
                    for s in c.series.iter_mut() {
                        while s.values.len() < row + 1 { s.values.push(0.0); }
                        ui.add(DragValue::new(&mut s.values[row]).speed(0.1).max_decimals(2));
                    }
                    ui.end_row();
                }
            });
        });
        crate::theme::hrow(ui, |ui| {
            if ui.button(crate::i18n::tr("+ Row")).clicked() { c.categories.push(crate::i18n::trf("Category {}", &[&(c.categories.len() + 1)])); }
            if ui.button(crate::i18n::tr("− Row")).clicked() && c.categories.len() > 1 { c.categories.pop(); for s in &mut c.series { s.values.truncate(c.categories.len()); } }
            if c.kind != ChartKind::Pie {
                if ui.button(crate::i18n::tr("+ Series")).clicked() { c.series.push(Series { name: crate::i18n::trf("Series {}", &[&(c.series.len() + 1)]), values: vec![1.0; c.categories.len()], color: None }); }
                if ui.button(crate::i18n::tr("− Series")).clicked() && c.series.len() > 1 { c.series.pop(); }
            }
        });
        ui.separator();
        let (rect, _) = ui.allocate_exact_size(vec2(400.0, 240.0), Sense::hover());
        let mut preview = c.clone();
        preview.width = 480.0; preview.height = 288.0;
        crate::canvas::paint_preview(app, ui, &render_chart(&preview, 0.0, 0.0), rect.min, 400.0 / 480.0);
        if ui.button(if d.pos.is_some() { "Update Chart" } else { "Insert Chart" }).clicked() { done = true; }
    });
    if done {
        d.chart.raw = None;
        match d.pos { Some(p) => app.doc.set_object(p, Object::Chart(d.chart)), None => app.doc.insert_object(Object::Chart(d.chart)) }
        app.canvas_focus = true;
    } else if open { app.insert.chart = Some(d); } else { app.canvas_focus = true; }
}

fn diagram_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.insert.diagram.take() else { return };
    let mut open = true;
    let mut done = false;
    crate::theme::dialog(ctx, "SmartArt").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hrow(ui, |ui| { for (k, _, n) in DIAGRAMS { ui.selectable_value(&mut d.diagram.kind, *k, crate::i18n::tr(&*n)); } });
        crate::theme::hrow(ui, |ui| {
            ui.label(crate::i18n::tr("Color"));
            crate::ribbon::swatch_choice(ui, &PALETTE, &mut d.diagram.color);
        });
        ui.columns(2, |cols| {
            cols[0].label("Text (one item per line; indent with Tab or two spaces for sub-items)");
            if cols[0].add(egui::TextEdit::multiline(&mut d.text).desired_rows(10).desired_width(220.0).code_editor()).changed() { d.diagram.items = Diagram::items_from_text(&d.text); }
            let (rect, _) = cols[1].allocate_exact_size(vec2(300.0, 170.0), Sense::hover());
            cols[1].painter().rect_filled(rect, 2.0, Color32::WHITE);
            let mut preview = d.diagram.clone();
            preview.width = 480.0; preview.height = 272.0;
            crate::canvas::paint_preview(app, &cols[1], &render_diagram(&preview, 0.0, 0.0), rect.min, 300.0 / 480.0);
        });
        if ui.button(if d.pos.is_some() { "Update" } else { "Insert" }).clicked() { done = true; }
    });
    if done {
        match d.pos { Some(p) => app.doc.set_object(p, Object::Diagram(d.diagram)), None => app.doc.insert_object(Object::Diagram(d.diagram)) }
        app.canvas_focus = true;
    } else if open { app.insert.diagram = Some(d); } else { app.canvas_focus = true; }
}

fn shape_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.insert.shape.take() else { return };
    let mut open = true;
    let mut apply = false;
    crate::theme::dialog(ctx, "Format Shape").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        shape_controls(ui, &mut d.shape, 1);
        ui.label(crate::i18n::tr("Text"));
        ui.add(egui::TextEdit::multiline(&mut d.shape.text).desired_rows(2).desired_width(260.0));
        crate::theme::footer(ui, |ui| if crate::theme::primary(ui, "Apply").clicked() { apply = true; });
    });
    if apply {
        d.shape.raw = None;
        app.doc.set_object(d.pos, Object::Shape(d.shape));
        app.canvas_focus = true;
    } else if open { app.insert.shape = Some(d); } else { app.canvas_focus = true; }
}

/// Fill, outline, weight, size and shape type (shared by the dialog and the Shape Format tab).
/// Shape fill, outline, text color and size. `pairs` label/control pairs go on each grid row
/// (1 in dialogs, 3 in the compact ribbon).
pub fn shape_controls(ui: &mut Ui, s: &mut Shape, pairs: usize) -> bool {
    let mut n = 0;
    let mut row = |ui: &mut Ui| { n += 1; if n % pairs == 0 { ui.end_row(); } };
    let mut changed = false;
    crate::theme::grid(ui, egui::Grid::new(ui.next_auto_id()).num_columns(2 * pairs), |ui| {
        ui.label(crate::i18n::tr("Shape"));
        crate::theme::Combo::from_id_salt("shape-kind").selected_text(crate::i18n::tr(&(SHAPES.iter().find(|x| x.0 == s.kind).map(|x| x.2).unwrap_or("Shape")))).show_ui(ui, |ui| {
            for (k, _, n) in SHAPES { changed |= ui.selectable_value(&mut s.kind, *k, crate::i18n::tr(&*n)).changed(); }
        });
        row(ui);
        ui.label(crate::i18n::tr("Fill"));
        crate::theme::hrow(ui, |ui| {
            let mut on = s.fill.is_some();
            if crate::theme::check(ui, &mut on, "").changed() { s.fill = on.then_some(PALETTE[0]); changed = true; }
            if let Some(c) = &mut s.fill { changed |= crate::ribbon::color_button(ui, c); }
        });
        row(ui);
        ui.label(crate::i18n::tr("Outline"));
        crate::theme::hrow(ui, |ui| {
            let mut on = s.outline.is_some();
            if crate::theme::check(ui, &mut on, "").changed() { s.outline = on.then_some(Color::BLACK); changed = true; }
            if let Some(c) = &mut s.outline { changed |= crate::ribbon::color_button(ui, c); }
            changed |= ui.add(DragValue::new(&mut s.outline_width).range(0.25..=12.0).speed(0.05).suffix(" pt")).changed();
        });
        row(ui);
        ui.label(crate::i18n::tr("Text color"));
        changed |= crate::ribbon::color_button(ui, &mut s.text_color);
        row(ui);
        ui.label(crate::i18n::tr("Size"));
        crate::theme::hrow(ui, |ui| {
            let mut w = s.width / 96.0;
            let mut h = s.height / 96.0;
            if ui.add(DragValue::new(&mut w).range(0.05..=20.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { s.width = w * 96.0; changed = true; }
            ui.label("×");
            if ui.add(DragValue::new(&mut h).range(0.0..=20.0).speed(0.01).max_decimals(2).suffix("\"")).changed() { s.height = h * 96.0; changed = true; }
        });
        row(ui);
    });
    if changed { s.raw = None; }
    changed
}

fn bookmark_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.insert.bookmark.take() else { return };
    let mut open = true;
    let mut close = false;
    crate::theme::dialog(ctx, "Bookmark").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        ui.label(crate::i18n::tr("Bookmark name"));
        ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(260.0));
        let valid = word_engine::Document::valid_bookmark_name(&d.name);
        if !d.name.is_empty() && !valid { ui.colored_label(Color32::from_rgb(190, 40, 40), "Start with a letter; use letters, numbers and underscores."); }
        let marks: Vec<(String, usize, usize)> = app.doc.bookmarks().into_iter().filter(|b| d.hidden || !b.0.starts_with('_')).collect();
        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
            for (name, ..) in &marks { if ui.selectable_label(d.name == *name, crate::i18n::tr(&name)).clicked() { d.name = name.clone(); } }
            if marks.is_empty() { ui.weak(crate::i18n::tr("No bookmarks yet.")); }
        });
        crate::theme::check(ui, &mut d.hidden, crate::i18n::tr("Hidden bookmarks"));
        crate::theme::hrow(ui, |ui| {
            if ui.add_enabled(valid, egui::Button::new(crate::i18n::tr("Add"))).clicked() {
                match app.doc.add_bookmark(&d.name) { Ok(()) => close = true, Err(e) => app.dialogs.error = Some(e) }
            }
            let exists = marks.iter().any(|b| b.0 == d.name);
            if ui.add_enabled(exists, egui::Button::new(crate::i18n::tr("Delete"))).clicked() { app.doc.delete_bookmark(&d.name); }
            if ui.add_enabled(exists, egui::Button::new(crate::i18n::tr("Go To"))).clicked() {
                if let Some((_, s, e)) = marks.iter().find(|b| b.0 == d.name) { app.doc.select(*s, *e); app.scroll_to_caret = true; }
            }
        });
    });
    if open && !close { app.insert.bookmark = Some(d); } else { app.canvas_focus = true; }
}

fn xref_dialog(app: &mut WordApp, ctx: &egui::Context) {
    let Some(mut d) = app.insert.xref.take() else { return };
    let mut open = true;
    let mut insert = false;
    crate::theme::dialog(ctx, "Cross-reference").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        app.canvas_focus = false;
        crate::theme::hrow(ui, |ui| {
            ui.label(crate::i18n::tr("Reference type"));
            if ui.selectable_label(d.headings, crate::i18n::tr("Heading")).clicked() { d.headings = true; d.selected = None; }
            if ui.selectable_label(!d.headings, crate::i18n::tr("Bookmark")).clicked() { d.headings = false; d.selected = None; }
        });
        crate::theme::hrow(ui, |ui| {
            ui.label(crate::i18n::tr("Insert reference to"));
            crate::theme::radio_value(ui, &mut d.page, false, if d.headings { "Heading text" } else { "Bookmark text" });
            crate::theme::radio_value(ui, &mut d.page, true, crate::i18n::tr("Page number"));
        });
        ui.label(if d.headings { "For which heading:" } else { "For which bookmark:" });
        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            if d.headings {
                for (k, (_, level, text)) in app.doc.outline().into_iter().enumerate() {
                    if ui.selectable_label(d.selected == Some(k), format!("{}{text}", "    ".repeat(level as usize))).clicked() { d.selected = Some(k); }
                }
            } else {
                for (k, (name, ..)) in app.doc.bookmarks().into_iter().filter(|b| !b.0.starts_with('_')).enumerate() {
                    if ui.selectable_label(d.selected == Some(k), crate::i18n::tr(&name)).clicked() { d.selected = Some(k); }
                }
            }
        });
        ui.weak(crate::i18n::tr("Inserted as a field that updates as the document changes; ⌘/Ctrl-click it to jump."));
        if ui.add_enabled(d.selected.is_some(), egui::Button::new(crate::i18n::tr("Insert"))).clicked() { insert = true; }
    });
    if insert {
        let target = if d.headings {
            let idx = app.doc.outline().get(d.selected.unwrap()).map(|o| o.0);
            idx.map(|i| app.doc.heading_bookmark(i))
        } else { app.doc.bookmarks().into_iter().filter(|b| !b.0.starts_with('_')).nth(d.selected.unwrap()).map(|b| b.0) };
        if let Some(name) = target { app.doc.insert_cross_reference(&name, d.page); }
        app.canvas_focus = true;
    } else if open { app.insert.xref = Some(d); } else { app.canvas_focus = true; }
}

fn addins_dialog(app: &mut WordApp, ctx: &egui::Context) {
    if !app.insert.addins { return; }
    let mut open = true;
    crate::theme::dialog(ctx, "Add-ins").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        crate::theme::section(ui, "Built-in add-ins");
        if ui.button(crate::i18n::tr("Wikipedia — search and cite articles")).clicked() { app.open_wikipedia(); app.insert.addins = false; }
        if ui.button(crate::i18n::tr("Dictionary — look up the selected word")).clicked() { app.open_dictionary(); app.insert.addins = false; }
        if ui.button(crate::i18n::tr("Online Pictures — openly licensed images")).clicked() { app.open_online_pictures(); app.insert.addins = false; }
        ui.separator();
        ui.weak(crate::i18n::tr("Office Store add-ins run on Microsoft's web runtime, which this app doesn't include."));
    });
    if !open { app.insert.addins = false; }
}
