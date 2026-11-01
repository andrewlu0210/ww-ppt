//! Find and replace across the deck: every slide's shape text, table cells,
//! SmartArt nodes and speaker notes, in slide order.

use std::ops::Range;

use ppt_model::{ShapeId, ShapeKind, TextBody};
use ppt_text::TextEngine;

use crate::Editor;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindOptions {
    pub match_case: bool,
    /// Matches must not continue a word on either side.
    pub whole_word: bool,
}

/// Where a match is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindPlace {
    Shape(ShapeId),
    /// A table cell (row, column).
    Cell(ShapeId, usize, usize),
    /// A SmartArt node, by its row in the text pane.
    Node(ShapeId, usize),
    Notes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatch {
    pub slide: usize,
    pub place: FindPlace,
    /// Byte range in the place's plain text.
    pub range: Range<usize>,
}

/// Letters and digits of alphabetic scripts; CJK text has no word boundaries to respect.
fn is_word_char(c: char) -> bool {
    c == '_' || (c.is_alphanumeric() && (c as u32) < 0x2E80)
}

fn chars_equal(a: char, b: char, match_case: bool) -> bool {
    a == b || (!match_case && a.to_lowercase().eq(b.to_lowercase()))
}

/// Non-overlapping occurrences of `query` in `text`.
pub fn find_in(text: &str, query: &str, opts: FindOptions) -> Vec<Range<usize>> {
    let q: Vec<char> = query.chars().collect();
    if q.is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + q.len() <= chars.len() {
        let hit = q.iter().enumerate().all(|(k, qc)| chars_equal(chars[i + k].1, *qc, opts.match_case));
        let bounded = !opts.whole_word || {
            let before = i.checked_sub(1).map(|j| chars[j].1);
            let after = chars.get(i + q.len()).map(|c| c.1);
            !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
        };
        if hit && bounded {
            let start = chars[i].0;
            let end = chars.get(i + q.len()).map_or(text.len(), |c| c.0);
            out.push(start..end);
            i += q.len();
        } else {
            i += 1;
        }
    }
    out
}

/// New texts for SmartArt rows: (row, text).
type NodeTexts = Vec<(usize, String)>;

/// Replaces `ranges` (sorted, non-overlapping) of `body` with `with`, each
/// taking the style of the text it replaces.
fn replace_ranges(body: &mut TextBody, ranges: &[Range<usize>], with: &str) {
    for r in ranges.iter().rev() {
        let style = body.styles_in(r.clone()).into_iter().next().unwrap_or_else(|| body.style_at(r.start));
        body.delete(r.clone());
        if !with.is_empty() {
            body.insert(r.start, with, &style);
        }
    }
}

impl Editor {
    /// Every place that holds text, in search order, with its current text
    /// (the live text of a box being edited).
    pub(crate) fn find_places(&self) -> Vec<(usize, FindPlace, String)> {
        let mut out = Vec::new();
        for (i, slide) in self.pres.slides.iter().enumerate() {
            for shape in &slide.shapes {
                let live = |cell: Option<(usize, usize)>| {
                    self.text.as_ref().filter(|t| i == self.slide && t.shape == shape.id && t.cell == cell).map(|t| t.rich.body.plain_text())
                };
                match &shape.kind {
                    ShapeKind::Diagram { data: Some(d), .. } => {
                        // A node edited on the canvas is live in the editor.
                        let live = self.text.as_ref().filter(|t| i == self.slide && t.shape == shape.id).and_then(|t| Some((t.diagram?.1, t.rich.body.plain_text())));
                        for (r, row) in d.rows().into_iter().enumerate() {
                            let text = live.as_ref().filter(|(lr, _)| *lr == r).map_or(row.text, |(_, t)| t.clone());
                            out.push((i, FindPlace::Node(shape.id, r), text));
                        }
                    }
                    ShapeKind::Table(table) => {
                        for (r, c) in table.visible_cells() {
                            let text = live(Some((r, c))).unwrap_or_else(|| table.rows[r].cells[c].text.plain_text());
                            out.push((i, FindPlace::Cell(shape.id, r, c), text));
                        }
                    }
                    _ => {
                        if let Some(text) = live(None).or_else(|| shape.text.as_ref().map(TextBody::plain_text)) {
                            out.push((i, FindPlace::Shape(shape.id), text));
                        }
                    }
                }
            }
            out.push((i, FindPlace::Notes, slide.notes.clone()));
        }
        out
    }

    /// Every match in the deck, in search order.
    pub fn find_all(&self, query: &str, opts: FindOptions) -> Vec<FindMatch> {
        self.find_places()
            .into_iter()
            .flat_map(|(slide, place, text)| find_in(&text, query, opts).into_iter().map(move |range| FindMatch { slide, place, range }))
            .collect()
    }

