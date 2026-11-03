//! Table editing beyond rows and columns (table-editing.js): cell blocks, merge and split,
//! cell size, AutoFit and distribute, cell formatting, sort, and text ↔ table conversion.

use crate::format::*;
use crate::model::*;
use crate::model::TablePart;

/// A rectangle of grid cells in one table: rows r0..=r1, grid columns c0..=c1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellBlock { pub table: usize, pub r0: usize, pub r1: usize, pub c0: usize, pub c1: usize }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoFit { Contents, Window, Fixed }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableStyle { Plain, Grid, HeaderShaded, Banded }

/// Border presets (applyBorderPreset): which edges of the selected block get the pen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderPreset { All, Outside, Inside, InsideH, InsideV, Top, Bottom, Left, Right, None }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellShift { Right, Left }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Separator { Tab, Comma, Paragraph }

impl Table {
    /// The cell covering grid position (row, column), with its index in the row.
    pub fn cell_at_grid(&self, row: usize, col: usize) -> Option<(usize, &Cell)> {
        let r = self.rows.get(row)?;
        let mut at = r.grid_before as usize;
        for (k, c) in r.cells.iter().enumerate() {
            if col >= at && col < at + c.span.max(1) as usize { return Some((k, c)); }
            at += c.span.max(1) as usize;
        }
        None
    }
    /// The row where a vertically merged cell's run starts.
    fn merge_origin(&self, row: usize, col: usize) -> usize {
        let mut r = row;
        while r > 0 && self.cell_at_grid(r, col).is_some_and(|(_, c)| c.v_merge == VMerge::Continue) { r -= 1; }
        r
    }
    fn width(&self) -> i32 { self.grid.iter().sum() }
    pub(crate) fn sync_cell_widths(&mut self) {
        let grid = self.grid.clone();
        for row in &mut self.rows {
            let mut g = row.grid_before as usize;
            for c in &mut row.cells {
                let span = c.span.max(1) as usize;
                c.width = grid[g.min(grid.len())..(g + span).min(grid.len())].iter().sum();
                g += span;
            }
        }
    }
}

impl Document {
    /// The cells touched by the selection as a rectangle (the caret's cell when collapsed).
    pub fn cell_block(&self) -> Option<CellBlock> {
        // The rectangle between the anchor's cell and the focus's cell, as Word selects cells.
        let cell_of = |pos: usize| self.paragraphs[self.locate(pos).0].cell;
        let (s, e) = (self.selection.anchor, self.selection.focus);
        let (a, f) = (cell_of(s), cell_of(e).or(cell_of(e.saturating_sub(1))));
        let first = a.or(f)?;
        let ti = self.table_of_cell(first)?;
        let t = &self.tables[ti];
        let mut block: Option<CellBlock> = None;
        for id in [a, f].into_iter().flatten().filter(|id| t.has_cell(*id)) {
            let Some((r, k)) = t.find_cell(id) else { continue };
            let c0 = t.grid_col(r, k);
            let c1 = c0 + t.rows[r].cells[k].span.max(1) as usize - 1;
            let r0 = t.merge_origin(r, c0);
            block = Some(match block {
                None => CellBlock { table: ti, r0, r1: r, c0, c1 },
                Some(b) => CellBlock { table: ti, r0: b.r0.min(r0), r1: b.r1.max(r), c0: b.c0.min(c0), c1: b.c1.max(c1) },
            });
        }
        // Grow the block over vertical merges that continue below it.
        let mut b = block?;
        while b.r1 + 1 < t.rows.len() && (b.c0..=b.c1).any(|c| t.cell_at_grid(b.r1 + 1, c).is_some_and(|(_, x)| x.v_merge == VMerge::Continue)) { b.r1 += 1; }
        Some(b)
    }

    /// Cell ids in a block, row by row.
    pub fn block_cells(&self, b: CellBlock) -> Vec<u64> {
        let t = &self.tables[b.table];
        let mut out = vec![];
        for r in b.r0..=b.r1 {
            let row = &t.rows[r];
            let mut at = row.grid_before as usize;
            for c in &row.cells {
                let span = c.span.max(1) as usize;
                if at + span > b.c0 && at <= b.c1 { out.push(c.id); }
                at += span;
            }
        }
        out
    }

    fn cell_range(&self, id: u64) -> (usize, usize) {
        let r = self.cell_paragraphs(id);
        let s = self.paragraph_start(r.start);
        (s, self.paragraph_start(r.end - 1) + self.paragraphs[r.end - 1].len())
    }

    /// Select whole cells of a block (Select Cell / Row / Column / Table).
    pub fn select_block(&mut self, b: CellBlock) {
        let cells = self.block_cells(b);
        let (Some(first), Some(last)) = (cells.first(), cells.last()) else { return };
        let s = self.cell_range(*first).0;
        let e = self.cell_range(*last).1;
        self.select(s, e);
    }
    pub fn select_table_part(&mut self, part: TablePart) {
        let Some(b) = self.cell_block() else { return };
        let t = &self.tables[b.table];
        let cols = t.grid.len().max(t.rows.iter().map(|r| r.grid_before as usize + r.cells.iter().map(|c| c.span.max(1) as usize).sum::<usize>()).max().unwrap_or(1));
        let block = match part {
            TablePart::Row => CellBlock { c0: 0, c1: cols - 1, ..b },
            TablePart::Column => CellBlock { r0: 0, r1: t.rows.len() - 1, ..b },
            TablePart::Table => CellBlock { table: b.table, r0: 0, r1: t.rows.len() - 1, c0: 0, c1: cols - 1 },
        };
        self.select_block(block);
    }

