//! Shared repository index and safe path layer for `mcp-codebase-cartography`.
//!
//! Provides repository-root discovery, ignored-file filtering, canonical
//! root-relative paths, file metadata/content loading, and a small on-demand
//! cache invalidated by file metadata. All paths are validated against the
//! repository root; anything escaping it is rejected with a structured error.

pub mod ast;
pub mod cache;
pub mod index;
pub mod languages;
pub mod paths;
pub mod symbols;

pub use ast::Symbol;
pub use index::{Engine, FileRecord, ReadError};
pub use languages::Language;
pub use paths::{PathError, RepoRoot};
pub use symbols::{get_file_outline, get_symbol_definition, SymbolError};
