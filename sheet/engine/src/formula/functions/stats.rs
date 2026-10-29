//! Statistics: the distributions (NORM, T, CHISQ, F, BETA, GAMMA, BINOM, POISSON, …) with their
//! inverses and tests, regression (LINEST, TREND, SLOPE…), percentiles and ranks, moments,
//! AGGREGATE and the FORECAST.ETS family (exponential triple smoothing, as forecast-ets.js).

use super::*;
use std::collections::HashMap;
use std::f64::consts::{LN_2, PI};

pub const DEFS: &[Def] = &[
    // Distributions
    s("NORM.DIST", 4, 4, norm_dist), s("NORMDIST", 4, 4, norm_dist),
    s("NORM.S.DIST", 1, 2, |a| { let z = n!(a[0]); let cum = if a.len() < 2 { true } else { b!(a[1]) }; num(if cum { norm_cdf(z) } else { (-0.5 * z * z).exp() / (2.0 * PI).sqrt() }) }),
    s("NORMSDIST", 1, 1, |a| num(norm_cdf(n!(a[0])))),
    s("NORM.INV", 3, 3, norm_inv_f), s("NORMINV", 3, 3, norm_inv_f),
    s("NORM.S.INV", 1, 1, |a| { let p = n!(a[0]); if p <= 0.0 || p >= 1.0 { err(ErrKind::Num) } else { num(norm_inv(p)) } }),
    s("NORMSINV", 1, 1, |a| { let p = n!(a[0]); if p <= 0.0 || p >= 1.0 { err(ErrKind::Num) } else { num(norm_inv(p)) } }),
    s("LOGNORM.DIST", 4, 4, lognorm_dist), s("LOGNORMDIST", 3, 4, |a| { let mut v = a.to_vec(); if v.len() < 4 { v.push(Value::Bool(true)); } lognorm_dist(&v) }),
    s("LOGNORM.INV", 3, 3, lognorm_inv), s("LOGINV", 3, 3, lognorm_inv),
    s("EXPON.DIST", 3, 3, expon_dist), s("EXPONDIST", 3, 3, expon_dist),
    s("POISSON.DIST", 3, 3, poisson_dist), s("POISSON", 3, 3, poisson_dist),
    s("BINOM.DIST", 4, 4, binom_dist), s("BINOMDIST", 4, 4, binom_dist),
    s("BINOM.DIST.RANGE", 3, 4, binom_range),
    s("BINOM.INV", 3, 3, binom_inv), s("CRITBINOM", 3, 3, binom_inv),
    s("WEIBULL.DIST", 4, 4, weibull), s("WEIBULL", 4, 4, weibull),
    s("GAMMA.DIST", 4, 4, gamma_dist), s("GAMMADIST", 4, 4, gamma_dist),
    s("GAMMA.INV", 3, 3, gamma_inv), s("GAMMAINV", 3, 3, gamma_inv),
    s("GAMMA", 1, 1, |a| { let x = n!(a[0]); if x <= 0.0 && x.fract() == 0.0 { err(ErrKind::Num) } else { num(gamma_fn(x)) } }),
    s("GAMMALN", 1, 1, |a| { let x = n!(a[0]); if x <= 0.0 { err(ErrKind::Num) } else { num(gamma_ln(x)) } }),
    s("GAMMALN.PRECISE", 1, 1, |a| { let x = n!(a[0]); if x <= 0.0 { err(ErrKind::Num) } else { num(gamma_ln(x)) } }),
    s("CHISQ.DIST", 3, 3, chisq_dist), s("CHISQ.DIST.RT", 2, 2, chisq_rt), s("CHIDIST", 2, 2, chisq_rt),
    s("CHISQ.INV", 2, 2, chisq_inv), s("CHISQ.INV.RT", 2, 2, chisq_inv_rt), s("CHIINV", 2, 2, chisq_inv_rt),
    s("T.DIST", 3, 3, |a| { let (x, k) = (n!(a[0]), n!(a[1]).trunc()); let cum = b!(a[2]); if k < 1.0 { return err(ErrKind::Num); } num(if cum { let p = t_tail(x.abs(), k); if x >= 0.0 { 1.0 - p } else { p } } else { t_pdf(x, k) }) }),
    s("T.DIST.RT", 2, 2, |a| { let (x, k) = (n!(a[0]), n!(a[1]).trunc()); if k < 1.0 { return err(ErrKind::Num); } let p = t_tail(x.abs(), k); num(if x >= 0.0 { p } else { 1.0 - p }) }),
    s("T.DIST.2T", 2, 2, |a| { let (x, k) = (n!(a[0]), n!(a[1]).trunc()); if k < 1.0 || x < 0.0 { return err(ErrKind::Num); } num(2.0 * t_tail(x, k)) }),
    s("TDIST", 3, 3, |a| { let (x, k, t) = (n!(a[0]), n!(a[1]).trunc(), n!(a[2]).trunc()); if k < 1.0 || x < 0.0 || (t != 1.0 && t != 2.0) { return err(ErrKind::Num); } num(t * t_tail(x, k)) }),
    s("T.INV", 2, 2, |a| { let (p, k) = (n!(a[0]), n!(a[1]).trunc()); if p <= 0.0 || p >= 1.0 || k < 1.0 { return err(ErrKind::Num); } if p == 0.5 { return num(0.0); } let q = tryv!(t_quantile(p.min(1.0 - p), k)); num(if p < 0.5 { -q } else { q }) }),
    s("T.INV.2T", 2, 2, t_inv_2t), s("TINV", 2, 2, t_inv_2t),
    s("F.DIST", 4, 4, f_dist_f), s("F.DIST.RT", 3, 3, f_rt), s("FDIST", 3, 3, f_rt),
    s("F.INV", 3, 3, |a| { let (p, d1, d2) = (n!(a[0]), n!(a[1]).trunc(), n!(a[2]).trunc()); if p < 0.0 || p >= 1.0 || d1 < 1.0 || d2 < 1.0 { return err(ErrKind::Num); } if p == 0.0 { return num(0.0); } num(tryv!(f_quantile(p, d1, d2, false))) }),
    s("F.INV.RT", 3, 3, f_inv_rt), s("FINV", 3, 3, f_inv_rt),
    s("BETA.DIST", 4, 6, beta_dist), s("BETADIST", 3, 5, |a| { let mut v = a[..3].to_vec(); v.push(Value::Bool(true)); v.extend_from_slice(&a[3..]); beta_dist(&v) }),
    s("BETA.INV", 3, 5, beta_inv), s("BETAINV", 3, 5, beta_inv),
    s("HYPGEOM.DIST", 5, 5, hypgeom), s("HYPGEOMDIST", 4, 4, |a| { let mut v = a.to_vec(); v.push(Value::Bool(false)); hypgeom(&v) }),
    s("NEGBINOM.DIST", 4, 4, negbinom), s("NEGBINOMDIST", 3, 3, |a| { let mut v = a.to_vec(); v.push(Value::Bool(false)); negbinom(&v) }),
    s("CONFIDENCE.NORM", 3, 3, conf_norm), s("CONFIDENCE", 3, 3, conf_norm),
    s("CONFIDENCE.T", 3, 3, |a| { let (al, sd, n) = (n!(a[0]), n!(a[1]), n!(a[2]).trunc()); if al <= 0.0 || al >= 1.0 || sd <= 0.0 || n < 2.0 { return err(ErrKind::Num); } num(tryv!(t_quantile(al / 2.0, n - 1.0)) * sd / n.sqrt()) }),
    s("FISHER", 1, 1, |a| { let x = n!(a[0]); if x <= -1.0 || x >= 1.0 { err(ErrKind::Num) } else { num(0.5 * ((1.0 + x) / (1.0 - x)).ln()) } }),
    s("FISHERINV", 1, 1, |a| num(n!(a[0]).tanh())),
    s("STANDARDIZE", 3, 3, |a| { let (x, m, sd) = (n!(a[0]), n!(a[1]), n!(a[2])); if sd <= 0.0 { err(ErrKind::Num) } else { num((x - m) / sd) } }),
    s("GAUSS", 1, 1, |a| num(norm_cdf(n!(a[0])) - 0.5)),
    s("PHI", 1, 1, |a| { let x = n!(a[0]); num((-(x * x) / 2.0).exp() / (2.0 * PI).sqrt()) }),
    // Tests
    e("CHISQ.TEST", 2, 2, chisq_test), e("CHITEST", 2, 2, chisq_test),
    e("T.TEST", 4, 4, t_test), e("TTEST", 4, 4, t_test),
    e("F.TEST", 2, 2, f_test), e("FTEST", 2, 2, f_test),
    e("Z.TEST", 2, 3, z_test), e("ZTEST", 2, 3, z_test),
    // Descriptive
    e("AVEDEV", 1, MANY, |c, a| { let l = tryv!(numbers(c, a)); if l.is_empty() { return err(ErrKind::Num); } let m = mean(&l); num(l.iter().map(|x| (x - m).abs()).sum::<f64>() / l.len() as f64) }),
    e("DEVSQ", 1, MANY, |c, a| { let l = tryv!(numbers(c, a)); if l.is_empty() { return err(ErrKind::Num); } num(sq_dev(&l, mean(&l))) }),
    e("GEOMEAN", 1, MANY, |c, a| { let l = tryv!(numbers(c, a)); if l.is_empty() || l.iter().any(|&x| x <= 0.0) { return err(ErrKind::Num); } num((l.iter().map(|x| x.ln()).sum::<f64>() / l.len() as f64).exp()) }),
    e("HARMEAN", 1, MANY, |c, a| { let l = tryv!(numbers(c, a)); if l.is_empty() || l.iter().any(|&x| x <= 0.0) { return err(ErrKind::Num); } num(l.len() as f64 / l.iter().map(|x| 1.0 / x).sum::<f64>()) }),
    e("SKEW", 1, MANY, |c, a| skew(tryv!(numbers(c, a)), true)), e("SKEW.P", 1, MANY, |c, a| skew(tryv!(numbers(c, a)), false)),
    e("KURT", 1, MANY, kurt),
    e("STDEVA", 1, MANY, |c, a| { let v = tryv!(desc_var(&tryv!(numbers_a(c, a)), 1.0)); num(v.sqrt()) }),
    e("STDEVPA", 1, MANY, |c, a| { let v = tryv!(desc_var(&tryv!(numbers_a(c, a)), 0.0)); num(v.sqrt()) }),
    e("VARA", 1, MANY, |c, a| num(tryv!(desc_var(&tryv!(numbers_a(c, a)), 1.0)))),
    e("VARPA", 1, MANY, |c, a| num(tryv!(desc_var(&tryv!(numbers_a(c, a)), 0.0)))),
    e("MODE.MULT", 1, MANY, mode_mult),
    e("MARGINOFERROR", 2, 2, |c, a| { let l = tryv!(numbers(c, &a[..1])); let cf = tryv!(arg_num(c, a, 1, 0.0)); if l.len() < 2 || cf <= 0.0 || cf >= 1.0 { return err(ErrKind::Num); } let sd = tryv!(desc_var(&l, 1.0)).sqrt(); num(norm_inv(1.0 - (1.0 - cf) / 2.0) * sd / (l.len() as f64).sqrt()) }),
    e("PROB", 3, 4, prob),
    // Percentiles and ranks
    e("PERCENTILE", 2, 2, |c, a| with_k(c, a, |s, p| percentile(s, p))), e("PERCENTILE.INC", 2, 2, |c, a| with_k(c, a, |s, p| percentile(s, p))),
    e("PERCENTILE.EXC", 2, 2, |c, a| with_k(c, a, percentile_exc)),
    e("QUARTILE", 2, 2, quartile), e("QUARTILE.INC", 2, 2, quartile),
    e("QUARTILE.EXC", 2, 2, |c, a| with_k(c, a, |s, q| { let i = q.trunc(); if !(1.0..=3.0).contains(&i) { return err(ErrKind::Num); } percentile_exc(s, i / 4.0) })),
    e("PERCENTRANK", 2, 3, |c, a| prank(c, a, false)), e("PERCENTRANK.INC", 2, 3, |c, a| prank(c, a, false)), e("PERCENTRANK.EXC", 2, 3, |c, a| prank(c, a, true)),
    e("RANK.AVG", 2, 3, rank_avg),
    e("TRIMMEAN", 2, 2, |c, a| with_k(c, a, |s, p| { if !(0.0..1.0).contains(&p) || s.is_empty() { return err(ErrKind::Num); } let d = ((s.len() as f64 * p) / 2.0).floor() as usize; let mid = &s[d..s.len() - d]; num(mid.iter().sum::<f64>() / mid.len() as f64) })),
    e("FREQUENCY", 2, 2, frequency),
    // Pairs and regression
    e("CORREL", 2, 2, |c, a| pair(c, a, false, |s| correlation(&s, 0.0))), e("PEARSON", 2, 2, |c, a| pair(c, a, false, |s| correlation(&s, 0.0))),
    e("RSQ", 2, 2, |c, a| pair(c, a, false, |s| match correlation(&s, 1.0) { Value::Num(r) => num(r * r), v => v })),
    e("SLOPE", 2, 2, |c, a| pair(c, a, true, |s| if s.sxx == 0.0 { err(ErrKind::Div0) } else { num(s.sxy / s.sxx) })),
    e("INTERCEPT", 2, 2, |c, a| pair(c, a, true, |s| if s.sxx == 0.0 { err(ErrKind::Div0) } else { num(s.my - s.sxy / s.sxx * s.mx) })),
    e("STEYX", 2, 2, |c, a| pair(c, a, true, |s| if s.n < 3.0 || s.sxx == 0.0 { err(ErrKind::Div0) } else { num(((s.syy - s.sxy * s.sxy / s.sxx) / (s.n - 2.0)).max(0.0).sqrt()) })),
    e("COVARIANCE.P", 2, 2, |c, a| pair(c, a, false, |s| num(s.sxy / s.n))), e("COVAR", 2, 2, |c, a| pair(c, a, false, |s| num(s.sxy / s.n))),
    e("COVARIANCE.S", 2, 2, |c, a| pair(c, a, false, |s| if s.n > 1.0 { num(s.sxy / (s.n - 1.0)) } else { err(ErrKind::Div0) })),
    e("FORECAST", 3, 3, forecast), e("FORECAST.LINEAR", 3, 3, forecast),
    e("LINEST", 1, 4, |c, a| linest(c, a, false)), e("LOGEST", 1, 4, |c, a| linest(c, a, true)),
    e("TREND", 1, 4, |c, a| trend(c, a, false)), e("GROWTH", 1, 4, |c, a| trend(c, a, true)),
    e("AGGREGATE", 3, MANY, aggregate),
    e("FORECAST.ETS", 3, 6, |c, a| { let m = tryv!(ets_model(c, &a[1], &a[2], a.get(3), a.get(4), a.get(5))); ets_each(c, &a[0], |t| forecast_at(&m, t)) }),
    e("FORECAST.ETS.CONFINT", 3, 7, |c, a| { let p = tryv!(ets_opt(c, a.get(3), 0.95)); let m = tryv!(ets_model(c, &a[1], &a[2], a.get(4), a.get(5), a.get(6))); ets_each(c, &a[0], |t| confidence_at(&m, t, p)) }),
    e("FORECAST.ETS.SEASONALITY", 2, 4, |c, a| { let one = Arg::V(Value::Num(1.0)); match ets_model(c, &a[0], &a[1], Some(&one), a.get(2), a.get(3)) { Ok(m) => num(m.m as f64), Err(e) => err(e) } }),
    e("FORECAST.ETS.STAT", 3, 6, |c, a| { let t = tryv!(ets_opt(c, a.get(2), f64::NAN)); if t.fract() != 0.0 || !(1.0..=8.0).contains(&t) { return err(ErrKind::Num); } let m = tryv!(ets_model(c, &a[0], &a[1], a.get(3), a.get(4), a.get(5))); num(statistic(&m, t as u32)) }),
];

