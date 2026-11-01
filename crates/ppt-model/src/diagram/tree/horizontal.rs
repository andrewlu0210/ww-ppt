//! Horizontal hierarchy (`hierarchy2`): the root at the left, each node's
//! children stacked in a column to its right, joined by straight lines from
//! the parent's right edge to the child's left edge.
//!
//! Columns (where each depth starts and how wide it is) and the gap between
//! stacked subtrees are read from the loaded drawing.

use std::collections::HashMap;

use super::{TreeTpl, find_node};
use crate::diagram::{Drawn, Node, Rect, Shown, Template, build_text, derived_id, scale_fonts};
use crate::{CustomPath, Emu, Geometry, PathCmd, Shape, Xfrm};

/// Where each depth sits in the loaded drawing.
#[derive(Debug)]
pub(in crate::diagram) struct Columns {
    /// Left edge of the nodes at each depth.
    x: Vec<f64>,
    /// Width of the widest node at each depth.
    w: Vec<f64>,
    /// Between neighboring subtrees stacked in a column.
    gap: f64,
    /// Thickness of the connector shapes.
    line: f64,
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// A node placed relative to its own vertical center.
struct Sub {
    /// (node id, parent id, depth, center y), the node itself first.
    items: Vec<(String, Option<String>, usize, f64)>,
    /// Vertical extent around the node's center (top is at most 0, bottom at least 0).
    top: f64,
    bottom: f64,
}

impl Columns {
    pub(in crate::diagram) fn measure(tree: &TreeTpl, t: &Template) -> Option<Columns> {
        let mut x = Vec::new();
        let mut w = Vec::new();
        for ids in &tree.depths {
            let rects: Vec<Rect> = ids.iter().map(|id| tree.rect(t, id)).collect();
            x.push(median(rects.iter().map(|r| r.x).collect()));
            w.push(rects.iter().map(|r| r.w).fold(0.0, f64::max));
        }
        // Neighboring subtrees, from the vertical extent each covers.
        fn walk(tree: &TreeTpl, t: &Template, n: &Node, gaps: &mut Vec<f64>) {
            for pair in n.children.windows(2) {
                let (a, b) = (tree.subtree_rect(t, &pair[0]), tree.subtree_rect(t, &pair[1]));
                gaps.push(b.y - (a.y + a.h));
            }
            n.children.iter().for_each(|c| walk(tree, t, c, gaps));
        }
        let mut gaps = Vec::new();
        walk(tree, t, &t.nodes[0], &mut gaps);
        let first_conn = tree.conns.values().next()?;
        let line = t.shapes[*first_conn].xfrm.h as f64;
        let avg_h = tree.boxes.values().map(|g| t.shapes[g[0]].xfrm.h as f64).sum::<f64>() / tree.boxes.len() as f64;
        let gap = if gaps.is_empty() { avg_h * 0.15 } else { gaps.iter().sum::<f64>() / gaps.len() as f64 }.max(0.0);
        Some(Columns { x, w, gap, line: line.max(1.0) })
    }

    /// Left edge of depth `d`; deeper than loaded continues with the last gap between columns.
    fn left(&self, d: usize) -> f64 {
        let last = self.x.len() - 1;
        if d <= last {
            return self.x[d];
        }
        let step = if last > 0 { (self.x[last] - self.x[last - 1]).max(self.w[last - 1]) } else { self.w[0] * 1.5 };
        self.x[last] + (d - last) as f64 * step
    }

    fn sub(&self, node: &Node, depth: usize, size: &dyn Fn(&str, usize) -> (f64, f64)) -> Sub {
        let h = size(&node.id, depth).1;
        let id = node.id.clone();
        if node.children.is_empty() {
            return Sub { items: vec![(id, None, depth, 0.0)], top: -h / 2.0, bottom: h / 2.0 };
        }
        let kids: Vec<Sub> = node.children.iter().map(|c| self.sub(c, depth + 1, size)).collect();
        let mut items = Vec::new();
        let mut cursor = 0.0;
        let mut centers = Vec::new();
        for k in &kids {
            let c = cursor - k.top;
            centers.push(c);
            for (i, p, d, y) in &k.items {
                items.push((i.clone(), p.clone(), *d, y + c));
            }
            cursor += (k.bottom - k.top) + self.gap;
        }
        let mid = (centers[0] + centers[centers.len() - 1]) / 2.0;
        // The parent is the origin.
        for it in &mut items {
            it.3 -= mid;
        }
        let direct: Vec<String> = node.children.iter().map(|c| c.id.clone()).collect();
        for it in &mut items {
            if it.1.is_none() && direct.contains(&it.0) {
                it.1 = Some(id.clone());
            }
        }
        items.insert(0, (id, None, depth, 0.0));
        let (first, last) = (kids[0].top - mid + centers[0], kids[kids.len() - 1].bottom - mid + centers[centers.len() - 1]);
        Sub { items, top: first.min(-h / 2.0), bottom: last.max(h / 2.0) }
    }

