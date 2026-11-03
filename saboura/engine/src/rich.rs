//! Styled runs inside an object's text (`rich-text.js`): `textRuns` are `{start, end, style}`
//! with UTF-16 offsets, sparse overrides of the object's own style (bold, italic, underline,
//! strike, textColor, highlight, link).

use serde_json::{Map, Value, json};

pub const STYLE_KEYS: [&str; 7] = ["bold", "italic", "underline", "strike", "textColor", "highlight", "link"];

/// One run: UTF-16 offsets and its style.
#[derive(Clone, Debug, PartialEq)]
pub struct Run { pub start: usize, pub end: usize, pub style: Map<String, Value> }

pub fn read(v: Option<&Value>) -> Vec<Run> {
    v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|r| Some(Run {
        start: r["start"].as_u64()? as usize, end: r["end"].as_u64()? as usize, style: r["style"].as_object()?.clone(),
    })).filter(|r| r.end > r.start && !r.style.is_empty()).collect()).unwrap_or_default()
}

pub fn write(runs: &[Run]) -> Value {
    if runs.is_empty() { return Value::Null; }
    Value::Array(runs.iter().map(|r| json!({ "start": r.start, "end": r.end, "style": r.style })).collect())
}

/// The UTF-16 offset of each char boundary, ending with the text's UTF-16 length.
pub fn utf16_offsets(text: &str) -> Vec<usize> {
    let mut v = Vec::with_capacity(text.len() + 1);
    let mut at = 0;
    for c in text.chars() { v.push(at); at += c.len_utf16(); }
    v.push(at);
    v
}

/// The style of each char: merged runs, one map per char.
fn per_char(text: &str, runs: &[Run]) -> Vec<Map<String, Value>> {
    let offs = utf16_offsets(text);
    let n = offs.len() - 1;
    let mut out = vec![Map::new(); n];
    for r in runs {
        for (k, o) in offs[..n].iter().enumerate() {
            if *o >= r.start && *o < r.end { for (key, v) in &r.style { out[k].insert(key.clone(), v.clone()); } }
        }
    }
    out
}

/// Runs from per-char styles, neighbours with the same style merged.
fn from_chars(text: &str, styles: &[Map<String, Value>]) -> Vec<Run> {
    let offs = utf16_offsets(text);
    let mut runs: Vec<Run> = Vec::new();
    for (k, s) in styles.iter().enumerate() {
        if s.is_empty() { continue; }
        match runs.last_mut() {
            Some(last) if last.end == offs[k] && last.style == *s => last.end = offs[k + 1],
            _ => runs.push(Run { start: offs[k], end: offs[k + 1], style: s.clone() }),
        }
    }
    runs
}

/// Sets `key` to `value` (Null removes it) on chars `from..to` (char indices).
pub fn apply(text: &str, runs: &[Run], from: usize, to: usize, key: &str, value: Value) -> Vec<Run> {
    let mut chars = per_char(text, runs);
    for s in chars.iter_mut().take(to).skip(from) { if value.is_null() { s.remove(key); } else { s.insert(key.to_string(), value.clone()); } }
    from_chars(text, &chars)
}

/// Whether every char in `from..to` has `key` set to true (or a value).
pub fn has(text: &str, runs: &[Run], from: usize, to: usize, key: &str) -> bool {
    let chars = per_char(text, runs);
    to > from && chars.iter().take(to).skip(from).all(|s| s.get(key).is_some_and(|v| !v.is_null() && v != &Value::Bool(false)))
}

/// Keeps runs on the same characters after the text changed from `old` to `new`: the
/// common start and end stay; typed chars take the style of the char before them.
pub fn adjust(old: &str, new: &str, runs: &[Run]) -> Vec<Run> {
    if runs.is_empty() { return Vec::new(); }
    let (a, b): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let pre = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let olds = per_char(old, runs);
    let mut styles: Vec<Map<String, Value>> = olds[..pre].to_vec();
    let fill = if pre > 0 { olds[pre - 1].clone() } else { Map::new() };
    for _ in pre..b.len() - suf { styles.push(fill.clone()); }
    styles.extend_from_slice(&olds[a.len() - suf..]);
    from_chars(new, &styles)
}

/// Spans of a text with their styles: (char range, style), covering the whole text.
pub fn spans(text: &str, runs: &[Run]) -> Vec<(std::ops::Range<usize>, Map<String, Value>)> {
    let chars = per_char(text, runs);
    let mut out: Vec<(std::ops::Range<usize>, Map<String, Value>)> = Vec::new();
    for (k, s) in chars.into_iter().enumerate() {
        match out.last_mut() {
            Some((r, st)) if *st == s => r.end = k + 1,
            _ => out.push((k..k + 1, s)),
        }
    }
    out
}
