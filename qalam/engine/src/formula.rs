//! Table formulas (table-formula.js): `=SUM(ABOVE)`, `=A1*B2`, `=AVERAGE(B2:B9)` evaluated over
//! table cells, with number pictures such as `#,##0.00` (§17.16.4.2).

use crate::model::{Document, VMerge};

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Num(f64),
    Neg(Box<Node>),
    Percent(Box<Node>),
    Range((usize, usize), (usize, usize)),
    Direction(Dir),
    Binary(String, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir { Above, Below, Left, Right }

pub const FUNCTIONS: &[&str] = &["SUM", "PRODUCT", "AVERAGE", "COUNT", "MAX", "MIN", "ABS", "INT", "SIGN", "NOT", "MOD", "ROUND", "AND", "OR", "IF", "TRUE", "FALSE"];
pub const NUMBER_FORMATS: &[&str] = &["0", "0.00", "#,##0", "#,##0.00", "$#,##0.00;($#,##0.00)", "0%", "0.00%"];

/// The number leading a cell's text: currency, grouping and accounting negatives allowed.
pub fn cell_number(text: &str) -> Option<f64> {
    let t = text.trim_start();
    let (neg_sign, rest) = match t.chars().next() { Some('-' | '−') => (true, &t[t.chars().next().unwrap().len_utf8()..]), Some('+') => (false, &t[1..]), _ => (false, t) };
    let rest = rest.trim_start();
    let paren = rest.starts_with('(');
    let rest = rest.trim_start_matches('(').trim_start().trim_start_matches(['$', '€', '£', '¥']).trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
    if !digits.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.') { return None; }
    let v: f64 = digits.replace(',', "").parse().ok()?;
    Some(if neg_sign || paren && text.contains(')') { -v } else { v })
}

#[derive(Clone, Debug, PartialEq)]
enum Tok { Num(f64), Ref(String), Name(String), Op(String) }

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() { i += 1; continue; }
        if ch.is_ascii_digit() || ch == '.' {
            let s = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') { i += 1; }
            out.push(Tok::Num(c[s..i].iter().collect::<String>().parse().map_err(|_| "Bad number in the formula.")?));
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let s = i;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_') { i += 1; }
            let word: String = c[s..i].iter().collect::<String>().to_uppercase();
            // A1-style or R1C1 references, optionally a range.
            let is_ref = |w: &str| { let l = w.trim_end_matches(|x: char| x.is_ascii_digit()); (!l.is_empty() && l.len() < w.len() && l.chars().all(|x| x.is_ascii_uppercase())) || (w.starts_with('R') && w.contains('C') && w[1..].split('C').all(|n| !n.is_empty() && n.chars().all(|d| d.is_ascii_digit()))) };
            if is_ref(&word) {
                let mut r = word.clone();
                if i < c.len() && c[i] == ':' {
                    let s2 = i + 1;
                    let mut j = s2;
                    while j < c.len() && c[j].is_ascii_alphanumeric() { j += 1; }
                    let w2: String = c[s2..j].iter().collect::<String>().to_uppercase();
                    if is_ref(&w2) { r = format!("{word}:{w2}"); i = j; }
                }
                out.push(Tok::Ref(r));
            } else { out.push(Tok::Name(word)); }
            continue;
        }
        let two: String = c[i..(i + 2).min(c.len())].iter().collect();
        if ["<=", ">=", "<>"].contains(&two.as_str()) { out.push(Tok::Op(two)); i += 2; continue; }
        if "+-*/^%(),;=<>".contains(ch) { out.push(Tok::Op(if ch == ';' { ",".into() } else { ch.to_string() })); i += 1; continue; }
        return Err(format!("Unexpected \"{ch}\" in the formula."));
    }
    Ok(out)
}

