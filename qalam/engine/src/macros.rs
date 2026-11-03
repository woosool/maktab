//! Macros: small programs that edit the document, written in Rhai (a sandboxed scripting
//! language) against a Word-like command set, recorded from what you do (Record Macro) or
//! written by hand. A document keeps its macros in a customXml part; the app keeps macros for
//! all documents in its config folder (like Word's Normal template).
//!
//! Scripts can only call the document commands registered here — no files, network or
//! processes — and they stop after a fixed number of operations, so a runaway loop can't hang
//! the app.

use crate::format::{Alignment, RunProps};
use crate::model::{Document, Paragraph};
use rhai::{Dynamic, Engine, EvalAltResult, Position, Scope};
use std::cell::RefCell;
use std::rc::Rc;

/// A macro: a name (a valid identifier), what it does, and its script.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Macro { pub name: String, pub description: String, pub code: String, pub shortcut: Option<String> }

/// Record Macro's state: the script so far and how deep inside a recorded command we are
/// (commands called by other commands aren't recorded twice).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Recorder { pub lines: Vec<String>, pub depth: usize, pub paused: bool }

impl Recorder {
    /// The recorded script, with consecutive typing merged into one call.
    pub fn script(&self) -> String {
        let mut out: Vec<String> = vec![];
        let mut typed = String::new();
        let flush = |typed: &mut String, out: &mut Vec<String>| if !typed.is_empty() { out.push(format!("type_text({});", quote(typed))); typed.clear(); };
        for l in &self.lines {
            match l.strip_prefix("\u{1}") { Some(t) => typed.push_str(t), None => { flush(&mut typed, &mut out); out.push(l.clone()); } }
        }
        flush(&mut typed, &mut out);
        out.join("\n")
    }
}

fn quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() { match c { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), '\n' => o.push_str("\\n"), '\t' => o.push_str("\\t"), c => o.push(c) } }
    o.push('"');
    o
}

/// The recorded line for typed text (merged with neighbouring typing by `script`).
pub fn text_line(text: &str) -> String {
    match text {
        "\n" => "new_paragraph();".into(),
        "\t" => "tab();".into(),
        t if t == crate::chars::LINE_BREAK.to_string() => "line_break();".into(),
        t if t == crate::chars::PAGE_BREAK.to_string() => "page_break();".into(),
        t => format!("\u{1}{t}"),
    }
}

/// Recorded lines for direct formatting.
pub fn run_props_line(p: &RunProps) -> String {
    let mut out = vec![];
    if let Some(f) = &p.font { out.push(format!("font({});", quote(f))); }
    if let Some(s) = p.size { out.push(format!("font_size({s});")); }
    if let Some(b) = p.bold { out.push(format!("set_bold({b});")); }
    if let Some(i) = p.italic { out.push(format!("set_italic({i});")); }
    if let Some(c) = &p.color { out.push(format!("color({});", quote(&c.map(|c| format!("#{}", c.hex())).unwrap_or_else(|| "auto".into())))); }
    if let Some(h) = &p.highlight { out.push(format!("highlight({});", quote(h.as_deref().unwrap_or("none")))); }
    if out.is_empty() { "// formatting".into() } else { out.join("\n") }
}

impl Document {
    /// Record a command's line unless it's called from inside another recorded command.
    pub(crate) fn rec_enter(&mut self, line: impl FnOnce() -> String) -> bool {
        let Some(r) = &mut self.recorder else { return false };
        if r.depth == 0 && !r.paused { r.lines.push(line()); }
        r.depth += 1;
        true
    }
    pub(crate) fn rec_exit(&mut self, entered: bool) { if entered { if let Some(r) = &mut self.recorder { r.depth = r.depth.saturating_sub(1); } } }

    /// Record a line the app produces (caret movement keys).
    pub fn record_line(&mut self, line: &str) {
        if let Some(r) = &mut self.recorder { if r.depth == 0 && !r.paused { r.lines.push(line.to_string()); } }
    }

