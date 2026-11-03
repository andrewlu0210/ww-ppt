//! What each frame of the video shows, following PowerPoint's video export:
//! every shown slide for a fixed time (or its recorded timing), its
//! transition first, and its click animations played one after another.

use ppt_model::Presentation;
use ppt_model::anim::Player;
use ppt_model::media::ClipStart;

/// Settings of an export.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoOptions {
    /// Frame width in pixels; the height follows the slide's aspect ratio.
    pub width: u32,
    pub fps: u32,
    /// How long each slide stays after its transition and animations (seconds).
    pub seconds_per_slide: f64,
    /// Use a slide's recorded "advance after" time instead, when it has one.
    pub use_timings: bool,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self { width: 1920, fps: 30, seconds_per_slide: 5.0, use_timings: true }
    }
}

impl VideoOptions {
    /// Frame size, both even (H.264 needs it).
    pub fn size(&self, pres: &Presentation) -> (u32, u32) {
        let even = |v: f64| ((v / 2.0).round() as u32).max(1) * 2;
        let w = even(self.width as f64);
        (w, even(w as f64 * pres.slide_height as f64 / pres.slide_width.max(1) as f64))
    }
}

/// Time far beyond any animation: the slide with everything played.
pub const SETTLED: f64 = 1e12;

/// What one frame shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shot {
    /// Slide `slide` with `clicks` clicks played and `ms` into the last one
    /// ([`SETTLED`]: finished). `clicks` is 0 with `ms` 0 before any animation.
    Slide { slide: usize, clicks: usize, ms: f64 },
    /// Progress `t` (0–1) of `to`'s transition from `from` (with every
    /// animation played), or from black for the first slide.
    Transition { from: Option<usize>, to: usize, t: f64 },
}

/// How long a clip plays and when it starts, for stretching its slide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipSpan {
    pub slide: usize,
    pub start: ClipStart,
    /// Wait after `start` (the effect's delay).
    pub delay_ms: f64,
    /// Playing time in the video (after trimming); `None` if it carries on
    /// past its slide (a loop, or music over several slides).
    pub len_ms: Option<f64>,
}

/// When a shown slide plays, in milliseconds from the start of the video.
#[derive(Clone, Debug, PartialEq)]
pub struct SlideTime {
    pub slide: usize,
    /// Its transition begins.
    pub start_ms: f64,
    /// Its transition has played: animations and media can begin.
    pub anim_ms: f64,
    /// Where click `n`'s animations begin (index 0: the ones that start by themselves).
    pub click_ms: Vec<f64>,
    pub end_ms: f64,
}

/// The frames of the video and when each shown slide plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub shots: Vec<Shot>,
    pub slides: Vec<SlideTime>,
}

/// One shot per frame, for the slides that are shown (hidden ones are skipped).
pub fn frames(pres: &Presentation, opts: &VideoOptions) -> Vec<Shot> {
    plan(pres, opts, &[]).shots
}

