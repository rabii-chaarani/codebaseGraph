from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any

import pytest

from codebase_graph.cli import _build_parser, main as cli_main
from codebase_graph.db import GraphNeighbor, LadybugGraphQueryAdapter, SearchIndexRow
from codebase_graph.ingest import GraphMaterializer
from codebase_graph.mcp.graph_commands import graph_command_spec, graph_tool_specs
from codebase_graph.mcp.runtime import GraphRuntimeConfig
from codebase_graph.mcp.tools import MAX_GRAPH_QUERY_LIMIT, _query_payload, handle_tool_call, tool_specs
from codebase_graph.reasoning import (
    CompactContextBuilder,
    ContextNode,
    collect_source_snippet,
    estimate_token_count,
    merge_context_profiles,
)
from codebase_graph.retrieval.search import CompactContextPayload, SearchHit, SearchRequest, SearchService


class _Result:
    def __init__(self, rows: list[list[Any]]) -> None:
        self.rows = rows
        self.requested_n: int | None = None
        self.closed = False

    def get_all(self) -> list[list[Any]]:
        return self.rows

    def get_n(self, count: int) -> list[list[Any]]:
        self.requested_n = count
        return self.rows[:count]

    def close(self) -> None:
        self.closed = True


class _RecordingStore:
    def __init__(self, rows: list[list[Any]] | None = None) -> None:
        self.rows = rows or []
        self.calls: list[tuple[str, dict[str, Any] | None]] = []

    def execute(self, statement: str, parameters: dict[str, Any] | None = None) -> _Result:
        self.calls.append((statement, parameters))
        self.result = _Result(self.rows)
        return self.result


class _Adapter:
    def __init__(self) -> None:
        self.search_calls: list[dict[str, Any]] = []
        self.neighbor_calls: list[dict[str, Any]] = []

    def search_index(self, *, node_type: str, index_name: str, query: str, limit: int) -> list[SearchIndexRow]:
        self.search_calls.append({"node_type": node_type, "index_name": index_name, "query": query, "limit": limit})
        if node_type != "Class":
            return []
        return [
            SearchIndexRow(
                id="opaque-class-id",
                node_type="Class",
                label="SampleService",
                qualified_name="sample.SampleService",
                path="sample/service.py",
                score=1.0,
            )
        ]

    def neighbors(
        self,
        *,
        node_id: str,
        node_type: str,
        relation: str,
        direction: str,
        limit: int,
    ) -> list[GraphNeighbor]:
        self.neighbor_calls.append(
            {
                "node_id": node_id,
                "node_type": node_type,
                "relation": relation,
                "direction": direction,
                "limit": limit,
            }
        )
        if relation != "Defines" or direction != "outgoing":
            return []
        return [
            GraphNeighbor(
                node_id="opaque-neighbor-id",
                node_type="Method",
                label="run",
                path="sample/service.py",
                line_start=2,
                line_end=3,
                summary="Run the service.",
            )
        ]


class _PathAdapter:
    def search_index(self, *, node_type: str, index_name: str, query: str, limit: int) -> list[SearchIndexRow]:
        return []

    def neighbors(
        self,
        *,
        node_id: str,
        node_type: str,
        relation: str,
        direction: str,
        limit: int,
    ) -> list[GraphNeighbor]:
        if direction != "outgoing":
            return []
        if node_id == "Function:A" and relation == "Calls":
            return [
                GraphNeighbor(
                    node_id="Function:B",
                    node_type="Function",
                    label="B",
                    source_node_id="Function:A",
                    target_node_id="Function:B",
                    edge_id="Calls:A:B",
                    edge_kind="function_call",
                )
            ]
        if node_id == "Function:B" and relation == "References":
            return [
                GraphNeighbor(
                    node_id="Symbol:C",
                    node_type="Symbol",
                    label="C",
                    source_node_id="Function:B",
                    target_node_id="Symbol:C",
                    edge_id="References:B:C",
                    edge_kind="symbol_reference",
                    edge_metadata={"confidence_source": "test"},
                )
            ]
        return []


