//! What spell checking needs from the editor: the current slide's texts and
//! where their characters are drawn.

use std::ops::Range;

use parley::{Affinity, BoundingBox, Cursor, Selection};
use ppt_geometry::kurbo::Affine;
use ppt_geometry::{TextFrame, text_affine, text_width_pt};
use ppt_model::ShapeKind;
use ppt_text::{BodyLayout, TextEngine};

use crate::{Editor, FindPlace};

/// Boxes (layout space, points) covering `range` of a laid-out text.
pub fn range_rects(layout: &BodyLayout, range: Range<usize>) -> Vec<BoundingBox> {
    let l = &layout.layout;
    let start = Selection::from_byte_index(l, range.start, Affinity::Downstream);
    let end = Cursor::from_byte_index(l, range.end, Affinity::Upstream);
    start.extend(end).geometry(l).into_iter().map(|(b, _)| b).collect()
}

impl Editor {
    /// The current slide's texts (shapes and table cells), live while edited.
    pub fn slide_texts(&self) -> Vec<(FindPlace, String)> {
        self.find_places().into_iter().filter(|(s, p, _)| *s == self.slide && *p != FindPlace::Notes).map(|(_, p, t)| (p, t)).collect()
    }

    /// A text's layout and the transform from its layout space (points) to slide space.
    pub fn text_geometry(&self, engine: &mut TextEngine, place: FindPlace) -> Option<(BodyLayout, Affine)> {
        if let Some(t) = &self.text {
            let edited = match place {
                FindPlace::Shape(id) => t.shape == id && t.cell.is_none(),
                FindPlace::Cell(id, r, c) => t.shape == id && t.cell == Some((r, c)),
                FindPlace::Notes => false,
            };
            if edited {
                let layout = t.rich.layout().clone();
                let tf = text_affine(&t.frame, &t.rich.body, layout.height);
                return Some((layout, tf));
            }
        }
        let slide = self.pres.slides.get(self.slide)?;
        let (body, frame) = match place {
            FindPlace::Shape(id) => {
                let shape = slide.shapes.iter().find(|s| s.id == id)?;
                (shape.text.as_ref()?, TextFrame::of(shape.xfrm, &shape.geometry))
            }
            FindPlace::Cell(id, r, c) => {
                let shape = slide.shapes.iter().find(|s| s.id == id)?;
                let ShapeKind::Table(table) = &shape.kind else { return None };
                let heights = self.table_heights.get(&id).cloned().unwrap_or_else(|| engine.table_row_heights(table));
                (&table.cell(r, c)?.text, TextFrame::plain(table.cell_frame(&heights, &shape.xfrm, r, c)))
            }
            FindPlace::Notes => return None,
        };
        let layout = engine.layout_body(body, text_width_pt(&frame, body));
        let tf = text_affine(&frame, body, layout.height);
        Some((layout, tf))
    }
}