/// A1 → (row 0, column 0); R2C3 → (1, 2).
fn cell_ref(text: &str) -> Result<(usize, usize), String> {
    if let Some(rc) = text.strip_prefix('R').and_then(|x| x.split_once('C')) {
        if let (Ok(r), Ok(c)) = (rc.0.parse::<usize>(), rc.1.parse::<usize>()) { return Ok((r.max(1) - 1, c.max(1) - 1)); }
    }
    let letters: String = text.chars().take_while(|c| c.is_ascii_uppercase()).collect();
    let row: usize = text[letters.len()..].parse().map_err(|_| format!("{text} is not a cell reference."))?;
    let col = letters.chars().fold(0usize, |n, ch| n * 26 + (ch as usize - 64)) - 1;
    Ok((row.max(1) - 1, col))
}

struct Parser { t: Vec<Tok>, i: usize }

impl Parser {
    fn peek_op(&self, ops: &[&str]) -> Option<String> { match self.t.get(self.i) { Some(Tok::Op(o)) if ops.contains(&o.as_str()) => Some(o.clone()), _ => None } }
    fn expect(&mut self, op: &str) -> Result<(), String> { if self.peek_op(&[op]).is_some() { self.i += 1; Ok(()) } else { Err(format!("Expected \"{op}\" in the formula.")) } }
    fn binary(&mut self, level: usize) -> Result<Node, String> {
        const LEVELS: [&[&str]; 4] = [&["=", "<", ">", "<=", ">=", "<>"], &["+", "-"], &["*", "/"], &["^"]];
        if level == LEVELS.len() { return self.percent(); }
        let mut left = self.binary(level + 1)?;
        while let Some(op) = self.peek_op(LEVELS[level]) { self.i += 1; let right = self.binary(level + 1)?; left = Node::Binary(op, Box::new(left), Box::new(right)); }
        Ok(left)
    }
    fn percent(&mut self) -> Result<Node, String> {
        let mut v = self.primary()?;
        while self.peek_op(&["%"]).is_some() { self.i += 1; v = Node::Percent(Box::new(v)); }
        Ok(v)
    }
    fn primary(&mut self) -> Result<Node, String> {
        let t = self.t.get(self.i).cloned().ok_or("The formula ends too soon.")?;
        self.i += 1;
        match t {
            Tok::Num(n) => Ok(Node::Num(n)),
            Tok::Op(o) if o == "(" => { let v = self.binary(0)?; self.expect(")")?; Ok(v) }
            Tok::Op(o) if o == "-" => Ok(Node::Neg(Box::new(self.binary(3)?))),
            Tok::Op(o) if o == "+" => self.binary(3),
            Tok::Ref(r) => { let (a, b) = r.split_once(':').unwrap_or((&r, &r)); Ok(Node::Range(cell_ref(a)?, cell_ref(b)?)) }
            Tok::Name(n) => {
                match n.as_str() {
                    "ABOVE" => return Ok(Node::Direction(Dir::Above)), "BELOW" => return Ok(Node::Direction(Dir::Below)),
                    "LEFT" => return Ok(Node::Direction(Dir::Left)), "RIGHT" => return Ok(Node::Direction(Dir::Right)),
                    "TRUE" | "FALSE" => { if self.peek_op(&["("]).is_some() { self.i += 1; self.expect(")")?; } return Ok(Node::Num(if n == "TRUE" { 1.0 } else { 0.0 })); }
                    _ => {}
                }
                if !FUNCTIONS.contains(&n.as_str()) { return Err(format!("{n} is not a formula function.")); }
                self.expect("(")?;
                let mut args = vec![];
                if self.peek_op(&[")"]).is_none() { loop { args.push(self.binary(0)?); if self.peek_op(&[","]).is_some() { self.i += 1; } else { break; } } }
                self.expect(")")?;
                Ok(Node::Call(n, args))
            }
            Tok::Op(o) => Err(format!("Unexpected \"{o}\" in the formula.")),
        }
    }
}

fn parse(src: &str) -> Result<Node, String> {
    let t = tokenize(src)?;
    if t.is_empty() { return Err("Type a formula after the equals sign.".into()); }
    let mut p = Parser { t, i: 0 };
    let n = p.binary(0)?;
    if p.i < p.t.len() { return Err("Unexpected text at the end of the formula.".into()); }
    Ok(n)
}

/// Evaluation context: the formula cell's table and grid position.
struct Ctx<'a> { doc: &'a Document, table: usize, row: usize, col: usize, end: usize }

