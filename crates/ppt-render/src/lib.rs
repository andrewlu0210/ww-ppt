//! Slide rendering onto a [`Canvas`]: a vello [`Scene`] or a PDF page.
//!
//! Everything is drawn in slide space (EMU) and mapped to device pixels by a
//! single `view` transform, so the same code serves the editor canvas,
//! thumbnails and export.

use std::collections::HashMap;
use std::sync::Arc;

use parley::{Layout, PositionedLayoutItem};
use ppt_geometry::{Shade, TextFrame, outlines, shape_affine, text_affine, text_width_pt};
use ppt_model::table::CellLook;
use ppt_model::{Color, Gradient, GradientKind, Paint, Presentation, Shape, ShapeId, ShapeKind, Slide, Table, TextBody, Xfrm};
use ppt_text::{BodyLayout, TextEngine};
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Cap, Circle, Line, ParamCurve, PathEl, PathSeg, Point, Rect, Shape as _, Stroke, Vec2};
use vello::peniko::{Blob, Color as PColor, ColorStop, Fill, Gradient as PGradient, ImageAlphaType, ImageData, ImageFormat};

mod anim;
mod canvas;
mod chart;
pub use canvas::{Canvas, CanvasExt, GlyphRun};
pub use anim::{draw_slide_animated, reveal_mask};
mod transition;
pub use transition::draw_transition;

pub use vello;

pub fn color(c: Color) -> PColor {
    PColor::from_rgba8(c.r, c.g, c.b, c.a)
}

/// A shape's drop shadow: a blurred rounded rectangle for rectangular shapes
/// and pictures, and the outline itself with a soft edge for other shapes.
fn draw_shadow(scene: &mut dyn Canvas, shape: &Shape, shadow: &ppt_model::Shadow, view: Affine) {
    let xf = &shape.xfrm;
    let (w, h) = (xf.w as f64, xf.h as f64);
    // Shapes without any visible paint cast no shadow.
    let picture = matches!(shape.kind, ShapeKind::Picture { .. });
    if shape.fill.is_none() && !picture {
        return;
    }
    let (sin, cos) = shadow.dir.to_radians().sin_cos();
    let offset = Affine::translate((cos * shadow.dist as f64, sin * shadow.dist as f64));
    let radius = match &shape.geometry {
        ppt_model::Geometry::Preset { name, .. } if name == "rect" => Some(0.0),
        ppt_model::Geometry::Preset { name, adjust } if name == "roundRect" => Some(w.min(h) * adjust.iter().find(|(k, _)| k == "adj").map_or(16_667.0, |(_, v)| *v) / 100_000.0),
        _ if picture => Some(0.0),
        _ => None,
    };
    if let Some(radius) = radius {
        let tf = view * offset * shape_affine(xf, false);
        scene.draw_blurred_rounded_rect(tf, Rect::new(0.0, 0.0, w, h), color(shadow.color), radius, shadow.blur as f64 / 2.0);
        return;
    }
    let tf = view * offset * shape_affine(xf, true);
    let mut path = BezPath::new();
    for outline in outlines(&shape.geometry, w, h).into_iter().filter(|o| o.fill) {
        path.extend(outline.path);
    }
    if path.elements().is_empty() {
        return;
    }
    // Rings of decreasing opacity around a lighter core approximate the blur.
    let c = color(shadow.color);
    let a = c.components[3];
    let blur = shadow.blur as f64;
    const RINGS: usize = 3;
    for i in (1..=RINGS).rev() {
        let width = blur * i as f64 / RINGS as f64;
        if width > 0.0 {
            scene.stroke(&Stroke::new(width), tf, c.with_alpha(a * 0.12), None, &path);
        }
    }
    scene.fill(Fill::NonZero, tf, c.with_alpha(a * if blur > 0.0 { 0.6 } else { 1.0 }), None, &path);
}

