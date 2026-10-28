//! Document model for ww-ppt.
//!
//! All slide geometry is stored in EMU (English Metric Units, 914400 per inch),
//! the native unit of OOXML. Font sizes are stored in points.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

mod text;

pub type Emu = i64;

pub const EMU_PER_INCH: f64 = 914_400.0;
pub const EMU_PER_PT: f64 = 12_700.0;

/// 16:9 widescreen slide, PowerPoint's default.
pub const DEFAULT_SLIDE_WIDTH: Emu = 12_192_000;
pub const DEFAULT_SLIDE_HEIGHT: Emu = 6_858_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Parses `RRGGBB` (the OOXML `srgbClr@val` format).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(hex, 16).ok()?;
        Some(Self::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

// Parley brushes need a Default; opaque black is the sensible text default.
impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeId(pub u32);

/// Position, size and orientation of a shape (`a:xfrm`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xfrm {
    pub x: Emu,
    pub y: Emu,
    pub w: Emu,
    pub h: Emu,
    /// Clockwise rotation in degrees.
    pub rot: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Xfrm {
    pub fn new(x: Emu, y: Emu, w: Emu, h: Emu) -> Self {
        Self { x, y, w, h, ..Default::default() }
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x as f64 + self.w as f64 / 2.0, self.y as f64 + self.h as f64 / 2.0)
    }

    /// Maps a slide-space point into the shape's unrotated local frame,
    /// where (0, 0) is the top-left corner and (w, h) the bottom-right.
    pub fn to_local(&self, px: f64, py: f64) -> (f64, f64) {
        let (cx, cy) = self.center();
        let (s, c) = (-self.rot).to_radians().sin_cos();
        let (dx, dy) = (px - cx, py - cy);
        let lx = dx * c - dy * s;
        let ly = dx * s + dy * c;
        (lx + self.w as f64 / 2.0, ly + self.h as f64 / 2.0)
    }

    /// Inverse of [`Xfrm::to_local`].
    pub fn to_slide(&self, lx: f64, ly: f64) -> (f64, f64) {
        let (cx, cy) = self.center();
        let (s, c) = self.rot.to_radians().sin_cos();
        let (dx, dy) = (lx - self.w as f64 / 2.0, ly - self.h as f64 / 2.0);
        (cx + dx * c - dy * s, cy + dx * s + dy * c)
    }
}

/// A single path segment of custom geometry, in path-local units.
#[derive(Clone, Debug, PartialEq)]
pub enum PathCmd {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    QuadTo(f64, f64, f64, f64),
    CubicTo(f64, f64, f64, f64, f64, f64),
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CustomPath {
    /// Path coordinate space; scaled to the shape size when drawn.
    pub w: f64,
    pub h: f64,
    pub cmds: Vec<PathCmd>,
    pub fill: bool,
    pub stroke: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    /// `a:prstGeom`, e.g. `rect`, `roundRect`, `ellipse`, with adjust values (`a:gd`).
    Preset { name: String, adjust: Vec<(String, f64)> },
    /// `a:custGeom`.
    Custom(Vec<CustomPath>),
}

impl Geometry {
    pub fn preset(name: &str) -> Self {
        Geometry::Preset { name: name.to_string(), adjust: Vec::new() }
    }

