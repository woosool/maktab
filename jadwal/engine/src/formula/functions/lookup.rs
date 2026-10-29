//! Lookup and reference functions, and the dynamic-array ones: VLOOKUP, XLOOKUP, MATCH, INDEX,
//! OFFSET, INDIRECT, ROW, FILTER, SORT, UNIQUE, SEQUENCE, TRANSPOSE…

use super::criteria::matches_pattern;
use super::*;
use crate::addr::{Pos, Range, MAX_COLS, MAX_ROWS};
use crate::formula::eval::{compare, sort_cmp};
use std::cmp::Ordering;

pub const DEFS: &[Def] = &[
    e("VLOOKUP", 3, 4, vlookup), e("HLOOKUP", 3, 4, hlookup), e("XLOOKUP", 3, 6, xlookup), e("MATCH", 2, 3, match_), e("XMATCH", 2, 4, xmatch),
    e("LOOKUP", 2, 3, lookup),
    r("INDEX", 2, 4, index), r("OFFSET", 3, 5, offset), r("INDIRECT", 1, 2, indirect),
    e("ROW", 0, 1, |c, a| match a.first() { None => num(c.pos.row as f64 + 1.0), Some(Arg::R(_, r)) => if r.rows() == 1 { num(r.r0 as f64 + 1.0) } else { Value::array(Matrix::new(r.rows() as usize, 1, (r.r0..=r.r1).map(|x| Value::Num(x as f64 + 1.0)).collect())) }, _ => err(ErrKind::Value) }),
    e("COLUMN", 0, 1, |c, a| match a.first() { None => num(c.pos.col as f64 + 1.0), Some(Arg::R(_, r)) => if r.cols() == 1 { num(r.c0 as f64 + 1.0) } else { Value::array(Matrix::new(1, r.cols() as usize, (r.c0..=r.c1).map(|x| Value::Num(x as f64 + 1.0)).collect())) }, _ => err(ErrKind::Value) }),
    e("ROWS", 1, 1, |c, a| match &a[0] { Arg::R(_, r) => num(r.rows() as f64), x => num(c.matrix_of(x).rows as f64) }),
    e("COLUMNS", 1, 1, |c, a| match &a[0] { Arg::R(_, r) => num(r.cols() as f64), x => num(c.matrix_of(x).cols as f64) }),
    e("AREAS", 1, 1, |_, a| match &a[0] { Arg::Areas(l) => num(l.len() as f64), Arg::R(..) => num(1.0), _ => err(ErrKind::Value) }),
    e("ADDRESS", 2, 5, address),
    e("TRANSPOSE", 1, 1, |c, a| Value::array(c.matrix_of(&a[0]).transpose())),
    e("FILTER", 2, 3, filter), e("SORT", 1, 4, sort), e("SORTBY", 2, MANY, sortby), e("UNIQUE", 1, 3, unique),
    e("SEQUENCE", 1, 4, sequence), e("RANDARRAY", 0, 5, randarray),
    e("HSTACK", 1, MANY, |c, a| stack(c, a, false)), e("VSTACK", 1, MANY, |c, a| stack(c, a, true)),
    e("TAKE", 2, 3, |c, a| take_drop(c, a, true)), e("DROP", 2, 3, |c, a| take_drop(c, a, false)),
    e("CHOOSECOLS", 2, MANY, |c, a| choose_lines(c, a, false)), e("CHOOSEROWS", 2, MANY, |c, a| choose_lines(c, a, true)),
    e("TOCOL", 1, 3, |c, a| to_line(c, a, true)), e("TOROW", 1, 3, |c, a| to_line(c, a, false)),
    e("WRAPROWS", 2, 3, |c, a| wrap(c, a, true)), e("WRAPCOLS", 2, 3, |c, a| wrap(c, a, false)),
    e("EXPAND", 2, 4, expand),
    e("HYPERLINK", 1, 2, |c, a| { let v = if a.len() > 1 { c.scalar_of(&a[1]) } else { c.scalar_of(&a[0]) }; v }),
];

