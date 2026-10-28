//! Parses samples/m0-demo.pptx, generated from python-pptx's default Office template.

use ppt_model::{Align, Anchor, ShapeKind};

fn deck() -> ppt_model::Presentation {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx");
    ppt_ooxml::parse_pptx(&std::fs::read(path).expect("run scripts/make_sample_pptx.py first")).unwrap()
}

#[test]
fn title_slide_inherits_layout() {
    let pres = deck();
    assert_eq!(pres.slides.len(), 4);
    let title = &pres.slides[0].shapes[0];
    assert!(title.xfrm.w > 0 && title.xfrm.h > 0, "title position comes from the layout");
    let body = title.text.as_ref().unwrap();
    assert_eq!(body.paragraphs[0].text(), "ww-ppt M0 技術驗證");
    assert_eq!(body.paragraphs[0].align, Align::Center);
    assert!(body.paragraphs[0].first_style().size_pt >= 40.0);
    // python-pptx ships the 2007 Office template, whose title placeholder is centered vertically.
    assert_eq!(body.anchor, Anchor::Middle);
}

#[test]
fn content_slide_has_bullets_and_runs() {
    let pres = deck();
    let body = pres.slides[1].shapes[1].text.as_ref().unwrap();
    assert_eq!(body.paragraphs.len(), 6);
    assert!(body.paragraphs[0].bullet.is_some());
    assert_eq!(body.paragraphs[1].level, 1);
    let last = &body.paragraphs[5];
    assert!(last.runs[0].style.bold);
    assert_eq!(last.runs[1].style.color, ppt_model::Color::rgb(0xC0, 0, 0));
    assert!(last.runs[2].style.italic && last.runs[2].style.underline);
}

#[test]
fn shapes_and_picture() {
    let pres = deck();
    let shapes = &pres.slides[2].shapes;
    // title, roundRect, ellipse, arrow, star, connector, text box, 2 grouped shapes
    assert_eq!(shapes.len(), 9);
    assert_eq!(shapes[3].xfrm.rot, 20.0);
    assert!(shapes[1].fill.is_some(), "theme style fill");
    assert!(shapes[5].geometry.is_line());
    let pic = &pres.slides[3].shapes[1];
    let ShapeKind::Picture { media: Some(media), .. } = &pic.kind else { panic!("expected picture") };
    assert!(pres.media.contains_key(media));
}
