//! Equations: a linear (LaTeX-like, with Unicode math characters) source parsed into a tree,
//! typeset in the TeX manner with Noto Sans Math, and converted to and from Office Math (OMML,
//! §22.1) so they stay live equations in Word.

use crate::layout::DrawItem;
use crate::xml::{Element, Node as XNode};
use crate::Color;

const FONT: &str = "Noto Sans Math";

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Row(Vec<Node>),
    /// A symbol: variables are drawn italic, numbers and operators upright.
    Sym(String, SymKind),
    /// Fraction; `bar` is false for binomials.
    Frac(Box<Node>, Box<Node>, bool),
    Scripts { base: Box<Node>, sub: Option<Box<Node>>, sup: Option<Box<Node>> },
    Rad { deg: Option<Box<Node>>, body: Box<Node> },
    /// A large operator (∑ ∫ ∏ …) with limits and its operand.
    Nary { op: char, sub: Option<Box<Node>>, sup: Option<Box<Node>>, body: Box<Node> },
    /// Stretchy delimiters around a body; '\0' is no delimiter on that side.
    Delim { open: char, close: char, body: Box<Node> },
    /// A function name (sin, log, lim) applied to an argument; `under` is a limit below it.
    Func { name: String, under: Option<Box<Node>>, arg: Box<Node> },
    Matrix(Vec<Vec<Node>>),
    /// An accent (hat, bar, vector arrow, dot, tilde) over a body.
    Accent(char, Box<Node>),
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymKind { Var, Num, Op, Rel, Punct }

// ---------------------------------------------------------------------------
// Parsing

const GREEK: &[(&str, char)] = &[
    ("alpha", 'α'), ("beta", 'β'), ("gamma", 'γ'), ("delta", 'δ'), ("epsilon", 'ϵ'), ("varepsilon", 'ε'), ("zeta", 'ζ'), ("eta", 'η'), ("theta", 'θ'), ("vartheta", 'ϑ'),
    ("iota", 'ι'), ("kappa", 'κ'), ("lambda", 'λ'), ("mu", 'μ'), ("nu", 'ν'), ("xi", 'ξ'), ("pi", 'π'), ("rho", 'ρ'), ("sigma", 'σ'), ("tau", 'τ'),
    ("upsilon", 'υ'), ("phi", 'ϕ'), ("varphi", 'φ'), ("chi", 'χ'), ("psi", 'ψ'), ("omega", 'ω'),
    ("Gamma", 'Γ'), ("Delta", 'Δ'), ("Theta", 'Θ'), ("Lambda", 'Λ'), ("Xi", 'Ξ'), ("Pi", 'Π'), ("Sigma", 'Σ'), ("Upsilon", 'Υ'), ("Phi", 'Φ'), ("Psi", 'Ψ'), ("Omega", 'Ω'),
];

/// Operators and relations by command name.
pub const SYMBOLS: &[(&str, char)] = &[
    ("pm", '±'), ("mp", '∓'), ("times", '×'), ("div", '÷'), ("cdot", '⋅'), ("ast", '∗'), ("star", '⋆'), ("circ", '∘'), ("bullet", '∙'), ("oplus", '⊕'), ("otimes", '⊗'),
    ("le", '≤'), ("leq", '≤'), ("ge", '≥'), ("geq", '≥'), ("ne", '≠'), ("neq", '≠'), ("approx", '≈'), ("equiv", '≡'), ("sim", '∼'), ("simeq", '≃'), ("cong", '≅'), ("propto", '∝'),
    ("ll", '≪'), ("gg", '≫'), ("to", '→'), ("rightarrow", '→'), ("leftarrow", '←'), ("Rightarrow", '⇒'), ("Leftarrow", '⇐'), ("leftrightarrow", '↔'), ("Leftrightarrow", '⇔'), ("mapsto", '↦'), ("implies", '⟹'), ("iff", '⟺'),
    ("in", '∈'), ("notin", '∉'), ("ni", '∋'), ("subset", '⊂'), ("supset", '⊃'), ("subseteq", '⊆'), ("supseteq", '⊇'), ("cup", '∪'), ("cap", '∩'), ("setminus", '∖'), ("emptyset", '∅'),
    ("forall", '∀'), ("exists", '∃'), ("neg", '¬'), ("land", '∧'), ("lor", '∨'), ("partial", '∂'), ("nabla", '∇'), ("infty", '∞'), ("hbar", 'ℏ'), ("ell", 'ℓ'), ("Re", 'ℜ'), ("Im", 'ℑ'), ("aleph", 'ℵ'),
    ("degree", '°'), ("prime", '′'), ("cdots", '⋯'), ("ldots", '…'), ("vdots", '⋮'), ("ddots", '⋱'), ("angle", '∠'), ("perp", '⊥'), ("parallel", '∥'), ("therefore", '∴'), ("because", '∵'),
];

const NARY: &[(&str, char)] = &[("sum", '∑'), ("prod", '∏'), ("coprod", '∐'), ("int", '∫'), ("iint", '∬'), ("iiint", '∭'), ("oint", '∮'), ("bigcup", '⋃'), ("bigcap", '⋂'), ("bigvee", '⋁'), ("bigwedge", '⋀')];

