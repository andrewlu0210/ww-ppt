//! PDF export: slides drawn through the same renderer as the screen, onto
//! krilla pages. Shapes stay vector, text stays text (embedded, subset fonts
//! with its source characters, so it can be searched and copied), and
//! pictures embed their original JPEG or PNG data.

use std::collections::HashMap;
use std::sync::Arc;

use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Point, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, LinearGradient, Paint, RadialGradient, SpreadMethod, Stop, Stroke, StrokeDash};
use krilla::page::PageSettings;
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::{Data, Document};
use ppt_model::{EMU_PER_PT, Presentation};
use ppt_render::vello::kurbo::{self, Affine, BezPath, PathEl, Rect, Shape as _};
use ppt_render::vello::peniko::{self, BrushRef, Color as PColor, FontData, ImageData};
use ppt_render::{Canvas, DrawOptions, GlyphRun, RenderCtx, draw_slide};

/// What to export.
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfOptions {
    /// Include slides hidden from the slide show.
    pub include_hidden: bool,
}

/// The deck as a PDF, one page per slide at the slide's size.
pub fn export_pdf(pres: &Presentation, cx: &mut RenderCtx, opts: PdfOptions) -> Result<Vec<u8>, String> {
    let (w, h) = ((pres.slide_width as f64 / EMU_PER_PT) as f32, (pres.slide_height as f64 / EMU_PER_PT) as f32);
    let settings = || PageSettings::from_wh(w, h).ok_or("invalid slide size");
    let mut doc = Document::new();
    let mut fonts = HashMap::new();
    let view = Affine::scale(1.0 / EMU_PER_PT);
    // Slide units per output "pixel": hairlines drawn by the renderer come out at a quarter point.
    let draw = DrawOptions { emu_per_px: EMU_PER_PT / 4.0, ..Default::default() };
    let mut pages = 0;
    for slide in pres.slides.iter().filter(|s| opts.include_hidden || !s.hidden) {
        let mut page = doc.start_page_with(settings()?);
        let mut surface = page.surface();
        let mut canvas = PdfCanvas { surface: &mut surface, fonts: &mut fonts, clips: Vec::new() };
        draw_slide(&mut canvas, cx, pres, slide, view, &draw);
        canvas.close_clips();
        surface.finish();
        page.finish();
        pages += 1;
    }
    if pages == 0 {
        // A PDF needs a page: an empty one when every slide is hidden.
        doc.start_page_with(settings()?).finish();
    }
    doc.finish().map_err(|e| format!("{e:?}"))
}

struct PdfCanvas<'a, 'b> {
    surface: &'a mut Surface<'b>,
    /// Fonts by (blob id, collection index); `None` when krilla can't read one.
    fonts: &'a mut HashMap<(u64, u32), Option<Font>>,
    /// Surface pushes per open clip layer.
    clips: Vec<usize>,
}

fn transform(a: Affine) -> Transform {
    let [sx, ky, kx, sy, tx, ty] = a.as_coeffs().map(|v| v as f32);
    Transform::from_row(sx, ky, kx, sy, tx, ty)
}

fn path(p: &BezPath) -> Option<Path> {
    let mut b = PathBuilder::new();
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(p) => b.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => b.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(a, p) => b.quad_to(a.x as f32, a.y as f32, p.x as f32, p.y as f32),
            PathEl::CurveTo(a, c, p) => b.cubic_to(a.x as f32, a.y as f32, c.x as f32, c.y as f32, p.x as f32, p.y as f32),
            PathEl::ClosePath => b.close(),
        }
    }
    b.finish()
}

fn rule(r: peniko::Fill) -> FillRule {
    match r {
        peniko::Fill::NonZero => FillRule::NonZero,
        peniko::Fill::EvenOdd => FillRule::EvenOdd,
    }
}

fn opacity(a: f32) -> NormalizedF32 {
    NormalizedF32::new(a.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE)
}

/// RGB and alpha of a color.
fn rgba(c: PColor) -> (rgb::Color, f32) {
    let [r, g, b, a] = c.to_rgba8().to_u8_array();
    (rgb::Color::new(r, g, b), a as f32 / 255.0)
}

