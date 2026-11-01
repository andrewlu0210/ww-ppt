//! Org chart layouts (`orgChart1`): every node is one box with an elbow
//! connector to its parent. After the tree changes, the boxes are placed again
//! as a tidy tree and the connectors are redrawn between them.
//!
//! How a level is arranged is read from the loaded drawing: children in a
//! row under their parent, or hanging, stacked below it and indented. Gaps,
//! indents and the elbow's proportions are measured there too.

use std::collections::HashMap;

use super::{Drawn, Node, Rect, Shown, Template, build_text, derived_id, scale_fonts};
use crate::{CustomPath, Emu, Geometry, PathCmd, Shape};

/// How the children of nodes at one depth are arranged, measured on the loaded drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Level {
    /// Stacked below the parent and indented (hanging), instead of in a row.
    stack: bool,
    /// Hanging: left edge of the children relative to the parent's.
    indent: f64,
    /// Hanging: where the connector leaves the parent, as a fraction of its width.
    hang: f64,
    /// Parent's bottom to the first child's top.
    first_gap: f64,
    /// Hanging: between one child's subtree and the next.
    step_gap: f64,
    /// Row: height of the elbow's horizontal bar as a fraction of the connector.
    mid: f64,
}

impl Default for Level {
    fn default() -> Level {
        Level { stack: false, indent: 0.0, hang: 0.1, first_gap: 0.0, step_gap: 0.0, mid: 0.5 }
    }
}

/// What the loaded drawing says about an org chart.
#[derive(Debug)]
pub(super) struct TreeTpl {
    /// Template shape of each node's box, and of the connector to its parent.
    boxes: HashMap<String, usize>,
    conns: HashMap<String, usize>,
    /// Node ids per depth, in order.
    depths: Vec<Vec<String>>,
    /// Arrangement of the children of nodes at each depth.
    levels: Vec<Level>,
    /// Between neighboring subtrees in a row.
    hgap: f64,
    /// Everything the loaded drawing covers.
    bounds: Rect,
}

