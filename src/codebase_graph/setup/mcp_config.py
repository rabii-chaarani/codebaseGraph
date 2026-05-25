from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .clients import get_client_adapter
from .descriptor import build_server_descriptor
from .state import MCP_SERVER_NAME


@dataclass(frozen=True, slots=True)
class McpConfigResult:
    action: str
    client: str
    path: str | None
    server_name: str
    entry: dict[str, Any]
    descriptor: dict[str, Any] | None = None
    patch: Any = None
    payload: Any = None

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
        if self.patch is not None:
            payload["patch"] = self.patch
        if self.payload is not None:
            payload["payload"] = self.payload
        return payload


def configure_mcp_client(
    *,
    client: str,
    config_path: str | Path | None,
    setup_config_path: Path,
    dry_run: bool = False,
    skip: bool = False,
) -> McpConfigResult:
    descriptor = build_server_descriptor(setup_config_path)
    entry = descriptor.stdio_entry()
    if skip or client == "none":
        return McpConfigResult("skipped", client, None, MCP_SERVER_NAME, entry, descriptor=descriptor.as_dict())
    adapter = get_client_adapter(client)
    path = Path(config_path).expanduser().resolve() if config_path is not None else adapter.default_config_path(descriptor)
    existing_text = path.read_text(encoding="utf-8") if path.exists() else None
    rendered = adapter.render(existing_text, descriptor)
    if dry_run:
        return McpConfigResult(
            "dry_run",
            client,
            path.as_posix(),
            MCP_SERVER_NAME,
            rendered.entry,
            descriptor=descriptor.as_dict(),
            patch=rendered.patch,
            payload=rendered.payload,
        )
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    with tmp_path.open("w", encoding="utf-8") as handle:
        handle.write(rendered.text)
    os.replace(tmp_path, path)
    return McpConfigResult(
        rendered.action,
        client,
        path.as_posix(),
        MCP_SERVER_NAME,
        rendered.entry,
        descriptor=descriptor.as_dict(),
        patch=rendered.patch,
        payload=rendered.payload,
    )


def server_entry(setup_config_path: Path) -> dict[str, Any]:
    return build_server_descriptor(setup_config_path).stdio_entry()


def default_config_path(client: str) -> Path:
    descriptor = build_server_descriptor(Path.cwd() / ".codebaseGraph" / "config.json")
    return get_client_adapter(client).default_config_path(descriptor)
