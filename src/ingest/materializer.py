from __future__ import annotations

import hashlib
import json
import os
from collections.abc import Mapping
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Literal

from core import CodeGraph
from db import LadybugCodeGraphStore, create_ladybug_database
from extract import GraphBuilder
from ontology import ONTOLOGY_NAME

from .tree_sitter_parser import ParserUnavailableError, parser_for_language

MaterializeMode = Literal["full", "changed"]

MANIFEST_SCHEMA_VERSION = 1
DEFAULT_STATE_DIR = ".codebase_graph"
DEFAULT_MANIFEST_NAME = "manifest.json"
DEFAULT_DB_NAME = "graph.lbug"
PARSER_VERSION = "tree-sitter-python-v1"
SUPPORTED_SUFFIXES = {".py": "python"}
EXCLUDED_PARTS = {
    ".git",
    ".venv",
    "__pycache__",
    ".pytest_cache",
    ".ruff_cache",
    "build",
    "dist",
    ".codebase_graph",
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
    def empty(cls) -> MaterializationManifest:
        return cls(files={})

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

    def is_compatible(self) -> bool:
        return (
            self.schema_version == MANIFEST_SCHEMA_VERSION
            and self.ontology == ONTOLOGY_NAME
            and self.parser_version == PARSER_VERSION
        )

    def diff(self, current_files: Mapping[str, SourceSnapshot]) -> ManifestDiff:
        if not self.is_compatible():
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
    ) -> None:
        self.source_root = Path(source_root).resolve()
        self.state_dir = self.source_root / DEFAULT_STATE_DIR
        self.db_path = db_path if db_path is not None else self.state_dir / DEFAULT_DB_NAME
        self.manifest_path = Path(manifest_path) if manifest_path is not None else self.state_dir / DEFAULT_MANIFEST_NAME
        self.include_fts = include_fts
        self.repository_label = repository_label or self.source_root.name or "repository"
        self.store = store or create_ladybug_database(self.db_path, include_fts=include_fts)
        self.builder = GraphBuilder(repository_label=self.repository_label, source_root=self.source_root)

    def materialize(self, mode: MaterializeMode = "changed") -> MaterializationResult:
        if mode not in {"full", "changed"}:
            raise ValueError(f"Unsupported materialization mode: {mode}")

        previous_manifest = self.store.read_manifest(self.manifest_path)
        snapshots, diagnostics = self._scan_source_files()
        supported = {path: snapshot for path, snapshot in snapshots.items() if snapshot.language is not None}

        if mode == "full":
            diff = ManifestDiff(
                added=tuple(sorted(supported)),
                modified=(),
                unchanged=(),
                deleted=tuple(sorted(previous_manifest.files)),
                force_rebuild=True,
            )
            self.store.clear_graph()
            retained_node_ids: set[str] = set()
        else:
            diff = previous_manifest.diff(supported)
            if diff.force_rebuild:
                self.store.clear_graph()
                retained_node_ids = set()
            else:
                retained_node_ids = _retained_node_ids(previous_manifest, set(diff.rebuild_paths) | set(diff.deleted))
                for path in diff.deleted:
                    self.store.delete_partition(
                        path,
                        manifest_entry=previous_manifest.files.get(path),
                        retained_node_ids=retained_node_ids,
                    )

        rebuilt_entries: dict[str, ManifestEntry] = {}
        for path in diff.rebuild_paths:
            snapshot = supported[path]
            previous_entry = None if diff.force_rebuild else previous_manifest.files.get(path)
            graph = self._build_graph(snapshot)
            self.store.replace_partition(
                path,
                graph,
                previous_entry=previous_entry,
                retained_node_ids=retained_node_ids,
            )
            rebuilt_entries[path] = _manifest_entry(snapshot, graph)

        next_files = {
            path: entry
            for path, entry in previous_manifest.files.items()
            if path not in set(diff.deleted) | set(diff.rebuild_paths)
        }
        next_files.update(rebuilt_entries)
        next_manifest = MaterializationManifest(files=next_files)
        self.store.write_manifest(next_manifest, self.manifest_path)

        unsupported_paths = tuple(path for path, snapshot in snapshots.items() if snapshot.language is None)
        skipped_paths = tuple(sorted((*diff.unchanged, *unsupported_paths)))
        return MaterializationResult(
            mode=mode,
            scanned=len(snapshots),
            rebuilt=len(rebuilt_entries),
            skipped=len(skipped_paths),
            deleted=len(diff.deleted),
            diagnostics=tuple(diagnostics),
            manifest_path=self.manifest_path.as_posix(),
            rebuilt_paths=tuple(sorted(rebuilt_entries)),
            skipped_paths=skipped_paths,
            deleted_paths=diff.deleted,
            graph_summary=_manifest_summary(next_manifest),
        )

    def _scan_source_files(self) -> tuple[dict[str, SourceSnapshot], list[str]]:
        snapshots: dict[str, SourceSnapshot] = {}
        diagnostics: list[str] = []
        for path in sorted(self.source_root.rglob("*")):
            if not path.is_file() or _is_excluded(path, self.source_root):
                continue
            relative_path = path.relative_to(self.source_root).as_posix()
            language = SUPPORTED_SUFFIXES.get(path.suffix)
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
            parser = parser_for_language(snapshot.language)
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


def _is_excluded(path: Path, source_root: Path) -> bool:
    parts = path.relative_to(source_root).parts
    return any(part in EXCLUDED_PARTS or part.endswith(".egg-info") for part in parts)


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


def _retained_node_ids(manifest: MaterializationManifest, touched_paths: set[str]) -> set[str]:
    retained: set[str] = set()
    for path, entry in manifest.files.items():
        if path in touched_paths:
            continue
        retained.update(entry.node_ids)
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
