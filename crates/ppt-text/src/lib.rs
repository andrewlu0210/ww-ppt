//! Text layout for slide text bodies.
//!
//! Layout runs in points. A whole text body is a single parley [`Layout`];
//! paragraph alignment, indents and spacing are applied line by line.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, BreakReason, FontContext, FontFamily, FontFamilyName, FontStyle, FontWeight, InlineBox,
    InlineBoxKind, Layout, LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty, VerticalAlign, YieldData,
};
use ppt_model::{
    Align, Bullet as BulletDef, BulletKind, Color, EMU_PER_PT, Emu, Paragraph, RunStyle, Shadow, TabAlign, Table, TextBody, TextOutline,
};

/// What a run of glyphs is painted with: its fill color and its text effects.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextBrush {
    pub color: Color,
    pub outline: Option<TextOutline>,
    pub shadow: Option<Shadow>,
}

pub use parley;

/// Single line spacing in PowerPoint is about 1.2× the font size for typical fonts.
pub const SINGLE_LINE_HEIGHT: f32 = 1.2;

/// Size of superscript and subscript text relative to the run's size.
const SCRIPT_SCALE: f32 = 2.0 / 3.0;

/// Font size of the layout's default style when every character has its own style.
const STRUT_SIZE: f32 = 1.0;

pub struct TextEngine {
    pub font_cx: FontContext,
    pub layout_cx: LayoutContext<TextBrush>,
    /// Requested family (lowercase) → registered family to use instead.
    aliases: HashMap<String, String>,
    /// Families appended to every font stack, e.g. a CJK font for missing glyphs.
    fallbacks: Vec<String>,
}

impl TextEngine {
    /// `system_fonts` must be false on wasm, where fonts are registered from bytes.
    pub fn new(system_fonts: bool) -> Self {
        let collection = Collection::new(CollectionOptions { shared: false, system_fonts });
        Self {
            font_cx: FontContext { collection, source_cache: SourceCache::default() },
            layout_cx: LayoutContext::new(),
            aliases: HashMap::new(),
            fallbacks: Vec::new(),
        }
    }

    /// Registers font file data (ttf/otf/ttc) and returns the family names it contains.
    pub fn register_font(&mut self, data: Vec<u8>) -> Vec<String> {
        let families = self.font_cx.collection.register_fonts(Blob::new(Arc::new(data)), None);
        let mut names: Vec<String> = families
            .into_iter()
            .filter_map(|(id, _)| self.font_cx.collection.family_name(id).map(str::to_string))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Uses `actual` whenever a text run asks for `requested` (font substitution).
    pub fn set_alias(&mut self, requested: &str, actual: &str) {
        self.aliases.insert(requested.to_lowercase(), actual.to_string());
    }

    pub fn set_fallbacks(&mut self, families: Vec<String>) {
        self.fallbacks = families;
    }

    fn resolve<'a>(&'a self, family: &'a str) -> &'a str {
        self.aliases.get(&family.to_lowercase()).map(String::as_str).unwrap_or(family)
    }

    /// Font stack for a run: latin face, East Asian face, then the global fallbacks.
    pub fn font_stack(&self, style: &RunStyle) -> FontFamily<'static> {
        let mut names: Vec<String> = Vec::new();
        for family in [style.latin_font.as_str(), style.ea_font.as_str()].into_iter().chain(self.fallbacks.iter().map(String::as_str)) {
            if family.is_empty() {
                continue;
            }
            let resolved = self.resolve(family).to_string();
            if !names.contains(&resolved) {
                names.push(resolved);
            }
        }
        let list: Vec<FontFamilyName<'static>> = names.into_iter().map(|n| FontFamilyName::Named(Cow::Owned(n))).collect();
        FontFamily::List(Cow::Owned(list))
    }

