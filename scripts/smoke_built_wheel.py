from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, BinaryIO


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        raise SystemExit("usage: smoke_built_wheel.py /path/to/codebase-graph")
    executable = Path(argv[1])
    with tempfile.TemporaryDirectory(prefix="codebase-graph-wheel-smoke-") as tmp_dir:
        repo_root = _sample_repo(Path(tmp_dir) / "sample_repo")
        setup = _run(
            [
                executable.as_posix(),
                "setup",
                "--repo-root",
                repo_root.as_posix(),
                "--mcp-client",
                "none",
                "--instructions-target",
                "skip",
            ]
        )
        setup_payload = json.loads(setup.stdout)
        config_path = Path(setup_payload["config_path"])

        health = json.loads(_run([executable.as_posix(), "graph-health", "--repo-root", repo_root.as_posix()]).stdout)
        if not health.get("ok") or not health.get("graph_readable"):
            raise AssertionError(f"graph-health failed readiness smoke: {health}")

        search = json.loads(
            _run(
                [
                    executable.as_posix(),
                    "graph-search",
                    "SampleService",
                    "--repo-root",
                    repo_root.as_posix(),
                    "--no-refresh",
                    "--detail",
                    "slim",
                    "--json",
                ]
            ).stdout
        )
        if not search.get("results"):
            raise AssertionError(f"graph-search returned no results: {search}")

        _install_verify_smoke(executable, config_path, Path(tmp_dir) / "mcp.json")
        _mcp_smoke([executable.as_posix(), "mcp", "serve", "--config", config_path.as_posix()])
    return 0


def _run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, capture_output=True, text=True, check=True)


def _install_verify_smoke(executable: Path, config_path: Path, client_config_path: Path) -> None:
    verify = json.loads(
        _run(
            [
                executable.as_posix(),
                "mcp",
                "install",
                "--client",
                "generic",
                "--config-path",
                config_path.as_posix(),
                "--client-config-path",
                client_config_path.as_posix(),
                "--verify",
                "--json",
            ]
        ).stdout
    )
    verification = verify.get("verification") or {}
    stdio = verification.get("stdio") or {}
    checks = stdio.get("checks") or {}
    required_checks = ("initialize", "tools_list", "graph_health", "graph_search", "tool_error_result")
    if verification.get("ok") is not True or not all(checks.get(check) is True for check in required_checks):
        raise AssertionError(f"mcp install --verify failed readiness smoke: {verify}")


def _sample_repo(repo_root: Path) -> Path:
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
    (repo_root / "README.md").write_text("# Sample Repo\n\nSampleService smoke fixture.\n", encoding="utf-8")
    return repo_root


def _mcp_smoke(command: list[str]) -> None:
    proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert proc.stdin is not None
    assert proc.stdout is not None
    try:
        initialized = _rpc(proc.stdin, proc.stdout, "initialize", {"protocolVersion": "2025-11-25"})
        listed = _rpc(proc.stdin, proc.stdout, "tools/list", {})
        health = _rpc(proc.stdin, proc.stdout, "tools/call", {"name": "graph_health", "arguments": {}})
    finally:
        proc.stdin.close()
        proc.wait(timeout=10)
    assert proc.stderr is not None
    stderr = proc.stderr.read()
    if proc.returncode != 0:
        raise AssertionError(stderr.decode("utf-8", errors="replace"))
    if initialized["result"]["protocolVersion"] != "2025-11-25":
        raise AssertionError(initialized)
    tool_names = {tool["name"] for tool in listed["result"]["tools"]}
    if not {"graph_health", "graph_search", "graph_query"}.issubset(tool_names):
        raise AssertionError(listed)
    if health["result"]["structuredContent"].get("ok") is not True:
        raise AssertionError(health)


def _rpc(stdin: BinaryIO, stdout: BinaryIO, method: str, params: dict[str, Any]) -> dict[str, Any]:
    request_id = _rpc.counter
    _rpc.counter += 1
    body = json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}).encode("utf-8")
    stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body)
    stdin.flush()
    return _read_response(stdout)


_rpc.counter = 1  # type: ignore[attr-defined]


def _read_response(stdout: BinaryIO) -> dict[str, Any]:
    header = stdout.readline()
    if not header.lower().startswith(b"content-length:"):
        raise AssertionError(f"unexpected MCP header: {header!r}")
    length = int(header.split(b":", 1)[1].strip())
    separator = stdout.readline()
    if separator not in {b"\r\n", b"\n"}:
        raise AssertionError(f"unexpected MCP header separator: {separator!r}")
    return json.loads(stdout.read(length).decode("utf-8"))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
