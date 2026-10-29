//! Number formats (numfmt.js): Excel format codes (sections, colors, conditions, digits, thousands,
//! percent, scientific, fractions, dates and times, elapsed time, text), the built-in format ids,
//! and reading typed input ("1,234", "12%", "$5", "1/15/2024", "13:45") into numbers.

use crate::date::{hms, serial, weekday, ymd};
use crate::value::{fmt_num, Value};

/// The Office theme's colors: dk1, lt1, dk2, lt2, accent1–6, hlink, folHlink.
pub const OFFICE_THEME: [[u8; 3]; 12] = [
    [0, 0, 0], [255, 255, 255], [0x0E, 0x28, 0x41], [0xE8, 0xE8, 0xE8], [0x15, 0x60, 0x82], [0xE9, 0x71, 0x32],
    [0x19, 0x6B, 0x24], [0x0F, 0x9E, 0xD5], [0xA0, 0x2B, 0x93], [0x4E, 0xA7, 0x2E], [0x46, 0x78, 0xC8], [0x96, 0x60, 0x7D],
];

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Formatted { pub text: String, pub color: Option<[u8; 3]> }

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Format codes of the built-in number format ids a file may use without listing them.
pub fn builtin(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General", 1 => "0", 2 => "0.00", 3 => "#,##0", 4 => "#,##0.00", 9 => "0%", 10 => "0.00%", 11 => "0.00E+00", 12 => "# ?/?", 13 => "# ??/??",
        14 => "m/d/yyyy", 15 => "d-mmm-yy", 16 => "d-mmm", 17 => "mmm-yy", 18 => "h:mm AM/PM", 19 => "h:mm:ss AM/PM", 20 => "h:mm", 21 => "h:mm:ss", 22 => "m/d/yyyy h:mm",
        37 => "#,##0 ;(#,##0)", 38 => "#,##0 ;[Red](#,##0)", 39 => "#,##0.00;(#,##0.00)", 40 => "#,##0.00;[Red](#,##0.00)",
        41 => r#"_(* #,##0_);_(* \(#,##0\);_(* "-"_);_(@_)"#, 42 => r#"_("$"* #,##0_);_("$"* \(#,##0\);_("$"* "-"_);_(@_)"#,
        43 => r#"_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)"#, 44 => r#"_("$"* #,##0.00_);_("$"* \(#,##0.00\);_("$"* "-"??_);_(@_)"#,
        45 => "mm:ss", 46 => "[h]:mm:ss", 47 => "mmss.0", 48 => "##0.0E+0", 49 => "@",
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
enum T { Lit(String), Zero, Hash, Q, Dot, Comma, Pct, Exp(bool), Slash, At, Date(String), AmPm(String), Elapsed(char, usize), General, Fill(char), Pad(char) }

#[derive(Clone, Debug, Default)]
struct Section { toks: Vec<T>, color: Option<[u8; 3]>, cond: Option<(String, f64)> }

fn color_named(s: &str) -> Option<[u8; 3]> {
    let l = s.to_ascii_lowercase();
    Some(match l.as_str() {
        "black" => [0, 0, 0], "white" => [255, 255, 255], "red" => [255, 0, 0], "green" => [0, 128, 0], "blue" => [0, 0, 255],
        "yellow" => [255, 255, 0], "magenta" => [255, 0, 255], "cyan" => [0, 255, 255],
        _ => { let k: usize = l.strip_prefix("color")?.parse().ok()?; return INDEXED.get(k.wrapping_sub(1)).copied(); }
    })
}

/// The legacy 56-color palette ([Color1]…[Color56] and indexed colors 8–63).
pub const INDEXED: [[u8; 3]; 56] = [
    [0,0,0],[255,255,255],[255,0,0],[0,255,0],[0,0,255],[255,255,0],[255,0,255],[0,255,255],[128,0,0],[0,128,0],[0,0,128],[128,128,0],[128,0,128],[0,128,128],[192,192,192],[128,128,128],
    [153,153,255],[153,51,102],[255,255,204],[204,255,255],[102,0,102],[255,128,128],[0,102,204],[204,204,255],[0,0,128],[255,0,255],[255,255,0],[0,255,255],[128,0,128],[128,0,0],[0,128,128],[0,0,255],
    [0,204,255],[204,255,255],[204,255,204],[255,255,153],[153,204,255],[255,153,204],[204,153,255],[255,204,153],[51,102,255],[51,204,204],[153,204,0],[255,204,0],[255,153,0],[255,102,0],[102,102,153],[150,150,150],
    [0,51,102],[51,153,102],[0,51,0],[51,51,0],[153,51,0],[153,51,102],[51,51,153],[51,51,51],
];

fn parse_sections(code: &str) -> Vec<Section> {
    let c: Vec<char> = code.chars().collect();
    let mut out = vec![Section::default()];
    let mut i = 0;
    let lit = |s: &mut Section, x: String| { if let Some(T::Lit(p)) = s.toks.last_mut() { p.push_str(&x); } else { s.toks.push(T::Lit(x)); } };
    while i < c.len() {
        let sec = out.last_mut().unwrap();
        let ch = c[i];
        match ch {
            ';' => { out.push(Section::default()); i += 1; }
            '"' => { let mut j = i + 1; let mut s = String::new(); while j < c.len() && c[j] != '"' { s.push(c[j]); j += 1; } lit(sec, s); i = j + 1; }
            '\\' => { if i + 1 < c.len() { lit(sec, c[i + 1].to_string()); } i += 2; }
            '_' => { if i + 1 < c.len() { sec.toks.push(T::Pad(c[i + 1])); } i += 2; }
            '*' => { if i + 1 < c.len() { sec.toks.push(T::Fill(c[i + 1])); } i += 2; }
            '[' => {
                let mut j = i + 1; let mut s = String::new(); while j < c.len() && c[j] != ']' { s.push(c[j]); j += 1; }
                i = j + 1;
                let l = s.to_ascii_lowercase();
                if let Some(col) = color_named(&s) { sec.color = Some(col); }
                else if l.chars().all(|x| x == 'h') && !l.is_empty() { sec.toks.push(T::Elapsed('h', l.len())); }
                else if l.chars().all(|x| x == 'm') && !l.is_empty() { sec.toks.push(T::Elapsed('m', l.len())); }
                else if l.chars().all(|x| x == 's') && !l.is_empty() { sec.toks.push(T::Elapsed('s', l.len())); }
                else if let Some(rest) = s.strip_prefix(['<', '>', '=']).map(|_| &s[..]) {
                    let op: String = rest.chars().take_while(|x| "<>=".contains(*x)).collect();
                    if let Ok(v) = rest[op.len()..].trim().parse::<f64>() { sec.cond = Some((op, v)); }
                } else if let Some(cur) = s.strip_prefix('$') { lit(sec, cur.split('-').next().unwrap_or("").to_string()); }
            }
            '0' => { sec.toks.push(T::Zero); i += 1; }
            '#' => { sec.toks.push(T::Hash); i += 1; }
            '?' => { sec.toks.push(T::Q); i += 1; }
            '.' => { sec.toks.push(T::Dot); i += 1; }
            ',' => { sec.toks.push(T::Comma); i += 1; }
            '%' => { sec.toks.push(T::Pct); i += 1; }
            '/' => { sec.toks.push(T::Slash); i += 1; }
            '@' => { sec.toks.push(T::At); i += 1; }
            'E' | 'e' if matches!(c.get(i + 1), Some('+' | '-')) => { sec.toks.push(T::Exp(c[i + 1] == '+')); i += 2; }
            _ => {
                let rest: String = c[i..].iter().collect();
                let low = rest.to_ascii_lowercase();
                if low.starts_with("general") { sec.toks.push(T::General); i += 7; continue; }
                if low.starts_with("am/pm") { sec.toks.push(T::AmPm(rest[..5].to_string())); i += 5; continue; }
                if low.starts_with("a/p") { sec.toks.push(T::AmPm(rest[..3].to_string())); i += 3; continue; }
                let l = ch.to_ascii_lowercase();
                if "ymdhsbge".contains(l) && ch != 'E' {
                    let mut j = i; while j < c.len() && c[j].to_ascii_lowercase() == l { j += 1; }
                    sec.toks.push(T::Date(c[i..j].iter().collect::<String>().to_ascii_lowercase()));
                    i = j; continue;
                }
                lit(sec, ch.to_string()); i += 1;
            }
        }
    }
    out
}

fn is_date(s: &Section) -> bool { s.toks.iter().any(|t| matches!(t, T::Date(_) | T::Elapsed(..) | T::AmPm(_))) }

/// True for a format that shows dates or times.
pub fn is_date_format(code: &str) -> bool { parse_sections(code).first().is_some_and(is_date) }
pub fn is_percent_format(code: &str) -> bool { parse_sections(code).first().is_some_and(|s| s.toks.contains(&T::Pct)) }
pub fn is_text_format(code: &str) -> bool { code.trim() == "@" }

pub fn format(v: &Value, code: &str, date1904: bool) -> Formatted {
    match v {
        Value::Num(n) => format_number(*n, code, date1904),
        Value::Str(s) => {
            let secs = parse_sections(code);
            match secs.get(3).or_else(|| secs.iter().find(|s| s.toks.contains(&T::At))) {
                Some(sec) if sec.toks.iter().any(|t| matches!(t, T::At | T::Lit(_))) => Formatted { text: sec.toks.iter().map(|t| match t { T::At => s.to_string(), T::Lit(l) => l.clone(), T::Pad(_) => " ".into(), _ => String::new() }).collect(), color: sec.color },
                _ => Formatted { text: s.to_string(), color: None },
            }
        }
        Value::Bool(b) => Formatted { text: if *b { "TRUE" } else { "FALSE" }.into(), color: None },
        Value::Err(e) => Formatted { text: e.text().into(), color: None },
        Value::Empty => Formatted::default(),
        Value::Array(m) => format(m.data.first().unwrap_or(&Value::Empty), code, date1904),
    }
}

pub fn format_number(n: f64, code: &str, date1904: bool) -> Formatted {
    if code.is_empty() || code.eq_ignore_ascii_case("general") { return Formatted { text: general(n, 11), color: None }; }
    let secs = parse_sections(code);
    // Pick the section: conditions when present, else positive;negative;zero.
    let test = |c: &(String, f64)| match c.0.as_str() { "<" => n < c.1, ">" => n > c.1, "=" => n == c.1, "<=" => n <= c.1, ">=" => n >= c.1, "<>" => n != c.1, _ => false };
    let has_cond = secs.iter().any(|s| s.cond.is_some());
    let (sec, abs) = if has_cond {
        match secs.iter().position(|s| s.cond.as_ref().is_some_and(test)) {
            Some(i) => (&secs[i], i > 0 || secs[i].cond.as_ref().is_some_and(|c| c.0.contains('<') && c.1 <= 0.0)),
            None => { let rest: Vec<&Section> = secs.iter().filter(|s| s.cond.is_none()).collect(); match rest.first() { Some(s) => (*s, n < 0.0 && secs.len() > 1), None => (&secs[0], false) } }
        }
    } else if n < 0.0 && secs.len() > 1 && !secs[1].toks.is_empty() { (&secs[1], true) }
    else if n == 0.0 && secs.len() > 2 { (&secs[2], true) }
    else { (&secs[0], false) };
    let x = if abs { n.abs() } else { n };
    let text = if is_date(sec) { format_date(x, sec, date1904) } else { format_num(x, sec) };
    Formatted { text, color: sec.color }
}

/// General: up to `width` characters, switching to scientific for very large or small numbers.
pub fn general(n: f64, width: usize) -> String {
    let s = fmt_num(n);
    if s.len() <= width || s.contains('E') { return s; }
    let a = n.abs();
    if a >= 1e11 || a < 1e-9 {
        let s = format!("{:.5E}", n); let (m, e) = s.split_once('E').unwrap();
        let m = m.trim_end_matches('0').trim_end_matches('.'); let e: i32 = e.parse().unwrap_or(0);
        return format!("{m}E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    // Round away digits that do not fit.
    let int_len = (a.log10().floor() as i64 + 1).max(1) as usize + (n < 0.0) as usize;
    let dec = width.saturating_sub(int_len + 1);
    let r = format!("{:.*}", dec, n);
    if r.contains('.') { r.trim_end_matches('0').trim_end_matches('.').to_string() } else { r }
}

fn format_num(n: f64, sec: &Section) -> String {
    let t = &sec.toks;
    if t.iter().any(|x| *x == T::General) {
        return t.iter().map(|x| match x { T::General => general(n, 11), T::Lit(l) => l.clone(), T::Pad(_) => " ".into(), _ => String::new() }).collect();
    }
    if !t.iter().any(|x| matches!(x, T::Zero | T::Hash | T::Q)) {
        // Only literals (e.g. "-"): show them, with the sign for negatives.
        let s: String = t.iter().map(|x| match x { T::Lit(l) => l.clone(), T::Pad(_) | T::Fill(_) => " ".into(), T::Pct => "%".into(), T::At => fmt_num(n), _ => String::new() }).collect();
        return s;
    }
    let mut n = n;
    let pct = t.iter().filter(|x| **x == T::Pct).count();
    n *= 100f64.powi(pct as i32);
    // Commas right after the last digit placeholder scale by 1000 each.
    let last_ph = t.iter().rposition(|x| matches!(x, T::Zero | T::Hash | T::Q)).unwrap();
    let mut scale = 0;
    let mut k = last_ph + 1;
    while k < t.len() && t[k] == T::Comma { scale += 1; k += 1; }
    if t.iter().any(|x| *x == T::Dot) { let dot = t.iter().position(|x| *x == T::Dot).unwrap(); let mut k = dot; while k > 0 && t[k - 1] == T::Comma { scale += 1; k -= 1; } }
    n /= 1000f64.powi(scale);
    if t.contains(&T::Slash) { return fraction(n, t); }
    if let Some(ei) = t.iter().position(|x| matches!(x, T::Exp(_))) { return scientific(n, t, ei); }
    let dot = t.iter().position(|x| *x == T::Dot);
    let (int_t, dec_t) = match dot { Some(d) => (&t[..d], &t[d + 1..]), None => (t.as_slice(), &[][..]) };
    let decs = dec_t.iter().filter(|x| matches!(x, T::Zero | T::Hash | T::Q)).count();
    let thousands = int_t.iter().enumerate().any(|(i, x)| *x == T::Comma && int_t[..i].iter().any(|y| matches!(y, T::Zero | T::Hash | T::Q)) && int_t[i + 1..].iter().any(|y| matches!(y, T::Zero | T::Hash | T::Q)));
    let neg = n < 0.0;
    let r = round_half_away(n.abs(), decs);
    let s = format!("{:.*}", decs, r);
    let (ip, dp) = s.split_once('.').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((s.clone(), String::new()));
    let ip = if ip == "0" { String::new() } else { ip };
    // Integer part: walk placeholders right to left.
    let mut digits: Vec<char> = ip.chars().collect();
    let mut out_rev: Vec<String> = vec![];
    let n_ph = int_t.iter().filter(|x| matches!(x, T::Zero | T::Hash | T::Q)).count();
    let mut seen = 0;
    let mut count_since_sep = 0;
    for tk in int_t.iter().rev() {
        match tk {
            T::Zero | T::Hash | T::Q => {
                seen += 1;
                let mut chunk = String::new();
                if seen == n_ph {
                    // The leftmost placeholder takes every remaining digit.
                    let mut rest: Vec<char> = vec![];
                    while let Some(d) = digits.pop() { rest.push(d); }
                    if rest.is_empty() { match tk { T::Zero => rest.push('0'), T::Q => rest.push(' '), _ => {} } }
                    for d in rest { if thousands && count_since_sep == 3 && d != ' ' { chunk.push(','); count_since_sep = 0; } chunk.push(d); count_since_sep += 1; }
                    out_rev.push(chunk.chars().rev().collect::<String>().chars().rev().collect());
                } else {
                    let d = digits.pop().unwrap_or(match tk { T::Zero => '0', T::Q => ' ', _ => '\0' });
                    if d != '\0' {
                        if thousands && count_since_sep == 3 { chunk.push(','); count_since_sep = 0; }
                        chunk.push(d); count_since_sep += 1;
                    }
                    out_rev.push(chunk);
                }
            }
            T::Lit(l) => out_rev.push(l.chars().rev().collect()),
            T::Pad(_) | T::Fill(_) => out_rev.push(" ".into()),
            T::Pct => out_rev.push("%".into()),
            T::At => out_rev.push(fmt_num(n).chars().rev().collect()),
            _ => {}
        }
    }
    let mut out: String = out_rev.concat().chars().rev().collect();
    if let Some(_) = dot {
        // Trailing # decimals drop their zeros; ? become spaces.
        let mut dchars: Vec<char> = dp.chars().collect();
        let kinds: Vec<&T> = dec_t.iter().filter(|x| matches!(x, T::Zero | T::Hash | T::Q)).collect();
        for i in (0..dchars.len()).rev() { if dchars[i] == '0' { match kinds[i] { T::Hash => { dchars.pop(); } T::Q => dchars[i] = ' ', _ => break } } else { break } }
        let mut di = 0; let mut dec_out = String::from(".");
        for tk in dec_t { match tk { T::Zero | T::Hash | T::Q => { if di < dchars.len() { dec_out.push(dchars[di]); } di += 1; } T::Lit(l) => dec_out.push_str(l), T::Pad(_) => dec_out.push(' '), T::Pct => dec_out.push('%'), _ => {} } }
        out.push_str(&dec_out);
    }
    if neg && (r != 0.0) { out.insert(0, '-'); }
    out
}

fn round_half_away(x: f64, d: usize) -> f64 {
    let f = 10f64.powi(d as i32);
    let y: f64 = format!("{:.12e}", x * f).parse().unwrap_or(x * f);
    y.round() / f
}

fn scientific(n: f64, t: &[T], ei: usize) -> String {
    let mant_t = &t[..ei]; let exp_t = &t[ei + 1..];
    let plus = matches!(t[ei], T::Exp(true));
    let int_ph = mant_t.iter().take_while(|x| **x != T::Dot).filter(|x| matches!(x, T::Zero | T::Hash | T::Q)).count().max(1);
    let decs = mant_t.iter().skip_while(|x| **x != T::Dot).filter(|x| matches!(x, T::Zero | T::Hash | T::Q)).count();
    let exp_digits = exp_t.iter().filter(|x| matches!(x, T::Zero | T::Hash | T::Q)).count().max(1);
    if n == 0.0 { return format!("{}{}E{}{}", "0", if decs > 0 { format!(".{}", "0".repeat(decs)) } else { String::new() }, if plus { "+" } else { "" }, "0".repeat(exp_digits)); }
    let mut e = n.abs().log10().floor() as i32;
    // Engineering style (##0.0E+0): exponent a multiple of the integer placeholders.
    if int_ph > 1 { e = e.div_euclid(int_ph as i32) * int_ph as i32; }
    let mut m = n.abs() / 10f64.powi(e);
    m = round_half_away(m, decs);
    if m >= 10f64.powi(int_ph as i32) { m /= 10f64.powi(int_ph as i32); e += int_ph as i32; }
    let ms = format!("{:.*}", decs, m);
    let es = format!("{:0width$}", e.abs(), width = exp_digits);
    format!("{}{}E{}{}", if n < 0.0 { "-" } else { "" }, ms, if e < 0 { "-" } else if plus { "+" } else { "" }, es)
}

fn fraction(n: f64, t: &[T]) -> String {
    let slash = t.iter().position(|x| *x == T::Slash).unwrap();
    let den_t = &t[slash + 1..];
    let den_digits = den_t.iter().filter(|x| matches!(x, T::Zero | T::Hash | T::Q)).count();
    let fixed: Option<u64> = den_t.iter().filter_map(|x| if let T::Lit(l) = x { l.trim().parse().ok() } else { None }).next();
    // An integer part exists when placeholders, then a literal space, come before the numerator.
    let before = &t[..slash];
    let num_start = before.iter().rposition(|x| !matches!(x, T::Zero | T::Hash | T::Q)).map_or(0, |i| i + 1);
    let has_int = before[..num_start].iter().any(|x| matches!(x, T::Zero | T::Hash | T::Q));
    let neg = n < 0.0; let a = n.abs();
    let (whole, frac) = if has_int { (a.trunc(), a.fract()) } else { (0.0, a) };
    let (num, den) = if let Some(d) = fixed { ((frac * d as f64).round() as u64, d) } else {
        let maxd = 10u64.pow(den_digits.max(1) as u32) - 1;
        let mut best = (0u64, 1u64, f64::MAX);
        for d in 1..=maxd { let nn = (frac * d as f64).round() as u64; let e = (frac - nn as f64 / d as f64).abs(); if e < best.2 - 1e-12 { best = (nn, d, e); } }
        (best.0, best.1)
    };
    let (whole, num) = if num == den && has_int { (whole + 1.0, 0) } else { (whole, num) };
    let sign = if neg { "-" } else { "" };
    let width = den_digits.max(1);
    if has_int {
        if num == 0 { return format!("{sign}{}{}", whole as u64, " ".repeat(2 * width + 2)); }
        if whole == 0.0 { return format!("{sign}{:>w$}/{:<w$}", num, den, w = width); }
        format!("{sign}{} {:>w$}/{:<w$}", whole as u64, num, den, w = width)
    } else { format!("{sign}{}/{}", num, den) }
}

fn format_date(n: f64, sec: &Section, date1904: bool) -> String {
    if !(0.0..2958466.0).contains(&n) { return "#".repeat(11); }
    let t = &sec.toks;
    let has_ampm = t.iter().any(|x| matches!(x, T::AmPm(_)));
    // Fractional seconds: "ss.0" — the dot and zeros after seconds.
    let frac_digits = t.iter().enumerate().find(|(i, x)| **x == T::Dot && *i > 0 && matches!(&t[i - 1], T::Date(d) if d.starts_with('s'))).map(|(i, _)| t[i + 1..].iter().take_while(|x| **x == T::Zero).count()).unwrap_or(0);
    let rounded = if frac_digits > 0 { n } else { (n * 86400.0).round() / 86400.0 };
    let (y, mo, d) = ymd(rounded, date1904);
    let (h, mi, s, ms) = hms(rounded);
    let wd = weekday(rounded, date1904) as usize;
    let mut out = String::new();
    let mut i = 0;
    while i < t.len() {
        match &t[i] {
            T::Date(tok) => {
                let c = tok.chars().next().unwrap();
                let len = tok.len();
                // m after h (or before s) is minutes.
                let minute = c == 'm' && (t[..i].iter().rev().find(|x| matches!(x, T::Date(_) | T::Elapsed(..))).is_some_and(|x| matches!(x, T::Date(p) if p.starts_with('h')) || matches!(x, T::Elapsed('h', _)))
                    || t[i + 1..].iter().find(|x| matches!(x, T::Date(_))).is_some_and(|x| matches!(x, T::Date(p) if p.starts_with('s'))));
                match c {
                    'y' | 'e' => out.push_str(&if len <= 2 && c == 'y' { format!("{:02}", y % 100) } else { y.to_string() }),
                    'b' => out.push_str(&(y + 543).to_string()),
                    'm' if minute => out.push_str(&if len >= 2 { format!("{:02}", mi) } else { mi.to_string() }),
                    'm' => out.push_str(&match len { 1 => mo.to_string(), 2 => format!("{:02}", mo), 3 => MONTHS[mo as usize - 1][..3].to_string(), 5 => MONTHS[mo as usize - 1][..1].to_string(), _ => MONTHS[mo as usize - 1].to_string() }),
                    'd' => out.push_str(&match len { 1 => d.to_string(), 2 => format!("{:02}", d), 3 => DAYS[wd][..3].to_string(), _ => DAYS[wd].to_string() }),
                    'h' => { let hh = if has_ampm { if h % 12 == 0 { 12 } else { h % 12 } } else { h }; out.push_str(&if len >= 2 { format!("{:02}", hh) } else { hh.to_string() }); }
                    's' => out.push_str(&if len >= 2 { format!("{:02}", s) } else { s.to_string() }),
                    'g' => {}
                    _ => out.push_str(tok),
                }
            }
            T::Elapsed(c, len) => {
                let total = (n * 86400.0).round();
                let v = match c { 'h' => (total / 3600.0).floor(), 'm' => (total / 60.0).floor(), _ => total };
                out.push_str(&format!("{:0w$}", v as i64, w = *len));
            }
            T::AmPm(p) => { let pm = h >= 12; out.push_str(if p.len() == 3 { if pm { if p.starts_with('A') { "P" } else { "p" } } else if p.starts_with('A') { "A" } else { "a" } } else if pm { "PM" } else { "AM" }); }
            T::Dot if frac_digits > 0 && i > 0 && matches!(&t[i - 1], T::Date(d) if d.starts_with('s')) => {
                let f = format!("{:03}", ms); out.push('.'); out.push_str(&f[..frac_digits.min(3)]); i += 1 + frac_digits; continue;
            }
            T::Lit(l) => out.push_str(l),
            T::Pad(_) => out.push(' '),
            T::Dot => out.push('.'), T::Comma => out.push(','), T::Slash => out.push('/'), T::Zero => out.push('0'), T::Pct => out.push('%'),
            _ => {}
        }
        i += 1;
    }
    out
}

// ---- reading typed input ----

/// A number in plain text, as a formula coerces it ("1,234.5", " 12 ", "1e3", "50%", "$3").
pub fn parse_number(s: &str) -> Option<f64> {
    parse_input(s, false).map(|x| x.0)
}

fn parse_plain(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let ok = s.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')) && s.chars().any(|c| c.is_ascii_digit());
    if !ok { return None; }
    s.parse::<f64>().ok().filter(|x| x.is_finite())
}

/// Reads typed text as a number and the format its spelling implies.
pub fn parse_input(text: &str, date1904: bool) -> Option<(f64, Option<String>)> {
    let s = text.trim();
    if s.is_empty() { return None; }
    if let Some(n) = parse_plain(s) { return Some((n, None)); }
    // (123) is negative.
    let (neg, s) = if s.starts_with('(') && s.ends_with(')') { (true, &s[1..s.len() - 1]) } else { (false, s) };
    let (neg, s) = if let Some(r) = s.strip_prefix('-') { (!neg, r.trim_start()) } else { (neg, s) };
    let sign = if neg { -1.0 } else { 1.0 };
    // Currency.
    for cur in ['$', '€', '£', '¥'] {
        if let Some(r) = s.strip_prefix(cur).or_else(|| s.strip_suffix(cur)) {
            let r = r.trim(); let r = r.strip_prefix('-').map(|x| (true, x)).unwrap_or((false, r));
            if let Some(v) = grouped(r.1) {
                let decs = if r.1.contains('.') { 2 } else { 0 };
                let fmt = if cur == '$' { format!("\"$\"#,##0{}", if decs > 0 { ".00" } else { "" }) } else { format!("#,##0{} \"{}\"", if decs > 0 { ".00" } else { "" }, cur) };
                let fmt = if s.starts_with(cur) && cur != '$' { format!("\"{cur}\"#,##0{}", if decs > 0 { ".00" } else { "" }) } else { fmt };
                return Some((sign * if r.0 { -v } else { v }, Some(fmt)));
            }
        }
    }
    if let Some(r) = s.strip_suffix('%') {
        let v = grouped(r.trim())?;
        return Some((sign * v / 100.0, Some(if r.contains('.') { "0.00%".into() } else { "0%".into() })));
    }
    if let Some(v) = grouped(s) { return Some((sign * v, Some(if s.contains('.') { "#,##0.00".into() } else { "#,##0".into() }))); }
    if neg { return None; }
    // Fractions: "1/2" alone would be a date; "0 1/2" and "1 1/4" are mixed numbers.
    if let Some((w, f)) = s.split_once(' ') {
        if let (Ok(w), Some((a, b))) = (w.parse::<f64>(), f.split_once('/')) {
            if let (Ok(a), Ok(b)) = (a.parse::<f64>(), b.parse::<f64>()) { if b != 0.0 && a < b && !w.is_nan() { return Some((w + a / b, Some("# ?/?".into()))); } }
        }
    }
    parse_date_time(s, date1904)
}

/// "1,234,567.8" with properly placed separators.
fn grouped(s: &str) -> Option<f64> {
    if let Some(n) = parse_plain(s) { return Some(n); }
    if !s.contains(',') { return None; }
    let (ip, dp) = s.split_once('.').unwrap_or((s, ""));
    let groups: Vec<&str> = ip.split(',').collect();
    if groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3) { return None; }
    if !groups.iter().all(|g| g.chars().all(|c| c.is_ascii_digit())) || !dp.chars().all(|c| c.is_ascii_digit()) { return None; }
    format!("{}.{}", groups.concat(), if dp.is_empty() { "0" } else { dp }).parse().ok()
}

fn month_from(s: &str) -> Option<u32> {
    let l = s.to_ascii_lowercase();
    if l.len() < 3 || !l.is_ascii() { return None; }
    MONTHS.iter().position(|m| m.to_ascii_lowercase().starts_with(&l) || (l.len() >= 3 && m.to_ascii_lowercase()[..3] == l[..3] && m.to_ascii_lowercase().starts_with(&l[..l.len().min(m.len())]))).map(|i| i as u32 + 1)
}

fn parse_time(s: &str) -> Option<(f64, bool, bool)> {
    let l = s.trim().to_ascii_lowercase();
    let (body, ampm) = if let Some(b) = l.strip_suffix("am").or(l.strip_suffix('a')) { (b.trim(), Some(false)) } else if let Some(b) = l.strip_suffix("pm").or(l.strip_suffix('p')) { (b.trim(), Some(true)) } else { (l.as_str(), None) };
    let parts: Vec<&str> = body.split(':').collect();
    if parts.len() < 2 && ampm.is_none() { return None; }
    if parts.len() > 3 { return None; }
    let mut h: f64 = parts[0].parse().ok()?;
    let m: f64 = if parts.len() > 1 { parts[1].parse().ok()? } else { 0.0 };
    let sec: f64 = if parts.len() > 2 { parts[2].parse().ok()? } else { 0.0 };
    if m >= 60.0 || sec >= 60.0 || h < 0.0 { return None; }
    if let Some(pm) = ampm { if h > 12.0 || h == 0.0 { return None; } if pm && h < 12.0 { h += 12.0; } if !pm && h == 12.0 { h = 0.0; } }
    Some(((h * 3600.0 + m * 60.0 + sec) / 86400.0, parts.len() > 2, ampm.is_some()))
}

fn parse_date(s: &str, date1904: bool) -> Option<(f64, String)> {
    let s = s.trim().trim_end_matches(',');
    let seps = |c: char| c == '/' || c == '-' || c == ' ' || c == '.' || c == ',';
    let parts: Vec<&str> = s.split(seps).filter(|p| !p.is_empty()).collect();
    let num = |p: &str| p.parse::<i64>().ok();
    let valid = |y: i64, m: i64, d: i64| (1..=12).contains(&m) && d >= 1 && d <= crate::date::days_in_month(y, m as u32) as i64 && (0..=9999).contains(&y);
    let year = |y: i64, len: usize| if len <= 2 { if y < 30 { 2000 + y } else { 1900 + y } } else { y };
    match parts.as_slice() {
        [a, b, c] => {
            // yyyy-mm-dd
            if a.len() == 4 { let (y, m, d) = (num(a)?, num(b).or_else(|| month_from(b).map(|x| x as i64))?, num(c)?); if valid(y, m, d) { return Some((serial(y, m, d, date1904), "yyyy-mm-dd".into())); } }
            // d-mmm-yyyy or mmm d yyyy
            if let Some(m) = month_from(b) { let (d, y) = (num(a)?, year(num(c)?, c.len())); if valid(y, m as i64, d) { return Some((serial(y, m as i64, d, date1904), "d-mmm-yy".into())); } }
            if let Some(m) = month_from(a) { let (d, y) = (num(b)?, year(num(c)?, c.len())); if valid(y, m as i64, d) { return Some((serial(y, m as i64, d, date1904), "d-mmm-yy".into())); } }
            // m/d/yyyy (and d/m/yyyy when the first part cannot be a month).
            let (x, y2, z) = (num(a)?, num(b)?, year(num(c)?, c.len()));
            if valid(z, x, y2) { return Some((serial(z, x, y2, date1904), "m/d/yyyy".into())); }
            if valid(z, y2, x) { return Some((serial(z, y2, x, date1904), "d/m/yyyy".into())); }
            None
        }
        [a, b] => {
            let this_year = ymd(now_serial(), false).0;
            if let Some(m) = month_from(b) { let d = num(a)?; if valid(this_year, m as i64, d) { return Some((serial(this_year, m as i64, d, date1904), "d-mmm".into())); } if d > 31 { let y = year(d, a.len()); return Some((serial(y, m as i64, 1, date1904), "mmm-yy".into())); } }
            if let Some(m) = month_from(a) { let d = num(b)?; if b.len() <= 2 && valid(this_year, m as i64, d) { return Some((serial(this_year, m as i64, d, date1904), "d-mmm".into())); } let y = year(d, b.len()); return Some((serial(y, m as i64, 1, date1904), "mmm-yy".into())); }
            if s.contains('/') || s.contains('-') {
                let (x, y2) = (num(a)?, num(b)?);
                if valid(this_year, x, y2) { return Some((serial(this_year, x, y2, date1904), "d-mmm".into())); }
            }
            None
        }
        _ => None,
    }
}

fn parse_date_time(s: &str, date1904: bool) -> Option<(f64, Option<String>)> {
    if let Some((t, secs, ampm)) = parse_time(s) {
        let f = match (secs, ampm) { (true, true) => "h:mm:ss AM/PM", (false, true) => "h:mm AM/PM", (true, false) => "h:mm:ss", _ => "h:mm" };
        return Some((t, Some(f.into())));
    }
    if let Some((d, f)) = parse_date(s, date1904) { return Some((d, Some(f))); }
    // Date then time: "2024-01-15 13:45", "1/15/2024 1:30 PM".
    let words: Vec<&str> = s.split_whitespace().collect();
    for k in 1..words.len() {
        let (dp, tp) = (words[..k].join(" "), words[k..].join(" "));
        if let (Some((d, _)), Some((t, _, _))) = (parse_date(&dp, date1904), parse_time(&tp)) {
            return Some((d + t, Some("m/d/yyyy h:mm".into())));
        }
    }
    None
}

/// How a value cell reads in the formula bar for editing.
pub fn edit_text(v: &Value, fmt: &str, date1904: bool) -> String {
    match v {
        Value::Num(n) => {
            if is_date_format(fmt) && *n >= 0.0 {
                let has_time = n.fract().abs() > 1e-9;
                let has_date = n.trunc() != 0.0;
                let mut s = String::new();
                if has_date || !has_time { s.push_str(&format_number(*n, "m/d/yyyy", date1904).text); }
                if has_time { if !s.is_empty() { s.push(' '); } s.push_str(&format_number(*n, if hms(*n).2 != 0 { "h:mm:ss AM/PM" } else { "h:mm:ss AM/PM" }, date1904).text); }
                s
            } else if is_percent_format(fmt) { format!("{}%", fmt_num(*n * 100.0)) } else { fmt_num(*n) }
        }
        Value::Str(s) => {
            // Text that would read back as something else keeps a leading apostrophe.
            if parse_input(s, date1904).is_some() || s.starts_with('=') || s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false") || crate::value::ErrKind::parse(s).is_some() { format!("'{s}") } else { s.to_string() }
        }
        v => v.display(),
    }
}

/// The current local date and time as a serial.
pub fn now_serial() -> f64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    secs / 86400.0 + 25569.0 + local_offset_hours() / 24.0
}

/// The local time zone's offset, from the TZ the system reports (best effort, no extra crates).
fn local_offset_hours() -> f64 {
    use std::sync::OnceLock;
    static OFF: OnceLock<f64> = OnceLock::new();
    *OFF.get_or_init(|| {
        #[cfg(unix)]
        {
            if let Ok(o) = std::process::Command::new("date").arg("+%z").output() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.len() == 5 { let sign = if s.starts_with('-') { -1.0 } else { 1.0 }; let h: f64 = s[1..3].parse().unwrap_or(0.0); let m: f64 = s[3..5].parse().unwrap_or(0.0); return sign * (h + m / 60.0); }
            }
        }
        #[cfg(windows)]
        {
            if let Ok(o) = std::process::Command::new("powershell").args(["-NoProfile", "-Command", "[TimeZoneInfo]::Local.GetUtcOffset([DateTime]::Now).TotalHours"]).output() {
                if let Ok(h) = String::from_utf8_lossy(&o.stdout).trim().replace(',', ".").parse::<f64>() { return h; }
            }
        }
        0.0
    })
}

