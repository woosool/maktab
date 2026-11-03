//! Matrices and the remaining math: MMULT, MINVERSE, MDETERM, MUNIT, SUMX2MY2…, SERIESSUM,
//! MULTINOMIAL, COMBINA, PERMUTATIONA, FACTDOUBLE, the reciprocal/hyperbolic trigonometry.

use super::*;

pub const DEFS: &[Def] = &[
    e("MMULT", 2, 2, mmult), e("MINVERSE", 1, 1, minverse), e("MDETERM", 1, 1, mdeterm),
    s("MUNIT", 1, 1, |a| { let k = n!(a[0]).trunc(); if !(1.0..=300.0).contains(&k) { return err(ErrKind::Value); } let k = k as usize; Value::array(Matrix::new(k, k, (0..k * k).map(|i| Value::Num((i / k == i % k) as u8 as f64)).collect())) }),
    e("SUMX2MY2", 2, 2, |c, a| pairs(c, a, |x, y| x * x - y * y)), e("SUMX2PY2", 2, 2, |c, a| pairs(c, a, |x, y| x * x + y * y)),
    e("SUMXMY2", 2, 2, |c, a| pairs(c, a, |x, y| (x - y) * (x - y))),
    e("SERIESSUM", 4, 4, seriessum),
    e("MULTINOMIAL", 1, MANY, |c, a| { let l = tryv!(numbers(c, a)); if l.iter().any(|x| *x < 0.0) { return err(ErrKind::Num); } let (mut tot, mut out) = (0.0, 1.0); for x in l { let k = x.trunc(); tot += k; out *= choose(tot, k); } num(out) }),
    s("COMBINA", 2, 2, |a| { let (nn, k) = (n!(a[0]).trunc(), n!(a[1]).trunc()); if nn < 0.0 || k < 0.0 || (nn == 0.0 && k > 0.0) { return err(ErrKind::Num); } if k == 0.0 { return num(1.0); } num(choose(nn + k - 1.0, k)) }),
    s("PERMUTATIONA", 2, 2, |a| { let (nn, k) = (n!(a[0]).trunc(), n!(a[1]).trunc()); if nn < 0.0 || k < 0.0 { return err(ErrKind::Num); } num(nn.powf(k)) }),
    s("FACTDOUBLE", 1, 1, |a| { let x = n!(a[0]); if x < 0.0 || x.trunc() > 300.0 { return err(ErrKind::Num); } let mut i = x.trunc(); let mut o = 1.0; while i > 1.0 { o *= i; i -= 2.0; } num(o) }),
    s("ISO.CEILING", 1, 2, |a| { let x = n!(a[0]); let m = tryv!(opt(a, 1, 1.0)).abs(); if m == 0.0 { return num(0.0); } num(snap((x / m).ceil()) * m) }),
    s("ACOSH", 1, 1, |a| { let x = n!(a[0]); if x < 1.0 { err(ErrKind::Num) } else { num(x.acosh()) } }),
    s("ASINH", 1, 1, |a| num(n!(a[0]).asinh())),
    s("ATANH", 1, 1, |a| { let x = n!(a[0]); if x.abs() >= 1.0 { err(ErrKind::Num) } else { num(x.atanh()) } }),
    s("ACOT", 1, 1, |a| num(std::f64::consts::FRAC_PI_2 - n!(a[0]).atan())),
    s("ACOTH", 1, 1, |a| { let x = n!(a[0]); if x.abs() <= 1.0 { err(ErrKind::Num) } else { num((1.0 / x).atanh()) } }),
    s("COT", 1, 1, |a| recip(n!(a[0]), f64::tan)), s("CSC", 1, 1, |a| recip(n!(a[0]), f64::sin)), s("COTH", 1, 1, |a| recip(n!(a[0]), f64::tanh)),
    s("CSCH", 1, 1, |a| recip(n!(a[0]), f64::sinh)),
    s("SEC", 1, 1, |a| num(1.0 / n!(a[0]).cos())), s("SECH", 1, 1, |a| num(1.0 / n!(a[0]).cosh())),
];

/// 1/f(x), where x = 0 is #DIV/0! as 1/0 is.
fn recip(x: f64, f: fn(f64) -> f64) -> Value { if x == 0.0 { err(ErrKind::Div0) } else { num(1.0 / f(x)) } }
fn snap(x: f64) -> f64 { let r = x.round(); if (x - r).abs() < 1e-9 { r } else { x } }
/// C(n, k) as a float product (large results overflow to #NUM! through `num`).
fn choose(n: f64, k: f64) -> f64 { let k = k.min(n - k); if k < 0.0 { return 0.0; } let mut o = 1.0; let mut i = 0.0; while i < k { o = o * (n - i) / (i + 1.0); i += 1.0; } o.round() }