class _SemanticPathAdapter:
    def search_index(self, *, node_type: str, index_name: str, query: str, limit: int) -> list[SearchIndexRow]:
        return []

    def neighbors(
        self,
        *,
        node_id: str,
        node_type: str,
        relation: str,
        direction: str,
        limit: int,
    ) -> list[GraphNeighbor]:
        if node_id != "CallExpression:helper" or relation != "ResolvesTo" or direction != "outgoing":
            return []
        return [
            GraphNeighbor(
                node_id="Function:plain",
                node_type="Function",
                label="plain",
                source_node_id="CallExpression:helper",
                target_node_id="Function:plain",
                edge_id="edge:plain",
                edge_kind="syntax_resolution",
            ),
            GraphNeighbor(
                node_id="Function:helper",
                node_type="Function",
                label="helper",
                source_node_id="CallExpression:helper",
                target_node_id="Function:helper",
                edge_id="edge:semantic",
                edge_kind="semantic_resolution",
                edge_confidence=0.91,
                edge_metadata={
                    "resolution_source": "symbol_table",
                    "evidence_id": "evidence:helper",
                    "diagnostics": ["matched import binding"],
                },
            ),
        ]


class _DedupePathAdapter:
    def search_index(self, *, node_type: str, index_name: str, query: str, limit: int) -> list[SearchIndexRow]:
        return []

    def neighbors(
        self,
        *,
        node_id: str,
        node_type: str,
        relation: str,
        direction: str,
        limit: int,
    ) -> list[GraphNeighbor]:
        if node_id != "Function:A" or direction != "outgoing" or relation not in {"Calls", "References"}:
            return []
        return [
            GraphNeighbor(
                node_id="Symbol:C",
                node_type="Symbol",
                label="C",
                source_node_id="Function:A",
                target_node_id="Symbol:C",
                edge_id=f"{relation}:A:C",
                edge_kind=relation.lower(),
            )
        ]


class _AdapterStore:
    def __init__(self, adapter: _Adapter) -> None:
        self.graph_query_adapter = adapter


def test_search_query_uses_ontology_index_names_and_parameterized_user_text() -> None:
    malicious_query = "SampleService'); MATCH (n) RETURN n"
    store = _RecordingStore()

    SearchService(store).search(SearchRequest(malicious_query, limit=2, budget=0))

    assert store.calls
    for statement, parameters in store.calls:
        assert statement.startswith("CALL QUERY_FTS_INDEX('")
        assert malicious_query not in statement
        assert parameters == {"query": malicious_query, "top": 10}


@pytest.mark.parametrize(
    ("relation", "edge_kind"),
    (
        ("ResolvesTo", "semantic_resolution"),
        ("Calls", "semantic_call_target"),
        ("References", "semantic_reference"),
    ),
)
def test_query_adapter_projects_relation_metadata_for_semantic_neighbors(relation: str, edge_kind: str) -> None:
    store = _RecordingStore(
        [
            [
                "Function:helper",
                "helper",
                "sample.helper",
                "sample.py",
                3,
                6,
                "Helper function.",
                f"edge:{relation}",
                edge_kind,
                "CallExpression:helper",
                "Function:helper",
                0.91,
                json.dumps({"resolver": "semantic", "evidence_id": f"evidence:{relation}"}),
            ]
        ]
    )

    neighbors = LadybugGraphQueryAdapter(store).neighbors(
        node_id="CallExpression:helper",
        node_type="CallExpression",
        relation=relation,
        direction="outgoing",
        limit=1,
    )

    assert len(neighbors) == 1
    assert neighbors[0].edge_id == f"edge:{relation}"
    assert neighbors[0].edge_kind == edge_kind
    assert neighbors[0].source_node_id == "CallExpression:helper"
    assert neighbors[0].target_node_id == "Function:helper"
    assert neighbors[0].edge_confidence == 0.91
    assert neighbors[0].edge_metadata == {"resolver": "semantic", "evidence_id": f"evidence:{relation}"}


def test_search_result_ranking_dedupes_by_node_id_preserving_best_raw_score() -> None:
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


