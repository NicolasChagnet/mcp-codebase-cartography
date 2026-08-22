//! Compressed file view: imports plus declaration signatures with bodies
//! stripped to line counts.

use std::fmt::Write;
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

/// Return a compact view of `file_path`: its imports, then each declaration's
/// signature with the body logic stripped and replaced by a line count.
pub fn get_compressed_file(engine: &mut Engine, file_path: &Path) -> Result<String, CompressError> {
    let rel = engine.resolve(file_path)?;
    let lang = Language::from_path(&rel).ok_or(CompressError::Unsupported)?;
    let bytes = engine.read_file(file_path)?;
    let source = String::from_utf8_lossy(&bytes);
    let symbols = ast::parse(lang, &source);

    let mut out = String::new();
    out.push_str(&rel);
    out.push('\n');

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
    if !imports.is_empty() {
        out.push_str("imports:\n");
        for imp in &imports {
            let _ = writeln!(out, "  {imp}");
        }
    }

    out.push_str("symbols:\n");
    for sym in &symbols {
        let sig = signature(&source, sym);
        let body_lines = sym.line_end.saturating_sub(sym.line_start);
        let _ = writeln!(out, "  {sig}   // [Body hidden: {body_lines} lines]");
    }
    Ok(out)
}

/// The declaration's signature: the first line of its source, trimmed.
fn signature(source: &str, sym: &ast::Symbol) -> String {
    let span = &source[sym.start_byte..sym.end_byte];
    span.lines().next().unwrap_or("").trim().to_string()
}
