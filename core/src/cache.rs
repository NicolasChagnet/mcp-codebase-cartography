//! Small in-process cache invalidated by file metadata.

use std::collections::HashMap;
use std::time::SystemTime;

/// Metadata snapshot used to detect file changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub len: u64,
    pub modified: SystemTime,
}

/// A cache keyed by path, storing a value plus the metadata it was loaded
/// from. Entries are refreshed on demand when the on-disk metadata changes.
pub struct Cache<V> {
    entries: HashMap<String, (FileMeta, V)>,
}

impl<V> Default for Cache<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> Cache<V> {
    pub fn new() -> Self {
        Cache {
            entries: HashMap::new(),
        }
    }

    /// Return a cached value if present and its metadata still matches.
    pub fn get(&self, key: &str, meta: &FileMeta) -> Option<&V> {
        match self.entries.get(key) {
            Some((cached_meta, value)) if cached_meta == meta => Some(value),
            _ => None,
        }
    }

    /// Insert or replace a value for `key`.
    pub fn insert(&mut self, key: String, meta: FileMeta, value: V) {
        self.entries.insert(key, (meta, value));
    }
}
