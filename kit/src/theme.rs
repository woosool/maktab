//! The app's widget theme and dialog chrome: square corners, roomy padding, clear focus and
//! selection, and dialogs that open centered over a dimmed, click-blocking backdrop with a title
//! row and a right-aligned footer whose primary button is filled with the accent color.

use crate::colors::{ACCENT, c_card, c_frame, c_hover, c_ink, c_ink2, c_ink3, c_line, c_press};
use egui::{vec2, Align, Color32, CornerRadius, FontId, Id, Layout, Margin, Order, RichText, Sense, Stroke, Ui};

/// Corner radius of every widget, menu and window (0 = square, 90° corners).
pub const RADIUS: u8 = 0;
/// A lighter accent for selected items and hovered menu rows.
pub const SELECT: Color32 = Color32::from_rgb(0xDE, 0xEB, 0xFA);
const ACCENT_HOVER: Color32 = Color32::from_rgb(0x12, 0x4A, 0x9E);
const FIELD_LINE: Color32 = Color32::from_rgb(0xC4, 0xC4, 0xC4);

/// Light chrome from the JS editor's tokens (editor-chrome.css).
pub fn visuals() -> egui::Visuals {
    let r = CornerRadius::same(RADIUS);
    let mut v = egui::Visuals::light();
    v.panel_fill = c_frame();
    v.window_fill = c_card();
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_rgb(0xF7, 0xF7, 0xF7);
    v.code_bg_color = Color32::from_rgb(0xF5, 0xF5, 0xF5);
    v.window_corner_radius = r;
    v.menu_corner_radius = r;
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(0xD0, 0xD0, 0xD0));
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(46) };
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(36) };
    v.selection.bg_fill = SELECT;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.text_cursor.stroke = Stroke::new(1.5, ACCENT);
    v.striped = false;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.6 };
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open, &mut v.widgets.noninteractive] {
        w.corner_radius = r;
        w.expansion = 0.0;
    }
    // Resting: white fields with a visible gray outline, like Office's text boxes and drop-downs.
    v.widgets.inactive.weak_bg_fill = Color32::WHITE;
    v.widgets.inactive.bg_fill = Color32::WHITE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, c_ink2());
    v.widgets.hovered.weak_bg_fill = c_hover();
    v.widgets.hovered.bg_fill = Color32::WHITE;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x9A, 0x9A, 0x9A));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, c_ink());
    v.widgets.active.weak_bg_fill = c_press();
    v.widgets.active.bg_fill = Color32::WHITE;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, c_ink());
    // An open drop-down keeps the accent outline while its list shows.
    v.widgets.open.weak_bg_fill = Color32::WHITE;
    v.widgets.open.bg_fill = Color32::WHITE;
    v.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.open.fg_stroke = Stroke::new(1.0, c_ink());
    v.widgets.noninteractive.bg_fill = c_card();
    v.widgets.noninteractive.weak_bg_fill = c_card();
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, c_line());
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, c_ink());
    v
}

