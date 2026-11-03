//! Financial functions: annuities (PMT, RATE…), cash flows (NPV, IRR, XIRR…), depreciation
//! (SLN, DB, VDB, AMORLINC…), discount securities and T-bills, and coupon bonds (PRICE, YIELD,
//! DURATION, ODDF*/ODDL*), ported from formula/functions.js.

use super::time::yearfrac;
use super::*;
use crate::date::{days_in_month, serial, ymd};

pub const DEFS: &[Def] = &[
    s("PMT", 3, 5, |a| { let (r, n, p, f, t) = (n!(a[0]), n!(a[1]), n!(a[2]), tryv!(opt(a, 3, 0.0)), tryv!(opt(a, 4, 0.0))); pmt(r, n, p, f, t) }),
    s("IPMT", 4, 6, |a| { let v = tryv!(nums(a, 6, &[(4, 0.0), (5, 0.0)])); ipmt(v[0], v[1], v[2], v[3], v[4], v[5]) }),
    s("PPMT", 4, 6, |a| { let v = tryv!(nums(a, 6, &[(4, 0.0), (5, 0.0)])); let p = tryv!(val(pmt(v[0], v[2], v[3], v[4], v[5]))); let i = tryv!(val(ipmt(v[0], v[1], v[2], v[3], v[4], v[5]))); num(p - i) }),
    s("FV", 3, 5, |a| { let v = tryv!(nums(a, 5, &[(3, 0.0), (4, 0.0)])); let (r, n, m, p, t) = (v[0], v[1], v[2], v[3], tf(v[4])); if r == 0.0 { return num(-(p + m * n)); } let q = (1.0 + r).powf(n); num(-(p * q + m * (1.0 + r * t) * (q - 1.0) / r)) }),
    s("PV", 3, 5, |a| { let v = tryv!(nums(a, 5, &[(3, 0.0), (4, 0.0)])); let (r, n, m, f, t) = (v[0], v[1], v[2], v[3], tf(v[4])); if r == 0.0 { return num(-(f + m * n)); } let q = (1.0 + r).powf(n); num(-(f + m * (1.0 + r * t) * (q - 1.0) / r) / q) }),
    s("NPER", 3, 5, |a| { let v = tryv!(nums(a, 5, &[(3, 0.0), (4, 0.0)])); let (r, m, p, f, t) = (v[0], v[1], v[2], v[3], tf(v[4]));
        if r <= -1.0 { return err(ErrKind::Num); } if r == 0.0 { return if m == 0.0 { err(ErrKind::Num) } else { num(-(p + f) / m) }; }
        let adj = m * (1.0 + r * t); let q = (adj - f * r) / (p * r + adj); if !(q > 0.0) { return err(ErrKind::Num); } num(q.ln() / (1.0 + r).ln()) }),
    s("RATE", 3, 6, rate),
    e("NPV", 2, MANY, |ctx, a| { let r = tryv!(arg_num(ctx, a, 0, 0.0)); let l = tryv!(numbers(ctx, &a[1..])); if r <= -1.0 { return err(ErrKind::Num); } num(l.iter().enumerate().map(|(i, v)| v / (1.0 + r).powf(i as f64 + 1.0)).sum()) }),
    e("IRR", 1, 2, |ctx, a| { let l = tryv!(numbers(ctx, &a[..1])); let g = tryv!(arg_num(ctx, a, 1, 0.1));
        if !l.iter().any(|&v| v > 0.0) || !l.iter().any(|&v| v < 0.0) { return err(ErrKind::Num); }
        let scale = l.iter().fold(0.0f64, |m, v| m.max(v.abs())); solve_rate(&|r| l.iter().enumerate().map(|(i, v)| v / (1.0 + r).powf(i as f64)).sum(), g, scale) }),
    e("XNPV", 3, 3, |ctx, a| { let r = tryv!(arg_num(ctx, a, 0, 0.0)); if r <= -1.0 { return err(ErrKind::Num); } let (vs, ds) = tryv!(flows(ctx, &a[1], &a[2]));
        let d0 = ds[0]; let mut t = 0.0; for (v, d) in vs.iter().zip(&ds) { if *d < d0 { return err(ErrKind::Num); } t += v / (1.0 + r).powf((d - d0) / 365.0); } num(t) }),
    e("XIRR", 2, 3, |ctx, a| { let g = tryv!(arg_num(ctx, a, 2, 0.1)); let (vs, ds) = tryv!(flows(ctx, &a[0], &a[1]));
        if !vs.iter().any(|&v| v > 0.0) || !vs.iter().any(|&v| v < 0.0) { return err(ErrKind::Num); }
        let d0 = ds[0]; if ds.iter().any(|&d| d < d0) { return err(ErrKind::Num); }
        let scale = vs.iter().fold(0.0f64, |m, v| m.max(v.abs())); solve_rate(&|r| vs.iter().zip(&ds).map(|(v, d)| v / (1.0 + r).powf((d - d0) / 365.0)).sum(), g, scale) }),
    e("MIRR", 3, 3, |ctx, a| { let fl = tryv!(numbers(ctx, &a[..1])); let (fr, rr) = (tryv!(arg_num(ctx, a, 1, 0.0)), tryv!(arg_num(ctx, a, 2, 0.0))); let n = fl.len();
        if !fl.iter().any(|&v| v < 0.0) || !fl.iter().any(|&v| v > 0.0) { return err(ErrKind::Div0); }
        let neg: f64 = fl.iter().enumerate().filter(|(_, v)| **v < 0.0).map(|(i, v)| v / (1.0 + fr).powf(i as f64)).sum();
        let pos: f64 = fl.iter().enumerate().filter(|(_, v)| **v > 0.0).map(|(i, v)| v * (1.0 + rr).powf((n - 1 - i) as f64)).sum();
        num((pos / -neg).powf(1.0 / (n as f64 - 1.0)) - 1.0) }),
    e("FVSCHEDULE", 2, 2, |ctx, a| { let mut out = tryv!(arg_num(ctx, a, 0, 0.0)); let mut bad = None;
        each(ctx, &a[1], &mut |v, direct| { if bad.is_some() { return; } match v { Value::Empty => {} Value::Err(e) => bad = Some(*e), Value::Num(r) => out *= 1.0 + r, _ if !direct => bad = Some(ErrKind::Value), v => match v.to_num() { Ok(r) => out *= 1.0 + r, Err(e) => bad = Some(e) } } });
        match bad { Some(e) => err(e), None => num(out) } }),
    s("SLN", 3, 3, |a| { let (c, s, l) = (n!(a[0]), n!(a[1]), n!(a[2])); if l == 0.0 { err(ErrKind::Div0) } else { num((c - s) / l) } }),
    s("SYD", 4, 4, |a| { let (c, s, l, k) = (n!(a[0]), n!(a[1]), n!(a[2]), n!(a[3])); if l <= 0.0 || k < 1.0 || k > l { return err(ErrKind::Num); } num((c - s) * (l - k + 1.0) * 2.0 / (l * (l + 1.0))) }),
    s("DDB", 4, 5, |a| { let v = tryv!(nums(a, 5, &[(4, 2.0)])); let (c, s, l, k, f) = (v[0], v[1], v[2], v[3], v[4]);
        if l <= 0.0 || k < 1.0 || k > l || c < 0.0 || s < 0.0 || f <= 0.0 || k.ceil() > MAX_PERIODS { return err(ErrKind::Num); }
        let (mut book, mut dep) = (c, 0.0); for _ in 1..=k.ceil() as u64 { dep = (book * (f / l)).min((book - s).max(0.0)); book -= dep; if dep == 0.0 { break; } } num(dep) }),
    s("DB", 4, 5, |a| { let v = tryv!(nums(a, 5, &[(4, 12.0)])); let (c, sv, l, k, m) = (v[0], v[1], v[2], v[3], v[4]);
        if c < 0.0 || sv < 0.0 || l <= 0.0 || k < 1.0 || m < 1.0 || m > 12.0 || k > l.ceil() + if m < 12.0 { 1.0 } else { 0.0 } || k > MAX_PERIODS { return err(ErrKind::Num); }
        let rate = ((1.0 - (sv / c).powf(1.0 / l)) * 1000.0).round() / 1000.0; let (mut book, mut dep) = (c, 0.0); let mut i = 1.0;
        while i <= k { dep = if i == 1.0 { c * rate * m / 12.0 } else if i == l.trunc() + 1.0 && m < 12.0 { book * rate * (12.0 - m) / 12.0 } else { book * rate }; book -= dep; if dep == 0.0 { break; } i += 1.0; }
        num(dep) }),
    s("VDB", 5, 7, |a| { let v = tryv!(nums(a, 6, &[(5, 2.0)])); let (c, sv, l, s0, e0, f) = (v[0], v[1], v[2], v[3], v[4], v[5]);
        let sw = match a.get(6) { None | Some(Value::Empty) => false, Some(x) => b!(x) };
        if c < 0.0 || sv < 0.0 || l <= 0.0 || s0 < 0.0 || e0 > l || s0 > e0 || f <= 0.0 || e0.ceil() > MAX_PERIODS { return err(ErrKind::Num); }
        let (mut total, mut book) = (0.0, c);
        for i in 1..=e0.ceil() as u64 { let i = i as f64; let ddb = (book * (f / l)).min((book - sv).max(0.0)); let sln = ((book - sv) / (l - i + 1.0)).max(0.0);
            let dep = if sw { ddb } else { ddb.max(sln) }; book -= dep; let (from, to) = (s0.max(i - 1.0), e0.min(i)); if to > from { total += dep * (to - from); } }
        num(total) }),
    s("CUMIPMT", 6, 6, |a| cumulative(a, false)), s("CUMPRINC", 6, 6, |a| cumulative(a, true)),
    s("ISPMT", 4, 4, |a| { let (r, k, n, p) = (n!(a[0]), n!(a[1]), n!(a[2]), n!(a[3])); if n == 0.0 { err(ErrKind::Div0) } else { num(-p * r * (1.0 - k / n)) } }),
    s("PDURATION", 3, 3, |a| { let (r, p, f) = (n!(a[0]), n!(a[1]), n!(a[2])); if r <= 0.0 || p <= 0.0 || f <= 0.0 { return err(ErrKind::Num); } num((f.ln() - p.ln()) / (1.0 + r).ln()) }),
    s("RRI", 3, 3, |a| { let (n, p, f) = (n!(a[0]), n!(a[1]), n!(a[2])); if n <= 0.0 || p == 0.0 { return err(ErrKind::Num); } num((f / p).powf(1.0 / n) - 1.0) }),
    s("EFFECT", 2, 2, |a| { let (r, k) = (n!(a[0]), n!(a[1]).trunc()); if r <= 0.0 || k < 1.0 { return err(ErrKind::Num); } num((1.0 + r / k).powf(k) - 1.0) }),
    s("NOMINAL", 2, 2, |a| { let (r, k) = (n!(a[0]), n!(a[1]).trunc()); if r <= 0.0 || k < 1.0 { return err(ErrKind::Num); } num(k * ((1.0 + r).powf(1.0 / k) - 1.0)) }),
    s("DOLLARDE", 2, 2, |a| dollar(a, true)), s("DOLLARFR", 2, 2, |a| dollar(a, false)),
    // Discount securities and Treasury bills.
    s("DISC", 4, 5, |a| { let (yf, _) = tryv!(disc_args(a, 4)); let (p, r) = (n!(a[2]), n!(a[3])); if p <= 0.0 || r <= 0.0 { return err(ErrKind::Num); } num((r - p) / r / yf) }),
    s("INTRATE", 4, 5, |a| { let (yf, _) = tryv!(disc_args(a, 4)); let (i, r) = (n!(a[2]), n!(a[3])); if i <= 0.0 || r <= 0.0 { return err(ErrKind::Num); } num((r - i) / i / yf) }),
    s("RECEIVED", 4, 5, |a| { let (yf, _) = tryv!(disc_args(a, 4)); let (i, d) = (n!(a[2]), n!(a[3])); if i <= 0.0 || d <= 0.0 || d * yf >= 1.0 { return err(ErrKind::Num); } num(i / (1.0 - d * yf)) }),
    s("PRICEDISC", 4, 5, |a| { let (yf, _) = tryv!(disc_args(a, 4)); let (d, r) = (n!(a[2]), n!(a[3])); if d <= 0.0 || r <= 0.0 { return err(ErrKind::Num); } num(r - d * r * yf) }),
    s("YIELDDISC", 4, 5, |a| { let (yf, _) = tryv!(disc_args(a, 4)); let (p, r) = (n!(a[2]), n!(a[3])); if p <= 0.0 || r <= 0.0 { return err(ErrKind::Num); } num((r - p) / p / yf) }),
    s("TBILLEQ", 3, 3, |a| { let (_, d) = tryv!(disc_args(&[a[0].clone(), a[1].clone(), Value::Empty, Value::Empty, Value::Num(2.0)], 4)); let r = n!(a[2]); if r <= 0.0 || d > 366.0 { return err(ErrKind::Num); } num(365.0 * r / (360.0 - r * d)) }),
    s("TBILLPRICE", 3, 3, |a| { let (_, d) = tryv!(disc_args(&[a[0].clone(), a[1].clone(), Value::Empty, Value::Empty, Value::Num(2.0)], 4)); let r = n!(a[2]); if r <= 0.0 || d > 366.0 { return err(ErrKind::Num); } let p = 100.0 * (1.0 - r * d / 360.0); if p <= 0.0 { err(ErrKind::Num) } else { num(p) } }),
    s("TBILLYIELD", 3, 3, |a| { let (_, d) = tryv!(disc_args(&[a[0].clone(), a[1].clone(), Value::Empty, Value::Empty, Value::Num(2.0)], 4)); let p = n!(a[2]); if p <= 0.0 || d > 366.0 { return err(ErrKind::Num); } num((100.0 - p) / p * (360.0 / d)) }),
    // Coupon bonds.
    s("COUPPCD", 3, 4, |a| { let c = tryv!(coupon(a, 0, 1, 2, 3)); num(c.pcd) }),
    s("COUPNCD", 3, 4, |a| { let c = tryv!(coupon(a, 0, 1, 2, 3)); num(c.ncd) }),
    s("COUPNUM", 3, 4, |a| { let c = tryv!(coupon(a, 0, 1, 2, 3)); num(c.num as f64) }),
    s("COUPDAYBS", 3, 4, |a| { let c = tryv!(coupon(a, 0, 1, 2, 3)); num(c.days().0) }),
    s("COUPDAYS", 3, 4, |a| { let c = tryv!(coupon(a, 0, 1, 2, 3)); num(c.days().1) }),
    s("COUPDAYSNC", 3, 4, |a| { let c = tryv!(coupon(a, 0, 1, 2, 3)); num(c.days().2) }),
    s("PRICE", 6, 7, |a| { let c = tryv!(coupon(a, 0, 1, 5, 6)); let v = tryv!(bond_nums(a, &[(2, false), (3, false), (4, true)])); num(c.price(v[0], v[1], v[2])) }),
    s("YIELD", 6, 7, |a| { let c = tryv!(coupon(a, 0, 1, 5, 6)); let v = tryv!(bond_nums(a, &[(2, false), (3, true), (4, true)])); c.yld(v[0], v[1], v[2]) }),
    s("DURATION", 5, 6, |a| { let c = tryv!(coupon(a, 0, 1, 4, 5)); let v = tryv!(bond_nums(a, &[(2, false), (3, false)])); num(c.duration(v[0], v[1])) }),
    s("MDURATION", 5, 6, |a| { let c = tryv!(coupon(a, 0, 1, 4, 5)); let v = tryv!(bond_nums(a, &[(2, false), (3, false)])); num(c.duration(v[0], v[1]) / (1.0 + v[1] / c.freq)) }),
    s("ACCRINT", 6, 8, accrint),
    s("ACCRINTM", 3, 5, |a| { let (i, s) = (n!(a[0]), n!(a[1])); let par = if matches!(a.get(3), None | Some(Value::Empty)) { Value::Num(1000.0) } else { a[3].clone() };
        let v = tryv!(bond_nums(&[a[2].clone(), par], &[(0, true), (1, true)])); if !(i.trunc() < s.trunc()) { return err(ErrKind::Num); } let yf = tryv!(yf(i, s, tryv!(opt(a, 4, 0.0)))); num(v[1] * v[0] * yf) }),
    s("PRICEMAT", 5, 6, |a| { let (im, sm, is, v) = tryv!(mat_args(a)); num((100.0 + im * v[0] * 100.0) / (1.0 + sm * v[1]) - is * v[0] * 100.0) }),
    s("YIELDMAT", 5, 6, |a| { let (im, sm, is, v) = tryv!(mat_args(a)); let base = v[1] / 100.0 + is * v[0]; num((1.0 + im * v[0] - base) / base / sm) }),
    s("ODDFPRICE", 8, 9, |a| { let p = tryv!(odd_first(a)); let v = tryv!(bond_nums(a, &[(4, false), (5, false), (6, true)])); num(p.price(v[0], v[1], v[2])) }),
    s("ODDFYIELD", 8, 9, |a| { let p = tryv!(odd_first(a)); let v = tryv!(bond_nums(a, &[(4, false), (5, true), (6, true)])); let (r, price, red) = (v[0], v[1], v[2]);
        let (mut lo, mut hi) = (-p.freq + 1e-6, 1.0); while p.price(r, hi, red) > price { hi *= 2.0; if hi > 1e6 { return err(ErrKind::Num); } }
        if !(p.price(r, lo, red) >= price) { return err(ErrKind::Num); }
        let mut i = 0; while i < 300 && hi - lo > 1e-15 { let mid = (lo + hi) / 2.0; if p.price(r, mid, red) > price { lo = mid } else { hi = mid } i += 1; } num((lo + hi) / 2.0) }),
    s("ODDLPRICE", 7, 8, |a| { let (freq, dc, ac, dsc) = tryv!(odd_last(a)); let v = tryv!(bond_nums(a, &[(3, false), (4, false), (5, true)])); let sc = 100.0 * v[0] / freq; num((v[2] + dc * sc) / (1.0 + dsc * v[1] / freq) - ac * sc) }),
    s("ODDLYIELD", 7, 8, |a| { let (freq, dc, ac, dsc) = tryv!(odd_last(a)); let v = tryv!(bond_nums(a, &[(3, false), (4, true), (5, true)])); let sc = 100.0 * v[0] / freq; let paid = v[1] + ac * sc; num((v[2] + dc * sc - paid) / paid * (freq / dsc)) }),
    s("AMORLINC", 6, 7, |a| { let (c, sv, per, r, yf) = tryv!(amor_args(a)); let one = c * r; let first = yf * r * c; let full = ((c - sv - first) / one).trunc();
        let dep = if per == 0.0 { first } else if per <= full { one } else if per == full + 1.0 { c - sv - one * full - first } else { 0.0 }; num(dep.max(0.0)) }),
    s("AMORDEGRC", 6, 7, |a| { let (c, sv, per, r, yf) = tryv!(amor_args(a)); let life = 1.0 / r; if !(life > 2.0) { return err(ErrKind::Num); }
        let rate = r * if life <= 4.0 { 1.5 } else if life <= 6.0 { 2.0 } else { 2.5 }; let round = |x: f64| super::math::round_to(x, 0);
        let first = round(yf * rate * c); if per == 0.0 { return num(first); } let periods = (life - 1e-9).ceil(); if per > periods - 1.0 { return num(0.0); }
        let (mut cost, mut half) = (c - first, None::<f64>); let mut k = 1.0;
        while k <= per { if cost < sv { return num(0.0); } if k >= periods - 2.0 && half.is_none() { half = Some(cost * 0.5); } let dep = half.unwrap_or(rate * cost); if k == per { return num(round(dep)); } cost -= dep; k += 1.0; }
        num(0.0) }),
];

