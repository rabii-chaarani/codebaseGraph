from __future__ import annotations

from typing import Any

def build_compact_graph_context(
    graph: dict[str, Any],
    query: str,
    *,
    kind: str | None = None,
    profile: str = "dependencies",
    limit: int = 3,
    max_depth: int = 1,
    budget: int = 600,
    include_raw: bool = False,
) -> dict[str, Any]:
    nodes = list(graph.get("nodes", []))
    edges = list(graph.get("edges", []))
    matches = _match_nodes(nodes, query, kind=kind)[:limit]
    match_ids = {node["id"] for node in matches}
    related_edges = [edge for edge in edges if edge.get("source_id") in match_ids or edge.get("target_id") in match_ids]
    related_ids = {edge.get("source_id") for edge in related_edges} | {edge.get("target_id") for edge in related_edges}
    related_nodes = [node for node in nodes if node.get("id") in related_ids and node.get("id") not in match_ids]
    lines: list[str] = []
    for node in matches:
        label = node.get("qualified_name") or node.get("label") or node.get("id")
        path = node.get("path") or ""
        lines.append(f"- {node.get('table')}: {label} {f'({path})' if path else ''}".strip())
    for edge in related_edges[: max(0, budget // 80)]:
        lines.append(f"- {edge.get('type')}: {edge.get('source_id')} -> {edge.get('target_id')}")
    text = "\n".join(lines)
    if len(text) > budget:
        text = text[:budget].rstrip()
    payload = {
        "query": query,
        "profile": profile,
        "max_depth": max_depth,
        "context": text,
        "items": matches,
        "related": related_nodes[:limit],
        "edge_count": len(related_edges),
    }
    if include_raw:
        payload["raw_edges"] = related_edges
    return payload

def _match_nodes(nodes: list[dict[str, Any]], query: str, kind: str | None = None) -> list[dict[str, Any]]:
    terms = [term for term in query.lower().replace("_", " ").replace(".", " ").split() if term]
    scored: list[tuple[int, dict[str, Any]]] = []
    for node in nodes:
        if kind and node.get("table") != kind and node.get("kind") != kind:
            continue
        haystack = " ".join(
            str(node.get(field, "")) for field in ("id", "label", "qualified_name", "path", "summary", "kind", "table")
        ).lower()
        score = sum(1 for term in terms if term in haystack)
        if score or not terms:
            scored.append((score, node))
    return [node for _, node in sorted(scored, key=lambda item: (-item[0], item[1].get("id", "")))]
