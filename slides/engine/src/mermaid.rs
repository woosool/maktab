//! Mermaid diagrams drawn as native shapes (so they print, export and can be ungrouped and
//! edited): flowcharts (`graph`/`flowchart`, all directions, node shapes, edge kinds and
//! labels, subgraphs, `style` fills), state diagrams, sequence diagrams, pie charts, mind
//! maps and timelines. The source is kept on the group to edit again.

use crate::diagram::{line, mix, Node, Paint, Palette};
use crate::pres::Presentation;
use crate::scene::{Loc, Xfrm};
use std::collections::HashMap;

pub const TEMPLATES: &[(&str, &str)] = &[
    ("Flowchart", "flowchart TD\n    A[Start] --> B{Is it working?}\n    B -->|Yes| C[Ship it]\n    B -->|No| D[Debug]\n    D --> B\n    C --> E((Done))"),
    ("Left to right", "graph LR\n    idea([Idea]) --> plan[Plan] --> build[[Build]] --> test{Test}\n    test -- pass --> launch>Launch]\n    test -. fail .-> build"),
    ("Subgraphs", "flowchart TB\n    subgraph Frontend\n        ui[Web app] --> api\n    end\n    subgraph Backend\n        api[API] --> db[(Database)]\n        api --> cache[(Cache)]\n    end"),
    ("Sequence", "sequenceDiagram\n    participant U as User\n    participant A as App\n    participant S as Server\n    U->>A: Open slides\n    A->>S: Fetch deck\n    S-->>A: Deck data\n    Note over A: Render\n    A-->>U: Show slides"),
    ("State", "stateDiagram-v2\n    [*] --> Draft\n    Draft --> Review : submit\n    Review --> Draft : changes\n    Review --> Published : approve\n    Published --> [*]"),
    ("Pie", "pie title Time spent\n    \"Design\" : 35\n    \"Build\" : 40\n    \"Test\" : 15\n    \"Docs\" : 10"),
    ("Mind map", "mindmap\n  root((Sard))\n    Slides\n      Themes\n      Layouts\n    Animations\n      Entrance\n      Exit\n    Show\n      Presenter view"),
    ("Gantt", "gantt
    title Release plan
    dateFormat YYYY-MM-DD
    section Design
    Research      :done, r1, 2026-01-05, 10d
    Mockups       :active, m1, after r1, 12d
    section Build
    Engine        :crit, e1, after m1, 20d
    App           :a1, after m1, 25d
    section Launch
    Beta          :milestone, after a1, 0d
    Release       :after a1, 7d"),
    ("Class", "classDiagram
    class Shape {
        +String name
        +area() float
    }
    class Circle {
        +float radius
    }
    class Square {
        +float side
    }
    Shape <|-- Circle
    Shape <|-- Square
    Slide \"1\" *-- \"many\" Shape : contains"),
    ("Entity relationship", "erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : contains
    PRODUCT ||--o{ LINE_ITEM : \"ordered in\"
    CUSTOMER {
        string name
        string email PK
    }
    ORDER {
        int id PK
        date placed
    }"),
    ("Timeline", "timeline\n    title Product history\n    2024 : Idea\n    2025 : First release : Arabic support\n    2026 : Sard 1.0"),
];

#[derive(Clone, Debug, PartialEq)]
struct GNode { id: String, label: String, shape: String, fill: Option<String> }
#[derive(Clone, Debug, PartialEq)]
struct Edge { from: usize, to: usize, label: String, dotted: bool, thick: bool, arrow: bool }
#[derive(Default, Debug)]
struct Graph { nodes: Vec<GNode>, edges: Vec<Edge>, dir: String, groups: Vec<(String, Vec<usize>)> }

impl Graph {
    fn node(&mut self, id: &str, label: Option<&str>, shape: Option<&str>) -> usize {
        if let Some(i) = self.nodes.iter().position(|n| n.id == id) {
            if let Some(l) = label { self.nodes[i].label = l.into(); }
            if let Some(s) = shape { self.nodes[i].shape = s.into(); }
            return i;
        }
        self.nodes.push(GNode { id: id.into(), label: label.unwrap_or(id).into(), shape: shape.unwrap_or("rect").into(), fill: None });
        self.nodes.len() - 1
    }
}

fn unquote(s: &str) -> String { s.trim().trim_matches('"').replace("<br>", "\n").replace("<br/>", "\n") }

/// `id[label]`, `id(label)`, `id{label}`… → (id, label, shape).
fn node_token(t: &str) -> Option<(String, Option<String>, Option<&'static str>)> {
    let t = t.trim();
    if t.is_empty() { return None; }
    let forms: &[(&str, &str, &str)] = &[("([", "])", "stadium"), ("[[", "]]", "subroutine"), ("[(", ")]", "database"), ("((", "))", "circle"), ("{{", "}}", "hexagon"), ("[/", "/]", "parallelogram"), ("[\\", "\\]", "parallelogramAlt"), ("[/", "\\]", "trapezoid"), ("[", "]", "rect"), ("(", ")", "round"), ("{", "}", "diamond"), (">", "]", "flag")];
    for (open, close, shape) in forms {
        if let Some(k) = t.find(open) {
            if t.ends_with(close) && k > 0 {
                let id = t[..k].trim().to_string();
                let label = unquote(&t[k + open.len()..t.len() - close.len()]);
                return Some((id, Some(label), Some(shape)));
            }
        }
    }
    let id = t.split_whitespace().next()?.trim_end_matches(';').to_string();
    Some((id, None, None))
}

/// Split a flowchart line on its edges: nodes and (label, dotted, thick, arrow) between them.
fn split_edges(line: &str) -> (Vec<String>, Vec<(String, bool, bool, bool)>) {
    let mut nodes = vec![];
    let mut edges = vec![];
    let b = line.as_bytes();
    let mut i = 0;
    let mut start = 0;
    let mut depth = 0i32;
    while i < b.len() {
        let c = b[i] as char;
        if "[({".contains(c) { depth += 1; }
        if "])}".contains(c) { depth -= 1; }
        if depth == 0 && (line[i..].starts_with("--") || line[i..].starts_with("==") || line[i..].starts_with("-.")) {
            let seg_start = i;
            // The edge runs to its end marker (>, - or =), with an optional inline label.
            let rest = &line[i..];
            let (mut j, mut label) = (0usize, String::new());
            let dotted = rest.starts_with("-.");
            let thick = rest.starts_with("==");
            // Inline text: "-- text -->" / "-. text .->" / "== text ==>".
            let closers = ["-->", "---", "==>", "===", ".->", ".-"];
            let first_len = rest.chars().take_while(|c| "-=.".contains(*c)).count();
            let after_first = &rest[first_len..];
            let ends_now = after_first.starts_with('>') || after_first.starts_with(' ') && closers.iter().all(|c| !after_first.contains(c)) || after_first.starts_with('|') || !after_first.starts_with(' ');
            if !ends_now || (after_first.starts_with(' ') && closers.iter().any(|c| after_first.contains(c)) && !after_first.trim_start().starts_with(|ch: char| ch.is_alphanumeric() && after_first.trim_start().split_whitespace().next().is_some_and(|w| w.contains(['[', '(', '{']) || after_first.trim_start().split_whitespace().nth(1).is_none_or(|n| !n.starts_with("-") && !n.starts_with("=") && !n.starts_with(".")))) && false) {}
            let mut arrow = false;
            j += first_len;
            if after_first.starts_with('>') { arrow = true; j += 1; }
            else if first_len <= 2 && after_first.starts_with(' ') {
                // Look for a closer later on the line: then what's between is the label.
                if let Some((k, cl)) = closers.iter().filter_map(|cl| after_first.find(cl).map(|k| (k, *cl))).min_by_key(|x| x.0) {
                    let between = &after_first[..k];
                    if !between.trim().is_empty() && !between.contains(['[', '(', '{']) {
                        label = between.trim().to_string();
                        j += k + cl.len();
                        arrow = cl.ends_with('>');
                        let more = &line[seg_start + j..];
                        if more.starts_with('>') { arrow = true; j += 1; }
                    }
                }
            }
            let mut tail = &line[seg_start + j..];
            if tail.starts_with('|') { if let Some(k) = tail[1..].find('|') { label = tail[1..1 + k].trim().to_string(); j += k + 2; tail = &line[seg_start + j..]; } }
            let _ = tail;
            nodes.push(line[start..seg_start].to_string());
            edges.push((unquote(&label), dotted, thick, arrow));
            i = seg_start + j;
            start = i;
            continue;
        }
        i += c.len_utf8();
    }
    nodes.push(line[start..].to_string());
    (nodes, edges)
}

fn parse_flowchart(src: &str, state: bool) -> Graph {
    let mut g = Graph { dir: "TB".into(), ..Default::default() };
    let mut group: Option<usize> = None;
    let mut starts = 0;
    for (k, raw) in src.lines().enumerate() {
        let l = raw.trim().trim_end_matches(';');
        if l.is_empty() || l.starts_with("%%") { continue; }
        if k == 0 || l.starts_with("graph") || l.starts_with("flowchart") || l.starts_with("stateDiagram") {
            if let Some(d) = l.split_whitespace().nth(1) { g.dir = d.to_uppercase(); }
            if l.starts_with("graph") || l.starts_with("flowchart") || l.starts_with("stateDiagram") { continue; }
        }
        if let Some(t) = l.strip_prefix("direction ") { g.dir = t.trim().to_uppercase(); continue; }
        if let Some(t) = l.strip_prefix("subgraph") { let title = node_token(t).map(|n| n.1.unwrap_or(n.0)).unwrap_or_default(); g.groups.push((title, vec![])); group = Some(g.groups.len() - 1); continue; }
        if l == "end" { group = None; continue; }
        if l.starts_with("classDef") || l.starts_with("class ") || l.starts_with("click") || l.starts_with("linkStyle") || l.starts_with("note") { continue; }
        if let Some(t) = l.strip_prefix("style ") {
            let mut it = t.splitn(2, ' ');
            let (id, props) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            if let Some(fill) = props.split(',').find_map(|p| p.trim().strip_prefix("fill:")) { let i = g.node(id, None, None); g.nodes[i].fill = Some(fill.trim().trim_start_matches('#').to_string()); }
            continue;
        }
        // State diagrams: "A --> B : label", "[*]" as start/end, "state "x" as Y".
        let (body, slabel) = if state { match l.split_once(" : ") { Some((a, b)) => (a, Some(b.trim().to_string())), None => (l, None) } } else { (l, None) };
        if state && body.starts_with("state ") { if let Some((name, id)) = body[6..].split_once(" as ") { g.node(id.trim(), Some(&unquote(name)), Some("round")); } continue; }
        let (parts, edges) = split_edges(body);
        let mut prev: Vec<usize> = vec![];
        for (pi, part) in parts.iter().enumerate() {
            let mut cur = vec![];
            for tok in part.split('&') {
                let tok = tok.trim();
                if tok.is_empty() { continue; }
                let id = if state && tok == "[*]" { starts += 1; if pi == 0 { "__start".to_string() } else { format!("__end{starts}") } } else { String::new() };
                let i = if !id.is_empty() { let shape = if id == "__start" { "start" } else { "end" }; g.node(&id, Some(""), Some(shape)) } else {
                    let Some((id, label, shape)) = node_token(tok) else { continue };
                    g.node(&id, label.as_deref(), shape.or(if state { Some("round") } else { None }))
                };
                if let Some(gr) = group { if !g.groups[gr].1.contains(&i) { g.groups[gr].1.push(i); } }
                cur.push(i);
            }
            if pi > 0 {
                let (label, dotted, thick, arrow) = edges[pi - 1].clone();
                let label = if label.is_empty() { slabel.clone().unwrap_or_default() } else { label };
                for &a in &prev { for &b in &cur { g.edges.push(Edge { from: a, to: b, label: label.clone(), dotted, thick, arrow: arrow || state }); } }
            }
            prev = cur;
        }
    }
    g
}

/// Layered layout: ranks by longest path (cycles broken), ordering by barycenters.
fn place(g: &Graph) -> (Vec<(f64, f64)>, usize, usize) {
    let n = g.nodes.len();
    let mut rank = vec![0usize; n];
    // Back edges (found by DFS) are ignored for ranking.
    let mut state = vec![0u8; n];
    let mut back = vec![false; g.edges.len()];
    fn dfs(u: usize, g: &Graph, st: &mut [u8], back: &mut [bool]) {
        st[u] = 1;
        for (k, e) in g.edges.iter().enumerate().filter(|(_, e)| e.from == u) {
            match st[e.to] { 1 => back[k] = true, 0 => dfs(e.to, g, st, back), _ => {} }
        }
        st[u] = 2;
    }
    for u in 0..n { if state[u] == 0 { dfs(u, g, &mut state, &mut back); } }
    for _ in 0..n {
        let mut changed = false;
        for (k, e) in g.edges.iter().enumerate() { if !back[k] && e.from != e.to && rank[e.to] < rank[e.from] + 1 { rank[e.to] = rank[e.from] + 1; changed = true; } }
        if !changed { break; }
    }
    let ranks = rank.iter().max().map(|r| r + 1).unwrap_or(1);
    let mut rows: Vec<Vec<usize>> = vec![vec![]; ranks];
    for i in 0..n { rows[rank[i]].push(i); }
    // Barycenter sweeps, down then up.
    for sweep in 0..6 {
        let pos: HashMap<usize, f64> = rows.iter().flat_map(|r| r.iter().enumerate().map(|(k, &i)| (i, k as f64 / r.len().max(1) as f64))).collect();
        let order: Vec<usize> = if sweep % 2 == 0 { (1..ranks).collect() } else { (0..ranks.saturating_sub(1)).rev().collect() };
        for r in order {
            let mut keyed: Vec<(f64, usize)> = rows[r].iter().map(|&i| {
                let nb: Vec<f64> = g.edges.iter().filter_map(|e| if e.to == i && rank[e.from] != r { pos.get(&e.from).copied() } else if e.from == i && rank[e.to] != r { pos.get(&e.to).copied() } else { None }).collect();
                (if nb.is_empty() { pos[&i] } else { nb.iter().sum::<f64>() / nb.len() as f64 }, i)
            }).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            rows[r] = keyed.into_iter().map(|k| k.1).collect();
        }
    }
    // Then swap neighbours while that removes crossings (with the rows above and below).
    let cross = |rows: &Vec<Vec<usize>>, r: usize| -> usize {
        let mut c = 0;
        for adj in [r.wrapping_sub(1), r + 1] {
            let Some(other) = rows.get(adj) else { continue };
            let pos = |i: usize, row: &Vec<usize>| row.iter().position(|&x| x == i);
            let links: Vec<(usize, usize)> = g.edges.iter().filter_map(|e| {
                let (a, b) = if rank[e.from] == r && rank[e.to] == adj { (e.from, e.to) } else if rank[e.to] == r && rank[e.from] == adj { (e.to, e.from) } else { return None };
                Some((pos(a, &rows[r])?, pos(b, other)?))
            }).collect();
            for x in 0..links.len() { for y in x + 1..links.len() { if (links[x].0 < links[y].0) != (links[x].1 < links[y].1) && links[x].0 != links[y].0 && links[x].1 != links[y].1 { c += 1; } } }
        }
        c
    };
    for _ in 0..4 {
        let mut better = false;
        for r in 0..ranks {
            for k in 0..rows[r].len().saturating_sub(1) {
                let before = cross(&rows, r);
                rows[r].swap(k, k + 1);
                if cross(&rows, r) < before { better = true; } else { rows[r].swap(k, k + 1); }
            }
        }
        if !better { break; }
    }
    let widest = rows.iter().map(|r| r.len()).max().unwrap_or(1);
    let mut at = vec![(0.0, 0.0); n];
    for (r, row) in rows.iter().enumerate() {
        for (k, &i) in row.iter().enumerate() {
            // Centered rows: slot k of the row's width within the widest row.
            let x = (widest as f64 - row.len() as f64) / 2.0 + k as f64 + 0.5;
            at[i] = (x, r as f64 + 0.5);
        }
    }
    (at, widest, ranks)
}

fn preset_of(shape: &str) -> &'static str {
    match shape { "round" => "roundRect", "stadium" => "flowChartTerminator", "subroutine" => "flowChartPredefinedProcess", "database" => "can", "circle" | "start" | "end" => "ellipse", "hexagon" => "hexagon", "parallelogram" | "parallelogramAlt" => "parallelogram", "trapezoid" => "trapezoid", "diamond" => "diamond", "flag" => "homePlate", _ => "rect" }
}

/// Where the segment from the center c toward p leaves a box of half sizes (hw, hh).
fn clip(c: (f64, f64), p: (f64, f64), hw: f64, hh: f64, round: bool) -> (f64, f64) {
    let (dx, dy) = (p.0 - c.0, p.1 - c.1);
    if dx == 0.0 && dy == 0.0 { return c; }
    let t = if round { 1.0 / ((dx / hw).powi(2) + (dy / hh).powi(2)).sqrt() } else { (hw / dx.abs().max(1e-9)).min(hh / dy.abs().max(1e-9)) };
    (c.0 + dx * t.min(1.0), c.1 + dy * t.min(1.0))
}

/// A right-angled path from box a (center, size) to box b along the flow's main axis.
#[allow(clippy::too_many_arguments)]
fn route(a: (f64, f64), sa: (f64, f64), b: (f64, f64), sb: (f64, f64), horiz: bool, sign: f64, forward: bool, lane: f64, offset: (f64, f64)) -> Vec<(f64, f64)> {
    // (main, cross) coordinates and back.
    let mc = |p: (f64, f64)| if horiz { (p.0, p.1) } else { (p.1, p.0) };
    let xy = |m: f64, c: f64| if horiz { (m, c) } else { (c, m) };
    let ext = |s: (f64, f64)| if horiz { (s.0, s.1) } else { (s.1, s.0) };
    let ((am, ac), (bm, bc)) = (mc(a), mc(b));
    let ((ae, ax), (be, bx)) = (ext(sa), ext(sb));
    if forward {
        let (s, e) = ((am + sign * ae / 2.0, ac + offset.0), (bm - sign * be / 2.0, bc + offset.1));
        if (s.1 - e.1).abs() < 1_000.0 { return vec![xy(s.0, s.1), xy(e.0, s.1)]; }
        // The bend comes just before the target, clear of boxes on ranks in between.
        let mid = e.0 - sign * ((e.0 - s.0).abs() / 2.0).min(320_000.0);
        vec![xy(s.0, s.1), xy(mid, s.1), xy(mid, e.1), xy(e.0, e.1)]
    } else {
        let side = (ac + ax / 2.0).max(bc + bx / 2.0) + 200_000.0 + lane;
        vec![xy(am + offset.0, ac + ax / 2.0), xy(am + offset.0, side), xy(bm + offset.1, side), xy(bm + offset.1, bc + bx / 2.0)]
    }
}

/// Lines along points; arrows at the end (and start).
fn polyline(pts: &[(f64, f64)], style: Option<(String, f64, Option<String>)>, end: Option<&'static str>, start: Option<&'static str>) -> Vec<Node> {
    let n = pts.len();
    (1..n).map(|k| {
        let mut l = line(pts[k - 1].0, pts[k - 1].1, pts[k].0, pts[k].1, if k == n - 1 || k == 1 { "straightConnector1" } else { "line" });
        l.line = style.clone();
        l.arrows = (if k == 1 { start } else { None }, if k == n - 1 { end } else { None });
        l
    }).collect()
}

fn flow_nodes(g: &Graph, fr: &Xfrm, pal: &Palette) -> Vec<Node> {
    if g.nodes.is_empty() { return vec![]; }
    let (at, widest, ranks) = place(g);
    let horiz = matches!(g.dir.as_str(), "LR" | "RL");
    let (cols, rows) = if horiz { (ranks, widest) } else { (widest, ranks) };
    let (cw, rh) = (fr.w / cols as f64, fr.h / rows as f64);
    let (bw, bh) = ((cw * 0.72).min(2_400_000.0), (rh * 0.5).min(900_000.0));
    let center = |i: usize| -> (f64, f64) {
        let (a, r) = at[i];
        let (mut x, mut y) = if horiz { (r, a) } else { (a, r) };
        if g.dir == "BT" { y = rows as f64 - y; }
        if g.dir == "RL" { x = cols as f64 - x; }
        (fr.x + x * cw, fr.y + y * rh)
    };
    let size = |i: usize| -> (f64, f64) {
        match g.nodes[i].shape.as_str() { "start" | "end" => (bh * 0.4, bh * 0.4), "circle" => (bh * 1.2, bh * 1.2), "diamond" => (bw * 1.0, bh * 1.35), _ => (bw, bh) }
    };
    let mut out = vec![];
    // Subgraph frames first (under everything).
    for (title, members) in &g.groups {
        if members.is_empty() { continue; }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &m in members { let (cx, cy) = center(m); let (w, h) = size(m); x0 = x0.min(cx - w / 2.0); y0 = y0.min(cy - h / 2.0); x1 = x1.max(cx + w / 2.0); y1 = y1.max(cy + h / 2.0); }
        let pad = bh * 0.35;
        let mut b = Node::shape("rect", Xfrm { x: x0 - pad, y: y0 - pad * 2.2, w: x1 - x0 + pad * 2.0, h: y1 - y0 + pad * 3.2, ..Default::default() }, title, 10);
        b.fill = Paint::Solid(mix(&pal.accents[0], 0.88));
        b.line = Some((mix(&pal.accents[0], 0.3), 1.0, Some("dash".into())));
        b.text_color = mix(&pal.accents[0], -0.4);
        b.align = "l";
        b.anchor = "t";
        b.bold = true;
        out.push(b);
    }
    // Edges run at right angles: out of the side facing the next rank, across halfway, in.
    // Edges back up (or within a rank) go round the outside in their own lane.
    let sign = if matches!(g.dir.as_str(), "BT" | "RL") { -1.0 } else { 1.0 };
    let mut lanes = 0.0;
    let flags: Vec<(usize, usize, bool)> = g.edges.iter().map(|e| (e.from, e.to, at[e.to].1 > at[e.from].1)).collect();
    let crossx = |i: usize| { let (w, h) = size(i); if horiz { h } else { w } };
    let orderx = |i: usize| at[i].0;
    let port = ports(&flags, &crossx, &orderx);
    for (k, e) in g.edges.iter().enumerate() {
        let (a, b) = (center(e.from), center(e.to));
        let forward = at[e.to].1 > at[e.from].1;
        let both = forward && g.edges.iter().any(|o| o.from == e.to && o.to == e.from);
        let lane = if forward { 0.0 } else { lanes += 1.0; (if horiz { rh } else { cw }) * 0.12 * lanes };
        let off = if both { (bh * 0.15, bh * 0.15) } else { port[k] };
        let pts = route(a, size(e.from), b, size(e.to), horiz, sign, forward, lane, off);
        let style = Some((mix(&pal.dk2, 0.15), if e.thick { 2.75 } else { 1.5 }, e.dotted.then(|| "dash".to_string())));
        out.extend(polyline(&pts, style, if e.arrow { Some("triangle") } else { None }, None));
        if !e.label.is_empty() {
            let m = pts.len() / 2;
            let (p1, p2) = (pts[m - 1], pts[m]);
            let (mx, my) = ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0);
            let (tw, th) = ((e.label.chars().count() as f64 * 70_000.0 + 120_000.0).min(cw * 0.8), 230_000.0);
            let mut t = Node::shape("rect", Xfrm { x: mx - tw / 2.0, y: my - th / 2.0, w: tw, h: th, ..Default::default() }, &e.label, 20);
            t.fill = Paint::Solid(pal.lt1.clone());
            t.text_color = pal.dk2.clone();
            t.size = 11.0;
            out.push(t);
        }
        let _ = k;
    }
    for (i, n) in g.nodes.iter().enumerate() {
        let (cx, cy) = center(i);
        let (w, h) = size(i);
        let mut b = Node::shape(preset_of(&n.shape), Xfrm { x: cx - w / 2.0, y: cy - h / 2.0, w, h, ..Default::default() }, &n.label, 0);
        let fill = n.fill.clone().unwrap_or_else(|| match n.shape.as_str() { "diamond" => pal.accents[1].clone(), "start" | "end" => pal.dk2.clone(), "database" => pal.accents[4].clone(), _ => pal.accents[0].clone() });
        b.fill = Paint::Gradient(mix(&fill, 0.18), mix(&fill, -0.1));
        b.line = Some((mix(&fill, -0.3), if n.shape == "end" { 3.0 } else { 1.0 }, None));
        b.text_color = if crate::diagram::mix(&fill, 0.0) == fill && { let v = u32::from_str_radix(&fill, 16).unwrap_or(0); (0.299 * (v >> 16 & 255) as f64 + 0.587 * (v >> 8 & 255) as f64 + 0.114 * (v & 255) as f64) > 165.0 } { pal.dk1.clone() } else { "FFFFFF".into() };
        b.shadow = true;
        if n.shape == "round" { b.adjust = vec![("adj", 22_000.0)]; }
        if n.shape == "parallelogramAlt" { b.x.flip_h = true; }
        out.push(b);
    }
    out
}

fn sequence_nodes(src: &str, fr: &Xfrm, pal: &Palette) -> Vec<Node> {
    let mut parts: Vec<(String, String)> = vec![];
    let mut msgs: Vec<(usize, usize, String, bool, bool, Option<String>)> = vec![]; // from, to, text, dashed, arrow, note
    let mut idx = |id: &str, label: Option<&str>, parts: &mut Vec<(String, String)>| -> usize {
        if let Some(i) = parts.iter().position(|p| p.0 == id) { if let Some(l) = label { parts[i].1 = l.into(); } return i; }
        parts.push((id.into(), label.unwrap_or(id).into()));
        parts.len() - 1
    };
    for l in src.lines().skip(1) {
        let l = l.trim();
        if l.is_empty() || l.starts_with("%%") || l.starts_with("autonumber") || l.starts_with("activate") || l.starts_with("deactivate") || l == "end" || l.starts_with("loop") || l.starts_with("alt") || l.starts_with("else") || l.starts_with("opt") { continue; }
        if let Some(t) = l.strip_prefix("participant ").or_else(|| l.strip_prefix("actor ")) {
            match t.split_once(" as ") { Some((id, lab)) => { idx(id.trim(), Some(lab.trim()), &mut parts); } None => { idx(t.trim(), None, &mut parts); } }
            continue;
        }
        if let Some(t) = l.strip_prefix("Note ").or_else(|| l.strip_prefix("note ")) {
            if let Some((who, text)) = t.split_once(':') {
                let who = who.trim().trim_start_matches("over").trim_start_matches("right of").trim_start_matches("left of").trim();
                let first = who.split(',').next().unwrap_or("").trim();
                let i = idx(first, None, &mut parts);
                msgs.push((i, i, String::new(), false, false, Some(text.trim().to_string())));
            }
            continue;
        }
        for (op, dashed, arrow) in [("-->>", true, true), ("->>", false, true), ("-->", true, false), ("->", false, false), ("--x", true, true), ("-x", false, true), ("--)", true, true), ("-)", false, true)] {
            if let Some((a, rest)) = l.split_once(op) {
                let (b, text) = rest.split_once(':').unwrap_or((rest, ""));
                let (i, j) = (idx(a.trim().trim_end_matches(['+', '-']), None, &mut parts), idx(b.trim().trim_start_matches(['+', '-']), None, &mut parts));
                msgs.push((i, j, text.trim().to_string(), dashed, arrow, None));
                break;
            }
        }
    }
    if parts.is_empty() { return vec![]; }
    let np = parts.len() as f64;
    let cw = fr.w / np;
    let (bw, bh) = ((cw * 0.75).min(2_200_000.0), (fr.h * 0.11).min(600_000.0));
    let rows = msgs.len() as f64 + 1.0;
    let rh = (fr.h - bh * 2.0) / rows;
    let cx = |i: usize| fr.x + cw * (i as f64 + 0.5);
    let mut out = vec![];
    for (i, (_, label)) in parts.iter().enumerate() {
        let mut life = line(cx(i), fr.y + bh, cx(i), fr.y + fr.h - bh, "line");
        life.line = Some((mix(&pal.dk2, 0.45), 1.0, Some("dash".into())));
        out.push(life);
        for y in [fr.y, fr.y + fr.h - bh] {
            let mut b = Node::shape("roundRect", Xfrm { x: cx(i) - bw / 2.0, y, w: bw, h: bh, ..Default::default() }, label, 0);
            b.fill = Paint::Gradient(mix(&pal.accents[0], 0.15), mix(&pal.accents[0], -0.1));
            b.line = Some((mix(&pal.accents[0], -0.3), 1.0, None));
            b.shadow = true;
            out.push(b);
        }
    }
    for (k, (i, j, text, dashed, arrow, note)) in msgs.iter().enumerate() {
        let y = fr.y + bh + rh * (k as f64 + 0.8);
        if let Some(n) = note {
            let w = (cw * 0.9).max(bw);
            let mut b = Node::shape("foldedCorner", Xfrm { x: cx(*i) - w / 2.0, y: y - rh * 0.4, w, h: rh * 0.75, ..Default::default() }, n, 5);
            b.fill = Paint::Solid("FFF2CC".into());
            b.line = Some(("BF9000".into(), 0.75, None));
            b.text_color = "3D3000".into();
            out.push(b);
            continue;
        }
        let (x1, x2) = if i == j { (cx(*i), cx(*i) + cw * 0.35) } else { (cx(*i), cx(*j)) };
        if i == j {
            // A message to itself: out, down and back.
            let mut a = line(x1, y - rh * 0.25, x2, y - rh * 0.25, "line"); a.line = Some((pal.dk1.clone(), 1.25, None)); out.push(a);
            let mut b = line(x2, y - rh * 0.25, x2, y + rh * 0.1, "line"); b.line = Some((pal.dk1.clone(), 1.25, None)); out.push(b);
            let mut c = line(x2, y + rh * 0.1, x1, y + rh * 0.1, "straightConnector1"); c.line = Some((pal.dk1.clone(), 1.25, None)); c.arrows = (None, Some("triangle")); out.push(c);
        } else {
            let mut a = line(x1, y, x2, y, "straightConnector1");
            a.line = Some((pal.dk1.clone(), 1.25, dashed.then(|| "dash".to_string())));
            if *arrow { a.arrows = (None, Some("triangle")); } else { a.arrows = (None, Some("arrow")); }
            out.push(a);
        }
        if !text.is_empty() {
            let w = (x2 - x1).abs().max(cw * 0.8);
            let mut t = Node::shape("rect", Xfrm { x: (x1 + x2) / 2.0 - w / 2.0, y: y - rh * 0.55, w, h: rh * 0.5, ..Default::default() }, text, 6);
            t.fill = Paint::None;
            t.text_color = pal.dk1.clone();
            t.anchor = "b";
            out.push(t);
        }
    }
    out
}

fn pie_nodes(src: &str, fr: &Xfrm, pal: &Palette) -> Vec<Node> {
    let mut title = String::new();
    let mut data: Vec<(String, f64)> = vec![];
    for (k, l) in src.lines().enumerate() {
        let l = l.trim();
        if k == 0 { if let Some(t) = l.split_once("title") { title = t.1.trim().to_string(); } continue; }
        if let Some(t) = l.strip_prefix("title ") { title = t.trim().into(); continue; }
        if let Some((a, b)) = l.rsplit_once(':') { if let Ok(v) = b.trim().parse::<f64>() { data.push((unquote(a), v.max(0.0))); } }
    }
    let total: f64 = data.iter().map(|d| d.1).sum();
    if total <= 0.0 { return vec![]; }
    let mut out = vec![];
    let th = if title.is_empty() { 0.0 } else { fr.h * 0.12 };
    if !title.is_empty() { let mut t = Node::shape("rect", Xfrm { x: fr.x, y: fr.y, w: fr.w, h: th, ..Default::default() }, &title, 7); t.fill = Paint::None; t.text_color = pal.dk1.clone(); t.bold = true; out.push(t); }
    let d = (fr.h - th).min(fr.w * 0.55);
    let (px, py) = (fr.x, fr.y + th + (fr.h - th - d) / 2.0);
    let mut a = -90.0f64;
    let lx = px + d + fr.w * 0.05;
    let lh = ((fr.h - th) / data.len().max(1) as f64).min(400_000.0);
    for (k, (label, v)) in data.iter().enumerate() {
        let sweep = v / total * 360.0;
        let c = pal.accents[k % 6].clone();
        let mut s = Node::shape("pie", Xfrm { x: px, y: py, w: d, h: d, ..Default::default() }, "", 0);
        s.adjust = vec![("adj1", a.rem_euclid(360.0) * 60_000.0), ("adj2", (a + sweep).rem_euclid(360.0) * 60_000.0)];
        s.fill = Paint::Solid(c.clone());
        s.line = Some((pal.lt1.clone(), 1.5, None));
        out.push(s);
        // The percentage inside the slice.
        let mid = (a + sweep / 2.0).to_radians();
        let (tw, tht) = (d * 0.22, d * 0.1);
        let mut p = Node::shape("rect", Xfrm { x: px + d / 2.0 + d * 0.32 * mid.cos() - tw / 2.0, y: py + d / 2.0 + d * 0.32 * mid.sin() - tht / 2.0, w: tw, h: tht, ..Default::default() }, &format!("{:.0}%", v / total * 100.0), 8);
        p.fill = Paint::None;
        p.text_color = "FFFFFF".into();
        p.bold = true;
        if sweep > 12.0 { out.push(p); }
        // The legend.
        let y = fr.y + th + k as f64 * lh + ((fr.h - th) - lh * data.len() as f64) / 2.0;
        let mut sw = Node::shape("rect", Xfrm { x: lx, y: y + lh * 0.25, w: lh * 0.5, h: lh * 0.5, ..Default::default() }, "", 60);
        sw.fill = Paint::Solid(c);
        out.push(sw);
        let mut t = Node::shape("rect", Xfrm { x: lx + lh * 0.7, y, w: fr.x + fr.w - lx - lh * 0.7, h: lh, ..Default::default() }, &format!("{label} ({v})"), 9);
        t.fill = Paint::None;
        t.text_color = pal.dk1.clone();
        t.align = "l";
        out.push(t);
        a += sweep;
    }
    out
}

/// Mind maps: indentation is the tree; drawn as a horizontal hierarchy from the root.
fn mindmap_items(src: &str) -> Vec<(u8, String)> {
    let lines: Vec<&str> = src.lines().skip(1).filter(|l| !l.trim().is_empty()).collect();
    let base = lines.iter().map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    let mut indents: Vec<usize> = vec![];
    lines.iter().map(|l| {
        let ind = l.len() - l.trim_start().len() - base;
        while indents.last().is_some_and(|&i| i >= ind && !indents.is_empty()) { if indents.last() == Some(&ind) { break; } indents.pop(); }
        if indents.last() != Some(&ind) { indents.push(ind); }
        let level = indents.len().saturating_sub(1).min(4) as u8;
        let t = l.trim();
        let label = node_token(t).and_then(|n| n.1).unwrap_or_else(|| t.to_string());
        (level, label)
    }).collect()
}

fn timeline_items(src: &str) -> (String, Vec<(u8, String)>) {
    let mut title = String::new();
    let mut items = vec![];
    for l in src.lines().skip(1) {
        let l = l.trim();
        if let Some(t) = l.strip_prefix("title ") { title = t.into(); continue; }
        if l.is_empty() || l.starts_with("section") { continue; }
        let mut parts = l.split(':').map(str::trim);
        let period = parts.next().unwrap_or("");
        if !period.is_empty() { items.push((0, period.to_string())); }
        for e in parts { if !e.is_empty() { items.push((1, e.to_string())); } }
    }
    (title, items)
}


// ---- Gantt charts -----------------------------------------------------------------------------

/// Days since 1970-01-01 of "YYYY-MM-DD".
fn day(s: &str) -> Option<i64> {
    let mut it = s.trim().split('-').map(|p| p.parse::<i64>());
    let (y, m, d) = (it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?);
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}
fn date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{}-{m:02}-{d:02}", yoe + era * 400 + if m <= 2 { 1 } else { 0 })
}
fn duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let (n, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit() && c != '.')?);
    let n: f64 = n.parse().ok()?;
    Some((n * match unit { "w" => 7.0, "h" => 1.0 / 24.0, _ => 1.0 }).ceil().max(0.0) as i64)
}

