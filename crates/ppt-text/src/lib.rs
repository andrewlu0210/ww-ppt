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
    Alignment, AlignmentOptions, BreakReason, FontContext, FontFamily, FontFamilyName, FontStyle, FontWeight, Layout,
    LayoutContext, LineHeight, StyleProperty, YieldData,
};
use ppt_model::{Align, Color, EMU_PER_PT, Emu, RunStyle, Table, TextBody};

pub use parley;

/// Single line spacing in PowerPoint is about 1.2× the font size for typical fonts.
pub const SINGLE_LINE_HEIGHT: f32 = 1.2;

pub struct TextEngine {
    pub font_cx: FontContext,
    pub layout_cx: LayoutContext<Color>,
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
    pub fn run_properties(&self, style: &RunStyle, font_scale: f32) -> Vec<StyleProperty<'static, Color>> {
        vec![
            StyleProperty::FontFamily(self.font_stack(style)),
            StyleProperty::FontSize(style.size_pt * font_scale),
            StyleProperty::FontWeight(if style.bold { FontWeight::BOLD } else { FontWeight::NORMAL }),
            StyleProperty::FontStyle(if style.italic { FontStyle::Italic } else { FontStyle::Normal }),
            StyleProperty::Underline(style.underline),
            StyleProperty::Brush(style.color),
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
        let default_props = self.run_properties(&first_style, scale);
        let mut span_props: Vec<(Range<usize>, Vec<StyleProperty<'static, Color>>)> = Vec::new();
        for (i, (p, range)) in body.paragraphs.iter().zip(&ranges).enumerate() {
            let line_height = SINGLE_LINE_HEIGHT * p.line_spacing * (1.0 - body.line_spacing_reduction);
            // The paragraph's `\n` (if any) carries its end style so empty lines get the right height.
            let para_end = if i + 1 < ranges.len() { range.end + 1 } else { range.end };
            let mut props = self.run_properties(&p.end_style, scale);
            props.push(StyleProperty::LineHeight(LineHeight::FontSizeRelative(line_height)));
            span_props.push((range.start..para_end.max(range.start), props));
            let mut pos = range.start;
            for run in &p.runs {
                let r = pos..pos + run.text.len();
                pos = r.end;
                span_props.push((r, self.run_properties(&run.style, scale)));
            }
        }

        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, &text, 1.0, true);
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(SINGLE_LINE_HEIGHT)));
        for prop in default_props {
            builder.push_default(prop);
        }
        for (range, props) in span_props {
            if range.is_empty() {
                continue;
            }
            for prop in props {
                builder.push(prop, range.clone());
            }
        }
        let mut layout: Layout<Color> = builder.build(&text);

        // Bullets are shaped separately and drawn in the hanging indent.
        let bullets_shaped: Vec<Option<Layout<Color>>> = body
            .paragraphs
            .iter()
            .zip(&ranges)
            .map(|(p, r)| match p.bullet {
                Some(c) if !r.is_empty() => Some(self.layout_bullet(c, p.first_style(), scale)),
                _ => None,
            })
            .collect();

        // Mandatory breaks in text order: `\n` ends a paragraph, U+2028 is a soft line break.
        let mut breaks = text.chars().filter(|c| *c == '\n' || *c == '\u{2028}');
        let max = if body.wrap { width_pt.max(1.0) } else { f32::MAX };
        let to_pt = |emu: i64| (emu as f64 / EMU_PER_PT) as f32;
        let mut lines = Vec::new();
        {
            let mut breaker = layout.break_lines();
            breaker.state_mut().set_layout_max_advance(max);
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
                    if let Some(Some(b)) = bullets_shaped.get(para) {
                        x = (x + b.width() + BULLET_GAP).max(to_pt(p.mar_l));
                    }
                }
                let x = x.clamp(0.0, (width_pt - 1.0).max(0.0));
                breaker.state_mut().set_line_x(x);
                breaker.state_mut().set_line_max_advance(if body.wrap { (max - x).max(1.0) } else { f32::MAX });
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

        let mut bullets = Vec::new();
        for (line, info) in layout.lines().zip(&lines) {
            if !info.first {
                continue;
            }
            let p = &body.paragraphs[info.paragraph];
            if let Some(Some(b)) = bullets_shaped.get(info.paragraph) {
                let baseline = b.lines().next().map(|l| l.metrics().baseline).unwrap_or(0.0);
                // Left-aligned bullets sit at the hanging indent; otherwise just before the text.
                let text_start = line.metrics().inline_min_coord + line.metrics().offset;
                let x = match p.align {
                    Align::Left | Align::Justify => to_pt(p.mar_l + p.indent).max(0.0),
                    _ => text_start - b.width() - BULLET_GAP,
                };
                bullets.push(Bullet { layout: b.clone(), x, y: line.metrics().baseline - baseline });
            }
        }
        let height = layout.lines().last().map(|l| l.metrics().block_max_coord).unwrap_or(0.0);
        BodyLayout { layout, lines, bullets, height }
    }

    fn layout_bullet(&mut self, bullet: char, style: &RunStyle, scale: f32) -> Layout<Color> {
        let text = bullet.to_string();
        let props = self.run_properties(style, scale);
        let mut builder = self.layout_cx.ranged_builder(&mut self.font_cx, &text, 1.0, true);
        for prop in props {
            builder.push_default(prop);
        }
        builder.push_default(StyleProperty::Underline(false));
        let mut layout = builder.build(&text);
        layout.break_all_lines(None);
        layout
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

/// Gap between a bullet and the text after it, in points.
const BULLET_GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineInfo {
    pub paragraph: usize,
    /// First line of its paragraph.
    pub first: bool,
}

#[derive(Clone)]
pub struct Bullet {
    pub layout: Layout<Color>,
    /// Offset of the bullet layout's origin in body layout space.
    pub x: f32,
    pub y: f32,
}

#[derive(Clone)]
pub struct BodyLayout {
    /// Every paragraph in one layout; coordinates are points.
    pub layout: Layout<Color>,
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
        p.bullet = Some('•');
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
}

