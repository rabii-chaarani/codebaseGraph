from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any

import pytest

from cli import main as cli_main
from ingest import GraphMaterializer
from reasoning import CompactContextBuilder
from retrieval.search import SearchHit, SearchRequest, SearchService


class _Result:
    def __init__(self, rows: list[list[Any]]) -> None:
        self.rows = rows

    def get_all(self) -> list[list[Any]]:
        return self.rows


class _RecordingStore:
    def __init__(self, rows: list[list[Any]] | None = None) -> None:
        self.rows = rows or []
        self.calls: list[tuple[str, dict[str, Any] | None]] = []

    def execute(self, statement: str, parameters: dict[str, Any] | None = None) -> _Result:
        self.calls.append((statement, parameters))
        return _Result(self.rows)


def test_search_query_uses_ontology_index_names_and_parameterized_user_text() -> None:
    malicious_query = "SampleService'); MATCH (n) RETURN n"
    store = _RecordingStore()

    SearchService(store).search(SearchRequest(malicious_query, limit=2, budget=0))

    assert store.calls
    for statement, parameters in store.calls:
        assert statement.startswith("CALL QUERY_FTS_INDEX('")
        assert malicious_query not in statement
        assert parameters == {"query": malicious_query, "top": 2}


def test_search_result_ranking_dedupes_by_node_id_and_uses_stable_tiebreaks() -> None:
    service = SearchService(_RecordingStore())
    hits = [
        SearchHit(id="Function:helper", type="Function", label="helper", score=0.2, index_order=4),
        SearchHit(id="Function:helper", type="Function", label="helper", score=0.7, index_order=4),
        SearchHit(id="Class:SampleService", type="Class", label="SampleService", score=0.7, index_order=2),
        SearchHit(id="File:service", type="File", label="service.py", path="service.py", score=0.1, index_order=8),
    ]

    ranked = service._rank_hits(hits)

    assert [hit.id for hit in ranked] == ["Class:SampleService", "Function:helper", "File:service"]
    assert ranked[1].score == 0.7


def test_compact_context_respects_max_depth_limit_and_budget() -> None:
    long_summary = "x" * 200
    store = _RecordingStore(
        [["Method:run", "run", "sample.SampleService.run", "sample_project/service.py", 4, 6, long_summary]]
    )
    builder = CompactContextBuilder(store)

    assert builder.build("Class:SampleService", "Class", profile="definitions", max_depth=0) == []

    context = builder.build(
        "Class:SampleService",
        "Class",
        profile="definitions",
        limit=1,
        budget=80,
        max_depth=1,
    )

    assert len(context) == 1
    assert context[0].relation == "Defines"
    assert context[0].direction == "outgoing"
    assert context[0].summary
    assert len(context[0].summary) < len(long_summary)


def test_search_request_rejects_invalid_profile() -> None:
    with pytest.raises(ValueError, match="Unknown context profile"):
        SearchRequest("SampleService", profile="missing").validate()


def test_search_service_returns_sample_class_with_compact_context(tmp_path: Path) -> None:
    _require_graph_runtime()
    materializer = _materialize_fixture(tmp_path, include_fts=True)

    payload = SearchService(materializer.store).search(SearchRequest("SampleService", limit=3))
    data = payload.as_dict()

    assert data["query"] == "SampleService"
    assert data["profile"] == "brief"
    class_hit = next(hit for hit in data["results"] if hit["type"] == "Class" and hit["label"] == "SampleService")
    assert class_hit["path"] == "sample_project/service.py"
    assert class_hit["score"] > 0
    assert class_hit["context"]
    assert any(item["type"] in {"Module", "Method"} for item in class_hit["context"])


def test_search_service_returns_function_hit_with_score_and_context(tmp_path: Path) -> None:
    _require_graph_runtime()
    materializer = _materialize_fixture(tmp_path, include_fts=True)

    payload = SearchService(materializer.store).search(SearchRequest("helper", limit=3))
    helper_hit = next(hit for hit in payload.as_dict()["results"] if hit["type"] == "Function" and hit["label"] == "helper")

    assert helper_hit["path"] == "sample_project/service.py"
    assert helper_hit["span"]["line_start"] > 0
    assert helper_hit["score"] > 0
    assert helper_hit["context"]


def test_cli_search_and_context_return_compact_json_without_refresh(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    _require_graph_runtime()
    source_root = _copy_fixture(tmp_path)
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"

    assert cli_main([
        "materialize",
        "--source-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--mode",
        "full",
    ]) == 0
    capsys.readouterr()

    assert cli_main([
        "search",
        "SampleService",
        "--source-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--no-refresh",
        "--json",
    ]) == 0
    search_payload = json.loads(capsys.readouterr().out)
    assert search_payload["results"]
    assert any(hit["label"] == "SampleService" for hit in search_payload["results"])

    assert cli_main([
        "context",
        "helper",
        "--source-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--no-refresh",
    ]) == 0
    context_payload = json.loads(capsys.readouterr().out)
    assert context_payload["results"]
    assert any(hit["label"] == "helper" and hit["context"] for hit in context_payload["results"])


def _require_graph_runtime() -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")


def _materialize_fixture(tmp_path: Path, *, include_fts: bool) -> GraphMaterializer:
    source_root = _copy_fixture(tmp_path)
    materializer = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        include_fts=include_fts,
    )
    materializer.materialize(mode="full")
    return materializer


def _copy_fixture(tmp_path: Path) -> Path:
    source = Path("tests/fixtures/sample_project")
    target = tmp_path / "sample_project"
    shutil.copytree(source, target)
    return target
