//! Get Data and Text to Columns (importdialog.js, get-data.js, query-editor.js): the text import
//! wizard (file origin/encoding, delimited or fixed width, delimiters and qualifier, a live
//! preview, a data type per column, where to put the data), the Navigator listing another
//! workbook's sheets and tables or a web page's tables, From Web's address box and From
//! Table/Range. Imports become workbook connections the Queries & Connections pane refreshes.

use crate::app::SheetApp;
use crate::data_tab::{open, Dlg};
use crate::i18n::tr;
use egui::{vec2, RichText};
use sheet_engine::addr::{parse_a1, Pos, Range};
use sheet_engine::connections::{self, Source};
use sheet_engine::import::{self, ColType, DateOrder, Encoding, TextOptions};

/// The text import / Text to Columns wizard.
#[derive(Clone)]
pub struct TextWizard {
    pub path: String,
    pub bytes: std::sync::Arc<Vec<u8>>,
    pub opts: TextOptions,
    pub step: u8,
    /// Destination: None = a new sheet; Some(cell text) on the active sheet.
    pub dest: Option<String>,
    pub col_sel: usize,
}

impl TextWizard {
    pub fn text(&self) -> String { import::decode(&self.bytes, self.opts.encoding) }
    fn rows(&self, limit: usize) -> Vec<Vec<String>> {
        let t = self.text();
        let cut: String = t.split_inclusive('\n').take(limit + self.opts.start_row).collect();
        import::split(&cut, &self.opts)
    }
}

pub fn pick_text(app: &mut SheetApp) {
    if let Some(p) = rfd::FileDialog::new().add_filter(tr("Text Files"), &["csv", "txt", "tsv", "prn", "tab"]).pick_file() { open_text(app, &p.to_string_lossy()); }
}

pub fn open_text(app: &mut SheetApp, path: &str) {
    match std::fs::read(path) {
        Ok(b) => {
            let enc = Encoding::detect(&b);
            let mut opts = TextOptions::detect(&import::decode(&b, enc));
            opts.encoding = enc;
            open(app, Dlg::TextImport(TextWizard { path: path.to_string(), bytes: std::sync::Arc::new(b), opts, step: 1, dest: None, col_sel: 0 }));
        }
        Err(e) => app.error = Some(format!("{path}: {e}")),
    }
}

pub fn pick_workbook(app: &mut SheetApp) {
    if let Some(p) = rfd::FileDialog::new().add_filter(tr("Excel Workbook"), &["xlsx", "xlsm", "csv"]).pick_file() { open_workbook(app, &p.to_string_lossy()); }
}

pub fn open_workbook(app: &mut SheetApp, path: &str) {
    match sheet_engine::open(std::path::Path::new(path)) {
        Ok(wb) => {
            let items = import::workbook_items(&wb).into_iter().map(|(n, t, rows)| (n, t, rows.iter().take(30).map(|r| r.iter().map(|v| v.display()).collect()).collect())).collect();
            open(app, Dlg::Navigator(Navigator { source: NavSource::Workbook(path.to_string()), items, sel: 0, dest: None }));
        }
        Err(e) => app.error = Some(e),
    }
}

/// Recent Sources: a file path or a web address.
pub fn open_source(app: &mut SheetApp, src: &str) {
    if src.starts_with("http://") || src.starts_with("https://") { fetch_web(app, src); }
    else if src.to_ascii_lowercase().ends_with(".xlsx") || src.to_ascii_lowercase().ends_with(".xlsm") { open_workbook(app, src); }
    else { open_text(app, src); }
}

