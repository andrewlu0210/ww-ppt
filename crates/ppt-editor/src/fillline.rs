//! 設定圖形格式 › 填滿與線條: the fill and outline of the selected shapes, set in full
//! (solid or gradient fill with transparency; outline color, transparency, width, dash
//! and arrowheads) and read back for the format pane. A change that alters nothing is
//! not an undo step, so dragging a slider back and forth does not fill the history.

use std::sync::Arc;

use ppt_model::{Color, Dash, Gradient, Line, LineEnd, Paint, Shadow, Shape, ShapeKind};

use crate::Editor;

/// What a shape's fill should become.
#[derive(Clone, Debug, PartialEq)]
pub enum FillSpec {
    None,
    Solid(Color),
    Gradient(Gradient),
}

/// Changes to the outline; what is `None` stays as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineSpec {
    /// `Some(false)`: no outline. `Some(true)`: an outline (a black one if there was none).
    pub visible: Option<bool>,
    /// New red, green, blue.
    pub rgb: Option<(u8, u8, u8)>,
    /// New opacity, 0 (clear) to 255.
    pub alpha: Option<u8>,
    /// Width in EMU.
    pub width: Option<i64>,
    pub dash: Option<Dash>,
    pub head: Option<LineEnd>,
    pub tail: Option<LineEnd>,
}

/// Changes to the drop shadow; what is `None` stays as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadowSpec {
    /// `Some(false)`: no shadow. `Some(true)`: a shadow (the default one if there was none).
    pub visible: Option<bool>,
    pub rgb: Option<(u8, u8, u8)>,
    /// New opacity, 0 (clear) to 255.
    pub alpha: Option<u8>,
    /// Blur radius and offset distance in EMU, offset direction in degrees.
    pub blur: Option<i64>,
    pub dist: Option<i64>,
    pub dir: Option<f64>,
}

/// What the format pane shows for the selection. A field is `None` when the shapes differ.
#[derive(Clone, Debug, PartialEq)]
pub struct FillLineInfo {
    /// At least one selected shape can have a fill (not lines, pictures, tables ...).
    pub can_fill: bool,
    /// At least one selected shape is an open path, which can have arrowheads.
    pub can_arrows: bool,
    pub fill: Option<Option<Paint>>,
    pub line: Option<Option<Line>>,
    /// At least one selected shape can have a shadow.
    pub can_shadow: bool,
    pub shadow: Option<Option<Shadow>>,
}

fn can_fill(s: &Shape) -> bool {
    !s.geometry.is_line() && matches!(s.kind, ShapeKind::Shape)
}

/// Shapes and pictures take a drop shadow (tables, charts and SmartArt do not).
fn can_shadow(s: &Shape) -> bool {
    matches!(s.kind, ShapeKind::Shape | ShapeKind::Picture { .. })
}

/// The shadow a shape gets when it is given one without details (PowerPoint's offset diagonal bottom right).
fn default_shadow() -> Shadow {
    Shadow { color: Color { r: 0, g: 0, b: 0, a: 102 }, blur: 50_800, dist: 38_100, dir: 45.0 }
}

/// Lines, connectors and freeform paths that are not closed.
fn can_have_arrows(s: &Shape) -> bool {
    s.geometry.is_line() || matches!(&s.geometry, ppt_model::Geometry::Custom(paths) if !paths.is_empty() && paths.iter().all(|p| !p.fill))
}

impl Editor {
    /// Applies `f` to the selected shapes; an undo step only if something changed.
    fn edit_selected_if_changed(&mut self, f: impl Fn(&mut Shape)) -> bool {
        let changed = self.selected_shapes().iter().any(|s| {
            let mut copy = (*s).clone();
            f(&mut copy);
            copy != **s
        });
        changed && self.modify_selected(f)
    }

