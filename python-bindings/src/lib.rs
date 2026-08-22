//! Rust/Python bridge for `mcp-codebase-cartography`.
//!
//! This is the narrow seam between the Python MCP server and the Rust core.
//! It exposes an [`Engine`] that owns a [`cartography_core::Engine`] over an indexed
//! codebase root and backs the core operations invoked by the MCP tools.
//! Core structs/enums are converted to Python strings, lists, and
//! dictionaries matching the documented JSON shapes, and core errors are
//! surfaced as Python exceptions rather than panics.

use std::path::Path;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use cartography_core::diff::ChangeStatus;
use cartography_core::index::Engine as CoreEngine;

/// In-process indexed engine over a codebase root.
///
/// Constructed with the repository root path; core operations are invoked on
/// it from Python. Mutable access is confined to each method call.
#[pyclass]
struct Engine {
    engine: CoreEngine,
}

#[pymethods]
impl Engine {
    /// Create an engine rooted at `root`, discovering the repository root.
    #[new]
    fn new(root: String) -> PyResult<Self> {
        let engine = CoreEngine::new(Path::new(&root))
            .map_err(|e| PyRuntimeError::new_err(format!("failed to initialize engine: {e}")))?;
        Ok(Engine { engine })
    }

    /// Absolute path of the repository root this engine indexes.
    fn root(&self) -> String {
        self.engine.root().to_string_lossy().into_owned()
    }

