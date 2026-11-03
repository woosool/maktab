//! The start-up splash card (as in Qalam and Jadwal): the window first appears as a small
//! borderless card while fonts and assets load and a file named on the command line (or
//! dropped on the card) waits; then it grows into the main window where it was last time.

use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, pos2, vec2};

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::theme::{self, palette};

/// The splash card's size.
pub const SIZE: [f32; 2] = [440.0, 340.0];

/// Shortest time the card shows.
const MIN_SECONDS: f64 = 1.3;

/// Draws the card; returns true while it is showing (the rest of the frame is skipped).
pub fn show(app: &mut App, ui: &mut egui::Ui) -> bool {
    let now = ui.input(|i| i.time);
    let Some(start) = &mut app.splash else {
        return false;
    };
    if *start == 0.0 {
        *start = now.max(1e-6);
    }
    let elapsed = now - *start;
    // A file dropped on the card opens after it.
    if let Some(path) = ui.ctx().input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .find(|p| !p.as_os_str().is_empty())
    }) {
        app.pending_open = Some(path);
    }
    if elapsed > MIN_SECONDS {
        app.splash = None;
        grow_into_main_window(app, ui.ctx());
        if let Some(path) = app.pending_open.take() {
            app.open_path(&path);
        }
        app.auto_check_updates(&ui.ctx().clone());
        return false;
    }
    let dark = ui.visuals().dark_mode;
    let p = palette(dark);
    let (bg, ink, sub) = if dark {
        (
            Color32::from_gray(0x1C),
            Color32::from_gray(0xF2),
            Color32::from_gray(0xA8),
        )
    } else {
        (
            Color32::WHITE,
            Color32::from_rgb(0x2A, 0x14, 0x14),
            Color32::from_gray(0x70),
        )
    };
    let status = match app.pending_open.as_ref().and_then(|p| p.file_name()) {
        Some(n) => trf("Opening {}…", &[&n.to_string_lossy()]),
        None => tr("Starting…"),
    };
    egui::CentralPanel::no_frame().show(ui, |ui| {
        let rect = ui.max_rect().shrink(10.0);
        ui.painter().add(
            egui::epaint::Shadow {
                offset: [0, 3],
                blur: 12,
                spread: 0,
                color: Color32::from_black_alpha(70),
            }
            .as_shape(rect, 16.0),
        );
        ui.painter().rect(
            rect,
            16.0,
            bg,
            egui::Stroke::new(1.0, sub.gamma_multiply(0.25)),
            egui::StrokeKind::Inside,
        );
        let drag = ui.interact(rect, egui::Id::unique("splash-drag"), Sense::drag());
        drag.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, crate::APP_NAME));
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let a = (elapsed / 0.4).clamp(0.0, 1.0) as f32;
        let c = rect.center() - vec2(0.0, 44.0);
        if let Some(t) = crate::icon_texture(ui.ctx()) {
            let r = Rect::from_center_size(c - vec2(0.0, 10.0), vec2(120.0, 120.0));
            ui.painter().image(
                t.id(),
                r,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE.gamma_multiply(a),
            );
        }
        let title = format!(
            "{}  {}",
            crate::APP_NAME,
            crate::i18n::shape(crate::APP_NAME_AR)
        );
        ui.painter().text(
            c + vec2(0.0, 70.0),
            Align2::CENTER_CENTER,
            title,
            theme::bold(30.0),
            ink.gamma_multiply(a),
        );
        let g = ui.painter().layout(
            status,
            FontId::proportional(14.0),
            sub.gamma_multiply(a),
            rect.width() - 40.0,
        );
        ui.painter().galley(
            c + vec2(-g.size().x / 2.0, 104.0 - g.size().y / 2.0),
            g,
            sub,
        );
        let w = 160.0;
        let bar = Rect::from_center_size(c + vec2(0.0, 128.0), vec2(w, 3.0));
        ui.painter()
            .rect_filled(bar, 1.5, sub.gamma_multiply(0.25 * a));
        let t = ((elapsed * 0.8) % 1.0) as f32;
        let x = if crate::i18n::rtl() {
            bar.max.x - 50.0 - (w - 50.0) * t
        } else {
            bar.min.x + (w - 50.0) * t
        };
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(x, bar.min.y), vec2(50.0, 3.0)),
            1.5,
            p.accent.gamma_multiply(a),
        );
        let maker = trf("Made by {}", &[&"Woosool · woosool.com"]);
        ui.painter().text(
            pos2(rect.center().x, rect.max.y - 22.0),
            Align2::CENTER_CENTER,
            maker,
            FontId::proportional(12.0),
            sub.gamma_multiply(a),
        );
    });
    ui.ctx().request_repaint();
    true
}

/// Restores decorations and the main window's last size, position and maximized state.
fn grow_into_main_window(app: &App, ctx: &egui::Context) {
    let (size, pos, maximized) = app
        .settings
        .window
        .unwrap_or(([1360.0, 880.0], None, false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(900.0, 600.0)));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(size[0], size[1])));
    let pos = pos.map(|p| pos2(p[0], p[1])).or_else(|| {
        let m = ctx.input(|i| i.viewport().monitor_size)?;
        Some(pos2(
            ((m.x - size[0]) / 2.0).max(0.0),
            ((m.y - size[1]) / 2.0).max(24.0),
        ))
    });
    if let Some(p) = pos {
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(p));
    }
    if maximized {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
    }
}
