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
        let span = &source[sym.start_byte..sym.end_byte];
        let mut lines = span.lines();
        // The signature is the declaration's first line, which carries the
        // opening delimiter for brace languages.
        let sig = lines.next().unwrap_or("").trim().to_string();
        let rest: Vec<&str> = lines.collect();
        if rest.is_empty() {
            // One-line declaration: both delimiters already sit on the
            // signature line, so there is no interior to hide.
            let _ = writeln!(out, "  {sig}   // [Body hidden: 0 lines]");
        } else if let Some(closing) = closing_delimiter(span) {
            // Brace language: retain the closing delimiter and hide the
            // interior between the signature and it. The delimiter may share
            // its line with the last statement, so the hidden count comes from
            // the interior text rather than the declaration span.
            let interior = &span[sig_end(span)..closing.pos];
            let hidden = interior.lines().count();
            let _ = writeln!(out, "  {sig}   // [Body hidden: {hidden} lines]");
            let _ = writeln!(out, "  {}", closing.text);
        } else {
            // No closing delimiter (e.g. Python): hide the whole interior.
            let _ = writeln!(out, "  {sig}   // [Body hidden: {} lines]", rest.len());
        }
    }
    Ok(out)
}

/// The byte offset just past the declaration's first line.
fn sig_end(span: &str) -> usize {
    span.find('\n').map(|i| i + 1).unwrap_or(span.len())
}

/// A block-closing delimiter found at the end of a brace-language span.
struct Closing<'a> {
    /// Byte offset of the delimiter within the span.
    pos: usize,
    /// The delimiter text (e.g. `}`).
    text: &'a str,
}

/// The trailing block-closing delimiter (`}`) of a brace-language span, or
/// `None` for languages without block braces (e.g. Python), whose spans end in
/// a statement rather than a delimiter.
fn closing_delimiter(span: &str) -> Option<Closing<'_>> {
    let trimmed = span.trim_end();
    let last = trimmed.chars().last()?;
    if last != '}' {
        return None;
    }
    let pos = trimmed.len() - last.len_utf8();
    Some(Closing { pos, text: &trimmed[pos..] })
}
