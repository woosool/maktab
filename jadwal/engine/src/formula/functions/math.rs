//! Math and basic statistics: SUM, AVERAGE, COUNT, ROUND, the IF/IFS aggregates, SUMPRODUCT…

use super::criteria::Criteria;
use super::*;

pub const DEFS: &[Def] = &[
    e("SUM", 1, MANY, sum), e("AVERAGE", 1, MANY, average), e("MIN", 1, MANY, min), e("MAX", 1, MANY, max),
    e("COUNT", 1, MANY, count), e("COUNTA", 1, MANY, counta), e("COUNTBLANK", 1, 1, countblank), e("PRODUCT", 1, MANY, product),
    e("AVERAGEA", 1, MANY, averagea), e("MINA", 1, MANY, mina), e("MAXA", 1, MANY, maxa), e("SUMSQ", 1, MANY, sumsq),
    e("SUMPRODUCT", 1, MANY, sumproduct),
    e("SUMIF", 2, 3, sumif), e("COUNTIF", 2, 2, countif), e("AVERAGEIF", 2, 3, averageif),
    e("SUMIFS", 3, MANY, sumifs), e("COUNTIFS", 2, MANY, countifs), e("AVERAGEIFS", 3, MANY, averageifs), e("MAXIFS", 3, MANY, maxifs), e("MINIFS", 3, MANY, minifs),
    e("SUBTOTAL", 2, MANY, subtotal), e("MEDIAN", 1, MANY, median), e("MODE", 1, MANY, mode), e("MODE.SNGL", 1, MANY, mode),
    e("STDEV", 1, MANY, stdev_s), e("STDEV.S", 1, MANY, stdev_s), e("STDEVP", 1, MANY, stdev_p), e("STDEV.P", 1, MANY, stdev_p),
    e("VAR", 1, MANY, var_s), e("VAR.S", 1, MANY, var_s), e("VARP", 1, MANY, var_p), e("VAR.P", 1, MANY, var_p),
    e("LARGE", 2, 2, large), e("SMALL", 2, 2, small), e("RANK", 2, 3, rank), e("RANK.EQ", 2, 3, rank),
    s("ABS", 1, 1, |a| num(n!(a[0]).abs())), s("SIGN", 1, 1, |a| num(n!(a[0]).signum() * (n!(a[0]) != 0.0) as i32 as f64)),
    s("INT", 1, 1, |a| num(n!(a[0]).floor())), s("SQRT", 1, 1, |a| { let x = n!(a[0]); if x < 0.0 { err(ErrKind::Num) } else { num(x.sqrt()) } }),
    s("EXP", 1, 1, |a| num(n!(a[0]).exp())), s("LN", 1, 1, |a| { let x = n!(a[0]); if x <= 0.0 { err(ErrKind::Num) } else { num(x.ln()) } }),
    s("LOG10", 1, 1, |a| { let x = n!(a[0]); if x <= 0.0 { err(ErrKind::Num) } else { num(x.log10()) } }),
    s("LOG", 1, 2, |a| { let x = n!(a[0]); let b = tryv!(opt(a, 1, 10.0)); if x <= 0.0 || b <= 0.0 || b == 1.0 { err(ErrKind::Num) } else { num(x.ln() / b.ln()) } }),
    s("POWER", 2, 2, |a| super::super::eval::binary(super::super::parser::BinOp::Pow, &a[0], &a[1])),
    s("MOD", 2, 2, |a| { let (x, d) = (n!(a[0]), n!(a[1])); if d == 0.0 { err(ErrKind::Div0) } else { num(x - d * (x / d).floor()) } }),
    s("QUOTIENT", 2, 2, |a| { let (x, d) = (n!(a[0]), n!(a[1])); if d == 0.0 { err(ErrKind::Div0) } else { num((x / d).trunc()) } }),
    s("PI", 0, 0, |_| num(std::f64::consts::PI)),
    s("ROUND", 2, 2, |a| num(round_to(n!(a[0]), n!(a[1]).trunc() as i32))),
    s("ROUNDUP", 2, 2, |a| num(round_dir(n!(a[0]), n!(a[1]).trunc() as i32, true))),
    s("ROUNDDOWN", 2, 2, |a| num(round_dir(n!(a[0]), n!(a[1]).trunc() as i32, false))),
    s("TRUNC", 1, 2, |a| num(round_dir(n!(a[0]), tryv!(opt(a, 1, 0.0)).trunc() as i32, false))),
    s("MROUND", 2, 2, |a| { let (x, m) = (n!(a[0]), n!(a[1])); if m == 0.0 { return num(0.0); } if x * m < 0.0 { return err(ErrKind::Num); } num(round_to(x / m, 0) * m) }),
    s("CEILING", 1, 2, |a| { let x = n!(a[0]); let m = tryv!(opt(a, 1, 1.0)); if m == 0.0 { return num(0.0); } if x > 0.0 && m < 0.0 { return err(ErrKind::Num); } num(snap((x / m).ceil()) * m) }),
    s("FLOOR", 1, 2, |a| { let x = n!(a[0]); let m = tryv!(opt(a, 1, 1.0)); if m == 0.0 { return err(ErrKind::Div0); } if x > 0.0 && m < 0.0 { return err(ErrKind::Num); } num(snap((x / m).floor()) * m) }),
    s("CEILING.MATH", 1, 3, |a| { let x = n!(a[0]); let m = tryv!(opt(a, 1, 1.0)).abs(); let mode = tryv!(opt(a, 2, 0.0)); if m == 0.0 { return num(0.0); } num(if x < 0.0 && mode != 0.0 { -snap((-x / m).ceil()) * m } else { snap((x / m).ceil()) * m }) }),
    s("FLOOR.MATH", 1, 3, |a| { let x = n!(a[0]); let m = tryv!(opt(a, 1, 1.0)).abs(); let mode = tryv!(opt(a, 2, 0.0)); if m == 0.0 { return num(0.0); } num(if x < 0.0 && mode != 0.0 { -snap((-x / m).floor()) * m } else { snap((x / m).floor()) * m }) }),
    s("CEILING.PRECISE", 1, 2, |a| { let x = n!(a[0]); let m = tryv!(opt(a, 1, 1.0)).abs(); if m == 0.0 { return num(0.0); } num(snap((x / m).ceil()) * m) }),
    s("FLOOR.PRECISE", 1, 2, |a| { let x = n!(a[0]); let m = tryv!(opt(a, 1, 1.0)).abs(); if m == 0.0 { return num(0.0); } num(snap((x / m).floor()) * m) }),
    s("EVEN", 1, 1, |a| { let x = n!(a[0]); let r = (x.abs() / 2.0).ceil() * 2.0; num(r * x.signum()) }),
    s("ODD", 1, 1, |a| { let x = n!(a[0]); let mut r = x.abs().ceil(); if r % 2.0 == 0.0 { r += 1.0; } num(r * if x < 0.0 { -1.0 } else { 1.0 }) }),
    s("FACT", 1, 1, |a| { let x = n!(a[0]).trunc(); if x < 0.0 { return err(ErrKind::Num); } num((1..=x as u64).fold(1.0, |p, k| p * k as f64)) }),
    s("COMBIN", 2, 2, |a| { let (nn, k) = (n!(a[0]).trunc(), n!(a[1]).trunc()); if nn < 0.0 || k < 0.0 || k > nn { return err(ErrKind::Num); } num(combin(nn, k)) }),
    s("PERMUT", 2, 2, |a| { let (nn, k) = (n!(a[0]).trunc(), n!(a[1]).trunc()); if nn < 0.0 || k < 0.0 || k > nn { return err(ErrKind::Num); } num((0..k as u64).fold(1.0, |p, i| p * (nn - i as f64))) }),
    s("GCD", 1, MANY, |a| { let mut g = 0u64; for v in a { let x = n!(v); if x < 0.0 { return err(ErrKind::Num); } g = gcd(g, x.trunc() as u64); } num(g as f64) }),
    s("LCM", 1, MANY, |a| { let mut l = 1u64; for v in a { let x = n!(v); if x < 0.0 { return err(ErrKind::Num); } let x = x.trunc() as u64; if x == 0 { return num(0.0); } l = l / gcd(l, x) * x; } num(l as f64) }),
    s("RAND", 0, 0, |_| num(rand())),
    s("RANDBETWEEN", 2, 2, |a| { let (lo, hi) = (n!(a[0]).ceil(), n!(a[1]).floor()); if lo > hi { return err(ErrKind::Num); } num((lo + (rand() * (hi - lo + 1.0)).floor()).min(hi)) }),
    s("SIN", 1, 1, |a| num(n!(a[0]).sin())), s("COS", 1, 1, |a| num(n!(a[0]).cos())), s("TAN", 1, 1, |a| num(n!(a[0]).tan())),
    s("ASIN", 1, 1, |a| { let x = n!(a[0]); if x.abs() > 1.0 { err(ErrKind::Num) } else { num(x.asin()) } }),
    s("ACOS", 1, 1, |a| { let x = n!(a[0]); if x.abs() > 1.0 { err(ErrKind::Num) } else { num(x.acos()) } }),
    s("ATAN", 1, 1, |a| num(n!(a[0]).atan())),
    s("ATAN2", 2, 2, |a| { let (x, y) = (n!(a[0]), n!(a[1])); if x == 0.0 && y == 0.0 { err(ErrKind::Div0) } else { num(y.atan2(x)) } }),
    s("SINH", 1, 1, |a| num(n!(a[0]).sinh())), s("COSH", 1, 1, |a| num(n!(a[0]).cosh())), s("TANH", 1, 1, |a| num(n!(a[0]).tanh())),
    s("DEGREES", 1, 1, |a| num(n!(a[0]).to_degrees())), s("RADIANS", 1, 1, |a| num(n!(a[0]).to_radians())),
    s("SQRTPI", 1, 1, |a| { let x = n!(a[0]); if x < 0.0 { err(ErrKind::Num) } else { num((x * std::f64::consts::PI).sqrt()) } }),
];

