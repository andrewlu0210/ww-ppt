//! Rich-text editing of one text body: caret, selection, IME composition,
//! formatting and a local undo history.
//!
//! The body is laid out with [`TextEngine::layout_body`], the same function the
//! renderer uses, so what you edit is exactly what is displayed.

use std::ops::Range;

use parley::{Affinity, BoundingBox, Cursor, Selection};
use ppt_model::{Paragraph, RunStyle, TextBody};
use ppt_text::{BodyLayout, TextEngine};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    TextStart,
    TextEnd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    None,
    Typing,
    Deleting,
    Other,
}

#[derive(Clone)]
struct State {
    body: TextBody,
    anchor: usize,
    focus: usize,
}

pub struct RichEdit {
    pub body: TextBody,
    width_pt: f32,
    layout: BodyLayout,
    selection: Selection,
    compose: Option<Range<usize>>,
    compose_style: RunStyle,
    show_caret: bool,
    /// Formatting chosen with a collapsed selection; applies to the next typed text.
    pending: Option<RunStyle>,
    undo: Vec<State>,
    redo: Vec<State>,
    last_op: Op,
}

/// Normalizes pasted/typed text to the body's separators.
fn sanitize(text: &str) -> String {
    text.replace("\r\n", "\n").replace(['\r', '\u{2029}'], "\n").replace('\u{000B}', "\u{2028}")
}

impl RichEdit {
    pub fn new(engine: &mut TextEngine, body: TextBody, width_pt: f32) -> Self {
        let layout = engine.layout_body(&body, width_pt);
        let end = body.len();
        let selection = Selection::from_byte_index(&layout.layout, end, Affinity::Upstream);
        Self {
            body,
            width_pt,
            layout,
            selection,
            compose: None,
            compose_style: RunStyle::default(),
            show_caret: true,
            pending: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_op: Op::None,
        }
    }

    pub fn layout(&self) -> &BodyLayout {
        &self.layout
    }

    pub fn set_width(&mut self, engine: &mut TextEngine, width_pt: f32) {
        if (width_pt - self.width_pt).abs() > f32::EPSILON {
            self.width_pt = width_pt;
            let (a, f) = self.indices();
            self.relayout(engine, a, f);
        }
    }

    fn indices(&self) -> (usize, usize) {
        (self.selection.anchor().index(), self.selection.focus().index())
    }

    pub fn range(&self) -> Range<usize> {
        self.selection.text_range()
    }

    /// Selects a byte range of the text.
    pub fn select_range(&mut self, range: Range<usize>) {
        self.last_op = Op::None;
        self.pending = None;
        let len = self.body.len();
        self.set_selection(range.start.min(len), range.end.min(len));
    }

    pub fn is_collapsed(&self) -> bool {
        self.selection.is_collapsed()
    }

    pub fn is_composing(&self) -> bool {
        self.compose.is_some()
    }

    /// Rebuilds the layout and places the selection at byte offsets.
    fn relayout(&mut self, engine: &mut TextEngine, anchor: usize, focus: usize) {
        self.layout = engine.layout_body(&self.body, self.width_pt);
        let len = self.body.len();
        self.set_selection(anchor.min(len), focus.min(len));
    }

    fn set_selection(&mut self, anchor: usize, focus: usize) {
        let l = &self.layout.layout;
        let a = Cursor::from_byte_index(l, anchor, Affinity::Downstream);
        let f = Cursor::from_byte_index(l, focus, if focus >= anchor { Affinity::Upstream } else { Affinity::Downstream });
        self.selection = Selection::new(a, f);
    }

    fn snapshot(&self) -> State {
        let (anchor, focus) = self.indices();
        State { body: self.body.clone(), anchor, focus }
    }

