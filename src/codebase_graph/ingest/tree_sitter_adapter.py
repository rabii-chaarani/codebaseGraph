from __future__ import annotations

import importlib
import json
import os
from collections.abc import Iterable
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
    native_result = _run_native_profile_queries(root_node, profile, source_bytes=source_bytes)
    if native_result is not None:
        return native_result
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


def _run_native_profile_queries(
    root_node: Any,
    profile: LanguageProfile,
    *,
    source_bytes: bytes,
) -> ParserQueryResult | None:
    if os.environ.get("CODEBASE_GRAPH_NATIVE") != "1":
        return None
    try:
        from codebase_graph._native.tree_sitter_normalization import normalize_profiled_syntax

        output = normalize_profiled_syntax(
            _encode_native_profiled_syntax(root_node, profile, source_bytes=source_bytes)
        )
        if output is None:
            return None
        return _decode_native_profiled_syntax(output)
    except Exception:
        return None


def _encode_native_profiled_syntax(
    root_node: Any,
    profile: LanguageProfile,
    *,
    source_bytes: bytes,
) -> str:
    lines = [
        "TSNORM",
        f"LANGUAGE\t{_hex(profile.language)}",
        _encode_native_hex_list("ROOT_TYPES", profile.root_node_types),
    ]
    for mapping in profile.capture_mappings:
        lines.append(
            "\t".join(
                (
                    "MAPPING",
                    _hex(mapping.capture_name),
                    _hex(mapping.context_rule),
                    str(len(mapping.parser_node_types)),
                    *(_hex(node_type) for node_type in mapping.parser_node_types),
                )
            )
        )
    _append_native_node_records(root_node, source_bytes=source_bytes, lines=lines, parent_id=None, next_id=[0])
    return "\n".join(lines) + "\n"


def _append_native_node_records(
    raw_node: Any,
    *,
    source_bytes: bytes,
    lines: list[str],
    parent_id: int | None,
    next_id: list[int],
) -> int:
    node_id = next_id[0]
    next_id[0] += 1
    node_type, text, line_start, line_end, byte_start, byte_end, capture_name, fields, children = _native_node_parts(
        raw_node,
        source_bytes=source_bytes,
    )
    fields = dict(fields)
    field_types = fields.get("_field_types", {})
    field_descendant_types = fields.get("_field_descendant_types", {})
    lines.append(
        "\t".join(
            (
                "NODE",
                str(node_id),
                "" if parent_id is None else str(parent_id),
                _hex(node_type),
                _hex(text),
                _int_field(line_start),
                _int_field(line_end),
                _int_field(byte_start),
                _int_field(byte_end),
                _hex(capture_name),
                *_encode_native_field_tokens(fields),
                *_encode_native_string_map(field_types if isinstance(field_types, dict) else {}),
                *_encode_native_string_list_map(
                    field_descendant_types if isinstance(field_descendant_types, dict) else {}
                ),
            )
        )
    )
    for child in children:
        _append_native_node_records(
            child,
            source_bytes=source_bytes,
            lines=lines,
            parent_id=node_id,
            next_id=next_id,
        )
    return node_id


def _native_node_parts(
    raw_node: Any,
    *,
    source_bytes: bytes,
) -> tuple[
    str,
    str,
    int | None,
    int | None,
    int | None,
    int | None,
    str,
    dict[str, Any],
    Iterable[Any],
]:
    if isinstance(raw_node, NormalizedSyntaxNode):
        return (
            raw_node.node_type,
            raw_node.text or _first_field_label(raw_node.fields),
            raw_node.line_start,
            raw_node.line_end,
            raw_node.byte_start,
            raw_node.byte_end,
            raw_node.capture_name,
            raw_node.fields,
            raw_node.children,
        )
    if isinstance(raw_node, dict):
        children = raw_node.get("children", raw_node.get("body", ())) or ()
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
        return (
            str(raw_node.get("type") or raw_node.get("node_type") or raw_node.get("kind") or "unknown"),
            str(raw_node.get("text") or _first_field_label(fields) or ""),
            _optional_int(raw_node.get("line_start")),
            _optional_int(raw_node.get("line_end")),
            _optional_int(raw_node.get("byte_start")),
            _optional_int(raw_node.get("byte_end")),
            str(raw_node.get("capture_name") or ""),
            fields,
            children,
        )
    fields = _tree_sitter_fields(raw_node, source_bytes)
    return (
        str(getattr(raw_node, "type", "") or type(raw_node).__name__),
        _node_text(raw_node, source_bytes) or _first_field_label(fields),
        _point_line(getattr(raw_node, "start_point", None)),
        _point_line(getattr(raw_node, "end_point", None)),
        getattr(raw_node, "start_byte", None),
        getattr(raw_node, "end_byte", None),
        "",
        fields,
        getattr(raw_node, "named_children", getattr(raw_node, "children", ())) or (),
    )


