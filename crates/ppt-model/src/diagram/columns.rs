//! Grouped list (`lProcess2`): the nodes side by side as tall columns, each a
//! title on top of a stack of boxes, one for each of its children. The
//! columns share the frame's width; a column's boxes share the room below the
//! title, shrinking as there are more of them.
//!
//! The proportions come from the loaded drawing and match the layout's own
//! rules (`lin` along the row, then along each column).

use super::{Drawn, Node, Rect, Shape, Shown, Template, build_text, derived_id, scale_fonts, shown, template_index};
use crate::{Emu, Xfrm};

#[derive(Debug)]
pub(super) struct ColumnsTpl {
    /// Each loaded top-level node's title shape, and its children's boxes.
    titles: Vec<usize>,
    boxes: Vec<Vec<usize>>,
    /// Where the row starts (left margin) and the gap between columns, as a share of a column.
    margin: f64,
    gap: f64,
    /// A column's top and height, and a column's width.
    top: f64,
    height: f64,
    width: f64,
    /// Boxes: left edge and width as shares of the column, the room they share (top and bottom against the column's top), and the gap between two at full size as a share of the column's height.
    box_left: f64,
    box_width: f64,
    area: (f64, f64),
    box_gap: f64,
}

impl ColumnsTpl {
    pub(super) fn detect(t: &Template) -> Option<ColumnsTpl> {
        let m = t.nodes.len();
        if !t.layout.ends_with("/lProcess2") || m == 0 || t.shapes.len() != t.shape_pres.len() {
            return None;
        }
        let shape_of = |id: &str| -> Option<usize> {
            let mine: Vec<usize> = (0..t.shapes.len()).filter(|&i| t.pres[t.shape_pres[i]].assoc == id).collect();
            if let [i] = mine[..] { Some(i) } else { None }
        };
        let (mut titles, mut boxes) = (Vec::new(), Vec::new());
        let mut kids = 0;
        for node in &t.nodes {
            let title = shape_of(&node.id)?;
            if t.shows(t.shape_pres[title]) != (true, false) {
                return None;
            }
            titles.push(title);
            boxes.push(node.children.iter().map(|c| shape_of(&c.id)).collect::<Option<Vec<_>>>()?);
            kids += node.children.len();
        }
        // Nothing else is drawn, and there are boxes to learn from.
        if kids == 0 || t.shapes.len() != m + kids {
            return None;
        }
        let r = |i: usize| Rect::of(&t.shapes[i].xfrm);
        let first = r(titles[0]);
        let gap = if m >= 2 { ((r(titles[1]).x - first.x) - first.w) / first.w } else { 0.075 };
        if titles.iter().any(|&i| (r(i).w - first.w).abs() > 2.0 || (r(i).y - first.y).abs() > 2.0 || (r(i).h - first.h).abs() > 2.0) || gap < 0.0 {
            return None;
        }
        // Boxes: from the column with the most of them.
        let k = (0..m).max_by_key(|&k| boxes[k].len())?;
        let (title, list) = (r(titles[k]), &boxes[k]);
        let (b0, bl) = (r(list[0]), r(*list.last()?));
        let area = (b0.y - title.y, bl.y + bl.h - title.y);
        let box_gap = if list.len() >= 2 {
            let gap_now = r(list[1]).y - (b0.y + b0.h);
            // At full size a box fills the room, so gaps and heights shrink together.
            gap_now * (area.1 - area.0) / b0.h / title.h
        } else {
            0.1
        };
        Some(ColumnsTpl { titles, boxes, margin: first.x, gap, top: first.y, height: first.h, width: first.w, box_left: (b0.x - title.x) / title.w, box_width: b0.w / title.w, area, box_gap })
    }

    pub(super) fn layout(&self, t: &Template, nodes: &[Node]) -> (Vec<Shape>, Vec<Drawn>) {
        let (n, m) = (nodes.len(), t.nodes.len());
        let usable = t.w as f64 - 2.0 * self.margin;
        let w = usable / (n as f64 + self.gap * (n as f64 - 1.0));
        let sw = w / self.width;
        let mut shapes = Vec::new();
        let mut drawn = Vec::new();
        for (j, node) in nodes.iter().enumerate() {
            let tt = template_index(j, n, m);
            let x = self.margin + j as f64 * w * (1.0 + self.gap);
            let ti = self.titles[tt];
            let mut title = t.shapes[ti].clone();
            title.xfrm = Xfrm { x: x.round() as Emu, y: self.top.round() as Emu, w: w.round() as Emu, h: self.height.round() as Emu, ..title.xfrm };
            if let Some(body) = &t.shapes[ti].text {
                let (old, new): (Shown, Shown) = (shown(&t.nodes[tt], true, false), shown(node, true, false));
                let mut body = if old == new { body.clone() } else { build_text(body, &old, &new, 1.0) };
                let [l, tp, r, b] = body.insets;
                body.insets = [(l as f64 * sw) as Emu, tp, (r as f64 * sw) as Emu, b];
                scale_fonts(&mut body, sw.min(1.0));
                title.text = Some(body);
            }
            shapes.push(title);
            drawn.push(Drawn { model_id: derived_id(&node.id, &t.pres[t.shape_pres[ti]].id), template: ti, node: Some(node.id.clone()) });
            // The boxes share the room below the title: at full size one each, less as there are more.
            let c = node.children.len();
            let room = self.area.1 - self.area.0;
            let nominal_gap = self.box_gap * self.height;
            let total = c as f64 * room + (c as f64 - 1.0) * nominal_gap;
            let k = if c >= 2 && total > room { room / total } else { 1.0 };
            let (bh, bg) = (room * k, nominal_gap * k);
            for (i, child) in node.children.iter().enumerate() {
                // What the child looks like: the same place in the loaded column, or the nearest box there is.
                let look = &self.boxes[tt];
                let bi = look.get(i).or(look.last()).copied().or_else(|| self.boxes.iter().flatten().next().copied()).expect("the template has boxes");
                let mut b = t.shapes[bi].clone();
                let (bx, by) = (x + self.box_left * w, self.top + self.area.0 + i as f64 * (bh + bg));
                let bw = self.box_width * w;
                b.xfrm = Xfrm { x: bx.round() as Emu, y: by.round() as Emu, w: bw.round() as Emu, h: bh.round() as Emu, ..b.xfrm };
                if let Some(body) = &t.shapes[bi].text {
                    let look_node = t.nodes.iter().flat_map(|n| n.children.iter()).find(|c| t.pres[t.shape_pres[bi]].assoc == c.id).expect("the box's node");
                    let (old, new): (Shown, Shown) = (shown(look_node, true, true), shown(child, true, true));
                    let mut body = if old == new { body.clone() } else { build_text(body, &old, &new, 1.0) };
                    let (th, tw) = (t.shapes[bi].xfrm.h as f64, t.shapes[bi].xfrm.w as f64);
                    let [l, tp, r, bt] = body.insets;
                    body.insets = [(l as f64 * bw / tw) as Emu, (tp as f64 * bh / th) as Emu, (r as f64 * bw / tw) as Emu, (bt as f64 * bh / th) as Emu];
                    scale_fonts(&mut body, (bw / tw).min(bh / th).min(1.0));
                    b.text = Some(body);
                }
                shapes.push(b);
                drawn.push(Drawn { model_id: derived_id(&child.id, &t.pres[t.shape_pres[bi]].id), template: bi, node: Some(child.id.clone()) });
            }
        }
        (shapes, drawn)
    }
}
