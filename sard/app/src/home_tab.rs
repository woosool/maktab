//! The Home tab, complete: Clipboard, Slides, Font, Paragraph, Drawing, Editing, Voice and
//! Designer groups, as PowerPoint arranges them.

use crate::app::{Pane, SlidesApp};
use crate::dialogs::{Dialog, FontDialog, ParaDialog};
use crate::i18n::tr;
use egui::{vec2, Color32, RichText, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::edit::{Order, RunFormat};
use slides_engine::format::Effect;
use slides_engine::scene::Loc;

pub fn hex(c: word_engine::Color) -> String { format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2) }

pub fn home(app: &mut SlidesApp, ui: &mut Ui) {
    let scene = app.scene();
    let run = scene.as_ref().and_then(|s| app.current_run(s));
    let has_sel = !app.selection.is_empty() || app.edit.is_some();
    let shapes = !app.selection.is_empty();
    // ---- Clipboard
    let clip = group_l(ui, "Clipboard", "Clipboard options", |ui| {
        let (main, arrow) = large_split(ui, "paste", "Paste", "Paste (⌘V)");
        if main.clicked() { app.paste(); }
        crate::theme::menu(&arrow, |ui| {
            menu_heading(ui, "Paste Options:");
            if menu_item(ui, "paste", "Use Destination Theme") { app.paste_as("dest"); ui.close(); }
            if menu_item(ui, "formatPainter", "Keep Source Formatting") { app.paste_as("keep"); ui.close(); }
            if menu_item(ui, "picture", "Picture") { app.paste_as("picture"); ui.close(); }
            if menu_item(ui, "textBox", "Keep Text Only") { app.paste_as("text"); ui.close(); }
        });
        rows(ui, |ui| {
            if small(ui, "cut", Some("Cut"), false, has_sel, "Cut (⌘X)").clicked() { app.cut(); }
            if small(ui, "copy", Some("Copy"), false, has_sel, "Copy (⌘C)").clicked() { app.copy(); }
            let fp = small(ui, "formatPainter", Some("Format Painter"), app.painter.is_some(), has_sel || app.painter.is_some(), "Format Painter: click to copy formatting once, double-click to keep it on");
            if fp.double_clicked() { app.pick_format(true); } else if fp.clicked() { if app.painter.is_some() { app.painter = None; } else { app.pick_format(false); } }
        });
    });
    if clip { app.notify(&tr("Copy with ⌘C, then paste with ⌘V, or use Paste Options.")); }
    // ---- Slides
    group(ui, "Slides", |ui| {
        let (main, arrow) = large_split(ui, "newSlide", "New Slide", "Add a slide (⌘M)");
        if main.clicked() { app.new_slide(None); }
        crate::theme::menu(&arrow, |ui| {
            if let Some(l) = crate::nav::layout_gallery(app, ui) { app.new_slide(Some(l)); ui.close(); }
            ui.separator();
            if menu_item(ui, "duplicate", "Duplicate Selected Slides") { app.duplicate_slides(); ui.close(); }
            if menu_item(ui, "outlineView", "Slides from Outline…") { slides_from_outline(app); ui.close(); }
            if menu_item(ui, "seeAllSlides", "Reuse Slides…") { app.pane = Pane::Reuse; ui.close(); }
        });
        if large(ui, "seeAllSlides", "Reuse Slides", app.pane == Pane::Reuse, true, "Insert slides from another presentation").clicked() { app.pane = if app.pane == Pane::Reuse { Pane::None } else { Pane::Reuse }; }
        rows(ui, |ui| {
            let layout = small_menu(ui, "layout", "Layout", "Change the slide's layout");
            crate::theme::menu(&layout, |ui| if let Some(l) = crate::nav::layout_gallery(app, ui) { app.set_layout(&l); ui.close(); });
            if small(ui, "resetSlide", Some("Reset"), false, true, "Reset the position, size and formatting of the slide placeholders").clicked() { reset_slide(app); }
            let sec = small_menu(ui, "section", "Section", "Organize slides into sections");
            crate::theme::menu(&sec, |ui| section_menu(app, ui));
        });
    });
    // ---- Font
    let font_launch = group_l(ui, "Font", "Font dialog (⌘T)", |ui| {
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                font_boxes(app, ui, run.as_ref());
                if small(ui, "fontGrow", None, false, has_sel, "Increase Font Size (⇧⌘>)").clicked() { let s = run.as_ref().map(|r| r.size).unwrap_or(18.0); app.format(RunFormat { size: Some(grow(s, true)), ..Default::default() }); }
                if small(ui, "fontShrink", None, false, has_sel, "Decrease Font Size (⇧⌘<)").clicked() { let s = run.as_ref().map(|r| r.size).unwrap_or(18.0); app.format(RunFormat { size: Some(grow(s, false)), ..Default::default() }); }
                if small(ui, "clearFormat", None, false, has_sel, "Clear All Formatting").clicked() {
                    app.format(RunFormat { bold: Some(false), italic: Some(false), underline: Some(false), strike: Some(false), color: Some(String::new()), baseline: Some(0), highlight: Some(None), ..Default::default() });
                    app.text_cmd(|p, l, c, r| { p.set_char_spacing(l, c, r, 0.0); p.set_caps(l, c, r, "none"); p.set_text_effect(l, c, r, &Effect::None); p.set_text_outline(l, c, r, None, 0.0); });
                }
            });
            crate::theme::hrow(ui, |ui| {
                let r = run.clone().unwrap_or_default();
                if small(ui, "bold", None, r.bold, has_sel, "Bold (⌘B)").clicked() { app.format(RunFormat { bold: Some(!r.bold), ..Default::default() }); }
                if small(ui, "italic", None, r.italic, has_sel, "Italic (⌘I)").clicked() { app.format(RunFormat { italic: Some(!r.italic), ..Default::default() }); }
                if small(ui, "underline", None, r.underline, has_sel, "Underline (⌘U)").clicked() { app.format(RunFormat { underline: Some(!r.underline), ..Default::default() }); }
                let (_, uarrow) = small_split(ui, "underline", false, "Underline style");
                crate::theme::menu(&uarrow, |ui| for (k, n) in crate::dialogs::UNDERLINES.iter().skip(1) { if ui.button(tr(n)).clicked() { let k = k.to_string(); app.text_cmd(move |p, l, c, rr| p.set_underline_style(l, c, rr, &k)); ui.close(); } });
                if small(ui, "strike", None, r.strike, has_sel, "Strikethrough").clicked() { app.format(RunFormat { strike: Some(!r.strike), ..Default::default() }); }
                let shadowed = r.shadow.is_some();
                if small(ui, "shadowEffect", None, shadowed, has_sel, "Text Shadow").clicked() {
                    let e = if shadowed { Effect::None } else { Effect::Shadow { dir: 45.0, dist: 1.5, blur: 2.0, color: "000000".into(), alpha: 45 } };
                    app.text_cmd(move |p, l, c, rr| p.set_text_effect(l, c, rr, &e));
                }
                let cs = small_menu(ui, "charSpacing", "", "Character Spacing");
                crate::theme::menu(&cs, |ui| {
                    for (pts, n) in [(-3.0, "Very Tight"), (-1.5, "Tight"), (0.0, "Normal"), (3.0, "Loose"), (6.0, "Very Loose")] { if ui.button(tr(n)).clicked() { app.text_cmd(move |p, l, c, rr| p.set_char_spacing(l, c, rr, pts)); ui.close(); } }
                    ui.separator();
                    if ui.button(tr("More Spacing…")).clicked() { open_font_dialog(app, run.as_ref()); ui.close(); }
                });
                let cc = small_menu(ui, "changeCase", "", "Change Case");
                crate::theme::menu(&cc, |ui| for (k, n) in [("sentence", "Sentence case."), ("lower", "lowercase"), ("upper", "UPPERCASE"), ("title", "Capitalize Each Word"), ("toggle", "tOGGLE cASE")] { if ui.button(n).clicked() { app.text_cmd(move |p, l, c, rr| p.change_case(l, c, rr, k)); ui.close(); } });
                let h = r.highlight.unwrap_or(slides_engine::Rgba::rgb(255, 255, 0));
                let (hmain, harrow) = color_split(ui, "highlight", Color32::from_rgb(h.r, h.g, h.b), "Text Highlight Color");
                if hmain.clicked() { let c = h.to_hex(); app.text_cmd(move |p, l, cc, rr| p.set_highlight(l, cc, rr, Some(&c))); }
                crate::theme::menu(&harrow, |ui| if let Some(pick) = color_menu(ui, "No Color") { let v = pick.map(hex); app.text_cmd(move |p, l, c, rr| p.set_highlight(l, c, rr, v.as_deref())); ui.close(); });
                let c = r.color;
                let (main, arrow) = color_split(ui, "textColor", Color32::from_rgb(c.r, c.g, c.b), "Font Color");
                if main.clicked() { app.format(RunFormat { color: Some("C00000".into()), ..Default::default() }); }
                crate::theme::menu(&arrow, |ui| {
                    if let Some(pick) = color_menu(ui, "Automatic") { app.format(RunFormat { color: Some(pick.map(hex).unwrap_or_default()), ..Default::default() }); ui.close(); }
                    crate::theme::submenu(ui, tr("Gradient"), |ui| for (a, b, n) in [("FFFFFF", "4472C4", "Blue"), ("FFD966", "C00000", "Sunset"), ("A9D18E", "375623", "Forest"), ("F2F2F2", "404040", "Silver")] { if ui.button(tr(n)).clicked() { app.text_cmd(move |p, l, c, rr| p.set_text_fill(l, c, rr, a, Some(b))); ui.close(); } });
                });
            });
        });
    });
    if font_launch { open_font_dialog(app, run.as_ref()); }
    // ---- Paragraph
    let para_launch = group_l(ui, "Paragraph", "Paragraph dialog", |ui| {
        rows(ui, |ui| {
            crate::theme::hrow(ui, |ui| {
                if small(ui, "bullets", None, false, has_sel, "Bullets").clicked() { app.paragraphs(|p, l, c, r| p.set_bullets(l, c, r, "char:•")); }
                let (_, ba) = small_split(ui, "bullets", false, "Bullet Library");
                crate::theme::menu(&ba, |ui| {
                    if ui.button(tr("None")).clicked() { app.paragraphs(|p, l, c, r| p.set_bullets(l, c, r, "none")); ui.close(); }
                    ui.horizontal_wrapped(|ui| { ui.set_max_width(200.0); for ch in crate::dialogs::BULLET_CHARS.chars() { if ui.add(egui::Button::new(RichText::new(ch.to_string()).size(18.0)).min_size(vec2(36.0, 36.0))).clicked() { let k = format!("char:{ch}"); app.paragraphs(move |p, l, c, r| p.set_bullets(l, c, r, &k)); ui.close(); } } });
                    ui.separator();
                    if ui.button(tr("Bullets and Numbering…")).clicked() { app.dialog = Some(Dialog::Bullets { numbered: false, kind: "char:•".into(), size: 100.0, color: word_engine::Color(0, 0, 0), start: 1 }); ui.close(); }
                });
                if small(ui, "numbering", None, false, has_sel, "Numbering").clicked() { app.paragraphs(|p, l, c, r| p.set_bullets(l, c, r, "arabicPeriod")); }
                let (_, na) = small_split(ui, "numbering", false, "Numbering Library");
                crate::theme::menu(&na, |ui| {
                    if ui.button(tr("None")).clicked() { app.paragraphs(|p, l, c, r| p.set_bullets(l, c, r, "none")); ui.close(); }
                    for (k, n) in crate::dialogs::NUMBERS { if ui.button(*n).clicked() { app.paragraphs(move |p, l, c, r| p.set_bullets(l, c, r, k)); ui.close(); } }
                    ui.separator();
                    if ui.button(tr("Bullets and Numbering…")).clicked() { app.dialog = Some(Dialog::Bullets { numbered: true, kind: "arabicPeriod".into(), size: 100.0, color: word_engine::Color(0, 0, 0), start: 1 }); ui.close(); }
                });
                if small(ui, "indentLess", None, false, has_sel, "Decrease List Level").clicked() { app.paragraphs(|p, l, c, r| p.set_level(l, c, r, -1)); }
                if small(ui, "indentMore", None, false, has_sel, "Increase List Level").clicked() { app.paragraphs(|p, l, c, r| p.set_level(l, c, r, 1)); }
                let ls = small_menu(ui, "lineSpacing", "", "Line Spacing");
                crate::theme::menu(&ls, |ui| {
                    for m in [1.0, 1.5, 2.0, 2.5, 3.0] { if ui.button(format!("{m:.1}")).clicked() { app.paragraphs(move |p, l, c, r| p.set_line_spacing(l, c, r, m)); ui.close(); } }
                    ui.separator();
                    if ui.button(tr("Line Spacing Options…")).clicked() { open_para_dialog(app); ui.close(); }
                });
                let cols = small_menu(ui, "columns", "", "Add or Remove Columns");
                crate::theme::menu(&cols, |ui| {
                    for (n, label) in [(1u32, "One Column"), (2, "Two Columns"), (3, "Three Columns")] { if ui.button(tr(label)).clicked() { app.shapes_cmd(move |p, l| p.set_columns(l, n, 7.2)); ui.close(); } }
                    ui.separator();
                    if ui.button(tr("More Columns…")).clicked() { app.dialog = Some(Dialog::Columns { count: 2, gap: 7.2 }); ui.close(); }
                });
            });
            crate::theme::hrow(ui, |ui| {
                for (icon, algn, tip) in [("alignLeft", "l", "Align Left (⌘L)"), ("alignCenter", "ctr", "Center (⌘E)"), ("alignRight", "r", "Align Right (⌘R)"), ("justify", "just", "Justify (⌘J)")] {
                    if small(ui, icon, None, false, has_sel, tip).clicked() { app.paragraphs(move |p, l, c, r| p.set_align(l, c, r, algn)); }
                }
                let td = small_menu(ui, "textDirection", "", "Text Direction");
                crate::theme::menu(&td, |ui| {
                    for (d, n) in crate::panes::TEXT_DIRS { if ui.button(tr(n)).clicked() { app.shapes_cmd(move |p, l| p.set_text_direction(l, d)); ui.close(); } }
                    ui.separator();
                    if ui.button(tr("More Options…")).clicked() { app.pane = Pane::Format; ui.close(); }
                });
                let at = small_menu(ui, "alignText", "", "Align Text");
                crate::theme::menu(&at, |ui| {
                    for (icon, a, n) in [("valignTop", "t", "Top"), ("valignMiddle", "ctr", "Middle"), ("valignBottom", "b", "Bottom")] { if menu_item(ui, icon, n) { app.shapes_cmd(move |p, l| p.set_anchor(l, a)); ui.close(); } }
                    ui.separator();
                    if ui.button(tr("More Options…")).clicked() { app.pane = Pane::Format; ui.close(); }
                });
                let sa = small_menu(ui, "smartArtConvert", "", "Convert to SmartArt Graphic");
                crate::theme::menu(&sa, |ui| {
                    ui.set_width(3.0 * 100.0);
                    ui.horizontal_wrapped(|ui| for id in ["verticalBulletList", "basicBlockList", "basicProcess", "chevronProcess", "basicCycle", "orgChart", "basicVenn", "basicPyramid", "basicTimeline"] {
                        let name = slides_engine::diagram::LAYOUTS.iter().find(|l| l.0 == id).map(|l| l.1).unwrap_or(id);
                        if ui.add_sized(vec2(96.0, 28.0), egui::Button::new(egui::RichText::new(tr(name)).size(11.0))).clicked() { convert_to_smartart(app, id); ui.close(); }
                    });
                    ui.separator();
                    if ui.button(tr("More SmartArt Graphics…")).clicked() {
                        let loc = app.edit.as_ref().map(|e| e.loc.clone()).or_else(|| app.selection.first().cloned());
                        let text = loc.as_ref().map(|l| app.pres.shape_text(l, None)).unwrap_or_default();
                        crate::smartart_ui::open_smartart(app, &text);
                        if let Some(ed) = app.diagram_editor.as_mut() { ed.replace = loc; }
                        ui.close();
                    }
                });
                if small(ui, "textDirection", None, false, has_sel, "Right-to-Left Text Direction").clicked() { app.paragraphs(|p, l, c, r| { p.set_paragraph(l, c, r, None, None, None, None, Some(true)); p.set_align(l, c, r, "r"); }); }
            });
        });
    });
    if para_launch { open_para_dialog(app); }
    // ---- Drawing
    let draw_launch = group_l(ui, "Drawing", "Format Shape pane", |ui| {
        let gallery = large_menu(ui, "shapes", "Shapes", "Insert a shape");
        crate::theme::menu(&gallery, |ui| crate::ribbon::shape_gallery(app, ui));
        let arrange = large_menu(ui, "arrange", "Arrange", "Order, group and position objects");
        crate::theme::menu(&arrange, |ui| arrange_menu(app, ui));
        let qs = large_menu(ui, "quickStyles", "Quick Styles", "Choose a visual style for the shape");
        crate::theme::menu(&qs, |ui| quick_styles(app, ui));
        rows(ui, |ui| {
            fill_menu(app, ui, shapes);
            outline_menu(app, ui, shapes);
            effects_menu(app, ui, shapes);
        });
    });
    if draw_launch { app.pane = Pane::Format; }
    // ---- Editing
    group(ui, "Editing", |ui| {
        rows(ui, |ui| {
            if small(ui, "find", Some("Find"), false, true, "Find (⌘F)").clicked() { app.dialog = Some(Dialog::Find { query: String::new(), with: String::new(), match_case: false, whole: false, replace: false, last: None }); }
            let rp = small_menu(ui, "replace", "Replace", "Replace text or fonts");
            crate::theme::menu(&rp, |ui| {
                if menu_item(ui, "replace", "Replace…") { app.dialog = Some(Dialog::Find { query: String::new(), with: String::new(), match_case: false, whole: false, replace: true, last: None }); ui.close(); }
                if menu_item(ui, "fonts", "Replace Fonts…") { app.dialog = Some(Dialog::ReplaceFonts { from: String::new(), to: "Calibri".into() }); ui.close(); }
            });
            let sel = small_menu(ui, "select", "Select", "Select text or objects");
            crate::theme::menu(&sel, |ui| {
                if menu_item(ui, "selectAll", "Select All") { select_all(app); ui.close(); }
                if menu_item(ui, "select", "Select Objects") { app.edit = None; app.insert_mode = None; ui.close(); }
                if menu_item(ui, "selectionPane", "Selection Pane…") { app.pane = Pane::Selection; ui.close(); }
            });
        });
    });
    // ---- Voice
    group(ui, "Voice", |ui| {
        if large(ui, "dictate", "Dictate", false, true, "Dictate with the system's speech recognition").clicked() { dictate(app); }
    });
    group(ui, "Language", |ui| {
        let tm = large_menu(ui, "translate", "Translate", "Translate offline (the models download once)");
        crate::theme::menu(&tm, |ui| {
            if menu_item(ui, "translate", "Translate Selection…") {
                let text = app.edit.as_ref().map(|e| { let t = app.pres.shape_text(&e.loc, e.cell); let (a, b) = e.range(); t.chars().skip(a).take(b - a).collect::<String>() }).filter(|t| !t.is_empty())
                    .or_else(|| app.selection.first().map(|l| app.pres.shape_text(l, None))).unwrap_or_default();
                crate::translate::open(app, &text); ui.close();
            }
            crate::theme::submenu(ui, tr("Translate Slide"), |ui| for (c, n) in qalam_kit::translate_ui::LANGUAGES { if ui.button(tr(n)).clicked() { crate::translate::translate_slides(app, "auto", c, false); ui.close(); } });
            crate::theme::submenu(ui, tr("Translate Presentation"), |ui| for (c, n) in qalam_kit::translate_ui::LANGUAGES { if ui.button(tr(n)).clicked() { crate::translate::translate_slides(app, "auto", c, true); ui.close(); } });
            ui.separator();
            if menu_item(ui, "settings", "Translation Models…") { crate::translate::models(app); ui.close(); }
        });
    });
    // ---- Designer
    group(ui, "Designer", |ui| {
        if large(ui, "designer", "Designer", app.pane == Pane::Designer, true, "Design ideas for this slide").clicked() { app.pane = if app.pane == Pane::Designer { Pane::None } else { Pane::Designer }; }
    });
    let _ = Stroke::NONE;
}

