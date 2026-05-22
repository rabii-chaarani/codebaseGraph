from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from db.schema import quote_identifier
from ontology import CONTEXT_PROFILES, RELATION_TYPES


DEFAULT_CONTEXT_LIMIT = 3
DEFAULT_CONTEXT_BUDGET = 600


@dataclass(frozen=True, slots=True)
class ContextNode:
    relation: str
    direction: str
    type: str
    label: str
    path: str = ""
    span: dict[str, int] = field(default_factory=dict)
    summary: str = ""
    id: str = field(default="", repr=False)

    def as_dict(self) -> dict[str, Any]:
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
    def __init__(self, store: Any) -> None:
        self.store = store
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
        if profile not in CONTEXT_PROFILES:
            valid = ", ".join(sorted(CONTEXT_PROFILES))
            raise ValueError(f"Unknown context profile: {profile}. Valid profiles: {valid}")
        return dict(CONTEXT_PROFILES[profile])

    def _neighbors(self, node_id: str, node_type: str, relation: str, limit: int) -> list[ContextNode]:
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
        if direction == "outgoing":
            statement = (
                f"MATCH (source:{quote_identifier(node_type)} {{id: $node_id}})"
                f"-[:{quote_identifier(f'FROM_{relation}')}]->(edge:{quote_identifier(relation)})"
                f"-[:{quote_identifier(f'TO_{relation}')}]->(neighbor) "
                "RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, "
                f"neighbor.line_start, neighbor.line_end, neighbor.summary LIMIT {int(limit)}"
            )
        else:
            statement = (
                "MATCH (neighbor)"
                f"-[:{quote_identifier(f'FROM_{relation}')}]->(edge:{quote_identifier(relation)})"
                f"-[:{quote_identifier(f'TO_{relation}')}]->(target:{quote_identifier(node_type)} {{id: $node_id}}) "
                "RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, "
                f"neighbor.line_start, neighbor.line_end, neighbor.summary LIMIT {int(limit)}"
            )
        rows = self.store.execute(statement, {"node_id": node_id}).get_all()
        return [
            ContextNode(
                relation=relation,
                direction=direction,
                type=_type_from_id(_value(row, 0)),
                label=_text(_value(row, 1)) or _text(_value(row, 2)),
                path=_text(_value(row, 3)),
                span=_span(_value(row, 4), _value(row, 5)),
                summary=_text(_value(row, 6)),
                id=_text(_value(row, 0)),
            )
            for row in rows
        ]


def _fit_to_budget(node: ContextNode, remaining_budget: int) -> tuple[ContextNode | None, int]:
    cost = _context_cost(node)
    if cost <= remaining_budget:
        return node, cost
    fixed_cost = _context_cost(ContextNode(node.relation, node.direction, node.type, node.label, node.path, node.span, ""))
    summary_budget = remaining_budget - fixed_cost
    if summary_budget <= 0:
        return None, 0
    summary = node.summary[:summary_budget]
    compact = ContextNode(node.relation, node.direction, node.type, node.label, node.path, node.span, summary)
    return compact, _context_cost(compact)


def _context_cost(node: ContextNode) -> int:
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
    return node.id


def _type_from_id(value: Any) -> str:
    text = _text(value)
    if ":" not in text:
        return ""
    return text.split(":", 1)[0]


def _span(line_start: Any, line_end: Any) -> dict[str, int]:
    span: dict[str, int] = {}
    if line_start is not None:
        span["line_start"] = int(line_start)
    if line_end is not None:
        span["line_end"] = int(line_end)
    return span


def _text(value: Any) -> str:
    return "" if value is None else str(value)


def _value(row: Any, index: int) -> Any:
    try:
        return row[index]
    except IndexError:
        return None


__all__ = ["CompactContextBuilder", "ContextNode", "DEFAULT_CONTEXT_BUDGET", "DEFAULT_CONTEXT_LIMIT"]
