//! Repository-root discovery and safe, root-relative path handling.

use std::path::{Path, PathBuf};

/// Structured error for path operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The path escapes the repository root.
    OutsideRoot,
    /// The path does not exist.
    NotFound,
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::OutsideRoot => write!(f, "path escapes the repository root"),
            PathError::NotFound => write!(f, "path does not exist"),
        }
    }
}

impl std::error::Error for PathError {}

/// A discovered repository root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRoot {
    root: PathBuf,
}

impl RepoRoot {
    /// Discover the repository root by walking up from `start` looking for a
    /// VCS marker (`.git` or `.jj`). Falls back to `start` itself when none
    /// is found.
    pub fn discover(start: &Path) -> std::io::Result<Self> {
        let start = start.canonicalize()?;
        let mut dir = Some(start.as_path());
        while let Some(d) = dir {
            if d.join(".git").exists() || d.join(".jj").exists() {
                return Ok(RepoRoot {
                    root: d.to_path_buf(),
                });
            }
            dir = d.parent();
        }
        Ok(RepoRoot { root: start })
    }

    /// The absolute root path.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Resolve `path` to a canonical, root-relative path with `/` separators.
    /// Rejects paths that escape the root or do not exist.
    pub fn resolve_relative(&self, path: &Path) -> Result<String, PathError> {
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        let canonical = joined.canonicalize().map_err(|_| PathError::NotFound)?;
        let rel = canonical
            .strip_prefix(&self.root)
            .map_err(|_| PathError::OutsideRoot)?;
        Ok(rel.to_string_lossy().replace('\\', "/"))
    }
}