/// Font name and size boxes (the size accepts typed values).
pub fn font_boxes(app: &mut SlidesApp, ui: &mut Ui, run: Option<&slides_engine::scene::Run>) {
    let font = run.map(|r| r.font.clone()).unwrap_or_default();
    crate::theme::Combo::from_id_salt("rb-font").width(132.0).height(400.0).selected_text(RichText::new(&font).size(12.5)).show_ui(ui, |ui| {
        menu_heading(ui, "Theme Fonts");
        let (theme, _) = slides_engine::scene::color_context(&app.pres, &app.slide().unwrap_or_default());
        for (f, role) in [(theme.major.latin.clone(), "Headings"), (theme.minor.latin.clone(), "Body")] { if !f.is_empty() && ui.selectable_label(font == f, format!("{f}  ({})", tr(role))).clicked() { app.format(RunFormat { font: Some(f.clone()), ..Default::default() }); } }
        ui.separator();
        menu_heading(ui, "All Fonts");
        for name in word_engine::fonts::family_names() {
            if ui.selectable_label(font.eq_ignore_ascii_case(&name), RichText::new(&name).size(13.0)).clicked() { app.format(RunFormat { font: Some(name.clone()), ..Default::default() }); }
        }
    });
    let size = run.map(|r| r.size).unwrap_or(18.0);
    let id = egui::Id::new("rb-size-text");
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| format!("{size}"));
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(36.0).font(egui::FontId::proportional(12.5)));
    if r.has_focus() { ui.data_mut(|d| d.insert_temp(id, text.clone())); } else { ui.data_mut(|d| d.remove::<String>(id)); }
    if r.lost_focus() { if let Ok(v) = text.trim().parse::<f64>() { if (1.0..=4000.0).contains(&v) { app.format(RunFormat { size: Some(v), ..Default::default() }); } } }
    let (arect, ar) = ui.allocate_exact_size(vec2(14.0, 22.0), Sense::click());
    button_bg(ui, arect, &ar, false);
    caret(ui, arect.center());
    crate::theme::menu(&ar, |ui| for s in [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 44.0, 48.0, 54.0, 60.0, 66.0, 72.0, 80.0, 88.0, 96.0] {
        if ui.selectable_label((size - s).abs() < 0.01, format!("{s}")).clicked() { app.format(RunFormat { size: Some(s), ..Default::default() }); ui.close(); }
    });
}

