from __future__ import annotations

import hashlib
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any

from codebase_graph.core import CodeGraph, GraphEdge
from codebase_graph.ontology import get_relation_type

from .providers import ProviderResult
from .symbol_table import ProjectSymbolTable, build_project_symbol_table, candidate_symbol_keys

REFERENCE_TABLES = {"Reference", "ImportDeclaration", "CallExpression", "TypeAnnotation", "Decorator"}


@dataclass(frozen=True, slots=True)
class ReferenceCandidate:
    """Identifier, import, include, call, or type reference that may resolve to a symbol."""

    reference_node_id: str
    name: str
    scope_id: str
    language: str
    source_path: str


@dataclass(frozen=True, slots=True)
class ResolutionCandidate:
    """Possible target symbol for a reference with a score and rationale."""

    target_node_id: str
    score: float
    source: str
    rationale: str


@dataclass(frozen=True, slots=True)
class ResolutionEvidence:
    """Stored evidence explaining why a reference was or was not resolved."""

    evidence_id: str
    source: str
    confidence: float
    diagnostics: tuple[str, ...] = ()
    provider: str = ""
    metadata: Mapping[str, Any] = field(default_factory=dict)


def resolve_project_references(
    graphs: CodeGraph | Iterable[CodeGraph],
    *,
    symbol_table: ProjectSymbolTable | None = None,
    provider_results: Sequence[ProviderResult] = (),
) -> tuple[ResolutionEvidence, ...]:
    """Collect references, score candidates, merge evidence, and emit resolution edges."""
    graph_list = _graph_tuple(graphs)
    table = symbol_table or build_project_symbol_table(graph_list)
    evidences: list[ResolutionEvidence] = []
    for graph in graph_list:
        for candidate in collect_reference_candidates(graph):
            local = resolve_local_reference(candidate, table)
            provider_evidence = _provider_evidence(candidate, provider_results, table)
            decision = merge_resolution_evidence(candidate, local, provider_evidence)
            if decision is None:
                evidences.append(
                    ResolutionEvidence(
                        evidence_id=_stable_id("evidence", f"unresolved:{candidate.reference_node_id}"),
                        source="local",
                        confidence=0.0,
                        diagnostics=(f"Unresolved reference: {candidate.name}",),
                    )
                )
                continue
            edge = emit_resolves_to_edge(graph, candidate, decision)
            if edge is not None:
                evidences.append(
                    ResolutionEvidence(
                        evidence_id=_stable_id("evidence", edge.id),
                        source=decision.source,
                        confidence=decision.score,
                        provider=decision.source if decision.source != "symbol_table" else "",
                        metadata={"edge_id": edge.id, "target_node_id": decision.target_node_id},
                    )
                )
    return tuple(evidences)


def collect_reference_candidates(graph: CodeGraph) -> tuple[ReferenceCandidate, ...]:
    """Collect identifier, import, include, call, and type references from graph nodes."""
    candidates: list[ReferenceCandidate] = []
    for node in graph.nodes.values():
        if node.table not in REFERENCE_TABLES:
            continue
        name = str(node.metadata.get("imported_name") or node.label or "").strip()
        if not name:
            continue
        candidates.append(
            ReferenceCandidate(
                reference_node_id=node.id,
                name=name,
                scope_id=node.scope_id,
                language=node.language,
                source_path=node.path,
            )
        )
    return tuple(sorted(candidates, key=lambda item: (item.source_path, item.reference_node_id)))


def score_resolution_candidate(
    reference: ReferenceCandidate,
    target_node_id: str,
    *,
    source: str = "symbol_table",
    same_scope: bool = False,
    same_language: bool = False,
) -> ResolutionCandidate:
    """Score a target using local scope, language, and provider evidence."""
    score = 0.72
    if same_scope:
        score += 0.13
    if same_language:
        score += 0.05
    if source != "symbol_table":
        score = max(score, 0.95)
    return ResolutionCandidate(
        target_node_id=target_node_id,
        score=min(score, 1.0),
        source=source,
        rationale=f"{source} matched {reference.name}",
    )


def resolve_local_reference(
    reference: ReferenceCandidate,
    symbol_table: ProjectSymbolTable,
) -> ResolutionCandidate | None:
    """Resolve references using symbol tables and imports without external providers."""
    for key in candidate_symbol_keys(reference.name):
        symbols = symbol_table.by_name.get(key, ())
        if not symbols:
            continue
        ranked = sorted(
            symbols,
            key=lambda symbol: (
                symbol.scope_id != reference.scope_id,
                symbol.language != reference.language,
                symbol.visibility not in {"local", "public", "exported"},
                symbol.qualified_name,
                symbol.node_id,
            ),
        )
        symbol = ranked[0]
        return score_resolution_candidate(
            reference,
            symbol.node_id,
            same_scope=symbol.scope_id == reference.scope_id,
            same_language=symbol.language == reference.language,
        )
    return None


def merge_resolution_evidence(
    reference: ReferenceCandidate,
    local_candidate: ResolutionCandidate | None,
    provider_candidate: ResolutionCandidate | None = None,
) -> ResolutionCandidate | None:
    """Merge local and provider-backed evidence into one deterministic decision."""
    del reference
    if local_candidate is None:
        return provider_candidate
    if provider_candidate is None:
        return local_candidate
    if provider_candidate.score > local_candidate.score:
        return provider_candidate
    return local_candidate


def emit_resolves_to_edge(
    graph: CodeGraph,
    reference: ReferenceCandidate,
    candidate: ResolutionCandidate,
) -> GraphEdge | None:
    """Emit additive resolves-to and references relationships with confidence metadata."""
    source = graph.nodes.get(reference.reference_node_id)
    target = graph.nodes.get(candidate.target_node_id)
    if source is None or target is None or source.id == target.id:
        return None
    metadata = {
        "resolver": "semantic",
        "resolution_source": candidate.source,
        "rationale": candidate.rationale,
        "label": reference.name,
    }
    primary = _edge_if_allowed(
        graph,
        "ResolvesTo",
        source.id,
        target.id,
        "semantic_resolution",
        confidence=candidate.score,
        metadata=metadata,
    )
    _edge_if_allowed(
        graph,
        "References",
        source.id,
        target.id,
        "semantic_reference",
        confidence=min(candidate.score, 0.9),
        metadata=metadata,
    )
    return primary


def _provider_evidence(
    reference: ReferenceCandidate,
    provider_results: Sequence[ProviderResult],
    symbol_table: ProjectSymbolTable,
) -> ResolutionCandidate | None:
    for result in provider_results:
        if not result.target_symbol or result.diagnostics:
            continue
        if result.target_symbol == reference.name:
            target_node_id = str(result.metadata.get("target_node_id") or "")
            if not target_node_id:
                for key in candidate_symbol_keys(result.target_symbol):
                    symbols = symbol_table.by_name.get(key, ())
                    if symbols:
                        target_node_id = symbols[0].node_id
                        break
            if not target_node_id:
                continue
            return ResolutionCandidate(
                target_node_id=target_node_id,
                score=result.confidence,
                source=result.provider,
                rationale=f"{result.provider} resolved {reference.name}",
            )
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
