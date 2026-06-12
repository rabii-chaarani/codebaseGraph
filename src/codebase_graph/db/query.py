from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Protocol

from codebase_graph.ontology import get_relation_type

from .schema import quote_identifier


@dataclass(frozen=True, slots=True)
class GraphNeighbor:
    """Represent graph neighbor data used by Ladybug database persistence layer.

    The class belongs to Read-oriented query adapter used by search and context-building
    services.
    """
    node_id: str
    node_type: str
    label: str
    qualified_name: str = ""
    path: str = ""
    line_start: int | None = None
    line_end: int | None = None
    summary: str = ""
    relation: str = ""
    direction: str = ""
    source_node_id: str = ""
    target_node_id: str = ""
    edge_id: str = ""
    edge_kind: str = ""
    edge_confidence: float | None = None
    edge_metadata: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class SearchIndexRow:
    """Represent search index row data used by Ladybug database persistence layer.

    The class belongs to Read-oriented query adapter used by search and context-building
    services.
    """
    id: str
    node_type: str
    label: str
    qualified_name: str = ""
    path: str = ""
    line_start: int | None = None
    line_end: int | None = None
    summary: str = ""
    score: float = 0.0
    metadata: dict[str, Any] = field(default_factory=dict)


class GraphQueryAdapter(Protocol):
    """Adapt graph query data to the codebaseGraph interface."""
    def search_index(self, *, node_type: str, index_name: str, query: str, limit: int) -> list[SearchIndexRow]:
        """Search index for Ladybug database persistence layer.

        Args:
            node_type: Ontology node type used to choose a table or label.
            index_name: Full-text search index name declared by the ontology.
            query: User search text or read-only Cypher statement.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the Ladybug database persistence layer caller.
        """
        ...

    def neighbors(
        self,
        *,
        node_id: str,
        node_type: str,
        relation: str,
        direction: str,
        limit: int,
    ) -> list[GraphNeighbor]:
        """Manage Ladybug database persistence state.

        Args:
            node_id: Identifier for the node graph object.
            node_type: Ontology node type used to choose a table or label.
            relation: Ontology relation name used for graph traversal.
            direction: Traversal direction relative to the source node.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the Ladybug database persistence layer caller.
        """
        ...


