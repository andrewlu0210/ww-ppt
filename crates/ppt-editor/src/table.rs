//! Table editing: cell text, Tab navigation, grid lines, rows and columns.

use std::sync::Arc;

use ppt_geometry::text_width_pt;
use ppt_model::table::{TableCell, TableFlags, TableRow};
use ppt_model::{Anchor, EMU_PER_INCH, ParaDefaults, Shape, ShapeId, ShapeKind, Table, TextBody, Xfrm};
use ppt_text::TextEngine;

use crate::{CursorIcon, Drag, DragKind, Editor, PointerEvent, RichEdit, TextEdit, Tolerance};

/// Smallest column width / row height a drag can produce (0.1").
const MIN_TRACK: i64 = EMU_PER_INCH as i64 / 10;
/// Default cell margins, matching the parser.
const MARGINS: [i64; 4] = [91_440, 45_720, 91_440, 45_720];
/// Row height of a new table (0.4").
const NEW_ROW_HEIGHT: i64 = 370_840;

/// Where a point falls on a table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TableHit {
    Cell(usize, usize),
    /// The grid line after column / row `i` (inner lines only).
    ColLine(usize),
    RowLine(usize),
    /// Near the outer edge: drags move the table.
    Frame,
}

/// Scales column widths and row heights from `orig` (for resizing the frame).
pub(crate) fn scale_grid(t: &mut Table, orig: &Table, sx: f64, sy: f64) {
    for (w, o) in t.cols.iter_mut().zip(&orig.cols) {
        *w = ((*o as f64) * sx).round().max(1.0) as i64;
    }
    for (row, o) in t.rows.iter_mut().zip(&orig.rows) {
        row.height = ((o.height as f64) * sy).round().max(1.0) as i64;
    }
}

/// Text body for a new cell at (`row`, `col`): presentation defaults with the
/// table style's text formatting for that position.
fn cell_body(pres_defaults: Option<&Arc<Vec<ParaDefaults>>>, base: &ppt_model::RunStyle, table: &Table, row: usize, col: usize) -> TextBody {
    let mut style = ppt_model::RunStyle { size_pt: 18.0, ..base.clone() };
    for part in table.parts_for(row, col) {
        style.bold = part.bold.unwrap_or(style.bold);
        style.italic = part.italic.unwrap_or(style.italic);
        style.color = part.color.unwrap_or(style.color);
    }
    let defaults = pres_defaults.map(|d| {
        Arc::new(d.iter().map(|p| ParaDefaults { style: ppt_model::RunStyle { size_pt: p.style.size_pt, ..style.clone() }, ..p.clone() }).collect())
    });
    let mut body = TextBody { insets: MARGINS, anchor: Anchor::Top, wrap: true, defaults, ..Default::default() };
    body.paragraphs[0].end_style = style;
    body
}

/// An empty copy of `cell` (same borders and fill), for inserted rows/columns.
fn blank_like(cell: &TableCell) -> TableCell {
    let mut text = cell.text.clone();
    let end_style = text.paragraphs.first().map(|p| p.first_style().clone()).unwrap_or_default();
    let first = text.paragraphs.first().cloned().unwrap_or_default();
    text.paragraphs = vec![ppt_model::Paragraph { runs: Vec::new(), end_style, ..first }];
    TableCell { text, grid_span: 1, row_span: 1, h_merge: false, v_merge: false, ..cell.clone() }
}

fn table_of(shape: &Shape) -> Option<&Table> {
    match &shape.kind {
        ShapeKind::Table(t) => Some(t),
        _ => None,
    }
}

fn table_of_mut(shape: &mut Shape) -> Option<&mut Table> {
    match &mut shape.kind {
        ShapeKind::Table(t) => Some(t),
        _ => None,
    }
}

impl Editor {
    /// Recomputes the laid-out row heights of a table and keeps its frame in sync.
    fn layout_table(&mut self, engine: &mut TextEngine, id: ShapeId) -> Vec<i64> {
        let Some(shape) = self.pres.shape_mut(self.slide, id) else { return Vec::new() };
        let Some(table) = table_of(shape) else { return Vec::new() };
        let heights = engine.table_row_heights(table);
        let width = table.cols.iter().sum();
        shape.xfrm.h = heights.iter().sum();
        shape.xfrm.w = width;
        self.table_heights.insert(id, heights.clone());
        heights
    }

