from __future__ import annotations

import hashlib
import json
import os
import tempfile
from collections.abc import Mapping
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Literal

from codebase_graph.core import CodeGraph
from codebase_graph.db import LadybugCodeGraphStore, create_ladybug_database
from codebase_graph.extract import GraphBuilder
from codebase_graph.ontology import ONTOLOGY_NAME
from codebase_graph.paths import DEFAULT_STATE_DIR, derive_graph_state_paths

from .tree_sitter_parser import ParserRegistry, ParserUnavailableError, default_parser_registry

MaterializeMode = Literal["full", "changed"]

MANIFEST_SCHEMA_VERSION = 1
PARSER_VERSION = "tree-sitter-python-v1+markdown-docs-v1"
EXCLUDED_PARTS = {
    ".git",
    ".venv",
    "__pycache__",
    ".pytest_cache",
    ".ruff_cache",
    "build",
    "dist",
    ".codebase_graph",
    DEFAULT_STATE_DIR,
}


@dataclass(frozen=True, slots=True)
class SourceSnapshot:
    path: str
    absolute_path: Path
    content_hash: str
    language: str | None


@dataclass(frozen=True, slots=True)
class ManifestEntry:
    path: str
    content_hash: str
    language: str
    partition_id: str
    node_ids: tuple[str, ...]
    edge_ids: tuple[str, ...]
    node_types: Mapping[str, str] = field(default_factory=dict)
    edge_types: Mapping[str, str] = field(default_factory=dict)
    materialized_at: str = ""

    @classmethod
    def from_dict(cls, payload: Mapping[str, Any]) -> ManifestEntry:
        return cls(
            path=str(payload["path"]),
            content_hash=str(payload["content_hash"]),
            language=str(payload["language"]),
            partition_id=str(payload["partition_id"]),
            node_ids=tuple(str(value) for value in payload.get("node_ids", ())),
            edge_ids=tuple(str(value) for value in payload.get("edge_ids", ())),
            node_types={str(key): str(value) for key, value in dict(payload.get("node_types", {})).items()},
            edge_types={str(key): str(value) for key, value in dict(payload.get("edge_types", {})).items()},
            materialized_at=str(payload.get("materialized_at", "")),
        )

    def as_dict(self) -> dict[str, Any]:
        return {
            "path": self.path,
            "content_hash": self.content_hash,
            "language": self.language,
            "partition_id": self.partition_id,
            "node_ids": list(self.node_ids),
            "edge_ids": list(self.edge_ids),
            "node_types": dict(self.node_types),
            "edge_types": dict(self.edge_types),
            "materialized_at": self.materialized_at,
        }


