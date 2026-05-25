from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from codebase_graph.mcp.protocol import LATEST_PROTOCOL_VERSION

from .clients import get_client_adapter
from .descriptor import McpServerDescriptor, build_server_descriptor
from .state import MCP_SERVER_NAME, load_setup_config

INSTALL_CLIENTS = ("codex", "claude", "claude-project", "lmstudio", "hermes", "openclaw", "generic")
SCOPES = ("local", "user", "project")
NATIVE_EXECUTABLES = {
    "codex": "codex",
    "claude": "claude",
    "claude-project": "claude",
    "openclaw": "openclaw",
}


@dataclass(frozen=True, slots=True)
class McpInstallOptions:
    client: str = "codex"
    scope: str = "local"
    setup_config_path: str | Path = ".codebaseGraph/config.json"
    server_name: str | None = None
    client_config_path: str | Path | None = None
    dry_run: bool = False
    verify: bool = False
    skip: bool = False
    prefer_native: bool = True
    require_setup_config: bool = True


@dataclass(frozen=True, slots=True)
class McpInstallResult:
    action: str
    client: str
    scope: str
    server_name: str
    method: str | None
    path: str | None
    command: list[str] | None
    descriptor: dict[str, Any]
    entry: dict[str, Any]
    patch: Any = None
    payload: Any = None
    verification: dict[str, Any] | None = None
    error: str | None = None
    native_command: list[str] | None = None
    native_error: str | None = None

    def as_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "action": self.action,
            "client": self.client,
            "scope": self.scope,
            "server_name": self.server_name,
            "method": self.method,
            "path": self.path,
            "command": self.command,
            "descriptor": self.descriptor,
            "entry": self.entry,
        }
        if self.patch is not None:
            payload["patch"] = self.patch
        if self.payload is not None:
            payload["payload"] = self.payload
        if self.verification is not None:
            payload["verification"] = self.verification
        if self.error is not None:
            payload["error"] = self.error
        if self.native_command is not None:
            payload["native_command"] = self.native_command
        if self.native_error is not None:
            payload["native_error"] = self.native_error
        return payload


def supported_install_client_ids(*, include_all: bool = False) -> tuple[str, ...]:
    values = [*INSTALL_CLIENTS]
    if include_all:
        values.append("all")
    return tuple(sorted(values))


def default_server_name(repo_name: str | None) -> str:
    safe_repo_name = _safe_name(repo_name or "repository")
    return f"{MCP_SERVER_NAME}_{safe_repo_name}"


def install_mcp_clients(options: McpInstallOptions) -> list[McpInstallResult]:
    if options.client == "all":
        return [_install_with_failure_result(options, client) for client in INSTALL_CLIENTS]
    return [install_mcp_server(options)]


def install_mcp_server(options: McpInstallOptions) -> McpInstallResult:
    _validate_options(options)
    descriptor = _build_descriptor(options)
    entry = descriptor.stdio_entry()
    if options.skip or options.client == "none":
        return McpInstallResult(
            action="skipped",
            client=options.client,
            scope=options.scope,
            server_name=descriptor.name,
            method=None,
            path=None,
            command=None,
            descriptor=descriptor.as_dict(),
            entry=entry,
        )

    native_command = _native_command(options.client, descriptor, scope=options.scope)
    use_native = (
        options.prefer_native
        and options.client_config_path is None
        and native_command is not None
        and shutil.which(_native_executable(options.client))
    )
    if options.dry_run:
        if use_native:
            return _native_result("dry_run", options, descriptor, native_command, verification=None)
        return _file_adapter_result(options, descriptor, dry_run=True, native_command=native_command)

    if use_native and native_command is not None:
        try:
            completed = subprocess.run(native_command, capture_output=True, text=True, check=False, timeout=30)
        except subprocess.TimeoutExpired as exc:
            native_error = f"timed out after {exc.timeout}s"
        except OSError as exc:
            native_error = str(exc)
        else:
            if completed.returncode == 0:
                result = _native_result("updated", options, descriptor, native_command, verification=None)
                return _with_verification(result, descriptor, options.verify)
            native_error = _subprocess_error(completed)
        return _file_adapter_result(
            options,
            descriptor,
            dry_run=False,
            native_command=native_command,
            native_error=native_error,
        )

    return _file_adapter_result(
        options,
        descriptor,
        dry_run=False,
        native_command=native_command,
        native_error=_missing_native_error(options.client) if native_command is not None else None,
    )


