from __future__ import annotations

import hashlib
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from typing import Any

from codebase_graph.core import CodeGraph, GraphEdge
from codebase_graph.ontology import get_relation_type

from .build_context import BuildContext, collect_project_build_context, map_source_to_build_target
from .call_type_resolver import enrich_call_and_type_relations
from .providers import ProviderMode, ProviderResult, discover_semantic_providers
from .reference_resolver import ResolutionEvidence, resolve_project_references
from .symbol_table import build_project_symbol_table


@dataclass(frozen=True, slots=True)
class SemanticCapabilityReport:
    """Per-run and per-language report describing semantic enrichment depth."""

    syntax_graph: bool
    build_context: bool
    symbol_table: bool
    local_resolution: bool
    provider_resolution: bool
    diagnostics: tuple[str, ...] = ()

    def as_dict(self) -> dict[str, Any]:
        """Serialize capability data for graph metadata."""
        return {
            "syntax_graph": self.syntax_graph,
            "build_context": self.build_context,
            "symbol_table": self.symbol_table,
            "local_resolution": self.local_resolution,
            "provider_resolution": self.provider_resolution,
            "diagnostics": list(self.diagnostics),
        }


@dataclass(frozen=True, slots=True)
class EnrichmentDiagnostic:
    """Non-fatal semantic enrichment warning or degraded capability record."""

    code: str
    message: str
    severity: str
    source_path: str = ""
    provider: str = ""

    def as_dict(self) -> dict[str, str]:
        """Serialize diagnostic data for graph metadata."""
        return {
            "code": self.code,
            "message": self.message,
            "severity": self.severity,
            "source_path": self.source_path,
            "provider": self.provider,
        }


@dataclass(frozen=True, slots=True)
class SemanticEvidenceLink:
    """First-class semantic evidence link emitted through EvidencedBy."""

    semantic_relation_id: str
    evidence_node_id: str
    evidence_kind: str
    confidence: float
    metadata_fallback: bool = False

    def as_dict(self) -> dict[str, Any]:
        """Serialize evidence-link data for graph metadata."""
        return {
            "semantic_relation_id": self.semantic_relation_id,
            "evidence_node_id": self.evidence_node_id,
            "evidence_kind": self.evidence_kind,
            "confidence": self.confidence,
            "metadata_fallback": self.metadata_fallback,
        }


def persist_semantic_enrichment(
    graphs: CodeGraph | Iterable[CodeGraph],
    *,
    source_root: str,
    build_context: BuildContext | None = None,
    provider_mode: ProviderMode = "local_only",
    provider_results: Sequence[ProviderResult] = (),
) -> SemanticCapabilityReport:
    """Persist additive semantic edges, evidence records, diagnostics, and reports."""
    graph_list = _graph_tuple(graphs)
    context = build_context or collect_project_build_context(
        source_root,
        source_paths=tuple(_source_paths(graph_list)),
    )
    symbol_table = build_project_symbol_table(graph_list)
    providers = discover_semantic_providers(context)
    evidence = resolve_project_references(graph_list, symbol_table=symbol_table, provider_results=provider_results)
    relations = enrich_call_and_type_relations(graph_list, provider_results=provider_results)
    diagnostics = _diagnostics(context, evidence)
    provider_enabled = provider_mode != "local_only" and any(provider.available for provider in providers)
    report = report_enrichment_capabilities(
        syntax_graph=bool(graph_list),
        build_context=bool(context.targets),
        symbol_table=bool(symbol_table.symbols),
        local_resolution=bool(evidence),
        provider_resolution=provider_enabled,
        diagnostics=diagnostics,
    )
    for graph in graph_list:
        target = map_source_to_build_target(context, str(graph.metadata.get("source_path") or ""))
        evidence_links = persist_first_class_semantic_evidence(graph, evidence)
        graph.metadata["semantic_enrichment"] = report.as_dict()
        graph.metadata["semantic_build_context"] = {
            "ecosystem": context.ecosystem,
            "target": target.as_dict() if target is not None else None,
        }
        graph.metadata["semantic_relations"] = {
            "resolution_evidence": len(evidence),
            "call_type_relations": len(relations),
            "evidence_links": len(evidence_links),
        }
        write_resolution_evidence(graph, evidence)
        if evidence_links:
            graph.metadata["semantic_evidence_links"] = [link.as_dict() for link in evidence_links]
    return report


def attach_confidence_metadata(edge: GraphEdge, *, confidence: float, source: str) -> GraphEdge:
    """Attach confidence, source, and evidence identifiers to semantic edges."""
    edge.confidence = confidence
    edge.metadata["confidence_source"] = source
    return edge


def write_resolution_evidence(graph: CodeGraph, evidence: Sequence[ResolutionEvidence]) -> None:
    """Persist resolution evidence and diagnostics in graph metadata."""
    graph.metadata["semantic_resolution_evidence"] = [
        {
            "evidence_id": item.evidence_id,
            "source": item.source,
            "confidence": item.confidence,
            "diagnostics": list(item.diagnostics),
            "provider": item.provider,
            "metadata": dict(item.metadata),
        }
        for item in evidence
    ]


def report_enrichment_capabilities(
    *,
    syntax_graph: bool,
    build_context: bool,
    symbol_table: bool,
    local_resolution: bool,
    provider_resolution: bool,
    diagnostics: Sequence[EnrichmentDiagnostic | str] = (),
) -> SemanticCapabilityReport:
    """Write capability summaries per language and materialization run."""
    return SemanticCapabilityReport(
        syntax_graph=syntax_graph,
        build_context=build_context,
        symbol_table=symbol_table,
        local_resolution=local_resolution,
        provider_resolution=provider_resolution,
        diagnostics=tuple(
            item.message if isinstance(item, EnrichmentDiagnostic) else str(item)
            for item in diagnostics
        ),
    )


