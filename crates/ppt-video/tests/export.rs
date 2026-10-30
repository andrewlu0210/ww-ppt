//! Exports the sample deck to a real `.mp4` (needs a GPU and AVFoundation).

use ppt_video::{VideoOptions, export, frames};

fn deck() -> ppt_model::Presentation {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/vr/test5.pptx");
    ppt_ooxml::parse_pptx(&std::fs::read(path).unwrap()).unwrap()
}

fn out(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/video-test");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

const SMALL: VideoOptions = VideoOptions { width: 320, fps: 10, seconds_per_slide: 0.5, use_timings: true };

#[test]
fn writes_every_frame() {
    let pres = deck();
    let path = out("small.mp4");
    let mut calls = 0;
    let total = frames(&pres, &SMALL).len();
    assert!(export(&pres, &SMALL, &path, &mut |done, n| {
        calls += 1;
        assert_eq!(n, total);
        done <= n
    })
    .unwrap());
    assert_eq!(calls, total);
    let bytes = std::fs::read(&path).unwrap();
    // An MPEG-4 file: `ftyp` box first.
    assert_eq!(&bytes[4..8], b"ftyp");
}

#[test]
fn cancelling_removes_the_partial_file() {
    let pres = deck();
    let path = out("cancelled.mp4");
    let finished = export(&pres, &SMALL, &path, &mut |done, _| done < 3).unwrap();
    assert!(!finished);
    assert!(!path.exists());
}
