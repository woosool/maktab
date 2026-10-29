//! The formula tokenizer (formula/tokenizer.js): the text after "=" as a flat token list that
//! the parser and the reference rewriters (fill, copy, insert/delete rows) share, so the two can
//! never disagree about where a reference starts and ends.

use crate::value::ErrKind;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Num(f64),
    Str(String),
    Bool(bool),
    Err(ErrKind),
    /// A1, $A$1, A1:B2, A:A, 1:1 with an optional sheet (Sheet1!A1) or 3D span (S1:S3!A1); A1# spills.
    Ref { sheet: Option<String>, sheet_end: Option<String>, body: String, spill: bool },
    /// A function or defined name (upper case), optionally scoped to a sheet.
    Name { value: String, sheet: Option<String> },
    /// Table1[Column] or [@Column].
    TableRef { table: Option<String>, spec: String },
    Op(String),
    LParen, RParen,
    /// ',' or ';' (';' separates array rows).
    Sep(char),
    LBrace, RBrace,
    Colon,
    Space,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token { pub tok: Tok, pub start: usize, pub end: usize }

#[derive(Clone, Debug, PartialEq)]
pub struct SyntaxError { pub message: String, pub pos: usize }

fn ident_start(c: char) -> bool { c.is_alphabetic() || c == '_' || c == '\\' }
fn ident_char(c: char) -> bool { c.is_alphanumeric() || c == '_' || c == '.' || c == '\\' || c == '?' }

/// A name as it is compared: upper case where a character has a single upper-case form.
pub fn upper_name(s: &str) -> String {
    s.chars().map(|ch| { let mut u = ch.to_uppercase(); match (u.next(), u.next()) { (Some(x), None) => x, _ => ch } }).collect()
}

struct Lex<'a> { c: &'a [char], i: usize }

