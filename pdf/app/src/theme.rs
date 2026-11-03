//! Design tokens, widget styling, fonts and icons.

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke, Vec2};

/// Design tokens shared by every widget, in light and dark appearance.
#[derive(Clone, Copy)]
pub struct Palette {
    /// Title bar, toolbars and panels.
    pub chrome: Color32,
    /// Raised surfaces: floating controls, popups, cards.
    pub surface: Color32,
    /// The area behind the pages.
    pub canvas: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub input: Color32,
    pub line: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    /// Text and icons drawn on `accent`.
    pub on_accent: Color32,
    pub accent_soft: Color32,
    pub accent_text: Color32,
    /// Search matches and the current match.
    pub find: Color32,
    pub find_current: Color32,
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            chrome: Color32::from_rgb(29, 29, 28),
            surface: Color32::from_rgb(38, 38, 36),
            canvas: Color32::from_rgb(18, 18, 17),
            control: Color32::from_rgb(48, 48, 45),
            control_hover: Color32::from_rgb(60, 60, 56),
            input: Color32::from_rgb(22, 22, 21),
            line: Color32::from_rgb(52, 52, 49),
            border: Color32::from_rgb(70, 70, 66),
            border_strong: Color32::from_rgb(100, 100, 94),
            text: Color32::from_rgb(242, 241, 237),
            muted: Color32::from_rgb(170, 169, 159),
            accent: Color32::from_rgb(239, 146, 144),
            on_accent: Color32::from_rgb(20, 16, 16),
            accent_soft: Color32::from_rgb(70, 38, 38),
            accent_text: Color32::from_rgb(246, 170, 168),
            find: Color32::from_rgba_unmultiplied(255, 214, 0, 90),
            find_current: Color32::from_rgba_unmultiplied(255, 140, 0, 140),
        }
    } else {
        Palette {
            chrome: Color32::from_rgb(246, 245, 242),
            surface: Color32::WHITE,
            canvas: Color32::from_rgb(232, 231, 226),
            control: Color32::from_rgb(238, 237, 233),
            control_hover: Color32::from_rgb(226, 225, 220),
            input: Color32::WHITE,
            line: Color32::from_rgb(222, 221, 215),
            border: Color32::from_rgb(205, 204, 197),
            border_strong: Color32::from_rgb(160, 159, 152),
            text: Color32::from_rgb(21, 21, 21),
            muted: Color32::from_rgb(98, 97, 92),
            accent: Color32::from_rgb(191, 63, 63),
            on_accent: Color32::WHITE,
            accent_soft: Color32::from_rgb(251, 230, 229),
            accent_text: Color32::from_rgb(160, 40, 40),
            find: Color32::from_rgba_unmultiplied(255, 214, 0, 90),
            find_current: Color32::from_rgba_unmultiplied(255, 120, 0, 120),
        }
    }
}

pub fn visuals(dark: bool) -> egui::Visuals {
    let p = palette(dark);
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.muted);
    v.hyperlink_color = p.accent_text;
    v.panel_fill = p.chrome;
    v.window_fill = p.surface;
    v.faint_bg_color = p.canvas;
    v.extreme_bg_color = p.input;
    v.code_bg_color = p.control;
    v.text_edit_bg_color = Some(p.input);
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_highlight_topmost = false;
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 120 } else { 40 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 100 } else { 34 }),
    };
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent_text);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.striped = false;

    let radius = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.chrome;
    w.noninteractive.weak_bg_fill = p.chrome;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.muted);
    w.noninteractive.corner_radius = radius;

    w.inactive.weak_bg_fill = p.control;
    w.inactive.bg_fill = p.control;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.5, p.text);
    w.inactive.corner_radius = radius;
    w.inactive.expansion = 0.0;

    w.hovered.weak_bg_fill = p.control_hover;
    w.hovered.bg_fill = p.control_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.weak_bg_fill = p.accent_soft;
    w.active.bg_fill = p.accent_soft;
    w.active.bg_stroke = Stroke::new(1.5, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.weak_bg_fill = p.accent_soft;
    w.open.bg_fill = p.accent_soft;
    w.open.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.fg_stroke = Stroke::new(1.5, p.accent_text);
    w.open.corner_radius = radius;
    v
}

