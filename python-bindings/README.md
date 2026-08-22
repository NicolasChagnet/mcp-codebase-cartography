# `python-bindings`

Rust/Python bridge for `mcp-codebase-cartography`. A pyo3 extension
(`mcp_codebase_cartography._native`) exposes an `Engine` over the indexed
repository root and backs every MCP tool.

## Building

Requires [maturin](https://www.maturin.rs) and a Rust toolchain. From this
directory:

```sh
maturin develop --release
```

This builds the extension and installs it into the active virtual
environment. To produce a wheel instead:

```sh
maturin build --release
pip install target/wheels/mcp_codebase_cartography-*.whl
```

## Engine

`Engine(root)` discovers the repository root by walking up from `root` until a
VCS marker (`.git` or `.jj`) is found, then indexes it. `Engine.root()` returns
the absolute path of the discovered root.

## Exposed methods

The extension mirrors the MCP tool contract. Argument defaults match the
server registration:

- `get_codebase_map(max_depth: int = 2) -> str`
- `get_compressed_file(file_path: str) -> str`
- `read_file_range(file_path: str, start_line: int, end_line: int) -> str`
- `search_codebase(pattern: str, extension: str | None = None, max_results: int = 10) -> list`
- `get_file_outline(file_path: str) -> list`
- `get_symbol_definition(symbol_name: str, file_path: str | None = None) -> str`
- `get_upstream_refs(symbol_name: str) -> list`
- `get_downstream_refs(symbol_key: str, max_depth: int = 2) -> dict`
- `get_ast_diff(base_ref: str = "HEAD") -> list`

Result shapes are plain Python lists and dicts:

- `search_codebase` → `[{file, line, text}]`
- `get_file_outline` → `[{name, kind, line_start, line_end}]`
- `get_upstream_refs` → `[{file, line, context}]`
- `get_downstream_refs` → `{callers: [{symbol, kind, file, line_start, line_end, depth}], paths: [{path}]}`
- `get_ast_diff` → `[{status, name, kind, file, line_start, line_end, summary}]`