pub fn grow(size: f64, up: bool) -> f64 {
    const STEPS: [f64; 24] = [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 44.0, 48.0, 54.0, 60.0, 66.0, 72.0, 80.0, 88.0, 96.0];
    if up { STEPS.iter().copied().find(|s| *s > size + 0.01).unwrap_or(size + 8.0) } else { STEPS.iter().rev().copied().find(|s| *s < size - 0.01).unwrap_or((size - 1.0).max(1.0)) }
}

pub fn open_font_dialog(app: &mut SlidesApp, run: Option<&slides_engine::scene::Run>) {
    let r = run.cloned().unwrap_or_default();
    app.dialog = Some(Dialog::Font(FontDialog { font: r.font.clone(), size: r.size.max(1.0), bold: r.bold, italic: r.italic, color: word_engine::Color(r.color.r, r.color.g, r.color.b), underline: if r.underline { if r.underline_style.is_empty() { "sng".into() } else { r.underline_style.clone() } } else { "none".into() }, strike: if r.double_strike { 2 } else if r.strike { 1 } else { 0 }, offset: (r.baseline * 100.0).round() as i32, small_caps: r.small_caps, all_caps: r.caps, spacing: r.spacing }));
}

pub fn open_para_dialog(app: &mut SlidesApp) {
    let para = app.scene().and_then(|s| {
        let loc = app.edit.as_ref().map(|e| e.loc.clone()).or_else(|| app.selection.first().cloned())?;
        let it = SlidesApp::items_of(&s, &loc).into_iter().next()?;
        if let slides_engine::scene::Kind::Shape { text: Some(t), .. } = &it.kind { t.paras.first().cloned() } else { None }
    }).unwrap_or_default();
    let sp = |s: slides_engine::scene::Spacing| match s { slides_engine::scene::Spacing::Points(p) => p, _ => 0.0 };
    let align = match para.align { slides_engine::scene::Align::Center => "ctr", slides_engine::scene::Align::Right => "r", slides_engine::scene::Align::Justify => "just", _ => "l" };
    let line = match para.line { slides_engine::scene::Spacing::Percent(f) => f, _ => 1.0 };
    app.dialog = Some(Dialog::Paragraph(ParaDialog { align: align.into(), before_text: (para.margin + para.indent.min(0.0)) / 12_700.0, hanging: -para.indent / 12_700.0, before: sp(para.before), after: sp(para.after), line, rtl: para.rtl }));
}

