//! Slide animations (`p:timing`): the time node tree as PowerPoint stores it,
//! and a player that computes every animated shape's state for a given number
//! of clicks and the time since the last one.
//!
//! Effects are not modeled by name: the generic behaviors they are made of
//! (set, interpolate, filter, scale, rotate, motion path, color) are played,
//! so effects without special support still work.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{Color, ShapeId};

mod edit;
mod preset;

pub use edit::Trigger;
pub use edit::{effect_duration, effect_elements, set_effect_duration};
pub use preset::{PRESETS, Preset, build_effect};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timing {
    pub root: TimeNode,
    /// Shape ids (`spid`) of the loaded XML → the shape or group they name,
    /// to rewrite ids inside kept original XML.
    pub spids: HashMap<u32, ShapeId>,
    /// The loaded build list (`p:bldLst` entries).
    pub builds: Vec<Build>,
}

/// A `p:bldP` / `p:bldGraphic` entry: how a shape's text builds for the
/// effects with the same group id.
#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub element: ShapeId,
    pub grp_id: u32,
    pub xml: Arc<str>,
}

/// An element's original XML, kept so effects that were not edited save as they were.
#[derive(Clone, Debug, PartialEq)]
pub struct RawXml {
    pub xml: Arc<str>,
    /// Factor applied to every time inside when saving (duration edits).
    pub time_scale: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimeNode {
    pub kind: NodeKind,
    /// Begins on a click (`delay="indefinite"`).
    pub click: bool,
    /// Begins on its own as well (an `evt="onBegin"` condition): effects that
    /// start with the slide.
    pub auto_begin: bool,
    pub delay_ms: f64,
    /// Simple duration; `None` for indefinite or unspecified.
    pub dur_ms: Option<f64>,
    pub repeat: f64,
    pub auto_rev: bool,
    /// Fractions of the duration spent accelerating / decelerating.
    pub accel: f64,
    pub decel: f64,
    pub fill_hold: bool,
    /// `clickEffect`, `withEffect`, `afterEffect`, `mainSeq`, ...
    pub node_type: String,
    /// `entr`, `exit`, `emph`, `path`, ...
    pub preset_class: String,
    pub preset_id: Option<u32>,
    pub preset_subtype: Option<u32>,
    /// Links an effect to its build list entry.
    pub grp_id: Option<u32>,
    pub children: Vec<TimeNode>,
    /// The loaded XML of effects and of top-level sequences other than the main one.
    pub source: Option<RawXml>,
}

impl Default for TimeNode {
    fn default() -> Self {
        Self {
            kind: NodeKind::Par,
            click: false,
            auto_begin: false,
            delay_ms: 0.0,
            dur_ms: None,
            repeat: 1.0,
            auto_rev: false,
            accel: 0.0,
            decel: 0.0,
            fill_hold: false,
            node_type: String::new(),
            preset_class: String::new(),
            preset_id: None,
            preset_subtype: None,
            grp_id: None,
            children: Vec::new(),
            source: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    Par,
    Seq,
    Behavior(Target, Behavior),
    /// Nodes we don't play (commands, media, ...).
    Other,
}

/// What a behavior animates: shapes (several for a group) and optionally a
/// range of paragraphs of their text.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// The shape, or the group (by its group id), the XML names.
    pub element: ShapeId,
    pub shapes: Vec<ShapeId>,
    pub paragraphs: Option<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    /// A string: a keyword (`visible`) or a formula (`#ppt_y+0.1`).
    Str(String),
    Color(Color),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    /// Position in the behavior's duration, 0..=1.
    pub t: f64,
    pub value: Value,
    /// Formula applied to the value (`$` is the value), e.g. for bounces.
    pub fmla: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectMode {
    In,
    Out,
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Behavior {
    Set { attr: String, to: Value },
    Anim { attr: String, keys: Vec<Key>, discrete: bool },
    Effect { mode: EffectMode, filter: String },
    /// Scale factors (1.0 = 100%).
    Scale { from: Option<(f64, f64)>, to: Option<(f64, f64)>, by: Option<(f64, f64)> },
    /// Degrees.
    Rotate { from: Option<f64>, to: Option<f64>, by: Option<f64> },
    /// Points of the flattened path, as fractions of the slide size, relative to the shape.
    Motion { points: Vec<(f64, f64)> },
    Color { attr: String, to: Option<Color> },
}

// --- Formulas ----------------------------------------------------------------

/// Variables for animation formulas: the shape's center and size as fractions
/// of the slide, and `$` (the value a `fmla` transforms).
#[derive(Clone, Copy, Debug, Default)]
pub struct FormulaVars {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub value: f64,
}

/// Evaluates a PowerPoint animation formula (`#ppt_x-#ppt_w/2`, `1+#ppt_h/2`,
/// `$*sin(...)`); `None` when it doesn't parse.
pub fn eval_formula(s: &str, v: &FormulaVars) -> Option<f64> {
    let mut p = FormulaParser { s: s.as_bytes(), i: 0, v };
    let r = p.expr()?;
    p.skip_ws();
    (p.i == p.s.len()).then_some(r)
}

struct FormulaParser<'a> {
    s: &'a [u8],
    i: usize,
    v: &'a FormulaVars,
}

impl FormulaParser<'_> {
    fn skip_ws(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.skip_ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Option<f64> {
        let mut a = self.term()?;
        loop {
            if self.eat(b'+') {
                a += self.term()?;
            } else if self.eat(b'-') {
                a -= self.term()?;
            } else {
                return Some(a);
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut a = self.power()?;
        loop {
            if self.eat(b'*') {
                a *= self.power()?;
            } else if self.eat(b'/') {
                a /= self.power()?;
            } else if self.eat(b'%') {
                a %= self.power()?;
            } else {
                return Some(a);
            }
        }
    }

    fn power(&mut self) -> Option<f64> {
        let a = self.unary()?;
        if self.eat(b'^') { Some(a.powf(self.power()?)) } else { Some(a) }
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat(b'-') {
            return Some(-self.unary()?);
        }
        if self.eat(b'+') {
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Option<f64> {
        self.skip_ws();
        if self.eat(b'(') {
            let a = self.expr()?;
            return self.eat(b')').then_some(a);
        }
        if self.eat(b'$') {
            return Some(self.v.value);
        }
        let start = self.i;
        if self.s.get(self.i) == Some(&b'#') {
            self.i += 1;
        }
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'.') {
            self.i += 1;
        }
        let word = std::str::from_utf8(&self.s[start..self.i]).ok()?;
        if let Ok(n) = word.parse::<f64>() {
            return Some(n);
        }
        let v = self.v;
        match word {
            "#ppt_x" | "ppt_x" => return Some(v.x),
            "#ppt_y" | "ppt_y" => return Some(v.y),
            "#ppt_w" | "ppt_w" => return Some(v.w),
            "#ppt_h" | "ppt_h" => return Some(v.h),
            "pi" => return Some(std::f64::consts::PI),
            "e" => return Some(std::f64::consts::E),
            _ => {}
        }
        // Functions.
        if !self.eat(b'(') {
            return None;
        }
        let mut args = vec![self.expr()?];
        while self.eat(b',') {
            args.push(self.expr()?);
        }
        if !self.eat(b')') {
            return None;
        }
        let a = args[0];
        Some(match word {
            "abs" => a.abs(),
            "sqrt" => a.sqrt(),
            "sin" => a.sin(),
            "cos" => a.cos(),
            "tan" => a.tan(),
            "asin" => a.asin(),
            "acos" => a.acos(),
            "atan" => a.atan(),
            "ln" => a.ln(),
            "floor" => a.floor(),
            "ceil" => a.ceil(),
            "max" => args.iter().copied().fold(f64::MIN, f64::max),
            "min" => args.iter().copied().fold(f64::MAX, f64::min),
            // Deterministic stand-in for PowerPoint's random number.
            "rand" => a * 0.5,
            _ => return None,
        })
    }
}

// --- Player ------------------------------------------------------------------

/// An animated shape's state relative to its normal appearance.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeState {
    pub visible: bool,
    pub opacity: f64,
    /// Offset in EMU.
    pub dx: f64,
    pub dy: f64,
    pub sx: f64,
    pub sy: f64,
    /// Extra rotation in degrees.
    pub rot: f64,
    /// Scaling and rotation happen around this point (EMU): the shape's, or its group's, center.
    pub pivot: (f64, f64),
    /// A filter effect in progress: (filter, revealed fraction).
    pub reveal: Option<(String, f64)>,
    pub fill: Option<Color>,
    pub line: Option<Color>,
}

impl ShapeState {
    fn new(visible: bool, pivot: (f64, f64)) -> Self {
        Self { visible, opacity: 1.0, dx: 0.0, dy: 0.0, sx: 1.0, sy: 1.0, rot: 0.0, pivot, reveal: None, fill: None, line: None }
    }

    /// Whether the shape looks exactly as it does without animation.
    pub fn is_identity(&self) -> bool {
        *self == ShapeState::new(true, self.pivot)
    }
}

/// State of one paragraph of an animated text (only visibility and fading).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParaState {
    pub visible: bool,
    pub opacity: f64,
}

/// Every animated shape's state at one moment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub shapes: HashMap<ShapeId, ShapeState>,
    pub paragraphs: HashMap<(ShapeId, usize), ParaState>,
}

/// Geometry the player needs: the slide size and each shape's frame (EMU).
pub trait Stage {
    fn slide_size(&self) -> (f64, f64);
    /// (x, y, w, h) of the shape, unrotated.
    fn frame(&self, id: ShapeId) -> Option<(f64, f64, f64, f64)>;
    /// Number of paragraphs in the shape's text.
    fn paragraph_count(&self, id: ShapeId) -> usize;
}

/// [`Stage`] for a slide of a presentation.
pub struct SlideStage<'a> {
    pub pres: &'a crate::Presentation,
    pub slide: &'a crate::Slide,
}

impl Stage for SlideStage<'_> {
    fn slide_size(&self) -> (f64, f64) {
        (self.pres.slide_width as f64, self.pres.slide_height as f64)
    }