    /// Style properties shared by view-mode layout and the text editor.
    pub fn run_properties(&self, style: &RunStyle, font_scale: f32) -> Vec<StyleProperty<'static, TextBrush>> {
        let size = style.size_pt * font_scale;
        // Superscripts and subscripts are drawn smaller, shifted by a fraction of the full size.
        let shifted = style.baseline.abs() > 1e-4;
        vec![
            StyleProperty::FontFamily(self.font_stack(style)),
            StyleProperty::FontSize(if shifted { size * SCRIPT_SCALE } else { size }),
            StyleProperty::VerticalAlign(VerticalAlign::length(style.baseline * size)),
            StyleProperty::Strikethrough(style.strike),
            StyleProperty::LetterSpacing(style.spacing_pt * font_scale),
            StyleProperty::FontWeight(if style.bold { FontWeight::BOLD } else { FontWeight::NORMAL }),
            StyleProperty::FontStyle(if style.italic { FontStyle::Italic } else { FontStyle::Normal }),
            // Linked text shows in the theme's hyperlink color, underlined, like PowerPoint.
            StyleProperty::Underline(style.underline || style.link.is_some()),
            StyleProperty::Brush(TextBrush { color: style.link.as_ref().and_then(|l| l.color).unwrap_or(style.color), outline: style.outline, shadow: style.shadow }),
        ]
    }

    /// Lays out `body` for a box `width_pt` wide (after insets).
    ///
    /// The whole body is one parley layout so selections and caret movement
    /// work across paragraphs. Paragraph indents, spacing and alignment are
    /// applied per line while breaking.
    pub fn layout_body(&mut self, body: &TextBody, width_pt: f32) -> BodyLayout {
        let text = body.plain_text();
        let ranges = body.paragraph_ranges();
        let scale = body.font_scale;
        let first_style = body.paragraphs.first().map(|p| p.first_style().clone()).unwrap_or_default();

        // Style properties are computed up front: the builder borrows the contexts mutably.
        // parley sizes every line at least by the default style (a CSS-like strut), so a
        // non-empty body gets a negligible one; an empty body keeps its paragraph's size.
        let strut = if text.is_empty() { first_style } else { RunStyle { size_pt: STRUT_SIZE, ..first_style } };
        let default_props = self.run_properties(&strut, 1.0);
        let mut span_props: Vec<(Range<usize>, Vec<StyleProperty<'static, TextBrush>>)> = Vec::new();
        for (i, (p, range)) in body.paragraphs.iter().zip(&ranges).enumerate() {
            let line_height = SINGLE_LINE_HEIGHT * p.line_spacing * (1.0 - body.line_spacing_reduction);
            // The paragraph's `\n` (if any) ends its last line: it takes the last run's style
            // (an empty paragraph's comes from its end style), so it never makes a line taller.
            let para_end = if i + 1 < ranges.len() { range.end + 1 } else { range.end };
            let break_style = p.runs.iter().rev().find(|r| !r.text.is_empty()).map_or(&p.end_style, |r| &r.style);
            let mut props = self.run_properties(break_style, scale);
            props.push(StyleProperty::LineHeight(LineHeight::FontSizeRelative(line_height)));
            span_props.push((range.start..para_end.max(range.start), props));
            let mut pos = range.start;
            for run in &p.runs {
                let r = pos..pos + run.text.len();
                pos = r.end;
                span_props.push((r, self.run_properties(&run.style, scale)));
            }
        }

        // Tabs: shaped as a vanishingly small space (so byte offsets, carets and saving are
        // unchanged and no missing-glyph box shows), with an inline box before it that is as
        // wide as the gap to the next tab stop. The gap depends on where the tab lands on its
        // line, so the layout is measured and redone until the widths settle.
        let tab_at: Vec<usize> = text.match_indices('\t').map(|(i, _)| i).collect();
        let shaped_text = if tab_at.is_empty() { Cow::Borrowed(text.as_str()) } else { Cow::Owned(text.replace('\t', " ")) };
        for &i in &tab_at {
            span_props.push((i..i + 1, vec![StyleProperty::FontSize(TAB_FONT_SIZE), StyleProperty::LetterSpacing(0.0), StyleProperty::Underline(false)]));
        }

        // Bullets are shaped separately and drawn in the hanging indent.
        let numbers = list_numbers(&body.paragraphs, &ranges);
        let bullets_shaped: Vec<Option<(Layout<TextBrush>, String)>> = body
            .paragraphs
            .iter()
            .zip(&ranges)
            .zip(&numbers)
            .map(|((p, r), &n)| match &p.bullet {
                Some(b) if !r.is_empty() => Some(self.layout_bullet(b, n, p.first_style(), scale)),
                _ => None,
            })
            .collect();
        let bullet_widths: Vec<Option<f32>> = bullets_shaped.iter().map(|b| b.as_ref().map(|(l, _)| l.width())).collect();

        // Tab gaps and hanging punctuation both depend on where lines break, so the layout is
        // redone until neither changes. `extra[i]` lets line i run that far past the right edge
        // so its last character (a closing punctuation mark) can hang there, as PowerPoint does.
        let hang = Hanging::new(body, &ranges, &text);
        let mut tab_widths = vec![0.0f32; tab_at.len()];
        let mut extra: Vec<f32> = Vec::new();
        let (mut layout, mut lines) = self.build_lines(body, &shaped_text, &default_props, &span_props, &tab_at, &tab_widths, &extra, &bullet_widths, width_pt);
        for _ in 0..16 {
            let mut changed = false;
            if let Some(i) = hang.invalid_extra(&layout, &extra, width_pt) {
                extra.truncate(i);
                changed = true;
            } else {
                if !tab_at.is_empty() {
                    let (next, shrunk) = tab_stop_widths(&layout, &lines, body, &tab_at, &tab_widths, &hang, width_pt);
                    if next.iter().zip(&tab_widths).any(|(a, b)| (a - b).abs() >= 0.05) {
                        tab_widths = next;
                        changed = true;
                    }
                    // A narrowed tab makes room for the punctuation, so that line needs no overhang.
                    for i in shrunk {
                        if extra.get(i).is_some_and(|e| *e > 0.0) {
                            extra[i] = 0.0;
                            changed = true;
                        }
                    }
                }
                if !changed
                    && body.wrap
                    && let Some((i, e)) = hang.next_overhang(&layout, &lines, &extra, width_pt)
                {
                    extra.resize(i, 0.0);
                    extra.push(e);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
            (layout, lines) = self.build_lines(body, &shaped_text, &default_props, &span_props, &tab_at, &tab_widths, &extra, &bullet_widths, width_pt);
        }

        let to_pt = |emu: i64| (emu as f64 / EMU_PER_PT) as f32;
        let mut bullets = Vec::new();
        for (line, info) in layout.lines().zip(&lines) {
            if !info.first {
                continue;
            }
            let p = &body.paragraphs[info.paragraph];
            if let Some(Some((b, label))) = bullets_shaped.get(info.paragraph) {
                let baseline = b.lines().next().map(|l| l.metrics().baseline).unwrap_or(0.0);
                // Left-aligned bullets sit at the hanging indent; otherwise just before the text.
                let text_start = line.metrics().inline_min_coord + line.metrics().offset;
                let x = match p.align {
                    Align::Left | Align::Justify => to_pt(p.mar_l + p.indent).max(0.0),
                    _ => text_start - b.width() - BULLET_GAP,
                };
                bullets.push(Bullet { layout: b.clone(), text: label.clone(), x, y: line.metrics().baseline - baseline });
            }
        }
        let height = layout.lines().last().map(|l| l.metrics().block_max_coord).unwrap_or(0.0);
        BodyLayout { layout, text, lines, bullets, height }
    }

    /// Builds the parley layout of `text` and breaks it into lines with the paragraphs'
    /// indents, bullets and alignment. `tab_widths[k]` is the gap before the tab at `tab_at[k]`.
    #[allow(clippy::too_many_arguments)]
    fn build_lines(
        &mut self,
        body: &TextBody,
        text: &str,
        default_props: &[StyleProperty<'static, TextBrush>],
        span_props: &[(Range<usize>, Vec<StyleProperty<'static, TextBrush>>)],
        tab_at: &[usize],
        tab_widths: &[f32],
        extra: &[f32],
        bullet_widths: &[Option<f32>],
        width_pt: f32,
    ) -> (Layout<TextBrush>, Vec<LineInfo>) {
        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(SINGLE_LINE_HEIGHT)));
        for prop in default_props {
            builder.push_default(prop.clone());
        }
        for (range, props) in span_props {
            if range.is_empty() {
                continue;
            }
            for prop in props {
                builder.push(prop.clone(), range.clone());
            }
        }
        for (k, (&index, &width)) in tab_at.iter().zip(tab_widths).enumerate() {
            builder.push_inline_box(InlineBox {
                id: k as u64,
                kind: InlineBoxKind::InFlow,
                index,
                width,
                height: 0.0,
                baseline: None,
                vertical_align: VerticalAlign::BASELINE,
            });
        }
        let mut layout: Layout<TextBrush> = builder.build(text);
        // Mandatory breaks in text order: `\n` ends a paragraph, U+2028 is a soft line break.
        let mut breaks = text.chars().filter(|c| *c == '\n' || *c == '\u{2028}');
        let max = if body.wrap { width_pt.max(1.0) } else { f32::MAX };
        let to_pt = |emu: i64| (emu as f64 / EMU_PER_PT) as f32;
        let mut lines = Vec::new();
        {
            let mut breaker = layout.break_lines();
            // parley requires every line's limit to stay within the layout's; overhangs count too.
            // (Alignment works from each line's reported width, so this doesn't move anything.)
            let widest_overhang = extra.iter().copied().fold(0.0f32, f32::max);
            breaker.state_mut().set_layout_max_advance(if body.wrap { max + widest_overhang } else { max });
            let (mut para, mut first) = (0usize, true);
            loop {
                let p = &body.paragraphs[para.min(body.paragraphs.len() - 1)];
                let mut x = to_pt(p.mar_l);
                if first {
                    if para > 0 {
                        let prev = &body.paragraphs[para - 1];
                        let y = breaker.state().line_y();
                        breaker.state_mut().set_line_y(y + (prev.space_after + p.space_before) as f64);
                    }
                    x += to_pt(p.indent);
                    if let Some(Some(bw)) = bullet_widths.get(para) {
                        x = (x + bw + BULLET_GAP).max(to_pt(p.mar_l));
                    }
                }
                let x = x.clamp(0.0, (width_pt - 1.0).max(0.0));
                breaker.state_mut().set_line_x(x);
                let overhang = extra.get(lines.len()).copied().unwrap_or(0.0);
                breaker.state_mut().set_line_max_advance(if body.wrap { (max - x).max(1.0) + overhang } else { f32::MAX });
                let Some(data) = breaker.break_next() else { break };
                let YieldData::LineBreak(data) = data else { continue };
                // Alignment is applied by `align(Right)` below; the line width we report
                // decides how far right each line moves.
                let available = (width_pt - x).max(0.0);
                let content = data.metrics.advance - data.metrics.hanging_advance;
                let free = (available - content).max(0.0);
                let shift = match p.align {
                    Align::Left | Align::Justify => 0.0,
                    Align::Center => free / 2.0,
                    Align::Right => free,
                };
                breaker.set_prior_line_width(content + shift);
                lines.push(LineInfo { paragraph: para.min(body.paragraphs.len() - 1), first });
                match data.reason {
                    BreakReason::Explicit => match breaks.next() {
                        Some('\n') => {
                            para += 1;
                            first = true;
                        }
                        _ => first = false,
                    },
                    _ => first = false,
                }
            }
            breaker.finish();
        }
        layout.align(Alignment::Right, AlignmentOptions::default());
        (layout, lines)
    }

    /// Shapes item `n`'s bullet: its own font, color and relative size, else the text's.
    /// Also returns the label's text.
    fn layout_bullet(&mut self, bullet: &BulletDef, n: u32, style: &RunStyle, scale: f32) -> (Layout<TextBrush>, String) {
        let (text, font) = bullet.label(n);
        let mut style = RunStyle { size_pt: style.size_pt * bullet.size.unwrap_or(1.0), color: bullet.color.unwrap_or(style.color), ..style.clone() };
        if let Some(font) = font {
            style.latin_font = font.to_string();
            style.ea_font = font.to_string();
        }
        let props = self.run_properties(&style, scale);
        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, &text, 1.0, true);
        for prop in props {
            builder.push_default(prop);
        }
        builder.push_default(StyleProperty::Underline(false));
        let mut layout = builder.build(&text);
        layout.break_all_lines(None);
        (layout, text)
    }
}

impl TextEngine {
    /// Row heights of `table` in EMU: each row's minimum height, grown so every
    /// cell's text (plus margins) fits. Merged cells grow their last row.
    pub fn table_row_heights(&mut self, table: &Table) -> Vec<Emu> {
        let mut heights: Vec<Emu> = table.rows.iter().map(|r| r.height.max(0)).collect();
        let cols = table.col_offsets();
        let mut spanning = Vec::new();
        for (r, row) in table.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if cell.is_covered() || c >= table.cols.len() {
                    continue;
                }
                let end = (c + cell.grid_span.max(1) as usize).min(table.cols.len());
                let [l, t, rt, b] = cell.text.insets;
                let width = ((cols[end] - cols[c] - l - rt).max(0) as f64 / EMU_PER_PT) as f32;
                let text_h = if cell.text.is_empty() {
                    // An empty cell still holds one line at its end-of-paragraph size.
                    cell.text.paragraphs.first().map_or(18.0, |p| p.end_style.size_pt) * SINGLE_LINE_HEIGHT
                } else {
                    self.layout_body(&cell.text, width).height
                };
                let need = (text_h as f64 * EMU_PER_PT).ceil() as Emu + t + b;
                let span = cell.row_span.max(1) as usize;
                if span == 1 {
                    heights[r] = heights[r].max(need);
                } else {
                    spanning.push((r, (r + span).min(heights.len()), need));
                }
            }
        }
        for (start, end, need) in spanning {
            let have: Emu = heights[start..end].iter().sum();
            if need > have {
                heights[end - 1] += need - have;
            }
        }
        heights
    }
}

