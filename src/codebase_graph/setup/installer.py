from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

from codebase_graph.mcp.protocol import LATEST_PROTOCOL_VERSION

from .clients import get_client_adapter
from .descriptor import McpServerDescriptor, build_server_descriptor
from .state import MCP_SERVER_NAME, load_setup_config

SCOPES = ("local", "user", "project")
NativeCommandBuilder = Callable[[McpServerDescriptor, str], list[str]]
VisibilityCommandBuilder = Callable[[], list[str]]
ManualMetadataBuilder = Callable[[McpServerDescriptor], dict[str, Any]]


@dataclass(frozen=True, slots=True)
class McpInstallOptions:
    """Collect caller options for MCP install workflows.

    The class belongs to MCP client installation workflow across native CLIs and file-adapter
    fallbacks.
    """
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
    """Carry the observable outcome of MCP install workflows.

    The class belongs to MCP client installation workflow across native CLIs and file-adapter
    fallbacks.
    """
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
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the setup workflow and client configuration
            response contract.
        """
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


@dataclass(frozen=True, slots=True)
class InstallClientStrategy:
    """Represent install client strategy data used by setup workflow and client configuration.

    The class belongs to MCP client installation workflow across native CLIs and file-adapter
    fallbacks.
    """
    client_id: str
    adapter_id: str | None = None
    project_adapter_id: str | None = None
    forced_scope: str | None = None
    native_executable: str | None = None
    native_command_builder: NativeCommandBuilder | None = None
    visibility_command_builder: VisibilityCommandBuilder | None = None
    manual_metadata_builder: ManualMetadataBuilder | None = None

    def install_scope(self, scope: str) -> str:
        """Install scope for setup workflow and client configuration.

        This may spawn a native client command or write a client config file.

        Args:
            scope: Client-specific install scope.

        Returns:
            Formatted text returned to the caller.
        """
        return self.forced_scope or scope

    def adapter_client_id(self, scope: str) -> str:
        """Manage client identifier within setup workflow and client configuration.

        Args:
            scope: Client-specific install scope.

        Returns:
            Formatted text returned to the caller.
        """
        if self.project_adapter_id is not None and self.install_scope(scope) == "project":
            return self.project_adapter_id
        return self.adapter_id or self.client_id

    def native_command(self, descriptor: McpServerDescriptor, *, scope: str) -> list[str] | None:
        """Build command for setup workflow and client configuration.

        Args:
            descriptor: MCP server descriptor that will be rendered into client
            configuration.
            scope: Client-specific install scope.

        Returns:
            Ordered results returned to the setup workflow and client configuration caller.
        """
        if self.native_command_builder is None:
            return None
        return self.native_command_builder(descriptor, self.install_scope(scope))

    def visibility_command(self) -> list[str] | None:
        """Manage command within setup workflow and client configuration.

        Returns:
            Ordered results returned to the setup workflow and client configuration caller.
        """
        if self.visibility_command_builder is None:
            return None
        return self.visibility_command_builder()

    def manual_metadata(self, descriptor: McpServerDescriptor) -> dict[str, Any] | None:
        """Build manual setup metadata for clients without a local config file."""
        if self.manual_metadata_builder is None:
            return None
        return self.manual_metadata_builder(descriptor)


def _codex_native_command(descriptor: McpServerDescriptor, scope: str) -> list[str]:
    """Manage native command within setup workflow and client configuration.

    Args:
        descriptor: MCP server descriptor that will be rendered into client configuration.
        scope: Client-specific install scope.

    Returns:
        Ordered results returned to the setup workflow and client configuration caller.
    """
    return ["codex", "mcp", "add", descriptor.name, "--", descriptor.command, *descriptor.args]


def _claude_native_command(descriptor: McpServerDescriptor, scope: str) -> list[str]:
    """Manage native command within setup workflow and client configuration.

    Args:
        descriptor: MCP server descriptor that will be rendered into client configuration.
        scope: Client-specific install scope.

    Returns:
        Ordered results returned to the setup workflow and client configuration caller.
    """
    return [
        "claude",
        "mcp",
        "add",
        "--transport",
        "stdio",
        "--scope",
        scope,
        descriptor.name,
        "--",
        descriptor.command,
        *descriptor.args,
    ]


def _openclaw_native_command(descriptor: McpServerDescriptor, scope: str) -> list[str]:
    """Manage native command within setup workflow and client configuration.

    Args:
        descriptor: MCP server descriptor that will be rendered into client configuration.
        scope: Client-specific install scope.

    Returns:
        Ordered results returned to the setup workflow and client configuration caller.
    """
    entry = descriptor.stdio_entry(include_type=True)
    return ["openclaw", "mcp", "set", descriptor.name, json.dumps(entry, separators=(",", ":"), sort_keys=True)]


def _copilot_studio_metadata(descriptor: McpServerDescriptor) -> dict[str, Any]:
    """Build manual connection details for Microsoft Copilot Studio."""
    config_path = descriptor.setup_config_path or ".codebaseGraph/config.json"
    http_start_command = [
        descriptor.command,
        "mcp",
        "http",
        "--config",
        config_path,
        "--host",
        "127.0.0.1",
        "--port",
        "8765",
        "--path",
        "/mcp",
    ]
    return {
        "kind": "copilot_studio_manual_metadata",
        "stdio": descriptor.stdio_entry(include_type=True),
        "http": {
            "url": "http://127.0.0.1:8765/mcp",
            "start_command": http_start_command,
            "host": "127.0.0.1",
            "port": 8765,
            "path": "/mcp",
        },
        "notes": [
            "No local client configuration file is written for Copilot Studio.",
            "Remote Copilot Studio use requires user-managed endpoint exposure, bearer-token configuration, and TLS.",
        ],
    }


INSTALL_STRATEGIES: dict[str, InstallClientStrategy] = {
    "codex": InstallClientStrategy(
        client_id="codex",
        native_executable="codex",
        native_command_builder=_codex_native_command,
        visibility_command_builder=lambda: ["codex", "mcp", "list"],
    ),
    "claude": InstallClientStrategy(
        client_id="claude",
        project_adapter_id="claude-project",
        native_executable="claude",
        native_command_builder=_claude_native_command,
        visibility_command_builder=lambda: ["claude", "mcp", "list"],
    ),
    "claude-project": InstallClientStrategy(
        client_id="claude-project",
        forced_scope="project",
        native_executable="claude",
        native_command_builder=_claude_native_command,
        visibility_command_builder=lambda: ["claude", "mcp", "list"],
    ),
    "lmstudio": InstallClientStrategy(client_id="lmstudio"),
    "github-copilot": InstallClientStrategy(client_id="github-copilot"),
    "hermes": InstallClientStrategy(client_id="hermes"),
    "openclaw": InstallClientStrategy(
        client_id="openclaw",
        native_executable="openclaw",
        native_command_builder=_openclaw_native_command,
        visibility_command_builder=lambda: ["openclaw", "mcp", "list"],
    ),
    "generic": InstallClientStrategy(client_id="generic"),
    "copilot-studio": InstallClientStrategy(
        client_id="copilot-studio",
        manual_metadata_builder=_copilot_studio_metadata,
    ),
    "microsoft-copilot": InstallClientStrategy(
        client_id="microsoft-copilot",
        manual_metadata_builder=_copilot_studio_metadata,
    ),
}
INSTALL_CLIENTS = tuple(INSTALL_STRATEGIES)


def supported_install_client_ids(*, include_all: bool = False) -> tuple[str, ...]:
    """Return install client identifiers for setup workflow and client configuration.

    This may spawn a native client command or write a client config file.

    Args:
        include_all: Include all used by the setup workflow and client configuration
        workflow.

    Returns:
        Tuple of stable results returned to the setup workflow and client configuration
        caller.
    """
    values = [*INSTALL_CLIENTS]
    if include_all:
        values.append("all")
    return tuple(sorted(values))


def default_server_name(repo_name: str | None) -> str:
    """Create a namespace-safe MCP server name for a repository.

    Args:
        repo_name: Repository name that should appear in the generated MCP server key.

    Returns:
        Stable server name safe for supported client configuration formats.
    """
    safe_repo_name = _safe_name(repo_name or "repository")
    return f"{MCP_SERVER_NAME}_{safe_repo_name}"


def install_mcp_clients(options: McpInstallOptions) -> list[McpInstallResult]:
    """Install MCP clients for setup workflow and client configuration.

    This may spawn a native client command or write a client config file.

    Args:
        options: Caller-selected setup or install options.

    Returns:
        Ordered results returned to the setup workflow and client configuration caller.
    """
    if options.client == "all":
        return [_install_with_failure_result(options, client) for client in INSTALL_CLIENTS]
    return [install_mcp_server(options)]


def install_mcp_server(options: McpInstallOptions) -> McpInstallResult:
    """Install one MCP client using a native CLI when available or a file-adapter fallback.

    This may spawn a native client command or write a client config file.

    Args:
        options: Caller-selected setup or install options.

    Returns:
        McpInstallResult instance populated with data from the setup workflow and client
        configuration workflow.
    """
    _validate_options(options)
    strategy = _client_strategy(options.client)
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

    manual_metadata = strategy.manual_metadata(descriptor)
    if manual_metadata is not None:
        result = _manual_metadata_result(
            "dry_run" if options.dry_run else "reported",
            options,
            descriptor,
            manual_metadata,
        )
        return _with_verification(result, descriptor, options.verify and not options.dry_run)

    native_command = strategy.native_command(descriptor, scope=options.scope)
    use_native = (
        options.prefer_native
        and options.client_config_path is None
        and native_command is not None
        and strategy.native_executable is not None
        and shutil.which(strategy.native_executable)
    )
    # Native CLIs are preferred when available because they preserve client-
    # specific behavior; file adapters remain the fallback for portability and CI.
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
        # If the native command is unavailable or fails, fall back to rendering
        # the client config file directly and surface the native error in result.
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
        native_error=_missing_native_error(strategy) if native_command is not None else None,
    )


def _install_with_failure_result(options: McpInstallOptions, client: str) -> McpInstallResult:
    """Install with failure result for setup workflow and client configuration.

    This may spawn a native client command or write a client config file.

    Args:
        options: Caller-selected setup or install options.
        client: MCP client identifier selected by setup or install commands.

    Returns:
        McpInstallResult instance populated with data from the setup workflow and client
        configuration workflow.
    """
    client_options = McpInstallOptions(
        client=client,
        scope=_client_strategy(client).install_scope(options.scope),
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


def _manual_metadata_result(
    action: str,
    options: McpInstallOptions,
    descriptor: McpServerDescriptor,
    metadata: dict[str, Any],
) -> McpInstallResult:
    """Build metadata-only result for clients that require manual onboarding."""
    return McpInstallResult(
        action=action,
        client=options.client,
        scope=options.scope,
        server_name=descriptor.name,
        method="manual_metadata",
        path=None,
        command=None,
        descriptor=descriptor.as_dict(),
        entry=metadata["stdio"],
        patch=None,
        payload=metadata,
    )


def _file_adapter_result(
    options: McpInstallOptions,
    descriptor: McpServerDescriptor,
    *,
    dry_run: bool,
    native_command: list[str] | None = None,
    native_error: str | None = None,
) -> McpInstallResult:
    """Manage adapter result within setup workflow and client configuration.

    Args:
        options: Caller-selected setup or install options.
        descriptor: MCP server descriptor that will be rendered into client configuration.
        dry_run: Whether the operation should report changes without writing files.
        native_command: Client CLI command used for native MCP installation.
        native_error: Error captured from a failed native install attempt.

    Returns:
        McpInstallResult instance populated with data from the setup workflow and client
        configuration workflow.
    """
    adapter = get_client_adapter(_client_strategy(options.client).adapter_client_id(options.scope))
    path = (
        Path(options.client_config_path).expanduser().resolve()
        if options.client_config_path is not None
        else adapter.default_config_path(descriptor)
    )
    existing_text = path.read_text(encoding="utf-8") if path.exists() else None
    rendered = adapter.render(existing_text, descriptor)
    action = "dry_run" if dry_run else rendered.action
    if not dry_run:
        # Write through a sibling temp file so interrupted installs do not corrupt
        # an existing client configuration.
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
    """Build result for setup workflow and client configuration.

    Args:
        action: Action used by the setup workflow and client configuration workflow.
        options: Caller-selected setup or install options.
        descriptor: MCP server descriptor that will be rendered into client configuration.
        command: Command used by the setup workflow and client configuration workflow.
        verification: Verification used by the setup workflow and client configuration
        workflow.

    Returns:
        McpInstallResult instance populated with data from the setup workflow and client
        configuration workflow.
    """
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
    """Attach verification for setup workflow and client configuration.

    Args:
        result: Result used by the setup workflow and client configuration workflow.
        descriptor: MCP server descriptor that will be rendered into client configuration.
        enabled: Enabled used by the setup workflow and client configuration workflow.

    Returns:
        McpInstallResult instance populated with data from the setup workflow and client
        configuration workflow.
    """
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
    """Verify MCP install for setup workflow and client configuration.

    This may spawn a native client command or write a client config file.

    Args:
        descriptor: MCP server descriptor that will be rendered into client configuration.
        client: MCP client identifier selected by setup or install commands.
        server_name: MCP server name used as a stable client config key.
        timeout: Subprocess or server timeout in seconds.

    Returns:
        Structured mapping that follows the setup workflow and client configuration
        response contract.
    """
    stdio = _verify_stdio(descriptor, timeout=timeout)
    visibility = _verify_client_visibility(client, server_name, timeout=timeout)
    return {
        "ok": bool(stdio.get("ok")) and bool(visibility.get("ok", True)),
        "stdio": stdio,
        "client_visibility": visibility,
    }


def _verify_stdio(descriptor: McpServerDescriptor, *, timeout: int) -> dict[str, Any]:
    """Verify stdio for setup workflow and client configuration.

    Args:
        descriptor: MCP server descriptor that will be rendered into client configuration.
        timeout: Subprocess or server timeout in seconds.

    Returns:
        Structured mapping that follows the setup workflow and client configuration
        response contract.
    """
    command = [descriptor.command, *descriptor.args]
    payload = b"".join(
        _stdio_json_rpc_message(method, params, request_id=index)
        for index, (method, params) in enumerate(
            (
                ("initialize", {"protocolVersion": LATEST_PROTOCOL_VERSION}),
                ("tools/list", {}),
                ("tools/call", {"name": "graph_health", "arguments": {"include_structured_content": True}}),
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
    responses = _parse_stdio_messages(stdout)
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
    """Verify client visibility for setup workflow and client configuration.

    Args:
        client: MCP client identifier selected by setup or install commands.
        server_name: MCP server name used as a stable client config key.
        timeout: Subprocess or server timeout in seconds.

    Returns:
        Structured mapping that follows the setup workflow and client configuration
        response contract.
    """
    command = _client_strategy(client).visibility_command()
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
    """Build checks for setup workflow and client configuration.

    Args:
        responses: Responses used by the setup workflow and client configuration
        workflow.

    Returns:
        Structured mapping that follows the setup workflow and client configuration
        response contract.
    """
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


def _parse_stdio_messages(data: bytes) -> list[dict[str, Any]]:
    """Parse stdio messages for setup workflow and client configuration.

    Args:
        data: Raw bytes received from a transport.

    Returns:
        Structured mapping that follows the setup workflow and client configuration
        response contract.
    """
    messages: list[dict[str, Any]] = []
    for line in data.splitlines():
        if not line:
            continue
        messages.append(json.loads(line.decode("utf-8")))
    return messages


def _stdio_json_rpc_message(method: str, params: dict[str, Any], *, request_id: int) -> bytes:
    """Build JSON RPC message for setup workflow and client configuration.

    Args:
        method: Method used by the setup workflow and client configuration workflow.
        params: Params used by the setup workflow and client configuration workflow.
        request_id: Identifier for the request graph object.

    Returns:
        bytes instance populated with data from the setup workflow and client configuration
        workflow.
    """
    body = json.dumps(
        {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params},
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return body + b"\n"


def _build_descriptor(options: McpInstallOptions) -> McpServerDescriptor:
    """Build descriptor for setup workflow and client configuration.

    Args:
        options: Caller-selected setup or install options.

    Returns:
        McpServerDescriptor instance populated with data from the setup workflow and client
        configuration workflow.

    Raises:
        FileNotFoundError: Raised when validation or runtime preconditions fail.
    """
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
    """Validate options for setup workflow and client configuration.

    Args:
        options: Caller-selected setup or install options.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    if options.client not in {*INSTALL_CLIENTS, "none"}:
        supported = ", ".join(sorted([*INSTALL_CLIENTS, "all", "none"]))
        raise ValueError(f"Unsupported MCP client: {options.client}. Supported clients: {supported}")
    if options.scope not in SCOPES:
        raise ValueError(f"Unsupported MCP install scope: {options.scope}. Supported scopes: {', '.join(SCOPES)}")


