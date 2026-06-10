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
    """Carry the observable outcome of MCP config workflows.

    The class belongs to Compatibility wrapper for configuring a single MCP client from setup
    results.
    """
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
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the setup workflow and client configuration
            response contract.
        """
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
        """Manage install result within setup workflow and client configuration.

        This may spawn a native client command or write a client config file.

        Args:
            result: Result used by the setup workflow and client configuration
            workflow.

        Returns:
            McpConfigResult instance populated with data from the setup workflow and client
            configuration workflow.
        """
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
    """Configure MCP client for setup workflow and client configuration.

    Args:
        client: MCP client identifier selected by setup or install commands.
        config_path: Setup configuration path used to resolve runtime state.
        setup_config_path: Filesystem path for the setup config resource.
        dry_run: Whether the operation should report changes without writing files.
        skip: Whether setup should skip the client configuration step.

    Returns:
        McpConfigResult instance populated with data from the setup workflow and client
        configuration workflow.
    """
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
    """Build entry for setup workflow and client configuration.

    This starts a transport loop and blocks until the server stops.

    Args:
        setup_config_path: Filesystem path for the setup config resource.

    Returns:
        Structured mapping that follows the setup workflow and client configuration
        response contract.
    """
    return build_server_descriptor(setup_config_path).stdio_entry()


def default_config_path(client: str) -> Path:
    """Create the default config path for setup workflow and client configuration.

    Args:
        client: MCP client identifier selected by setup or install commands.

    Returns:
        Path instance populated with data from the setup workflow and client configuration
        workflow.
    """
    descriptor = build_server_descriptor(Path.cwd() / ".codebaseGraph" / "config.json")
    return get_client_adapter(client).default_config_path(descriptor)
