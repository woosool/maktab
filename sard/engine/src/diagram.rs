//! SmartArt graphics drawn as groups of native, editable shapes: 20 layouts (lists,
//! processes, cycles, hierarchies, relationships, matrices, pyramids), color schemes from the
//! theme's accents, styles from flat to 3-D, text sized to fit (the same size on peers), and
//! connectors. The text (one item per line, tabs for levels), layout, colors and style are
//! kept on the group so the graphic can be edited and rebuilt. Mermaid diagrams
//! (`crate::mermaid`) are drawn with the same pieces.

use crate::pres::Presentation;
use crate::scene::{Loc, Xfrm};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category { List, Process, Cycle, Hierarchy, Relationship, Matrix, Pyramid }

/// The layouts: (id, name, category).
pub const LAYOUTS: &[(&str, &str, Category)] = &[
    ("basicBlockList", "Basic Block List", Category::List),
    ("verticalBulletList", "Vertical Bullet List", Category::List),
    ("horizontalBulletList", "Horizontal Bullet List", Category::List),
    ("stackedList", "Stacked List", Category::List),
    ("basicProcess", "Basic Process", Category::Process),
    ("chevronProcess", "Basic Chevron Process", Category::Process),
    ("stepUpProcess", "Step Up Process", Category::Process),
    ("verticalProcess", "Vertical Process", Category::Process),
    ("basicTimeline", "Basic Timeline", Category::Process),
    ("basicCycle", "Basic Cycle", Category::Cycle),
    ("segmentedCycle", "Segmented Cycle", Category::Cycle),
    ("radialCycle", "Radial Cycle", Category::Cycle),
    ("orgChart", "Organization Chart", Category::Hierarchy),
    ("horizontalHierarchy", "Horizontal Hierarchy", Category::Hierarchy),
    ("basicVenn", "Basic Venn", Category::Relationship),
    ("basicRadial", "Basic Radial", Category::Relationship),
    ("basicTarget", "Basic Target", Category::Relationship),
    ("funnel", "Funnel", Category::Relationship),
    ("basicMatrix", "Basic Matrix", Category::Matrix),
    ("basicPyramid", "Basic Pyramid", Category::Pyramid),
    ("invertedPyramid", "Inverted Pyramid", Category::Pyramid),
];

pub const CATEGORIES: &[(Category, &str)] = &[(Category::List, "List"), (Category::Process, "Process"), (Category::Cycle, "Cycle"), (Category::Hierarchy, "Hierarchy"), (Category::Relationship, "Relationship"), (Category::Matrix, "Matrix"), (Category::Pyramid, "Pyramid")];

/// Color schemes: (id, name).
pub const COLORS: &[(&str, &str)] = &[
    ("colorful", "Colorful Range - Accent Colors"), ("accent1", "Colored Fill - Accent 1"), ("accent1grad", "Gradient Range - Accent 1"),
    ("accent2", "Colored Fill - Accent 2"), ("accent3", "Colored Fill - Accent 3"), ("accent4", "Colored Fill - Accent 4"),
    ("accent5", "Colored Fill - Accent 5"), ("accent6", "Colored Fill - Accent 6"), ("dark", "Dark 2 Fill"), ("outline", "Colored Outline - Accent 1"), ("transparent", "Transparent Gradient - Accent 1"),
];

/// Styles: (id, name).
pub const STYLES: &[(&str, &str)] = &[("flat", "Simple Fill"), ("subtle", "Subtle Effect"), ("moderate", "Moderate Effect"), ("intense", "Intense Effect"), ("polished", "Polished (3-D)"), ("outlineStyle", "White Outline")];

#[derive(Clone, Debug, PartialEq)]
pub struct Spec { pub layout: String, pub colors: String, pub style: String, pub items: Vec<(u8, String)>, pub rtl: bool }

impl Spec {
    pub fn new(layout: &str, text: &str) -> Spec { Spec { layout: layout.into(), colors: "colorful".into(), style: "subtle".into(), items: parse_items(text), rtl: false } }
    pub fn text(&self) -> String { self.items.iter().map(|(l, t)| format!("{}{}", "\t".repeat(*l as usize), t)).collect::<Vec<_>>().join("\n") }
    pub fn to_string(&self) -> String { format!("sard:smartart|{}|{}|{}|{}\n{}", self.layout, self.colors, self.style, self.rtl as u8, self.text()) }
    pub fn parse(s: &str) -> Option<Spec> {
        let rest = s.strip_prefix("sard:smartart|")?;
        let (head, body) = rest.split_once('\n').unwrap_or((rest, ""));
        let mut h = head.split('|');
        Some(Spec { layout: h.next()?.into(), colors: h.next().unwrap_or("colorful").into(), style: h.next().unwrap_or("subtle").into(), rtl: h.next() == Some("1"), items: parse_items(body) })
    }
}