/// A brush as a krilla paint and the opacity to draw it with.
fn paint(brush: BrushRef, brush_tf: Option<Affine>) -> Option<(Paint, f32)> {
    match brush {
        BrushRef::Solid(c) => {
            let (color, a) = rgba(c);
            Some((color.into(), a))
        }
        BrushRef::Gradient(g) => {
            let stops: Vec<Stop> = g
                .stops
                .iter()
                .map(|s| {
                    let (color, a) = rgba(s.color.to_alpha_color());
                    Stop { offset: opacity(s.offset), color: color.into(), opacity: opacity(a) }
                })
                .collect();
            let tf = transform(brush_tf.unwrap_or(Affine::IDENTITY));
            let p: Paint = match g.kind {
                peniko::GradientKind::Linear(l) => LinearGradient {
                    x1: l.start.x as f32,
                    y1: l.start.y as f32,
                    x2: l.end.x as f32,
                    y2: l.end.y as f32,
                    transform: tf,
                    spread_method: SpreadMethod::Pad,
                    stops,
                    anti_alias: true,
                }
                .into(),
                peniko::GradientKind::Radial(r) => RadialGradient {
                    fx: r.start_center.x as f32,
                    fy: r.start_center.y as f32,
                    fr: r.start_radius,
                    cx: r.end_center.x as f32,
                    cy: r.end_center.y as f32,
                    cr: r.end_radius,
                    transform: tf,
                    spread_method: SpreadMethod::Pad,
                    stops,
                    anti_alias: true,
                }
                .into(),
                _ => return None,
            };
            Some((p, 1.0))
        }
        _ => None,
    }
}

impl PdfCanvas<'_, '_> {
    fn close_clips(&mut self) {
        while !self.clips.is_empty() {
            self.pop_layer();
        }
    }

    fn font(&mut self, data: &FontData) -> Option<Font> {
        self.fonts
            .entry((data.data.id(), data.index))
            .or_insert_with(|| {
                let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(data.data.clone());
                Font::new(Data::from(bytes), data.index)
            })
            .clone()
    }
}

impl Canvas for PdfCanvas<'_, '_> {
    fn fill_path(&mut self, r: peniko::Fill, tf: Affine, brush: BrushRef<'_>, brush_tf: Option<Affine>, p: &BezPath) {
        // Mapped to page space here rather than through the page transform: slide units are tiny
        // in points, and gradient matrices that small get rounded to nothing in the file.
        let mut p = p.clone();
        p.apply_affine(tf);
        let (Some((paint, a)), Some(p)) = (paint(brush, Some(tf * brush_tf.unwrap_or(Affine::IDENTITY))), path(&p)) else { return };
        self.surface.set_stroke(None);
        self.surface.set_fill(Some(Fill { paint, opacity: opacity(a), rule: rule(r) }));
        self.surface.draw_path(&p);
    }

    fn stroke_path(&mut self, style: &kurbo::Stroke, tf: Affine, brush: BrushRef<'_>, p: &BezPath) {
        let (Some((paint, a)), Some(p)) = (paint(brush, None), path(p)) else { return };
        let dash = (!style.dash_pattern.is_empty()).then(|| StrokeDash { array: style.dash_pattern.iter().map(|d| *d as f32).collect(), offset: style.dash_offset as f32 });
        let line_cap = match style.start_cap {
            kurbo::Cap::Butt => LineCap::Butt,
            kurbo::Cap::Round => LineCap::Round,
            kurbo::Cap::Square => LineCap::Square,
        };
        let line_join = match style.join {
            kurbo::Join::Bevel => LineJoin::Bevel,
            kurbo::Join::Miter => LineJoin::Miter,
            kurbo::Join::Round => LineJoin::Round,
        };
        let stroke = Stroke { paint, width: style.width as f32, miter_limit: style.miter_limit as f32, line_cap, line_join, opacity: opacity(a), dash };
        self.surface.push_transform(&transform(tf));
        self.surface.set_fill(None);
        self.surface.set_stroke(Some(stroke));
        self.surface.draw_path(&p);
        self.surface.set_stroke(None);
        self.surface.pop();
    }

    fn push_clip_path(&mut self, r: peniko::Fill, tf: Affine, p: &BezPath) {
        // In page space: the clip has to outlive this call's transform.
        let mut p = p.clone();
        p.apply_affine(tf);
        match path(&p) {
            Some(p) => {
                self.surface.push_clip_path(&p, &rule(r));
                self.clips.push(1);
            }
            None => self.clips.push(0),
        }
    }

    fn pop_layer(&mut self) {
        for _ in 0..self.clips.pop().unwrap_or(0) {
            self.surface.pop();
        }
    }

