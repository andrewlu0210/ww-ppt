//! What slide drawing needs from a backend: the GPU scene for the screen, or
//! a PDF page for export.

use std::ops::Range;

use vello::Scene;
use vello::kurbo::{Affine, BezPath, Rect, Shape, Stroke};
use vello::peniko::{BrushRef, Color as PColor, Fill, FontData, ImageBrush, ImageData, StyleRef};

/// A run of glyphs in one font, size and color.
pub struct GlyphRun<'a> {
    pub font: &'a FontData,
    /// Points.
    pub size: f32,
    pub coords: &'a [i16],
    /// Maps the glyph positions (points) to device space.
    pub transform: Affine,
    /// Applied in font space (y up), e.g. a skew for synthetic italics.
    pub glyph_transform: Option<Affine>,
    pub color: PColor,
    pub glyphs: &'a [vello::Glyph],
    /// Outline width (points) to thicken the glyphs with, for synthetic bold.
    pub embolden: Option<f32>,
    /// The text the glyphs show, and each glyph's byte range in it (for
    /// selectable text in exports).
    pub text: &'a str,
    pub ranges: &'a [Range<usize>],
}

/// A drawing target. Paths are in the space `tf` maps to device space.
/// Structure hints for canvases that keep it (tagged PDF); the screen ignores them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A new paragraph of the text being drawn starts.
    Paragraph,
    /// What follows is decoration (bullet labels) until [`Mark::EndDecoration`].
    Decoration,
    EndDecoration,
}

pub trait Canvas {
    /// A structure hint; drawing is unaffected.
    fn mark(&mut self, _mark: Mark) {}

    fn fill_path(&mut self, rule: Fill, tf: Affine, brush: BrushRef<'_>, brush_tf: Option<Affine>, path: &BezPath);
    fn stroke_path(&mut self, style: &Stroke, tf: Affine, brush: BrushRef<'_>, path: &BezPath);
    fn push_clip_path(&mut self, rule: Fill, tf: Affine, path: &BezPath);
    /// Ends the innermost clip.
    fn pop_layer(&mut self);
    /// An image whose pixels span `0..width` × `0..height`; `encoded` is the
    /// original file (JPEG, PNG, ...) when there is one.
    fn draw_image_data(&mut self, image: &ImageData, encoded: Option<&[u8]>, tf: Affine);
    fn draw_blurred_rounded_rect(&mut self, tf: Affine, rect: Rect, color: PColor, radius: f64, std_dev: f64);
    fn draw_glyph_run(&mut self, run: &GlyphRun);
}

/// Shape-taking conveniences over [`Canvas`], named like vello's.
pub trait CanvasExt: Canvas {
    fn fill<'b>(&mut self, rule: Fill, tf: Affine, brush: impl Into<BrushRef<'b>>, brush_tf: Option<Affine>, shape: &impl Shape) {
        self.fill_path(rule, tf, brush.into(), brush_tf, &shape.to_path(0.1));
    }

    fn stroke<'b>(&mut self, style: &Stroke, tf: Affine, brush: impl Into<BrushRef<'b>>, _brush_tf: Option<Affine>, shape: &impl Shape) {
        self.stroke_path(style, tf, brush.into(), &shape.to_path(0.1));
    }

    fn push_clip_layer(&mut self, rule: Fill, tf: Affine, shape: &impl Shape) {
        self.push_clip_path(rule, tf, &shape.to_path(0.1));
    }
}

impl<C: Canvas + ?Sized> CanvasExt for C {}

impl Canvas for Scene {
    fn fill_path(&mut self, rule: Fill, tf: Affine, brush: BrushRef<'_>, brush_tf: Option<Affine>, path: &BezPath) {
        Scene::fill(self, rule, tf, brush, brush_tf, path);
    }

    fn stroke_path(&mut self, style: &Stroke, tf: Affine, brush: BrushRef<'_>, path: &BezPath) {
        Scene::stroke(self, style, tf, brush, None, path);
    }

    fn push_clip_path(&mut self, rule: Fill, tf: Affine, path: &BezPath) {
        Scene::push_clip_layer(self, rule, tf, path);
    }

    fn pop_layer(&mut self) {
        Scene::pop_layer(self);
    }

    fn draw_image_data(&mut self, image: &ImageData, _encoded: Option<&[u8]>, tf: Affine) {
        Scene::draw_image(self, &ImageBrush::new(image.clone()), tf);
    }

    fn draw_blurred_rounded_rect(&mut self, tf: Affine, rect: Rect, color: PColor, radius: f64, std_dev: f64) {
        Scene::draw_blurred_rounded_rect(self, tf, rect, color, radius, std_dev);
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun) {
        let mut draw = |style: StyleRef| {
            self.draw_glyphs(run.font)
                .brush(run.color)
                .hint(false)
                .transform(run.transform)
                .glyph_transform(run.glyph_transform)
                .font_size(run.size)
                .normalized_coords(run.coords)
                .draw(style, run.glyphs.iter().copied());
        };
        draw(Fill::NonZero.into());
        if let Some(w) = run.embolden {
            draw((&Stroke::new(w as f64)).into());
        }
    }
}
