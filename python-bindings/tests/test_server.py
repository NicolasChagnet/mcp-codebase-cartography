"""Contract tests for the MCP server tool registration.

These verify the advertised tool surface (names, defaults, optional arguments,
and representative response shapes) without re-testing the Rust core behavior.
"""

import json
import os

import pytest
from mcp.server.mcpserver.exceptions import ToolError
from mcp_codebase_cartography.server import build_server

# Repository root (parent of python-bindings) so the engine has files to index.
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

EXPECTED_TOOLS = {
    "get_codebase_map",
    "get_compressed_file",
    "search_codebase",
    "get_file_outline",
    "get_symbol_definition",
    "get_upstream_refs",
    "get_downstream_refs",
    "get_ast_diff",
}


@pytest.fixture(scope="module")
def server():
    return build_server(root=REPO_ROOT)


@pytest.mark.anyio
async def test_all_eight_tools_registered(server):
    tools = await server.list_tools()
    names = {t.name for t in tools}
    assert names == EXPECTED_TOOLS


@pytest.mark.anyio
async def test_tool_defaults_and_optional_args(server):
    tools = {t.name: t.input_schema for t in await server.list_tools()}

    assert tools["get_codebase_map"]["properties"]["max_depth"]["default"] == 2

    assert tools["search_codebase"]["properties"]["max_results"]["default"] == 10
    assert "extension" not in tools["search_codebase"]["required"]

    assert "file_path" not in tools["get_symbol_definition"]["required"]

    assert tools["get_downstream_refs"]["properties"]["max_depth"]["default"] == 2

    assert tools["get_ast_diff"]["properties"]["base_ref"]["default"] == "HEAD"


@pytest.mark.anyio
async def test_reference_tool_descriptions_explain_direction(server):
    tools = {t.name: t.description for t in await server.list_tools()}

    upstream = tools["get_upstream_refs"]
    assert "referenced, called, or used" in upstream
    assert "reference sites" in upstream
    assert "workspace-relative" in upstream
    assert "ambiguity" in upstream

    downstream = tools["get_downstream_refs"]
    assert "depend on, call, or reference" in downstream
    assert "max_depth" in downstream
    assert "transitive" in downstream
    assert "workspace-relative" in downstream


@pytest.mark.anyio
async def test_get_codebase_map_returns_tree(server):
    result = await server.call_tool("get_codebase_map", {"max_depth": 1})
    payload = json.loads(result.content[0].text)
    assert payload["kind"] == "dir"
    names = {n["name"] for n in payload["children"]}
    assert "core" in names
    assert "python-bindings" in names


@pytest.mark.anyio
async def test_get_file_outline_returns_json_list(server):
    result = await server.call_tool(
        "get_file_outline", {"file_path": "core/src/index.rs"}
    )
    # The SDK emits one content block per list element.
    payload = [json.loads(block.text) for block in result.content]
    assert payload, "expected at least one symbol in core/src/index.rs"
    assert {"name", "kind", "line_start", "line_end"} <= set(payload[0])


@pytest.mark.anyio
async def test_native_failure_surfaces_as_error(server):
    # Native failures are wrapped in a ToolError (an MCPError), which the SDK
    # surfaces to the client as a JSON-RPC error rather than a successful result.
    with pytest.raises(ToolError, match="path does not exist"):
        await server.call_tool(
            "get_compressed_file", {"file_path": "does/not/exist.rs"}
        )
