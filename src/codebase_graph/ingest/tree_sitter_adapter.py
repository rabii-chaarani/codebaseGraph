from __future__ import annotations

import importlib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from codebase_graph.extract import CaptureRecord, ParseBundle

from .languages import CaptureMapping, LanguageProfile, resolve_language_profile
from .tree_sitter_parser import ParserUnavailableError


@dataclass(frozen=True, slots=True)
class NormalizedSyntaxNode:
    """Language-neutral syntax node shape consumed by graph building."""

    node_type: str
    text: str = ""
    line_start: int | None = None
    line_end: int | None = None
    byte_start: int | None = None
    byte_end: int | None = None
    capture_name: str = ""
    children: tuple[NormalizedSyntaxNode, ...] = ()
    fields: dict[str, Any] = field(default_factory=dict)

    def as_dict(self) -> dict[str, Any]:
        """Return the mapping shape consumed by GraphBuilder."""
        payload: dict[str, Any] = {
            "type": self.node_type,
            "text": self.text,
            "line_start": self.line_start,
            "line_end": self.line_end,
            "byte_start": self.byte_start,
            "byte_end": self.byte_end,
            "capture_name": self.capture_name,
            "children": [child.as_dict() for child in self.children],
        }
        payload.update(self.fields)
        return payload


@dataclass(frozen=True, slots=True)
class ParserQueryResult:
    """Capture query output with diagnostics and normalized syntax nodes."""

    captures: tuple[CaptureRecord, ...]
    diagnostics: tuple[str, ...]
    syntax_nodes: tuple[NormalizedSyntaxNode, ...]


@dataclass(frozen=True, slots=True)
class TreeSitterProfiledParser:
    """Source parser backed by a LanguageProfile and tree-sitter grammar."""

    profile: LanguageProfile

    @property
    def language(self) -> str:
        """Language key reported to materialization."""
        return self.profile.language

    @property
    def parser_version(self) -> str:
        """Profile parser-version fragment used for manifest compatibility."""
        return self.profile.parser_version

    def parse_file(
        self,
        path: Path,
        *,
        relative_path: str,
        source_root: Path,
        repository_label: str,
        content_hash: str,
    ) -> ParseBundle:
        """Parse a profiled source file into a graph-builder bundle."""
        source_text = path.read_text(encoding="utf-8")
        return parse_profiled_source(
            source_text,
            self.profile,
            relative_path=relative_path,
            source_root=source_root,
            repository_label=repository_label,
            content_hash=content_hash,
        )


def create_tree_sitter_parser(profile: LanguageProfile) -> Any:
    """Create the tree-sitter parser selected by a language profile."""
    try:
        from tree_sitter import Language, Parser
    except ImportError as exc:
        raise ParserUnavailableError("Profiled parsing requires `tree-sitter`.") from exc
    try:
        grammar = importlib.import_module(profile.grammar_package)
    except ImportError as exc:
        raise ParserUnavailableError(
            f"Profiled parsing for {profile.language} requires `{profile.grammar_package}`."
        ) from exc
    raw_language = grammar.language()
    try:
        language = Language(raw_language)
    except TypeError:
        language = raw_language
    parser = Parser()
    if hasattr(parser, "set_language"):
        parser.set_language(language)
    else:
        parser.language = language
    return parser


def run_profile_queries(
    root_node: Any,
    profile: LanguageProfile,
    *,
    source_bytes: bytes = b"",
) -> ParserQueryResult:
    """Run profile capture mappings and return normalized syntax evidence."""
    normalized = normalize_syntax_node(root_node, source_bytes=source_bytes)
    diagnostics = []
    if profile.root_node_types and normalized.node_type not in profile.root_node_types:
        diagnostics.append(f"Unexpected root node {normalized.node_type} for {profile.language}")
    marked, captures = _mark_captures(normalized, profile)
    return ParserQueryResult(captures=tuple(captures), diagnostics=tuple(diagnostics), syntax_nodes=(marked,))


def normalize_syntax_node(
    raw_node: Any,
    *,
    source_bytes: bytes = b"",
    capture_name: str = "",
) -> NormalizedSyntaxNode:
    """Normalize raw grammar nodes into NormalizedSyntaxNode values."""
    if isinstance(raw_node, NormalizedSyntaxNode):
        return raw_node
    if isinstance(raw_node, dict):
        children = tuple(
            normalize_syntax_node(child, source_bytes=source_bytes)
            for child in raw_node.get("children", raw_node.get("body", ())) or ()
        )
        fields = {
            key: value
            for key, value in raw_node.items()
            if key
            not in {
                "type",
                "node_type",
                "kind",
                "text",
                "line_start",
                "line_end",
                "byte_start",
                "byte_end",
                "capture_name",
                "children",
                "body",
            }
        }
        return NormalizedSyntaxNode(
            node_type=str(raw_node.get("type") or raw_node.get("node_type") or raw_node.get("kind") or "unknown"),
            text=str(raw_node.get("text") or _first_field_label(fields) or ""),
            line_start=_optional_int(raw_node.get("line_start")),
            line_end=_optional_int(raw_node.get("line_end")),
            byte_start=_optional_int(raw_node.get("byte_start")),
            byte_end=_optional_int(raw_node.get("byte_end")),
            capture_name=str(capture_name or raw_node.get("capture_name") or ""),
            children=children,
            fields=fields,
        )
    node_type = str(getattr(raw_node, "type", "") or type(raw_node).__name__)
    children = tuple(
        normalize_syntax_node(child, source_bytes=source_bytes)
        for child in getattr(raw_node, "named_children", getattr(raw_node, "children", ())) or ()
    )
    fields = _tree_sitter_fields(raw_node, source_bytes)
    return NormalizedSyntaxNode(
        node_type=node_type,
        text=_node_text(raw_node, source_bytes) or _first_field_label(fields),
        line_start=_point_line(getattr(raw_node, "start_point", None)),
        line_end=_point_line(getattr(raw_node, "end_point", None)),
        byte_start=getattr(raw_node, "start_byte", None),
        byte_end=getattr(raw_node, "end_byte", None),
        capture_name=capture_name,
        children=children,
        fields=fields,
    )