const MAX_PERIODS: f64 = 1e6;
const MAX_DATE: f64 = 2958465.0;

fn tf(t: f64) -> f64 { if t != 0.0 { 1.0 } else { 0.0 } }
fn val(v: Value) -> Result<f64, ErrKind> { match v { Value::Num(n) => Ok(n), Value::Err(e) => Err(e), _ => Err(ErrKind::Value) } }

/// The first `count` arguments as numbers, missing ones taking the listed defaults.
fn nums(a: &[Value], count: usize, defaults: &[(usize, f64)]) -> Result<Vec<f64>, ErrKind> {
    (0..count).map(|i| { let d = defaults.iter().find(|x| x.0 == i).map(|x| x.1); match (a.get(i), d) { (None | Some(Value::Empty), Some(d)) => Ok(d), (Some(v), _) => v.to_num(), (None, None) => Ok(0.0) } }).collect()
}

fn pmt(r: f64, n: f64, p: f64, f: f64, t: f64) -> Value {
    if n == 0.0 { return err(ErrKind::Num); }
    if r == 0.0 { return num(-(p + f) / n); }
    let q = (1.0 + r).powf(n);
    num(-(r * (p * q + f)) / ((q - 1.0) * (1.0 + r * tf(t))))
}

/// Interest of period k: charged on the balance after k-1 payments (none on the first with payments in advance).
fn ipmt(r: f64, k: f64, n: f64, p: f64, f: f64, t: f64) -> Value {
    if k < 1.0 || k > n { return err(ErrKind::Num); }
    let m = tryv!(val(pmt(r, n, p, f, t)));
    let adv = t != 0.0;
    if adv && k == 1.0 { return num(0.0); }
    let per = k - 1.0; let q = (1.0 + r).powf(per);
    let bal = if r == 0.0 { -(p + m * per) } else { -(p * q + m * if adv { 1.0 + r } else { 1.0 } * ((q - 1.0) / r)) };
    num(if adv { bal * r / (1.0 + r) } else { bal * r })
}

