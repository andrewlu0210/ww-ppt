//! 替代文字 and the selection pane (選取窗格): the current slide's objects
//! top-most first, with names, visibility and alternative text.

use ppt_model::{ShapeId, ShapeKind};
use ppt_text::TextEngine;

use crate::{Editor, ZOrder};

/// One row of the selection pane.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneItem {
    /// The shape, or for a group header the group's id.
    pub id: ShapeId,
    pub name: String,
    /// "group", "picture", "table", "chart", "diagram", "text", "line", "shape" or "other".
    pub kind: &'static str,
    /// 1 for members listed under their group's header.
    pub depth: u8,
    pub hidden: bool,
    pub selected: bool,
    pub alt_text: String,
    pub decorative: bool,
}

impl Editor {
    /// The selection pane's rows: top-most object first, group members under their group.
    pub fn pane_items(&self) -> Vec<PaneItem> {
        let shapes = self.current_shapes();
        let mut out: Vec<PaneItem> = Vec::new();
        for s in shapes.iter().rev() {
            // Unparsed content kept verbatim has nothing to show or toggle.
            if matches!(&s.kind, ShapeKind::Opaque { label } if label == "hidden") {
                continue;
            }
            if let Some(g) = s.group
                && !out.iter().any(|i| i.id == g)
            {
                let members: Vec<_> = shapes.iter().filter(|m| m.group == Some(g)).collect();
                out.push(PaneItem {
                    id: g,
                    name: format!("{} {}", ppt_model::lang().pick("群組", "组合", "Group"), out.iter().filter(|i| i.kind == "group").count() + 1),
                    kind: "group",
                    depth: 0,
                    hidden: members.iter().all(|m| m.access.hidden),
                    selected: self.selection_group() == Some(g),
                    alt_text: String::new(),
                    decorative: false,
                });
            }
            let kind = match &s.kind {
                ShapeKind::Picture { .. } => "picture",
                ShapeKind::Table(_) => "table",
                ShapeKind::Chart(_) => "chart",
                ShapeKind::Diagram { .. } => "diagram",
                ShapeKind::Opaque { .. } => "other",
                ShapeKind::Shape if s.geometry.is_line() => "line",
                ShapeKind::Shape if s.fill.is_none() && s.line.is_none() && s.text.is_some() => "text",
                ShapeKind::Shape => "shape",
            };
            let grouped_selected = s.group.is_some() && self.selection_group() == s.group;
            out.push(PaneItem {
                id: s.id,
                name: s.name.clone(),
                kind,
                depth: u8::from(s.group.is_some()),
                hidden: s.access.hidden,
                selected: self.selection.contains(&s.id) && !grouped_selected,
                alt_text: s.access.alt_text.clone(),
                decorative: s.access.decorative,
            });
        }
        out
    }

    /// The shape ids a pane row stands for (a group header: all its members).
    fn pane_targets(&self, id: ShapeId) -> Vec<ShapeId> {
        let members = self.members(id);
        if members.is_empty() { vec![id] } else { members }
    }

    /// Selects what a pane row stands for. Group members are selected inside their group.
    pub fn select_in_pane(&mut self, engine: &mut TextEngine, id: ShapeId, additive: bool) -> bool {
        self.end_text_edit(engine);
        let targets = self.pane_targets(id);
        if targets.iter().any(|t| self.pres.shape(self.slide, *t).is_none_or(|s| s.access.hidden)) {
            return false;
        }
        let member_of = (targets == [id]).then(|| self.pres.shape(self.slide, id).and_then(|s| s.group)).flatten();
        self.entered_group = member_of;
        if additive {
            for t in targets {
                if let Some(i) = self.selection.iter().position(|s| *s == t) {
                    self.selection.remove(i);
                } else {
                    self.selection.push(t);
                }
            }
        } else {
            self.selection = targets;
        }
        true
    }

    /// Hides or shows a pane row (a group: all its members), as one undo step.
    pub fn set_shape_hidden(&mut self, engine: &mut TextEngine, id: ShapeId, hidden: bool) -> bool {
        let targets = self.pane_targets(id);
        self.edit_access(engine, &targets, |a| a.hidden = hidden)
    }

    /// 全部顯示 / 全部隱藏 on the current slide.
    pub fn set_all_hidden(&mut self, engine: &mut TextEngine, hidden: bool) -> bool {
        let ids: Vec<ShapeId> = self.pane_items().iter().filter(|i| i.kind != "group").map(|i| i.id).collect();
        self.edit_access(engine, &ids, |a| a.hidden = hidden)
    }

    /// Renames a shape (group headers cannot be renamed).
    pub fn rename_shape(&mut self, engine: &mut TextEngine, id: ShapeId, name: &str) -> bool {
        let name = name.trim();
        self.end_text_edit(engine);
        let Some(shape) = self.pres.shape(self.slide, id) else { return false };
        if name.is_empty() || shape.name == name {
            return false;
        }
        let before = self.snapshot();
        if let Some(s) = self.pres.shape_mut(self.slide, id) {
            s.name = name.to_string();
        }
        self.push_undo(before);
        true
    }

    /// The selected shape's alternative text and decorative mark.
    pub fn alt_text(&self) -> Option<(String, bool)> {
        self.selected_shape().map(|s| (s.access.alt_text.clone(), s.access.decorative))
    }

    /// Sets the selected shape's alternative text and decorative mark.
    pub fn set_alt_text(&mut self, engine: &mut TextEngine, text: &str, decorative: bool) -> bool {
        let Some(id) = self.selected_shape().map(|s| s.id) else { return false };
        let text = text.trim().to_string();
        self.edit_access(engine, &[id], |a| {
            a.alt_text = text.clone();
            a.decorative = decorative;
        })
    }

    /// Moves a pane row one step up (towards the front) or down, as one undo step.
    pub fn move_in_pane(&mut self, engine: &mut TextEngine, id: ShapeId, up: bool) -> bool {
        if !self.select_in_pane(engine, id, false) {
            return false;
        }
        self.reorder(if up { ZOrder::Forward } else { ZOrder::Backward })
    }

    fn edit_access(&mut self, engine: &mut TextEngine, ids: &[ShapeId], f: impl Fn(&mut ppt_model::Access)) -> bool {
        self.end_text_edit(engine);
        let changed = ids.iter().any(|id| {
            self.pres.shape(self.slide, *id).is_some_and(|s| {
                let mut a = s.access.clone();
                f(&mut a);
                a != s.access
            })
        });
        if !changed {
            return false;
        }
        let before = self.snapshot();
        for id in ids {
            if let Some(s) = self.pres.shape_mut(self.slide, *id) {
                f(&mut s.access);
            }
        }
        // Hidden shapes cannot stay selected.
        let slide = self.slide;
        let pres = &self.pres;
        self.selection.retain(|id| pres.shape(slide, *id).is_some_and(|s| !s.access.hidden));
        // Nor can a hidden group stay entered.
        if let Some(g) = self.entered_group
            && self.members(g).iter().all(|m| self.pres.shape(slide, *m).is_none_or(|s| s.access.hidden))
        {
            self.entered_group = None;
        }
        self.push_undo(before);
        true
    }
}
