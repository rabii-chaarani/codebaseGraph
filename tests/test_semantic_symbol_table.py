from __future__ import annotations

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode
from codebase_graph.semantic import build_project_symbol_table
from codebase_graph.semantic.symbol_table import candidate_symbol_keys


def test_symbol_table_indexes_declarations_scopes_imports_and_exports() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode("module:app", "Module", "app", qualified_name="app", language="python"))
    graph.add_node(GraphNode("scope:app", "Scope", "app scope", scope_id="module:app", qualified_name="app.<scope>"))
    graph.add_node(
        GraphNode(
            "function:helper",
            "Function",
            "helper",
            qualified_name="app.helper",
            scope_id="module:app",
            language="python",
        )
    )
    graph.add_node(GraphNode("import:path", "ImportDeclaration", "pathlib.Path", metadata={"imported_name": "pathlib.Path"}))
    graph.add_edge(GraphEdge("edge:export", "Exports", "module:app", "function:helper"))

    table = build_project_symbol_table(graph)

    assert table.by_name["helper"][0].node_id == "function:helper"
    assert table.by_node_id["function:helper"].visibility == "exported"
    assert table.scopes[0].owner_node_id == "module:app"
    assert table.imports[0].alias == "Path"


def test_candidate_symbol_keys_are_sorted_for_deterministic_resolution() -> None:
    assert candidate_symbol_keys("User::new") == ("new", "user::new")
