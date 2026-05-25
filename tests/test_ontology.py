from __future__ import annotations

import json
import re

from codebase_graph.ontology import (
    ONTOLOGY_NAME,
    PARSER_NODE_MAPPINGS,
    QUERY_HELPERS,
    RELATION_TYPES,
    get_node_type,
    get_relation_type,
    node_type_names,
    relation_type_names,
    schema_payload,
)


def test_schema_payload_is_json_serializable() -> None:
    payload = schema_payload()

    encoded = json.dumps(payload, sort_keys=True)

    assert ONTOLOGY_NAME in encoded
    assert payload["node_types"]
    assert payload["relation_types"]


def test_required_node_types_are_declared() -> None:
    names = set(node_type_names())

    assert {
        "Module",
        "ImportDeclaration",
        "ExportDeclaration",
        "Symbol",
        "Scope",
        "Class",
        "Function",
        "Method",
        "Parameter",
        "ReturnType",
        "TypeAnnotation",
        "TypeAlias",
        "Variable",
        "Constant",
        "ClassAttribute",
        "InstanceAttribute",
        "Property",
        "Decorator",
        "CallExpression",
        "Assignment",
        "Reference",
        "Literal",
        "Expression",
        "ControlFlowBlock",
        "ExceptionFlow",
        "APIEndpoint",
        "Component",
        "Route",
        "Query",
        "SecretRef",
        "Repository",
        "SourceRoot",
        "File",
        "Dependency",
        "DocumentationSource",
        "DocumentationChunk",
        "SyntaxCapture",
    } <= names


def test_declared_relation_endpoints_reference_declared_node_types() -> None:
    names = set(node_type_names())

    for relation in RELATION_TYPES:
        assert relation.source_types
        assert relation.target_types
        assert set(relation.source_types) <= names
        assert set(relation.target_types) <= names


def test_parser_node_mappings_reference_declared_nodes_and_relations() -> None:
    nodes = set(node_type_names())
    relations = set(relation_type_names())

    for mapping in PARSER_NODE_MAPPINGS:
        assert mapping.parser_node_types
        assert mapping.target_node_types
        assert set(mapping.target_node_types) <= nodes
        assert set(mapping.relation_types) <= relations


def test_example_parser_shapes_are_covered() -> None:
    covered_parser_nodes = {node for mapping in PARSER_NODE_MAPPINGS for node in mapping.parser_node_types}

    assert {
        "Module",
        "ImportFrom",
        "ClassDef",
        "FunctionDef",
        "AnnAssign",
        "Assign",
        "Call",
        "Name",
        "Attribute",
        "Constant",
    } <= covered_parser_nodes


def test_query_helpers_are_read_only() -> None:
    forbidden = re.compile(r"\b(CREATE|MERGE|DELETE|SET|DROP|LOAD|COPY)\b", re.IGNORECASE)

    for helper in QUERY_HELPERS:
        assert helper.query.lstrip().upper().startswith("MATCH ")
        assert not forbidden.search(helper.query)


def test_query_helpers_use_edge_node_relation_traversal() -> None:
    direct_relation = re.compile(r"-\[:(?!FROM_|TO_)([A-Za-z][A-Za-z0-9_]*)\]->")

    for helper in QUERY_HELPERS:
        assert not direct_relation.search(helper.query), helper.name

    helper_queries = {helper.name: helper.query for helper in QUERY_HELPERS}
    assert "[:FROM_ResolvesTo]" in helper_queries["callgraph_neighborhood"]
    assert "[:TO_ResolvesTo]" in helper_queries["callgraph_neighborhood"]
    assert "[:FROM_DependsOn]" in helper_queries["dependency_map"]
    assert "[:TO_DependsOn]" in helper_queries["dependency_map"]
    assert "[:FROM_RoutesTo]" in helper_queries["runtime_surface"]
    assert "[:TO_RoutesTo]" in helper_queries["runtime_surface"]
    assert "[:FROM_Documents]" in helper_queries["documentation_context"]
    assert "[:TO_Documents]" in helper_queries["documentation_context"]


def test_lookup_helpers_return_expected_specs() -> None:
    assert get_node_type("Class").name == "Class"
    assert get_relation_type("Calls").name == "Calls"
