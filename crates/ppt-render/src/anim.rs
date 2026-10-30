//! Drawing a slide in the middle of its animations: each shape with its
//! animated state (hidden, faded, moved, scaled, rotated, partly revealed).

use ppt_geometry::shape_affine;
use ppt_model::anim::{Frame, ParaState, ShapeState};
use ppt_model::{Paint, Presentation, Shape, Slide};
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Ellipse, Point, Rect, Shape as _, Vec2};
use vello::peniko::{BlendMode, Fill, Mix};

use crate::{DrawOptions, RenderCtx, draw_shape};

/// Like [`crate::draw_slide`], with the animation state in `frame` applied.
pub fn draw_slide_animated(scene: &mut Scene, cx: &mut RenderCtx, pres: &Presentation, slide: &Slide, view: Affine, opts: &DrawOptions, frame: &Frame) {
    let page = Rect::new(0.0, 0.0, pres.slide_width as f64, pres.slide_height as f64);
    scene.fill(Fill::NonZero, view, crate::color(slide.background.unwrap_or(ppt_model::Color::WHITE)), None, &page);
    scene.push_clip_layer(Fill::NonZero, view, &page);
    for shape in &slide.background_shapes {
        draw_shape(scene, cx, pres, shape, view, &DrawOptions { editing: false, ..*opts });
    }
    for shape in &slide.shapes {
        let with_paras = with_paragraph_states(shape, frame);
        let shape = with_paras.as_ref().unwrap_or(shape);
        match frame.shapes.get(&shape.id) {
            None => draw_shape(scene, cx, pres, shape, view, opts),
            Some(st) => draw_animated_shape(scene, cx, pres, shape, view, opts, st, page),
        }
    }
    scene.pop_layer();
}

#[allow(clippy::too_many_arguments)]
fn draw_animated_shape(scene: &mut Scene, cx: &mut RenderCtx, pres: &Presentation, shape: &Shape, view: Affine, opts: &DrawOptions, st: &ShapeState, page: Rect) {
    if !st.visible || st.opacity <= 0.0 || st.sx.abs() < 1e-6 || st.sy.abs() < 1e-6 {
        return;
    }
    let pivot = Vec2::new(st.pivot.0, st.pivot.1);
    let motion = Affine::translate(pivot + Vec2::new(st.dx, st.dy)) * Affine::rotate(st.rot.to_radians()) * Affine::scale_non_uniform(st.sx, st.sy) * Affine::translate(-pivot);
    let view = view * motion;
    let recolored;
    let shape = if st.fill.is_some() || st.line.is_some() {
        let mut s = shape.clone();
        if let Some(c) = st.fill {
            s.fill = Some(Paint::Solid(c));
        }
        if let (Some(c), Some(line)) = (st.line, s.line.as_mut()) {
            line.color = c;
        }
        recolored = s;
        &recolored
    } else {
        shape
    };
    let mut layers = 0;
    if st.opacity < 1.0 {
        scene.push_layer(Fill::NonZero, BlendMode::from(Mix::Normal), st.opacity as f32, view, &page.inflate(page.width(), page.height()));
        layers += 1;
    }
    if let Some((filter, r)) = &st.reveal {
        if *r <= 0.0 {
            for _ in 0..layers {
                scene.pop_layer();
            }
            return;
        }
        let (w, h) = (shape.xfrm.w as f64, shape.xfrm.h as f64);
        let (mask, rule) = reveal_mask(filter, *r, w.max(1.0), h.max(1.0));
        scene.push_clip_layer(rule, view * shape_affine(&shape.xfrm, false), &mask);
        layers += 1;
    }
    draw_shape(scene, cx, pres, shape, view, opts);
    for _ in 0..layers {
        scene.pop_layer();
    }
}

