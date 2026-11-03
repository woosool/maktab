//! PivotTables and slicers on screen (pivotdialog.js, recommended-pivots.js, slicer*.js,
//! insertslicers.js): Insert PivotTable, the PivotTable Fields pane (fields with checkboxes,
//! Filters / Columns / Rows / Values areas with drag and drop and value summaries), the
//! PivotTable Analyze tab, and slicers that filter a PivotTable or a table.

use crate::app::{rgb, SheetApp};
use crate::grid::SELECT;
use crate::i18n::tr;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, RichText, Stroke, StrokeKind, Ui};
use qalam_kit::colors::*;
use qalam_kit::widgets::*;
use sheet_engine::addr::{Pos, Range};
use sheet_engine::drawing::{ObjKind, Object, Slicer};
use sheet_engine::pivot::{Agg, BaseItem, DateLevel, Grouping, Hit, Layout, Pivot, ShowAs, Subtotals, ValueField};
use sheet_engine::Value;

/// The pivot whose report holds the active cell (index into the active sheet's pivots).
pub fn pivot_at(app: &SheetApp) -> Option<usize> {
    let s = &app.book.sheets[app.sheet()];
    let p = app.sel.active;
    s.pivots.iter().position(|v| v.anchor == p || v.output.is_some_and(|o| o.contains(p)))
}

/// Runs a pivot again (after a change to its fields or source) and recalculates.
pub fn rerun(app: &mut SheetApp, si: usize, k: usize) {
    let mut p = app.book.sheets[si].pivots[k].clone();
    if !sheet_engine::follow::pivot_source_valid(&app.book, &p) { app.notify(sheet_engine::follow::INVALID_PIVOT); return; }
    p.apply(&mut app.book, si);
    app.book.sheets[si].pivots[k] = p;
    app.changed();
}

fn edit_pivot(app: &mut SheetApp, si: usize, k: usize, label: &'static str, f: impl FnOnce(&mut Pivot)) {
    app.checkpoint(label);
    f(&mut app.book.sheets[si].pivots[k]);
    rerun(app, si, k);
}

fn unique_pivot_name(app: &SheetApp) -> String {
    (1..).map(|i| format!("PivotTable{i}")).find(|n| !app.book.sheets.iter().any(|s| s.pivots.iter().any(|p| &p.name == n))).unwrap()
}

// ---- Insert PivotTable --------------------------------------------------------------------------------

#[derive(Clone)]
pub struct InsertPivot { pub range: String, pub new_sheet: bool, pub dest: String, pub error: String }

pub fn open_insert(app: &mut SheetApp) {
    app.commit(None);
    let r = app.sel.range();
    let r = if r.is_cell() { app.book.current_region(app.sheet(), app.sel.active) } else { r };
    let range = sheet_engine::chart::make_ref(&app.book.sheets[app.sheet()].name, r);
    crate::objects::open_dialog(app, crate::objects::ObjDialog::InsertPivot(InsertPivot { range, new_sheet: true, dest: String::new(), error: String::new() }));
}

fn resolve_ref(app: &SheetApp, text: &str) -> Option<(usize, Range)> {
    let (sheet, r) = sheet_engine::chart::parse_ref(text)?;
    let si = match sheet { Some(n) => app.book.sheet_index(&n)?, None => app.sheet() };
    Some((si, r))
}

