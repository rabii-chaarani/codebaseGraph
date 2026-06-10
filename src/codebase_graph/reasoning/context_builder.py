from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from codebase_graph.db import graph_query_adapter
from codebase_graph.ontology import CONTEXT_PROFILES, RELATION_TYPES


DEFAULT_CONTEXT_LIMIT = 3
DEFAULT_CONTEXT_BUDGET = 600


@dataclass(frozen=True, slots=True)
class ContextNode:
    """Represent context node data used by graph context and architecture-query reasoning.

    The class belongs to Compact graph-neighborhood builder used to explain search results under
    a token budget.
    """
    relation: str
    direction: str
    type: str
    label: str
    path: str = ""
    span: dict[str, int] = field(default_factory=dict)
    summary: str = ""
    id: str = field(default="", repr=False)

    def as_dict(self, *, detail: str = "standard") -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Args:
            detail: Response detail level requested by CLI or MCP callers.

        Returns:
            Structured mapping that follows the graph context and architecture-query
            reasoning response contract.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
        if detail not in {"standard", "slim"}:
            raise ValueError(f"Unknown detail level: {detail}. Valid levels: slim, standard")
        if detail == "slim":
            payload: dict[str, Any] = {
                "relation": self.relation,
                "direction": self.direction,
                "type": self.type,
                "label": self.label,
            }
            if self.path:
                payload["path"] = self.path
            if self.span:
                payload["span"] = dict(self.span)
            if self.summary and self.summary != self.label:
                payload["summary"] = self.summary
            return payload
        return {
            "relation": self.relation,
            "direction": self.direction,
            "type": self.type,
            "label": self.label,
            "path": self.path,
            "span": dict(self.span),
            "summary": self.summary,
        }


