//! Shape outlines for DrawingML geometry.
//!
//! M0 covers the most common presets with hand-written outlines. The full
//! `presetShapeDefinitions.xml` guide-formula engine replaces this in M1.

use kurbo::{Affine, BezPath, Ellipse, Point, RoundedRect, Shape as _, Vec2};
use ppt_model::{Anchor, EMU_PER_PT, Geometry, PathCmd, TextBody, Xfrm};

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

/// Width available to text inside the body insets, in points.
pub fn text_width_pt(xfrm: &Xfrm, body: &TextBody) -> f32 {
    ((xfrm.w - body.insets[0] - body.insets[2]).max(0) as f64 / EMU_PER_PT) as f32
}

/// Maps text layout space (points, origin at the first line) to slide space,
/// honoring insets and vertical anchoring. Text is never mirrored by flips.
pub fn text_affine(xfrm: &Xfrm, body: &TextBody, content_height_pt: f32) -> Affine {
    let inner_h = (xfrm.h - body.insets[1] - body.insets[3]) as f64 / EMU_PER_PT;
    let content = content_height_pt as f64;
    let dy = match body.anchor {
        Anchor::Top => 0.0,
        Anchor::Middle => (inner_h - content) / 2.0,
        Anchor::Bottom => inner_h - content,
    };
    shape_affine(xfrm, false)
        * Affine::translate(Vec2::new(body.insets[0] as f64, body.insets[1] as f64))
        * Affine::scale(EMU_PER_PT)
        * Affine::translate(Vec2::new(0.0, dy))
}

/// An outline in the shape's local frame, (0, 0)..(w, h).
pub struct Outline {
    pub path: BezPath,
    pub fill: bool,
    pub stroke: bool,
}

/// Builds the outlines for `geometry` at size `w` × `h` (any unit).
pub fn outlines(geometry: &Geometry, w: f64, h: f64) -> Vec<Outline> {
    match geometry {
        Geometry::Preset { name, adjust } => {
            let adj = |key: &str, default: f64| {
                adjust.iter().find(|(k, _)| k == key).map(|(_, v)| *v).unwrap_or(default) / 100_000.0
            };
            let line = geometry.is_line();
            vec![Outline { path: preset_path(name, w, h, &adj), fill: !line, stroke: true }]
        }
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
                Outline { path, fill: p.fill, stroke: p.stroke }
            })
            .collect(),
    }
}

fn polygon(points: &[(f64, f64)]) -> BezPath {
    let mut path = BezPath::new();
    for (i, &(x, y)) in points.iter().enumerate() {
        if i == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    path.close_path();
    path
}

fn preset_path(name: &str, w: f64, h: f64, adj: &dyn Fn(&str, f64) -> f64) -> BezPath {
    let ss = w.min(h);
    match name {
        "line" | "straightConnector1" | "bentConnector2" | "bentConnector3" | "curvedConnector3" => {
            let mut path = BezPath::new();
            path.move_to((0.0, 0.0));
            path.line_to((w, h));
            path
        }
        "ellipse" => Ellipse::new((w / 2.0, h / 2.0), (w / 2.0, h / 2.0), 0.0).to_path(0.1),
        "roundRect" => {
            let r = ss * adj("adj", 16_667.0);
            RoundedRect::new(0.0, 0.0, w, h, r).to_path(0.1)
        }
        "triangle" => {
            let apex = w * adj("adj", 50_000.0);
            polygon(&[(apex, 0.0), (w, h), (0.0, h)])
        }
        "rtTriangle" => polygon(&[(0.0, 0.0), (w, h), (0.0, h)]),
        "diamond" => polygon(&[(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)]),
        "parallelogram" => {
            let x = ss * adj("adj", 25_000.0);
            polygon(&[(x, 0.0), (w, 0.0), (w - x, h), (0.0, h)])
        }
        "trapezoid" => {
            let x = ss * adj("adj", 25_000.0);
            polygon(&[(0.0, h), (x, 0.0), (w - x, 0.0), (w, h)])
        }
        "hexagon" => {
            let x = ss * adj("adj", 25_000.0);
            polygon(&[(0.0, h / 2.0), (x, 0.0), (w - x, 0.0), (w, h / 2.0), (w - x, h), (x, h)])
        }
        "octagon" => {
            let x = ss * adj("adj", 29_289.0);
            polygon(&[(x, 0.0), (w - x, 0.0), (w, x), (w, h - x), (w - x, h), (x, h), (0.0, h - x), (0.0, x)])
        }
        "pentagon" => polygon(&[
            (w / 2.0, 0.0),
            (w, h * 0.382),
            (w * 0.809, h),
            (w * 0.191, h),
            (0.0, h * 0.382),
        ]),
        "star5" => {
            let mut pts = Vec::with_capacity(10);
            for i in 0..10 {
                let r = if i % 2 == 0 { 0.5 } else { 0.5 * 0.382 };
                let a = (-90.0 + 36.0 * i as f64).to_radians();
                pts.push((w / 2.0 + w * r * a.cos(), h / 2.0 + h * r * a.sin()));
            }
            polygon(&pts)
        }
        "rightArrow" => {
            let body = h * adj("adj1", 50_000.0);
            let head = ss * adj("adj2", 50_000.0);
            let (y1, y2) = ((h - body) / 2.0, (h + body) / 2.0);
            polygon(&[(0.0, y1), (w - head, y1), (w - head, 0.0), (w, h / 2.0), (w - head, h), (w - head, y2), (0.0, y2)])
        }
        "leftArrow" => {
            let body = h * adj("adj1", 50_000.0);
            let head = ss * adj("adj2", 50_000.0);
            let (y1, y2) = ((h - body) / 2.0, (h + body) / 2.0);
            polygon(&[(w, y1), (head, y1), (head, 0.0), (0.0, h / 2.0), (head, h), (head, y2), (w, y2)])
        }
        "chevron" => {
            let x = ss * adj("adj", 50_000.0);
            polygon(&[(0.0, 0.0), (w - x, 0.0), (w, h / 2.0), (w - x, h), (0.0, h), (x, h / 2.0)])
        }
        "homePlate" => {
            let x = ss * adj("adj", 50_000.0);
            polygon(&[(0.0, 0.0), (w - x, 0.0), (w, h / 2.0), (w - x, h), (0.0, h)])
        }
        // "rect" and every preset not implemented yet.
        _ => kurbo::Rect::new(0.0, 0.0, w, h).to_path(0.1),
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
        let p = text_affine(&x, &body, 10.0) * Point::new(0.0, 0.0);
        assert!((p.y - 5.0 * EMU_PER_PT).abs() < 1e-6);
    }

    #[test]
    fn lines_are_not_filled() {
        let out = outlines(&Geometry::preset("straightConnector1"), 10.0, 10.0);
        assert!(!out[0].fill && out[0].stroke);
    }
}