    /// Refreshes the cached row heights of every table on the current slide.
    pub fn refresh_tables(&mut self, engine: &mut TextEngine) {
        self.table_heights.clear();
        let Some(slide) = self.pres.slides.get(self.slide) else { return };
        for shape in &slide.shapes {
            if let Some(t) = table_of(shape) {
                self.table_heights.insert(shape.id, engine.table_row_heights(t));
            }
        }
    }

    fn heights_of(&self, id: ShapeId, table: &Table) -> Vec<i64> {
        self.table_heights.get(&id).filter(|h| h.len() == table.rows.len()).cloned().unwrap_or_else(|| table.rows.iter().map(|r| r.height).collect())
    }

    /// What part of the table `id` is under the point.
    pub(crate) fn table_hit(&self, id: ShapeId, x: f64, y: f64, tol: f64) -> Option<TableHit> {
        let shape = self.pres.shape(self.slide, id)?;
        let table = table_of(shape)?;
        let heights = self.heights_of(id, table);
        let (lx, ly) = shape.xfrm.to_local(x, y);
        let (w, h) = (table.cols.iter().sum::<i64>() as f64, heights.iter().sum::<i64>() as f64);
        if lx < -tol / 2.0 || ly < -tol / 2.0 || lx > w + tol / 2.0 || ly > h + tol / 2.0 {
            return None;
        }
        let edge = tol / 2.0;
        if lx < edge || ly < edge || lx > w - edge || ly > h - edge {
            return Some(TableHit::Frame);
        }
        let cols = table.col_offsets();
        let mut rows = vec![0];
        for rh in &heights {
            rows.push(rows.last().unwrap() + rh);
        }
        let col = cols.iter().rposition(|&cx| cx as f64 <= lx).unwrap_or(0).min(table.cols.len() - 1);
        let row = rows.iter().rposition(|&ry| ry as f64 <= ly).unwrap_or(0).min(heights.len().saturating_sub(1));
        // Inner grid lines, except where a merged cell spans across them.
        for (i, &cx) in cols.iter().enumerate().take(cols.len() - 1).skip(1) {
            if (cx as f64 - lx).abs() <= edge && table.anchor_of(row, i - 1) != table.anchor_of(row, i) {
                return Some(TableHit::ColLine(i - 1));
            }
        }
        for (i, &ry) in rows.iter().enumerate().take(rows.len() - 1).skip(1) {
            if (ry as f64 - ly).abs() <= edge && table.anchor_of(i - 1, col) != table.anchor_of(i, col) {
                return Some(TableHit::RowLine(i - 1));
            }
        }
        let (r, c) = table.anchor_of(row, col);
        Some(TableHit::Cell(r, c))
    }

    /// Cursor over a table: resize arrows on inner grid lines, I-beam in cells.
    pub(crate) fn table_cursor(&self, ev: &PointerEvent, tol: &Tolerance) -> Option<CursorIcon> {
        let id = self.hit_test(ev.x, ev.y, tol.handle)?;
        match self.table_hit(id, ev.x, ev.y, tol.handle)? {
            TableHit::Cell(..) => Some(CursorIcon::Text),
            TableHit::ColLine(_) => Some(CursorIcon::Resize(0.0)),
            TableHit::RowLine(_) => Some(CursorIcon::Resize(90.0)),
            TableHit::Frame => Some(CursorIcon::Move),
        }
    }

    /// Clicks on a table: into a cell starts editing it; on a grid line starts a
    /// resize. Returns false to let the click select/move the table as a shape.
    pub(crate) fn table_pointer_down(&mut self, engine: &mut TextEngine, ev: &PointerEvent, tol: &Tolerance) -> bool {
        let Some(id) = self.hit_test(ev.x, ev.y, tol.handle) else { return false };
        if self.pres.shape(self.slide, id).and_then(table_of).is_none() {
            return false;
        }
        if !self.table_heights.contains_key(&id) {
            self.layout_table(engine, id);
        }
        match self.table_hit(id, ev.x, ev.y, tol.handle) {
            Some(TableHit::Cell(r, c)) => {
                self.begin_cell_edit(engine, id, r, c);
                if let Some((x, y)) = self.to_text_space(ev.x, ev.y)
                    && let Some(edit) = &mut self.text
                {
                    edit.rich.pointer_down(x, y, ev.clicks, false);
                }
                self.drag = Some(Drag { kind: DragKind::TextSelect, start: (ev.x, ev.y), origs: Vec::new(), base_selection: Vec::new(), before: None, moved: false });
                true
            }
            Some(hit @ (TableHit::ColLine(i) | TableHit::RowLine(i))) => {
                self.selection = vec![id];
                let kind = if matches!(hit, TableHit::ColLine(_)) { DragKind::TableCol(i) } else { DragKind::TableRow(i) };
                let xfrm = self.pres.shape(self.slide, id).map(|s| s.xfrm).unwrap_or_default();
                self.drag = Some(Drag { kind, start: (ev.x, ev.y), origs: vec![(id, xfrm)], base_selection: Vec::new(), before: Some(self.snapshot()), moved: false });
                true
            }
            _ => false,
        }
    }

