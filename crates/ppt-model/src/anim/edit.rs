//! Editing the main sequence: its effects as a flat list, rebuilt into the
//! click group → offset group → effect nesting PowerPoint writes.

use std::collections::HashMap;

use super::{NodeKind, RawXml, Target, TimeNode, Timing, active_dur, find_main_seq, visit_behaviors};
use crate::ShapeId;

/// What starts an effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Click,
    With,
    After,
}

impl Trigger {
    pub fn node_type(self) -> &'static str {
        match self {
            Trigger::Click => "clickEffect",
            Trigger::With => "withEffect",
            Trigger::After => "afterEffect",
        }
    }

    pub fn of(effect: &TimeNode) -> Self {
        match effect.node_type.as_str() {
            "withEffect" => Trigger::With,
            "afterEffect" => Trigger::After,
            _ => Trigger::Click,
        }
    }
}

fn is_effect(n: &TimeNode) -> bool {
    matches!(n.node_type.as_str(), "clickEffect" | "withEffect" | "afterEffect")
}

fn collect<'a>(n: &'a TimeNode, out: &mut Vec<&'a TimeNode>) {
    if is_effect(n) {
        out.push(n);
        return;
    }
    for c in &n.children {
        collect(c, out);
    }
}

fn find_main_seq_mut(n: &mut TimeNode) -> Option<&mut TimeNode> {
    if n.node_type == "mainSeq" {
        return Some(n);
    }
    n.children.iter_mut().find_map(find_main_seq_mut)
}

/// The main sequence for `effects`: a click group per click, and within it an
/// offset group per "after previous" effect, starting when the one before ends.
fn main_seq(effects: Vec<TimeNode>) -> TimeNode {
    let mut groups: Vec<TimeNode> = Vec::new();
    for (i, e) in effects.into_iter().enumerate() {
        let trigger = Trigger::of(&e);
        if trigger == Trigger::Click || groups.is_empty() {
            groups.push(TimeNode { click: true, auto_begin: i == 0 && trigger != Trigger::Click, fill_hold: true, ..Default::default() });
        }
        let group = groups.last_mut().expect("a group was just ensured");
        if trigger != Trigger::With || group.children.is_empty() {
            let start = group.children.iter().map(|g| g.delay_ms + active_dur(g)).fold(0.0, f64::max);
            group.children.push(TimeNode { delay_ms: start, fill_hold: true, ..Default::default() });
        }
        group.children.last_mut().expect("an offset group was just ensured").children.push(e);
    }
    TimeNode { kind: NodeKind::Seq, node_type: "mainSeq".into(), children: groups, ..Default::default() }
}

/// The shapes or groups an effect animates.
pub fn effect_elements(effect: &TimeNode) -> Vec<ShapeId> {
    let mut out = Vec::new();
    visit_behaviors(effect, &mut |t, _| {
        if !out.contains(&t.element) {
            out.push(t.element);
        }
    });
    out
}

/// How long an effect plays (ms).
pub fn effect_duration(effect: &TimeNode) -> f64 {
    active_dur(effect)
}

/// Stretches every time inside an effect so it plays for `ms`. Instant
/// settings (1 ms) stay instant.
pub fn set_effect_duration(effect: &mut TimeNode, ms: f64) {
    let current = active_dur(effect);
    if current <= 1.0 || ms <= 0.0 {
        return;
    }
    let f = ms / current;
    fn scale(n: &mut TimeNode, f: f64, top: bool) {
        if let Some(d) = n.dur_ms.as_mut().filter(|d| **d > 1.0) {
            *d *= f;
        }
        if !top {
            n.delay_ms *= f;
        }
        for c in &mut n.children {
            scale(c, f, false);
        }
    }
    scale(effect, f, true);
    if let Some(src) = &mut effect.source {
        src.time_scale *= f;
    }
}

impl Timing {
    /// Timing with an empty root, ready for effects.
    pub fn new() -> Self {
        Timing { root: TimeNode { node_type: "tmRoot".into(), ..Default::default() }, ..Default::default() }
    }

    /// The main sequence's effects, in playing order.
    pub fn effects(&self) -> Vec<&TimeNode> {
        let mut out = Vec::new();
        if let Some(main) = find_main_seq(&self.root) {
            for c in &main.children {
                collect(c, &mut out);
            }
        }
        out
    }

    /// Replaces the main sequence with `effects` (their `node_type` is the trigger).
    pub fn set_effects(&mut self, effects: Vec<TimeNode>) {
        let main = main_seq(effects);
        match find_main_seq_mut(&mut self.root) {
            Some(slot) => *slot = main,
            None => {
                if self.root.node_type.is_empty() {
                    self.root.node_type = "tmRoot".into();
                }
                self.root.children.insert(0, main);
            }
        }
    }

    /// No effects and nothing else to play: the slide needs no timing.
    pub fn is_empty(&self) -> bool {
        self.effects().is_empty() && self.root.children.iter().all(|c| c.node_type == "mainSeq")
    }

