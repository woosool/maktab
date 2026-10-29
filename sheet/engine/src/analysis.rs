//! Data ▸ Data Analysis, the Analysis ToolPak (analysis-tools.js): each tool takes columns (or
//! rows) of numbers and returns a table of values laid out as Excel's tool lays it out, which
//! the app writes to an output range, a new sheet or a new workbook. The distributions come from
//! the engine's own worksheet functions.

use crate::addr::{Pos, Range};
use crate::book::Workbook;
use crate::value::Value;

pub const TOOLS: &[&str] = &["Anova: Single Factor", "Anova: Two-Factor Without Replication", "Correlation", "Covariance", "Descriptive Statistics", "Exponential Smoothing", "F-Test Two-Sample for Variances", "Fourier Analysis", "Histogram", "Moving Average", "Random Number Generation", "Rank and Percentile", "Regression", "Sampling", "t-Test: Paired Two Sample for Means", "t-Test: Two-Sample Assuming Equal Variances", "t-Test: Two-Sample Assuming Unequal Variances", "z-Test: Two Sample for Means"];

pub type Table = Vec<Vec<Value>>;

fn s(t: &str) -> Value { Value::str(t) }
fn n(x: f64) -> Value { if x.is_finite() { Value::Num(x) } else { Value::Err(crate::value::ErrKind::Div0) } }

/// A worksheet function's value for numeric arguments (=NAME(a, b, …)).
pub fn func(name: &str, args: &[f64]) -> f64 {
    thread_local!(static WB: Workbook = Workbook::new());
    let a: Vec<String> = args.iter().map(|x| format!("{x:e}")).collect();
    WB.with(|wb| crate::formula::eval::evaluate_in(wb, 0, Pos::new(0, 0), &format!("{name}({})", a.join(","))).to_num().unwrap_or(f64::NAN))
}

/// Groups of numbers from a range, by columns or rows, with the labels in the first row/column.
pub fn groups(wb: &Workbook, sheet: usize, r: Range, by_rows: bool, labels: bool) -> Vec<(String, Vec<f64>)> {
    let s = &wb.sheets[sheet];
    let (outer, inner) = if by_rows { (r.r0..=r.r1, r.c0..=r.c1) } else { (r.c0..=r.c1, r.r0..=r.r1) };
    outer.enumerate().map(|(k, o)| {
        let at = |i: u32| if by_rows { Pos::new(o, i) } else { Pos::new(i, o) };
        let mut it: Vec<u32> = inner.clone().collect();
        let name = if labels && !it.is_empty() { let p = at(it.remove(0)); s.value(p).display() } else { format!("{} {}", if by_rows { "Row" } else { "Column" }, k + 1) };
        (name, it.into_iter().filter_map(|i| if let Value::Num(x) = s.value(at(i)) { Some(x) } else { None }).collect())
    }).collect()
}

