//! The board and its objects, read and written in the JSON format of the JS whiteboard
//! (`model.js`). Fields this port does not use yet are kept in `extra`, so a board opened
//! and saved here loses nothing.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Fills for new notes and shapes (`COLORS` in model.js).
pub const COLORS: [&str; 6] = ["#f6e7a4", "#d9e9d2", "#dce5f7", "#eadcf0", "#f3d6c8", "#ffffff"];
/// Inks for new drawings, each at least 3:1 against the light and dark board.
pub const PEN_COLORS: [&str; 6] = ["#687169", "#d03a3a", "#2f6fd6", "#26894a", "#c86b0a", "#8a5bd6"];
/// The farthest a coordinate may lie from the board's centre.
pub const BOARD_EXTENT: f64 = 1e6;
/// The most objects a board holds.
pub const MAX_OBJECTS: usize = 5000;
/// The most points one drawing may have.
pub const PEN_POINTS: usize = 20000;
pub const MIN_ZOOM: f64 = 0.01;
pub const MAX_ZOOM: f64 = 3.0;
/// Below this zoom the board leaves out text too small to read.
pub const DETAIL_ZOOM: f64 = 0.15;

pub const TYPES: [&str; 11] = ["note", "text", "rect", "ellipse", "arrow", "pen", "frame", "image", "link", "table", "file"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_font_size: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    /// A drawing's points, relative to its x, y.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<[f64; 2]>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width: Option<f64>,
    /// Connection lines: straight, elbow or curved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_cap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_cap: Option<String>,
    /// A rectangle's silhouette (diamond, hexagon...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub version: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub updated: f64,
    #[serde(default)]
    pub favorite: bool,
    pub items: Vec<Item>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The starter templates: key and name.
pub const TEMPLATES: [(&str, &str); 7] = [("blank", "Blank"), ("brainstorm", "Brainstorm"), ("kanban", "Kanban"), ("retrospective", "Retrospective"), ("swot", "SWOT analysis"), ("flowchart", "Flowchart"), ("weekly", "Weekly plan")];

/// Frames side by side with a note in each.
fn columns(titles: &[&str], notes: &[&str]) -> Vec<Item> {
    let mut v = Vec::new();
    for (n, t) in titles.iter().enumerate() {
        let x = 60.0 + n as f64 * 300.0;
        let c = n % 5;
        v.push(Item::new("frame", x, 60.0).with(|i| { i.w = 280.0; i.h = 560.0; i.text = t.to_string(); i.color = Some(COLORS[c].into()); i.auto_font_size = None; }));
        if !notes[n].is_empty() { v.push(Item::new("note", x + 30.0, 120.0).with(|i| { i.text = notes[n].into(); i.w = 220.0; i.h = 180.0; i.color = Some(COLORS[c].into()); })); }
    }
    v
}

/// A random version 4 UUID, as `crypto.randomUUID` gives.
pub fn uid() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(8) {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

impl Item {
    /// A new object with the JS defaults (`item` in model.js).
    pub fn new(kind: &str, x: f64, y: f64) -> Item {
        let text = kind == "text";
        Item {
            id: uid(),
            kind: kind.to_string(),
            x, y,
            w: if text { 280.0 } else { 190.0 },
            h: if text { 60.0 } else { 160.0 },
            text: String::new(),
            color: Some(if matches!(kind, "arrow" | "pen") { PEN_COLORS[0] } else { COLORS[0] }.to_string()),
            font_size: None,
            text_align: None,
            auto_font_size: (kind == "note").then_some(true),
            bold: None,
            rotation: None,
            points: None,
            line_width: None,
            line_type: None,
            start_id: None,
            end_id: None,
            start_cap: None,
            end_cap: None,
            shape: None,
            group_id: None,
            locked: None,
            extra: Map::new(),
        }
    }

    pub fn with(mut self, f: impl FnOnce(&mut Item)) -> Item { f(&mut self); self }

    pub fn is_connector(&self) -> bool { self.kind == "arrow" }
    pub fn is_locked(&self) -> bool { self.locked == Some(true) }
    pub fn rotation(&self) -> f64 { if crate::geometry::can_rotate(self) { self.rotation.unwrap_or(0.0) } else { 0.0 } }

    /// Text size in board units (`textStyle` in text.js).
    pub fn font_size(&self) -> f64 {
        self.font_size.unwrap_or(match self.kind.as_str() { "text" => 28.0, "arrow" => 16.0, _ => 19.0 })
    }
    pub fn is_bold(&self) -> bool { self.bold.unwrap_or(self.kind == "frame") }
    pub fn align(&self) -> &str {
        self.text_align.as_deref().unwrap_or(match self.kind.as_str() { "rect" | "ellipse" => "center", _ => "left" })
    }
    /// A field this port keeps in `extra` (bold, borderColor, cells...).
    pub fn get(&self, key: &str) -> Option<&Value> { self.extra.get(key) }
    pub fn str(&self, key: &str) -> Option<&str> { self.extra.get(key).and_then(|v| v.as_str()) }
    pub fn num(&self, key: &str) -> Option<f64> { self.extra.get(key).and_then(|v| v.as_f64()) }
    pub fn flag(&self, key: &str) -> bool { self.extra.get(key).and_then(|v| v.as_bool()).unwrap_or(false) }
    /// Sets a field kept in `extra`; `Value::Null` removes it.
    pub fn set(&mut self, key: &str, v: impl Into<Value>) {
        let v = v.into();
        if v.is_null() { self.extra.remove(key); } else { self.extra.insert(key.to_string(), v); }
    }
    pub fn set_flag(&mut self, key: &str, on: bool) { self.set(key, if on { Value::Bool(true) } else { Value::Null }); }
    /// A table's cells (`cells`), rows of strings.
    pub fn cells(&self) -> Option<Vec<Vec<String>>> { self.extra.get("cells").and_then(|v| serde_json::from_value(v.clone()).ok()) }
    pub fn set_cells(&mut self, cells: &[Vec<String>]) { self.set("cells", serde_json::to_value(cells).unwrap_or(Value::Null)); }
    /// The mind-map parent of a branch, when it has one.
    pub fn mind_parent(&self) -> Option<&str> { self.extra.get("mindMap").and_then(|m| m.get("parent")).and_then(|p| p.as_str()) }
    pub fn is_mind(&self) -> bool { self.extra.contains_key("mindMap") }

    pub fn color_rgb(&self) -> Option<[u8; 3]> { self.color.as_deref().and_then(parse_hex) }
}

/// `#rrggbb` as bytes.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 { return None; }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl Board {
    /// A new board, empty or from one of the starter templates (`board` in model.js).
    pub fn new(title: &str, template: &str) -> Board {
        let items = match template {
            "brainstorm" => {
                let mut v = vec![
                    Item::new("text", 80.0, 55.0).with(|i| { i.text = "Room for a great idea".into(); i.w = 680.0; i.h = 65.0; i.font_size = Some(36.0); }),
                    Item::new("text", 80.0, 130.0).with(|i| { i.text = "Start with a question. Follow your curiosity.".into(); i.w = 650.0; i.font_size = Some(18.0); }),
                ];
                for (n, t) in ["What if we…", "Something worth exploring", "A different perspective"].iter().enumerate() {
                    v.push(Item::new("note", 80.0 + n as f64 * 245.0, 240.0).with(|i| { i.text = t.to_string(); i.color = Some(COLORS[n].into()); i.w = 210.0; i.h = 190.0; }));
                }
                v
            }
            "kanban" => {
                let mut v = Vec::new();
                let notes = ["Your next step", "Make a little progress", "Celebrate the small wins"];
                for (n, t) in ["To do", "In progress", "Done"].iter().enumerate() {
                    let x = n as f64 * 290.0;
                    v.push(Item::new("frame", 70.0 + x, 60.0).with(|i| { i.w = 270.0; i.h = 500.0; i.text = t.to_string(); i.color = Some(COLORS[n].into()); }));
                    v.push(Item::new("note", 90.0 + x, 140.0).with(|i| { i.text = notes[n].into(); i.w = 230.0; i.h = 160.0; i.color = Some(COLORS[n].into()); }));
                }
                v
            }
            "retrospective" => columns(&["Went well", "To improve", "Action items"], &["What made us proud?", "What slowed us down?", "One thing to try next"]),
            "weekly" => columns(&["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"], &["Plan the week", "", "", "", "Review"]),
            "swot" => {
                let mut v = Vec::new();
                for (n, (t, c)) in [("Strengths", 1), ("Weaknesses", 4), ("Opportunities", 2), ("Threats", 3)].iter().enumerate() {
                    let (x, y) = (60.0 + (n % 2) as f64 * 460.0, 60.0 + (n / 2) as f64 * 360.0);
                    v.push(Item::new("frame", x, y).with(|i| { i.w = 440.0; i.h = 340.0; i.text = t.to_string(); i.color = Some(COLORS[*c].into()); i.auto_font_size = None; }));
                    v.push(Item::new("note", x + 30.0, y + 60.0).with(|i| { i.w = 160.0; i.h = 140.0; i.color = Some(COLORS[*c].into()); }));
                }
                v
            }
            "flowchart" => {
                let shape = |x: f64, y: f64, w: f64, h: f64, text: &str, shape: Option<&str>, color: usize| Item::new("rect", x, y).with(|i| {
                    i.w = w; i.h = h; i.text = text.into(); i.shape = shape.map(str::to_string); i.color = Some(COLORS[color].into());
                    i.text_align = Some("center".into()); i.auto_font_size = Some(true);
                });
                let items = vec![
                    shape(100.0, 200.0, 160.0, 70.0, "Start", Some("terminator"), 1),
                    shape(340.0, 195.0, 180.0, 80.0, "Do the work", None, 2),
                    shape(600.0, 165.0, 150.0, 140.0, "Done?", Some("diamond"), 3),
                    shape(840.0, 200.0, 160.0, 70.0, "End", Some("terminator"), 4),
                ];
                let mut v = items.clone();
                for k in 0..3 {
                    v.push(Item::new("arrow", 0.0, 0.0).with(|l| { l.start_id = Some(items[k].id.clone()); l.end_id = Some(items[k + 1].id.clone()); l.line_type = Some("elbow".into()); if k == 2 { l.text = "Yes".into(); } }));
                }
                v.push(Item::new("arrow", 0.0, 0.0).with(|l| { l.start_id = Some(items[2].id.clone()); l.end_id = Some(items[1].id.clone()); l.line_type = Some("curved".into()); l.text = "No".into(); }));
                v
            }
            _ => Vec::new(),
        };
        let mut board = Board { version: 1, id: uid(), title: title.to_string(), updated: now_ms(), favorite: false, items, extra: Map::new() };
        board.update_connectors();
        board
    }

    pub fn find(&self, id: &str) -> Option<&Item> { self.items.iter().find(|i| i.id == id) }
    pub fn find_mut(&mut self, id: &str) -> Option<&mut Item> { self.items.iter_mut().find(|i| i.id == id) }

    /// Reads a board file, checking it as `validateBoard` does (the parts this port knows).
    pub fn from_json(text: &str) -> Result<Board, String> {
        let b: Board = serde_json::from_str(text).map_err(|_| "This is not a supported Whiteboard file.".to_string())?;
        b.validate()?;
        Ok(b)
    }

    pub fn to_json(&self) -> String { serde_json::to_string(self).unwrap_or_default() }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.title.chars().count() > 200 || self.items.len() > MAX_OBJECTS {
            return Err("This is not a supported Whiteboard file.".into());
        }
        let mut ids = std::collections::HashSet::new();
        for i in &self.items {
            let finite = [i.x, i.y, i.w, i.h].iter().all(|v| v.is_finite() && v.abs() <= BOARD_EXTENT);
            if !ids.insert(&i.id) || !TYPES.contains(&i.kind.as_str()) && i.kind != "embed" || !finite
                || i.text.chars().count() > 20000 || i.color.as_deref().is_some_and(|c| parse_hex(c).is_none()) {
                return Err("The board contains an invalid object.".into());
            }
            if let Some(r) = i.rotation && (!r.is_finite() || !(-180.0..180.0).contains(&r)) {
                return Err("Invalid object rotation.".into());
            }
            if i.kind == "pen" && i.points.as_ref().is_none_or(|p| p.len() > PEN_POINTS || p.iter().any(|q| !q[0].is_finite() || !q[1].is_finite())) {
                return Err("The board contains an invalid object.".into());
            }
        }
        Ok(())
    }

    /// Moves connection lines attached to objects so their ends follow them (`updateConnectors`
    /// in scene.js). Elbow and curved lines meet the middle of a side, running across or down.
    pub fn update_connectors(&mut self) {
        use crate::geometry::{Box2, anchor, center};
        let boxes: std::collections::HashMap<String, Item> = self.items.iter().filter(|i| !i.is_connector()).map(|i| (i.id.clone(), i.clone())).collect();
        for c in self.items.iter_mut().filter(|i| i.is_connector()) {
            let start = c.start_id.as_ref().and_then(|id| boxes.get(id));
            let end = c.end_id.as_ref().and_then(|id| boxes.get(id));
            if start.is_none() { c.start_id = None; }
            if end.is_none() { c.end_id = None; }
            let sc = start.map(center).unwrap_or((c.x, c.y));
            let ec = end.map(center).unwrap_or((c.x + c.w, c.y + c.h));
            let routed = c.line_type.as_deref().is_some_and(|t| t != "straight");
            let mut axis_x = true;
            if routed {
                let bx = |o: Option<&Item>, p: (f64, f64)| o.map(|i| Box2 { x: i.x, y: i.y, w: i.w, h: i.h }).unwrap_or(Box2 { x: p.0, y: p.1, w: 0.0, h: 0.0 });
                let (s, e) = (bx(start, sc), bx(end, ec));
                let across = s.x.max(e.x) - (s.x + s.w).min(e.x + e.w) > 0.0;
                let down = s.y.max(e.y) - (s.y + s.h).min(e.y + e.h) > 0.0;
                axis_x = if across != down { across } else { (ec.0 - sc.0).abs() >= (ec.1 - sc.1).abs() };
                c.extra.insert("routeAxis".into(), Value::from(if axis_x { "x" } else { "y" }));
            }
            let toward = |from: (f64, f64), to: (f64, f64)| if !routed { to } else if axis_x { (to.0, from.1) } else { (from.0, to.1) };
            let mut a = start.map(|s| anchor(s, toward(sc, ec))).unwrap_or(sc);
            let mut b = end.map(|e| anchor(e, toward(ec, sc))).unwrap_or(ec);
            if let (Some(s), Some(e)) = (start, end) && s.id != e.id && (b.0 - a.0).hypot(b.1 - a.1) < 1.0 {
                let (dx, dy) = (ec.0 - sc.0, ec.1 - sc.1);
                if dx != 0.0 && dy != 0.0 {
                    a = (if dx > 0.0 { s.x + s.w } else { s.x }, sc.1);
                    b = (ec.0, if dy > 0.0 { e.y } else { e.y + e.h });
                }
            }
            c.x = a.0; c.y = a.1; c.w = b.0 - a.0; c.h = b.1 - a.1;
        }
    }

    /// Drops connection lines whose attached objects are gone (as deleting does in the JS).
    pub fn remove(&mut self, ids: &std::collections::HashSet<String>) {
        self.items.retain(|i| !ids.contains(&i.id));
        let present: std::collections::HashSet<String> = self.items.iter().map(|i| i.id.clone()).collect();
        self.items.retain(|i| !i.is_connector() || [&i.start_id, &i.end_id].iter().all(|e| e.as_ref().is_none_or(|id| present.contains(id))));
    }
}