fn reset_slide(app: &mut SlidesApp) {
    let Some(s) = app.slide() else { return };
    app.checkpoint("reset");
    let n = app.pres.slide(&s).and_then(|r| r.path("p:cSld/p:spTree")).map(|t| t.elements().count()).unwrap_or(0);
    for i in 0..n {
        let loc = Loc { part: s.clone(), path: vec![i] };
        if let Some(sp) = app.pres.element_mut(&loc) {
            if sp.path("p:nvSpPr/p:nvPr/p:ph").is_some() {
                // Back to the layout's position, size and formatting.
                if let Some(pr) = sp.child_mut("p:spPr") { pr.children.clear(); }
                if let Some(tx) = sp.child_mut("p:txBody") {
                    for p in tx.elements_mut().filter(|e| e.name == "a:p") {
                        p.remove_children("a:pPr");
                        for r in p.elements_mut() { if let Some(rpr) = r.child_mut("a:rPr") { rpr.children.clear(); rpr.attrs.retain(|(k, _)| k == "lang" || k == "dirty"); } }
                    }
                    if let Some(b) = tx.child_mut("a:bodyPr") { b.attrs.clear(); b.children.clear(); }
                }
            }
        }
    }
}

fn section_menu(app: &mut SlidesApp, ui: &mut Ui) {
    let mut sections = app.pres.sections();
    if menu_item(ui, "section", "Add Section") {
        let at = app.current;
        if sections.is_empty() && at > 0 { sections.push((tr("Default Section"), 0)); }
        sections.retain(|s| s.1 != at);
        sections.push((crate::i18n::trf("Section {}", &[&(sections.len() + 1)]), at));
        app.checkpoint("section");
        app.pres.set_sections(sections.clone());
        app.renaming_section = Some((at, sections.last().map(|s| s.0.clone()).unwrap_or_default()));
        ui.close();
    }
    let current = sections.iter().rposition(|s| s.1 <= app.current);
    if let Some(k) = current {
        if menu_item(ui, "rename", "Rename Section") { app.renaming_section = Some((sections[k].1, sections[k].0.clone())); ui.close(); }
        if menu_item(ui, "deleteSlide", "Remove Section") { sections.remove(k); app.checkpoint("section"); app.pres.set_sections(sections.clone()); ui.close(); }
    }
    if !sections.is_empty() && menu_item(ui, "clearFormat", "Remove All Sections") { app.checkpoint("section"); app.pres.set_sections(vec![]); ui.close(); }
    if !sections.is_empty() {
        ui.separator();
        if menu_item(ui, "collapse", "Collapse All") { app.collapsed_sections = sections.iter().map(|s| s.1).collect(); ui.close(); }
        if menu_item(ui, "expand", "Expand All") { app.collapsed_sections.clear(); ui.close(); }
    }
}

