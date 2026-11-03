//! The operators as functions (ADD, EQ, UMINUS…), Sheets extras (ISBETWEEN, EPOCHTODATE,
//! COUNTUNIQUE, FLATTEN, ARRAY_CONSTRAIN), CELL, INFO, FILTERXML, and the functions that need a
//! service this engine has none of (CUBE*, RTD, WEBSERVICE, STOCKHISTORY, TABLE, GETPIVOTDATA).

use super::*;
use crate::formula::eval::{binary, compare};
use crate::formula::parser::BinOp;

pub const DEFS: &[Def] = &[
    s("ADD", 2, 2, |a| op(BinOp::Add, a)), s("MINUS", 2, 2, |a| op(BinOp::Sub, a)), s("MULTIPLY", 2, 2, |a| op(BinOp::Mul, a)),
    s("DIVIDE", 2, 2, |a| op(BinOp::Div, a)), s("POW", 2, 2, |a| op(BinOp::Pow, a)),
    s("EQ", 2, 2, |a| op(BinOp::Eq, a)), s("NE", 2, 2, |a| op(BinOp::Ne, a)), s("GT", 2, 2, |a| op(BinOp::Gt, a)),
    s("GTE", 2, 2, |a| op(BinOp::Ge, a)), s("LT", 2, 2, |a| op(BinOp::Lt, a)), s("LTE", 2, 2, |a| op(BinOp::Le, a)),
    s("UMINUS", 1, 1, |a| num(-n!(a[0]))), s("UPLUS", 1, 1, |a| num(n!(a[0]))), s("UNARY_PERCENT", 1, 1, |a| num(n!(a[0]) / 100.0)),
    s("ISBETWEEN", 3, 5, isbetween),
    s("EPOCHTODATE", 1, 2, |a| { let t = n!(a[0]); let div = match tryv!(opt(a, 1, 1.0)).trunc() as i64 { 1 => 1.0, 2 => 1e3, 3 => 1e6, _ => return err(ErrKind::Value) }; num(25569.0 + t / div / 86400.0) }),
    e("COUNTUNIQUE", 1, MANY, |c, a| { let mut seen = std::collections::HashSet::new(); for x in a { each(c, x, &mut |v, _| { if let Some(k) = value_key(v) { seen.insert(k); } }); } num(seen.len() as f64) }),
    e("COUNTUNIQUEIFS", 3, MANY, countuniqueifs),
    e("FLATTEN", 1, MANY, |c, a| { let mut out = vec![]; for x in a { match x { Arg::V(v) if !matches!(v, Value::Array(_)) => out.push(v.clone()), _ => out.extend(flat(c, x)) } } if out.is_empty() { err(ErrKind::NA) } else { Value::array(Matrix::new(out.len(), 1, out)) } }),
    e("ARRAYFORMULA", 1, 1, |c, a| c.value_of(&a[0])),
    e("ARRAY_CONSTRAIN", 3, 3, |c, a| { let m = c.matrix_of(&a[0]); let r = tryv!(arg_num(c, a, 1, 0.0)).trunc(); let k = tryv!(arg_num(c, a, 2, 0.0)).trunc(); if r < 1.0 || k < 1.0 { return err(ErrKind::Value); } let (r, k) = ((r as usize).min(m.rows), (k as usize).min(m.cols)); Value::array(Matrix::new(r, k, (0..r).flat_map(|i| m.row(i)[..k].to_vec()).collect())) }),
    e("CELL", 1, 2, cell), e("INFO", 1, 1, info),
    s("FILTERXML", 2, 2, |a| filterxml(&t!(a[0]), &t!(a[1]))),
    s("IMAGE", 1, 5, |a| { let u = t!(a[0]); let u = u.trim(); let l = u.to_ascii_lowercase(); if !(l.starts_with("http://") || l.starts_with("https://") || l.starts_with("data:image/")) || u.contains(char::is_whitespace) { return err(ErrKind::Value); } let alt = match a.get(1) { Some(Value::Str(s)) => s.to_string(), _ => String::new() }; crate::cellimage::value(u, &alt) }),
    e("SPARKLINE", 1, 3, |c, a| { let mut any = false; each(c, &a[0], &mut |v, _| any |= matches!(v, Value::Num(_))); if any { Value::str("") } else { err(ErrKind::NA) } }),
    s("CUBEVALUE", 0, MANY, |_| err(ErrKind::NA)), s("CUBEMEMBER", 0, MANY, |_| err(ErrKind::NA)), s("CUBEMEMBERPROPERTY", 0, MANY, |_| err(ErrKind::NA)),
    s("CUBERANKEDMEMBER", 0, MANY, |_| err(ErrKind::NA)), s("CUBESET", 0, MANY, |_| err(ErrKind::NA)), s("CUBESETCOUNT", 0, MANY, |_| err(ErrKind::NA)),
    s("CUBEKPIMEMBER", 0, MANY, |_| err(ErrKind::NA)), s("RTD", 0, MANY, |_| err(ErrKind::NA)), s("STOCKHISTORY", 2, MANY, |_| err(ErrKind::NA)),
    s("WEBSERVICE", 1, 1, |_| err(ErrKind::Value)), e("TABLE", 2, 2, |_, _| err(ErrKind::Ref)), e("GETPIVOTDATA", 2, MANY, |_, _| err(ErrKind::Ref)),
];

