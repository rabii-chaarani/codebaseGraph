from __future__ import annotations

import csv
import json
import tempfile
from collections import defaultdict
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from codebase_graph.core import CodeGraph
from codebase_graph.ontology import NODE_TYPES, RELATION_TYPES

from .schema import build_ladybug_schema, build_ladybug_schema_statements, quote_identifier


class LadybugUnavailableError(RuntimeError):
    pass


@dataclass(frozen=True, slots=True)
class BulkLoadStats:
    node_rows: int = 0
    edge_rows: int = 0
    connector_rows: int = 0
    copy_calls: int = 0


class LadybugCodeGraphStore:
    def __init__(self, db_path: str | Path = ":memory:", *, include_fts: bool = True) -> None:
        self.db_path = db_path
        self.include_fts = include_fts
        try:
            import real_ladybug as lb
        except ImportError as exc:
            raise LadybugUnavailableError(
                "LadyBugDB Python bindings are required for codebaseGraph. "
                "Install a valid `codebase-graph` runtime with `real_ladybug` available."
            ) from exc

        self._lb = lb
        if str(db_path) != ":memory:":
            Path(db_path).parent.mkdir(parents=True, exist_ok=True)
        self.db = lb.Database(str(db_path))
        self.conn = lb.Connection(self.db)

    @property
    def schema_sql(self) -> str:
        return build_ladybug_schema(include_fts=self.include_fts)

    def ensure_schema(self) -> None:
        for statement in build_ladybug_schema_statements(include_fts=self.include_fts):
            self._execute_ignoring_existing(statement)

    def execute(self, statement: str, parameters: dict[str, Any] | None = None) -> Any:
        if parameters is None:
            return self.conn.execute(statement)
        return self.conn.execute(statement, parameters)

    def close(self) -> None:
        self.conn.close()
        self.db.close()

    def __enter__(self) -> LadybugCodeGraphStore:
        return self

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.close()

    def clear_graph(self) -> None:
        for relation_type in RELATION_TYPES:
            self._execute_ignoring_missing(f"MATCH ()-[r:{quote_identifier(f'FROM_{relation_type.name}')}]->() DELETE r")
            self._execute_ignoring_missing(f"MATCH ()-[r:{quote_identifier(f'TO_{relation_type.name}')}]->() DELETE r")
        for relation_type in RELATION_TYPES:
            self._execute_ignoring_missing(f"MATCH (n:{quote_identifier(relation_type.name)}) DELETE n")
        for node_type in NODE_TYPES:
            self._execute_ignoring_missing(f"MATCH (n:{quote_identifier(node_type.name)}) DELETE n")

    def replace_partition(
        self,
        path: str,
        graph: CodeGraph,
        *,
        previous_entry: Mapping[str, Any] | Any | None = None,
        retained_node_ids: set[str] | None = None,
        retained_edge_ids: set[str] | None = None,
    ) -> None:
        if previous_entry is not None:
            self.delete_partition(
                path,
                manifest_entry=previous_entry,
                retained_node_ids=retained_node_ids,
                retained_edge_ids=retained_edge_ids,
            )

        self.insert_graphs_bulk(
            [graph],
            skip_node_ids=retained_node_ids,
            skip_edge_ids=retained_edge_ids,
        )

    def insert_graphs_bulk(
        self,
        graphs: list[CodeGraph] | tuple[CodeGraph, ...],
        *,
        skip_node_ids: set[str] | None = None,
        skip_edge_ids: set[str] | None = None,
    ) -> BulkLoadStats:
        staging_tables = _build_bulk_staging_tables(
            graphs,
            skip_node_ids=skip_node_ids,
            skip_edge_ids=skip_edge_ids,
        )
        if staging_tables.is_empty:
            return BulkLoadStats()

        with tempfile.TemporaryDirectory(prefix="codebase-graph-ladybug-") as staging_dir:
            staging = staging_tables.write(Path(staging_dir))
            for statement in staging.copy_statements:
                self.execute(statement)
            return BulkLoadStats(
                node_rows=staging.node_rows,
                edge_rows=staging.edge_rows,
                connector_rows=staging.connector_rows,
                copy_calls=len(staging.copy_statements),
            )

    def delete_partition(
        self,
        path: str,
        *,
        manifest_entry: Mapping[str, Any] | Any | None = None,
        retained_node_ids: set[str] | None = None,
        retained_edge_ids: set[str] | None = None,
    ) -> None:
        if manifest_entry is None:
            return
        retained = retained_node_ids or set()
        retained_edges = retained_edge_ids or set()
        edge_types = _entry_mapping(manifest_entry, "edge_types")
        node_types = _entry_mapping(manifest_entry, "node_types")

        for edge_id in _entry_values(manifest_entry, "edge_ids"):
            if edge_id in retained_edges:
                continue
            edge_type = edge_types.get(edge_id)
            if edge_type:
                self._delete_edge(edge_id, edge_type)

        for node_id in _entry_values(manifest_entry, "node_ids"):
            if node_id in retained:
                continue
            node_type = node_types.get(node_id)
            if node_type:
                self._delete_node(node_id, node_type)

    def read_manifest(self, path: str | Path) -> Any:
        from codebase_graph.ingest.materializer import MaterializationManifest

        return MaterializationManifest.load(Path(path))

    def write_manifest(self, manifest: Any, path: str | Path) -> None:
        manifest.write(Path(path))

    def _execute_ignoring_existing(self, statement: str) -> None:
        try:
            self.conn.execute(statement)
        except Exception as exc:
            message = str(exc).lower()
            if "already exists" not in message and "exists already" not in message and "already installed" not in message:
                raise

    def _execute_ignoring_missing(self, statement: str, parameters: dict[str, Any] | None = None) -> None:
        try:
            self.execute(statement, parameters)
        except Exception as exc:
            message = str(exc).lower()
            if "does not exist" not in message and "not found" not in message:
                raise

    def _delete_edge(self, edge_id: str, edge_type: str) -> None:
        self._execute_ignoring_missing(
            f"MATCH ()-[r:{quote_identifier(f'FROM_{edge_type}')}]->(edge:{quote_identifier(edge_type)} {{id: $id}}) DELETE r",
            {"id": edge_id},
        )
        self._execute_ignoring_missing(
            f"MATCH (edge:{quote_identifier(edge_type)} {{id: $id}})-[r:{quote_identifier(f'TO_{edge_type}')}]->() DELETE r",
            {"id": edge_id},
        )
        self._execute_ignoring_missing(
            f"MATCH (edge:{quote_identifier(edge_type)} {{id: $id}}) DELETE edge",
            {"id": edge_id},
        )

    def _delete_node(self, node_id: str, node_type: str) -> None:
        self._execute_ignoring_missing(
            f"MATCH (node:{quote_identifier(node_type)} {{id: $id}}) DELETE node",
            {"id": node_id},
        )


