//! Rust/Python bridge for `mcp-codebase-cartography`.
//!
//! This is the narrow seam between the Python MCP server and the Rust core.
//! It exposes an [`Engine`] that owns an indexed codebase root and will back
//! the core operations invoked by the MCP tools. The core crate is wired in
//! by a later step; this module only defines the bridge surface.

use pyo3::prelude::*;

/// In-process indexed engine over a codebase root.
///
/// Constructed with the repository root path; core operations are invoked on
/// it from Python. The engine is intentionally narrow here and gains the
/// actual core-backed operations in a later step.
#[pyclass]
struct Engine {
    root: String,
}

#[pymethods]
impl Engine {
    /// Create an engine rooted at `root`.
    #[new]
    fn new(root: String) -> Self {
        Engine { root }
    }

    /// Absolute path of the repository root this engine indexes.
    fn root(&self) -> String {
        self.root.clone()
    }
}

/// The compiled extension module, importable as `mcp_codebase_cartography._native`.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    Ok(())
}
