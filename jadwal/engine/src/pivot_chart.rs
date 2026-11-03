//! PivotCharts: charts that show a PivotTable's report. The chart's series are rebuilt from the
//! report (its body rows as categories, each column item × value field as a series) whenever the
//! pivot changes; in the file the chart part names its pivot (c:pivotSource) so Excel treats it
//! as a PivotChart.

use crate::book::Workbook;
use crate::chart::{Chart, ChartKind, Series};
use crate::drawing::ObjKind;
use crate::pivot::{Pivot, Role};
use crate::value::Value;
use crate::xlsx::xml::text;

fn find<'a>(wb: &'a Workbook, name: &str) -> Option<(usize, &'a Pivot)> {
    wb.sheets.iter().enumerate().find_map(|(si, s)| s.pivots.iter().find(|p| p.name == name).map(|p| (si, p)))
}

/// c:pivotSource for a chart showing pivot `name`: "[file]Sheet!PivotTable1" (Excel only reads
/// the part after the file name).
pub fn source_xml(wb: &Workbook, name: &str) -> String {
    let sheet = find(wb, name).map(|(si, _)| wb.sheets[si].name.clone()).unwrap_or_else(|| "Sheet1".into());
    format!("<c:pivotSource><c:name>[Book1.xlsx]{}!{}</c:name><c:fmtId val=\"0\"/></c:pivotSource>", text(&crate::addr::quote_sheet(&sheet)), text(name))
}

/// The series a pivot's report gives: (name, categories, values).
pub fn report_series(wb: &Workbook, p: &Pivot) -> Vec<(String, Vec<String>, Vec<f64>)> {
    let rep = p.compute(wb);
    let mut rows: Vec<usize> = (0..rep.roles.len()).filter(|&r| rep.roles[r] == Role::Data).collect();
    // Without row fields the grand total row is the one category.
    let total = rows.is_empty();
    if total { rows = (0..rep.roles.len()).filter(|&r| rep.roles[r] == Role::Grand).take(1).collect(); }
    let cats: Vec<String> = rows.iter().map(|&r| if total { "Total".to_string() } else { rep.keys.get(r).map(|k| k.join(" - ")).unwrap_or_default() }).collect();
    let nv = rep.nv.max(1);
    let vnames: Vec<String> = p.values.iter().map(|v| p.value_name(wb, v)).collect();
    let mut out = vec![];
    for (k, ck) in rep.col_keys.iter().enumerate() {
        for v in 0..nv {
            let col = rep.lead + k * nv + v;
            let vals = rows.iter().map(|&r| match rep.cells.get(r).and_then(|row| row.get(col)) { Some(Value::Num(n)) => *n, _ => 0.0 }).collect();
            let mut name = ck.join(" - ");
            if nv > 1 || name.is_empty() { let vn = vnames.get(v).cloned().unwrap_or_else(|| "Total".into()); name = if name.is_empty() { vn } else { format!("{name} - {vn}") }; }
            out.push((name, cats.clone(), vals));
        }
    }
    out
}

/// A new PivotChart of pivot `name`.
pub fn new_chart(wb: &Workbook, name: &str, kind: ChartKind) -> Chart {
    let mut c = Chart { kind, title: Some(String::new()), legend: Some(crate::chart::LegendPos::Right), gridlines: !kind.is_round(), markers: kind == ChartKind::Line, pivot_source: Some(name.into()), dirty: true, ..Default::default() };
    if let Some((_, p)) = find(wb, name) { fill(&mut c, wb, p); }
    c
}

fn fill(c: &mut Chart, wb: &Workbook, p: &Pivot) -> bool {
    let data = report_series(wb, p);
    let same = c.series.len() == data.len() && c.series.iter().zip(&data).all(|(s, d)| s.name == d.0 && s.cat_cache == d.1 && s.val_cache == d.2);
    if same { return false; }
    let old = std::mem::take(&mut c.series);
    c.series = data.into_iter().enumerate().map(|(i, (name, cats, vals))| {
        let keep = old.get(i).cloned().unwrap_or_default();
        Series { name, cat_cache: cats, val_cache: vals, name_ref: None, cat_ref: None, val_ref: None, ..keep }
    }).collect();
    if c.title.as_deref() == Some("") && c.series.len() == 1 { c.title = Some(String::new()); }
    c.dirty = true;
    true
}

