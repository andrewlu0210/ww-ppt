//! SmartArt editing against the visual-regression samples: read the node tree,
//! edit it, save, and reopen.

use std::io::{Cursor, Read};

use ppt_model::diagram::{Arrangement, Diagram, Insert};
use ppt_model::{Presentation, ShapeKind};
use ppt_ooxml::{parse_pptx, save_pptx};

fn deck(name: &str) -> Presentation {
    let path = format!("{}/../../samples/vr/{name}", env!("CARGO_MANIFEST_DIR"));
    parse_pptx(&std::fs::read(path).unwrap()).unwrap()
}

/// (slide, shape) of every SmartArt with an editable node tree.
fn diagrams(pres: &Presentation) -> Vec<(usize, usize, &Diagram)> {
    let mut out = Vec::new();
    for (si, slide) in pres.slides.iter().enumerate() {
        for (i, s) in slide.shapes.iter().enumerate() {
            if let ShapeKind::Diagram { data: Some(d), .. } = &s.kind {
                out.push((si, i, d.as_ref()));
            }
        }
    }
    out
}

fn texts(d: &Diagram) -> Vec<String> {
    d.rows().into_iter().map(|r| format!("{}{}", "  ".repeat(r.depth), r.text)).collect()
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

/// Applies `edit` to the diagram at (slide, shape) and rebuilds its drawing.
fn edit(pres: &mut Presentation, slide: usize, shape: usize, edit: impl FnOnce(&mut Diagram)) {
    let s = &mut pres.slides[slide].shapes[shape];
    let ShapeKind::Diagram { shapes, data: Some(data), .. } = &mut s.kind else { panic!("not a diagram") };
    let mut d = (**data).clone();
    edit(&mut d);
    *shapes = std::sync::Arc::new(d.layout());
    *data = std::sync::Arc::new(d);
}

fn layout_name(d: &Diagram) -> &str {
    d.template.0.layout.rsplit('/').next().unwrap()
}

#[test]
fn every_sample_diagram_reads() {
    let mut seen = Vec::new();
    for name in ["test2.pptx", "test4.pptx", "test7.pptx"] {
        let pres = deck(name);
        let total = pres.slides.iter().flat_map(|s| &s.shapes).filter(|s| matches!(s.kind, ShapeKind::Diagram { .. })).count();
        let found = diagrams(&pres);
        assert_eq!(found.len(), total, "{name}: every drawn SmartArt has a node tree");
        for (_, _, d) in found {
            seen.push((layout_name(d).to_string(), d.arrangement(), d.can_nest()));
        }
    }
    let arrangement = |l: &str| seen.iter().find(|s| s.0 == l).map(|s| (s.1, s.2)).unwrap();
    assert_eq!(arrangement("vList2"), (Arrangement::Line { vertical: true }, false));
    assert_eq!(arrangement("hChevron3"), (Arrangement::Line { vertical: false }, false));
    assert_eq!(arrangement("chevron1"), (Arrangement::Line { vertical: false }, false));
    assert_eq!(arrangement("hList1"), (Arrangement::Line { vertical: false }, true));
    assert_eq!(arrangement("process3"), (Arrangement::Line { vertical: false }, true));
    assert_eq!(arrangement("cycle2"), (Arrangement::Cycle, false));
    // Text tied to the diagram as a whole: only text can change.
    assert_eq!(arrangement("PhasedProcess").0, Arrangement::Fixed);
}

#[test]
fn untouched_diagrams_save_verbatim() {
    let pres = deck("test7.pptx");
    let saved = save_pptx(&pres).unwrap();
    let orig = std::fs::read(format!("{}/../../samples/vr/test7.pptx", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for i in [1, 9, 11] {
        assert_eq!(part(&saved, &format!("ppt/diagrams/data{i}.xml")), part(&orig, &format!("ppt/diagrams/data{i}.xml")));
        assert_eq!(part(&saved, &format!("ppt/diagrams/drawing{i}.xml")), part(&orig, &format!("ppt/diagrams/drawing{i}.xml")));
    }
}

#[test]
fn retyped_text_round_trips() {
    let mut pres = deck("test7.pptx");
    let (si, i, d) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "vList2").unwrap();
    let before = texts(d);
    edit(&mut pres, si, i, |d| assert!(d.set_text(2, "新的文字")));
    let back = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let ShapeKind::Diagram { shapes, data: Some(d), .. } = &back.slides[si].shapes[i].kind else { panic!() };
    let mut want = before.clone();
    want[2] = "新的文字".into();
    assert_eq!(texts(d), want);
    // The drawing shows it too, in the same shape.
    assert_eq!(shapes.len(), d.template.0.shapes.len());
    assert!(shapes.iter().any(|s| s.text.as_ref().is_some_and(|t| t.plain_text() == "新的文字")));
}

#[test]
fn added_and_removed_nodes_round_trip() {
    let mut pres = deck("test7.pptx");
    let (si, i, d) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "hChevron3").unwrap();
    let before = texts(d);
    assert_eq!(before.len(), 4);
    let shapes_before = d.template.0.shapes.len();
    edit(&mut pres, si, i, |d| {
        let r = d.insert(3, Insert::After).unwrap();
        d.set_text(r, "Q5");
        d.delete(0);
    });
    let saved = save_pptx(&pres).unwrap();
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { shapes, data: Some(d), .. } = &back.slides[si].shapes[i].kind else { panic!("diagram survives") };
    let mut want = before[1..].to_vec();
    want.push("Q5".into());
    assert_eq!(texts(d), want);
    assert_eq!(shapes.len(), shapes_before);
    // Laid out again: still a line, every chevron inside the frame, left to right.
    assert_eq!(d.arrangement(), Arrangement::Line { vertical: false });
    assert!(shapes.windows(2).all(|w| w[0].xfrm.x < w[1].xfrm.x));
    assert!(shapes.iter().any(|s| s.text.as_ref().is_some_and(|t| t.plain_text() == "Q5")));
}

#[test]
fn children_round_trip() {
    let mut pres = deck("test2.pptx");
    let (si, i, d) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "hList1").unwrap();
    let before = texts(d);
    edit(&mut pres, si, i, |d| {
        // A child under the first node, and a new top-level node with a child.
        let r = d.insert(1, Insert::After).unwrap();
        d.set_text(r, "新的子項目");
        let last = d.rows().len() - 1;
        let top = d.insert(last, Insert::After).unwrap();
        // Inserting after a child stays at the child's level: promote it.
        assert!(d.shift(top, false));
        d.set_text(top, "第五欄");
        let kid = d.insert(top, Insert::Child).unwrap();
        d.set_text(kid, "第五欄的項目");
    });
    let back = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let ShapeKind::Diagram { shapes, data: Some(d), .. } = &back.slides[si].shapes[i].kind else { panic!() };
    let got = texts(d);
    assert_eq!(got.len(), before.len() + 3);
    assert_eq!(got[2], "  新的子項目");
    assert_eq!(&got[got.len() - 2..], ["第五欄", "  第五欄的項目"]);
    // Five header + list pairs.
    assert_eq!(shapes.len(), 10);
    let list = shapes.iter().find(|s| s.text.as_ref().is_some_and(|t| t.plain_text().contains("新的子項目"))).unwrap();
    assert_eq!(list.text.as_ref().unwrap().paragraphs.len(), 3);
}