/// Text to Columns over the selected column.
pub fn open_text_to_columns(app: &mut SheetApp) {
    let s = app.sheet();
    let r = app.sel.range();
    if r.cols() > 1 { app.error = Some(tr("Text to Columns can convert only one column at a time.")); return; }
    let mut r = r;
    if r.is_cell() { if let Some(e) = app.book.sheets[s].extent() { r.r1 = r.r1.max(e.row.min(r.r0 + 100_000)); } }
    let lines: Vec<String> = (r.r0..=r.r1.min(r.r0 + 200)).map(|row| app.book.sheets[s].value(Pos::new(row, r.c0)).display()).collect();
    let text = lines.join("\n");
    let mut opts = TextOptions::detect(&text);
    opts.qualifier = Some('"');
    opts.header = false;
    open(app, Dlg::TextToColumns(TextWizard { path: String::new(), bytes: std::sync::Arc::new(text.into_bytes()), opts, step: 1, dest: Some(Pos::new(r.r0, r.c0).a1()), col_sel: 0 }));
}

fn type_label(t: ColType) -> String {
    match t { ColType::General => tr("General"), ColType::Text => tr("Text"), ColType::Date(o) => format!("{} ({})", tr("Date"), DateOrder::ALL.iter().find(|x| x.0 == o).map_or("", |x| x.1)), ColType::Skip => tr("Do not import column (skip)") }
}

