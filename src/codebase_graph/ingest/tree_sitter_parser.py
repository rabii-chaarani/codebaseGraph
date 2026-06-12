from __future__ import annotations

import importlib.util
import re
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from codebase_graph.extract import ParseBundle
from .document_parser import MarkdownDocumentParser
from .languages import LanguageProfile, register_language_support


class ParserUnavailableError(RuntimeError):
    """Signal failures raised by the source scanning and graph materialization subsystem."""
    pass


class SourceParser(Protocol):
    """Represent source parser data used by source scanning and graph materialization."""
    language: str
    parser_version: str

    def parse_file(
        self,
        path: Path,
        *,
        relative_path: str,
        source_root: Path,
        repository_label: str,
        content_hash: str,
    ) -> ParseBundle:
        """Parse file for source scanning and graph materialization.

        Args:
            path: Filesystem path read from or written by this operation.
            relative_path: Repository-relative path stored in graph and manifest metadata.
            source_root: Root directory scanned for source files.
            repository_label: Repository label used by the source scanning and graph
            materialization workflow.
            content_hash: Content hash used by the source scanning and graph
            materialization workflow.

        Returns:
            ParseBundle instance populated with data from the source scanning and graph
            materialization workflow.
        """
        ...


@dataclass(frozen=True, slots=True)
class ParserRegistration:
    """Represent parser registration data used by source scanning and graph materialization."""
    language: str
    suffixes: tuple[str, ...]
    parser_factory: Callable[[], SourceParser]
    parser_version: str