// ---------- special functions ----------

fn whole_gamma(x: f64) -> Option<f64> { if x.fract() != 0.0 || !(1.0..=171.0).contains(&x) { return None; } Some((2..x as u64).fold(1.0, |p, i| p * i as f64)) }

/// ln Γ(x) for x > 0 (Lanczos, g = 7; exact at whole numbers).
pub fn gamma_ln(x: f64) -> f64 {
    if let Some(w) = whole_gamma(x) { return w.ln(); }
    const C: [f64; 9] = [0.999_999_999_999_809_9, 676.5203681218851, -1259.1392167224028, 771.323_428_777_653_1, -176.615_029_162_140_6, 12.507343278686905, -0.13857109526572012, 9.984_369_578_019_572e-6, 1.5056327351493116e-7];
    if x < 0.5 { return (PI / (PI * x).sin().abs()).ln() - gamma_ln(1.0 - x); }
    let x = x - 1.0;
    let t = x + 7.5;
    let a = C[0] + (1..9).map(|i| C[i] / (x + i as f64)).sum::<f64>();
    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}
pub fn gamma_fn(x: f64) -> f64 {
    if let Some(w) = whole_gamma(x) { return w; }
    if x < 0.5 { return PI / ((PI * x).sin() * gamma_fn(1.0 - x)); }
    gamma_ln(x).exp()
}
fn ln_comb(n: f64, k: f64) -> f64 { gamma_ln(n + 1.0) - gamma_ln(k + 1.0) - gamma_ln(n - k + 1.0) }

/// Regularized incomplete gamma (P, Q): series below a + 1, Lentz continued fraction above.
pub fn inc_gamma(a: f64, x: f64) -> (f64, f64) {
    if x <= 0.0 { return (0.0, 1.0); }
    let front = (-x + a * x.ln() - gamma_ln(a)).exp();
    let limit = (20.0 * a.sqrt() + 100.0).clamp(500.0, 200_000.0) as usize;
    if x < a + 1.0 {
        let (mut term, mut total) = (1.0 / a, 1.0 / a);
        for k in 1..limit { term *= x / (a + k as f64); total += term; if term.abs() < total.abs() * 1e-15 { break; } }
        let p = (total * front).min(1.0);
        return (p, 1.0 - p);
    }
    let (mut b, mut c, mut d) = (x + 1.0 - a, 1e300, 1.0 / (x + 1.0 - a));
    let mut h = d;
    for i in 1..limit {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b; if d.abs() < 1e-300 { d = 1e-300; }
        c = b + an / c; if c.abs() < 1e-300 { c = 1e-300; }
        d = 1.0 / d;
        let delta = d * c; h *= delta;
        if (delta - 1.0).abs() < 1e-15 { break; }
    }
    let q = (front * h).min(1.0);
    (1.0 - q, q)
}

fn beta_cf(x: f64, a: f64, b: f64) -> f64 {
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let tiny = 1e-300;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap; if d.abs() < tiny { d = tiny; } d = 1.0 / d;
    let mut h = d;
    for m in 1..10_000 {
        let m = m as f64; let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d; if d.abs() < tiny { d = tiny; } c = 1.0 + aa / c; if c.abs() < tiny { c = tiny; } d = 1.0 / d; h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d; if d.abs() < tiny { d = tiny; } c = 1.0 + aa / c; if c.abs() < tiny { c = tiny; } d = 1.0 / d;
        let del = d * c; h *= del;
        if (del - 1.0).abs() < 1e-16 { break; }
    }
    h
}
/// Regularized incomplete beta I_x(a, b) and its complement; `xc` is 1 - x when known more exactly.
pub fn inc_beta(x: f64, a: f64, b: f64, xc: f64) -> (f64, f64) {
    if x <= 0.0 { return (0.0, 1.0); }
    if xc <= 0.0 { return (1.0, 0.0); }
    let front = (gamma_ln(a + b) - gamma_ln(a) - gamma_ln(b) + a * x.ln() + b * xc.ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) { let p = (front * beta_cf(x, a, b) / a).min(1.0); (p, 1.0 - p) }
    else { let q = (front * beta_cf(xc, b, a) / b).min(1.0); (1.0 - q, q) }
}

pub fn norm_cdf(z: f64) -> f64 { let q = inc_gamma(0.5, z * z / 2.0).1; if z < 0.0 { 0.5 * q } else { 1.0 - 0.5 * q } }

/// Inverse standard normal (Acklam, refined once by Halley's step).
pub fn norm_inv(p: f64) -> f64 {
    const A: [f64; 6] = [-3.969683028665376e1, 2.209460984245205e2, -2.759285104469687e2, 1.38357751867269e2, -3.066479806614716e1, 2.506628277459239];
    const B: [f64; 5] = [-5.447609879822406e1, 1.615858368580409e2, -1.556989798598866e2, 6.680131188771972e1, -1.328068155288572e1];
    const C: [f64; 6] = [-7.784894002430293e-3, -3.223964580411365e-1, -2.400758277161838, -2.549732539343734, 4.374664141464968, 2.938163982698783];
    const D: [f64; 4] = [7.784695709041462e-3, 3.224671290700398e-1, 2.445134137142996, 3.754408661907416];
    let tail = |q: f64| (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5]) / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    let x = if p < 0.02425 { tail((-2.0 * p.ln()).sqrt()) } else if p > 1.0 - 0.02425 { -tail((-2.0 * (1.0 - p).ln()).sqrt()) } else {
        let q = p - 0.5; let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    };
    let e = norm_cdf(x) - p;
    let u = e * (2.0 * PI).sqrt() * (x * x / 2.0).exp();
    x - u / (1.0 + x * u / 2.0)
}

