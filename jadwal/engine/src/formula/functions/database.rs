//! Database functions: DSUM, DAVERAGE, DCOUNT, DGET… over a table with a header row, a field
//! (header text or 1-based column), and a criteria table (AND across a row, OR down the rows).

use super::criteria::Criteria;
use super::*;

pub const DEFS: &[Def] = &[
    e("DSUM", 3, 3, |c, a| agg(c, a, false, |l| num(l.iter().sum()))),
    e("DAVERAGE", 3, 3, |c, a| agg(c, a, false, |l| if l.is_empty() { err(ErrKind::Div0) } else { num(l.iter().sum::<f64>() / l.len() as f64) })),
    e("DCOUNT", 2, 3, |c, a| agg(c, a, true, |l| num(l.len() as f64))),
    e("DCOUNTA", 2, 3, dcounta),
    e("DMAX", 3, 3, |c, a| agg(c, a, false, |l| num(if l.is_empty() { 0.0 } else { l.iter().cloned().fold(f64::MIN, f64::max) }))),
    e("DMIN", 3, 3, |c, a| agg(c, a, false, |l| num(if l.is_empty() { 0.0 } else { l.iter().cloned().fold(f64::MAX, f64::min) }))),
    e("DPRODUCT", 3, 3, |c, a| agg(c, a, false, |l| num(if l.is_empty() { 0.0 } else { l.iter().product() }))),
    e("DSTDEV", 3, 3, |c, a| agg(c, a, false, |l| var(l, 1).map_or_else(err, |v| num(v.sqrt())))),
    e("DSTDEVP", 3, 3, |c, a| agg(c, a, false, |l| var(l, 0).map_or_else(err, |v| num(v.sqrt())))),
    e("DVAR", 3, 3, |c, a| agg(c, a, false, |l| var(l, 1).map_or_else(err, num))),
    e("DVARP", 3, 3, |c, a| agg(c, a, false, |l| var(l, 0).map_or_else(err, num))),
    e("DGET", 3, 3, dget),
];

fn var(l: &[f64], corr: usize) -> Result<f64, ErrKind> {
    if l.len() <= corr { return Err(ErrKind::Div0); }
    let m = l.iter().sum::<f64>() / l.len() as f64;
    Ok(l.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (l.len() - corr) as f64)
}

fn header_key(v: &Value) -> String { v.to_text().unwrap_or_default().trim().to_lowercase() }

/// A criterion as the advanced filter reads it: plain text finds values beginning with it.
fn db_criterion(v: &Value) -> Value {
    if let Value::Str(s) = v {
        let t = s.trim();
        let plain = !t.starts_with(['<', '>', '=', '#']) && crate::numfmt::parse_number(t).is_none() && !t.eq_ignore_ascii_case("true") && !t.eq_ignore_ascii_case("false");
        if plain { return Value::str(format!("{s}*")); }
    }
    v.clone()
}

/// The matching records: the field's value of each (or the whole row when no field is given).
fn select(ctx: &mut Ctx, a: &[Arg], any_field: bool) -> Result<(Option<usize>, Vec<Vec<Value>>), ErrKind> {
    let t = ctx.matrix_of(&a[0]);
    if t.rows < 1 { return Err(ErrKind::Value); }
    let heads: Vec<String> = t.row(0).iter().map(header_key).collect();
    let col = match a.get(1) {
        None | Some(Arg::Missing) => { if !any_field { return Err(ErrKind::Value); } None }
        Some(f) => match ctx.scalar_of(f) {
            Value::Empty if any_field => None,
            Value::Num(n) => { let c = n.trunc() as i64 - 1; if c < 0 || c as usize >= heads.len() { return Err(ErrKind::Value); } Some(c as usize) }
            Value::Err(e) => return Err(e),
            v => Some(heads.iter().position(|h| *h == header_key(&v)).ok_or(ErrKind::Value)?),
        },
    };
    let Some(crit) = a.get(2) else { return Err(ErrKind::Value) };
    let c = ctx.matrix_of(crit);
    if c.rows < 2 { return Err(ErrKind::Value); }
    let mut tests: Vec<Vec<(usize, Criteria)>> = vec![];
    for r in 1..c.rows {
        let mut row = vec![];
        for j in 0..c.cols {
            let v = c.get(r, j);
            if v.is_empty() || matches!(v, Value::Str(s) if s.is_empty()) { continue; }
            let at = heads.iter().position(|h| *h == header_key(c.get(0, j))).ok_or(ErrKind::Value)?;
            row.push((at, Criteria::new(&db_criterion(v))));
        }
        tests.push(row);
    }
    let out = (1..t.rows).filter(|&r| tests.iter().any(|row| row.iter().all(|(at, k)| k.test(t.get(r, *at))))).map(|r| t.row(r).to_vec()).collect();
    Ok((col, out))
}

fn agg(ctx: &mut Ctx, a: &[Arg], any_field: bool, f: fn(&[f64]) -> Value) -> Value {
    let (col, recs) = tryv!(select(ctx, a, any_field));
    let Some(col) = col else { return num(recs.len() as f64) };
    let mut l = vec![];
    for r in &recs { match &r[col] { Value::Err(e) => return err(*e), Value::Num(n) => l.push(*n), _ => {} } }
    f(&l)
}
fn dcounta(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let (col, recs) = tryv!(select(ctx, a, true));
    let filled = |v: &Value| !v.is_empty() && !matches!(v, Value::Str(s) if s.is_empty());
    num(recs.iter().filter(|r| match col { Some(c) => filled(&r[c]), None => r.iter().any(filled) }).count() as f64)
}
fn dget(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let (col, recs) = tryv!(select(ctx, a, false));
    match recs.len() { 0 => err(ErrKind::Value), 1 => col.map_or(err(ErrKind::Value), |c| recs[0][c].clone()), _ => err(ErrKind::Num) }
}
