//! Exports a deck as a video from the command line.
//! `cargo run -p ppt-video --release --example export -- deck.pptx out.mp4 [width] [seconds-per-slide]`

use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pres = ppt_ooxml::parse_pptx(&std::fs::read(&args[1]).unwrap()).unwrap();
    let mut opts = ppt_video::VideoOptions::default();
    if let Some(w) = args.get(3) {
        opts.width = w.parse().unwrap();
    }
    if let Some(s) = args.get(4) {
        opts.seconds_per_slide = s.parse().unwrap();
    }
    let start = Instant::now();
    let mut last = 0;
    let done = ppt_video::export(&pres, &opts, std::path::Path::new(&args[2]), &mut |i, n| {
        if i * 10 / n != last {
            last = i * 10 / n;
            println!("{}% ({i}/{n} frames, {:.1}s)", last * 10, start.elapsed().as_secs_f64());
        }
        true
    })
    .unwrap();
    println!("finished={done} in {:.1}s", start.elapsed().as_secs_f64());
}
