//! Headless slide rendering on a native wgpu device: the same renderer, text
//! engine and font substitution the app uses in the webview. Used by visual
//! regression tests and video export.

use std::collections::HashSet;

use image::RgbaImage;
use ppt_fonts::FontService;
use ppt_model::anim::Frame;
use ppt_model::{Presentation, Slide};
use ppt_render::vello::kurbo::Affine;
use ppt_render::vello::peniko::Color as PColor;
use ppt_render::vello::util::RenderContext;
use ppt_render::vello::wgpu;
use ppt_render::vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use ppt_render::{DrawOptions, ImageCache, RenderCtx, draw_slide};
use ppt_text::TextEngine;

/// Families always registered for missing glyphs; mirrors `FALLBACK_FAMILIES` in ui/src/lib/fonts.ts.
pub const FALLBACK_FAMILIES: [&str; 4] = ["Helvetica Neue", "PingFang TC", "Apple Symbols", "Zapf Dingbats"];

pub struct SlideRenderer {
    cx: RenderContext,
    dev_id: usize,
    renderer: Renderer,
    engine: TextEngine,
    fonts: FontService,
    loaded_paths: HashSet<String>,
    resolved: HashSet<String>,
    /// Decoded media, kept across slides like the app does (see [`SlideRenderer::new_deck`]).
    images: ImageCache,
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
            images: ImageCache::default(),
        };
        // The emoji family is in the stack but only registered for decks that need it.
        this.engine.set_fallbacks(FALLBACK_FAMILIES.iter().copied().chain([ppt_model::EMOJI_FAMILY]).map(str::to_string).collect());
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

    /// The text engine, for diagnostics.
    pub fn engine(&mut self) -> &mut TextEngine {
        &mut self.engine
    }

    /// Bytes held by decoded images.
    pub fn image_memory(&self) -> usize {
        self.images.memory()
    }

    /// Forgets decoded media; call before rendering a different deck (media keys repeat across decks).
    pub fn new_deck(&mut self) {
        self.images.clear();
    }

    /// Renders one slide `width` pixels wide, as the app would show it (no edit chrome).
    pub fn render(&mut self, pres: &Presentation, slide: &Slide, width: u32) -> Result<RgbaImage, String> {
        let (scene, height) = self.encode(pres, slide, width);
        self.render_scene(&scene, width, height)
    }

    /// The slide's vello scene at `width` pixels (text layout, image decoding), and its height.
    pub fn encode(&mut self, pres: &Presentation, slide: &Slide, width: u32) -> (Scene, u32) {
        let height = ((width as f64) * pres.slide_height as f64 / pres.slide_width as f64).round().max(1.0) as u32;
        let scale = width as f64 / pres.slide_width as f64;
        let mut scene = Scene::new();
        let opts = DrawOptions { editing: false, emu_per_px: 1.0 / scale, ..Default::default() };
        let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
        draw_slide(&mut scene, &mut cx, pres, slide, Affine::scale(scale), &opts);
        (scene, height)
    }

    /// The deck as a PDF (every slide, hidden ones included, to line up with reference PDFs).
    pub fn export_pdf(&mut self, pres: &Presentation) -> Result<Vec<u8>, String> {
        let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
        ppt_pdf::export_pdf(pres, &mut cx, ppt_pdf::PdfOptions { include_hidden: true })
    }

    /// Renders a slide with an animation frame applied.
    pub fn render_animated(&mut self, pres: &Presentation, slide: &Slide, frame: &Frame, width: u32) -> Result<RgbaImage, String> {
        let height = ((width as f64) * pres.slide_height as f64 / pres.slide_width as f64).round().max(1.0) as u32;
        let scale = width as f64 / pres.slide_width as f64;
        let mut scene = Scene::new();
        {
            let opts = DrawOptions { editing: false, emu_per_px: 1.0 / scale, ..Default::default() };
            let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
            ppt_render::draw_slide_animated(&mut scene, &mut cx, pres, slide, Affine::scale(scale), &opts, frame);
        }
        self.render_scene(&scene, width, height)
    }

    /// Renders a frame of the transition from `from` (black when `None`) to `to` at progress `t`.
    pub fn render_transition(&mut self, pres: &Presentation, from: Option<&Slide>, to: &Slide, kind: &ppt_model::TransitionKind, t: f64, width: u32) -> Result<RgbaImage, String> {
        self.render_transition_animated(pres, from.map(|s| (s, None)), (to, None), kind, t, width)
    }

    /// Like [`SlideRenderer::render_transition`], each slide with an optional animation state.
    pub fn render_transition_animated(&mut self, pres: &Presentation, from: Option<(&Slide, Option<&Frame>)>, to: (&Slide, Option<&Frame>), kind: &ppt_model::TransitionKind, t: f64, width: u32) -> Result<RgbaImage, String> {
        let height = ((width as f64) * pres.slide_height as f64 / pres.slide_width as f64).round().max(1.0) as u32;
        let scale = width as f64 / pres.slide_width as f64;
        let opts = DrawOptions { editing: false, emu_per_px: 1.0 / scale, ..Default::default() };
        let mut slide_scene = |(slide, frame): (&Slide, Option<&Frame>)| {
            let mut s = Scene::new();
            let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
            match frame {
                Some(f) => ppt_render::draw_slide_animated(&mut s, &mut cx, pres, slide, Affine::IDENTITY, &opts, f),
                None => draw_slide(&mut s, &mut cx, pres, slide, Affine::IDENTITY, &opts),
            }
            s
        };
        let a = from.map(&mut slide_scene);
        let b = slide_scene(to);
        let mut scene = Scene::new();
        ppt_render::draw_transition(&mut scene, a.as_ref(), &b, kind, t, Affine::scale(scale), pres.slide_width as f64, pres.slide_height as f64);
        self.render_scene(&scene, width, height)
    }

    pub fn render_scene(&mut self, scene: &Scene, width: u32, height: u32) -> Result<RgbaImage, String> {
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
        self.renderer.render_to_texture(device, queue, scene, &view, &params).map_err(|e| e.to_string())?;

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

