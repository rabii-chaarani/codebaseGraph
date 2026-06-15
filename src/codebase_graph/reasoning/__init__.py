"""Path explanation, causal trace, and context assembly."""

from .architecture_queries import (
    ARCHITECTURE_QUERY_GROUPS,
    ARCHITECTURE_QUERY_ORDER,
    ArchitectureQueryGroup,
    ArchitectureQuerySpec,
    architecture_query_catalog,
)
from .context_builder import (
    CompactContextBuilder,
    ContextEdge,
    ContextNode,
    ContextPath,
    ContextPathNode,
    DEFAULT_CONTEXT_LIMIT,
    SemanticRelationAnnotation,
    SourceSnippet,
    collect_source_snippet,
    estimate_token_count,
    extract_semantic_relation_annotations,
    prioritize_semantic_context,
    redact_source_snippet,
)
from .context_profiles import (
    ContextProfileSpec,
    builtin_context_profiles,
    load_context_profile_config,
    merge_context_profiles,
    validate_context_profile,
)

__all__ = [
    "ARCHITECTURE_QUERY_GROUPS",
    "ARCHITECTURE_QUERY_ORDER",
    "ArchitectureQueryGroup",
    "ArchitectureQuerySpec",
    "CompactContextBuilder",
    "ContextEdge",
    "ContextNode",
    "ContextPath",
    "ContextPathNode",
    "ContextProfileSpec",
    "DEFAULT_CONTEXT_LIMIT",
    "SemanticRelationAnnotation",
    "SourceSnippet",
    "architecture_query_catalog",
    "builtin_context_profiles",
    "collect_source_snippet",
    "estimate_token_count",
    "extract_semantic_relation_annotations",
    "load_context_profile_config",
    "merge_context_profiles",
    "prioritize_semantic_context",
    "redact_source_snippet",
    "validate_context_profile",
]
