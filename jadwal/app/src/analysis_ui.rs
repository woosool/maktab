//! Data ▸ Data Analysis (analysis-tools.js): the tool list, then one parameters dialog that shows
//! the fields the chosen tool takes, and the output to a range, a new sheet or a new workbook.

use crate::app::SheetApp;
use crate::data_tab::{open as open_dlg, Dlg};
use crate::data_tools::parse_ref;
use crate::i18n::tr;
use egui::vec2;
use sheet_engine::analysis::{self as an, Dist, TOOLS};
use sheet_engine::addr::Pos;

#[derive(Clone)]
pub struct Analysis {
    pub tool: Option<usize>,
    pub pick: usize,
    pub input: String,
    pub input2: String,
    pub by_rows: bool,
    pub labels: bool,
    pub alpha: f64,
    /// 0 output range, 1 new worksheet ply, 2 new workbook.
    pub output: u8,
    pub out_ref: String,
    pub confidence: Option<f64>,
    pub kth: Option<usize>,
    pub interval: usize,
    pub damping: f64,
    pub bins: String,
    pub pareto: bool,
    pub cumulative: bool,
    pub vars: usize,
    pub count: usize,
    pub dist: u8,
    pub p1: f64,
    pub p2: f64,
    pub seed: u32,
    pub periodic: bool,
    pub period: usize,
    pub diff: f64,
    pub v1: f64,
    pub v2: f64,
    pub residuals: bool,
    pub const_zero: bool,
    pub inverse: bool,
}

pub fn open(app: &mut SheetApp) {
    let input = crate::data_tools::ref_text(app, app.sheet(), app.sel.range());
    open_dlg(app, Dlg::Analysis(Analysis { tool: None, pick: 4, input, input2: String::new(), by_rows: false, labels: false, alpha: 0.05, output: 1, out_ref: String::new(), confidence: Some(0.95), kth: None, interval: 3, damping: 0.3, bins: String::new(), pareto: false, cumulative: false, vars: 1, count: 10, dist: 0, p1: 0.0, p2: 1.0, seed: 0, periodic: true, period: 2, diff: 0.0, v1: 1.0, v2: 1.0, residuals: false, const_zero: false, inverse: false }));
}

