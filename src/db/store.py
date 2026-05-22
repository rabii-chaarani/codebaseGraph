from __future__ import annotations

import json
from collections.abc import Mapping
from pathlib import Path
from typing import Any

from core import CodeGraph, GraphEdge, GraphNode
from ontology import NODE_TYPES, RELATION_TYPES

from .schema import build_ladybug_schema, build_ladybug_schema_statements, quote_identifier


class LadybugUnavailableError(RuntimeError):
    pass


class LadybugCodeGraphStore:
    def __init__(self, db_path: str | Path = ":memory:", *, include_fts: bool = True) -> None:
        self.db_path = db_path
        self.include_fts = include_fts
        try:
            import real_ladybug as lb
        except ImportError as exc:
            raise LadybugUnavailableError(
                "LadyBugDB Python bindings are not installed. Install `real_ladybug` or `codebase-graph[ladybug]`."
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
    ) -> None:
        if previous_entry is not None:
            self.delete_partition(path, manifest_entry=previous_entry, retained_node_ids=retained_node_ids)

        for node in graph.nodes.values():
            self._upsert_node(node)
        for edge in graph.edges.values():
            self._upsert_edge_node(edge)
        for edge in graph.edges.values():
            source = graph.nodes[edge.source_id]
            target = graph.nodes[edge.target_id]
            self._upsert_connector(edge, source, target)

    def delete_partition(
        self,
        path: str,
        *,
        manifest_entry: Mapping[str, Any] | Any | None = None,
        retained_node_ids: set[str] | None = None,
    ) -> None:
        if manifest_entry is None:
            return
        retained = retained_node_ids or set()
        edge_types = _entry_mapping(manifest_entry, "edge_types")
        node_types = _entry_mapping(manifest_entry, "node_types")

        for edge_id in _entry_values(manifest_entry, "edge_ids"):
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
        from ingest.materializer import MaterializationManifest

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

    def _upsert_node(self, node: GraphNode) -> None:
        table_fields = NODE_FIELDS[node.table]
        row = node.as_dict()
        statement, parameters = _merge_statement(node.table, table_fields, row)
        self.execute(statement, parameters)

    def _upsert_edge_node(self, edge: GraphEdge) -> None:
        table_fields = EDGE_FIELDS_BY_TYPE[edge.type]
        row = edge.as_dict()
        statement, parameters = _merge_statement(edge.type, table_fields, row)
        self.execute(statement, parameters)

    def _upsert_connector(self, edge: GraphEdge, source: GraphNode, target: GraphNode) -> None:
        from_relation = quote_identifier(f"FROM_{edge.type}")
        to_relation = quote_identifier(f"TO_{edge.type}")
        self.execute(
            (
                f"MATCH (source:{quote_identifier(source.table)} {{id: $source_id}}), "
                f"(edge:{quote_identifier(edge.type)} {{id: $edge_id}}) "
                f"MERGE (source)-[:{from_relation}]->(edge)"
            ),
            {"source_id": source.id, "edge_id": edge.id},
        )
        self.execute(
            (
                f"MATCH (edge:{quote_identifier(edge.type)} {{id: $edge_id}}), "
                f"(target:{quote_identifier(target.table)} {{id: $target_id}}) "
                f"MERGE (edge)-[:{to_relation}]->(target)"
            ),
            {"edge_id": edge.id, "target_id": target.id},
        )

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


def _merge_statement(table: str, fields: tuple[Any, ...], row: Mapping[str, Any]) -> tuple[str, dict[str, Any]]:
    parameters: dict[str, Any] = {"id": _field_value("id", row, "string")}
    assignments = []
    for field in fields:
        if field.name == "id":
            continue
        field_value = _field_value(field.name, row, field.value_type)
        if field_value is None:
            continue
        parameters[field.name] = field_value
        value = f"CAST(${field.name} AS JSON)" if field.value_type == "json" else f"${field.name}"
        assignments.append(f"n.{quote_identifier(field.name)} = {value}")
    statement = f"MERGE (n:{quote_identifier(table)} {{id: $id}})"
    if assignments:
        statement += f" SET {', '.join(assignments)}"
    return statement, parameters


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