    /// Moves an inner grid line by `delta` EMU from where the drag started.
    pub(crate) fn drag_table_border(&mut self, engine: &mut TextEngine, col: bool, index: usize, delta: f64) {
        let Some(&(id, _)) = self.drag.as_ref().and_then(|d| d.origs.first()) else { return };
        let Some(orig) = self.drag.as_ref().and_then(|d| d.before.as_ref()).and_then(|b| b.pres.shape(self.slide, id)).and_then(table_of).cloned() else {
            return;
        };
        let orig_heights = self.heights_of(id, &orig);
        let d = delta.round() as i64;
        let Some(table) = self.pres.shape_mut(self.slide, id).and_then(table_of_mut) else { return };
        if col {
            // The two neighboring columns trade width; the table keeps its size.
            if index + 1 >= orig.cols.len() {
                return;
            }
            let d = d.clamp(MIN_TRACK - orig.cols[index], orig.cols[index + 1] - MIN_TRACK);
            table.cols[index] = orig.cols[index] + d;
            table.cols[index + 1] = orig.cols[index + 1] - d;
        } else {
            // A row grows or shrinks (down to its text); rows below move with it.
            let Some(&h) = orig_heights.get(index) else { return };
            table.rows[index].height = (h + d).max(MIN_TRACK);
        }
        self.layout_table(engine, id);
    }

    pub(crate) fn begin_cell_edit(&mut self, engine: &mut TextEngine, id: ShapeId, row: usize, col: usize) {
        let heights = self.layout_table(engine, id);
        let Some(shape) = self.pres.shape(self.slide, id) else { return };
        let Some(table) = table_of(shape) else { return };
        let (row, col) = table.anchor_of(row, col);
        let Some(cell) = table.cell(row, col) else { return };
        let frame = table.cell_frame(&heights, &shape.xfrm, row, col);
        let original = cell.text.clone();
        let rich = RichEdit::new(engine, original.clone(), text_width_pt(&frame, &original));
        let original_xfrm = shape.xfrm;
        self.selection = vec![id];
        self.text = Some(TextEdit { shape: id, cell: Some((row, col)), frame, rich, original, original_xfrm, remove_if_empty: false });
    }

    /// Writes the live cell text into the table so rows grow while typing.
    pub(crate) fn fit_edited_cell(&mut self, engine: &mut TextEngine) {
        let Some(edit) = &self.text else { return };
        let (id, (r, c)) = (edit.shape, edit.cell.expect("cell edit"));
        let body = edit.rich.body.clone();
        if let Some(cell) = self.pres.shape_mut(self.slide, id).and_then(table_of_mut).and_then(|t| t.cell_mut(r, c)) {
            cell.text = body;
        }
        let heights = self.layout_table(engine, id);
        let Some(shape) = self.pres.shape(self.slide, id) else { return };
        let Some(table) = table_of(shape) else { return };
        let frame = table.cell_frame(&heights, &shape.xfrm, r, c);
        let edit = self.text.as_mut().expect("cell edit");
        edit.frame = frame;
        edit.rich.set_width(engine, text_width_pt(&frame, &edit.rich.body));
    }

    /// Tab / Shift+Tab: the next or previous cell, adding a row after the last one.
    pub(crate) fn next_cell(&mut self, engine: &mut TextEngine, forward: bool) -> bool {
        let Some((id, current)) = self.text.as_ref().and_then(|t| Some((t.shape, t.cell?))) else { return false };
        let Some(table) = self.pres.shape(self.slide, id).and_then(table_of) else { return false };
        let cells = table.visible_cells();
        let last = table.rows.len() - 1;
        let i = cells.iter().position(|&p| p == current).unwrap_or(0);
        let target = if forward { cells.get(i + 1).copied() } else { i.checked_sub(1).map(|j| cells[j]) };
        match target {
            Some((r, c)) => {
                self.end_text_edit(engine);
                self.begin_cell_edit(engine, id, r, c);
                if let Some(edit) = &mut self.text {
                    edit.rich.select_all();
                }
                true
            }
            None if forward => {
                self.insert_table_row(engine, true) && {
                    self.end_text_edit(engine);
                    self.begin_cell_edit(engine, id, last + 1, 0);
                    true
                }
            }
            None => false,
        }
    }