/// Switch between the light theme and Dark Mode.
pub fn apply_mode(ctx: &egui::Context, dark: bool) {
    ctx.all_styles_mut(|s| {
        if dark {
            let mut v = egui::Visuals::dark();
            let r = CornerRadius::same(RADIUS);
            v.window_corner_radius = r; v.menu_corner_radius = r;
            let (frame, card, field, ink, line) = (Color32::from_gray(0x1F), Color32::from_gray(0x2B), Color32::from_gray(0x33), Color32::from_gray(0xF2), Color32::from_gray(0x55));
            v.panel_fill = frame;
            v.window_fill = card;
            v.extreme_bg_color = field;
            v.faint_bg_color = Color32::from_gray(0x30);
            v.window_stroke = Stroke::new(1.0, Color32::from_gray(0x48));
            v.selection.bg_fill = Color32::from_rgb(0x26, 0x4F, 0x78);
            v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0x7F, 0xB2, 0xF0));
            v.hyperlink_color = Color32::from_rgb(0x7F, 0xB2, 0xF0);
            for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open, &mut v.widgets.noninteractive] { w.corner_radius = r; w.expansion = 0.0; }
            v.widgets.inactive.weak_bg_fill = field; v.widgets.inactive.bg_fill = field;
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, line);
            v.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_gray(0xDC));
            v.widgets.hovered.weak_bg_fill = Color32::from_gray(0x3A); v.widgets.hovered.bg_fill = field;
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(0x80));
            v.widgets.hovered.fg_stroke = Stroke::new(1.0, ink);
            v.widgets.active.weak_bg_fill = Color32::from_gray(0x4A); v.widgets.active.bg_fill = field;
            v.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x7F, 0xB2, 0xF0));
            v.widgets.open.weak_bg_fill = field; v.widgets.open.bg_fill = field;
            v.widgets.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x7F, 0xB2, 0xF0));
            v.widgets.noninteractive.bg_fill = card; v.widgets.noninteractive.weak_bg_fill = card;
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0x40));
            v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ink);
            s.visuals = v;
        } else { s.visuals = visuals(); }
    });
}

/// Apply the theme: visuals plus spacing (padding inside buttons and fields, gaps between rows).
pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.visuals = visuals();
        let sp = &mut s.spacing;
        sp.item_spacing = vec2(8.0, 7.0);
        sp.button_padding = vec2(12.0, 5.0);
        sp.interact_size = vec2(64.0, 26.0);
        sp.icon_width = 16.0;
        sp.icon_width_inner = 9.0;
        sp.icon_spacing = 7.0;
        sp.combo_width = 150.0;
        sp.text_edit_width = 220.0;
        sp.window_margin = Margin::same(0);
        sp.menu_margin = Margin::same(4);
        sp.indent = 20.0;
        sp.combo_height = 280.0;
        s.interaction.selectable_labels = false;
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(17.0));
    });
}

/// A dialog window: centered, movable, with its own title row and a close button.
thread_local! {
    /// Set while a dialog's range field is picking cells: dialogs aren't drawn and don't block the window.
    pub static PICKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub struct Dialog<'a> {
    title: String,
    open: Option<&'a mut bool>,
    resizable: bool,
    width: Option<f32>,
}

/// Start a dialog (replaces `egui::Window::new`).
pub fn dialog<'a>(_ctx: &egui::Context, title: impl Into<String>) -> Dialog<'a> {
    Dialog { title: crate::i18n::tr(&title.into()), open: None, resizable: false, width: None }
}