/// Common formats offered in the Number group and the Format Cells dialog.
pub const CATEGORIES: &[(&str, &[&str])] = &[
    ("General", &["General"]),
    ("Number", &["0", "0.00", "#,##0", "#,##0.00", "#,##0.00;[Red]#,##0.00", "#,##0.00;(#,##0.00)", "#,##0.00;[Red](#,##0.00)"]),
    ("Currency", &["\"$\"#,##0.00", "\"$\"#,##0.00;[Red]\"$\"#,##0.00", "\"$\"#,##0", "#,##0.00 \"€\"", "\"£\"#,##0.00"]),
    ("Accounting", &[r#"_("$"* #,##0.00_);_("$"* \(#,##0.00\);_("$"* "-"??_);_(@_)"#, r#"_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)"#]),
    ("Date", &["m/d/yyyy", "dddd, mmmm d, yyyy", "d-mmm-yy", "d-mmm", "mmm-yy", "mmmm d, yyyy", "yyyy-mm-dd", "dd/mm/yyyy", "m/d/yy h:mm"]),
    ("Time", &["h:mm", "h:mm AM/PM", "h:mm:ss", "h:mm:ss AM/PM", "[h]:mm:ss", "mm:ss.0"]),
    ("Percentage", &["0%", "0.00%"]),
    ("Fraction", &["# ?/?", "# ??/??", "# ???/???", "# ?/2", "# ?/4", "# ?/8", "# ??/16"]),
    ("Scientific", &["0.00E+00", "##0.0E+0"]),
    ("Text", &["@"]),
    ("Special", &["00000", "00000-0000", "(###) ###-####", "000-00-0000"]),
];

#[cfg(test)]
mod tests {
    use super::*;
    fn f(n: f64, c: &str) -> String { format_number(n, c, false).text }
    #[test]
    fn numbers() {
        assert_eq!(f(1234.567, "#,##0.00"), "1,234.57");
        assert_eq!(f(0.5, "0%"), "50%");
        assert_eq!(f(-5.0, "0.00;[Red](0.00)"), "(5.00)");
        assert_eq!(format_number(-5.0, "0.00;[Red](0.00)", false).color, Some([255, 0, 0]));
        assert_eq!(f(12345.0, "0.00E+00"), "1.23E+04");
        assert_eq!(f(0.0, "#,##0"), "0");
        assert_eq!(f(1.5, "# ?/?"), "1 1/2");
        assert_eq!(f(1234567.0, "#,##0,"), "1,235");
        assert_eq!(f(5.0, "\"$\"#,##0.00"), "$5.00");
        assert_eq!(f(3.1, "0.0#"), "3.1");
        assert_eq!(f(45306.5, "yyyy-mm-dd h:mm AM/PM"), "2024-01-15 12:00 PM");
        assert_eq!(f(45306.0, "dddd, mmmm d, yyyy"), "Monday, January 15, 2024");
        assert_eq!(f(1.5, "[h]:mm"), "36:00");
        assert_eq!(f(0.0, r#"_("$"* #,##0.00_);_("$"* \(#,##0.00\);_("$"* "-"??_);_(@_)"#).trim(), "$ -");
        assert_eq!(f(123.0, "00000"), "00123");
        assert_eq!(general(1234567890123.0, 11), "1.23457E+12");
    }
    #[test]
    fn input() {
        assert_eq!(parse_input("1,234.5", false).unwrap().0, 1234.5);
        assert_eq!(parse_input("12%", false).unwrap(), (0.12, Some("0%".into())));
        assert_eq!(parse_input("$1,000", false).unwrap().0, 1000.0);
        assert_eq!(parse_input("(5)", false).unwrap().0, -5.0);
        assert_eq!(parse_input("2024-01-15", false).unwrap().0, 45306.0);
        assert_eq!(parse_input("1/15/2024", false).unwrap().0, 45306.0);
        assert_eq!(parse_input("15-Jan-2024", false).unwrap().0, 45306.0);
        assert!((parse_input("13:30", false).unwrap().0 - 0.5625).abs() < 1e-12);
        assert!((parse_input("1:30 PM", false).unwrap().0 - 0.5625).abs() < 1e-12);
        assert!(parse_input("abc", false).is_none());
        assert!(parse_input("1,23", false).is_none());
    }
}