fn select_all(app: &mut SlidesApp) {
    if let Some(e) = app.edit.clone() { let n = app.pres.shape_text(&e.loc, e.cell).chars().count(); app.edit = Some(crate::app::TextEdit { anchor: 0, cursor: n, ..e }); return; }
    if let Some(s) = app.scene() { let mut v: Vec<Loc> = vec![]; for it in s.items.iter().filter(|i| i.own) { let l = Loc { part: it.loc.part.clone(), path: vec![it.loc.path[0]] }; if !v.contains(&l) { v.push(l); } } app.selection = v; }
}

/// Arrange: order, group, align, distribute, rotate, selection pane.
pub fn arrange_menu(app: &mut SlidesApp, ui: &mut Ui) {
    menu_heading(ui, "Order Objects");
    for (icon, label, order) in [("bringToFront", "Bring to Front", Order::Front), ("sendToBack", "Send to Back", Order::Back), ("bringForward", "Bring Forward", Order::Forward), ("sendBackward", "Send Backward", Order::Backward)] {
        if menu_item(ui, icon, label) && !app.selection.is_empty() { app.checkpoint("order"); app.selection = app.selection.clone().iter().filter_map(|l| app.pres.reorder(l, order)).collect(); ui.close(); }
    }
    menu_heading(ui, "Group Objects");
    group_items(app, ui);
    let scene = app.scene();
    menu_heading(ui, "Position Objects");
    crate::theme::submenu(ui, tr("Align"), |ui| {
        let (sw, sh) = app.pres.size();
        for (icon, label, k) in [("alignObjLeft", "Align Left", 0), ("alignObjCenter", "Align Center", 1), ("alignObjRight", "Align Right", 2), ("alignObjTop", "Align Top", 3), ("alignObjMiddle", "Align Middle", 4), ("alignObjBottom", "Align Bottom", 5)] {
            if menu_item(ui, icon, label) {
                let Some(scene) = &scene else { continue };
                let frames: Vec<_> = app.selection.iter().filter_map(|l| app.frame_of(scene, l).map(|f| (l.clone(), f))).collect();
                if frames.is_empty() { continue; }
                let (x0, y0, x1, y1) = if frames.len() == 1 { (0.0, 0.0, sw, sh) } else {
                    (frames.iter().map(|f| f.1.x).fold(f64::MAX, f64::min), frames.iter().map(|f| f.1.y).fold(f64::MAX, f64::min), frames.iter().map(|f| f.1.x + f.1.w).fold(f64::MIN, f64::max), frames.iter().map(|f| f.1.y + f.1.h).fold(f64::MIN, f64::max))
                };
                app.checkpoint("align");
                for (l, mut f) in frames { match k { 0 => f.x = x0, 1 => f.x = (x0 + x1 - f.w) / 2.0, 2 => f.x = x1 - f.w, 3 => f.y = y0, 4 => f.y = (y0 + y1 - f.h) / 2.0, _ => f.y = y1 - f.h } app.pres.set_xfrm(&l, &f); }
                ui.close();
            }
        }
        ui.separator();
        for (icon, label, h) in [("distributeH", "Distribute Horizontally", true), ("distributeV", "Distribute Vertically", false)] {
            if menu_item(ui, icon, label) {
                let Some(scene) = &scene else { continue };
                let frames: Vec<_> = app.selection.iter().filter_map(|l| app.frame_of(scene, l).map(|f| (l.clone(), f))).collect();
                let (sw, sh) = app.pres.size();
                app.checkpoint("distribute");
                app.pres.distribute(&frames, h, sw, sh);
                ui.close();
            }
        }
    });
    crate::theme::submenu(ui, tr("Rotate"), |ui| {
        for (icon, label, f) in [("rotate", "Rotate Right 90°", 0), ("rotate", "Rotate Left 90°", 1), ("flipV", "Flip Vertical", 2), ("flipH", "Flip Horizontal", 3)] {
            if menu_item(ui, icon, label) {
                let Some(scene) = &scene else { continue };
                app.checkpoint("rotate");
                for l in app.selection.clone() { if let Some(mut x) = app.frame_of(scene, &l) { match f { 0 => x.rot = (x.rot + 90.0) % 360.0, 1 => x.rot = (x.rot + 270.0) % 360.0, 2 => x.flip_v = !x.flip_v, _ => x.flip_h = !x.flip_h } app.pres.set_xfrm(&l, &x); } }
                ui.close();
            }
        }
    });
    ui.separator();
    if menu_item(ui, "selectionPane", "Selection Pane…") { app.pane = Pane::Selection; ui.close(); }
}