impl<'a> Dialog<'a> {
    pub fn open(mut self, open: &'a mut bool) -> Self { self.open = Some(open); self }
    pub fn collapsible(self, _: bool) -> Self { self }
    pub fn resizable(mut self, r: bool) -> Self { self.resizable = r; self }
    pub fn default_width(mut self, w: f32) -> Self { self.width = Some(w); self }

    pub fn show<R>(self, ctx: &egui::Context, add: impl FnOnce(&mut Ui) -> R) -> Option<egui::InnerResponse<Option<R>>> {
        let Dialog { title, mut open, resizable, width } = self;
        if open.as_deref() == Some(&false) { return None; }
        // While a range field is picking cells on the sheet, dialogs step aside (Excel's collapsed RefEdit).
        if PICKING.with(|p| p.get()) { return None; }
        backdrop(ctx);
        let screen = ctx.content_rect();
        let id = Id::unique(("dialog", title.as_str()));
        let frame = egui::Frame::window(&ctx.global_style()).inner_margin(Margin::ZERO).corner_radius(CornerRadius::same(RADIUS));
        let mut close = false;
        let r = egui::Window::new(title.as_str())
            .id(id)
            .title_bar(false)
            .collapsible(false)
            .resizable(resizable)
            .frame(frame)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(screen.center() - vec2(0.0, screen.height() * 0.06))
            .min_width(width.unwrap_or(380.0).min(screen.width() - 32.0))
            .default_width(width.unwrap_or(380.0))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                // The title and button rows stretch to the window's width; left alone that width is
                // the whole screen, and auto-sized dialogs grew to it. Cap the window at what the
                // content needed last frame (content that needs more still widens it).
                let wkey = id.with("content-w");
                if !resizable {
                    let need = ctx.data(|d| d.get_temp::<f32>(wkey)).unwrap_or(width.unwrap_or(380.0) - 40.0);
                    ui.set_max_width((need + 40.0).max(width.unwrap_or(380.0)).min(screen.width() - 32.0));
                }
                // Title row: the heading on the left, × on the right; dragging it moves the window.
                egui::Frame::NONE.inner_margin(Margin { left: 20, right: 10, top: 12, bottom: 8 }).show(ui, |ui| {
                    // The title on the reading side, × on the other.
                    let rtl = crate::i18n::rtl();
                    let w = ui.available_width();
                    ui.allocate_ui_with_layout(egui::vec2(w, 30.0), if rtl { Layout::right_to_left(Align::Center) } else { Layout::left_to_right(Align::Center) }, |ui| {
                        ui.label(RichText::new(title.as_str()).size(16.0).color(c_ink()).family(bold()));
                        ui.with_layout(if rtl { Layout::left_to_right(Align::Center) } else { Layout::right_to_left(Align::Center) }, |ui| {
                            if open.is_some() && close_glyph(ui).clicked() { close = true; }
                        });
                    });
                });
                egui::Frame::NONE.inner_margin(Margin { left: 20, right: 20, top: 4, bottom: 18 }).show(ui, |ui| {
                    let r = if crate::i18n::rtl() { ui.with_layout(Layout::top_down(Align::Max), add).inner } else { add(ui) };
                    let used = ui.min_rect().width();
                    ui.ctx().data_mut(|d| d.insert_temp(wkey, used));
                    r
                }).inner
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && open.is_some() { close = true; }
        if close { if let Some(o) = open.as_deref_mut() { *o = false; } }
        r
    }
}

/// The × in a dialog's title row.
fn close_glyph(ui: &mut Ui) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if r.hovered() { ui.painter().rect_filled(rect, CornerRadius::same(RADIUS), c_hover()); }
    let c = rect.center();
    let s = Stroke::new(1.4, if r.hovered() { c_ink() } else { c_ink3() });
    ui.painter().line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], s);
    ui.painter().line_segment([c + vec2(5.0, -5.0), c + vec2(-5.0, 5.0)], s);
    r.on_hover_text(crate::i18n::tr("Close"))
}

/// Dim the window behind dialogs and swallow clicks meant for it.
fn backdrop(ctx: &egui::Context) {
    let screen = ctx.content_rect();
    egui::Area::new(Id::unique("dialog-backdrop")).order(Order::Middle).fixed_pos(screen.min).interactable(true).show(ctx, |ui| {
        ui.painter().rect_filled(screen, CornerRadius::ZERO, Color32::from_black_alpha(56));
        ui.allocate_rect(screen, Sense::click_and_drag());
    });
}

/// A dialog's button row: a separator line, then buttons right-aligned. Add the primary button
/// first; it lands rightmost, with the others to its left.
pub fn footer<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(4.0);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 30.0), Layout::right_to_left(Align::Center), |ui| { ui.spacing_mut().item_spacing.x = 8.0; add(ui) }).inner
}

/// The dialog's main action: accent-filled with white text.
pub fn primary(ui: &mut Ui, label: impl Into<String>) -> egui::Response {
    primary_enabled(ui, true, label)
}

