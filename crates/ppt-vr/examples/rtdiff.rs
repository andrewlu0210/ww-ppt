//! Renders one slide of two decks side by side: `rtdiff a.pptx b.pptx <slide> out.png`.
#![allow(clippy::unwrap_used)] // test and tool code: a failure should stop with a backtrace
use ppt_vr::{SlideRenderer, compare, side_by_side};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let load = |p: &str| ppt_ooxml::parse_pptx(&std::fs::read(p).unwrap()).unwrap();
    let (a, b) = (load(&args[1]), load(&args[2]));
    let i: usize = args[3].parse().unwrap();
    let mut r = SlideRenderer::new().unwrap();
    r.ensure_fonts(a.font_families());
    // Render the earlier slides first, like the regression run does.
    for k in 0..i {
        r.render(&a, &a.slides[k], 1280).unwrap();
        r.render(&b, &b.slides[k], 1280).unwrap();
    }
    let (ia, ib) = (r.render(&a, &a.slides[i], 1280).unwrap(), r.render(&b, &b.slides[i], 1280).unwrap());
    let again = r.render(&a, &a.slides[i], 1280).unwrap();
    println!("same deck twice: {:.2}%", compare(&ia, &again).0.score);
    let (d, vis) = compare(&ia, &ib);
    println!("diff {:.2}%", d.score);
    side_by_side(&[&ia, &ib, &vis]).save(&args[4]).unwrap();
}