/// Quick Styles: theme colors × intensities.
pub fn quick_styles(app: &mut SlidesApp, ui: &mut Ui) {
    let accents = ["tx1", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6"];
    menu_heading(ui, "Theme Styles");
    for style in 0..6 {
        ui.allocate_ui_with_layout(vec2(7.0 * 34.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (k, a) in accents.iter().enumerate() {
                let base = if k == 0 { slides_engine::Rgba::rgb(0x40, 0x40, 0x40) } else { slides_engine::Rgba::hex(&app.accent_hex(k)).unwrap_or(slides_engine::Rgba::BLACK) };
                let c = Color32::from_rgb(base.r, base.g, base.b);
                let (r, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
                let fill = match style { 0 => Color32::WHITE, 1 => c, 2 => c, 3 => c.gamma_multiply(0.85), _ => c };
                ui.painter().rect(r.shrink(2.0), 3.0, fill, Stroke::new(if style == 0 { 1.5 } else { 1.0 }, if style == 0 { c } else { Color32::from_black_alpha(60) }), egui::StrokeKind::Inside);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Abc", egui::FontId::proportional(9.5), if style == 0 { c } else { Color32::WHITE });
                if resp.hovered() { ui.painter().rect_stroke(r, 3.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside); }
                if resp.clicked() { let a = a.to_string(); app.shapes_cmd(move |p, l| p.set_quick_style(l, &a, style)); ui.close(); }
            }
        });
    }
}

pub fn fill_menu(app: &mut SlidesApp, ui: &mut Ui, on: bool) {
    let fill = small_menu(ui, "shapeFill", "Shape Fill", "Fill the selected shape with a color, gradient, picture or texture");
    if !on { return; }
    crate::theme::menu(&fill, |ui| {
        if let Some(pick) = color_menu(ui, "No Fill") { let v = pick.map(hex).unwrap_or_default(); app.shapes_cmd(move |p, l| p.set_fill(l, Some(&v))); ui.close(); }
        if menu_item(ui, "picture", "Picture…") {
            if let Some(f) = rfd::FileDialog::new().add_filter(&tr("Pictures"), &["png", "jpg", "jpeg", "gif", "bmp"]).pick_file() {
                if let Ok(b) = std::fs::read(&f) { let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into()); app.shapes_cmd(move |p, l| p.set_picture_fill(l, b.clone(), &ext, false)); }
            }
            ui.close();
        }
        crate::theme::submenu(ui, tr("Gradient"), |ui| {
            let base = app.accent_hex(1);
            menu_heading(ui, "Light Variations");
            gradient_row(app, ui, &base, 0..4);
            menu_heading(ui, "Dark Variations");
            gradient_row(app, ui, &base, 4..8);
            ui.separator();
            if ui.button(tr("No Gradient")).clicked() { app.shapes_cmd(|p, l| p.set_fill(l, None)); ui.close(); }
            if ui.button(tr("More Gradients…")).clicked() { app.pane = Pane::Format; ui.close(); }
        });
        crate::theme::submenu(ui, tr("Texture"), |ui| {
            ui.set_width(220.0);
            ui.horizontal_wrapped(|ui| for t in slides_engine::format::TEXTURES {
                let key = format!("tex-{t}");
                let tex = ui.data(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new(&key))).or_else(|| {
                    let img = image::load_from_memory(&slides_engine::format::texture_png(t)).ok()?.to_rgba8();
                    let h = ui.ctx().load_texture(&key, egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR);
                    ui.data_mut(|d| d.insert_temp(egui::Id::new(&key), h.clone()));
                    Some(h)
                });
                let (r, resp) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
                if let Some(h) = tex { ui.painter().image(h.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE); }
                if resp.hovered() { ui.painter().rect_stroke(r, 0.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside); }
                if resp.on_hover_text(tr(t)).clicked() { let png = slides_engine::format::texture_png(t); app.shapes_cmd(move |p, l| p.set_picture_fill(l, png.clone(), "png", true)); ui.close(); }
            });
        });
    });
}

