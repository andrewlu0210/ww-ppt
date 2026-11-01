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
    use std::collections::{HashMap, HashSet};
    let pres = parse_pptx(saved).unwrap();
    let ShapeKind::Diagram { data: Some(d), .. } = &pres.slides[slide].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == "orgChart1")).unwrap().kind else { panic!() };
    assert_eq!(d.rows().len(), nodes);
    let xml = part(saved, "ppt/diagrams/data2.xml");
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
    let boxes = names.values().filter(|n| n.starts_with("rootText")).count();
    assert_eq!(boxes, nodes, "one box per node");
    let links = names.values().filter(|n| **n == "Name37").count();
    assert_eq!(links, nodes - 1, "one connector per link");
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
            ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == kind => Some((s, d.as_ref())),
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
        let rows = texts(d);
        assert!(rows.len() >= 3, "{kind}: {rows:?}");
        assert!(rows.iter().all(|r| r.trim().starts_with("項目") || r.trim().starts_with("子項目") || r.trim().starts_with("標題")), "{kind}: {rows:?}");
        // Drawn in its own frame, which is on the slide.
        assert_eq!((*w, *h), (shape.xfrm.w, shape.xfrm.h));
        assert!(shapes.iter().any(|s| s.text.as_ref().is_some_and(|t| !t.is_empty())), "{kind}");
        assert!(shapes.iter().all(|s| s.xfrm.x >= -2 && s.xfrm.y >= -2 && s.xfrm.x + s.xfrm.w <= *w + 2 && s.xfrm.y + s.xfrm.h <= *h + 2), "{kind}: inside its frame");
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
        let i = pres.slides[0].shapes.iter().position(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(d), .. } if layout_name(d) == kind)).unwrap();
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
