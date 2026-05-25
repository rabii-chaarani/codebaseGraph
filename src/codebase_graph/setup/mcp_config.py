from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .clients import get_client_adapter
from .descriptor import build_server_descriptor
from .installer import McpInstallOptions, McpInstallResult, install_mcp_server
from .state import MCP_SERVER_NAME


@dataclass(frozen=True, slots=True)
class McpConfigResult:
    action: str
    client: str
    path: str | None
    server_name: str
    entry: dict[str, Any]
    descriptor: dict[str, Any] | None = None
    method: str | None = None
    scope: str | None = None
    command: list[str] | None = None
    patch: Any = None
    payload: Any = None
    verification: dict[str, Any] | None = None
    native_command: list[str] | None = None
    native_error: str | None = None

    def as_dict(self) -> dict[str, Any]:
        payload = {
            "action": self.action,
            "client": self.client,
            "path": self.path,
            "server_name": self.server_name,
            "entry": self.entry,
        }
        if self.descriptor is not None:
            payload["descriptor"] = self.descriptor
        if self.method is not None:
            payload["method"] = self.method
        if self.scope is not None:
            payload["scope"] = self.scope
        if self.command is not None:
            payload["command"] = self.command
        if self.patch is not None:
            payload["patch"] = self.patch
        if self.payload is not None:
            payload["payload"] = self.payload
        if self.verification is not None:
            payload["verification"] = self.verification
        if self.native_command is not None:
            payload["native_command"] = self.native_command
        if self.native_error is not None:
            payload["native_error"] = self.native_error
        return payload

    @classmethod
    def from_install_result(cls, result: McpInstallResult) -> McpConfigResult:
        return cls(
            action=result.action,
            client=result.client,
            path=result.path,
            server_name=result.server_name,
            entry=result.entry,
            descriptor=result.descriptor,
            method=result.method,
            scope=result.scope,
            command=result.command,
            patch=result.patch,
            payload=result.payload,
            verification=result.verification,
            native_command=result.native_command,
            native_error=result.native_error,
        )


def configure_mcp_client(
    *,
    client: str,
    config_path: str | Path | None,
    setup_config_path: Path,
    dry_run: bool = False,
    skip: bool = False,
) -> McpConfigResult:
    result = install_mcp_server(
        McpInstallOptions(
            client=client,
            scope="project" if client == "claude-project" else "local",
            setup_config_path=setup_config_path,
            server_name=MCP_SERVER_NAME,
            client_config_path=config_path,
            dry_run=dry_run,
            skip=skip,
            require_setup_config=False,
        )
    )
    return McpConfigResult.from_install_result(result)


def server_entry(setup_config_path: Path) -> dict[str, Any]:
    return build_server_descriptor(setup_config_path).stdio_entry()


def default_config_path(client: str) -> Path:
    descriptor = build_server_descriptor(Path.cwd() / ".codebaseGraph" / "config.json")
    return get_client_adapter(client).default_config_path(descriptor)
