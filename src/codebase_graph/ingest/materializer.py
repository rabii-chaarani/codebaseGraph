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
    """Represent source snapshot data used by source scanning and graph materialization."""
    path: str
    absolute_path: Path
    content_hash: str
    language: str | None


@dataclass(frozen=True, slots=True)
class ManifestEntry:
    """Represent manifest entry data used by source scanning and graph materialization.

    The class belongs to Materialization workflow that scans source files, diffs manifests, and
    persists graph partitions.
    """
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
        """Manage dict within source scanning and graph materialization.

        Args:
            payload: Structured payload being normalized or serialized.

        Returns:
            ManifestEntry instance populated with data from the source scanning and graph
            materialization workflow.
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
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the source scanning and graph
            materialization response contract.
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
    """Represent materialization manifest data used by source scanning and graph materialization.

    The class belongs to Materialization workflow that scans source files, diffs manifests, and
    persists graph partitions.
    """
    schema_version: int = MANIFEST_SCHEMA_VERSION
    ontology: str = ONTOLOGY_NAME
    parser_version: str = PARSER_VERSION
    files: Mapping[str, ManifestEntry] = field(default_factory=dict)

    @classmethod
    def empty(cls, *, parser_version: str = PARSER_VERSION) -> MaterializationManifest:
        """Manage source scanning and graph materialization state.

        Args:
            parser_version: Parser version used by the source scanning and graph
            materialization workflow.

        Returns:
            MaterializationManifest instance populated with data from the source scanning
            and graph materialization workflow.
        """
        return cls(parser_version=parser_version, files={})

    @classmethod
    def load(cls, path: Path) -> MaterializationManifest:
        """Load source scanning and graph materialization for source scanning and graph materialization.

        Args:
            path: Filesystem path read from or written by this operation.

        Returns:
            MaterializationManifest instance populated with data from the source scanning
            and graph materialization workflow.
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
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the source scanning and graph
            materialization response contract.
        """
        return {
            "schema_version": self.schema_version,
            "ontology": self.ontology,
            "parser_version": self.parser_version,
            "files": [entry.as_dict() for entry in sorted(self.files.values(), key=lambda item: item.path)],
        }

    def write(self, path: Path) -> None:
        """Write source scanning and graph materialization for source scanning and graph materialization.

        This writes to disk and should leave complete files on success.

        Args:
            path: Filesystem path read from or written by this operation.
        """
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = path.with_suffix(path.suffix + ".tmp")
        with tmp_path.open("w", encoding="utf-8") as handle:
            json.dump(self.as_dict(), handle, indent=2, sort_keys=True)
            handle.write("\n")
        os.replace(tmp_path, path)

    def is_compatible(self, *, parser_version: str = PARSER_VERSION) -> bool:
        """Return whether compatible for source scanning and graph materialization.

        Args:
            parser_version: Parser version used by the source scanning and graph
            materialization workflow.

        Returns:
            True when the requested condition is satisfied; otherwise False.
        """
        return (
            self.schema_version == MANIFEST_SCHEMA_VERSION
            and self.ontology == ONTOLOGY_NAME
            and self.parser_version == parser_version
        )

    def diff(self, current_files: Mapping[str, SourceSnapshot], *, parser_version: str = PARSER_VERSION) -> ManifestDiff:
        """Manage source scanning and graph materialization state.

        Args:
            current_files: Current files used by the source scanning and graph
            materialization workflow.
            parser_version: Parser version used by the source scanning and graph
            materialization workflow.

        Returns:
            ManifestDiff instance populated with data from the source scanning and graph
            materialization workflow.
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
    """Represent manifest diff data used by source scanning and graph materialization.

    The class belongs to Materialization workflow that scans source files, diffs manifests, and
    persists graph partitions.
    """
    added: tuple[str, ...]
    modified: tuple[str, ...]
    unchanged: tuple[str, ...]
    deleted: tuple[str, ...]
    force_rebuild: bool = False

    @property
    def rebuild_paths(self) -> tuple[str, ...]:
        """Manage paths within source scanning and graph materialization.

        Returns:
            Tuple of stable results returned to the source scanning and graph
            materialization caller.
        """
        return tuple(sorted((*self.added, *self.modified)))


