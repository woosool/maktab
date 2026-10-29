//! The Power Query formula language (M), the part of it queries are written in: a tokenizer, an
//! expression tree, a parser and a printer. It covers what Excel's editor writes for the steps
//! Jadwal models (let expressions, function calls, lists, records, `each`, field and item access,
//! `type` values, if/then/else and the operators) and section documents (Section1.m).

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Null,
    Bool(bool),
    Num(f64),
    Text(String),
    /// A name: a regular identifier (dotted names like Table.Sort are one identifier), a quoted
    /// identifier (#"Changed Type"), or a hash keyword (#date, #table).
    Ident(String),
    /// `[Column]` inside `each`: a field of the current row `_`.
    Field(String),
    /// `x[Column]`.
    FieldOf(Box<Expr>, String),
    /// `x{i}` or `x{[Name="…"]}`.
    Item(Box<Expr>, Box<Expr>),
    List(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    Call(Box<Expr>, Vec<Expr>),
    Each(Box<Expr>),
    Lambda(Vec<String>, Box<Expr>),
    Unary(UOp, Box<Expr>),
    Binary(BOp, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Let(Vec<(String, Expr)>, Box<Expr>),
    /// A type value as written after `type`: "text", "nullable number", "table […]".
    Type(String),
    /// Parentheses as written (kept so formulas read back as they were).
    Paren(Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UOp { Neg, Not }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BOp { Or, And, Eq, Ne, Lt, Le, Gt, Ge, Add, Sub, Concat, Mul, Div }

impl BOp {
    fn prec(self) -> u8 {
        match self { BOp::Or => 1, BOp::And => 2, BOp::Eq | BOp::Ne => 3, BOp::Lt | BOp::Le | BOp::Gt | BOp::Ge => 4, BOp::Add | BOp::Sub | BOp::Concat => 5, BOp::Mul | BOp::Div => 6 }
    }
    fn text(self) -> &'static str {
        match self { BOp::Or => "or", BOp::And => "and", BOp::Eq => "=", BOp::Ne => "<>", BOp::Lt => "<", BOp::Le => "<=", BOp::Gt => ">", BOp::Ge => ">=", BOp::Add => "+", BOp::Sub => "-", BOp::Concat => "&", BOp::Mul => "*", BOp::Div => "/" }
    }
}

// ---- tokens ---------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    /// `[name]` with nothing but a (generalized) name inside: a field access.
    FieldName(String),
    P(&'static str),
}

const KEYWORDS: &[&str] = &["and", "as", "each", "else", "error", "false", "if", "in", "is", "let", "meta", "not", "null", "or", "otherwise", "section", "shared", "then", "true", "try", "type"];
const PUNCT: &[&str] = &["=>", "<=", ">=", "<>", "..", "...", "(", ")", "{", "}", "[", "]", ",", ";", "=", "<", ">", "+", "-", "*", "/", "&", "?", "!", "@"];

fn ident_start(c: char) -> bool { c.is_alphabetic() || c == '_' }
fn ident_part(c: char) -> bool { c.is_alphanumeric() || c == '_' }

/// Reads a string literal starting at the opening quote; returns (text, end index).
fn read_string(s: &[char], mut i: usize) -> Result<(String, usize), String> {
    i += 1;
    let mut out = String::new();
    loop {
        match s.get(i) {
            None => return Err("A text value is missing its closing quotation mark.".into()),
            Some('"') if s.get(i + 1) == Some(&'"') => { out.push('"'); i += 2; }
            Some('"') => return Ok((out, i + 1)),
            Some('#') if s.get(i + 1) == Some(&'(') => {
                // Escapes: #(cr), #(lf), #(tab), #(#), #(0041).
                let close = (i..s.len()).find(|&k| s[k] == ')').ok_or("A text escape is not closed.")?;
                let body: String = s[i + 2..close].iter().collect();
                for part in body.split(',') {
                    match part.trim() {
                        "cr" => out.push('\r'), "lf" => out.push('\n'), "tab" => out.push('\t'), "#" => out.push('#'),
                        h => out.push(u32::from_str_radix(h, 16).ok().and_then(char::from_u32).ok_or("Unknown text escape.")?),
                    }
                }
                i = close + 1;
            }
            Some(&c) => { out.push(c); i += 1; }
        }
    }
}

/// The tokens of `src` with the char offset each starts at.
pub fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, String> {
    let s: Vec<char> = src.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c.is_whitespace() || c == '\u{feff}' { i += 1; continue; }
        if c == '/' && s.get(i + 1) == Some(&'/') { while i < s.len() && s[i] != '\n' { i += 1; } continue; }
        if c == '/' && s.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < s.len() && !(s[i] == '*' && s[i + 1] == '/') { i += 1; }
            i += 2; continue;
        }
        let start = i;
        if c == '"' { let (t, e) = read_string(&s, i)?; out.push((Tok::Str(t), start)); i = e; continue; }
        if c == '#' && s.get(i + 1) == Some(&'"') { let (t, e) = read_string(&s, i + 1)?; out.push((Tok::Ident(t), start)); i = e; continue; }
        if c == '#' && s.get(i + 1).is_some_and(|c| c.is_alphabetic()) {
            let mut j = i + 1;
            while j < s.len() && ident_part(s[j]) { j += 1; }
            out.push((Tok::Ident(s[i..j].iter().collect()), start)); i = j; continue;
        }
        if c.is_ascii_digit() || (c == '.' && s.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let mut j = i;
            if c == '0' && matches!(s.get(i + 1), Some('x') | Some('X')) {
                j += 2;
                while j < s.len() && s[j].is_ascii_hexdigit() { j += 1; }
                let t: String = s[i + 2..j].iter().collect();
                out.push((Tok::Num(i64::from_str_radix(&t, 16).map_err(|e| e.to_string())? as f64), start)); i = j; continue;
            }
            while j < s.len() && (s[j].is_ascii_digit() || s[j] == '.' && s.get(j + 1).is_some_and(|d| d.is_ascii_digit())) { j += 1; }
            if j < s.len() && (s[j] == 'e' || s[j] == 'E') {
                let mut k = j + 1;
                if k < s.len() && (s[k] == '+' || s[k] == '-') { k += 1; }
                if k < s.len() && s[k].is_ascii_digit() { j = k; while j < s.len() && s[j].is_ascii_digit() { j += 1; } }
            }
            let t: String = s[i..j].iter().collect();
            out.push((Tok::Num(t.parse().map_err(|_| format!("'{t}' is not a number."))?), start)); i = j; continue;
        }
        if ident_start(c) {
            let mut j = i;
            loop {
                while j < s.len() && ident_part(s[j]) { j += 1; }
                // A dot followed by a letter continues the identifier (Table.PromoteHeaders, Int64.Type).
                if j + 1 < s.len() && s[j] == '.' && ident_start(s[j + 1]) { j += 1; continue; }
                break;
            }
            out.push((Tok::Ident(s[i..j].iter().collect()), start)); i = j; continue;
        }
        if c == '[' {
            // A field access holds just a name (a generalized identifier, or a quoted one).
            let mut j = i + 1;
            while j < s.len() && s[j] == ' ' { j += 1; }
            if s.get(j) == Some(&'#') && s.get(j + 1) == Some(&'"') {
                if let Ok((name, mut e)) = read_string(&s, j + 1) {
                    while e < s.len() && s[e] == ' ' { e += 1; }
                    if s.get(e) == Some(&']') { out.push((Tok::FieldName(name), start)); i = e + 1; continue; }
                }
            }
            if let Some(close) = (i + 1..s.len()).find(|&k| matches!(s[k], ']' | '[' | '=' | ',' | '(' | '{' | '\n')) {
                if s[close] == ']' {
                    let inner: String = s[i + 1..close].iter().collect();
                    let t = inner.trim();
                    if t.starts_with("#\"") && t.ends_with('"') && t.len() >= 3 {
                        let chars: Vec<char> = t.chars().collect();
                        if let Ok((name, e)) = read_string(&chars, 1) { if e == chars.len() { out.push((Tok::FieldName(name), start)); i = close + 1; continue; } }
                    } else if !t.is_empty() && !t.contains('"') && t.chars().all(|c| c.is_alphanumeric() || " _.-/%#$&'".contains(c)) {
                        out.push((Tok::FieldName(t.to_string()), start)); i = close + 1; continue;
                    }
                }
            }
        }
        let p = PUNCT.iter().filter(|p| { let pc: Vec<char> = p.chars().collect(); s[i..].starts_with(&pc) }).max_by_key(|p| p.len());
        match p {
            Some(p) => { out.push((Tok::P(p), start)); i += p.chars().count(); }
            None => return Err(format!("Unexpected character '{c}'.")),
        }
    }
    Ok(out)
}

