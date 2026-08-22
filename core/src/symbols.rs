//! Symbol lookup tools: file outlines and symbol definitions.

use std::path::Path;

use crate::ast::{self, Symbol};
use crate::index::{Engine, ReadError};
use crate::languages::Language;
use crate::paths::PathError;

/// Errors from symbol lookup.
#[derive(Debug)]
pub enum SymbolError {
    Path(PathError),
    Read(ReadError),
    Io(std::io::Error),
    /// The file's language is not supported.
    Unsupported,
    /// No symbol with the requested name was found.
    NotFound,
    /// The name matched symbols in multiple files; `files` lists them.
    Ambiguous {
        files: Vec<String>,
    },
}

impl std::fmt::Display for SymbolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolError::Path(e) => write!(f, "{e}"),
            SymbolError::Read(e) => write!(f, "{e}"),
            SymbolError::Io(e) => write!(f, "{e}"),
            SymbolError::Unsupported => write!(f, "unsupported file type"),
            SymbolError::NotFound => write!(f, "symbol not found"),
            SymbolError::Ambiguous { files } => {
                write!(f, "symbol found in multiple files: {}", files.join(", "))
            }
        }
    }
}

impl std::error::Error for SymbolError {}

impl From<PathError> for SymbolError {
    fn from(e: PathError) -> Self {
        SymbolError::Path(e)
    }
}

impl From<ReadError> for SymbolError {
    fn from(e: ReadError) -> Self {
        SymbolError::Read(e)
    }
}

impl From<std::io::Error> for SymbolError {
    fn from(e: std::io::Error) -> Self {
        SymbolError::Io(e)
    }
}

/// Return the outline (all symbols with kinds and line ranges) of a file.
pub fn get_file_outline(engine: &mut Engine, file_path: &Path) -> Result<Vec<Symbol>, SymbolError> {
    let rel = engine.resolve(file_path)?;
    let lang = Language::from_path(&rel).ok_or(SymbolError::Unsupported)?;
    let bytes = engine.read_file(file_path)?;
    let source = String::from_utf8_lossy(&bytes);
    Ok(ast::parse(lang, &source))
}

/// Return the exact source span of a symbol by name. When `file_path` is
/// given, searches only that file; otherwise searches all indexed files.
///
/// Ambiguity is handled deterministically: within a single file the first
/// match by line is returned; a name matching symbols in multiple files is an
/// error listing the candidate files.
pub fn get_symbol_definition(
    engine: &mut Engine,
    symbol_name: &str,
    file_path: Option<&Path>,
) -> Result<String, SymbolError> {
    let mut matches = collect_matches(engine, file_path, symbol_name)?;
    if matches.is_empty() {
        return Err(SymbolError::NotFound);
    }
    // Deterministic order: by file, then by line.
    matches.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.line_start.cmp(&b.1.line_start)));

    let first_file = &matches[0].0;
    if matches.iter().any(|(f, _, _)| f != first_file) {
        let mut files: Vec<String> = matches.iter().map(|(f, _, _)| f.clone()).collect();
        files.sort();
        files.dedup();
        return Err(SymbolError::Ambiguous { files });
    }

    let (_, sym, source) = matches.remove(0);
    Ok(source[sym.start_byte..sym.end_byte].to_string())
}

/// Collect `(file, symbol, source)` matches for `name` across the given file
/// (or all indexed files when `file_path` is `None`).
fn collect_matches(
    engine: &mut Engine,
    file_path: Option<&Path>,
    name: &str,
) -> Result<Vec<(String, Symbol, String)>, SymbolError> {
    let files: Vec<String> = match file_path {
        Some(p) => vec![engine.resolve(p)?],
        None => engine.list_files()?.into_iter().map(|r| r.path).collect(),
    };
    let mut out = Vec::new();
    for rel in files {
        let Some(lang) = Language::from_path(&rel) else {
            continue;
        };
        let bytes = engine.read_file(Path::new(&rel))?;
        let source = String::from_utf8_lossy(&bytes).into_owned();
        for sym in ast::parse(lang, &source) {
            if sym.name == name {
                out.push((rel.clone(), sym, source.clone()));
            }
        }
    }
    Ok(out)
}
