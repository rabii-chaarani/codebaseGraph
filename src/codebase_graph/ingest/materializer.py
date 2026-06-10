from __future__ import annotations

import hashlib
import json
import os
import tempfile
from collections.abc import Mapping
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Literal

from codebase_graph.core import CodeGraph
from codebase_graph.db import LadybugCodeGraphStore, create_ladybug_database
from codebase_graph.diagnostics import log_event
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
    ".cache",
    ".coverage",
    ".hypothesis",
    ".mypy_cache",
    ".nox",
    ".pyre",
    ".pytest_cache",
    ".pytype",
    ".ruff_cache",
    ".tox",
    ".vscode",
    "__pycache__",
    "build",
    "coverage",
    "dist",
    "htmlcov",
    "node_modules",
    "vendor",
    ".codebase_graph",
    DEFAULT_STATE_DIR,
}


@dataclass(frozen=True, slots=True)
class SourceSnapshot:
    """Store source snapshot data."""
    path: str
    absolute_path: Path
    content_hash: str
    language: str | None


@dataclass(frozen=True, slots=True)
class ManifestEntry:
    """Store manifest entry data."""
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
        """Convert dict.

        Args:
            payload: Payload to process.

        Returns:
            The computed result.
        """
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
        """Return a JSON-serializable dictionary representation.

        Returns:
            A dictionary containing the computed payload.
        """
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
    """Store materialization manifest data."""
    schema_version: int = MANIFEST_SCHEMA_VERSION
    ontology: str = ONTOLOGY_NAME
    parser_version: str = PARSER_VERSION
    files: Mapping[str, ManifestEntry] = field(default_factory=dict)

    @classmethod
    def empty(cls, *, parser_version: str = PARSER_VERSION) -> MaterializationManifest:
        """Return whether empty.

        Args:
            parser_version: Parser version value.

        Returns:
            The computed result.
        """
        return cls(parser_version=parser_version, files={})

    @classmethod
    def load(cls, path: Path) -> MaterializationManifest:
        """Load the operation.

        Args:
            path: The path to read or write.

        Returns:
            The computed result.
        """
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
        """Return a JSON-serializable dictionary representation.

        Returns:
            A dictionary containing the computed payload.
        """
        return {
            "schema_version": self.schema_version,
            "ontology": self.ontology,
            "parser_version": self.parser_version,
            "files": [entry.as_dict() for entry in sorted(self.files.values(), key=lambda item: item.path)],
        }

    def write(self, path: Path) -> None:
        """Write result.

        Args:
            path: The path to read or write.
        """
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = path.with_suffix(path.suffix + ".tmp")
        with tmp_path.open("w", encoding="utf-8") as handle:
            json.dump(self.as_dict(), handle, indent=2, sort_keys=True)
            handle.write("\n")
        os.replace(tmp_path, path)

    def is_compatible(self, *, parser_version: str = PARSER_VERSION) -> bool:
        """Return whether compatible.

        Args:
            parser_version: Parser version value.

        Returns:
            Whether the check succeeds.
        """
        return (
            self.schema_version == MANIFEST_SCHEMA_VERSION
            and self.ontology == ONTOLOGY_NAME
            and self.parser_version == parser_version
        )

    def diff(self, current_files: Mapping[str, SourceSnapshot], *, parser_version: str = PARSER_VERSION) -> ManifestDiff:
        """Process diff.

        Args:
            current_files: Current files value.
            parser_version: Parser version value.

        Returns:
            The computed result.
        """
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
    """Store manifest diff data."""
    added: tuple[str, ...]
    modified: tuple[str, ...]
    unchanged: tuple[str, ...]
    deleted: tuple[str, ...]
    force_rebuild: bool = False

    @property
    def rebuild_paths(self) -> tuple[str, ...]:
        """Process rebuild paths.

        Returns:
            A tuple containing the computed values.
        """
        return tuple(sorted((*self.added, *self.modified)))


@dataclass(frozen=True, slots=True)
class MaterializationResult:
    """Store the result of materialization operations."""
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

    def as_dict(self) -> dict[str, Any]:
        """Return a JSON-serializable dictionary representation.

        Returns:
            A dictionary containing the computed payload.
        """
        return {
            "mode": self.mode,
            "scanned": self.scanned,
            "rebuilt": self.rebuilt,
            "skipped": self.skipped,
            "deleted": self.deleted,
            "diagnostics": list(self.diagnostics),
            "manifest_path": self.manifest_path,
            "rebuilt_paths": list(self.rebuilt_paths),
            "skipped_paths": list(self.skipped_paths),
            "deleted_paths": list(self.deleted_paths),
            "graph_summary": dict(self.graph_summary),
        }


