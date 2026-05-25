from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .state import MCP_SERVER_NAME


@dataclass(frozen=True, slots=True)
class McpConfigResult:
    action: str
    client: str
    path: str | None
    server_name: str
    entry: dict[str, Any]

    def as_dict(self) -> dict[str, Any]:
        return {
            "action": self.action,
            "client": self.client,
            "path": self.path,
            "server_name": self.server_name,
            "entry": self.entry,
        }


def configure_mcp_client(
    *,
    client: str,
    config_path: str | Path | None,
    setup_config_path: Path,
    dry_run: bool = False,
    skip: bool = False,
) -> McpConfigResult:
    entry = server_entry(setup_config_path)
    if skip or client == "none":
        return McpConfigResult("skipped", client, None, MCP_SERVER_NAME, entry)
    path = Path(config_path).expanduser().resolve() if config_path is not None else default_config_path(client)
    next_payload, action = _next_config_payload(path, entry)
    if dry_run:
        return McpConfigResult("dry_run", client, path.as_posix(), MCP_SERVER_NAME, entry)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    with tmp_path.open("w", encoding="utf-8") as handle:
        json.dump(next_payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    os.replace(tmp_path, path)
    return McpConfigResult(action, client, path.as_posix(), MCP_SERVER_NAME, entry)


def server_entry(setup_config_path: Path) -> dict[str, Any]:
    return {
        "command": "codebase-graph",
        "args": ["mcp", "serve", "--config", setup_config_path.as_posix()],
    }


def default_config_path(client: str) -> Path:
    if client == "codex":
        base = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex"))
        return base / "mcp.json"
    if client == "claude":
        mac_path = Path.home() / "Library" / "Application Support" / "Claude" / "claude_desktop_config.json"
        if mac_path.parent.exists():
            return mac_path
        return Path.home() / ".config" / "claude" / "claude_desktop_config.json"
    raise ValueError(f"Unsupported MCP client: {client}")


def _next_config_payload(path: Path, entry: dict[str, Any]) -> tuple[dict[str, Any], str]:
    payload = _read_json(path)
    servers = payload.setdefault("mcpServers", {})
    previous = servers.get(MCP_SERVER_NAME)
    servers[MCP_SERVER_NAME] = entry
    if previous is None:
        return payload, "created"
    if previous == entry:
        return payload, "unchanged"
    return payload, "updated"


def _read_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    with path.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, dict):
        raise ValueError(f"MCP config must contain a JSON object: {path}")
    return payload
