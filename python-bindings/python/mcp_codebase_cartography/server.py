"""MCP stdio server entrypoint for mcp-codebase-cartography.

stdout is reserved for MCP protocol messages; nothing else is written to it.
"""

import logging
import os
import sys

from mcp_codebase_cartography import _native
from mcp.server import MCPServer

logger = logging.getLogger(__name__)
logging.basicConfig(stream=sys.stderr, level=logging.INFO, format="%(message)s")


def build_server(root: str | None = None) -> MCPServer:
    """Create the MCP server with all nine documented tools.

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
    async def read_file_range(file_path: str, start_line: int, end_line: int) -> str:
        """Read a slice of lines from a specific file with relative line numbers."""
        return engine.read_file_range(file_path, start_line, end_line)

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
        """Find all spots referencing this symbol in the codebase."""
        return engine.get_upstream_refs(symbol_name)

    @server.tool()
    async def get_downstream_refs(symbol_key: str, max_depth: int = 2) -> dict:
        """List all downstream callers up to N steps deep."""
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
