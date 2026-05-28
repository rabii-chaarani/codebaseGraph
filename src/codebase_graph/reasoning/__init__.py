"""Path explanation, causal trace, and context assembly."""

from .architecture_queries import (
    ARCHITECTURE_QUERY_GROUPS,
    ARCHITECTURE_QUERY_ORDER,
    ArchitectureQueryGroup,
    ArchitectureQuerySpec,
    architecture_query_catalog,
)
from .context_builder import CompactContextBuilder, ContextNode

__all__ = [
    "ARCHITECTURE_QUERY_GROUPS",
    "ARCHITECTURE_QUERY_ORDER",
    "ArchitectureQueryGroup",
    "ArchitectureQuerySpec",
    "CompactContextBuilder",
    "ContextNode",
    "architecture_query_catalog",
]
