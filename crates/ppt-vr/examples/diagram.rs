//! SmartArt relayout check: for every SmartArt in a deck, the slide as loaded |
//! with a node added at the end | with the first node removed, side by side,
//! after a save and reopen. Also writes the deck with every SmartArt extended
//! by one node, to open in PowerPoint.
//! `cargo run -p ppt-vr --release --example diagram -- deck.pptx out-dir`

use std::sync::Arc;

use ppt_model::diagram::{Diagram, Insert};
use ppt_model::{Presentation, ShapeKind};
use ppt_vr::{SlideRenderer, side_by_side};

fn edited(pres: &Presentation, slide: usize, shape: usize, f: &dyn Fn(&mut Diagram)) -> Presentation {
    let mut pres = pres.clone();
    if let ShapeKind::Diagram { shapes, data: Some(data), .. } = &mut pres.slides[slide].shapes[shape].kind {
        let mut d = (**data).clone();
        f(&mut d);
        *shapes = Arc::new(d.layout());
        *data = Arc::new(d);
    }
    // Through the file, as the app would see it after saving.
    ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&pres).unwrap()).unwrap()
}

fn add(d: &mut Diagram) {
    let last = d.rows().iter().rposition(|r| r.depth == 0).unwrap();
    if let Some(r) = d.insert(last, Insert::After) {
        d.set_text(r, "新增項目");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pres = ppt_ooxml::parse_pptx(&std::fs::read(&args[1]).unwrap()).unwrap();
    let out = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(out).unwrap();
    let mut r = SlideRenderer::new().unwrap();
    r.ensure_fonts(pres.font_families());
    let mut all = pres.clone();
    for (si, slide) in pres.slides.iter().enumerate() {
        for (i, s) in slide.shapes.iter().enumerate() {
            let ShapeKind::Diagram { data: Some(d), .. } = &s.kind else { continue };
            let name = d.template.0.layout.rsplit('/').next().unwrap_or_default().replace('#', "_");
            println!("slide {} shape {i}: {name} {:?} nests={}", si + 1, d.arrangement(), d.can_nest());
            if !d.can_restructure() {
                continue;
            }
            let grown = edited(&pres, si, i, &add);
            let shrunk = edited(&pres, si, i, &|d| {
                d.delete(0);
            });
            let images: Vec<_> = [&pres, &grown, &shrunk].iter().map(|p| r.render(p, &p.slides[si], 640).unwrap()).collect();
            side_by_side(&images.iter().collect::<Vec<_>>()).save(out.join(format!("slide{:02}-{i}-{name}.png", si + 1))).unwrap();
            all = edited(&all, si, i, &add);
        }
    }
    std::fs::write(out.join("edited.pptx"), ppt_ooxml::save_pptx(&all).unwrap()).unwrap();
}
