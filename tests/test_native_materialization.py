from __future__ import annotations

import sys
import types

import pytest

import codebase_graph._native as native_pkg
from codebase_graph._native.materialization import (
    NativeMaterializationUnavailable,
    materialize_syntax_batch,
)


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