/// The upper tail of Student's t at t ≥ 0.
fn t_tail(t: f64, df: f64) -> f64 { let d = df + t * t; 0.5 * inc_beta(df / d, df / 2.0, 0.5, t * t / d).0 }
fn t_pdf(t: f64, df: f64) -> f64 { (gamma_ln((df + 1.0) / 2.0) - gamma_ln(df / 2.0) - 0.5 * (df * PI).ln() - (df + 1.0) / 2.0 * (t * t / df).ln_1p()).exp() }

/// Bisection for the x in [lo, hi] where the increasing `cdf` reaches `target`.
fn invert(cdf: &dyn Fn(f64) -> f64, target: f64, mut lo: f64, mut hi: f64) -> f64 {
    for _ in 0..2000 {
        let mid = lo + (hi - lo) / 2.0;
        if mid <= lo || mid >= hi || hi - lo <= 1e-16 * hi.abs() { break; }
        if cdf(mid) < target { lo = mid; } else { hi = mid; }
    }
    lo + (hi - lo) / 2.0
}
/// Grows `hi` until the cdf reaches the target.
fn bracket(cdf: &dyn Fn(f64) -> f64, target: f64, mut hi: f64) -> Result<f64, ErrKind> {
    while cdf(hi) < target { if hi >= f64::MAX { return Err(ErrKind::Num); } hi = (hi * 2.0).min(f64::MAX); }
    Ok(hi)
}
fn t_quantile(tail: f64, df: f64) -> Result<f64, ErrKind> {
    if tail >= 0.5 { return Ok(0.0); }
    if tail <= 0.0 { return Err(ErrKind::Num); }
    if df == 1.0 { return Ok(1.0 / (PI * tail).tan()); }
    let cdf = |t: f64| -t_tail(t, df);
    let hi = bracket(&cdf, -tail, 1.0)?;
    Ok(invert(&cdf, -tail, 0.0, hi))
}
fn gamma_quantile(p: f64, shape: f64, scale: f64, right: bool) -> Result<f64, ErrKind> {
    let upper = if right { p <= 0.5 } else { p > 0.5 };
    let target = if upper { -(if right { p } else { 1.0 - p }) } else if right { 1.0 - p } else { p };
    let cdf = |x: f64| if upper { -inc_gamma(shape, x).1 } else { inc_gamma(shape, x).0 };
    let hi = bracket(&cdf, target, shape + 10.0 * shape.sqrt() + 10.0)?;
    let v = invert(&cdf, target, 0.0, hi) * scale;
    if v.is_finite() { Ok(v) } else { Err(ErrKind::Num) }
}
fn f_cdf(x: f64, d1: f64, d2: f64) -> (f64, f64) { let s = d1 * x + d2; inc_beta(d1 * x / s, d1 / 2.0, d2 / 2.0, d2 / s) }
fn f_quantile(p: f64, d1: f64, d2: f64, right: bool) -> Result<f64, ErrKind> {
    let upper = if right { p <= 0.5 } else { p > 0.5 };
    let target = if upper { -(if right { p } else { 1.0 - p }) } else if right { 1.0 - p } else { p };
    let cdf = |x: f64| { let (lp, lq) = f_cdf(x, d1, d2); if upper { -lq } else { lp } };
    let hi = bracket(&cdf, target, 1.0)?;
    Ok(invert(&cdf, target, 0.0, hi))
}

fn binom_pmf(n: f64, k: f64, p: f64) -> f64 {
    if p == 0.0 { return if k == 0.0 { 1.0 } else { 0.0 }; }
    if p == 1.0 { return if k == n { 1.0 } else { 0.0 }; }
    if n <= 1000.0 {
        let (hits, misses) = (p.powf(k), (1.0 - p).powf(n - k));
        if hits >= 1e-307 && misses >= 1e-307 { let mut m = hits; for j in 1..=k as u64 { m = m * (n - k + j as f64) / j as f64; } return m * misses; }
    }
    (ln_comb(n, k) + k * p.ln() + (n - k) * (-p).ln_1p()).exp()
}
fn binom_cdf(n: f64, k: f64, p: f64) -> (f64, f64) {
    if k < 0.0 { return (0.0, 1.0); }
    if k >= n { return (1.0, 0.0); }
    if n <= 1000.0 { let t = (0..=k as u64).map(|i| binom_pmf(n, i as f64, p)).sum::<f64>().min(1.0); return (t, 1.0 - t); }
    inc_beta(1.0 - p, n - k, k + 1.0, p)
}

// ---------- distribution functions ----------