fn gantt_nodes(src: &str, fr: &Xfrm, pal: &Palette) -> Vec<Node> {
    let mut title = String::new();
    let mut section = String::new();
    // (section, name, start, end, done, active, crit, milestone)
    let mut tasks: Vec<(String, String, i64, i64, bool, bool, bool, bool)> = vec![];
    let mut ids: HashMap<String, i64> = HashMap::new();
    let mut last_end = day("2026-01-01").unwrap_or(0);
    for l in src.lines().skip(1) {
        let l = l.trim();
        if l.is_empty() || l.starts_with("%%") || l.starts_with("dateFormat") || l.starts_with("axisFormat") || l.starts_with("excludes") || l.starts_with("todayMarker") { continue; }
        if let Some(t) = l.strip_prefix("title ") { title = t.trim().into(); continue; }
        if let Some(t) = l.strip_prefix("section ") { section = t.trim().into(); continue; }
        let Some((name, spec)) = l.split_once(':') else { continue };
        let mut parts: Vec<&str> = spec.split(',').map(str::trim).collect();
        let flag = |p: &mut Vec<&str>, f: &str| { let had = p.first() == Some(&f); if had { p.remove(0); } had };
        let (mut done, mut active, mut crit, mut mile) = (false, false, false, false);
        for _ in 0..4 { done |= flag(&mut parts, "done"); active |= flag(&mut parts, "active"); crit |= flag(&mut parts, "crit"); mile |= flag(&mut parts, "milestone"); }
        // [id], [start], end-or-duration.
        let (id, start, end) = match parts.len() {
            0 => continue,
            1 => (None, None, parts[0]),
            2 => (None, Some(parts[0]), parts[1]),
            _ => (Some(parts[0]), Some(parts[1]), parts[2]),
        };
        let s = match start {
            Some(st) if st.starts_with("after ") => st[6..].split_whitespace().filter_map(|i| ids.get(i).copied()).max().unwrap_or(last_end),
            Some(st) => day(st).unwrap_or(last_end),
            None => last_end,
        };
        let e = day(end).or_else(|| duration(end).map(|d| s + d)).unwrap_or(s + 1);
        if let Some(id) = id { ids.insert(id.to_string(), e); }
        last_end = e;
        tasks.push((section.clone(), name.trim().to_string(), s, e.max(s), done, active, crit, mile));
    }
    if tasks.is_empty() { return vec![]; }
    let (t0, t1) = (tasks.iter().map(|t| t.2).min().unwrap(), tasks.iter().map(|t| t.3).max().unwrap().max(tasks.iter().map(|t| t.2).min().unwrap() + 1));
    let mut out = vec![];
    let top = if title.is_empty() { 0.0 } else { fr.h * 0.1 };
    if !title.is_empty() { let mut t = Node::shape("rect", Xfrm { h: top, ..*fr }, &title, 7); t.fill = Paint::None; t.text_color = pal.dk1.clone(); t.bold = true; t.size = 18.0; out.push(t); }
    let label_w = fr.w * 0.26;
    let (cx, cw) = (fr.x + label_w, fr.w - label_w);
    let axis_h = fr.h * 0.08;
    let rows = tasks.len() as f64;
    let rh = (fr.h - top - axis_h) / rows;
    let x_of = |d: i64| cx + (d - t0) as f64 / (t1 - t0) as f64 * cw;
    // Section bands and names.
    let mut k = 0;
    let mut sec_i = 0;
    while k < tasks.len() {
        let name = tasks[k].0.clone();
        let n = tasks[k..].iter().take_while(|t| t.0 == name).count();
        let y = fr.y + top + axis_h + k as f64 * rh;
        let mut band = Node::shape("rect", Xfrm { x: fr.x, y, w: fr.w, h: rh * n as f64, ..Default::default() }, "", 1);
        band.fill = Paint::Solid(if sec_i % 2 == 0 { mix(&pal.accents[sec_i % 6], 0.9) } else { pal.lt1.clone() });
        out.push(band);
        if !name.is_empty() {
            let mut t = Node::shape("rect", Xfrm { x: fr.x, y, w: label_w * 0.45, h: rh * n as f64, ..Default::default() }, &name, 2);
            t.label = true; t.fill = Paint::None; t.text_color = mix(&pal.accents[sec_i % 6], -0.4); t.bold = true; t.align = "l";
            out.push(t);
        }
        k += n;
        sec_i += 1;
    }
    // The time axis: a tick per even step.
    let span = t1 - t0;
    let step = [1, 2, 7, 14, 30, 61, 91, 182, 365].into_iter().find(|s| span / s <= 8).unwrap_or(365);
    let mut d = t0;
    while d <= t1 {
        let x = x_of(d);
        let mut gl = line(x, fr.y + top + axis_h, x, fr.y + fr.h, "line");
        gl.line = Some((mix(&pal.dk2, 0.75), 0.5, None));
        out.push(gl);
        let mut t = Node::shape("rect", Xfrm { x: x - 500_000.0, y: fr.y + top, w: 1_000_000.0, h: axis_h, ..Default::default() }, &date(d)[5..], 3);
        t.label = true; t.fill = Paint::None; t.text_color = pal.dk2.clone(); t.size = 10.0;
        out.push(t);
        d += step;
    }
    for (k, (sec, name, s, e, done, active, crit, mile)) in tasks.iter().enumerate() {
        let y = fr.y + top + axis_h + k as f64 * rh;
        let si = tasks.iter().map(|t| t.0.clone()).fold(vec![], |mut v: Vec<String>, x| { if !v.contains(&x) { v.push(x); } v }).iter().position(|x| x == sec).unwrap_or(0);
        let mut t = Node::shape("rect", Xfrm { x: fr.x + label_w * 0.45, y, w: label_w * 0.55, h: rh, ..Default::default() }, name, 4);
        t.label = true; t.fill = Paint::None; t.text_color = pal.dk1.clone(); t.align = "l";
        out.push(t);
        let c = if *crit { "C0392B".to_string() } else if *done { "A6A6A6".to_string() } else { pal.accents[si % 6].clone() };
        let (x0, x1) = (x_of(*s), x_of(*e));
        let mut bar = if *mile {
            let d = rh * 0.6;
            Node::shape("diamond", Xfrm { x: x0 - d / 2.0, y: y + rh * 0.2, w: d, h: d, ..Default::default() }, "", 0)
        } else { Node::shape("roundRect", Xfrm { x: x0, y: y + rh * 0.2, w: (x1 - x0).max(30_000.0), h: rh * 0.6, ..Default::default() }, "", 0) };
        bar.fill = Paint::Gradient(mix(&c, 0.15), mix(&c, -0.1));
        bar.line = Some((mix(&c, -0.3), if *active { 2.0 } else { 0.75 }, None));
        bar.shadow = true;
        out.push(bar);
    }
    out
}

