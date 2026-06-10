from __future__ import annotations

import re
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from codebase_graph.extract import ParseBundle
from .document_parser import MarkdownDocumentParser


class ParserUnavailableError(RuntimeError):
    """Signal parser unavailable error failures."""
    pass


class SourceParser(Protocol):
    """Represent a source parser."""
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
        """Parse file.

        Args:
            path: The path to read or write.
            relative_path: The relative path to read or write.
            source_root: Source root value.
            repository_label: Repository label value.
            content_hash: Content hash value.

        Returns:
            The computed result.
        """
        ...


@dataclass(frozen=True, slots=True)
class ParserRegistration:
    """Store parser registration data."""
    language: str
    suffixes: tuple[str, ...]
    parser_factory: Callable[[], SourceParser]
    parser_version: str


class ParserRegistry:
    """Represent a parser registry."""
    def __init__(self, registrations: Mapping[str, ParserRegistration] | None = None) -> None:
        """Initialize the instance.

        Args:
            registrations: Registrations value.
        """
        self._registrations: dict[str, ParserRegistration] = dict(registrations or {})
        self._suffix_to_language: dict[str, str] = {}
        for registration in self._registrations.values():
            self._register_suffixes(registration)

    @property
    def parser_version(self) -> str:
        """Return parser for version.

        Returns:
            The computed string.
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
        """Register result.

        Args:
            language: Language value.
            suffixes: Suffixes value.
            parser_factory: Parser factory value.
            parser_version: Parser version value.
        """
        registration = ParserRegistration(language, suffixes, parser_factory, parser_version)
        self._registrations[language] = registration
        self._register_suffixes(registration)

    def language_for_path(self, path: Path) -> str | None:
        """Process language for path.

        Args:
            path: The path to read or write.

        Returns:
            The computed result.
        """
        return self._suffix_to_language.get(path.suffix)

    def parser_for_language(self, language: str) -> SourceParser:
        """Return parser for for language.

        Args:
            language: Language value.

        Returns:
            The computed result.
        """
        try:
            registration = self._registrations[language]
        except KeyError as exc:
            raise ValueError(f"Unsupported materializer language: {language}") from exc
        return registration.parser_factory()

    def _register_suffixes(self, registration: ParserRegistration) -> None:
        """Register suffixes.

        Args:
            registration: Registration value.
        """
        for suffix in registration.suffixes:
            self._suffix_to_language[suffix] = registration.language


@dataclass(frozen=True, slots=True)
class TreeSitterPythonParser:
    """Store tree sitter python parser data."""
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
        """Parse file.

        Args:
            path: The path to read or write.
            relative_path: The relative path to read or write.
            source_root: Source root value.
            repository_label: Repository label value.
            content_hash: Content hash value.

        Returns:
            The computed result.
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
        """Parse source.

        Args:
            source_text: Source text value.

        Returns:
            A dictionary containing the computed payload.
        """
        parser = _python_parser()
        source_bytes = source_text.encode("utf-8")
        tree = parser.parse(source_bytes)
        return _convert_node(tree.root_node, source_bytes)


def default_parser_registry() -> ParserRegistry:
    """Create the default parser registry.

    Returns:
        The computed result.
    """
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
    """Return parser for for language.

    Args:
        language: Language value.

    Returns:
        The computed result.
    """
    return default_parser_registry().parser_for_language(language)


