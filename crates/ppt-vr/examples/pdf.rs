//! Exports decks to PDF and compares the PDF pages (rasterized with
//! `pdftoppm`) with the reference PDF and with the screen renderer.
//! `cargo run -p ppt-vr --release --example pdf -- samples/vr/test.pptx [...]`
//! Writes `target/vr-pdf/<name>.pdf` and `page-NN.png` (reference | PDF | screen).
#![allow(clippy::unwrap_used)] // test and tool code: a failure should stop with a backtrace

use std::path::Path;
use std::process::Command;

use image::RgbaImage;
use ppt_vr::{SlideRenderer, compare, side_by_side};

const WIDTH: u32 = 960;

fn rasterize(pdf: &Path, dir: &Path, prefix: &str, height: u32) -> Vec<RgbaImage> {
    let status = Command::new("pdftoppm")
        .args(["-png", "-scale-to-x", &WIDTH.to_string(), "-scale-to-y", &height.to_string()])
        .arg(pdf)
        .arg(dir.join(prefix))
        .status()
        .expect("pdftoppm");
    assert!(status.success());
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.file_name().unwrap().to_str().unwrap().starts_with(&format!("{prefix}-"))).collect();
    files.sort();
    files.iter().map(|f| image::open(f).unwrap().to_rgba8()).collect()
}

fn main() {
    let mut r = SlideRenderer::new().unwrap();
    for arg in std::env::args().skip(1) {
        let pptx = Path::new(&arg);
        let name = pptx.file_stem().unwrap().to_str().unwrap();
        let dir = Path::new("target/vr-pdf").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pres = ppt_ooxml::parse_pptx(&std::fs::read(pptx).unwrap()).unwrap();
        r.new_deck();
        r.ensure_fonts(pres.font_families());
        let t = std::time::Instant::now();
        let bytes = r.export_pdf(&pres).unwrap();
        let out = dir.join(format!("{name}.pdf"));
        std::fs::write(&out, &bytes).unwrap();
        println!("{name}: {} slides, {} KB, {:?}", pres.slides.len(), bytes.len() / 1024, t.elapsed());
        let height = (WIDTH as f64 * pres.slide_height as f64 / pres.slide_width as f64).round() as u32;
        let ours = rasterize(&out, &dir, "pdf", height);
        let reference = pptx.with_extension("pdf");
        let refs = if reference.exists() { rasterize(&reference, &dir, "ref", height) } else { Vec::new() };
        let (mut sum_ref, mut sum_screen) = (0.0, 0.0);
        for (i, page) in ours.iter().enumerate() {
            let screen = r.render(&pres, &pres.slides[i], WIDTH).unwrap();
            let (vs_screen, _) = compare(&screen, page);
            sum_screen += vs_screen.score;
            let line = match refs.get(i) {
                Some(rf) => {
                    let (vs_ref, _) = compare(rf, page);
                    let (screen_ref, _) = compare(rf, &screen);
                    sum_ref += vs_ref.score;
                    side_by_side(&[rf, page, &screen]).save(dir.join(format!("page-{:02}.png", i + 1))).unwrap();
                    format!("vs reference {:6.2}% (screen {:6.2}%)", vs_ref.score, screen_ref.score)
                }
                None => {
                    side_by_side(&[page, &screen]).save(dir.join(format!("page-{:02}.png", i + 1))).unwrap();
                    String::new()
                }
            };
            println!("  page {:2}  vs screen {:6.2}%  {line}", i + 1, vs_screen.score);
        }
        let n = ours.len().max(1) as f64;
        println!("  mean: vs screen {:.2}%{}", sum_screen / n, if refs.is_empty() { String::new() } else { format!(", vs reference {:.2}%", sum_ref / n) });
    }
}