    /// Points every target at new shape ids (a duplicated slide).
    pub fn remap(&mut self, map: &HashMap<ShapeId, ShapeId>) {
        fn walk(n: &mut TimeNode, map: &HashMap<ShapeId, ShapeId>) {
            if let NodeKind::Behavior(t, _) = &mut n.kind {
                t.element = map.get(&t.element).copied().unwrap_or(t.element);
                for s in &mut t.shapes {
                    *s = map.get(s).copied().unwrap_or(*s);
                }
            }
            for c in &mut n.children {
                walk(c, map);
            }
        }
        walk(&mut self.root, map);
        for v in self.spids.values_mut() {
            *v = map.get(v).copied().unwrap_or(*v);
        }
        for b in &mut self.builds {
            b.element = map.get(&b.element).copied().unwrap_or(b.element);
        }
    }

    /// Runs `f` on every behavior's target.
    pub fn targets_mut(&mut self, mut f: impl FnMut(&mut Target)) {
        fn walk(n: &mut TimeNode, f: &mut impl FnMut(&mut Target)) {
            if let NodeKind::Behavior(t, _) = &mut n.kind {
                f(t);
            }
            for c in &mut n.children {
                walk(c, f);
            }
        }
        walk(&mut self.root, &mut f);
    }

    /// Removes effects (and other sequences) animating an element `keep` rejects.
    /// Returns whether anything was removed.
    pub fn retain_elements(&mut self, keep: impl Fn(ShapeId) -> bool) -> bool {
        let effects: Vec<TimeNode> = self.effects().into_iter().cloned().collect();
        let n = effects.len();
        let kept: Vec<TimeNode> = effects.into_iter().filter(|e| effect_elements(e).into_iter().all(&keep)).collect();
        let mut changed = kept.len() != n;
        if changed {
            self.set_effects(kept);
        }
        let before = self.root.children.len();
        self.root.children.retain(|c| c.node_type == "mainSeq" || effect_elements(c).into_iter().all(&keep));
        changed |= self.root.children.len() != before;
        self.builds.retain(|b| keep(b.element));
        changed
    }
}

impl RawXml {
    pub fn new(xml: &str) -> Self {
        RawXml { xml: xml.into(), time_scale: 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Behavior, EffectMode, Player};
    use super::*;

    fn fade(id: u32, trigger: Trigger, delay: f64) -> TimeNode {
        let target = Target { element: ShapeId(id), shapes: vec![ShapeId(id)], paragraphs: None };
        let b = TimeNode { kind: NodeKind::Behavior(target, Behavior::Effect { mode: EffectMode::In, filter: "fade".into() }), dur_ms: Some(500.0), ..Default::default() };
        TimeNode { node_type: trigger.node_type().into(), delay_ms: delay, children: vec![b], fill_hold: true, ..Default::default() }
    }

    #[test]
    fn effects_round_trip_through_the_nesting() {
        let mut t = Timing::new();
        t.set_effects(vec![fade(1, Trigger::Click, 0.0), fade(2, Trigger::With, 0.0), fade(3, Trigger::After, 250.0), fade(4, Trigger::Click, 0.0)]);
        let main = find_main_seq(&t.root).unwrap();
        assert_eq!(main.children.len(), 2);
        // Click 1: [fade 1, fade 2] then, 500 ms in, [fade 3].
        let g = &main.children[0];
        assert!(g.click && !g.auto_begin);
        assert_eq!(g.children.iter().map(|o| (o.delay_ms, o.children.len())).collect::<Vec<_>>(), vec![(0.0, 2), (500.0, 1)]);
        assert_eq!(t.effects().len(), 4);
        let p = Player::new(&t);
        assert_eq!(p.clicks(), 2);
        assert_eq!(p.duration(1), 500.0 + 250.0 + 500.0);
        // Starting with "after previous" plays with the slide.
        t.set_effects(vec![fade(1, Trigger::After, 0.0)]);
        assert_eq!(Player::new(&t).clicks(), 0);
    }

    #[test]
    fn durations_scale_and_elements_prune() {
        let mut e = fade(1, Trigger::Click, 100.0);
        e.source = Some(RawXml::new("<p:par/>"));
        set_effect_duration(&mut e, 2000.0);
        assert_eq!((effect_duration(&e), e.delay_ms, e.source.as_ref().unwrap().time_scale), (2000.0, 100.0, 4.0));
        let mut t = Timing::new();
        t.set_effects(vec![fade(1, Trigger::Click, 0.0), fade(2, Trigger::Click, 0.0)]);
        assert!(t.retain_elements(|id| id != ShapeId(1)));
        assert_eq!(effect_elements(t.effects()[0]), vec![ShapeId(2)]);
        assert!(!t.retain_elements(|_| true));
        t.remap(&HashMap::from([(ShapeId(2), ShapeId(7))]));
        assert_eq!(effect_elements(t.effects()[0]), vec![ShapeId(7)]);
    }
}