def _python_parser() -> Any:
    """Process python parser.

    Returns:
        The computed result.
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
    """Convert a tree-sitter node into the builder's parser-node mapping.

    Args:
        node: Tree-sitter node to convert.
        source_bytes: UTF-8 source bytes used for text extraction.
        decorators: Decorators already collected for a decorated definition.

    Returns:
        A parser-node mapping consumed by `GraphBuilder`.
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
    """Process class fields.

    Args:
        node: Node value.
        source_bytes: Source bytes value.
        decorators: Decorators value.

    Returns:
        A dictionary containing the computed payload.
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
    """Process function fields.

    Args:
        node: Node value.
        source_bytes: Source bytes value.
        decorators: Decorators value.

    Returns:
        A dictionary containing the computed payload.
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
    """Process parameter node.

    Args:
        node: Node value.
        source_bytes: Source bytes value.

    Returns:
        A dictionary containing the computed payload.
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
    if ":" in text:
        annotation = text.split(":", 1)[1].split("=", 1)[0].strip()
        if annotation:
            parameter["annotation"] = {"type": "type", "id": annotation, "text": annotation}
    return parameter


def _import_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    """Process import fields.

    Args:
        node: Node value.
        source_bytes: Source bytes value.

    Returns:
        A dictionary containing the computed payload.
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
    """Process import alias.

    Args:
        raw_name: Raw name value.

    Returns:
        A dictionary containing the computed payload.
    """
    name = raw_name.strip().split(" as ", 1)[0].strip()
    return {"type": "alias", "name": name}


def _call_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    """Call fields.

    Args:
        node: Node value.
        source_bytes: Source bytes value.

    Returns:
        A dictionary containing the computed payload.
    """
    function = node.child_by_field_name("function")
    if function is not None:
        return {"func": _convert_node(function, source_bytes)}
    text = _node_text(node, source_bytes)
    return {"func": {"type": "identifier", "id": text.split("(", 1)[0].strip()}}


def _assignment_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    """Process assignment fields.

    Args:
        node: Node value.
        source_bytes: Source bytes value.

    Returns:
        A dictionary containing the computed payload.
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
    """Process attribute fields.

    Args:
        node: Node value.
        source_bytes: Source bytes value.

    Returns:
        A dictionary containing the computed payload.
    """
    text = _node_text(node, source_bytes)
    if "." not in text:
        return {"id": text}
    base, attr = text.rsplit(".", 1)
    return {"value": {"type": "identifier", "id": base}, "attr": attr}


def _literal_value(node: Any, source_bytes: bytes) -> str:
    """Process literal value.

    Args:
        node: Node value.
        source_bytes: Source bytes value.

    Returns:
        The computed string.
    """
    return _node_text(node, source_bytes).strip("'\"")


def _semantic_children(node: Any) -> tuple[Any, ...]:
    """Return semantic children.

    Args:
        node: Node value.

    Returns:
        A tuple containing the computed values.
    """
    ignored = {"identifier", "type_identifier", "parameters", "decorator", "block"}
    return tuple(child for child in _named_children(node) if child.type not in ignored)


def _named_children(node: Any | None) -> tuple[Any, ...]:
    """Process named children.

    Args:
        node: Node value.

    Returns:
        A tuple containing the computed values.
    """
    if node is None:
        return ()
    return tuple(getattr(node, "named_children", ()) or ())


def _field_text(node: Any, field_name: str, source_bytes: bytes) -> str:
    """Return text field data.

    Args:
        node: Node value.
        field_name: Field name value.
        source_bytes: Source bytes value.

    Returns:
        The computed string.
    """
    child = node.child_by_field_name(field_name)
    return _node_text(child, source_bytes) if child is not None else ""


def _node_text(node: Any, source_bytes: bytes) -> str:
    """Return node text.

    Args:
        node: Node value.
        source_bytes: Source bytes value.

    Returns:
        The computed string.
    """
    return source_bytes[node.start_byte:node.end_byte].decode("utf-8", errors="replace")


def _line_start(node: Any) -> int:
    """Process line start.

    Args:
        node: Node value.

    Returns:
        The computed integer.
    """
    return _point_row(node.start_point) + 1


def _line_end(node: Any) -> int:
    """Process line end.

    Args:
        node: Node value.

    Returns:
        The computed integer.
    """
    return _point_row(node.end_point) + 1


def _point_row(point: Any) -> int:
    """Process point row.

    Args:
        point: Point value.

    Returns:
        The computed integer.
    """
    if hasattr(point, "row"):
        return int(point.row)
    return int(point[0])