    /// Where the search continues from: the place being edited and its
    /// selection, or the start of the current slide. `notes` is the notes
    /// selection when the notes have focus.
    fn find_position(&self, notes: Option<Range<usize>>) -> (usize, Option<FindPlace>, Range<usize>) {
        if let Some(r) = notes {
            return (self.slide, Some(FindPlace::Notes), r);
        }
        if let Some(m) = self.node_match.as_ref().filter(|m| m.slide == self.slide && self.text.is_none()) {
            return (m.slide, Some(m.place), m.range.clone());
        }
        match &self.text {
            Some(t) => {
                let place = match t.cell {
                    Some((r, c)) => FindPlace::Cell(t.shape, r, c),
                    None => FindPlace::Shape(t.shape),
                };
                (self.slide, Some(place), t.rich.range())
            }
            None => (self.slide, None, 0..0),
        }
    }

    /// What the search position currently selects, as a match to replace:
    /// text in the box being edited, a shown SmartArt match, or `notes`.
    pub fn current_match(&self, notes: Option<Range<usize>>) -> Option<FindMatch> {
        let (slide, place, range) = self.find_position(notes);
        Some(FindMatch { slide, place: place?, range })
    }

    /// The match after (or before) the current position, wrapping around:
    /// its index among all matches, and the match. The selected match itself is skipped.
    pub fn find_next(&self, query: &str, opts: FindOptions, backwards: bool, notes: Option<Range<usize>>) -> Option<(usize, Vec<FindMatch>)> {
        let all = self.find_all(query, opts);
        if all.is_empty() {
            return None;
        }
        let places: Vec<(usize, FindPlace)> = self.find_places().into_iter().map(|(s, p, _)| (s, p)).collect();
        let order = |slide: usize, place: Option<FindPlace>| place.and_then(|p| places.iter().position(|q| *q == (slide, p)));
        let (slide, place, sel) = self.find_position(notes);
        // Position as (place order, offset); with nothing edited, just before the current slide's first place.
        let here = match order(slide, place) {
            Some(o) => (o as isize, sel.start),
            None => (places.iter().position(|q| q.0 == slide).map_or(0, |o| o as isize) - 1, usize::MAX),
        };
        let key = |m: &FindMatch| (order(m.slide, Some(m.place)).map_or(-1, |o| o as isize), m.range.start);
        let index = if backwards {
            all.iter().rposition(|m| key(m) < here).unwrap_or(all.len() - 1)
        } else {
            let selected = |m: &FindMatch| order(m.slide, Some(m.place)) == order(slide, place) && m.range == sel;
            all.iter().position(|m| key(m) > here || (key(m) == here && !selected(m))).unwrap_or(0)
        };
        Some((index, all))
    }

    /// Shows a match: goes to its slide and selects it in its text (the
    /// caller selects notes matches in the notes pane).
    pub fn select_match(&mut self, engine: &mut TextEngine, m: &FindMatch) -> bool {
        if m.slide >= self.pres.slides.len() {
            return false;
        }
        self.node_match = None;
        if let FindPlace::Node(id, row) = m.place {
            // Shown in the SmartArt's text pane.
            if self.slide != m.slide {
                self.set_slide(engine, m.slide);
            } else {
                self.end_text_edit(engine);
            }
            self.selection = vec![id];
            self.diagram_focus = Some(row);
            self.diagram_range = Some(m.range.clone());
            self.diagram_requests += 1;
            self.node_match = Some(m.clone());
            return true;
        }
        let editing_it = self.slide == m.slide
            && self.text.as_ref().is_some_and(|t| match m.place {
                FindPlace::Shape(id) => t.shape == id && t.cell.is_none(),
                FindPlace::Cell(id, r, c) => t.shape == id && t.cell == Some((r, c)),
                FindPlace::Node(..) | FindPlace::Notes => false,
            });
        if !editing_it {
            if self.slide != m.slide {
                self.set_slide(engine, m.slide);
            } else {
                self.end_text_edit(engine);
            }
            match m.place {
                FindPlace::Shape(id) => self.begin_text_edit(engine, id, false),
                FindPlace::Cell(id, r, c) => {
                    self.refresh_tables(engine);
                    self.begin_cell_edit(engine, id, r, c);
                }
                FindPlace::Notes => {
                    self.selection.clear();
                    return true;
                }
                FindPlace::Node(..) => unreachable!("handled above"),
            }
        }
        if let Some(t) = &mut self.text {
            t.rich.select_range(m.range.clone());
        }
        true
    }

    /// Replaces match `m` (still matching `query`) with `with`. Text in a box
    /// is replaced through the text editor, so it undoes like typing.
    pub fn replace_match(&mut self, engine: &mut TextEngine, m: &FindMatch, query: &str, with: &str, opts: FindOptions) -> bool {
        let still = self.find_all(query, opts).contains(m);
        if !still {
            return false;
        }
        if m.place == FindPlace::Notes {
            self.end_text_edit(engine);
            let before = self.snapshot();
            let notes = &mut self.pres.slides[m.slide].notes;
            notes.replace_range(m.range.clone(), with);
            self.push_undo(before);
            return true;
        }
        if let FindPlace::Node(..) = m.place {
            return self.replace_node_range(engine, m, with);
        }
        self.replace_range(engine, m, with)
    }