    fn frame(&self, id: ShapeId) -> Option<(f64, f64, f64, f64)> {
        let x = &self.slide.shapes.iter().find(|s| s.id == id)?.xfrm;
        Some((x.x as f64, x.y as f64, x.w as f64, x.h as f64))
    }

    fn paragraph_count(&self, id: ShapeId) -> usize {
        self.slide.shapes.iter().find(|s| s.id == id).and_then(|s| s.text.as_ref()).map_or(0, |t| t.paragraphs.len())
    }
}

/// One step of the main sequence: what starts with a click (or automatically).
#[derive(Clone, Debug)]
struct Step<'a> {
    node: &'a TimeNode,
    /// Clicks needed before it starts (0: with the slide).
    click: usize,
    /// Start relative to that click (ms).
    offset: f64,
}

/// Plays a slide's timing.
pub struct Player<'a> {
    steps: Vec<Step<'a>>,
    clicks: usize,
    /// Behaviors in document order, for the initial visibility.
    initially_hidden: Vec<(ShapeId, Option<usize>)>,
}

/// Time far beyond any animation: everything has finished.
const END: f64 = 1e12;

fn simple_dur(n: &TimeNode) -> f64 {
    if let Some(d) = n.dur_ms {
        return d;
    }
    match &n.kind {
        NodeKind::Par => n.children.iter().map(|c| c.delay_ms + active_dur(c)).fold(0.0, f64::max),
        NodeKind::Seq => n.children.iter().map(|c| c.delay_ms + active_dur(c)).sum(),
        NodeKind::Behavior(..) | NodeKind::Other => 0.0,
    }
}

