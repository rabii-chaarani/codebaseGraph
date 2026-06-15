from __future__ import annotations

from codebase_graph.core import CodeGraph, GraphNode
from codebase_graph.semantic import ProviderResult, build_project_symbol_table, persist_semantic_enrichment, resolve_project_references


def test_semantic_enrichment_adds_resolved_call_edges() -> None:
    graph = CodeGraph(metadata={"source_path": "main.go", "language": "go", "source_root": "."})
    graph.add_node(GraphNode("file:main", "File", "main.go", path="main.go"))
    graph.add_node(GraphNode("module:main", "Module", "main", language="go", path="main.go", qualified_name="main"))
    graph.add_node(
        GraphNode(
            "function:helper",
            "Function",
            "helper",
            language="go",
            path="main.go",
            qualified_name="main.helper",
            scope_id="module:main",
        )
    )
    graph.add_node(
        GraphNode(
            "call:helper",
            "CallExpression",
            "helper",
            language="go",
            path="main.go",
            scope_id="function:main",
        )
    )

    report = persist_semantic_enrichment((graph,), source_root=".", provider_mode="local_only")

    assert report.syntax_graph
    assert any(edge.type == "ResolvesTo" and edge.target_id == "function:helper" for edge in graph.edges.values())
    assert any(edge.type == "Calls" and edge.target_id == "function:helper" for edge in graph.edges.values())
    assert graph.metadata["semantic_enrichment"]["provider_resolution"] is False


def test_symbol_table_indexes_import_bindings_and_exports() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("import:fmt", "ImportDeclaration", "fmt", metadata={"imported_name": "fmt"}))
    graph.add_node(GraphNode("dep:fmt", "Dependency", "fmt", qualified_name="fmt"))

    table = build_project_symbol_table(graph)

    assert table.imports[0].imported_name == "fmt"
    assert table.by_name["fmt"][0].node_id == "dep:fmt"


def test_reference_resolver_merges_provider_evidence_with_symbol_table_targets() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("function:helper", "Function", "helper", qualified_name="helper", language="rust"))
    graph.add_node(GraphNode("call:helper", "CallExpression", "helper", language="rust"))

    evidence = resolve_project_references(
        graph,
        provider_results=(ProviderResult("rust_analyzer", "definition", target_symbol="helper", confidence=0.96),),
    )

    assert evidence[0].source == "rust_analyzer"
    edge = next(edge for edge in graph.edges.values() if edge.type == "ResolvesTo")
    assert edge.target_id == "function:helper"
    assert edge.confidence == 0.96