/// Strokes `path` with the line's width and dash pattern, plus arrowheads on open paths.
pub fn stroke_line(scene: &mut dyn Canvas, tf: Affine, line: &ppt_model::Line, path: &BezPath) {
    let width = line.width.max(1) as f64;
    let mut stroke = Stroke::new(width).with_caps(Cap::Butt);
    let pattern = line.dash.pattern();
    if !pattern.is_empty() {
        stroke = stroke.with_dashes(0.0, pattern.iter().map(|d| d * width));
    }
    let brush = color(line.color);
    scene.stroke(&stroke, tf, brush, None, path);
    let open = !matches!(path.elements().last(), Some(PathEl::ClosePath));
    if !open {
        return;
    }
    let segments: Vec<PathSeg> = path.segments().collect();
    if let (Some(first), Some(last)) = (segments.first(), segments.last()) {
        draw_line_end(scene, tf, line, line.head, first.start(), first.eval(0.05));
        draw_line_end(scene, tf, line, line.tail, last.end(), last.eval(0.95));
    }
}

/// An arrowhead at `tip`, pointing away from `toward` (a point just inside the path).
fn draw_line_end(scene: &mut dyn Canvas, tf: Affine, line: &ppt_model::Line, end: ppt_model::LineEnd, tip: Point, toward: Point) {
    use ppt_model::EndKind;
    if end.kind == EndKind::None {
        return;
    }
    let dir = tip - toward;
    if dir.hypot() < 1e-9 {
        return;
    }
    let d = dir.normalize();
    let n = Vec2::new(-d.y, d.x);
    // Sizes scale with the line width (small, medium, large = 2, 3, 5 times).
    let lw = (line.width as f64).max(9_525.0);
    let factor = |v: u8| [2.0, 3.0, 5.0][v.min(2) as usize] * lw;
    let (len, half) = (factor(end.length), factor(end.width) / 2.0);
    let base = tip - d * len;
    let brush = color(line.color);
    let mut shape = BezPath::new();
    match end.kind {
        EndKind::Triangle => {
            shape.move_to(tip);
            shape.line_to(base + n * half);
            shape.line_to(base - n * half);
            shape.close_path();
        }
        EndKind::Stealth => {
            shape.move_to(tip);
            shape.line_to(base + n * half);
            shape.line_to(tip - d * (len * 0.6));
            shape.line_to(base - n * half);
            shape.close_path();
        }
        EndKind::Diamond => {
            let c = tip - d * (len / 2.0);
            shape.move_to(tip);
            shape.line_to(c + n * half);
            shape.line_to(base);
            shape.line_to(c - n * half);
            shape.close_path();
        }
        EndKind::Oval => {
            let c = tip - d * (len / 2.0);
            let angle = d.y.atan2(d.x);
            shape = vello::kurbo::Ellipse::new(c, (len / 2.0, half), angle).to_path(0.1);
        }
        EndKind::Arrow => {
            let mut open = BezPath::new();
            open.move_to(base + n * half);
            open.line_to(tip);
            open.line_to(base - n * half);
            scene.stroke(&Stroke::new(line.width.max(1) as f64), tf, brush, None, &open);
            return;
        }
        EndKind::None => return,
    }
    scene.fill(Fill::NonZero, tf, brush, None, &shape);
}

/// Fills `path` (in a `w` × `h` local box mapped by `tf`) with a solid color or gradient.
pub fn fill_paint(scene: &mut dyn Canvas, tf: Affine, paint: &Paint, w: f64, h: f64, path: &impl vello::kurbo::Shape) {
    match paint {
        Paint::Solid(c) => scene.fill(Fill::NonZero, tf, color(*c), None, path),
        Paint::Gradient(g) => {
            let stops: Vec<ColorStop> = g.stops.iter().map(|(pos, c)| ColorStop::from((pos.clamp(0.0, 1.0), color(*c)))).collect();
            let (brush, brush_tf) = gradient_brush(g, w, h);
            scene.fill(Fill::NonZero, tf, &brush.with_stops(stops.as_slice()), Some(brush_tf), path);
        }
    }
}

