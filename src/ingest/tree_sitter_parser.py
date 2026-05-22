from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from extract import ParseBundle


class ParserUnavailableError(RuntimeError):
    pass


@dataclass(frozen=True, slots=True)
class TreeSitterPythonParser:
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
        parser = _python_parser()
        source_bytes = source_text.encode("utf-8")
        tree = parser.parse(source_bytes)
        return _convert_node(tree.root_node, source_bytes)


def parser_for_language(language: str) -> TreeSitterPythonParser:
    if language == "python":
        return TreeSitterPythonParser()
    raise ValueError(f"Unsupported materializer language: {language}")


def _python_parser() -> Any:
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
    if node.type == "decorated_definition":
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

    converted.setdefault("children", [_convert_node(child, source_bytes) for child in _semantic_children(node)])
    return converted


def _class_fields(
    node: Any,
    source_bytes: bytes,
    decorators: tuple[dict[str, Any], ...],
) -> dict[str, Any]:
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
    name = raw_name.strip().split(" as ", 1)[0].strip()
    return {"type": "alias", "name": name}


def _call_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    function = node.child_by_field_name("function")
    if function is not None:
        return {"func": _convert_node(function, source_bytes)}
    text = _node_text(node, source_bytes)
    return {"func": {"type": "identifier", "id": text.split("(", 1)[0].strip()}}


def _assignment_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    left = node.child_by_field_name("left")
    right = node.child_by_field_name("right")
    fields: dict[str, Any] = {}
    if left is not None:
        fields["target"] = _convert_node(left, source_bytes)
    if right is not None:
        fields["value"] = _convert_node(right, source_bytes)
    return fields


def _attribute_fields(node: Any, source_bytes: bytes) -> dict[str, Any]:
    text = _node_text(node, source_bytes)
    if "." not in text:
        return {"id": text}
    base, attr = text.rsplit(".", 1)
    return {"value": {"type": "identifier", "id": base}, "attr": attr}


def _literal_value(node: Any, source_bytes: bytes) -> str:
    return _node_text(node, source_bytes).strip("'\"")


def _semantic_children(node: Any) -> tuple[Any, ...]:
    ignored = {"identifier", "type_identifier", "parameters", "decorator", "block"}
    return tuple(child for child in _named_children(node) if child.type not in ignored)


def _named_children(node: Any | None) -> tuple[Any, ...]:
    if node is None:
        return ()
    return tuple(getattr(node, "named_children", ()) or ())


def _field_text(node: Any, field_name: str, source_bytes: bytes) -> str:
    child = node.child_by_field_name(field_name)
    return _node_text(child, source_bytes) if child is not None else ""


def _node_text(node: Any, source_bytes: bytes) -> str:
    return source_bytes[node.start_byte:node.end_byte].decode("utf-8", errors="replace")


def _line_start(node: Any) -> int:
    return _point_row(node.start_point) + 1


def _line_end(node: Any) -> int:
    return _point_row(node.end_point) + 1


def _point_row(point: Any) -> int:
    if hasattr(point, "row"):
        return int(point.row)
    return int(point[0])
