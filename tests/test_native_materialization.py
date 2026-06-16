from __future__ import annotations

from collections import Counter
from pathlib import Path
import sys
import types
from typing import Any

import pytest

import codebase_graph._native as native_pkg
from codebase_graph._native.materialization import (
    NativeMaterializationUnavailable,
    materialize_syntax_batch,
)
from codebase_graph.core import CodeGraph
from codebase_graph.db.store import BulkLoadStats
from codebase_graph.ingest import GraphMaterializer, MaterializationManifest

FIXTURE_ROOT = Path("tests/fixtures/golden_parity_project")


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


def test_native_materialization_falls_back_when_disabled(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("CODEBASE_GRAPH_NATIVE", raising=False)

    assert materialize_syntax_batch({}) is None


def test_native_materialization_strict_requires_extension(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("CODEBASE_GRAPH_NATIVE", raising=False)
    monkeypatch.delattr(native_pkg, "_native", raising=False)
    monkeypatch.setitem(sys.modules, "codebase_graph._native._native", None)

    with pytest.raises(NativeMaterializationUnavailable, match="extension is unavailable"):
        materialize_syntax_batch({}, strict=True)


def test_native_materialization_decodes_extension_result(monkeypatch: pytest.MonkeyPatch) -> None:
    extension = types.ModuleType("codebase_graph._native._native")
    extension.materialize_syntax_batch = lambda _payload: (
        '{"snapshots":{},"diff":{"added":[],"modified":[],"unchanged":[],"deleted":[],"force_rebuild":false},'
        '"diagnostics":[],"rebuilt_entries":{},"node_rows":1,"edge_rows":2,"connector_rows":3,'
        '"copy_calls":4,"graph_summary":{"node_count":1,"edge_count":2},"skipped":false,'
        '"database_written":true}'
    )
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE", "1")
    monkeypatch.setattr(native_pkg, "_native", extension, raising=False)
    monkeypatch.setitem(sys.modules, "codebase_graph._native._native", extension)

    result = materialize_syntax_batch({"source_root": "/tmp"})

    assert result is not None
    assert result.bulk_stats.node_rows == 1
    assert result.bulk_stats.edge_rows == 2
    assert result.bulk_stats.connector_rows == 3
    assert result.bulk_stats.copy_calls == 4
    assert result.database_written is True


def test_native_syntax_batch_matches_python_golden_type_counts(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("codebase_graph._native._native")
    python_nodes, python_edges = _python_golden_type_counts(tmp_path)
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE", "1")
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_STRICT", "1")

    materializer = GraphMaterializer(
        FIXTURE_ROOT,
        db_path=tmp_path / "native.ladybug",
        manifest_path=tmp_path / "native-manifest.json",
        include_fts=False,
        semantic_enrichment=False,
    )
    payload = materializer._native_syntax_batch_payload(
        mode="full",
        previous_manifest=MaterializationManifest(),
        temp_db_path=tmp_path / "native.ladybug",
        staging_dir=tmp_path / "staging",
        strict=True,
    )

    result = materialize_syntax_batch(payload, strict=True)

    assert result is not None
    native_nodes = Counter()
    native_edges = Counter()
    for entry in result.rebuilt_entries.values():
        native_nodes.update(entry.get("node_types", {}).values())
        native_edges.update(entry.get("edge_types", {}).values())
    assert dict(sorted(native_nodes.items())) == dict(sorted(python_nodes.items()))
    assert dict(sorted(native_edges.items())) == dict(sorted(python_edges.items()))


def _python_golden_type_counts(tmp_path: Path) -> tuple[Counter[str], Counter[str]]:
    store = CapturingStore()
    materializer = GraphMaterializer(
        FIXTURE_ROOT,
        db_path=":memory:",
        manifest_path=tmp_path / "python-manifest.json",
        include_fts=False,
        semantic_enrichment=False,
        store=store,
    )
    materializer.materialize(mode="full")
    nodes: Counter[str] = Counter()
    edges: Counter[str] = Counter()
    for graph in store.graphs:
        nodes.update(node.table for node in graph.nodes.values())
        edges.update(edge.type for edge in graph.edges.values())
    return nodes, edges