// ---- class and ER diagrams (records joined by relations) --------------------------------------

struct Rel { from: usize, to: usize, label: String, start: Option<&'static str>, end: Option<&'static str>, dashed: bool, card: (String, String) }

/// Records (title, lines) placed by the layered layout, joined by relations.
fn records_nodes(names: &[(String, Vec<String>)], rels: &[Rel], fr: &Xfrm, pal: &Palette, er: bool) -> Vec<Node> {
    let mut g = Graph { dir: "TB".into(), ..Default::default() };
    for (n, _) in names { g.node(n, None, None); }
    for r in rels { g.edges.push(Edge { from: r.from, to: r.to, label: String::new(), dotted: false, thick: false, arrow: false }); }
    let (at, widest, ranks) = place(&g);
    let (cw, rh) = (fr.w / widest as f64, fr.h / ranks as f64);
    let size = |i: usize| -> (f64, f64) {
        let lines = names[i].1.len() as f64;
        let longest = names[i].1.iter().chain(std::iter::once(&names[i].0)).map(|l| l.chars().count()).max().unwrap_or(4) as f64;
        ((longest * 85_000.0 + 250_000.0).min(cw * 0.82).max(cw * 0.4), ((lines + 1.4) * 230_000.0).min(rh * 0.78))
    };
    let center = |i: usize| (fr.x + at[i].0 * cw, fr.y + at[i].1 * rh);
    let mut out = vec![];
    let mut lanes = 0.0;
    let flags: Vec<(usize, usize, bool)> = rels.iter().map(|r| if at[r.to].1 >= at[r.from].1 { (r.from, r.to, at[r.to].1 > at[r.from].1) } else { (r.to, r.from, true) }).collect();
    let crossx = |i: usize| size(i).0;
    let orderx = |i: usize| at[i].0;
    let port = ports(&flags, &crossx, &orderx);
    for (k, r) in rels.iter().enumerate() {
        let forward = at[r.to].1 > at[r.from].1;
        let lane = if forward { 0.0 } else { lanes += 1.0; cw * 0.1 * lanes };
        let (a, b, sa, sb, f) = if forward || at[r.to].1 == at[r.from].1 { (center(r.from), center(r.to), size(r.from), size(r.to), forward) } else { (center(r.to), center(r.from), size(r.to), size(r.from), true) };
        let flipped = !(forward || at[r.to].1 == at[r.from].1);
        let pts = route(a, sa, b, sb, false, 1.0, f, lane, port[k]);
        let (s, e) = if flipped { (r.end, r.start) } else { (r.start, r.end) };
        out.extend(polyline(&pts, Some((mix(&pal.dk2, 0.15), 1.25, r.dashed.then(|| "dash".to_string()))), e, s));
        let m = pts.len() / 2;
        if !r.label.is_empty() {
            let (p1, p2) = (pts[m - 1], pts[m]);
            let w = (r.label.chars().count() as f64 * 70_000.0 + 140_000.0).min(cw * 0.9);
            let mut t = Node::shape("rect", Xfrm { x: (p1.0 + p2.0) / 2.0 - w / 2.0, y: (p1.1 + p2.1) / 2.0 - 120_000.0, w, h: 240_000.0, ..Default::default() }, &r.label, 20);
            t.fill = Paint::Solid(pal.lt1.clone()); t.text_color = pal.dk2.clone(); t.size = 10.0;
            out.push(t);
        }
        // Cardinalities beside each end.
        let (c0, c1) = if flipped { (&r.card.1, &r.card.0) } else { (&r.card.0, &r.card.1) };
        for (txt, p) in [(c0, pts[0]), (c1, *pts.last().unwrap())] {
            if txt.is_empty() { continue; }
            let mut t = Node::shape("rect", Xfrm { x: p.0 + 40_000.0, y: p.1 + if p == pts[0] { 20_000.0 } else { -240_000.0 }, w: 420_000.0, h: 220_000.0, ..Default::default() }, txt, 21);
            t.label = true; t.fill = Paint::None; t.text_color = pal.dk2.clone(); t.size = 9.0; t.align = "l";
            out.push(t);
        }
    }
    for (i, (title, lines)) in names.iter().enumerate() {
        let (cx, cy) = center(i);
        let (w, h) = size(i);
        let head_h = (h / (lines.len() as f64 + 1.4) * 1.4).min(h);
        let c = pal.accents[if er { 4 } else { 0 }].clone();
        let mut head = Node::shape("rect", Xfrm { x: cx - w / 2.0, y: cy - h / 2.0, w, h: head_h, ..Default::default() }, title, 0);
        head.fill = Paint::Gradient(mix(&c, 0.1), mix(&c, -0.15)); head.line = Some((mix(&c, -0.35), 1.0, None)); head.bold = true; head.shadow = true;
        out.push(head);
        if !lines.is_empty() {
            let mut body = Node::shape("rect", Xfrm { x: cx - w / 2.0, y: cy - h / 2.0 + head_h, w, h: h - head_h, ..Default::default() }, &lines.join("\n"), 1);
            body.fill = Paint::Solid(mix(&c, 0.9)); body.line = Some((mix(&c, -0.35), 1.0, None)); body.text_color = pal.dk1.clone(); body.align = "l"; body.anchor = "t";
            out.push(body);
        }
    }
    out
}

