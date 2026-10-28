//! Save → reopen round trips against the sample deck and the built-in template.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use ppt_model::{Color, Geometry, Line, Presentation, Shape, ShapeKind, TextBody, Xfrm};
use ppt_ooxml::{parse_pptx, save_pptx, template_package};

fn sample() -> Presentation {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx");
    parse_pptx(&std::fs::read(path).unwrap()).unwrap()
}

fn parts(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut f = zip.by_index(i).unwrap();
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).unwrap();
            (f.name().unwrap().into_owned(), buf)
        })
        .collect()
}

fn texts(pres: &Presentation, slide: usize) -> Vec<String> {
    pres.slides[slide].shapes.iter().filter_map(|s| s.text.as_ref()).map(|t| t.plain_text()).collect()
}

#[test]
fn unchanged_deck_saves_identical_parts() {
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let saved = save_pptx(&parse_pptx(&original).unwrap()).unwrap();
    let (a, b) = (parts(&original), parts(&saved));
    assert_eq!(a.keys().collect::<std::collections::BTreeSet<_>>(), b.keys().collect());
    for (name, bytes) in &a {
        // The three bookkeeping parts are rewritten, everything else is copied.
        if matches!(name.as_str(), "[Content_Types].xml" | "ppt/presentation.xml" | "ppt/_rels/presentation.xml.rels") {
            continue;
        }
        assert_eq!(bytes, &b[name], "{name} changed");
    }
    let reparsed = parse_pptx(&saved).unwrap();
    assert_eq!(reparsed.slides.len(), 4);
    for i in 0..4 {
        assert_eq!(texts(&reparsed, i), texts(&parse_pptx(&original).unwrap(), i));
    }
}