/// The shape with hidden or fading paragraphs made transparent (layout unchanged).
fn with_paragraph_states(shape: &Shape, frame: &Frame) -> Option<Shape> {
    let text = shape.text.as_ref()?;
    let states: Vec<(usize, ParaState)> = (0..text.paragraphs.len()).filter_map(|i| frame.paragraphs.get(&(shape.id, i)).map(|s| (i, *s))).collect();
    if states.iter().all(|(_, s)| s.visible && s.opacity >= 1.0) {
        return None;
    }
    let mut shape = shape.clone();
    let body = shape.text.as_mut()?;
    for (i, st) in states {
        let alpha = if st.visible { st.opacity.clamp(0.0, 1.0) } else { 0.0 };
        let p = &mut body.paragraphs[i];
        let fade = |c: &mut ppt_model::Color| c.a = (c.a as f64 * alpha).round() as u8;
        for run in &mut p.runs {
            fade(&mut run.style.color);
        }
        fade(&mut p.end_style.color);
        if let Some(b) = p.bullet.as_mut() {
            // A bullet without its own color takes the (now faded) text color.
            if let Some(c) = b.color.as_mut() {
                fade(c);
            }
        }
    }
    Some(shape)
}

/// A fixed shuffled order of `n` items.
fn shuffled(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    for i in (1..n).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        order.swap(i, (x % (i as u64 + 1)) as usize);
    }
    order
}

fn rects(rs: impl IntoIterator<Item = Rect>) -> BezPath {
    let mut p = BezPath::new();
    for r in rs {
        p.extend(r.path_elements(0.1));
    }
    p
}

