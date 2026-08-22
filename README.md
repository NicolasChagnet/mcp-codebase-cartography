# `mcp-codebase-cartography`

This repository hosts the code for an MCP server exposing various tools aimed at efficiently exploring codebases without wasting tokens, using AST-based tools and graph representations.

## Usage

MCP configuration `mcp.json`:

```json
{
  "servers": {
    "mcp-codebase-cartography": {
      "type": "stdio",
      "command": "uvx",
      "args": ["mcp-codebase-cartography"]
    }
  }
}
```

### Building from source

The server ships a native extension (`python-bindings/`) built with
[maturin](https://www.maturin.rs). From `python-bindings/`, build and install
it into the active virtual environment:

```sh
maturin develop --release
```

or build a wheel and install it:

```sh
maturin build --release
pip install target/wheels/mcp_codebase_cartography-*.whl
```

### Repository root

The server indexes the repository root, discovered from the process working
directory by walking up until a VCS marker (`.git` or `.jj`) is found. Run the
server from the repository root (or set the working directory of the MCP
client to it) so the tools operate on the intended codebase.

## Tools

The various exposed tools by this server are:

- `get_codebase_map(max_depth: int = 2)`: Returns the root folder directory tree, filtering out ignored files (.git, node_modules, build dirs). Shows file roles and structural layout to give the agent a macro overview.

- `get_compressed_file(file_path: string)`: Preferred over standard file reading. Returns a file's imports, type declarations, docstrings, and function signatures with body logic stripped and replaced by line counts (e.g., // [Body hidden: 45 lines]).

- `read_file_range(file_path: string, start_line: int, end_line: int)`: Reads a slice of lines from a specific file. Used after identifying exact method coordinates using `get_file_outline` or `get_compressed_file`. Returns a plaintext slice with relative line numbers.

- `search_codebase(pattern: string, extension: string | undefined, max_results: int = 10)`: Runs regex search over indexed files using an ultra-fast in-memory/ripgrep backend. Returns matching files and snippets truncated to 1 line of context.

- `get_file_outline(file_path: string)` Parses AST to return classes, functions, and interfaces along with their line ranges and AST node kinds. Returns a JSON list of symbols containing name, kind, line_start, and line_end.

- `get_symbol_definition(symbol_name: string, file_path: string | undefined)`: Fetches the implementation code for a single symbol by name without returning the rest of the file. Returns the extracted source string of the AST node.

- `get_downstream_refs(symbol_key: string, max_depth: int = 2)`: Performs a BFS search on the graph to list all downstream callers up to N steps deep. Helps evaluate impact before modifying code. Returns a list of caller symbols, file locations, and graph impact paths.

- `get_upstream_refs(symbol_name: string)`: Finds all spots referencing this symbol in the codebase. Returns a list of files, line numbers, and caller contexts.

- `get_ast_diff(base_ref: string = "HEAD")`: Summarizes code changes structurally across commits or uncommitted working trees. Filters out formatting and whitespace changes. Returns a text summary listing modified, added, or deleted functions/classes.
