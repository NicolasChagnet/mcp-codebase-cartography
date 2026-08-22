//! Regex search over indexed files using the ripgrep backend.

use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, Sink, SinkMatch};

use crate::index::{Engine, ReadError};
use crate::paths::PathError;

/// Errors from searching the codebase.
#[derive(Debug)]
pub enum SearchError {
    Path(PathError),
    Read(ReadError),
    Io(std::io::Error),
    /// The pattern is not a valid regex.
    InvalidRegex(String),
    /// The extension filter is malformed (empty or contains a path separator).
    InvalidExtension,
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::Path(e) => write!(f, "{e}"),
            SearchError::Read(e) => write!(f, "{e}"),
            SearchError::Io(e) => write!(f, "{e}"),
            SearchError::InvalidRegex(e) => write!(f, "invalid regex: {e}"),
            SearchError::InvalidExtension => write!(f, "invalid extension filter"),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<PathError> for SearchError {
    fn from(e: PathError) -> Self {
        SearchError::Path(e)
    }
}

impl From<ReadError> for SearchError {
    fn from(e: ReadError) -> Self {
        SearchError::Read(e)
    }
}

impl From<std::io::Error> for SearchError {
    fn from(e: std::io::Error) -> Self {
        SearchError::Io(e)
    }
}

/// A single match: file, 1-indexed line number, and the matching line text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub file: String,
    pub line: usize,
    pub text: String,
}

/// Collects matches for one file, stopping once `max` is reached.
struct FileSink {
    file: String,
    matches: Vec<SearchMatch>,
    max: usize,
}

impl Sink for FileSink {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        if self.matches.len() >= self.max {
            return Ok(false);
        }
        let line = mat.lines().next().unwrap_or_default();
        self.matches.push(SearchMatch {
            file: self.file.clone(),
            line: mat.line_number().unwrap_or(0) as usize,
            text: String::from_utf8_lossy(line).into_owned(),
        });
        Ok(true)
    }
}

/// Normalize an extension filter: strip a leading dot and reject path
/// separators or empty values.
fn normalize_extension(ext: &str) -> Result<String, SearchError> {
    let e = ext.strip_prefix('.').unwrap_or(ext);
    if e.is_empty() || e.contains('/') || e.contains('\\') {
        return Err(SearchError::InvalidExtension);
    }
    Ok(e.to_ascii_lowercase())
}

/// Search indexed files for `pattern`, returning up to `max_results` matches
/// in deterministic (file, then line) order. `extension` optionally restricts
/// the search to files with that extension. Traversal reuses the engine's
/// `ignore`-based file listing, so `.gitignore` and ignored directories are
/// respected.
pub fn search_codebase(
    engine: &mut Engine,
    pattern: &str,
    extension: Option<&str>,
    max_results: usize,
) -> Result<Vec<SearchMatch>, SearchError> {
    let matcher =
        RegexMatcher::new(pattern).map_err(|e| SearchError::InvalidRegex(e.to_string()))?;
    let ext = match extension {
        Some(e) => Some(normalize_extension(e)?),
        None => None,
    };
    let mut searcher = Searcher::new();
    let mut results = Vec::new();
    for rec in engine.list_files()? {
        if results.len() >= max_results {
            break;
        }
        if let Some(ext) = &ext {
            let file_ext = rec.path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            if &file_ext != ext {
                continue;
            }
        }
        let abs = engine.root().join(&rec.path);
        let bytes = engine.read_file(&abs)?;
        let mut sink = FileSink {
            file: rec.path.clone(),
            matches: Vec::new(),
            max: max_results - results.len(),
        };
        searcher
            .search_slice(&matcher, &bytes, &mut sink)
            .map_err(|e| SearchError::Io(std::io::Error::other(e)))?;
        results.extend(sink.matches);
    }
    Ok(results)
}
