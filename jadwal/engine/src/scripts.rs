//! Scripts: Jadwal's automation (the JS app's scripts.js, script-runtime, script-recorder,
//! script-gallery). Instead of running JavaScript, Jadwal has a small command language that can
//! only touch the workbook: each line is one command acting on the current selection, so a
//! script can't reach files, the network or the machine, and every run is bounded (a step
//! budget) and undoable as one step. The recorder writes the same language, so a recorded
//! macro is a script you can read and edit.
//!
//! ```text
//! # a comment (a # at the start of a line, or " # " after a command)
//! select A2:A20          # pick cells (on the current sheet)
//! sheet "Data"           # switch sheet
//! offset 1 0 / resize 5 2
//! set B1 Total           # one cell's content (text, number or =formula)
//! value ={value}*2       # every selected cell; {value} {row} {col} {n} are filled in
//! bold on / italic off / underline on
//! fill #FFFF00 / fill none / color #C00000 / format "0.00%" / align center
//! clear contents|formats|all
//! upper / lower / proper / trim     # text in the selection
//! add 5 / multiply 1.1 / round 2    # numbers in the selection
//! sort asc|desc                     # the selection's rows by its first column
//! repeat 3 … end
//! message Done
//! ```
//!
//! Scripts are kept in the workbook as a custom XML part (customXml/item*.xml with a
//! `urn:jadwal:scripts` root), which Excel carries along untouched.

use crate::addr::{parse_a1, parse_range, Pos, Range};
use crate::book::{Sheet, Workbook};
use crate::style::{HAlign, Underline};
use crate::value::Value;
use crate::xlsx::xml::{text, Element};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Script { pub name: String, pub description: String, pub code: String }

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptError { pub line: usize, pub message: String }

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "Line {}: {}", self.line, self.message) }
}

/// What a run left: the sheet and selection it ended on, its messages and cells changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome { pub sheet: usize, pub sel: Range, pub messages: Vec<String>, pub cells: usize }

/// Cell operations a run may do before it stops.
pub const BUDGET: usize = 2_000_000;
const MAX_CELLS: u64 = 1_000_000;

struct Run<'a> { wb: &'a mut Workbook, sheet: usize, sel: Range, messages: Vec<String>, steps: usize, cells: usize }

fn err(line: usize, m: impl Into<String>) -> ScriptError { ScriptError { line, message: m.into() } }

/// Splits a line into words; "quoted text" is one word.
fn words(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut q = false;
    let mut had = false;
    for c in s.chars() {
        match c {
            '"' => { q = !q; had = true; }
            c if c.is_whitespace() && !q => { if !cur.is_empty() || had { out.push(std::mem::take(&mut cur)); } had = false; }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() || had { out.push(cur); }
    out
}

fn strip_comment(l: &str) -> &str {
    let mut q = false;
    for (i, c) in l.char_indices() { if c == '"' { q = !q; } if c == '#' && !q && (l[..i].trim().is_empty() || l[..i].ends_with(char::is_whitespace) && l[i + 1..].chars().next().is_none_or(char::is_whitespace)) { return &l[..i]; } }
    l
}

fn color(s: &str) -> Option<Option<[u8; 3]>> {
    if s.eq_ignore_ascii_case("none") { return Some(None); }
    let h = s.trim_start_matches('#');
    if h.len() != 6 { return None; }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]))
}

fn on_off(w: Option<&String>) -> bool { !w.is_some_and(|w| w.eq_ignore_ascii_case("off") || w == "0" || w.eq_ignore_ascii_case("false")) }