@dataclass(frozen=True, slots=True)
class MaterializationResult:
    """Carry the observable outcome of materialization workflows.

    The class belongs to Materialization workflow that scans source files, diffs manifests, and
    persists graph partitions.
    """
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
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the source scanning and graph
            materialization response contract.
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
    """Manage source scanning, parser execution, manifest diffing, and database writes.

    The class belongs to Materialization workflow that scans source files, diffs manifests, and
    persists graph partitions.
    """
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
        """Initialize graph materializer with the collaborators and state it owns.

        Args:
            source_root: Root directory scanned for source files.
            db_path: Ladybug database path, or an in-memory database marker.
            manifest_path: Manifest path used to track previously materialized file
            partitions.
            include_fts: Include full-text search used by the source scanning and graph
            materialization workflow.
            repository_label: Repository label used by the source scanning and graph
            materialization workflow.
            store: Graph store used for persistence or read-only queries.
            parser_registry: Parser registry used by the source scanning and graph
            materialization workflow.
            graph_builder: Graph builder used by the source scanning and graph
            materialization workflow.
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
        """Return the database store used by this materializer.

        The store is opened lazily so dry-run setup paths and tests can construct
        a materializer without touching Ladybug until graph data must be read or
        written.

        Returns:
            Open graph store bound to the configured database path.
        """
        if self._store is None:
            self._store = create_ladybug_database(self.db_path, include_fts=self.include_fts)
        return self._store

    @store.setter
    def store(self, value: LadybugCodeGraphStore | None) -> None:
        """Inject or clear the database store used by this materializer.

        Args:
            value: Store supplied by tests or callers that manage the store lifecycle.
        """
        self._store = value
        self._store_injected = value is not None

    def close(self) -> None:
        """Close the owned database store when the materializer opened it."""
        self._close_store()

    def materialize(self, mode: MaterializeMode = "changed") -> MaterializationResult:
        """Synchronize source files, manifest state, and the Ladybug graph database.

        This may rebuild the graph database and update the manifest.

        Args:
            mode: Materialization mode selected by the caller.

        Returns:
            MaterializationResult instance populated with data from the source scanning and
            graph materialization workflow.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
        if mode not in {"full", "changed"}:
            raise ValueError(f"Unsupported materialization mode: {mode}")

        previous_manifest = self._read_manifest()
        snapshots, diagnostics = self._scan_source_files()
        supported = {path: snapshot for path, snapshot in snapshots.items() if snapshot.language is not None}
        force_atomic_recovery = self._should_force_atomic_recovery()

        # The manifest is the authority for which graph rows belong to each file.
        # Unsupported files are reported in diagnostics but excluded from diffing
        # so they never create empty graph partitions.
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
                # Imports, dependencies, and shared support nodes can be referenced
                # from untouched files. The retained ID sets keep those shared rows
                # alive while changed/deleted partitions are replaced.
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
        """Rebuild a persistent database beside the current one and swap it into place after success.

        This may rebuild the graph database and update the manifest.

        Args:
            mode: Materialization mode selected by the caller.
            snapshots: Current source snapshots keyed by repository-relative path.
            diagnostics: Warnings collected while scanning or parsing source files.
            supported: Snapshots whose language has a registered parser.
            diff: Manifest diff describing added, modified, unchanged, and deleted files.

        Returns:
            MaterializationResult instance populated with data from the source scanning and
            graph materialization workflow.

        Raises:
            Exception: Raised when validation or runtime preconditions fail.
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
        """Read manifest for source scanning and graph materialization.

        Returns:
            MaterializationManifest instance populated with data from the source scanning
            and graph materialization workflow.
        """
        if self._store_injected and self._store is not None and hasattr(self._store, "read_manifest"):
            return self._store.read_manifest(self.manifest_path)
        return MaterializationManifest.load(self.manifest_path)

    def _write_manifest(self, manifest: MaterializationManifest) -> None:
        """Write manifest for source scanning and graph materialization.

        This writes to disk and should leave complete files on success.

        Args:
            manifest: Materialization manifest whose partition metadata is being inspected.
        """
        if self._store_injected and self._store is not None and hasattr(self._store, "write_manifest"):
            self._store.write_manifest(manifest, self.manifest_path)
            return
        manifest.write(self.manifest_path)

    def _can_atomic_rebuild(self) -> bool:
        """Manage atomic rebuild within source scanning and graph materialization.

        Returns:
            True when the requested condition is satisfied; otherwise False.
        """
        return not self._store_injected and not _is_memory_db_path(self.db_path)

    def _should_force_atomic_recovery(self) -> bool:
        """Manage force atomic recovery within source scanning and graph materialization.

        Returns:
            True when the requested condition is satisfied; otherwise False.
        """
        return self._can_atomic_rebuild() and self._rebuild_marker_path.exists()

    @property
    def _rebuild_marker_path(self) -> Path:
        """Manage marker path within source scanning and graph materialization.

        Returns:
            Path instance populated with data from the source scanning and graph
            materialization workflow.
        """
        return self.manifest_path.with_suffix(self.manifest_path.suffix + ".rebuild-pending")

    def _close_store(self) -> None:
        """Close store for source scanning and graph materialization."""
        if self._store is None:
            return
        close = getattr(self._store, "close", None)
        if callable(close):
            close()
        self._store = None

    def _scan_source_files(self) -> tuple[dict[str, SourceSnapshot], list[str]]:
        """Scan source files for source scanning and graph materialization.

        Returns:
            Structured mapping that follows the source scanning and graph
            materialization response contract.
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
        """Build graph for source scanning and graph materialization.

        Args:
            snapshot: Current source file snapshot with path, hash, and language.

        Returns:
            CodeGraph instance populated with data from the source scanning and graph
            materialization workflow.

        Raises:
            Exception: Raised when validation or runtime preconditions fail.
            ValueError: Raised when validation or runtime preconditions fail.
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
    """Return whether excluded part for source scanning and graph materialization.

    Args:
        part: Part used by the source scanning and graph materialization workflow.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    return part in EXCLUDED_PARTS or part.endswith(".egg-info")


