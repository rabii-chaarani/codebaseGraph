from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class NativeMaterializationUnavailable(RuntimeError):
    """Raised when strict native batch materialization is requested but unavailable."""


@dataclass(frozen=True, slots=True)
class NativeBulkStats:
    node_rows: int
    edge_rows: int
    connector_rows: int
    copy_calls: int


@dataclass(frozen=True, slots=True)
class NativeSyntaxBatchResult:
    snapshots: dict[str, Any]
    diff: dict[str, Any]
    diagnostics: list[str]
    rebuilt_entries: dict[str, Any]
    bulk_stats: NativeBulkStats
    graph_summary: dict[str, int]
    skipped: bool
    database_written: bool


def materialize_syntax_batch(payload: dict[str, Any], *, strict: bool = False) -> NativeSyntaxBatchResult | None:
    """Run the in-process Rust syntax materialization batch kernel when available."""
    if not strict and os.environ.get("CODEBASE_GRAPH_NATIVE") != "1":
        return None
    try:
        from codebase_graph._native import _native
    except ImportError as exc:
        if strict:
            raise NativeMaterializationUnavailable("native materialization extension is unavailable") from exc
        return None

    try:
        raw = _native.materialize_syntax_batch(json.dumps(payload, separators=(",", ":"), sort_keys=True))
    except Exception as exc:
        if strict:
            raise NativeMaterializationUnavailable(f"native materialization failed: {exc}") from exc
        return None

    result = json.loads(raw)
    return NativeSyntaxBatchResult(
        snapshots=dict(result.get("snapshots", {})),
        diff=dict(result.get("diff", {})),
        diagnostics=list(result.get("diagnostics", ())),
        rebuilt_entries=dict(result.get("rebuilt_entries", {})),
        bulk_stats=NativeBulkStats(
            node_rows=int(result.get("node_rows", 0)),
            edge_rows=int(result.get("edge_rows", 0)),
            connector_rows=int(result.get("connector_rows", 0)),
            copy_calls=int(result.get("copy_calls", 0)),
        ),
        graph_summary=dict(result.get("graph_summary", {})),
        skipped=bool(result.get("skipped", False)),
        database_written=bool(result.get("database_written", False)),
    )


def staging_dir_for(db_path: str | Path) -> str:
    """Return a deterministic staging sibling for a native materialization target."""
    path = Path(db_path)
    return path.with_suffix(path.suffix + ".native-staging").as_posix()
