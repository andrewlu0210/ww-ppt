//! Groups: selecting, entering, resizing, grouping and ungrouping.
//!
//! Members of a group stay flat in the slide's shape list, tagged with the
//! group's id. A click selects the whole group; a second click (or a double
//! click) enters it and selects the member under the pointer.

use std::collections::HashMap;

use ppt_model::{Shape, ShapeId, Xfrm};

use crate::{Editor, bounds};

/// The axis-aligned frame around `xfrms`.
pub(crate) fn union_frame(xfrms: impl IntoIterator<Item = Xfrm>) -> Option<Xfrm> {
    let b = xfrms.into_iter().map(|x| bounds(&x)).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))?;
    Some(Xfrm::new(b.0.round() as i64, b.1.round() as i64, (b.2 - b.0).round() as i64, (b.3 - b.1).round() as i64))
}

/// Maps `orig` (inside the frame `from`) to the frame `to`, scaling position and size.
pub(crate) fn rescale(orig: &Xfrm, from: &Xfrm, to: &Xfrm) -> Xfrm {
    let sx = to.w as f64 / from.w.max(1) as f64;
    let sy = to.h as f64 / from.h.max(1) as f64;
    Xfrm {
        x: to.x + ((orig.x - from.x) as f64 * sx).round() as i64,
        y: to.y + ((orig.y - from.y) as f64 * sy).round() as i64,
        w: (orig.w as f64 * sx).round() as i64,
        h: (orig.h as f64 * sy).round() as i64,
        ..*orig
    }
}

/// Clears the group of shapes copied without the rest of their group.
pub(crate) fn detach_partial_groups(copies: &mut [Shape], slide: &[Shape]) {
    let mut counts: HashMap<ShapeId, usize> = HashMap::new();
    for s in copies.iter() {
        if let Some(g) = s.group {
            *counts.entry(g).or_default() += 1;
        }
    }
    for s in copies.iter_mut() {
        if let Some(g) = s.group
            && slide.iter().filter(|x| x.group == Some(g)).count() != counts[&g]
        {
            s.group = None;
        }
    }
}

impl Editor {
    pub(crate) fn members(&self, group: ShapeId) -> Vec<ShapeId> {
        self.current_shapes().iter().filter(|s| s.group == Some(group)).map(|s| s.id).collect()
    }

    /// What a click on `id` selects: the whole group unless that group is entered.
    pub(crate) fn click_target(&self, id: ShapeId) -> Vec<ShapeId> {
        match self.pres.shape(self.slide, id).and_then(|s| s.group) {
            Some(g) if self.entered_group != Some(g) => self.members(g),
            _ => vec![id],
        }
    }

    /// The group whose members are exactly the selection.
    pub fn selection_group(&self) -> Option<ShapeId> {
        let first = self.pres.shape(self.slide, *self.selection.first()?)?.group?;
        let members = self.members(first);
        (members.len() > 1 && members.len() == self.selection.len() && members.iter().all(|m| self.selection.contains(m))).then_some(first)
    }

    /// Frame of the entered group, drawn dashed while its members are edited.
    pub fn entered_group_frame(&self) -> Option<Xfrm> {
        let g = self.entered_group?;
        union_frame(self.current_shapes().iter().filter(|s| s.group == Some(g)).map(|s| s.xfrm))
    }

    /// Whether `id` belongs to the selected group (clicks on it start a move, then may enter).
    pub(crate) fn in_selected_group(&self, id: ShapeId) -> Option<ShapeId> {
        let g = self.selection_group()?;
        (self.pres.shape(self.slide, id)?.group == Some(g)).then_some(g)
    }

    /// Groups the selection (two or more shapes) into a new group placed at the
    /// topmost member's depth. Existing groups among them are merged.
    pub fn group_selected(&mut self) -> bool {
        if self.selection.len() < 2 {
            return false;
        }
        let before = self.snapshot();
        let id = self.pres.alloc_id();
        let selection = self.selection.clone();
        let shapes = &mut self.pres.slides[self.slide].shapes;
        let top = shapes.iter().rposition(|s| selection.contains(&s.id)).unwrap_or(0);
        let below = shapes[..top].iter().filter(|s| !selection.contains(&s.id)).count();
        let (mut members, rest): (Vec<Shape>, Vec<Shape>) = shapes.drain(..).partition(|s| selection.contains(&s.id));
        for m in members.iter_mut() {
            m.group = Some(id);
        }
        let mut rest = rest;
        let tail = rest.split_off(below);
        *shapes = [rest, members, tail].concat();
        self.entered_group = None;
        self.prune_animations();
        self.push_undo(before);
        true
    }

    /// Ungroups the selected group; its members stay selected.
    pub fn ungroup_selected(&mut self) -> bool {
        let Some(g) = self.selection_group().or_else(|| self.selected_shape().and_then(|s| s.group)) else { return false };
        let before = self.snapshot();
        for s in self.pres.slides[self.slide].shapes.iter_mut().filter(|s| s.group == Some(g)) {
            s.group = None;
        }
        self.selection = self.current_shapes().iter().filter(|s| before.pres.shape(self.slide, s.id).is_some_and(|b| b.group == Some(g))).map(|s| s.id).collect();
        self.entered_group = None;
        self.prune_animations();
        self.push_undo(before);
        true
    }

    /// The frame handles and resizing act on: the single shape or the selected group.
    pub fn selection_frame(&self) -> Option<Xfrm> {
        if let Some(s) = self.selected_shape() {
            return Some(s.xfrm);
        }
        self.selection_group()?;
        union_frame(self.selected_shapes().iter().map(|s| s.xfrm))
    }
}
