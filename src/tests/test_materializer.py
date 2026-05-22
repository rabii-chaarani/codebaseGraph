from __future__ import annotations

import shutil
from pathlib import Path

import pytest

from ingest import (
    GraphMaterializer,
    ManifestEntry,
    MaterializationManifest,
    SourceSnapshot,
    TreeSitterPythonParser,
)
from ontology import ONTOLOGY_NAME


def test_manifest_diff_tracks_added_modified_unchanged_and_deleted(tmp_path: Path) -> None:
    manifest = MaterializationManifest(
        files={
            "same.py": _entry("same.py", "same"),
            "changed.py": _entry("changed.py", "old"),
            "deleted.py": _entry("deleted.py", "old"),
        }
    )
    current = {
        "same.py": _snapshot(tmp_path, "same.py", "same"),
        "changed.py": _snapshot(tmp_path, "changed.py", "new"),
        "added.py": _snapshot(tmp_path, "added.py", "new"),
    }

    diff = manifest.diff(current)

    assert diff.added == ("added.py",)
    assert diff.modified == ("changed.py",)
    assert diff.unchanged == ("same.py",)
    assert diff.deleted == ("deleted.py",)
    assert diff.rebuild_paths == ("added.py", "changed.py")
    assert not diff.force_rebuild


def test_manifest_diff_forces_rebuild_on_contract_mismatch(tmp_path: Path) -> None:
    manifest = MaterializationManifest(schema_version=0, ontology=ONTOLOGY_NAME, parser_version="old", files={})
    current = {"service.py": _snapshot(tmp_path, "service.py", "hash")}

    diff = manifest.diff(current)

    assert diff.force_rebuild
    assert diff.added == ("service.py",)
    assert diff.rebuild_paths == ("service.py",)


def test_tree_sitter_python_parser_maps_sample_fixture_to_graph_tree() -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    fixture = Path("tests/fixtures/sample_project/sample_project/service.py")
    parser = TreeSitterPythonParser()

    bundle = parser.parse_file(
        fixture,
        relative_path="sample_project/service.py",
        source_root=fixture.parents[1],
        repository_label="sample",
        content_hash="hash",
    )

    assert bundle.language == "python"
    assert bundle.tree["type"] == "module"
    assert any(child["type"] == "class_definition" and child["name"] == "SampleService" for child in bundle.tree["children"])
    assert any(child["type"] == "function_definition" and child["name"] == "helper" for child in bundle.tree["children"])


def test_full_materialization_writes_python_graph_to_ladybug(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)

    materializer = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        include_fts=False,
    )
    result = materializer.materialize(mode="full")

    assert result.rebuilt == 3
    assert result.deleted == 0
    assert result.graph_summary["partition_count"] == 3
    assert _labels(materializer, "File") == {
        "__init__.py",
        "cli.py",
        "service.py",
    }
    assert "SampleService" in _labels(materializer, "Class")
    assert "run" in _labels(materializer, "Method")
    assert {"helper", "main"} <= _labels(materializer, "Function")


def test_changed_materialization_only_rebuilds_changed_files(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)
    manifest_path = tmp_path / "manifest.json"
    materializer = GraphMaterializer(source_root, db_path=":memory:", manifest_path=manifest_path, include_fts=False)

    first = materializer.materialize(mode="changed")
    second = materializer.materialize(mode="changed")
    service_path = source_root / "sample_project" / "service.py"
    service_path.write_text(service_path.read_text(encoding="utf-8") + "\n\ndef added() -> str:\n    return 'added'\n", encoding="utf-8")
    third = materializer.materialize(mode="changed")
    (source_root / "sample_project" / "cli.py").unlink()
    fourth = materializer.materialize(mode="changed")

    assert first.rebuilt == 3
    assert second.rebuilt == 0
    assert third.rebuilt == 1
    assert third.rebuilt_paths == ("sample_project/service.py",)
    assert "added" in _labels(materializer, "Function")
    assert fourth.rebuilt == 0
    assert fourth.deleted == 1
    assert fourth.deleted_paths == ("sample_project/cli.py",)
    assert "cli.py" not in _labels(materializer, "File")


def _entry(path: str, content_hash: str) -> ManifestEntry:
    return ManifestEntry(
        path=path,
        content_hash=content_hash,
        language="python",
        partition_id=path,
        node_ids=(),
        edge_ids=(),
    )


def _snapshot(tmp_path: Path, path: str, content_hash: str) -> SourceSnapshot:
    return SourceSnapshot(path=path, absolute_path=tmp_path / path, content_hash=content_hash, language="python")


def _copy_fixture(tmp_path: Path) -> Path:
    source = Path("tests/fixtures/sample_project")
    target = tmp_path / "sample_project"
    shutil.copytree(source, target)
    return target


def _labels(materializer: GraphMaterializer, table: str) -> set[str]:
    result = materializer.store.execute(f"MATCH (n:`{table}`) RETURN n.label")
    return {row[0] for row in result.get_all()}