/// Undo and redo of whole item lists (`History` in model.js), at most 80 steps back.
/// Embedded pictures are kept once in a pool shared by every step, not copied into each.
#[derive(Default)]
pub struct History {
    past: Vec<Vec<Item>>,
    future: Vec<Vec<Item>>,
    pool: std::collections::HashMap<u64, String>,
}

const POOL_MARK: &str = "\u{0}pool:";

impl History {
    /// A step's copy of the items, large `src` values replaced by pool references.
    fn pack(&mut self, items: &[Item]) -> Vec<Item> {
        items.iter().map(|i| {
            let mut c = i.clone();
            if let Some(Value::String(src)) = c.extra.get_mut("src") && src.len() > 4096 {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                src.hash(&mut h);
                let key = h.finish();
                let full = std::mem::replace(src, format!("{POOL_MARK}{key}"));
                self.pool.entry(key).or_insert(full);
            }
            c
        }).collect()
    }
    fn unpack(&self, mut items: Vec<Item>) -> Vec<Item> {
        for i in items.iter_mut() {
            if let Some(Value::String(src)) = i.extra.get_mut("src") && let Some(key) = src.strip_prefix(POOL_MARK).and_then(|k| k.parse::<u64>().ok()) && let Some(full) = self.pool.get(&key) { *src = full.clone(); }
        }
        items
    }
    /// Drops pictures no step refers to any more.
    fn prune(&mut self) {
        let used: std::collections::HashSet<u64> = self.past.iter().chain(&self.future).flatten()
            .filter_map(|i| i.str("src").and_then(|s| s.strip_prefix(POOL_MARK)).and_then(|k| k.parse().ok())).collect();
        self.pool.retain(|k, _| used.contains(k));
    }
    pub fn push(&mut self, items: &[Item]) {
        let packed = self.pack(items);
        self.past.push(packed);
        if self.past.len() > 80 { self.past.remove(0); }
        self.future.clear();
        self.prune();
    }
    pub fn undo(&mut self, items: &mut Vec<Item>) -> bool {
        let Some(prev) = self.past.pop() else { return false };
        let now = self.pack(items);
        self.future.push(now);
        *items = self.unpack(prev);
        true
    }
    pub fn redo(&mut self, items: &mut Vec<Item>) -> bool {
        let Some(next) = self.future.pop() else { return false };
        let now = self.pack(items);
        self.past.push(now);
        *items = self.unpack(next);
        true
    }
    pub fn can_undo(&self) -> bool { !self.past.is_empty() }
    pub fn can_redo(&self) -> bool { !self.future.is_empty() }
    pub fn clear(&mut self) { self.past.clear(); self.future.clear(); self.pool.clear(); }
    /// Pictures held for undo (for tests and diagnostics).
    pub fn pooled(&self) -> usize { self.pool.len() }
}