#[test]
fn cycle_round_trips() {
    let mut pres = deck("test7.pptx");
    let (si, i, d) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "cycle2").unwrap();
    let n = d.rows().len();
    edit(&mut pres, si, i, |d| {
        let r = d.insert(n - 1, Insert::After).unwrap();
        d.set_text(r, "新");
    });
    let back = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let ShapeKind::Diagram { shapes, data: Some(d), .. } = &back.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), n + 1);
    assert_eq!(d.arrangement(), Arrangement::Cycle);
    assert_eq!(shapes.len(), 2 * (n + 1));
}

#[test]
fn copied_slide_keeps_its_drawing() {
    let mut pres = deck("test7.pptx");
    let (si, ..) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "cycle2").unwrap();
    let copy = pres.slides[si].clone();
    pres.slides.push(copy);
    let back = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let last = back.slides.last().unwrap();
    assert!(last.shapes.iter().any(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(_), shapes, .. } if !shapes.is_empty())));
}

fn org() -> (Presentation, usize, usize) {
    let pres = deck("test8.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "orgChart1").expect("an org chart");
    (pres, si, i)
}

/// (text, frame) of every box.
fn boxes(shapes: &[ppt_model::Shape]) -> Vec<(String, ppt_model::Xfrm)> {
    shapes.iter().filter_map(|s| Some((s.text.as_ref()?.plain_text(), s.xfrm))).collect()
}

/// The saved data model of the org chart on `slide` is consistent: unique ids,
/// every reference resolves, and the presentation points form one tree with a
/// box per node.
fn check_org_data(saved: &[u8], slide: usize, nodes: usize) {
    check_tree_data(saved, slide, nodes, "orgChart1", "ppt/diagrams/data2.xml", "rootText", "Name37");
}

/// [`check_org_data`] for any tree layout: its data part, and the start of the names of its box and connector points.
fn check_tree_data(saved: &[u8], slide: usize, nodes: usize, layout: &str, data_part: &str, box_name: &str, link_name: &str) {
    check_pres_data(saved, slide, nodes, layout, data_part, box_name, link_name, nodes - 1);
}

/// The saved data model is consistent, with `links` points named like `link_name`.
#[allow(clippy::too_many_arguments)]
fn check_pres_data(saved: &[u8], slide: usize, nodes: usize, layout: &str, data_part: &str, box_name: &str, link_name: &str, links: usize) {
    check_pres_counts(saved, slide, nodes, layout, data_part, (box_name, nodes), (link_name, links));
}

/// [`check_pres_data`] with the number of points named like `boxes.0` given.
fn check_pres_counts(saved: &[u8], slide: usize, nodes: usize, layout: &str, data_part: &str, boxes: (&str, usize), links: (&str, usize)) {
    let (box_name, box_count) = boxes;
    let (link_name, links) = links;
    use std::collections::{HashMap, HashSet};
    let pres = parse_pptx(saved).unwrap();
    let ShapeKind::Diagram { data: Some(d), .. } = &pres.slides[slide].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == layout)).unwrap().kind else { panic!() };
    assert_eq!(d.rows().len(), nodes);
    let xml = part(saved, data_part);
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let mut ids = HashSet::new();
    let mut kind: HashMap<&str, &str> = HashMap::new();
    let mut assoc: HashMap<&str, &str> = HashMap::new();
    let mut names: HashMap<&str, &str> = HashMap::new();
    for pt in doc.descendants().filter(|n| n.tag_name().name() == "pt" && n.attribute("modelId").is_some()) {
        let id = pt.attribute("modelId").unwrap();
        assert!(ids.insert(id), "duplicate point {id}");
        kind.insert(id, pt.attribute("type").unwrap_or("node"));
        if let Some(set) = pt.children().find(|c| c.tag_name().name() == "prSet") {
            if let Some(a) = set.attribute("presAssocID") {
                assoc.insert(id, a);
            }
            if let Some(n) = set.attribute("presName") {
                names.insert(id, n);
            }
        }
    }
    let mut cxn_ids = HashSet::new();
    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    for c in doc.descendants().filter(|n| n.tag_name().name() == "cxn") {
        assert!(cxn_ids.insert(c.attribute("modelId").unwrap()), "duplicate connection");
        let (s, t) = (c.attribute("srcId").unwrap(), c.attribute("destId").unwrap());
        assert!(ids.contains(s) && ids.contains(t), "dangling connection {s} → {t}");
        if c.attribute("type") == Some("presParOf") {
            assert!(parent_of.insert(t, s).is_none(), "{t} hangs under two points");
        }
    }
    for (p, a) in &assoc {
        assert!(ids.contains(a), "{p} presents a missing point");
    }
    // Every presentation point but the first hangs somewhere, and the chain ends at one root.
    let pres_pts: Vec<&str> = kind.iter().filter(|(_, k)| **k == "pres").map(|(i, _)| *i).collect();
    let roots: Vec<&&str> = pres_pts.iter().filter(|p| !parent_of.contains_key(**p)).collect();
    assert_eq!(roots.len(), 1, "one root point, got {}", roots.len());
    for p in &pres_pts {
        let mut at = *p;
        for _ in 0..100 {
            match parent_of.get(at) {
                Some(up) => at = up,
                None => break,
            }
        }
        assert_eq!(at, *roots[0], "{p} is not under the root");
    }
    let boxes = names.values().filter(|n| n.starts_with(box_name)).count();
    assert_eq!(boxes, box_count, "{box_name} points");
    let found = names.values().filter(|n| n.starts_with(link_name)).count();
    assert_eq!(found, links, "{link_name} points");
}