/// Lines to (level, text): leading tabs (or each two spaces, or "-"/"*" bullets) set the level.
pub fn parse_items(text: &str) -> Vec<(u8, String)> {
    text.lines().filter(|l| !l.trim().is_empty()).map(|l| {
        let tabs = l.chars().take_while(|c| *c == '\t').count();
        let spaces = l.chars().take_while(|c| *c == ' ').count() / 2;
        let t = l.trim().trim_start_matches(['-', '*', '•']).trim();
        ((tabs + spaces).min(4) as u8, t.to_string())
    }).collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint { None, Solid(String), Gradient(String, String) }

/// One shape of a drawn diagram.
#[derive(Clone, Debug)]
pub struct Node {
    pub preset: String,
    pub x: Xfrm,
    pub text: String,
    pub fill: Paint,
    /// Outline: color, width (pt), dash.
    pub line: Option<(String, f64, Option<String>)>,
    pub text_color: String,
    pub size: f64,
    pub bold: bool,
    pub align: &'static str,
    pub anchor: &'static str,
    /// Arrow at the line's end (and start).
    pub arrows: (Option<&'static str>, Option<&'static str>),
    pub adjust: Vec<(&'static str, f64)>,
    pub shadow: bool,
    pub bevel: bool,
    /// Peers share a text size: the role groups them.
    pub role: u8,
    /// Text on no shape (labels beside or over other shapes).
    pub label: bool,
    /// A label over a filled shape (light text).
    pub on_fill: bool,
    /// The item (index in the text) this shape shows.
    pub item: Option<usize>,
}

impl Node {
    pub fn shape(preset: &str, x: Xfrm, text: &str, role: u8) -> Node {
        Node { preset: preset.into(), x, text: text.into(), fill: Paint::None, line: None, text_color: "FFFFFF".into(), size: 0.0, bold: false, align: "ctr", anchor: "ctr", arrows: (None, None), adjust: vec![], shadow: false, bevel: false, role, label: false, on_fill: false, item: None }
    }
    pub fn is_line(&self) -> bool { self.preset.contains("Connector") || self.preset == "line" }
}

/// A line from (x1, y1) to (x2, y2): its frame and flips.
pub fn line(x1: f64, y1: f64, x2: f64, y2: f64, preset: &str) -> Node {
    let x = Xfrm { x: x1.min(x2), y: y1.min(y2), w: (x2 - x1).abs().max(1.0), h: (y2 - y1).abs().max(1.0), flip_h: x2 < x1, flip_v: y2 < y1, ..Default::default() };
    let mut n = Node::shape(preset, x, "", 99);
    n.fill = Paint::None;
    n
}

fn f(x: f64, y: f64, w: f64, h: f64) -> Xfrm { Xfrm { x, y, w, h, ..Default::default() } }

/// Parent of each item (by index), from the levels.
fn parents(items: &[(u8, String)]) -> Vec<Option<usize>> {
    let mut stack: Vec<usize> = vec![];
    items.iter().enumerate().map(|(i, (l, _))| {
        while let Some(&top) = stack.last() { if items[top].0 >= *l { stack.pop(); } else { break; } }
        let p = stack.last().copied();
        stack.push(i);
        p
    }).collect()
}

/// Top-level items and their sub-items' text.
fn tops(items: &[(u8, String)]) -> Vec<(String, Vec<String>)> {
    let mut v: Vec<(String, Vec<String>)> = vec![];
    for (l, t) in items { if *l == 0 || v.is_empty() { v.push((t.clone(), vec![])); } else { v.last_mut().unwrap().1.push(t.clone()); } }
    v
}

/// The shapes of a layout in a frame (before colors and styles).
pub fn layout(id: &str, items: &[(u8, String)], fr: &Xfrm) -> Vec<Node> {
    let mut out = layout_raw(id, items, fr);
    // Text on flipped shapes would come out upside down: it moves to a label on top.
    let mut labels = vec![];
    for n in out.iter_mut().filter(|n| n.x.flip_v && !n.text.is_empty() && !n.is_line()) {
        let mut l = Node::shape("rect", Xfrm { flip_v: false, flip_h: false, ..n.x }, &n.text, n.role);
        l.label = true;
        l.bold = n.bold;
        l.on_fill = true;
        n.text.clear();
        labels.push(l);
    }
    out.extend(labels);
    tag_items(&mut out, items);
    out
}

/// Which item each shape shows: by its text (a top item's box may hold its sub-items too;
/// those point at their own item when it has no box of its own).
fn tag_items(nodes: &mut [Node], items: &[(u8, String)]) {
    let par = parents(items);
    let mut owner: Vec<Option<usize>> = vec![None; items.len()];
    for (i, (_, t)) in items.iter().enumerate() {
        let exact = nodes.iter().position(|n| n.item.is_none() && !n.is_line() && n.text == *t && !t.is_empty());
        let start = || nodes.iter().position(|n| n.item.is_none() && !n.is_line() && !t.is_empty() && n.text.starts_with(&format!("{t}\n")));
        if let Some(k) = exact.or_else(start) { nodes[k].item = Some(i); owner[i] = Some(k); }
    }
    let _ = (par, owner);
}

/// The item a click on a shape of the graphic means (its own, or the box holding it).
pub fn item_at(spec: &Spec, shape_item: Option<usize>) -> Option<usize> { shape_item.filter(|&k| k < spec.items.len()) }

/// The end (exclusive) of item k's subtree.
fn subtree_end(items: &[(u8, String)], k: usize) -> usize {
    let l = items[k].0;
    (k + 1..items.len()).find(|&j| items[j].0 <= l).unwrap_or(items.len())
}

/// Edits on one item of a graphic (the Design tab); returns the item to select after.
pub fn edit_item(items: &mut Vec<(u8, String)>, k: usize, op: &str, text: &str) -> Option<usize> {
    if items.is_empty() { items.push((0, text.into())); return Some(0); }
    let k = k.min(items.len() - 1);
    let l = items[k].0;
    let end = subtree_end(items, k);
    match op {
        "after" => { items.insert(end, (l, text.into())); Some(end) }
        "before" => { items.insert(k, (l, text.into())); Some(k) }
        "above" => { for it in &mut items[k..end] { it.0 = (it.0 + 1).min(4); } items.insert(k, (l, text.into())); Some(k) }
        "below" => { items.insert(end, ((l + 1).min(4), text.into())); Some(end) }
        "promote" => { if l == 0 { return Some(k); } for it in &mut items[k..end] { it.0 -= 1; } Some(k) }
        "demote" => {
            // Only under an earlier sibling.
            if k == 0 || items[k - 1].0 < l || l >= 4 { return Some(k); }
            for it in &mut items[k..end] { it.0 = (it.0 + 1).min(4); }
            Some(k)
        }
        "up" => {
            let prev = (0..k).rev().find(|&j| items[j].0 <= l).filter(|&j| items[j].0 == l)?;
            let block: Vec<_> = items.drain(k..end).collect();
            for (o, it) in block.into_iter().enumerate() { items.insert(prev + o, it); }
            Some(prev)
        }
        "down" => {
            if end >= items.len() || items[end].0 != l { return Some(k); }
            let next_end = subtree_end(items, end);
            let block: Vec<_> = items.drain(end..next_end).collect();
            let n = block.len();
            for (o, it) in block.into_iter().enumerate() { items.insert(k + o, it); }
            Some(k + n)
        }
        "remove" => {
            if items.len() == 1 { return Some(0); }
            items.remove(k);
            // Its children move up a level.
            let e = subtree_end_from(items, k, l);
            for it in &mut items[k..e] { it.0 = it.0.saturating_sub(1); }
            Some(k.min(items.len() - 1))
        }
        _ => Some(k),
    }
}

fn subtree_end_from(items: &[(u8, String)], k: usize, l: u8) -> usize { (k..items.len()).find(|&j| items[j].0 <= l).unwrap_or(items.len()) }

fn layout_raw(id: &str, items: &[(u8, String)], fr: &Xfrm) -> Vec<Node> {
    let t = tops(items);
    let n = t.len().max(1);
    let nf = n as f64;
    let (x0, y0, w, h) = (fr.x, fr.y, fr.w, fr.h);
    let mut out = vec![];
    let joined = |(a, subs): &(String, Vec<String>)| if subs.is_empty() { a.clone() } else { format!("{a}\n{}", subs.iter().map(|s| format!("• {s}")).collect::<Vec<_>>().join("\n")) };
    match id {
        "verticalBulletList" => {
            let rows: f64 = t.iter().map(|(_, s)| 1.0 + s.len() as f64 * 0.75).sum();
            let unit = h / rows.max(1.0);
            let mut y = y0;
            for (k, (head, subs)) in t.iter().enumerate() {
                let hh = unit * 0.85;
                let mut b = Node::shape("roundRect", f(x0, y, w, hh), head, 0);
                b.align = "l";
                b.adjust = vec![("adj", 12_000.0)];
                b.role = 0;
                let _ = k;
                out.push(b);
                y += unit;
                if !subs.is_empty() {
                    let sh = unit * 0.75 * subs.len() as f64;
                    let mut s = Node::shape("rect", f(x0 + w * 0.04, y - unit * 0.1, w * 0.96, sh), &subs.iter().map(|s| format!("• {s}")).collect::<Vec<_>>().join("\n"), 1);
                    s.align = "l";
                    s.anchor = "t";
                    out.push(s);
                    y += sh;
                }
            }
        }
        "horizontalBulletList" | "stackedList" => {
            let gap = w * 0.03;
            let cw = (w - gap * (nf - 1.0)) / nf;
            for (k, (head, subs)) in t.iter().enumerate() {
                let x = x0 + k as f64 * (cw + gap);
                if id == "stackedList" {
                    let d = cw.min(h * 0.32);
                    let mut c = Node::shape("ellipse", f(x + (cw - d) / 2.0, y0, d, d), head, 0);
                    c.bold = true;
                    out.push(c);
                    let mut body = Node::shape("roundRect", f(x, y0 + d * 0.75, cw, h - d * 0.75), &subs.join("\n"), 1);
                    body.anchor = "b";
                    body.adjust = vec![("adj", 8_000.0)];
                    out.insert(0, body);
                } else {
                    let mut hd = Node::shape("rect", f(x, y0, cw, h * 0.22), head, 0);
                    hd.bold = true;
                    out.push(hd);
                    let mut body = Node::shape("rect", f(x, y0 + h * 0.22, cw, h * 0.78), &subs.iter().map(|s| format!("• {s}")).collect::<Vec<_>>().join("\n"), 1);
                    body.align = "l";
                    body.anchor = "t";
                    out.push(body);
                }
            }
        }
        "basicProcess" | "chevronProcess" | "stepUpProcess" => {
            let chevron = id == "chevronProcess";
            let step = id == "stepUpProcess";
            let gap = if chevron { -w * 0.02 } else { w * 0.12 / nf.max(1.0) * 2.0 };
            let bw = (w - gap * (nf - 1.0)) / nf;
            for (k, it) in t.iter().enumerate() {
                let x = x0 + k as f64 * (bw + gap);
                let (bh, y) = if step { let hh = h * (0.35 + 0.65 * (k as f64 + 1.0) / nf); (hh * 0.5, y0 + h - hh) } else { (h * 0.5, y0 + h * 0.25) };
                let mut b = Node::shape(if chevron { if k == 0 { "homePlate" } else { "chevron" } } else { "roundRect" }, f(x, y, bw, if step { bh.max(h * 0.25) } else { bh }), &joined(it), 0);
                if !chevron { b.adjust = vec![("adj", 10_000.0)]; }
                if chevron { b.adjust = vec![("adj", 30_000.0)]; }
                out.push(b);
                if !chevron && k + 1 < t.len() {
                    let aw = gap * 0.6;
                    let mut a = Node::shape(if step { "rightArrow" } else { "triangle" }, f(x + bw + gap * 0.2, if step { y + bh * 0.3 } else { y0 + h * 0.5 - aw * 0.55 }, aw, aw * 1.1), "", 50);
                    if !step { a.x.rot = 90.0; }
                    out.push(a);
                }
            }
        }
        "verticalProcess" => {
            let gap = h * 0.12 / nf.max(1.0) * 2.0;
            let bh = (h - gap * (nf - 1.0)) / nf;
            for (k, it) in t.iter().enumerate() {
                let y = y0 + k as f64 * (bh + gap);
                let mut b = Node::shape("roundRect", f(x0 + w * 0.15, y, w * 0.7, bh), &joined(it), 0);
                b.adjust = vec![("adj", 12_000.0)];
                out.push(b);
                if k + 1 < t.len() { let aw = gap * 0.7; out.push(Node::shape("downArrow", f(x0 + w / 2.0 - aw * 0.6, y + bh + gap * 0.15, aw * 1.2, aw), "", 50)); }
            }
        }
        "basicTimeline" => {
            let ly = y0 + h * 0.5;
            let mut l = line(x0, ly, x0 + w, ly, "straightConnector1");
            l.arrows = (None, Some("triangle"));
            out.push(l);
            let cw = w / nf;
            for (k, (head, subs)) in t.iter().enumerate() {
                let cx = x0 + cw * (k as f64 + 0.5);
                let d = (h * 0.12).min(cw * 0.3);
                out.push(Node::shape("ellipse", f(cx - d / 2.0, ly - d / 2.0, d, d), "", 60));
                let above = k % 2 == 0;
                let bh = h * 0.36;
                let mut b = Node::shape("roundRect", f(cx - cw * 0.46, if above { y0 } else { ly + h * 0.14 }, cw * 0.92, bh), &if subs.is_empty() { head.clone() } else { format!("{head}\n{}", subs.join("\n")) }, 0);
                b.adjust = vec![("adj", 10_000.0)];
                out.push(b);
                let (a, z) = if above { (y0 + bh, ly - d / 2.0) } else { (ly + d / 2.0, ly + h * 0.14) };
                out.insert(0, line(cx, a, cx, z, "line"));
            }
        }
        "basicCycle" | "radialCycle" | "basicRadial" => {
            let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
            let m = w.min(h);
            let center = id != "basicCycle";
            let ring: Vec<&(String, Vec<String>)> = if center { t.iter().skip(1).collect() } else { t.iter().collect() };
            let rn = ring.len().max(1) as f64;
            let s = (m * if center { 0.24 } else { 0.28 }).min(m * 2.6 / rn.max(3.0));
            let r = m / 2.0 - s / 2.0;
            if center {
                let cs = m * 0.32;
                let mut c = Node::shape("ellipse", f(cx - cs / 2.0, cy - cs / 2.0, cs, cs), &t.first().map(|x| x.0.clone()).unwrap_or_default(), 0);
                c.bold = true;
                out.push(c);
            }
            for (k, it) in ring.iter().enumerate() {
                let a = -std::f64::consts::FRAC_PI_2 + k as f64 * std::f64::consts::TAU / rn;
                let (px, py) = (cx + r * a.cos(), cy + r * a.sin());
                if center { out.insert(0, line(cx, cy, px, py, "line")); }
                out.push(Node::shape("ellipse", f(px - s / 2.0, py - s / 2.0, s, s), &it.0, 1));
                if !center {
                    // A curved arrow between this item and the next.
                    let b = a + std::f64::consts::PI / rn;
                    let aw = s * 0.32;
                    let mut ar = Node::shape("rightArrow", f(cx + r * b.cos() - aw / 2.0, cy + r * b.sin() - aw * 0.3, aw, aw * 0.6), "", 50);
                    ar.x.rot = (b.to_degrees() + 90.0).rem_euclid(360.0);
                    out.push(ar);
                }
            }
        }
        "segmentedCycle" => {
            let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
            let m = w.min(h) * 0.96;
            for (k, it) in t.iter().enumerate() {
                let a0 = -90.0 + k as f64 * 360.0 / nf;
                let a1 = a0 + 360.0 / nf;
                let mut p = Node::shape("pie", f(cx - m / 2.0, cy - m / 2.0, m, m), "", 0);
                p.adjust = vec![("adj1", (a0.rem_euclid(360.0)) * 60_000.0), ("adj2", (a1.rem_euclid(360.0)) * 60_000.0)];
                p.text = String::new();
                out.push(p);
                let mid = ((a0 + a1) / 2.0).to_radians();
                let (tw, th) = (m * 0.3, m * 0.16);
                let mut lab = Node::shape("rect", f(cx + m * 0.3 * mid.cos() - tw / 2.0, cy + m * 0.3 * mid.sin() - th / 2.0, tw, th), &it.0, 1);
                lab.label = true;
                out.push(lab);
            }
        }
        "orgChart" | "horizontalHierarchy" => {
            // A tidy tree: leaves get the slots, parents sit over their children.
            let par = parents(items);
            let kids: Vec<Vec<usize>> = (0..items.len()).map(|i| (0..items.len()).filter(|&j| par[j] == Some(i)).collect()).collect();
            let depth: Vec<usize> = (0..items.len()).map(|i| { let mut d = 0; let mut c = par[i]; while let Some(p) = c { d += 1; c = par[p]; } d }).collect();
            let levels = depth.iter().max().map(|d| d + 1).unwrap_or(1) as f64;
            let mut slot = vec![0.0f64; items.len()];
            let mut next = 0.0;
            fn place(i: usize, kids: &[Vec<usize>], slot: &mut [f64], next: &mut f64) {
                if kids[i].is_empty() { slot[i] = *next; *next += 1.0; return; }
                for &k in &kids[i] { place(k, kids, slot, next); }
                slot[i] = (slot[kids[i][0]] + slot[*kids[i].last().unwrap()]) / 2.0;
            }
            for i in (0..items.len()).filter(|&i| par[i].is_none()) { place(i, &kids, &mut slot, &mut next); }
            let slots = next.max(1.0);
            let horiz = id == "horizontalHierarchy";
            let (along, across) = if horiz { (w, h) } else { (h, w) };
            let cell = across / slots;
            let lane = along / levels;
            let (bw, bh) = if horiz { (lane * 0.72, (cell * 0.8).min(lane * 0.5)) } else { ((cell * 0.86).min(lane * 2.2), lane * 0.6) };
            let pos = |i: usize| -> (f64, f64) {
                let a = (depth[i] as f64 + 0.5) * lane;
                let c = (slot[i] + 0.5) * cell;
                if horiz { (x0 + a, y0 + c) } else { (x0 + c, y0 + a) }
            };
            for i in 0..items.len() {
                let (cx, cy) = pos(i);
                if let Some(p) = par[i] {
                    let (px, py) = pos(p);
                    // Elbow: down (or across) from the parent, then to the child.
                    if horiz {
                        let mid = px + bw / 2.0 + (cx - bw / 2.0 - px - bw / 2.0) / 2.0;
                        out.insert(0, line(px + bw / 2.0, py, mid, py, "line"));
                        out.insert(0, line(mid, py, mid, cy, "line"));
                        out.insert(0, line(mid, cy, cx - bw / 2.0, cy, "line"));
                    } else {
                        let mid = py + bh / 2.0 + (cy - bh / 2.0 - py - bh / 2.0) / 2.0;
                        out.insert(0, line(px, py + bh / 2.0, px, mid, "line"));
                        out.insert(0, line(px, mid, cx, mid, "line"));
                        out.insert(0, line(cx, mid, cx, cy - bh / 2.0, "line"));
                    }
                }
                let mut b = Node::shape("roundRect", f(cx - bw / 2.0, cy - bh / 2.0, bw, bh), &items[i].1, depth[i].min(2) as u8);
                b.adjust = vec![("adj", 10_000.0)];
                if depth[i] == 0 { b.bold = true; }
                out.push(b);
            }
        }
        "basicVenn" => {
            let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
            let m = w.min(h);
            let d = if n == 1 { m * 0.8 } else { m * 0.58 };
            let r = if n == 1 { 0.0 } else { m * 0.21 };
            for (k, it) in t.iter().enumerate() {
                let a = -std::f64::consts::FRAC_PI_2 + k as f64 * std::f64::consts::TAU / nf;
                let (px, py) = (cx + r * a.cos(), cy + r * a.sin());
                let mut c = Node::shape("ellipse", f(px - d / 2.0, py - d / 2.0, d, d), "", 0);
                c.role = 0;
                out.push(c);
                let (tw, th) = (d * 0.6, d * 0.3);
                let mut lab = Node::shape("rect", f(cx + r * 1.55 * a.cos() - tw / 2.0, cy + r * 1.55 * a.sin() - th / 2.0, tw, th), &it.0, 1);
                lab.label = true;
                lab.bold = true;
                out.push(lab);
            }
        }
        "basicTarget" => {
            let m = w.min(h * 1.6);
            let d0 = h.min(m * 0.62);
            let (cx, cy) = (x0 + d0 / 2.0 + w * 0.02, y0 + h / 2.0);
            for (k, _) in t.iter().enumerate() {
                let d = d0 * (1.0 - k as f64 / nf);
                out.push(Node::shape("ellipse", f(cx - d / 2.0, cy - d / 2.0, d, d), "", 0));
            }
            for (k, it) in t.iter().enumerate() {
                let y = y0 + h * (k as f64 + 0.5) / nf;
                let lx = x0 + d0 + w * 0.08;
                out.push(line(cx + d0 * (0.5 - 0.5 * k as f64 / nf) * 0.7, cy - d0 * 0.5 * (1.0 - k as f64 / nf) * 0.7 + (y - cy) * 0.3, lx, y, "line"));
                let mut lab = Node::shape("rect", f(lx, y - h / nf * 0.4, x0 + w - lx, h / nf * 0.8), &joined(it), 1);
                lab.label = true;
                lab.align = "l";
                out.push(lab);
            }
        }
        "funnel" => {
            let rh = h / nf;
            for (k, it) in t.iter().enumerate() {
                let top = w * (1.0 - 0.7 * k as f64 / nf);
                let mut s = Node::shape("trapezoid", f(x0 + (w - top) / 2.0, y0 + k as f64 * rh, top, rh * 0.94), &it.0, 0);
                s.x.flip_v = true;
                s.adjust = vec![("adj", 35_000.0 / nf.max(1.0) * 2.0)];
                out.push(s);
            }
        }
        "basicMatrix" => {
            let side = w.min(h);
            let (sx, sy) = (x0 + (w - side) / 2.0, y0 + (h - side) / 2.0);
            let q = side / 2.0;
            for (k, it) in t.iter().take(4).enumerate() {
                let (c, r) = ((k % 2) as f64, (k / 2) as f64);
                let mut b = Node::shape("round2SameRect", f(sx + c * q + q * 0.03, sy + r * q + q * 0.03, q * 0.94, q * 0.94), &joined(it), 0);
                b.adjust = vec![("adj1", 12_000.0), ("adj2", 0.0)];
                b.x.rot = [270.0, 0.0, 180.0, 90.0][k];
                b.x.rot = 0.0;
                out.push(b);
            }
        }
        "basicPyramid" | "invertedPyramid" => {
            let inv = id == "invertedPyramid";
            let pw = w.min(h * 1.3);
            let px = x0 + (w - pw) / 2.0;
            let rh = h / nf;
            for (k, it) in t.iter().enumerate() {
                let i = if inv { n - 1 - k } else { k };
                let wt = pw * (i as f64) / nf;
                let wb = pw * (i as f64 + 1.0) / nf;
                let mut s = Node::shape(if i == 0 { "triangle" } else { "trapezoid" }, f(px + (pw - wb) / 2.0, y0 + k as f64 * rh, wb, rh * 0.96), &it.0, 0);
                if i > 0 { s.adjust = vec![("adj", (wb - wt) / 2.0 / (rh * 0.96).max(1.0) * 100_000.0 * (rh * 0.96) / wb.max(1.0))]; }
                if inv { s.x.flip_v = true; }
                out.push(s);
            }
        }
        _ => {
            // Basic Block List.
            let cols = nf.sqrt().ceil().max(1.0).min(4.0).max(if nf > 2.0 { 3.0f64.min(nf) } else { nf });
            let rows = (nf / cols).ceil();
            let (gw, gh) = (w / cols, h / rows);
            for (k, it) in t.iter().enumerate() {
                let (c, r) = ((k as f64 % cols), (k as f64 / cols).floor());
                // The last row is centered.
                let in_row = if r == rows - 1.0 { nf - r * cols } else { cols };
                let off = (cols - in_row) * gw / 2.0;
                out.push(Node::shape("rect", f(x0 + off + c * gw + gw * 0.04, y0 + r * gh + gh * 0.05, gw * 0.92, gh * 0.9), &joined(it), 0));
            }
        }
    }
    out
}

/// The theme's colors used: accent1..6, dk1, lt1, dk2.
#[derive(Clone, Debug)]
pub struct Palette { pub accents: Vec<String>, pub dk1: String, pub lt1: String, pub dk2: String }

impl Default for Palette {
    fn default() -> Self { Palette { accents: ["4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5", "70AD47"].map(String::from).to_vec(), dk1: "000000".into(), lt1: "FFFFFF".into(), dk2: "44546A".into() } }
}

impl Palette {
    pub fn of(pres: &Presentation, part: &str) -> Palette {
        let (theme, _) = crate::scene::color_context(pres, part);
        let get = |k: &str, d: &str| theme.scheme.get(k).map(|c| c.to_hex()).unwrap_or_else(|| d.into());
        let d = Palette::default();
        Palette { accents: (1..=6).map(|i| get(&format!("accent{i}"), &d.accents[i - 1])).collect(), dk1: get("dk1", "000000"), lt1: get("lt1", "FFFFFF"), dk2: get("dk2", "44546A") }
    }
}

fn rgb(h: &str) -> (f64, f64, f64) { let v = u32::from_str_radix(h.trim_start_matches('#'), 16).unwrap_or(0x4472C4); ((v >> 16 & 255) as f64, (v >> 8 & 255) as f64, (v & 255) as f64) }
fn hex(r: f64, g: f64, b: f64) -> String { format!("{:02X}{:02X}{:02X}", r.clamp(0.0, 255.0) as u8, g.clamp(0.0, 255.0) as u8, b.clamp(0.0, 255.0) as u8) }
/// Toward white (k > 0) or black (k < 0).
pub fn mix(h: &str, k: f64) -> String { let (r, g, b) = rgb(h); if k >= 0.0 { hex(r + (255.0 - r) * k, g + (255.0 - g) * k, b + (255.0 - b) * k) } else { hex(r * (1.0 + k), g * (1.0 + k), b * (1.0 + k)) } }
fn light(h: &str) -> bool { let (r, g, b) = rgb(h); 0.299 * r + 0.587 * g + 0.114 * b > 165.0 }

/// Colors, styles and text sizes on a layout's shapes.
pub fn style(nodes: &mut [Node], colors: &str, style: &str, pal: &Palette) {
    let count = nodes.iter().filter(|n| !n.is_line() && n.role == 0).count().max(1);
    let base = match colors { c if c.starts_with("accent") => { let k = c.trim_start_matches("accent").chars().next().and_then(|d| d.to_digit(10)).unwrap_or(1) as usize; pal.accents[(k - 1).min(5)].clone() } "dark" => pal.dk2.clone(), _ => pal.accents[0].clone() };
    let mut role0 = 0usize;
    for n in nodes.iter_mut() {
        if n.is_line() {
            n.line = Some((if colors == "dark" { mix(&pal.dk2, 0.3) } else { mix(&base, -0.15) }, 1.5, None));
            continue;
        }
        if n.label {
            n.fill = Paint::None;
            n.text_color = if n.on_fill && colors != "outline" { "FFFFFF".into() } else if colors == "dark" { pal.dk2.clone() } else { pal.dk1.clone() };
            n.line = None;
            continue;
        }
        // The fill by scheme: each top item its own accent (colorful) or a shade of one.
        let k = if n.role == 0 { role0 += 1; role0 - 1 } else { 0 };
        let c = match colors {
            "colorful" => if n.role == 0 { pal.accents[k % 6].clone() } else { mix(&pal.accents[0], 0.55) },
            "accent1grad" | "transparent" => if n.role == 0 { mix(&base, -0.25 + 0.6 * k as f64 / count.max(2) as f64) } else { mix(&base, 0.6) },
            _ => if n.role == 0 { base.clone() } else if n.role >= 50 { mix(&base, 0.35) } else { mix(&base, 0.4 + 0.15 * (n.role as f64 - 1.0).max(0.0)) },
        };
        let c = if n.role == 60 { base.clone() } else { c };
        match colors {
            "outline" => { n.fill = Paint::Solid(pal.lt1.clone()); n.line = Some((base.clone(), 1.75, None)); n.text_color = mix(&base, -0.3); }
            "transparent" => { n.fill = Paint::Solid(mix(&c, 0.45)); n.line = Some((c.clone(), 1.0, None)); n.text_color = mix(&c, -0.55); }
            _ => {
                n.fill = Paint::Solid(c.clone());
                n.line = Some((pal.lt1.clone(), 1.0, None));
                n.text_color = if light(&c) { mix(&c, -0.75) } else { "FFFFFF".into() };
            }
        }
        // Venn circles let the overlaps show.
        if n.preset == "ellipse" && n.text.is_empty() && n.role == 0 && colors != "outline" { n.fill = Paint::Solid(mix(&c, 0.25)); }
        match style {
            "subtle" => n.shadow = true,
            "moderate" => { if let Paint::Solid(s) = &n.fill { n.fill = Paint::Gradient(mix(s, 0.25), mix(s, -0.12)); } n.shadow = true; }
            "intense" => { if let Paint::Solid(s) = &n.fill { n.fill = Paint::Gradient(mix(s, 0.35), mix(s, -0.25)); } n.shadow = true; n.line = None; }
            "polished" => { n.bevel = true; n.line = None; }
            "outlineStyle" => { n.line = Some((pal.lt1.clone(), 3.0, None)); n.shadow = true; }
            _ => {}
        }
        if n.role >= 50 { n.shadow = false; n.bevel = false; }
    }
    fit_text(nodes);
}

/// Text sizes: the largest that fits every box of a role (peers match), 8–36 pt.
pub fn fit_text(nodes: &mut [Node]) {
    let mut sizes: std::collections::HashMap<u8, f64> = Default::default();
    for n in nodes.iter() {
        if n.text.is_empty() || n.size > 0.0 { continue; }
        let (w, h) = (n.x.w / 12_700.0, n.x.h / 12_700.0);
        let inset = match n.preset.as_str() { "ellipse" | "triangle" | "trapezoid" | "diamond" | "chevron" | "homePlate" | "hexagon" => 0.68, _ => 0.86 };
        let (w, h) = (w * inset - 10.0, h * inset - 6.0);
        let lines: Vec<&str> = n.text.lines().collect();
        let longest_word = n.text.split_whitespace().map(|w| w.chars().count()).max().unwrap_or(1) as f64;
        // Size s: longest word fits the width, and the wrapped lines fit the height.
        let mut s: f64 = 36.0;
        while s > 8.0 {
            let cw = s * 0.52;
            let per_line = (w / cw).floor().max(1.0);
            let rows: f64 = lines.iter().map(|l| (l.chars().count() as f64 / per_line).ceil().max(1.0)).sum();
            if longest_word * cw <= w && rows * s * 1.2 <= h { break; }
            s -= 1.0;
        }
        let e = sizes.entry(n.role).or_insert(36.0);
        *e = e.min(s);
    }
    for n in nodes.iter_mut() { if n.size == 0.0 { n.size = sizes.get(&n.role).copied().unwrap_or(14.0); } }
}

/// Mirror a drawing for right-to-left reading.
pub fn mirror(nodes: &mut [Node], fr: &Xfrm) {
    for n in nodes.iter_mut() {
        n.x.x = fr.x + fr.w - (n.x.x - fr.x) - n.x.w;
        if n.is_line() { n.x.flip_h = !n.x.flip_h; }
        if matches!(n.preset.as_str(), "homePlate" | "chevron" | "rightArrow") { n.x.flip_h = !n.x.flip_h; }
        if n.align == "l" { n.align = "r"; }
    }
}

impl Presentation {
    /// Draw nodes as shapes on a part and group them; returns the group.
    pub fn insert_nodes(&mut self, part: &str, nodes: &[Node], name: &str, descr: &str) -> Option<Loc> {
        let mut locs = vec![];
        let mut frames = vec![];
        for n in nodes {
            let loc = self.insert_shape(part, &n.preset, &n.x, n.fill == Paint::None && !n.is_line() && !n.text.is_empty())?;
            for (k, v) in &n.adjust { self.set_adjust(&loc, k, *v); }
            match &n.fill {
                Paint::None => self.set_fill(&loc, Some("")),
                Paint::Solid(c) => self.set_fill(&loc, Some(c)),
                Paint::Gradient(a, b) => self.set_gradient_fill(&loc, &[(0.0, a.clone()), (1.0, b.clone())], 90.0, false),
            }
            match &n.line { Some((c, w, d)) => self.set_line(&loc, Some(c), Some(*w), d.as_deref()), None => self.set_line(&loc, Some(""), None, None) }
            if n.arrows != (None, None) { self.set_arrows(&loc, n.arrows.0, n.arrows.1); }
            if n.shadow { self.set_effect(&loc, &crate::format::Effect::Shadow { dir: 90.0, dist: 2.0, blur: 5.0, color: "000000".into(), alpha: 30 }); }
            if n.bevel { self.set_three_d(&loc, &crate::three_d::ThreeD { top: Some(crate::three_d::Bevel { kind: "circle".into(), w: 76_200.0, h: 76_200.0 }), material: "plastic".into(), ..Default::default() }); }
            if !n.text.is_empty() {
                self.set_shape_text(&loc, None, &n.text);
                self.format_shape_text(&loc, None, &crate::edit::RunFormat { color: Some(n.text_color.clone()), size: Some(n.size.max(6.0)), bold: Some(n.bold), ..Default::default() });
                self.set_align(&loc, None, None, n.align);
                self.set_anchor(&loc, n.anchor);
                self.set_text_box(&loc, Some([4.0, 2.0, 4.0, 2.0]), Some(true), Some("shrink"));
            }
            if let Some(k) = n.item { if let Some(c) = self.element_mut(&loc).and_then(|sp| sp.elements_mut().find(|e| e.local().starts_with("nv"))).and_then(|nv| nv.child_mut("p:cNvPr")) { c.set_attr("descr", format!("sard:item:{k}")); } }
            locs.push(loc);
            frames.push(n.x);
        }
        let g = self.group(&locs, &frames)?;
        if let Some(c) = self.element_mut(&g).and_then(|e| e.path_mut("p:nvGrpSpPr/p:cNvPr")) { c.set_attr("descr", descr); c.set_attr("name", name); }
        Some(g)
    }

    /// Insert a SmartArt graphic.
    pub fn insert_diagram(&mut self, part: &str, spec: &Spec, fr: &Xfrm) -> Option<Loc> {
        let mut nodes = layout(&spec.layout, &spec.items, fr);
        if nodes.is_empty() { return None; }
        style(&mut nodes, &spec.colors, &spec.style, &Palette::of(self, part));
        if spec.rtl { mirror(&mut nodes, fr); }
        self.insert_nodes(part, &nodes, "SmartArt", &spec.to_string())
    }

    /// The SmartArt of a group (also reading Qalam's older graphics).
    pub fn diagram(&self, loc: &Loc) -> Option<Spec> {
        let d = self.element(loc)?.path("p:nvGrpSpPr/p:cNvPr")?.attr("descr")?;
        if let Some(s) = Spec::parse(d) { return Some(s); }
        let old = word_engine::objects::Diagram::from_spec(d, 0.0, 0.0)?;
        use word_engine::objects::DiagramKind as K;
        let layout = match old.kind { K::List => "basicBlockList", K::Process => "basicProcess", K::Cycle => "basicCycle", K::Hierarchy => "orgChart", K::Pyramid => "basicPyramid" };
        Some(Spec { layout: layout.into(), colors: "colorful".into(), style: "subtle".into(), items: old.items, rtl: false })
    }

    /// Rebuild a group (SmartArt or Mermaid) in its place and z position with new nodes.
    pub fn replace_group(&mut self, loc: &Loc, build: impl FnOnce(&mut Presentation, &Xfrm) -> Option<Loc>) -> Option<Loc> {
        let x = self.xfrm(loc)?;
        self.delete_shapes(std::slice::from_ref(loc));
        let g = build(self, &x)?;
        let mut cur = g;
        while cur.path[0] > loc.path[0] { match self.reorder(&cur, crate::edit::Order::Backward) { Some(n) if n != cur => cur = n, _ => break } }
        Some(cur)
    }

    /// The item a shape of a graphic shows (from its tag).
    pub fn diagram_item(&self, child: &Loc) -> Option<usize> {
        self.element(child)?.elements().find(|e| e.local().starts_with("nv"))?.child("p:cNvPr")?.attr("descr")?.strip_prefix("sard:item:")?.parse().ok()
    }

    pub fn update_diagram(&mut self, loc: &Loc, spec: &Spec) -> Option<Loc> {
        let part = loc.part.clone();
        self.replace_group(loc, |p, x| p.insert_diagram(&part, spec, x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_edits() {
        let mut v = parse_items("A\n\ta1\n\ta2\nB\nC");
        assert_eq!(edit_item(&mut v, 0, "down", ""), Some(1));
        assert_eq!(v.iter().map(|i| i.1.as_str()).collect::<Vec<_>>(), ["B", "A", "a1", "a2", "C"]);
        edit_item(&mut v, 1, "after", "N");
        assert_eq!(v[4], (0, "N".into()));
        edit_item(&mut v, 3, "promote", "");
        assert_eq!(v[3], (0, "a2".into()));
        edit_item(&mut v, 3, "demote", "");
        assert_eq!(v[3], (1, "a2".into()));
        edit_item(&mut v, 1, "remove", "");
        assert_eq!(v[1], (0, "a1".into()));
        let g = layout("orgChart", &parse_items("CEO\n\tCTO\n\tCFO"), &Xfrm { w: 1e6, h: 1e6, ..Default::default() });
        assert_eq!(g.iter().filter_map(|n| n.item).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn every_layout_draws_and_round_trips() {
        let mut p = Presentation::new_blank(true);
        let l = p.all_layouts().last().cloned().unwrap();
        let s = p.add_slide(&l, 0);
        let fr = Xfrm { x: 1e6, y: 1e6, w: 8e6, h: 4.5e6, ..Default::default() };
        let text = "Plan\n\tGoals\n\tBudget\nBuild\n\tCode\nTest\nLaunch";
        for (id, ..) in LAYOUTS {
            for (c, _) in COLORS.iter().take(3) {
                let spec = Spec { layout: id.to_string(), colors: c.to_string(), style: "moderate".into(), items: parse_items(text), rtl: false };
                let g = p.insert_diagram(&s, &spec, &fr).unwrap_or_else(|| panic!("{id}"));
                assert_eq!(p.diagram(&g).unwrap(), spec);
                p.delete_shapes(&[g]);
            }
        }
        let spec = Spec::new("orgChart", "CEO\n\tCTO\n\t\tDev\n\tCFO");
        let g = p.insert_diagram(&s, &spec, &fr).unwrap();
        let g = p.update_diagram(&g, &Spec { layout: "horizontalHierarchy".into(), ..spec.clone() }).unwrap();
        assert_eq!(p.diagram(&g).unwrap().layout, "horizontalHierarchy");
        assert!(crate::render::slide_png(&p, &s, 400).is_some());
        if let Ok(dir) = std::env::var("SLIDES_SNAPSHOTS") {
            // A sheet of every layout.
            let mut q = Presentation::new_blank(true);
            let s2 = q.add_slide(&l, 0);
            let (cols, cw, ch) = (5usize, 12_192_000.0 / 5.0, 6_858_000.0 / 5.0);
            for (k, (id, ..)) in LAYOUTS.iter().enumerate() {
                let fr = Xfrm { x: (k % cols) as f64 * cw + cw * 0.06, y: (k / cols) as f64 * ch + ch * 0.06, w: cw * 0.88, h: ch * 0.88, ..Default::default() };
                let text = if id.contains("Hierarchy") || *id == "orgChart" { "CEO\n\tSales\n\tTech\n\t\tWeb\n\t\tApps" } else { "Plan\n\tGoals\nBuild\nTest\nLaunch" };
                q.insert_diagram(&s2, &Spec { layout: id.to_string(), colors: "colorful".into(), style: "moderate".into(), items: parse_items(text), rtl: false }, &fr);
            }
            std::fs::write(format!("{dir}/smartart-layouts.png"), crate::render::slide_png(&q, &s2, 1600).unwrap()).unwrap();
        }
    }
}
