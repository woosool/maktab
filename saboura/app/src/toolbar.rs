//! The interface floating over the board, laid out as the JS app's editor: the board bar
//! (back, name, save state, menu, command palette), the tool rail with its shape and table
//! pickers, the bar of properties for the selection, zoom and history controls, the minimap
//! and the context menu.

use crate::app::{BoardApp, Panel, Tool};
use crate::canvas::{hex, list_kind, set_list, toggle_flag};
use crate::i18n::tr;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use qalam_kit::colors::*;
use saboura_engine::geometry;
use saboura_engine::model::{COLORS, PEN_COLORS};

/// Extra colours offered beside the note and pen palettes.
const MORE_COLORS: [&str; 8] = ["#fde68a", "#fca5a5", "#93c5fd", "#86efac", "#c4b5fd", "#ffffff", "#9ca3af", "#1f2937"];
const QUICK_SHAPES: [&str; 11] = ["rectangle", "terminator", "ellipse", "diamond", "triangle", "star", "hexagon", "library", "line", "parallelogram", "cylinder"];
const FONTS: [(&str, &str); 3] = [("noto-sans", "Sans"), ("eb-garamond", "Serif"), ("roboto-mono", "Mono")];

fn panel_frame(ui: &Ui) -> egui::Frame {
    egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(4)).corner_radius(10)
}

/// An icon button; `on` shows it pressed.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str, on: bool) -> egui::Response {
    icon_button_enabled(ui, icon, tip, on, true)
}

pub fn icon_button_enabled(ui: &mut Ui, icon: &str, tip: &str, on: bool, enabled: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), if enabled { Sense::click() } else { Sense::hover() });
    if on { ui.painter().rect_filled(r, 7.0, ACCENT.gamma_multiply(0.18)); } else if resp.hovered() && enabled { ui.painter().rect_filled(r, 7.0, c_hover()); }
    let ink = if !enabled { c_ink3().gamma_multiply(0.5) } else if on { ACCENT } else { c_ink2() };
    crate::icons::paint(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(18.0, 18.0)), ink);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

fn swatch(ui: &mut Ui, color: &str, on: bool, hollow: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let c = hex(Some(color), Color32::GRAY);
    if hollow { ui.painter().circle_stroke(r.center(), 8.0, Stroke::new(3.0, c)); }
    else { ui.painter().circle(r.center(), 9.0, c, Stroke::new(1.0, c_line())); }
    if on || resp.hovered() { ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(1.5, if on { ACCENT } else { c_line() })); }
    resp.clicked()
}

/// All the floating chrome, drawn after the canvas.
pub fn chrome(app: &mut BoardApp, ui: &mut Ui) {
    let board = app.canvas_rect;
    if !board.is_positive() { return; }
    board_bar(app, ui, board);
    tool_rail(app, ui, board);
    selection_bar(app, ui, board);
    bottom_bar(app, ui, board);
    if app.prefs.minimap { minimap(app, ui, board); }
    crate::dialogs::show(app, ui);
}

fn board_bar(app: &mut BoardApp, ui: &mut Ui, board: Rect) {
    let (at, pivot) = crate::rtl::corner(board, true, true, vec2(8.0, 8.0));
    egui::Area::new(egui::Id::unique("board-bar")).pivot(pivot).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        panel_frame(ui).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
            crate::rtl::row(ui, |ui| {
                if icon_button(ui, "back", &tr("Boards"), app.panel == Panel::Home).clicked() { app.panel = Panel::Home; }
                let mut title = app.board.title.clone();
                let w = (ui.fonts_mut(|f| f.layout_no_wrap(title.clone(), egui::FontId::new(14.0, egui::FontFamily::Name("ui-bold".into())), c_ink()).size().x) + 16.0).clamp(80.0, 320.0);
                let r = ui.add(egui::TextEdit::singleline(&mut title).desired_width(w).char_limit(200).frame(egui::Frame::NONE).font(egui::FontId::new(14.0, egui::FontFamily::Name("ui-bold".into()))));
                if r.changed() { app.board.title = title; app.changed(); }
                let (label, color) = if app.dirty && app.path.is_none() { (tr("Not saved"), Color32::from_rgb(0xc8, 0x6b, 0x0a)) }
                    else if app.dirty { (tr("Saving…"), c_ink3()) } else if app.path.is_some() { (format!("✓ {}", tr("Saved")), Color32::from_rgb(0x26, 0x89, 0x4a)) }
                    else { (tr("Not saved"), c_ink3()) };
                let chip = ui.add(egui::Button::new(egui::RichText::new(label).size(12.0).color(color)).frame_when_inactive(false))
                    .on_hover_text(app.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| tr("Save this board to a file")));
                if chip.clicked() { app.save(false); }
                let more = icon_button(ui, "more", &tr("Menu"), false);
                crate::theme::menu(&more, |ui| file_menu(app, ui));
                if icon_button(ui, "command", &tr("Command palette (Ctrl/Cmd K)"), matches!(app.panel, Panel::Palette(_))).clicked() { app.panel = Panel::Palette(String::new()); }
            });
        });
    });
}

