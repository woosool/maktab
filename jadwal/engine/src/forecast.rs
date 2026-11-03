//! Data ▸ Forecast Sheet (forecast-sheet.js): a new sheet with the timeline and values, then the
//! forecast to an end point by exponential triple smoothing (FORECAST.ETS) with confidence bounds
//! (FORECAST.ETS.CONFINT), as worksheet formulas so the table follows its data.

use crate::addr::{col_name, Pos, Range};
use crate::book::{Input, Workbook};
use crate::value::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Periods to forecast past the last point.
    pub periods: u32,
    pub confidence: f64,
    /// Seasonality: None = detect automatically.
    pub seasonality: Option<u32>,
    pub include_stats: bool,
}

impl Default for Options { fn default() -> Self { Options { periods: 6, confidence: 0.95, seasonality: None, include_stats: false } } }

/// Builds the forecast sheet from a timeline column and a values column (each with a header in
/// its first row). Returns the new sheet and the table's range there.
pub fn create(wb: &mut Workbook, sheet: usize, timeline: Range, values: Range, o: &Options) -> Result<(usize, Range), String> {
    let n = timeline.rows().min(values.rows());
    if n < 4 { return Err("Select a timeline and values with at least three data points.".into()); }
    let s = &wb.sheets[sheet];
    let t: Vec<f64> = (1..n).map(|i| s.value(Pos::new(timeline.r0 + i, timeline.c0)).to_num().unwrap_or(f64::NAN)).collect();
    if t.iter().any(|x| !x.is_finite()) { return Err("The timeline must be numbers or dates.".into()); }
    let step = (t[t.len() - 1] - t[0]) / (t.len() - 1) as f64;
    if step <= 0.0 { return Err("The timeline must increase.".into()); }
    let heads = [s.value(timeline.start()).display(), s.value(values.start()).display()];
    let fmt = wb.style_at(sheet, Pos::new(timeline.r0 + 1, timeline.c0)).num_fmt.clone();
    let vals: Vec<Value> = (1..n).map(|i| s.value(Pos::new(values.r0 + i, values.c0))).collect();
    let at = wb.add_sheet(sheet, Some(wb.unique_sheet_name(&format!("Forecast {}", wb.sheets[sheet].name))));
    let put = |wb: &mut Workbook, p: Pos, v: Value| { let c = wb.sheets[at].cell_mut(p); c.input = Input::Value(v.clone()); c.value = v; };
    let vname = if heads[1].is_empty() { "Values".to_string() } else { heads[1].clone() };
    for (j, h) in [if heads[0].is_empty() { "Timeline".to_string() } else { heads[0].clone() }, vname.clone(), format!("Forecast({vname})"), format!("Lower Confidence Bound({vname})"), format!("Upper Confidence Bound({vname})")].into_iter().enumerate() {
        put(wb, Pos::new(0, j as u32), Value::str(h));
    }
    let last = n - 1; // last data row (1-based rows on the new sheet: 1..=last)
    for (i, (x, v)) in t.iter().zip(&vals).enumerate() { put(wb, Pos::new(1 + i as u32, 0), Value::Num(*x)); put(wb, Pos::new(1 + i as u32, 1), v.clone()); }
    let hist = |c: u32| format!("${0}$2:${0}${1}", col_name(c), last + 1);
    let season = o.seasonality.map_or("1".to_string(), |s| s.to_string());
    let conf = crate::value::fmt_num(o.confidence);
    // The last actual point starts the forecast lines.
    let lr = last as u32;
    wb.set_input(at, Pos::new(lr, 2), &format!("=B{}", lr + 1));
    wb.set_input(at, Pos::new(lr, 3), &format!("=B{}", lr + 1));
    wb.set_input(at, Pos::new(lr, 4), &format!("=B{}", lr + 1));
    for k in 1..=o.periods {
        let r = lr + k;
        put(wb, Pos::new(r, 0), Value::Num(t[t.len() - 1] + step * k as f64));
        let f = format!("FORECAST.ETS(A{0},{1},{2},{season},1)", r + 1, hist(1), hist(0));
        let ci = format!("FORECAST.ETS.CONFINT(A{0},{1},{2},{conf},{season},1)", r + 1, hist(1), hist(0));
        wb.set_input(at, Pos::new(r, 2), &format!("={f}"));
        wb.set_input(at, Pos::new(r, 3), &format!("=C{}-{ci}", r + 1));
        wb.set_input(at, Pos::new(r, 4), &format!("=C{}+{ci}", r + 1));
    }
    let end = lr + o.periods;
    let table = Range { r0: 0, c0: 0, r1: end, c1: 4 };
    if fmt != "General" { wb.apply_style(at, &[Range { r0: 1, c0: 0, r1: end, c1: 0 }], &|st| st.num_fmt = fmt.clone()); }
    wb.apply_style(at, &[Range { r0: 1, c0: 2, r1: end, c1: 4 }], &|st| st.num_fmt = "0.00".into());
    for c in 0..5 { wb.sheets[at].col_widths.insert(c, if c < 2 { 90.0 } else { 150.0 }); }
    if o.include_stats {
        let names = ["Alpha", "Beta", "Gamma", "MASE", "SMAPE", "MAE", "RMSE"];
        put(wb, Pos::new(0, 7), Value::str("Statistic")); put(wb, Pos::new(0, 8), Value::str("Value"));
        for (i, nm) in names.iter().enumerate() {
            put(wb, Pos::new(1 + i as u32, 7), Value::str(*nm));
            wb.set_input(at, Pos::new(1 + i as u32, 8), &format!("=FORECAST.ETS.STAT({},{},{},{season},1)", hist(1), hist(0), i + 1));
        }
    }
    let _ = wb.create_table(at, table, true, "TableStyleMedium2");
    wb.recalc();
    Ok((at, table))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forecast_sheet_builds() {
        let mut wb = Workbook::new();
        wb.set_input(0, Pos::new(0, 0), "Month"); wb.set_input(0, Pos::new(0, 1), "Sales");
        for i in 0..12 { wb.set_input(0, Pos::new(1 + i, 0), &(i + 1).to_string()); wb.set_input(0, Pos::new(1 + i, 1), &(10.0 + 2.0 * i as f64).to_string()); }
        wb.recalc();
        let (s, r) = create(&mut wb, 0, crate::addr::parse_range("A1:A13").unwrap(), crate::addr::parse_range("B1:B13").unwrap(), &Options { periods: 3, ..Default::default() }).unwrap();
        assert_eq!(r, crate::addr::parse_range("A1:E16").unwrap());
        assert_eq!(wb.display(s, Pos::new(0, 2)), "Forecast(Sales)");
        let f = wb.sheets[s].value(Pos::new(13, 2)).to_num().unwrap();
        assert!((f - 34.0).abs() < 0.5, "{f}");
        let lo = wb.sheets[s].value(Pos::new(13, 3)).to_num().unwrap();
        assert!(lo <= f);
        assert_eq!(wb.sheets[s].tables.len(), 1);
    }
}