fn rate(a: &[Value]) -> Value {
    let v = tryv!(nums(a, 6, &[(3, 0.0), (4, 0.0), (5, 0.1)]));
    let (n, m, p, f, t, g) = (v[0], v[1], v[2], v[3], tf(v[4]), v[5]);
    if n <= 0.0 { return err(ErrKind::Num); }
    let annuity = |r: f64| if r == 0.0 { n } else { (n * r.ln_1p()).exp_m1() / r };
    let bal = |r: f64| { let an = annuity(r); p * (1.0 + r * an) + m * (1.0 + r * t) * an + f };
    let scale = p.abs().max(f.abs()).max(m.abs() * n.abs());
    if scale == 0.0 { return err(ErrKind::Num); }
    let mut r = g;
    for _ in 0..100 {
        let y = bal(r);
        if y.abs() < 1e-12 * scale { return num(r); }
        let h = 1e-6; let dy = (bal(r + h) - y) / h;
        if !dy.is_finite() || dy == 0.0 { break; }
        let next = r - y / dy;
        if !next.is_finite() { break; }
        if (next - r).abs() < 1e-12 { return num(next); }
        r = next;
    }
    if bal(r).abs() < 1e-6 * scale { num(r) } else { err(ErrKind::Num) }
}

/// Newton's method on a cash-flow function, falling back to bisection; zero is judged relative to the largest flow.
fn solve_rate(f: &dyn Fn(f64) -> f64, guess: f64, scale: f64) -> Value {
    let tol = 1e-12 * scale; let mut r = guess;
    for _ in 0..100 {
        let y = f(r);
        if y.abs() < tol { return num(r); }
        let h = 1e-7; let dy = (f(r + h) - y) / h;
        if !dy.is_finite() || dy == 0.0 { break; }
        let next = r - y / dy;
        if !next.is_finite() || next <= -1.0 { break; }
        if (next - r).abs() < 1e-12 { return num(next); }
        r = next;
    }
    let sign = |x: f64| x.signum();
    let hi = 10.0; let fhi = f(hi); if fhi == 0.0 { return num(hi); }
    let (mut lo, mut flo) = (0.0, f(0.0));
    let mut k = 1;
    while k <= 16 && flo.is_finite() && flo != 0.0 && sign(flo) == sign(fhi) { let nx = -1.0 + 2f64.powi(-k); let fx = f(nx); if !fx.is_finite() { break; } lo = nx; flo = fx; k += 1; }
    if flo == 0.0 { return num(lo); }
    if !flo.is_finite() || !fhi.is_finite() || sign(flo) == sign(fhi) { return err(ErrKind::Num); }
    let mut hi = hi;
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0; let fm = f(mid);
        if fm.abs() < tol { return num(mid); }
        if !fm.is_finite() { return err(ErrKind::Num); }
        if sign(fm) == sign(flo) { lo = mid; flo = fm; } else { hi = mid; }
    }
    num((lo + hi) / 2.0)
}