/// The board menu: every menu of the menu bar, as submenus.
pub fn file_menu(app: &mut BoardApp, ui: &mut Ui) {
    for (title, entries) in crate::commands::MENUS {
        crate::theme::submenu(ui, tr(title), |ui| {
            ui.set_min_width(230.0);
            for (id, _, accel) in entries.iter() {
                if *id == "-" { ui.separator(); continue; }
                let mut text = crate::commands::label(id);
                if let Some(on) = crate::commands::checked(app, id) { text = format!("{} {text}", if on { "✓" } else { "  " }); }
                if crate::rtl::item(ui, &text, &crate::commands::shortcut_text(accel), crate::commands::enabled(app, id)).clicked() { ui.close(); crate::commands::run(app, id); }
            }
        });
    }
}

fn tool_rail(app: &mut BoardApp, ui: &mut Ui, board: Rect) {
    let pos = pos2(board.min.x + 8.0, board.min.y + 100.0);
    let mut anchors: Vec<(&str, Pos2)> = Vec::new();
    let (pos, pivot) = crate::rtl::corner(board, true, true, vec2(8.0, pos.y - board.min.y));
    egui::Area::new(egui::Id::unique("tool-rail")).pivot(pivot).fixed_pos(pos).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        panel_frame(ui).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
            ui.vertical(|ui| {
                let tools: [(Tool, &str, &str); 4] = [(Tool::Select, "select", "Select — V"), (Tool::Hand, "hand", "Hand — H"), (Tool::Note, "note", "Sticky note — N"), (Tool::Text, "text", "Text — T")];
                for (tool, icon, tip) in tools {
                    if icon_button(ui, icon, &tr(tip), app.tool == tool).clicked() { app.tool = tool; app.panel = Panel::None; }
                }
                let shape_icon = if app.tool == Tool::Shape { app.shape } else { "shape" };
                let r = icon_button(ui, if shape_icon == "ellipse" || crate::icons::known(shape_icon) { shape_icon } else { "shape" }, &tr("Shape — R"), app.tool == Tool::Shape || app.panel == Panel::Shapes);
                anchors.push(("shapes", if crate::rtl::rtl() { r.rect.left_top() } else { r.rect.right_top() }));
                if r.clicked() { app.panel = if app.panel == Panel::Shapes { Panel::None } else { Panel::Shapes }; }
                if icon_button(ui, "line", &tr("Connection line — L"), app.tool == Tool::Arrow).clicked() { app.tool = Tool::Arrow; app.panel = Panel::None; }
                if icon_button(ui, "pen", &tr("Pen — P"), app.tool == Tool::Pen).clicked() { app.tool = Tool::Pen; app.panel = Panel::None; }
                if icon_button(ui, "frame", &tr("Frame — F"), app.tool == Tool::Frame).clicked() { app.tool = Tool::Frame; app.panel = Panel::None; }
                let r = icon_button(ui, "table", &tr("Table"), matches!(app.panel, Panel::Table(..)));
                anchors.push(("table", if crate::rtl::rtl() { r.rect.left_top() } else { r.rect.right_top() }));
                if r.clicked() { app.panel = if matches!(app.panel, Panel::Table(..)) { Panel::None } else { Panel::Table(3, 3) }; }
                if icon_button(ui, "mindmap", &tr("Mind map"), false).clicked() { app.panel = Panel::None; app.new_mind_map(); }
                if icon_button(ui, "image", &tr("Add image"), false).clicked() { app.panel = Panel::None; app.insert_image_dialog(); }
                if icon_button(ui, "comment", &tr("Comment — C"), app.tool == Tool::Comment).clicked() { app.tool = Tool::Comment; app.panel = Panel::None; }
                ui.separator();
                if icon_button(ui, "templates", &tr("Templates"), matches!(app.panel, Panel::Templates(..))).clicked() { app.panel = Panel::Templates(String::new(), String::new()); }
                if icon_button(ui, "sticker", &tr("Icons and stickers"), matches!(app.panel, Panel::Icons(..))).clicked() { app.panel = Panel::Icons(String::new(), String::new(), 0); }
                if icon_button(ui, "vote", &tr("Dot voting"), app.side == crate::app::Side::Votes).clicked() { app.toggle_side(crate::app::Side::Votes); }
                if app.tool == Tool::Pen {
                    ui.separator();
                    for c in PEN_COLORS { if swatch(ui, c, app.pen_color == c, false) { app.pen_color = c.into(); } }
                }
            });
        });
    });
    for (what, at) in anchors {
        match (what, &app.panel) {
            ("shapes", Panel::Shapes) => shape_popover(app, ui, at),
            ("table", Panel::Table(..)) => table_popover(app, ui, at),
            _ => {}
        }
    }
}

