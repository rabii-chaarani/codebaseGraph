from __future__ import annotations

import json
import subprocess
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
from codebase_graph.version import rust_package_version


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
    assert older["result"]["serverInfo"]["version"] == rust_package_version()
    assert fallback["result"]["protocolVersion"] == LATEST_PROTOCOL_VERSION
    assert "2025-11-25" in SUPPORTED_PROTOCOL_VERSIONS


def test_architecture_query_catalog_is_available_over_mcp_without_opening_graph(tmp_path: Path) -> None:
    db_path = tmp_path / "graph.ldb"
    db_path.write_text("", encoding="utf-8")
    server = McpGraphServer(GraphRuntimeConfig(repo_root=tmp_path, db_path=db_path))

    server.handle_json_rpc({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}})
    listed = server.handle_json_rpc({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}})
    all_queries = server.handle_json_rpc(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": "graph_architecture_queries", "arguments": {"include_structured_content": True}},
        }
    )
    filtered = server.handle_json_rpc(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "graph_architecture_queries",
                "arguments": {"group": "overview", "include_structured_content": True},
            },
        }
    )
    invalid = server.handle_json_rpc(
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "graph_architecture_queries",
                "arguments": {"group": "missing", "include_structured_content": True},
            },
        }
    )

    assert listed is not None
    assert all_queries is not None
    assert filtered is not None
    assert invalid is not None
    assert any(tool["name"] == "graph_architecture_queries" for tool in listed["result"]["tools"])
    assert all_queries["result"]["structuredContent"]["workflow"] == "coding_task_architecture_discovery"
    assert all_queries["result"]["structuredContent"]["execution_tool"] == "graph_query"
    assert [group["name"] for group in filtered["result"]["structuredContent"]["groups"]] == ["overview"]
    assert invalid["result"]["isError"] is True
    assert invalid["result"]["structuredContent"]["error"]["type"] == "ValueError"


def test_mcp_rejects_tools_before_initialize(tmp_path: Path) -> None:
    db_path = tmp_path / "graph.ldb"
    db_path.write_text("", encoding="utf-8")
    server = McpGraphServer(GraphRuntimeConfig(repo_root=tmp_path, db_path=db_path))

    listed = server.handle_json_rpc({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}})
    called = server.handle_json_rpc(
        {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "graph_health", "arguments": {}}}
    )

    assert listed is not None
    assert called is not None
    assert listed["error"]["code"] == -32002
    assert called["error"]["code"] == -32002


