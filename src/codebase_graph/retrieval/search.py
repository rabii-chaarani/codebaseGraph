from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from codebase_graph.db import SearchIndexRow, graph_query_adapter
from codebase_graph.ontology import CONTEXT_PROFILES, SEARCH_INDEXES
from codebase_graph.reasoning.context_builder import CompactContextBuilder, ContextNode, DEFAULT_CONTEXT_BUDGET, DEFAULT_CONTEXT_LIMIT


DEFAULT_SEARCH_LIMIT = 3
MAX_CANDIDATE_LIMIT = 50
MIN_CANDIDATE_LIMIT = 10
DETAIL_LEVELS = {"standard", "slim"}
DEFINITION_TYPES = {"Class", "Function", "Method", "Variable", "Constant"}
GENERIC_TYPES = {"Symbol", "Dependency"}


@dataclass(frozen=True, slots=True)
class SearchRequest:
    """Store search request data."""
    query: str
    limit: int = DEFAULT_SEARCH_LIMIT
    profile: str = "brief"
    budget: int = DEFAULT_CONTEXT_BUDGET
    max_depth: int | None = None
    context_limit: int = DEFAULT_CONTEXT_LIMIT
    detail: str = "standard"

    def validate(self) -> None:
        """Validate the configured values."""
        if not self.query.strip():
            raise ValueError("Search query must not be empty")
        if self.limit <= 0:
            raise ValueError("Search limit must be greater than zero")
        if self.budget < 0:
            raise ValueError("Context budget must be zero or greater")
        if self.max_depth is not None and self.max_depth < 0:
            raise ValueError("Context max depth must be zero or greater")
        if self.context_limit < 0:
            raise ValueError("Context limit must be zero or greater")
        _validate_detail(self.detail)
        if self.profile not in CONTEXT_PROFILES:
            valid = ", ".join(sorted(CONTEXT_PROFILES))
            raise ValueError(f"Unknown context profile: {self.profile}. Valid profiles: {valid}")


@dataclass(slots=True)
class SearchHit:
    """Store search hit data."""
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

    def as_dict(self, *, detail: str = "standard") -> dict[str, Any]:
        """Return a JSON-serializable dictionary representation.

        Args:
            detail: Detail value.

        Returns:
            A dictionary containing the computed payload.
        """
        _validate_detail(detail)
        if detail == "slim":
            payload: dict[str, Any] = {
                "id": self.id,
                "type": self.type,
                "label": self.label,
                "rank_score": self.rank_score,
            }
            _set_non_empty(payload, "path", self.path)
            _set_non_empty(payload, "span", dict(self.span))
            _set_meaningful_summary(payload, self.summary, self.label)
            context = [node.as_dict(detail=detail) for node in self.context]
            _set_non_empty(payload, "context", context)
            return payload
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
            "context": [node.as_dict(detail=detail) for node in self.context],
        }


@dataclass(frozen=True, slots=True)
class CompactContextPayload:
    """Store compact context payload data."""
    query: str
    profile: str
    limit: int
    budget: int
    results: tuple[SearchHit, ...]

    def as_dict(self, *, detail: str = "standard") -> dict[str, Any]:
        """Return a JSON-serializable dictionary representation.

        Args:
            detail: Detail value.

        Returns:
            A dictionary containing the computed payload.
        """
        _validate_detail(detail)
        return {
            "query": self.query,
            "profile": self.profile,
            "limit": self.limit,
            "budget": self.budget,
            "results": [hit.as_dict(detail=detail) for hit in self.results],
        }


@dataclass(frozen=True, slots=True)
class FTSIndexSpec:
    """Store metadata for one full-text search index."""
    node_type: str
    index_name: str
    order: int


class SearchService:
    """Coordinate search operations."""
    def __init__(self, store: Any) -> None:
        """Initialize the instance.

        Args:
            store: The store used by the operation.
        """
        self.store = store
        self.query = graph_query_adapter(store)
        self.indexes = tuple(_fts_index_specs())

    def search(self, request: SearchRequest) -> CompactContextPayload:
        """Search graph indexes and attach compact context to top hits.

        Args:
            request: Validated search parameters and context budget.

        Returns:
            Ranked hits with optional graph context.
        """
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
                limit=request.context_limit,
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
        """Return query FTS.

        Args:
            query: Query value.
            limit: Limit value.

        Returns:
            A list containing the computed values.
        """
        hits: list[SearchHit] = []
        for spec in self.indexes:
            hits.extend(
                _hit_from_index_row(row, spec)
                for row in self.query.search_index(
                    node_type=spec.node_type,
                    index_name=spec.index_name,
                    query=query,
                    limit=limit,
                )
            )
        return hits

    def _rank_hits(self, hits: list[SearchHit], *, query: str = "", profile: str = "brief") -> list[SearchHit]:
        """Deduplicate and rank raw full-text search hits.

        Args:
            hits: Raw hits from every configured full-text index.
            query: Original user query used for lexical scoring.
            profile: Context profile used to infer ranking intent.

        Returns:
            Hits ordered by rank score and stable tie-breakers.
        """
        best_by_id: dict[str, SearchHit] = {}
        for hit in hits:
            previous = best_by_id.get(hit.id)
            if previous is None or _raw_hit_sort_key(hit) < _raw_hit_sort_key(previous):
                best_by_id[hit.id] = hit
        deduped = list(best_by_id.values())
        # A node can appear in several FTS indexes; score only the best raw hit so
        # broad indexes cannot swamp more precise label/path matches.
        _assign_rank_scores(deduped, query=query, profile=profile)
        return sorted(deduped, key=_ranked_hit_sort_key)