/// Where a popover opens beside a rail button: after it in reading order.
fn popover_at(at: Pos2) -> (Pos2, Align2) {
    if crate::rtl::rtl() { (at - vec2(8.0, 0.0), Align2::RIGHT_TOP) } else { (at + vec2(8.0, 0.0), Align2::LEFT_TOP) }
}

fn shape_popover(app: &mut BoardApp, ui: &mut Ui, at: Pos2) {
    let (at, pivot) = popover_at(at);
    egui::Area::new(egui::Id::unique("shape-popover")).pivot(pivot).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        panel_frame(ui).inner_margin(8).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            crate::toolbar::Grid::new("quick-shapes").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
                for (n, key) in QUICK_SHAPES.iter().enumerate() {
                    let name = geometry::SHAPES.iter().find(|s| s.0 == *key).map(|s| s.1).unwrap_or(match *key { "library" => "Shape library (S)", "line" => "Connection line", _ => key });
                    let on = app.tool == Tool::Shape && app.shape == *key;
                    if icon_button(ui, key, &tr(name), on).clicked() {
                        match *key {
                            "library" => app.panel = Panel::Library,
                            "line" => { app.tool = Tool::Arrow; app.panel = Panel::None; }
                            k => { app.tool = Tool::Shape; app.shape = geometry::SHAPES.iter().find(|s| s.0 == k).map(|s| s.0).unwrap_or("rectangle"); app.panel = Panel::None; }
                        }
                    }
                    if n % 4 == 3 { ui.end_row(); }
                }
            });
            if ui.add_sized([150.0, 28.0], egui::Button::new(tr("Shape library (S)"))).clicked() { app.panel = Panel::Library; }
        });
    });
}

fn table_popover(app: &mut BoardApp, ui: &mut Ui, at: Pos2) {
    let Panel::Table(mut rows, mut cols) = app.panel else { return };
    let (at, pivot) = popover_at(at);
    egui::Area::new(egui::Id::unique("table-popover")).pivot(pivot).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        panel_frame(ui).inner_margin(10).show(ui, |ui| {
            ui.label(format!("{} {} × {}", tr("Table"), rows, cols));
            let size = 18.0;
            let (r, resp) = ui.allocate_exact_size(vec2(size * 8.0, size * 8.0), Sense::click());
            if let Some(p) = resp.hover_pos() {
                rows = (((p.y - r.min.y) / size) as usize + 1).clamp(1, 8);
                cols = (((p.x - r.min.x) / size) as usize + 1).clamp(1, 8);
            }
            for y in 0..8 { for x in 0..8 {
                let c = Rect::from_min_size(r.min + vec2(x as f32 * size, y as f32 * size), vec2(size - 3.0, size - 3.0));
                let on = y < rows && x < cols;
                ui.painter().rect(c, 2.0, if on { ACCENT.gamma_multiply(0.35) } else { c_card() }, Stroke::new(1.0, if on { ACCENT } else { c_line() }), egui::StrokeKind::Inside);
            } }
            if resp.clicked() { app.panel = Panel::None; app.insert_table(rows, cols); } else { app.panel = Panel::Table(rows, cols); }
        });
    });
}

