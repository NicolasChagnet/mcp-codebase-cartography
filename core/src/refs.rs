//! Reference graph tools: upstream reference spots and downstream callers.
//!
//! Resolution is name-based (lexical): a reference is matched to a definition
//! by identifier text. This is deliberately conservative for dynamic or
//! ambiguous languages (Python, JavaScript) where precise type-based
//! resolution is not available; a name defined in multiple files yields
//! multiple candidate edges rather than a guess.

use crate::graph::SymbolGraph;
use crate::index::{Engine, ReadError};
use crate::paths::PathError;

/// A single reference site: where a symbol name is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefSpot {
    pub file: String,
    pub line: usize,
    /// The trimmed source line at the reference site.
    pub context: String,
}

/// A caller found by downstream BFS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerRecord {
    pub symbol: String,
    pub kind: String,
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
    /// Hop distance from the queried symbol (1 = direct caller).
    pub depth: usize,
}

/// An impact path from the queried symbol outward through its callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactPath {
    /// Symbol keys from the root to the deepest caller, e.g.
    /// `["leaf", "mid", "root"]` means `root` calls `mid` calls `leaf`.
    pub path: Vec<String>,
}

/// Result of a downstream reference query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownstreamResult {
    pub callers: Vec<CallerRecord>,
    pub paths: Vec<ImpactPath>,
}

/// Errors from reference queries.
#[derive(Debug)]
pub enum RefError {
    Path(PathError),
    Read(ReadError),
    Io(std::io::Error),
    NotFound,
    Ambiguous { files: Vec<String> },
}

impl std::fmt::Display for RefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefError::Path(e) => write!(f, "{e}"),
            RefError::Read(e) => write!(f, "{e}"),
            RefError::Io(e) => write!(f, "{e}"),
            RefError::NotFound => write!(f, "symbol not found"),
            RefError::Ambiguous { files } => {
                write!(f, "symbol found in multiple files: {}", files.join(", "))
            }
        }
    }
}

impl std::error::Error for RefError {}

impl From<PathError> for RefError {
    fn from(e: PathError) -> Self {
        RefError::Path(e)
    }
}

impl From<ReadError> for RefError {
    fn from(e: ReadError) -> Self {
        RefError::Read(e)
    }
}

impl From<std::io::Error> for RefError {
    fn from(e: std::io::Error) -> Self {
        RefError::Io(e)
    }
}

/// Find all spots referencing `symbol_name` across the codebase.
///
/// Returns every identifier/call site whose text matches the symbol name,
/// with file, line, and the surrounding source line as context. These are the
/// symbol's direct dependents/callers (who references, calls, or uses it).
/// Paths are workspace-relative. Errors with [`RefError::NotFound`] if no
/// definition with that name exists.
pub fn get_upstream_refs(engine: &mut Engine, symbol_name: &str) -> Result<Vec<RefSpot>, RefError> {
    let graph = SymbolGraph::build(engine)?;
    if !graph.has_name(symbol_name) {
        return Err(RefError::NotFound);
    }
    Ok(graph.upstream_spots(symbol_name))
}

/// List all downstream callers of `symbol_key` up to `max_depth` hops.
///
/// Returns the transitive callers/impact of the queried symbol: the symbols
/// and files that depend on, call, or reference it, walked up to `max_depth`
/// hops. `symbol_key` is either a bare symbol name or `file:name` to
/// disambiguate. Performs a bounded BFS over the reference graph following
/// reverse edges (who references the symbol), returning caller records and
/// impact paths. Paths are workspace-relative.
pub fn get_downstream_refs(
    engine: &mut Engine,
    symbol_key: &str,
    max_depth: usize,
) -> Result<DownstreamResult, RefError> {
    let graph = SymbolGraph::build(engine)?;
    let key = graph.resolve_key(symbol_key)?;
    let (callers, paths) = graph.downstream(&key, max_depth);
    Ok(DownstreamResult { callers, paths })
}
