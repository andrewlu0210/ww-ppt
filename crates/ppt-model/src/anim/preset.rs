//! Built-in effects the editor can add, made of the same behaviors PowerPoint
//! writes for them so the saved file plays (and lists) the same there.

use super::{Behavior, EffectMode, Key, NodeKind, Target, TimeNode, Value};

/// An effect the editor offers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    /// `entr`, `exit` or `emph`.
    pub class: &'static str,
    pub id: u32,
    /// Default option (direction, ...).
    pub subtype: u32,
    pub dur_ms: f64,
    /// Whether the effect has a direction (`subtype` 1 top, 2 right, 4 bottom, 8 left).
    pub directional: bool,
}

const fn p(class: &'static str, id: u32, subtype: u32, dur_ms: f64, directional: bool) -> Preset {
    Preset { class, id, subtype, dur_ms, directional }
}

pub const PRESETS: &[Preset] = &[
    p("entr", 1, 0, 1.0, false),    // appear
    p("entr", 10, 0, 500.0, false), // fade
    p("entr", 2, 4, 500.0, true),   // fly in
    p("entr", 22, 4, 500.0, true),  // wipe
    p("entr", 53, 16, 500.0, false), // zoom
    p("entr", 16, 37, 500.0, false), // split
    p("entr", 3, 10, 500.0, false), // blinds
    p("entr", 4, 16, 500.0, false), // box
    p("entr", 6, 16, 2000.0, false), // shape (circle)
    p("entr", 21, 1, 2000.0, false), // wheel
    p("entr", 14, 10, 500.0, false), // random bars
    p("entr", 5, 10, 500.0, false), // checkerboard
    p("entr", 9, 0, 500.0, false),  // dissolve
    p("emph", 6, 0, 2000.0, false), // grow / shrink
    p("emph", 8, 0, 2000.0, false), // spin
    p("exit", 1, 0, 1.0, false),    // disappear
    p("exit", 10, 0, 500.0, false), // fade
    p("exit", 2, 4, 500.0, true),   // fly out
    p("exit", 22, 4, 500.0, true),  // wipe
    p("exit", 53, 16, 500.0, false), // zoom
    p("exit", 16, 21, 500.0, false), // split
    p("exit", 21, 1, 2000.0, false), // wheel
    p("exit", 14, 10, 500.0, false), // random bars
    p("exit", 9, 0, 500.0, false),  // dissolve
];

pub fn find(class: &str, id: u32) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.class == class && p.id == id)
}

fn behavior(t: &Target, b: Behavior, delay: f64, dur: f64, hold: bool) -> TimeNode {
    TimeNode { kind: NodeKind::Behavior(t.clone(), b), delay_ms: delay, dur_ms: Some(dur), fill_hold: hold, ..Default::default() }
}

fn visibility(t: &Target, visible: bool, at: f64) -> TimeNode {
    let to = Value::Str(if visible { "visible" } else { "hidden" }.into());
    behavior(t, Behavior::Set { attr: "style.visibility".into(), to }, at, 1.0, true)
}

fn filter(t: &Target, mode: EffectMode, filter: &str, dur: f64) -> TimeNode {
    behavior(t, Behavior::Effect { mode, filter: filter.into() }, 0.0, dur, false)
}

fn anim(t: &Target, attr: &str, from: Value, to: Value, dur: f64) -> TimeNode {
    let keys = vec![Key { t: 0.0, value: from, fmla: None }, Key { t: 1.0, value: to, fmla: None }];
    behavior(t, Behavior::Anim { attr: attr.into(), keys, discrete: false }, 0.0, dur, true)
}

fn s(v: &str) -> Value {
    Value::Str(v.into())
}

/// Where a shape starts (or ends) when flying from (or to) `subtype`'s edge.
fn off_slide(subtype: u32) -> (&'static str, &'static str) {
    match subtype {
        1 => ("#ppt_x", "0-#ppt_h/2"),
        2 => ("1+#ppt_w/2", "#ppt_y"),
        8 => ("0-#ppt_w/2", "#ppt_y"),
        _ => ("#ppt_x", "1+#ppt_h/2"),
    }
}

/// The wipe filter that starts at `subtype`'s edge; exits play it backwards.
fn wipe(subtype: u32, exit: bool) -> &'static str {
    match (subtype, exit) {
        (1, false) | (4, true) => "wipe(down)",
        (2, false) | (8, true) => "wipe(left)",
        (8, false) | (2, true) => "wipe(right)",
        _ => "wipe(up)",
    }
}

