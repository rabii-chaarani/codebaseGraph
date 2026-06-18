from __future__ import annotations

import subprocess
from pathlib import Path

import pytest

import codebase_graph.cli as cli_module
import codebase_graph.mcp.server as mcp_server
import codebase_graph.mcp.tools as mcp_tools
from codebase_graph.mcp.runtime import GraphRuntimeConfig


def test_codebase_graph_cli_is_thin_rust_launcher() -> None:
    text = Path("src/codebase_graph/cli/__init__.py").read_text(encoding="utf-8")

    assert "GraphMaterializer" not in text
    assert "argparse" not in text
    assert "run_setup" not in text
    assert "SearchService" not in text
    assert "codebase_graph._native.product_cli" not in text
    assert "run_product_cli" not in text


def test_codebase_graph_mcp_script_is_thin_rust_launcher() -> None:
    text = Path("src/codebase_graph/mcp/server.py").read_text(encoding="utf-8")

    assert "argparse" not in text
    assert "serve_stdio" not in text
    assert "serve_http" not in text
    assert "build_http_server" not in text
    assert "codebase_graph._native.product_cli" not in text
    assert "run_product_cli" not in text


def test_codebase_graph_cli_delegates_rust_owned_command_to_binary(monkeypatch, capsys) -> None:
    calls: list[list[str]] = []

    def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        calls.append(command)
        return subprocess.CompletedProcess(command, 0, stdout='{"ok":true}\n', stderr="")

    monkeypatch.setattr(cli_module, "_native_product_binary", lambda: "/tmp/codebase-graph")
    monkeypatch.setattr(cli_module.subprocess, "run", fake_run)

    status = cli_module.main(["graph-schema", "--json"])

    captured = capsys.readouterr()
    assert status == 0
    assert calls == [["/tmp/codebase-graph", "graph-schema", "--json"]]
    assert captured.out == '{"ok":true}\n'
    assert captured.err == ""


def test_codebase_graph_cli_delegates_setup_to_binary(monkeypatch, capsys) -> None:
    calls: list[list[str]] = []

    def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        calls.append(command)
        return subprocess.CompletedProcess(command, 0, stdout='{"config_action":"dry_run"}\n', stderr="")

    monkeypatch.setattr(cli_module, "_native_product_binary", lambda: "/tmp/codebase-graph")
    monkeypatch.setattr(cli_module.subprocess, "run", fake_run)

    status = cli_module.main(["setup", "--repo-root", ".", "--mcp-client", "none", "--dry-run", "--json"])

    captured = capsys.readouterr()
    assert status == 0
    assert calls == [["/tmp/codebase-graph", "setup", "--repo-root", ".", "--mcp-client", "none", "--dry-run", "--json"]]
    assert captured.out == '{"config_action":"dry_run"}\n'
    assert captured.err == ""


def test_codebase_graph_cli_delegates_process_stdio_to_binary(monkeypatch) -> None:
    calls: list[list[str]] = []

    def fake_call(command: list[str]) -> int:
        calls.append(command)
        return 0

    monkeypatch.setattr(cli_module, "_native_product_binary", lambda: "/tmp/codebase-graph")
    monkeypatch.setattr(cli_module.subprocess, "call", fake_call)

    status = cli_module.main(["mcp", "serve", "--repo-root", "."])

    assert status == 0
    assert calls == [["/tmp/codebase-graph", "mcp", "serve", "--repo-root", "."]]


def test_codebase_graph_cli_requires_rust_binary_even_if_force_python_is_set(monkeypatch) -> None:
    monkeypatch.setattr(cli_module, "_native_product_binary", lambda: None)
    monkeypatch.setenv("CODEBASE_GRAPH_FORCE_PYTHON_CLI", "1")

    try:
        cli_module.main(["graph-schema", "--json"])
    except SystemExit as exc:
        assert "Rust native CLI binary is required" in str(exc)
    else:  # pragma: no cover - defensive assertion for the no-fallback contract.
        raise AssertionError("Rust-owned command unexpectedly ran without a Rust binary")


def test_codebase_graph_mcp_script_delegates_to_binary(monkeypatch) -> None:
    calls: list[list[str]] = []

    def fake_call(command: list[str]) -> int:
        calls.append(command)
        return 0

    monkeypatch.setattr(mcp_server, "_native_product_binary", lambda: "/tmp/codebase-graph")
    monkeypatch.setattr(mcp_server.subprocess, "call", fake_call)

    status = mcp_server.main(["--repo-root", "."])

    assert status == 0
    assert calls == [["/tmp/codebase-graph", "mcp", "serve", "--repo-root", "."]]


def test_codebase_graph_mcp_help_delegates_to_binary_help(monkeypatch) -> None:
    calls: list[list[str]] = []

    def fake_call(command: list[str]) -> int:
        calls.append(command)
        return 0

    monkeypatch.setattr(mcp_server, "_native_product_binary", lambda: "/tmp/codebase-graph")
    monkeypatch.setattr(mcp_server.subprocess, "call", fake_call)

    status = mcp_server.main(["--help"])

    assert status == 0
    assert calls == [["/tmp/codebase-graph", "mcp", "--help"]]


def test_codebase_graph_mcp_script_requires_rust_binary_even_if_force_python_is_set(monkeypatch) -> None:
    monkeypatch.setattr(mcp_server, "_native_product_binary", lambda: None)
    monkeypatch.setenv("CODEBASE_GRAPH_FORCE_PYTHON_CLI", "1")

    try:
        mcp_server.main(["--repo-root", "."])
    except SystemExit as exc:
        assert "Rust native MCP binary is required" in str(exc)
    else:  # pragma: no cover - defensive assertion for the no-fallback contract.
        raise AssertionError("MCP script unexpectedly ran without a Rust binary")


def test_mcp_db_tool_requires_rust_binary_without_python_fallback(monkeypatch, tmp_path: Path) -> None:
    runtime = GraphRuntimeConfig(repo_root=tmp_path, db_path=tmp_path / "graph.ldb", manifest_path=None)

    monkeypatch.setattr(mcp_tools, "_native_product_binary", lambda: None)
    assert not hasattr(mcp_tools, "open_graph_store")

    with pytest.raises(RuntimeError, match="Rust native CLI binary is required"):
        mcp_tools.handle_tool_call("graph_health", {}, runtime=runtime)


def test_mcp_db_tool_propagates_rust_failure_without_python_fallback(monkeypatch, tmp_path: Path) -> None:
    runtime = GraphRuntimeConfig(repo_root=tmp_path, db_path=tmp_path / "graph.ldb", manifest_path=None)

    def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        return subprocess.CompletedProcess(command, 2, stdout="", stderr="native failed")

    monkeypatch.setattr(mcp_tools, "_native_product_binary", lambda: "/tmp/codebase-graph")
    monkeypatch.setattr(mcp_tools.subprocess, "run", fake_run)
    assert not hasattr(mcp_tools, "open_graph_store")

    with pytest.raises(RuntimeError, match="native failed"):
        mcp_tools.handle_tool_call("graph_search", {"query": "SampleService"}, runtime=runtime)