    pub fn start_recording(&mut self) { self.recorder = Some(Recorder::default()); }
    /// Stop recording and return the script.
    pub fn stop_recording(&mut self) -> String { self.recorder.take().map(|r| r.script()).unwrap_or_default() }
}

/// What a macro run did: lines it printed, and an error with its line number.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RunResult { pub output: Vec<String>, pub error: Option<(String, Option<usize>)> }

/// Every command a macro can call, for the editor's reference list: (signature, description).
pub const COMMANDS: &[(&str, &str)] = &[
    ("type_text(text)", "Type text at the caret, replacing any selection"),
    ("new_paragraph()", "Start a new paragraph (Enter)"),
    ("line_break() / page_break() / tab()", "Insert a line break, page break or tab"),
    ("delete_back() / delete_forward()", "Delete the character before or after the caret"),
    ("delete_word_back() / delete_word_forward()", "Delete a word"),
    ("move_left(n) / move_right(n)", "Move the caret by characters"),
    ("move_word_left(n) / move_word_right(n)", "Move by words"),
    ("line_start() / line_end()", "Home / End of the paragraph"),
    ("doc_start() / doc_end()", "Start or end of the document"),
    ("next_paragraph() / previous_paragraph()", "Move to the next or previous paragraph"),
    ("extend_left(n) / extend_right(n)", "Extend the selection by characters"),
    ("select(start, end) / select_all()", "Select a range or everything"),
    ("select_word() / select_paragraph()", "Select the word or paragraph at the caret"),
    ("caret() / selection_end()", "The caret position / selection end"),
    ("selected_text() / text()", "The selected text / the whole document's text"),
    ("paragraph_count() / paragraph_text(i) / current_paragraph()", "Paragraphs"),
    ("goto_paragraph(i)", "Put the caret at the start of paragraph i (0-based)"),
    ("word_count() / length()", "Counts"),
    ("bold() / italic() / underline() / strikethrough()", "Toggle formatting"),
    ("set_bold(on) / set_italic(on)", "Set formatting on or off"),
    ("font(name) / font_size(points) / color(\"#RRGGBB\") / highlight(name)", "Character formatting"),
    ("grow_font() / shrink_font() / clear_formatting()", "Font size steps; remove direct formatting"),
    ("style(name)", "Apply a paragraph style (\"Heading1\", \"Normal\", \"Title\"…)"),
    ("align(\"left\"|\"center\"|\"right\"|\"justify\")", "Paragraph alignment"),
    ("bullets() / numbering()", "Toggle a bulleted or numbered list"),
    ("space_before(pt) / space_after(pt) / line_spacing(lines)", "Paragraph spacing"),
    ("indent(inches)", "Left indent"),
    ("find(text) -> bool", "Select the next match after the caret"),
    ("replace_all(find, with, match_case, whole_words) -> count", "Replace everywhere"),
    ("insert_table(rows, cols)", "Insert a table at the caret"),
    ("add_comment(text)", "Comment on the selection"),
    ("undo() / redo()", "Undo or redo the last action"),
    ("print(value)", "Show a line in the macro output"),
];

fn alignment(s: &str) -> Option<Alignment> { Some(match s.to_lowercase().as_str() { "left" => Alignment::Left, "center" | "centre" => Alignment::Center, "right" => Alignment::Right, "justify" | "justified" => Alignment::Justify, _ => return None }) }