impl Run<'_> {
    fn cells(&self) -> impl Iterator<Item = Pos> + use<> { let r = self.sel; (r.r0..=r.r1).flat_map(move |row| (r.c0..=r.c1).map(move |c| Pos::new(row, c))) }
    fn tick(&mut self, line: usize, n: usize) -> Result<(), ScriptError> {
        self.steps += n;
        if self.steps > BUDGET { return Err(err(line, "The script did too much work and was stopped.")); }
        Ok(())
    }
    fn check_size(&self, line: usize) -> Result<(), ScriptError> {
        let n = (self.sel.r1 - self.sel.r0 + 1) as u64 * (self.sel.c1 - self.sel.c0 + 1) as u64;
        if n > MAX_CELLS { return Err(err(line, "The selection is too large for a script.")); }
        Ok(())
    }
    fn map_cells(&mut self, line: usize, f: &dyn Fn(&Value, &str) -> Option<String>) -> Result<(), ScriptError> {
        self.check_size(line)?;
        let si = self.sheet;
        let ps: Vec<Pos> = self.cells().filter(|p| self.wb.sheets[si].cells.contains_key(p)).collect();
        self.tick(line, ps.len() + 1)?;
        for p in ps {
            let v = self.wb.sheets[si].value(p);
            if self.wb.edit_text(si, p).starts_with('=') { continue; }
            if let Some(t) = f(&v, &self.wb.display(si, p)) { self.wb.set_input(si, p, &t); self.cells += 1; }
        }
        Ok(())
    }

    fn block(&mut self, lines: &[(usize, Vec<String>)]) -> Result<(), ScriptError> {
        let mut i = 0;
        while i < lines.len() {
            let (ln, w) = &lines[i];
            let ln = *ln;
            let cmd = w[0].to_ascii_lowercase();
            let arg = |k: usize| w.get(k).cloned().unwrap_or_default();
            let rest = || w[1..].join(" ");
            self.tick(ln, 1)?;
            match cmd.as_str() {
                "repeat" => {
                    let n: usize = arg(1).parse().map_err(|_| err(ln, "repeat needs a count"))?;
                    if n > 10_000 { return Err(err(ln, "repeat can run at most 10000 times")); }
                    let end = matching_end(lines, i).ok_or_else(|| err(ln, "repeat without end"))?;
                    for _ in 0..n { self.block(&lines[i + 1..end])?; }
                    i = end;
                }
                "end" => return Err(err(ln, "end without repeat")),
                "select" => {
                    let a = arg(1);
                    self.sel = parse_range(&a).or_else(|| parse_a1(&a).map(|p| Range::new(p, p))).ok_or_else(|| err(ln, format!("{a} isn't a cell or range")))?;
                }
                "sheet" => { let n = rest(); self.sheet = self.wb.sheet_index(&n).ok_or_else(|| err(ln, format!("There's no sheet named {n}")))?; }
                "offset" | "resize" => {
                    let (a, b): (i64, i64) = (arg(1).parse().map_err(|_| err(ln, "needs two numbers"))?, arg(2).parse().map_err(|_| err(ln, "needs two numbers"))?);
                    let r = self.sel;
                    let clamp = |v: i64, max: u32| v.clamp(0, max as i64 - 1) as u32;
                    self.sel = if cmd == "offset" { Range { r0: clamp(r.r0 as i64 + a, crate::addr::MAX_ROWS), r1: clamp(r.r1 as i64 + a, crate::addr::MAX_ROWS), c0: clamp(r.c0 as i64 + b, crate::addr::MAX_COLS), c1: clamp(r.c1 as i64 + b, crate::addr::MAX_COLS) } }
                        else { if a < 1 || b < 1 { return Err(err(ln, "resize needs sizes of at least 1")); } Range { r1: clamp(r.r0 as i64 + a - 1, crate::addr::MAX_ROWS), c1: clamp(r.c0 as i64 + b - 1, crate::addr::MAX_COLS), ..r } };
                }
                "set" => {
                    let p = parse_a1(&arg(1)).ok_or_else(|| err(ln, format!("{} isn't a cell", arg(1))))?;
                    let t = w[2..].join(" ");
                    self.wb.set_input(self.sheet, p, &t);
                    self.cells += 1;
                }
                "value" => {
                    self.check_size(ln)?;
                    let tpl = rest();
                    let r0 = self.sel;
                    let ps: Vec<Pos> = self.cells().collect();
                    self.tick(ln, ps.len())?;
                    for (n, p) in ps.into_iter().enumerate() {
                        let cur = self.wb.display(self.sheet, p);
                        let t = tpl.replace("{value}", &cur).replace("{row}", &(p.row + 1).to_string()).replace("{col}", &(p.col - r0.c0 + 1).to_string()).replace("{n}", &(n + 1).to_string());
                        self.wb.set_input(self.sheet, p, &t);
                        self.cells += 1;
                    }
                }
                "bold" | "italic" | "underline" | "fill" | "color" | "format" | "align" => {
                    self.check_size(ln)?;
                    let on = on_off(w.get(1));
                    let a = arg(1);
                    let r = [self.sel];
                    match cmd.as_str() {
                        "bold" => self.wb.apply_style(self.sheet, &r, &|s| s.bold = on),
                        "italic" => self.wb.apply_style(self.sheet, &r, &|s| s.italic = on),
                        "underline" => self.wb.apply_style(self.sheet, &r, &|s| s.underline = if on { Underline::Single } else { Underline::None }),
                        "fill" => { let c = color(&a).ok_or_else(|| err(ln, "fill needs a color like #FFFF00 or none"))?; self.wb.apply_style(self.sheet, &r, &|s| s.fill = c) }
                        "color" => { let c = color(&a).ok_or_else(|| err(ln, "color needs a color like #C00000"))?; self.wb.apply_style(self.sheet, &r, &|s| s.color = c) }
                        "format" => { let f = rest(); self.wb.apply_style(self.sheet, &r, &|s| s.num_fmt = f.clone()) }
                        _ => {
                            let h = match a.to_ascii_lowercase().as_str() { "left" => HAlign::Left, "center" => HAlign::Center, "right" => HAlign::Right, "general" => HAlign::General, _ => return Err(err(ln, "align needs left, center, right or general")) };
                            self.wb.apply_style(self.sheet, &r, &|s| s.h = h)
                        }
                    }
                    self.cells += 1;
                }
                "clear" => {
                    let what = arg(1).to_ascii_lowercase();
                    let (c, f, a) = match what.as_str() { "" | "contents" => (true, false, false), "formats" => (false, true, false), "all" => (true, true, true), _ => return Err(err(ln, "clear needs contents, formats or all")) };
                    self.wb.clear(self.sheet, self.sel, c, f, a);
                    self.cells += 1;
                }
                "upper" => self.map_cells(ln, &|v, _| if let Value::Str(s) = v { Some(s.to_uppercase()) } else { None })?,
                "lower" => self.map_cells(ln, &|v, _| if let Value::Str(s) = v { Some(s.to_lowercase()) } else { None })?,
                "trim" => self.map_cells(ln, &|v, _| if let Value::Str(s) = v { Some(s.split_whitespace().collect::<Vec<_>>().join(" ")) } else { None })?,
                "proper" => self.map_cells(ln, &|v, _| if let Value::Str(s) = v { Some(s.split(' ').map(|w| { let mut c = w.chars(); c.next().map(|f| format!("{}{}", f.to_uppercase(), c.as_str().to_lowercase())).unwrap_or_default() }).collect::<Vec<_>>().join(" ")) } else { None })?,
                "add" | "multiply" | "round" => {
                    let k: f64 = arg(1).parse().map_err(|_| err(ln, format!("{cmd} needs a number")))?;
                    let op = cmd.clone();
                    self.map_cells(ln, &|v, _| if let Value::Num(n) = v { Some(num_text(match op.as_str() { "add" => n + k, "multiply" => n * k, _ => { let m = 10f64.powi(k as i32); (n * m).round() / m } })) } else { None })?
                }
                "sort" => {
                    let asc = !arg(1).eq_ignore_ascii_case("desc");
                    self.check_size(ln)?;
                    self.wb.sort(self.sheet, self.sel, &[(self.sel.c0, asc)], false, false);
                    self.cells += 1;
                }
                "message" => self.messages.push(rest()),
                _ => return Err(err(ln, format!("Unknown command: {}", w[0]))),
            }
            i += 1;
        }
        Ok(())
    }
}

