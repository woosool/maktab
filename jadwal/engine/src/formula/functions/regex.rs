//! A small backtracking regex engine for REGEXMATCH/EXTRACT/REPLACE/TEST (JavaScript syntax:
//! classes, groups, named groups, lookaround, backreferences, lazy quantifiers, flags i/m/s).
//! Matching works on chars and counts its steps: a pattern that would backtrack catastrophically
//! (the nested repeats regex-safety.js refuses) runs out of budget and is reported as an error.

use std::cell::Cell;

const STEP_LIMIT: u64 = 2_000_000;
const MAX_REPEAT: usize = 10_000;

#[derive(Debug, Clone)]
enum Item { Range(char, char), Digit(bool), Word(bool), Space(bool) }

#[derive(Debug, Clone)]
enum Node {
    Char(char), Any, Class(Vec<Item>, bool), Start, End, WordB(bool),
    Group(Option<usize>, Vec<Vec<Node>>), Look(bool, bool, Vec<Vec<Node>>), Backref(usize),
    Repeat(Box<Node>, usize, usize, bool),
}

pub type Caps = Vec<Option<(usize, usize)>>;

pub struct Regex { alts: Vec<Vec<Node>>, pub groups: usize, pub names: Vec<(String, usize)>, icase: bool, multi: bool, dotall: bool }

/// The match failed to finish within the step budget.
pub struct TooHard;

struct P<'a> { c: &'a [char], i: usize, groups: usize, names: Vec<(String, usize)> }