/// Removes the floating-point dust of a division before ceil/floor (2.0000000000000004 → 2).
fn snap(x: f64) -> f64 { let r = x.round(); if (x - r).abs() < 1e-9 { r } else { x } }

pub fn round_to(x: f64, d: i32) -> f64 {
    let f = 10f64.powi(d.abs());
    // Round half away from zero on the decimal digits as typed (1.005 → 1.01).
    let fix = |y: f64| { let s = format!("{:.12e}", y); s.parse::<f64>().unwrap_or(y) };
    if d >= 0 { (fix(x * f)).round() / f } else { (fix(x / f)).round() * f }
}

fn round_dir(x: f64, d: i32, up: bool) -> f64 {
    let f = 10f64.powi(d.abs());
    let y = if d >= 0 { snap_digits(x * f) } else { snap_digits(x / f) };
    let r = if up { y.abs().ceil() * y.signum() } else { y.trunc() };
    if d >= 0 { r / f } else { r * f }
}
fn snap_digits(y: f64) -> f64 { let r = y.round(); if (y - r).abs() < 1e-9 * y.abs().max(1.0) { r } else { y } }

pub fn combin(n: f64, k: f64) -> f64 { let k = k.min(n - k); (0..k as u64).fold(1.0, |p, i| p * (n - i as f64) / (i as f64 + 1.0)).round() }
fn gcd(a: u64, b: u64) -> u64 { if b == 0 { a } else { gcd(b, a % b) } }

