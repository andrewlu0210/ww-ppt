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