pub fn insert_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut InsertPivot) -> bool {
    let mut keep = true;
    let mut open = true;
    crate::theme::dialog(ctx, "Create PivotTable").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(380.0);
        ui.label(tr("Choose the data that you want to analyze"));
        crate::theme::hrow(ui, |ui| { ui.label(tr("Table/Range:")); crate::refedit::field(ui, &mut d.range, "pivot_ui.rs:68", 220.0); });
        ui.add_space(6.0);
        let changing = d.dest.starts_with("#change");
        if !changing {
            ui.label(tr("Choose where you want the PivotTable report to be placed"));
            ui.radio_value(&mut d.new_sheet, true, tr("New Worksheet"));
            ui.radio_value(&mut d.new_sheet, false, tr("Existing Worksheet"));
        }
        if !d.new_sheet && !changing {
            crate::theme::hrow(ui, |ui| {
                ui.label(tr("Location:"));
                crate::refedit::field(ui, &mut d.dest, "pivot_ui.rs:79", 160.0);
                if ui.button(tr("Use Active Cell")).clicked() { d.dest = sheet_engine::chart::make_ref(&app.book.sheets[app.sheet()].name, Range::cell(app.sel.active)); }
            });
        }
        if !d.error.is_empty() { ui.colored_label(Color32::from_rgb(0xC0, 0x20, 0x20), &d.error); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let res = change_source(app, d).unwrap_or_else(|| create(app, d));
                match res { Ok(()) => keep = false, Err(e) => d.error = tr(&e) }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

fn create(app: &mut SheetApp, d: &InsertPivot) -> Result<(), String> {
    let (src, r) = resolve_ref(app, &d.range).ok_or("The data source reference isn't valid.")?;
    if r.rows() < 2 { return Err("A PivotTable needs a header row and at least one row of data.".into()); }
    let r = { let last = app.book.sheets[src].extent().map(|e| e.row).unwrap_or(r.r0 + 1); Range { r1: r.r1.min(last.max(r.r0 + 1)), ..r } };
    let src_name = app.book.sheets[src].name.clone();
    let dest = if d.new_sheet { None } else { Some(resolve_ref(app, &d.dest).ok_or("The destination reference isn't valid.")?) };
    app.checkpoint("Insert PivotTable");
    let (si, anchor) = match dest {
        Some((si, r)) => (si, r.start()),
        None => { let at = app.sheet(); let si = app.book.add_sheet(at, None); (si, Pos::new(2, 0)) }
    };
    let name = unique_pivot_name(app);
    let p = Pivot::new(&name, &src_name, r, anchor);
    app.book.sheets[si].pivots.push(p);
    app.book.active = si;
    app.sel = crate::app::Selection::at(anchor);
    app.reveal = true;
    app.changed();
    let mut st = crate::objects::state(&app.ctx); st.pane_closed = None; crate::objects::set_state(&app.ctx, st);
    Ok(())
}

/// Recommended PivotTables: a text field by the first numeric field, summed.
pub fn recommended(app: &mut SheetApp) {
    let r = app.sel.range();
    let r = if r.is_cell() { app.book.current_region(app.sheet(), app.sel.active) } else { r };
    let si = app.sheet();
    if r.rows() < 2 { app.error = Some(tr("Select a range with a header row and data first.")); return; }
    let s = &app.book.sheets[si];
    let numeric = |c: u32| (r.r0 + 1..=r.r1.min(r.r0 + 200)).any(|row| matches!(s.value(Pos::new(row, c)), Value::Num(_)));
    let text_col = (r.c0..=r.c1).find(|&c| !numeric(c));
    let num_col = (r.c0..=r.c1).find(|&c| numeric(c));
    let d = InsertPivot { range: sheet_engine::chart::make_ref(&s.name, r), new_sheet: true, dest: String::new(), error: String::new() };
    if let Err(e) = create(app, &d) { app.error = Some(tr(&e)); return; }
    let si = app.sheet();
    let k = app.book.sheets[si].pivots.len() - 1;
    let p = &mut app.book.sheets[si].pivots[k];
    if let Some(c) = text_col { p.rows = vec![(c - r.c0) as usize]; }
    if let Some(c) = num_col { p.values = vec![ValueField::new((c - r.c0) as usize, Agg::Sum)]; } else if let Some(c) = text_col { p.values = vec![ValueField::new((c - r.c0) as usize, Agg::Count)]; }
    rerun(app, si, k);
}

// ---- the PivotTable Fields pane ------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Area { Filters, Columns, Rows, Values }

/// What is being dragged: a field, and the area and position it comes from.
#[derive(Clone, Copy)]
struct Dragged { field: usize, from: Option<(Area, usize)> }

fn area_list(p: &Pivot, a: Area) -> Vec<usize> {
    match a { Area::Filters => p.filters.clone(), Area::Columns => p.cols.clone(), Area::Rows => p.rows.clone(), Area::Values => p.values.iter().map(|v| v.field).collect() }
}
fn remove_from(p: &mut Pivot, a: Area, i: usize) {
    match a { Area::Filters => { if i < p.filters.len() { p.filters.remove(i); } } Area::Columns => { if i < p.cols.len() { p.cols.remove(i); } } Area::Rows => { if i < p.rows.len() { p.rows.remove(i); } } Area::Values => { if i < p.values.len() { p.values.remove(i); } } }
}
fn insert_into(p: &mut Pivot, a: Area, at: usize, field: usize, numeric: bool) {
    // A field sits in one of Filters/Columns/Rows at a time (Values may repeat it).
    if a != Area::Values { p.filters.retain(|f| *f != field); p.cols.retain(|f| *f != field); p.rows.retain(|f| *f != field); }
    match a {
        Area::Filters => { let at = at.min(p.filters.len()); p.filters.insert(at, field); }
        Area::Columns => { let at = at.min(p.cols.len()); p.cols.insert(at, field); }
        Area::Rows => { let at = at.min(p.rows.len()); p.rows.insert(at, field); }
        Area::Values => { let at = at.min(p.values.len()); p.values.insert(at, ValueField::new(field, if numeric { Agg::Sum } else { Agg::Count })); }
    }
}

fn field_numeric(app: &SheetApp, p: &Pivot, f: usize) -> bool {
    let Some(si) = app.book.sheet_index(&p.source_sheet) else { return false };
    let s = &app.book.sheets[si];
    let c = p.source.c0 + f as u32;
    let mut nums = 0; let mut texts = 0;
    for r in p.source.r0 + 1..=p.source.r1.min(p.source.r0 + 300) { match s.value(Pos::new(r, c)) { Value::Num(_) => nums += 1, Value::Str(_) => texts += 1, _ => {} } }
    nums > texts
}

pub fn panel(app: &mut SheetApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    dialogs(app, &ctx);
    let Some(k) = pivot_at(app) else { return };
    let si = app.sheet();
    let name = app.book.sheets[si].pivots[k].name.clone();
    if crate::objects::state(&app.ctx).pane_closed.as_deref() == Some(name.as_str()) { return; }
    egui::Panel::right("pivot-fields").resizable(true).default_size(270.0).min_size(220.0)
        .frame(egui::Frame::NONE.fill(c_frame()).inner_margin(egui::Margin::same(10)).stroke(Stroke::new(1.0, c_line())))
        .show(ui, |ui| fields_pane(app, ui, si, k));
}

fn fields_pane(app: &mut SheetApp, ui: &mut Ui, si: usize, k: usize) {
    let p = app.book.sheets[si].pivots[k].clone();
    let fields = p.fields(&app.book);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("PivotTable Fields")).size(15.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if close_button(ui, "Close").clicked() { let mut st = crate::objects::state(&app.ctx); st.pane_closed = Some(p.name.clone()); crate::objects::set_state(&app.ctx, st); }
        });
    });
    ui.label(RichText::new(tr("Choose fields to add to report:")).color(c_ink2()));
    let search_id = egui::Id::new(("pivot-search", si, k));
    let mut q = ui.data(|d| d.get_temp::<String>(search_id)).unwrap_or_default();
    if ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Search")).desired_width(f32::INFINITY)).changed() { ui.data_mut(|d| d.insert_temp(search_id, q.clone())); }
    let mut action: Option<Box<dyn FnOnce(&mut Pivot)>> = None;
    let mut label: &'static str = "PivotTable Fields";
    egui::Frame::NONE.fill(c_card()).stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("pivot-field-list").max_height(180.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (f, n) in fields.iter().enumerate() {
                if !q.is_empty() && !n.to_lowercase().contains(&q.to_lowercase()) { continue; }
                let used = p.rows.contains(&f) || p.cols.contains(&f) || p.filters.contains(&f) || p.values.iter().any(|v| v.field == f);
                let id = egui::Id::new(("pivot-field", si, k, f));
                // The checkbox takes clicks; the handle beside it drags the field to an area.
                ui.horizontal(|ui| {
                    ui.dnd_drag_source(id, Dragged { field: f, from: None }, |ui| { ui.label(RichText::new("⠿").color(c_ink3())); }).response.on_hover_text(tr("Drag to an area below"));
                    let mut on = used;
                    let cb = ui.checkbox(&mut on, n);
                    cb.context_menu(|ui| {
                        if p.calc_index(f).is_none() && ui.button(tr("Group…")).clicked() { open_group(app, si, k, f); ui.close(); }
                        if p.group_of(p.base_of(f)).is_some() || p.extra.iter().any(|e| e.base == p.base_of(f)) { if ui.button(tr("Ungroup")).clicked() { label = "Ungroup"; action = Some(Box::new(move |p: &mut Pivot| p.ungroup(f))); ui.close(); } }
                        if let Some(c) = p.calc_index(f) { if ui.button(tr("Delete Calculated Field")).clicked() { label = "Delete Calculated Field"; action = Some(Box::new(move |p: &mut Pivot| p.remove_calc(c))); ui.close(); } }
                    });
                    if cb.clicked() {
                        let numeric = field_numeric(app, &p, f);
                        label = if on { "Add Field" } else { "Remove Field" };
                        action = Some(Box::new(move |p: &mut Pivot| {
                            if on { if numeric { insert_into(p, Area::Values, usize::MAX, f, true); } else { insert_into(p, Area::Rows, usize::MAX, f, false); } }
                            else { p.rows.retain(|x| *x != f); p.cols.retain(|x| *x != f); p.filters.retain(|x| *x != f); p.values.retain(|v| v.field != f); p.hidden.remove(&f); }
                        }));
                    }
                });
            }
        });
    });
    ui.add_space(6.0);
    ui.label(RichText::new(tr("Drag fields between areas below:")).color(c_ink2()));
    let areas = [(Area::Filters, "Filters"), (Area::Columns, "Columns"), (Area::Rows, "Rows"), (Area::Values, "Values")];
    let w = ((ui.available_width() - 8.0) / 2.0).max(60.0);
    egui::Grid::new("pivot-areas").num_columns(2).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (n, (a, title)) in areas.iter().enumerate() {
            ui.vertical(|ui| {
                ui.set_width(w);
                ui.label(RichText::new(tr(title)).strong());
                let frame = egui::Frame::NONE.fill(c_card()).stroke(Stroke::new(1.0, c_line())).inner_margin(4);
                let (_, dropped) = ui.dnd_drop_zone::<Dragged, _>(frame, |ui| {
                    ui.set_min_size(vec2(w - 10.0, 90.0));
                    let list = area_list(&p, *a);
                    for (i, f) in list.iter().enumerate() {
                        let text = if *a == Area::Values { p.value_name(&app.book, &p.values[i]) } else { fields.get(*f).cloned().unwrap_or_default() };
                        let id = egui::Id::new(("pivot-area-item", si, k, n, i));
                        let r = ui.dnd_drag_source(id, Dragged { field: *f, from: Some((*a, i)) }, |ui| {
                            egui::Frame::NONE.fill(c_hover()).corner_radius(3).inner_margin(egui::Margin::symmetric(5, 2)).show(ui, |ui| { ui.set_width(w - 22.0); ui.add(egui::Label::new(text).truncate()); });
                        }).response;
                        let (a2, i2, f2) = (*a, i, *f);
                        r.context_menu(|ui| {
                            for &(to, t) in areas.iter() { if to != a2 && ui.button(format!("{} {}", tr("Move to"), tr(t))).clicked() { let numeric = field_numeric(app, &p, f2); label = "Move Field"; action = Some(Box::new(move |p: &mut Pivot| { remove_from(p, a2, i2); insert_into(p, to, usize::MAX, f2, numeric); })); ui.close(); } }
                            if i2 > 0 && ui.button(tr("Move Up")).clicked() { label = "Move Field"; action = Some(Box::new(move |p: &mut Pivot| { swap(p, a2, i2, i2 - 1); })); ui.close(); }
                            if ui.button(tr("Move Down")).clicked() { label = "Move Field"; action = Some(Box::new(move |p: &mut Pivot| { swap(p, a2, i2, i2 + 1); })); ui.close(); }
                            if a2 == Area::Values {
                                ui.separator();
                                if ui.button(tr("Value Field Settings…")).clicked() { open_value_settings(app, si, k, i2); ui.close(); }
                                for g in Agg::ALL { if ui.radio(p.values[i2].agg == g, tr(g.label())).clicked() { label = "Summarize Values By"; action = Some(Box::new(move |p: &mut Pivot| { if let Some(v) = p.values.get_mut(i2) { v.agg = g; } })); ui.close(); } }
                            }
                            if a2 == Area::Filters || a2 == Area::Rows || a2 == Area::Columns {
                                ui.separator();
                                if p.calc_index(f2).is_none() && ui.button(tr("Group…")).clicked() { open_group(app, si, k, f2); ui.close(); }
                                if p.group_of(p.base_of(f2)).is_some() || p.extra.iter().any(|e| e.base == p.base_of(f2)) { if ui.button(tr("Ungroup")).clicked() { label = "Ungroup"; action = Some(Box::new(move |p: &mut Pivot| p.ungroup(f2))); ui.close(); } }
                                let items = p.items(&app.book, f2);
                                menu_heading(ui, &tr("Show items"));
                                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                    for it in items {
                                        let hidden = p.hidden.get(&f2).is_some_and(|h| h.contains(&it));
                                        let mut on = !hidden;
                                        if ui.checkbox(&mut on, &it).clicked() { label = "Filter"; let it = it.clone(); action = Some(Box::new(move |p: &mut Pivot| { let h = p.hidden.entry(f2).or_default(); if on { h.remove(&it); } else { h.insert(it); } })); }
                                    }
                                });
                            }
                            ui.separator();
                            if ui.button(tr("Remove Field")).clicked() { label = "Remove Field"; action = Some(Box::new(move |p: &mut Pivot| remove_from(p, a2, i2))); ui.close(); }
                        });
                    }
                });
                if let Some(d) = dropped {
                    let numeric = field_numeric(app, &p, d.field);
                    let to = *a;
                    label = "Move Field";
                    action = Some(Box::new(move |p: &mut Pivot| {
                        if let Some((from, i)) = d.from { if from == to { return; } remove_from(p, from, i); }
                        insert_into(p, to, usize::MAX, d.field, numeric);
                    }));
                }
            });
            if n % 2 == 1 { ui.end_row(); }
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(tr("Refresh")).clicked() { rerun(app, si, k); }
        if ui.button(tr("Insert Slicer")).clicked() { open_slicers(app); }
    });
    if let Some(f) = action { edit_pivot(app, si, k, label, f); }
}