/// The visible part of a `w` x `h` shape (local coordinates) when filter
/// `filter` has revealed fraction `r`. Unknown filters reveal everything.
pub fn reveal_mask(filter: &str, r: f64, w: f64, h: f64) -> (BezPath, Fill) {
    let r = r.clamp(0.0, 1.0);
    let (name, arg) = filter.split_once('(').map_or((filter, ""), |(n, a)| (n, a.trim_end_matches(')')));
    let full = Rect::new(0.0, 0.0, w, h);
    let (cx, cy) = (w / 2.0, h / 2.0);
    // Everything except an inner shape: "in" variants that close toward the center.
    let outside = |inner: BezPath| {
        let mut p = BezPath::from_vec(full.path_elements(0.1).collect());
        p.extend(inner);
        (p, Fill::EvenOdd)
    };
    match name {
        "wipe" | "slide" => {
            let rect = match arg {
                "down" | "fromTop" => Rect::new(0.0, 0.0, w, h * r),
                "up" | "fromBottom" => Rect::new(0.0, h * (1.0 - r), w, h),
                "left" | "fromRight" => Rect::new(w * (1.0 - r), 0.0, w, h),
                _ => Rect::new(0.0, 0.0, w * r, h),
            };
            (rects([rect]), Fill::NonZero)
        }
        "barn" => match arg {
            "inVertical" => (rects([Rect::new(0.0, 0.0, cx * r, h), Rect::new(w - cx * r, 0.0, w, h)]), Fill::NonZero),
            "inHorizontal" => (rects([Rect::new(0.0, 0.0, w, cy * r), Rect::new(0.0, h - cy * r, w, h)]), Fill::NonZero),
            "outHorizontal" => (rects([Rect::new(0.0, cy * (1.0 - r), w, cy * (1.0 + r))]), Fill::NonZero),
            _ => (rects([Rect::new(cx * (1.0 - r), 0.0, cx * (1.0 + r), h)]), Fill::NonZero),
        },
        "blinds" => {
            const N: usize = 6;
            let vertical = arg == "vertical";
            let band = if vertical { w } else { h } / N as f64;
            (
                rects((0..N).map(|i| {
                    let a = i as f64 * band;
                    if vertical { Rect::new(a, 0.0, a + band * r, h) } else { Rect::new(0.0, a, w, a + band * r) }
                })),
                Fill::NonZero,
            )
        }
        "box" => {
            if arg == "in" {
                outside(rects([Rect::new(cx * r, cy * r, w - cx * r, h - cy * r)]))
            } else {
                (rects([Rect::new(cx * (1.0 - r), cy * (1.0 - r), cx * (1.0 + r), cy * (1.0 + r))]), Fill::NonZero)
            }
        }
        "circle" => {
            // The ellipse through the corners at r = 1.
            let k = std::f64::consts::SQRT_2;
            if arg == "in" {
                let k = k * (1.0 - r);
                outside(Ellipse::new(Point::new(cx, cy), (cx * k, cy * k), 0.0).to_path(0.1))
            } else {
                (Ellipse::new(Point::new(cx, cy), (cx * k * r, cy * k * r), 0.0).to_path(0.1), Fill::NonZero)
            }
        }
        "diamond" => {
            let diamond = |k: f64| {
                let mut p = BezPath::new();
                p.move_to((cx, cy - h * k));
                p.line_to((cx + w * k, cy));
                p.line_to((cx, cy + h * k));
                p.line_to((cx - w * k, cy));
                p.close_path();
                p
            };
            if arg == "in" { outside(diamond(1.0 - r)) } else { (diamond(r), Fill::NonZero) }
        }
        "plus" => {
            let plus = |k: f64| rects([Rect::new(cx - cx * k, 0.0, cx + cx * k, h), Rect::new(0.0, cy - cy * k, w, cy + cy * k)]);
            if arg == "in" { outside(plus(1.0 - r)) } else { (plus(r), Fill::NonZero) }
        }
        "wheel" | "wedge" => {
            // A pie growing clockwise from 12 o'clock (wheel: one spoke per arg).
            let spokes = arg.parse::<usize>().unwrap_or(1).max(1);
            let rad = (w * w + h * h).sqrt();
            let mut p = BezPath::new();
            let sweep = std::f64::consts::TAU / spokes as f64 * r;
            for i in 0..spokes {
                let start = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / spokes as f64;
                p.move_to((cx, cy));
                let steps = 24;
                for s in 0..=steps {
                    let a = start + sweep * s as f64 / steps as f64;
                    p.line_to((cx + rad * a.cos(), cy + rad * a.sin()));
                }
                p.close_path();
            }
            (p, Fill::NonZero)
        }
        "dissolve" | "checkerboard" => {
            let cols = 16usize;
            let cell = w / cols as f64;
            let rows = ((h / cell).ceil() as usize).max(1);
            let n = cols * rows;
            let shown = (r * n as f64).round() as usize;
            (
                rects(shuffled(n, 5).into_iter().take(shown).map(|i| {
                    let (c, rw) = ((i % cols) as f64, (i / cols) as f64);
                    Rect::new(c * cell, rw * cell, (c + 1.02) * cell, (rw + 1.02) * cell)
                })),
                Fill::NonZero,
            )
        }
        "randombar" => {
            const N: usize = 48;
            let vertical = arg == "vertical";
            let bar = if vertical { w } else { h } / N as f64;
            let shown = (r * N as f64).round() as usize;
            (
                rects(shuffled(N, 9).into_iter().take(shown).map(|i| {
                    let a = i as f64 * bar;
                    if vertical { Rect::new(a, 0.0, a + bar * 1.02, h) } else { Rect::new(0.0, a, w, a + bar * 1.02) }
                })),
                Fill::NonZero,
            )
        }
        "strips" => {
            // A diagonal edge sweeping across from the named corner's opposite.
            let (sx, sy) = match arg {
                "downLeft" => (-1.0, 1.0),
                "upLeft" => (-1.0, -1.0),
                "upRight" => (1.0, -1.0),
                _ => (1.0, 1.0),
            };
            let d = (w + h) * r;
            let mut p = BezPath::new();
            let corner = |x: f64, y: f64| Point::new(if sx > 0.0 { x } else { w - x }, if sy > 0.0 { y } else { h - y });
            p.move_to(corner(0.0, 0.0));
            p.line_to(corner(d, 0.0));
            p.line_to(corner(0.0, d));
            p.close_path();
            (p, Fill::NonZero)
        }
        _ => (rects([full]), Fill::NonZero),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_grow_with_progress() {
        for f in ["wipe(down)", "wipe(left)", "barn(outVertical)", "blinds(horizontal)", "box(out)", "circle(out)", "diamond(out)", "dissolve", "randombar(vertical)", "wheel(2)"] {
            let (a, b) = (reveal_mask(f, 0.25, 100.0, 50.0).0.area().abs(), reveal_mask(f, 0.75, 100.0, 50.0).0.area().abs());
            assert!(b > a, "{f}: {a} vs {b}");
        }
        assert!((reveal_mask("wipe(down)", 0.5, 100.0, 50.0).0.area().abs() - 2500.0).abs() < 1e-6);
        assert_eq!(reveal_mask("box(in)", 0.5, 100.0, 50.0).1, Fill::EvenOdd);
        assert!((reveal_mask("unknown", 0.1, 100.0, 50.0).0.area().abs() - 5000.0).abs() < 1e-6);
    }
}