fn mean(x: &[f64]) -> f64 { x.iter().sum::<f64>() / x.len() as f64 }
fn var(x: &[f64]) -> f64 { let m = mean(x); x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (x.len() as f64 - 1.0) }
fn sorted(x: &[f64]) -> Vec<f64> { let mut v = x.to_vec(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v }
fn median(x: &[f64]) -> f64 { let v = sorted(x); let k = v.len(); if k == 0 { f64::NAN } else if k % 2 == 1 { v[k / 2] } else { (v[k / 2 - 1] + v[k / 2]) / 2.0 } }
fn mode(x: &[f64]) -> Option<f64> {
    let mut best: Option<(f64, usize)> = None;
    for (i, a) in x.iter().enumerate() { let c = x.iter().filter(|b| *b == a).count(); if c > 1 && best.is_none_or(|b| c > b.1) && !x[..i].contains(a) { best = Some((*a, c)); } }
    best.map(|b| b.0)
}

pub fn descriptive(g: &[(String, Vec<f64>)], confidence: Option<f64>, kth: Option<usize>) -> Table {
    let mut rows: Vec<Vec<Value>> = vec![];
    let mut head = vec![];
    for (name, _) in g { head.push(s(name)); head.push(Value::Empty); }
    rows.push(head);
    let mut labels = vec!["Mean", "Standard Error", "Median", "Mode", "Standard Deviation", "Sample Variance", "Kurtosis", "Skewness", "Range", "Minimum", "Maximum", "Sum", "Count"].into_iter().map(String::from).collect::<Vec<_>>();
    if let Some(k) = kth { labels.push(format!("Largest({k})")); labels.push(format!("Smallest({k})")); }
    if let Some(c) = confidence { labels.push(format!("Confidence Level({}%)", crate::value::fmt_num(c * 100.0))); }
    for (i, l) in labels.iter().enumerate() {
        let mut row = vec![];
        for (_, x) in g {
            let k = x.len() as f64;
            let m = mean(x); let sd = var(x).sqrt();
            let sk = if k > 2.0 { k / ((k - 1.0) * (k - 2.0)) * x.iter().map(|v| ((v - m) / sd).powi(3)).sum::<f64>() } else { f64::NAN };
            let ku = if k > 3.0 { k * (k + 1.0) / ((k - 1.0) * (k - 2.0) * (k - 3.0)) * x.iter().map(|v| ((v - m) / sd).powi(4)).sum::<f64>() - 3.0 * (k - 1.0).powi(2) / ((k - 2.0) * (k - 3.0)) } else { f64::NAN };
            let v = sorted(x);
            let val = match i {
                0 => n(m), 1 => n(sd / k.sqrt()), 2 => n(median(x)), 3 => mode(x).map(n).unwrap_or(Value::Err(crate::value::ErrKind::NA)), 4 => n(sd), 5 => n(var(x)), 6 => n(ku), 7 => n(sk),
                8 => n(v.last().unwrap_or(&f64::NAN) - v.first().unwrap_or(&f64::NAN)), 9 => n(*v.first().unwrap_or(&f64::NAN)), 10 => n(*v.last().unwrap_or(&f64::NAN)), 11 => n(x.iter().sum()), 12 => n(k),
                _ => {
                    let extra = i - 13;
                    match (kth, extra) {
                        (Some(kk), 0) => v.len().checked_sub(kk).map(|j| n(v[j])).unwrap_or(Value::Err(crate::value::ErrKind::Num)),
                        (Some(kk), 1) => v.get(kk.saturating_sub(1)).map(|x| n(*x)).unwrap_or(Value::Err(crate::value::ErrKind::Num)),
                        _ => n(func("T.INV.2T", &[1.0 - confidence.unwrap_or(0.95), k - 1.0]) * sd / k.sqrt()),
                    }
                }
            };
            row.push(s(l)); row.push(val);
        }
        rows.push(row);
    }
    rows
}

fn pair_matrix(g: &[(String, Vec<f64>)], f: impl Fn(&[f64], &[f64]) -> f64) -> Table {
    let mut rows = vec![std::iter::once(Value::Empty).chain(g.iter().map(|x| s(&x.0))).collect::<Vec<_>>()];
    for (i, (name, a)) in g.iter().enumerate() {
        let mut row = vec![s(name)];
        for (j, (_, b)) in g.iter().enumerate() { row.push(if j <= i { n(f(a, b)) } else { Value::Empty }); }
        rows.push(row);
    }
    rows
}

fn cov_p(a: &[f64], b: &[f64]) -> f64 { let k = a.len().min(b.len()); let (ma, mb) = (mean(&a[..k]), mean(&b[..k])); (0..k).map(|i| (a[i] - ma) * (b[i] - mb)).sum::<f64>() / k as f64 }
pub fn correlation(g: &[(String, Vec<f64>)]) -> Table { pair_matrix(g, |a, b| cov_p(a, b) / (cov_p(a, a) * cov_p(b, b)).sqrt()) }
pub fn covariance(g: &[(String, Vec<f64>)]) -> Table { pair_matrix(g, cov_p) }

pub fn anova_single(g: &[(String, Vec<f64>)], alpha: f64) -> Table {
    let all: Vec<f64> = g.iter().flat_map(|x| x.1.clone()).collect();
    let gm = mean(&all);
    let ssb: f64 = g.iter().map(|(_, x)| x.len() as f64 * (mean(x) - gm).powi(2)).sum();
    let ssw: f64 = g.iter().map(|(_, x)| { let m = mean(x); x.iter().map(|v| (v - m).powi(2)).sum::<f64>() }).sum();
    let (dfb, dfw) = (g.len() as f64 - 1.0, all.len() as f64 - g.len() as f64);
    let (msb, msw) = (ssb / dfb, ssw / dfw);
    let f = msb / msw;
    let mut t = vec![vec![s("Anova: Single Factor")], vec![], vec![s("SUMMARY")], vec![s("Groups"), s("Count"), s("Sum"), s("Average"), s("Variance")]];
    for (name, x) in g { t.push(vec![s(name), n(x.len() as f64), n(x.iter().sum()), n(mean(x)), n(var(x))]); }
    t.extend([vec![], vec![], vec![s("ANOVA")], vec![s("Source of Variation"), s("SS"), s("df"), s("MS"), s("F"), s("P-value"), s("F crit")]]);
    t.push(vec![s("Between Groups"), n(ssb), n(dfb), n(msb), n(f), n(func("F.DIST.RT", &[f, dfb, dfw])), n(func("F.INV.RT", &[alpha, dfb, dfw]))]);
    t.push(vec![s("Within Groups"), n(ssw), n(dfw), n(msw)]);
    t.push(vec![]);
    t.push(vec![s("Total"), n(ssb + ssw), n(dfb + dfw)]);
    t
}

/// Two-factor without replication over a block (rows × columns of numbers).
pub fn anova_two(block: &[Vec<f64>], row_names: &[String], col_names: &[String], alpha: f64) -> Table {
    let (r, c) = (block.len(), block.first().map_or(0, |x| x.len()));
    let all: Vec<f64> = block.iter().flatten().copied().collect();
    let gm = mean(&all);
    let rm: Vec<f64> = block.iter().map(|x| mean(x)).collect();
    let cm: Vec<f64> = (0..c).map(|j| block.iter().map(|x| x[j]).sum::<f64>() / r as f64).collect();
    let ssr = c as f64 * rm.iter().map(|m| (m - gm).powi(2)).sum::<f64>();
    let ssc = r as f64 * cm.iter().map(|m| (m - gm).powi(2)).sum::<f64>();
    let sst: f64 = all.iter().map(|v| (v - gm).powi(2)).sum();
    let sse = sst - ssr - ssc;
    let (dfr, dfc, dfe) = (r as f64 - 1.0, c as f64 - 1.0, (r as f64 - 1.0) * (c as f64 - 1.0));
    let mse = sse / dfe;
    let mut t = vec![vec![s("Anova: Two-Factor Without Replication")], vec![], vec![s("SUMMARY"), s("Count"), s("Sum"), s("Average"), s("Variance")]];
    for (i, x) in block.iter().enumerate() { t.push(vec![s(row_names.get(i).map_or("", |v| v.as_str())), n(c as f64), n(x.iter().sum()), n(rm[i]), n(var(x))]); }
    t.push(vec![]);
    for j in 0..c { let col: Vec<f64> = block.iter().map(|x| x[j]).collect(); t.push(vec![s(col_names.get(j).map_or("", |v| v.as_str())), n(r as f64), n(col.iter().sum()), n(cm[j]), n(var(&col))]); }
    t.extend([vec![], vec![], vec![s("ANOVA")], vec![s("Source of Variation"), s("SS"), s("df"), s("MS"), s("F"), s("P-value"), s("F crit")]]);
    for (name, ss, df) in [("Rows", ssr, dfr), ("Columns", ssc, dfc)] { let f = ss / df / mse; t.push(vec![s(name), n(ss), n(df), n(ss / df), n(f), n(func("F.DIST.RT", &[f, df, dfe])), n(func("F.INV.RT", &[alpha, df, dfe]))]); }
    t.push(vec![s("Error"), n(sse), n(dfe), n(mse)]);
    t.push(vec![]);
    t.push(vec![s("Total"), n(sst), n(dfr + dfc + dfe)]);
    t
}

pub fn exp_smoothing(x: &[f64], damping: f64) -> Table {
    let a = 1.0 - damping;
    let mut out = vec![vec![Value::Err(crate::value::ErrKind::NA)]];
    let mut f = x.first().copied().unwrap_or(0.0);
    for i in 1..x.len() { if i > 1 { f = a * x[i - 1] + (1.0 - a) * f; } out.push(vec![n(f)]); }
    out
}

pub fn moving_average(x: &[f64], interval: usize) -> Table {
    (0..x.len()).map(|i| vec![if i + 1 < interval { Value::Err(crate::value::ErrKind::NA) } else { n(mean(&x[i + 1 - interval..=i])) }]).collect()
}

pub fn f_test(a: &(String, Vec<f64>), b: &(String, Vec<f64>), alpha: f64) -> Table {
    let (va, vb) = (var(&a.1), var(&b.1));
    let (d1, d2) = (a.1.len() as f64 - 1.0, b.1.len() as f64 - 1.0);
    let f = va / vb;
    let p = if f >= 1.0 { func("F.DIST.RT", &[f, d1, d2]) } else { func("F.DIST", &[f, d1, d2, 1.0]) };
    let crit = if f >= 1.0 { func("F.INV.RT", &[alpha, d1, d2]) } else { func("F.INV", &[alpha, d1, d2]) };
    vec![vec![s("F-Test Two-Sample for Variances")], vec![], vec![Value::Empty, s(&a.0), s(&b.0)], vec![s("Mean"), n(mean(&a.1)), n(mean(&b.1))], vec![s("Variance"), n(va), n(vb)], vec![s("Observations"), n(d1 + 1.0), n(d2 + 1.0)], vec![s("df"), n(d1), n(d2)], vec![s("F"), n(f)], vec![s("P(F<=f) one-tail"), n(p)], vec![s("F Critical one-tail"), n(crit)]]
}

/// kind: 0 paired, 1 equal variances, 2 unequal variances.
pub fn t_test(a: &(String, Vec<f64>), b: &(String, Vec<f64>), kind: u8, diff: f64, alpha: f64) -> Table {
    let (x, y) = (&a.1, &b.1);
    let (na, nb) = (x.len() as f64, y.len() as f64);
    let (ma, mb, va, vb) = (mean(x), mean(y), var(x), var(y));
    let title = ["t-Test: Paired Two Sample for Means", "t-Test: Two-Sample Assuming Equal Variances", "t-Test: Two-Sample Assuming Unequal Variances"][kind as usize];
    let mut t = vec![vec![s(title)], vec![], vec![Value::Empty, s(&a.0), s(&b.0)], vec![s("Mean"), n(ma), n(mb)], vec![s("Variance"), n(va), n(vb)], vec![s("Observations"), n(na), n(nb)]];
    let (stat, df) = match kind {
        0 => {
            let k = x.len().min(y.len());
            let d: Vec<f64> = (0..k).map(|i| x[i] - y[i]).collect();
            t.push(vec![s("Pearson Correlation"), n(cov_p(x, y) / (cov_p(x, x) * cov_p(y, y)).sqrt())]);
            ((mean(&d) - diff) / (var(&d) / k as f64).sqrt(), k as f64 - 1.0)
        }
        1 => {
            let sp = ((na - 1.0) * va + (nb - 1.0) * vb) / (na + nb - 2.0);
            t.push(vec![s("Pooled Variance"), n(sp)]);
            ((ma - mb - diff) / (sp * (1.0 / na + 1.0 / nb)).sqrt(), na + nb - 2.0)
        }
        _ => {
            let se = va / na + vb / nb;
            let df = (se * se / ((va / na).powi(2) / (na - 1.0) + (vb / nb).powi(2) / (nb - 1.0))).round();
            ((ma - mb - diff) / se.sqrt(), df)
        }
    };
    t.push(vec![s("Hypothesized Mean Difference"), n(diff)]);
    t.push(vec![s("df"), n(df)]);
    t.push(vec![s("t Stat"), n(stat)]);
    let one = func("T.DIST.RT", &[stat.abs(), df]);
    t.push(vec![s("P(T<=t) one-tail"), n(one)]);
    t.push(vec![s("t Critical one-tail"), n(func("T.INV", &[1.0 - alpha, df]))]);
    t.push(vec![s("P(T<=t) two-tail"), n(2.0 * one)]);
    t.push(vec![s("t Critical two-tail"), n(func("T.INV.2T", &[alpha, df]))]);
    t
}

pub fn z_test(a: &(String, Vec<f64>), b: &(String, Vec<f64>), diff: f64, v1: f64, v2: f64, alpha: f64) -> Table {
    let (na, nb) = (a.1.len() as f64, b.1.len() as f64);
    let z = (mean(&a.1) - mean(&b.1) - diff) / (v1 / na + v2 / nb).sqrt();
    let one = 1.0 - func("NORM.S.DIST", &[z.abs(), 1.0]);
    vec![vec![s("z-Test: Two Sample for Means")], vec![], vec![Value::Empty, s(&a.0), s(&b.0)], vec![s("Mean"), n(mean(&a.1)), n(mean(&b.1))], vec![s("Known Variance"), n(v1), n(v2)], vec![s("Observations"), n(na), n(nb)], vec![s("Hypothesized Mean Difference"), n(diff)], vec![s("z"), n(z)], vec![s("P(Z<=z) one-tail"), n(one)], vec![s("z Critical one-tail"), n(func("NORM.S.INV", &[1.0 - alpha]))], vec![s("P(Z<=z) two-tail"), n(2.0 * one)], vec![s("z Critical two-tail"), n(func("NORM.S.INV", &[1.0 - alpha / 2.0]))]]
}

/// Fourier Analysis: the DFT of 2^k values, as complex-number text (IMREAL/IMAGINARY readable).
pub fn fourier(x: &[(f64, f64)], inverse: bool) -> Result<Table, String> {
    let k = x.len();
    if k == 0 || k & (k - 1) != 0 || k > 4096 { return Err("Input range must have 2^n values (up to 4096).".into()); }
    let sg = if inverse { 1.0 } else { -1.0 };
    let mut out = vec![];
    for f in 0..k {
        let (mut re, mut im) = (0.0, 0.0);
        for (t, (a, b)) in x.iter().enumerate() {
            let ang = sg * 2.0 * std::f64::consts::PI * (f * t) as f64 / k as f64;
            re += a * ang.cos() - b * ang.sin();
            im += a * ang.sin() + b * ang.cos();
        }
        if inverse { re /= k as f64; im /= k as f64; }
        let clean = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v };
        let (re, im) = (clean(re), clean(im));
        let t = if im == 0.0 { crate::value::fmt_num(re) } else if re == 0.0 { format!("{}i", crate::value::fmt_num(im)) } else { format!("{}{}{}i", crate::value::fmt_num(re), if im < 0.0 { "-" } else { "+" }, crate::value::fmt_num(im.abs())) };
        out.push(vec![if im == 0.0 { n(re) } else { s(&t) }]);
    }
    Ok(out)
}