const FUNCS: &[&str] = &["sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "log", "ln", "lg", "exp", "lim", "max", "min", "sup", "inf", "det", "gcd", "deg", "dim", "ker", "arg", "Pr"];

const ACCENTS: &[(&str, char)] = &[("hat", '\u{302}'), ("bar", '\u{305}'), ("overline", '\u{305}'), ("vec", '\u{20D7}'), ("dot", '\u{307}'), ("ddot", '\u{308}'), ("tilde", '\u{303}')];

fn is_nary(c: char) -> bool { "∑∏∐∫∬∭∮∯∰⋃⋂⋁⋀".contains(c) }
fn is_rel(c: char) -> bool { "=<>≤≥≠≈≡∼≃≅∝≪≫→←⇒⇐↔⇔↦⟹⟺∈∉∋⊂⊃⊆⊇∣∥⊥:".contains(c) }
fn is_bin(c: char) -> bool { "+-−±∓×÷⋅∗⋆∘∙⊕⊗∪∩∖∧∨".contains(c) }

struct Parser { s: Vec<char>, i: usize }

impl Parser {
    fn peek(&self) -> Option<char> { self.s.get(self.i).copied() }
    fn skip_ws(&mut self) { while self.peek().is_some_and(|c| c == ' ') { self.i += 1; } }
    fn name(&mut self) -> String {
        let st = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) { self.i += 1; }
        if self.i == st && self.peek().is_some() { self.i += 1; }
        self.s[st..self.i].iter().collect()
    }
    /// A `{…}` group (or a single atom when there are no braces).
    fn arg(&mut self) -> Node {
        self.skip_ws();
        if self.peek() == Some('{') { self.i += 1; let r = self.row(Some('}')); if self.peek() == Some('}') { self.i += 1; } return r; }
        self.atom().unwrap_or(Node::Row(vec![]))
    }
    fn optional(&mut self) -> Option<Node> {
        if self.peek() != Some('[') { return None; }
        self.i += 1;
        let r = self.row(Some(']'));
        if self.peek() == Some(']') { self.i += 1; }
        Some(r)
    }
    /// Elements until `end` (not consumed) or the input ends.
    fn row(&mut self, end: Option<char>) -> Node {
        let mut items: Vec<Node> = vec![];
        loop {
            self.skip_ws();
            let Some(c) = self.peek() else { break };
            if Some(c) == end { break; }
            if end.is_none() && c == '}' { self.i += 1; continue; }
            // Matrix cells and \left bodies end at a closing brace or \end.
            if end == Some('\u{1}') && (c == '}' || c == '\\' && self.s[self.i + 1..].starts_with(&['e', 'n', 'd'])) { break; }
            if (c == '\\' && self.s[self.i + 1..].starts_with(&['r', 'i', 'g', 'h', 't'])) || c == '&' || (c == '\\' && self.s.get(self.i + 1) == Some(&'\\')) {
                if end.is_some() { break; }
                self.i += 1; continue;
            }
            if c == '^' || c == '_' {
                self.i += 1;
                let script = self.arg();
                let base = items.pop().unwrap_or(Node::Row(vec![]));
                items.push(attach(base, c == '^', script));
                continue;
            }
            if c == '/' && !items.is_empty() {
                // Linear fraction: the atom before over the atom after.
                self.i += 1;
                let num = items.pop().unwrap();
                let den = self.atom().unwrap_or(Node::Row(vec![]));
                items.push(Node::Frac(Box::new(strip_parens(num)), Box::new(strip_parens(den)), true));
                continue;
            }
            match self.atom() {
                Some(Node::Nary { op, sub, sup, .. }) => {
                    // Limits, then the operand (the next element).
                    let (mut sub, mut sup) = (sub, sup);
                    loop {
                        self.skip_ws();
                        match self.peek() {
                            Some('_') => { self.i += 1; sub = Some(Box::new(self.arg())); }
                            Some('^') => { self.i += 1; sup = Some(Box::new(self.arg())); }
                            _ => break,
                        }
                    }
                    let mut body = vec![];
                    self.skip_ws();
                    if let Some(b) = (self.peek().is_some_and(|c| Some(c) != end && c != '}' && !is_rel(c) && !is_bin(c))).then(|| self.atom()).flatten() {
                        let mut b = b;
                        loop {
                            match self.peek() {
                                Some('^') => { self.i += 1; let s = self.arg(); b = attach(b, true, s); }
                                Some('_') => { self.i += 1; let s = self.arg(); b = attach(b, false, s); }
                                _ => break,
                            }
                        }
                        body.push(b);
                    }
                    items.push(Node::Nary { op, sub, sup, body: Box::new(Node::Row(body)) });
                }
                Some(Node::Func { name, arg, .. }) if matches!(arg.as_ref(), Node::Row(v) if v.is_empty()) => {
                    let mut under = None;
                    if self.peek() == Some('_') { self.i += 1; under = Some(Box::new(self.arg())); }
                    self.skip_ws();
                    let arg = if self.peek().is_some_and(|c| Some(c) != end && c != '}' && !is_rel(c) && !is_bin(c)) {
                        let mut a = self.atom().unwrap_or(Node::Row(vec![]));
                        while let Some(c @ ('^' | '_')) = self.peek() { self.i += 1; let s = self.arg(); a = attach(a, c == '^', s); }
                        a
                    } else { Node::Row(vec![]) };
                    items.push(Node::Func { name, under, arg: Box::new(arg) });
                }
                Some(n) => items.push(n),
                None => {}
            }
        }
        if items.len() == 1 { items.pop().unwrap() } else { Node::Row(items) }
    }
    fn atom(&mut self) -> Option<Node> {
        self.skip_ws();
        let c = self.peek()?;
        self.i += 1;
        Some(match c {
            '{' => { let r = self.row(Some('}')); if self.peek() == Some('}') { self.i += 1; } r }
            '\\' => return self.command(),
            '(' | '[' | '|' | '⟨' | '⌊' | '⌈' | '‖' => {
                let close = match c { '(' => ')', '[' => ']', '⟨' => '⟩', '⌊' => '⌋', '⌈' => '⌉', o => o };
                let body = self.row(Some(close));
                if self.peek() == Some(close) { self.i += 1; Node::Delim { open: c, close, body: Box::new(body) } }
                else { Node::Row(vec![Node::Sym(c.to_string(), SymKind::Punct), body]) }
            }
            '√' => Node::Rad { deg: None, body: Box::new(self.arg()) },
            c if is_nary(c) => Node::Nary { op: c, sub: None, sup: None, body: Box::new(Node::Row(vec![])) },
            c if c.is_ascii_digit() || c == '.' && self.peek().is_some_and(|d| d.is_ascii_digit()) => {
                let mut t = c.to_string();
                while let Some(d) = self.peek().filter(|d| d.is_ascii_digit() || *d == '.' && self.s.get(self.i + 1).is_some_and(|e| e.is_ascii_digit())) { t.push(d); self.i += 1; }
                Node::Sym(t, SymKind::Num)
            }
            c if c.is_ascii_alphabetic() => {
                // Function names typed without a backslash are recognized as Word does.
                let st = self.i - 1;
                let mut j = st;
                while self.s.get(j).is_some_and(|c| c.is_ascii_alphabetic()) { j += 1; }
                let word: String = self.s[st..j].iter().collect();
                if let Some(f) = FUNCS.iter().filter(|f| word.starts_with(**f)).max_by_key(|f| f.len()) {
                    if word.len() == f.len() || FUNCS.contains(&f) && word.len() > f.len() && f.len() >= 3 {
                        self.i = st + f.len();
                        return Some(Node::Func { name: f.to_string(), under: None, arg: Box::new(Node::Row(vec![])) });
                    }
                }
                Node::Sym(c.to_string(), SymKind::Var)
            }
            '-' => Node::Sym("−".into(), SymKind::Op),
            '*' => Node::Sym("∗".into(), SymKind::Op),
            '\'' => Node::Sym("′".into(), SymKind::Punct),
            ',' | ';' | '!' | ')' | ']' => Node::Sym(c.to_string(), SymKind::Punct),
            c if is_rel(c) => Node::Sym(c.to_string(), SymKind::Rel),
            c if is_bin(c) => Node::Sym(c.to_string(), SymKind::Op),
            c if c.is_alphabetic() => Node::Sym(c.to_string(), SymKind::Var),
            c => Node::Sym(c.to_string(), SymKind::Punct),
        })
    }
    fn command(&mut self) -> Option<Node> {
        let name = self.name();
        let n = name.as_str();
        Some(match n {
            "frac" | "dfrac" | "tfrac" => { let a = self.arg(); let b = self.arg(); Node::Frac(Box::new(a), Box::new(b), true) }
            "binom" => { let a = self.arg(); let b = self.arg(); Node::Delim { open: '(', close: ')', body: Box::new(Node::Frac(Box::new(a), Box::new(b), false)) } }
            "sqrt" => { let deg = self.optional(); let body = self.arg(); Node::Rad { deg: deg.map(Box::new), body: Box::new(body) } }
            "left" => {
                let open = self.peek().map(|c| { self.i += 1; if c == '\\' { let n = self.name(); match n.as_str() { "{" => '{', "langle" => '⟨', "|" => '‖', "lfloor" => '⌊', "lceil" => '⌈', _ => '\0' } } else if c == '.' { '\0' } else { c } }).unwrap_or('\0');
                let body = self.row(None.or(Some('\u{1}')));
                let mut close = '\0';
                if self.peek() == Some('\\') && self.s[self.i + 1..].starts_with(&['r', 'i', 'g', 'h', 't']) {
                    self.i += 6;
                    close = self.peek().map(|c| { self.i += 1; if c == '\\' { let n = self.name(); match n.as_str() { "}" => '}', "rangle" => '⟩', "|" => '‖', "rfloor" => '⌋', "rceil" => '⌉', _ => '\0' } } else if c == '.' { '\0' } else { c } }).unwrap_or('\0');
                }
                Node::Delim { open, close, body: Box::new(body) }
            }
            "{" => { let body = self.row(Some('\\')); if self.s[self.i..].starts_with(&['\\', '}']) { self.i += 2; } Node::Delim { open: '{', close: '}', body: Box::new(body) } }
            "}" => Node::Sym("}".into(), SymKind::Punct),
            "text" | "mathrm" | "operatorname" => { let t = self.raw_group(); if n == "operatorname" { Node::Func { name: t, under: None, arg: Box::new(Node::Row(vec![])) } } else { Node::Text(t) } }
            "begin" => {
                let env = self.raw_group();
                let rows = self.matrix_rows(&env);
                let m = Node::Matrix(rows);
                match env.as_str() { "pmatrix" => Node::Delim { open: '(', close: ')', body: Box::new(m) }, "bmatrix" => Node::Delim { open: '[', close: ']', body: Box::new(m) }, "vmatrix" => Node::Delim { open: '|', close: '|', body: Box::new(m) }, "cases" => Node::Delim { open: '{', close: '\0', body: Box::new(m) }, _ => m }
            }
            "matrix" | "pmatrix" | "bmatrix" => {
                // \matrix{a & b \\ c & d}
                self.skip_ws();
                if self.peek() == Some('{') { self.i += 1; }
                let rows = self.matrix_rows("}");
                let m = Node::Matrix(rows);
                match n { "pmatrix" => Node::Delim { open: '(', close: ')', body: Box::new(m) }, "bmatrix" => Node::Delim { open: '[', close: ']', body: Box::new(m) }, _ => m }
            }
            "," | ";" | " " | "quad" | "qquad" => Node::Sym(match n { "quad" => "\u{2003}", "qquad" => "\u{2003}\u{2003}", ";" => "\u{2005}", _ => "\u{2009}" }.into(), SymKind::Punct),
            _ => {
                if let Some(&(_, c)) = GREEK.iter().find(|g| g.0 == n) { return Some(Node::Sym(c.to_string(), if c.is_uppercase() { SymKind::Num } else { SymKind::Var })); }
                if let Some(&(_, c)) = NARY.iter().find(|g| g.0 == n) { return Some(Node::Nary { op: c, sub: None, sup: None, body: Box::new(Node::Row(vec![])) }); }
                if let Some(&(_, c)) = SYMBOLS.iter().find(|g| g.0 == n) { return Some(Node::Sym(c.to_string(), if is_rel(c) { SymKind::Rel } else if is_bin(c) { SymKind::Op } else { SymKind::Num })); }
                if let Some(&(_, c)) = ACCENTS.iter().find(|g| g.0 == n) { let b = self.arg(); return Some(Node::Accent(c, Box::new(b))); }
                if FUNCS.contains(&n) { return Some(Node::Func { name: n.to_string(), under: None, arg: Box::new(Node::Row(vec![])) }); }
                Node::Text(format!("\\{n}"))
            }
        })
    }
    fn raw_group(&mut self) -> String {
        self.skip_ws();
        if self.peek() != Some('{') { return String::new(); }
        self.i += 1;
        let st = self.i;
        let mut depth = 0;
        while let Some(c) = self.peek() { if c == '{' { depth += 1; } if c == '}' { if depth == 0 { break; } depth -= 1; } self.i += 1; }
        let t: String = self.s[st..self.i].iter().collect();
        if self.peek() == Some('}') { self.i += 1; }
        t
    }
    /// Cells separated by & and rows by \\, up to \end{env} (or a closing brace).
    fn matrix_rows(&mut self, env: &str) -> Vec<Vec<Node>> {
        let mut rows = vec![vec![]];
        loop {
            let cell = self.row(Some('\u{1}'));
            rows.last_mut().unwrap().push(cell);
            match self.peek() {
                Some('&') => { self.i += 1; }
                Some('\\') if self.s.get(self.i + 1) == Some(&'\\') => { self.i += 2; rows.push(vec![]); }
                Some('\\') => {
                    // \end{…}
                    let save = self.i;
                    self.i += 1;
                    if self.name() == "end" { self.raw_group(); break; }
                    self.i = save + 1;
                }
                Some('}') if env == "}" => { self.i += 1; break; }
                None => break,
                _ => { self.i += 1; }
            }
        }
        rows.retain(|r| !r.is_empty() && !(r.len() == 1 && r[0] == Node::Row(vec![])));
        rows
    }
}