    /// The table being worked on and the current cell: the edited cell, or for a
    /// selected table (`fallback_last` picks its last cell instead of the first).
    fn table_target(&self, fallback_last: bool) -> Option<(ShapeId, usize, usize)> {
        if let Some(edit) = &self.text {
            let (r, c) = edit.cell?;
            return Some((edit.shape, r, c));
        }
        let shape = self.selected_shape()?;
        let t = table_of(shape)?;
        Some(if fallback_last { (shape.id, t.rows.len() - 1, t.cols.len() - 1) } else { (shape.id, 0, 0) })
    }

    /// Whether a table (or one of its cells) is the current target, for the table ribbon tab.
    pub fn in_table(&self) -> bool {
        self.table_target(false).is_some()
    }

    /// Runs a structural change on the target table as one undo step, then puts
    /// the caret in cell `then(row, col)` of the result (when editing).
    fn change_table(&mut self, engine: &mut TextEngine, fallback_last: bool, f: impl FnOnce(&mut Table, usize, usize) -> Option<(usize, usize)>) -> bool {
        let Some((id, r, c)) = self.table_target(fallback_last) else { return false };
        let editing = self.text.is_some();
        self.end_text_edit(engine);
        let before = self.snapshot();
        let Some(table) = self.pres.shape_mut(self.slide, id).and_then(table_of_mut) else { return false };
        let Some(next) = f(table, r, c) else { return false };
        if table.rows.is_empty() || table.cols.is_empty() {
            // Nothing left: the table goes away.
            if let Some(slide) = self.pres.slides.get_mut(self.slide) {
                slide.shapes.retain(|s| s.id != id);
            }
            self.selection.clear();
            self.push_undo(before);
            return true;
        }
        self.layout_table(engine, id);
        self.push_undo(before);
        if editing {
            self.begin_cell_edit(engine, id, next.0, next.1);
        } else {
            self.selection = vec![id];
        }
        true
    }

    /// Inserts a row above or below the current one.
    pub fn insert_table_row(&mut self, engine: &mut TextEngine, below: bool) -> bool {
        self.change_table(engine, below, |t, r, c| {
            let span = t.cell(r, c).map_or(1, |x| x.row_span.max(1) as usize);
            let at = if below { r + span } else { r };
            let template = if below { at - 1 } else { at };
            let mut cells = Vec::with_capacity(t.cols.len());
            for col in 0..t.cols.len() {
                // A vertical merge crossing the insertion point absorbs the new row.
                let crosses = t.cell(at, col).is_some_and(|x| x.v_merge);
                let source = &t.rows[template].cells[col];
                let mut cell = blank_like(source);
                cell.h_merge = source.h_merge;
                cell.grid_span = if source.h_merge { 1 } else { source.grid_span };
                if crosses {
                    cell.v_merge = true;
                    let (ar, ac) = t.anchor_of(at, col);
                    if ac == col
                        && let Some(anchor) = t.cell_mut(ar, ac)
                    {
                        anchor.row_span += 1;
                    }
                }
                cells.push(cell);
            }
            let height = t.rows[template].height;
            t.rows.insert(at, TableRow { height, cells });
            Some((at, c))
        })
    }

    /// Inserts a column left or right of the current one; the table widens.
    pub fn insert_table_col(&mut self, engine: &mut TextEngine, right: bool) -> bool {
        self.change_table(engine, right, |t, r, c| {
            let span = t.cell(r, c).map_or(1, |x| x.grid_span.max(1) as usize);
            let at = if right { c + span } else { c };
            let template = if right { at - 1 } else { at };
            for row in 0..t.rows.len() {
                let crosses = t.cell(row, at).is_some_and(|x| x.h_merge);
                let source = &t.rows[row].cells[template];
                let mut cell = blank_like(source);
                cell.v_merge = source.v_merge;
                cell.row_span = if source.v_merge { 1 } else { source.row_span };
                if crosses {
                    cell.h_merge = true;
                    let (ar, ac) = t.anchor_of(row, at);
                    if ar == row
                        && let Some(anchor) = t.cell_mut(ar, ac)
                    {
                        anchor.grid_span += 1;
                    }
                }
                t.rows[row].cells.insert(at, cell);
            }
            let width = t.cols[template];
            t.cols.insert(at, width);
            Some((r, at))
        })
    }

