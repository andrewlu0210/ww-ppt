//! 檢查協助工具: problems that make the deck hard to use with assistive
//! technology, like PowerPoint's accessibility checker.

use ppt_model::{Background, Color, Paint, Shape, ShapeId, ShapeKind, Slide};

use crate::Editor;
use crate::a11y::slide_title;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Content that is very hard or impossible to understand.
    Error,
    /// Content that is hard to understand.
    Warning,
    /// Content that could be easier to understand.
    Tip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    MissingAltText,
    MissingTitle,
    DuplicateTitle,
    LowContrast,
    NoHeaderRow,
    ReadingOrder,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub slide: usize,
    pub shape: Option<ShapeId>,
    pub severity: Severity,
    pub kind: IssueKind,
    /// What and where, e.g. "圖片「標誌」".
    pub message: String,
}

/// WCAG relative luminance.
fn luminance(c: Color) -> f64 {
    let ch = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c.r) + 0.7152 * ch(c.g) + 0.0722 * ch(c.b)
}

/// WCAG contrast ratio, 1 to 21.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The color behind `shape`'s text: its own fill, the filled shapes below it
/// (and the layout's decorations) that cover its center, and the slide
/// background, composited for transparency. `None` when a picture shows
/// through: there is no single color to compare with.
fn backdrop(shapes: &[Shape], index: usize, decorations: &[Shape], slide_bg: &Background) -> Option<Color> {
    let paint_color = |p: &Paint| match p {
        Paint::Solid(c) => *c,
        // A gradient: its middle stop stands for it.
        Paint::Gradient(g) => g.stops.get(g.stops.len() / 2).map_or(Color::WHITE, |s| s.1),
    };
    let s = &shapes[index];
    let (cx, cy) = s.xfrm.center();
    let covers = |b: &Shape| {
        let (lx, ly) = b.xfrm.to_local(cx, cy);
        lx >= 0.0 && ly >= 0.0 && lx <= b.xfrm.w as f64 && ly <= b.xfrm.h as f64
    };
    // Layers from the top down, until one is opaque.
    let mut layers: Vec<Color> = Vec::new();
    let below = std::iter::once(s).chain(shapes[..index].iter().rev().filter(|b| covers(b))).chain(decorations.iter().rev().filter(|b| covers(b)));
    for b in below.filter(|b| !b.access.hidden) {
        if matches!(b.kind, ShapeKind::Picture { .. }) {
            return None;
        }
        if let Some(f) = &b.fill {
            let c = paint_color(f);
            layers.push(c);
            if c.a == 255 {
                break;
            }
        }
    }
    if layers.last().is_none_or(|c| c.a < 255) {
        match slide_bg {
            Background::Paint(p) => layers.push(Color { a: 255, ..paint_color(p) }),
            Background::Picture { .. } => return None,
        }
    }
    let mut out = *layers.last().expect("background added");
    for c in layers.iter().rev().skip(1) {
        let a = c.a as f64 / 255.0;
        let mix = |top: u8, bottom: u8| (top as f64 * a + bottom as f64 * (1.0 - a)).round() as u8;
        out = Color { r: mix(c.r, out.r), g: mix(c.g, out.g), b: mix(c.b, out.b), a: 255 };
    }
    Some(out)
}

/// Text that reads as large: 18pt, or 14pt bold.
fn large(size: f32, bold: bool) -> bool {
    size >= 18.0 || (bold && size >= 14.0)
}

impl Editor {
    /// Every accessibility problem in the deck, by slide, errors first within a slide.
    pub fn check_accessibility(&self) -> Vec<Issue> {
        let mut out = Vec::new();
        let titles: Vec<String> = self.pres.slides.iter().map(slide_title).collect();
        for (i, slide) in self.pres.slides.iter().enumerate() {
            let push = |out: &mut Vec<Issue>, shape: Option<ShapeId>, severity, kind, message: String| out.push(Issue { slide: i, shape, severity, kind, message });
            if titles[i].is_empty() {
                push(&mut out, None, Severity::Error, IssueKind::MissingTitle, format!("投影片 {}", i + 1));
            } else if let Some(first) = titles[..i].iter().position(|t| *t == titles[i]) {
                push(&mut out, None, Severity::Warning, IssueKind::DuplicateTitle, format!("投影片 {}：「{}」與投影片 {} 相同", i + 1, titles[i], first + 1));
            }
            let bg = self.pres.slide_background(slide);
            for (k, s) in slide.shapes.iter().enumerate().filter(|(_, s)| !s.access.hidden) {
                let named = |what: &str| format!("{what}「{}」", s.name);
                match &s.kind {
                    ShapeKind::Picture { .. } | ShapeKind::Chart(_) | ShapeKind::Diagram { .. } if s.access.alt_text.trim().is_empty() && !s.access.decorative => {
                        let what = match s.kind {
                            ShapeKind::Picture { .. } => "圖片",
                            ShapeKind::Chart(_) => "圖表",
                            _ => "SmartArt",
                        };
                        push(&mut out, Some(s.id), Severity::Error, IssueKind::MissingAltText, named(what));
                    }
                    ShapeKind::Table(t) if !t.flags.first_row => push(&mut out, Some(s.id), Severity::Warning, IssueKind::NoHeaderRow, named("表格")),
                    _ => {}
                }
                // Table and SmartArt text sits on their own cell and shape fills.
                let own_fills = matches!(s.kind, ShapeKind::Table(_) | ShapeKind::Diagram { .. } | ShapeKind::Chart(_));
                let decorations: &[Shape] = if slide.hide_background_graphics { &[] } else { &slide.background_shapes };
                if !own_fills && let Some(back) = backdrop(&slide.shapes, k, decorations, bg) {
                    let worst = s
                        .text_bodies()
                        .flat_map(|t| t.paragraphs.iter())
                        .flat_map(|p| p.runs.iter())
                        .filter(|r| !r.text.trim().is_empty() && r.style.color.a > 0)
                        .map(|r| {
                            let ratio = contrast(r.style.color, back);
                            let need = if large(r.style.size_pt, r.style.bold) { 3.0 } else { 4.5 };
                            (ratio, need)
                        })
                        .filter(|(ratio, need)| ratio < need)
                        .min_by(|a, b| a.0.total_cmp(&b.0));
                    if let Some((ratio, _)) = worst {
                        push(&mut out, Some(s.id), Severity::Warning, IssueKind::LowContrast, format!("{}，對比 {ratio:.1}:1", named("文字")));
                    }
                }
            }
            if let Some(message) = reading_order_issue(slide, i) {
                push(&mut out, None, Severity::Tip, IssueKind::ReadingOrder, message);
            }
        }
        out.sort_by_key(|i| (i.slide, i.severity));
        out
    }
}