pub fn text_wizard(app: &mut SheetApp, ctx: &egui::Context, w: &mut TextWizard, to_columns: bool) -> bool {
    let mut keep = true; let mut open_ = true;
    let title = if to_columns { format!("{} - {}", tr("Convert Text to Columns Wizard"), crate::i18n::trf("Step {} of 3", &[&w.step])) } else { format!("{} - {}", tr("Text Import Wizard"), crate::i18n::trf("Step {} of 3", &[&w.step])) };
    crate::theme::dialog(ctx, title).open(&mut open_).resizable(true).default_width(640.0).show(ctx, |ui| {
        ui.set_min_width(600.0);
        match w.step {
            1 => {
                ui.label(tr("Choose the file type that best describes your data:"));
                let mut fixed = w.opts.fixed.is_some();
                crate::theme::radio_value(ui, &mut fixed, false, tr("Delimited - Characters such as commas or tabs separate each field."));
                crate::theme::radio_value(ui, &mut fixed, true, tr("Fixed width - Fields are aligned in columns with spaces between each field."));
                if fixed != w.opts.fixed.is_some() { w.opts.fixed = fixed.then(|| import::guess_breaks(&w.text())); }
                if !to_columns {
                    ui.horizontal(|ui| {
                        ui.label(tr("Start import at row:"));
                        let mut row = w.opts.start_row + 1;
                        ui.add(egui::DragValue::new(&mut row).range(1..=100000));
                        w.opts.start_row = row - 1;
                        ui.label(tr("File origin:"));
                        let cur = Encoding::ALL.iter().find(|e| e.0 == w.opts.encoding).map_or("", |e| e.1);
                        crate::theme::Combo::from_id_salt("import-enc").selected_text(cur).show_ui(ui, |ui| for (e, l) in Encoding::ALL { ui.selectable_value(&mut w.opts.encoding, e, l); });
                    });
                    crate::theme::check(ui, &mut w.opts.header, tr("My data has headers."));
                }
            }
            2 if w.opts.fixed.is_none() => {
                ui.label(tr("Delimiters"));
                ui.horizontal(|ui| {
                    crate::theme::check(ui, &mut w.opts.tab, tr("Tab"));
                    crate::theme::check(ui, &mut w.opts.semicolon, tr("Semicolon"));
                    crate::theme::check(ui, &mut w.opts.comma, tr("Comma"));
                    crate::theme::check(ui, &mut w.opts.space, tr("Space"));
                    let mut other = w.opts.other.is_some();
                    crate::theme::check(ui, &mut other, tr("Other:"));
                    let mut t = w.opts.other.map(|c| c.to_string()).unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut t).desired_width(24.0).char_limit(1)).changed() || other != w.opts.other.is_some() { w.opts.other = if other { t.chars().next().or(Some('|')) } else { None }; }
                });
                ui.horizontal(|ui| {
                    crate::theme::check(ui, &mut w.opts.consecutive, tr("Treat consecutive delimiters as one"));
                    ui.label(tr("Text qualifier:"));
                    let label = match w.opts.qualifier { Some('"') => "\"", Some('\'') => "'", _ => "{none}" };
                    crate::theme::Combo::from_id_salt("import-q").selected_text(label).show_ui(ui, |ui| {
                        ui.selectable_value(&mut w.opts.qualifier, Some('"'), "\"");
                        ui.selectable_value(&mut w.opts.qualifier, Some('\''), "'");
                        ui.selectable_value(&mut w.opts.qualifier, None, "{none}");
                    });
                });
            }
            2 => {
                ui.label(tr("Set the field widths (column breaks): type the character positions where columns start, separated by commas."));
                let mut t = w.opts.fixed.clone().unwrap_or_default().iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ");
                if ui.text_edit_singleline(&mut t).changed() { let mut b: Vec<usize> = t.split(',').filter_map(|x| x.trim().parse().ok()).filter(|x| *x > 0).collect(); b.sort(); b.dedup(); w.opts.fixed = Some(b); }
                ui.monospace(w.text().lines().skip(w.opts.start_row).take(1).next().unwrap_or("").chars().take(90).collect::<String>());
            }
            _ => {
                ui.label(tr("Select each column and set its data format."));
                let n = w.rows(20).iter().map(|r| r.len()).max().unwrap_or(0);
                if w.opts.types.len() < n { w.opts.types.resize(n, ColType::General); }
                ui.horizontal(|ui| {
                    ui.label(tr("Column:"));
                    ui.add(egui::DragValue::new(&mut w.col_sel).range(0..=n.saturating_sub(1)).custom_formatter(|v, _| (v as usize + 1).to_string()));
                    let cur = w.opts.types.get(w.col_sel).copied().unwrap_or_default();
                    let mut t = cur;
                    crate::theme::Combo::from_id_salt("import-type").selected_text(type_label(cur)).show_ui(ui, |ui| {
                        ui.selectable_value(&mut t, ColType::General, tr("General"));
                        ui.selectable_value(&mut t, ColType::Text, tr("Text"));
                        for (o, l) in DateOrder::ALL { ui.selectable_value(&mut t, ColType::Date(o), format!("{} ({l})", tr("Date"))); }
                        ui.selectable_value(&mut t, ColType::Skip, tr("Do not import column (skip)"));
                    });
                    if t != cur && w.col_sel < w.opts.types.len() { w.opts.types[w.col_sel] = t; }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Destination:"));
                    let mut new_sheet = w.dest.is_none();
                    if !to_columns {
                        crate::theme::radio_value(ui, &mut new_sheet, true, tr("New worksheet"));
                        crate::theme::radio_value(ui, &mut new_sheet, false, tr("Existing worksheet"));
                        if new_sheet { w.dest = None; } else if w.dest.is_none() { w.dest = Some(app.sel.active.a1()); }
                    }
                    if let Some(d) = &mut w.dest { crate::refedit::field(ui, d, "data_import.rs:171", 80.0); }
                });
            }
        }
        // The preview.
        ui.add_space(6.0);
        ui.label(RichText::new(tr("Data preview")).strong());
        egui::ScrollArea::both().max_height(180.0).id_salt("import-preview").show(ui, |ui| {
            let rows = w.rows(20);
            crate::theme::grid(ui, egui::Grid::new("import-grid").striped(true).spacing(vec2(12.0, 2.0)), |ui| {
                if w.step == 3 {
                    for j in 0..rows.iter().map(|r| r.len()).max().unwrap_or(0) { let t = type_label(w.opts.types.get(j).copied().unwrap_or_default()); if ui.selectable_label(j == w.col_sel, RichText::new(t).small()).clicked() { w.col_sel = j; } }
                    ui.end_row();
                }
                for r in rows { for f in r { ui.label(f.chars().take(40).collect::<String>()); } ui.end_row(); }
            });
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Finish").clicked() { if finish(app, w, to_columns) { keep = false; } }
            if !to_columns && w.opts.fixed.is_none() && crate::theme::secondary(ui, "Transform Data").clicked() { transform_text(app, w); keep = false; }
            if w.step < 3 && crate::theme::secondary(ui, "Next >").clicked() { w.step += 1; }
            if w.step > 1 && crate::theme::secondary(ui, "< Back").clicked() { w.step -= 1; }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

fn finish(app: &mut SheetApp, w: &TextWizard, to_columns: bool) -> bool {
    let s = app.sheet();
    if to_columns {
        let r = app.sel.range();
        let r = Range { r1: r.r1.max(r.r0 + w.text().lines().count().saturating_sub(1) as u32), ..r };
        let dest = w.dest.as_deref().and_then(|d| parse_a1(d.trim().trim_start_matches('=').replace('$', "").as_str()));
        app.checkpoint("Text to Columns");
        import::text_to_columns(&mut app.book, s, Range { c1: r.c0, ..r }, &w.opts, dest);
        app.changed();
        return true;
    }
    let rows = import::split(&w.text(), &w.opts);
    if rows.is_empty() { app.error = Some(tr("The file has no data to import.")); return false; }
    app.checkpoint("Get Data");
    let (si, at) = match &w.dest {
        Some(d) => match parse_a1(d.trim().trim_start_matches('=').replace('$', "").as_str()) { Some(p) => (s, p), None => { app.error = Some(tr("The destination reference is not valid.")); return false; } },
        None => { let name = std::path::Path::new(&w.path).file_stem().and_then(|x| x.to_str()).unwrap_or("Data").to_string(); let n = app.book.unique_sheet_name(&name); (app.book.add_sheet(app.book.sheets.len(), Some(n)), Pos::new(0, 0)) }
    };
    let r = import::place(&mut app.book, si, at, &rows, &w.opts);
    let delimiter = if w.opts.comma { ',' } else if w.opts.semicolon { ';' } else if w.opts.space { ' ' } else if let Some(c) = w.opts.other { c } else { '\t' };
    let name = std::path::Path::new(&w.path).file_stem().and_then(|x| x.to_str()).unwrap_or("Query").to_string();
    if w.opts.fixed.is_none() { connections::register(&mut app.book, &name, Source::Text { path: w.path.clone(), delimiter, encoding: w.opts.encoding }, si, r); }
    if w.opts.header && r.rows() > 1 { let _ = app.book.create_table(si, r, true, "TableStyleMedium2"); }
    app.book.active = si;
    app.sel = crate::app::Selection::at(at);
    crate::data_tab::note_recent(app, &w.path);
    app.changed();
    true
}

// ---- the Navigator --------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
pub enum NavSource { Workbook(String), Web(String) }

#[derive(Clone)]
pub struct Navigator {
    pub source: NavSource,
    /// (name, is a table, preview rows)
    pub items: Vec<(String, bool, Vec<Vec<String>>)>,
    pub sel: usize,
    pub dest: Option<String>,
}

fn fetch_web(app: &mut SheetApp, url: &str) {
    match crate::data_tab::fetch(url) {
        Ok(html) => {
            let t = import::html_tables(&html);
            if t.is_empty() { app.error = Some(tr("No tables were found on that page.")); return; }
            let items = t.into_iter().map(|t| (t.name, true, t.rows.into_iter().take(30).collect())).collect();
            open(app, Dlg::Navigator(Navigator { source: NavSource::Web(url.to_string()), items, sel: 0, dest: None }));
        }
        Err(e) => app.error = Some(format!("{}: {e}", tr("We couldn't connect to the web page"))),
    }
}

pub fn web_dialog(app: &mut SheetApp, ctx: &egui::Context, url: &mut String) -> bool {
    let mut keep = true; let mut open_ = true;
    crate::theme::dialog(ctx, "From Web").open(&mut open_).resizable(false).default_width(480.0).show(ctx, |ui| {
        ui.set_min_width(460.0);
        ui.label(tr("URL"));
        let r = ui.add(egui::TextEdit::singleline(url).desired_width(440.0).hint_text("https://"));
        let go = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() || go {
                let u = url.trim().to_string();
                let u = if u.contains("://") { u } else { format!("https://{u}") };
                keep = false;
                fetch_web(app, &u);
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

pub fn navigator(app: &mut SheetApp, ctx: &egui::Context, n: &mut Navigator) -> bool {
    let mut keep = true; let mut open_ = true;
    crate::theme::dialog(ctx, "Navigator").open(&mut open_).resizable(true).default_width(720.0).show(ctx, |ui| {
        ui.set_min_width(680.0);
        ui.label(match &n.source { NavSource::Workbook(p) | NavSource::Web(p) => p.clone() });
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                egui::ScrollArea::vertical().id_salt("nav-list").max_height(320.0).show(ui, |ui| {
                    for (i, (name, table, _)) in n.items.iter().enumerate() {
                        let icon = if *table { "▦ " } else { "▤ " };
                        if ui.selectable_label(n.sel == i, format!("{icon}{name}")).clicked() { n.sel = i; }
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::both().id_salt("nav-preview").max_height(320.0).show(ui, |ui| {
                if let Some((_, _, rows)) = n.items.get(n.sel) {
                    crate::theme::grid(ui, egui::Grid::new("nav-grid").striped(true), |ui| { for r in rows.iter().take(30) { for v in r { ui.label(v.chars().take(30).collect::<String>()); } ui.end_row(); } });
                }
            });
        });
        ui.horizontal(|ui| {
            let mut new_sheet = n.dest.is_none();
            crate::theme::radio_value(ui, &mut new_sheet, true, tr("New worksheet"));
            crate::theme::radio_value(ui, &mut new_sheet, false, tr("Existing worksheet"));
            if new_sheet { n.dest = None; } else if n.dest.is_none() { n.dest = Some(app.sel.active.a1()); }
            if let Some(d) = &mut n.dest { crate::refedit::field(ui, d, "data_import.rs:301", 80.0); }
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "Load").clicked() { if load_item(app, n) { keep = false; } }
            if crate::theme::secondary(ui, "Transform Data").clicked() { transform_item(app, n); keep = false; }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

fn load_item(app: &mut SheetApp, n: &Navigator) -> bool {
    let Some((name, _, _)) = n.items.get(n.sel).cloned() else { return false };
    let s = app.sheet();
    let (src, recent) = match &n.source { NavSource::Workbook(p) => (Source::Workbook { path: p.clone(), item: name.clone() }, p.clone()), NavSource::Web(u) => (Source::Web { url: u.clone(), table: n.sel + 1 }, u.clone()) };
    app.checkpoint("Get Data");
    let (si, at) = match &n.dest {
        Some(d) => match parse_a1(d.trim().trim_start_matches('=').replace('$', "").as_str()) { Some(p) => (s, p), None => { app.error = Some(tr("The destination reference is not valid.")); return false; } },
        None => { let nm = app.book.unique_sheet_name(&name); (app.book.add_sheet(app.book.sheets.len(), Some(nm)), Pos::new(0, 0)) }
    };
    match connections::add(&mut app.book, &name, src, si, at, &crate::data_tab::fetch) {
        Ok(id) => {
            if let Some((_, r)) = connections::destination(&app.book, id) { if r.rows() > 1 && app.book.table_at(si, r.start()).is_none() { let _ = app.book.create_table(si, r, true, "TableStyleMedium2"); } }
            app.book.active = si;
            app.sel = crate::app::Selection::at(at);
            crate::data_tab::note_recent(app, &recent);
            app.changed();
            true
        }
        Err(e) => { app.error = Some(e); false }
    }
}

pub fn table_range_dialog(app: &mut SheetApp, ctx: &egui::Context, range: &mut String) -> bool {
    let mut keep = true; let mut open_ = true;
    crate::theme::dialog(ctx, "Create Query").open(&mut open_).resizable(false).show(ctx, |ui| {
        ui.label(tr("Where is the data for your query?"));
        crate::refedit::field(ui, range, "data_import.rs:337", 260.0);
        ui.label(RichText::new(tr("A range that isn't a table becomes one; the query opens in the Power Query Editor.")).small());
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let text = range.trim().trim_start_matches('=').to_string();
                let (sheet, r) = text.rsplit_once('!').map(|(s, r)| (app.book.sheet_index(s.trim_matches('\'')), r.to_string())).unwrap_or((Some(app.sheet()), text.clone()));
                match (sheet, sheet_engine::addr::parse_range(&r.replace('$', "")).or_else(|| parse_a1(&r.replace('$', "")).map(Range::cell))) {
                    (Some(si), Some(r)) => {
                        app.checkpoint("From Table/Range");
                        match sheet_engine::powerquery::from_table_range(&mut app.book, si, r, true) {
                            Ok(q) => { app.changed(); keep = false; crate::pq_editor::open(app, None, Some(q)); }
                            Err(e) => app.error = Some(e),
                        }
                    }
                    _ => app.error = Some(tr("The destination reference is not valid.")),
                }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

// ---- Transform Data: the source as a query in the Power Query Editor ----------------------------

fn transform_text(app: &mut SheetApp, w: &TextWizard) {
    use sheet_engine::powerquery::{self as pq, Op, Query, Source as QS};
    let delimiter = if w.opts.comma { ',' } else if w.opts.semicolon { ';' } else if w.opts.space { ' ' } else if let Some(c) = w.opts.other { c } else { '\t' };
    let encoding = match w.opts.encoding { Encoding::Utf8 => 65001, Encoding::Utf16Le => 1200, Encoding::Utf16Be => 1201, Encoding::Latin1 => 28591, Encoding::Windows1252 => 1252 };
    let columns = w.rows(200).iter().map(|r| r.len()).max();
    let name = std::path::Path::new(&w.path).file_stem().and_then(|x| x.to_str()).unwrap_or("Query").to_string();
    let mut q = Query::new(&pq::unique_name(&app.book, &name), QS::Csv { path: w.path.clone(), delimiter: delimiter.to_string(), encoding, columns });
    if w.opts.header { q.add_step(Op::PromoteHeaders, None); }
    pq::detect_types(&app.book, &mut q);
    crate::data_tab::note_recent(app, &w.path);
    crate::pq_editor::open(app, None, Some(q));
}

fn transform_item(app: &mut SheetApp, n: &Navigator) {
    use sheet_engine::powerquery::{self as pq, Op, Query, Source as QS};
    let Some((name, table, _)) = n.items.get(n.sel).cloned() else { return };
    let (src, recent) = match &n.source {
        NavSource::Workbook(p) => (QS::Workbook { path: p.clone(), item: name.clone(), kind: if table { "Table".into() } else { "Sheet".into() } }, p.clone()),
        NavSource::Web(u) => (QS::Web { url: u.clone(), index: n.sel }, u.clone()),
    };
    let sheet = matches!(&src, QS::Workbook { kind, .. } if kind == "Sheet");
    let mut q = Query::new(&pq::unique_name(&app.book, &name), src);
    if sheet { q.add_step(Op::PromoteHeaders, None); }
    pq::detect_types(&app.book, &mut q);
    crate::data_tab::note_recent(app, &recent);
    crate::pq_editor::open(app, None, Some(q));
}
