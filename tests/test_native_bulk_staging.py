from __future__ import annotations

import csv
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode
from codebase_graph.db.store import (
    LadybugCodeGraphStore,
    _build_bulk_staging_tables,
    _write_native_bulk_staging,
)

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


@pytest.fixture(scope="session")
def native_bulk_staging_binary() -> Path:
    if shutil.which("cargo") is None:
        pytest.skip("cargo is required for native bulk staging parity tests")

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


def test_native_bulk_staging_matches_python_files(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    native_bulk_staging_binary: Path,
) -> None:
    monkeypatch.setenv("CODEBASE_GRAPH_COMPAT_BULK_STAGING", native_bulk_staging_binary.as_posix())
    graph = _sample_graph()

    python_dir = tmp_path / "python"
    native_dir = tmp_path / "native"
    python_result = _build_bulk_staging_tables([graph]).write(python_dir)
    native_result = _write_native_bulk_staging([graph], native_dir)

    assert native_result is not None
    assert native_result.node_rows == python_result.node_rows
    assert native_result.edge_rows == python_result.edge_rows
    assert native_result.connector_rows == python_result.connector_rows
    assert _normalize_copy_statements(native_result.copy_statements, native_dir) == _normalize_copy_statements(
        python_result.copy_statements,
        python_dir,
    )
    assert _canonical_staging_files(native_dir) == _canonical_staging_files(python_dir)


def test_store_native_bulk_loader_preserves_stats(
    monkeypatch: pytest.MonkeyPatch,
    native_bulk_staging_binary: Path,
) -> None:
    monkeypatch.setenv("CODEBASE_GRAPH_COMPAT_BULK_STAGING", native_bulk_staging_binary.as_posix())
    graph = _sample_graph()
    expected = _build_bulk_staging_tables([graph])

    store = object.__new__(LadybugCodeGraphStore)
    executed: list[str] = []
    monkeypatch.setattr(store, "execute", executed.append)

    stats = store.insert_graphs_bulk([graph])

    assert stats.node_rows == sum(len(rows) for rows in expected.nodes.values())
    assert stats.edge_rows == sum(len(rows) for rows in expected.edges.values())
    assert stats.connector_rows == sum(len(rows) for rows in expected.connectors.values())
    assert stats.copy_calls == len(executed)


def _sample_graph() -> CodeGraph:
    graph = CodeGraph()
    graph.add_node(
        GraphNode(
            id="file:app",
            table="File",
            label="app.py",
            kind="file",
            language="python",
            path="src/app.py",
            summary="app.py",
            metadata={"canonical_key": "src/app.py", "content_hash": "hash-1", "size_bytes": 42},
        )
    )
    graph.add_node(
        GraphNode(
            id="fn:handler",
            table="Function",
            label="handler",
            kind="function",
            language="python",
            path="src/app.py",
            qualified_name="handler",
            scope_id="file:app",
            line_start=3,
            line_end=6,
            byte_start=12,
            byte_end=44,
            summary="handler",
            metadata={"canonical_key": "src/app.py|Function|handler", "details": {"stable": True}},
        )
    )
    graph.add_node(
        GraphNode(
            id="import:json",
            table="ImportDeclaration",
            label="json",
            kind="import_statement",
            language="python",
            path="src/app.py",
            scope_id="file:app",
            summary="json",
            metadata={"canonical_key": "src/app.py|import|json", "imported_name": "json"},
        )
    )
    graph.add_node(
        GraphNode(
            id="dependency:json",
            table="Dependency",
            label="json",
            kind="dependency",
            path="src/app.py",
            summary="json",
            metadata={"canonical_key": "dependency|json", "version": "3.12", "ecosystem": "python"},
        )
    )
    graph.add_edge(
        GraphEdge(
            id="edge:file-fn",
            type="Contains",
            source_id="file:app",
            target_id="fn:handler",
            kind="file_function",
            metadata={"canonical_key": "contains|file|fn"},
        )
    )
    graph.add_edge(
        GraphEdge(
            id="edge:file-import",
            type="Imports",
            source_id="file:app",
            target_id="import:json",
            kind="declares_import",
            metadata={"canonical_key": "imports|file|json"},
        )
    )
    graph.add_edge(
        GraphEdge(
            id="edge:import-dependency",
            type="DependsOn",
            source_id="import:json",
            target_id="dependency:json",
            kind="import_dependency",
            metadata={"canonical_key": "depends|json"},
        )
    )
    return graph


def _normalize_copy_statements(statements: tuple[str, ...], staging_dir: Path) -> tuple[str, ...]:
    return tuple(statement.replace(staging_dir.as_posix(), "$STAGING") for statement in statements)


def _canonical_staging_files(staging_dir: Path) -> dict[str, Any]:
    files: dict[str, Any] = {}
    for path in sorted(staging_dir.iterdir()):
        if path.suffix == ".json":
            rows = json.loads(path.read_text(encoding="utf-8"))
            files[path.name] = sorted(rows, key=lambda row: json.dumps(row, sort_keys=True))
        elif path.suffix == ".csv":
            with path.open(newline="", encoding="utf-8") as handle:
                rows = list(csv.DictReader(handle))
            files[path.name] = sorted(rows, key=lambda row: (row["from_id"], row["to_id"], row["role"]))
    return files
