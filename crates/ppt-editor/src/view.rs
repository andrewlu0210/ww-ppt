//! 檢視: drawing guides (dragged on the canvas, snapped to) and the grid.

use ppt_model::{DrawGuide, Emu};
use ppt_text::TextEngine;

use crate::{Editor, PointerEvent};

/// Guides are stored in eighths of a point: positions round to that unit.
const GUIDE_UNIT: f64 = 12_700.0 / 8.0;
const EMU_PER_CM: f64 = 360_000.0;

impl Editor {
    /// Nearest grid line to `v` on the axis whose slide extent is `extent`
    /// (the grid runs through the slide's center, like PowerPoint's).
    pub(crate) fn grid_round(&self, v: f64, extent: Emu) -> f64 {
        let g = self.pres.view.grid_spacing.max(1) as f64;
        let c = extent as f64 / 2.0;
        c + ((v - c) / g).round() * g
    }

    /// Index of the shown guide within `tol` of the point.
    pub(crate) fn guide_at(&self, x: f64, y: f64, tol: f64) -> Option<usize> {
        if !self.pres.view.show_guides {
            return None;
        }
        self.pres.view.guides.iter().position(|g| (if g.vertical { x } else { y } - g.pos as f64).abs() <= tol)
    }

    /// Moves guide `index` to the pointer.
    pub(crate) fn guide_move(&mut self, index: usize, ev: &PointerEvent) {
        if let Some(g) = self.pres.view.guides.get_mut(index) {
            let v = if g.vertical { ev.x } else { ev.y };
            g.pos = ((v / GUIDE_UNIT).round() * GUIDE_UNIT).round() as Emu;
        }
    }

    /// Ends a guide drag: a guide dropped off the slide is removed.
    pub(crate) fn guide_end(&mut self, index: usize) {
        let (w, h) = (self.pres.slide_width, self.pres.slide_height);
        if let Some(g) = self.pres.view.guides.get(index)
            && (g.pos < 0 || g.pos > if g.vertical { w } else { h })
        {
            self.pres.view.guides.remove(index);
        }
    }

    /// While a guide is dragged: its distance from the slide center in centimeters, e.g. "3.20".
    pub fn guide_label(&self) -> Option<String> {
        let index = self.dragged_guide()?;
        let g = self.pres.view.guides.get(index)?;
        let center = if g.vertical { self.pres.slide_width } else { self.pres.slide_height } as f64 / 2.0;
        Some(format!("{:.2}", ((g.pos as f64 - center) / EMU_PER_CM).abs()))
    }

    /// Shows or hides guides, as one undo step (the setting is saved with the deck).
    pub fn set_show_guides(&mut self, engine: &mut TextEngine, show: bool) -> bool {
        self.edit_view(engine, |v| v.show_guides = show)
    }

    pub fn set_snap_to_grid(&mut self, engine: &mut TextEngine, snap: bool) -> bool {
        self.edit_view(engine, |v| v.snap_to_grid = snap)
    }

    pub fn set_grid_spacing(&mut self, engine: &mut TextEngine, spacing: Emu) -> bool {
        self.edit_view(engine, |v| v.grid_spacing = spacing.max(1))
    }

    /// Adds a guide through the slide's center (and shows guides).
    pub fn add_guide(&mut self, engine: &mut TextEngine, vertical: bool) -> bool {
        let pos = if vertical { self.pres.slide_width } else { self.pres.slide_height } / 2;
        let snap = |p: f64| ((p / GUIDE_UNIT).round() * GUIDE_UNIT).round() as Emu;
        // Step off guides already there (by one centimeter), so the new one shows.
        let mut pos = snap(pos as f64);
        while self.pres.view.guides.iter().any(|g| g.vertical == vertical && (g.pos - pos).abs() < 1_000) {
            pos = snap(pos as f64 + EMU_PER_CM);
        }
        self.edit_view(engine, |v| {
            v.show_guides = true;
            v.guides.push(DrawGuide { vertical, pos });
        })
    }

    fn edit_view(&mut self, engine: &mut TextEngine, f: impl FnOnce(&mut ppt_model::ViewSettings)) -> bool {
        self.end_text_edit(engine);
        let mut view = self.pres.view.clone();
        f(&mut view);
        if view == self.pres.view {
            return false;
        }
        let before = self.snapshot();
        self.pres.view = view;
        self.push_undo(before);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppt_model::{Presentation, Slide};

    #[test]
    fn grid_lines_run_through_the_center() {
        let mut pres = Presentation::default();
        pres.slides.push(Slide::default());
        pres.view.grid_spacing = 360_000;
        let e = Editor::new(pres);
        let w = e.pres.slide_width;
        let c = w as f64 / 2.0;
        assert_eq!(e.grid_round(c + 100_000.0, w), c);
        assert_eq!(e.grid_round(c + 200_000.0, w), c + 360_000.0);
    }
}
