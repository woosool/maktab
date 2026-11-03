//! Import of Miro boards (`migration/miro-convert.js`): the JSON of Miro's REST API — a board
//! with its `items`, `connectors` and `mindmapNodes` — becomes a Saboura board. Sticky notes,
//! shapes, text, frames, cards and mind-map nodes keep their place, size, colour and text;
//! connectors keep their ends, route, caps and captions. Pictures, which Miro's API only gives
//! as links, become placeholders that say so.

use crate::model::{Board, COLORS, Item, uid};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Miro's named colours (`MIRO_COLORS`).
const NAMED: [(&str, &str); 18] = [
    ("gray", "#e6e6e6"), ("light_yellow", "#fff9b1"), ("yellow", "#f5d128"), ("orange", "#ff9d48"), ("light_green", "#d5f692"),
    ("green", "#c9df56"), ("dark_green", "#93d275"), ("cyan", "#67c6c0"), ("light_pink", "#ffcee0"), ("pink", "#ea94bb"),
    ("violet", "#c6a2d2"), ("red", "#f0939d"), ("light_blue", "#a6ccf5"), ("blue", "#6cd8fa"), ("dark_blue", "#9ea9ff"),
    ("black", "#1a1a1a"), ("white", "#ffffff"), ("transparent", ""),
];

/// A Miro colour (name, #rgb, #rrggbb, #rrggbbaa or rgb()) as #rrggbb.
pub fn color(v: &Value) -> Option<String> {
    let s = v.as_str()?.trim().to_lowercase().replace('-', "_");
    if let Some((_, c)) = NAMED.iter().find(|(n, _)| *n == s) { return (!c.is_empty()).then(|| c.to_string()); }
    let h = s.trim_start_matches('#');
    if h.len() == 3 && h.chars().all(|c| c.is_ascii_hexdigit()) { return Some(format!("#{}", h.chars().flat_map(|c| [c, c]).collect::<String>())); }
    if (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit()) { return (h.len() == 6 || &h[6..] != "00").then(|| format!("#{}", &h[..6])); }
    let nums: Vec<f64> = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')'))?.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if nums.len() >= 3 && nums.get(3).is_none_or(|a| *a > 0.0) { return Some(format!("#{:02x}{:02x}{:02x}", nums[0].clamp(0.0, 255.0) as u8, nums[1].clamp(0.0, 255.0) as u8, nums[2].clamp(0.0, 255.0) as u8)); }
    None
}

/// Miro shape names as (object type, silhouette) (`SHAPE_MAP`).
pub fn shape(name: &str) -> (&'static str, Option<&'static str>) {
    match name.to_lowercase().as_str() {
        "circle" | "ellipse" | "oval" | "flow_chart_connector" => ("ellipse", None),
        "triangle" => ("rect", Some("triangle")),
        "rhombus" | "diamond" | "flow_chart_decision" => ("rect", Some("diamond")),
        "parallelogram" | "flow_chart_input_output" => ("rect", Some("parallelogram")),
        "hexagon" | "flow_chart_preparation" => ("rect", Some("hexagon")),
        "star" => ("rect", Some("star")),
        "can" | "cylinder" | "flow_chart_magnetic_disk" => ("rect", Some("cylinder")),
        "flow_chart_terminator" | "pill" => ("rect", Some("terminator")),
        "flow_chart_document" | "flow_chart_multidocuments" | "flow_chart_multidocument" => ("rect", Some("document")),
        "flow_chart_predefined_process" | "flow_chart_predefined_process_2" => ("rect", Some("predefined-process")),
        "flow_chart_manual_input" => ("rect", Some("manual-input")),
        "flow_chart_manual_operation" => ("rect", Some("manual-operation")),
        "flow_chart_delay" => ("rect", Some("delay")),
        "flow_chart_display" => ("rect", Some("display")),
        "flow_chart_off_page_connector" | "flow_chart_offpage_connector" => ("rect", Some("off-page")),
        "flow_chart_or" => ("rect", Some("or")),
        "flow_chart_summing_junction" => ("rect", Some("summing-junction")),
        "flow_chart_internal_storage" => ("rect", Some("internal-storage")),
        "flow_chart_merge" => ("rect", Some("merge")),
        "cloud" => ("rect", Some("cloud")),
        _ => ("rect", None),
    }
}

/// Plain text from Miro's HTML: paragraphs and breaks become lines, tags go, entities decode.
pub fn html_text(html: &str) -> String {
    let mut s = html.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n").replace("</p>", "\n").replace("</li>", "\n").replace("</div>", "\n");
    let mut out = String::new();
    let mut tag = false;
    for c in s.drain(..) {
        match c { '<' => tag = true, '>' if tag => tag = false, _ if !tag => out.push(c), _ => {} }
    }
    let out = out.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'").replace("&amp;", "&");
    out.trim_end_matches('\n').to_string()
}

fn num(v: &Value) -> Option<f64> { v.as_f64().filter(|f| f.is_finite()) }

