from __future__ import annotations

import os
import shutil
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Mapping

from .state import MCP_SERVER_NAME


@dataclass(frozen=True, slots=True)
class McpServerDescriptor:
    """Represent MCP server descriptor data used by setup workflow and client configuration.

    The class belongs to MCP server descriptor construction from repository setup paths.
    """
    name: str
    transport: str
    command: str
    args: tuple[str, ...]
    env: Mapping[str, str] = field(default_factory=dict)
    cwd: str | None = None
    setup_config_path: str | None = None
    repo_root: str | None = None
    timeout: int = 60
    tool_policy: str | None = "graph_query_read_only"

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the setup workflow and client configuration
            response contract.
        """
        payload: dict[str, Any] = {
            "name": self.name,
            "transport": self.transport,
            "command": self.command,
            "args": list(self.args),
            "env": dict(sorted(self.env.items())),
            "cwd": self.cwd,
            "setup_config_path": self.setup_config_path,
            "repo_root": self.repo_root,
            "timeout": self.timeout,
        }
        if self.tool_policy:
            payload["tool_policy"] = self.tool_policy
        return payload

    def stdio_entry(self, *, include_type: bool = False, include_timeout: bool = False) -> dict[str, Any]:
        """Build entry for setup workflow and client configuration.

        Args:
            include_type: Ontology type name for include handling.
            include_timeout: Include timeout used by the setup workflow and client
            configuration workflow.

        Returns:
            Structured mapping that follows the setup workflow and client configuration
            response contract.
        """
        entry: dict[str, Any] = {"command": self.command, "args": list(self.args)}
        if include_type:
            entry["type"] = "stdio"
        if self.env:
            entry["env"] = dict(sorted(self.env.items()))
        if self.cwd:
            entry["cwd"] = self.cwd
        if include_timeout:
            entry["startup_timeout_sec"] = self.timeout
        return entry


def build_server_descriptor(
    setup_config_path: Path,
    *,
    repo_root: Path | None = None,
    name: str = MCP_SERVER_NAME,
    timeout: int = 60,
) -> McpServerDescriptor:
    """Build server descriptor for setup workflow and client configuration.

    This starts a transport loop and blocks until the server stops.

    Args:
        setup_config_path: Filesystem path for the setup config resource.
        repo_root: Repository root used to resolve graph state paths.
        name: Name used by the setup workflow and client configuration workflow.
        timeout: Subprocess or server timeout in seconds.

    Returns:
        McpServerDescriptor instance populated with data from the setup workflow and client
        configuration workflow.
    """
    config_path = setup_config_path.expanduser().resolve()
    resolved_repo_root = repo_root.expanduser().resolve() if repo_root is not None else config_path.parent.parent
    return McpServerDescriptor(
        name=name,
        transport="stdio",
        command=resolve_server_command(),
        args=("mcp", "serve", "--config", config_path.as_posix()),
        env={},
        cwd=None,
        setup_config_path=config_path.as_posix(),
        repo_root=resolved_repo_root.as_posix(),
        timeout=timeout,
    )


def resolve_server_command() -> str:
    """Resolve server command for setup workflow and client configuration.

    This starts a transport loop and blocks until the server stops.

    Returns:
        Formatted text returned to the caller.
    """
    sibling_script = Path(sys.executable).with_name("codebase-graph")
    if sibling_script.exists() and os.access(sibling_script, os.X_OK):
        return sibling_script.as_posix()
    return shutil.which("codebase-graph") or "codebase-graph"