def test_identifier_query_reranks_concrete_definition_above_generic_symbol() -> None:
    service = SearchService(_RecordingStore())
    hits = [
        SearchHit(
            id="Symbol:SampleService",
            type="Symbol",
            label="SampleService",
            path="sample_project/cli.py",
            score=1.4,
            index_order=0,
        ),
        SearchHit(
            id="Class:SampleService",
            type="Class",
            label="SampleService",
            qualified_name="sample_project.service.SampleService",
            path="sample_project/service.py",
            score=0.2,
            index_order=2,
        ),
    ]

    ranked = service._rank_hits(hits, query="SampleService", profile="brief")

    assert ranked[0].type == "Class"
    assert ranked[0].score == 0.2
    assert ranked[0].rank_score > ranked[1].rank_score
    assert ranked[1].score_components["generic_penalty"] > 0


def test_generic_penalty_only_applies_when_matching_concrete_definition_exists() -> None:
    service = SearchService(_RecordingStore())

    without_definition = service._rank_hits(
        [SearchHit(id="Symbol:SampleService", type="Symbol", label="SampleService", score=1.0)],
        query="SampleService",
        profile="brief",
    )
    with_definition = service._rank_hits(
        [
            SearchHit(id="Symbol:SampleService", type="Symbol", label="SampleService", score=1.0),
            SearchHit(id="Class:SampleService", type="Class", label="SampleService", score=0.1),
        ],
        query="SampleService",
        profile="brief",
    )

    assert without_definition[0].score_components["generic_penalty"] == 0
    symbol_hit = next(hit for hit in with_definition if hit.type == "Symbol")
    assert symbol_hit.score_components["generic_penalty"] > 0


def test_path_and_dependency_intents_boost_matching_ontology_families() -> None:
    service = SearchService(_RecordingStore())
    path_ranked = service._rank_hits(
        [
            SearchHit(id="Function:helper", type="Function", label="helper", path="sample_project/service.py", score=1.0),
            SearchHit(id="File:service", type="File", label="service.py", path="sample_project/service.py", score=0.3),
        ],
        query="service.py",
        profile="brief",
    )
    dependency_ranked = service._rank_hits(
        [
            SearchHit(id="Class:SampleService", type="Class", label="SampleService", score=1.0),
            SearchHit(id="Dependency:service", type="Dependency", label=".service.SampleService", score=0.3),
        ],
        query=".service.SampleService",
        profile="dependencies",
    )

    assert path_ranked[0].type == "File"
    assert dependency_ranked[0].type == "Dependency"


def test_compact_context_respects_max_depth_limit_and_budget() -> None:
    long_summary = " ".join(f"word{i}" for i in range(200))
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
        budget=40,
        max_depth=1,
    )

    assert len(context) == 1
    assert context[0].relation == "Defines"
    assert context[0].direction == "outgoing"
    assert context[0].summary
    assert estimate_token_count(context[0].summary) < estimate_token_count(long_summary)


def test_compact_context_builds_depth_two_evidence_paths_with_edge_metadata() -> None:
    builder = CompactContextBuilder(_AdapterStore(_PathAdapter()))

    context = builder.build(
        "Function:A",
        "Function",
        profile="callgraph",
        limit=3,
        budget=200,
        max_depth=2,
        root_label="A",
    )

    terminal = next(item for item in context if item.label == "C")
    payload = terminal.as_dict()

    assert payload["evidence_path"]["chain"] == "Function A Calls Function B References Symbol C"
    assert payload["evidence_path"]["edges"][1]["edge_id"] == "References:B:C"
    assert payload["evidence_path"]["edges"][1]["metadata"] == {"confidence_source": "test"}


