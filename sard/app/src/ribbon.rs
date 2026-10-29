//! The ribbon (ribbon/tabs.js), drawn with Qalam's components: a row of tabs with quick
//! access, and one card of command groups for the active tab.

use crate::app::{SlidesApp, Tab};
use crate::i18n::tr;
use egui::{pos2, vec2, FontFamily, FontId, Rect, RichText, Sense, Stroke, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use slides_engine::edit::{Order, RunFormat};

const GROUP_H: f32 = 76.0;

pub fn ribbon(app: &mut SlidesApp, ui: &mut Ui) {
    let scene = app.scene();
    let kind = scene.as_ref().and_then(|s| app.selected_kind(s));
    // Contextual tabs follow the selection.
    if app.tab == Tab::ShapeFormat && !matches!(kind, Some("shape") | Some("picture")) { app.tab = Tab::Home; }
    if matches!(app.tab, Tab::Table | Tab::TableLayout) && kind != Some("table") { app.tab = Tab::Home; }
    if app.tab == Tab::SmartArt && crate::smartart_ui::selected(app).is_none() { app.tab = Tab::Home; }
    egui::Panel::top("ribbon").show_separator_line(false).frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 6 })).show(ui, |ui| {
        let mut active = Rect::NOTHING;
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 30.0), row_layout(), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let file = ui.add(egui::Button::new(RichText::new(tr("File")).size(14.0).color(c_ink()).family(FontFamily::Name("ui-bold".into()))).frame(false).min_size(vec2(44.0, 30.0)));
            crate::theme::menu(&file, |ui| file_menu(app, ui));
            let master = match app.view { crate::views::View::SlideMaster => Some("Slide Master"), crate::views::View::HandoutMaster => Some("Handout Master"), crate::views::View::NotesMaster => Some("Notes Master"), _ => None };
            let mut tabs = vec![(Tab::Home, "Home"), (Tab::Insert, "Insert"), (Tab::Design, "Design"), (Tab::Transitions, "Transitions"), (Tab::Animations, "Animations"), (Tab::SlideShow, "Slide Show"), (Tab::View, "View")];
            if let Some(m) = master { tabs.insert(0, (Tab::Master, m)); if app.view != crate::views::View::SlideMaster { tabs.retain(|t| matches!(t.0, Tab::Master | Tab::View)); } }
            if crate::smartart_ui::selected(app).is_some() { tabs.push((Tab::SmartArt, "SmartArt Design")); }
            match kind { Some("shape") => tabs.push((Tab::ShapeFormat, "Shape Format")), Some("picture") => tabs.push((Tab::ShapeFormat, "Picture Format")), Some("table") => { tabs.push((Tab::Table, "Table Design")); tabs.push((Tab::TableLayout, "Layout")); } _ => {} }
            for (tab, label) in tabs {
                let label = tr(label);
                let selected = app.tab == tab;
                let contextual = matches!(tab, Tab::ShapeFormat | Tab::Table | Tab::TableLayout | Tab::Master | Tab::SmartArt);
                let font = FontId::new(14.0, if selected { FontFamily::Name("ui-bold".into()) } else { FontFamily::Proportional });
                let g = ui.painter().layout_no_wrap(label, font, if selected || contextual { ACCENT } else { c_ink2() });
                let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 22.0, 30.0), Sense::click());
                if selected {
                    let t = Rect::from_min_max(rect.min + vec2(0.0, 2.0), rect.max + vec2(0.0, 1.0));
                    ui.painter().rect_filled(t, 0.0, c_card());
                    let s = Stroke::new(1.0, c_line());
                    ui.painter().line_segment([t.left_bottom(), t.left_top()], s);
                    ui.painter().line_segment([t.left_top(), t.right_top()], s);
                    ui.painter().line_segment([t.right_top(), t.right_bottom()], s);
                    active = t;
                } else if r.hovered() { ui.painter().rect_filled(rect.shrink2(vec2(0.0, 3.0)), 0.0, c_hover()); }
                ui.painter().galley(pos2(rect.center().x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0), g, c_ink());
                if r.clicked() { app.tab = tab; }
            }
            ui.with_layout(far_layout(), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if small(ui, "present", Some("Present"), false, true, "Start the slide show from this slide (⇧F5)").clicked() { app.start_show(app.current); }
                ui.add_space(10.0);
                if small(ui, "save", None, false, true, "Save (⌘S)").clicked() { app.save(false); }
                if small(ui, "redo", None, false, app.can_redo(), "Redo (⇧⌘Z)").clicked() { app.redo(); }
                if small(ui, "undo", None, false, app.can_undo(), "Undo (⌘Z)").clicked() { app.undo(); }
            });
        });
        let painter = ui.painter().clone();
        egui::Frame::NONE.fill(c_card()).inner_margin(egui::Margin::symmetric(8, 2)).stroke(Stroke::new(1.0, c_line())).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // A tab wider than the window draws compactly (labels become tooltips), as in Qalam.
            let full_id = egui::Id::new(("ribbon-full-width", app.tab as usize));
            let full = ui.data(|d| d.get_temp::<f32>(full_id)).unwrap_or(0.0);
            let is_compact = full > ui.available_width();
            qalam_kit::widgets::COMPACT.with(|c| c.set(is_compact));
            let out = egui::ScrollArea::horizontal().id_salt("ribbon-scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded).show(ui, |ui| {
                ui.with_layout(row_layout(), |ui| {
                    ui.set_height(GROUP_H + 6.0);
                    match app.tab {
                        Tab::Home => home(app, ui),
                        Tab::Insert => insert(app, ui),
                        Tab::Design => crate::design_tab::design(app, ui),
                        Tab::Transitions => crate::transitions_tab::transitions(app, ui),
                        Tab::Animations => crate::animations_tab::animations(app, ui),
                        Tab::SlideShow => slide_show(app, ui),
                        Tab::View => view(app, ui),
                        Tab::ShapeFormat => shape_format(app, ui),
                        Tab::Table => crate::table_tab::design(app, ui),
                        Tab::TableLayout => crate::table_tab::layout(app, ui),
                        Tab::Master => crate::views::master_tab(app, ui),
                        Tab::SmartArt => crate::smartart_ui::design_tab(app, ui),
                    }
                });
            });
            qalam_kit::widgets::COMPACT.with(|c| c.set(false));
            if !is_compact { ui.data_mut(|d| d.insert_temp(full_id, out.content_size.x)); }
        });
        if active.is_positive() {
            painter.rect_filled(Rect::from_min_max(pos2(active.min.x + 1.0, active.max.y - 2.0), pos2(active.max.x - 1.0, active.max.y + 1.5)), 0.0, c_card());
        }
    });
}