/// Where `key` sits in `list`: exact (with wildcards for text when `wild`), or the largest
/// value not above it in a sorted list (`approx`).
fn find_in(list: &[Value], key: &Value, approx: bool, wild: bool) -> Option<usize> {
    if approx {
        // Binary search as Excel does (it assumes ascending order), skipping other types.
        let (mut lo, mut hi) = (0usize, list.len());
        let mut best = None;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let v = &list[mid];
            let same_type = std::mem::discriminant(v) == std::mem::discriminant(key) || (matches!(v, Value::Num(_)) && matches!(key, Value::Num(_)));
            if !same_type {
                // Step past mismatched types linearly.
                let mut k = mid; while k < hi && std::mem::discriminant(&list[k]) != std::mem::discriminant(key) { k += 1; }
                if k == hi { hi = mid; continue; }
                if compare(&list[k], key) != Ordering::Greater { best = Some(k); lo = k + 1; } else { hi = mid; }
                continue;
            }
            if compare(v, key) != Ordering::Greater { best = Some(mid); lo = mid + 1; } else { hi = mid; }
        }
        return best;
    }
    if let (true, Value::Str(k)) = (wild, key) {
        if k.contains(['*', '?', '~']) { return list.iter().position(|v| matches!(v, Value::Str(s) if matches_pattern(k, s))); }
    }
    list.iter().position(|v| same(v, key))
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) { (Value::Num(x), Value::Num(y)) => x == y, (Value::Str(x), Value::Str(y)) => x.to_lowercase() == y.to_lowercase(), (Value::Bool(x), Value::Bool(y)) => x == y, _ => false }
}

fn vlookup(ctx: &mut Ctx, a: &[Arg]) -> Value { vh_lookup(ctx, a, true) }
fn hlookup(ctx: &mut Ctx, a: &[Arg]) -> Value { vh_lookup(ctx, a, false) }
fn vh_lookup(ctx: &mut Ctx, a: &[Arg], vertical: bool) -> Value {
    let keyv = ctx.value_of(&a[0]);
    let m = ctx.matrix_of(&a[1]);
    let m = if vertical { m } else { m.transpose() };
    let col = tryv!(arg_num(ctx, a, 2, 0.0)).trunc() as usize;
    let approx = tryv!(arg_bool(ctx, a, 3, true));
    if col < 1 { return err(ErrKind::Value); }
    if col > m.cols { return err(ErrKind::Ref); }
    let keys: Vec<Value> = (0..m.rows).map(|r| m.get(r, 0).clone()).collect();
    let one = |k: &Value| -> Value {
        if let Value::Err(e) = k { return Value::Err(*e); }
        match find_in(&keys, k, approx, true) { Some(r) => { let v = m.get(r, col - 1).clone(); if v.is_empty() { Value::Num(0.0) } else { v } } None => err(ErrKind::NA) }
    };
    match &keyv { Value::Array(km) => Value::array(Matrix::new(km.rows, km.cols, km.data.iter().map(one).collect())), k => one(k) }
}

fn match_(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let key = ctx.scalar_of(&a[0]);
    let list = flat(ctx, &a[1]);
    let mode = tryv!(arg_num(ctx, a, 2, 1.0));
    if let Value::Err(e) = key { return Value::Err(e); }
    let hit = if mode == 0.0 { find_in(&list, &key, false, true) } else if mode > 0.0 { find_in(&list, &key, true, false) } else {
        // Descending: the smallest value not below the key.
        let mut best = None;
        for (i, v) in list.iter().enumerate() { if std::mem::discriminant(v) == std::mem::discriminant(&key) { if compare(v, &key) != Ordering::Less { best = Some(i); } else { break; } } }
        best
    };
    hit.map_or(err(ErrKind::NA), |i| num(i as f64 + 1.0))
}

/// XMATCH/XLOOKUP's search: match_mode 0 exact, -1 exact or next smaller, 1 exact or next
/// larger, 2 wildcard; search_mode 1 first, -1 last, 2/-2 binary.
fn xfind(list: &[Value], key: &Value, mode: i64, search: i64) -> Option<usize> {
    let order: Box<dyn Iterator<Item = usize>> = if search < 0 { Box::new((0..list.len()).rev()) } else { Box::new(0..list.len()) };
    let mut best: Option<usize> = None;
    for i in order {
        let v = &list[i];
        if mode == 2 { if let (Value::Str(k), Value::Str(s)) = (key, v) { if matches_pattern(k, s) { return Some(i); } } else if same(v, key) { return Some(i); } continue; }
        if same(v, key) { return Some(i); }
        if mode == 0 || std::mem::discriminant(v) != std::mem::discriminant(key) { continue; }
        let o = compare(v, key);
        let better = |b: usize| { let c = compare(v, &list[b]); if mode < 0 { c == Ordering::Greater } else { c == Ordering::Less } };
        if (mode < 0 && o == Ordering::Less) || (mode > 0 && o == Ordering::Greater) { if best.is_none_or(better) { best = Some(i); } }
    }
    best
}

