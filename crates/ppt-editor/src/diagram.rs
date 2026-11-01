//! SmartArt editing through the text pane: node text, adding, removing,
//! reordering and re-leveling nodes. The drawing is rebuilt after each change.

use std::collections::HashMap;
use std::sync::Arc;

use ppt_geometry::{TextFrame, text_width_pt};
use ppt_model::diagram::{Diagram, Insert, Row};
use ppt_model::{EMU_PER_PT, Shape, ShapeId, ShapeKind, TextBody, Xfrm};
use ppt_text::TextEngine;

use crate::{Editor, RichEdit, TextEdit};

/// The selected SmartArt as the text pane shows it.
pub struct DiagramInfo {
    pub rows: Vec<Row>,
    /// Nodes can be added, removed and reordered.
    pub restructure: bool,
    /// Nodes can have children (shown inside their parent's shapes).
    pub nest: bool,
    /// One top-level node only (an org chart): it cannot be added beside or promoted to.
    pub single_root: bool,
}

/// `body` with every font size multiplied by `k`.
fn scaled(body: &TextBody, k: f64) -> TextBody {
    let mut b = body.clone();
    for p in &mut b.paragraphs {
        for r in &mut p.runs {
            r.style.size_pt = (r.style.size_pt as f64 * k) as f32;
        }
        p.end_style.size_pt = (p.end_style.size_pt as f64 * k) as f32;
    }
    b
}

/// The largest font multiplier (at most 1) at which `shape`'s text fits its height.
fn fit_scale(engine: &mut TextEngine, shape: &Shape) -> f64 {
    let Some(body) = &shape.text else { return 1.0 };
    let frame = TextFrame::for_text(shape.xfrm, &shape.geometry, Some(body));
    let room = (shape.xfrm.h - frame.inset[1] - frame.inset[3] - body.insets[1] - body.insets[3]) as f64 / EMU_PER_PT;
    let fits = |k: f64, engine: &mut TextEngine| {
        let b = scaled(body, k);
        engine.layout_body(&b, text_width_pt(&frame, &b)).height as f64 <= room + 0.5
    };
    if body.is_empty() || fits(1.0, engine) {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.25, 1.0);
    for _ in 0..7 {
        let mid = (lo + hi) / 2.0;
        if fits(mid, engine) { lo = mid } else { hi = mid }
    }
    lo
}

/// Shrinks text that no longer fits, as PowerPoint does: shapes of the same
/// role (layout element) share one size.
fn fit_text(engine: &mut TextEngine, d: &Diagram, shapes: &mut [Shape]) {
    let t = &d.template.0;
    let role = |k: usize| t.shape_pres.get(d.drawn[k].template).map(|&p| t.pres[p].name.as_str()).unwrap_or_default();
    let mut groups: HashMap<&str, f64> = HashMap::new();
    for (k, shape) in shapes.iter().enumerate() {
        let tpl = &t.shapes[d.drawn[k].template];
        if shape.text.is_none() || shape.text == tpl.text {
            continue;
        }
        // Relative to how the loaded text fitted (it may overflow on purpose).
        let need = (fit_scale(engine, shape) / fit_scale(engine, tpl)).min(1.0);
        let g = groups.entry(role(k)).or_insert(1.0);
        *g = g.min(need);
    }
    for (k, shape) in shapes.iter_mut().enumerate() {
        if let Some(&g) = groups.get(role(k)).filter(|g| **g < 0.999)
            && let Some(body) = &shape.text
        {
            shape.text = Some(scaled(body, g));
        }
    }
}

fn data(kind: &ShapeKind) -> Option<&Arc<Diagram>> {
    match kind {
        ShapeKind::Diagram { data: Some(d), .. } => Some(d),
        _ => None,
    }
}

impl Editor {
    /// The selected SmartArt with an editable node tree.
    fn selected_diagram(&self) -> Option<(ShapeId, &Diagram)> {
        let shape = self.selected_shape()?;
        Some((shape.id, data(&shape.kind)?))
    }

    pub fn diagram_info(&self) -> Option<DiagramInfo> {
        let (_, d) = self.selected_diagram()?;
        Some(DiagramInfo { rows: d.rows(), restructure: d.can_restructure(), nest: d.can_nest(), single_root: d.is_tree() })
    }

    /// Text of row `row` of SmartArt `id` on `slide`.
    pub(crate) fn node_text(&self, slide: usize, id: ShapeId, row: usize) -> Option<String> {
        let d = data(&self.pres.shape(slide, id)?.kind)?;
        d.rows().into_iter().nth(row).map(|r| r.text)
    }

