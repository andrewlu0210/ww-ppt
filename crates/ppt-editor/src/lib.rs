//! Editing state and interaction logic, independent of rendering and the DOM.
//!
//! All pointer coordinates are in slide space (EMU). The host converts from
//! screen coordinates and passes tolerances already scaled to EMU.

mod a11y;
mod access;
mod anim;
mod check;
mod design;
mod diagram;
mod draw;
mod formatinfo;
mod find;
mod footer;
mod format;
mod group;
mod layout;
mod narration;
mod painter;
mod richedit;
mod shapeprops;
mod spell;
pub use formatinfo::{FillSpec, FormatInfo, LineSpec, ShadowSpec, TextBoxInfo};
pub use texteffect::{TextEffectInfo, TextEffectSpec, TextOutlineSpec};
pub use shapeprops::TextBoxSpec;
pub mod show;
mod table;
mod texteffect;
mod view;

use std::collections::HashMap;

use parley::BoundingBox;
use ppt_geometry::kurbo::{Affine, Point};
use ppt_geometry::{TextFrame, text_affine, text_width_pt};
use ppt_model::{
    Align, Anchor, Bullet, BulletKind, Color, EMU_PER_INCH, EMU_PER_PT, Geometry, Line, Link, Paint, Paragraph, Presentation, RunStyle, Shape, ShapeId,
    ShapeKind, Slide, TextBody, Xfrm,
};
use ppt_text::{BodyLayout, SINGLE_LINE_HEIGHT, TextEngine};

pub use a11y::{A11yItem, A11yParagraph, A11yRole, describe, slide_title};
pub use access::PaneItem;
pub use check::{Issue, IssueKind, Severity, contrast};
pub use diagram::DiagramInfo;
pub use draw::ARROW_LINE;
pub use find::{FindMatch, FindOptions, FindPlace, find_in};
pub use footer::{DateText, HeaderFooter};
pub use richedit::{Move, RichEdit};
pub use spell::range_rects;
pub use table::{BorderSet, CellRange};

/// Smallest width/height a resize can produce (2pt), except for lines.
const MIN_SIZE: f64 = 25_400.0;
const UNDO_LIMIT: usize = 100;
/// Offset for pasted/duplicated shapes on the same slide.
const PASTE_OFFSET: i64 = 228_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    NW,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    Rotate,
}

impl Handle {
    pub const RESIZE: [Handle; 8] = [Handle::NW, Handle::N, Handle::NE, Handle::E, Handle::SE, Handle::S, Handle::SW, Handle::W];

