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
    let image = match *shot {
        Shot::Slide { slide, clicks, ms } => match state(pres, slide, clicks, ms) {
            Some(frame) => r.render_animated(pres, &pres.slides[slide], &frame, width)?,
            None => r.render(pres, &pres.slides[slide], width)?,
        },
        Shot::Transition { from, to, t } => {
            let kind = pres.slides[to].transition.as_ref().map_or(ppt_model::TransitionKind::Fade { through_black: false }, |t| t.kind.clone());
            let before = from.map(|i| (&pres.slides[i], state(pres, i, usize::MAX, SETTLED)));
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
    let start = match clip.start {
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
    /// The picture's frame in pixels: x, y, width, height.
    rect: (f64, f64, f64, f64),
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
            let x = pres.slides[c.info.slide].shapes.iter().find(|s| s.id == c.info.shape)?.xfrm;
            let source = frames::VideoSource::open(&pres.media.get(&c.info.part)?, &c.info.part).ok()?;
            let rect = (x.x as f64 * scale, x.y as f64 * scale, x.w as f64 * scale, x.h as f64 * scale);
            Some(VideoPlay { source, rect, at_ms, until_ms: until_ms.min(own_end), trim_start_ms: c.info.trim_start_ms, len_ms: c.len_ms, repeat: c.info.repeat })
        })
        .collect()
}

/// Writes `pres` as a video to `path`. `progress(done, total)` is called after
/// each frame; returning false cancels (the partial file is removed).
#[cfg(target_os = "macos")]
pub fn export(pres: &Presentation, opts: &VideoOptions, path: &Path, progress: &mut dyn FnMut(usize, usize) -> bool) -> Result<bool, String> {
    let clips = load_clips(pres);
    let spans: Vec<ClipSpan> = clips.iter().map(|c| ClipSpan { slide: c.info.slide, start: c.info.start, len_ms: (c.info.slides == 1 && !c.info.repeat).then_some(c.len_ms) }).collect();
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
        let path = std::env::temp_dir().join(format!("want-point-mix-{}.wav", std::process::id()));
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
    for (i, shot) in shots.iter().enumerate() {
        let pixels = match &last {
            Some((s, p)) if s == shot => p.clone(),
            _ => render(&mut r, pres, shot, width, height)?,
        };
        // Playing videos are drawn over their pictures.
        let t_ms = i as f64 * 1000.0 / opts.fps.max(1) as f64;
        let mut shown: Option<Vec<u8>> = None;
        for v in &videos {
            if let Some(image) = v.media_ms(t_ms).and_then(|ms| v.source.image_at(ms)) {
                frames::draw(shown.get_or_insert_with(|| pixels.clone()), width, height, &image, v.rect);
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