fn xmatch(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let key = ctx.scalar_of(&a[0]);
    let list = flat(ctx, &a[1]);
    let mode = tryv!(arg_num(ctx, a, 2, 0.0)) as i64; let search = tryv!(arg_num(ctx, a, 3, 1.0)) as i64;
    xfind(&list, &key, mode, search).map_or(err(ErrKind::NA), |i| num(i as f64 + 1.0))
}

fn xlookup(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let keyv = ctx.value_of(&a[0]);
    let look = ctx.matrix_of(&a[1]);
    let ret = ctx.matrix_of(&a[2]);
    let mode = tryv!(arg_num(ctx, a, 4, 0.0)) as i64; let search = tryv!(arg_num(ctx, a, 5, 1.0)) as i64;
    let vertical = look.cols == 1;
    if (vertical && ret.rows != look.rows) || (!vertical && ret.cols != look.cols) { return err(ErrKind::Value); }
    let list = look.data.clone();
    let missing = a.get(3).filter(|x| !x.is_missing()).map(|x| ctx.value_of(x));
    let one = |k: &Value| -> Value {
        match xfind(&list, k, mode, search) {
            Some(i) => if vertical { Value::array(Matrix::new(1, ret.cols, ret.row(i).to_vec())) } else { Value::array(Matrix::new(ret.rows, 1, (0..ret.rows).map(|r| ret.get(r, i).clone()).collect())) },
            None => missing.clone().unwrap_or(err(ErrKind::NA)),
        }
    };
    let fix = |v: Value| if v.is_empty() { Value::Num(0.0) } else { v };
    match &keyv { Value::Array(km) => Value::array(Matrix::new(km.rows, km.cols, km.data.iter().map(|k| fix(one(k).scalar())).collect())), k => match one(k) { Value::Array(m) => Value::Array(m), v => fix(v) } }
}

fn lookup(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let key = ctx.scalar_of(&a[0]);
    let m = ctx.matrix_of(&a[1]);
    let (keys, res): (Vec<Value>, Vec<Value>) = if a.len() == 3 { (m.data.clone(), ctx.matrix_of(&a[2]).data) } else if m.rows >= m.cols {
        ((0..m.rows).map(|r| m.get(r, 0).clone()).collect(), (0..m.rows).map(|r| m.get(r, m.cols - 1).clone()).collect())
    } else { (m.row(0).to_vec(), m.row(m.rows - 1).to_vec()) };
    find_in(&keys, &key, true, false).and_then(|i| res.get(i).cloned()).unwrap_or(err(ErrKind::NA))
}