    pub fn is_line(&self) -> bool {
        matches!(self, Geometry::Preset { name, .. }
            if name == "line" || name.starts_with("straightConnector") || name.starts_with("bentConnector") || name.starts_with("curvedConnector"))
    }
}

impl Default for Geometry {
    fn default() -> Self {
        Geometry::preset("rect")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub color: Color,
    pub width: Emu,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunStyle {
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Color,
    /// Resolved latin typeface (theme references like `+mn-lt` are already expanded).
    pub latin_font: String,
    /// Resolved East Asian typeface; may be empty.
    pub ea_font: String,
}

impl Default for RunStyle {
    fn default() -> Self {
        Self {
            size_pt: 18.0,
            bold: false,
            italic: false,
            underline: false,
            color: Color::BLACK,
            latin_font: "Calibri".to_string(),
            ea_font: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: RunStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    pub align: Align,
    pub level: u8,
    pub bullet: Option<char>,
    /// Line spacing as a multiple of single spacing (`a:lnSpc/a:spcPct`).
    pub line_spacing: f32,
    /// Left margin of every line (`marL`).
    pub mar_l: Emu,
    /// First-line offset relative to `mar_l` (`indent`); negative for hanging bullets.
    pub indent: Emu,
    /// Space before/after the paragraph, in points.
    pub space_before: f32,
    pub space_after: f32,
    /// Style used when the paragraph has no runs (`a:endParaRPr`).
    pub end_style: RunStyle,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            align: Align::Left,
            level: 0,
            bullet: None,
            line_spacing: 1.0,
            mar_l: 0,
            indent: 0,
            space_before: 0.0,
            space_after: 0.0,
            end_style: RunStyle::default(),
        }
    }
}

impl Paragraph {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Style of the first run, or the end-of-paragraph style for empty paragraphs.
    pub fn first_style(&self) -> &RunStyle {
        self.runs.first().map(|r| &r.style).unwrap_or(&self.end_style)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBody {
    pub paragraphs: Vec<Paragraph>,
    pub anchor: Anchor,
    /// Left, top, right, bottom insets.
    pub insets: [Emu; 4],
    pub wrap: bool,
    /// `a:normAutofit@fontScale`, 1.0 when absent.
    pub font_scale: f32,
    /// `a:normAutofit@lnSpcReduction`, 0.0 when absent.
    pub line_spacing_reduction: f32,
    /// `a:spAutoFit`: the shape's height follows its text.
    pub auto_grow: bool,
    /// Inherited formatting per list level (from layout, master and theme).
    /// The writer only saves properties that differ from these.
    pub defaults: Option<Arc<Vec<ParaDefaults>>>,
}

/// What a paragraph at one list level looks like with no direct formatting.
#[derive(Clone, Debug, PartialEq)]
pub struct ParaDefaults {
    pub style: RunStyle,
    pub align: Align,
    pub bullet: Option<char>,
    pub line_spacing: f32,
    pub mar_l: Emu,
    pub indent: Emu,
    pub space_before: f32,
    pub space_after: f32,
}

impl TextBody {
    /// Defaults for list level `level`, if known.
    pub fn defaults_for(&self, level: u8) -> Option<&ParaDefaults> {
        self.defaults.as_ref().and_then(|d| d.get(level as usize).or(d.first()))
    }
}

impl Default for TextBody {
    fn default() -> Self {
        Self {
            paragraphs: vec![Paragraph::default()],
            anchor: Anchor::Top,
            insets: [91_440, 45_720, 91_440, 45_720],
            wrap: true,
            font_scale: 1.0,
            line_spacing_reduction: 0.0,
            auto_grow: false,
            defaults: None,
        }
    }
}

impl TextBody {
    /// Paragraphs joined with `\n`, the representation used while editing.
    pub fn plain_text(&self) -> String {
        self.paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>().join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.paragraphs.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty()))
    }

    /// Replaces the text, giving each paragraph a single run.
    ///
    /// Paragraph `i` keeps the formatting of the old paragraph `i` (or of the last
    /// old paragraph for newly added ones), so plain-text editing does not lose
    /// alignment, bullets or font settings.
    pub fn set_plain_text(&mut self, text: &str) {
        let old = std::mem::take(&mut self.paragraphs);
        let fallback = old.last().cloned().unwrap_or_default();
        self.paragraphs = text
            .split('\n')
            .enumerate()
            .map(|(i, line)| {
                let template = old.get(i).unwrap_or(&fallback);
                let style = template.first_style().clone();
                Paragraph {
                    runs: if line.is_empty() {
                        Vec::new()
                    } else {
                        vec![Run { text: line.to_string(), style: style.clone() }]
                    },
                    end_style: style,
                    ..template.clone()
                }
            })
            .collect();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placeholder {
    /// `p:ph@type`: `title`, `ctrTitle`, `subTitle`, `body`, `dt`, `ftr`, `sldNum`, ...
    pub kind: String,
    pub idx: u32,
}

impl Placeholder {
    /// Hint shown in an empty placeholder while editing (never printed).
    pub fn prompt(&self) -> Option<&'static str> {
        match self.kind.as_str() {
            "title" | "ctrTitle" => Some("按一下以新增標題"),
            "subTitle" => Some("按一下以新增副標題"),
            "body" | "obj" => Some("按一下以新增文字"),
            _ => None,
        }
    }
}

/// Picture cropping (`a:srcRect`): fractions of the image trimmed from each
/// edge. Negative values pad the image instead.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Crop {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Crop {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ShapeKind {
    Shape,
    /// `p:pic`; `media` is a key into [`Presentation::media`].
    Picture { media: Option<String>, crop: Crop },
    /// Content the editor cannot edit yet (tables, charts, SmartArt, ...).
    /// Kept verbatim on save; drawn as a labelled frame.
    Opaque { label: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub id: ShapeId,
    pub name: String,
    pub kind: ShapeKind,
    pub xfrm: Xfrm,
    pub geometry: Geometry,
    pub fill: Option<Color>,
    pub line: Option<Line>,
    pub text: Option<TextBody>,
    pub placeholder: Option<Placeholder>,
    /// Where the shape came from in the opened file, for lossless saving.
    pub source: Option<Arc<ShapeSource>>,
}

/// The original XML of a shape and its values as parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeSource {
    /// Part the XML came from, e.g. `ppt/slides/slide3.xml`.
    pub part: String,
    pub doc: Arc<str>,
    /// Byte range of the shape element in `doc`.
    pub range: Range<usize>,
    /// Namespace declarations in scope at the part's root element.
    pub namespaces: Arc<Vec<(String, String)>>,
    /// Relationship ids the shape refers to (`r:embed`, `r:id`, ...) → (type, target).
    pub rels: Vec<(String, String, String)>,
    /// Enclosing `p:grpSp` / `mc:AlternateContent` the shape was flattened out of.
    pub container: Option<Arc<ContainerSource>>,
    pub parsed: ParsedShape,
}

/// A group (or alternate-content block) whose children were flattened.
#[derive(Debug, PartialEq)]
pub struct ContainerSource {
    pub range: Range<usize>,
    /// Number of drawable children flattened out of it.
    pub children: usize,
    /// Distinguishes containers with equal ranges in different documents.
    pub part: String,
}

/// Snapshot of the editable fields at load time.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedShape {
    pub xfrm: Xfrm,
    pub fill: Option<Color>,
    pub line: Option<Line>,
    pub text: Option<TextBody>,
}

impl ShapeSource {
    pub fn xml(&self) -> &str {
        &self.doc[self.range.clone()]
    }
}

impl Shape {
    pub fn new(id: ShapeId, name: impl Into<String>, xfrm: Xfrm) -> Self {
        Self {
            id,
            name: name.into(),
            kind: ShapeKind::Shape,
            xfrm,
            geometry: Geometry::default(),
            fill: None,
            line: None,
            text: None,
            placeholder: None,
            source: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slide {
    /// Index into [`Presentation::layouts`].
    pub layout: Option<usize>,
    /// Non-placeholder shapes inherited from the layout and master. Drawn
    /// behind the slide content and never selectable.
    pub background_shapes: Vec<Shape>,
    pub shapes: Vec<Shape>,
    pub background: Option<Color>,
    pub source: Option<Arc<SlideSource>>,
}

/// The original part of a slide.
#[derive(Debug, PartialEq)]
pub struct SlideSource {
    pub part: String,
    pub doc: Arc<str>,
    pub namespaces: Arc<Vec<(String, String)>>,
    /// Shape ids as loaded, in order; equal to the current ids when the slide's shape list is untouched.
    pub shape_ids: Vec<ShapeId>,
}

/// A slide layout: what a new slide based on it starts with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub name: String,
    /// Empty placeholder shapes with fully resolved position and text styles.
    pub placeholders: Vec<Shape>,
    /// Layout and master decorations.
    pub background_shapes: Vec<Shape>,
    pub background: Option<Color>,
    /// Package part of the layout, e.g. `ppt/slideLayouts/slideLayout2.xml`.
    pub part: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Presentation {
    pub slide_width: Emu,
    pub slide_height: Emu,
    pub slides: Vec<Slide>,
    pub layouts: Vec<Layout>,
    /// Encoded image bytes keyed by package part name.
    pub media: HashMap<String, Arc<Vec<u8>>>,
    /// Theme fonts used as defaults for new text (latin, east asian).
    pub minor_font: (String, String),
    /// Defaults for new text boxes (the presentation's default text style).
    pub text_defaults: Option<Arc<Vec<ParaDefaults>>>,
    /// The package the deck was loaded from; saving copies untouched parts from it.
    pub package: Option<Arc<Vec<u8>>>,
    next_id: u32,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            slide_width: DEFAULT_SLIDE_WIDTH,
            slide_height: DEFAULT_SLIDE_HEIGHT,
            slides: Vec::new(),
            layouts: Vec::new(),
            media: HashMap::new(),
            minor_font: ("Calibri".to_string(), String::new()),
            text_defaults: None,
            package: None,
            next_id: 1,
        }
    }
}

impl Presentation {
    /// A one-slide deck with a title text box, used when the app starts.
    pub fn new_blank() -> Self {
        let mut pres = Presentation::default();
        let mut slide = Slide { background: Some(Color::WHITE), ..Default::default() };
        let id = pres.alloc_id();
        let mut title = Shape::new(id, "標題 1", Xfrm::new(838_200, 2_130_425, 10_515_600, 1_470_025));
        let mut body = TextBody { anchor: Anchor::Middle, ..Default::default() };
        let style = RunStyle { size_pt: 44.0, ..pres.default_run_style() };
        body.paragraphs[0].align = Align::Center;
        body.paragraphs[0].end_style = style;
        body.set_plain_text("ww-ppt：按兩下即可編輯文字");
        title.text = Some(body);
        slide.shapes.push(title);
        pres.slides.push(slide);
        pres
    }

