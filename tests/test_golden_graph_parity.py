from __future__ import annotations

import json
from collections import Counter
from pathlib import Path
from typing import Any

from codebase_graph.core import CodeGraph
from codebase_graph.db.store import BulkLoadStats
from codebase_graph.ingest import GraphMaterializer

FIXTURE_ROOT = Path("tests/fixtures/golden_parity_project")
GOLDEN_PATH = Path("tests/fixtures/golden_graph_parity.json")


class CapturingStore:
    def __init__(self) -> None:
        self.graphs: list[CodeGraph] = []

    def clear_graph(self) -> None:
        self.graphs.clear()

    def delete_partition(self, *_args: Any, **_kwargs: Any) -> None:
        return None

    def insert_graphs_bulk(self, graphs: list[CodeGraph], **_kwargs: Any) -> BulkLoadStats:
        self.graphs.extend(graphs)
        return BulkLoadStats(
            node_rows=sum(len(graph.nodes) for graph in graphs),
            edge_rows=sum(len(graph.edges) for graph in graphs),
            connector_rows=sum(len(graph.edges) * 2 for graph in graphs),
            copy_calls=0,
        )

    def close(self) -> None:
        return None


def test_golden_graph_parity_fixture_matches_current_materialization(tmp_path: Path) -> None:
    actual = build_golden_snapshot(tmp_path)
    expected = json.loads(GOLDEN_PATH.read_text(encoding="utf-8"))

    _assert_snapshot_equal(actual, expected)


def build_golden_snapshot(tmp_path: Path) -> dict[str, Any]:
    store = CapturingStore()
    materializer = GraphMaterializer(
        FIXTURE_ROOT,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        include_fts=False,
        store=store,
    )
    result = materializer.materialize(mode="full")

    return {
        "fixture": FIXTURE_ROOT.as_posix(),
        "result": _canonical_result(result.as_dict()),
        "aggregate": _aggregate_graphs(store.graphs),
        "graphs": [_canonical_graph(graph) for graph in sorted(store.graphs, key=_graph_path)],
    }


def _canonical_result(result: dict[str, Any]) -> dict[str, Any]:
    stable = dict(result)
    stable.pop("manifest_path", None)
    return _stable(stable)


def _canonical_graph(graph: CodeGraph) -> dict[str, Any]:
    graph_dict = graph.as_dict()
    return {
        "path": _graph_path(graph),
        "summary": _stable(graph.summary()),
        "metadata": _stable(_stable_graph_metadata(graph_dict["metadata"])),
        "nodes": [_stable(node) for node in graph_dict["nodes"]],
        "edges": [_stable(edge) for edge in graph_dict["edges"]],
    }


def _stable_graph_metadata(metadata: dict[str, Any]) -> dict[str, Any]:
    stable = dict(metadata)
    # First-class semantic rows are checked through graph edges; these lists are duplicate bookkeeping.
    stable.pop("semantic_evidence_links", None)
    stable.pop("semantic_resolution_evidence", None)
    stable.pop("semantic_evidence_fallback", None)
    return stable


def _aggregate_graphs(graphs: list[CodeGraph]) -> dict[str, Any]:
    node_counts: Counter[str] = Counter()
    edge_counts: Counter[str] = Counter()
    for graph in graphs:
        node_counts.update(node.table for node in graph.nodes.values())
        edge_counts.update(edge.type for edge in graph.edges.values())
    return {
        "graph_count": len(graphs),
        "node_count": sum(node_counts.values()),
        "edge_count": sum(edge_counts.values()),
        "node_counts": dict(sorted(node_counts.items())),
        "edge_counts": dict(sorted(edge_counts.items())),
    }


def _graph_path(graph: CodeGraph) -> str:
    file_nodes = sorted(graph.nodes_by_type("File"), key=lambda node: (node.path, node.label, node.id))
    if file_nodes:
        return file_nodes[0].path or file_nodes[0].label
    paths = sorted({node.path for node in graph.nodes.values() if node.path})
    return paths[0] if paths else ""


def _stable(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _stable(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        items = [_stable(item) for item in value]
        if all(isinstance(item, dict) for item in items):
            return sorted(items, key=lambda item: json.dumps(item, separators=(",", ":"), sort_keys=True))
        return items
    if isinstance(value, tuple):
        return [_stable(item) for item in value]
    if isinstance(value, str):
        return _stable_string(value)
    return value


def _stable_string(value: str) -> str:
    fixture_root = FIXTURE_ROOT.resolve().as_posix()
    return value.replace(fixture_root, "$FIXTURE_ROOT")


def _assert_snapshot_equal(actual: dict[str, Any], expected: dict[str, Any]) -> None:
    assert actual["fixture"] == expected["fixture"]
    assert actual["result"] == expected["result"]
    assert actual["aggregate"] == expected["aggregate"]
    assert [graph["path"] for graph in actual["graphs"]] == [graph["path"] for graph in expected["graphs"]]
    for actual_graph, expected_graph in zip(actual["graphs"], expected["graphs"], strict=True):
        path = actual_graph["path"]
        assert actual_graph["summary"] == expected_graph["summary"], f"{path} summary changed"
        assert actual_graph["metadata"] == expected_graph["metadata"], f"{path} graph metadata changed"
        _assert_records(path, "nodes", actual_graph["nodes"], expected_graph["nodes"])
        _assert_records(path, "edges", actual_graph["edges"], expected_graph["edges"])


def _assert_records(path: str, record_type: str, actual: list[dict[str, Any]], expected: list[dict[str, Any]]) -> None:
    assert len(actual) == len(expected), f"{path} {record_type} count changed"
    for actual_record, expected_record in zip(actual, expected, strict=True):
        actual_id = actual_record.get("id")
        expected_id = expected_record.get("id")
        assert actual_id == expected_id, f"{path} {record_type} order or id changed: {actual_id} != {expected_id}"
        field_names = sorted(set(actual_record) | set(expected_record))
        for field_name in field_names:
            assert actual_record.get(field_name) == expected_record.get(field_name), (
                f"{path} {record_type} {actual_id} field {field_name} changed"
            )
