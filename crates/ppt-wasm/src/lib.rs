//! WebAssembly facade used by the React shell.
//!
//! The document model, editor and renderer all live here, inside the webview,
//! so pointer and keyboard handling never wait on IPC. The native backend only
//! supplies file and font bytes.
#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use js_sys::{Array, Object, Reflect, Uint8Array};
use ppt_editor::{AlignTo, BorderSet, CursorIcon, Editor, Handle, Modifiers, PointerEvent, Toggle, Tolerance, ZOrder, utf16_to_byte_offset};
use ppt_geometry::shape_affine;
use ppt_model::{Align, Color, EMU_PER_PT, Presentation};
use ppt_render::vello::kurbo::{Affine, Point, Rect, Vec2};
use ppt_render::vello::peniko::Color as PColor;
use ppt_render::vello::util::{RenderContext, RenderSurface};
use ppt_render::vello::wgpu::{self, CurrentSurfaceTexture, SurfaceTarget};
use ppt_render::vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use ppt_render::{
    DrawOptions, ImageCache, RenderCtx, SelectionOverlay, TextOverlay, draw_body_layout, draw_guides, draw_marquee, draw_selection,
    draw_slide, draw_text_overlay, image_size,
};
use ppt_text::TextEngine;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

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
    media_counter: u32,
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
        media_counter: 0,
    })
}

impl App {
    /// Device pixels per EMU.
    fn scale(&self) -> f64 {
        let pres = &self.editor.pres;
        let margin = FIT_MARGIN * self.dpr;
        let sx = (self.width as f64 - 2.0 * margin).max(1.0) / pres.slide_width as f64;
        let sy = (self.height as f64 - 2.0 * margin).max(1.0) / pres.slide_height as f64;
        sx.min(sy) * self.zoom
    }

    /// Slide space (EMU) → canvas device pixels.
    fn view(&self) -> Affine {
        let s = self.scale();
        let pres = &self.editor.pres;
        let ox = (self.width as f64 - pres.slide_width as f64 * s) / 2.0;
        let oy = (self.height as f64 - pres.slide_height as f64 * s) / 2.0;
        Affine::translate(Vec2::new(ox, oy)) * Affine::scale(s)
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

    fn build_scene(&mut self) {
        let view = self.view();
        let s = self.scale();
        self.scene.reset();
        {
            let pres = &self.editor.pres;
            if let Some(slide) = pres.slides.get(self.editor.slide) {
                let opts = DrawOptions { skip_text: self.editor.text.as_ref().map(|t| t.shape), skip_cell: self.editor.text.as_ref().and_then(|t| t.cell), editing: true, emu_per_px: 1.0 / s };
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
    pub fn load_pptx(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let pres = ppt_ooxml::parse_pptx(bytes).map_err(js_err)?;
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
            CursorIcon::Rotate => "crosshair",
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
            _ => Toggle::Underline,
        };
        let changed = self.editor.toggle_style(&mut self.engine, t);
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

    // --- Slides --------------------------------------------------------------------

    #[wasm_bindgen(js_name = setSlide)]
    pub fn set_slide(&mut self, index: usize) {
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

    /// Renders slide `index` (or layout `index` when `layout` is true) at `width` px.
    /// Resolves with `{ width, height, pixels }` (RGBA).
    #[wasm_bindgen(js_name = renderThumbnail)]
    pub fn render_thumbnail(&mut self, index: usize, width: u32, layout: bool) -> Result<js_sys::Promise, JsValue> {
        let pres = &self.editor.pres;
        let slide = if layout {
            pres.layouts.get(index).map(|l| ppt_model::Slide {
                background_shapes: l.background_shapes.clone(),
                shapes: l.placeholders.clone(),
                background: l.background,
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
        set("selectionCount", (e.selection.len() as u32).into());
        set("hasSelection", (!e.selection.is_empty()).into());
        set("editingText", e.is_editing_text().into());
        set("inTable", e.in_table().into());
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
        set("zoom", self.zoom.into());
        set("frameMs", self.last_frame_ms.into());
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
        let start = js_sys::Date::now();
        self.build_scene();
        let gpu = &mut self.gpu;
        let dev = &gpu.cx.devices[gpu.surface.dev_id];
        let params = RenderParams { base_color: WORKSPACE, width: self.width, height: self.height, antialiasing_method: AaConfig::Area };
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
        self.dirty = false;
        self.last_frame_ms = js_sys::Date::now() - start;
        Ok(true)
    }
}