/// Installs light and dark styles; `ctx.set_theme` then picks one.
pub fn install(ctx: &egui::Context) {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let mut style = (*ctx.style_of(theme)).clone();
        style.visuals = visuals(theme == egui::Theme::Dark);
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.icon_width = 16.0;
        style.spacing.icon_width_inner = 9.0;
        style.spacing.icon_spacing = 8.0;
        style.spacing.combo_width = 140.0;
        style.spacing.text_edit_width = 220.0;
        style.spacing.window_margin = egui::Margin::same(14);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.scroll = egui::style::ScrollStyle::floating();
        style.interaction.selectable_labels = false;
        for (text, size) in [
            (egui::TextStyle::Body, 13.5),
            (egui::TextStyle::Button, 13.0),
            (egui::TextStyle::Heading, 17.0),
            (egui::TextStyle::Small, 11.0),
        ] {
            style
                .text_styles
                .insert(text, egui::FontId::proportional(size));
        }
        ctx.set_style_of(theme, style);
    }
}

pub const ICON_FAMILY: &str = "icons";
pub const BOLD_FAMILY: &str = "ui-bold";
pub const SCRIPT_FAMILY: &str = "script";

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // The bundled Liberation Sans (Arial metrics) has no Arabic glyphs, so Arabic text falls
    // back to Noto Sans Arabic in a run of its own, which egui shapes right to left.
    fonts.font_data.insert(
        "ui".into(),
        egui::FontData::from_static(pdf_engine::FontFace::Sans.data()).into(),
    );
    fonts.font_data.insert(
        "ui-bold-face".into(),
        egui::FontData::from_static(pdf_engine::FontFace::SansBold.data()).into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "ui".into());
    let mut bold = fonts.families[&egui::FontFamily::Proportional].clone();
    bold.insert(0, "ui-bold-face".into());
    // Symbols such as ⌘ ⇧ ↩ used in shortcut hints.
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/Apple Symbols.ttf") {
        fonts
            .font_data
            .insert("symbols".into(), egui::FontData::from_owned(bytes).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("symbols".into());
        }
        bold.push("symbols".into());
    }
    fonts
        .families
        .insert(egui::FontFamily::Name(BOLD_FAMILY.into()), bold);
    // Arabic fallback and the script face for typed signatures (bundled by the engine, OFL).
    fonts.font_data.insert(
        "arabic".into(),
        egui::FontData::from_static(pdf_engine::fallback_font_data()).into(),
    );
    for family in fonts.families.values_mut() {
        family.push("arabic".into());
    }
    fonts.font_data.insert(
        "script".into(),
        egui::FontData::from_static(pdf_engine::FontFace::Script.data()).into(),
    );
    let mut script = vec!["script".to_string()];
    script.extend(
        fonts.families[&egui::FontFamily::Proportional]
            .iter()
            .cloned(),
    );
    fonts
        .families
        .insert(egui::FontFamily::Name(SCRIPT_FAMILY.into()), script);
    // Phosphor icons (MIT), see assets/fonts/PHOSPHOR-LICENSE.
    fonts.font_data.insert(
        "phosphor".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Phosphor.ttf")).into(),
    );
    fonts.families.insert(
        egui::FontFamily::Name(ICON_FAMILY.into()),
        vec!["phosphor".into()],
    );
    ctx.set_fonts(fonts);
}

pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(BOLD_FAMILY.into()))
}

/// Icons used by the interface, drawn from the Phosphor Regular font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Open,
    Undo,
    Redo,
    Search,
    ZoomIn,
    ZoomOut,
    FitWidth,
    Up,
    Down,
    RotateCw,
    RotateCcw,
    Trash,
    Duplicate,
    BlankPage,
    Extract,
    Merge,
    Highlight,
    Underline,
    Strike,
    TextBox,
    Note,
    Rectangle,
    Ellipse,
    Ink,
    Select,
    Pages,
    Organize,
    Comments,
    Sun,
    Moon,
    Pdf,
    EditText,
    Calendar,
    Check,
    Signature,
    Close,
    Left,
    Right,
    Image,
}