fn depth_lists(nodes: &[Node]) -> Vec<Vec<String>> {
    fn walk(n: &Node, depth: usize, out: &mut Vec<Vec<String>>) {
        if out.len() <= depth {
            out.push(Vec::new());
        }
        out[depth].push(n.id.clone());
        for c in &n.children {
            walk(c, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    for n in nodes {
        walk(n, 0, &mut out);
    }
    out
}

impl TreeTpl {
    /// Recognizes an org chart whose drawing is one box per node and one
    /// connector per parent link.
    pub(super) fn detect(t: &Template) -> Option<TreeTpl> {
        if !t.layout.ends_with("/orgChart1") || t.nodes.len() != 1 || t.shapes.len() != t.shape_pres.len() {
            return None;
        }
        let depths = depth_lists(&t.nodes);
        let count: usize = depths.iter().map(Vec::len).sum();
        if count < 2 {
            return None;
        }
        let trans: HashMap<&str, &str> = depths.iter().flatten().filter_map(|id| Some((t.links.get(id)?.par_trans.as_str(), id.as_str()))).collect();
        let nodes: HashMap<&str, ()> = depths.iter().flatten().map(|id| (id.as_str(), ())).collect();
        let (mut boxes, mut conns) = (HashMap::new(), HashMap::new());
        for (i, &p) in t.shape_pres.iter().enumerate() {
            let assoc = t.pres[p].assoc.as_str();
            let shape = &t.shapes[i];
            if nodes.contains_key(assoc) {
                if shape.text.is_none() || boxes.insert(assoc.to_string(), i).is_some() {
                    return None;
                }
            } else {
                let child = trans.get(assoc)?;
                if shape.text.is_some() || conns.insert(child.to_string(), i).is_some() {
                    return None;
                }
            }
        }
        if boxes.len() != count || conns.len() != count - 1 || conns.contains_key(&depths[0][0]) {
            return None;
        }
        let bounds = t.shapes.iter().fold(Rect::default(), |r, s| r.union(Rect::of(&s.xfrm)));
        let mut tree = TreeTpl { boxes, conns, depths, levels: Vec::new(), hgap: 0.0, bounds };
        tree.measure(t);
        Some(tree)
    }

    fn rect(&self, t: &Template, id: &str) -> Rect {
        Rect::of(&t.shapes[self.boxes[id]].xfrm)
    }

    /// Boxes of `node` and everything below it in the loaded drawing.
    fn subtree_rect(&self, t: &Template, node: &Node) -> Rect {
        node.children.iter().fold(self.rect(t, &node.id), |r, c| r.union(self.subtree_rect(t, c)))
    }

    fn measure(&mut self, t: &Template) {
        fn collect<'a>(n: &'a Node, depth: usize, out: &mut Vec<Vec<&'a Node>>) {
            if out.len() <= depth {
                out.push(Vec::new());
            }
            out[depth].push(n);
            for c in &n.children {
                collect(c, depth + 1, out);
            }
        }
        let mut by_depth: Vec<Vec<&Node>> = Vec::new();
        collect(&t.nodes[0], 0, &mut by_depth);
        let mut hgaps = Vec::new();
        let mut levels: Vec<Level> = Vec::new();
        for parents in &by_depth {
            let mut lv = levels.last().copied().unwrap_or_default();
            if let Some(p) = parents.iter().find(|p| !p.children.is_empty()) {
                let pr = self.rect(t, &p.id);
                let kids: Vec<Rect> = p.children.iter().map(|c| self.rect(t, &c.id)).collect();
                lv.stack = kids.iter().all(|k| k.center().0 > pr.center().0 + pr.w * 0.05);
                lv.first_gap = kids[0].y - (pr.y + pr.h);
                let conn = &t.shapes[self.conns[&p.children[0].id]];
                if lv.stack {
                    lv.indent = kids[0].x - pr.x;
                    lv.hang = if pr.w > 0.0 { (conn.xfrm.x as f64 - pr.x) / pr.w } else { 0.1 };
                    let first = self.subtree_rect(t, &p.children[0]);
                    lv.step_gap = if kids.len() >= 2 { (self.subtree_rect(t, &p.children[1]).y - (first.y + first.h)).max(0.0) } else { kids[0].h * 0.3 };
                } else if let Geometry::Custom(paths) = &conn.geometry
                    && let Some(PathCmd::LineTo(_, y)) = paths.first().and_then(|p| p.cmds.get(1))
                    && conn.xfrm.h > 0
                {
                    // The path may be in its own units (`h`) or in EMU.
                    let h = paths[0].h.max(if paths[0].h == 0.0 { conn.xfrm.h as f64 } else { 0.0 });
                    lv.mid = (y / h).clamp(0.0, 1.0);
                }
            }
            levels.push(lv);
            for p in parents {
                for pair in p.children.windows(2) {
                    if !lv.stack {
                        let (a, b) = (self.subtree_rect(t, &pair[0]), self.subtree_rect(t, &pair[1]));
                        hgaps.push(b.x - (a.x + a.w));
                    }
                }
            }
        }
        let avg_w = self.boxes.values().map(|&i| t.shapes[i].xfrm.w as f64).sum::<f64>() / self.boxes.len() as f64;
        self.hgap = if hgaps.is_empty() { avg_w * 0.1 } else { hgaps.iter().sum::<f64>() / hgaps.len() as f64 }.max(0.0);
        self.levels = levels;
    }

    fn level(&self, depth: usize) -> Level {
        self.levels.get(depth).or(self.levels.last()).copied().unwrap_or_default()
    }

    /// The loaded node whose look the node `id` at `depth` takes, and whether
    /// that is its own (same depth as when loaded) or another's at that depth.
    pub(super) fn look(&self, id: &str, depth: usize) -> (&str, bool) {
        if self.depths.get(depth).is_some_and(|ids| ids.iter().any(|i| i == id)) {
            return (self.depths[depth].iter().find(|i| *i == id).map(String::as_str).unwrap_or_default(), true);
        }
        let ids = self.depths.get(depth).unwrap_or_else(|| self.depths.last().expect("a root"));
        (ids[0].as_str(), false)
    }
}

/// A node placed relative to its own box's top left corner.
struct Sub {
    /// (node id, parent id, depth, x, y), the node itself first.
    items: Vec<(String, Option<String>, usize, f64, f64)>,
    /// Horizontal extent (left is at most 0) and bottom of the subtree.
    l: f64,
    r: f64,
    b: f64,
    /// The node's own box.
    w: f64,
}

impl TreeTpl {
    fn sub(&self, node: &Node, depth: usize, size: &dyn Fn(&str, usize) -> (f64, f64)) -> Sub {
        let (w, h) = size(&node.id, depth);
        let id = node.id.clone();
        if node.children.is_empty() {
            return Sub { items: vec![(id, None, depth, 0.0, 0.0)], l: 0.0, r: w, b: h, w };
        }
        let lv = self.level(depth);
        let kids: Vec<Sub> = node.children.iter().map(|c| self.sub(c, depth + 1, size)).collect();
        let top = h + lv.first_gap;
        let mut items = Vec::new();
        let (mut l, mut r, mut b);
        let shift;
        if lv.stack {
            let mut y = top;
            (l, r, b) = (0.0f64, w, h);
            for k in &kids {
                for (i, p, d, x, yy) in &k.items {
                    items.push((i.clone(), p.clone(), *d, x + lv.indent, yy + y));
                }
                l = l.min(lv.indent + k.l);
                r = r.max(lv.indent + k.r);
                b = y + k.b;
                y += k.b + lv.step_gap;
            }
            shift = 0.0;
        } else {
            let mut cursor = 0.0;
            let mut centers = Vec::new();
            for k in &kids {
                let bx = cursor - k.l;
                centers.push(bx + k.w / 2.0);
                for (i, p, d, x, yy) in &k.items {
                    items.push((i.clone(), p.clone(), *d, x + bx, yy + top));
                }
                cursor += (k.r - k.l) + self.hgap;
            }
            let span = cursor - self.hgap;
            let px = (centers[0] + centers[centers.len() - 1]) / 2.0 - w / 2.0;
            // The parent is the origin: move the children's frame.
            shift = -px;
            l = px.min(0.0) + shift;
            r = (px + w).max(span) + shift;
            b = top + kids.iter().map(|k| k.b).fold(0.0, f64::max);
        }
        for it in &mut items {
            it.3 += shift;
        }
        // Direct children name this node as their parent.
        let direct: Vec<String> = node.children.iter().map(|c| c.id.clone()).collect();
        for it in &mut items {
            if it.1.is_none() && direct.contains(&it.0) {
                it.1 = Some(id.clone());
            }
        }
        items.insert(0, (id, None, depth, 0.0, 0.0));
        Sub { items, l, r, b, w }
    }

    /// The drawing for `nodes`: connectors, then boxes, and what each one is.
    pub(super) fn layout(&self, t: &Template, nodes: &[Node]) -> (Vec<Shape>, Vec<Drawn>) {
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
            let (look, _) = self.look(id, depth);
            let r = self.rect(t, look);
            (r.w, r.h)
        };
        let root = self.sub(&nodes[0], 0, &size);
        let (width, height) = ((root.r - root.l).max(1.0), root.b.max(1.0));
        // Shrink to fit the frame, keeping the margins the loaded drawing had.
        let b = self.bounds;
        let (mx, my) = (b.x.min(t.w as f64 - b.x - b.w).max(0.0), b.y.min(t.h as f64 - b.y - b.h).max(0.0));
        let k = ((t.w as f64 - 2.0 * mx) / width).min((t.h as f64 - 2.0 * my) / height).clamp(0.2, 1.0);
        // Centered where the loaded drawing was, but inside the margins.
        let x0 = (b.x + b.w / 2.0 - width * k / 2.0).clamp(mx, (t.w as f64 - mx - width * k).max(mx));
        let place = |x: f64, y: f64| (x0 + (x - root.l) * k, b.y + y * k);

        let mut rects: HashMap<&str, Rect> = HashMap::new();
        for (id, _, depth, x, y) in &root.items {
            let (w, h) = size(id, *depth);
            let (px, py) = place(*x, *y);
            rects.insert(id.as_str(), Rect { x: px, y: py, w: w * k, h: h * k });
        }
        let id_of = |tpl: usize, node: &str, own: bool| {
            let p = &t.pres[t.shape_pres[tpl]];
            if own { p.id.clone() } else { derived_id(node, &p.id) }
        };
        let mut conns = Vec::new();
        let mut boxes = Vec::new();
        let mut conn_drawn = Vec::new();
        let mut box_drawn = Vec::new();
        for (id, parent, depth, ..) in &root.items {
            let node = node_of[id];
            let (look, own) = self.look(id, *depth);
            let r = rects[id.as_str()];
            // The box.
            let tpl = self.boxes[look];
            let mut s = t.shapes[tpl].clone();
            s.xfrm = crate::Xfrm { x: r.x.round() as Emu, y: r.y.round() as Emu, w: r.w.round() as Emu, h: r.h.round() as Emu, ..s.xfrm };
            if let Some(body) = &t.shapes[tpl].text {
                let look_node = find_node(&t.nodes, look).expect("template node");
                let (old, new): (Shown, Shown) = (vec![(0, &look_node.text)], vec![(0, &node.text)]);
                let mut body = if old == new { body.clone() } else { build_text(body, &old, &new, 1.0) };
                if let Some(text) = &mut s.text {
                    let [l, tp, rr, bt] = body.insets;
                    body.insets = [(l as f64 * k) as Emu, (tp as f64 * k) as Emu, (rr as f64 * k) as Emu, (bt as f64 * k) as Emu];
                    scale_fonts(&mut body, k);
                    *text = body;
                }
            }
            boxes.push(s);
            box_drawn.push(Drawn { model_id: id_of(tpl, id, own), template: tpl, node: Some(id.clone()) });
            // The connector to the parent.
            let Some(parent) = parent else { continue };
            let pr = rects[parent.as_str()];
            let lv = self.level(*depth - 1);
            let ctpl = self.conns[look];
            let mut c = t.shapes[ctpl].clone();
            let (fill, stroke) = match &c.geometry {
                Geometry::Custom(p) => p.first().map_or((true, true), |p| (p.fill, p.stroke)),
                _ => (true, true),
            };
            let (x, y, w, h, cmds) = if lv.stack {
                let x = pr.x + lv.hang * pr.w;
                let (w, h) = ((r.x - x).max(0.0), (r.y + r.h / 2.0 - (pr.y + pr.h)).max(0.0));
                (x, pr.y + pr.h, w, h, vec![PathCmd::MoveTo(0.0, 0.0), PathCmd::LineTo(0.0, h), PathCmd::LineTo(w, h)])
            } else {
                let (pc, cc) = (pr.center().0, r.center().0);
                let (w, h) = ((cc - pc).abs(), (r.y - (pr.y + pr.h)).max(0.0));
                let m = h * lv.mid;
                let cmds = if cc >= pc {
                    vec![PathCmd::MoveTo(0.0, 0.0), PathCmd::LineTo(0.0, m), PathCmd::LineTo(w, m), PathCmd::LineTo(w, h)]
                } else {
                    vec![PathCmd::MoveTo(w, 0.0), PathCmd::LineTo(w, m), PathCmd::LineTo(0.0, m), PathCmd::LineTo(0.0, h)]
                };
                (pc.min(cc), pr.y + pr.h, w, h, cmds)
            };
            c.xfrm = crate::Xfrm { x: x.round() as Emu, y: y.round() as Emu, w: w.round() as Emu, h: h.round() as Emu, ..c.xfrm };
            let (pw, ph) = (c.xfrm.w as f64, c.xfrm.h as f64);
            // Path units are the shape's size in EMU, rounded as the shape is.
            let scale = |v: f64, from: f64, to: f64| if from > 0.0 { v * to / from } else { v };
            let cmds = cmds
                .into_iter()
                .map(|cmd| match cmd {
                    PathCmd::MoveTo(a, b) => PathCmd::MoveTo(scale(a, w, pw), scale(b, h, ph)),
                    PathCmd::LineTo(a, b) => PathCmd::LineTo(scale(a, w, pw), scale(b, h, ph)),
                    other => other,
                })
                .collect();
            c.geometry = Geometry::Custom(vec![CustomPath { w: pw.max(1.0), h: ph.max(1.0), cmds, fill, stroke }]);
            conns.push(c);
            conn_drawn.push(Drawn { model_id: id_of(ctpl, id, own), template: ctpl, node: None });
        }
        conns.extend(boxes);
        conn_drawn.extend(box_drawn);
        (conns, conn_drawn)
    }
}

fn find_node<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    super::find(nodes, id)
}
