from __future__ import annotations

import json
from collections import Counter
from pathlib import Path
import subprocess
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
from codebase_graph.ingest import GraphMaterializer

FIXTURE_ROOT = Path("tests/fixtures/golden_parity_project")
REPO_ROOT = Path(__file__).resolve().parents[1]


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


def test_native_materialization_non_strict_still_fails_explicitly(monkeypatch: pytest.MonkeyPatch) -> None:

    with pytest.raises(NativeMaterializationUnavailable, match="native materialization"):
        materialize_syntax_batch({}, strict=False)


def test_native_materialization_strict_requires_extension(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delattr(native_pkg, "_native", raising=False)
    monkeypatch.setitem(sys.modules, "codebase_graph._native._native", None)

    with pytest.raises(NativeMaterializationUnavailable, match="extension is unavailable"):
        materialize_syntax_batch({}, strict=True)


def test_native_materialization_decodes_extension_result(monkeypatch: pytest.MonkeyPatch) -> None:
    extension = types.ModuleType("codebase_graph._native._native")
    payloads: list[dict[str, Any]] = []

    def materialize(payload: str) -> str:
        import json

        payloads.append(json.loads(payload))
        return (
            '{"snapshots":{},"diff":{"added":[],"modified":[],"unchanged":[],"deleted":[],"force_rebuild":false},'
            '"diagnostics":[],"rebuilt_entries":{},"node_rows":1,"edge_rows":2,"connector_rows":3,'
            '"copy_calls":4,"graph_summary":{"node_count":1,"edge_count":2},"skipped":false,'
            '"phase_timings":{"scan_seconds":0.25},"database_written":true}'
        )

    extension.materialize_syntax_batch = materialize
    monkeypatch.setattr(native_pkg, "_native", extension, raising=False)
    monkeypatch.setitem(sys.modules, "codebase_graph._native._native", extension)

    result = materialize_syntax_batch(
        {
            "source_root": "/tmp",
            "semantic_enrichment": True,
            "semantic_provider_mode": "local_only",
        }
    )

    assert result is not None
    assert payloads[0]["semantic_enrichment"] is True
    assert payloads[0]["semantic_provider_mode"] == "local_only"
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
) -> None:
    pytest.importorskip("codebase_graph._native._native")
    python_nodes, python_edges = _python_golden_type_counts(tmp_path)
    native = _native_materialization_subprocess(tmp_path / "native-syntax", semantic_enrichment=False)
    native_nodes = Counter(native["nodes"])
    native_edges = Counter(native["edges"])
    assert dict(sorted(native_nodes.items())) == dict(sorted(python_nodes.items()))
    assert dict(sorted(native_edges.items())) == dict(sorted(python_edges.items()))


def test_native_semantic_syntax_batch_reports_semantic_phase_timings(
    tmp_path: Path,
) -> None:
    pytest.importorskip("codebase_graph._native._native")
    native = _native_materialization_subprocess(tmp_path / "native-semantic", semantic_enrichment=True)

    assert "semantic_symbol_index_seconds" in native["phase_timings"]
    assert "semantic_resolution_seconds" in native["phase_timings"]
    assert "semantic_edge_promotion_seconds" in native["phase_timings"]


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


def _native_materialization_subprocess(tmp_path: Path, *, semantic_enrichment: bool) -> dict[str, Any]:
    tmp_path.mkdir(parents=True, exist_ok=True)
    script = """
from __future__ import annotations

from collections import Counter
from pathlib import Path
import json
import sys

from codebase_graph._native.materialization import materialize_syntax_batch
from codebase_graph.ingest import GraphMaterializer, MaterializationManifest

fixture_root = Path(sys.argv[1])
tmp_path = Path(sys.argv[2])
semantic_enrichment = sys.argv[3] == "1"

materializer = GraphMaterializer(
    fixture_root,
    db_path=tmp_path / "native.ladybug",
    manifest_path=tmp_path / "native-manifest.json",
    include_fts=False,
    semantic_enrichment=semantic_enrichment,
)
payload = materializer._native_syntax_batch_payload(
    mode="full",
    previous_manifest=MaterializationManifest(),
    temp_db_path=tmp_path / "native.ladybug",
    staging_dir=tmp_path / "staging",
    strict=True,
)
result = materialize_syntax_batch(payload, strict=True)
if result is None:
    raise SystemExit("native materialization returned None")
native_nodes = Counter()
native_edges = Counter()
for entry in result.rebuilt_entries.values():
    native_nodes.update(entry.get("node_types", {}).values())
    native_edges.update(entry.get("edge_types", {}).values())
print(json.dumps({
    "nodes": dict(sorted(native_nodes.items())),
    "edges": dict(sorted(native_edges.items())),
    "phase_timings": result.phase_timings,
}, sort_keys=True))
"""
    completed = subprocess.run(
        [
            sys.executable,
            "-c",
            script,
            FIXTURE_ROOT.as_posix(),
            tmp_path.as_posix(),
            "1" if semantic_enrichment else "0",
        ],
        cwd=REPO_ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    assert completed.returncode == 0, completed.stderr or completed.stdout
    return json.loads(completed.stdout)
