"""Behavioral tests for the MCP tools against a controlled dummy repository.

These build a server over a temporary repository (with a ``.git`` marker so
root discovery cannot escape into the checkout) and exercise the tools through
``server.call_tool(...)``, asserting real content, bounds, and documented
result shapes. Git-history-dependent AST diff coverage stays in the existing
real-repository contract tests.
"""

import json

import pytest
from mcp.server.mcpserver.exceptions import ToolError
from mcp_codebase_cartography.server import build_server

APP_PY = """\
import os
from util import helper


class Greeter:
    def greet(self, name):
        return f"hello {name}"


def shared():
    return 1


def main():
    g = Greeter()
    print(g.greet("world"))
    return helper()
"""

UTIL_RS = """\
use std::fmt;

pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Point { x, y }
    }
}

pub fn helper() -> i32 {
    42
}

pub fn shared() -> i32 {
    1
}
"""


@pytest.fixture
def repo(tmp_path):
    """A small repository with source files plus ignored paths."""
    (tmp_path / ".git").mkdir()
    (tmp_path / ".gitignore").write_text("build/\n*.log\n")
    (tmp_path / "src").mkdir()
    (tmp_path / "src" / "app.py").write_text(APP_PY)
    (tmp_path / "src" / "util.rs").write_text(UTIL_RS)
    # Ignored files that must never surface in any result.
    (tmp_path / "build").mkdir()
    (tmp_path / "build" / "ignored.rs").write_text("pub fn ignored() {}\n")
    (tmp_path / "notes.log").write_text("debug noise\n")
    (tmp_path / "__pycache__").mkdir()
    (tmp_path / "__pycache__" / "app.pyc").write_bytes(b"\x00\x01")
    return tmp_path


@pytest.fixture
def server(repo):
    return build_server(root=str(repo))


def _list_blocks(result):
    """The SDK emits one content block per list element."""
    return [json.loads(b.text) for b in result.content]


@pytest.mark.anyio
async def test_codebase_map_excludes_ignored(server):
    text = (
        (await server.call_tool("get_codebase_map", {"max_depth": 2})).content[0].text
    )
    assert "src/" in text
    assert "app.py" in text
    assert "util.rs" in text
    assert "build" not in text
    assert "notes.log" not in text
    assert "__pycache__" not in text


@pytest.mark.anyio
async def test_compressed_file(server):
    text = (
        (await server.call_tool("get_compressed_file", {"file_path": "src/app.py"}))
        .content[0]
        .text
    )
    assert text.startswith("src/app.py")
    assert "imports:" in text
    assert "from util import helper" in text
    assert "symbols:" in text
    assert "def main" in text
    assert "[Body hidden:" in text


@pytest.mark.anyio
async def test_read_file_range_relative_numbers(server):
    text = (
        (
            await server.call_tool(
                "read_file_range",
                {"file_path": "src/app.py", "start_line": 1, "end_line": 3},
            )
        )
        .content[0]
        .text
    )
    lines = text.splitlines()
    assert lines[0].startswith("1: ")
    assert lines[1].startswith("2: ")
    assert lines[2].startswith("3: ")
    assert "import os" in lines[0]
    assert "from util import helper" in lines[1]


@pytest.mark.anyio
async def test_read_file_range_clamps_end(server):
    text = (
        (
            await server.call_tool(
                "read_file_range",
                {"file_path": "src/app.py", "start_line": 1, "end_line": 999},
            )
        )
        .content[0]
        .text
    )
    last = text.splitlines()[-1]
    assert last.startswith(f"{len(APP_PY.splitlines())}: ")
    assert "return helper()" in last


@pytest.mark.anyio
async def test_read_file_range_invalid(server):
    with pytest.raises(ToolError, match="invalid line range"):
        await server.call_tool(
            "read_file_range",
            {"file_path": "src/app.py", "start_line": 0, "end_line": 3},
        )


