//! Table editing (Table Design and Table Layout): rows and columns added and removed, sizes,
//! merged cells, cell fill, borders, margins and alignment, and the style options.

use crate::pres::Presentation;
use crate::scene::Loc;
use crate::xml::{Element, Node};

fn new_cell() -> Element {
    Element::new("a:tc").with_child(Element::new("a:txBody").with_child(Element::new("a:bodyPr")).with_child(Element::new("a:lstStyle")).with_child(Element::new("a:p").with_child(Element::new("a:endParaRPr").with_attr("lang", "en-US")))).with_child(Element::new("a:tcPr"))
}

impl Presentation {
    fn tbl_mut(&mut self, loc: &Loc) -> Option<&mut Element> { self.element_mut(loc)?.path_mut("a:graphic/a:graphicData/a:tbl") }
    fn tbl(&self, loc: &Loc) -> Option<&Element> { self.element(loc)?.path("a:graphic/a:graphicData/a:tbl") }

    /// (rows, columns) of a table.
    pub fn table_size(&self, loc: &Loc) -> Option<(usize, usize)> {
        let t = self.tbl(loc)?;
        Some((t.children_named("a:tr").count(), t.path("a:tblGrid").map(|g| g.children_named("a:gridCol").count()).unwrap_or(0)))
    }