def test_compact_context_adds_semantic_annotations_and_prioritizes_semantic_edges() -> None:
    builder = CompactContextBuilder(
        _AdapterStore(_SemanticPathAdapter()),
        profile_catalog={"semantic": {"relations": ["ResolvesTo"], "max_depth": 1}},
    )

    context = builder.build(
        "CallExpression:helper",
        "CallExpression",
        profile="semantic",
        limit=2,
        budget=200,
        max_depth=1,
        root_label="helper()",
    )
    slim_payload = context[0].as_dict(detail="slim")
    standard_payload = context[0].as_dict(detail="standard")

    assert [node.label for node in context] == ["helper", "plain"]
    assert slim_payload["semantic_annotations"] == [
        {"relation_kind": "semantic_resolution", "confidence": 0.91}
    ]
    assert standard_payload["semantic_annotations"] == [
        {
            "relation_kind": "semantic_resolution",
            "confidence": 0.91,
            "provider": "symbol_table",
            "evidence_ids": ["edge:semantic", "evidence:helper"],
            "diagnostics": ["matched import binding"],
            "metadata": {
                "resolution_source": "symbol_table",
                "evidence_id": "evidence:helper",
                "diagnostics": ["matched import binding"],
            },
        }
    ]


def test_compact_context_semantic_controls_can_hide_annotations_and_confidence() -> None:
    builder = CompactContextBuilder(
        _AdapterStore(_SemanticPathAdapter()),
        profile_catalog={"semantic": {"relations": ["ResolvesTo"], "max_depth": 1}},
    )

    context = builder.build("CallExpression:helper", "CallExpression", profile="semantic", limit=1, budget=200)

    without_semantic = context[0].as_dict(detail="standard", include_semantic=False)
    without_confidence = context[0].as_dict(detail="standard", include_confidence=False)

    assert "semantic_annotations" not in without_semantic
    assert "confidence" not in without_confidence["semantic_annotations"][0]
    assert "confidence" not in without_confidence["evidence_path"]["edges"][0]


def test_compact_context_dedupes_by_terminal_node_plus_relation_chain() -> None:
    builder = CompactContextBuilder(_AdapterStore(_DedupePathAdapter()))

    context = builder.build("Function:A", "Function", profile="callgraph", limit=3, budget=200, max_depth=1, root_label="A")

    assert [item.relation for item in context] == ["Calls", "References"]
    assert {item.evidence_path.chain for item in context if item.evidence_path is not None} == {
        "Function A Calls Symbol C",
        "Function A References Symbol C",
    }


def test_compact_context_uses_adapter_types_and_opaque_node_ids() -> None:
    adapter = _Adapter()
    builder = CompactContextBuilder(_AdapterStore(adapter))

    context = builder.build("opaque-class-id", "Class", profile="definitions", limit=1, budget=120, max_depth=1)

    assert context[0].id == "opaque-neighbor-id"
    assert context[0].type == "Method"
    assert context[0].label == "run"
    assert adapter.neighbor_calls[0]["node_id"] == "opaque-class-id"


def test_search_service_uses_query_adapter_for_fts() -> None:
    adapter = _Adapter()

    payload = SearchService(_AdapterStore(adapter)).search(SearchRequest("SampleService", limit=1, budget=0))

    data = payload.as_dict()
    assert data["results"][0]["id"] == "opaque-class-id"
    assert data["results"][0]["type"] == "Class"
    assert adapter.search_calls


def test_search_request_rejects_invalid_context_limit_and_detail() -> None:
    with pytest.raises(ValueError, match="Context limit must be zero or greater"):
        SearchRequest("SampleService", context_limit=-1).validate()
    with pytest.raises(ValueError, match="Unknown detail level"):
        SearchRequest("SampleService", detail="debug").validate()


def test_search_service_respects_zero_context_limit() -> None:
    adapter = _Adapter()

    payload = SearchService(_AdapterStore(adapter)).search(SearchRequest("SampleService", limit=1, context_limit=0))

    data = payload.as_dict()
    assert data["results"][0]["context"] == []
    assert adapter.search_calls
    assert adapter.neighbor_calls == []