fn norm_dist(a: &[Value]) -> Value {
    let (x, m, sd) = (n!(a[0]), n!(a[1]), n!(a[2])); let cum = b!(a[3]);
    if sd <= 0.0 { return err(ErrKind::Num); }
    let z = (x - m) / sd;
    num(if cum { norm_cdf(z) } else { (-0.5 * z * z).exp() / (sd * (2.0 * PI).sqrt()) })
}
fn norm_inv_f(a: &[Value]) -> Value { let (p, m, sd) = (n!(a[0]), n!(a[1]), n!(a[2])); if p <= 0.0 || p >= 1.0 || sd <= 0.0 { return err(ErrKind::Num); } num(m + sd * norm_inv(p)) }
fn lognorm_dist(a: &[Value]) -> Value {
    let (x, m, sd) = (n!(a[0]), n!(a[1]), n!(a[2])); let cum = b!(a[3]);
    if x <= 0.0 || sd <= 0.0 { return err(ErrKind::Num); }
    let z = (x.ln() - m) / sd;
    num(if cum { norm_cdf(z) } else { (-0.5 * z * z).exp() / (x * sd * (2.0 * PI).sqrt()) })
}
fn lognorm_inv(a: &[Value]) -> Value { let (p, m, sd) = (n!(a[0]), n!(a[1]), n!(a[2])); if p <= 0.0 || p >= 1.0 || sd <= 0.0 { return err(ErrKind::Num); } num((m + sd * norm_inv(p)).exp()) }
fn expon_dist(a: &[Value]) -> Value { let (x, l) = (n!(a[0]), n!(a[1])); let cum = b!(a[2]); if x < 0.0 || l <= 0.0 { return err(ErrKind::Num); } num(if cum { -(-l * x).exp_m1() } else { l * (-l * x).exp() }) }
fn poisson_dist(a: &[Value]) -> Value {
    let (k, l) = (n!(a[0]).floor(), n!(a[1])); let cum = b!(a[2]);
    if k < 0.0 || l < 0.0 { return err(ErrKind::Num); }
    if l == 0.0 { return num(if cum || k == 0.0 { 1.0 } else { 0.0 }); }
    num(if cum { inc_gamma(k + 1.0, l).1 } else { (-l + k * l.ln() - gamma_ln(k + 1.0)).exp() })
}
fn binom_dist(a: &[Value]) -> Value {
    let (k, n, p) = (n!(a[0]).floor(), n!(a[1]).floor(), n!(a[2])); let cum = b!(a[3]);
    if k < 0.0 || k > n || !(0.0..=1.0).contains(&p) { return err(ErrKind::Num); }
    num(if cum { binom_cdf(n, k, p).0 } else { binom_pmf(n, k, p) })
}
fn binom_range(a: &[Value]) -> Value {
    let (n, p, lo) = (n!(a[0]).floor(), n!(a[1]), n!(a[2]).floor());
    let hi = if a.len() > 3 && !a[3].is_empty() { n!(a[3]).floor() } else { lo };
    if n < 0.0 || !(0.0..=1.0).contains(&p) || lo < 0.0 || lo > n || hi < lo || hi > n { return err(ErrKind::Num); }
    if hi - lo <= 1000.0 { return num(((lo as u64..=hi as u64).map(|i| binom_pmf(n, i as f64, p)).sum::<f64>()).min(1.0)); }
    let (below, upto) = (binom_cdf(n, lo - 1.0, p), binom_cdf(n, hi, p));
    num((if lo > n * p { below.1 - upto.1 } else { upto.0 - below.0 }).clamp(0.0, 1.0))
}
fn binom_inv(a: &[Value]) -> Value {
    let (n, p, al) = (n!(a[0]).trunc(), n!(a[1]), n!(a[2]));
    if n < 0.0 || !(0.0..=1.0).contains(&p) || !(0.0..=1.0).contains(&al) { return err(ErrKind::Num); }
    let (mut lo, mut hi) = (0.0f64, n);
    while lo < hi {
        let w = hi - lo; let mid = lo + ((hi - lo) / 2.0).floor();
        if binom_cdf(n, mid, p).0 >= al - 1e-12 { hi = mid; } else { lo = mid + 1.0; }
        if hi - lo >= w { break; }
    }
    num(lo)
}
fn weibull(a: &[Value]) -> Value {
    let (x, al, be) = (n!(a[0]), n!(a[1]), n!(a[2])); let cum = b!(a[3]);
    if x < 0.0 || al <= 0.0 || be <= 0.0 { return err(ErrKind::Num); }
    let t = (x / be).powf(al);
    num(if cum { -(-t).exp_m1() } else { al / be * (x / be).powf(al - 1.0) * (-t).exp() })
}
fn gamma_dist(a: &[Value]) -> Value {
    let (x, al, be) = (n!(a[0]), n!(a[1]), n!(a[2])); let cum = b!(a[3]);
    if x < 0.0 || al <= 0.0 || be <= 0.0 { return err(ErrKind::Num); }
    if cum { return num(inc_gamma(al, x / be).0); }
    if x == 0.0 { return if al < 1.0 { err(ErrKind::Num) } else if al == 1.0 { num(1.0 / be) } else { num(0.0) }; }
    num(((al - 1.0) * x.ln() - x / be - gamma_ln(al) - al * be.ln()).exp())
}
fn gamma_inv(a: &[Value]) -> Value {
    let (p, al, be) = (n!(a[0]), n!(a[1]), n!(a[2]));
    if p < 0.0 || p >= 1.0 || al <= 0.0 || be <= 0.0 { return err(ErrKind::Num); }
    if p == 0.0 { return num(0.0); }
    num(tryv!(gamma_quantile(p, al, be, false)))
}
fn chisq_dist(a: &[Value]) -> Value {
    let (x, k) = (n!(a[0]), n!(a[1]).trunc()); let cum = b!(a[2]);
    if x < 0.0 || k < 1.0 { return err(ErrKind::Num); }
    if cum { return num(inc_gamma(k / 2.0, x / 2.0).0); }
    if x == 0.0 { return if k < 2.0 { err(ErrKind::Num) } else if k == 2.0 { num(0.5) } else { num(0.0) }; }
    num(((k / 2.0 - 1.0) * x.ln() - x / 2.0 - gamma_ln(k / 2.0) - k / 2.0 * LN_2).exp())
}
fn chisq_rt(a: &[Value]) -> Value { let (x, k) = (n!(a[0]), n!(a[1]).trunc()); if x < 0.0 || k < 1.0 { return err(ErrKind::Num); } num(inc_gamma(k / 2.0, x / 2.0).1) }
fn chisq_inv(a: &[Value]) -> Value { let (p, k) = (n!(a[0]), n!(a[1]).trunc()); if p < 0.0 || p >= 1.0 || k < 1.0 { return err(ErrKind::Num); } if p == 0.0 { return num(0.0); } num(tryv!(gamma_quantile(p, k / 2.0, 2.0, false))) }
fn chisq_inv_rt(a: &[Value]) -> Value { let (p, k) = (n!(a[0]), n!(a[1]).trunc()); if p <= 0.0 || p > 1.0 || k < 1.0 { return err(ErrKind::Num); } if p == 1.0 { return num(0.0); } num(tryv!(gamma_quantile(p, k / 2.0, 2.0, true))) }
fn t_inv_2t(a: &[Value]) -> Value { let (p, k) = (n!(a[0]), n!(a[1]).trunc()); if p <= 0.0 || p > 1.0 || k < 1.0 { return err(ErrKind::Num); } num(tryv!(t_quantile(p / 2.0, k))) }
fn f_dist_f(a: &[Value]) -> Value {
    let (x, d1, d2) = (n!(a[0]), n!(a[1]).trunc(), n!(a[2]).trunc()); let cum = b!(a[3]);
    if x < 0.0 || d1 < 1.0 || d2 < 1.0 { return err(ErrKind::Num); }
    if cum { return num(f_cdf(x, d1, d2).0); }
    if x == 0.0 { return if d1 < 2.0 { err(ErrKind::Num) } else if d1 == 2.0 { num(1.0) } else { num(0.0) }; }
    let ln_b = gamma_ln(d1 / 2.0) + gamma_ln(d2 / 2.0) - gamma_ln((d1 + d2) / 2.0);
    num((0.5 * (d1 * (d1 * x).ln() + d2 * d2.ln() - (d1 + d2) * (d1 * x + d2).ln()) - ln_b).exp() / x)
}
fn f_rt(a: &[Value]) -> Value { let (x, d1, d2) = (n!(a[0]), n!(a[1]).trunc(), n!(a[2]).trunc()); if x < 0.0 || d1 < 1.0 || d2 < 1.0 { return err(ErrKind::Num); } num(f_cdf(x, d1, d2).1) }
fn f_inv_rt(a: &[Value]) -> Value {
    let (p, d1, d2) = (n!(a[0]), n!(a[1]).trunc(), n!(a[2]).trunc());
    if p <= 0.0 || p > 1.0 || d1 < 1.0 || d2 < 1.0 { return err(ErrKind::Num); }
    if p == 1.0 { return num(0.0); }
    num(tryv!(f_quantile(p, d1, d2, true)))
}
fn bounds(a: &[Value], i: usize) -> Result<(f64, f64), ErrKind> { Ok((opt(a, i, 0.0)?, opt(a, i + 1, 1.0)?)) }
fn beta_dist(a: &[Value]) -> Value {
    let (x, al, be) = (n!(a[0]), n!(a[1]), n!(a[2])); let cum = b!(a[3]);
    let (lo, hi) = tryv!(bounds(a, 4));
    if al <= 0.0 || be <= 0.0 || x < lo || x > hi || lo == hi { return err(ErrKind::Num); }
    let t = (x - lo) / (hi - lo);
    if cum { return num(inc_beta(t, al, be, 1.0 - t).0); }
    if (x == 0.0 && al <= 1.0) || (x == 1.0 && be <= 1.0) { return err(ErrKind::Num); }
    if t == 0.0 || t == 1.0 { return num(0.0); }
    num(((al - 1.0) * t.ln() + (be - 1.0) * (-t).ln_1p() - (gamma_ln(al) + gamma_ln(be) - gamma_ln(al + be))).exp() / (hi - lo))
}
fn beta_inv(a: &[Value]) -> Value {
    let (p, al, be) = (n!(a[0]), n!(a[1]), n!(a[2]));
    let (lo, hi) = tryv!(bounds(a, 3));
    if p <= 0.0 || p >= 1.0 || al <= 0.0 || be <= 0.0 || lo >= hi { return err(ErrKind::Num); }
    let q = if p == 0.5 && al == be { 0.5 } else { invert(&|t| inc_beta(t, al, be, 1.0 - t).0, p, 0.0, 1.0) };
    num(lo + (hi - lo) * q)
}
fn hypgeom(a: &[Value]) -> Value {
    let (x, n, m, nn) = (n!(a[0]).trunc(), n!(a[1]).trunc(), n!(a[2]).trunc(), n!(a[3]).trunc()); let cum = b!(a[4]);
    if x < 0.0 || n < 0.0 || m < 0.0 || nn < 1.0 || n > nn || m > nn || x > n || x > m { return err(ErrKind::Num); }
    let lowest = (n - (nn - m)).max(0.0);
    if x < lowest { return err(ErrKind::Num); }
    let pmf = |k: f64| (ln_comb(m, k) + ln_comb(nn - m, n - k) - ln_comb(nn, n)).exp();
    if !cum { return num(pmf(x)); }
    let highest = n.min(m);
    if x >= highest { return num(1.0); }
    let mode = highest.min(lowest.max(((n + 1.0) * (m + 1.0) / (nn + 2.0)).floor()));
    let start = x.min(mode);
    let peak = pmf(start);
    let (mut total, mut term, mut k) = (peak, peak, start);
    while k > lowest && term > total * 1e-17 { term *= k * (nn - m - n + k) / ((m - k + 1.0) * (n - k + 1.0)); total += term; k -= 1.0; }
    term = peak; k = start;
    while k < x && term > total * 1e-17 { term *= (m - k) * (n - k) / ((k + 1.0) * (nn - m - n + k + 1.0)); total += term; k += 1.0; }
    num(total.min(1.0))
}
fn negbinom(a: &[Value]) -> Value {
    let (f, s, p) = (n!(a[0]).trunc(), n!(a[1]).trunc(), n!(a[2])); let cum = b!(a[3]);
    if f < 0.0 || s < 1.0 || !(0.0..=1.0).contains(&p) { return err(ErrKind::Num); }
    if p == 1.0 { return num(if cum || f == 0.0 { 1.0 } else { 0.0 }); }
    if p == 0.0 { return num(0.0); }
    num(if cum { inc_beta(p, s, f + 1.0, 1.0 - p).0 } else { (ln_comb(f + s - 1.0, f) + s * p.ln() + f * (-p).ln_1p()).exp() })
}
fn conf_norm(a: &[Value]) -> Value { let (al, sd, n) = (n!(a[0]), n!(a[1]), n!(a[2]).trunc()); if al <= 0.0 || al >= 1.0 || sd <= 0.0 || n < 1.0 { return err(ErrKind::Num); } num(norm_inv(1.0 - al / 2.0) * sd / n.sqrt()) }

// ---------- data helpers ----------

fn mean(l: &[f64]) -> f64 { l.iter().sum::<f64>() / l.len() as f64 }
fn sq_dev(l: &[f64], m: f64) -> f64 { l.iter().map(|x| (x - m) * (x - m)).sum() }
/// Variance with `corr` (1 for a sample); #DIV/0! when too few values.
fn desc_var(l: &[f64], corr: f64) -> Result<f64, ErrKind> { if (l.len() as f64) <= corr { return Err(ErrKind::Div0); } Ok(sq_dev(l, mean(l)) / (l.len() as f64 - corr)) }
fn sorted(ctx: &Ctx, a: &Arg) -> Result<Vec<f64>, ErrKind> { let mut s = numbers(ctx, std::slice::from_ref(a))?; s.sort_by(|x, y| x.total_cmp(y)); Ok(s) }
/// 15-significant-digit form, for the equality of the = operator.
fn sig15(x: f64) -> f64 { format!("{:.14e}", x).parse().unwrap_or(x) }