impl Icon {
    pub fn glyph(self) -> &'static str {
        match self {
            Icon::Open => "\u{E256}",
            Icon::Undo => "\u{E08A}",
            Icon::Redo => "\u{E08C}",
            Icon::Search => "\u{E30C}",
            Icon::ZoomIn => "\u{E310}",
            Icon::ZoomOut => "\u{E30E}",
            Icon::FitWidth => "\u{EB06}",
            Icon::Up => "\u{E13C}",
            Icon::Down => "\u{E136}",
            Icon::RotateCw => "\u{E036}",
            Icon::RotateCcw => "\u{E038}",
            Icon::Trash => "\u{E4A6}",
            Icon::Duplicate => "\u{E1CA}",
            Icon::BlankPage => "\u{E236}",
            Icon::Extract => "\u{EAF0}",
            Icon::Merge => "\u{E710}",
            Icon::Highlight => "\u{EC76}",
            Icon::Underline => "\u{E5C4}",
            Icon::Strike => "\u{E5C2}",
            Icon::TextBox => "\u{E48A}",
            Icon::Note => "\u{E348}",
            Icon::Rectangle => "\u{E45E}",
            Icon::Ellipse => "\u{E18A}",
            Icon::Ink => "\u{E3B4}",
            Icon::Select => "\u{E1DC}",
            Icon::Pages => "\u{E710}",
            Icon::Organize => "\u{E464}",
            Icon::Comments => "\u{E17A}",
            Icon::Sun => "\u{E472}",
            Icon::Moon => "\u{E330}",
            Icon::Pdf => "\u{E702}",
            Icon::EditText => "\u{E7D8}",
            Icon::Calendar => "\u{E108}",
            Icon::Check => "\u{E182}",
            Icon::Signature => "\u{EBAC}",
            Icon::Close => "\u{E4F6}",
            Icon::Left => "\u{E138}",
            Icon::Right => "\u{E13A}",
            Icon::Image => "\u{E2CA}",
        }
    }
}

pub fn draw_icon(p: &egui::Painter, c: egui::Pos2, icon: Icon, color: Color32, size: f32) {
    p.text(
        c,
        egui::Align2::CENTER_CENTER,
        icon.glyph(),
        egui::FontId::new(size, egui::FontFamily::Name(ICON_FAMILY.into())),
        color,
    );
}

/// Square, frameless icon button with an accessible name and tooltip.
/// `selected` paints it as an on-state toggle.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    icon_button_sized(ui, icon, label, selected, 30.0)
}

pub fn icon_button_sized(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    side: f32,
) -> egui::Response {
    let p = palette(ui.visuals().dark_mode);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), egui::Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        if selected {
            egui::WidgetInfo::selected(egui::Role::Button, enabled, true, label)
        } else {
            egui::WidgetInfo::labeled(egui::Role::Button, enabled, label)
        }
    });
    let fill = if selected {
        p.accent_soft
    } else if response.hovered() && enabled {
        p.control_hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    let color = if !enabled {
        p.muted.gamma_multiply(0.5)
    } else if selected {
        p.accent_text
    } else {
        p.text
    };
    draw_icon(ui.painter(), rect.center(), icon, color, side * 0.6);
    response.on_hover_text(label)
}

/// A wide button with an icon and a label, used in tool lists.
pub fn tool_row(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let p = palette(ui.visuals().dark_mode);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), egui::Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        if selected {
            egui::WidgetInfo::selected(egui::Role::Button, enabled, true, label)
        } else {
            egui::WidgetInfo::labeled(egui::Role::Button, enabled, label)
        }
    });
    let fill = if selected {
        p.accent_soft
    } else if response.hovered() && enabled {
        p.control_hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    let color = if !enabled {
        p.muted.gamma_multiply(0.6)
    } else if selected {
        p.accent_text
    } else {
        p.text
    };
    let rtl = crate::i18n::rtl();
    let (icon_x, text_x, align) = if rtl {
        (
            rect.right() - 18.0,
            rect.right() - 36.0,
            egui::Align2::RIGHT_CENTER,
        )
    } else {
        (
            rect.left() + 18.0,
            rect.left() + 36.0,
            egui::Align2::LEFT_CENTER,
        )
    };
    draw_icon(
        ui.painter(),
        egui::pos2(icon_x, rect.center().y),
        icon,
        color,
        18.0,
    );
    ui.painter().text(
        egui::pos2(text_x, rect.center().y),
        align,
        label,
        egui::FontId::proportional(13.0),
        color,
    );
    response
}