/// The peniko gradient for `g` over a `w` × `h` box, plus the brush transform.
fn gradient_brush(g: &Gradient, w: f64, h: f64) -> (PGradient, Affine) {
    let (w, h) = (w.max(1.0), h.max(1.0));
    match g.kind {
        GradientKind::Linear { angle, scaled } => {
            let (sin, cos) = angle.to_radians().sin_cos();
            // A scaled angle is set in a unit square, so its normal stretches inversely with the box.
            let (ux, uy) = if scaled { (cos / w, sin / h) } else { (cos, sin) };
            let len = ux.hypot(uy).max(1e-12);
            let (ux, uy) = (ux / len, uy / len);
            // Half the box's extent along the direction: the corners land on 0 and 1.
            let half = ux.abs() * w / 2.0 + uy.abs() * h / 2.0;
            let (cx, cy) = (w / 2.0, h / 2.0);
            (PGradient::new_linear((cx - ux * half, cy - uy * half), (cx + ux * half, cy + uy * half)), Affine::IDENTITY)
        }
        GradientKind::Path { focus: [l, t, r, b], .. } => {
            // Drawn in the unit square and stretched to the box, from the focus rectangle's center.
            let (cx, cy) = ((l + (1.0 - r)) / 2.0, (t + (1.0 - b)) / 2.0);
            let radius = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].iter().map(|(x, y)| (x - cx).hypot(y - cy)).fold(0.0, f64::max);
            (PGradient::new_radial((cx, cy), radius.max(1e-3) as f32), Affine::scale_non_uniform(w, h))
        }
    }
}

/// Longest side decoded images are kept at.
const MAX_IMAGE_SIDE: u32 = 2560;

/// Decoded images keyed by media part name. Failed decodes are cached as `None`.
#[derive(Default)]
pub struct ImageCache {
    images: HashMap<String, Option<ImageData>>,
}

impl ImageCache {
    fn get(&mut self, pres: &Presentation, key: &str) -> Option<&ImageData> {
        self.images
            .entry(key.to_string())
            .or_insert_with(|| {
                let bytes = pres.media.get(key)?;
                let mut img = image::load_from_memory(bytes).ok()?.to_rgba8();
                // Photos far larger than any slide display would crowd the GPU image atlas
                // (images can then fail to draw) and memory; slides never need more.
                let (w, h) = img.dimensions();
                if w.max(h) > MAX_IMAGE_SIDE {
                    let s = MAX_IMAGE_SIDE as f64 / w.max(h) as f64;
                    let (nw, nh) = (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1));
                    img = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
                }
                let (width, height) = img.dimensions();
                Some(ImageData {
                    data: Blob::new(Arc::new(img.into_raw())),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width,
                    height,
                })
            })
            .as_ref()
    }

    pub fn clear(&mut self) {
        self.images.clear();
    }
}

/// Pixel size of an encoded image, without decoding it fully.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()
}

pub struct RenderCtx<'a> {
    pub engine: &'a mut TextEngine,
    pub images: &'a mut ImageCache,
}

/// How a slide is drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct DrawOptions {
    /// Leave out this shape's text; the caller draws it from a live editor layout.
    pub skip_text: Option<ShapeId>,
    /// With `skip_text` on a table: the (row, column) of the cell being edited.
    pub skip_cell: Option<(usize, usize)>,
    /// Editing view: show prompts in empty placeholders and frames for unsupported content.
    pub editing: bool,
    /// Slide units (EMU) per device pixel, for hairlines and dashes.
    pub emu_per_px: f64,
    /// Number shown by slide number fields (set by [`draw_slide`]).
    pub slide_number: Option<usize>,
}

/// The 1-based position of `slide` in the deck, when it is one of its slides.
pub fn slide_number(pres: &Presentation, slide: &Slide) -> Option<usize> {
    pres.slides.iter().position(|s| std::ptr::eq(s, slide)).map(|i| i + 1)
}

