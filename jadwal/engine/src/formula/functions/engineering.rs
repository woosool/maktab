//! Engineering: base conversions, bit operations, CONVERT, ERF, Bessel functions, complex
//! numbers (the IM* family) and ROMAN/ARABIC, ported from functions.js.

use super::*;

pub const DEFS: &[Def] = &[
    s("ERF", 1, 2, |a| { let x = n!(a[0]); if a.len() < 2 { return num(1.0 - erfc(x)); } let y = n!(a[1]); num((1.0 - erfc(y)) - (1.0 - erfc(x))) }),
    s("ERF.PRECISE", 1, 1, |a| num(1.0 - erfc(n!(a[0])))),
    s("ERFC", 1, 1, |a| num(erfc(n!(a[0])))), s("ERFC.PRECISE", 1, 1, |a| num(erfc(n!(a[0])))),
    s("COMPLEX", 2, 3, |a| { let (re, im) = (n!(a[0]), n!(a[1])); let sfx = if a.len() > 2 && !a[2].is_empty() { t!(a[2]) } else { "i".into() }; if sfx != "i" && sfx != "j" { return err(ErrKind::Value); } fmt_c(C(re, im), Some(sfx.chars().next().unwrap_or('i')), true) }),
    s("IMREAL", 1, 1, |a| num(tryv!(parse_c(&a[0])).0 .0)), s("IMAGINARY", 1, 1, |a| num(tryv!(parse_c(&a[0])).0 .1)),
    s("IMABS", 1, 1, |a| { let z = tryv!(parse_c(&a[0])).0; num(z.0.hypot(z.1)) }),
    s("IMARGUMENT", 1, 1, |a| { let z = tryv!(parse_c(&a[0])).0; if z.0 == 0.0 && z.1 == 0.0 { err(ErrKind::Div0) } else { num(z.1.atan2(z.0)) } }),
    s("IMCONJUGATE", 1, 1, |a| un(a, |z| Ok(C(z.0, -z.1)))),
    e("IMSUM", 1, MANY, |c, a| fold_c(c, a, C(0.0, 0.0), true, |x, y| C(x.0 + y.0, x.1 + y.1))),
    e("IMPRODUCT", 1, MANY, |c, a| fold_c(c, a, C(1.0, 0.0), false, cmul)),
    s("IMSUB", 2, 2, |a| bin(a, true, |x, y| Ok(C(x.0 - y.0, x.1 - y.1)))),
    s("IMDIV", 2, 2, |a| bin(a, false, cdiv)),
    s("IMPOWER", 2, 2, |a| { let (z, sf) = tryv!(parse_c(&a[0])); let p = n!(a[1]); fmt_c(tryv!(cpow(z, p)), sf, false) }),
    s("IMSQRT", 1, 1, |a| un(a, |z| cpow(z, 0.5))), s("IMEXP", 1, 1, |a| un(a, |z| Ok(cexp(z)))), s("IMLN", 1, 1, |a| un(a, cln)),
    s("IMLOG10", 1, 1, |a| un(a, |z| cln(z).map(|l| C(l.0 / std::f64::consts::LN_10, l.1 / std::f64::consts::LN_10)))),
    s("IMLOG2", 1, 1, |a| un(a, |z| cln(z).map(|l| C(l.0 / std::f64::consts::LN_2, l.1 / std::f64::consts::LN_2)))),
    s("IMSIN", 1, 1, |a| un(a, |z| Ok(csin(z)))), s("IMCOS", 1, 1, |a| un(a, |z| Ok(ccos(z)))),
    s("IMTAN", 1, 1, |a| un(a, |z| cdiv(csin(z), ccos(z)))), s("IMCOT", 1, 1, |a| un(a, |z| cdiv(ccos(z), csin(z)))),
    s("IMSEC", 1, 1, |a| un(a, |z| cdiv(C(1.0, 0.0), ccos(z)))), s("IMCSC", 1, 1, |a| un(a, |z| cdiv(C(1.0, 0.0), csin(z)))),
    s("IMSINH", 1, 1, |a| un(a, |z| Ok(csinh(z)))), s("IMCOSH", 1, 1, |a| un(a, |z| Ok(ccosh(z)))),
    s("IMSECH", 1, 1, |a| un(a, |z| cdiv(C(1.0, 0.0), ccosh(z)))), s("IMCSCH", 1, 1, |a| un(a, |z| cdiv(C(1.0, 0.0), csinh(z)))),
    s("BESSELJ", 2, 2, |a| { let (x, n) = tryv!(bargs(a, false)); num(bessel_j(n, x)) }),
    s("BESSELY", 2, 2, |a| { let (x, n) = tryv!(bargs(a, true)); num(bessel_y(n, x)) }),
    s("BESSELI", 2, 2, |a| { let (x, n) = tryv!(bargs(a, false)); let v = bessel_i_series(n, x.abs()); num(if x < 0.0 && n % 2 == 1 { -v } else { v }) }),
    s("BESSELK", 2, 2, |a| { let (x, n) = tryv!(bargs(a, true)); num(bessel_k(n, x)) }),
    s("BASE", 2, 3, |a| { let (v, b, m) = (n!(a[0]).trunc(), n!(a[1]).trunc(), tryv!(opt(a, 2, 0.0))); if v < 0.0 || v >= 2f64.powi(53) || !(2.0..=36.0).contains(&b) || !(0.0..256.0).contains(&m) { return err(ErrKind::Num); } Value::str(pad(radix(v as u64, b as u32), m as usize)) }),
    s("DECIMAL", 2, 2, |a| { let t = t!(a[0]); let b = n!(a[1]).trunc(); if !(2.0..=36.0).contains(&b) { return err(ErrKind::Num); } let mut out = 0.0; for ch in t.trim().chars() { match ch.to_digit(36) { Some(d) if (d as f64) < b => out = out * b + d as f64, _ => return err(ErrKind::Num) } } num(out) }),
    s("BIN2DEC", 1, 1, |a| from_base(&a[0], 2).map_or_else(err, num)), s("OCT2DEC", 1, 1, |a| from_base(&a[0], 8).map_or_else(err, num)), s("HEX2DEC", 1, 1, |a| from_base(&a[0], 16).map_or_else(err, num)),
    s("BIN2OCT", 1, 2, |a| conv(a, 2, 8)), s("BIN2HEX", 1, 2, |a| conv(a, 2, 16)), s("OCT2BIN", 1, 2, |a| conv(a, 8, 2)),
    s("OCT2HEX", 1, 2, |a| conv(a, 8, 16)), s("HEX2BIN", 1, 2, |a| conv(a, 16, 2)), s("HEX2OCT", 1, 2, |a| conv(a, 16, 8)),
    s("DEC2BIN", 1, 2, |a| to_base(n!(a[0]), 2, a.get(1))), s("DEC2OCT", 1, 2, |a| to_base(n!(a[0]), 8, a.get(1))), s("DEC2HEX", 1, 2, |a| to_base(n!(a[0]), 16, a.get(1))),
    s("BITAND", 2, 2, |a| bit(a, |x, y| x & y)), s("BITOR", 2, 2, |a| bit(a, |x, y| x | y)), s("BITXOR", 2, 2, |a| bit(a, |x, y| x ^ y)),
    s("BITLSHIFT", 2, 2, |a| shift(n!(a[0]), n!(a[1]))), s("BITRSHIFT", 2, 2, |a| shift(n!(a[0]), -n!(a[1]))),
    s("DELTA", 1, 2, |a| { let (x, y) = (n!(a[0]), tryv!(opt(a, 1, 0.0))); num((x == y) as i32 as f64) }),
    s("GESTEP", 1, 2, |a| { let (x, y) = (n!(a[0]), tryv!(opt(a, 1, 0.0))); num((x >= y) as i32 as f64) }),
    s("ROMAN", 1, 2, roman), s("ARABIC", 1, 1, arabic),
    s("CONVERT", 3, 3, |a| { let x = n!(a[0]); let (f, t) = (t!(a[1]), t!(a[2])); convert(x, f.trim(), t.trim()).map_or(err(ErrKind::NA), num) }),
];