fn swap(p: &mut Pivot, a: Area, i: usize, j: usize) {
    match a {
        Area::Filters => if j < p.filters.len() { p.filters.swap(i, j) },
        Area::Columns => if j < p.cols.len() { p.cols.swap(i, j) },
        Area::Rows => if j < p.rows.len() { p.rows.swap(i, j) },
        Area::Values => if j < p.values.len() { p.values.swap(i, j) },
    }
}

// ---- the PivotTable Analyze tab --------------------------------------------------------------------------

pub fn analyze_tab(app: &mut SheetApp, ui: &mut Ui) {
    let Some(k) = pivot_at(app) else { return };
    let si = app.sheet();
    let p = app.book.sheets[si].pivots[k].clone();
    group(ui, "PivotTable", |ui| {
        crate::theme::vcol(ui, |ui| {
            ui.label(tr("PivotTable Name:"));
            let id = egui::Id::new(("pivot-name", si, k));
            let mut name = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| p.name.clone());
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
            if r.changed() { ui.data_mut(|d| d.insert_temp(id, name.clone())); }
            if r.lost_focus() {
                ui.data_mut(|d| d.remove::<String>(id));
                let taken = app.book.sheets.iter().any(|s| s.pivots.iter().any(|x| x.name == name.trim()));
                if !name.trim().is_empty() && !taken {
                    let old = p.name.clone();
                    let new = name.trim().to_string();
                    app.checkpoint("PivotTable Name");
                    app.book.sheets[si].pivots[k].name = new.clone();
                    for s in &mut app.book.sheets { for o in &mut s.objects { if let ObjKind::Slicer(sl) = &mut o.kind { if sl.pivot == old { sl.pivot = new.clone(); } } } }
                    app.dirty = true;
                }
            }
        });
    });
    group(ui, "Data", |ui| {
        if large(ui, "updateTable", "Refresh", false, true, "Get the latest data from the source").clicked() { app.checkpoint("Refresh"); rerun(app, si, k); }
        if large(ui, "selectTable", "Change Data Source", false, true, "Use a different source range").clicked() {
            crate::objects::open_dialog(app, crate::objects::ObjDialog::InsertPivot(InsertPivot { range: sheet_engine::chart::make_ref(&p.source_sheet, p.source), new_sheet: false, dest: format!("#change{k}"), error: String::new() }));
        }
    });
    group(ui, "Filter", |ui| {
        if large(ui, "filter", "Insert Slicer", false, true, "Filter the PivotTable with buttons").clicked() { open_slicers(app); }
    });
    group(ui, "Show", |ui| {
        let closed = crate::objects::state(&app.ctx).pane_closed.as_deref() == Some(p.name.as_str());
        if large(ui, "selectionPane", "Field List", !closed, true, "Show or hide the PivotTable Fields pane").clicked() {
            let mut st = crate::objects::state(&app.ctx); st.pane_closed = if closed { None } else { Some(p.name.clone()) }; crate::objects::set_state(&app.ctx, st);
        }
    });
    group(ui, "Group", |ui| {
        let hit = hit_at(app, si, k);
        let field = match hit { Some(Hit::Field(f)) => Some(f), _ => p.rows.first().or(p.cols.first()).copied() };
        if large(ui, "group", "Group Field", false, field.is_some(), "Group dates by years, quarters, months... or numbers in ranges").clicked() { if let Some(f) = field { open_group(app, si, k, f); } }
        let grouped = field.is_some_and(|f| p.group_of(p.base_of(f)).is_some() || p.extra.iter().any(|e| e.base == p.base_of(f)));
        if large(ui, "ungroup", "Ungroup", false, grouped, "Remove the grouping of the field").clicked() { if let Some(f) = field { edit_pivot(app, si, k, "Ungroup", move |p| p.ungroup(f)); } }
    });
    group(ui, "Calculations", |ui| {
        let r = large_menu(ui, "formula", "Fields, Items & Sets", "Calculated fields");
        crate::theme::menu(&r, |ui| { if ui.button(tr("Calculated Field…")).clicked() { open_calc(app, si, k); ui.close(); } });
        let vi = match hit_at(app, si, k) { Some(Hit::Value(i)) if i < p.values.len() => Some(i), _ => (!p.values.is_empty()).then_some(0) };
        if large(ui, "settings", "Field Settings", false, vi.is_some(), "Value Field Settings: summarize by, show values as, number format").clicked() { if let Some(i) = vi { open_value_settings(app, si, k, i); } }
    });
    group(ui, "Actions", |ui| {
        if large(ui, "deleteTable", "Delete PivotTable", false, true, "Remove the PivotTable and its report").clicked() {
            app.checkpoint("Delete PivotTable");
            let p = app.book.sheets[si].pivots.remove(k);
            if let Some(o) = p.output { app.book.clear(si, o, true, true, false); }
            app.changed();
        }
    });
}