class LadybugGraphQueryAdapter:
    """Adapt ladybug graph query data to the codebaseGraph interface."""
    def __init__(self, store: Any) -> None:
        """Initialize ladybug graph query adapter with the collaborators and state it owns.

        Args:
            store: Graph store used for persistence or read-only queries.
        """
        self.store = store

    def search_index(self, *, node_type: str, index_name: str, query: str, limit: int) -> list[SearchIndexRow]:
        """Search index for Ladybug database persistence layer.

        Args:
            node_type: Ontology node type used to choose a table or label.
            index_name: Full-text search index name declared by the ontology.
            query: User search text or read-only Cypher statement.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the Ladybug database persistence layer caller.
        """
        rows = self.store.execute(
            _fts_query_statement(node_type=node_type, index_name=index_name),
            {"query": query, "top": limit},
        ).get_all()
        return [
            SearchIndexRow(
                id=_text(_value(row, 0)),
                node_type=node_type,
                label=_text(_value(row, 1)),
                qualified_name=_text(_value(row, 2)),
                path=_text(_value(row, 3)),
                line_start=_optional_int(_value(row, 4)),
                line_end=_optional_int(_value(row, 5)),
                summary=_text(_value(row, 6)),
                score=float(_value(row, 7) or 0.0),
            )
            for row in rows
        ]

    def neighbors(
        self,
        *,
        node_id: str,
        node_type: str,
        relation: str,
        direction: str,
        limit: int,
    ) -> list[GraphNeighbor]:
        """Manage Ladybug database persistence state.

        Args:
            node_id: Identifier for the node graph object.
            node_type: Ontology node type used to choose a table or label.
            relation: Ontology relation name used for graph traversal.
            direction: Traversal direction relative to the source node.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the Ladybug database persistence layer caller.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
        if direction not in {"outgoing", "incoming"}:
            raise ValueError(f"Unsupported relation direction: {direction}")
        try:
            relation_type = get_relation_type(relation)
        except KeyError:
            return []

        if direction == "outgoing":
            if node_type not in relation_type.source_types:
                return []
            neighbor_types = relation_type.target_types
        else:
            if node_type not in relation_type.target_types:
                return []
            neighbor_types = relation_type.source_types

        neighbors: list[GraphNeighbor] = []
        for neighbor_type in neighbor_types:
            remaining = limit - len(neighbors)
            if remaining <= 0:
                break
            rows = self.store.execute(
                _neighbor_statement(
                    node_type=node_type,
                    neighbor_type=neighbor_type,
                    relation=relation,
                    direction=direction,
                    limit=remaining,
                ),
                {"node_id": node_id},
            ).get_all()
            neighbors.extend(_neighbor_from_row(row, neighbor_type, relation=relation, direction=direction) for row in rows)
        return neighbors


def graph_query_adapter(store: Any) -> GraphQueryAdapter:
    """Return query adapter for Ladybug database persistence layer.

    Args:
        store: Graph store used for persistence or read-only queries.

    Returns:
        GraphQueryAdapter instance populated with data from the Ladybug database persistence
        layer workflow.
    """
    adapter = getattr(store, "graph_query_adapter", None)
    if adapter is not None:
        return adapter
    return LadybugGraphQueryAdapter(store)


def _fts_query_statement(*, node_type: str, index_name: str) -> str:
    """Manage query statement within Ladybug database persistence layer.

    Args:
        node_type: Ontology node type used to choose a table or label.
        index_name: Full-text search index name declared by the ontology.

    Returns:
        Formatted text returned to the caller.
    """
    return (
        f"CALL QUERY_FTS_INDEX('{node_type}', '{index_name}', $query, TOP := $top) "
        "RETURN node.id, node.label, node.qualified_name, node.path, "
        "node.line_start, node.line_end, node.summary, score"
    )


def _neighbor_statement(
    *,
    node_type: str,
    neighbor_type: str,
    relation: str,
    direction: str,
    limit: int,
) -> str:
    """Manage statement within Ladybug database persistence layer.

    Args:
        node_type: Ontology node type used to choose a table or label.
        neighbor_type: Ontology node type expected on the other side of a relation.
        relation: Ontology relation name used for graph traversal.
        direction: Traversal direction relative to the source node.
        limit: Maximum number of rows or results requested.

    Returns:
        Formatted text returned to the caller.
    """
    if direction == "outgoing":
        return (
            f"MATCH (source:{quote_identifier(node_type)} {{id: $node_id}})"
            f"-[:{quote_identifier(f'FROM_{relation}')}]->(edge:{quote_identifier(relation)})"
            f"-[:{quote_identifier(f'TO_{relation}')}]->(neighbor:{quote_identifier(neighbor_type)}) "
            "RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, "
            "neighbor.line_start, neighbor.line_end, neighbor.summary, "
            "edge.id, edge.kind, edge.source_id, edge.target_id, edge.confidence, edge.metadata "
            f"LIMIT {int(limit)}"
        )
    return (
        f"MATCH (neighbor:{quote_identifier(neighbor_type)})"
        f"-[:{quote_identifier(f'FROM_{relation}')}]->(edge:{quote_identifier(relation)})"
        f"-[:{quote_identifier(f'TO_{relation}')}]->(target:{quote_identifier(node_type)} {{id: $node_id}}) "
        "RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, "
        "neighbor.line_start, neighbor.line_end, neighbor.summary, "
        "edge.id, edge.kind, edge.source_id, edge.target_id, edge.confidence, edge.metadata "
        f"LIMIT {int(limit)}"
    )


def _neighbor_from_row(row: Any, node_type: str, *, relation: str, direction: str) -> GraphNeighbor:
    """Manage from row within Ladybug database persistence layer.

    Args:
        row: Database row returned by Ladybug.
        node_type: Ontology node type used to choose a table or label.

    Returns:
        GraphNeighbor instance populated with data from the Ladybug database persistence
        layer workflow.
    """
    return GraphNeighbor(
        node_id=_text(_value(row, 0)),
        node_type=node_type,
        label=_text(_value(row, 1)),
        qualified_name=_text(_value(row, 2)),
        path=_text(_value(row, 3)),
        line_start=_optional_int(_value(row, 4)),
        line_end=_optional_int(_value(row, 5)),
        summary=_text(_value(row, 6)),
        relation=relation,
        direction=direction,
        edge_id=_text(_value(row, 7)),
        edge_kind=_text(_value(row, 8)),
        source_node_id=_text(_value(row, 9)),
        target_node_id=_text(_value(row, 10)),
        edge_confidence=_optional_float(_value(row, 11)),
        edge_metadata=_metadata(_value(row, 12)),
    )


def _optional_int(value: Any) -> int | None:
    """Manage int within Ladybug database persistence layer.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        int | None instance populated with data from the Ladybug database persistence layer
        workflow.
    """
    return None if value is None else int(value)


def _optional_float(value: Any) -> float | None:
    """Coerce optional numeric edge metadata from Ladybug rows."""
    return None if value is None else float(value)


def _metadata(value: Any) -> dict[str, Any]:
    """Coerce JSON metadata from Ladybug rows into a dictionary."""
    if isinstance(value, dict):
        return value
    if isinstance(value, str) and value:
        try:
            decoded = json.loads(value)
        except json.JSONDecodeError:
            return {}
        return decoded if isinstance(decoded, dict) else {}
    return {}


def _text(value: Any) -> str:
    """Coerce Ladybug database persistence layer for Ladybug database persistence layer.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Formatted text returned to the caller.
    """
    return "" if value is None else str(value)


def _value(row: Any, index: int) -> Any:
    """Manage Ladybug database persistence state.

    Args:
        row: Database row returned by Ladybug.
        index: Row index or search index metadata used to select fields.

    Returns:
        Any instance populated with data from the Ladybug database persistence layer
        workflow.
    """
    try:
        return row[index]
    except IndexError:
        return None