// ---- parser ---------------------------------------------------------------------------------

pub struct Parser { toks: Vec<(Tok, usize)>, pos: usize, end: usize }

fn is_kw(t: &Tok, k: &str) -> bool { matches!(t, Tok::Ident(x) if x == k) }

impl Parser {
    pub fn new(src: &str) -> Result<Parser, String> { let toks = tokenize(src)?; let end = src.chars().count(); Ok(Parser { toks, pos: 0, end }) }
    pub fn peek(&self) -> Option<&Tok> { self.toks.get(self.pos).map(|t| &t.0) }
    pub fn offset(&self) -> usize { self.toks.get(self.pos).map_or(self.end, |t| t.1) }
    pub fn done(&self) -> bool { self.pos >= self.toks.len() }
    fn next(&mut self) -> Option<Tok> { let t = self.toks.get(self.pos).map(|t| t.0.clone()); self.pos += 1; t }
    fn eat_p(&mut self, p: &str) -> bool { if matches!(self.peek(), Some(Tok::P(x)) if *x == p) { self.pos += 1; true } else { false } }
    fn eat_kw(&mut self, k: &str) -> bool { if self.peek().is_some_and(|t| is_kw(t, k)) { self.pos += 1; true } else { false } }
    pub fn expect_p(&mut self, p: &str) -> Result<(), String> { if self.eat_p(p) { Ok(()) } else { Err(format!("Expected '{p}'{}.", self.near())) } }
    fn expect_kw(&mut self, k: &str) -> Result<(), String> { if self.eat_kw(k) { Ok(()) } else { Err(format!("Expected '{k}'{}.", self.near())) } }
    fn near(&self) -> String { match self.peek() { Some(Tok::Ident(t)) => format!(" near '{t}'"), Some(Tok::P(p)) => format!(" near '{p}'"), Some(Tok::Str(s)) => format!(" near \"{s}\""), Some(Tok::Num(n)) => format!(" near {n}"), Some(Tok::FieldName(f)) => format!(" near [{f}]"), None => " at the end".into() } }