    /// Which edges the handle moves: -1 = left/top, 1 = right/bottom.
    fn sign(self) -> (f64, f64) {
        match self {
            Handle::NW => (-1.0, -1.0),
            Handle::N => (0.0, -1.0),
            Handle::NE => (1.0, -1.0),
            Handle::E => (1.0, 0.0),
            Handle::SE => (1.0, 1.0),
            Handle::S => (0.0, 1.0),
            Handle::SW => (-1.0, 1.0),
            Handle::W => (-1.0, 0.0),
            Handle::Rotate => (0.0, 0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CursorIcon {
    Default,
    Move,
    Text,
    Rotate,
    /// Resize along this screen angle in degrees (0 = horizontal).
    Resize(f64),
    /// Drawing a new shape.
    Crosshair,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PointerEvent {
    pub x: f64,
    pub y: f64,
    pub shift: bool,
    /// Held while dragging: disables smart guides.
    pub alt: bool,
    pub meta: bool,
    pub clicks: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    pub ctrl: bool,
}

/// What a right click landed on (`Editor::context_target`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextTarget {
    Blank,
    Object,
    Text,
}

/// Hit-test tolerances in EMU, derived from screen pixels by the host.
#[derive(Clone, Copy, Debug)]
pub struct Tolerance {
    pub handle: f64,
    /// Distance from the top edge to the rotation handle.
    pub rotate_offset: f64,
    /// Smart guide snapping distance.
    pub snap: f64,
}

/// A smart guide line shown while dragging, in slide space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub vertical: bool,
    /// x for vertical guides, y for horizontal ones.
    pub pos: f64,
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignTo {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Bold,
    Italic,
    Underline,
    Strike,
    Superscript,
    Subscript,
}

/// Baseline shifts PowerPoint uses for superscript and subscript.
const SUPERSCRIPT: f32 = 0.3;
const SUBSCRIPT: f32 = -0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZOrder {
    Front,
    Forward,
    Backward,
    Back,
}

/// Text formatting at the caret/selection, for toolbar state.
#[derive(Clone, Debug, PartialEq)]
pub struct TextFormat {
    pub style: RunStyle,
    pub align: Align,
    pub bullet: bool,
    /// Numbering scheme when the paragraph is numbered (`arabicPeriod`, ...).
    pub numbering: Option<String>,
    pub level: u8,
    pub line_spacing: f32,
    /// Left margin, first-line offset (negative: hanging) in EMU, and the space before and
    /// after the paragraph in points.
    pub mar_l: i64,
    pub indent: i64,
    pub space_before: f32,
    pub space_after: f32,
}

/// A slide on the slide clipboard, with what it needs to land in another deck.
#[derive(Clone)]
pub struct SlideClip {
    slide: Slide,
    /// Name of the slide's layout, to find the same one in the deck it is pasted into.
    layout_name: Option<String>,
    /// The pictures it shows, by key.
    media: Vec<(String, std::sync::Arc<Vec<u8>>)>,
}

#[derive(Clone)]
struct Snapshot {
    pres: Presentation,
    slide: usize,
    selection: Vec<ShapeId>,
}

#[derive(Clone, Copy, PartialEq)]
enum DragKind {
    Move,
    Resize(Handle),
    Rotate,
    TextSelect,
    Marquee { additive: bool },
    /// Selecting a block of cells on a table.
    CellSelect(ShapeId),
    /// Dragging the border after table column / row `index`.
    TableCol(usize),
    TableRow(usize),
    /// Drawing a new shape with the picked tool.
    Draw(ShapeId),
    /// Moving drawing guide `index`.
    Guide(usize),
}

struct Drag {
    kind: DragKind,
    start: (f64, f64),
    origs: Vec<(ShapeId, Xfrm)>,
    base_selection: Vec<ShapeId>,
    before: Option<Snapshot>,
    /// Take `before` on the first real move rather than on pointer down, so a plain
    /// click (select without moving) does not copy the whole presentation.
    snapshot_on_move: bool,
    moved: bool,
}

pub struct TextEdit {
    pub shape: ShapeId,
    /// The table cell (row, column) being edited, for tables.
    pub cell: Option<(usize, usize)>,
    /// Where the text sits: the shape (with its preset's text rectangle), or the table cell.
    frame: TextFrame,
    pub rich: RichEdit,
    original: TextBody,
    /// The shape's frame before editing (tables grow while typing).
    original_xfrm: Xfrm,
    /// Remove the shape when editing ends with no text (new text boxes).
    remove_if_empty: bool,
    /// SmartArt: (index of the drawn shape, row of its node) when editing a node's text in place.
    pub diagram: Option<(usize, usize)>,
}

/// What the host needs to draw the text being edited.
pub struct TextEditView<'a> {
    /// Maps editor layout space (points) to slide space.
    pub transform: Affine,
    pub layout: &'a BodyLayout,
    pub selection: Vec<BoundingBox>,
    pub caret: Option<BoundingBox>,
    pub compose: Vec<BoundingBox>,
    /// IME candidate window anchor, in layout space.
    pub ime_area: BoundingBox,
}

pub struct Editor {
    pub pres: Presentation,
    pub slide: usize,
    /// Selected shapes on the current slide, in selection order.
    pub selection: Vec<ShapeId>,
    pub text: Option<TextEdit>,
    /// Smart guides to draw for the current drag.
    pub guides: Vec<Guide>,
    /// Rubber-band rectangle (x0, y0, x1, y1) while marquee-selecting.
    pub marquee: Option<(f64, f64, f64, f64)>,
    drag: Option<Drag>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    clipboard: Vec<Shape>,
    /// What the format painter copied.
    copied_format: Option<painter::CopiedFormat>,
    /// How many times the clipboard was pasted onto its source slide (for offsets).
    paste_count: i64,
    clipboard_slide: Option<usize>,
    /// Bumped on every document change; lets hosts cache thumbnails and know about unsaved edits.
    revision: u64,
    /// Laid-out row heights of the current slide's tables, for hit testing without a text engine.
    table_heights: HashMap<ShapeId, Vec<i64>>,
    /// The group whose members are being selected individually.
    pub entered_group: Option<ShapeId>,
    /// Selected block of table cells (instead of text in one cell).
    pub cell_range: Option<CellRange>,
    /// A click on a member of the selected group enters the group on release,
    /// unless the pointer moved (then it was a move): (group, member).
    pending_enter: Option<(ShapeId, ShapeId)>,
    /// The slide whose notes typing is folded into the last undo step.
    notes_undo: Option<usize>,
    /// The deck before a recording session's narrations (one undo step for all of them).
    recording_before: Option<Snapshot>,
    /// The picture in crop mode: its handles trim it instead of resizing it.
    pub crop: Option<ShapeId>,
    /// The shape the next drag draws (picked in the gallery).
    pub draw_tool: Option<String>,
    /// The tool of the shape being drawn.
    draw_shape_tool: Option<String>,
    /// The SmartArt row whose typing is folded into the last undo step.
    diagram_undo: Option<(ShapeId, usize)>,
    /// Bumped when a SmartArt is double-clicked: the host opens its text pane
    /// at `diagram_focus`.
    pub diagram_requests: u32,
    pub diagram_focus: Option<usize>,
    /// Text to select in that row (a find match), as a byte range.
    pub diagram_range: Option<std::ops::Range<usize>>,
    /// The SmartArt find match last shown, to continue searching from.
    node_match: Option<FindMatch>,
}

/// A snap target: (position, extent start, extent end).
type SnapLine = (f64, f64, f64);

/// Axis-aligned bounds of a possibly rotated shape.
fn bounds(x: &Xfrm) -> (f64, f64, f64, f64) {
    let (w, h) = (x.w as f64, x.h as f64);
    let pts = [x.to_slide(0.0, 0.0), x.to_slide(w, 0.0), x.to_slide(w, h), x.to_slide(0.0, h)];
    pts.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(x0, y0, x1, y1), p| (x0.min(p.0), y0.min(p.1), x1.max(p.0), y1.max(p.1)))
}

impl Editor {
    pub fn new(pres: Presentation) -> Self {
        Self {
            pres,
            slide: 0,
            selection: Vec::new(),
            text: None,
            guides: Vec::new(),
            marquee: None,
            drag: None,
            undo: Vec::new(),
            redo: Vec::new(),
            clipboard: Vec::new(),
            copied_format: None,
            paste_count: 0,
            clipboard_slide: None,
            revision: 0,
            table_heights: HashMap::new(),
            entered_group: None,
            pending_enter: None,
            notes_undo: None,
            recording_before: None,
            cell_range: None,
            crop: None,
            draw_tool: None,
            draw_shape_tool: None,
            diagram_undo: None,
            diagram_requests: 0,
            diagram_focus: None,
            diagram_range: None,
            node_match: None,
        }
    }

    pub fn load(&mut self, pres: Presentation) {
        *self = Editor::new(pres);
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn changed(&mut self) {
        self.revision += 1;
    }

    pub fn set_slide(&mut self, engine: &mut TextEngine, index: usize) {
        self.end_text_edit(engine);
        self.slide = index.min(self.pres.slides.len().saturating_sub(1));
        self.selection.clear();
        self.crop = None;
        self.entered_group = None;
        self.cell_range = None;
        self.refresh_tables(engine);
    }

    /// Starts editing the current slide's notes; the edits until the next
    /// `begin_notes_edit` (or any other change) form one undo step.
    pub fn begin_notes_edit(&mut self, engine: &mut TextEngine) {
        self.end_text_edit(engine);
        self.notes_undo = None;
    }

    pub fn notes(&self, index: usize) -> &str {
        self.pres.slides.get(index).map_or("", |s| s.notes.as_str())
    }

    /// Replaces the current slide's notes.
    pub fn set_notes(&mut self, text: &str) -> bool {
        let slide = self.slide;
        if self.pres.slides.get(slide).is_none_or(|s| s.notes == text) {
            return false;
        }
        if self.notes_undo == Some(slide) {
            self.changed();
        } else {
            let snap = self.snapshot();
            self.push_undo(snap);
            self.notes_undo = Some(slide);
        }
        self.pres.slide_mut(slide).unwrap().notes = text.to_string();
        true
    }

    /// Sets the current slide's transition, or every slide's with `all`.
    pub fn set_transition(&mut self, engine: &mut TextEngine, t: Option<ppt_model::Transition>, all: bool) -> bool {
        let targets: Vec<usize> = if all { (0..self.pres.slides.len()).collect() } else { vec![self.slide] };
        if targets.iter().all(|&i| self.pres.slides.get(i).is_none_or(|s| s.transition == t)) {
            return false;
        }
        self.end_text_edit(engine);
        let snap = self.snapshot();
        self.push_undo(snap);
        for i in targets {
            if let Some(s) = self.pres.slide_mut(i) {
                s.transition = t.clone();
            }
        }
        true
    }

    fn current_shapes(&self) -> &[Shape] {
        self.pres.slides.get(self.slide).map(|s| s.shapes.as_slice()).unwrap_or(&[])
    }

    pub fn is_selected(&self, id: ShapeId) -> bool {
        self.selection.contains(&id)
    }

    /// The single selected shape, if exactly one is selected.
    pub fn selected_shape(&self) -> Option<&Shape> {
        match self.selection.as_slice() {
            [id] => self.pres.shape(self.slide, *id),
            _ => None,
        }
    }

    pub fn selected_shapes(&self) -> Vec<&Shape> {
        self.current_shapes().iter().filter(|s| self.selection.contains(&s.id)).collect()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.text.as_ref().is_some_and(|t| t.rich.can_undo())
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { pres: self.pres.clone(), slide: self.slide, selection: self.selection.clone() }
    }

    fn push_undo(&mut self, snap: Snapshot) {
        self.notes_undo = None;
        self.diagram_undo = None;
        self.undo.push(snap);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.changed();
    }

    pub fn undo(&mut self, engine: &mut TextEngine) -> bool {
        self.notes_undo = None;
        self.diagram_undo = None;
        // Inside a text box, undo typing first.
        if let Some(edit) = &mut self.text
            && edit.rich.can_undo()
        {
            edit.rich.undo(engine);
            self.fit_edited_shape(engine);
            return true;
        }
        self.end_text_edit(engine);
        let Some(snap) = self.undo.pop() else { return false };
        self.redo.push(self.snapshot());
        self.restore(snap);
        self.refresh_tables(engine);
        true
    }

    pub fn redo(&mut self, engine: &mut TextEngine) -> bool {
        self.notes_undo = None;
        self.diagram_undo = None;
        if let Some(edit) = &mut self.text
            && edit.rich.redo(engine)
        {
            self.fit_edited_shape(engine);
            return true;
        }
        self.end_text_edit(engine);
        let Some(snap) = self.redo.pop() else { return false };
        self.undo.push(self.snapshot());
        self.restore(snap);
        self.refresh_tables(engine);
        true
    }

    fn restore(&mut self, snap: Snapshot) {
        self.pres = snap.pres;
        self.slide = snap.slide.min(self.pres.slides.len().saturating_sub(1));
        let slide = self.slide;
        self.selection = snap.selection.into_iter().filter(|id| self.pres.shape(slide, *id).is_some()).collect();
        self.entered_group = None;
        self.cell_range = None;
        self.crop = None;
        self.changed();
    }

    /// Runs `f` on every selected shape as one undo step.
    fn modify_selected(&mut self, f: impl Fn(&mut Shape)) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let selection = self.selection.clone();
        let Some(slide) = self.pres.slide_mut(self.slide) else { return false };
        for shape in slide.shapes.iter_mut().filter(|s| selection.contains(&s.id)) {
            f(shape);
        }
        self.push_undo(before);
        true
    }

    // --- Hit testing -------------------------------------------------------

    /// Topmost selectable shape under the point.
    pub fn hit_test(&self, x: f64, y: f64, tol: f64) -> Option<ShapeId> {
        self.current_shapes()
            .iter()
            .rev()
            .filter(|s| selectable(s))
            .find(|s| shape_contains(s, x, y, tol))
            .map(|s| s.id)
    }

    /// Handle positions of the single selected shape, in slide space.
    pub fn handles(&self, tol: &Tolerance) -> Vec<(Handle, (f64, f64))> {
        // A locked shape has no handles: it can't be resized or rotated.
        if self.selection_locked() {
            return Vec::new();
        }
        if let Some(shape) = self.selected_shape() {
            let points = handle_points(&shape.xfrm, shape.geometry.is_line(), tol.rotate_offset);
            // A picture being cropped doesn't rotate.
            if self.crop == Some(shape.id) {
                return points.into_iter().filter(|h| h.0 != Handle::Rotate).collect();
            }
            return points;
        }
        // A selected group resizes as a whole (it does not rotate).
        let Some(frame) = self.selection_frame() else { return Vec::new() };
        handle_points(&frame, false, tol.rotate_offset).into_iter().filter(|h| h.0 != Handle::Rotate).collect()
    }

    fn handle_at(&self, x: f64, y: f64, tol: &Tolerance) -> Option<Handle> {
        if self.text.is_some() {
            return None;
        }
        self.handles(tol).into_iter().find(|(_, (hx, hy))| (hx - x).hypot(hy - y) <= tol.handle).map(|(h, _)| h)
    }

    pub fn hover_cursor(&self, ev: &PointerEvent, tol: &Tolerance) -> CursorIcon {
        if self.draw_tool.is_some() {
            return CursorIcon::Crosshair;
        }
        if let Some(drag) = &self.drag {
            return match drag.kind {
                DragKind::Move => CursorIcon::Move,
                DragKind::Rotate => CursorIcon::Rotate,
                DragKind::TextSelect => CursorIcon::Text,
                DragKind::Marquee { .. } => CursorIcon::Default,
                DragKind::Resize(h) => resize_cursor(h, &drag.origs.first().map(|o| o.1).unwrap_or_default()),
                DragKind::CellSelect(_) => CursorIcon::Text,
                DragKind::Draw(_) => CursorIcon::Crosshair,
                DragKind::TableCol(_) => CursorIcon::Resize(0.0),
                DragKind::TableRow(_) => CursorIcon::Resize(90.0),
                DragKind::Guide(i) => guide_cursor(self.pres.view.guides.get(i).is_some_and(|g| g.vertical)),
            };
        }
        if let Some(edit) = &self.text
            && frame_contains(&edit.frame.xfrm, ev.x, ev.y, tol.handle)
        {
            return CursorIcon::Text;
        }
        if self.handle_at(ev.x, ev.y, tol).is_none()
            && let Some(cursor) = self.table_cursor(ev, tol)
        {
            return cursor;
        }
        if let Some(h) = self.handle_at(ev.x, ev.y, tol) {
            let xfrm = self.selected_shape().map(|s| s.xfrm).unwrap_or_default();
            return if h == Handle::Rotate { CursorIcon::Rotate } else { resize_cursor(h, &xfrm) };
        }
        if self.hit_test(ev.x, ev.y, tol.handle).is_some() {
            return CursorIcon::Move;
        }
        match self.guide_at(ev.x, ev.y, tol.handle) {
            Some(i) => guide_cursor(self.pres.view.guides[i].vertical),
            None => CursorIcon::Default,
        }
    }

    /// The guide being dragged.
    pub fn dragged_guide(&self) -> Option<usize> {
        match self.drag.as_ref()?.kind {
            DragKind::Guide(i) => Some(i),
            _ => None,
        }
    }

    // --- Pointer -----------------------------------------------------------

    fn origs(&self) -> Vec<(ShapeId, Xfrm)> {
        self.selected_shapes().iter().map(|s| (s.id, s.xfrm)).collect()
    }

    /// Right click: what is under the pointer. An object that isn't selected yet becomes the selection
    /// (as with PowerPoint); a click inside the text being edited keeps editing, any other click ends it.
    pub fn context_target(&mut self, engine: &mut TextEngine, ev: &PointerEvent, tol: &Tolerance) -> ContextTarget {
        let hit = self.hit_test(ev.x, ev.y, tol.handle);
        if let Some(edit) = &self.text {
            if hit == Some(edit.shape) {
                return ContextTarget::Text;
            }
            self.end_text_edit(engine);
        }
        match hit {
            Some(id) => {
                if !self.is_selected(id) {
                    self.cell_range = None;
                    self.selection = self.click_target(id);
                }
                ContextTarget::Object
            }
            None => {
                self.cell_range = None;
                self.selection.clear();
                ContextTarget::Blank
            }
        }
    }

    /// Returns true when the view needs to be redrawn.
    pub fn pointer_down(&mut self, engine: &mut TextEngine, ev: &PointerEvent, tol: &Tolerance) -> bool {
        if self.draw_tool.is_some() {
            self.end_text_edit(engine);
            self.cell_range = None;
            return self.begin_draw(ev);
        }
        // Shift+click on another cell of the same table selects the block of cells in between.
        if ev.shift
            && let Some((id, from)) = self.range_origin()
            && let Some(to) = self.cell_at(id, ev.x, ev.y, tol.handle)
            && (self.cell_range.is_some() || Some(to) != self.text.as_ref().and_then(|t| t.cell))
        {
            self.select_cells(engine, id, from, to);
            self.drag = Some(Drag { kind: DragKind::CellSelect(id), start: (ev.x, ev.y), origs: Vec::new(), base_selection: Vec::new(), before: None, snapshot_on_move: false, moved: false });
            return true;
        }
        self.cell_range = None;
        // Clicks inside the text being edited move the caret.
        if let Some(frame) = self.text.as_ref().map(|t| t.frame.xfrm) {
            let inside = frame_contains(&frame, ev.x, ev.y, tol.handle);
            if inside {
                if let Some((x, y)) = self.to_text_space(ev.x, ev.y)
                    && let Some(edit) = &mut self.text
                {
                    edit.rich.pointer_down(x, y, ev.clicks, ev.shift);
                }
                self.drag = Some(Drag {
                    kind: DragKind::TextSelect,
                    start: (ev.x, ev.y),
                    origs: Vec::new(),
                    base_selection: Vec::new(),
                    before: None,
                    snapshot_on_move: false,
                    moved: false,
                });
                return true;
            }
            self.end_text_edit(engine);
        }

        // Crop mode ends with a click anywhere but on the picture's handles.
        if self.crop.is_some() && self.handle_at(ev.x, ev.y, tol).is_none() {
            self.crop = None;
        }
        if let Some(handle) = self.handle_at(ev.x, ev.y, tol) {
            let kind = if handle == Handle::Rotate { DragKind::Rotate } else { DragKind::Resize(handle) };
            self.drag = Some(Drag { kind, start: (ev.x, ev.y), origs: self.origs(), base_selection: Vec::new(), before: None, snapshot_on_move: true, moved: false });
            return true;
        }

        if !ev.shift && !ev.meta && self.table_pointer_down(engine, ev, tol) {
            return true;
        }

        // Guides are grabbed where no shape is.
        if self.hit_test(ev.x, ev.y, tol.handle).is_none()
            && let Some(i) = self.guide_at(ev.x, ev.y, tol.handle)
        {
            self.drag = Some(Drag { kind: DragKind::Guide(i), start: (ev.x, ev.y), origs: Vec::new(), base_selection: Vec::new(), before: None, snapshot_on_move: true, moved: false });
            return true;
        }
        match self.hit_test(ev.x, ev.y, tol.handle) {
            Some(id) => {
                let group = self.pres.shape(self.slide, id).and_then(|s| s.group);
                if group.is_some() && self.entered_group != group {
                    self.entered_group = None;
                }
                if ev.shift || ev.meta {
                    // Toggle membership (a whole group at a time); dragging still moves the selection.
                    let target = self.click_target(id);
                    if self.is_selected(id) {
                        self.selection.retain(|s| !target.contains(s));
                        return true;
                    }
                    self.selection.extend(target);
                } else if ev.clicks >= 2 && group.is_some() {
                    // Double click goes straight into the group.
                    self.entered_group = group;
                    self.selection = vec![id];
                } else if let Some(g) = self.in_selected_group(id) {
                    self.pending_enter = Some((g, id));
                } else if !self.is_selected(id) {
                    if group.is_none() || self.entered_group != group {
                        self.entered_group = None;
                    }
                    self.selection = self.click_target(id);
                }
                let shape = self.pres.shape(self.slide, id).expect("hit shape exists");
                if ev.clicks >= 2 && matches!(shape.kind, ShapeKind::Diagram { data: Some(_), .. }) {
                    // A node's own text is edited in place; the rest in the text pane, at the node clicked.
                    if self.begin_diagram_text_edit(engine, id, ev.x, ev.y) {
                        if let Some((x, y)) = self.to_text_space(ev.x, ev.y)
                            && let Some(edit) = &mut self.text
                        {
                            edit.rich.pointer_down(x, y, ev.clicks - 1, false);
                        }
                        return true;
                    }
                    self.diagram_focus = self.diagram_row_at(id, ev.x, ev.y);
                    self.diagram_range = None;
                    self.diagram_requests += 1;
                    return true;
                }
                if ev.clicks >= 2 && can_hold_text(shape) {
                    self.begin_text_edit(engine, id, false);
                    if let Some((x, y)) = self.to_text_space(ev.x, ev.y)
                        && let Some(edit) = &mut self.text
                    {
                        edit.rich.pointer_down(x, y, ev.clicks - 1, false);
                    }
                    return true;
                }
                if !self.selection_locked() {
                    self.drag = Some(Drag { kind: DragKind::Move, start: (ev.x, ev.y), origs: self.origs(), base_selection: Vec::new(), before: None, snapshot_on_move: true, moved: false });
                }
            }
            None => {
                let additive = ev.shift || ev.meta;
                if !additive {
                    self.selection.clear();
                    self.entered_group = None;
                }
                self.drag = Some(Drag {
                    kind: DragKind::Marquee { additive },
                    start: (ev.x, ev.y),
                    origs: Vec::new(),
                    base_selection: self.selection.clone(),
                    before: None,
                    snapshot_on_move: false,
                    moved: false,
                });
            }
        }
        true
    }

    pub fn pointer_move(&mut self, engine: &mut TextEngine, ev: &PointerEvent, tol: &Tolerance) -> bool {
        let Some(drag) = &mut self.drag else { return false };
        let (dx, dy) = (ev.x - drag.start.0, ev.y - drag.start.1);
        if !drag.moved && dx.hypot(dy) < tol.handle / 3.0 {
            return false;
        }
        let first_move = !drag.moved;
        drag.moved = true;
        let kind = drag.kind;
        let start = drag.start;
        let origs = drag.origs.clone();
        if first_move && drag.snapshot_on_move {
            // Nothing has changed since pointer down, so this is still the state before the drag.
            let snap = self.snapshot();
            if let Some(d) = &mut self.drag {
                d.before = Some(snap);
            }
        }
        match kind {
            DragKind::TextSelect => {
                // Dragging out of the edited cell into another one selects cells instead of text.
                if let Some((id, from)) = self.range_origin()
                    && let Some(to) = self.cell_at(id, ev.x, ev.y, tol.handle)
                    && to != from
                {
                    self.select_cells(engine, id, from, to);
                    if let Some(d) = &mut self.drag {
                        d.kind = DragKind::CellSelect(id);
                    }
                    return true;
                }
                if let Some((x, y)) = self.to_text_space(ev.x, ev.y)
                    && let Some(edit) = &mut self.text
                {
                    edit.rich.pointer_drag(x, y);
                }
            }
            DragKind::CellSelect(id) => {
                if let Some(to) = self.cell_at(id, ev.x, ev.y, tol.handle)
                    && let Some(range) = &mut self.cell_range
                {
                    range.focus = to;
                }
            }
            DragKind::TableCol(index) | DragKind::TableRow(index) => {
                let col = matches!(kind, DragKind::TableCol(_));
                self.drag_table_border(engine, col, index, if col { dx } else { dy });
            }
            DragKind::Marquee { additive } => {
                let rect = (start.0.min(ev.x), start.1.min(ev.y), start.0.max(ev.x), start.1.max(ev.y));
                self.marquee = Some(rect);
                let base = self.drag.as_ref().map(|d| d.base_selection.clone()).unwrap_or_default();
                let mut sel = if additive { base } else { Vec::new() };
                let inside = |s: &Shape| {
                    let (x0, y0, x1, y1) = bounds(&s.xfrm);
                    x0 >= rect.0 && y0 >= rect.1 && x1 <= rect.2 && y1 <= rect.3
                };
                let shapes = self.current_shapes();
                for s in shapes.iter().filter(|s| selectable(s)) {
                    // A group is picked only when all of it is inside the rectangle.
                    let whole = match s.group {
                        Some(g) if self.entered_group != Some(g) => shapes.iter().filter(|m| m.group == Some(g)).all(inside),
                        _ => inside(s),
                    };
                    if whole && !sel.contains(&s.id) {
                        sel.push(s.id);
                    }
                }
                self.selection = sel;
            }
            DragKind::Move => {
                let (mut dx, mut dy) = (dx, dy);
                if ev.shift {
                    // Constrain to the dominant axis.
                    if dx.abs() > dy.abs() { dy = 0.0 } else { dx = 0.0 }
                }
                self.guides.clear();
                if !ev.alt {
                    let moving: Vec<ShapeId> = origs.iter().map(|o| o.0).collect();
                    let union = origs.iter().map(|o| bounds(&o.1)).fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |a, b| {
                        (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
                    });
                    let moved = (union.0 + dx, union.1 + dy, union.2 + dx, union.3 + dy);
                    let (sx, sy, guides) = self.snap(&moving, moved, tol.snap, ev.shift && dx == 0.0, ev.shift && dy == 0.0);
                    dx += sx;
                    dy += sy;
                    self.guides = guides;
                }
                for (id, o) in &origs {
                    if let Some(shape) = self.pres.shape_mut(self.slide, *id) {
                        shape.xfrm = Xfrm { x: o.x + dx.round() as i64, y: o.y + dy.round() as i64, ..*o };
                    }
                }
            }
            DragKind::Draw(id) => self.draw_move(id, start, ev),
            DragKind::Guide(i) => self.guide_move(i, ev),
            DragKind::Resize(handle) if origs.len() > 1 => {
                // A group: scale every member with the group's frame.
                let Some(frame) = group::union_frame(origs.iter().map(|o| o.1)) else { return false };
                let new = resize(&frame, handle, dx, dy, ev.shift, MIN_SIZE);
                for (id, o) in &origs {
                    if let Some(shape) = self.pres.shape_mut(self.slide, *id) {
                        shape.xfrm = group::rescale(o, &frame, &new);
                    }
                }
            }
            DragKind::Resize(handle) if self.crop.is_some_and(|c| origs.first().is_some_and(|o| o.0 == c)) => {
                let (id, orig) = origs[0];
                let new = resize(&orig, handle, dx, dy, false, MIN_SIZE);
                let orig_crop = self.drag.as_ref().and_then(|d| d.before.as_ref()).and_then(|b| b.pres.shape(self.slide, id)).and_then(|s| match s.kind {
                    ShapeKind::Picture { crop, .. } => Some(crop),
                    _ => None,
                });
                if let Some(c) = orig_crop {
                    self.crop_drag(id, orig, c, handle, new);
                }
            }
            DragKind::Resize(handle) => {
                let Some(&(id, orig)) = origs.first() else { return false };
                let is_line = self.pres.shape(self.slide, id).is_some_and(|s| s.geometry.is_line());
                // "Lock aspect ratio" does what Shift does.
                let locked_aspect = self.pres.shape(self.slide, id).is_some_and(|s| s.access.lock_aspect);
                let mut new = resize(&orig, handle, dx, dy, ev.shift || locked_aspect, if is_line { 0.0 } else { MIN_SIZE });
                self.guides.clear();
                if !ev.alt && !ev.shift && !locked_aspect && orig.rot == 0.0 && !is_line {
                    new = self.snap_resize(id, new, handle, tol.snap);
                }
                let before = self.drag.as_ref().and_then(|d| d.before.as_ref()).and_then(|b| b.pres.shape(self.slide, id).cloned());
                if let Some(shape) = self.pres.shape_mut(self.slide, id) {
                    shape.xfrm = new;
                    // Tables scale their grid with the frame.
                    if let (ShapeKind::Table(t), Some(ShapeKind::Table(before))) = (&mut shape.kind, before.map(|b| b.kind)) {
                        table::scale_grid(t, &before, new.w as f64 / orig.w.max(1) as f64, new.h as f64 / orig.h.max(1) as f64);
                    }
                }
            }
            DragKind::Rotate => {
                let Some(&(id, orig)) = origs.first() else { return false };
                let (cx, cy) = orig.center();
                let mut angle = (ev.y - cy).atan2(ev.x - cx).to_degrees() + 90.0;
                if ev.shift {
                    angle = (angle / 15.0).round() * 15.0;
                }
                if let Some(shape) = self.pres.shape_mut(self.slide, id) {
                    shape.xfrm = Xfrm { rot: angle.rem_euclid(360.0), ..orig };
                }
            }
        }
        true
    }

    pub fn pointer_up(&mut self, _engine: &mut TextEngine, _ev: &PointerEvent) -> bool {
        self.guides.clear();
        let had_marquee = self.marquee.take().is_some();
        let pending = self.pending_enter.take();
        let Some(drag) = self.drag.take() else { return had_marquee };
        if let DragKind::Draw(id) = drag.kind {
            self.end_draw(id, drag.start, drag.moved);
            if let Some(before) = drag.before {
                self.push_undo(before);
            }
            return true;
        }
        if let DragKind::Guide(i) = drag.kind {
            if !drag.moved {
                return false;
            }
            self.guide_end(i);
            if let Some(before) = drag.before {
                self.push_undo(before);
            }
            return true;
        }
        if !drag.moved
            && let Some((group, member)) = pending
        {
            self.entered_group = Some(group);
            self.selection = vec![member];
            return true;
        }
        if drag.moved
            && let Some(before) = drag.before
        {
            self.push_undo(before);
        }
        drag.moved || had_marquee
    }

    /// Snap lines from the slide and the other shapes, as (vertical lines, horizontal lines).
    fn snap_targets(&self, exclude: &[ShapeId]) -> (Vec<SnapLine>, Vec<SnapLine>) {
        let (w, h) = (self.pres.slide_width as f64, self.pres.slide_height as f64);
        let mut xs = vec![(0.0, 0.0, h), (w / 2.0, 0.0, h), (w, 0.0, h)];
        let mut ys = vec![(0.0, 0.0, w), (h / 2.0, 0.0, w), (h, 0.0, w)];
        if self.pres.view.show_guides {
            for g in &self.pres.view.guides {
                if g.vertical { xs.push((g.pos as f64, 0.0, h)) } else { ys.push((g.pos as f64, 0.0, w)) }
            }
        }
        for s in self.current_shapes().iter().filter(|s| !exclude.contains(&s.id)) {
            let (x0, y0, x1, y1) = bounds(&s.xfrm);
            for x in [x0, (x0 + x1) / 2.0, x1] {
                xs.push((x, y0, y1));
            }
            for y in [y0, (y0 + y1) / 2.0, y1] {
                ys.push((y, x0, x1));
            }
        }
        (xs, ys)
    }

    /// Returns the correction to apply to a moving box and the guides to show.
    fn snap(&self, moving: &[ShapeId], b: (f64, f64, f64, f64), tol: f64, lock_x: bool, lock_y: bool) -> (f64, f64, Vec<Guide>) {
        let (xs, ys) = self.snap_targets(moving);
        let best = |edges: [f64; 3], targets: &[SnapLine]| {
            let mut best: Option<(f64, SnapLine)> = None;
            for e in edges {
                for t in targets {
                    let d = t.0 - e;
                    if d.abs() <= tol && best.is_none_or(|(bd, _)| d.abs() < bd.abs()) {
                        best = Some((d, *t));
                    }
                }
            }
            best
        };
        let mut guides = Vec::new();
        let mut sx = 0.0;
        let mut sy = 0.0;
        let grid = self.pres.view.snap_to_grid;
        if !lock_x {
            match best([b.0, (b.0 + b.2) / 2.0, b.2], &xs) {
                Some((d, _)) => sx = d,
                // Otherwise the left edge goes to the nearest grid line.
                None if grid => sx = self.grid_round(b.0, self.pres.slide_width) - b.0,
                None => {}
            }
        }
        if !lock_y {
            match best([b.1, (b.1 + b.3) / 2.0, b.3], &ys) {
                Some((d, _)) => sy = d,
                None if grid => sy = self.grid_round(b.1, self.pres.slide_height) - b.1,
                None => {}
            }
        }
        // Report every line the snapped box now touches, like PowerPoint.
        let b = (b.0 + sx, b.1 + sy, b.2 + sx, b.3 + sy);
        if sx != 0.0 || xs.iter().any(|t| [b.0, (b.0 + b.2) / 2.0, b.2].iter().any(|e| (t.0 - e).abs() < 1.0)) {
            for t in &xs {
                if [b.0, (b.0 + b.2) / 2.0, b.2].iter().any(|e| (t.0 - e).abs() < 1.0) {
                    guides.push(Guide { vertical: true, pos: t.0, from: t.1.min(b.1), to: t.2.max(b.3) });
                }
            }
        }
        if sy != 0.0 || ys.iter().any(|t| [b.1, (b.1 + b.3) / 2.0, b.3].iter().any(|e| (t.0 - e).abs() < 1.0)) {
            for t in &ys {
                if [b.1, (b.1 + b.3) / 2.0, b.3].iter().any(|e| (t.0 - e).abs() < 1.0) {
                    guides.push(Guide { vertical: false, pos: t.0, from: t.1.min(b.0), to: t.2.max(b.2) });
                }
            }
        }
        (sx, sy, guides)
    }

    /// Snaps the edges a resize handle moves (unrotated shapes only).
    fn snap_resize(&mut self, id: ShapeId, mut x: Xfrm, handle: Handle, tol: f64) -> Xfrm {
        let (xs, ys) = self.snap_targets(&[id]);
        let (sx, sy) = handle.sign();
        let nearest = |v: f64, targets: &[SnapLine]| {
            targets.iter().map(|t| (t.0 - v, *t)).filter(|(d, _)| d.abs() <= tol).min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
        };
        if sx != 0.0 {
            let edge = if sx > 0.0 { (x.x + x.w) as f64 } else { x.x as f64 };
            if let Some((d, t)) = nearest(edge, &xs) {
                let d = d.round() as i64;
                if sx > 0.0 { x.w += d } else { x.x += d; x.w -= d }
                self.guides.push(Guide { vertical: true, pos: t.0, from: t.1.min(x.y as f64), to: t.2.max((x.y + x.h) as f64) });
            } else if self.pres.view.snap_to_grid {
                let d = (self.grid_round(edge, self.pres.slide_width) - edge).round() as i64;
                if sx > 0.0 { x.w += d } else { x.x += d; x.w -= d }
            }
        }
        if sy != 0.0 {
            let edge = if sy > 0.0 { (x.y + x.h) as f64 } else { x.y as f64 };
            if let Some((d, t)) = nearest(edge, &ys) {
                let d = d.round() as i64;
                if sy > 0.0 { x.h += d } else { x.y += d; x.h -= d }
                self.guides.push(Guide { vertical: false, pos: t.0, from: t.1.min(x.x as f64), to: t.2.max((x.x + x.w) as f64) });
            } else if self.pres.view.snap_to_grid {
                let d = (self.grid_round(edge, self.pres.slide_height) - edge).round() as i64;
                if sy > 0.0 { x.h += d } else { x.y += d; x.h -= d }
            }
        }
        x
    }

    // --- Text editing ------------------------------------------------------

    pub fn is_editing_text(&self) -> bool {
        self.text.is_some()
    }

    fn begin_text_edit(&mut self, engine: &mut TextEngine, id: ShapeId, remove_if_empty: bool) {
        let default_style = self.pres.default_run_style();
        let text_defaults = self.pres.text_defaults.clone();
        let Some(shape) = self.pres.shape_mut(self.slide, id) else { return };
        let body = shape.text.get_or_insert_with(|| {
            // Shapes without text get centered white text, like PowerPoint's default shape style.
            let mut body = TextBody { anchor: Anchor::Middle, defaults: text_defaults, ..Default::default() };
            body.paragraphs[0].align = Align::Center;
            body.paragraphs[0].end_style = RunStyle { color: Color::WHITE, ..default_style };
            body
        });
        let original = body.clone();
        let frame = TextFrame::for_text(shape.xfrm, &shape.geometry, Some(body));
        let width = text_width_pt(&frame, body);
        let rich = RichEdit::new(engine, original.clone(), width);
        self.selection = vec![id];
        self.text = Some(TextEdit { shape: id, cell: None, frame, rich, original, original_xfrm: shape.xfrm, remove_if_empty, diagram: None });
    }

    /// Commits the edited text into the model and leaves text mode.
    pub fn end_text_edit(&mut self, engine: &mut TextEngine) {
        let Some(mut edit) = self.text.take() else { return };
        edit.rich.clear_compose(engine);
        let body = edit.rich.body;
        if let Some((_, row)) = edit.diagram {
            // The node keeps plain text; the drawing is laid out again from it.
            let text = body.plain_text();
            if self.node_text(self.slide, edit.shape, row).is_some_and(|old| old != text) {
                self.diagram_undo = None;
                self.diagram_set_text_of(engine, edit.shape, row, &text);
            }
            return;
        }
        if edit.remove_if_empty && body.is_empty() {
            if let Some(slide) = self.pres.slide_mut(self.slide) {
                slide.shapes.retain(|s| s.id != edit.shape);
            }
            self.selection.clear();
            self.changed();
            return;
        }
        if body == edit.original {
            return;
        }
        // The snapshot must show the text as it was before editing.
        let mut before = self.snapshot();
        let put = |shape: &mut Shape, body: TextBody| match edit.cell {
            Some((r, c)) => {
                if let ShapeKind::Table(t) = &mut shape.kind
                    && let Some(cell) = t.cell_mut(r, c)
                {
                    cell.text = body;
                }
            }
            None => shape.text = Some(body),
        };
        if let Some(s) = before.pres.shape_mut(self.slide, edit.shape) {
            put(s, edit.original.clone());
            if edit.cell.is_some() {
                s.xfrm = edit.original_xfrm;
            }
        }
        if let Some(shape) = self.pres.shape_mut(self.slide, edit.shape) {
            put(shape, body);
            self.push_undo(before);
        }
    }

    /// Maps a slide-space point to the edited text's layout space.
    fn to_text_space(&self, x: f64, y: f64) -> Option<(f32, f32)> {
        let edit = self.text.as_ref()?;
        let p = text_affine(&edit.frame, &edit.rich.body, edit.rich.layout().height).inverse() * Point::new(x, y);
        Some((p.x as f32, p.y as f32))
    }

    /// Starts editing the selected shape when the user just types, replacing its text.
    fn ensure_text_edit_for_typing(&mut self, engine: &mut TextEngine) -> bool {
        if self.text.is_some() {
            return true;
        }
        let Some(shape) = self.selected_shape() else { return false };
        if !can_hold_text(shape) {
            return false;
        }
        let id = shape.id;
        self.begin_text_edit(engine, id, false);
        if let Some(edit) = &mut self.text {
            edit.rich.select_all();
        }
        true
    }

    pub fn insert_text(&mut self, engine: &mut TextEngine, text: &str) -> bool {
        if text.is_empty() || !self.ensure_text_edit_for_typing(engine) {
            return false;
        }
        self.text.as_mut().expect("text edit active").rich.insert(engine, text);
        self.fit_edited_shape(engine);
        true
    }

    /// IME preedit update. `cursor` is a byte offset inside the preedit text.
    pub fn compose_update(&mut self, engine: &mut TextEngine, text: &str, cursor: usize) -> bool {
        if !self.ensure_text_edit_for_typing(engine) {
            return false;
        }
        self.text.as_mut().expect("text edit active").rich.set_compose(engine, text, cursor);
        self.fit_edited_shape(engine);
        true
    }

    /// IME commit: the preedit is replaced by the final text.
    pub fn compose_end(&mut self, engine: &mut TextEngine, text: &str) -> bool {
        let Some(edit) = self.text.as_mut() else { return self.insert_text(engine, text) };
        edit.rich.commit_compose(engine, text);
        self.fit_edited_shape(engine);
        true
    }

    pub fn is_composing(&self) -> bool {
        self.text.as_ref().is_some_and(|t| t.rich.is_composing())
    }

    pub fn text_view(&self) -> Option<TextEditView<'_>> {
        let edit = self.text.as_ref()?;
        let layout = edit.rich.layout();
        Some(TextEditView {
            transform: text_affine(&edit.frame, &edit.rich.body, layout.height),
            layout,
            selection: edit.rich.selection_rects(),
            caret: edit.rich.caret(),
            compose: edit.rich.compose_rects(),
            ime_area: edit.rich.ime_area(),
        })
    }

    /// Grows or shrinks an auto-fit (`spAutoFit`) shape to the edited text's height.
    fn fit_edited_shape(&mut self, engine: &mut TextEngine) {
        if self.text.as_ref().is_some_and(|t| t.cell.is_some()) {
            return self.fit_edited_cell(engine);
        }
        let Some(edit) = &mut self.text else { return };
        if edit.diagram.is_some() {
            return;
        }
        let id = edit.shape;
        let Some(shape) = self.pres.slide_mut(self.slide).and_then(|s| s.shapes.iter_mut().find(|s| s.id == id)) else { return };
        edit.frame = TextFrame::of(shape.xfrm, &shape.geometry);
        edit.rich.set_width(engine, text_width_pt(&edit.frame, &edit.rich.body));
        let body = &edit.rich.body;
        if !body.auto_grow {
            return;
        }
        let min_line = body.paragraphs.first().map(|p| p.first_style().size_pt * SINGLE_LINE_HEIGHT).unwrap_or(0.0);
        let content = edit.rich.layout().height.max(min_line) as f64;
        shape.xfrm.h = (content * EMU_PER_PT).round() as i64 + body.insets[1] + body.insets[3];
        edit.frame = TextFrame::of(shape.xfrm, &shape.geometry);
    }

    pub fn selected_text(&self) -> Option<String> {
        self.text.as_ref()?.rich.selected_text()
    }

    /// Deletes the text selection, used by cut.
    pub fn delete_text_selection(&mut self, engine: &mut TextEngine) -> bool {
        let Some(edit) = self.text.as_mut() else { return false };
        edit.rich.delete_selection(engine);
        self.fit_edited_shape(engine);
        true
    }

    // --- Formatting --------------------------------------------------------

    /// Applies a character format to the text selection, or to all text of the selected shapes.
    pub fn apply_text_style(&mut self, engine: &mut TextEngine, f: impl Fn(&mut RunStyle)) -> bool {
        if self.cell_range.is_some() {
            return self.style_cells(engine, &f);
        }
        if let Some(edit) = &mut self.text {
            edit.rich.apply_style(engine, f);
            self.fit_edited_shape(engine);
            return true;
        }
        if !self.selected_shapes().iter().any(|s| s.text_bodies().next().is_some()) {
            return false;
        }
        self.modify_selected(|shape| {
            for body in shape.text_bodies_mut() {
                let len = body.len();
                body.apply_style(0..len, &f);
            }
        })
    }

    /// Links the selected text (or the selected shapes' text) to `url`, or removes links with `None`.
    pub fn set_link(&mut self, engine: &mut TextEngine, url: Option<&str>) -> bool {
        let link = url.filter(|u| !u.trim().is_empty()).map(|u| std::sync::Arc::new(Link::url(u.trim(), self.pres.link_color)));
        self.apply_text_style(engine, |s| s.link = link.clone())
    }

    /// Bold/italic/underline toggle: turns it off only when the current text already has it.
    pub fn toggle_style(&mut self, engine: &mut TextEngine, which: Toggle) -> bool {
        let Some(current) = self.text_format().map(|f| f.style) else { return false };
        let on = !match which {
            Toggle::Bold => current.bold,
            Toggle::Italic => current.italic,
            Toggle::Underline => current.underline,
            Toggle::Strike => current.strike,
            Toggle::Superscript => current.baseline > 0.0,
            Toggle::Subscript => current.baseline < 0.0,
        };
        self.apply_text_style(engine, |s| match which {
            Toggle::Bold => s.bold = on,
            Toggle::Italic => s.italic = on,
            Toggle::Underline => s.underline = on,
            Toggle::Strike => s.strike = on,
            Toggle::Superscript => s.baseline = if on { SUPERSCRIPT } else { 0.0 },
            Toggle::Subscript => s.baseline = if on { SUBSCRIPT } else { 0.0 },
        })
    }

    /// Resets character formatting to what the text inherits: the selected
    /// text, or all text of the selected shapes.
    pub fn clear_formatting(&mut self, engine: &mut TextEngine) -> bool {
        if self.cell_range.is_some() {
            return false;
        }
        if let Some(edit) = &mut self.text {
            edit.rich.edit_body(engine, |body, range| body.clear_formatting(range));
            self.fit_edited_shape(engine);
            return true;
        }
        if !self.selected_shapes().iter().any(|s| s.text_bodies().next().is_some()) {
            return false;
        }
        self.modify_selected(|shape| {
            for body in shape.text_bodies_mut() {
                let len = body.len();
                body.clear_formatting(0..len);
            }
        })
    }

    /// Where text sits vertically in the edited shape, or in the selected shapes.
    pub fn set_text_anchor(&mut self, anchor: Anchor) -> bool {
        if let Some(edit) = &mut self.text {
            if edit.cell.is_some() || edit.rich.body.anchor == anchor {
                return false;
            }
            edit.rich.body.anchor = anchor;
            self.changed();
            return true;
        }
        let ids: Vec<ShapeId> = self.selected_shapes().iter().filter(|s| s.text.as_ref().is_some_and(|t| t.anchor != anchor)).map(|s| s.id).collect();
        if ids.is_empty() {
            return false;
        }
        self.modify_selected(|shape| {
            if let Some(body) = &mut shape.text {
                body.anchor = anchor;
            }
        })
    }

    /// The vertical text position of the edited shape, or of the single selected shape.
    pub fn text_anchor(&self) -> Option<Anchor> {
        match &self.text {
            Some(edit) if edit.cell.is_none() => Some(edit.rich.body.anchor),
            Some(_) => None,
            None => self.selected_shape().and_then(|s| s.text.as_ref()).map(|t| t.anchor),
        }
    }

    /// Applies a paragraph format to the selected paragraphs, or to all paragraphs of the selected shapes.
    pub fn apply_paragraph(&mut self, engine: &mut TextEngine, f: impl Fn(&mut Paragraph)) -> bool {
        if self.cell_range.is_some() {
            return self.paragraph_cells(engine, &f);
        }
        if let Some(edit) = &mut self.text {
            edit.rich.apply_paragraphs(engine, f);
            self.fit_edited_shape(engine);
            return true;
        }
        if !self.selected_shapes().iter().any(|s| s.text_bodies().next().is_some()) {
            return false;
        }
        self.modify_selected(|shape| {
            for body in shape.text_bodies_mut() {
                let len = body.len();
                body.apply_paragraphs(0..len, &f);
            }
        })
    }

    /// Numbers the affected paragraphs with `scheme`, or removes numbering when
    /// they already use it.
    pub fn toggle_numbering(&mut self, engine: &mut TextEngine, scheme: &str) -> bool {
        let on = self.text_format().and_then(|f| f.numbering).as_deref() != Some(scheme);
        let scheme = scheme.to_string();
        self.apply_paragraph(engine, |p| {
            if on {
                // Keep a numbered list's own color/size if it had one; numbers use the text font.
                let base = p.bullet.clone().unwrap_or_else(|| Bullet::char('•'));
                p.bullet = Some(Bullet { kind: BulletKind::AutoNum { scheme: scheme.clone(), start: 1 }, font: None, ..base });
                if p.indent >= 0 {
                    p.mar_l = 342_900 * (p.level as i64 + 1);
                    p.indent = -342_900;
                }
            } else {
                p.bullet = None;
                p.indent = 0;
                p.mar_l = 0;
            }
        })
    }

    /// Toggles bullets on the affected paragraphs, with PowerPoint's default hanging indent.
    pub fn toggle_bullets(&mut self, engine: &mut TextEngine) -> bool {
        let on = !self.text_format().is_some_and(|f| f.bullet);
        self.apply_paragraph(engine, |p| {
            if on {
                p.bullet = Some(Bullet { font: Some("Arial".into()), ..Bullet::char('•') });
                if p.indent >= 0 {
                    p.mar_l = 285_750 * (p.level as i64 + 1);
                    p.indent = -285_750;
                }
            } else {
                p.bullet = None;
                p.indent = 0;
                p.mar_l = 0;
            }
        })
    }

    /// Changes the list level (indent) by `delta`.
    pub fn change_level(&mut self, engine: &mut TextEngine, delta: i32) -> bool {
        self.apply_paragraph(engine, |p| {
            let level = (p.level as i32 + delta).clamp(0, 8);
            let step = 457_200 * (level - p.level as i32) as i64;
            p.level = level as u8;
            p.mar_l = (p.mar_l + step).max(0);
        })
    }

    /// Formatting at the caret (or of the selected shape's first character).
    pub fn text_format(&self) -> Option<TextFormat> {
        if let Some(edit) = &self.text {
            let p = edit.rich.current_paragraph();
            return Some(TextFormat {
                style: edit.rich.typing_style(),
                align: p.align,
                bullet: p.bullet.as_ref().is_some_and(|b| !b.is_numbered()),
                numbering: numbering_of(p),
                level: p.level,
                line_spacing: p.line_spacing,
                mar_l: p.mar_l,
                indent: p.indent,
                space_before: p.space_before,
                space_after: p.space_after,
            });
        }
        let body = match self.range_text_body() {
            Some(b) => b,
            None => self.selected_shapes().into_iter().find_map(|s| s.text_bodies().next())?,
        };
        let p = body.paragraphs.first()?;
        Some(TextFormat {
            style: body.style_at(0),
            align: p.align,
            bullet: p.bullet.as_ref().is_some_and(|b| !b.is_numbered()),
            numbering: numbering_of(p),
            level: p.level,
            line_spacing: p.line_spacing,
            mar_l: p.mar_l,
            indent: p.indent,
            space_before: p.space_before,
            space_after: p.space_after,
        })
    }

    pub fn set_fill(&mut self, fill: Option<Color>) -> bool {
        self.modify_selected(|s| {
            if !s.geometry.is_line() && matches!(s.kind, ShapeKind::Shape) {
                s.fill = fill.map(Paint::Solid);
            }
        })
    }

    /// Sets the outline color (`None` = no outline), keeping the width.
    pub fn set_line_color(&mut self, color: Option<Color>) -> bool {
        self.modify_selected(|s| {
            s.line = color.map(|color| Line { color, ..s.line.unwrap_or(Line::new(color, 12_700)) });
        })
    }

    pub fn set_line_width(&mut self, width: i64) -> bool {
        self.modify_selected(|s| {
            let color = s.line.map(|l| l.color).unwrap_or(Color::BLACK);
            s.line = Some(Line { width, ..s.line.unwrap_or(Line::new(color, width)) });
        })
    }

    pub fn selection_fill(&self) -> Option<Option<Color>> {
        self.selected_shapes().first().map(|s| s.fill.as_ref().map(Paint::color))
    }

    pub fn selection_line(&self) -> Option<Option<Line>> {
        self.selected_shapes().first().map(|s| s.line)
    }

    // --- Keyboard ----------------------------------------------------------

    /// Returns true if the key was handled (the host should then prevent default).
    pub fn key_down(&mut self, engine: &mut TextEngine, key: &str, m: Modifiers) -> bool {
        if m.meta {
            match key.to_lowercase().as_str() {
                "z" => return if m.shift { self.redo(engine) } else { self.undo(engine) },
                "b" => {
                    self.toggle_style(engine, Toggle::Bold);
                    return true;
                }
                "i" => {
                    self.toggle_style(engine, Toggle::Italic);
                    return true;
                }
                "u" => {
                    self.toggle_style(engine, Toggle::Underline);
                    return true;
                }
                "l" => return self.apply_paragraph(engine, |p| p.align = Align::Left),
                "e" => return self.apply_paragraph(engine, |p| p.align = Align::Center),
                "r" => return self.apply_paragraph(engine, |p| p.align = Align::Right),
                "j" => return self.apply_paragraph(engine, |p| p.align = Align::Justify),
                "]" | "}" => return self.apply_text_style(engine, |s| s.size_pt = grow_size(s.size_pt, true)),
                "[" | "{" => return self.apply_text_style(engine, |s| s.size_pt = grow_size(s.size_pt, false)),
                "d" if self.text.is_none() => return self.duplicate_selected(),
                "g" | "G" if m.alt && self.text.is_none() => return if m.shift { self.ungroup_selected() } else { self.group_selected() },
                _ => {}
            }
        }
        if self.text.is_some() {
            return self.text_key(engine, key, m);
        }
        if self.cell_range.is_some() {
            match key {
                "Backspace" | "Delete" => return self.clear_cells(engine),
                "Escape" => {
                    self.cell_range = None;
                    return true;
                }
                _ => {}
            }
        }
        if self.draw_tool.is_some() && key == "Escape" {
            self.draw_tool = None;
            return true;
        }
        if self.crop.is_some() && matches!(key, "Escape" | "Enter") {
            self.crop = None;
            return true;
        }
        match key {
            "Backspace" | "Delete" => self.delete_selected(),
            "Escape" => {
                // Inside a group, Esc goes back to the whole group first.
                if let Some(g) = self.entered_group.take() {
                    self.selection = self.members(g);
                    return true;
                }
                let had = !self.selection.is_empty();
                self.selection.clear();
                had
            }
            // Enter on a group steps into it, selecting its first member.
            "Enter" if self.selection_group().is_some() => {
                let g = self.selection_group().expect("checked");
                self.entered_group = Some(g);
                self.selection = self.members(g).into_iter().take(1).collect();
                true
            }
            "Enter" | "F2" => {
                if let Some(id) = self.selected_shape().filter(|s| matches!(s.kind, ShapeKind::Table(_))).map(|s| s.id) {
                    self.begin_cell_edit(engine, id, 0, 0);
                    return true;
                }
                let Some(id) = self.selected_shape().filter(|s| can_hold_text(s)).map(|s| s.id) else { return false };
                self.begin_text_edit(engine, id, false);
                true
            }
            "Tab" => self.cycle_selection(!m.shift),
            "a" if m.meta => {
                self.selection = self.current_shapes().iter().filter(|s| selectable(s)).map(|s| s.id).collect();
                true
            }
            // Option+←/→ rotates by 15° (1° with Shift), like PowerPoint.
            "ArrowLeft" | "ArrowRight" if m.alt => {
                let deg = if m.shift { 1.0 } else { 15.0 };
                self.rotate_selection(if key == "ArrowLeft" { -deg } else { deg })
            }
            "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => {
                // ⌘ nudges finely past the grid; with snap to grid a step is one grid unit.
                let step = if m.shift {
                    EMU_PER_INCH / 10.0
                } else if self.pres.view.snap_to_grid && !m.meta {
                    self.pres.view.grid_spacing as f64
                } else if m.meta {
                    EMU_PER_INCH / 400.0
                } else {
                    EMU_PER_INCH / 100.0
                } as i64;
                let (dx, dy) = match key {
                    "ArrowLeft" => (-step, 0),
                    "ArrowRight" => (step, 0),
                    "ArrowUp" => (0, -step),
                    _ => (0, step),
                };
                self.nudge(dx, dy)
            }
            _ => false,
        }
    }