pub(crate) fn active_dur(n: &TimeNode) -> f64 {
    simple_dur(n) * n.repeat.max(0.0) * if n.auto_rev { 2.0 } else { 1.0 }
}

/// SMIL acceleration/deceleration of a progress value.
fn ease(p: f64, accel: f64, decel: f64) -> f64 {
    let (a, d) = (accel.clamp(0.0, 1.0), decel.clamp(0.0, 1.0));
    if a + d <= 0.0 || a + d > 1.0 {
        return p;
    }
    let r = 1.0 / (1.0 - a / 2.0 - d / 2.0);
    if p < a {
        r * p * p / (2.0 * a)
    } else if p <= 1.0 - d {
        r * (p - a / 2.0)
    } else {
        let q = 1.0 - p;
        1.0 - r * q * q / (2.0 * d)
    }
}

/// Local simple time (ms) of a node at `t` ms after it began, or `None` when
/// it is not (or no longer) active.
fn local_time(n: &TimeNode, t: f64) -> Option<f64> {
    if t < 0.0 {
        return None;
    }
    let simple = simple_dur(n);
    let active = active_dur(n);
    let t = if t >= active {
        if !n.fill_hold && !matches!(n.kind, NodeKind::Par | NodeKind::Seq) {
            return None;
        }
        // Containers stay so their held children can be evaluated at their end.
        active
    } else {
        t
    };
    if simple <= 0.0 {
        return Some(0.0);
    }
    let cycle = if n.auto_rev { 2.0 * simple } else { simple };
    let mut it = if t >= active { if n.auto_rev { 0.0 } else { simple } } else { t % cycle };
    if n.auto_rev && it > simple {
        it = cycle - it;
    }
    Some(ease(it / simple, n.accel, n.decel) * simple)
}

impl<'a> Player<'a> {
    pub fn new(timing: &'a Timing) -> Self {
        let mut steps = Vec::new();
        let mut clicks = 0;
        if let Some(main) = find_main_seq(&timing.root) {
            let mut prev_end = 0.0;
            for (i, child) in main.children.iter().enumerate() {
                let (click, offset) = if child.click && !(i == 0 && child.auto_begin) {
                    clicks += 1;
                    (clicks, child.delay_ms.max(0.0))
                } else if i == 0 {
                    (0, child.delay_ms.max(0.0))
                } else {
                    (clicks, prev_end + child.delay_ms.max(0.0))
                };
                prev_end = offset + active_dur(child);
                steps.push(Step { node: child, click, offset });
            }
        }
        // A target whose first visibility change makes it visible starts hidden.
        let mut first: Vec<(ShapeId, Option<usize>, bool)> = Vec::new();
        for s in &steps {
            visit_behaviors(s.node, &mut |target, b| {
                let visible = match b {
                    Behavior::Set { attr, to: Value::Str(v) } if attr == "style.visibility" => v == "visible",
                    _ => return,
                };
                for &id in &target.shapes {
                    for p in paragraph_keys(target) {
                        if !first.iter().any(|(i, q, _)| *i == id && *q == p) {
                            first.push((id, p, visible));
                        }
                    }
                }
            });
        }
        let initially_hidden = first.into_iter().filter(|f| f.2).map(|(i, p, _)| (i, p)).collect();
        Self { steps, clicks, initially_hidden }
    }