/// Item numbers for numbered paragraphs (0 elsewhere). A list continues
/// across deeper levels and empty paragraphs; any other paragraph at the same
/// level, or a different scheme, starts it over.
fn list_numbers(paragraphs: &[Paragraph], ranges: &[Range<usize>]) -> Vec<u32> {
    let mut counters: [Option<(String, u32)>; 9] = Default::default();
    paragraphs
        .iter()
        .zip(ranges)
        .map(|(p, r)| {
            let level = (p.level as usize).min(8);
            for c in counters.iter_mut().skip(level + 1) {
                *c = None;
            }
            if r.is_empty() {
                return 0;
            }
            match p.bullet.as_ref().map(|b| &b.kind) {
                Some(BulletKind::AutoNum { scheme, start }) => {
                    let n = match &counters[level] {
                        Some((s, n)) if s == scheme => n + 1,
                        _ => *start,
                    };
                    counters[level] = Some((scheme.clone(), n));
                    n
                }
                _ => {
                    counters[level] = None;
                    0
                }
            }
        })
        .collect()
}

/// Gap between a bullet and the text after it, in points.
const BULLET_GAP: f32 = 4.0;

/// Font size the space standing in for a tab is shaped at: wide enough to exist, too small to show.
const TAB_FONT_SIZE: f32 = 0.01;

