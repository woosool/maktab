//! Criteria as COUNTIF, SUMIFS and the database functions read them (">=5", "<>x", "a*b?",
//! "=", a number), and Excel's wildcards (* any run, ? one character, ~ escapes).

use super::super::eval::compare;
use crate::value::Value;
use std::cmp::Ordering;

#[derive(Clone, Debug)]
pub struct Criteria { op: Op, target: Value, pattern: Option<Vec<char>> }

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op { Eq, Ne, Lt, Gt, Le, Ge }

impl Criteria {
    pub fn new(c: &Value) -> Criteria {
        let Value::Str(s) = c else {
            return Criteria { op: Op::Eq, target: if c.is_empty() { Value::str("") } else { c.clone() }, pattern: None };
        };
        let (op, rest) = [("<=", Op::Le), (">=", Op::Ge), ("<>", Op::Ne), ("<", Op::Lt), (">", Op::Gt), ("=", Op::Eq)].iter()
            .find(|(p, _)| s.starts_with(p)).map(|(p, o)| (*o, &s[p.len()..])).unwrap_or((Op::Eq, &s[..]));
        let target = if rest.is_empty() { Value::str("") } else if let Some(n) = crate::numfmt::parse_number(rest) { Value::Num(n) }
            else if let Some((n, _)) = crate::numfmt::parse_input(rest, false).filter(|_| rest.contains(['/', '-', ':'])) { Value::Num(n) }
            else if rest.eq_ignore_ascii_case("TRUE") { Value::Bool(true) } else if rest.eq_ignore_ascii_case("FALSE") { Value::Bool(false) }
            else if let Some(e) = crate::value::ErrKind::parse(rest) { Value::Err(e) } else { Value::str(rest) };
        let pattern = match (&target, op) { (Value::Str(t), Op::Eq | Op::Ne) if t.contains(['*', '?', '~']) => Some(t.to_lowercase().chars().collect()), _ => None };
        Criteria { op, target, pattern }
    }

    pub fn test(&self, v: &Value) -> bool {
        if let Some(p) = &self.pattern {
            let hit = match v { Value::Str(s) => wildcard(p, &s.to_lowercase().chars().collect::<Vec<_>>()), Value::Empty => wildcard(p, &[]), _ => false };
            return if self.op == Op::Eq { hit } else { !hit };
        }
        match (&self.target, v) {
            // "" matches blank cells (and empty text); "=" alone too.
            (Value::Str(t), _) if t.is_empty() => {
                let blank = matches!(v, Value::Empty) || matches!(v, Value::Str(s) if s.is_empty());
                return match self.op { Op::Eq => blank, Op::Ne => !blank, _ => false };
            }
            (Value::Num(_), Value::Str(s)) => {
                // Text that reads as a number still does not match a number criterion.
                let _ = s;
                return self.op == Op::Ne;
            }
            (Value::Num(_), Value::Empty) | (Value::Num(_), Value::Bool(_)) => return self.op == Op::Ne,
            (Value::Str(_), Value::Num(_)) | (Value::Str(_), Value::Bool(_)) | (Value::Str(_), Value::Empty) => return self.op == Op::Ne,
            (Value::Bool(_), v) if !matches!(v, Value::Bool(_)) => return self.op == Op::Ne,
            (Value::Err(a), Value::Err(b)) => return (a == b) == (self.op == Op::Eq),
            (_, Value::Err(_)) => return self.op == Op::Ne,
            _ => {}
        }
        let o = compare(v, &self.target);
        match self.op { Op::Eq => o == Ordering::Equal, Op::Ne => o != Ordering::Equal, Op::Lt => o == Ordering::Less, Op::Gt => o == Ordering::Greater, Op::Le => o != Ordering::Greater, Op::Ge => o != Ordering::Less }
    }
}

/// Matches lower-cased text against a lower-cased wildcard pattern.
pub fn wildcard(p: &[char], s: &[char]) -> bool {
    let (mut pi, mut si) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while si < s.len() {
        if pi < p.len() && p[pi] == '~' && pi + 1 < p.len() {
            if p[pi + 1] == s[si] { pi += 2; si += 1; continue; }
        } else if pi < p.len() && p[pi] == '?' { pi += 1; si += 1; continue; }
        else if pi < p.len() && p[pi] == '*' { star = Some(pi); mark = si; pi += 1; continue; }
        else if pi < p.len() && p[pi] == s[si] { pi += 1; si += 1; continue; }
        if let Some(st) = star { pi = st + 1; mark += 1; si = mark; continue; }
        return false;
    }
    while pi < p.len() && p[pi] == '*' { pi += 1; }
    pi == p.len()
}

/// Wildcard match for MATCH, VLOOKUP and SEARCH (case-insensitive).
pub fn matches_pattern(pattern: &str, text: &str) -> bool {
    wildcard(&pattern.to_lowercase().chars().collect::<Vec<_>>(), &text.to_lowercase().chars().collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crit() {
        assert!(Criteria::new(&Value::str(">=5")).test(&Value::Num(5.0)));
        assert!(!Criteria::new(&Value::str(">=5")).test(&Value::str("9")));
        assert!(Criteria::new(&Value::str("a*")).test(&Value::str("Apple")));
        assert!(Criteria::new(&Value::str("<>a*")).test(&Value::str("pear")));
        assert!(Criteria::new(&Value::str("")).test(&Value::Empty));
        assert!(Criteria::new(&Value::Num(3.0)).test(&Value::Num(3.0)));
        assert!(Criteria::new(&Value::str("3")).test(&Value::Num(3.0)));
        assert!(matches_pattern("b?d", "BAD"));
        assert!(matches_pattern("~*", "*"));
    }
}