    /// Clicks it takes to play every animation on the slide.
    pub fn clicks(&self) -> usize {
        self.clicks
    }

    /// Whether the effects started by click `click` are still playing `t` ms after it.
    pub fn running(&self, click: usize, t: f64) -> bool {
        self.steps.iter().any(|s| s.click == click && t < s.offset + active_dur(s.node))
    }

    /// When everything started by click `click` has finished (ms after it).
    pub fn duration(&self, click: usize) -> f64 {
        self.steps.iter().filter(|s| s.click == click).map(|s| s.offset + active_dur(s.node)).fold(0.0, f64::max)
    }

    /// The effects in playing order, for the animation list.
    pub fn effects(&self) -> Vec<EffectInfo> {
        let mut out = Vec::new();
        for s in &self.steps {
            // The step's own delay is already part of its offset.
            collect_effects(s.node, s.click, s.offset - s.node.delay_ms, &mut out);
        }
        out
    }

    /// The state when all clicks play one after another from the slide's start,
    /// `t` ms in: the editor's preview. `None` once everything has played.
    pub fn preview_frame(&self, stage: &dyn Stage, t: f64) -> Option<Frame> {
        let mut begin = 0.0;
        for click in 0..=self.clicks {
            let d = self.duration(click);
            if t < begin + d {
                return Some(self.frame(stage, click, t - begin));
            }
            begin += d;
        }
        None
    }

    /// The state after `clicks` clicks, `t` ms after the last one (or after the slide began).
    pub fn frame(&self, stage: &dyn Stage, clicks: usize, t: f64) -> Frame {
        let (sw, sh) = stage.slide_size();
        let mut f = Frame::default();
        for &(id, para) in &self.initially_hidden {
            match para {
                Some(p) => {
                    f.paragraphs.insert((id, p), ParaState { visible: false, opacity: 1.0 });
                }
                None => {
                    let pivot = stage.frame(id).map_or((0.0, 0.0), |(x, y, w, h)| (x + w / 2.0, y + h / 2.0));
                    f.shapes.insert(id, ShapeState::new(false, pivot));
                }
            }
        }
        let mut cx = EvalCx { stage, f: &mut f, sw, sh };
        for s in &self.steps {
            let local = match s.click.cmp(&clicks) {
                std::cmp::Ordering::Less => END,
                std::cmp::Ordering::Equal => t - s.offset,
                std::cmp::Ordering::Greater => continue,
            };
            cx.node(s.node, local);
        }
        f
    }
}

/// One effect of the main sequence, as the animation list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectInfo {
    /// Clicks before it starts (0: with the slide).
    pub click: usize,
    /// `clickEffect`, `withEffect` or `afterEffect`.
    pub trigger: String,
    /// `entr`, `exit`, `emph`, `path`.
    pub class: String,
    pub preset_id: Option<u32>,
    pub shapes: Vec<ShapeId>,
    /// Start relative to its click, and length (ms).
    pub start_ms: f64,
    pub dur_ms: f64,
}

/// Effects are the nodes with a `nodeType` of click/with/after under a step.
fn collect_effects(n: &TimeNode, click: usize, begin: f64, out: &mut Vec<EffectInfo>) {
    if matches!(n.node_type.as_str(), "clickEffect" | "withEffect" | "afterEffect") {
        let mut shapes = Vec::new();
        visit_behaviors(n, &mut |t, _| {
            for id in &t.shapes {
                if !shapes.contains(id) {
                    shapes.push(*id);
                }
            }
        });
        out.push(EffectInfo {
            click,
            trigger: n.node_type.clone(),
            class: n.preset_class.clone(),
            preset_id: n.preset_id,
            shapes,
            start_ms: begin + n.delay_ms,
            dur_ms: active_dur(n),
        });
        return;
    }
    let mut seq_begin = begin + n.delay_ms;
    for c in &n.children {
        match n.kind {
            NodeKind::Seq => {
                collect_effects(c, click, seq_begin, out);
                seq_begin += c.delay_ms + active_dur(c);
            }
            _ => collect_effects(c, click, begin + n.delay_ms, out),
        }
    }
}

pub(crate) fn find_main_seq(n: &TimeNode) -> Option<&TimeNode> {
    if n.node_type == "mainSeq" {
        return Some(n);
    }
    n.children.iter().find_map(find_main_seq)
}

pub(crate) fn visit_behaviors<'a>(n: &'a TimeNode, f: &mut impl FnMut(&'a Target, &'a Behavior)) {
    if let NodeKind::Behavior(t, b) = &n.kind {
        f(t, b);
    }
    for c in &n.children {
        visit_behaviors(c, f);
    }
}

fn paragraph_keys(t: &Target) -> Vec<Option<usize>> {
    match t.paragraphs {
        Some((a, b)) => (a..=b).map(Some).collect(),
        None => vec![None],
    }
}

