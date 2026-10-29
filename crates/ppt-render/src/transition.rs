//! Slide transitions: one frame composed from the outgoing and incoming
//! slides, each pre-encoded once as a scene in slide space (EMU).

use ppt_model::transition::{Dir, TransitionKind};
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Circle, Rect, Shape, Vec2};
use vello::peniko::{BlendMode, Color as PColor, Fill, Mix};

/// Bars across the slide for "random bars", cells across for "dissolve".
const BARS: usize = 96;
const DISSOLVE_COLS: usize = 32;

fn ease(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// A fixed shuffled order of `n` items, so a transition looks the same each time.
fn shuffled(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    for i in (1..n).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        order.swap(i, (x % (i as u64 + 1)) as usize);
    }
    order
}

struct Frame<'a> {
    scene: &'a mut Scene,
    view: Affine,
    page: Rect,
}

impl Frame<'_> {
    /// Draws a slide (or black for `None`) moved by `offset` (EMU).
    fn slide(&mut self, s: Option<&Scene>, offset: Vec2) {
        let tf = self.view * Affine::translate(offset);
        match s {
            Some(s) => self.scene.append(s, Some(tf)),
            None => self.scene.fill(Fill::NonZero, tf, PColor::BLACK, None, &self.page),
        }
    }

    /// Draws `s` scaled by `k` around the slide center, with `alpha`.
    fn scaled(&mut self, s: Option<&Scene>, k: f64, alpha: f64) {
        let c = self.page.center().to_vec2();
        let tf = Affine::translate(c) * Affine::scale(k) * Affine::translate(-c);
        self.scene.push_layer(Fill::NonZero, BlendMode::from(Mix::Normal), alpha as f32, self.view, &self.page);
        let view = self.view;
        self.view = view * tf;
        self.slide(s, Vec2::ZERO);
        self.view = view;
        self.scene.pop_layer();
    }

    fn faded(&mut self, s: Option<&Scene>, alpha: f64) {
        self.scene.push_layer(Fill::NonZero, BlendMode::from(Mix::Normal), alpha as f32, self.view, &self.page);
        self.slide(s, Vec2::ZERO);
        self.scene.pop_layer();
    }

    fn clipped(&mut self, s: Option<&Scene>, clip: &impl Shape) {
        self.scene.push_clip_layer(Fill::NonZero, self.view, clip);
        self.slide(s, Vec2::ZERO);
        self.scene.pop_layer();
    }
}

fn rects(rs: impl IntoIterator<Item = Rect>) -> BezPath {
    let mut p = BezPath::new();
    for r in rs {
        p.extend(r.path_elements(0.1));
    }
    p
}