fn attach(base: Node, sup: bool, script: Node) -> Node {
    match base {
        Node::Scripts { base, sub, sup: s } if (sup && s.is_none()) || (!sup && sub.is_none()) => {
            if sup { Node::Scripts { base, sub, sup: Some(Box::new(script)) } } else { Node::Scripts { base, sub: Some(Box::new(script)), sup: s } }
        }
        Node::Nary { op, sub, sup: s, body } if (sup && s.is_none()) || (!sup && sub.is_none()) => {
            if sup { Node::Nary { op, sub, sup: Some(Box::new(script)), body } } else { Node::Nary { op, sub: Some(Box::new(script)), sup: s, body } }
        }
        b => if sup { Node::Scripts { base: Box::new(b), sub: None, sup: Some(Box::new(script)) } } else { Node::Scripts { base: Box::new(b), sub: Some(Box::new(script)), sup: None } },
    }
}

/// `(a+b)/c`: the parentheses of a linear fraction's operands are grouping, not shown.
fn strip_parens(n: Node) -> Node { match n { Node::Delim { open: '(', close: ')', body } => *body, n => n } }

pub fn parse(src: &str) -> Node {
    let mut p = Parser { s: src.chars().collect(), i: 0 };
    p.row(None)
}

// ---------------------------------------------------------------------------
// Back to linear text (OMML read, editing an equation)

