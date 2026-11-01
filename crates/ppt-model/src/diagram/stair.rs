//! Step-down process (`StepDownProcess`): each node is a small group (a bent
//! arrow, a title and a box for its children) one step right of and below the
//! one before; the last is the title alone.
//!
//! The step, and where each shape of a group sits against its title, are
//! those of the loaded drawing. The whole staircase is scaled to fill the
//! frame's width or height, as PowerPoint does, and centered in it.

use super::{Drawn, Node, Rect, Shape, Shown, Template, build_text, derived_id, scale_fonts, shown, template_index};
use crate::{Emu, Xfrm};

/// What the drawing says about a staircase.
#[derive(Debug)]
pub(super) struct StairTpl {
    /// One step: how far each title is right of and below the one before.
    step: (f64, f64),
    /// Each loaded node's shapes with their centers relative to the title's top left corner.
    groups: Vec<Vec<(usize, (f64, f64))>>,
    /// Most the staircase grows beyond its loaded size.
    max_scale: f64,
}

/// The extent of a shape as it looks: a quarter turn swaps its sides.
fn visual(x: &Xfrm) -> (f64, f64) {
    let quarter = ((x.rot / 90.0).round() as i64).rem_euclid(2) == 1 && (x.rot % 90.0).abs() < 1.0;
    if quarter { (x.h as f64, x.w as f64) } else { (x.w as f64, x.h as f64) }
}

impl StairTpl {
    pub(super) fn detect(t: &Template, units: &[Vec<usize>]) -> Option<StairTpl> {
        let m = units.len();
        if m < 3 {
            return None;
        }
        // The title of each node: the shape showing its own text.
        let title = |u: &Vec<usize>| u.iter().copied().find(|&i| t.shows(t.shape_pres[i]).0);
        let titles: Vec<usize> = units.iter().map(title).collect::<Option<_>>()?;
        let at = |i: usize| (t.shapes[i].xfrm.x as f64, t.shapes[i].xfrm.y as f64);
        let steps: Vec<(f64, f64)> = titles.windows(2).map(|w| (at(w[1]).0 - at(w[0]).0, at(w[1]).1 - at(w[0]).1)).collect();
        let mean = (steps.iter().map(|s| s.0).sum::<f64>() / steps.len() as f64, steps.iter().map(|s| s.1).sum::<f64>() / steps.len() as f64);
        // Down and to the right, by the same step each time.
        if mean.0 <= 0.0 || mean.1 <= 0.0 || steps.iter().any(|s| (s.0 - mean.0).abs() > mean.0 * 0.03 || (s.1 - mean.1).abs() > mean.1 * 0.03) {
            return None;
        }
        let groups: Vec<Vec<(usize, (f64, f64))>> = units
            .iter()
            .zip(&titles)
            .map(|(u, &ti)| {
                let o = at(ti);
                u.iter().map(|&i| (i, (Rect::of(&t.shapes[i].xfrm).center().0 - o.0, Rect::of(&t.shapes[i].xfrm).center().1 - o.1))).collect()
            })
            .collect();
        // Groups before the last look alike.
        let same = |a: &Vec<(usize, (f64, f64))>, b: &Vec<(usize, (f64, f64))>| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x.1.0 - y.1.0).abs() < 2_000.0 && (x.1.1 - y.1.1).abs() < 2_000.0);
        if groups[..m - 1].windows(2).any(|w| !same(&w[0], &w[1])) {
            return None;
        }
        Some(StairTpl { step: mean, groups, max_scale: 2.0 })
    }

    pub(super) fn layout(&self, t: &Template, nodes: &[Node]) -> (Vec<Shape>, Vec<Drawn>) {
        let (n, m) = (nodes.len(), t.nodes.len());
        // The staircase at its loaded size, titles' top left corners at j steps.
        let place = |j: usize, s: f64, origin: (f64, f64)| (origin.0 + j as f64 * self.step.0 * s, origin.1 + j as f64 * self.step.1 * s);
        let extent = |s: f64| -> Rect {
            let mut r = Rect::default();
            for j in 0..n {
                let g = &self.groups[template_index(j, n, m)];
                let p = place(j, s, (0.0, 0.0));
                for &(i, off) in g {
                    let (w, h) = visual(&t.shapes[i].xfrm);
                    r = r.union(Rect { x: p.0 + off.0 * s - w * s / 2.0, y: p.1 + off.1 * s - h * s / 2.0, w: w * s, h: h * s });
                }
            }
            r
        };
        let full = extent(1.0);
        let s = ((t.w as f64 / full.w.max(1.0)).min(t.h as f64 / full.h.max(1.0))).clamp(0.2, self.max_scale);
        let fit = extent(s);
        // Centered in the frame.
        let origin = ((t.w as f64 - fit.w) / 2.0 - fit.x, (t.h as f64 - fit.h) / 2.0 - fit.y);
        let mut shapes = Vec::new();
        let mut drawn = Vec::new();
        for (j, node) in nodes.iter().enumerate() {
            let tt = template_index(j, n, m);
            let p = place(j, s, origin);
            let (unit_pres, trans_pres) = t.plan(&node.id, j, n);
            let ids: std::collections::HashMap<usize, &String> = unit_pres.iter().chain(&trans_pres).map(|(id, pi)| (*pi, id)).collect();
            for &(i, off) in &self.groups[tt] {
                let mut sh = t.shapes[i].clone();
                let (w, h) = (sh.xfrm.w as f64 * s, sh.xfrm.h as f64 * s);
                let c = (p.0 + off.0 * s, p.1 + off.1 * s);
                sh.xfrm = Xfrm { x: (c.0 - w / 2.0).round() as Emu, y: (c.1 - h / 2.0).round() as Emu, w: w.round() as Emu, h: h.round() as Emu, ..sh.xfrm };
                if let Some(body) = &t.shapes[i].text {
                    let (own, desc) = t.shows(t.shape_pres[i]);
                    let mut body = body.clone();
                    if own || desc {
                        let (old, new): (Shown, Shown) = (shown(&t.nodes[tt], own, desc), shown(node, own, desc));
                        if old != new {
                            body = build_text(&body, &old, &new, 1.0);
                        }
                    }
                    let [l, tp, r, b] = body.insets;
                    body.insets = [(l as f64 * s) as Emu, (tp as f64 * s) as Emu, (r as f64 * s) as Emu, (b as f64 * s) as Emu];
                    scale_fonts(&mut body, s);
                    sh.text = Some(body);
                }
                let pi = t.shape_pres[i];
                shapes.push(sh);
                drawn.push(Drawn { model_id: ids.get(&pi).map_or_else(|| derived_id(&node.id, &t.pres[pi].id), |id| (*id).clone()), template: i, node: Some(node.id.clone()) });
            }
        }
        (shapes, drawn)
    }
}