    fn text_key(&mut self, engine: &mut TextEngine, key: &str, m: Modifiers) -> bool {
        if key == "Escape" {
            self.end_text_edit(engine);
            return true;
        }
        let edit = self.text.as_mut().expect("text edit active");
        let r = &mut edit.rich;
        let mv = |r: &mut RichEdit, mv: Move| r.move_caret(mv, m.shift);
        match key {
            "ArrowLeft" => mv(r, if m.meta { Move::LineStart } else if m.alt { Move::WordLeft } else { Move::Left }),
            "ArrowRight" => mv(r, if m.meta { Move::LineEnd } else if m.alt { Move::WordRight } else { Move::Right }),
            "ArrowUp" => mv(r, if m.meta { Move::TextStart } else { Move::Up }),
            "ArrowDown" => mv(r, if m.meta { Move::TextEnd } else { Move::Down }),
            "Home" => mv(r, Move::LineStart),
            "End" => mv(r, Move::LineEnd),
            "Backspace" if m.meta => r.delete_to_line_start(engine),
            "Backspace" => r.backspace(engine, m.alt),
            "Delete" => r.delete_forward(engine, m.alt),
            // Shift+Enter is a line break inside the paragraph.
            "Enter" => r.insert(engine, if m.shift { "\u{2028}" } else { "\n" }),
            "Tab" if edit.cell.is_some() => return self.next_cell(engine, !m.shift),
            "Tab" => {
                // Tab at a paragraph start changes the list level, like PowerPoint.
                let at_start = r.is_collapsed() && r.body.paragraph_ranges()[r.body.paragraph_at(r.range().start)].start == r.range().start;
                if at_start || !r.is_collapsed() {
                    return self.change_level(engine, if m.shift { -1 } else { 1 });
                }
                r.insert(engine, "\t");
            }
            "a" if m.meta => r.select_all(),
            _ => return false,
        }
        self.fit_edited_shape(engine);
        true
    }

