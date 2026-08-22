//! The shared repository index engine.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::cache::{Cache, FileMeta};
use crate::paths::{PathError, RepoRoot};

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
    symbols: HashMap<String, Vec<String>>,
}

impl Engine {
    /// Create an engine rooted at `root`, discovering the repository root.
    pub fn new(root: &Path) -> std::io::Result<Self> {
        let root = RepoRoot::discover(root)?;
        Ok(Engine {
            root,
            ignored: DEFAULT_IGNORED.iter().map(|s| s.to_string()).collect(),
            cache: Cache::new(),
            symbols: HashMap::new(),
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
    /// (sorted) order.
    pub fn list_files(&self) -> std::io::Result<Vec<FileRecord>> {
        let mut records = Vec::new();
        self.walk(self.root.path(), &mut records)?;
        records.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(records)
    }

    fn walk(&self, dir: &Path, out: &mut Vec<FileRecord>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if self.ignored.iter().any(|i| i == &name) {
                continue;
            }
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() {
                self.walk(&path, out)?;
            } else if ft.is_file()
                && let Ok(rel) = self.root.resolve_relative(&path)
            {
                let meta = entry.metadata()?;
                out.push(FileRecord {
                    path: rel,
                    meta: FileMeta {
                        len: meta.len(),
                        modified: meta.modified()?,
                    },
                });
            }
        }
        Ok(())
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

    /// Store symbol -> reference locations.
    pub fn set_symbol_refs(&mut self, symbol: &str, refs: Vec<String>) {
        self.symbols.insert(symbol.to_string(), refs);
    }

    /// Look up stored references for a symbol.
    pub fn symbol_refs(&self, symbol: &str) -> Option<&[String]> {
        self.symbols.get(symbol).map(Vec::as_slice)
    }
}