/// The values and whole-day dates of XNPV/XIRR; anything but a number is #VALUE!.
fn flows(ctx: &Ctx, v: &Arg, d: &Arg) -> Result<(Vec<f64>, Vec<f64>), ErrKind> {
    let vs = flat(ctx, v); let ds = flat(ctx, d);
    if vs.len() != ds.len() || vs.is_empty() { return Err(ErrKind::Num); }
    let f = |l: &[Value], t: bool| l.iter().map(|v| match v { Value::Num(n) => Ok(if t { n.trunc() } else { *n }), Value::Err(e) => Err(*e), _ => Err(ErrKind::Value) }).collect::<Result<Vec<f64>, ErrKind>>();
    Ok((f(&vs, false)?, f(&ds, true)?))
}

fn cumulative(a: &[Value], principal: bool) -> Value {
    let v = tryv!(nums(a, 6, &[]));
    let (r, n, p, s, e, t) = (v[0], v[1], v[2], v[3].trunc(), v[4].trunc(), v[5]);
    if r <= 0.0 || n <= 0.0 || p <= 0.0 || s < 1.0 || e < s || e > n || (t != 0.0 && t != 1.0) || e - s > MAX_PERIODS { return err(ErrKind::Num); }
    let m = if principal { tryv!(val(pmt(r, n, p, 0.0, t))) } else { 0.0 };
    let mut total = 0.0; let mut k = s;
    while k <= e { let i = tryv!(val(ipmt(r, k, n, p, 0.0, t))); total += if principal { m - i } else { i }; k += 1.0; }
    num(total)
}