/// "Change Data Source" reuses the Create PivotTable dialog with a marker destination.
pub fn change_source(app: &mut SheetApp, d: &InsertPivot) -> Option<Result<(), String>> {
    let k: usize = d.dest.strip_prefix("#change")?.parse().ok()?;
    let si = app.sheet();
    Some((|| {
        let (src, r) = resolve_ref(app, &d.range).ok_or("The data source reference isn't valid.")?;
        if k >= app.book.sheets[si].pivots.len() { return Err("The PivotTable is gone.".into()); }
        let name = app.book.sheets[src].name.clone();
        edit_pivot(app, si, k, "Change Data Source", |p| { let n = r.cols() as usize; p.source_sheet = name; p.source = r; p.rows.retain(|f| *f < n); p.cols.retain(|f| *f < n); p.filters.retain(|f| *f < n); p.values.retain(|v| v.field < n); });
        Ok(())
    })())
}

// ---- slicers ------------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct InsertSlicer { pub pivot: Option<String>, pub table: Option<String>, pub fields: Vec<(String, bool)> }

/// Insert ▸ Slicer: for the pivot or table at the active cell.
pub fn open_slicers(app: &mut SheetApp) {
    let si = app.sheet();
    if let Some(k) = pivot_at(app) {
        let p = &app.book.sheets[si].pivots[k];
        let fields = p.fields(&app.book).into_iter().map(|f| (f, false)).collect();
        crate::objects::open_dialog(app, crate::objects::ObjDialog::InsertSlicer(InsertSlicer { pivot: Some(p.name.clone()), table: None, fields }));
    } else if let Some(t) = app.book.table_at(si, app.sel.active) {
        let t = &app.book.sheets[si].tables[t];
        let fields = t.columns.iter().map(|c| (c.name.clone(), false)).collect();
        crate::objects::open_dialog(app, crate::objects::ObjDialog::InsertSlicer(InsertSlicer { pivot: None, table: Some(t.name.clone()), fields }));
    } else {
        app.error = Some(tr("Select a cell in a PivotTable or a table to insert a slicer."));
    }
}

pub fn slicer_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut InsertSlicer) -> bool {
    let mut keep = true;
    let mut open = true;
    crate::theme::dialog(ctx, "Insert Slicers").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(260.0);
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| for (n, on) in &mut d.fields { ui.checkbox(on, n.as_str()); });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let chosen: Vec<(usize, String)> = d.fields.iter().enumerate().filter(|(_, f)| f.1).map(|(i, f)| (i, f.0.clone())).collect();
                for (n, (f, caption)) in chosen.into_iter().enumerate() {
                    let sl = Slicer { pivot: d.pivot.clone().unwrap_or_default(), table: d.table.clone().unwrap_or_default(), field: f, caption };
                    let mut a = crate::objects::anchor_at_cell(app, 180.0, 230.0);
                    if let sheet_engine::drawing::Anchor::Two { from, to, .. } = &mut a { from.col += 3 + 3 * n as u32; to.col += 3 + 3 * n as u32; from.dy += 0.0; }
                    if d.table.is_some() { if let Some((ts, ti)) = app.book.find_table(&sl.table) { if app.book.sheets[ts].tables[ti].filter.is_none() { app.book.sheets[ts].tables[ti].filter = Some(Default::default()); } } }
                    crate::objects::insert(app, Object::new(ObjKind::Slicer(sl), a), "Slicer", "Insert Slicer");
                }
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

const HEAD: f32 = 30.0;
const ITEM: f32 = 26.0;

fn item_rects(r: Rect, n: usize, z: f32) -> Vec<Rect> {
    let top = r.min.y + HEAD * z;
    (0..n).map(|i| Rect::from_min_size(pos2(r.min.x + 8.0 * z, top + i as f32 * (ITEM + 4.0) * z), vec2(r.width() - 16.0 * z, ITEM * z))).take_while(|x| x.max.y <= r.max.y - 2.0 * z).collect()
}
fn clear_rect(r: Rect, z: f32) -> Rect { Rect::from_min_size(pos2(r.max.x - 26.0 * z, r.min.y + 6.0 * z), vec2(18.0 * z, 18.0 * z)) }