def _decode_native_profiled_syntax(output: str) -> ParserQueryResult:
    root_id = 0
    diagnostics: list[str] = []
    captures: list[tuple[str, int]] = []
    raw_nodes: dict[int, dict[str, Any]] = {}
    children_by_parent: dict[int | None, list[int]] = {}

    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        match parts[0]:
            case "RESULT":
                root_id = int(parts[1])
            case "DIAG":
                diagnostics.append(_unhex(parts[1]))
            case "NODE":
                node_id, parent_id, payload = _decode_native_node(parts[1:])
                raw_nodes[node_id] = payload
                children_by_parent.setdefault(parent_id, []).append(node_id)
            case "CAP":
                captures.append((_unhex(parts[1]), int(parts[2])))
            case other:
                raise ValueError(f"Unknown native tree-sitter normalization record: {other}")

    built_nodes: dict[int, NormalizedSyntaxNode] = {}

    def build(node_id: int) -> NormalizedSyntaxNode:
        if node_id in built_nodes:
            return built_nodes[node_id]
        payload = raw_nodes[node_id]
        node = NormalizedSyntaxNode(
            node_type=payload["node_type"],
            text=payload["text"],
            line_start=payload["line_start"],
            line_end=payload["line_end"],
            byte_start=payload["byte_start"],
            byte_end=payload["byte_end"],
            capture_name=payload["capture_name"],
            children=tuple(build(child_id) for child_id in children_by_parent.get(node_id, ())),
            fields=payload["fields"],
        )
        built_nodes[node_id] = node
        return node

    root = build(root_id)
    capture_records = tuple(CaptureRecord(capture_name, build(node_id).as_dict()) for capture_name, node_id in captures)
    return ParserQueryResult(captures=capture_records, diagnostics=tuple(diagnostics), syntax_nodes=(root,))


def _decode_native_node(parts: list[str]) -> tuple[int, int | None, dict[str, Any]]:
    node_id = int(parts[0])
    parent_id = int(parts[1]) if parts[1] else None
    field_count = int(parts[9])
    cursor = 10
    fields: dict[str, Any] = {}
    for _ in range(field_count):
        key = _unhex(parts[cursor])
        value = json.loads(_unhex(parts[cursor + 1]))
        fields[key] = value
        cursor += 2
    return (
        node_id,
        parent_id,
        {
            "node_type": _unhex(parts[2]),
            "text": _unhex(parts[3]),
            "line_start": _int(parts[4]),
            "line_end": _int(parts[5]),
            "byte_start": _int(parts[6]),
            "byte_end": _int(parts[7]),
            "capture_name": _unhex(parts[8]),
            "fields": fields,
        },
    )


def _encode_native_hex_list(record_type: str, values: Iterable[str]) -> str:
    value_list = tuple(values)
    return "\t".join((record_type, str(len(value_list)), *(_hex(value) for value in value_list)))


def _encode_native_field_tokens(fields: dict[str, Any]) -> tuple[str, ...]:
    encoded = [str(len(fields))]
    for key, value in fields.items():
        encoded.extend((_hex(key), _hex(json.dumps(value, separators=(",", ":"), sort_keys=True))))
    return tuple(encoded)


def _encode_native_string_map(values: dict[Any, Any]) -> tuple[str, ...]:
    encoded = [str(len(values))]
    for key, value in values.items():
        encoded.extend((_hex(str(key)), _hex(str(value))))
    return tuple(encoded)


def _encode_native_string_list_map(values: dict[Any, Any]) -> tuple[str, ...]:
    encoded = [str(len(values))]
    for key, raw_items in values.items():
        items = tuple(str(item) for item in raw_items) if isinstance(raw_items, list | tuple) else ()
        encoded.extend((_hex(str(key)), str(len(items)), *(_hex(item) for item in items)))
    return tuple(encoded)


def _hex(value: str) -> str:
    return value.encode("utf-8").hex()


def _unhex(value: str) -> str:
    return bytes.fromhex(value).decode("utf-8")