fn class_nodes(src: &str, fr: &Xfrm, pal: &Palette) -> Vec<Node> {
    let mut names: Vec<(String, Vec<String>)> = vec![];
    let mut rels = vec![];
    let idx = |n: &str, names: &mut Vec<(String, Vec<String>)>| -> usize {
        let n = n.trim().trim_matches('`');
        let n = n.split('~').next().unwrap_or(n).trim();
        names.iter().position(|x| x.0 == n).unwrap_or_else(|| { names.push((n.to_string(), vec![])); names.len() - 1 })
    };
    let mut open: Option<usize> = None;
    // Relations, longest arrows first: (text, head at left, head at right, dashed).
    let ops: &[(&str, Option<&'static str>, Option<&'static str>, bool)] = &[("<|--", Some("triangle"), None, false), ("--|>", None, Some("triangle"), false), ("<|..", Some("triangle"), None, true), ("..|>", None, Some("triangle"), true), ("*--", Some("diamond"), None, false), ("--*", None, Some("diamond"), false), ("o--", Some("diamond"), None, false), ("--o", None, Some("diamond"), false), ("<--", Some("arrow"), None, false), ("-->", None, Some("arrow"), false), ("<..", Some("arrow"), None, true), ("..>", None, Some("arrow"), true), ("..", None, None, true), ("--", None, None, false)];
    for l in src.lines().skip(1) {
        let l = l.trim();
        if l.is_empty() || l.starts_with("%%") || l.starts_with("direction") || l.starts_with("note") || l.starts_with("<<") { continue; }
        if let Some(i) = open { if l == "}" { open = None; } else { names[i].1.push(l.to_string()); } continue; }
        if let Some(t) = l.strip_prefix("class ") {
            let t = t.trim();
            let (name, body) = t.split_once('{').map(|(a, b)| (a, Some(b))).unwrap_or((t, None));
            let i = idx(name, &mut names);
            if let Some(b) = body { let b = b.trim(); if let Some(inline) = b.strip_suffix('}') { for m in inline.split(';').filter(|m| !m.trim().is_empty()) { names[i].1.push(m.trim().into()); } } else { open = Some(i); if !b.is_empty() { names[i].1.push(b.into()); } } }
            continue;
        }
        let (body, label) = l.split_once(" : ").map(|(a, b)| (a, b.trim().to_string())).unwrap_or((l, String::new()));
        if let Some((op, s, e, dashed)) = ops.iter().find(|o| body.contains(o.0)) {
            let (a, b) = body.split_once(op).unwrap();
            // Cardinalities in quotes beside the names: A "1" --> "*" B.
            let q = |t: &str| t.split('"').nth(1).map(String::from).unwrap_or_default();
            let strip = |t: &str| t.split('"').filter(|p| !p.trim().is_empty()).map(str::trim).filter(|p| !p.chars().all(|c| c.is_ascii_digit() || "*.n".contains(c))).last().unwrap_or("").to_string();
            let (ia, ib) = (idx(&strip(a), &mut names), idx(&strip(b), &mut names));
            rels.push(Rel { from: ia, to: ib, label, start: *s, end: *e, dashed: *dashed, card: (q(a), q(b)) });
            continue;
        }
        if let Some((n, m)) = l.split_once(':') { let i = idx(n, &mut names); names[i].1.push(m.trim().into()); }
    }
    records_nodes(&names, &rels, fr, pal, false)
}