struct EvalCx<'a, 'f> {
    stage: &'a dyn Stage,
    f: &'f mut Frame,
    sw: f64,
    sh: f64,
}

impl EvalCx<'_, '_> {
    fn node(&mut self, n: &TimeNode, t: f64) {
        let Some(local) = local_time(n, t) else { return };
        match &n.kind {
            NodeKind::Par => {
                for c in &n.children {
                    self.node(c, local - c.delay_ms);
                }
            }
            NodeKind::Seq => {
                let mut begin = 0.0;
                for c in &n.children {
                    begin += c.delay_ms;
                    self.node(c, local - begin);
                    begin += active_dur(c);
                }
            }
            NodeKind::Behavior(target, b) => {
                let simple = simple_dur(n);
                let p = if simple > 0.0 { (local / simple).clamp(0.0, 1.0) } else { 1.0 };
                self.behavior(target, b, p);
            }
            NodeKind::Other => {}
        }
    }

    /// Center and size of the target (all its shapes) as fractions of the slide, and its center in EMU.
    fn target_geometry(&self, t: &Target) -> Option<(FormulaVars, (f64, f64))> {
        let mut b: Option<(f64, f64, f64, f64)> = None;
        for &id in &t.shapes {
            let (x, y, w, h) = self.stage.frame(id)?;
            b = Some(match b {
                None => (x, y, x + w, y + h),
                Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x + w), e.max(y + h)),
            });
        }
        let (x0, y0, x1, y1) = b?;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        Some((FormulaVars { x: cx / self.sw, y: cy / self.sh, w: (x1 - x0) / self.sw, h: (y1 - y0) / self.sh, value: 0.0 }, (cx, cy)))
    }

    fn value(&self, v: &Value, vars: &FormulaVars) -> Option<f64> {
        match v {
            Value::Num(n) => Some(*n),
            Value::Str(s) => eval_formula(s, vars),
            Value::Color(_) => None,
        }
    }

    /// The keyframed value at `p`: interpolated between the surrounding keys, then
    /// passed through the segment's formula (`$` is the interpolated value).
    fn keyed(&self, keys: &[Key], p: f64, discrete: bool, vars: &FormulaVars) -> Option<f64> {
        let mut i = keys.iter().rposition(|k| k.t <= p).unwrap_or(0);
        // At the last key, the value is the end of the segment before it (with that segment's formula).
        if !discrete && i + 1 == keys.len() && i > 0 {
            i -= 1;
        }
        let a = &keys[i];
        let va = self.value(&a.value, vars)?;
        let v = match keys.get(i + 1) {
            Some(b) if !discrete && b.t > a.t => {
                let vb = self.value(&b.value, vars)?;
                va + (vb - va) * ((p - a.t) / (b.t - a.t)).min(1.0)
            }
            _ => va,
        };
        match &a.fmla {
            Some(f) if !f.is_empty() => eval_formula(f, &FormulaVars { value: v, ..*vars }),
            _ => Some(v),
        }
    }

    fn behavior(&mut self, target: &Target, b: &Behavior, p: f64) {
        // Paragraph targets only show, hide and fade.
        if target.paragraphs.is_some() {
            for &id in &target.shapes {
                for para in paragraph_keys(target).into_iter().flatten() {
                    if para >= self.stage.paragraph_count(id) {
                        continue;
                    }
                    let st = self.f.paragraphs.entry((id, para)).or_insert(ParaState { visible: true, opacity: 1.0 });
                    match b {
                        Behavior::Set { attr, to: Value::Str(v) } if attr == "style.visibility" => st.visible = v == "visible",
                        // Any filter on a paragraph plays as a fade.
                        Behavior::Effect { mode, .. } if mode != &EffectMode::None => {
                            st.opacity = if p >= 1.0 { 1.0 } else if *mode == EffectMode::In { p } else { 1.0 - p };
                        }
                        _ => {}
                    }
                }
            }
            return;
        }
        let Some((vars, pivot)) = self.target_geometry(target) else { return };
        let apply = |f: &mut Frame, g: &dyn Fn(&mut ShapeState)| {
            for &id in &target.shapes {
                let st = f.shapes.entry(id).or_insert_with(|| ShapeState::new(true, pivot));
                st.pivot = pivot;
                g(st);
            }
        };
        let (sw, sh) = (self.sw, self.sh);
        match b {
            Behavior::Set { attr, to } => match (attr.as_str(), to) {
                ("style.visibility", Value::Str(v)) => {
                    let vis = v == "visible";
                    apply(self.f, &|s| s.visible = vis);
                }
                ("style.opacity", v) => {
                    if let Some(o) = self.value(v, &vars) {
                        apply(self.f, &|s| s.opacity = o);
                    }
                }
                _ => {}
            },
            Behavior::Anim { attr, keys, discrete } => {
                if keys.is_empty() {
                    return;
                }
                if attr == "style.visibility" {
                    let i = keys.iter().rposition(|k| k.t <= p).unwrap_or(0);
                    if let Value::Str(v) = &keys[i].value {
                        let vis = v == "visible";
                        apply(self.f, &|s| s.visible = vis);
                    }
                    return;
                }
                let Some(v) = self.keyed(keys, p, *discrete, &vars) else { return };
                match attr.as_str() {
                    "ppt_x" => apply(self.f, &|s| s.dx = (v - vars.x) * sw),
                    "ppt_y" => apply(self.f, &|s| s.dy = (v - vars.y) * sh),
                    "ppt_w" if vars.w > 0.0 => apply(self.f, &|s| s.sx = v / vars.w),
                    "ppt_h" if vars.h > 0.0 => apply(self.f, &|s| s.sy = v / vars.h),
                    "r" | "ppt_r" | "style.rotation" => apply(self.f, &|s| s.rot = v),
                    "style.opacity" => apply(self.f, &|s| s.opacity = v.clamp(0.0, 1.0)),
                    _ => {}
                }
            }
            Behavior::Effect { mode, filter } => {
                if *mode == EffectMode::None {
                    return;
                }
                let r = if *mode == EffectMode::In { p } else { 1.0 - p };
                if p >= 1.0 {
                    // A finished filter leaves the shape as it is (exits hide it with a set).
                    return;
                }
                let fade = filter.starts_with("fade");
                let filter = filter.clone();
                apply(self.f, &|s| {
                    if fade {
                        s.opacity = r;
                    } else {
                        s.reveal = Some((filter.clone(), r));
                    }
                });
            }
            Behavior::Scale { from, to, by } => {
                let (fx, fy) = from.unwrap_or((1.0, 1.0));
                let (x, y) = match (to, by) {
                    (Some((tx, ty)), _) => (fx + (tx - fx) * p, fy + (ty - fy) * p),
                    (None, Some((bx, by))) => (fx * (1.0 + (bx - 1.0) * p), fy * (1.0 + (by - 1.0) * p)),
                    _ => (fx, fy),
                };
                apply(self.f, &|s| {
                    s.sx *= x;
                    s.sy *= y;
                });
            }
            Behavior::Rotate { from, to, by } => {
                let a = from.unwrap_or(0.0);
                let deg = match (to, by) {
                    (Some(t), _) => a + (t - a) * p,
                    (None, Some(b)) => a + b * p,
                    _ => a,
                };
                apply(self.f, &|s| s.rot += deg);
            }
            Behavior::Motion { points } => {
                let Some((x, y)) = point_along(points, p) else { return };
                apply(self.f, &|s| {
                    s.dx += x * sw;
                    s.dy += y * sh;
                });
            }
            Behavior::Color { attr, to: Some(c) } => {
                if p < 0.5 {
                    return;
                }
                let c = *c;
                match attr.as_str() {
                    "stroke.color" => apply(self.f, &|s| s.line = Some(c)),
                    "fillcolor" | "fill.color" | "fill.color2" => apply(self.f, &|s| s.fill = Some(c)),
                    _ => {}
                }
            }
            Behavior::Color { .. } => {}
        }
    }
}

