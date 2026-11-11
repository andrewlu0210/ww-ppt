//! Contact sheet of transition frames (25 / 50 / 75 %) between the first two
//! slides of a deck: `cargo run -p ppt-vr --release --example transitions -- deck.pptx out.png`.
#![allow(clippy::unwrap_used)] // test and tool code: a failure should stop with a backtrace

use ppt_model::transition::{Dir, TransitionKind};
use ppt_vr::{SlideRenderer, side_by_side};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pres = ppt_ooxml::parse_pptx(&std::fs::read(&args[1]).unwrap()).unwrap();
    let mut r = SlideRenderer::new().unwrap();
    r.ensure_fonts(pres.font_families());
    let kinds = [
        TransitionKind::Fade { through_black: false },
        TransitionKind::Fade { through_black: true },
        TransitionKind::Push(Dir::Up),
        TransitionKind::Wipe(Dir::Right),
        TransitionKind::Cover(Dir::LeftDown),
        TransitionKind::Pull(Dir::Left),
        TransitionKind::Split { vertical: true, outward: true },
        TransitionKind::Split { vertical: false, outward: false },
        TransitionKind::RandomBar { vertical: true },
        TransitionKind::Dissolve,
        TransitionKind::Zoom { inward: true },
        TransitionKind::Circle,
    ];
    let mut rows = Vec::new();
    for k in &kinds {
        let frames: Vec<_> = [0.25, 0.5, 0.75].iter().map(|&t| r.render_transition(&pres, Some(&pres.slides[0]), &pres.slides[1], k, t, 320).unwrap()).collect();
        rows.push(side_by_side(&frames.iter().collect::<Vec<_>>()));
        println!("{k:?}");
    }
    let (w, h) = (rows[0].width(), rows[0].height());
    let mut sheet = image::RgbaImage::from_pixel(w * 2 + 8, (h + 8) * rows.len().div_ceil(2) as u32, image::Rgba([128, 128, 128, 255]));
    for (i, row) in rows.iter().enumerate() {
        image::imageops::overlay(&mut sheet, row, ((i % 2) as u32 * (w + 8)) as i64, ((i / 2) as u32 * (h + 8)) as i64);
    }
    sheet.save(&args[2]).unwrap();
}
