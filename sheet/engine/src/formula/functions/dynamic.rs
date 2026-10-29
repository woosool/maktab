//! Grouping and trimming dynamic arrays: GROUPBY, PIVOTBY (a LAMBDA or a function named bare
//! summarises each group), PERCENTOF, TRIMRANGE, SINGLE (the saved form of the @ operator).

use super::*;
use crate::addr::Range;
use crate::formula::eval::sort_cmp;
use std::collections::HashMap;

pub const DEFS: &[Def] = &[
    e("GROUPBY", 3, 7, groupby), e("PIVOTBY", 4, 10, pivotby),
    e("PERCENTOF", 2, 2, |c, a| { let p: f64 = tryv!(numbers(c, &a[..1])).iter().sum(); let w: f64 = tryv!(numbers(c, &a[1..])).iter().sum(); if w == 0.0 { err(ErrKind::Div0) } else { num(p / w) } }),
    r("TRIMRANGE", 1, 3, trimrange),
    e("SINGLE", 1, 1, |c, a| match &a[0] { Arg::V(Value::Array(m)) => m.data.first().cloned().unwrap_or(Value::Empty), x => c.scalar_of(x) }),
];

fn blank(v: &Value) -> bool { v.is_empty() || matches!(v, Value::Str(s) if s.is_empty()) }
/// Case-insensitive identity of a value.
fn key(v: &Value) -> String {
    match v { v if blank(v) => String::new(), Value::Str(s) => format!("s:{}", s.to_lowercase()), Value::Num(n) => format!("n:{n}"), Value::Bool(b) => format!("b:{b}"), Value::Err(e) => format!("e:{}", e.text()), _ => String::new() }
}
fn label(v: &Value) -> Value { if v.is_empty() { Value::str("") } else { v.clone() } }

/// The inputs GROUPBY and PIVOTBY share, headers taken off and filtered rows dropped.
struct Inputs { keys: Vec<Vec<Value>>, vals: Vec<Vec<Value>>, cols: Option<Vec<Value>>, nk: usize, nv: usize, key_heads: Vec<Value>, val_heads: Vec<Value>, show: bool }

fn inputs(ctx: &mut Ctx, rf: &Arg, vf: &Arg, cf: Option<&Arg>, heads: Option<&Arg>, filter: Option<&Arg>) -> Result<Inputs, ErrKind> {
    let (k, v) = (ctx.matrix_of(rf), ctx.matrix_of(vf));
    let c = cf.map(|a| ctx.matrix_of(a));
    let rows = |m: &Matrix| (0..m.rows).map(|r| m.row(r).to_vec()).collect::<Vec<_>>();
    let (keys, vals) = (rows(&k), rows(&v));
    if keys.is_empty() || keys.len() != vals.len() || c.as_ref().is_some_and(|c| c.rows != keys.len()) { return Err(ErrKind::Value); }
    let given = heads.filter(|a| !a.is_missing());
    let mut mode = match given { Some(a) => ctx.scalar_of(a).to_num()?.trunc() as i64, None => 0 };
    if given.is_none() && matches!(vals[0].first(), Some(Value::Str(_))) && matches!(vals.get(1).and_then(|r| r.first()), Some(Value::Num(_))) {
        mode = if k.cols > 1 || c.as_ref().is_some_and(|c| c.cols > 1) { 3 } else { 1 };
    }
    if !(0..=3).contains(&mode) { return Err(ErrKind::Value); }
    let has = mode == 1 || mode == 3;
    let (nk, nv) = (k.cols, v.cols);
    let name = |row: &[Value], off: usize| row.iter().enumerate().map(|(i, h)| if has && !blank(h) { h.clone() } else { Value::str(format!("Column{}", off + i + 1)) }).collect::<Vec<_>>();
    let (key_heads, val_heads) = (name(&keys[0], 0), name(&vals[0], nk));
    let start = has as usize;
    let mut keep: Vec<bool> = (0..keys.len()).map(|i| i >= start).collect();
    if let Some(f) = filter.filter(|a| !a.is_missing()) {
        let m = ctx.matrix_of(f);
        let fl: Vec<Value> = (0..m.rows).map(|r| m.get(r, 0).clone()).collect();
        let off = if fl.len() == keys.len() { 0 } else if fl.len() + start == keys.len() { start } else { return Err(ErrKind::Value) };
        for i in start..keep.len() { keep[i] = fl[i - off].to_bool()?; }
    }
    let pick = |l: Vec<Vec<Value>>| l.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(r, _)| r).collect::<Vec<_>>();
    let cols = c.map(|c| (0..c.rows).filter(|&r| keep[r]).map(|r| c.get(r, 0).clone()).collect());
    Ok(Inputs { keys: pick(keys), vals: pick(vals), cols, nk, nv, key_heads, val_heads, show: mode >= 2 })
}

