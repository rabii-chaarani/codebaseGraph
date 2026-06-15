from __future__ import annotations

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode
from codebase_graph.semantic import (
    EnrichmentDiagnostic,
    ResolutionEvidence,
    attach_confidence_metadata,
    emit_semantic_evidence_edges,
    persist_semantic_edges,
    report_enrichment_capabilities,
    write_resolution_evidence,
)


def test_report_enrichment_capabilities_serializes_diagnostics() -> None:
    report = report_enrichment_capabilities(
        syntax_graph=True,
        build_context=False,
        symbol_table=True,
        local_resolution=True,
        provider_resolution=False,
        diagnostics=(EnrichmentDiagnostic("missing_context", "No build metadata", "warning"),),
    )

    assert report.as_dict() == {
        "syntax_graph": True,
        "build_context": False,
        "symbol_table": True,
        "local_resolution": True,
        "provider_resolution": False,
        "diagnostics": ["No build metadata"],
    }


def test_write_resolution_evidence_persists_metadata_without_graph_shape_changes() -> None:
    graph = CodeGraph()

    write_resolution_evidence(
        graph,
        (
            ResolutionEvidence(
                "evidence:1",
                "symbol_table",
                0.85,
                metadata={"edge_id": "edge:1"},
            ),
        ),
    )

    assert graph.metadata["semantic_resolution_evidence"][0]["confidence"] == 0.85
    assert graph.metadata["semantic_resolution_evidence"][0]["metadata"] == {"edge_id": "edge:1"}


def test_persist_semantic_edges_and_confidence_metadata_are_additive() -> None:
    graph = CodeGraph()
    edge = GraphEdge("edge:semantic", "References", "source", "target", metadata={"resolver": "semantic"})

    attach_confidence_metadata(edge, confidence=0.77, source="test")
    persisted = persist_semantic_edges(graph, (edge,))

    assert persisted == (edge,)
    assert graph.edges["edge:semantic"].confidence == 0.77
    assert graph.edges["edge:semantic"].metadata["confidence_source"] == "test"


def test_emit_semantic_evidence_edges_links_valid_evidence_nodes() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("call:helper", "CallExpression", "helper"))
    graph.add_node(GraphNode("function:helper", "Function", "helper"))
    graph.add_node(GraphNode("syntax:call", "SyntaxCapture", "helper()"))
    graph.add_edge(
        GraphEdge(
            "edge:resolve",
            "ResolvesTo",
            "call:helper",
            "function:helper",
            kind="semantic_resolution",
            confidence=0.91,
        )
    )

    links = emit_semantic_evidence_edges(
        graph,
        (
            ResolutionEvidence(
                "evidence:resolve",
                "symbol_table",
                0.91,
                metadata={"edge_id": "edge:resolve", "evidence_node_id": "syntax:call"},
            ),
        ),
    )

    assert len(links) == 1
    assert links[0].semantic_relation_id == "edge:resolve"
    assert links[0].evidence_node_id == "syntax:call"
    assert any(
        edge.type == "EvidencedBy"
        and edge.source_id == "call:helper"
        and edge.target_id == "syntax:call"
        and edge.kind == "semantic_evidence"
        for edge in graph.edges.values()
    )


def test_emit_semantic_evidence_edges_records_metadata_fallback_without_legal_endpoint() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("ref:helper", "Reference", "helper"))
    graph.add_node(GraphNode("function:helper", "Function", "helper"))
    graph.add_edge(
        GraphEdge(
            "edge:resolve",
            "ResolvesTo",
            "ref:helper",
            "function:helper",
            kind="semantic_resolution",
            confidence=0.91,
        )
    )

    links = emit_semantic_evidence_edges(
        graph,
        (
            ResolutionEvidence(
                "evidence:resolve",
                "symbol_table",
                0.91,
                metadata={"edge_id": "edge:resolve"},
            ),
        ),
    )

    assert links == ()
    assert graph.metadata["semantic_evidence_fallback"] == [
        {
            "semantic_relation_id": "edge:resolve",
            "source_node_id": "ref:helper",
            "evidence_id": "evidence:resolve",
            "metadata": {"edge_id": "edge:resolve"},
        }
    ]
