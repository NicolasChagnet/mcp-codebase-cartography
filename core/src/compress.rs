//! Structured file view: imports plus declaration metadata and signatures.

use std::path::Path;

use crate::ast;
use crate::index::{Engine, ReadError};
use crate::languages::Language;
use crate::paths::PathError;

/// Errors from compressing a file.
#[derive(Debug)]
pub enum CompressError {
    Path(PathError),
    Read(ReadError),
    /// The file's language is not supported.
    Unsupported,
}

impl std::fmt::Display for CompressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressError::Path(e) => write!(f, "{e}"),
            CompressError::Read(e) => write!(f, "{e}"),
            CompressError::Unsupported => write!(f, "unsupported file type"),
        }
    }
}

impl std::error::Error for CompressError {}

impl From<PathError> for CompressError {
    fn from(e: PathError) -> Self {
        CompressError::Path(e)
    }
}

impl From<ReadError> for CompressError {
    fn from(e: ReadError) -> Self {
        CompressError::Read(e)
    }
}

/// Common import-statement prefixes, used to surface imports in the compact
/// view across the supported languages.
const IMPORT_PREFIXES: &[&str] = &[
    "import ",
    "use ",
    "from ",
    "#include",
    "require",
    "using ",
    "package ",
    "extern crate",
    "include ",
    "import_module",
];

fn is_import(line: &str) -> bool {
    let t = line.trim_start();
    IMPORT_PREFIXES.iter().any(|p| t.starts_with(p))
}

/// A single declaration's metadata and signature line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRecord {
    pub name: String,
    /// Canonical kind, e.g. `function`, `class`, `interface`, `method`.
    pub kind: String,
    /// 1-indexed first line of the declaration.
    pub line_start: usize,
    /// 1-indexed last line of the declaration.
    pub line_end: usize,
    /// The declaration's first line (its signature).
    pub signature: String,
}

/// The structured view of a file: its path, unique imports, and symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStructure {
    /// Workspace-relative path of the file.
    pub path: String,
    /// Unique import-like lines in document order.
    pub imports: Vec<String>,
    /// Declarations in document order.
    pub symbols: Vec<SymbolRecord>,
}

/// Return the structured view of `file_path`: its workspace-relative path,
/// unique imports, and each declaration's metadata and signature.
pub fn get_file_structure(
    engine: &mut Engine,
    file_path: &Path,
) -> Result<FileStructure, CompressError> {
    let rel = engine.resolve(file_path)?;
    let lang = Language::from_path(&rel).ok_or(CompressError::Unsupported)?;
    let bytes = engine.read_file(file_path)?;
    let source = String::from_utf8_lossy(&bytes);
    let symbols = ast::parse(lang, &source);

    // Imports: unique import-like lines in document order.
    let mut imports: Vec<String> = Vec::new();
    for line in source.lines() {
        if is_import(line) {
            let t = line.trim();
            if !imports.contains(&t.to_string()) {
                imports.push(t.to_string());
            }
        }
    }

    let symbols = symbols
        .into_iter()
        .map(|sym| {
            let span = &source[sym.start_byte..sym.end_byte];
            // The signature is the declaration's first line, which carries the
            // opening delimiter for brace languages.
            let signature = span.lines().next().unwrap_or("").trim().to_string();
            SymbolRecord {
                name: sym.name,
                kind: sym.kind,
                line_start: sym.line_start,
                line_end: sym.line_end,
                signature,
            }
        })
        .collect();

    Ok(FileStructure {
        path: rel,
        imports,
        symbols,
    })
}