/// The point at fraction `p` of a polyline's length.
fn point_along(points: &[(f64, f64)], p: f64) -> Option<(f64, f64)> {
    let first = *points.first()?;
    let lens: Vec<f64> = points.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).collect();
    let total: f64 = lens.iter().sum();
    if total <= 0.0 {
        return Some(first);
    }
    let mut want = p.clamp(0.0, 1.0) * total;
    for (w, len) in points.windows(2).zip(&lens) {
        if want <= *len && *len > 0.0 {
            let k = want / len;
            return Some((w[0].0 + (w[1].0 - w[0].0) * k, w[0].1 + (w[1].1 - w[0].1) * k));
        }
        want -= len;
    }
    points.last().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas() {
        let v = FormulaVars { x: 0.5, y: 0.25, w: 0.2, h: 0.1, value: 2.0 };
        let e = |s: &str| eval_formula(s, &v).unwrap();
        assert_eq!(e("#ppt_x"), 0.5);
        assert!((e("1+#ppt_h/2") - 1.05).abs() < 1e-9);
        assert!((e("0-#ppt_w/2") + 0.1).abs() < 1e-9);
        assert!((e("#ppt_y-(#ppt_h*2)") - 0.05).abs() < 1e-9);
        assert_eq!(e("$*2^2"), 8.0);
        assert!((e("sin(pi/2)") - 1.0).abs() < 1e-9);
        assert_eq!(e("-3+max(1,4)"), 1.0);
        assert!(eval_formula("#ppt_q", &v).is_none());
        assert!(eval_formula("1+", &v).is_none());
    }

    #[test]
    fn easing_keeps_the_ends() {
        for (a, d) in [(0.0, 0.0), (0.5, 0.0), (0.3, 0.3), (0.0, 1.0)] {
            assert!(ease(0.0, a, d).abs() < 1e-9);
            assert!((ease(1.0, a, d) - 1.0).abs() < 1e-9);
        }
        assert!(ease(0.25, 0.5, 0.5) < 0.25);
    }

    struct TestStage;
    impl Stage for TestStage {
        fn slide_size(&self) -> (f64, f64) {
            (1000.0, 500.0)
        }
        fn frame(&self, id: ShapeId) -> Option<(f64, f64, f64, f64)> {
            Some((100.0 * id.0 as f64, 100.0, 100.0, 50.0))
        }
        fn paragraph_count(&self, _: ShapeId) -> usize {
            3
        }
    }

    fn behavior(id: u32, b: Behavior, delay: f64, dur: f64) -> TimeNode {
        TimeNode { kind: NodeKind::Behavior(Target { element: ShapeId(id), shapes: vec![ShapeId(id)], paragraphs: None }, b), delay_ms: delay, dur_ms: Some(dur), fill_hold: true, ..Default::default() }
    }

    fn appear(id: u32) -> TimeNode {
        behavior(id, Behavior::Set { attr: "style.visibility".into(), to: Value::Str("visible".into()) }, 0.0, 1.0)
    }

    fn effect(children: Vec<TimeNode>, node_type: &str, delay: f64) -> TimeNode {
        TimeNode { children, node_type: node_type.into(), delay_ms: delay, fill_hold: true, ..Default::default() }
    }

    /// mainSeq → click groups → effects, as PowerPoint writes them.
    fn timing(groups: Vec<(bool, Vec<TimeNode>)>) -> Timing {
        let groups = groups.into_iter().map(|(click, effects)| TimeNode { click, children: effects, fill_hold: true, ..Default::default() }).collect();
        let main = TimeNode { kind: NodeKind::Seq, node_type: "mainSeq".into(), children: groups, ..Default::default() };
        Timing { root: TimeNode { node_type: "tmRoot".into(), children: vec![main], ..Default::default() }, ..Default::default() }
    }

    #[test]
    fn entrances_hide_until_their_click() {
        let fly = behavior(
            2,
            Behavior::Anim {
                attr: "ppt_y".into(),
                keys: vec![Key { t: 0.0, value: Value::Str("1+#ppt_h/2".into()), fmla: None }, Key { t: 1.0, value: Value::Str("#ppt_y".into()), fmla: None }],
                discrete: false,
            },
            0.0,
            500.0,
        );
        let t = timing(vec![(true, vec![effect(vec![appear(1)], "clickEffect", 0.0)]), (true, vec![effect(vec![appear(2), fly], "clickEffect", 0.0)])]);
        let p = Player::new(&t);
        assert_eq!(p.clicks(), 2);
        let f0 = p.frame(&TestStage, 0, 0.0);
        assert!(!f0.shapes[&ShapeId(1)].visible && !f0.shapes[&ShapeId(2)].visible);
        let f1 = p.frame(&TestStage, 1, 10.0);
        assert!(f1.shapes[&ShapeId(1)].visible && !f1.shapes[&ShapeId(2)].visible);
        // Halfway through the fly-in: from below the slide (y center 1 + h/2 = 525) toward 125.
        let f2 = p.frame(&TestStage, 2, 250.0);
        let s = &f2.shapes[&ShapeId(2)];
        assert!(s.visible && (s.dy - (325.0 - 125.0)).abs() < 1e-6, "{s:?}");
        assert!(p.running(2, 250.0) && !p.running(2, 600.0));
        let end = p.frame(&TestStage, 2, 600.0);
        assert!(end.shapes[&ShapeId(2)].dy.abs() < 1e-6);
    }

    #[test]
    fn with_and_after_previous_chain_within_a_click() {
        // Click 1: fade shape 1 (500 ms); with it, shape 2 appears; after it (delay 500), shape 3 appears.
        let fade = behavior(1, Behavior::Effect { mode: EffectMode::In, filter: "fade".into() }, 0.0, 500.0);
        let g1 = TimeNode {
            children: vec![effect(vec![appear(1), fade], "clickEffect", 0.0), effect(vec![appear(2)], "withEffect", 0.0)],
            fill_hold: true,
            ..Default::default()
        };
        let g2 = TimeNode { delay_ms: 500.0, children: vec![effect(vec![appear(3)], "afterEffect", 0.0)], fill_hold: true, ..Default::default() };
        let click = TimeNode { click: true, children: vec![g1, g2], fill_hold: true, ..Default::default() };
        let main = TimeNode { kind: NodeKind::Seq, node_type: "mainSeq".into(), children: vec![click], ..Default::default() };
        let t = Timing { root: TimeNode { children: vec![main], ..Default::default() }, ..Default::default() };
        let p = Player::new(&t);
        let f = p.frame(&TestStage, 1, 250.0);
        assert!((f.shapes[&ShapeId(1)].opacity - 0.5).abs() < 1e-9);
        assert!(f.shapes[&ShapeId(2)].visible && !f.shapes[&ShapeId(3)].visible);
        let f = p.frame(&TestStage, 1, 501.0);
        assert!(f.shapes[&ShapeId(3)].visible);
        assert_eq!(f.shapes[&ShapeId(1)].opacity, 1.0);
    }

    #[test]
    fn exits_and_auto_start_and_motion() {
        // Starts with the slide: shape 1 exits (visible until then), moving along a path.
        let path = behavior(1, Behavior::Motion { points: vec![(0.0, 0.0), (0.1, 0.0), (0.1, 0.2)] }, 0.0, 1000.0);
        let hide = behavior(1, Behavior::Set { attr: "style.visibility".into(), to: Value::Str("hidden".into()) }, 1000.0, 1.0);
        let mut first = TimeNode { click: true, auto_begin: true, children: vec![effect(vec![path, hide], "afterEffect", 0.0)], fill_hold: true, ..Default::default() };
        first.children[0].preset_class = "exit".into();
        let main = TimeNode { kind: NodeKind::Seq, node_type: "mainSeq".into(), children: vec![first], ..Default::default() };
        let t = Timing { root: TimeNode { children: vec![main], ..Default::default() }, ..Default::default() };
        let p = Player::new(&t);
        assert_eq!(p.clicks(), 0);
        let f = p.frame(&TestStage, 0, 0.0);
        assert!(f.shapes.get(&ShapeId(1)).is_none_or(|s| s.visible));
        // Half the path length (0.1 + 0.2 = 0.3) is 0.15: past the corner.
        let f = p.frame(&TestStage, 0, 500.0);
        let s = &f.shapes[&ShapeId(1)];
        assert!((s.dx - 100.0).abs() < 1e-6 && (s.dy - 25.0).abs() < 1e-6, "{s:?}");
        assert!(!p.frame(&TestStage, 0, 2000.0).shapes[&ShapeId(1)].visible);
    }

    #[test]
    fn key_formulas_apply_to_the_interpolated_value() {
        // PowerPoint's stretch: width = #ppt_w * sin(2.5π$) with $ going 0 → 1.
        let anim = behavior(
            1,
            Behavior::Anim {
                attr: "ppt_w".into(),
                keys: vec![Key { t: 0.0, value: Value::Num(0.0), fmla: Some("#ppt_w*sin(2.5*pi*$)".into()) }, Key { t: 1.0, value: Value::Num(1.0), fmla: None }],
                discrete: false,
            },
            0.0,
            1000.0,
        );
        let t = timing(vec![(true, vec![effect(vec![appear(1), anim], "clickEffect", 0.0)])]);
        let p = Player::new(&t);
        let sx = |ms: f64| p.frame(&TestStage, 1, ms).shapes[&ShapeId(1)].sx;
        assert!(sx(0.0).abs() < 1e-9);
        assert!((sx(200.0) - 1.0).abs() < 1e-9, "sin(π/2) = 1 at 20%");
        assert!((sx(1000.0) - 1.0).abs() < 1e-9);
        assert!(sx(600.0) < 0.0, "overshoots through a mirror image");
    }

    #[test]
    fn effect_list_and_preview() {
        let fade = behavior(2, Behavior::Effect { mode: EffectMode::In, filter: "fade".into() }, 0.0, 500.0);
        let mut e1 = effect(vec![appear(1)], "clickEffect", 0.0);
        e1.preset_class = "entr".into();
        e1.preset_id = Some(1);
        let e2 = effect(vec![appear(2), fade], "withEffect", 200.0);
        let t = timing(vec![(true, vec![e1]), (true, vec![e2])]);
        let p = Player::new(&t);
        let list = p.effects();
        assert_eq!(list.len(), 2);
        assert_eq!((list[0].click, list[0].preset_id, list[0].shapes.clone()), (1, Some(1), vec![ShapeId(1)]));
        assert_eq!((list[1].trigger.as_str(), list[1].start_ms, list[1].dur_ms), ("withEffect", 200.0, 500.0));
        // Preview: click 1 takes 1 ms, then click 2 runs for 700 ms.
        let f = p.preview_frame(&TestStage, 1.0 + 450.0).unwrap();
        assert!((f.shapes[&ShapeId(2)].opacity - 0.5).abs() < 1e-9);
        assert!(p.preview_frame(&TestStage, 2000.0).is_none());
    }

    #[test]
    fn paragraphs_build_one_at_a_time() {
        let para = |i: usize| TimeNode {
            kind: NodeKind::Behavior(Target { element: ShapeId(1), shapes: vec![ShapeId(1)], paragraphs: Some((i, i)) }, Behavior::Set { attr: "style.visibility".into(), to: Value::Str("visible".into()) }),
            dur_ms: Some(1.0),
            fill_hold: true,
            ..Default::default()
        };
        let t = timing(vec![(true, vec![effect(vec![para(0)], "clickEffect", 0.0)]), (true, vec![effect(vec![para(1)], "clickEffect", 0.0)])]);
        let p = Player::new(&t);
        let f = p.frame(&TestStage, 1, 10.0);
        assert!(f.paragraphs[&(ShapeId(1), 0)].visible && !f.paragraphs[&(ShapeId(1), 1)].visible);
        assert!(!f.shapes.contains_key(&ShapeId(1)));
    }
}
