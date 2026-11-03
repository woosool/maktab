use crate::rich_text::{Alignment, FontFamily, InlineImage, Mark, RichBody, Span, safe_link};
use base64::Engine;
use scraper::{ElementRef, Html, Node, Selector};

#[derive(Default, Clone)]
pub struct Fragment {
    pub text: String,
    pub rich: RichBody,
    pub skipped_images: usize,
}
struct Rule {
    selector: Selector,
    style: String,
    weight: usize,
}
struct Importer<'a> {
    resolve_image: &'a dyn Fn(&str) -> Option<InlineImage>,
    fragment: Fragment,
    rules: Vec<Rule>,
    count: usize,
    table_depth: usize,
    stylesheet: String,
}
pub fn import_html(source: &str) -> Result<Fragment, String> {
    import_html_with_images(source, &|_| None)
}
pub fn import_html_with_images(
    source: &str,
    resolve_image: &dyn Fn(&str) -> Option<InlineImage>,
) -> Result<Fragment, String> {
    import_at_depth(source, resolve_image, 0)
}
fn import_at_depth(
    source: &str,
    resolve_image: &dyn Fn(&str) -> Option<InlineImage>,
    table_depth: usize,
) -> Result<Fragment, String> {
    if source.len() > 2_000_000 {
        return Err("The pasted HTML is larger than 2 MB. Paste a smaller selection.".into());
    }
    let html = Html::parse_fragment(source);
    let mut rules = Vec::new();
    for style in html.select(&Selector::parse("style").expect("static selector")) {
        let text = style.text().collect::<String>();
        for block in text.split('}') {
            if let Some((selectors, declarations)) = block.split_once('{') {
                for selector in selectors.split(',') {
                    if let Ok(parsed) = Selector::parse(selector.trim()) {
                        rules.push(Rule {
                            selector: parsed,
                            style: declarations.into(),
                            weight: selector.matches('#').count() * 100
                                + selector.matches('.').count() * 10
                                + selector
                                    .chars()
                                    .filter(|c| c.is_alphabetic())
                                    .count()
                                    .min(9),
                        });
                    }
                }
            }
        }
    }
    rules.sort_by_key(|r| r.weight);
    let mut importer = Importer {
        resolve_image,
        fragment: Fragment::default(),
        rules,
        count: 0,
        table_depth,
        stylesheet: html
            .select(&Selector::parse("style").expect("static selector"))
            .map(|s| s.text().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n"),
    };
    importer.walk(html.root_element(), &[], false, 0, 0);
    while importer.fragment.text.ends_with(['\n', ' ']) {
        importer.fragment.text.pop();
    }
    let count = importer.fragment.text.chars().count();
    for span in &mut importer.fragment.rich.spans {
        span.end = span.end.min(count);
    }
    importer.fragment.rich.spans.retain(|s| s.start < s.end);
    for p in &mut importer.fragment.rich.paragraphs {
        p.end = p.end.min(count);
    }
    for table in &mut importer.fragment.rich.tables {
        table.end = table.end.min(count);
    }
    Ok(importer.fragment)
}
impl Importer<'_> {
    fn line(&mut self) {
        while self.fragment.text.ends_with(' ') {
            self.fragment.text.pop();
            self.count = self.count.saturating_sub(1);
        }
        if !self.fragment.text.is_empty() && !self.fragment.text.ends_with('\n') {
            self.fragment.text.push('\n');
            self.count += 1;
        }
    }
    fn text(&mut self, text: &str, marks: &[Mark], pre: bool) {
        let start = self.count;
        for c in text.chars() {
            if !pre && c.is_whitespace() {
                if !self.fragment.text.is_empty() && !self.fragment.text.ends_with([' ', '\n']) {
                    self.fragment.text.push(' ');
                    self.count += 1;
                }
            } else {
                self.fragment.text.push(c);
                self.count += 1;
            }
        }
        if self.count > start {
            for mark in marks {
                self.fragment.rich.spans.push(Span {
                    start,
                    end: self.count,
                    mark: mark.clone(),
                });
            }
        }
    }
    fn walk(
        &mut self,
        element: ElementRef<'_>,
        parent: &[Mark],
        pre: bool,
        depth: usize,
        list: usize,
    ) {
        if depth > 64 || self.count > 500_000 {
            return;
        }
        let tag = element.value().name();
        if [
            "script", "style", "head", "iframe", "object", "embed", "svg", "math", "form", "input",
            "button", "textarea", "noscript", "template",
        ]
        .contains(&tag)
        {
            return;
        }
        let block = [
            "p",
            "div",
            "section",
            "article",
            "header",
            "footer",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "blockquote",
            "pre",
            "ul",
            "ol",
            "li",
            "table",
            "tr",
        ]
        .contains(&tag);
        if block {
            self.line();
        }
        let mut marks = parent.to_vec();
        let mut declarations = String::new();
        for rule in &self.rules {
            if rule.selector.matches(&element) {
                declarations.push_str(&rule.style);
                declarations.push(';');
            }
        }
        if let Some(style) = element.attr("style") {
            declarations.push_str(style);
        }
        let hidden = declarations.to_lowercase();
        if hidden.contains("display:none")
            || hidden.contains("display: none")
            || hidden.contains("visibility:hidden")
        {
            return;
        }
        match tag {
            "b" | "strong" => set(&mut marks, Mark::Bold),
            "i" | "em" => set(&mut marks, Mark::Italic),
            "u" => set(&mut marks, Mark::Underline),
            "s" | "strike" | "del" => set(&mut marks, Mark::Strike),
            "sup" => set(&mut marks, Mark::Superscript),
            "sub" => set(&mut marks, Mark::Subscript),
            "code" | "pre" => set(&mut marks, Mark::Family(FontFamily::Mono)),
            "mark" => set(&mut marks, Mark::Highlight([255, 240, 100])),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                set(&mut marks, Mark::Bold);
                set(
                    &mut marks,
                    Mark::Size(match tag {
                        "h1" => 28,
                        "h2" => 24,
                        "h3" => 20,
                        _ => 17,
                    }),
                );
            }
            "a" => {
                if let Some(url) = element.attr("href").filter(|url| safe_link(url)) {
                    set(&mut marks, Mark::Link(url.into()));
                }
            }
            "font" => {
                if let Some(color) = element.attr("color").and_then(parse_color) {
                    set(&mut marks, Mark::Color(color));
                }
                if let Some(face) = element.attr("face") {
                    set(&mut marks, Mark::Family(parse_family(face)));
                }
                if let Some(size) = element.attr("size").and_then(|v| v.parse::<usize>().ok()) {
                    set(
                        &mut marks,
                        Mark::Size([10, 10, 13, 16, 18, 24, 32, 48][size.min(7)]),
                    );
                }
            }
            "br" => {
                self.fragment.text.push('\n');
                self.count += 1;
                return;
            }
            "hr" => {
                let start = self.count;
                self.text("─", &[], true);
                self.fragment.rich.rules.push(start..self.count);
                self.line();
                return;
            }
            "img" => {
                self.image(element);
                return;
            }
            _ => {}
        }
        for entry in declarations.split(';') {
            let Some((property, value)) = entry.split_once(':') else {
                continue;
            };
            let property = property.trim().to_lowercase();
            let value = value.trim().trim_end_matches("!important").trim();
            let lower = value.to_lowercase();
            match property.as_str() {
                "font-weight" => {
                    if lower == "bold"
                        || lower == "bolder"
                        || lower.parse::<u16>().is_ok_and(|n| n >= 600)
                    {
                        set(&mut marks, Mark::Bold);
                    } else if lower == "normal" || lower.parse::<u16>().is_ok_and(|n| n < 600) {
                        remove(&mut marks, &Mark::Bold);
                    }
                }
                "font-style" => {
                    if lower == "italic" || lower == "oblique" {
                        set(&mut marks, Mark::Italic);
                    } else if lower == "normal" {
                        remove(&mut marks, &Mark::Italic);
                    }
                }
                "font-family" => set(&mut marks, Mark::Family(parse_family(value))),
                "font-size" => {
                    if let Some(size) = parse_size(value) {
                        set(&mut marks, Mark::Size(size));
                    }
                }
                "color" => {
                    if let Some(color) = parse_color(value) {
                        set(&mut marks, Mark::Color(color));
                    }
                }
                "background-color" | "background" => {
                    if let Some(color) = parse_color(value) {
                        set(&mut marks, Mark::Highlight(color));
                    }
                }
                "text-decoration" | "text-decoration-line" => {
                    if lower.contains("none") {
                        remove(&mut marks, &Mark::Underline);
                        remove(&mut marks, &Mark::Strike);
                    }
                    if lower.contains("underline") {
                        set(&mut marks, Mark::Underline);
                    }
                    if lower.contains("line-through") {
                        set(&mut marks, Mark::Strike);
                    }
                }
                "vertical-align" => match lower.as_str() {
                    "super" => set(&mut marks, Mark::Superscript),
                    "sub" => set(&mut marks, Mark::Subscript),
                    _ => {}
                },
                "text-align" if !["p", "h1", "h2", "h3", "h4", "h5", "h6"].contains(&tag) => {
                    self.fragment.rich.alignment = match lower.as_str() {
                        "center" => Alignment::Center,
                        "right" => Alignment::Right,
                        _ => Alignment::Left,
                    }
                }
                _ => {}
            }
        }
        if tag == "table" && self.table_depth < 6 {
            self.table(element, &marks);
            return;
        }
        if tag == "li" {
            let level = element
                .ancestors()
                .filter_map(ElementRef::wrap)
                .filter(|p| ["ul", "ol"].contains(&p.value().name()))
                .count()
                .clamp(1, 8);
            self.text(&"\u{00a0}".repeat(level * 8), &marks, true);
            self.text(
                &if list > 0 {
                    format!("{list}. ")
                } else {
                    "• ".into()
                },
                &marks,
                false,
            );
        }
        let paragraph_start = self.count;
        let mut index = 0;
        for child in element.children() {
            match child.value() {
                Node::Text(text) => self.text(text, &marks, pre || tag == "pre"),
                Node::Element(_) => {
                    if let Some(child) = ElementRef::wrap(child) {
                        if tag == "ol" && child.value().name() == "li" {
                            index += 1;
                        }
                        self.walk(
                            child,
                            &marks,
                            pre || tag == "pre",
                            depth + 1,
                            if tag == "ol" { index } else { 0 },
                        );
                        if ["td", "th"].contains(&child.value().name()) {
                            self.text("\t", &[], true);
                        }
                    }
                }
                _ => {}
            }
        }
        if ["p", "h1", "h2", "h3", "h4", "h5", "h6"].contains(&tag) && self.count > paragraph_start
        {
            let px = |key| css_property(&declarations, key).and_then(css_pixels);
            let default_before = if tag.starts_with('h') { 18.0 } else { 0.0 };
            let default_after = if tag == "p" { 8.0 } else { 6.0 };
            let alignment = match css_property(&declarations, "text-align") {
                Some("center") => Alignment::Center,
                Some("right") => Alignment::Right,
                _ => self.fragment.rich.alignment.clone(),
            };
            self.fragment
                .rich
                .paragraphs
                .push(crate::rich_text::RichParagraph {
                    start: paragraph_start,
                    end: self.count,
                    indent: (px("margin-left").unwrap_or(0.0)
                        + px("padding-left").unwrap_or_else(|| {
                            css_property(&declarations, "padding")
                                .and_then(|p| p.split_whitespace().last().and_then(css_pixels))
                                .unwrap_or(0.0)
                        }))
                    .clamp(0.0, 400.0),
                    before: px("margin-top").unwrap_or(default_before).clamp(0.0, 100.0),
                    after: px("margin-bottom")
                        .unwrap_or(default_after)
                        .clamp(0.0, 100.0),
                    alignment,
                });
        }
        if block {
            self.line();
        }
    }
    fn declarations(&self, element: ElementRef<'_>) -> String {
        let mut declarations = String::new();
        for rule in &self.rules {
            if rule.selector.matches(&element) {
                declarations.push_str(&rule.style);
                declarations.push(';');
            }
        }
        declarations.push_str(element.attr("style").unwrap_or(""));
        declarations
    }
    fn table(&mut self, element: ElementRef<'_>, inherited: &[Mark]) {
        use crate::rich_text::{RichCell, RichTable};
        let start = self.count;
        let mut rows = Vec::new();
        for row in element
            .select(&Selector::parse("tr").expect("static selector"))
            .take(128)
        {
            if row
                .ancestors()
                .filter_map(ElementRef::wrap)
                .find(|p| p.value().name() == "table")
                .is_none_or(|p| p.id() != element.id())
            {
                continue;
            }
            let mut cells = Vec::new();
            for cell in row
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|c| ["td", "th"].contains(&c.value().name()))
                .take(32)
            {
                let declarations = self.declarations(cell);
                let mut inherited_declarations: Vec<_> = cell
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .take_while(|p| p.value().name() != "table")
                    .map(|p| self.declarations(p))
                    .collect();
                inherited_declarations.reverse();
                inherited_declarations.push(declarations.clone());
                let cell_style = inherited_declarations.join(";");
                let source = format!(
                    "<style>{}</style><div style=\"{}\">{}</div>",
                    self.stylesheet,
                    cell_style.replace('&', "&amp;").replace('"', "&quot;"),
                    cell.inner_html()
                );
                let Ok(mut fragment) =
                    import_at_depth(&source, self.resolve_image, self.table_depth + 1)
                else {
                    continue;
                };
                let len = fragment.text.chars().count();
                let mut spans: Vec<_> = inherited
                    .iter()
                    .map(|mark| Span {
                        start: 0,
                        end: len,
                        mark: mark.clone(),
                    })
                    .collect();
                spans.extend(fragment.rich.spans);
                fragment.rich.spans = spans;
                if cell.value().name() == "th" {
                    fragment.rich.spans.push(Span {
                        start: 0,
                        end: len,
                        mark: Mark::Bold,
                    });
                }
                let offset = self.count;
                self.fragment.text.push_str(&fragment.text);
                self.count += len;
                for span in &fragment.rich.spans {
                    let mut span = span.clone();
                    span.start += offset;
                    span.end += offset;
                    self.fragment.rich.spans.push(span);
                }
                self.text("\t", &[], true);
                cells.push(RichCell {
                    padding: element_padding(&declarations),
                    text: fragment.text,
                    rich: Box::new(fragment.rich),
                    colspan: cell
                        .attr("colspan")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1)
                        .clamp(1, 32),
                    rowspan: cell
                        .attr("rowspan")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1)
                        .clamp(1, 128),
                    background: element_background(cell, &declarations)
                        .or_else(|| element_background(row, &self.declarations(row))),
                    border: element_border(cell, &declarations),
                    width: element_width(cell, &declarations),
                });
            }
            if !cells.is_empty() {
                rows.push(cells);
                self.line();
            }
        }
        if rows.is_empty() {
            return;
        }
        if self.count == start {
            self.fragment.text.push('\n');
            self.count += 1;
        }
        let declarations = self.declarations(element);
        let mut columns = Vec::new();
        for col in element
            .select(&Selector::parse("col").expect("static selector"))
            .filter(|col| {
                col.ancestors()
                    .filter_map(ElementRef::wrap)
                    .find(|p| p.value().name() == "table")
                    .is_some_and(|p| p.id() == element.id())
            })
            .take(32)
        {
            if let Some(width) = element_width(col, &self.declarations(col)) {
                let span = col
                    .attr("span")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, 32);
                for _ in 0..span {
                    if columns.len() < 32 {
                        columns.push(width);
                    }
                }
            }
        }
        self.fragment.rich.tables.push(RichTable {
            columns,
            start,
            end: self.count,
            rows,
            border: element_border(element, &declarations),
            background: element_background(element, &declarations),
            width: element_width(element, &declarations),
        });
        self.line();
    }
    fn image(&mut self, element: ElementRef<'_>) {
        let src = element.attr("src").unwrap_or("");
        if let Some(mut image) = (self.resolve_image)(src) {
            image.position = self.count;
            image.alt = element.attr("alt").unwrap_or(&image.alt).to_string();
            let styled = element
                .attr("style")
                .unwrap_or("")
                .split(';')
                .filter_map(|s| s.split_once(':'))
                .find(|(k, _)| k.trim().eq_ignore_ascii_case("width"))
                .and_then(|(_, v)| v.trim().trim_end_matches("px").parse::<u32>().ok());
            image.width = element
                .attr("width")
                .and_then(|v| v.parse::<u32>().ok())
                .or(styled)
                .unwrap_or(image.width)
                .clamp(1, 1200);
            self.fragment.rich.images.push(image);
            return;
        }
        if let Some((header, encoded)) = src.strip_prefix("data:").and_then(|v| v.split_once(','))
            && header.ends_with(";base64")
            && [
                "image/png;base64",
                "image/jpeg;base64",
                "image/gif;base64",
                "image/webp;base64",
            ]
            .contains(&header)
            && encoded.len() < 20_000_000
            && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded)
        {
            let format = image::guess_format(&bytes).ok();
            if let Some(format) = format
                && let Ok((w, h)) =
                    image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
                        .into_dimensions()
                && u64::from(w) * u64::from(h) <= 32_000_000
            {
                self.fragment.rich.images.push(InlineImage {
                    cid: format!("{}@barid", uuid::Uuid::new_v4()),
                    name: "Pasted image".into(),
                    mime: header.trim_end_matches(";base64").into(),
                    data: bytes,
                    position: self.count,
                    width: element
                        .attr("width")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(w.min(480)),
                    alt: element.attr("alt").unwrap_or("Pasted image").into(),
                });
                return;
            }
        }
        self.fragment.skipped_images += 1;
        if let Some(alt) = element.attr("alt").filter(|alt| !alt.is_empty()) {
            self.text(&format!("[{alt}]"), &[], false);
        }
    }
}
fn set(marks: &mut Vec<Mark>, mark: Mark) {
    marks.retain(|m| !m.same_kind(&mark));
    marks.push(mark);
}
fn remove(marks: &mut Vec<Mark>, mark: &Mark) {
    marks.retain(|m| !m.same_kind(mark));
}
fn parse_family(value: &str) -> FontFamily {
    let value = value.to_lowercase();
    if value.contains("mono") || value.contains("courier") || value.contains("consolas") {
        FontFamily::Mono
    } else if value.contains("georgia")
        || value.contains("times")
        || (value.contains("serif") && !value.contains("sans"))
    {
        FontFamily::Serif
    } else {
        FontFamily::Sans
    }
}
fn parse_size(value: &str) -> Option<u16> {
    let value = value.trim().to_lowercase();
    let number = value
        .trim_end_matches(|c: char| c.is_alphabetic() || c == '%')
        .parse::<f32>()
        .ok()?;
    let size = if value.ends_with("pt") {
        number * 1.3333
    } else if value.ends_with("em") || value.ends_with("rem") {
        number * 14.0
    } else if value.ends_with('%') {
        number * 0.14
    } else {
        number
    };
    Some(size.round().clamp(8.0, 72.0) as u16)
}
fn parse_color(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().to_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        if !hex.is_ascii() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        if hex.len() == 3 {
            return Some([
                u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?,
                u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?,
            ]);
        }
        if hex.len() == 6 || hex.len() == 8 {
            return Some([
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
            ]);
        }
    }
    if value.starts_with("rgb")
        && let Some((_, rest)) = value.split_once('(')
    {
        let values: Vec<_> = rest
            .trim_end_matches(')')
            .split(',')
            .take(3)
            .map(|n| n.trim().parse::<u8>())
            .collect();
        if values.len() == 3 {
            return Some([
                *values[0].as_ref().ok()?,
                *values[1].as_ref().ok()?,
                *values[2].as_ref().ok()?,
            ]);
        }
    }
    Some(match value.as_str() {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "red" => [255, 0, 0],
        "green" => [0, 128, 0],
        "blue" => [0, 0, 255],
        "yellow" => [255, 255, 0],
        "gray" | "grey" => [128, 128, 128],
        "orange" => [255, 165, 0],
        "purple" => [128, 0, 128],
        "navy" => [0, 0, 128],
        _ => return None,
    })
}

