from __future__ import annotations

import hashlib
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from typing import Any

from codebase_graph.core import CodeGraph, GraphEdge
from codebase_graph.ontology import get_relation_type

from .providers import ProviderResult


@dataclass(frozen=True, slots=True)
class CallResolution:
    """Semantic target selected for a call expression."""

    call_node_id: str
    caller_node_id: str
    target_node_id: str
    confidence: float
    source: str


@dataclass(frozen=True, slots=True)
class TypeResolution:
    """Semantic target selected for a type annotation or type reference."""

    type_node_id: str
    target_node_id: str
    confidence: float
    source: str


def enrich_call_and_type_relations(
    graphs: CodeGraph | Iterable[CodeGraph],
    *,
    provider_results: Sequence[ProviderResult] = (),
) -> tuple[CallResolution | TypeResolution, ...]:
    """Add call and type semantic edges from existing resolved references."""
    del provider_results
    resolutions: list[CallResolution | TypeResolution] = []
    for graph in _graph_tuple(graphs):
        for edge in tuple(graph.edges_by_type("ResolvesTo")):
            source = graph.nodes.get(edge.source_id)
            target = graph.nodes.get(edge.target_id)
            if source is None or target is None:
                continue
            if source.table == "CallExpression":
                resolution = resolve_call_target(graph, edge)
                if resolution is not None:
                    resolutions.append(resolution)
            elif source.table == "TypeAnnotation":
                resolution = resolve_type_annotation(graph, edge)
                if resolution is not None:
                    resolutions.append(resolution)
    return tuple(resolutions)


def resolve_call_target(graph: CodeGraph, resolution_edge: GraphEdge) -> CallResolution | None:
    """Resolve a call expression to a callable target when evidence is sufficient."""
    source = graph.nodes.get(resolution_edge.source_id)
    target = graph.nodes.get(resolution_edge.target_id)
    if source is None or target is None or source.table != "CallExpression":
        return None
    if target.table not in {"Function", "Method", "Class", "APIEndpoint"}:
        return None
    _edge_if_allowed(
        graph,
        "Calls",
        source.id,
        target.id,
        "semantic_call_target",
        confidence=resolution_edge.confidence,
        metadata={"resolver": "semantic", "source_edge": resolution_edge.id},
    )
    caller_id = source.scope_id
    return CallResolution(
        call_node_id=source.id,
        caller_node_id=caller_id,
        target_node_id=target.id,
        confidence=resolution_edge.confidence,
        source=str(resolution_edge.metadata.get("resolution_source") or "semantic"),
    )


def resolve_type_annotation(graph: CodeGraph, resolution_edge: GraphEdge) -> TypeResolution | None:
    """Resolve a type annotation to a declaration node."""
    source = graph.nodes.get(resolution_edge.source_id)
    target = graph.nodes.get(resolution_edge.target_id)
    if source is None or target is None or source.table != "TypeAnnotation":
        return None
    _edge_if_allowed(
        graph,
        "References",
        source.id,
        target.id,
        "semantic_type_reference",
        confidence=resolution_edge.confidence,
        metadata={"resolver": "semantic", "source_edge": resolution_edge.id},
    )
    return TypeResolution(
        type_node_id=source.id,
        target_node_id=target.id,
        confidence=resolution_edge.confidence,
        source=str(resolution_edge.metadata.get("resolution_source") or "semantic"),
    )


def resolve_method_receiver(graph: CodeGraph, call_node_id: str) -> CallResolution | None:
    """Use receiver context to distinguish methods from same-named functions."""
    call = graph.nodes.get(call_node_id)
    if call is None or call.table != "CallExpression":
        return None
    terminal = call.label.rsplit(".", 1)[-1].rsplit("::", 1)[-1]
    candidates = [node for node in graph.nodes_by_type("Method") if node.label == terminal]
    if not candidates:
        return None
    target = sorted(candidates, key=lambda item: item.qualified_name)[0]
    edge = _edge_if_allowed(
        graph,
        "Calls",
        call.id,
        target.id,
        "semantic_receiver_call",
        confidence=0.78,
        metadata={"resolver": "semantic_receiver", "label": call.label},
    )
    if edge is None:
        return None
    return CallResolution(call.id, call.scope_id, target.id, edge.confidence, "receiver")


def resolve_provider_backed_relation(
    graph: CodeGraph,
    source_node_id: str,
    target_node_id: str,
    *,
    confidence: float = 0.95,
) -> CallResolution | TypeResolution | None:
    """Promote high-confidence provider answers into call and type relationships."""
    source = graph.nodes.get(source_node_id)
    if source is None:
        return None
    if source.table == "CallExpression":
        edge = _edge_if_allowed(
            graph,
            "Calls",
            source_node_id,
            target_node_id,
            "provider_call_target",
            confidence=confidence,
            metadata={"resolver": "provider"},
        )
        return None if edge is None else CallResolution(source_node_id, source.scope_id, target_node_id, confidence, "provider")
    if source.table == "TypeAnnotation":
        edge = _edge_if_allowed(
            graph,
            "References",
            source_node_id,
            target_node_id,
            "provider_type_reference",
            confidence=confidence,
            metadata={"resolver": "provider"},
        )
        return None if edge is None else TypeResolution(source_node_id, target_node_id, confidence, "provider")
    return None


def _edge_if_allowed(
    graph: CodeGraph,
    edge_type: str,
    source_id: str,
    target_id: str,
    kind: str,
    *,
    confidence: float,
    metadata: dict[str, Any],
) -> GraphEdge | None:
    source = graph.nodes.get(source_id)
    target = graph.nodes.get(target_id)
    if source is None or target is None:
        return None
    spec = get_relation_type(edge_type)
    if source.table not in spec.source_types or target.table not in spec.target_types:
        return None
    edge = GraphEdge(
        id=_stable_id("edge", f"{edge_type}|{source_id}|{target_id}|{kind}"),
        type=edge_type,
        source_id=source_id,
        target_id=target_id,
        kind=kind,
        confidence=confidence,
        metadata={"canonical_key": f"{edge_type}|{source_id}|{target_id}|{kind}", **metadata},
    )
    return graph.add_edge(edge)


def _graph_tuple(graphs: CodeGraph | Iterable[CodeGraph]) -> tuple[CodeGraph, ...]:
    if isinstance(graphs, CodeGraph):
        return (graphs,)
    return tuple(graphs)


def _stable_id(prefix: str, key: str) -> str:
    digest = hashlib.sha1(key.encode("utf-8")).hexdigest()[:20]
    return f"{prefix}:{digest}"