pub fn primary_enabled(ui: &mut Ui, enabled: bool, label: impl Into<String>) -> egui::Response {
    let text = RichText::new(crate::i18n::tr(&label.into())).color(Color32::WHITE).family(bold());
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = ACCENT;
        w.inactive.bg_stroke = Stroke::new(1.0, ACCENT);
        w.hovered.weak_bg_fill = ACCENT_HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT_HOVER);
        w.active.weak_bg_fill = ACCENT_HOVER;
        ui.add_enabled(enabled, egui::Button::new(text).min_size(vec2(84.0, 30.0)))
    }).inner
}

/// A secondary dialog button with the same size as the primary one.
pub fn secondary(ui: &mut Ui, label: &str) -> egui::Response {
    ui.add(egui::Button::new(crate::i18n::tr(label)).min_size(vec2(84.0, 30.0)))
}

fn bold() -> egui::FontFamily { egui::FontFamily::Name("ui-bold".into()) }

/// A bold section heading inside a dialog, with space above it.
pub fn section(ui: &mut Ui, text: &str) {
    let text = &crate::i18n::tr(text);
    ui.add_space(6.0);
    ui.label(RichText::new(text).color(c_ink()).family(bold()));
    ui.add_space(2.0);
}

thread_local! {
    /// Inside a drop-down menu: ribbon buttons stretch to the menu's width.
    pub static IN_MENU: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A drop-down menu under a button, with every item as wide as the menu.
pub fn menu<R>(button: &egui::Response, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    // In right-to-left, a drop-down hangs from the button's right edge.
    let mut popup = egui::Popup::menu(button);
    if crate::i18n::rtl() { popup = popup.align(egui::RectAlign::BOTTOM_END).align_alternatives(&[egui::RectAlign::TOP_END, egui::RectAlign::BOTTOM_START]); }
    popup.show(|ui| full_width(ui, add)).map(|r| r.inner)
}

/// A submenu row. In right-to-left it opens to the left, with its arrow on the left.
pub fn submenu<R>(ui: &mut Ui, label: impl Into<RichText>, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let rich: RichText = label.into();
    if !crate::i18n::rtl() { return ui.menu_button(rich, add).inner; }
    let label = rich.text().to_string();
    let id = ui.make_persistent_id(("rtl-submenu", &label));
    // Which submenu of this menu is open (opening one closes its siblings).
    let active_id = ui.id().with("rtl-submenu-active");
    let active: Option<Id> = ui.data(|d| d.get_temp(active_id));
    let h = 28.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(160.0), h), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    let open_now = active == Some(id);
    if resp.hovered() || open_now { ui.painter().rect_filled(rect, CornerRadius::same(4), c_hover()); }
    ui.painter().text(egui::pos2(rect.max.x - 10.0, rect.center().y), egui::Align2::RIGHT_CENTER, &label, FontId::proportional(13.0), c_ink());
    // A left-pointing arrow.
    let c = egui::pos2(rect.min.x + 12.0, rect.center().y);
    ui.painter().add(egui::Shape::convex_polygon(vec![c + vec2(-3.0, 0.0), c + vec2(2.0, -4.0), c + vec2(2.0, 4.0)], c_ink2(), Stroke::NONE));
    if resp.hovered() || resp.clicked() { ui.data_mut(|d| d.insert_temp(active_id, id)); }
    else if open_now && ui.ui_contains_pointer() && !resp.contains_pointer() {
        // The pointer moved onto another item of this menu.
        let over_popup = ui.ctx().layer_id_at(ui.ctx().pointer_hover_pos().unwrap_or_default()).is_some_and(|l| l.id == id.with("pop"));
        if !over_popup { ui.data_mut(|d| d.remove::<Id>(active_id)); }
    }
    let open = ui.data(|d| d.get_temp::<Id>(active_id)) == Some(id);
    let mut close_parent = false;
    let r = egui::Popup::from_response(&resp).id(id.with("pop")).kind(egui::PopupKind::Menu).open(open)
        .align(egui::RectAlign::LEFT_START).align_alternatives(&[egui::RectAlign::RIGHT_START]).gap(2.0)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .show(|ui| { let r = full_width(ui, add); if ui.will_parent_close() { close_parent = true; } r })
        .map(|r| r.inner);
    if close_parent { ui.data_mut(|d| d.remove::<Id>(active_id)); ui.close(); }
    r
}

