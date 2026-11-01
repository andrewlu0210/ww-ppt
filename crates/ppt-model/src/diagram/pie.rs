//! Pie cycle (`cycle8`): one pie slice per node, pushed a little out from the
//! center, with a curved arrow along the outside of each slice. After the
//! nodes change, the slices share the circle again, and the arrows and text
//! follow.
//!
//! The text of a slice sits in the largest upright rectangle that fits inside
//! it, found from the slice's angles; how that differs from the loaded
//! drawing is measured there once and carried over.

use super::{Drawn, Node, Shape, Shown, Template, build_text, derived_id, scale_fonts, template_index};
use crate::{Emu, Geometry, Xfrm};

/// What the loaded drawing says about a pie cycle.
#[derive(Debug)]
pub(super) struct PieTpl {
    /// Template shapes of each node's slice and arrow.
    wedges: Vec<usize>,
    arrows: Vec<usize>,
    /// The circle's center, how far slices are pushed out, and its radius.
    center: (f64, f64),
    explode: f64,
    radius: f64,
    /// Where the first slice starts (degrees, clockwise from the right), and the arrow head's angle.
    start: f64,
    head: f64,
    /// Share of the radius the text rectangle may reach.
    reach: f64,
    /// Text insets of the loaded slices minus what the rectangle search gave (per side).
    bias: [f64; 4],
}

fn adj(g: &Geometry, name: &str) -> Option<f64> {
    let Geometry::Preset { adjust, .. } = g else { return None };
    adjust.iter().find(|(k, _)| k == name).map(|a| a.1)
}

fn set_adj(g: &mut Geometry, name: &str, value: f64) {
    if let Geometry::Preset { adjust, .. } = g {
        match adjust.iter_mut().find(|(k, _)| k == name) {
            Some(a) => a.1 = value,
            None => adjust.push((name.into(), value)),
        }
    }
}

fn is_preset(g: &Geometry, want: &str) -> bool {
    matches!(g, Geometry::Preset { name, .. } if name == want)
}

/// Degrees as the 60000ths of a degree presets take, within one turn.
fn angle_units(deg: f64) -> f64 {
    (deg.rem_euclid(360.0) * 60_000.0).round().min(21_599_999.0)
}

/// The largest upright rectangle (x0, y0, x1, y1) inside the slice from `a` to `b`
/// (degrees clockwise from the right, at most a half turn) of a circle of radius `r`
/// around the origin.
fn text_box(a: f64, b: f64, r: f64) -> (f64, f64, f64, f64) {
    let (ar, br) = (a.to_radians(), b.to_radians());
    let whole = (b - a) >= 360.0 - 1e-6;
    // The vertical extent at `x`: the disc, and the half-planes of the two edges.
    let span = |x: f64| -> Option<(f64, f64)> {
        let q = (r * r - x * x).max(0.0).sqrt();
        let (mut lo, mut hi) = (-q, q);
        if !whole {
            // Edge a: y cos a >= x sin a.   Edge b: y cos b <= x sin b.
            for (cos, rhs, upper) in [(ar.cos(), x * ar.sin(), false), (br.cos(), x * br.sin(), true)] {
                if cos.abs() > 1e-9 {
                    let bound = rhs / cos;
                    // Dividing by a negative flips the side.
                    if (cos > 0.0) != upper {
                        lo = lo.max(bound);
                    } else {
                        hi = hi.min(bound);
                    }
                } else if (upper && rhs < 0.0) || (!upper && rhs > 0.0) {
                    // y * 0 against the other side: the edge is vertical and x is on its wrong side.
                    return None;
                }
            }
        }
        (hi > lo).then_some((lo, hi))
    };
    const N: usize = 120;
    let xs: Vec<f64> = (0..=N).map(|i| -r + 2.0 * r * i as f64 / N as f64).collect();
    let spans: Vec<Option<(f64, f64)>> = xs.iter().map(|&x| span(x)).collect();
    let mut best = (0.0, (0.0, 0.0, 0.0, 0.0));
    for i in 0..=N {
        let Some((lo0, hi0)) = spans[i] else { continue };
        for j in i + 1..=N {
            let Some((lo1, hi1)) = spans[j] else { continue };
            let (lo, hi) = (lo0.max(lo1), hi0.min(hi1));
            let area = (xs[j] - xs[i]) * (hi - lo);
            if hi > lo && area > best.0 {
                best = (area, (xs[i], lo, xs[j], hi));
            }
        }
    }
    best.1
}