    /// Sets rows' texts of SmartArt `id` on `slide` and rebuilds its drawing (no undo step).
    pub(crate) fn set_node_texts(&mut self, engine: &mut TextEngine, slide: usize, id: ShapeId, texts: &[(usize, String)]) -> bool {
        let Some(d) = self.pres.shape(slide, id).and_then(|s| data(&s.kind)) else { return false };
        let mut d = (**d).clone();
        let mut changed = false;
        for (row, text) in texts {
            changed |= d.set_text(*row, text);
        }
        if changed {
            self.store_diagram(engine, slide, id, d);
        }
        changed
    }

    /// Puts `d` (with its drawing rebuilt) in place of SmartArt `id`'s content.
    fn store_diagram(&mut self, engine: &mut TextEngine, slide: usize, id: ShapeId, mut d: Diagram) {
        let mut shapes = d.layout();
        fit_text(engine, &d, &mut shapes);
        if let Some(shape) = self.pres.shape_mut(slide, id)
            && let ShapeKind::Diagram { shapes: drawn, data, .. } = &mut shape.kind
        {
            *drawn = Arc::new(shapes);
            *data = Some(Arc::new(d));
        }
    }

    /// Applies `f` to a copy of the selected SmartArt; when it reports a change,
    /// that copy (with its drawing rebuilt) replaces the original as one undo step.
    fn edit_diagram<R>(&mut self, engine: &mut TextEngine, merge_typing: Option<usize>, f: impl FnOnce(&mut Diagram) -> Option<R>) -> Option<R> {
        let (id, _) = self.selected_diagram()?;
        self.edit_diagram_of(engine, id, merge_typing, f)
    }

    fn edit_diagram_of<R>(&mut self, engine: &mut TextEngine, id: ShapeId, merge_typing: Option<usize>, f: impl FnOnce(&mut Diagram) -> Option<R>) -> Option<R> {
        let mut d = data(&self.pres.shape(self.slide, id)?.kind)?.as_ref().clone();
        let result = f(&mut d)?;
        let key = merge_typing.map(|row| (id, row));
        if key.is_some() && self.diagram_undo == key {
            self.changed();
        } else {
            let snap = self.snapshot();
            self.push_undo(snap);
        }
        self.diagram_undo = key;
        self.node_match = None;
        self.store_diagram(engine, self.slide, id, d);
        Some(result)
    }

    /// Replaces row `row`'s text; typing into one row is one undo step.
    pub fn diagram_set_text(&mut self, engine: &mut TextEngine, row: usize, text: &str) -> bool {
        self.edit_diagram(engine, Some(row), |d| d.set_text(row, text).then_some(())).is_some()
    }

    /// Replaces row `row`'s text of SmartArt `id` (one undo step).
    pub(crate) fn diagram_set_text_of(&mut self, engine: &mut TextEngine, id: ShapeId, row: usize, text: &str) -> bool {
        self.edit_diagram_of(engine, id, None, |d| d.set_text(row, text).then_some(())).is_some()
    }

    /// Adds a new SmartArt of layout `kind` (see `ppt_ooxml::smartart_kinds`) to
    /// the current slide, centered, and selects it.
    pub fn insert_smartart(&mut self, engine: &mut TextEngine, kind: &str) -> bool {
        self.end_text_edit(engine);
        if self.pres.slides.is_empty() {
            return false;
        }
        let l = ppt_model::lang();
        let labels = ppt_ooxml::Labels { root: l.pick("標題", "标题", "Title"), top: l.pick("項目 {}", "项目 {}", "Item {}"), child: l.pick("子項目 {}", "子项目 {}", "Sub-item {}") };
        let Ok((mut shape, package)) = ppt_ooxml::new_smartart(&self.pres, kind, (self.pres.slide_width, self.pres.slide_height), &labels) else { return false };
        let before = self.snapshot();
        let id = self.pres.alloc_id();
        shape.id = id;
        let data = data(&shape.kind).map(|d| d.as_ref().clone());
        self.pres.package = Some(package);
        self.pres.slides[self.slide].shapes.push(shape);
        // The new texts may not fit their shapes as the starter's did.
        if let Some(d) = data {
            self.store_diagram(engine, self.slide, id, d);
        }
        self.selection = vec![id];
        self.push_undo(before);
        true
    }