struct Group { keys: Vec<Value>, idx: Vec<usize> }
fn group_rows(keys: &[Vec<Value>]) -> Vec<Group> {
    let mut at: HashMap<String, usize> = HashMap::new(); let mut out: Vec<Group> = vec![];
    for (i, t) in keys.iter().enumerate() {
        let k: String = t.iter().map(key).collect::<Vec<_>>().join("\u{0}");
        let g = *at.entry(k).or_insert_with(|| { out.push(Group { keys: t.clone(), idx: vec![] }); out.len() - 1 });
        out[g].idx.push(i);
    }
    out
}

/// One summary: the function over column j of the value rows at `idx`, as a scalar.
fn summarize(ctx: &mut Ctx, f: &Arg, vals: &[Vec<Value>], idx: &[usize], j: usize) -> Value {
    let part = Value::array(Matrix::new(idx.len(), 1, idx.iter().map(|&i| vals[i][j].clone()).collect()));
    let mut args = vec![Arg::V(part)];
    if matches!(f, Arg::Builtin(n) if n == "PERCENTOF") { args.push(Arg::V(Value::array(Matrix::new(vals.len(), 1, vals.iter().map(|r| r[j].clone()).collect())))); }
    match ctx.apply(f, args) { Value::Array(m) if m.data.len() > 1 => err(ErrKind::Calc), v => v.scalar() }
}

/// Sorts groups by their keys, or by output column |order| (descending when negative).
fn sort_groups(groups: &mut Vec<Group>, order: Option<f64>, nk: usize, vals: Vec<Value>, width: usize) -> Result<(), ErrKind> {
    let by_keys = |a: &Group, b: &Group| { for k in 0..nk { let o = sort_cmp(&label(&a.keys[k]), &label(&b.keys[k]), false); if o.is_ne() { return o; } } std::cmp::Ordering::Equal };
    let col = order.map_or(0, |o| o.trunc().abs() as usize);
    if col == 0 { groups.sort_by(by_keys); return Ok(()); }
    if col > width { return Err(ErrKind::Value); }
    let desc = order.is_some_and(|o| o < 0.0);
    let mut tagged: Vec<(Value, usize)> = vals.into_iter().zip(0..).collect();
    let cmp = |p: &Value, q: &Value| {
        if blank(p) != blank(q) { return if blank(p) { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less }; }
        if p.is_err() != q.is_err() { return if p.is_err() { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less }; }
        if blank(p) || p.is_err() { return std::cmp::Ordering::Equal; }
        let o = sort_cmp(p, q, false); if desc { o.reverse() } else { o }
    };
    tagged.sort_by(|(p, i), (q, j)| cmp(p, q).then_with(|| by_keys(&groups[*i], &groups[*j])));
    let order: Vec<usize> = tagged.into_iter().map(|(_, i)| i).collect();
    let mut taken: Vec<Option<Group>> = groups.drain(..).map(Some).collect();
    groups.extend(order.into_iter().filter_map(|i| taken[i].take()));
    Ok(())
}

fn depth_of(ctx: &Ctx, a: &[Arg], i: usize) -> Result<i64, ErrKind> { let d = arg_num(ctx, a, i, 1.0)?.trunc() as i64; if (-2..=2).contains(&d) { Ok(d) } else { Err(ErrKind::Value) } }
fn order_of(ctx: &Ctx, a: &[Arg], i: usize) -> Result<Option<f64>, ErrKind> { match a.get(i) { None | Some(Arg::Missing) => Ok(None), Some(x) => match ctx.scalar_of(x) { Value::Empty => Ok(None), v => v.to_num().map(Some) } } }

