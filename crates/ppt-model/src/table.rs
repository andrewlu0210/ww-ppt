//! Tables (`a:tbl` inside a `p:graphicFrame`).
//!
//! Cells keep only their explicit properties (`a:tcPr`); the table style
//! fills in the rest when a cell is drawn, so banding and header formatting
//! follow structural edits.

use std::sync::Arc;

use crate::{Color, Emu, Line, TextBody, Xfrm};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// Grid column widths (`a:gridCol@w`).
    pub cols: Vec<Emu>,
    pub rows: Vec<TableRow>,
    pub flags: TableFlags,
    pub style: Option<Arc<TableStyle>>,
}

/// Which table style parts apply (`a:tblPr` attributes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableFlags {
    pub first_row: bool,
    pub last_row: bool,
    pub first_col: bool,
    pub last_col: bool,
    pub band_row: bool,
    pub band_col: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableRow {
    /// Minimum height; rows grow to fit their text.
    pub height: Emu,
    pub cells: Vec<TableCell>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableCell {
    pub text: TextBody,
    /// Columns / rows this cell spans (1 = no merge).
    pub grid_span: u32,
    pub row_span: u32,
    /// Covered by a merged cell to the left / above; not drawn.
    pub h_merge: bool,
    pub v_merge: bool,
    pub fill: CellFill,
    /// Explicit left, top, right, bottom borders; `None` defers to the table style.
    pub borders: [Option<Border>; 4],
}

impl Default for TableCell {
    fn default() -> Self {
        Self { text: TextBody::default(), grid_span: 1, row_span: 1, h_merge: false, v_merge: false, fill: CellFill::Inherit, borders: [None; 4] }
    }
}

impl TableCell {
    /// Whether this cell is hidden under a merged neighbor.
    pub fn is_covered(&self) -> bool {
        self.h_merge || self.v_merge
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CellFill {
    /// From the table style.
    #[default]
    Inherit,
    None,
    Solid(Color),
}

/// A border edge: drawn with `Some(line)`, explicitly absent with `None`.
pub type Border = Option<Line>;

/// Edges are indexed left, top, right, bottom.
pub const LEFT: usize = 0;
pub const TOP: usize = 1;
pub const RIGHT: usize = 2;
pub const BOTTOM: usize = 3;

/// A table style (`a:tblStyle`), with colors already resolved against the theme.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableStyle {
    pub id: String,
    pub whole: StylePart,
    pub band1_h: StylePart,
    pub band2_h: StylePart,
    pub band1_v: StylePart,
    pub band2_v: StylePart,
    pub first_row: StylePart,
    pub last_row: StylePart,
    pub first_col: StylePart,
    pub last_col: StylePart,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StylePart {
    pub fill: Option<CellFill>,
    pub left: Option<Border>,
    pub right: Option<Border>,
    pub top: Option<Border>,
    pub bottom: Option<Border>,
    pub inside_h: Option<Border>,
    pub inside_v: Option<Border>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<Color>,
}

/// Fill and borders of one cell after applying the table style.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellLook {
    pub fill: Option<Color>,
    pub borders: [Option<Line>; 4],
}

impl Table {
    pub fn cell(&self, row: usize, col: usize) -> Option<&TableCell> {
        self.rows.get(row)?.cells.get(col)
    }

    pub fn cell_mut(&mut self, row: usize, col: usize) -> Option<&mut TableCell> {
        self.rows.get_mut(row)?.cells.get_mut(col)
    }

    /// Style parts that apply to cell (`row`, `col`), lowest priority first.
    pub fn parts_for(&self, row: usize, col: usize) -> Vec<&StylePart> {
        let Some(style) = self.style.as_deref() else { return Vec::new() };
        let f = &self.flags;
        let (rows, cols) = (self.rows.len(), self.cols.len());
        let mut parts = vec![&style.whole];
        if f.band_col && !(f.first_col && col == 0) && !(f.last_col && col + 1 == cols) {
            let i = col - usize::from(f.first_col);
            parts.push(if i.is_multiple_of(2) { &style.band1_v } else { &style.band2_v });
        }
        if f.band_row {
            let header = f.first_row && row == 0;
            let footer = f.last_row && row + 1 == rows;
            if !header && !footer {
                let i = row - usize::from(f.first_row);
                parts.push(if i.is_multiple_of(2) { &style.band1_h } else { &style.band2_h });
            }
        }
        if f.first_col && col == 0 {
            parts.push(&style.first_col);
        }
        if f.last_col && col + 1 == cols {
            parts.push(&style.last_col);
        }
        if f.last_row && row + 1 == rows {
            parts.push(&style.last_row);
        }
        if f.first_row && row == 0 {
            parts.push(&style.first_row);
        }
        parts
    }

    /// Effective fill and borders of cell (`row`, `col`). A merged cell's right
    /// and bottom edges come from the last row / column it covers.
    pub fn look(&self, row: usize, col: usize) -> CellLook {
        let Some(cell) = self.cell(row, col) else { return CellLook::default() };
        let last_row = row + cell.row_span.max(1) as usize - 1;
        let last_col = col + cell.grid_span.max(1) as usize - 1;
        let (rows, cols) = (self.rows.len(), self.cols.len());

        let mut fill = CellFill::None;
        let mut edges: [Border; 4] = [None; 4];
        let parts = self.parts_for(row, col);
        for part in &parts {
            if let Some(f) = part.fill {
                fill = f;
            }
            let pick = |outer: Option<Border>, inside: Option<Border>, is_outer: bool| if is_outer { outer } else { inside.or(outer) };
            let left = pick(part.left, part.inside_v, col == 0);
            let top = pick(part.top, part.inside_h, row == 0);
            let right = pick(part.right, part.inside_v, last_col + 1 >= cols);
            let bottom = pick(part.bottom, part.inside_h, last_row + 1 >= rows);
            for (edge, value) in edges.iter_mut().zip([left, top, right, bottom]) {
                if let Some(v) = value {
                    *edge = v;
                }
            }
        }
        if cell.fill != CellFill::Inherit {
            fill = cell.fill;
        }
        for (edge, explicit) in edges.iter_mut().zip(cell.borders) {
            if let Some(b) = explicit {
                *edge = b;
            }
        }
        CellLook { fill: if let CellFill::Solid(c) = fill { Some(c) } else { None }, borders: edges }
    }

    /// Slide-space frame of cell (`row`, `col`) for the given row heights
    /// (see `TextEngine::table_row_heights`). Table rotation is ignored.
    pub fn cell_frame(&self, heights: &[Emu], table: &Xfrm, row: usize, col: usize) -> Xfrm {
        let cols = self.col_offsets();
        let cell = self.cell(row, col);
        let col = col.min(self.cols.len());
        let row = row.min(heights.len());
        let end_c = (col + cell.map_or(1, |c| c.grid_span.max(1) as usize)).min(self.cols.len());
        let end_r = (row + cell.map_or(1, |c| c.row_span.max(1) as usize)).min(heights.len());
        let y0: Emu = heights[..row].iter().sum();
        let h: Emu = heights[row..end_r].iter().sum();
        Xfrm { x: table.x + cols[col], y: table.y + y0, w: cols[end_c] - cols[col], h, ..Default::default() }
    }

    /// The visible cell covering grid position (`row`, `col`): itself, or the
    /// merged cell it lies under.
    pub fn anchor_of(&self, row: usize, col: usize) -> (usize, usize) {
        let (mut r, mut c) = (row, col);
        while c > 0 && self.cell(r, c).is_some_and(|x| x.h_merge) {
            c -= 1;
        }
        while r > 0 && self.cell(r, c).is_some_and(|x| x.v_merge) {
            r -= 1;
        }
        // A cell can be under a block merged both ways.
        while c > 0 && self.cell(r, c).is_some_and(|x| x.h_merge) {
            c -= 1;
        }
        (r, c)
    }

    /// Visible cells in reading order.
    pub fn visible_cells(&self) -> Vec<(usize, usize)> {
        let cols = self.cols.len();
        (0..self.rows.len())
            .flat_map(|r| (0..self.rows[r].cells.len().min(cols)).map(move |c| (r, c)))
            .filter(|&(r, c)| self.cell(r, c).is_some_and(|x| !x.is_covered()))
            .collect()
    }

    /// Left edge of each grid column plus the total width, relative to the table.
    pub fn col_offsets(&self) -> Vec<Emu> {
        let mut out = Vec::with_capacity(self.cols.len() + 1);
        let mut x = 0;
        out.push(0);
        for w in &self.cols {
            x += w;
            out.push(x);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(r: u8) -> Line {
        Line { color: Color::rgb(r, 0, 0), width: 12_700 }
    }

    fn table(rows: usize, cols: usize) -> Table {
        let style = TableStyle {
            whole: StylePart {
                fill: Some(CellFill::Solid(Color::rgb(1, 1, 1))),
                left: Some(Some(line(1))),
                right: Some(Some(line(1))),
                top: Some(Some(line(1))),
                bottom: Some(Some(line(1))),
                inside_h: Some(Some(line(2))),
                inside_v: Some(Some(line(3))),
                ..Default::default()
            },
            band1_h: StylePart { fill: Some(CellFill::Solid(Color::rgb(9, 9, 9))), ..Default::default() },
            first_row: StylePart { fill: Some(CellFill::Solid(Color::rgb(7, 7, 7))), bottom: Some(Some(line(4))), ..Default::default() },
            ..Default::default()
        };
        Table {
            cols: vec![100; cols],
            rows: (0..rows).map(|_| TableRow { height: 50, cells: vec![TableCell::default(); cols] }).collect(),
            flags: TableFlags { first_row: true, band_row: true, ..Default::default() },
            style: Some(Arc::new(style)),
        }
    }

    #[test]
    fn header_band_and_plain_rows() {
        let t = table(4, 2);
        assert_eq!(t.look(0, 0).fill, Some(Color::rgb(7, 7, 7)));
        // Banding starts after the header row.
        assert_eq!(t.look(1, 0).fill, Some(Color::rgb(9, 9, 9)));
        assert_eq!(t.look(2, 0).fill, Some(Color::rgb(1, 1, 1)));
        assert_eq!(t.look(3, 1).fill, Some(Color::rgb(9, 9, 9)));
    }

    #[test]
    fn outer_and_inside_borders() {
        let t = table(3, 2);
        let header = t.look(0, 0).borders;
        assert_eq!(header[LEFT], Some(line(1)));
        assert_eq!(header[RIGHT], Some(line(3)));
        assert_eq!(header[BOTTOM], Some(line(4)));
        let corner = t.look(2, 1).borders;
        assert_eq!(corner[RIGHT], Some(line(1)));
        assert_eq!(corner[BOTTOM], Some(line(1)));
        assert_eq!(corner[TOP], Some(line(2)));
    }

    #[test]
    fn explicit_cell_properties_win() {
        let mut t = table(2, 2);
        let cell = t.cell_mut(1, 0).unwrap();
        cell.fill = CellFill::None;
        cell.borders[TOP] = Some(None);
        let look = t.look(1, 0);
        assert_eq!(look.fill, None);
        assert_eq!(look.borders[TOP], None);
    }

    #[test]
    fn merged_cell_takes_far_edges() {
        let mut t = table(2, 3);
        t.cell_mut(1, 0).unwrap().grid_span = 3;
        assert_eq!(t.look(1, 0).borders[RIGHT], Some(line(1)));
        assert_eq!(t.col_offsets(), vec![0, 100, 200, 300]);
    }
}