    /// Tab / Shift+Tab: selects the next or previous shape in drawing order.
    /// Groups count as one shape unless the group is entered.
    fn cycle_selection(&mut self, forward: bool) -> bool {
        let mut units: Vec<Vec<ShapeId>> = Vec::new();
        for s in self.current_shapes().iter().filter(|s| selectable(s)) {
            if let Some(g) = self.entered_group
                && s.group != Some(g)
            {
                continue;
            }
            let target = self.click_target(s.id);
            if !units.iter().any(|u| u.contains(&s.id)) {
                units.push(target);
            }
        }
        if units.is_empty() {
            return false;
        }
        let current = self.selection.last().and_then(|id| units.iter().position(|u| u.contains(id)));
        let n = units.len();
        let next = match current {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        self.selection = units.swap_remove(next);
        true
    }

    // --- Shape commands ----------------------------------------------------

    pub fn delete_selected(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let selection = std::mem::take(&mut self.selection);
        let Some(slide) = self.pres.slide_mut(self.slide) else { return false };
        slide.shapes.retain(|s| !selection.contains(&s.id));
        self.prune_animations();
        self.push_undo(before);
        true
    }

    pub fn nudge(&mut self, dx: i64, dy: i64) -> bool {
        if self.selection_locked() {
            return false;
        }
        self.modify_selected(|s| {
            s.xfrm.x += dx;
            s.xfrm.y += dy;
        })
    }

    /// Copies the selected shapes to the internal clipboard. Returns how many were copied.
    pub fn copy(&mut self) -> usize {
        let mut copies: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        group::detach_partial_groups(&mut copies, self.current_shapes());
        self.clipboard = copies;
        self.clipboard_slide = Some(self.slide);
        self.paste_count = 0;
        self.clipboard.len()
    }

    pub fn cut(&mut self) -> bool {
        self.copy() > 0 && self.delete_selected()
    }

    pub fn has_clipboard(&self) -> bool {
        !self.clipboard.is_empty()
    }

    /// Pastes the internal clipboard onto the current slide and selects the copies.
    pub fn paste(&mut self, engine: &mut TextEngine) -> bool {
        if self.clipboard.is_empty() || self.pres.slides.is_empty() {
            return false;
        }
        self.end_text_edit(engine);
        if self.clipboard_slide == Some(self.slide) {
            self.paste_count += 1;
        } else {
            self.paste_count = 0;
            self.clipboard_slide = Some(self.slide);
        }
        let offset = PASTE_OFFSET * self.paste_count;
        let shapes = self.clipboard.clone();
        self.insert_copies(shapes, offset)
    }

    pub fn duplicate_selected(&mut self) -> bool {
        let mut shapes: Vec<Shape> = self.selected_shapes().into_iter().cloned().collect();
        group::detach_partial_groups(&mut shapes, self.current_shapes());
        self.insert_copies(shapes, PASTE_OFFSET)
    }

    fn insert_copies(&mut self, shapes: Vec<Shape>, offset: i64) -> bool {
        if shapes.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let mut ids = Vec::new();
        // Copies form groups of their own.
        let mut groups: HashMap<ShapeId, ShapeId> = HashMap::new();
        for mut shape in shapes {
            shape.id = self.pres.alloc_id();
            if let Some(g) = shape.group {
                shape.group = Some(*groups.entry(g).or_insert_with(|| self.pres.alloc_id()));
            }
            shape.xfrm.x += offset;
            shape.xfrm.y += offset;
            ids.push(shape.id);
            self.pres.slide_mut(self.slide).unwrap().shapes.push(shape);
        }
        self.selection = ids;
        self.push_undo(before);
        true
    }

    /// Pastes plain text as a new text box (when the clipboard holds no shapes of ours).
    pub fn paste_text_box(&mut self, engine: &mut TextEngine, text: &str) -> bool {
        if !self.add_text_box(engine) {
            return false;
        }
        self.insert_text(engine, text);
        self.end_text_edit(engine);
        true
    }

    pub fn reorder(&mut self, order: ZOrder) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let selection = self.selection.clone();
        let entered = self.entered_group;
        let shapes = &mut self.pres.slide_mut(self.slide).unwrap().shapes;
        *shapes = reordered(std::mem::take(shapes), &selection, entered, order);
        // The snapshot already holds the original order.
        if *shapes == before.pres.slides[self.slide].shapes {
            return false;
        }
        self.push_undo(before);
        true
    }

