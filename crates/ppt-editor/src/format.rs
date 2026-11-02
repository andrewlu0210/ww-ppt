//! Shape format commands: exact size and position, rotation and flips,
//! changing the shape's outline, shadows, and picture cropping.

use ppt_model::{Crop, Geometry, Shadow, ShapeKind, Xfrm};

use crate::group::rescale;
use crate::{Editor, Handle};

/// Smallest visible part of a cropped picture (fraction of the image).
const MIN_VISIBLE: f64 = 0.01;

impl Editor {
    /// Moves and resizes the single selected shape, or the selected group as a
    /// whole, to `to` (rotation applies to single shapes).
    pub fn set_selection_frame(&mut self, to: Xfrm) -> bool {
        if self.selection_locked() {
            return false;
        }
        let Some(from) = self.selection_frame() else { return false };
        let to = Xfrm { w: to.w.max(1), h: to.h.max(1), ..to };
        if (from.x, from.y, from.w, from.h, from.rot) == (to.x, to.y, to.w, to.h, to.rot) {
            return false;
        }
        let single = self.selected_shape().is_some();
        self.modify_selected(|s| {
            s.xfrm = if single { Xfrm { flip_h: s.xfrm.flip_h, flip_v: s.xfrm.flip_v, ..to } } else { rescale(&s.xfrm, &from, &to) };
        })
    }

    /// Turns the selection by `degrees` (clockwise); a group turns about its center.
    pub fn rotate_selection(&mut self, degrees: f64) -> bool {
        if self.selection_locked() {
            return false;
        }
        let Some(frame) = self.selection_frame() else { return false };
        let group = self.selected_shape().is_none();
        let (cx, cy) = frame.center();
        let (sin, cos) = degrees.to_radians().sin_cos();
        self.modify_selected(|s| {
            if group {
                let (x, y) = s.xfrm.center();
                let (dx, dy) = (x - cx, y - cy);
                let (nx, ny) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
                s.xfrm.x = (nx - s.xfrm.w as f64 / 2.0).round() as i64;
                s.xfrm.y = (ny - s.xfrm.h as f64 / 2.0).round() as i64;
            }
            s.xfrm.rot = (s.xfrm.rot + degrees).rem_euclid(360.0);
        })
    }

    /// Mirrors the selection left-right (or top-bottom); a group mirrors as a whole.
    pub fn flip_selection(&mut self, horizontal: bool) -> bool {
        if self.selection_locked() {
            return false;
        }
        let Some(frame) = self.selection_frame() else { return false };
        let group = self.selected_shape().is_none();
        let (cx, cy) = frame.center();
        self.modify_selected(|s| {
            let x = &mut s.xfrm;
            if group {
                if horizontal {
                    x.x = (2.0 * cx - x.x as f64 - x.w as f64).round() as i64;
                } else {
                    x.y = (2.0 * cy - x.y as f64 - x.h as f64).round() as i64;
                }
            }
            if horizontal {
                x.flip_h = !x.flip_h;
            } else {
                x.flip_v = !x.flip_v;
            }
            // A mirror image turns the other way.
            x.rot = (360.0 - x.rot).rem_euclid(360.0);
        })
    }

    /// Gives the selected shapes (not pictures, tables or charts) another preset outline.
    pub fn change_shape(&mut self, preset: &str) -> bool {
        let applies = |s: &ppt_model::Shape| matches!(s.kind, ShapeKind::Shape) && !s.geometry.is_line();
        if !self.selected_shapes().into_iter().any(applies) {
            return false;
        }
        let geometry = Geometry::preset(preset);
        self.modify_selected(|s| {
            if applies(s) {
                s.geometry = geometry.clone();
            }
        })
    }

    /// Sets (or with `None` removes) the selected shapes' drop shadow.
    pub fn set_shadow(&mut self, shadow: Option<Shadow>) -> bool {
        if self.selected_shapes().iter().all(|s| s.shadow == shadow) {
            return false;
        }
        self.modify_selected(|s| s.shadow = shadow)
    }

    /// The selected shape's shadow, when exactly one shape is selected.
    pub fn selection_shadow(&self) -> Option<Option<Shadow>> {
        self.selected_shape().map(|s| s.shadow)
    }

    /// The selected picture's cropping.
    pub fn selected_crop(&self) -> Option<Crop> {
        match self.selected_shape()?.kind {
            ShapeKind::Picture { crop, .. } => Some(crop),
            _ => None,
        }
    }

    /// Starts or ends crop mode on the selected picture: its handles then trim
    /// the picture instead of resizing it.
    pub fn set_crop_mode(&mut self, on: bool) -> bool {
        let target = if on { self.selected_shape().filter(|s| matches!(s.kind, ShapeKind::Picture { .. })).map(|s| s.id) } else { None };
        if target == self.crop {
            return false;
        }
        self.crop = target;
        true
    }