pub fn paint_slicer(app: &SheetApp, p: &egui::Painter, s: &Slicer, r: Rect, z: f32) {
    p.rect_filled(r, 2.0, Color32::WHITE);
    p.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_gray(0xBF)), StrokeKind::Inside);
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    p.text(pos2(r.min.x + 8.0 * z, r.min.y + HEAD * z / 2.0), Align2::LEFT_CENTER, &s.caption, FontId::new(13.0 * z, egui::FontFamily::Name("ui-bold".into())), Color32::from_gray(0x40));
    let items = sheet_engine::drawing::slicer_items(&app.book, s);
    let filtered = items.iter().any(|i| !i.1);
    let cr = clear_rect(r, z);
    p.text(cr.center(), Align2::CENTER_CENTER, "⊘", FontId::proportional(13.0 * z), if filtered { SELECT } else { Color32::from_gray(0xC8) });
    let accent = app.book.theme_colors.get(4).copied().map(rgb).unwrap_or(SELECT);
    for (rect, (name, on)) in item_rects(r, items.len(), z).into_iter().zip(&items) {
        p.rect_filled(rect, 3.0, if *on { accent.gamma_multiply(0.85) } else { Color32::from_gray(0xF0) });
        p.rect_stroke(rect, 3.0, Stroke::new(1.0, if *on { accent } else { Color32::from_gray(0xD0) }), StrokeKind::Inside);
        p.with_clip_rect(rect.intersect(r)).text(rect.center(), Align2::CENTER_CENTER, name, FontId::proportional(12.0 * z), if *on { Color32::WHITE } else { Color32::from_gray(0x80) });
    }
}

/// A click on a slicer: an item button selects it alone (⌘ toggles it), the clear button shows all.
pub fn slicer_click(app: &mut SheetApp, si: usize, i: usize, r: Rect, pt: Pos2, z: f32, toggle: bool) -> bool {
    let ObjKind::Slicer(s) = app.book.sheets[si].objects[i].kind.clone() else { return false };
    let items = sheet_engine::drawing::slicer_items(&app.book, &s);
    let selected: Vec<String> = if clear_rect(r, z).contains(pt) { items.iter().map(|x| x.0.clone()).collect() } else {
        let Some(k) = item_rects(r, items.len(), z).iter().position(|x| x.contains(pt)) else { return false };
        let name = items[k].0.clone();
        let on: Vec<String> = items.iter().filter(|x| x.1).map(|x| x.0.clone()).collect();
        if toggle { if on.contains(&name) && on.len() > 1 { on.into_iter().filter(|x| *x != name).collect() } else { let mut v = on; if !v.contains(&name) { v.push(name); } v } }
        else if on.len() == 1 && on[0] == name { items.iter().map(|x| x.0.clone()).collect() } else { vec![name] }
    };
    let all = selected.len() == items.len();
    if s.pivot.is_empty() {
        let Some((ts, ti)) = app.book.find_table(&s.table) else { return false };
        app.checkpoint("Slicer");
        let crit = (!all).then(|| sheet_engine::tables::Criteria::Values { values: selected.iter().filter(|v| *v != "(blank)").cloned().collect(), blanks: selected.iter().any(|v| v == "(blank)") });
        if app.book.sheets[ts].tables[ti].filter.is_none() { app.book.sheets[ts].tables[ti].filter = Some(Default::default()); }
        app.book.set_column_filter(ts, sheet_engine::tables::Scope::Table(ti), s.field as u32, crit);
        app.changed();
    } else {
        let Some((ps, pk)) = app.book.sheets.iter().enumerate().find_map(|(x, sh)| sh.pivots.iter().position(|p| p.name == s.pivot).map(|k| (x, k))) else { return false };
        let hidden: std::collections::BTreeSet<String> = items.iter().map(|x| x.0.clone()).filter(|x| !selected.contains(x)).collect();
        edit_pivot(app, ps, pk, "Slicer", move |p| { if hidden.is_empty() { p.hidden.remove(&s.field); } else { p.hidden.insert(s.field, hidden); } });
    }
    true
}

// ---- Group, Value Field Settings and Calculated Field dialogs ------------------------------------------

#[derive(Clone)]
pub enum PivotDlg { Group(GroupDlg), Values(ValueDlg), Calc(CalcDlg) }

#[derive(Clone)]
pub struct GroupDlg { pub si: usize, pub k: usize, pub field: usize, pub date: bool, pub start: String, pub end: String, pub auto_start: bool, pub auto_end: bool, pub by: String, pub levels: [bool; 7], pub days: String, pub error: String }

#[derive(Clone)]
pub struct ValueDlg { pub si: usize, pub k: usize, pub i: usize, pub name: String, pub agg: Agg, pub show: ShowAs, pub base_field: usize, pub base_item: BaseItem, pub num_fmt: String, pub show_tab: bool }

#[derive(Clone)]
pub struct CalcDlg { pub si: usize, pub k: usize, pub name: String, pub formula: String, pub error: String }

fn dlg_id() -> egui::Id { egui::Id::new("jadwal-pivot-dialog") }
pub fn open_dialog(ctx: &egui::Context) -> Option<PivotDlg> { ctx.data(|d| d.get_temp::<PivotDlg>(dlg_id())) }
fn set_dialog(ctx: &egui::Context, d: Option<PivotDlg>) { ctx.data_mut(|m| match d { Some(d) => { m.insert_temp(dlg_id(), d); } None => { m.remove::<PivotDlg>(dlg_id()); } }); }

/// The field or value field at the active cell of pivot `k`.
pub fn hit_at(app: &SheetApp, si: usize, k: usize) -> Option<Hit> {
    let p = &app.book.sheets[si].pivots[k];
    let a = app.sel.active;
    if a.row < p.anchor.row || a.col < p.anchor.col { return None; }
    p.hit(&app.book, (a.row - p.anchor.row) as usize, (a.col - p.anchor.col) as usize)
}

fn date_text(app: &SheetApp, s: f64) -> String { sheet_engine::numfmt::format(&Value::Num(s), "m/d/yyyy", app.book.date1904).text }

/// Opens the Group dialog for a field (dates by levels, numbers by ranges).
pub fn open_group(app: &mut SheetApp, si: usize, k: usize, f: usize) {
    let p = &app.book.sheets[si].pivots[k];
    if p.calc_index(f).is_some() { return; }
    let b = p.base_of(f);
    let Some((lo, hi)) = p.field_bounds(&app.book, b) else { app.error = Some(tr("Cannot group that selection.")); return };
    // A date field: numbers shown with a date format.
    let date = app.book.sheet_index(&p.source_sheet).is_some_and(|ss| {
        (p.source.r0 + 1..=p.source.r1.min(p.source.r0 + 50)).map(|r| Pos::new(r, p.source.c0 + b as u32)).find(|q| matches!(app.book.sheets[ss].value(*q), Value::Num(_)))
            .is_some_and(|q| sheet_engine::numfmt::is_date_format(&app.book.style_at(ss, q).num_fmt))
    }) || matches!(p.groups.get(&b), Some(Grouping::Date { .. }));
    let mut d = GroupDlg { si, k, field: b, date, start: String::new(), end: String::new(), auto_start: true, auto_end: true, by: "10".into(), levels: [false; 7], days: "1".into(), error: String::new() };
    match p.groups.get(&b) {
        Some(Grouping::Date { start, end, by, auto_start, auto_end, .. }) => {
            d.start = date_text(app, *start); d.end = date_text(app, *end); d.auto_start = *auto_start; d.auto_end = *auto_end; d.days = format!("{by}");
            for g in std::iter::once(&p.groups[&b]).chain(p.extra.iter().filter(|e| e.base == b).map(|e| &e.group)) { if let Grouping::Date { level, .. } = g { d.levels[*level as usize] = true; } }
        }
        Some(Grouping::Range { start, end, by, auto_start, auto_end }) => { d.start = format!("{start}"); d.end = format!("{end}"); d.by = format!("{by}"); d.auto_start = *auto_start; d.auto_end = *auto_end; }
        _ => {
            if date { d.start = date_text(app, lo); d.end = date_text(app, hi); d.levels[DateLevel::Months as usize] = true; } else { d.start = format!("{}", lo.floor()); d.end = format!("{}", hi.ceil()); }
        }
    }
    set_dialog(&app.ctx, Some(PivotDlg::Group(d)));
}