/// The bar of properties for the selection, floating above it.
fn selection_bar(app: &mut BoardApp, ui: &mut Ui, board: Rect) {
    if app.selection.is_empty() || app.drag.is_some() { return; }
    let items: Vec<_> = app.selected_items().into_iter().cloned().collect();
    let b = geometry::bounds(&items);
    let top = app.view.to_screen((b.x + b.w / 2.0, b.y));
    let bottom = app.view.to_screen((b.x + b.w / 2.0, b.y + b.h));
    let above = board.min.y + top.1 as f32 - 64.0 > board.min.y + 56.0;
    let anchor = if above { pos2(board.min.x + top.0 as f32, board.min.y + top.1 as f32 - 40.0) } else { pos2(board.min.x + bottom.0 as f32, board.min.y + bottom.1 as f32 + 44.0) };
    let all = |k: &[&str]| items.iter().all(|i| k.contains(&i.kind.as_str()));
    let shapes = all(&["rect", "ellipse"]);
    let lines = all(&["arrow"]);
    let pens = all(&["pen"]);
    let images = all(&["image"]);
    let fills = !lines && !pens && !images;
    let texts = !pens && !images;
    let first = items[0].clone();
    let locked = items.iter().all(|i| i.is_locked());
    egui::Area::new(egui::Id::unique("selection-bar")).pivot(Align2::CENTER_BOTTOM).fixed_pos(anchor)
        .order(egui::Order::Foreground).constrain_to(if crate::rtl::rtl() { Rect::from_min_max(board.min + vec2(8.0, 8.0), board.max - vec2(60.0, 8.0)) } else { Rect::from_min_max(board.min + vec2(60.0, 8.0), board.max - vec2(8.0, 8.0)) }).show(ui.ctx(), |ui| {
        panel_frame(ui).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
            crate::rtl::row(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                if locked {
                    if icon_button(ui, "unlock", &tr("Unlock"), false).clicked() { app.toggle_lock(); }
                    return;
                }
                if shapes {
                    let current = if first.kind == "ellipse" { "ellipse".to_string() } else { first.shape.clone().unwrap_or("rectangle".into()) };
                    let r = icon_button(ui, if crate::icons::known(&current) { &current } else { "shape" }, &tr("Shape"), false);
                    crate::theme::menu(&r, |ui| {
                        crate::toolbar::Grid::new("switch-shape").show(ui, |ui| {
                            for (n, (k, name)) in geometry::SHAPES.iter().enumerate() {
                                if icon_button(ui, if crate::icons::known(k) { k } else { "shape" }, &tr(name), current == *k).clicked() {
                                    app.edit_selected(|i| { i.kind = if *k == "ellipse" { "ellipse" } else { "rect" }.into(); i.shape = (!matches!(*k, "rectangle" | "ellipse")).then(|| k.to_string()); });
                                    ui.close();
                                }
                                if n % 6 == 5 { ui.end_row(); }
                            }
                        });
                    });
                }
                // Fill (or line/ink) colour.
                let color = first.color.clone().unwrap_or_default();
                let palette: Vec<&str> = if lines || pens { PEN_COLORS.iter().chain(MORE_COLORS.iter()).copied().collect() } else { COLORS.iter().chain(MORE_COLORS.iter()).copied().collect() };
                if fills || lines || pens {
                    let r = color_button(ui, &color, false, &tr(if lines || pens { "Color" } else { "Fill" }));
                    crate::theme::menu(&r, |ui| { if let Some(c) = color_grid(ui, &palette, &color, false) { app.edit_selected(|i| i.color = Some(c.clone())); ui.close(); } });
                }
                if shapes {
                    let line = first.str("borderColor").unwrap_or("#878f86").to_string();
                    let r = color_button(ui, &line, true, &tr("Border"));
                    crate::theme::menu(&r, |ui| {
                        if let Some(c) = color_grid(ui, &PEN_COLORS.iter().chain(MORE_COLORS.iter()).copied().collect::<Vec<_>>(), &line, true) { app.edit_selected(|i| i.set("borderColor", c.clone())); ui.close(); }
                        ui.separator();
                        style_choices(app, ui, first.str("lineStyle").unwrap_or("solid"));
                    });
                }
                if shapes || lines || pens {
                    let w = first.line_width.unwrap_or(if lines { 2.0 } else if pens { 3.0 } else { 1.0 });
                    let r = ui.add(egui::Button::new(format!("{w:.0}px")).frame_when_inactive(false).min_size(vec2(0.0, 28.0))).on_hover_text(tr("Line width"));
                    crate::theme::menu(&r, |ui| {
                        for v in [1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0] { if ui.selectable_label(w == v, format!("{v:.0}px")).clicked() { app.edit_selected(|i| i.line_width = Some(v)); ui.close(); } }
                    });
                }
                if lines {
                    ui.separator();
                    let r = ui.add(egui::Button::new(tr(&cap(first.line_type.as_deref().unwrap_or("straight")))).frame_when_inactive(false).min_size(vec2(0.0, 28.0))).on_hover_text(tr("Line type"));
                    crate::theme::menu(&r, |ui| {
                        for (t, name) in [("straight", "Straight"), ("elbow", "Elbow"), ("curved", "Curved")] {
                            if ui.selectable_label(first.line_type.as_deref().unwrap_or("straight") == t, tr(name)).clicked() { app.edit_selected(|i| i.line_type = Some(t.into())); ui.close(); }
                        }
                        ui.separator();
                        style_choices(app, ui, first.str("lineStyle").unwrap_or("solid"));
                    });
                    for (start, label) in [(true, "Start"), (false, "End")] {
                        let current = if start { first.start_cap.clone().unwrap_or("none".into()) } else { first.end_cap.clone().unwrap_or("arrow".into()) };
                        let r = ui.add(egui::Button::new(format!("{} {}", tr(label), cap_symbol(&current, start))).frame_when_inactive(false).min_size(vec2(0.0, 28.0))).on_hover_text(tr(if start { "Start of line" } else { "End of line" }));
                        crate::theme::menu(&r, |ui| {
                            for (k, name) in CAPS {
                                if ui.selectable_label(current == k, format!("{}  {}", cap_symbol(k, start), tr(name))).clicked() {
                                    app.edit_selected(|i| if start { i.start_cap = (k != "none").then(|| k.to_string()); } else { i.end_cap = (k != "arrow").then(|| k.to_string()); });
                                    ui.close();
                                }
                            }
                        });
                    }
                }
                if texts {
                    ui.separator();
                    let family = FONTS.iter().find(|f| Some(f.0) == first.str("fontFamily")).map(|f| f.1).unwrap_or("Sans");
                    let size = first.font_size();
                    let r = ui.add(egui::Button::new(format!("{} · {size:.0}", tr(family))).frame_when_inactive(false).min_size(vec2(0.0, 28.0))).on_hover_text(tr("Font and size"));
                    crate::theme::menu(&r, |ui| {
                        for (id, name) in FONTS { if ui.selectable_label(family == name, tr(name)).clicked() { app.edit_selected(|i| i.set("fontFamily", if id == "noto-sans" { serde_json::Value::Null } else { id.into() })); } }
                        ui.separator();
                        crate::rtl::wrap(ui, |ui| {
                            for s in [12.0, 14.0, 16.0, 18.0, 24.0, 32.0, 48.0, 64.0] {
                                if ui.selectable_label((size - s).abs() < 0.5, format!("{s:.0}")).clicked() { app.edit_selected(|i| { i.font_size = Some(s); i.auto_font_size = None; }); }
                            }
                        });
                        let mut auto = first.auto_font_size == Some(true);
                        if ui.checkbox(&mut auto, tr("Fit text to the object")).changed() { app.edit_selected(|i| i.auto_font_size = auto.then_some(true)); }
                        ui.separator();
                        ui.label(tr("Highlight"));
                        let hl = first.str("highlight").unwrap_or("").to_string();
                        crate::rtl::row(ui, |ui| {
                            if ui.selectable_label(hl.is_empty(), tr("None")).clicked() { crate::canvas::set_text_style(app, "highlight", serde_json::Value::Null); }
                        });
                        if let Some(c) = color_grid(ui, &["#fde68a", "#bbf7d0", "#bfdbfe", "#fbcfe8", "#fed7aa", "#e9d5ff"], &hl, false) { crate::canvas::set_text_style(app, "highlight", c.into()); }
                        ui.separator();
                        ui.label(tr("Text color"));
                        let tc = first.str("textColor").unwrap_or("#30342f").to_string();
                        if let Some(c) = color_grid(ui, &["#30342f", "#ffffff", "#d03a3a", "#2f6fd6", "#26894a", "#c86b0a", "#8a5bd6", "#687169"], &tc, false) { crate::canvas::set_text_style(app, "textColor", c.into()); }
                    });
                    ui.separator();
                    for (key, label, tip) in [("bold", "B", "Bold (Ctrl/Cmd B)"), ("italic", "I", "Italic (Ctrl/Cmd I)"), ("underline", "U", "Underline (Ctrl/Cmd U)"), ("strike", "S", "Strikethrough")] {
                        let on = items.iter().all(|i| if key == "bold" { i.is_bold() } else { i.flag(key) });
                        let mut rt = egui::RichText::new(label).size(14.0);
                        rt = match key { "bold" => rt.strong(), "italic" => rt.italics(), "underline" => rt.underline(), _ => rt.strikethrough() };
                        let r = ui.add(egui::Button::new(rt).selected(on).frame_when_inactive(false).min_size(vec2(28.0, 28.0))).on_hover_text(tr(tip));
                        if r.clicked() { toggle_flag(app, key); }
                    }
                    ui.separator();
                    for (a, icon, tip) in [("left", "align-left", "Align left"), ("center", "align-center", "Center"), ("right", "align-right", "Align right")] {
                        if icon_button(ui, icon, &tr(tip), crate::canvas::align_of(&first, &first.text) == a).clicked() { app.edit_selected(|i| i.text_align = Some(a.into())); }
                    }
                    if !all(&["table"]) {
                        let kind = list_kind(&first);
                        let r = icon_button(ui, "list", &tr("List"), kind.is_some());
                        crate::theme::menu(&r, |ui| {
                            for (k, name) in [(None, "No list"), (Some("bullet"), "Bulleted list"), (Some("number"), "Numbered list")] {
                                if ui.selectable_label(kind.as_deref() == k, tr(name)).clicked() { app.edit_selected(|i| set_list(i, k)); ui.close(); }
                            }
                        });
                    }
                }
                if all(&["table"]) && items.len() == 1 {
                    let r = ui.add(egui::Button::new(tr("Rows & columns")).frame_when_inactive(false).min_size(vec2(0.0, 28.0)));
                    crate::theme::menu(&r, |ui| table_menu(app, ui));
                }
                if images && items.len() == 1 && icon_button(ui, "image-options", &tr("Crop, alternative text, replace"), false).clicked() { app.panel = Panel::Image(crate::app::ImageOptions::of(&first)); }
                if items.len() == 1 && app.board.items.iter().any(|c| c.mind_parent() == Some(first.id.as_str())) {
                    let collapsed = first.flag("mindCollapsed");
                    if icon_button(ui, if collapsed { "expand" } else { "collapse" }, &tr(if collapsed { "Expand branch" } else { "Collapse branch" }), false).clicked() { crate::commands::run(app, "collapse"); }
                }
                ui.separator();
                if !lines && !pens {
                    if icon_button(ui, "tag", &tr("Tags"), first.get("tagIds").is_some()).clicked() { app.panel = Panel::Tags; }
                    let link = first.str("link").map(str::to_string);
                    if icon_button(ui, "link", &tr("Link"), link.is_some()).clicked() { app.panel = Panel::Link(link.unwrap_or_default()); }
                }
                if icon_button(ui, "lock", &tr("Lock (Ctrl/Cmd Shift L)"), false).clicked() { app.toggle_lock(); }
                if icon_button(ui, "trash", &tr("Delete"), false).clicked() { app.delete_selection(); }
                let more = icon_button(ui, "more", &tr("More"), false);
                crate::theme::menu(&more, |ui| context_menu(app, ui));
            });
        });
    });
}