fn index(ctx: &mut Ctx, a: &[Expr]) -> Arg {
    let src = ctx.arg(&a[0]);
    let row = if a.len() > 1 { ctx.eval(&a[1]).scalar() } else { Value::Empty };
    let col = if a.len() > 2 { ctx.eval(&a[2]).scalar() } else { Value::Empty };
    let area = if a.len() > 3 { ctx.eval(&a[3]).scalar().to_num().unwrap_or(1.0) as usize } else { 1 };
    let (r, c) = match (row.to_num(), col.to_num()) { (Ok(r), Ok(c)) => (r.trunc() as i64, c.trunc() as i64), (Err(e), _) | (_, Err(e)) => return Arg::V(Value::Err(e)) };
    if r < 0 || c < 0 { return Arg::V(err(ErrKind::Value)); }
    let src = match src { Arg::Areas(l) => match l.get(area.wrapping_sub(1)) { Some((s, rg)) => Arg::R(*s, *rg), None => return Arg::V(err(ErrKind::Ref)) }, s => s };
    match src {
        Arg::R(s, rg) => {
            let (r, c) = if rg.rows() == 1 && a.len() == 2 { (0, r) } else { (r, c) };
            let r0 = if r == 0 { rg.r0 } else { rg.r0 + r as u32 - 1 }; let r1 = if r == 0 { rg.r1 } else { r0 };
            let c0 = if c == 0 { rg.c0 } else { rg.c0 + c as u32 - 1 }; let c1 = if c == 0 { rg.c1 } else { c0 };
            if r1 > rg.r1 || c1 > rg.c1 { return Arg::V(err(ErrKind::Ref)); }
            Arg::R(s, Range { r0, c0, r1, c1 })
        }
        Arg::V(Value::Err(e)) => Arg::V(Value::Err(e)),
        other => {
            let m = ctx.matrix_of(&other);
            let (r, c) = if m.rows == 1 && a.len() == 2 { (1, r) } else if m.cols == 1 && a.len() == 2 { (r, 1) } else { (r, c) };
            if r as usize > m.rows || c as usize > m.cols { return Arg::V(err(ErrKind::Ref)); }
            Arg::V(match (r, c) {
                (0, 0) => Value::Array(std::sync::Arc::new(m)),
                (0, c) => Value::array(Matrix::new(m.rows, 1, (0..m.rows).map(|i| m.get(i, c as usize - 1).clone()).collect())),
                (r, 0) => Value::array(Matrix::new(1, m.cols, m.row(r as usize - 1).to_vec())),
                (r, c) => m.get(r as usize - 1, c as usize - 1).clone(),
            })
        }
    }
}

fn offset(ctx: &mut Ctx, a: &[Expr]) -> Arg {
    let Arg::R(s, base) = ctx.arg(&a[0]) else { return Arg::V(err(ErrKind::Value)) };
    let get = |ctx: &mut Ctx, i: usize, d: f64| -> Result<f64, ErrKind> { match a.get(i) { None | Some(Expr::Missing) => Ok(d), Some(e) => ctx.eval(e).scalar().to_num() } };
    let (dr, dc) = match (get(ctx, 1, 0.0), get(ctx, 2, 0.0)) { (Ok(x), Ok(y)) => (x.trunc() as i64, y.trunc() as i64), (Err(e), _) | (_, Err(e)) => return Arg::V(Value::Err(e)) };
    let (h, w) = match (get(ctx, 3, base.rows() as f64), get(ctx, 4, base.cols() as f64)) { (Ok(x), Ok(y)) => (x.trunc() as i64, y.trunc() as i64), (Err(e), _) | (_, Err(e)) => return Arg::V(Value::Err(e)) };
    if h == 0 || w == 0 { return Arg::V(err(ErrKind::Ref)); }
    let r0 = base.r0 as i64 + dr; let c0 = base.c0 as i64 + dc;
    let (r0, r1) = if h > 0 { (r0, r0 + h - 1) } else { (r0 + h + 1, r0) };
    let (c0, c1) = if w > 0 { (c0, c0 + w - 1) } else { (c0 + w + 1, c0) };
    if r0 < 0 || c0 < 0 || r1 >= MAX_ROWS as i64 || c1 >= MAX_COLS as i64 { return Arg::V(err(ErrKind::Ref)); }
    Arg::R(s, Range { r0: r0 as u32, c0: c0 as u32, r1: r1 as u32, c1: c1 as u32 })
}

fn indirect(ctx: &mut Ctx, a: &[Expr]) -> Arg {
    let t = match ctx.eval(&a[0]).scalar().to_text() { Ok(t) => t, Err(e) => return Arg::V(Value::Err(e)) };
    let a1 = a.get(1).map(|e| ctx.eval(e).scalar().to_bool().unwrap_or(true)).unwrap_or(true);
    let (sheet, body) = match t.rsplit_once('!') { Some((s, b)) => (Some(s.trim_matches('\'').replace("''", "'")), b.to_string()), None => (None, t.clone()) };
    let si = match &sheet { Some(n) => match ctx.book.sheet_index(n) { Some(i) => i, None => return Arg::V(err(ErrKind::Ref)) }, None => ctx.sheet };
    let range = if a1 { crate::addr::parse_range(&body) } else { r1c1(&body, ctx.pos) };
    match range {
        Some(r) => Arg::R(si, r),
        None => {
            // A defined name.
            match ctx.book.name_index(&body, Some(si)) { Some(_) => ctx.arg(&Expr::Name { name: crate::formula::lexer::upper_name(&body), sheet }), None => Arg::V(err(ErrKind::Ref)) }
        }
    }
}

