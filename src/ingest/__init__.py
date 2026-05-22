"""Repository, documentation, issue, and tool-output ingestion."""

from .materializer import (
    GraphMaterializer,
    ManifestDiff,
    ManifestEntry,
    MaterializationManifest,
    MaterializationResult,
    MaterializeMode,
    SourceSnapshot,
)
from .tree_sitter_parser import ParserUnavailableError, TreeSitterPythonParser, parser_for_language

__all__ = [
    "GraphMaterializer",
    "ManifestDiff",
    "ManifestEntry",
    "MaterializationManifest",
    "MaterializationResult",
    "MaterializeMode",
    "ParserUnavailableError",
    "SourceSnapshot",
    "TreeSitterPythonParser",
    "parser_for_language",
]
