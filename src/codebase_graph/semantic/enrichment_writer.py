from __future__ import annotations

from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from typing import Any

from codebase_graph.core import CodeGraph, GraphEdge

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
        graph.metadata["semantic_enrichment"] = report.as_dict()
        graph.metadata["semantic_build_context"] = {
            "ecosystem": context.ecosystem,
            "target": target.as_dict() if target is not None else None,
        }
        graph.metadata["semantic_relations"] = {
            "resolution_evidence": len(evidence),
            "call_type_relations": len(relations),
        }
        write_resolution_evidence(graph, evidence)
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