pub fn open_value_settings(app: &mut SheetApp, si: usize, k: usize, i: usize) {
    let p = &app.book.sheets[si].pivots[k];
    let Some(v) = p.values.get(i) else { return };
    let d = ValueDlg { si, k, i, name: p.value_name(&app.book, v), agg: v.agg, show: v.show, base_field: v.base_field, base_item: v.base_item.clone(), num_fmt: if v.num_fmt.is_empty() { "General".into() } else { v.num_fmt.clone() }, show_tab: false };
    set_dialog(&app.ctx, Some(PivotDlg::Values(d)));
}

pub fn open_calc(app: &mut SheetApp, si: usize, k: usize) {
    let n = app.book.sheets[si].pivots[k].calcs.len();
    set_dialog(&app.ctx, Some(PivotDlg::Calc(CalcDlg { si, k, name: format!("Field{}", n + 1), formula: "= 0".into(), error: String::new() })));
}

/// The pivot dialogs (called every frame from the pane hook).
pub fn dialogs(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = open_dialog(ctx) else { return };
    let ok = |si: usize, k: usize, app: &SheetApp| si < app.book.sheets.len() && k < app.book.sheets[si].pivots.len();
    let keep = match &mut d {
        PivotDlg::Group(g) => ok(g.si, g.k, app) && group_dialog(app, ctx, g),
        PivotDlg::Values(v) => ok(v.si, v.k, app) && value_dialog(app, ctx, v),
        PivotDlg::Calc(c) => ok(c.si, c.k, app) && calc_dialog(app, ctx, c),
    };
    // A dialog may have opened another (or closed itself).
    if keep { set_dialog(ctx, Some(d)); } else { set_dialog(ctx, None); }
}

fn group_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut GroupDlg) -> bool {
    let mut keep = true;
    let mut open = true;
    crate::theme::dialog(ctx, "Grouping").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(300.0);
        ui.label(RichText::new(tr("Auto")).strong());
        crate::theme::hrow(ui, |ui| { ui.checkbox(&mut d.auto_start, tr("Starting at:")); ui.add_enabled(!d.auto_start, egui::TextEdit::singleline(&mut d.start).desired_width(120.0)); });
        crate::theme::hrow(ui, |ui| { ui.checkbox(&mut d.auto_end, tr("Ending at:")); ui.add_enabled(!d.auto_end, egui::TextEdit::singleline(&mut d.end).desired_width(120.0)); });
        ui.add_space(6.0);
        ui.label(RichText::new(tr("By")).strong());
        if d.date {
            egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| {
                ui.set_width(200.0);
                for l in DateLevel::ALL { let i = l as usize; if ui.selectable_label(d.levels[i], tr(l.label())).clicked() { d.levels[i] = !d.levels[i]; } }
            });
            let days_only = d.levels.iter().enumerate().all(|(i, on)| *on == (i == DateLevel::Days as usize));
            crate::theme::hrow(ui, |ui| { ui.label(tr("Number of days:")); ui.add_enabled(days_only, egui::TextEdit::singleline(&mut d.days).desired_width(60.0)); });
        } else {
            ui.add(egui::TextEdit::singleline(&mut d.by).desired_width(120.0));
        }
        if !d.error.is_empty() { ui.colored_label(Color32::from_rgb(0xC0, 0x20, 0x20), &d.error); }
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() { match apply_group(app, d) { Ok(()) => keep = false, Err(e) => d.error = tr(&e) } }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

fn apply_group(app: &mut SheetApp, d: &GroupDlg) -> Result<(), String> {
    let d1904 = app.book.date1904;
    let (si, k) = (d.si, d.k);
    let mut p = app.book.sheets[si].pivots[k].clone();
    let (lo, hi) = p.field_bounds(&app.book, d.field).ok_or("Cannot group that selection.")?;
    if d.date {
        let date = |t: &str, auto: bool, dflt: f64| -> Result<f64, String> { if auto { return Ok(dflt); } sheet_engine::numfmt::parse_input(t.trim(), d1904).map(|x| x.0).ok_or("Enter a valid date.".into()) };
        let (start, end) = (date(&d.start, d.auto_start, lo)?, date(&d.end, d.auto_end, hi)?);
        if end < start { return Err("The ending date must be after the starting date.".into()); }
        let levels: Vec<DateLevel> = DateLevel::ALL.into_iter().filter(|l| d.levels[*l as usize]).collect();
        if levels.is_empty() { return Err("Select one or more units to group by.".into()); }
        let by = sheet_engine::numfmt::parse_number(d.days.trim()).filter(|x| *x >= 1.0).ok_or("Enter a number of days of 1 or more.")?;
        p.group_dates(&app.book, d.field, &levels, start, end, by.floor(), d.auto_start, d.auto_end);
    } else {
        let num = |t: &str, auto: bool, dflt: f64| -> Result<f64, String> { if auto { return Ok(dflt); } sheet_engine::numfmt::parse_number(t.trim()).ok_or("Enter a number.".into()) };
        let (start, end) = (num(&d.start, d.auto_start, lo)?, num(&d.end, d.auto_end, hi)?);
        if end < start { return Err("The ending value must be greater than the starting value.".into()); }
        let by = sheet_engine::numfmt::parse_number(d.by.trim()).filter(|x| *x > 0.0).ok_or("Enter a positive number to group by.")?;
        p.group_range(d.field, start, end, by, d.auto_start, d.auto_end);
    }
    app.checkpoint("Group");
    app.book.sheets[si].pivots[k] = p;
    rerun(app, si, k);
    Ok(())
}

const FORMATS: [&str; 9] = ["General", "0", "0.00", "#,##0", "#,##0.00", "0%", "0.00%", "m/d/yyyy", "$#,##0.00"];