fn css_property<'a>(style: &'a str, name: &str) -> Option<&'a str> {
    style
        .split(';')
        .filter_map(|s| s.split_once(':'))
        .filter(|(k, _)| k.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
        .next_back()
}
fn element_background(element: ElementRef<'_>, style: &str) -> Option<[u8; 3]> {
    css_property(style, "background-color")
        .or_else(|| css_property(style, "background"))
        .or_else(|| element.attr("bgcolor"))
        .and_then(parse_color)
}
fn element_border(element: ElementRef<'_>, style: &str) -> bool {
    if let Some(border) = css_property(style, "border") {
        return !border.starts_with('0') && !border.eq_ignore_ascii_case("none");
    }
    if let Some(style_value) = css_property(style, "border-style") {
        if style_value.eq_ignore_ascii_case("none") || style_value.eq_ignore_ascii_case("hidden") {
            return false;
        }
        return !css_property(style, "border-width")
            .is_some_and(|w| w.starts_with('0') && !w.starts_with("0."));
    }
    element
        .attr("border")
        .is_some_and(|v| v.parse::<u32>().unwrap_or(0) > 0)
}
fn css_pixels(value: &str) -> Option<f32> {
    let factor = if value.ends_with("pt") {
        4.0 / 3.0
    } else {
        1.0
    };
    value
        .trim_end_matches("px")
        .trim_end_matches("pt")
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|v| v * factor)
}
fn element_padding(style: &str) -> [u8; 2] {
    let Some(padding) = css_property(style, "padding") else {
        return [6, 6];
    };
    let values: Vec<_> = padding.split_whitespace().filter_map(css_pixels).collect();
    if values.is_empty() {
        return [6, 6];
    }
    let v = values[0].clamp(0.0, 64.0).round() as u8;
    let h = values
        .get(1)
        .copied()
        .unwrap_or(values[0])
        .clamp(0.0, 64.0)
        .round() as u8;
    [h, v]
}
fn element_width(element: ElementRef<'_>, style: &str) -> Option<u32> {
    css_property(style, "width")
        .or_else(|| element.attr("width"))
        .and_then(|v| {
            let value = v.trim();
            let factor = if value.ends_with("pt") {
                4.0 / 3.0
            } else {
                1.0
            };
            let number = value
                .trim_end_matches("px")
                .trim_end_matches("pt")
                .trim()
                .parse::<f32>()
                .ok()?;
            if number.is_finite() && number > 0.0 {
                Some((number * factor).round() as u32)
            } else {
                None
            }
        })
        .map(|w| w.clamp(1, 2400))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_inline_styles_and_semantic_formatting() {
        let fragment=import_html("<p><strong>Hello</strong> <span style='font-family:Georgia;font-size:18pt;color:#123abc;background-color:yellow'>café</span> <a href='https://example.com'>Link</a><sub>2</sub><sup>3</sup></p>").unwrap();
        assert_eq!(fragment.text, "Hello café Link23");
        let has = |mark: &Mark| fragment.rich.spans.iter().any(|s| &s.mark == mark);
        assert!(has(&Mark::Bold));
        assert!(has(&Mark::Family(FontFamily::Serif)));
        assert!(has(&Mark::Size(24)));
        assert!(has(&Mark::Color([0x12, 0x3a, 0xbc])));
        assert!(has(&Mark::Highlight([255, 255, 0])));
        assert!(has(&Mark::Link("https://example.com".into())));
        assert!(has(&Mark::Subscript));
        assert!(has(&Mark::Superscript));
    }
    #[test]
    fn stylesheet_classes_and_nested_normal_overrides_are_respected() {
        let fragment=import_html("<style>.red{color:rgb(200,10,20);font-weight:bold}.normal{font-weight:normal}</style><div class='red'>Bold <span class='normal'>normal</span></div>").unwrap();
        assert_eq!(fragment.text, "Bold normal");
        assert!(
            fragment
                .rich
                .spans
                .iter()
                .any(|s| s.mark == Mark::Color([200, 10, 20]))
        );
        assert!(
            !fragment
                .rich
                .spans
                .iter()
                .any(|s| s.mark == Mark::Bold && s.start <= 7 && s.end > 7)
        );
    }
    #[test]
    fn active_content_is_removed_and_remote_images_are_not_loaded() {
        let fragment=import_html("<script>alert('bad')</script><p onclick='bad()'>Safe <a href='javascript:alert(1)'>text</a></p><iframe>evil</iframe><img src='https://tracker.example/pixel' alt='Remote picture'><div style='display:none'>secret</div>").unwrap();
        assert!(fragment.text.contains("Safe text"));
        assert!(!fragment.text.contains("bad"));
        assert!(!fragment.text.contains("evil"));
        assert!(!fragment.text.contains("secret"));
        assert_eq!(fragment.skipped_images, 1);
        assert!(fragment.rich.images.is_empty());
        assert!(
            !fragment
                .rich
                .spans
                .iter()
                .any(|s| matches!(s.mark, Mark::Link(_)))
        );
        assert!(!fragment.rich.html(&fragment.text).contains("onclick"));
        assert_eq!(parse_color("#éx"), None);
    }
    #[test]
    fn source_column_groups_and_styled_paragraphs_are_preserved() {
        let html = r#"<h1 style="margin-top:24pt;margin-bottom:6pt">Title</h1><p style="margin-left:36pt;padding:0pt 0pt 0pt 24pt;margin-top:2pt;margin-bottom:2pt">• A long paragraph that wraps inside its indentation.</p><table style="border-style:none;border-width:medium"><colgroup><col width="114"><col width="104"><col width="203"><col width="203"></colgroup><tr><td style="border-width:0.6pt;border-style:solid;padding:3pt 5pt;background-color:rgb(31,78,121)">Layer</td><td>Component</td><td>Version</td><td>Purpose</td></tr></table>"#;
        let f = import_html(html).unwrap();
        assert_eq!(f.rich.tables[0].columns, vec![114, 104, 203, 203]);
        let cell = &f.rich.tables[0].rows[0][0];
        assert!(cell.border);
        assert_eq!(cell.padding, [7, 4]);
        assert_eq!(cell.background, Some([31, 78, 121]));
        assert_eq!(f.rich.paragraphs[1].indent, 80.0);
        assert_eq!(f.rich.paragraphs[0].before, 32.0);
        let again = import_html(&f.rich.html(&f.text)).unwrap();
        assert_eq!(again.rich.paragraphs[1].indent, 80.0);
        assert_eq!(again.rich.tables[0].columns, vec![114, 104, 203, 203]);
        assert_eq!(again.rich.tables[0].rows[0][0].padding, [7, 4]);
    }
    #[test]
    fn paragraphs_lists_and_table_text_are_retained() {
        let fragment=import_html("<h2>Heading</h2><p>One</p><ol><li>First</li><li>Second</li></ol><table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table>").unwrap();
        assert!(fragment.text.contains("Heading\nOne"));
        assert!(fragment.text.contains("1. First"));
        assert!(fragment.text.contains("2. Second"));
        assert!(fragment.text.contains("\u{00a0}"));
        assert!(fragment.text.contains("A\tB"));
        assert!(fragment.rich.spans.iter().any(|s| s.mark == Mark::Size(24)));
    }
    #[test]
    fn embedded_data_images_are_decoded_into_cid_assets() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(2, 2))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes.get_ref());
        let fragment=import_html(&format!("<p>Before<img src='data:image/png;base64,{encoded}' width='180' alt='Example'>After</p>")).unwrap();
        assert_eq!(fragment.text, "BeforeAfter");
        assert_eq!(fragment.rich.images.len(), 1);
        assert_eq!(fragment.rich.images[0].position, 6);
        assert_eq!(fragment.rich.images[0].width, 180);
        assert_eq!(fragment.rich.images[0].data, *bytes.get_ref());
    }
}
