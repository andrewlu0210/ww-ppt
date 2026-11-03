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

/// Gives slide 3 (which has no media) a narration made from the deck's own music file.
fn with_narration() -> ppt_model::Presentation {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/media-demo.pptx");
    let mut pres = ppt_ooxml::parse_pptx(&std::fs::read(path).unwrap()).unwrap();
    let music = ppt_ooxml::media_clips(&pres).into_iter().find(|c| c.kind == ClipKind::Audio).unwrap();
    let bytes = pres.media.get(&music.part).unwrap();
    pres.media.insert("new/narration1.m4a".into(), bytes);
    pres.slides[2].narration = Some(ppt_model::media::Narration { media: "new/narration1.m4a".into() });
    pres
}

#[test]
fn a_narration_is_saved_as_an_audio_object_that_plays_by_itself() {
    let pres = with_narration();
    let saved = ppt_ooxml::save_pptx(&pres).unwrap();
    let again = ppt_ooxml::parse_pptx(&saved).unwrap();
    let clips = ppt_ooxml::media_clips(&again);
    // The two original clips on slide 2 and the video on slide 1 are still there, plus the narration.
    assert_eq!(clips.len(), 4, "{clips:#?}");
    let n = clips.iter().find(|c| c.slide == 2).expect("narration on slide 3");
    assert_eq!((n.kind, n.start, n.slides, n.repeat), (ClipKind::Audio, ClipStart::Auto, 1, false));
    assert!((n.volume - 1.0).abs() < 1e-9);
    assert!(again.media.get(&n.part).is_some_and(|b| b.len() > 1000));
    assert!(again.slides[2].shapes.iter().any(|s| s.name == ppt_model::media::NARRATION_NAME));
    // Saving again (now an ordinary audio object) keeps one narration.
    let twice = ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&again).unwrap()).unwrap();
    assert_eq!(ppt_ooxml::media_clips(&twice).iter().filter(|c| c.slide == 2).count(), 1);
}

#[test]
fn a_narration_joins_existing_animations() {
    // Slide 2 has its own timing (music and a click-started video): the narration is added to it.
    let mut pres = with_narration();
    pres.slides[1].narration = pres.slides[2].narration.take();
    let again = ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&pres).unwrap()).unwrap();
    let clips = ppt_ooxml::media_clips(&again);
    let on_two: Vec<_> = clips.iter().filter(|c| c.slide == 1).collect();
    assert_eq!(on_two.len(), 3, "{on_two:#?}");
    assert!(on_two.iter().any(|c| c.start == ClipStart::Click(1)), "the click-started video keeps its click");
    assert_eq!(on_two.iter().filter(|c| c.start == ClipStart::Auto && c.slides == 1).count(), 1, "the narration starts by itself");
}