    /// A name (regular or quoted identifier).
    pub fn name(&mut self) -> Result<String, String> {
        match self.next() { Some(Tok::Ident(n)) if !KEYWORDS.contains(&n.as_str()) || n == "type" => Ok(n), _ => { self.pos -= 1; Err(format!("Expected a name{}.", self.near())) } }
    }
    /// A section member name, which may be quoted.
    pub fn expect_kw_pub(&mut self, k: &str) -> Result<(), String> { self.expect_kw(k) }
    pub fn eat_kw_pub(&mut self, k: &str) -> bool { self.eat_kw(k) }
    pub fn eat_p_pub(&mut self, p: &str) -> bool { self.eat_p(p) }

    pub fn expr(&mut self) -> Result<Expr, String> {
        if self.eat_kw("each") { return Ok(Expr::Each(Box::new(self.expr()?))); }
        if self.eat_kw("let") {
            let mut binds = vec![];
            loop {
                let n = self.name()?;
                self.expect_p("=")?;
                binds.push((n, self.expr()?));
                if !self.eat_p(",") { break; }
            }
            self.expect_kw("in")?;
            return Ok(Expr::Let(binds, Box::new(self.expr()?)));
        }
        if self.eat_kw("if") {
            let c = self.expr()?; self.expect_kw("then")?;
            let a = self.expr()?; self.expect_kw("else")?;
            let b = self.expr()?;
            return Ok(Expr::If(Box::new(c), Box::new(a), Box::new(b)));
        }
        // (x, y) => body
        if matches!(self.peek(), Some(Tok::P("("))) {
            let save = self.pos;
            self.pos += 1;
            let mut params = vec![];
            let mut ok = true;
            if !self.eat_p(")") {
                loop {
                    match self.next() { Some(Tok::Ident(n)) => { params.push(n); if self.eat_kw("as") { let _ = self.next(); } } _ => { ok = false; break; } }
                    if self.eat_p(")") { break; }
                    if !self.eat_p(",") { ok = false; break; }
                }
            }
            if ok && self.eat_p("=>") { return Ok(Expr::Lambda(params, Box::new(self.expr()?))); }
            self.pos = save;
        }
        self.binary(1)
    }

    fn binop(&self) -> Option<BOp> {
        Some(match self.peek()? {
            Tok::Ident(k) if k == "or" => BOp::Or, Tok::Ident(k) if k == "and" => BOp::And,
            Tok::P("=") => BOp::Eq, Tok::P("<>") => BOp::Ne, Tok::P("<") => BOp::Lt, Tok::P("<=") => BOp::Le, Tok::P(">") => BOp::Gt, Tok::P(">=") => BOp::Ge,
            Tok::P("+") => BOp::Add, Tok::P("-") => BOp::Sub, Tok::P("&") => BOp::Concat, Tok::P("*") => BOp::Mul, Tok::P("/") => BOp::Div,
            _ => return None,
        })
    }

