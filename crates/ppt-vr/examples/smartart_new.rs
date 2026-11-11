//! Inserts a new SmartArt of every layout into the sample deck and renders it,
//! before and after a save and reopen.
//! `cargo run -p ppt-vr --release --example smartart_new -- out-dir`

use ppt_ooxml::{Labels, new_smartart, parse_pptx, save_pptx, smartart_kinds};
use ppt_vr::{SlideRenderer, side_by_side};

fn main() {
    let out = std::path::Path::new(&std::env::args().nth(1).expect("out dir")).to_path_buf();
    std::fs::create_dir_all(&out).unwrap();
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let mut r = SlideRenderer::new().unwrap();
    for kind in smartart_kinds() {
        let mut pres = parse_pptx(&bytes).unwrap();
        let labels = Labels { root: "標題", top: "項目 {}", child: "子項目 {}" };
        let (mut shape, package) = new_smartart(&pres, kind, (pres.slide_width, pres.slide_height), &labels).unwrap();
        shape.id = pres.alloc_id();
        pres.package = Some(package);
        pres.slide_mut(0).unwrap().shapes.push(shape);
        let back = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
        r.ensure_fonts(pres.font_families());
        let images: Vec<_> = [&pres, &back].iter().map(|p| r.render(p, &p.slides[0], 640).unwrap()).collect();
        side_by_side(&images.iter().collect::<Vec<_>>()).save(out.join(format!("{kind}.png"))).unwrap();
        println!("{kind}");
    }
}