impl Ctx<'_> {
    fn cell_text(&self, row: usize, col: usize) -> Option<(u64, String)> {
        let t = &self.doc.tables[self.table];
        let mut r = row;
        // Continuation cells read the merge's first cell.
        while r > 0 && t.cell_at_grid(r, col).is_some_and(|(_, c)| c.v_merge == VMerge::Continue) { r -= 1; }
        let (_, c) = t.cell_at_grid(r, col)?;
        Some((c.id, self.doc.paragraphs.iter().filter(|p| p.cell == Some(c.id)).map(|p| p.text()).collect::<Vec<_>>().join(" ").replace(crate::chars::OBJECT, "")))
    }
    fn range(&self, a: (usize, usize), b: (usize, usize)) -> Vec<f64> {
        let mut seen = vec![];
        let mut out = vec![];
        for r in a.0.min(b.0)..=a.0.max(b.0) { for c in a.1.min(b.1)..=a.1.max(b.1) {
            if let Some((id, text)) = self.cell_text(r, c) { if !seen.contains(&id) { seen.push(id); if let Some(v) = cell_number(&text) { out.push(v); } } }
        } }
        out
    }
    /// The run of numeric cells next to the formula cell; empty cells before the run are skipped,
    /// and the first text cell after it (a header) ends it.
    fn direction(&self, d: Dir) -> Vec<f64> {
        let t = &self.doc.tables[self.table];
        let (dr, dc): (i64, i64) = match d { Dir::Above => (-1, 0), Dir::Below => (1, 0), Dir::Left => (0, -1), Dir::Right => (0, 1) };
        let (mut r, mut c) = (self.row as i64 + dr, if dc < 0 { self.col as i64 - 1 } else if dc > 0 { self.end as i64 + 1 } else { self.col as i64 });
        let cols = t.grid.len() as i64;
        let (mut out, mut seen) = (vec![], vec![]);
        while r >= 0 && (r as usize) < t.rows.len() && c >= 0 && c < cols {
            if let Some((id, text)) = self.cell_text(r as usize, c as usize) {
                if !seen.contains(&id) {
                    seen.push(id);
                    match cell_number(&text) {
                        Some(v) => out.push(v),
                        None if !out.is_empty() => break,
                        None if !text.trim().is_empty() => break,
                        None => {}
                    }
                }
            }
            r += dr; c += dc;
        }
        out
    }
    fn values(&self, n: &Node) -> Result<Vec<f64>, String> {
        Ok(match n { Node::Range(a, b) => self.range(*a, *b), Node::Direction(d) => self.direction(*d), other => vec![self.eval(other)?] })
    }
    fn eval(&self, n: &Node) -> Result<f64, String> {
        Ok(match n {
            Node::Num(v) => *v,
            Node::Neg(x) => -self.eval(x)?,
            Node::Percent(x) => self.eval(x)? / 100.0,
            Node::Range(..) | Node::Direction(_) => self.values(n)?.iter().sum(),
            Node::Binary(op, a, b) => {
                let (a, b) = (self.eval(a)?, self.eval(b)?);
                let t = |x: bool| if x { 1.0 } else { 0.0 };
                match op.as_str() {
                    "+" => a + b, "-" => a - b, "*" => a * b, "^" => a.powf(b),
                    "/" => { if b == 0.0 { return Err("!Zero Divide".into()); } a / b }
                    "=" => t(a == b), "<>" => t(a != b), "<" => t(a < b), ">" => t(a > b), "<=" => t(a <= b), ">=" => t(a >= b),
                    _ => return Err("Unknown operator.".into()),
                }
            }
            Node::Call(name, args) => {
                let all = || -> Result<Vec<f64>, String> { let mut v = vec![]; for a in args { v.extend(self.values(a)?); } Ok(v) };
                let need = |k: usize| if args.len() == k { Ok(()) } else { Err(format!("{name} needs {k} argument{}.", if k == 1 { "" } else { "s" })) };
                let arg = |i: usize| self.eval(&args[i]);
                match name.as_str() {
                    "SUM" => all()?.iter().sum(),
                    "PRODUCT" => all()?.iter().product(),
                    "AVERAGE" => { let v = all()?; if v.is_empty() { return Err("AVERAGE has no numbers to average.".into()); } v.iter().sum::<f64>() / v.len() as f64 }
                    "COUNT" => all()?.len() as f64,
                    "MAX" => { let v = all()?; if v.is_empty() { 0.0 } else { v.into_iter().fold(f64::MIN, f64::max) } }
                    "MIN" => { let v = all()?; if v.is_empty() { 0.0 } else { v.into_iter().fold(f64::MAX, f64::min) } }
                    "ABS" => { need(1)?; arg(0)?.abs() }
                    "INT" => { need(1)?; arg(0)?.trunc() }
                    "SIGN" => { need(1)?; let v = arg(0)?; if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 } }
                    "NOT" => { need(1)?; if arg(0)? == 0.0 { 1.0 } else { 0.0 } }
                    "MOD" => { need(2)?; let b = arg(1)?; if b == 0.0 { return Err("!Zero Divide".into()); } arg(0)? % b }
                    "ROUND" => { need(2)?; let f = 10f64.powi(arg(1)?.trunc() as i32); (arg(0)? * f).round() / f }
                    "AND" => { need(2)?; if arg(0)? != 0.0 && arg(1)? != 0.0 { 1.0 } else { 0.0 } }
                    "OR" => { need(2)?; if arg(0)? != 0.0 || arg(1)? != 0.0 { 1.0 } else { 0.0 } }
                    "IF" => { need(3)?; if arg(0)? != 0.0 { arg(1)? } else { arg(2)? } }
                    "TRUE" => 1.0, "FALSE" => 0.0,
                    _ => return Err(format!("{name} is not a formula function.")),
                }
            }
        })
    }
}

