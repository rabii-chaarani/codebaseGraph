from __future__ import annotations

import shutil
from pathlib import Path

import pytest

import codebase_graph.ingest.materializer as materializer_module
from codebase_graph.db import LadybugCodeGraphStore
from codebase_graph.ingest import (
    GraphMaterializer,
    MarkdownDocumentParser,
    ManifestEntry,
    MaterializationManifest,
    ParserRegistry,
    SourceSnapshot,
    TreeSitterPythonParser,
)
from codebase_graph.ontology import ONTOLOGY_NAME


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


def test_materializer_defaults_to_canonical_codebasegraph_state_paths(tmp_path: Path) -> None:
    source_root = tmp_path / "sample repo"
    source_root.mkdir()

    materializer = GraphMaterializer(source_root, store=object())

    assert materializer.state_dir == source_root / ".codebaseGraph"
    assert materializer.db_path == source_root / ".codebaseGraph" / "sample_repo_graph.ldb"
    assert materializer.manifest_path == source_root / ".codebaseGraph" / "manifest.json"


def test_scan_source_files_uses_parser_registry_for_suffix_mapping(tmp_path: Path) -> None:
    registry = ParserRegistry()
    registry.register(
        "notes",
        suffixes=(".notes",),
        parser_factory=MarkdownDocumentParser,
        parser_version="notes-v1",
    )
    source_root = tmp_path / "project"
    source_root.mkdir()
    (source_root / "handoff.notes").write_text("# Handoff\n", encoding="utf-8")

    materializer = GraphMaterializer(source_root, store=object(), parser_registry=registry)
    snapshots, diagnostics = materializer._scan_source_files()

    assert snapshots["handoff.notes"].language == "notes"
    assert not diagnostics


def test_scan_source_files_prunes_excluded_directories(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    source_dir = tmp_path / "src"
    source_dir.mkdir()
    (source_dir / "app.py").write_text("VALUE = 1\n", encoding="utf-8")
    observed_dirnames: list[tuple[str, ...]] = []

    def fake_walk(root: Path) -> object:
        dirnames = [".venv", "src", "package.egg-info"]
        yield Path(root).as_posix(), dirnames, []
        observed_dirnames.append(tuple(dirnames))
        for dirname in dirnames:
            yield (Path(root) / dirname).as_posix(), [], ["app.py"]

    monkeypatch.setattr("codebase_graph.ingest.materializer.os.walk", fake_walk)
    materializer = GraphMaterializer(tmp_path, db_path=":memory:", manifest_path=tmp_path / "manifest.json", store=object())

    snapshots, diagnostics = materializer._scan_source_files()

    assert observed_dirnames == [("src",)]
    assert tuple(snapshots) == ("src/app.py",)
    assert not diagnostics


def test_full_materialization_writes_python_graph_to_ladybug(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)
    ignored_dir = source_root / ".venv"
    ignored_dir.mkdir()
    (ignored_dir / "ignored.py").write_text("def ignored() -> None:\n    pass\n", encoding="utf-8")

    materializer = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        include_fts=False,
    )
    result = materializer.materialize(mode="full")

    assert result.rebuilt == 4
    assert result.deleted == 0
    assert result.graph_summary["partition_count"] == 4
    assert _labels(materializer, "File") == {
        "__init__.py",
        "README.md",
        "cli.py",
        "service.py",
    }
    assert "README.md" in _labels(materializer, "DocumentationSource")
    assert _labels(materializer, "DocumentationChunk")
    assert "SampleService" in _labels(materializer, "Class")
    assert "run" in _labels(materializer, "Method")
    assert {"helper", "main"} <= _labels(materializer, "Function")
    assert "ignored" not in _labels(materializer, "Function")


def test_full_materialization_handles_local_imports_inside_methods(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = tmp_path / "local_import_project"
    source_root.mkdir()
    (source_root / "service.py").write_text(
        "class Loader:\n"
        "    def load(self) -> object:\n"
        "        from pathlib import Path\n"
        "        return Path('.')\n",
        encoding="utf-8",
    )

    materializer = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        include_fts=False,
    )
    result = materializer.materialize(mode="full")

    assert result.rebuilt == 1
    assert "pathlib.Path" in _labels(materializer, "ImportDeclaration")
    assert "Path" in _labels(materializer, "CallExpression")
    metadata = materializer.store.execute(
        "MATCH (n:`ImportDeclaration` {label: 'pathlib.Path'}) RETURN n.metadata"
    ).get_all()
    assert '"imported_name":"pathlib.Path"' in metadata[0][0]


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

    assert first.rebuilt == 4
    assert second.rebuilt == 0
    assert third.rebuilt == 1
    assert third.rebuilt_paths == ("sample_project/service.py",)
    assert "added" in _labels(materializer, "Function")
    assert fourth.rebuilt == 0
    assert fourth.deleted == 1
    assert fourth.deleted_paths == ("sample_project/cli.py",)
    assert "cli.py" not in _labels(materializer, "File")


def test_changed_ondisk_materialization_rebuilds_atomically_without_inplace_deletes(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"

    GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")
    service_path = source_root / "sample_project" / "service.py"
    service_path.write_text(
        service_path.read_text(encoding="utf-8") + "\n\ndef changed_mode_added() -> str:\n    return 'added'\n",
        encoding="utf-8",
    )

    def fail_clear_graph(self: LadybugCodeGraphStore) -> None:
        raise AssertionError("on-disk changed mode must not clear the target DB in place")

    def fail_delete_partition(self: LadybugCodeGraphStore, *args: object, **kwargs: object) -> None:
        raise AssertionError("on-disk changed mode must not delete target partitions in place")

    monkeypatch.setattr(LadybugCodeGraphStore, "clear_graph", fail_clear_graph)
    monkeypatch.setattr(LadybugCodeGraphStore, "delete_partition", fail_delete_partition)

    result = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(
        mode="changed"
    )

    assert result.mode == "changed"
    assert result.rebuilt == 4
    assert "changed_mode_added" in _labels(
        GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False),
        "Function",
    )