impl Lex<'_> {
    fn peek(&self, k: usize) -> Option<char> { self.c.get(self.i + k).copied() }
    /// A sheet name at `i`: quoted 'My Sheet' or a plain identifier; returns (name, end).
    fn sheet_name(&self, i: usize) -> Option<(String, usize)> {
        let c = self.c;
        if c.get(i) == Some(&'\'') {
            let mut j = i + 1; let mut s = String::new();
            while j < c.len() {
                if c[j] == '\'' { if c.get(j + 1) == Some(&'\'') { s.push('\''); j += 2; continue; } return (!s.is_empty()).then_some((s, j + 1)); }
                s.push(c[j]); j += 1;
            }
            return None;
        }
        // Plain sheet names may start with a digit only when quoted, as files write them.
        if !c.get(i).is_some_and(|&ch| ident_start(ch)) { return None; }
        let mut j = i + 1;
        while j < c.len() && ident_char(c[j]) { j += 1; }
        Some((c[i..j].iter().collect(), j))
    }
    /// Sheet1! or 'A b'! or Sheet1:Sheet3! at `i`.
    fn sheet_prefix(&self, i: usize) -> Option<(String, Option<String>, usize)> {
        if self.c[i..].starts_with(&['#', 'R', 'E', 'F', '!']) { return Some(("#REF".into(), None, i + 5)); }
        let (a, j) = self.sheet_name(i)?;
        if self.c.get(j) == Some(&'!') { return Some((a, None, j + 1)); }
        if self.c.get(j) == Some(&':') {
            let (b, k) = self.sheet_name(j + 1)?;
            if self.c.get(k) == Some(&'!') { return Some((a, Some(b), k + 1)); }
        }
        None
    }
    fn dollar_letters(&self, mut j: usize) -> Option<usize> {
        if self.c.get(j) == Some(&'$') { j += 1; }
        let s = j;
        while j < self.c.len() && self.c[j].is_ascii_alphabetic() && j - s < 3 { j += 1; }
        (j > s).then_some(j)
    }
    fn dollar_digits(&self, mut j: usize) -> Option<usize> {
        if self.c.get(j) == Some(&'$') { j += 1; }
        let s = j;
        while j < self.c.len() && self.c[j].is_ascii_digit() && j - s < 7 { j += 1; }
        (j > s).then_some(j)
    }
    fn cell(&self, j: usize) -> Option<usize> { let k = self.dollar_letters(j)?; self.dollar_digits(k) }
    /// The end of a reference body at `j` (A1, A1:B2, A:B, 1:2), checked to end where a name could not go on.
    fn ref_body(&self, j: usize) -> Option<usize> {
        let ends = |k: usize| !self.c.get(k).is_some_and(|&ch| ch.is_alphanumeric() || ch == '_' || ch == '(' || ch == '!' || ch == '.' || ch == '\\' || ch == '?');
        if let Some(k) = self.cell(j) {
            if self.c.get(k) == Some(&':') { if let Some(m) = self.cell(k + 1) { if ends(m) { return Some(m); } } }
            if ends(k) { return Some(k); }
            return None;
        }
        if let Some(k) = self.dollar_letters(j) {
            if self.c.get(k) == Some(&':') { if let Some(m) = self.dollar_letters(k + 1) { if ends(m) && !self.c.get(m).is_some_and(|c| c.is_ascii_digit()) { return Some(m); } } }
        }
        if let Some(k) = self.dollar_digits(j) {
            if self.c.get(k) == Some(&':') { if let Some(m) = self.dollar_digits(k + 1) { if ends(m) { return Some(m); } } }
        }
        None
    }
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, SyntaxError> {
    let chars: Vec<char> = src.chars().collect();
    let mut lx = Lex { c: &chars, i: 0 };
    let mut out: Vec<Token> = Vec::new();
    while lx.i < chars.len() {
        let start = lx.i;
        let ch = chars[start];
        let push = |out: &mut Vec<Token>, tok: Tok, end: usize| out.push(Token { tok, start, end });
        if ch.is_whitespace() {
            while lx.i < chars.len() && chars[lx.i].is_whitespace() { lx.i += 1; }
            push(&mut out, Tok::Space, lx.i);
            continue;
        }
        if ch == '"' {
            let mut j = start + 1; let mut s = String::new();
            loop {
                if j >= chars.len() { return Err(SyntaxError { message: "Unclosed text".into(), pos: start }); }
                if chars[j] == '"' { if chars.get(j + 1) == Some(&'"') { s.push('"'); j += 2; continue; } j += 1; break; }
                s.push(chars[j]); j += 1;
            }
            lx.i = j; push(&mut out, Tok::Str(s), j); continue;
        }
        if ch == '#' {
            let rest: String = chars[start..].iter().take(8).collect::<String>().to_ascii_uppercase();
            if let Some(e) = ErrKind::ALL.into_iter().find(|e| rest.starts_with(e.text())) {
                // #REF!A1 is a reference to a deleted sheet.
                if e == ErrKind::Ref { if let Some(k) = lx.ref_body(start + 5) { let body: String = chars[start + 5..k].iter().collect(); lx.i = k; push(&mut out, Tok::Ref { sheet: Some("#REF".into()), sheet_end: None, body, spill: false }, k); continue; } }
                lx.i = start + e.text().chars().count(); push(&mut out, Tok::Err(e), lx.i); continue;
            }
            // A1# — the spill operator after a reference.
            if let Some(Token { tok: Tok::Ref { spill, .. }, end, .. }) = out.last_mut() { if *end == start { *spill = true; *end = start + 1; lx.i += 1; continue; } }
            return Err(SyntaxError { message: "Unexpected #".into(), pos: start });
        }
        // References and names, with an optional sheet prefix.
        if ch == '\'' || ident_start(ch) || ch == '$' || ch.is_ascii_digit() {
            let (sheet, sheet_end, body_at) = match lx.sheet_prefix(start) { Some((a, b, j)) => (Some(a), b, j), None => (None, None, start) };
            if let Some(k) = lx.ref_body(body_at) {
                // A bare 1:2 needs care (numbers); a bare digit run with no colon is a number.
                let body: String = chars[body_at..k].iter().collect();
                let digits_only = body.chars().all(|c| c.is_ascii_digit() || c == '$');
                if !(sheet.is_none() && digits_only) || body.contains(':') {
                    lx.i = k; push(&mut out, Tok::Ref { sheet, sheet_end, body, spill: false }, k); continue;
                }
            }
            if let Some(sheet) = sheet.clone() {
                if let Some((n, k)) = lx.sheet_name(body_at).filter(|_| chars.get(body_at) != Some(&'\'')) {
                    lx.i = k; push(&mut out, Tok::Name { value: upper_name(&n), sheet: Some(sheet) }, k); continue;
                }
            }
        }
        if ch.is_ascii_digit() || (ch == '.' && lx.peek(1).is_some_and(|c| c.is_ascii_digit())) {
            let mut j = start;
            while j < chars.len() && chars[j].is_ascii_digit() { j += 1; }
            if chars.get(j) == Some(&'.') { j += 1; while j < chars.len() && chars[j].is_ascii_digit() { j += 1; } }
            if matches!(chars.get(j), Some('e' | 'E')) {
                let mut k = j + 1;
                if matches!(chars.get(k), Some('+' | '-')) { k += 1; }
                if chars.get(k).is_some_and(|c| c.is_ascii_digit()) { while k < chars.len() && chars[k].is_ascii_digit() { k += 1; } j = k; }
            }
            let s: String = chars[start..j].iter().collect();
            lx.i = j; push(&mut out, Tok::Num(s.parse().map_err(|_| SyntaxError { message: "Bad number".into(), pos: start })?), j); continue;
        }
        if ident_start(ch) {
            let mut j = start + 1;
            while j < chars.len() && ident_char(chars[j]) { j += 1; }
            let s: String = chars[start..j].iter().collect();
            // Table1[Column]
            if chars.get(j) == Some(&'[') {
                let (spec, k) = bracket(&chars, j)?;
                lx.i = k; push(&mut out, Tok::TableRef { table: Some(s), spec }, k); continue;
            }
            lx.i = j;
            let u = upper_name(&s);
            let next_paren = chars[j..].iter().find(|c| !c.is_whitespace()) == Some(&'(');
            match u.as_str() { "TRUE" | "FALSE" if !next_paren => push(&mut out, Tok::Bool(u == "TRUE"), j), _ => push(&mut out, Tok::Name { value: u, sheet: None }, j) }
            continue;
        }
        if ch == '[' {
            let (spec, k) = bracket(&chars, start)?;
            lx.i = k; push(&mut out, Tok::TableRef { table: None, spec }, k); continue;
        }
        let two: String = chars[start..(start + 2).min(chars.len())].iter().collect();
        let (tok, len) = match ch {
            '(' => (Tok::LParen, 1), ')' => (Tok::RParen, 1), ',' | ';' => (Tok::Sep(ch), 1), '{' => (Tok::LBrace, 1), '}' => (Tok::RBrace, 1), ':' => (Tok::Colon, 1),
            '<' | '>' if two == "<=" || two == ">=" || two == "<>" => (Tok::Op(two.clone()), 2),
            '+' | '-' | '*' | '/' | '^' | '&' | '=' | '<' | '>' | '%' | '@' => (Tok::Op(ch.to_string()), 1),
            _ => return Err(SyntaxError { message: format!("Unexpected '{ch}'"), pos: start }),
        };
        lx.i = start + len; push(&mut out, tok, lx.i);
    }
    Ok(out)
}