/// The gap each tab needs to reach its stop, given a layout made with `current` gaps.
///
/// Stops are measured from the left of the text area. A tab goes to the first explicit stop
/// past where it starts, else to the next multiple of the paragraph's default tab size.
/// Right, center and decimal stops line up the text that follows (up to the next tab or the
/// end of the line); a decimal stop is treated as a right stop.
///
/// When a line only fits because its last character hangs past the edge, PowerPoint narrows
/// the line's last tab instead, by the overhang. Returns the gaps and the lines narrowed so.
fn tab_stop_widths(
    layout: &Layout<TextBrush>,
    lines: &[LineInfo],
    body: &TextBody,
    tab_at: &[usize],
    current: &[f32],
    hang: &Hanging,
    width_pt: f32,
) -> (Vec<f32>, Vec<usize>) {
    let to_pt = |emu: i64| (emu as f64 / EMU_PER_PT) as f32;
    let mut out = current.to_vec();
    let mut shrunk = Vec::new();
    for (index, (line, info)) in layout.lines().zip(lines).enumerate() {
        let p = &body.paragraphs[info.paragraph];
        let m = line.metrics();
        // Positions without the alignment shift; boxes in visual (= logical, for LTR) order.
        let boxes: Vec<(usize, f32)> = line
            .items()
            .filter_map(|item| match item {
                PositionedLayoutItem::InlineBox(b) => Some((b.id as usize, b.x - m.offset)),
                PositionedLayoutItem::GlyphRun(_) => None,
            })
            .filter(|(k, _)| *k < tab_at.len())
            .collect();
        let line_end = m.inline_min_coord + m.advance - m.hanging_advance;
        for (n, &(k, x)) in boxes.iter().enumerate() {
            let next = boxes.get(n + 1).map_or(line_end, |b| b.1);
            let following = (next - (x + current[k])).max(0.0);
            let stop = p.tabs.iter().find(|t| to_pt(t.pos) > x + 0.5).map(|t| (to_pt(t.pos), t.align)).unwrap_or_else(|| {
                let size = to_pt(p.tab_size.max(1));
                ((((x + 0.5) / size).floor() + 1.0) * size, TabAlign::Left)
            });
            let gap = match stop.1 {
                TabAlign::Left => stop.0 - x,
                TabAlign::Center => stop.0 - x - following / 2.0,
                TabAlign::Right | TabAlign::Decimal => stop.0 - x - following,
            };
            out[k] = gap.max(0.0);
        }
        if let (Some(&(last, _)), Some(punct)) = (boxes.last(), hang.trailing(&line)) {
            let end = line_end + boxes.iter().map(|(k, _)| out[*k] - current[*k]).sum::<f32>();
            let over = end - width_pt;
            if over > 0.01 && over <= punct + 0.01 {
                out[last] = (out[last] - over - 0.05).max(0.0);
                shrunk.push(index);
            }
        }
    }
    (out, shrunk)
}