fn dollar(a: &[Value], de: bool) -> Value {
    let (x, f) = (n!(a[0]), n!(a[1]).trunc());
    if f < 0.0 { return err(ErrKind::Num); } if f == 0.0 { return err(ErrKind::Div0); }
    let (whole, frac) = (x.trunc(), x - x.trunc()); let digits = 10f64.powf(f.log10().ceil());
    num(whole + if de { frac * digits / f } else { frac * f / digits })
}

/// YEARFRAC on whole days; a bad basis is #NUM!.
fn yf(s: f64, e: f64, b: f64) -> Result<f64, ErrKind> { let b = b.trunc(); if !(0.0..=4.0).contains(&b) { return Err(ErrKind::Num); } yearfrac(s.trunc(), e.trunc(), b as i64).ok_or(ErrKind::Num) }

/// Settlement < maturity (args 0, 1), basis at `bi`: the year fraction and the days between.
fn disc_args(a: &[Value], bi: usize) -> Result<(f64, f64), ErrKind> {
    let (s, m) = (a[0].to_num()?, a[1].to_num()?);
    if !(s.trunc() < m.trunc()) { return Err(ErrKind::Num); }
    Ok((yf(s, m, opt(a, bi, 0.0)?)?, m.trunc() - s.trunc()))
}