    /// Return the root folder directory tree, filtering out ignored files.
    fn get_codebase_map(&self, max_depth: usize) -> PyResult<String> {
        cartography_core::tree::get_codebase_map(&self.engine, max_depth)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Return a compact view of a file: imports plus declaration signatures.
    fn get_compressed_file(&mut self, file_path: &str) -> PyResult<String> {
        cartography_core::compress::get_compressed_file(&mut self.engine, Path::new(file_path))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Read lines `start_line..=end_line` (1-indexed) with relative numbers.
    fn read_file_range(
        &mut self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> PyResult<String> {
        cartography_core::read::read_file_range(
            &mut self.engine,
            Path::new(file_path),
            start_line,
            end_line,
        )
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Regex search over indexed files, returning up to `max_results` matches.
    #[pyo3(signature = (pattern, extension=None, max_results=10))]
    fn search_codebase(
        &mut self,
        py: Python<'_>,
        pattern: &str,
        extension: Option<&str>,
        max_results: usize,
    ) -> PyResult<PyObject> {
        let matches = cartography_core::search::search_codebase(
            &mut self.engine,
            pattern,
            extension,
            max_results,
        )
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        search_matches_to_py(py, &matches)
    }

    /// Return the outline (name, kind, line range) of a file's symbols.
    fn get_file_outline(&mut self, py: Python<'_>, file_path: &str) -> PyResult<PyObject> {
        let symbols =
            cartography_core::symbols::get_file_outline(&mut self.engine, Path::new(file_path))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        symbols_to_py(py, &symbols)
    }

    /// Return the exact source span of a symbol by name.
    #[pyo3(signature = (symbol_name, file_path=None))]
    fn get_symbol_definition(
        &mut self,
        symbol_name: &str,
        file_path: Option<&str>,
    ) -> PyResult<String> {
        cartography_core::symbols::get_symbol_definition(
            &mut self.engine,
            symbol_name,
            file_path.map(Path::new),
        )
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// List all spots referencing `symbol_name` across the codebase.
    fn get_upstream_refs(&mut self, py: Python<'_>, symbol_name: &str) -> PyResult<PyObject> {
        let spots = cartography_core::refs::get_upstream_refs(&mut self.engine, symbol_name)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        ref_spots_to_py(py, &spots)
    }

    /// List downstream callers of `symbol_key` up to `max_depth` hops.
    fn get_downstream_refs(
        &mut self,
        py: Python<'_>,
        symbol_key: &str,
        max_depth: usize,
    ) -> PyResult<PyObject> {
        let result =
            cartography_core::refs::get_downstream_refs(&mut self.engine, symbol_key, max_depth)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        downstream_to_py(py, &result)
    }

    /// Summarize structural code changes against `base_ref`.
    fn get_ast_diff(&mut self, py: Python<'_>, base_ref: &str) -> PyResult<PyObject> {
        let changes = cartography_core::diff::get_ast_diff(&mut self.engine, base_ref)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        changes_to_py(py, &changes)
    }
}

/// Convert search matches to a list of `{file, line, text}` dicts.
fn search_matches_to_py(
    py: Python<'_>,
    matches: &[cartography_core::search::SearchMatch],
) -> PyResult<PyObject> {
    let out = PyList::empty(py);
    for m in matches {
        let d = PyDict::new(py);
        d.set_item("file", &m.file)?;
        d.set_item("line", m.line)?;
        d.set_item("text", &m.text)?;
        out.append(d)?;
    }
    Ok(out.into_any().unbind())
}

/// Convert symbols to a list of `{name, kind, line_start, line_end}` dicts.
fn symbols_to_py(py: Python<'_>, symbols: &[cartography_core::ast::Symbol]) -> PyResult<PyObject> {
    let out = PyList::empty(py);
    for s in symbols {
        let d = PyDict::new(py);
        d.set_item("name", &s.name)?;
        d.set_item("kind", &s.kind)?;
        d.set_item("line_start", s.line_start)?;
        d.set_item("line_end", s.line_end)?;
        out.append(d)?;
    }
    Ok(out.into_any().unbind())
}

/// Convert reference spots to a list of `{file, line, context}` dicts.
fn ref_spots_to_py(
    py: Python<'_>,
    spots: &[cartography_core::refs::RefSpot],
) -> PyResult<PyObject> {
    let out = PyList::empty(py);
    for s in spots {
        let d = PyDict::new(py);
        d.set_item("file", &s.file)?;
        d.set_item("line", s.line)?;
        d.set_item("context", &s.context)?;
        out.append(d)?;
    }
    Ok(out.into_any().unbind())
}

/// Convert a downstream result to `{callers: [...], paths: [...]}`.
fn downstream_to_py(
    py: Python<'_>,
    result: &cartography_core::refs::DownstreamResult,
) -> PyResult<PyObject> {
    let callers = PyList::empty(py);
    for c in &result.callers {
        let d = PyDict::new(py);
        d.set_item("symbol", &c.symbol)?;
        d.set_item("kind", &c.kind)?;
        d.set_item("file", &c.file)?;
        d.set_item("line_start", c.line_start)?;
        d.set_item("line_end", c.line_end)?;
        d.set_item("depth", c.depth)?;
        callers.append(d)?;
    }
    let paths = PyList::empty(py);
    for p in &result.paths {
        let d = PyDict::new(py);
        d.set_item("path", p.path.clone())?;
        paths.append(d)?;
    }
    let out = PyDict::new(py);
    out.set_item("callers", callers)?;
    out.set_item("paths", paths)?;
    Ok(out.into_any().unbind())
}

/// Convert symbol changes to a list of `{status, name, kind, file,
/// line_start, line_end, summary}` dicts.
fn changes_to_py(
    py: Python<'_>,
    changes: &[cartography_core::diff::SymbolChange],
) -> PyResult<PyObject> {
    let out = PyList::empty(py);
    for c in changes {
        let status = match c.status {
            ChangeStatus::Added => "added",
            ChangeStatus::Deleted => "deleted",
            ChangeStatus::Modified => "modified",
        };
        let d = PyDict::new(py);
        d.set_item("status", status)?;
        d.set_item("name", &c.name)?;
        d.set_item("kind", &c.kind)?;
        d.set_item("file", &c.file)?;
        d.set_item("line_start", c.line_start)?;
        d.set_item("line_end", c.line_end)?;
        d.set_item("summary", &c.summary)?;
        out.append(d)?;
    }
    Ok(out.into_any().unbind())
}

/// The compiled extension module, importable as `mcp_codebase_cartography._native`.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    Ok(())
}