fn num_text(n: f64) -> String { if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{}", (n * 1e10).round() / 1e10) } }

fn matching_end(lines: &[(usize, Vec<String>)], i: usize) -> Option<usize> {
    let mut depth = 0;
    for (k, (_, w)) in lines.iter().enumerate().skip(i) {
        match w[0].to_ascii_lowercase().as_str() { "repeat" => depth += 1, "end" => { depth -= 1; if depth == 0 { return Some(k); } } _ => {} }
    }
    None
}

/// Parses a script into (line number, words) without running it; errors name the line.
pub fn check(code: &str) -> Result<Vec<(usize, Vec<String>)>, ScriptError> {
    let lines: Vec<(usize, Vec<String>)> = code.lines().enumerate().map(|(i, l)| (i + 1, words(strip_comment(l)))).filter(|(_, w)| !w.is_empty()).collect();
    let mut depth: i32 = 0;
    for (ln, w) in &lines {
        match w[0].to_ascii_lowercase().as_str() { "repeat" => depth += 1, "end" => { depth -= 1; if depth < 0 { return Err(err(*ln, "end without repeat")); } } _ => {} }
    }
    if depth > 0 { return Err(err(lines.last().map_or(1, |l| l.0), "repeat without end")); }
    Ok(lines)
}

/// Runs a script on sheet `sheet` starting with selection `sel`.
pub fn run(wb: &mut Workbook, sheet: usize, sel: Range, code: &str) -> Result<Outcome, ScriptError> {
    let lines = check(code)?;
    let mut r = Run { wb, sheet, sel, messages: vec![], steps: 0, cells: 0 };
    let res = r.block(&lines);
    r.wb.recalc();
    res?;
    Ok(Outcome { sheet: r.sheet, sel: r.sel, messages: r.messages, cells: r.cells })
}

/// The sample scripts.
pub fn gallery() -> Vec<Script> {
    let s = |n: &str, d: &str, c: &str| Script { name: n.into(), description: d.into(), code: c.into() };
    vec![
        s("Highlight selection", "Bold text on a yellow fill.", "# Make the selection stand out\nbold on\nfill #FFF2CC\n"),
        s("Clean up text", "Trim extra spaces and use Proper Case.", "trim\nproper\n"),
        s("Uppercase", "Change text in the selection to UPPERCASE.", "upper\n"),
        s("Increase by 10%", "Multiply numbers in the selection by 1.1.", "multiply 1.1\nround 2\n"),
        s("Number rows", "Fill the selection with 1, 2, 3…", "value {n}\n"),
        s("Header row", "Format the selection as a header: bold, shaded, centered.", "bold on\nfill #DDEBF7\nalign center\n"),
        s("Mark as done", "Fill the selection with a ✓ and a green fill.", "value ✓\nfill #E2EFDA\ncolor #375623\nalign center\n"),
        s("Banded rows", "Shade every other row of the selection.", "# Shades rows 1, 3, 5… of a 10-row block\nresize 1 1\nrepeat 5\n  fill #F2F2F2\n  offset 2 0\nend\n"),
        s("Clear formatting", "Remove formats from the selection.", "clear formats\n"),
    ]
}

// ---- storage ----

const NS: &str = "urn:jadwal:scripts";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml";

fn part_of(wb: &Workbook) -> Option<String> {
    wb.parts.iter().find(|(k, v)| k.starts_with("customXml/item") && !k.contains("Props") && !k.contains("_rels") && String::from_utf8_lossy(v).contains(NS)).map(|(k, _)| k.clone())
}

/// The scripts stored in the workbook.
pub fn load(wb: &Workbook) -> Vec<Script> {
    let Some(p) = part_of(wb) else { return vec![] };
    let Ok(root) = Element::parse(&crate::xlsx::xml::part_text(&wb.parts[&p])) else { return vec![] };
    root.all("script").map(|e| Script { name: e.attr("name").unwrap_or("Script").into(), description: e.attr("description").unwrap_or("").into(), code: e.text() }).collect()
}

/// Stores the scripts in the workbook (or removes the part when there are none).
pub fn store(wb: &mut Workbook, scripts: &[Script]) {
    let existing = part_of(wb);
    let rels_name = "xl/_rels/workbook.xml.rels";
    if scripts.is_empty() {
        if let Some(p) = existing { wb.parts.remove(&p); }
        return;
    }
    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<scripts xmlns=\"{NS}\">");
    for s in scripts { x.push_str(&format!("<script name=\"{}\" description=\"{}\" xml:space=\"preserve\">{}</script>", crate::xlsx::xml::attr(&s.name), crate::xlsx::xml::attr(&s.description), text(&s.code))); }
    x.push_str("</scripts>");
    let path = existing.unwrap_or_else(|| (1..).map(|i| format!("customXml/item{i}.xml")).find(|n| !wb.parts.contains_key(n)).unwrap());
    wb.parts.insert(path.clone(), x.into_bytes());
    // The workbook's relationship to it (the writer keeps customXml relationships it finds here).
    let target = format!("../{path}");
    let rels = wb.parts.get(rels_name).map(|b| crate::xlsx::xml::part_text(b)).unwrap_or_else(|| "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>".into());
    if !rels.contains(&format!("Target=\"{target}\"")) {
        let id = (1..).map(|i| format!("rIdJs{i}")).find(|id| !rels.contains(&format!("Id=\"{id}\""))).unwrap();
        let rels = rels.replace("</Relationships>", &format!("<Relationship Id=\"{id}\" Type=\"{REL}\" Target=\"{target}\"/></Relationships>"));
        wb.parts.insert(rels_name.into(), rels.into_bytes());
    }
}