class GraphMaterializer:
    """Scan source files and persist their generated graph partitions."""
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
        """Initialize the instance.

        Args:
            source_root: Source root value.
            db_path: The db path to read or write.
            manifest_path: The manifest path to read or write.
            include_fts: Include fts value.
            repository_label: Repository label value.
            store: The store used by the operation.
            parser_registry: Parser registry value.
            graph_builder: Graph builder value.
        """
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
        """Return the backing graph store, creating it lazily.

        Returns:
            The open graph store used for materialization.
        """
        if self._store is None:
            self._store = create_ladybug_database(self.db_path, include_fts=self.include_fts)
        return self._store

    @store.setter
    def store(self, value: LadybugCodeGraphStore | None) -> None:
        """Process store.

        Args:
            value: Value value.
        """
        self._store = value
        self._store_injected = value is not None

    def close(self) -> None:
        """Close the owned graph store if one was opened."""
        self._close_store()

    def materialize(self, mode: MaterializeMode = "changed") -> MaterializationResult:
        """Materialize source files into the graph database.

        Args:
            mode: Whether to rebuild all files or only files changed since the manifest.

        Returns:
            Counts, diagnostics, manifest location, and graph summary for the run.
        """
        if mode not in {"full", "changed"}:
            raise ValueError(f"Unsupported materialization mode: {mode}")

        previous_manifest = self._read_manifest()
        snapshots, diagnostics = self._scan_source_files()
        supported = {path: snapshot for path, snapshot in snapshots.items() if snapshot.language is not None}
        force_atomic_recovery = self._should_force_atomic_recovery()

        # Full rebuilds and crash recovery prefer an atomic database swap so a
        # failed run does not leave a partially deleted persistent graph behind.
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
            elif self._can_atomic_rebuild() and _diff_has_changes(diff):
                # Persistent stores are rebuilt atomically even for changed mode
                # because partition-level deletion can otherwise expose a mixed graph.
                return self._materialize_full_atomic(
                    mode=mode,
                    snapshots=snapshots,
                    diagnostics=diagnostics,
                    supported=supported,
                    diff=ManifestDiff(
                        added=tuple(sorted(supported)),
                        modified=(),
                        unchanged=(),
                        deleted=tuple(sorted(previous_manifest.files)),
                        force_rebuild=True,
                    ),
                )
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
        """Materialize full atomic.

        Args:
            mode: Mode value.
            snapshots: Snapshots value.
            diagnostics: Diagnostics value.
            supported: Supported value.
            diff: Diff value.

        Returns:
            The computed result.
        """
        target_db_path = _filesystem_db_path(self.db_path)
        lock_fd, lock_path = _acquire_materialization_lock(target_db_path)
        try:
            rebuilt_entries: dict[str, ManifestEntry] = {}
            rebuilt_graphs: dict[str, CodeGraph] = {}
            for path in diff.rebuild_paths:
                snapshot = supported[path]
                graph = self._build_graph(snapshot)
                rebuilt_graphs[path] = graph
                rebuilt_entries[path] = _manifest_entry(snapshot, graph)

            next_manifest = MaterializationManifest(parser_version=self.parser_version, files=rebuilt_entries)
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
                _unlink_db_sidecars(target_db_path)
                os.replace(temp_db_path, target_db_path)
                os.replace(temp_manifest_path, self.manifest_path)
                _unlink_db_sidecars(target_db_path)
                _unlink_if_exists(marker_path)
                self._store = None
            except Exception:
                if temp_store is not None:
                    temp_store.close()
                _unlink_if_exists(temp_db_path)
                _unlink_db_sidecars(temp_db_path)
                _unlink_if_exists(temp_manifest_path)
                _unlink_if_exists(temp_manifest_path.with_suffix(temp_manifest_path.suffix + ".tmp"))
                raise
        finally:
            _release_materialization_lock(lock_fd, lock_path)

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
        """Read manifest.

        Returns:
            The computed result.
        """
        if self._store_injected and self._store is not None and hasattr(self._store, "read_manifest"):
            return self._store.read_manifest(self.manifest_path)
        return MaterializationManifest.load(self.manifest_path)

    def _write_manifest(self, manifest: MaterializationManifest) -> None:
        """Write manifest.

        Args:
            manifest: Manifest value.
        """
        if self._store_injected and self._store is not None and hasattr(self._store, "write_manifest"):
            self._store.write_manifest(manifest, self.manifest_path)
            return
        manifest.write(self.manifest_path)

    def _can_atomic_rebuild(self) -> bool:
        """Process can atomic rebuild.

        Returns:
            Whether the check succeeds.
        """
        return not self._store_injected and not _is_memory_db_path(self.db_path)

    def _should_force_atomic_recovery(self) -> bool:
        """Process should force atomic recovery.

        Returns:
            Whether the check succeeds.
        """
        return self._can_atomic_rebuild() and self._rebuild_marker_path.exists()

    @property
    def _rebuild_marker_path(self) -> Path:
        """Process rebuild marker path.

        Returns:
            The computed result.
        """
        return self.manifest_path.with_suffix(self.manifest_path.suffix + ".rebuild-pending")

    def _close_store(self) -> None:
        """Close store."""
        if self._store is None:
            return
        close = getattr(self._store, "close", None)
        if callable(close):
            close()
        self._store = None

    def _scan_source_files(self) -> tuple[dict[str, SourceSnapshot], list[str]]:
        """Scan source files.

        Returns:
            A tuple containing the computed values.
        """
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
                if language is None:
                    snapshots[relative_path] = SourceSnapshot(
                        path=relative_path,
                        absolute_path=path,
                        content_hash="",
                        language=None,
                    )
                    diagnostics.append(f"Skipped unsupported file: {relative_path}")
                    continue
                snapshots[relative_path] = SourceSnapshot(
                    path=relative_path,
                    absolute_path=path,
                    content_hash=_file_hash(path),
                    language=language,
                )
        return snapshots, diagnostics

    def _build_graph(self, snapshot: SourceSnapshot) -> CodeGraph:
        """Build graph.

        Args:
            snapshot: Snapshot value.

        Returns:
            The computed result.
        """
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
    """Return whether excluded part.

    Args:
        part: Part value.

    Returns:
        Whether the check succeeds.
    """
    return part in EXCLUDED_PARTS or part.endswith(".egg-info")


