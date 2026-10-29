//! The Insert tab's commands, each handed to the module that implements it. Commands whose
//! module isn't written yet say so instead of doing nothing.

use crate::app::{Dialog, SheetApp};
use crate::i18n::tr;
use egui::Ui;


pub fn table(app: &mut SheetApp) { crate::filter_ui::open_create(app, "TableStyleMedium2"); }
pub fn link(app: &mut SheetApp) { let p = app.sel.active; let t = app.book.sheets[app.sheet()].links.get(&p).map(|l| l.target.clone()).unwrap_or_default(); app.dialog = Some(Dialog::Link { pos: p, target: t }); }
pub fn note(app: &mut SheetApp) { let p = app.sel.active; let t = app.book.sheets[app.sheet()].notes.get(&p).cloned().unwrap_or_default(); app.dialog = Some(Dialog::Note { pos: p, text: t }); }

// ---- Illustrations ----
pub fn online_pictures(app: &mut SheetApp) { crate::ins_pictures::open(&app.ctx, crate::ins_pictures::Dlg::Online(Default::default())); }
pub fn picture_in_cell(app: &mut SheetApp) { crate::sparkline_ui::picture_in_cell(app); }
pub fn shapes_gallery(app: &mut SheetApp, ui: &mut Ui) { crate::ins_shapes::gallery(app, ui); }
pub fn icons(app: &mut SheetApp) { crate::ins_pictures::open(&app.ctx, crate::ins_pictures::Dlg::Icons { cat: 0, query: String::new() }); }
pub fn smart_art(app: &mut SheetApp) { crate::ins_pictures::open(&app.ctx, crate::ins_pictures::Dlg::SmartArt(crate::ins_smartart::Editor::new())); }
pub fn screenshot_menu(app: &mut SheetApp, ui: &mut Ui) { crate::ins_pictures::screenshot_menu(app, ui); }
pub fn model_3d(app: &mut SheetApp) { crate::ins_pictures::insert_model(app); }

// ---- Add-ins ----
pub fn scripts(app: &mut SheetApp) { crate::scripts_ui::open(app); }
pub fn add_ins_menu(app: &mut SheetApp, ui: &mut Ui) { crate::scripts_ui::add_ins_menu(app, ui); }

// ---- Charts and tours ----
pub fn more_charts_menu(app: &mut SheetApp, ui: &mut Ui) { crate::sparkline_ui::more_charts_menu(app, ui); }
pub fn map_chart_menu(app: &mut SheetApp, ui: &mut Ui) { crate::maps_ui::menu(app, ui); }
pub fn pivot_chart_menu(app: &mut SheetApp, ui: &mut Ui) { crate::pivot_chart_ui::menu(app, ui); }
pub fn map_3d(app: &mut SheetApp) { crate::maps_ui::open_3d(app); }

// ---- Sparklines and filters ----
pub fn sparklines(app: &mut SheetApp, kind: u8) { crate::sparkline_ui::open_create(app, [sheet_engine::sparkline::Kind::Line, sheet_engine::sparkline::Kind::Column, sheet_engine::sparkline::Kind::WinLoss][kind.min(2) as usize]); }
pub fn timeline(app: &mut SheetApp) { crate::timeline_ui::open_insert(app); }

// ---- Text and symbols ----
pub fn header_footer(app: &mut SheetApp) { crate::page_view::header_footer(app); }
pub fn word_art_menu(app: &mut SheetApp, ui: &mut Ui) { crate::ins_shapes::wordart_menu(app, ui); }
pub fn signature_menu(app: &mut SheetApp, ui: &mut Ui) { crate::ins_pictures::signature_menu(app, ui); }
pub fn object(app: &mut SheetApp) { crate::ins_pictures::open(&app.ctx, crate::ins_pictures::Dlg::Object { icon: true }); }
pub fn equation(app: &mut SheetApp, preset: Option<&str>) { match preset { Some(p) => { crate::ins_pictures::insert_equation(app, None, p); } None => crate::ins_pictures::open(&app.ctx, crate::ins_pictures::Dlg::Equation { target: None, linear: String::new() }) } }
pub fn equation_menu(app: &mut SheetApp, ui: &mut Ui) { crate::ins_pictures::equation_menu(app, ui); }
pub fn symbol(app: &mut SheetApp) { crate::ins_pictures::open(&app.ctx, crate::ins_pictures::Dlg::Symbol { tab: 0, subset: 0, font: 0, selected: None }); }
