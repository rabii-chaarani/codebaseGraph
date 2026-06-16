"""Internal native acceleration wrappers.

These modules are not part of the public API. Public callers should continue to use
the Python modules that own fallback and compatibility behavior.
"""

from .graph_builder import NativeGraphBuilderUnavailable, build_file_graph

__all__ = ["NativeGraphBuilderUnavailable", "build_file_graph"]