    pub(in crate::diagram) fn layout(&self, tree: &TreeTpl, t: &Template, nodes: &[Node]) -> (Vec<Shape>, Vec<Drawn>) {
        let node_of: HashMap<String, &Node> = {
            fn walk<'a>(n: &'a Node, out: &mut HashMap<String, &'a Node>) {
                out.insert(n.id.clone(), n);
                n.children.iter().for_each(|c| walk(c, out));
            }
            let mut m = HashMap::new();
            nodes.iter().for_each(|n| walk(n, &mut m));
            m
        };
        let size = |id: &str, depth: usize| {
            let (look, _) = tree.look(id, depth);
            let r = tree.rect(t, look);
            (r.w, r.h)
        };
        let root = self.sub(&nodes[0], 0, &size);
        // Width: the columns up to the deepest node, with its own width.
        let right = root.items.iter().map(|(id, _, d, _)| self.left(*d) + size(id, *d).0).fold(0.0, f64::max);
        let (width, height) = ((right - self.left(0)).max(1.0), (root.bottom - root.top).max(1.0));
        // Shrink to fit the frame, keeping the margins the loaded drawing had.
        let b = tree.bounds;
        let (mx, my) = (b.x.min(t.w as f64 - b.x - b.w).max(0.0), b.y.min(t.h as f64 - b.y - b.h).max(0.0));
        let k = ((t.w as f64 - 2.0 * mx) / width).min((t.h as f64 - 2.0 * my) / height).clamp(0.2, 1.0);
        let x0 = b.x.clamp(mx, (t.w as f64 - mx - width * k).max(mx));
        let y0 = (b.y + b.h / 2.0 - height * k / 2.0).clamp(my, (t.h as f64 - my - height * k).max(my));
        let place = |depth: usize, cy: f64| (x0 + (self.left(depth) - self.left(0)) * k, y0 + (cy - root.top) * k);

        let mut rects: HashMap<&str, Rect> = HashMap::new();
        for (id, _, depth, cy) in &root.items {
            let (w, h) = size(id, *depth);
            let (x, y) = place(*depth, *cy);
            rects.insert(id.as_str(), Rect { x, y: y - h * k / 2.0, w: w * k, h: h * k });
        }
        let id_of = |tpl: usize, node: &str, own: bool| {
            let p = &t.pres[t.shape_pres[tpl]];
            if own { p.id.clone() } else { derived_id(node, &p.id) }
        };
        let mut shapes = Vec::new();
        let mut drawn = Vec::new();
        for (id, parent, depth, _) in &root.items {
            let node = node_of[id];
            let (look, own) = tree.look(id, *depth);
            let r = rects[id.as_str()];
            // The connector from the parent's right edge to this node's left edge.
            if let Some(parent) = parent {
                let pr = rects[parent.as_str()];
                let (x1, y1, x2, y2) = (pr.x + pr.w, pr.y + pr.h / 2.0, r.x, r.y + r.h / 2.0);
                let (dx, dy) = (x2 - x1, y2 - y1);
                let len = dx.hypot(dy).max(1.0);
                let ctpl = tree.conns[look];
                let mut c = t.shapes[ctpl].clone();
                let thick = self.line;
                let (mx, my) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
                c.xfrm = Xfrm { x: (mx - len / 2.0).round() as Emu, y: (my - thick / 2.0).round() as Emu, w: len.round() as Emu, h: thick.round() as Emu, rot: dy.atan2(dx).to_degrees().rem_euclid(360.0), ..c.xfrm };
                let (fill, stroke) = match &c.geometry {
                    Geometry::Custom(p) => p.first().map_or((true, true), |p| (p.fill, p.stroke)),
                    _ => (true, true),
                };
                let (pw, ph) = (c.xfrm.w as f64, c.xfrm.h as f64);
                c.geometry = Geometry::Custom(vec![CustomPath { w: pw.max(1.0), h: ph.max(1.0), cmds: vec![PathCmd::MoveTo(0.0, ph / 2.0), PathCmd::LineTo(pw, ph / 2.0)], fill, stroke }]);
                shapes.push(c);
                drawn.push(Drawn { model_id: id_of(ctpl, id, own), template: ctpl, node: None });
            }
            // The node's box.
            let tpl = tree.boxes[look][0];
            let mut s = t.shapes[tpl].clone();
            s.xfrm = Xfrm { x: r.x.round() as Emu, y: r.y.round() as Emu, w: r.w.round() as Emu, h: r.h.round() as Emu, ..s.xfrm };
            if let Some(body) = &t.shapes[tpl].text {
                let look_node = find_node(&t.nodes, look).expect("template node");
                let (old, new): (Shown, Shown) = (vec![(0, &look_node.text)], vec![(0, &node.text)]);
                let mut body = if old == new { body.clone() } else { build_text(body, &old, &new, 1.0) };
                let [l, tp, rr, bt] = body.insets;
                body.insets = [(l as f64 * k) as Emu, (tp as f64 * k) as Emu, (rr as f64 * k) as Emu, (bt as f64 * k) as Emu];
                scale_fonts(&mut body, k);
                s.text = Some(body);
            }
            shapes.push(s);
            drawn.push(Drawn { model_id: id_of(tpl, id, own), template: tpl, node: Some(id.clone()) });
        }
        (shapes, drawn)
    }
}