def _normalize_db_path(db_path: str | Path) -> str | Path:
    """Normalize DB path.

    Args:
        db_path: The db path to read or write.

    Returns:
        The computed result.
    """
    if _is_memory_db_path(db_path):
        return ":memory:"
    return Path(db_path)


def _is_memory_db_path(db_path: str | Path) -> bool:
    """Return whether memory db path.

    Args:
        db_path: The db path to read or write.

    Returns:
        Whether the check succeeds.
    """
    return str(db_path) == ":memory:"


def _filesystem_db_path(db_path: str | Path) -> Path:
    """Process filesystem DB path.

    Args:
        db_path: The db path to read or write.

    Returns:
        The computed result.
    """
    if _is_memory_db_path(db_path):
        raise ValueError("In-memory databases do not have a filesystem path")
    return Path(db_path)


def _temporary_sibling(path: Path, *, suffix: str) -> Path:
    """Create temporary sibling.

    Args:
        path: The path to read or write.
        suffix: Suffix value.

    Returns:
        The computed result.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temp_path = tempfile.mkstemp(prefix=f".{path.name}.", suffix=suffix, dir=path.parent)
    os.close(descriptor)
    os.unlink(temp_path)
    return Path(temp_path)


def _acquire_materialization_lock(db_path: Path) -> tuple[int, Path]:
    """Process acquire materialization lock.

    Args:
        db_path: The db path to read or write.

    Returns:
        A tuple containing the computed values.
    """
    lock_path = Path(f"{db_path}.lock")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    while True:
        try:
            descriptor = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            break
        except FileExistsError as exc:
            if _materialization_lock_is_stale(lock_path):
                _unlink_if_exists(lock_path)
                log_event(
                    "materializer.stale_lock_removed",
                    level="WARNING",
                    db_path=db_path.as_posix(),
                    lock_path=lock_path.as_posix(),
                )
                continue
            log_event(
                "materializer.lock_exists",
                level="WARNING",
                db_path=db_path.as_posix(),
                lock_path=lock_path.as_posix(),
            )
            raise RuntimeError(
                f"codebaseGraph materialization is already in progress for {db_path}. "
                f"If no materializer is running, inspect the lock file before removing it: {lock_path}"
            ) from exc
    payload = {
        "created_at": datetime.now(timezone.utc).isoformat(),
        "pid": os.getpid(),
        "db_path": db_path.as_posix(),
    }
    try:
        os.write(descriptor, (json.dumps(payload, sort_keys=True) + "\n").encode("utf-8"))
    except Exception:
        os.close(descriptor)
        _unlink_if_exists(lock_path)
        raise
    return descriptor, lock_path


def _materialization_lock_is_stale(lock_path: Path) -> bool:
    """Return materialization lock is stale.

    Args:
        lock_path: The lock path to read or write.

    Returns:
        Whether the check succeeds.
    """
    try:
        payload = json.loads(lock_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False
    pid = payload.get("pid") if isinstance(payload, dict) else None
    if not isinstance(pid, int) or pid <= 0 or pid == os.getpid():
        return False
    return not _process_is_running(pid)


def _process_is_running(pid: int) -> bool:
    """Process process is running.

    Args:
        pid: Pid value.

    Returns:
        Whether the check succeeds.
    """
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _release_materialization_lock(descriptor: int, lock_path: Path) -> None:
    """Process release materialization lock.

    Args:
        descriptor: The descriptor used by the operation.
        lock_path: The lock path to read or write.
    """
    os.close(descriptor)
    _unlink_if_exists(lock_path)


def _write_rebuild_marker(marker_path: Path, db_path: Path, manifest_path: Path) -> None:
    """Write rebuild marker.

    Args:
        marker_path: The marker path to read or write.
        db_path: The db path to read or write.
        manifest_path: The manifest path to read or write.
    """
    marker_path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = marker_path.with_suffix(marker_path.suffix + ".tmp")
    with tmp_path.open("w", encoding="utf-8") as handle:
        json.dump(
            {
                "created_at": datetime.now(timezone.utc).isoformat(),
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
    """Unlink if exists.

    Args:
        path: The path to read or write.
    """
    try:
        path.unlink()
    except FileNotFoundError:
        return


def _unlink_db_sidecars(db_path: Path) -> None:
    """Unlink DB sidecars.

    Args:
        db_path: The db path to read or write.
    """
    for suffix in (".wal", ".shm", ".shadow"):
        _unlink_if_exists(Path(f"{db_path}{suffix}"))


def _diff_has_changes(diff: ManifestDiff) -> bool:
    """Process diff has changes.

    Args:
        diff: Diff value.

    Returns:
        Whether the check succeeds.
    """
    return bool(diff.rebuild_paths or diff.deleted)


def _is_excluded(path: Path, source_root: Path) -> bool:
    """Return whether excluded.

    Args:
        path: The path to read or write.
        source_root: Source root value.

    Returns:
        Whether the check succeeds.
    """
    parts = path.relative_to(source_root).parts
    return any(_is_excluded_part(part) for part in parts)


def _file_hash(path: Path) -> str:
    """Return hash file data.

    Args:
        path: The path to read or write.

    Returns:
        The computed string.
    """
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _partition_id(path: str) -> str:
    """Process partition ID.

    Args:
        path: The path to read or write.

    Returns:
        The computed string.
    """
    return hashlib.sha1(path.encode("utf-8")).hexdigest()[:20]


def _manifest_entry(snapshot: SourceSnapshot, graph: CodeGraph) -> ManifestEntry:
    """Process manifest entry.

    Args:
        snapshot: Snapshot value.
        graph: Graph value.

    Returns:
        The computed result.
    """
    return ManifestEntry(
        path=snapshot.path,
        content_hash=snapshot.content_hash,
        language=snapshot.language or "",
        partition_id=_partition_id(snapshot.path),
        node_ids=tuple(sorted(graph.nodes)),
        edge_ids=tuple(sorted(graph.edges)),
        node_types={node_id: node.table for node_id, node in graph.nodes.items()},
        edge_types={edge_id: edge.type for edge_id, edge in graph.edges.items()},
        materialized_at=datetime.now(timezone.utc).isoformat(),
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
    """Return materialization result.

    Args:
        mode: Mode value.
        snapshots: Snapshots value.
        diagnostics: Diagnostics value.
        diff: Diff value.
        manifest_path: The manifest path to read or write.
        rebuilt_entries: Rebuilt entries value.
        next_manifest: Next manifest value.

    Returns:
        The computed result.
    """
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
    """Return retained node ids.

    Args:
        manifest: Manifest value.
        touched_paths: Touched paths value.

    Returns:
        The computed result.
    """
    retained: set[str] = set()
    for path, entry in manifest.files.items():
        if path in touched_paths:
            continue
        retained.update(entry.node_ids)
    return retained


def _retained_edge_ids(manifest: MaterializationManifest, touched_paths: set[str]) -> set[str]:
    """Return retained edge ids.

    Args:
        manifest: Manifest value.
        touched_paths: Touched paths value.

    Returns:
        The computed result.
    """
    retained: set[str] = set()
    for path, entry in manifest.files.items():
        if path in touched_paths:
            continue
        retained.update(entry.edge_ids)
    return retained


def _manifest_summary(manifest: MaterializationManifest) -> dict[str, int | str]:
    """Process manifest summary.

    Args:
        manifest: Manifest value.

    Returns:
        A dictionary containing the computed payload.
    """
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
