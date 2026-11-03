mod crash;
mod html_paste;
mod i18n;
mod model;
mod network;
mod notify;
mod rich_text;
mod search;
mod store;
mod threads;
mod ui;

fn main() -> eframe::Result {
    crash::install();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1040.0, 680.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Barid",
        options,
        Box::new(|cc| Ok(Box::new(ui::Barid::new(cc)))),
    )
}
