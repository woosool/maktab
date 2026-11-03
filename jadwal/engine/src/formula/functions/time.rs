//! Date and time functions: TODAY, DATE, YEAR, EDATE, EOMONTH, WEEKDAY, DATEDIF, NETWORKDAYS…

use super::*;
use crate::date::{days_in_month, hms, serial, weekday, ymd};

fn d(v: &Value) -> Result<f64, ErrKind> {
    match v { Value::Str(s) => crate::numfmt::parse_input(s, false).map(|x| x.0).ok_or(ErrKind::Value), v => v.to_num() }.and_then(|x| if x < 0.0 { Err(ErrKind::Num) } else { Ok(x) })
}
macro_rules! dt { ($v:expr) => { match d(&$v) { Ok(x) => x, Err(e) => return Value::Err(e) } }; }

pub const DEFS: &[Def] = &[
    e("TODAY", 0, 0, |c, _| num(c.calc.now.floor())),
    e("NOW", 0, 0, |c, _| num(c.calc.now)),
    s("DATE", 3, 3, |a| { let (y, m, dd) = (n!(a[0]).trunc() as i64, n!(a[1]).trunc() as i64, n!(a[2]).trunc() as i64); if !(0..10000).contains(&y) { return err(ErrKind::Num); } let s = serial(y, m, dd, false); if s < 0.0 { err(ErrKind::Num) } else { num(s) } }),
    s("TIME", 3, 3, |a| { let t = (n!(a[0]).trunc() * 3600.0 + n!(a[1]).trunc() * 60.0 + n!(a[2]).trunc()) / 86400.0; if t < 0.0 { err(ErrKind::Num) } else { num(t.fract()) } }),
    s("YEAR", 1, 1, |a| num(ymd(dt!(a[0]), false).0 as f64)),
    s("MONTH", 1, 1, |a| num(ymd(dt!(a[0]), false).1 as f64)),
    s("DAY", 1, 1, |a| num(ymd(dt!(a[0]), false).2 as f64)),
    s("HOUR", 1, 1, |a| num(hms(dt!(a[0])).0 as f64)),
    s("MINUTE", 1, 1, |a| num(hms(dt!(a[0])).1 as f64)),
    s("SECOND", 1, 1, |a| num(hms(dt!(a[0])).2 as f64)),
    s("WEEKDAY", 1, 2, |a| { let s = dt!(a[0]); let t = tryv!(opt(a, 1, 1.0)) as i64; let w = weekday(s, false) as i64; // 0 = Sunday
        num(match t { 1 | 17 => w + 1, 2 | 11 => (w + 6) % 7 + 1, 3 => (w + 6) % 7, 12 => (w + 5) % 7 + 1, 13 => (w + 4) % 7 + 1, 14 => (w + 3) % 7 + 1, 15 => (w + 2) % 7 + 1, 16 => (w + 1) % 7 + 1, _ => return err(ErrKind::Num) } as f64) }),
    s("EDATE", 2, 2, |a| { let s = dt!(a[0]); let m = n!(a[1]).trunc() as i64; num(add_months(s, m, false)) }),
    s("EOMONTH", 2, 2, |a| { let s = dt!(a[0]); let m = n!(a[1]).trunc() as i64; num(add_months(s, m, true)) }),
    s("DAYS", 2, 2, |a| num(dt!(a[0]).floor() - dt!(a[1]).floor())),
    s("DAYS360", 2, 3, |a| { let (s, e) = (dt!(a[0]), dt!(a[1])); let eu = a.get(2).map(|v| v.to_bool().unwrap_or(false)).unwrap_or(false); num(days360(s, e, eu)) }),
    s("DATEDIF", 3, 3, |a| datedif(dt!(a[0]), dt!(a[1]), &t!(a[2]))),
    s("DATEVALUE", 1, 1, |a| match &a[0] { Value::Str(s) => crate::numfmt::parse_input(s, false).map_or(err(ErrKind::Value), |x| num(x.0.floor())), _ => err(ErrKind::Value) }),
    s("TIMEVALUE", 1, 1, |a| match &a[0] { Value::Str(s) => crate::numfmt::parse_input(s, false).map_or(err(ErrKind::Value), |x| num(x.0.fract())), _ => err(ErrKind::Value) }),
    s("WEEKNUM", 1, 2, |a| { let s = dt!(a[0]); let t = tryv!(opt(a, 1, 1.0)) as i64; if t == 21 { return num(isoweek(s) as f64); }
        let start = match t { 1 | 17 => 0, 2 | 11 => 1, 12 => 2, 13 => 3, 14 => 4, 15 => 5, 16 => 6, _ => return err(ErrKind::Num) };
        let (y, _, _) = ymd(s, false); let jan1 = serial(y, 1, 1, false); let off = (weekday(jan1, false) as i64 - start).rem_euclid(7);
        num(((s.floor() - jan1 + off as f64) / 7.0).floor() + 1.0) }),
    s("ISOWEEKNUM", 1, 1, |a| num(isoweek(dt!(a[0])) as f64)),
    s("YEARFRAC", 2, 3, |a| { let (s, e) = (dt!(a[0]), dt!(a[1])); let b = tryv!(opt(a, 2, 0.0)) as i64; yearfrac(s, e, b).map_or(err(ErrKind::Num), num) }),
    e("NETWORKDAYS", 2, 3, |c, a| { let (s, e) = (tryv!(d(&arg_val(c, a, 0))), tryv!(d(&arg_val(c, a, 1)))); let h = holidays(c, a.get(2)); networkdays(s, e, &[0, 6], &tryv!(h)) }),
    e("NETWORKDAYS.INTL", 2, 4, |c, a| { let (s, e) = (tryv!(d(&arg_val(c, a, 0))), tryv!(d(&arg_val(c, a, 1)))); let w = tryv!(weekend(&arg_val(c, a, 2))); let h = holidays(c, a.get(3)); networkdays(s, e, &w, &tryv!(h)) }),
    e("WORKDAY", 2, 3, |c, a| { let s = tryv!(d(&arg_val(c, a, 0))); let k = tryv!(arg_num(c, a, 1, 0.0)); let h = holidays(c, a.get(2)); workday(s, k, &[0, 6], &tryv!(h)) }),
    e("WORKDAY.INTL", 2, 4, |c, a| { let s = tryv!(d(&arg_val(c, a, 0))); let k = tryv!(arg_num(c, a, 1, 0.0)); let w = tryv!(weekend(&arg_val(c, a, 2))); let h = holidays(c, a.get(3)); workday(s, k, &w, &tryv!(h)) }),
];

