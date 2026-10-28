//! Headless slide rendering and image comparison for visual regression tests.
//!
//! Slides are drawn with the same renderer, text engine and font substitution
//! the app uses, but on a native wgpu device instead of WebGPU in the webview.

use std::collections::HashSet;

use image::{Rgba, RgbaImage};
use ppt_fonts::FontService;
use ppt_model::{Presentation, Slide};
use ppt_render::vello::kurbo::Affine;
use ppt_render::vello::peniko::Color as PColor;
use ppt_render::vello::util::RenderContext;
use ppt_render::vello::wgpu;
use ppt_render::vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use ppt_render::{DrawOptions, ImageCache, RenderCtx, draw_slide};
use ppt_text::TextEngine;

/// Families always registered for missing glyphs; mirrors `FALLBACK_FAMILIES` in ui/src/lib/fonts.ts.
pub const FALLBACK_FAMILIES: [&str; 2] = ["Helvetica Neue", "PingFang TC"];

pub struct SlideRenderer {
    cx: RenderContext,
    dev_id: usize,
    renderer: Renderer,
    engine: TextEngine,
    fonts: FontService,
    loaded_paths: HashSet<String>,
    resolved: HashSet<String>,
}

impl SlideRenderer {
    pub fn new() -> Result<Self, String> {
        let mut cx = RenderContext::new();
        let dev_id = pollster::block_on(cx.device(None)).ok_or("no wgpu device")?;
        let renderer = Renderer::new(
            &cx.devices[dev_id].device,
            RendererOptions { use_cpu: false, antialiasing_support: AaSupport::area_only(), num_init_threads: None, pipeline_cache: None },
        )
        .map_err(|e| e.to_string())?;
        let mut this = Self {
            cx,
            dev_id,
            renderer,
            engine: TextEngine::new(false),
            fonts: FontService::new(),
            loaded_paths: HashSet::new(),
            resolved: HashSet::new(),
        };
        this.engine.set_fallbacks(FALLBACK_FAMILIES.iter().map(|f| f.to_string()).collect());
        this.ensure_fonts(FALLBACK_FAMILIES.iter().map(|f| f.to_string()));
        Ok(this)
    }

    /// Registers each family (or its substitute), like `ensureFonts` in the app.
    pub fn ensure_fonts(&mut self, families: impl IntoIterator<Item = String>) {
        for requested in families {
            if !self.resolved.insert(requested.to_lowercase()) {
                continue;
            }
            let Some(font) = self.fonts.resolve(&requested) else { continue };
            for path in font.paths {
                if self.loaded_paths.insert(path.clone())
                    && let Ok(bytes) = std::fs::read(&path)
                {
                    self.engine.register_font(bytes);
                }
            }
            if !font.family.eq_ignore_ascii_case(&requested) {
                self.engine.set_alias(&requested, &font.family);
            }
        }
    }