/// A drop-down field (egui's ComboBox) that mirrors in right-to-left: the text on the right,
/// the arrow on the left, and the list hanging from the right edge.
pub struct Combo<'a> { id: Id, salt: egui::Id, width: Option<f32>, height: Option<f32>, text: egui::WidgetText, _p: std::marker::PhantomData<&'a ()> }

impl<'a> Combo<'a> {
    pub fn from_id_salt(salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        let salt = Id::new(salt);
        Combo { id: salt, salt, width: None, height: None, text: egui::WidgetText::default(), _p: Default::default() }
    }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = Some(h); self }
    pub fn selected_text(mut self, t: impl Into<egui::WidgetText>) -> Self { self.text = t.into(); self }
    pub fn show_ui<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if !crate::i18n::rtl() {
            let mut c = egui::ComboBox::from_id_salt(self.salt).selected_text(self.text);
            if let Some(w) = self.width { c = c.width(w); }
            if let Some(h) = self.height { c = c.height(h); }
            return c.show_ui(ui, add).inner;
        }
        let id = ui.make_persistent_id(self.id);
        let w = self.width.unwrap_or(ui.spacing().combo_width);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, ui.spacing().interact_size.y.max(22.0)), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, self.text.text()));
        let v = ui.style().interact(&resp);
        ui.painter().rect(rect, v.corner_radius, v.weak_bg_fill, v.bg_stroke, egui::StrokeKind::Inside);
        let galley = self.text.into_galley(ui, Some(egui::TextWrapMode::Truncate), w - 28.0, egui::TextStyle::Button);
        ui.painter().galley(egui::pos2(rect.max.x - 8.0 - galley.size().x, rect.center().y - galley.size().y / 2.0), galley, v.text_color());
        let c = egui::pos2(rect.min.x + 11.0, rect.center().y);
        ui.painter().add(egui::Shape::convex_polygon(vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)], v.fg_stroke.color, Stroke::NONE));
        let mut popup = egui::Popup::menu(&resp).id(id.with("popup")).align(egui::RectAlign::BOTTOM_END).align_alternatives(&[egui::RectAlign::TOP_END]).width(w);
        popup = popup.close_behavior(egui::PopupCloseBehavior::CloseOnClick);
        let h = self.height.unwrap_or(300.0);
        popup.show(|ui| {
            ui.set_min_width(w);
            egui::ScrollArea::vertical().max_height(h).show(ui, |ui| {
                ui.with_layout(Layout::top_down_justified(Align::Max), add).inner
            }).inner
        }).map(|r| r.inner)
    }
}

/// Lay out menu items full width (selectable rows, buttons and ribbon items).
pub fn full_width<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.set_min_width(180.0);
    // Menus never stretch across the window (full-width rows would otherwise widen them).
    ui.set_max_width(ui.min_rect().width().max(280.0));
    // Right-aligned rows need a definite right edge: a popup's is otherwise unbounded (its
    // positions would be infinite, then NaN).
    if crate::i18n::rtl() { ui.set_width(260.0); }
    let outer = IN_MENU.with(|m| m.replace(true));
    let align = if crate::i18n::rtl() { Align::RIGHT } else { Align::LEFT };
    let r = ui.with_layout(Layout::top_down_justified(align), |ui| {
        ui.spacing_mut().button_padding = egui::vec2(10.0, 5.0);
        add(ui)
    }).inner;
    IN_MENU.with(|m| m.set(outer));
    r
}

// ---------------------------------------------------------------------------
// Right-to-left building blocks: in Arabic, rows run right to left, checkboxes and radio
// buttons put their box on the right, and form grids list label then field from the right.

