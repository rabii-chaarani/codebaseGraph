from __future__ import annotations

from .base import ClientConfigAdapter, RenderedClientConfig
from .codex import CodexAdapter
from .hermes import HermesAdapter
from .json_clients import ClaudeAdapter, ClaudeProjectAdapter, GenericAdapter, LmStudioAdapter, OpenClawAdapter

ADAPTERS: dict[str, ClientConfigAdapter] = {
    adapter.client_id: adapter
    for adapter in (
        CodexAdapter(),
        ClaudeAdapter(),
        ClaudeProjectAdapter(),
        LmStudioAdapter(),
        HermesAdapter(),
        OpenClawAdapter(),
        GenericAdapter(),
    )
}


def get_client_adapter(client_id: str) -> ClientConfigAdapter:
    try:
        return ADAPTERS[client_id]
    except KeyError as exc:
        supported = ", ".join(sorted([*ADAPTERS, "none"]))
        raise ValueError(f"Unsupported MCP client: {client_id}. Supported clients: {supported}") from exc


def supported_client_ids() -> tuple[str, ...]:
    return tuple(sorted([*ADAPTERS, "none"]))


__all__ = ["ADAPTERS", "ClientConfigAdapter", "RenderedClientConfig", "get_client_adapter", "supported_client_ids"]