pub fn add_months(s: f64, m: i64, end: bool) -> f64 {
    let (y, mo, dd) = ymd(s, false);
    let total = y * 12 + mo as i64 - 1 + m;
    let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let last = days_in_month(ny, nm);
    serial(ny, nm as i64, if end { last } else { dd.min(last) } as i64, false)
}

fn days360(s: f64, e: f64, european: bool) -> f64 {
    let (y1, m1, mut d1) = ymd(s, false); let (y2, m2, mut d2) = ymd(e, false);
    if european { d1 = d1.min(30); d2 = d2.min(30); } else {
        let last_feb = |y: i64, m: u32, d: u32| m == 2 && d == days_in_month(y, 2);
        if last_feb(y1, m1, d1) { if last_feb(y2, m2, d2) { d2 = 30; } d1 = 30; }
        if d2 == 31 && d1 >= 30 { d2 = 30; }
        if d1 == 31 { d1 = 30; }
    }
    ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + d2 as i64 - d1 as i64) as f64
}

fn datedif(s: f64, e: f64, unit: &str) -> Value {
    if s > e { return err(ErrKind::Num); }
    let (y1, m1, d1) = ymd(s, false); let (y2, m2, d2) = ymd(e, false);
    let months = (y2 - y1) * 12 + m2 as i64 - m1 as i64 - if d2 < d1 { 1 } else { 0 };
    num(match unit.to_ascii_uppercase().as_str() {
        "Y" => (months / 12) as f64, "M" => months as f64, "D" => e.floor() - s.floor(),
        "YM" => (months % 12) as f64,
        "MD" => { if d2 >= d1 { (d2 - d1) as f64 } else { let (py, pm) = if m2 == 1 { (y2 - 1, 12) } else { (y2, m2 - 1) }; (days_in_month(py, pm) as i64 - d1 as i64 + d2 as i64).max(0) as f64 } }
        "YD" => { let mut a = serial(y2, m1 as i64, d1 as i64, false); if a > e { a = serial(y2 - 1, m1 as i64, d1 as i64, false); } e.floor() - a }
        _ => return err(ErrKind::Num),
    })
}

