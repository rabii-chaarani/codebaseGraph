from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

DOC_PATH = Path("docs/rust_rewrite.md")
SCRIPT_PATH = Path("scripts/benchmark_materialization.py")


class FakeMaterializationResult:
    def __init__(self, *, mode: str) -> None:
        self.mode = mode

    def as_dict(self) -> dict[str, Any]:
        return {
            "mode": self.mode,
            "scanned": 2,
            "rebuilt": 2 if self.mode == "full" else 0,
            "skipped": 0 if self.mode == "full" else 2,
            "deleted": 0,
            "diagnostics": [],
            "manifest_path": "manifest.json",
            "rebuilt_paths": ["sample.py"] if self.mode == "full" else [],
            "skipped_paths": [] if self.mode == "full" else ["sample.py"],
            "deleted_paths": [],
            "graph_summary": {"nodes": 3, "edges": 2},
        }


class FakeMaterializer:
    calls: list[dict[str, Any]] = []

    def __init__(
        self,
        source_root: Path,
        *,
        db_path: Path,
        manifest_path: Path,
        include_fts: bool,
        semantic_enrichment: bool,
    ) -> None:
        self.source_root = source_root
        self.db_path = db_path
        self.manifest_path = manifest_path
        self.include_fts = include_fts
        self.semantic_enrichment = semantic_enrichment

    def materialize(self, mode: str) -> FakeMaterializationResult:
        self.calls.append(
            {
                "source_root": self.source_root,
                "db_path": self.db_path,
                "manifest_path": self.manifest_path,
                "include_fts": self.include_fts,
                "semantic_enrichment": self.semantic_enrichment,
                "mode": mode,
            }
        )
        return FakeMaterializationResult(mode=mode)

    def close(self) -> None:
        self.calls.append({"closed": self.source_root})


def test_rust_rewrite_design_doc_defines_boundary_benchmark_and_parity() -> None:
    text = DOC_PATH.read_text(encoding="utf-8")

    assert "codebase_graph._native" in text
    assert "pyo3" in text
    assert "maturin" in text
    assert "CODEBASE_GRAPH_NATIVE=1" in text
    assert "python scripts/benchmark_materialization.py" in text
    assert "Golden parity fixtures" in text
    assert "ParseBundle" in text
    assert "GraphBuildResult" in text
    assert "BulkLoadStats" in text


def test_materialization_benchmark_reports_full_mode_with_isolated_state(tmp_path: Path) -> None:
    module = _load_benchmark_script()
    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    FakeMaterializer.calls = []

    report = module.run_benchmark(
        [repo_root],
        mode="full",
        iterations=2,
        warmups=1,
        state_dir=tmp_path / "state",
        include_fts=False,
        semantic_enrichment=False,
        materializer_factory=FakeMaterializer,
    )

    materialize_calls = [call for call in FakeMaterializer.calls if "mode" in call]
    assert [call["mode"] for call in materialize_calls] == ["full", "full", "full"]
    assert {call["db_path"].name for call in materialize_calls} == {"graph.ladybug"}
    assert len({call["db_path"].parent for call in materialize_calls}) == 3
    assert all(call["include_fts"] is False for call in materialize_calls)
    assert all(call["semantic_enrichment"] is False for call in materialize_calls)
    assert report["repositories"][0]["summary"]["measured_iterations"] == 2
    assert report["repositories"][0]["summary"]["latest_graph_summary"] == {"edges": 2, "nodes": 3}


def test_materialization_benchmark_seeds_changed_mode_once(tmp_path: Path) -> None:
    module = _load_benchmark_script()
    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    FakeMaterializer.calls = []

    report = module.run_benchmark(
        [repo_root],
        mode="changed",
        iterations=2,
        warmups=1,
        state_dir=tmp_path / "state",
        include_fts=True,
        semantic_enrichment=True,
        materializer_factory=FakeMaterializer,
    )

    materialize_calls = [call for call in FakeMaterializer.calls if "mode" in call]
    assert [call["mode"] for call in materialize_calls] == ["full", "changed", "changed", "changed"]
    assert len({call["manifest_path"] for call in materialize_calls}) == 1
    assert [item["phase"] for item in report["repositories"][0]["iterations"]] == [
        "seed",
        "warmup",
        "measured",
        "measured",
    ]


def test_materialization_benchmark_cli_writes_json_report(tmp_path: Path, monkeypatch: Any) -> None:
    module = _load_benchmark_script()
    repo_root = tmp_path / "repo"
    output_path = tmp_path / "report.json"
    repo_root.mkdir()
    FakeMaterializer.calls = []
    monkeypatch.setattr(module, "GraphMaterializer", FakeMaterializer)

    assert module.main(["--repo-root", str(repo_root), "--iterations", "1", "--warmups", "0", "--output", str(output_path)]) == 0

    report = json.loads(output_path.read_text(encoding="utf-8"))
    assert report["benchmark"] == "materialization"
    assert report["repositories"][0]["summary"]["measured_iterations"] == 1


def _load_benchmark_script() -> Any:
    spec = importlib.util.spec_from_file_location("benchmark_materialization", SCRIPT_PATH)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module