/// A row of widgets in the interface's reading direction (replaces `ui.horizontal`).
pub fn hrow<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if crate::i18n::rtl() {
        // A row as tall as a widget (it grows if its content is taller), as wide as the column.
        let h = ui.spacing().interact_size.y;
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), h), Layout::right_to_left(Align::Center), add)
    } else { ui.horizontal(add) }
}

/// A column whose rows align to the reading side (replaces `ui.vertical`).
pub fn vcol<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if crate::i18n::rtl() { ui.with_layout(Layout::top_down(Align::Max), add) } else { ui.vertical(add) }
}

/// Columns side by side, top-aligned, in reading order (replaces `ui.horizontal_top`).
pub fn hcols<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if crate::i18n::rtl() { ui.with_layout(Layout::right_to_left(Align::Min), add) } else { ui.horizontal_top(add) }
}

/// A form grid; in Arabic, a stacked form aligned right (each label above its field), since
/// egui's grids only run left to right.
pub fn grid<R>(ui: &mut Ui, grid: egui::Grid, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if crate::i18n::rtl() {
        let w = ui.available_width();
        ui.allocate_ui_with_layout(egui::vec2(w, 0.0), Layout::right_to_left(Align::Min).with_main_wrap(true), |ui| { ui.set_max_width(w); ui.spacing_mut().item_spacing.y = 8.0; add(ui) })
    } else { grid.show(ui, add) }
}

fn mirrored_choice(ui: &mut Ui, on: bool, text: egui::WidgetText, round: bool) -> egui::Response {
    let s = ui.spacing().icon_width;
    let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
    let size = egui::vec2(s + ui.spacing().icon_spacing + galley.size().x, galley.size().y.max(s).max(ui.spacing().interact_size.y));
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    let v = ui.style().interact_selectable(&r, on);
    let bx = egui::Rect::from_center_size(egui::pos2(rect.max.x - s / 2.0, rect.center().y), egui::vec2(s, s));
    if round {
        ui.painter().circle(bx.center(), s / 2.0, v.bg_fill, v.bg_stroke);
        if on { ui.painter().circle_filled(bx.center(), s / 4.0, v.fg_stroke.color); }
    } else {
        ui.painter().rect(bx, CornerRadius::same(RADIUS), ui.visuals().widgets.inactive.bg_fill, ui.visuals().widgets.inactive.bg_stroke, egui::StrokeKind::Inside);
        if on {
            let c = bx.center();
            ui.painter().line(vec![c + egui::vec2(-4.0, 0.0), c + egui::vec2(-1.0, 3.0), c + egui::vec2(4.0, -3.5)], Stroke::new(1.6, ui.visuals().widgets.active.fg_stroke.color));
        }
    }
    ui.painter().galley(egui::pos2(bx.min.x - ui.spacing().icon_spacing - galley.size().x, rect.center().y - galley.size().y / 2.0), galley, ui.visuals().text_color());
    r
}

/// A checkbox; in Arabic the box sits to the right of its label.
pub fn check(ui: &mut Ui, value: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    if !crate::i18n::rtl() { return ui.checkbox(value, text); }
    let mut r = mirrored_choice(ui, *value, text.into(), false);
    if r.clicked() { *value = !*value; r.mark_changed(); }
    r
}

/// A radio button bound to a value; mirrored in Arabic.
pub fn radio_value<V: PartialEq>(ui: &mut Ui, current: &mut V, alternative: V, text: impl Into<egui::WidgetText>) -> egui::Response {
    if !crate::i18n::rtl() { return ui.radio_value(current, alternative, text); }
    let mut r = mirrored_choice(ui, *current == alternative, text.into(), true);
    if r.clicked() && *current != alternative { *current = alternative; r.mark_changed(); }
    r
}

/// A radio button showing a state; mirrored in Arabic.
pub fn radio(ui: &mut Ui, on: bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    if !crate::i18n::rtl() { return ui.radio(on, text); }
    mirrored_choice(ui, on, text.into(), true)
}
