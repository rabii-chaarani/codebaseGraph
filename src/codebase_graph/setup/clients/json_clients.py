from __future__ import annotations

import json
import os
from copy import deepcopy
from pathlib import Path
from typing import Any

from codebase_graph.setup.descriptor import McpServerDescriptor

from .base import RenderedClientConfig, action_for_server


class JsonMcpServersAdapter:
    client_id = "generic"
    include_type = True
    root_path = ("mcpServers",)

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        return Path.home() / ".config" / "mcp" / "mcp.json"

    def entry(self, descriptor: McpServerDescriptor) -> dict[str, Any]:
        return descriptor.stdio_entry(include_type=self.include_type)

    def render(self, existing_text: str | None, descriptor: McpServerDescriptor) -> RenderedClientConfig:
        payload = _read_json_text(existing_text)
        next_payload = deepcopy(payload)
        container = _container(next_payload, self.root_path)
        entry = self.entry(descriptor)
        previous = container.get(descriptor.name)
        container[descriptor.name] = entry
        action = action_for_server(previous, entry, file_exists=existing_text is not None)
        text = json.dumps(next_payload, indent=2, sort_keys=True) + "\n"
        if existing_text == text:
            action = "unchanged"
        return RenderedClientConfig(text=text, action=action, entry=entry, patch=next_payload, payload=next_payload)


class ClaudeAdapter(JsonMcpServersAdapter):
    client_id = "claude"
    include_type = False

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        mac_path = Path.home() / "Library" / "Application Support" / "Claude" / "claude_desktop_config.json"
        if mac_path.parent.exists():
            return mac_path
        return Path.home() / ".config" / "claude" / "claude_desktop_config.json"


class ClaudeProjectAdapter(JsonMcpServersAdapter):
    client_id = "claude-project"

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        if descriptor.repo_root:
            return Path(descriptor.repo_root) / ".mcp.json"
        return Path.cwd() / ".mcp.json"


class LmStudioAdapter(JsonMcpServersAdapter):
    client_id = "lmstudio"

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        return Path.home() / ".lmstudio" / "mcp.json"


class GenericAdapter(JsonMcpServersAdapter):
    client_id = "generic"
    include_type = False


class OpenClawAdapter(JsonMcpServersAdapter):
    client_id = "openclaw"
    root_path = ("mcp", "servers")

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        return Path(os.environ.get("OPENCLAW_HOME", Path.home() / ".openclaw")) / "mcp.json5"


def _read_json_text(existing_text: str | None) -> dict[str, Any]:
    if existing_text is None or not existing_text.strip():
        return {}
    payload = json.loads(existing_text)
    if not isinstance(payload, dict):
        raise ValueError("MCP config must contain a JSON object")
    return payload


def _container(payload: dict[str, Any], path: tuple[str, ...]) -> dict[str, Any]:
    cursor = payload
    for key in path:
        next_value = cursor.setdefault(key, {})
        if not isinstance(next_value, dict):
            raise ValueError(f"MCP config key must contain an object: {'.'.join(path)}")
        cursor = next_value
    return cursor