fn style_choices(app: &mut BoardApp, ui: &mut Ui, current: &str) {
    crate::rtl::row(ui, |ui| {
        for (k, name) in [("solid", "Solid"), ("dashed", "Dashed"), ("dotted", "Dotted")] {
            if ui.selectable_label(current == k, tr(name)).clicked() { app.edit_selected(|i| i.set("lineStyle", if k == "solid" { serde_json::Value::Null } else { k.into() })); }
        }
    });
}

fn table_menu(app: &mut BoardApp, ui: &mut Ui) {
    let (r, c) = app.cell.unwrap_or((0, 0));
    let id = app.selection[0].clone();
    let act = |app: &mut BoardApp, row: bool, at: usize, remove: bool| {
        app.checkpoint();
        if let Some(i) = app.board.find_mut(&id) { saboura_engine::edit::table_insert(i, row, at, remove); }
        app.changed();
    };
    if crate::rtl::item(ui, &tr("Insert row above"), "", true).clicked() { act(app, true, r, false); ui.close(); }
    if crate::rtl::item(ui, &tr("Insert row below"), "", true).clicked() { act(app, true, r + 1, false); ui.close(); }
    if crate::rtl::item(ui, &tr("Insert column left"), "", true).clicked() { act(app, false, c, false); ui.close(); }
    if crate::rtl::item(ui, &tr("Insert column right"), "", true).clicked() { act(app, false, c + 1, false); ui.close(); }
    ui.separator();
    if crate::rtl::item(ui, &tr("Delete row"), "", true).clicked() { act(app, true, r, true); ui.close(); }
    if crate::rtl::item(ui, &tr("Delete column"), "", true).clicked() { act(app, false, c, true); ui.close(); }
}

