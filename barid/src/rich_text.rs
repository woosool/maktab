use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MessageMode {
    #[default]
    Html,
    Plain,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FontFamily {
    #[default]
    Sans,
    Serif,
    Mono,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Mark {
    Family(FontFamily),
    Highlight([u8; 3]),
    Superscript,
    Subscript,
    Bold,
    Italic,
    Underline,
    Strike,
    Color([u8; 3]),
    Size(u16),
    Link(String),
}
impl Mark {
    pub fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub mark: Mark,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlineImage {
    pub cid: String,
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
    pub position: usize,
    pub width: u32,
    pub alt: String,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct RichBody {
    #[serde(default)]
    pub paragraphs: Vec<RichParagraph>,
    #[serde(default)]
    pub rules: Vec<std::ops::Range<usize>>,
    #[serde(default)]
    pub tables: Vec<RichTable>,
    pub spans: Vec<Span>,
    pub images: Vec<InlineImage>,
    pub alignment: Alignment,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RichParagraph {
    pub start: usize,
    pub end: usize,
    pub indent: f32,
    pub before: f32,
    pub after: f32,
    pub alignment: Alignment,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RichTable {
    #[serde(default)]
    pub columns: Vec<u32>,
    pub start: usize,
    pub end: usize,
    pub rows: Vec<Vec<RichCell>>,
    pub border: bool,
    pub background: Option<[u8; 3]>,
    pub width: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RichCell {
    #[serde(default = "cell_padding")]
    pub padding: [u8; 2],
    pub text: String,
    pub rich: Box<RichBody>,
    pub colspan: usize,
    pub rowspan: usize,
    pub background: Option<[u8; 3]>,
    pub border: bool,
    pub width: Option<u32>,
}
fn cell_padding() -> [u8; 2] {
    [6, 6]
}
impl RichTable {
    fn html(&self) -> String {
        let mut html = format!(
            "<table cellpadding=\"6\" cellspacing=\"0\" style=\"border-collapse:collapse;{}{}\">",
            if self.border {
                "border:1px solid #c8cdd3;"
            } else {
                ""
            },
            self.background
                .map(|c| format!("background-color:#{:02x}{:02x}{:02x};", c[0], c[1], c[2]))
                .unwrap_or_default()
        );
        if let Some(width) = self.width {
            html = html.replacen("<table ", &format!("<table width=\"{width}\" "), 1);
        }
        if !self.columns.is_empty() {
            html.push_str("<colgroup>");
            for width in &self.columns {
                html.push_str(&format!("<col width=\"{width}\">"));
            }
            html.push_str("</colgroup>");
        }
        for row in &self.rows {
            html.push_str("<tr>");
            for cell in row {
                html.push_str(&format!(
                    "<td colspan=\"{}\" rowspan=\"{}\" style=\"vertical-align:top;{}{}\">",
                    cell.colspan,
                    cell.rowspan,
                    if self.border || cell.border {
                        "border:1px solid #c8cdd3;"
                    } else {
                        ""
                    },
                    cell.background
                        .map(|c| format!("background-color:#{:02x}{:02x}{:02x};", c[0], c[1], c[2]))
                        .unwrap_or_default()
                ));
                let cell_start = html.rfind("<td ").expect("Cell opening tag");
                let style = html[cell_start..]
                    .find("vertical-align:top;")
                    .expect("Cell style")
                    + cell_start;
                html.insert_str(
                    style,
                    &format!("padding:{}px {}px;", cell.padding[1], cell.padding[0]),
                );
                if let Some(width) = cell.width {
                    let insert = html.rfind("<td ").expect("Cell opening tag");
                    html.insert_str(insert + 4, &format!("width=\"{width}\" "));
                }
                html.push_str(&cell.rich.html(&cell.text));
                html.push_str("</td>");
            }
            html.push_str("</tr>");
        }
        html.push_str("</table>");
        html
    }
}
impl RichBody {
    pub fn all_images(&self) -> Vec<&InlineImage> {
        let mut images: Vec<_> = self.images.iter().collect();
        for table in &self.tables {
            for row in &table.rows {
                for cell in row {
                    images.extend(cell.rich.all_images());
                }
            }
        }
        images
    }
    pub fn visit_images_mut(&mut self, f: &mut impl FnMut(&mut InlineImage)) {
        for image in &mut self.images {
            f(image);
        }
        for table in &mut self.tables {
            for row in &mut table.rows {
                for cell in row {
                    cell.rich.visit_images_mut(f);
                }
            }
        }
    }

    pub fn apply(&mut self, start: usize, end: usize, mark: Mark) {
        if start >= end {
            return;
        }
        let toggle = matches!(
            mark,
            Mark::Bold
                | Mark::Italic
                | Mark::Underline
                | Mark::Strike
                | Mark::Superscript
                | Mark::Subscript
        ) && (start..end).all(|i| {
            self.spans
                .iter()
                .any(|s| s.start <= i && i < s.end && s.mark == mark)
        });
        let mut result = Vec::new();
        for span in self.spans.drain(..) {
            if span.mark.same_kind(&mark) && span.start < end && span.end > start {
                if span.start < start {
                    result.push(Span {
                        start: span.start,
                        end: start,
                        mark: span.mark.clone(),
                    });
                }
                if span.end > end {
                    result.push(Span {
                        start: end,
                        end: span.end,
                        mark: span.mark,
                    });
                }
            } else {
                result.push(span);
            }
        }
        if !toggle {
            result.push(Span { start, end, mark });
        }
        self.spans = result;
    }
    pub fn clear(&mut self, start: usize, end: usize) {
        let mut result = Vec::new();
        for span in self.spans.drain(..) {
            if span.start < end && span.end > start {
                if span.start < start {
                    result.push(Span {
                        start: span.start,
                        end: start,
                        mark: span.mark.clone(),
                    });
                }
                if span.end > end {
                    result.push(Span {
                        start: end,
                        end: span.end,
                        mark: span.mark,
                    });
                }
            } else {
                result.push(span);
            }
        }
        self.spans = result;
    }
    pub fn rebase(&mut self, old: &str, new: &str) -> std::ops::Range<usize> {
        let old: Vec<char> = old.chars().collect();
        let new: Vec<char> = new.chars().collect();
        let mut prefix = 0;
        while prefix < old.len().min(new.len()) && old[prefix] == new[prefix] {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < old.len() - prefix
            && suffix < new.len() - prefix
            && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
        {
            suffix += 1;
        }
        let old_end = old.len() - suffix;
        let new_end = new.len() - suffix;
        let delta = new.len() as isize - old.len() as isize;
        let shift = |v: usize| v.saturating_add_signed(delta);
        self.paragraphs.retain_mut(|p| {
            if old_end <= p.start {
                p.start = shift(p.start);
                p.end = shift(p.end);
                true
            } else {
                prefix >= p.end
            }
        });
        self.rules.retain_mut(|range| {
            if old_end <= range.start {
                range.start = shift(range.start);
                range.end = shift(range.end);
                true
            } else {
                prefix >= range.end
            }
        });
        self.tables.retain_mut(|table| {
            if old_end <= table.start {
                table.start = shift(table.start);
                table.end = shift(table.end);
                true
            } else {
                prefix >= table.end
            }
        });
        for span in &mut self.spans {
            if span.end <= prefix {
                continue;
            }
            if span.start >= old_end {
                span.start = shift(span.start);
                span.end = shift(span.end);
            } else {
                span.start = span.start.min(prefix);
                span.end = if span.end >= old_end {
                    shift(span.end)
                } else {
                    new_end
                };
            }
        }
        self.spans
            .retain(|s| s.start < s.end && s.start < new.len());
        for image in &mut self.images {
            if image.position >= old_end {
                image.position = shift(image.position);
            } else if image.position > prefix {
                image.position = new_end;
            }
            image.position = image.position.min(new.len());
        }
        prefix..new_end
    }
    pub fn html(&self, text: &str) -> String {
        let align = match self.alignment {
            Alignment::Left => "left",
            Alignment::Center => "center",
            Alignment::Right => "right",
        };
        let mut html = format!(
            "<div style=\"font-family:Arial,sans-serif;font-size:14px;text-align:{align};white-space:pre-wrap\">"
        );
        let chars: Vec<char> = text.chars().collect();
        for i in 0..=chars.len() {
            for _ in self.paragraphs.iter().filter(|p| p.end == i) {
                html.push_str("</p>");
            }
            for p in self.paragraphs.iter().filter(|p| p.start == i && p.end > i) {
                let align = match p.alignment {
                    Alignment::Left => "left",
                    Alignment::Center => "center",
                    Alignment::Right => "right",
                };
                html.push_str(&format!(
                    "<p style=\"margin-top:{}px;margin-bottom:{}px;margin-left:{}px;text-align:{}\">",
                    p.before, p.after, p.indent, align
                ));
            }

            if self.rules.iter().any(|r| r.start == i) {
                html.push_str("<hr>");
            }
            if self.rules.iter().any(|r| r.start <= i && i < r.end) {
                continue;
            }
            if let Some(table) = self.tables.iter().find(|t| t.start == i) {
                html.push_str(&table.html());
            }
            if self.tables.iter().any(|t| t.start <= i && i < t.end) {
                continue;
            }

            for image in self
                .images
                .iter()
                .filter(|image| image.position.min(chars.len()) == i)
            {
                html.push_str(&format!("<img src=\"cid:{}\" alt=\"{}\" width=\"{}\" style=\"max-width:100%;height:auto\">",escape(&image.cid),escape(&image.alt),image.width.clamp(40,1200)));
            }
            if i == chars.len() {
                break;
            }
            let mut style = String::new();
            let mut link = None;
            let mut underline = false;
            let mut strike = false;
            for span in self
                .spans
                .iter()
                .filter(|span| span.start <= i && i < span.end)
            {
                match &span.mark {
                    Mark::Family(family) => style.push_str(match family {
                        FontFamily::Sans => "font-family:Arial,sans-serif;",
                        FontFamily::Serif => "font-family:Georgia,serif;",
                        FontFamily::Mono => "font-family:monospace;",
                    }),
                    Mark::Highlight(c) => style.push_str(&format!(
                        "background-color:#{:02x}{:02x}{:02x};",
                        c[0], c[1], c[2]
                    )),
                    Mark::Superscript => style.push_str("vertical-align:super;font-size:smaller;"),
                    Mark::Subscript => style.push_str("vertical-align:sub;font-size:smaller;"),
                    Mark::Bold => style.push_str("font-weight:bold;"),
                    Mark::Italic => style.push_str("font-style:italic;"),
                    Mark::Underline => underline = true,
                    Mark::Strike => strike = true,
                    Mark::Color(c) => {
                        style.push_str(&format!("color:#{:02x}{:02x}{:02x};", c[0], c[1], c[2]))
                    }
                    Mark::Size(size) => {
                        style.push_str(&format!("font-size:{}px;", size.clamp(&10, &48)))
                    }
                    Mark::Link(url) => {
                        if safe_link(url) {
                            link = Some(url);
                        }
                    }
                }
            }
            if underline || strike {
                style.push_str(&format!(
                    "text-decoration-line:{}{};",
                    if underline { "underline " } else { "" },
                    if strike { "line-through" } else { "" }
                ));
            }
            if let Some(url) = link {
                html.push_str(&format!("<a href=\"{}\">", escape(url)));
            }
            if !style.is_empty() {
                html.push_str(&format!("<span style=\"{style}\">"));
            }
            if chars[i] == '\n' {
                html.push_str("<br>");
            } else {
                html.push_str(&escape(&chars[i].to_string()));
            }
            if !style.is_empty() {
                html.push_str("</span>");
            }
            if link.is_some() {
                html.push_str("</a>");
            }
        }
        html.push_str("</div>");
        html
    }
}
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn safe_link(url: &str) -> bool {
    !url.chars().any(char::is_control)
        && ["https://", "http://", "mailto:"]
            .iter()
            .any(|scheme| url.to_ascii_lowercase().starts_with(scheme))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formatting_can_be_toggled_without_removing_neighboring_ranges() {
        let mut rich = RichBody::default();
        rich.apply(0, 8, Mark::Bold);
        rich.apply(2, 5, Mark::Bold);
        assert!(rich.spans.iter().any(|s| s.start == 0 && s.end == 2));
        assert!(rich.spans.iter().any(|s| s.start == 5 && s.end == 8));
        assert!(!rich.spans.iter().any(|s| s.start <= 3 && s.end > 3));
        rich.apply(2, 5, Mark::Italic);
        rich.clear(3, 4);
        assert!(
            rich.spans
                .iter()
                .any(|s| s.start == 2 && s.end == 3 && s.mark == Mark::Italic)
        );
    }
    #[test]
    fn unicode_edits_move_styles_and_inline_image_positions_by_characters() {
        let mut rich = RichBody::default();
        rich.apply(0, 4, Mark::Bold);
        rich.images.push(InlineImage {
            cid: "test@barid".into(),
            name: "test.png".into(),
            mime: "image/png".into(),
            data: vec![],
            position: 5,
            width: 200,
            alt: "Picture".into(),
        });
        rich.rebase("café ✉", "Hi café ✉");
        assert_eq!((rich.spans[0].start, rich.spans[0].end), (3, 7));
        assert_eq!(rich.images[0].position, 8);
        rich.rebase("Hi café ✉", "Hi caZZfé ✉");
        assert_eq!(rich.spans[0].end, 9);
        assert_eq!(rich.images[0].position, 10);
    }
    #[test]
    fn html_escapes_untrusted_text_links_and_image_descriptions() {
        let mut rich = RichBody::default();
        rich.apply(0, 50, Mark::Link("javascript:alert(1)".into()));
        rich.apply(0, 3, Mark::Bold);
        rich.images.push(InlineImage {
            cid: "test@barid".into(),
            name: "test.png".into(),
            mime: "image/png".into(),
            data: vec![],
            position: 0,
            width: 5000,
            alt: "\" onerror=\"bad()".into(),
        });
        let html = rich.html("<script>&");
        assert!(!html.contains("<script>"));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("font-weight:bold"));
        assert!(html.contains("&quot; onerror=&quot;bad()"));
        assert!(html.contains("width=\"1200\""));
        assert!(!safe_link("https://example.com\r\nBad"));
        assert!(safe_link("mailto:hello@example.com"));
    }
}
