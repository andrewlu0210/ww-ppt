//! Document model for ww-ppt.
//!
//! All slide geometry is stored in EMU (English Metric Units, 914400 per inch),
//! the native unit of OOXML. Font sizes are stored in points.

use std::ops::Range;
use std::sync::Arc;

pub use bullet::{Bullet, BulletKind};
pub use table::Table;
pub use transition::{Transition, TransitionKind};

pub mod anim;
pub mod bullet;
pub mod chart;
pub mod diagram;
pub mod media;
pub mod table;
pub mod transition;
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

/// How an area is painted.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Color),
    Gradient(Arc<Gradient>),
}

impl Paint {
    /// A single representative color (the first gradient stop), for toolbars
    /// and places that cannot draw gradients.
    pub fn color(&self) -> Color {
        match self {
            Paint::Solid(c) => *c,
            Paint::Gradient(g) => g.stops.first().map(|s| s.1).unwrap_or(Color::WHITE),
        }
    }
}

/// `a:gradFill`.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// (position 0..=1, color), sorted by position.
    pub stops: Vec<(f32, Color)>,
    pub kind: GradientKind,
    /// The gradient turns with the shape (`rotWithShape`).
    pub rotate_with_shape: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    /// `a:lin`: direction in degrees clockwise from the x axis. With `scaled`
    /// the angle is applied to a unit square and stretched with the shape.
    Linear { angle: f64, scaled: bool },
    /// `a:path`: radiates from the focus rectangle (`fillToRect`, fractions
    /// left/top/right/bottom) outwards; `circle` vs. `rect`/`shape` paths.
    Path { circle: bool, focus: [f64; 4] },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub color: Color,
    pub width: Emu,
    pub dash: Dash,
    /// Decorations at the start (`a:headEnd`) and end (`a:tailEnd`) of the path.
    pub head: LineEnd,
    pub tail: LineEnd,
}

impl Line {
    /// A solid line without arrowheads.
    pub fn new(color: Color, width: Emu) -> Self {
        Line { color, width, dash: Dash::Solid, head: LineEnd::NONE, tail: LineEnd::NONE }
    }
}

/// `a:prstDash` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dash {
    #[default]
    Solid,
    Dot,
    Dash,
    LgDash,
    DashDot,
    LgDashDot,
    LgDashDotDot,
    SysDash,
    SysDot,
    SysDashDot,
    SysDashDotDot,
}

impl Dash {
    pub fn parse(s: &str) -> Dash {
        match s {
            "dot" => Dash::Dot,
            "dash" => Dash::Dash,
            "lgDash" => Dash::LgDash,
            "dashDot" => Dash::DashDot,
            "lgDashDot" => Dash::LgDashDot,
            "lgDashDotDot" => Dash::LgDashDotDot,
            "sysDash" => Dash::SysDash,
            "sysDot" => Dash::SysDot,
            "sysDashDot" => Dash::SysDashDot,
            "sysDashDotDot" => Dash::SysDashDotDot,
            _ => Dash::Solid,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dash::Solid => "solid",
            Dash::Dot => "dot",
            Dash::Dash => "dash",
            Dash::LgDash => "lgDash",
            Dash::DashDot => "dashDot",
            Dash::LgDashDot => "lgDashDot",
            Dash::LgDashDotDot => "lgDashDotDot",
            Dash::SysDash => "sysDash",
            Dash::SysDot => "sysDot",
            Dash::SysDashDot => "sysDashDot",
            Dash::SysDashDotDot => "sysDashDotDot",
        }
    }