fn bond_nums(a: &[Value], list: &[(usize, bool)]) -> Result<Vec<f64>, ErrKind> {
    list.iter().map(|&(i, pos)| { let n = a.get(i).cloned().unwrap_or(Value::Empty).to_num()?; if if pos { !(n > 0.0) } else { n < 0.0 } { Err(ErrKind::Num) } else { Ok(n) } }).collect()
}

fn days360(a: f64, b: f64, european: bool) -> f64 {
    let (y1, m1, mut d1) = ymd(a, false); let (y2, m2, mut d2) = ymd(b, false);
    if european { d1 = d1.min(30); d2 = d2.min(30); } else {
        let a_last = d1 == days_in_month(y1, m1);
        if m1 == 2 && a_last && m2 == 2 && d2 == days_in_month(y2, 2) { d2 = 30; }
        if d1 == 31 || (m1 == 2 && a_last) { d1 = 30; }
        if d2 == 31 && d1 == 30 { d2 = 30; }
    }
    ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + d2 as i64 - d1 as i64) as f64
}
fn basis_days(a: f64, b: f64, basis: i64) -> f64 { match basis { 0 => days360(a, b, false), 4 => days360(a, b, true), _ => b.trunc() - a.trunc() } }
fn period_len(start: f64, end: f64, basis: i64, freq: f64) -> f64 { match basis { 1 => end - start, 3 => 365.0 / freq, _ => 360.0 / freq } }

/// Coupon dates every 12/freq months from an anchor: k periods after it (k < 0 before); month ends stay month ends.
fn schedule(anchor: f64, freq: f64) -> impl Fn(i64) -> f64 {
    let (y, m, d) = ymd(anchor, false); let eom = d == days_in_month(y, m); let step = (12.0 / freq) as i64;
    move |k| { let t = y * 12 + (m as i64 - 1) + k * step; let (ty, tm) = (t.div_euclid(12), (t.rem_euclid(12) + 1) as u32); let last = days_in_month(ty, tm);
        serial(ty, tm as i64, if eom { last } else { d.min(last) } as i64, false) }
}

