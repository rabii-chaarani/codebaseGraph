from __future__ import annotations

from pathlib import Path

import pytest

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode
from codebase_graph.db import (
    LadybugCodeGraphStore,
    build_ladybug_schema,
    build_ladybug_schema_statements,
    create_ladybug_database,
    ladybug_type,
    quote_identifier,
)
from codebase_graph.ontology import NODE_TYPES, RELATION_TYPES, SEARCH_INDEXES


def test_ladybug_schema_declares_all_ontology_nodes_and_edge_nodes() -> None:
    schema = build_ladybug_schema()

    for node_type in NODE_TYPES:
        assert f"CREATE NODE TABLE IF NOT EXISTS `{node_type.name}`" in schema
    for relation_type in RELATION_TYPES:
        assert f"CREATE NODE TABLE IF NOT EXISTS `{relation_type.name}`" in schema


def test_ladybug_schema_declares_from_and_to_connector_tables() -> None:
    schema = build_ladybug_schema()

    for relation_type in RELATION_TYPES:
        assert f"CREATE REL TABLE IF NOT EXISTS `FROM_{relation_type.name}`" in schema
        assert f"CREATE REL TABLE IF NOT EXISTS `TO_{relation_type.name}`" in schema

        for source_type in set(relation_type.source_types):
            assert f"FROM `{source_type}` TO `{relation_type.name}`" in schema
        for target_type in set(relation_type.target_types):
            assert f"FROM `{relation_type.name}` TO `{target_type}`" in schema


def test_ladybug_schema_keeps_relation_payload_on_edge_nodes() -> None:
    schema = build_ladybug_schema()

    contains_start = schema.index("CREATE NODE TABLE IF NOT EXISTS `Contains`")
    from_contains_start = schema.index("CREATE REL TABLE IF NOT EXISTS `FROM_Contains`")
    contains_table = schema[contains_start:from_contains_start]

    assert "`id` STRING PRIMARY KEY" in contains_table
    assert "`kind` STRING" in contains_table
    assert "`source_id` STRING" in contains_table
    assert "`target_id` STRING" in contains_table
    assert "`confidence` DOUBLE" in contains_table
    assert "`metadata` JSON" in contains_table


def test_ladybug_schema_maps_types_and_quotes_identifiers() -> None:
    assert ladybug_type("string") == "STRING"
    assert ladybug_type("integer") == "INT64"
    assert ladybug_type("number") == "DOUBLE"
    assert ladybug_type("boolean") == "BOOLEAN"
    assert ladybug_type("json") == "JSON"
    assert quote_identifier("Query") == "`Query`"
    assert quote_identifier("odd`name") == "`odd``name`"


def test_ladybug_schema_rejects_unknown_field_type() -> None:
    with pytest.raises(ValueError, match="Unsupported ontology field type"):
        ladybug_type("object")


def test_ladybug_schema_deduplicates_connector_endpoint_pairs() -> None:
    schema = build_ladybug_schema()

    assert schema.count("FROM `Contains` TO `Assignment`") == 1
    assert schema.count("FROM `Contains` TO `Query`") == 1
    assert "TO `Assignment` |" not in schema


def test_ladybug_schema_creates_fts_indexes_for_semantic_node_tables_only() -> None:
    statements = build_ladybug_schema_statements(include_fts=True)
    schema = ";\n".join(statements)
    relation_names = {relation_type.name for relation_type in RELATION_TYPES}

    assert "INSTALL fts" in statements
    for index in SEARCH_INDEXES:
        for node_type in index["node_types"]:
            assert f"CALL CREATE_FTS_INDEX('{node_type}', '{index['name']}_{node_type}'" in schema
            assert node_type not in relation_names


def test_ladybug_schema_can_skip_fts_statements() -> None:
    statements = build_ladybug_schema_statements(include_fts=False)

    assert "INSTALL json" in statements
    assert "LOAD json" in statements
    assert "INSTALL fts" not in statements
    assert "LOAD fts" not in statements
    assert not any(statement.startswith("CALL CREATE_FTS_INDEX") for statement in statements)


def test_ladybug_schema_executes_against_in_memory_database() -> None:
    real_ladybug = pytest.importorskip("real_ladybug")
    conn = real_ladybug.Connection(real_ladybug.Database(":memory:"))

    for statement in build_ladybug_schema_statements():
        conn.execute(statement)


def test_ladybug_store_creates_in_memory_database_without_persistent_file() -> None:
    pytest.importorskip("real_ladybug")

    store = create_ladybug_database(":memory:")

    assert isinstance(store, LadybugCodeGraphStore)
    assert store.schema_sql.startswith("INSTALL json")


def test_ladybug_store_schema_setup_is_idempotent() -> None:
    pytest.importorskip("real_ladybug")
    store = create_ladybug_database(":memory:")

    store.ensure_schema()


def test_ladybug_store_allows_multiple_read_only_handles(tmp_path: Path) -> None:
    pytest.importorskip("real_ladybug")
    db_path = tmp_path / "graph.ldb"
    writer = create_ladybug_database(db_path, include_fts=False)
    writer.close()

    first = create_ladybug_database(db_path, include_fts=False, read_only=True)
    try:
        second = create_ladybug_database(db_path, include_fts=False, read_only=True)
        second.close()
    finally:
        first.close()


def test_ladybug_store_bulk_loader_groups_rows_by_table() -> None:
    graph = CodeGraph()
    graph.add_node(GraphNode(id="file:service", table="File", label="service.py", kind="source_file"))
    graph.add_node(GraphNode(id="function:one", table="Function", label="one", kind="function"))
    graph.add_node(GraphNode(id="function:two", table="Function", label="two", kind="function"))
    graph.add_edge(GraphEdge(id="contains:one", type="Contains", source_id="file:service", target_id="function:one"))
    graph.add_edge(GraphEdge(id="contains:two", type="Contains", source_id="file:service", target_id="function:two"))
    store = object.__new__(LadybugCodeGraphStore)
    statements: list[str] = []
    store.execute = lambda statement, parameters=None: statements.append(statement)  # type: ignore[method-assign]

    stats = store.insert_graphs_bulk([graph])

    assert stats.node_rows == 3
    assert stats.edge_rows == 2
    assert stats.connector_rows == 4
    assert stats.copy_calls == 5
    assert len(statements) == 5
    assert any(statement.startswith("COPY `File`") and statement.endswith('";') for statement in statements)
    assert any(statement.startswith("COPY `Function`") and statement.endswith('";') for statement in statements)
    assert any(statement.startswith("COPY `Contains`") and statement.endswith('";') for statement in statements)
    assert any('COPY `FROM_Contains`' in statement and 'from="File", to="Contains"' in statement for statement in statements)
    assert any('COPY `TO_Contains`' in statement and 'from="Contains", to="Function"' in statement for statement in statements)