#[test]
fn org_chart_is_a_tree() {
    let (pres, si, i) = org();
    let ShapeKind::Diagram { data: Some(d), shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.arrangement(), Arrangement::Tree);
    assert!(d.can_restructure() && d.can_nest());
    assert_eq!(d.rows().len(), 11);
    assert_eq!(shapes.len(), 11 + 10, "a box per node and a connector per link");
    // One root: a node added beside it goes under it, and the departments cannot become roots.
    let mut one = (**d).clone();
    assert_eq!(one.insert(0, Insert::After), Some(11));
    assert_eq!(one.rows()[11].depth, 1);
    assert_eq!(one.insert(0, Insert::Before), Some(12));
    assert!(!one.shift(1, false));
    assert!(one.delete(0).is_none());
}

#[test]
fn org_chart_adds_and_removes_nodes() {
    let (mut pres, si, i) = org();
    edit(&mut pres, si, i, |d| {
        // A third department under the root, and a team under the last one.
        let r = d.insert(1, Insert::After).unwrap();
        d.set_text(r, "新部門");
        let last = d.rows().len() - 1;
        let kid = d.insert(last, Insert::After).unwrap();
        d.set_text(kid, "新團隊");
        // Remove the first department's last team.
        d.delete(5).unwrap();
    });
    let ShapeKind::Diagram { data: Some(d), shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), 12);
    let b = boxes(shapes);
    assert_eq!(b.len(), 12);
    let frame = |name: &str| b.iter().find(|(t, _)| t == name).unwrap().1;
    // The departments sit in one row, in order, under the root.
    let (a, new, c) = (frame("资讯共享旺智分中心"), frame("新部門"), frame("资讯共享旺能分中心"));
    assert!(a.y == new.y && new.y == c.y && a.x < new.x && new.x < c.x);
    assert!(new.y > frame("资讯中心").y);
    // Teams hang below their department.
    assert!(frame("新團隊").y > c.y);
    // Everything stays inside the frame.
    let ShapeKind::Diagram { w, h, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert!(b.iter().all(|(_, x)| x.x >= 0 && x.y >= 0 && x.x + x.w <= *w && x.y + x.h <= *h));
    let saved = save_pptx(&pres).unwrap();
    check_org_data(&saved, si, 12);
    // And reopens with the same tree.
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "orgChart1")).unwrap().kind else { panic!() };
    assert_eq!(texts(d2), texts(d));
    assert_eq!(s2.len(), shapes.len());
}

#[test]
fn org_chart_levels_follow_depth() {
    let (mut pres, si, i) = org();
    edit(&mut pres, si, i, |d| {
        // Demote the second department's last team under the previous one: a level deeper.
        let rows = d.rows();
        let team = rows.iter().rposition(|r| r.depth == 2).unwrap();
        assert!(d.shift(team, true));
        // And a new top-level department hanging nothing.
        let r = d.insert(1, Insert::Child).unwrap();
        d.set_text(r, "新的第一個");
    });
    let saved = save_pptx(&pres).unwrap();
    check_org_data(&saved, si, 12);
}