    /// Dash and gap lengths in multiples of the line width (ECMA-376 20.1.10.48).
    pub fn pattern(self) -> &'static [f64] {
        match self {
            Dash::Solid => &[],
            Dash::Dot => &[1.0, 3.0],
            Dash::Dash => &[4.0, 3.0],
            Dash::LgDash => &[8.0, 3.0],
            Dash::DashDot => &[4.0, 3.0, 1.0, 3.0],
            Dash::LgDashDot => &[8.0, 3.0, 1.0, 3.0],
            Dash::LgDashDotDot => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
            Dash::SysDash => &[3.0, 1.0],
            Dash::SysDot => &[1.0, 1.0],
            Dash::SysDashDot => &[3.0, 1.0, 1.0, 1.0],
            Dash::SysDashDotDot => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// An arrowhead or other line-end decoration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEnd {
    pub kind: EndKind,
    /// Width and length: 0 small, 1 medium, 2 large.
    pub width: u8,
    pub length: u8,
}

impl LineEnd {
    pub const NONE: LineEnd = LineEnd { kind: EndKind::None, width: 1, length: 1 };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndKind {
    None,
    Triangle,
    Stealth,
    Diamond,
    Oval,
    Arrow,
}

impl EndKind {
    pub fn parse(s: &str) -> EndKind {
        match s {
            "triangle" => EndKind::Triangle,
            "stealth" => EndKind::Stealth,
            "diamond" => EndKind::Diamond,
            "oval" => EndKind::Oval,
            "arrow" => EndKind::Arrow,
            _ => EndKind::None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EndKind::None => "none",
            EndKind::Triangle => "triangle",
            EndKind::Stealth => "stealth",
            EndKind::Diamond => "diamond",
            EndKind::Oval => "oval",
            EndKind::Arrow => "arrow",
        }
    }
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
    pub strike: bool,
    /// Baseline shift as a fraction of the font size: positive raises
    /// (superscript, `0.3`), negative lowers (subscript, `-0.25`).
    pub baseline: f32,
    /// Extra space between characters, in points (`spc`).
    pub spacing_pt: f32,
    pub color: Color,
    /// Resolved latin typeface (theme references like `+mn-lt` are already expanded).
    pub latin_font: String,
    /// Resolved East Asian typeface; may be empty.
    pub ea_font: String,
    /// Hyperlink on the run (`a:hlinkClick`).
    pub link: Option<Arc<Link>>,
    /// The run is a field (`a:fld`): its text is a cached value PowerPoint
    /// recomputes, like the slide number.
    pub field: Option<Arc<Field>>,
}

/// A text field (`a:fld`).
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// `slidenum`, `datetime1`, ... (`type`).
    pub kind: String,
    /// The field's GUID (`id`), `{...}`.
    pub id: String,
}

impl Field {
    pub const SLIDE_NUMBER: &str = "slidenum";

    pub fn new(kind: &str) -> Self {
        // PowerPoint wants a GUID; any unique one will do.
        // (No clock in WebAssembly: a counter through two rounds of splitmix64.)
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x05EE_D0FF_1E1D);
        let mix = |mut z: u64| {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (a, b) = (mix(n), mix(n ^ 0xA5A5_A5A5_A5A5_A5A5));
        Field { kind: kind.to_string(), id: format!("{{{:08X}-{:04X}-4{:03X}-8{:03X}-{:012X}}}", (a >> 32) as u32, (a >> 16) as u16, a as u16 & 0xFFF, (b >> 52) as u16 & 0xFFF, b & 0xFFFF_FFFF_FFFF) }
    }
}

/// A hyperlink: a relationship of the slide plus how PowerPoint shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// Relationship type (hyperlink, slide, ...).
    pub kind: String,
    /// URL for external links, part name for internal ones; empty for pure actions.
    pub target: String,
    pub external: bool,
    /// `action`, e.g. `ppaction://hlinksldjump`.
    pub action: Option<String>,
    pub tooltip: Option<String>,
    /// The theme's hyperlink color, which linked text is drawn in.
    pub color: Option<Color>,
}

impl Link {
    pub const HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

    /// A link to a web address.
    pub fn url(url: &str, color: Option<Color>) -> Self {
        Link { kind: Self::HYPERLINK.to_string(), target: url.to_string(), external: true, action: None, tooltip: None, color }
    }
}

