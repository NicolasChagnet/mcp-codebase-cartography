"""MCP stdio server entrypoint for mcp-codebase-cartography.

stdout is reserved for MCP protocol messages; nothing else is written to it.
"""

import logging
import os
import sys

from mcp.server import MCPServer
from mcp_codebase_cartography import _native

logger = logging.getLogger(__name__)
logging.basicConfig(stream=sys.stderr, level=logging.INFO, format="%(message)s")


def build_server(root: str | None = None) -> MCPServer:
    """Create the MCP server with all eight documented tools.

    A single native ``Engine`` is instantiated over the repository root (the
    process working directory by default) and shared by every tool handler.
    Native failures raise Python exceptions, which the MCP SDK surfaces to the
    client as ``isError`` tool results.
    """
    engine = _native.Engine(root or os.getcwd())
    server = MCPServer("mcp-codebase-cartography")

    @server.tool()
    async def get_codebase_map(max_depth: int = 2) -> dict:
        """Return the root folder directory tree as a structured object, filtering out ignored files."""
        return engine.get_codebase_map(max_depth)

    @server.tool()
    async def get_compressed_file(file_path: str) -> str:
        """Return a file's imports, type declarations, docstrings, and function signatures."""
        return engine.get_compressed_file(file_path)

    @server.tool()
    async def search_codebase(
        pattern: str, extension: str | None = None, max_results: int = 10
    ) -> list:
        """Run regex search over indexed files, returning matching files and snippets."""
        return engine.search_codebase(pattern, extension, max_results)

    @server.tool()
    async def get_file_outline(file_path: str) -> list:
        """Return classes, functions, and interfaces with line ranges and AST node kinds."""
        return engine.get_file_outline(file_path)

    @server.tool()
    async def get_symbol_definition(
        symbol_name: str, file_path: str | None = None
    ) -> str:
        """Fetch the implementation code for a single symbol by name."""
        return engine.get_symbol_definition(symbol_name, file_path)

    @server.tool()
    async def get_upstream_refs(symbol_name: str) -> list:
        """Return the reference sites for a symbol: every file and line where the queried symbol is referenced, called, or used. These are the symbol's direct dependents/callers. Paths are workspace-relative. Resolution is name-based/lexical; if the name is defined in multiple files, an ambiguity error is raised."""
        return engine.get_upstream_refs(symbol_name)

    @server.tool()
    async def get_downstream_refs(symbol_key: str, max_depth: int = 2) -> dict:
        """Return the transitive callers/impact of a symbol: the symbols and files that depend on, call, or reference the queried symbol, traversed up to max_depth hops. Returns caller records and impact paths. Paths are workspace-relative. Resolution is name-based/lexical; pass `file:name` to disambiguate."""
        return engine.get_downstream_refs(symbol_key, max_depth)

    @server.tool()
    async def get_ast_diff(base_ref: str = "HEAD") -> list:
        """Summarize code changes structurally across commits or uncommitted working trees."""
        return engine.get_ast_diff(base_ref)

    return server


def main() -> None:
    """Run the MCP server over stdio until the client disconnects."""
    logger.info("Starting codebase cartography MCP server...")
    build_server().run(transport="stdio")


if __name__ == "__main__":
    main()