/// The filled call-to-action button (Save).
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let p = palette(ui.visuals().dark_mode);
    let galley = ui
        .painter()
        .layout_no_wrap(label.into(), bold(13.0), p.on_accent);
    let size = Vec2::new(galley.size().x + 28.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let fill = if response.hovered() {
        p.accent.gamma_multiply(0.9)
    } else {
        p.accent
    };
    ui.painter().rect_filled(rect, 14.0, fill);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, p.on_accent);
    response
}

/// Thin vertical rule between toolbar groups.
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 20.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0, palette(ui.visuals().dark_mode).line),
    );
}

/// Small uppercase section caption.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    let p = palette(ui.visuals().dark_mode);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.0)
            .color(p.muted)
            .strong(),
    );
}

/// Frame for floating controls over the canvas.
pub fn floating_frame(dark: bool) -> egui::Frame {
    let p = palette(dark);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(egui::Margin::symmetric(6, 4))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 110 } else { 30 }),
        })
}

/// A row in reading order (right to left for Arabic).
pub fn row_layout() -> egui::Layout {
    if crate::i18n::rtl() {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    }
}

/// A row starting from the far side (buttons at the end of a dialog).
pub fn far_layout() -> egui::Layout {
    if crate::i18n::rtl() {
        egui::Layout::left_to_right(egui::Align::Center)
    } else {
        egui::Layout::right_to_left(egui::Align::Center)
    }
}

/// A column aligned to the reading side.
pub fn column_layout() -> egui::Layout {
    egui::Layout::top_down(if crate::i18n::rtl() {
        egui::Align::Max
    } else {
        egui::Align::Min
    })
}

/// A horizontal row in reading order that is only as tall as its contents.
pub fn row<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(size, row_layout(), add).inner
}

/// Dialog buttons on the far side, in a row only as tall as the buttons.
pub fn footer<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    row(ui, |ui| ui.with_layout(far_layout(), add).inner)
}

// ----- menus and dialog widgets that follow the reading direction ---------------------------

/// A menu-bar button with its dropdown: aligned to the button's start edge (right edge in
/// Arabic) with items laid out in reading order.
pub fn menu(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let p = palette(ui.visuals().dark_mode);
    let rtl = crate::i18n::rtl();
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), egui::FontId::proportional(13.5), p.text);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + 16.0, 26.0),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    if open || response.hovered() {
        ui.painter().rect_filled(
            rect,
            5.0,
            if open { p.accent_soft } else { p.control_hover },
        );
    }
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, p.text);
    egui::Popup::menu(&response)
        .align(if rtl {
            egui::RectAlign::BOTTOM_END
        } else {
            egui::RectAlign::BOTTOM_START
        })
        .layout(column_layout())
        .width(width)
        .show(|ui| {
            ui.set_min_width(width);
            ui.spacing_mut().item_spacing.y = 1.0;
            add(ui);
        });
    response
}