def test_search_payload_threads_semantic_output_controls() -> None:
    payload = CompactContextPayload(
        query="helper",
        profile="brief",
        limit=1,
        budget=600,
        include_semantic=False,
        include_confidence=False,
        results=(
            SearchHit(
                id="CallExpression:helper",
                type="CallExpression",
                label="helper",
                context=[
                    CompactContextBuilder(
                        _AdapterStore(_SemanticPathAdapter()),
                        profile_catalog={"semantic": {"relations": ["ResolvesTo"], "max_depth": 1}},
                    ).build("CallExpression:helper", "CallExpression", profile="semantic", limit=1)[0]
                ],
            ),
        ),
    )

    context = payload.as_dict(detail="standard")["results"][0]["context"][0]

    assert "semantic_annotations" not in context
    assert "confidence" not in context["evidence_path"]["edges"][0]


def test_search_request_rejects_invalid_profile() -> None:
    with pytest.raises(ValueError, match="Unknown context profile"):
        SearchRequest("SampleService", profile="missing").validate()


def test_search_request_accepts_runtime_custom_profile_catalog() -> None:
    catalog = merge_context_profiles(
        {
            "repo_flow": {
                "description": "Repository-defined flow profile.",
                "relations": ["Defines", "Calls"],
                "max_depth": 2,
            }
        }
    )

    SearchRequest("SampleService", profile="repo_flow").validate(catalog)


def test_graph_cli_and_mcp_schemas_accept_semantic_output_controls() -> None:
    parser = _build_parser()

    args = parser.parse_args(
        [
            "graph-search",
            "helper",
            "--no-semantic",
            "--no-confidence",
            "--include-evidence",
        ]
    )
    payload = graph_command_spec("graph-search").payload_from_args(args)
    search_schema = graph_command_spec("graph-search").input_schema["properties"]
    context_schema = graph_command_spec("graph-context").input_schema["properties"]

    assert payload["include_semantic"] is False
    assert payload["include_confidence"] is False
    assert payload["include_evidence"] is True
    assert search_schema["include_semantic"]["default"] is True
    assert search_schema["include_confidence"]["default"] is True
    assert "include_evidence" in context_schema


def test_source_snippet_collection_redacts_secret_like_literals(tmp_path: Path) -> None:
    source = tmp_path / "sample.py"
    source.write_text(
        "def load():\n"
        "    OPENAI_API_KEY = 'sk-testsecret1234567890'\n"
        "    return OPENAI_API_KEY\n",
        encoding="utf-8",
    )

    snippet = collect_source_snippet(
        tmp_path,
        "sample.py",
        {"line_start": 2, "line_end": 2},
    )

    assert snippet is not None
    assert "sk-testsecret" not in snippet.text
    assert "[REDACTED]" in snippet.text
    assert snippet.redactions


def test_slim_payload_omits_diagnostics_and_duplicate_summaries() -> None:
    payload = CompactContextPayload(
        query="run",
        profile="brief",
        limit=1,
        budget=600,
        results=(
            SearchHit(
                id="Method:run",
                type="Method",
                label="run",
                qualified_name="sample.Service.run",
                path="sample/service.py",
                span={"line_start": 4, "line_end": 8},
                score=2.0,
                rank_score=0.9,
                score_components={"fts": 1.0},
                summary="run",
                context=[
                    ContextNode("Defines", "incoming", "Module", "sample.service", "sample/service.py", summary="sample.service"),
                    ContextNode("Documents", "outgoing", "DocumentationChunk", "Usage", "README.md", summary="Use run to start the service."),
                ],
            ),
        ),
    )

    hit = payload.as_dict(detail="slim")["results"][0]

    assert hit == {
        "id": "Method:run",
        "type": "Method",
        "label": "run",
        "rank_score": 0.9,
        "path": "sample/service.py",
        "span": {"line_start": 4, "line_end": 8},
        "context": [
            {
                "relation": "Defines",
                "direction": "incoming",
                "type": "Module",
                "label": "sample.service",
                "path": "sample/service.py",
            },
            {
                "relation": "Documents",
                "direction": "outgoing",
                "type": "DocumentationChunk",
                "label": "Usage",
                "path": "README.md",
                "summary": "Use run to start the service.",
            },
        ],
    }


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
    assert class_hit["rank_score"] > 0
    assert class_hit["score_components"]["type"] > 0
    assert class_hit["context"]
    assert any(item["type"] in {"Module", "Method"} for item in class_hit["context"])


