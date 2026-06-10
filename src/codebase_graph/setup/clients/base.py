from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from codebase_graph.setup.descriptor import McpServerDescriptor


@dataclass(frozen=True, slots=True)
class RenderedClientConfig:
    """Store configuration for rendered client operations."""
    text: str
    action: str
    entry: dict[str, Any]
    patch: Any
    payload: Any


class ClientConfigAdapter(Protocol):
    """Adapt client config operations to the project interface."""
    client_id: str

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path.

        Args:
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        ...

    def render(self, existing_text: str | None, descriptor: McpServerDescriptor) -> RenderedClientConfig:
        """Render the operation.

        Args:
            existing_text: Existing text value.
            descriptor: The descriptor used by the operation.

        Returns:
            The computed result.
        """
        ...


def action_for_server(previous: Any, next_value: Any, *, file_exists: bool) -> str:
    """Process action for server.

    Args:
        previous: Previous value.
        next_value: Next value to compare.
        file_exists: File exists value.

    Returns:
        The computed string.
    """
    if previous is None:
        return "created"
    if previous == next_value:
        return "unchanged"
    return "updated"