class CompactContextBuilder:
    """Traverse graph relations to produce compact context for agents."""
    def __init__(self, store: Any) -> None:
        """Initialize compact context builder with the collaborators and state it owns.

        Args:
            store: Graph store used for persistence or read-only queries.
        """
        self.store = store
        self.query = graph_query_adapter(store)
        self._relation_names = {relation_type.name for relation_type in RELATION_TYPES}

    def build(
        self,
        node_id: str,
        node_type: str,
        *,
        profile: str = "brief",
        limit: int = DEFAULT_CONTEXT_LIMIT,
        budget: int = DEFAULT_CONTEXT_BUDGET,
        max_depth: int | None = None,
    ) -> list[ContextNode]:
        """Build a budgeted graph-neighborhood explanation for a selected node.

        Args:
            node_id: Identifier for the node graph object.
            node_type: Ontology node type used to choose a table or label.
            profile: Context profile controlling graph-neighborhood traversal.
            limit: Maximum number of rows or results requested.
            budget: Approximate token budget available for compact context.
            max_depth: Optional traversal depth limit for graph context.

        Returns:
            Ordered results returned to the graph context and architecture-query reasoning
            caller.
        """
        profile_config = self._profile(profile)
        if limit <= 0 or budget <= 0:
            return []
        depth_limit = profile_config["max_depth"] if max_depth is None else max_depth
        if depth_limit <= 0:
            return []

        relations = tuple(
            relation
            for relation in profile_config["relations"]
            if relation in self._relation_names
        )
        if not relations:
            return []

        context: list[ContextNode] = []
        seen = {node_id}
        frontier = [(node_id, node_type, 0)]
        used_budget = 0

        while frontier and len(context) < limit:
            current_id, current_type, depth = frontier.pop(0)
            if depth >= depth_limit:
                continue
            for relation in relations:
                for candidate in self._neighbors(current_id, current_type, relation, limit):
                    if candidate.type == "" or candidate.label == "":
                        continue
                    candidate_key = f"{candidate.type}:{candidate.label}:{candidate.path}:{candidate.span}"
                    node_key = _node_key(candidate)
                    dedupe_key = node_key or candidate_key
                    if dedupe_key in seen:
                        continue
                    compact_candidate, item_cost = _fit_to_budget(candidate, budget - used_budget)
                    if compact_candidate is None:
                        return context
                    context.append(compact_candidate)
                    used_budget += item_cost
                    seen.add(dedupe_key)
                    if node_key:
                        frontier.append((node_key, candidate.type, depth + 1))
                    if len(context) >= limit:
                        return context
        return context

    def _profile(self, profile: str) -> dict[str, Any]:
        """Manage graph context and architecture-query reasoning state.

        Args:
            profile: Context profile controlling graph-neighborhood traversal.

        Returns:
            Structured mapping that follows the graph context and architecture-query
            reasoning response contract.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
        if profile not in CONTEXT_PROFILES:
            valid = ", ".join(sorted(CONTEXT_PROFILES))
            raise ValueError(f"Unknown context profile: {profile}. Valid profiles: {valid}")
        return dict(CONTEXT_PROFILES[profile])

    def _neighbors(self, node_id: str, node_type: str, relation: str, limit: int) -> list[ContextNode]:
        """Manage graph context and architecture-query reasoning state.

        Args:
            node_id: Identifier for the node graph object.
            node_type: Ontology node type used to choose a table or label.
            relation: Ontology relation name used for graph traversal.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the graph context and architecture-query reasoning
            caller.
        """
        outgoing = self._query_neighbors(node_id, node_type, relation, "outgoing", limit)
        incoming = self._query_neighbors(node_id, node_type, relation, "incoming", limit)
        return [*outgoing, *incoming]

    def _query_neighbors(
        self,
        node_id: str,
        node_type: str,
        relation: str,
        direction: str,
        limit: int,
    ) -> list[ContextNode]:
        """Build neighbors for graph context and architecture-query reasoning.

        Args:
            node_id: Identifier for the node graph object.
            node_type: Ontology node type used to choose a table or label.
            relation: Ontology relation name used for graph traversal.
            direction: Traversal direction relative to the source node.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the graph context and architecture-query reasoning
            caller.
        """
        return [
            ContextNode(
                relation=relation,
                direction=direction,
                type=neighbor.node_type,
                label=neighbor.label or neighbor.qualified_name,
                path=neighbor.path,
                span=_span(neighbor.line_start, neighbor.line_end),
                summary=neighbor.summary,
                id=neighbor.node_id,
            )
            for neighbor in self.query.neighbors(
                node_id=node_id,
                node_type=node_type,
                relation=relation,
                direction=direction,
                limit=limit,
            )
        ]


def _fit_to_budget(node: ContextNode, remaining_budget: int) -> tuple[ContextNode | None, int]:
    """Manage to budget within graph context and architecture-query reasoning.

    Args:
        node: Parser or graph node being inspected.
        remaining_budget: Remaining budget used by the graph context and architecture-
        query reasoning workflow.

    Returns:
        Tuple of stable results returned to the graph context and architecture-query
        reasoning caller.
    """
    cost = _context_cost(node)
    if cost <= remaining_budget:
        return node, cost
    fixed_cost = _context_cost(ContextNode(node.relation, node.direction, node.type, node.label, node.path, node.span, ""))
    summary_budget = remaining_budget - fixed_cost
    if summary_budget <= 0:
        return None, 0
    # Keep the relationship identity and source location intact, then trim only
    # the summary text because it is the least structural part of the context row.
    summary = node.summary[:summary_budget]
    compact = ContextNode(node.relation, node.direction, node.type, node.label, node.path, node.span, summary)
    return compact, _context_cost(compact)


def _context_cost(node: ContextNode) -> int:
    """Manage cost within graph context and architecture-query reasoning.

    Args:
        node: Parser or graph node being inspected.

    Returns:
        Integer count, status code, or index used by the caller.
    """
    return sum(
        len(str(value))
        for value in (
            node.relation,
            node.direction,
            node.type,
            node.label,
            node.path,
            node.summary,
            *node.span.values(),
        )
    )


def _node_key(node: ContextNode) -> str:
    """Manage key within graph context and architecture-query reasoning.

    Args:
        node: Parser or graph node being inspected.

    Returns:
        Formatted text returned to the caller.
    """
    return node.id


def _span(line_start: Any, line_end: Any) -> dict[str, int]:
    """Manage graph context and architecture-query reasoning state.

    Args:
        line_start: Start line from parser or database metadata.
        line_end: End line from parser or database metadata.

    Returns:
        Structured mapping that follows the graph context and architecture-query
        reasoning response contract.
    """
    span: dict[str, int] = {}
    if line_start is not None:
        span["line_start"] = int(line_start)
    if line_end is not None:
        span["line_end"] = int(line_end)
    return span


__all__ = ["CompactContextBuilder", "ContextNode", "DEFAULT_CONTEXT_BUDGET", "DEFAULT_CONTEXT_LIMIT"]