fn isoweek(s: f64) -> u32 {
    let w = (weekday(s, false) + 6) % 7; // Monday = 0
    let thursday = s.floor() - w as f64 + 3.0;
    let (y, _, _) = ymd(thursday, false);
    ((thursday - serial(y, 1, 1, false)) / 7.0).floor() as u32 + 1
}

pub fn yearfrac(s: f64, e: f64, basis: i64) -> Option<f64> {
    let (s, e) = if s > e { (e, s) } else { (s, e) };
    let (y1, m1, d1) = ymd(s, false); let (y2, m2, d2) = ymd(e, false);
    Some(match basis {
        0 => { let (mut d1, mut d2) = (d1, d2); if d1 == 31 { d1 = 30; } if d2 == 31 && d1 >= 30 { d2 = 30; } if m1 == 2 && d1 == days_in_month(y1, 2) { d1 = 30; if m2 == 2 && d2 == days_in_month(y2, 2) { d2 = 30; } }
            ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + d2 as i64 - d1 as i64) as f64 / 360.0 }
        1 => {
            let days = e - s;
            if y1 == y2 { days / if crate::date::is_leap(y1) { 366.0 } else { 365.0 } }
            else if (y2 - y1 == 1) && (m1 > m2 || (m1 == m2 && d1 >= d2)) {
                let leap = (crate::date::is_leap(y1) && (m1 < 3)) || (crate::date::is_leap(y2) && (m2 > 2 || (m2 == 2 && d2 == 29)));
                days / if leap { 366.0 } else { 365.0 }
            } else {
                let years = (y1..=y2).map(|y| if crate::date::is_leap(y) { 366.0 } else { 365.0 }).sum::<f64>() / (y2 - y1 + 1) as f64;
                days / years
            }
        }
        2 => (e - s) / 360.0, 3 => (e - s) / 365.0, 4 => days360(s, e, true) / 360.0,
        _ => return None,
    })
}

fn holidays(ctx: &Ctx, a: Option<&Arg>) -> Result<Vec<f64>, ErrKind> {
    let Some(a) = a.filter(|x| !x.is_missing()) else { return Ok(vec![]) };
    flat(ctx, a).iter().filter(|v| !v.is_empty()).map(|v| d(v).map(|x| x.floor())).collect()
}

/// The weekend days (0 = Sunday) from a weekend number or a "0000011" mask (Monday first).
fn weekend(v: &Value) -> Result<Vec<u32>, ErrKind> {
    match v {
        Value::Empty => Ok(vec![0, 6]),
        Value::Str(s) => { if s.len() != 7 || !s.chars().all(|c| c == '0' || c == '1') || &**s == "1111111" { return Err(ErrKind::Value); } Ok(s.chars().enumerate().filter(|(_, c)| *c == '1').map(|(i, _)| ((i + 1) % 7) as u32).collect()) }
        v => { let k = v.to_num()? as i64; Ok(match k { 1 => vec![6, 0], 2 => vec![0, 1], 3 => vec![1, 2], 4 => vec![2, 3], 5 => vec![3, 4], 6 => vec![4, 5], 7 => vec![5, 6], 11..=17 => vec![((k - 11 + 0) as u32 + 0) % 7], _ => return Err(ErrKind::Num) }) }
    }
}

fn networkdays(s: f64, e: f64, wk: &[u32], hol: &[f64]) -> Value {
    let (a, b, sign) = if s <= e { (s.floor(), e.floor(), 1.0) } else { (e.floor(), s.floor(), -1.0) };
    let mut k = 0.0; let mut x = a;
    while x <= b { if !wk.contains(&weekday(x, false)) && !hol.contains(&x) { k += 1.0; } x += 1.0; }
    num(k * sign)
}

fn workday(s: f64, k: f64, wk: &[u32], hol: &[f64]) -> Value {
    if wk.len() >= 7 { return err(ErrKind::Value); }
    let step = if k >= 0.0 { 1.0 } else { -1.0 };
    let mut left = k.abs().trunc() as i64; let mut x = s.floor();
    while left > 0 { x += step; if !wk.contains(&weekday(x, false)) && !hol.contains(&x) { left -= 1; } }
    num(x)
}
