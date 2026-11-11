//! The template follows the interface language. Kept in its own test binary
//! because the language is process-wide.
#![allow(clippy::unwrap_used)] // test and tool code: a failure should stop with a backtrace

use ppt_model::{Lang, set_lang};
use ppt_ooxml::{parse_pptx, template_package};

#[test]
fn english_template() {
    set_lang(Lang::En);
    let pres = parse_pptx(&template_package()).unwrap();
    assert_eq!(pres.layouts.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Title Slide", "Title and Content", "Section Header", "Two Content", "Title Only", "Blank"]);
    let slide = &pres.slides[0];
    assert_eq!(slide.shapes.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Title 1", "Subtitle 2"]);
    assert_eq!(slide.shapes[0].placeholder.as_ref().unwrap().prompt(), Some("Click to add title"));
}