/// An argument as a matrix of numbers: any non-number (blank, text, logical) is #VALUE!.
pub fn num_matrix(ctx: &Ctx, a: &Arg) -> Result<(usize, usize, Vec<f64>), ErrKind> {
    let m = ctx.matrix_of(a);
    let mut out = Vec::with_capacity(m.data.len());
    for v in &m.data { match v { Value::Num(n) => out.push(*n), Value::Err(e) => return Err(*e), _ => return Err(ErrKind::Value) } }
    Ok((m.rows, m.cols, out))
}
fn to_value(r: usize, c: usize, d: Vec<f64>) -> Value { Value::array(Matrix::new(r, c, d.into_iter().map(num).collect())) }

fn mmult(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let (ar, ac, x) = tryv!(num_matrix(ctx, &a[0])); let (br, bc, y) = tryv!(num_matrix(ctx, &a[1]));
    if ac != br { return err(ErrKind::Value); }
    if ar.saturating_mul(bc) > 1_000_000 { return err(ErrKind::Num); }
    let mut out = vec![0.0; ar * bc];
    for i in 0..ar { for j in 0..bc { out[i * bc + j] = (0..ac).map(|k| x[i * ac + k] * y[k * bc + j]).sum(); } }
    to_value(ar, bc, out)
}

/// Gauss-Jordan inversion with partial pivoting; None when singular.
pub fn invert(n: usize, m: &[f64]) -> Option<Vec<f64>> {
    let mut a = m.to_vec(); let mut inv: Vec<f64> = (0..n * n).map(|i| (i / n == i % n) as u8 as f64).collect();
    let scale = m.iter().fold(0.0f64, |s, x| s.max(x.abs())).max(1e-300);
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i * n + c].abs().total_cmp(&a[j * n + c].abs()))?;
        if a[p * n + c].abs() <= 1e-15 * scale { return None; }
        for k in 0..n { a.swap(c * n + k, p * n + k); inv.swap(c * n + k, p * n + k); }
        let d = a[c * n + c];
        for k in 0..n { a[c * n + k] /= d; inv[c * n + k] /= d; }
        for r in 0..n { if r != c { let f = a[r * n + c]; if f != 0.0 { for k in 0..n { a[r * n + k] -= f * a[c * n + k]; inv[r * n + k] -= f * inv[c * n + k]; } } } }
    }
    Some(inv)
}

/// Determinant by LU decomposition with partial pivoting.
pub fn determinant(n: usize, m: &[f64]) -> f64 {
    let mut a = m.to_vec(); let mut det = 1.0;
    for c in 0..n {
        let Some(p) = (c..n).max_by(|&i, &j| a[i * n + c].abs().total_cmp(&a[j * n + c].abs())) else { return 0.0 };
        if a[p * n + c] == 0.0 { return 0.0; }
        if p != c { for k in 0..n { a.swap(c * n + k, p * n + k); } det = -det; }
        let d = a[c * n + c]; det *= d;
        for r in c + 1..n { let f = a[r * n + c] / d; for k in c..n { a[r * n + k] -= f * a[c * n + k]; } }
    }
    det
}

fn minverse(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let (r, c, m) = tryv!(num_matrix(ctx, &a[0]));
    if r != c { return err(ErrKind::Value); }
    if r > 1000 { return err(ErrKind::Num); }
    match invert(r, &m) { Some(i) => to_value(r, r, i), None => err(ErrKind::Num) }
}
fn mdeterm(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let (r, c, m) = tryv!(num_matrix(ctx, &a[0]));
    if r != c { return err(ErrKind::Value); }
    if r > 1000 { return err(ErrKind::Num); }
    num(determinant(r, &m))
}

/// Pairs of numbers at the same place in two equal-sized arrays (pairs with a non-number skipped).
fn pairs(ctx: &mut Ctx, a: &[Arg], f: fn(f64, f64) -> f64) -> Value {
    let (x, y) = (ctx.matrix_of(&a[0]), ctx.matrix_of(&a[1]));
    if x.data.len() != y.data.len() { return err(ErrKind::NA); }
    let mut s = 0.0;
    for (p, q) in x.data.iter().zip(&y.data) {
        if let Value::Err(e) = p { return err(*e); } if let Value::Err(e) = q { return err(*e); }
        if let (Value::Num(p), Value::Num(q)) = (p, q) { s += f(*p, *q); }
    }
    num(s)
}

fn seriessum(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let x = tryv!(arg_num(ctx, a, 0, 0.0)); let nn = tryv!(arg_num(ctx, a, 1, 0.0)); let m = tryv!(arg_num(ctx, a, 2, 0.0));
    let cs: Vec<f64> = match &a[3] {
        Arg::V(v) if !matches!(v, Value::Array(_)) => vec![tryv!(v.to_num())],
        arg => { let mut o = vec![]; for v in &ctx.matrix_of(arg).data { match v { Value::Num(n) => o.push(*n), Value::Err(e) => return err(*e), _ => return err(ErrKind::Value) } } o }
    };
    num(cs.iter().enumerate().map(|(i, c)| c * x.powf(nn + i as f64 * m)).sum())
}