/// The effect `class`/`id` on `target`, with option `subtype` (the preset's
/// default when `None`). Its trigger is click; callers set `node_type`.
pub fn build_effect(class: &str, id: u32, subtype: Option<u32>, target: &Target) -> Option<TimeNode> {
    let preset = find(class, id)?;
    let sub = subtype.filter(|s| preset.directional && matches!(s, 1 | 2 | 4 | 8)).unwrap_or(preset.subtype);
    let d = preset.dur_ms;
    let t = target;
    let children = match (class, id) {
        ("entr", 1) => vec![visibility(t, true, 0.0)],
        ("entr", 2) => {
            let (x, y) = off_slide(sub);
            vec![visibility(t, true, 0.0), anim(t, "ppt_x", s(x), s("#ppt_x"), d), anim(t, "ppt_y", s(y), s("#ppt_y"), d)]
        }
        ("entr", 53) => vec![
            visibility(t, true, 0.0),
            anim(t, "ppt_w", Value::Num(0.0), s("#ppt_w"), d),
            anim(t, "ppt_h", Value::Num(0.0), s("#ppt_h"), d),
            filter(t, EffectMode::In, "fade", d),
        ],
        ("entr", _) => {
            let f = match id {
                10 => "fade",
                22 => wipe(sub, false),
                16 => "barn(outVertical)",
                3 => "blinds(horizontal)",
                4 => "box(in)",
                6 => "circle(in)",
                21 => "wheel(1)",
                14 => "randombar(horizontal)",
                5 => "checkerboard(across)",
                _ => "dissolve",
            };
            vec![visibility(t, true, 0.0), filter(t, EffectMode::In, f, d)]
        }
        ("emph", 6) => vec![behavior(t, Behavior::Scale { from: None, to: None, by: Some((1.5, 1.5)) }, 0.0, d, true)],
        ("emph", _) => vec![behavior(t, Behavior::Rotate { from: None, to: None, by: Some(360.0) }, 0.0, d, true)],
        ("exit", 1) => vec![visibility(t, false, 0.0)],
        ("exit", 2) => {
            let (x, y) = off_slide(sub);
            vec![anim(t, "ppt_x", s("#ppt_x"), s(x), d), anim(t, "ppt_y", s("#ppt_y"), s(y), d), visibility(t, false, d - 1.0)]
        }
        ("exit", 53) => vec![
            anim(t, "ppt_w", s("#ppt_w"), Value::Num(0.0), d),
            anim(t, "ppt_h", s("#ppt_h"), Value::Num(0.0), d),
            filter(t, EffectMode::Out, "fade", d),
            visibility(t, false, d - 1.0),
        ],
        _ => {
            let f = match id {
                10 => "fade",
                22 => wipe(sub, true),
                16 => "barn(inVertical)",
                21 => "wheel(1)",
                14 => "randombar(horizontal)",
                _ => "dissolve",
            };
            vec![filter(t, EffectMode::Out, f, d), visibility(t, false, d - 1.0)]
        }
    };
    Some(TimeNode {
        preset_class: class.into(),
        preset_id: Some(id),
        preset_subtype: Some(sub),
        node_type: "clickEffect".into(),
        fill_hold: true,
        children,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Player, Stage, Timing};
    use super::*;
    use crate::ShapeId;

    struct OneShape;
    impl Stage for OneShape {
        fn slide_size(&self) -> (f64, f64) {
            (1000.0, 500.0)
        }
        fn frame(&self, _: ShapeId) -> Option<(f64, f64, f64, f64)> {
            Some((450.0, 200.0, 100.0, 100.0))
        }
        fn paragraph_count(&self, _: ShapeId) -> usize {
            1
        }
    }

    #[test]
    fn every_preset_builds_and_ends_in_its_state() {
        let target = Target { element: ShapeId(1), shapes: vec![ShapeId(1)], paragraphs: None };
        for p in PRESETS {
            let e = build_effect(p.class, p.id, None, &target).unwrap();
            assert_eq!(super::super::edit::effect_duration(&e), p.dur_ms, "{p:?}");
            let mut t = Timing::new();
            t.set_effects(vec![e]);
            let player = Player::new(&t);
            let start = player.frame(&OneShape, 0, 0.0);
            let end = player.frame(&OneShape, 1, 1e9);
            let visible = |f: &super::super::Frame| f.shapes.get(&ShapeId(1)).is_none_or(|s| s.visible);
            match p.class {
                "entr" => assert!(!visible(&start) && visible(&end), "{p:?}"),
                "exit" => assert!(visible(&start) && !visible(&end), "{p:?}"),
                _ => assert!(visible(&end)),
            }
            if p.class == "entr" {
                let s = &end.shapes[&ShapeId(1)];
                assert!(s.dx.abs() < 1e-9 && s.dy.abs() < 1e-9 && (s.sx - 1.0).abs() < 1e-9 && s.opacity == 1.0 && s.reveal.is_none(), "{p:?}: {s:?}");
            }
        }
        // Fly in from the left starts off the slide's left edge.
        let e = build_effect("entr", 2, Some(8), &target).unwrap();
        let mut t = Timing::new();
        t.set_effects(vec![e]);
        let s = &Player::new(&t).frame(&OneShape, 1, 0.0).shapes[&ShapeId(1)];
        assert!((s.dx - (-50.0 - 500.0)).abs() < 1e-6, "{s:?}");
    }
}
