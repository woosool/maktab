//! Bullets and numbering (numbering.js): abstract definitions, instances with
//! start overrides, and list-marker text.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumFmt {
    Bullet, Decimal, LowerLetter, UpperLetter, LowerRoman, UpperRoman, DecimalZero, None,
    /// Arabic letters in alphabetical (أ ب ت) and abjad (أ ب ج د) order; Arabic-Indic digits (١ ٢ ٣).
    ArabicAlpha, ArabicAbjad, HindiNumbers,
    /// Hebrew numerals (gematria: א … י, יא …) and letters (א ב ג).
    Hebrew1, Hebrew2,
    /// 1st, One, First (English).
    Ordinal, CardinalText, OrdinalText,
    /// *, †, ‡, §, then doubled (footnote symbols).
    Chicago,
}

/// Every format offered in Define New Number Format, with its display name.
pub const NUMBER_FORMATS: &[(NumFmt, &str)] = &[
    (NumFmt::Decimal, "1, 2, 3"), (NumFmt::DecimalZero, "01, 02, 03"), (NumFmt::UpperRoman, "I, II, III"), (NumFmt::LowerRoman, "i, ii, iii"),
    (NumFmt::UpperLetter, "A, B, C"), (NumFmt::LowerLetter, "a, b, c"), (NumFmt::Ordinal, "1st, 2nd, 3rd"), (NumFmt::CardinalText, "One, Two, Three"),
    (NumFmt::OrdinalText, "First, Second, Third"), (NumFmt::ArabicAlpha, "أ، ب، ت"), (NumFmt::ArabicAbjad, "أ، ب، ج"), (NumFmt::HindiNumbers, "١، ٢، ٣"),
    (NumFmt::Hebrew1, "א, ב, ג (numerals)"), (NumFmt::Hebrew2, "א, ב, ג (letters)"), (NumFmt::None, "(none)"),
];