/// Applies `f` to the sorted numbers of the first argument and each value of the second (an array
/// of k answers one result per element).
fn with_k(ctx: &mut Ctx, a: &[Arg], f: impl Fn(&[f64], f64) -> Value) -> Value {
    let s = tryv!(sorted(ctx, &a[0]));
    lift1(ctx, &a[1], |k| f(&s, k))
}
fn lift1(ctx: &Ctx, a: &Arg, f: impl Fn(f64) -> Value) -> Value {
    match ctx.value_of(a) {
        Value::Array(m) => Value::array(Matrix::new(m.rows, m.cols, m.data.iter().map(|v| match v.to_num() { Ok(x) => f(x), Err(e) => err(e) }).collect())),
        v => f(tryv!(v.to_num())),
    }
}
pub fn percentile(s: &[f64], p: f64) -> Value {
    if s.is_empty() || !(0.0..=1.0).contains(&p) { return err(ErrKind::Num); }
    let rank = 1.0 + p * (s.len() - 1) as f64; let lo = rank.floor();
    let i = lo as usize;
    num(if rank == lo { s[i - 1] } else { s[i - 1] + (s[i] - s[i - 1]) * (rank - lo) })
}
fn percentile_exc(s: &[f64], p: f64) -> Value {
    let n = s.len();
    if n == 0 || p <= 0.0 || p >= 1.0 { return err(ErrKind::Num); }
    let rank = p * (n + 1) as f64;
    if rank < 1.0 || rank > n as f64 { return err(ErrKind::Num); }
    let lo = rank.floor(); let (x, y) = (s[lo as usize - 1], s[(lo as usize).min(n - 1)]);
    num(x + (rank - lo) * (y - x))
}
fn quartile(ctx: &mut Ctx, a: &[Arg]) -> Value { with_k(ctx, a, |s, q| { let i = q.trunc(); if q < 0.0 || i > 4.0 { return err(ErrKind::Num); } percentile(s, i / 4.0) }) }
fn prank(ctx: &mut Ctx, a: &[Arg], exc: bool) -> Value {
    let s = tryv!(sorted(ctx, &a[0]));
    let digits = tryv!(arg_num(ctx, a, 2, 3.0));
    lift1(ctx, &a[1], |v| {
        if s.is_empty() || digits.trunc() < 1.0 { return err(ErrKind::Num); }
        let n = s.len();
        if v < s[0] || v > s[n - 1] { return err(ErrKind::NA); }
        if n == 1 && !exc { return num(1.0); }
        let i = s.partition_point(|&x| x < v);
        let rank = if exc {
            if s[i] != v { (i as f64 + (v - s[i - 1]) / (s[i] - s[i - 1])) / (n + 1) as f64 } else { (i + 1) as f64 / (n + 1) as f64 }
        } else if s[i] != v { ((i - 1) as f64 + (v - s[i - 1]) / (s[i] - s[i - 1])) / (n - 1) as f64 } else { i as f64 / (n - 1) as f64 };
        let f = 10f64.powi(digits.trunc() as i32);
        num((rank * f + 1e-9).floor() / f)
    })
}
fn rank_avg(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let s = tryv!(sorted(ctx, &a[1]));
    let asc = tryv!(arg_num(ctx, a, 2, 0.0)) != 0.0;
    lift1(ctx, &a[0], |x| {
        let x = sig15(x);
        let below = s.iter().filter(|&&v| sig15(v) < x).count();
        let ties = s.iter().filter(|&&v| sig15(v) == x).count();
        if ties == 0 { return err(ErrKind::NA); }
        let above = s.len() - below - ties;
        num((if asc { below } else { above }) as f64 + (ties as f64 + 1.0) / 2.0)
    })
}
fn skew(l: Vec<f64>, sample: bool) -> Value {
    let n = l.len() as f64;
    if n < 3.0 { return err(ErrKind::Div0); }
    let m = mean(&l);
    let sd = (sq_dev(&l, m) / if sample { n - 1.0 } else { n }).sqrt();
    if sd == 0.0 { return err(ErrKind::Div0); }
    let s3: f64 = l.iter().map(|x| ((x - m) / sd).powi(3)).sum();
    num(if sample { n / ((n - 1.0) * (n - 2.0)) * s3 } else { s3 / n })
}
fn kurt(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let l = tryv!(numbers(ctx, a)); let n = l.len() as f64;
    if n < 4.0 { return err(ErrKind::Div0); }
    let m = mean(&l); let sd = (sq_dev(&l, m) / (n - 1.0)).sqrt();
    if sd == 0.0 { return err(ErrKind::Div0); }
    let s4: f64 = l.iter().map(|x| ((x - m) / sd).powi(4)).sum();
    num(n * (n + 1.0) / ((n - 1.0) * (n - 2.0) * (n - 3.0)) * s4 - 3.0 * (n - 1.0).powi(2) / ((n - 2.0) * (n - 3.0)))
}
fn mode_mult(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let l = tryv!(numbers(ctx, a));
    let mut counts: HashMap<u64, usize> = HashMap::new(); let mut order = vec![];
    for v in &l { let c = counts.entry(v.to_bits()).or_insert(0); if *c == 0 { order.push(*v); } *c += 1; }
    let best = counts.values().copied().max().unwrap_or(0);
    if best < 2 { return err(ErrKind::NA); }
    let out: Vec<Value> = order.into_iter().filter(|v| counts.get(&v.to_bits()) == Some(&best)).map(Value::Num).collect();
    Value::array(Matrix::new(out.len(), 1, out))
}
fn prob(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let x = tryv!(numbers(ctx, &a[..1])); let p = tryv!(numbers(ctx, &a[1..2]));
    if x.len() != p.len() || x.is_empty() { return err(ErrKind::NA); }
    if p.iter().any(|v| !(0.0..=1.0).contains(v)) || (p.iter().sum::<f64>() - 1.0).abs() > 1e-9 { return err(ErrKind::Num); }
    let lo = tryv!(arg_val(ctx, a, 2).to_num());
    let hi = if a.len() > 3 && !a[3].is_missing() { tryv!(arg_val(ctx, a, 3).to_num()) } else { lo };
    num(x.iter().zip(&p).filter(|(v, _)| **v >= lo && **v <= hi).map(|(_, q)| q).sum())
}
fn frequency(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let vals = tryv!(numbers(ctx, &a[..1])); let edges = tryv!(numbers(ctx, &a[1..2]));
    let mut order: Vec<usize> = (0..edges.len()).collect();
    order.sort_by(|&i, &j| edges[i].total_cmp(&edges[j]));
    let sorted: Vec<f64> = order.iter().map(|&i| edges[i]).collect();
    let mut counts = vec![0.0; sorted.len() + 1];
    for v in vals { let lo = sorted.partition_point(|&e| v > e); counts[if lo < sorted.len() { order[lo] } else { lo }] += 1.0; }
    let n = counts.len();
    Value::array(Matrix::new(n, 1, counts.into_iter().map(Value::Num).collect()))
}

/// The numeric pairs at matching positions of two same-sized arguments; an error in either is the answer.
fn pairs(ctx: &Ctx, a: &Arg, b: &Arg) -> Result<Vec<(f64, f64)>, ErrKind> {
    let (x, y) = (flat(ctx, a), flat(ctx, b));
    if x.len() != y.len() { return Err(ErrKind::NA); }
    let mut out = vec![];
    for (u, v) in x.iter().zip(&y) {
        if let Value::Err(e) = u { return Err(*e); }
        if let Value::Err(e) = v { return Err(*e); }
        if let (Value::Num(p), Value::Num(q)) = (u, v) { out.push((*p, *q)); }
    }
    Ok(out)
}
struct Pair { n: f64, mx: f64, my: f64, sxx: f64, syy: f64, sxy: f64 }
/// Summary statistics of the pairs; `swap` reads the arguments as (ys, xs).
fn pair(ctx: &mut Ctx, a: &[Arg], swap: bool, f: impl Fn(Pair) -> Value) -> Value {
    let p = tryv!(if swap { pairs(ctx, &a[1], &a[0]) } else { pairs(ctx, &a[0], &a[1]) });
    if p.is_empty() { return err(ErrKind::Div0); }
    let n = p.len() as f64;
    let mx = p.iter().map(|q| q.0).sum::<f64>() / n; let my = p.iter().map(|q| q.1).sum::<f64>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for (x, y) in &p { sxx += (x - mx) * (x - mx); syy += (y - my) * (y - my); sxy += (x - mx) * (y - my); }
    f(Pair { n, mx, my, sxx, syy, sxy })
}
fn correlation(s: &Pair, corr: f64) -> Value {
    if s.sxx == 0.0 || s.syy == 0.0 { return err(ErrKind::Div0); }
    let c = s.n - corr;
    num((s.sxy / c) / ((s.sxx / c).sqrt() * (s.syy / c).sqrt()))
}
fn forecast(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let p = tryv!(pairs(ctx, &a[2], &a[1]));
    if p.is_empty() { return err(ErrKind::Div0); }
    let n = p.len() as f64;
    let mx = p.iter().map(|q| q.0).sum::<f64>() / n; let my = p.iter().map(|q| q.1).sum::<f64>() / n;
    let (mut sxx, mut sxy) = (0.0, 0.0);
    for (x, y) in &p { sxx += (x - mx) * (x - mx); sxy += (x - mx) * (y - my); }
    if sxx == 0.0 { return err(ErrKind::Div0); }
    lift1(ctx, &a[0], |x| num(my + sxy / sxx * (x - mx)))
}

// ---------- tests ----------

fn chisq_test(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let (x, y) = (ctx.matrix_of(&a[0]), ctx.matrix_of(&a[1]));
    if x.rows != y.rows || x.cols != y.cols { return err(ErrKind::NA); }
    let mut stat = 0.0;
    for (o, e) in x.data.iter().zip(&y.data) {
        if let (Value::Num(o), Value::Num(e)) = (o, e) { if *e == 0.0 { return err(ErrKind::Div0); } stat += (o - e).powi(2) / e; }
    }
    let df = if x.rows > 1 && x.cols > 1 { (x.rows - 1) * (x.cols - 1) } else { x.rows * x.cols - 1 };
    if df < 1 { return err(ErrKind::NA); }
    num(inc_gamma(df as f64 / 2.0, stat / 2.0).1)
}
fn t_test(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let xs = tryv!(numbers(ctx, &a[..1])); let ys = tryv!(numbers(ctx, &a[1..2]));
    let tl = tryv!(arg_num(ctx, a, 2, 0.0)).trunc(); let ty = tryv!(arg_num(ctx, a, 3, 0.0)).trunc();
    if (tl != 1.0 && tl != 2.0) || !(1.0..=3.0).contains(&ty) { return err(ErrKind::Num); }
    let prob = |t: f64, df: f64| if df <= 0.0 { err(ErrKind::Div0) } else { num(tl * t_tail(t.abs(), df)) };
    if ty == 1.0 {
        return pair(ctx, a, false, |s| {
            if s.n < 2.0 { return err(ErrKind::Div0); }
            let (vx, vy, cv) = (s.sxx / (s.n - 1.0), s.syy / (s.n - 1.0), s.sxy / (s.n - 1.0));
            let se2 = ((vx - cv) + (vy - cv)) / s.n;
            if se2 <= 0.0 || !se2.is_finite() { return err(ErrKind::Div0); }
            prob((s.mx - s.my) / se2.sqrt(), s.n - 1.0)
        });
    }
    if xs.len() < 2 || ys.len() < 2 { return err(ErrKind::Div0); }
    let (nx, ny) = (xs.len() as f64, ys.len() as f64);
    let (mx, my) = (mean(&xs), mean(&ys));
    let (vx, vy) = (sq_dev(&xs, mx) / (nx - 1.0), sq_dev(&ys, my) / (ny - 1.0));
    if ty == 2.0 {
        let df = nx + ny - 2.0;
        let se2 = ((nx - 1.0) * vx + (ny - 1.0) * vy) / df * (1.0 / nx + 1.0 / ny);
        if se2 == 0.0 || !se2.is_finite() { return err(ErrKind::Div0); }
        return prob((mx - my) / se2.sqrt(), df);
    }
    let se2 = vx / nx + vy / ny;
    if se2 == 0.0 { return err(ErrKind::Div0); }
    let den = (vx / nx).powi(2) / (nx - 1.0) + (vy / ny).powi(2) / (ny - 1.0);
    if den == 0.0 || !den.is_finite() { return err(ErrKind::Div0); }
    prob((mx - my) / se2.sqrt(), se2 * se2 / den)
}
fn f_test(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let mut ls = [vec![], vec![]];
    for (i, l) in ls.iter_mut().enumerate() { for v in flat(ctx, &a[i]) { match v { Value::Err(e) => return err(e), Value::Num(n) => l.push(n), _ => {} } } }
    let [l1, l2] = ls;
    if l1.len() < 2 || l2.len() < 2 { return err(ErrKind::Div0); }
    let (v1, v2) = (sq_dev(&l1, mean(&l1)) / (l1.len() - 1) as f64, sq_dev(&l2, mean(&l2)) / (l2.len() - 1) as f64);
    if !v1.is_finite() || !v2.is_finite() { return err(ErrKind::Num); }
    if v1 == 0.0 || v2 == 0.0 { return err(ErrKind::Div0); }
    let (p, q) = if v1 >= v2 { f_cdf(v1 / v2, (l1.len() - 1) as f64, (l2.len() - 1) as f64) } else { f_cdf(v2 / v1, (l2.len() - 1) as f64, (l1.len() - 1) as f64) };
    num(2.0 * p.min(q))
}
fn z_test(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let l = tryv!(numbers(ctx, &a[..1])); let mu = tryv!(arg_val(ctx, a, 1).to_num());
    let n = l.len() as f64;
    if n == 0.0 { return err(ErrKind::NA); }
    let m = mean(&l);
    let sd = if a.len() < 3 || a[2].is_missing() { if n < 2.0 { return err(ErrKind::Div0); } (sq_dev(&l, m) / (n - 1.0)).sqrt() } else { tryv!(arg_val(ctx, a, 2).to_num()) };
    if sd == 0.0 { return err(ErrKind::Div0); }
    num(norm_cdf(-(m - mu) / (sd / n.sqrt())))
}