/// Characters that may not start a line in Chinese and Korean text; at the end of a line
/// PowerPoint lets one of them hang past the right edge instead of wrapping it.
const NO_LINE_START: &str = "!%),.:;?]}¢°’”‰′″℃、。〉》」』】〕〗〙〛！％），．：；？］｝～｡｣､";

/// Whether `c` may hang at a line end in text of language `lang`. Japanese hangs only its
/// full stop and commas; Chinese and Korean any closing punctuation; other languages nothing.
fn hangs(c: char, lang: &str) -> bool {
    let lang = lang.to_ascii_lowercase();
    let is = |p: &str| lang == p || lang.starts_with(&format!("{p}-"));
    if is("ja") {
        matches!(c, '。' | '、' | '，' | '．' | '｡' | '､')
    } else if is("zh") || is("ko") {
        NO_LINE_START.contains(c)
    } else {
        false
    }
}

/// Hanging punctuation for one text body: which byte offsets hold a character that may hang.
struct Hanging<'a> {
    text: &'a str,
    /// Byte range of each run with its language, for paragraphs that allow hanging.
    runs: Vec<(Range<usize>, &'a str)>,
}

impl<'a> Hanging<'a> {
    fn new(body: &'a TextBody, ranges: &[Range<usize>], text: &'a str) -> Self {
        let mut runs = Vec::new();
        for (p, range) in body.paragraphs.iter().zip(ranges) {
            if !p.hanging_punct {
                continue;
            }
            let mut pos = range.start;
            for run in &p.runs {
                runs.push((pos..pos + run.text.len(), run.style.lang.as_str()));
                pos += run.text.len();
            }
        }
        Self { text, runs }
    }

    fn at(&self, i: usize) -> bool {
        let Some(c) = self.text.get(i..).and_then(|t| t.chars().next()) else { return false };
        self.runs.iter().find(|(r, _)| r.contains(&i)).is_some_and(|(_, lang)| hangs(c, lang))
    }

    /// The line's clusters in text order: (byte offset, advance, is whitespace).
    fn clusters(line: &parley::Line<'_, TextBrush>) -> Vec<(usize, f32, bool)> {
        let mut out: Vec<(usize, f32, bool)> =
            line.runs().flat_map(|r| r.clusters().map(|c| (c.text_range().start, c.advance(), c.is_space_or_nbsp() || c.is_hard_line_break())).collect::<Vec<_>>()).collect();
        out.sort_by_key(|c| c.0);
        out
    }

    /// Width of the line's last visible character, when that character may hang.
    fn trailing(&self, line: &parley::Line<'_, TextBrush>) -> Option<f32> {
        let clusters = Self::clusters(line);
        let last = clusters.iter().rev().find(|c| !c.2)?;
        self.at(last.0).then_some(last.1)
    }

    /// The first line `i` (without an overhang yet) that could take the start of the next line
    /// up to a hanging punctuation mark: everything but the mark fits, the mark hangs. Returns
    /// `i` and the overhang to allow.
    fn next_overhang(&self, layout: &Layout<TextBrush>, lines: &[LineInfo], extra: &[f32], width_pt: f32) -> Option<(usize, f32)> {
        let all: Vec<_> = layout.lines().collect();
        for i in 0..all.len().saturating_sub(1) {
            if lines.get(i + 1).is_none_or(|l| l.first) || extra.get(i).is_some_and(|e| *e > 0.0) {
                continue;
            }
            let m = all[i].metrics();
            let available = width_pt - m.inline_min_coord;
            // Trailing spaces stop hanging once the next line's text joins this one.
            let content = m.advance;
            let next = Self::clusters(&all[i + 1]);
            // Walk to the first mark. If everything before it fits on line i, line breaking only
            // moved it down because there was no break before the mark, so all of it comes up.
            let mut prefix = 0.0;
            for (n, &(at, advance, space)) in next.iter().enumerate() {
                if space {
                    break;
                }
                // Only the last of several closing marks in a row can hang.
                let followed = next.get(n + 1).is_some_and(|c| !c.2 && self.at(c.0));
                if self.at(at) && !followed {
                    if content + prefix <= available + 0.01 && content + prefix + advance > available {
                        return Some((i, content + prefix + advance - available + 0.05));
                    }
                    break;
                }
                prefix += advance;
                if content + prefix > available + 0.01 {
                    break;
                }
            }
        }
        None
    }

