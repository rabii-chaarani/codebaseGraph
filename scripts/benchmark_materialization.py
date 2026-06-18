#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import statistics
import sys
import tempfile
import time
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

REPO_ROOT = Path(__file__).resolve().parents[1]
SRC_ROOT = REPO_ROOT / "src"
if SRC_ROOT.as_posix() not in sys.path:
    sys.path.insert(0, SRC_ROOT.as_posix())

from codebase_graph.ingest import GraphMaterializer  # noqa: E402

BenchmarkMode = Literal["full", "changed"]
MaterializerFactory = Callable[..., GraphMaterializer]


@dataclass(frozen=True, slots=True)
class MaterializationTiming:
    repo_root: Path
    mode: BenchmarkMode
    phase: str
    iteration: int
    elapsed_seconds: float
    peak_rss_bytes: int | None
    result: dict[str, Any]

    def as_dict(self) -> dict[str, Any]:
        payload = {
            "repo_root": self.repo_root.as_posix(),
            "mode": self.mode,
            "phase": self.phase,
            "iteration": self.iteration,
            "elapsed_seconds": round(self.elapsed_seconds, 6),
            "peak_rss_bytes": self.peak_rss_bytes,
            "result": self.result,
        }
        if phase_timings := _phase_timings(self.result):
            payload["phase_timings"] = phase_timings
        return payload


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    report = run_benchmark(
        repo_roots=args.repo_root,
        mode=args.mode,
        iterations=args.iterations,
        warmups=args.warmups,
        state_dir=args.state_dir,
        include_fts=not args.no_fts,
        semantic_enrichment=not args.no_semantic_enrichment,
    )
    output = json.dumps(report, indent=2, sort_keys=True)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(output + "\n", encoding="utf-8")
    print(output)
    return 0


def run_benchmark(
    repo_roots: Iterable[Path],
    *,
    mode: BenchmarkMode,
    iterations: int,
    warmups: int,
    state_dir: Path | None = None,
    include_fts: bool = True,
    semantic_enrichment: bool = True,
    materializer_factory: MaterializerFactory | None = None,
) -> dict[str, Any]:
    if iterations < 1:
        raise ValueError("iterations must be at least 1")
    if warmups < 0:
        raise ValueError("warmups cannot be negative")

    roots = [Path(root).expanduser().resolve() for root in repo_roots]
    if not roots:
        raise ValueError("at least one --repo-root is required")
    missing = [root.as_posix() for root in roots if not root.exists()]
    if missing:
        raise ValueError(f"repo root does not exist: {', '.join(missing)}")
    factory = materializer_factory or GraphMaterializer

    if state_dir is None:
        with tempfile.TemporaryDirectory(prefix="codebase-graph-benchmark-") as temp_state:
            return _run_with_state_dir(
                roots,
                mode=mode,
                iterations=iterations,
                warmups=warmups,
                state_dir=Path(temp_state),
                include_fts=include_fts,
                semantic_enrichment=semantic_enrichment,
                materializer_factory=factory,
                state_dir_is_temporary=True,
            )

    return _run_with_state_dir(
        roots,
        mode=mode,
        iterations=iterations,
        warmups=warmups,
        state_dir=state_dir.expanduser().resolve(),
        include_fts=include_fts,
        semantic_enrichment=semantic_enrichment,
        materializer_factory=factory,
        state_dir_is_temporary=False,
    )


def _run_with_state_dir(
    repo_roots: list[Path],
    *,
    mode: BenchmarkMode,
    iterations: int,
    warmups: int,
    state_dir: Path,
    include_fts: bool,
    semantic_enrichment: bool,
    materializer_factory: MaterializerFactory,
    state_dir_is_temporary: bool,
) -> dict[str, Any]:
    state_dir.mkdir(parents=True, exist_ok=True)
    repositories = [
        _benchmark_repo(
            repo_root,
            mode=mode,
            iterations=iterations,
            warmups=warmups,
            state_dir=state_dir,
            include_fts=include_fts,
            semantic_enrichment=semantic_enrichment,
            materializer_factory=materializer_factory,
        )
        for repo_root in repo_roots
    ]
    return {
        "benchmark": "materialization",
        "mode": mode,
        "iterations": iterations,
        "warmups": warmups,
        "include_fts": include_fts,
        "semantic_enrichment": semantic_enrichment,
        "native_runtime": True,
        "state_dir": state_dir.as_posix(),
        "state_dir_is_temporary": state_dir_is_temporary,
        "repositories": repositories,
    }