    /// Aligns the selection: to each other when several shapes are selected, else to the slide.
    pub fn align(&mut self, to: AlignTo) -> bool {
        if self.selection_locked() {
            return false;
        }
        let shapes = self.selected_shapes();
        if shapes.is_empty() {
            return false;
        }
        let frame = if shapes.len() == 1 {
            (0.0, 0.0, self.pres.slide_width as f64, self.pres.slide_height as f64)
        } else {
            shapes.iter().map(|s| bounds(&s.xfrm)).fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
        };
        self.modify_selected(|s| {
            let b = bounds(&s.xfrm);
            let (dx, dy) = match to {
                AlignTo::Left => (frame.0 - b.0, 0.0),
                AlignTo::Center => ((frame.0 + frame.2) / 2.0 - (b.0 + b.2) / 2.0, 0.0),
                AlignTo::Right => (frame.2 - b.2, 0.0),
                AlignTo::Top => (0.0, frame.1 - b.1),
                AlignTo::Middle => (0.0, (frame.1 + frame.3) / 2.0 - (b.1 + b.3) / 2.0),
                AlignTo::Bottom => (0.0, frame.3 - b.3),
            };
            s.xfrm.x += dx.round() as i64;
            s.xfrm.y += dy.round() as i64;
        })
    }

