//! Applying a layout to a slide, and resetting a slide to its layout.

use ppt_model::{Placeholder, Shape, ShapeKind};
use ppt_text::TextEngine;

use crate::Editor;

/// Placeholder kinds that stand in for each other when a layout changes.
fn family(kind: &str) -> &str {
    match kind {
        "title" | "ctrTitle" => "title",
        "body" | "obj" | "subTitle" | "" => "body",
        k => k,
    }
}

type Rule = dyn Fn(&Placeholder, &Placeholder) -> bool;

/// Which of `slide_phs` each layout placeholder takes over: the same type and
/// index first, then the next one of a compatible type.
fn match_placeholders(layout_phs: &[&Placeholder], slide_phs: &[&Placeholder]) -> Vec<Option<usize>> {
    let mut taken = vec![false; slide_phs.len()];
    let mut out = vec![None; layout_phs.len()];
    let passes: [&Rule; 2] = [&|a, b| a.kind == b.kind && a.idx == b.idx, &|a, b| family(&a.kind) == family(&b.kind)];
    for pass in passes {
        for (i, l) in layout_phs.iter().enumerate() {
            if out[i].is_some() {
                continue;
            }
            if let Some(j) = slide_phs.iter().enumerate().position(|(j, s)| !taken[j] && pass(l, s)) {
                taken[j] = true;
                out[i] = Some(j);
            }
        }
    }
    out
}

fn is_empty_placeholder(s: &Shape) -> bool {
    s.placeholder.is_some() && !matches!(s.kind, ShapeKind::Picture { .. } | ShapeKind::Table(_) | ShapeKind::Chart(_)) && s.text.as_ref().is_none_or(|t| t.is_empty())
}

impl Editor {
    /// Applies layout `index` to the current slide: placeholders move to the
    /// layout's positions and take its formatting where they hadn't overridden
    /// it; empty placeholders the layout doesn't have go away, and the layout's
    /// others are added. With `reset`, all placeholder formatting and positions
    /// go back to the layout's.
    pub fn apply_layout(&mut self, engine: &mut TextEngine, index: usize, reset: bool) -> bool {
        let Some(new) = self.pres.layouts.get(index).cloned() else { return false };
        let Some(current) = self.pres.slides.get(self.slide).map(|s| s.layout) else { return false };
        if current == Some(index) && !reset {
            return false;
        }
        self.end_text_edit(engine);
        let before = self.snapshot();
        let old = current.and_then(|i| self.pres.layouts.get(i)).cloned();
        let mut fresh: Vec<Shape> = Vec::new();
        for s in &new.placeholders {
            fresh.push(Shape { id: self.pres.alloc_id(), ..s.clone() });
        }
        let background: Vec<Shape> = new.background_shapes.iter().map(|s| Shape { id: self.pres.alloc_id(), ..s.clone() }).collect();

        let slide = self.pres.slide_mut(self.slide).unwrap();
        let slide_phs: Vec<(usize, Placeholder)> = slide.shapes.iter().enumerate().filter_map(|(i, s)| s.placeholder.clone().map(|p| (i, p))).collect();
        let layout_refs: Vec<&Placeholder> = new.placeholders.iter().filter_map(|s| s.placeholder.as_ref()).collect();
        let slide_refs: Vec<&Placeholder> = slide_phs.iter().map(|(_, p)| p).collect();
        let matches = match_placeholders(&layout_refs, &slide_refs);
        // The old layout's placeholder for a slide placeholder: what it inherited before.
        let old_phs: Vec<&Placeholder> = old.iter().flat_map(|l| l.placeholders.iter().filter_map(|s| s.placeholder.as_ref())).collect();
        let old_match = match_placeholders(&slide_refs, &old_phs);

        let mut matched = vec![false; slide.shapes.len()];
        let mut added = Vec::new();
        let layout_shapes: Vec<&Shape> = new.placeholders.iter().filter(|s| s.placeholder.is_some()).collect();
        for (li, m) in matches.iter().enumerate() {
            let target = layout_shapes[li];
            match m {
                Some(si) => {
                    let (shape_index, _) = slide_phs[*si];
                    matched[shape_index] = true;
                    let old_ph = old_match[*si].and_then(|oi| old.as_ref().and_then(|l| l.placeholders.iter().filter(|s| s.placeholder.is_some()).nth(oi)));
                    let shape = &mut slide.shapes[shape_index];
                    shape.xfrm = target.xfrm;
                    shape.placeholder = target.placeholder.clone();
                    if let (Some(body), Some(new_body)) = (&mut shape.text, &target.text) {
                        let old_body = old_ph.and_then(|s| s.text.clone()).unwrap_or_else(|| body.clone());
                        body.rebase(&old_body, new_body);
                        if reset {
                            body.anchor = new_body.anchor;
                            body.insets = new_body.insets;
                            body.reset_formatting();
                        }
                    }
                }
                None => added.push(fresh[new.placeholders.iter().position(|s| std::ptr::eq(s, target)).unwrap_or(0)].clone()),
            }
        }
        let mut i = 0;
        slide.shapes.retain(|s| {
            let keep = matched[i] || !is_empty_placeholder(s);
            i += 1;
            keep
        });
        slide.shapes.extend(added);
        slide.background_shapes = background;
        slide.layout = Some(index);
        self.selection.clear();
        self.push_undo(before);
        self.refresh_tables(engine);
        true
    }

    /// Resets the current slide's placeholders to its layout.
    pub fn reset_slide(&mut self, engine: &mut TextEngine) -> bool {
        let Some(index) = self.pres.slides.get(self.slide).and_then(|s| s.layout) else { return false };
        self.apply_layout(engine, index, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_match_by_type_then_family() {
        let ph = |k: &str, i: u32| Placeholder { kind: k.into(), idx: i };
        let layout = [ph("title", 0), ph("body", 1), ph("body", 2)];
        let slide = [ph("ctrTitle", 0), ph("subTitle", 1)];
        let m = match_placeholders(&layout.iter().collect::<Vec<_>>(), &slide.iter().collect::<Vec<_>>());
        assert_eq!(m, vec![Some(0), Some(1), None]);
        let slide = [ph("body", 2), ph("body", 1)];
        let m = match_placeholders(&layout.iter().collect::<Vec<_>>(), &slide.iter().collect::<Vec<_>>());
        assert_eq!(m, vec![None, Some(1), Some(0)]);
    }
}