    /// Renders one slide `width` pixels wide, as the app would show it (no edit chrome).
    pub fn render(&mut self, pres: &Presentation, slide: &Slide, width: u32) -> Result<RgbaImage, String> {
        let height = ((width as f64) * pres.slide_height as f64 / pres.slide_width as f64).round().max(1.0) as u32;
        let scale = width as f64 / pres.slide_width as f64;
        let mut scene = Scene::new();
        {
            // A fresh image cache per deck: media keys are only unique within one package.
            let mut images = ImageCache::default();
            let opts = DrawOptions { skip_text: None, editing: false, emu_per_px: 1.0 / scale };
            let mut cx = RenderCtx { engine: &mut self.engine, images: &mut images };
            draw_slide(&mut scene, &mut cx, pres, slide, Affine::scale(scale), &opts);
        }

        let dev = &self.cx.devices[self.dev_id];
        let (device, queue) = (&dev.device, &dev.queue);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vr"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let params = RenderParams { base_color: PColor::WHITE, width, height, antialiasing_method: AaConfig::Area };
        self.renderer.render_to_texture(device, queue, &scene, &view, &params).map_err(|e| e.to_string())?;

        let padded = (width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vr readback"),
            size: (padded * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("vr copy") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        {
            let data = buffer.get_mapped_range(..).map_err(|e| e.to_string())?;
            for row in data.chunks(padded as usize).take(height as usize) {
                pixels.extend_from_slice(&row[..(width * 4) as usize]);
            }
        }
        buffer.unmap();
        RgbaImage::from_raw(width, height, pixels).ok_or_else(|| "bad readback size".to_string())
    }
}

/// A channel difference above this counts as a mismatch.
pub const PIXEL_THRESHOLD: u8 = 48;

#[derive(Clone, Copy, Debug)]
pub struct Diff {
    /// Mismatched pixels, in percent of the page.
    pub score: f64,
}

fn distance(p: &Rgba<u8>, q: &Rgba<u8>) -> u8 {
    (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0)
}

/// Whether `p` has a close match within one pixel of (x, y) in `other`, so
/// antialiasing and sub-pixel glyph placement don't count as differences.
fn matched_nearby(p: &Rgba<u8>, other: &RgbaImage, x: u32, y: u32) -> bool {
    let (w, h) = other.dimensions();
    (y.saturating_sub(1)..=(y + 1).min(h - 1))
        .any(|sy| (x.saturating_sub(1)..=(x + 1).min(w - 1)).any(|sx| distance(p, other.get_pixel(sx, sy)) <= PIXEL_THRESHOLD))
}

/// Compares two same-sized images; returns the score and a visualization
/// (the actual image faded, mismatches in red).
pub fn compare(expected: &RgbaImage, actual: &RgbaImage) -> (Diff, RgbaImage) {
    assert_eq!(expected.dimensions(), actual.dimensions(), "compare needs equal sizes");
    let (w, h) = expected.dimensions();
    let mut bad = 0u64;
    let vis = RgbaImage::from_fn(w, h, |x, y| {
        let (p, q) = (expected.get_pixel(x, y), actual.get_pixel(x, y));
        let same = distance(p, q) <= PIXEL_THRESHOLD || (matched_nearby(p, actual, x, y) && matched_nearby(q, expected, x, y));
        if same {
            let fade = |v: u8| 255 - (255 - v) / 4;
            Rgba([fade(q[0]), fade(q[1]), fade(q[2]), 255])
        } else {
            bad += 1;
            Rgba([255, 0, 0, 255])
        }
    });
    (Diff { score: bad as f64 * 100.0 / (w as f64 * h as f64) }, vis)
}

/// Places images side by side with a gray gutter.
pub fn side_by_side(images: &[&RgbaImage]) -> RgbaImage {
    const GAP: u32 = 8;
    let h = images.iter().map(|i| i.height()).max().unwrap_or(0);
    let w = images.iter().map(|i| i.width()).sum::<u32>() + GAP * images.len().saturating_sub(1) as u32;
    let mut out = RgbaImage::from_pixel(w, h, Rgba([128, 128, 128, 255]));
    let mut x = 0;
    for img in images {
        image::imageops::overlay(&mut out, *img, x as i64, 0);
        x += img.width() + GAP;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_score_zero() {
        let img = RgbaImage::from_fn(20, 10, |x, _| Rgba([(x * 10) as u8, 0, 0, 255]));
        assert_eq!(compare(&img, &img).0.score, 0.0);
    }

    #[test]
    fn a_shifted_block_is_a_mismatch() {
        let block = |x0: u32| RgbaImage::from_fn(40, 40, move |x, y| if (x0..x0 + 10).contains(&x) && y < 10 { Rgba([0, 0, 0, 255]) } else { Rgba([255; 4]) });
        let (d, _) = compare(&block(0), &block(20));
        assert!(d.score > 5.0, "{}", d.score);
        // A one-pixel shift only flags the hard edges, while a missing thin line is still caught.
        assert!(compare(&block(0), &block(1)).0.score < 1.0);
        let line = |on: bool| RgbaImage::from_fn(40, 40, move |x, _| if on && x == 20 { Rgba([0, 0, 0, 255]) } else { Rgba([255; 4]) });
        assert!(compare(&line(true), &line(false)).0.score > 2.0);
    }
}
