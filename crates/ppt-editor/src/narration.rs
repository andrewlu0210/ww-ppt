//! Recorded narration: each slide gets an audio object that plays by itself, and
//! the time the slide stayed becomes its automatic advance time. A recording
//! session (`begin_recordings` … `end_recordings`) is one undo step.

use std::sync::Arc;

use ppt_model::media::{NARRATION_NAME, Narration};
use ppt_model::{ShapeKind, Transition, TransitionKind};

use crate::Editor;

impl Editor {
    /// Starts a session: the narrations added until `end_recordings` undo together.
    pub fn begin_recordings(&mut self) {
        self.recording_before = Some(self.snapshot());
    }

    /// Gives slide `slide` the narration `bytes` (an audio file of type `ext`).
    /// `stay_ms` becomes the slide's advance time when given. Replaces an earlier narration.
    pub fn add_recording(&mut self, slide: usize, key: String, bytes: Arc<Vec<u8>>, stay_ms: Option<u32>) -> bool {
        let Some(s) = self.pres.slides.get_mut(slide) else { return false };
        s.shapes.retain(|sh| !(sh.name == NARRATION_NAME && matches!(sh.kind, ShapeKind::Picture { .. })));
        s.narration = Some(Narration { media: key.clone() });
        if let Some(ms) = stay_ms {
            // Keep the slide's own transition; a slide without one gets a plain cut.
            let t = s.transition.get_or_insert(Transition { kind: TransitionKind::Cut { through_black: false }, duration_ms: 0, on_click: true, after_ms: None });
            t.after_ms = Some(ms);
        }
        self.pres.media.insert(key, bytes);
        true
    }

    /// Ends the session; `false` if nothing was recorded (nothing to undo).
    pub fn end_recordings(&mut self) -> bool {
        let Some(before) = self.recording_before.take() else { return false };
        if before.pres.slides.iter().zip(&self.pres.slides).all(|(a, b)| a.narration == b.narration && a.transition == b.transition) {
            return false;
        }
        self.push_undo(before);
        true
    }

    /// Whether slide `slide` has a narration (recorded now, or loaded from a file).
    pub fn has_narration(&self, slide: usize) -> bool {
        self.pres.slides.get(slide).is_some_and(|s| s.narration.is_some() || s.shapes.iter().any(|sh| sh.name == NARRATION_NAME && matches!(sh.kind, ShapeKind::Picture { .. })))
    }

    /// Removes slide `slide`'s narration (one undo step); `false` if it has none.
    pub fn clear_narration(&mut self, slide: usize) -> bool {
        if !self.has_narration(slide) {
            return false;
        }
        let before = self.snapshot();
        let s = &mut self.pres.slides[slide];
        s.narration = None;
        s.shapes.retain(|sh| !(sh.name == NARRATION_NAME && matches!(sh.kind, ShapeKind::Picture { .. })));
        self.push_undo(before);
        true
    }
}