class ParserRegistry:
    """Represent parser registry data used by source scanning and graph materialization.

    The class belongs to Tree-sitter Python parser adapter that normalizes syntax into
    GraphBuilder input.
    """
    def __init__(self, registrations: Mapping[str, ParserRegistration] | None = None) -> None:
        """Initialize parser registry with the collaborators and state it owns.

        Args:
            registrations: Registrations used by the source scanning and graph
            materialization workflow.
        """
        self._registrations: dict[str, ParserRegistration] = dict(registrations or {})
        self._suffix_to_language: dict[str, str] = {}
        for registration in self._registrations.values():
            self._register_suffixes(registration)

    @property
    def parser_version(self) -> str:
        """Return version for source scanning and graph materialization.

        Returns:
            Formatted text returned to the caller.
        """
        return "+".join(
            registration.parser_version
            for registration in self._registrations.values()
        )

    def register(
        self,
        language: str,
        *,
        suffixes: tuple[str, ...],
        parser_factory: Callable[[], SourceParser],
        parser_version: str,
    ) -> None:
        """Register source scanning and graph materialization for source scanning and graph materialization.

        Args:
            language: Language used by the source scanning and graph materialization
            workflow.
            suffixes: Suffixes used by the source scanning and graph materialization
            workflow.
            parser_factory: Parser factory used by the source scanning and graph
            materialization workflow.
            parser_version: Parser version used by the source scanning and graph
            materialization workflow.
        """
        registration = ParserRegistration(language, suffixes, parser_factory, parser_version)
        self._registrations[language] = registration
        self._register_suffixes(registration)

    def register_language_profile(self, profile: LanguageProfile) -> None:
        """Register a profiled tree-sitter parser for a language profile."""
        from .tree_sitter_adapter import TreeSitterProfiledParser

        self.register(
            profile.language,
            suffixes=profile.suffixes,
            parser_factory=lambda profile=profile: TreeSitterProfiledParser(profile),
            parser_version=profile.parser_version,
        )

    def language_for_path(self, path: Path) -> str | None:
        """Manage for path within source scanning and graph materialization.

        Args:
            path: Filesystem path read from or written by this operation.

        Returns:
            str | None instance populated with data from the source scanning and graph
            materialization workflow.
        """
        return self._suffix_to_language.get(path.suffix)

    def parser_for_language(self, language: str) -> SourceParser:
        """Return the parser registered for a language.

        Args:
            language: Language used by the source scanning and graph materialization
            workflow.

        Returns:
            SourceParser instance populated with data from the source scanning and graph
            materialization workflow.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
        try:
            registration = self._registrations[language]
        except KeyError as exc:
            raise ValueError(f"Unsupported materializer language: {language}") from exc
        return registration.parser_factory()

    def _register_suffixes(self, registration: ParserRegistration) -> None:
        """Register suffixes for source scanning and graph materialization.

        Args:
            registration: Registration used by the source scanning and graph
            materialization workflow.
        """
        for suffix in registration.suffixes:
            self._suffix_to_language[suffix] = registration.language


@dataclass(frozen=True, slots=True)
class TreeSitterPythonParser:
    """Represent tree sitter python parser data used by source scanning and graph materialization.
    """
    language: str = "python"
    parser_version: str = "tree-sitter-python-v1"

    def parse_file(
        self,
        path: Path,
        *,
        relative_path: str,
        source_root: Path,
        repository_label: str,
        content_hash: str,
    ) -> ParseBundle:
        """Parse file for source scanning and graph materialization.

        Args:
            path: Filesystem path read from or written by this operation.
            relative_path: Repository-relative path stored in graph and manifest metadata.
            source_root: Root directory scanned for source files.
            repository_label: Repository label used by the source scanning and graph
            materialization workflow.
            content_hash: Content hash used by the source scanning and graph
            materialization workflow.

        Returns:
            ParseBundle instance populated with data from the source scanning and graph
            materialization workflow.
        """
        source_text = path.read_text(encoding="utf-8")
        return ParseBundle(
            language=self.language,
            path=relative_path,
            source_text=source_text,
            tree=self.parse_source(source_text),
            repository_label=repository_label,
            source_root=source_root.as_posix(),
            content_hash=content_hash,
        )

    def parse_source(self, source_text: str) -> dict[str, Any]:
        """Parse source for source scanning and graph materialization.

        Args:
            source_text: Original source text used for labels, summaries, and byte-range
            extraction.

        Returns:
            Structured mapping that follows the source scanning and graph
            materialization response contract.
        """
        parser = _python_parser()
        source_bytes = source_text.encode("utf-8")
        tree = parser.parse(source_bytes)
        return _convert_node(tree.root_node, source_bytes)


def default_parser_registry() -> ParserRegistry:
    """Create the default parser registry for source scanning and graph materialization.

    Returns:
        ParserRegistry instance populated with data from the source scanning and graph
        materialization workflow.
    """
    return assemble_profiled_parser_registry()


def assemble_profiled_parser_registry(
    source_root: str | Path | None = None,
    *,
    include_unavailable: bool = False,
) -> ParserRegistry:
    """Assemble parser registry with built-in parsers and available profiled languages."""
    registry = _base_parser_registry()
    for profile in register_language_support(source_root):
        if include_unavailable or importlib.util.find_spec(profile.grammar_package) is not None:
            registry.register_language_profile(profile)
    return registry


def _base_parser_registry() -> ParserRegistry:
    """Create the built-in parser registry before optional profiled language registration."""
    registry = ParserRegistry()
    registry.register(
        "python",
        suffixes=(".py",),
        parser_factory=TreeSitterPythonParser,
        parser_version=TreeSitterPythonParser().parser_version,
    )
    registry.register(
        "markdown",
        suffixes=(".md", ".mdx"),
        parser_factory=MarkdownDocumentParser,
        parser_version=MarkdownDocumentParser().parser_version,
    )
    return registry


def parser_for_language(language: str) -> SourceParser:
    """Return the parser registered for a language.

    Args:
        language: Language used by the source scanning and graph materialization
        workflow.

    Returns:
        SourceParser instance populated with data from the source scanning and graph
        materialization workflow.
    """
    return default_parser_registry().parser_for_language(language)


def _python_parser() -> Any:
    """Manage parser within source scanning and graph materialization.

    Returns:
        Any instance populated with data from the source scanning and graph materialization
        workflow.

    Raises:
        ParserUnavailableError: Raised when validation or runtime preconditions fail.
    """
    try:
        from tree_sitter import Language, Parser
        import tree_sitter_python
    except ImportError as exc:
        raise ParserUnavailableError(
            "Tree-sitter Python parsing requires `tree-sitter` and `tree-sitter-python`."
        ) from exc

    raw_language = tree_sitter_python.language()
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


def _convert_node(node: Any, source_bytes: bytes, decorators: tuple[dict[str, Any], ...] = ()) -> dict[str, Any]:
    """Normalize a tree-sitter Python node into the mapping shape consumed by GraphBuilder.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.
        decorators: Decorators used by the source scanning and graph materialization
        workflow.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    if node.type == "decorated_definition":
        # Tree-sitter wraps decorators around the real definition; flatten them
        # onto the class/function node so the graph builder sees one declaration.
        converted_decorators = tuple(
            _convert_node(child, source_bytes)
            for child in _named_children(node)
            if child.type == "decorator"
        )
        for child in _named_children(node):
            if child.type in {"class_definition", "function_definition"}:
                return _convert_node(child, source_bytes, converted_decorators)

    converted: dict[str, Any] = {
        "type": node.type,
        "text": _node_text(node, source_bytes),
        "line_start": _line_start(node),
        "line_end": _line_end(node),
        "byte_start": node.start_byte,
        "byte_end": node.end_byte,
    }

    if node.type == "module":
        converted["children"] = [_convert_node(child, source_bytes) for child in _named_children(node)]
    elif node.type == "class_definition":
        converted.update(_class_fields(node, source_bytes, decorators))
    elif node.type == "function_definition":
        converted.update(_function_fields(node, source_bytes, decorators))
    elif node.type in {"import_statement", "import_from_statement"}:
        converted.update(_import_fields(node, source_bytes))
    elif node.type == "call":
        converted.update(_call_fields(node, source_bytes))
    elif node.type == "assignment":
        converted.update(_assignment_fields(node, source_bytes))
    elif node.type in {"identifier", "type_identifier"}:
        converted["id"] = _node_text(node, source_bytes)
    elif node.type == "attribute":
        converted.update(_attribute_fields(node, source_bytes))
    elif node.type in {"string", "integer", "float", "true", "false", "none"}:
        converted["value"] = _literal_value(node, source_bytes)

    # Unknown syntax is still traversed through semantic children so new Python
    # grammar nodes do not hide nested definitions or calls from the graph.
    converted.setdefault("children", [_convert_node(child, source_bytes) for child in _semantic_children(node)])
    return converted


