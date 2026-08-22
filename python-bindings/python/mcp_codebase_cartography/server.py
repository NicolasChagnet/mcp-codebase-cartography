"""MCP stdio server entrypoint for mcp-codebase-cartography.

stdout is reserved for MCP protocol messages; nothing else is written to it.
"""

import anyio
from mcp.server import Server
from mcp.server.stdio import stdio_server


def main() -> None:
    """Run the MCP server over stdio until the client disconnects."""
    server = Server("mcp-codebase-cartography")

    async def _run() -> None:
        async with stdio_server() as (read_stream, write_stream):
            await server.run(
                read_stream,
                write_stream,
                server.create_initialization_options(),
            )

    anyio.run(_run)


if __name__ == "__main__":
    main()