fn op(o: BinOp, a: &[Value]) -> Value {
    if matches!(o, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow) { let x = n!(a[0]); let y = n!(a[1]); return binary(o, &Value::Num(x), &Value::Num(y)); }
    binary(o, &a[0], &a[1])
}

fn isbetween(a: &[Value]) -> Value {
    let li = if a.len() > 3 && !a[3].is_empty() { b!(a[3]) } else { true };
    let hi = if a.len() > 4 && !a[4].is_empty() { b!(a[4]) } else { true };
    for v in &a[..3] { if let Value::Err(e) = v { return err(*e); } if v.is_empty() { return err(ErrKind::Value); } }
    let (c1, c2) = (compare(&a[0], &a[1]), compare(&a[0], &a[2]));
    boolv((if li { c1.is_ge() } else { c1.is_gt() }) && (if hi { c2.is_le() } else { c2.is_lt() }))
}

/// Case-insensitive identity of a value (None for blanks), as UNIQUE compares.
fn value_key(v: &Value) -> Option<String> {
    Some(match v {
        Value::Empty => return None, Value::Str(s) if s.is_empty() => return None,
        Value::Str(s) => format!("s:{}", s.to_lowercase()), Value::Num(n) => format!("n:{n}"), Value::Bool(b) => format!("b:{b}"),
        Value::Err(e) => format!("e:{}", e.text()), Value::Array(_) => return None,
    })
}

fn countuniqueifs(ctx: &mut Ctx, a: &[Arg]) -> Value {
    if a.len() % 2 == 0 { return err(ErrKind::Value); }
    let m = ctx.matrix_of(&a[0]);
    let pairs: Vec<(&Arg, super::criteria::Criteria)> = a[1..].chunks(2).map(|p| (&p[0], super::math::crit_arg(ctx, &p[1]))).collect();
    let hits = tryv!(super::math::criteria_rows(ctx, &pairs));
    if hits.len() != m.data.len() { return err(ErrKind::Value); }
    let seen: std::collections::HashSet<String> = m.data.iter().zip(&hits).filter(|(_, h)| **h).filter_map(|(v, _)| value_key(v)).collect();
    num(seen.len() as f64)
}

