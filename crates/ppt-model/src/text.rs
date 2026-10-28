//! Rich-text editing on [`TextBody`].
//!
//! Positions are byte offsets into the flattened text, where paragraphs are
//! joined by `\n` (the same string [`TextBody::plain_text`] returns and the
//! layout engine sees).

use std::ops::Range;

use crate::{Paragraph, Run, RunStyle, TextBody};

/// Flattened form used by the edit operations.
struct Flat {
    text: String,
    /// Non-overlapping spans covering `text`, including the `\n` separators.
    spans: Vec<(Range<usize>, RunStyle)>,
    /// Paragraph properties (runs left empty).
    paras: Vec<Paragraph>,
}

impl Flat {
    fn split_at(&mut self, pos: usize) {
        if let Some(i) = self.spans.iter().position(|(r, _)| r.start < pos && pos < r.end) {
            let (r, style) = self.spans[i].clone();
            self.spans[i].0 = r.start..pos;
            self.spans.insert(i + 1, (pos..r.end, style));
        }
    }

    fn normalize(&mut self) {
        self.spans.retain(|(r, _)| !r.is_empty());
        self.spans.sort_by_key(|(r, _)| r.start);
        let mut merged: Vec<(Range<usize>, RunStyle)> = Vec::with_capacity(self.spans.len());
        for (r, s) in self.spans.drain(..) {
            match merged.last_mut() {
                Some((last, ls)) if last.end == r.start && *ls == s => last.end = r.end,
                _ => merged.push((r, s)),
            }
        }
        self.spans = merged;
    }

    fn paragraph_at(&self, pos: usize) -> usize {
        self.text[..pos.min(self.text.len())].matches('\n').count()
    }
}

impl TextBody {
    fn flatten(&self) -> Flat {
        let mut text = String::new();
        let mut spans = Vec::new();
        let mut paras = Vec::with_capacity(self.paragraphs.len());
        for (i, p) in self.paragraphs.iter().enumerate() {
            if i > 0 {
                let start = text.len();
                text.push('\n');
                spans.push((start..text.len(), self.paragraphs[i - 1].end_style.clone()));
            }
            for run in &p.runs {
                let start = text.len();
                text.push_str(&run.text);
                spans.push((start..text.len(), run.style.clone()));
            }
            paras.push(Paragraph { runs: Vec::new(), ..p.clone() });
        }
        let mut flat = Flat { text, spans, paras };
        flat.normalize();
        flat
    }

    fn unflatten(&mut self, flat: Flat) {
        let mut paragraphs = Vec::with_capacity(flat.paras.len());
        let mut start = 0;
        let segments: Vec<Range<usize>> = flat
            .text
            .split('\n')
            .map(|seg| {
                let r = start..start + seg.len();
                start = r.end + 1;
                r
            })
            .collect();
        debug_assert_eq!(segments.len(), flat.paras.len());
        for (seg, mut para) in segments.into_iter().zip(flat.paras) {
            let mut runs: Vec<Run> = Vec::new();
            for (r, style) in &flat.spans {
                let (s, e) = (r.start.max(seg.start), r.end.min(seg.end));
                if s < e {
                    match runs.last_mut() {
                        Some(last) if last.style == *style => last.text.push_str(&flat.text[s..e]),
                        _ => runs.push(Run { text: flat.text[s..e].to_string(), style: style.clone() }),
                    }
                }
            }
            para.runs = runs;
            paragraphs.push(para);
        }
        self.paragraphs = paragraphs;
    }

