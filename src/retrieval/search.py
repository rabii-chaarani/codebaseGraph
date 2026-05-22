from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from ontology import CONTEXT_PROFILES, SEARCH_INDEXES
from reasoning.context_builder import CompactContextBuilder, ContextNode, DEFAULT_CONTEXT_BUDGET, DEFAULT_CONTEXT_LIMIT


DEFAULT_SEARCH_LIMIT = 3


@dataclass(frozen=True, slots=True)
class SearchRequest:
    query: str
    limit: int = DEFAULT_SEARCH_LIMIT
    profile: str = "brief"
    budget: int = DEFAULT_CONTEXT_BUDGET
    max_depth: int | None = None

    def validate(self) -> None:
        if not self.query.strip():
            raise ValueError("Search query must not be empty")
        if self.limit <= 0:
            raise ValueError("Search limit must be greater than zero")
        if self.budget < 0:
            raise ValueError("Context budget must be zero or greater")
        if self.max_depth is not None and self.max_depth < 0:
            raise ValueError("Context max depth must be zero or greater")
        if self.profile not in CONTEXT_PROFILES:
            valid = ", ".join(sorted(CONTEXT_PROFILES))
            raise ValueError(f"Unknown context profile: {self.profile}. Valid profiles: {valid}")


@dataclass(slots=True)
class SearchHit:
    id: str
    type: str
    label: str
    qualified_name: str = ""
    path: str = ""
    span: dict[str, int] = field(default_factory=dict)
    score: float = 0.0
    summary: str = ""
    context: list[ContextNode] = field(default_factory=list)
    index_order: int = 0

    def as_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "type": self.type,
            "label": self.label,
            "qualified_name": self.qualified_name,
            "path": self.path,
            "span": dict(self.span),
            "score": self.score,
            "summary": self.summary,
            "context": [node.as_dict() for node in self.context],
        }


@dataclass(frozen=True, slots=True)
class CompactContextPayload:
    query: str
    profile: str
    limit: int
    budget: int
    results: tuple[SearchHit, ...]

    def as_dict(self) -> dict[str, Any]:
        return {
            "query": self.query,
            "profile": self.profile,
            "limit": self.limit,
            "budget": self.budget,
            "results": [hit.as_dict() for hit in self.results],
        }


@dataclass(frozen=True, slots=True)
class FTSIndexSpec:
    node_type: str
    index_name: str
    order: int


class SearchService:
    def __init__(self, store: Any) -> None:
        self.store = store
        self.indexes = tuple(_fts_index_specs())

    def search(self, request: SearchRequest) -> CompactContextPayload:
        request.validate()
        hits = self._rank_hits(self._query_fts(request.query, request.limit))
        context_builder = CompactContextBuilder(self.store)
        compact_hits: list[SearchHit] = []
        for hit in hits[: request.limit]:
            hit.context = context_builder.build(
                hit.id,
                hit.type,
                profile=request.profile,
                limit=DEFAULT_CONTEXT_LIMIT,
                budget=request.budget,
                max_depth=request.max_depth,
            )
            compact_hits.append(hit)
        return CompactContextPayload(
            query=request.query,
            profile=request.profile,
            limit=request.limit,
            budget=request.budget,
            results=tuple(compact_hits),
        )

    def _query_fts(self, query: str, limit: int) -> list[SearchHit]:
        hits: list[SearchHit] = []
        for spec in self.indexes:
            result = self.store.execute(
                _fts_query_statement(spec),
                {"query": query, "top": limit},
            )
            rows = result.get_all()
            hits.extend(_hit_from_row(row, spec) for row in rows)
        return hits

    def _rank_hits(self, hits: list[SearchHit]) -> list[SearchHit]:
        best_by_id: dict[str, SearchHit] = {}
        for hit in hits:
            previous = best_by_id.get(hit.id)
            if previous is None or _hit_sort_key(hit) < _hit_sort_key(previous):
                best_by_id[hit.id] = hit
        return sorted(best_by_id.values(), key=_hit_sort_key)


def _fts_query_statement(spec: FTSIndexSpec) -> str:
    return (
        f"CALL QUERY_FTS_INDEX('{spec.node_type}', '{spec.index_name}', $query, TOP := $top) "
        "RETURN node.id, node.label, node.qualified_name, node.path, "
        "node.line_start, node.line_end, node.summary, score"
    )


def _fts_index_specs() -> list[FTSIndexSpec]:
    specs: list[FTSIndexSpec] = []
    order = 0
    for index in SEARCH_INDEXES:
        index_name = str(index["name"])
        for node_type in index["node_types"]:
            specs.append(FTSIndexSpec(node_type=str(node_type), index_name=f"{index_name}_{node_type}", order=order))
            order += 1
    return specs


def _hit_from_row(row: Any, spec: FTSIndexSpec) -> SearchHit:
    return SearchHit(
        id=_text(_value(row, 0)),
        type=spec.node_type,
        label=_text(_value(row, 1)),
        qualified_name=_text(_value(row, 2)),
        path=_text(_value(row, 3)),
        span=_span(_value(row, 4), _value(row, 5)),
        summary=_text(_value(row, 6)),
        score=float(_value(row, 7) or 0.0),
        index_order=spec.order,
    )


def _hit_sort_key(hit: SearchHit) -> tuple[float, int, str, str, str]:
    return (-hit.score, hit.index_order, hit.type, hit.path, hit.label)


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


__all__ = [
    "CompactContextPayload",
    "DEFAULT_SEARCH_LIMIT",
    "FTSIndexSpec",
    "SearchHit",
    "SearchRequest",
    "SearchService",
]