/// Format a number with a numeric picture: 0 # , . and literals; sections positive;negative;zero.
pub fn format_number(value: f64, picture: &str) -> String {
    if picture.is_empty() {
        let r = (value * 100.0).round() / 100.0;
        let r = if r == 0.0 { 0.0 } else { r };
        return if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") };
    }
    let sections: Vec<&str> = picture.split(';').collect();
    let neg_section = value < 0.0 && sections.len() > 1;
    let section = if value == 0.0 && sections.len() > 2 { sections[2] } else if neg_section { sections[1] } else { sections[0] };
    let chars: Vec<char> = section.chars().collect();
    let is_tok = |c: char| "0#,.".contains(c);
    let (Some(first), Some(last)) = (chars.iter().position(|c| is_tok(*c) && *c != ','), chars.iter().rposition(|c| is_tok(*c) && *c != ',')) else { return section.to_string() };
    let body: Vec<char> = chars[first..=last].to_vec();
    let dot = body.iter().position(|c| *c == '.');
    let (int_part, frac_part): (&[char], &[char]) = match dot { Some(d) => (&body[..d], &body[d + 1..]), None => (&body[..], &[]) };
    let decimals = frac_part.iter().filter(|c| **c == '0' || **c == '#').count();
    let grouping = int_part.contains(&',');
    let minimum = int_part.iter().filter(|c| **c == '0').count();
    let fixed = format!("{:.*}", decimals, value.abs());
    let (mut whole, mut part) = match fixed.split_once('.') { Some((w, p)) => (w.to_string(), p.to_string()), None => (fixed.clone(), String::new()) };
    if whole == "0" && minimum == 0 { whole.clear(); }
    while whole.len() < minimum { whole.insert(0, '0'); }
    if grouping {
        let digits: Vec<char> = whole.chars().collect();
        whole = digits.iter().enumerate().map(|(i, c)| if i > 0 && (digits.len() - i) % 3 == 0 { format!(",{c}") } else { c.to_string() }).collect();
    }
    let opt: Vec<char> = frac_part.iter().filter(|c| **c == '0' || **c == '#').copied().collect();
    for (i, c) in opt.iter().enumerate().rev() { if *c == '#' && part.ends_with('0') && part.len() > i { part.pop(); } else { break; } }
    let number = if part.is_empty() { whole } else { format!("{whole}.{part}") };
    let sign = if value < 0.0 && !neg_section && value.abs() >= 0.5 * 10f64.powi(-(decimals as i32)) { "-" } else { "" };
    let lit = |s: &[char]| s.iter().filter(|c| **c != '\'').collect::<String>();
    format!("{sign}{}{number}{}", lit(&chars[..first]), lit(&chars[last + 1..]))
}

