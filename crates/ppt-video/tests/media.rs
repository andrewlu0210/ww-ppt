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

#[test]
fn a_narration_stretches_its_slide_and_is_heard() {
    use ppt_model::media::{ClipKind, Narration};
    let mut pres = ppt_ooxml::parse_pptx(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/media-demo.pptx")).unwrap()).unwrap();
    // The deck's 2 s music file as slide 3's narration, through a save and load like a real recording.
    let music = ppt_ooxml::media_clips(&pres).into_iter().find(|c| c.kind == ClipKind::Audio).unwrap();
    pres.media.insert("new/narration1.m4a".into(), pres.media.get(&music.part).unwrap());
    pres.slide_mut(2).unwrap().narration = Some(Narration { media: "new/narration1.m4a".into() });
    let pres = ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&pres).unwrap()).unwrap();

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/video-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("narration.mp4");
    let opts = VideoOptions { width: 320, fps: 10, seconds_per_slide: 0.5, use_timings: true };
    assert!(export(&pres, &opts, &path, &mut |_, _| true).unwrap());
    let Some(d) = ffprobe(&path, &["-show_entries", "format=duration", "-of", "csv=p=0"]) else { return };
    // 2.5 s + 3 s as before, and the last slide now lasts the narration's 2 s (not 0.5 s).
    let duration: f64 = d.parse().unwrap();
    assert!((duration - 7.5).abs() < 0.2, "{duration}");
}

#[test]
fn a_delayed_video_with_a_rotation_and_shapes_around_it_exports() {
    // `samples/media-layers.pptx`: a rotated, cropped video that starts 1 s in, with a shape under and one over it.
    let pres = ppt_ooxml::parse_pptx(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/media-layers.pptx")).unwrap()).unwrap();
    let clips = ppt_ooxml::media_clips(&pres);
    assert_eq!(clips.len(), 1, "{clips:#?}");
    assert_eq!(clips[0].delay_ms, 1000.0);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/video-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("layers.mp4");
    let opts = VideoOptions { width: 320, fps: 10, seconds_per_slide: 0.5, use_timings: true };
    assert!(export(&pres, &opts, &path, &mut |_, _| true).unwrap());
    let Some(d) = ffprobe(&path, &["-show_entries", "format=duration", "-of", "csv=p=0"]) else { return };
    // 1 s of delay and the 3 s video: the slide stays 4 s.
    let duration: f64 = d.parse().unwrap();
    assert!((duration - 4.0).abs() < 0.2, "{duration}");
}