fn er_nodes(src: &str, fr: &Xfrm, pal: &Palette) -> Vec<Node> {
    let mut names: Vec<(String, Vec<String>)> = vec![];
    let mut rels = vec![];
    let idx = |n: &str, names: &mut Vec<(String, Vec<String>)>| -> usize { let n = n.trim(); names.iter().position(|x| x.0 == n).unwrap_or_else(|| { names.push((n.to_string(), vec![])); names.len() - 1 }) };
    let card = |m: &str| -> String { match m { "||" => "1".into(), "|o" | "o|" => "0..1".into(), "}|" | "|{" => "1..*".into(), "}o" | "o{" => "0..*".into(), _ => String::new() } };
    let mut open: Option<usize> = None;
    for l in src.lines().skip(1) {
        let l = l.trim();
        if l.is_empty() || l.starts_with("%%") { continue; }
        if let Some(i) = open { if l == "}" { open = None; } else { names[i].1.push(l.split_whitespace().take(3).collect::<Vec<_>>().join(" ")); } continue; }
        if let Some(n) = l.strip_suffix('{') { open = Some(idx(n, &mut names)); continue; }
        // A ||--o{ B : label
        let (body, label) = l.split_once(':').map(|(a, b)| (a.trim(), b.trim().trim_matches('"').to_string())).unwrap_or((l, String::new()));
        let toks: Vec<&str> = body.split_whitespace().collect();
        if toks.len() == 3 && toks[1].len() >= 6 {
            let rel = toks[1];
            let (l2, r2) = (&rel[..2], &rel[rel.len() - 2..]);
            let dashed = rel.contains("..");
            let (a, b) = (idx(toks[0], &mut names), idx(toks[2], &mut names));
            rels.push(Rel { from: a, to: b, label, start: None, end: None, dashed, card: (card(l2), card(r2)) });
        } else if toks.len() == 1 { idx(toks[0], &mut names); }
    }
    records_nodes(&names, &rels, fr, pal, true)
}