fn groupby(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let f = match &a[2] { x @ (Arg::F(_) | Arg::Builtin(_)) => x.clone(), _ => return err(ErrKind::Value) };
    let g = tryv!(inputs(ctx, &a[0], &a[1], None, a.get(3), a.get(6)));
    let d = tryv!(depth_of(ctx, a, 4)); let order = tryv!(order_of(ctx, a, 5));
    if g.keys.is_empty() { return err(ErrKind::Calc); }
    if g.keys.len().saturating_mul(g.nk + g.nv) > 2_000_000 { return err(ErrKind::Num); }
    let mut groups = group_rows(&g.keys);
    let col = order.map_or(0, |o| o.trunc().abs() as usize);
    let sv: Vec<Value> = groups.iter().map(|grp| if col == 0 || col > g.nk + g.nv { Value::Empty } else if col <= g.nk { grp.keys[col - 1].clone() } else { summarize(ctx, &f, &g.vals, &grp.idx, col - g.nk - 1) }).collect();
    tryv!(sort_groups(&mut groups, order, g.nk, sv, g.nk + g.nv));
    let mut aggs = |ctx: &mut Ctx, idx: &[usize]| (0..g.nv).map(|j| summarize(ctx, &f, &g.vals, idx, j)).collect::<Vec<_>>();
    let blanks = |n: usize| vec![Value::str(""); n];
    let subtotals = d.abs() == 2 && g.nk > 1;
    let top = d < 0;
    let grand = Value::str(if subtotals { "Grand Total" } else { "Total" });
    let all: Vec<usize> = (0..g.keys.len()).collect();
    let mut out: Vec<Vec<Value>> = vec![];
    if g.show { out.push([g.key_heads.clone(), g.val_heads.clone()].concat()); }
    let grand_row = |ctx: &mut Ctx, aggs: &mut dyn FnMut(&mut Ctx, &[usize]) -> Vec<Value>| [vec![grand.clone()], blanks(g.nk - 1), aggs(ctx, &all)].concat();
    if top && d != 0 { let r = grand_row(ctx, &mut aggs); out.push(r); }
    let mut i = 0;
    while i < groups.len() {
        if !subtotals { let r = [groups[i].keys.iter().map(label).collect(), aggs(ctx, &groups[i].idx)].concat(); out.push(r); i += 1; continue; }
        let k0 = key(&groups[i].keys[0]);
        let mut j = i; while j < groups.len() && key(&groups[j].keys[0]) == k0 { j += 1; }
        let idx: Vec<usize> = groups[i..j].iter().flat_map(|g| g.idx.iter().copied()).collect();
        let name = format!("{} Total", groups[i].keys[0].to_text().unwrap_or_default()).trim().to_string();
        let sub = [vec![Value::str(name)], blanks(g.nk - 1), aggs(ctx, &idx)].concat();
        let body: Vec<Vec<Value>> = groups[i..j].iter().map(|grp| [grp.keys.iter().map(label).collect(), aggs(ctx, &grp.idx)].concat()).collect();
        if top { out.push(sub); out.extend(body); } else { out.extend(body); out.push(sub); }
        i = j;
    }
    if !top && d != 0 { let r = grand_row(ctx, &mut aggs); out.push(r); }
    Value::array(Matrix::from_rows(out))
}