def _class_fields(
    node: Any,
    source_bytes: bytes,
    decorators: tuple[dict[str, Any], ...],
) -> dict[str, Any]:
    """Manage fields within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.
        decorators: Decorators used by the source scanning and graph materialization
        workflow.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    fields: dict[str, Any] = {"name": _field_text(node, "name", source_bytes)}
    if decorators:
        fields["decorator_list"] = list(decorators)
    body = node.child_by_field_name("body")
    fields["children"] = [_convert_node(child, source_bytes) for child in _named_children(body)]
    return fields


def _function_fields(
    node: Any,
    source_bytes: bytes,
    decorators: tuple[dict[str, Any], ...],
) -> dict[str, Any]:
    """Manage fields within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.
        decorators: Decorators used by the source scanning and graph materialization
        workflow.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    fields: dict[str, Any] = {"name": _field_text(node, "name", source_bytes)}
    parameters = node.child_by_field_name("parameters")
    if parameters is not None:
        fields["args"] = {"type": "arguments", "args": [_parameter_node(child, source_bytes) for child in _named_children(parameters)]}
    return_type = node.child_by_field_name("return_type")
    if return_type is not None:
        fields["returns"] = _convert_node(return_type, source_bytes)
    if decorators:
        fields["decorator_list"] = list(decorators)
    body = node.child_by_field_name("body")
    fields["children"] = [_convert_node(child, source_bytes) for child in _named_children(body)]
    return fields


def _parameter_node(node: Any, source_bytes: bytes) -> dict[str, Any]:
    """Manage node within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    text = _node_text(node, source_bytes)
    name = text.split(":", 1)[0].split("=", 1)[0].strip().lstrip("*")
    parameter: dict[str, Any] = {
        "type": "arg",
        "arg": name,
        "text": text,
        "line_start": _line_start(node),
        "line_end": _line_end(node),
        "byte_start": node.start_byte,
        "byte_end": node.end_byte,
    }
    annotation_node = node.child_by_field_name("type")
    if annotation_node is not None:
        parameter["annotation"] = _convert_node(annotation_node, source_bytes)
    elif ":" in text:
        annotation = text.split(":", 1)[1].split("=", 1)[0].strip()
        if annotation:
            parameter["annotation"] = {
                "type": "type",
                "id": annotation,
                "text": annotation,
                "line_start": _line_start(node),
                "line_end": _line_end(node),
                "byte_start": node.start_byte,
                "byte_end": node.end_byte,
            }
    return parameter


def _import_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    """Manage fields within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    text = _node_text(node, source_bytes).strip()
    if node.type == "import_from_statement":
        match = re.match(r"from\s+([.\w]+)\s+import\s+(.+)", text)
        if match:
            names = [_import_alias(name) for name in match.group(2).split(",")]
            return {"module": match.group(1), "names": names}
    if node.type == "import_statement":
        imported = text.removeprefix("import").strip()
        return {"names": [_import_alias(name) for name in imported.split(",")]}
    return {}


