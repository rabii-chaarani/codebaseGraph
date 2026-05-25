from __future__ import annotations

import json
import subprocess
import sys
import threading
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, BinaryIO

try:
    import tomllib
except ImportError:  # pragma: no cover - Python 3.10 compatibility
    import tomli as tomllib

import pytest

from codebase_graph.mcp.protocol import LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS, McpGraphServer
from codebase_graph.mcp.runtime import GraphRuntimeConfig
from codebase_graph.mcp.transports.http import build_http_server
from codebase_graph.setup import SetupOptions, run_setup
from codebase_graph.setup.clients import supported_client_ids
from codebase_graph.setup.descriptor import build_server_descriptor
from codebase_graph.setup.mcp_config import configure_mcp_client


def test_initialize_negotiates_supported_and_fallback_protocol_versions(tmp_path: Path) -> None:
    db_path = tmp_path / "graph.ldb"
    db_path.write_text("", encoding="utf-8")
    server = McpGraphServer(GraphRuntimeConfig(repo_root=tmp_path, db_path=db_path))

    older = server.handle_json_rpc(
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}}
    )
    fallback = server.handle_json_rpc(
        {"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "1.0.0"}}
    )

    assert older is not None
    assert fallback is not None
    assert older["result"]["protocolVersion"] == "2024-11-05"
    assert fallback["result"]["protocolVersion"] == LATEST_PROTOCOL_VERSION
    assert "2025-11-25" in SUPPORTED_PROTOCOL_VERSIONS


def test_descriptor_prefers_current_environment_script(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    bin_dir = tmp_path / "venv" / "bin"
    bin_dir.mkdir(parents=True)
    python_path = bin_dir / "python"
    script_path = bin_dir / "codebase-graph"
    python_path.write_text("", encoding="utf-8")
    script_path.write_text("", encoding="utf-8")
    script_path.chmod(0o755)
    monkeypatch.setattr(sys, "executable", python_path.as_posix())
    monkeypatch.setenv("PATH", "")

    descriptor = build_server_descriptor(tmp_path / ".codebaseGraph" / "config.json")

    assert descriptor.command == script_path.as_posix()
    assert descriptor.stdio_entry()["command"] == script_path.as_posix()
    assert descriptor.as_dict()["transport"] == "stdio"


def test_client_adapters_emit_native_config_shapes(tmp_path: Path) -> None:
    setup_config_path = tmp_path / ".codebaseGraph" / "config.json"
    setup_config_path.parent.mkdir()
    clients = set(supported_client_ids()) - {"none"}

    rendered = {
        client: configure_mcp_client(
            client=client,
            config_path=tmp_path / f"{client}.config",
            setup_config_path=setup_config_path,
            dry_run=True,
        ).as_dict()
        for client in clients
    }

    codex_patch = rendered["codex"]["patch"]
    codex_payload = tomllib.loads(codex_patch)
    assert codex_payload["mcp_servers"]["codebase_graph"]["command"]
    assert codex_payload["mcp_servers"]["codebase_graph"]["startup_timeout_sec"] == 60
    assert "type" not in rendered["claude"]["payload"]["mcpServers"]["codebase_graph"]
    assert rendered["claude"]["payload"]["mcpServers"]["codebase_graph"]["command"]
    assert rendered["claude-project"]["payload"]["mcpServers"]["codebase_graph"]["type"] == "stdio"
    assert rendered["lmstudio"]["payload"]["mcpServers"]["codebase_graph"]["type"] == "stdio"
    assert rendered["generic"]["payload"]["mcpServers"]["codebase_graph"]["args"][0:2] == ["mcp", "serve"]
    assert rendered["openclaw"]["payload"]["mcp"]["servers"]["codebase_graph"]["type"] == "stdio"
    assert "mcp_servers:" in rendered["hermes"]["patch"]


def test_unsupported_mcp_client_lists_supported_clients(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="Supported clients:"):
        configure_mcp_client(
            client="missing",
            config_path=tmp_path / "missing.json",
            setup_config_path=tmp_path / ".codebaseGraph" / "config.json",
        )


def test_stdio_mcp_wire_initialize_list_call_and_tool_error(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)
    result = run_setup(SetupOptions(repo_root=repo_root, mcp_client="none", instructions_target="skip"))
    setup_payload = json.loads(result.paths.config_path.read_text(encoding="utf-8"))
    command = setup_payload["mcp"]["command"]

    proc = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert proc.stdin is not None
    assert proc.stdout is not None
    try:
        initialized = _rpc(proc.stdin, proc.stdout, "initialize", {"protocolVersion": "2025-11-25"})
        listed = _rpc(proc.stdin, proc.stdout, "tools/list", {})
        health = _rpc(proc.stdin, proc.stdout, "tools/call", {"name": "graph_health", "arguments": {}})
        search = _rpc(
            proc.stdin,
            proc.stdout,
            "tools/call",
            {"name": "graph_search", "arguments": {"query": "SampleService", "limit": 2}},
        )
        failure = _rpc(
            proc.stdin,
            proc.stdout,
            "tools/call",
            {"name": "graph_query", "arguments": {"statement": "MATCH (n) DELETE n"}},
        )
    finally:
        proc.stdin.close()
        proc.wait(timeout=10)

    assert initialized["result"]["protocolVersion"] == "2025-11-25"
    assert {tool["name"] for tool in listed["result"]["tools"]} >= {"graph_health", "graph_search", "graph_query"}
    assert health["result"]["structuredContent"]["ok"] is True
    assert search["result"]["structuredContent"]["results"]
    assert "error" not in failure
    assert failure["result"]["isError"] is True
    assert failure["result"]["structuredContent"]["error"]["type"] == "ValueError"


def test_http_mcp_transport_handles_initialize_list_and_call(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)
    result = run_setup(SetupOptions(repo_root=repo_root, mcp_client="none", instructions_target="skip"))
    try:
        httpd = build_http_server(config_path=result.paths.config_path, host="127.0.0.1", port=0)
    except PermissionError as exc:
        pytest.skip(f"local socket bind is unavailable in this environment: {exc}")
    thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    thread.start()
    host, port = httpd.server_address
    try:
        initialize = _http_rpc(host, port, "initialize", {"protocolVersion": "2025-11-25"})
        listed = _http_rpc(host, port, "tools/list", {})
        health = _http_rpc(host, port, "tools/call", {"name": "graph_health", "arguments": {}})
        with pytest.raises(urllib.error.HTTPError) as exc_info:
            _http_rpc(host, port, "ping", {}, protocol_version="1900-01-01")
    finally:
        httpd.shutdown()
        httpd.server_close()
        thread.join(timeout=10)

    assert initialize["result"]["protocolVersion"] == "2025-11-25"
    assert any(tool["name"] == "graph_context" for tool in listed["result"]["tools"])
    assert health["result"]["structuredContent"]["ok"] is True
    assert exc_info.value.code == 400


def _rpc(stdin: BinaryIO, stdout: BinaryIO, method: str, params: dict[str, Any]) -> dict[str, Any]:
    request_id = _rpc.counter
    _rpc.counter += 1
    payload = json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}).encode("utf-8")
    stdin.write(f"Content-Length: {len(payload)}\r\n\r\n".encode("ascii") + payload)
    stdin.flush()
    return _read_stdio_response(stdout)