    /// Inserts an empty text box in the middle of the slide and starts editing it.
    pub fn add_text_box(&mut self, engine: &mut TextEngine) -> bool {
        self.end_text_edit(engine);
        if self.pres.slides.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let (w, h) = (4 * EMU_PER_INCH as i64, (0.6 * EMU_PER_INCH) as i64);
        let xfrm = Xfrm::new((self.pres.slide_width - w) / 2, (self.pres.slide_height - h) / 2, w, h);
        let id = self.pres.alloc_id();
        let mut shape = Shape::new(id, format!("{} {}", ppt_model::lang().pick("文字方塊", "文本框", "TextBox"), id.0), xfrm);
        let mut body = TextBody { auto_grow: true, defaults: self.pres.text_defaults.clone(), ..Default::default() };
        body.paragraphs[0].end_style = self.pres.default_run_style();
        shape.text = Some(body);
        self.pres.slide_mut(self.slide).unwrap().shapes.push(shape);
        self.push_undo(before);
        self.begin_text_edit(engine, id, true);
        self.fit_edited_shape(engine);
        true
    }

    /// Inserts a preset shape with the default accent style and selects it.
    pub fn add_shape(&mut self, engine: &mut TextEngine, preset: &str) -> bool {
        self.end_text_edit(engine);
        if self.pres.slides.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let (w, h) = (2 * EMU_PER_INCH as i64, (1.5 * EMU_PER_INCH) as i64);
        let xfrm = Xfrm::new((self.pres.slide_width - w) / 2, (self.pres.slide_height - h) / 2, w, h);
        let id = self.pres.alloc_id();
        let mut shape = Shape::new(id, format!("{preset} {}", id.0), xfrm);
        shape.geometry = Geometry::preset(preset);
        shape.fill = Some(Paint::Solid(Color::rgb(0x44, 0x72, 0xC4)));
        shape.line = Some(Line::new(Color::rgb(0x2F, 0x52, 0x8F), 12_700));
        self.pres.slide_mut(self.slide).unwrap().shapes.push(shape);
        self.selection = vec![id];
        self.push_undo(before);
        true
    }