impl RunStyle {
    /// Moves every property still equal to `old` (an inherited default) to `new`'s value.
    pub fn rebase(&mut self, old: &RunStyle, new: &RunStyle) {
        macro_rules! take {
            ($($f:ident),*) => { $( if self.$f == old.$f { self.$f = new.$f.clone(); } )* };
        }
        take!(size_pt, bold, italic, underline, strike, baseline, spacing_pt, color, latin_font, ea_font);
    }
}

impl Default for RunStyle {
    fn default() -> Self {
        Self {
            size_pt: 18.0,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            baseline: 0.0,
            spacing_pt: 0.0,
            color: Color::BLACK,
            latin_font: "Calibri".to_string(),
            ea_font: String::new(),
            link: None,
            field: None,
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
    pub bullet: Option<Bullet>,
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
    pub bullet: Option<Bullet>,
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

/// An outer shadow: a blurred copy of the shape offset by `dist` toward `dir`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    /// Blur radius, in EMU.
    pub blur: Emu,
    /// Offset distance, in EMU.
    pub dist: Emu,
    /// Offset direction in degrees, clockwise from the x axis.
    pub dir: f64,
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
    /// `a:tbl` in a graphic frame.
    Table(Box<Table>),
    /// A chart, drawn from the values cached in its part; saved unchanged.
    Chart(Box<chart::Chart>),
    /// SmartArt, drawn from the pre-rendered drawing PowerPoint stores with it.
    /// `shapes` are relative to a `w` × `h` frame at the origin. `data` is the
    /// editable node tree, when the data model could be read.
    Diagram { shapes: Arc<Vec<Shape>>, w: Emu, h: Emu, data: Option<Arc<diagram::Diagram>> },
    /// Content the editor cannot edit yet (charts, SmartArt, ...).
    /// Kept verbatim on save; drawn as a labelled frame.
    Opaque { label: String },
}

/// Accessibility and visibility settings of a shape (`p:cNvPr`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Access {
    /// Alternative text read by screen readers (`descr`).
    pub alt_text: String,
    /// Purely decorative: screen readers skip it (`adec:decorative`).
    pub decorative: bool,
    /// Hidden in the editor and in shows (`hidden="1"`), shown again from the selection pane.
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub id: ShapeId,
    pub name: String,
    pub kind: ShapeKind,
    pub xfrm: Xfrm,
    pub geometry: Geometry,
    pub fill: Option<Paint>,
    pub line: Option<Line>,
    pub text: Option<TextBody>,
    pub placeholder: Option<Placeholder>,
    /// The (outermost) group the shape belongs to, identified by an id of its
    /// own. Group members are kept contiguous in z-order.
    pub group: Option<ShapeId>,
    /// Drop shadow (`a:outerShdw`); drawn only, saved with the shape's XML.
    pub shadow: Option<Shadow>,
    /// Where the shape came from in the opened file, for lossless saving.
    pub access: Access,
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
    pub fill: Option<Paint>,
    pub line: Option<Line>,
    pub text: Option<TextBody>,
    pub kind: ShapeKind,
    pub group: Option<ShapeId>,
    pub placeholder: Option<Placeholder>,
    pub geometry: Geometry,
    pub shadow: Option<Shadow>,
    pub name: String,
    pub access: Access,
}

impl ShapeSource {
    pub fn xml(&self) -> &str {
        &self.doc[self.range.clone()]
    }
}

/// Color emoji font; large, so it is only loaded for decks that use emoji.
pub const EMOJI_FAMILY: &str = "Apple Color Emoji";

/// Characters that need the emoji font (pictographs, symbols, flags).
pub fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B50 | 0x2B55)
}