impl<'a> P<'a> {
    fn peek(&self) -> Option<char> { self.c.get(self.i).copied() }
    fn alts(&mut self, nested: bool) -> Result<Vec<Vec<Node>>, ()> {
        let mut alts = vec![vec![]];
        while let Some(ch) = self.peek() {
            if ch == ')' { if !nested { return Err(()); } break; }
            if ch == '|' { alts.push(vec![]); self.i += 1; continue; }
            let a = self.atom()?;
            let a = self.quant(a)?;
            if let Some(last) = alts.last_mut() { last.push(a); }
        }
        if nested && self.peek() != Some(')') { return Err(()); }
        Ok(alts)
    }
    fn num(&mut self) -> Option<usize> { let s = self.i; while self.peek().is_some_and(|c| c.is_ascii_digit()) { self.i += 1; } if s == self.i { None } else { self.c[s..self.i].iter().collect::<String>().parse().ok().map(|n: usize| n.min(MAX_REPEAT + 1)) } }
    fn quant(&mut self, a: Node) -> Result<Node, ()> {
        let (min, max) = match self.peek() {
            Some('*') => { self.i += 1; (0, usize::MAX) }
            Some('+') => { self.i += 1; (1, usize::MAX) }
            Some('?') => { self.i += 1; (0, 1) }
            Some('{') => {
                let save = self.i; self.i += 1;
                match self.num() {
                    Some(lo) => {
                        let hi = if self.peek() == Some(',') { self.i += 1; self.num().unwrap_or(usize::MAX) } else { lo };
                        if self.peek() != Some('}') { self.i = save; return Ok(a); }
                        self.i += 1; if hi < lo || lo > MAX_REPEAT { return Err(()); } (lo, hi)
                    }
                    None => { self.i = save; return Ok(a); }
                }
            }
            _ => return Ok(a),
        };
        if matches!(a, Node::Start | Node::End | Node::WordB(_)) { return Err(()); }
        let greedy = if self.peek() == Some('?') { self.i += 1; false } else { true };
        if matches!(self.peek(), Some('*' | '+' | '?')) { return Err(()); }
        Ok(Node::Repeat(Box::new(a), min, max, greedy))
    }
    fn escape(&mut self, in_class: bool) -> Result<Item, Node> {
        let ch = self.peek().unwrap_or('\\'); self.i += 1;
        let lit = |c: char| Ok(Item::Range(c, c));
        match ch {
            'd' => Ok(Item::Digit(true)), 'D' => Ok(Item::Digit(false)), 'w' => Ok(Item::Word(true)), 'W' => Ok(Item::Word(false)),
            's' => Ok(Item::Space(true)), 'S' => Ok(Item::Space(false)),
            'n' => lit('\n'), 't' => lit('\t'), 'r' => lit('\r'), 'f' => lit('\u{c}'), 'v' => lit('\u{b}'), '0' => lit('\0'),
            'b' if in_class => lit('\u{8}'),
            'b' => Err(Node::WordB(true)), 'B' => Err(Node::WordB(false)),
            'x' | 'u' => {
                let braced = ch == 'u' && self.peek() == Some('{');
                if braced { self.i += 1; }
                let len = if ch == 'x' { 2 } else if braced { 6 } else { 4 };
                let s = self.i; while self.i < s + len && self.peek().is_some_and(|c| c.is_ascii_hexdigit()) { self.i += 1; }
                if braced { if self.peek() == Some('}') { self.i += 1; } }
                let h: String = self.c[s..self.i].iter().collect();
                match u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) { Some(c) if !h.is_empty() => lit(c), _ => { self.i = s; lit(ch) } }
            }
            '1'..='9' if !in_class => { self.i -= 1; let n = self.num().unwrap_or(1); Err(Node::Backref(n)) }
            c => lit(c),
        }
    }
    fn atom(&mut self) -> Result<Node, ()> {
        let ch = self.peek().ok_or(())?; self.i += 1;
        Ok(match ch {
            '.' => Node::Any, '^' => Node::Start, '$' => Node::End,
            '*' | '+' | '?' => return Err(()),
            '(' => {
                let mut cap = None;
                let mut look = None;
                if self.peek() == Some('?') {
                    self.i += 1;
                    match self.peek() {
                        Some(':') => self.i += 1,
                        Some('=') => { self.i += 1; look = Some((true, false)); }
                        Some('!') => { self.i += 1; look = Some((true, true)); }
                        Some('<') if matches!(self.c.get(self.i + 1), Some('=' | '!')) => { look = Some((false, self.c[self.i + 1] == '!')); self.i += 2; }
                        Some('<') | Some('P') => {
                            if self.peek() == Some('P') { self.i += 1; }
                            if self.peek() != Some('<') { return Err(()); }
                            self.i += 1; let s = self.i;
                            while self.peek().is_some_and(|c| c != '>') { self.i += 1; }
                            if self.peek() != Some('>') { return Err(()); }
                            let name: String = self.c[s..self.i].iter().collect(); self.i += 1;
                            self.groups += 1; self.names.push((name, self.groups)); cap = Some(self.groups);
                        }
                        _ => return Err(()),
                    }
                } else { self.groups += 1; cap = Some(self.groups); }
                let alts = self.alts(true)?; self.i += 1;
                match look { Some((ahead, neg)) => Node::Look(ahead, neg, alts), None => Node::Group(cap, alts) }
            }
            '[' => {
                let neg = if self.peek() == Some('^') { self.i += 1; true } else { false };
                let mut items = vec![];
                loop {
                    let c = self.peek().ok_or(())?;
                    if c == ']' { self.i += 1; break; }
                    self.i += 1;
                    let lo = if c == '\\' { match self.escape(true) { Ok(it) => it, Err(_) => return Err(()) } } else { Item::Range(c, c) };
                    if let Item::Range(l, _) = lo {
                        if self.peek() == Some('-') && self.c.get(self.i + 1).is_some_and(|c| *c != ']') {
                            self.i += 1; let h = self.peek().ok_or(())?; self.i += 1;
                            let hi = if h == '\\' { match self.escape(true) { Ok(Item::Range(x, _)) => x, _ => return Err(()) } } else { h };
                            if hi < l { return Err(()); }
                            items.push(Item::Range(l, hi)); continue;
                        }
                    }
                    items.push(lo);
                }
                Node::Class(items, neg)
            }
            '\\' => { if self.peek().is_none() { return Err(()); } match self.escape(false) { Ok(Item::Range(c, _)) => Node::Char(c), Ok(it) => Node::Class(vec![it], false), Err(n) => n } }
            ')' => return Err(()),
            c => Node::Char(c),
        })
    }
}