// ---------- regression ----------

struct Fit { coef: Vec<f64>, b: f64, k: usize, used: usize, n: usize, df: f64, constant: bool, column: bool, row: bool, rows: Vec<Vec<Value>>, inv: Vec<Vec<f64>>, mx: Vec<f64>, ssreg: f64, ssresid: f64, sstotal: f64 }

/// Gauss-Jordan inverse with partial pivoting; None when singular.
fn invert_matrix(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let mut m: Vec<Vec<f64>> = a.iter().enumerate().map(|(i, r)| { let mut r = r.clone(); r.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 })); r }).collect();
    let scale = a.iter().flatten().fold(0.0f64, |s, v| s.max(v.abs()));
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))?;
        if m[p][c].abs() <= scale * 1e-13 || m[p][c] == 0.0 { return None; }
        m.swap(c, p);
        let d = m[c][c];
        for v in m[c].iter_mut() { *v /= d; }
        for r in 0..n { if r != c { let f = m[r][c]; if f != 0.0 { for j in 0..2 * n { let t = m[c][j]; m[r][j] -= f * t; } } } }
    }
    Some(m.into_iter().map(|r| r[n..].to_vec()).collect())
}
/// The variables that are neither constant nor a combination of the earlier ones (incremental Cholesky).
fn independent_columns(a: &[Vec<f64>], kept: &[(Vec<f64>, f64)]) -> Vec<usize> {
    let k = a.len(); let mut keep: Vec<usize> = vec![]; let mut l: Vec<Vec<f64>> = vec![];
    for j in 0..k {
        let size: f64 = kept.iter().map(|(x, _)| x[j] * x[j]).sum();
        let mut w: Vec<f64> = vec![];
        for (ai, &ka) in keep.iter().enumerate() { let mut s = a[ka][j]; for b in 0..ai { s -= l[ai][b] * w[b]; } w.push(s / l[ai][ai]); }
        let rest = a[j][j] - w.iter().map(|x| x * x).sum::<f64>();
        if !(rest > size * 1e-13) || !(rest > a[j][j] * 1e-12) { continue; }
        w.push(rest.sqrt()); l.push(w); keep.push(j);
    }
    keep
}
fn regression(ctx: &Ctx, ys: &Arg, xs: Option<&Arg>, constant: bool, log: bool) -> Result<Fit, ErrKind> {
    let ym = ctx.matrix_of(ys);
    let mut y = ym.data.clone();
    if log { for v in y.iter_mut() { if let Value::Num(n) = v { if *n <= 0.0 { return Err(ErrKind::Num); } *v = Value::Num(n.ln()); } } }
    let n = y.len();
    let column = ym.cols == 1; let row = !column && ym.rows == 1;
    let rows: Vec<Vec<Value>> = match xs {
        None => (1..=n).map(|i| vec![Value::Num(i as f64)]).collect(),
        Some(x) => {
            let xm = ctx.matrix_of(x);
            if xm.data.len() == n && (xm.rows == 1 || xm.cols == 1 || !(column || row)) { xm.data.iter().map(|v| vec![v.clone()]).collect() }
            else if column && xm.rows == n { (0..n).map(|r| xm.row(r).to_vec()).collect() }
            else if row && xm.cols == n { (0..n).map(|c| (0..xm.rows).map(|r| xm.get(r, c).clone()).collect()).collect() }
            else { return Err(ErrKind::Ref); }
        }
    };
    let k = rows.first().map_or(1, |r| r.len());
    let mut kept: Vec<(Vec<f64>, f64)> = vec![];
    for i in 0..n {
        if let Value::Err(e) = y[i] { return Err(e); }
        let mut ok = matches!(y[i], Value::Num(_));
        let mut xr = vec![];
        for v in &rows[i] { match v { Value::Err(e) => return Err(*e), Value::Num(x) => xr.push(*x), _ => ok = false } }
        if ok { if let Value::Num(yv) = y[i] { kept.push((xr, yv)); } }
    }
    let m = kept.len(); let p = k + constant as usize;
    if m < 2 || m < p { return Err(ErrKind::Div0); }
    let mut mx = vec![0.0; k]; let mut my = 0.0;
    if constant { for (xr, yv) in &kept { for j in 0..k { mx[j] += xr[j]; } my += yv; } for v in mx.iter_mut() { *v /= m as f64; } my /= m as f64; }
    let mut a = vec![vec![0.0; k]; k]; let mut r = vec![0.0; k];
    for (xr, yv) in &kept { let dy = yv - my; for i in 0..k { let di = xr[i] - mx[i]; r[i] += di * dy; for j in i..k { a[i][j] += di * (xr[j] - mx[j]); } } }
    for i in 0..k { for j in 0..i { a[i][j] = a[j][i]; } }
    let (inv, coef, used) = match invert_matrix(&a) {
        Some(inv) => { let coef = if k == 1 { vec![r[0] / a[0][0]] } else { inv.iter().map(|l| l.iter().zip(&r).map(|(v, q)| v * q).sum()).collect() }; (inv, coef, k) }
        None => {
            let keep = independent_columns(&a, &kept);
            let sub = if keep.is_empty() { None } else { Some(invert_matrix(&keep.iter().map(|&i| keep.iter().map(|&j| a[i][j]).collect()).collect::<Vec<_>>()).ok_or(ErrKind::Div0)?) };
            let mut inv = vec![vec![0.0; k]; k]; let mut coef = vec![0.0; k];
            if let Some(sub) = sub { for (ai, &i) in keep.iter().enumerate() { for (bi, &j) in keep.iter().enumerate() { inv[i][j] = sub[ai][bi]; } coef[i] = keep.iter().enumerate().map(|(bi, &j)| sub[ai][bi] * r[j]).sum(); } }
            (inv, coef, keep.len())
        }
    };
    let b = if constant { my - coef.iter().zip(&mx).map(|(c, x)| c * x).sum::<f64>() } else { 0.0 };
    let (mut ssreg, mut ssresid, mut sstotal) = (0.0, 0.0, 0.0);
    for (xr, yv) in &kept { let f = b + coef.iter().zip(xr).map(|(c, x)| c * x).sum::<f64>(); ssreg += (f - my).powi(2); ssresid += (yv - f).powi(2); sstotal += (yv - my).powi(2); }
    Ok(Fit { coef, b, k, used, n: m, df: (m - used - constant as usize) as f64, constant, column, row, rows, inv, mx, ssreg, ssresid, sstotal })
}
fn const_arg(ctx: &Ctx, a: &[Arg], i: usize) -> Result<bool, ErrKind> { match a.get(i) { None | Some(Arg::Missing) => Ok(true), Some(x) => ctx.scalar_of(x).to_bool() } }
fn linest(ctx: &mut Ctx, a: &[Arg], log: bool) -> Value {
    let c = tryv!(const_arg(ctx, a, 2)); let stats = tryv!(arg_bool(ctx, a, 3, false));
    let xs = a.get(1).filter(|x| !x.is_missing());
    let f = tryv!(regression(ctx, &a[0], xs, c, log));
    let tr = |v: f64| if log { v.exp() } else { v };
    let fin = |v: f64| num(v);
    let mut first: Vec<Value> = f.coef.iter().rev().map(|&v| fin(tr(v))).collect(); first.push(fin(tr(f.b)));
    if !stats { let n = first.len(); return Value::array(Matrix::new(1, n, first)); }
    let se_y = (f.ssresid / f.df).sqrt();
    let mean_term: f64 = (0..f.k).map(|i| f.mx[i] * (0..f.k).map(|j| f.inv[i][j] * f.mx[j]).sum::<f64>()).sum();
    let mut second: Vec<Value> = (0..f.k).rev().map(|j| fin(se_y * f.inv[j][j].sqrt())).collect();
    second.push(if f.constant { fin(se_y * (1.0 / f.n as f64 + mean_term).sqrt()) } else { err(ErrKind::NA) });
    let pad = |mut v: Vec<Value>| { v.resize(f.k + 1, err(ErrKind::NA)); v };
    let rows = vec![first, second, pad(vec![fin(f.ssreg / f.sstotal), fin(se_y)]), pad(vec![fin((f.ssreg / f.used as f64) / (f.ssresid / f.df)), num(f.df)]), pad(vec![num(f.ssreg), num(f.ssresid)])];
    Value::array(Matrix::from_rows(rows))
}
fn trend(ctx: &mut Ctx, a: &[Arg], log: bool) -> Value {
    let c = tryv!(const_arg(ctx, a, 3));
    let xs = a.get(1).filter(|x| !x.is_missing());
    let f = tryv!(regression(ctx, &a[0], xs, c, log));
    let new = a.get(2).filter(|x| !x.is_missing());
    let targets: Vec<Vec<Value>> = match new {
        None => f.rows.clone(),
        Some(x) => {
            let nm = ctx.matrix_of(x);
            if f.k == 1 { nm.data.iter().map(|v| vec![v.clone()]).collect() }
            else if f.column && nm.cols == f.k { (0..nm.rows).map(|r| nm.row(r).to_vec()).collect() }
            else if f.row && nm.rows == f.k { (0..nm.cols).map(|cc| (0..nm.rows).map(|r| nm.get(r, cc).clone()).collect()).collect() }
            else { return err(ErrKind::Ref); }
        }
    };
    let out: Vec<Value> = targets.iter().map(|xr| {
        let mut v = f.b;
        for j in 0..f.k { match xr.get(j).map(|x| x.to_num()) { Some(Ok(x)) => v += f.coef[j] * x, Some(Err(e)) => return err(e), None => return err(ErrKind::Ref) } }
        num(if log { v.exp() } else { v })
    }).collect();
    let shape = ctx.matrix_of(if f.k == 1 { new.unwrap_or(&a[0]) } else { &a[0] });
    let n = out.len();
    Value::array(if shape.rows >= shape.cols { Matrix::new(n, 1, out) } else { Matrix::new(1, n, out) })
}

