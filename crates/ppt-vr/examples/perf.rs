//! Times opening, drawing, saving and exporting a deck, stage by stage, with
//! the process's memory at each step. Native code: the app runs the same code
//! as WebAssembly, which is slower (single-threaded, no SIMD by default).
//! `cargo run -p ppt-vr --release --example perf -- samples/vr/test7.pptx`

use std::time::{Duration, Instant};

use ppt_vr::SlideRenderer;

/// Resident memory of this process in MB (from `ps`).
fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok();
    out.and_then(|o| String::from_utf8(o.stdout).ok()).and_then(|s| s.trim().parse::<f64>().ok()).map_or(0.0, |kb| kb / 1024.0)
}

fn ms(d: Duration) -> String {
    format!("{:8.1} ms", d.as_secs_f64() * 1000.0)
}

fn step<T>(label: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let v = f();
    let d = t.elapsed();
    println!("{label:<34}{}   RSS {:7.0} MB", ms(d), rss_mb());
    (v, d)
}

fn main() {
    let path = std::env::args().nth(1).expect("path to a .pptx");
    let mut r = SlideRenderer::new().unwrap();
    println!("{path}   (RSS at start {:.0} MB)", rss_mb());
    let (bytes, t_read) = step("read file", || std::fs::read(&path).unwrap());
    println!("  {:.1} MB on disk", bytes.len() as f64 / 1e6);
    let (pres, t_parse) = step("parse", || ppt_ooxml::parse_pptx(&bytes).unwrap());
    let media: usize = pres.media.values().map(|m| m.len()).sum();
    let shapes: usize = pres.slides.iter().map(|s| s.shapes.len()).sum();
    println!("  {} slides, {} shapes, {} layouts, {} media files ({:.1} MB)", pres.slides.len(), shapes, pres.layouts.len(), pres.media.len(), media as f64 / 1e6);
    let (_, t_fonts) = step("load fonts", || r.ensure_fonts(pres.font_families()));

    // The editor's first slide at a typical canvas width (device pixels).
    const VIEW: u32 = 2000;
    let ((scene, h), t_enc0) = step("slide 1: build scene", || r.encode(&pres, &pres.slides[0], VIEW));
    let (_, t_gpu0) = step("slide 1: GPU render", || r.render_scene(&scene, VIEW, h).unwrap());
    println!("{:<34}{}", "=> first slide on screen", ms(t_read + t_parse + t_fonts + t_enc0 + t_gpu0));

    // Every slide: building the scene the first time (decodes its images) and again (cached).
    println!("\nper slide (build scene first / again, GPU render) at {VIEW}px:");
    let mut first = Vec::new();
    for (i, slide) in pres.slides.iter().enumerate() {
        let t = Instant::now();
        let (scene, h) = r.encode(&pres, slide, VIEW);
        let enc = t.elapsed();
        let t = Instant::now();
        r.render_scene(&scene, VIEW, h).unwrap();
        let gpu = t.elapsed();
        let t = Instant::now();
        let _ = r.encode(&pres, slide, VIEW);
        let again = t.elapsed();
        first.push((i + 1, enc, again, gpu));
    }
    first.sort_by_key(|x| std::cmp::Reverse(x.1));
    for (n, enc, again, gpu) in first.iter().take(8) {
        println!("  slide {n:3}  {} / {}  GPU {}", ms(*enc), ms(*again), ms(*gpu));
    }
    let total = |f: fn(&(usize, Duration, Duration, Duration)) -> Duration| first.iter().map(f).sum::<Duration>();
    println!("  all slides: first {}  again {}  GPU {}   RSS {:.0} MB", ms(total(|x| x.1)), ms(total(|x| x.2)), ms(total(|x| x.3)), rss_mb());
    let again_max = first.iter().map(|x| x.2).max().unwrap_or_default();
    println!("  slowest repeat build (drag/typing redraw budget 8–16 ms): {}", ms(again_max));

    // Thumbnails like the slide pane (320px), with images already decoded.
    let (_, _) = step("\nthumbnails (320px, all slides)", || {
        for slide in &pres.slides {
            r.render(&pres, slide, 320).unwrap();
        }
    });

    let (saved, _) = step("save (unchanged)", || ppt_ooxml::save_pptx(&pres).unwrap());
    println!("  {:.1} MB", saved.len() as f64 / 1e6);
    let mut edited = pres.clone();
    if let Some(s) = edited.slides[0].shapes.first_mut() {
        s.xfrm.x += 12_700;
    }
    let (_, _) = step("save (one shape moved)", || ppt_ooxml::save_pptx(&edited).unwrap());
    let (_, _) = step("clone document (undo snapshot)", || pres.clone());
    let (pdf, _) = step("export PDF", || r.export_pdf(&pres).unwrap());
    println!("  {:.1} MB", pdf.len() as f64 / 1e6);
}
