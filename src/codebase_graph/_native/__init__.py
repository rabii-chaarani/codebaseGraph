"""Internal native acceleration wrappers.

These modules are not part of the public API. Public callers should continue to use
the Python modules that own fallback and compatibility behavior.
"""

from .bulk_staging import NativeBulkStagingUnavailable, NativeBulkStagingResult, write_bulk_staging
from .graph_builder import NativeGraphBuilderUnavailable, build_file_graph
from .tree_sitter_normalization import (
    NativeTreeSitterNormalizationUnavailable,
    normalize_profiled_syntax,
)

__all__ = [
    "NativeBulkStagingResult",
    "NativeBulkStagingUnavailable",
    "NativeGraphBuilderUnavailable",
    "NativeTreeSitterNormalizationUnavailable",
    "build_file_graph",
    "normalize_profiled_syntax",
    "write_bulk_staging",
]