    fn binary(&mut self, min: u8) -> Result<Expr, String> {
        let mut left = self.unary()?;
        while let Some(op) = self.binop() {
            if op.prec() < min { break; }
            self.pos += 1;
            let right = self.binary(op.prec() + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat_p("-") { return Ok(match self.unary()? { Expr::Num(n) => Expr::Num(-n), e => Expr::Unary(UOp::Neg, Box::new(e)) }); }
        if self.eat_p("+") { return self.unary(); }
        if self.eat_kw("not") { return Ok(Expr::Unary(UOp::Not, Box::new(self.unary()?))); }
        if matches!(self.peek(), Some(Tok::Ident(k)) if k == "if" || k == "each" || k == "let") { return self.expr(); }
        let mut e = self.primary()?;
        loop {
            match self.peek() {
                Some(Tok::FieldName(f)) => { let f = f.clone(); self.pos += 1; e = Expr::FieldOf(Box::new(e), f); }
                Some(Tok::P("{")) => { self.pos += 1; let i = self.expr()?; self.expect_p("}")?; e = Expr::Item(Box::new(e), Box::new(i)); }
                Some(Tok::P("(")) => {
                    self.pos += 1;
                    let mut args = vec![];
                    if !self.eat_p(")") { loop { args.push(self.expr()?); if self.eat_p(")") { break; } self.expect_p(",")?; } }
                    e = Expr::Call(Box::new(e), args);
                }
                Some(Tok::P("?")) => { self.pos += 1; }
                _ => break,
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Str(s)) => Ok(Expr::Text(s)),
            Some(Tok::FieldName(f)) => Ok(Expr::Field(f)),
            Some(Tok::Ident(k)) if k == "null" => Ok(Expr::Null),
            Some(Tok::Ident(k)) if k == "true" => Ok(Expr::Bool(true)),
            Some(Tok::Ident(k)) if k == "false" => Ok(Expr::Bool(false)),
            Some(Tok::Ident(k)) if k == "type" => {
                let mut t = String::new();
                if self.eat_kw("nullable") || matches!(self.peek(), Some(Tok::Ident(n)) if n == "nullable") { if matches!(self.peek(), Some(Tok::Ident(n)) if n == "nullable") { self.pos += 1; } t.push_str("nullable "); }
                match self.next() {
                    Some(Tok::Ident(n)) => {
                        t.push_str(&n);
                        if (n == "table" || n == "record") && matches!(self.peek(), Some(Tok::P("["))) {
                            // Keep the row type's text as written.
                            let start = self.pos;
                            let mut depth = 0;
                            while let Some(tk) = self.next() { match tk { Tok::P("[") => depth += 1, Tok::P("]") => { depth -= 1; if depth == 0 { break; } } _ => {} } }
                            t.push(' ');
                            t.push_str(&self.toks[start..self.pos].iter().map(|x| tok_text(&x.0)).collect::<Vec<_>>().join(" "));
                        }
                        Ok(Expr::Type(t))
                    }
                    _ => Err("Expected a type name.".into()),
                }
            }
            Some(Tok::Ident(k)) if KEYWORDS.contains(&k.as_str()) => { self.pos -= 1; Err(format!("Unexpected '{k}'.")) }
            Some(Tok::Ident(n)) => Ok(Expr::Ident(n)),
            Some(Tok::P("(")) => { let e = self.expr()?; self.expect_p(")")?; Ok(Expr::Paren(Box::new(e))) }
            Some(Tok::P("{")) => {
                let mut v = vec![];
                if !self.eat_p("}") { loop { v.push(self.expr()?); if self.eat_p("}") { break; } self.expect_p(",")?; } }
                Ok(Expr::List(v))
            }
            Some(Tok::P("[")) => {
                let mut v = vec![];
                if !self.eat_p("]") {
                    loop {
                        let mut key = self.name()?;
                        while let Some(Tok::Ident(more)) = self.peek().cloned() { if KEYWORDS.contains(&more.as_str()) { break; } self.pos += 1; key.push(' '); key.push_str(&more); }
                        self.expect_p("=")?;
                        v.push((key, self.expr()?));
                        if self.eat_p("]") { break; }
                        self.expect_p(",")?;
                    }
                }
                Ok(Expr::Record(v))
            }
            _ => { self.pos = self.pos.saturating_sub(1); Err(format!("Unexpected token{}.", self.near())) }
        }
    }
}

fn tok_text(t: &Tok) -> String {
    match t { Tok::Num(n) => num_text(*n), Tok::Str(s) => quote(s), Tok::Ident(i) => ident(i), Tok::FieldName(f) => format!("[{}]", field(f)), Tok::P(p) => p.to_string() }
}

/// Parses one whole expression.
pub fn parse(src: &str) -> Result<Expr, String> {
    let mut p = Parser::new(src)?;
    let e = p.expr()?;
    if !p.done() { return Err(format!("Unexpected text after the expression{}.", p.near())); }
    Ok(e)
}

// ---- section documents --------------------------------------------------------------------

/// A member of Section1.m: its name, its expression's source text, and the expression (an error
/// when it is outside what the parser reads).
#[derive(Clone, Debug)]
pub struct Member { pub name: String, pub text: String, pub expr: Result<Expr, String>, pub shared: bool }

/// Splits a section document into its members.
pub fn parse_section(src: &str) -> Result<Vec<Member>, String> {
    let chars: Vec<char> = src.chars().collect();
    let toks = tokenize(src)?;
    let mut i = 0;
    let at = |i: usize| toks.get(i).map(|t| &t.0);
    if !at(0).is_some_and(|t| is_kw(t, "section")) { return Err("The document doesn't start with a section.".into()); }
    i += 1;
    while i < toks.len() && at(i) != Some(&Tok::P(";")) { i += 1; }
    i += 1;
    let mut out = vec![];
    while i < toks.len() {
        // Attributes: [Description = "…"]
        if at(i) == Some(&Tok::P("[")) { let mut d = 0; while i < toks.len() { match at(i) { Some(Tok::P("[")) => d += 1, Some(Tok::P("]")) => { d -= 1; if d == 0 { i += 1; break; } } _ => {} } i += 1; } }
        let shared = at(i).is_some_and(|t| is_kw(t, "shared"));
        if shared { i += 1; }
        let Some(Tok::Ident(name)) = at(i).cloned() else { return Err("Expected a member name.".into()) };
        i += 1;
        if at(i) != Some(&Tok::P("=")) { return Err(format!("Expected '=' after {name}.")); }
        i += 1;
        let start = toks.get(i).map_or(chars.len(), |t| t.1);
        // The member runs to the next ';' outside brackets.
        let mut depth = 0i32;
        let mut j = i;
        while j < toks.len() {
            match at(j) { Some(Tok::P("(" | "[" | "{")) => depth += 1, Some(Tok::P(")" | "]" | "}")) => depth -= 1, Some(Tok::P(";")) if depth <= 0 => break, _ => {} }
            j += 1;
        }
        let end = toks.get(j).map_or(chars.len(), |t| t.1);
        let text: String = chars[start..end].iter().collect::<String>().trim().to_string();
        out.push(Member { name, expr: parse(&text), text, shared });
        i = j + 1;
    }
    Ok(out)
}

// ---- printing -----------------------------------------------------------------------------

/// A text literal.
pub fn quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c { '"' => o.push_str("\"\""), '\r' => o.push_str("#(cr)"), '\n' => o.push_str("#(lf)"), '\t' => o.push_str("#(tab)"), c => o.push(c) }
    }
    o.push('"');
    o
}

