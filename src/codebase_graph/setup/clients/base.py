from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from codebase_graph.setup.descriptor import McpServerDescriptor


@dataclass(frozen=True, slots=True)
class RenderedClientConfig:
    text: str
    action: str
    entry: dict[str, Any]
    patch: Any
    payload: Any


class ClientConfigAdapter(Protocol):
    client_id: str

    def default_config_path(self, descriptor: McpServerDescriptor) -> Path:
        ...

    def render(self, existing_text: str | None, descriptor: McpServerDescriptor) -> RenderedClientConfig:
        ...


def action_for_server(previous: Any, next_value: Any, *, file_exists: bool) -> str:
    if previous is None:
        return "created"
    if previous == next_value:
        return "unchanged"
    return "updated"