def create_ladybug_database(db_path: str | Path = ":memory:", *, include_fts: bool = True) -> LadybugCodeGraphStore:
    store = LadybugCodeGraphStore(db_path, include_fts=include_fts)
    store.ensure_schema()
    return store


NODE_FIELDS = {
    node_type.name: tuple(field for field in node_type.fields)
    for node_type in NODE_TYPES
}
_OMIT_JSON_VALUE = object()
EDGE_FIELDS_BY_TYPE = {
    relation_type.name: tuple(field for field in relation_type.fields)
    for relation_type in RELATION_TYPES
}


@dataclass(slots=True)
class _BulkStagingTables:
    nodes: dict[str, dict[str, dict[str, Any]]]
    edges: dict[str, dict[str, dict[str, Any]]]
    connectors: dict[tuple[str, str, str], dict[tuple[str, str, str], dict[str, str]]]

    @property
    def is_empty(self) -> bool:
        return not any(self.nodes.values()) and not any(self.edges.values()) and not any(self.connectors.values())

    def write(self, staging_dir: Path) -> _BulkStagingResult:
        staging_dir.mkdir(parents=True, exist_ok=True)
        copy_statements: list[str] = []
        node_rows = 0
        edge_rows = 0
        connector_rows = 0

        for node_type in NODE_TYPES:
            rows = self.nodes.get(node_type.name, {})
            if not rows:
                continue
            path = staging_dir / f"{_stage_file_stem(node_type.name)}.json"
            _write_json_rows(path, rows.values())
            node_rows += len(rows)
            copy_statements.append(f'COPY {quote_identifier(node_type.name)} FROM "{_copy_path(path)}";')

        for relation_type in RELATION_TYPES:
            rows = self.edges.get(relation_type.name, {})
            if not rows:
                continue
            path = staging_dir / f"{_stage_file_stem(relation_type.name)}.json"
            _write_json_rows(path, rows.values())
            edge_rows += len(rows)
            copy_statements.append(f'COPY {quote_identifier(relation_type.name)} FROM "{_copy_path(path)}";')

        for relation_type in RELATION_TYPES:
            for connector_table in (f"FROM_{relation_type.name}", f"TO_{relation_type.name}"):
                connector_groups = [
                    (endpoint_pair, rows)
                    for endpoint_pair, rows in self.connectors.items()
                    if endpoint_pair[0] == connector_table and rows
                ]
                for (table, source_type, target_type), rows in sorted(connector_groups):
                    path = staging_dir / (
                        f"{_stage_file_stem(table)}__"
                        f"{_stage_file_stem(source_type)}__{_stage_file_stem(target_type)}.csv"
                    )
                    _write_csv_rows(path, ("from_id", "to_id", "role"), rows.values())
                    connector_rows += len(rows)
                    copy_statements.append(
                        f'COPY {quote_identifier(table)} FROM "{_copy_path(path)}" '
                        f'(header=true, from="{source_type}", to="{target_type}");'
                    )

        return _BulkStagingResult(
            copy_statements=tuple(copy_statements),
            node_rows=node_rows,
            edge_rows=edge_rows,
            connector_rows=connector_rows,
        )