    /// Apply a change to every selected cell in one undo step.
    pub fn for_selected_cells(&mut self, f: impl Fn(&mut Cell)) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        let ids = self.block_cells(b);
        for row in &mut self.tables[b.table].rows { for c in &mut row.cells { if ids.contains(&c.id) { f(c); } } }
        self.changed();
    }
    pub fn set_cell_shading(&mut self, color: Option<Color>) { self.for_selected_cells(|c| c.shading = color); }
    /// Vertical alignment in the cells plus horizontal paragraph alignment (Word's 9-way cell alignment).
    pub fn set_cell_alignment(&mut self, vertical: CellVAlign, horizontal: Alignment) {
        let Some(b) = self.cell_block() else { return };
        let ids = self.block_cells(b);
        self.for_selected_cells(|c| c.v_align = vertical);
        for p in &mut self.paragraphs { if p.cell.is_some_and(|c| ids.contains(&c)) { p.props.alignment = Some(horizontal); } }
        self.changed();
    }

    /// Apply a border preset with `pen` to the selected block. `BorderPreset::None` writes "no
    /// border" on every edge, hiding the table's borders there too.
    pub fn apply_border_preset(&mut self, preset: BorderPreset, pen: Border) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        let nil = Border { width: 0.0, space: 0.0, color: None };
        let t = &mut self.tables[b.table];
        for (ri, row) in t.rows.iter_mut().enumerate() {
            if ri < b.r0 || ri > b.r1 { continue; }
            let mut at = row.grid_before as usize;
            for c in &mut row.cells {
                let span = c.span.max(1) as usize;
                let (c0, c1) = (at, at + span - 1);
                at += span;
                if c1 < b.c0 || c0 > b.c1 { continue; }
                let (top, bottom, left, right) = (ri == b.r0, ri == b.r1, c0 <= b.c0, c1 >= b.c1);
                let set = |side: &mut Option<Border>, on: bool, v: Border| if on { *side = Some(v); };
                let bd = &mut c.borders;
                match preset {
                    BorderPreset::All => { *bd = CellBorders { top: Some(pen), right: Some(pen), bottom: Some(pen), left: Some(pen) }; }
                    BorderPreset::None => { *bd = CellBorders { top: Some(nil), right: Some(nil), bottom: Some(nil), left: Some(nil) }; }
                    BorderPreset::Outside => { set(&mut bd.top, top, pen); set(&mut bd.bottom, bottom, pen); set(&mut bd.left, left, pen); set(&mut bd.right, right, pen); }
                    BorderPreset::Inside => { set(&mut bd.top, !top, pen); set(&mut bd.bottom, !bottom, pen); set(&mut bd.left, !left, pen); set(&mut bd.right, !right, pen); }
                    BorderPreset::InsideH => { set(&mut bd.top, !top, pen); set(&mut bd.bottom, !bottom, pen); }
                    BorderPreset::InsideV => { set(&mut bd.left, !left, pen); set(&mut bd.right, !right, pen); }
                    BorderPreset::Top => set(&mut bd.top, top, pen),
                    BorderPreset::Bottom => set(&mut bd.bottom, bottom, pen),
                    BorderPreset::Left => set(&mut bd.left, left, pen),
                    BorderPreset::Right => set(&mut bd.right, right, pen),
                }
            }
        }
        self.changed();
    }

    /// Margins for the selected cells (twips: top, right, bottom, left); `None` inherits the table's.
    pub fn set_cell_margins(&mut self, margins: Option<[i32; 4]>) { self.for_selected_cells(|c| c.margins = margins); }
    /// The table's default cell margins.
    pub fn set_table_cell_margins(&mut self, margins: [i32; 4]) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[b.table];
        t.cell_margins = margins.map(|v| v.clamp(0, 31680));
        // tblCellMar is regenerated from the model.
        t.raw_props = None;
        self.changed();
    }

    /// Insert a cell before the current one, shifting the row's cells right (insertCells). The
    /// row grows by one grid column; other rows end with an empty grid position there.
    pub fn insert_cells(&mut self) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(ti) = self.table_of_cell(id) else { return false };
        let (r, k) = self.tables[ti].find_cell(id).unwrap();
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[ti];
        let width = t.grid.last().copied().unwrap_or(1440);
        let used = |row: &Row| row.grid_before as usize + row.cells.iter().map(|c| c.span.max(1) as usize).sum::<usize>() + row.grid_after as usize;
        // Grow the grid when this row has no trailing slack.
        if t.rows[r].grid_after > 0 { t.rows[r].grid_after -= 1; }
        else {
            t.grid.push(width);
            for (i, row) in t.rows.iter_mut().enumerate() { if i != r { row.grid_after += 1; } }
        }
        let mut cell = Cell::new(t.grid[t.grid_col(r, k).min(t.grid.len() - 1)]);
        cell.shading = t.rows[r].cells[k].shading;
        let new_id = cell.id;
        t.rows[r].cells.insert(k, cell);
        let _ = used;
        t.sync_cell_widths();
        let at = self.cell_paragraphs(id).start;
        let mut p = self.paragraphs[at].sibling();
        p.cell = Some(new_id);
        self.paragraphs.insert(at, p);
        self.select(self.paragraph_start(at), self.paragraph_start(at));
        self.changed();
        true
    }

    /// Delete the current cell, shifting the row's later cells left (deleteCells).
    pub fn delete_cell(&mut self) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(ti) = self.table_of_cell(id) else { return false };
        let (r, k) = self.tables[ti].find_cell(id).unwrap();
        if self.tables[ti].rows[r].cells.len() == 1 { self.delete_table_part(TablePart::Row); return true; }
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[ti];
        let gone = t.rows[r].cells.remove(k);
        t.rows[r].grid_after += gone.span.max(1);
        let first = self.cell_paragraphs(id).start;
        self.paragraphs.retain(|p| p.cell != Some(id));
        let at = first.min(self.paragraphs.len() - 1);
        self.select(self.paragraph_start(at), self.paragraph_start(at));
        self.changed();
        true
    }

    /// Divide the current cell into two rows (Draw Table's horizontal stroke): a row is added
    /// below, where the other cells continue as vertical merges.
    pub fn split_cell_rows(&mut self) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(ti) = self.table_of_cell(id) else { return false };
        let (r, k) = self.tables[ti].find_cell(id).unwrap();
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[ti];
        let mut new_row = Row { cells: vec![], header: false, ..t.rows[r].clone() };
        new_row.min_height = 0;
        let mut new_ids = vec![];
        for (i, c) in t.rows[r].cells.iter_mut().enumerate() {
            let mut n = Cell { id: new_id(), ..c.clone() };
            if i == k { n.v_merge = VMerge::None; c.v_merge = if c.v_merge == VMerge::Continue { VMerge::Continue } else { VMerge::None }; }
            else {
                if c.v_merge == VMerge::None { c.v_merge = VMerge::Restart; }
                n.v_merge = VMerge::Continue;
            }
            new_ids.push(n.id);
            new_row.cells.push(n);
        }
        t.rows.insert(r + 1, new_row);
        // New paragraphs follow the row's last cell.
        let last = t.rows[r].cells.last().unwrap().id;
        let mut at = self.cell_paragraphs(last).end;
        let template = self.paragraphs[self.cell_paragraphs(id).start].sibling();
        for nid in new_ids { let mut p = template.clone(); p.id = new_id(); p.cell = Some(nid); self.paragraphs.insert(at, p); at += 1; }
        self.changed();
        true
    }

    /// Split the current cell into two columns at a fraction of its width (Draw Table's vertical stroke).
    pub fn split_cell_at(&mut self, fraction: f32) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(ti) = self.table_of_cell(id) else { return false };
        let (r, k) = self.tables[ti].find_cell(id).unwrap();
        let g0 = self.tables[ti].grid_col(r, k);
        let span = self.tables[ti].rows[r].cells[k].span.max(1) as usize;
        if !self.split_cell(2) { return false; }
        // With a single grid column, place the new column boundary where the stroke was drawn.
        let t = &mut self.tables[ti];
        if span == 1 && g0 + 1 < t.grid.len() {
            let total = t.grid[g0] + t.grid[g0 + 1];
            t.grid[g0] = ((total as f32 * fraction.clamp(0.1, 0.9)).round() as i32).max(216);
            t.grid[g0 + 1] = total - t.grid[g0];
            t.sync_cell_widths();
        }
        true
    }

    /// Erase the border between two cells (Draw Table's eraser): merges them.
    pub fn erase_border(&mut self, a: u64, b: u64) -> bool {
        let (Some(ta), Some(tb)) = (self.table_of_cell(a), self.table_of_cell(b)) else { return false };
        if ta != tb { return false; }
        let (sa, _) = self.cell_range(a);
        let (_, eb) = self.cell_range(b);
        let (s, e) = if sa <= eb { (sa, eb) } else { (self.cell_range(b).0, self.cell_range(a).1) };
        self.selection = Selection { anchor: s, focus: e };
        self.merge_cells()
    }

    /// Merge the selected block into one cell, keeping every cell's text (mergeBlock).
    pub fn merge_cells(&mut self) -> bool {
        let Some(b) = self.cell_block() else { return false };
        let ids = self.block_cells(b);
        if ids.len() < 2 { return false; }
        self.checkpoint(EditKind::Other);
        let width: usize = b.c1 - b.c0 + 1;
        let keep = ids[0];
        // Collect non-empty paragraphs of the other cells into the first.
        let mut moved = vec![];
        for id in &ids[1..] {
            let r = self.cell_paragraphs(*id);
            let paras: Vec<Paragraph> = self.paragraphs.drain(r.clone()).collect();
            let has_text = paras.iter().any(|p| !p.is_empty());
            let mut blank = paras[0].clone();
            blank.runs.clear();
            blank.id = new_id();
            self.paragraphs.insert(r.start, blank);
            if has_text { moved.extend(paras.into_iter().filter(|p| !p.is_empty())); }
        }
        let r = self.cell_paragraphs(keep);
        let mut at = r.end;
        if self.paragraphs[r.clone()].iter().all(|p| p.is_empty()) && !moved.is_empty() { self.paragraphs.drain(r.clone()); at = r.start; }
        for mut p in moved { p.cell = Some(keep); self.paragraphs.insert(at, p); at += 1; }
        // Restructure: one cell spanning the columns in the first row, vertical continues below.
        let t = &mut self.tables[b.table];
        let grid_w: i32 = t.grid[b.c0.min(t.grid.len())..(b.c1 + 1).min(t.grid.len())].iter().sum();
        let mut removed = vec![];
        for r in b.r0..=b.r1 {
            let row = &mut t.rows[r];
            let mut at = row.grid_before as usize;
            let mut first: Option<usize> = None;
            let mut k = 0;
            while k < row.cells.len() {
                let span = row.cells[k].span.max(1) as usize;
                if at + span > b.c0 && at <= b.c1 {
                    if first.is_none() {
                        first = Some(k);
                        let c = &mut row.cells[k];
                        c.span = width as u32;
                        c.width = grid_w;
                        c.v_merge = if b.r0 == b.r1 { VMerge::None } else if r == b.r0 { VMerge::Restart } else { VMerge::Continue };
                        if r > b.r0 { removed.push(c.id); }
                    } else {
                        removed.push(row.cells[k].id);
                        row.cells.remove(k);
                        at += span;
                        continue;
                    }
                }
                at += span;
                k += 1;
            }
        }
        // Continuation cells keep a single empty paragraph.
        let first_ids: Vec<u64> = t.rows[b.r0 + 1..=b.r1].iter().filter_map(|row| {
            let mut at = row.grid_before as usize;
            row.cells.iter().find(|c| { let hit = at <= b.c0 && b.c0 < at + c.span.max(1) as usize; at += c.span.max(1) as usize; hit }).map(|c| c.id)
        }).collect();
        let gone: Vec<u64> = removed.into_iter().filter(|id| !first_ids.contains(id)).collect();
        self.paragraphs.retain(|p| !p.cell.is_some_and(|c| gone.contains(&c)));
        for id in first_ids {
            let r = self.cell_paragraphs(id);
            if r.len() > 1 { self.paragraphs.drain(r.start + 1..r.end); }
            if let Some(p) = self.paragraphs.get_mut(r.start) { p.runs.clear(); }
        }
        let (s, _) = self.cell_range(keep);
        self.selection = Selection { anchor: s, focus: s };
        self.changed();
        true
    }

    /// Split the current cell into `columns` cells (and undo a vertical merge) (splitCells).
    pub fn split_cell(&mut self, columns: usize) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(ti) = self.table_of_cell(id) else { return false };
        let (r, k) = self.tables[ti].find_cell(id).unwrap();
        let cell = self.tables[ti].rows[r].cells[k].clone();
        let columns = columns.max(1);
        if columns == 1 && cell.v_merge == VMerge::None && cell.span <= 1 { return false; }
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[ti];
        let g0 = t.grid_col(r, k);
        // Undo a vertical merge: continuation cells become ordinary empty cells.
        if cell.v_merge == VMerge::Restart {
            for row in &mut t.rows[r + 1..] {
                let mut at = row.grid_before as usize;
                let mut hit = false;
                for c in &mut row.cells { if at == g0 && c.v_merge == VMerge::Continue { c.v_merge = VMerge::None; hit = true; } at += c.span.max(1) as usize; }
                if !hit { break; }
            }
            t.rows[r].cells[k].v_merge = VMerge::None;
        }
        let span = cell.span.max(1) as usize;
        let mut new_cells = vec![];
        if columns > 1 {
            if span >= columns {
                // Split along existing grid columns.
                let per = span / columns;
                let mut spans = vec![per as u32; columns];
                spans[columns - 1] += (span - per * columns) as u32;
                t.rows[r].cells[k].span = spans[0];
                for (n, sp) in spans.iter().enumerate().skip(1) { let mut c = Cell::new(0); c.span = *sp; c.shading = cell.shading; let _ = n; new_cells.push(c); }
            } else {
                // Divide the grid column; other rows' cells widen over the new columns.
                let total: i32 = t.grid[g0..g0 + span].iter().sum();
                let extra = columns - span;
                let w = total / columns as i32;
                t.grid.splice(g0..g0 + span, std::iter::repeat_n(w, columns));
                for (ri, row) in t.rows.iter_mut().enumerate() {
                    if ri == r { continue; }
                    let mut at = row.grid_before as usize;
                    for c in &mut row.cells { let s = c.span.max(1) as usize; if at <= g0 && g0 < at + s { c.span += extra as u32; } at += s; }
                }
                t.rows[r].cells[k].span = 1;
                for _ in 1..columns { let mut c = Cell::new(w); c.shading = cell.shading; new_cells.push(c); }
            }
            let ins: Vec<Cell> = new_cells.clone();
            t.rows[r].cells.splice(k + 1..k + 1, ins);
        }
        t.sync_cell_widths();
        let range = self.cell_paragraphs(id);
        let template = self.paragraphs[range.start].sibling();
        let mut at = range.end;
        for c in &new_cells { let mut p = template.clone(); p.id = new_id(); p.cell = Some(c.id); self.paragraphs.insert(at, p); at += 1; }
        self.remove_orphans_for_table(ti);
        self.changed();
        true
    }

    /// Cells without paragraphs get one (after structure edits).
    fn remove_orphans_for_table(&mut self, ti: usize) {
        let ids: Vec<u64> = self.tables[ti].cells().map(|c| c.id).collect();
        let mut last = self.paragraphs.iter().position(|p| p.cell.is_some_and(|c| ids.contains(&c))).unwrap_or(0);
        for id in ids {
            match self.paragraphs.iter().rposition(|p| p.cell == Some(id)) {
                Some(i) => last = i,
                None => { let mut p = Paragraph::default(); p.cell = Some(id); p.props.space_after = Some(0.0); self.paragraphs.insert(last + 1, p); last += 1; }
            }
        }
    }

    /// Split the table above the current row, with an empty paragraph between (splitTable).
    pub fn split_table(&mut self) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(ti) = self.table_of_cell(id) else { return false };
        let (r, _) = self.tables[ti].find_cell(id).unwrap();
        if r == 0 { return false; }
        self.checkpoint(EditKind::Other);
        let mut second = self.tables[ti].clone();
        second.id = new_id();
        second.rows = self.tables[ti].rows.split_off(r);
        for c in second.rows[0].cells.iter_mut() { if c.v_merge == VMerge::Continue { c.v_merge = VMerge::Restart; } }
        let first_of_second = second.rows[0].cells[0].id;
        let at = self.cell_paragraphs(first_of_second).start;
        let mut gap = Paragraph::default();
        gap.style = self.paragraphs[at].style.clone();
        self.paragraphs.insert(at, gap);
        self.tables.insert(ti + 1, second);
        self.select(self.paragraph_start(at), self.paragraph_start(at));
        self.changed();
        true
    }

    /// Row heights for the selected rows: (twips, exact); 0 = automatic.
    pub fn set_row_height(&mut self, twips: i32, exact: bool) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        for r in &mut self.tables[b.table].rows[b.r0..=b.r1] { r.min_height = twips.max(0); r.exact = exact && twips > 0; }
        self.changed();
    }
    pub fn set_header_rows(&mut self, on: bool) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[b.table];
        // Header rows are a run from the top: marking row k marks every row above it.
        for (i, r) in t.rows.iter_mut().enumerate() { if i <= b.r1 { r.header = on; } }
        self.changed();
    }
    pub fn set_rows_cant_split(&mut self, on: bool) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        for r in &mut self.tables[b.table].rows[b.r0..=b.r1] { r.cant_split = on; }
        self.changed();
    }

    /// Give selected columns (or every column) equal widths (distributeColumns).
    pub fn distribute_columns(&mut self) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[b.table];
        let (c0, c1) = if b.c0 == b.c1 { (0, t.grid.len() - 1) } else { (b.c0, b.c1.min(t.grid.len() - 1)) };
        let total: i32 = t.grid[c0..=c1].iter().sum();
        let n = (c1 - c0 + 1) as i32;
        for (i, g) in t.grid[c0..=c1].iter_mut().enumerate() { *g = total / n + if (i as i32) < total % n { 1 } else { 0 }; }
        t.sync_cell_widths();
        self.changed();
    }
    /// Give selected rows (or all rows) the same minimum height (distributeRows).
    pub fn distribute_rows(&mut self, heights: &[f32]) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[b.table];
        let (r0, r1) = if b.r0 == b.r1 { (0, t.rows.len() - 1) } else { (b.r0, b.r1) };
        let avg = heights.get(r0..=r1).map(|h| h.iter().sum::<f32>() / (r1 - r0 + 1) as f32).unwrap_or(0.0);
        let tw = (avg * 15.0).round() as i32;
        for r in &mut t.rows[r0..=r1] { r.min_height = tw; r.exact = false; }
        self.changed();
    }

    /// Set one grid column's width, taking the difference from its right neighbor (border drag).
    pub fn set_column_width(&mut self, table: usize, col: usize, twips: i32, keep_total: bool) {
        let t = &self.tables[table];
        if col >= t.grid.len() { return; }
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[table];
        let min = 216; // 0.15"
        let old = t.grid[col];
        let mut new = twips.max(min);
        if keep_total && col + 1 < t.grid.len() {
            let pair = old + t.grid[col + 1];
            new = new.min(pair - min);
            t.grid[col + 1] = pair - new;
        }
        t.grid[col] = new;
        t.sync_cell_widths();
        self.changed();
    }

    /// AutoFit: to the writing width, to the contents (natural text widths), or fixed.
    pub fn autofit(&mut self, mode: AutoFit) {
        let Some(b) = self.cell_block() else { return };
        let writing = self.page.width - self.page.left - self.page.right - self.page.gutter;
        self.checkpoint(EditKind::Other);
        self.tables[b.table].fixed = mode == AutoFit::Fixed;
        self.tables[b.table].raw_props = None;
        match mode {
            AutoFit::Fixed => {}
            AutoFit::Window => {
                let t = &mut self.tables[b.table];
                let total = t.width().max(1);
                let mut acc = 0;
                let n = t.grid.len();
                for (i, g) in t.grid.iter_mut().enumerate() {
                    *g = if i + 1 == n { writing - acc } else { (*g as i64 * writing as i64 / total as i64) as i32 };
                    acc += *g;
                }
                t.indent = 0;
            }
            AutoFit::Contents => {
                // Each column fits its widest unwrapped paragraph plus cell margins, within the page.
                let t = &self.tables[b.table];
                let mut need = vec![360i32; t.grid.len()];
                for (ri, row) in t.rows.iter().enumerate() {
                    let mut at = row.grid_before as usize;
                    for c in &row.cells {
                        let span = c.span.max(1) as usize;
                        if span == 1 && at < need.len() {
                            let mut widest = 0.0f32;
                            for p in self.paragraphs.iter().filter(|p| p.cell == Some(c.id)) {
                                let mut w = 0.0;
                                for r in &p.runs {
                                    let f = self.run_format(p, r);
                                    let key = crate::fonts::resolve(&f.font, f.bold, f.italic);
                                    w += r.text.chars().map(|ch| crate::fonts::advance(key, ch, f.size * 4.0 / 3.0)).sum::<f32>();
                                }
                                widest = widest.max(w);
                            }
                            need[at] = need[at].max((widest * 15.0).ceil() as i32 + 2 * 108 + 30);
                        }
                        let _ = ri;
                        at += span;
                    }
                }
                let total: i32 = need.iter().sum();
                if total > writing { for g in &mut need { *g = (*g as i64 * writing as i64 / total as i64) as i32; } }
                let t = &mut self.tables[b.table];
                t.grid = need;
            }
        }
        self.tables[b.table].sync_cell_widths();
        self.changed();
    }

    /// Borders on every cell edge (Table Grid) or none.
    pub fn set_table_borders(&mut self, on: bool) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        let t = &mut self.tables[b.table];
        t.borders = on;
        if !on && t.style.as_deref() == Some("TableGrid") { t.style = None; }
        if on && t.style.is_none() && self.styles.get("TableGrid").is_some() { t.style = Some("TableGrid".into()); }
        // Direct tblPr is regenerated from the model.
        t.raw_props = None;
        self.changed();
    }

    /// Quick table looks applied as direct cell formatting (round-trips anywhere).
    pub fn apply_table_style(&mut self, style: TableStyle, accent: Color) {
        let Some(b) = self.cell_block() else { return };
        self.checkpoint(EditKind::Other);
        let light = Color(((accent.0 as u16 + 255 * 4) / 5) as u8, ((accent.1 as u16 + 255 * 4) / 5) as u8, ((accent.2 as u16 + 255 * 4) / 5) as u8);
        let t = &mut self.tables[b.table];
        t.borders = style != TableStyle::Plain;
        t.raw_props = None;
        let mut header_cells = vec![];
        for (ri, row) in t.rows.iter_mut().enumerate() {
            for c in &mut row.cells {
                c.shading = match style {
                    TableStyle::Plain | TableStyle::Grid => None,
                    TableStyle::HeaderShaded => if ri == 0 { Some(accent) } else { None },
                    TableStyle::Banded => if ri == 0 { Some(accent) } else if ri % 2 == 0 { Some(light) } else { None },
                };
                if ri == 0 { header_cells.push(c.id); }
            }
            if ri == 0 { row.header = matches!(style, TableStyle::HeaderShaded | TableStyle::Banded) || row.header; }
        }
        // Header text: white and bold on a dark fill.
        let header = matches!(style, TableStyle::HeaderShaded | TableStyle::Banded);
        for p in &mut self.paragraphs {
            if p.cell.is_some_and(|c| header_cells.contains(&c)) {
                for r in &mut p.runs {
                    r.props.bold = header.then_some(true);
                    r.props.color = header.then_some(Some(Color::WHITE));
                }
                p.mark.bold = header.then_some(true);
                p.mark.color = header.then_some(Some(Color::WHITE));
            }
        }
        self.changed();
    }

    /// Sort body rows by a grid column's text (numbers numerically) (sortTable).
    pub fn sort_table(&mut self, col: usize, descending: bool, header: bool) -> bool {
        let Some(b) = self.cell_block() else { return false };
        let t = self.tables[b.table].clone();
        if t.rows.iter().any(|r| r.cells.iter().any(|c| c.v_merge != VMerge::None)) { return false; }
        let skip = if header { 1 } else { t.rows.iter().take_while(|r| r.header).count() };
        if t.rows.len() <= skip + 1 { return false; }
        self.checkpoint(EditKind::Other);
        let key = |r: &Row| -> String {
            let mut at = r.grid_before as usize;
            let id = r.cells.iter().find(|c| { let hit = at <= col && col < at + c.span.max(1) as usize; at += c.span.max(1) as usize; hit }).map(|c| c.id);
            id.map(|id| self.paragraphs.iter().filter(|p| p.cell == Some(id)).map(|p| p.text()).collect::<Vec<_>>().join(" ")).unwrap_or_default()
        };
        let mut body: Vec<(String, Row)> = t.rows[skip..].iter().map(|r| (key(r), r.clone())).collect();
        let num = |s: &str| s.trim().replace([',', '$', '%', '€', '£'], "").parse::<f64>().ok();
        body.sort_by(|a, b| {
            let o = match (num(&a.0), num(&b.0)) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                // Empty cells sort last; text compares case-insensitively.
                _ => (a.0.trim().is_empty(), a.0.to_lowercase()).cmp(&(b.0.trim().is_empty(), b.0.to_lowercase())),
            };
            if descending { o.reverse() } else { o }
        });
        let mut rows: Vec<Row> = t.rows[..skip].to_vec();
        rows.extend(body.into_iter().map(|(_, r)| r));
        // Reorder paragraphs to follow the new row order.
        let order: Vec<u64> = rows.iter().flat_map(|r| r.cells.iter().map(|c| c.id)).collect();
        let first = self.paragraphs.iter().position(|p| p.cell.is_some_and(|c| order.contains(&c))).unwrap();
        let count = self.paragraphs.iter().filter(|p| p.cell.is_some_and(|c| order.contains(&c))).count();
        let mut block: Vec<Paragraph> = self.paragraphs.drain(first..first + count).collect();
        block.sort_by_key(|p| order.iter().position(|c| Some(*c) == p.cell).unwrap_or(usize::MAX));
        self.paragraphs.splice(first..first, block);
        self.tables[b.table].rows = rows;
        let pos = self.paragraph_start(first);
        self.selection = Selection { anchor: pos, focus: pos };
        self.changed();
        true
    }

    /// Turn the selected paragraphs into a table, splitting each at the separator (convertTextToTable).
    pub fn convert_text_to_table(&mut self, sep: Separator) -> bool {
        let indices: Vec<usize> = self.selected_paragraph_indices().collect();
        if indices.iter().any(|&i| self.paragraphs[i].cell.is_some()) { return false; }
        let split = |p: &Paragraph| -> Vec<String> {
            let t = p.text();
            match sep { Separator::Tab => t.split('\t').map(String::from).collect(), Separator::Comma => t.split(',').map(|x| x.trim().to_string()).collect(), Separator::Paragraph => vec![t] }
        };
        let rows: Vec<Vec<String>> = match sep {
            Separator::Paragraph => indices.iter().map(|&i| split(&self.paragraphs[i])).collect(),
            _ => indices.iter().map(|&i| split(&self.paragraphs[i])).collect(),
        };
        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
        self.checkpoint(EditKind::Other);
        let writing = self.page.width - self.page.left - self.page.right - self.page.gutter;
        let w = writing / cols as i32;
        let mut table = Table { id: new_id(), grid: vec![w; cols], rows: vec![], borders: true, style: self.styles.get("TableGrid").map(|_| "TableGrid".into()), indent: 0, alignment: Alignment::Left, cell_margins: DEFAULT_CELL_MARGINS, raw_props: None, parent: None, fixed: false };
        let mut paragraphs = vec![];
        for (k, values) in rows.iter().enumerate() {
            let template = &self.paragraphs[indices[k]];
            let run = template.runs.first().map(|r| Run { text: String::new(), props: r.props.clone(), style: r.style.clone(), ..Default::default() }).unwrap_or_default();
            let mut row = Row { cells: vec![], min_height: 0, exact: false, header: false, cant_split: false, grid_before: 0, grid_after: 0, extra: vec![] };
            for c in 0..cols {
                let cell = Cell::new(w);
                let mut p = template.sibling();
                p.props.numbering = None;
                p.props.space_after = Some(0.0);
                p.cell = Some(cell.id);
                if let Some(v) = values.get(c).filter(|v| !v.is_empty()) { p.runs.push(Run { text: v.clone(), ..run.clone() }); }
                paragraphs.push(p);
                row.cells.push(cell);
            }
            table.rows.push(row);
        }
        let (first, last) = (indices[0], *indices.last().unwrap());
        let mut after = self.paragraphs[last].sibling();
        after.runs.clear();
        let needs_after = last + 1 >= self.paragraphs.len() || self.paragraphs[last + 1].cell.is_some();
        self.paragraphs.splice(first..=last, paragraphs);
        if needs_after { let at = first + rows.len() * cols; self.paragraphs.insert(at, after); }
        self.tables.push(table);
        self.sort_tables();
        let pos = self.paragraph_start(first);
        self.selection = Selection { anchor: pos, focus: pos };
        self.changed();
        true
    }

    /// Replace the current table with paragraphs, one per row, cells joined by the separator.
    pub fn convert_table_to_text(&mut self, sep: Separator) -> bool {
        let Some(id) = self.current_cell() else { return false };
        let Some(ti) = self.table_of_cell(id) else { return false };
        self.checkpoint(EditKind::Other);
        let t = self.tables.remove(ti);
        let first = self.paragraphs.iter().position(|p| p.cell.is_some_and(|c| t.has_cell(c))).unwrap();
        let mut out = vec![];
        for row in &t.rows {
            let mut line = Paragraph::default();
            let mut started = false;
            for c in row.cells.iter().filter(|c| c.v_merge != VMerge::Continue) {
                let paras: Vec<&Paragraph> = self.paragraphs.iter().filter(|p| p.cell == Some(c.id)).collect();
                if sep == Separator::Paragraph {
                    for p in paras { let mut q = p.clone(); q.cell = None; q.id = new_id(); out.push(q); }
                    continue;
                }
                if started { line.runs.push(Run::text(if sep == Separator::Tab { "\t" } else { ", " })); }
                started = true;
                for (k, p) in paras.iter().enumerate() {
                    if k > 0 { line.runs.push(Run::text(" ")); }
                    if k == 0 && line.style.is_none() { line.style = p.style.clone(); }
                    line.runs.extend(p.runs.iter().cloned());
                }
            }
            if sep != Separator::Paragraph { line.normalize_public(); out.push(line); }
        }
        self.paragraphs.retain(|p| !p.cell.is_some_and(|c| t.has_cell(c)));
        let n = out.len();
        self.paragraphs.splice(first..first, out);
        let s = self.paragraph_start(first);
        let e = self.paragraph_start(first + n - 1) + self.paragraphs[first + n - 1].len();
        self.selection = Selection { anchor: s, focus: e };
        self.changed();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: usize, cols: usize) -> Document {
        let mut d = Document::new(vec![Paragraph::new("x")]);
        d.insert_table(rows, cols);
        for r in 0..rows { for c in 0..cols { d.replace(&format!("{r}{c}")); if r + 1 < rows || c + 1 < cols { d.move_cell(true); } } }
        d
    }
    fn cell_text(d: &Document, r: usize, c: usize) -> String {
        let t = &d.tables[0];
        let (_, cell) = t.cell_at_grid(r, c).unwrap();
        d.paragraphs.iter().filter(|p| p.cell == Some(cell.id)).map(|p| p.text()).collect::<Vec<_>>().join("|")
    }

    #[test]
    fn block_merge_and_split() {
        let mut d = grid(3, 3);
        let a = d.paragraph_start(d.paragraphs.iter().position(|p| p.text() == "00").unwrap());
        let b = d.paragraph_start(d.paragraphs.iter().position(|p| p.text() == "11").unwrap()) + 1;
        d.select(a, b);
        let block = d.cell_block().unwrap();
        assert_eq!((block.r0, block.r1, block.c0, block.c1), (0, 1, 0, 1));
        assert!(d.merge_cells());
        let t = &d.tables[0];
        assert_eq!(t.rows[0].cells.len(), 2);
        assert_eq!((t.rows[0].cells[0].span, t.rows[0].cells[0].v_merge), (2, VMerge::Restart));
        assert_eq!(t.rows[1].cells[0].v_merge, VMerge::Continue);
        assert_eq!(cell_text(&d, 0, 0), "00|01|10|11", "merged text is kept");
        assert_eq!(cell_text(&d, 1, 2), "12");
        // The merged cell's four lines spread over both rows: row 0 stays one line high.
        let l = crate::layout_document(&d);
        let h = |r: usize, c: usize| l.cells.iter().find(|b| b.row == r && b.col == c).map(|b| b.y1 - b.y0).unwrap();
        assert!(h(0, 2) < h(1, 2), "the last merged row takes the extra height ({} vs {})", h(0, 2), h(1, 2));
        // Splitting restores separate cells.
        assert!(d.split_cell(2));
        assert_eq!(d.tables[0].rows[0].cells.len(), 3);
        assert_eq!(d.tables[0].rows[1].cells[0].v_merge, VMerge::None);
        d.undo(); d.undo();
        assert_eq!(cell_text(&d, 1, 1), "11");
    }

    #[test]
    fn select_parts_sort_and_sizes() {
        let mut d = grid(3, 2);
        // Put the caret in "21" and select its column.
        let p = d.paragraph_start(d.paragraphs.iter().position(|p| p.text() == "21").unwrap());
        d.select(p, p);
        d.select_table_part(TablePart::Column);
        assert_eq!(d.selected_text(), "01\n10\n11\n20\n21", "a column block spans its rows");
        d.select(p, p);
        assert!(d.sort_table(0, true, true));
        assert_eq!((cell_text(&d, 0, 0), cell_text(&d, 1, 0), cell_text(&d, 2, 0)), ("00".into(), "20".into(), "10".into()));
        d.distribute_columns();
        d.autofit(AutoFit::Window);
        let p = d.paragraph_start(d.paragraphs.iter().position(|p| p.text() == "10").unwrap());
        d.select(p, p);
        let writing = d.page.width - d.page.left - d.page.right;
        assert_eq!(d.tables[0].grid.iter().sum::<i32>(), writing);
        d.set_column_width(0, 0, 2000, true);
        assert_eq!(d.tables[0].grid[0], 2000);
        assert_eq!(d.tables[0].grid.iter().sum::<i32>(), writing, "the neighbor absorbs the change");
        d.autofit(AutoFit::Contents);
        assert!(d.tables[0].grid.iter().sum::<i32>() < writing / 2);
        assert!(d.split_table());
        assert_eq!(d.tables.len(), 2);
        crate::layout_document(&d);
    }

    #[test]
    fn borders_margins_and_cell_shifts() {
        let mut d = grid(2, 3);
        let p = d.paragraph_start(d.paragraphs.iter().position(|p| p.text() == "00").unwrap());
        let q = d.paragraph_start(d.paragraphs.iter().position(|p| p.text() == "11").unwrap());
        d.select(p, q);
        let pen = Border { width: 1.5, space: 0.0, color: Some(Color(200, 0, 0)) };
        d.apply_border_preset(BorderPreset::Outside, pen);
        let t = &d.tables[0];
        assert_eq!(t.rows[0].cells[0].borders.top, Some(pen));
        assert_eq!(t.rows[0].cells[0].borders.right, None, "inside edges keep inheriting");
        assert_eq!(t.rows[1].cells[1].borders.right, Some(pen));
        d.set_cell_margins(Some([100, 200, 100, 200]));
        d.set_table_cell_margins([0, 50, 0, 50]);
        // Margins and borders round-trip.
        let back = crate::docx::read(&crate::docx::write(&d).unwrap()).unwrap();
        assert_eq!(back.tables[0].rows[0].cells[0].borders.top.map(|b| b.width), Some(1.5));
        assert_eq!(back.tables[0].rows[0].cells[0].margins, Some([100, 200, 100, 200]));
        assert_eq!(back.tables[0].cell_margins, [0, 50, 0, 50]);
        // Insert a cell (shift right) and delete it (shift left).
        d.select(p, p);
        assert!(d.insert_cells());
        assert_eq!(d.tables[0].rows[0].cells.len(), 4);
        assert_eq!(d.tables[0].rows[1].grid_after, 1);
        crate::layout_document(&d);
        assert!(d.delete_cell());
        assert_eq!(d.tables[0].rows[0].cells.len(), 3);
        assert_eq!(cell_text(&d, 0, 0), "00");
        // Draw-table strokes: a vertical split and a horizontal split, then erase a border.
        let p = d.paragraph_start(d.paragraphs.iter().position(|p| p.text() == "12").unwrap());
        d.select(p, p);
        assert!(d.split_cell_at(0.3));
        assert!(d.split_cell_rows());
        assert_eq!(d.tables[0].rows.len(), 3);
        crate::layout_document(&d);
        let ids: Vec<u64> = d.tables[0].rows[0].cells.iter().map(|c| c.id).collect();
        assert!(d.erase_border(ids[0], ids[1]));
        assert_eq!(cell_text(&d, 0, 0), "00|01");
    }

    #[test]
    fn convert_both_ways() {
        let mut d = Document::new(vec![Paragraph::new("a\tb\tc"), Paragraph::new("1\t2"), Paragraph::new("after")]);
        d.select(0, 9);
        assert!(d.convert_text_to_table(Separator::Tab));
        assert_eq!(d.tables[0].rows.len(), 2);
        assert_eq!(cell_text(&d, 1, 1), "2");
        assert_eq!(d.paragraphs.last().unwrap().text(), "after");
        assert!(d.convert_table_to_text(Separator::Comma));
        assert_eq!(d.text(), "a, b, c\n1, 2, \nafter");
        assert!(d.tables.is_empty());
    }
}