def _import_alias(raw_name: str) -> dict[str, str]:
    """Manage alias within source scanning and graph materialization.

    Args:
        raw_name: Name used to select or label raw data.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    name = raw_name.strip().split(" as ", 1)[0].strip()
    return {"type": "alias", "name": name}


def _call_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    """Dispatch fields for source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    function = node.child_by_field_name("function")
    if function is not None:
        return {"func": _convert_node(function, source_bytes)}
    text = _node_text(node, source_bytes)
    return {"func": {"type": "identifier", "id": text.split("(", 1)[0].strip()}}


def _assignment_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    """Manage fields within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    left = node.child_by_field_name("left")
    right = node.child_by_field_name("right")
    fields: dict[str, Any] = {}
    if left is not None:
        fields["target"] = _convert_node(left, source_bytes)
    if right is not None:
        fields["value"] = _convert_node(right, source_bytes)
    return fields


def _attribute_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    """Manage fields within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.

    Returns:
        Structured mapping that follows the source scanning and graph materialization
        response contract.
    """
    object_node = node.child_by_field_name("object")
    attribute_node = node.child_by_field_name("attribute")
    if object_node is not None and attribute_node is not None:
        return {
            "value": _convert_node(object_node, source_bytes),
            "attr": _node_text(attribute_node, source_bytes),
        }

    text = _node_text(node, source_bytes)
    if "." not in text:
        return {"id": text}
    base, attr = text.rsplit(".", 1)
    return {
        "value": {
            "type": "identifier",
            "id": base,
            "text": base,
            "line_start": _line_start(node),
            "line_end": _line_end(node),
            "byte_start": node.start_byte,
            "byte_end": node.end_byte,
        },
        "attr": attr,
    }


def _literal_value(node: Any, source_bytes: bytes) -> str:
    """Manage value within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.

    Returns:
        Formatted text returned to the caller.
    """
    return _node_text(node, source_bytes).strip("'\"")


def _semantic_children(node: Any) -> tuple[Any, ...]:
    """Return children for source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.

    Returns:
        Tuple of stable results returned to the source scanning and graph materialization
        caller.
    """
    ignored = {"identifier", "type_identifier", "parameters", "decorator", "block"}
    return tuple(child for child in _named_children(node) if child.type not in ignored)


def _named_children(node: Any | None) -> tuple[Any, ...]:
    """Manage children within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.

    Returns:
        Tuple of stable results returned to the source scanning and graph materialization
        caller.
    """
    if node is None:
        return ()
    return tuple(getattr(node, "named_children", ()) or ())


def _field_text(node: Any, field_name: str, source_bytes: bytes) -> str:
    """Read text for source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        field_name: Field name being extracted from a node or edge entry.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.

    Returns:
        Formatted text returned to the caller.
    """
    child = node.child_by_field_name(field_name)
    return _node_text(child, source_bytes) if child is not None else ""


def _node_text(node: Any, source_bytes: bytes) -> str:
    """Manage text within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.
        source_bytes: UTF-8 encoded source used to slice tree-sitter node text.

    Returns:
        Formatted text returned to the caller.
    """
    return source_bytes[node.start_byte:node.end_byte].decode("utf-8", errors="replace")


def _line_start(node: Any) -> int:
    """Manage start within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.

    Returns:
        Integer count, status code, or index used by the caller.
    """
    return _point_row(node.start_point) + 1


def _line_end(node: Any) -> int:
    """Manage end within source scanning and graph materialization.

    Args:
        node: Parser or graph node being inspected.

    Returns:
        Integer count, status code, or index used by the caller.
    """
    return _point_row(node.end_point) + 1


def _point_row(point: Any) -> int:
    """Manage row within source scanning and graph materialization.

    Args:
        point: Point used by the source scanning and graph materialization workflow.

    Returns:
        Integer count, status code, or index used by the caller.
    """
    if hasattr(point, "row"):
        return int(point.row)
    return int(point[0])
