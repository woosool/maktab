//! Information functions: ISBLANK, ISNUMBER, ISERROR, TYPE, N, NA, ERROR.TYPE, ISFORMULA…

use super::*;

pub const DEFS: &[Def] = &[
    e("ISBLANK", 1, 1, |c, a| { let v = c.scalar_of(&a[0]); boolv(v.is_empty()) }),
    s("ISNUMBER", 1, 1, |a| boolv(matches!(a[0], Value::Num(_)))),
    s("ISTEXT", 1, 1, |a| boolv(matches!(a[0], Value::Str(_)))),
    s("ISNONTEXT", 1, 1, |a| boolv(!matches!(a[0], Value::Str(_)))),
    s("ISLOGICAL", 1, 1, |a| boolv(matches!(a[0], Value::Bool(_)))),
    s("ISERROR", 1, 1, |a| boolv(a[0].is_err())),
    s("ISERR", 1, 1, |a| boolv(a[0].is_err() && a[0].err() != Some(ErrKind::NA))),
    s("ISNA", 1, 1, |a| boolv(a[0].err() == Some(ErrKind::NA))),
    s("ISEVEN", 1, 1, |a| boolv(n!(a[0]).trunc() % 2.0 == 0.0)),
    s("ISODD", 1, 1, |a| boolv(n!(a[0]).trunc() % 2.0 != 0.0)),
    s("NA", 0, 0, |_| err(ErrKind::NA)),
    s("N", 1, 1, |a| match &a[0] { Value::Num(n) => num(*n), Value::Bool(b) => num(*b as i32 as f64), Value::Err(e) => Value::Err(*e), _ => num(0.0) }),
    s("T", 1, 1, |a| match &a[0] { Value::Str(_) => a[0].clone(), Value::Err(e) => Value::Err(*e), _ => Value::str("") }),
    e("TYPE", 1, 1, |c, a| { if matches!(&a[0], Arg::R(_, r) if !r.is_cell()) { return num(64.0); } match c.value_of(&a[0]) { Value::Num(_) | Value::Empty => num(1.0), Value::Str(_) => num(2.0), Value::Bool(_) => num(4.0), Value::Err(_) => num(16.0), Value::Array(_) => num(64.0) } }),
    s("ERROR.TYPE", 1, 1, |a| match a[0] { Value::Err(e) => num(e.code()), _ => err(ErrKind::NA) }),
    e("ISFORMULA", 1, 1, |c, a| match &a[0] { Arg::R(s, r) => boolv(c.book.sheets[*s].cells.get(&r.start()).is_some_and(|x| x.is_formula())), _ => err(ErrKind::Value) }),
    e("FORMULATEXT", 1, 1, |c, a| match &a[0] { Arg::R(s, r) => c.book.sheets[*s].cells.get(&r.start()).and_then(|x| x.formula()).map_or(err(ErrKind::NA), |f| Value::str(format!("={f}"))), _ => err(ErrKind::Value) }),
    e("ISREF", 1, 1, |_, a| boolv(matches!(a[0], Arg::R(..) | Arg::Areas(_)))),
    e("SHEET", 0, 1, |c, a| match a.first() { None => num(c.sheet as f64 + 1.0), Some(Arg::R(s, _)) => num(*s as f64 + 1.0), Some(x) => match c.value_of(x) { Value::Str(n) => c.book.sheet_index(&n).map_or(err(ErrKind::NA), |i| num(i as f64 + 1.0)), _ => err(ErrKind::Value) } }),
    e("SHEETS", 0, 1, |c, a| match a.first() { Some(Arg::Areas(l)) => num(l.len() as f64), Some(_) => num(1.0), None => num(c.book.sheets.len() as f64) }),
];
