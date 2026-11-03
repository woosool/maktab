//! Logical functions: IF, IFS, IFERROR, AND, OR, SWITCH, CHOOSE, LET, and the LAMBDA helpers
//! (MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY, ISOMITTED).

use super::*;
use crate::formula::eval::compare;

pub const DEFS: &[Def] = &[
    l("IF", 1, 3, if_), l("IFS", 2, MANY, ifs), l("IFERROR", 2, 2, iferror), l("IFNA", 2, 2, ifna),
    e("AND", 1, MANY, and), e("OR", 1, MANY, or), e("XOR", 1, MANY, xor),
    s("NOT", 1, 1, |a| boolv(!b!(a[0]))), s("TRUE", 0, 0, |_| boolv(true)), s("FALSE", 0, 0, |_| boolv(false)),
    l("SWITCH", 3, MANY, switch), r("CHOOSE", 2, MANY, choose), l("LET", 3, MANY, let_),
    e("MAP", 2, MANY, map), e("REDUCE", 3, 3, reduce), e("SCAN", 3, 3, scan), e("BYROW", 2, 2, byrow), e("BYCOL", 2, 2, bycol),
    e("MAKEARRAY", 3, 3, makearray), l("ISOMITTED", 1, 1, isomitted),
];

/// Evaluates a condition (arrays are applied element by element by the caller).
fn if_(ctx: &mut Ctx, a: &[Expr]) -> Value {
    let c = ctx.eval(&a[0]);
    let pick = |ctx: &mut Ctx, i: usize, t: bool| -> Value {
        match a.get(i) { Some(Expr::Missing) => Value::Num(0.0), Some(e) => ctx.eval(e), None => Value::Bool(t) }
    };
    if let Value::Array(m) = &c {
        let (yes, no) = (pick(ctx, 1, true), pick(ctx, 2, false));
        let mut out = Vec::with_capacity(m.data.len());
        for (i, v) in m.data.iter().enumerate() {
            let (r, col) = (i / m.cols, i % m.cols);
            let at = |x: &Value| match x { Value::Array(xm) => xm.get(r, col).clone(), x => x.clone() };
            out.push(match v.to_bool() { Ok(true) => at(&yes), Ok(false) => at(&no), Err(e) => Value::Err(e) });
        }
        return Value::array(Matrix::new(m.rows, m.cols, out));
    }
    match c.to_bool() { Ok(true) => pick(ctx, 1, true), Ok(false) => pick(ctx, 2, false), Err(e) => Value::Err(e) }
}
fn ifs(ctx: &mut Ctx, a: &[Expr]) -> Value {
    if a.len() % 2 != 0 { return err(ErrKind::Value); }
    for p in a.chunks(2) { match ctx.eval(&p[0]).scalar().to_bool() { Ok(true) => return ctx.eval(&p[1]), Ok(false) => {} Err(e) => return Value::Err(e) } }
    err(ErrKind::NA)
}
fn iferror(ctx: &mut Ctx, a: &[Expr]) -> Value {
    let v = ctx.eval(&a[0]);
    if let Value::Array(m) = &v { if m.data.iter().any(|x| x.is_err()) { let alt = ctx.eval(&a[1]); return Value::array(Matrix::new(m.rows, m.cols, m.data.iter().map(|x| if x.is_err() { alt.scalar() } else { x.clone() }).collect())); } }
    if v.is_err() { ctx.eval(&a[1]) } else { v }
}
fn ifna(ctx: &mut Ctx, a: &[Expr]) -> Value {
    let v = ctx.eval(&a[0]);
    if let Value::Array(m) = &v { if m.data.iter().any(|x| x.err() == Some(ErrKind::NA)) { let alt = ctx.eval(&a[1]); return Value::array(Matrix::new(m.rows, m.cols, m.data.iter().map(|x| if x.err() == Some(ErrKind::NA) { alt.scalar() } else { x.clone() }).collect())); } }
    if v.err() == Some(ErrKind::NA) { ctx.eval(&a[1]) } else { v }
}
fn logicals(ctx: &Ctx, a: &[Arg]) -> Result<Vec<bool>, ErrKind> {
    let mut out = vec![]; let mut bad = None;
    for x in a {
        each(ctx, x, &mut |v, direct| {
            if bad.is_some() { return; }
            match v { Value::Bool(b) => out.push(*b), Value::Num(n) => out.push(*n != 0.0), Value::Err(e) => bad = Some(*e),
                Value::Str(_) if direct => match v.to_bool() { Ok(b) => out.push(b), Err(e) => bad = Some(e) }, _ => {} }
        });
    }
    if let Some(e) = bad { return Err(e); }
    if out.is_empty() { return Err(ErrKind::Value); }
    Ok(out)
}
fn and(ctx: &mut Ctx, a: &[Arg]) -> Value { boolv(tryv!(logicals(ctx, a)).iter().all(|x| *x)) }
fn or(ctx: &mut Ctx, a: &[Arg]) -> Value { boolv(tryv!(logicals(ctx, a)).iter().any(|x| *x)) }
fn xor(ctx: &mut Ctx, a: &[Arg]) -> Value { boolv(tryv!(logicals(ctx, a)).iter().filter(|x| **x).count() % 2 == 1) }
fn switch(ctx: &mut Ctx, a: &[Expr]) -> Value {
    let v = ctx.eval(&a[0]).scalar();
    if let Value::Err(e) = v { return Value::Err(e); }
    let rest = &a[1..];
    for p in rest.chunks(2) {
        if p.len() == 1 { return ctx.eval(&p[0]); }
        let k = ctx.eval(&p[0]).scalar();
        if compare(&v, &k) == std::cmp::Ordering::Equal && std::mem::discriminant(&v) == std::mem::discriminant(&k) { return ctx.eval(&p[1]); }
    }
    err(ErrKind::NA)
}
fn choose(ctx: &mut Ctx, a: &[Expr]) -> Arg {
    let i = ctx.eval(&a[0]);
    if let Value::Array(m) = &i {
        let out: Vec<Value> = m.data.iter().map(|x| match x.to_num() { Ok(k) if k >= 1.0 && (k as usize) < a.len() => { let v = ctx.eval(&a[k as usize]); v.scalar() } Ok(_) => err(ErrKind::Value), Err(e) => Value::Err(e) }).collect();
        return Arg::V(Value::array(Matrix::new(m.rows, m.cols, out)));
    }
    match i.to_num() {
        Ok(k) if k >= 1.0 && (k.trunc() as usize) < a.len() => ctx.arg(&a[k.trunc() as usize]),
        Ok(_) => Arg::V(err(ErrKind::Value)),
        Err(e) => Arg::V(Value::Err(e)),
    }
}
fn let_(ctx: &mut Ctx, a: &[Expr]) -> Value {
    if a.len() % 2 == 0 { return err(ErrKind::Value); }
    ctx.scopes.push(vec![]);
    for p in a[..a.len() - 1].chunks(2) {
        let Expr::Name { name, sheet: None } = &p[0] else { ctx.scopes.pop(); return err(ErrKind::Value) };
        let v = ctx.arg(&p[1]);
        // A LET-bound lambda can call itself: give it its own name.
        let v = match v { Arg::F(f) => { let mut l = (*f).clone(); let me = name.clone(); l.captured.push((me.clone(), Arg::F(f.clone()))); Arg::F(std::sync::Arc::new(l)) } v => v };
        ctx.scopes.last_mut().unwrap().push((name.clone(), v));
    }
    let out = ctx.arg(a.last().unwrap());
    let v = match out { Arg::R(s, r) if r.is_cell() => ctx.cell(s, r.start()), o => ctx.value_of(&o) };
    ctx.scopes.pop();
    v
}
fn isomitted(ctx: &mut Ctx, a: &[Expr]) -> Value {
    match &a[0] { Expr::Name { name, sheet: None } => boolv(matches!(ctx.lookup_var(name), Some(Arg::Missing) | None)), _ => boolv(false) }
}

