//! The Navigation pane (View ▸ Navigation Pane): search the document, and browse it by
//! headings (collapsible, with the current section highlighted), page thumbnails, or search
//! results in context.

use crate::app::WordApp;
use crate::ribbon::{c_card, c_ink, c_ink2, c_ink3, c_line, ACCENT};
use egui::{vec2, Color32, RichText, Sense, TextureHandle, Ui};
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum NavTab { #[default] Headings, Pages, Results }

#[derive(Default)]
pub struct NavState {
    pub tab: NavTab,
    pub query: String,
    pub collapsed: HashSet<u64>,
    /// Page thumbnails for one document revision.
    pub thumbs: Vec<Option<TextureHandle>>,
    pub thumbs_rev: u64,
}

fn thumbnail(app: &WordApp, ctx: &egui::Context, page: usize) -> Option<TextureHandle> {
    let svg = word_engine::svg::page_svg(&app.doc, &app.layout, page);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let s = tree.size();
    let scale = 320.0 / s.width().max(1.0);
    let (w, h) = ((s.width() * scale) as u32, (s.height() * scale) as u32);
    let mut pm = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    pm.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
    Some(ctx.load_texture(format!("nav-thumb-{page}"), egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], pm.data()), egui::TextureOptions::LINEAR))
}

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let side = if crate::i18n::rtl() { egui::Panel::right("navigation") } else { egui::Panel::left("navigation") };
    side.resizable(true).default_size(280.0).min_size(200.0)
        .frame(egui::Frame::NONE.fill(c_card()).inner_margin(egui::Margin { left: 14, right: 10, top: 10, bottom: 8 }).stroke(egui::Stroke::new(1.0, c_line())))
        .show(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                ui.label(RichText::new(crate::i18n::tr("Navigation")).size(16.0).color(c_ink()).family(egui::FontFamily::Name("ui-bold".into())));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| if crate::ribbon::close_button(ui, "Close").clicked() { app.show_outline = false; });
            });
            ui.add_space(6.0);
            let nav = &mut app.extras.nav;
            let r = ui.add(egui::TextEdit::singleline(&mut nav.query).hint_text(crate::i18n::tr("Search document")).desired_width(f32::INFINITY));
            if r.has_focus() { app.canvas_focus = false; }
            if r.changed() { nav.tab = if nav.query.trim().is_empty() { NavTab::Headings } else { NavTab::Results }; }
            ui.add_space(6.0);
            crate::theme::hrow(ui, |ui| {
                for (t, n) in [(NavTab::Headings, "Headings"), (NavTab::Pages, "Pages"), (NavTab::Results, "Results")] {
                    let on = nav.tab == t;
                    let r = ui.add(egui::Button::new(RichText::new(crate::i18n::tr(n)).color(if on { ACCENT } else { c_ink2() })).frame(false));
                    if on { let rr = r.rect; ui.painter().line_segment([egui::pos2(rr.min.x, rr.max.y + 1.0), egui::pos2(rr.max.x, rr.max.y + 1.0)], egui::Stroke::new(2.0, ACCENT)); }
                    if r.clicked() { nav.tab = t; }
                }
            });
            ui.separator();
            match app.extras.nav.tab {
                NavTab::Headings => headings(app, ui),
                NavTab::Pages => pages(app, ui),
                NavTab::Results => results(app, ui),
            }
        });
}

fn headings(app: &mut WordApp, ui: &mut Ui) {
    let entries = app.doc.outline();
    if entries.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(crate::i18n::tr("Create an interactive outline of your document.")).color(c_ink2()));
        ui.label(RichText::new(crate::i18n::tr("It's a great way to keep track of where you are or quickly move your content around. To get started, go to the Home tab and apply Heading styles to the headings in your document.")).small().color(c_ink3()));
        return;
    }
    let caret = app.doc.locate(app.doc.range().0).0;
    // The heading whose section holds the caret.
    let current = entries.iter().rev().find(|e| e.0 <= caret).map(|e| e.0);
    let mut hide_below: Option<u8> = None;
    let mut go = None;
    let mut toggle = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (k, (index, level, text)) in entries.iter().enumerate() {
            if let Some(h) = hide_below { if *level > h { continue; } hide_below = None; }
            let id = app.doc.paragraphs[*index].id;
            let collapsed = app.extras.nav.collapsed.contains(&id);
            if collapsed { hide_below = Some(*level); }
            let has_children = entries.get(k + 1).is_some_and(|n| n.1 > *level);
            let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
            let on = Some(*index) == current;
            if on { ui.painter().rect_filled(rect, 0.0, ACCENT.gamma_multiply(0.14)); ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, ACCENT); }
            else if r.hovered() { ui.painter().rect_filled(rect, 0.0, c_line().gamma_multiply(0.6)); }
            let x = rect.min.x + 6.0 + *level as f32 * 14.0;
            if has_children {
                let tri = egui::Rect::from_center_size(egui::pos2(x + 5.0, rect.center().y), vec2(14.0, 14.0));
                ui.painter().text(tri.center(), egui::Align2::CENTER_CENTER, if collapsed { "▸" } else { "▾" }, egui::FontId::proportional(12.0), c_ink3());
                if r.clicked() && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| tri.expand(3.0).contains(p)) { toggle = Some(id); continue; }
            }
            let label: String = if text.chars().count() > 60 { format!("{}…", text.chars().take(59).collect::<String>()) } else { text.clone() };
            let font = if *level == 0 { egui::FontId::new(13.0, egui::FontFamily::Name("ui-bold".into())) } else { egui::FontId::proportional(13.0) };
            ui.painter().with_clip_rect(rect).text(egui::pos2(x + 16.0, rect.center().y), egui::Align2::LEFT_CENTER, label, font, if on { ACCENT } else { c_ink() });
            if r.clicked() { go = Some(*index); }
        }
    });
    if let Some(id) = toggle { if !app.extras.nav.collapsed.remove(&id) { app.extras.nav.collapsed.insert(id); } }
    if let Some(i) = go { let s = app.doc.paragraph_start(i); app.doc.select(s, s); app.scroll_to_caret = true; app.canvas_focus = true; }
}

