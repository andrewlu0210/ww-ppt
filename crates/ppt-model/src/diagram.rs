//! SmartArt content: the data model's tree of text nodes, and the rebuilding of
//! the stored drawing after the tree changes.
//!
//! PowerPoint saves both the data (`dgm:dataModel`) and a drawing laid out from
//! it (`dsp:drawing`). The drawing is rebuilt from the one loaded with the file:
//! shapes are retexted in place, and for layouts that arrange their nodes along
//! a line or around a circle, each node's shapes are repeated, moved and scaled.
//! Shapes are matched to nodes through presentation points (`type="pres"`).

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::sync::Arc;

use crate::{Emu, Paragraph, PathCmd, Run, Shape, TextBody, Xfrm};

mod pie;
mod radial;
mod tree;

/// Deepest level a node can be demoted to (0 is the top level).
pub const MAX_DEPTH: usize = 4;

/// A data point of type `node` with its children, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// `modelId`.
    pub id: String,
    /// One entry per paragraph.
    pub text: Vec<String>,
    pub children: Vec<Node>,
}

/// How the loaded drawing places its top-level nodes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arrangement {
    /// Anything else: text can change, nodes cannot be added or removed.
    Fixed,
    /// Evenly spaced along one axis (lists, processes, chevrons).
    Line { vertical: bool },
    /// Evenly spaced around a circle.
    Cycle,
    /// An org chart: one root, each node's children in a row or hanging below it.
    Tree,
    /// Pie slices around a circle, one per node.
    Pie,
    /// A hub with the nodes around it on an arc.
    Radial,
}

/// A presentation point: one element of the layout, usually one drawn shape.
#[derive(Clone, Debug, PartialEq)]
pub struct PresPoint {
    pub id: String,
    /// `presName`, the layout element it stands for.
    pub name: String,
    /// `presAssocID`: the data point it presents.
    pub assoc: String,
    /// Nodes whose text it shows (`presOf` connections, in `destOrd` order).
    pub sources: Vec<String>,
}

/// Ids of the points that tie a node to its parent (`dgm:cxn type="parOf"`).
#[derive(Clone, Debug, PartialEq)]
pub struct Links {
    pub cxn: String,
    pub par_trans: String,
    pub sib_trans: String,
}

/// The diagram as loaded. Never changes after loading.
#[derive(Debug)]
pub struct Template {
    /// `loTypeId`.
    pub layout: String,
    pub nodes: Vec<Node>,
    pub pres: Vec<PresPoint>,
    pub links: HashMap<String, Links>,
    /// The stored drawing, relative to a `w` × `h` frame.
    pub shapes: Vec<Shape>,
    /// Presentation point of each shape (index into `pres`).
    pub shape_pres: Vec<usize>,
    pub w: Emu,
    pub h: Emu,
    pub arrangement: Arrangement,
    /// Children are shown as paragraphs of their parent's shapes, so they can
    /// be added, removed and moved between levels.
    pub nests: bool,
    /// Levels stacked into one pyramid: each a trapezoid whose top edge is the one above's bottom edge.
    pub tapered: bool,
    /// Pie cycle (`cycle8`): what the drawing says about its slices.
    pie: Option<pie::PieTpl>,
    /// Radial list (`radial2`): what the drawing says about its hub and nodes.
    radial: Option<radial::RadialTpl>,
    /// Cycle: one arrow around the whole ring of nodes (shape index), and where its tail and
    /// tip sit after the first node, in node spacings (`cycle3`).
    ring: Option<(usize, f64, f64)>,
    /// Cycle: an arc along the ring from each node to the next (`cycle5`), and how far it
    /// stops short of each node, along the ring (EMU).
    arcs: Option<f64>,
    /// What the drawing says about an org chart's layout.
    tree: Option<tree::TreeTpl>,
    /// The original parts, for saving.
    pub data_xml: Arc<str>,
    pub drawing_xml: Arc<str>,
    units: Vec<Unit>,
    /// Shapes tied to no node (backgrounds), before and after the node shapes.
    fixed_before: Vec<usize>,
    fixed_after: Vec<usize>,
}

/// Units placed around a circle: center, radius, each unit's angle, and direction (1 clockwise).
struct Circle {
    center: (f64, f64),
    radius: f64,
    angles: Vec<f64>,
    dir: f64,
}

/// A presentation point to write: its id and the template point it copies.
pub type Planned = (String, usize);

/// One top-level node's shapes in the loaded drawing.
#[derive(Debug, Default)]
struct Unit {
    shapes: Vec<usize>,
    /// Shapes of the transition to the next node (arrows, spacers).
    trans: Vec<usize>,
    rect: Rect,
}

/// Shared, immutable template; equal only to itself.
#[derive(Clone, Debug)]
pub struct TemplateRef(pub Arc<Template>);

impl PartialEq for TemplateRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A shape of the current drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    /// `modelId` of its presentation point.
    pub model_id: String,
    /// The loaded shape it was made from (index into `Template::shapes`).
    pub template: usize,
    /// The node it belongs to, if any.
    pub node: Option<String>,
}

/// Editable SmartArt content (`ShapeKind::Diagram::data`).
#[derive(Clone, Debug, PartialEq)]
pub struct Diagram {
    pub nodes: Vec<Node>,
    /// Parallel to the drawn shapes.
    pub drawn: Vec<Drawn>,
    pub template: TemplateRef,
}

