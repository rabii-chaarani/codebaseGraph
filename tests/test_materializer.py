from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

import codebase_graph.ingest.materializer as materializer_module
from codebase_graph.db import LadybugCodeGraphStore
from codebase_graph.extract import CaptureRecord, ParseBundle
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

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


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


def test_native_syntax_batch_payload_includes_canonical_ontology_schema(tmp_path: Path) -> None:
    materializer = GraphMaterializer(tmp_path, db_path=tmp_path / "graph.ldb", manifest_path=tmp_path / "manifest.json")

    payload = materializer._native_syntax_batch_payload(
        mode="full",
        previous_manifest=MaterializationManifest(),
        temp_db_path=tmp_path / "next.ldb",
        staging_dir=tmp_path / "staging",
        strict=False,
    )

    assert payload["ontology"] == ONTOLOGY_NAME
    assert payload["ontology_schema"]["ontology"] == ONTOLOGY_NAME
    assert any(
        relation["name"] == "References"
        and relation["source_types"]
        and relation["target_types"]
        for relation in payload["ontology_schema"]["relation_types"]
    )


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
        dirnames = [".mypy_cache", ".tox", ".venv", "node_modules", "src", "package.egg-info", "vendor"]
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


def test_scan_source_files_does_not_hash_unsupported_files(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    source_root = tmp_path / "project"
    source_root.mkdir()
    unsupported = source_root / "archive.bin"
    supported = source_root / "service.py"
    unsupported.write_bytes(b"\0" * 1024)
    supported.write_text("VALUE = 1\n", encoding="utf-8")
    hashed_paths: list[str] = []
    real_file_hash = materializer_module._file_hash

    def recording_file_hash(path: Path) -> str:
        hashed_paths.append(path.name)
        return real_file_hash(path)

    monkeypatch.setattr(materializer_module, "_file_hash", recording_file_hash)
    materializer = GraphMaterializer(source_root, db_path=":memory:", manifest_path=tmp_path / "manifest.json", store=object())

    snapshots, diagnostics = materializer._scan_source_files()

    assert hashed_paths == ["service.py"]
    assert snapshots["archive.bin"].language is None
    assert snapshots["archive.bin"].content_hash == ""
    assert snapshots["service.py"].content_hash
    assert diagnostics == ["Skipped unsupported file: archive.bin"]


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


def test_full_materialization_writes_supported_language_graphs(tmp_path: Path) -> None:
    pytest.importorskip("real_ladybug")
    source_root = tmp_path / "mixed_language_project"
    source_root.mkdir()
    (source_root / "lib.rs").write_text(
        "use std::fmt;\n"
        "struct User { id: i32 }\n"
        "impl User { fn new(id: i32) -> Self { User { id } } }\n"
        "fn helper() { User::new(1); }\n",
        encoding="utf-8",
    )
    (source_root / "main.go").write_text(
        "package main\n"
        "import \"fmt\"\n"
        "type User struct { ID int }\n"
        "func (u User) Name() string { fmt.Println(u.ID); return \"\" }\n"
        "func helper() { fmt.Println(1) }\n",
        encoding="utf-8",
    )
    (source_root / "lib.c").write_text(
        "#include <stdio.h>\n"
        "struct User { int id; };\n"
        "void helper(void) { printf(\"%d\", 1); }\n",
        encoding="utf-8",
    )
    (source_root / "lib.cpp").write_text(
        "#include <string>\n"
        "namespace app {\n"
        "class User { public: std::string name(); };\n"
        "std::string User::name() { return std::string(); }\n"
        "void helper() { User u; u.name(); }\n"
        "}\n",
        encoding="utf-8",
    )
    (source_root / "solver.f90").write_text(
        "module math_mod\n"
        "use iso_fortran_env\n"
        "contains\n"
        "subroutine greet()\n"
        "call print_hello()\n"
        "end subroutine greet\n"
        "end module math_mod\n",
        encoding="utf-8",
    )

    materializer = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        include_fts=False,
    )
    result = materializer.materialize(mode="full")

    assert result.rebuilt == 5
    assert not result.diagnostics
    assert _labels(materializer, "File") == {"lib.c", "lib.cpp", "lib.rs", "main.go", "solver.f90"}
    assert {"User", "math_mod", "app"} <= (_labels(materializer, "Class") | _labels(materializer, "Module"))
    assert {"helper", "greet"} <= _labels(materializer, "Function")
    assert {"new", "Name", "name"} <= _labels(materializer, "Method")
    assert {"std::fmt", "fmt", "stdio.h", "string", "iso_fortran_env"} <= _labels(materializer, "ImportDeclaration")
    assert {"User::new", "fmt.Println", "printf", "u.name", "print_hello"} <= _labels(materializer, "CallExpression")


