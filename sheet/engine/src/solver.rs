//! Solver (solver.js, solver-lp.js, solver-nlp.js): a sheet's optimization model, kept in hidden
//! sheet-scoped names as Excel keeps it (solver_opt, solver_typ, solver_adj, solver_lhs1 …), solved
//! by a bounded two-phase primal simplex (Simplex LP) or an augmented-Lagrangian projected
//! quasi-Newton method (GRG Nonlinear), with branch and bound for integer, binary and
//! AllDifferent constraints. Evolutionary runs the nonlinear method from several seeded starts.

use crate::addr::{parse_range, quote_sheet, Pos, Range};
use crate::book::{DefinedName, Input, Workbook};
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goal { Max, Min, Value(f64) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rel { Le, Eq, Ge, Int, Bin, Dif }

impl Rel {
    pub const ALL: [Rel; 6] = [Rel::Le, Rel::Eq, Rel::Ge, Rel::Int, Rel::Bin, Rel::Dif];
    pub fn label(self) -> &'static str { match self { Rel::Le => "<=", Rel::Eq => "=", Rel::Ge => ">=", Rel::Int => "int", Rel::Bin => "bin", Rel::Dif => "dif" } }
    fn code(self) -> u32 { Rel::ALL.iter().position(|r| *r == self).unwrap() as u32 + 1 }
    fn word(self) -> Option<&'static str> { match self { Rel::Int => Some("integer"), Rel::Bin => Some("binary"), Rel::Dif => Some("AllDifferent"), _ => None } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine { Grg, Simplex, Evolutionary }

impl Engine {
    pub const ALL: [(Engine, &'static str); 3] = [(Engine::Grg, "GRG Nonlinear"), (Engine::Simplex, "Simplex LP"), (Engine::Evolutionary, "Evolutionary")];
    fn code(self) -> u32 { match self { Engine::Grg => 1, Engine::Simplex => 2, Engine::Evolutionary => 3 } }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Constraint { pub lhs: String, pub rel: Rel, pub rhs: String }

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub objective: String,
    pub goal: Goal,
    /// The variable cells: ranges separated by commas.
    pub vars: String,
    pub constraints: Vec<Constraint>,
    pub nonneg: bool,
    pub engine: Engine,
    pub precision: f64,
    pub max_iter: u32,
}

impl Default for Model {
    fn default() -> Self { Model { objective: String::new(), goal: Goal::Max, vars: String::new(), constraints: vec![], nonneg: true, engine: Engine::Grg, precision: 1e-6, max_iter: 1000 } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status { Optimal, Converged, Infeasible, Unbounded, IterationLimit, NotLinear, Error }

impl Status {
    pub fn message(self) -> &'static str {
        match self {
            Status::Optimal => "Solver found a solution. All Constraints and optimality conditions are satisfied.",
            Status::Converged => "Solver has converged to the current solution. All Constraints are satisfied.",
            Status::Infeasible => "Solver could not find a feasible solution.",
            Status::Unbounded => "The Objective Cell values do not converge.",
            Status::IterationLimit => "The maximum iteration limit was reached; continue anyway?",
            Status::NotLinear => "The linearity conditions required by this LP Solver are not satisfied.",
            Status::Error => "Solver encountered an error value in the Objective Cell or a Constraint cell.",
        }
    }
    pub fn found(self) -> bool { matches!(self, Status::Optimal | Status::Converged) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outcome { pub status: Status, pub x: Vec<f64>, pub objective: f64, pub original: Vec<f64> }

// ---- the model in names ------------------------------------------------------------------------

fn strip_sheet(t: &str) -> &str { t.rsplit_once('!').map_or(t, |(_, r)| r) }
fn range_of(t: &str) -> Option<Range> { parse_range(&strip_sheet(t.trim().trim_start_matches('=')).replace('$', "")) }
fn abs(r: Range) -> String {
    let a = |p: Pos| format!("${}${}", crate::addr::col_name(p.col), p.row + 1);
    if r.is_cell() { a(r.start()) } else { format!("{}:{}", a(r.start()), a(r.end())) }
}
/// The cells a ranges text (A1:B2,C4) names.
pub fn cells_of(t: &str) -> Option<Vec<Pos>> {
    let mut v = vec![];
    for part in t.split(',').filter(|p| !p.trim().is_empty()) { v.extend(range_of(part)?.iter()); }
    (!v.is_empty()).then_some(v)
}

/// Reads a sheet's model from its solver_ names.
pub fn load(wb: &Workbook, sheet: usize) -> Model {
    let get = |n: &str| wb.names.iter().find(|d| d.sheet == Some(sheet) && d.name.eq_ignore_ascii_case(n)).map(|d| d.formula.trim_start_matches('=').to_string());
    let num = |n: &str| get(n).and_then(|t| t.trim().parse::<f64>().ok());
    let mut m = Model::default();
    let tidy = |t: String| t.split(',').map(|p| strip_sheet(p).replace('$', "")).collect::<Vec<_>>().join(",");
    if let Some(t) = get("solver_opt") { m.objective = tidy(t); }
    m.goal = match num("solver_typ").unwrap_or(1.0) as u32 { 2 => Goal::Min, 3 => Goal::Value(num("solver_val").unwrap_or(0.0)), _ => Goal::Max };
    if let Some(t) = get("solver_adj") { m.vars = tidy(t); }
    for i in 1..=num("solver_num").unwrap_or(0.0) as usize {
        let Some(lhs) = get(&format!("solver_lhs{i}")) else { continue };
        let rel = Rel::ALL.get((num(&format!("solver_rel{i}")).unwrap_or(1.0) as usize).saturating_sub(1)).copied().unwrap_or(Rel::Le);
        let rhs = get(&format!("solver_rhs{i}")).unwrap_or_default();
        let rhs = if rel.word().is_some() { rel.word().unwrap().to_string() } else if rhs.parse::<f64>().is_ok() { rhs } else { tidy(rhs) };
        m.constraints.push(Constraint { lhs: tidy(lhs), rel, rhs });
    }
    m.engine = match num("solver_eng").unwrap_or(1.0) as u32 { 2 => Engine::Simplex, 3 => Engine::Evolutionary, _ => Engine::Grg };
    m.nonneg = num("solver_neg").unwrap_or(1.0) == 1.0;
    if let Some(p) = num("solver_pre") { m.precision = p; }
    if let Some(i) = num("solver_itr") { m.max_iter = i as u32; }
    m
}

/// Saves a model into the sheet's hidden solver_ names.
pub fn save(wb: &mut Workbook, sheet: usize, m: &Model) {
    wb.names.retain(|d| !(d.sheet == Some(sheet) && d.name.to_ascii_lowercase().starts_with("solver_")));
    let q = quote_sheet(&wb.sheets[sheet].name);
    let qual = |t: &str| t.split(',').filter_map(|p| range_of(p).map(|r| format!("{q}!{}", abs(r)))).collect::<Vec<_>>().join(",");
    let mut put = |n: &str, v: String| wb.names.push(DefinedName { name: n.into(), sheet: Some(sheet), formula: v, hidden: true, comment: String::new() });
    if !m.objective.is_empty() { put("solver_opt", qual(&m.objective)); }
    put("solver_typ", match m.goal { Goal::Max => "1", Goal::Min => "2", Goal::Value(_) => "3" }.into());
    put("solver_val", match m.goal { Goal::Value(v) => crate::value::fmt_num(v), _ => "0".into() });
    if !m.vars.is_empty() { put("solver_adj", qual(&m.vars)); }
    put("solver_num", m.constraints.len().to_string());
    for (i, c) in m.constraints.iter().enumerate() {
        put(&format!("solver_lhs{}", i + 1), qual(&c.lhs));
        put(&format!("solver_rel{}", i + 1), c.rel.code().to_string());
        let rhs = if let Some(w) = c.rel.word() { format!("\"{w}\"") } else if c.rhs.trim().parse::<f64>().is_ok() { c.rhs.trim().to_string() } else { qual(&c.rhs) };
        put(&format!("solver_rhs{}", i + 1), rhs);
    }
    put("solver_eng", m.engine.code().to_string());
    put("solver_neg", if m.nonneg { "1" } else { "2" }.into());
    put("solver_pre", crate::value::fmt_num(m.precision));
    put("solver_itr", m.max_iter.to_string());
    put("solver_ver", "3".into());
}

// ---- the problem the methods see ---------------------------------------------------------------

struct Problem {
    sheet: usize,
    obj: Pos,
    vars: Vec<Pos>,
    /// Constraint rows: (lhs cell, relation, rhs: a constant or a cell).
    rows: Vec<(Pos, Rel, Result<f64, Pos>)>,
    lb: Vec<f64>,
    ub: Vec<f64>,
    integer: Vec<bool>,
    groups: Vec<Vec<usize>>,
    sense: f64,
    target: Option<f64>,
}

fn build(wb: &Workbook, sheet: usize, m: &Model) -> Result<Problem, String> {
    let obj = range_of(&m.objective).filter(|r| r.is_cell()).map(|r| r.start()).ok_or("The Objective Cell must be a single cell on the active sheet.")?;
    if !wb.sheets[sheet].cells.get(&obj).is_some_and(|c| c.is_formula()) { return Err("Objective Cell must contain a formula.".into()); }
    let vars = cells_of(&m.vars).ok_or("The variable cells are not valid.")?;
    if vars.iter().any(|p| wb.sheets[sheet].cells.get(p).is_some_and(|c| c.is_formula())) { return Err("Variable cells must be constants, not formulas.".into()); }
    let n = vars.len();
    let mut p = Problem { sheet, obj, rows: vec![], lb: vec![if m.nonneg { 0.0 } else { f64::NEG_INFINITY }; n], ub: vec![f64::INFINITY; n], integer: vec![false; n], groups: vec![], sense: if m.goal == Goal::Max { -1.0 } else { 1.0 }, target: None, vars };
    if let Goal::Value(v) = m.goal { p.target = Some(v); p.sense = 0.0; }
    for c in &m.constraints {
        let lhs = cells_of(&c.lhs).ok_or_else(|| format!("The constraint reference {} is not valid.", c.lhs))?;
        match c.rel {
            Rel::Int | Rel::Bin | Rel::Dif => {
                let idx: Vec<usize> = lhs.iter().map(|q| p.vars.iter().position(|v| v == q).ok_or("Integer, binary and AllDifferent constraints may apply only to variable cells.")).collect::<Result<_, _>>()?;
                for &j in &idx {
                    p.integer[j] = true;
                    if c.rel == Rel::Bin { p.lb[j] = p.lb[j].max(0.0); p.ub[j] = p.ub[j].min(1.0); }
                    if c.rel == Rel::Dif { p.lb[j] = p.lb[j].max(1.0); p.ub[j] = p.ub[j].min(idx.len() as f64); }
                }
                if c.rel == Rel::Dif { p.groups.push(idx); }
            }
            rel => {
                let rhs: Vec<Result<f64, Pos>> = if let Ok(v) = c.rhs.trim().trim_start_matches('=').parse::<f64>() { vec![Ok(v); lhs.len()] } else {
                    let r = cells_of(&c.rhs).ok_or_else(|| format!("The constraint value {} is not valid.", c.rhs))?;
                    if r.len() == 1 { vec![Err(r[0]); lhs.len()] } else if r.len() == lhs.len() { r.into_iter().map(Err).collect() } else { return Err("The constraint's two sides must be the same size.".into()); }
                };
                // A plain bound on one variable tightens its bounds instead of adding a row.
                for (q, r) in lhs.into_iter().zip(rhs) {
                    if let (Some(j), Ok(v)) = (p.vars.iter().position(|x| *x == q), r) {
                        match rel { Rel::Le => p.ub[j] = p.ub[j].min(v), Rel::Ge => p.lb[j] = p.lb[j].max(v), _ => { p.lb[j] = p.lb[j].max(v); p.ub[j] = p.ub[j].min(v); } }
                        continue;
                    }
                    p.rows.push((q, rel, r));
                }
            }
        }
    }
    Ok(p)
}

/// One evaluation: the minimized objective and each row's lhs - rhs.
fn evaluate(wb: &mut Workbook, p: &Problem, x: &[f64]) -> Option<(f64, Vec<f64>)> {
    for (q, v) in p.vars.iter().zip(x) { let c = wb.sheets[p.sheet].cell_mut(*q); c.input = Input::Value(Value::Num(*v)); c.value = Value::Num(*v); }
    wb.recalc();
    let num = |q: Pos| wb.sheets[p.sheet].value(q).to_num().ok().filter(|v| v.is_finite());
    let o = num(p.obj)?;
    let f = if p.target.is_some() { 0.0 } else { p.sense * o };
    let mut g = Vec::with_capacity(p.rows.len() + 1);
    for (q, _, r) in &p.rows { g.push(num(*q)? - match r { Ok(v) => *v, Err(c) => num(*c)? }); }
    if let Some(t) = p.target { g.push(o - t); }
    Some((f, g))
}

fn rels(p: &Problem) -> Vec<Rel> { let mut v: Vec<Rel> = p.rows.iter().map(|r| r.1).collect(); if p.target.is_some() { v.push(Rel::Eq); } v }

/// Solves the sheet's model. With `keep` the solution stays in the variable cells, otherwise the
/// original values are restored.
pub fn solve(wb: &mut Workbook, sheet: usize, m: &Model, keep: bool) -> Result<Outcome, String> {
    let p = build(wb, sheet, m)?;
    let original: Vec<f64> = p.vars.iter().map(|q| wb.sheets[sheet].value(*q).to_num().unwrap_or(0.0)).collect();
    let rel = rels(&p);
    let int_any = p.integer.iter().any(|b| *b);
    let mut relax = |wb: &mut Workbook, lb: &[f64], ub: &[f64], x0: &[f64]| -> (Status, Vec<f64>, f64) {
        match m.engine {
            Engine::Simplex => simplex_relax(wb, &p, &rel, lb, ub, x0),
            Engine::Grg => nlp(wb, &p, &rel, lb, ub, x0, m.precision, m.max_iter),
            Engine::Evolutionary => {
                let mut best: Option<(Status, Vec<f64>, f64)> = None;
                let mut seed = 0x9e3779b9u32;
                for k in 0..6 {
                    let start: Vec<f64> = if k == 0 { x0.to_vec() } else { (0..x0.len()).map(|j| { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; let r = seed as f64 / u32::MAX as f64; let lo = if lb[j].is_finite() { lb[j] } else { x0[j] - 10.0 * x0[j].abs().max(1.0) }; let hi = if ub[j].is_finite() { ub[j] } else { x0[j] + 10.0 * x0[j].abs().max(1.0) }; lo + r * (hi - lo) }).collect() };
                    let r = nlp(wb, &p, &rel, lb, ub, &start, m.precision, m.max_iter);
                    if r.0.found() && best.as_ref().is_none_or(|b| !b.0.found() || r.2 < b.2) { best = Some(r); } else if best.is_none() { best = Some(r); }
                }
                let b = best.unwrap();
                (if b.0.found() { Status::Converged } else { b.0 }, b.1, b.2)
            }
        }
    };
    let (status, x, f) = if int_any { branch_and_bound(wb, &p, &mut relax, &original) } else { relax(wb, &p.lb.clone(), &p.ub.clone(), &original) };
    let x = if x.len() == p.vars.len() { x } else { original.clone() };
    let final_x = if keep && status.found() { x.clone() } else { original.clone() };
    let at = evaluate(wb, &p, &x);
    let objective = wb.sheets[sheet].value(p.obj).to_num().unwrap_or(f64::NAN);
    let _ = (f, at);
    for (q, v) in p.vars.iter().zip(&final_x) { let c = wb.sheets[sheet].cell_mut(*q); c.input = Input::Value(Value::Num(*v)); c.value = Value::Num(*v); }
    wb.recalc();
    Ok(Outcome { status, x, objective, original })
}

fn branch_and_bound(wb: &mut Workbook, p: &Problem, relax: &mut dyn FnMut(&mut Workbook, &[f64], &[f64], &[f64]) -> (Status, Vec<f64>, f64), x0: &[f64]) -> (Status, Vec<f64>, f64) {
    let n = p.vars.len();
    let mut lb0 = p.lb.clone(); let mut ub0 = p.ub.clone();
    for j in 0..n { if p.integer[j] { lb0[j] = (lb0[j] - 1e-6).ceil(); ub0[j] = (ub0[j] + 1e-6).floor(); } }
    let mut stack = vec![(lb0, ub0)];
    let mut best: Option<(Vec<f64>, f64)> = None;
    let mut nodes = 0;
    let mut unbounded = false;
    while let Some((lb, ub)) = stack.pop() {
        if nodes > 5000 { break; }
        if (0..n).any(|j| lb[j] > ub[j]) { continue; }
        nodes += 1;
        let start: Vec<f64> = (0..n).map(|j| x0[j].clamp(lb[j], ub[j].max(lb[j]))).collect();
        let (st, x, f) = relax(wb, &lb, &ub, &start);
        if st == Status::Unbounded { unbounded = true; continue; }
        if !st.found() { continue; }
        if best.as_ref().is_some_and(|b| f >= b.1 - 1e-9) { continue; }
        let mut j = None; let mut frac = 1e-6;
        for k in 0..n { if p.integer[k] { let fr = (x[k] - x[k].round()).abs(); if fr > frac { frac = fr; j = Some(k); } } }
        if let Some(j) = j {
            let v = x[j];
            let (mut dl, mut du, mut ul, mut uu) = (lb.clone(), ub.clone(), lb.clone(), ub.clone());
            du[j] = v.floor(); ul[j] = v.ceil();
            let _ = (&mut dl, &mut uu);
            if v - v.floor() < 0.5 { stack.push((ul, uu)); stack.push((dl, du)); } else { stack.push((dl, du)); stack.push((ul, uu)); }
            continue;
        }
        let xr: Vec<f64> = (0..n).map(|k| if p.integer[k] { x[k].round() } else { x[k] }).collect();
        // AllDifferent: split on the first clash.
        let clash = p.groups.iter().find_map(|g| { let mut seen = std::collections::HashMap::new(); g.iter().find_map(|&k| seen.insert(xr[k] as i64, k).map(|o| (k, xr[k], o))) });
        if let Some((k, v, other)) = clash {
            let child = || (lb.clone(), ub.clone());
            let (mut below, mut above, mut ob, mut oa) = (child(), child(), child(), child());
            below.1[k] = v - 1.0; above.0[k] = v + 1.0;
            for c in [&mut ob, &mut oa] { c.0[k] = v; c.1[k] = v; }
            ob.1[other] = v - 1.0; oa.0[other] = v + 1.0;
            stack.extend([oa, ob, above, below]);
            continue;
        }
        best = Some((xr, f));
    }
    match best { Some((x, f)) => (Status::Optimal, x, f), None if unbounded => (Status::Unbounded, vec![], f64::NAN), None => (Status::Infeasible, vec![], f64::NAN) }
}

// ---- Simplex LP --------------------------------------------------------------------------------

fn simplex_relax(wb: &mut Workbook, p: &Problem, rel: &[Rel], lb: &[f64], ub: &[f64], x0: &[f64]) -> (Status, Vec<f64>, f64) {
    let n = p.vars.len();
    // Coefficients by unit differences from a base point.
    let base: Vec<f64> = (0..n).map(|j| x0[j].clamp(lb[j], ub[j].max(lb[j]))).collect();
    let Some((f0, g0)) = evaluate(wb, p, &base) else { return (Status::Error, vec![], f64::NAN) };
    let mut c = vec![0.0; n];
    let mut a = vec![vec![0.0; n]; g0.len()];
    for j in 0..n {
        let mut x = base.clone(); x[j] += 1.0;
        let Some((f, g)) = evaluate(wb, p, &x) else { return (Status::Error, vec![], f64::NAN) };
        c[j] = f - f0;
        for i in 0..g.len() { a[i][j] = g[i] - g0[i]; }
    }
    // Linearity check at a second point.
    let probe: Vec<f64> = base.iter().enumerate().map(|(j, v)| v + 1.5 + j as f64 * 0.37).collect();
    if let Some((f, g)) = evaluate(wb, p, &probe) {
        let pred = |row: &[f64], k0: f64| k0 + row.iter().zip(&probe).zip(&base).map(|((r, x), b)| r * (x - b)).sum::<f64>();
        let bad = |act: f64, pr: f64| (act - pr).abs() > 1e-6 * (1.0 + act.abs());
        if bad(f, pred(&c, f0)) || g.iter().enumerate().any(|(i, v)| bad(*v, pred(&a[i], g0[i]))) { return (Status::NotLinear, vec![], f64::NAN); }
    }
    // g_i(x) = g0_i + a_i·(x - base) rel 0  →  a_i·x rel a_i·base - g0_i
    let b: Vec<f64> = (0..g0.len()).map(|i| a[i].iter().zip(&base).map(|(x, y)| x * y).sum::<f64>() - g0[i]).collect();
    let r: Vec<Rel> = rel.to_vec();
    let res = solve_lp(&c, &a, &r, &b, lb, ub, 50000);
    match res {
        Ok((x, _)) => { let f = f0 + c.iter().zip(&x).zip(&base).map(|((c, x), b)| c * (x - b)).sum::<f64>(); (Status::Optimal, x, f) }
        Err(s) => (s, vec![], f64::NAN),
    }
}

const FREE: u8 = 0; const AT_LOWER: u8 = 1; const AT_UPPER: u8 = 2; const BASIC: u8 = 3;

/// minimize c·x subject to A_i·x (rel_i) b_i, lb ≤ x ≤ ub: the bounded two-phase primal simplex
/// of solver-lp.js (Dantzig pricing, Bland's rule after a run of degenerate pivots).
pub fn solve_lp(c: &[f64], a: &[Vec<f64>], rel: &[Rel], b: &[f64], lb: &[f64], ub: &[f64], max_iter: usize) -> Result<(Vec<f64>, f64), Status> {
    let tol = 1e-9;
    let (n, m) = (c.len(), b.len());
    let nn = n + 2 * m;
    let (mut lo, mut hi, mut cost) = (vec![0.0; nn], vec![0.0; nn], vec![0.0; nn]);
    for j in 0..n { lo[j] = lb[j]; hi[j] = ub[j]; if lo[j] > hi[j] + tol { return Err(Status::Infeasible); } if lo[j] > hi[j] { hi[j] = lo[j]; } }
    for i in 0..m {
        lo[n + i] = if rel[i] == Rel::Ge { f64::NEG_INFINITY } else { 0.0 };
        hi[n + i] = if rel[i] == Rel::Le { f64::INFINITY } else { 0.0 };
        lo[n + m + i] = 0.0; hi[n + m + i] = f64::INFINITY;
    }
    let mut val = vec![0.0; nn];
    let mut state = vec![FREE; nn];
    for j in 0..n + m {
        if lo[j].is_finite() { val[j] = lo[j]; state[j] = AT_LOWER; } else if hi[j].is_finite() { val[j] = hi[j]; state[j] = AT_UPPER; } else { val[j] = 0.0; state[j] = FREE; }
    }
    let mut t = vec![vec![0.0; nn]; m];
    let mut basis = vec![0usize; m];
    let mut xb = vec![0.0; m];
    let scale = b.iter().fold(1.0f64, |s, v| s.max(v.abs()));
    for i in 0..m {
        let mut r = b[i];
        for j in 0..n { r -= a[i][j] * val[j]; }
        let sg = if r >= 0.0 { 1.0 } else { -1.0 };
        for j in 0..n { t[i][j] = sg * a[i][j]; }
        t[i][n + i] = sg; t[i][n + m + i] = 1.0;
        basis[i] = n + m + i; state[n + m + i] = BASIC; xb[i] = r.abs();
    }
    let mut iterations = 0usize;
    let mut d = vec![0.0; nn];
    let price = |d: &mut Vec<f64>, t: &Vec<Vec<f64>>, basis: &Vec<usize>, cost: &Vec<f64>| {
        d.copy_from_slice(cost);
        for i in 0..m { let cb = cost[basis[i]]; if cb == 0.0 { continue; } for j in 0..nn { if t[i][j] != 0.0 { d[j] -= cb * t[i][j]; } } }
    };
    let pivot = |t: &mut Vec<Vec<f64>>, d: &mut Vec<f64>, r: usize, q: usize| {
        let pv = t[r][q];
        for j in 0..nn { t[r][j] /= pv; }
        t[r][q] = 1.0;
        let pr = t[r].clone();
        for (i, row) in t.iter_mut().enumerate() {
            if i == r { continue; }
            let f = row[q]; if f == 0.0 { continue; }
            for j in 0..nn { if pr[j] != 0.0 { row[j] -= f * pr[j]; } }
            row[q] = 0.0;
        }
        let f = d[q];
        if f != 0.0 { for j in 0..nn { if pr[j] != 0.0 { d[j] -= f * pr[j]; } } d[q] = 0.0; }
    };
    #[derive(PartialEq)] enum It { Optimal, Unbounded, Limit }
    let mut iterate = |t: &mut Vec<Vec<f64>>, d: &mut Vec<f64>, basis: &mut Vec<usize>, xb: &mut Vec<f64>, state: &mut Vec<u8>, val: &mut Vec<f64>, cost: &Vec<f64>, lo: &Vec<f64>, hi: &Vec<f64>| -> It {
        price(d, t, basis, cost);
        let mut degenerate = 0;
        loop {
            if iterations >= max_iter { return It::Limit; }
            let bland = degenerate > 50;
            let (mut q, mut best) = (usize::MAX, 0.0);
            for j in 0..nn {
                let s = state[j];
                if s == BASIC || lo[j] == hi[j] { continue; }
                let dj = d[j];
                let gain = if s == AT_LOWER { -dj } else if s == AT_UPPER { dj } else { dj.abs() };
                if gain > tol * (1.0 + cost[j].abs()) { if bland { q = j; break; } if gain > best { best = gain; q = j; } }
            }
            if q == usize::MAX { return It::Optimal; }
            let dir = if state[q] == AT_LOWER { 1.0 } else if state[q] == AT_UPPER { -1.0 } else if d[q] < 0.0 { 1.0 } else { -1.0 };
            let mut theta = hi[q] - lo[q];
            let (mut leave, mut leave_to, mut leave_alpha) = (usize::MAX, 0u8, 0.0f64);
            for i in 0..m {
                let alpha = t[i][q] * dir;
                if alpha.abs() <= 1e-11 { continue; }
                let k = basis[i];
                let mut lim = if alpha > 0.0 { if !lo[k].is_finite() { continue; } (xb[i] - lo[k]) / alpha } else { if !hi[k].is_finite() { continue; } (hi[k] - xb[i]) / -alpha };
                if lim < 0.0 { lim = 0.0; }
                let nearer = lim < theta - 1e-12;
                let tie = !nearer && leave != usize::MAX && lim <= theta + 1e-12 && (if bland { k < basis[leave] } else { alpha.abs() > leave_alpha.abs() });
                if nearer || tie { theta = lim; leave = i; leave_to = if alpha > 0.0 { AT_LOWER } else { AT_UPPER }; leave_alpha = alpha; }
            }
            if !theta.is_finite() { return It::Unbounded; }
            iterations += 1;
            degenerate = if theta <= 1e-12 { degenerate + 1 } else { 0 };
            if theta != 0.0 { for i in 0..m { if t[i][q] != 0.0 { xb[i] -= t[i][q] * dir * theta; } } }
            let entering = val[q] + dir * theta;
            if leave == usize::MAX {
                state[q] = if state[q] == AT_LOWER { AT_UPPER } else { AT_LOWER };
                val[q] = if state[q] == AT_LOWER { lo[q] } else { hi[q] };
                continue;
            }
            let k = basis[leave];
            state[k] = leave_to; val[k] = if leave_to == AT_LOWER { lo[k] } else { hi[k] };
            pivot(t, d, leave, q);
            basis[leave] = q; state[q] = BASIC; xb[leave] = entering;
        }
    };
    for i in 0..m { cost[n + m + i] = 1.0; }
    let st = if m > 0 { iterate(&mut t, &mut d, &mut basis, &mut xb, &mut state, &mut val, &cost, &lo, &hi) } else { It::Optimal };
    if st == It::Limit { return Err(Status::IterationLimit); }
    let infeas: f64 = (0..m).filter(|&i| basis[i] >= n + m).map(|i| xb[i].max(0.0)).sum();
    if infeas > 1e-7 * scale { return Err(Status::Infeasible); }
    for i in 0..m { cost[n + m + i] = 0.0; hi[n + m + i] = 0.0; }
    for j in n + m..nn { if state[j] != BASIC { state[j] = AT_LOWER; val[j] = 0.0; } }
    price(&mut d, &t, &basis, &cost);
    for i in 0..m {
        if basis[i] < n + m { continue; }
        let (mut q, mut big) = (usize::MAX, 1e-9);
        for j in 0..n + m { if state[j] != BASIC && t[i][j].abs() > big { big = t[i][j].abs(); q = j; } }
        if q == usize::MAX { continue; }
        let k = basis[i];
        state[k] = AT_LOWER; val[k] = 0.0;
        let ev = val[q];
        pivot(&mut t, &mut d, i, q);
        basis[i] = q; state[q] = BASIC; xb[i] = ev;
    }
    cost[..n].copy_from_slice(c);
    let st = iterate(&mut t, &mut d, &mut basis, &mut xb, &mut state, &mut val, &cost, &lo, &hi);
    let mut x: Vec<f64> = val[..n].to_vec();
    for i in 0..m { if basis[i] < n { x[basis[i]] = xb[i]; } }
    let obj = c.iter().zip(&x).map(|(a, b)| a * b).sum();
    match st { It::Optimal => Ok((x, obj)), It::Unbounded => Err(Status::Unbounded), It::Limit => Err(Status::IterationLimit) }
}

// ---- GRG Nonlinear -----------------------------------------------------------------------------

fn violation(rel: &[Rel], g: &[f64]) -> f64 {
    rel.iter().zip(g).map(|(r, v)| match r { Rel::Le => v.max(0.0), Rel::Ge => (-v).max(0.0), _ => v.abs() }).fold(0.0, f64::max)
}

#[allow(clippy::too_many_arguments)]
fn nlp(wb: &mut Workbook, p: &Problem, rel: &[Rel], lb: &[f64], ub: &[f64], x0: &[f64], precision: f64, max_iter: u32) -> (Status, Vec<f64>, f64) {
    let n = x0.len();
    let m = rel.len();
    let clamp = |x: &[f64]| -> Vec<f64> { x.iter().enumerate().map(|(j, v)| v.max(lb[j]).min(ub[j])).collect() };
    let mut x = clamp(x0);
    let mut eval = |x: &[f64]| evaluate(wb, p, x);
    let Some(mut at) = eval(&x) else { return (Status::Error, x, f64::NAN) };
    let sign: Vec<f64> = rel.iter().map(|r| if *r == Rel::Ge { -1.0 } else { 1.0 }).collect();
    let is_eq: Vec<bool> = rel.iter().map(|r| *r == Rel::Eq).collect();
    let mut lam = vec![0.0; m];
    let mut rho = 100.0;
    let merit = |r: &(f64, Vec<f64>), lam: &[f64], rho: f64| -> f64 {
        let mut v = r.0;
        for i in 0..m { let c = sign[i] * r.1[i]; if is_eq[i] { v += lam[i] * c + rho / 2.0 * c * c; } else { let t = (lam[i] + rho * c).max(0.0); v += (t * t - lam[i] * lam[i]) / (2.0 * rho); } }
        v
    };
    let mut iters = 0u32;
    let grad_of = |eval: &mut dyn FnMut(&[f64]) -> Option<(f64, Vec<f64>)>, x: &[f64], base: &(f64, Vec<f64>), lam: &[f64], rho: f64| -> Vec<f64> {
        let phi0 = merit(base, lam, rho);
        (0..n).map(|j| {
            let h = 1e-7 * x[j].abs().max(1.0);
            let mut y = x.to_vec();
            let back = y[j] + h > ub[j];
            y[j] += if back { -h } else { h };
            match eval(&y) { Some(r) => { let d = (merit(&r, lam, rho) - phi0) / h; if back { -d } else { d } } None => 0.0 }
        }).collect()
    };
    let mut omega = if m > 0 { 1e-2 } else { 1e-8 };
    let mut last_v = f64::INFINITY;
    for _outer in 0..40 {
        // Inner: projected BFGS on the merit function.
        let mut hinv: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect();
        let mut phi = merit(&at, &lam, rho);
        let mut grad = grad_of(&mut eval, &x, &at, &lam, rho);
        let mut flat = 0;
        for _k in 0..200 {
            if iters >= max_iter { break; }
            iters += 1;
            let pg = (0..n).map(|j| ((x[j] - grad[j]).max(lb[j]).min(ub[j]) - x[j]).abs()).fold(0.0, f64::max);
            if pg <= omega { break; }
            let free: Vec<bool> = (0..n).map(|j| lb[j] < ub[j] && !((x[j] <= lb[j] + 1e-12 && grad[j] > 0.0) || (x[j] >= ub[j] - 1e-12 && grad[j] < 0.0))).collect();
            let mut d: Vec<f64> = (0..n).map(|i| if !free[i] { 0.0 } else { -(0..n).filter(|&j| free[j]).map(|j| hinv[i][j] * grad[j]).sum::<f64>() }).collect();
            let mut slope: f64 = (0..n).map(|j| grad[j] * d[j]).sum();
            if slope >= 0.0 {
                for i in 0..n { for j in 0..n { hinv[i][j] = if i == j { 1.0 } else { 0.0 }; } }
                d = (0..n).map(|j| if free[j] { -grad[j] } else { 0.0 }).collect();
                slope = (0..n).map(|j| grad[j] * d[j]).sum();
                if slope >= 0.0 { break; }
            }
            let dmax = d.iter().fold(0.0f64, |a, v| a.max(v.abs()));
            let xmax = x.iter().fold(1.0f64, |a, v| a.max(v.abs()));
            let mut alpha: f64 = if _k == 0 { (0.1 * xmax / dmax).min(1.0) } else { 1.0 };
            let mut step = None;
            for _t in 0..40 {
                let xt = clamp(&x.iter().zip(&d).map(|(a, b)| a + alpha * b).collect::<Vec<_>>());
                if let Some(rt) = eval(&xt) {
                    let pt = merit(&rt, &lam, rho);
                    let dec: f64 = (0..n).map(|j| grad[j] * (xt[j] - x[j])).sum();
                    if pt <= phi + 1e-4 * dec && xt != x { step = Some((xt, rt, pt)); break; }
                }
                alpha *= 0.3;
            }
            let Some((xt, rt, pt)) = step else { break };
            let gn = grad_of(&mut eval, &xt, &rt, &lam, rho);
            let s: Vec<f64> = (0..n).map(|j| xt[j] - x[j]).collect();
            let y: Vec<f64> = (0..n).map(|j| gn[j] - grad[j]).collect();
            let sy: f64 = s.iter().zip(&y).map(|(a, b)| a * b).sum();
            if sy > 1e-12 {
                // BFGS update of the inverse Hessian.
                let hy: Vec<f64> = (0..n).map(|i| (0..n).map(|j| hinv[i][j] * y[j]).sum()).collect();
                let yhy: f64 = y.iter().zip(&hy).map(|(a, b)| a * b).sum();
                for i in 0..n { for j in 0..n { hinv[i][j] += (sy + yhy) * s[i] * s[j] / (sy * sy) - (hy[i] * s[j] + s[i] * hy[j]) / sy; } }
            }
            let change = (pt - phi).abs();
            flat = if change <= 1e-10 * phi.abs().max(1.0) { flat + 1 } else { 0 };
            x = xt; at = rt; phi = pt; grad = gn;
            if x.iter().any(|v| v.abs() > 1e20) || at.0 < -1e30 { return (Status::Unbounded, x, at.0); }
            if flat >= 4 { break; }
        }
        let v = violation(rel, &at.1);
        if v <= precision && omega <= 1e-7 { break; }
        if m == 0 { break; }
        if v <= precision.max(0.1 * last_v.min(1.0)) {
            for i in 0..m { let c = sign[i] * at.1[i]; lam[i] = if is_eq[i] { lam[i] + rho * c } else { (lam[i] + rho * c).max(0.0) }; }
            omega = (omega * 0.1).max(1e-7);
        } else {
            rho = (rho * 10.0).min(1e12);
            omega = (omega * 0.5).max(1e-7);
        }
        last_v = v;
    }
    let v = violation(rel, &at.1);
    let status = if v > precision.max(1e-6) * 10.0 { Status::Infeasible } else if iters >= max_iter { Status::IterationLimit } else { Status::Optimal };
    (status, x, at.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::parse_a1;

    #[test]
    fn simplex_core() {
        // max 3x + 5y st x ≤ 4, 2y ≤ 12, 3x + 2y ≤ 18 → (2, 6), 36
        let (x, f) = solve_lp(&[-3.0, -5.0], &[vec![1.0, 0.0], vec![0.0, 2.0], vec![3.0, 2.0]], &[Rel::Le; 3], &[4.0, 12.0, 18.0], &[0.0; 2], &[f64::INFINITY; 2], 1000).unwrap();
        assert!((x[0] - 2.0).abs() < 1e-9 && (x[1] - 6.0).abs() < 1e-9 && (f + 36.0).abs() < 1e-9);
        assert_eq!(solve_lp(&[1.0], &[vec![1.0], vec![1.0]], &[Rel::Ge, Rel::Le], &[5.0, 3.0], &[0.0], &[f64::INFINITY], 100), Err(Status::Infeasible));
        assert_eq!(solve_lp(&[-1.0], &[], &[], &[], &[0.0], &[f64::INFINITY], 100), Err(Status::Unbounded));
    }

    fn sheet_lp() -> Workbook {
        let mut wb = Workbook::new();
        // Variables A1:B1, objective C1, constraints D1:D2.
        wb.set_input(0, parse_a1("A1").unwrap(), "0");
        wb.set_input(0, parse_a1("B1").unwrap(), "0");
        wb.set_input(0, parse_a1("C1").unwrap(), "=3*A1+5*B1");
        wb.set_input(0, parse_a1("D1").unwrap(), "=2*B1");
        wb.set_input(0, parse_a1("D2").unwrap(), "=3*A1+2*B1");
        wb.set_input(0, parse_a1("E2").unwrap(), "18");
        wb.recalc();
        wb
    }

    fn model(engine: Engine) -> Model {
        Model { objective: "C1".into(), goal: Goal::Max, vars: "A1:B1".into(), constraints: vec![
            Constraint { lhs: "A1".into(), rel: Rel::Le, rhs: "4".into() },
            Constraint { lhs: "D1".into(), rel: Rel::Le, rhs: "12".into() },
            Constraint { lhs: "D2".into(), rel: Rel::Le, rhs: "E2".into() },
        ], engine, ..Default::default() }
    }

    #[test]
    fn solver_simplex_on_a_sheet_and_names_round_trip() {
        let mut wb = sheet_lp();
        let m = model(Engine::Simplex);
        save(&mut wb, 0, &m);
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        assert_eq!(load(&back, 0), m);
        let r = solve(&mut wb, 0, &m, true).unwrap();
        assert_eq!(r.status, Status::Optimal);
        assert!((r.objective - 36.0).abs() < 1e-6, "{r:?}");
        assert_eq!(wb.display(0, parse_a1("B1").unwrap()), "6");
        // Not linear.
        wb.set_input(0, parse_a1("C1").unwrap(), "=A1*B1");
        wb.recalc();
        assert_eq!(solve(&mut wb, 0, &m, false).unwrap().status, Status::NotLinear);
    }

    #[test]
    fn solver_grg_and_integers() {
        let mut wb = sheet_lp();
        let r = solve(&mut wb, 0, &model(Engine::Grg), true).unwrap();
        assert!(r.status.found(), "{r:?}");
        assert!((r.objective - 36.0).abs() < 1e-3, "{r:?}");
        // Minimize (x-3)^2 + (y+1)^2 with y ≥ 0 → (3, 0), 1.
        let mut wb = Workbook::new();
        wb.set_input(0, parse_a1("A1").unwrap(), "1");
        wb.set_input(0, parse_a1("B1").unwrap(), "1");
        wb.set_input(0, parse_a1("C1").unwrap(), "=(A1-3)^2+(B1+1)^2");
        wb.recalc();
        let m = Model { objective: "C1".into(), goal: Goal::Min, vars: "A1:B1".into(), ..Default::default() };
        let r = solve(&mut wb, 0, &m, true).unwrap();
        assert!((r.objective - 1.0).abs() < 1e-4 && (r.x[0] - 3.0).abs() < 1e-2, "{r:?}");
        // Integer: max x + y with 2x + 2y ≤ 7 → 3.
        let mut wb = Workbook::new();
        wb.set_input(0, parse_a1("A1").unwrap(), "0");
        wb.set_input(0, parse_a1("B1").unwrap(), "0");
        wb.set_input(0, parse_a1("C1").unwrap(), "=A1+B1");
        wb.set_input(0, parse_a1("D1").unwrap(), "=2*A1+2*B1");
        wb.recalc();
        let m = Model { objective: "C1".into(), vars: "A1:B1".into(), engine: Engine::Simplex, constraints: vec![Constraint { lhs: "D1".into(), rel: Rel::Le, rhs: "7".into() }, Constraint { lhs: "A1:B1".into(), rel: Rel::Int, rhs: "integer".into() }], ..Default::default() };
        let r = solve(&mut wb, 0, &m, true).unwrap();
        assert_eq!(r.status, Status::Optimal);
        assert!((r.objective - 3.0).abs() < 1e-9, "{r:?}");
        assert!(r.x.iter().all(|v| v.fract() == 0.0));
    }
}