pub fn rand() -> f64 {
    use std::cell::Cell;
    thread_local!(static S: Cell<u64> = Cell::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(42) | 1));
    S.with(|s| { let mut x = s.get(); x ^= x << 13; x ^= x >> 7; x ^= x << 17; s.set(x); (x >> 11) as f64 / (1u64 << 53) as f64 })
}

fn sum(ctx: &mut Ctx, a: &[Arg]) -> Value { num(tryv!(numbers(ctx, a)).iter().sum()) }
fn sumsq(ctx: &mut Ctx, a: &[Arg]) -> Value { num(tryv!(numbers(ctx, a)).iter().map(|x| x * x).sum()) }
fn product(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(numbers(ctx, a)); num(if v.is_empty() { 0.0 } else { v.iter().product() }) }
fn average(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(numbers(ctx, a)); if v.is_empty() { err(ErrKind::Div0) } else { num(v.iter().sum::<f64>() / v.len() as f64) } }
fn averagea(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(numbers_a(ctx, a)); if v.is_empty() { err(ErrKind::Div0) } else { num(v.iter().sum::<f64>() / v.len() as f64) } }
fn min(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(numbers(ctx, a)); num(v.iter().copied().fold(f64::INFINITY, f64::min).min(if v.is_empty() { 0.0 } else { f64::INFINITY })) }
fn max(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(numbers(ctx, a)); num(v.iter().copied().fold(f64::NEG_INFINITY, f64::max).max(if v.is_empty() { 0.0 } else { f64::NEG_INFINITY })) }
fn mina(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(numbers_a(ctx, a)); if v.is_empty() { num(0.0) } else { num(v.iter().copied().fold(f64::INFINITY, f64::min)) } }
fn maxa(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(numbers_a(ctx, a)); if v.is_empty() { num(0.0) } else { num(v.iter().copied().fold(f64::NEG_INFINITY, f64::max)) } }