impl NumFmt {
    pub fn from_ooxml(v: &str) -> NumFmt {
        match v {
            "bullet" => NumFmt::Bullet, "lowerLetter" => NumFmt::LowerLetter, "upperLetter" => NumFmt::UpperLetter,
            "lowerRoman" => NumFmt::LowerRoman, "upperRoman" => NumFmt::UpperRoman, "decimalZero" => NumFmt::DecimalZero,
            "none" => NumFmt::None, "arabicAlpha" => NumFmt::ArabicAlpha, "arabicAbjad" => NumFmt::ArabicAbjad, "hindiNumbers" => NumFmt::HindiNumbers,
            "hebrew1" => NumFmt::Hebrew1, "hebrew2" => NumFmt::Hebrew2, "ordinal" => NumFmt::Ordinal, "cardinalText" => NumFmt::CardinalText, "ordinalText" => NumFmt::OrdinalText, "chicago" => NumFmt::Chicago,
            _ => NumFmt::Decimal,
        }
    }
    pub fn ooxml(&self) -> &'static str {
        match self {
            NumFmt::Bullet => "bullet", NumFmt::Decimal => "decimal", NumFmt::LowerLetter => "lowerLetter", NumFmt::UpperLetter => "upperLetter",
            NumFmt::LowerRoman => "lowerRoman", NumFmt::UpperRoman => "upperRoman", NumFmt::DecimalZero => "decimalZero", NumFmt::None => "none",
            NumFmt::ArabicAlpha => "arabicAlpha", NumFmt::ArabicAbjad => "arabicAbjad", NumFmt::HindiNumbers => "hindiNumbers",
            NumFmt::Hebrew1 => "hebrew1", NumFmt::Hebrew2 => "hebrew2", NumFmt::Ordinal => "ordinal", NumFmt::CardinalText => "cardinalText", NumFmt::OrdinalText => "ordinalText", NumFmt::Chicago => "chicago",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub fmt: NumFmt,
    pub text: String,
    pub start: u32,
    pub indent_left: i32,
    pub hanging: i32,
    pub font: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AbstractNum {
    pub id: u32,
    pub levels: Vec<Level>,
    /// Original XML, written back unchanged when the definition was imported.
    pub raw: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NumInstance {
    pub id: u32,
    pub abstract_id: u32,
    pub start_overrides: HashMap<u8, u32>,
    pub raw: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Numbering {
    pub abstracts: Vec<AbstractNum>,
    pub nums: Vec<NumInstance>,
    pub changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind { Bullet, Number }

/// The script whose list letters a new list uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script { Latin, Arabic, Hebrew }

const BULLETS: [&str; 3] = ["\u{2022}", "\u{25E6}", "\u{25AA}"];

impl Numbering {
    pub fn level(&self, num_id: u32, ilvl: u8) -> Option<&Level> {
        let num = self.nums.iter().find(|n| n.id == num_id)?;
        let abs = self.abstracts.iter().find(|a| a.id == num.abstract_id)?;
        abs.levels.get(ilvl as usize).or(abs.levels.last())
    }
    pub fn start(&self, num_id: u32, ilvl: u8) -> u32 {
        let num = self.nums.iter().find(|n| n.id == num_id);
        if let Some(s) = num.and_then(|n| n.start_overrides.get(&ilvl)) { return *s; }
        self.level(num_id, ilvl).map(|l| l.start).unwrap_or(1)
    }
    pub fn kind(&self, num_id: u32) -> Option<ListKind> {
        self.level(num_id, 0).map(|l| if l.fmt == NumFmt::Bullet { ListKind::Bullet } else { ListKind::Number })
    }

    /// A new list for a right-to-left paragraph: Arabic (or Hebrew) letters replace a, b, c.
    pub fn new_list_for(&mut self, kind: ListKind, script: Script) -> u32 {
        if kind == ListKind::Bullet || script == Script::Latin { return self.new_list(kind); }
        let cycle = match script { Script::Hebrew => [NumFmt::Decimal, NumFmt::Hebrew2, NumFmt::Hebrew1], _ => [NumFmt::Decimal, NumFmt::ArabicAlpha, NumFmt::ArabicAbjad] };
        let levels = (0..9).map(|i| Level { fmt: cycle[i % 3], text: format!("%{}.", i + 1), start: 1, indent_left: 720 * (i as i32 + 1), hanging: 360, font: None }).collect();
        self.new_list_levels(levels)
    }

    /// Create a new list definition and instance (newList in numbering.js).
    pub fn new_list(&mut self, kind: ListKind) -> u32 {
        let abs_id = self.abstracts.iter().map(|a| a.id + 1).max().unwrap_or(0);
        let num_id = self.nums.iter().map(|n| n.id + 1).max().unwrap_or(1).max(1);
        let levels = (0..9).map(|i| {
            let (fmt, text) = match kind {
                ListKind::Bullet => (NumFmt::Bullet, BULLETS[i % 3].to_string()),
                ListKind::Number => ([NumFmt::Decimal, NumFmt::LowerLetter, NumFmt::LowerRoman][i % 3], format!("%{}.", i + 1)),
            };
            Level { fmt, text, start: 1, indent_left: 720 * (i as i32 + 1), hanging: 360, font: None }
        }).collect();
        self.abstracts.push(AbstractNum { id: abs_id, levels, raw: None });
        self.nums.push(NumInstance { id: num_id, abstract_id: abs_id, start_overrides: HashMap::new(), raw: None });
        self.changed = true;
        num_id
    }

    /// A new list from explicit level definitions (bullet/numbering libraries, multilevel lists).
    pub fn new_list_levels(&mut self, levels: Vec<Level>) -> u32 {
        let abs_id = self.abstracts.iter().map(|a| a.id + 1).max().unwrap_or(0);
        let num_id = self.nums.iter().map(|n| n.id + 1).max().unwrap_or(1).max(1);
        self.abstracts.push(AbstractNum { id: abs_id, levels, raw: None });
        self.nums.push(NumInstance { id: num_id, abstract_id: abs_id, start_overrides: HashMap::new(), raw: None });
        self.changed = true;
        num_id
    }

    /// A new instance of `num_id`'s definition starting at `start` (restartList).
    pub fn restart(&mut self, num_id: u32, ilvl: u8, start: u32) -> Option<u32> {
        let abs = self.nums.iter().find(|n| n.id == num_id)?.abstract_id;
        let id = self.nums.iter().map(|n| n.id + 1).max().unwrap_or(1);
        self.nums.push(NumInstance { id, abstract_id: abs, start_overrides: HashMap::from([(ilvl, start)]), raw: None });
        self.changed = true;
        Some(id)
    }
}

pub fn format_number(n: u32, fmt: NumFmt) -> String {
    match fmt {
        NumFmt::Decimal => n.to_string(),
        NumFmt::DecimalZero => format!("{n:02}"),
        NumFmt::LowerLetter => letters(n),
        NumFmt::UpperLetter => letters(n).to_uppercase(),
        NumFmt::LowerRoman => roman(n).to_lowercase(),
        NumFmt::UpperRoman => roman(n),
        NumFmt::Bullet | NumFmt::None => String::new(),
        NumFmt::ArabicAlpha => cycle(n, ARABIC_ALPHA),
        NumFmt::ArabicAbjad => cycle(n, ARABIC_ABJAD),
        NumFmt::HindiNumbers => n.to_string().chars().map(|d| char::from_u32(0x0660 + d.to_digit(10).unwrap_or(0)).unwrap()).collect(),
        NumFmt::Hebrew1 => gematria(n),
        NumFmt::Hebrew2 => cycle(n, HEBREW),
        NumFmt::Ordinal => { let suffix = match (n % 10, n % 100) { (_, 11..=13) => "th", (1, _) => "st", (2, _) => "nd", (3, _) => "rd", _ => "th" }; format!("{n}{suffix}") }
        NumFmt::CardinalText => capitalize(&cardinal(n)),
        NumFmt::OrdinalText => capitalize(&ordinal_text(n)),
        NumFmt::Chicago => { let s = ['*', '†', '‡', '§'][((n.max(1) - 1) % 4) as usize]; std::iter::repeat_n(s, ((n.max(1) - 1) / 4 + 1) as usize).collect() }
    }
}

const ARABIC_ALPHA: &[char] = &['أ', 'ب', 'ت', 'ث', 'ج', 'ح', 'خ', 'د', 'ذ', 'ر', 'ز', 'س', 'ش', 'ص', 'ض', 'ط', 'ظ', 'ع', 'غ', 'ف', 'ق', 'ك', 'ل', 'م', 'ن', 'ه', 'و', 'ي'];
const ARABIC_ABJAD: &[char] = &['أ', 'ب', 'ج', 'د', 'ه', 'و', 'ز', 'ح', 'ط', 'ي', 'ك', 'ل', 'م', 'ن', 'س', 'ع', 'ف', 'ص', 'ق', 'ر', 'ش', 'ت', 'ث', 'خ', 'ذ', 'ض', 'ظ', 'غ'];
const HEBREW: &[char] = &['א', 'ב', 'ג', 'ד', 'ה', 'ו', 'ז', 'ח', 'ט', 'י', 'כ', 'ל', 'מ', 'נ', 'ס', 'ע', 'פ', 'צ', 'ק', 'ר', 'ש', 'ת'];

/// Letter sequences repeat the letter after the alphabet ends (Word: …, ي, أأ, بب).
fn cycle(n: u32, letters: &[char]) -> String {
    if n == 0 { return String::new(); }
    let k = letters.len() as u32;
    letters[((n - 1) % k) as usize].to_string().repeat(((n - 1) / k + 1) as usize)
}

/// Hebrew numerals: hundreds, tens and units letters, with 15/16 written ט״ו / ט״ז.
fn gematria(mut n: u32) -> String {
    let mut s = String::new();
    while n >= 400 { s.push('ת'); n -= 400; }
    for (v, c) in [(300, 'ש'), (200, 'ר'), (100, 'ק')] { if n >= v { s.push(c); n -= v; } }
    if n == 15 || n == 16 { s.push('ט'); s.push(if n == 15 { 'ו' } else { 'ז' }); return s; }
    let tens = ['י', 'כ', 'ל', 'מ', 'נ', 'ס', 'ע', 'פ', 'צ'];
    if n >= 10 { s.push(tens[(n / 10 - 1) as usize]); n %= 10; }
    if n > 0 { s.push(HEBREW[(n - 1) as usize]); }
    s
}

const ONES: [&str; 20] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"];
const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

fn cardinal(n: u32) -> String {
    match n {
        0..=19 => ONES[n as usize].into(),
        20..=99 => if n % 10 == 0 { TENS[(n / 10) as usize].into() } else { format!("{}-{}", TENS[(n / 10) as usize], ONES[(n % 10) as usize]) },
        100..=999 => if n % 100 == 0 { format!("{} hundred", ONES[(n / 100) as usize]) } else { format!("{} hundred {}", ONES[(n / 100) as usize], cardinal(n % 100)) },
        _ => if n % 1000 == 0 { format!("{} thousand", cardinal(n / 1000)) } else { format!("{} thousand {}", cardinal(n / 1000), cardinal(n % 1000)) },
    }
}

fn ordinal_text(n: u32) -> String {
    let c = cardinal(n);
    let (head, last) = match c.rfind([' ', '-']) { Some(i) => (&c[..=i], &c[i + 1..]), None => ("", c.as_str()) };
    let last = match last {
        "one" => "first".into(), "two" => "second".into(), "three" => "third".into(), "five" => "fifth".into(), "eight" => "eighth".into(),
        "nine" => "ninth".into(), "twelve" => "twelfth".into(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    };
    format!("{head}{last}")
}

fn capitalize(s: &str) -> String { let mut c = s.chars(); c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default() }

fn letters(n: u32) -> String {
    if n == 0 { return String::new(); }
    let c = (b'a' + ((n - 1) % 26) as u8) as char;
    c.to_string().repeat(((n - 1) / 26 + 1) as usize)
}

fn roman(mut n: u32) -> String {
    const T: [(u32, &str); 13] = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut s = String::new();
    for (v, r) in T { while n >= v { s.push_str(r); n -= v; } }
    s
}

/// Tracks counters while walking paragraphs in document order (listMarkers).
#[derive(Default)]
pub struct MarkerCounter { counters: HashMap<u32, [u32; 9]>, seen: HashMap<u32, [bool; 9]> }

impl MarkerCounter {
    pub fn next(&mut self, numbering: &Numbering, num_id: u32, ilvl: u8) -> Option<String> {
        let level = numbering.level(num_id, ilvl)?;
        let ilvl = (ilvl as usize).min(8);
        // Instances of one abstract definition share counters unless overridden.
        let key = numbering.nums.iter().find(|n| n.id == num_id).map(|n| if n.start_overrides.is_empty() { n.abstract_id } else { 10_000 + n.id }).unwrap_or(num_id);
        let counters = self.counters.entry(key).or_insert([0; 9]);
        let seen = self.seen.entry(key).or_insert([false; 9]);
        if !seen[ilvl] { counters[ilvl] = numbering.start(num_id, ilvl as u8); seen[ilvl] = true; } else { counters[ilvl] += 1; }
        for deeper in ilvl + 1..9 { seen[deeper] = false; }
        if level.fmt == NumFmt::Bullet { return Some(level.text.clone()); }
        let mut text = level.text.clone();
        for l in 0..=ilvl {
            let fmt = numbering.level(num_id, l as u8).map(|x| x.fmt).unwrap_or(NumFmt::Decimal);
            let value = if seen[l] { counters[l] } else { numbering.start(num_id, l as u8) };
            text = text.replace(&format!("%{}", l + 1), &format_number(value, fmt));
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markers_count_and_nest() {
        let mut n = Numbering::default();
        let id = n.new_list(ListKind::Number);
        let mut c = MarkerCounter::default();
        assert_eq!(c.next(&n, id, 0).unwrap(), "1.");
        assert_eq!(c.next(&n, id, 1).unwrap(), "a.");
        assert_eq!(c.next(&n, id, 1).unwrap(), "b.");
        assert_eq!(c.next(&n, id, 0).unwrap(), "2.");
        assert_eq!(c.next(&n, id, 1).unwrap(), "a.");
        assert_eq!(roman(1994), "MCMXCIV");
        assert_eq!(format_number(3, NumFmt::ArabicAlpha), "ت");
        assert_eq!(format_number(3, NumFmt::ArabicAbjad), "ج");
        assert_eq!(format_number(29, NumFmt::ArabicAlpha), "أأ");
        assert_eq!(format_number(2026, NumFmt::HindiNumbers), "٢٠٢٦");
        assert_eq!(format_number(15, NumFmt::Hebrew1), "טו");
        assert_eq!(format_number(123, NumFmt::Hebrew1), "קכג");
        assert_eq!(format_number(22, NumFmt::Ordinal), "22nd");
        assert_eq!(format_number(21, NumFmt::CardinalText), "Twenty-one");
        assert_eq!(format_number(12, NumFmt::OrdinalText), "Twelfth");
        assert_eq!(format_number(40, NumFmt::OrdinalText), "Fortieth");
    }
}