def _normalize_db_path(db_path: str | Path) -> str | Path:
    """Normalize database path for source scanning and graph materialization.

    Args:
        db_path: Ladybug database path, or an in-memory database marker.

    Returns:
        str | Path instance populated with data from the source scanning and graph
        materialization workflow.
    """
    if _is_memory_db_path(db_path):
        return ":memory:"
    return Path(db_path)


def _is_memory_db_path(db_path: str | Path) -> bool:
    """Return whether memory database path for source scanning and graph materialization.

    Args:
        db_path: Ladybug database path, or an in-memory database marker.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    return str(db_path) == ":memory:"


def _filesystem_db_path(db_path: str | Path) -> Path:
    """Manage database path within source scanning and graph materialization.

    Args:
        db_path: Ladybug database path, or an in-memory database marker.

    Returns:
        Path instance populated with data from the source scanning and graph materialization
        workflow.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    if _is_memory_db_path(db_path):
        raise ValueError("In-memory databases do not have a filesystem path")
    return Path(db_path)


def _temporary_sibling(path: Path, *, suffix: str) -> Path:
    """Create sibling for source scanning and graph materialization.

    Args:
        path: Filesystem path read from or written by this operation.
        suffix: Suffix used by the source scanning and graph materialization workflow.

    Returns:
        Path instance populated with data from the source scanning and graph materialization
        workflow.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temp_path = tempfile.mkstemp(prefix=f".{path.name}.", suffix=suffix, dir=path.parent)
    os.close(descriptor)
    os.unlink(temp_path)
    return Path(temp_path)


def _acquire_materialization_lock(db_path: Path) -> tuple[int, Path]:
    """Create or replace a materialization lock after checking for active or stale owners.

    Args:
        db_path: Ladybug database path, or an in-memory database marker.

    Returns:
        Tuple of stable results returned to the source scanning and graph materialization
        caller.

    Raises:
        Exception: Raised when validation or runtime preconditions fail.
        RuntimeError: Raised when validation or runtime preconditions fail.
    """
    lock_path = Path(f"{db_path}.lock")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    while True:
        try:
            # O_EXCL makes lock acquisition atomic across concurrent setup
            # processes targeting the same persistent database.
            descriptor = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            break
        except FileExistsError as exc:
            if _materialization_lock_is_stale(lock_path):
                # A stale lock means the recorded process is gone or the lock
                # cannot be parsed; removing it lets crash recovery continue.
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
    """Manage lock is stale within source scanning and graph materialization.

    Args:
        lock_path: Lock file path guarding graph materialization.

    Returns:
        True when the requested condition is satisfied; otherwise False.
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
    """Manage is running within source scanning and graph materialization.

    This executes the selected workflow and returns a process status code or result object.

    Args:
        pid: Operating-system identifier read from a materialization lock file.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _release_materialization_lock(descriptor: int, lock_path: Path) -> None:
    """Manage materialization lock within source scanning and graph materialization.

    Args:
        descriptor: MCP server descriptor that will be rendered into client configuration.
        lock_path: Lock file path guarding graph materialization.
    """
    os.close(descriptor)
    _unlink_if_exists(lock_path)


def _write_rebuild_marker(marker_path: Path, db_path: Path, manifest_path: Path) -> None:
    """Write rebuild marker for source scanning and graph materialization.

    This writes to disk and should leave complete files on success.

    Args:
        marker_path: Filesystem path for the marker resource.
        db_path: Ladybug database path, or an in-memory database marker.
        manifest_path: Manifest path used to track previously materialized file partitions.
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
    """Remove if exists for source scanning and graph materialization.

    Args:
        path: Filesystem path read from or written by this operation.
    """
    try:
        path.unlink()
    except FileNotFoundError:
        return