#[cfg(test)]
mod nesting_tests {
    use crate::model::*;

    #[test]
    fn nested_tables_lay_out_inside_their_cell_and_round_trip() {
        let mut d = Document::new(vec![Paragraph::new("Before")]);
        d.select(6, 6);
        d.insert_table(2, 3);
        // Caret is in the first cell: insert a 2×2 table there.
        d.replace("Outer");
        d.insert_table(2, 2);
        d.replace("Inner");
        assert_eq!(d.tables.len(), 2);
        let inner = d.tables.iter().position(|t| t.parent.is_some()).unwrap();
        let outer = 1 - inner;
        let outer_first = d.tables[outer].rows[0].cells[0].id;
        assert_eq!(d.tables[inner].parent, Some(outer_first));
        // Layout: the inner table's cells sit inside the outer first cell.
        let l = crate::layout_document(&d);
        let ob = l.cells.iter().find(|c| c.cell == outer_first).unwrap();
        let inner_ids: Vec<u64> = d.tables[inner].cells().map(|c| c.id).collect();
        let ib: Vec<_> = l.cells.iter().filter(|c| inner_ids.contains(&c.cell)).collect();
        assert_eq!(ib.len(), 4);
        for c in &ib { assert!(c.x0 >= ob.x0 - 0.5 && c.x1 <= ob.x1 + 0.5 && c.y0 >= ob.y0 - 0.5 && c.y1 <= ob.y1 + 0.5, "inner cell inside outer cell"); }
        let text: String = l.pages[0].items.iter().filter_map(|i| match i { crate::layout::DrawItem::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| g.ch).collect::<String>()), _ => None }).collect();
        assert!(text.contains("Inner") && text.contains("Outer"));
        // Saved as w:tbl inside w:tc and read back nested.
        let e = crate::docx::read(&crate::docx::write(&d).unwrap()).unwrap();
        assert_eq!(e.tables.len(), 2);
        assert_eq!(e.tables.iter().filter(|t| t.parent.is_some()).count(), 1);
        assert!(e.text().contains("Inner"));
        // Deleting the outer table takes the inner one with it.
        let p = d.paragraph_start(d.paragraphs.iter().position(|p| p.cell == Some(outer_first)).unwrap());
        d.select(p, p);
        d.delete_table_part(TablePart::Table);
        assert!(d.tables.is_empty(), "{:?}", d.tables.len());
        assert!(d.paragraphs.iter().all(|p| p.cell.is_none()));
    }

    #[test]
    fn autofit_widens_a_column_for_unbreakable_content() {
        let mut d = Document::new(vec![Paragraph::new("x")]);
        d.select(1, 1);
        d.insert_table(1, 6);
        d.insert_object(crate::objects::Object::Equation(crate::equation::Equation::new("(x+a)^n=\\sum_{k=0}^{n}\\binom{n}{k}x^k a^{n-k}")));
        let eq_w = (0..d.len()).find_map(|p| d.object_at(p)).map(|o| o.size().0).unwrap();
        let l = crate::layout_document(&d);
        let first = d.tables[0].rows[0].cells[0].id;
        let c = l.cells.iter().find(|c| c.cell == first).unwrap();
        assert!(c.x1 - c.x0 >= eq_w, "column fits the equation: {} < {eq_w}", c.x1 - c.x0);
        let g = d.page.geometry();
        let right = l.cells.iter().map(|c| c.x1).fold(0.0, f32::max);
        assert!(right <= g.width - g.right + 1.0, "table stays within the margins");
        // Fixed layout keeps the even columns.
        d.select(d.range().0, d.range().0);
        d.autofit(crate::tables::AutoFit::Fixed);
        let l = crate::layout_document(&d);
        let c = l.cells.iter().find(|c| c.cell == first).unwrap();
        assert!(c.x1 - c.x0 < eq_w);
    }
}

