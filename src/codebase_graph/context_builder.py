from __future__ import annotations

from typing import Any

from .graph_context import build_compact_graph_context

def assemble_context(query: str, graph: dict[str, Any], *, budget: int = 1200) -> dict[str, Any]:
    return build_compact_graph_context(graph, query, budget=budget, limit=5, include_raw=False)