fn pyramid() -> (Presentation, usize, usize) {
    let pres = deck("test9.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "pyramid1").expect("a pyramid");
    (pres, si, i)
}

fn adj(s: &ppt_model::Shape) -> f64 {
    let ppt_model::Geometry::Preset { adjust, .. } = &s.geometry else { panic!("a preset trapezoid") };
    adjust.iter().find(|(k, _)| k == "adj").map_or(0.0, |a| a.1)
}

/// Levels share the height, widen by one step each, and each one's top edge
/// is the bottom edge of the one above.
fn check_pyramid(shapes: &[ppt_model::Shape], n: usize) {
    let levels: Vec<_> = shapes.iter().filter(|s| s.text.is_some()).collect();
    assert_eq!(levels.len(), n);
    let step = levels[0].xfrm.w;
    for (j, s) in levels.iter().enumerate() {
        assert!((s.xfrm.w - step * (j as i64 + 1)).abs() <= n as i64, "level {j} is {} wide, step {step}", s.xfrm.w);
        assert_eq!(s.xfrm.h, levels[0].xfrm.h);
        if j > 0 {
            assert!((s.xfrm.y - (levels[j - 1].xfrm.y + levels[j - 1].xfrm.h)).abs() <= 1);
            // The slant: inset per side is half a step.
            let inset = adj(s) * s.xfrm.w.min(s.xfrm.h) as f64 / 100_000.0;
            assert!((inset - step as f64 / 2.0).abs() <= step as f64 * 0.01, "level {j} inset {inset}");
        }
    }
    // The top level is a triangle.
    let top = levels[0];
    assert!((adj(top) * top.xfrm.w.min(top.xfrm.h) as f64 / 100_000.0 - top.xfrm.w as f64 / 2.0).abs() <= top.xfrm.w as f64 * 0.01);
}

#[test]
fn pyramid_levels_follow_the_count() {
    let (mut pres, si, i) = pyramid();
    let ShapeKind::Diagram { data: Some(d), shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert!(d.template.0.tapered && d.can_restructure());
    check_pyramid(shapes, 4);
    edit(&mut pres, si, i, |d| {
        let r = d.insert(3, Insert::After).unwrap();
        d.set_text(r, "新層");
        let r = d.insert(0, Insert::Before).unwrap();
        d.set_text(r, "頂");
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    check_pyramid(shapes, 6);
    // Its bottom stays as wide as it was, and inside the frame.
    let ShapeKind::Diagram { w, h, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert!(shapes.iter().all(|s| s.xfrm.x >= 0 && s.xfrm.y >= 0 && s.xfrm.x + s.xfrm.w <= *w && s.xfrm.y + s.xfrm.h <= *h));
    // Saved and reopened, still a pyramid of six.
    let back = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "pyramid1")).unwrap().kind else { panic!() };
    assert_eq!(d2.rows().len(), 6);
    check_pyramid(s2, 6);
    // And with a level removed.
    edit(&mut pres, si, i, |d| {
        d.delete(0).unwrap();
        d.delete(0).unwrap();
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    check_pyramid(shapes, 4);
}

/// A deck that is in the repository, with a new SmartArt of `kind` added to its first slide.
fn with_new_smartart(kind: &str) -> Presentation {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let mut pres = parse_pptx(&bytes).unwrap();
    let labels = ppt_ooxml::Labels { root: "標題", top: "項目 {}", child: "子項目 {}" };
    let (mut shape, package) = ppt_ooxml::new_smartart(&pres, kind, (pres.slide_width, pres.slide_height), &labels).unwrap();
    shape.id = pres.alloc_id();
    pres.package = Some(package);
    pres.slides[0].shapes.push(shape);
    pres
}

fn new_diagram<'a>(pres: &'a Presentation, kind: &str) -> (&'a ppt_model::Shape, &'a Diagram) {
    pres.slides[0]
        .shapes
        .iter()
        .find_map(|s| match &s.kind {
            ShapeKind::Diagram { data: Some(d), .. } if layout_name(d).split('#').next() == Some(kind) => Some((s, d.as_ref())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a {kind} SmartArt"))
}

#[test]
fn new_smartarts_of_every_layout_save_and_reopen() {
    for kind in ppt_ooxml::smartart_kinds() {
        let pres = with_new_smartart(kind);
        let (shape, d) = new_diagram(&pres, kind);
        let ShapeKind::Diagram { shapes, w, h, .. } = &shape.kind else { panic!() };
        assert!(d.can_restructure(), "{kind}");
        if matches!(kind, "orgChart1" | "hierarchy1" | "hierarchy2") {
            assert_eq!(d.arrangement(), Arrangement::Tree, "{kind}");
        }
        let rows = texts(d);
        assert!(rows.len() >= 3, "{kind}: {rows:?}");
        assert!(rows.iter().all(|r| r.trim().starts_with("項目") || r.trim().starts_with("子項目") || r.trim().starts_with("標題")), "{kind}: {rows:?}");
        // Drawn in its own frame, which is on the slide.
        assert_eq!((*w, *h), (shape.xfrm.w, shape.xfrm.h));
        assert!(shapes.iter().any(|s| s.text.as_ref().is_some_and(|t| !t.is_empty())), "{kind}");
        // Shapes turned a quarter keep their unturned frame: compare their turned extent.
        assert!(
            shapes.iter().all(|s| {
                let x = &s.xfrm;
                let (cx, cy) = (x.x + x.w / 2, x.y + x.h / 2);
                let (hw, hh) = if (x.rot - 90.0).abs() < 1.0 || (x.rot - 270.0).abs() < 1.0 { (x.h / 2, x.w / 2) } else { (x.w / 2, x.h / 2) };
                cx - hw >= -2 && cy - hh >= -2 && cx + hw <= *w + 2 && cy + hh <= *h + 2
            }),
            "{kind}: inside its frame"
        );
        assert!(shape.xfrm.x >= 0 && shape.xfrm.x + shape.xfrm.w <= pres.slide_width && shape.xfrm.y >= 0 && shape.xfrm.y + shape.xfrm.h <= pres.slide_height, "{kind}: on the slide");

        // Saved, it is a real SmartArt: five parts, related and typed.
        let saved = save_pptx(&pres).unwrap();
        let zip = zip::ZipArchive::new(Cursor::new(saved.as_slice())).unwrap();
        let names: Vec<String> = zip.file_names().filter_map(Result::ok).map(|n| n.into_owned()).collect();
        for p in ["data", "layout", "quickStyle", "colors", "drawing"] {
            assert!(names.iter().any(|n| n.starts_with(&format!("ppt/diagrams/{p}")) && n.ends_with(".xml")), "{kind}: {p}");
        }
        let types = part(&saved, "[Content_Types].xml");
        assert!(types.contains("diagramData+xml") && types.contains("diagramLayout+xml") && types.contains("diagramStyle+xml") && types.contains("diagramColors+xml") && types.contains("diagramDrawing+xml"), "{kind}");
        drop(zip);
        let back = parse_pptx(&saved).unwrap();
        let (_, d2) = new_diagram(&back, kind);
        assert_eq!(texts(d2), rows, "{kind}");

        // And it edits like any other: add a node, save, reopen.
        let mut pres = back;
        let i = pres.slides[0].shapes.iter().position(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d).split('#').next() == Some(kind))).unwrap();
        edit(&mut pres, 0, i, |d| {
            let r = d.insert(0, Insert::Child).or_else(|| d.insert(0, Insert::After)).unwrap();
            d.set_text(r, "新增");
        });
        let again = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
        let (_, d3) = new_diagram(&again, kind);
        assert_eq!(d3.rows().len(), rows.len() + 1, "{kind}");
        assert!(texts(d3).iter().any(|r| r.trim() == "新增"), "{kind}");
    }
}

#[test]
fn two_new_smartarts_get_parts_of_their_own() {
    let mut pres = with_new_smartart("vList2");
    let labels = ppt_ooxml::Labels { root: "標題", top: "項目 {}", child: "子項目 {}" };
    let (mut shape, package) = ppt_ooxml::new_smartart(&pres, "vList2", (pres.slide_width, pres.slide_height), &labels).unwrap();
    shape.id = pres.alloc_id();
    pres.package = Some(package);
    pres.slides[0].shapes.push(shape);
    let saved = save_pptx(&pres).unwrap();
    let zip = zip::ZipArchive::new(Cursor::new(saved.as_slice())).unwrap();
    let data: Vec<String> = zip.file_names().filter_map(Result::ok).map(|n| n.into_owned()).filter(|n| n.starts_with("ppt/diagrams/data")).collect();
    assert_eq!(data.len(), 2, "{data:?}");
    let back = parse_pptx(&saved).unwrap();
    assert_eq!(back.slides[0].shapes.iter().filter(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(_), .. })).count(), 2);
}


fn hierarchy() -> (Presentation, usize, usize) {
    let pres = deck("test10.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "hierarchy1").expect("a hierarchy");
    (pres, si, i)
}

#[test]
fn hierarchy_is_a_tree_of_shape_groups() {
    let (pres, si, i) = hierarchy();
    let ShapeKind::Diagram { data: Some(d), shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.arrangement(), Arrangement::Tree);
    assert_eq!(d.rows().len(), 12);
    // A box with its shadow per node, and a connector per link.
    assert_eq!(shapes.len(), 12 * 2 + 11);
}

#[test]
fn hierarchy_adds_and_removes_nodes() {
    let (mut pres, si, i) = hierarchy();
    edit(&mut pres, si, i, |d| {
        // Another document under the first department, one fewer under the second.
        let r = d.insert(1, Insert::Child).unwrap();
        d.set_text(r, "新文件");
        let last = d.rows().len() - 1;
        d.delete(last).unwrap();
    });
    let ShapeKind::Diagram { data: Some(d), shapes, w, h } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), 12);
    let b = boxes(shapes);
    let frame = |name: &str| b.iter().find(|(t, _)| t == name).unwrap().1;
    // The new document sits in the row of its siblings, below its department.
    let (new, old) = (frame("新文件"), frame("服务启停"));
    assert_eq!(new.y, old.y);
    assert!(new.y > frame("快速手册").y);
    assert!(b.iter().all(|(_, x)| x.x >= 0 && x.y >= 0 && x.x + x.w <= *w && x.y + x.h <= *h));
    // Every shape still has its shadow with it: a text shape sits inside its shadow's frame, offset as before.
    let saved = save_pptx(&pres).unwrap();
    check_tree_data(&saved, si, 12, "hierarchy1", "ppt/diagrams/data2.xml", "text", "Name");
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "hierarchy1")).unwrap().kind else { panic!() };
    assert_eq!(texts(d2), texts(d));
    assert_eq!(s2.len(), shapes.len());
}