/// A name as an identifier: bare when it is a regular identifier, else #"quoted".
pub fn ident(n: &str) -> String {
    let regular = n.chars().next().is_some_and(ident_start) && n.split('.').all(|p| p.chars().next().is_some_and(ident_start) && p.chars().all(ident_part)) && !KEYWORDS.contains(&n);
    if regular || n.starts_with('#') && n.len() > 1 && n[1..].chars().all(ident_part) { n.to_string() } else { format!("#{}", quote(n)) }
}

/// A field name inside [ ].
pub fn field(n: &str) -> String {
    if !n.is_empty() && n.trim() == n && n.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ' ' || c == '.') && !KEYWORDS.contains(&n) && !n.chars().next().unwrap().is_ascii_digit() { n.to_string() } else { format!("#{}", quote(n)) }
}

pub fn num_text(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else if n.is_nan() { "#nan".into() } else if n.is_infinite() { if n > 0.0 { "#infinity".into() } else { "-#infinity".into() } } else { format!("{n}") }
}

fn prec(e: &Expr) -> u8 {
    match e { Expr::Binary(op, _, _) => op.prec(), Expr::Unary(..) => 7, Expr::Each(_) | Expr::Lambda(..) | Expr::If(..) | Expr::Let(..) => 0, _ => 8 }
}