fn r1c1(s: &str, at: Pos) -> Option<Range> {
    let one = |s: &str| -> Option<Pos> {
        let u = s.to_ascii_uppercase();
        let (rp, cp) = u.strip_prefix('R')?.split_once('C')?;
        let part = |p: &str, base: u32| -> Option<u32> {
            if p.is_empty() { return Some(base); }
            if let Some(x) = p.strip_prefix('[').and_then(|x| x.strip_suffix(']')) { let d: i64 = x.parse().ok()?; return u32::try_from(base as i64 + d).ok(); }
            p.parse::<u32>().ok().filter(|v| *v >= 1).map(|v| v - 1)
        };
        Some(Pos::new(part(rp, at.row)?, part(cp, at.col)?))
    };
    match s.split_once(':') { Some((a, b)) => Some(Range::new(one(a)?, one(b)?)), None => one(s).map(Range::cell) }
}

fn address(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let r = tryv!(arg_num(ctx, a, 0, 0.0)) as u32; let c = tryv!(arg_num(ctx, a, 1, 0.0)) as u32;
    let abs = tryv!(arg_num(ctx, a, 2, 1.0)) as u32; let a1 = tryv!(arg_bool(ctx, a, 3, true));
    if r < 1 || c < 1 || r > MAX_ROWS || c > MAX_COLS { return err(ErrKind::Value); }
    let body = if a1 {
        let (ca, ra) = match abs { 1 => ("$", "$"), 2 => ("", "$"), 3 => ("$", ""), _ => ("", "") };
        format!("{ca}{}{ra}{r}", crate::addr::col_name(c - 1))
    } else {
        match abs { 1 => format!("R{r}C{c}"), 2 => format!("R{r}C[{c}]"), 3 => format!("R[{r}]C{c}"), _ => format!("R[{r}]C[{c}]") }
    };
    match a.get(4).filter(|x| !x.is_missing()) { Some(x) => Value::str(format!("{}!{body}", crate::addr::quote_sheet(&t!(ctx.scalar_of(x))))), None => Value::str(body) }
}

fn filter(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let m = ctx.matrix_of(&a[0]);
    let inc = ctx.matrix_of(&a[1]);
    let by_rows = inc.cols == 1 && inc.rows == m.rows;
    let by_cols = inc.rows == 1 && inc.cols == m.cols && !by_rows;
    if !by_rows && !by_cols { return err(ErrKind::Value); }
    let mut keep = vec![];
    for v in &inc.data { match v.to_bool() { Ok(b) => keep.push(b), Err(e) => return Value::Err(e) } }
    let out: Vec<Vec<Value>> = if by_rows { (0..m.rows).filter(|r| keep[*r]).map(|r| m.row(r).to_vec()).collect() } else {
        let t = m.transpose(); let cols: Vec<Vec<Value>> = (0..t.rows).filter(|c| keep[*c]).map(|c| t.row(c).to_vec()).collect();
        if cols.is_empty() { vec![] } else { Matrix::from_rows(cols).transpose().data.chunks(keep.iter().filter(|x| **x).count()).map(|c| c.to_vec()).collect() }
    };
    if out.is_empty() { return a.get(2).map(|x| ctx.value_of(x)).unwrap_or(err(ErrKind::Calc)); }
    Value::array(Matrix::from_rows(out))
}

fn sort(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let m = ctx.matrix_of(&a[0]);
    let idx = tryv!(arg_num(ctx, a, 1, 1.0)) as usize; let asc = tryv!(arg_num(ctx, a, 2, 1.0)) >= 0.0; let by_col = tryv!(arg_bool(ctx, a, 3, false));
    let m = if by_col { m.transpose() } else { m };
    if idx < 1 || idx > m.cols { return err(ErrKind::Value); }
    let mut rows: Vec<Vec<Value>> = (0..m.rows).map(|r| m.row(r).to_vec()).collect();
    rows.sort_by(|x, y| { let o = sort_cmp(&x[idx - 1], &y[idx - 1], false); if asc { o } else { o.reverse() } });
    let out = Matrix::from_rows(rows);
    Value::array(if by_col { out.transpose() } else { out })
}