/// The script engine with the document commands bound to `doc`.
fn engine(doc: Rc<RefCell<Document>>, output: Rc<RefCell<Vec<String>>>) -> Engine {
    let mut e = Engine::new();
    e.set_max_operations(2_000_000);
    e.set_max_call_levels(64);
    e.set_max_string_size(10_000_000);
    e.set_max_array_size(100_000);
    e.set_max_expr_depths(64, 32);
    { let out = output.clone(); e.on_print(move |s| out.borrow_mut().push(s.to_string())); }
    { let out = output.clone(); e.on_debug(move |s, _, _| out.borrow_mut().push(s.to_string())); }
    macro_rules! cmd {
        ($name:literal, |$d:ident| $body:expr) => {{ let d = doc.clone(); e.register_fn($name, move || { let $d = &mut *d.borrow_mut(); $body }); }};
        ($name:literal, |$d:ident, $a:ident: $t:ty| $body:expr) => {{ let d = doc.clone(); e.register_fn($name, move |$a: $t| { let $d = &mut *d.borrow_mut(); $body }); }};
        ($name:literal, |$d:ident, $a:ident: $t:ty, $b:ident: $u:ty| $body:expr) => {{ let d = doc.clone(); e.register_fn($name, move |$a: $t, $b: $u| { let $d = &mut *d.borrow_mut(); $body }); }};
        ($name:literal, |$d:ident, $a:ident: $t:ty, $b:ident: $u:ty, $c:ident: $v:ty, $x:ident: $w:ty| $body:expr) => {{ let d = doc.clone(); e.register_fn($name, move |$a: $t, $b: $u, $c: $v, $x: $w| { let $d = &mut *d.borrow_mut(); $body }); }};
    }
    let n = |v: i64| v.max(0) as usize;
    // Typing and deleting.
    cmd!("type_text", |d, t: &str| d.replace(t));
    cmd!("new_paragraph", |d| d.insert_paragraph());
    cmd!("line_break", |d| d.insert_line_break());
    cmd!("page_break", |d| d.insert_page_break());
    cmd!("tab", |d| d.insert_tab());
    cmd!("delete_back", |d| d.delete(false, false));
    cmd!("delete_forward", |d| d.delete(true, false));
    cmd!("delete_word_back", |d| d.delete(false, true));
    cmd!("delete_word_forward", |d| d.delete(true, true));
    // Moving and selecting.
    cmd!("move_left", |d, k: i64| { let (s, e2) = d.range(); let p = if s != e2 { s } else { let mut p = s; for _ in 0..n(k) { p = d.next_grapheme(p, false); } p }; d.select(p, p); });
    cmd!("move_right", |d, k: i64| { let (s, e2) = d.range(); let p = if s != e2 { e2 } else { let mut p = e2; for _ in 0..n(k) { p = d.next_grapheme(p, true); } p }; d.select(p, p); });
    cmd!("move_word_left", |d, k: i64| { let mut p = d.range().0; for _ in 0..n(k) { p = d.word_boundary(p, false); } d.select(p, p); });
    cmd!("move_word_right", |d, k: i64| { let mut p = d.range().1; for _ in 0..n(k) { p = d.word_boundary(p, true); } d.select(p, p); });
    cmd!("extend_left", |d, k: i64| { let (a, mut f) = (d.selection.anchor, d.selection.focus); for _ in 0..n(k) { f = d.next_grapheme(f, false); } d.select(a, f); });
    cmd!("extend_right", |d, k: i64| { let (a, mut f) = (d.selection.anchor, d.selection.focus); for _ in 0..n(k) { f = d.next_grapheme(f, true); } d.select(a, f); });
    cmd!("line_start", |d| { let (s, _) = d.paragraph_bounds(d.range().0); d.select(s, s); });
    cmd!("line_end", |d| { let (_, e2) = d.paragraph_bounds(d.range().1); d.select(e2, e2); });
    cmd!("doc_start", |d| d.select(0, 0));
    cmd!("doc_end", |d| { let l = d.len(); d.select(l, l); });
    cmd!("next_paragraph", |d| { let (i, _) = d.locate(d.range().1); let p = if i + 1 < d.paragraphs.len() { d.paragraph_start(i + 1) } else { d.len() }; d.select(p, p); });
    cmd!("previous_paragraph", |d| { let (i, _) = d.locate(d.range().0); let p = d.paragraph_start(i.saturating_sub(1)); d.select(p, p); });
    cmd!("select", |d, a: i64, b: i64| { let l = d.len(); d.select(n(a).min(l), n(b).min(l)); });
    cmd!("select_all", |d| d.select_all());
    cmd!("select_word", |d| { let (a, b) = d.word_at(d.range().0); d.select(a, b); });
    cmd!("select_paragraph", |d| { let (a, b) = d.paragraph_bounds(d.range().0); d.select(a, b); });
    cmd!("goto_paragraph", |d, i: i64| { let k = n(i).min(d.paragraphs.len().saturating_sub(1)); let p = d.paragraph_start(k); d.select(p, p); });
    // Reading.
    cmd!("caret", |d| d.range().0 as i64);
    cmd!("selection_end", |d| d.range().1 as i64);
    cmd!("selected_text", |d| d.selected_text());
    cmd!("text", |d| d.text());
    cmd!("length", |d| d.len() as i64);
    cmd!("word_count", |d| d.word_count() as i64);
    cmd!("paragraph_count", |d| d.paragraphs.len() as i64);
    cmd!("paragraph_text", |d, i: i64| d.paragraphs.get(n(i)).map(|p| p.text()).unwrap_or_default());
    cmd!("current_paragraph", |d| d.locate(d.range().0).0 as i64);
    // Formatting.
    cmd!("bold", |d| d.toggle_bold());
    cmd!("italic", |d| d.toggle_italic());
    cmd!("underline", |d| d.toggle_underline());
    cmd!("strikethrough", |d| d.toggle_strike());
    cmd!("set_bold", |d, on: bool| d.set_run_props(RunProps { bold: Some(on), bold_cs: Some(on), ..Default::default() }));
    cmd!("set_italic", |d, on: bool| d.set_run_props(RunProps { italic: Some(on), italic_cs: Some(on), ..Default::default() }));
    cmd!("font", |d, f: &str| d.set_run_props(RunProps { font: Some(f.to_string()), ..Default::default() }));
    { let d = doc.clone(); e.register_fn("font_size", move |s: f64| d.borrow_mut().set_font_size(s as f32)); }
    { let d = doc.clone(); e.register_fn("font_size", move |s: i64| d.borrow_mut().set_font_size(s as f32)); }
    cmd!("color", |d, c: &str| d.set_run_props(RunProps { color: Some(if c.eq_ignore_ascii_case("auto") { None } else { crate::Color::from_hex(c.trim_start_matches('#')) }), ..Default::default() }));
    cmd!("highlight", |d, h: &str| d.set_run_props(RunProps { highlight: Some(if h.eq_ignore_ascii_case("none") { None } else { Some(h.to_string()) }), ..Default::default() }));
    cmd!("grow_font", |d| d.grow_font(true));
    cmd!("shrink_font", |d| d.grow_font(false));
    cmd!("clear_formatting", |d| d.clear_run_formatting());
    cmd!("style", |d, s: &str| { let id = d.styles.styles.values().find(|st| st.name.eq_ignore_ascii_case(s)).map(|st| st.id.clone()).unwrap_or_else(|| s.replace(' ', "")); d.set_paragraph_style(&id); });
    { let d = doc.clone(); e.register_fn("align", move |a: &str| -> Result<(), Box<EvalAltResult>> { let al = alignment(a).ok_or_else(|| format!("align: \"{a}\" isn't left, center, right or justify"))?; d.borrow_mut().set_alignment(al); Ok(()) }); }
    cmd!("bullets", |d| d.toggle_list(crate::numbering::ListKind::Bullet));
    cmd!("numbering", |d| d.toggle_list(crate::numbering::ListKind::Number));
    { let d = doc.clone(); e.register_fn("space_before", move |v: f64| d.borrow_mut().set_paragraph_spacing(Some(v as f32), None)); }
    { let d = doc.clone(); e.register_fn("space_after", move |v: f64| d.borrow_mut().set_paragraph_spacing(None, Some(v as f32))); }
    { let d = doc.clone(); e.register_fn("line_spacing", move |v: f64| d.borrow_mut().set_line_spacing(crate::format::LineRule::Auto, v as f32)); }
    { let d = doc.clone(); e.register_fn("indent", move |v: f64| d.borrow_mut().set_para_props(crate::format::ParaProps { indent_left: Some((v * 1440.0) as i32), ..Default::default() })); }
    // Find and replace, tables, comments, history.
    cmd!("find", |d, q: &str| { let from = d.range().1; let hits = d.find_all(q, false, false); match hits.iter().find(|h| h.0 >= from).or(hits.first()) { Some(&(a, b)) => { d.select(a, b); true } None => false } });
    cmd!("replace_all", |d, q: &str, w: &str, mc: bool, ww: bool| d.replace_all(q, w, mc, ww) as i64);
    { let d = doc.clone(); e.register_fn("replace_all", move |q: &str, w: &str| d.borrow_mut().replace_all(q, w, false, false) as i64); }
    cmd!("insert_table", |d, r: i64, c: i64| d.insert_table(n(r).clamp(1, 500), n(c).clamp(1, 63)));
    cmd!("add_comment", |d, t: &str| { let author = if d.revision_author.is_empty() { "Macro".to_string() } else { d.revision_author.clone() }; let date = (d.clock)(); d.add_comment(&author, t, &date); });
    cmd!("undo", |d| d.undo());
    cmd!("redo", |d| d.redo());
    e
}