/// Draws the whole slide.
pub fn draw_slide(scene: &mut dyn Canvas, cx: &mut RenderCtx, pres: &Presentation, slide: &Slide, view: Affine, opts: &DrawOptions) {
    let opts = &DrawOptions { slide_number: slide_number(pres, slide).or(opts.slide_number), ..*opts };
    let page = Rect::new(0.0, 0.0, pres.slide_width as f64, pres.slide_height as f64);
    scene.fill(Fill::NonZero, view, color(slide.background.unwrap_or(Color::WHITE)), None, &page);
    scene.push_clip_layer(Fill::NonZero, view, &page);
    for shape in &slide.background_shapes {
        draw_shape(scene, cx, pres, shape, view, &DrawOptions { editing: false, ..*opts });
    }
    for shape in &slide.shapes {
        draw_shape(scene, cx, pres, shape, view, opts);
    }
    scene.pop_layer();
}

const FRAME: PColor = PColor::from_rgba8(0xA6, 0xA6, 0xA6, 255);

pub fn draw_shape(scene: &mut dyn Canvas, cx: &mut RenderCtx, pres: &Presentation, shape: &Shape, view: Affine, opts: &DrawOptions) {
    let xf = &shape.xfrm;
    let tf = view * shape_affine(xf, true);
    let (w, h) = (xf.w as f64, xf.h as f64);
    let hairline = opts.emu_per_px.max(1.0);

    if let ShapeKind::Opaque { label } = &shape.kind {
        if label == "hidden" || w <= 0.0 || h <= 0.0 {
            return;
        }
        // Tables, charts, SmartArt: a labelled frame until they are supported.
        let rect = Rect::new(0.0, 0.0, w, h);
        scene.fill(Fill::NonZero, tf, PColor::from_rgba8(0xF2, 0xF2, 0xF2, 255), None, &rect);
        scene.stroke(&Stroke::new(hairline).with_dashes(0.0, [4.0 * hairline, 3.0 * hairline]), tf, FRAME, None, &rect);
        let mut body = TextBody { anchor: ppt_model::Anchor::Middle, ..Default::default() };
        body.paragraphs[0].align = ppt_model::Align::Center;
        body.paragraphs[0].end_style = ppt_model::RunStyle { size_pt: 14.0, color: Color::rgb(0x80, 0x80, 0x80), ..pres.default_run_style() };
        body.set_plain_text(&format!("{label}（尚未支援編輯）"));
        draw_body(scene, cx.engine, &body, &TextFrame::plain(*xf), view);
        return;
    }
    let with_text = opts.skip_text != Some(shape.id);

    if let ShapeKind::Diagram { shapes, w: dw, h: dh } = &shape.kind {
        // The stored drawing, moved and stretched with the frame.
        let place = view * Affine::translate((xf.x as f64, xf.y as f64)) * Affine::scale_non_uniform(w / (*dw).max(1) as f64, h / (*dh).max(1) as f64);
        for child in shapes.iter() {
            draw_shape(scene, cx, pres, child, place, &DrawOptions { editing: false, ..*opts });
        }
        return;
    }
    if let ShapeKind::Chart(chart) = &shape.kind {
        chart::draw_chart(scene, cx.engine, chart, xf, view);
        return;
    }
    if let ShapeKind::Table(table) = &shape.kind {
        draw_table(scene, cx.engine, table, xf, view, opts.skip_cell.filter(|_| !with_text));
        return;
    }

    if let Some(shadow) = &shape.shadow {
        draw_shadow(scene, shape, shadow, view);
    }

    if let ShapeKind::Picture { media, crop } = &shape.kind {
        match media.as_deref().and_then(|m| cx.images.get(pres, m)) {
            Some(img) if img.width > 0 && img.height > 0 => {
                // The visible part of the image (after cropping) stretches to the frame.
                let (iw, ih) = (img.width as f64, img.height as f64);
                let vis_w = (1.0 - crop.left - crop.right).max(1e-6) * iw;
                let vis_h = (1.0 - crop.top - crop.bottom).max(1e-6) * ih;
                let place = Affine::scale_non_uniform(w / vis_w, h / vis_h) * Affine::translate((-crop.left * iw, -crop.top * ih));
                // Pictures are masked by their geometry (rect, rounded rect, custom outline).
                let mut mask = BezPath::new();
                for outline in outlines(&shape.geometry, w, h).into_iter().filter(|o| o.fill) {
                    mask.extend(outline.path);
                }
                if mask.elements().is_empty() {
                    mask = Rect::new(0.0, 0.0, w, h).to_path(0.1);
                }
                let encoded = media.as_deref().and_then(|m| pres.media.get(m)).map(|b| b.as_slice());
                scene.push_clip_layer(Fill::NonZero, tf, &mask);
                scene.draw_image_data(&img.clone(), encoded, tf * place);
                scene.pop_layer();
            }
            _ => {
                // Unsupported or missing image: show a neutral placeholder.
                let rect = Rect::new(0.0, 0.0, w, h);
                scene.fill(Fill::NonZero, tf, PColor::from_rgba8(230, 230, 230, 255), None, &rect);
                scene.stroke(&Stroke::new(12_700.0), tf, PColor::from_rgba8(160, 160, 160, 255), None, &rect);
            }
        }
    }

    for outline in outlines(&shape.geometry, w, h) {
        if outline.fill
            && let Some(fill) = &shape.fill
        {
            fill_paint(scene, tf, fill, w, h, &outline.path);
            if let Shade::Tint(t) = outline.shade {
                let veil = if t > 0.0 { PColor::WHITE } else { PColor::BLACK };
                scene.fill(Fill::NonZero, tf, veil.with_alpha(t.abs()), None, &outline.path);
            }
        }
        if outline.stroke
            && let Some(line) = shape.line
        {
            stroke_line(scene, tf, &line, &outline.path);
        }
    }

    let Some(body) = &shape.text else { return };
    if !body.is_empty() {
        if with_text {
            let numbered = opts.slide_number.and_then(|n| body.with_slide_number(n));
            draw_body(scene, cx.engine, numbered.as_ref().unwrap_or(body), &TextFrame::of(*xf, &shape.geometry), view);
        }
        return;
    }
    // Empty placeholder while editing: dashed frame plus a gray prompt.
    if opts.editing
        && let Some(prompt) = shape.placeholder.as_ref().and_then(|p| p.prompt())
    {
        let rect = Rect::new(0.0, 0.0, w, h);
        scene.stroke(&Stroke::new(hairline).with_dashes(0.0, [4.0 * hairline, 3.0 * hairline]), view * shape_affine(xf, false), FRAME, None, &rect);
        if with_text {
            let mut hint = body.clone();
            hint.set_plain_text(prompt);
            hint.apply_style(0..hint.len(), |s| s.color = Color::rgb(0x80, 0x80, 0x80));
            hint.apply_paragraphs(0..hint.len(), |p| p.bullet = None);
            draw_body(scene, cx.engine, &hint, &TextFrame::of(*xf, &shape.geometry), view);
        }
    }
}