    /// Sets the fill of the selected shapes that can have one.
    pub fn set_fill_spec(&mut self, spec: FillSpec) -> bool {
        let paint = match spec {
            FillSpec::None => None,
            FillSpec::Solid(c) => Some(Paint::Solid(c)),
            FillSpec::Gradient(g) => Some(Paint::Gradient(Arc::new(g))),
        };
        self.edit_selected_if_changed(|s| {
            if can_fill(s) {
                s.fill = paint.clone();
            }
        })
    }

    /// Changes the outline of the selected shapes.
    pub fn set_line_spec(&mut self, spec: LineSpec) -> bool {
        self.edit_selected_if_changed(|s| {
            if matches!(s.kind, ShapeKind::Opaque { .. }) {
                return;
            }
            let current = s.line;
            if spec.visible == Some(false) {
                s.line = None;
                return;
            }
            // Touching any property of a shape without an outline gives it one.
            let wants_line = spec.visible == Some(true) || spec.rgb.is_some() || spec.alpha.is_some() || spec.width.is_some() || spec.dash.is_some() || spec.head.is_some() || spec.tail.is_some();
            let Some(mut line) = current.or_else(|| wants_line.then(|| Line::new(Color::BLACK, 12_700))) else { return };
            if let Some((r, g, b)) = spec.rgb {
                line.color = Color { r, g, b, a: line.color.a };
            }
            if let Some(a) = spec.alpha {
                line.color.a = a;
            }
            if let Some(w) = spec.width {
                line.width = w.max(0);
            }
            if let Some(d) = spec.dash {
                line.dash = d;
            }
            if can_have_arrows(s) {
                if let Some(h) = spec.head {
                    line.head = h;
                }
                if let Some(t) = spec.tail {
                    line.tail = t;
                }
            }
            s.line = Some(line);
        })
    }

    /// Changes the drop shadow of the selected shapes.
    pub fn set_shadow_spec(&mut self, spec: ShadowSpec) -> bool {
        self.edit_selected_if_changed(|s| {
            if !can_shadow(s) {
                return;
            }
            if spec.visible == Some(false) {
                s.shadow = None;
                return;
            }
            let wants = spec.visible == Some(true) || spec.rgb.is_some() || spec.alpha.is_some() || spec.blur.is_some() || spec.dist.is_some() || spec.dir.is_some();
            let Some(mut shadow) = s.shadow.or_else(|| wants.then(default_shadow)) else { return };
            if let Some((r, g, b)) = spec.rgb {
                shadow.color = Color { r, g, b, a: shadow.color.a };
            }
            if let Some(a) = spec.alpha {
                shadow.color.a = a;
            }
            if let Some(v) = spec.blur {
                shadow.blur = v.max(0);
            }
            if let Some(v) = spec.dist {
                shadow.dist = v.max(0);
            }
            if let Some(v) = spec.dir {
                shadow.dir = v.rem_euclid(360.0);
            }
            s.shadow = Some(shadow);
        })
    }

    /// The fill and outline of the selection, `None` when nothing is selected.
    pub fn fill_line_info(&self) -> Option<FillLineInfo> {
        let shapes = self.selected_shapes();
        if shapes.is_empty() {
            return None;
        }
        let fillable: Vec<&&Shape> = shapes.iter().filter(|s| can_fill(s)).collect();
        // The common value, or `None` when the shapes differ.
        fn common<T: PartialEq>(mut items: impl Iterator<Item = T>) -> Option<T> {
            let first = items.next()?;
            let mut rest = items;
            rest.all(|v| v == first).then_some(first)
        }
        let fill = common(fillable.iter().map(|s| s.fill.clone()));
        let line = common(shapes.iter().map(|s| s.line));
        let shadowable: Vec<&&Shape> = shapes.iter().filter(|s| can_shadow(s)).collect();
        let shadow = common(shadowable.iter().map(|s| s.shadow));
        Some(FillLineInfo { can_fill: !fillable.is_empty(), can_arrows: shapes.iter().any(|s| can_have_arrows(s)), fill, line, can_shadow: !shadowable.is_empty(), shadow })
    }
}