/// Histogram: counts per bin (bins given or ten equal ones), with More; optional Pareto and
/// cumulative %.
pub fn histogram(x: &[f64], bins: Option<Vec<f64>>, pareto: bool, cumulative: bool) -> Table {
    let bins = bins.filter(|b| !b.is_empty()).unwrap_or_else(|| {
        let v = sorted(x);
        let (lo, hi) = (v.first().copied().unwrap_or(0.0), v.last().copied().unwrap_or(0.0));
        let k = ((x.len() as f64).sqrt().round() as usize).clamp(1, 20);
        (0..k).map(|i| lo + (hi - lo) * i as f64 / k as f64).collect()
    });
    let mut counts = vec![0usize; bins.len() + 1];
    for v in x { let i = bins.iter().position(|b| v <= b).unwrap_or(bins.len()); counts[i] += 1; }
    let mut rows: Vec<(Value, usize)> = bins.iter().map(|b| n(*b)).chain(std::iter::once(s("More"))).zip(counts.iter().copied()).collect();
    if pareto { rows.sort_by(|a, b| b.1.cmp(&a.1)); }
    let total = x.len().max(1) as f64;
    let mut head = vec![s("Bin"), s("Frequency")];
    if cumulative { head.push(s("Cumulative %")); }
    let mut t = vec![head];
    let mut acc = 0;
    for (b, c) in rows { acc += c; let mut r = vec![b, n(c as f64)]; if cumulative { r.push(n(acc as f64 / total)); } t.push(r); }
    t
}