def test_changed_ondisk_materialization_noop_does_not_rebuild(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"

    GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")
    result = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(
        mode="changed"
    )

    assert result.mode == "changed"
    assert result.rebuilt == 0
    assert result.deleted == 0


def test_changed_ondisk_materialization_failure_keeps_previous_db_and_manifest(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = tmp_path / "project"
    source_root.mkdir()
    service_path = source_root / "service.py"
    service_path.write_text("def old_name() -> str:\n    return 'old'\n", encoding="utf-8")
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"

    GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")
    previous_manifest = manifest_path.read_text(encoding="utf-8")
    service_path.write_text("def new_name() -> str:\n    return 'new'\n", encoding="utf-8")
    real_create_ladybug_database = materializer_module.create_ladybug_database

    def failing_create_ladybug_database(db_path: str | Path, *, include_fts: bool = True) -> LadybugCodeGraphStore:
        store = real_create_ladybug_database(db_path, include_fts=include_fts)

        def fail_insert(*args: object, **kwargs: object) -> None:
            raise RuntimeError("changed bulk insert failed")

        store.insert_graphs_bulk = fail_insert  # type: ignore[method-assign]
        return store

    monkeypatch.setattr(materializer_module, "create_ladybug_database", failing_create_ladybug_database)

    with pytest.raises(RuntimeError, match="changed bulk insert failed"):
        GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(
            mode="changed"
        )

    assert manifest_path.read_text(encoding="utf-8") == previous_manifest
    reader = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False)
    assert "old_name" in _labels(reader, "Function")
    assert "new_name" not in _labels(reader, "Function")
    assert not _marker_path(manifest_path).exists()


def test_full_ondisk_materialization_failure_keeps_previous_db_and_manifest(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = tmp_path / "project"
    source_root.mkdir()
    service_path = source_root / "service.py"
    service_path.write_text("def old_name() -> str:\n    return 'old'\n", encoding="utf-8")
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"

    GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")
    previous_manifest = manifest_path.read_text(encoding="utf-8")
    service_path.write_text("def new_name() -> str:\n    return 'new'\n", encoding="utf-8")
    real_create_ladybug_database = materializer_module.create_ladybug_database

    def failing_create_ladybug_database(db_path: str | Path, *, include_fts: bool = True) -> LadybugCodeGraphStore:
        store = real_create_ladybug_database(db_path, include_fts=include_fts)

        def fail_insert(*args: object, **kwargs: object) -> None:
            raise RuntimeError("bulk insert failed")

        store.insert_graphs_bulk = fail_insert  # type: ignore[method-assign]
        return store

    monkeypatch.setattr(materializer_module, "create_ladybug_database", failing_create_ladybug_database)

    with pytest.raises(RuntimeError, match="bulk insert failed"):
        GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")

    assert manifest_path.read_text(encoding="utf-8") == previous_manifest
    reader = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False)
    assert "old_name" in _labels(reader, "Function")
    assert "new_name" not in _labels(reader, "Function")
    assert not _marker_path(manifest_path).exists()


def test_full_ondisk_materialization_replaces_stale_db_without_clear_graph(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = tmp_path / "project"
    source_root.mkdir()
    old_path = source_root / "old_module.py"
    old_path.write_text("def old_name() -> str:\n    return 'old'\n", encoding="utf-8")
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"

    GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")
    old_path.unlink()
    (source_root / "new_module.py").write_text("def new_name() -> str:\n    return 'new'\n", encoding="utf-8")

    def fail_clear_graph(self: LadybugCodeGraphStore) -> None:
        raise AssertionError("full on-disk rebuild must not clear the target DB in place")

    monkeypatch.setattr(LadybugCodeGraphStore, "clear_graph", fail_clear_graph)
    result = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")

    assert result.rebuilt == 1
    reader = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False)
    assert "new_name" in _labels(reader, "Function")
    assert "old_name" not in _labels(reader, "Function")


def test_full_ondisk_materialization_replaces_stale_sidecars(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"

    GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")
    Path(f"{db_path}.wal").write_text("stale wal from previous database", encoding="utf-8")
    Path(f"{db_path}.shadow").write_text("stale shadow from previous database", encoding="utf-8")

    GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")

    reader = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False)
    assert "SampleService" in _labels(reader, "Class")
    assert not Path(f"{db_path}.shadow").exists()


def test_pending_rebuild_marker_forces_changed_mode_atomic_rebuild(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"

    GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="full")
    marker_path = _marker_path(manifest_path)
    marker_path.write_text("{}\n", encoding="utf-8")

    def fail_clear_graph(self: LadybugCodeGraphStore) -> None:
        raise AssertionError("marker recovery must use the atomic rebuild path")

    monkeypatch.setattr(LadybugCodeGraphStore, "clear_graph", fail_clear_graph)
    result = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(mode="changed")

    assert result.mode == "changed"
    assert result.rebuilt == 4
    assert not marker_path.exists()
    reader = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False)
    assert "SampleService" in _labels(reader, "Class")


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


def _marker_path(manifest_path: Path) -> Path:
    return manifest_path.with_suffix(manifest_path.suffix + ".rebuild-pending")