    /// Records an undo step; consecutive typing (or deleting) is merged into one step.
    fn record(&mut self, op: Op) {
        let merge = op == self.last_op && matches!(op, Op::Typing | Op::Deleting);
        if !merge {
            self.undo.push(self.snapshot());
        }
        self.redo.clear();
        self.last_op = op;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn undo(&mut self, engine: &mut TextEngine) -> bool {
        self.clear_compose(engine);
        let Some(state) = self.undo.pop() else { return false };
        self.redo.push(self.snapshot());
        self.restore(engine, state);
        true
    }

    pub fn redo(&mut self, engine: &mut TextEngine) -> bool {
        let Some(state) = self.redo.pop() else { return false };
        self.undo.push(self.snapshot());
        self.restore(engine, state);
        true
    }

    fn restore(&mut self, engine: &mut TextEngine, state: State) {
        self.body = state.body;
        self.pending = None;
        self.last_op = Op::None;
        self.relayout(engine, state.anchor, state.focus);
    }

    /// Style that typed text will get at the current position.
    pub fn typing_style(&self) -> RunStyle {
        if let Some(p) = &self.pending {
            return p.clone();
        }
        let r = self.range();
        let style = if r.is_empty() { self.body.style_at(r.start) } else { self.body.styles_in(r.clone()).into_iter().next().unwrap_or_else(|| self.body.style_at(r.start)) };
        // Typed text next to a field is plain text.
        RunStyle { field: None, ..style }
    }

    /// Paragraph containing the selection start.
    pub fn current_paragraph(&self) -> &Paragraph {
        &self.body.paragraphs[self.body.paragraph_at(self.range().start)]
    }

    // --- Text changes ------------------------------------------------------

    pub fn insert(&mut self, engine: &mut TextEngine, text: &str) {
        let text = sanitize(text);
        if text.is_empty() {
            return;
        }
        self.clear_compose(engine);
        self.record(if text.contains('\n') || text.chars().count() > 1 { Op::Other } else { Op::Typing });
        let r = self.range();
        let style = self.pending.take().unwrap_or_else(|| self.typing_style());
        self.body.delete(r.clone());
        self.body.insert(r.start, &text, &style);
        let caret = r.start + text.len();
        self.relayout(engine, caret, caret);
    }

    fn delete_range(&mut self, engine: &mut TextEngine, r: Range<usize>) {
        if r.is_empty() {
            return;
        }
        self.record(Op::Deleting);
        self.pending = None;
        self.body.delete(r.clone());
        self.relayout(engine, r.start, r.start);
    }

    /// Deletes the selection, or the cluster/word before the caret.
    pub fn backspace(&mut self, engine: &mut TextEngine, word: bool) {
        let r = if !self.is_collapsed() {
            self.range()
        } else {
            let focus = self.selection.focus();
            let l = &self.layout.layout;
            let prev = if word { focus.previous_visual_word(l) } else { focus.previous_visual(l) };
            prev.index().min(focus.index())..focus.index()
        };
        self.delete_range(engine, r);
    }

    /// Deletes the selection, or the cluster/word after the caret.
    pub fn delete_forward(&mut self, engine: &mut TextEngine, word: bool) {
        let r = if !self.is_collapsed() {
            self.range()
        } else {
            let focus = self.selection.focus();
            let l = &self.layout.layout;
            let next = if word { focus.next_visual_word(l) } else { focus.next_visual(l) };
            focus.index()..next.index().max(focus.index())
        };
        self.delete_range(engine, r);
    }

    /// Deletes from the start of the visual line to the caret (⌘⌫).
    pub fn delete_to_line_start(&mut self, engine: &mut TextEngine) {
        if !self.is_collapsed() {
            return self.delete_range(engine, self.range());
        }
        let start = self.selection.line_start(&self.layout.layout, true).text_range();
        self.delete_range(engine, start);
    }

    pub fn delete_selection(&mut self, engine: &mut TextEngine) {
        self.delete_range(engine, self.range());
    }

    pub fn selected_text(&self) -> Option<String> {
        let r = self.range();
        (!r.is_empty()).then(|| self.body.plain_text()[r].to_string())
    }

    // --- Caret and selection ----------------------------------------------

    pub fn move_caret(&mut self, m: Move, extend: bool) {
        self.last_op = Op::None;
        self.pending = None;
        let l = &self.layout.layout;
        let sel = self.selection;
        if !extend && !sel.is_collapsed() && matches!(m, Move::Left | Move::Right) {
            let r = sel.text_range();
            let at = if m == Move::Left { r.start } else { r.end };
            self.selection = Selection::from_byte_index(l, at, Affinity::Downstream);
            return;
        }
        self.selection = match m {
            Move::Left => sel.previous_visual(l, extend),
            Move::Right => sel.next_visual(l, extend),
            Move::Up => sel.previous_line(l, extend),
            Move::Down => sel.next_line(l, extend),
            Move::WordLeft => sel.previous_visual_word(l, extend),
            Move::WordRight => sel.next_visual_word(l, extend),
            Move::LineStart => sel.line_start(l, extend),
            Move::LineEnd => sel.line_end(l, extend),
            Move::TextStart | Move::TextEnd => {
                let at = if m == Move::TextStart { 0 } else { self.body.len() };
                let c = Cursor::from_byte_index(l, at, Affinity::Downstream);
                if extend { sel.extend(c) } else { c.into() }
            }
        };
    }

    pub fn select_all(&mut self) {
        self.last_op = Op::None;
        self.set_selection(0, self.body.len());
    }

    /// Pointer press in layout space. `clicks`: 2 selects a word, 3 a paragraph.
    pub fn pointer_down(&mut self, x: f32, y: f32, clicks: u32, shift: bool) {
        self.last_op = Op::None;
        self.pending = None;
        let l = &self.layout.layout;
        self.selection = if shift {
            self.selection.shift_click_extension(l, x, y)
        } else {
            match clicks {
                2 => Selection::word_from_point(l, x, y),
                n if n >= 3 => Selection::hard_line_from_point(l, x, y),
                _ => Selection::from_point(l, x, y),
            }
        };
    }

    pub fn pointer_drag(&mut self, x: f32, y: f32) {
        self.selection = self.selection.extend_to_point(&self.layout.layout, x, y);
    }

    // --- IME ---------------------------------------------------------------

    /// Shows `text` as the IME preedit; `cursor` is a byte offset inside it.
    pub fn set_compose(&mut self, engine: &mut TextEngine, text: &str, cursor: usize) {
        if text.is_empty() {
            self.clear_compose(engine);
            return;
        }
        let start = match &self.compose {
            Some(r) => {
                let r = r.clone();
                let start = r.start;
                self.body.delete(r);
                start
            }
            None => {
                self.record(Op::Typing);
                self.compose_style = self.pending.take().unwrap_or_else(|| self.typing_style());
                let r = self.range();
                self.body.delete(r.clone());
                r.start
            }
        };
        self.body.insert(start, text, &self.compose_style);
        self.compose = Some(start..start + text.len());
        let caret = start + cursor.min(text.len());
        self.relayout(engine, caret, caret);
    }

    /// Removes the preedit text, leaving the caret where it started.
    pub fn clear_compose(&mut self, engine: &mut TextEngine) {
        if let Some(r) = self.compose.take() {
            self.body.delete(r.clone());
            self.relayout(engine, r.start, r.start);
        }
    }

    /// Replaces the preedit with the committed text.
    pub fn commit_compose(&mut self, engine: &mut TextEngine, text: &str) {
        let had_compose = self.compose.is_some();
        let style = self.compose_style.clone();
        self.clear_compose(engine);
        if text.is_empty() {
            return;
        }
        if had_compose {
            // The undo step was recorded when composing started.
            self.last_op = Op::Typing;
            self.pending = Some(style);
        }
        self.insert(engine, text);
    }

    // --- Formatting --------------------------------------------------------

    /// Applies `f` to the selected characters, or to the next typed text when the selection is collapsed.
    pub fn apply_style(&mut self, engine: &mut TextEngine, f: impl Fn(&mut RunStyle)) {
        let r = self.range();
        if r.is_empty() {
            let mut style = self.typing_style();
            f(&mut style);
            self.pending = Some(style);
            // An empty paragraph shows its end style, so update that too.
            let p = self.body.paragraph_at(r.start);
            if self.body.paragraph_ranges()[p].is_empty() {
                self.record(Op::Other);
                f(&mut self.body.paragraphs[p].end_style);
                let (a, fo) = self.indices();
                self.relayout(engine, a, fo);
            }
            return;
        }
        self.record(Op::Other);
        self.body.apply_style(r, f);
        let (a, fo) = self.indices();
        self.relayout(engine, a, fo);
    }

    /// Changes the body with access to the selected range, as one undo step.
    pub fn edit_body(&mut self, engine: &mut TextEngine, f: impl FnOnce(&mut TextBody, Range<usize>)) {
        self.record(Op::Other);
        let range = self.range();
        f(&mut self.body, range);
        let (a, fo) = self.indices();
        self.relayout(engine, a, fo);
    }

    pub fn apply_paragraphs(&mut self, engine: &mut TextEngine, f: impl Fn(&mut Paragraph)) {
        self.record(Op::Other);
        self.body.apply_paragraphs(self.range(), f);
        let (a, fo) = self.indices();
        self.relayout(engine, a, fo);
    }

    // --- Geometry (layout space, points) -----------------------------------

    pub fn selection_rects(&self) -> Vec<BoundingBox> {
        self.selection.geometry(&self.layout.layout).into_iter().map(|(b, _)| b).collect()
    }

    pub fn caret(&self) -> Option<BoundingBox> {
        (self.show_caret && self.selection.is_collapsed()).then(|| self.selection.focus().geometry(&self.layout.layout, 1.0))
    }

    pub fn compose_rects(&self) -> Vec<BoundingBox> {
        let Some(r) = &self.compose else { return Vec::new() };
        let l = &self.layout.layout;
        let start = Selection::from_byte_index(l, r.start, Affinity::Downstream);
        let end = Cursor::from_byte_index(l, r.end, Affinity::Upstream);
        start.extend(end).geometry(l).into_iter().map(|(b, _)| b).collect()
    }

    /// Where the IME candidate window should appear.
    pub fn ime_area(&self) -> BoundingBox {
        let caret = self.selection.focus().geometry(&self.layout.layout, 1.0);
        self.compose_rects().into_iter().fold(caret, |a, b| a.union(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> TextEngine {
        let mut engine = TextEngine::new(false);
        engine.register_font(std::fs::read("/System/Library/Fonts/Helvetica.ttc").unwrap());
        engine.set_alias("Calibri", "Helvetica");
        engine
    }

    fn edit(eng: &mut TextEngine, text: &str) -> RichEdit {
        let mut body = TextBody::default();
        body.set_plain_text(text);
        RichEdit::new(eng, body, 300.0)
    }

    #[test]
    fn typing_merges_into_one_undo_step() {
        let mut eng = engine();
        let mut e = edit(&mut eng, "");
        for c in ["a", "b", "c"] {
            e.insert(&mut eng, c);
        }
        assert_eq!(e.body.plain_text(), "abc");
        e.undo(&mut eng);
        assert_eq!(e.body.plain_text(), "");
        e.redo(&mut eng);
        assert_eq!(e.body.plain_text(), "abc");
    }

    #[test]
    fn bold_applies_to_selection_and_pending_style() {
        let mut eng = engine();
        let mut e = edit(&mut eng, "hello world");
        e.move_caret(Move::TextStart, false);
        e.move_caret(Move::WordRight, true);
        e.apply_style(&mut eng, |s| s.bold = true);
        assert!(e.body.paragraphs[0].runs[0].style.bold);
        assert!(!e.body.paragraphs[0].runs.last().unwrap().style.bold);
        // Collapsed: the next typed text is italic, existing text untouched.
        e.move_caret(Move::TextEnd, false);
        e.apply_style(&mut eng, |s| s.italic = true);
        e.insert(&mut eng, "!");
        let last = e.body.paragraphs[0].runs.last().unwrap();
        assert_eq!(last.text, "!");
        assert!(last.style.italic);
    }

    #[test]
    fn enter_splits_and_backspace_joins_paragraphs() {
        let mut eng = engine();
        let mut e = edit(&mut eng, "ab");
        e.move_caret(Move::Left, false);
        e.insert(&mut eng, "\n");
        assert_eq!(e.body.paragraphs.len(), 2);
        e.backspace(&mut eng, false);
        assert_eq!(e.body.plain_text(), "ab");
        assert_eq!(e.range(), 1..1);
    }

    #[test]
    fn ime_compose_and_commit() {
        let mut eng = engine();
        let mut e = edit(&mut eng, "x");
        e.set_compose(&mut eng, "ㄋㄧˇ", "ㄋㄧˇ".len());
        assert_eq!(e.body.plain_text(), "xㄋㄧˇ");
        assert!(!e.compose_rects().is_empty());
        e.commit_compose(&mut eng, "你");
        assert_eq!(e.body.plain_text(), "x你");
        assert!(!e.is_composing());
        e.undo(&mut eng);
        assert_eq!(e.body.plain_text(), "x");
    }

    #[test]
    fn up_down_cross_paragraphs() {
        let mut eng = engine();
        let mut e = edit(&mut eng, "first\nsecond");
        e.move_caret(Move::TextStart, false);
        e.move_caret(Move::Down, false);
        assert!(e.range().start >= 6, "caret moved into the second paragraph: {:?}", e.range());
        e.move_caret(Move::Up, true);
        assert!(!e.is_collapsed());
    }

    #[test]
    fn paragraph_formatting() {
        let mut eng = engine();
        let mut e = edit(&mut eng, "a\nb");
        e.select_all();
        e.apply_paragraphs(&mut eng, |p| p.align = ppt_model::Align::Center);
        assert!(e.body.paragraphs.iter().all(|p| p.align == ppt_model::Align::Center));
        assert_eq!(e.current_paragraph().align, ppt_model::Align::Center);
    }
}
