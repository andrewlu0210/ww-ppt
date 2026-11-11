//! Format painter: copies the formatting of the selected text or shape and
//! applies it to other text or shapes in one undo step.

use ppt_model::{Align, Bullet, Emu, Line, Paint, RunStyle, Shadow};
use ppt_text::TextEngine;

use crate::Editor;

/// Paragraph settings that travel with the format.
#[derive(Clone, Debug)]
struct ParaFormat {
    align: Align,
    level: u8,
    bullet: Option<Bullet>,
    line_spacing: f32,
    mar_l: Emu,
    indent: Emu,
    space_before: f32,
    space_after: f32,
}

/// Outline, fill and shadow of a shape.
#[derive(Clone, Debug)]
struct ShapeFormat {
    fill: Option<Paint>,
    line: Option<Line>,
    shadow: Option<Shadow>,
}

/// What the format painter picked up.
#[derive(Clone, Debug)]
pub struct CopiedFormat {
    text: Option<(RunStyle, ParaFormat)>,
    /// Only set when a whole shape was the source (not a text selection).
    shape: Option<ShapeFormat>,
}

impl CopiedFormat {
    fn style_fn(&self) -> Option<impl Fn(&mut RunStyle) + '_> {
        let (style, _) = self.text.as_ref()?;
        // Links and fields belong to the text itself, not to its look.
        Some(move |s: &mut RunStyle| {
            let (link, field) = (s.link.take(), s.field.take());
            *s = style.clone();
            s.link = link;
            s.field = field;
        })
    }

    fn para_fn(&self) -> Option<impl Fn(&mut ppt_model::Paragraph) + '_> {
        let (_, para) = self.text.as_ref()?;
        Some(move |p: &mut ppt_model::Paragraph| {
            p.align = para.align;
            p.level = para.level;
            p.bullet = para.bullet.clone();
            p.line_spacing = para.line_spacing;
            p.mar_l = para.mar_l;
            p.indent = para.indent;
            p.space_before = para.space_before;
            p.space_after = para.space_after;
        })
    }
}

fn para_format(p: &ppt_model::Paragraph) -> ParaFormat {
    ParaFormat { align: p.align, level: p.level, bullet: p.bullet.clone(), line_spacing: p.line_spacing, mar_l: p.mar_l, indent: p.indent, space_before: p.space_before, space_after: p.space_after }
}

impl Editor {
    /// Remembers the format at the caret / text selection, or of the selected shape.
    pub fn copy_format(&mut self) -> bool {
        let copied = if let Some(edit) = &self.text {
            let mut style = edit.rich.typing_style();
            style.link = None;
            CopiedFormat { text: Some((style, para_format(edit.rich.current_paragraph()))), shape: None }
        } else if let Some(shape) = self.selected_shapes().first() {
            let text = shape.text.as_ref().and_then(|b| {
                let p = b.paragraphs.first()?;
                let mut style = b.style_at(0);
                style.link = None;
                Some((style, para_format(p)))
            });
            CopiedFormat { text, shape: Some(ShapeFormat { fill: shape.fill.clone(), line: shape.line, shadow: shape.shadow }) }
        } else {
            return false;
        };
        self.copied_format = Some(copied);
        true
    }

    pub fn has_copied_format(&self) -> bool {
        self.copied_format.is_some()
    }

    /// Applies the remembered format to the text selection, or to the selected shapes
    /// (their text and, if a shape was copied, outline, fill and shadow). Does nothing
    /// and returns false when there is no target yet, such as a caret without a selection.
    pub fn paste_format(&mut self, engine: &mut TextEngine) -> bool {
        let Some(copied) = self.copied_format.clone() else { return false };
        if let Some(edit) = &mut self.text {
            if edit.rich.is_collapsed() {
                return false;
            }
            let (Some(style), Some(para)) = (copied.style_fn(), copied.para_fn()) else { return false };
            edit.rich.edit_body(engine, |body, range| {
                body.apply_style(range.clone(), &style);
                body.apply_paragraphs(range, &para);
            });
            self.fit_edited_shape(engine);
            return true;
        }
        if self.cell_range.is_some() {
            let (Some(style), Some(para)) = (copied.style_fn(), copied.para_fn()) else { return false };
            let styled = self.style_cells(engine, &style);
            return self.paragraph_cells(engine, &para) || styled;
        }
        let targets = self.selected_shapes();
        let has_text = copied.text.is_some() && targets.iter().any(|s| s.text_bodies().next().is_some());
        if targets.is_empty() || (!has_text && copied.shape.is_none()) {
            return false;
        }
        self.modify_selected(|shape| {
            if let Some(f) = &copied.shape {
                if !shape.geometry.is_line() && matches!(shape.kind, ppt_model::ShapeKind::Shape) {
                    shape.fill = f.fill.clone();
                }
                shape.line = f.line;
                shape.shadow = f.shadow;
            }
            if let (Some(style), Some(para)) = (copied.style_fn(), copied.para_fn()) {
                for body in shape.text_bodies_mut() {
                    let len = body.len();
                    body.apply_style(0..len, &style);
                    body.apply_paragraphs(0..len, &para);
                }
            }
        })
    }
}