fn cell(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let kind = tryv!(arg_val(ctx, a, 0).to_text()).trim().to_lowercase();
    let target = match a.get(1) {
        None | Some(Arg::Missing) => None,
        Some(Arg::R(s, r)) => Some((*s, r.start())),
        Some(Arg::Areas(l)) if !l.is_empty() => Some((l[0].0, l[0].1.start())),
        Some(Arg::V(Value::Err(e))) => return err(*e),
        _ => return err(ErrKind::Value),
    };
    let (s, p) = target.unwrap_or((ctx.sheet, ctx.pos));
    let v = || target.map(|_| ctx.cell(s, p));
    match kind.as_str() {
        "address" => {
            let own = s == ctx.sheet;
            let prefix = if own { String::new() } else { format!("'{}'!", ctx.book.sheets.get(s).map(|x| x.name.replace('\'', "''")).unwrap_or_default()) };
            Value::str(format!("{prefix}${}${}", crate::addr::col_name(p.col), p.row + 1))
        }
        "row" => num(p.row as f64 + 1.0), "col" => num(p.col as f64 + 1.0), "filename" => Value::str(""),
        "contents" => match v() { None => err(ErrKind::Value), Some(Value::Empty) => num(0.0), Some(x) => x },
        "type" => match v() { None => err(ErrKind::Value), Some(Value::Empty) => Value::str("b"), Some(Value::Str(_)) => Value::str("l"), _ => Value::str("v") },
        "prefix" => match v() { None => err(ErrKind::Value), Some(Value::Str(_)) => Value::str("'"), _ => Value::str("") },
        "format" => Value::str("G"), "color" | "parentheses" => num(0.0), "protect" => num(1.0),
        "width" => { let sh = ctx.book.sheets.get(s); let w = sh.map(|x| x.col_widths.get(&p.col).copied().unwrap_or(x.default_col_width)).unwrap_or(64.0); num((w as f64 / 7.0).round()) }
        _ => err(ErrKind::Value),
    }
}

fn info(ctx: &mut Ctx, a: &[Arg]) -> Value {
    match tryv!(arg_val(ctx, a, 0).to_text()).trim().to_lowercase().as_str() {
        "directory" => Value::str("/"), "numfile" => num(ctx.book.sheets.len() as f64), "origin" => Value::str("$A:$A$1"),
        "osversion" => Value::str(std::env::consts::OS), "recalc" => Value::str(if ctx.book.manual { "Manual" } else { "Automatic" }),
        "release" => Value::str("1.0"), "system" => Value::str(if cfg!(target_os = "macos") { "mac" } else { "pcdos" }),
        _ => err(ErrKind::Value),
    }
}

// ---- FILTERXML: a small XML reader and the common XPath subset ----

/// An element of the parsed document.
struct Node { name: String, attrs: Vec<(String, String)>, kids: Vec<usize>, text: String }