fn pivotby(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let f = match &a[3] { x @ (Arg::F(_) | Arg::Builtin(_)) => x.clone(), _ => return err(ErrKind::Value) };
    let g = tryv!(inputs(ctx, &a[0], &a[2], Some(&a[1]), a.get(4), a.get(9)));
    let rd = tryv!(depth_of(ctx, a, 5)); let ro = tryv!(order_of(ctx, a, 6)); let cd = tryv!(depth_of(ctx, a, 7)); let co = tryv!(order_of(ctx, a, 8));
    if g.keys.is_empty() { return err(ErrKind::Calc); }
    let cols: Vec<Vec<Value>> = g.cols.clone().unwrap_or_default().into_iter().map(|v| vec![v]).collect();
    let mut rg = group_rows(&g.keys); let mut cg = group_rows(&cols);
    if rg.len().saturating_mul(cg.len()) > 1_000_000 { return err(ErrKind::Num); }
    let (rt, ct) = (rd != 0, cd != 0);
    let cell_idx = |r: &Group, c: &Group| -> Vec<usize> { let set: std::collections::HashSet<usize> = c.idx.iter().copied().collect(); r.idx.iter().copied().filter(|i| set.contains(i)).collect() };
    let cell = |ctx: &mut Ctx, r: &Group, c: &Group| { let idx = cell_idx(r, c); if idx.is_empty() { Value::str("") } else { summarize(ctx, &f, &g.vals, &idx, 0) } };
    // Row sort values: a key column, a column group's cell, or the row total.
    let rcol = ro.map_or(0, |o| o.trunc().abs() as usize);
    let rw = g.nk + cg.len() + ct as usize;
    let sv: Vec<Value> = rg.iter().map(|grp| if rcol == 0 || rcol > rw { Value::Empty } else if rcol <= g.nk { grp.keys[rcol - 1].clone() } else if rcol - g.nk - 1 < cg.len() { cell(ctx, grp, &cg[rcol - g.nk - 1]) } else { summarize(ctx, &f, &g.vals, &grp.idx, 0) }).collect();
    tryv!(sort_groups(&mut rg, ro, g.nk, sv, rw));
    let ccol = co.map_or(0, |o| o.trunc().abs() as usize);
    let cw = 1 + rg.len() + rt as usize;
    let sv: Vec<Value> = cg.iter().map(|grp| if ccol == 0 || ccol > cw { Value::Empty } else if ccol == 1 { grp.keys[0].clone() } else if ccol - 2 < rg.len() { cell(ctx, &rg[ccol - 2], grp) } else { summarize(ctx, &f, &g.vals, &grp.idx, 0) }).collect();
    tryv!(sort_groups(&mut cg, co, 1, sv, cw));
    let mut out: Vec<Vec<Value>> = vec![];
    let mut head: Vec<Value> = if g.show { g.key_heads.iter().map(label).collect() } else { vec![Value::str(""); g.nk] };
    head.extend(cg.iter().map(|c| label(&c.keys[0]))); if ct { head.push(Value::str("Total")); }
    out.push(head);
    let all: Vec<usize> = (0..g.keys.len()).collect();
    let total_row = |ctx: &mut Ctx| { let mut row = vec![Value::str("Total")]; row.extend(vec![Value::str(""); g.nk - 1]); for c in &cg { row.push(summarize(ctx, &f, &g.vals, &c.idx, 0)); } if ct { row.push(summarize(ctx, &f, &g.vals, &all, 0)); } row };
    if rt && rd < 0 { let r = total_row(ctx); out.push(r); }
    for r in &rg {
        let mut row: Vec<Value> = r.keys.iter().map(label).collect();
        for c in &cg { row.push(cell(ctx, r, c)); }
        if ct { row.push(summarize(ctx, &f, &g.vals, &r.idx, 0)); }
        out.push(row);
    }
    if rt && rd > 0 { let r = total_row(ctx); out.push(r); }
    Value::array(Matrix::from_rows(out))
}

/// TRIMRANGE: drops blank rows/columns at the edges (1 leading, 2 trailing, 3 both); a reference stays one.
fn trimrange(ctx: &mut Ctx, a: &[Expr]) -> Arg {
    let arg = ctx.arg(&a[0]);
    let mode = |ctx: &mut Ctx, i: usize| -> Result<u8, ErrKind> { match a.get(i) { None | Some(Expr::Missing) => Ok(3), Some(e) => { let v = ctx.eval(e); let m = if v.is_empty() { 3.0 } else { v.scalar().to_num()?.trunc() }; if (0.0..=3.0).contains(&m) { Ok(m as u8) } else { Err(ErrKind::Value) } } } };
    let (tr, tc) = match (mode(ctx, 1), mode(ctx, 2)) { (Ok(r), Ok(c)) => (r, c), (Err(e), _) | (_, Err(e)) => return Arg::V(err(e)) };
    let m = ctx.matrix_of(&arg);
    let row_used = |r: usize| m.row(r).iter().any(|v| !v.is_empty());
    let col_used = |c: usize| (0..m.rows).any(|r| !m.get(r, c).is_empty());
    let (mut r0, mut r1, mut c0, mut c1) = (0usize, m.rows, 0usize, m.cols);
    if tr & 1 != 0 { while r0 < r1 && !row_used(r0) { r0 += 1; } }
    if tr & 2 != 0 { while r1 > r0 && !row_used(r1 - 1) { r1 -= 1; } }
    if tc & 1 != 0 { while c0 < c1 && !col_used(c0) { c0 += 1; } }
    if tc & 2 != 0 { while c1 > c0 && !col_used(c1 - 1) { c1 -= 1; } }
    if r0 >= r1 || c0 >= c1 { return Arg::V(err(ErrKind::Calc)); }
    if let Arg::R(s, rg) = arg {
        // The read matrix may be clipped to the used area; offsets are from the range's corner.
        return Arg::R(s, Range { r0: rg.r0 + r0 as u32, c0: rg.c0 + c0 as u32, r1: rg.r0 + r1 as u32 - 1, c1: rg.c0 + c1 as u32 - 1 });
    }
    Arg::V(Value::array(Matrix::new(r1 - r0, c1 - c0, (r0..r1).flat_map(|r| m.row(r)[c0..c1].to_vec()).collect())))
}