    /// Inserts a picture centered on the slide, scaled to fit 60% of it.
    pub fn add_picture(&mut self, engine: &mut TextEngine, key: String, bytes: std::sync::Arc<Vec<u8>>, px_w: u32, px_h: u32) -> bool {
        self.end_text_edit(engine);
        if self.pres.slides.is_empty() || px_w == 0 || px_h == 0 {
            return false;
        }
        let before = self.snapshot();
        let (sw, sh) = (self.pres.slide_width as f64, self.pres.slide_height as f64);
        // 96 dpi, capped to 60% of the slide.
        let (mut w, mut h) = (px_w as f64 * EMU_PER_INCH / 96.0, px_h as f64 * EMU_PER_INCH / 96.0);
        let scale = (sw * 0.6 / w).min(sh * 0.6 / h).min(1.0);
        w *= scale;
        h *= scale;
        let xfrm = Xfrm::new(((sw - w) / 2.0) as i64, ((sh - h) / 2.0) as i64, w as i64, h as i64);
        self.pres.media.insert(key.clone(), bytes);
        let id = self.pres.alloc_id();
        let mut shape = Shape::new(id, format!("{} {}", ppt_model::lang().pick("圖片", "图片", "Picture"), id.0), xfrm);
        shape.kind = ShapeKind::Picture { media: Some(key), crop: Default::default() };
        // A new picture keeps its proportions when resized, as in PowerPoint.
        shape.access.lock_aspect = true;
        self.pres.slide_mut(self.slide).unwrap().shapes.push(shape);
        self.selection = vec![id];
        self.push_undo(before);
        true
    }

    // --- Slide commands ----------------------------------------------------

    /// Adds a slide after the current one. `layout` defaults to the current slide's layout.
    pub fn add_slide(&mut self, engine: &mut TextEngine, layout: Option<usize>) -> bool {
        self.end_text_edit(engine);
        let before = self.snapshot();
        let layout = layout
            .or_else(|| self.pres.slides.get(self.slide).and_then(|s| s.layout))
            .unwrap_or(if self.pres.layouts.len() > 1 { 1 } else { 0 });
        let slide = self.pres.slide_from_layout(layout);
        let at = if self.pres.slides.is_empty() { 0 } else { self.slide + 1 };
        self.pres.slides.insert(at, std::sync::Arc::new(slide));
        self.slide = at;
        self.selection.clear();
        self.push_undo(before);
        true
    }

    pub fn duplicate_slide(&mut self, engine: &mut TextEngine, index: usize) -> bool {
        self.end_text_edit(engine);
        let Some(src) = self.pres.slides.get(index).map(|s| Slide::clone(s)) else { return false };
        let before = self.snapshot();
        let copy = self.fresh_copy(src);
        self.pres.slides.insert(index + 1, std::sync::Arc::new(copy));
        self.slide = index + 1;
        self.selection.clear();
        self.push_undo(before);
        true
    }

    /// A copy of `src` whose shapes have ids of their own.
    fn fresh_copy(&mut self, src: Slide) -> Slide {
        let mut copy = Slide { background_shapes: Vec::new(), shapes: Vec::new(), ..src.clone() };
        for s in src.background_shapes {
            copy.background_shapes.push(Shape { id: self.pres.alloc_id(), ..s });
        }
        let mut ids: HashMap<ShapeId, ShapeId> = HashMap::new();
        for s in src.shapes {
            let group = s.group.map(|g| *ids.entry(g).or_insert_with(|| self.pres.alloc_id()));
            let id = self.pres.alloc_id();
            ids.insert(s.id, id);
            copy.shapes.push(Shape { id, group, ..s });
        }
        // The copy's animations play on its own shapes.
        if let Some(t) = &mut copy.timing {
            std::sync::Arc::make_mut(t).remap(&ids);
        }
        copy
    }

    /// Takes slide `index` for the slide clipboard.
    pub fn copy_slide(&self, index: usize) -> Option<SlideClip> {
        let slide = Slide::clone(self.pres.slides.get(index)?);
        let layout_name = slide.layout.and_then(|l| self.pres.layouts.get(l)).map(|l| l.name.clone());
        let mut keys: Vec<&String> = slide
            .shapes
            .iter()
            .filter_map(|s| match &s.kind {
                ShapeKind::Picture { media: Some(k), .. } => Some(k),
                _ => None,
            })
            .collect();
        if let Some(ppt_model::Background::Picture { media }) = &slide.background {
            keys.push(media);
        }
        let mut media: Vec<(String, std::sync::Arc<Vec<u8>>)> = Vec::new();
        for k in keys {
            if !media.iter().any(|(m, _)| m == k)
                && let Some(bytes) = self.pres.media.get(k)
            {
                media.push((k.clone(), bytes));
            }
        }
        Some(SlideClip { slide, layout_name, media })
    }