pub fn to_linear(n: &Node) -> String {
    let grp = |n: &Node| { let t = to_linear(n); if t.chars().count() == 1 { t } else { format!("{{{t}}}") } };
    match n {
        Node::Row(v) => {
            let mut out = String::new();
            for x in v {
                let t = to_linear(x);
                if out.ends_with(|c: char| c.is_ascii_alphabetic()) && t.starts_with(|c: char| c.is_ascii_alphabetic()) && out.contains('\\') { out.push(' '); }
                out += &t;
            }
            out
        }
        Node::Sym(s, _) => {
            if let Some((name, _)) = GREEK.iter().chain(SYMBOLS.iter()).find(|(_, c)| s.chars().count() == 1 && s.starts_with(*c)) { return format!("\\{name} "); }
            match s.as_str() { "−" => "-".into(), "∗" => "*".into(), "′" => "'".into(), "\u{2009}" => "\\,".into(), "\u{2003}" => "\\quad ".into(), _ => s.clone() }
        }
        Node::Frac(a, b, true) => format!("\\frac{{{}}}{{{}}}", to_linear(a), to_linear(b)),
        Node::Frac(a, b, false) => format!("\\binom{{{}}}{{{}}}", to_linear(a), to_linear(b)),
        Node::Scripts { base, sub, sup } => format!("{}{}{}", grp(base), sub.as_ref().map(|s| format!("_{}", grp(s))).unwrap_or_default(), sup.as_ref().map(|s| format!("^{}", grp(s))).unwrap_or_default()),
        Node::Rad { deg, body } => format!("\\sqrt{}{{{}}}", deg.as_ref().map(|d| format!("[{}]", to_linear(d))).unwrap_or_default(), to_linear(body)),
        Node::Nary { op, sub, sup, body } => {
            let name = NARY.iter().find(|(_, c)| c == op).map(|(n, _)| format!("\\{n}")).unwrap_or(op.to_string());
            format!("{name}{}{} {}", sub.as_ref().map(|s| format!("_{}", grp(s))).unwrap_or_default(), sup.as_ref().map(|s| format!("^{}", grp(s))).unwrap_or_default(), to_linear(body))
        }
        Node::Delim { open, close, body } => {
            if let (Node::Frac(a, b, false), '(', ')') = (body.as_ref(), open, close) { return format!("\\binom{{{}}}{{{}}}", to_linear(a), to_linear(b)); }
            if let Node::Matrix(rows) = body.as_ref() {
                let env = match (open, close) { ('(', ')') => "pmatrix", ('[', ']') => "bmatrix", ('|', '|') => "vmatrix", ('{', '\0') => "cases", _ => "matrix" };
                return format!("\\begin{{{env}}}{}\\end{{{env}}}", matrix_linear(rows));
            }
            let side = |c: char, left: bool| match c { '\0' => ".".to_string(), '{' => "\\{".into(), '}' => "\\}".into(), '⟨' => "\\langle ".into(), '⟩' => "\\rangle ".into(), c => { let _ = left; c.to_string() } };
            if matches!((open, close), ('(', ')') | ('[', ']') | ('|', '|')) { format!("{open}{}{close}", to_linear(body)) }
            else { format!("\\left{}{}\\right{}", side(*open, true), to_linear(body), side(*close, false)) }
        }
        Node::Func { name, under, arg } => {
            let f = if FUNCS.contains(&name.as_str()) { format!("\\{name}") } else { format!("\\operatorname{{{name}}}") };
            format!("{f}{} {}", under.as_ref().map(|u| format!("_{}", grp(u))).unwrap_or_default(), to_linear(arg))
        }
        Node::Matrix(rows) => format!("\\begin{{matrix}}{}\\end{{matrix}}", matrix_linear(rows)),
        Node::Accent(c, b) => format!("\\{}{{{}}}", ACCENTS.iter().find(|a| a.1 == *c).map(|a| a.0).unwrap_or("hat"), to_linear(b)),
        Node::Text(t) => if t.starts_with('\\') { t.clone() } else { format!("\\text{{{t}}}") },
    }
}

fn matrix_linear(rows: &[Vec<Node>]) -> String { rows.iter().map(|r| r.iter().map(to_linear).collect::<Vec<_>>().join("&")).collect::<Vec<_>>().join("\\\\") }

// ---------------------------------------------------------------------------
// Typesetting

/// A typeset box: width, height above and depth below the baseline, and its drawing with the
/// baseline's left end at (0, 0).
#[derive(Clone, Debug, Default)]
pub struct MBox { pub w: f32, pub asc: f32, pub desc: f32, pub items: Vec<DrawItem> }

fn shift(items: &mut [DrawItem], dx: f32, dy: f32) {
    for it in items {
        match it {
            DrawItem::Glyphs { glyphs, baseline, .. } => { *baseline += dy; for g in glyphs { g.x += dx; } }
            DrawItem::Path { points, .. } => for p in points { p.0 += dx; p.1 += dy; },
            DrawItem::Rect { x, y, .. } | DrawItem::Image { x, y, .. } | DrawItem::Frame { x, y, .. } => { *x += dx; *y += dy; }
            DrawItem::Line { x1, y1, x2, y2, .. } => { *x1 += dx; *x2 += dx; *y1 += dy; *y2 += dy; }
        }
    }
}

impl MBox {
    fn place(&mut self, mut b: MBox, dx: f32, dy: f32) { shift(&mut b.items, dx, dy); self.items.extend(b.items); }
}

/// Math italic for Latin and Greek letters (Unicode Mathematical Alphanumeric Symbols).
fn italic(c: char) -> char {
    match c {
        'h' => 'ℎ',
        'a'..='z' => char::from_u32(0x1D44E + c as u32 - 'a' as u32).unwrap_or(c),
        'A'..='Z' => char::from_u32(0x1D434 + c as u32 - 'A' as u32).unwrap_or(c),
        'α'..='ω' => char::from_u32(0x1D6FC + c as u32 - 'α' as u32).unwrap_or(c),
        'ϵ' => '𝜖', 'ϑ' => '𝜗', 'ϕ' => '𝜙',
        c => c,
    }
}

struct Ctx { size: f32, color: Color }

impl Ctx {
    fn em(&self) -> f32 { self.size * 4.0 / 3.0 }
    fn axis(&self) -> f32 { self.em() * 0.25 }
    fn rule(&self) -> f32 { (self.em() * 0.055).max(0.6) }
    fn smaller(&self, k: f32) -> Ctx { Ctx { size: (self.size * k).max(5.0), color: self.color } }
    fn text(&self, t: &str, upright: bool) -> MBox {
        let t: String = if upright { t.to_string() } else { t.chars().map(italic).collect() };
        let (items, w) = crate::objects::text_line(&t, FONT, false, self.size, self.color, 0.0, 0.0);
        let em = self.em();
        // Glyph extents from the font's metrics would be exact; letters span about this much.
        let tall = t.chars().any(|c| c.is_uppercase() || c.is_ascii_digit() || "bdfhklt()[]{}|∂ℓβδζθλξ∫∑∏√".contains(c) || (c as u32) >= 0x1D400);
        let deep = t.chars().any(|c| "gjpqy()[]{}|,;βγζημξρφχψ∫∑∏".contains(c) || matches!(c as u32, 0x1D454 | 0x1D457 | 0x1D45D | 0x1D45E | 0x1D466));
        MBox { w, asc: if tall { em * 0.72 } else { em * 0.5 }, desc: if deep { em * 0.22 } else { em * 0.02 }, items }
    }
}