impl Shape {
    /// The shape's own text plus, for tables, every cell's text.
    pub fn text_bodies(&self) -> Box<dyn Iterator<Item = &TextBody> + '_> {
        let cells: Box<dyn Iterator<Item = &TextBody>> = match &self.kind {
            ShapeKind::Table(t) => Box::new(t.rows.iter().flat_map(|r| r.cells.iter().map(|c| &c.text))),
            ShapeKind::Diagram { shapes, .. } => Box::new(shapes.iter().flat_map(|s| s.text.iter())),
            _ => Box::new(std::iter::empty()),
        };
        Box::new(self.text.iter().chain(cells))
    }

    /// Mutable version of [`Shape::text_bodies`].
    pub fn text_bodies_mut(&mut self) -> Box<dyn Iterator<Item = &mut TextBody> + '_> {
        let cells: Box<dyn Iterator<Item = &mut TextBody>> = match &mut self.kind {
            ShapeKind::Table(t) => Box::new(t.rows.iter_mut().flat_map(|r| r.cells.iter_mut().map(|c| &mut c.text))),
            _ => Box::new(std::iter::empty()),
        };
        Box::new(self.text.iter_mut().chain(cells))
    }

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
            group: None,
            shadow: None,
            access: Access::default(),
            source: None,
        }
    }
}

/// How a slide (or layout) background is filled.
#[derive(Clone, Debug, PartialEq)]
pub enum Background {
    Paint(Paint),
    /// A picture stretched over the slide; `media` is a key into [`Presentation::media`].
    Picture { media: String },
}

impl Background {
    pub const WHITE: Background = Background::Paint(Paint::Solid(Color::WHITE));

    /// A single representative color, for toolbars and places that cannot draw pictures.
    pub fn color(&self) -> Color {
        match self {
            Background::Paint(p) => p.color(),
            Background::Picture { .. } => Color::WHITE,
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
    /// The slide's own background (`p:bg`); `None` follows the layout.
    pub background: Option<Background>,
    /// Layout and master decorations are not shown (`p:sld/@showMasterSp="0"`).
    pub hide_background_graphics: bool,
    /// Skipped in slide shows (`p:sld/@show="0"`).
    pub hidden: bool,
    /// Speaker notes as plain text, one line per paragraph.
    pub notes: String,
    pub transition: Option<Transition>,
    /// Animations, played from the original XML (which is saved as is).
    pub timing: Option<Arc<anim::Timing>>,
    pub source: Option<Arc<SlideSource>>,
}

/// The original part of a slide.
#[derive(Clone, Debug, PartialEq)]
pub struct SlideSource {
    pub part: String,
    pub doc: Arc<str>,
    pub namespaces: Arc<Vec<(String, String)>>,
    /// Shape ids as loaded, in order; equal to the current ids when the slide's shape list is untouched.
    pub shape_ids: Vec<ShapeId>,
    /// The transition as loaded, to tell whether it was edited.
    pub transition: Option<Transition>,
    /// The animations as loaded, to tell whether they were edited.
    pub timing: Option<Arc<anim::Timing>>,
    /// The layout as loaded, to tell whether another one was applied.
    pub layout: Option<usize>,
    /// The background and background graphics setting as loaded.
    pub background: Option<Background>,
    pub hide_background_graphics: bool,
}

/// A slide layout: what a new slide based on it starts with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub name: String,
    /// Empty placeholder shapes with fully resolved position and text styles.
    pub placeholders: Vec<Shape>,
    /// Date, footer and slide number placeholders, with the layout's text.
    pub footers: Vec<Shape>,
    /// Layout and master decorations.
    pub background_shapes: Vec<Shape>,
    /// The layout's background, or the master's when it has none.
    pub background: Option<Background>,
    /// Package part of the layout, e.g. `ppt/slideLayouts/slideLayout2.xml`.
    pub part: String,
}

/// A drawing guide (檢視 > 參考線): a line shapes snap to, not part of the slides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawGuide {
    pub vertical: bool,
    /// x of vertical guides, y of horizontal ones.
    pub pos: Emu,
}