fn horizontal() -> (Presentation, usize, usize) {
    let pres = deck("test11.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "hierarchy2").expect("a horizontal hierarchy");
    (pres, si, i)
}

/// Both ends of a straight connector: from its center, length and turn.
fn line_ends(s: &ppt_model::Shape) -> ((f64, f64), (f64, f64)) {
    let x = &s.xfrm;
    let (cx, cy) = (x.x as f64 + x.w as f64 / 2.0, x.y as f64 + x.h as f64 / 2.0);
    let (sin, cos) = x.rot.to_radians().sin_cos();
    let half = x.w as f64 / 2.0;
    ((cx - half * cos, cy - half * sin), (cx + half * cos, cy + half * sin))
}

#[test]
fn horizontal_hierarchy_adds_and_removes_nodes() {
    let (mut pres, si, i) = horizontal();
    let ShapeKind::Diagram { data: Some(d), shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.arrangement(), Arrangement::Tree);
    assert_eq!(d.rows().len(), 14);
    assert_eq!(shapes.len(), 14 + 13);
    let before = texts(d);
    edit(&mut pres, si, i, |d| {
        // A new top-level topic, and a sub-topic under the first one.
        let r = d.insert(0, Insert::Child).unwrap();
        d.set_text(r, "新主題");
        let kid = d.insert(1, Insert::Child).unwrap();
        d.set_text(kid, "新子題");
        d.delete(3).unwrap();
    });
    let ShapeKind::Diagram { data: Some(d), shapes, w, h } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), 15);
    assert_eq!(shapes.len(), 15 + 14);
    let b = boxes(shapes);
    let frame = |name: &str| b.iter().find(|(t, _)| t == name).unwrap().1;
    // Children are in a column to the right of their parent, new topics at the bottom of the column.
    let (root, topic, kid) = (frame(&before[0]), frame("新主題"), frame("新子題"));
    assert!(topic.x > root.x + root.w && kid.x > topic.x - 1);
    assert!(topic.y > root.y, "the new top-level topic is below the root's middle");
    assert!(b.iter().all(|(_, x)| x.x >= 0 && x.y >= 0 && x.x + x.w <= *w && x.y + x.h <= *h));
    // Each connector joins a parent's right edge to a child's left edge.
    let lines: Vec<_> = shapes.iter().filter(|s| s.xfrm.h < 100_000 && s.xfrm.w > s.xfrm.h * 3).collect();
    let boxes_: Vec<_> = shapes.iter().filter(|s| s.xfrm.h >= 100_000).collect();
    for l in lines {
        let (a, z) = line_ends(l);
        let near = |p: (f64, f64), edge: &dyn Fn(&ppt_model::Xfrm) -> f64| boxes_.iter().any(|s| (edge(&s.xfrm) - p.0).abs() <= 3.0 && (s.xfrm.y as f64 + s.xfrm.h as f64 / 2.0 - p.1).abs() <= 3.0);
        assert!(near(a, &|x| (x.x + x.w) as f64), "starts at a box's right edge: {a:?}");
        assert!(near(z, &|x| x.x as f64), "ends at a box's left edge: {z:?}");
    }
    let saved = save_pptx(&pres).unwrap();
    check_tree_data(&saved, si, 15, "hierarchy2", "ppt/diagrams/data12.xml", "Level", "connTx");
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "hierarchy2")).unwrap().kind else { panic!() };
    assert_eq!(texts(d2), texts(d));
    assert_eq!(s2.len(), shapes.len());
}