def test_materializer_keeps_python_graph_builder_as_default(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    source_root = _native_fixture_root(tmp_path)
    registry = _native_fixture_registry()

    def fail_native_builder(*args: object, **kwargs: object) -> None:
        raise AssertionError("native graph builder must not run by default")

    monkeypatch.delenv("CODEBASE_GRAPH_NATIVE", raising=False)
    monkeypatch.setattr(materializer_module, "build_native_file_graph", fail_native_builder)

    store = CapturingStore()
    result = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        store=store,
        parser_registry=registry,
        semantic_enrichment=False,
    ).materialize(mode="full")

    assert result.rebuilt == 1
    assert {node.label for node in store.graphs[0].nodes_by_type("Function")} == {"helper"}


def test_materializer_uses_native_graph_builder_when_opted_in(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_binary = _native_graph_builder_binary()
    source_root = _native_fixture_root(tmp_path)
    registry = _native_fixture_registry()

    python_store = CapturingStore()
    GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "python_manifest.json",
        store=python_store,
        parser_registry=registry,
        semantic_enrichment=False,
    ).materialize(mode="full")

    native_calls: list[str] = []
    real_native_builder = materializer_module.build_native_file_graph

    def recording_native_builder(bundle: ParseBundle, **kwargs: object) -> object:
        native_calls.append(bundle.path)
        return real_native_builder(bundle, **kwargs)

    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE", "1")
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_GRAPH_BUILDER", native_binary.as_posix())
    monkeypatch.setattr(materializer_module, "build_native_file_graph", recording_native_builder)
    native_store = CapturingStore()

    result = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "native_manifest.json",
        store=native_store,
        parser_registry=registry,
        semantic_enrichment=False,
    ).materialize(mode="full")

    assert result.rebuilt == 1
    assert native_calls == ["service.native"]
    assert native_store.graphs[0].as_dict() == python_store.graphs[0].as_dict()


def test_native_scan_diff_matches_python_rebuild_decisions(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_binary = _native_graph_builder_binary()
    source_root = tmp_path / "scan_project"
    source_root.mkdir()
    (source_root / "same.py").write_text("VALUE = 'same'\n", encoding="utf-8")
    (source_root / "changed.py").write_text("VALUE = 'new'\n", encoding="utf-8")
    (source_root / "added.py").write_text("VALUE = 'added'\n", encoding="utf-8")
    (source_root / "archive.bin").write_bytes(b"\0" * 128)
    ignored_dir = source_root / ".venv"
    ignored_dir.mkdir()
    (ignored_dir / "ignored.py").write_text("VALUE = 'ignored'\n", encoding="utf-8")
    registry = ParserRegistry()
    registry.register("python", suffixes=(".py",), parser_factory=ToyParser, parser_version="toy-v1")
    previous_manifest = MaterializationManifest(
        parser_version="toy-v1",
        files={
            "same.py": _entry("same.py", materializer_module._file_hash(source_root / "same.py")),
            "changed.py": _entry("changed.py", "old-hash"),
            "deleted.py": _entry("deleted.py", "old-hash"),
        },
    )
    materializer = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        store=object(),
        parser_registry=registry,
        semantic_enrichment=False,
    )

    monkeypatch.delenv("CODEBASE_GRAPH_NATIVE", raising=False)
    expected = materializer._scan_source_state(previous_manifest)
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE", "1")
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_SCAN_DIFF", native_binary.as_posix())
    actual = materializer._scan_source_state(previous_manifest)

    assert _scan_state_shape(actual) == _scan_state_shape(expected)


def test_materializer_uses_native_scan_when_opted_in(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_binary = _native_graph_builder_binary()
    source_root = tmp_path / "native_scan_project"
    source_root.mkdir()
    (source_root / "service.py").write_text("VALUE = 1\n", encoding="utf-8")
    registry = ParserRegistry()
    registry.register("python", suffixes=(".py",), parser_factory=ToyParser, parser_version="toy-v1")

    def fail_python_hash(path: Path) -> str:
        raise AssertionError(f"native scan should hash {path.name}")

    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE", "1")
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_SCAN_DIFF", native_binary.as_posix())
    monkeypatch.setattr(materializer_module, "_file_hash", fail_python_hash)
    materializer = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        store=object(),
        parser_registry=registry,
        semantic_enrichment=False,
    )

    snapshots, diagnostics = materializer._scan_source_files()

    assert not diagnostics
    assert snapshots["service.py"].content_hash
    assert snapshots["service.py"].language == "python"


def test_materializer_runs_local_semantic_enrichment_before_persistence(tmp_path: Path) -> None:
    source_root = tmp_path / "semantic_project"
    source_root.mkdir()
    (source_root / "main.toy").write_text("helper()\n", encoding="utf-8")
    registry = ParserRegistry()
    registry.register("toy", suffixes=(".toy",), parser_factory=ToyParser, parser_version="toy-v1")
    store = CapturingStore()

    result = GraphMaterializer(
        source_root,
        db_path=":memory:",
        manifest_path=tmp_path / "manifest.json",
        store=store,
        parser_registry=registry,
    ).materialize(mode="full")

    graph = store.graphs[0]
    assert result.rebuilt == 1
    assert any(edge.type == "ResolvesTo" and edge.kind == "semantic_resolution" for edge in graph.edges.values())
    assert any(edge.type == "Calls" and edge.kind == "semantic_call_target" for edge in graph.edges.values())
    assert graph.metadata["semantic_enrichment"]["provider_resolution"] is False


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