/// A line of the text pane: one node.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub depth: usize,
    pub id: String,
    /// Paragraphs joined with `\n`.
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn of(x: &Xfrm) -> Rect {
        Rect { x: x.x as f64, y: x.y as f64, w: x.w as f64, h: x.h as f64 }
    }
    fn union(self, o: Rect) -> Rect {
        if self.w <= 0.0 && self.h <= 0.0 {
            return o;
        }
        let (x0, y0) = (self.x.min(o.x), self.y.min(o.y));
        let (x1, y1) = ((self.x + self.w).max(o.x + o.w), (self.y + self.h).max(o.y + o.h));
        Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
    fn center(self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    fn around(c: (f64, f64), w: f64, h: f64) -> Rect {
        Rect { x: c.0 - w / 2.0, y: c.1 - h / 2.0, w, h }
    }
    /// (start, length) along the axis, then across it.
    fn axis(self, vertical: bool) -> (f64, f64, f64, f64) {
        if vertical { (self.y, self.h, self.x, self.w) } else { (self.x, self.w, self.y, self.h) }
    }
    fn from_axis(vertical: bool, a: f64, al: f64, c: f64, cl: f64) -> Rect {
        if vertical { Rect { x: c, y: a, w: cl, h: al } } else { Rect { x: a, y: c, w: al, h: cl } }
    }
}

/// The angle at which the circle (center `c`, radius `r`) leaves `rect` going on from
/// `from` (an angle inside it) in direction `dir` (1 clockwise, -1 the other way).
fn leave_angle(rect: Rect, c: (f64, f64), r: f64, from: f64, dir: f64) -> f64 {
    let inside = |a: f64| {
        let (x, y) = (c.0 + r * a.cos(), c.1 + r * a.sin());
        x >= rect.x && x <= rect.x + rect.w && y >= rect.y && y <= rect.y + rect.h
    };
    let step = 0.002;
    let mut a = from;
    for _ in 0..(PI / step) as usize {
        if !inside(a + dir * step) {
            break;
        }
        a += dir * step;
    }
    a
}

/// Commands of the arc of radius `r` around `c` from angle `from` (radians, clockwise) sweeping `by`,
/// as a move and cubic segments.
fn arc_path(c: (f64, f64), r: f64, from: f64, by: f64) -> Vec<PathCmd> {
    let segments = (by.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
    let step = by / segments as f64;
    let point = |a: f64| (c.0 + r * a.cos(), c.1 + r * a.sin());
    let tangent = |a: f64| (-a.sin(), a.cos());
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let start = point(from);
    let mut cmds = vec![PathCmd::MoveTo(start.0, start.1)];
    for s in 0..segments {
        let (a, b) = (from + s as f64 * step, from + (s + 1) as f64 * step);
        let (p0, p3, ta, tb) = (point(a), point(b), tangent(a), tangent(b));
        cmds.push(PathCmd::CubicTo(p0.0 + k * r * ta.0, p0.1 + k * r * ta.1, p3.0 - k * r * tb.0, p3.1 - k * r * tb.1, p3.0, p3.1));
    }
    cmds
}

/// A GUID-shaped id derived from two strings (FNV-1a), so the writer and the
/// layout name the points they create the same way.
pub fn derived_id(a: &str, b: &str) -> String {
    let hash = |seed: u64| {
        let mut h = seed;
        for byte in a.bytes().chain([0]).chain(b.bytes()) {
            h ^= byte as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    };
    let (h1, h2) = (hash(0xcbf2_9ce4_8422_2325), hash(0x8422_2325_cbf2_9ce4));
    format!("{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}", h1 >> 32, (h1 >> 16) & 0xFFFF, h1 & 0xFFFF, h2 >> 48, h2 & 0xFFFF_FFFF_FFFF)
}

/// Which loaded node (of `m`) the node at `j` (of `n`) takes its look from:
/// the first and last keep theirs (a different first shape, say), the middle
/// ones repeat the loaded middle ones.
pub fn template_index(j: usize, n: usize, m: usize) -> usize {
    if m < 3 {
        return j.min(m - 1);
    }
    if j == 0 {
        0
    } else if j + 1 == n {
        m - 1
    } else {
        j.clamp(1, m - 2)
    }
}

fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|n| if n.id == id { Some(n) } else { find(&n.children, id) })
}

/// Every node below `node`, depth first, with its depth below `node` (children are 0).
fn descendants(node: &Node) -> Vec<(usize, &Node)> {
    fn walk<'a>(n: &'a Node, depth: usize, out: &mut Vec<(usize, &'a Node)>) {
        for c in &n.children {
            out.push((depth, c));
            walk(c, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(node, 0, &mut out);
    out
}

fn flatten(nodes: &[Node]) -> Vec<(usize, Node)> {
    fn walk(nodes: &[Node], depth: usize, out: &mut Vec<(usize, Node)>) {
        for n in nodes {
            out.push((depth, Node { children: Vec::new(), ..n.clone() }));
            walk(&n.children, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, 0, &mut out);
    out
}

/// Rebuilds the tree from outline rows (each depth at most one more than the row before).
fn unflatten(rows: Vec<(usize, Node)>) -> Vec<Node> {
    fn take(rows: &[(usize, Node)], at: &mut usize, depth: usize) -> Vec<Node> {
        let mut out = Vec::new();
        while *at < rows.len() && rows[*at].0 == depth {
            let mut node = rows[*at].1.clone();
            *at += 1;
            node.children = take(rows, at, depth + 1);
            out.push(node);
        }
        out
    }
    let mut at = 0;
    take(&rows, &mut at, 0)
}

/// End (exclusive) of the subtree starting at row `i`.
fn subtree_end(rows: &[(usize, Node)], i: usize) -> usize {
    let d = rows[i].0;
    (i + 1..rows.len()).find(|&k| rows[k].0 <= d).unwrap_or(rows.len())
}

/// Text and its paragraph structure as shown by one shape: (level, lines) per source node.
type Shown<'a> = Vec<(u8, &'a [String])>;

/// Paragraphs for `new`, styled after the paragraphs `tpl` showed for `old`.
fn build_text(tpl: &TextBody, old: &Shown, new: &Shown, font: f64) -> TextBody {
    let mut body = tpl.clone();
    if tpl.paragraphs.is_empty() {
        return body;
    }
    // Paragraph range each old source occupied.
    let mut ranges = Vec::new();
    let mut at = 0;
    for (_, lines) in old {
        let len = lines.len().max(1);
        ranges.push(at..at + len);
        at += len;
    }
    let last = tpl.paragraphs.len() - 1;
    let pick = |source: usize, line: usize, level: u8| -> &Paragraph {
        let by_source = ranges.get(source.min(ranges.len().saturating_sub(1))).map(|r| (r.start + line.min(r.len() - 1)).min(last));
        let p = &tpl.paragraphs[by_source.unwrap_or(last)];
        if p.level == level { p } else { tpl.paragraphs.iter().find(|q| q.level == level).unwrap_or(p) }
    };
    body.paragraphs = Vec::new();
    for (i, (level, lines)) in new.iter().enumerate() {
        let lines: &[String] = if lines.is_empty() { &[String::new()] } else { lines };
        for (k, line) in lines.iter().enumerate() {
            let t = pick(i, k, *level);
            let mut style = t.first_style().clone();
            style.size_pt = (style.size_pt as f64 * font) as f32;
            let runs = if line.is_empty() { Vec::new() } else { vec![Run { text: line.clone(), style: style.clone() }] };
            body.paragraphs.push(Paragraph { runs, level: *level, end_style: style, ..t.clone() });
        }
    }
    if body.paragraphs.is_empty() {
        body.paragraphs.push(Paragraph { runs: Vec::new(), ..tpl.paragraphs[0].clone() });
    }
    body
}

fn scale_fonts(body: &mut TextBody, font: f64) {
    if (font - 1.0).abs() < 1e-6 {
        return;
    }
    for p in &mut body.paragraphs {
        for r in &mut p.runs {
            r.style.size_pt = (r.style.size_pt as f64 * font) as f32;
        }
        p.end_style.size_pt = (p.end_style.size_pt as f64 * font) as f32;
    }
}

/// `shape` moved from `from` to `to` (scaling its frame, insets and fonts).
fn placed(shape: &Shape, from: Rect, to: Rect, font: f64) -> Shape {
    let sx = if from.w > 0.0 { to.w / from.w } else { 1.0 };
    let sy = if from.h > 0.0 { to.h / from.h } else { 1.0 };
    let mut s = shape.clone();
    let x = &shape.xfrm;
    s.xfrm.x = (to.x + (x.x as f64 - from.x) * sx).round() as Emu;
    s.xfrm.y = (to.y + (x.y as f64 - from.y) * sy).round() as Emu;
    s.xfrm.w = (x.w as f64 * sx).round() as Emu;
    s.xfrm.h = (x.h as f64 * sy).round() as Emu;
    if let Some(body) = &mut s.text {
        let [l, t, r, b] = body.insets;
        body.insets = [(l as f64 * sx) as Emu, (t as f64 * sy) as Emu, (r as f64 * sx) as Emu, (b as f64 * sy) as Emu];
        scale_fonts(body, font);
    }
    s
}

/// `shape` turned by `angle` (radians, clockwise) around `c`, brought `rho` times
/// as far from it, and scaled by `k` about its own center.
fn turned(shape: &Shape, c: (f64, f64), angle: f64, k: f64, rho: f64) -> Shape {
    let r = Rect::of(&shape.xfrm);
    let (cx, cy) = r.center();
    let (dx, dy) = ((cx - c.0) * rho, (cy - c.1) * rho);
    let (sin, cos) = angle.sin_cos();
    let center = (c.0 + dx * cos - dy * sin, c.1 + dx * sin + dy * cos);
    let mut s = placed(shape, r, Rect::around(center, r.w * k, r.h * k), k.min(1.0));
    s.xfrm.rot = (shape.xfrm.rot + angle.to_degrees()).rem_euclid(360.0);
    s
}

impl Template {
    /// Builds a template, working out how its drawing arranges the nodes.
    #[allow(clippy::too_many_arguments)]
    pub fn new(layout: String, nodes: Vec<Node>, pres: Vec<PresPoint>, links: HashMap<String, Links>, shapes: Vec<Shape>, shape_pres: Vec<usize>, w: Emu, h: Emu, data_xml: Arc<str>, drawing_xml: Arc<str>) -> Template {
        let mut t = Template { layout, nodes, pres, links, shapes, shape_pres, w, h, arrangement: Arrangement::Fixed, nests: false, tapered: false, ring: None, arcs: None, pie: None, radial: None, tree: None, data_xml, drawing_xml, units: Vec::new(), fixed_before: Vec::new(), fixed_after: Vec::new() };
        t.analyze();
        t
    }

    fn analyze(&mut self) {
        if let Some(radial) = radial::RadialTpl::detect(self) {
            self.nests = self.units_show_children();
            self.radial = Some(radial);
            self.arrangement = Arrangement::Radial;
            return;
        }
        if let Some(pie) = pie::PieTpl::detect(self) {
            self.pie = Some(pie);
            self.arrangement = Arrangement::Pie;
            return;
        }
        if let Some(tree) = tree::TreeTpl::detect(self) {
            self.tree = Some(tree);
            self.arrangement = Arrangement::Tree;
            self.nests = true;
            return;
        }
        let m = self.nodes.len();
        let top: HashMap<&str, usize> = self.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
        let sib: HashMap<&str, usize> = self.nodes.iter().enumerate().filter_map(|(i, n)| Some((self.links.get(&n.id)?.sib_trans.as_str(), i))).collect();
        let below: HashSet<&str> = self.nodes.iter().flat_map(|n| descendants(n).into_iter().map(|(_, d)| d.id.as_str())).collect();
        let mut units: Vec<Unit> = (0..m).map(|_| Unit::default()).collect();
        let mut seen_node = false;
        let mut movable = m >= 2 && self.shapes.len() == self.shape_pres.len();
        for (i, &p) in self.shape_pres.iter().enumerate() {
            let assoc = self.pres[p].assoc.as_str();
            if let Some(&u) = top.get(assoc) {
                units[u].shapes.push(i);
                seen_node = true;
            } else if let Some(&u) = sib.get(assoc) {
                units[u].trans.push(i);
                seen_node = true;
            } else if below.contains(assoc) {
                // Children drawn as shapes of their own (org charts): too varied to repeat.
                movable = false;
            } else if self.shapes[i].text.as_ref().is_some_and(|t| !t.is_empty()) {
                // Text tied to the diagram as a whole, laid out by rules we don't follow.
                movable = false;
            } else if seen_node {
                self.fixed_after.push(i);
            } else {
                self.fixed_before.push(i);
            }
        }
        for u in &mut units {
            u.rect = u.shapes.iter().fold(Rect::default(), |r, &i| r.union(Rect::of(&self.shapes[i].xfrm)));
            if u.shapes.is_empty() {
                movable = false;
            }
        }
        // Children can be edited when every node's shapes have room for them.
        self.nests = movable && units.iter().all(|u| u.shapes.iter().any(|&i| self.shows(self.shape_pres[i]).1));
        self.units = units;
        if !movable {
            return;
        }
        let ring = self.find_ring();
        let arcs = self.find_arcs();
        self.arrangement = if self.is_cycle(ring.map(|r| r.0), arcs.is_some()) {
            self.ring = ring;
            self.arcs = arcs;
            Arrangement::Cycle
        } else if let Some(vertical) = self.line_axis() {
            Arrangement::Line { vertical }
        } else {
            Arrangement::Fixed
        };
        self.tapered = self.arrangement == (Arrangement::Line { vertical: true }) && self.is_pyramid();
    }

    /// The axis the units are evenly spaced along, if they are.
    fn line_axis(&self) -> Option<bool> {
        let centers: Vec<(f64, f64)> = self.units.iter().map(|u| u.rect.center()).collect();
        let spread = |f: fn(&(f64, f64)) -> f64| {
            let v: Vec<f64> = centers.iter().map(f).collect();
            v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
        };
        let vertical = spread(|c| c.1) > spread(|c| c.0);
        let rects: Vec<_> = self.units.iter().map(|u| u.rect.axis(vertical)).collect();
        let steps: Vec<f64> = rects.windows(2).map(|w| w[1].0 - w[0].0).collect();
        let mean = steps.iter().sum::<f64>() / steps.len() as f64;
        let cross = rects.iter().map(|r| r.3).fold(0.0, f64::max).max(1.0);
        let even = mean > 0.0 && steps.iter().all(|s| (s - mean).abs() <= mean * 0.1);
        let aligned = rects.iter().all(|r| ((r.2 + r.3 / 2.0) - (rects[0].2 + rects[0].3 / 2.0)).abs() <= cross * 0.1);
        (even && aligned).then_some(vertical)
    }

    /// One trapezoid per node, each as wide at its bottom as the level's number
    /// times the top one's, centered on one axis.
    fn is_pyramid(&self) -> bool {
        let one: Option<Vec<&Shape>> = self.units.iter().map(|u| (u.shapes.len() == 1 && u.trans.is_empty()).then(|| &self.shapes[u.shapes[0]])).collect();
        let Some(shapes) = one else { return false };
        let trapezoid = |s: &&Shape| matches!(&s.geometry, crate::Geometry::Preset { name, .. } if name == "trapezoid");
        let w0 = shapes[0].xfrm.w as f64;
        let cx = Rect::of(&shapes[0].xfrm).center().0;
        shapes.iter().all(trapezoid) && w0 > 0.0 && shapes.iter().enumerate().all(|(j, s)| (s.xfrm.w as f64 - w0 * (j + 1) as f64).abs() <= w0 * 0.03 && (Rect::of(&s.xfrm).center().0 - cx).abs() <= w0 * 0.03)
    }

    fn cycle_geometry(&self) -> Option<Circle> {
        let m = self.units.len();
        if m < 3 {
            return None;
        }
        let centers: Vec<(f64, f64)> = self.units.iter().map(|u| u.rect.center()).collect();
        let c = (centers.iter().map(|p| p.0).sum::<f64>() / m as f64, centers.iter().map(|p| p.1).sum::<f64>() / m as f64);
        let dist: Vec<f64> = centers.iter().map(|p| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt()).collect();
        let r = dist.iter().sum::<f64>() / m as f64;
        let angles: Vec<f64> = centers.iter().map(|p| (p.1 - c.1).atan2(p.0 - c.0)).collect();
        let step = |i: usize| (angles[(i + 1) % m] - angles[i]).rem_euclid(2.0 * PI);
        let dir = if step(0) < PI { 1.0 } else { -1.0 };
        let want = 2.0 * PI / m as f64;
        let even = (0..m).all(|i| {
            let s = if dir > 0.0 { step(i) } else { 2.0 * PI - step(i) };
            (s - want).abs() <= want * 0.15
        });
        (r > 0.0 && dist.iter().all(|d| (d - r).abs() <= r * 0.1) && even).then_some(Circle { center: c, radius: r, angles, dir })
    }

    /// A circular arrow around the whole ring, tied to the first node alone: its shape,
    /// and the offsets of its tail and tip from that node in node spacings.
    fn find_ring(&self) -> Option<(usize, f64, f64)> {
        let m = self.units.len();
        let [i] = self.units.first()?.trans[..] else { return None };
        if m < 3 || self.units[1..].iter().any(|u| !u.trans.is_empty()) {
            return None;
        }
        let crate::Geometry::Preset { name, adjust } = &self.shapes[i].geometry else { return None };
        let adj = |k: &str| adjust.iter().find(|(n, _)| n == k).map(|a| a.1);
        let (tip, tail) = (adj("adj3")? / 60_000.0, adj("adj4")? / 60_000.0);
        let Circle { angles, dir, .. } = self.cycle_geometry()?;
        if name != "circularArrow" || dir < 0.0 || self.shapes[i].xfrm.rot != 0.0 {
            return None;
        }
        let (first, step) = (angles[0].to_degrees(), 360.0 / m as f64);
        Some((i, (tail - first).rem_euclid(360.0) / step, ((tip - first + 180.0).rem_euclid(360.0) - 180.0) / step))
    }

    /// Arcs along the ring, one from each node to the next, each ending short of the nodes:
    /// how short, along the ring.
    fn find_arcs(&self) -> Option<f64> {
        let m = self.units.len();
        let Circle { center: c, radius: r, dir, .. } = self.cycle_geometry()?;
        if dir < 0.0 || self.units.iter().any(|u| u.trans.len() != 1 || u.shapes.len() != 1) {
            return None;
        }
        let arc = |u: &Unit| -> Option<((f64, f64), (f64, f64))> {
            let s = &self.shapes[u.trans[0]];
            let crate::Geometry::Custom(paths) = &s.geometry else { return None };
            let [path] = &paths[..] else { return None };
            if s.xfrm.rot != 0.0 || path.w < 0.0 || path.h < 0.0 {
                return None;
            }
            // Path coordinates are in EMU when the path has no size of its own.
            let (sx, sy) = (if path.w > 0.0 { s.xfrm.w as f64 / path.w } else { 1.0 }, if path.h > 0.0 { s.xfrm.h as f64 / path.h } else { 1.0 });
            let (PathCmd::MoveTo(x0, y0), Some(end)) = (path.cmds.first()?, path.cmds.last()) else { return None };
            let (x1, y1) = match end {
                PathCmd::CubicTo(.., x, y) => (*x, *y),
                _ => return None,
            };
            let o = (s.xfrm.x as f64, s.xfrm.y as f64);
            Some(((o.0 + x0 * sx, o.1 + y0 * sy), (o.0 + x1 * sx, o.1 + y1 * sy)))
        };
        let angle = |p: (f64, f64)| (p.1 - c.1).atan2(p.0 - c.0);
        let ahead = |from: f64, to: f64| (to - from).rem_euclid(2.0 * PI);
        let mut margins = Vec::new();
        for (j, u) in self.units.iter().enumerate() {
            let ((s, e), next) = (arc(u)?, &self.units[(j + 1) % m]);
            // The arc has to sit on the ring through the node centers.
            let on_ring = |p: (f64, f64)| (((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt() - r).abs() <= r * 0.03;
            if !on_ring(s) || !on_ring(e) {
                return None;
            }
            let (here, there) = (u.rect.center(), next.rect.center());
            let leave = leave_angle(u.rect, c, r, angle(here), 1.0);
            let enter = leave_angle(next.rect, c, r, angle(there), -1.0);
            margins.push(ahead(leave, angle(s)) * r);
            margins.push(ahead(angle(e), enter) * r);
        }
        let mean = margins.iter().sum::<f64>() / margins.len() as f64;
        (margins.iter().all(|&g| g >= -r * 0.02 && g <= r * 0.5) && mean >= 0.0).then_some(mean)
    }

    fn is_cycle(&self, ring: Option<usize>, arcs: bool) -> bool {
        let Some(Circle { radius: r, .. }) = self.cycle_geometry() else { return false };
        // Connectors between nodes must be small (not arcs around the whole circle).
        self.units.iter().flat_map(|u| &u.trans).filter(|&&i| Some(i) != ring && !arcs).all(|&i| {
            let x = &self.shapes[i].xfrm;
            (x.w.max(x.h) as f64) < r
        })
    }

    fn links_of(&self, id: &str) -> Links {
        self.links.get(id).cloned().unwrap_or_else(|| Links { cxn: derived_id(id, "cxn"), par_trans: derived_id(id, "parTrans"), sib_trans: derived_id(id, "sibTrans") })
    }

    /// Every top-level node has a shape showing its children's text, so children can be edited.
    fn units_show_children(&self) -> bool {
        self.nodes.iter().all(|n| (0..self.shapes.len()).any(|i| self.pres[self.shape_pres[i]].assoc == n.id && self.shows(self.shape_pres[i]).1))
    }

    /// Org charts: the loaded node whose look the node `id` at `depth` takes,
    /// and whether it is the node itself (still at the depth it was loaded at).
    pub fn tree_look(&self, id: &str, depth: usize) -> Option<(&str, bool)> {
        self.tree.as_ref().map(|t| t.look(id, depth))
    }

    /// Whether the transition shapes after the last node are drawn (a closed cycle).
    pub fn wraps(&self) -> bool {
        self.arrangement == Arrangement::Cycle && self.units.last().is_some_and(|u| !u.trans.is_empty())
    }

    /// Presentation points of node `node` at position `j` of `n`, as
    /// (id, template point), and those of its transition to the next node.
    /// Points the node already had keep their ids; others are derived.
    pub fn plan(&self, node: &str, j: usize, n: usize) -> (Vec<Planned>, Vec<Planned>) {
        let m = self.nodes.len();
        let t = template_index(j, n, m);
        let tpl = &self.nodes[t].id;
        let was_top = self.nodes.iter().any(|x| x.id == node);
        let mine = |assoc: &str, own_assoc: &str, derive_from: &str| -> Vec<Planned> {
            let mut used: HashMap<&str, usize> = HashMap::new();
            self.pres
                .iter()
                .enumerate()
                .filter(|(_, p)| p.assoc == assoc)
                .map(|(i, p)| {
                    let k = used.entry(p.name.as_str()).or_default();
                    let own = was_top.then(|| self.pres.iter().filter(|q| q.assoc == own_assoc && q.name == p.name).nth(*k)).flatten();
                    *k += 1;
                    (own.map_or_else(|| derived_id(derive_from, &p.id), |q| q.id.clone()), i)
                })
                .collect()
        };
        let unit = mine(tpl, node, node);
        let last = j + 1 == n;
        let trans = if last && !self.wraps() {
            Vec::new()
        } else {
            let tt = if last { m - 1 } else { j.min(m.saturating_sub(2)) };
            let sib = self.links_of(node).sib_trans;
            mine(&self.links_of(&self.nodes[tt].id).sib_trans, &sib, &sib)
        };
        (unit, trans)
    }

    /// What a template point shows of the node it presents: (its own text, its children's).
    pub fn shows(&self, p: usize) -> (bool, bool) {
        let pres = &self.pres[p];
        let Some(node) = find(&self.nodes, &pres.assoc) else { return (false, false) };
        if pres.sources.is_empty() {
            // An empty children box (`desTx`, `childText`) of a node without children.
            let name = pres.name.to_ascii_lowercase();
            return (false, name.contains("des") || name.contains("child"));
        }
        let below: HashSet<&str> = descendants(node).into_iter().map(|(_, d)| d.id.as_str()).collect();
        (pres.sources.contains(&node.id), pres.sources.iter().any(|s| below.contains(s.as_str())))
    }

    fn same_structure(&self, nodes: &[Node]) -> bool {
        fn same(a: &[Node], b: &[Node]) -> bool {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.id == y.id && same(&x.children, &y.children))
        }
        same(&self.nodes, nodes)
    }
}

/// What `node` shows through a point that shows its own text and/or its children's.
fn shown(node: &Node, own: bool, desc: bool) -> Shown<'_> {
    let mut out: Shown = Vec::new();
    if own {
        out.push((0, &node.text));
    }
    if desc {
        let base = own as u8;
        out.extend(descendants(node).into_iter().map(|(d, n)| (base + d as u8, n.text.as_slice())));
    }
    out
}

/// What the sources of a point show, levels relative to the shallowest one.
fn shown_by<'a>(nodes: &'a [Node], sources: &[String]) -> Shown<'a> {
    fn depth_of(nodes: &[Node], id: &str, d: usize) -> Option<usize> {
        nodes.iter().find_map(|n| if n.id == id { Some(d) } else { depth_of(&n.children, id, d + 1) })
    }
    let found: Vec<(usize, &Node)> = sources.iter().filter_map(|s| Some((depth_of(nodes, s, 0)?, find(nodes, s)?))).collect();
    let min = found.iter().map(|f| f.0).min().unwrap_or(0);
    found.into_iter().map(|(d, n)| ((d - min) as u8, n.text.as_slice())).collect()
}

impl Diagram {
    pub fn new(template: Arc<Template>) -> Diagram {
        let drawn = template
            .shape_pres
            .iter()
            .enumerate()
            .map(|(i, &p)| Drawn { model_id: template.pres[p].id.clone(), template: i, node: find(&template.nodes, &template.pres[p].assoc).map(|n| n.id.clone()) })
            .collect();
        Diagram { nodes: template.nodes.clone(), drawn, template: TemplateRef(template) }
    }

    pub fn arrangement(&self) -> Arrangement {
        self.template.0.arrangement
    }

    /// Nodes can be added, removed and reordered.
    pub fn can_restructure(&self) -> bool {
        self.arrangement() != Arrangement::Fixed
    }

    /// Children can be added and nodes moved between levels.
    pub fn can_nest(&self) -> bool {
        self.can_restructure() && self.template.0.nests
    }

    /// An org chart: one root, with everything else below it.
    pub fn is_tree(&self) -> bool {
        self.arrangement() == Arrangement::Tree
    }

    /// Rebuilds the drawing for the current nodes: shapes relative to the
    /// template's frame, and what each one is.
    pub fn layout(&mut self) -> Vec<Shape> {
        let t = self.template.0.clone();
        let (shapes, drawn) = match &t.tree {
            Some(tree) if !t.same_structure(&self.nodes) => tree.layout(&t, &self.nodes),
            _ if t.radial.is_some() && !t.same_structure(&self.nodes) => t.radial.as_ref().expect("checked").layout(&t, &self.nodes),
            _ if t.pie.is_some() && !t.same_structure(&self.nodes) => t.pie.as_ref().expect("checked").layout(&t, &self.nodes),
            _ if t.same_structure(&self.nodes) || t.arrangement == Arrangement::Fixed => self.retext(&t),
            _ => self.rearrange(&t),
        };
        self.drawn = drawn;
        shapes
    }

    /// Same shapes, text updated where a node's text changed.
    fn retext(&self, t: &Template) -> (Vec<Shape>, Vec<Drawn>) {
        let mut shapes = Vec::new();
        let mut drawn = Vec::new();
        for (i, shape) in t.shapes.iter().enumerate() {
            let p = &t.pres[t.shape_pres.get(i).copied().unwrap_or(0)];
            let mut s = shape.clone();
            let old = shown_by(&t.nodes, &p.sources);
            let new = shown_by(&self.nodes, &p.sources);
            if old != new
                && let Some(body) = &shape.text
            {
                s.text = Some(build_text(body, &old, &new, 1.0));
            }
            shapes.push(s);
            drawn.push(Drawn { model_id: p.id.clone(), template: i, node: find(&self.nodes, &p.assoc).map(|n| n.id.clone()) });
        }
        (shapes, drawn)
    }

    fn rearrange(&self, t: &Template) -> (Vec<Shape>, Vec<Drawn>) {
        let (m, n) = (t.units.len(), self.nodes.len());
        let mut shapes = Vec::new();
        let mut drawn = Vec::new();
        for &i in &t.fixed_before {
            shapes.push(t.shapes[i].clone());
            drawn.push(Drawn { model_id: t.pres[t.shape_pres[i]].id.clone(), template: i, node: None });
        }
        let geometry = Geometry::new(t, n);
        for (j, node) in self.nodes.iter().enumerate() {
            let tu = template_index(j, n, m);
            let (unit_pres, trans_pres) = t.plan(&node.id, j, n);
            let ids: HashMap<usize, &String> = unit_pres.iter().chain(&trans_pres).map(|(id, p)| (*p, id)).collect();
            for &i in &t.units[tu].shapes {
                let p = t.shape_pres[i];
                let mut s = geometry.unit_shape(t, i, tu, j);
                let (own, desc) = t.shows(p);
                if own || desc {
                    let old = shown(&t.nodes[tu], own, desc);
                    let new = shown(node, own, desc);
                    if old != new
                        && let Some(body) = &s.text
                    {
                        s.text = Some(build_text(body, &old, &new, 1.0));
                    }
                }
                shapes.push(s);
                drawn.push(Drawn { model_id: ids.get(&p).map_or_else(|| derived_id(&node.id, &t.pres[p].id), |s| (*s).clone()), template: i, node: Some(node.id.clone()) });
            }
            if trans_pres.is_empty() {
                continue;
            }
            let tt = if j + 1 == n { m - 1 } else { j.min(m - 2) };
            for &i in &t.units[tt].trans {
                let p = t.shape_pres[i];
                shapes.push(geometry.trans_shape(t, i, tt, j));
                drawn.push(Drawn { model_id: ids.get(&p).map_or_else(|| derived_id(&t.links_of(&node.id).sib_trans, &t.pres[p].id), |s| (*s).clone()), template: i, node: None });
            }
        }
        for &i in &t.fixed_after {
            shapes.push(t.shapes[i].clone());
            drawn.push(Drawn { model_id: t.pres[t.shape_pres[i]].id.clone(), template: i, node: None });
        }
        (shapes, drawn)
    }

    // --- Text pane -----------------------------------------------------------

    pub fn rows(&self) -> Vec<Row> {
        flatten(&self.nodes).into_iter().map(|(depth, n)| Row { depth, id: n.id, text: n.text.join("\n") }).collect()
    }

    /// Replaces row `i`'s text (paragraphs separated by `\n`).
    pub fn set_text(&mut self, i: usize, text: &str) -> bool {
        let mut rows = flatten(&self.nodes);
        let Some(row) = rows.get_mut(i) else { return false };
        let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        if row.1.text == lines {
            return false;
        }
        row.1.text = lines;
        self.nodes = unflatten(rows);
        true
    }

    fn fresh_id(&self) -> String {
        let mut taken: HashSet<String> = flatten(&self.nodes).into_iter().map(|(_, n)| n.id).collect();
        let t = &self.template.0;
        taken.extend(flatten(&t.nodes).into_iter().map(|(_, n)| n.id));
        taken.extend(t.pres.iter().map(|p| p.id.clone()));
        (0..).map(|k| derived_id(&t.layout, &format!("node{k}"))).find(|id| !taken.contains(id)).expect("unbounded")
    }

    /// Adds a node at row `i`'s level, after (or before) it and its children,
    /// or as its last child. Returns the new row.
    pub fn insert(&mut self, i: usize, place: Insert) -> Option<usize> {
        let mut rows = flatten(&self.nodes);
        let depth = rows.get(i)?.0;
        // An org chart has one root: a node "beside" it goes under it.
        let place = if depth == 0 && self.is_tree() { Insert::Child } else { place };
        let (at, depth) = match place {
            Insert::After => (subtree_end(&rows, i), depth),
            Insert::Before => (i, depth),
            Insert::Child => (subtree_end(&rows, i), depth + 1),
        };
        let allowed = if depth == 0 { self.can_restructure() } else { self.can_nest() && depth <= MAX_DEPTH };
        if !allowed {
            return None;
        }
        let node = Node { id: self.fresh_id(), text: vec![String::new()], children: Vec::new() };
        rows.insert(at, (depth, node));
        self.nodes = unflatten(rows);
        Some(at)
    }

    /// Removes row `i` and its children. Returns the row to go to.
    pub fn delete(&mut self, i: usize) -> Option<usize> {
        let mut rows = flatten(&self.nodes);
        let depth = rows.get(i)?.0;
        if !(if depth == 0 { self.can_restructure() } else { self.can_nest() }) {
            return None;
        }
        let end = subtree_end(&rows, i);
        if depth == 0 && rows.iter().filter(|r| r.0 == 0).count() == 1 {
            return None;
        }
        rows.drain(i..end);
        self.nodes = unflatten(rows);
        Some(i.saturating_sub(1).min(flatten(&self.nodes).len() - 1))
    }

    /// Demotes (`deeper`) or promotes row `i` with its children.
    pub fn shift(&mut self, i: usize, deeper: bool) -> bool {
        if !self.can_nest() {
            return false;
        }
        let mut rows = flatten(&self.nodes);
        let Some(&(depth, _)) = rows.get(i) else { return false };
        let end = subtree_end(&rows, i);
        if deeper {
            // Needs an earlier sibling to become the parent.
            if i == 0 || rows[i - 1].0 < depth || rows[i..end].iter().any(|r| r.0 + 1 > MAX_DEPTH) {
                return false;
            }
            rows[i..end].iter_mut().for_each(|r| r.0 += 1);
        } else {
            if depth == 0 || (depth == 1 && self.is_tree()) {
                return false;
            }
            rows[i..end].iter_mut().for_each(|r| r.0 -= 1);
        }
        self.nodes = unflatten(rows);
        true
    }

    /// Swaps row `i` (with its children) with the sibling before or after it.
    /// Returns the row it moved to.
    pub fn move_row(&mut self, i: usize, up: bool) -> Option<usize> {
        let rows = flatten(&self.nodes);
        let depth = rows.get(i)?.0;
        if !(if depth == 0 { self.can_restructure() } else { self.can_nest() }) {
            return None;
        }
        let end = subtree_end(&rows, i);
        let (a, b, c) = if up {
            let prev = (0..i).rev().find(|&k| rows[k].0 <= depth).filter(|&k| rows[k].0 == depth)?;
            (prev, i, end)
        } else {
            let next = Some(end).filter(|&k| k < rows.len() && rows[k].0 == depth)?;
            (i, next, subtree_end(&rows, next))
        };
        let mut out = rows[..a].to_vec();
        out.extend_from_slice(&rows[b..c]);
        out.extend_from_slice(&rows[a..b]);
        out.extend_from_slice(&rows[c..]);
        self.nodes = unflatten(out);
        Some(if up { a } else { a + (c - b) })
    }

    /// The row of the node whose own text, and nothing else, drawn shape
    /// `shape` shows (so editing that text edits the node).
    pub fn row_of_own_text(&self, shape: usize) -> Option<usize> {
        let t = &self.template.0;
        let p = *t.shape_pres.get(self.drawn.get(shape)?.template)?;
        if t.shows(p) != (true, false) {
            return None;
        }
        self.row_of_shape(shape)
    }

    /// The node a drawn shape belongs to, as a row.
    pub fn row_of_shape(&self, shape: usize) -> Option<usize> {
        let id = self.drawn.get(shape)?.node.as_ref()?;
        flatten(&self.nodes).iter().position(|(_, n)| &n.id == id)
    }
}

/// Where [`Diagram::insert`] puts the new node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Insert {
    After,
    Before,
    Child,
}

/// Where each node and transition goes for `n` nodes.
struct Geometry {
    kind: Arrangement,
    /// Line: unit rects for the new nodes; Cycle: centers.
    rects: Vec<Rect>,
    /// Line: scale along and across the axis; Cycle: node scale in `along`.
    along: f64,
    across: f64,
    /// Line: start of the cross extent before and after.
    cross0: (f64, f64),
    /// Cycle: center, start angle, direction, old and new count.
    center: (f64, f64),
    angle0: f64,
    dir: f64,
    m: usize,
    n: usize,
    /// Pyramid: how far each level's top edge is inset from its bottom edge, per side.
    taper: Option<f64>,
    /// Cycle: how much closer to the center than loaded the nodes sit, to stay inside the frame.
    rho: f64,
    /// Cycle: the ring arrow's shape and offsets (see [`Template::ring`]).
    ring: Option<(usize, f64, f64)>,
    /// Cycle: how far arcs between nodes stop short of them, along the ring.
    arcs: Option<f64>,
}

impl Geometry {
    fn new(t: &Template, n: usize) -> Geometry {
        let m = t.units.len();
        let mut g = Geometry { kind: t.arrangement, rects: Vec::new(), along: 1.0, across: 1.0, cross0: (0.0, 0.0), center: (0.0, 0.0), angle0: 0.0, dir: 1.0, m, n, taper: None, rho: 1.0, ring: t.ring, arcs: t.arcs };
        match t.arrangement {
            Arrangement::Line { vertical } => {
                let r: Vec<_> = t.units.iter().map(|u| u.rect.axis(vertical)).collect();
                let size = r.iter().map(|r| r.1).sum::<f64>() / m as f64;
                let gap = r.windows(2).map(|w| w[1].0 - (w[0].0 + w[0].1)).sum::<f64>() / (m - 1) as f64;
                let start = r[0].0;
                let span = r[m - 1].0 + r[m - 1].1 - start;
                let c0 = r.iter().map(|r| r.2).fold(f64::MAX, f64::min);
                let extent = r.iter().map(|r| r.2 + r.3).fold(f64::MIN, f64::max) - c0;
                if t.tapered {
                    // Levels share the height and widen by one step each.
                    let (top, bottom) = (r[0].0, r[m - 1].0 + r[m - 1].1);
                    let (cx, width) = (t.units[m - 1].rect.center().0, t.units[m - 1].rect.w);
                    let level = (bottom - top) / n as f64;
                    g.along = (m as f64 / n as f64).min(1.0);
                    g.across = g.along;
                    g.rects = (0..n)
                        .map(|j| {
                            let w = width * (j + 1) as f64 / n as f64;
                            Rect { x: cx - w / 2.0, y: top + j as f64 * level, w, h: level }
                        })
                        .collect();
                    g.taper = Some(width / (2.0 * n as f64));
                    return g;
                }
                let frame = if vertical { t.w } else { t.h } as f64;
                let total = n as f64 * size + (n as f64 - 1.0) * gap;
                let fit = if total > 0.0 { span / total } else { 1.0 };
                // Nodes filling the cross direction keep their width; others keep their shape.
                let (along, across) = if extent >= frame * 0.9 { (fit, 1.0) } else { let k = fit.min(frame / extent.max(1.0)); (k, k) };
                let first = start + (span - total * along) / 2.0;
                let c0_new = c0 + (extent - extent * across) / 2.0;
                g.along = along;
                g.across = across;
                g.cross0 = (c0, c0_new);
                g.rects = (0..n)
                    .map(|j| {
                        let tu = &r[template_index(j, n, m)];
                        Rect::from_axis(vertical, first + j as f64 * (size + gap) * along, tu.1 * along, c0_new + (tu.2 - c0) * across, tu.3 * across)
                    })
                    .collect();
            }
            Arrangement::Cycle => {
                let Circle { center: c, radius: r, angles, dir } = t.cycle_geometry().expect("cycle checked when loading");
                g.center = c;
                g.angle0 = angles[0];
                g.dir = dir;
                // Nodes shrink with the gaps between them, but stay inside the frame.
                let chord = (PI / n.max(2) as f64).sin() / (PI / m as f64).sin();
                let size = t.units.iter().map(|u| u.rect.w.max(u.rect.h)).fold(0.0, f64::max);
                let room = c.0.min(c.1).min(t.w as f64 - c.0).min(t.h as f64 - c.1);
                // Growing (fewer nodes) stops at the frame; the loaded size always fits.
                let fit = if size > 0.0 { (2.0 * (room - r) / size).max(1.0) } else { 1.0 };
                g.along = chord.min(fit);
                // Another count puts nodes where the loaded ring did not reach: pull them in until they fit.
                let half = (
                    t.units.iter().map(|u| u.rect.w).fold(0.0, f64::max) * g.along / 2.0,
                    t.units.iter().map(|u| u.rect.h).fold(0.0, f64::max) * g.along / 2.0,
                );
                let reach = |room: f64, d: f64| if d.abs() > 1e-6 { room / d.abs() } else { f64::INFINITY };
                g.rho = (0..n)
                    .map(|j| {
                        let a = g.angle0 + dir * 2.0 * PI * j as f64 / n as f64;
                        let (cx, cy) = (a.cos(), a.sin());
                        let x = if cx > 0.0 { t.w as f64 - c.0 - half.0 } else { c.0 - half.0 };
                        let y = if cy > 0.0 { t.h as f64 - c.1 - half.1 } else { c.1 - half.1 };
                        reach(x, cx).min(reach(y, cy)) / r
                    })
                    .fold(1.0, f64::min)
                    .max(0.3);
                // The nodes shrink with the ring.
                g.along *= g.rho;
                let r = r * g.rho;
                g.rects = (0..n)
                    .map(|j| {
                        let a = g.angle0 + dir * 2.0 * PI * j as f64 / n as f64;
                        Rect::around((c.0 + r * a.cos(), c.1 + r * a.sin()), 0.0, 0.0)
                    })
                    .collect();
            }
            Arrangement::Fixed | Arrangement::Tree | Arrangement::Pie | Arrangement::Radial => {}
        }
        g
    }

    fn font(&self) -> f64 {
        match self.kind {
            Arrangement::Line { .. } => self.along.min(self.across).min(1.0),
            _ => self.along.min(1.0),
        }
    }

    fn unit_shape(&self, t: &Template, i: usize, tu: usize, j: usize) -> Shape {
        let from = t.units[tu].rect;
        match self.kind {
            Arrangement::Line { .. } => {
                let mut s = placed(&t.shapes[i], from, self.rects[j], self.font());
                if let (Some(inset), crate::Geometry::Preset { adjust, .. }) = (self.taper, &mut s.geometry) {
                    // Trapezoid `adj` is the inset as a share of the shorter side.
                    let r = self.rects[j];
                    let value = inset / r.w.min(r.h).max(1.0) * 100_000.0;
                    match adjust.iter_mut().find(|(k, _)| k == "adj") {
                        Some(a) => a.1 = value,
                        None => adjust.push(("adj".into(), value)),
                    }
                }
                s
            }
            _ => {
                let k = self.along;
                let to = Rect::around(self.rects[j].center(), from.w * k, from.h * k);
                placed(&t.shapes[i], from, to, self.font())
            }
        }
    }

    /// The arc along the ring from new node `j` to the next one (`shape` is any of the loaded arcs).
    fn arc_between(&self, t: &Template, shape: &Shape, j: usize) -> Shape {
        let margin = self.arcs.expect("arcs");
        let (c, r) = (self.center, self.rects.first().map_or(0.0, |p| ((p.center().0 - self.center.0).powi(2) + (p.center().1 - self.center.1).powi(2)).sqrt()));
        let node = |k: usize| {
            let tu = template_index(k % self.n, self.n, self.m);
            let u = t.units[tu].rect;
            let center = self.rects[k % self.n].center();
            Rect::around(center, u.w * self.along, u.h * self.along)
        };
        // Angles keep counting past the last node, so the last arc runs on to the first.
        let angle = |k: usize| self.angle0 + self.dir * 2.0 * PI * k as f64 / self.n as f64;
        let a = leave_angle(node(j), c, r, angle(j), 1.0) + margin * self.rho / r;
        let b = leave_angle(node(j + 1), c, r, angle(j + 1), -1.0) - margin * self.rho / r;
        let sweep = b - a;
        let mut s = shape.clone();
        // A square around the ring, with the path in its own coordinates.
        let side = 2.0 * r;
        s.xfrm = Xfrm { x: (c.0 - r).round() as Emu, y: (c.1 - r).round() as Emu, w: side.round() as Emu, h: side.round() as Emu, ..shape.xfrm };
        let cmds = if sweep > 0.0 { arc_path((r, r), r, a, sweep) } else { Vec::new() };
        s.geometry = crate::Geometry::Custom(vec![crate::CustomPath { w: side.round(), h: side.round(), cmds, fill: false, stroke: true }]);
        s
    }

    /// Transition shape `i` (after loaded node `tt`) for the gap after new node `j`.
    fn trans_shape(&self, t: &Template, i: usize, tt: usize, j: usize) -> Shape {
        let shape = &t.shapes[i];
        let r = Rect::of(&shape.xfrm);
        match self.kind {
            Arrangement::Line { vertical } => {
                let (a, b) = (t.units[tt].rect.axis(vertical), t.units[(tt + 1).min(self.m - 1)].rect.axis(vertical));
                let gap = (a.0 + a.1 + b.0) / 2.0;
                let (ra, ral, rc, rcl) = r.axis(vertical);
                let offset = ra + ral / 2.0 - gap;
                let (na, nb) = (self.rects[j].axis(vertical), self.rects[(j + 1).min(self.n - 1)].axis(vertical));
                let new_gap = (na.0 + na.1 + nb.0) / 2.0;
                let k = self.along.min(self.across);
                let center_a = new_gap + offset * self.along;
                let center_c = self.cross0.1 + (rc + rcl / 2.0 - self.cross0.0) * self.across;
                let to = Rect::from_axis(vertical, center_a - ral * k / 2.0, ral * k, center_c - rcl * k / 2.0, rcl * k);
                placed(shape, r, to, self.font())
            }
            _ if self.arcs.is_some() => self.arc_between(t, shape, j),
            _ if self.ring.is_some_and(|r| r.0 == i) => {
                // The arrow around the ring starts and ends next to the first node, as far from it as a share of the spacing.
                let (_, tail, tip) = self.ring.expect("checked");
                let step = 360.0 / self.n as f64;
                let first = self.angle0.to_degrees();
                let mut s = shape.clone();
                let c = r.center();
                let center = (self.center.0 + (c.0 - self.center.0) * self.rho, self.center.1 + (c.1 - self.center.1) * self.rho);
                s.xfrm = Xfrm { x: (center.0 - r.w * self.rho / 2.0).round() as Emu, y: (center.1 - r.h * self.rho / 2.0).round() as Emu, w: (r.w * self.rho).round() as Emu, h: (r.h * self.rho).round() as Emu, ..shape.xfrm };
                if let crate::Geometry::Preset { adjust, .. } = &mut s.geometry {
                    for (name, share) in [("adj3", tip), ("adj4", tail)] {
                        let value = (first + share * step).rem_euclid(360.0) * 60_000.0;
                        match adjust.iter_mut().find(|(k, _)| k == name) {
                            Some(a) => a.1 = value,
                            None => adjust.push((name.into(), value)),
                        }
                    }
                }
                s
            }
            _ => {
                // Turn it from its gap (after loaded node tt) to the new gap.
                let old = self.angle0 + self.dir * 2.0 * PI * (tt as f64 + 0.5) / self.m as f64;
                let new = self.angle0 + self.dir * 2.0 * PI * (j as f64 + 0.5) / self.n as f64;
                turned(shape, self.center, new - old, self.along, self.rho)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, text: &str, children: Vec<Node>) -> Node {
        Node { id: id.into(), text: vec![text.into()], children }
    }

    fn text_shape(x: Emu, y: Emu, w: Emu, h: Emu, text: &str) -> Shape {
        let mut s = Shape::new(crate::ShapeId(0), "", Xfrm::new(x, y, w, h));
        let mut body = TextBody::default();
        body.set_plain_text(text);
        s.text = Some(body);
        s
    }

    fn pres(id: &str, name: &str, assoc: &str, sources: &[&str]) -> PresPoint {
        PresPoint { id: id.into(), name: name.into(), assoc: assoc.into(), sources: sources.iter().map(|s| s.to_string()).collect() }
    }

    /// Three boxes in a row with arrows between them (like `process1`).
    fn process() -> Arc<Template> {
        let nodes = vec![node("a", "A", vec![]), node("b", "B", vec![]), node("c", "C", vec![])];
        let mut links = HashMap::new();
        for id in ["a", "b", "c"] {
            links.insert(id.to_string(), Links { cxn: format!("{id}-cxn"), par_trans: format!("{id}-par"), sib_trans: format!("{id}-sib") });
        }
        let pres = vec![pres("pa", "node", "a", &["a"]), pres("ta", "sibTrans", "a-sib", &[]), pres("pb", "node", "b", &["b"]), pres("tb", "sibTrans", "b-sib", &[]), pres("pc", "node", "c", &["c"])];
        let shapes = vec![text_shape(0, 0, 200, 100, "A"), text_shape(220, 40, 60, 20, ""), text_shape(300, 0, 200, 100, "B"), text_shape(520, 40, 60, 20, ""), text_shape(600, 0, 200, 100, "C")];
        Arc::new(Template::new("process1".into(), nodes, pres, links, shapes, vec![0, 1, 2, 3, 4], 800, 100, "".into(), "".into()))
    }

    #[test]
    fn detects_a_line() {
        let t = process();
        assert_eq!(t.arrangement, Arrangement::Line { vertical: false });
        assert!(!t.nests);
    }

    #[test]
    fn retext_keeps_geometry() {
        let mut d = Diagram::new(process());
        assert!(d.set_text(1, "Beta"));
        let shapes = d.layout();
        assert_eq!(shapes.len(), 5);
        assert_eq!(shapes[2].text.as_ref().unwrap().plain_text(), "Beta");
        assert_eq!(shapes[2].xfrm, d.template.0.shapes[2].xfrm);
        assert_eq!(d.drawn[2].model_id, "pb");
    }

    #[test]
    fn adding_a_node_repeats_the_middle_one() {
        let mut d = Diagram::new(process());
        let row = d.insert(1, Insert::After).unwrap();
        assert_eq!(row, 2);
        d.set_text(2, "New");
        let shapes = d.layout();
        // 4 boxes and 3 arrows, all inside the frame, in order.
        assert_eq!(shapes.len(), 7);
        let boxes: Vec<&Shape> = shapes.iter().filter(|s| s.xfrm.w > 100).collect();
        assert_eq!(boxes.len(), 4);
        assert!(boxes.windows(2).all(|w| w[0].xfrm.x + w[0].xfrm.w <= w[1].xfrm.x));
        assert!(boxes[0].xfrm.x >= 0 && boxes[3].xfrm.x + boxes[3].xfrm.w <= 801);
        assert_eq!(boxes[2].text.as_ref().unwrap().plain_text(), "New");
        // Kept nodes keep their points; the new one gets derived ids.
        assert_eq!(d.drawn[0].model_id, "pa");
        let new_id = d.nodes[2].id.clone();
        assert_eq!(d.drawn[4].model_id, derived_id(&new_id, "pb"));
        // Smaller boxes, smaller text.
        assert!(boxes[0].text.as_ref().unwrap().paragraphs[0].runs[0].style.size_pt < 18.0);
    }

    #[test]
    fn deleting_and_reordering() {
        let mut d = Diagram::new(process());
        assert_eq!(d.delete(0), Some(0));
        assert_eq!(d.rows().iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["B", "C"]);
        assert_eq!(d.move_row(1, true), Some(0));
        assert_eq!(d.rows().iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["C", "B"]);
        let shapes = d.layout();
        assert_eq!(shapes.len(), 3);
        // The last node can't go.
        d.delete(0);
        assert_eq!(d.delete(0), None);
    }

    #[test]
    fn outline_shifts_follow_powerpoint() {
        let mut d = Diagram::new(process());
        // Without room for children, levels are fixed.
        assert!(!d.shift(1, true));
        let mut t = Arc::try_unwrap(process()).unwrap();
        t.nests = true;
        let mut d = Diagram::new(Arc::new(t));
        assert!(d.shift(1, true));
        assert_eq!(d.rows().iter().map(|r| r.depth).collect::<Vec<_>>(), [0, 1, 0]);
        // Promoting the child back: later siblings would become its children.
        assert_eq!(d.insert(1, Insert::After), Some(2));
        assert!(d.shift(1, false));
        assert_eq!(d.rows().iter().map(|r| r.depth).collect::<Vec<_>>(), [0, 0, 1, 0]);
        // A first child can't be demoted further (no earlier sibling).
        assert!(!d.shift(2, true));
    }

    #[test]
    fn children_shown_as_paragraphs() {
        // hList1-like: a header per node and a box listing its children.
        let nodes = vec![node("a", "A", vec![node("a1", "a1", vec![]), node("a2", "a2", vec![])]), node("b", "B", vec![node("b1", "b1", vec![])])];
        let links = HashMap::new();
        let pres = vec![pres("ha", "parTx", "a", &["a"]), pres("da", "desTx", "a", &["a1", "a2"]), pres("hb", "parTx", "b", &["b"]), pres("db", "desTx", "b", &["b1"])];
        let mut list = text_shape(0, 100, 200, 300, "a1\na2");
        list.text.as_mut().unwrap().paragraphs[1].level = 0;
        let shapes = vec![text_shape(0, 0, 200, 100, "A"), list, text_shape(300, 0, 200, 100, "B"), text_shape(300, 100, 200, 300, "b1")];
        let t = Arc::new(Template::new("hList1".into(), nodes, pres, links, shapes, vec![0, 1, 2, 3], 500, 400, "".into(), "".into()));
        assert!(t.nests);
        let mut d = Diagram::new(t);
        // Add a grandchild under a2.
        let r = d.insert(2, Insert::Child).unwrap();
        d.set_text(r, "deep");
        let shapes = d.layout();
        let body = shapes[1].text.as_ref().unwrap();
        assert_eq!(body.plain_text(), "a1\na2\ndeep");
        assert_eq!(body.paragraphs[2].level, 1);
    }

    #[test]
    fn cycle_respaces_around_the_center() {
        let nodes: Vec<Node> = (0..4).map(|i| node(&format!("n{i}"), "x", vec![])).collect();
        let mut links = HashMap::new();
        let mut pres_pts = Vec::new();
        let mut shapes = Vec::new();
        let mut shape_pres = Vec::new();
        for i in 0..4 {
            links.insert(format!("n{i}"), Links { cxn: format!("c{i}"), par_trans: format!("p{i}"), sib_trans: format!("s{i}") });
            let a = -PI / 2.0 + i as f64 * PI / 2.0;
            let (x, y) = (500.0 + 300.0 * a.cos(), 500.0 + 300.0 * a.sin());
            pres_pts.push(pres(&format!("P{i}"), "node", &format!("n{i}"), &[&format!("n{i}")]));
            shape_pres.push(pres_pts.len() - 1);
            shapes.push(text_shape(x as Emu - 100, y as Emu - 100, 200, 200, "x"));
            let b = a + PI / 4.0;
            pres_pts.push(pres(&format!("T{i}"), "sibTrans", &format!("s{i}"), &[]));
            shape_pres.push(pres_pts.len() - 1);
            shapes.push(text_shape((500.0 + 300.0 * b.cos()) as Emu - 20, (500.0 + 300.0 * b.sin()) as Emu - 10, 40, 20, ""));
        }
        let t = Arc::new(Template::new("cycle2".into(), nodes, pres_pts, links, shapes, shape_pres, 1000, 1000, "".into(), "".into()));
        assert_eq!(t.arrangement, Arrangement::Cycle);
        assert!(t.wraps());
        let mut d = Diagram::new(t);
        d.insert(3, Insert::After);
        let shapes = d.layout();
        // 5 nodes and 5 arrows, nodes still 300 from the center.
        assert_eq!(shapes.len(), 10);
        for s in shapes.iter().step_by(2) {
            let (cx, cy) = Rect::of(&s.xfrm).center();
            assert!((((cx - 500.0).powi(2) + (cy - 500.0).powi(2)).sqrt() - 300.0).abs() < 2.0);
        }
    }

    #[test]
    fn derived_ids_look_like_guids() {
        let id = derived_id("a", "b");
        assert_eq!(id.len(), 38);
        assert!(id.starts_with('{') && id.ends_with('}'));
        assert_ne!(id, derived_id("b", "a"));
        assert_eq!(id, derived_id("a", "b"));
    }

    /// A root with two departments in a row; the first has two teams hanging below it.
    fn org_chart() -> Arc<Template> {
        let nodes = vec![node("r", "R", vec![node("a", "A", vec![node("a1", "A1", vec![]), node("a2", "A2", vec![])]), node("b", "B", vec![])])];
        let ids = ["r", "a", "a1", "a2", "b"];
        let mut links = HashMap::new();
        for id in ids {
            links.insert(id.to_string(), Links { cxn: format!("{id}-cxn"), par_trans: format!("{id}-par"), sib_trans: format!("{id}-sib") });
        }
        let mut pres_pts = Vec::new();
        let mut shapes = Vec::new();
        let mut shape_pres = Vec::new();
        let conn = |x, y, w, h| {
            let mut s = Shape::new(crate::ShapeId(0), "", Xfrm::new(x, y, w, h));
            s.geometry = crate::Geometry::Custom(vec![crate::CustomPath { w: 0.0, h: 0.0, cmds: vec![crate::PathCmd::MoveTo(0.0, 0.0), crate::PathCmd::LineTo(0.0, h as f64 / 2.0), crate::PathCmd::LineTo(w as f64, h as f64 / 2.0)], fill: true, stroke: true }]);
            s
        };
        // Boxes, in the order of `ids`; the connector to each non-root node follows its box.
        let boxes = [(150, 0, 100, 40), (0, 80, 100, 40), (20, 140, 80, 30), (20, 180, 80, 30), (250, 80, 100, 40)];
        for (k, id) in ids.iter().enumerate() {
            pres_pts.push(pres(&format!("p{id}"), "rootText", id, &[id]));
            shape_pres.push(pres_pts.len() - 1);
            let (x, y, w, h) = boxes[k];
            shapes.push(text_shape(x, y, w, h, &id.to_uppercase()));
            if k > 0 {
                pres_pts.push(pres(&format!("c{id}"), "Name37", &format!("{id}-par"), &[]));
                shape_pres.push(pres_pts.len() - 1);
                // Teams hang from a point 10 in from A's left edge.
                shapes.push(if k == 2 || k == 3 { conn(10, 80, x - 10, y - 80 + h / 2) } else { conn(x, y - 30, 40, 30) });
            }
        }
        Arc::new(Template::new("urn:x/layout/orgChart1".into(), nodes, pres_pts, links, shapes, shape_pres, 400, 260, "".into(), "".into()))
    }

    #[test]
    fn org_chart_is_a_tree() {
        let t = org_chart();
        assert_eq!(t.arrangement, Arrangement::Tree);
        let d = Diagram::new(t);
        assert!(d.can_restructure() && d.can_nest());
    }

    #[test]
    fn org_chart_relays_out_after_adding() {
        let mut d = Diagram::new(org_chart());
        // Beside the root goes under it.
        let row = d.insert(0, Insert::After).unwrap();
        assert_eq!(row, 5);
        assert_eq!(d.rows()[row].depth, 1);
        d.set_text(row, "New");
        let shapes = d.layout();
        let frame = |text: &str| shapes.iter().find(|s| s.text.as_ref().is_some_and(|t| t.plain_text() == text)).unwrap().xfrm;
        let (a, b, new, root) = (frame("A"), frame("B"), frame("New"), frame("R"));
        // Three departments in a row below the root, which sits over the middle one.
        assert!(a.y == b.y && b.y == new.y && a.x < b.x && b.x < new.x && root.y + root.h < a.y);
        let mid = |x: crate::Xfrm| x.x + x.w / 2;
        assert!((mid(root) - mid(b)).abs() <= 1);
        // The teams still hang below A, indented, one under the other.
        let (a1, a2) = (frame("A1"), frame("A2"));
        assert!(a1.x > a.x && a1.x == a2.x && a1.y > a.y + a.h && a2.y > a1.y + a1.h);
        // A box and a connector per node, boxes and connectors named by who they belong to.
        assert_eq!(shapes.len(), 6 + 5);
        assert_eq!(d.drawn.iter().filter(|x| x.node.is_some()).count(), 6);
        // Everything fits the frame.
        assert!(shapes.iter().all(|s| s.xfrm.x >= 0 && s.xfrm.y >= 0 && s.xfrm.x + s.xfrm.w <= 400 && s.xfrm.y + s.xfrm.h <= 260));
        // A connector runs from its parent's bottom to the child's side or top.
        let hang = shapes.iter().find(|s| s.xfrm.y == a.y + a.h && s.xfrm.x < a1.x && s.xfrm.h > 0 && s.text.is_none() && (s.xfrm.x + s.xfrm.w - a1.x).abs() <= 1);
        assert!(hang.is_some(), "a hanging connector ends at A1's left edge");
    }

    #[test]
    fn org_chart_demoting_follows_the_level_above() {
        let mut d = Diagram::new(org_chart());
        // B becomes a team of A: A's teams hang, and B is the third.
        assert!(d.shift(4, true));
        let shapes = d.layout();
        let frame = |text: &str| shapes.iter().find(|s| s.text.as_ref().is_some_and(|t| t.plain_text() == text)).unwrap().xfrm;
        let (a2, b) = (frame("A2"), frame("B"));
        assert!(b.x == a2.x && b.y > a2.y);
        // The first level cannot be left: the root stays alone.
        assert!(!d.shift(1, false));
        assert!(d.delete(0).is_none());
    }


    /// Three stacked trapezoids, each one step wider than the one above (like `pyramid1`).
    fn pyramid() -> Arc<Template> {
        let nodes = vec![node("a", "A", vec![]), node("b", "B", vec![]), node("c", "C", vec![])];
        let mut links = HashMap::new();
        let mut pres_pts = Vec::new();
        let mut shapes = Vec::new();
        for (k, id) in ["a", "b", "c"].iter().enumerate() {
            links.insert(id.to_string(), Links { cxn: format!("{id}-cxn"), par_trans: format!("{id}-par"), sib_trans: format!("{id}-sib") });
            pres_pts.push(pres(&format!("p{id}"), "level", id, &[id]));
            let (w, h) = (100 * (k as i64 + 1), 90);
            let mut s = text_shape(150 - w / 2, 90 * k as i64, w, h, &id.to_uppercase());
            s.geometry = crate::Geometry::Preset { name: "trapezoid".into(), adjust: vec![("adj".into(), 50_000.0 * 100.0 / h as f64)] };
            shapes.push(s);
        }
        Arc::new(Template::new("urn:x/layout/pyramid1".into(), nodes, pres_pts, links, shapes, vec![0, 1, 2], 300, 270, "".into(), "".into()))
    }

    #[test]
    fn pyramid_levels_share_the_height() {
        let t = pyramid();
        assert!(t.tapered);
        let mut d = Diagram::new(t);
        let r = d.insert(2, Insert::After).unwrap();
        d.set_text(r, "D");
        let shapes = d.layout();
        assert_eq!(shapes.len(), 4);
        for (j, s) in shapes.iter().enumerate() {
            let near = |a: i64, b: f64| (a as f64 - b).abs() <= 1.0;
            assert!(near(s.xfrm.h, 67.5) && near(s.xfrm.y, 67.5 * j as f64) && near(s.xfrm.w, 75.0 * (j as f64 + 1.0)));
            // Each level slants half a step per side.
            let crate::Geometry::Preset { adjust, .. } = &s.geometry else { panic!() };
            let inset = adjust[0].1 * s.xfrm.w.min(s.xfrm.h) as f64 / 100_000.0;
            assert!((inset - 37.5).abs() < 1.0, "inset {inset}");
        }
    }

}
