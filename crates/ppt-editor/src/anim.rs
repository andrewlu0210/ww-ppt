//! Animation commands: the current slide's main sequence edited as a list of
//! effects, each an undo step.

use std::sync::Arc;

use ppt_model::ShapeId;
use ppt_model::anim::{NodeKind, Target, TimeNode, Timing, Trigger, build_effect, effect_duration, effect_elements, set_effect_duration};

use crate::Editor;

/// The first behavior's target: what an effect animates.
fn effect_target(e: &TimeNode) -> Option<Target> {
    if let NodeKind::Behavior(t, _) = &e.kind {
        return Some(t.clone());
    }
    e.children.iter().find_map(effect_target)
}

impl Editor {
    /// The current slide's effects, in order.
    pub fn animations(&self) -> Vec<&TimeNode> {
        self.pres.slides.get(self.slide).and_then(|s| s.timing.as_deref()).map(|t| t.effects()).unwrap_or_default()
    }

    /// Edits the current slide's effect list as one undo step; `f` returns whether it changed anything.
    fn edit_animations(&mut self, f: impl FnOnce(&mut Vec<TimeNode>) -> bool) -> bool {
        let mut effects: Vec<TimeNode> = self.animations().into_iter().cloned().collect();
        if !f(&mut effects) {
            return false;
        }
        let before = self.snapshot();
        let Some(slide) = self.pres.slide_mut(self.slide) else { return false };
        let mut timing = slide.timing.as_deref().cloned().unwrap_or_else(Timing::new);
        timing.set_effects(effects);
        slide.timing = (!timing.is_empty()).then(|| Arc::new(timing));
        self.push_undo(before);
        true
    }

    /// What an animation of the selection targets: each selected shape, or a
    /// selected group as a whole. Shapes inside an entered group can't be animated.
    fn animation_targets(&self) -> Vec<Target> {
        let shapes = self.current_shapes();
        let mut out: Vec<Target> = Vec::new();
        for s in shapes.iter().filter(|s| self.selection.contains(&s.id)) {
            if s.group.is_some() && s.group == self.entered_group {
                continue;
            }
            let element = s.group.unwrap_or(s.id);
            if out.iter().any(|t| t.element == element) {
                continue;
            }
            let members = shapes.iter().filter(|m| m.id == element || m.group == Some(element)).map(|m| m.id).collect();
            out.push(Target { element, shapes: members, paragraphs: None });
        }
        out
    }

    /// Whether the selection can get an animation.
    pub fn can_animate(&self) -> bool {
        !self.animation_targets().is_empty()
    }

    /// Selects what effect `index` animates.
    pub fn select_animation(&mut self, engine: &mut ppt_text::TextEngine, index: usize) -> bool {
        let Some(elements) = self.animations().get(index).map(|e| effect_elements(e)) else { return false };
        self.end_text_edit(engine);
        let shapes = self.current_shapes();
        self.selection = shapes.iter().filter(|s| elements.contains(&s.id) || s.group.is_some_and(|g| elements.contains(&g))).map(|s| s.id).collect();
        self.entered_group = None;
        self.cell_range = None;
        true
    }

    /// Adds effect `class`/`id` to every selected shape: the first on click,
    /// the others with it. Returns whether anything was added.
    pub fn add_animation(&mut self, class: &str, id: u32, subtype: Option<u32>) -> bool {
        let targets = self.animation_targets();
        if targets.is_empty() {
            return false;
        }
        self.edit_animations(|effects| {
            let mut added = false;
            for t in targets {
                let Some(mut e) = build_effect(class, id, subtype, &t) else { continue };
                e.grp_id = Some(free_grp_id(effects, t.element));
                e.node_type = if added { Trigger::With } else { Trigger::Click }.node_type().into();
                effects.push(e);
                added = true;
            }
            added
        })
    }

    /// Replaces effect `index` with `class`/`id`, keeping what starts it and its
    /// target. A new direction of the same effect keeps its duration.
    pub fn change_animation(&mut self, index: usize, class: &str, id: u32, subtype: Option<u32>) -> bool {
        self.edit_animations(|effects| {
            let Some(old) = effects.get(index) else { return false };
            let Some(target) = effect_target(old) else { return false };
            let Some(mut e) = build_effect(class, id, subtype, &target) else { return false };
            if old.preset_class == e.preset_class && old.preset_id == e.preset_id {
                if old.preset_subtype == e.preset_subtype {
                    return false;
                }
                set_effect_duration(&mut e, effect_duration(old));
            }
            e.node_type = old.node_type.clone();
            e.delay_ms = old.delay_ms;
            e.grp_id = old.grp_id;
            effects[index] = e;
            true
        })
    }

    pub fn remove_animation(&mut self, index: usize) -> bool {
        self.edit_animations(|effects| {
            if index >= effects.len() {
                return false;
            }
            effects.remove(index);
            true
        })
    }

    /// Moves effect `from` so it ends up at index `to`.
    pub fn move_animation(&mut self, from: usize, to: usize) -> bool {
        self.edit_animations(|effects| {
            if from >= effects.len() || to >= effects.len() || from == to {
                return false;
            }
            let e = effects.remove(from);
            effects.insert(to, e);
            true
        })
    }

    pub fn set_animation_trigger(&mut self, index: usize, trigger: Trigger) -> bool {
        self.edit_animations(|effects| {
            let Some(e) = effects.get_mut(index).filter(|e| Trigger::of(e) != trigger) else { return false };
            e.node_type = trigger.node_type().into();
            true
        })
    }

    /// Sets effect `index`'s delay and/or duration (ms).
    pub fn set_animation_timing(&mut self, index: usize, delay_ms: Option<f64>, dur_ms: Option<f64>) -> bool {
        self.edit_animations(|effects| {
            let Some(e) = effects.get_mut(index) else { return false };
            let mut changed = false;
            if let Some(d) = delay_ms.map(|d| d.max(0.0)).filter(|d| (d - e.delay_ms).abs() >= 0.5) {
                e.delay_ms = d;
                changed = true;
            }
            if let Some(d) = dur_ms.filter(|d| *d > 0.0 && (d - effect_duration(e)).abs() >= 0.5) {
                let before = effect_duration(e);
                set_effect_duration(e, d);
                changed |= effect_duration(e) != before;
            }
            changed
        })
    }

    /// After shapes were deleted, grouped or ungrouped: drops effects on shapes
    /// that are gone or now inside a group, and follows group membership.
    pub(crate) fn prune_animations(&mut self) {
        let Some(slide) = self.pres.slide_mut(self.slide) else { return };
        let Some(timing) = slide.timing.as_deref() else { return };
        let shapes = &slide.shapes;
        let keep = |e: ShapeId| shapes.iter().any(|s| (s.id == e && s.group.is_none()) || s.group == Some(e));
        let mut t = timing.clone();
        t.retain_elements(keep);
        t.targets_mut(|target| {
            if target.shapes.iter().any(|id| !shapes.iter().any(|s| s.id == *id)) {
                target.shapes = shapes.iter().filter(|s| s.id == target.element || s.group == Some(target.element)).map(|s| s.id).collect();
            }
        });
        if &t != timing {
            slide.timing = (!t.is_empty()).then(|| Arc::new(t));
        }
    }
}

/// The lowest build group id no other effect on `element` uses.
fn free_grp_id(effects: &[TimeNode], element: ShapeId) -> u32 {
    let used: Vec<u32> = effects.iter().filter(|e| effect_elements(e).contains(&element)).filter_map(|e| e.grp_id).collect();
    (0..).find(|g| !used.contains(g)).unwrap_or(0)
}
