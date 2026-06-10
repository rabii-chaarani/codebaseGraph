from __future__ import annotations

import json
import os
from copy import deepcopy
from pathlib import Path
from typing import Any

from codebase_graph.setup.descriptor import McpServerDescriptor

from .base import RenderedClientConfig, action_for_server


class JsonMcpServersAdapter:
    """Adapt json MCP servers operations to the project interface."""
    client_id = "generic"
    include_type = True
    root_path = ("mcpServers",)

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        return Path.home() / ".config" / "mcp" / "mcp.json"

    def entry(self, descriptor: McpServerDescriptor) -> dict[str, Any]:
        """Process entry.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            A dictionary containing the computed payload.
        """
        return descriptor.stdio_entry(include_type=self.include_type)

    def render(self, existing_text: str | None, descriptor: McpServerDescriptor) -> RenderedClientConfig:
        """Render the operation.

        Args:
            existing_text: Existing text value.
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
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
    """Adapt claude operations to the project interface."""
    client_id = "claude"
    include_type = False

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        mac_path = Path.home() / "Library" / "Application Support" / "Claude" / "claude_desktop_config.json"
        if mac_path.parent.exists():
            return mac_path
        return Path.home() / ".config" / "claude" / "claude_desktop_config.json"


class ClaudeProjectAdapter(JsonMcpServersAdapter):
    """Adapt claude project operations to the project interface."""
    client_id = "claude-project"

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        if descriptor.repo_root:
            return Path(descriptor.repo_root) / ".mcp.json"
        return Path.cwd() / ".mcp.json"


class LmStudioAdapter(JsonMcpServersAdapter):
    """Adapt lm studio operations to the project interface."""
    client_id = "lmstudio"

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        return Path.home() / ".lmstudio" / "mcp.json"


class GenericAdapter(JsonMcpServersAdapter):
    """Adapt generic operations to the project interface."""
    client_id = "generic"
    include_type = False


class OpenClawAdapter(JsonMcpServersAdapter):
    """Adapt open claw operations to the project interface."""
    client_id = "openclaw"
    root_path = ("mcp", "servers")

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        return Path(os.environ.get("OPENCLAW_HOME", Path.home() / ".openclaw")) / "mcp.json5"


def _read_json_text(existing_text: str | None) -> dict[str, Any]:
    """Read JSON text.

    Args:
        existing_text: Existing text value.

    Returns:
        A dictionary containing the computed payload.
    """
    if existing_text is None or not existing_text.strip():
        return {}
    payload = json.loads(existing_text)
    if not isinstance(payload, dict):
        raise ValueError("MCP config must contain a JSON object")
    return payload


def _container(payload: dict[str, Any], path: tuple[str, ...]) -> dict[str, Any]:
    """Process container.

    Args:
        payload: Payload to process.
        path: The path to read or write.

    Returns:
        A dictionary containing the computed payload.
    """
    cursor = payload
    for key in path:
        next_value = cursor.setdefault(key, {})
        if not isinstance(next_value, dict):
            raise ValueError(f"MCP config key must contain an object: {'.'.join(path)}")
        cursor = next_value
    return cursor