fn file_menu(app: &mut SlidesApp, ui: &mut Ui) { crate::filemenu::file_menu(app, ui); }

fn hex(c: word_engine::Color) -> String { format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2) }

fn home(app: &mut SlidesApp, ui: &mut Ui) { crate::home_tab::home(app, ui); }
const SHAPES: &[(&str, &[&str])] = &[
    ("Lines", &["line", "straightConnector1", "bentConnector3", "curvedConnector3"]),
    ("Rectangles", &["rect", "roundRect", "snip1Rect", "snip2SameRect", "round1Rect", "round2SameRect"]),
    ("Basic Shapes", &["ellipse", "triangle", "rtTriangle", "parallelogram", "trapezoid", "diamond", "pentagon", "hexagon", "octagon", "donut", "noSmoking", "blockArc", "heart", "lightningBolt", "sun", "moon", "cloud", "smileyFace", "can", "cube", "frame", "plaque", "teardrop", "pie"]),
    ("Block Arrows", &["rightArrow", "leftArrow", "upArrow", "downArrow", "leftRightArrow", "upDownArrow", "quadArrow", "bentArrow", "uturnArrow", "chevron", "homePlate", "notchedRightArrow", "circularArrow"]),
    ("Flowchart", &["flowChartProcess", "flowChartAlternateProcess", "flowChartDecision", "flowChartInputOutput", "flowChartDocument", "flowChartTerminator", "flowChartConnector"]),
    ("Stars and Banners", &["star4", "star5", "star6", "star8", "star12", "star24", "ribbon2", "wave", "doubleWave", "horizontalScroll"]),
    ("Callouts", &["wedgeRectCallout", "wedgeRoundRectCallout", "wedgeEllipseCallout", "cloudCallout", "borderCallout1"]),
];