def _install_with_failure_result(options: McpInstallOptions, client: str) -> McpInstallResult:
    client_options = McpInstallOptions(
        client=client,
        scope=_scope_for_client(client, options.scope),
        setup_config_path=options.setup_config_path,
        server_name=options.server_name,
        client_config_path=options.client_config_path,
        dry_run=options.dry_run,
        verify=options.verify,
        skip=options.skip,
        prefer_native=options.prefer_native,
        require_setup_config=options.require_setup_config,
    )
    try:
        return install_mcp_server(client_options)
    except Exception as exc:
        try:
            descriptor = _build_descriptor(client_options)
            entry = descriptor.stdio_entry()
            descriptor_payload = descriptor.as_dict()
            server_name = descriptor.name
        except Exception:
            entry = {}
            descriptor_payload = {}
            server_name = client_options.server_name or MCP_SERVER_NAME
        return McpInstallResult(
            action="failed",
            client=client,
            scope=client_options.scope,
            server_name=server_name,
            method=None,
            path=None,
            command=None,
            descriptor=descriptor_payload,
            entry=entry,
            error=str(exc),
        )


def _file_adapter_result(
    options: McpInstallOptions,
    descriptor: McpServerDescriptor,
    *,
    dry_run: bool,
    native_command: list[str] | None = None,
    native_error: str | None = None,
) -> McpInstallResult:
    adapter = get_client_adapter(_adapter_client_id(options.client, options.scope))
    path = (
        Path(options.client_config_path).expanduser().resolve()
        if options.client_config_path is not None
        else adapter.default_config_path(descriptor)
    )
    existing_text = path.read_text(encoding="utf-8") if path.exists() else None
    rendered = adapter.render(existing_text, descriptor)
    action = "dry_run" if dry_run else rendered.action
    if not dry_run:
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = path.with_suffix(path.suffix + ".tmp")
        with tmp_path.open("w", encoding="utf-8") as handle:
            handle.write(rendered.text)
        os.replace(tmp_path, path)
    result = McpInstallResult(
        action=action,
        client=options.client,
        scope=options.scope,
        server_name=descriptor.name,
        method="file_adapter",
        path=path.as_posix(),
        command=None,
        descriptor=descriptor.as_dict(),
        entry=rendered.entry,
        patch=rendered.patch,
        payload=rendered.payload,
        native_command=native_command,
        native_error=native_error,
    )
    return _with_verification(result, descriptor, options.verify and not dry_run)


def _native_result(
    action: str,
    options: McpInstallOptions,
    descriptor: McpServerDescriptor,
    command: list[str],
    *,
    verification: dict[str, Any] | None,
) -> McpInstallResult:
    return McpInstallResult(
        action=action,
        client=options.client,
        scope=options.scope,
        server_name=descriptor.name,
        method="native_cli",
        path=None,
        command=command,
        descriptor=descriptor.as_dict(),
        entry=descriptor.stdio_entry(),
        verification=verification,
    )


def _with_verification(
    result: McpInstallResult,
    descriptor: McpServerDescriptor,
    enabled: bool,
) -> McpInstallResult:
    if not enabled:
        return result
    verification = verify_mcp_install(descriptor, client=result.client, server_name=result.server_name)
    return McpInstallResult(
        action=result.action,
        client=result.client,
        scope=result.scope,
        server_name=result.server_name,
        method=result.method,
        path=result.path,
        command=result.command,
        descriptor=result.descriptor,
        entry=result.entry,
        patch=result.patch,
        payload=result.payload,
        verification=verification,
        error=result.error,
        native_command=result.native_command,
        native_error=result.native_error,
    )