/// erfc to double precision: a positive series below 2.5, a continued fraction beyond.
pub fn erfc(x: f64) -> f64 {
    if x.is_nan() { return x; }
    if x < 0.0 { return 2.0 - erfc(-x); }
    if x > 27.0 { return 0.0; }
    if x < 2.5 {
        let (mut term, mut sum) = (x, x);
        for n in 1..400 { term *= 2.0 * x * x / (2 * n + 1) as f64; sum += term; if term < sum * 1e-17 { break; } }
        return 1.0 - 2.0 / std::f64::consts::PI.sqrt() * (-x * x).exp() * sum;
    }
    let mut f = x;
    for k in (1..=80).rev() { f = x + (k as f64 / 2.0) / f; }
    (-x * x).exp() / (std::f64::consts::PI.sqrt() * f)
}

// ---- complex numbers ----

#[derive(Clone, Copy, Debug)]
struct C(f64, f64);
type Cr = Result<C, ErrKind>;

fn cmul(a: C, b: C) -> C { C(a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0) }
fn cdiv(a: C, b: C) -> Cr { let d = b.0 * b.0 + b.1 * b.1; if d == 0.0 { Err(ErrKind::Num) } else { Ok(C((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)) } }
fn cexp(z: C) -> C { C(z.0.exp() * z.1.cos(), z.0.exp() * z.1.sin()) }
fn cln(z: C) -> Cr { if z.0 == 0.0 && z.1 == 0.0 { Err(ErrKind::Num) } else { Ok(C(z.0.hypot(z.1).ln(), z.1.atan2(z.0))) } }
fn csin(z: C) -> C { C(z.0.sin() * z.1.cosh(), z.0.cos() * z.1.sinh()) }
fn ccos(z: C) -> C { C(z.0.cos() * z.1.cosh(), -z.0.sin() * z.1.sinh()) }
fn csinh(z: C) -> C { C(z.0.sinh() * z.1.cos(), z.0.cosh() * z.1.sin()) }
fn ccosh(z: C) -> C { C(z.0.cosh() * z.1.cos(), z.0.sinh() * z.1.sin()) }
/// A whole power by repeated squaring (exact for small integers), else the polar form.
fn cpow(z: C, n: f64) -> Cr {
    if z.0 == 0.0 && z.1 == 0.0 { return if n > 0.0 { Ok(C(0.0, 0.0)) } else { Err(ErrKind::Num) }; }
    if n.fract() == 0.0 && n.abs() <= 1e6 {
        let (mut out, mut base, mut k) = (C(1.0, 0.0), z, n.abs() as u64);
        while k > 0 { if k % 2 == 1 { out = cmul(out, base); } base = cmul(base, base); k /= 2; }
        if !out.0.is_finite() || !out.1.is_finite() { return Err(ErrKind::Num); }
        return if n < 0.0 { cdiv(C(1.0, 0.0), out) } else { Ok(out) };
    }
    let (r, th) = (z.0.hypot(z.1).powf(n), z.1.atan2(z.0) * n);
    Ok(C(r * th.cos(), r * th.sin()))
}

/// "3+4i", "-2j", "5", 7 → the number and its suffix (none for a real number).
fn parse_c(v: &Value) -> Result<(C, Option<char>), ErrKind> {
    let t = match v {
        Value::Err(e) => return Err(*e),
        Value::Empty => return Ok((C(0.0, 0.0), None)),
        Value::Num(n) => return Ok((C(*n, 0.0), None)),
        Value::Str(s) => s.chars().filter(|c| !c.is_whitespace()).collect::<String>(),
        Value::Array(m) => return parse_c(m.data.first().unwrap_or(&Value::Empty)),
        _ => return Err(ErrKind::Value),
    };
    if t.is_empty() { return Ok((C(0.0, 0.0), None)); }
    let b = t.as_bytes();
    // A number at `i`: digits[.digits][e±digits]; returns its end.
    let numlen = |i: usize| -> Option<usize> {
        let mut j = i; let d0 = j; while j < b.len() && b[j].is_ascii_digit() { j += 1; }
        let int = j > d0; let mut frac = false;
        if j < b.len() && b[j] == b'.' { j += 1; let f0 = j; while j < b.len() && b[j].is_ascii_digit() { j += 1; } frac = j > f0; }
        if !int && !frac { return None; }
        if j < b.len() && (b[j] == b'e' || b[j] == b'E') { let mut k = j + 1; if k < b.len() && (b[k] == b'+' || b[k] == b'-') { k += 1; } let e0 = k; while k < b.len() && b[k].is_ascii_digit() { k += 1; } if k > e0 { j = k; } }
        Some(j)
    };
    let pnum = |s: &str| s.parse::<f64>().map_err(|_| ErrKind::Num);
    let last = b[b.len() - 1];
    let sfx = if last == b'i' || last == b'j' { Some(last as char) } else { None };
    let mut i = 0; if b[0] == b'+' || b[0] == b'-' { i = 1; }
    let first_end = numlen(i);
    if sfx.is_none() { return if first_end == Some(b.len()) { Ok((C(pnum(&t)?, 0.0), None)) } else { Err(ErrKind::Num) }; }
    let body = b.len() - 1;
    // Pure imaginary: [sign][num]i
    if first_end.map_or(i == body, |e| e == body) {
        let m = if i == body { 1.0 } else { pnum(&t[i..body])? };
        return Ok((C(0.0, if b[0] == b'-' { -m } else { m }), sfx));
    }
    let Some(e) = first_end else { return Err(ErrKind::Num) };
    if e >= body || (b[e] != b'+' && b[e] != b'-') { return Err(ErrKind::Num); }
    let re = pnum(&t[..e])?;
    let im = if e + 1 == body { 1.0 } else if numlen(e + 1) == Some(body) { pnum(&t[e + 1..body])? } else { return Err(ErrKind::Num) };
    Ok((C(re, if b[e] == b'-' { -im } else { im }), sfx))
}

/// One part with 15 significant digits, as Excel writes complex numbers.
fn part(n: f64) -> String {
    let r: f64 = format!("{:.14e}", n).parse().unwrap_or(n);
    let a = r.abs();
    if r == 0.0 || (1e-7..1e21).contains(&a) { return format!("{}", r); }
    let s = format!("{:.14e}", r);
    let (m, ex) = s.split_once('e').unwrap_or((&s, "0"));
    let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
    let e: i32 = ex.parse().unwrap_or(0);
    format!("{}E{}{:02}", m, if e < 0 { '-' } else { '+' }, e.abs())
}

fn fmt_c(z: C, sfx: Option<char>, exact: bool) -> Value {
    let (r0, i0) = (z.0, z.1);
    if !r0.is_finite() || !i0.is_finite() { return err(ErrKind::Num); }
    let noise = |n: f64, o: f64| !exact && n.abs() < 1e-14 && n.abs() < o.abs() * 1e-14;
    let p15 = |n: f64| format!("{:.14e}", n).parse::<f64>().unwrap_or(n);
    let re = if noise(r0, i0) { 0.0 } else { p15(r0) };
    let im = if noise(i0, r0) { 0.0 } else { p15(i0) };
    let s = sfx.unwrap_or('i');
    if im == 0.0 { return Value::str(part(re)); }
    let it = if im.abs() == 1.0 { String::new() } else { part(im.abs()) };
    if re == 0.0 { return Value::str(format!("{}{}{}", if im < 0.0 { "-" } else { "" }, it, s)); }
    Value::str(format!("{}{}{}{}", part(re), if im < 0.0 { '-' } else { '+' }, it, s))
}

fn un(a: &[Value], f: fn(C) -> Cr) -> Value { let (z, sf) = tryv!(parse_c(&a[0])); fmt_c(tryv!(f(z)), sf, false) }
fn bin(a: &[Value], exact: bool, f: fn(C, C) -> Cr) -> Value {
    let (x, sx) = tryv!(parse_c(&a[0])); let (y, sy) = tryv!(parse_c(&a[1]));
    if let (Some(p), Some(q)) = (sx, sy) { if p != q { return err(ErrKind::Value); } }
    fmt_c(tryv!(f(x, y)), sx.or(sy), exact)
}
fn fold_c(ctx: &mut Ctx, args: &[Arg], init: C, exact: bool, f: fn(C, C) -> C) -> Value {
    let (mut acc, mut sfx, mut bad) = (init, None::<char>, None);
    for a in args { each(ctx, a, &mut |v, _| {
        if bad.is_some() || v.is_empty() || matches!(v, Value::Str(s) if s.is_empty()) { return; }
        match parse_c(v) {
            Err(e) => bad = Some(e),
            Ok((z, s)) => { if let (Some(p), Some(q)) = (sfx, s) { if p != q { bad = Some(ErrKind::Value); return; } } sfx = sfx.or(s); acc = f(acc, z); }
        }
    }); }
    if let Some(e) = bad { return err(e); }
    fmt_c(acc, sfx, exact)
}

// ---- Bessel functions (integer order) ----

const EULER: f64 = 0.5772156649015329;

fn bargs(a: &[Value], positive: bool) -> Result<(f64, u32), ErrKind> {
    let x = a[0].to_num()?; let n = a[1].to_num()?.trunc();
    if !(0.0..=1e6).contains(&n) || (positive && !(x > 0.0)) { return Err(ErrKind::Num); }
    Ok((x, n as u32))
}

fn ln_gamma(x: f64) -> f64 {
    const G: [f64; 9] = [0.99999999999980993, 676.5203681218851, -1259.1392167224028, 771.32342877765313, -176.61502916214059, 12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7];
    let x = x - 1.0; let t = x + 7.5;
    let a = (1..9).fold(G[0], |a, i| a + G[i] / (x + i as f64));
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

fn bessel_j_series(n: u32, x: f64) -> f64 {
    if n >= 400 { return 0.0; }
    let h = x / 2.0; let h2 = h * h;
    let mut term = (1..=n).fold(1.0, |t, j| t * h / j as f64);
    let mut sum = term; let mut k = 1.0;
    while term != 0.0 { term *= -h2 / (k * (n as f64 + k)); sum += term; if term.abs() <= sum.abs() * 1e-17 { break; } k += 1.0; }
    sum
}

/// I_n(x), x ≥ 0, by its all-positive power series, scaled to survive huge or tiny first terms.
fn bessel_i_series(n: u32, x: f64) -> f64 {
    if x == 0.0 { return if n == 0 { 1.0 } else { 0.0 }; }
    let h = x / 2.0; let h2 = h * h; let nf = n as f64;
    let mut offset = 0.0; let mut term;
    if n <= 300 {
        term = (1..=n).fold(1.0, |t, j| t * h / j as f64);
        if term == 0.0 { return 0.0; }
        if !term.is_finite() { return f64::INFINITY; }
    } else {
        let lt = nf * h.ln() - ln_gamma(nf + 1.0);
        if lt + h2 / (nf + 1.0) < -745.0 { return 0.0; }
        if lt > 709.0 { return f64::INFINITY; }
        if lt < -700.0 { offset = lt; term = 1.0; } else { term = lt.exp(); }
    }
    let mut sum = term; let mut k = 1.0;
    loop {
        term *= h2 / (k * (nf + k)); sum += term;
        if term <= sum * 1e-17 { break; }
        if sum > 1e250 { sum *= 1e-250; term *= 1e-250; offset += 250.0 * std::f64::consts::LN_10; if offset + sum.ln() > 710.0 { return f64::INFINITY; } }
        k += 1.0;
    }
    if offset != 0.0 { (offset + sum.ln()).exp() } else { sum }
}

/// Miller's downward recurrence: (J0, J1, Jn, Σ for Y0, Σ for Y1).
fn bessel_j_miller(x: f64, n: u32) -> (f64, f64, f64, f64, f64) {
    let top = (n as f64).max(x.ceil());
    let mut start = (top + (40.0 * top).sqrt().ceil() + 40.0) as i64; if start % 2 == 1 { start += 1; }
    let (mut above, mut at, mut norm) = (0.0f64, 1.0f64, 0.0f64);
    let j0; let (mut j1, mut jn, mut s0, mut s1) = (0.0, 0.0, 0.0, 0.0);
    let mut k = start;
    loop {
        let kf = k as f64;
        if k == n as i64 { jn = at; }
        if k % 2 == 0 {
            norm += if k != 0 { 2.0 * at } else { at };
            if k != 0 { s0 += (if (k / 2) % 2 == 1 { -2.0 } else { 2.0 }) / kf * at; }
            if k == 0 { j0 = at; break; }
        } else {
            s1 += (if ((k + 1) / 2) % 2 == 1 { -1.0 } else { 1.0 }) * (if k == 1 { 1.0 } else { 4.0 * kf / (kf * kf - 1.0) }) * at;
            if k == 1 { j1 = at; }
        }
        let below = 2.0 * kf / x * at - above; above = at; at = below;
        if at.abs() > 1e250 { for v in [&mut above, &mut at, &mut norm, &mut j1, &mut jn, &mut s0, &mut s1] { *v *= 1e-250; } }
        k -= 1;
    }
    (j0 / norm, j1 / norm, jn / norm, s0 / norm, s1 / norm)
}

fn hankel(nu: f64, x: f64) -> (f64, f64) {
    let mu = 4.0 * nu * nu; let (mut p, mut q, mut term) = (0.0, 0.0, 1.0);
    for k in 0..40 {
        if k % 2 == 0 { p += if (k / 2) % 2 == 1 { -term } else { term }; } else { q += if ((k - 1) / 2) % 2 == 1 { -term } else { term }; }
        let next = term * (mu - ((2 * k + 1) as f64).powi(2)) / (8.0 * (k + 1) as f64 * x);
        if next.abs() < 1e-17 { break; }
        term = next;
    }
    let chi = x - (nu / 2.0 + 0.25) * std::f64::consts::PI; let amp = (2.0 / (std::f64::consts::PI * x)).sqrt();
    (amp * (p * chi.cos() - q * chi.sin()), amp * (p * chi.sin() + q * chi.cos()))
}

fn bessel_j(n: u32, x: f64) -> f64 {
    let ax = x.abs();
    let v = if ax * ax <= 4.0 * (n as f64 + 1.0) { bessel_j_series(n, ax) }
    else if ax <= 1e4 || n as f64 >= ax { bessel_j_miller(ax, n).2 }
    else {
        let (mut below, mut at) = (hankel(0.0, ax).0, hankel(1.0, ax).0);
        for k in 1..n { let nx = 2.0 * k as f64 / ax * at - below; below = at; at = nx; }
        if n == 0 { below } else { at }
    };
    if x < 0.0 && n % 2 == 1 { -v } else { v }
}

fn bessel_y(n: u32, x: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let (y0, y1) = if x > 1e4 { (hankel(0.0, x).1, hankel(1.0, x).1) }
    else if x < 1e-300 { (2.0 / pi * ((x / 2.0).ln() + EULER), -2.0 / (pi * x)) }
    else { let (j0, j1, _, s0, s1) = bessel_j_miller(x, 1); let l = (x / 2.0).ln() + EULER; (2.0 / pi * (l * j0 - 2.0 * s0), 2.0 / pi * (-j0 / x + l * j1 + s1)) };
    if n == 0 { return y0; }
    let (mut below, mut at) = (y0, y1);
    let mut k = 1; while k < n && at.is_finite() { let nx = 2.0 * k as f64 / x * at - below; below = at; at = nx; k += 1; }
    at
}

fn bessel_k01(nu: u32, x: f64) -> f64 {
    let mu = 4.0 * (nu * nu) as f64;
    if x >= 20.0 {
        let (mut sum, mut term) = (0.0, 1.0);
        for k in 0..60 { sum += term; let nx = term * (mu - ((2 * k + 1) as f64).powi(2)) / (8.0 * (k + 1) as f64 * x); if nx.abs() < 1e-17 * sum || nx.abs() > term.abs() { break; } term = nx; }
        return (std::f64::consts::PI / (2.0 * x)).sqrt() * (-x).exp() * sum;
    }
    if x < 1e-300 { return if nu == 1 { 1.0 / x } else { -((x / 2.0).ln() + EULER) }; }
    let h = std::f64::consts::PI.powi(2) / (2.0 * x + 40.0); let end = (1.0 + 42.0 / x).acosh();
    let mut sum = 0.5 * (-x).exp(); let mut t = h;
    while t <= end { let c = t.cosh(); sum += (-x * c).exp() * if nu == 1 { c } else { 1.0 }; t += h; }
    h * sum
}

fn bessel_k(n: u32, x: f64) -> f64 {
    let k0 = bessel_k01(0, x); if n == 0 { return k0; }
    let (mut below, mut at) = (k0, bessel_k01(1, x));
    let mut k = 1; while k < n && at.is_finite() { let nx = below + 2.0 * k as f64 / x * at; below = at; at = nx; k += 1; }
    at
}

// ---- bases and bits ----

fn radix(mut v: u64, b: u32) -> String {
    if v == 0 { return "0".into(); }
    let mut d = Vec::new();
    while v > 0 { d.push(std::char::from_digit((v % b as u64) as u32, b).unwrap_or('0').to_ascii_uppercase()); v /= b as u64; }
    d.iter().rev().collect()
}
fn pad(s: String, w: usize) -> String { if s.len() >= w { s } else { format!("{}{}", "0".repeat(w - s.len()), s) } }

/// BIN2DEC and friends: at most 10 digits, a full-width number with the top bit set is negative.
fn from_base(v: &Value, base: u32) -> Result<f64, ErrKind> {
    let d = v.to_text()?.trim().to_uppercase();
    if d.is_empty() { return Ok(0.0); }
    if d.len() > 10 { return Err(ErrKind::Num); }
    let n = u64::from_str_radix(&d, base).map_err(|_| ErrKind::Num)? as f64;
    let span = (base as f64).powi(10);
    Ok(if d.len() == 10 && n >= span / 2.0 { n - span } else { n })
}
fn bits(base: u32) -> i32 { match base { 2 => 10, 8 => 30, _ => 40 } }
fn to_base(x: f64, base: u32, places: Option<&Value>) -> Value {
    let n = x.trunc(); let b = bits(base); let half = 2f64.powi(b - 1);
    if n < -half || n >= half { return err(ErrKind::Num); }
    let pl = match places { None | Some(Value::Empty) => None, Some(p) => Some(n!(p)) };
    if n < 0.0 { return Value::str(radix((n + 2f64.powi(b)) as u64, base)); }
    let d = radix(n as u64, base);
    match pl { None => Value::str(d), Some(w) => { let w = w.trunc(); if w < d.len() as f64 || w > 10.0 { err(ErrKind::Num) } else { Value::str(pad(d, w as usize)) } } }
}
fn conv(a: &[Value], from: u32, to: u32) -> Value { let n = tryv!(from_base(&a[0], from)); to_base(n, to, a.get(1)) }
fn bit(a: &[Value], f: fn(u64, u64) -> u64) -> Value {
    let (x, y) = (n!(a[0]), n!(a[1])); let lim = 2f64.powi(48);
    if [x, y].iter().any(|v| *v < 0.0 || *v >= lim || v.fract() != 0.0) { return err(ErrKind::Num); }
    num(f(x as u64, y as u64) as f64)
}
fn shift(n: f64, sh: f64) -> Value {
    if n.fract() != 0.0 || sh.fract() != 0.0 || n < 0.0 || n >= 2f64.powi(48) || sh.abs() > 53.0 { return err(ErrKind::Num); }
    let out = if sh >= 0.0 { n * 2f64.powf(sh) } else { (n / 2f64.powf(-sh)).floor() };
    if out >= 2f64.powi(48) { err(ErrKind::Num) } else { num(out) }
}

// ---- ROMAN / ARABIC ----

fn roman(a: &[Value]) -> Value {
    let x = n!(a[0]);
    let mode = match a.get(1) { None | Some(Value::Empty) => 0, Some(Value::Bool(b)) => if *b { 0 } else { 4 }, Some(f) => n!(f).trunc() as i64 };
    let mut v = x.trunc() as i64;
    if x.trunc() < 0.0 || x.trunc() > 3999.0 || !(0..=4).contains(&mode) { return err(ErrKind::Value); }
    let chars = ['M', 'D', 'C', 'L', 'X', 'V', 'I']; let vals = [1000i64, 500, 100, 50, 10, 5, 1];
    let mut out = String::new();
    for place in 0..4 {
        let mut idx = 2 * place; let digit = v / vals[idx];
        if digit % 5 == 4 {
            let from = if digit == 4 { idx - 1 } else { idx - 2 };
            let mut steps = 0; while steps < mode && idx < 6 { if vals[from] - vals[idx + 1] > v { break; } idx += 1; steps += 1; }
            out.push(chars[idx]); out.push(chars[from]); v += vals[idx] - vals[from];
        } else {
            if digit > 4 { out.push(chars[idx - 1]); }
            for _ in 0..digit % 5 { out.push(chars[idx]); }
            v %= vals[idx];
        }
    }
    Value::str(out)
}

fn arabic(a: &[Value]) -> Value {
    let r = t!(a[0]).trim().to_uppercase();
    if r.is_empty() { return num(0.0); }
    let (neg, body) = match r.strip_prefix('-') { Some(b) => (true, b), None => (false, r.as_str()) };
    let val = |c: char| match c { 'I' => 1, 'V' => 5, 'X' => 10, 'L' => 50, 'C' => 100, 'D' => 500, 'M' => 1000, _ => 0 };
    let cs: Vec<char> = body.chars().collect();
    if cs.is_empty() || cs.iter().any(|c| val(*c) == 0) { return err(ErrKind::Value); }
    let mut total = 0i64;
    for i in 0..cs.len() { let (cur, nx) = (val(cs[i]), cs.get(i + 1).map_or(0, |c| val(*c))); total += if cur < nx { -cur } else { cur }; }
    num(if neg { -total } else { total } as f64)
}

// ---- CONVERT ----

const PREFIXES: &[(&str, f64)] = &[("Y", 1e24), ("Z", 1e21), ("E", 1e18), ("P", 1e15), ("T", 1e12), ("G", 1e9), ("M", 1e6), ("k", 1e3), ("h", 1e2), ("da", 1e1), ("e", 1e1), ("d", 1e-1), ("c", 1e-2), ("m", 1e-3), ("u", 1e-6), ("n", 1e-9), ("p", 1e-12), ("f", 1e-15), ("a", 1e-18), ("z", 1e-21), ("y", 1e-24),
    ("ki", 1024.0), ("Mi", 1048576.0), ("Gi", 1073741824.0), ("Ti", 1099511627776.0), ("Pi", 1125899906842624.0), ("Ei", 1152921504606846976.0), ("Zi", 1180591620717411303424.0), ("Yi", 1208925819614629174706176.0)];
const UNITS: &[(&str, &[(&str, f64)])] = &[
    ("mass", &[("g", 1.0), ("sg", 14593.9029372064), ("lbm", 453.59237), ("u", 1.66053886282828e-24), ("ozm", 28.349523125), ("grain", 0.06479891), ("cwt", 45359.237), ("shweight", 45359.237), ("uk_cwt", 50802.34544), ("lcwt", 50802.34544), ("hweight", 50802.34544), ("stone", 6350.29318), ("ton", 907184.74), ("uk_ton", 1016046.9088), ("LTON", 1016046.9088), ("brton", 1016046.9088)]),
    ("length", &[("m", 1.0), ("mi", 1609.344), ("Nmi", 1852.0), ("in", 0.0254), ("ft", 0.3048), ("yd", 0.9144), ("ang", 1e-10), ("ell", 1.143), ("ly", 9.46073047258e15), ("parsec", 3.08567758128155e16), ("pc", 3.08567758128155e16), ("Pica", 0.000352777777777778), ("Picapt", 0.000352777777777778), ("pica", 0.00423333333333333), ("survey_mi", 1609.34721869444)]),
    ("time", &[("yr", 31557600.0), ("day", 86400.0), ("d", 86400.0), ("hr", 3600.0), ("mn", 60.0), ("min", 60.0), ("sec", 1.0), ("s", 1.0)]),
    ("pressure", &[("Pa", 1.0), ("p", 1.0), ("atm", 101325.0), ("at", 101325.0), ("mmHg", 133.322), ("psi", 6894.75729316836), ("Torr", 133.322368421053)]),
    ("force", &[("N", 1.0), ("dyn", 1e-5), ("dy", 1e-5), ("lbf", 4.4482216152605), ("pond", 0.00980665)]),
    ("energy", &[("J", 1.0), ("e", 1e-7), ("c", 4.184), ("cal", 4.1868), ("eV", 1.602176565e-19), ("ev", 1.602176565e-19), ("HPh", 2684519.53769617), ("hh", 2684519.53769617), ("Wh", 3600.0), ("wh", 3600.0), ("flb", 1.3558179483314), ("BTU", 1055.05585262), ("btu", 1055.05585262)]),
    ("power", &[("W", 1.0), ("w", 1.0), ("HP", 745.69987158227), ("h", 745.69987158227), ("PS", 735.49875)]),
    ("magnetism", &[("T", 1.0), ("ga", 1e-4)]),
    ("volume", &[("l", 0.001), ("L", 0.001), ("lt", 0.001), ("tsp", 4.92892159375e-6), ("tspm", 5e-6), ("tbs", 1.478676478125e-5), ("oz", 2.95735295625e-5), ("cup", 2.365882365e-4), ("pt", 4.73176473e-4), ("us_pt", 4.73176473e-4), ("uk_pt", 5.6826125e-4), ("qt", 9.46352946e-4), ("uk_qt", 0.0011365225), ("gal", 0.003785411784), ("uk_gal", 0.00454609), ("m3", 1.0), ("ang3", 1e-30), ("barrel", 0.158987294928), ("bushel", 0.03523907016688), ("ft3", 0.028316846592), ("in3", 1.6387064e-5), ("ly3", 8.46786664623715e47), ("mi3", 4168181825.44058), ("yd3", 0.764554857984), ("MTON", 1.13267386368), ("GRT", 2.8316846592), ("regton", 2.8316846592), ("Nmi3", 6352182208.0), ("Pica3", 4.39039566186557e-11), ("Picapt3", 4.39039566186557e-11)]),
    ("area", &[("m2", 1.0), ("uk_acre", 4046.8564224), ("us_acre", 4046.87260987425), ("ang2", 1e-20), ("ar", 100.0), ("ft2", 0.09290304), ("ha", 10000.0), ("in2", 0.00064516), ("ly2", 8.95054210748189e31), ("Morgen", 2500.0), ("mi2", 2589988.110336), ("Nmi2", 3429904.0), ("Pica2", 1.24452160493827e-7), ("Picapt2", 1.24452160493827e-7), ("yd2", 0.83612736)]),
    ("information", &[("bit", 1.0), ("byte", 8.0)]),
    ("speed", &[("m/s", 1.0), ("m/sec", 1.0), ("m/h", 1.0 / 3600.0), ("m/hr", 1.0 / 3600.0), ("mph", 0.44704), ("kn", 0.514444444444444), ("admkn", 0.514773333333333)]),
];
const PREFIXABLE: &[&str] = &["g", "u", "m", "ang", "ly", "parsec", "pc", "sec", "s", "Pa", "p", "atm", "at", "mmHg", "N", "dyn", "dy", "pond", "J", "e", "c", "cal", "eV", "ev", "Wh", "wh", "W", "w", "T", "ga", "l", "L", "lt", "tspm", "m3", "ang3", "ly3", "m2", "ang2", "ly2", "ar", "bit", "byte", "m/s", "m/sec", "m/h", "m/hr"];

fn lookup_unit(u: &str) -> Option<(&'static str, f64)> { UNITS.iter().find_map(|(g, t)| t.iter().find(|(n, _)| *n == u).map(|(_, f)| (*g, *f))) }

fn unit_factor(u: &str) -> Option<(&'static str, f64)> {
    if let Some(r) = lookup_unit(u) { return Some(r); }
    for (p, mult) in PREFIXES {
        let Some(base) = u.strip_prefix(p) else { continue };
        if base.is_empty() || !PREFIXABLE.contains(&base) { continue; }
        if p.len() == 2 && p.ends_with('i') && base != "bit" && base != "byte" { continue; }
        if let Some((g, f)) = lookup_unit(base) {
            let pw = if base.ends_with('3') { 3 } else if base.ends_with('2') { 2 } else { 1 };
            let pw = if pw > 1 && g != "volume" && g != "area" { 1 } else { pw };
            return Some((g, f * mult.powi(pw)));
        }
    }
    None
}

/// A temperature unit: (to Celsius, from Celsius).
fn temp(u: &str) -> Option<(fn(f64) -> f64, fn(f64) -> f64)> {
    Some(match u {
        "C" | "cel" => (|c| c, |k| k),
        "F" | "fah" => (|f| (f - 32.0) * 5.0 / 9.0, |c| c * 9.0 / 5.0 + 32.0),
        "K" | "kel" => (|k| k - 273.15, |c| c + 273.15),
        "Rank" => (|r| (r - 491.67) * 5.0 / 9.0, |c| (c + 273.15) * 9.0 / 5.0),
        "Reau" => (|r| r * 1.25, |c| c * 0.8),
        _ => return None,
    })
}

fn convert(x: f64, from: &str, to: &str) -> Option<f64> {
    let strip = |s: &str| -> String { if s.len() >= 2 && (s.ends_with('2') || s.ends_with('3')) && s.as_bytes()[s.len() - 2] == b'^' { format!("{}{}", &s[..s.len() - 2], &s[s.len() - 1..]) } else { s.to_string() } };
    let (f, t) = (strip(from), strip(to));
    match (temp(&f), temp(&t)) {
        (Some(a), Some(b)) => return Some(b.1(a.0(x))),
        (None, None) => {}
        _ => return None,
    }
    let (a, b) = (unit_factor(&f)?, unit_factor(&t)?);
    if a.0 != b.0 { return None; }
    Some(x * a.1 / b.1)
}