    /// Length of the flattened text in bytes.
    pub fn len(&self) -> usize {
        let text: usize = self.paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.text.len()).sum();
        text + self.paragraphs.len().saturating_sub(1)
    }

    /// Byte range of each paragraph, excluding its `\n` separator.
    pub fn paragraph_ranges(&self) -> Vec<Range<usize>> {
        let mut start = 0;
        self.paragraphs
            .iter()
            .map(|p| {
                let len: usize = p.runs.iter().map(|r| r.text.len()).sum();
                let r = start..start + len;
                start = r.end + 1;
                r
            })
            .collect()
    }

    pub fn paragraph_at(&self, pos: usize) -> usize {
        self.paragraph_ranges().iter().position(|r| pos <= r.end).unwrap_or(self.paragraphs.len().saturating_sub(1))
    }

    /// The style new text typed at `pos` should get: the character before it
    /// in the same paragraph, else the paragraph's first run, else its end style.
    pub fn style_at(&self, pos: usize) -> RunStyle {
        let flat = self.flatten();
        let p = flat.paragraph_at(pos);
        let seg_start = flat.text[..pos.min(flat.text.len())].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let probe = if pos > seg_start { Some(pos - 1) } else { None };
        let para_end = flat.text[pos.min(flat.text.len())..].find('\n').map(|i| pos + i).unwrap_or(flat.text.len());
        let probe = probe.or((pos < para_end).then_some(pos));
        probe
            .and_then(|i| flat.spans.iter().find(|(r, _)| r.contains(&i)).map(|(_, s)| s.clone()))
            .unwrap_or_else(|| flat.paras[p].end_style.clone())
    }

    /// Styles of all characters in `range` (deduplicated, in order).
    pub fn styles_in(&self, range: Range<usize>) -> Vec<RunStyle> {
        let flat = self.flatten();
        let mut out: Vec<RunStyle> = Vec::new();
        for (r, s) in &flat.spans {
            if r.start < range.end && range.start < r.end && flat.text[r.start.max(range.start)..r.end.min(range.end)] != *"\n" && !out.contains(s) {
                out.push(s.clone());
            }
        }
        out
    }

    /// Inserts `text` at `pos` with `style`. `\n` starts new paragraphs that
    /// inherit the properties of the paragraph being split.
    pub fn insert(&mut self, pos: usize, text: &str, style: &RunStyle) {
        if text.is_empty() {
            return;
        }
        let mut flat = self.flatten();
        let pos = pos.min(flat.text.len());
        let p = flat.paragraph_at(pos);
        flat.split_at(pos);
        let len = text.len();
        for (r, _) in flat.spans.iter_mut() {
            if r.start >= pos {
                *r = r.start + len..r.end + len;
            }
        }
        flat.text.insert_str(pos, text);
        flat.spans.push((pos..pos + len, style.clone()));
        let new_paras = text.matches('\n').count();
        for _ in 0..new_paras {
            let mut para = flat.paras[p].clone();
            para.end_style = style.clone();
            flat.paras.insert(p + 1, para);
        }
        flat.normalize();
        self.unflatten(flat);
    }

    /// Deletes `range`. Paragraphs joined by removing a `\n` keep the
    /// properties of the first paragraph.
    pub fn delete(&mut self, range: Range<usize>) {
        let mut flat = self.flatten();
        let range = range.start.min(flat.text.len())..range.end.min(flat.text.len());
        if range.is_empty() {
            return;
        }
        let p = flat.paragraph_at(range.start);
        let removed = flat.text[range.clone()].matches('\n').count();
        // A joined paragraph that ends up empty keeps the last removed paragraph's end style.
        if removed > 0 {
            let last_end = flat.paras[p + removed].end_style.clone();
            flat.paras[p].end_style = last_end;
        }
        flat.paras.drain(p + 1..=p + removed);
        let len = range.len();
        flat.split_at(range.start);
        flat.split_at(range.end);
        flat.spans.retain(|(r, _)| !(r.start >= range.start && r.end <= range.end));
        for (r, _) in flat.spans.iter_mut() {
            if r.start >= range.end {
                *r = r.start - len..r.end - len;
            }
        }
        flat.text.replace_range(range, "");
        flat.normalize();
        self.unflatten(flat);
    }

    /// Applies `f` to the character styles in `range`, and to the end style of
    /// every paragraph whose end lies inside it.
    pub fn apply_style(&mut self, range: Range<usize>, f: impl Fn(&mut RunStyle)) {
        let ranges = self.paragraph_ranges();
        for (para, seg) in self.paragraphs.iter_mut().zip(&ranges) {
            let covers_end = range.start <= seg.end && range.end >= seg.end && (seg.is_empty() || range.start < seg.end);
            if covers_end {
                f(&mut para.end_style);
            }
        }
        let mut flat = self.flatten();
        flat.split_at(range.start);
        flat.split_at(range.end);
        for (r, style) in flat.spans.iter_mut() {
            if r.start >= range.start && r.end <= range.end {
                f(style);
            }
        }
        flat.normalize();
        self.unflatten(flat);
    }

    /// Applies `f` to every paragraph touched by `range` (at least the one containing `range.start`).
    pub fn apply_paragraphs(&mut self, range: Range<usize>, f: impl Fn(&mut Paragraph)) {
        let ranges = self.paragraph_ranges();
        let first = self.paragraph_at(range.start);
        for (i, (para, seg)) in self.paragraphs.iter_mut().zip(&ranges).enumerate() {
            if i == first || (seg.start < range.end && range.start <= seg.end) {
                f(para);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(size: f32) -> RunStyle {
        RunStyle { size_pt: size, ..Default::default() }
    }

    fn body(text: &str) -> TextBody {
        let mut b = TextBody::default();
        b.paragraphs[0].end_style = style(18.0);
        b.set_plain_text(text);
        b
    }

    #[test]
    fn ranges_and_len() {
        let b = body("ab\n\n你好");
        assert_eq!(b.len(), "ab\n\n你好".len());
        assert_eq!(b.paragraph_ranges(), vec![0..2, 3..3, 4..10]);
        assert_eq!(b.paragraph_at(3), 1);
        assert_eq!(b.paragraph_at(10), 2);
    }

    #[test]
    fn insert_with_style_and_split_paragraphs() {
        let mut b = body("hello");
        b.paragraphs[0].align = crate::Align::Center;
        b.insert(2, "XY", &style(30.0));
        assert_eq!(b.plain_text(), "heXYllo");
        assert_eq!(b.paragraphs[0].runs.len(), 3);
        assert_eq!(b.paragraphs[0].runs[1].style.size_pt, 30.0);
        b.insert(4, "\n", &style(30.0));
        assert_eq!(b.plain_text(), "heXY\nllo");
        assert_eq!(b.paragraphs.len(), 2);
        assert_eq!(b.paragraphs[1].align, crate::Align::Center, "new paragraph inherits properties");
        assert_eq!(b.style_at(5).size_pt, 18.0);
        assert_eq!(b.style_at(4).size_pt, 30.0);
    }

    #[test]
    fn delete_joins_paragraphs() {
        let mut b = body("one\ntwo\nthree");
        b.paragraphs[1].align = crate::Align::Right;
        b.paragraphs[0].align = crate::Align::Center;
        b.delete(2..6);
        assert_eq!(b.plain_text(), "ono\nthree");
        assert_eq!(b.paragraphs.len(), 2);
        assert_eq!(b.paragraphs[0].align, crate::Align::Center);
        b.delete(0..b.len());
        assert_eq!(b.plain_text(), "");
        assert_eq!(b.paragraphs.len(), 1);
    }

    #[test]
    fn apply_style_splits_runs_and_merges_back() {
        let mut b = body("abcdef");
        b.apply_style(1..3, |s| s.bold = true);
        assert_eq!(b.paragraphs[0].runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["a", "bc", "def"]);
        assert!(b.paragraphs[0].runs[1].style.bold);
        b.apply_style(0..6, |s| s.bold = true);
        assert_eq!(b.paragraphs[0].runs.len(), 1);
        assert!(b.paragraphs[0].end_style.bold, "whole-paragraph selection updates the end style");
        assert_eq!(b.styles_in(0..6).len(), 1);
    }

    #[test]
    fn style_at_empty_paragraph_uses_end_style() {
        let mut b = body("a\n");
        b.paragraphs[1].end_style.size_pt = 40.0;
        assert_eq!(b.style_at(2).size_pt, 40.0);
        assert_eq!(b.style_at(1).size_pt, 18.0);
        assert_eq!(b.style_at(0).size_pt, 18.0);
    }

    #[test]
    fn apply_paragraphs_touches_selected_ones() {
        let mut b = body("a\nb\nc");
        b.apply_paragraphs(2..3, |p| p.level = 1);
        assert_eq!(b.paragraphs.iter().map(|p| p.level).collect::<Vec<_>>(), [0, 1, 0]);
        b.apply_paragraphs(0..5, |p| p.level = 2);
        assert_eq!(b.paragraphs.iter().map(|p| p.level).collect::<Vec<_>>(), [2, 2, 2]);
    }
}