const CAPS: [(&str, &str); 10] = [("none", "None"), ("arrow", "Arrowhead"), ("triangle", "Filled triangle"), ("triangle-open", "Open triangle"), ("diamond", "Filled diamond"), ("diamond-open", "Open diamond"), ("circle", "Dot"), ("circle-open", "Open circle"), ("bar", "Bar (one)"), ("crowfoot", "Crow's foot (many)")];

fn cap_symbol(kind: &str, start: bool) -> &'static str {
    match (kind, start) {
        ("none", _) => "—", ("arrow", false) => "→", ("arrow", true) => "←", ("triangle", false) => "▶", ("triangle", true) => "◀",
        ("triangle-open", false) => "▷", ("triangle-open", true) => "◁", ("diamond", _) => "◆", ("diamond-open", _) => "◇",
        ("circle", _) => "●", ("circle-open", _) => "○", ("bar", _) => "┤", _ => "⋔",
    }
}

fn color_button(ui: &mut Ui, color: &str, hollow: bool, tip: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    if resp.hovered() { ui.painter().rect_filled(r, 7.0, c_hover()); }
    let c = hex(Some(color), Color32::GRAY);
    if hollow { ui.painter().circle_stroke(r.center(), 7.5, Stroke::new(3.0, c)); } else { ui.painter().circle(r.center(), 9.0, c, Stroke::new(1.0, c_line())); }
    resp.on_hover_text(tip)
}