def _unlink_db_sidecars(db_path: Path) -> None:
    """Remove database sidecars for source scanning and graph materialization.

    Args:
        db_path: Ladybug database path, or an in-memory database marker.
    """
    for suffix in (".wal", ".shm", ".shadow"):
        _unlink_if_exists(Path(f"{db_path}{suffix}"))


def _diff_has_changes(diff: ManifestDiff) -> bool:
    """Manage has changes within source scanning and graph materialization.

    Args:
        diff: Manifest diff describing added, modified, unchanged, and deleted files.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    return bool(diff.rebuild_paths or diff.deleted)


def _is_excluded(path: Path, source_root: Path) -> bool:
    """Return whether excluded for source scanning and graph materialization.

    Args:
        path: Filesystem path read from or written by this operation.
        source_root: Root directory scanned for source files.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    parts = path.relative_to(source_root).parts
    return any(_is_excluded_part(part) for part in parts)


def _file_hash(path: Path) -> str:
    """Manage hash within source scanning and graph materialization.

    Args:
        path: Filesystem path read from or written by this operation.

    Returns:
        Formatted text returned to the caller.
    """
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _partition_id(path: str) -> str:
    """Manage identifier within source scanning and graph materialization.

    Args:
        path: Filesystem path read from or written by this operation.

    Returns:
        Formatted text returned to the caller.
    """
    return hashlib.sha1(path.encode("utf-8")).hexdigest()[:20]


def _manifest_entry(snapshot: SourceSnapshot, graph: CodeGraph) -> ManifestEntry:
    """Manage entry within source scanning and graph materialization.

    Args:
        snapshot: Current source file snapshot with path, hash, and language.
        graph: In-memory graph whose nodes and edges are being persisted or summarized.

    Returns:
        ManifestEntry instance populated with data from the source scanning and graph
        materialization workflow.
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
    """Manage result within source scanning and graph materialization.

    Args:
        mode: Materialization mode selected by the caller.
        snapshots: Current source snapshots keyed by repository-relative path.
        diagnostics: Warnings collected while scanning or parsing source files.
        diff: Manifest diff describing added, modified, unchanged, and deleted files.
        manifest_path: Manifest path used to track previously materialized file partitions.
        rebuilt_entries: Rebuilt entries used by the source scanning and graph
        materialization workflow.
        next_manifest: Next manifest used by the source scanning and graph
        materialization workflow.

    Returns:
        MaterializationResult instance populated with data from the source scanning and
        graph materialization workflow.
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
    """Return node identifiers for source scanning and graph materialization.

    Args:
        manifest: Materialization manifest whose partition metadata is being inspected.
        touched_paths: Touched paths used by the source scanning and graph
        materialization workflow.

    Returns:
        set[str] instance populated with data from the source scanning and graph
        materialization workflow.
    """
    retained: set[str] = set()
    for path, entry in manifest.files.items():
        if path in touched_paths:
            continue
        retained.update(entry.node_ids)
    return retained


def _retained_edge_ids(manifest: MaterializationManifest, touched_paths: set[str]) -> set[str]:
    """Return edge identifiers for source scanning and graph materialization.

    Args:
        manifest: Materialization manifest whose partition metadata is being inspected.
        touched_paths: Touched paths used by the source scanning and graph
        materialization workflow.

    Returns:
        set[str] instance populated with data from the source scanning and graph
        materialization workflow.
    """
    retained: set[str] = set()
    for path, entry in manifest.files.items():
        if path in touched_paths:
            continue
        retained.update(entry.edge_ids)
    return retained


def _manifest_summary(manifest: MaterializationManifest) -> dict[str, int | str]:
    """Manage summary within source scanning and graph materialization.

    Args:
        manifest: Materialization manifest whose partition metadata is being inspected.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
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
