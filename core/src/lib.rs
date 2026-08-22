//! Shared repository index and safe path layer for `mcp-codebase-cartography`.
//!
//! Provides repository-root discovery, ignored-file filtering, canonical
//! root-relative paths, file metadata/content loading, and a small on-demand
//! cache invalidated by file metadata. All paths are validated against the
//! repository root; anything escaping it is rejected with a structured error.

pub mod ast;
pub mod cache;
pub mod compress;
pub mod diff;
pub mod graph;
pub mod index;
pub mod languages;
pub mod paths;
pub mod read;
pub mod refs;
pub mod search;
pub mod symbols;
pub mod tree;

pub use ast::Symbol;
pub use compress::{CompressError, get_compressed_file};
pub use diff::{ChangeStatus, DiffError, SymbolChange, get_ast_diff};
pub use index::{Engine, FileRecord, ReadError};
pub use languages::Language;
pub use paths::{PathError, RepoRoot};
pub use read::{ReadRangeError, read_file_range};
pub use refs::{
    CallerRecord, DownstreamResult, ImpactPath, RefError, RefSpot, get_downstream_refs,
    get_upstream_refs,
};
pub use search::{SearchError, SearchMatch, search_codebase};
pub use symbols::{SymbolError, get_file_outline, get_symbol_definition};
pub use tree::{TreeError, get_codebase_map};
