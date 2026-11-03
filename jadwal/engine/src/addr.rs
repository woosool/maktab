//! Cell addresses (refs.js): column letters, A1 text, and rectangular ranges.

pub const MAX_ROWS: u32 = 1_048_576;
pub const MAX_COLS: u32 = 16_384;

/// A cell position, zero-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos { pub row: u32, pub col: u32 }

impl Pos {
    pub const fn new(row: u32, col: u32) -> Pos { Pos { row, col } }
    pub fn a1(&self) -> String { format!("{}{}", col_name(self.col), self.row + 1) }
}

/// An inclusive rectangle of cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Range { pub r0: u32, pub c0: u32, pub r1: u32, pub c1: u32 }

impl Range {
    pub fn new(a: Pos, b: Pos) -> Range { Range { r0: a.row.min(b.row), c0: a.col.min(b.col), r1: a.row.max(b.row), c1: a.col.max(b.col) } }
    pub fn cell(p: Pos) -> Range { Range::new(p, p) }
    pub fn rows(&self) -> u32 { self.r1 - self.r0 + 1 }
    pub fn cols(&self) -> u32 { self.c1 - self.c0 + 1 }
    pub fn contains(&self, p: Pos) -> bool { p.row >= self.r0 && p.row <= self.r1 && p.col >= self.c0 && p.col <= self.c1 }
    pub fn start(&self) -> Pos { Pos::new(self.r0, self.c0) }
    pub fn end(&self) -> Pos { Pos::new(self.r1, self.c1) }
    pub fn is_cell(&self) -> bool { self.r0 == self.r1 && self.c0 == self.c1 }
    pub fn intersects(&self, o: &Range) -> bool { self.r0 <= o.r1 && o.r0 <= self.r1 && self.c0 <= o.c1 && o.c0 <= self.c1 }
    pub fn union(&self, o: &Range) -> Range { Range { r0: self.r0.min(o.r0), c0: self.c0.min(o.c0), r1: self.r1.max(o.r1), c1: self.c1.max(o.c1) } }
    pub fn iter(&self) -> impl Iterator<Item = Pos> + '_ { (self.r0..=self.r1).flat_map(move |r| (self.c0..=self.c1).map(move |c| Pos::new(r, c))) }
    pub fn a1(&self) -> String {
        if self.c0 == 0 && self.c1 == MAX_COLS - 1 { return format!("{}:{}", self.r0 + 1, self.r1 + 1); }
        if self.r0 == 0 && self.r1 == MAX_ROWS - 1 { return format!("{}:{}", col_name(self.c0), col_name(self.c1)); }
        if self.is_cell() { self.start().a1() } else { format!("{}:{}", self.start().a1(), self.end().a1()) }
    }
}

/// 0 → A, 25 → Z, 26 → AA.
pub fn col_name(mut c: u32) -> String {
    let mut s = Vec::new();
    loop { s.push(b'A' + (c % 26) as u8); if c < 26 { break; } c = c / 26 - 1; }
    s.reverse();
    String::from_utf8(s).unwrap()
}

/// A → 0; None past XFD or for anything not letters.
pub fn col_index(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 3 { return None; }
    let mut n: u32 = 0;
    for ch in s.chars() {
        if !ch.is_ascii_alphabetic() { return None; }
        n = n * 26 + (ch.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
    }
    (n >= 1 && n <= MAX_COLS).then(|| n - 1)
}

/// "B3" → Pos(2, 1); "$B$3" too.
pub fn parse_a1(s: &str) -> Option<Pos> {
    let s = s.replace('$', "");
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let (l, d) = s.split_at(split);
    let col = col_index(l)?;
    let row: u32 = d.parse().ok()?;
    (row >= 1 && row <= MAX_ROWS).then(|| Pos::new(row - 1, col))
}

/// "A1:B2", "A:C", "2:5" or "B3".
pub fn parse_range(s: &str) -> Option<Range> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once(':') {
        if let (Some(a), Some(b)) = (parse_a1(a), parse_a1(b)) { return Some(Range::new(a, b)); }
        let (a, b) = (a.replace('$', ""), b.replace('$', ""));
        if let (Some(x), Some(y)) = (col_index(&a), col_index(&b)) { return Some(Range { r0: 0, r1: MAX_ROWS - 1, c0: x.min(y), c1: x.max(y) }); }
        if let (Ok(x), Ok(y)) = (a.parse::<u32>(), b.parse::<u32>()) {
            if x >= 1 && y >= 1 && x <= MAX_ROWS && y <= MAX_ROWS { return Some(Range { r0: x.min(y) - 1, r1: x.max(y) - 1, c0: 0, c1: MAX_COLS - 1 }); }
        }
        return None;
    }
    parse_a1(s).map(Range::cell)
}

/// A sheet name as a formula writes it: quoted when it is not a plain identifier.
pub fn quote_sheet(name: &str) -> String {
    let plain = !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && parse_a1(name).is_none() && col_index(name).is_none_or(|_| name.len() > 3);
    if plain { name.to_string() } else { format!("'{}'", name.replace('\'', "''")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(col_name(0), "A"); assert_eq!(col_name(25), "Z"); assert_eq!(col_name(26), "AA"); assert_eq!(col_name(16383), "XFD");
        assert_eq!(col_index("XFD"), Some(16383)); assert_eq!(parse_a1("$C$10"), Some(Pos::new(9, 2)));
        assert_eq!(parse_range("A:B").unwrap().r1, MAX_ROWS - 1);
        assert_eq!(quote_sheet("My Sheet"), "'My Sheet'"); assert_eq!(quote_sheet("Sheet1"), "Sheet1");
    }
}
