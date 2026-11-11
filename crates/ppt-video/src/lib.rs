//! Video export: renders every frame of a slide show headlessly (transitions
//! and animations included) and encodes it as an H.264 `.mp4`.

mod mix;
mod timeline;
#[cfg(target_os = "macos")]
mod decode;
#[cfg(target_os = "macos")]
mod encoder;
#[cfg(target_os = "macos")]
mod frames;

use std::collections::HashMap;
use std::path::Path;

use ppt_model::Presentation;
use ppt_model::anim::{Frame, Player, SlideStage};
use ppt_model::media::{ClipKind, ClipStart, MediaClip};
use ppt_offscreen::SlideRenderer;
pub use timeline::{ClipSpan, Plan, SETTLED, Shot, SlideTime, VideoOptions, frames, plan};

/// The animation state `shot` shows of slide `index` (`None`: no animations).
fn state(pres: &Presentation, index: usize, clicks: usize, ms: f64) -> Option<Frame> {
    let slide = &pres.slides[index];
    let timing = slide.timing.as_deref()?;
    let player = Player::new(timing);
    Some(player.frame(&SlideStage { pres, slide }, clicks.min(player.clicks()), ms))
}

/// Renders one shot as RGBA pixels.
fn render(r: &mut SlideRenderer, pres: &Presentation, shot: &Shot, width: u32, height: u32) -> Result<Vec<u8>, String> {
    render_with(r, pres, shot, None, width, height)
}

/// The slide of `shot` with only its first `keep` shapes (the shapes up to a video's picture).
fn render_up_to(r: &mut SlideRenderer, pres: &Presentation, shot: &Shot, keep: usize, width: u32, height: u32) -> Result<Vec<u8>, String> {
    render_with(r, pres, shot, Some(keep), width, height)
}

fn render_with(r: &mut SlideRenderer, pres: &Presentation, shot: &Shot, keep: Option<usize>, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let image = match *shot {
        Shot::Slide { slide, clicks, ms } => {
            let mut cut;
            let s = match keep {
                Some(n) => {
                    cut = ppt_model::Slide::clone(&pres.slides[slide]);
                    cut.shapes.truncate(n);
                    &cut
                }
                None => &*pres.slides[slide],
            };
            match state(pres, slide, clicks, ms) {
                Some(frame) => r.render_animated(pres, s, &frame, width)?,
                None => r.render(pres, s, width)?,
            }
        }
        Shot::Transition { from, to, t } => {
            let kind = pres.slides[to].transition.as_ref().map_or(ppt_model::TransitionKind::Fade { through_black: false }, |t| t.kind.clone());
            let before = from.map(|i| (&*pres.slides[i], state(pres, i, usize::MAX, SETTLED)));
            let after = state(pres, to, 0, 0.0);
            r.render_transition_animated(pres, before.as_ref().map(|(s, f)| (*s, f.as_ref())), (&pres.slides[to], after.as_ref()), &kind, t, width)?
        }
    };
    // The renderer rounds the height from the aspect ratio; the video's is even.
    let mut pixels = image.into_raw();
    pixels.resize((width * height * 4) as usize, 255);
    Ok(pixels)
}

/// The mixed sound of a deck's clips on the video's timeline, as a WAV file
/// (removed when dropped), and the clips' lengths that stretch their slides.
#[cfg(target_os = "macos")]
struct Sound {
    path: std::path::PathBuf,
}