/// Parses XML into a flat node list (index 0 is a virtual root); None when malformed.
fn parse_xml(src: &str) -> Option<Vec<Node>> {
    let mut nodes = vec![Node { name: String::new(), attrs: vec![], kids: vec![], text: String::new() }];
    let mut stack = vec![0usize];
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'<' {
            let rest = &src[i..];
            if rest.starts_with("<?") { i += rest.find("?>")? + 2; continue; }
            if rest.starts_with("<!--") { i += rest.find("-->")? + 3; continue; }
            if rest.starts_with("<![CDATA[") { let e = rest.find("]]>")?; let t = &rest[9..e]; for &n in &stack { nodes[n].text.push_str(t); } i += e + 3; continue; }
            if rest.starts_with("<!") { i += rest.find('>')? + 1; continue; }
            let e = rest.find('>')?;
            let tag = &rest[1..e];
            if let Some(name) = tag.strip_prefix('/') { let top = stack.pop()?; if nodes[top].name != name.trim() || stack.is_empty() { return None; } }
            else {
                let selfc = tag.ends_with('/');
                let tag = tag.trim_end_matches('/');
                let name: String = tag.chars().take_while(|c| !c.is_whitespace()).collect();
                if name.is_empty() { return None; }
                let mut attrs = vec![]; let mut r = &tag[name.len()..];
                while let Some(eq) = r.find('=') {
                    let k = r[..eq].trim().to_string(); let after = r[eq + 1..].trim_start();
                    let q = after.chars().next()?; if q != '"' && q != '\'' { return None; }
                    let end = after[1..].find(q)?; attrs.push((k, unescape(&after[1..1 + end]))); r = &after[end + 2..];
                }
                let id = nodes.len(); nodes.push(Node { name, attrs, kids: vec![], text: String::new() });
                let parent = *stack.last()?; nodes[parent].kids.push(id);
                if !selfc { stack.push(id); }
                if nodes.len() > 200_000 { return None; }
            }
            i += e + 1;
        } else {
            let e = src[i..].find('<').map_or(b.len(), |k| i + k);
            let t = unescape(&src[i..e]);
            for &n in &stack[1..] { nodes[n].text.push_str(&t); }
            i = e;
        }
    }
    if stack.len() != 1 || nodes[0].kids.is_empty() { return None; }
    Some(nodes)
}
fn unescape(s: &str) -> String { s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&") }

/// Evaluates a location path made of `/name`, `//name`, `*`, `[n]`, `[last()]`, `@attr` and `text()`.
fn filterxml(xml: &str, path: &str) -> Value {
    let Some(nodes) = parse_xml(xml) else { return err(ErrKind::Value) };
    let p = path.trim();
    if !p.starts_with('/') { return err(ErrKind::Value); }
    let mut cur = vec![0usize];
    let mut i = 0; let pb = p.as_bytes();
    let mut out: Option<Vec<String>> = None;
    while i < pb.len() {
        let deep = p[i..].starts_with("//"); i += if deep { 2 } else { 1 };
        let end = p[i..].find('/').map_or(p.len(), |k| i + k);
        let step = &p[i..end]; i = end;
        let (name, pred) = match step.find('[') { Some(k) => (&step[..k], Some(step[k + 1..].trim_end_matches(']'))), None => (step, None) };
        let gather = |n: usize, acc: &mut Vec<usize>| { fn walk(ns: &[Node], n: usize, acc: &mut Vec<usize>) { for &k in &ns[n].kids { acc.push(k); walk(ns, k, acc); } } if deep { walk(&nodes, n, acc) } else { acc.extend(&nodes[n].kids) } };
        if let Some(at) = name.strip_prefix('@') {
            if i < pb.len() { return err(ErrKind::Value); }
            let mut v = vec![]; for &n in &cur { let mut c = vec![n]; if deep { gather(n, &mut c); } for k in c { for (a, val) in &nodes[k].attrs { if at == "*" || a == at { v.push(val.clone()); } } } }
            out = Some(v); break;
        }
        if name == "text()" { out = Some(cur.iter().map(|&n| nodes[n].text.clone()).collect()); break; }
        if name.is_empty() || name.contains(|c: char| "()=<>'\"".contains(c)) { return err(ErrKind::Value); }
        let mut next = vec![];
        for &n in &cur {
            let mut c = vec![]; gather(n, &mut c);
            let hit: Vec<usize> = c.into_iter().filter(|&k| name == "*" || nodes[k].name == name).collect();
            match pred {
                None => next.extend(hit),
                Some("last()") => next.extend(hit.last()),
                Some(q) => match q.trim().parse::<usize>() { Ok(k) if k >= 1 => next.extend(hit.get(k - 1)), _ => return err(ErrKind::Value) },
            }
        }
        cur = next;
    }
    let texts = out.unwrap_or_else(|| cur.iter().filter(|&&n| n != 0).map(|&n| nodes[n].text.clone()).collect());
    if texts.is_empty() { return err(ErrKind::NA); }
    let vals: Vec<Value> = texts.iter().map(|t| { let t = t.split_whitespace().collect::<Vec<_>>().join(" "); crate::numfmt::parse_number(&t).map_or_else(|| Value::str(&t), Value::Num) }).collect();
    Value::array(Matrix::new(vals.len(), 1, vals))
}