def test_search_service_returns_sample_class_first_for_exact_identifier(tmp_path: Path) -> None:
    _require_graph_runtime()
    materializer = _materialize_fixture(tmp_path, include_fts=True)

    payload = SearchService(materializer.store).search(SearchRequest("SampleService", limit=1))
    hit = payload.as_dict()["results"][0]

    assert hit["type"] == "Class"
    assert hit["label"] == "SampleService"
    assert hit["path"] == "sample_project/service.py"
    assert hit["score"] < 1.0
    assert hit["rank_score"] > hit["score"]


def test_search_service_returns_function_hit_with_score_and_context(tmp_path: Path) -> None:
    _require_graph_runtime()
    materializer = _materialize_fixture(tmp_path, include_fts=True)

    payload = SearchService(materializer.store).search(SearchRequest("helper", limit=3))
    helper_hit = next(hit for hit in payload.as_dict()["results"] if hit["type"] == "Function" and hit["label"] == "helper")

    assert helper_hit["path"] == "sample_project/service.py"
    assert helper_hit["span"]["line_start"] > 0
    assert helper_hit["score"] > 0
    assert helper_hit["rank_score"] > 0
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
        "search",
        "SampleService",
        "--source-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--limit",
        "1",
        "--no-refresh",
        "--json",
    ]) == 0
    top_payload = json.loads(capsys.readouterr().out)
    assert top_payload["results"][0]["type"] == "Class"
    assert top_payload["results"][0]["rank_score"] > top_payload["results"][0]["score"]

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
    context_block = capsys.readouterr().out
    assert context_block.startswith("q helper\n")
    assert "file path sample_project/service.py" in context_block
    assert not context_block.lstrip().startswith("{")

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
        "--json",
    ]) == 0
    context_payload = json.loads(capsys.readouterr().out)
    assert context_payload["results"]
    assert any(hit["label"] == "helper" and hit["context"] for hit in context_payload["results"])