#[cfg(target_os = "macos")]
impl Drop for Sound {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A clip with its media decoded.
#[cfg(target_os = "macos")]
struct Clip {
    info: MediaClip,
    pcm: std::sync::Arc<Vec<f32>>,
    /// Length after trimming.
    len_ms: f64,
}

/// Decodes the clips of shown slides; those whose media cannot be read are left out.
#[cfg(target_os = "macos")]
fn load_clips(pres: &Presentation) -> Vec<Clip> {
    let mut decoded: HashMap<String, Option<(std::sync::Arc<Vec<f32>>, f64)>> = HashMap::new();
    let mut out = Vec::new();
    for info in ppt_ooxml::media_clips(pres).into_iter().filter(|c| !pres.slides[c.slide].hidden) {
        let entry = decoded.entry(info.part.clone()).or_insert_with(|| {
            let bytes = pres.media.get(&info.part)?;
            let d = decode::decode(&bytes, &info.part).ok()?;
            Some((std::sync::Arc::new(d.pcm), d.duration_ms))
        });
        let Some((pcm, duration)) = entry.clone() else { continue };
        let len_ms = (duration - info.trim_start_ms - info.trim_end_ms).max(0.0);
        out.push(Clip { info, pcm, len_ms });
    }
    out
}

/// When `clip` starts and the latest it plays until, from the plan.
#[cfg(target_os = "macos")]
fn placement(clip: &MediaClip, plan: &Plan) -> Option<(f64, f64)> {
    let at = plan.slides.iter().position(|s| s.slide == clip.slide)?;
    let time = &plan.slides[at];
    let start = clip.delay_ms
        + match clip.start {
            ClipStart::Auto => time.click_ms[0],
            ClipStart::Click(n) => *time.click_ms.get(n).unwrap_or(&time.anim_ms),
        };
    // Music over several slides ends with the last of them.
    let last = (at + clip.slides - 1).min(plan.slides.len() - 1);
    Some((start, plan.slides[last].end_ms))
}

/// A video clip being drawn over its picture while it plays.
#[cfg(target_os = "macos")]
struct VideoPlay {
    source: frames::VideoSource,
    /// The picture's frame in pixels, with its rotation, mirroring and crop.
    frame: frames::PictureFrame,
    /// Shapes up to and including the picture; `Some` when other shapes lie above it (they stay on top).
    keep: Option<usize>,
    at_ms: f64,
    until_ms: f64,
    trim_start_ms: f64,
    len_ms: f64,
    repeat: bool,
}

#[cfg(target_os = "macos")]
impl VideoPlay {
    /// How far into the media the frame at `t_ms` of the video is; `None` when the clip is not on screen.
    fn media_ms(&self, t_ms: f64) -> Option<f64> {
        if t_ms < self.at_ms || t_ms >= self.until_ms || self.len_ms <= 0.0 {
            return None;
        }
        let played = t_ms - self.at_ms;
        // Once over, the last frame stays on screen until the slide ends.
        let at = if self.repeat { played % self.len_ms } else { played.min(self.len_ms - 1.0).max(0.0) };
        Some(self.trim_start_ms + at)
    }
}

#[cfg(target_os = "macos")]
fn video_plays(pres: &Presentation, clips: &[Clip], plan: &Plan, width: u32) -> Vec<VideoPlay> {
    let scale = width as f64 / pres.slide_width.max(1) as f64;
    clips
        .iter()
        .filter(|c| c.info.kind == ClipKind::Video)
        .filter_map(|c| {
            let (at_ms, until_ms) = placement(&c.info, plan)?;
            // A video shows only on its own slide.
            let own_end = plan.slides.iter().find(|s| s.slide == c.info.slide)?.end_ms;
            let shapes = &pres.slides[c.info.slide].shapes;
            let at = shapes.iter().position(|s| s.id == c.info.shape)?;
            let (x, kind) = (shapes[at].xfrm, &shapes[at].kind);
            let source = frames::VideoSource::open(&pres.media.get(&c.info.part)?, &c.info.part).ok()?;
            let crop = match kind {
                ppt_model::ShapeKind::Picture { crop, .. } => (crop.left, crop.top, crop.right, crop.bottom),
                _ => (0.0, 0.0, 0.0, 0.0),
            };
            let frame = frames::PictureFrame { x: x.x as f64 * scale, y: x.y as f64 * scale, w: x.w as f64 * scale, h: x.h as f64 * scale, rot: x.rot, flip_h: x.flip_h, flip_v: x.flip_v, crop };
            Some(VideoPlay { source, frame, keep: (at + 1 < shapes.len()).then_some(at + 1), at_ms, until_ms: until_ms.min(own_end), trim_start_ms: c.info.trim_start_ms, len_ms: c.len_ms, repeat: c.info.repeat })
        })
        .collect()
}

/// Writes `pres` as a video to `path`. `progress(done, total)` is called after
/// each frame; returning false cancels (the partial file is removed).
#[cfg(target_os = "macos")]
pub fn export(pres: &Presentation, opts: &VideoOptions, path: &Path, progress: &mut dyn FnMut(usize, usize) -> bool) -> Result<bool, String> {
    let clips = load_clips(pres);
    let spans: Vec<ClipSpan> = clips.iter().map(|c| ClipSpan { slide: c.info.slide, start: c.info.start, delay_ms: c.info.delay_ms, len_ms: (c.info.slides == 1 && !c.info.repeat).then_some(c.len_ms) }).collect();
    let plan = timeline::plan(pres, opts, &spans);
    let shots = &plan.shots;
    if shots.is_empty() {
        return Err(ppt_model::tr("沒有可匯出的投影片", "没有可导出的幻灯片", "No slides to export", &[]));
    }
    let total_ms = shots.len() as f64 * 1000.0 / opts.fps.max(1) as f64;
    let tracks: Vec<mix::Track> = clips
        .iter()
        .filter(|c| !c.pcm.is_empty())
        .filter_map(|c| {
            let (at_ms, until_ms) = placement(&c.info, &plan)?;
            Some(mix::Track { pcm: c.pcm.clone(), from_ms: c.info.trim_start_ms, to_ms: c.info.trim_start_ms + c.len_ms, at_ms, until_ms, volume: c.info.volume as f32, repeat: c.info.repeat })
        })
        .collect();
    let sound = if tracks.is_empty() {
        None
    } else {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!("want-point-mix-{}-{}.wav", std::process::id(), COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::write(&path, mix::wav(&mix::mix(&tracks, total_ms))).map_err(|e| e.to_string())?;
        Some(Sound { path })
    };
    let (width, height) = opts.size(pres);
    let mut r = SlideRenderer::new()?;
    r.ensure_fonts(pres.font_families());
    let mut encoder = encoder::Encoder::new(path, width, height, opts.fps, sound.as_ref().map(|s| s.path.as_path()))?;
    let videos = video_plays(pres, &clips, &plan, width);
    // Unchanged frames (a slide holding still) are rendered once.
    let mut last: Option<(Shot, Vec<u8>)> = None;
    // For videos with shapes above them: the slide without those shapes, per shot.
    let mut below_cache: Vec<Option<(Shot, Vec<u8>)>> = vec![None; videos.len()];
    for (i, shot) in shots.iter().enumerate() {
        let pixels = match &last {
            Some((s, p)) if s == shot => p.clone(),
            _ => render(&mut r, pres, shot, width, height)?,
        };
        // Playing videos are drawn in their pictures; shapes above a picture stay on top of the video.
        let t_ms = i as f64 * 1000.0 / opts.fps.max(1) as f64;
        let mut shown: Option<Vec<u8>> = None;
        for (n, v) in videos.iter().enumerate() {
            let Some(image) = v.media_ms(t_ms).and_then(|ms| v.source.image_at(ms)) else { continue };
            match v.keep {
                None => frames::draw(shown.get_or_insert_with(|| pixels.clone()), width, height, &image, v.frame),
                Some(keep) => {
                    // The slide without what lies above the picture, then with the video in it; only
                    // where the upper shapes change nothing does the video show through.
                    let below = match &below_cache[n] {
                        Some((s, p)) if s == shot => p.clone(),
                        _ => render_up_to(&mut r, pres, shot, keep, width, height)?,
                    };
                    let mut with_video = below.clone();
                    frames::draw(&mut with_video, width, height, &image, v.frame);
                    let out = shown.get_or_insert_with(|| pixels.clone());
                    for (((o, full), b), w) in out.chunks_exact_mut(4).zip(pixels.chunks_exact(4)).zip(below.chunks_exact(4)).zip(with_video.chunks_exact(4)) {
                        if full == b {
                            o.copy_from_slice(w);
                        }
                    }
                    below_cache[n] = Some((*shot, below));
                }
            }
        }
        if let Err(e) = encoder.push(shown.as_deref().unwrap_or(&pixels)) {
            encoder.cancel();
            let _ = std::fs::remove_file(path);
            return Err(e);
        }
        last = Some((*shot, pixels));
        if !progress(i + 1, shots.len()) {
            encoder.cancel();
            let _ = std::fs::remove_file(path);
            return Ok(false);
        }
    }
    encoder.finish()?;
    Ok(true)
}