def test_descriptor_prefers_explicit_native_cli(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    native_binary = tmp_path / "bin" / "codebase-graph"
    native_binary.parent.mkdir(parents=True)
    native_binary.write_text("", encoding="utf-8")
    native_binary.chmod(0o755)
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_CLI", native_binary.as_posix())
    monkeypatch.setenv("PATH", "")

    descriptor = build_server_descriptor(tmp_path / ".codebaseGraph" / "config.json")

    assert descriptor.command == native_binary.as_posix()
    assert descriptor.stdio_entry()["command"] == native_binary.as_posix()
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
    assert rendered["github-copilot"]["payload"]["servers"]["codebase_graph"]["type"] == "stdio"
    assert rendered["github-copilot"]["payload"]["servers"]["codebase_graph"]["args"][0:2] == ["mcp", "serve"]
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
        structured_health = _rpc(
            proc.stdin,
            proc.stdout,
            "tools/call",
            {"name": "graph_health", "arguments": {"include_structured_content": True}},
        )
        search = _rpc(
            proc.stdin,
            proc.stdout,
            "tools/call",
            {"name": "graph_search", "arguments": {"query": "SampleService", "limit": 2}},
        )
        json_search = _rpc(
            proc.stdin,
            proc.stdout,
            "tools/call",
            {"name": "graph_search", "arguments": {"query": "SampleService", "limit": 2, "output_format": "json"}},
        )
        structured_search = _rpc(
            proc.stdin,
            proc.stdout,
            "tools/call",
            {
                "name": "graph_search",
                "arguments": {
                    "query": "SampleService",
                    "limit": 2,
                    "include_structured_content": True,
                },
            },
        )
        failure = _rpc(
            proc.stdin,
            proc.stdout,
            "tools/call",
            {"name": "graph_query", "arguments": {"statement": "MATCH (n) DELETE n"}},
        )
        structured_failure = _rpc(
            proc.stdin,
            proc.stdout,
            "tools/call",
            {
                "name": "graph_query",
                "arguments": {
                    "statement": "MATCH (n) DELETE n",
                    "include_structured_content": True,
                },
            },
        )
    finally:
        proc.stdin.close()
        proc.wait(timeout=10)

    assert initialized["result"]["protocolVersion"] == "2025-11-25"
    assert {tool["name"] for tool in listed["result"]["tools"]} >= {"graph_health", "graph_search", "graph_query"}
    for tool in listed["result"]["tools"]:
        properties = tool["inputSchema"]["properties"]
        assert properties["output_format"]["enum"] == ["json", "block"]
        assert properties["output_format"]["default"] == "block"
        assert properties["include_structured_content"]["default"] is False
    graph_search_tool = next(tool for tool in listed["result"]["tools"] if tool["name"] == "graph_search")
    assert "context_limit" in graph_search_tool["inputSchema"]["properties"]
    assert graph_search_tool["inputSchema"]["properties"]["detail"]["enum"] == ["slim", "standard"]
    assert "structuredContent" not in health["result"]
    assert health["result"]["content"][0]["text"].startswith("health ok=true ")
    assert structured_health["result"]["structuredContent"]["ok"] is True
    assert "structuredContent" not in search["result"]
    assert search["result"]["content"][0]["text"].startswith("q SampleService\n")
    assert "id=Class:" in search["result"]["content"][0]["text"]
    assert "structuredContent" not in json_search["result"]
    json_payload = json.loads(json_search["result"]["content"][0]["text"])
    assert json_payload["results"]
    assert structured_search["result"]["structuredContent"] == json_payload
    assert structured_search["result"]["content"][0]["text"].startswith("q SampleService\n")
    assert "error" not in failure
    assert failure["result"]["isError"] is True
    assert "structuredContent" not in failure["result"]
    assert failure["result"]["content"][0]["text"].startswith("error tool=graph_query type=ValueError")
    assert structured_failure["result"]["structuredContent"]["error"]["type"] == "ValueError"


def test_stdio_mcp_malformed_frame_returns_parse_error(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)
    result = run_setup(SetupOptions(repo_root=repo_root, mcp_client="none", instructions_target="skip"))
    setup_payload = json.loads(result.paths.config_path.read_text(encoding="utf-8"))

    completed = subprocess.run(
        setup_payload["mcp"]["command"],
        input=b"{\n",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )

    responses = _stdio_messages(completed.stdout)
    assert completed.returncode == 0
    stderr_events = [json.loads(line) for line in completed.stderr.decode("utf-8").splitlines()]
    assert stderr_events[0]["event"] == "mcp.stdio_parse_error"
    assert responses[0]["error"]["code"] == -32700


def test_http_mcp_rejects_remote_bind_without_explicit_opt_in(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="localhost"):
        build_http_server(repo_root=tmp_path, db_path=tmp_path / "missing.ldb", host="0.0.0.0", port=0)


def test_http_mcp_rejects_remote_bind_without_auth_token(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="auth token"):
        build_http_server(
            repo_root=tmp_path,
            db_path=tmp_path / "missing.ldb",
            host="0.0.0.0",
            port=0,
            allow_remote=True,
        )


def test_http_mcp_accepts_remote_bind_with_auth_token(tmp_path: Path) -> None:
    db_path = tmp_path / "graph.ldb"
    db_path.write_text("", encoding="utf-8")
    try:
        httpd = build_http_server(
            repo_root=tmp_path,
            db_path=db_path,
            host="0.0.0.0",
            port=0,
            allow_remote=True,
            auth_token="secret-token",
        )
    except PermissionError as exc:
        pytest.skip(f"remote socket bind is unavailable in this environment: {exc}")

    httpd.server_close()


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
        initialize, session_id = _http_rpc_with_session(host, port, "initialize", {"protocolVersion": "2025-11-25"})
        with pytest.raises(urllib.error.HTTPError) as missing_session:
            _http_rpc(host, port, "tools/list", {})
        listed = _http_rpc(host, port, "tools/list", {}, session_id=session_id)
        health = _http_rpc(
            host,
            port,
            "tools/call",
            {"name": "graph_health", "arguments": {}},
            session_id=session_id,
        )
        with pytest.raises(urllib.error.HTTPError) as exc_info:
            _http_rpc(host, port, "ping", {}, protocol_version="1900-01-01")
    finally:
        httpd.shutdown()
        httpd.server_close()
        thread.join(timeout=10)

    assert initialize["result"]["protocolVersion"] == "2025-11-25"
    assert missing_session.value.code == 400
    assert any(tool["name"] == "graph_context" for tool in listed["result"]["tools"])
    assert "structuredContent" not in health["result"]
    assert health["result"]["content"][0]["text"].startswith("health ok=true ")
    assert exc_info.value.code == 400


def test_http_mcp_transport_enforces_bearer_token_when_configured(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)
    result = run_setup(SetupOptions(repo_root=repo_root, mcp_client="none", instructions_target="skip"))
    try:
        httpd = build_http_server(config_path=result.paths.config_path, host="127.0.0.1", port=0, auth_token="secret")
    except PermissionError as exc:
        pytest.skip(f"local socket bind is unavailable in this environment: {exc}")
    thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    thread.start()
    host, port = httpd.server_address
    try:
        with pytest.raises(urllib.error.HTTPError) as missing_exc:
            _http_rpc(host, port, "initialize", {"protocolVersion": "2025-11-25"}, origin=f"http://{host}:{port}")
        with pytest.raises(urllib.error.HTTPError) as wrong_exc:
            _http_rpc(
                host,
                port,
                "initialize",
                {"protocolVersion": "2025-11-25"},
                auth_token="wrong",
                origin=f"http://{host}:{port}",
            )
        initialized = _http_rpc(
            host,
            port,
            "initialize",
            {"protocolVersion": "2025-11-25"},
            auth_token="secret",
            origin=f"http://{host}:{port}",
        )
    finally:
        httpd.shutdown()
        httpd.server_close()
        thread.join(timeout=10)

    assert missing_exc.value.code == 401
    assert wrong_exc.value.code == 401
    assert initialized["result"]["protocolVersion"] == "2025-11-25"


def _rpc(stdin: BinaryIO, stdout: BinaryIO, method: str, params: dict[str, Any]) -> dict[str, Any]:
    request_id = _rpc.counter
    _rpc.counter += 1
    payload = json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}).encode("utf-8")
    stdin.write(payload + b"\n")
    stdin.flush()
    return _read_stdio_response(stdout)