/// [`frames`] plus the slides' timing. A slide stays long enough for the clips
/// that play only on it (`ClipSpan::len_ms`) to finish.
pub fn plan(pres: &Presentation, opts: &VideoOptions, clips: &[ClipSpan]) -> Plan {
    let fps = opts.fps.max(1) as f64;
    let frame_ms = 1000.0 / fps;
    let mut out = Vec::new();
    let mut times = Vec::new();
    // Frames for `ms` milliseconds; the fraction carries over so the total stays exact.
    let mut owed = 0.0;
    let now = std::cell::Cell::new(0.0);
    let mut span = |ms: f64, out: &mut Vec<Shot>, at: &dyn Fn(f64) -> Shot| {
        owed += ms;
        now.set(now.get() + ms);
        let n = (owed / frame_ms).floor().max(0.0) as usize;
        owed -= n as f64 * frame_ms;
        for k in 0..n {
            out.push(at(k as f64 * frame_ms / ms.max(1e-9)));
        }
    };
    let mut prev: Option<usize> = None;
    for (i, slide) in pres.slides.iter().enumerate().filter(|(_, s)| !s.hidden) {
        let start_ms = now.get();
        if let Some(t) = slide.transition.as_ref().filter(|t| t.animates()) {
            let ms = t.duration_ms as f64;
            span(ms, &mut out, &|p| Shot::Transition { from: prev, to: i, t: p });
        }
        let anim_ms = now.get();
        let player = slide.timing.as_deref().map(Player::new);
        let mut played = 0.0;
        let clicks = player.as_ref().map_or(0, Player::clicks);
        let mut click_ms = Vec::new();
        for click in 0..=clicks {
            click_ms.push(anim_ms + played);
            let ms = player.as_ref().map_or(0.0, |p| p.duration(click));
            if ms > 0.0 {
                span(ms, &mut out, &|p| Shot::Slide { slide: i, clicks: click, ms: p * ms });
                played += ms;
            }
        }
        let stay = slide.transition.as_ref().and_then(|t| t.after_ms).filter(|_| opts.use_timings).map_or(opts.seconds_per_slide * 1000.0, |ms| ms as f64);
        // A recorded time counts from the slide's start, animations included.
        let mut hold = if opts.use_timings && slide.transition.as_ref().is_some_and(|t| t.after_ms.is_some()) { (stay - played).max(0.0) } else { stay };
        // Wait for the sounds and videos that play only here.
        for c in clips.iter().filter(|c| c.slide == i) {
            let Some(len) = c.len_ms else { continue };
            let begin = c.delay_ms
                + match c.start {
                    ClipStart::Auto => click_ms[0],
                    ClipStart::Click(n) => click_ms.get(n).copied().unwrap_or(anim_ms + played),
                };
            hold = hold.max(begin + len - now.get());
        }
        span(hold, &mut out, &|_| Shot::Slide { slide: i, clicks, ms: SETTLED });
        times.push(SlideTime { slide: i, start_ms, anim_ms, click_ms, end_ms: now.get() });
        prev = Some(i);
    }
    Plan { shots: out, slides: times }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppt_model::{Slide, Transition, TransitionKind};

    fn deck(slides: Vec<Slide>) -> Presentation {
        let mut pres = Presentation::default();
        pres.slides = slides;
        pres
    }

    #[test]
    fn slides_hold_for_the_set_time_and_hidden_ones_are_skipped() {
        let hidden = Slide { hidden: true, ..Default::default() };
        let pres = deck(vec![Slide::default(), hidden, Slide::default()]);
        let opts = VideoOptions { fps: 10, seconds_per_slide: 2.0, ..Default::default() };
        let f = frames(&pres, &opts);
        assert_eq!(f.len(), 40);
        assert!(f[..20].iter().all(|s| matches!(s, Shot::Slide { slide: 0, .. })));
        assert!(f[20..].iter().all(|s| matches!(s, Shot::Slide { slide: 2, .. })));
    }

    #[test]
    fn transitions_come_first_and_recorded_times_win() {
        let fade = Transition { kind: TransitionKind::Fade { through_black: false }, duration_ms: 500, on_click: true, after_ms: Some(3000) };
        let pres = deck(vec![Slide::default(), Slide { transition: Some(fade), ..Default::default() }]);
        let opts = VideoOptions { fps: 10, seconds_per_slide: 1.0, ..Default::default() };
        let f = frames(&pres, &opts);
        // 1 s, then 0.5 s of transition and 3 s on the second slide.
        assert_eq!(f.len(), 10 + 5 + 30);
        assert_eq!(f[10], Shot::Transition { from: Some(0), to: 1, t: 0.0 });
        assert!(matches!(f[14], Shot::Transition { t, .. } if (t - 0.8).abs() < 1e-9));
        let without = frames(&pres, &VideoOptions { use_timings: false, ..opts });
        assert_eq!(without.len(), 10 + 5 + 10);
    }

    #[test]
    fn a_slide_waits_for_the_clip_that_plays_only_on_it() {
        let pres = deck(vec![Slide::default(), Slide::default()]);
        let opts = VideoOptions { fps: 10, seconds_per_slide: 1.0, ..Default::default() };
        let clip = ClipSpan { slide: 0, start: ClipStart::Auto, delay_ms: 0.0, len_ms: Some(2500.0) };
        let p = plan(&pres, &opts, &[clip]);
        assert_eq!(p.shots.len(), 25 + 10);
        assert_eq!((p.slides[0].anim_ms, p.slides[0].end_ms), (0.0, 2500.0));
        assert_eq!(p.slides[1].start_ms, 2500.0);
        // A loop (no length) leaves the time alone.
        let looped = ClipSpan { len_ms: None, ..clip };
        assert_eq!(plan(&pres, &opts, &[looped]).shots.len(), 20);
    }

    #[test]
    fn a_delayed_clip_starts_later_and_holds_its_slide_longer() {
        let pres = deck(vec![Slide::default()]);
        let opts = VideoOptions { fps: 10, seconds_per_slide: 1.0, ..Default::default() };
        let clip = ClipSpan { slide: 0, start: ClipStart::Auto, delay_ms: 1000.0, len_ms: Some(2000.0) };
        // It plays from 1 s to 3 s: the slide stays 3 s, not 1 s.
        assert_eq!(plan(&pres, &opts, &[clip]).shots.len(), 30);
    }

    #[test]
    fn frame_size_is_even() {
        let pres = deck(vec![]);
        let (w, h) = VideoOptions { width: 1281, ..Default::default() }.size(&pres);
        assert_eq!((w % 2, h % 2), (0, 0));
        assert_eq!(w, 1282);
    }
}
