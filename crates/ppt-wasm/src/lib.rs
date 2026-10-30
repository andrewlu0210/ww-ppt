//! WebAssembly facade used by the React shell.
//!
//! The document model, editor and renderer all live here, inside the webview,
//! so pointer and keyboard handling never wait on IPC. The native backend only
//! supplies file and font bytes.
#![cfg(target_arch = "wasm32")]

use std::collections::HashMap;
use std::sync::Arc;

use js_sys::{Array, Object, Reflect, Uint8Array};
use ppt_editor::show::{Blank, Screen, SlideShow};
use ppt_model::anim::{PRESETS, Player, SlideStage, Trigger, effect_elements};
use ppt_editor::{AlignTo, BorderSet, CursorIcon, Editor, FindMatch, FindOptions, FindPlace, Handle, Modifiers, PointerEvent, Toggle, Tolerance, ZOrder, utf16_to_byte_offset};
use ppt_geometry::shape_affine;
use ppt_model::{Align, Background, Color, EMU_PER_PT, Gradient, GradientKind, Paint, Presentation};
use ppt_ooxml::design;
use ppt_render::vello::kurbo::{Affine, Point, Rect, Vec2};
use ppt_render::vello::peniko::Color as PColor;
use ppt_render::vello::util::{RenderContext, RenderSurface};
use ppt_render::vello::wgpu::{self, CurrentSurfaceTexture, SurfaceTarget};
use ppt_render::vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use ppt_render::{
    DrawOptions, ImageCache, RenderCtx, SelectionOverlay, TextOverlay, draw_body_layout, draw_guides, draw_marquee, draw_selection,
    draw_shape, draw_slide, draw_slide_animated, draw_text_overlay, image_size,
};
use ppt_text::TextEngine;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// Time far past any animation (ms): shows effects finished.
const ANIM_END: f64 = 1e12;
/// Gap around the slide when fitted to the canvas, in CSS pixels.
const FIT_MARGIN: f64 = 24.0;
const HANDLE_RADIUS: f64 = 7.0;
const ROTATE_OFFSET: f64 = 22.0;
const SNAP_DISTANCE: f64 = 6.0;
/// Workspace background behind the slide.
const WORKSPACE: PColor = PColor::from_rgba8(0xE6, 0xE6, 0xE6, 255);

struct Gpu {
    cx: RenderContext,
    surface: RenderSurface<'static>,
    renderer: Renderer,
}

#[wasm_bindgen]
pub struct App {
    editor: Editor,
    engine: TextEngine,
    images: ImageCache,
    gpu: Gpu,
    scene: Scene,
    /// Canvas size in device pixels.
    width: u32,
    height: u32,
    dpr: f64,
    /// Multiplier on top of fit-to-window.
    zoom: f64,
    caret_on: bool,
    dirty: bool,
    last_frame_ms: f64,
    /// Of that, building the scene (slide drawing, text layout, overlays).
    last_scene_ms: f64,
    media_counter: u32,
    /// Slide show in progress: the canvas shows only the slide, fitted edge to edge.
    show: Option<SlideShow>,
    /// Where the show's slide goes (device pixels), instead of the whole canvas: the presenter view.
    show_viewport: Option<Rect>,
    /// A transition being played (in the show, or as a preview in the editor).
    anim: Option<Anim>,
    /// When the current animation click began (ms; in the future while a transition plays first).
    step_start_ms: f64,
    /// Whether the current click plays from its start; false shows it finished (after going back).
    step_animate: bool,
    /// An animation is playing: keep drawing frames.
    step_running: bool,
    /// When the editor's animation preview started (ms), while it plays.
    preview_start_ms: Option<f64>,
    /// Misspellings by text key (`spellKey`): the text they were found in, and byte ranges.
    spelling: HashMap<String, (String, Vec<std::ops::Range<usize>>)>,
    /// Gridlines are shown (a viewing preference, not saved with the deck).
    show_grid: bool,
}

/// A running transition: both slides pre-encoded in slide space.
struct Anim {
    from: Option<Scene>,
    to: Scene,
    kind: ppt_model::TransitionKind,
    start_ms: f64,
    duration_ms: f64,
}

/// Shadow presets: name, direction (degrees), distance and blur (EMU), as
/// PowerPoint's outer shadows.
const SHADOWS: [(&str, f64, i64, i64); 5] = [
    ("bottomRight", 45.0, 38_100, 50_800),
    ("bottom", 90.0, 38_100, 50_800),
    ("right", 0.0, 38_100, 50_800),
    ("bottomLeft", 135.0, 38_100, 50_800),
    ("center", 0.0, 0, 63_500),
];

/// Key of a text for spelling results: `s<id>` for a shape, `c<id>:<row>:<col>` for a cell.
fn spell_key(place: FindPlace) -> String {
    match place {
        FindPlace::Shape(id) => format!("s{}", id.0),
        FindPlace::Cell(id, r, c) => format!("c{}:{r}:{c}", id.0),
        FindPlace::Notes => "n".into(),
    }
}

