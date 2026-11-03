//! Exports `samples/media-demo.pptx` (a video, looping music over two slides and a
//! click-started video) and checks the file has both tracks (needs a GPU, AVFoundation
//! and `ffprobe`; skipped without it).

use std::process::Command;

use ppt_video::{VideoOptions, export};

fn ffprobe(path: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = Command::new("ffprobe").args(["-v", "error"]).args(args).arg(path).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[test]
fn exports_video_and_sound() {
    let pres = ppt_ooxml::parse_pptx(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/media-demo.pptx")).unwrap()).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/video-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("media.mp4");
    let opts = VideoOptions { width: 320, fps: 10, seconds_per_slide: 0.5, use_timings: true };
    assert!(export(&pres, &opts, &path, &mut |_, _| true).unwrap());
    let Some(streams) = ffprobe(&path, &["-show_entries", "stream=codec_type", "-of", "csv=p=0"]) else {
        eprintln!("ffprobe is not installed: skipping the checks");
        return;
    };
    assert!(streams.contains("video") && streams.contains("audio"), "{streams}");
    // 2.5 s (video, 3 s minus 0.5 s trimmed) + 3 s (click-started video) + 0.5 s.
    let duration: f64 = ffprobe(&path, &["-show_entries", "format=duration", "-of", "csv=p=0"]).unwrap().parse().unwrap();
    assert!((duration - 6.0).abs() < 0.2, "{duration}");
}