fn color_grid(ui: &mut Ui, colors: &[&str], current: &str, hollow: bool) -> Option<String> {
    let mut picked = None;
    crate::toolbar::Grid::new(("colors", hollow, colors.len())).spacing(vec2(2.0, 2.0)).show(ui, |ui| {
        for (n, c) in colors.iter().enumerate() {
            if swatch(ui, c, current.eq_ignore_ascii_case(c), hollow) { picked = Some(c.to_string()); }
            if n % 7 == 6 { ui.end_row(); }
        }
    });
    // Any colour, by the system picker.
    let mut rgb = hex(Some(current), Color32::GRAY);
    crate::rtl::row(ui, |ui| {
        ui.label(tr("Custom"));
        if ui.color_edit_button_srgba(&mut rgb).changed() { picked = Some(format!("#{:02x}{:02x}{:02x}", rgb.r(), rgb.g(), rgb.b())); }
    });
    picked
}

fn cap(s: &str) -> String { let mut c = s.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }

fn bottom_bar(app: &mut BoardApp, ui: &mut Ui, board: Rect) {
    let (at, pivot) = crate::rtl::corner(board, false, false, vec2(8.0, 8.0));
    egui::Area::new(egui::Id::unique("bottom-bar")).pivot(pivot).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        panel_frame(ui).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
            crate::rtl::row(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                let (u, r) = (app.history.can_undo(), app.history.can_redo());
                if icon_button_enabled(ui, "undo", &tr("Undo (Ctrl/Cmd Z)"), false, u).clicked() { app.undo(); }
                if icon_button_enabled(ui, "redo", &tr("Redo (Ctrl/Cmd Shift Z)"), false, r).clicked() { app.redo(); }
                ui.separator();
                if icon_button(ui, "minus", &tr("Zoom out"), false).clicked() { app.zoom_by(false); }
                if ui.add(egui::Button::new(format!("{:.0}%", app.view.z * 100.0)).frame_when_inactive(false).min_size(vec2(52.0, 28.0))).on_hover_text(tr("Reset zoom to 100 percent")).clicked() { app.zoom_reset(); }
                if icon_button(ui, "plus", &tr("Zoom in"), false).clicked() { app.zoom_by(true); }
                if icon_button(ui, "fit", &tr("Fit board to screen (Shift 1)"), false).clicked() { app.fit(); }
                ui.separator();
                if icon_button(ui, "frame", &tr("Frames"), app.side == crate::app::Side::Frames).clicked() { app.toggle_side(crate::app::Side::Frames); }
                if icon_button(ui, "comment", &tr("Comments"), app.side == crate::app::Side::Comments).clicked() { app.toggle_side(crate::app::Side::Comments); }
                if icon_button(ui, "map", &tr("Minimap (M)"), app.prefs.minimap).clicked() { app.prefs.minimap = !app.prefs.minimap; app.prefs.save(); }
                if icon_button(ui, "grid", &tr("Grid"), app.prefs.grid).clicked() { app.prefs.grid = !app.prefs.grid; app.prefs.save(); }
                let dark = app.prefs.dark;
                if icon_button(ui, if dark { "sun" } else { "moon" }, &tr(if dark { "Light mode" } else { "Dark mode" }), false).clicked() { app.set_dark(!dark); }
            });
        });
    });
}

/// The whole board in miniature, with the view's outline; a press or drag moves the view.
fn minimap(app: &mut BoardApp, ui: &mut Ui, board: Rect) {
    let size = vec2(200.0, 130.0);
    let (at, pivot) = crate::rtl::corner(board, false, false, vec2(8.0, 56.0));
    egui::Area::new(egui::Id::unique("minimap")).pivot(pivot).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        panel_frame(ui).show(ui, |ui| {
            let (r, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let p = ui.painter_at(r);
            p.rect_filled(r, 6.0, crate::canvas::board_bg(app.prefs.dark));
            let view_box = { let a = app.view.to_board((0.0, 0.0)); let b = app.view.to_board((board.width() as f64, board.height() as f64)); geometry::Box2::from_points(a, b) };
            let content = geometry::bounds(&app.board.items);
            let (x0, y0) = (content.x.min(view_box.x), content.y.min(view_box.y));
            let (x1, y1) = ((content.x + content.w).max(view_box.x + view_box.w), (content.y + content.h).max(view_box.y + view_box.h));
            let s = ((size.x as f64 - 12.0) / (x1 - x0).max(1.0)).min((size.y as f64 - 12.0) / (y1 - y0).max(1.0));
            let off = vec2((size.x - ((x1 - x0) * s) as f32) / 2.0, (size.y - ((y1 - y0) * s) as f32) / 2.0);
            let to = |x: f64, y: f64| r.min + off + vec2(((x - x0) * s) as f32, ((y - y0) * s) as f32);
            for i in geometry::stacked(&app.board.items) {
                let b = geometry::object_bounds(i);
                let c = match i.kind.as_str() { "frame" => c_line(), "arrow" | "pen" => c_ink3(), _ => hex(i.color.as_deref(), c_ink3()) };
                p.rect_filled(Rect::from_min_max(to(b.x, b.y), to(b.x + b.w, b.y + b.h)).expand(0.5), 1.0, c);
            }
            p.rect_stroke(Rect::from_min_max(to(view_box.x, view_box.y), to(view_box.x + view_box.w, view_box.y + view_box.h)), 2.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Middle);
            if (resp.clicked() || resp.dragged()) && let Some(q) = resp.interact_pointer_pos() {
                let bx = x0 + ((q.x - r.min.x - off.x) as f64) / s;
                let by = y0 + ((q.y - r.min.y - off.y) as f64) / s;
                app.view.x = board.width() as f64 / 2.0 - bx * app.view.z;
                app.view.y = board.height() as f64 / 2.0 - by * app.view.z;
            }
        });
    });
}

