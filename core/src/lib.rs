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
pub mod index;
pub mod languages;
pub mod paths;
pub mod read;
pub mod search;
pub mod symbols;
pub mod tree;

pub use ast::Symbol;
pub use compress::{get_compressed_file, CompressError};
pub use diff::{get_ast_diff, ChangeStatus, DiffError, SymbolChange};
pub use index::{Engine, FileRecord, ReadError};
pub use languages::Language;
pub use paths::{PathError, RepoRoot};
pub use read::{read_file_range, ReadRangeError};
pub use search::{search_codebase, SearchError, SearchMatch};
pub use symbols::{get_file_outline, get_symbol_definition, SymbolError};
pub use tree::{get_codebase_map, TreeError};