fn layout_node(n: &Node, c: &Ctx, display: bool) -> MBox {
    match n {
        Node::Row(v) => {
            let mut out = MBox::default();
            for (k, x) in v.iter().enumerate() {
                // Spacing around binary operators and relations (none for a leading sign).
                let (before, after) = match x {
                    Node::Sym(_, SymKind::Rel) => (0.28, 0.28),
                    Node::Sym(_, SymKind::Op) if k == 0 || matches!(v.get(k - 1), Some(Node::Sym(_, SymKind::Rel | SymKind::Op | SymKind::Punct))) => (0.0, 0.0),
                    Node::Sym(_, SymKind::Op) => (0.22, 0.22),
                    Node::Sym(s, SymKind::Punct) if s == "," || s == ";" => (0.0, 0.17),
                    _ => (0.0, 0.0),
                };
                out.w += c.em() * before;
                let b = layout_node(x, c, display);
                let w = b.w;
                out.asc = out.asc.max(b.asc);
                out.desc = out.desc.max(b.desc);
                let x0 = out.w;
                out.place(b, x0, 0.0);
                out.w += w + c.em() * after;
            }
            if v.is_empty() { out.asc = c.em() * 0.5; }
            out
        }
        Node::Sym(s, kind) => c.text(s, *kind != SymKind::Var),
        Node::Text(t) => c.text(t, true),
        Node::Frac(a, b, bar) => {
            let sub = if display { c.smaller(1.0) } else { c.smaller(0.85) };
            let (na, nb) = (layout_node(a, &sub, false), layout_node(b, &sub, false));
            let (t, gap, pad) = (c.rule(), c.em() * 0.12, c.em() * 0.12);
            let w = na.w.max(nb.w) + 2.0 * pad;
            let ax = c.axis();
            let mut out = MBox { w, asc: ax + t / 2.0 + gap + na.desc + na.asc, desc: (nb.asc + nb.desc + gap + t / 2.0 - ax).max(0.0), items: vec![] };
            let (nw, nbw, nad, nba) = (na.w, nb.w, na.desc, nb.asc);
            out.place(na, (w - nw) / 2.0, -ax - t / 2.0 - gap - nad);
            out.place(nb, (w - nbw) / 2.0, -ax + t / 2.0 + gap + nba);
            if *bar { out.items.push(DrawItem::Line { x1: pad * 0.5, y1: -ax, x2: w - pad * 0.5, y2: -ax, width: t, color: c.color }); }
            out
        }
        Node::Scripts { base, sub, sup } => {
            let b = layout_node(base, c, display);
            let small = c.smaller(0.7);
            let (bw, basc, bdesc) = (b.w, b.asc, b.desc);
            let mut out = MBox { w: bw, asc: basc, desc: bdesc, items: vec![] };
            out.place(b, 0.0, 0.0);
            let italic_kern = if matches!(base.as_ref(), Node::Sym(_, SymKind::Var)) { c.em() * 0.04 } else { 0.0 };
            let mut extra: f32 = 0.0;
            if let Some(s) = sup {
                let sb = layout_node(s, &small, false);
                let up = (basc - sb.asc * 0.55).max(c.em() * 0.38);
                out.asc = out.asc.max(up + sb.asc);
                extra = extra.max(sb.w + italic_kern);
                let sw = sb.w;
                out.place(sb, bw + italic_kern, -up);
                let _ = sw;
            }
            if let Some(s) = sub {
                let sb = layout_node(s, &small, false);
                let down = (bdesc + sb.asc * 0.4).max(c.em() * 0.2);
                out.desc = out.desc.max(down + sb.desc);
                extra = extra.max(sb.w);
                out.place(sb, bw, down);
            }
            out.w = bw + extra + c.em() * 0.05;
            out
        }
        Node::Rad { deg, body } => {
            let b = layout_node(body, c, display);
            let (t, gap) = (c.rule(), c.em() * 0.1);
            let sign_w = c.em() * 0.55;
            let top = -(b.asc + gap + t);
            let bottom = b.desc + c.em() * 0.05;
            let mut out = MBox { w: 0.0, asc: -top + t, desc: bottom, items: vec![] };
            let mut x0 = 0.0;
            if let Some(d) = deg {
                let db = layout_node(d, &c.smaller(0.55), false);
                let dw = db.w;
                let h = db.asc + db.desc;
                out.asc = out.asc.max(-top * 0.45 + h);
                out.place(db, 0.0, top * 0.45);
                x0 = (dw - sign_w * 0.4).max(0.0);
            }
            let mid = (top + bottom) * 0.35;
            let pts = vec![(x0, mid + c.em() * 0.05), (x0 + sign_w * 0.25, mid - c.em() * 0.04), (x0 + sign_w * 0.55, bottom), (x0 + sign_w, top), (x0 + sign_w + b.w + gap, top)];
            out.items.push(DrawItem::Path { points: pts, closed: false, fill: None, stroke: Some((c.color, t)), ink: None });
            let bw = b.w;
            out.place(b, x0 + sign_w + gap * 0.5, 0.0);
            out.w = x0 + sign_w + bw + gap * 1.5;
            out
        }
        Node::Nary { op, sub, sup, body } => {
            let limits_over = display && !"∫∬∭∮∯∰".contains(*op);
            let big = c.smaller(if display { 1.6 } else { 1.15 });
            let mut o = big.text(&op.to_string(), true);
            // Large operators sit centered on the math axis.
            let h = o.asc + o.desc;
            let dy = -c.axis() - (o.asc - h / 2.0) + o.asc - h / 2.0;
            let center = -c.axis() + (o.asc - o.desc) / 2.0;
            shift(&mut o.items, 0.0, center + dy * 0.0);
            o.asc = c.axis() + h / 2.0;
            o.desc = h / 2.0 - c.axis();
            let small = c.smaller(0.7);
            let (sb, pb) = (sub.as_ref().map(|s| layout_node(s, &small, false)), sup.as_ref().map(|s| layout_node(s, &small, false)));
            let mut out = MBox::default();
            let ow = o.w;
            if limits_over {
                let w = ow.max(sb.as_ref().map(|b| b.w).unwrap_or(0.0)).max(pb.as_ref().map(|b| b.w).unwrap_or(0.0));
                let (oasc, odesc) = (o.asc, o.desc);
                out.asc = oasc; out.desc = odesc;
                out.place(o, (w - ow) / 2.0, 0.0);
                if let Some(p) = pb { let y = -oasc - c.em() * 0.08 - p.desc; out.asc = out.asc.max(-y + p.asc); let pw = p.w; out.place(p, (w - pw) / 2.0, y); }
                if let Some(s) = sb { let y = odesc + c.em() * 0.08 + s.asc; out.desc = out.desc.max(y + s.desc); let sw = s.w; out.place(s, (w - sw) / 2.0, y); }
                out.w = w;
            } else {
                let (oasc, odesc) = (o.asc, o.desc);
                out.asc = oasc; out.desc = odesc;
                out.place(o, 0.0, 0.0);
                let mut extra: f32 = 0.0;
                if let Some(p) = pb { let y = -oasc + p.asc * 0.7; out.asc = out.asc.max(-y + p.asc); extra = extra.max(p.w); out.place(p, ow + c.em() * 0.05, y); }
                if let Some(s) = sb { let y = odesc - s.desc; out.desc = out.desc.max(y + s.desc); extra = extra.max(s.w); out.place(s, ow - c.em() * 0.1, y); }
                out.w = ow + extra + c.em() * 0.05;
            }
            let b = layout_node(body, c, display);
            let x = out.w + c.em() * 0.12;
            out.asc = out.asc.max(b.asc);
            out.desc = out.desc.max(b.desc);
            let bw = b.w;
            out.place(b, x, 0.0);
            out.w = x + bw;
            out
        }
        Node::Delim { open, close, body } => {
            let b = layout_node(body, c, display);
            // Delimiters stretch symmetrically about the axis to cover the body.
            let ax = c.axis();
            let half = (b.asc - ax).max(b.desc + ax).max(c.em() * 0.5) + c.em() * 0.08;
            let (top, bot) = (-ax - half, -ax + half);
            let dw = c.em() * 0.32;
            let mut out = MBox { w: 0.0, asc: ax + half, desc: (half - ax).max(0.0), items: vec![] };
            let mut x = 0.0;
            if *open != '\0' { out.items.extend(delimiter(*open, x, top, bot, dw, c)); x += dw; }
            let bw = b.w;
            out.place(b, x + c.em() * 0.04, 0.0);
            x += bw + c.em() * 0.08;
            if *close != '\0' { out.items.extend(delimiter(*close, x, top, bot, dw, c)); x += dw; }
            out.w = x;
            out
        }
        Node::Func { name, under, arg } => {
            let f = c.text(name, true);
            let mut out = MBox::default();
            let (fw, fasc, fdesc) = (f.w, f.asc, f.desc);
            out.asc = fasc; out.desc = fdesc;
            let mut w = fw;
            if let Some(u) = under {
                let ub = layout_node(u, &c.smaller(0.7), false);
                let uw = ub.w;
                w = fw.max(uw);
                let y = fdesc + c.em() * 0.08 + ub.asc;
                out.desc = out.desc.max(y + ub.desc);
                out.place(f, (w - fw) / 2.0, 0.0);
                out.place(ub, (w - uw) / 2.0, y);
            } else { out.place(f, 0.0, 0.0); }
            let a = layout_node(arg, c, display);
            let x = w + c.em() * 0.17;
            out.asc = out.asc.max(a.asc); out.desc = out.desc.max(a.desc);
            let aw = a.w;
            out.place(a, x, 0.0);
            out.w = x + aw;
            out
        }
        Node::Matrix(rows) => {
            let cells: Vec<Vec<MBox>> = rows.iter().map(|r| r.iter().map(|n| layout_node(n, c, false)).collect()).collect();
            let cols = cells.iter().map(|r| r.len()).max().unwrap_or(0);
            let colw: Vec<f32> = (0..cols).map(|k| cells.iter().filter_map(|r| r.get(k)).map(|b| b.w).fold(0.0, f32::max)).collect();
            let (cg, rg) = (c.em() * 0.8, c.em() * 0.25);
            let heights: Vec<(f32, f32)> = cells.iter().map(|r| (r.iter().map(|b| b.asc).fold(c.em() * 0.5, f32::max), r.iter().map(|b| b.desc).fold(0.0, f32::max))).collect();
            let total: f32 = heights.iter().map(|h| h.0 + h.1).sum::<f32>() + rg * (heights.len().saturating_sub(1)) as f32;
            let top = -c.axis() - total / 2.0;
            let mut out = MBox { w: colw.iter().sum::<f32>() + cg * (cols.saturating_sub(1)) as f32, asc: c.axis() + total / 2.0, desc: (total / 2.0 - c.axis()).max(0.0), items: vec![] };
            let mut y = top;
            for (r, row) in cells.into_iter().enumerate() {
                let base = y + heights[r].0;
                let mut x = 0.0;
                for (k, b) in row.into_iter().enumerate() { let bw = b.w; out.place(b, x + (colw[k] - bw) / 2.0, base); x += colw[k] + cg; }
                y = base + heights[r].1 + rg;
            }
            out
        }
        Node::Accent(a, body) => {
            let b = layout_node(body, c, display);
            let (bw, basc) = (b.w, b.asc);
            let mut out = MBox { w: bw, asc: basc + c.em() * 0.25, desc: b.desc, items: vec![] };
            out.place(b, 0.0, 0.0);
            let y = -basc - c.em() * 0.1;
            let t = c.rule();
            let (x0, x1, mid) = (bw * 0.1, bw * 0.9, bw / 2.0);
            let e = c.em();
            match a {
                '\u{305}' => out.items.push(DrawItem::Line { x1: 0.0, y1: y, x2: bw, y2: y, width: t, color: c.color }),
                '\u{20D7}' => {
                    out.items.push(DrawItem::Line { x1: x0, y1: y, x2: x1, y2: y, width: t, color: c.color });
                    out.items.push(DrawItem::Path { points: vec![(x1 - e * 0.12, y - e * 0.07), (x1, y), (x1 - e * 0.12, y + e * 0.07)], closed: false, fill: None, stroke: Some((c.color, t)), ink: None });
                }
                '\u{302}' => out.items.push(DrawItem::Path { points: vec![(mid - e * 0.18, y + e * 0.05), (mid, y - e * 0.08), (mid + e * 0.18, y + e * 0.05)], closed: false, fill: None, stroke: Some((c.color, t)), ink: None }),
                '\u{303}' => out.items.push(DrawItem::Path { points: vec![(mid - e * 0.2, y), (mid - e * 0.1, y - e * 0.06), (mid + e * 0.1, y + e * 0.03), (mid + e * 0.2, y - e * 0.03)], closed: false, fill: None, stroke: Some((c.color, t)), ink: None }),
                _ => {
                    let dots = if *a == '\u{308}' { vec![mid - e * 0.1, mid + e * 0.1] } else { vec![mid] };
                    for d in dots { out.items.push(DrawItem::Path { points: (0..8).map(|k| { let ang = k as f32 * std::f32::consts::TAU / 8.0; (d + ang.cos() * e * 0.035, y + ang.sin() * e * 0.035) }).collect(), closed: true, fill: Some(c.color), stroke: None, ink: None }); }
                }
            }
            out
        }
    }
}