/// A bracketed table specifier from '[' at `i`, nested brackets included; returns (inside, end).
fn bracket(c: &[char], i: usize) -> Result<(String, usize), SyntaxError> {
    let mut depth = 0; let mut j = i;
    while j < c.len() {
        match c[j] { '[' => depth += 1, ']' => { depth -= 1; if depth == 0 { return Ok((c[i + 1..j].iter().collect(), j + 1)); } } '\'' => j += 1, _ => {} }
        j += 1;
    }
    Err(SyntaxError { message: "Unclosed [".into(), pos: i })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refs() {
        let t = tokenize("SUM(Sheet1!A1:B2,'My S'!$C$3)+LOG10(2)").unwrap();
        assert!(matches!(&t[2].tok, Tok::Ref { sheet: Some(s), body, .. } if s == "Sheet1" && body == "A1:B2"));
        assert!(matches!(&t[4].tok, Tok::Ref { sheet: Some(s), body, .. } if s == "My S" && body == "$C$3"));
        assert!(matches!(&t[7].tok, Tok::Name { value, .. } if value == "LOG10"));
        let t = tokenize("A:A+1:3+A1#").unwrap();
        assert!(matches!(&t[0].tok, Tok::Ref { body, .. } if body == "A:A"));
        assert!(matches!(&t[2].tok, Tok::Ref { body, .. } if body == "1:3"));
        assert!(matches!(&t[4].tok, Tok::Ref { spill: true, .. }));
        assert!(matches!(tokenize("1.5e3").unwrap()[0].tok, Tok::Num(n) if n == 1500.0));
    }
}
