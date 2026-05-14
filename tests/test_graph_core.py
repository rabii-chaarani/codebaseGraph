from __future__ import annotations

from pathlib import Path

from codebase_graph import CodebaseGraph

FIXTURE = Path(__file__).parent / "fixtures" / "sample_project"

def test_import_and_status_defaults(tmp_path: Path) -> None:
    graph = CodebaseGraph(source_root=FIXTURE, state_dir=tmp_path / "graph")
    status = graph.status().as_dict()
    assert status["database_exists"] is False
    assert status["stale"] is True
    assert status["database_path"].endswith("knowledge_graph.json")

def test_materialize_schema_search_context_and_cypher(tmp_path: Path) -> None:
    graph = CodebaseGraph(source_root=FIXTURE, state_dir=tmp_path / "graph")
    materialized = graph.materialize()
    assert materialized["summary"]["ontology"] == "codebase_graph_v1"
    assert materialized["summary"]["node_count"] > 0
    schema = graph.schema()
    assert schema["ontology"] == "codebase_graph_v1"
    search = graph.search("SampleService", limit=5)
    assert any(item["label"] == "SampleService" for item in search["items"])
    context = graph.context("SampleService", budget=500)
    assert "SampleService" in context["context"]
    cypher = graph.cypher("MATCH (n:PythonClass) RETURN n.id, n.label, n.qualified_name LIMIT 5")
    assert cypher["count"] == 1
    assert cypher["rows"][0]["n.label"] == "SampleService"
