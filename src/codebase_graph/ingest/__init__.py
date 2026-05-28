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
from .tree_sitter_parser import (
    ParserRegistry,
    ParserRegistration,
    ParserUnavailableError,
    SourceParser,
    TreeSitterPythonParser,
    default_parser_registry,
    parser_for_language,
)
from .document_parser import MarkdownDocumentParser

__all__ = [
    "GraphMaterializer",
    "MarkdownDocumentParser",
    "ManifestDiff",
    "ManifestEntry",
    "MaterializationManifest",
    "MaterializationResult",
    "MaterializeMode",
    "ParserRegistry",
    "ParserRegistration",
    "ParserUnavailableError",
    "SourceParser",
    "SourceSnapshot",
    "TreeSitterPythonParser",
    "default_parser_registry",
    "parser_for_language",
]