// ---- recording ----

fn hex(c: [u8; 3]) -> String { format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]) }

/// The commands that turn sheet `before` into sheet `si` of `after`: what the recorder writes
/// after each change while recording (contents first, then formatting, cell by cell).
pub fn record_diff(before: &Sheet, before_styles: &crate::style::Styles, after: &Workbook, si: usize) -> Vec<String> {
    let s = &after.sheets[si];
    let mut out = vec![];
    let mut keys: Vec<Pos> = before.cells.keys().chain(s.cells.keys()).copied().collect();
    keys.sort_by_key(|p| (p.row, p.col));
    keys.dedup();
    if keys.len() > 5000 { return vec!["# (a change to more than 5000 cells wasn't recorded)".into()]; }
    let d1904 = after.date1904;
    for p in keys {
        let (a, b) = (before.cells.get(&p), s.cells.get(&p));
        let ta = a.map(|c| c.edit_text(&before_styles.get(c.style).num_fmt, d1904)).unwrap_or_default();
        let tb = b.map(|c| c.edit_text(&after.styles.get(c.style).num_fmt, d1904)).unwrap_or_default();
        let (sa, sb) = (before_styles.get(a.map_or(0, |c| c.style)).clone(), after.styles.get(b.map_or(0, |c| c.style)).clone());
        if ta != tb { out.push(if tb.is_empty() { format!("select {}\nclear contents", p.a1()) } else { format!("set {} {}", p.a1(), tb) }); }
        if sa != sb {
            let mut st = vec![];
            if sa.bold != sb.bold { st.push(format!("bold {}", if sb.bold { "on" } else { "off" })); }
            if sa.italic != sb.italic { st.push(format!("italic {}", if sb.italic { "on" } else { "off" })); }
            if sa.underline != sb.underline { st.push(format!("underline {}", if sb.underline != Underline::None { "on" } else { "off" })); }
            if sa.fill != sb.fill { st.push(format!("fill {}", sb.fill.map_or("none".into(), hex))); }
            if sa.color != sb.color { st.push(format!("color {}", sb.color.map_or("#000000".into(), hex))); }
            if sa.num_fmt != sb.num_fmt { st.push(format!("format \"{}\"", sb.num_fmt)); }
            if sa.h != sb.h { st.push(format!("align {}", match sb.h { HAlign::Left => "left", HAlign::Center => "center", HAlign::Right => "right", _ => "general" })); }
            if !st.is_empty() { out.push(format!("select {}\n{}", p.a1(), st.join("\n"))); }
        }
    }
    // Consecutive cells given the same formatting become one range.
    merge_selects(out)
}