_rpc.counter = 1  # type: ignore[attr-defined]


def _read_stdio_response(stdout: BinaryIO) -> dict[str, Any]:
    line = stdout.readline()
    assert line
    assert not line.lower().startswith(b"content-length:")
    return json.loads(line.decode("utf-8"))


def _stdio_messages(data: bytes) -> list[dict[str, Any]]:
    assert b"content-length:" not in data.lower()
    return [json.loads(line.decode("utf-8")) for line in data.splitlines() if line]


def _http_rpc(
    host: str,
    port: int,
    method: str,
    params: dict[str, Any],
    *,
    protocol_version: str = "2025-11-25",
    auth_token: str | None = None,
    origin: str | None = None,
    session_id: str | None = None,
) -> dict[str, Any]:
    return _http_rpc_with_headers(
        host,
        port,
        method,
        params,
        protocol_version=protocol_version,
        auth_token=auth_token,
        origin=origin,
        session_id=session_id,
    )[0]


def _http_rpc_with_session(
    host: str,
    port: int,
    method: str,
    params: dict[str, Any],
    *,
    protocol_version: str = "2025-11-25",
    auth_token: str | None = None,
    origin: str | None = None,
    session_id: str | None = None,
) -> tuple[dict[str, Any], str]:
    payload, headers = _http_rpc_with_headers(
        host,
        port,
        method,
        params,
        protocol_version=protocol_version,
        auth_token=auth_token,
        origin=origin,
        session_id=session_id,
    )
    resolved_session_id = headers.get("Mcp-Session-Id")
    assert resolved_session_id
    return payload, resolved_session_id


def _http_rpc_with_headers(
    host: str,
    port: int,
    method: str,
    params: dict[str, Any],
    *,
    protocol_version: str = "2025-11-25",
    auth_token: str | None = None,
    origin: str | None = None,
    session_id: str | None = None,
) -> tuple[dict[str, Any], Any]:
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode("utf-8")
    headers = {
        "Accept": "application/json, text/event-stream",
        "Content-Type": "application/json",
        "MCP-Protocol-Version": protocol_version,
        "Origin": origin or f"http://{host}:{port}",
    }
    if auth_token is not None:
        headers["Authorization"] = f"Bearer {auth_token}"
    if session_id is not None:
        headers["Mcp-Session-Id"] = session_id
    request = urllib.request.Request(
        f"http://{host}:{port}/mcp",
        data=payload,
        headers=headers,
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.loads(response.read().decode("utf-8")), response.headers


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
