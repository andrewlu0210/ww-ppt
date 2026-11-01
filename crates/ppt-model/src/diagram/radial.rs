//! Radial list (`radial2`): a hub in the middle, and each top-level node (a
//! round title with a box for its children's text) on a circle around it to
//! the right, joined to the hub by a thin line.
//!
//! The nodes sit on an arc whose start and span depend on how many there are
//! (the layout's `cycle` rule); sizes and the radius are those of the loaded
//! drawing, scaled down together when the arc needs more room.

use super::{Drawn, Node, Rect, Shape, Shown, Template, build_text, derived_id, scale_fonts, shown, template_index};
use crate::{CustomPath, Emu, Geometry, PathCmd, Xfrm};

/// Where a node's shapes sit relative to the middle of the pair, in the loaded drawing.
#[derive(Debug)]
struct Member {
    /// Template shape index.
    shape: usize,
    /// Center of the shape minus the pair's center.
    offset: (f64, f64),
    /// What it shows of its node: (own text, children's text).
    shows: (bool, bool),
}

#[derive(Debug)]
pub(super) struct RadialTpl {
    hub: usize,
    /// Each loaded node's connector to the hub, and its shapes.
    conns: Vec<usize>,
    members: Vec<Vec<Member>>,
    /// The hub's center and the circle's radius.
    center: (f64, f64),
    radius: f64,
    /// Half the width of the hub's connection circle, the line's thickness.
    site: f64,
    line: f64,
}

/// Where the pairs sit, as degrees clockwise from straight up, for `n` nodes.
fn angles(n: usize) -> Vec<f64> {
    let (start, span) = match n {
        0 | 1 => return vec![90.0],
        2 => (70.0, 40.0),
        3 => (60.0, 60.0),
        _ => (45.0, 90.0),
    };
    (0..n).map(|i| start + span * i as f64 / (n - 1) as f64).collect()
}

impl RadialTpl {
    pub(super) fn detect(t: &Template) -> Option<RadialTpl> {
        let m = t.nodes.len();
        if !t.layout.ends_with("/radial2") || m == 0 || t.shapes.len() != 1 + 3 * m || t.shapes.len() != t.shape_pres.len() {
            return None;
        }
        let mut hub = None;
        let mut conns = Vec::new();
        let mut members: Vec<Vec<Member>> = Vec::new();
        let mut centers = Vec::new();
        for node in &t.nodes {
            let par = t.links.get(&node.id)?.par_trans.as_str();
            let conn = (0..t.shapes.len()).find(|&i| t.pres[t.shape_pres[i]].assoc == par && matches!(t.shapes[i].geometry, Geometry::Custom(_)))?;
            let mine: Vec<usize> = (0..t.shapes.len()).filter(|&i| t.pres[t.shape_pres[i]].assoc == node.id).collect();
            let [a, b] = mine[..] else { return None };
            // The round title shows the node's text, the box its children's.
            let order = if t.shows(t.shape_pres[a]).0 { [a, b] } else { [b, a] };
            if !t.shows(t.shape_pres[order[0]]).0 || !t.shows(t.shape_pres[order[1]]).1 {
                return None;
            }
            let pair = Rect::of(&t.shapes[a].xfrm).union(Rect::of(&t.shapes[b].xfrm));
            let c = pair.center();
            centers.push(c);
            members.push(order.iter().map(|&i| Member { shape: i, offset: (Rect::of(&t.shapes[i].xfrm).center().0 - c.0, Rect::of(&t.shapes[i].xfrm).center().1 - c.1), shows: t.shows(t.shape_pres[i]) }).collect());
            conns.push(conn);
        }
        // Everything else is the hub.
        for i in 0..t.shapes.len() {
            if !conns.contains(&i) && !members.iter().flatten().any(|mb| mb.shape == i) {
                if hub.is_some() || t.shapes[i].text.as_ref().is_some_and(|b| !b.is_empty()) {
                    return None;
                }
                hub = Some(i);
            }
        }
        let hub = hub?;
        let center = Rect::of(&t.shapes[hub].xfrm).center();
        let polar = |c: (f64, f64)| ((c.0 - center.0).hypot(c.1 - center.1), (c.0 - center.0).atan2(center.1 - c.1).to_degrees());
        let radius = centers.iter().map(|&c| polar(c).0).sum::<f64>() / m as f64;
        let want = angles(m);
        if radius <= 0.0 || centers.iter().zip(&want).any(|(&c, w)| (polar(c).0 - radius).abs() > radius * 0.02 || (polar(c).1 - w).abs() > 3.0) {
            return None;
        }
        // The line starts where the hub's connection circle's edge is along the ray's x, and is as thick as it is.
        let mut site = 0.0;
        for (k, &c) in conns.iter().enumerate() {
            let x = &t.shapes[c].xfrm;
            let (cx, cy) = (x.x as f64 + x.w as f64 / 2.0, x.y as f64 + x.h as f64 / 2.0);
            let (sin, cos) = x.rot.to_radians().sin_cos();
            let start = (cx - x.w as f64 / 2.0 * cos - center.0, cy - x.w as f64 / 2.0 * sin - center.1);
            // Along the ray from the hub to the round title.
            let title = Rect::of(&t.shapes[members[k][0].shape].xfrm).center();
            let phi = (title.1 - center.1).atan2(title.0 - center.0);
            site += start.0.hypot(start.1) * phi.cos().abs() / m as f64;
        }
        let line = t.shapes[conns[0]].xfrm.h as f64;
        Some(RadialTpl { hub, conns, members, center, radius, site, line })
    }