impl Document {
    /// Evaluate a formula (with or without its leading "=") in the current cell.
    pub fn evaluate_formula(&self, formula: &str, picture: &str) -> Result<String, String> {
        let src = formula.trim().trim_start_matches('=');
        let tree = parse(src)?;
        let id = self.current_cell().ok_or("Place the caret in a table cell.")?;
        let table = self.table_of_cell(id).ok_or("Place the caret in a table cell.")?;
        let t = &self.tables[table];
        let (r, k) = t.find_cell(id).unwrap();
        let col = t.grid_col(r, k);
        let ctx = Ctx { doc: self, table, row: r, col, end: col + t.rows[r].cells[k].span.max(1) as usize - 1 };
        let v = ctx.eval(&tree)?;
        if !v.is_finite() { return Err("The result is not a number.".into()); }
        Ok(format_number(v, picture))
    }
    /// The formula Word suggests for the current cell.
    pub fn suggested_formula(&self) -> String {
        let probe = |f: &str| self.evaluate_formula(f, "").ok().is_some_and(|v| v != "0");
        if probe("=COUNT(ABOVE)") { "=SUM(ABOVE)".into() } else if probe("=COUNT(LEFT)") { "=SUM(LEFT)".into() } else { "=".into() }
    }
    /// Insert a formula's result at the caret.
    pub fn insert_formula(&mut self, formula: &str, picture: &str) -> Result<(), String> {
        let v = self.evaluate_formula(formula, picture)?;
        self.replace(&v);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, Paragraph};

    #[test]
    fn numbers_and_pictures() {
        assert_eq!(cell_number("$1,250.50 total"), Some(1250.5));
        assert_eq!(cell_number("(300)"), Some(-300.0));
        assert_eq!(cell_number("Q1"), None);
        assert_eq!(format_number(1234.5, "#,##0.00"), "1,234.50");
        assert_eq!(format_number(-12.0, "$#,##0.00;($#,##0.00)"), "($12.00)");
        assert_eq!(format_number(0.256, "0%"), "0%");
        assert_eq!(format_number(7.0, ""), "7");
    }

    #[test]
    fn sums_directions_refs_and_errors() {
        let mut d = Document::new(vec![Paragraph::new("x")]);
        d.insert_table(4, 3);
        for (i, v) in ["Item", "Q1", "Q2", "A", "10", "$1,000", "B", "20", "5", "Total", "", ""].iter().enumerate() {
            d.replace(v);
            if i < 11 { d.move_cell(true); }
        }
        // Caret in row 3, column 1 (the empty "Total" row's Q1 cell).
        let cell = d.tables[0].rows[3].cells[1].id;
        let p = d.paragraph_start(d.paragraphs.iter().position(|p| p.cell == Some(cell)).unwrap());
        d.select(p, p);
        assert_eq!(d.suggested_formula(), "=SUM(ABOVE)");
        assert_eq!(d.evaluate_formula("=SUM(ABOVE)", "").unwrap(), "30");
        assert_eq!(d.evaluate_formula("=AVERAGE(B2:C3)", "0.00").unwrap(), "258.75");
        assert_eq!(d.evaluate_formula("=C2/B2*2+MAX(B2,C3)^0", "#,##0").unwrap(), "201");
        assert_eq!(d.evaluate_formula("=IF(B3>B2, 1, 0)", "").unwrap(), "1");
        assert_eq!(d.evaluate_formula("=ROUND(10/3, 2)", "").unwrap(), "3.33");
        assert!(d.evaluate_formula("=B2/0", "").unwrap_err().contains("Zero"));
        assert!(d.evaluate_formula("=FOO(1)", "").is_err());
        d.insert_formula("=SUM(ABOVE)", "#,##0.00").unwrap();
        assert_eq!(d.paragraphs.iter().find(|p| p.cell == Some(cell)).unwrap().text(), "30.00");
    }
}
