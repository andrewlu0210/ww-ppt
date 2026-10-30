//! Drawing a shape by dragging it out on the slide (after picking it in the
//! shape gallery).

use ppt_model::{Color, EndKind, Geometry, Line, LineEnd, Paint, Shape, ShapeId, Xfrm};
use ppt_text::TextEngine;

use crate::{Drag, DragKind, Editor, PointerEvent};

/// Tool name for a line with an arrowhead at its end.
pub const ARROW_LINE: &str = "arrowLine";

/// Size of a shape placed with a click instead of a drag (EMU).
const CLICK_SIZE: i64 = 914_400;

fn is_line_tool(tool: &str) -> bool {
    tool == "line" || tool == ARROW_LINE
}

/// The frame from `a` to `b`: a box for shapes, a (possibly flipped) diagonal
/// for lines. `square` keeps shapes square and lines at multiples of 45°.
fn frame_between(tool: &str, a: (f64, f64), b: (f64, f64), square: bool) -> Xfrm {
    let (mut dx, mut dy) = (b.0 - a.0, b.1 - a.1);
    if square {
        if is_line_tool(tool) {
            let len = dx.hypot(dy);
            let angle = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
            (dx, dy) = (len * angle.cos(), len * angle.sin());
        } else {
            let side = dx.abs().max(dy.abs());
            (dx, dy) = (side * dx.signum(), side * dy.signum());
        }
    }
    let x = Xfrm::new(a.0.min(a.0 + dx).round() as i64, a.1.min(a.1 + dy).round() as i64, dx.abs().round() as i64, dy.abs().round() as i64);
    if is_line_tool(tool) { Xfrm { flip_h: dx < 0.0, flip_v: dy < 0.0, ..x } } else { Xfrm { w: x.w.max(1), h: x.h.max(1), ..x } }
}

impl Editor {
    /// Picks the shape the next drag on the slide draws (`None` cancels).
    pub fn set_draw_tool(&mut self, engine: &mut TextEngine, tool: Option<&str>) {
        self.end_text_edit(engine);
        self.draw_tool = tool.map(str::to_string);
    }

    /// Starts drawing with the picked tool at the pointer. Returns false when no tool is picked.
    pub(crate) fn begin_draw(&mut self, ev: &PointerEvent) -> bool {
        let Some(tool) = self.draw_tool.take() else { return false };
        if self.pres.slides.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let id = self.pres.alloc_id();
        let line = is_line_tool(&tool);
        let preset = if line { "line" } else { tool.as_str() };
        let mut shape = Shape::new(id, format!("{preset} {}", id.0), frame_between(&tool, (ev.x, ev.y), (ev.x, ev.y), false));
        shape.geometry = Geometry::preset(preset);
        if line {
            let mut l = Line::new(Color::rgb(0x44, 0x72, 0xC4), 12_700);
            if tool == ARROW_LINE {
                l.tail = LineEnd { kind: EndKind::Triangle, ..LineEnd::NONE };
            }
            shape.line = Some(l);
        } else {
            shape.fill = Some(Paint::Solid(Color::rgb(0x44, 0x72, 0xC4)));
            shape.line = Some(Line::new(Color::rgb(0x2F, 0x52, 0x8F), 12_700));
        }
        self.pres.slides[self.slide].shapes.push(shape);
        self.selection = vec![id];
        self.entered_group = None;
        self.drag = Some(Drag { kind: DragKind::Draw(id), start: (ev.x, ev.y), origs: Vec::new(), base_selection: vec![], before: Some(before), moved: false });
        self.draw_shape_tool = Some(tool);
        true
    }

    pub(crate) fn draw_move(&mut self, id: ShapeId, start: (f64, f64), ev: &PointerEvent) {
        let Some(tool) = self.draw_shape_tool.clone() else { return };
        let frame = frame_between(&tool, start, (ev.x, ev.y), ev.shift);
        if let Some(s) = self.pres.shape_mut(self.slide, id) {
            s.xfrm = frame;
        }
    }

    /// Ends drawing; a click without a drag places the shape at a default size.
    pub(crate) fn end_draw(&mut self, id: ShapeId, start: (f64, f64), moved: bool) {
        let tool = self.draw_shape_tool.take().unwrap_or_default();
        if !moved && let Some(s) = self.pres.shape_mut(self.slide, id) {
            let (x, y) = (start.0.round() as i64, start.1.round() as i64);
            s.xfrm = if is_line_tool(&tool) { Xfrm::new(x, y, CLICK_SIZE, 0) } else { Xfrm::new(x - CLICK_SIZE / 2, y - CLICK_SIZE / 2, CLICK_SIZE, CLICK_SIZE) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_follow_the_drag() {
        assert_eq!(frame_between("rect", (100.0, 100.0), (50.0, 300.0), false), Xfrm::new(50, 100, 50, 200));
        assert_eq!(frame_between("rect", (100.0, 100.0), (50.0, 300.0), true), Xfrm::new(-100, 100, 200, 200));
        let l = frame_between("line", (100.0, 100.0), (0.0, 300.0), false);
        assert!(l.flip_h && !l.flip_v && (l.x, l.w, l.h) == (0, 100, 200));
        let l = frame_between("line", (0.0, 0.0), (100.0, 10.0), true);
        assert_eq!(l.h, 0, "snaps to horizontal");
    }
}