/// Draws a table: cell fills, then text, then borders on top.
pub fn draw_table(scene: &mut dyn Canvas, engine: &mut TextEngine, table: &Table, xf: &Xfrm, view: Affine, skip_cell: Option<(usize, usize)>) {
    let heights = engine.table_row_heights(table);
    let cells: Vec<(usize, usize, Xfrm)> = table.visible_cells().into_iter().map(|(r, c)| (r, c, table.cell_frame(&heights, xf, r, c))).collect();
    let looks: Vec<CellLook> = cells.iter().map(|&(r, c, _)| table.look(r, c)).collect();
    for ((_, _, cx), look) in cells.iter().zip(&looks) {
        if let Some(fill) = look.fill {
            let rect = Rect::new(cx.x as f64, cx.y as f64, (cx.x + cx.w) as f64, (cx.y + cx.h) as f64);
            scene.fill(Fill::NonZero, view, color(fill), None, &rect);
        }
    }
    for &(r, c, ref cx) in &cells {
        let body = &table.rows[r].cells[c].text;
        if !body.is_empty() && skip_cell != Some((r, c)) {
            draw_body(scene, engine, body, &TextFrame::plain(*cx), view);
        }
    }
    for ((_, _, cx), look) in cells.iter().zip(&looks) {
        let (x0, y0, x1, y1) = (cx.x as f64, cx.y as f64, (cx.x + cx.w) as f64, (cx.y + cx.h) as f64);
        let edges = [
            Line::new((x0, y0), (x0, y1)),
            Line::new((x0, y0), (x1, y0)),
            Line::new((x1, y0), (x1, y1)),
            Line::new((x0, y1), (x1, y1)),
        ];
        for (edge, border) in edges.iter().zip(look.borders) {
            if let Some(line) = border.filter(|l| l.width > 0) {
                scene.stroke(&Stroke::new(line.width as f64), view, color(line.color), None, edge);
            }
        }
    }
}

