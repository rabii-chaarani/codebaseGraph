from __future__ import annotations

from pathlib import Path

from codebase_graph import CodebaseGraph
from codebase_graph.code_map import MAX_INDEXED_FILE_BYTES

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

def test_status_and_materialize_ignore_non_indexable_files(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "pyproject.toml").write_text('[project]\nname = "filter-fixture"\nversion = "0.1.0"\n')
    (repo / "included.py").write_text("class IncludedClass:\n    pass\n")
    (repo / "README.MD").write_text("# Fixture\n\nUppercase markdown suffix should stay indexable.\n")

    (repo / "build").mkdir()
    (repo / "build" / "generated.py").write_text("class IgnoredBuildClass:\n    pass\n")
    (repo / "oversized.py").write_text("#" * (MAX_INDEXED_FILE_BYTES + 1))
    (repo / "payload.json").write_text('{"ignored": true}\n')

    graph = CodebaseGraph(source_root=repo, state_dir=tmp_path / "graph")
    assert graph.status().source_file_count == 3

    graph.materialize()
    assert any(item["label"] == "IncludedClass" for item in graph.search("IncludedClass", limit=5)["items"])
    assert graph.search("IgnoredBuildClass", limit=5)["count"] == 0
    assert graph.search("oversized", limit=5)["count"] == 0
    assert graph.search("payload", limit=5)["count"] == 0