fn default_size(kind: &str) -> (f64, f64) {
    match kind {
        "sticky_note" => (199.0, 228.0), "text" => (280.0, 60.0), "frame" => (800.0, 600.0), "image" => (300.0, 200.0),
        "card" | "app_card" => (320.0, 94.0), "document" | "embed" | "preview" => (280.0, 115.0), "mindmap_node" => (200.0, 76.0), _ => (200.0, 120.0),
    }
}

/// What the import did: counts by Miro type and what was left out.
#[derive(Debug, Default)]
pub struct Report { pub converted: usize, pub skipped: Vec<String> }

/// Converts Miro's board JSON (`{ board, items, connectors, mindmapNodes }`, or a bare item
/// list) into a board.
pub fn convert(content: &Value) -> Result<(Board, Report), String> {
    let items: Vec<&Value> = content["items"].as_array().or(content["data"].as_array()).or(content.as_array()).map(|a| a.iter().collect()).unwrap_or_default();
    let nodes: Vec<&Value> = content["mindmapNodes"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    let all: Vec<&Value> = items.iter().chain(nodes.iter()).copied().filter(|i| !i["id"].is_null()).collect();
    if all.is_empty() && content["connectors"].as_array().is_none_or(|c| c.is_empty()) { return Err("This file has no Miro board items.".into()); }
    let key = |v: &Value| match v { Value::String(s) => s.clone(), other => other.to_string() };
    let by_id: HashMap<String, &Value> = all.iter().map(|i| (key(&i["id"]), *i)).collect();
    let size = |i: &Value| {
        let (dw, dh) = default_size(i["type"].as_str().unwrap_or(""));
        match (num(&i["geometry"]["width"]), num(&i["geometry"]["height"])) {
            (Some(w), Some(h)) => (w, h), (Some(w), None) => (w, w * dh / dw), (None, Some(h)) => (h * dw / dh, h), _ => (dw, dh),
        }
    };
    // Centres on the board: positions relative to a parent are added to its top-left corner.
    fn center(i: &Value, by_id: &HashMap<String, &Value>, size: &dyn Fn(&Value) -> (f64, f64), depth: usize) -> (f64, f64) {
        let p = &i["position"];
        let (mut x, mut y) = (num(&p["x"]).unwrap_or(0.0), num(&p["y"]).unwrap_or(0.0));
        if p["origin"].as_str().is_some_and(|o| o != "center") { let s = size(i); x += s.0 / 2.0; y += s.1 / 2.0; }
        if p["relativeTo"] == "parent_top_left" && depth < 32 {
            let pid = match &i["parent"]["id"] { Value::String(s) => s.clone(), Value::Null => String::new(), o => o.to_string() };
            if let Some(parent) = by_id.get(&pid) {
                let c = center(parent, by_id, size, depth + 1);
                let s = size(parent);
                x += c.0 - s.0 / 2.0;
                y += c.1 - s.1 / 2.0;
            }
        }
        (x, y)
    }
    let mut report = Report::default();
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut out: Vec<Item> = Vec::new();
    for i in &all {
        let kind = i["type"].as_str().unwrap_or("");
        let (w, h) = size(i);
        let (cx, cy) = center(i, &by_id, &size, 0);
        let data = &i["data"];
        let style = &i["style"];
        let text = html_text(data["content"].as_str().or(data["title"].as_str()).or(data["nodeView"]["data"]["content"].as_str()).unwrap_or(""));
        let fill = color(&style["fillColor"]);
        let mut item = match kind {
            "sticky_note" => Item::new("note", 0.0, 0.0).with(|n| { n.color = Some(fill.clone().unwrap_or_else(|| "#fff9b1".into())); }),
            "text" => Item::new("text", 0.0, 0.0),
            "frame" => Item::new("frame", 0.0, 0.0).with(|f| { f.color = Some(fill.clone().unwrap_or_else(|| "#ffffff".into())); f.auto_font_size = None; }),
            "shape" => {
                let (t, s) = shape(data["shape"].as_str().unwrap_or("rectangle"));
                Item::new(t, 0.0, 0.0).with(|r| {
                    r.shape = s.map(str::to_string);
                    r.color = Some(fill.clone().unwrap_or_else(|| "#ffffff".into()));
                    r.text_align = Some("center".into());
                    if let Some(b) = color(&style["borderColor"]) { r.set("borderColor", b); }
                    if let Some(bw) = num(&style["borderWidth"]).or_else(|| style["borderWidth"].as_str().and_then(|s| s.parse().ok())) { r.line_width = Some(bw.clamp(1.0, 12.0)); }
                    if let Some(st) = style["borderStyle"].as_str().filter(|s| matches!(*s, "dashed" | "dotted")) { r.set("lineStyle", st); }
                })
            }
            "card" | "app_card" => Item::new("note", 0.0, 0.0).with(|n| {
                n.color = Some(COLORS[2].into());
                let d = html_text(data["description"].as_str().unwrap_or(""));
                if !d.is_empty() { n.extra.insert("cardDescription".into(), Value::from(d)); }
            }),
            "mindmap_node" => Item::new("rect", 0.0, 0.0).with(|r| { r.color = Some(fill.clone().unwrap_or_else(|| COLORS[2].into())); r.text_align = Some("center".into()); }),
            "image" | "document" | "embed" | "preview" => Item::new("rect", 0.0, 0.0).with(|r| {
                r.color = Some("#eeeeee".into());
                r.text_align = Some("center".into());
                r.set("lineStyle", "dashed");
                let url = data["imageUrl"].as_str().or(data["documentUrl"].as_str()).or(data["url"].as_str()).unwrap_or("");
                if !url.is_empty() && (url.starts_with("http://") || url.starts_with("https://")) { r.set("link", url); }
            }),
            other => { report.skipped.push(other.to_string()); continue; }
        };
        item.w = w; item.h = h;
        item.x = cx - w / 2.0; item.y = cy - h / 2.0;
        item.text = match kind {
            "image" | "document" | "embed" | "preview" if text.is_empty() => format!("{} ({kind})", data["title"].as_str().unwrap_or("Miro")),
            "card" | "app_card" => match item.str("cardDescription") { Some(d) => format!("{text}\n{d}"), None => text },
            _ => text,
        }.chars().take(20000).collect();
        item.extra.remove("cardDescription");
        if let Some(fs) = num(&style["fontSize"]).or_else(|| style["fontSize"].as_str().and_then(|s| s.parse().ok())) { item.font_size = Some(fs.clamp(6.0, 400.0)); item.auto_font_size = None; }
        if let Some(a) = style["textAlign"].as_str().filter(|a| matches!(*a, "left" | "center" | "right")) { item.text_align = Some(a.into()); }
        if let Some(c) = color(&style["color"]) { item.set("textColor", c); }
        if let Some(r) = num(&i["geometry"]["rotation"]).filter(|r| *r != 0.0) && crate::geometry::can_rotate(&item) { item.rotation = Some(crate::geometry::normalize_rotation(r)); }
        ids.insert(key(&i["id"]), item.id.clone());
        out.push(item);
        report.converted += 1;
    }
    // Mind-map branches keep their tree.
    for n in &nodes {
        let (Some(id), Some(pid)) = (ids.get(&key(&n["id"])), n["parent"]["id"].as_str().map(str::to_string).or_else(|| (!n["parent"]["id"].is_null()).then(|| n["parent"]["id"].to_string()))) else { continue };
        let parent = ids.get(&pid).cloned();
        if let Some(item) = out.iter_mut().find(|i| i.id == *id) {
            item.set("mindMap", json!({ "parent": parent, "direction": "right", "auto": false, "line": "curved" }));
        }
    }
    for c in content["connectors"].as_array().into_iter().flatten() {
        let style = &c["style"];
        let mut line = Item::new("arrow", 0.0, 0.0);
        line.start_id = ids.get(&key(&c["startItem"]["id"])).cloned();
        line.end_id = ids.get(&key(&c["endItem"]["id"])).cloned();
        if line.start_id.is_none() && line.end_id.is_none() { report.skipped.push("connector".into()); continue; }
        line.line_type = Some(match c["shape"].as_str() { Some("elbowed") => "elbow", Some("curved") => "curved", _ => "straight" }.into());
        if let Some(col) = color(&style["strokeColor"]) { line.color = Some(col); }
        if let Some(wd) = num(&style["strokeWidth"]).or_else(|| style["strokeWidth"].as_str().and_then(|s| s.parse().ok())) { line.line_width = Some(wd.clamp(1.0, 12.0)); }
        if let Some(st) = style["strokeStyle"].as_str().filter(|s| matches!(*s, "dashed" | "dotted")) { line.set("lineStyle", st); }
        let cap = |v: &Value| match v.as_str().unwrap_or("") {
            "none" => "none", "oval" => "circle-open", "filled_oval" => "circle", "triangle" => "triangle-open", "filled_triangle" => "triangle",
            "diamond" => "diamond-open", "filled_diamond" => "diamond", "erd_one" | "erd_only_one" => "bar", "erd_many" | "erd_one_or_many" | "erd_zero_or_many" => "crowfoot", _ => "arrow",
        }.to_string();
        let (sc, ec) = (cap(&style["startStrokeCap"]), if style["endStrokeCap"].is_null() { "arrow".to_string() } else { cap(&style["endStrokeCap"]) });
        line.start_cap = (sc != "none").then_some(sc);
        line.end_cap = (ec != "arrow").then_some(ec);
        line.text = c["captions"].as_array().map(|cs| cs.iter().filter_map(|x| x["content"].as_str()).map(html_text).collect::<Vec<_>>().join(" ")).unwrap_or_default();
        out.push(line);
        report.converted += 1;
    }
    if out.len() > crate::model::MAX_OBJECTS { return Err(format!("This Miro board has more than {} objects.", crate::model::MAX_OBJECTS)); }
    let title = content["board"]["name"].as_str().unwrap_or("Miro board").chars().take(200).collect::<String>();
    let mut board = Board::new(&title, "blank");
    board.id = uid();
    board.items = out;
    board.update_connectors();
    board.validate()?;
    Ok((board, report))
}