fn func(a: &Arg) -> Option<Arg> { matches!(a, Arg::F(_) | Arg::Builtin(_)).then(|| a.clone()) }

fn map(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let Some(f) = func(a.last().unwrap()) else { return err(ErrKind::Value) };
    let ms: Vec<Matrix> = a[..a.len() - 1].iter().map(|x| ctx.matrix_of(x)).collect();
    let (rows, cols) = (ms[0].rows, ms[0].cols);
    let mut out = Vec::with_capacity(rows * cols);
    for r in 0..rows { for c in 0..cols {
        let vals = ms.iter().map(|m| Arg::V(m.get(r, c).clone())).collect();
        out.push(ctx.apply(&f, vals).scalar());
    } }
    Value::array(Matrix::new(rows, cols, out))
}
fn reduce(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let Some(f) = func(&a[2]) else { return err(ErrKind::Value) };
    let mut acc = ctx.value_of(&a[0]);
    for v in ctx.matrix_of(&a[1]).data { acc = ctx.apply(&f, vec![Arg::V(acc), Arg::V(v)]); }
    acc
}
fn scan(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let Some(f) = func(&a[2]) else { return err(ErrKind::Value) };
    let mut acc = ctx.value_of(&a[0]);
    let m = ctx.matrix_of(&a[1]);
    let mut out = vec![];
    for v in &m.data { acc = ctx.apply(&f, vec![Arg::V(acc), Arg::V(v.clone())]); out.push(acc.scalar()); }
    Value::array(Matrix::new(m.rows, m.cols, out))
}
fn byrow(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let Some(f) = func(&a[1]) else { return err(ErrKind::Value) };
    let m = ctx.matrix_of(&a[0]);
    let out: Vec<Value> = (0..m.rows).map(|r| ctx.apply(&f, vec![Arg::V(Value::array(Matrix::new(1, m.cols, m.row(r).to_vec())))]).scalar()).collect();
    Value::array(Matrix::new(m.rows, 1, out))
}
fn bycol(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let Some(f) = func(&a[1]) else { return err(ErrKind::Value) };
    let m = ctx.matrix_of(&a[0]).transpose();
    let out: Vec<Value> = (0..m.rows).map(|r| ctx.apply(&f, vec![Arg::V(Value::array(Matrix::new(m.cols, 1, m.row(r).to_vec())))]).scalar()).collect();
    Value::array(Matrix::new(1, m.rows, out))
}
fn makearray(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let (rows, cols) = (tryv!(arg_num(ctx, a, 0, 0.0)) as usize, tryv!(arg_num(ctx, a, 1, 0.0)) as usize);
    let Some(f) = func(&a[2]) else { return err(ErrKind::Value) };
    if rows == 0 || cols == 0 || rows * cols > 1_000_000 { return err(ErrKind::Value); }
    let mut out = Vec::with_capacity(rows * cols);
    for r in 0..rows { for c in 0..cols { out.push(ctx.apply(&f, vec![Arg::V(Value::Num(r as f64 + 1.0)), Arg::V(Value::Num(c as f64 + 1.0))]).scalar()); } }
    Value::array(Matrix::new(rows, cols, out))
}