fn cycle3() -> (Presentation, usize, usize) {
    let pres = deck("test12.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "cycle3").expect("a continuous cycle");
    (pres, si, i)
}

/// (center, share of the way around from the first node) of nodes and the ring arrow's (tail, tip) in node spacings.
fn ring_of(shapes: &[ppt_model::Shape], nodes: usize) -> (Vec<(f64, f64)>, (f64, f64)) {
    let arrow = shapes.iter().find(|s| matches!(&s.geometry, ppt_model::Geometry::Preset { name, .. } if name == "circularArrow")).expect("the ring arrow");
    let centers: Vec<(f64, f64)> = shapes.iter().filter(|s| s.text.is_some()).map(|s| (s.xfrm.x as f64 + s.xfrm.w as f64 / 2.0, s.xfrm.y as f64 + s.xfrm.h as f64 / 2.0)).collect();
    assert_eq!(centers.len(), nodes);
    let c = (centers.iter().map(|p| p.0).sum::<f64>() / nodes as f64, centers.iter().map(|p| p.1).sum::<f64>() / nodes as f64);
    let first = (centers[0].1 - c.1).atan2(centers[0].0 - c.0).to_degrees();
    let ppt_model::Geometry::Preset { adjust, .. } = &arrow.geometry else { unreachable!() };
    let adj = |n: &str| adjust.iter().find(|(k, _)| k == n).unwrap().1 / 60_000.0;
    let step = 360.0 / nodes as f64;
    ((centers), (((adj("adj4") - first).rem_euclid(360.0)) / step, ((adj("adj3") - first + 180.0).rem_euclid(360.0) - 180.0) / step))
}

#[test]
fn continuous_cycle_respaces_nodes_and_its_arrow() {
    let (mut pres, si, i) = cycle3();
    let ShapeKind::Diagram { data: Some(d), shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.arrangement(), Arrangement::Cycle);
    assert_eq!(d.rows().len(), 5);
    let (_, (tail, tip)) = ring_of(shapes, 5);
    edit(&mut pres, si, i, |d| {
        let r = d.insert(4, Insert::After).unwrap();
        d.set_text(r, "新階段");
        let r = d.insert(1, Insert::After).unwrap();
        d.set_text(r, "又一個");
    });
    let ShapeKind::Diagram { data: Some(d), shapes, w, h } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), 7);
    let (centers, (tail7, tip7)) = ring_of(shapes, 7);
    // The nodes sit on one circle, evenly spaced.
    let arrow = shapes.iter().find(|s| matches!(&s.geometry, ppt_model::Geometry::Preset { name, .. } if name == "circularArrow")).unwrap();
    let c = (centers.iter().map(|p| p.0).sum::<f64>() / 7.0, centers.iter().map(|p| p.1).sum::<f64>() / 7.0);
    let _ = arrow;
    let radii: Vec<f64> = centers.iter().map(|p| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt()).collect();
    assert!(radii.iter().all(|r| (r - radii[0]).abs() <= radii[0] * 0.01), "{radii:?}");
    // The arrow starts and ends the same share of a node spacing from the first node, and is one shape.
    assert!((tail7 - tail).abs() < 0.01 && (tip7 - tip).abs() < 0.01, "{tail} {tip} vs {tail7} {tip7}");
    assert_eq!(shapes.iter().filter(|s| matches!(&s.geometry, ppt_model::Geometry::Preset { name, .. } if name == "circularArrow")).count(), 1);
    assert!(shapes.iter().all(|s| s.xfrm.x >= -2 && s.xfrm.y >= -2 && s.xfrm.x + s.xfrm.w <= *w + 2 && s.xfrm.y + s.xfrm.h <= *h + 2));
    let saved = save_pptx(&pres).unwrap();
    check_pres_data(&saved, si, 7, "cycle3", "ppt/diagrams/data8.xml", "node", "sibTrans", 1);
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "cycle3")).unwrap().kind else { panic!() };
    assert_eq!(texts(d2), texts(d));
    assert_eq!(s2.len(), shapes.len());
    assert_eq!(d2.arrangement(), Arrangement::Cycle);
    // Removing the first node: the next one leads, and the arrow follows it.
    edit(&mut pres, si, i, |d| {
        d.delete(0).unwrap();
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(ring_of(shapes, 6).0.len(), 6);
    let saved = save_pptx(&pres).unwrap();
    check_pres_data(&saved, si, 6, "cycle3", "ppt/diagrams/data8.xml", "node", "sibTrans", 1);
}

fn cycle5() -> (Presentation, usize, usize) {
    let pres = deck("test13.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "cycle5").expect("a block cycle");
    (pres, si, i)
}

/// (start, end) of an arc shape's path in the drawing's coordinates.
fn arc_ends(s: &ppt_model::Shape) -> ((f64, f64), (f64, f64)) {
    let ppt_model::Geometry::Custom(paths) = &s.geometry else { panic!("an arc") };
    let cmds = &paths[0].cmds;
    let ppt_model::PathCmd::MoveTo(x0, y0) = cmds[0] else { panic!() };
    let ppt_model::PathCmd::CubicTo(.., x1, y1) = *cmds.last().unwrap() else { panic!() };
    let scale = if paths[0].w > 0.0 { s.xfrm.w as f64 / paths[0].w } else { 1.0 };
    ((s.xfrm.x as f64 + x0 * scale, s.xfrm.y as f64 + y0 * scale), (s.xfrm.x as f64 + x1 * scale, s.xfrm.y as f64 + y1 * scale))
}

#[test]
fn block_cycle_gets_an_arc_between_each_pair_of_nodes() {
    let (mut pres, si, i) = cycle5();
    let ShapeKind::Diagram { data: Some(d), shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.arrangement(), Arrangement::Cycle);
    assert_eq!(shapes.len(), 5 + 5);
    edit(&mut pres, si, i, |d| {
        let r = d.insert(4, Insert::After).unwrap();
        d.set_text(r, "新階段");
        let r = d.insert(0, Insert::After).unwrap();
        d.set_text(r, "又一個");
    });
    let ShapeKind::Diagram { data: Some(d), shapes, w, h } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), 7);
    assert_eq!(shapes.len(), 7 + 7);
    let nodes: Vec<&ppt_model::Shape> = shapes.iter().filter(|s| s.text.is_some()).collect();
    let arcs: Vec<&ppt_model::Shape> = shapes.iter().filter(|s| s.text.is_none()).collect();
    let center = |s: &ppt_model::Shape| (s.xfrm.x as f64 + s.xfrm.w as f64 / 2.0, s.xfrm.y as f64 + s.xfrm.h as f64 / 2.0);
    let c = (nodes.iter().map(|s| center(s).0).sum::<f64>() / 7.0, nodes.iter().map(|s| center(s).1).sum::<f64>() / 7.0);
    let radius = ((center(nodes[0]).0 - c.0).powi(2) + (center(nodes[0]).1 - c.1).powi(2)).sqrt();
    let angle = |p: (f64, f64)| (p.1 - c.1).atan2(p.0 - c.0).to_degrees().rem_euclid(360.0);
    for (k, arc) in arcs.iter().enumerate() {
        let (s, e) = arc_ends(arc);
        // On the ring, going clockwise from just after node k to just before the next one.
        for p in [s, e] {
            let dist = ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt();
            assert!((dist - radius).abs() <= radius * 0.01, "arc {k} off the ring: {dist} vs {radius}");
        }
        let from = angle(center(nodes[k]));
        let (a, b) = ((angle(s) - from).rem_euclid(360.0), (angle(e) - from).rem_euclid(360.0));
        let spacing = 360.0 / 7.0;
        assert!(a > 0.0 && a < b && b < spacing, "arc {k} runs {a:.1}..{b:.1} of the {spacing:.1} spacing");
    }
    assert!(shapes.iter().all(|s| s.xfrm.x >= -2 && s.xfrm.y >= -2 && s.xfrm.x + s.xfrm.w <= *w + 2 && s.xfrm.y + s.xfrm.h <= *h + 2));
    let saved = save_pptx(&pres).unwrap();
    check_pres_data(&saved, si, 7, "cycle5", "ppt/diagrams/data4.xml", "node", "sibTrans", 7);
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "cycle5")).unwrap().kind else { panic!() };
    assert_eq!(texts(d2), texts(d));
    assert_eq!(s2.len(), shapes.len());
    assert_eq!(d2.arrangement(), Arrangement::Cycle);
    // And fewer nodes: still one arc each.
    edit(&mut pres, si, i, |d| {
        d.delete(0).unwrap();
        d.delete(0).unwrap();
        d.delete(0).unwrap();
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(shapes.len(), 4 + 4);
}

fn cycle8() -> (Presentation, usize, usize) {
    let pres = deck("test14.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "cycle8").expect("a pie cycle");
    (pres, si, i)
}

fn pie_angles(s: &ppt_model::Shape) -> (f64, f64) {
    let ppt_model::Geometry::Preset { adjust, .. } = &s.geometry else { panic!("a pie") };
    let adj = |n: &str| adjust.iter().find(|(k, _)| k == n).unwrap().1 / 60_000.0;
    (adj("adj1"), adj("adj2"))
}

#[test]
fn pie_cycle_relayout_reproduces_the_loaded_drawing() {
    let (mut pres, si, i) = cycle8();
    let ShapeKind::Diagram { data: Some(d), shapes: loaded, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.arrangement(), Arrangement::Pie);
    assert_eq!(loaded.len(), 10);
    let loaded = loaded.clone();
    // The same five slices with another last one: laid out again, they land where they were.
    edit(&mut pres, si, i, |d| {
        let r = d.insert(4, Insert::After).unwrap();
        d.set_text(r, "新的");
        d.delete(4).unwrap();
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(shapes.len(), 10);
    for (new, old) in shapes.iter().zip(loaded.iter()) {
        for (a, b) in [(new.xfrm.x, old.xfrm.x), (new.xfrm.y, old.xfrm.y), (new.xfrm.w, old.xfrm.w), (new.xfrm.h, old.xfrm.h)] {
            assert!((a - b).abs() <= 2_000, "{:?} vs {:?}", new.xfrm, old.xfrm);
        }
    }
    for (new, old) in shapes.iter().zip(loaded.iter()).take(5) {
        let (a, b) = (pie_angles(new), pie_angles(old));
        assert!((a.0 - b.0).abs() < 0.1 && (a.1 - b.1).abs() < 0.1, "{a:?} vs {b:?}");
        // The text rectangle is where the loaded one was, within a tenth of the slice.
        let (n, o) = (new.text.as_ref().unwrap().insets, old.text.as_ref().unwrap().insets);
        for s in 0..4 {
            assert!((n[s] - o[s]).abs() as f64 <= new.xfrm.w as f64 * 0.1, "inset {s}: {n:?} vs {o:?}");
        }
    }
}

#[test]
fn pie_cycle_adds_and_removes_nodes() {
    let (mut pres, si, i) = cycle8();
    edit(&mut pres, si, i, |d| {
        let r = d.insert(4, Insert::After).unwrap();
        d.set_text(r, "新階段");
        let r = d.insert(1, Insert::After).unwrap();
        d.set_text(r, "又一個");
    });
    let ShapeKind::Diagram { data: Some(d), shapes, w, h } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), 7);
    assert_eq!(shapes.len(), 14);
    // Seven slices share the circle: each sweeps a seventh, one after another.
    let slices: Vec<&ppt_model::Shape> = shapes.iter().filter(|s| matches!(&s.geometry, ppt_model::Geometry::Preset { name, .. } if name == "pie")).collect();
    assert_eq!(slices.len(), 7);
    for (k, s) in slices.iter().enumerate() {
        let (a, b) = pie_angles(s);
        let sweep = (b - a).rem_euclid(360.0);
        assert!((sweep - 360.0 / 7.0).abs() < 0.01, "slice {k} sweeps {sweep}");
        if k > 0 {
            assert!((a - pie_angles(slices[k - 1]).1).abs() < 0.01);
        }
        assert!(s.text.as_ref().is_some_and(|t| !t.is_empty()));
    }
    assert!(shapes.iter().all(|s| s.xfrm.x >= -2 && s.xfrm.y >= -2 && s.xfrm.x + s.xfrm.w <= *w + 2 && s.xfrm.y + s.xfrm.h <= *h + 2));
    let saved = save_pptx(&pres).unwrap();
    // A slice is four points; arrows are named like the arrow points, dummies come two to a slice.
    check_pres_data(&saved, si, 7, "cycle8", "ppt/diagrams/data3.xml", "arrowWedge", "dummy", 14);
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "cycle8")).unwrap().kind else { panic!() };
    assert_eq!(texts(d2), texts(d));
    assert_eq!(s2.len(), shapes.len());
    assert_eq!(d2.arrangement(), Arrangement::Pie);
    // The saved text rectangles read back as the same insets.
    for (a, b) in s2.iter().zip(shapes.iter()).filter(|(_, b)| b.text.is_some()) {
        let (x, y) = (a.text.as_ref().unwrap().insets, b.text.as_ref().unwrap().insets);
        for s in 0..4 {
            assert!((x[s] - y[s]).abs() <= 3_000, "inset {s}: {x:?} vs {y:?}");
        }
    }
    // Fewer: three slices, a third each.
    edit(&mut pres, si, i, |d| {
        for _ in 0..4 {
            d.delete(0).unwrap();
        }
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(shapes.len(), 6);
    let (a, b) = pie_angles(shapes.iter().find(|s| matches!(&s.geometry, ppt_model::Geometry::Preset { name, .. } if name == "pie")).unwrap());
    assert!(((b - a).rem_euclid(360.0) - 120.0).abs() < 0.01);
}

fn radial() -> (Presentation, usize, usize) {
    let pres = deck("test15.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "radial2").expect("a radial list");
    (pres, si, i)
}

#[test]
fn radial_list_relayout_reproduces_the_loaded_drawing() {
    let (mut pres, si, i) = radial();
    let ShapeKind::Diagram { data: Some(d), shapes: loaded, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.arrangement(), Arrangement::Radial);
    assert!(d.can_nest());
    assert_eq!(loaded.len(), 1 + 3 * 3);
    let loaded = loaded.clone();
    // The same three nodes with another last one: laid out again, they land where they were.
    edit(&mut pres, si, i, |d| {
        let r = d.insert(4, Insert::After).unwrap();
        d.set_text(r, "新的");
        d.delete(4).unwrap();
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(shapes.len(), loaded.len());
    // Every loaded shape has one where it was (the order of the lines differs).
    for old in loaded.iter() {
        let near = |a: i64, b: i64| (a - b).abs() <= 6_000;
        assert!(shapes.iter().any(|new| near(new.xfrm.x, old.xfrm.x) && near(new.xfrm.y, old.xfrm.y) && near(new.xfrm.w, old.xfrm.w) && near(new.xfrm.h, old.xfrm.h) && (new.xfrm.rot - old.xfrm.rot + 180.0).rem_euclid(360.0) - 180.0 < 0.3 && (new.xfrm.rot - old.xfrm.rot + 180.0).rem_euclid(360.0) - 180.0 > -0.3), "nothing like {:?}", old.xfrm);
    }
}

#[test]
fn radial_list_adds_and_removes_nodes() {
    let (mut pres, si, i) = radial();
    edit(&mut pres, si, i, |d| {
        let r = d.insert(0, Insert::After).unwrap();
        d.set_text(r, "新響應");
        let kid = d.insert(r, Insert::Child).unwrap();
        d.set_text(kid, "新說明");
    });
    let ShapeKind::Diagram { data: Some(d), shapes, w, h } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), 8);
    assert_eq!(shapes.len(), 1 + 3 * 4);
    // Four nodes on an arc (from 45 to 135 degrees): the first is highest, then they go down, and none leaves the frame.
    let titles: Vec<&ppt_model::Shape> = shapes.iter().filter(|s| matches!(&s.geometry, ppt_model::Geometry::Preset { name, .. } if name == "ellipse")).collect();
    assert_eq!(titles.len(), 4);
    assert!(titles.windows(2).all(|p| p[0].xfrm.y < p[1].xfrm.y));
    assert!(shapes.iter().filter(|s| s.xfrm.rot == 0.0).all(|s| s.xfrm.x >= -2 && s.xfrm.y >= -2 && s.xfrm.x + s.xfrm.w <= *w + 2 && s.xfrm.y + s.xfrm.h <= *h + 2));
    // The new node's box shows its child's text.
    assert!(shapes.iter().any(|s| s.text.as_ref().is_some_and(|t| t.plain_text() == "新說明")));
    let saved = save_pptx(&pres).unwrap();
    check_pres_counts(&saved, si, 8, "radial2", "ppt/diagrams/data6.xml", ("parentNode", 4), ("Name25", 4));
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "radial2")).unwrap().kind else { panic!() };
    assert_eq!(texts(d2), texts(d));
    assert_eq!(s2.len(), shapes.len());
    assert_eq!(d2.arrangement(), Arrangement::Radial);
    edit(&mut pres, si, i, |d| {
        d.delete(0).unwrap();
        d.delete(0).unwrap();
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(shapes.len(), 1 + 3 * 2);
}

fn stair() -> (Presentation, usize, usize) {
    let pres = deck("test16.pptx");
    let (si, i, _) = diagrams(&pres).into_iter().find(|(.., d)| layout_name(d) == "StepDownProcess").expect("a step-down process");
    (pres, si, i)
}

#[test]
fn step_down_relayout_reproduces_the_loaded_drawing() {
    let (mut pres, si, i) = stair();
    let ShapeKind::Diagram { data: Some(d), shapes: loaded, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.arrangement(), Arrangement::Stair);
    assert_eq!(loaded.len(), 3 * 3 + 1);
    let loaded = loaded.clone();
    // The same four steps with another last one: laid out again, they land where they were.
    edit(&mut pres, si, i, |d| {
        let r = d.insert(3, Insert::After).unwrap();
        d.set_text(r, "新的");
        d.delete(3).unwrap();
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(shapes.len(), loaded.len());
    for (new, old) in shapes.iter().zip(loaded.iter()) {
        // Within a hundredth of the frame: the loaded drawing is a hair off its own fit.
        for (a, b) in [(new.xfrm.x, old.xfrm.x), (new.xfrm.y, old.xfrm.y), (new.xfrm.w, old.xfrm.w), (new.xfrm.h, old.xfrm.h)] {
            assert!((a - b).abs() <= 6_000, "{:?} vs {:?}", new.xfrm, old.xfrm);
        }
        assert_eq!(new.xfrm.rot, old.xfrm.rot);
    }
}

#[test]
fn step_down_adds_and_removes_steps() {
    let (mut pres, si, i) = stair();
    edit(&mut pres, si, i, |d| {
        let r = d.insert(1, Insert::After).unwrap();
        d.set_text(r, "新一步");
        let r = d.insert(4, Insert::After).unwrap();
        d.set_text(r, "又一步");
    });
    let ShapeKind::Diagram { data: Some(d), shapes, w, h } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert_eq!(d.rows().len(), 6);
    // Titles come down and to the right, one step after another.
    let titles: Vec<&ppt_model::Shape> = shapes.iter().filter(|s| s.text.as_ref().is_some_and(|t| !t.is_empty())).collect();
    assert_eq!(titles.len(), 6);
    assert!(titles.windows(2).all(|p| p[1].xfrm.x > p[0].xfrm.x && p[1].xfrm.y > p[0].xfrm.y));
    // Even steps, and all of it inside the frame.
    let dx: Vec<i64> = titles.windows(2).map(|p| p[1].xfrm.x - p[0].xfrm.x).collect();
    assert!(dx.iter().all(|d| (d - dx[0]).abs() <= 2));
    assert!(shapes.iter().all(|s| s.xfrm.x >= -2 && s.xfrm.y >= -2 && s.xfrm.x + s.xfrm.w <= *w + 2 && s.xfrm.y + s.xfrm.h <= *h + 2 || s.xfrm.rot != 0.0));
    let saved = save_pptx(&pres).unwrap();
    check_pres_data(&saved, si, 6, "StepDownProcess", "ppt/diagrams/data1.xml", "ParentText", "sibTrans", 5);
    let back = parse_pptx(&saved).unwrap();
    let ShapeKind::Diagram { data: Some(d2), shapes: s2, .. } = &back.slides[si].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "StepDownProcess")).unwrap().kind else { panic!() };
    assert_eq!(texts(d2), texts(d));
    assert_eq!(s2.len(), shapes.len());
    assert_eq!(d2.arrangement(), Arrangement::Stair);
    // Fewer steps: bigger ones, to fill the frame.
    let before = shapes.iter().find(|s| s.text.as_ref().is_some_and(|t| !t.is_empty())).unwrap().xfrm.w;
    edit(&mut pres, si, i, |d| {
        for _ in 0..3 {
            d.delete(0).unwrap();
        }
    });
    let ShapeKind::Diagram { shapes, .. } = &pres.slides[si].shapes[i].kind else { panic!() };
    assert!(shapes.iter().find(|s| s.text.as_ref().is_some_and(|t| !t.is_empty())).unwrap().xfrm.w > before);
}
