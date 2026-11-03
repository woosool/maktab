//! Edits that build structure: quick connect (`quick-connect.js`), mind-map branches
//! (`mind-map.js`) and tables (`table-*.js`).

use crate::geometry::{Box2, bounds, object_bounds};
use crate::model::{Board, Item, uid};
use serde_json::json;

pub const DIRECTIONS: [(&str, f64, f64); 4] = [("top", 0.0, -1.0), ("right", 1.0, 0.0), ("bottom", 0.0, 1.0), ("left", -1.0, 0.0)];

/// Whether an object takes quick-connect ports.
pub fn can_quick_connect(i: &Item) -> bool { matches!(i.kind.as_str(), "rect" | "ellipse" | "note" | "text") && !i.is_mind() }

/// Adds a copy of `id` beside it toward `dir` (top, right, bottom, left), joined to it by a
/// line, moving along the side until the place is free. Returns the new object's ID.
pub fn quick_connect(board: &mut Board, id: &str, dir: &str, line_type: &str) -> Option<String> {
    let src = board.find(id)?.clone();
    if !can_quick_connect(&src) || src.is_locked() { return None; }
    let (_, dx, dy) = *DIRECTIONS.iter().find(|d| d.0 == dir)?;
    let gap = 80.0;
    let mut copy = src.clone();
    copy.id = uid();
    copy.text.clear();
    copy.group_id = None;
    copy.extra.remove("textRuns");
    copy.extra.remove("textBlocks");
    copy.x = src.x + dx * (src.w + gap);
    copy.y = src.y + dy * (src.h + gap);
    let others: Vec<Box2> = board.items.iter().filter(|i| !i.is_connector() && i.kind != "frame").map(object_bounds).collect();
    let free = |c: &Item| { let b = object_bounds(c); !others.iter().any(|o| o.intersects(&Box2 { x: b.x + 1.0, y: b.y + 1.0, w: b.w - 2.0, h: b.h - 2.0 })) };
    let base = (copy.x, copy.y);
    'search: for k in 0..40 {
        let step = (k as f64 / 2.0).ceil() * if k % 2 == 1 { 1.0 } else { -1.0 };
        let (sx, sy) = if dx != 0.0 { (0.0, step * (src.h + 30.0)) } else { (step * (src.w + 30.0), 0.0) };
        copy.x = base.0 + sx;
        copy.y = base.1 + sy;
        if free(&copy) { break 'search; }
    }
    let new_id = copy.id.clone();
    let mut line = Item::new("arrow", 0.0, 0.0);
    line.start_id = Some(src.id.clone());
    line.end_id = Some(new_id.clone());
    line.line_type = Some(line_type.to_string());
    board.items.push(copy);
    board.items.push(line);
    board.update_connectors();
    Some(new_id)
}

/// Adds a branch to a mind-map node (making it a root if it isn't in a map yet), or a
/// sibling with `sibling`. Returns the new node's ID.
pub fn add_branch(board: &mut Board, id: &str, sibling: bool) -> Option<String> {
    let node = board.find(id)?.clone();
    let parent_id = if sibling { node.mind_parent().map(str::to_string) } else { Some(node.id.clone()) };
    let parent_id = match parent_id { Some(p) => p, None if sibling => return add_branch(board, id, false), None => return None };
    if !board.find(&parent_id)?.is_mind() {
        let p = board.find_mut(&parent_id)?;
        p.set("mindMap", json!({ "parent": null, "direction": "right", "auto": true, "line": "curved" }));
    }
    let parent = board.find(&parent_id)?.clone();
    let colors = crate::model::COLORS;
    let depth = mind_depth(board, &parent_id) + 1;
    let mut child = Item::new("rect", parent.x + parent.w + 100.0, parent.y);
    child.w = 180.0;
    child.h = 64.0;
    child.color = Some(colors[depth % 5].to_string());
    child.text_align = Some("center".into());
    child.auto_font_size = Some(true);
    let order = board.items.iter().filter(|i| i.mind_parent() == Some(&parent_id)).count();
    child.set("order", order as i64 + 1);
    child.set("mindMap", json!({ "parent": parent_id, "direction": "right", "auto": true, "line": "curved" }));
    let new_id = child.id.clone();
    board.items.push(child);
    layout_mind(board, &mind_root(board, &parent_id));
    Some(new_id)
}

pub fn mind_depth(board: &Board, id: &str) -> usize {
    let mut d = 0;
    let mut cur = board.find(id).and_then(|i| i.mind_parent().map(str::to_string));
    while let Some(p) = cur { d += 1; if d > 64 { break; } cur = board.find(&p).and_then(|i| i.mind_parent().map(str::to_string)); }
    d
}

