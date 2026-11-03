//! Delimited text (io.js: detectDelimiter, sheetToCsv): reading CSV/TSV with quoted fields and
//! writing a sheet's displayed values back out.

use crate::addr::Pos;
use crate::book::Workbook;

/// Splits delimited text into rows of fields; quotes may hold delimiters, newlines and "".
pub fn parse(text: &str, delim: char) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut rows = vec![]; let mut row = vec![]; let mut field = String::new();
    let mut quoted = false; let mut at_start = true;
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if quoted {
            if c == '"' { if it.peek() == Some(&'"') { field.push('"'); it.next(); } else { quoted = false; } } else { field.push(c); }
            continue;
        }
        match c {
            '"' if at_start => { quoted = true; at_start = false; }
            c if c == delim => { row.push(std::mem::take(&mut field)); at_start = true; }
            '\r' => {}
            '\n' => { row.push(std::mem::take(&mut field)); rows.push(std::mem::take(&mut row)); at_start = true; }
            c => { field.push(c); at_start = false; }
        }
    }
    if !field.is_empty() || !row.is_empty() { row.push(field); rows.push(row); }
    rows
}

/// The delimiter a file most likely uses: tab, semicolon, comma or pipe, judged by consistency.
pub fn detect(text: &str, filename: &str) -> char {
    if filename.to_ascii_lowercase().ends_with(".tsv") || filename.to_ascii_lowercase().ends_with(".tab") { return '\t'; }
    let lines: Vec<&str> = text.lines().take(20).filter(|l| !l.trim().is_empty()).collect();
    let mut best = (',', 0usize, false);
    for d in ['\t', ';', ',', '|'] {
        let counts: Vec<usize> = lines.iter().map(|l| parse(l, d).first().map_or(0, |r| r.len() - 1)).collect();
        let k = counts.first().copied().unwrap_or(0);
        let consistent = k > 0 && counts.iter().all(|c| *c == k);
        if (consistent && !best.2) || (consistent == best.2 && k > best.1) { best = (d, k, consistent); }
    }
    best.0
}

/// Reads delimited text into a new workbook (values typed as if entered).
pub fn read(text: &str, filename: &str) -> Workbook {
    let d = detect(text, filename);
    let mut wb = Workbook::new();
    let stem = std::path::Path::new(filename).file_stem().and_then(|s| s.to_str()).unwrap_or("Sheet1");
    if wb.valid_sheet_name(stem, Some(0)).is_ok() { wb.sheets[0].name = stem.chars().take(31).collect(); }
    for (r, row) in parse(text, d).iter().enumerate() {
        for (c, f) in row.iter().enumerate() {
            if !f.is_empty() && r < crate::addr::MAX_ROWS as usize && c < crate::addr::MAX_COLS as usize {
                // A leading = in a CSV is data, not a formula (it may come from anywhere).
                let t = if f.starts_with('=') { format!("'{f}") } else { f.clone() };
                wb.set_input(0, Pos::new(r as u32, c as u32), &t);
            }
        }
    }
    wb.recalc();
    wb
}

fn quote(s: &str, d: char) -> String {
    if s.contains([d, '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// A sheet's displayed values as delimited text.
pub fn write(wb: &Workbook, sheet: usize, d: char) -> String {
    let Some(e) = wb.sheets[sheet].extent() else { return String::new() };
    let mut out = String::new();
    for r in 0..=e.row {
        let line: Vec<String> = (0..=e.col).map(|c| quote(&wb.display(sheet, Pos::new(r, c)), d)).collect();
        out.push_str(line.join(&d.to_string()).trim_end_matches(d));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv() {
        assert_eq!(parse("a,\"b,c\",\"d\"\"e\"\n1,2", ','), vec![vec!["a", "b,c", "d\"e"], vec!["1", "2"]]);
        assert_eq!(detect("a;b;c\n1;2;3", "x.csv"), ';');
        let wb = read("x,y\n1,=1+1\n", "t.csv");
        assert_eq!(wb.display(0, Pos::new(1, 1)), "=1+1");
    }

    #[test]
    fn image_cell_writes_alt_text() {
        let mut wb = Workbook::new();
        wb.set_input(0, Pos::new(0, 0), "=IMAGE(\"https://x.test/a.png\",\"Logo\")");
        wb.recalc();
        assert_eq!(write(&wb, 0, ','), "Logo\r\n");
    }
}