fn freq_basis(f: &Value, b: Option<&Value>) -> Result<(f64, i64), ErrKind> {
    let freq = f.to_num()?.trunc(); let bas = match b { None | Some(Value::Empty) => 0.0, Some(v) => v.to_num()? }.trunc();
    if ![1.0, 2.0, 4.0].contains(&freq) || !(0.0..=4.0).contains(&bas) { return Err(ErrKind::Num); }
    Ok((freq, bas as i64))
}

struct Coupon { s: f64, freq: f64, basis: i64, pcd: f64, ncd: f64, num: i64 }

fn date_arg(v: &Value) -> Result<f64, ErrKind> { let n = v.to_num()?; if n < 0.0 || n >= MAX_DATE + 1.0 { Err(ErrKind::Num) } else { Ok(n) } }

fn coupon(a: &[Value], si: usize, mi: usize, fi: usize, bi: usize) -> Result<Coupon, ErrKind> {
    let (s, m) = (date_arg(&a[si])?.trunc(), date_arg(&a[mi])?.trunc());
    let (freq, basis) = freq_basis(&a[fi], a.get(bi))?;
    coupon_of(s, m, freq, basis)
}
fn coupon_of(s: f64, m: f64, freq: f64, basis: i64) -> Result<Coupon, ErrKind> {
    if !(s < m) { return Err(ErrKind::Num); }
    let at = schedule(m, freq); let mut k = 0; while at(-k) > s { k += 1; }
    Ok(Coupon { s, freq, basis, pcd: at(-k), ncd: at(1 - k), num: k })
}

impl Coupon {
    /// A (accrued days), E (days in the period), DSC (days to the next coupon).
    fn days(&self) -> (f64, f64, f64) {
        let a = basis_days(self.pcd, self.s, self.basis);
        let e = match self.basis { 1 => self.ncd - self.pcd, 3 => 365.0 / self.freq, _ => 360.0 / self.freq };
        (a, e, if self.basis == 0 || self.basis == 4 { e - a } else { self.ncd - self.s })
    }
    fn price(&self, rate: f64, yld: f64, red: f64) -> f64 {
        let (a, e, dsc) = self.days(); let n = self.num as f64; let c = 100.0 * rate / self.freq; let y = 1.0 + yld / self.freq;
        if self.num == 1 { return (red + c) / (1.0 + yld / self.freq * (dsc / e)) - c * (a / e); }
        let mut p = red / y.powf(n - 1.0 + dsc / e);
        for k in 1..=self.num { p += c / y.powf(k as f64 - 1.0 + dsc / e); }
        p - c * (a / e)
    }
    fn yld(&self, rate: f64, pr: f64, red: f64) -> Value {
        let (a, e, dsc) = self.days(); let c = rate / self.freq;
        if self.num == 1 { let base = pr / 100.0 + a / e * c; return num((red / 100.0 + c - base) / base * (self.freq * e / dsc)); }
        let (mut lo, mut hi) = (-0.99 * self.freq, 10.0);
        if self.price(rate, hi, red) > pr { return err(ErrKind::Num); }
        for _ in 0..200 { let mid = (lo + hi) / 2.0; if self.price(rate, mid, red) > pr { lo = mid } else { hi = mid } if hi - lo < 1e-14 { break; } }
        num((lo + hi) / 2.0)
    }
    fn duration(&self, rate: f64, yld: f64) -> f64 {
        let (_, e, dsc) = self.days(); let c = 100.0 * rate / self.freq; let y = 1.0 + yld / self.freq;
        let (mut d, mut p) = (0.0, 0.0);
        for k in 1..=self.num { let t = k as f64 - 1.0 + dsc / e; let cash = c + if k == self.num { 100.0 } else { 0.0 }; d += t * cash / y.powf(t); p += cash / y.powf(t); }
        d / p / self.freq
    }
}

fn accrint(a: &[Value]) -> Value {
    let (i, fi, s) = (tryv!(date_arg(&a[0])), tryv!(date_arg(&a[1])), tryv!(date_arg(&a[2])));
    let f = n!(a[5]);
    let par = if matches!(a.get(4), None | Some(Value::Empty)) { Value::Num(1000.0) } else { a[4].clone() };
    let v = tryv!(bond_nums(&[a[3].clone(), par], &[(0, true), (1, true)]));
    let b = tryv!(opt(a, 6, 0.0)).trunc();
    let from_issue = match a.get(7) { None | Some(Value::Empty) => true, Some(x) => b!(x) };
    let freq = f.trunc();
    if ![1.0, 2.0, 4.0].contains(&freq) || !(0.0..=4.0).contains(&b) || !(i.trunc() < s.trunc()) { return err(ErrKind::Num); }
    let from = if !from_issue && s > fi { fi } else { i }.trunc(); let to = s.trunc();
    if b != 1.0 { return num(v[1] * v[0] * tryv!(yf(from, to, b))); }
    let at = schedule(fi.trunc(), freq);
    let mut k = 0; while at(k) > from { k -= 1; } while at(k + 1) <= from { k += 1; }
    let mut periods = 0.0; while at(k) < to { periods += (to.min(at(k + 1)) - from.max(at(k))) / (at(k + 1) - at(k)); k += 1; }
    num(v[1] * v[0] / freq * periods)
}

