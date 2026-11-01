//! Shape outlines for DrawingML geometry: presets via the guide-formula
//! engine in [`preset`], custom geometry from parsed paths.

mod preset;

use kurbo::{Affine, BezPath, Point, Shape as _, Vec2};
use ppt_model::{Anchor, EMU_PER_PT, Emu, Geometry, PathCmd, TextBody, Xfrm};

pub use kurbo;

/// Maps the shape's local frame (EMU, origin at its top-left) to slide space.
pub fn shape_affine(xfrm: &Xfrm, with_flip: bool) -> Affine {
    let (cx, cy) = xfrm.center();
    let (w, h) = (xfrm.w as f64, xfrm.h as f64);
    let flip = if with_flip {
        Affine::scale_non_uniform(if xfrm.flip_h { -1.0 } else { 1.0 }, if xfrm.flip_v { -1.0 } else { 1.0 })
    } else {
        Affine::IDENTITY
    };
    Affine::translate(Vec2::new(cx, cy)) * Affine::rotate(xfrm.rot.to_radians()) * flip * Affine::translate(Vec2::new(-w / 2.0, -h / 2.0))
}

/// Where a shape's text goes: its frame, plus the preset's text rectangle
/// (an ellipse keeps text away from its curve) as extra left/top/right/bottom insets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextFrame {
    pub xfrm: Xfrm,
    pub inset: [Emu; 4],
}

impl TextFrame {
    /// The whole frame (text boxes, table cells).
    pub fn plain(xfrm: Xfrm) -> Self {
        TextFrame { xfrm, inset: [0; 4] }
    }

    /// The frame of a shape's text: the preset's text rectangle, unless the text has a frame of its own.
    pub fn for_text(xfrm: Xfrm, geometry: &Geometry, body: Option<&TextBody>) -> Self {
        if body.is_some_and(|b| b.exact_frame) { TextFrame::plain(xfrm) } else { TextFrame::of(xfrm, geometry) }
    }

    pub fn of(xfrm: Xfrm, geometry: &Geometry) -> Self {
        let (w, h) = (xfrm.w as f64, xfrm.h as f64);
        let inset = match geometry {
            Geometry::Preset { name, adjust } => preset::text_rect(name, adjust, w, h)
                .map(|[l, t, r, b]| [l.max(0.0), t.max(0.0), (w - r).max(0.0), (h - b).max(0.0)].map(|v| v.round() as Emu))
                .unwrap_or([0; 4]),
            Geometry::Custom(_) => [0; 4],
        };
        TextFrame { xfrm, inset }
    }

    fn insets(&self, body: &TextBody) -> [f64; 4] {
        std::array::from_fn(|i| (self.inset[i] + body.insets[i]) as f64)
    }
}

/// Width available to text inside the text rectangle and body insets, in points.
pub fn text_width_pt(frame: &TextFrame, body: &TextBody) -> f32 {
    let [l, _, r, _] = frame.insets(body);
    ((frame.xfrm.w as f64 - l - r).max(0.0) / EMU_PER_PT) as f32
}

/// Maps text layout space (points, origin at the first line) to slide space,
/// honoring insets and vertical anchoring. Text is never mirrored by flips.
pub fn text_affine(frame: &TextFrame, body: &TextBody, content_height_pt: f32) -> Affine {
    let xfrm = &frame.xfrm;
    let [l, t, _, b] = frame.insets(body);
    let inner_h = (xfrm.h as f64 - t - b) / EMU_PER_PT;
    let content = content_height_pt as f64;
    let dy = match body.anchor {
        Anchor::Top => 0.0,
        Anchor::Middle => (inner_h - content) / 2.0,
        Anchor::Bottom => inner_h - content,
    };
    shape_affine(xfrm, false)
        * Affine::translate(Vec2::new(l, t))
        * Affine::scale(EMU_PER_PT)
        * Affine::translate(Vec2::new(0.0, dy))
}

/// An outline in the shape's local frame, (0, 0)..(w, h).
pub struct Outline {
    pub path: BezPath,
    pub fill: bool,
    pub stroke: bool,
    /// Lighten/darken applied over the fill (3D-looking presets).
    pub shade: Shade,
}

pub use preset::{Shade, is_known as is_known_preset, text_rect as preset_text_rect};

/// Builds the outlines for `geometry` at size `w` × `h` (any unit).
pub fn outlines(geometry: &Geometry, w: f64, h: f64) -> Vec<Outline> {
    match geometry {
        Geometry::Preset { name, adjust } => preset::outlines(name, adjust, w, h)
            // Unknown presets show as their frame.
            .unwrap_or_else(|| vec![Outline { path: kurbo::Rect::new(0.0, 0.0, w, h).to_path(0.1), fill: true, stroke: true, shade: Shade::Normal }]),
        Geometry::Custom(paths) => paths
            .iter()
            .map(|p| {
                let sx = if p.w > 0.0 { w / p.w } else { 1.0 };
                let sy = if p.h > 0.0 { h / p.h } else { 1.0 };
                let pt = |x: f64, y: f64| Point::new(x * sx, y * sy);
                let mut path = BezPath::new();
                for cmd in &p.cmds {
                    match *cmd {
                        PathCmd::MoveTo(x, y) => path.move_to(pt(x, y)),
                        PathCmd::LineTo(x, y) => path.line_to(pt(x, y)),
                        PathCmd::QuadTo(x1, y1, x, y) => path.quad_to(pt(x1, y1), pt(x, y)),
                        PathCmd::CubicTo(x1, y1, x2, y2, x, y) => path.curve_to(pt(x1, y1), pt(x2, y2), pt(x, y)),
                        PathCmd::Close => path.close_path(),
                    }
                }
                Outline { path, fill: p.fill, stroke: p.stroke, shade: if p.fill { Shade::Normal } else { Shade::None } }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_stay_inside_bounds() {
        for name in ["rect", "roundRect", "ellipse", "triangle", "diamond", "hexagon", "star5", "rightArrow", "chevron"] {
            let out = outlines(&Geometry::preset(name), 200.0, 100.0);
            let bb = out[0].path.bounding_box();
            assert!(bb.x0 >= -1e-6 && bb.y0 >= -1e-6 && bb.x1 <= 200.0 + 1e-6 && bb.y1 <= 100.0 + 1e-6, "{name}: {bb:?}");
        }
    }

    #[test]
    fn shape_affine_matches_xfrm_to_slide() {
        let x = Xfrm { rot: 45.0, ..Xfrm::new(1000, 2000, 400, 200) };
        let p = shape_affine(&x, true) * Point::new(100.0, 50.0);
        let (sx, sy) = x.to_slide(100.0, 50.0);
        assert!((p.x - sx).abs() < 1e-6 && (p.y - sy).abs() < 1e-6);
    }

    #[test]
    fn text_affine_centers_content() {
        let x = Xfrm::new(0, 0, 254_000, 254_000);
        let body = TextBody { anchor: Anchor::Middle, insets: [0; 4], ..Default::default() };
        // 20pt box, 10pt content → content starts 5pt down.
        let p = text_affine(&TextFrame::plain(x), &body, 10.0) * Point::new(0.0, 0.0);
        assert!((p.y - 5.0 * EMU_PER_PT).abs() < 1e-6);
    }

    #[test]
    fn lines_are_not_filled() {
        let out = outlines(&Geometry::preset("straightConnector1"), 10.0, 10.0);
        assert!(!out[0].fill && out[0].stroke);
    }
}