/// A stretchy delimiter from `top` to `bot` drawn as a stroke, `dw` wide, starting at x.
fn delimiter(ch: char, x: f32, top: f32, bot: f32, dw: f32, c: &Ctx) -> Vec<DrawItem> {
    let t = c.rule() * 1.3;
    let (l, r) = (x + dw * 0.25, x + dw * 0.75);
    let mid = (top + bot) / 2.0;
    let h = bot - top;
    let stroke = |pts: Vec<(f32, f32)>| DrawItem::Path { points: pts, closed: false, fill: None, stroke: Some((c.color, t)), ink: None };
    let arc = |from: f32, to: f32, cx: f32, rx: f32| -> Vec<(f32, f32)> { (0..=16).map(|k| { let a = from + (to - from) * k as f32 / 16.0; (cx + rx * a.cos(), mid + h / 2.0 * a.sin()) }).collect() };
    use std::f32::consts::FRAC_PI_2;
    match ch {
        '(' => vec![stroke(arc(-FRAC_PI_2 * 1.0 - 0.0, -FRAC_PI_2 * 3.0, r + dw * 0.1, dw * 0.55).into_iter().map(|(px, py)| (px, py)).collect())],
        ')' => vec![stroke(arc(-FRAC_PI_2, FRAC_PI_2, l - dw * 0.1, dw * 0.55))],
        '[' => vec![stroke(vec![(r, top), (l, top), (l, bot), (r, bot)])],
        ']' => vec![stroke(vec![(l, top), (r, top), (r, bot), (l, bot)])],
        '⌊' => vec![stroke(vec![(l, top), (l, bot), (r, bot)])],
        '⌋' => vec![stroke(vec![(r, top), (r, bot), (l, bot)])],
        '⌈' => vec![stroke(vec![(r, top), (l, top), (l, bot)])],
        '⌉' => vec![stroke(vec![(l, top), (r, top), (r, bot)])],
        '|' => vec![stroke(vec![((l + r) / 2.0, top), ((l + r) / 2.0, bot)])],
        '‖' => vec![stroke(vec![(l + dw * 0.08, top), (l + dw * 0.08, bot)]), stroke(vec![(r - dw * 0.08, top), (r - dw * 0.08, bot)])],
        '⟨' => vec![stroke(vec![(r, top), (l, mid), (r, bot)])],
        '⟩' => vec![stroke(vec![(l, top), (r, mid), (l, bot)])],
        '{' | '}' => {
            let s = if ch == '{' { 1.0 } else { -1.0 };
            let (a, b, m) = if ch == '{' { (r, (l + r) / 2.0, l) } else { (l, (l + r) / 2.0, r) };
            let q = h * 0.06;
            let _ = s;
            vec![stroke(vec![(a, top), (b + (a - b) * 0.3, top + q), (b, top + 2.0 * q), (b, mid - 2.0 * q), (b - (b - m) * 0.4, mid - q * 0.6), (m, mid), (b - (b - m) * 0.4, mid + q * 0.6), (b, mid + 2.0 * q), (b, bot - 2.0 * q), (b + (a - b) * 0.3, bot - q), (a, bot)])]
        }
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// The object

/// An equation in a document: its linear source, font size and color. Display equations stand
/// on their own line (m:oMathPara).
#[derive(Clone, Debug, PartialEq)]
pub struct Equation { pub linear: String, pub size: f32, pub color: Color, pub display: bool }

impl Equation {
    pub fn new(linear: &str) -> Equation { Equation { linear: linear.to_string(), size: 11.0, color: Color::BLACK, display: false } }
    pub fn typeset(&self) -> MBox {
        let tree = parse(&self.linear);
        let mut b = layout_node(&tree, &Ctx { size: self.size, color: self.color }, self.display);
        b.w = b.w.max(4.0);
        b.asc = b.asc.max(self.size * 4.0 / 3.0 * 0.72);
        b.desc = b.desc.max(self.size * 4.0 / 3.0 * 0.22);
        b
    }
    /// (width, height, depth below the baseline).
    pub fn metrics(&self) -> (f32, f32, f32) { let b = self.typeset(); (b.w, b.asc + b.desc, b.desc) }
    /// Draw with the box's top-left at (x, y).
    pub fn render(&self, x: f32, y: f32) -> Vec<DrawItem> {
        let mut b = self.typeset();
        let asc = b.asc;
        shift(&mut b.items, x, y + asc);
        b.items
    }
}

/// Built-in equations (Insert ▸ Equation gallery).
pub const GALLERY: &[(&str, &str)] = &[
    ("Area of Circle", "A=\\pi r^2"),
    ("Binomial Theorem", "(x+a)^n=\\sum_{k=0}^{n}\\binom{n}{k}x^k a^{n-k}"),
    ("Expansion of a Sum", "(1+x)^n=1+\\frac{nx}{1!}+\\frac{n(n-1)x^2}{2!}+\\cdots"),
    ("Fourier Series", "f(x)=a_0+\\sum_{n=1}^{\\infty}\\left(a_n\\cos\\frac{n\\pi x}{L}+b_n\\sin\\frac{n\\pi x}{L}\\right)"),
    ("Pythagorean Theorem", "a^2+b^2=c^2"),
    ("Quadratic Formula", "x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}"),
    ("Taylor Expansion", "e^x=1+\\frac{x}{1!}+\\frac{x^2}{2!}+\\frac{x^3}{3!}+\\cdots,\\quad -\\infty<x<\\infty"),
    ("Trig Identity 1", "\\sin\\alpha\\pm\\sin\\beta=2\\sin\\frac{1}{2}(\\alpha\\pm\\beta)\\cos\\frac{1}{2}(\\alpha\\mp\\beta)"),
    ("Integral", "\\int_0^1 x^2\\,dx=\\frac{1}{3}"),
    ("Limit", "\\lim_{n\\to\\infty}\\left(1+\\frac{1}{n}\\right)^n=e"),
    ("Matrix", "A=\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}"),
];

// ---------------------------------------------------------------------------
// OMML

fn esc(s: &str) -> String { let mut o = String::new(); crate::xml::escape_into(s, &mut o, false); o }

fn omml_run(text: &str, upright: bool) -> String {
    // Upright text in math is m:sty p (plain); variables keep the default italic.
    let sty = if upright { r#"<m:rPr><m:sty m:val="p"/></m:rPr>"# } else { "" };
    format!(r#"<m:r>{sty}<w:rPr><w:rFonts w:ascii="Cambria Math" w:hAnsi="Cambria Math"/></w:rPr><m:t xml:space="preserve">{}</m:t></m:r>"#, esc(text))
}

fn omml_e(tag: &str, n: Option<&Node>) -> String { format!("<m:{tag}>{}</m:{tag}>", n.map(to_omml).unwrap_or_default()) }

pub fn to_omml(n: &Node) -> String {
    match n {
        Node::Row(v) => v.iter().map(to_omml).collect(),
        Node::Sym(s, k) => omml_run(s, *k != SymKind::Var),
        Node::Text(t) => format!(r#"<m:r><m:rPr><m:nor/></m:rPr><w:rPr><w:rFonts w:ascii="Cambria Math" w:hAnsi="Cambria Math"/></w:rPr><m:t xml:space="preserve">{}</m:t></m:r>"#, esc(t)),
        Node::Frac(a, b, bar) => format!("<m:f>{}{}{}</m:f>", if *bar { String::new() } else { r#"<m:fPr><m:type m:val="noBar"/></m:fPr>"#.into() }, omml_e("num", Some(a)), omml_e("den", Some(b))),
        Node::Scripts { base, sub: Some(s), sup: Some(p) } => format!("<m:sSubSup>{}{}{}</m:sSubSup>", omml_e("e", Some(base)), omml_e("sub", Some(s)), omml_e("sup", Some(p))),
        Node::Scripts { base, sub: Some(s), sup: None } => format!("<m:sSub>{}{}</m:sSub>", omml_e("e", Some(base)), omml_e("sub", Some(s))),
        Node::Scripts { base, sub: None, sup: Some(p) } => format!("<m:sSup>{}{}</m:sSup>", omml_e("e", Some(base)), omml_e("sup", Some(p))),
        Node::Scripts { base, .. } => to_omml(base),
        Node::Rad { deg, body } => format!("<m:rad>{}{}{}</m:rad>", if deg.is_none() { r#"<m:radPr><m:degHide m:val="1"/></m:radPr>"# } else { "" }, omml_e("deg", deg.as_deref()), omml_e("e", Some(body))),
        Node::Nary { op, sub, sup, body } => {
            let loc = if "∫∬∭∮∯∰".contains(*op) { "subSup" } else { "undOvr" };
            let hide = format!("{}{}", if sub.is_none() { r#"<m:subHide m:val="1"/>"# } else { "" }, if sup.is_none() { r#"<m:supHide m:val="1"/>"# } else { "" });
            format!(r#"<m:nary><m:naryPr><m:chr m:val="{op}"/><m:limLoc m:val="{loc}"/>{hide}</m:naryPr>{}{}{}</m:nary>"#, omml_e("sub", sub.as_deref()), omml_e("sup", sup.as_deref()), omml_e("e", Some(body)))
        }
        Node::Delim { open, close, body } => {
            let side = |c: char| if c == '\0' { String::new() } else { c.to_string() };
            let pr = if (*open, *close) == ('(', ')') { String::new() } else { format!(r#"<m:dPr><m:begChr m:val="{}"/><m:endChr m:val="{}"/></m:dPr>"#, esc(&side(*open)), esc(&side(*close))) };
            format!("<m:d>{pr}{}</m:d>", omml_e("e", Some(body)))
        }
        Node::Func { name, under, arg } => {
            let fname = match under { Some(u) => format!("<m:limLow>{}{}</m:limLow>", format!("<m:e>{}</m:e>", omml_run(name, true)), omml_e("lim", Some(u))), None => omml_run(name, true) };
            format!("<m:func><m:fName>{fname}</m:fName>{}</m:func>", omml_e("e", Some(arg)))
        }
        Node::Matrix(rows) => format!("<m:m>{}</m:m>", rows.iter().map(|r| format!("<m:mr>{}</m:mr>", r.iter().map(|c| omml_e("e", Some(c))).collect::<String>())).collect::<String>()),
        Node::Accent(a, b) => if *a == '\u{305}' { format!(r#"<m:bar><m:barPr><m:pos m:val="top"/></m:barPr>{}</m:bar>"#, omml_e("e", Some(b))) } else { format!(r#"<m:acc><m:accPr><m:chr m:val="{a}"/></m:accPr>{}</m:acc>"#, omml_e("e", Some(b))) },
    }
}

/// An `m:oMath` element for the equation.
pub fn omml(eq: &Equation) -> String { format!("<m:oMath>{}</m:oMath>", to_omml(&parse(&eq.linear))) }

fn kids(e: &Element) -> impl Iterator<Item = &Element> { e.children.iter().filter_map(|n| if let XNode::Element(x) = n { Some(x) } else { None }) }
fn val(e: Option<&Element>) -> Option<String> { e.and_then(|x| x.attr("m:val")).map(String::from) }

/// The equation tree of an m:oMath (or any OMML container's children).
pub fn from_omml(e: &Element) -> Node {
    let items: Vec<Node> = kids(e).filter_map(omml_node).collect();
    if items.len() == 1 { items.into_iter().next().unwrap() } else { Node::Row(items) }
}

fn child_node(e: &Element, tag: &str) -> Option<Node> { e.child(tag).map(from_omml).filter(|n| *n != Node::Row(vec![])) }

fn omml_node(e: &Element) -> Option<Node> {
    let b = |n: Option<Node>| Box::new(n.unwrap_or(Node::Row(vec![])));
    Some(match e.name.as_str() {
        "m:r" => {
            let t: String = kids(e).filter(|k| k.name == "m:t").map(|k| k.text()).collect();
            let rpr = e.child("m:rPr");
            if rpr.is_some_and(|r| r.child("m:nor").is_some()) { return Some(Node::Text(t)); }
            let upright = rpr.and_then(|r| r.child("m:sty")).and_then(|s| s.attr("m:val")).is_some_and(|v| v == "p");
            // Split into symbols so operators get their spacing.
            let mut v = vec![];
            let mut num = String::new();
            for c in t.chars() {
                if c.is_ascii_digit() || c == '.' && !num.is_empty() { num.push(c); continue; }
                if !num.is_empty() { v.push(Node::Sym(std::mem::take(&mut num), SymKind::Num)); }
                let kind = if is_rel(c) { SymKind::Rel } else if is_bin(c) { SymKind::Op } else if c.is_alphabetic() && !upright { SymKind::Var } else if c.is_alphabetic() { SymKind::Num } else { SymKind::Punct };
                v.push(Node::Sym(if c == '-' { "−".into() } else { c.to_string() }, kind));
            }
            if !num.is_empty() { v.push(Node::Sym(num, SymKind::Num)); }
            if upright && v.iter().all(|n| matches!(n, Node::Sym(s, _) if s.chars().all(|c| c.is_alphabetic()))) && v.len() > 1 { return Some(Node::Text(t)); }
            if v.len() == 1 { v.pop().unwrap() } else { Node::Row(v) }
        }
        "m:f" => Node::Frac(b(child_node(e, "m:num")), b(child_node(e, "m:den")), val(e.child("m:fPr").and_then(|p| p.child("m:type"))).as_deref() != Some("noBar")),
        "m:sSup" => Node::Scripts { base: b(child_node(e, "m:e")), sub: None, sup: Some(b(child_node(e, "m:sup"))) },
        "m:sSub" => Node::Scripts { base: b(child_node(e, "m:e")), sub: Some(b(child_node(e, "m:sub"))), sup: None },
        "m:sSubSup" => Node::Scripts { base: b(child_node(e, "m:e")), sub: Some(b(child_node(e, "m:sub"))), sup: Some(b(child_node(e, "m:sup"))) },
        "m:rad" => Node::Rad { deg: child_node(e, "m:deg").map(Box::new), body: b(child_node(e, "m:e")) },
        "m:nary" => {
            let op = val(e.child("m:naryPr").and_then(|p| p.child("m:chr"))).and_then(|s| s.chars().next()).unwrap_or('∫');
            Node::Nary { op, sub: child_node(e, "m:sub").map(Box::new), sup: child_node(e, "m:sup").map(Box::new), body: b(child_node(e, "m:e")) }
        }
        "m:d" => {
            let pr = e.child("m:dPr");
            let side = |tag: &str, def: char| match pr.and_then(|p| p.child(tag)) { Some(x) => x.attr("m:val").and_then(|v| v.chars().next()).unwrap_or('\0'), None => def };
            let es: Vec<Node> = kids(e).filter(|k| k.name == "m:e").map(from_omml).collect();
            let body = if es.len() == 1 { es.into_iter().next().unwrap() } else {
                let mut v = vec![];
                for (k, x) in es.into_iter().enumerate() { if k > 0 { v.push(Node::Sym(",".into(), SymKind::Punct)); } v.push(x); }
                Node::Row(v)
            };
            Node::Delim { open: side("m:begChr", '('), close: side("m:endChr", ')'), body: Box::new(body) }
        }
        "m:func" => {
            let fname = e.child("m:fName");
            let (name, under) = match fname.and_then(|f| f.child("m:limLow")) {
                Some(l) => (l.child("m:e").map(|x| x.text()).unwrap_or_default(), child_node(l, "m:lim").map(Box::new)),
                None => (fname.map(|f| f.text()).unwrap_or_default(), None),
            };
            Node::Func { name: name.trim().to_string(), under, arg: b(child_node(e, "m:e")) }
        }
        "m:limLow" => Node::Func { name: e.child("m:e").map(|x| x.text()).unwrap_or_default(), under: child_node(e, "m:lim").map(Box::new), arg: b(None) },
        "m:m" => Node::Matrix(kids(e).filter(|k| k.name == "m:mr").map(|r| kids(r).filter(|k| k.name == "m:e").map(from_omml).collect()).collect()),
        "m:acc" => Node::Accent(val(e.child("m:accPr").and_then(|p| p.child("m:chr"))).and_then(|s| s.chars().next()).unwrap_or('\u{302}'), b(child_node(e, "m:e"))),
        "m:bar" => Node::Accent('\u{305}', b(child_node(e, "m:e"))),
        "m:box" | "m:borderBox" | "m:phant" | "m:groupChr" => from_omml(e.child("m:e")?),
        "m:sPre" => Node::Row(vec![Node::Scripts { base: Box::new(Node::Row(vec![])), sub: child_node(e, "m:sub").map(Box::new), sup: child_node(e, "m:sup").map(Box::new) }, from_omml(e.child("m:e")?)]),
        "m:eqArr" => Node::Matrix(kids(e).filter(|k| k.name == "m:e").map(|x| vec![from_omml(x)]).collect()),
        n if n.ends_with("Pr") || n == "w:bookmarkStart" || n == "w:bookmarkEnd" || n == "m:ctrlPr" => return None,
        _ => from_omml(e),
    })
}

/// An equation read from an m:oMath element.
pub fn read_omml(e: &Element, display: bool) -> Equation {
    let mut eq = Equation::new(&to_linear(&from_omml(e)));
    eq.display = display;
    eq
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typesets_and_round_trips_omml() {
        for (_, src) in GALLERY {
            let tree = parse(src);
            let eq = Equation::new(src);
            let (w, h, d) = eq.metrics();
            assert!(w > 20.0 && h > 10.0 && d >= 0.0, "{src}: {w}×{h}");
            // OMML out and back gives the same tree.
            let xml = format!(r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{}</m:oMath>"#, to_omml(&tree));
            let back = from_omml(&crate::xml::parse(&xml).unwrap());
            assert_eq!(parse(&to_linear(&back)), parse(&to_linear(&tree)), "{src}\n{}\n{}", to_linear(&tree), to_linear(&back));
        }
        // Linear fractions, scripts and functions.
        assert!(matches!(parse("(a+b)/c"), Node::Frac(..)));
        assert!(matches!(parse("x^2"), Node::Scripts { sup: Some(_), .. }));
        assert!(matches!(parse("sin x"), Node::Func { .. }));
        let q = Equation::new("x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}");
        assert!(q.metrics().1 > Equation::new("x=1").metrics().1 * 1.5, "a fraction is taller than a line");
    }
}