/// An expression in M (single line, spaced as Excel's editor writes).
pub fn print(e: &Expr) -> String {
    match e {
        Expr::Null => "null".into(),
        Expr::Bool(b) => b.to_string(),
        Expr::Num(n) => num_text(*n),
        Expr::Text(s) => quote(s),
        Expr::Ident(n) => ident(n),
        Expr::Field(f) => format!("[{}]", field(f)),
        Expr::FieldOf(x, f) => format!("{}[{}]", wrap(x, 8), field(f)),
        Expr::Item(x, i) => format!("{}{{{}}}", wrap(x, 8), print(i)),
        Expr::List(v) => format!("{{{}}}", v.iter().map(print).collect::<Vec<_>>().join(", ")),
        Expr::Record(v) => format!("[{}]", v.iter().map(|(k, x)| format!("{}={}", field(k), print(x))).collect::<Vec<_>>().join(", ")),
        Expr::Call(f, a) => format!("{}({})", wrap(f, 8), a.iter().map(print).collect::<Vec<_>>().join(", ")),
        Expr::Each(b) => format!("each {}", print(b)),
        Expr::Lambda(p, b) => format!("({}) => {}", p.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "), print(b)),
        Expr::Unary(UOp::Neg, x) => format!("-{}", wrap(x, 7)),
        Expr::Unary(UOp::Not, x) => format!("not {}", wrap(x, 7)),
        Expr::Binary(op, a, b) => format!("{} {} {}", wrap(a, op.prec()), op.text(), wrap(b, op.prec() + 1)),
        Expr::If(c, a, b) => format!("if {} then {} else {}", print(c), print(a), print(b)),
        Expr::Let(binds, body) => format!("let {} in {}", binds.iter().map(|(n, x)| format!("{} = {}", ident(n), print(x))).collect::<Vec<_>>().join(", "), print(body)),
        Expr::Type(t) => format!("type {t}"),
        Expr::Paren(x) => format!("({})", print(x)),
    }
}

fn wrap(e: &Expr, min: u8) -> String { if prec(e) < min { format!("({})", print(e)) } else { print(e) } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_m() {
        let src = "let\r\n    Source = Excel.CurrentWorkbook(){[Name=\"Table1\"]}[Content],\r\n    #\"Changed Type\" = Table.TransformColumnTypes(Source,{{\"a\", Int64.Type}, {\"Unit Price\", type nullable number}})\r\nin\r\n    #\"Changed Type\"";
        let e = parse(src).unwrap();
        let Expr::Let(b, body) = &e else { panic!() };
        assert_eq!(b.len(), 2);
        assert_eq!(**body, Expr::Ident("Changed Type".into()));
        assert_eq!(print(&b[0].1), "Excel.CurrentWorkbook(){[Name=\"Table1\"]}[Content]");
        assert_eq!(print(&b[1].1), "Table.TransformColumnTypes(Source, {{\"a\", Int64.Type}, {\"Unit Price\", type nullable number}})");
        let f = parse("each if [Unit Price] * 2 > 10 and not Text.StartsWith([Name], \"x\") then \"big\" & [Name] else null").unwrap();
        assert_eq!(print(&f), "each if [Unit Price] * 2 > 10 and not Text.StartsWith([Name], \"x\") then \"big\" & [Name] else null");
        assert_eq!(print(&parse("(1 + 2) * -3").unwrap()), "(1 + 2) * -3");
        assert_eq!(print(&parse("[#\"a]b\"] = \"q\"\"\"").unwrap()), "[#\"a]b\"] = \"q\"\"\"");
        assert_eq!(ident("Changed Type"), "#\"Changed Type\"");
        assert_eq!(ident("Source"), "Source");
        assert_eq!(ident("each"), "#\"each\"");
    }

    #[test]
    fn splits_sections() {
        let m = parse_section("section Section1;\r\n\r\nshared Query1 = let\r\n    Source = 1+1\r\nin\r\n    Source;\r\n\r\nshared #\"My Q\" = let a = \"x;y\" in a;").unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "Query1");
        assert_eq!(m[1].name, "My Q");
        assert_eq!(m[1].text, "let a = \"x;y\" in a");
        assert!(m[1].expr.is_ok());
    }
}
