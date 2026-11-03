//! Page Layout ▸ Themes: the Themes, Colors, Fonts and Effects galleries, Customize Colors and
//! Customize Fonts, Browse for Themes and Save Current Theme (.thmx); and the theme-aware color
//! picker the ribbon and dialogs use (theme columns with tints, then standard colors).

use crate::app::{rgb, SheetApp};
use crate::i18n::tr;
use egui::{vec2, Id, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use sheet_engine::style::Rgb;
use sheet_engine::theme::{self, ThemeColor, PICKER_SLOTS};

/// Customize Colors / Customize Fonts, while open.
#[derive(Clone)]
pub enum ThemeDialog {
    Colors { name: String, colors: [Rgb; 12] },
    Fonts { name: String, major: String, minor: String },
}

fn did() -> Id { Id::new("jadwal-theme-dialog") }
fn open(app: &SheetApp, d: ThemeDialog) { app.ctx.data_mut(|x| x.insert_temp(did(), Some(d))); }

// ---- the color picker ------------------------------------------------------------------------

/// A color flyout with the workbook's theme colors: returns the color picked (None = automatic)
/// and, for a theme swatch, the theme color it is.
pub fn color_menu(ui: &mut Ui, auto_label: &str, theme_colors: &[Rgb]) -> Option<Option<(Rgb, Option<ThemeColor>)>> {
    let mut out = None;
    ui.spacing_mut().item_spacing.y = 4.0;
    if !auto_label.is_empty() && none_row(ui, auto_label) { out = Some(None); ui.close(); }
    menu_heading(ui, "Theme Colors");
    let cols: Vec<(Rgb, u8)> = PICKER_SLOTS.iter().map(|&k| (theme_colors.get(k as usize).copied().unwrap_or(sheet_engine::numfmt::OFFICE_THEME[k as usize]), k)).collect();
    for row in 0..6 {
        ui.allocate_ui_with_layout(vec2(10.0 * 24.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (base, k) in &cols {
                let t = if row == 0 { 0.0 } else { theme::picker_tints(*base)[row - 1] };
                let tc = ThemeColor::new(*k, t);
                let c = tc.resolve(theme_colors);
                let name = ["Background 1", "Text 1", "Background 2", "Text 2", "Accent 1", "Accent 2", "Accent 3", "Accent 4", "Accent 5", "Accent 6"][PICKER_SLOTS.iter().position(|x| x == k).unwrap()];
                let tip = if t == 0.0 { tr(name) } else { format!("{}, {} {}%", tr(name), tr(if t > 0.0 { "Lighter" } else { "Darker" }), (t.abs() * 100.0).round()) };
                if swatch(ui, rgb(c), 20.0, false).on_hover_text(tip).clicked() { out = Some(Some((c, Some(tc)))); ui.close(); }
            }
        });
        if row == 0 { ui.add_space(3.0); }
    }
    menu_heading(ui, "Standard Colors");
    let std: Vec<word_engine::Color> = PALETTE[20..30].iter().map(|c| word_engine::Color(c[0], c[1], c[2])).collect();
    let mut picked = None;
    swatch_row(ui, &std, &mut picked);
    let recent_id = Id::new("recent-custom-colors");
    let recent: Vec<[u8; 3]> = ui.ctx().data_mut(|d| d.get_persisted::<Vec<[u8; 3]>>(recent_id)).unwrap_or_default();
    if !recent.is_empty() {
        menu_heading(ui, "Recent Colors");
        let r: Vec<word_engine::Color> = recent.iter().take(10).map(|c| word_engine::Color(c[0], c[1], c[2])).collect();
        swatch_row(ui, &r, &mut picked);
    }
    ui.separator();
    let pick_id = Id::new("custom-color-value");
    let mut v: [u8; 3] = ui.ctx().data_mut(|d| d.get_persisted::<[u8; 3]>(pick_id)).unwrap_or([0x18, 0x5A, 0xBD]);
    crate::theme::submenu(ui, tr("More Colors…"), |ui| {
        if let Some(c) = custom_picker(ui, &mut v) {
            picked = Some(Some(c));
            let mut r = recent.clone();
            r.retain(|x| *x != v);
            r.insert(0, v);
            r.truncate(10);
            ui.ctx().data_mut(|d| d.insert_persisted(recent_id, r));
            ui.close();
        }
    });
    ui.ctx().data_mut(|d| d.insert_persisted(pick_id, v));
    if let Some(Some(c)) = picked { out = Some(Some(([c.0, c.1, c.2], None))); }
    out
}

// ---- the Themes group --------------------------------------------------------------------------

fn strip(ui: &Ui, r: Rect, colors: &[Rgb]) {
    let n = colors.len().max(1) as f32;
    let w = r.width() / n;
    for (i, c) in colors.iter().enumerate() { ui.painter().rect_filled(Rect::from_min_size(r.min + vec2(i as f32 * w, 0.0), vec2(w, r.height())), 0.0, rgb(*c)); }
}

/// A theme tile: "Aa" in its fonts over its background, with the accent strip.
fn theme_tile(ui: &mut Ui, t: &theme::Builtin, current: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(88.0, 70.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, rgb(t.colors.colors[1]));
    p.text(rect.min + vec2(10.0, 8.0), egui::Align2::LEFT_TOP, "Aa", egui::FontId::proportional(22.0), rgb(t.colors.colors[0]));
    strip(ui, Rect::from_min_size(rect.min + vec2(6.0, 40.0), vec2(rect.width() - 12.0, 8.0)), &t.colors.colors[4..10]);
    ui.painter().text(rect.center_bottom() - vec2(0.0, 4.0), egui::Align2::CENTER_BOTTOM, &t.name, egui::FontId::proportional(10.0), rgb(t.colors.colors[0]));
    let ring = if current { Stroke::new(2.0, crate::grid::SELECT) } else if resp.hovered() { Stroke::new(1.5, c_ink3()) } else { Stroke::new(1.0, c_line()) };
    ui.painter().rect_stroke(rect, 4.0, ring, StrokeKind::Inside);
    resp.on_hover_text(format!("{}\n{} / {}", t.name, t.major, t.minor))
}

fn apply_theme(app: &mut SheetApp, t: &theme::Builtin) {
    app.checkpoint("Theme");
    theme::apply(&mut app.book, t);
    app.book.cf_dirty();
    app.dirty = true;
}

pub fn group(app: &mut SheetApp, ui: &mut Ui) {
    qalam_kit::widgets::group(ui, "Themes", |ui| {
        let r = large_menu(ui, "themes", "Themes", "Change the workbook's colors, fonts and effects at once");
        crate::theme::menu(&r, |ui| {
            ui.set_width(390.0);
            menu_heading(ui, "Built-In");
            let all = theme::builtins();
            let cur = app.book.theme.name.clone();
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for chunk in all.chunks(4) {
                    ui.horizontal(|ui| for t in chunk { if theme_tile(ui, t, t.name == cur).clicked() { apply_theme(app, t); ui.close(); } });
                }
            });
            ui.separator();
            if ui.button(tr("Reset to Theme from Template")).clicked() { let t = all[0].clone(); apply_theme(app, &t); ui.close(); }
            if ui.button(tr("Browse for Themes…")).clicked() { browse(app); ui.close(); }
            if ui.button(tr("Save Current Theme…")).clicked() { save(app); ui.close(); }
        });
        rows(ui, |ui| {
            let r = small_menu(ui, "themeColors", "Colors", "Theme colors");
            crate::theme::menu(&r, |ui| {
                ui.set_width(300.0);
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for p in theme::palettes() {
                        let (rect, resp) = ui.allocate_exact_size(vec2(280.0, 24.0), Sense::click());
                        if resp.hovered() || app.book.theme.colors_name == p.name { ui.painter().rect_filled(rect, 4.0, c_hover()); }
                        let colors: Vec<Rgb> = [0usize, 2, 4, 5, 6, 7, 8, 9].iter().map(|&i| p.colors[i]).collect();
                        for (i, c) in colors.iter().enumerate() {
                            let sw = Rect::from_min_size(rect.min + vec2(6.0 + i as f32 * 14.0, 5.0), vec2(13.0, 14.0));
                            ui.painter().rect_filled(sw, 0.0, rgb(*c));
                        }
                        ui.painter().text(rect.min + vec2(6.0 + 8.0 * 14.0 + 10.0, 12.0), egui::Align2::LEFT_CENTER, &p.name, egui::FontId::proportional(13.0), c_ink());
                        if resp.clicked() { app.checkpoint("Theme Colors"); theme::apply_colors(&mut app.book, &p); app.book.cf_dirty(); app.dirty = true; ui.close(); }
                    }
                });
                ui.separator();
                if ui.button(tr("Customize Colors…")).clicked() {
                    let mut c = sheet_engine::numfmt::OFFICE_THEME;
                    for (i, x) in app.book.theme_colors.iter().take(12).enumerate() { c[i] = *x; }
                    open(app, ThemeDialog::Colors { name: tr("Custom 1"), colors: c }); ui.close();
                }
            });
            let r = small_menu(ui, "themeFonts", "Fonts", "Theme fonts");
            crate::theme::menu(&r, |ui| {
                ui.set_width(280.0);
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (name, major, minor) in theme::font_pairs() {
                        let on = app.book.theme.major == major && app.book.theme.minor == minor;
                        let (rect, resp) = ui.allocate_exact_size(vec2(260.0, 52.0), Sense::click());
                        if resp.hovered() || on { ui.painter().rect_filled(rect, 4.0, c_hover()); }
                        ui.painter().text(rect.min + vec2(8.0, 4.0), egui::Align2::LEFT_TOP, &name, egui::FontId::proportional(11.0), c_ink3());
                        ui.painter().text(rect.min + vec2(8.0, 18.0), egui::Align2::LEFT_TOP, &major, egui::FontId::proportional(15.0), c_ink());
                        ui.painter().text(rect.min + vec2(8.0, 36.0), egui::Align2::LEFT_TOP, &minor, egui::FontId::proportional(12.0), c_ink2());
                        if resp.clicked() { app.checkpoint("Theme Fonts"); theme::apply_fonts(&mut app.book, &name, &major, &minor); app.dirty = true; ui.close(); }
                    }
                });
                ui.separator();
                if ui.button(tr("Customize Fonts…")).clicked() { open(app, ThemeDialog::Fonts { name: tr("Custom 1"), major: app.book.theme.major.clone(), minor: app.book.theme.minor.clone() }); ui.close(); }
            });
            let r = small_menu(ui, "effects", "Effects", "Theme effects");
            crate::theme::menu(&r, |ui| {
                for e in theme::effects() {
                    if ui.selectable_label(app.book.theme.effects_name == e.name, tr(&e.name)).clicked() { app.checkpoint("Theme Effects"); theme::apply_effects(&mut app.book, &e); app.dirty = true; ui.close(); }
                }
            });
        });
    });
}

