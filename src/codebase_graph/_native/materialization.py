from __future__ import annotations

import json
import os
import time
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
    phase_timings: dict[str, float]
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
        encode_started = time.perf_counter()
        encoded_payload = json.dumps(payload, separators=(",", ":"), sort_keys=True)
        python_json_encode_seconds = time.perf_counter() - encode_started
        native_started = time.perf_counter()
        raw = _native.materialize_syntax_batch(encoded_payload)
        native_call_seconds = time.perf_counter() - native_started
    except Exception as exc:
        if strict:
            raise NativeMaterializationUnavailable(f"native materialization failed: {exc}") from exc
        return None

    decode_started = time.perf_counter()
    result = json.loads(raw)
    python_json_decode_seconds = time.perf_counter() - decode_started
    phase_timings = _phase_timings(result.get("phase_timings", {}))
    phase_timings["python_json_encode_seconds"] = python_json_encode_seconds
    phase_timings["python_json_decode_seconds"] = python_json_decode_seconds
    phase_timings["native_call_seconds"] = native_call_seconds
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
        phase_timings=phase_timings,
        skipped=bool(result.get("skipped", False)),
        database_written=bool(result.get("database_written", False)),
    )


def staging_dir_for(db_path: str | Path) -> str:
    """Return a deterministic staging sibling for a native materialization target."""
    path = Path(db_path)
    return path.with_suffix(path.suffix + ".native-staging").as_posix()


def _phase_timings(value: Any) -> dict[str, float]:
    if not isinstance(value, dict):
        return {}
    timings: dict[str, float] = {}
    for key, seconds in value.items():
        if isinstance(seconds, int | float):
            timings[str(key)] = float(seconds)
    return timings