pub fn draw_body(scene: &mut dyn Canvas, engine: &mut TextEngine, body: &TextBody, frame: &TextFrame, view: Affine) {
    let layout = engine.layout_body(body, text_width_pt(frame, body));
    draw_body_layout(scene, &layout, view * text_affine(frame, body, layout.height));
}

/// Draws a laid-out text body (glyphs and bullets); `tf` maps points to device space.
pub fn draw_body_layout(scene: &mut dyn Canvas, layout: &BodyLayout, tf: Affine) {
    draw_layout(scene, &layout.layout, &layout.text, tf);
    for b in &layout.bullets {
        draw_layout(scene, &b.layout, &b.text, tf * Affine::translate(Vec2::new(b.x as f64, b.y as f64)));
    }
}

/// Outline width for synthetic bold, relative to the font size.
const SYNTHETIC_BOLD: f32 = 0.035;

/// Draws a parley layout built from `text`, whose units are points; `tf` maps points to device space.
pub fn draw_layout(scene: &mut dyn Canvas, layout: &Layout<Color>, text: &str, tf: Affine) {
    for line in layout.lines() {
        let line_text = line.text_range();
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            let run = glyph_run.run();
            let style = glyph_run.style();
            let brush = color(style.brush);
            let coords: Vec<i16> = run.normalized_coords().iter().map(|c| c.to_bits()).collect();
            // vello applies this in font space (y up), so a positive x-skew leans glyphs to the right.
            let glyph_transform = run.synthesis().skew().map(|angle| Affine::skew((angle as f64).to_radians().tan(), 0.0));
            let glyphs: Vec<vello::Glyph> = glyph_run.positioned_glyphs().map(|g| vello::Glyph { id: g.id, x: g.x, y: g.y }).collect();
            // Each glyph's source text: the clusters of this run on this line, in glyph order.
            let mut ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(glyphs.len());
            for cluster in run.visual_clusters().filter(|c| c.text_range().start >= line_text.start && c.text_range().end <= line_text.end) {
                for _ in cluster.glyphs() {
                    ranges.push(cluster.text_range());
                }
            }
            if ranges.len() != glyphs.len() {
                ranges = vec![0..0; glyphs.len()];
            }
            scene.draw_glyph_run(&GlyphRun {
                font: run.font(),
                size: run.font_size(),
                coords: &coords,
                transform: tf,
                glyph_transform,
                color: brush,
                glyphs: &glyphs,
                // Synthetic bold when the family has no face heavy enough: thicken the outlines.
                embolden: run.synthesis().embolden().then_some(run.font_size() * SYNTHETIC_BOLD),
                text,
                ranges: &ranges,
            });
            if let Some(strike) = &style.strikethrough {
                let metrics = run.font_metrics();
                let offset = strike.offset.unwrap_or(metrics.strikethrough_offset) as f64;
                let size = strike.size.unwrap_or(metrics.strikethrough_size).max(0.5) as f64;
                let x0 = glyph_run.offset() as f64;
                let y = glyph_run.baseline() as f64 - offset;
                let rect = Rect::new(x0, y, x0 + glyph_run.advance() as f64, y + size);
                scene.fill(Fill::NonZero, tf, brush, None, &rect);
            }
            if let Some(underline) = &style.underline {
                let metrics = run.font_metrics();
                let offset = underline.offset.unwrap_or(metrics.underline_offset) as f64;
                let size = underline.size.unwrap_or(metrics.underline_size).max(0.5) as f64;
                let x0 = glyph_run.offset() as f64;
                let y = glyph_run.baseline() as f64 - offset;
                let rect = Rect::new(x0, y, x0 + glyph_run.advance() as f64, y + size);
                scene.fill(Fill::NonZero, tf, brush, None, &rect);
            }
        }
    }
}