// ---------- AGGREGATE ----------

fn aggregate(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let f = tryv!(arg_num(ctx, a, 0, 0.0)).trunc(); let o = tryv!(arg_num(ctx, a, 1, 0.0)).trunc();
    if !(1.0..=19.0).contains(&f) || !(0.0..=7.0).contains(&o) { return err(ErrKind::Value); }
    let o = o as u32;
    let (skip_hidden, skip_errors, skip_nested) = (o & 1 == 1, o & 2 == 2, o < 4);
    const NAMES: [&str; 19] = ["AVERAGE", "COUNT", "COUNTA", "MAX", "MIN", "PRODUCT", "STDEV", "STDEV.P", "SUM", "VAR", "VAR.P", "MEDIAN", "MODE.SNGL", "LARGE", "SMALL", "PERCENTILE.INC", "QUARTILE.INC", "PERCENTILE.EXC", "QUARTILE.EXC"];
    let clean = |x: &Arg| -> Arg {
        let areas: Vec<(usize, crate::addr::Range)> = match x { Arg::R(s, r) => vec![(*s, *r)], Arg::Areas(l) => l.clone(), Arg::V(v) => return if skip_errors && v.is_err() { Arg::V(Value::Empty) } else if skip_errors { if let Value::Array(m) = v { Arg::V(Value::array(Matrix::new(m.rows, m.cols, m.data.iter().map(|v| if v.is_err() { Value::Empty } else { v.clone() }).collect()))) } else { x.clone() } } else { x.clone() }, o => return o.clone() };
        let mut vals = vec![];
        for (s, r) in areas {
            let Some(sh) = ctx.book.sheets.get(s) else { continue };
            for p in ctx.clip(s, r).iter() {
                if skip_hidden && sh.hidden_rows.contains(&p.row) { continue; }
                if skip_nested && sh.cells.get(&p).and_then(|c| c.formula()).is_some_and(|t| { let u = t.to_ascii_uppercase(); u.contains("SUBTOTAL(") || u.contains("AGGREGATE(") }) { continue; }
                let v = ctx.cell(s, p);
                vals.push(if skip_errors && v.is_err() { Value::Empty } else { v });
            }
        }
        let n = vals.len();
        Arg::V(Value::array(Matrix::new(n.max(1), 1, if n == 0 { vec![Value::Empty] } else { vals })))
    };
    let name = NAMES[f as usize - 1];
    let refs = &a[2..];
    let args: Vec<Arg> = if f >= 14.0 { if refs.len() < 2 { return err(ErrKind::Value); } vec![clean(&refs[0]), refs[1].clone()] } else { refs.iter().map(clean).collect() };
    call_with(ctx, name, args)
}

// ---------- FORECAST.ETS ----------

const MAX_SEASONALITY: usize = 8760;

/// Days since 1899-12-30 as (year, month, day), on the plain Gregorian calendar.
fn ymd(serial: f64) -> (i64, u32, u32) { crate::date::civil_from_days(serial.floor() as i64 + crate::date::days_from_civil(1899, 12, 30)) }
fn dim(y: i64, m: u32) -> f64 { crate::date::days_in_month(y, m) as f64 }

/// A timeline of dates on one day of each month (or every month's last day): (day, end_of_month).
fn month_calendar(xs: &[f64]) -> Option<(u32, bool)> {
    if xs.len() < 2 || !xs.iter().all(|&x| x.fract() == 0.0 && x > 0.0 && x < 3e6) { return None; }
    let parts: Vec<_> = xs.iter().map(|&x| ymd(x)).collect();
    let mut months: Vec<i64> = parts.iter().map(|p| p.0 * 12 + p.1 as i64).collect(); months.sort(); months.dedup();
    let mut d: Vec<u64> = xs.iter().map(|x| x.to_bits()).collect(); d.sort(); d.dedup();
    if months.len() < 2 || months.len() != d.len() { return None; }
    let day = parts[0].2;
    let eom = parts.iter().all(|&(y, m, dd)| dd as f64 == dim(y, m));
    if !eom && !parts.iter().all(|p| p.2 == day) { return None; }
    Some((day, eom))
}
fn month_pos(serial: f64, (day, eom): (u32, bool)) -> f64 {
    let anchor = |y: i64, m: u32| if eom { dim(y, m) } else { (day as f64).min(dim(y, m)) };
    let (y, m, d) = ymd(serial); let d = d as f64;
    let index = (y * 12 + m as i64) as f64;
    let here = anchor(y, m);
    if d == here { return index; }
    if d > here { let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) }; return index + (d - here) / (dim(y, m) - here + anchor(ny, nm)); }
    let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
    index - (here - d) / (dim(py, pm) - anchor(py, pm) + here)
}

struct Model { m: usize, n: usize, alpha: f64, beta: f64, gamma: f64, l: f64, b: f64, seas: Vec<f64>, mae: f64, rmse: f64, smape: f64, mase: f64, sigma: f64, step_stat: f64, origin: f64, step: f64, months: Option<(u32, bool)> }
impl Model { fn to_index(&self, x: f64) -> f64 { let p = match self.months { Some(c) => month_pos(x, c), None => x }; (p - self.origin) / self.step } }