def _benchmark_repo(
    repo_root: Path,
    *,
    mode: BenchmarkMode,
    iterations: int,
    warmups: int,
    state_dir: Path,
    include_fts: bool,
    semantic_enrichment: bool,
    materializer_factory: MaterializerFactory,
) -> dict[str, Any]:
    repo_state_dir = state_dir / _state_name(repo_root)
    timings: list[MaterializationTiming] = []

    if mode == "changed":
        timings.append(
            _time_materialization(
                repo_root,
                mode="full",
                phase="seed",
                iteration=0,
                state_dir=repo_state_dir / "changed",
                include_fts=include_fts,
                semantic_enrichment=semantic_enrichment,
                materializer_factory=materializer_factory,
            )
        )
        for index in range(warmups):
            timings.append(
                _time_materialization(
                    repo_root,
                    mode="changed",
                    phase="warmup",
                    iteration=index + 1,
                    state_dir=repo_state_dir / "changed",
                    include_fts=include_fts,
                    semantic_enrichment=semantic_enrichment,
                    materializer_factory=materializer_factory,
                )
            )
        for index in range(iterations):
            timings.append(
                _time_materialization(
                    repo_root,
                    mode="changed",
                    phase="measured",
                    iteration=index + 1,
                    state_dir=repo_state_dir / "changed",
                    include_fts=include_fts,
                    semantic_enrichment=semantic_enrichment,
                    materializer_factory=materializer_factory,
                )
            )
    else:
        for index in range(warmups):
            timings.append(
                _time_materialization(
                    repo_root,
                    mode="full",
                    phase="warmup",
                    iteration=index + 1,
                    state_dir=repo_state_dir / "full" / f"warmup-{index + 1}",
                    include_fts=include_fts,
                    semantic_enrichment=semantic_enrichment,
                    materializer_factory=materializer_factory,
                )
            )
        for index in range(iterations):
            timings.append(
                _time_materialization(
                    repo_root,
                    mode="full",
                    phase="measured",
                    iteration=index + 1,
                    state_dir=repo_state_dir / "full" / f"measured-{index + 1}",
                    include_fts=include_fts,
                    semantic_enrichment=semantic_enrichment,
                    materializer_factory=materializer_factory,
                )
            )

    measured = [timing for timing in timings if timing.phase == "measured"]
    return {
        "repo_root": repo_root.as_posix(),
        "iterations": [timing.as_dict() for timing in timings],
        "summary": _summary(measured),
    }


def _time_materialization(
    repo_root: Path,
    *,
    mode: BenchmarkMode,
    phase: str,
    iteration: int,
    state_dir: Path,
    include_fts: bool,
    semantic_enrichment: bool,
    materializer_factory: MaterializerFactory,
) -> MaterializationTiming:
    state_dir.mkdir(parents=True, exist_ok=True)
    db_path = state_dir / "graph.ladybug"
    manifest_path = state_dir / "manifest.json"
    materializer = materializer_factory(
        repo_root,
        db_path=db_path,
        manifest_path=manifest_path,
        include_fts=include_fts,
        semantic_enrichment=semantic_enrichment,
    )
    started = time.perf_counter()
    try:
        result = materializer.materialize(mode=mode)
    finally:
        materializer.close()
    elapsed = time.perf_counter() - started
    return MaterializationTiming(
        repo_root=repo_root,
        mode=mode,
        phase=phase,
        iteration=iteration,
        elapsed_seconds=elapsed,
        peak_rss_bytes=_peak_rss_bytes(),
        result=result.as_dict(),
    )