fn gradient_row(app: &mut SlidesApp, ui: &mut Ui, base: &str, range: std::ops::Range<usize>) {
    ui.allocate_ui_with_layout(vec2(4.0 * 40.0, 36.0), egui::Layout::left_to_right(egui::Align::Center), |ui| for v in range {
        let (stops, angle, radial) = slides_engine::format::gradient_variation(base, v);
        let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
        let col = |h: &str| { let c = slides_engine::Rgba::hex(h).unwrap_or(slides_engine::Rgba::BLACK); Color32::from_rgb(c.r, c.g, c.b) };
        let (a, b) = (col(&stops[0].1), col(&stops[stops.len() - 1].1));
        let mut mesh = egui::Mesh::default();
        let (tl, tr_, bl, br) = if (angle - 90.0).abs() < 1.0 { (a, a, b, b) } else if angle.abs() < 1.0 && !radial { (a, b, a, b) } else if radial { (b, b, b, b) } else { (a, a, a, b) };
        mesh.colored_vertex(r.left_top(), tl); mesh.colored_vertex(r.right_top(), tr_); mesh.colored_vertex(r.left_bottom(), bl); mesh.colored_vertex(r.right_bottom(), br);
        mesh.add_triangle(0, 1, 2); mesh.add_triangle(1, 3, 2);
        ui.painter().add(egui::Shape::mesh(mesh));
        if radial { ui.painter().circle_filled(r.center(), 8.0, a); }
        if resp.hovered() { ui.painter().rect_stroke(r, 0.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside); }
        if resp.clicked() { app.shapes_cmd(move |p, l| p.set_gradient_fill(l, &stops, angle, radial)); ui.close(); }
    });
}

pub fn outline_menu(app: &mut SlidesApp, ui: &mut Ui, on: bool) {
    let outline = small_menu(ui, "shapeOutline", "Shape Outline", "Outline color, weight, dashes and arrows");
    if !on { return; }
    crate::theme::menu(&outline, |ui| {
        if let Some(pick) = color_menu(ui, "No Outline") { let v = pick.map(hex).unwrap_or_default(); app.shapes_cmd(move |p, l| p.set_line(l, Some(&v), None, None)); ui.close(); }
        crate::theme::submenu(ui, tr("Weight"), |ui| for w in [0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.0, 4.5, 6.0] { if ui.button(format!("{w} pt")).clicked() { app.shapes_cmd(move |p, l| p.set_line(l, None, Some(w), None)); ui.close(); } });
        crate::theme::submenu(ui, tr("Dashes"), |ui| for (d, n) in [("solid", "Solid"), ("sysDot", "Round Dot"), ("sysDash", "Square Dot"), ("dash", "Dash"), ("dashDot", "Dash Dot"), ("lgDash", "Long Dash"), ("lgDashDot", "Long Dash Dot"), ("lgDashDotDot", "Long Dash Dot Dot")] { if ui.button(tr(n)).clicked() { app.shapes_cmd(move |p, l| p.set_line(l, None, None, Some(d))); ui.close(); } });
        crate::theme::submenu(ui, tr("Arrows"), |ui| for (h, t, n) in [(None, None, "None"), (None, Some("triangle"), "Arrow at end"), (Some("triangle"), None, "Arrow at start"), (Some("triangle"), Some("triangle"), "Arrows at both ends"), (None, Some("oval"), "Round end"), (None, Some("stealth"), "Stealth end")] { if ui.button(tr(n)).clicked() { app.shapes_cmd(move |p, l| p.set_arrows(l, h, t)); ui.close(); } });
    });
}

pub fn effects_menu(app: &mut SlidesApp, ui: &mut Ui, on: bool) {
    let eff = small_menu(ui, "shapeEffects", "Shape Effects", "Shadow, reflection, glow, soft edges, bevel and 3-D rotation");
    if !on { return; }
    let (a1, a2, a4) = (app.accent_hex(1), app.accent_hex(2), app.accent_hex(4));
    crate::theme::menu(&eff, |ui| {
        let mut row = |ui: &mut Ui, title: &str, list: Vec<(&str, Effect)>| { crate::theme::submenu(ui, tr(title), |ui| for (n, e) in list { if ui.button(tr(n)).clicked() { app.shapes_cmd(move |p, l| p.set_effect(l, &e)); ui.close(); } }); };
        row(ui, "Preset", vec![("No Presets", Effect::None), ("Preset 1", Effect::Shadow { dir: 90.0, dist: 3.0, blur: 4.0, color: "000000".into(), alpha: 35 }), ("Preset 2", Effect::Bevel { kind: "circle".into(), size: 6.0 }), ("Preset 3", Effect::Glow { radius: 6.0, color: "4472C4".into() })]);
        row(ui, "Shadow", vec![("No Shadow", Effect::Shadow { dir: 0.0, dist: 0.0, blur: 0.0, color: "000000".into(), alpha: 0 }), ("Offset: Bottom Right", Effect::Shadow { dir: 45.0, dist: 3.0, blur: 4.0, color: "000000".into(), alpha: 40 }), ("Offset: Bottom", Effect::Shadow { dir: 90.0, dist: 3.0, blur: 4.0, color: "000000".into(), alpha: 40 }), ("Offset: Center", Effect::Shadow { dir: 0.0, dist: 0.0, blur: 8.0, color: "000000".into(), alpha: 40 }), ("Inside: Top Left", Effect::InnerShadow { dir: 225.0, dist: 4.0, blur: 5.0 }), ("Inside: Center", Effect::InnerShadow { dir: 0.0, dist: 0.0, blur: 9.0 }), ("Perspective: Below", Effect::Shadow { dir: 90.0, dist: 14.0, blur: 12.0, color: "000000".into(), alpha: 22 })]);
        row(ui, "Reflection", vec![("No Reflection", Effect::Reflection { size: 0.0, dist: 0.0 }), ("Tight Reflection: Touching", Effect::Reflection { size: 0.35, dist: 0.0 }), ("Half Reflection: Touching", Effect::Reflection { size: 0.55, dist: 0.0 }), ("Full Reflection: 4 pt offset", Effect::Reflection { size: 0.9, dist: 4.0 })]);
        row(ui, "Glow", vec![("No Glow", Effect::Glow { radius: 0.0, color: a1.clone() }), ("Glow: 5 pt; Accent 1", Effect::Glow { radius: 5.0, color: a1.clone() }), ("Glow: 8 pt; Accent 2", Effect::Glow { radius: 8.0, color: a2.clone() }), ("Glow: 11 pt; Accent 4", Effect::Glow { radius: 11.0, color: a4.clone() }), ("Glow: 18 pt; Accent 1", Effect::Glow { radius: 18.0, color: a1.clone() })]);
        row(ui, "Soft Edges", vec![("No Soft Edges", Effect::SoftEdge { radius: 0.0 }), ("1 Point", Effect::SoftEdge { radius: 1.0 }), ("2.5 Point", Effect::SoftEdge { radius: 2.5 }), ("5 Point", Effect::SoftEdge { radius: 5.0 }), ("10 Point", Effect::SoftEdge { radius: 10.0 }), ("25 Point", Effect::SoftEdge { radius: 25.0 }), ("50 Point", Effect::SoftEdge { radius: 50.0 })]);
        drop(row);
        crate::theme::submenu(ui, tr("Bevel"), |ui| crate::three_d_ui::bevel_gallery(app, ui));
        crate::theme::submenu(ui, tr("3-D Rotation"), |ui| crate::three_d_ui::rotation_gallery(app, ui));
        ui.separator();
        if ui.button(tr("3-D Options…")).clicked() { app.pane = Pane::Format; app.format_tab = 1; ui.close(); }
        if ui.button(tr("Format Shape…")).clicked() { app.pane = Pane::Format; ui.close(); }
    });
}

