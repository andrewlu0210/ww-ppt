//! Prints paragraph settings and line positions of a named shape: `lines deck.pptx <slide> <shape name>`.
#![allow(clippy::unwrap_used)] // test and tool code: a failure should stop with a backtrace
use ppt_vr::SlideRenderer;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pres = ppt_ooxml::parse_pptx(&std::fs::read(&args[1]).unwrap()).unwrap();
    let slide = &pres.slides[args[2].parse::<usize>().unwrap()];
    let shape = slide.shapes.iter().find(|s| s.name == args[3]).expect("shape");
    let body = shape.text.as_ref().unwrap();
    for p in &body.paragraphs {
        let size = p.first_style().size_pt;
        let sizes: Vec<String> = p.runs.iter().map(|r| format!("{}:{:?}", r.style.size_pt, r.text.chars().take(4).collect::<String>())).collect();
        println!("runs {sizes:?} end {}", p.end_style.size_pt);
        println!("size {size} spacing {} before {} after {} fonts {:?}/{:?}", p.line_spacing, p.space_before, p.space_after, p.first_style().latin_font, p.first_style().ea_font);
    }
    let mut r = SlideRenderer::new().unwrap();
    r.ensure_fonts(pres.font_families());
    let width = ppt_geometry::text_width_pt(&ppt_geometry::TextFrame::of(shape.xfrm, &shape.geometry), body);
    let layout = r.engine().layout_body(body, width);
    for line in layout.layout.lines() {
        let m = line.metrics();
        println!("line baseline {:.1} height {:.1} min {:.1} max {:.1}", m.baseline, m.line_height, m.block_min_coord, m.block_max_coord);
    }
}