def persist_semantic_edges(graph: CodeGraph, edges: Sequence[GraphEdge]) -> tuple[GraphEdge, ...]:
    """Persist resolved reference, call, and type relationships without deleting graph rows."""
    return tuple(graph.add_edge(edge) for edge in edges)


def persist_first_class_semantic_evidence(
    graph: CodeGraph,
    evidence: Sequence[ResolutionEvidence],
) -> tuple[SemanticEvidenceLink, ...]:
    """Persist first-class semantic evidence where legal graph endpoints exist."""
    return emit_semantic_evidence_edges(graph, evidence)


def emit_semantic_evidence_edges(
    graph: CodeGraph,
    evidence: Sequence[ResolutionEvidence],
) -> tuple[SemanticEvidenceLink, ...]:
    """Write EvidencedBy edges for semantic resolutions with valid evidence nodes."""
    links: list[SemanticEvidenceLink] = []
    fallbacks: list[dict[str, Any]] = []
    for item in evidence:
        semantic_relation_id = str(item.metadata.get("edge_id") or "")
        semantic_edge = graph.edges.get(semantic_relation_id)
        if semantic_edge is None:
            continue
        evidence_node_ids = _semantic_evidence_node_ids(graph, semantic_edge.source_id, item)
        if not evidence_node_ids:
            fallbacks.append(
                {
                    "semantic_relation_id": semantic_relation_id,
                    "source_node_id": semantic_edge.source_id,
                    "evidence_id": item.evidence_id,
                    "metadata": dict(item.metadata),
                }
            )
            continue
        for evidence_node_id in evidence_node_ids:
            evidence_node = graph.nodes[evidence_node_id]
            edge = _evidence_edge_if_allowed(
                graph,
                semantic_edge.source_id,
                evidence_node_id,
                confidence=item.confidence,
                metadata={
                    "resolver": "semantic",
                    "semantic_relation_id": semantic_relation_id,
                    "evidence_id": item.evidence_id,
                    "source": item.source,
                    "provider": item.provider,
                },
            )
            if edge is None:
                continue
            links.append(
                SemanticEvidenceLink(
                    semantic_relation_id=semantic_relation_id,
                    evidence_node_id=evidence_node_id,
                    evidence_kind=evidence_node.table,
                    confidence=item.confidence,
                )
            )
    if fallbacks:
        graph.metadata["semantic_evidence_fallback"] = fallbacks
    return tuple(links)


def _diagnostics(
    context: BuildContext,
    evidence: Sequence[ResolutionEvidence],
) -> tuple[EnrichmentDiagnostic, ...]:
    diagnostics = [
        EnrichmentDiagnostic("build_context", message, "warning")
        for message in context.diagnostics
    ]
    diagnostics.extend(
        EnrichmentDiagnostic("unresolved_reference", message, "info")
        for item in evidence
        for message in item.diagnostics
    )
    return tuple(diagnostics)


def _semantic_evidence_node_ids(
    graph: CodeGraph,
    source_node_id: str,
    evidence: ResolutionEvidence,
) -> tuple[str, ...]:
    node_ids: list[str] = []
    explicit_id = str(evidence.metadata.get("evidence_node_id") or "")
    if _is_valid_evidence_target(graph, explicit_id):
        node_ids.append(explicit_id)
    for edge in graph.edges_by_type("DerivedFrom"):
        if edge.source_id == source_node_id and _is_valid_evidence_target(graph, edge.target_id):
            node_ids.append(edge.target_id)
    source_node = graph.nodes.get(source_node_id)
    if source_node is not None and source_node.path:
        for node in graph.nodes_by_type("File"):
            if node.path == source_node.path and _is_valid_evidence_target(graph, node.id):
                node_ids.append(node.id)
    return tuple(dict.fromkeys(node_ids))


def _is_valid_evidence_target(graph: CodeGraph, node_id: str) -> bool:
    node = graph.nodes.get(node_id)
    return node is not None and node.table in {"SyntaxCapture", "File", "DocumentationChunk"}


def _evidence_edge_if_allowed(
    graph: CodeGraph,
    source_id: str,
    target_id: str,
    *,
    confidence: float,
    metadata: dict[str, Any],
) -> GraphEdge | None:
    source = graph.nodes.get(source_id)
    target = graph.nodes.get(target_id)
    if source is None or target is None:
        return None
    spec = get_relation_type("EvidencedBy")
    if source.table not in spec.source_types or target.table not in spec.target_types:
        return None
    edge = GraphEdge(
        id=f"edge:semantic-evidence:{_stable_key(source_id, target_id, str(metadata['semantic_relation_id']))}",
        type="EvidencedBy",
        source_id=source_id,
        target_id=target_id,
        kind="semantic_evidence",
        confidence=confidence,
        metadata={
            "canonical_key": f"EvidencedBy|{source_id}|{target_id}|semantic_evidence",
            **metadata,
        },
    )
    return graph.add_edge(edge)


def _stable_key(*values: str) -> str:
    return hashlib.sha1("|".join(values).encode("utf-8")).hexdigest()[:20]


def _source_paths(graphs: Sequence[CodeGraph]) -> tuple[str, ...]:
    return tuple(
        sorted(
            {
                node.path
                for graph in graphs
                for node in graph.nodes_by_type("File")
                if node.path
            }
        )
    )


def _graph_tuple(graphs: CodeGraph | Iterable[CodeGraph]) -> tuple[CodeGraph, ...]:
    if isinstance(graphs, CodeGraph):
        return (graphs,)
    return tuple(graphs)