fn count(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let mut k = 0;
    for x in a { each(ctx, x, &mut |v, direct| { if matches!(v, Value::Num(_)) || (direct && (matches!(v, Value::Bool(_)) || (matches!(v, Value::Str(_)) && v.to_num().is_ok()))) { k += 1; } }); }
    num(k as f64)
}
fn counta(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let mut k = 0;
    for x in a { if x.is_missing() { k += 1; continue; } each(ctx, x, &mut |v, _| if !v.is_empty() { k += 1 }); }
    num(k as f64)
}
fn countblank(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let vals = flat(ctx, &a[0]);
    num(vals.iter().filter(|v| v.is_empty() || matches!(v, Value::Str(s) if s.is_empty())).count() as f64)
}

/// Pairs every cell of `range` with the cell at the same offset in `other` (as SUMIF's sum range).
pub(super) fn criteria_rows(ctx: &Ctx, ranges: &[(&Arg, Criteria)]) -> Result<Vec<bool>, ErrKind> {
    let mut hits: Option<Vec<bool>> = None;
    let mut shape = None;
    for (a, c) in ranges {
        let m = ctx.matrix_of(a);
        if let Some(s) = shape { if s != (m.rows, m.cols) { return Err(ErrKind::Value); } } else { shape = Some((m.rows, m.cols)); }
        let h: Vec<bool> = m.data.iter().map(|v| c.test(v)).collect();
        hits = Some(match hits { None => h, Some(prev) => prev.iter().zip(h).map(|(x, y)| *x && y).collect() });
    }
    Ok(hits.unwrap_or_default())
}

/// The range whose values are summed: the same shape as the criteria range, from its corner.
fn same_shape(ctx: &Ctx, target: &Arg, like: &Arg) -> Matrix {
    if let (Arg::R(s, t), Arg::R(_, l)) = (target, like) {
        let l = ctx.clip(*s, *l);
        let r = crate::addr::Range { r0: t.r0, c0: t.c0, r1: (t.r0 + l.rows() - 1).min(crate::addr::MAX_ROWS - 1), c1: (t.c0 + l.cols() - 1).min(crate::addr::MAX_COLS - 1) };
        return ctx.range_matrix(*s, r);
    }
    ctx.matrix_of(target)
}

pub(super) fn crit_arg(ctx: &Ctx, a: &Arg) -> Criteria { Criteria::new(&ctx.scalar_of(a)) }