fn value_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut ValueDlg) -> bool {
    let mut keep = true;
    let mut open = true;
    let p = app.book.sheets[d.si].pivots[d.k].clone();
    let fields = p.fields(&app.book);
    let src = p.fields(&app.book).get(p.values.get(d.i).map_or(0, |v| v.field)).cloned().unwrap_or_default();
    crate::theme::dialog(ctx, "Value Field Settings").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(340.0);
        ui.label(format!("{} {src}", tr("Source Name:")));
        crate::theme::hrow(ui, |ui| { ui.label(tr("Custom Name:")); ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(220.0)); });
        ui.horizontal(|ui| {
            if ui.selectable_label(!d.show_tab, tr("Summarize Values By")).clicked() { d.show_tab = false; }
            if ui.selectable_label(d.show_tab, tr("Show Values As")).clicked() { d.show_tab = true; }
        });
        ui.separator();
        let calc = p.values.get(d.i).is_some_and(|v| p.calc_index(v.field).is_some());
        if !d.show_tab {
            ui.label(tr("Summarize value field by"));
            egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| {
                ui.set_width(220.0);
                for a in Agg::ALL {
                    if calc && a != Agg::Sum { continue; }
                    if ui.selectable_label(d.agg == a, tr(a.label())).clicked() {
                        let old = format!("{} of {src}", d.agg.label());
                        if d.name == old { d.name = format!("{} of {src}", a.label()); }
                        d.agg = a;
                    }
                }
            });
        } else {
            ui.label(tr("Show values as"));
            egui::ComboBox::from_id_salt("pivot-show-as").selected_text(tr(d.show.label())).width(220.0).show_ui(ui, |ui| {
                for s in ShowAs::ALL { ui.selectable_value(&mut d.show, s, tr(s.label())); }
            });
            if d.show.needs_base() {
                let axis: Vec<usize> = p.rows.iter().chain(&p.cols).copied().collect();
                let choices: Vec<usize> = if axis.is_empty() { (0..p.n_source() + p.extra.len()).collect() } else { axis };
                if !choices.contains(&d.base_field) { d.base_field = choices[0]; }
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(tr("Base field:"));
                        egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| { ui.set_width(150.0); for f in &choices { if ui.selectable_label(d.base_field == *f, fields.get(*f).cloned().unwrap_or_default()).clicked() { d.base_field = *f; } } });
                    });
                    if d.show == ShowAs::Difference {
                        ui.vertical(|ui| {
                            ui.label(tr("Base item:"));
                            egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| {
                                ui.set_width(150.0);
                                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                                    if ui.selectable_label(d.base_item == BaseItem::Prev, tr("(previous)")).clicked() { d.base_item = BaseItem::Prev; }
                                    if ui.selectable_label(d.base_item == BaseItem::Next, tr("(next)")).clicked() { d.base_item = BaseItem::Next; }
                                    for it in p.items(&app.book, d.base_field) { let on = d.base_item == BaseItem::Item(it.clone()); if ui.selectable_label(on, &it).clicked() { d.base_item = BaseItem::Item(it); } }
                                });
                            });
                        });
                    }
                });
            }
        }
        ui.add_space(6.0);
        crate::theme::hrow(ui, |ui| {
            ui.label(tr("Number Format:"));
            ui.add(egui::TextEdit::singleline(&mut d.num_fmt).desired_width(140.0));
            egui::ComboBox::from_id_salt("pivot-numfmt").selected_text("…").width(40.0).show_ui(ui, |ui| { for f in FORMATS { if ui.selectable_label(d.num_fmt == f, f).clicked() { d.num_fmt = f.into(); } } });
        });
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                let (i, dd) = (d.i, d.clone());
                let default = format!("{} of {src}", if calc { "Sum" } else { dd.agg.label() });
                edit_pivot(app, d.si, d.k, "Value Field Settings", move |p| {
                    if let Some(v) = p.values.get_mut(i) {
                        v.agg = dd.agg; v.show = dd.show; v.base_field = dd.base_field; v.base_item = dd.base_item;
                        v.name = if dd.name.trim().is_empty() || dd.name == default { String::new() } else { dd.name.clone() };
                        v.num_fmt = if dd.num_fmt.trim() == "General" { String::new() } else { dd.num_fmt.trim().to_string() };
                    }
                });
                keep = false;
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

