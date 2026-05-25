from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from codebase_graph.ontology import CONTEXT_PROFILES, SEARCH_INDEXES
from codebase_graph.reasoning.context_builder import CompactContextBuilder, ContextNode, DEFAULT_CONTEXT_BUDGET, DEFAULT_CONTEXT_LIMIT


DEFAULT_SEARCH_LIMIT = 3
MAX_CANDIDATE_LIMIT = 50
MIN_CANDIDATE_LIMIT = 10
DEFINITION_TYPES = {"Class", "Function", "Method", "Variable", "Constant"}
GENERIC_TYPES = {"Symbol", "Dependency"}


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
    rank_score: float = 0.0
    score_components: dict[str, float] = field(default_factory=dict)
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
            "rank_score": self.rank_score,
            "score_components": dict(self.score_components),
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
        candidate_limit = _candidate_limit(request.limit)
        hits = self._rank_hits(
            self._query_fts(request.query, candidate_limit),
            query=request.query,
            profile=request.profile,
        )
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

    def _rank_hits(self, hits: list[SearchHit], *, query: str = "", profile: str = "brief") -> list[SearchHit]:
        best_by_id: dict[str, SearchHit] = {}
        for hit in hits:
            previous = best_by_id.get(hit.id)
            if previous is None or _raw_hit_sort_key(hit) < _raw_hit_sort_key(previous):
                best_by_id[hit.id] = hit
        deduped = list(best_by_id.values())
        _assign_rank_scores(deduped, query=query, profile=profile)
        return sorted(deduped, key=_ranked_hit_sort_key)


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


def _assign_rank_scores(hits: list[SearchHit], *, query: str, profile: str) -> None:
    if not hits:
        return
    max_score = max((hit.score for hit in hits), default=0.0)
    concrete_labels = {
        _normalize(hit.label)
        for hit in hits
        if hit.type in DEFINITION_TYPES and hit.label
    }
    intent = _query_intent(query, profile)

    for hit in hits:
        fts_score = hit.score / max_score if max_score > 0 else 0.0
        lexical_score = _lexical_score(query, hit)
        type_score = _type_score(hit.type, intent)
        generic_penalty = _generic_penalty(hit, concrete_labels)
        rank_score = (0.45 * fts_score) + (0.35 * lexical_score) + type_score - generic_penalty
        hit.score_components = {
            "fts": round(fts_score, 6),
            "lexical": round(lexical_score, 6),
            "type": round(type_score, 6),
            "generic_penalty": round(generic_penalty, 6),
        }
        hit.rank_score = round(rank_score, 6)


def _candidate_limit(limit: int) -> int:
    return min(max(limit * 4, MIN_CANDIDATE_LIMIT), MAX_CANDIDATE_LIMIT)


def _query_intent(query: str, profile: str) -> str:
    if profile in {"dependencies", "runtime", "docs"}:
        return profile
    if _looks_like_path(query):
        return "path"
    if _looks_like_identifier(query):
        return "definition"
    return "general"


def _lexical_score(query: str, hit: SearchHit) -> float:
    normalized_query = _normalize(query)
    if not normalized_query:
        return 0.0
    label = _normalize(hit.label)
    qualified_name = _normalize(hit.qualified_name)
    path = _normalize(hit.path)

    if label == normalized_query:
        return 1.0
    if qualified_name == normalized_query:
        return 0.95
    if qualified_name.endswith(f".{normalized_query}") or qualified_name.endswith(f"/{normalized_query}"):
        return 0.85
    if path == normalized_query or path.endswith(f"/{normalized_query}"):
        return 0.8
    if normalized_query in label:
        return 0.55
    if normalized_query in qualified_name:
        return 0.45
    if normalized_query in path:
        return 0.35
    return 0.0


def _type_score(node_type: str, intent: str) -> float:
    if intent == "definition":
        if node_type in {"Class", "Function", "Method"}:
            return 0.7
        if node_type in {"Variable", "Constant"}:
            return 0.6
        if node_type == "Module":
            return 0.2
        return 0.0
    if intent == "path":
        return {"File": 0.7, "Module": 0.6, "SourceRoot": 0.25, "Repository": 0.2}.get(node_type, 0.0)
    if intent == "dependencies":
        return {"Dependency": 0.7, "ImportDeclaration": 0.65, "Module": 0.2}.get(node_type, 0.0)
    if intent == "runtime":
        return {"APIEndpoint": 0.7, "Route": 0.65, "Component": 0.55, "Query": 0.45, "SecretRef": 0.35}.get(node_type, 0.0)
    if intent == "docs":
        return {"DocumentationSource": 0.7, "DocumentationChunk": 0.65}.get(node_type, 0.0)
    if node_type in DEFINITION_TYPES:
        return 0.25
    return 0.0


def _generic_penalty(hit: SearchHit, concrete_labels: set[str]) -> float:
    if hit.type in GENERIC_TYPES and _normalize(hit.label) in concrete_labels:
        return 0.45
    return 0.0


def _looks_like_identifier(query: str) -> bool:
    cleaned = query.strip()
    return cleaned.replace("_", "").isalnum() and not cleaned[0:1].isdigit()


def _looks_like_path(query: str) -> bool:
    cleaned = query.strip()
    return "/" in cleaned or "\\" in cleaned or cleaned.endswith((".py", ".toml", ".md", ".json", ".yaml", ".yml"))


def _normalize(value: str) -> str:
    return value.strip().lower()


def _ranked_hit_sort_key(hit: SearchHit) -> tuple[float, int, str, str, str]:
    return (-hit.rank_score, hit.index_order, hit.type, hit.path, hit.label)


def _raw_hit_sort_key(hit: SearchHit) -> tuple[float, int, str, str, str]:
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
    "MAX_CANDIDATE_LIMIT",
    "SearchHit",
    "SearchRequest",
    "SearchService",
]