pub fn dialog(app: &mut SheetApp, ctx: &egui::Context, a: &mut Analysis) -> bool {
    let mut keep = true; let mut open_ = true;
    let Some(t) = a.tool else {
        crate::theme::dialog(ctx, "Data Analysis").open(&mut open_).resizable(false).default_width(340.0).show(ctx, |ui| {
            ui.label(tr("Analysis Tools"));
            egui::Frame::group(ui.style()).show(ui, |ui| egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                ui.set_min_width(300.0);
                for (i, n) in TOOLS.iter().enumerate() { let r = ui.selectable_label(a.pick == i, tr(n)); if r.clicked() { a.pick = i; } if r.double_clicked() { a.tool = Some(i); } }
            }));
            crate::theme::footer(ui, |ui| {
                if crate::theme::primary(ui, "OK").clicked() { a.tool = Some(a.pick); }
                if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
            });
        });
        return keep && open_;
    };
    let name = TOOLS[t];
    let two = name.starts_with("t-Test") || name.starts_with("z-Test") || name.starts_with("F-Test") || name == "Regression";
    crate::theme::dialog(ctx, name).open(&mut open_).resizable(false).default_width(440.0).show(ctx, |ui| {
        ui.set_min_width(420.0);
        crate::theme::section(ui, "Input");
        crate::theme::grid(ui, egui::Grid::new("an-in").num_columns(2).spacing(vec2(8.0, 6.0)), |ui| {
            if name != "Random Number Generation" {
                let l = if two { if name == "Regression" { "Input Y Range:" } else { "Variable 1 Range:" } } else { "Input Range:" };
                ui.label(tr(l)); crate::refedit::field(ui, &mut a.input, "analysis_ui.rs:76", 200.0); ui.end_row();
                if two { ui.label(tr(if name == "Regression" { "Input X Range:" } else { "Variable 2 Range:" })); crate::refedit::field(ui, &mut a.input2, "analysis_ui.rs:77", 200.0); ui.end_row(); }
            }
            match name {
                "Histogram" => { ui.label(tr("Bin Range:")); crate::refedit::field(ui, &mut a.bins, "analysis_ui.rs:80", 200.0); ui.end_row(); }
                "Moving Average" => { ui.label(tr("Interval:")); ui.add(egui::DragValue::new(&mut a.interval).range(2..=1000)); ui.end_row(); }
                "Exponential Smoothing" => { ui.label(tr("Damping factor:")); ui.add(egui::DragValue::new(&mut a.damping).range(0.0..=1.0).speed(0.01)); ui.end_row(); }
                "Random Number Generation" => {
                    ui.label(tr("Number of Variables:")); ui.add(egui::DragValue::new(&mut a.vars).range(1..=1000)); ui.end_row();
                    ui.label(tr("Number of Random Numbers:")); ui.add(egui::DragValue::new(&mut a.count).range(1..=100000)); ui.end_row();
                    ui.label(tr("Distribution:"));
                    let dists = ["Uniform", "Normal", "Bernoulli", "Binomial", "Poisson"];
                    crate::theme::Combo::from_id_salt("an-dist").selected_text(tr(dists[a.dist as usize])).show_ui(ui, |ui| for (i, d) in dists.iter().enumerate() { ui.selectable_value(&mut a.dist, i as u8, tr(d)); });
                    ui.end_row();
                    let (l1, l2) = match a.dist { 0 => ("Between", Some("and")), 1 => ("Mean =", Some("Standard deviation =")), 2 => ("p Value =", None), 3 => ("p Value =", Some("Number of trials =")), _ => ("Lambda =", None) };
                    ui.label(tr(l1)); ui.add(egui::DragValue::new(&mut a.p1).speed(0.1)); ui.end_row();
                    if let Some(l2) = l2 { ui.label(tr(l2)); ui.add(egui::DragValue::new(&mut a.p2).speed(0.1)); ui.end_row(); }
                    ui.label(tr("Random Seed:")); ui.add(egui::DragValue::new(&mut a.seed)); ui.end_row();
                }
                "Sampling" => {
                    ui.label(tr("Sampling Method:"));
                    ui.horizontal(|ui| { crate::theme::radio_value(ui, &mut a.periodic, true, tr("Periodic")); crate::theme::radio_value(ui, &mut a.periodic, false, tr("Random")); });
                    ui.end_row();
                    if a.periodic { ui.label(tr("Period:")); ui.add(egui::DragValue::new(&mut a.period).range(1..=100000)); } else { ui.label(tr("Number of Samples:")); ui.add(egui::DragValue::new(&mut a.count).range(1..=100000)); }
                    ui.end_row();
                }
                n if n.starts_with("t-Test") || n.starts_with("z-Test") => {
                    ui.label(tr("Hypothesized Mean Difference:")); ui.add(egui::DragValue::new(&mut a.diff).speed(0.1)); ui.end_row();
                    if n.starts_with("z-Test") { ui.label(tr("Variable 1 Variance (known):")); ui.add(egui::DragValue::new(&mut a.v1).speed(0.1)); ui.end_row(); ui.label(tr("Variable 2 Variance (known):")); ui.add(egui::DragValue::new(&mut a.v2).speed(0.1)); ui.end_row(); }
                }
                _ => {}
            }
            if matches!(name, "Anova: Single Factor" | "Anova: Two-Factor Without Replication" | "F-Test Two-Sample for Variances") || name.starts_with("t-Test") || name.starts_with("z-Test") {
                ui.label(tr("Alpha:")); ui.add(egui::DragValue::new(&mut a.alpha).range(0.0001..=0.9999).speed(0.01)); ui.end_row();
            }
        });
        if matches!(name, "Anova: Single Factor" | "Correlation" | "Covariance" | "Descriptive Statistics" | "Rank and Percentile") {
            ui.horizontal(|ui| { ui.label(tr("Grouped By:")); crate::theme::radio_value(ui, &mut a.by_rows, false, tr("Columns")); crate::theme::radio_value(ui, &mut a.by_rows, true, tr("Rows")); });
        }
        if name != "Random Number Generation" { crate::theme::check(ui, &mut a.labels, tr(if a.by_rows { "Labels in First Column" } else { "Labels in First Row" })); }
        match name {
            "Descriptive Statistics" => {
                let mut on = a.confidence.is_some(); crate::theme::check(ui, &mut on, tr("Confidence Level for Mean:")); a.confidence = on.then_some(a.confidence.unwrap_or(0.95));
                let mut k = a.kth.is_some(); crate::theme::check(ui, &mut k, tr("Kth Largest / Smallest")); a.kth = if k { let mut v = a.kth.unwrap_or(1); ui.add(egui::DragValue::new(&mut v).range(1..=10000)); Some(v) } else { None };
            }
            "Histogram" => { crate::theme::check(ui, &mut a.pareto, tr("Pareto (sorted histogram)")); crate::theme::check(ui, &mut a.cumulative, tr("Cumulative Percentage")); }
            "Regression" => { crate::theme::check(ui, &mut a.const_zero, tr("Constant is Zero")); crate::theme::check(ui, &mut a.residuals, tr("Residuals")); }
            "Fourier Analysis" => { crate::theme::check(ui, &mut a.inverse, tr("Inverse")); }
            _ => {}
        }
        crate::theme::section(ui, "Output options");
        ui.horizontal(|ui| { crate::theme::radio_value(ui, &mut a.output, 0, tr("Output Range:")); ui.add_enabled_ui(a.output == 0, |ui| crate::refedit::field(ui, &mut a.out_ref, "analysis_ui.rs:127", 140.0)).inner; });
        crate::theme::radio_value(ui, &mut a.output, 1, tr("New Worksheet Ply"));
        crate::theme::radio_value(ui, &mut a.output, 2, tr("New Workbook"));
        crate::theme::footer(ui, |ui| {
            if crate::theme::primary(ui, "OK").clicked() {
                match run(app, a) { Ok(()) => keep = false, Err(e) => app.error = Some(tr(&e)) }
            }
            if crate::theme::secondary(ui, "Cancel").clicked() { keep = false; }
        });
    });
    keep && open_
}