fn merge_selects(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<(Option<Range>, String)> = vec![];
    for l in lines {
        if let Some((sel, body)) = l.strip_prefix("select ").and_then(|x| x.split_once('\n')) {
            let p = parse_a1(sel).unwrap();
            if let Some((Some(r), b)) = out.last_mut() {
                if b == body && ((r.r0 == r.r1 && p.row == r.r0 && p.col == r.c1 + 1) || (r.c0 == p.col && r.c1 == p.col && p.row == r.r1 + 1)) { if p.row == r.r0 && r.r0 == r.r1 { r.c1 = p.col; } else { r.r1 = p.row; } continue; }
            }
            out.push((Some(Range::new(p, p)), body.to_string()));
        } else { out.push((None, l)); }
    }
    out.into_iter().map(|(r, b)| match r { Some(r) => format!("select {}\n{b}", if r.r0 == r.r1 && r.c0 == r.c1 { r.start().a1() } else { r.a1() }), None => b }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Workbook {
        let mut wb = Workbook::new();
        for (i, t) in ["  alpha  beta ", "gamma", "3", "4.5"].iter().enumerate() { wb.set_input(0, Pos::new(i as u32, 0), t); }
        wb
    }

    #[test]
    fn runs_commands() {
        let mut wb = book();
        let sel = parse_range("A1:A4").unwrap();
        let o = run(&mut wb, 0, sel, "trim\nproper\nmultiply 2\nselect B1:B3\nvalue {n}\nbold on\nfill #FFFF00\nset C1 =SUM(B1:B3)\nmessage done # a comment\n").unwrap();
        assert_eq!(wb.display(0, Pos::new(0, 0)), "Alpha Beta");
        assert_eq!(wb.display(0, Pos::new(3, 0)), "9");
        assert_eq!(wb.display(0, Pos::new(0, 2)), "6");
        assert!(wb.style_at(0, Pos::new(1, 1)).bold && wb.style_at(0, Pos::new(1, 1)).fill == Some([255, 255, 0]));
        assert_eq!(o.messages, vec!["done"]);
        assert_eq!(o.sel, parse_range("B1:B3").unwrap());
        let mut wb = book();
        run(&mut wb, 0, Range::new(Pos::new(0, 3), Pos::new(0, 3)), "repeat 3\n value x{row}\n offset 1 0\nend").unwrap();
        assert_eq!(wb.display(0, Pos::new(2, 3)), "x3");
    }

    #[test]
    fn errors_and_limits() {
        let mut wb = book();
        let sel = Range::new(Pos::new(0, 0), Pos::new(0, 0));
        assert_eq!(run(&mut wb, 0, sel, "bold on\nfrobnicate").unwrap_err().line, 2);
        assert_eq!(run(&mut wb, 0, sel, "repeat 2\nbold").unwrap_err().message, "repeat without end");
        assert!(run(&mut wb, 0, sel, "repeat 10000\nrepeat 10000\nbold on\nend\nend").unwrap_err().message.contains("too much"));
        for g in gallery() { assert!(check(&g.code).is_ok(), "{}", g.name); }
    }

    #[test]
    fn stores_and_records() {
        let mut wb = book();
        let scripts = vec![Script { name: "Mine".into(), description: "d <&>".into(), code: "bold on\nvalue \"a & b\"\n".into() }];
        store(&mut wb, &scripts);
        let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
        assert_eq!(load(&back), scripts);
        // Saving again keeps one part.
        let mut b2 = back.clone();
        store(&mut b2, &scripts);
        assert_eq!(b2.parts.keys().filter(|k| k.starts_with("customXml/item")).count(), 1);
        // Recording: diff two states, then replay on a fresh copy.
        let before = wb.sheets[0].clone();
        let styles = wb.styles.clone();
        wb.set_input(0, Pos::new(5, 1), "hello");
        wb.apply_style(0, &[parse_range("A1:C1").unwrap()], &|s| s.bold = true);
        let code = record_diff(&before, &styles, &wb, 0).join("\n");
        assert!(code.contains("set B6 hello") && code.contains("select A1:C1\nbold on"), "{code}");
        let mut fresh = book();
        run(&mut fresh, 0, Range::default(), &code).unwrap();
        assert_eq!(fresh.display(0, Pos::new(5, 1)), "hello");
        assert!(fresh.style_at(0, Pos::new(0, 2)).bold);
    }
}