@dataclass(frozen=True, slots=True)
class _BulkStagingResult:
    copy_statements: tuple[str, ...]
    node_rows: int
    edge_rows: int
    connector_rows: int


def _build_bulk_staging_tables(
    graphs: list[CodeGraph] | tuple[CodeGraph, ...],
    *,
    skip_node_ids: set[str] | None = None,
    skip_edge_ids: set[str] | None = None,
) -> _BulkStagingTables:
    skipped_nodes = skip_node_ids or set()
    skipped_edges = skip_edge_ids or set()
    node_rows: dict[str, dict[str, dict[str, Any]]] = defaultdict(dict)
    edge_rows: dict[str, dict[str, dict[str, Any]]] = defaultdict(dict)
    connector_rows: dict[tuple[str, str, str], dict[tuple[str, str, str], dict[str, str]]] = defaultdict(dict)

    for graph in graphs:
        for node in graph.nodes.values():
            if node.id in skipped_nodes:
                continue
            row = _row_for_fields(node.as_dict(), NODE_FIELDS[node.table], for_json_copy=True)
            _merge_staged_row(node_rows[node.table], node.id, row)

        for edge in graph.edges.values():
            if edge.id in skipped_edges:
                continue
            row = _row_for_fields(edge.as_dict(), EDGE_FIELDS_BY_TYPE[edge.type], for_json_copy=True)
            _merge_staged_row(edge_rows[edge.type], edge.id, row)

        for edge in graph.edges.values():
            if edge.id in skipped_edges:
                continue
            source = graph.nodes[edge.source_id]
            target = graph.nodes[edge.target_id]
            _add_connector_row(
                connector_rows,
                table=f"FROM_{edge.type}",
                source_type=source.table,
                target_type=edge.type,
                from_id=source.id,
                to_id=edge.id,
                role="source",
            )
            _add_connector_row(
                connector_rows,
                table=f"TO_{edge.type}",
                source_type=edge.type,
                target_type=target.table,
                from_id=edge.id,
                to_id=target.id,
                role="target",
            )

    return _BulkStagingTables(nodes=dict(node_rows), edges=dict(edge_rows), connectors=dict(connector_rows))