/// Run a macro's script on `doc`. The document keeps every change made before an error.
pub fn run(doc: &mut Document, code: &str) -> RunResult {
    let shared = Rc::new(RefCell::new(std::mem::replace(doc, Document::new(vec![Paragraph::new("")]))));
    let output = Rc::new(RefCell::new(vec![]));
    // The recorder isn't fed by a running macro.
    let recorder = shared.borrow_mut().recorder.take();
    let result = {
        let e = engine(shared.clone(), output.clone());
        let mut scope = Scope::new();
        e.run_with_scope(&mut scope, code)
    };
    let mut back = Rc::try_unwrap(shared).map(|c| c.into_inner()).unwrap_or_else(|rc| std::mem::replace(&mut *rc.borrow_mut(), Document::new(vec![Paragraph::new("")])));
    back.recorder = recorder;
    *doc = back;
    let error = result.err().map(|e| {
        let line = match e.position() { Position::NONE => None, p => p.line() };
        let msg = match *e { EvalAltResult::ErrorTooManyOperations(_) => "The macro ran too long and was stopped (an endless loop?).".to_string(), ref other => other.to_string() };
        (msg, line)
    });
    RunResult { output: Rc::try_unwrap(output).map(|c| c.into_inner()).unwrap_or_default(), error }
}