def build_parse_bundle(
    profile: LanguageProfile,
    query_result: ParserQueryResult,
    *,
    source_text: str,
    relative_path: str,
    source_root: str | Path,
    repository_label: str,
    content_hash: str,
) -> ParseBundle:
    """Package normalized syntax and captures into the existing parse bundle contract."""
    tree = query_result.syntax_nodes[0].as_dict() if query_result.syntax_nodes else {"type": "Module", "children": []}
    return ParseBundle(
        language=profile.language,
        path=relative_path,
        source_text=source_text,
        tree=tree,
        captures=(),
        repository_label=repository_label,
        source_root=Path(source_root).as_posix(),
        content_hash=content_hash,
    )


def parse_profiled_source(
    source_text: str,
    profile: LanguageProfile,
    *,
    relative_path: str,
    source_root: str | Path,
    repository_label: str,
    content_hash: str,
) -> ParseBundle:
    """Create a parser, run profile queries, normalize syntax, and return a parse bundle."""
    parser = create_tree_sitter_parser(profile)
    source_bytes = source_text.encode("utf-8")
    tree = parser.parse(source_bytes)
    result = run_profile_queries(tree.root_node, profile, source_bytes=source_bytes)
    return build_parse_bundle(
        profile,
        result,
        source_text=source_text,
        relative_path=relative_path,
        source_root=source_root,
        repository_label=repository_label,
        content_hash=content_hash,
    )


def parser_for_profile(path_or_language: str | Path) -> TreeSitterProfiledParser | None:
    """Return a profiled parser for a language or source path when a profile exists."""
    profile = resolve_language_profile(path_or_language)
    return TreeSitterProfiledParser(profile) if profile is not None else None


def _mark_captures(
    node: NormalizedSyntaxNode,
    profile: LanguageProfile,
) -> tuple[NormalizedSyntaxNode, list[CaptureRecord]]:
    captures: list[CaptureRecord] = []
    child_pairs = [_mark_captures(child, profile) for child in node.children]
    children = tuple(child for child, _ in child_pairs)
    for _, child_captures in child_pairs:
        captures.extend(child_captures)
    mapping = _mapping_for_node_type(node.node_type, profile)
    capture_name = mapping.capture_name if mapping is not None else node.capture_name
    marked = NormalizedSyntaxNode(
        node_type=node.node_type,
        text=node.text,
        line_start=node.line_start,
        line_end=node.line_end,
        byte_start=node.byte_start,
        byte_end=node.byte_end,
        capture_name=capture_name,
        children=children,
        fields=node.fields,
    )
    if mapping is not None:
        captures.append(CaptureRecord(capture_name, marked.as_dict()))
    return marked, captures


def _mapping_for_node_type(node_type: str, profile: LanguageProfile) -> CaptureMapping | None:
    for mapping in profile.capture_mappings:
        if node_type in mapping.parser_node_types:
            return mapping
    return None


def _tree_sitter_fields(raw_node: Any, source_bytes: bytes) -> dict[str, Any]:
    fields: dict[str, Any] = {}
    child_by_field_name = getattr(raw_node, "child_by_field_name", None)
    if child_by_field_name is None:
        return fields
    for field_name in ("name", "module", "path", "function", "type", "return_type"):
        child = child_by_field_name(field_name)
        if child is not None:
            fields[field_name] = _node_text(child, source_bytes)
    return fields


def _node_text(raw_node: Any, source_bytes: bytes) -> str:
    text = getattr(raw_node, "text", None)
    if isinstance(text, bytes):
        return text.decode("utf-8", errors="replace")
    if isinstance(text, str):
        return text
    start = getattr(raw_node, "start_byte", None)
    end = getattr(raw_node, "end_byte", None)
    if isinstance(start, int) and isinstance(end, int) and source_bytes:
        return source_bytes[start:end].decode("utf-8", errors="replace")
    return ""


def _point_line(point: Any) -> int | None:
    if isinstance(point, tuple) and point:
        return int(point[0]) + 1
    row = getattr(point, "row", None)
    if row is not None:
        return int(row) + 1
    return None


def _optional_int(value: Any) -> int | None:
    return int(value) if isinstance(value, int) else None


def _first_field_label(fields: dict[str, Any]) -> str:
    for key in ("name", "id", "module", "path", "function"):
        value = fields.get(key)
        if isinstance(value, str) and value:
            return value
    return ""