#[test]
fn edited_text_keeps_other_formatting_and_slides() {
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let mut pres = parse_pptx(&original).unwrap();
    // Slide 2, content placeholder: change the first paragraph's text.
    let body = pres.slides[1].shapes[1].text.as_mut().unwrap();
    let end = body.paragraph_ranges()[0].end;
    body.delete(0..end);
    let style = body.style_at(0);
    body.insert(0, "新的第一段 & <特殊字元>", &style);
    let saved = save_pptx(&pres).unwrap();

    let (a, b) = (parts(&original), parts(&saved));
    assert_eq!(a["ppt/slides/slide1.xml"], b["ppt/slides/slide1.xml"], "untouched slide copied verbatim");
    assert_ne!(a["ppt/slides/slide2.xml"], b["ppt/slides/slide2.xml"]);

    let re = parse_pptx(&saved).unwrap();
    let body = re.slides[1].shapes[1].text.as_ref().unwrap();
    assert_eq!(body.paragraphs[0].text(), "新的第一段 & <特殊字元>");
    assert_eq!(body.paragraphs.len(), 6);
    assert!(body.paragraphs[0].bullet.is_some(), "bullet still inherited from the layout");
    assert_eq!(body.paragraphs[1].level, 1);
    let last = &body.paragraphs[5];
    assert!(last.runs[0].style.bold);
    assert_eq!(last.runs[1].style.color, Color::rgb(0xC0, 0, 0));
    // Only the changed paragraph was regenerated, and inherited sizes were not written out.
    let xml = String::from_utf8(b["ppt/slides/slide2.xml"].clone()).unwrap();
    assert!(!xml.contains(r#"sz="3200""#), "inherited size should stay implicit");
    assert_eq!(re.slides[1].shapes[0].text.as_ref().unwrap().plain_text(), "核心目標");
}

#[test]
fn moved_and_recolored_shapes() {
    let mut pres = sample();
    let shape = &mut pres.slides[2].shapes[2];
    shape.xfrm.x += 100_000;
    shape.xfrm.rot = 45.0;
    shape.fill = Some(Color::rgb(0x12, 0x34, 0x56));
    shape.line = Some(Line { color: Color::rgb(1, 2, 3), width: 25_400 });
    let expect = shape.xfrm;
    // Placeholder that inherits its position: moving it must add an xfrm.
    pres.slides[0].shapes[0].xfrm.y += 50_000;
    let title_xfrm = pres.slides[0].shapes[0].xfrm;
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let s = &re.slides[2].shapes[2];
    assert_eq!(s.xfrm, expect);
    assert_eq!(s.fill, Some(Color::rgb(0x12, 0x34, 0x56)));
    assert_eq!(s.line, Some(Line { color: Color::rgb(1, 2, 3), width: 25_400 }));
    assert_eq!(s.text.as_ref().unwrap().plain_text(), "Ellipse");
    assert_eq!(re.slides[0].shapes[0].xfrm, title_xfrm);
}

#[test]
fn edited_group_child_is_written_ungrouped_in_place() {
    let mut pres = sample();
    let n = pres.slides[2].shapes.len();
    pres.slides[2].shapes[n - 1].xfrm.x += 200_000;
    let expect: Vec<Xfrm> = pres.slides[2].shapes.iter().map(|s| s.xfrm).collect();
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    assert_eq!(re.slides[2].shapes.iter().map(|s| s.xfrm).collect::<Vec<_>>(), expect);
}

#[test]
fn add_delete_reorder_duplicate_slides() {
    let mut pres = sample();
    // New slide from layout "Title and Content" with text and a new rectangle.
    let layout = pres.layouts.iter().position(|l| l.name == "Title and Content").unwrap();
    let mut slide = pres.slide_from_layout(layout);
    let title = slide.shapes.iter_mut().find(|s| s.placeholder.as_ref().is_some_and(|p| p.kind == "title")).unwrap();
    let body = title.text.as_mut().unwrap();
    let style = body.style_at(0);
    body.insert(0, "新增的投影片", &style);
    let id = pres.alloc_id();
    let mut rect = Shape::new(id, "rect", Xfrm::new(1_000_000, 4_000_000, 2_000_000, 1_000_000));
    rect.geometry = Geometry::preset("roundRect");
    rect.fill = Some(Color::rgb(0x44, 0x72, 0xC4));
    slide.shapes.push(rect);
    pres.slides.insert(1, slide);
    // Duplicate the picture slide, delete the shapes slide, move the title slide last.
    let dup = pres.slides[4].clone();
    pres.slides.push(dup);
    pres.slides.remove(3);
    let first = pres.slides.remove(0);
    pres.slides.push(first);

    let saved = save_pptx(&pres).unwrap();
    let names = parts(&saved);
    assert!(!names.contains_key("ppt/slides/slide3.xml"), "deleted slide's part is dropped");
    let re = parse_pptx(&saved).unwrap();
    assert_eq!(re.slides.len(), 5);
    assert_eq!(texts(&re, 0)[0], "新增的投影片");
    assert!(re.slides[0].shapes.iter().any(|s| matches!(&s.geometry, Geometry::Preset { name, .. } if name == "roundRect")));
    assert_eq!(texts(&re, 4)[0], "ww-ppt M0 技術驗證");
    // Both picture slides reference real media.
    for i in [2, 3] {
        let pic = re.slides[i].shapes.iter().find_map(|s| match &s.kind {
            ShapeKind::Picture { media, .. } => media.clone(),
            _ => None,
        });
        assert!(re.media.contains_key(&pic.unwrap()));
    }
}

#[test]
fn shape_pasted_onto_another_slide_carries_its_image() {
    let mut pres = sample();
    let mut pic = pres.slides[3].shapes[1].clone();
    pic.id = pres.alloc_id();
    pres.slides[0].shapes.push(pic);
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let ShapeKind::Picture { media: Some(m), .. } = &re.slides[0].shapes.last().unwrap().kind else { panic!("picture lost") };
    assert!(re.media.contains_key(m));
}

#[test]
fn template_is_a_complete_deck() {
    let pres = parse_pptx(&template_package()).unwrap();
    assert_eq!(pres.slide_width, 12_192_000);
    assert_eq!(pres.layouts.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["標題投影片", "標題及內容", "區段標題", "兩項物件", "只有標題", "空白"]);
    let slide = &pres.slides[0];
    assert_eq!(slide.shapes.len(), 2);
    let title = &slide.shapes[0];
    assert_eq!(title.placeholder.as_ref().unwrap().kind, "ctrTitle");
    assert!(title.text.as_ref().unwrap().is_empty());
    assert_eq!(title.text.as_ref().unwrap().paragraphs[0].end_style.size_pt, 60.0);
    assert_eq!(title.xfrm.w, 9_144_000);
    // Body placeholders on the content layout carry bullets from the master.
    let content = &pres.layouts[1].placeholders[1];
    let p = &content.text.as_ref().unwrap().paragraphs[0];
    assert_eq!(p.bullet, Some('•'));
    assert_eq!(p.end_style.size_pt, 28.0);
    assert_eq!(pres.minor_font, ("Calibri".to_string(), "新細明體".to_string()));
}

#[test]
fn new_deck_from_template_round_trips() {
    let mut pres = parse_pptx(&template_package()).unwrap();
    let body = pres.slides[0].shapes[0].text.as_mut().unwrap();
    let style = body.style_at(0);
    body.insert(0, "我的簡報", &style);
    body.apply_style(0.."我的".len(), |s| s.bold = true);
    let id = pres.alloc_id();
    let mut tb = Shape::new(id, "文字方塊", Xfrm::new(500_000, 500_000, 3_000_000, 400_000));
    let mut text = TextBody { auto_grow: true, defaults: pres.text_defaults.clone(), ..Default::default() };
    text.paragraphs[0].end_style = pres.default_run_style();
    let style = text.style_at(0);
    text.insert(0, "Hello\n世界", &style);
    tb.text = Some(text);
    pres.slides[0].shapes.push(tb);
    let layout = pres.slide_from_layout(5);
    pres.slides.push(layout);

    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    assert_eq!(re.slides.len(), 2);
    let title = re.slides[0].shapes[0].text.as_ref().unwrap();
    assert_eq!(title.plain_text(), "我的簡報");
    assert!(title.paragraphs[0].runs[0].style.bold && !title.paragraphs[0].runs[1].style.bold);
    assert_eq!(title.paragraphs[0].runs[0].style.size_pt, 60.0);
    let tb = re.slides[0].shapes.last().unwrap();
    assert_eq!(tb.text.as_ref().unwrap().plain_text(), "Hello\n世界");
    assert!(tb.text.as_ref().unwrap().auto_grow);
    assert_eq!(tb.fill, None);
    assert_eq!(re.slides[1].layout, Some(5));
}
