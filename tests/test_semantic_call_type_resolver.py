from __future__ import annotations

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode
from codebase_graph.semantic import (
    enrich_call_and_type_relations,
    resolve_method_receiver,
    resolve_provider_backed_relation,
)


def test_enrich_call_and_type_relations_promotes_resolved_calls_and_types() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("call:helper", "CallExpression", "helper", scope_id="function:main"))
    graph.add_node(GraphNode("function:helper", "Function", "helper"))
    graph.add_node(GraphNode("parameter:user", "Parameter", "user"))
    graph.add_node(GraphNode("type:User", "TypeAnnotation", "User"))
    graph.add_node(GraphNode("class:User", "Class", "User"))
    graph.add_edge(
        GraphEdge("edge:parameter-type", "HasTypeAnnotation", "parameter:user", "type:User", kind="syntax_type")
    )
    graph.add_edge(GraphEdge("edge:call-resolves", "ResolvesTo", "call:helper", "function:helper", confidence=0.91))
    graph.add_edge(GraphEdge("edge:type-resolves", "ResolvesTo", "type:User", "class:User", confidence=0.82))

    resolutions = enrich_call_and_type_relations(graph)

    assert {type(resolution).__name__ for resolution in resolutions} == {"CallResolution", "TypeResolution"}
    assert any(edge.type == "Calls" and edge.target_id == "function:helper" for edge in graph.edges.values())
    assert any(edge.type == "References" and edge.target_id == "class:User" for edge in graph.edges.values())
    assert any(
        edge.type == "HasTypeAnnotation"
        and edge.source_id == "parameter:user"
        and edge.target_id == "type:User"
        and edge.kind == "semantic_type_annotation"
        for edge in graph.edges.values()
    )


def test_type_resolution_without_typed_owner_keeps_reference_fallback_only() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("type:User", "TypeAnnotation", "User"))
    graph.add_node(GraphNode("class:User", "Class", "User"))
    graph.add_edge(GraphEdge("edge:type-resolves", "ResolvesTo", "type:User", "class:User", confidence=0.82))

    resolutions = enrich_call_and_type_relations(graph)

    assert {type(resolution).__name__ for resolution in resolutions} == {"TypeResolution"}
    assert any(edge.kind == "semantic_type_reference" for edge in graph.edges.values())
    assert not any(edge.kind == "semantic_type_annotation" for edge in graph.edges.values())


def test_resolve_method_receiver_adds_receiver_call_edge() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("call:name", "CallExpression", "u.name", scope_id="function:main"))
    graph.add_node(GraphNode("method:name", "Method", "name", qualified_name="User.name"))

    resolution = resolve_method_receiver(graph, "call:name")

    assert resolution is not None
    assert resolution.target_node_id == "method:name"
    assert any(edge.kind == "semantic_receiver_call" for edge in graph.edges.values())


def test_resolve_provider_backed_relation_adds_type_reference() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("parameter:user", "Parameter", "user"))
    graph.add_node(GraphNode("type:User", "TypeAnnotation", "User"))
    graph.add_node(GraphNode("class:User", "Class", "User"))
    graph.add_edge(
        GraphEdge("edge:parameter-type", "HasTypeAnnotation", "parameter:user", "type:User", kind="syntax_type")
    )

    resolution = resolve_provider_backed_relation(graph, "type:User", "class:User", confidence=0.97)

    assert resolution is not None
    assert resolution.confidence == 0.97
    assert any(edge.kind == "provider_type_reference" for edge in graph.edges.values())
    assert any(edge.kind == "semantic_type_annotation" for edge in graph.edges.values())