/// Brings every PivotChart's series up to date with its pivot; true when any changed.
pub fn sync(wb: &mut Workbook) -> bool {
    let mut todo = vec![];
    for (si, s) in wb.sheets.iter().enumerate() { for (oi, o) in s.objects.iter().enumerate() { if let ObjKind::Chart(c) = &o.kind { if let Some(pv) = &c.pivot_source { todo.push((si, oi, pv.clone())); } } } }
    let mut any = false;
    for (si, oi, pv) in todo {
        let Some((_, p)) = find(wb, &pv) else { continue };
        let p = p.clone();
        let mut c = match &wb.sheets[si].objects[oi].kind { ObjKind::Chart(c) => (**c).clone(), _ => continue };
        if fill(&mut c, wb, &p) { wb.sheets[si].objects[oi].kind = ObjKind::Chart(Box::new(c)); any = true; }
    }
    any
}

/// The fields a PivotChart shows buttons for: (label, pivot field, is a value field).
pub fn field_buttons(wb: &Workbook, name: &str) -> Vec<(String, Option<usize>)> {
    let Some((_, p)) = find(wb, name) else { return vec![] };
    let names = p.fields(wb);
    let mut v: Vec<(String, Option<usize>)> = p.filters.iter().map(|f| (names.get(*f).cloned().unwrap_or_default(), Some(*f))).collect();
    v.extend(p.values.iter().map(|x| (p.value_name(wb, x), None)));
    v.extend(p.rows.iter().chain(p.cols.iter()).map(|f| (names.get(*f).cloned().unwrap_or_default(), Some(*f))));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{add_object, Anchor, Object};
    use crate::Pos;

    #[test]
    fn follows_pivot_and_round_trips() {
        let mut wb = Workbook::new();
        for (i, (a, b)) in [("Region", "Sales"), ("East", "10"), ("West", "20"), ("East", "5")].iter().enumerate() {
            wb.set_input(0, Pos::new(i as u32, 0), a);
            wb.set_input(0, Pos::new(i as u32, 1), b);
        }
        let mut p = Pivot::new("PivotTable1", "Sheet1", crate::addr::parse_range("A1:B4").unwrap(), Pos::new(0, 4));
        p.rows.push(0);
        p.values.push(crate::pivot::ValueField { field: 1, ..Default::default() });
        p.apply(&mut wb, 0);
        wb.sheets[0].pivots.push(p);
        let c = new_chart(&wb, "PivotTable1", ChartKind::Column);
        assert_eq!(c.series.len(), 1);
        assert_eq!(c.series[0].cat_cache, vec!["East", "West"]);
        assert_eq!(c.series[0].val_cache, vec![15.0, 20.0]);
        add_object(&mut wb.sheets[0].objects, Object::new(ObjKind::Chart(Box::new(c)), Anchor::default()), "Chart");
        // The pivot changes: the chart follows.
        wb.sheets[0].pivots[0].hidden.insert(0, ["West".to_string()].into());
        assert!(sync(&mut wb));
        assert_eq!(wb.sheets[0].objects[0].chart().unwrap().series[0].val_cache, vec![15.0]);
        assert!(!sync(&mut wb));
        assert_eq!(field_buttons(&wb, "PivotTable1").len(), 2);
        let bytes = crate::xlsx::write(&wb).unwrap();
        let back = crate::xlsx::read(&bytes).unwrap();
        let ch = back.sheets[0].objects.iter().find_map(|o| o.chart()).expect("chart");
        assert_eq!(ch.pivot_source.as_deref(), Some("PivotTable1"));
        let part = back.parts.iter().find(|(k, _)| k.starts_with("xl/charts/chart")).map(|(_, v)| String::from_utf8_lossy(v).to_string()).unwrap();
        assert!(part.contains("<c:pivotSource><c:name>[Book1.xlsx]Sheet1!PivotTable1</c:name>") && part.contains("<c:pivotFmts>"));
    }
}