def test_cli_graph_commands_match_mcp_tool_payloads(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
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
    runtime = GraphRuntimeConfig(repo_root=source_root, db_path=db_path, manifest_path=manifest_path)

    assert cli_main([
        "graph-health",
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
    ]) == 0
    health_block = capsys.readouterr().out
    assert health_block.startswith("health ok=true ")
    assert "total_nodes=" in health_block
    assert not health_block.lstrip().startswith("{")

    assert cli_main([
        "graph-health",
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--json",
    ]) == 0
    assert json.loads(capsys.readouterr().out) == handle_tool_call("graph_health", {}, runtime=runtime)

    search_args = {
        "query": "SampleService",
        "limit": 2,
        "profile": "brief",
        "budget": 600,
        "context_limit": 1,
        "detail": "slim",
    }
    assert cli_main([
        "graph-search",
        "SampleService",
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--limit",
        "2",
        "--context-limit",
        "1",
        "--detail",
        "slim",
        "--no-refresh",
        "--json",
    ]) == 0
    search_payload = json.loads(capsys.readouterr().out)
    assert search_payload == handle_tool_call("graph_search", search_args, runtime=runtime)
    assert "score" not in search_payload["results"][0]
    assert len(search_payload["results"][0].get("context", [])) <= 1

    assert cli_main([
        "graph-search",
        "SampleService",
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--limit",
        "2",
        "--context-limit",
        "1",
        "--detail",
        "slim",
        "--no-refresh",
        "--format",
        "block",
    ]) == 0
    block_output = capsys.readouterr().out
    assert block_output.startswith("q SampleService\n")
    assert "file path sample_project/service.py" in block_output
    assert "id=Class:" in block_output
    assert not block_output.lstrip().startswith("{")

    hit = next(item for item in search_payload["results"] if item["label"] == "SampleService")
    context_args = {
        "node_id": hit["id"],
        "node_type": hit["type"],
        "limit": 1,
        "profile": "definitions",
        "budget": 600,
        "context_limit": 3,
        "detail": "slim",
    }
    assert cli_main([
        "graph-context",
        "--node-id",
        hit["id"],
        "--node-type",
        hit["type"],
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--profile",
        "definitions",
        "--limit",
        "1",
        "--detail",
        "slim",
        "--json",
    ]) == 0
    assert json.loads(capsys.readouterr().out) == handle_tool_call("graph_context", context_args, runtime=runtime)

    assert cli_main([
        "graph-context",
        "--node-id",
        hit["id"],
        "--node-type",
        hit["type"],
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--profile",
        "definitions",
        "--limit",
        "1",
        "--detail",
        "slim",
        "--format",
        "block",
    ]) == 0
    context_block = capsys.readouterr().out
    assert context_block.startswith(f"context {hit['type']} id={hit['id']} profile=definitions\n")
    assert "file path " in context_block

    assert cli_main([
        "graph-search",
        "SampleService",
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--limit",
        "2",
        "--context-limit",
        "1",
        "--detail",
        "slim",
        "--no-refresh",
    ]) == 0
    default_block = capsys.readouterr().out
    assert default_block.startswith("q SampleService\n")
    assert not default_block.lstrip().startswith("{")

    statement = "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1"
    query_args = {"statement": statement, "parameters": {}, "limit": 5}
    assert cli_main([
        "graph-query",
        statement,
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--limit",
        "5",
    ]) == 0
    query_block = capsys.readouterr().out
    assert query_block.startswith("query rows=1 truncated=false\n")
    assert "columns total_nodes" in query_block
    assert "row 1 total_nodes=" in query_block
    assert not query_block.lstrip().startswith("{")

    assert cli_main([
        "graph-query",
        statement,
        "--repo-root",
        source_root.as_posix(),
        "--db",
        db_path.as_posix(),
        "--manifest",
        manifest_path.as_posix(),
        "--limit",
        "5",
        "--json",
    ]) == 0
    assert json.loads(capsys.readouterr().out) == handle_tool_call("graph_query", query_args, runtime=runtime)


def test_graph_command_specs_drive_mcp_tool_specs() -> None:
    assert tool_specs() == graph_tool_specs()


def test_graph_command_specs_build_cli_payloads() -> None:
    parser = _build_parser()
    cases = [
        (
            [
                "graph-search",
                "SampleService",
                "--limit",
                "2",
                "--context-limit",
                "1",
                "--detail",
                "slim",
            ],
            "graph_search",
            {
                "query": "SampleService",
                "limit": 2,
                "profile": "brief",
                "budget": 600,
                "context_limit": 1,
                "detail": "slim",
            },
        ),
        (
            [
                "graph-context",
                "--node-id",
                "Class:1",
                "--node-type",
                "Class",
                "--profile",
                "definitions",
                "--limit",
                "1",
                "--detail",
                "slim",
            ],
            "graph_context",
            {
                "node_id": "Class:1",
                "node_type": "Class",
                "limit": 1,
                "profile": "definitions",
                "budget": 600,
                "context_limit": 3,
                "detail": "slim",
            },
        ),
        (
            [
                "graph-query",
                "MATCH (n) RETURN n",
                "--parameters",
                '{"limit": 1}',
                "--limit",
                "5",
            ],
            "graph_query",
            {"statement": "MATCH (n) RETURN n", "parameters": {"limit": 1}, "limit": 5},
        ),
    ]
    for argv, tool_name, expected_payload in cases:
        args = parser.parse_args(argv)
        spec = graph_command_spec(args.command)

        assert spec.tool_name == tool_name
        assert spec.payload_from_args(args) == expected_payload


def test_cli_graph_metadata_commands_do_not_open_graph_db(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli_main(["graph-schema"]) == 0
    schema_output = capsys.readouterr().out
    assert schema_output.startswith("schema ")
    assert "node_types " in schema_output
    assert not schema_output.lstrip().startswith("{")

    assert cli_main(["graph-schema", "--json"]) == 0
    schema_json_output = capsys.readouterr().out
    assert "\n  " not in schema_json_output
    schema = json.loads(schema_json_output)
    assert schema["ontology"]
    assert schema["context_profiles"]

    assert cli_main(["graph-schema", "--json", "--pretty"]) == 0
    pretty_schema_output = capsys.readouterr().out
    assert "\n  " in pretty_schema_output
    assert json.loads(pretty_schema_output)["ontology"]

    assert cli_main(["graph-query-helpers"]) == 0
    helpers_block = capsys.readouterr().out
    assert helpers_block.startswith("query_helpers count=")
    assert "repository_overview" in helpers_block
    assert not helpers_block.lstrip().startswith("{")

    assert cli_main(["graph-query-helpers", "--json"]) == 0
    helpers = json.loads(capsys.readouterr().out)
    assert any(helper["name"] == "repository_overview" for helper in helpers["query_helpers"])

    assert cli_main(["graph-architecture-queries", "--group", "overview"]) == 0
    architecture_block = capsys.readouterr().out
    assert architecture_block.startswith("architecture_queries ")
    assert "group overview " in architecture_block
    assert not architecture_block.lstrip().startswith("{")

    assert cli_main(["graph-architecture-queries", "--group", "overview", "--json"]) == 0
    architecture = json.loads(capsys.readouterr().out)
    assert [group["name"] for group in architecture["groups"]] == ["overview"]


def test_cli_graph_query_rejects_write_like_statements(tmp_path: Path) -> None:
    _require_graph_runtime()
    source_root = _copy_fixture(tmp_path)
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"
    materializer = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path)
    try:
        materializer.materialize(mode="full")
    finally:
        materializer.close()

    with pytest.raises(SystemExit) as exc_info:
        cli_main([
            "graph-query",
            "MATCH (n) DELETE n",
            "--repo-root",
            source_root.as_posix(),
            "--db",
            db_path.as_posix(),
            "--manifest",
            manifest_path.as_posix(),
        ])

    assert exc_info.value.code == 2


def test_graph_query_fetches_limit_plus_one_rows_without_materializing_all() -> None:
    store = _RecordingStore([[1], [2], [3], [4]])

    payload = _query_payload(store, {"statement": "MATCH (n) RETURN n", "limit": 2})

    assert store.result.requested_n == 3
    assert store.result.closed is True
    assert payload == {
        "statement": "MATCH (n) RETURN n",
        "row_count": 2,
        "rows": [[1], [2]],
        "truncated": True,
    }


def test_graph_query_rejects_unbounded_response_limits() -> None:
    store = _RecordingStore([[1]])

    with pytest.raises(ValueError, match="greater than zero"):
        _query_payload(store, {"statement": "MATCH (n) RETURN n", "limit": 0})
    with pytest.raises(ValueError, match=f"{MAX_GRAPH_QUERY_LIMIT} or less"):
        _query_payload(store, {"statement": "MATCH (n) RETURN n", "limit": MAX_GRAPH_QUERY_LIMIT + 1})


def test_graph_query_rejects_procedure_calls() -> None:
    store = _RecordingStore([[1]])

    with pytest.raises(ValueError, match="blocked keyword: CALL"):
        _query_payload(store, {"statement": "CALL CREATE_FTS_INDEX('File', 'label')"})


@pytest.mark.parametrize(
    ("statement", "keyword"),
    [
        ("EXPORT DATABASE '/tmp/graph-export'", "EXPORT"),
        ("IMPORT DATABASE '/tmp/graph-export'", "IMPORT"),
        ("ATTACH '/tmp/other.ldb' AS other", "ATTACH"),
        ("USE other", "USE"),
        ("TRUNCATE TABLE File", "TRUNCATE"),
        ("UPDATE File SET label = 'x'", "UPDATE"),
    ],
)
def test_graph_query_rejects_database_administration_statements(statement: str, keyword: str) -> None:
    store = _RecordingStore([[1]])

    with pytest.raises(ValueError, match=f"blocked keyword: {keyword}"):
        _query_payload(store, {"statement": statement})


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