fn calc_dialog(app: &mut SheetApp, ctx: &egui::Context, d: &mut CalcDlg) -> bool {
    let mut keep = true;
    let mut open = true;
    let p = app.book.sheets[d.si].pivots[d.k].clone();
    let names = p.fields(&app.book);
    let src: Vec<String> = names.iter().take(p.n_source()).cloned().collect();
    crate::theme::dialog(ctx, "Insert Calculated Field").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(360.0);
        crate::theme::hrow(ui, |ui| {
            ui.label(tr("Name:"));
            egui::ComboBox::from_id_salt("pivot-calc-name").selected_text(d.name.clone()).width(180.0).show_ui(ui, |ui| {
                for c in &p.calcs { if ui.selectable_label(d.name == c.name, &c.name).clicked() { d.name = c.name.clone(); d.formula = format!("={}", c.formula); } }
            });
            ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(120.0));
        });
        crate::theme::hrow(ui, |ui| { ui.label(tr("Formula:")); ui.add(egui::TextEdit::singleline(&mut d.formula).desired_width(260.0)); });
        ui.label(tr("Fields:"));
        egui::Frame::NONE.stroke(Stroke::new(1.0, c_line())).inner_margin(4).show(ui, |ui| {
            ui.set_width(260.0);
            egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                for n in &src {
                    let quoted = if n.chars().all(|c| c.is_alphanumeric() || c == '_') { n.clone() } else { format!("'{}'", n.replace('\'', "''")) };
                    ui.horizontal(|ui| { if ui.small_button(format!("{} {n}", tr("Insert Field"))).clicked() { d.formula.push_str(&quoted); } });
                }
            });
        });
        if !d.error.is_empty() { ui.colored_label(Color32::from_rgb(0xC0, 0x20, 0x20), &d.error); }
        crate::theme::footer(ui, |ui| {
            let commit = |app: &mut SheetApp, d: &mut CalcDlg| -> bool {
                let name = d.name.trim().to_string();
                if name.is_empty() { d.error = tr("Enter a name for the field."); return false; }
                if let Err(e) = sheet_engine::pivot::parse_calc(&d.formula, &src) { d.error = e; return false; }
                if src.iter().chain(p.extra.iter().map(|e| &e.name)).any(|s| s.eq_ignore_ascii_case(&name)) { d.error = tr("That name is used by another field."); return false; }
                let formula = d.formula.clone();
                edit_pivot(app, d.si, d.k, "Calculated Field", move |p| {
                    let f = formula.trim();
                    let f = f.strip_prefix('=').unwrap_or(f).trim().to_string();
                    if let Some(c) = p.calcs.iter_mut().find(|c| c.name.eq_ignore_ascii_case(&name)) { c.formula = f; return; }
                    let fi = p.add_calc(&name, &f);
                    p.values.push(ValueField::new(fi, Agg::Sum));
                });
                d.error.clear();
                true
            };
            if crate::theme::primary(ui, "OK").clicked() && commit(app, d) { keep = false; }
            if crate::theme::secondary(ui, "Add").clicked() { commit(app, d); }
            if let Some(c) = p.calcs.iter().position(|c| c.name.eq_ignore_ascii_case(d.name.trim())) {
                if crate::theme::secondary(ui, "Delete").clicked() { edit_pivot(app, d.si, d.k, "Delete Calculated Field", move |p| p.remove_calc(c)); }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open
}

/// PivotTable commands in the cell context menu (called by the grid's menu).
pub fn cell_menu(app: &mut SheetApp, ui: &mut Ui) {
    let Some(k) = pivot_at(app) else { return };
    let si = app.sheet();
    let p = app.book.sheets[si].pivots[k].clone();
    ui.separator();
    if ui.button(tr("Refresh")).clicked() { app.checkpoint("Refresh"); rerun(app, si, k); ui.close(); }
    match hit_at(app, si, k) {
        Some(Hit::Field(f)) => {
            if p.calc_index(f).is_none() && ui.button(tr("Group…")).clicked() { open_group(app, si, k, f); ui.close(); }
            if (p.group_of(p.base_of(f)).is_some() || p.extra.iter().any(|e| e.base == p.base_of(f))) && ui.button(tr("Ungroup")).clicked() { edit_pivot(app, si, k, "Ungroup", move |p| p.ungroup(f)); ui.close(); }
        }
        Some(Hit::Value(i)) if i < p.values.len() => { if ui.button(tr("Value Field Settings…")).clicked() { open_value_settings(app, si, k, i); ui.close(); } }
        _ => {}
    }
}

// ---- the Design tab (PivotTable styles and layout) --------------------------------------------------------

fn style_swatch(ui: &mut Ui, theme: &[[u8; 3]], name: &str, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(34.0, 26.0), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(r, 1.0, Color32::WHITE);
    if let Some([hf, _, bf, tf, df, _, line]) = sheet_engine::pivot::style_colors(theme, name) {
        let row = r.height() / 5.0;
        for i in 0..5 {
            let rr = Rect::from_min_size(pos2(r.min.x, r.min.y + i as f32 * row), vec2(r.width(), row));
            let c = match i { 0 => hf, 4 => tf, 2 => bf.or(df), _ => df };
            if let Some(c) = c { p.rect_filled(rr, 0.0, rgb(c)); }
        }
        if let Some(l) = line { p.line_segment([pos2(r.min.x, r.min.y + row), pos2(r.max.x, r.min.y + row)], Stroke::new(1.0, rgb(l))); }
    }
    p.rect_stroke(r, 1.0, Stroke::new(if on { 2.0 } else { 1.0 }, if on { SELECT } else { Color32::from_gray(0xC0) }), StrokeKind::Inside);
    resp.on_hover_text(name)
}

pub fn design_tab(app: &mut SheetApp, ui: &mut Ui) {
    let Some(k) = pivot_at(app) else { return };
    let si = app.sheet();
    let p = app.book.sheets[si].pivots[k].clone();
    group(ui, "Layout", |ui| {
        let r = large_menu(ui, "subtotals", "Subtotals", "Show or hide subtotals");
        crate::theme::menu(&r, |ui| {
            for (s, t) in [(Subtotals::Off, "Do Not Show Subtotals"), (Subtotals::Bottom, "Show all Subtotals at Bottom of Group"), (Subtotals::Top, "Show all Subtotals at Top of Group")] {
                if ui.radio(p.subtotals == s, tr(t)).clicked() { edit_pivot(app, si, k, "Subtotals", move |p| p.subtotals = s); ui.close(); }
            }
        });
        let r = large_menu(ui, "grandTotals", "Grand Totals", "Show or hide grand totals");
        crate::theme::menu(&r, |ui| {
            for (rg, cg, t) in [(false, false, "Off for Rows and Columns"), (true, true, "On for Rows and Columns"), (true, false, "On for Rows Only"), (false, true, "On for Columns Only")] {
                if ui.radio(p.row_grand == rg && p.col_grand == cg, tr(t)).clicked() { edit_pivot(app, si, k, "Grand Totals", move |p| { p.row_grand = rg; p.col_grand = cg; }); ui.close(); }
            }
        });
        let r = large_menu(ui, "layout", "Report Layout", "Compact, Outline or Tabular form");
        crate::theme::menu(&r, |ui| {
            for (l, t) in [(Layout::Compact, "Show in Compact Form"), (Layout::Outline, "Show in Outline Form"), (Layout::Tabular, "Show in Tabular Form")] {
                if ui.radio(p.layout == l, tr(t)).clicked() { edit_pivot(app, si, k, "Report Layout", move |p| p.layout = l); ui.close(); }
            }
            ui.separator();
            if ui.radio(p.repeat_labels, tr("Repeat All Item Labels")).clicked() { edit_pivot(app, si, k, "Report Layout", |p| p.repeat_labels = true); ui.close(); }
            if ui.radio(!p.repeat_labels, tr("Do Not Repeat Item Labels")).clicked() { edit_pivot(app, si, k, "Report Layout", |p| p.repeat_labels = false); ui.close(); }
        });
        let r = large_menu(ui, "blankRows", "Blank Rows", "A blank line after each item");
        crate::theme::menu(&r, |ui| {
            if ui.radio(p.blank_rows, tr("Insert Blank Line after Each Item")).clicked() { edit_pivot(app, si, k, "Blank Rows", |p| p.blank_rows = true); ui.close(); }
            if ui.radio(!p.blank_rows, tr("Remove Blank Line after Each Item")).clicked() { edit_pivot(app, si, k, "Blank Rows", |p| p.blank_rows = false); ui.close(); }
        });
    });
    group(ui, "PivotTable Style Options", |ui| {
        crate::theme::vcol(ui, |ui| {
            let st = p.style.clone();
            let mut v = st.row_headers; if ui.checkbox(&mut v, tr("Row Headers")).clicked() { edit_pivot(app, si, k, "Style Options", move |p| p.style.row_headers = v); }
            let mut v = st.col_headers; if ui.checkbox(&mut v, tr("Column Headers")).clicked() { edit_pivot(app, si, k, "Style Options", move |p| p.style.col_headers = v); }
        });
        crate::theme::vcol(ui, |ui| {
            let st = p.style.clone();
            let mut v = st.banded_rows; if ui.checkbox(&mut v, tr("Banded Rows")).clicked() { edit_pivot(app, si, k, "Style Options", move |p| p.style.banded_rows = v); }
            let mut v = st.banded_cols; if ui.checkbox(&mut v, tr("Banded Columns")).clicked() { edit_pivot(app, si, k, "Style Options", move |p| p.style.banded_cols = v); }
        });
    });
    group(ui, "PivotTable Styles", |ui| {
        let theme = app.book.theme_colors.clone();
        let mut pick: Option<String> = None;
        ui.horizontal(|ui| {
            for name in ["PivotStyleLight16", "PivotStyleLight17", "PivotStyleMedium2", "PivotStyleMedium9", "PivotStyleDark2"] {
                if style_swatch(ui, &theme, name, p.style.name == name).clicked() { pick = Some(name.into()); }
            }
        });
        let r = large_menu(ui, "tableStyles", "More Styles", "All PivotTable styles");
        crate::theme::menu(&r, |ui| {
            if ui.selectable_label(p.style.name.is_empty(), tr("None")).clicked() { pick = Some(String::new()); ui.close(); }
            for fam in ["Light", "Medium", "Dark"] {
                menu_heading(ui, &tr(fam));
                for row in 0..4 {
                    ui.horizontal(|ui| for c in 1..=7u32 {
                        let name = format!("PivotStyle{fam}{}", row * 7 + c);
                        if style_swatch(ui, &theme, &name, p.style.name == name).clicked() { pick = Some(name); ui.close(); }
                    });
                }
            }
        });
        if let Some(n) = pick { edit_pivot(app, si, k, "PivotTable Style", move |p| p.style.name = n); }
    });
}