@pytest.mark.anyio
async def test_search_codebase_extension_filter(server):
    py = await server.call_tool(
        "search_codebase", {"pattern": "Greeter", "extension": "py"}
    )
    py_matches = _list_blocks(py)
    assert py_matches
    assert all(m["file"] == "src/app.py" for m in py_matches)
    assert all(m["line"] >= 1 for m in py_matches)
    assert all("text" in m for m in py_matches)

    rs = await server.call_tool(
        "search_codebase", {"pattern": "helper", "extension": "rs"}
    )
    rs_matches = _list_blocks(rs)
    assert rs_matches
    assert all(m["file"] == "src/util.rs" for m in rs_matches)


@pytest.mark.anyio
async def test_search_codebase_max_results(server):
    result = await server.call_tool(
        "search_codebase", {"pattern": "helper", "max_results": 1}
    )
    assert len(_list_blocks(result)) == 1


@pytest.mark.anyio
async def test_search_codebase_invalid_regex(server):
    with pytest.raises(ToolError, match="invalid regex"):
        await server.call_tool("search_codebase", {"pattern": "("})


@pytest.mark.anyio
async def test_search_codebase_invalid_extension(server):
    with pytest.raises(ToolError, match="invalid extension filter"):
        await server.call_tool("search_codebase", {"pattern": "x", "extension": "a/b"})


@pytest.mark.anyio
async def test_file_outline_python(server):
    symbols = _list_blocks(
        await server.call_tool("get_file_outline", {"file_path": "src/app.py"})
    )
    by_name = {s["name"]: s for s in symbols}
    assert by_name["Greeter"]["kind"] == "class"
    assert by_name["greet"]["kind"] == "function"
    assert by_name["main"]["kind"] == "function"
    assert by_name["main"]["line_start"] <= by_name["main"]["line_end"]


@pytest.mark.anyio
async def test_file_outline_rust(server):
    symbols = _list_blocks(
        await server.call_tool("get_file_outline", {"file_path": "src/util.rs"})
    )
    by_name = {s["name"]: s for s in symbols}
    assert by_name["Point"]["kind"] == "struct"
    assert by_name["new"]["kind"] == "function"
    assert by_name["helper"]["kind"] == "function"


@pytest.mark.anyio
async def test_symbol_definition_scoped(server):
    text = (
        (
            await server.call_tool(
                "get_symbol_definition",
                {"symbol_name": "helper", "file_path": "src/util.rs"},
            )
        )
        .content[0]
        .text
    )
    assert "pub fn helper() -> i32" in text
    assert "42" in text


@pytest.mark.anyio
async def test_symbol_definition_global(server):
    text = (
        (await server.call_tool("get_symbol_definition", {"symbol_name": "Greeter"}))
        .content[0]
        .text
    )
    assert "class Greeter" in text


@pytest.mark.anyio
async def test_symbol_definition_not_found(server):
    with pytest.raises(ToolError, match="symbol not found"):
        await server.call_tool("get_symbol_definition", {"symbol_name": "nope"})


@pytest.mark.anyio
async def test_symbol_definition_ambiguous(server):
    # `shared` is defined in both app.py and util.rs.
    with pytest.raises(ToolError, match="multiple files"):
        await server.call_tool("get_symbol_definition", {"symbol_name": "shared"})


@pytest.mark.anyio
async def test_upstream_refs(server):
    spots = _list_blocks(
        await server.call_tool("get_upstream_refs", {"symbol_name": "helper"})
    )
    assert spots
    assert all({"file", "line", "context"} <= set(s) for s in spots)
    assert any(s["file"] == "src/app.py" for s in spots)


@pytest.mark.anyio
async def test_upstream_refs_not_found(server):
    with pytest.raises(ToolError, match="symbol not found"):
        await server.call_tool("get_upstream_refs", {"symbol_name": "nope"})


@pytest.mark.anyio
async def test_downstream_refs(server):
    result = await server.call_tool(
        "get_downstream_refs", {"symbol_key": "src/util.rs:helper"}
    )
    payload = json.loads(result.content[0].text)
    assert set(payload) == {"callers", "paths"}
    callers = payload["callers"]
    assert any(
        c["symbol"] == "main" and c["depth"] == 1 and c["file"] == "src/app.py"
        for c in callers
    )
