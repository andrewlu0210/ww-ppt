//! Contact sheet of a slide's animation: one row per click, frames at 0 / 25 /
//! 50 / 75 / 100 % of what that click starts.
//! `cargo run -p ppt-vr --release --example animation -- deck.pptx <slide> out.png`
#![allow(clippy::unwrap_used)] // test and tool code: a failure should stop with a backtrace

use ppt_model::anim::{Player, SlideStage};
use ppt_vr::{SlideRenderer, side_by_side};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pres = ppt_ooxml::parse_pptx(&std::fs::read(&args[1]).unwrap()).unwrap();
    let slide = &pres.slides[args[2].parse::<usize>().unwrap()];
    let timing = slide.timing.as_ref().expect("slide has no animations");
    let player = Player::new(timing);
    let stage = SlideStage { pres: &pres, slide };
    println!("clicks: {}", player.clicks());
    let mut r = SlideRenderer::new().unwrap();
    r.ensure_fonts(pres.font_families());
    let mut rows = Vec::new();
    for click in 0..=player.clicks() {
        let d = player.duration(click);
        println!("click {click}: {d} ms");
        let frames: Vec<_> = [0.0, 0.25, 0.5, 0.75, 1.0].iter().map(|k| r.render_animated(&pres, slide, &player.frame(&stage, click, d * k), 240).unwrap()).collect();
        rows.push(side_by_side(&frames.iter().collect::<Vec<_>>()));
    }
    let (w, h) = (rows[0].width(), rows[0].height());
    let mut sheet = image::RgbaImage::from_pixel(w, (h + 6) * rows.len() as u32, image::Rgba([128, 128, 128, 255]));
    for (i, row) in rows.iter().enumerate() {
        image::imageops::overlay(&mut sheet, row, 0, (i as u32 * (h + 6)) as i64);
    }
    sheet.save(&args[3]).unwrap();
}