def _fts_index_specs() -> list[FTSIndexSpec]:
    """Process FTS index specs.

    Returns:
        A list containing the computed values.
    """
    specs: list[FTSIndexSpec] = []
    order = 0
    for index in SEARCH_INDEXES:
        index_name = str(index["name"])
        for node_type in index["node_types"]:
            specs.append(FTSIndexSpec(node_type=str(node_type), index_name=f"{index_name}_{node_type}", order=order))
            order += 1
    return specs


def _hit_from_index_row(row: SearchIndexRow, spec: FTSIndexSpec) -> SearchHit:
    """Process hit from index row.

    Args:
        row: Row value.
        spec: Spec value.

    Returns:
        The computed result.
    """
    return SearchHit(
        id=row.id,
        type=spec.node_type,
        label=row.label,
        qualified_name=row.qualified_name,
        path=row.path,
        span=_span(row.line_start, row.line_end),
        summary=row.summary,
        score=row.score,
        index_order=spec.order,
    )


def _assign_rank_scores(hits: list[SearchHit], *, query: str, profile: str) -> None:
    """Process assign rank scores.

    Args:
        hits: Hits value.
        query: Query value.
        profile: Profile value.
    """
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
    """Process candidate limit.

    Args:
        limit: Limit value.

    Returns:
        The computed integer.
    """
    return min(max(limit * 4, MIN_CANDIDATE_LIMIT), MAX_CANDIDATE_LIMIT)


def _query_intent(query: str, profile: str) -> str:
    """Return query intent.

    Args:
        query: Query value.
        profile: Profile value.

    Returns:
        The computed string.
    """
    if profile in {"dependencies", "runtime", "docs"}:
        return profile
    if _looks_like_path(query):
        return "path"
    if _looks_like_identifier(query):
        return "definition"
    return "general"


def _lexical_score(query: str, hit: SearchHit) -> float:
    """Process lexical score.

    Args:
        query: Query value.
        hit: Hit value.

    Returns:
        The computed result.
    """
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
    """Return type score.

    Args:
        node_type: Node type value.
        intent: Intent value.

    Returns:
        The computed result.
    """
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
    """Process generic penalty.

    Args:
        hit: Hit value.
        concrete_labels: Concrete labels value.

    Returns:
        The computed result.
    """
    if hit.type in GENERIC_TYPES and _normalize(hit.label) in concrete_labels:
        return 0.45
    return 0.0


def _looks_like_identifier(query: str) -> bool:
    """Process looks like identifier.

    Args:
        query: Query value.

    Returns:
        Whether the check succeeds.
    """
    cleaned = query.strip()
    return cleaned.replace("_", "").isalnum() and not cleaned[0:1].isdigit()


def _looks_like_path(query: str) -> bool:
    """Process looks like path.

    Args:
        query: Query value.

    Returns:
        Whether the check succeeds.
    """
    cleaned = query.strip()
    return "/" in cleaned or "\\" in cleaned or cleaned.endswith((".py", ".toml", ".md", ".json", ".yaml", ".yml"))


def _normalize(value: str) -> str:
    """Normalize result.

    Args:
        value: Value value.

    Returns:
        The computed string.
    """
    return value.strip().lower()


def _ranked_hit_sort_key(hit: SearchHit) -> tuple[float, int, str, str, str]:
    """Process ranked hit sort key.

    Args:
        hit: Hit value.

    Returns:
        A tuple containing the computed values.
    """
    return (-hit.rank_score, hit.index_order, hit.type, hit.path, hit.label)


def _raw_hit_sort_key(hit: SearchHit) -> tuple[float, int, str, str, str]:
    """Return raw hit sort key.

    Args:
        hit: Hit value.

    Returns:
        A tuple containing the computed values.
    """
    return (-hit.score, hit.index_order, hit.type, hit.path, hit.label)


def _span(line_start: Any, line_end: Any) -> dict[str, int]:
    """Process span.

    Args:
        line_start: Line start value.
        line_end: Line end value.

    Returns:
        A dictionary containing the computed payload.
    """
    span: dict[str, int] = {}
    if line_start is not None:
        span["line_start"] = int(line_start)
    if line_end is not None:
        span["line_end"] = int(line_end)
    return span


def _validate_detail(detail: str) -> None:
    """Validate detail.

    Args:
        detail: Detail value.
    """
    if detail not in DETAIL_LEVELS:
        valid = ", ".join(sorted(DETAIL_LEVELS))
        raise ValueError(f"Unknown detail level: {detail}. Valid levels: {valid}")


def _set_non_empty(payload: dict[str, Any], key: str, value: Any) -> None:
    """Set non empty.

    Args:
        payload: Payload to process.
        key: Key value.
        value: Value value.
    """
    if value not in ("", None, [], {}):
        payload[key] = value


def _set_meaningful_summary(payload: dict[str, Any], summary: str, label: str) -> None:
    """Set meaningful summary.

    Args:
        payload: Payload to process.
        summary: Summary value.
        label: Label value.
    """
    if summary and summary != label:
        payload["summary"] = summary


__all__ = [
    "CompactContextPayload",
    "DETAIL_LEVELS",
    "DEFAULT_SEARCH_LIMIT",
    "FTSIndexSpec",
    "MAX_CANDIDATE_LIMIT",
    "SearchHit",
    "SearchRequest",
    "SearchService",
]
