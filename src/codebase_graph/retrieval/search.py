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
    """Represent search request data used by search, ranking, and block-format retrieval.

    The class belongs to FTS-backed graph search with rank scoring and compact context assembly.
    """
    query: str
    limit: int = DEFAULT_SEARCH_LIMIT
    profile: str = "brief"
    budget: int = DEFAULT_CONTEXT_BUDGET
    max_depth: int | None = None
    context_limit: int = DEFAULT_CONTEXT_LIMIT
    detail: str = "standard"

    def validate(self) -> None:
        """Validate search, ranking, and block-format retrieval for search, ranking, and block-format retrieval.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
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
    """Represent search hit data used by search, ranking, and block-format retrieval.

    The class belongs to FTS-backed graph search with rank scoring and compact context assembly.
    """
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
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Args:
            detail: Response detail level requested by CLI or MCP callers.

        Returns:
            Structured mapping that follows the search, ranking, and block-format
            retrieval response contract.
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
    """Represent compact context payload data used by search, ranking, and block-format retrieval.

    The class belongs to FTS-backed graph search with rank scoring and compact context assembly.
    """
    query: str
    profile: str
    limit: int
    budget: int
    results: tuple[SearchHit, ...]

    def as_dict(self, *, detail: str = "standard") -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Args:
            detail: Response detail level requested by CLI or MCP callers.

        Returns:
            Structured mapping that follows the search, ranking, and block-format
            retrieval response contract.
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
    """Describe a declared f t s index used by search, ranking, and block-format retrieval."""
    node_type: str
    index_name: str
    order: int


class SearchService:
    """Manage FTS-backed search and compact context assembly."""
    def __init__(self, store: Any) -> None:
        """Initialize search service with the collaborators and state it owns.

        Args:
            store: Graph store used for persistence or read-only queries.
        """
        self.store = store
        self.query = graph_query_adapter(store)
        self.indexes = tuple(_fts_index_specs())

    def search(self, request: SearchRequest) -> CompactContextPayload:
        """Run full-text search, rank deduplicated hits, and attach compact graph context.

        Args:
            request: Validated request object carrying query and context settings.

        Returns:
            CompactContextPayload instance populated with data from the search, ranking, and
            block-format retrieval workflow.
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
            # Context is attached only after ranking so expensive graph traversal
            # runs for visible results, not every raw FTS candidate.
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
        """Build full-text search for search, ranking, and block-format retrieval.

        Args:
            query: User search text or read-only Cypher statement.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the search, ranking, and block-format retrieval
            caller.
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
        """Collapse duplicate FTS rows and apply domain-specific rank scoring.

        Args:
            hits: Hits used by the search, ranking, and block-format retrieval
            workflow.
            query: User search text or read-only Cypher statement.
            profile: Context profile controlling graph-neighborhood traversal.

        Returns:
            Ordered results returned to the search, ranking, and block-format retrieval
            caller.
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
    """Manage index specs within search, ranking, and block-format retrieval.

    Returns:
        Ordered results returned to the search, ranking, and block-format retrieval caller.
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
    """Manage from index row within search, ranking, and block-format retrieval.

    Args:
        row: Database row returned by Ladybug.
        spec: Spec used by the search, ranking, and block-format retrieval workflow.

    Returns:
        SearchHit instance populated with data from the search, ranking, and block-format
        retrieval workflow.
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
    """Manage rank scores within search, ranking, and block-format retrieval.

    Args:
        hits: Hits used by the search, ranking, and block-format retrieval workflow.
        query: User search text or read-only Cypher statement.
        profile: Context profile controlling graph-neighborhood traversal.
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
    """Manage limit within search, ranking, and block-format retrieval.

    Args:
        limit: Maximum number of rows or results requested.

    Returns:
        Integer count, status code, or index used by the caller.
    """
    return min(max(limit * 4, MIN_CANDIDATE_LIMIT), MAX_CANDIDATE_LIMIT)


def _query_intent(query: str, profile: str) -> str:
    """Build intent for search, ranking, and block-format retrieval.

    Args:
        query: User search text or read-only Cypher statement.
        profile: Context profile controlling graph-neighborhood traversal.

    Returns:
        Formatted text returned to the caller.
    """
    if profile in {"dependencies", "runtime", "docs"}:
        return profile
    if _looks_like_path(query):
        return "path"
    if _looks_like_identifier(query):
        return "definition"
    return "general"


def _lexical_score(query: str, hit: SearchHit) -> float:
    """Manage score within search, ranking, and block-format retrieval.

    Args:
        query: User search text or read-only Cypher statement.
        hit: Hit used by the search, ranking, and block-format retrieval workflow.

    Returns:
        Numeric score used for ranking or reporting.
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
    """Return score for search, ranking, and block-format retrieval.

    Args:
        node_type: Ontology node type used to choose a table or label.
        intent: Intent used by the search, ranking, and block-format retrieval
        workflow.

    Returns:
        Numeric score used for ranking or reporting.
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
    """Manage penalty within search, ranking, and block-format retrieval.

    Args:
        hit: Hit used by the search, ranking, and block-format retrieval workflow.
        concrete_labels: Concrete labels used by the search, ranking, and block-format
        retrieval workflow.

    Returns:
        Numeric score used for ranking or reporting.
    """
    if hit.type in GENERIC_TYPES and _normalize(hit.label) in concrete_labels:
        return 0.45
    return 0.0


def _looks_like_identifier(query: str) -> bool:
    """Manage like identifier within search, ranking, and block-format retrieval.

    Args:
        query: User search text or read-only Cypher statement.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    cleaned = query.strip()
    return cleaned.replace("_", "").isalnum() and not cleaned[0:1].isdigit()


