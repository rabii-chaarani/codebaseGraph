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
    sibling_script = Path(sys.executable).with_name("codebase-graph")
    if sibling_script.exists() and os.access(sibling_script, os.X_OK):
        return sibling_script.as_posix()
    return shutil.which("codebase-graph") or "codebase-graph"