#[cfg(test)]
mod delete_move_tests {
    use crate::model::*;

    fn with_table() -> Document {
        let mut d = Document::new(vec![Paragraph::new("Before text"), Paragraph::new("After text")]);
        d.select(11, 11);
        d.insert_table(2, 2);
        d.replace("cell");
        d
    }

    #[test]
    fn deleting_across_a_table_removes_it() {
        let mut d = with_table();
        let end = d.len();
        // From "Before| text" to "After| text".
        d.select(6, end - 5);
        d.delete(false, false);
        assert!(d.tables.is_empty(), "the table goes");
        assert!(d.paragraphs.iter().all(|p| p.cell.is_none()));
        assert_eq!(d.text(), "Before text");
        // Selecting just part of a table's text keeps the table.
        let mut d = with_table();
        d.select(3, 14);
        d.delete(false, false);
        assert_eq!(d.tables.len(), 1);
        // Undo brings it back.
        let mut d = with_table();
        let n = d.len();
        d.select(0, n);
        d.delete(false, false);
        assert!(d.tables.is_empty());
        d.undo();
        assert_eq!(d.tables.len(), 1);
    }

    #[test]
    fn moving_a_nested_table_out_keeps_the_outer_one() {
        let mut d = Document::new(vec![Paragraph::new("Top"), Paragraph::new("Bottom")]);
        d.select(3, 3);
        d.insert_table(2, 3);
        d.insert_table(2, 2);
        d.replace("inner");
        let inner = d.tables.iter().position(|t| t.parent.is_some()).unwrap();
        let outer_id = d.tables.iter().find(|t| t.parent.is_none()).unwrap().id;
        let outer_cells = d.tables.iter().find(|t| t.id == outer_id).unwrap().cells().count();
        // Drag the inner table to the end of the document.
        let end = d.len();
        assert!(d.move_table(inner, end));
        assert_eq!(d.tables.len(), 2);
        assert!(d.tables.iter().all(|t| t.parent.is_none()), "both are top-level now");
        let outer = d.tables.iter().find(|t| t.id == outer_id).unwrap();
        assert_eq!(outer.cells().count(), outer_cells, "the outer table is unchanged");
        assert!(outer.cells().all(|c| d.paragraphs.iter().any(|p| p.cell == Some(c.id))));
        let l = crate::layout_document(&d);
        let inner_t = d.tables.iter().find(|t| t.id != outer_id).unwrap();
        let ib = l.cells.iter().filter(|c| inner_t.has_cell(c.cell)).map(|c| c.y0).fold(f32::MAX, f32::min);
        let ob = l.cells.iter().filter(|c| outer.has_cell(c.cell)).map(|c| c.y1).fold(0.0, f32::max);
        assert!(ib >= ob, "the moved table lays out below the outer one");
        assert!(crate::docx::read(&crate::docx::write(&d).unwrap()).unwrap().tables.iter().all(|t| t.parent.is_none()));
    }
}