def _looks_like_path(query: str) -> bool:
    """Manage like path within search, ranking, and block-format retrieval.

    Args:
        query: User search text or read-only Cypher statement.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    cleaned = query.strip()
    return "/" in cleaned or "\\" in cleaned or cleaned.endswith((".py", ".toml", ".md", ".json", ".yaml", ".yml"))


def _normalize(value: str) -> str:
    """Normalize search, ranking, and block-format retrieval for search, ranking, and block-format retrieval.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Formatted text returned to the caller.
    """
    return value.strip().lower()


def _ranked_hit_sort_key(hit: SearchHit) -> tuple[float, int, str, str, str]:
    """Manage hit sort key within search, ranking, and block-format retrieval.

    Args:
        hit: Hit used by the search, ranking, and block-format retrieval workflow.

    Returns:
        Tuple of stable results returned to the search, ranking, and block-format retrieval
        caller.
    """
    return (-hit.rank_score, hit.index_order, hit.type, hit.path, hit.label)


def _raw_hit_sort_key(hit: SearchHit) -> tuple[float, int, str, str, str]:
    """Return hit sort key for search, ranking, and block-format retrieval.

    Args:
        hit: Hit used by the search, ranking, and block-format retrieval workflow.

    Returns:
        Tuple of stable results returned to the search, ranking, and block-format retrieval
        caller.
    """
    return (-hit.score, hit.index_order, hit.type, hit.path, hit.label)


def _span(line_start: Any, line_end: Any) -> dict[str, int]:
    """Manage search, ranking, and block-format retrieval within search, ranking, and block-format retrieval.

    Args:
        line_start: Start line from parser or database metadata.
        line_end: End line from parser or database metadata.

    Returns:
        Structured mapping that follows the search, ranking, and block-format retrieval
        response contract.
    """
    span: dict[str, int] = {}
    if line_start is not None:
        span["line_start"] = int(line_start)
    if line_end is not None:
        span["line_end"] = int(line_end)
    return span


def _validate_detail(detail: str) -> None:
    """Validate detail for search, ranking, and block-format retrieval.

    Args:
        detail: Response detail level requested by CLI or MCP callers.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    if detail not in DETAIL_LEVELS:
        valid = ", ".join(sorted(DETAIL_LEVELS))
        raise ValueError(f"Unknown detail level: {detail}. Valid levels: {valid}")


def _set_non_empty(payload: dict[str, Any], key: str, value: Any) -> None:
    """Set non empty for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.
        key: Key used by the search, ranking, and block-format retrieval workflow.
        value: Input being normalized for serialization or validation.
    """
    if value not in ("", None, [], {}):
        payload[key] = value


def _set_meaningful_summary(payload: dict[str, Any], summary: str, label: str) -> None:
    """Set meaningful summary for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.
        summary: Summary used by the search, ranking, and block-format retrieval
        workflow.
        label: Human-readable label stored on a graph node or edge.
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