/// Check a script for syntax errors without running it.
pub fn check(code: &str) -> Result<(), (String, Option<usize>)> {
    let e = engine(Rc::new(RefCell::new(Document::new(vec![Paragraph::new("")]))), Rc::new(RefCell::new(vec![])));
    e.compile(code).map(|_| ()).map_err(|err| (err.to_string(), err.position().line()))
}

/// Is this a usable macro name (letters, digits and underscores, not starting with a digit)?
pub fn valid_name(name: &str) -> bool {
    let mut c = name.chars();
    c.next().is_some_and(|f| f.is_alphabetic() || f == '_') && c.all(|x| x.is_alphanumeric() || x == '_') && name.len() <= 80
}

// ---------------------------------------------------------------------------
// Storage: a customXml part in the document (Word keeps custom XML parts).

pub const NAMESPACE: &str = "urn:word-rust:macros";

fn esc(s: &str) -> String { let mut o = String::new(); crate::xml::escape_into(s, &mut o, true); o }

pub fn macros_xml(list: &[Macro]) -> String {
    let mut out = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><wm:macros xmlns:wm="{NAMESPACE}">"#);
    for m in list {
        out += &format!(r#"<wm:macro wm:name="{}" wm:description="{}"{}>{}</wm:macro>"#, esc(&m.name), esc(&m.description), m.shortcut.as_ref().map(|s| format!(r#" wm:shortcut="{}""#, esc(s))).unwrap_or_default(), esc(&m.code));
    }
    out + "</wm:macros>"
}

pub fn read_macros(text: &str) -> Option<Vec<Macro>> {
    let root = crate::xml::parse(text).ok()?;
    if !root.name.ends_with("macros") || !text.contains(NAMESPACE) { return None; }
    Some(root.elements().filter(|e| e.name.ends_with("macro")).map(|e| Macro {
        name: e.attr("wm:name").unwrap_or_default().to_string(),
        description: e.attr("wm:description").unwrap_or_default().to_string(),
        shortcut: e.attr("wm:shortcut").map(String::from),
        code: e.text(),
    }).collect())
}

#[allow(dead_code)]
fn unused(_: Dynamic) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_replay() {
        let mut d = Document::new(vec![Paragraph::new("")]);
        d.start_recording();
        d.replace("Hello");
        d.replace(" world");
        d.insert_paragraph();
        d.toggle_bold();
        d.replace("Bold");
        d.set_alignment(Alignment::Center);
        d.set_paragraph_style("Heading1");
        let script = d.stop_recording();
        assert_eq!(script, "type_text(\"Hello world\");\nnew_paragraph();\nbold();\ntype_text(\"Bold\");\nalign(\"center\");\nstyle(\"Heading1\");", "{script}");
        let mut e = Document::new(vec![Paragraph::new("")]);
        let r = run(&mut e, &script);
        assert!(r.error.is_none(), "{r:?}");
        assert_eq!(e.text(), d.text());
        assert_eq!(e.paragraphs[1].style.as_deref(), Some("Heading1"));
        assert!(e.run_format(&e.paragraphs[1], &e.paragraphs[1].runs[0]).bold);
    }

    #[test]
    fn scripts_loop_read_and_stop() {
        let mut d = Document::new(vec![Paragraph::new("alpha beta"), Paragraph::new("gamma")]);
        let r = run(&mut d, r#"
            let n = paragraph_count();
            for i in 0..n { goto_paragraph(i); line_end(); type_text("!"); }
            let hits = replace_all("beta", "BETA");
            print(`replaced ${hits}`);
            doc_end(); new_paragraph(); insert_table(2, 2);
        "#);
        assert!(r.error.is_none(), "{r:?}");
        assert_eq!(r.output, vec!["replaced 1"]);
        assert!(d.text().starts_with("alpha BETA!\ngamma!"));
        assert_eq!(d.tables.len(), 1);
        // Errors report their line; endless loops stop.
        let r = run(&mut d, "type_text(\"x\");\nalign(\"sideways\");");
        assert_eq!(r.error.as_ref().and_then(|e| e.1), Some(2));
        assert!(d.text().contains('x'), "work before the error stays");
        let r = run(&mut d, "loop { }");
        assert!(r.error.unwrap().0.contains("too long"));
        assert!(check("let x = ;").is_err() && check("bold();").is_ok());
    }

    #[test]
    fn macros_round_trip_in_docx() {
        let mut d = Document::new(vec![Paragraph::new("x")]);
        d.macros = vec![Macro { name: "Shout".into(), description: "Upper & loud <now>".into(), code: "select_all();\nbold();".into(), shortcut: Some("Cmd+Alt+1".into()) }];
        let e = crate::docx::read(&crate::docx::write(&d).unwrap()).unwrap();
        assert_eq!(e.macros, d.macros);
    }
}