    /// The first line whose overhang no longer ends in a hanging mark (the text moved).
    fn invalid_extra(&self, layout: &Layout<TextBrush>, extra: &[f32], width_pt: f32) -> Option<usize> {
        layout.lines().zip(extra).position(|(line, &e)| {
            if e <= 0.0 {
                return false;
            }
            let m = line.metrics();
            let available = width_pt - m.inline_min_coord;
            match self.trailing(&line) {
                Some(w) => m.advance - m.hanging_advance - w > available + 0.01,
                None => true,
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineInfo {
    pub paragraph: usize,
    /// First line of its paragraph.
    pub first: bool,
}

#[derive(Clone)]
pub struct Bullet {
    pub layout: Layout<TextBrush>,
    /// The label's text (for text extraction in exports).
    pub text: String,
    /// Offset of the bullet layout's origin in body layout space.
    pub x: f32,
    pub y: f32,
}

#[derive(Clone)]
pub struct BodyLayout {
    /// Every paragraph in one layout; coordinates are points.
    pub layout: Layout<TextBrush>,
    /// The text the layout was built from.
    pub text: String,
    /// One entry per layout line.
    pub lines: Vec<LineInfo>,
    pub bullets: Vec<Bullet>,
    /// Height including paragraph spacing and a trailing empty paragraph.
    pub height: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppt_model::Run;

    fn engine() -> TextEngine {
        let mut engine = TextEngine::new(false);
        let data = std::fs::read("/System/Library/Fonts/Helvetica.ttc").expect("Helvetica is part of macOS");
        let families = engine.register_font(data);
        assert!(families.iter().any(|f| f == "Helvetica"), "{families:?}");
        engine.set_alias("Calibri", "Helvetica");
        engine
    }

    fn body(text: &str, size: f32) -> TextBody {
        let mut body = TextBody::default();
        body.paragraphs[0].end_style = RunStyle { size_pt: size, ..Default::default() };
        body.set_plain_text(text);
        body
    }

    fn line_x(b: &BodyLayout, i: usize) -> f32 {
        let m = *b.layout.get(i).unwrap().metrics();
        m.inline_min_coord + m.offset
    }

    #[test]
    fn wraps_long_text() {
        let mut engine = engine();
        let one = engine.layout_body(&body("Hello", 20.0), 400.0);
        let many = engine.layout_body(&body(&"Hello world ".repeat(20), 20.0), 400.0);
        assert_eq!(one.layout.len(), 1);
        assert!(many.layout.len() > 3, "got {}", many.layout.len());
        assert!(many.height > one.height * 3.0);
    }

    #[test]
    fn paragraphs_share_one_layout_including_empty_ones() {
        let mut engine = engine();
        let layout = engine.layout_body(&body("a\n\nb", 10.0), 100.0);
        assert_eq!(layout.layout.len(), 3);
        assert_eq!(layout.lines.iter().map(|l| l.paragraph).collect::<Vec<_>>(), [0, 1, 2]);
        assert!(layout.lines.iter().all(|l| l.first));
        assert!((layout.height - 36.0).abs() < 0.5, "three 12pt lines, got {}", layout.height);
    }

    #[test]
    fn trailing_empty_paragraph_counts_toward_height() {
        let mut engine = engine();
        let one = engine.layout_body(&body("a", 10.0), 100.0);
        let two = engine.layout_body(&body("a\n", 10.0), 100.0);
        assert!((two.height - one.height * 2.0).abs() < 0.5, "{} vs {}", one.height, two.height);
    }

    #[test]
    fn per_paragraph_alignment() {
        let mut engine = engine();
        let mut b = body("left\ncenter\nright", 10.0);
        b.paragraphs[1].align = Align::Center;
        b.paragraphs[2].align = Align::Right;
        let l = engine.layout_body(&b, 200.0);
        let adv = |i: usize| l.layout.get(i).unwrap().metrics().advance;
        assert!(line_x(&l, 0).abs() < 0.01);
        assert!((line_x(&l, 1) - (200.0 - adv(1)) / 2.0).abs() < 0.5, "center x {}", line_x(&l, 1));
        assert!((line_x(&l, 2) + adv(2) - 200.0).abs() < 0.5, "right edge {}", line_x(&l, 2) + adv(2));
    }

    #[test]
    fn hanging_indent_with_bullet() {
        let mut engine = engine();
        let mut b = body(&"word ".repeat(30), 10.0);
        let p = &mut b.paragraphs[0];
        p.mar_l = 342_900; // 27pt
        p.indent = -342_900;
        p.bullet = Some(BulletDef::char('•'));
        let l = engine.layout_body(&b, 200.0);
        assert!(l.layout.len() > 1);
        assert_eq!(l.bullets.len(), 1);
        assert!(l.bullets[0].x.abs() < 0.01, "bullet in the hanging indent");
        assert!((line_x(&l, 0) - 27.0).abs() < 0.5, "first line text after bullet: {}", line_x(&l, 0));
        assert!((line_x(&l, 1) - 27.0).abs() < 0.5, "continuation lines at marL: {}", line_x(&l, 1));
    }

    #[test]
    fn paragraph_spacing_moves_lines_down() {
        let mut engine = engine();
        let plain = engine.layout_body(&body("a\nb", 10.0), 100.0);
        let mut spaced = body("a\nb", 10.0);
        spaced.paragraphs[0].space_after = 6.0;
        spaced.paragraphs[1].space_before = 10.0;
        let spaced = engine.layout_body(&spaced, 100.0);
        let y = |b: &BodyLayout| b.layout.get(1).unwrap().metrics().baseline;
        assert!((y(&spaced) - y(&plain) - 16.0).abs() < 0.5);
        assert!((spaced.height - plain.height - 16.0).abs() < 0.5);
    }

    #[test]
    fn mixed_runs_keep_their_sizes() {
        let mut engine = engine();
        let mut b = body("", 10.0);
        b.paragraphs[0].runs = vec![
            Run { text: "small ".into(), style: RunStyle { size_pt: 10.0, ..Default::default() } },
            Run { text: "BIG".into(), style: RunStyle { size_pt: 40.0, ..Default::default() } },
        ];
        let l = engine.layout_body(&b, 500.0);
        assert_eq!(l.layout.len(), 1);
        assert!(l.height > 40.0, "line grows to the largest run: {}", l.height);
    }

    #[test]
    fn font_stack_applies_aliases_and_fallbacks() {
        let mut engine = engine();
        engine.set_fallbacks(vec!["PingFang TC".into()]);
        let style = RunStyle { latin_font: "Calibri".into(), ea_font: "微軟正黑體".into(), ..Default::default() };
        let FontFamily::List(list) = engine.font_stack(&style) else { panic!() };
        let names: Vec<String> = list.iter().map(|n| n.to_string()).collect();
        assert_eq!(names.len(), 3);
        assert!(names[0].contains("Helvetica") && names[1].contains("微軟正黑體") && names[2].contains("PingFang TC"), "{names:?}");
    }

    #[test]
    fn smaller_lines_after_a_large_one_keep_their_own_height() {
        let mut engine = engine();
        let para = |size: f32| Paragraph {
            runs: vec![Run { text: "Ab".into(), style: RunStyle { size_pt: size, ..Default::default() } }],
            end_style: RunStyle { size_pt: size, ..Default::default() },
            ..Default::default()
        };
        let body = TextBody { paragraphs: vec![para(18.0), para(10.0), para(10.0)], ..Default::default() };
        let l = engine.layout_body(&body, 400.0);
        let heights: Vec<f32> = l.layout.lines().map(|line| line.metrics().line_height).collect();
        assert!((heights[0] - 21.6).abs() < 0.1 && (heights[1] - 12.0).abs() < 0.1 && (heights[2] - 12.0).abs() < 0.1, "{heights:?}");
        // An empty body still has a line as tall as its paragraph's text would be.
        let empty = TextBody { paragraphs: vec![Paragraph { end_style: RunStyle { size_pt: 20.0, ..Default::default() }, ..Default::default() }], ..Default::default() };
        assert!((engine.layout_body(&empty, 400.0).height - 24.0).abs() < 0.5);
    }

    #[test]
    fn numbered_lists_count_per_level() {
        let num = |level: u8| Paragraph {
            level,
            bullet: Some(BulletDef { kind: BulletKind::AutoNum { scheme: "arabicPeriod".into(), start: 1 }, font: None, color: None, size: None }),
            runs: vec![Run { text: "x".into(), style: RunStyle::default() }],
            ..Default::default()
        };
        let plain = Paragraph { runs: vec![Run { text: "x".into(), style: RunStyle::default() }], ..Default::default() };
        let paras = vec![num(0), num(1), num(1), num(0), plain, num(0)];
        let ranges: Vec<Range<usize>> = (0..paras.len()).map(|i| i * 2..i * 2 + 1).collect();
        // 1, (a: 1, 2), 2, then a plain paragraph restarts the list.
        assert_eq!(list_numbers(&paras, &ranges), vec![1, 1, 2, 2, 0, 1]);
        let mut engine = engine();
        let mut body = TextBody { paragraphs: paras, ..Default::default() };
        body.paragraphs[0].mar_l = 342_900;
        body.paragraphs[0].indent = -342_900;
        let l = engine.layout_body(&body, 400.0);
        assert_eq!(l.bullets.len(), 5, "every numbered paragraph gets its number");
    }


    /// Glyph (x, y, font size) of every glyph, in order.
    fn glyphs(b: &BodyLayout) -> Vec<(f32, f32, f32)> {
        let mut out = Vec::new();
        for line in b.layout.lines() {
            for item in line.items() {
                if let parley::PositionedLayoutItem::GlyphRun(run) = item {
                    let size = run.run().font_size();
                    out.extend(run.positioned_glyphs().map(|g| (g.x, g.y, size)));
                }
            }
        }
        out
    }

    /// x of the first glyph drawn at a normal size after byte offset `from` (tab stand-ins are tiny).
    fn x_after(b: &BodyLayout, from: usize) -> f32 {
        for line in b.layout.lines() {
            for item in line.items() {
                if let parley::PositionedLayoutItem::GlyphRun(run) = item
                    && run.run().font_size() > 1.0
                    && run.run().text_range().start >= from
                {
                    return run.positioned_glyphs().next().map_or(f32::NAN, |g| g.x);
                }
            }
        }
        f32::NAN
    }

    #[test]
    fn tabs_go_to_default_stops() {
        let mut engine = engine();
        // 預設定位點每 1 吋（72pt）一個；從 "ab" 之後跳到 72pt，第二個 Tab 跳到 144pt
        let b = engine.layout_body(&body("ab\tc\td", 12.0), 400.0);
        assert!((x_after(&b, 3) - 72.0).abs() < 0.5, "c at {}", x_after(&b, 3));
        assert!((x_after(&b, 5) - 144.0).abs() < 0.5, "d at {}", x_after(&b, 5));
        // Tab 本身只是極小的空白，不會畫出缺字方框
        assert!(glyphs(&b).iter().filter(|g| g.2 > 1.0).count() == 4);
        // 開頭的 Tab 讓整行從第一個定位點開始
        let lead = engine.layout_body(&body("\tx", 12.0), 400.0);
        assert!((x_after(&lead, 1) - 72.0).abs() < 0.5, "x at {}", x_after(&lead, 1));
    }

    #[test]
    fn tabs_use_explicit_stops_and_their_alignment() {
        use ppt_model::{TabAlign, TabStop};
        let mut engine = engine();
        let mut tb = body("a\tb\tcc", 12.0);
        let pt = |v: f64| (v * EMU_PER_PT) as i64;
        tb.paragraphs[0].tabs = vec![TabStop { pos: pt(30.0), align: TabAlign::Left }, TabStop { pos: pt(200.0), align: TabAlign::Right }];
        tb.paragraphs[0].tab_size = pt(50.0);
        let b = engine.layout_body(&tb, 400.0);
        assert!((x_after(&b, 2) - 30.0).abs() < 0.5, "b at {}", x_after(&b, 2));
        // 靠右定位點：後面的 "cc" 結束在 200pt
        let line = b.layout.lines().next().unwrap();
        let end = line.metrics().inline_min_coord + line.metrics().advance;
        assert!((end - 200.0).abs() < 0.5, "cc ends at {end}");
        // 超過最後一個定位點後，用預設間距的倍數
        let mut past = body("a\tb\tc", 12.0);
        past.paragraphs[0].tabs = vec![TabStop { pos: pt(30.0), align: TabAlign::Left }];
        past.paragraphs[0].tab_size = pt(50.0);
        let b = engine.layout_body(&past, 400.0);
        assert!((x_after(&b, 4) - 50.0).abs() < 0.5, "c at {}", x_after(&b, 4));
    }

    /// `text` in language `lang`, with the box `overflow` points too narrow for it on one line.
    fn hang_case(engine: &mut TextEngine, text: &str, lang: &str, overflow: f32) -> (TextBody, f32) {
        let mut tb = body(text, 20.0);
        for r in &mut tb.paragraphs[0].runs {
            r.style.lang = lang.to_string();
        }
        let natural = engine.layout_body(&tb, 1000.0).layout.lines().next().map_or(0.0, |l| l.metrics().advance);
        (tb, natural - overflow)
    }

    #[test]
    fn closing_punctuation_hangs_in_chinese_text() {
        let mut engine = engine();
        let bang = engine.layout_body(&body("!", 20.0), 1000.0).layout.lines().next().unwrap().metrics().advance;
        // 中文：只差最後的「!」放不下時，讓它懸在框外，不換行
        let (tb, width) = hang_case(&mut engine, "aa xxxxxxxx!", "zh-CN", bang * 0.6);
        assert_eq!(engine.layout_body(&tb, width).layout.len(), 1);
        // 英文、或段落關掉 hangingPunct：照常換行
        let (tb, width) = hang_case(&mut engine, "aa xxxxxxxx!", "en-US", bang * 0.6);
        assert_eq!(engine.layout_body(&tb, width).layout.len(), 2);
        let (mut tb, width) = hang_case(&mut engine, "aa xxxxxxxx!", "zh-CN", bang * 0.6);
        tb.paragraphs[0].hanging_punct = false;
        assert_eq!(engine.layout_body(&tb, width).layout.len(), 2);
        // 超出比標點本身還寬：放不下，照常換行
        let (tb, width) = hang_case(&mut engine, "aa xxxxxxxx!", "zh-CN", bang + 2.0);
        assert_eq!(engine.layout_body(&tb, width).layout.len(), 2);
        // 日文只有句號和逗號會懸掛
        let (tb, width) = hang_case(&mut engine, "aa xxxxxxxx!", "ja-JP", bang * 0.6);
        assert_eq!(engine.layout_body(&tb, width).layout.len(), 2);
    }

    #[test]
    fn a_tab_narrows_instead_of_letting_punctuation_hang() {
        let mut engine = engine();
        let (tb, _) = hang_case(&mut engine, "\txxxxxxxx!", "zh-CN", 0.0);
        // 框寬讓 Tab（到 72pt）後的文字超出 5pt：Tab 縮 5pt，整行剛好到右緣
        let text_w = engine.layout_body(&body("xxxxxxxx!", 20.0), 1000.0).layout.lines().next().unwrap().metrics().advance;
        let width = 72.0 + text_w - 5.0;
        let b = engine.layout_body(&tb, width);
        assert_eq!(b.layout.len(), 1);
        assert!((x_after(&b, 1) - 67.0).abs() < 0.2, "text at {}", x_after(&b, 1));
    }

    #[test]
    fn scripts_shift_and_shrink_and_spacing_widens() {
        let mut e = engine();
        let mut b = body("x2x2", 30.0);
        b.apply_style(1..2, |s| s.baseline = 0.3);
        b.apply_style(3..4, |s| s.baseline = -0.25);
        let g = glyphs(&e.layout_body(&b, 500.0));
        assert!(g[1].1 < g[0].1 - 5.0 && g[3].1 > g[0].1 + 5.0, "{g:?}");
        assert!((g[1].2 - 20.0).abs() < 0.01, "scripts are smaller: {g:?}");
        let plain = glyphs(&e.layout_body(&body("abc", 30.0), 500.0));
        let mut spaced = body("abc", 30.0);
        spaced.apply_style(0..3, |s| s.spacing_pt = 5.0);
        let wide = glyphs(&e.layout_body(&spaced, 500.0));
        assert!((wide[2].0 - plain[2].0 - 10.0).abs() < 0.5, "{plain:?} {wide:?}");
    }
}