fn sortby(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let m = ctx.matrix_of(&a[0]);
    let mut keys = vec![];
    for p in a[1..].chunks(2) {
        let k = ctx.matrix_of(&p[0]);
        let asc = p.get(1).map(|x| ctx.scalar_of(x).to_num().unwrap_or(1.0) >= 0.0).unwrap_or(true);
        if k.data.len() != m.rows && k.data.len() != m.cols { return err(ErrKind::Value); }
        keys.push((k.data, asc));
    }
    let by_rows = keys[0].0.len() == m.rows;
    let n = if by_rows { m.rows } else { m.cols };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&x, &y| { for (k, asc) in &keys { let o = sort_cmp(&k[x], &k[y], false); let o = if *asc { o } else { o.reverse() }; if o != Ordering::Equal { return o; } } Ordering::Equal });
    if by_rows { Value::array(Matrix::from_rows(order.iter().map(|&r| m.row(r).to_vec()).collect())) }
    else { let t = m.transpose(); Value::array(Matrix::from_rows(order.iter().map(|&r| t.row(r).to_vec()).collect()).transpose()) }
}

fn unique(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let m = ctx.matrix_of(&a[0]);
    let by_col = tryv!(arg_bool(ctx, a, 1, false)); let once = tryv!(arg_bool(ctx, a, 2, false));
    let m = if by_col { m.transpose() } else { m };
    let rows: Vec<Vec<Value>> = (0..m.rows).map(|r| m.row(r).to_vec()).collect();
    let eq = |x: &[Value], y: &[Value]| x.iter().zip(y).all(|(p, q)| same(p, q) || (p.is_empty() && q.is_empty()));
    let mut out: Vec<Vec<Value>> = vec![];
    for r in &rows {
        let k = rows.iter().filter(|o| eq(o, r)).count();
        if once && k > 1 { continue; }
        if !out.iter().any(|o| eq(o, r)) { out.push(r.clone()); }
    }
    if out.is_empty() { return err(ErrKind::Calc); }
    let out = Matrix::from_rows(out);
    Value::array(if by_col { out.transpose() } else { out })
}

fn sequence(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let rows = tryv!(arg_num(ctx, a, 0, 1.0)) as i64; let cols = tryv!(arg_num(ctx, a, 1, 1.0)) as i64;
    let start = tryv!(arg_num(ctx, a, 2, 1.0)); let step = tryv!(arg_num(ctx, a, 3, 1.0));
    if rows < 1 || cols < 1 { return err(ErrKind::Calc); }
    if rows * cols > 4_000_000 { return err(ErrKind::Num); }
    Value::array(Matrix::new(rows as usize, cols as usize, (0..rows * cols).map(|i| Value::Num(start + step * i as f64)).collect()))
}

fn randarray(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let rows = tryv!(arg_num(ctx, a, 0, 1.0)) as i64; let cols = tryv!(arg_num(ctx, a, 1, 1.0)) as i64;
    let lo = tryv!(arg_num(ctx, a, 2, 0.0)); let hi = tryv!(arg_num(ctx, a, 3, 1.0)); let whole = tryv!(arg_bool(ctx, a, 4, false));
    if rows < 1 || cols < 1 || lo > hi { return err(ErrKind::Value); }
    Value::array(Matrix::new(rows as usize, cols as usize, (0..rows * cols).map(|_| { let x = super::math::rand(); Value::Num(if whole { (lo + (x * (hi - lo + 1.0)).floor()).min(hi) } else { lo + x * (hi - lo) }) }).collect()))
}

fn stack(ctx: &mut Ctx, a: &[Arg], vertical: bool) -> Value {
    let ms: Vec<Matrix> = a.iter().map(|x| { let m = ctx.matrix_of(x); if vertical { m } else { m.transpose() } }).collect();
    let w = ms.iter().map(|m| m.cols).max().unwrap_or(1);
    let mut rows = vec![];
    for m in ms { for r in 0..m.rows { let mut row = m.row(r).to_vec(); row.resize(w, err(ErrKind::NA)); rows.push(row); } }
    let out = Matrix::from_rows(rows);
    Value::array(if vertical { out } else { out.transpose() })
}

