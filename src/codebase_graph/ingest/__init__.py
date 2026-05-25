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
from .document_parser import MarkdownDocumentParser

__all__ = [
    "GraphMaterializer",
    "MarkdownDocumentParser",
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