def _int_field(value: int | None) -> str:
    return "" if value is None else str(value)


def _int(value: str) -> int | None:
    return int(value) if value else None


def _mark_captures(
    node: NormalizedSyntaxNode,
    profile: LanguageProfile,
    ancestors: tuple[str, ...] = (),
) -> tuple[NormalizedSyntaxNode, list[CaptureRecord]]:
    captures: list[CaptureRecord] = []
    child_pairs = [_mark_captures(child, profile, (*ancestors, node.node_type)) for child in node.children]
    children = tuple(child for child, _ in child_pairs)
    for _, child_captures in child_pairs:
        captures.extend(child_captures)
    mapping = _mapping_for_node(node, profile, ancestors)
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


def _mapping_for_node(
    node: NormalizedSyntaxNode,
    profile: LanguageProfile,
    ancestors: tuple[str, ...],
) -> CaptureMapping | None:
    candidates = [mapping for mapping in profile.capture_mappings if node.node_type in mapping.parser_node_types]
    for mapping in candidates:
        if mapping.context_rule and _context_rule_matches(mapping.context_rule, node, ancestors):
            return mapping
    for mapping in candidates:
        if not mapping.context_rule:
            return mapping
    return None


def _tree_sitter_fields(raw_node: Any, source_bytes: bytes) -> dict[str, Any]:
    fields: dict[str, Any] = {}
    field_types: dict[str, str] = {}
    field_descendant_types: dict[str, tuple[str, ...]] = {}
    child_by_field_name = getattr(raw_node, "child_by_field_name", None)
    if child_by_field_name is not None:
        for field_name in ("name", "module", "path", "function", "type", "return_type", "declarator"):
            child = child_by_field_name(field_name)
            if child is None:
                continue
            field_types[field_name] = str(getattr(child, "type", ""))
            field_descendant_types[field_name] = tuple(sorted(_node_types(child)))
            if field_name != "declarator":
                fields[field_name] = _clean_label(_node_text(child, source_bytes))
        _augment_field_metadata(raw_node, source_bytes, fields, field_types, field_descendant_types)
    if field_types:
        fields["_field_types"] = field_types
    if field_descendant_types:
        fields["_field_descendant_types"] = {
            name: list(types)
            for name, types in field_descendant_types.items()
        }
    return fields


def _augment_field_metadata(
    raw_node: Any,
    source_bytes: bytes,
    fields: dict[str, Any],
    field_types: dict[str, str],
    field_descendant_types: dict[str, tuple[str, ...]],
) -> None:
    node_type = str(getattr(raw_node, "type", ""))
    if "name" not in fields:
        name = _derived_name(raw_node, source_bytes)
        if name:
            fields["name"] = name
    if node_type in {"use_declaration", "import_declaration", "preproc_include", "use_statement"}:
        module = _import_module(raw_node, source_bytes)
        if module:
            fields["module"] = module
    if node_type == "subroutine_call" and "function" not in fields:
        function = _first_descendant_text(raw_node, source_bytes, {"identifier", "name"})
        if function:
            fields["function"] = function
    if node_type == "type_declaration":
        type_spec = _first_descendant(raw_node, {"type_spec"})
        type_child = type_spec.child_by_field_name("type") if type_spec is not None else None
        if type_child is not None:
            field_types["type"] = str(getattr(type_child, "type", ""))
            field_descendant_types["type"] = tuple(sorted(_node_types(type_child)))


def _derived_name(raw_node: Any, source_bytes: bytes) -> str:
    node_type = str(getattr(raw_node, "type", ""))
    if node_type in {"function_definition", "function_declaration", "field_declaration"}:
        declarator = raw_node.child_by_field_name("declarator")
        return _declarator_name(declarator, source_bytes)
    if node_type == "function_declarator":
        return _declarator_name(raw_node, source_bytes)
    if node_type == "type_declaration":
        type_spec = _first_descendant(raw_node, {"type_spec"})
        if type_spec is not None:
            name = type_spec.child_by_field_name("name")
            if name is not None:
                return _clean_label(_node_text(name, source_bytes))
    if node_type in {"module", "subroutine", "function"}:
        statement_type = {
            "module": "module_statement",
            "subroutine": "subroutine_statement",
            "function": "function_statement",
        }[node_type]
        statement = _first_descendant(raw_node, {statement_type})
        if statement is not None:
            name = statement.child_by_field_name("name") or _first_descendant(statement, {"name"})
            if name is not None:
                return _clean_label(_node_text(name, source_bytes))
    if node_type == "package_clause":
        return _first_descendant_text(raw_node, source_bytes, {"package_identifier", "identifier"})
    return ""