/// Smart guides and the marquee rectangle, in device space.
pub fn draw_guides(scene: &mut Scene, lines: &[(Point, Point)], dpr: f64) {
    let stroke = Stroke::new(1.0 * dpr).with_dashes(0.0, [4.0 * dpr, 3.0 * dpr]);
    for (a, b) in lines {
        scene.stroke(&stroke, Affine::IDENTITY, PColor::from_rgba8(0xE8, 0x56, 0x2A, 255), None, &Line::new(*a, *b));
    }
}

pub fn draw_marquee(scene: &mut Scene, rect: Rect, dpr: f64) {
    scene.fill(Fill::NonZero, Affine::IDENTITY, PColor::from_rgba8(0x43, 0x7E, 0xC9, 0x22), None, &rect);
    scene.stroke(&Stroke::new(1.0 * dpr), Affine::IDENTITY, PColor::from_rgba8(0x43, 0x7E, 0xC9, 0xCC), None, &rect);
}

const SELECTION: PColor = PColor::from_rgba8(0x7A, 0x8E, 0xA8, 255);
const HANDLE_BORDER: PColor = PColor::from_rgba8(0x5F, 0x6B, 0x7A, 255);

/// Selection frame drawn in device space so line widths do not scale with zoom.
pub struct SelectionOverlay {
    pub corners: [Point; 4],
    pub handles: Vec<Point>,
    pub rotate: Option<Point>,
    /// Device pixels per CSS pixel.
    pub dpr: f64,
    /// Dashed frame while editing text, solid otherwise.
    pub editing: bool,
}

pub fn draw_selection(scene: &mut Scene, o: &SelectionOverlay) {
    let mut frame = BezPath::new();
    frame.move_to(o.corners[0]);
    for c in &o.corners[1..] {
        frame.line_to(*c);
    }
    frame.close_path();
    let mut stroke = Stroke::new(1.0 * o.dpr);
    if o.editing {
        stroke = stroke.with_dashes(0.0, [4.0 * o.dpr, 3.0 * o.dpr]);
    }
    scene.stroke(&stroke, Affine::IDENTITY, SELECTION, None, &frame);
    if let Some(rot) = o.rotate {
        let top = o.corners[0].midpoint(o.corners[1]);
        scene.stroke(&Stroke::new(1.0 * o.dpr), Affine::IDENTITY, SELECTION, None, &Line::new(top, rot));
        draw_handle(scene, rot, o.dpr);
    }
    for h in &o.handles {
        draw_handle(scene, *h, o.dpr);
    }
}

fn draw_handle(scene: &mut Scene, at: Point, dpr: f64) {
    let circle = Circle::new(at, 4.5 * dpr);
    scene.fill(Fill::NonZero, Affine::IDENTITY, PColor::WHITE, None, &circle);
    scene.stroke(&Stroke::new(1.0 * dpr), Affine::IDENTITY, HANDLE_BORDER, None, &circle);
}

/// Text selection highlights, caret and IME composition underline, all in
/// text layout space (points) mapped by `tf`.
pub struct TextOverlay {
    pub selection: Vec<Rect>,
    pub caret: Option<Rect>,
    pub compose: Vec<Rect>,
}

pub fn draw_text_overlay(scene: &mut Scene, o: &TextOverlay, tf: Affine) {
    for r in &o.selection {
        scene.fill(Fill::NonZero, tf, PColor::from_rgba8(0x99, 0xC2, 0xFF, 0x88), None, r);
    }
    for r in &o.compose {
        let underline = Rect::new(r.x0, r.y1 - 1.5, r.x1, r.y1);
        scene.fill(Fill::NonZero, tf, PColor::from_rgba8(0x33, 0x33, 0x33, 255), None, &underline);
    }
    if let Some(c) = o.caret {
        scene.fill(Fill::NonZero, tf, PColor::BLACK, None, &c);
    }
}