    /// Deletes the current row (all rows a merged cell spans).
    pub fn delete_table_row(&mut self, engine: &mut TextEngine) -> bool {
        self.change_table(engine, false, |t, r, c| {
            let span = t.cell(r, c).map_or(1, |x| x.row_span.max(1) as usize);
            for _ in 0..span {
                remove_row(t, r);
            }
            Some((r.min(t.rows.len().saturating_sub(1)), c.min(t.cols.len().saturating_sub(1))))
        })
    }

    /// Deletes the current column (all columns a merged cell spans); the table narrows.
    pub fn delete_table_col(&mut self, engine: &mut TextEngine) -> bool {
        self.change_table(engine, false, |t, r, c| {
            let span = t.cell(r, c).map_or(1, |x| x.grid_span.max(1) as usize);
            for _ in 0..span {
                remove_col(t, c);
            }
            Some((r.min(t.rows.len().saturating_sub(1)), c.min(t.cols.len().saturating_sub(1))))
        })
    }

    /// Inserts a `rows` × `cols` table in the deck's default table style and edits its first cell.
    pub fn insert_table(&mut self, engine: &mut TextEngine, rows: usize, cols: usize) -> bool {
        self.end_text_edit(engine);
        if self.pres.slides.is_empty() || rows == 0 || cols == 0 {
            return false;
        }
        let before = self.snapshot();
        let width = (self.pres.slide_width as f64 * 0.8).round() as i64;
        let col_w = width / cols as i64;
        let mut table = Table {
            cols: vec![col_w; cols],
            rows: Vec::new(),
            flags: TableFlags { first_row: true, band_row: true, ..Default::default() },
            style: self.pres.table_style.clone(),
        };
        table.rows = (0..rows).map(|_| TableRow { height: NEW_ROW_HEIGHT, cells: vec![TableCell::default(); cols] }).collect();
        let defaults = self.pres.text_defaults.clone();
        let base = self.pres.default_run_style();
        for r in 0..rows {
            for c in 0..cols {
                let body = cell_body(defaults.as_ref(), &base, &table, r, c);
                table.rows[r].cells[c].text = body;
            }
        }
        let h = NEW_ROW_HEIGHT * rows as i64;
        let xfrm = Xfrm::new((self.pres.slide_width - col_w * cols as i64) / 2, (self.pres.slide_height - h) / 2, col_w * cols as i64, h);
        let id = self.pres.alloc_id();
        let mut shape = Shape::new(id, format!("表格 {}", id.0), xfrm);
        shape.kind = ShapeKind::Table(Box::new(table));
        self.pres.slides[self.slide].shapes.push(shape);
        self.layout_table(engine, id);
        self.push_undo(before);
        self.begin_cell_edit(engine, id, 0, 0);
        true
    }
}

/// Removes row `r`, moving merges that start or pass through it.
fn remove_row(t: &mut Table, r: usize) {
    if r >= t.rows.len() {
        return;
    }
    for c in 0..t.cols.len() {
        let Some(cell) = t.cell(r, c).cloned() else { continue };
        if cell.v_merge {
            let (ar, ac) = t.anchor_of(r, c);
            if ac == c
                && let Some(anchor) = t.cell_mut(ar, ac)
            {
                anchor.row_span = anchor.row_span.saturating_sub(1).max(1);
            }
        } else if cell.row_span > 1
            && let Some(below) = t.cell_mut(r + 1, c)
        {
            // The merged cell continues from the next row.
            *below = TableCell { row_span: cell.row_span - 1, v_merge: false, ..cell };
        }
    }
    t.rows.remove(r);
}

/// Removes column `c`, moving merges that start or pass through it.
fn remove_col(t: &mut Table, c: usize) {
    if c >= t.cols.len() {
        return;
    }
    for r in 0..t.rows.len() {
        let Some(cell) = t.cell(r, c).cloned() else { continue };
        if cell.h_merge {
            let (ar, ac) = t.anchor_of(r, c);
            if ar == r
                && let Some(anchor) = t.cell_mut(ar, ac)
            {
                anchor.grid_span = anchor.grid_span.saturating_sub(1).max(1);
            }
        } else if cell.grid_span > 1
            && let Some(right) = t.cell_mut(r, c + 1)
        {
            *right = TableCell { grid_span: cell.grid_span - 1, h_merge: false, ..cell };
        }
    }
    for row in &mut t.rows {
        if c < row.cells.len() {
            row.cells.remove(c);
        }
    }
    t.cols.remove(c);
}