/// One menu entry: an optional check mark at the reading start, the label, and the shortcut at
/// the reading end. Closes the menu when clicked.
pub fn menu_item(
    ui: &mut egui::Ui,
    label: &str,
    shortcut: Option<&str>,
    enabled: bool,
    checked: Option<bool>,
) -> egui::Response {
    let p = palette(ui.visuals().dark_mode);
    let rtl = crate::i18n::rtl();
    let width = ui.available_width().max(160.0);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, 26.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response.widget_info(|| match checked {
        Some(c) => egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, c, label),
        None => egui::WidgetInfo::labeled(egui::Role::Button, enabled, label),
    });
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, 5.0, p.control_hover);
    }
    let color = if enabled {
        p.text
    } else {
        p.muted.gamma_multiply(0.7)
    };
    let (start, end, a_start, a_end) = if rtl {
        (
            rect.right() - 26.0,
            rect.left() + 10.0,
            egui::Align2::RIGHT_CENTER,
            egui::Align2::LEFT_CENTER,
        )
    } else {
        (
            rect.left() + 26.0,
            rect.right() - 10.0,
            egui::Align2::LEFT_CENTER,
            egui::Align2::RIGHT_CENTER,
        )
    };
    if checked == Some(true) {
        let x = if rtl {
            rect.right() - 13.0
        } else {
            rect.left() + 13.0
        };
        draw_icon(
            ui.painter(),
            egui::pos2(x, rect.center().y),
            Icon::Check,
            p.accent_text,
            14.0,
        );
    }
    let y = rect.center().y;
    ui.painter().text(
        egui::pos2(start, y),
        a_start,
        label,
        egui::FontId::proportional(13.0),
        color,
    );
    if let Some(s) = shortcut {
        ui.painter().text(
            egui::pos2(end, y),
            a_end,
            s,
            egui::FontId::proportional(12.0),
            p.muted,
        );
    }
    if response.clicked() {
        ui.close();
    }
    response
}

/// A thin separator across a menu.
pub fn menu_separator(ui: &mut egui::Ui) {
    let p = palette(ui.visuals().dark_mode);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 7.0), egui::Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.line));
}

/// A check box with its box at the reading start.
pub fn check(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    let r = choice(ui, *value, label, false);
    if r.clicked() {
        *value = !*value;
    }
    r
}

/// A radio button with its circle at the reading start.
pub fn radio(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    choice(ui, selected, label, true)
}

fn choice(ui: &mut egui::Ui, on: bool, label: &str, round: bool) -> egui::Response {
    let p = palette(ui.visuals().dark_mode);
    let rtl = crate::i18n::rtl();
    let galley = ui.painter().layout(
        label.to_string(),
        egui::FontId::proportional(13.5),
        p.text,
        (ui.available_width() - 26.0).max(40.0),
    );
    let size = Vec2::new(galley.size().x + 26.0, galley.size().y.max(20.0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            if round {
                egui::Role::RadioButton
            } else {
                egui::Role::CheckBox
            },
            true,
            on,
            label,
        )
    });
    let bx = if rtl {
        rect.right() - 8.0
    } else {
        rect.left() + 8.0
    };
    let c = egui::pos2(bx, rect.top() + 10.0);
    let stroke = Stroke::new(
        1.0,
        if response.hovered() {
            p.border_strong
        } else {
            p.border
        },
    );
    if round {
        ui.painter().circle(c, 7.0, p.input, stroke);
        if on {
            ui.painter().circle_filled(c, 3.5, p.accent);
        }
    } else {
        let b = egui::Rect::from_center_size(c, Vec2::splat(14.0));
        ui.painter().rect(
            b,
            3.0,
            if on { p.accent } else { p.input },
            stroke,
            egui::StrokeKind::Inside,
        );
        if on {
            draw_icon(ui.painter(), c, Icon::Check, p.on_accent, 12.0);
        }
    }
    let tx = if rtl {
        rect.right() - 22.0 - galley.size().x
    } else {
        rect.left() + 22.0
    };
    ui.painter().galley(
        egui::pos2(
            tx,
            rect.top() + (20.0 - galley.rows.first().map_or(16.0, |r| r.height())) / 2.0,
        ),
        galley,
        p.text,
    );
    response
}

/// A collapsible section whose arrow sits at the reading start.
pub fn collapsible(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let p = palette(ui.visuals().dark_mode);
    let rtl = crate::i18n::rtl();
    let id = ui.make_persistent_id(("collapsible", title));
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let galley =
        ui.painter()
            .layout_no_wrap(title.to_string(), egui::FontId::proportional(13.5), p.text);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + 24.0, 24.0),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, title));
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    let icon = match (open, rtl) {
        (true, _) => Icon::Down,
        (false, false) => Icon::Right,
        (false, true) => Icon::Left,
    };
    let (ix, tx, align) = if rtl {
        (
            rect.right() - 8.0,
            rect.right() - 20.0,
            egui::Align2::RIGHT_CENTER,
        )
    } else {
        (
            rect.left() + 8.0,
            rect.left() + 20.0,
            egui::Align2::LEFT_CENTER,
        )
    };
    draw_icon(
        ui.painter(),
        egui::pos2(ix, rect.center().y),
        icon,
        p.muted,
        12.0,
    );
    ui.painter().text(
        egui::pos2(tx, rect.center().y),
        align,
        title,
        egui::FontId::proportional(13.5),
        p.text,
    );
    if open {
        ui.with_layout(column_layout(), |ui| {
            ui.add_space(2.0);
            add(ui);
        });
    }
}