pub fn context_menu(app: &mut BoardApp, ui: &mut Ui) {
    let has = !app.selection.is_empty();
    let center = app.view_center();
    if crate::rtl::item(ui, &tr("Cut"), "", has).clicked() { app.cut(); ui.close(); }
    if crate::rtl::item(ui, &tr("Copy"), "", has).clicked() { app.copy(); ui.close(); }
    if crate::rtl::item(ui, &tr("Paste"), "", true).clicked() { app.paste(None, center); ui.close(); }
    if crate::rtl::item(ui, &tr("Duplicate"), "", has).clicked() { app.duplicate(); ui.close(); }
    ui.separator();
    if crate::rtl::item(ui, &tr("Bring to front"), "", has).clicked() { app.bring_to_front(true); ui.close(); }
    if crate::rtl::item(ui, &tr("Send to back"), "", has).clicked() { app.bring_to_front(false); ui.close(); }
    if crate::rtl::item(ui, &tr("Group"), "", app.selection.len() > 1).clicked() { app.group(true); ui.close(); }
    if crate::rtl::item(ui, &tr("Ungroup"), "", has).clicked() { app.group(false); ui.close(); }
    let locked = has && app.selected_items().iter().all(|i| i.is_locked());
    if crate::rtl::item(ui, &tr(if locked { "Unlock" } else { "Lock" }), "", has).clicked() { app.toggle_lock(); ui.close(); }
    if app.selection.len() > 1 {
        crate::theme::submenu(ui, tr("Align"), |ui| {
            for id in ["align-left", "align-center", "align-right", "align-top", "align-middle", "align-bottom", "distribute-h", "distribute-v"] {
                if crate::rtl::item(ui, &crate::commands::label(id), "", crate::commands::enabled(app, id)).clicked() { crate::commands::run(app, id); ui.close(); }
            }
        });
    }
    if app.single().is_some_and(|i| geometry::can_rotate(i) && i.rotation() != 0.0) && crate::rtl::item(ui, &tr("Reset rotation"), "", true).clicked() { app.edit_selected(|i| i.rotation = None); ui.close(); }
    if app.single().is_some_and(|i| i.is_mind() || saboura_engine::edit::can_quick_connect(i)) && crate::rtl::item(ui, &tr("Add branch (Tab)"), "", true).clicked() { app.add_branch(false); ui.close(); }
    if app.single().is_some_and(|i| i.kind == "image") && crate::rtl::item(ui, &crate::commands::label("image-options"), "", true).clicked() { crate::commands::run(app, "image-options"); ui.close(); }
    if has && crate::rtl::item(ui, &crate::commands::label("tags"), "", true).clicked() { app.panel = Panel::Tags; ui.close(); }
    if crate::commands::enabled(app, "collapse") && crate::rtl::item(ui, &crate::commands::label("collapse"), "", true).clicked() { crate::commands::run(app, "collapse"); ui.close(); }
    ui.separator();
    if crate::rtl::item(ui, &tr("Delete"), "", has).clicked() { app.delete_selection(); ui.close(); }
    if crate::rtl::item(ui, &tr("Select all"), "", true).clicked() { app.select_all(); ui.close(); }
}

/// egui's grid, laid out start to end even in a right-to-left interface (egui's own grid
/// panics there).
pub struct Grid { inner: egui::Grid }

impl Grid {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug) -> Grid { Grid { inner: egui::Grid::new(id) } }
    pub fn spacing(self, s: impl Into<egui::Vec2>) -> Grid { Grid { inner: self.inner.spacing(s) } }
    pub fn striped(self, on: bool) -> Grid { Grid { inner: self.inner.striped(on) } }
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| self.inner.show(ui, add).inner)
    }
}