    /// A new slide based on layout `index`, with fresh shape ids.
    pub fn slide_from_layout(&mut self, index: usize) -> Slide {
        let Some(layout) = self.layouts.get(index).cloned() else {
            return Slide { background: Some(Color::WHITE), ..Default::default() };
        };
        let mut fresh = |shapes: Vec<Shape>| -> Vec<Shape> {
            shapes
                .into_iter()
                .map(|mut s| {
                    s.id = self.alloc_id();
                    s
                })
                .collect()
        };
        Slide {
            layout: Some(index),
            background_shapes: fresh(layout.background_shapes),
            shapes: fresh(layout.placeholders),
            background: layout.background,
            source: None,
        }
    }

    pub fn alloc_id(&mut self) -> ShapeId {
        let id = ShapeId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Makes sure future ids do not collide with ids loaded from a file.
    pub fn bump_next_id(&mut self, seen: ShapeId) {
        self.next_id = self.next_id.max(seen.0 + 1);
    }

    pub fn default_run_style(&self) -> RunStyle {
        RunStyle {
            latin_font: self.minor_font.0.clone(),
            ea_font: self.minor_font.1.clone(),
            ..Default::default()
        }
    }

    pub fn shape(&self, slide: usize, id: ShapeId) -> Option<&Shape> {
        self.slides.get(slide)?.shapes.iter().find(|s| s.id == id)
    }

    pub fn shape_mut(&mut self, slide: usize, id: ShapeId) -> Option<&mut Shape> {
        self.slides.get_mut(slide)?.shapes.iter_mut().find(|s| s.id == id)
    }

    /// Every font family referenced by text on slides and layouts.
    pub fn font_families(&self) -> Vec<String> {
        let mut out = vec![self.minor_font.0.clone(), self.minor_font.1.clone()];
        let slides = self.slides.iter().flat_map(|s| s.background_shapes.iter().chain(&s.shapes));
        let layouts = self.layouts.iter().flat_map(|l| l.background_shapes.iter().chain(&l.placeholders));
        for shape in slides.chain(layouts) {
            let Some(text) = &shape.text else { continue };
            for p in &text.paragraphs {
                for style in p.runs.iter().map(|r| &r.style).chain([&p.end_style]) {
                    out.push(style.latin_font.clone());
                    out.push(style.ea_font.clone());
                }
            }
        }
        out.retain(|f| !f.is_empty());
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_round_trip_with_rotation() {
        let x = Xfrm { rot: 30.0, ..Xfrm::new(100, 200, 400, 300) };
        let (lx, ly) = x.to_local(350.0, 420.0);
        let (sx, sy) = x.to_slide(lx, ly);
        assert!((sx - 350.0).abs() < 1e-6 && (sy - 420.0).abs() < 1e-6);
        // Center maps to the middle of the local frame regardless of rotation.
        let (cx, cy) = x.center();
        let (lx, ly) = x.to_local(cx, cy);
        assert!((lx - 200.0).abs() < 1e-6 && (ly - 150.0).abs() < 1e-6);
    }

    #[test]
    fn set_plain_text_keeps_paragraph_formatting() {
        let mut body = TextBody::default();
        body.paragraphs[0].align = Align::Center;
        body.paragraphs[0].end_style.size_pt = 40.0;
        body.set_plain_text("第一行\n\nthird");
        assert_eq!(body.paragraphs.len(), 3);
        assert!(body.paragraphs.iter().all(|p| p.align == Align::Center));
        assert!(body.paragraphs[1].runs.is_empty());
        assert_eq!(body.paragraphs[2].runs[0].style.size_pt, 40.0);
        assert_eq!(body.plain_text(), "第一行\n\nthird");
    }
}
