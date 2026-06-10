from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from codebase_graph.ontology import ONTOLOGY_NAME, get_relation_type


@dataclass(slots=True)
class GraphNode:
    """Represent one ontology node before it is written to the database.

    The class belongs to In-memory graph primitives used before rows are persisted into Ladybug.
    """
    id: str
    table: str
    label: str
    kind: str = ""
    language: str = ""
    path: str = ""
    qualified_name: str = ""
    scope_id: str = ""
    line_start: int | None = None
    line_end: int | None = None
    byte_start: int | None = None
    byte_end: int | None = None
    tree_sitter_node_type: str = ""
    capture_name: str = ""
    summary: str = ""
    metadata: dict[str, Any] = field(default_factory=dict)

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the in-memory graph model response contract.
        """
        return {
            "id": self.id,
            "table": self.table,
            "label": self.label,
            "kind": self.kind,
            "language": self.language,
            "path": self.path,
            "qualified_name": self.qualified_name,
            "scope_id": self.scope_id,
            "line_start": self.line_start,
            "line_end": self.line_end,
            "byte_start": self.byte_start,
            "byte_end": self.byte_end,
            "tree_sitter_node_type": self.tree_sitter_node_type,
            "capture_name": self.capture_name,
            "summary": self.summary,
            "metadata": self.metadata,
        }


@dataclass(slots=True)
class GraphEdge:
    """Represent one ontology relation between two graph nodes.

    The class belongs to In-memory graph primitives used before rows are persisted into Ladybug.
    """
    id: str
    type: str
    source_id: str
    target_id: str
    kind: str = ""
    confidence: float = 1.0
    line_start: int | None = None
    line_end: int | None = None
    byte_start: int | None = None
    byte_end: int | None = None
    metadata: dict[str, Any] = field(default_factory=dict)

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the in-memory graph model response contract.
        """
        return {
            "id": self.id,
            "type": self.type,
            "source_id": self.source_id,
            "target_id": self.target_id,
            "kind": self.kind,
            "confidence": self.confidence,
            "line_start": self.line_start,
            "line_end": self.line_end,
            "byte_start": self.byte_start,
            "byte_end": self.byte_end,
            "metadata": self.metadata,
        }


@dataclass(slots=True)
class CodeGraph:
    """Collect graph nodes and relation edges for validation, serialization, and persistence.

    The class belongs to In-memory graph primitives used before rows are persisted into Ladybug.
    """
    nodes: dict[str, GraphNode] = field(default_factory=dict)
    edges: dict[str, GraphEdge] = field(default_factory=dict)
    ontology: str = ONTOLOGY_NAME
    metadata: dict[str, Any] = field(default_factory=dict)

    def add_node(self, node: GraphNode) -> GraphNode:
        """Add node for in-memory graph model.

        Args:
            node: Parser or graph node being inspected.

        Returns:
            GraphNode instance populated with data from the in-memory graph model workflow.
        """
        existing = self.nodes.get(node.id)
        if existing is None:
            self.nodes[node.id] = node
            return node
        _merge_node(existing, node)
        return existing

    def add_edge(self, edge: GraphEdge) -> GraphEdge:
        """Add edge for in-memory graph model.

        Args:
            edge: Edge used by the in-memory graph model workflow.

        Returns:
            GraphEdge instance populated with data from the in-memory graph model workflow.
        """
        self.edges.setdefault(edge.id, edge)
        return self.edges[edge.id]

    def nodes_by_type(self, table: str) -> list[GraphNode]:
        """Manage by type within in-memory graph model.

        Args:
            table: Ontology table or node type targeted by the operation.

        Returns:
            Ordered results returned to the in-memory graph model caller.
        """
        return [node for node in self.nodes.values() if node.table == table]

    def edges_by_type(self, edge_type: str) -> list[GraphEdge]:
        """Manage by type within in-memory graph model.

        Args:
            edge_type: Ontology relation type used for the edge node and connector tables.

        Returns:
            Ordered results returned to the in-memory graph model caller.
        """
        return [edge for edge in self.edges.values() if edge.type == edge_type]

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the in-memory graph model response contract.
        """
        return {
            "ontology": self.ontology,
            "metadata": self.metadata,
            "nodes": [
                node.as_dict()
                for node in sorted(self.nodes.values(), key=lambda item: (item.table, item.id))
            ],
            "edges": [
                edge.as_dict()
                for edge in sorted(self.edges.values(), key=lambda item: (item.type, item.id))
            ],
        }

    def summary(self) -> dict[str, Any]:
        """Summarize in-memory graph model for in-memory graph model.

        Returns:
            Structured mapping that follows the in-memory graph model response contract.
        """
        node_counts: dict[str, int] = {}
        edge_counts: dict[str, int] = {}
        for node in self.nodes.values():
            node_counts[node.table] = node_counts.get(node.table, 0) + 1
        for edge in self.edges.values():
            edge_counts[edge.type] = edge_counts.get(edge.type, 0) + 1
        return {
            "ontology": self.ontology,
            "node_count": len(self.nodes),
            "edge_count": len(self.edges),
            "node_counts": node_counts,
            "edge_counts": edge_counts,
        }

    def validate_schema(self) -> None:
        """Validate schema for in-memory graph model.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
        node_tables = {node.id: node.table for node in self.nodes.values()}
        for edge in self.edges.values():
            if edge.source_id not in node_tables:
                raise ValueError(f"Relation {edge.id} source is missing: {edge.source_id}")
            if edge.target_id not in node_tables:
                raise ValueError(f"Relation {edge.id} target is missing: {edge.target_id}")
            spec = get_relation_type(edge.type)
            source_table = node_tables[edge.source_id]
            target_table = node_tables[edge.target_id]
            if source_table not in spec.source_types:
                raise ValueError(f"{edge.type} cannot start from {source_table}")
            if target_table not in spec.target_types:
                raise ValueError(f"{edge.type} cannot target {target_table}")


def _merge_node(existing: GraphNode, incoming: GraphNode) -> None:
    """Merge node for in-memory graph model.

    Args:
        existing: Existing used by the in-memory graph model workflow.
        incoming: Incoming used by the in-memory graph model workflow.
    """
    for field_name in (
        "label",
        "kind",
        "language",
        "path",
        "qualified_name",
        "scope_id",
        "tree_sitter_node_type",
        "capture_name",
        "summary",
    ):
        if not getattr(existing, field_name) and getattr(incoming, field_name):
            setattr(existing, field_name, getattr(incoming, field_name))
    for field_name in ("line_start", "line_end", "byte_start", "byte_end"):
        if getattr(existing, field_name) is None and getattr(incoming, field_name) is not None:
            setattr(existing, field_name, getattr(incoming, field_name))
    existing.metadata.update(incoming.metadata)
