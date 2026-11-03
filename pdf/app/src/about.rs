//! The About and Settings windows.

use eframe::egui::{self, RichText};

use crate::app::{App, ThemeChoice};
use crate::i18n::{self, Lang, n_, tr, trf};
use crate::theme;

/// The app's licence, shown verbatim.
pub const LICENSE: &str = include_str!("../../LICENSE");

const FONT_LICENSES: [(&str, &str); 4] = [
    (
        "Liberation Sans, Liberation Serif (SIL OFL 1.1)",
        include_str!("../../engine/assets/fonts/Liberation-OFL.txt"),
    ),
    (
        "Noto Sans Arabic (SIL OFL 1.1)",
        include_str!("../../engine/assets/fonts/NotoArabicHebrew-OFL.txt"),
    ),
    (
        "Great Vibes (SIL OFL 1.1)",
        include_str!("../../engine/assets/fonts/GreatVibes-OFL.txt"),
    ),
    (
        "Phosphor Icons (MIT)",
        include_str!("../assets/fonts/PHOSPHOR-LICENSE"),
    ),
];

pub fn about(app: &mut App, ctx: &egui::Context) {
    if !app.about_open {
        return;
    }
    let mut open = true;
    theme::dialog(
        ctx,
        "about",
        &trf("About {}", &[&crate::APP_NAME]),
        &mut open,
        420.0,
        |ui| {
            {
                // The icon, name and maker; links to the product page; licence.
                theme::row(ui, |ui| {
                    if let Some(t) = crate::icon_texture(ui.ctx()) {
                        ui.add(egui::Image::new((t.id(), egui::vec2(72.0, 72.0))))
                            .widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Image, true, tr("Waraq icon"))
                            });
                    }
                    ui.with_layout(theme::column_layout(), |ui| {
                    ui.label(RichText::new(format!("{}  {}", crate::APP_NAME, i18n::shape(crate::APP_NAME_AR))).font(theme::bold(22.0)));
                    ui.label(trf("Version {}", &[&crate::update::CURRENT]));
                    theme::para(ui, "A free desktop PDF editor for reading, organizing, commenting, filling, signing and redacting PDF files offline.", 13.5, None);
                });
                });
                ui.add_space(8.0);
                theme::row(ui, |ui| {
                    ui.label(tr("Made by"));
                    ui.hyperlink_to("Woosool", "https://woosool.com");
                });
                ui.with_layout(theme::column_layout(), |ui| {
                ui.hyperlink_to(tr("Waraq on woosool.com"), crate::PRODUCT_PAGE);
                if ui.button(tr("Check for Updates…")).clicked() {
                    crate::update::check(ui.ctx(), true, None);
                }
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Released under the MIT License.")).strong());
                ui.label(RichText::new(tr("Copyright © 2026 Woosool")).small());
                theme::collapsible(ui, &tr("License"), |ui| {
                    theme::verbatim(ui, "license", LICENSE, 180.0);
                });
                theme::collapsible(ui, &tr("Fonts and icons"), |ui| {
                    for (name, text) in FONT_LICENSES {
                        theme::collapsible(ui, name, |ui| {
                            theme::verbatim(ui, name, text, 140.0);
                        });
                    }
                });
                ui.add_space(6.0);
                theme::para(ui, n_("Fonts: Liberation Sans and Serif, Noto Sans Arabic, Great Vibes (OFL). Icons: Phosphor (MIT)."), 13.0, Some(ui.visuals().weak_text_color()));
            });
            }
        },
    );
    app.about_open = open;
}

pub fn settings(app: &mut App, ctx: &egui::Context) {
    if !app.settings_open {
        return;
    }
    let mut open = true;
    theme::dialog(ctx, "settings", &tr("Settings"), &mut open, 380.0, |ui| {
        ui.with_layout(theme::column_layout(), |ui| {
            theme::caption(ui, &tr("Language"));
            let mut lang = app.settings.lang;
            egui::ComboBox::from_id_salt("settings-lang")
                .selected_text(lang.name())
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (l, _) in Lang::ALL {
                        ui.selectable_value(&mut lang, l, l.name());
                    }
                })
                .response
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tr("Language"))
                });
            if lang != app.settings.lang {
                app.settings.lang = lang;
                app.settings.save();
            }
            ui.add_space(8.0);
            theme::caption(ui, &tr("Appearance"));
            for (choice, label) in [
                (ThemeChoice::System, i18n::n_("Match system appearance")),
                (ThemeChoice::Light, i18n::n_("Light appearance")),
                (ThemeChoice::Dark, i18n::n_("Dark appearance")),
            ] {
                if theme::radio(ui, app.theme == choice, &tr(label)).clicked() {
                    app.theme = choice;
                }
            }
            if theme::check(ui, &mut app.settings.splash, &tr("Show the start screen")).clicked() {
                app.settings.save();
            }
            ui.add_space(8.0);
            theme::caption(ui, &tr("Updates"));
            if theme::check(
                ui,
                &mut app.settings.auto_update,
                &tr("Check for updates automatically"),
            )
            .clicked()
            {
                app.settings.save();
            }
            if ui.button(tr("Check Now")).clicked() {
                crate::update::check(ui.ctx(), true, None);
            }
        });
    });
    app.settings_open = open;
}