def test_ondisk_materialization_rejects_concurrent_writer_lock(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"
    lock_path = Path(f"{db_path}.lock")
    lock_path.write_text(json.dumps({"pid": os.getpid()}) + "\n", encoding="utf-8")

    with pytest.raises(RuntimeError, match="materialization is already in progress"):
        GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(
            mode="full"
        )

    assert lock_path.exists()


def test_ondisk_materialization_recovers_stale_writer_lock(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    source_root = _copy_fixture(tmp_path)
    db_path = tmp_path / "graph.lbug"
    manifest_path = tmp_path / "manifest.json"
    lock_path = Path(f"{db_path}.lock")
    lock_path.write_text(json.dumps({"pid": 123456, "db_path": db_path.as_posix()}) + "\n", encoding="utf-8")
    monkeypatch.setattr(materializer_module, "_process_is_running", lambda pid: False)

    result = GraphMaterializer(source_root, db_path=db_path, manifest_path=manifest_path, include_fts=False).materialize(
        mode="full"
    )

    assert result.rebuilt == 4
    assert not lock_path.exists()


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


def _native_graph_builder_binary() -> Path:
    if shutil.which("cargo") is None:
        pytest.skip("cargo is required for native materializer integration tests")

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


def _native_fixture_root(tmp_path: Path) -> Path:
    source_root = tmp_path / "native_materializer_project"
    source_root.mkdir()
    (source_root / "service.native").write_text("helper()\n", encoding="utf-8")
    return source_root


def _native_fixture_registry() -> ParserRegistry:
    registry = ParserRegistry()
    registry.register(
        "native_fixture",
        suffixes=(".native",),
        parser_factory=NativeCaptureParser,
        parser_version="native-fixture-v1",
    )
    return registry


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


def _scan_state_shape(state) -> dict[str, object]:  # noqa: ANN001
    return {
        "snapshots": {
            path: {
                "content_hash": snapshot.content_hash,
                "language": snapshot.language,
            }
            for path, snapshot in state.snapshots.items()
        },
        "diagnostics": tuple(state.diagnostics),
        "diff": None
        if state.diff is None
        else {
            "added": state.diff.added,
            "modified": state.diff.modified,
            "unchanged": state.diff.unchanged,
            "deleted": state.diff.deleted,
            "force_rebuild": state.diff.force_rebuild,
        },
    }


class ToyParser:
    language = "toy"
    parser_version = "toy-v1"

    def parse_file(
        self,
        path: Path,
        *,
        relative_path: str,
        source_root: Path,
        repository_label: str,
        content_hash: str,
    ) -> ParseBundle:
        return ParseBundle(
            language=self.language,
            path=relative_path,
            source_text=path.read_text(encoding="utf-8"),
            repository_label=repository_label,
            source_root=source_root.as_posix(),
            content_hash=content_hash,
            tree={
                "type": "module",
                "children": [
                    {"type": "function_definition", "name": "helper", "line_start": 1, "byte_start": 0},
                    {
                        "type": "function_definition",
                        "name": "main",
                        "line_start": 2,
                        "byte_start": 10,
                        "children": [
                            {
                                "type": "call",
                                "function": "helper",
                                "line_start": 3,
                                "byte_start": 20,
                            }
                        ],
                    },
                ],
            },
        )


class NativeCaptureParser:
    language = "native_fixture"
    parser_version = "native-fixture-v1"

    def parse_file(
        self,
        path: Path,
        *,
        relative_path: str,
        source_root: Path,
        repository_label: str,
        content_hash: str,
    ) -> ParseBundle:
        return ParseBundle(
            language=self.language,
            path=relative_path,
            source_text=path.read_text(encoding="utf-8"),
            repository_label=repository_label,
            source_root=source_root.as_posix(),
            content_hash=content_hash,
            captures=(
                CaptureRecord(
                    "definition.function",
                    {
                        "type": "function_definition",
                        "name": "helper",
                        "text": "def helper",
                        "line_start": 1,
                        "line_end": 1,
                        "byte_start": 0,
                        "byte_end": 10,
                    },
                ),
                CaptureRecord(
                    "reference.call",
                    {
                        "type": "call",
                        "name": "helper",
                        "text": "helper",
                        "line_start": 1,
                        "line_end": 1,
                        "byte_start": 0,
                        "byte_end": 6,
                    },
                ),
            ),
        )


class CapturingStore:
    def __init__(self) -> None:
        self.graphs = []

    def clear_graph(self) -> None:
        self.graphs.clear()

    def delete_partition(self, *args: object, **kwargs: object) -> None:
        return

    def insert_graphs_bulk(self, graphs, **kwargs) -> None:  # noqa: ANN001, ANN003
        self.graphs.extend(graphs)
