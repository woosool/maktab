//! Field results that don't depend on pagination: DATE/TIME with Word's date-time pictures
//! (§17.16.4.1) and document-property fields.

use crate::model::Document;

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// (year, month 1–12, day, hour, minute, second) from an ISO date-time.
pub fn parse_iso(iso: &str) -> Option<(i64, u32, u32, u32, u32, u32)> {
    let n = |a: usize, b: usize| iso.get(a..b).and_then(|s| s.parse::<i64>().ok());
    Some((n(0, 4)?, n(5, 7)? as u32, n(8, 10)? as u32, n(11, 13).unwrap_or(0) as u32, n(14, 16).unwrap_or(0) as u32, n(17, 19).unwrap_or(0) as u32))
}

fn weekday(y: i64, m: u32, d: u32) -> usize {
    // Days from civil (Hinnant), 1970-01-01 was a Thursday.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    (days + 3).rem_euclid(7) as usize
}

/// Format an ISO date-time with a Word picture: d dd ddd dddd, M MM MMM MMMM, yy yyyy,
/// h hh H HH, m mm, s ss, am/pm (AM/PM), and 'quoted' literal text.
pub fn format_date(iso: &str, picture: &str) -> String {
    let Some((y, mo, d, h, mi, s)) = parse_iso(iso) else { return String::new() };
    let pic: Vec<char> = picture.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let has_ampm = picture.to_lowercase().contains("am/pm");
    while i < pic.len() {
        let c = pic[i];
        let run = pic[i..].iter().take_while(|&&x| x == c).count();
        match c {
            '\'' => { let end = pic[i + 1..].iter().position(|&x| x == '\'').map(|p| i + 1 + p).unwrap_or(pic.len()); out.extend(&pic[i + 1..end]); i = end + 1; continue; }
            'd' => out += &match run { 1 => d.to_string(), 2 => format!("{d:02}"), 3 => DAYS[weekday(y, mo, d)][..3].to_string(), _ => DAYS[weekday(y, mo, d)].to_string() },
            'M' => out += &match run { 1 => mo.to_string(), 2 => format!("{mo:02}"), 3 => MONTHS[mo as usize - 1][..3].to_string(), _ => MONTHS[mo as usize - 1].to_string() },
            'y' => out += &if run <= 2 { format!("{:02}", y % 100) } else { y.to_string() },
            'h' => { let h12 = if h % 12 == 0 { 12 } else { h % 12 }; out += &if run == 1 { h12.to_string() } else { format!("{h12:02}") }; }
            'H' => out += &if run == 1 { h.to_string() } else { format!("{h:02}") },
            'm' => out += &if run == 1 { mi.to_string() } else { format!("{mi:02}") },
            's' => out += &if run == 1 { s.to_string() } else { format!("{s:02}") },
            'a' | 'A' if has_ampm && pic[i..].iter().collect::<String>().to_lowercase().starts_with("am/pm") => {
                let pm = h >= 12;
                out += if c == 'a' { if pm { "pm" } else { "am" } } else if pm { "PM" } else { "AM" };
                i += 5;
                continue;
            }
            _ => { for _ in 0..run { out.push(c); } }
        }
        i += run;
    }
    out
}

/// Date-time pictures offered by Insert ▸ Date & Time.
pub const DATE_FORMATS: &[&str] = &["M/d/yyyy", "dddd, MMMM d, yyyy", "MMMM d, yyyy", "M/d/yy", "yyyy-MM-dd", "d-MMM-yy", "M.d.yyyy", "MMM. d, yy", "d MMMM yyyy", "MMMM yy", "MMM-yy", "M/d/yyyy h:mm am/pm", "M/d/yyyy h:mm:ss am/pm", "h:mm am/pm", "h:mm:ss am/pm", "HH:mm", "HH:mm:ss"];

/// A property field's value.
pub fn property(doc: &Document, name: &str) -> String {
    let p = &doc.properties;
    match name {
        "AUTHOR" => if p.author.is_empty() { doc.revision_author.clone() } else { p.author.clone() },
        "TITLE" => p.title.clone(),
        "SUBJECT" => p.subject.clone(),
        "KEYWORDS" => p.keywords.clone(),
        "COMMENTS" => p.description.clone(),
        "FILENAME" => if doc.file_name.is_empty() { "Document1".into() } else { doc.file_name.clone() },
        "NUMWORDS" => doc.paragraphs.iter().map(|p| p.text().split_whitespace().count()).sum::<usize>().to_string(),
        "NUMCHARS" => doc.paragraphs.iter().map(|p| p.text().chars().filter(|c| !c.is_whitespace()).count()).sum::<usize>().to_string(),
        _ => String::new(),
    }
}

/// A DATE/TIME field's result now.
pub fn date_value(doc: &Document, picture: &str) -> String {
    let now = (doc.local_clock)();
    let now = if now.is_empty() { (doc.clock)() } else { now };
    let (time, pic) = match picture.strip_prefix("TIME:") { Some(p) => (true, p), None => (false, picture) };
    let pic = if pic.is_empty() { if time { "h:mm am/pm" } else { "M/d/yyyy" } } else { pic };
    format_date(&now, pic)
}

#[cfg(test)]
mod tests {
    #[test]
    fn date_pictures() {
        let t = "2026-10-22T16:05:09";
        assert_eq!(super::format_date(t, "dddd, MMMM d, yyyy"), "Thursday, October 22, 2026");
        assert_eq!(super::format_date(t, "M/d/yy h:mm am/pm"), "10/22/26 4:05 pm");
        assert_eq!(super::format_date(t, "yyyy-MM-dd HH:mm:ss"), "2026-10-22 16:05:09");
        assert_eq!(super::format_date(t, "d MMM 'at' H"), "22 Oct at 16");
    }
}