impl PieTpl {
    pub(super) fn detect(t: &Template) -> Option<PieTpl> {
        let m = t.nodes.len();
        if !t.layout.ends_with("/cycle8") || m < 3 || t.shapes.len() != 2 * m || t.shapes.len() != t.shape_pres.len() {
            return None;
        }
        let (mut wedges, mut arrows) = (Vec::new(), Vec::new());
        for node in &t.nodes {
            let sib = t.links.get(&node.id)?.sib_trans.as_str();
            let wedge = (0..t.shapes.len()).find(|&i| is_preset(&t.shapes[i].geometry, "pie") && t.pres[t.shape_pres[i]].sources.first() == Some(&node.id))?;
            let arrow = (0..t.shapes.len()).find(|&i| is_preset(&t.shapes[i].geometry, "circularArrow") && t.pres[t.shape_pres[i]].assoc == sib)?;
            if t.shapes[wedge].text.is_none() || !node.children.is_empty() {
                return None;
            }
            wedges.push(wedge);
            arrows.push(arrow);
        }
        let rects: Vec<super::Rect> = wedges.iter().map(|&i| super::Rect::of(&t.shapes[i].xfrm)).collect();
        let center = (rects.iter().map(|r| r.center().0).sum::<f64>() / m as f64, rects.iter().map(|r| r.center().1).sum::<f64>() / m as f64);
        let explode = rects.iter().map(|r| ((r.center().0 - center.0).powi(2) + (r.center().1 - center.1).powi(2)).sqrt()).sum::<f64>() / m as f64;
        let radius = rects[0].w / 2.0;
        let start = adj(&t.shapes[wedges[0]].geometry, "adj1")? / 60_000.0;
        let sweep = (adj(&t.shapes[wedges[0]].geometry, "adj2")? / 60_000.0 - start).rem_euclid(360.0);
        // Evenly shared, clockwise, from one slice to the next.
        if (sweep - 360.0 / m as f64).abs() > 1.0 || rects.iter().any(|r| (r.w - rects[0].w).abs() > 2.0 || (r.h - r.w).abs() > 2.0) {
            return None;
        }
        let head = adj(&t.shapes[arrows[0]].geometry, "adj2")? / 60_000.0;
        let mut tpl = PieTpl { wedges, arrows, center, explode, radius, start, head, reach: 0.97, bias: [0.0; 4] };
        // What the search gives for the loaded slices, against their text insets.
        let mut bias = [0.0; 4];
        for (k, &w) in tpl.wedges.iter().enumerate() {
            let shape = &t.shapes[w];
            let insets = shape.text.as_ref()?.insets;
            let pred = tpl.insets(k, m, shape.xfrm.w as f64);
            for s in 0..4 {
                bias[s] += (insets[s] as f64 - pred[s]) / m as f64;
            }
        }
        tpl.bias = bias;
        Some(tpl)
    }

    /// Text insets of slice `k` of `n` in a slice shape `side` wide (without the carried-over bias).
    fn insets(&self, k: usize, n: usize, side: f64) -> [f64; 4] {
        let sweep = 360.0 / n as f64;
        let a = self.start + k as f64 * sweep;
        let (x0, y0, x1, y1) = text_box(a, a + sweep, self.radius * self.reach);
        let r = side / 2.0;
        // The shape's own frame has the slice's apex at its center.
        [r + x0, r + y0, side - (r + x1), side - (r + y1)]
    }

    pub(super) fn layout(&self, t: &Template, nodes: &[Node]) -> (Vec<Shape>, Vec<Drawn>) {
        let (n, m) = (nodes.len(), t.nodes.len());
        let sweep = 360.0 / n as f64;
        let mut wedges = Vec::new();
        let mut arrows = Vec::new();
        let (mut wedge_drawn, mut arrow_drawn) = (Vec::new(), Vec::new());
        for (j, node) in nodes.iter().enumerate() {
            let tt = template_index(j, n, m);
            let (wi, ai) = (self.wedges[tt], self.arrows[tt]);
            let a = self.start + j as f64 * sweep;
            let mid = (a + sweep / 2.0).to_radians();
            let c = (self.center.0 + self.explode * mid.cos(), self.center.1 + self.explode * mid.sin());
            let id_of = |tpl: usize| derived_id(&node.id, &t.pres[t.shape_pres[tpl]].id);

            let mut w = t.shapes[wi].clone();
            let side = w.xfrm.w as f64;
            w.xfrm = Xfrm { x: (c.0 - side / 2.0).round() as Emu, y: (c.1 - side / 2.0).round() as Emu, ..w.xfrm };
            set_adj(&mut w.geometry, "adj1", angle_units(a));
            set_adj(&mut w.geometry, "adj2", angle_units(a + sweep));
            if let Some(body) = &t.shapes[wi].text {
                let look = &t.nodes[tt];
                let (old, new): (Shown, Shown) = (vec![(0, &look.text)], vec![(0, &node.text)]);
                let mut body = if old == new { body.clone() } else { build_text(body, &old, &new, 1.0) };
                let pred = self.insets(j, n, side);
                body.insets = std::array::from_fn(|s| (pred[s] + self.bias[s]).max(0.0).round() as Emu);
                scale_fonts(&mut body, 1.0);
                w.text = Some(body);
            }
            wedges.push(w);
            wedge_drawn.push(Drawn { model_id: id_of(wi), template: wi, node: Some(node.id.clone()) });

            let mut ar = t.shapes[ai].clone();
            let aside = ar.xfrm.w as f64;
            ar.xfrm = Xfrm { x: (c.0 - aside / 2.0).round() as Emu, y: (c.1 - aside / 2.0).round() as Emu, ..ar.xfrm };
            // The head takes the angle it had, unless the slice is too narrow for it.
            let head = self.head.min(sweep * 0.4);
            set_adj(&mut ar.geometry, "adj2", (head * 60_000.0).round());
            set_adj(&mut ar.geometry, "adj3", angle_units(a + sweep - head));
            set_adj(&mut ar.geometry, "adj4", angle_units(a));
            arrows.push(ar);
            arrow_drawn.push(Drawn { model_id: id_of(ai), template: ai, node: None });
        }
        wedges.extend(arrows);
        wedge_drawn.extend(arrow_drawn);
        (wedges, wedge_drawn)
    }
}
