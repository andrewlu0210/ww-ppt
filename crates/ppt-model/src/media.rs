//! Encoded media (pictures) by package part name. Pictures of the loaded file
//! stay in the package and are read when needed; pictures added in the editor
//! are kept in memory until saved.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Where loaded media comes from (the package; implemented by the reader).
pub trait MediaSource: Send + Sync {
    fn read(&self, part: &str) -> Option<Vec<u8>>;
    fn contains(&self, part: &str) -> bool;
}

#[derive(Clone, Default)]
pub struct MediaStore {
    added: HashMap<String, Arc<Vec<u8>>>,
    source: Option<Arc<dyn MediaSource>>,
}

impl MediaStore {
    /// Media of the loaded package, read on demand.
    pub fn set_source(&mut self, source: Arc<dyn MediaSource>) {
        self.source = Some(source);
    }

    /// The encoded bytes of `key`. Package media is read (and decompressed) on
    /// every call: callers that need it often keep what they make of it.
    pub fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.added.get(key).cloned().or_else(|| self.source.as_ref()?.read(key).map(Arc::new))
    }

    pub fn insert(&mut self, key: String, bytes: Arc<Vec<u8>>) {
        self.added.insert(key, bytes);
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.added.contains_key(key) || self.source.as_ref().is_some_and(|s| s.contains(key))
    }

    /// Media added since loading, which saving has to write.
    pub fn added(&self) -> impl Iterator<Item = (&String, &Arc<Vec<u8>>)> {
        self.added.iter()
    }
}

impl PartialEq for MediaStore {
    fn eq(&self, other: &Self) -> bool {
        let same_source = match (&self.source, &other.source) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        same_source && self.added == other.added
    }
}

impl fmt::Debug for MediaStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MediaStore").field("added", &self.added.keys().collect::<Vec<_>>()).field("source", &self.source.is_some()).finish()
    }
}

/// What a slide's audio or video clip plays, as PowerPoint stored it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipKind {
    Audio,
    Video,
}

/// When a clip starts playing on its slide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipStart {
    /// With the slide (before any click).
    Auto,
    /// With click `n` of the slide's animations (1 is the first click).
    Click(usize),
}

/// An audio or video object on a slide with its playback settings. Read from
/// the original XML for video export; the editor does not edit these.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaClip {
    /// Index into [`crate::Presentation::slides`].
    pub slide: usize,
    pub kind: ClipKind,
    /// The picture or icon the clip is shown as (its frame on the slide).
    pub shape: crate::ShapeId,
    /// Key into [`crate::Presentation::media`].
    pub part: String,
    /// Time cut from the start / the end of the media (milliseconds).
    pub trim_start_ms: f64,
    pub trim_end_ms: f64,
    /// 0–1.
    pub volume: f64,
    pub repeat: bool,
    /// Slides the sound plays over, counting its own: 1 stops with its slide,
    /// more is background music that carries on.
    pub slides: usize,
    pub start: ClipStart,
}

/// Name of the audio object holding a slide's recorded narration.
pub const NARRATION_NAME: &str = "Narration";

/// A recording to write into a slide when saving, as an audio object that plays
/// by itself when the slide appears (what PowerPoint's "Record Slide Show" makes).
#[derive(Clone, Debug, PartialEq)]
pub struct Narration {
    /// Key into [`crate::Presentation::media`].
    pub media: String,
}
