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
      "args": ["mcp-codebase-cartography", "serve"]
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

### Installing from PyPI

The package is published to PyPI. Run it without installing via `uvx`, or
install it with pip:

```sh
uvx mcp-codebase-cartography serve
pip install mcp-codebase-cartography
```

### Releasing

Releases are cut from semver tags. The version lives in
`python-bindings/Cargo.toml` (`package.version`) and is read dynamically by
`pyproject.toml`.

1. Bump `package.version` in `python-bindings/Cargo.toml`.
2. Run the quality gates (`cargo clippy`, `cargo test`, `uvx ruff check
   python-bindings`, `uv run pyrefly check python-bindings`).
3. Commit and push a matching tag: `git tag v0.1.0 && git push origin v0.1.0`
   (or `jj tag set v0.1.0`).

Pushing a `vMAJOR.MINOR.PATCH` tag triggers `.github/workflows/release.yml`,
which builds the sdist and wheels, verifies the tag matches the package
version, publishes to PyPI, and creates a GitHub release with the artifacts.

Prerequisite: PyPI [Trusted Publishing](https://docs.pypi.org/trusted-publishers/)
must be configured for the `pypi` GitHub environment so the workflow can
publish without a token.

### Repository root

The server indexes the repository root, discovered from the process working
directory by walking up until a VCS marker (`.git` or `.jj`) is found. Run the
server from the repository root (or set the working directory of the MCP
client to it) so the tools operate on the intended codebase.

## Tools

The various exposed tools by this server are:

- `get_codebase_map(max_depth: int = 2)`: Returns the root folder directory tree as a structured object, filtering out ignored files (.git, node_modules, build dirs). Each node has a `name`, workspace-relative `path`, `kind` (`dir` or `file`), and nested `children`; directories truncated by `max_depth` carry a `collapsed_entries` count of omitted entries. Gives the agent a macro overview of the structural layout.

- `get_file_structure(file_path: string)`: Preferred over standard file reading. Returns a file's structured view: its workspace-relative `path`, unique `imports`, and each declaration's metadata and `signature` (the declaration's first line). Returns `{path, imports: [string], symbols: [{name, kind, line_start, line_end, signature}]}`.

- `search_codebase(pattern: string, extension: string | undefined, max_results: int = 10)`: Runs regex search over indexed files using an ultra-fast in-memory/ripgrep backend. Returns matching files and snippets truncated to 1 line of context.

- `get_symbol_definition(symbol_name: string, file_path: string | undefined)`: Fetches the implementation code for a single symbol by name without returning the rest of the file. Returns the extracted source string of the AST node. Use this to drill into a symbol's body after locating it via `get_file_structure`.

- `get_downstream_refs(symbol_key: string, max_depth: int = 2)`: Traverses the reference graph to list the transitive callers and impact paths of a symbol — the symbols/files that depend on, call, or reference it — up to `max_depth` hops. Helps evaluate impact before modifying code. Returns `{callers: [{symbol, kind, file, line_start, line_end, depth}], paths: [{path}]}` with workspace-relative paths. `symbol_key` is a bare name or `file:name` to disambiguate; resolution is conservative and name-based (lexical) with ambiguity errors where applicable.

- `get_upstream_refs(symbol_name: string)`: Finds every reference site for this symbol — the files and lines where it is called, used, or referenced across the codebase. Upstream results are direct reference sites with workspace-relative paths and the surrounding source line as context. Returns `[{file, line, context}]`. Resolution is conservative and name-based (lexical) with ambiguity errors where applicable.

- `get_ast_diff(base_ref: string = "HEAD")`: Summarizes code changes structurally across commits or uncommitted working trees. Filters out formatting and whitespace changes. Returns a text summary listing modified, added, or deleted functions/classes.

  JJ is an optional diff backend, not an installation requirement. Git
  repositories (a `.git` directory) use Git without needing `jj`. Only
  JJ-backed repositories (a `.jj` directory) require the `jj` executable, and
  only when calling `get_ast_diff`; if it is missing, the tool returns a clear
  error. CI installs `jj` solely for integration coverage.
