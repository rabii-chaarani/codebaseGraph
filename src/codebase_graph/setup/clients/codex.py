from __future__ import annotations

import os
import re
from pathlib import Path
from typing import Any

from codebase_graph.setup.descriptor import McpServerDescriptor

from .base import RenderedClientConfig


class CodexAdapter:
    """Adapt codex operations to the project interface."""
    client_id = "codex"

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        base = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex"))
        return base / "config.toml"

    def render(self, existing_text: str | None, descriptor: McpServerDescriptor) -> RenderedClientConfig:
        """Render the operation.

        Args:
            existing_text: Existing text value.
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        entry = descriptor.stdio_entry(include_timeout=True)
        patch = _toml_block(descriptor, entry)
        next_text, previous = _upsert_toml_block(existing_text or "", descriptor.name, patch)
        if previous is None:
            action = "created"
        elif previous == patch.rstrip():
            action = "unchanged"
        else:
            action = "updated"
        if existing_text == next_text:
            action = "unchanged"
        return RenderedClientConfig(text=next_text, action=action, entry=entry, patch=patch, payload=patch)


def _upsert_toml_block(existing: str, server_name: str, block: str) -> tuple[str, str | None]:
    """Upsert toml block.

    Args:
        existing: Existing value.
        server_name: Server name value.
        block: Block value.

    Returns:
        A tuple containing the computed values.
    """
    lines = existing.splitlines()
    start: int | None = None
    end = len(lines)
    header_re = re.compile(rf"^\[mcp_servers\.{re.escape(server_name)}(?:\.env)?\]\s*$")
    any_header_re = re.compile(r"^\[[^\]]+\]\s*$")
    for index, line in enumerate(lines):
        if header_re.match(line):
            start = index
            break
    if start is None:
        prefix = existing.rstrip()
        separator = "\n\n" if prefix else ""
        return f"{prefix}{separator}{block}", None
    for index in range(start + 1, len(lines)):
        if any_header_re.match(lines[index]) and not header_re.match(lines[index]):
            end = index
            break
    previous = "\n".join(lines[start:end]).rstrip()
    next_lines = [*lines[:start], *block.rstrip().splitlines(), *lines[end:]]
    return "\n".join(next_lines).rstrip() + "\n", previous


def _toml_block(descriptor: McpServerDescriptor, entry: dict[str, Any]) -> str:
    """Render TOML block.

    Args:
        descriptor: The descriptor used by the operation.
        entry: Entry value.

    Returns:
        The computed string.
    """
    lines = [
        f"[mcp_servers.{descriptor.name}]",
        f"command = {_toml_string(entry['command'])}",
        f"args = {_toml_array(entry['args'])}",
        f"startup_timeout_sec = {int(entry['startup_timeout_sec'])}",
    ]
    if descriptor.cwd:
        lines.append(f"cwd = {_toml_string(descriptor.cwd)}")
    if descriptor.env:
        lines.append("")
        lines.append(f"[mcp_servers.{descriptor.name}.env]")
        for key, value in sorted(descriptor.env.items()):
            lines.append(f"{key} = {_toml_string(value)}")
    return "\n".join(lines) + "\n"


def _toml_array(values: list[str]) -> str:
    """Render TOML array.

    Args:
        values: Values value.

    Returns:
        The computed string.
    """
    return "[" + ", ".join(_toml_string(value) for value in values) + "]"


def _toml_string(value: str) -> str:
    """Render TOML string.

    Args:
        value: Value value.

    Returns:
        The computed string.
    """
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'