fn browse(app: &mut SheetApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("Office Themes"), &["thmx", "xml"]).pick_file() else { return };
    match std::fs::read(&path).ok().and_then(|b| theme::read_thmx(&b)) {
        Some(t) => apply_theme(app, &t),
        None => app.error = Some(tr("That file isn't an Office theme.")),
    }
}

fn save(app: &mut SheetApp) {
    let Some(path) = rfd::FileDialog::new().add_filter(tr("Office Theme"), &["thmx"]).set_file_name(format!("{}.thmx", app.book.theme.name)).save_file() else { return };
    if let Err(e) = std::fs::write(&path, theme::thmx(&app.book)) { app.error = Some(format!("{}\n{e}", tr("The theme couldn't be saved."))); }
}

// ---- dialogs ------------------------------------------------------------------------------------

pub fn dialogs(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = ctx.data(|x| x.get_temp::<Option<ThemeDialog>>(did())).flatten() else { return };
    let mut keep = true;
    let mut open_ = true;
    match &mut d {
        ThemeDialog::Colors { name, colors } => {
            crate::theme::dialog(ctx, "Create New Theme Colors").open(&mut open_).resizable(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Grid::new("theme-colors").num_columns(2).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
                        let labels = ["Text/Background - Dark 1", "Text/Background - Light 1", "Text/Background - Dark 2", "Text/Background - Light 2", "Accent 1", "Accent 2", "Accent 3", "Accent 4", "Accent 5", "Accent 6", "Hyperlink", "Followed Hyperlink"];
                        for (i, l) in labels.iter().enumerate() {
                            ui.label(tr(l));
                            let mut c = word_engine::Color(colors[i][0], colors[i][1], colors[i][2]);
                            if color_button(ui, &mut c) { colors[i] = [c.0, c.1, c.2]; }
                            ui.end_row();
                        }
                    });
                    ui.add_space(12.0);
                    // A sample: text on both backgrounds, the accents as bars and the links.
                    ui.vertical(|ui| {
                        ui.label(RichText::new(tr("Sample")).strong());
                        for (bg, fg) in [(1usize, 0usize), (0, 1)] {
                            let (r, _) = ui.allocate_exact_size(vec2(170.0, 80.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, rgb(colors[bg]));
                            ui.painter().text(r.min + vec2(8.0, 6.0), egui::Align2::LEFT_TOP, tr("Text"), egui::FontId::proportional(13.0), rgb(colors[fg]));
                            for k in 0..6 { let h = 12.0 + k as f32 * 6.0; ui.painter().rect_filled(Rect::from_min_size(pos(r, 8.0 + k as f32 * 18.0, 72.0 - h), vec2(14.0, h)), 0.0, rgb(colors[4 + k])); }
                            ui.painter().text(r.right_top() + vec2(-8.0, 6.0), egui::Align2::RIGHT_TOP, tr("Hyperlink"), egui::FontId::proportional(11.0), rgb(colors[10]));
                            ui.painter().text(r.right_top() + vec2(-8.0, 22.0), egui::Align2::RIGHT_TOP, tr("Followed Hyperlink"), egui::FontId::proportional(11.0), rgb(colors[11]));
                        }
                    });
                });
                ui.horizontal(|ui| { ui.label(tr("Name:")); ui.add(egui::TextEdit::singleline(name).desired_width(220.0)); });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "Save").clicked() {
                        let p = theme::Palette { name: name.clone(), colors: *colors };
                        app.checkpoint("Theme Colors"); theme::apply_colors(&mut app.book, &p); app.book.cf_dirty(); app.dirty = true; keep = false;
                    }
                    if crate::theme::secondary(ui, "Reset").clicked() { *colors = sheet_engine::numfmt::OFFICE_THEME; }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
        }
        ThemeDialog::Fonts { name, major, minor } => {
            crate::theme::dialog(ctx, "Create New Theme Fonts").open(&mut open_).resizable(false).show(ctx, |ui| {
                let fonts = word_engine::fonts::family_names();
                egui::Grid::new("theme-fonts").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                    for (label, v) in [("Heading font:", &mut *major), ("Body font:", &mut *minor)] {
                        ui.label(tr(label));
                        crate::theme::Combo::from_id_salt(("theme-font", label)).width(220.0).selected_text(v.clone()).show_ui(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| for f in &fonts { if ui.selectable_label(v == f, f).clicked() { *v = f.clone(); } });
                        });
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new(major.clone()).size(18.0));
                ui.label(RichText::new(tr("Body text body text body text.")).size(12.0));
                ui.horizontal(|ui| { ui.label(tr("Name:")); ui.add(egui::TextEdit::singleline(name).desired_width(220.0)); });
                crate::theme::footer(ui, |ui| {
                    if crate::theme::primary(ui, "Save").clicked() {
                        app.checkpoint("Theme Fonts"); theme::apply_fonts(&mut app.book, name, major, minor); app.dirty = true; keep = false;
                    }
                    if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
                });
            });
        }
    }
    let next = if keep && open_ { Some(d) } else { None };
    // A dialog opened meanwhile isn't overwritten by this one closing.
    ctx.data_mut(|x| x.insert_temp(did(), next));
}

fn pos(r: Rect, x: f32, y: f32) -> egui::Pos2 { r.min + vec2(x, y) }
