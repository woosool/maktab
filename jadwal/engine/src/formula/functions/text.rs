//! Text functions: LEN, LEFT, MID, FIND, SUBSTITUTE, TEXT, TEXTJOIN, CONCAT…

use super::*;

fn chars(s: &str) -> Vec<char> { s.chars().collect() }
fn count_arg(a: &[Value], i: usize, d: f64) -> Result<usize, ErrKind> { let k = opt(a, i, d)?; if k < 0.0 { Err(ErrKind::Value) } else { Ok(k.trunc() as usize) } }

pub const DEFS: &[Def] = &[
    s("LEN", 1, 1, |a| num(t!(a[0]).chars().count() as f64)),
    s("LEFT", 1, 2, |a| { let k = tryv!(count_arg(a, 1, 1.0)); Value::str(t!(a[0]).chars().take(k).collect::<String>()) }),
    s("RIGHT", 1, 2, |a| { let k = tryv!(count_arg(a, 1, 1.0)); let c = chars(&t!(a[0])); Value::str(c[c.len().saturating_sub(k)..].iter().collect::<String>()) }),
    s("MID", 3, 3, |a| { let st = n!(a[1]); let k = n!(a[2]); if st < 1.0 || k < 0.0 { return err(ErrKind::Value); } Value::str(t!(a[0]).chars().skip(st as usize - 1).take(k as usize).collect::<String>()) }),
    s("UPPER", 1, 1, |a| Value::str(t!(a[0]).to_uppercase())),
    s("LOWER", 1, 1, |a| Value::str(t!(a[0]).to_lowercase())),
    s("PROPER", 1, 1, |a| Value::str(proper(&t!(a[0])))),
    s("TRIM", 1, 1, |a| Value::str(t!(a[0]).split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" "))),
    s("CLEAN", 1, 1, |a| Value::str(t!(a[0]).chars().filter(|c| *c as u32 >= 32).collect::<String>())),
    s("REPT", 2, 2, |a| { let k = n!(a[1]); if k < 0.0 { return err(ErrKind::Value); } let s = t!(a[0]); if s.len() * k as usize > 32767 { return err(ErrKind::Value); } Value::str(s.repeat(k as usize)) }),
    s("EXACT", 2, 2, |a| boolv(t!(a[0]) == t!(a[1]))),
    s("CHAR", 1, 1, |a| { let k = n!(a[0]) as u32; if !(1..=255).contains(&k) { return err(ErrKind::Value); } Value::str(win1252(k as u8).to_string()) }),
    s("UNICHAR", 1, 1, |a| char::from_u32(n!(a[0]) as u32).filter(|c| *c as u32 > 0).map_or(err(ErrKind::Value), |c| Value::str(c.to_string()))),
    s("CODE", 1, 1, |a| t!(a[0]).chars().next().map_or(err(ErrKind::Value), |c| num(if (c as u32) < 128 { c as u32 } else { (1..=255u8).find(|k| win1252(*k) == c).map_or(63, |k| k as u32) } as f64))),
    s("UNICODE", 1, 1, |a| t!(a[0]).chars().next().map_or(err(ErrKind::Value), |c| num(c as u32 as f64))),
    s("CONCATENATE", 1, MANY, |a| { let mut s = String::new(); for v in a { s.push_str(&t!(v)); } Value::str(s) }),
    e("CONCAT", 1, MANY, |c, a| { let mut s = String::new(); for x in a { for v in flat(c, x) { s.push_str(&t!(v)); } } Value::str(s) }),
    e("TEXTJOIN", 3, MANY, textjoin),
    s("SUBSTITUTE", 3, 4, |a| {
        let (s, old, new) = (t!(a[0]), t!(a[1]), t!(a[2]));
        if old.is_empty() { return Value::str(s); }
        match a.get(3) {
            None | Some(Value::Empty) => Value::str(s.replace(&old, &new)),
            Some(v) => { let k = n!(v); if k < 1.0 { return err(ErrKind::Value); } let mut out = String::new(); let mut rest = &s[..]; let mut i = 0;
                while let Some(p) = rest.find(&old) { i += 1; if i == k as usize { out.push_str(&rest[..p]); out.push_str(&new); rest = &rest[p + old.len()..]; break; } out.push_str(&rest[..p + old.len()]); rest = &rest[p + old.len()..]; }
                out.push_str(rest); Value::str(out) }
        }
    }),
    s("REPLACE", 4, 4, |a| { let c = chars(&t!(a[0])); let st = n!(a[1]); let k = n!(a[2]); if st < 1.0 || k < 0.0 { return err(ErrKind::Value); }
        let st = (st as usize - 1).min(c.len()); let en = (st + k as usize).min(c.len());
        Value::str(c[..st].iter().collect::<String>() + t!(a[3]).as_str() + c[en..].iter().collect::<String>().as_str()) }),
    s("FIND", 2, 3, |a| find(a, true)),
    s("SEARCH", 2, 3, |a| find(a, false)),
    s("VALUE", 1, 1, |a| match &a[0] { Value::Num(n) => num(*n), Value::Empty => num(0.0), Value::Str(s) => crate::numfmt::parse_input(s.trim(), false).map_or(err(ErrKind::Value), |x| num(x.0)), Value::Bool(_) => err(ErrKind::Value), v => v.clone() }),
    s("NUMBERVALUE", 1, 3, |a| { let s = t!(a[0]); let dec = a.get(1).map(|v| v.to_text().unwrap_or_default()).filter(|s| !s.is_empty()).unwrap_or(".".into()); let grp = a.get(2).map(|v| v.to_text().unwrap_or_default()).filter(|s| !s.is_empty()).unwrap_or(",".into());
        let mut t: String = s.chars().filter(|c| !c.is_whitespace()).collect(); t = t.replace(&grp, ""); t = t.replace(&dec, ".");
        let pct = t.chars().rev().take_while(|c| *c == '%').count(); let t = t.trim_end_matches('%');
        if t.is_empty() { return num(0.0); } t.parse::<f64>().map_or(err(ErrKind::Value), |x| num(x / 100f64.powi(pct as i32))) }),
    s("TEXT", 2, 2, |a| { let f = t!(a[1]); match &a[0] { Value::Err(e) => Value::Err(*e), v => { let v = if let Value::Str(s) = v { crate::numfmt::parse_input(s, false).map(|x| Value::Num(x.0)).unwrap_or(v.clone()) } else { v.clone() }; Value::str(crate::numfmt::format(&v, &f, false).text) } } }),
    s("FIXED", 1, 3, |a| { let x = n!(a[0]); let d = tryv!(opt(a, 1, 2.0)) as i32; let no = a.get(2).map(|v| v.to_bool().unwrap_or(false)).unwrap_or(false);
        let r = super::math::round_to(x, d); let fmt = format!("{}{}{}", if no { "0" } else { "#,##0" }, if d > 0 { "." } else { "" }, "0".repeat(d.max(0) as usize));
        Value::str(crate::numfmt::format(&Value::Num(r), &fmt, false).text) }),
    s("DOLLAR", 1, 2, |a| { let x = n!(a[0]); let d = tryv!(opt(a, 1, 2.0)) as i32; let r = super::math::round_to(x, d);
        let fmt = format!("$#,##0{}{};($#,##0{}{})", if d > 0 { "." } else { "" }, "0".repeat(d.max(0) as usize), if d > 0 { "." } else { "" }, "0".repeat(d.max(0) as usize));
        Value::str(crate::numfmt::format(&Value::Num(r), &fmt, false).text) }),
    s("TEXTBEFORE", 2, 6, |a| text_around(a, true)),
    s("TEXTAFTER", 2, 6, |a| text_around(a, false)),
    e("TEXTSPLIT", 2, 6, textsplit),
    s("LEFTB", 1, 2, |a| call_alias("LEFT", a)), s("RIGHTB", 1, 2, |a| call_alias("RIGHT", a)), s("MIDB", 3, 3, |a| call_alias("MID", a)),
    s("LENB", 1, 1, |a| call_alias("LEN", a)), s("FINDB", 2, 3, |a| find(a, true)), s("SEARCHB", 2, 3, |a| find(a, false)), s("REPLACEB", 4, 4, |a| call_alias("REPLACE", a)),
    s("REGEXMATCH", 2, 2, |a| { let t = t!(a[0]); let re = tryv!(compile(&a[1], false, &t)); let c: Vec<char> = t.chars().collect(); boolv(tryv!(re.find_at(&c, 0).map_err(|_| ErrKind::Value)).is_some()) }),
    s("REGEXTEST", 2, 3, |a| { let t = t!(a[0]); let cs = tryv!(mode01(a, 2)); let re = tryv!(compile(&a[1], cs, &t)); let c: Vec<char> = t.chars().collect(); boolv(tryv!(re.find_at(&c, 0).map_err(|_| ErrKind::Value)).is_some()) }),
    s("REGEXEXTRACT", 2, 4, regexextract), s("REGEXREPLACE", 3, 5, regexreplace),
    e("ARRAYTOTEXT", 1, 2, |c, a| { let mode = tryv!(fmt_mode(&arg_val(c, a, 1))); let m = c.matrix_of(&a[0]);
        let (between, rows_sep, open, close) = if mode { (",", ";", "{", "}") } else { (", ", ", ", "", "") };
        let mut out = String::from(open);
        for r in 0..m.rows { if r > 0 { out.push_str(rows_sep); } for (j, v) in m.row(r).iter().enumerate() { if j > 0 { out.push_str(between); } out.push_str(&value_as_text(v, mode)); if out.len() > MAX_TEXT * 4 { return err(ErrKind::Value); } } }
        out.push_str(close); if out.chars().count() > MAX_TEXT { err(ErrKind::Value) } else { Value::str(out) } }),
    e("VALUETOTEXT", 1, 2, |c, a| { let mode = tryv!(fmt_mode(&arg_val(c, a, 1))); let o = value_as_text(&c.scalar_of(&a[0]), mode); if o.chars().count() > MAX_TEXT { err(ErrKind::Value) } else { Value::str(o) } }),
    s("ENCODEURL", 1, 1, |a| { let t = t!(a[0]); let mut o = String::new();
        for b in t.bytes() { if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { o.push(b as char) } else { o.push_str(&format!("%{b:02X}")) } }
        if o.len() > MAX_TEXT { err(ErrKind::Value) } else { Value::str(o) } }),
    s("ASC", 1, 1, |a| Value::str(t!(a[0]).chars().map(|c| match c { '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c), '\u{3000}' => ' ', c => c }).collect::<String>())),
    s("DBCS", 1, 1, dbcs), s("JIS", 1, 1, dbcs),
    s("BAHTTEXT", 1, 1, bahttext),
    e("PHONETIC", 1, 1, |_, _| err(ErrKind::NA)),
    s("SPLIT", 2, 4, split),
    e("JOIN", 2, MANY, |c, a| { let d = t!(arg_val(c, a, 0)); let mut parts = vec![]; let mut len = 0usize;
        for x in &a[1..] { for v in flat(c, x) { let s = t!(v); len += s.chars().count() + d.chars().count(); if len > MAX_TEXT + d.chars().count() { return err(ErrKind::Value); } parts.push(s); } }
        Value::str(parts.join(&d)) }),
    s("TO_TEXT", 1, 1, |a| Value::str(t!(a[0]))),
    s("TO_DOLLARS", 1, 1, |a| num(n!(a[0]))), s("TO_PERCENT", 1, 1, |a| num(n!(a[0]))), s("TO_DATE", 1, 1, |a| num(n!(a[0]))),
    s("TO_PURE_NUMBER", 1, 1, |a| match &a[0] { Value::Num(x) => num(*x), Value::Err(e) => Value::Err(*e), v => num(n!(v)) }),
    s("ISEMAIL", 1, 1, |a| boolv(matches!(&a[0], Value::Str(s) if is_email(s.trim())))),
    s("ISURL", 1, 1, |a| boolv(matches!(&a[0], Value::Str(s) if is_url(s.trim())))),
];

fn proper(s: &str) -> String {
    let mut out = String::new(); let mut prev_letter = false;
    for c in s.chars() { if c.is_alphabetic() { if prev_letter { out.extend(c.to_lowercase()) } else { out.extend(c.to_uppercase()) } prev_letter = true; } else { out.push(c); prev_letter = false; } }
    out
}

/// Windows-1252, the code page CHAR and CODE use.
pub fn win1252(k: u8) -> char {
    const HI: [u32; 32] = [0x20AC, 0x81, 0x201A, 0x192, 0x201E, 0x2026, 0x2020, 0x2021, 0x2C6, 0x2030, 0x160, 0x2039, 0x152, 0x8D, 0x17D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x2DC, 0x2122, 0x161, 0x203A, 0x153, 0x9D, 0x17E, 0x178];
    if (0x80..0xA0).contains(&k) { char::from_u32(HI[(k - 0x80) as usize]).unwrap() } else { k as char }
}

fn find(a: &[Value], case: bool) -> Value {
    let (needle, hay) = (t!(a[0]), t!(a[1]));
    let st = tryv!(opt(a, 2, 1.0));
    let hc = chars(&hay);
    if st < 1.0 || st as usize > hc.len() + 1 { return err(ErrKind::Value); }
    let from = st as usize - 1;
    if needle.is_empty() { return num(st); }
    if case {
        let sub: String = hc[from..].iter().collect();
        return sub.find(&needle).map_or(err(ErrKind::Value), |p| num((from + sub[..p].chars().count() + 1) as f64));
    }
    let nc = chars(&needle);
    for i in from..hc.len() {
        for j in i..=hc.len() {
            if super::criteria::matches_pattern(&needle, &hc[i..j].iter().collect::<String>()) { return num(i as f64 + 1.0); }
            if !nc.contains(&'*') && j - i > nc.len() { break; }
        }
    }
    err(ErrKind::Value)
}

fn textjoin(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let delim = t!(ctx.scalar_of(&a[0]));
    let skip = tryv!(arg_bool(ctx, a, 1, true));
    let mut parts = vec![];
    for x in &a[2..] { for v in flat(ctx, x) { let s = t!(v); if !(skip && s.is_empty()) { parts.push(s); } } }
    let s = parts.join(&delim);
    if s.chars().count() > 32767 { err(ErrKind::Value) } else { Value::str(s) }
}

fn text_around(a: &[Value], before: bool) -> Value {
    let s = t!(a[0]); let d = t!(a[1]);
    let inst = tryv!(opt(a, 2, 1.0)) as i64;
    let nocase = tryv!(opt(a, 3, 0.0)) != 0.0;
    if inst == 0 { return err(ErrKind::Value); }
    let (hs, ds) = if nocase { (s.to_lowercase(), d.to_lowercase()) } else { (s.clone(), d.clone()) };
    let mut idx: Vec<usize> = if ds.is_empty() { vec![] } else { hs.match_indices(&ds).map(|(i, _)| i).collect() };
    if ds.is_empty() { idx = if inst > 0 { vec![0] } else { vec![s.len()] }; }
    let pick = if inst > 0 { idx.get(inst as usize - 1) } else { idx.len().checked_sub((-inst) as usize).and_then(|k| idx.get(k)) };
    match pick {
        Some(&i) => Value::str(if before { &s[..i] } else { &s[i + d.len()..] }),
        None => if let Some(v) = a.get(5).filter(|v| !v.is_empty()) { v.clone() } else { err(ErrKind::NA) },
    }
}

fn textsplit(ctx: &mut Ctx, a: &[Arg]) -> Value {
    let s = t!(ctx.scalar_of(&a[0]));
    let cols: Vec<String> = flat(ctx, &a[1]).iter().filter_map(|v| v.to_text().ok()).filter(|x| !x.is_empty()).collect();
    let rows: Vec<String> = a.get(2).filter(|x| !x.is_missing()).map(|x| flat(ctx, x).iter().filter_map(|v| v.to_text().ok()).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
    let ignore = tryv!(arg_bool(ctx, a, 3, false));
    let pad = a.get(5).map(|x| ctx.scalar_of(x)).filter(|v| !v.is_empty()).unwrap_or(Value::Err(ErrKind::NA));
    let split = |t: &str, ds: &[String]| -> Vec<String> {
        if ds.is_empty() { return vec![t.to_string()]; }
        let mut out = vec![]; let mut cur = String::new(); let mut i = 0; let b = t.as_bytes();
        'o: while i < b.len() { for d in ds { if t[i..].starts_with(d.as_str()) { out.push(std::mem::take(&mut cur)); i += d.len(); continue 'o; } } let ch = t[i..].chars().next().unwrap(); cur.push(ch); i += ch.len_utf8(); }
        out.push(cur);
        if ignore { out.retain(|x| !x.is_empty()); }
        out
    };
    let lines: Vec<Vec<String>> = split(&s, &rows).iter().map(|r| split(r, &cols)).collect();
    let w = lines.iter().map(|r| r.len()).max().unwrap_or(1);
    let data: Vec<Vec<Value>> = lines.into_iter().map(|r| { let mut v: Vec<Value> = r.into_iter().map(Value::str).collect(); v.resize(w, pad.clone()); v }).collect();
    Value::array(Matrix::from_rows(data))
}

const MAX_TEXT: usize = 32767;

/// The byte variants (LEFTB…) behave as their character forms, as in the JS (no DBCS locale).
fn call_alias(name: &str, a: &[Value]) -> Value {
    match get(name).map(|d| d.kind) { Some(Kind::Scalar(f)) => f(a), _ => err(ErrKind::Name) }
}

fn mode01(a: &[Value], i: usize) -> Result<bool, ErrKind> { let m = opt(a, i, 0.0)?; if m == 0.0 || m == 1.0 { Ok(m == 1.0) } else { Err(ErrKind::Value) } }
fn fmt_mode(v: &Value) -> Result<bool, ErrKind> { let m = match v { Value::Empty => 0.0, v => v.to_num()?.trunc() }; if m == 0.0 || m == 1.0 { Ok(m == 1.0) } else { Err(ErrKind::Value) } }

/// A user's pattern compiled, or #VALUE! when it is invalid (or the text too long to search safely).
fn compile(p: &Value, icase: bool, subject: &str) -> Result<super::regex::Regex, ErrKind> {
    let p = p.to_text()?;
    if subject.len() > 1_000_000 { return Err(ErrKind::Value); }
    super::regex::Regex::new(&p, icase).ok_or(ErrKind::Value)
}

fn regexextract(a: &[Value]) -> Value {
    use super::regex::span;
    let t = t!(a[0]);
    let mode = match a.get(2) { None | Some(Value::Empty) => None, Some(v) => Some(n!(v)) };
    if mode.is_some_and(|m| m != 0.0 && m != 1.0 && m != 2.0) { return err(ErrKind::Value); }
    let cs = tryv!(mode01(a, 3));
    let re = tryv!(compile(&a[1], cs, &t));
    let c: Vec<char> = t.chars().collect();
    if mode == Some(1.0) {
        let all = tryv!(re.find_all(&c).map_err(|_| ErrKind::Value));
        if all.is_empty() { return err(ErrKind::NA); }
        let n = all.len();
        return Value::array(Matrix::new(n, 1, all.iter().map(|m| Value::str(span(&c, m[0]).unwrap_or_default())).collect()));
    }
    let Some(m) = tryv!(re.find_at(&c, 0).map_err(|_| ErrKind::Value)) else { return err(ErrKind::NA) };
    let whole = Value::str(span(&c, m[0]).unwrap_or_default());
    let groups = || { let g: Vec<Value> = m[1..].iter().map(|s| Value::str(span(&c, *s).unwrap_or_default())).collect(); Value::array(Matrix::new(1, g.len(), g)) };
    match mode {
        Some(0.0) => whole,
        Some(_) => if m.len() > 1 { groups() } else { whole },
        None => if m.len() > 2 { groups() } else if m.len() == 2 { Value::str(span(&c, m[1]).unwrap_or_default()) } else { whole },
    }
}

fn regexreplace(a: &[Value]) -> Value {
    let t = t!(a[0]);
    let k = tryv!(opt(a, 3, 0.0)).trunc();
    let cs = tryv!(mode01(a, 4));
    let re = tryv!(compile(&a[1], cs, &t));
    let rep: Vec<char> = t!(a[2]).chars().collect();
    let c: Vec<char> = t.chars().collect();
    let mut all = tryv!(re.find_all(&c).map_err(|_| ErrKind::Value));
    if k != 0.0 {
        let at = if k > 0.0 { k - 1.0 } else { all.len() as f64 + k };
        if at < 0.0 || at >= all.len() as f64 { return Value::str(t); }
        all = vec![all.swap_remove(at as usize)];
    }
    let mut out = String::new(); let mut last = 0;
    for m in &all {
        let Some((s, e)) = m[0] else { continue };
        out.extend(&c[last..s]);
        // $0 / $& is the match, $n a group, $$ a dollar; anything else is literal.
        let mut i = 0;
        while i < rep.len() {
            let ch = rep[i];
            if ch == '$' && i + 1 < rep.len() {
                let nx = rep[i + 1];
                if nx == '$' { out.push('$'); i += 2; continue; }
                if nx == '&' { out.extend(&c[s..e]); i += 2; continue; }
                if nx.is_ascii_digit() {
                    let two = rep.get(i + 2).filter(|d| d.is_ascii_digit()).map(|d| (nx as usize - 48) * 10 + (*d as usize - 48));
                    let (g, w) = match two { Some(g) if g < m.len() && g > 0 => (g, 3), _ => (nx as usize - 48, 2) };
                    if g < m.len() { if let Some((x, y)) = m[g] { out.extend(&c[x..y]); } i += w; continue; }
                }
            }
            out.push(ch); i += 1;
            if out.len() > MAX_TEXT * 4 { return err(ErrKind::Value); }
        }
        last = e;
        if out.chars().count() > MAX_TEXT { return err(ErrKind::Value); }
    }
    out.extend(&c[last..]);
    if out.chars().count() > MAX_TEXT { err(ErrKind::Value) } else { Value::str(out) }
}

/// A value as ARRAYTOTEXT/VALUETOTEXT write it; `strict` quotes text.
fn value_as_text(v: &Value, strict: bool) -> String {
    match v {
        Value::Empty => String::new(), Value::Err(e) => e.text().into(), Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        Value::Num(n) => crate::value::fmt_num(*n),
        Value::Str(s) => if strict { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() },
        Value::Array(_) => "#VALUE!".into(),
    }
}

fn dbcs(a: &[Value]) -> Value { Value::str(t!(a[0]).chars().map(|c| match c { '!'..='~' => char::from_u32(c as u32 + 0xFEE0).unwrap_or(c), ' ' => '\u{3000}', c => c }).collect::<String>()) }

fn split(a: &[Value]) -> Value {
    let t = t!(a[0]); let d = t!(a[1]);
    let each = match a.get(2) { None | Some(Value::Empty) => true, Some(v) => b!(v) };
    let skip = match a.get(3) { None | Some(Value::Empty) => true, Some(v) => b!(v) };
    let mut parts: Vec<String> = if d.is_empty() { vec![t] } else if each { t.split(|c| d.contains(c)).map(String::from).collect() } else { t.split(d.as_str()).map(String::from).collect() };
    if skip { parts.retain(|p| !p.is_empty()); }
    if parts.is_empty() { return err(ErrKind::NA); }
    let n = parts.len();
    // Pieces that read as numbers become numbers, as in Sheets.
    Value::array(Matrix::new(1, n, parts.into_iter().map(|p| match crate::numfmt::parse_number(p.trim()) { Some(x) if !p.trim().is_empty() => Value::Num(x), _ => Value::str(p) }).collect()))
}

/// Thai words for 1..=999,999; `higher` when a higher block precedes it.
fn thai_block(mut n: u64, higher: bool) -> String {
    const D: [&str; 10] = ["", "หนึ่ง", "สอง", "สาม", "สี่", "ห้า", "หก", "เจ็ด", "แปด", "เก้า"];
    let whole = n; let mut out = String::new();
    for (p, w) in [(100000, "แสน"), (10000, "หมื่น"), (1000, "พัน"), (100, "ร้อย")] { if n >= p { out += D[(n / p) as usize]; out += w; n %= p; } }
    let (ten, one) = (n / 10, n % 10);
    if ten >= 1 { out += if ten == 2 { "ยี่" } else if ten >= 3 { D[ten as usize] } else { "" }; out += "สิบ"; }
    if one == 1 && (whole > 1 || higher) { out += "เอ็ด" } else if one > 0 { out += D[one as usize] }
    out
}

fn bahttext(a: &[Value]) -> Value {
    let x = n!(a[0]);
    let satangs = (x.abs() * 100.0 + 0.5).floor();
    if !satangs.is_finite() || satangs > 1.8e19 { return err(ErrKind::Num); }
    let satangs = satangs as u64;
    let (mut baht, satang) = (satangs / 100, satangs % 100);
    let mut text = String::new();
    if baht == 0 && satang == 0 { text = "ศูนย์".into(); }
    while baht > 0 {
        let block = baht % 1_000_000; baht /= 1_000_000;
        text = format!("{}{}{}", if baht > 0 { "ล้าน" } else { "" }, if block > 0 { thai_block(block, baht > 0) } else { String::new() }, text);
    }
    if !text.is_empty() { text += "บาท"; }
    if satang == 0 { text += "ถ้วน" } else { text += &thai_block(satang, false); text += "สตางค์"; }
    Value::str(if x < 0.0 { format!("ลบ{text}") } else { text })
}

fn label_ok(l: &str) -> bool { !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-') && !l.starts_with('-') && !l.ends_with('-') }

/// local@label.label…: no spaces, @ or quotes in the local part.
fn is_email(s: &str) -> bool {
    let Some((local, dom)) = s.split_once('@') else { return false };
    !local.is_empty() && !local.chars().any(|c| c.is_whitespace() || c == '"' || c == '@') && dom.contains('.') && dom.split('.').all(label_ok)
}

/// A web address: a scheme (http, https, ftp, mailto) or www., or a bare host.tld[/path].
fn is_url(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    if s.chars().any(char::is_whitespace) { return false; }
    for p in ["http://", "https://", "ftp://"] { if let Some(rest) = l.strip_prefix(p) { let host = rest.split(['/', '?', '#']).next().unwrap_or(""); let host = host.rsplit('@').next().unwrap_or(""); let host = host.split(':').next().unwrap_or(""); return !host.is_empty() && host.split('.').all(label_ok); } }
    if let Some(rest) = l.strip_prefix("mailto:") { return !rest.is_empty(); }
    let host = l.split('/').next().unwrap_or("");
    host.contains('.') && host.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
}
