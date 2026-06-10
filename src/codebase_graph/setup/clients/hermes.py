from __future__ import annotations

from pathlib import Path
from typing import Any

from codebase_graph.setup.descriptor import McpServerDescriptor

from .base import RenderedClientConfig

START_MARKER = "# codebaseGraph MCP server start"
END_MARKER = "# codebaseGraph MCP server end"


class HermesAdapter:
    """Adapt hermes operations to the project interface."""
    client_id = "hermes"

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        return Path.home() / ".hermes" / "config.yaml"

    def render(self, existing_text: str | None, descriptor: McpServerDescriptor) -> RenderedClientConfig:
        """Render the operation.

        Args:
            existing_text: Existing text value.
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        entry = descriptor.stdio_entry(include_type=True)
        patch = _yaml_block(descriptor, entry)
        next_text, previous = _upsert_marked_block(existing_text or "", patch)
        if previous is None:
            action = "created"
        elif previous == patch.rstrip():
            action = "unchanged"
        else:
            action = "updated"
        if existing_text == next_text:
            action = "unchanged"
        return RenderedClientConfig(text=next_text, action=action, entry=entry, patch=patch, payload=patch)


def _upsert_marked_block(existing: str, block: str) -> tuple[str, str | None]:
    """Upsert marked block.

    Args:
        existing: Existing value.
        block: Block value.

    Returns:
        A tuple containing the computed values.
    """
    start = existing.find(START_MARKER)
    end = existing.find(END_MARKER)
    if start == -1 or end == -1 or end < start:
        prefix = existing.rstrip()
        separator = "\n\n" if prefix else ""
        return f"{prefix}{separator}{block}", None
    after_end = end + len(END_MARKER)
    previous = existing[start:after_end].rstrip()
    next_text = existing[:start].rstrip() + "\n\n" + block.rstrip() + "\n\n" + existing[after_end:].lstrip()
    return next_text.rstrip() + "\n", previous


def _yaml_block(descriptor: McpServerDescriptor, entry: dict[str, Any]) -> str:
    """Render YAML block.

    Args:
        descriptor: The descriptor used by the operation.
        entry: Entry value.

    Returns:
        The computed string.
    """
    lines = [
        START_MARKER,
        "mcp_servers:",
        f"  {descriptor.name}:",
        "    type: stdio",
        f"    command: {_yaml_scalar(entry['command'])}",
        "    args:",
    ]
    for arg in entry["args"]:
        lines.append(f"      - {_yaml_scalar(arg)}")
    if descriptor.cwd:
        lines.append(f"    cwd: {_yaml_scalar(descriptor.cwd)}")
    if descriptor.env:
        lines.append("    env:")
        for key, value in sorted(descriptor.env.items()):
            lines.append(f"      {key}: {_yaml_scalar(value)}")
    lines.append(END_MARKER)
    return "\n".join(lines) + "\n"


def _yaml_scalar(value: str) -> str:
    """Render YAML scalar.

    Args:
        value: Value value.

    Returns:
        The computed string.
    """
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'