fn take_drop(ctx: &mut Ctx, a: &[Arg], take: bool) -> Value {
    let m = ctx.matrix_of(&a[0]);
    let pick = |n: Option<f64>, len: usize| -> (usize, usize) {
        match n { None => (0, len), Some(n) => { let k = (n.abs() as usize).min(len); if take { if n >= 0.0 { (0, k) } else { (len - k, len) } } else if n >= 0.0 { (k, len) } else { (0, len - k) } } }
    };
    let rn = match a.get(1) { Some(x) if !x.is_missing() => Some(tryv!(ctx.scalar_of(x).to_num())), _ => None };
    let cn = match a.get(2) { Some(x) if !x.is_missing() => Some(tryv!(ctx.scalar_of(x).to_num())), _ => None };
    let (r0, r1) = pick(rn, m.rows); let (c0, c1) = pick(cn, m.cols);
    if r1 <= r0 || c1 <= c0 { return err(ErrKind::Calc); }
    Value::array(Matrix::from_rows((r0..r1).map(|r| m.row(r)[c0..c1].to_vec()).collect()))
}

fn choose_lines(ctx: &mut Ctx, a: &[Arg], rows: bool) -> Value {
    let m = ctx.matrix_of(&a[0]);
    let m = if rows { m } else { m.transpose() };
    let mut out = vec![];
    for x in &a[1..] { for v in flat(ctx, x) { let k = tryv!(v.to_num()) as i64; let i = if k > 0 { k - 1 } else { m.rows as i64 + k }; if k == 0 || i < 0 || i >= m.rows as i64 { return err(ErrKind::Value); } out.push(m.row(i as usize).to_vec()); } }
    let o = Matrix::from_rows(out);
    Value::array(if rows { o } else { o.transpose() })
}

fn to_line(ctx: &mut Ctx, a: &[Arg], col: bool) -> Value {
    let m = ctx.matrix_of(&a[0]);
    let ignore = tryv!(arg_num(ctx, a, 1, 0.0)) as i64; let by_col = tryv!(arg_bool(ctx, a, 2, false));
    let m = if by_col { m.transpose() } else { m };
    let v: Vec<Value> = m.data.into_iter().filter(|x| !((ignore == 1 || ignore == 3) && x.is_empty()) && !((ignore == 2 || ignore == 3) && x.is_err())).collect();
    if v.is_empty() { return err(ErrKind::Calc); }
    let n = v.len();
    Value::array(if col { Matrix::new(n, 1, v) } else { Matrix::new(1, n, v) })
}

fn wrap(ctx: &mut Ctx, a: &[Arg], rows: bool) -> Value {
    let v = flat(ctx, &a[0]);
    let k = tryv!(arg_num(ctx, a, 1, 0.0)) as usize;
    if k < 1 { return err(ErrKind::Value); }
    let pad = a.get(2).map(|x| ctx.scalar_of(x)).unwrap_or(err(ErrKind::NA));
    let out: Vec<Vec<Value>> = v.chunks(k).map(|c| { let mut r = c.to_vec(); r.resize(k, pad.clone()); r }).collect();
    let o = Matrix::from_rows(out);
    Value::array(if rows { o } else { o.transpose() })
}

fn expand(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let m = ctx.matrix_of(&a[0]);
    let rows = match a.get(1) { Some(x) if !x.is_missing() => tryv!(ctx.scalar_of(x).to_num()) as usize, _ => m.rows };
    let cols = match a.get(2) { Some(x) if !x.is_missing() => tryv!(ctx.scalar_of(x).to_num()) as usize, _ => m.cols };
    if rows < m.rows || cols < m.cols { return err(ErrKind::Value); }
    let pad = a.get(3).map(|x| ctx.scalar_of(x)).unwrap_or(err(ErrKind::NA));
    Value::array(Matrix::from_rows((0..rows).map(|r| (0..cols).map(|c| if r < m.rows && c < m.cols { m.get(r, c).clone() } else { pad.clone() }).collect()).collect()))
}

/// Structured references (Table1[Amount], [@Amount], Table1[[#Headers],[Qty]:[Price]]).
pub fn table_ref(ctx: &mut Ctx, table: Option<&str>, spec: &str) -> Option<Arg> {
    crate::tables::resolve(ctx.book, ctx.sheet, ctx.pos, table, spec).map(|(s, r)| Arg::R(s, r))
}