def _client_strategy(client: str) -> InstallClientStrategy:
    """Manage strategy within setup workflow and client configuration.

    Args:
        client: MCP client identifier selected by setup or install commands.

    Returns:
        InstallClientStrategy instance populated with data from the setup workflow and
        client configuration workflow.
    """
    if client == "none":
        return InstallClientStrategy(client_id="none")
    return INSTALL_STRATEGIES[client]


def _missing_native_error(strategy: InstallClientStrategy) -> str | None:
    """Manage native error within setup workflow and client configuration.

    Args:
        strategy: Strategy used by the setup workflow and client configuration
        workflow.

    Returns:
        str | None instance populated with data from the setup workflow and client
        configuration workflow.
    """
    if strategy.native_executable is None:
        return None
    return f"{strategy.native_executable} executable not found"


def _subprocess_error(completed: subprocess.CompletedProcess[str]) -> str:
    """Summarize error for setup workflow and client configuration.

    Args:
        completed: Completed used by the setup workflow and client configuration
        workflow.

    Returns:
        Formatted text returned to the caller.
    """
    output = "\n".join(part for part in (completed.stdout.strip(), completed.stderr.strip()) if part)
    if output:
        return f"exit {completed.returncode}: {output}"
    return f"exit {completed.returncode}"


def _safe_name(value: str) -> str:
    """Sanitize name for setup workflow and client configuration.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Formatted text returned to the caller.
    """
    normalized = re.sub(r"[^A-Za-z0-9_-]+", "_", value.strip())
    return normalized.strip("._-").lower() or "repository"