def verify_mcp_install(
    descriptor: McpServerDescriptor,
    *,
    client: str,
    server_name: str,
    timeout: int = 10,
) -> dict[str, Any]:
    stdio = _verify_stdio(descriptor, timeout=timeout)
    visibility = _verify_client_visibility(client, server_name, timeout=timeout)
    return {
        "ok": bool(stdio.get("ok")) and bool(visibility.get("ok", True)),
        "stdio": stdio,
        "client_visibility": visibility,
    }


def _verify_stdio(descriptor: McpServerDescriptor, *, timeout: int) -> dict[str, Any]:
    command = [descriptor.command, *descriptor.args]
    payload = b"".join(
        _frame_json_rpc(method, params, request_id=index)
        for index, (method, params) in enumerate(
            (
                ("initialize", {"protocolVersion": LATEST_PROTOCOL_VERSION}),
                ("tools/list", {}),
                ("tools/call", {"name": "graph_health", "arguments": {}}),
                ("tools/call", {"name": "graph_search", "arguments": {"query": descriptor.name, "limit": 1}}),
                ("tools/call", {"name": "graph_query", "arguments": {"statement": "MATCH (n) DELETE n"}}),
            ),
            start=1,
        )
    )
    try:
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        stdout, stderr = process.communicate(payload, timeout=timeout)
    except subprocess.TimeoutExpired:
        process.kill()  # type: ignore[possibly-unbound]
        return {"ok": False, "command": command, "error": f"stdio smoke timed out after {timeout}s"}
    except OSError as exc:
        return {"ok": False, "command": command, "error": str(exc)}
    responses = _parse_stdio_frames(stdout)
    if process.returncode != 0:
        return {
            "ok": False,
            "command": command,
            "returncode": process.returncode,
            "stderr": stderr.decode("utf-8", errors="replace"),
            "responses": responses,
        }
    checks = _stdio_checks(responses)
    return {
        "ok": all(checks.values()),
        "command": command,
        "checks": checks,
        "responses": responses,
        "stderr": stderr.decode("utf-8", errors="replace"),
    }