pub fn shape_gallery(app: &mut SlidesApp, ui: &mut Ui) {
    ui.set_width(300.0);
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        for (group, names) in SHAPES {
            menu_heading(ui, group);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                for n in *names {
                    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                    if resp.hovered() { ui.painter().rect_filled(r, 4.0, c_hover()); }
                    let g = slides_engine::geom::preset(n, 18.0, 18.0, &[]);
                    for p in &g.paths {
                        for poly in slides_engine::geom::flatten(&p.cmds, 6) {
                            let pts: Vec<egui::Pos2> = poly.iter().map(|q| pos2(r.min.x + 5.0 + q.x as f32, r.min.y + 5.0 + q.y as f32)).collect();
                            if pts.len() > 1 { ui.painter().add(egui::Shape::line(pts, Stroke::new(1.1, c_ink2()))); }
                        }
                    }
                    if resp.on_hover_text(slides_engine::edit::preset_label(n)).clicked() { app.insert_mode = Some(n.to_string()); ui.close(); }
                }
            });
        }
    });
}

fn order_menu(app: &mut SlidesApp, ui: &mut Ui) {
    for (icon, label, order) in [("bringToFront", "Bring to Front", Order::Front), ("bringForward", "Bring Forward", Order::Forward), ("sendBackward", "Send Backward", Order::Backward), ("sendToBack", "Send to Back", Order::Back)] {
        if menu_item(ui, icon, label) && !app.selection.is_empty() {
            app.checkpoint("order");
            app.selection = app.selection.clone().iter().filter_map(|l| app.pres.reorder(l, order)).collect();
            ui.close();
        }
    }
    ui.separator();
    let scene = app.scene();
    let (sw, sh) = app.pres.size();
    for (icon, label, k) in [("alignObjLeft", "Align Left", 0), ("alignObjCenter", "Align Center", 1), ("alignObjRight", "Align Right", 2), ("alignObjTop", "Align Top", 3), ("alignObjMiddle", "Align Middle", 4), ("alignObjBottom", "Align Bottom", 5)] {
        if menu_item(ui, icon, label) {
            let Some(scene) = &scene else { continue };
            let frames: Vec<_> = app.selection.iter().filter_map(|l| app.frame_of(scene, l).map(|f| (l.clone(), f))).collect();
            if frames.is_empty() { continue; }
            // One object aligns to the slide; several align to each other.
            let (x0, y0, x1, y1) = if frames.len() == 1 { (0.0, 0.0, sw, sh) } else {
                (frames.iter().map(|f| f.1.x).fold(f64::MAX, f64::min), frames.iter().map(|f| f.1.y).fold(f64::MAX, f64::min), frames.iter().map(|f| f.1.x + f.1.w).fold(f64::MIN, f64::max), frames.iter().map(|f| f.1.y + f.1.h).fold(f64::MIN, f64::max))
            };
            app.checkpoint("align");
            for (l, mut f) in frames {
                match k { 0 => f.x = x0, 1 => f.x = (x0 + x1 - f.w) / 2.0, 2 => f.x = x1 - f.w, 3 => f.y = y0, 4 => f.y = (y0 + y1 - f.h) / 2.0, _ => f.y = y1 - f.h }
                app.pres.set_xfrm(&l, &f);
            }
            ui.close();
        }
    }
}

