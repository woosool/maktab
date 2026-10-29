//! The formula parser (formula/parser.js): tokens to an expression tree, with Excel's operator
//! precedence (unary minus binds tighter than ^, so =-2^2 is 4).

use super::lexer::{tokenize, SyntaxError, Tok, Token};
use crate::addr::{col_index, MAX_COLS, MAX_ROWS};
use crate::value::ErrKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellRef { pub row: u32, pub col: u32, pub row_abs: bool, pub col_abs: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefKind { Cell, Area, Cols, Rows }

#[derive(Clone, Debug, PartialEq)]
pub struct Reference { pub sheet: Option<String>, pub sheet_end: Option<String>, pub a: CellRef, pub b: CellRef, pub kind: RefKind, pub spill: bool }

impl Reference {
    pub fn range(&self) -> crate::addr::Range { crate::addr::Range::new(crate::addr::Pos::new(self.a.row, self.a.col), crate::addr::Pos::new(self.b.row, self.b.col)) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp { Add, Sub, Mul, Div, Pow, Concat, Eq, Ne, Lt, Gt, Le, Ge }

impl BinOp {
    pub fn text(&self) -> &'static str {
        match self { BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/", BinOp::Pow => "^", BinOp::Concat => "&", BinOp::Eq => "=", BinOp::Ne => "<>", BinOp::Lt => "<", BinOp::Gt => ">", BinOp::Le => "<=", BinOp::Ge => ">=" }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Err(ErrKind),
    Ref(Reference),
    Name { name: String, sheet: Option<String> },
    Table { table: Option<String>, spec: String },
    Call { name: String, args: Vec<Expr> },
    /// LAMBDA(x, x+1)(4): calling the value of an expression.
    Invoke { callee: Box<Expr>, args: Vec<Expr> },
    Neg(Box<Expr>),
    Plus(Box<Expr>),
    Percent(Box<Expr>),
    /// @ implicit intersection.
    Implicit(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// A range built from two reference expressions: A1:INDEX(A:A, 5).
    Span(Box<Expr>, Box<Expr>),
    /// (A1:A3,C1:C3): several areas.
    Union(Vec<Expr>),
    /// A1:C3 B2:D4 — the space operator.
    Intersect(Box<Expr>, Box<Expr>),
    Array(Vec<Vec<Expr>>),
    /// An argument left empty: IF(A1,,2).
    Missing,
}

/// "A1", "$A$1:B2", "A:C", "1:3" → the two corners and the kind.
pub fn parse_ref_body(body: &str) -> Option<(CellRef, CellRef, RefKind)> {
    fn cell(s: &str) -> Option<CellRef> {
        let col_abs = s.starts_with('$');
        let s = s.trim_start_matches('$');
        let split = s.find(|c: char| c == '$' || c.is_ascii_digit())?;
        let (l, rest) = s.split_at(split);
        let row_abs = rest.starts_with('$');
        let row: u32 = rest.trim_start_matches('$').parse().ok()?;
        let col = col_index(l)?;
        (row >= 1 && row <= MAX_ROWS).then_some(CellRef { row: row - 1, col, row_abs, col_abs })
    }
    match body.split_once(':') {
        None => cell(body).map(|c| (c, c, RefKind::Cell)),
        Some((x, y)) => {
            if let (Some(a), Some(b)) = (cell(x), cell(y)) { return Some((a, b, RefKind::Area)); }
            let (xa, ya) = (x.starts_with('$'), y.starts_with('$'));
            let (xs, ys) = (x.trim_start_matches('$'), y.trim_start_matches('$'));
            if let (Some(c0), Some(c1)) = (col_index(xs), col_index(ys)) {
                return Some((CellRef { row: 0, col: c0, row_abs: true, col_abs: xa }, CellRef { row: MAX_ROWS - 1, col: c1, row_abs: true, col_abs: ya }, RefKind::Cols));
            }
            if let (Ok(r0), Ok(r1)) = (xs.parse::<u32>(), ys.parse::<u32>()) {
                if r0 >= 1 && r1 >= 1 && r0 <= MAX_ROWS && r1 <= MAX_ROWS {
                    return Some((CellRef { row: r0 - 1, col: 0, row_abs: xa, col_abs: true }, CellRef { row: r1 - 1, col: MAX_COLS - 1, row_abs: ya, col_abs: true }, RefKind::Rows));
                }
            }
            None
        }
    }
}

struct P { t: Vec<Token>, i: usize }

fn err<T>(m: &str, pos: usize) -> Result<T, SyntaxError> { Err(SyntaxError { message: m.into(), pos }) }

impl P {
    fn peek(&self) -> Option<&Tok> { self.t.get(self.i).map(|t| &t.tok) }
    fn pos(&self) -> usize { self.t.get(self.i).map_or(usize::MAX, |t| t.start) }
    fn next(&mut self) -> Option<Tok> { let t = self.t.get(self.i).map(|t| t.tok.clone()); self.i += 1; t }
    fn is_op(&self, op: &str) -> bool { matches!(self.peek(), Some(Tok::Op(o)) if o == op) }

    fn expr(&mut self) -> Result<Expr, SyntaxError> { self.compare() }
    fn compare(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.concat()?;
        loop {
            let op = match self.peek() { Some(Tok::Op(o)) => match o.as_str() { "=" => BinOp::Eq, "<>" => BinOp::Ne, "<" => BinOp::Lt, ">" => BinOp::Gt, "<=" => BinOp::Le, ">=" => BinOp::Ge, _ => break }, _ => break };
            self.i += 1; let b = self.concat()?; a = Expr::Binary(op, Box::new(a), Box::new(b));
        }
        Ok(a)
    }
    fn concat(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.additive()?;
        while self.is_op("&") { self.i += 1; let b = self.additive()?; a = Expr::Binary(BinOp::Concat, Box::new(a), Box::new(b)); }
        Ok(a)
    }
    fn additive(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.term()?;
        loop {
            let op = if self.is_op("+") { BinOp::Add } else if self.is_op("-") { BinOp::Sub } else { break };
            self.i += 1; let b = self.term()?; a = Expr::Binary(op, Box::new(a), Box::new(b));
        }
        Ok(a)
    }
    fn term(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.power()?;
        loop {
            let op = if self.is_op("*") { BinOp::Mul } else if self.is_op("/") { BinOp::Div } else { break };
            self.i += 1; let b = self.power()?; a = Expr::Binary(op, Box::new(a), Box::new(b));
        }
        Ok(a)
    }
    fn power(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.percent()?;
        while self.is_op("^") { self.i += 1; let b = self.percent()?; a = Expr::Binary(BinOp::Pow, Box::new(a), Box::new(b)); }
        Ok(a)
    }
    fn percent(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.unary()?;
        while self.is_op("%") { self.i += 1; a = Expr::Percent(Box::new(a)); }
        Ok(a)
    }
    fn unary(&mut self) -> Result<Expr, SyntaxError> {
        if self.is_op("-") { self.i += 1; return Ok(Expr::Neg(Box::new(self.unary()?))); }
        if self.is_op("+") { self.i += 1; return Ok(Expr::Plus(Box::new(self.unary()?))); }
        if self.is_op("@") { self.i += 1; return Ok(Expr::Implicit(Box::new(self.unary()?))); }
        self.intersect()
    }
    fn intersect(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.span()?;
        while matches!(self.peek(), Some(Tok::Space)) { self.i += 1; let b = self.span()?; a = Expr::Intersect(Box::new(a), Box::new(b)); }
        Ok(a)
    }
    fn span(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.postfix()?;
        while matches!(self.peek(), Some(Tok::Colon)) { self.i += 1; let b = self.postfix()?; a = Expr::Span(Box::new(a), Box::new(b)); }
        Ok(a)
    }
    fn postfix(&mut self) -> Result<Expr, SyntaxError> {
        let mut a = self.primary()?;
        // LAMBDA(x, x+1)(4)
        while matches!(self.peek(), Some(Tok::LParen)) && matches!(a, Expr::Call { .. } | Expr::Invoke { .. }) {
            self.i += 1; let args = self.args()?; a = Expr::Invoke { callee: Box::new(a), args };
        }
        Ok(a)
    }
    /// Arguments after '(' through ')'.
    fn args(&mut self) -> Result<Vec<Expr>, SyntaxError> {
        let mut args = Vec::new();
        if matches!(self.peek(), Some(Tok::RParen)) { self.i += 1; return Ok(args); }
        loop {
            if matches!(self.peek(), Some(Tok::Sep(_) | Tok::RParen)) { args.push(Expr::Missing); } else { args.push(self.expr()?); }
            match self.next() { Some(Tok::Sep(_)) => continue, Some(Tok::RParen) => break, _ => return err("Expected ) or ,", self.pos()) }
        }
        Ok(args)
    }
    fn primary(&mut self) -> Result<Expr, SyntaxError> {
        let pos = self.pos();
        match self.next() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Str(s)) => Ok(Expr::Str(s)),
            Some(Tok::Bool(b)) => Ok(Expr::Bool(b)),
            Some(Tok::Err(e)) => Ok(Expr::Err(e)),
            Some(Tok::Ref { sheet, sheet_end, body, spill }) => {
                let (a, b, kind) = parse_ref_body(&body).ok_or(SyntaxError { message: "Bad reference".into(), pos })?;
                if sheet.as_deref() == Some("#REF") { return Ok(Expr::Err(ErrKind::Ref)); }
                Ok(Expr::Ref(Reference { sheet, sheet_end, a, b, kind, spill }))
            }
            Some(Tok::TableRef { table, spec }) => Ok(Expr::Table { table, spec }),
            Some(Tok::Name { value, sheet }) => {
                if sheet.is_none() && matches!(self.peek(), Some(Tok::LParen)) {
                    self.i += 1;
                    let name = value.strip_prefix("_XLFN.").or(value.strip_prefix("_XLWS.")).unwrap_or(&value).to_string();
                    let name = name.strip_prefix("_XLPM.").map(String::from).unwrap_or(name);
                    return Ok(Expr::Call { name, args: self.args()? });
                }
                let value = value.strip_prefix("_XLPM.").map(String::from).unwrap_or(value);
                Ok(Expr::Name { name: value, sheet })
            }
            Some(Tok::LParen) => {
                let first = self.expr()?;
                let mut items = vec![first];
                while matches!(self.peek(), Some(Tok::Sep(','))) { self.i += 1; items.push(self.expr()?); }
                if self.next() != Some(Tok::RParen) { return err("Expected )", self.pos()); }
                Ok(if items.len() == 1 { items.pop().unwrap() } else { Expr::Union(items) })
            }
            Some(Tok::LBrace) => {
                let mut rows = vec![vec![]];
                loop {
                    let neg = if self.is_op("-") { self.i += 1; true } else { false };
                    let v = match self.next() {
                        Some(Tok::Num(n)) => Expr::Num(if neg { -n } else { n }),
                        Some(Tok::Str(s)) if !neg => Expr::Str(s),
                        Some(Tok::Bool(b)) if !neg => Expr::Bool(b),
                        Some(Tok::Err(e)) if !neg => Expr::Err(e),
                        _ => return err("Bad array constant", self.pos()),
                    };
                    rows.last_mut().unwrap().push(v);
                    match self.next() { Some(Tok::Sep(',')) => {} Some(Tok::Sep(';')) => rows.push(vec![]), Some(Tok::RBrace) => break, _ => return err("Expected }", self.pos()) }
                }
                if rows.iter().any(|r| r.len() != rows[0].len()) { return err("Array rows differ in length", pos); }
                Ok(Expr::Array(rows))
            }
            Some(_) => err("Unexpected token", pos),
            None => err("Formula ends too soon", pos),
        }
    }
}

/// Parses the text after "=".
pub fn parse(src: &str) -> Result<Expr, SyntaxError> {
    let raw = tokenize(src)?;
    // A space is the intersection operator only between two references; elsewhere it is layout.
    let mut t: Vec<Token> = Vec::with_capacity(raw.len());
    for (k, tok) in raw.iter().enumerate() {
        if tok.tok == Tok::Space {
            let prev = t.last().map(|x| &x.tok);
            let next = raw.get(k + 1).map(|x| &x.tok);
            let left = matches!(prev, Some(Tok::Ref { .. } | Tok::RParen));
            let right = matches!(next, Some(Tok::Ref { .. } | Tok::LParen | Tok::Name { .. }));
            if left && right { t.push(tok.clone()); }
            continue;
        }
        t.push(tok.clone());
    }
    let mut p = P { t, i: 0 };
    if p.t.is_empty() { return err("Empty formula", 0); }
    let e = p.expr()?;
    if p.i < p.t.len() { return err("Unexpected text after the formula", p.pos()); }
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence() {
        assert!(matches!(parse("-2^2").unwrap(), Expr::Binary(BinOp::Pow, ..)));
        assert!(matches!(parse("1+2*3").unwrap(), Expr::Binary(BinOp::Add, ..)));
        assert!(matches!(parse("SUM(A1:B2, ,3)").unwrap(), Expr::Call { args, .. } if args.len() == 3 && args[1] == Expr::Missing));
        assert!(matches!(parse("{1,2;3,4}").unwrap(), Expr::Array(r) if r.len() == 2));
        assert!(matches!(parse("A1:INDEX(A:A,3)").unwrap(), Expr::Span(..)));
        assert!(matches!(parse("_xlfn.XLOOKUP(1,A:A,B:B)").unwrap(), Expr::Call { name, .. } if name == "XLOOKUP"));
        assert!(parse("1+").is_err());
    }
}