    fn draw_image_data(&mut self, image: &ImageData, encoded: Option<&[u8]>, tf: Affine) {
        let original = encoded.and_then(|bytes| {
            let data = || Data::from(bytes.to_vec());
            match bytes {
                [0xFF, 0xD8, ..] => Image::from_jpeg(data(), true).ok(),
                [0x89, b'P', b'N', b'G', ..] => Image::from_png(data(), true).ok(),
                _ => None,
            }
        });
        let img = original.unwrap_or_else(|| Image::from_rgba8(image.data.data().to_vec(), image.width, image.height));
        let Some(size) = Size::from_wh(image.width as f32, image.height as f32) else { return };
        self.surface.push_transform(&transform(tf));
        self.surface.draw_image(img, size);
        self.surface.pop();
    }

    fn draw_blurred_rounded_rect(&mut self, tf: Affine, rect: Rect, color: PColor, radius: f64, std_dev: f64) {
        // A soft edge from a few translucent layers instead of a real blur.
        let (rgb, a) = rgba(color);
        const LAYERS: usize = 4;
        for i in 0..LAYERS {
            let grow = std_dev * (1.5 - i as f64);
            let r = rect.inflate(grow, grow);
            if r.width() <= 0.0 || r.height() <= 0.0 {
                continue;
            }
            let shape = r.to_rounded_rect((radius + grow).max(0.0)).to_path(0.1);
            let Some(p) = path(&shape) else { continue };
            self.surface.push_transform(&transform(tf));
            self.surface.set_stroke(None);
            self.surface.set_fill(Some(Fill { paint: rgb.into(), opacity: opacity(a / LAYERS as f32), rule: FillRule::NonZero }));
            self.surface.draw_path(&p);
            self.surface.pop();
        }
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun) {
        let Some(first) = run.glyphs.first() else { return };
        let Some(font) = self.font(run.font) else { return };
        let size = run.size.max(0.01);
        let (x0, y0) = (first.x, first.y);
        let glyphs: Vec<KrillaGlyph> = run
            .glyphs
            .iter()
            .enumerate()
            .map(|(i, g)| {
                // Each glyph sits where the next one's advance ends; offsets carry vertical shifts.
                let next = run.glyphs.get(i + 1).map_or(g.x, |n| n.x);
                KrillaGlyph {
                    glyph_id: GlyphId::new(g.id),
                    text_range: run.ranges.get(i).cloned().unwrap_or(0..0),
                    x_advance: (next - g.x) / size,
                    x_offset: 0.0,
                    y_offset: -(g.y - y0) / size,
                    y_advance: 0.0,
                    location: None,
                }
            })
            .collect();
        let (rgb, a) = rgba(run.color);
        // Synthetic italics lean the glyphs about the baseline (font space is y up).
        let lean = match run.glyph_transform {
            Some(g) => {
                let [_, _, kx, ..] = g.as_coeffs();
                Affine::translate((x0 as f64, y0 as f64)) * Affine::skew(-kx, 0.0) * Affine::translate((-(x0 as f64), -(y0 as f64)))
            }
            None => Affine::IDENTITY,
        };
        self.surface.push_transform(&transform(run.transform * lean));
        self.surface.set_fill(Some(Fill { paint: rgb.into(), opacity: opacity(a), rule: FillRule::NonZero }));
        self.surface.set_stroke(run.embolden.map(|w| Stroke { paint: rgb.into(), width: w, opacity: opacity(a), ..Default::default() }));
        self.surface.draw_glyphs(Point::from_xy(x0, y0), &glyphs, font, run.text, size, false);
        self.surface.set_stroke(None);
        self.surface.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppt_render::ImageCache;
    use ppt_text::TextEngine;

    #[test]
    fn one_page_per_shown_slide_with_embedded_text() {
        let mut pres = ppt_ooxml::parse_pptx(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap()).unwrap();
        pres.slides[1].hidden = true;
        let mut engine = TextEngine::new(false);
        engine.register_font(std::fs::read("/System/Library/Fonts/Helvetica.ttc").unwrap());
        engine.set_fallbacks(vec!["Helvetica".into()]);
        let mut images = ImageCache::default();
        let mut cx = RenderCtx { engine: &mut engine, images: &mut images };
        let pdf = export_pdf(&pres, &mut cx, PdfOptions::default()).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(text.contains(&format!("/Type/Pages/Count {}", pres.slides.len() - 1)), "hidden slides are left out");
        assert!(text.contains("/FontFile2") || text.contains("/FontFile3"), "fonts are embedded");
        let all = export_pdf(&pres, &mut cx, PdfOptions { include_hidden: true }).unwrap();
        assert!(all.len() > pdf.len());
    }
}