/// Draws the transition from `from` (black for the first slide) to `to` at
/// progress `t` in 0..=1. `view` maps slide space to device pixels; the
/// scenes are in slide space, `w` x `h` EMU.
#[allow(clippy::too_many_arguments)]
pub fn draw_transition(scene: &mut Scene, from: Option<&Scene>, to: &Scene, kind: &TransitionKind, t: f64, view: Affine, w: f64, h: f64) {
    let t = t.clamp(0.0, 1.0);
    let e = ease(t);
    let page = Rect::new(0.0, 0.0, w, h);
    scene.push_clip_layer(Fill::NonZero, view, &page);
    let mut f = Frame { scene, view, page };
    let to = Some(to);
    let shift = |d: Dir, k: f64| {
        let (x, y) = d.vector();
        Vec2::new(x * w * k, y * h * k)
    };
    match kind {
        TransitionKind::Cut { through_black } => f.slide(if *through_black && t < 1.0 { None } else { to }, Vec2::ZERO),
        TransitionKind::Fade { through_black: true } => {
            f.slide(None, Vec2::ZERO);
            if t < 0.5 {
                f.faded(from, 1.0 - 2.0 * t);
            } else {
                f.faded(to, 2.0 * t - 1.0);
            }
        }
        TransitionKind::Fade { .. } | TransitionKind::Other(_) => {
            f.slide(from, Vec2::ZERO);
            f.faded(to, t);
        }
        TransitionKind::Push(d) => {
            f.slide(from, shift(*d, e));
            f.slide(to, shift(*d, e - 1.0));
        }
        TransitionKind::Cover(d) => {
            f.slide(from, Vec2::ZERO);
            f.slide(to, shift(*d, e - 1.0));
        }
        TransitionKind::Pull(d) => {
            f.slide(to, Vec2::ZERO);
            f.slide(from, shift(*d, e));
        }
        TransitionKind::Wipe(d) => {
            f.slide(from, Vec2::ZERO);
            let clip = match d {
                Dir::Left => Rect::new(w * (1.0 - e), 0.0, w, h),
                Dir::Right => Rect::new(0.0, 0.0, w * e, h),
                Dir::Up => Rect::new(0.0, h * (1.0 - e), w, h),
                _ => Rect::new(0.0, 0.0, w, h * e),
            };
            f.clipped(to, &clip);
        }
        TransitionKind::Split { vertical, outward } => {
            f.slide(from, Vec2::ZERO);
            let clip = match (vertical, outward) {
                (true, true) => rects([Rect::new(w / 2.0 * (1.0 - e), 0.0, w / 2.0 * (1.0 + e), h)]),
                (false, true) => rects([Rect::new(0.0, h / 2.0 * (1.0 - e), w, h / 2.0 * (1.0 + e))]),
                (true, false) => rects([Rect::new(0.0, 0.0, w / 2.0 * e, h), Rect::new(w - w / 2.0 * e, 0.0, w, h)]),
                (false, false) => rects([Rect::new(0.0, 0.0, w, h / 2.0 * e), Rect::new(0.0, h - h / 2.0 * e, w, h)]),
            };
            f.clipped(to, &clip);
        }
        TransitionKind::RandomBar { vertical } => {
            f.slide(from, Vec2::ZERO);
            let shown = (t * BARS as f64).round() as usize;
            let bar = if *vertical { w } else { h } / BARS as f64;
            let clip = rects(shuffled(BARS, 7).into_iter().take(shown).map(|i| {
                let a = i as f64 * bar;
                // Slight overlap so neighbouring bars leave no seams.
                if *vertical { Rect::new(a, 0.0, a + bar * 1.02, h) } else { Rect::new(0.0, a, w, a + bar * 1.02) }
            }));
            f.clipped(to, &clip);
        }
        TransitionKind::Dissolve => {
            f.slide(from, Vec2::ZERO);
            let cell = w / DISSOLVE_COLS as f64;
            let rows = (h / cell).ceil() as usize;
            let n = DISSOLVE_COLS * rows;
            let shown = (t * n as f64).round() as usize;
            let clip = rects(shuffled(n, 11).into_iter().take(shown).map(|i| {
                let (c, r) = ((i % DISSOLVE_COLS) as f64, (i / DISSOLVE_COLS) as f64);
                Rect::new(c * cell, r * cell, (c + 1.02) * cell, (r + 1.02) * cell)
            }));
            f.clipped(to, &clip);
        }
        TransitionKind::Zoom { inward: true } => {
            f.slide(from, Vec2::ZERO);
            f.scaled(to, 0.3 + 0.7 * e, t);
        }
        TransitionKind::Zoom { inward: false } => {
            f.slide(to, Vec2::ZERO);
            f.scaled(from, 1.0 - 0.7 * e, 1.0 - t);
        }
        TransitionKind::Circle => {
            f.slide(from, Vec2::ZERO);
            let r = e * (w * w + h * h).sqrt() / 2.0;
            f.clipped(to, &Circle::new(page.center(), r.max(1.0)));
        }
    }
    f.scene.pop_layer();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffle_is_a_fixed_permutation() {
        let a = shuffled(50, 3);
        assert_eq!(a, shuffled(50, 3));
        let mut sorted = a.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(a, (0..50).collect::<Vec<_>>());
    }
}