/// A centred dialog window with a title bar in reading order (close button at the reading end).
pub fn dialog<R>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    open: &mut bool,
    width: f32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    let p = palette(ctx.global_style().visuals.dark_mode);
    let rtl = crate::i18n::rtl();
    let mut close = false;
    let r = egui::Window::new(title)
        .id(egui::Id::unique(id))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(0)))
        .show(ctx, |ui| {
            ui.set_width(width + 28.0);
            // Title bar.
            let (bar, _) =
                ui.allocate_exact_size(Vec2::new(width + 28.0, 40.0), egui::Sense::hover());
            ui.painter().rect_filled(
                bar,
                egui::CornerRadius {
                    nw: 10,
                    ne: 10,
                    sw: 0,
                    se: 0,
                },
                p.accent_soft,
            );
            ui.painter()
                .hline(bar.x_range(), bar.bottom(), Stroke::new(1.0, p.line));
            ui.painter().text(
                bar.center(),
                egui::Align2::CENTER_CENTER,
                title,
                egui::FontId::proportional(15.0),
                p.text,
            );
            let cx = if rtl {
                bar.left() + 22.0
            } else {
                bar.right() - 22.0
            };
            let cr =
                egui::Rect::from_center_size(egui::pos2(cx, bar.center().y), Vec2::splat(26.0));
            let c = ui.interact(cr, ui.scope_id().with("close"), egui::Sense::click());
            c.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::tr("Close"))
            });
            if c.hovered() {
                ui.painter().rect_filled(cr, 6.0, p.control_hover);
            }
            draw_icon(ui.painter(), cr.center(), Icon::Close, p.text, 16.0);
            if c.clicked() {
                close = true;
            }
            egui::Frame::new()
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.with_layout(column_layout(), add).inner
                })
                .inner
        });
    if close {
        *open = false;
    }
    r.and_then(|r| r.inner)
}

/// A wrapping paragraph of interface text. Arabic is wrapped in reading order first and each
/// line shaped on its own, so lines keep their order (egui would wrap the visual string).
pub fn para(ui: &mut egui::Ui, key: &str, size: f32, color: Option<egui::Color32>) {
    para_text(ui, &crate::i18n::raw(key), size, color);
}

/// `para` for text that is already translated (in reading order).
pub fn para_text(ui: &mut egui::Ui, text: &str, size: f32, color: Option<egui::Color32>) {
    let color = color.unwrap_or_else(|| ui.visuals().text_color());
    if !crate::i18n::rtl() {
        ui.label(egui::RichText::new(text).size(size).color(color));
        return;
    }
    let text = text.to_string();
    let width = ui.available_width().max(60.0);
    let font = egui::FontId::proportional(size);
    let fits = |ui: &egui::Ui, line: &str| {
        ui.painter()
            .layout_no_wrap(crate::i18n::shape(line), font.clone(), color)
            .size()
            .x
            <= width
    };
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if current.is_empty() || fits(ui, &candidate) {
            current = candidate;
        } else {
            lines.push(std::mem::replace(&mut current, word.to_string()));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    ui.with_layout(column_layout(), |ui| {
        for line in lines {
            ui.label(
                egui::RichText::new(crate::i18n::shape(&line))
                    .size(size)
                    .color(color),
            );
        }
    });
}

/// Text that must appear exactly as written (licences): never translated or reordered, laid
/// out left to right and left-aligned in a scrollable monospace block, in every language.
pub fn verbatim(ui: &mut egui::Ui, id: &str, text: &str, max_height: f32) {
    let width = ui.available_width();
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.set_width(width - 12.0);
                ui.add(egui::Label::new(egui::RichText::new(text).small().monospace()).wrap());
            });
    });
}