/// A tip when the title is not read first, or text is read out of its
/// top-to-bottom, left-to-right order on the slide.
fn reading_order_issue(slide: &Slide, index: usize) -> Option<String> {
    let readable: Vec<&Shape> = slide.shapes.iter().filter(|s| !s.access.hidden && !s.access.decorative && s.text_bodies().any(|t| !t.is_empty())).collect();
    if readable.len() < 2 {
        return None;
    }
    let is_title = |s: &Shape| s.placeholder.as_ref().is_some_and(|p| matches!(p.kind.as_str(), "title" | "ctrTitle"));
    if readable.iter().position(|s| is_title(s)).is_some_and(|p| p > 0) {
        return Some(format!("投影片 {}：標題不是第一個被讀出的物件", index + 1));
    }
    // Rows by top edge (within a fifth of the shorter shape's height), then left to right.
    let mut visual = readable.clone();
    visual.sort_by(|a, b| {
        let tol = a.xfrm.h.min(b.xfrm.h) / 5;
        if (a.xfrm.y - b.xfrm.y).abs() <= tol { a.xfrm.x.cmp(&b.xfrm.x) } else { a.xfrm.y.cmp(&b.xfrm.y) }
    });
    let same = visual.iter().zip(&readable).all(|(a, b)| a.id == b.id);
    (!same).then(|| format!("投影片 {}：閱讀順序和畫面上的位置不一致", index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppt_model::{Placeholder, Presentation, TextBody, Xfrm};

    fn text(id: ShapeId, name: &str, s: &str, x: i64, y: i64) -> Shape {
        let mut shape = Shape::new(id, name, Xfrm::new(x, y, 1000, 200));
        let mut body = TextBody::default();
        body.set_plain_text(s);
        shape.text = Some(body);
        shape
    }

    #[test]
    fn finds_missing_titles_alt_text_contrast_and_order() {
        let mut pres = Presentation::default();
        let mut t = text(pres.alloc_id(), "Title 1", "營收", 0, 0);
        t.placeholder = Some(Placeholder { kind: "title".into(), idx: 0 });
        let mut pic = Shape::new(pres.alloc_id(), "Logo", Xfrm::new(0, 0, 10, 10));
        pic.kind = ShapeKind::Picture { media: None, crop: Default::default() };
        // Light gray 12pt text on white: 1.6:1.
        let mut pale = text(pres.alloc_id(), "Note", "備註", 0, 900);
        for p in &mut pale.text.as_mut().unwrap().paragraphs {
            for r in &mut p.runs {
                r.style.color = Color::rgb(0xCC, 0xCC, 0xCC);
                r.style.size_pt = 12.0;
            }
        }
        // Read before the text above it.
        let lower = text(pres.alloc_id(), "Lower", "下方", 0, 2000);
        let upper = text(pres.alloc_id(), "Upper", "上方", 0, 500);
        pres.slides.push(Slide { shapes: vec![t.clone(), pic, pale, lower, upper], background: Some(Background::WHITE), ..Default::default() });
        pres.slides.push(Slide { shapes: vec![t], background: Some(Background::WHITE), ..Default::default() });
        pres.slides.push(Slide { background: Some(Background::WHITE), ..Default::default() });
        let e = Editor::new(pres);
        let kinds: Vec<(usize, IssueKind)> = e.check_accessibility().iter().map(|i| (i.slide, i.kind)).collect();
        assert_eq!(
            kinds,
            vec![(0, IssueKind::MissingAltText), (0, IssueKind::LowContrast), (0, IssueKind::ReadingOrder), (1, IssueKind::DuplicateTitle), (2, IssueKind::MissingTitle)]
        );
        assert!((contrast(Color::BLACK, Color::WHITE) - 21.0).abs() < 1e-9);
    }
}
