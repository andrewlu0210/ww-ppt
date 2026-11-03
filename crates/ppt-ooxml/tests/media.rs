//! Audio and video clips of `samples/media-demo.pptx` (made by `scripts/make_media_pptx.py`).

use ppt_model::media::{ClipKind, ClipStart};

#[test]
fn reads_clips_and_their_playback_settings() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/media-demo.pptx");
    let pres = ppt_ooxml::parse_pptx(&std::fs::read(path).unwrap()).unwrap();
    let clips = ppt_ooxml::media_clips(&pres);
    assert_eq!(clips.len(), 3, "{clips:#?}");

    let video = &clips[0];
    assert_eq!((video.slide, video.kind, video.start), (0, ClipKind::Video, ClipStart::Auto));
    assert_eq!(video.trim_start_ms, 500.0);
    assert!((video.volume - 0.8).abs() < 1e-9);
    assert!(!video.repeat && video.slides == 1);
    assert!(pres.media.get(&video.part).is_some_and(|b| b.len() > 1000));

    let music = &clips[1];
    assert_eq!((music.slide, music.kind, music.start), (1, ClipKind::Audio, ClipStart::Auto));
    assert!(music.repeat);
    assert_eq!(music.slides, 2);
    assert!((music.volume - 0.6).abs() < 1e-9);

    let on_click = &clips[2];
    assert_eq!((on_click.slide, on_click.kind, on_click.start), (1, ClipKind::Video, ClipStart::Click(1)));
}