fn fill_outline(app: &mut SlidesApp, ui: &mut Ui) {
    let on = !app.selection.is_empty();
    let fill = small_menu(ui, "shapeFill", "Shape Fill", "Fill the shape with a color");
    if on { crate::theme::menu(&fill, |ui| if let Some(pick) = color_menu(ui, "No Fill") { app.checkpoint("fill"); let v = pick.map(hex).unwrap_or_default(); for l in app.selection.clone() { app.pres.set_fill(&l, Some(&v)); } ui.close(); }); }
    let outline = small_menu(ui, "shapeOutline", "Shape Outline", "Outline color, weight and dashes");
    if on {
        crate::theme::menu(&outline, |ui| {
            if let Some(pick) = color_menu(ui, "No Outline") { app.checkpoint("line"); let v = pick.map(hex).unwrap_or_default(); for l in app.selection.clone() { app.pres.set_line(&l, Some(&v), None, None); } ui.close(); }
            crate::theme::submenu(ui, tr("Weight"), |ui| for w in [0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.0, 4.5, 6.0] { if ui.button(format!("{w} pt")).clicked() { app.checkpoint("line"); for l in app.selection.clone() { app.pres.set_line(&l, None, Some(w), None); } ui.close(); } });
            crate::theme::submenu(ui, tr("Dashes"), |ui| for (d, n) in [("solid", "Solid"), ("sysDot", "Round Dot"), ("dash", "Dash"), ("dashDot", "Dash Dot"), ("lgDash", "Long Dash")] { if ui.button(tr(n)).clicked() { app.checkpoint("line"); for l in app.selection.clone() { app.pres.set_line(&l, None, None, Some(d)); } ui.close(); } });
            crate::theme::submenu(ui, tr("Arrows"), |ui| for (h, t, n) in [(None, None, "None"), (None, Some("triangle"), "Arrow at end"), (Some("triangle"), None, "Arrow at start"), (Some("triangle"), Some("triangle"), "Arrows at both ends")] { if ui.button(tr(n)).clicked() { app.checkpoint("line"); for l in app.selection.clone() { app.pres.set_arrows(&l, h, t); } ui.close(); } });
        });
    }
}

fn insert(app: &mut SlidesApp, ui: &mut Ui) { crate::insert_tab::insert(app, ui); }
fn slide_show(app: &mut SlidesApp, ui: &mut Ui) { crate::views::slide_show_tab(app, ui); }

fn view(app: &mut SlidesApp, ui: &mut Ui) { crate::views::view_tab(app, ui); }

fn shape_format(app: &mut SlidesApp, ui: &mut Ui) {
    group(ui, "Insert Shapes", |ui| {
        let shapes = large_menu(ui, "shapes", "Shapes", "Insert a shape");
        crate::theme::menu(&shapes, |ui| shape_gallery(app, ui));
    });
    group(ui, "Shape Styles", |ui| { rows(ui, |ui| fill_outline(app, ui)); });
    group(ui, "Arrange", |ui| {
        let arrange = large_menu(ui, "arrange", "Arrange", "Order and align");
        crate::theme::menu(&arrange, |ui| order_menu(app, ui));
        rows(ui, |ui| {
            let scene = app.scene();
            for (icon, label, f) in [("rotate", "Rotate Right 90°", 0), ("flipH", "Flip Horizontal", 1), ("flipV", "Flip Vertical", 2)] {
                if small(ui, icon, Some(label), false, !app.selection.is_empty(), label).clicked() {
                    let Some(scene) = &scene else { continue };
                    app.checkpoint("rotate");
                    for l in app.selection.clone() {
                        if let Some(mut x) = app.frame_of(scene, &l) { match f { 0 => x.rot = (x.rot + 90.0) % 360.0, 1 => x.flip_h = !x.flip_h, _ => x.flip_v = !x.flip_v } app.pres.set_xfrm(&l, &x); }
                    }
                }
            }
        });
    });
}