/// Seeded uniform numbers in [0, 1) (solver-nlp.js's generator).
pub fn seeded(seed: u32) -> impl FnMut() -> f64 {
    let mut a = if seed == 0 { 0x9e3779b9 } else { seed };
    move || {
        a = a.wrapping_add(0x6d2b79f5);
        let mut t = a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dist { Uniform(f64, f64), Normal(f64, f64), Bernoulli(f64), Binomial(f64, u32), Poisson(f64) }

pub fn random(vars: usize, count: usize, d: Dist, seed: u32) -> Table {
    let mut r = seeded(if seed == 0 { (crate::numfmt::now_serial().fract() * 4e9) as u32 } else { seed });
    (0..count).map(|_| (0..vars).map(|_| n(match d {
        Dist::Uniform(a, b) => a + (b - a) * r(),
        Dist::Normal(m, sd) => { let (u, v) = (r().max(1e-300), r()); m + sd * (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos() }
        Dist::Bernoulli(p) => (r() < p) as u8 as f64,
        Dist::Binomial(p, k) => (0..k).filter(|_| r() < p).count() as f64,
        Dist::Poisson(l) => { let lim = (-l).exp(); let mut k = 0.0; let mut prod = r(); while prod > lim { k += 1.0; prod *= r(); } k }
    })).collect()).collect()
}

/// Rank and Percentile for each group: Point, value, Rank, Percent (sorted descending).
pub fn rank_percentile(g: &[(String, Vec<f64>)]) -> Table {
    let mut t = vec![g.iter().flat_map(|(name, _)| [s("Point"), s(name), s("Rank"), s("Percent")]).collect::<Vec<_>>()];
    let cols: Vec<Vec<[Value; 4]>> = g.iter().map(|(_, x)| {
        let mut idx: Vec<usize> = (0..x.len()).collect();
        idx.sort_by(|a, b| x[*b].partial_cmp(&x[*a]).unwrap());
        let k = x.len();
        idx.iter().map(|&i| {
            let rank = x.iter().filter(|v| **v > x[i]).count() + 1;
            let below = x.iter().filter(|v| **v < x[i]).count();
            [n(i as f64 + 1.0), n(x[i]), n(rank as f64), n(if k > 1 { (below as f64 / (k - 1) as f64 * 100.0).floor() / 100.0 } else { 1.0 })]
        }).collect()
    }).collect();
    let rows = cols.iter().map(|c| c.len()).max().unwrap_or(0);
    for i in 0..rows { t.push(cols.iter().flat_map(|c| c.get(i).cloned().unwrap_or([Value::Empty, Value::Empty, Value::Empty, Value::Empty])).collect()); }
    t
}

/// Sampling: every `period`th value, or `count` random values (with replacement, as Excel).
pub fn sampling(x: &[f64], periodic: Option<usize>, count: usize, seed: u32) -> Table {
    match periodic {
        Some(p) if p > 0 => x.iter().skip(p - 1).step_by(p).map(|v| vec![n(*v)]).collect(),
        _ => { let mut r = seeded(if seed == 0 { 7 } else { seed }); (0..count).map(|_| vec![n(x[((r() * x.len() as f64) as usize).min(x.len().saturating_sub(1))])]).collect() }
    }
}

/// Regression: y on one or more x columns (ordinary least squares), Excel's SUMMARY OUTPUT.
pub fn regression(y: &[f64], xs: &[(String, Vec<f64>)], const_zero: bool, confidence: f64, residuals: bool) -> Result<Table, String> {
    let k = xs.len();
    let m = y.len();
    if xs.iter().any(|x| x.1.len() != m) { return Err("Input ranges must have the same number of rows.".into()); }
    let p = k + (!const_zero) as usize;
    if m <= p { return Err("Not enough observations for the number of variables.".into()); }
    let row = |i: usize| -> Vec<f64> { let mut r = if const_zero { vec![] } else { vec![1.0] }; r.extend(xs.iter().map(|x| x.1[i])); r };
    let mut xtx = vec![vec![0.0; p]; p];
    let mut xty = vec![0.0; p];
    for i in 0..m { let r = row(i); for a in 0..p { xty[a] += r[a] * y[i]; for b in 0..p { xtx[a][b] += r[a] * r[b]; } } }
    let inv = invert(&xtx).ok_or("The regression cannot be solved (the X columns are collinear).")?;
    let beta: Vec<f64> = (0..p).map(|a| (0..p).map(|b| inv[a][b] * xty[b]).sum()).collect();
    let fit: Vec<f64> = (0..m).map(|i| row(i).iter().zip(&beta).map(|(a, b)| a * b).sum()).collect();
    let my = mean(y);
    let sse: f64 = (0..m).map(|i| (y[i] - fit[i]).powi(2)).sum();
    let sst: f64 = if const_zero { y.iter().map(|v| v * v).sum() } else { y.iter().map(|v| (v - my).powi(2)).sum() };
    let ssr = sst - sse;
    let (dfr, dfe) = (k as f64, (m - p) as f64);
    let mse = sse / dfe;
    let r2 = ssr / sst;
    let adj = 1.0 - (1.0 - r2) * ((m as f64 - (!const_zero) as u8 as f64) / dfe);
    let f = (ssr / dfr) / mse;
    let mut t = vec![vec![s("SUMMARY OUTPUT")], vec![], vec![s("Regression Statistics")], vec![s("Multiple R"), n(r2.sqrt())], vec![s("R Square"), n(r2)], vec![s("Adjusted R Square"), n(adj)], vec![s("Standard Error"), n(mse.sqrt())], vec![s("Observations"), n(m as f64)], vec![], vec![s("ANOVA")], vec![Value::Empty, s("df"), s("SS"), s("MS"), s("F"), s("Significance F")]];
    t.push(vec![s("Regression"), n(dfr), n(ssr), n(ssr / dfr), n(f), n(func("F.DIST.RT", &[f, dfr, dfe]))]);
    t.push(vec![s("Residual"), n(dfe), n(sse), n(mse)]);
    t.push(vec![s("Total"), n(dfr + dfe), n(sst)]);
    t.push(vec![]);
    let pc = crate::value::fmt_num(confidence * 100.0);
    t.push(vec![Value::Empty, s("Coefficients"), s("Standard Error"), s("t Stat"), s("P-value"), s(&format!("Lower {pc}%")), s(&format!("Upper {pc}%"))]);
    let tc = func("T.INV.2T", &[1.0 - confidence, dfe]);
    let names: Vec<String> = (if const_zero { vec![] } else { vec!["Intercept".to_string()] }).into_iter().chain(xs.iter().map(|x| x.0.clone())).collect();
    for a in 0..p {
        let se = (mse * inv[a][a]).sqrt();
        let ts = beta[a] / se;
        t.push(vec![s(&names[a]), n(beta[a]), n(se), n(ts), n(func("T.DIST.2T", &[ts.abs(), dfe])), n(beta[a] - tc * se), n(beta[a] + tc * se)]);
    }
    if residuals {
        t.extend([vec![], vec![], vec![s("RESIDUAL OUTPUT")], vec![], vec![s("Observation"), s("Predicted Y"), s("Residuals")]]);
        for i in 0..m { t.push(vec![n(i as f64 + 1.0), n(fit[i]), n(y[i] - fit[i])]); }
    }
    Ok(t)
}

fn invert(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let k = a.len();
    let mut m: Vec<Vec<f64>> = a.iter().enumerate().map(|(i, r)| { let mut r = r.clone(); r.extend((0..k).map(|j| (i == j) as u8 as f64)); r }).collect();
    for c in 0..k {
        let p = (c..k).max_by(|x, y| m[*x][c].abs().partial_cmp(&m[*y][c].abs()).unwrap())?;
        if m[p][c].abs() < 1e-12 { return None; }
        m.swap(c, p);
        let d = m[c][c];
        for v in m[c].iter_mut() { *v /= d; }
        for r in 0..k { if r != c { let f = m[r][c]; if f != 0.0 { for j in 0..2 * k { m[r][j] -= f * m[c][j]; } } } }
    }
    Some(m.into_iter().map(|r| r[k..].to_vec()).collect())
}

/// Writes a tool's table from `at`; returns the area.
pub fn write(wb: &mut Workbook, sheet: usize, at: Pos, t: &Table) -> Range { crate::consolidate::put(wb, sheet, at, t) }

#[cfg(test)]
mod tests {
    use super::*;
    fn num(v: &Value) -> f64 { v.to_num().unwrap() }

    #[test]
    fn toolpak_numbers() {
        let a = ("A".to_string(), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let b = ("B".to_string(), vec![2.0, 4.0, 5.0, 4.0, 5.0]);
        let d = descriptive(&[a.clone()], Some(0.95), Some(1));
        assert_eq!(num(&d[1][1]), 3.0);
        assert!((num(&d[5][1]) - 1.5811388).abs() < 1e-6);
        assert_eq!(num(&d[13][1]), 5.0);
        assert!((num(&d[16][1]) - 1.9632432).abs() < 1e-5, "{:?}", d[16]);
        let c = correlation(&[a.clone(), b.clone()]);
        assert!((num(&c[2][1]) - 0.7745967).abs() < 1e-6);
        let an = anova_single(&[a.clone(), b.clone()], 0.05);
        let between = an.iter().find(|r| r.first() == Some(&s("Between Groups"))).unwrap();
        assert!((num(&between[1]) - 2.5).abs() < 1e-9 && (num(&between[4]) - 1.25).abs() < 1e-9);
        assert!((num(&between[5]) - 0.296).abs() < 5e-3, "{between:?}");
        let r = regression(&b.1, &[a.clone()], false, 0.95, true).unwrap();
        let slope = r.iter().find(|x| x.first() == Some(&s("A"))).unwrap();
        assert!((num(&slope[1]) - 0.6).abs() < 1e-9);
        let tt = t_test(&a, &b, 0, 0.0, 0.05);
        let ts = tt.iter().find(|x| x.first() == Some(&s("t Stat"))).unwrap();
        assert!((num(&ts[1]) + 2.2360680).abs() < 1e-6);
        let h = histogram(&[1.0, 2.0, 2.0, 3.0, 9.0], Some(vec![2.0, 5.0]), false, true);
        assert_eq!(h[1][1], n(3.0)); assert_eq!(h[3][0], s("More")); assert_eq!(h[3][2], n(1.0));
        assert_eq!(moving_average(&[1.0, 2.0, 3.0, 4.0], 2)[3][0], n(3.5));
        assert_eq!(exp_smoothing(&[3.0, 5.0, 9.0], 0.5)[2][0], n(4.0));
        let f = fourier(&[(1.0, 0.0), (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)], false).unwrap();
        assert_eq!(f[2][0], n(1.0));
        assert!(fourier(&[(1.0, 0.0); 3], false).is_err());
        let rp = rank_percentile(&[a.clone()]);
        assert_eq!(rp[1], vec![n(5.0), n(5.0), n(1.0), n(1.0)]);
        assert_eq!(sampling(&a.1, Some(2), 0, 0), vec![vec![n(2.0)], vec![n(4.0)]]);
        let rn = random(2, 3, Dist::Uniform(0.0, 1.0), 42);
        assert_eq!(rn.len(), 3);
        assert_eq!(random(2, 3, Dist::Uniform(0.0, 1.0), 42), rn);
        let ft = f_test(&a, &b, 0.05);
        assert!((num(&ft[7][1]) - 1.6666667).abs() < 1e-6);
    }
}
