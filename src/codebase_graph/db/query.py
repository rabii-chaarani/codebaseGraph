from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Protocol

from codebase_graph.ontology import get_relation_type

from .schema import quote_identifier


@dataclass(frozen=True, slots=True)
class GraphNeighbor:
    """Store graph neighbor data."""
    node_id: str
    node_type: str
    label: str
    qualified_name: str = ""
    path: str = ""
    line_start: int | None = None
    line_end: int | None = None
    summary: str = ""


@dataclass(frozen=True, slots=True)
class SearchIndexRow:
    """Store search index row data."""
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
    """Adapt graph query operations to the project interface."""
    def search_index(self, *, node_type: str, index_name: str, query: str, limit: int) -> list[SearchIndexRow]:
        """Search index.

        Args:
            node_type: Node type value.
            index_name: Index name value.
            query: Query value.
            limit: Limit value.

        Returns:
            A list containing the computed values.
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
        """Process neighbors.

        Args:
            node_id: The node id to identify.
            node_type: Node type value.
            relation: Relation value.
            direction: Direction value.
            limit: Limit value.

        Returns:
            A list containing the computed values.
        """
        ...


class LadybugGraphQueryAdapter:
    """Adapt ladybug graph query operations to the project interface."""
    def __init__(self, store: Any) -> None:
        """Initialize the instance.

        Args:
            store: The store used by the operation.
        """
        self.store = store

    def search_index(self, *, node_type: str, index_name: str, query: str, limit: int) -> list[SearchIndexRow]:
        """Search index.

        Args:
            node_type: Node type value.
            index_name: Index name value.
            query: Query value.
            limit: Limit value.

        Returns:
            A list containing the computed values.
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
        """Process neighbors.

        Args:
            node_id: The node id to identify.
            node_type: Node type value.
            relation: Relation value.
            direction: Direction value.
            limit: Limit value.

        Returns:
            A list containing the computed values.
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
            neighbors.extend(_neighbor_from_row(row, neighbor_type) for row in rows)
        return neighbors


def graph_query_adapter(store: Any) -> GraphQueryAdapter:
    """Return graph query adapter.

    Args:
        store: The store used by the operation.

    Returns:
        The computed result.
    """
    adapter = getattr(store, "graph_query_adapter", None)
    if adapter is not None:
        return adapter
    return LadybugGraphQueryAdapter(store)


def _fts_query_statement(*, node_type: str, index_name: str) -> str:
    """Process FTS query statement.

    Args:
        node_type: Node type value.
        index_name: Index name value.

    Returns:
        The computed string.
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
    """Process neighbor statement.

    Args:
        node_type: Node type value.
        neighbor_type: Neighbor type value.
        relation: Relation value.
        direction: Direction value.
        limit: Limit value.

    Returns:
        The computed string.
    """
    if direction == "outgoing":
        return (
            f"MATCH (source:{quote_identifier(node_type)} {{id: $node_id}})"
            f"-[:{quote_identifier(f'FROM_{relation}')}]->(edge:{quote_identifier(relation)})"
            f"-[:{quote_identifier(f'TO_{relation}')}]->(neighbor:{quote_identifier(neighbor_type)}) "
            "RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, "
            f"neighbor.line_start, neighbor.line_end, neighbor.summary LIMIT {int(limit)}"
        )
    return (
        f"MATCH (neighbor:{quote_identifier(neighbor_type)})"
        f"-[:{quote_identifier(f'FROM_{relation}')}]->(edge:{quote_identifier(relation)})"
        f"-[:{quote_identifier(f'TO_{relation}')}]->(target:{quote_identifier(node_type)} {{id: $node_id}}) "
        "RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, "
        f"neighbor.line_start, neighbor.line_end, neighbor.summary LIMIT {int(limit)}"
    )


def _neighbor_from_row(row: Any, node_type: str) -> GraphNeighbor:
    """Process neighbor from row.

    Args:
        row: Row value.
        node_type: Node type value.

    Returns:
        The computed result.
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
    )


def _optional_int(value: Any) -> int | None:
    """Process optional int.

    Args:
        value: Value value.

    Returns:
        The computed result.
    """
    return None if value is None else int(value)


def _text(value: Any) -> str:
    """Return text for result.

    Args:
        value: Value value.

    Returns:
        The computed string.
    """
    return "" if value is None else str(value)


def _value(row: Any, index: int) -> Any:
    """Return value for result.

    Args:
        row: Row value.
        index: Index value.

    Returns:
        The computed result.
    """
    try:
        return row[index]
    except IndexError:
        return None