    /// Replaces the text of `m`, a SmartArt node's, with `with` (one undo step).
    fn replace_node_range(&mut self, engine: &mut TextEngine, m: &FindMatch, with: &str) -> bool {
        let FindPlace::Node(id, row) = m.place else { return false };
        self.end_text_edit(engine);
        let Some(mut text) = self.node_text(m.slide, id, row) else { return false };
        if m.range.end > text.len() || !text.is_char_boundary(m.range.start) || !text.is_char_boundary(m.range.end) {
            return false;
        }
        text.replace_range(m.range.clone(), with);
        let before = self.snapshot();
        if !self.set_node_texts(engine, m.slide, id, &[(row, text)]) {
            return false;
        }
        self.push_undo(before);
        // Continue after the replacement.
        let end = m.range.start + with.len();
        self.node_match = Some(FindMatch { range: end..end, ..m.clone() });
        true
    }

    /// Replaces the text of `m` (a shape or cell match) with `with` through the
    /// text editor, leaving it editing there.
    pub fn replace_range(&mut self, engine: &mut TextEngine, m: &FindMatch, with: &str) -> bool {
        if let FindPlace::Node(..) = m.place {
            return self.replace_node_range(engine, m, with);
        }
        if m.place == FindPlace::Notes || !self.select_match(engine, m) {
            return false;
        }
        let Some(t) = &mut self.text else { return false };
        if with.is_empty() {
            t.rich.delete_selection(engine);
        } else {
            t.rich.insert(engine, with);
        }
        self.fit_edited_shape(engine);
        self.changed();
        true
    }

    /// Replaces every match in the deck as one undo step. Returns how many.
    pub fn replace_all(&mut self, engine: &mut TextEngine, query: &str, with: &str, opts: FindOptions) -> usize {
        self.end_text_edit(engine);
        let all = self.find_all(query, opts);
        if all.is_empty() {
            return 0;
        }
        let before = self.snapshot();
        // New node texts per SmartArt: (slide, shape, [(row, text)]).
        let mut nodes: Vec<(usize, ShapeId, NodeTexts)> = Vec::new();
        let mut i = 0;
        while i < all.len() {
            let (slide, place) = (all[i].slide, all[i].place);
            let ranges: Vec<Range<usize>> = all[i..].iter().take_while(|m| m.slide == slide && m.place == place).map(|m| m.range.clone()).collect();
            i += ranges.len();
            if let FindPlace::Node(id, row) = place {
                let mut text = self.node_text(slide, id, row).unwrap_or_default();
                for r in ranges.iter().rev() {
                    text.replace_range(r.clone(), with);
                }
                match nodes.last_mut().filter(|n| n.0 == slide && n.1 == id) {
                    Some(n) => n.2.push((row, text)),
                    None => nodes.push((slide, id, vec![(row, text)])),
                }
                continue;
            }
            let s = &mut self.pres.slides[slide];
            match place {
                FindPlace::Notes => {
                    for r in ranges.iter().rev() {
                        s.notes.replace_range(r.clone(), with);
                    }
                }
                FindPlace::Shape(id) => {
                    if let Some(body) = s.shapes.iter_mut().find(|x| x.id == id).and_then(|x| x.text.as_mut()) {
                        replace_ranges(body, &ranges, with);
                    }
                }
                FindPlace::Cell(id, r, c) => {
                    if let Some(ShapeKind::Table(t)) = s.shapes.iter_mut().find(|x| x.id == id).map(|x| &mut x.kind)
                        && let Some(cell) = t.cell_mut(r, c)
                    {
                        replace_ranges(&mut cell.text, &ranges, with);
                    }
                }
                FindPlace::Node(..) => unreachable!("handled above"),
            }
        }
        for (slide, id, texts) in nodes {
            self.set_node_texts(engine, slide, id, &texts);
        }
        self.push_undo(before);
        self.refresh_tables(engine);
        all.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_rules() {
        let o = FindOptions::default();
        assert_eq!(find_in("Apple apple APPLE", "apple", o), vec![0..5, 6..11, 12..17]);
        assert_eq!(find_in("Apple apple", "apple", FindOptions { match_case: true, ..o }), vec![6..11]);
        let word = FindOptions { whole_word: true, ..o };
        assert_eq!(find_in("cat concat cat_x cat.", "cat", word), vec![0..3, 17..20]);
        // CJK has no word boundaries: whole word still finds it inside text.
        assert_eq!(find_in("營收成長營收", "營收", word), vec![0..6, 12..18]);
        assert_eq!(find_in("aaaa", "aa", o), vec![0..2, 2..4]);
        assert!(find_in("abc", "", o).is_empty());
    }
}