    /// Starts editing, on the canvas, the text of the node drawn at slide point
    /// (`x`, `y`) in SmartArt `id`. False when that text cannot be edited in place
    /// (it shows more than the node's own text, or the frame is turned or stretched).
    pub(crate) fn begin_diagram_text_edit(&mut self, engine: &mut TextEngine, id: ShapeId, x: f64, y: f64) -> bool {
        let Some(shape) = self.pres.shape(self.slide, id) else { return false };
        let ShapeKind::Diagram { shapes, w, h, data: Some(d) } = &shape.kind else { return false };
        let (sx, sy) = (shape.xfrm.w as f64 / (*w).max(1) as f64, shape.xfrm.h as f64 / (*h).max(1) as f64);
        if shape.xfrm.rot != 0.0 || shape.xfrm.flip_h || shape.xfrm.flip_v || (sx - 1.0).abs() > 0.02 || (sy - 1.0).abs() > 0.02 {
            return false;
        }
        let Some(index) = self.diagram_shape_at(id, x, y) else { return false };
        let Some(row) = d.row_of_own_text(index) else { return false };
        let child = &shapes[index];
        let Some(body) = child.text.clone() else { return false };
        let at = |v: i64, s: f64, origin: i64| origin + (v as f64 * s).round() as i64;
        let xf = Xfrm { x: at(child.xfrm.x, sx, shape.xfrm.x), y: at(child.xfrm.y, sy, shape.xfrm.y), w: (child.xfrm.w as f64 * sx).round() as i64, h: (child.xfrm.h as f64 * sy).round() as i64, ..child.xfrm };
        let frame = TextFrame::for_text(xf, &child.geometry, Some(&body));
        let rich = RichEdit::new(engine, body.clone(), text_width_pt(&frame, &body));
        let original_xfrm = shape.xfrm;
        self.selection = vec![id];
        self.text = Some(TextEdit { shape: id, cell: None, frame, rich, original: body, original_xfrm, remove_if_empty: false, diagram: Some((index, row)) });
        true
    }

    /// Adds a node after, before or below row `row`. Returns the new row.
    pub fn diagram_insert(&mut self, engine: &mut TextEngine, row: usize, place: Insert) -> Option<usize> {
        self.edit_diagram(engine, None, |d| d.insert(row, place))
    }

    /// Removes row `row` with its children. Returns the row to go to.
    pub fn diagram_delete(&mut self, engine: &mut TextEngine, row: usize) -> Option<usize> {
        self.edit_diagram(engine, None, |d| d.delete(row))
    }

    /// Demotes (`deeper`) or promotes row `row`.
    pub fn diagram_shift(&mut self, engine: &mut TextEngine, row: usize, deeper: bool) -> bool {
        self.edit_diagram(engine, None, |d| d.shift(row, deeper).then_some(())).is_some()
    }

    /// Moves row `row` before the previous or after the next sibling. Returns where it went.
    pub fn diagram_move(&mut self, engine: &mut TextEngine, row: usize, up: bool) -> Option<usize> {
        self.edit_diagram(engine, None, |d| d.move_row(row, up))
    }

    /// The row of the node drawn at slide point (`x`, `y`) in SmartArt `id`.
    pub(crate) fn diagram_row_at(&self, id: ShapeId, x: f64, y: f64) -> Option<usize> {
        let index = self.diagram_shape_at(id, x, y)?;
        data(&self.pres.shape(self.slide, id)?.kind)?.row_of_shape(index)
    }

    /// The (topmost) drawn shape with text at slide point (`x`, `y`) in SmartArt `id`.
    fn diagram_shape_at(&self, id: ShapeId, x: f64, y: f64) -> Option<usize> {
        let shape = self.pres.shape(self.slide, id)?;
        let ShapeKind::Diagram { shapes, w, h, data: Some(_) } = &shape.kind else { return None };
        let (lx, ly) = shape.xfrm.to_local(x, y);
        // Into the drawing's own frame.
        let (dx, dy) = (lx * *w as f64 / shape.xfrm.w.max(1) as f64, ly * *h as f64 / shape.xfrm.h.max(1) as f64);
        // Topmost first.
        shapes.iter().enumerate().rev().filter(|(_, s)| s.text.is_some()).find_map(|(i, s)| {
            let (sx, sy) = s.xfrm.to_local(dx, dy);
            (sx >= 0.0 && sy >= 0.0 && sx <= s.xfrm.w as f64 && sy <= s.xfrm.h as f64).then_some(i)
        })
    }
}