fn pages(app: &mut WordApp, ui: &mut Ui) {
    let n = app.layout.pages.len();
    if app.extras.nav.thumbs_rev != app.doc.revision || app.extras.nav.thumbs.len() != n {
        app.extras.nav.thumbs = vec![None; n];
        app.extras.nav.thumbs_rev = app.doc.revision;
    }
    let current = app.layout.page_of(app.doc.range().0);
    let mut go = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
        let w = (ui.available_width() - 40.0).clamp(80.0, 200.0);
        let origin = ui.min_rect().min.to_vec2();
        let mut made = 0;
        for k in 0..n {
            let p = &app.layout.pages[k];
            let h = w * p.height / p.width.max(1.0);
            ui.vertical_centered(|ui| {
                let (rect, r) = ui.allocate_exact_size(vec2(w, h), Sense::click());
                // Thumbnails are drawn for the pages in view, once per revision.
                let visible = rect.translate(-origin).intersects(viewport.expand(300.0));
                if visible && app.extras.nav.thumbs[k].is_none() {
                    if made < 2 { app.extras.nav.thumbs[k] = thumbnail(app, ui.ctx(), k); made += 1; } else { ui.ctx().request_repaint(); }
                }
                ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                if let Some(t) = &app.extras.nav.thumbs[k] { ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE); }
                ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(if k == current { 3.0 } else { 1.0 }, if k == current { ACCENT } else { c_line() }), egui::StrokeKind::Outside);
                if r.clicked() { go = Some(k); }
                ui.label(RichText::new((k + 1).to_string()).small().color(c_ink3()));
            });
            ui.add_space(6.0);
        }
    });
    if let Some(k) = go {
        if let Some(l) = app.layout.lines.iter().find(|l| l.page == k) { let s = l.start; app.doc.select(s, s); app.scroll_to_caret = true; app.canvas_focus = true; }
    }
}

fn results(app: &mut WordApp, ui: &mut Ui) {
    let q = app.extras.nav.query.trim().to_string();
    if q.is_empty() { ui.label(RichText::new(crate::i18n::tr("Type in the search box to find text in the document.")).color(c_ink3())); return; }
    let hits = app.doc.find_all(&q, false, false);
    ui.label(RichText::new(crate::i18n::trf(if hits.len() == 1 { "{} result" } else { "{} results" }, &[&hits.len()])).color(c_ink2()));
    let text: Vec<char> = app.doc.text().chars().collect();
    let mut go = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for &(a, b) in hits.iter().take(500) {
            // The match in its line of text.
            let s = (a.saturating_sub(30)..a).rev().find(|&k| text.get(k) == Some(&'\n')).map(|k| k + 1).unwrap_or(a.saturating_sub(30));
            let e = (b..(b + 50).min(text.len())).find(|&k| text[k] == '\n').unwrap_or((b + 50).min(text.len()));
            let before = crate::i18n::clean(&text[s..a].iter().collect::<String>());
            let hit = crate::i18n::clean(&text[a..b.min(text.len())].iter().collect::<String>());
            let after = crate::i18n::clean(&text[b.min(text.len())..e].iter().collect::<String>());
            let mut job = egui::text::LayoutJob::default();
            let fmt = |c: Color32, bold: bool| egui::TextFormat { font_id: if bold { egui::FontId::new(13.0, egui::FontFamily::Name("ui-bold".into())) } else { egui::FontId::proportional(13.0) }, color: c, ..Default::default() };
            job.append(&before, 0.0, fmt(c_ink2(), false));
            job.append(&hit, 0.0, egui::TextFormat { background: Color32::from_rgba_unmultiplied(255, 220, 0, 140), ..fmt(c_ink(), true) });
            job.append(&after, 0.0, fmt(c_ink2(), false));
            job.wrap.max_width = ui.available_width() - 8.0;
            let r = ui.add(egui::Label::new(job).sense(Sense::click()));
            if r.clicked() { go = Some((a, b)); }
            ui.separator();
        }
    });
    if let Some((a, b)) = go { app.doc.select(a, b); app.scroll_to_caret = true; }
}