/// Slides from an outline: a text file, one slide per unindented line, indented lines as
/// bullets.
fn slides_from_outline(app: &mut SlidesApp) {
    let Some(p) = rfd::FileDialog::new().add_filter(&tr("Outline"), &["txt", "md"]).pick_file() else { return };
    let Ok(text) = std::fs::read_to_string(&p) else { return };
    let layout = app.pres.all_layouts().into_iter().find(|l| app.pres.layout_type(l) == "obj");
    let Some(layout) = layout else { return };
    app.checkpoint("outline");
    let mut at = app.current + 1;
    let mut current: Option<(String, Vec<String>)> = None;
    let mut flush = |app: &mut SlidesApp, item: Option<(String, Vec<String>)>, at: &mut usize| {
        let Some((title, body)) = item else { return };
        let s = app.pres.add_slide(&layout, *at);
        *at += 1;
        let scene = app.pres.scene(&s);
        for it in scene.items.iter().filter(|i| i.own) {
            match it.placeholder.as_ref().map(|p| p.0.as_str()) {
                Some("title") => app.pres.set_shape_text(&it.loc, None, &title),
                Some(_) if !body.is_empty() => app.pres.set_shape_text(&it.loc, None, &body.join("\n")),
                _ => {}
            }
        }
    };
    for line in text.lines() {
        if line.trim().is_empty() { continue; }
        let indented = line.starts_with([' ', '\t']) || line.trim_start().starts_with(['-', '*', '•']);
        let t = line.trim().trim_start_matches(['-', '*', '•', '#']).trim().to_string();
        if indented { if let Some(c) = &mut current { c.1.push(t); } } else { let prev = current.take(); flush(app, prev, &mut at); current = Some((t, vec![])); }
    }
    let prev = current.take();
    flush(app, prev, &mut at);
    app.go_to(app.current + 1);
}

/// Convert the selected text shape's paragraphs into a SmartArt graphic in its place.
fn convert_to_smartart(app: &mut SlidesApp, layout: &str) {
    let Some(loc) = app.edit.as_ref().map(|e| e.loc.clone()).or_else(|| app.selection.first().cloned()) else { return };
    let Some(scene) = app.scene() else { return };
    // Paragraph levels from the scene.
    let items: Vec<(u8, String)> = SlidesApp::items_of(&scene, &loc).into_iter().next().and_then(|it| if let slides_engine::scene::Kind::Shape { text: Some(t), .. } = &it.kind { Some(t.paras.iter().map(|p| (p.level.min(4) as u8, p.runs.iter().map(|r| r.text.as_str()).collect::<String>())).filter(|i| !i.1.trim().is_empty()).collect()) } else { None }).unwrap_or_default();
    if items.is_empty() { return; }
    let spec = slides_engine::diagram::Spec { items, ..slides_engine::diagram::Spec::new(layout, "") };
    crate::smartart_ui::apply_spec(app, None, Some(loc), &spec);
}

/// Dictate: macOS and Windows have system dictation that types into the focused text; start
/// editing so it lands in the slide, and open the system's dictation.
fn dictate(app: &mut SlidesApp) {
    if app.edit.is_none() {
        if let Some(l) = app.selection.first().cloned() { let n = app.pres.shape_text(&l, None).chars().count(); app.edit = Some(crate::app::TextEdit { loc: l, cell: None, cursor: n, anchor: n }); }
        else { app.insert_text_here(""); }
    }
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("osascript").args(["-e", "tell application \"System Events\" to key code 63 using {}", "-e", "delay 0.05", "-e", "tell application \"System Events\" to key code 63 using {}"]).spawn(); }
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", "(New-Object -ComObject WScript.Shell).SendKeys('^#h')"]).spawn(); }
    app.notify(&tr("Dictation: speak, and your words are typed at the cursor. Press the dictation key again to stop."));
}

/// Group, Ungroup and Regroup (Arrange and the right-click menu).
pub fn group_items(app: &mut SlidesApp, ui: &mut Ui) {
    let scene = app.scene();
    if menu_item(ui, "group", "Group") && app.selection.len() > 1 {
        if let Some(sc) = &scene {
            let frames: Vec<_> = app.selection.iter().filter_map(|l| app.frame_of(sc, l)).collect();
            if frames.len() == app.selection.len() {
                app.checkpoint("group");
                let mut sel = app.selection.clone();
                sel.sort_by_key(|l| l.path[0]);
                let frames: Vec<_> = sel.iter().filter_map(|l| app.frame_of(sc, l)).collect();
                if let Some(g) = app.pres.group(&sel, &frames) { app.last_group = sel.len(); app.selection = vec![g]; }
            }
        }
        ui.close();
    }
    if menu_item(ui, "ungroup", "Ungroup") {
        app.checkpoint("ungroup");
        let mut out = vec![];
        let mut sel = app.selection.clone();
        sel.sort_by_key(|l| std::cmp::Reverse(l.path[0]));
        for l in sel { out.extend(app.pres.ungroup(&l)); }
        if !out.is_empty() { app.regroup = out.clone(); app.selection = out; }
        ui.close();
    }
    if menu_item(ui, "group", "Regroup") && app.regroup.len() > 1 {
        if let Some(sc) = &scene {
            let sel = app.regroup.clone();
            let frames: Vec<_> = sel.iter().filter_map(|l| app.frame_of(sc, l)).collect();
            if frames.len() == sel.len() { app.checkpoint("group"); if let Some(g) = app.pres.group(&sel, &frames) { app.selection = vec![g]; } }
        }
        ui.close();
    }
}