fn run(app: &mut SheetApp, a: &Analysis) -> Result<(), String> {
    let name = TOOLS[a.tool.unwrap()];
    let input = || parse_ref(app, &a.input).ok_or("Input range must be a contiguous reference.".to_string());
    let groups = |by_rows: bool| -> Result<Vec<(String, Vec<f64>)>, String> { let (s, r) = input()?; let g = an::groups(&app.book, s, r, by_rows, a.labels); if g.iter().all(|x| x.1.is_empty()) { Err("Input range contains non-numeric data.".into()) } else { Ok(g) } };
    let pair = || -> Result<((String, Vec<f64>), (String, Vec<f64>)), String> {
        let (s1, r1) = input()?; let (s2, r2) = parse_ref(app, &a.input2).ok_or("Input range must be a contiguous reference.")?;
        let mut g1 = an::groups(&app.book, s1, r1, false, a.labels).into_iter().next().ok_or("Input range is empty.")?;
        let mut g2 = an::groups(&app.book, s2, r2, false, a.labels).into_iter().next().ok_or("Input range is empty.")?;
        if !a.labels { g1.0 = "Variable 1".into(); g2.0 = "Variable 2".into(); }
        if g1.1.len() < 2 || g2.1.len() < 2 { return Err("Each input range must have at least two values.".into()); }
        Ok((g1, g2))
    };
    let table = match name {
        "Descriptive Statistics" => an::descriptive(&groups(a.by_rows)?, a.confidence, a.kth),
        "Correlation" => an::correlation(&groups(a.by_rows)?),
        "Covariance" => an::covariance(&groups(a.by_rows)?),
        "Anova: Single Factor" => an::anova_single(&groups(a.by_rows)?, a.alpha),
        "Anova: Two-Factor Without Replication" => {
            let (s, r) = input()?;
            let sh = &app.book.sheets[s];
            let (r0, c0) = (r.r0 + a.labels as u32, r.c0 + a.labels as u32);
            let block: Vec<Vec<f64>> = (r0..=r.r1).map(|row| (c0..=r.c1).map(|c| sh.value(Pos::new(row, c)).to_num().unwrap_or(0.0)).collect()).collect();
            let rn: Vec<String> = (r0..=r.r1).map(|row| if a.labels { sh.value(Pos::new(row, r.c0)).display() } else { format!("Row {}", row - r0 + 1) }).collect();
            let cn: Vec<String> = (c0..=r.c1).map(|c| if a.labels { sh.value(Pos::new(r.r0, c)).display() } else { format!("Column {}", c - c0 + 1) }).collect();
            an::anova_two(&block, &rn, &cn, a.alpha)
        }
        "Exponential Smoothing" => an::exp_smoothing(&groups(false)?[0].1, a.damping),
        "Moving Average" => an::moving_average(&groups(false)?[0].1, a.interval),
        "F-Test Two-Sample for Variances" => { let (x, y) = pair()?; an::f_test(&x, &y, a.alpha) }
        "t-Test: Paired Two Sample for Means" => { let (x, y) = pair()?; an::t_test(&x, &y, 0, a.diff, a.alpha) }
        "t-Test: Two-Sample Assuming Equal Variances" => { let (x, y) = pair()?; an::t_test(&x, &y, 1, a.diff, a.alpha) }
        "t-Test: Two-Sample Assuming Unequal Variances" => { let (x, y) = pair()?; an::t_test(&x, &y, 2, a.diff, a.alpha) }
        "z-Test: Two Sample for Means" => { let (x, y) = pair()?; an::z_test(&x, &y, a.diff, a.v1, a.v2, a.alpha) }
        "Fourier Analysis" => {
            let (s, r) = input()?;
            let sh = &app.book.sheets[s];
            let vals: Vec<(f64, f64)> = r.iter().skip(a.labels as usize).map(|p| { let v = sh.value(p); match &v { sheet_engine::Value::Str(t) => parse_complex(t), v => (v.to_num().unwrap_or(0.0), 0.0) } }).collect();
            an::fourier(&vals, a.inverse)?
        }
        "Histogram" => {
            let bins = if a.bins.trim().is_empty() { None } else { let (s, r) = parse_ref(app, &a.bins).ok_or("Bin range must be a contiguous reference.")?; Some(an::groups(&app.book, s, r, false, a.labels).into_iter().flat_map(|g| g.1).collect()) };
            an::histogram(&groups(false)?.into_iter().flat_map(|g| g.1).collect::<Vec<_>>(), bins, a.pareto, a.cumulative)
        }
        "Random Number Generation" => {
            let d = match a.dist { 0 => Dist::Uniform(a.p1, a.p2), 1 => Dist::Normal(a.p1, a.p2), 2 => Dist::Bernoulli(a.p1), 3 => Dist::Binomial(a.p1, a.p2 as u32), _ => Dist::Poisson(a.p1) };
            an::random(a.vars, a.count, d, a.seed)
        }
        "Rank and Percentile" => an::rank_percentile(&groups(a.by_rows)?),
        "Regression" => {
            let (sy, ry) = input()?; let (sx, rx) = parse_ref(app, &a.input2).ok_or("Input X range is not valid.")?;
            let y: Vec<f64> = an::groups(&app.book, sy, ry, false, a.labels).into_iter().next().map(|g| g.1).unwrap_or_default();
            let xs = an::groups(&app.book, sx, rx, false, a.labels);
            let xs: Vec<(String, Vec<f64>)> = xs.into_iter().enumerate().map(|(i, (n, v))| (if a.labels { n } else { format!("X Variable {}", i + 1) }, v)).collect();
            an::regression(&y, &xs, a.const_zero, 0.95, a.residuals)?
        }
        "Sampling" => an::sampling(&groups(false)?.into_iter().flat_map(|g| g.1).collect::<Vec<_>>(), a.periodic.then_some(a.period), a.count, a.seed),
        _ => return Err("This tool isn't available.".into()),
    };
    app.checkpoint("Data Analysis");
    let (si, at) = match a.output {
        0 => { let (s, r) = parse_ref(app, &a.out_ref).ok_or("Output range is not valid.")?; (s, r.start()) }
        1 => { let n = app.book.unique_sheet_name(name.split(':').next().unwrap_or("Analysis")); (app.book.add_sheet(app.book.active + 1, Some(n)), Pos::new(0, 0)) }
        _ => {
            let mut wb = sheet_engine::Workbook::new();
            an::write(&mut wb, 0, Pos::new(0, 0), &table);
            wb.recalc();
            if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.xlsx", name.split(':').next().unwrap_or("Analysis"))).save_file() { sheet_engine::save(&wb, &p).map_err(|e| e.to_string())?; app.open_path(p); }
            return Ok(());
        }
    };
    an::write(&mut app.book, si, at, &table);
    app.book.sheets[si].col_widths.entry(at.col).or_insert(140.0);
    app.book.active = si;
    app.changed();
    Ok(())
}

fn parse_complex(t: &str) -> (f64, f64) {
    let t = t.trim().trim_end_matches(['i', 'j']);
    if let Some(k) = t.rfind(['+', '-']).filter(|k| *k > 0) { (t[..k].parse().unwrap_or(0.0), match &t[k..] { "+" => 1.0, "-" => -1.0, x => x.parse().unwrap_or(0.0) }) } else { (0.0, t.parse().unwrap_or(0.0)) }
}
