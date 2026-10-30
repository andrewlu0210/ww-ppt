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
