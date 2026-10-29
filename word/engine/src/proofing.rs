//! Proofing: spelling against the bundled US English Hunspell dictionary (SCOWL), with
//! suggestions, ignored and added words, plus a few grammar checks Word also flags (repeated
//! words, a/an, sentence capitals, double spaces).

use crate::model::Document;
use std::collections::HashSet;
use std::sync::OnceLock;

static DICT: OnceLock<Option<spellbook::Dictionary>> = OnceLock::new();

fn dict() -> Option<&'static spellbook::Dictionary> {
    DICT.get_or_init(|| {
        let text = |p: &str| crate::resources::load(p).map(|b| String::from_utf8_lossy(b).into_owned());
        spellbook::Dictionary::new(&text("dict/en_US.aff")?, &text("dict/en_US.dic")?).ok()
    }).as_ref()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind { Spelling, RepeatedWord, Article, Capital, Spaces }

impl IssueKind {
    pub fn label(&self) -> &'static str {
        match self { IssueKind::Spelling => "Spelling", IssueKind::RepeatedWord => "Repeated word", IssueKind::Article => "Use \"a\" or \"an\"", IssueKind::Capital => "Capitalize the first word", IssueKind::Spaces => "Extra space" }
    }
    pub fn is_grammar(&self) -> bool { *self != IssueKind::Spelling }
}

/// A proofing issue over [start, end) with suggested replacements.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue { pub start: usize, pub end: usize, pub word: String, pub kind: IssueKind, pub suggestions: Vec<String> }

/// Is the word spelled correctly? Numbers, single letters, words with digits and all-caps
/// acronyms pass, as in Word's defaults.
pub fn correct(word: &str) -> bool {
    let w = word.trim_matches(|c: char| c == '\'' || c == '\u{2019}');
    if w.chars().count() <= 1 || w.chars().any(|c| c.is_ascii_digit()) || !w.chars().any(|c| c.is_alphabetic()) { return true; }
    if w.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) && w.chars().count() <= 6 { return true; }
    // Only Latin-script words are checked (the dictionary is English).
    if !w.chars().all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '\u{2019}' || c == '-') { return true; }
    let Some(d) = dict() else { return true };
    let w = w.replace('\u{2019}', "'");
    d.check(&w) || w.split('-').all(|p| !p.is_empty() && d.check(p))
}

/// Up to `n` suggestions for a misspelled word.
pub fn suggest(word: &str, n: usize) -> Vec<String> {
    let Some(d) = dict() else { return vec![] };
    let mut out = vec![];
    d.suggest(&word.replace('\u{2019}', "'"), &mut out);
    out.truncate(n);
    out
}

/// Words in a text: (char offset, word).
pub fn words(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut k = 0;
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}' || c == '-';
    while k < chars.len() {
        if chars[k].is_alphanumeric() {
            let s = k;
            while k < chars.len() && is_word(chars[k]) { k += 1; }
            let mut e = k;
            while e > s && !chars[e - 1].is_alphanumeric() { e -= 1; }
            out.push((s, chars[s..e].iter().collect()));
        } else { k += 1; }
    }
    out
}

