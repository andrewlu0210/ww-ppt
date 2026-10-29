//! Dev tool: `cargo run -p ppt-ooxml --example resave -- in.pptx out.pptx [--edit]`.
//! Re-saves a deck; with `--edit` it also changes text, moves a shape and adds a slide.

use ppt_model::{Color, Geometry, Paint, Shape, Xfrm};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (&args[1], &args[2]);
    let mut pres = match input.as_str() {
        "template" => ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap(),
        path => ppt_ooxml::parse_pptx(&std::fs::read(path).unwrap()).unwrap(),
    };
    if args.iter().any(|a| a == "--edit") {
        if let Some(body) = pres.slides[0].shapes.iter_mut().find_map(|s| s.text.as_mut()) {
            let len = body.len();
            let style = body.style_at(0);
            body.delete(0..len);
            body.insert(0, "已由 ww-ppt 修改", &style);
        }
        if let Some(s) = pres.slides[0].shapes.last_mut() {
            s.xfrm.y += 300_000;
        }
        let layout = pres.layouts.len().min(2).saturating_sub(1);
        let mut slide = pres.slide_from_layout(layout);
        let id = pres.alloc_id();
        let mut rect = Shape::new(id, "rect", Xfrm::new(1_000_000, 1_000_000, 3_000_000, 1_500_000));
        rect.geometry = Geometry::preset("roundRect");
        rect.fill = Some(Paint::Solid(Color::rgb(0xED, 0x7D, 0x31)));
        slide.shapes.push(rect);
        if let Some(body) = slide.shapes.iter_mut().find_map(|s| s.text.as_mut()) {
            let style = body.style_at(0);
            body.insert(0, "新投影片標題", &style);
        }
        pres.slides.push(slide);
    }
    std::fs::write(output, ppt_ooxml::save_pptx(&pres).unwrap()).unwrap();
    println!("saved {output} ({} slides)", pres.slides.len());
}