fn sumif(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let c = crit_arg(ctx, &a[1]);
    let hits = tryv!(criteria_rows(ctx, &[(&a[0], c)]));
    let vals = if a.len() > 2 && !a[2].is_missing() { same_shape(ctx, &a[2], &a[0]) } else { ctx.matrix_of(&a[0]) };
    let mut t = 0.0;
    for (i, h) in hits.iter().enumerate() { if *h { match vals.data.get(i) { Some(Value::Num(n)) => t += n, Some(Value::Err(e)) => return err(*e), _ => {} } } }
    num(t)
}
fn countif(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let c = crit_arg(ctx, &a[1]);
    let hits = tryv!(criteria_rows(ctx, &[(&a[0], c)]));
    num(hits.iter().filter(|h| **h).count() as f64)
}
fn averageif(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let c = crit_arg(ctx, &a[1]);
    let hits = tryv!(criteria_rows(ctx, &[(&a[0], c)]));
    let vals = if a.len() > 2 && !a[2].is_missing() { same_shape(ctx, &a[2], &a[0]) } else { ctx.matrix_of(&a[0]) };
    let xs: Vec<f64> = hits.iter().enumerate().filter(|(_, h)| **h).filter_map(|(i, _)| if let Some(Value::Num(n)) = vals.data.get(i) { Some(*n) } else { None }).collect();
    if xs.is_empty() { err(ErrKind::Div0) } else { num(xs.iter().sum::<f64>() / xs.len() as f64) }
}
fn ifs_values(ctx: &Ctx, a: &[Arg]) -> Result<Vec<f64>, ErrKind> {
    if a.len() % 2 == 0 { return Err(ErrKind::Value); }
    let pairs: Vec<(&Arg, Criteria)> = a[1..].chunks(2).map(|p| (&p[0], crit_arg(ctx, &p[1]))).collect();
    let hits = criteria_rows(ctx, &pairs)?;
    let vals = ctx.matrix_of(&a[0]);
    if vals.data.len() != hits.len() { return Err(ErrKind::Value); }
    let mut out = vec![];
    for (i, h) in hits.iter().enumerate() { if *h { match &vals.data[i] { Value::Num(n) => out.push(*n), Value::Err(e) => return Err(*e), _ => {} } } }
    Ok(out)
}
fn sumifs(ctx: &mut Ctx, a: &[Arg]) -> Value { num(tryv!(ifs_values(ctx, a)).iter().sum()) }
fn averageifs(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(ifs_values(ctx, a)); if v.is_empty() { err(ErrKind::Div0) } else { num(v.iter().sum::<f64>() / v.len() as f64) } }
fn maxifs(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(ifs_values(ctx, a)); num(v.iter().copied().fold(f64::NEG_INFINITY, f64::max).max(if v.is_empty() { 0.0 } else { f64::NEG_INFINITY })) }
fn minifs(ctx: &mut Ctx, a: &[Arg]) -> Value { let v = tryv!(ifs_values(ctx, a)); num(v.iter().copied().fold(f64::INFINITY, f64::min).min(if v.is_empty() { 0.0 } else { f64::INFINITY })) }
fn countifs(ctx: &mut Ctx, a: &[Arg]) -> Value {
    if a.len() % 2 != 0 { return err(ErrKind::Value); }
    let pairs: Vec<(&Arg, Criteria)> = a.chunks(2).map(|p| (&p[0], crit_arg(ctx, &p[1]))).collect();
    num(tryv!(criteria_rows(ctx, &pairs)).iter().filter(|h| **h).count() as f64)
}

fn sumproduct(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let ms: Vec<Matrix> = a.iter().map(|x| ctx.matrix_of(x)).collect();
    let (r, c) = (ms[0].rows, ms[0].cols);
    if ms.iter().any(|m| m.rows != r || m.cols != c) { return err(ErrKind::Value); }
    let mut t = 0.0;
    for i in 0..r * c {
        let mut p = 1.0;
        for m in &ms { match &m.data[i] { Value::Num(n) => p *= n, Value::Err(e) => return err(*e), _ => p = 0.0 } }
        t += p;
    }
    num(t)
}

