from __future__ import annotations

import shutil
import subprocess
import sys
from dataclasses import replace
from pathlib import Path
from typing import Any

import pytest

from codebase_graph.extract import GraphBuilder
from codebase_graph.ingest import (
    build_parse_bundle,
    create_tree_sitter_parser,
    normalize_syntax_node,
    parse_profiled_source,
    resolve_language_profile,
    run_profile_queries,
)
from codebase_graph.ingest.tree_sitter_parser import ParserUnavailableError

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


@pytest.fixture(scope="session")
def native_tree_sitter_normalizer_binary() -> Path:
    if shutil.which("cargo") is None:
        pytest.skip("cargo is required for native tree-sitter normalization tests")

    subprocess.run(
        [
            "cargo",
            "build",
            "--manifest-path",
            RUST_MANIFEST.as_posix(),
            "--bin",
            NATIVE_BINARY_NAME,
            "--quiet",
        ],
        check=True,
    )
    suffix = ".exe" if sys.platform.startswith("win") else ""
    binary = REPO_ROOT / "rust" / "target" / "debug" / f"{NATIVE_BINARY_NAME}{suffix}"
    assert binary.exists()
    return binary


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


def test_run_profile_queries_applies_context_rules() -> None:
    profile = resolve_language_profile("rust")
    result = run_profile_queries(
        {
            "type": "source_file",
            "children": [
                {
                    "type": "impl_item",
                    "children": [{"type": "function_item", "text": "new", "line_start": 2}],
                }
            ],
        },
        profile,
    )

    assert [capture.capture for capture in result.captures] == ["definition.method"]
    assert result.syntax_nodes[0].children[0].children[0].capture_name == "definition.method"


def test_native_profile_queries_match_python_for_context_rules(
    monkeypatch: pytest.MonkeyPatch,
    native_tree_sitter_normalizer_binary: Path,
) -> None:
    profile = resolve_language_profile("rust")
    tree = {
        "type": "source_file",
        "children": [
            {
                "type": "impl_item",
                "children": [{"type": "function_item", "text": "new", "line_start": 2}],
            }
        ],
    }

    expected = _query_result_shape(run_profile_queries(tree, profile))
    monkeypatch.setenv("CODEBASE_GRAPH_COMPAT_TREE_SITTER_NORMALIZER", native_tree_sitter_normalizer_binary.as_posix())
    actual = _query_result_shape(run_profile_queries(tree, profile))

    assert actual == expected


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


def test_native_profiled_source_matches_python_graph(
    monkeypatch: pytest.MonkeyPatch,
    native_tree_sitter_normalizer_binary: Path,
) -> None:
    profile = resolve_language_profile("rust")
    source_text = "impl User { fn new() -> Self { User {} } }\nfn helper() { User::new(); }\n"

    expected = GraphBuilder().build_file_graph(
        parse_profiled_source(
            source_text,
            profile,
            relative_path="src/lib.rs",
            source_root=".",
            repository_label="repo",
            content_hash="hash",
        )
    ).graph.as_dict()
    monkeypatch.setenv("CODEBASE_GRAPH_COMPAT_TREE_SITTER_NORMALIZER", native_tree_sitter_normalizer_binary.as_posix())
    actual = GraphBuilder().build_file_graph(
        parse_profiled_source(
            source_text,
            profile,
            relative_path="src/lib.rs",
            source_root=".",
            repository_label="repo",
            content_hash="hash",
        )
    ).graph.as_dict()

    assert actual == expected


def test_create_tree_sitter_parser_reports_missing_grammar() -> None:
    profile = resolve_language_profile("fortran")
    assert profile is not None
    profile = replace(profile, grammar_package="missing_tree_sitter_fortran")

    with pytest.raises(ParserUnavailableError):
        create_tree_sitter_parser(profile)


def _query_result_shape(result: Any) -> dict[str, Any]:
    return {
        "captures": [capture.capture for capture in result.captures],
        "diagnostics": list(result.diagnostics),
        "tree": result.syntax_nodes[0].as_dict(),
    }
