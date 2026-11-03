//! A shape's edited fill and outline (gradient with a clear stop, line opacity, dash, arrowheads)
//! survive saving and loading, both when the shape is patched in its slide and when it is new.

use std::sync::Arc;

use ppt_model::{Color, Dash, EndKind, Geometry, Gradient, GradientKind, Line, LineEnd, Paint, Shadow, Shape, ShapeKind, Xfrm};

fn deck() -> ppt_model::Presentation {
    ppt_ooxml::parse_pptx(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap()).unwrap()
}

fn gradient() -> Gradient {
    Gradient { stops: vec![(0.0, Color::rgb(255, 0, 0)), (0.4, Color { r: 0, g: 255, b: 0, a: 128 }), (1.0, Color::rgb(0, 0, 255))], kind: GradientKind::Linear { angle: 45.0, scaled: false }, rotate_with_shape: true }
}

fn styled_line() -> Line {
    Line { color: Color { r: 10, g: 20, b: 30, a: 100 }, width: 38_100, dash: Dash::SysDash, head: LineEnd { kind: EndKind::Oval, width: 0, length: 2 }, tail: LineEnd { kind: EndKind::Triangle, width: 2, length: 1 } }
}

fn shadow() -> Shadow {
    Shadow { color: Color { r: 200, g: 0, b: 0, a: 150 }, blur: 127_000, dist: 76_200, dir: 135.0 }
}

fn find<'a>(pres: &'a ppt_model::Presentation, name: &str) -> &'a Shape {
    pres.slides.iter().flat_map(|s| &s.shapes).find(|s| s.name == name).unwrap_or_else(|| panic!("no shape {name}"))
}

#[test]
fn edited_fill_and_outline_round_trip() {
    let mut pres = deck();
    // An existing plain shape (patched in place) and a new shape and a new line.
    let existing = pres.slides.iter().flat_map(|s| &s.shapes).find(|s| matches!(s.kind, ShapeKind::Shape) && !s.geometry.is_line()).map(|s| s.name.clone()).expect("a plain shape in the sample");
    for slide in &mut pres.slides {
        for s in slide.shapes.iter_mut().filter(|s| s.name == existing) {
            s.fill = Some(Paint::Gradient(Arc::new(gradient())));
            s.line = Some(Line { head: LineEnd::NONE, tail: LineEnd::NONE, ..styled_line() });
            s.shadow = Some(shadow());
        }
    }
    let id = pres.alloc_id();
    let mut fresh = Shape::new(id, "fresh", Xfrm::new(1_000_000, 1_000_000, 2_000_000, 1_000_000));
    fresh.fill = Some(Paint::Gradient(Arc::new(Gradient { kind: GradientKind::Path { circle: true, focus: [0.5; 4] }, ..gradient() })));
    fresh.line = Some(Line { head: LineEnd::NONE, tail: LineEnd::NONE, ..styled_line() });
    fresh.shadow = Some(shadow());
    pres.slides[0].shapes.push(fresh);
    let line_id = pres.alloc_id();
    let mut arrow = Shape::new(line_id, "arrow", Xfrm::new(500_000, 500_000, 3_000_000, 0));
    arrow.geometry = Geometry::preset("line");
    arrow.line = Some(styled_line());
    pres.slides[0].shapes.push(arrow);

    let again = ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&pres).unwrap()).unwrap();
    let same_fill = |a: &Option<Paint>, b: &Option<Paint>| match (a, b) {
        (Some(Paint::Gradient(x)), Some(Paint::Gradient(y))) => x.stops.len() == y.stops.len() && x.stops.iter().zip(&y.stops).all(|(p, q)| (p.0 - q.0).abs() < 0.01 && p.1.a.abs_diff(q.1.a) <= 1 && (p.1.r, p.1.g, p.1.b) == (q.1.r, q.1.g, q.1.b)) && x.kind == y.kind,
        _ => a == b,
    };
    let same_line = |a: &Option<Line>, b: &Option<Line>| match (a, b) {
        (Some(x), Some(y)) => x.color.a.abs_diff(y.color.a) <= 1 && (x.color.r, x.color.g, x.color.b, x.width, x.dash, x.head, x.tail) == (y.color.r, y.color.g, y.color.b, y.width, y.dash, y.head, y.tail),
        _ => a == b,
    };
    for name in [existing.as_str(), "fresh", "arrow"] {
        let (before, after) = (find(&pres, name), find(&again, name));
        if name != "arrow" {
            assert!(same_fill(&before.fill, &after.fill), "{name}: {:?} vs {:?}", before.fill, after.fill);
        }
        assert!(same_line(&before.line, &after.line), "{name}: {:?} vs {:?}", before.line, after.line);
        if name != "arrow" {
            let (b, a) = (before.shadow.unwrap(), after.shadow.unwrap_or_else(|| panic!("{name} lost its shadow")));
            assert_eq!((b.blur, b.dist, (b.color.r, b.color.g, b.color.b)), (a.blur, a.dist, (a.color.r, a.color.g, a.color.b)), "{name}");
            assert!((b.dir - a.dir).abs() < 0.01 && b.color.a.abs_diff(a.color.a) <= 1, "{name}: {b:?} vs {a:?}");
        }
    }
}
