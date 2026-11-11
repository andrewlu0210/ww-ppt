//! 設定圖形格式 › 大小與屬性: "Lock aspect ratio" and the text box settings (vertical alignment,
//! margins, word wrap, resize shape to fit text) of the selected shapes.

use ppt_geometry::{TextFrame, text_width_pt};
use ppt_model::{Anchor, EMU_PER_PT, Shape, ShapeKind};
use ppt_text::{SINGLE_LINE_HEIGHT, TextEngine};

use crate::Editor;

/// Changes to the text box settings; what is `None` stays as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextBoxSpec {
    pub anchor: Option<Anchor>,
    /// Left, top, right, bottom margins in EMU.
    pub insets: [Option<i64>; 4],
    pub wrap: Option<bool>,
    /// The shape's height follows its text (`spAutoFit`).
    pub auto_grow: Option<bool>,
}

/// Shapes with a text body of their own (not table cells or SmartArt text).
fn has_text_box(s: &Shape) -> bool {
    matches!(s.kind, ShapeKind::Shape) && s.text.is_some()
}

/// Sets the height of an auto-fit shape to its text (as typing in it would).
fn fit_height(engine: &mut TextEngine, shape: &mut Shape) {
    let Some(body) = &shape.text else { return };
    if !body.auto_grow {
        return;
    }
    let frame = TextFrame::of(shape.xfrm, &shape.geometry);
    let layout = engine.layout_body(body, text_width_pt(&frame, body));
    let min_line = body.paragraphs.first().map(|p| p.first_style().size_pt * SINGLE_LINE_HEIGHT).unwrap_or(0.0);
    let content = layout.height.max(min_line) as f64;
    shape.xfrm.h = (content * EMU_PER_PT).round() as i64 + body.insets[1] + body.insets[3];
}

impl Editor {
    /// Turns "Lock aspect ratio" on or off for the selected shapes.
    pub fn set_lock_aspect(&mut self, on: bool) -> bool {
        self.edit_selected_if_changed(|s| s.access.lock_aspect = on)
    }

    /// Changes the text box settings of the selected shapes, as one undo step.
    pub fn set_text_box_spec(&mut self, engine: &mut TextEngine, spec: TextBoxSpec) -> bool {
        let apply = |s: &mut Shape, engine: Option<&mut TextEngine>| {
            let Some(body) = &mut s.text else { return };
            if let Some(a) = spec.anchor {
                body.anchor = a;
            }
            for (i, v) in spec.insets.iter().enumerate() {
                if let Some(v) = v {
                    body.insets[i] = (*v).max(0);
                }
            }
            if let Some(w) = spec.wrap {
                body.wrap = w;
            }
            if let Some(g) = spec.auto_grow {
                body.auto_grow = g;
            }
            if let Some(engine) = engine {
                fit_height(engine, s);
            }
        };
        let ids: Vec<_> = self
            .selected_shapes()
            .into_iter()
            .filter(|s| has_text_box(s))
            .filter(|s| {
                let mut copy = (*s).clone();
                apply(&mut copy, None);
                copy.text != s.text
            })
            .map(|s| s.id)
            .collect();
        if ids.is_empty() {
            return false;
        }
        self.end_text_edit(engine);
        let before = self.snapshot();
        let slide = self.slide;
        for id in ids {
            if let Some(shape) = self.pres.slide_mut(slide).and_then(|s| s.shapes.iter_mut().find(|s| s.id == id)) {
                apply(shape, Some(engine));
            }
        }
        self.push_undo(before);
        true
    }
}