fn is_word(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }
fn lower(c: char) -> char { c.to_lowercase().next().unwrap_or(c) }

struct M<'a> { re: &'a Regex, t: &'a [char], steps: Cell<u64> }

type K<'k> = &'k mut dyn FnMut(&M, usize, &mut Caps) -> bool;

impl<'a> M<'a> {
    fn tick(&self) -> bool { let s = self.steps.get() + 1; self.steps.set(s); s > STEP_LIMIT }
    fn item(&self, it: &Item, c: char) -> bool {
        match it {
            Item::Range(lo, hi) => (*lo..=*hi).contains(&c) || (self.re.icase && { let l = lower(c); let u = c.to_uppercase().next().unwrap_or(c); (*lo..=*hi).contains(&l) || (*lo..=*hi).contains(&u) }),
            Item::Digit(p) => c.is_ascii_digit() == *p, Item::Word(p) => is_word(c) == *p, Item::Space(p) => c.is_whitespace() == *p,
        }
    }
    /// One char-consuming node at `pos`, if it matches.
    fn single(&self, n: &Node, pos: usize) -> Option<bool> {
        let c = self.t.get(pos).copied();
        Some(match n {
            Node::Char(x) => c.is_some_and(|c| c == *x || (self.re.icase && lower(c) == lower(*x))),
            Node::Any => c.is_some_and(|c| self.re.dotall || !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')),
            Node::Class(items, neg) => c.is_some_and(|c| items.iter().any(|it| self.item(it, c)) != *neg),
            _ => return None,
        })
    }
    fn alts(&self, alts: &[Vec<Node>], pos: usize, caps: &mut Caps, k: K) -> bool {
        for a in alts { if self.seq(a, pos, caps, k) { return true; } if self.steps.get() > STEP_LIMIT { return false; } }
        false
    }
    fn seq(&self, nodes: &[Node], pos: usize, caps: &mut Caps, k: K) -> bool {
        if self.tick() { return false; }
        let Some((n, rest)) = nodes.split_first() else { return k(self, pos, caps) };
        if let Some(ok) = self.single(n, pos) { return ok && self.seq(rest, pos + 1, caps, k); }
        let t = self.t;
        match n {
            Node::Start => (pos == 0 || (self.re.multi && t[pos - 1] == '\n')) && self.seq(rest, pos, caps, k),
            Node::End => (pos == t.len() || (self.re.multi && t[pos] == '\n')) && self.seq(rest, pos, caps, k),
            Node::WordB(want) => { let a = pos > 0 && is_word(t[pos - 1]); let b = pos < t.len() && is_word(t[pos]); ((a != b) == *want) && self.seq(rest, pos, caps, k) }
            Node::Backref(g) => {
                let Some(Some((s, e))) = caps.get(*g).copied() else { return self.seq(rest, pos, caps, k) };
                let len = e - s;
                if pos + len > t.len() { return false; }
                let ok = (0..len).all(|j| { let (x, y) = (t[s + j], t[pos + j]); x == y || (self.re.icase && lower(x) == lower(y)) });
                ok && self.seq(rest, pos + len, caps, k)
            }
            Node::Group(cap, alts) => self.alts(alts, pos, caps, &mut |m: &M, p, caps: &mut Caps| {
                let old = cap.map(|g| caps[g]);
                if let Some(g) = cap { caps[*g] = Some((pos, p)); }
                if m.seq(rest, p, caps, k) { return true; }
                if let (Some(g), Some(o)) = (cap, old) { caps[*g] = o; }
                false
            }),
            Node::Look(ahead, neg, alts) => {
                let mut inner = caps.clone();
                let found = if *ahead { self.alts(alts, pos, &mut inner, &mut |_: &M, _, _: &mut Caps| true) } else {
                    (0..=pos).rev().any(|st| self.alts(alts, st, &mut inner, &mut |_: &M, p, _: &mut Caps| p == pos))
                };
                if found == *neg { return false; }
                if !*neg { let save = caps.clone(); *caps = inner; if self.seq(rest, pos, caps, k) { return true; } *caps = save; return false; }
                self.seq(rest, pos, caps, k)
            }
            Node::Repeat(node, min, max, greedy) => self.rep(node, *min, *max, *greedy, 0, pos, usize::MAX, rest, caps, k),
            _ => false,
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn rep(&self, node: &Node, min: usize, max: usize, greedy: bool, count: usize, pos: usize, last: usize, rest: &[Node], caps: &mut Caps, k: K) -> bool {
        if self.tick() { return false; }
        // A repeat whose last pass took nothing stops (as JavaScript does), so x* over "" ends.
        if count > min && pos == last { return false; }
        // Plain single-char repeats run without recursion per char.
        if self.single(node, pos).is_some() {
            let mut n = 0; while n < max && self.single(node, pos + n) == Some(true) { n += 1; if self.tick() { return false; } }
            if n < min { return false; }
            if greedy { for j in (min..=n).rev() { if self.seq(rest, pos + j, caps, k) { return true; } if self.steps.get() > STEP_LIMIT { return false; } } }
            else { for j in min..=n { if self.seq(rest, pos + j, caps, k) { return true; } if self.steps.get() > STEP_LIMIT { return false; } } }
            return false;
        }
        let more = |m: &M, caps: &mut Caps, k: K| count < max && m.seq(std::slice::from_ref(node), pos, caps, &mut |m: &M, p, caps: &mut Caps| m.rep(node, min, max, greedy, count + 1, p, pos, rest, caps, k));
        if count < min { return more(self, caps, k); }
        if greedy { more(self, caps, k) || self.seq(rest, pos, caps, k) } else { self.seq(rest, pos, caps, k) || more(self, caps, k) }
    }
}

impl Regex {
    /// Compiles a pattern; RE2-style leading inline flags (?i)(?s)(?m) are accepted.
    pub fn new(pattern: &str, icase: bool) -> Option<Regex> {
        let mut p = pattern; let (mut icase, mut multi, mut dotall) = (icase, false, false);
        while let Some(rest) = p.strip_prefix("(?") {
            let end = rest.find(')')?; let f = &rest[..end];
            if f.is_empty() || !f.chars().all(|c| matches!(c, 'i' | 'm' | 's')) { break; }
            icase |= f.contains('i'); multi |= f.contains('m'); dotall |= f.contains('s'); p = &rest[end + 1..];
        }
        let c: Vec<char> = p.chars().collect();
        let mut ps = P { c: &c, i: 0, groups: 0, names: vec![] };
        let alts = ps.alts(false).ok()?;
        Some(Regex { alts, groups: ps.groups, names: ps.names, icase, multi, dotall })
    }
    /// The first match at or after char `from`: group spans (0 = the whole match) in chars.
    pub fn find_at(&self, t: &[char], from: usize) -> Result<Option<Caps>, TooHard> {
        let m = M { re: self, t, steps: Cell::new(0) };
        for st in from..=t.len() {
            let mut caps: Caps = vec![None; self.groups + 1];
            let mut end = None;
            if m.alts(&self.alts, st, &mut caps, &mut |_: &M, p, _: &mut Caps| { end = Some(p); true }) {
                caps[0] = end.map(|e| (st, e));
                return Ok(Some(caps));
            }
            if m.steps.get() > STEP_LIMIT { return Err(TooHard); }
        }
        Ok(None)
    }
    /// Every non-overlapping match, as JavaScript's matchAll finds them.
    pub fn find_all(&self, t: &[char]) -> Result<Vec<Caps>, TooHard> {
        let mut out = vec![]; let mut at = 0;
        while at <= t.len() {
            let Some(c) = self.find_at(t, at)? else { break };
            let Some((s, e)) = c[0] else { break };
            at = if e == s { e + 1 } else { e };
            out.push(c);
            if out.len() > 100_000 { return Err(TooHard); }
        }
        Ok(out)
    }
}

/// The text of a group span.
pub fn span(t: &[char], c: Option<(usize, usize)>) -> Option<String> { c.map(|(s, e)| t[s..e].iter().collect()) }
