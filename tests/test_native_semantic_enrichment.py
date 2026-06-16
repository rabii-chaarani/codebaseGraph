from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode
from codebase_graph.semantic import ProviderResult, persist_semantic_enrichment

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


@pytest.fixture(scope="session")
def native_semantic_binary() -> Path:
    if shutil.which("cargo") is None:
        pytest.skip("cargo is required for native semantic enrichment tests")

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


def test_native_semantic_batch_matches_python_enrichment(
    monkeypatch: pytest.MonkeyPatch,
    native_semantic_binary: Path,
) -> None:
    monkeypatch.delenv("CODEBASE_GRAPH_NATIVE", raising=False)
    expected_graph = _semantic_graph()
    expected_report = persist_semantic_enrichment((expected_graph,), source_root=".", provider_mode="local_only")

    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE", "1")
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_SEMANTIC_BATCH", native_semantic_binary.as_posix())
    actual_graph = _semantic_graph()
    actual_report = persist_semantic_enrichment((actual_graph,), source_root=".", provider_mode="local_only")

    assert actual_report.as_dict() == expected_report.as_dict()
    assert actual_graph.as_dict() == expected_graph.as_dict()


def test_native_semantic_batch_defers_provider_backed_enrichment_to_python(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE", "1")
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_SEMANTIC_BATCH", "/missing/native-semantic-batch")
    graph = CodeGraph()
    graph.add_node(GraphNode("function:helper", "Function", "helper", qualified_name="helper", language="rust"))
    graph.add_node(GraphNode("call:helper", "CallExpression", "helper", language="rust"))

    persist_semantic_enrichment(
        graph,
        source_root=".",
        provider_mode="local_only",
        provider_results=(ProviderResult("rust_analyzer", "definition", target_symbol="helper", confidence=0.96),),
    )

    evidence = graph.metadata["semantic_resolution_evidence"]
    edge = next(edge for edge in graph.edges.values() if edge.type == "ResolvesTo")
    assert evidence[0]["source"] == "rust_analyzer"
    assert evidence[0]["provider"] == "rust_analyzer"
    assert edge.confidence == 0.96


def _semantic_graph() -> CodeGraph:
    graph = CodeGraph(metadata={"source_path": "src/lib.rs", "language": "rust", "source_root": "."})
    graph.add_node(GraphNode("file:lib", "File", "lib.rs", language="rust", path="src/lib.rs"))
    graph.add_node(
        GraphNode(
            "module:lib",
            "Module",
            "lib",
            language="rust",
            path="src/lib.rs",
            qualified_name="lib",
            scope_id="file:lib",
        )
    )
    graph.add_node(
        GraphNode(
            "scope:module",
            "Scope",
            "lib scope",
            language="rust",
            path="src/lib.rs",
            qualified_name="lib.<scope>",
            scope_id="module:lib",
        )
    )
    graph.add_node(
        GraphNode(
            "function:main",
            "Function",
            "main",
            language="rust",
            path="src/lib.rs",
            qualified_name="lib.main",
            scope_id="module:lib",
        )
    )
    graph.add_node(
        GraphNode(
            "function:helper",
            "Function",
            "helper",
            language="rust",
            path="src/lib.rs",
            qualified_name="lib.helper",
            scope_id="module:lib",
        )
    )
    graph.add_node(
        GraphNode(
            "class:User",
            "Class",
            "User",
            language="rust",
            path="src/lib.rs",
            qualified_name="lib.User",
            scope_id="module:lib",
        )
    )
    graph.add_node(
        GraphNode(
            "parameter:user",
            "Parameter",
            "user",
            language="rust",
            path="src/lib.rs",
            scope_id="function:main",
        )
    )
    graph.add_node(
        GraphNode(
            "type:User",
            "TypeAnnotation",
            "User",
            language="rust",
            path="src/lib.rs",
            scope_id="parameter:user",
        )
    )
    graph.add_node(
        GraphNode(
            "call:helper",
            "CallExpression",
            "helper",
            language="rust",
            path="src/lib.rs",
            scope_id="function:main",
        )
    )
    graph.add_node(
        GraphNode(
            "import:fmt",
            "ImportDeclaration",
            "fmt",
            language="rust",
            path="src/lib.rs",
            scope_id="module:lib",
            metadata={"imported_name": "fmt"},
        )
    )
    graph.add_node(GraphNode("dependency:fmt", "Dependency", "fmt", path="src/lib.rs", qualified_name="fmt"))
    graph.add_node(GraphNode("syntax:call", "SyntaxCapture", "helper()", language="rust", path="src/lib.rs"))
    graph.add_node(GraphNode("syntax:type", "SyntaxCapture", "User", language="rust", path="src/lib.rs"))
    graph.add_node(GraphNode("syntax:import", "SyntaxCapture", "use fmt;", language="rust", path="src/lib.rs"))
    graph.add_edge(GraphEdge("edge:parameter-type", "HasTypeAnnotation", "parameter:user", "type:User", "syntax_type"))
    graph.add_edge(GraphEdge("edge:call-syntax", "DerivedFrom", "call:helper", "syntax:call", "parser_capture"))
    graph.add_edge(GraphEdge("edge:type-syntax", "DerivedFrom", "type:User", "syntax:type", "parser_capture"))
    graph.add_edge(GraphEdge("edge:import-syntax", "DerivedFrom", "import:fmt", "syntax:import", "parser_capture"))
    return graph
