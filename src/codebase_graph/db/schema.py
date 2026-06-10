from __future__ import annotations

from collections.abc import Iterable

from codebase_graph.ontology import EDGE_FIELDS, NODE_TYPES, RELATION_TYPES, SEARCH_INDEXES, FieldSpec

TYPE_MAP = {
    "string": "STRING",
    "integer": "INT64",
    "number": "DOUBLE",
    "boolean": "BOOLEAN",
    "json": "JSON",
}


def quote_identifier(name: str) -> str:
    """Quote identifier for Ladybug database persistence layer.

    Args:
        name: Name used by the Ladybug database persistence layer workflow.

    Returns:
        Formatted text returned to the caller.
    """
    return f"`{name.replace('`', '``')}`"


def ladybug_type(value_type: str) -> str:
    """Manage type within Ladybug database persistence layer.

    Args:
        value_type: Ontology type name for value handling.

    Returns:
        Formatted text returned to the caller.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    try:
        return TYPE_MAP[value_type]
    except KeyError as exc:
        raise ValueError(f"Unsupported ontology field type for LadyBugDB: {value_type}") from exc


def build_ladybug_schema(*, include_fts: bool = True) -> str:
    """Build ladybug schema for Ladybug database persistence layer.

    Args:
        include_fts: Include full-text search used by the Ladybug database persistence
        layer workflow.

    Returns:
        Formatted text returned to the caller.
    """
    return ";\n\n".join(build_ladybug_schema_statements(include_fts=include_fts)) + ";"


def build_ladybug_schema_statements(*, include_fts: bool = True) -> list[str]:
    """Build ladybug schema statements for Ladybug database persistence layer.

    Args:
        include_fts: Include full-text search used by the Ladybug database persistence
        layer workflow.

    Returns:
        Ordered results returned to the Ladybug database persistence layer caller.
    """
    statements = [
        "INSTALL json",
        "LOAD json",
    ]
    if include_fts:
        statements.extend(("INSTALL fts", "LOAD fts"))
    # Schema generation mirrors the ontology: semantic node tables first,
    # relation-node tables second, connector relation tables last.
    statements.extend(_semantic_node_table_sql())
    statements.extend(_edge_node_table_sql())
    statements.extend(_connector_table_sql())
    if include_fts:
        statements.extend(_fts_index_sql())
    return statements


def _semantic_node_table_sql() -> list[str]:
    """Return node table SQL for Ladybug database persistence layer.

    Returns:
        Ordered results returned to the Ladybug database persistence layer caller.
    """
    return [
        _node_table_sql(node_type.name, node_type.fields)
        for node_type in NODE_TYPES
    ]


def _edge_node_table_sql() -> list[str]:
    """Manage node table SQL within Ladybug database persistence layer.

    Returns:
        Ordered results returned to the Ladybug database persistence layer caller.
    """
    return [
        _node_table_sql(relation_type.name, relation_type.fields or EDGE_FIELDS)
        for relation_type in RELATION_TYPES
    ]


def _connector_table_sql() -> list[str]:
    """Manage table SQL within Ladybug database persistence layer.

    Returns:
        Ordered results returned to the Ladybug database persistence layer caller.
    """
    statements: list[str] = []
    for relation_type in RELATION_TYPES:
        relation_name = relation_type.name
        source_pairs = _dedupe_pairs((source_type, relation_name) for source_type in relation_type.source_types)
        target_pairs = _dedupe_pairs((relation_name, target_type) for target_type in relation_type.target_types)
        # Edge payload lives on the relation node table; connector tables only
        # model typed endpoints so Cypher traversals stay explicit.
        statements.append(_relation_table_sql(f"FROM_{relation_name}", source_pairs, role="source"))
        statements.append(_relation_table_sql(f"TO_{relation_name}", target_pairs, role="target"))
    return statements


def _node_table_sql(table_name: str, fields: Iterable[FieldSpec]) -> str:
    """Manage table SQL within Ladybug database persistence layer.

    Args:
        table_name: Name used to select or label table data.
        fields: Field mapping to read or serialize.

    Returns:
        Formatted text returned to the caller.
    """
    columns = [_field_sql(field) for field in _dedupe_fields(fields)]
    return f"CREATE NODE TABLE IF NOT EXISTS {quote_identifier(table_name)}(\n" + ",\n".join(columns) + "\n)"


def _relation_table_sql(table_name: str, endpoint_pairs: Iterable[tuple[str, str]], *, role: str) -> str:
    """Return table SQL for Ladybug database persistence layer.

    Args:
        table_name: Name used to select or label table data.
        endpoint_pairs: Endpoint pairs used by the Ladybug database persistence layer
        workflow.
        role: Role used by the Ladybug database persistence layer workflow.

    Returns:
        Formatted text returned to the caller.
    """
    endpoints = [
        f"  FROM {quote_identifier(source_type)} TO {quote_identifier(target_type)}"
        for source_type, target_type in endpoint_pairs
    ]
    columns = [*endpoints, f"  {quote_identifier('role')} STRING DEFAULT '{role}'"]
    return f"CREATE REL TABLE IF NOT EXISTS {quote_identifier(table_name)}(\n" + ",\n".join(columns) + "\n)"


def _field_sql(field: FieldSpec) -> str:
    """Read SQL for Ladybug database persistence layer.

    Args:
        field: Field used by the Ladybug database persistence layer workflow.

    Returns:
        Formatted text returned to the caller.
    """
    primary_key = " PRIMARY KEY" if field.name == "id" else ""
    return f"  {quote_identifier(field.name)} {ladybug_type(field.value_type)}{primary_key}"


def _dedupe_fields(fields: Iterable[FieldSpec]) -> list[FieldSpec]:
    """Deduplicate fields for Ladybug database persistence layer.

    Args:
        fields: Field mapping to read or serialize.

    Returns:
        Ordered results returned to the Ladybug database persistence layer caller.
    """
    seen: set[str] = set()
    deduped: list[FieldSpec] = []
    for field in fields:
        if field.name in seen:
            continue
        seen.add(field.name)
        deduped.append(field)
    return deduped


def _dedupe_pairs(pairs: Iterable[tuple[str, str]]) -> list[tuple[str, str]]:
    """Deduplicate pairs for Ladybug database persistence layer.

    Args:
        pairs: Pairs used by the Ladybug database persistence layer workflow.

    Returns:
        Ordered results returned to the Ladybug database persistence layer caller.
    """
    seen: set[tuple[str, str]] = set()
    deduped: list[tuple[str, str]] = []
    for pair in pairs:
        if pair in seen:
            continue
        seen.add(pair)
        deduped.append(pair)
    return deduped


def _fts_index_sql() -> list[str]:
    """Manage index SQL within Ladybug database persistence layer.

    Returns:
        Ordered results returned to the Ladybug database persistence layer caller.
    """
    statements: list[str] = []
    for index in SEARCH_INDEXES:
        fields = ", ".join(repr(field) for field in index["fields"])
        for node_type in index["node_types"]:
            # Ladybug builds FTS indexes per node table, so the shared ontology
            # index name is expanded with the concrete node type.
            index_name = f"{index['name']}_{node_type}"
            statements.append(f"CALL CREATE_FTS_INDEX('{node_type}', '{index_name}', [{fields}])")
    return statements
