//! 頁首及頁尾: the date, footer and slide number placeholders of slides.

use std::sync::Arc;

use ppt_model::{Field, Paragraph, Run, Shape, Slide, TextBody};
use ppt_text::TextEngine;

use crate::Editor;

/// How the date shows.
#[derive(Clone, Debug, PartialEq)]
pub enum DateText {
    /// Today's date as a field PowerPoint updates when the file is opened.
    Auto(String),
    Fixed(String),
}

/// Which footer items slides show.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeaderFooter {
    pub date: Option<DateText>,
    pub slide_number: bool,
    pub footer: Option<String>,
    /// Leave title slides without them.
    pub skip_title: bool,
}

/// Field type of a date that updates itself (PowerPoint's default format).
const DATE_FIELD: &str = "datetime1";

fn kind_of(s: &Shape) -> Option<&str> {
    s.placeholder.as_ref().map(|p| p.kind.as_str()).filter(|k| matches!(*k, "dt" | "ftr" | "sldNum"))
}

/// Replaces a body's text with one run (a field when `field` is set), keeping
/// the formatting of its first run or paragraph mark.
fn set_text(body: &mut TextBody, text: &str, field: Option<&str>) {
    let first = body.paragraphs.first().cloned().unwrap_or_default();
    let mut style = first.runs.first().map_or_else(|| first.end_style.clone(), |r| r.style.clone());
    style.field = field.map(|k| Arc::new(Field::new(k)));
    style.link = None;
    let runs = if text.is_empty() { Vec::new() } else { vec![Run { text: text.to_string(), style }] };
    body.paragraphs = vec![Paragraph { runs, ..first }];
}

fn is_title_slide(slide: &Slide, layouts: &[ppt_model::Layout]) -> bool {
    let title = |s: &Shape| s.placeholder.as_ref().is_some_and(|p| p.kind == "ctrTitle");
    slide.shapes.iter().any(title) || slide.layout.and_then(|i| layouts.get(i)).is_some_and(|l| l.placeholders.iter().any(title))
}

impl Editor {
    /// What the current slide shows in its footer.
    pub fn header_footer(&self) -> HeaderFooter {
        let Some(slide) = self.pres.slides.get(self.slide) else { return HeaderFooter::default() };
        let find = |k: &str| slide.shapes.iter().find(|s| kind_of(s) == Some(k));
        let date = find("dt").and_then(|s| s.text.as_ref()).map(|t| {
            let auto = t.paragraphs.iter().flat_map(|p| &p.runs).any(|r| r.style.field.is_some());
            if auto { DateText::Auto(t.plain_text()) } else { DateText::Fixed(t.plain_text()) }
        });
        HeaderFooter { date, slide_number: find("sldNum").is_some(), footer: find("ftr").and_then(|s| s.text.as_ref()).map(|t| t.plain_text()), skip_title: false }
    }

    /// Shows or hides the date, footer and slide number on the current slide
    /// (or every slide with `all`), adding the layouts' placeholders as needed.
    pub fn set_header_footer(&mut self, engine: &mut TextEngine, hf: &HeaderFooter, all: bool) -> bool {
        self.end_text_edit(engine);
        let targets: Vec<usize> = if all { (0..self.pres.slides.len()).collect() } else { vec![self.slide] };
        let before = self.snapshot();
        let mut changed = false;
        for i in targets {
            let layouts = &self.pres.layouts;
            let slide = &self.pres.slides[i];
            let skip = hf.skip_title && is_title_slide(slide, layouts);
            let prototypes: Vec<Shape> = slide.layout.and_then(|l| layouts.get(l)).map(|l| l.footers.clone()).unwrap_or_default();
            let number = (i + 1).to_string();
            for kind in ["dt", "ftr", "sldNum"] {
                let content: Option<(String, Option<&str>)> = if skip {
                    None
                } else {
                    match kind {
                        "dt" => hf.date.as_ref().map(|d| match d {
                            DateText::Auto(t) => (t.clone(), Some(DATE_FIELD)),
                            DateText::Fixed(t) => (t.clone(), None),
                        }),
                        "ftr" => hf.footer.clone().map(|t| (t, None)),
                        _ => hf.slide_number.then(|| (number.clone(), Some(Field::SLIDE_NUMBER))),
                    }
                };
                // Look first: taking the slide for writing copies it when an undo snapshot shares it.
                let existing = self.pres.slides[i].shapes.iter().position(|s| kind_of(s) == Some(kind));
                match (content, existing) {
                    (None, Some(at)) => {
                        self.pres.slide_mut(i).expect("i is a slide index").shapes.remove(at);
                        changed = true;
                    }
                    (Some((text, field)), Some(at)) => {
                        if let Some(body) = &mut self.pres.slide_mut(i).expect("i is a slide index").shapes[at].text {
                            let old = body.clone();
                            set_text(body, &text, field);
                            // Same text and kind of field: leave the slide as it was.
                            let same = old.plain_text() == body.plain_text() && old.paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.style.field.as_ref().map(|f| f.kind.clone())).eq(body.paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.style.field.as_ref().map(|f| f.kind.clone())));
                            if same {
                                *body = old;
                            } else {
                                changed = true;
                            }
                        }
                    }
                    (Some((text, field)), None) => {
                        let Some(proto) = prototypes.iter().find(|s| kind_of(s) == Some(kind)) else { continue };
                        let mut shape = Shape { id: self.pres.alloc_id(), source: None, ..proto.clone() };
                        let body = shape.text.get_or_insert_with(TextBody::default);
                        set_text(body, &text, field);
                        self.pres.slide_mut(i).expect("i is a slide index").shapes.push(shape);
                        changed = true;
                    }
                    (None, None) => {}
                }
            }
        }
        if changed {
            self.selection.clear();
            self.push_undo(before);
        }
        changed
    }
}
