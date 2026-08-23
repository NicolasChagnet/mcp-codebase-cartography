//! The shared repository index engine.

use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::cache::{Cache, FileMeta, Snapshot};
use crate::graph::SymbolGraph;
use crate::paths::{PathError, RepoRoot};
use crate::refs::RefError;

/// Default directory/file names that are always ignored.
const DEFAULT_IGNORED: &[&str] = &[
    ".git",
    ".jj",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "build",
    "dist",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".vscode",
    ".opencode",
    ".DS_Store",
];

/// A parsed file record: root-relative path plus metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    /// Root-relative path with `/` separators.
    pub path: String,
    pub meta: FileMeta,
}

/// Errors from reading a file.
#[derive(Debug)]
pub enum ReadError {
    Path(PathError),
    Io(std::io::Error),
    Binary,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Path(e) => write!(f, "{e}"),
            ReadError::Io(e) => write!(f, "{e}"),
            ReadError::Binary => write!(f, "binary file"),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<PathError> for ReadError {
    fn from(e: PathError) -> Self {
        ReadError::Path(e)
    }
}

/// The in-process index engine over a repository root.
pub struct Engine {
    root: RepoRoot,
    ignored: Vec<String>,
    cache: Cache<Vec<u8>>,
    /// Cached file inventory, refreshed when the on-disk state changes.
    files: Mutex<Snapshot<Vec<FileRecord>>>,
    /// Lazy symbol graph, invalidated whenever the file inventory changes.
    graph: Mutex<Snapshot<SymbolGraph>>,
}

impl Engine {
    /// Create an engine rooted at `root`, discovering the repository root.
    pub fn new(root: &Path) -> std::io::Result<Self> {
        let root = RepoRoot::discover(root)?;
        Ok(Engine {
            root,
            ignored: DEFAULT_IGNORED.iter().map(|s| s.to_string()).collect(),
            cache: Cache::new(),
            files: Mutex::new(Snapshot::new()),
            graph: Mutex::new(Snapshot::new()),
        })
    }

    /// The absolute repository root path.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// Resolve a path to a canonical root-relative path, rejecting escapes.
    pub fn resolve(&self, path: &Path) -> Result<String, PathError> {
        self.root.resolve_relative(path)
    }

    /// List all non-ignored regular files under the root, in deterministic
    /// (sorted) order. Traversal uses the `ignore` crate so `.gitignore`,
    /// hidden files, and standard ignored directories are handled
    /// consistently. The result is cached and refreshed only when the
    /// on-disk file set or metadata changes.
    pub fn list_files(&self) -> std::io::Result<Vec<FileRecord>> {
        self.files()
    }

    /// Walk the filesystem and return the current file snapshot.
    fn walk_files(&self) -> std::io::Result<Vec<FileRecord>> {
        let mut records = Vec::new();
        let ignored = self.ignored.clone();
        let mut walker = ignore::WalkBuilder::new(self.root.path());
        walker
            // Apply gitignore rules even outside a git repo (e.g. tests).
            .require_git(false)
            // Skip standard ignored directories by name without descending.
            .filter_entry(move |entry| {
                let name = entry.file_name();
                !ignored.iter().any(|i| name == std::ffi::OsStr::new(i))
            });
        for result in walker.build() {
            let entry = result.map_err(std::io::Error::other)?;
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let Ok(rel) = self.root.resolve_relative(entry.path()) else {
                continue;
            };
            let meta = entry.metadata().map_err(std::io::Error::other)?;
            records.push(FileRecord {
                path: rel,
                meta: FileMeta {
                    len: meta.len(),
                    modified: meta.modified()?,
                },
            });
        }
        records.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(records)
    }

    /// Return the current file snapshot, refreshing the cache when the
    /// on-disk state changed. Edits, additions, deletions, and ignore/config
    /// changes all surface as a different snapshot (paths and/or metadata),
    /// so the cached inventory is reused only while it stays accurate. When
    /// the snapshot changes, the symbol graph cache is invalidated so
    /// downstream queries never reuse stale state.
    fn files(&self) -> std::io::Result<Vec<FileRecord>> {
        let fresh = self.walk_files()?;
        let mut slot = self.files.lock().unwrap();
        if slot.get() != Some(&fresh) {
            slot.replace(fresh.clone());
            self.graph.lock().unwrap().clear();
        }
        Ok(fresh)
    }

    /// Load a file's bytes, using the cache and refreshing when metadata
    /// changes. Rejects binary files.
    pub fn read_file(&mut self, path: &Path) -> Result<Vec<u8>, ReadError> {
        let rel = self.resolve(path)?;
        let abs = self.root.path().join(&rel);
        let meta = fs::metadata(&abs).map_err(|_| ReadError::Path(PathError::NotFound))?;
        let file_meta = FileMeta {
            len: meta.len(),
            modified: meta.modified().map_err(ReadError::Io)?,
        };
        if let Some(bytes) = self.cache.get(&rel, &file_meta) {
            return Ok(bytes.clone());
        }
        let bytes = fs::read(&abs).map_err(ReadError::Io)?;
        if bytes.contains(&0) {
            return Err(ReadError::Binary);
        }
        self.cache.insert(rel, file_meta, bytes.clone());
        Ok(bytes)
    }

    /// Return the cached symbol graph, building it lazily and rebuilding when
    /// the file snapshot changes. Callers never build or invalidate the cache
    /// themselves.
    pub(crate) fn graph(&mut self) -> Result<MutexGuard<'_, Snapshot<SymbolGraph>>, RefError> {
        self.files()?;
        if self.graph.lock().unwrap().get().is_none() {
            let g = SymbolGraph::build(self)?;
            self.graph.lock().unwrap().replace(g);
        }
        Ok(self.graph.lock().unwrap())
    }
}
