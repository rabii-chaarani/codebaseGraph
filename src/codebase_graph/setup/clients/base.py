from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from codebase_graph.setup.descriptor import McpServerDescriptor


@dataclass(frozen=True, slots=True)
class RenderedClientConfig:
    """Carry configuration needed by setup workflow and client configuration operations."""
    text: str
    action: str
    entry: dict[str, Any]
    patch: Any
    payload: Any


class ClientConfigAdapter(Protocol):
    """Adapt client config data to the codebaseGraph interface."""
    client_id: str

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        """Create the default config path for setup workflow and client configuration.

        Args:
            descriptor: MCP server descriptor that will be rendered into client
            configuration.

        Returns:
            Path instance populated with data from the setup workflow and client
            configuration workflow.
        """
        ...

    def render(self, existing_text: str | None, descriptor: McpServerDescriptor) -> RenderedClientConfig:
        """Render setup workflow and client configuration for setup workflow and client configuration.

        Args:
            existing_text: Existing client configuration text, if the file already exists.
            descriptor: MCP server descriptor that will be rendered into client
            configuration.

        Returns:
            RenderedClientConfig instance populated with data from the setup workflow and
            client configuration workflow.
        """
        ...


def action_for_server(previous: Any, next_value: Any, *, file_exists: bool) -> str:
    """Manage for server within setup workflow and client configuration.

    This starts a transport loop and blocks until the server stops.

    Args:
        previous: Previously rendered configuration text.
        next_value: Newly rendered configuration text.
        file_exists: Whether the target configuration file already exists.

    Returns:
        Formatted text returned to the caller.
    """
    if previous is None:
        return "created"
    if previous == next_value:
        return "unchanged"
    return "updated"