def _verify_client_visibility(client: str, server_name: str, *, timeout: int) -> dict[str, Any]:
    command = _visibility_command(client)
    if command is None:
        return {"ok": True, "skipped": True, "reason": f"{client} has no CLI visibility check"}
    executable = command[0]
    if shutil.which(executable) is None:
        return {"ok": True, "skipped": True, "reason": f"{executable} executable not found"}
    completed = subprocess.run(command, capture_output=True, text=True, check=False, timeout=timeout)
    output = f"{completed.stdout}\n{completed.stderr}"
    return {
        "ok": completed.returncode == 0 and server_name in output,
        "command": command,
        "returncode": completed.returncode,
        "found": server_name in output,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def _stdio_checks(responses: list[dict[str, Any]]) -> dict[str, bool]:
    by_id = {response.get("id"): response for response in responses}
    initialized = by_id.get(1, {}).get("result", {}).get("protocolVersion") == LATEST_PROTOCOL_VERSION
    tools = by_id.get(2, {}).get("result", {}).get("tools", [])
    listed = {"graph_health", "graph_search"}.issubset({tool.get("name") for tool in tools})
    health = by_id.get(3, {}).get("result", {}).get("structuredContent", {}).get("ok") is True
    search_no_rpc_error = "error" not in by_id.get(4, {})
    tool_error = by_id.get(5, {}).get("result", {}).get("isError") is True
    return {
        "initialize": initialized,
        "tools_list": listed,
        "graph_health": health,
        "graph_search": search_no_rpc_error,
        "tool_error_result": tool_error,
    }


def _parse_stdio_frames(data: bytes) -> list[dict[str, Any]]:
    messages: list[dict[str, Any]] = []
    cursor = 0
    while cursor < len(data):
        header_end = data.find(b"\r\n\r\n", cursor)
        delimiter_length = 4
        if header_end == -1:
            header_end = data.find(b"\n\n", cursor)
            delimiter_length = 2
        if header_end == -1:
            break
        header = data[cursor:header_end].decode("ascii", errors="replace")
        length = None
        for line in header.splitlines():
            if line.lower().startswith("content-length:"):
                length = int(line.split(":", 1)[1].strip())
                break
        if length is None:
            break
        body_start = header_end + delimiter_length
        body_end = body_start + length
        messages.append(json.loads(data[body_start:body_end].decode("utf-8")))
        cursor = body_end
    return messages


def _frame_json_rpc(method: str, params: dict[str, Any], *, request_id: int) -> bytes:
    body = json.dumps(
        {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params},
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body


def _native_command(client: str, descriptor: McpServerDescriptor, *, scope: str) -> list[str] | None:
    if client == "codex":
        return ["codex", "mcp", "add", descriptor.name, "--", descriptor.command, *descriptor.args]
    if client in {"claude", "claude-project"}:
        return [
            "claude",
            "mcp",
            "add",
            "--transport",
            "stdio",
            "--scope",
            _scope_for_client(client, scope),
            descriptor.name,
            "--",
            descriptor.command,
            *descriptor.args,
        ]
    if client == "openclaw":
        entry = descriptor.stdio_entry(include_type=True)
        return ["openclaw", "mcp", "set", descriptor.name, json.dumps(entry, separators=(",", ":"), sort_keys=True)]
    return None


def _visibility_command(client: str) -> list[str] | None:
    if client == "codex":
        return ["codex", "mcp", "list"]
    if client in {"claude", "claude-project"}:
        return ["claude", "mcp", "list"]
    if client == "openclaw":
        return ["openclaw", "mcp", "list"]
    return None


def _build_descriptor(options: McpInstallOptions) -> McpServerDescriptor:
    config_path = Path(options.setup_config_path).expanduser().resolve()
    repo_root: Path | None = None
    repo_name: str | None = None
    if config_path.exists():
        setup_payload = load_setup_config(config_path)
        repo_root = Path(str(setup_payload["repo_root"])).expanduser().resolve()
        repo_name = str(setup_payload.get("repo_name") or repo_root.name)
    elif options.require_setup_config:
        raise FileNotFoundError(
            f"codebaseGraph setup config does not exist: {config_path}. "
            "Run `codebase-graph setup --mcp-client none` first."
        )
    server_name = options.server_name or default_server_name(repo_name or config_path.parent.parent.name)
    return build_server_descriptor(config_path, repo_root=repo_root, name=server_name)


def _validate_options(options: McpInstallOptions) -> None:
    if options.client not in {*INSTALL_CLIENTS, "none"}:
        supported = ", ".join(sorted([*INSTALL_CLIENTS, "all", "none"]))
        raise ValueError(f"Unsupported MCP client: {options.client}. Supported clients: {supported}")
    if options.scope not in SCOPES:
        raise ValueError(f"Unsupported MCP install scope: {options.scope}. Supported scopes: {', '.join(SCOPES)}")


def _native_executable(client: str) -> str:
    return NATIVE_EXECUTABLES[client]


def _adapter_client_id(client: str, scope: str) -> str:
    if client == "claude" and scope == "project":
        return "claude-project"
    return client


def _scope_for_client(client: str, scope: str) -> str:
    if client == "claude-project":
        return "project"
    return scope


def _missing_native_error(client: str) -> str | None:
    executable = NATIVE_EXECUTABLES.get(client)
    if executable is None:
        return None
    return f"{executable} executable not found"


def _subprocess_error(completed: subprocess.CompletedProcess[str]) -> str:
    output = "\n".join(part for part in (completed.stdout.strip(), completed.stderr.strip()) if part)
    if output:
        return f"exit {completed.returncode}: {output}"
    return f"exit {completed.returncode}"


def _safe_name(value: str) -> str:
    normalized = re.sub(r"[^A-Za-z0-9_-]+", "_", value.strip())
    return normalized.strip("._-").lower() or "repository"