    /// Insert a row before `at` (rows count), copying the formatting of its neighbour.
    pub fn insert_row(&mut self, loc: &Loc, at: usize) {
        let Some(t) = self.tbl_mut(loc) else { return };
        let rows: Vec<usize> = t.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(e) if e.name == "a:tr")).map(|(i, _)| i).collect();
        let Some(&src_idx) = rows.get(at.min(rows.len().saturating_sub(1))) else { return };
        let Node::Element(src) = &t.children[src_idx] else { return };
        let mut row = src.clone();
        // Empty cells, keeping cell properties and run formatting.
        for tc in row.elements_mut().filter(|e| e.name == "a:tc") {
            tc.remove_attr("rowSpan"); tc.remove_attr("vMerge");
            if let Some(tx) = tc.child_mut("a:txBody") {
                let end = tx.find("a:rPr").cloned().map(|mut r| { r.name = "a:endParaRPr".into(); r });
                tx.remove_children("a:p");
                let mut p = Element::new("a:p");
                if let Some(e) = end { p.children.push(Node::Element(e)); }
                tx.children.push(Node::Element(p));
            }
        }
        let pos = if at >= rows.len() { rows.last().map(|i| i + 1).unwrap_or(t.children.len()) } else { rows[at] };
        t.children.insert(pos, Node::Element(row));
    }

    /// Insert a column before `at`, splitting the width of its neighbour.
    pub fn insert_column(&mut self, loc: &Loc, at: usize) {
        let Some(t) = self.tbl_mut(loc) else { return };
        let Some(grid) = t.child_mut("a:tblGrid") else { return };
        let cols: Vec<usize> = grid.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(_))).map(|(i, _)| i).collect();
        let w = cols.get(at.min(cols.len().saturating_sub(1))).and_then(|&i| if let Node::Element(e) = &grid.children[i] { e.attr_f64("w") } else { None }).unwrap_or(914_400.0);
        let new = Node::Element(Element::new("a:gridCol").with_attr("w", (w.round() as i64).to_string()));
        let pos = if at >= cols.len() { cols.last().map(|i| i + 1).unwrap_or(grid.children.len()) } else { cols[at] };
        grid.children.insert(pos, new);
        for tr in t.elements_mut().filter(|e| e.name == "a:tr") {
            let cells: Vec<usize> = tr.children.iter().enumerate().filter(|(_, n)| matches!(n, Node::Element(e) if e.name == "a:tc")).map(|(i, _)| i).collect();
            let mut cell = new_cell();
            // Copy the neighbour's cell properties (fill, borders).
            if let Some(&ni) = cells.get(at.min(cells.len().saturating_sub(1))) { if let Node::Element(n) = &tr.children[ni] { if let Some(pr) = n.child("a:tcPr") { cell.remove_children("a:tcPr"); cell.children.push(Node::Element(pr.clone())); } } }
            let pos = if at >= cells.len() { cells.last().map(|i| i + 1).unwrap_or(tr.children.len()) } else { cells[at] };
            tr.children.insert(pos, Node::Element(cell));
        }
        // The frame widens with the table, but stays on the slide: past the right edge, the
        // columns shrink to keep the table's width.
        if let Some(mut x) = self.xfrm(loc) {
            let (sw, _) = self.size();
            if x.x + x.w + w <= sw { x.w += w; self.set_xfrm(loc, &x); }
            else if let Some(grid) = self.tbl_mut(loc).and_then(|t| t.child_mut("a:tblGrid")) {
                let total: f64 = grid.children_named("a:gridCol").filter_map(|c| c.attr_f64("w")).sum();
                let k = x.w / total.max(1.0);
                for c in grid.elements_mut() { if let Some(cw) = c.attr_f64("w") { c.set_attr("w", ((cw * k).round() as i64).to_string()); } }
            }
        }
    }

    pub fn delete_row(&mut self, loc: &Loc, row: usize) {
        let Some(t) = self.tbl_mut(loc) else { return };
        if t.children_named("a:tr").count() <= 1 { return; }
        let mut k = 0;
        t.children.retain(|n| if let Node::Element(e) = n { if e.name == "a:tr" { k += 1; return k - 1 != row; } true } else { true });
    }

    pub fn delete_column(&mut self, loc: &Loc, col: usize) {
        let w = {
            let Some(t) = self.tbl_mut(loc) else { return };
            let Some(grid) = t.child_mut("a:tblGrid") else { return };
            if grid.children_named("a:gridCol").count() <= 1 { return; }
            let w = grid.children_named("a:gridCol").nth(col).and_then(|c| c.attr_f64("w")).unwrap_or(0.0);
            let mut k = 0;
            grid.children.retain(|n| if let Node::Element(_) = n { k += 1; k - 1 != col } else { true });
            for tr in t.elements_mut().filter(|e| e.name == "a:tr") {
                let mut k = 0;
                tr.children.retain(|n| if let Node::Element(e) = n { if e.name == "a:tc" { k += 1; return k - 1 != col; } true } else { true });
            }
            w
        };
        if let Some(mut x) = self.xfrm(loc) { x.w = (x.w - w).max(91_440.0); self.set_xfrm(loc, &x); }
    }

    /// Set a column's width (EMU); the frame follows the table's total width.
    pub fn set_column_width(&mut self, loc: &Loc, col: usize, w: f64) {
        let total = {
            let Some(t) = self.tbl_mut(loc) else { return };
            let Some(grid) = t.child_mut("a:tblGrid") else { return };
            if let Some(c) = grid.elements_mut().filter(|e| e.name == "a:gridCol").nth(col) { c.set_attr("w", (w.max(91_440.0).round() as i64).to_string()); }
            grid.children_named("a:gridCol").filter_map(|c| c.attr_f64("w")).sum::<f64>()
        };
        if let Some(mut x) = self.xfrm(loc) { x.w = total; self.set_xfrm(loc, &x); }
    }
    /// Set a row's (minimum) height (EMU).
    pub fn set_row_height(&mut self, loc: &Loc, row: usize, h: f64) {
        if let Some(tr) = self.tbl_mut(loc).and_then(|t| t.elements_mut().filter(|e| e.name == "a:tr").nth(row)) { tr.set_attr("h", (h.max(91_440.0).round() as i64).to_string()); }
    }
    /// Make all columns the same width, and all rows the same height.
    pub fn distribute_columns(&mut self, loc: &Loc) {
        let Some((_, n)) = self.table_size(loc) else { return };
        let total: f64 = self.tbl(loc).and_then(|t| t.path("a:tblGrid")).map(|g| g.children_named("a:gridCol").filter_map(|c| c.attr_f64("w")).sum()).unwrap_or(0.0);
        for c in 0..n { self.set_column_width(loc, c, total / n as f64); }
    }
    pub fn distribute_rows(&mut self, loc: &Loc) {
        let Some(t) = self.tbl(loc) else { return };
        let hs: Vec<f64> = t.children_named("a:tr").map(|r| r.attr_f64("h").unwrap_or(370_840.0)).collect();
        let avg = hs.iter().sum::<f64>() / hs.len().max(1) as f64;
        for r in 0..hs.len() { self.set_row_height(loc, r, avg); }
    }

    fn cell_mut(&mut self, loc: &Loc, row: usize, col: usize) -> Option<&mut Element> {
        self.tbl_mut(loc)?.elements_mut().filter(|e| e.name == "a:tr").nth(row)?.elements_mut().filter(|e| e.name == "a:tc").nth(col)
    }
    fn tc_pr(&mut self, loc: &Loc, row: usize, col: usize) -> Option<&mut Element> {
        Some(self.cell_mut(loc, row, col)?.ensure_at("a:tcPr", &["a:extLst"]))
    }

    /// Cell shading (hex), or no fill (""), or back to the style (None).
    pub fn set_cell_fill(&mut self, loc: &Loc, cells: &[(usize, usize)], fill: Option<&str>) {
        for &(r, c) in cells {
            let Some(pr) = self.tc_pr(loc, r, c) else { continue };
            for n in ["a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill"] { pr.remove_children(n); }
            if let Some(f) = fill {
                let el = if f.is_empty() { Element::new("a:noFill") } else { Element::new("a:solidFill").with_child(Element::new("a:srgbClr").with_attr("val", f)) };
                let pos = pr.children.iter().position(|n| matches!(n, Node::Element(e) if matches!(e.name.as_str(), "a:headers" | "a:extLst"))).unwrap_or(pr.children.len());
                pr.children.insert(pos, Node::Element(el));
            }
        }
    }
    /// Cell borders: which ("all", "outside", "inside", "top", "bottom", "left", "right", "none")
    /// with a color (hex) and width (points).
    pub fn set_cell_borders(&mut self, loc: &Loc, cells: &[(usize, usize)], which: &str, color: &str, width: f64) {
        let (r0, r1) = (cells.iter().map(|c| c.0).min().unwrap_or(0), cells.iter().map(|c| c.0).max().unwrap_or(0));
        let (c0, c1) = (cells.iter().map(|c| c.1).min().unwrap_or(0), cells.iter().map(|c| c.1).max().unwrap_or(0));
        for &(r, c) in cells {
            let edges: Vec<&str> = match which {
                "all" | "none" => vec!["a:lnL", "a:lnR", "a:lnT", "a:lnB"],
                "outside" => [("a:lnL", c == c0), ("a:lnR", c == c1), ("a:lnT", r == r0), ("a:lnB", r == r1)].iter().filter(|e| e.1).map(|e| e.0).collect(),
                "inside" => [("a:lnL", c != c0), ("a:lnR", c != c1), ("a:lnT", r != r0), ("a:lnB", r != r1)].iter().filter(|e| e.1).map(|e| e.0).collect(),
                "top" => if r == r0 { vec!["a:lnT"] } else { vec![] }, "bottom" => if r == r1 { vec!["a:lnB"] } else { vec![] },
                "left" => if c == c0 { vec!["a:lnL"] } else { vec![] }, "right" => if c == c1 { vec!["a:lnR"] } else { vec![] },
                _ => vec![],
            };
            let Some(pr) = self.tc_pr(loc, r, c) else { continue };
            for e in edges {
                pr.remove_children(e);
                let ln = if which == "none" { Element::new(e).with_attr("w", "0").with_child(Element::new("a:noFill")) } else { Element::new(e).with_attr("w", ((width * 12_700.0).round() as i64).to_string()).with_child(Element::new("a:solidFill").with_child(Element::new("a:srgbClr").with_attr("val", color))) };
                // Borders come first in tcPr, in L R T B order.
                let order = ["a:lnL", "a:lnR", "a:lnT", "a:lnB", "a:lnTlToBr", "a:lnBlToTr"];
                let rank = order.iter().position(|x| *x == e).unwrap_or(9);
                let pos = pr.children.iter().position(|n| matches!(n, Node::Element(x) if order.iter().position(|o| *o == x.name).is_none_or(|k| k > rank))).unwrap_or(pr.children.len());
                pr.children.insert(pos, Node::Element(ln));
            }
        }
    }
    /// Vertical alignment of cell text ("t", "ctr", "b").
    pub fn set_cell_anchor(&mut self, loc: &Loc, cells: &[(usize, usize)], anchor: &str) {
        for &(r, c) in cells { if let Some(pr) = self.tc_pr(loc, r, c) { pr.set_attr("anchor", anchor); } }
    }
    /// Cell margins (points): l, t, r, b.
    pub fn set_cell_margins(&mut self, loc: &Loc, cells: &[(usize, usize)], m: [f64; 4]) {
        for &(r, c) in cells { if let Some(pr) = self.tc_pr(loc, r, c) { for (k, v) in ["marL", "marT", "marR", "marB"].iter().zip(m) { pr.set_attr(k, ((v * 12_700.0).round() as i64).to_string()); } } }
    }

    /// Merge a rectangle of cells (the first keeps the text of all).
    pub fn merge_cells(&mut self, loc: &Loc, r0: usize, c0: usize, r1: usize, c1: usize) {
        let mut texts = vec![];
        for r in r0..=r1 { for c in c0..=c1 { let t = self.shape_text(loc, Some((r, c))); if !t.is_empty() && (r, c) != (r0, c0) { texts.push(t); } } }
        let Some(t) = self.tbl_mut(loc) else { return };
        for (ri, tr) in t.elements_mut().filter(|e| e.name == "a:tr").enumerate() {
            if ri < r0 || ri > r1 { continue; }
            for (ci, tc) in tr.elements_mut().filter(|e| e.name == "a:tc").enumerate() {
                if ci < c0 || ci > c1 { continue; }
                if (ri, ci) == (r0, c0) {
                    if c1 > c0 { tc.set_attr("gridSpan", (c1 - c0 + 1).to_string()); }
                    if r1 > r0 { tc.set_attr("rowSpan", (r1 - r0 + 1).to_string()); }
                } else {
                    if ci > c0 { tc.set_attr("hMerge", "1"); }
                    if ri > r0 { tc.set_attr("vMerge", "1"); }
                }
            }
        }
        if !texts.is_empty() {
            let first = self.shape_text(loc, Some((r0, c0)));
            let n = first.chars().count();
            self.insert_text(loc, Some((r0, c0)), n, &format!("{}{}", if first.is_empty() { "" } else { "\n" }, texts.join("\n")));
            for r in r0..=r1 { for c in c0..=c1 { if (r, c) != (r0, c0) { let n = self.shape_text(loc, Some((r, c))).chars().count(); self.delete_text(loc, Some((r, c)), 0, n); } } }
        }
    }
    /// Split a merged cell back into its cells.
    pub fn split_cell(&mut self, loc: &Loc, row: usize, col: usize) {
        let Some(t) = self.tbl_mut(loc) else { return };
        let (rs, cs) = {
            let Some(tc) = t.elements().filter(|e| e.name == "a:tr").nth(row).and_then(|tr| tr.elements().filter(|e| e.name == "a:tc").nth(col)) else { return };
            (tc.attr_i64("rowSpan").unwrap_or(1).max(1) as usize, tc.attr_i64("gridSpan").unwrap_or(1).max(1) as usize)
        };
        for (ri, tr) in t.elements_mut().filter(|e| e.name == "a:tr").enumerate() {
            if ri < row || ri >= row + rs { continue; }
            for (ci, tc) in tr.elements_mut().filter(|e| e.name == "a:tc").enumerate() {
                if ci < col || ci >= col + cs { continue; }
                for k in ["rowSpan", "gridSpan", "hMerge", "vMerge"] { tc.remove_attr(k); }
            }
        }
    }

    /// Table style options: header row, total row, banded rows, first and last column, banded columns.
    pub fn set_table_option(&mut self, loc: &Loc, option: &str, on: bool) {
        let Some(t) = self.tbl_mut(loc) else { return };
        let pr = t.ensure_at("a:tblPr", &["a:tblGrid", "a:tr"]);
        if on { pr.set_attr(option, "1"); } else { pr.remove_attr(option); }
    }
    pub fn table_option(&self, loc: &Loc, option: &str) -> bool { self.tbl(loc).and_then(|t| t.child("a:tblPr")).and_then(|p| p.attr_bool(option)).unwrap_or(false) }

    /// A table style: one of the built-in style ids (Medium Style 2 – Accent 1 is the default).
    pub fn set_table_style(&mut self, loc: &Loc, style_id: &str, accent: &str) {
        let Some(t) = self.tbl_mut(loc) else { return };
        let pr = t.ensure_at("a:tblPr", &["a:tblGrid", "a:tr"]);
        pr.remove_children("a:tableStyleId");
        pr.children.push(Node::Element(Element::new("a:tableStyleId").with_text(style_id)));
        // Remembered so the look follows the chosen accent when the style isn't in the file.
        pr.set_attr("qalamAccent", accent);
        // Direct cell fills would hide the style.
        for tr in t.elements_mut().filter(|e| e.name == "a:tr") {
            for tc in tr.elements_mut().filter(|e| e.name == "a:tc") {
                if let Some(p) = tc.child_mut("a:tcPr") { for n in ["a:noFill", "a:solidFill", "a:gradFill"] { p.remove_children(n); } }
            }
        }
    }

    /// Delete the whole table.
    pub fn delete_table(&mut self, loc: &Loc) { self.delete_shapes(std::slice::from_ref(loc)); }
}