fn ets_opt(ctx: &Ctx, a: Option<&Arg>, d: f64) -> Result<f64, ErrKind> { match a { None | Some(Arg::Missing) => Ok(d), Some(x) => match ctx.scalar_of(x) { Value::Empty => Ok(d), v => v.to_num() } } }
fn ets_model(ctx: &Ctx, values: &Arg, timeline: &Arg, season: Option<&Arg>, fill: Option<&Arg>, agg: Option<&Arg>) -> Result<Model, ErrKind> {
    let (season, fill, agg) = (ets_opt(ctx, season, 1.0)?, ets_opt(ctx, fill, 1.0)?, ets_opt(ctx, agg, 1.0)?);
    if season.fract() != 0.0 || season < 0.0 { return Err(ErrKind::Num); }
    if fill != 0.0 && fill != 1.0 { return Err(ErrKind::Num); }
    if agg.fract() != 0.0 || !(1.0..=7.0).contains(&agg) { return Err(ErrKind::Num); }
    let (ys, xs) = (flat(ctx, values), flat(ctx, timeline));
    if ys.len() != xs.len() { return Err(ErrKind::NA); }
    let mut pts = vec![];
    for (x, y) in xs.iter().zip(&ys) {
        if let Value::Err(e) = x { return Err(*e); }
        if let Value::Err(e) = y { return Err(*e); }
        if x.is_empty() && y.is_empty() { continue; }
        let Value::Num(x) = x else { return Err(ErrKind::Value) };
        pts.push((*x, if let Value::Num(v) = y { Some(*v) } else { None }));
    }
    if pts.len() < 2 || !pts.iter().any(|p| p.1.is_some()) { return Err(ErrKind::Value); }
    prepare_and_fit(&pts, fill == 1.0, agg as u32, (season as usize).min(usize::MAX))
}
fn aggregate_bucket(v: &[f64], agg: u32) -> f64 {
    match agg {
        1 => v.iter().sum::<f64>() / v.len() as f64, 2 | 3 => v.len() as f64, 4 => v.iter().copied().fold(f64::MIN, f64::max),
        5 => { let mut s = v.to_vec(); s.sort_by(|a, b| a.total_cmp(b)); let k = s.len() / 2; if s.len() % 2 == 1 { s[k] } else { (s[k - 1] + s[k]) / 2.0 } }
        6 => v.iter().copied().fold(f64::MAX, f64::min), _ => v.iter().sum(),
    }
}
fn prepare_and_fit(pts: &[(f64, Option<f64>)], completion: bool, agg: u32, season: usize) -> Result<Model, ErrKind> {
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let months = month_calendar(&xs);
    let pos = |x: f64| match months { Some(c) => month_pos(x, c), None => x };
    let located: Vec<(f64, Option<f64>)> = pts.iter().map(|&(x, y)| (pos(x), y)).collect();
    let mut distinct: Vec<f64> = located.iter().map(|p| p.0).collect(); distinct.sort_by(|a, b| a.total_cmp(b)); distinct.dedup();
    if distinct.len() < 2 { return Err(ErrKind::Num); }
    // The most common positive gap (to 12 digits), the smaller on a tie.
    let mut counts: Vec<(f64, usize)> = vec![];
    for w in distinct.windows(2) { let d: f64 = format!("{:.11e}", w[1] - w[0]).parse().unwrap_or(0.0); if d > 0.0 { match counts.iter_mut().find(|c| c.0 == d) { Some(c) => c.1 += 1, None => counts.push((d, 1)) } } }
    let (mut step, mut best) = (0.0, 0);
    for (d, c) in counts { if c > best || (c == best && d < step) { step = d; best = c; } }
    if !(step > 0.0) { return Err(ErrKind::Num); }
    let origin = distinct[0];
    let mut buckets: HashMap<i64, Vec<f64>> = HashMap::new();
    for &(x, y) in &located { let e = buckets.entry(((x - origin) / step).round() as i64).or_default(); if let Some(y) = y { e.push(y); } }
    let count = (*buckets.keys().max().unwrap_or(&0) + 1) as f64;
    if count - buckets.len() as f64 > 0.3 * count || count > 1e7 { return Err(ErrKind::Num); }
    let count = count as usize;
    let mut raw: Vec<Option<f64>> = vec![None; count];
    for (k, v) in &buckets { if !v.is_empty() && *k >= 0 { raw[*k as usize] = Some(aggregate_bucket(v, agg)); } }
    if raw.iter().filter(|v| v.is_none()).count() as f64 > 0.3 * count as f64 { return Err(ErrKind::Num); }
    if raw[0].is_none() { raw[0] = Some(if completion { raw.iter().flatten().next().copied().unwrap_or(0.0) } else { 0.0 }); }
    if raw[count - 1].is_none() { raw[count - 1] = Some(if completion { raw.iter().rev().flatten().next().copied().unwrap_or(0.0) } else { 0.0 }); }
    let mut y: Vec<f64> = raw.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
    let mut i = 1;
    while i + 1 < count {
        if raw[i].is_some() { i += 1; continue; }
        let mut j = i; while raw[j].is_none() { j += 1; }
        let (prev, next) = (y[i - 1], raw[j].unwrap_or(0.0));
        for k in i..j { y[k] = if completion { prev + (next - prev) * (k - i + 1) as f64 / (j - i + 1) as f64 } else { 0.0 }; }
        i = j;
    }
    let step_stat = if months.is_some() { if step % 12.0 == 0.0 { 365.0 * step / 12.0 } else { 31.0 * step } } else { step };
    Ok(fit_ets(&y, season, step_stat, origin, step, months))
}
fn slope_of(y: &[f64]) -> f64 {
    let n = y.len(); if n < 2 { return 0.0; }
    let mx = (n - 1) as f64 / 2.0; let my = mean(y);
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, v) in y.iter().enumerate() { sxy += (i as f64 - mx) * (v - my); sxx += (i as f64 - mx).powi(2); }
    if sxx != 0.0 { sxy / sxx } else { 0.0 }
}
fn detect_seasonality(y: &[f64]) -> usize {
    let n = y.len(); if n < 4 { return 0; }
    let (b, m) = (slope_of(y), mean(y));
    let z: Vec<f64> = y.iter().enumerate().map(|(i, v)| v - m - b * (i as f64 - (n - 1) as f64 / 2.0)).collect();
    let den: f64 = z.iter().map(|v| v * v).sum();
    if !(den > 1e-12 * n as f64 * (m.abs() + 1.0).powi(2)) { return 0; }
    let max_lag = (n - 1) / 3;
    let mut acf = vec![0.0; max_lag + 2];
    for k in 1..=max_lag + 1 { acf[k] = (0..n.saturating_sub(k)).map(|i| z[i] * z[i + k]).sum::<f64>() / den; }
    let (mut best, mut bv) = (0, 0.3);
    for k in 2..=max_lag { let next = if k + 1 <= max_lag + 1 { acf[k + 1] } else { f64::NEG_INFINITY }; if acf[k] > acf[k - 1] && acf[k] >= next && acf[k] > bv { best = k; bv = acf[k]; } }
    best.min(MAX_SEASONALITY)
}
fn initial_state(y: &[f64], m: usize) -> (f64, Vec<f64>) {
    let n = y.len();
    if m == 0 { return (slope_of(y), vec![]); }
    let (mut sxy, mut sxx) = (0.0, 0.0); let (mut mt, mut my) = (vec![0.0; m], vec![0.0; m]);
    for ph in 0..m {
        let (mut st, mut sy, mut c) = (0.0, 0.0, 0.0);
        let mut t = ph; while t < n { st += t as f64; sy += y[t]; c += 1.0; t += m; }
        mt[ph] = if c > 0.0 { st / c } else { ph as f64 }; my[ph] = if c > 0.0 { sy / c } else { y[n - 1] };
        let mut t = ph; while t < n { sxy += (t as f64 - mt[ph]) * (y[t] - my[ph]); sxx += (t as f64 - mt[ph]).powi(2); t += m; }
    }
    let b = if sxx != 0.0 { sxy / sxx } else { slope_of(y) };
    let s: Vec<f64> = (0..m).map(|ph| my[ph] - b * mt[ph]).collect();
    let mu = mean(&s);
    (b, s.into_iter().map(|v| v - mu).collect())
}
struct Run { l: f64, b: f64, seas: Vec<f64>, errors: Vec<f64>, fitted: Vec<f64>, sse: f64 }
fn smooth(y: &[f64], m: usize, alpha: f64, beta: f64, gamma: f64, init: &(f64, Vec<f64>)) -> Run {
    let n = y.len();
    let mut seas = if m > 0 { vec![0.0; n] } else { vec![] };
    let mut b = init.0;
    let mut l = y[0] - if m > 0 { init.1[0] } else { 0.0 };
    if m > 0 { seas[0] = init.1[0]; }
    let (mut errors, mut fitted) = (vec![0.0; n], vec![0.0; n]); fitted[0] = y[0];
    let mut sse = 0.0;
    for t in 1..n {
        let sp = if m > 0 { if t >= m { seas[t - m] } else { init.1[t % m] } } else { 0.0 };
        let f = l + b + sp; let e = y[t] - f;
        fitted[t] = f; errors[t] = e; sse += e * e;
        l = f - sp + alpha * e; b += beta * e;
        if m > 0 { seas[t] = sp + gamma * e; }
    }
    Run { l, b, seas, errors, fitted, sse }
}
fn stat_window(n: usize) -> usize { if n <= 10 { n } else { ((0.4 * n as f64).floor().min(10.0)).max((0.2 * n as f64).ceil()) as usize } }
fn fit_ets(y: &[f64], seasonality: usize, step_stat: f64, origin: f64, step: f64, months: Option<(u32, bool)>) -> Model {
    const GRID: [f64; 12] = [0.002, 0.1, 0.125, 0.126, 0.25, 0.251, 0.5, 0.501, 0.75, 0.751, 0.9, 0.998];
    const TREND: [f64; 4] = [0.001, 0.1, 0.25, 0.5];
    const SEASON: [f64; 5] = [0.001, 0.125, 0.25, 0.5, 0.75];
    let n = y.len();
    let mut m = if seasonality == 1 { detect_seasonality(y) } else { seasonality };
    if m >= n || m < 2 { m = 0; }
    let init = initial_state(y, m);
    let mut best: (f64, f64, f64, Run);
    if n <= 2 { best = (0.9, 0.001, 0.0, smooth(y, 0, 0.9, 0.001, 0.0, &init)); }
    else {
        let mut cur: Option<(f64, f64, f64, Run)> = None;
        for &al in &GRID { for &be in &TREND { for &ga in if m > 0 { &SEASON[..] } else { &[0.0][..] } {
            let run = smooth(y, m, al, be, ga, &init);
            if cur.as_ref().is_none_or(|c| run.sse < c.3.sse - 1e-12 * (1.0 + c.3.sse)) { cur = Some((al, be, ga, run)); }
        } } }
        best = cur.unwrap_or_else(|| (0.9, 0.001, 0.0, smooth(y, m, 0.9, 0.001, 0.0, &init)));
    }
    if n > 2 && !(best.3.sse > 1e-20 * (1.0 + y.iter().map(|v| v * v).sum::<f64>())) { best = (0.9, 0.001, 0.001, smooth(y, m, 0.9, 0.001, 0.001, &init)); }
    let run = &best.3;
    let lw = stat_window(n);
    let (mut sae, mut sse, mut sape) = (0.0, 0.0, 0.0);
    for t in n - lw..n { let e = run.errors[t]; sae += e.abs(); sse += e * e; let d = y[t].abs() + run.fitted[t].abs(); if d != 0.0 { sape += 2.0 * e.abs() / d; } }
    let train = if lw < n { n - lw } else { n };
    let naive = if train > 1 { (1..train).map(|t| (y[t] - y[t - 1]).abs()).sum::<f64>() / (train - 1) as f64 } else { 0.0 };
    let mae = sae / lw as f64;
    let sse_all: f64 = run.errors.iter().map(|e| e * e).sum();
    Model { m, n, alpha: best.0, beta: best.1, gamma: if m > 0 { best.2 } else { f64::EPSILON }, l: run.l, b: run.b, seas: run.seas.clone(), mae, rmse: (sse / lw as f64).sqrt(), smape: sape / lw as f64, mase: if naive != 0.0 { mae / naive } else { 0.0 }, sigma: (sse_all / n as f64).sqrt(), step_stat, origin, step, months }
}
fn line_at(md: &Model, h: f64) -> f64 {
    if md.m == 0 { return md.l + h * md.b; }
    let m = md.m as i64;
    let at = |k: i64| md.seas[md.n - md.m + (((k - 1) % m + m) % m) as usize];
    let k = h.floor(); let frac = h - k; let base = md.l + h * md.b;
    if k.abs() > 1e15 { return f64::NAN; }
    let k = k as i64;
    if frac != 0.0 { base + at(k) + frac * (at(k + 1) - at(k)) } else { base + at(k) }
}
fn forecast_at(md: &Model, target: f64) -> Value { let i = md.to_index(target); if i < -1e-9 { return err(ErrKind::Num); } num(line_at(md, i - (md.n - 1) as f64)) }
fn confidence_at(md: &Model, target: f64, level: f64) -> Value {
    if !(0.0..1.0).contains(&level) { return err(ErrKind::Num); }
    let idx = md.to_index(target);
    if idx < -1e-9 { return err(ErrKind::Num); }
    if level == 0.0 { return num(0.0); }
    let h = (idx - (md.n - 1) as f64 - 1e-9).ceil().max(1.0);
    if h > 1e7 { return err(ErrKind::Num); }
    let mut v = 1.0;
    for j in 1..h as usize { let c = md.alpha + md.beta * j as f64 + if md.m > 0 && j % md.m == 0 { md.gamma } else { 0.0 }; v += c * c; }
    num(norm_inv(0.5 + level / 2.0) * md.sigma * v.sqrt())
}
fn statistic(md: &Model, t: u32) -> f64 { match t { 1 => md.alpha, 2 => md.beta, 3 => md.gamma, 4 => md.mase, 5 => md.smape, 6 => md.mae, 7 => md.rmse, _ => md.step_stat } }
fn ets_each(ctx: &Ctx, target: &Arg, f: impl Fn(f64) -> Value) -> Value { lift1(ctx, target, f) }