/// Editing aids saved with the deck (`ppt/viewProps.xml`).
#[derive(Clone, Debug, PartialEq)]
pub struct ViewSettings {
    pub guides: Vec<DrawGuide>,
    pub show_guides: bool,
    pub snap_to_grid: bool,
    pub grid_spacing: Emu,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self { guides: Vec::new(), show_guides: false, snap_to_grid: false, grid_spacing: 72_000 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Presentation {
    pub slide_width: Emu,
    pub slide_height: Emu,
    pub slides: Vec<Slide>,
    pub layouts: Vec<Layout>,
    /// Encoded image bytes keyed by package part name.
    pub media: media::MediaStore,
    /// Theme fonts used as defaults for new text (latin, east asian).
    pub minor_font: (String, String),
    /// Defaults for new text boxes (the presentation's default text style).
    pub text_defaults: Option<Arc<Vec<ParaDefaults>>>,
    /// The package the deck was loaded from; saving copies untouched parts from it.
    pub package: Option<Arc<Vec<u8>>>,
    /// Style for new tables (the deck's default table style).
    pub table_style: Option<Arc<table::TableStyle>>,
    /// Built-in table styles offered in the style gallery, resolved against the theme.
    pub table_styles: Vec<Arc<table::TableStyle>>,
    /// The theme's hyperlink color, for links added in the editor.
    pub link_color: Option<Color>,
    pub view: ViewSettings,
    next_id: u32,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            slide_width: DEFAULT_SLIDE_WIDTH,
            slide_height: DEFAULT_SLIDE_HEIGHT,
            slides: Vec::new(),
            layouts: Vec::new(),
            media: media::MediaStore::default(),
            minor_font: ("Calibri".to_string(), String::new()),
            text_defaults: None,
            package: None,
            table_style: None,
            table_styles: Vec::new(),
            link_color: None,
            view: ViewSettings::default(),
            next_id: 1,
        }
    }
}

impl Presentation {
    /// A one-slide deck with a title text box, used when the app starts.
    pub fn new_blank() -> Self {
        let mut pres = Presentation::default();
        let mut slide = Slide { background: Some(Background::WHITE), ..Default::default() };
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
            return Slide { background: Some(Background::WHITE), ..Default::default() };
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
            background: None,
            hide_background_graphics: false,
            hidden: false,
            notes: String::new(),
            transition: None,
            timing: None,
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

    /// What `slide`'s background shows: its own, else its layout's, else white.
    pub fn slide_background<'a>(&'a self, slide: &'a Slide) -> &'a Background {
        slide.background.as_ref().or_else(|| slide.layout.and_then(|i| self.layouts.get(i)?.background.as_ref())).unwrap_or(&Background::WHITE)
    }

    pub fn shape(&self, slide: usize, id: ShapeId) -> Option<&Shape> {
        self.slides.get(slide)?.shapes.iter().find(|s| s.id == id)
    }

    pub fn shape_mut(&mut self, slide: usize, id: ShapeId) -> Option<&mut Shape> {
        self.slides.get_mut(slide)?.shapes.iter_mut().find(|s| s.id == id)
    }

    /// Every font family referenced by text on slides and layouts, plus
    /// [`EMOJI_FAMILY`] when some text contains emoji.
    pub fn font_families(&self) -> Vec<String> {
        let mut out = vec![self.minor_font.0.clone(), self.minor_font.1.clone()];
        let slides = self.slides.iter().flat_map(|s| s.background_shapes.iter().chain(&s.shapes));
        let layouts = self.layouts.iter().flat_map(|l| l.background_shapes.iter().chain(&l.placeholders));
        let mut emoji = false;
        for text in slides.chain(layouts).flat_map(Shape::text_bodies) {
            for p in &text.paragraphs {
                for style in p.runs.iter().map(|r| &r.style).chain([&p.end_style]) {
                    out.push(style.latin_font.clone());
                    out.push(style.ea_font.clone());
                }
                emoji = emoji || p.runs.iter().any(|r| r.text.chars().any(is_emoji));
                if let Some(b) = &p.bullet {
                    let (label, font) = b.label(1);
                    out.extend(font.map(str::to_string));
                    emoji = emoji || label.chars().any(is_emoji);
                }
            }
        }
        if emoji {
            out.push(EMOJI_FAMILY.to_string());
        }
        for shape in self.slides.iter().flat_map(|s| &s.shapes) {
            if let ShapeKind::Chart(c) = &shape.kind {
                out.extend(c.font_families());
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
