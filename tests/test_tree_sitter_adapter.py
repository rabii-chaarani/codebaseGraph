from __future__ import annotations

from dataclasses import replace

import pytest

from codebase_graph.extract import GraphBuilder
from codebase_graph.ingest import (
    build_parse_bundle,
    create_tree_sitter_parser,
    normalize_syntax_node,
    resolve_language_profile,
    run_profile_queries,
)
from codebase_graph.ingest.tree_sitter_parser import ParserUnavailableError


def test_normalize_syntax_node_preserves_expected_fields() -> None:
    node = normalize_syntax_node(
        {
            "type": "function_item",
            "text": "main",
            "line_start": 1,
            "line_end": 3,
            "byte_start": 0,
            "byte_end": 12,
            "children": [{"type": "call_expression", "text": "println"}],
        }
    )

    assert node.node_type == "function_item"
    assert node.children[0].node_type == "call_expression"
    assert node.as_dict()["line_start"] == 1


def test_run_profile_queries_marks_profile_captures() -> None:
    profile = resolve_language_profile("rust")
    result = run_profile_queries(
        {
            "type": "source_file",
            "children": [
                {"type": "function_item", "text": "main", "line_start": 1},
                {"type": "call_expression", "text": "println", "line_start": 2},
            ],
        },
        profile,
    )

    assert [capture.capture for capture in result.captures] == ["definition.function", "reference.call"]
    assert result.syntax_nodes[0].children[0].capture_name == "definition.function"


def test_build_parse_bundle_feeds_graph_builder_tree() -> None:
    profile = resolve_language_profile("rust")
    result = run_profile_queries(
        {"type": "source_file", "children": [{"type": "function_item", "name": "main", "line_start": 1}]},
        profile,
    )
    bundle = build_parse_bundle(
        profile,
        result,
        source_text="fn main() {}",
        relative_path="src/main.rs",
        source_root=".",
        repository_label="repo",
        content_hash="hash",
    )

    graph = GraphBuilder().build_file_graph(bundle).graph

    assert "main" in {node.label for node in graph.nodes_by_type("Function")}


def test_create_tree_sitter_parser_reports_missing_grammar() -> None:
    profile = resolve_language_profile("fortran")
    assert profile is not None
    profile = replace(profile, grammar_package="missing_tree_sitter_fortran")

    with pytest.raises(ParserUnavailableError):
        create_tree_sitter_parser(profile)
