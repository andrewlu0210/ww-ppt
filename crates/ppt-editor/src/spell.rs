//! What spell checking needs from the editor: the current slide's texts and
//! where their characters are drawn.

use std::ops::Range;

use parley::{Affinity, BoundingBox, Cursor, Selection};
use ppt_geometry::kurbo::Affine;
use ppt_geometry::{TextFrame, text_affine, text_width_pt};
use ppt_model::{ShapeId, ShapeKind, Xfrm};
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
        self.find_places()
            .into_iter()
            .filter(|(s, p, _)| {
                *s == self.slide
                    && match p {
                        FindPlace::Notes => false,
                        // Only nodes whose own text is drawn in a shape of its own show where a word is.
                        FindPlace::Node(id, row) => self.node_shape(*id, *row).is_some(),
                        _ => true,
                    }
            })
            .map(|(_, p, t)| (p, t))
            .collect()
    }

    /// The drawn shape of SmartArt `id` (on the current slide) that shows row `row`'s own text, and nothing else.
    fn node_shape(&self, id: ShapeId, row: usize) -> Option<usize> {
        let shape = self.pres.shape(self.slide, id)?;
        let ShapeKind::Diagram { shapes, data: Some(d), .. } = &shape.kind else { return None };
        // A turned frame would need the text turned too.
        if shape.xfrm.rot != 0.0 || shape.xfrm.flip_h || shape.xfrm.flip_v {
            return None;
        }
        (0..shapes.len()).find(|&k| shapes[k].text.is_some() && d.row_of_own_text(k) == Some(row))
    }

    /// A text's layout and the transform from its layout space (points) to slide space.
    pub fn text_geometry(&self, engine: &mut TextEngine, place: FindPlace) -> Option<(BodyLayout, Affine)> {
        if let Some(t) = &self.text {
            let edited = match place {
                FindPlace::Shape(id) => t.shape == id && t.cell.is_none() && t.diagram.is_none(),
                FindPlace::Cell(id, r, c) => t.shape == id && t.cell == Some((r, c)),
                FindPlace::Node(id, row) => t.shape == id && t.diagram.is_some_and(|(_, r)| r == row),
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
            FindPlace::Node(id, row) => {
                let k = self.node_shape(id, row)?;
                let shape = slide.shapes.iter().find(|s| s.id == id)?;
                let ShapeKind::Diagram { shapes, w, h, .. } = &shape.kind else { return None };
                let child = &shapes[k];
                // The drawing's own frame, stretched to the SmartArt's.
                let (sx, sy) = (shape.xfrm.w as f64 / (*w).max(1) as f64, shape.xfrm.h as f64 / (*h).max(1) as f64);
                let at = |v: i64, s: f64, origin: i64| origin + (v as f64 * s).round() as i64;
                let xf = Xfrm { x: at(child.xfrm.x, sx, shape.xfrm.x), y: at(child.xfrm.y, sy, shape.xfrm.y), w: (child.xfrm.w as f64 * sx).round() as i64, h: (child.xfrm.h as f64 * sy).round() as i64, ..child.xfrm };
                (child.text.as_ref()?, TextFrame::of(xf, &child.geometry))
            }
            FindPlace::Notes => return None,
        };
        let layout = engine.layout_body(body, text_width_pt(&frame, body));
        let tf = text_affine(&frame, body, layout.height);
        Some((layout, tf))
    }
}