def _row_for_fields(row: Mapping[str, Any], fields: tuple[Any, ...], *, for_json_copy: bool = False) -> dict[str, Any]:
    return {
        field.name: _copy_field_value(field.name, row, field.value_type, for_json_copy=for_json_copy)
        for field in fields
    }


def _copy_field_value(name: str, row: Mapping[str, Any], value_type: str, *, for_json_copy: bool = False) -> Any:
    if not for_json_copy or value_type != "json":
        return _field_value(name, row, value_type)
    if name in row:
        value = row[name]
    else:
        metadata = row.get("metadata") if isinstance(row.get("metadata"), Mapping) else {}
        value = metadata.get(name)
    safe = _json_safe(value if value is not None else {})
    if safe is _OMIT_JSON_VALUE:
        return {}
    return safe


def _merge_staged_row(rows: dict[str, dict[str, Any]], row_id: str, row: dict[str, Any]) -> None:
    existing = rows.get(row_id)
    if existing is None:
        rows[row_id] = row
        return
    for key, value in row.items():
        if value not in (None, "", {}, []) and existing.get(key) in (None, "", {}, []):
            existing[key] = value
    existing_metadata = existing.get("metadata")
    incoming_metadata = row.get("metadata")
    if isinstance(existing_metadata, dict) and isinstance(incoming_metadata, dict):
        existing_metadata.update(incoming_metadata)


def _add_connector_row(
    rows: dict[tuple[str, str, str], dict[tuple[str, str, str], dict[str, str]]],
    *,
    table: str,
    source_type: str,
    target_type: str,
    from_id: str,
    to_id: str,
    role: str,
) -> None:
    key = (table, source_type, target_type)
    rows[key][(from_id, to_id, role)] = {"from_id": from_id, "to_id": to_id, "role": role}


def _write_json_rows(path: Path, rows: Any) -> None:
    with path.open("w", encoding="utf-8") as handle:
        json.dump(list(rows), handle, separators=(",", ":"), sort_keys=True)
        handle.write("\n")


def _write_csv_rows(path: Path, columns: tuple[str, ...], rows: Any) -> None:
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            writer.writerow({column: row.get(column, "") for column in columns})


def _stage_file_stem(name: str) -> str:
    return "".join(character.lower() if character.isalnum() else "_" for character in name).strip("_") or "table"


def _copy_path(path: Path) -> str:
    return path.as_posix().replace('"', '\\"')


def _field_value(name: str, row: Mapping[str, Any], value_type: str) -> Any:
    if name in row:
        value = row[name]
    else:
        metadata = row.get("metadata") if isinstance(row.get("metadata"), Mapping) else {}
        value = metadata.get(name)
    if value_type == "json":
        return json.dumps(_json_safe(value if value is not None else {}), sort_keys=True)
    return value


def _json_safe(value: Any) -> Any:
    if isinstance(value, Mapping):
        safe_items = {}
        for key, item in value.items():
            safe_item = _json_safe(item)
            if safe_item is _OMIT_JSON_VALUE:
                continue
            safe_items[str(key)] = safe_item
        return safe_items
    if isinstance(value, list | tuple):
        if not value:
            return _OMIT_JSON_VALUE
        return [_json_safe(item) for item in value]
    if value is None:
        return _OMIT_JSON_VALUE
    return value


def _entry_values(entry: Mapping[str, Any] | Any, field_name: str) -> tuple[str, ...]:
    if isinstance(entry, Mapping):
        values = entry.get(field_name, ())
    else:
        values = getattr(entry, field_name, ())
    return tuple(str(value) for value in values)


def _entry_mapping(entry: Mapping[str, Any] | Any, field_name: str) -> dict[str, str]:
    if isinstance(entry, Mapping):
        values = entry.get(field_name, {})
    else:
        values = getattr(entry, field_name, {})
    return {str(key): str(value) for key, value in dict(values).items()}