def _summary(timings: list[MaterializationTiming]) -> dict[str, Any]:
    elapsed = [timing.elapsed_seconds for timing in timings]
    latest_result = timings[-1].result if timings else {}
    graph_summary = latest_result.get("graph_summary", {}) if isinstance(latest_result, dict) else {}
    total_elapsed = sum(elapsed)
    total_files = sum(_numeric_result(timing.result, "scanned") for timing in timings)
    total_nodes = sum(_graph_count(timing.result, "node") for timing in timings)
    total_edges = sum(_graph_count(timing.result, "edge") for timing in timings)
    memory_samples = [timing.peak_rss_bytes for timing in timings if timing.peak_rss_bytes is not None]
    return {
        "measured_iterations": len(timings),
        "total_seconds": round(total_elapsed, 6),
        "mean_seconds": round(statistics.fmean(elapsed), 6) if elapsed else 0.0,
        "median_seconds": round(statistics.median(elapsed), 6) if elapsed else 0.0,
        "min_seconds": round(min(elapsed), 6) if elapsed else 0.0,
        "max_seconds": round(max(elapsed), 6) if elapsed else 0.0,
        "files_per_second": _rate(total_files, total_elapsed),
        "nodes_per_second": _rate(total_nodes, total_elapsed),
        "edges_per_second": _rate(total_edges, total_elapsed),
        "peak_rss_bytes": max(memory_samples) if memory_samples else None,
        "latest_graph_summary": graph_summary,
        "phase_timings": _phase_summary(timings),
    }


def _rate(count: int, elapsed_seconds: float) -> float:
    if elapsed_seconds <= 0:
        return 0.0
    return round(count / elapsed_seconds, 6)


def _numeric_result(result: dict[str, Any], key: str) -> int:
    value = result.get(key, 0)
    return int(value) if isinstance(value, int | float) else 0


def _graph_count(result: dict[str, Any], prefix: str) -> int:
    graph_summary = result.get("graph_summary", {})
    if not isinstance(graph_summary, dict):
        return 0
    for key in (f"{prefix}_count", f"{prefix}s"):
        value = graph_summary.get(key)
        if value is not None and isinstance(value, int | float):
            return int(value)
    return 0


def _phase_timings(result: dict[str, Any]) -> dict[str, float]:
    value = result.get("phase_timings", {})
    if not isinstance(value, dict):
        return {}
    timings: dict[str, float] = {}
    for phase, seconds in value.items():
        if isinstance(seconds, int | float):
            timings[str(phase)] = round(float(seconds), 6)
    return timings


def _phase_summary(timings: list[MaterializationTiming]) -> dict[str, dict[str, float]]:
    values_by_phase: dict[str, list[float]] = {}
    for timing in timings:
        for phase, seconds in _phase_timings(timing.result).items():
            values_by_phase.setdefault(phase, []).append(seconds)
    return {
        phase: {
            "total_seconds": round(sum(values), 6),
            "mean_seconds": round(statistics.fmean(values), 6),
            "median_seconds": round(statistics.median(values), 6),
            "min_seconds": round(min(values), 6),
            "max_seconds": round(max(values), 6),
        }
        for phase, values in sorted(values_by_phase.items())
    }


def _peak_rss_bytes() -> int | None:
    try:
        import resource
    except ImportError:
        return None
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform == "darwin":
        return int(rss)
    return int(rss * 1024)


def _state_name(repo_root: Path) -> str:
    digest = hashlib.sha256(repo_root.as_posix().encode("utf-8")).hexdigest()[:10]
    safe_name = "".join(character if character.isalnum() or character in {"-", "_"} else "-" for character in repo_root.name)
    return f"{safe_name or 'repo'}-{digest}"


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Benchmark codebaseGraph materialization baseline throughput.")
    parser.add_argument(
        "--repo-root",
        action="append",
        type=Path,
        required=True,
        help="Repository root to materialize; repeat for representative repositories.",
    )
    parser.add_argument("--mode", choices=("full", "changed"), default="full", help="Materialization mode to measure.")
    parser.add_argument("--iterations", type=int, default=3, help="Measured iterations per repository.")
    parser.add_argument("--warmups", type=int, default=1, help="Warmup iterations per repository.")
    parser.add_argument(
        "--state-dir",
        type=Path,
        default=None,
        help="Directory for benchmark databases and manifests; defaults to an isolated temporary directory.",
    )
    parser.add_argument("--output", type=Path, default=None, help="Optional JSON report path.")
    parser.add_argument("--no-fts", action="store_true", help="Disable FTS setup during materialization.")
    parser.add_argument(
        "--no-semantic-enrichment",
        action="store_true",
        help="Disable semantic enrichment for syntax-only baseline isolation.",
    )
    return parser


if __name__ == "__main__":
    raise SystemExit(main())