_rpc.counter = 1  # type: ignore[attr-defined]


def _read_stdio_response(stdout: BinaryIO) -> dict[str, Any]:
    header = stdout.readline()
    assert header.lower().startswith(b"content-length:")
    length = int(header.split(b":", 1)[1].strip())
    assert stdout.readline() in {b"\r\n", b"\n"}
    return json.loads(stdout.read(length).decode("utf-8"))


def _http_rpc(
    host: str,
    port: int,
    method: str,
    params: dict[str, Any],
    *,
    protocol_version: str = "2025-11-25",
) -> dict[str, Any]:
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode("utf-8")
    request = urllib.request.Request(
        f"http://{host}:{port}/mcp",
        data=payload,
        headers={
            "Accept": "application/json, text/event-stream",
            "Content-Type": "application/json",
            "MCP-Protocol-Version": protocol_version,
            "Origin": f"http://{host}:{port}",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.loads(response.read().decode("utf-8"))


def _fresh_repo(tmp_path: Path) -> Path:
    repo_root = tmp_path / "fresh_repo"
    package = repo_root / "sample_project"
    package.mkdir(parents=True)
    (package / "__init__.py").write_text("", encoding="utf-8")
    (package / "service.py").write_text(
        "class SampleService:\n"
        "    def run(self) -> str:\n"
        "        return helper()\n\n"
        "def helper() -> str:\n"
        "    return 'ok'\n",
        encoding="utf-8",
    )
    (repo_root / "README.md").write_text(
        "# Fresh Repo\n\nThis repository documents the SampleService workflow.\n",
        encoding="utf-8",
    )
    return repo_root