    pub(super) fn layout(&self, t: &Template, nodes: &[Node]) -> (Vec<Shape>, Vec<Drawn>) {
        let (n, m) = (nodes.len(), t.nodes.len());
        let ang = angles(n);
        let spot = |j: usize, s: f64| {
            let a = ang[j].to_radians();
            (self.center.0 + s * self.radius * a.sin(), self.center.1 - s * self.radius * a.cos())
        };
        let (w, h) = (t.w as f64, t.h as f64);
        let hub = Rect::of(&t.shapes[self.hub].xfrm);
        // Scaled down about the hub until everything is within the frame.
        let mut s: f64 = 1.0;
        for j in 0..n {
            let (cx, cy) = spot(j, 1.0);
            let (dx, dy) = (cx - self.center.0, cy - self.center.1);
            let tt = template_index(j, n, m);
            // The pair's box around its center, relative to the hub.
            let r = self.members[tt].iter().fold(Rect::default(), |r, mb| {
                let x = &t.shapes[mb.shape].xfrm;
                r.union(Rect { x: dx + mb.offset.0 - x.w as f64 / 2.0, y: dy + mb.offset.1 - x.h as f64 / 2.0, w: x.w as f64, h: x.h as f64 })
            });
            let room = |lo: f64, hi: f64, at: f64, size: f64| -> f64 {
                let mut k: f64 = 1.0;
                if lo < 0.0 {
                    k = k.min((at / -lo).max(0.0));
                }
                if hi > 0.0 {
                    k = k.min(((size - at) / hi).max(0.0));
                }
                k
            };
            s = s.min(room(r.x, r.x + r.w, self.center.0, w)).min(room(r.y, r.y + r.h, self.center.1, h));
        }
        s = s.clamp(0.2, 1.0);

        let mut conns = Vec::new();
        let mut conn_drawn = Vec::new();
        let mut shapes = Vec::new();
        let mut drawn = Vec::new();
        // The hub, scaled about its center.
        let mut hub_shape = t.shapes[self.hub].clone();
        hub_shape.xfrm = Xfrm { x: (self.center.0 - hub.w * s / 2.0).round() as Emu, y: (self.center.1 - hub.h * s / 2.0).round() as Emu, w: (hub.w * s).round() as Emu, h: (hub.h * s).round() as Emu, ..hub_shape.xfrm };
        let hub_pres = &t.pres[t.shape_pres[self.hub]].id;
        let hub_drawn = Drawn { model_id: hub_pres.clone(), template: self.hub, node: None };
        for (j, node) in nodes.iter().enumerate() {
            let tt = template_index(j, n, m);
            let c = spot(j, s);
            let id_of = |tpl: usize| derived_id(&node.id, &t.pres[t.shape_pres[tpl]].id);
            let mut parent_center = c;
            let mut parent_r = 0.0;
            for mb in &self.members[tt] {
                let mut sh = t.shapes[mb.shape].clone();
                let (sw, shh) = (sh.xfrm.w as f64 * s, sh.xfrm.h as f64 * s);
                let ctr = (c.0 + mb.offset.0 * s, c.1 + mb.offset.1 * s);
                sh.xfrm = Xfrm { x: (ctr.0 - sw / 2.0).round() as Emu, y: (ctr.1 - shh / 2.0).round() as Emu, w: sw.round() as Emu, h: shh.round() as Emu, ..sh.xfrm };
                if let Some(body) = &t.shapes[mb.shape].text {
                    let (own, desc) = mb.shows;
                    let (old, new): (Shown, Shown) = (shown(&t.nodes[tt], own, desc), shown(node, own, desc));
                    let mut body = if old == new { body.clone() } else { build_text(body, &old, &new, 1.0) };
                    let [l, tp, r, b] = body.insets;
                    body.insets = [(l as f64 * s) as Emu, (tp as f64 * s) as Emu, (r as f64 * s) as Emu, (b as f64 * s) as Emu];
                    scale_fonts(&mut body, s);
                    sh.text = Some(body);
                }
                if mb.shows.0 {
                    parent_center = ctr;
                    parent_r = sw.min(shh) / 2.0;
                }
                shapes.push(sh);
                drawn.push(Drawn { model_id: id_of(mb.shape), template: mb.shape, node: Some(node.id.clone()) });
            }
            // The line from the hub's connection circle to the title's edge.
            let (dx, dy) = (parent_center.0 - self.center.0, parent_center.1 - self.center.1);
            let dist = dx.hypot(dy).max(1.0);
            let phi = dy.atan2(dx);
            let start = (self.site * s / phi.cos().abs().max(0.2)).min(dist);
            let end = (dist - parent_r).max(start);
            let len = (end - start).max(1.0);
            let mid = (start + end) / 2.0;
            let (mx, my) = (self.center.0 + mid * phi.cos(), self.center.1 + mid * phi.sin());
            let ctpl = self.conns[tt];
            let mut cs = t.shapes[ctpl].clone();
            let thick = (self.line * s).max(1.0);
            cs.xfrm = Xfrm { x: (mx - len / 2.0).round() as Emu, y: (my - thick / 2.0).round() as Emu, w: len.round() as Emu, h: thick.round() as Emu, rot: phi.to_degrees().rem_euclid(360.0), ..cs.xfrm };
            let (fill, stroke) = match &cs.geometry {
                Geometry::Custom(p) => p.first().map_or((true, true), |p| (p.fill, p.stroke)),
                _ => (true, true),
            };
            let (pw, ph) = (cs.xfrm.w as f64, cs.xfrm.h as f64);
            cs.geometry = Geometry::Custom(vec![CustomPath { w: pw.max(1.0), h: ph.max(1.0), cmds: vec![PathCmd::MoveTo(0.0, ph / 2.0), PathCmd::LineTo(pw, ph / 2.0)], fill, stroke }]);
            conns.push(cs);
            conn_drawn.push(Drawn { model_id: id_of(ctpl), template: ctpl, node: None });
        }
        // Lines, then the hub, then the nodes (as drawn when loaded).
        conns.push(hub_shape);
        conn_drawn.push(hub_drawn);
        conns.extend(shapes);
        conn_drawn.extend(drawn);
        (conns, conn_drawn)
    }
}