/// The diagram's shapes in a frame, or an error to show.
pub fn nodes(src: &str, fr: &Xfrm, pal: &Palette) -> Result<Vec<Node>, String> {
    let first = src.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("%%")).unwrap_or("");
    let kind = first.split_whitespace().next().unwrap_or("");
    let body: String = src.lines().skip_while(|l| l.trim().is_empty() || l.trim().starts_with("%%")).collect::<Vec<_>>().join("\n");
    let out = match kind {
        "graph" | "flowchart" => flow_nodes(&parse_flowchart(&body, false), fr, pal),
        "stateDiagram" | "stateDiagram-v2" => flow_nodes(&parse_flowchart(&body, true), fr, pal),
        "sequenceDiagram" => sequence_nodes(&body, fr, pal),
        "gantt" => gantt_nodes(&body, fr, pal),
        "classDiagram" | "classDiagram-v2" => class_nodes(&body, fr, pal),
        "erDiagram" => er_nodes(&body, fr, pal),
        "pie" => pie_nodes(&body, fr, pal),
        "mindmap" => {
            let items = mindmap_items(&body);
            let mut n = crate::diagram::layout("horizontalHierarchy", &items, fr);
            crate::diagram::style(&mut n, "colorful", "moderate", pal);
            for x in n.iter_mut().filter(|x| x.preset == "roundRect" && x.role == 0) { x.preset = "ellipse".into(); }
            n
        }
        "timeline" => {
            let (title, items) = timeline_items(&body);
            let top = if title.is_empty() { 0.0 } else { fr.h * 0.14 };
            let inner = Xfrm { y: fr.y + top, h: fr.h - top, ..*fr };
            let mut n = crate::diagram::layout("basicTimeline", &items, &inner);
            crate::diagram::style(&mut n, "colorful", "subtle", pal);
            if !title.is_empty() { let mut t = Node::shape("rect", Xfrm { h: top, ..*fr }, &title, 7); t.fill = Paint::None; t.text_color = pal.dk1.clone(); t.bold = true; t.size = 20.0; n.push(t); }
            n
        }
        "" => return Err("Type a Mermaid diagram, for example: flowchart TD".into()),
        other => return Err(format!("“{other}” diagrams aren't supported yet. Try flowchart, sequenceDiagram, stateDiagram, classDiagram, erDiagram, gantt, pie, mindmap or timeline.")),
    };
    if out.is_empty() { return Err("The diagram is empty.".into()); }
    let mut out = out;
    crate::diagram::fit_text(&mut out);
    Ok(out)
}