fn sorted(ctx: &Ctx, a: &[Arg]) -> Result<Vec<f64>, ErrKind> { let mut v = numbers(ctx, a)?; v.sort_by(|x, y| x.partial_cmp(y).unwrap()); Ok(v) }
fn median(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let v = tryv!(sorted(ctx, a));
    if v.is_empty() { return err(ErrKind::Num); }
    let m = v.len() / 2;
    num(if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 })
}
fn mode(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let v = tryv!(numbers(ctx, a));
    let mut best: Option<(f64, usize)> = None;
    for (i, x) in v.iter().enumerate() {
        let k = v.iter().filter(|y| *y == x).count();
        if k > 1 && best.is_none_or(|b| k > b.1) && v[..i].iter().all(|y| y != x) { best = Some((*x, k)); }
    }
    best.map_or(err(ErrKind::NA), |b| num(b.0))
}
pub fn variance(v: &[f64], sample: bool) -> Option<f64> {
    let n = v.len() as f64;
    if v.is_empty() || (sample && v.len() < 2) { return None; }
    let m = v.iter().sum::<f64>() / n;
    Some(v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / if sample { n - 1.0 } else { n })
}
fn var_s(ctx: &mut Ctx, a: &[Arg]) -> Value { variance(&tryv!(numbers(ctx, a)), true).map_or(err(ErrKind::Div0), num) }
fn var_p(ctx: &mut Ctx, a: &[Arg]) -> Value { variance(&tryv!(numbers(ctx, a)), false).map_or(err(ErrKind::Div0), num) }
fn stdev_s(ctx: &mut Ctx, a: &[Arg]) -> Value { variance(&tryv!(numbers(ctx, a)), true).map_or(err(ErrKind::Div0), |x| num(x.sqrt())) }
fn stdev_p(ctx: &mut Ctx, a: &[Arg]) -> Value { variance(&tryv!(numbers(ctx, a)), false).map_or(err(ErrKind::Div0), |x| num(x.sqrt())) }
fn large(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let v = tryv!(sorted(ctx, &a[..1]));
    let k = tryv!(arg_num(ctx, a, 1, 0.0)).ceil() as usize;
    if k == 0 || k > v.len() { err(ErrKind::Num) } else { num(v[v.len() - k]) }
}
fn small(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let v = tryv!(sorted(ctx, &a[..1]));
    let k = tryv!(arg_num(ctx, a, 1, 0.0)).ceil() as usize;
    if k == 0 || k > v.len() { err(ErrKind::Num) } else { num(v[k - 1]) }
}
fn rank(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let x = tryv!(arg_num(ctx, a, 0, 0.0));
    let v = tryv!(numbers(ctx, &a[1..2]));
    let asc = tryv!(arg_num(ctx, a, 2, 0.0)) != 0.0;
    if !v.contains(&x) { return err(ErrKind::NA); }
    num(1.0 + v.iter().filter(|y| if asc { **y < x } else { **y > x }).count() as f64)
}

/// SUBTOTAL(function_num, ranges…): 1-11 count hidden rows, 101-111 skip them; both skip filtered rows.
fn subtotal(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let f = tryv!(arg_num(ctx, a, 0, 0.0)) as i64;
    let skip_hidden = f > 100;
    // Cells that are themselves SUBTOTALs are left out so totals don't count twice.
    let refs: Vec<Arg> = a[1..].iter().map(|x| match x {
        Arg::R(s, r) => {
            let sh = &ctx.book.sheets[*s];
            let r2 = ctx.clip(*s, *r);
            let vals: Vec<Value> = r2.iter().filter(|p| !(skip_hidden && sh.hidden_rows.contains(&p.row)) && !sh.filtered_rows.contains(&p.row))
                .filter(|p| !sh.cells.get(p).and_then(|c| c.formula()).is_some_and(|t| t.to_ascii_uppercase().contains("SUBTOTAL(")))
                .map(|p| ctx.cell(*s, p)).collect();
            let n = vals.len();
            Arg::V(Value::array(Matrix::new(1, n.max(1), if n == 0 { vec![Value::Empty] } else { vals })))
        }
        o => o.clone(),
    }).collect();
    match f % 100 {
        1 => average(ctx, &refs), 2 => count(ctx, &refs), 3 => counta(ctx, &refs), 4 => max(ctx, &refs), 5 => min(ctx, &refs), 6 => product(ctx, &refs),
        7 => stdev_s(ctx, &refs), 8 => stdev_p(ctx, &refs), 9 => sum(ctx, &refs), 10 => var_s(ctx, &refs), 11 => var_p(ctx, &refs), _ => err(ErrKind::Value),
    }
}