@dataclass(frozen=True, slots=True)
class MaterializationManifest:
    schema_version: int = MANIFEST_SCHEMA_VERSION
    ontology: str = ONTOLOGY_NAME
    parser_version: str = PARSER_VERSION
    files: Mapping[str, ManifestEntry] = field(default_factory=dict)

    @classmethod
    def empty(cls, *, parser_version: str = PARSER_VERSION) -> MaterializationManifest:
        return cls(parser_version=parser_version, files={})

    @classmethod
    def load(cls, path: Path) -> MaterializationManifest:
        if not path.exists():
            return cls.empty()
        with path.open("r", encoding="utf-8") as handle:
            payload = json.load(handle)
        return cls(
            schema_version=int(payload.get("schema_version", 0)),
            ontology=str(payload.get("ontology", "")),
            parser_version=str(payload.get("parser_version", "")),
            files={
                str(file_payload["path"]): ManifestEntry.from_dict(file_payload)
                for file_payload in payload.get("files", [])
            },
        )

    def as_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "ontology": self.ontology,
            "parser_version": self.parser_version,
            "files": [entry.as_dict() for entry in sorted(self.files.values(), key=lambda item: item.path)],
        }

    def write(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = path.with_suffix(path.suffix + ".tmp")
        with tmp_path.open("w", encoding="utf-8") as handle:
            json.dump(self.as_dict(), handle, indent=2, sort_keys=True)
            handle.write("\n")
        os.replace(tmp_path, path)

    def is_compatible(self, *, parser_version: str = PARSER_VERSION) -> bool:
        return (
            self.schema_version == MANIFEST_SCHEMA_VERSION
            and self.ontology == ONTOLOGY_NAME
            and self.parser_version == parser_version
        )

    def diff(self, current_files: Mapping[str, SourceSnapshot], *, parser_version: str = PARSER_VERSION) -> ManifestDiff:
        if not self.is_compatible(parser_version=parser_version):
            return ManifestDiff(
                added=tuple(sorted(current_files)),
                modified=(),
                unchanged=(),
                deleted=tuple(sorted(path for path in self.files if path not in current_files)),
                force_rebuild=True,
            )

        added: list[str] = []
        modified: list[str] = []
        unchanged: list[str] = []
        for path, snapshot in current_files.items():
            previous = self.files.get(path)
            if previous is None:
                added.append(path)
            elif previous.content_hash != snapshot.content_hash or previous.language != snapshot.language:
                modified.append(path)
            else:
                unchanged.append(path)

        deleted = [path for path in self.files if path not in current_files]
        return ManifestDiff(
            added=tuple(sorted(added)),
            modified=tuple(sorted(modified)),
            unchanged=tuple(sorted(unchanged)),
            deleted=tuple(sorted(deleted)),
            force_rebuild=False,
        )


@dataclass(frozen=True, slots=True)
class ManifestDiff:
    added: tuple[str, ...]
    modified: tuple[str, ...]
    unchanged: tuple[str, ...]
    deleted: tuple[str, ...]
    force_rebuild: bool = False

    @property
    def rebuild_paths(self) -> tuple[str, ...]:
        return tuple(sorted((*self.added, *self.modified)))


@dataclass(frozen=True, slots=True)
class MaterializationResult:
    mode: MaterializeMode
    scanned: int
    rebuilt: int
    skipped: int
    deleted: int
    diagnostics: tuple[str, ...]
    manifest_path: str
    rebuilt_paths: tuple[str, ...]
    skipped_paths: tuple[str, ...]
    deleted_paths: tuple[str, ...]
    graph_summary: Mapping[str, Any]


class GraphMaterializer:
    def __init__(
        self,
        source_root: str | Path,
        db_path: str | Path | None = None,
        *,
        manifest_path: str | Path | None = None,
        include_fts: bool = True,
        repository_label: str | None = None,
        store: LadybugCodeGraphStore | None = None,
        parser_registry: ParserRegistry | None = None,
        graph_builder: GraphBuilder | None = None,
    ) -> None:
        self.source_root = Path(source_root).resolve()
        paths = derive_graph_state_paths(self.source_root)
        self.state_dir = paths.state_dir
        self.db_path = _normalize_db_path(db_path if db_path is not None else paths.db_path)
        self.manifest_path = Path(manifest_path) if manifest_path is not None else paths.manifest_path
        self.include_fts = include_fts
        self.repository_label = repository_label or self.source_root.name or "repository"
        self._store = store
        self._store_injected = store is not None
        self.parser_registry = parser_registry or default_parser_registry()
        self.parser_version = self.parser_registry.parser_version
        self.builder = graph_builder or GraphBuilder(repository_label=self.repository_label, source_root=self.source_root)

    @property
    def store(self) -> LadybugCodeGraphStore:
        if self._store is None:
            self._store = create_ladybug_database(self.db_path, include_fts=self.include_fts)
        return self._store

    @store.setter
    def store(self, value: LadybugCodeGraphStore | None) -> None:
        self._store = value
        self._store_injected = value is not None

    def materialize(self, mode: MaterializeMode = "changed") -> MaterializationResult:
        if mode not in {"full", "changed"}:
            raise ValueError(f"Unsupported materialization mode: {mode}")

        previous_manifest = self._read_manifest()
        snapshots, diagnostics = self._scan_source_files()
        supported = {path: snapshot for path, snapshot in snapshots.items() if snapshot.language is not None}
        force_atomic_recovery = self._should_force_atomic_recovery()

        if mode == "full" or force_atomic_recovery:
            diff = ManifestDiff(
                added=tuple(sorted(supported)),
                modified=(),
                unchanged=(),
                deleted=tuple(sorted(previous_manifest.files)),
                force_rebuild=True,
            )
            if self._can_atomic_rebuild():
                return self._materialize_full_atomic(
                    mode=mode,
                    snapshots=snapshots,
                    diagnostics=diagnostics,
                    supported=supported,
                    diff=diff,
                )
            self.store.clear_graph()
            retained_node_ids: set[str] = set()
            retained_edge_ids: set[str] = set()
        else:
            diff = previous_manifest.diff(supported, parser_version=self.parser_version)
            if diff.force_rebuild:
                if self._can_atomic_rebuild():
                    return self._materialize_full_atomic(
                        mode=mode,
                        snapshots=snapshots,
                        diagnostics=diagnostics,
                        supported=supported,
                        diff=diff,
                    )
                self.store.clear_graph()
                retained_node_ids = set()
                retained_edge_ids = set()
            else:
                touched_paths = set(diff.rebuild_paths) | set(diff.deleted)
                retained_node_ids = _retained_node_ids(previous_manifest, touched_paths)
                retained_edge_ids = _retained_edge_ids(previous_manifest, touched_paths)
                for path in diff.deleted:
                    self.store.delete_partition(
                        path,
                        manifest_entry=previous_manifest.files.get(path),
                        retained_node_ids=retained_node_ids,
                        retained_edge_ids=retained_edge_ids,
                    )

        rebuilt_entries: dict[str, ManifestEntry] = {}
        rebuilt_graphs: dict[str, CodeGraph] = {}
        for path in diff.rebuild_paths:
            snapshot = supported[path]
            graph = self._build_graph(snapshot)
            rebuilt_graphs[path] = graph
            rebuilt_entries[path] = _manifest_entry(snapshot, graph)

        if not diff.force_rebuild:
            for path in diff.rebuild_paths:
                self.store.delete_partition(
                    path,
                    manifest_entry=previous_manifest.files.get(path),
                    retained_node_ids=retained_node_ids,
                    retained_edge_ids=retained_edge_ids,
                )

        if rebuilt_graphs:
            self.store.insert_graphs_bulk(
                [rebuilt_graphs[path] for path in sorted(rebuilt_graphs)],
                skip_node_ids=retained_node_ids,
                skip_edge_ids=retained_edge_ids,
            )

        next_files = {
            path: entry
            for path, entry in previous_manifest.files.items()
            if path not in set(diff.deleted) | set(diff.rebuild_paths)
        }
        next_files.update(rebuilt_entries)
        next_manifest = MaterializationManifest(parser_version=self.parser_version, files=next_files)
        self._write_manifest(next_manifest)

        return _materialization_result(
            mode=mode,
            snapshots=snapshots,
            diagnostics=diagnostics,
            diff=diff,
            manifest_path=self.manifest_path,
            rebuilt_entries=rebuilt_entries,
            next_manifest=next_manifest,
        )

    def _materialize_full_atomic(
        self,
        *,
        mode: MaterializeMode,
        snapshots: Mapping[str, SourceSnapshot],
        diagnostics: list[str],
        supported: Mapping[str, SourceSnapshot],
        diff: ManifestDiff,
    ) -> MaterializationResult:
        rebuilt_entries: dict[str, ManifestEntry] = {}
        rebuilt_graphs: dict[str, CodeGraph] = {}
        for path in diff.rebuild_paths:
            snapshot = supported[path]
            graph = self._build_graph(snapshot)
            rebuilt_graphs[path] = graph
            rebuilt_entries[path] = _manifest_entry(snapshot, graph)

        next_manifest = MaterializationManifest(parser_version=self.parser_version, files=rebuilt_entries)
        target_db_path = _filesystem_db_path(self.db_path)
        temp_db_path = _temporary_sibling(target_db_path, suffix=".lbug.tmp")
        temp_manifest_path = _temporary_sibling(self.manifest_path, suffix=".manifest.tmp")
        marker_path = self._rebuild_marker_path
        temp_store: LadybugCodeGraphStore | None = None
        try:
            temp_store = create_ladybug_database(temp_db_path, include_fts=self.include_fts)
            if rebuilt_graphs:
                temp_store.insert_graphs_bulk([rebuilt_graphs[path] for path in sorted(rebuilt_graphs)])
            temp_store.close()
            temp_store = None

            next_manifest.write(temp_manifest_path)
            _write_rebuild_marker(marker_path, target_db_path, self.manifest_path)
            self._close_store()
            os.replace(temp_db_path, target_db_path)
            os.replace(temp_manifest_path, self.manifest_path)
            _unlink_if_exists(marker_path)
            self._store = None
        except Exception:
            if temp_store is not None:
                temp_store.close()
            _unlink_if_exists(temp_db_path)
            _unlink_if_exists(temp_manifest_path)
            _unlink_if_exists(temp_manifest_path.with_suffix(temp_manifest_path.suffix + ".tmp"))
            raise

        return _materialization_result(
            mode=mode,
            snapshots=snapshots,
            diagnostics=diagnostics,
            diff=diff,
            manifest_path=self.manifest_path,
            rebuilt_entries=rebuilt_entries,
            next_manifest=next_manifest,
        )

    def _read_manifest(self) -> MaterializationManifest:
        if self._store_injected and self._store is not None and hasattr(self._store, "read_manifest"):
            return self._store.read_manifest(self.manifest_path)
        return MaterializationManifest.load(self.manifest_path)

    def _write_manifest(self, manifest: MaterializationManifest) -> None:
        if self._store_injected and self._store is not None and hasattr(self._store, "write_manifest"):
            self._store.write_manifest(manifest, self.manifest_path)
            return
        manifest.write(self.manifest_path)

    def _can_atomic_rebuild(self) -> bool:
        return not self._store_injected and not _is_memory_db_path(self.db_path)

    def _should_force_atomic_recovery(self) -> bool:
        return self._can_atomic_rebuild() and self._rebuild_marker_path.exists()

    @property
    def _rebuild_marker_path(self) -> Path:
        return self.manifest_path.with_suffix(self.manifest_path.suffix + ".rebuild-pending")

    def _close_store(self) -> None:
        if self._store is None:
            return
        close = getattr(self._store, "close", None)
        if callable(close):
            close()
        self._store = None

    def _scan_source_files(self) -> tuple[dict[str, SourceSnapshot], list[str]]:
        snapshots: dict[str, SourceSnapshot] = {}
        diagnostics: list[str] = []
        for current_root, dirnames, filenames in os.walk(self.source_root):
            dirnames[:] = [name for name in sorted(dirnames) if not _is_excluded_part(name)]
            current_path = Path(current_root)
            for filename in sorted(filenames):
                path = current_path / filename
                if _is_excluded(path, self.source_root):
                    continue
                relative_path = path.relative_to(self.source_root).as_posix()
                language = self.parser_registry.language_for_path(path)
                snapshots[relative_path] = SourceSnapshot(
                    path=relative_path,
                    absolute_path=path,
                    content_hash=_file_hash(path),
                    language=language,
                )
                if language is None:
                    diagnostics.append(f"Skipped unsupported file: {relative_path}")
        return snapshots, diagnostics

    def _build_graph(self, snapshot: SourceSnapshot) -> CodeGraph:
        if snapshot.language is None:
            raise ValueError(f"Cannot build graph for unsupported file: {snapshot.path}")
        try:
            parser = self.parser_registry.parser_for_language(snapshot.language)
            bundle = parser.parse_file(
                snapshot.absolute_path,
                relative_path=snapshot.path,
                source_root=self.source_root,
                repository_label=self.repository_label,
                content_hash=snapshot.content_hash,
            )
        except ParserUnavailableError:
            raise
        result = self.builder.build_file_graph(bundle)
        return result.graph


def _is_excluded_part(part: str) -> bool:
    return part in EXCLUDED_PARTS or part.endswith(".egg-info")


def _normalize_db_path(db_path: str | Path) -> str | Path:
    if _is_memory_db_path(db_path):
        return ":memory:"
    return Path(db_path)


def _is_memory_db_path(db_path: str | Path) -> bool:
    return str(db_path) == ":memory:"


def _filesystem_db_path(db_path: str | Path) -> Path:
    if _is_memory_db_path(db_path):
        raise ValueError("In-memory databases do not have a filesystem path")
    return Path(db_path)


def _temporary_sibling(path: Path, *, suffix: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temp_path = tempfile.mkstemp(prefix=f".{path.name}.", suffix=suffix, dir=path.parent)
    os.close(descriptor)
    os.unlink(temp_path)
    return Path(temp_path)


def _write_rebuild_marker(marker_path: Path, db_path: Path, manifest_path: Path) -> None:
    marker_path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = marker_path.with_suffix(marker_path.suffix + ".tmp")
    with tmp_path.open("w", encoding="utf-8") as handle:
        json.dump(
            {
                "created_at": datetime.now(UTC).isoformat(),
                "db_path": db_path.as_posix(),
                "manifest_path": manifest_path.as_posix(),
            },
            handle,
            indent=2,
            sort_keys=True,
        )
        handle.write("\n")
    os.replace(tmp_path, marker_path)


def _unlink_if_exists(path: Path) -> None:
    try:
        path.unlink()
    except FileNotFoundError:
        return


def _is_excluded(path: Path, source_root: Path) -> bool:
    parts = path.relative_to(source_root).parts
    return any(_is_excluded_part(part) for part in parts)


def _file_hash(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _partition_id(path: str) -> str:
    return hashlib.sha1(path.encode("utf-8")).hexdigest()[:20]


def _manifest_entry(snapshot: SourceSnapshot, graph: CodeGraph) -> ManifestEntry:
    return ManifestEntry(
        path=snapshot.path,
        content_hash=snapshot.content_hash,
        language=snapshot.language or "",
        partition_id=_partition_id(snapshot.path),
        node_ids=tuple(sorted(graph.nodes)),
        edge_ids=tuple(sorted(graph.edges)),
        node_types={node_id: node.table for node_id, node in graph.nodes.items()},
        edge_types={edge_id: edge.type for edge_id, edge in graph.edges.items()},
        materialized_at=datetime.now(UTC).isoformat(),
    )


def _materialization_result(
    *,
    mode: MaterializeMode,
    snapshots: Mapping[str, SourceSnapshot],
    diagnostics: list[str],
    diff: ManifestDiff,
    manifest_path: Path,
    rebuilt_entries: Mapping[str, ManifestEntry],
    next_manifest: MaterializationManifest,
) -> MaterializationResult:
    unsupported_paths = tuple(path for path, snapshot in snapshots.items() if snapshot.language is None)
    skipped_paths = tuple(sorted((*diff.unchanged, *unsupported_paths)))
    return MaterializationResult(
        mode=mode,
        scanned=len(snapshots),
        rebuilt=len(rebuilt_entries),
        skipped=len(skipped_paths),
        deleted=len(diff.deleted),
        diagnostics=tuple(diagnostics),
        manifest_path=manifest_path.as_posix(),
        rebuilt_paths=tuple(sorted(rebuilt_entries)),
        skipped_paths=skipped_paths,
        deleted_paths=diff.deleted,
        graph_summary=_manifest_summary(next_manifest),
    )


def _retained_node_ids(manifest: MaterializationManifest, touched_paths: set[str]) -> set[str]:
    retained: set[str] = set()
    for path, entry in manifest.files.items():
        if path in touched_paths:
            continue
        retained.update(entry.node_ids)
    return retained


def _retained_edge_ids(manifest: MaterializationManifest, touched_paths: set[str]) -> set[str]:
    retained: set[str] = set()
    for path, entry in manifest.files.items():
        if path in touched_paths:
            continue
        retained.update(entry.edge_ids)
    return retained


def _manifest_summary(manifest: MaterializationManifest) -> dict[str, int | str]:
    node_ids: set[str] = set()
    edge_ids: set[str] = set()
    for entry in manifest.files.values():
        node_ids.update(entry.node_ids)
        edge_ids.update(entry.edge_ids)
    return {
        "ontology": manifest.ontology,
        "partition_count": len(manifest.files),
        "node_count": len(node_ids),
        "edge_count": len(edge_ids),
    }
