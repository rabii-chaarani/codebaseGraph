from __future__ import annotations

from collections.abc import Iterator

import pytest

from codebase_graph.extract import CaptureRecord, CaptureTableRegistry, GraphBuilder, ParseBundle
from codebase_graph.ingest import TreeSitterPythonParser
from codebase_graph.ontology import PARSER_NODE_MAPPINGS


def _walk_parser_nodes(node: object) -> Iterator[dict[str, object]]:
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from _walk_parser_nodes(value)
    elif isinstance(node, list):
        for item in node:
            yield from _walk_parser_nodes(item)


def test_tree_sitter_parser_preserves_attribute_object_coordinates() -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    parser = TreeSitterPythonParser()

    tree = parser.parse_source('registry.register_exact("x", "Y")\n')

    attribute = next(
        node
        for node in _walk_parser_nodes(tree)
        if node.get("type") == "attribute" and node.get("text") == "registry.register_exact"
    )
    value = attribute["value"]
    assert isinstance(value, dict)
    assert value["type"] == "identifier"
    assert value["id"] == "registry"
    assert value["line_start"] == 1
    assert value["byte_start"] == 0
    assert value["byte_end"] == len("registry")
    assert attribute["attr"] == "register_exact"


def test_tree_sitter_parser_preserves_parameter_annotation_coordinates() -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    parser = TreeSitterPythonParser()

    tree = parser.parse_source(
        "def f(capture_name: str, table: str | CaptureTableResolver) -> None:\n"
        "    pass\n"
    )

    function = next(node for node in _walk_parser_nodes(tree) if node.get("type") == "function_definition")
    args = function["args"]
    assert isinstance(args, dict)
    parameters = args["args"]
    assert isinstance(parameters, list)
    annotations = [parameter["annotation"] for parameter in parameters]
    assert {annotation["text"] for annotation in annotations} == {"str", "str | CaptureTableResolver"}
    assert all(annotation["line_start"] == 1 for annotation in annotations)
    assert all(isinstance(annotation["byte_start"], int) for annotation in annotations)
    assert all(isinstance(annotation["byte_end"], int) for annotation in annotations)


def test_graph_builder_keeps_registry_references_spanned_and_evidenced() -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    parser = TreeSitterPythonParser()
    source = (
        "class CaptureTableRegistry:\n"
        "    def register_exact(\n"
        "        self,\n"
        "        capture_name: str,\n"
        "        table: str | CaptureTableResolver,\n"
        "    ) -> None:\n"
        "        pass\n"
        "\n"
        "def default_capture_table_registry() -> CaptureTableRegistry:\n"
        "    registry = CaptureTableRegistry()\n"
        '    registry.register_exact("x", "Y")\n'
        "    return registry\n"
    )
    tree = parser.parse_source(source)

    graph = GraphBuilder(default_language="python").build(tree, source_path="sample.py")

    registry_references = [node for node in graph.nodes_by_type("Reference") if node.label == "registry"]
    assert registry_references
    assert all(node.line_start is not None for node in registry_references)
    assert all(node.byte_start is not None for node in registry_references)

    type_annotations = graph.nodes_by_type("TypeAnnotation")
    assert type_annotations
    assert all(node.line_start is not None for node in type_annotations)
    assert all(node.byte_start is not None for node in type_annotations)

    derived_edges = graph.edges_by_type("DerivedFrom")
    checked_nodes = [*registry_references, *type_annotations]
    for semantic in checked_nodes:
        syntax_ids = [edge.target_id for edge in derived_edges if edge.source_id == semantic.id]
        assert len(syntax_ids) == 1
        syntax = graph.nodes[syntax_ids[0]]
        assert syntax.tree_sitter_node_type == semantic.tree_sitter_node_type
        assert syntax.line_start == semantic.line_start
        assert syntax.byte_start == semantic.byte_start


def test_graph_builder_maps_python_ast_shaped_tree_to_ontology() -> None:
    parse_tree = {
        "type": "Module",
        "body": [
            {
                "type": "ImportFrom",
                "module": "dataclasses",
                "names": [{"type": "alias", "name": "dataclass"}],
                "line_start": 1,
            },
            {
                "type": "ClassDef",
                "name": "WikiConfig",
                "line_start": 5,
                "decorator_list": [
                    {
                        "type": "Call",
                        "func": {"type": "Name", "id": "dataclass"},
                        "keywords": [{"type": "keyword", "arg": "slots", "value": {"type": "Constant", "value": True}}],
                    }
                ],
                "body": [
                    {
                        "type": "AnnAssign",
                        "target": {"type": "Name", "id": "vault_dir"},
                        "annotation": {"type": "Name", "id": "Path"},
                        "value": {
                            "type": "Call",
                            "func": {"type": "Name", "id": "Path"},
                            "args": [{"type": "Constant", "value": "wiki"}],
                        },
                        "line_start": 7,
                    },
                    {
                        "type": "FunctionDef",
                        "name": "raw_dir",
                        "args": {"type": "arguments", "args": [{"type": "arg", "arg": "self"}]},
                        "returns": {"type": "Name", "id": "Path"},
                        "decorator_list": [{"type": "Name", "id": "property"}],
                        "body": [
                            {
                                "type": "Return",
                                "value": {
                                    "type": "Attribute",
                                    "value": {"type": "Name", "id": "self"},
                                    "attr": "vault_dir",
                                },
                            }
                        ],
                        "line_start": 10,
                    },
                ],
            },
            {
                "type": "Assign",
                "targets": [{"type": "Name", "id": "PAGE_KINDS"}],
                "value": {
                    "type": "Tuple",
                    "elts": [
                        {"type": "Constant", "value": "sources"},
                        {"type": "Constant", "value": "entities"},
                    ],
                },
                "line_start": 15,
            },
        ],
    }

    graph = GraphBuilder(default_language="python").build(parse_tree, source_path="wiki_config.py")

    labels_by_type = {
        table: {node.label for node in graph.nodes_by_type(table)}
        for table in ("ImportDeclaration", "Class", "Method", "ClassAttribute", "Constant", "Decorator")
    }
    assert "dataclasses.dataclass" in labels_by_type["ImportDeclaration"]
    assert "WikiConfig" in labels_by_type["Class"]
    assert "raw_dir" in labels_by_type["Method"]
    assert "vault_dir" in labels_by_type["ClassAttribute"]
    assert "PAGE_KINDS" in labels_by_type["Constant"]
    assert {"dataclass", "property"} <= labels_by_type["Decorator"]
    assert graph.edges_by_type("DerivedFrom")
    assert graph.edges_by_type("HasReturnType")
    assert graph.edges_by_type("HasTypeAnnotation")