impl Document {
    /// Proofing issues in the body text, in document order. `ignored` words never count.
    pub fn proofing_issues(&self, ignored: &HashSet<String>, grammar: bool) -> Vec<Issue> {
        let mut out = vec![];
        let mut base = 0;
        for p in &self.paragraphs {
            let text = p.text();
            let chars: Vec<char> = text.chars().collect();
            let ws = words(&text);
            for (k, (at, w)) in ws.iter().enumerate() {
                let lower = w.to_lowercase();
                if !ignored.contains(w) && !ignored.contains(&lower) && !correct(w) {
                    out.push(Issue { start: base + at, end: base + at + w.chars().count(), word: w.clone(), kind: IssueKind::Spelling, suggestions: vec![] });
                }
                if !grammar { continue; }
                if let Some((pat, prev)) = k.checked_sub(1).map(|j| &ws[j]).map(|(pa, pw)| (pa, pw)) {
                    let gap: String = chars[pat + prev.chars().count()..*at].iter().collect();
                    if prev.eq_ignore_ascii_case(w) && gap.chars().all(|c| c == ' ') && w.chars().any(|c| c.is_alphabetic()) {
                        out.push(Issue { start: base + pat + prev.chars().count(), end: base + at + w.chars().count(), word: format!("{prev} {w}"), kind: IssueKind::RepeatedWord, suggestions: vec![prev.clone()] });
                    }
                    let vowel = w.chars().next().is_some_and(|c| "aeiouAEIOU".contains(c)) && !["uni", "use", "usu", "one", "eu"].iter().any(|x| lower.starts_with(x));
                    let silent_h = ["hour", "honest", "honor", "heir"].iter().any(|x| lower.starts_with(x));
                    if gap == " " && (prev == "a" || prev == "A") && (vowel || silent_h) {
                        out.push(Issue { start: base + pat, end: base + pat + 1, word: prev.clone(), kind: IssueKind::Article, suggestions: vec![format!("{prev}n")] });
                    } else if gap == " " && (prev == "an" || prev == "An") && !vowel && !silent_h && w.chars().next().is_some_and(|c| c.is_alphabetic()) {
                        out.push(Issue { start: base + pat, end: base + pat + 2, word: prev.clone(), kind: IssueKind::Article, suggestions: vec![prev[..1].to_string()] });
                    }
                }
                // A sentence's first word starts with a capital.
                let before: String = chars[..*at].iter().collect::<String>();
                let t = before.trim_end();
                let starts_sentence = (t.is_empty() && p.cell.is_none() && k == 0) || t.ends_with(['.', '!', '?']) && !t.ends_with("..") && !t.split_whitespace().last().is_some_and(|x| ["e.g.", "i.e.", "etc.", "Mr.", "Mrs.", "Dr.", "vs."].contains(&x));
                if starts_sentence && w.chars().next().is_some_and(|c| c.is_lowercase()) && w.chars().all(|c| c.is_alphabetic()) && chars.len() > 12 {
                    let mut cap = w.clone(); if let Some(f) = cap.get_mut(0..1) { f.make_ascii_uppercase(); }
                    out.push(Issue { start: base + at, end: base + at + w.chars().count(), word: w.clone(), kind: IssueKind::Capital, suggestions: vec![cap] });
                }
            }
            if grammar {
                let mut k = 0;
                while k + 1 < chars.len() {
                    if chars[k] == ' ' && chars[k + 1] == ' ' && k > 0 && chars[k - 1] != ' ' && chars[k - 1] != '\t' {
                        let mut e = k; while e < chars.len() && chars[e] == ' ' { e += 1; }
                        if e < chars.len() { out.push(Issue { start: base + k, end: base + e, word: " ".repeat(e - k), kind: IssueKind::Spaces, suggestions: vec![" ".into()] }); }
                        k = e;
                    } else { k += 1; }
                }
            }
            base += p.len() + 1;
        }
        out.sort_by_key(|i| i.start);
        out
    }

    /// Replace an issue's text with a suggestion (one undo step).
    pub fn fix_issue(&mut self, issue: &Issue, with: &str) {
        self.select(issue.start, issue.end);
        self.replace(with);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Paragraph;

    #[test]
    fn spelling_and_grammar() {
        assert!(correct("houses") && correct("running") && correct("don't") && correct("NASA") && correct("well-known"));
        assert!(!correct("recieve") && !correct("teh"));
        assert!(suggest("recieve", 5).iter().any(|s| s == "receive"));
        let d = Document::new(vec![Paragraph::new("This is is a apple.  we recieve mail every day here.")]);
        let issues = d.proofing_issues(&HashSet::new(), true);
        let kinds: Vec<IssueKind> = issues.iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&IssueKind::RepeatedWord) && kinds.contains(&IssueKind::Article) && kinds.contains(&IssueKind::Spaces) && kinds.contains(&IssueKind::Capital), "{kinds:?}");
        let sp = issues.iter().find(|i| i.kind == IssueKind::Spelling).unwrap();
        assert_eq!(sp.word, "recieve");
        let ignored: HashSet<String> = ["recieve".to_string()].into();
        assert!(!d.proofing_issues(&ignored, false).iter().any(|i| i.word == "recieve"));
    }
}

/// A word split into syllables with the US English hyphenation patterns (Immersive Reader).
pub fn syllables(word: &str) -> Vec<String> {
    use hyphenation::{Hyphenator, Language, Load, Standard};
    static H: OnceLock<Option<Standard>> = OnceLock::new();
    let Some(d) = H.get_or_init(|| Standard::from_embedded(Language::EnglishUS).ok()).as_ref() else { return vec![word.to_string()] };
    if word.chars().count() < 4 || !word.chars().all(|c| c.is_ascii_alphabetic()) { return vec![word.to_string()]; }
    let lower = word.to_lowercase();
    let breaks = d.hyphenate(&lower).breaks;
    let mut out = vec![];
    let mut last = 0;
    for b in breaks { out.push(word[last..b].to_string()); last = b; }
    out.push(word[last..].to_string());
    out
}