/// PRICEMAT/YIELDMAT: the year fractions issue→maturity, settlement→maturity, issue→settlement and [rate, yld|pr].
fn mat_args(a: &[Value]) -> Result<(f64, f64, f64, Vec<f64>), ErrKind> {
    let (s, m, i) = (a[0].to_num()?, a[1].to_num()?, a[2].to_num()?);
    let v = bond_nums(a, &[(3, false), (4, false)])?;
    if !(s.trunc() < m.trunc()) || !(i.trunc() < s.trunc()) { return Err(ErrKind::Num); }
    let b = opt(a, 5, 0.0)?;
    Ok((yf(i, m, b)?, yf(s, m, b)?, yf(i, s, b)?, v))
}

fn amor_args(a: &[Value]) -> Result<(f64, f64, f64, f64, f64), ErrKind> {
    let v = nums(a, 7, &[(6, 0.0)])?;
    let (c, d, fp, sv, per, r, b) = (v[0], v[1], v[2], v[3], v[4], v[5], v[6].trunc());
    if c < 0.0 || sv < 0.0 || sv > c || per < 0.0 || r <= 0.0 || d > fp || ![0.0, 1.0, 3.0, 4.0].contains(&b) { return Err(ErrKind::Num); }
    Ok((c, sv, per.trunc(), r, yf(d, fp, b)?))
}

struct OddFirst { freq: f64, dc_sum: f64, a_sum: f64, nq: f64, dsc: f64, e: f64, n: i64 }
impl OddFirst {
    fn price(&self, rate: f64, yld: f64, red: f64) -> f64 {
        let sc = 100.0 * rate / self.freq; let f = 1.0 + yld / self.freq; let t = self.nq + self.dsc / self.e;
        let mut p = red / f.powf(self.n as f64 + t) + sc * self.dc_sum / f.powf(t);
        for k in 1..=self.n { p += sc / f.powf(k as f64 + t); }
        p - sc * self.a_sum
    }
}

fn odd_first(a: &[Value]) -> Result<OddFirst, ErrKind> {
    let v = nums(a, 4, &[])?; let (s, m, i, fc) = (v[0].trunc(), v[1].trunc(), v[2].trunc(), v[3].trunc());
    let (freq, bas) = freq_basis(&a[7], a.get(8))?;
    if !(i < s && s < fc && fc < m) { return Err(ErrKind::Num); }
    let at = schedule(fc, freq);
    let (mut dc, mut ac, mut k) = (0.0, 0.0, 0i64);
    let mut j = 1;
    loop {
        let (start, end) = (at(-j), at(1 - j)); let nl = period_len(start, end, bas, freq); let from = i.max(start);
        dc += basis_days(from, end, bas) / nl;
        if s > from { ac += basis_days(from, s.min(end), bas) / nl; }
        if start < s && s <= end { k = j - 1; }
        if start <= i { break; }
        if j > 4000 { return Err(ErrKind::Num); }
        j += 1;
    }
    let (qs, qe) = (at(-k - 1), at(-k)); let e = period_len(qs, qe, bas, freq);
    let dsc = if bas == 0 || bas == 4 { e - basis_days(qs, s, bas) } else { qe - s };
    let reg = coupon_of(fc, m, freq, bas)?;
    Ok(OddFirst { freq, dc_sum: dc, a_sum: ac, nq: k as f64, dsc, e, n: reg.num })
}

fn odd_last(a: &[Value]) -> Result<(f64, f64, f64, f64), ErrKind> {
    let v = nums(a, 3, &[])?; let (s, m, li) = (v[0].trunc(), v[1].trunc(), v[2].trunc());
    let (freq, bas) = freq_basis(&a[6], a.get(7))?;
    if !(li < s && s < m) { return Err(ErrKind::Num); }
    let at = schedule(li, freq);
    let (mut dc, mut ac, mut dsc) = (0.0, 0.0, 0.0);
    let mut j = 1;
    while at(j - 1) < m {
        let (start, end) = (at(j - 1), at(j)); let nl = period_len(start, end, bas, freq);
        dc += basis_days(start, end.min(m), bas) / nl;
        if s > start { ac += basis_days(start, end.min(s), bas) / nl; }
        if s < end && s.max(start) < m { dsc += basis_days(s.max(start), end.min(m), bas) / nl; }
        if j > 4000 { return Err(ErrKind::Num); }
        j += 1;
    }
    Ok((freq, dc, ac, dsc))
}