pub fn mind_root(board: &Board, id: &str) -> String {
    let mut cur = id.to_string();
    for _ in 0..64 { match board.find(&cur).and_then(|i| i.mind_parent().map(str::to_string)) { Some(p) => cur = p, None => break } }
    cur
}

/// Lays out a mind map to the right of its root: each subtree stacked in a column.
pub fn layout_mind(board: &mut Board, root: &str) {
    fn children(board: &Board, id: &str) -> Vec<String> {
        let mut c: Vec<&Item> = board.items.iter().filter(|i| i.mind_parent() == Some(id)).collect();
        c.sort_by(|a, b| a.num("order").unwrap_or(0.0).total_cmp(&b.num("order").unwrap_or(0.0)));
        c.into_iter().map(|i| i.id.clone()).collect()
    }
    fn height(board: &Board, id: &str) -> f64 {
        let own = board.find(id).map(|i| i.h).unwrap_or(0.0);
        let kids = children(board, id);
        if kids.is_empty() { return own; }
        own.max(kids.iter().map(|k| height(board, k)).sum::<f64>() + 24.0 * (kids.len() - 1) as f64)
    }
    fn place(board: &mut Board, id: &str, x: f64, center_y: f64) {
        let (w, h) = match board.find(id) { Some(i) => (i.w, i.h), None => return };
        if let Some(i) = board.find_mut(id) { i.x = x; i.y = center_y - h / 2.0; }
        let kids = children(board, id);
        let total = kids.iter().map(|k| height(board, k)).sum::<f64>() + 24.0 * kids.len().saturating_sub(1) as f64;
        let mut y = center_y - total / 2.0;
        for k in kids {
            let kh = height(board, &k);
            place(board, &k, x + w + 100.0, y + kh / 2.0);
            y += kh + 24.0;
        }
    }
    let Some(r) = board.find(root) else { return };
    let (x, cy) = (r.x, r.y + r.h / 2.0);
    place(board, root, x, cy);
}

/// A new table of `rows` × `cols` empty cells.
pub fn new_table(x: f64, y: f64, rows: usize, cols: usize) -> Item {
    let mut t = Item::new("table", x, y);
    t.w = 120.0 * cols as f64;
    t.h = 40.0 * rows as f64;
    t.color = Some("#ffffff".into());
    t.auto_font_size = None;
    t.set_cells(&vec![vec![String::new(); cols]; rows]);
    t
}

/// Column and row edges of a table, from its stored sizes or evenly (`tableLayout`).
pub fn table_layout(i: &Item) -> (Vec<f64>, Vec<f64>) {
    let cells = i.cells().unwrap_or_default();
    let (rows, cols) = (cells.len().max(1), cells.first().map(|r| r.len()).unwrap_or(1).max(1));
    let tracks = |key: &str, count: usize, total: f64| {
        let v: Option<Vec<f64>> = i.get(key).and_then(|v| serde_json::from_value(v.clone()).ok()).filter(|v: &Vec<f64>| v.len() == count);
        let v = v.unwrap_or_else(|| vec![total / count as f64; count]);
        let sum: f64 = v.iter().sum();
        let mut edges = vec![0.0];
        for s in v { edges.push(edges.last().unwrap() + s * total / sum); }
        edges
    };
    (tracks("columnWidths", cols, i.w), tracks("rowHeights", rows, i.h))
}

/// Adds or removes a row or column at `index`.
pub fn table_insert(i: &mut Item, row: bool, index: usize, remove: bool) {
    let mut cells = i.cells().unwrap_or_default();
    if cells.is_empty() { return; }
    let cols = cells[0].len();
    if row {
        if remove { if cells.len() > 1 { cells.remove(index.min(cells.len() - 1)); i.h -= i.h / (cells.len() + 1) as f64; } }
        else if cells.len() < 20 { cells.insert(index.min(cells.len()), vec![String::new(); cols]); i.h += i.h / (cells.len() - 1) as f64; }
    } else if remove { if cols > 1 { for r in &mut cells { r.remove(index.min(cols - 1)); } i.w -= i.w / cols as f64; } }
    else if cols < 20 { for r in &mut cells { r.insert(index.min(cols), String::new()); } i.w += i.w / cols as f64; }
    i.extra.remove("columnWidths");
    i.extra.remove("rowHeights");
    i.set_cells(&cells);
}

/// The box around a whole mind map or other objects; re-exported for the app.
pub fn items_bounds(items: &[Item]) -> Box2 { bounds(items) }