def _declarator_name(raw_node: Any | None, source_bytes: bytes) -> str:
    if raw_node is None:
        return ""
    child_by_field_name = getattr(raw_node, "child_by_field_name", None)
    if child_by_field_name is not None:
        for field_name in ("name", "declarator"):
            child = child_by_field_name(field_name)
            label = _declarator_name(child, source_bytes) if field_name == "declarator" else _node_text(child, source_bytes)
            if label:
                return _clean_label(label)
    if getattr(raw_node, "type", "") in {
        "identifier",
        "field_identifier",
        "type_identifier",
        "qualified_identifier",
        "namespace_identifier",
    }:
        return _clean_label(_node_text(raw_node, source_bytes))
    for child in getattr(raw_node, "named_children", ()) or ():
        label = _declarator_name(child, source_bytes)
        if label:
            return label
    return ""


def _import_module(raw_node: Any, source_bytes: bytes) -> str:
    node_type = str(getattr(raw_node, "type", ""))
    if node_type == "preproc_include":
        path = raw_node.child_by_field_name("path")
        return _strip_import_delimiters(_node_text(path, source_bytes)) if path is not None else ""
    if node_type == "use_declaration":
        for child in getattr(raw_node, "named_children", ()) or ():
            return _clean_label(_node_text(child, source_bytes))
    if node_type == "import_declaration":
        for candidate_type in (
            "interpreted_string_literal_content",
            "raw_string_literal_content",
            "interpreted_string_literal",
            "raw_string_literal",
            "string_literal",
        ):
            label = _first_descendant_text(raw_node, source_bytes, {candidate_type})
            if label:
                return _strip_import_delimiters(label)
    if node_type == "use_statement":
        return _first_descendant_text(raw_node, source_bytes, {"module_name", "name"})
    return ""


def _context_rule_matches(rule: str, node: NormalizedSyntaxNode, ancestors: tuple[str, ...]) -> bool:
    normalized = rule.lower().strip()
    if normalized.startswith("inside "):
        return any(_context_name_matches(ancestor, normalized.removeprefix("inside ")) for ancestor in ancestors)
    if normalized.startswith("type is "):
        return _field_type_matches(node, "type", normalized.removeprefix("type is "))
    if normalized == "qualified declarator":
        return _field_descendant_has(node, "declarator", "qualified_identifier")
    if normalized == "function declarator":
        return _field_type_matches(node, "declarator", "function_declarator") or _field_descendant_has(
            node,
            "declarator",
            "function_declarator",
        )
    return False


def _context_name_matches(node_type: str, expected: str) -> bool:
    aliases = {
        "impl": {"impl_item"},
        "class": {"class_specifier", "struct_specifier"},
    }
    expected_types = aliases.get(expected, {expected})
    return node_type in expected_types


def _field_type_matches(node: NormalizedSyntaxNode, field_name: str, expected_type: str) -> bool:
    field_types = node.fields.get("_field_types", {})
    return isinstance(field_types, dict) and field_types.get(field_name) == expected_type


def _field_descendant_has(node: NormalizedSyntaxNode, field_name: str, expected_type: str) -> bool:
    descendants = node.fields.get("_field_descendant_types", {})
    if not isinstance(descendants, dict):
        return False
    values = descendants.get(field_name, ())
    return expected_type in values


def _node_types(raw_node: Any) -> set[str]:
    types = {str(getattr(raw_node, "type", ""))}
    for child in getattr(raw_node, "named_children", ()) or ():
        types.update(_node_types(child))
    return {item for item in types if item}


def _first_descendant(raw_node: Any, node_types: set[str]) -> Any | None:
    for child in getattr(raw_node, "named_children", ()) or ():
        if getattr(child, "type", "") in node_types:
            return child
        found = _first_descendant(child, node_types)
        if found is not None:
            return found
    return None


def _first_descendant_text(raw_node: Any, source_bytes: bytes, node_types: Iterable[str]) -> str:
    descendant = _first_descendant(raw_node, set(node_types))
    return _clean_label(_node_text(descendant, source_bytes)) if descendant is not None else ""


def _strip_import_delimiters(value: str) -> str:
    return value.strip().strip('"').strip("'").strip("<>").strip()


def _clean_label(value: str) -> str:
    return value.strip().replace("\n", " ")


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