    /// Pastes the clipboard's slide after slide `after` (at the end when `None`) and goes to it.
    /// From the same deck the slide is copied as is. From another one it takes the layout of the
    /// same name (else the default one), brings its pictures along, and is written from the
    /// editor's own model; charts, SmartArt and other objects only their own package can describe
    /// are left out. Returns how many objects were left out.
    pub fn paste_slide(&mut self, engine: &mut TextEngine, clip: &SlideClip, after: Option<usize>, same_deck: bool) -> usize {
        self.end_text_edit(engine);
        let before = self.snapshot();
        let mut slide = clip.slide.clone();
        let mut dropped = 0;
        if !same_deck {
            let layout = clip
                .layout_name
                .as_ref()
                .and_then(|n| self.pres.layouts.iter().position(|l| l.name.eq_ignore_ascii_case(n)))
                .unwrap_or(if self.pres.layouts.len() > 1 { 1 } else { 0 });
            let fresh = self.pres.slide_from_layout(layout);
            slide.layout = fresh.layout;
            slide.background_shapes = fresh.background_shapes;
            slide.source = None;
            let total = slide.shapes.len();
            slide.shapes.retain(|s| !matches!(s.kind, ShapeKind::Chart(_) | ShapeKind::Diagram { .. } | ShapeKind::Opaque { .. }));
            dropped = total - slide.shapes.len();
            if dropped > 0 {
                // The animations may point at what was left out.
                slide.timing = None;
            }
            // Pictures: keep the key when the deck has no other picture under it.
            let mut keys: HashMap<String, String> = HashMap::new();
            for (key, bytes) in &clip.media {
                // Free key, or the same picture already under it: keep the key.
                let new = if self.pres.media.get(key).is_none_or(|b| *b == **bytes) {
                    key.clone()
                } else {
                    let mut n = 1;
                    while self.pres.media.contains_key(&format!("pasted{n}/{key}")) {
                        n += 1;
                    }
                    format!("pasted{n}/{key}")
                };
                if !self.pres.media.contains_key(&new) {
                    self.pres.media.insert(new.clone(), bytes.clone());
                }
                keys.insert(key.clone(), new);
            }
            for s in &mut slide.shapes {
                s.source = None;
                if let ShapeKind::Picture { media: Some(k), .. } = &mut s.kind
                    && let Some(n) = keys.get(k)
                {
                    *k = n.clone();
                }
            }
            if let Some(ppt_model::Background::Picture { media }) = &mut slide.background
                && let Some(n) = keys.get(media)
            {
                *media = n.clone();
            }
        }
        let copy = self.fresh_copy(slide);
        let at = after.map_or(self.pres.slides.len(), |i| (i + 1).min(self.pres.slides.len()));
        self.pres.slides.insert(at, std::sync::Arc::new(copy));
        self.slide = at;
        self.selection.clear();
        self.push_undo(before);
        dropped
    }

    pub fn delete_slide(&mut self, engine: &mut TextEngine, index: usize) -> bool {
        self.end_text_edit(engine);
        if index >= self.pres.slides.len() || self.pres.slides.len() <= 1 {
            return false;
        }
        let before = self.snapshot();
        self.pres.slides.remove(index);
        self.slide = self.slide.min(self.pres.slides.len() - 1);
        self.selection.clear();
        self.push_undo(before);
        true
    }

    /// Moves slide `from` so it ends up at index `to`.
    pub fn move_slide(&mut self, engine: &mut TextEngine, from: usize, to: usize) -> bool {
        self.end_text_edit(engine);
        let n = self.pres.slides.len();
        if from >= n || to >= n || from == to {
            return false;
        }
        let before = self.snapshot();
        let slide = self.pres.slides.remove(from);
        self.pres.slides.insert(to, slide);
        self.slide = to;
        self.selection.clear();
        self.push_undo(before);
        true
    }
}

/// PowerPoint's font size steps for ⌘] / ⌘[.
fn numbering_of(p: &Paragraph) -> Option<String> {
    match p.bullet.as_ref().map(|b| &b.kind) {
        Some(BulletKind::AutoNum { scheme, .. }) => Some(scheme.clone()),
        _ => None,
    }
}

fn grow_size(size: f32, up: bool) -> f32 {
    const STEPS: [f32; 22] = [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 44.0, 48.0, 54.0, 60.0, 66.0, 72.0, 80.0];
    if up {
        STEPS.iter().copied().find(|s| *s > size + 0.01).unwrap_or(size + 8.0)
    } else {
        STEPS.iter().rev().copied().find(|s| *s < size - 0.01).unwrap_or((size - 1.0).max(1.0))
    }
}

/// Hidden content kept only for saving is never selectable.
/// The shapes with the selection moved in z-order. A group moves as one block
/// (and others step over it whole); members of the entered group move only
/// among themselves.
fn reordered(shapes: Vec<Shape>, selection: &[ShapeId], entered: Option<ShapeId>, order: ZOrder) -> Vec<Shape> {
    // Consecutive shapes of one (not entered) group form a block.
    let mut blocks: Vec<Vec<Shape>> = Vec::new();
    for s in shapes {
        let joins = s.group.is_some() && s.group != entered && blocks.last().is_some_and(|b| b[0].group == s.group);
        if joins {
            blocks.last_mut().expect("checked").push(s);
        } else {
            blocks.push(vec![s]);
        }
    }
    let selected = |b: &Vec<Shape>| b.iter().any(|s| selection.contains(&s.id));
    // Members of the entered group stay inside it.
    let scope = |b: &Vec<Shape>| if entered.is_some() && b[0].group == entered { entered } else { None };
    let step_forward = |blocks: &mut Vec<Vec<Shape>>| {
        let mut moved = false;
        for i in (0..blocks.len().saturating_sub(1)).rev() {
            if selected(&blocks[i]) && !selected(&blocks[i + 1]) && scope(&blocks[i]) == scope(&blocks[i + 1]) {
                blocks.swap(i, i + 1);
                moved = true;
            }
        }
        moved
    };
    let step_backward = |blocks: &mut Vec<Vec<Shape>>| {
        let mut moved = false;
        for i in 1..blocks.len() {
            if selected(&blocks[i]) && !selected(&blocks[i - 1]) && scope(&blocks[i]) == scope(&blocks[i - 1]) {
                blocks.swap(i, i - 1);
                moved = true;
            }
        }
        moved
    };
    match order {
        ZOrder::Forward => {
            step_forward(&mut blocks);
        }
        ZOrder::Backward => {
            step_backward(&mut blocks);
        }
        ZOrder::Front => while step_forward(&mut blocks) {},
        ZOrder::Back => while step_backward(&mut blocks) {},
    }
    blocks.into_iter().flatten().collect()
}

fn selectable(shape: &Shape) -> bool {
    !shape.access.hidden && !matches!(&shape.kind, ShapeKind::Opaque { label } if label == "hidden")
}

fn can_hold_text(shape: &Shape) -> bool {
    !shape.geometry.is_line() && matches!(shape.kind, ShapeKind::Shape)
}

fn frame_contains(frame: &Xfrm, x: f64, y: f64, tol: f64) -> bool {
    let (lx, ly) = frame.to_local(x, y);
    lx >= -tol / 2.0 && ly >= -tol / 2.0 && lx <= frame.w as f64 + tol / 2.0 && ly <= frame.h as f64 + tol / 2.0
}

fn shape_contains(shape: &Shape, x: f64, y: f64, tol: f64) -> bool {
    let xf = &shape.xfrm;
    let (lx, ly) = xf.to_local(x, y);
    let (w, h) = (xf.w as f64, xf.h as f64);
    if shape.geometry.is_line() {
        // Distance from the diagonal (0,0)-(w,h), honoring flips.
        let (x0, y0, x1, y1) = match (xf.flip_h, xf.flip_v) {
            (false, false) | (true, true) => (0.0, 0.0, w, h),
            _ => (w, 0.0, 0.0, h),
        };
        return segment_distance((lx, ly), (x0, y0), (x1, y1)) <= tol;
    }
    lx >= -tol / 2.0 && ly >= -tol / 2.0 && lx <= w + tol / 2.0 && ly <= h + tol / 2.0
}

fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

pub fn handle_points(xfrm: &Xfrm, is_line: bool, rotate_offset: f64) -> Vec<(Handle, (f64, f64))> {
    let (w, h) = (xfrm.w as f64, xfrm.h as f64);
    if is_line {
        let (a, b) = match (xfrm.flip_h, xfrm.flip_v) {
            (false, false) | (true, true) => ((0.0, 0.0), (w, h)),
            _ => ((w, 0.0), (0.0, h)),
        };
        // Line endpoints act as the corner handles of their frame.
        let ha = if a.0 == 0.0 { Handle::NW } else { Handle::NE };
        let hb = if b.0 == w { Handle::SE } else { Handle::SW };
        return vec![(ha, xfrm.to_slide(a.0, a.1)), (hb, xfrm.to_slide(b.0, b.1))];
    }
    let mut out: Vec<(Handle, (f64, f64))> = Handle::RESIZE
        .iter()
        .map(|&handle| {
            let (sx, sy) = handle.sign();
            (handle, xfrm.to_slide((sx + 1.0) / 2.0 * w, (sy + 1.0) / 2.0 * h))
        })
        .collect();
    out.push((Handle::Rotate, xfrm.to_slide(w / 2.0, -rotate_offset)));
    out
}

/// Resizes in the shape's own (rotated) frame, keeping the opposite edge fixed.
fn resize(orig: &Xfrm, handle: Handle, dx: f64, dy: f64, keep_aspect: bool, min: f64) -> Xfrm {
    let (sx, sy) = handle.sign();
    let (s, c) = (-orig.rot).to_radians().sin_cos();
    let (ldx, ldy) = (dx * c - dy * s, dx * s + dy * c);
    let (ow, oh) = (orig.w as f64, orig.h as f64);
    let mut w = (ow + sx * ldx).max(min);
    let mut h = (oh + sy * ldy).max(min);
    if keep_aspect && ow > 0.0 && oh > 0.0 {
        if sx != 0.0 && sy != 0.0 {
            let scale = (w / ow).max(h / oh);
            w = ow * scale;
            h = oh * scale;
        } else if sx != 0.0 {
            // An edge handle scales both sides; the other one grows about the center.
            h = oh * (w / ow);
        } else if sy != 0.0 {
            w = ow * (h / oh);
        }
    }
    // Shift the center so the opposite edges stay put.
    let (lcx, lcy) = (sx * (w - ow) / 2.0, sy * (h - oh) / 2.0);
    let (s, c) = orig.rot.to_radians().sin_cos();
    let (ocx, ocy) = orig.center();
    let (cx, cy) = (ocx + lcx * c - lcy * s, ocy + lcx * s + lcy * c);
    Xfrm { x: (cx - w / 2.0).round() as i64, y: (cy - h / 2.0).round() as i64, w: w.round() as i64, h: h.round() as i64, ..*orig }
}

/// Cursor over a guide: vertical guides move sideways.
fn guide_cursor(vertical: bool) -> CursorIcon {
    CursorIcon::Resize(if vertical { 0.0 } else { 90.0 })
}

fn resize_cursor(handle: Handle, xfrm: &Xfrm) -> CursorIcon {
    let (sx, sy) = handle.sign();
    let base = sy.atan2(sx).to_degrees();
    CursorIcon::Resize((base + xfrm.rot).rem_euclid(180.0))
}

/// Converts a UTF-16 offset (what the DOM reports) into a byte offset in `text`.
pub fn utf16_to_byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, ch) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += ch.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests;
