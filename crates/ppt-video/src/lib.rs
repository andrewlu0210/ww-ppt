//! Video export: renders every frame of a slide show headlessly (transitions
//! and animations included) and encodes it as an H.264 `.mp4`.

mod timeline;
#[cfg(target_os = "macos")]
mod encoder;

use std::path::Path;

use ppt_model::Presentation;
use ppt_model::anim::{Frame, Player, SlideStage};
use ppt_offscreen::SlideRenderer;
pub use timeline::{SETTLED, Shot, VideoOptions, frames};

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

/// Writes `pres` as a video to `path`. `progress(done, total)` is called after
/// each frame; returning false cancels (the partial file is removed).
#[cfg(target_os = "macos")]
pub fn export(pres: &Presentation, opts: &VideoOptions, path: &Path, progress: &mut dyn FnMut(usize, usize) -> bool) -> Result<bool, String> {
    let shots = frames(pres, opts);
    if shots.is_empty() {
        return Err(ppt_model::tr("沒有可匯出的投影片", "没有可导出的幻灯片", "No slides to export", &[]));
    }
    let (width, height) = opts.size(pres);
    let mut r = SlideRenderer::new()?;
    r.ensure_fonts(pres.font_families());
    let mut encoder = encoder::Encoder::new(path, width, height, opts.fps)?;
    // Unchanged frames (a slide holding still) are rendered once.
    let mut last: Option<(Shot, Vec<u8>)> = None;
    for (i, shot) in shots.iter().enumerate() {
        let pixels = match &last {
            Some((s, p)) if s == shot => p.clone(),
            _ => render(&mut r, pres, shot, width, height)?,
        };
        if let Err(e) = encoder.push(&pixels) {
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