fn parse_spell_key(key: &str) -> Option<FindPlace> {
    let id = |v: &str| v.parse().ok().map(ppt_model::ShapeId);
    if let Some(rest) = key.strip_prefix('s') {
        return Some(FindPlace::Shape(id(rest)?));
    }
    let mut parts = key.strip_prefix('c')?.split(':');
    Some(FindPlace::Cell(id(parts.next()?)?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Counts heap use, for `memoryStats` (the WASM memory itself only grows, so it
/// shows the high-water mark, not what is in use).
struct CountingAlloc;

static HEAP_NOW: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static HEAP_PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn heap_add(n: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    let now = HEAP_NOW.fetch_add(n, Relaxed) + n;
    HEAP_PEAK.fetch_max(now, Relaxed);
}

unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let p = unsafe { std::alloc::System.alloc(layout) };
        if !p.is_null() {
            heap_add(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) };
        HEAP_NOW.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { std::alloc::System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            HEAP_NOW.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
            heap_add(new_size);
        }
        p
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// A high-resolution clock (`performance.now()`), in milliseconds.
fn now_ms() -> f64 {
    js_sys::Reflect::get(&js_sys::global(), &"performance".into())
        .ok()
        .and_then(|p| js_sys::Reflect::get(&p, &"now".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()).and_then(|f| f.call0(&p).ok()))
        .and_then(|v| v.as_f64())
        .unwrap_or_else(js_sys::Date::now)
}

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn hex(c: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn parse_hex(s: &str) -> Option<Color> {
    Color::from_hex(s.trim_start_matches('#'))
}

fn template() -> Presentation {
    ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).expect("built-in template parses")
}

/// Creates the app on a canvas. Fails when WebGPU is unavailable.
#[wasm_bindgen(js_name = createApp)]
pub async fn create_app(canvas: HtmlCanvasElement, dpr: f64) -> Result<App, JsValue> {
    console_error_panic_hook::set_once();
    let (width, height) = (canvas.width().max(1), canvas.height().max(1));
    let mut cx = RenderContext::new();
    let surface = cx
        .create_surface(SurfaceTarget::Canvas(canvas), width, height, wgpu::PresentMode::AutoVsync)
        .await
        .map_err(js_err)?;
    let device = &cx.devices[surface.dev_id].device;
    let renderer = Renderer::new(
        device,
        RendererOptions { use_cpu: false, antialiasing_support: AaSupport::area_only(), num_init_threads: None, pipeline_cache: None },
    )
    .map_err(js_err)?;
    Ok(App {
        editor: Editor::new(template()),
        engine: TextEngine::new(false),
        images: ImageCache::default(),
        gpu: Gpu { cx, surface, renderer },
        scene: Scene::new(),
        width,
        height,
        dpr,
        zoom: 1.0,
        caret_on: true,
        dirty: true,
        last_frame_ms: 0.0,
        last_scene_ms: 0.0,
        media_counter: 0,
        show: None,
        show_viewport: None,
        anim: None,
        step_start_ms: 0.0,
        step_animate: false,
        step_running: false,
        preview_start_ms: None,
        spelling: HashMap::new(),
        show_grid: false,
    })
}

impl App {
    /// Device pixels per EMU.
    fn scale(&self) -> f64 {
        let pres = &self.editor.pres;
        if let (Some(_), Some(r)) = (&self.show, self.show_viewport) {
            return (r.width() / pres.slide_width as f64).min(r.height() / pres.slide_height as f64);
        }
        let (margin, zoom) = if self.show.is_some() { (0.0, 1.0) } else { (FIT_MARGIN * self.dpr, self.zoom) };
        let sx = (self.width as f64 - 2.0 * margin).max(1.0) / pres.slide_width as f64;
        let sy = (self.height as f64 - 2.0 * margin).max(1.0) / pres.slide_height as f64;
        sx.min(sy) * zoom
    }

    /// Slide space (EMU) → canvas device pixels.
    fn view(&self) -> Affine {
        if let (Some(_), Some(r)) = (&self.show, self.show_viewport) {
            let pres = &self.editor.pres;
            let s = (r.width() / pres.slide_width as f64).min(r.height() / pres.slide_height as f64);
            let ox = r.x0 + (r.width() - pres.slide_width as f64 * s) / 2.0;
            let oy = r.y0 + (r.height() - pres.slide_height as f64 * s) / 2.0;
            return Affine::translate(Vec2::new(ox, oy)) * Affine::scale(s);
        }
        let s = self.scale();
        let pres = &self.editor.pres;
        let ox = (self.width as f64 - pres.slide_width as f64 * s) / 2.0;
        let oy = (self.height as f64 - pres.slide_height as f64 * s) / 2.0;
        Affine::translate(Vec2::new(ox, oy)) * Affine::scale(s)
    }

    /// Gridlines over the slide and drawing guides across the canvas.
    fn draw_grid_and_guides(&mut self, view: Affine) {
        use ppt_render::vello::kurbo::{Line, Stroke};
        let pres = &self.editor.pres;
        let (w, h) = (pres.slide_width as f64, pres.slide_height as f64);
        let s = self.scale();
        let hair = Stroke::new(1.0);
        if self.show_grid {
            // Lines at the grid spacing, thinned out until they are at least 8 CSS px apart.
            let mut step = pres.view.grid_spacing.max(1) as f64;
            for k in [2.0, 2.5, 2.0].iter().cycle().take(30) {
                if step * s >= 8.0 * self.dpr {
                    break;
                }
                step *= k;
            }
            let color = PColor::from_rgba8(0x80, 0x80, 0x80, 0x50);
            let lines = |extent: f64, c: f64| {
                let n = (c / step).ceil() as i64;
                (-n..=n).map(move |i| c + i as f64 * step).filter(move |v| *v > 0.0 && *v < extent)
            };
            for x in lines(w, w / 2.0) {
                let (a, b) = (view * Point::new(x, 0.0), view * Point::new(x, h));
                self.scene.stroke(&hair, Affine::IDENTITY, color, None, &Line::new((a.x.round() + 0.5, a.y), (b.x.round() + 0.5, b.y)));
            }
            for y in lines(h, h / 2.0) {
                let (a, b) = (view * Point::new(0.0, y), view * Point::new(w, y));
                self.scene.stroke(&hair, Affine::IDENTITY, color, None, &Line::new((a.x, a.y.round() + 0.5), (b.x, b.y.round() + 0.5)));
            }
        }
        if pres.view.show_guides {
            let dragged = self.editor.dragged_guide();
            let dash = Stroke::new(1.0 * self.dpr).with_dashes(0.0, [5.0 * self.dpr, 3.0 * self.dpr]);
            let (cw, ch) = (self.width as f64, self.height as f64);
            for (i, g) in pres.view.guides.iter().enumerate() {
                let color = if dragged == Some(i) { PColor::from_rgba8(0x43, 0x7E, 0xC9, 0xFF) } else { PColor::from_rgba8(0x70, 0x70, 0x70, 0xC0) };
                let p = view * Point::new(g.pos as f64, g.pos as f64);
                let line = if g.vertical { Line::new((p.x, 0.0), (p.x, ch)) } else { Line::new((0.0, p.y), (cw, p.y)) };
                self.scene.stroke(&dash, Affine::IDENTITY, color, None, &line);
            }
        }
    }

    fn tolerance(&self) -> Tolerance {
        let emu_per_css = self.dpr / self.scale();
        Tolerance { handle: HANDLE_RADIUS * emu_per_css, rotate_offset: ROTATE_OFFSET * emu_per_css, snap: SNAP_DISTANCE * emu_per_css }
    }

    fn pointer(&self, x: f64, y: f64, shift: bool, alt: bool, meta: bool, clicks: u32) -> PointerEvent {
        let p = self.view().inverse() * Point::new(x * self.dpr, y * self.dpr);
        PointerEvent { x: p.x, y: p.y, shift, alt, meta, clicks }
    }

    fn touch(&mut self, changed: bool) -> bool {
        if changed {
            self.dirty = true;
            self.caret_on = true;
        }
        changed
    }

    /// A slide encoded in slide space, for transitions.
    /// A slide encoded in slide space, for transitions: before its animations
    /// (the incoming slide) or with all of them played (`finished`, the outgoing one).
    fn slide_scene(&mut self, index: usize, s: f64, finished: bool) -> Option<Scene> {
        let slide = self.editor.pres.slides.get(index)?;
        let mut scene = Scene::new();
        let opts = DrawOptions { editing: false, emu_per_px: 1.0 / s, ..Default::default() };
        let frame = slide.timing.as_deref().map(|t| {
            let player = Player::new(t);
            let stage = SlideStage { pres: &self.editor.pres, slide };
            if finished { player.frame(&stage, player.clicks(), ANIM_END) } else { player.frame(&stage, 0, 0.0) }
        });
        let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
        match &frame {
            Some(f) => draw_slide_animated(&mut scene, &mut cx, &self.editor.pres, slide, Affine::IDENTITY, &opts, f),
            None => draw_slide(&mut scene, &mut cx, &self.editor.pres, slide, Affine::IDENTITY, &opts),
        }
        Some(scene)
    }

    /// Starts the current slide's animation click `step` now (or after the slide's
    /// transition when it just began), or shows it finished.
    fn begin_step(&mut self, animate: bool, after_transition: bool) {
        let delay = if after_transition { self.anim.as_ref().map_or(0.0, |a| a.duration_ms) } else { 0.0 };
        self.step_start_ms = js_sys::Date::now() + delay;
        self.step_animate = animate;
        self.dirty = true;
    }

    /// Starts slide `to`'s transition from slide `from` (black for `None`), if it has one.
    fn start_transition(&mut self, from: Option<usize>, to: usize) {
        self.anim = None;
        let Some(t) = self.editor.pres.slides.get(to).and_then(|s| s.transition.clone()).filter(|t| t.animates()) else { return };
        let s = self.scale();
        let from = from.and_then(|i| self.slide_scene(i, s, true));
        let Some(to) = self.slide_scene(to, s, false) else { return };
        self.anim = Some(Anim { from, to, kind: t.kind, start_ms: js_sys::Date::now(), duration_ms: t.duration_ms as f64 });
        self.dirty = true;
    }

    /// Draws the running transition, if any; false when there is none (or it just ended).
    fn draw_anim(&mut self, view: Affine) -> bool {
        let Some(a) = &self.anim else { return false };
        let t = (js_sys::Date::now() - a.start_ms) / a.duration_ms.max(1.0);
        if t >= 1.0 {
            self.anim = None;
            return false;
        }
        let pres = &self.editor.pres;
        ppt_render::draw_transition(&mut self.scene, a.from.as_ref(), &a.to, &a.kind, t, view, pres.slide_width as f64, pres.slide_height as f64);
        true
    }

    /// The slide show frame: the slide only, or a blank / end screen.
    fn build_show_scene(&mut self, show: &SlideShow, view: Affine, s: f64) {
        let blank = match show.blank() {
            Some(Blank::White) => Some(PColor::WHITE),
            Some(Blank::Black) => Some(PColor::BLACK),
            None => None,
        };
        let full = Rect::new(0.0, 0.0, self.width as f64, self.height as f64);
        // In the presenter view, blanking shows on the slide area only.
        let area = match self.show_viewport {
            Some(_) => {
                let pres = &self.editor.pres;
                view.transform_rect_bbox(Rect::new(0.0, 0.0, pres.slide_width as f64, pres.slide_height as f64))
            }
            None => full,
        };
        self.scene.fill(ppt_render::vello::peniko::Fill::NonZero, Affine::IDENTITY, PColor::BLACK, None, &full);
        self.scene.fill(ppt_render::vello::peniko::Fill::NonZero, Affine::IDENTITY, blank.unwrap_or(PColor::BLACK), None, &area);
        if blank.is_none() && self.draw_anim(view) {
            return;
        }
        self.step_running = false;
        if let (None, Screen::Slide(i)) = (blank, show.screen())
            && let Some(slide) = self.editor.pres.slides.get(i)
        {
            let opts = DrawOptions { editing: false, emu_per_px: 1.0 / s, ..Default::default() };
            let frame = slide.timing.as_deref().map(|t| {
                let player = Player::new(t);
                let t = if self.step_animate { (js_sys::Date::now() - self.step_start_ms).max(0.0) } else { ANIM_END };
                self.step_running = self.step_animate && (player.running(show.step(), t) || js_sys::Date::now() < self.step_start_ms);
                player.frame(&SlideStage { pres: &self.editor.pres, slide }, show.step(), t)
            });
            let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
            match &frame {
                Some(f) => draw_slide_animated(&mut self.scene, &mut cx, &self.editor.pres, slide, view, &opts, f),
                None => draw_slide(&mut self.scene, &mut cx, &self.editor.pres, slide, view, &opts),
            }
        }
    }

    fn build_scene(&mut self) {
        self.scene.reset();
        let view = self.view();
        let s = self.scale();
        // The view above is computed while the show is set (edge-to-edge fit).
        if let Some(show) = self.show.take() {
            self.build_show_scene(&show, view, s);
            self.show = Some(show);
            return;
        }
        // A transition preview replaces the editing view while it plays.
        if self.draw_anim(view) {
            return;
        }
        // So does an animation preview.
        if let Some(start) = self.preview_start_ms {
            let pres = &self.editor.pres;
            let frame = pres.slides.get(self.editor.slide).and_then(|slide| {
                let player = Player::new(slide.timing.as_deref()?);
                player.preview_frame(&SlideStage { pres, slide }, js_sys::Date::now() - start).map(|f| (slide, f))
            });
            match frame {
                Some((slide, f)) => {
                    let opts = DrawOptions { editing: false, emu_per_px: 1.0 / s, ..Default::default() };
                    let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
                    draw_slide_animated(&mut self.scene, &mut cx, pres, slide, view, &opts, &f);
                    self.step_running = true;
                    return;
                }
                None => {
                    self.preview_start_ms = None;
                    self.step_running = false;
                }
            }
        }
        {
            let pres = &self.editor.pres;
            if let Some(slide) = pres.slides.get(self.editor.slide) {
                let opts = DrawOptions { skip_text: self.editor.text.as_ref().map(|t| t.shape), skip_cell: self.editor.text.as_ref().and_then(|t| t.cell), editing: true, emu_per_px: 1.0 / s, slide_number: None };
                let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
                draw_slide(&mut self.scene, &mut cx, pres, slide, view, &opts);
            }
        }

        // Live text of the shape being edited, drawn from the editor's own layout.
        let min_caret_w = 1.0 / (s * EMU_PER_PT);
        if let Some(v) = self.editor.text_view() {
            let tf = view * v.transform;
            draw_body_layout(&mut self.scene, v.layout, tf);
            let rect = |b: &ppt_text::parley::BoundingBox| Rect::new(b.x0, b.y0, b.x1, b.y1);
            let overlay = TextOverlay {
                selection: v.selection.iter().map(rect).collect(),
                caret: v.caret.as_ref().filter(|_| self.caret_on).map(|c| Rect::new(c.x0, c.y0, c.x0 + (c.x1 - c.x0).max(min_caret_w), c.y1)),
                compose: v.compose.iter().map(rect).collect(),
            };
            draw_text_overlay(&mut self.scene, &overlay, tf);
        }

        self.draw_spelling(view);
        self.draw_crop_view(view);
        self.draw_grid_and_guides(view);

        let to_dev = |(x, y): (f64, f64)| view * Point::new(x, y);
        let tol = self.tolerance();
        let single = self.editor.selected_shape().is_some();
        let editing = self.editor.is_editing_text();
        let handles = self.editor.handles(&tol);
        let frame_corners = |xf: &ppt_model::Xfrm| {
            let tf = view * shape_affine(xf, false);
            let (w, h) = (xf.w as f64, xf.h as f64);
            [tf * Point::new(0.0, 0.0), tf * Point::new(w, 0.0), tf * Point::new(w, h), tf * Point::new(0.0, h)]
        };
        // The entered group's frame, so it is clear which group is being edited.
        if let Some(frame) = self.editor.entered_group_frame() {
            let overlay = SelectionOverlay { corners: frame_corners(&frame), handles: Vec::new(), rotate: None, dpr: self.dpr, editing: true };
            draw_selection(&mut self.scene, &overlay);
        }
        // A selected group shows one frame with handles around all its members.
        if self.editor.selection_group().is_some()
            && let Some(frame) = self.editor.selection_frame()
        {
            let overlay = SelectionOverlay {
                corners: frame_corners(&frame),
                handles: handles.iter().map(|h| to_dev(h.1)).collect(),
                rotate: None,
                dpr: self.dpr,
                editing: false,
            };
            draw_selection(&mut self.scene, &overlay);
        }
        // Selected table cells.
        for frame in self.editor.cell_range_frames() {
            let rect = Rect::new(frame.x as f64, frame.y as f64, (frame.x + frame.w) as f64, (frame.y + frame.h) as f64);
            self.scene.fill(ppt_render::vello::peniko::Fill::NonZero, view, PColor::from_rgba8(0x43, 0x7E, 0xC9, 0x40), None, &rect);
        }
        let shapes = if self.editor.selection_group().is_some() { Vec::new() } else { self.editor.selected_shapes() };
        for shape in shapes {
            let xf = shape.xfrm;
            let corners = if shape.geometry.is_line() {
                let (w, h) = (xf.w as f64, xf.h as f64);
                let (a, b) = if xf.flip_h != xf.flip_v { ((w, 0.0), (0.0, h)) } else { ((0.0, 0.0), (w, h)) };
                let (a, b) = (to_dev(xf.to_slide(a.0, a.1)), to_dev(xf.to_slide(b.0, b.1)));
                [a, b, b, a]
            } else {
                let tf = view * shape_affine(&xf, false);
                let (w, h) = (xf.w as f64, xf.h as f64);
                [tf * Point::new(0.0, 0.0), tf * Point::new(w, 0.0), tf * Point::new(w, h), tf * Point::new(0.0, h)]
            };
            let show_handles = single && !editing;
            let overlay = SelectionOverlay {
                corners,
                handles: if show_handles { handles.iter().filter(|h| h.0 != Handle::Rotate).map(|h| to_dev(h.1)).collect() } else { Vec::new() },
                rotate: if show_handles { handles.iter().find(|h| h.0 == Handle::Rotate).map(|h| to_dev(h.1)) } else { None },
                dpr: self.dpr,
                editing,
            };
            draw_selection(&mut self.scene, &overlay);
        }

        let guides: Vec<(Point, Point)> = self
            .editor
            .guides
            .iter()
            .map(|g| if g.vertical { (to_dev((g.pos, g.from)), to_dev((g.pos, g.to))) } else { (to_dev((g.from, g.pos)), to_dev((g.to, g.pos))) })
            .collect();
        draw_guides(&mut self.scene, &guides, self.dpr);
        if let Some((x0, y0, x1, y1)) = self.editor.marquee {
            let (a, b) = (to_dev((x0, y0)), to_dev((x1, y1)));
            draw_marquee(&mut self.scene, Rect::from_points(a, b), self.dpr);
        }
    }

    fn reset_document(&mut self, pres: Presentation) {
        self.editor.load(pres);
        self.editor.refresh_tables(&mut self.engine);
        self.images.clear();
        self.zoom = 1.0;
        self.dirty = true;
    }
}

#[wasm_bindgen]
impl App {
    /// Heap in use and its peak since the last call, in bytes, plus decoded images: `[now, peak, images]`.
    #[wasm_bindgen(js_name = memoryStats)]
    pub fn memory_stats(&self) -> Vec<f64> {
        use std::sync::atomic::Ordering::Relaxed;
        let now = HEAP_NOW.load(Relaxed);
        let peak = HEAP_PEAK.swap(now, Relaxed);
        vec![now as f64, peak as f64, self.images.memory() as f64]
    }

    // --- Fonts ---------------------------------------------------------------

    /// Registers font bytes; returns the family names in the file.
    #[wasm_bindgen(js_name = registerFont)]
    pub fn register_font(&mut self, bytes: Vec<u8>) -> Array {
        let names = self.engine.register_font(bytes);
        // Table rows are measured with the fonts; new faces can change their heights.
        self.editor.refresh_tables(&mut self.engine);
        self.dirty = true;
        names.into_iter().map(JsValue::from).collect()
    }

    /// Runs asking for `requested` will use `actual`.
    #[wasm_bindgen(js_name = setFontAlias)]
    pub fn set_font_alias(&mut self, requested: &str, actual: &str) {
        self.engine.set_alias(requested, actual);
        self.editor.refresh_tables(&mut self.engine);
        self.dirty = true;
    }

    #[wasm_bindgen(js_name = setFallbackFonts)]
    pub fn set_fallback_fonts(&mut self, families: Vec<String>) {
        self.engine.set_fallbacks(families);
        self.dirty = true;
    }

    /// Font families referenced by the open deck (including layouts).
    #[wasm_bindgen(js_name = fontFamilies)]
    pub fn font_families(&self) -> Array {
        let fams = self.editor.pres.font_families();
        fams.into_iter().map(JsValue::from).collect()
    }

    // --- Documents -----------------------------------------------------------

    #[wasm_bindgen(js_name = loadPptx)]
    pub fn load_pptx(&mut self, bytes: Vec<u8>) -> Result<(), JsValue> {
        let pres = ppt_ooxml::parse_pptx_owned(bytes).map_err(js_err)?;
        if pres.slides.is_empty() {
            return Err(JsValue::from_str("簡報中沒有投影片"));
        }
        self.reset_document(pres);
        Ok(())
    }

    /// Starts a new deck from the built-in template.
    #[wasm_bindgen(js_name = newDeck)]
    pub fn new_deck(&mut self) {
        self.reset_document(template());
    }

    /// Serializes the deck as `.pptx` bytes (ends any text editing first).
    #[wasm_bindgen(js_name = savePptx)]
    pub fn save_pptx(&mut self) -> Result<Vec<u8>, JsValue> {
        self.editor.end_text_edit(&mut self.engine);
        self.dirty = true;
        ppt_ooxml::save_pptx(&self.editor.pres).map_err(js_err)
    }

    /// The deck as a PDF (slides hidden from the show are left out).
    #[wasm_bindgen(js_name = exportPdf)]
    pub fn export_pdf(&mut self) -> Result<Vec<u8>, JsValue> {
        self.editor.end_text_edit(&mut self.engine);
        self.dirty = true;
        let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
        ppt_pdf::export_pdf(&self.editor.pres, &mut cx, ppt_pdf::PdfOptions::default()).map_err(|e| JsValue::from_str(&e))
    }

    /// Increases on every document change; the shell compares it to know about unsaved edits.
    pub fn revision(&self) -> f64 {
        self.editor.revision() as f64
    }

    // --- View ----------------------------------------------------------------

    pub fn resize(&mut self, width: u32, height: u32, dpr: f64) {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height, dpr) == (self.width, self.height, self.dpr) {
            return;
        }
        self.width = width;
        self.height = height;
        self.dpr = dpr;
        self.gpu.cx.resize_surface(&mut self.gpu.surface, width, height);
        self.dirty = true;
    }

    #[wasm_bindgen(js_name = setZoom)]
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = zoom.clamp(0.1, 8.0);
        self.dirty = true;
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Toggles the caret; called by a JS timer.
    pub fn blink(&mut self) {
        if self.editor.is_editing_text() && !self.editor.is_composing() {
            self.caret_on = !self.caret_on;
            self.dirty = true;
        }
    }

    // --- Pointer & keyboard ----------------------------------------------------

    #[wasm_bindgen(js_name = pointerDown)]
    pub fn pointer_down(&mut self, x: f64, y: f64, shift: bool, alt: bool, meta: bool, clicks: u32) -> bool {
        let ev = self.pointer(x, y, shift, alt, meta, clicks);
        let tol = self.tolerance();
        let changed = self.editor.pointer_down(&mut self.engine, &ev, &tol);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = pointerMove)]
    pub fn pointer_move(&mut self, x: f64, y: f64, shift: bool, alt: bool, meta: bool) -> bool {
        let ev = self.pointer(x, y, shift, alt, meta, 0);
        let tol = self.tolerance();
        let changed = self.editor.pointer_move(&mut self.engine, &ev, &tol);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = pointerUp)]
    pub fn pointer_up(&mut self, x: f64, y: f64, shift: bool, alt: bool, meta: bool) -> bool {
        let ev = self.pointer(x, y, shift, alt, meta, 0);
        let changed = self.editor.pointer_up(&mut self.engine, &ev);
        self.touch(changed)
    }

    /// CSS cursor for the pointer position.
    pub fn cursor(&self, x: f64, y: f64) -> String {
        let ev = self.pointer(x, y, false, false, false, 0);
        match self.editor.hover_cursor(&ev, &self.tolerance()) {
            CursorIcon::Default => "default",
            CursorIcon::Move => "move",
            CursorIcon::Text => "text",
            CursorIcon::Rotate | CursorIcon::Crosshair => "crosshair",
            CursorIcon::Resize(angle) => match ((angle + 22.5) / 45.0).floor() as i32 % 4 {
                0 => "ew-resize",
                1 => "nwse-resize",
                2 => "ns-resize",
                _ => "nesw-resize",
            },
        }
        .to_string()
    }

    #[wasm_bindgen(js_name = keyDown)]
    pub fn key_down(&mut self, key: &str, shift: bool, alt: bool, meta: bool, ctrl: bool) -> bool {
        let handled = self.editor.key_down(&mut self.engine, key, Modifiers { shift, alt, meta, ctrl });
        self.touch(handled)
    }

    #[wasm_bindgen(js_name = insertText)]
    pub fn insert_text(&mut self, text: &str) -> bool {
        let changed = self.editor.insert_text(&mut self.engine, text);
        self.touch(changed)
    }

    /// IME preedit. `cursor_utf16` is the caret inside the preedit (DOM units); negative = end.
    #[wasm_bindgen(js_name = composeUpdate)]
    pub fn compose_update(&mut self, text: &str, cursor_utf16: i32) -> bool {
        let cursor = if cursor_utf16 < 0 { text.len() } else { utf16_to_byte_offset(text, cursor_utf16 as usize) };
        let changed = self.editor.compose_update(&mut self.engine, text, cursor);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = composeEnd)]
    pub fn compose_end(&mut self, text: &str) -> bool {
        let changed = self.editor.compose_end(&mut self.engine, text);
        self.touch(changed)
    }

    /// IME candidate anchor `[x, y, w, h]` in CSS pixels, or empty when not editing.
    #[wasm_bindgen(js_name = imeRect)]
    pub fn ime_rect(&self) -> Vec<f64> {
        let Some(v) = self.editor.text_view() else { return Vec::new() };
        let a = v.ime_area;
        let tf = self.view() * v.transform;
        let pts = [Point::new(a.x0, a.y0), Point::new(a.x1, a.y0), Point::new(a.x1, a.y1), Point::new(a.x0, a.y1)].map(|p| tf * p);
        let (x0, y0) = pts.iter().fold((f64::MAX, f64::MAX), |(x, y), p| (x.min(p.x), y.min(p.y)));
        let (x1, y1) = pts.iter().fold((f64::MIN, f64::MIN), |(x, y), p| (x.max(p.x), y.max(p.y)));
        let d = self.dpr;
        vec![x0 / d, y0 / d, (x1 - x0) / d, (y1 - y0) / d]
    }

    // --- Spelling ----------------------------------------------------------------

    /// The current slide's texts to spell check: `[{ key, text }]`.
    #[wasm_bindgen(js_name = spellTexts)]
    pub fn spell_texts(&self) -> Array {
        self.editor
            .slide_texts()
            .into_iter()
            .filter(|(_, t)| t.chars().any(|c| c.is_alphabetic()))
            .map(|(place, text)| {
                let o = Object::new();
                let _ = Reflect::set(&o, &"key".into(), &spell_key(place).into());
                let _ = Reflect::set(&o, &"text".into(), &text.into());
                JsValue::from(o)
            })
            .collect()
    }

    /// Records the misspellings found in `text` (the text of `key` when it was
    /// checked): `[start, length, ...]` in UTF-16 units.
    #[wasm_bindgen(js_name = setSpelling)]
    pub fn set_spelling(&mut self, key: &str, text: &str, ranges: Vec<u32>) {
        let bytes: Vec<std::ops::Range<usize>> = ranges.chunks_exact(2).map(|r| utf16_to_byte_offset(text, r[0] as usize)..utf16_to_byte_offset(text, (r[0] + r[1]) as usize)).collect();
        let changed = match self.spelling.get(key) {
            Some((t, old)) => t != text || *old != bytes,
            None => !bytes.is_empty(),
        };
        if bytes.is_empty() {
            self.spelling.remove(key);
        } else {
            self.spelling.insert(key.to_string(), (text.to_string(), bytes));
        }
        self.dirty |= changed;
    }

    /// Forgets every misspelling (spell checking turned off).
    #[wasm_bindgen(js_name = clearSpelling)]
    pub fn clear_spelling(&mut self) {
        self.dirty |= !self.spelling.is_empty();
        self.spelling.clear();
    }

    /// The misspelled word under a point (CSS pixels): `{ key, text, start, length, word }`
    /// (UTF-16 units) or null.
    #[wasm_bindgen(js_name = spellingAt)]
    pub fn spelling_at(&mut self, x: f64, y: f64) -> JsValue {
        let view = self.view();
        let p = Point::new(x * self.dpr, y * self.dpr);
        for (place, text, ranges) in self.current_spelling() {
            let Some((layout, tf)) = self.editor.text_geometry(&mut self.engine, place) else { continue };
            let local = (view * tf).inverse() * p;
            for r in ranges {
                let hit = ppt_editor::range_rects(&layout, r.clone()).iter().any(|b| local.x >= b.x0 - 1.0 && local.x <= b.x1 + 1.0 && local.y >= b.y0 && local.y <= b.y1 + 2.0);
                if hit {
                    let u16 = |b: usize| text[..b].encode_utf16().count() as u32;
                    let o = Object::new();
                    let _ = Reflect::set(&o, &"key".into(), &spell_key(place).into());
                    let _ = Reflect::set(&o, &"text".into(), &text.as_str().into());
                    let _ = Reflect::set(&o, &"start".into(), &u16(r.start).into());
                    let _ = Reflect::set(&o, &"length".into(), &(u16(r.end) - u16(r.start)).into());
                    let _ = Reflect::set(&o, &"word".into(), &text[r.clone()].into());
                    return o.into();
                }
            }
        }
        JsValue::NULL
    }

    /// Replaces `length` UTF-16 units at `start` of text `key` (a correction), as an edit there.
    #[wasm_bindgen(js_name = replaceSpelling)]
    pub fn replace_spelling(&mut self, key: &str, start: u32, length: u32, with: &str) -> bool {
        let Some(place) = parse_spell_key(key) else { return false };
        let Some((_, text)) = self.editor.slide_texts().into_iter().find(|(p, _)| *p == place) else { return false };
        let range = utf16_to_byte_offset(&text, start as usize)..utf16_to_byte_offset(&text, (start + length) as usize);
        let m = FindMatch { slide: self.editor.slide, place, range };
        let changed = self.editor.replace_range(&mut self.engine, &m, with);
        self.touch(changed)
    }

    // --- Find and replace ---------------------------------------------------------

    /// Selects the next (or previous) match of `query` after the current
    /// position: the edited text's selection, or the notes selection
    /// (`notes_start`/`notes_end` in UTF-16 units, -1 when the notes aren't
    /// involved). Returns `{ index, total, slide, notes, start, end }` (`start`/`end`
    /// in UTF-16 units of the notes text, for notes matches) or null.
    #[wasm_bindgen(js_name = findNext)]
    pub fn find_next(&mut self, query: &str, match_case: bool, whole_word: bool, backwards: bool, notes_start: i32, notes_end: i32) -> JsValue {
        let opts = FindOptions { match_case, whole_word };
        let notes = self.notes_range(notes_start, notes_end);
        let Some((index, all)) = self.editor.find_next(query, opts, backwards, notes) else { return JsValue::NULL };
        self.show_match(index, &all)
    }

    /// How many matches `query` has in the deck.
    #[wasm_bindgen(js_name = findCount)]
    pub fn find_count(&self, query: &str, match_case: bool, whole_word: bool) -> u32 {
        self.editor.find_all(query, FindOptions { match_case, whole_word }).len() as u32
    }

    /// Replaces the selected match (if the selection is one) and selects the
    /// next; returns it like `findNext`.
    #[wasm_bindgen(js_name = replaceNext)]
    pub fn replace_next(&mut self, query: &str, with: &str, match_case: bool, whole_word: bool, notes_start: i32, notes_end: i32) -> JsValue {
        let opts = FindOptions { match_case, whole_word };
        let notes = self.notes_range(notes_start, notes_end);
        let current = match &notes {
            Some(r) => Some(FindMatch { slide: self.editor.slide, place: FindPlace::Notes, range: r.clone() }),
            None => self.editor.text.as_ref().map(|t| FindMatch {
                slide: self.editor.slide,
                place: match t.cell {
                    Some((r, c)) => FindPlace::Cell(t.shape, r, c),
                    None => FindPlace::Shape(t.shape),
                },
                range: t.rich.range(),
            }),
        };
        let mut after_notes = notes;
        if let Some(m) = current
            && self.editor.replace_match(&mut self.engine, &m, query, with, opts)
        {
            self.dirty = true;
            // Continue after the replacement.
            let end = m.range.start + with.len();
            after_notes = (m.place == FindPlace::Notes).then_some(end..end);
            if let Some(t) = &mut self.editor.text {
                t.rich.select_range(end..end);
            }
        }
        let Some((index, all)) = self.editor.find_next(query, opts, false, after_notes) else { return JsValue::NULL };
        self.show_match(index, &all)
    }

    /// Replaces every match as one undo step; returns how many.
    #[wasm_bindgen(js_name = replaceAll)]
    pub fn replace_all(&mut self, query: &str, with: &str, match_case: bool, whole_word: bool) -> u32 {
        let n = self.editor.replace_all(&mut self.engine, query, with, FindOptions { match_case, whole_word });
        self.touch(n > 0);
        n as u32
    }

    /// Misspellings of the current slide's texts that still match what was checked.
    fn current_spelling(&self) -> Vec<(FindPlace, String, Vec<std::ops::Range<usize>>)> {
        if self.spelling.is_empty() {
            return Vec::new();
        }
        self.editor
            .slide_texts()
            .into_iter()
            .filter_map(|(place, text)| {
                let (checked, ranges) = self.spelling.get(&spell_key(place))?;
                (*checked == text).then(|| (place, text, ranges.clone()))
            })
            .collect()
    }

    /// Crop mode: the trimmed-away parts of the picture, dimmed, and the whole image's outline.
    fn draw_crop_view(&mut self, view: Affine) {
        let Some((id, full)) = self.editor.crop_view() else { return };
        let pres = &self.editor.pres;
        let Some(shape) = pres.shape(self.editor.slide, id) else { return };
        let ppt_model::ShapeKind::Picture { media, crop } = &shape.kind else { return };
        let (fw, fh) = (full.w as f64, full.h as f64);
        let shown = Rect::new(crop.left * fw, crop.top * fh, crop.left * fw + shape.xfrm.w as f64, crop.top * fh + shape.xfrm.h as f64);
        use ppt_render::vello::kurbo::Shape as _;
        let mut ring = Rect::new(0.0, 0.0, fw, fh).to_path(0.1);
        ring.extend(shown.to_path(0.1));
        let tf = view * shape_affine(&full, false);
        let ghost = ppt_model::Shape { xfrm: full, kind: ppt_model::ShapeKind::Picture { media: media.clone(), crop: ppt_model::Crop::default() }, line: None, shadow: None, geometry: ppt_model::Geometry::preset("rect"), ..shape.clone() };
        self.scene.push_layer(ppt_render::vello::peniko::Fill::EvenOdd, ppt_render::vello::peniko::BlendMode::from(ppt_render::vello::peniko::Mix::Normal), 0.35, tf, &ring);
        let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
        draw_shape(&mut self.scene, &mut cx, pres, &ghost, view, &DrawOptions::default());
        self.scene.pop_layer();
        // The whole image's outline, dashed in device pixels.
        let d = self.dpr;
        let mut edge = Rect::new(0.0, 0.0, fw, fh).to_path(0.1);
        edge.apply_affine(tf);
        let outline = ppt_render::vello::kurbo::Stroke::new(d).with_dashes(0.0, [4.0 * d, 3.0 * d]);
        self.scene.stroke(&outline, Affine::IDENTITY, PColor::from_rgba8(0x40, 0x40, 0x40, 255), None, &edge);
    }

    /// Red wavy lines under misspelled words.
    fn draw_spelling(&mut self, view: Affine) {
        let d = self.dpr;
        for (place, _, ranges) in self.current_spelling() {
            let Some((layout, tf)) = self.editor.text_geometry(&mut self.engine, place) else { continue };
            let tf = view * tf;
            for r in ranges {
                for b in ppt_editor::range_rects(&layout, r) {
                    let (a, z) = (tf * Point::new(b.x0, b.y1), tf * Point::new(b.x1, b.y1));
                    let len = (z - a).hypot();
                    if len < 1.0 {
                        continue;
                    }
                    let dir = (z - a) / len;
                    let down = Vec2::new(-dir.y, dir.x);
                    let mut path = ppt_render::vello::kurbo::BezPath::new();
                    let (step, amp) = (2.0 * d, 1.0 * d);
                    let mut t = 0.0;
                    let mut k = 0;
                    while t <= len {
                        let pt = a + dir * t + down * (if k % 2 == 0 { 0.0 } else { amp } + amp);
                        if k == 0 { path.move_to(pt) } else { path.line_to(pt) }
                        t += step;
                        k += 1;
                    }
                    self.scene.stroke(&ppt_render::vello::kurbo::Stroke::new(1.0 * d), Affine::IDENTITY, PColor::from_rgba8(0xE0, 0x24, 0x24, 255), None, &path);
                }
            }
        }
    }

    fn notes_range(&self, start: i32, end: i32) -> Option<std::ops::Range<usize>> {
        if start < 0 || end < start {
            return None;
        }
        let notes = self.editor.notes(self.editor.slide);
        Some(utf16_to_byte_offset(notes, start as usize)..utf16_to_byte_offset(notes, end as usize))
    }

    fn show_match(&mut self, index: usize, all: &[FindMatch]) -> JsValue {
        let m = &all[index];
        self.preview_start_ms = None;
        self.editor.select_match(&mut self.engine, m);
        self.dirty = true;
        self.caret_on = true;
        let o = Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = Reflect::set(&o, &k.into(), &v);
        };
        set("index", (index as u32).into());
        set("total", (all.len() as u32).into());
        set("slide", (m.slide as u32).into());
        set("notes", (m.place == FindPlace::Notes).into());
        if m.place == FindPlace::Notes {
            let notes = self.editor.notes(m.slide);
            let u16 = |b: usize| notes[..b].encode_utf16().count() as u32;
            set("start", u16(m.range.start).into());
            set("end", u16(m.range.end).into());
        }
        o.into()
    }

    // --- Clipboard -------------------------------------------------------------

    #[wasm_bindgen(js_name = selectedText)]
    pub fn selected_text(&self) -> Option<String> {
        self.editor.selected_text()
    }

    #[wasm_bindgen(js_name = deleteTextSelection)]
    pub fn delete_text_selection(&mut self) -> bool {
        let changed = self.editor.delete_text_selection(&mut self.engine);
        self.touch(changed)
    }

    /// Copies the selected shapes; returns how many.
    #[wasm_bindgen(js_name = copyShapes)]
    pub fn copy_shapes(&mut self) -> u32 {
        self.editor.copy() as u32
    }

    #[wasm_bindgen(js_name = cutShapes)]
    pub fn cut_shapes(&mut self) -> bool {
        let changed = self.editor.cut();
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = pasteShapes)]
    pub fn paste_shapes(&mut self) -> bool {
        let changed = self.editor.paste(&mut self.engine);
        self.touch(changed)
    }

    /// Pastes plain text as a new text box.
    #[wasm_bindgen(js_name = pasteTextBox)]
    pub fn paste_text_box(&mut self, text: &str) -> bool {
        let changed = self.editor.paste_text_box(&mut self.engine, text);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = isEditingText)]
    pub fn is_editing_text(&self) -> bool {
        self.editor.is_editing_text()
    }

    // --- Formatting --------------------------------------------------------------

    /// `which`: "bold" | "italic" | "underline".
    pub fn toggle(&mut self, which: &str) -> bool {
        let t = match which {
            "bold" => Toggle::Bold,
            "italic" => Toggle::Italic,
            "strike" => Toggle::Strike,
            "superscript" => Toggle::Superscript,
            "subscript" => Toggle::Subscript,
            _ => Toggle::Underline,
        };
        let changed = self.editor.toggle_style(&mut self.engine, t);
        self.touch(changed)
    }

    /// Extra space between characters, in points (0 is normal).
    #[wasm_bindgen(js_name = setCharSpacing)]
    pub fn set_char_spacing(&mut self, pt: f32) -> bool {
        let pt = pt.clamp(-20.0, 100.0);
        let changed = self.editor.apply_text_style(&mut self.engine, |s| s.spacing_pt = pt);
        self.touch(changed)
    }

    /// Character formatting back to what the text inherits.
    #[wasm_bindgen(js_name = clearFormatting)]
    pub fn clear_formatting(&mut self) -> bool {
        let changed = self.editor.clear_formatting(&mut self.engine);
        self.touch(changed)
    }

    /// Vertical position of the text in its shape: "top", "middle" or "bottom".
    #[wasm_bindgen(js_name = setTextAnchor)]
    pub fn set_text_anchor(&mut self, anchor: &str) -> bool {
        let a = match anchor {
            "middle" => ppt_model::Anchor::Middle,
            "bottom" => ppt_model::Anchor::Bottom,
            _ => ppt_model::Anchor::Top,
        };
        let changed = self.editor.set_text_anchor(a);
        self.touch(changed)
    }

    /// Applies layout `index` to the current slide.
    #[wasm_bindgen(js_name = applyLayout)]
    pub fn apply_layout(&mut self, index: usize) -> bool {
        let changed = self.editor.apply_layout(&mut self.engine, index, false);
        self.touch(changed)
    }

    /// Resets the current slide's placeholders to its layout.
    #[wasm_bindgen(js_name = resetSlide)]
    pub fn reset_slide(&mut self) -> bool {
        let changed = self.editor.reset_slide(&mut self.engine);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setFontSize)]
    pub fn set_font_size(&mut self, pt: f32) -> bool {
        let pt = pt.clamp(1.0, 400.0);
        let changed = self.editor.apply_text_style(&mut self.engine, |s| s.size_pt = pt);
        self.touch(changed)
    }

    /// Sets the font for Latin and East Asian text alike.
    #[wasm_bindgen(js_name = setFontFamily)]
    pub fn set_font_family(&mut self, family: &str) -> bool {
        let f = family.to_string();
        let changed = self.editor.apply_text_style(&mut self.engine, |s| {
            s.latin_font = f.clone();
            s.ea_font = f.clone();
        });
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setTextColor)]
    pub fn set_text_color(&mut self, color: &str) -> bool {
        let Some(c) = parse_hex(color) else { return false };
        let changed = self.editor.apply_text_style(&mut self.engine, |s| s.color = c);
        self.touch(changed)
    }

    /// Links the selected text to `url`; an empty string removes links.
    #[wasm_bindgen(js_name = setLink)]
    pub fn set_link(&mut self, url: &str) -> bool {
        let changed = self.editor.set_link(&mut self.engine, Some(url).filter(|u| !u.is_empty()));
        self.touch(changed)
    }

    /// `align`: "left" | "center" | "right" | "justify".
    #[wasm_bindgen(js_name = setAlign)]
    pub fn set_align(&mut self, align: &str) -> bool {
        let a = match align {
            "center" => Align::Center,
            "right" => Align::Right,
            "justify" => Align::Justify,
            _ => Align::Left,
        };
        let changed = self.editor.apply_paragraph(&mut self.engine, |p| p.align = a);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setLineSpacing)]
    pub fn set_line_spacing(&mut self, multiple: f32) -> bool {
        let changed = self.editor.apply_paragraph(&mut self.engine, |p| p.line_spacing = multiple);
        self.touch(changed)
    }

    /// Numbers the selected paragraphs (`arabicPeriod`, `romanUcPeriod`, ...) or removes numbering.
    #[wasm_bindgen(js_name = toggleNumbering)]
    pub fn toggle_numbering(&mut self, scheme: &str) -> bool {
        let changed = self.editor.toggle_numbering(&mut self.engine, scheme);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = toggleBullets)]
    pub fn toggle_bullets(&mut self) -> bool {
        let changed = self.editor.toggle_bullets(&mut self.engine);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = changeLevel)]
    pub fn change_level(&mut self, delta: i32) -> bool {
        let changed = self.editor.change_level(&mut self.engine, delta);
        self.touch(changed)
    }

    /// Shape fill; empty string = no fill.
    #[wasm_bindgen(js_name = setFill)]
    pub fn set_fill(&mut self, color: &str) -> bool {
        let changed = self.editor.set_fill(parse_hex(color));
        self.touch(changed)
    }

    /// Outline color; empty string = no outline.
    #[wasm_bindgen(js_name = setLineColor)]
    pub fn set_line_color(&mut self, color: &str) -> bool {
        let changed = self.editor.set_line_color(parse_hex(color));
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setLineWidth)]
    pub fn set_line_width(&mut self, pt: f64) -> bool {
        let changed = self.editor.set_line_width((pt * EMU_PER_PT).round() as i64);
        self.touch(changed)
    }

    // --- Arrange & insert ----------------------------------------------------------

    /// `order`: "front" | "forward" | "backward" | "back".
    pub fn reorder(&mut self, order: &str) -> bool {
        let o = match order {
            "front" => ZOrder::Front,
            "forward" => ZOrder::Forward,
            "backward" => ZOrder::Backward,
            _ => ZOrder::Back,
        };
        let changed = self.editor.reorder(o);
        self.touch(changed)
    }

    /// `to`: "left" | "center" | "right" | "top" | "middle" | "bottom".
    #[wasm_bindgen(js_name = alignShapes)]
    pub fn align_shapes(&mut self, to: &str) -> bool {
        let t = match to {
            "left" => AlignTo::Left,
            "center" => AlignTo::Center,
            "right" => AlignTo::Right,
            "top" => AlignTo::Top,
            "middle" => AlignTo::Middle,
            _ => AlignTo::Bottom,
        };
        let changed = self.editor.align(t);
        self.touch(changed)
    }

    pub fn undo(&mut self) -> bool {
        let changed = self.editor.undo(&mut self.engine);
        self.touch(changed)
    }

    pub fn redo(&mut self) -> bool {
        let changed = self.editor.redo(&mut self.engine);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = deleteSelected)]
    pub fn delete_selected(&mut self) -> bool {
        let changed = self.editor.delete_selected();
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = duplicateSelected)]
    pub fn duplicate_selected(&mut self) -> bool {
        let changed = self.editor.duplicate_selected();
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = addTextBox)]
    pub fn add_text_box(&mut self) -> bool {
        let changed = self.editor.add_text_box(&mut self.engine);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = addShape)]
    pub fn add_shape(&mut self, preset: &str) -> bool {
        let changed = self.editor.add_shape(&mut self.engine, preset);
        self.touch(changed)
    }

    /// Moves and resizes the selection (EMU; rotation in degrees, single shapes only).
    #[wasm_bindgen(js_name = setFrame)]
    pub fn set_frame(&mut self, x: f64, y: f64, w: f64, h: f64, rot: f64) -> bool {
        let to = ppt_model::Xfrm { rot: rot.rem_euclid(360.0), ..ppt_model::Xfrm::new(x.round() as i64, y.round() as i64, w.round() as i64, h.round() as i64) };
        let changed = self.editor.set_selection_frame(to);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = rotateSelection)]
    pub fn rotate_selection(&mut self, degrees: f64) -> bool {
        let changed = self.editor.rotate_selection(degrees);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = flipSelection)]
    pub fn flip_selection(&mut self, horizontal: bool) -> bool {
        let changed = self.editor.flip_selection(horizontal);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = changeShape)]
    pub fn change_shape(&mut self, preset: &str) -> bool {
        let changed = self.editor.change_shape(preset);
        self.touch(changed)
    }

    /// Applies a shadow preset (`SHADOWS`) or "none".
    #[wasm_bindgen(js_name = setShadow)]
    pub fn set_shadow(&mut self, preset: &str) -> bool {
        let shadow = SHADOWS.iter().find(|(name, ..)| *name == preset).map(|&(_, dir, dist, blur)| ppt_model::Shadow { color: Color { r: 0, g: 0, b: 0, a: 102 }, blur, dist, dir });
        let changed = self.editor.set_shadow(shadow);
        self.touch(changed)
    }

    /// Starts or ends crop mode on the selected picture.
    #[wasm_bindgen(js_name = setCropMode)]
    pub fn set_crop_mode(&mut self, on: bool) -> bool {
        let changed = self.editor.set_crop_mode(on);
        self.touch(changed)
    }

    /// Crops the selected picture (fractions trimmed from each side).
    #[wasm_bindgen(js_name = setCrop)]
    pub fn set_crop(&mut self, left: f64, top: f64, right: f64, bottom: f64) -> bool {
        let changed = self.editor.set_crop(ppt_model::Crop { left, top, right, bottom });
        self.touch(changed)
    }

    /// Removes the selected picture's cropping, rotation and flips, at the image's own proportions.
    #[wasm_bindgen(js_name = resetPicture)]
    pub fn reset_picture(&mut self) -> bool {
        let pixels = self.editor.selected_shape().and_then(|s| match &s.kind {
            ppt_model::ShapeKind::Picture { media: Some(m), .. } => self.editor.pres.media.get(m).and_then(|b| image_size(&b)),
            _ => None,
        });
        let changed = self.editor.reset_picture(pixels);
        self.touch(changed)
    }

    /// Picks the shape the next drag on the slide draws ("" cancels).
    #[wasm_bindgen(js_name = setDrawTool)]
    pub fn set_draw_tool(&mut self, tool: &str) {
        self.editor.set_draw_tool(&mut self.engine, (!tool.is_empty()).then_some(tool));
        self.dirty = true;
    }

    /// The picked drawing tool, or "".
    #[wasm_bindgen(js_name = drawTool)]
    pub fn draw_tool(&self) -> String {
        self.editor.draw_tool.clone().unwrap_or_default()
    }

    /// Gallery icons for preset shapes: `[{ name, viewBox, paths: [{ d, fill, stroke }] }]`.
    #[wasm_bindgen(js_name = presetIcons)]
    pub fn preset_icons(&self, names: Vec<String>) -> Array {
        use ppt_render::vello::kurbo::Shape as _;
        names
            .iter()
            .map(|name| {
                let preset = if name == ppt_editor::ARROW_LINE { "line" } else { name.as_str() };
                let (w, h) = if preset == "line" { (20.0, 20.0) } else { (20.0, 16.0) };
                let outlines = ppt_geometry::outlines(&ppt_model::Geometry::preset(preset), w, h);
                let mut bounds = Rect::new(0.0, 0.0, w, h);
                let paths = Array::new();
                for o in &outlines {
                    bounds = bounds.union(o.path.bounding_box());
                    let p = Object::new();
                    let _ = Reflect::set(&p, &"d".into(), &o.path.to_svg().into());
                    let _ = Reflect::set(&p, &"fill".into(), &o.fill.into());
                    let _ = Reflect::set(&p, &"stroke".into(), &o.stroke.into());
                    paths.push(&p);
                }
                let pad = 1.5;
                let o = Object::new();
                let _ = Reflect::set(&o, &"name".into(), &name.as_str().into());
                let _ = Reflect::set(&o, &"viewBox".into(), &format!("{} {} {} {}", bounds.x0 - pad, bounds.y0 - pad, bounds.width() + 2.0 * pad, bounds.height() + 2.0 * pad).into());
                let _ = Reflect::set(&o, &"paths".into(), &paths);
                JsValue::from(o)
            })
            .collect()
    }

    /// The current slide's footer items: `{ date: "none" | "auto" | "fixed", dateText, slideNumber, footer: string | null }`.
    #[wasm_bindgen(js_name = headerFooter)]
    pub fn header_footer(&self) -> Object {
        let hf = self.editor.header_footer();
        let o = Object::new();
        let (mode, text) = match &hf.date {
            None => ("none", String::new()),
            Some(ppt_editor::DateText::Auto(t)) => ("auto", t.clone()),
            Some(ppt_editor::DateText::Fixed(t)) => ("fixed", t.clone()),
        };
        let _ = Reflect::set(&o, &"date".into(), &mode.into());
        let _ = Reflect::set(&o, &"dateText".into(), &text.into());
        let _ = Reflect::set(&o, &"slideNumber".into(), &hf.slide_number.into());
        let _ = Reflect::set(&o, &"footer".into(), &hf.footer.map_or(JsValue::NULL, JsValue::from));
        o
    }

    /// Shows the date (`date_mode` "none", "auto" or "fixed"), slide number and
    /// footer on the current slide, or on every slide with `all`.
    #[wasm_bindgen(js_name = setHeaderFooter)]
    #[allow(clippy::too_many_arguments)]
    pub fn set_header_footer(&mut self, date_mode: &str, date_text: &str, slide_number: bool, footer: bool, footer_text: &str, skip_title: bool, all: bool) -> bool {
        let date = match date_mode {
            "auto" => Some(ppt_editor::DateText::Auto(date_text.to_string())),
            "fixed" => Some(ppt_editor::DateText::Fixed(date_text.to_string())),
            _ => None,
        };
        let hf = ppt_editor::HeaderFooter { date, slide_number, footer: footer.then(|| footer_text.to_string()), skip_title };
        let changed = self.editor.set_header_footer(&mut self.engine, &hf, all);
        self.touch(changed)
    }

    // --- Accessibility ----------------------------------------------------------------

    /// The current slide for screen readers: `{ description, items: [{ id, role, label,
    /// altText, paragraphs: [{ text, level, bullet }], cells, rect: [x, y, w, h] (CSS px),
    /// selected }] }` in reading order.
    #[wasm_bindgen(js_name = a11ySlide)]
    pub fn a11y_slide(&self) -> Object {
        let view = self.view();
        let items: Array = self
            .editor
            .a11y_items()
            .into_iter()
            .map(|i| {
                let o = Object::new();
                let role = match i.role {
                    ppt_editor::A11yRole::Heading => "heading",
                    ppt_editor::A11yRole::Text => "text",
                    ppt_editor::A11yRole::Image => "image",
                    ppt_editor::A11yRole::Table => "table",
                    ppt_editor::A11yRole::Chart => "chart",
                    ppt_editor::A11yRole::Shape => "shape",
                };
                let tf = view * shape_affine(&i.xfrm, false);
                let r = tf.transform_rect_bbox(Rect::new(0.0, 0.0, i.xfrm.w.max(1) as f64, i.xfrm.h.max(1) as f64));
                let rect: Array = [r.x0, r.y0, r.width(), r.height()].iter().map(|v| JsValue::from(v / self.dpr)).collect();
                let paragraphs: Array = i
                    .paragraphs
                    .iter()
                    .map(|p| {
                        let po = Object::new();
                        let _ = Reflect::set(&po, &"text".into(), &p.text.clone().into());
                        let _ = Reflect::set(&po, &"level".into(), &p.level.into());
                        let _ = Reflect::set(&po, &"bullet".into(), &p.bullet.into());
                        JsValue::from(po)
                    })
                    .collect();
                let cells: Array = i.cells.iter().map(|row| JsValue::from(row.iter().map(|c| JsValue::from(c.clone())).collect::<Array>())).collect();
                let _ = Reflect::set(&o, &"id".into(), &i.id.0.into());
                let _ = Reflect::set(&o, &"role".into(), &role.into());
                let _ = Reflect::set(&o, &"label".into(), &i.label.into());
                let _ = Reflect::set(&o, &"altText".into(), &i.alt_text.into());
                let _ = Reflect::set(&o, &"paragraphs".into(), &paragraphs);
                let _ = Reflect::set(&o, &"cells".into(), &cells);
                let _ = Reflect::set(&o, &"rect".into(), &rect);
                let _ = Reflect::set(&o, &"selected".into(), &i.selected.into());
                JsValue::from(o)
            })
            .collect();
        let o = Object::new();
        let _ = Reflect::set(&o, &"description".into(), &self.editor.describe_slide().into());
        let _ = Reflect::set(&o, &"items".into(), &items);
        o
    }

    /// Accessibility problems: `[{ slide, shape (id or null), severity: "error" | "warning" | "tip",
    /// kind, message }]`, by slide.
    #[wasm_bindgen(js_name = checkAccessibility)]
    pub fn check_accessibility(&self) -> Array {
        self.editor
            .check_accessibility()
            .into_iter()
            .map(|i| {
                let o = Object::new();
                let severity = match i.severity {
                    ppt_editor::Severity::Error => "error",
                    ppt_editor::Severity::Warning => "warning",
                    ppt_editor::Severity::Tip => "tip",
                };
                let kind = match i.kind {
                    ppt_editor::IssueKind::MissingAltText => "altText",
                    ppt_editor::IssueKind::MissingTitle => "title",
                    ppt_editor::IssueKind::DuplicateTitle => "duplicateTitle",
                    ppt_editor::IssueKind::LowContrast => "contrast",
                    ppt_editor::IssueKind::NoHeaderRow => "headerRow",
                    ppt_editor::IssueKind::ReadingOrder => "readingOrder",
                };
                let _ = Reflect::set(&o, &"slide".into(), &(i.slide as u32).into());
                let _ = Reflect::set(&o, &"shape".into(), &i.shape.map_or(JsValue::NULL, |s| s.0.into()));
                let _ = Reflect::set(&o, &"severity".into(), &severity.into());
                let _ = Reflect::set(&o, &"kind".into(), &kind.into());
                let _ = Reflect::set(&o, &"message".into(), &i.message.into());
                JsValue::from(o)
            })
            .collect()
    }

    /// What is selected or edited, in one sentence ("" when nothing is).
    #[wasm_bindgen(js_name = describeSelection)]
    pub fn describe_selection(&self) -> String {
        self.editor.describe_selection()
    }

    #[wasm_bindgen(js_name = describeSlide)]
    pub fn describe_slide(&self) -> String {
        self.editor.describe_slide()
    }

    /// Accessible names of all slides: "投影片 2：標題" (plus "，已隱藏").
    #[wasm_bindgen(js_name = slideLabels)]
    pub fn slide_labels(&self) -> Array {
        self.editor
            .pres
            .slides
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let title = ppt_editor::slide_title(s);
                let mut label = if title.is_empty() { format!("投影片 {}", i + 1) } else { format!("投影片 {}：{title}", i + 1) };
                if s.hidden {
                    label.push_str("，已隱藏");
                }
                JsValue::from(label)
            })
            .collect()
    }

    /// Selection pane rows, top-most first:
    /// `[{ id, name, kind, depth, hidden, selected, altText, decorative }]`.
    #[wasm_bindgen(js_name = paneItems)]
    pub fn pane_items(&self) -> Array {
        self.editor
            .pane_items()
            .into_iter()
            .map(|i| {
                let o = Object::new();
                let _ = Reflect::set(&o, &"id".into(), &i.id.0.into());
                let _ = Reflect::set(&o, &"name".into(), &i.name.into());
                let _ = Reflect::set(&o, &"kind".into(), &i.kind.into());
                let _ = Reflect::set(&o, &"depth".into(), &i.depth.into());
                let _ = Reflect::set(&o, &"hidden".into(), &i.hidden.into());
                let _ = Reflect::set(&o, &"selected".into(), &i.selected.into());
                let _ = Reflect::set(&o, &"altText".into(), &i.alt_text.into());
                let _ = Reflect::set(&o, &"decorative".into(), &i.decorative.into());
                JsValue::from(o)
            })
            .collect()
    }

    #[wasm_bindgen(js_name = selectInPane)]
    pub fn select_in_pane(&mut self, id: u32, additive: bool) -> bool {
        let changed = self.editor.select_in_pane(&mut self.engine, ppt_model::ShapeId(id), additive);
        self.dirty |= changed;
        changed
    }

    #[wasm_bindgen(js_name = setShapeHidden)]
    pub fn set_shape_hidden(&mut self, id: u32, hidden: bool) -> bool {
        let changed = self.editor.set_shape_hidden(&mut self.engine, ppt_model::ShapeId(id), hidden);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setAllHidden)]
    pub fn set_all_hidden(&mut self, hidden: bool) -> bool {
        let changed = self.editor.set_all_hidden(&mut self.engine, hidden);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = renameShape)]
    pub fn rename_shape(&mut self, id: u32, name: &str) -> bool {
        let changed = self.editor.rename_shape(&mut self.engine, ppt_model::ShapeId(id), name);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = moveInPane)]
    pub fn move_in_pane(&mut self, id: u32, up: bool) -> bool {
        let changed = self.editor.move_in_pane(&mut self.engine, ppt_model::ShapeId(id), up);
        self.touch(changed)
    }

    /// The selected shape's `{ text, decorative }`, or null without a single selection.
    #[wasm_bindgen(js_name = altText)]
    pub fn alt_text(&self) -> JsValue {
        let Some((text, decorative)) = self.editor.alt_text() else { return JsValue::NULL };
        let o = Object::new();
        let _ = Reflect::set(&o, &"text".into(), &text.into());
        let _ = Reflect::set(&o, &"decorative".into(), &decorative.into());
        o.into()
    }

    #[wasm_bindgen(js_name = setAltText)]
    pub fn set_alt_text(&mut self, text: &str, decorative: bool) -> bool {
        let changed = self.editor.set_alt_text(&mut self.engine, text, decorative);
        self.touch(changed)
    }

    // --- View -------------------------------------------------------------------------

    /// `{ showGrid, showGuides, snapToGrid, gridSpacing (EMU), guides (count) }`.
    #[wasm_bindgen(js_name = viewSettings)]
    pub fn view_settings(&self) -> Object {
        let v = &self.editor.pres.view;
        let o = Object::new();
        let _ = Reflect::set(&o, &"showGrid".into(), &self.show_grid.into());
        let _ = Reflect::set(&o, &"showGuides".into(), &v.show_guides.into());
        let _ = Reflect::set(&o, &"snapToGrid".into(), &v.snap_to_grid.into());
        let _ = Reflect::set(&o, &"gridSpacing".into(), &(v.grid_spacing as f64).into());
        let _ = Reflect::set(&o, &"guides".into(), &(v.guides.len() as u32).into());
        o
    }

    #[wasm_bindgen(js_name = setShowGrid)]
    pub fn set_show_grid(&mut self, show: bool) {
        self.show_grid = show;
        self.dirty = true;
    }

    #[wasm_bindgen(js_name = setShowGuides)]
    pub fn set_show_guides(&mut self, show: bool) -> bool {
        let changed = self.editor.set_show_guides(&mut self.engine, show);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setSnapToGrid)]
    pub fn set_snap_to_grid(&mut self, snap: bool) -> bool {
        let changed = self.editor.set_snap_to_grid(&mut self.engine, snap);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setGridSpacing)]
    pub fn set_grid_spacing(&mut self, emu: f64) -> bool {
        let changed = self.editor.set_grid_spacing(&mut self.engine, emu.round() as i64);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = addGuide)]
    pub fn add_guide(&mut self, vertical: bool) -> bool {
        let changed = self.editor.add_guide(&mut self.engine, vertical);
        self.touch(changed)
    }

    /// While a guide is dragged: its distance from the center in cm (else "").
    #[wasm_bindgen(js_name = guideLabel)]
    pub fn guide_label(&self) -> String {
        self.editor.guide_label().unwrap_or_default()
    }

    /// For the rulers: `[left, top, width, height]` of the slide in canvas CSS pixels,
    /// then the selection's `[x0, y0, x1, y1]` in EMU when something is selected.
    #[wasm_bindgen(js_name = rulerInfo)]
    pub fn ruler_info(&self) -> Vec<f64> {
        let pres = &self.editor.pres;
        let r = self.view().transform_rect_bbox(Rect::new(0.0, 0.0, pres.slide_width as f64, pres.slide_height as f64));
        let mut out = vec![r.x0 / self.dpr, r.y0 / self.dpr, r.width() / self.dpr, r.height() / self.dpr];
        let frame = self.editor.selection_frame();
        if let Some(x) = frame {
            out.extend([x.x as f64, x.y as f64, (x.x + x.w) as f64, (x.y + x.h) as f64]);
        }
        out
    }

    // --- Design -----------------------------------------------------------------------

    /// The current slide's background: `{ kind: "solid" | "gradient" | "picture", color,
    /// colors: [start, end], radial, angle, own, hideGraphics }`.
    pub fn background(&self) -> Object {
        let o = Object::new();
        let Some((bg, own)) = self.editor.background() else { return o };
        let (kind, colors, radial, angle) = match bg {
            Background::Paint(Paint::Solid(c)) => ("solid", vec![*c, *c], false, 90.0),
            Background::Paint(Paint::Gradient(g)) => {
                let first = g.stops.first().map_or(Color::WHITE, |s| s.1);
                let last = g.stops.last().map_or(Color::WHITE, |s| s.1);
                match g.kind {
                    GradientKind::Linear { angle, .. } => ("gradient", vec![first, last], false, angle),
                    GradientKind::Path { .. } => ("gradient", vec![first, last], true, 0.0),
                }
            }
            Background::Picture { .. } => ("picture", vec![Color::WHITE, Color::WHITE], false, 90.0),
        };
        let hide = self.editor.pres.slides.get(self.editor.slide).is_some_and(|s| s.hide_background_graphics);
        let _ = Reflect::set(&o, &"kind".into(), &kind.into());
        let _ = Reflect::set(&o, &"color".into(), &hex(bg.color()).into());
        let _ = Reflect::set(&o, &"colors".into(), &colors.iter().map(|c| JsValue::from(hex(*c))).collect::<Array>());
        let _ = Reflect::set(&o, &"radial".into(), &radial.into());
        let _ = Reflect::set(&o, &"angle".into(), &angle.into());
        let _ = Reflect::set(&o, &"own".into(), &own.into());
        let _ = Reflect::set(&o, &"hideGraphics".into(), &hide.into());
        o
    }

    /// Fills the current slide's background with one color.
    #[wasm_bindgen(js_name = setBackgroundSolid)]
    pub fn set_background_solid(&mut self, color: &str) -> bool {
        let Some(c) = parse_hex(color) else { return false };
        let changed = self.editor.set_background(&mut self.engine, Some(Background::Paint(Paint::Solid(c))));
        self.touch(changed)
    }

    /// A two-color gradient background: linear at `angle` degrees, or radial from the center.
    #[wasm_bindgen(js_name = setBackgroundGradient)]
    pub fn set_background_gradient(&mut self, start: &str, end: &str, radial: bool, angle: f64) -> bool {
        let (Some(a), Some(b)) = (parse_hex(start), parse_hex(end)) else { return false };
        let kind = if radial { GradientKind::Path { circle: true, focus: [0.5; 4] } } else { GradientKind::Linear { angle, scaled: false } };
        let g = Gradient { stops: vec![(0.0, a), (1.0, b)], kind, rotate_with_shape: true };
        let changed = self.editor.set_background(&mut self.engine, Some(Background::Paint(Paint::Gradient(Arc::new(g)))));
        self.touch(changed)
    }

    /// A picture background from an image file. Returns false if it cannot be decoded.
    #[wasm_bindgen(js_name = setBackgroundPicture)]
    pub fn set_background_picture(&mut self, bytes: Vec<u8>, ext: &str) -> bool {
        if image_size(&bytes).is_none() {
            return false;
        }
        self.media_counter += 1;
        let key = format!("new/image{}.{}", self.media_counter, ext.to_lowercase());
        let changed = self.editor.set_background_picture(&mut self.engine, key, Arc::new(bytes));
        self.touch(changed)
    }

    /// The current slide follows its layout's background again (重設背景).
    #[wasm_bindgen(js_name = resetBackground)]
    pub fn reset_background(&mut self) -> bool {
        let changed = self.editor.set_background(&mut self.engine, None);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setHideBackgroundGraphics)]
    pub fn set_hide_background_graphics(&mut self, hide: bool) -> bool {
        let changed = self.editor.set_hide_background_graphics(&mut self.engine, hide);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = applyBackgroundToAll)]
    pub fn apply_background_to_all(&mut self) -> bool {
        let changed = self.editor.apply_background_to_all(&mut self.engine);
        self.touch(changed)
    }

    /// The deck's theme: `{ colorScheme, colors: [12 hex in dk1 lt1 dk2 lt2 accent1-6 hlink folHlink order], fontScheme, major, minor }`, or null.
    #[wasm_bindgen(js_name = themeInfo)]
    pub fn theme_info(&self) -> JsValue {
        let Some(t) = design::theme_info(&self.editor.pres) else { return JsValue::NULL };
        let o = Object::new();
        let _ = Reflect::set(&o, &"colorScheme".into(), &t.color_scheme.into());
        let _ = Reflect::set(&o, &"colors".into(), &t.colors.iter().map(|c| JsValue::from(hex(*c))).collect::<Array>());
        let _ = Reflect::set(&o, &"fontScheme".into(), &t.font_scheme.into());
        let _ = Reflect::set(&o, &"major".into(), &t.major.into());
        let _ = Reflect::set(&o, &"minor".into(), &t.minor.into());
        o.into()
    }

    /// Built-in color variants: `[{ name, label, colors: [10 hex: dk2 lt2 accent1-6 hlink folHlink] }]`.
    #[wasm_bindgen(js_name = colorSchemes)]
    pub fn color_schemes(&self) -> Array {
        design::COLOR_SCHEMES
            .iter()
            .map(|cs| {
                let o = Object::new();
                let _ = Reflect::set(&o, &"name".into(), &cs.name.into());
                let _ = Reflect::set(&o, &"label".into(), &cs.label.into());
                let colors: Array = cs.colors.iter().map(|v| JsValue::from(format!("#{v:06X}"))).collect();
                let _ = Reflect::set(&o, &"colors".into(), &colors);
                JsValue::from(o)
            })
            .collect()
    }

    /// Built-in font variants: `[{ name, major, minor }]`.
    #[wasm_bindgen(js_name = fontSchemes)]
    pub fn font_schemes(&self) -> Array {
        design::FONT_SCHEMES
            .iter()
            .map(|fs| {
                let o = Object::new();
                let _ = Reflect::set(&o, &"name".into(), &fs.name.into());
                let _ = Reflect::set(&o, &"major".into(), &fs.major.into());
                let _ = Reflect::set(&o, &"minor".into(), &fs.minor.into());
                JsValue::from(o)
            })
            .collect()
    }

    fn restyle(&mut self, pres: Result<Presentation, ppt_ooxml::Error>) -> Result<bool, JsValue> {
        let pres = pres.map_err(js_err)?;
        self.editor.replace_document(&mut self.engine, pres);
        // Media keys are renamed by the save the restyle went through.
        self.images.clear();
        Ok(self.touch(true))
    }

    #[wasm_bindgen(js_name = setColorScheme)]
    pub fn set_color_scheme(&mut self, name: &str) -> Result<bool, JsValue> {
        let Some(cs) = design::COLOR_SCHEMES.iter().find(|c| c.name == name) else { return Ok(false) };
        let pres = design::set_color_scheme(&self.editor.pres, cs);
        self.restyle(pres)
    }

    #[wasm_bindgen(js_name = setFontScheme)]
    pub fn set_font_scheme(&mut self, name: &str) -> Result<bool, JsValue> {
        let Some(fs) = design::FONT_SCHEMES.iter().find(|f| f.name == name) else { return Ok(false) };
        let pres = design::set_font_scheme(&self.editor.pres, fs);
        self.restyle(pres)
    }

    /// Changes the slide size (EMU). `fit`: "maximize" or "fit" (ensure fit).
    #[wasm_bindgen(js_name = setSlideSize)]
    pub fn set_slide_size(&mut self, width: f64, height: f64, fit: &str) -> Result<bool, JsValue> {
        let (w, h) = (width.round() as i64, height.round() as i64);
        if !(914_400 / 2..=51_206_400).contains(&w) || !(914_400 / 2..=51_206_400).contains(&h) {
            return Err(JsValue::from_str("投影片大小必須介於 1.27 到 142.24 公分之間"));
        }
        if (w, h) == (self.editor.pres.slide_width, self.editor.pres.slide_height) {
            return Ok(false);
        }
        let fit = if fit == "maximize" { design::Fit::Maximize } else { design::Fit::EnsureFit };
        let pres = design::set_slide_size(&self.editor.pres, w, h, fit);
        self.restyle(pres)
    }

    /// Inserts `text` at the caret, or as a new text box when no text is being edited.
    #[wasm_bindgen(js_name = insertSymbol)]
    pub fn insert_symbol(&mut self, text: &str) -> bool {
        let changed = if self.editor.is_editing_text() { self.editor.insert_text(&mut self.engine, text) } else { self.editor.paste_text_box(&mut self.engine, text) };
        self.touch(changed)
    }

    /// Groups the selected shapes (`true`) or ungroups the selected group.
    #[wasm_bindgen(js_name = groupShapes)]
    pub fn group_shapes(&mut self, group: bool) -> bool {
        let changed = if group { self.editor.group_selected() } else { self.editor.ungroup_selected() };
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = tableMerge)]
    pub fn table_merge(&mut self) -> bool {
        let changed = self.editor.merge_cells(&mut self.engine);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = tableSplit)]
    pub fn table_split(&mut self, rows: usize, cols: usize) -> bool {
        let changed = self.editor.split_cell(&mut self.engine, rows, cols);
        self.touch(changed)
    }

    /// Shading of the selected cells; "" removes it.
    #[wasm_bindgen(js_name = setCellFill)]
    pub fn set_cell_fill(&mut self, color: &str) -> bool {
        let changed = self.editor.set_cell_fill(&mut self.engine, parse_hex(color));
        self.touch(changed)
    }

    /// `which`: all, outside, inside, top, bottom, left, right, none.
    #[wasm_bindgen(js_name = setCellBorders)]
    pub fn set_cell_borders(&mut self, which: &str, color: &str, width_pt: f64) -> bool {
        let Some(which) = BorderSet::parse(which) else { return false };
        let line = ppt_model::Line::new(parse_hex(color).unwrap_or(Color::BLACK), (width_pt * EMU_PER_PT).round() as i64);
        let changed = self.editor.set_cell_borders(&mut self.engine, which, line);
        self.touch(changed)
    }

    /// `anchor`: top, middle, bottom.
    #[wasm_bindgen(js_name = setCellAnchor)]
    pub fn set_cell_anchor(&mut self, anchor: &str) -> bool {
        let anchor = match anchor {
            "middle" => ppt_model::Anchor::Middle,
            "bottom" => ppt_model::Anchor::Bottom,
            _ => ppt_model::Anchor::Top,
        };
        let changed = self.editor.set_cell_anchor(&mut self.engine, anchor);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setTableStyle)]
    pub fn set_table_style(&mut self, id: &str) -> bool {
        let changed = self.editor.set_table_style(&mut self.engine, id);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = setTableFlag)]
    pub fn set_table_flag(&mut self, flag: &str, on: bool) -> bool {
        let changed = self.editor.set_table_flag(&mut self.engine, flag, on);
        self.touch(changed)
    }

    /// Gallery styles with the colors a preview needs: `{id, name, header, band, body, line}`.
    #[wasm_bindgen(js_name = tableStyles)]
    pub fn table_styles(&self) -> Array {
        use ppt_model::table::{CellFill, StylePart};
        let fill = |p: &StylePart| match p.fill {
            Some(CellFill::Solid(c)) => hex(c),
            _ => String::new(),
        };
        let line = |p: &StylePart| p.inside_h.or(p.bottom).flatten().map(|l| hex(l.color)).unwrap_or_default();
        self.editor
            .pres
            .table_styles
            .iter()
            .map(|s| {
                let o = Object::new();
                for (k, v) in [("id", s.id.clone()), ("name", s.name.clone()), ("header", fill(&s.first_row)), ("band", fill(&s.band1_h)), ("body", fill(&s.whole)), ("line", line(&s.whole))] {
                    let _ = Reflect::set(&o, &k.into(), &v.into());
                }
                JsValue::from(o)
            })
            .collect()
    }

    #[wasm_bindgen(js_name = insertTable)]
    pub fn insert_table(&mut self, rows: usize, cols: usize) -> bool {
        let changed = self.editor.insert_table(&mut self.engine, rows, cols);
        self.touch(changed)
    }

    /// Row/column commands on the current table: `rowAbove`, `rowBelow`,
    /// `colLeft`, `colRight`, `deleteRow`, `deleteCol`.
    #[wasm_bindgen(js_name = tableCommand)]
    pub fn table_command(&mut self, command: &str) -> bool {
        let (e, eng) = (&mut self.editor, &mut self.engine);
        let changed = match command {
            "rowAbove" => e.insert_table_row(eng, false),
            "rowBelow" => e.insert_table_row(eng, true),
            "colLeft" => e.insert_table_col(eng, false),
            "colRight" => e.insert_table_col(eng, true),
            "deleteRow" => e.delete_table_row(eng),
            "deleteCol" => e.delete_table_col(eng),
            _ => false,
        };
        self.touch(changed)
    }

    /// Inserts an image file (`ext`: png, jpg, ...). Returns false if it cannot be decoded.
    #[wasm_bindgen(js_name = insertPicture)]
    pub fn insert_picture(&mut self, bytes: Vec<u8>, ext: &str) -> bool {
        let Some((w, h)) = image_size(&bytes) else { return false };
        self.media_counter += 1;
        let key = format!("new/image{}.{}", self.media_counter, ext.to_lowercase());
        let changed = self.editor.add_picture(&mut self.engine, key, Arc::new(bytes), w, h);
        self.touch(changed)
    }

    // --- Notes ----------------------------------------------------------------------

    /// Speaker notes of slide `index` (plain text).
    pub fn notes(&self, index: usize) -> String {
        self.editor.notes(index).to_string()
    }

    /// Call when the notes pane gets focus: its typing becomes one undo step.
    #[wasm_bindgen(js_name = beginNotesEdit)]
    pub fn begin_notes_edit(&mut self) {
        self.editor.begin_notes_edit(&mut self.engine);
        self.dirty = true;
    }

    /// Replaces the current slide's notes.
    #[wasm_bindgen(js_name = setNotes)]
    pub fn set_notes(&mut self, text: &str) -> bool {
        self.editor.set_notes(text)
    }

    // --- SmartArt text pane -------------------------------------------------------

    /// The selected SmartArt's nodes, `{ rows: [{ depth, text }], restructure, nest }`,
    /// or null when no editable SmartArt is selected.
    #[wasm_bindgen(js_name = diagramInfo)]
    pub fn diagram_info(&self) -> JsValue {
        let Some(info) = self.editor.diagram_info() else { return JsValue::NULL };
        let rows: Array = info
            .rows
            .into_iter()
            .map(|r| {
                let o = Object::new();
                let _ = Reflect::set(&o, &"depth".into(), &(r.depth as u32).into());
                let _ = Reflect::set(&o, &"text".into(), &r.text.into());
                JsValue::from(o)
            })
            .collect();
        let o = Object::new();
        let _ = Reflect::set(&o, &"rows".into(), &rows.into());
        let _ = Reflect::set(&o, &"restructure".into(), &info.restructure.into());
        let _ = Reflect::set(&o, &"nest".into(), &info.nest.into());
        o.into()
    }

    /// Replaces a SmartArt row's text (paragraphs separated by `\n`).
    #[wasm_bindgen(js_name = diagramSetText)]
    pub fn diagram_set_text(&mut self, row: usize, text: &str) -> bool {
        let changed = self.editor.diagram_set_text(&mut self.engine, row, text);
        self.touch(changed)
    }

    /// Structure commands on SmartArt row `row`: `addAfter`, `addBefore`,
    /// `addChild`, `delete`, `promote`, `demote`, `moveUp`, `moveDown`.
    /// Returns the row to focus next, or -1 when nothing changed.
    #[wasm_bindgen(js_name = diagramCommand)]
    pub fn diagram_command(&mut self, command: &str, row: usize) -> i32 {
        use ppt_model::diagram::Insert;
        let (e, eng) = (&mut self.editor, &mut self.engine);
        let next = match command {
            "addAfter" => e.diagram_insert(eng, row, Insert::After),
            "addBefore" => e.diagram_insert(eng, row, Insert::Before),
            "addChild" => e.diagram_insert(eng, row, Insert::Child),
            "delete" => e.diagram_delete(eng, row),
            "promote" => e.diagram_shift(eng, row, false).then_some(row),
            "demote" => e.diagram_shift(eng, row, true).then_some(row),
            "moveUp" => e.diagram_move(eng, row, true),
            "moveDown" => e.diagram_move(eng, row, false),
            _ => None,
        };
        self.touch(next.is_some());
        next.map_or(-1, |r| r as i32)
    }

    // --- Slide show -----------------------------------------------------------------

    /// Starts a slide show from the first slide or from the current one. Returns false for an empty deck.
    #[wasm_bindgen(js_name = startShow)]
    pub fn start_show(&mut self, from_beginning: bool) -> bool {
        self.editor.end_text_edit(&mut self.engine);
        let pres = &self.editor.pres;
        self.show = if from_beginning { SlideShow::from_beginning(pres) } else { SlideShow::new(pres, self.editor.slide) };
        self.dirty = true;
        // The first slide's transition plays from black.
        if let Some(Screen::Slide(i)) = self.show.as_ref().map(|s| s.screen()) {
            self.start_transition(None, i);
        }
        // Effects that start with the slide play after its transition.
        self.begin_step(true, true);
        self.show.is_some()
    }

    /// Ends the show; the editor goes to the slide that was last shown (the last one after the end screen).
    #[wasm_bindgen(js_name = endShow)]
    pub fn end_show(&mut self) {
        self.show_viewport = None;
        self.anim = None;
        self.step_running = false;
        let last = self.editor.pres.slides.len().saturating_sub(1);
        match self.show.take().map(|s| s.screen()) {
            Some(Screen::Slide(i)) => self.editor.set_slide(&mut self.engine, i),
            Some(Screen::End) => self.editor.set_slide(&mut self.engine, last),
            None => {}
        }
        self.dirty = true;
    }

    /// Navigates the show: "next" (false means the show is over), "prev",
    /// "first", "last", "black", "white", or "goto" with a 0-based `index`.
    #[wasm_bindgen(js_name = showCommand)]
    pub fn show_command(&mut self, command: &str, index: usize) -> bool {
        // A click during a transition only finishes it (effects that start with the slide begin now).
        if command == "next" && self.anim.take().is_some() {
            self.step_start_ms = self.step_start_ms.min(js_sys::Date::now());
            self.dirty = true;
            return true;
        }
        self.anim = None;
        let Some(show) = self.show.as_mut() else { return false };
        let before = show.screen();
        let before_step = show.step();
        let mut running = true;
        match command {
            "next" => running = show.advance(),
            "prev" => show.back(),
            "first" => show.first(),
            "last" => show.last(),
            "goto" => show.goto(index),
            "black" => show.toggle_blank(Blank::Black),
            "white" => show.toggle_blank(Blank::White),
            _ => {}
        }
        let (after, after_step) = (show.screen(), show.step());
        // Moving forward plays the incoming slide's transition.
        if let (Screen::Slide(from), Screen::Slide(to)) = (before, after)
            && command == "next"
            && to != from
        {
            self.start_transition(Some(from), to);
        }
        if after != before || after_step != before_step {
            // Going back shows the click finished; anything else plays it.
            self.begin_step(command != "prev", after != before);
        }
        self.dirty = true;
        running
    }

    /// Sets the show state exactly (`slide` -1 is the end screen), to mirror the presenter's window.
    /// With `animate`, a change of slide plays the new slide's transition (as the presenter's window does);
    /// with `animate_step`, animation click `step` plays from its start.
    #[wasm_bindgen(js_name = showSet)]
    pub fn show_set(&mut self, slide: i32, blank: &str, animate: bool, step: usize, animate_step: bool) {
        let Some(show) = self.show.as_mut() else { return };
        let (before, before_step) = (show.screen(), show.step());
        let screen = if slide < 0 { Screen::End } else { Screen::Slide(slide as usize) };
        let blank = match blank {
            "black" => Some(Blank::Black),
            "white" => Some(Blank::White),
            _ => None,
        };
        show.set(screen, blank);
        show.set_step(step);
        let step = show.step();
        match (before, screen) {
            (from, Screen::Slide(to)) if animate && from != screen => self.start_transition(if let Screen::Slide(f) = from { Some(f) } else { None }, to),
            (from, _) if from != screen => self.anim = None,
            _ => {}
        }
        if before != screen || before_step != step {
            self.begin_step(animate_step, before != screen);
        }
        self.dirty = true;
    }

    /// Sets the current slide's transition (every slide's with `all`). `id` "none" removes it;
    /// `after_ms` < 0 means no automatic advance.
    #[wasm_bindgen(js_name = setTransition)]
    pub fn set_transition(&mut self, id: &str, option: &str, duration_ms: u32, on_click: bool, after_ms: i32, all: bool) -> bool {
        let t = match id {
            "none" => None,
            _ => {
                let Some(kind) = ppt_model::TransitionKind::from_ids(id, option) else { return false };
                Some(ppt_model::Transition { kind, duration_ms, on_click, after_ms: u32::try_from(after_ms).ok() })
            }
        };
        let changed = self.editor.set_transition(&mut self.engine, t, all);
        self.touch(changed)
    }

    /// Plays all of the current slide's animations in the editor, click after click.
    #[wasm_bindgen(js_name = previewAnimations)]
    pub fn preview_animations(&mut self) {
        self.editor.end_text_edit(&mut self.engine);
        self.anim = None;
        self.preview_start_ms = Some(js_sys::Date::now());
        self.dirty = true;
    }

    /// The current slide's animations in playing order: `[{ click, trigger,
    /// class, presetId, subtype, shapes, startMs, durMs, delayMs, selected, badge }]`
    /// (`shapes` are names; `badge` is the top-left of the animated shapes in CSS pixels).
    pub fn animations(&self) -> Array {
        let e = &self.editor;
        let Some(slide) = e.pres.slides.get(e.slide) else { return Array::new() };
        let Some(timing) = slide.timing.as_deref() else { return Array::new() };
        let view = self.view();
        Player::new(timing)
            .effects()
            .into_iter()
            .zip(timing.effects())
            .map(|(fx, node)| {
                let o = Object::new();
                let shapes: Vec<&ppt_model::Shape> = fx.shapes.iter().filter_map(|id| slide.shapes.iter().find(|s| s.id == *id)).collect();
                let names: Array = shapes.iter().map(|s| JsValue::from(s.name.as_str())).collect();
                let elements = effect_elements(node);
                let members: Vec<&ppt_model::Shape> = slide.shapes.iter().filter(|s| elements.contains(&s.id) || s.group.is_some_and(|g| elements.contains(&g))).collect();
                let selected = members.iter().any(|s| e.is_selected(s.id));
                let corner = members.iter().map(|s| (s.xfrm.x, s.xfrm.y)).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1)));
                let badge: JsValue = match corner {
                    Some((x, y)) => {
                        let p = view * Point::new(x as f64, y as f64);
                        Array::of2(&(p.x / self.dpr).into(), &(p.y / self.dpr).into()).into()
                    }
                    None => JsValue::NULL,
                };
                let _ = Reflect::set(&o, &"click".into(), &(fx.click as u32).into());
                let _ = Reflect::set(&o, &"trigger".into(), &fx.trigger.into());
                let _ = Reflect::set(&o, &"class".into(), &fx.class.into());
                let _ = Reflect::set(&o, &"presetId".into(), &fx.preset_id.map_or(JsValue::NULL, JsValue::from));
                let _ = Reflect::set(&o, &"subtype".into(), &node.preset_subtype.map_or(JsValue::NULL, JsValue::from));
                let _ = Reflect::set(&o, &"shapes".into(), &names.into());
                let _ = Reflect::set(&o, &"startMs".into(), &fx.start_ms.into());
                let _ = Reflect::set(&o, &"durMs".into(), &fx.dur_ms.into());
                let _ = Reflect::set(&o, &"delayMs".into(), &node.delay_ms.into());
                let _ = Reflect::set(&o, &"selected".into(), &selected.into());
                let _ = Reflect::set(&o, &"badge".into(), &badge);
                JsValue::from(o)
            })
            .collect()
    }

    /// The effects the editor can add: `[{ class, id, subtype, durMs, directional }]`.
    #[wasm_bindgen(js_name = animationPresets)]
    pub fn animation_presets(&self) -> Array {
        PRESETS
            .iter()
            .map(|p| {
                let o = Object::new();
                let _ = Reflect::set(&o, &"class".into(), &p.class.into());
                let _ = Reflect::set(&o, &"id".into(), &p.id.into());
                let _ = Reflect::set(&o, &"subtype".into(), &p.subtype.into());
                let _ = Reflect::set(&o, &"durMs".into(), &p.dur_ms.into());
                let _ = Reflect::set(&o, &"directional".into(), &p.directional.into());
                JsValue::from(o)
            })
            .collect()
    }

    /// Adds an effect to every selected shape; `subtype < 0` uses the default option.
    #[wasm_bindgen(js_name = addAnimation)]
    pub fn add_animation(&mut self, class: &str, id: u32, subtype: i32) -> bool {
        self.editor.end_text_edit(&mut self.engine);
        let changed = self.editor.add_animation(class, id, (subtype >= 0).then_some(subtype as u32));
        self.touch(changed)
    }

    /// Replaces effect `index`; `subtype < 0` uses the default option.
    #[wasm_bindgen(js_name = changeAnimation)]
    pub fn change_animation(&mut self, index: usize, class: &str, id: u32, subtype: i32) -> bool {
        let changed = self.editor.change_animation(index, class, id, (subtype >= 0).then_some(subtype as u32));
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = removeAnimation)]
    pub fn remove_animation(&mut self, index: usize) -> bool {
        let changed = self.editor.remove_animation(index);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = moveAnimation)]
    pub fn move_animation(&mut self, from: usize, to: usize) -> bool {
        let changed = self.editor.move_animation(from, to);
        self.touch(changed)
    }

    /// `trigger`: "clickEffect", "withEffect" or "afterEffect".
    #[wasm_bindgen(js_name = setAnimationTrigger)]
    pub fn set_animation_trigger(&mut self, index: usize, trigger: &str) -> bool {
        let t = match trigger {
            "withEffect" => Trigger::With,
            "afterEffect" => Trigger::After,
            _ => Trigger::Click,
        };
        let changed = self.editor.set_animation_trigger(index, t);
        self.touch(changed)
    }

    /// Sets effect `index`'s delay and duration (ms); a negative value leaves it unchanged.
    #[wasm_bindgen(js_name = setAnimationTiming)]
    pub fn set_animation_timing(&mut self, index: usize, delay_ms: f64, dur_ms: f64) -> bool {
        let changed = self.editor.set_animation_timing(index, (delay_ms >= 0.0).then_some(delay_ms), (dur_ms > 0.0).then_some(dur_ms));
        self.touch(changed)
    }

    /// Selects the shapes effect `index` animates.
    #[wasm_bindgen(js_name = selectAnimation)]
    pub fn select_animation(&mut self, index: usize) -> bool {
        let changed = self.editor.select_animation(&mut self.engine, index);
        self.touch(changed)
    }

    /// Plays the current slide's transition in the editor (from the previous slide).
    #[wasm_bindgen(js_name = previewTransition)]
    pub fn preview_transition(&mut self) {
        let i = self.editor.slide;
        self.editor.end_text_edit(&mut self.engine);
        self.preview_start_ms = None;
        self.start_transition(i.checked_sub(1), i);
    }

    /// Draws the show's slide into this rectangle (CSS pixels) instead of the whole canvas; w = 0 resets.
    #[wasm_bindgen(js_name = setShowViewport)]
    pub fn set_show_viewport(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let d = self.dpr;
        self.show_viewport = (w > 0.0 && h > 0.0).then(|| Rect::new(x * d, y * d, (x + w) * d, (y + h) * d));
        self.dirty = true;
    }

    /// `{ slide, next, count, blank }`: `slide` / `next` are -1 for the end screen, `blank` is "", "black" or "white".
    /// Null when no show is running.
    #[wasm_bindgen(js_name = showState)]
    pub fn show_state(&self) -> JsValue {
        let Some(show) = &self.show else { return JsValue::NULL };
        let o = Object::new();
        let slide = match show.screen() {
            Screen::Slide(i) => i as i32,
            Screen::End => -1,
        };
        let blank = match show.blank() {
            Some(Blank::Black) => "black",
            Some(Blank::White) => "white",
            None => "",
        };
        let next = match show.upcoming() {
            Screen::Slide(i) => i as i32,
            Screen::End => -1,
        };
        let current = match show.screen() {
            Screen::Slide(i) => self.editor.pres.slides.get(i).and_then(|s| s.transition.clone()),
            Screen::End => None,
        };
        let _ = Reflect::set(&o, &"transition".into(), &self.anim.is_some().into());
        let _ = Reflect::set(&o, &"step".into(), &(show.step() as u32).into());
        let _ = Reflect::set(&o, &"steps".into(), &(show.steps() as u32).into());
        let _ = Reflect::set(&o, &"stepAnimate".into(), &self.step_animate.into());
        let _ = Reflect::set(&o, &"transitionMs".into(), &current.as_ref().filter(|t| t.animates()).map_or(0, |t| t.duration_ms).into());
        let _ = Reflect::set(&o, &"afterMs".into(), &current.as_ref().and_then(|t| t.after_ms).map_or(-1.0, |ms| ms as f64).into());
        let _ = Reflect::set(&o, &"onClick".into(), &current.as_ref().is_none_or(|t| t.on_click).into());
        let _ = Reflect::set(&o, &"slide".into(), &slide.into());
        let _ = Reflect::set(&o, &"next".into(), &next.into());
        let _ = Reflect::set(&o, &"count".into(), &(show.slide_count() as u32).into());
        let _ = Reflect::set(&o, &"blank".into(), &blank.into());
        o.into()
    }

    // --- Slides --------------------------------------------------------------------

    #[wasm_bindgen(js_name = setSlide)]
    pub fn set_slide(&mut self, index: usize) {
        self.preview_start_ms = None;
        self.editor.set_slide(&mut self.engine, index);
        self.dirty = true;
    }

    /// Adds a slide after the current one; `layout < 0` reuses the current slide's layout.
    #[wasm_bindgen(js_name = addSlide)]
    pub fn add_slide(&mut self, layout: i32) -> bool {
        let changed = self.editor.add_slide(&mut self.engine, (layout >= 0).then_some(layout as usize));
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = duplicateSlide)]
    pub fn duplicate_slide(&mut self, index: usize) -> bool {
        let changed = self.editor.duplicate_slide(&mut self.engine, index);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = deleteSlide)]
    pub fn delete_slide(&mut self, index: usize) -> bool {
        let changed = self.editor.delete_slide(&mut self.engine, index);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = moveSlide)]
    pub fn move_slide(&mut self, from: usize, to: usize) -> bool {
        let changed = self.editor.move_slide(&mut self.engine, from, to);
        self.touch(changed)
    }

    #[wasm_bindgen(js_name = layoutNames)]
    pub fn layout_names(&self) -> Array {
        self.editor.pres.layouts.iter().map(|l| JsValue::from(l.name.clone())).collect()
    }

    /// Slide size `[width, height]` in EMU.
    #[wasm_bindgen(js_name = slideSize)]
    pub fn slide_size(&self) -> Vec<f64> {
        vec![self.editor.pres.slide_width as f64, self.editor.pres.slide_height as f64]
    }

    /// Prepares slide `index` for display at the current zoom without drawing it:
    /// its images are decoded at the size the editor shows them, so switching to
    /// it later is quick. Call when idle.
    #[wasm_bindgen(js_name = prefetchSlide)]
    pub fn prefetch_slide(&mut self, index: usize) {
        let (s, view) = (self.scale(), self.view());
        let pres = &self.editor.pres;
        let Some(slide) = pres.slides.get(index) else { return };
        let mut scene = Scene::new();
        let opts = DrawOptions { editing: true, emu_per_px: 1.0 / s, ..Default::default() };
        let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
        draw_slide(&mut scene, &mut cx, pres, slide, view, &opts);
    }

    /// Renders slide `index` (or layout `index` when `layout` is true) at `width` px.
    /// Resolves with `{ width, height, pixels }` (RGBA).
    #[wasm_bindgen(js_name = renderThumbnail)]
    pub fn render_thumbnail(&mut self, index: usize, width: u32, layout: bool) -> Result<js_sys::Promise, JsValue> {
        let pres = &self.editor.pres;
        let slide = if layout {
            pres.layouts.get(index).map(|l| ppt_model::Slide {
                background_shapes: l.background_shapes.clone(),
                shapes: l.placeholders.clone(),
                background: l.background.clone(),
                ..Default::default()
            })
        } else {
            pres.slides.get(index).cloned()
        };
        let slide = slide.ok_or_else(|| JsValue::from_str("no such slide"))?;
        let width = width.clamp(16, 2048);
        let height = ((width as f64) * pres.slide_height as f64 / pres.slide_width as f64).round().max(1.0) as u32;
        let scale = width as f64 / pres.slide_width as f64;
        let mut scene = Scene::new();
        {
            // Layout previews show the placeholder prompts so they are recognizable.
            let opts = DrawOptions { editing: layout, emu_per_px: 1.0 / scale, ..Default::default() };
            let mut cx = RenderCtx { engine: &mut self.engine, images: &mut self.images };
            draw_slide(&mut scene, &mut cx, pres, &slide, Affine::scale(scale), &opts);
        }

        let dev = &self.gpu.cx.devices[self.gpu.surface.dev_id];
        let (device, queue) = (&dev.device, &dev.queue);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("thumbnail"),
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
        self.gpu.renderer.render_to_texture(device, queue, &scene, &view, &params).map_err(js_err)?;

        let padded = (width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thumbnail readback"),
            size: (padded * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("thumbnail copy") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);

        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let buf = buffer.clone();
            buffer.map_async(wgpu::MapMode::Read, .., move |result| {
                if let Err(e) = result {
                    let _ = reject.call1(&JsValue::NULL, &js_err(e));
                    return;
                }
                let mut pixels = Vec::with_capacity((width * height * 4) as usize);
                if let Ok(data) = buf.get_mapped_range(..) {
                    for row in data.chunks(padded as usize).take(height as usize) {
                        pixels.extend_from_slice(&row[..(width * 4) as usize]);
                    }
                }
                buf.unmap();
                let out = Object::new();
                let _ = Reflect::set(&out, &"width".into(), &width.into());
                let _ = Reflect::set(&out, &"height".into(), &height.into());
                let _ = Reflect::set(&out, &"pixels".into(), &Uint8Array::from(pixels.as_slice()));
                let _ = resolve.call1(&JsValue::NULL, &out);
            });
        });
        Ok(promise)
    }

    /// UI state snapshot for the React shell.
    pub fn status(&self) -> Object {
        let o = Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = Reflect::set(&o, &JsValue::from_str(k), &v);
        };
        let e = &self.editor;
        set("slideIndex", (e.slide as u32).into());
        set("slideCount", (e.pres.slides.len() as u32).into());
        set("showGrid", self.show_grid.into());
        set("showGuides", e.pres.view.show_guides.into());
        set("snapToGrid", e.pres.view.snap_to_grid.into());
        set("gridSpacing", (e.pres.view.grid_spacing as f64).into());
        set("selectionCount", (e.selection.len() as u32).into());
        set("hasSelection", (!e.selection.is_empty()).into());
        set("editingText", e.is_editing_text().into());
        set("inTable", e.in_table().into());
        set("smartArt", e.diagram_info().is_some().into());
        set("diagramRequests", e.diagram_requests.into());
        set("diagramFocus", e.diagram_focus.map_or(-1, |r| r as i32).into());
        set("canMerge", e.can_merge_cells().into());
        if let Some(t) = e.current_table() {
            set("tableStyle", t.style.as_ref().map(|s| s.id.clone()).unwrap_or_default().into());
            let f = t.flags;
            let flags = Object::new();
            for (k, v) in [("firstRow", f.first_row), ("lastRow", f.last_row), ("firstCol", f.first_col), ("lastCol", f.last_col), ("bandRow", f.band_row), ("bandCol", f.band_col)] {
                let _ = Reflect::set(&flags, &k.into(), &v.into());
            }
            set("tableFlags", flags.into());
        }
        if let Some(a) = e.current_cell_anchor() {
            let a = match a {
                ppt_model::Anchor::Top => "top",
                ppt_model::Anchor::Middle => "middle",
                ppt_model::Anchor::Bottom => "bottom",
            };
            set("cellAnchor", a.into());
        }
        set("canGroup", (e.selection.len() > 1 && e.selection_group().is_none()).into());
        set("canUngroup", (e.selection_group().is_some() || e.selected_shape().is_some_and(|s| s.group.is_some())).into());
        set("canUndo", e.can_undo().into());
        set("canRedo", e.can_redo().into());
        set("canPaste", e.has_clipboard().into());
        if let Some(t) = e.pres.slides.get(e.slide).and_then(|s| s.transition.as_ref()) {
            let o = Object::new();
            let _ = Reflect::set(&o, &"id".into(), &t.kind.id().into());
            let _ = Reflect::set(&o, &"option".into(), &t.kind.option().into());
            let _ = Reflect::set(&o, &"durationMs".into(), &t.duration_ms.into());
            let _ = Reflect::set(&o, &"onClick".into(), &t.on_click.into());
            let _ = Reflect::set(&o, &"afterMs".into(), &t.after_ms.map_or(-1.0, |ms| ms as f64).into());
            set("transition", o.into());
        }
        let marks: Array = e.pres.slides.iter().map(|s| JsValue::from(s.transition.as_ref().is_some_and(|t| t.animates()))).collect();
        set("transitionSlides", marks.into());
        if let Some(a) = e.text_anchor() {
            let a = match a {
                ppt_model::Anchor::Top => "top",
                ppt_model::Anchor::Middle => "middle",
                ppt_model::Anchor::Bottom => "bottom",
            };
            set("textAnchor", a.into());
        }
        set("slideLayout", e.pres.slides.get(e.slide).and_then(|s| s.layout).map_or(-1, |l| l as i32).into());
        set("animations", self.animations().into());
        set("canAnimate", e.can_animate().into());
        set("zoom", self.zoom.into());
        set("frameMs", self.last_frame_ms.into());
        set("sceneMs", self.last_scene_ms.into());
        set("revision", (e.revision() as f64).into());
        set("selectionName", e.selected_shape().map(|s| s.name.clone()).unwrap_or_default().into());
        let fmt = e.text_format();
        set("hasText", fmt.is_some().into());
        if let Some(f) = fmt {
            let latin = f.style.latin_font.clone();
            set("fontFamily", if latin.is_empty() { f.style.ea_font.clone() } else { latin }.into());
            set("fontSize", f.style.size_pt.into());
            set("bold", f.style.bold.into());
            set("italic", f.style.italic.into());
            set("underline", f.style.underline.into());
            set("strike", f.style.strike.into());
            set("baseline", f.style.baseline.into());
            set("charSpacing", f.style.spacing_pt.into());
            set("lineSpacing", f.line_spacing.into());
            set("textColor", hex(f.style.color).into());
            set("link", f.style.link.as_ref().filter(|l| l.external).map(|l| l.target.clone()).unwrap_or_default().into());
            let align = match f.align {
                Align::Left => "left",
                Align::Center => "center",
                Align::Right => "right",
                Align::Justify => "justify",
            };
            set("align", align.into());
            set("bullet", f.bullet.into());
            set("numbering", f.numbering.clone().unwrap_or_default().into());
        }
        if let Some(f) = e.selection_frame() {
            let o = Object::new();
            for (k, v) in [("x", f.x as f64), ("y", f.y as f64), ("w", f.w as f64), ("h", f.h as f64), ("rot", f.rot)] {
                let _ = Reflect::set(&o, &k.into(), &v.into());
            }
            let _ = Reflect::set(&o, &"single".into(), &e.selected_shape().is_some().into());
            set("frame", o.into());
        }
        if let Some(shadow) = e.selection_shadow() {
            let name = match shadow {
                None => "none",
                Some(s) => SHADOWS.iter().find(|(_, dir, dist, blur)| (s.dir - dir).abs() < 0.5 && s.dist == *dist && s.blur == *blur).map_or("custom", |p| p.0),
            };
            set("shadow", name.into());
        }
        set("drawTool", e.draw_tool.clone().unwrap_or_default().into());
        set("canChangeShape", e.selected_shapes().iter().any(|s| matches!(s.kind, ppt_model::ShapeKind::Shape) && !s.geometry.is_line()).into());
        if let Some(c) = e.selected_crop() {
            let o = Object::new();
            for (k, v) in [("left", c.left), ("top", c.top), ("right", c.right), ("bottom", c.bottom)] {
                let _ = Reflect::set(&o, &k.into(), &v.into());
            }
            set("crop", o.into());
            set("cropMode", e.crop.is_some().into());
        }
        match e.selection_fill() {
            Some(Some(c)) => set("fill", hex(c).into()),
            Some(None) => set("fill", "".into()),
            None => {}
        }
        match e.selection_line() {
            Some(Some(l)) => {
                set("lineColor", hex(l.color).into());
                set("lineWidth", (l.width as f64 / EMU_PER_PT).into());
            }
            Some(None) => set("lineColor", "".into()),
            None => {}
        }
        o
    }

    /// Renders if anything changed. Returns true when a frame was presented.
    pub fn render(&mut self) -> Result<bool, JsValue> {
        if !self.dirty {
            return Ok(false);
        }
        let start = now_ms();
        self.build_scene();
        self.last_scene_ms = now_ms() - start;
        let gpu = &mut self.gpu;
        let dev = &gpu.cx.devices[gpu.surface.dev_id];
        let params = RenderParams { base_color: if self.show.is_some() { PColor::BLACK } else { WORKSPACE }, width: self.width, height: self.height, antialiasing_method: AaConfig::Area };
        gpu.renderer.render_to_texture(&dev.device, &dev.queue, &self.scene, &gpu.surface.target_view, &params).map_err(js_err)?;
        let texture = match gpu.surface.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(t) | CurrentSurfaceTexture::Suboptimal(t) => t,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                gpu.cx.configure_surface(&gpu.surface);
                return Ok(false);
            }
            _ => return Ok(false),
        };
        let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = dev.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("blit") });
        gpu.surface.blitter.copy(&dev.device, &mut encoder, &gpu.surface.target_view, &view);
        dev.queue.submit([encoder.finish()]);
        dev.queue.present(texture);
        // Keep drawing frames while a transition or an animation plays.
        self.dirty = self.anim.is_some() || self.step_running;
        self.last_frame_ms = now_ms() - start;
        Ok(true)
    }
}