impl Presentation {
    pub fn insert_mermaid(&mut self, part: &str, src: &str, fr: &Xfrm) -> Result<Loc, String> {
        let nodes = nodes(src, fr, &Palette::of(self, part))?;
        self.insert_nodes(part, &nodes, "Mermaid Diagram", &format!("sard:mermaid\n{src}")).ok_or_else(|| "Couldn't draw the diagram.".into())
    }
    /// The Mermaid source of a group made by `insert_mermaid`.
    pub fn mermaid(&self, loc: &Loc) -> Option<String> {
        self.element(loc)?.path("p:nvGrpSpPr/p:cNvPr")?.attr("descr")?.strip_prefix("sard:mermaid\n").map(String::from)
    }
    pub fn update_mermaid(&mut self, loc: &Loc, src: &str) -> Result<Loc, String> {
        nodes(src, &Xfrm { w: 1e6, h: 1e6, ..Default::default() }, &Palette::default())?;
        let part = loc.part.clone();
        let mut err = None;
        let g = self.replace_group(loc, |p, x| match p.insert_mermaid(&part, src, x) { Ok(l) => Some(l), Err(e) => { err = Some(e); None } });
        g.ok_or_else(|| err.unwrap_or_else(|| "Couldn't draw the diagram.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowchart_parses() {
        let g = parse_flowchart("flowchart LR\n A[Start] --> B{Ok?}\n B -->|Yes| C([End])\n B -. no .-> A\n D & E --> C\n style C fill:#ff0000", false);
        assert_eq!(g.dir, "LR");
        assert_eq!(g.nodes.iter().map(|n| (n.id.as_str(), n.shape.as_str())).collect::<Vec<_>>(), vec![("A", "rect"), ("B", "diamond"), ("C", "stadium"), ("D", "rect"), ("E", "rect")]);
        assert_eq!(g.edges.len(), 5);
        assert_eq!(g.edges[1].label, "Yes");
        assert!(g.edges[2].dotted && g.edges[2].label == "no");
        assert_eq!(g.nodes[2].fill.as_deref(), Some("ff0000"));
    }

    #[test]
    fn every_template_draws() {
        let mut p = Presentation::new_blank(true);
        let l = p.all_layouts().last().cloned().unwrap();
        let s = p.add_slide(&l, 0);
        let fr = Xfrm { x: 5e5, y: 5e5, w: 1.1e7, h: 5.8e6, ..Default::default() };
        for (name, src) in TEMPLATES {
            let g = p.insert_mermaid(&s, src, &fr).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(p.mermaid(&g).as_deref(), Some(*src));
            if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") {
                std::fs::write(format!("{dir}/mermaid-{}.png", name.replace(' ', "-")), crate::render::slide_png(&p, &s, 1100).unwrap()).unwrap();
            }
            p.delete_shapes(&[g]);
        }
        assert!(nodes("journey\n title x", &fr, &Palette::default()).is_err());
    }
}

/// Where each edge leaves and enters its boxes: edges sharing a side spread along it.
/// `edges`: (from, to, forward); `cross`: each box's extent across the flow.
fn ports(edges: &[(usize, usize, bool)], cross: &dyn Fn(usize) -> f64, order: &dyn Fn(usize) -> f64) -> Vec<(f64, f64)> {
    let mut out = vec![(0.0, 0.0); edges.len()];
    let n = edges.iter().map(|e| e.0.max(e.1)).max().map(|m| m + 1).unwrap_or(0);
    for node in 0..n {
        // Leaving (forward) and entering (forward), each sorted by where the other end is.
        for leaving in [true, false] {
            let mut ks: Vec<usize> = (0..edges.len()).filter(|&k| edges[k].2 && if leaving { edges[k].0 == node } else { edges[k].1 == node }).collect();
            ks.sort_by(|&a, &b| order(if leaving { edges[a].1 } else { edges[a].0 }).total_cmp(&order(if leaving { edges[b].1 } else { edges[b].0 })));
            let m = ks.len() as f64;
            for (j, &k) in ks.iter().enumerate() {
                let off = if m > 1.0 { (j as f64 - (m - 1.0) / 2.0) / m * cross(node) * 0.7 } else { 0.0 };
                if leaving { out[k].0 = off; } else { out[k].1 = off; }
            }
        }
    }
    out
}
