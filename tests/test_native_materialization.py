from __future__ import annotations

from collections import Counter
import json
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
from codebase_graph.ingest import GraphMaterializer, MaterializationManifest, TreeSitterPythonParser

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
        '"phase_timings":{"scan_seconds":0.25},"database_written":true}'
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
    assert result.phase_timings["scan_seconds"] == 0.25
    assert "python_json_encode_seconds" in result.phase_timings
    assert "python_json_decode_seconds" in result.phase_timings
    assert "native_call_seconds" in result.phase_timings
    assert result.database_written is True


def test_native_syntax_batch_matches_python_golden_type_counts(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("codebase_graph._native._native")
    python_nodes, python_edges, python_node_ids, python_edge_ids = _python_golden_type_counts(tmp_path)
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
    for path in ("src/app.py", "src/util.py"):
        entry = result.rebuilt_entries[path]
        native_node_ids = {
            node_id
            for node_id in entry["node_ids"]
            if not node_id.startswith(("Repository:", "SourceRoot:"))
        }
        native_edge_ids = set(entry["edge_ids"])
        missing_edge_ids = python_edge_ids[path] - native_edge_ids
        bookkeeping_edge_ids = native_edge_ids - python_edge_ids[path]

        assert native_node_ids == python_node_ids[path]
        assert missing_edge_ids == set()
        assert len(bookkeeping_edge_ids) == 2


def test_native_python_normalizer_matches_python_parse_bundle_contract() -> None:
    extension = pytest.importorskip("codebase_graph._native._native")
    if not hasattr(extension, "normalize_python_source"):
        pytest.skip("native extension does not expose Python normalizer test hook")
    source = (
        "from pkg.mod import thing as alias, other\n"
        "@route('/items')\n"
        "class Service:\n"
        "    @cached\n"
        "    def handle(self, item: Item = default()) -> Response:\n"
        "        self.item = build(item)\n"
        "        return self.item\n"
    )

    expected = TreeSitterPythonParser().parse_source(source)
    actual = json.loads(extension.normalize_python_source(source))

    assert actual == expected


def test_native_materialization_includes_symlinked_source_files(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("codebase_graph._native._native")
    source = tmp_path / "source"
    repo = tmp_path / "repo"
    source.mkdir()
    repo.mkdir()
    (source / "bridge.cc").write_text("int bridge() { return 1; }\n", encoding="utf-8")
    (source / "bridge.h").write_text("int bridge();\n", encoding="utf-8")
    cc_link = repo / "bridge.rs.cc"
    h_link = repo / "bridge.rs.h"
    try:
        cc_link.symlink_to(source / "bridge.cc")
        h_link.symlink_to(source / "bridge.h")
    except OSError as exc:
        pytest.skip(f"symlinks are unavailable on this filesystem: {exc}")

    python_materializer = GraphMaterializer(
        repo,
        db_path=":memory:",
        manifest_path=tmp_path / "python-manifest.json",
        include_fts=False,
        semantic_enrichment=False,
    )
    python_snapshots, _diagnostics = python_materializer._scan_source_files_python()
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE", "1")
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_STRICT", "1")
    native_materializer = GraphMaterializer(
        repo,
        db_path=tmp_path / "native.ladybug",
        manifest_path=tmp_path / "native-manifest.json",
        include_fts=False,
        semantic_enrichment=False,
    )
    payload = native_materializer._native_syntax_batch_payload(
        mode="full",
        previous_manifest=MaterializationManifest(),
        temp_db_path=tmp_path / "native.ladybug",
        staging_dir=tmp_path / "staging",
        strict=True,
    )

    result = materialize_syntax_batch(payload, strict=True)

    assert result is not None
    assert set(result.rebuilt_entries) == set(python_snapshots)
    assert result.rebuilt_entries["bridge.rs.cc"]["language"] == python_snapshots["bridge.rs.cc"].language
    assert result.rebuilt_entries["bridge.rs.h"]["language"] == python_snapshots["bridge.rs.h"].language


def _python_golden_type_counts(
    tmp_path: Path,
) -> tuple[Counter[str], Counter[str], dict[str, set[str]], dict[str, set[str]]]:
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
    node_ids: dict[str, set[str]] = {}
    edge_ids: dict[str, set[str]] = {}
    for graph in store.graphs:
        nodes.update(node.table for node in graph.nodes.values())
        edges.update(edge.type for edge in graph.edges.values())
        for node_id, node in graph.nodes.items():
            relative_path = _fixture_relative_path(node.path or node.metadata.get("path"))
            node_ids.setdefault(relative_path, set()).add(node_id)
        for edge_id, edge in graph.edges.items():
            endpoint = graph.nodes.get(edge.source_id) or graph.nodes.get(edge.target_id)
            if endpoint is None:
                continue
            relative_path = _fixture_relative_path(endpoint.path or endpoint.metadata.get("path"))
            edge_ids.setdefault(relative_path, set()).add(edge_id)
    return nodes, edges, node_ids, edge_ids


def _fixture_relative_path(path: object) -> str:
    text = str(path or "")
    marker = f"{FIXTURE_ROOT.as_posix()}/"
    if marker in text:
        return text.split(marker, 1)[1]
    return text