def test_graph_builder_uses_capture_names_as_primary_semantic_signal() -> None:
    bundle = ParseBundle(
        language="python",
        path="api.py",
        captures=(
            CaptureRecord(
                "definition.function",
                {"type": "identifier", "text": "handler", "start_byte": 10, "end_byte": 17},
            ),
            CaptureRecord(
                "reference.call",
                {"type": "identifier", "text": "json_response", "start_byte": 24, "end_byte": 37},
            ),
            CaptureRecord(
                "doc.string",
                {"type": "string", "text": "Handle the API request.", "start_byte": 40, "end_byte": 65},
            ),
        ),
    )

    result = GraphBuilder(repository_label="sample").build_file_graph(bundle)
    graph = result.graph

    assert {node.label for node in graph.nodes_by_type("Function")} == {"handler"}
    assert {node.label for node in graph.nodes_by_type("CallExpression")} == {"json_response"}
    assert {node.label for node in graph.nodes_by_type("DocumentationChunk")} == {"Handle the API request."}
    assert not result.diagnostics
    assert not result.unresolved


def test_graph_builder_accepts_registered_capture_table_mapping() -> None:
    registry = CaptureTableRegistry()
    registry.register_exact("custom.component", "Component")
    bundle = ParseBundle(
        language="custom",
        path="component.custom",
        captures=(
            CaptureRecord(
                "custom.component",
                {"type": "identifier", "name": "RegisteredWidget", "line_start": 1},
            ),
        ),
    )

    result = GraphBuilder(capture_table_registry=registry).build_file_graph(bundle)

    assert {node.label for node in result.graph.nodes_by_type("Component")} == {"RegisteredWidget"}


def test_graph_builder_routes_local_imports_through_containing_scope() -> None:
    parse_tree = {
        "type": "Module",
        "body": [
            {
                "type": "ClassDef",
                "name": "Loader",
                "body": [
                    {
                        "type": "FunctionDef",
                        "name": "load",
                        "body": [
                            {
                                "type": "ImportFrom",
                                "module": "pathlib",
                                "names": [{"type": "alias", "name": "Path"}],
                                "line_start": 3,
                            },
                        ],
                    },
                ],
            },
        ],
    }

    graph = GraphBuilder(default_language="python").build(parse_tree, source_path="loader.py")

    import_edge = next(
        edge
        for edge in graph.edges_by_type("Imports")
        if graph.nodes[edge.target_id].label == "pathlib.Path"
    )
    assert graph.nodes[import_edge.source_id].table == "Scope"
    assert graph.nodes[import_edge.target_id].table == "ImportDeclaration"


def test_graph_builder_emits_relation_families_advertised_by_parser_mappings() -> None:
    parse_tree = {
        "type": "Module",
        "body": [
            {
                "type": "ImportFrom",
                "module": "fastapi",
                "names": [{"type": "alias", "name": "APIRouter"}],
            },
            {"type": "FunctionDef", "name": "helper", "body": []},
            {"type": "FunctionDef", "name": "auth_required", "body": []},
            {
                "type": "FunctionDef",
                "name": "list_users",
                "args": {
                    "type": "arguments",
                    "args": [
                        {
                            "type": "arg",
                            "arg": "user_id",
                            "annotation": {"type": "Name", "id": "int"},
                        }
                    ],
                },
                "returns": {"type": "Name", "id": "Response"},
                "decorator_list": [{"type": "Name", "id": "auth_required"}],
                "body": [
                    {"type": "call", "capture_name": "route", "text": "/users", "handler": "list_users"},
                    {"type": "Call", "func": {"type": "Name", "id": "helper"}},
                    {
                        "type": "string",
                        "capture_name": "query.sql",
                        "text": "SELECT * FROM users",
                        "table": "users",
                    },
                    {"type": "Name", "capture_name": "secret.env", "id": "DATABASE_URL"},
                    {
                        "type": "Assign",
                        "targets": [{"type": "Name", "id": "CACHE"}],
                        "value": {"type": "Call", "func": {"type": "Name", "id": "helper"}},
                    },
                    {"type": "Name", "capture_name": "reference.identifier", "id": "helper"},
                    {"type": "raise_statement", "capture_name": "raises", "name": "ValueError"},
                    {"type": "except_clause", "capture_name": "handles", "name": "ValueError"},
                    {"type": "docstring", "capture_name": "doc.string", "text": "List users."},
                ],
            },
            {"type": "component_declaration", "capture_name": "component", "name": "UserService"},
            {"type": "export_statement", "name": "list_users"},
        ],
    }

    graph = GraphBuilder(default_language="python").build(parse_tree, source_path="api.py")

    mapped_relations = {relation for mapping in PARSER_NODE_MAPPINGS for relation in mapping.relation_types}
    emitted_relations = set(graph.summary()["edge_counts"])
    assert mapped_relations <= emitted_relations