/// Built-in table styles offered by Table Design: (style id, name, accent token).
pub const TABLE_STYLES: &[(&str, &str, &str)] = &[
    ("{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}", "Medium Style 2 – Accent 1", "accent1"),
    ("{21E4AEA4-8DFA-4A89-87EB-49C32662AFE8}", "Medium Style 2 – Accent 2", "accent2"),
    ("{F5AB1C69-6EDB-4FF4-983F-18BD219EF322}", "Medium Style 2 – Accent 3", "accent3"),
    ("{00A15C55-8517-42AA-B614-E9B94910E393}", "Medium Style 2 – Accent 4", "accent4"),
    ("{7DF18680-E054-41AD-8BC1-D1AEF772440D}", "Medium Style 2 – Accent 5", "accent5"),
    ("{93296810-A885-4BE3-A3E7-6D5BEEA58F35}", "Medium Style 2 – Accent 6", "accent6"),
    ("{073A0DAA-6AF3-43AB-8588-CEC1D06C72B9}", "Medium Style 2", "tx1"),
    ("{2D5ABB26-0587-4C30-8999-92F81FD0307C}", "No Style, No Grid", "none"),
    ("{5940675A-B579-460E-94D1-54222C63F5DA}", "No Style, Table Grid", "grid"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Xfrm;

    #[test]
    fn table_editing() {
        let mut p = Presentation::new_blank(true);
        let blank = p.all_layouts().into_iter().find(|l| p.layout_type(l) == "blank").unwrap();
        let s = p.add_slide(&blank, 0);
        let t = p.insert_table(&s, 2, 2, &Xfrm { x: 1e6, y: 1e6, w: 4e6, h: 1e6, ..Default::default() }).unwrap();
        p.set_shape_text(&t, Some((0, 0)), "A");
        p.set_shape_text(&t, Some((0, 1)), "B");
        p.insert_row(&t, 1);
        p.insert_column(&t, 2);
        assert_eq!(p.table_size(&t), Some((3, 3)));
        assert_eq!(p.shape_text(&t, Some((0, 0))), "A");
        p.merge_cells(&t, 0, 0, 0, 1);
        assert_eq!(p.shape_text(&t, Some((0, 0))), "A\nB");
        p.split_cell(&t, 0, 0);
        p.delete_row(&t, 2);
        p.delete_column(&t, 2);
        assert_eq!(p.table_size(&t), Some((2, 2)));
        p.set_cell_fill(&t, &[(1, 1)], Some("FF0000"));
        p.set_cell_borders(&t, &[(0, 0), (1, 1)], "outside", "000000", 1.0);
        p.set_table_option(&t, "bandRow", false);
        p.set_table_style(&t, TABLE_STYLES[1].0, TABLE_STYLES[1].2);
        assert!(Presentation::open(&p.save().unwrap()).is_ok());
    }
}
