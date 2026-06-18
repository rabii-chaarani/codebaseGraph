"""Internal native acceleration wrappers.

These modules are not part of the public API. Public callers should use the Rust
production binary; these wrappers exist for development and compatibility imports.
"""

from .bulk_staging import NativeBulkStagingUnavailable, NativeBulkStagingResult, write_bulk_staging
from .graph_builder import NativeGraphBuilderUnavailable, build_file_graph
from .materialization import NativeMaterializationUnavailable, NativeSyntaxBatchResult, materialize_syntax_batch
from .scan_diff import NativeScanDiffResult, NativeScanDiffUnavailable, scan_repository
from .semantic_enrichment import (
    NativeSemanticBatchResult,
    NativeSemanticBatchUnavailable,
    run_semantic_batch,
)
from .tree_sitter_normalization import (
    NativeTreeSitterNormalizationUnavailable,
    normalize_profiled_syntax,
)

__all__ = [
    "NativeBulkStagingResult",
    "NativeBulkStagingUnavailable",
    "NativeGraphBuilderUnavailable",
    "NativeMaterializationUnavailable",
    "NativeScanDiffResult",
    "NativeScanDiffUnavailable",
    "NativeSemanticBatchResult",
    "NativeSemanticBatchUnavailable",
    "NativeSyntaxBatchResult",
    "NativeTreeSitterNormalizationUnavailable",
    "build_file_graph",
    "materialize_syntax_batch",
    "normalize_profiled_syntax",
    "run_semantic_batch",
    "scan_repository",
    "write_bulk_staging",
]