    /// Sets the selected picture's cropping, keeping the image's scale: the
    /// frame grows or shrinks with what is shown.
    pub fn set_crop(&mut self, crop: Crop) -> bool {
        let Some(old) = self.selected_crop() else { return false };
        let clamp = |c: Crop| {
            let (l, r) = (c.left.clamp(0.0, 1.0), c.right.clamp(0.0, 1.0));
            let (t, b) = (c.top.clamp(0.0, 1.0), c.bottom.clamp(0.0, 1.0));
            let r = r.min(1.0 - MIN_VISIBLE - l);
            let b = b.min(1.0 - MIN_VISIBLE - t);
            Crop { left: l, top: t, right: r.max(0.0), bottom: b.max(0.0) }
        };
        let crop = clamp(crop);
        if crop == old {
            return false;
        }
        self.modify_selected(|s| {
            let ShapeKind::Picture { crop: c, .. } = &mut s.kind else { return };
            let x = &mut s.xfrm;
            // The whole image at the current scale, in the shape's own (unrotated) axes.
            let full_w = x.w as f64 / (1.0 - old.left - old.right).max(MIN_VISIBLE);
            let full_h = x.h as f64 / (1.0 - old.top - old.bottom).max(MIN_VISIBLE);
            let (l0, t0) = (x.x as f64 - old.left * full_w, x.y as f64 - old.top * full_h);
            let new = Xfrm {
                x: (l0 + crop.left * full_w).round() as i64,
                y: (t0 + crop.top * full_h).round() as i64,
                w: ((1.0 - crop.left - crop.right) * full_w).round().max(1.0) as i64,
                h: ((1.0 - crop.top - crop.bottom) * full_h).round().max(1.0) as i64,
                ..*x
            };
            *x = new;
            *c = crop;
        })
    }

    /// Removes the selected picture's cropping and sizes it to the image's own
    /// proportions (`pixels`), keeping its width.
    pub fn reset_picture(&mut self, pixels: Option<(u32, u32)>) -> bool {
        if self.selected_crop().is_none() {
            return false;
        }
        self.crop = None;
        self.modify_selected(|s| {
            let ShapeKind::Picture { crop, .. } = &mut s.kind else { return };
            *crop = Crop::default();
            s.xfrm.rot = 0.0;
            s.xfrm.flip_h = false;
            s.xfrm.flip_v = false;
            if let Some((w, h)) = pixels.filter(|p| p.0 > 0 && p.1 > 0) {
                let (cx, cy) = s.xfrm.center();
                let height = (s.xfrm.w as f64 * h as f64 / w as f64).round() as i64;
                s.xfrm.h = height.max(1);
                s.xfrm.y = (cy - height as f64 / 2.0).round() as i64;
                s.xfrm.x = (cx - s.xfrm.w as f64 / 2.0).round() as i64;
            }
        })
    }

    /// Crop mode drag: the frame moved to `new` by `handle`; trims the picture
    /// to match, keeping the image where it is.
    pub(crate) fn crop_drag(&mut self, id: ppt_model::ShapeId, orig: Xfrm, orig_crop: Crop, handle: Handle, new: Xfrm) {
        let Some(shape) = self.pres.shape_mut(self.slide, id) else { return };
        let ShapeKind::Picture { crop, .. } = &mut shape.kind else { return };
        let full_w = orig.w as f64 / (1.0 - orig_crop.left - orig_crop.right).max(MIN_VISIBLE);
        let full_h = orig.h as f64 / (1.0 - orig_crop.top - orig_crop.bottom).max(MIN_VISIBLE);
        let (dx, dy) = handle.sign();
        let grow_w = (new.w - orig.w) as f64 / full_w;
        let grow_h = (new.h - orig.h) as f64 / full_h;
        let mut c = orig_crop;
        // Uncovering more than the image (negative crop) isn't possible.
        if dx < 0.0 {
            c.left = (orig_crop.left - grow_w).clamp(0.0, 1.0 - MIN_VISIBLE - c.right);
        } else if dx > 0.0 {
            c.right = (orig_crop.right - grow_w).clamp(0.0, 1.0 - MIN_VISIBLE - c.left);
        }
        if dy < 0.0 {
            c.top = (orig_crop.top - grow_h).clamp(0.0, 1.0 - MIN_VISIBLE - c.bottom);
        } else if dy > 0.0 {
            c.bottom = (orig_crop.bottom - grow_h).clamp(0.0, 1.0 - MIN_VISIBLE - c.top);
        }
        *crop = c;
        // The frame follows the clamped crop, anchored at the opposite edge.
        let w = ((1.0 - c.left - c.right) * full_w).round() as i64;
        let h = ((1.0 - c.top - c.bottom) * full_h).round() as i64;
        let left0 = orig.x as f64 - orig_crop.left * full_w;
        let top0 = orig.y as f64 - orig_crop.top * full_h;
        let x = if orig.rot == 0.0 { (left0 + c.left * full_w).round() as i64 } else { new.x };
        let y = if orig.rot == 0.0 { (top0 + c.top * full_h).round() as i64 } else { new.y };
        shape.xfrm = Xfrm { x, y, w: w.max(1), h: h.max(1), ..new };
    }

    /// In crop mode: the whole (uncropped) image's frame, for drawing it dimmed.
    pub fn crop_view(&self) -> Option<(ppt_model::ShapeId, Xfrm)> {
        let id = self.crop?;
        let s = self.pres.shape(self.slide, id)?;
        let ShapeKind::Picture { crop: c, .. } = s.kind else { return None };
        let full_w = s.xfrm.w as f64 / (1.0 - c.left - c.right).max(MIN_VISIBLE);
        let full_h = s.xfrm.h as f64 / (1.0 - c.top - c.bottom).max(MIN_VISIBLE);
        Some((id, Xfrm { x: (s.xfrm.x as f64 - c.left * full_w).round() as i64, y: (s.xfrm.y as f64 - c.top * full_h).round() as i64, w: full_w.round() as i64, h: full_h.round() as i64, ..s.xfrm }))
    }
}
