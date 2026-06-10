from __future__ import annotations

import hashlib
from collections.abc import Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode
from codebase_graph.ontology import ONTOLOGY_NAME, get_relation_type, node_type_names, relation_type_names


@dataclass(frozen=True, slots=True)
class CaptureRecord:
    """Store capture record data."""
    capture: str
    node: Any


@dataclass(frozen=True, slots=True)
class ParseBundle:
    """Store parse bundle data."""
    language: str
    path: str
    source_text: str = ""
    tree: Any | None = None
    captures: Sequence[CaptureRecord | Mapping[str, Any] | tuple[Any, str]] = ()
    repository_label: str = "repository"
    source_root: str = "."
    content_hash: str = ""


@dataclass(frozen=True, slots=True)
class GraphBuildResult:
    """Store the result of graph build operations."""
    nodes: list[dict[str, Any]]
    edges: list[dict[str, Any]]
    diagnostics: list[str]
    unresolved: list[str]
    graph: CodeGraph

    def as_dict(self) -> dict[str, Any]:
        """Return a JSON-serializable dictionary representation.

        Returns:
            A dictionary containing the computed payload.
        """
        return {
            "nodes": self.nodes,
            "edges": self.edges,
            "diagnostics": self.diagnostics,
            "unresolved": self.unresolved,
            "summary": self.graph.summary(),
        }


@dataclass(frozen=True, slots=True)
class ParserNode:
    """Store parser node data."""
    node_type: str
    fields: Mapping[str, Any]
    children: tuple[Any, ...]
    line_start: int | None = None
    line_end: int | None = None
    byte_start: int | None = None
    byte_end: int | None = None
    capture_name: str = ""
    text: str = ""


@dataclass(frozen=True, slots=True)
class BuildContext:
    """Store build context data."""
    path: str
    language: str
    source_text: str
    repository_label: str
    source_root: str


@dataclass(frozen=True, slots=True)
class ScopeFrame:
    """Store scope frame data."""
    node_id: str
    table: str
    label: str
    scope_id: str
    qualified_name: str


CaptureTableResolver = Callable[[str, ScopeFrame], str | None]


class CaptureTableRegistry:
    """Represent a capture table registry."""
    def __init__(self) -> None:
        """Initialize the instance."""
        self._exact: dict[str, str | CaptureTableResolver] = {}
        self._prefix: list[tuple[str, str | CaptureTableResolver]] = []

    def register_exact(self, capture_name: str, table: str | CaptureTableResolver) -> None:
        """Register exact.

        Args:
            capture_name: Capture name value.
            table: Table value.
        """
        self._exact[_normalize_capture_name(capture_name)] = table

    def register_prefix(self, prefix: str, table: str | CaptureTableResolver) -> None:
        """Register prefix.

        Args:
            prefix: Prefix value.
            table: Table value.
        """
        self._prefix.append((_normalize_capture_name(prefix), table))

    def table_for(self, capture_name: str, owner: ScopeFrame) -> str | None:
        """Return table for for.

        Args:
            capture_name: Capture name value.
            owner: Owner value.

        Returns:
            The computed result.
        """
        capture = _normalize_capture_name(capture_name)
        if not capture:
            return None
        if capture in self._exact:
            return _resolve_capture_table(self._exact[capture], capture, owner)
        for prefix, table in self._prefix:
            if capture.startswith(prefix):
                return _resolve_capture_table(table, capture, owner)
        return None


def default_capture_table_registry() -> CaptureTableRegistry:
    """Create the default capture table registry.

    Returns:
        The computed result.
    """
    registry = CaptureTableRegistry()
    for capture in ("definition.class", "definition.struct", "definition.interface"):
        registry.register_exact(capture, "Class")
    registry.register_exact("definition.component", "Component")
    registry.register_exact("component", "Component")
    registry.register_exact("definition.method", "Method")
    registry.register_exact("definition.function", _function_capture_table)
    registry.register_exact("definition.parameter", "Parameter")
    registry.register_exact("parameter", "Parameter")
    registry.register_exact("type.return", "ReturnType")
    registry.register_exact("return_type", "ReturnType")
    for capture in ("type", "type.annotation", "reference.type"):
        registry.register_exact(capture, "TypeAnnotation")
    registry.register_exact("definition.type_alias", "TypeAlias")
    registry.register_exact("definition.constant", "Constant")
    registry.register_exact("definition.variable", "Variable")
    registry.register_exact("decorator", "Decorator")
    registry.register_exact("definition.decorator", "Decorator")
    for capture in ("reference.import", "reference.include", "reference.require", "reference.use", "import"):
        registry.register_exact(capture, "ImportDeclaration")
    registry.register_exact("export", "ExportDeclaration")
    registry.register_exact("definition.export", "ExportDeclaration")
    registry.register_exact("reference.call", "CallExpression")
    registry.register_exact("call", "CallExpression")
    registry.register_prefix("query.", "Query")
    registry.register_prefix("secret.", "SecretRef")
    registry.register_exact("entrypoint.api", "APIEndpoint")
    registry.register_exact("endpoint", "APIEndpoint")
    registry.register_exact("route", "Route")
    registry.register_exact("doc.source", "DocumentationSource")
    registry.register_prefix("doc", "DocumentationChunk")
    registry.register_exact("literal", "Literal")
    registry.register_exact("string", "Literal")
    registry.register_exact("number", "Literal")
    registry.register_exact("control_flow", "ControlFlowBlock")
    registry.register_exact("exception", "ExceptionFlow")
    registry.register_exact("raises", "ExceptionFlow")
    registry.register_exact("handles", "ExceptionFlow")
    registry.register_prefix("reference", "Reference")
    return registry


class GraphBuilder:
    """Build an ontology graph from tree-sitter-shaped parser output.

    The builder deliberately uses duck typing instead of importing tree-sitter.
    It accepts dictionaries, Python AST-like objects, and tree-sitter Node-like
    objects with ``type``, ``children``, ``start_point``, and ``end_point``.
    """

    def __init__(
        self,
        *,
        default_language: str = "",
        repository_label: str = "repository",
        source_root: str | Path = ".",
        include_syntax_captures: bool = True,
        capture_table_registry: CaptureTableRegistry | None = None,
    ) -> None:
        """Initialize the instance.

        Args:
            default_language: Default language value.
            repository_label: Repository label value.
            source_root: Source root value.
            include_syntax_captures: Include syntax captures value.
            capture_table_registry: Capture table registry value.
        """
        self.default_language = default_language
        self.repository_label = repository_label
        self.source_root = Path(source_root).as_posix()
        self.include_syntax_captures = include_syntax_captures
        self.capture_table_registry = capture_table_registry or default_capture_table_registry()
        self._node_types = set(node_type_names())
        self._relation_types = set(relation_type_names())
        self._graph = CodeGraph()
        self._context = BuildContext("", "", "", repository_label, self.source_root)
        self._syntax_nodes: dict[int, str] = {}
        self._symbols_by_name: dict[str, list[str]] = {}
        self._diagnostics: list[str] = []
        self._unresolved: list[str] = []

    def build_file_graph(self, bundle: ParseBundle) -> GraphBuildResult:
        """Build file graph.

        Args:
            bundle: Bundle value.

        Returns:
            The computed result.
        """
        if bundle.captures:
            graph = self.build_from_captures(
                bundle.captures,
                source_path=bundle.path,
                language=bundle.language,
                source_text=bundle.source_text,
                repository_label=bundle.repository_label,
                source_root=bundle.source_root,
            )
        else:
            tree = bundle.tree or {"type": "Module", "children": []}
            graph = self.build(
                tree,
                source_path=bundle.path,
                language=bundle.language,
                source_text=bundle.source_text,
                repository_label=bundle.repository_label,
                source_root=bundle.source_root,
            )
        if bundle.content_hash:
            for node in graph.nodes_by_type("File"):
                node.metadata["content_hash"] = bundle.content_hash
        return GraphBuildResult(
            nodes=graph.as_dict()["nodes"],
            edges=graph.as_dict()["edges"],
            diagnostics=list(self._diagnostics),
            unresolved=list(self._unresolved),
            graph=graph,
        )

    def build(
        self,
        parse_tree: Any,
        *,
        source_path: str | Path,
        language: str | None = None,
        source_text: str = "",
        repository_label: str | None = None,
        source_root: str | Path | None = None,
    ) -> CodeGraph:
        """Build a graph from a parser tree for one source file.

        Args:
            parse_tree: Parser output shaped as mappings, AST-like objects, or tree-sitter nodes.
            source_path: Source file path represented by the graph.
            language: Language identifier for parser-specific node handling.
            source_text: Source text used to recover labels and snippets from parser byte ranges.
            repository_label: Repository node label to use for this build.
            source_root: Source root represented by the graph.

        Returns:
            A validated ontology graph for the source file.
        """
        path = Path(source_path).as_posix()
        root = Path(source_root).as_posix() if source_root is not None else self.source_root
        repo_label = repository_label or self.repository_label
        self._graph = CodeGraph(
            ontology=ONTOLOGY_NAME,
            metadata={"source_path": path, "language": language or self.default_language, "source_root": root},
        )
        self._context = BuildContext(
            path=path,
            language=language or self.default_language,
            source_text=source_text,
            repository_label=repo_label,
            source_root=root,
        )
        self._syntax_nodes = {}
        self._symbols_by_name = {}
        self._diagnostics = []
        self._unresolved = []

        # Every file graph starts with the same ownership spine so later semantic
        # nodes can attach to a stable Repository -> SourceRoot -> File hierarchy.
        repository = self._support_node("Repository", repo_label, repo_label, path="")
        source = self._support_node("SourceRoot", root, root, path=root)
        file = self._support_node("File", path, Path(path).name, path=path)
        self._edge("Contains", repository.id, source.id, "repository_source_root")
        self._edge("Contains", source.id, file.id, "source_root_file")

        root_node = self._normalize(parse_tree)
        if root_node.node_type in {"Module", "module", "program", "source_file"}:
            module = self._semantic_node("Module", root_node, label=_module_label(path), owner=file)
            module_scope = self._scope_for(module)
            self._edge("Contains", file.id, module.id, "file_module")
            self._edge("Contains", module.id, module_scope.id, "module_contains_scope")
            self._edge("HasScope", module.id, module_scope.id, "module_scope")
            self._traverse(root_node, ScopeFrame(module.id, "Module", module.label, module_scope.id, module.label))
        else:
            file_scope = self._scope_for(file)
            self._edge("HasScope", file.id, file_scope.id, "file_scope")
            self._traverse(root_node, ScopeFrame(file.id, "File", file.label, file_scope.id, file.label))

        self._graph.validate_schema()
        return self._graph

    def build_from_captures(
        self,
        captures: Iterable[CaptureRecord | Mapping[str, Any] | tuple[Any, str]],
        *,
        source_path: str | Path,
        language: str | None = None,
        source_text: str = "",
        repository_label: str | None = None,
        source_root: str | Path | None = None,
    ) -> CodeGraph:
        """Build a graph from explicit capture records.

        Args:
            captures: Capture records emitted by a parser query.
            source_path: Source file path represented by the graph.
            language: Language identifier for parser-specific node handling.
            source_text: Source text used to recover labels and snippets from parser byte ranges.
            repository_label: Repository node label to use for this build.
            source_root: Source root represented by the graph.

        Returns:
            A validated ontology graph for the captured file.
        """
        root = {
            "type": "Module",
            "children": [
                {"type": _capture_node_type(capture), "capture_name": _capture_name(capture), "node": _capture_node(capture)}
                for capture in captures
            ],
        }
        return self.build(
            root,
            source_path=source_path,
            language=language,
            source_text=source_text,
            repository_label=repository_label,
            source_root=source_root,
        )

    def _traverse(self, raw_node: Any, owner: ScopeFrame) -> None:
        """Walk parser nodes and emit semantic graph nodes.

        Args:
            raw_node: Raw parser node to normalize and inspect.
            owner: Current lexical ownership frame for emitted semantic nodes.
        """
        node = self._normalize(raw_node)
        syntax_id = self._syntax_capture(node)
        next_owner = owner
        capture_table = self.capture_table_registry.table_for(node.capture_name, owner)

        if capture_table is not None:
            semantic = self._emit_captured_semantic(capture_table, node, owner, syntax_id)
            if capture_table in {"Class", "Function", "Method", "Component"}:
                scope = self._scope_for(semantic)
                self._edge("Contains", semantic.id, scope.id, f"{capture_table.lower()}_contains_scope")
                self._edge("HasScope", semantic.id, scope.id, f"{capture_table.lower()}_scope")
                next_owner = ScopeFrame(semantic.id, capture_table, semantic.label, scope.id, semantic.qualified_name)
        elif node.node_type in {"Module", "module", "program", "source_file"} and owner.table != "Module":
            semantic = self._semantic_node("Module", node, label=_module_label(self._context.path), owner_id=owner.node_id)
            scope = self._scope_for(semantic)
            self._edge("Contains", owner.node_id, semantic.id, "contains_module")
            self._edge("Contains", semantic.id, scope.id, "module_contains_scope")
            self._edge("HasScope", semantic.id, scope.id, "module_scope")
            self._derived_from(semantic.id, syntax_id)
            next_owner = ScopeFrame(semantic.id, "Module", semantic.label, scope.id, semantic.qualified_name)
        elif node.node_type in IMPORT_NODE_TYPES:
            self._emit_import(node, owner, syntax_id)
        elif node.node_type in EXPORT_NODE_TYPES:
            self._emit_simple_semantic("ExportDeclaration", node, owner, syntax_id)
        elif node.node_type in CLASS_NODE_TYPES:
            semantic = self._emit_declaration("Class", node, owner, syntax_id)
            scope = self._scope_for(semantic)
            self._edge("Contains", semantic.id, scope.id, "class_contains_scope")
            self._edge("HasScope", semantic.id, scope.id, "class_scope")
            next_owner = ScopeFrame(semantic.id, "Class", semantic.label, scope.id, semantic.qualified_name)
            self._emit_decorators(node, semantic)
        elif node.node_type in FUNCTION_NODE_TYPES:
            table = "Method" if owner.table in {"Class", "Component"} else "Function"
            semantic = self._emit_declaration(table, node, owner, syntax_id)
            scope = self._scope_for(semantic)
            self._edge("Contains", semantic.id, scope.id, f"{table.lower()}_contains_scope")
            self._edge("HasScope", semantic.id, scope.id, f"{table.lower()}_scope")
            next_owner = ScopeFrame(semantic.id, table, semantic.label, scope.id, semantic.qualified_name)
            self._emit_parameters(node, semantic)
            self._emit_return_type(node, semantic)
            self._emit_decorators(node, semantic)
        elif node.node_type in ASSIGNMENT_NODE_TYPES:
            self._emit_assignment(node, owner, syntax_id)
        elif node.node_type in CALL_NODE_TYPES:
            self._emit_call(node, owner, syntax_id)
        elif node.node_type in REFERENCE_NODE_TYPES:
            self._emit_reference(node, owner, syntax_id)
        elif node.node_type in LITERAL_NODE_TYPES:
            self._emit_simple_semantic("Literal", node, owner, syntax_id)
        elif node.node_type in PARAMETER_NODE_TYPES:
            self._emit_simple_semantic("Parameter", node, owner, syntax_id)
        elif node.node_type in RETURN_TYPE_NODE_TYPES:
            self._emit_simple_semantic("ReturnType", node, owner, syntax_id)
        elif node.node_type in TYPE_NODE_TYPES:
            self._emit_simple_semantic("TypeAnnotation", node, owner, syntax_id)
        elif node.node_type in CONTROL_FLOW_NODE_TYPES:
            self._emit_simple_semantic("ControlFlowBlock", node, owner, syntax_id)
        elif node.node_type in EXCEPTION_FLOW_NODE_TYPES:
            self._emit_simple_semantic("ExceptionFlow", node, owner, syntax_id)

        # Children inherit the nearest semantic declaration scope, not simply the
        # syntactic parent, so nested functions/classes resolve names correctly.
        for child in self._semantic_children(node):
            self._traverse(child, next_owner)

    def _emit_captured_semantic(
        self,
        table: str,
        node: ParserNode,
        owner: ScopeFrame,
        syntax_id: str,
    ) -> GraphNode:
        """Emit captured semantic.

        Args:
            table: Table value.
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.

        Returns:
            The computed result.
        """
        if table == "ImportDeclaration":
            return self._emit_import(node, owner, syntax_id)
        if table == "ExportDeclaration":
            return self._emit_simple_semantic("ExportDeclaration", node, owner, syntax_id)
        if table in {"Class", "Function", "Method"}:
            return self._emit_declaration(table, node, owner, syntax_id)
        if table == "CallExpression":
            return self._emit_call(node, owner, syntax_id)
        if table == "Reference":
            return self._emit_reference(node, owner, syntax_id)
        return self._emit_simple_semantic(table, node, owner, syntax_id)

    def _emit_import(self, node: ParserNode, owner: ScopeFrame, syntax_id: str) -> GraphNode:
        """Emit import.

        Args:
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.

        Returns:
            The computed result.
        """
        imported = _import_label(node) or _label_for(node)
        semantic = self._semantic_node(
            "ImportDeclaration",
            node,
            label=imported or node.node_type,
            owner_id=owner.node_id,
            metadata={"imported_name": imported},
        )
        self._connect_owner(owner, semantic)
        self._edge_if_allowed("Imports", _import_source_id(owner), semantic.id, "declares_import")
        self._derived_from(semantic.id, syntax_id)
        if imported:
            dependency = self._support_node("Dependency", imported, imported, path=self._context.path)
            self._edge("DependsOn", semantic.id, dependency.id, "import_dependency")
            self._edge("EvidencedBy", dependency.id, syntax_id, "parser_evidence")
        return semantic

    def _emit_declaration(self, table: str, node: ParserNode, owner: ScopeFrame, syntax_id: str) -> GraphNode:
        """Emit declaration.

        Args:
            table: Table value.
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.

        Returns:
            The computed result.
        """
        semantic = self._semantic_node(table, node, owner_id=owner.node_id, owner_qualified_name=owner.qualified_name)
        self._connect_owner(owner, semantic)
        self._edge("Defines", owner.node_id, semantic.id, f"defines_{table.lower()}")
        if owner.table in {"Module", "Scope", "Class", "Function", "Method"}:
            self._edge("Declares", owner.node_id, semantic.id, f"declares_{table.lower()}")
        self._derived_from(semantic.id, syntax_id)
        return semantic

    def _emit_assignment(self, node: ParserNode, owner: ScopeFrame, syntax_id: str) -> GraphNode:
        """Emit assignment.

        Args:
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.

        Returns:
            The computed result.
        """
        assignment = self._semantic_node("Assignment", node, owner_id=owner.node_id, owner_qualified_name=owner.qualified_name)
        self._connect_owner(owner, assignment)
        self._derived_from(assignment.id, syntax_id)

        target_label = _assignment_target_label(node)
        if target_label:
            target_table = _assignment_target_table(target_label, owner, node)
            target = self._semantic_node(
                target_table,
                node,
                label=target_label,
                owner_id=owner.node_id,
                owner_qualified_name=owner.qualified_name,
            )
            self._connect_owner(owner, target)
            self._edge("Defines", owner.node_id, target.id, f"defines_{target_table.lower()}")
            self._edge("Assigns", assignment.id, target.id, "assignment_target")
            self._derived_from(target.id, syntax_id)
            annotation = _field(node, "annotation")
            if annotation is not None:
                type_node = self._emit_type_annotation(annotation, target)
                self._edge("HasTypeAnnotation", target.id, type_node.id, "assignment_annotation")

        value = _field(node, "value")
        if value is not None and _normalized_type(value) in CALL_NODE_TYPES:
            call = self._emit_call(self._normalize(value), owner, self._syntax_capture(self._normalize(value)))
            self._edge("Assigns", assignment.id, call.id, "assignment_value")

        return assignment

    def _emit_call(self, node: ParserNode, owner: ScopeFrame, syntax_id: str) -> GraphNode:
        """Emit call.

        Args:
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.

        Returns:
            The computed result.
        """
        call = self._semantic_node(
            "CallExpression",
            node,
            label=_call_label(node) or _label_for(node),
            owner_id=owner.node_id,
            owner_qualified_name=owner.qualified_name,
        )
        self._connect_owner(owner, call)
        if owner.table in {"Function", "Method", "APIEndpoint", "Route", "Component"}:
            self._edge("Calls", owner.node_id, call.id, "body_call")
        target = self._emit_reference_edges(call, call.label, kind_prefix="call")
        if target is not None:
            self._edge_if_allowed("Calls", call.id, target.id, "call_target")
        self._derived_from(call.id, syntax_id)
        return call

    def _emit_reference(self, node: ParserNode, owner: ScopeFrame, syntax_id: str) -> GraphNode:
        """Emit reference.

        Args:
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.

        Returns:
            The computed result.
        """
        reference = self._semantic_node(
            "Reference",
            node,
            label=_label_for(node),
            owner_id=owner.node_id,
            owner_qualified_name=owner.qualified_name,
        )
        self._connect_owner(owner, reference)
        self._emit_reference_edges(reference, reference.label, kind_prefix="reference")
        self._derived_from(reference.id, syntax_id)
        return reference

    def _emit_simple_semantic(self, table: str, node: ParserNode, owner: ScopeFrame, syntax_id: str) -> GraphNode:
        """Emit simple semantic.

        Args:
            table: Table value.
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.

        Returns:
            The computed result.
        """
        semantic = self._semantic_node(
            table,
            node,
            label=_label_for(node),
            owner_id=owner.node_id,
            owner_qualified_name=owner.qualified_name,
        )
        self._connect_owner(owner, semantic)
        self._emit_contextual_relations(semantic, node, owner, syntax_id)
        self._derived_from(semantic.id, syntax_id)
        return semantic

    def _emit_parameters(self, node: ParserNode, callable_node: GraphNode) -> None:
        """Emit parameters.

        Args:
            node: Node value.
            callable_node: Callable node value.
        """
        for index, parameter in enumerate(_parameters(node)):
            parser_node = self._normalize(parameter)
            syntax_id = self._syntax_capture(parser_node)
            param_node = self._semantic_node(
                "Parameter",
                parser_node,
                label=_label_for(parser_node) or f"param_{index}",
                owner_id=callable_node.id,
                owner_qualified_name=callable_node.qualified_name,
            )
            self._edge("HasParameter", callable_node.id, param_node.id, "callable_parameter", metadata={"ordinal": index})
            self._derived_from(param_node.id, syntax_id)
            annotation = _field(parser_node, "annotation")
            if annotation is not None:
                type_node = self._emit_type_annotation(annotation, param_node)
                self._edge("HasTypeAnnotation", param_node.id, type_node.id, "parameter_annotation")

    def _emit_return_type(self, node: ParserNode, callable_node: GraphNode) -> None:
        """Emit return type.

        Args:
            node: Node value.
            callable_node: Callable node value.
        """
        raw_return = _field(node, "returns") or _field(node, "return_type")
        if raw_return is None:
            return
        return_parser = self._normalize(raw_return)
        syntax_id = self._syntax_capture(return_parser)
        return_node = self._semantic_node(
            "ReturnType",
            return_parser,
            label=_label_for(return_parser),
            owner_id=callable_node.id,
            owner_qualified_name=callable_node.qualified_name,
        )
        self._edge("HasReturnType", callable_node.id, return_node.id, "callable_return_type")
        type_node = self._emit_type_annotation(return_parser, return_node)
        self._edge("HasTypeAnnotation", return_node.id, type_node.id, "return_type_annotation")
        self._derived_from(return_node.id, syntax_id)

    def _emit_type_annotation(self, raw_node: Any, owner: GraphNode) -> GraphNode:
        """Emit type annotation.

        Args:
            raw_node: Raw node value.
            owner: Owner value.

        Returns:
            The computed result.
        """
        parser_node = self._normalize(raw_node)
        syntax_id = self._syntax_capture(parser_node)
        type_node = self._semantic_node(
            "TypeAnnotation",
            parser_node,
            label=_label_for(parser_node),
            owner_id=owner.id,
            owner_qualified_name=owner.qualified_name,
        )
        self._emit_reference_edges(type_node, type_node.label, kind_prefix="type_annotation")
        self._derived_from(type_node.id, syntax_id)
        return type_node

    def _emit_decorators(self, node: ParserNode, declaration: GraphNode) -> None:
        """Emit decorators.

        Args:
            node: Node value.
            declaration: Declaration value.
        """
        for raw_decorator in _iter_field_items(node, "decorator_list", "decorators"):
            decorator_node = self._normalize(raw_decorator)
            syntax_id = self._syntax_capture(decorator_node)
            decorator = self._semantic_node(
                "Decorator",
                decorator_node,
                label=_call_label(decorator_node) or _label_for(decorator_node),
                owner_id=declaration.id,
                owner_qualified_name=declaration.qualified_name,
            )
            self._edge("DecoratedBy", declaration.id, decorator.id, "declaration_decorator")
            target = self._emit_reference_edges(decorator, decorator.label, kind_prefix="decorator")
            if target is not None:
                self._edge_if_allowed("Calls", decorator.id, target.id, "decorator_call")
            self._derived_from(decorator.id, syntax_id)

    def _emit_contextual_relations(
        self,
        semantic: GraphNode,
        node: ParserNode,
        owner: ScopeFrame,
        syntax_id: str,
    ) -> None:
        """Emit contextual relations.

        Args:
            semantic: Semantic value.
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.
        """
        table = semantic.table

        if table == "ExportDeclaration":
            self._edge_if_allowed("Exports", owner.node_id, semantic.id, "exports_declaration")
            target = self._resolve_reference_target(_export_target_label(node) or semantic.label, EXPORT_TARGET_TYPES)
            if target is not None and target.id != semantic.id:
                self._edge_if_allowed("Exports", owner.node_id, target.id, "exports_symbol")

        if table in DEFINED_CAPTURE_TABLES:
            self._edge_if_allowed("Defines", owner.node_id, semantic.id, f"defines_{table.lower()}")
            self._edge_if_allowed("Declares", owner.node_id, semantic.id, f"declares_{table.lower()}")

        if table in {"Component", "APIEndpoint", "Route"}:
            self._edge_if_allowed("Exposes", owner.node_id, semantic.id, f"exposes_{table.lower()}")

        if table in {"Route", "APIEndpoint"}:
            target = self._runtime_target(node, owner, syntax_id)
            if target is not None and target.id != semantic.id:
                self._edge_if_allowed("RoutesTo", semantic.id, target.id, "runtime_handler")
                self._edge_if_allowed("Exposes", semantic.id, target.id, "runtime_surface")

        if table == "Parameter":
            self._edge_if_allowed("HasParameter", owner.node_id, semantic.id, "captured_parameter")
            annotation = _field(node, "annotation", "type_annotation")
            if annotation is not None:
                type_node = self._emit_type_annotation(annotation, semantic)
                self._edge("HasTypeAnnotation", semantic.id, type_node.id, "parameter_annotation")

        if table == "ReturnType":
            self._edge_if_allowed("HasReturnType", owner.node_id, semantic.id, "captured_return_type")
            type_node = self._emit_type_annotation(node, semantic)
            self._edge("HasTypeAnnotation", semantic.id, type_node.id, "return_type_annotation")

        if table == "TypeAnnotation":
            self._edge_if_allowed("HasTypeAnnotation", owner.node_id, semantic.id, "captured_type_annotation")
            self._emit_reference_edges(semantic, semantic.label, kind_prefix="type_annotation")

        if table == "TypeAlias":
            annotation = _field(node, "annotation", "type_annotation", "value")
            if annotation is not None:
                type_node = self._emit_type_annotation(annotation, semantic)
                self._edge_if_allowed("HasTypeAnnotation", semantic.id, type_node.id, "type_alias_annotation")

        if table == "Decorator":
            self._edge_if_allowed("DecoratedBy", owner.node_id, semantic.id, "captured_decorator")
            target = self._emit_reference_edges(semantic, semantic.label, kind_prefix="decorator")
            if target is not None:
                self._edge_if_allowed("Calls", semantic.id, target.id, "decorator_call")

        if table == "Query":
            self._edge_if_allowed("ExecutesQuery", owner.node_id, semantic.id, "executes_query")
            self._emit_reference_edges(semantic, _query_reference_label(node), kind_prefix="query")

        if table == "SecretRef":
            self._edge_if_allowed("UsesSecret", owner.node_id, semantic.id, "uses_secret")
            self._emit_reference_edges(semantic, semantic.label, kind_prefix="secret")

        if table in {"DocumentationSource", "DocumentationChunk"}:
            self._edge_if_allowed("Documents", semantic.id, owner.node_id, "documents_owner")
            self._edge_if_allowed("EvidencedBy", semantic.id, syntax_id, "parser_evidence")

        if table == "ExceptionFlow":
            if _is_raise_flow(node):
                self._edge_if_allowed("Raises", owner.node_id, semantic.id, "raises_exception")
            if _is_handle_flow(node):
                self._edge_if_allowed("Handles", owner.node_id, semantic.id, "handles_exception")

        if table == "Reference":
            self._emit_reference_edges(semantic, semantic.label, kind_prefix="reference")

        if table == "ControlFlowBlock":
            self._emit_reference_edges(semantic, _control_flow_reference_label(node), kind_prefix="control_flow")

    def _emit_reference_edges(
        self,
        source: GraphNode,
        label: str,
        *,
        kind_prefix: str,
        target_tables: set[str] | None = None,
    ) -> GraphNode | None:
        """Emit reference edges.

        Args:
            source: Source value.
            label: Label value.
            kind_prefix: Kind prefix value.
            target_tables: Target tables value.

        Returns:
            The computed result.
        """
        target = self._resolve_reference_target(label, target_tables)
        if target is None or target.id == source.id:
            return None
        metadata = {"label": label, "resolver": "label"}
        self._edge_if_allowed("References", source.id, target.id, f"{kind_prefix}_reference", metadata=metadata)
        self._edge_if_allowed("ResolvesTo", source.id, target.id, f"{kind_prefix}_resolution", metadata=metadata)
        return target

    def _connect_owner(self, owner: ScopeFrame, semantic: GraphNode) -> None:
        """Process connect owner.

        Args:
            owner: Owner value.
            semantic: Semantic value.
        """
        self._edge("Contains", owner.node_id, semantic.id, f"contains_{semantic.table.lower()}")
        if owner.scope_id:
            self._edge("Contains", owner.scope_id, semantic.id, f"scope_contains_{semantic.table.lower()}")

    def _support_node(self, table: str, stable_key: str, label: str, *, path: str) -> GraphNode:
        """Process support node.

        Args:
            table: Table value.
            stable_key: Stable key value.
            label: Label value.
            path: The path to read or write.

        Returns:
            The computed result.
        """
        node = GraphNode(
            id=_id(table, stable_key),
            table=table,
            label=label,
            kind=table.lower(),
            path=path,
            summary=label,
            metadata={"canonical_key": stable_key},
        )
        added = self._graph.add_node(node)
        self._register_resolvable(added)
        return added

    def _semantic_node(
        self,
        table: str,
        parser_node: ParserNode,
        *,
        label: str | None = None,
        owner: GraphNode | None = None,
        owner_id: str = "",
        owner_qualified_name: str = "",
        metadata: dict[str, Any] | None = None,
    ) -> GraphNode:
        """Return semantic node.

        Args:
            table: Table value.
            parser_node: Parser node value.
            label: Label value.
            owner: Owner value.
            owner_id: The owner id to identify.
            owner_qualified_name: Owner qualified name value.
            metadata: Metadata value.

        Returns:
            The computed result.
        """
        if table not in self._node_types:
            raise ValueError(f"Unknown ontology node type: {table}")
        semantic_label = label or _label_for(parser_node) or table
        qualified_name = _qualified_name(owner_qualified_name or (owner.qualified_name if owner else ""), semantic_label)
        stable_key = "|".join(
            str(value)
            for value in (
                self._context.path,
                table,
                qualified_name,
                parser_node.node_type,
                parser_node.line_start,
                parser_node.byte_start,
                semantic_label,
            )
        )
        node = GraphNode(
            id=_id(table, stable_key),
            table=table,
            label=semantic_label,
            kind=_kind_for(table, parser_node),
            language=self._context.language,
            path=self._context.path,
            qualified_name=qualified_name,
            scope_id=owner_id or (owner.id if owner else ""),
            line_start=parser_node.line_start,
            line_end=parser_node.line_end,
            byte_start=parser_node.byte_start,
            byte_end=parser_node.byte_end,
            tree_sitter_node_type=parser_node.node_type,
            capture_name=parser_node.capture_name,
            summary=_summary_for(table, semantic_label, parser_node),
            metadata={"canonical_key": stable_key, **(metadata or {})},
        )
        added = self._graph.add_node(node)
        self._register_resolvable(added)
        return added

    def _symbol_node(self, label: str) -> GraphNode | None:
        """Process symbol node.

        Args:
            label: Label value.

        Returns:
            The computed result.
        """
        symbol_label = label.strip()
        if not symbol_label:
            return None
        stable_key = f"{self._context.path}|Symbol|{symbol_label}"
        node = GraphNode(
            id=_id("Symbol", stable_key),
            table="Symbol",
            label=symbol_label,
            kind="symbol_reference",
            language=self._context.language,
            path=self._context.path,
            qualified_name=symbol_label,
            summary=symbol_label,
            metadata={"canonical_key": stable_key, "resolution": "name_placeholder"},
        )
        added = self._graph.add_node(node)
        self._register_resolvable(added)
        return added

    def _register_resolvable(self, node: GraphNode) -> None:
        """Register resolvable.

        Args:
            node: Node value.
        """
        if node.table not in RESOLVABLE_NODE_TYPES:
            return
        keys = {node.label, node.qualified_name, str(node.metadata.get("imported_name") or "")}
        for key in keys:
            normalized = _symbol_key(key)
            if not normalized:
                continue
            self._symbols_by_name.setdefault(normalized, [])
            if node.id not in self._symbols_by_name[normalized]:
                self._symbols_by_name[normalized].append(node.id)

    def _resolve_reference_target(self, label: str, target_tables: set[str] | None = None) -> GraphNode | None:
        """Resolve reference target.

        Args:
            label: Label value.
            target_tables: Target tables value.

        Returns:
            The computed result.
        """
        reference_label = label.strip()
        if not reference_label:
            return None
        candidate_labels = (reference_label, reference_label.rsplit(".", 1)[-1])
        for candidate_label in candidate_labels:
            for node_id in reversed(self._symbols_by_name.get(_symbol_key(candidate_label), ())):
                node = self._graph.nodes.get(node_id)
                if node is not None and (target_tables is None or node.table in target_tables):
                    return node
        if target_tables is not None and "Symbol" not in target_tables:
            return None
        return self._symbol_node(reference_label)

    def _scope_for(self, owner: GraphNode) -> GraphNode:
        """Process scope for.

        Args:
            owner: Owner value.

        Returns:
            The computed result.
        """
        stable_key = f"{self._context.path}|{owner.id}|scope"
        scope = GraphNode(
            id=_id("Scope", stable_key),
            table="Scope",
            label=f"{owner.label} scope",
            kind=f"{owner.table.lower()}_scope",
            language=owner.language,
            path=owner.path,
            qualified_name=f"{owner.qualified_name or owner.label}.<scope>",
            scope_id=owner.id,
            line_start=owner.line_start,
            line_end=owner.line_end,
            byte_start=owner.byte_start,
            byte_end=owner.byte_end,
            summary=f"Scope for {owner.label}",
            metadata={"canonical_key": stable_key},
        )
        return self._graph.add_node(scope)

    def _syntax_capture(self, node: ParserNode) -> str:
        """Return syntax capture.

        Args:
            node: Node value.

        Returns:
            The computed string.
        """
        stable_key = "|".join(
            str(value)
            for value in (self._context.path, node.node_type, node.line_start, node.byte_start, _label_for(node))
        )
        syntax_id = _id("SyntaxCapture", stable_key)
        if not self.include_syntax_captures:
            return syntax_id
        if id(node) in self._syntax_nodes:
            return self._syntax_nodes[id(node)]
        syntax = GraphNode(
            id=syntax_id,
            table="SyntaxCapture",
            label=node.capture_name or node.node_type,
            kind=node.node_type,
            language=self._context.language,
            path=self._context.path,
            line_start=node.line_start,
            line_end=node.line_end,
            byte_start=node.byte_start,
            byte_end=node.byte_end,
            tree_sitter_node_type=node.node_type,
            capture_name=node.capture_name,
            summary=node.text[:160],
            metadata={"canonical_key": stable_key, "fields": sorted(node.fields.keys())},
        )
        self._graph.add_node(syntax)
        self._syntax_nodes[id(node)] = syntax_id
        return syntax_id

    def _derived_from(self, semantic_id: str, syntax_id: str) -> None:
        """Process derived from.

        Args:
            semantic_id: The semantic id to identify.
            syntax_id: The syntax id to identify.
        """
        if self.include_syntax_captures and syntax_id in self._graph.nodes:
            self._edge("DerivedFrom", semantic_id, syntax_id, "parser_capture")

    def _runtime_target(self, node: ParserNode, owner: ScopeFrame, syntax_id: str) -> GraphNode | None:
        """Process runtime target.

        Args:
            node: Node value.
            owner: Owner value.
            syntax_id: The syntax id to identify.

        Returns:
            The computed result.
        """
        label = _runtime_target_label(node)
        if label:
            target = self._resolve_reference_target(label, RUNTIME_TARGET_TYPES)
            if target is not None:
                return target
            endpoint = self._semantic_node(
                "APIEndpoint",
                node,
                label=label,
                owner_id=owner.node_id,
                owner_qualified_name=owner.qualified_name,
                metadata={"inferred_from": "runtime_target"},
            )
            self._connect_owner(owner, endpoint)
            self._edge_if_allowed("Defines", owner.node_id, endpoint.id, "defines_inferred_endpoint")
            self._edge_if_allowed("Exposes", owner.node_id, endpoint.id, "exposes_inferred_endpoint")
            self._derived_from(endpoint.id, syntax_id)
            return endpoint
        if owner.table in RUNTIME_TARGET_TYPES:
            return self._graph.nodes.get(owner.node_id)
        return None

    def _edge_if_allowed(
        self,
        edge_type: str,
        source_id: str,
        target_id: str,
        kind: str,
        *,
        metadata: dict[str, Any] | None = None,
    ) -> GraphEdge | None:
        """Process edge if allowed.

        Args:
            edge_type: Edge type value.
            source_id: The source id to identify.
            target_id: The target id to identify.
            kind: Kind value.
            metadata: Metadata value.

        Returns:
            The computed result.
        """
        source = self._graph.nodes.get(source_id)
        target = self._graph.nodes.get(target_id)
        if source is None or target is None:
            return None
        spec = get_relation_type(edge_type)
        if source.table not in spec.source_types or target.table not in spec.target_types:
            return None
        return self._edge(edge_type, source_id, target_id, kind, metadata=metadata)

    def _edge(
        self,
        edge_type: str,
        source_id: str,
        target_id: str,
        kind: str,
        *,
        metadata: dict[str, Any] | None = None,
    ) -> GraphEdge:
        """Process edge.

        Args:
            edge_type: Edge type value.
            source_id: The source id to identify.
            target_id: The target id to identify.
            kind: Kind value.
            metadata: Metadata value.

        Returns:
            The computed result.
        """
        if edge_type not in self._relation_types:
            raise ValueError(f"Unknown ontology relation type: {edge_type}")
        edge = GraphEdge(
            id=_id("edge", f"{edge_type}|{source_id}|{target_id}|{kind}"),
            type=edge_type,
            source_id=source_id,
            target_id=target_id,
            kind=kind,
            metadata={"canonical_key": f"{edge_type}|{source_id}|{target_id}|{kind}", **(metadata or {})},
        )
        return self._graph.add_edge(edge)

    def _normalize(self, raw_node: Any) -> ParserNode:
        """Normalize result.

        Args:
            raw_node: Raw node value.

        Returns:
            The computed result.
        """
        if isinstance(raw_node, ParserNode):
            return raw_node
        if isinstance(raw_node, Mapping):
            nested = raw_node.get("node")
            if nested is not None:
                nested_node = self._normalize(nested)
                return ParserNode(
                    node_type=str(raw_node.get("type") or nested_node.node_type),
                    fields={**nested_node.fields, **{key: value for key, value in raw_node.items() if key != "node"}},
                    children=nested_node.children,
                    line_start=nested_node.line_start,
                    line_end=nested_node.line_end,
                    byte_start=nested_node.byte_start,
                    byte_end=nested_node.byte_end,
                    capture_name=str(raw_node.get("capture_name") or nested_node.capture_name or ""),
                    text=nested_node.text,
                )
            fields = {key: value for key, value in raw_node.items() if key not in DICT_NODE_META_KEYS}
            children = tuple(_coerce_children(raw_node))
            return ParserNode(
                node_type=str(raw_node.get("type") or raw_node.get("node_type") or raw_node.get("kind") or "unknown"),
                fields=fields,
                children=children,
                line_start=_line(raw_node, "line_start", "start_line"),
                line_end=_line(raw_node, "line_end", "end_line"),
                byte_start=_line(raw_node, "byte_start", "start_byte"),
                byte_end=_line(raw_node, "byte_end", "end_byte"),
                capture_name=str(raw_node.get("capture_name") or raw_node.get("capture") or ""),
                text=str(raw_node.get("text") or ""),
            )
        node_type = getattr(raw_node, "type", "") or type(raw_node).__name__
        fields = _object_fields(raw_node)
        return ParserNode(
            node_type=str(node_type),
            fields=fields,
            children=tuple(getattr(raw_node, "children", ()) or _field_children(fields)),
            line_start=_point_line(getattr(raw_node, "start_point", None)) or getattr(raw_node, "lineno", None),
            line_end=_point_line(getattr(raw_node, "end_point", None)) or getattr(raw_node, "end_lineno", None),
            byte_start=getattr(raw_node, "start_byte", None) or getattr(raw_node, "col_offset", None),
            byte_end=getattr(raw_node, "end_byte", None) or getattr(raw_node, "end_col_offset", None),
            text=_node_text(raw_node),
        )

    def _semantic_children(self, node: ParserNode) -> tuple[Any, ...]:
        """Return semantic children.

        Args:
            node: Node value.

        Returns:
            A tuple containing the computed values.
        """
        ignored_fields = {"name", "id", "module", "names", "args", "returns", "return_type", "decorator_list", "decorators"}
        children: list[Any] = list(node.children)
        for field_name, value in node.fields.items():
            if field_name in ignored_fields:
                continue
            if _is_parser_like(value):
                children.append(value)
            elif isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
                children.extend(item for item in value if _is_parser_like(item))
        return tuple(children)


IMPORT_NODE_TYPES = {"import_statement", "import_from_statement", "import_declaration", "Import", "ImportFrom"}
EXPORT_NODE_TYPES = {"export_statement", "export_clause", "export_declaration"}
CLASS_NODE_TYPES = {"class_definition", "class_declaration", "struct_item", "interface_declaration", "ClassDef"}
FUNCTION_NODE_TYPES = {"function_definition", "function_declaration", "method_definition", "method_declaration", "FunctionDef"}
PARAMETER_NODE_TYPES = {"parameter", "typed_parameter", "default_parameter", "arg"}
RETURN_TYPE_NODE_TYPES = {"return_type", "returns"}
TYPE_NODE_TYPES = {"type", "type_identifier", "type_annotation", "annotation"}
ASSIGNMENT_NODE_TYPES = {"assignment", "assignment_expression", "variable_declaration", "Assign", "AnnAssign"}
CALL_NODE_TYPES = {"call", "call_expression", "invocation_expression", "Call"}
REFERENCE_NODE_TYPES = {"identifier", "field_identifier", "attribute", "Name", "Attribute"}
LITERAL_NODE_TYPES = {"string", "integer", "float", "true", "false", "null", "none", "Constant"}
CONTROL_FLOW_NODE_TYPES = {"if_statement", "for_statement", "while_statement", "match_statement", "switch_statement"}
EXCEPTION_FLOW_NODE_TYPES = {"try_statement", "except_clause", "catch_clause", "raise_statement", "throw_statement"}
RESOLVABLE_NODE_TYPES = {
    "Symbol",
    "Module",
    "Class",
    "Function",
    "Method",
    "Variable",
    "Constant",
    "ClassAttribute",
    "InstanceAttribute",
    "Property",
    "Parameter",
    "Dependency",
    "APIEndpoint",
    "Component",
}
EXPORT_TARGET_TYPES = {
    "Class",
    "Function",
    "Method",
    "Variable",
    "Constant",
    "ClassAttribute",
    "InstanceAttribute",
    "Property",
    "APIEndpoint",
    "Component",
}
RUNTIME_TARGET_TYPES = {"Function", "Method", "Component", "APIEndpoint"}
IMPORT_SOURCE_TYPES = {"File", "Module", "Scope"}
DEFINED_CAPTURE_TABLES = {
    "APIEndpoint",
    "Component",
    "Route",
    "TypeAlias",
    "Variable",
    "Constant",
    "ClassAttribute",
    "InstanceAttribute",
    "Property",
}
DICT_NODE_META_KEYS = {
    "type",
    "node_type",
    "kind",
    "children",
    "body",
    "line_start",
    "line_end",
    "start_line",
    "end_line",
    "byte_start",
    "byte_end",
    "start_byte",
    "end_byte",
    "capture",
    "capture_name",
    "text",
}


def _capture_node(capture: Mapping[str, Any] | tuple[Any, str]) -> Any:
    """Process capture node.

    Args:
        capture: Capture value.

    Returns:
        The computed result.
    """
    if isinstance(capture, CaptureRecord):
        return capture.node
    if isinstance(capture, tuple):
        return capture[0]
    return capture.get("node") or capture


def _capture_name(capture: Mapping[str, Any] | tuple[Any, str]) -> str:
    """Process capture name.

    Args:
        capture: Capture value.

    Returns:
        The computed string.
    """
    if isinstance(capture, CaptureRecord):
        return capture.capture
    if isinstance(capture, tuple):
        return str(capture[1])
    return str(capture.get("capture_name") or capture.get("capture") or "")


def _capture_node_type(capture: Mapping[str, Any] | tuple[Any, str]) -> str:
    """Process capture node type.

    Args:
        capture: Capture value.

    Returns:
        The computed string.
    """
    node = _capture_node(capture)
    if isinstance(node, Mapping):
        return str(node.get("type") or node.get("node_type") or node.get("kind") or "unknown")
    return str(getattr(node, "type", "") or type(node).__name__)


def _table_from_capture(capture_name: str, owner: ScopeFrame) -> str | None:
    """Return table for from capture.

    Args:
        capture_name: Capture name value.
        owner: Owner value.

    Returns:
        The computed result.
    """
    return default_capture_table_registry().table_for(capture_name, owner)


def _normalize_capture_name(capture_name: str) -> str:
    """Normalize capture name.

    Args:
        capture_name: Capture name value.

    Returns:
        The computed string.
    """
    return capture_name.lstrip("@")


def _resolve_capture_table(table: str | CaptureTableResolver, capture: str, owner: ScopeFrame) -> str | None:
    """Resolve capture table.

    Args:
        table: Table value.
        capture: Capture value.
        owner: Owner value.

    Returns:
        The computed result.
    """
    if callable(table):
        return table(capture, owner)
    return table


def _function_capture_table(_capture: str, owner: ScopeFrame) -> str:
    """Process function capture table.

    Args:
        _capture: Capture value.
        owner: Owner value.

    Returns:
        The computed string.
    """
    return "Method" if owner.table in {"Class", "Component"} else "Function"


def _import_source_id(owner: ScopeFrame) -> str:
    """Process import source ID.

    Args:
        owner: Owner value.

    Returns:
        The computed string.
    """
    if owner.table in IMPORT_SOURCE_TYPES:
        return owner.node_id
    return owner.scope_id or owner.node_id


def _id(prefix: str, value: str) -> str:
    """Process ID.

    Args:
        prefix: Prefix value.
        value: Value value.

    Returns:
        The computed string.
    """
    return f"{prefix}:{hashlib.sha1(value.encode('utf-8')).hexdigest()[:20]}"


def _module_label(path: str) -> str:
    """Process module label.

    Args:
        path: The path to read or write.

    Returns:
        The computed string.
    """
    stem = path.rsplit(".", 1)[0]
    return stem.replace("/", ".")


def _qualified_name(owner: str, label: str) -> str:
    """Process qualified name.

    Args:
        owner: Owner value.
        label: Label value.

    Returns:
        The computed string.
    """
    if not owner or owner == label:
        return label
    if not label:
        return owner
    return f"{owner}.{label}"


def _kind_for(table: str, node: ParserNode) -> str:
    """Process kind for.

    Args:
        table: Table value.
        node: Node value.

    Returns:
        The computed string.
    """
    if table == "Method":
        return "method"
    if table == "Function":
        return "function"
    if table == "Class":
        return "class"
    return node.node_type


def _field(node: ParserNode, *names: str) -> Any:
    """Return result field data.

    Args:
        node: Node value.
        names: Names value.

    Returns:
        The computed result.
    """
    for name in names:
        if name in node.fields:
            return node.fields[name]
    return None


def _iter_field_items(node: ParserNode, *names: str) -> tuple[Any, ...]:
    """Iterate over field items.

    Args:
        node: Node value.
        names: Names value.

    Returns:
        A tuple containing the computed values.
    """
    items: list[Any] = []
    for name in names:
        value = node.fields.get(name)
        if value is None:
            continue
        if isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
            items.extend(value)
        else:
            items.append(value)
    return tuple(items)


def _label_for(node: ParserNode) -> str:
    """Process label for.

    Args:
        node: Node value.

    Returns:
        The computed string.
    """
    for key in ("name", "id", "arg", "attr", "module"):
        value = node.fields.get(key)
        label = _value_label(value)
        if label:
            return label
    if "value" in node.fields:
        return _value_label(node.fields["value"])
    return node.text.strip() or node.node_type


def _summary_for(table: str, label: str, node: ParserNode) -> str:
    """Return summary for for.

    Args:
        table: Table value.
        label: Label value.
        node: Node value.

    Returns:
        The computed string.
    """
    if table in {"DocumentationSource", "DocumentationChunk"} and node.text.strip():
        return node.text.strip()
    return label


def _value_label(value: Any) -> str:
    """Return value for label.

    Args:
        value: Value value.

    Returns:
        The computed string.
    """
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    if isinstance(value, (int, float, bool)):
        return str(value)
    if isinstance(value, Mapping):
        if "id" in value:
            return str(value["id"])
        if "name" in value:
            return str(value["name"])
        if "arg" in value:
            return str(value["arg"])
        if "attr" in value:
            base = _value_label(value.get("value"))
            return f"{base}.{value['attr']}" if base else str(value["attr"])
        if "value" in value:
            return _value_label(value["value"])
    if hasattr(value, "id"):
        return str(getattr(value, "id"))
    if hasattr(value, "name"):
        return str(getattr(value, "name"))
    if hasattr(value, "arg"):
        return str(getattr(value, "arg"))
    if hasattr(value, "attr"):
        base = _value_label(getattr(value, "value", None))
        return f"{base}.{getattr(value, 'attr')}" if base else str(getattr(value, "attr"))
    if hasattr(value, "value"):
        return _value_label(getattr(value, "value"))
    return ""


def _symbol_key(label: str) -> str:
    """Process symbol key.

    Args:
        label: Label value.

    Returns:
        The computed string.
    """
    return label.strip().lower()


def _export_target_label(node: ParserNode) -> str:
    """Process export target label.

    Args:
        node: Node value.

    Returns:
        The computed string.
    """
    for field_name in ("exported", "target", "name", "declaration"):
        label = _value_label(node.fields.get(field_name))
        if label:
            return label
    return _label_for(node)


def _runtime_target_label(node: ParserNode) -> str:
    """Process runtime target label.

    Args:
        node: Node value.

    Returns:
        The computed string.
    """
    for field_name in ("handler", "endpoint", "target", "function", "callback"):
        label = _value_label(node.fields.get(field_name))
        if label:
            return label
    return ""


def _query_reference_label(node: ParserNode) -> str:
    """Return query reference label.

    Args:
        node: Node value.

    Returns:
        The computed string.
    """
    for field_name in ("table", "collection", "model", "target", "index"):
        label = _value_label(node.fields.get(field_name))
        if label:
            return label
    return ""


def _control_flow_reference_label(node: ParserNode) -> str:
    """Process control flow reference label.

    Args:
        node: Node value.

    Returns:
        The computed string.
    """
    for field_name in ("test", "condition", "subject"):
        label = _value_label(node.fields.get(field_name))
        if label:
            return label
    return ""


def _is_raise_flow(node: ParserNode) -> bool:
    """Return whether raise flow.

    Args:
        node: Node value.

    Returns:
        Whether the check succeeds.
    """
    capture = node.capture_name.lstrip("@")
    return capture == "raises" or node.node_type in {"raise_statement", "throw_statement"}


def _is_handle_flow(node: ParserNode) -> bool:
    """Return whether handle flow.

    Args:
        node: Node value.

    Returns:
        Whether the check succeeds.
    """
    capture = node.capture_name.lstrip("@")
    return capture == "handles" or node.node_type in {"try_statement", "except_clause", "catch_clause"}


def _import_label(node: ParserNode) -> str:
    """Process import label.

    Args:
        node: Node value.

    Returns:
        The computed string.
    """
    module = _value_label(node.fields.get("module"))
    names = node.fields.get("names")
    imported_names: list[str] = []
    if isinstance(names, Sequence) and not isinstance(names, (str, bytes, bytearray)):
        imported_names = [_value_label(name) for name in names if _value_label(name)]
    elif names is not None:
        imported_names = [_value_label(names)]
    if module and imported_names:
        return ", ".join(f"{module}.{name}" for name in imported_names)
    return module or ", ".join(imported_names)


def _call_label(node: ParserNode) -> str:
    """Call label.

    Args:
        node: Node value.

    Returns:
        The computed string.
    """
    return _value_label(node.fields.get("func")) or _value_label(node.fields.get("function"))


def _assignment_target_label(node: ParserNode) -> str:
    """Process assignment target label.

    Args:
        node: Node value.

    Returns:
        The computed string.
    """
    target = node.fields.get("target")
    targets = node.fields.get("targets")
    if target is not None:
        return _value_label(target)
    if isinstance(targets, Sequence) and not isinstance(targets, (str, bytes, bytearray)) and targets:
        return _value_label(targets[0])
    return _value_label(targets)


def _assignment_target_table(label: str, owner: ScopeFrame, node: ParserNode) -> str:
    """Process assignment target table.

    Args:
        label: Label value.
        owner: Owner value.
        node: Node value.

    Returns:
        The computed string.
    """
    if label.isupper():
        return "Constant"
    if owner.table == "Class":
        return "ClassAttribute"
    if "." in label:
        return "InstanceAttribute"
    if node.node_type == "AnnAssign" and owner.table == "Class":
        return "ClassAttribute"
    return "Variable"


def _parameters(node: ParserNode) -> tuple[Any, ...]:
    """Process parameters.

    Args:
        node: Node value.

    Returns:
        A tuple containing the computed values.
    """
    raw_args = node.fields.get("args") or node.fields.get("parameters")
    if raw_args is None:
        return ()
    if isinstance(raw_args, Mapping):
        args = raw_args.get("args") or raw_args.get("children") or ()
        if isinstance(args, Sequence) and not isinstance(args, (str, bytes, bytearray)):
            return tuple(args)
    if hasattr(raw_args, "args"):
        args = getattr(raw_args, "args")
        if isinstance(args, Sequence):
            return tuple(args)
    if isinstance(raw_args, Sequence) and not isinstance(raw_args, (str, bytes, bytearray)):
        return tuple(raw_args)
    return (raw_args,)


def _normalized_type(raw_node: Any) -> str:
    """Normalize type.

    Args:
        raw_node: Raw node value.

    Returns:
        The computed string.
    """
    if isinstance(raw_node, ParserNode):
        return raw_node.node_type
    if isinstance(raw_node, Mapping):
        return str(raw_node.get("type") or raw_node.get("node_type") or raw_node.get("kind") or "unknown")
    return str(getattr(raw_node, "type", "") or type(raw_node).__name__)


def _coerce_children(raw_node: Mapping[str, Any]) -> tuple[Any, ...]:
    """Coerce children.

    Args:
        raw_node: Raw node value.

    Returns:
        A tuple containing the computed values.
    """
    children: list[Any] = []
    for key in ("children", "body"):
        value = raw_node.get(key)
        if isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
            children.extend(value)
        elif value is not None:
            children.append(value)
    return tuple(children)


def _field_children(fields: Mapping[str, Any]) -> tuple[Any, ...]:
    """Return children field data.

    Args:
        fields: Fields value.

    Returns:
        A tuple containing the computed values.
    """
    children: list[Any] = []
    for value in fields.values():
        if _is_parser_like(value):
            children.append(value)
        elif isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
            children.extend(item for item in value if _is_parser_like(item))
    return tuple(children)


def _object_fields(raw_node: Any) -> Mapping[str, Any]:
    """Return object fields.

    Args:
        raw_node: Raw node value.

    Returns:
        A dictionary containing the computed payload.
    """
    if hasattr(raw_node, "_fields"):
        return {name: getattr(raw_node, name) for name in getattr(raw_node, "_fields")}
    if hasattr(raw_node, "child_by_field_name"):
        fields: dict[str, Any] = {}
        for name in ("name", "body", "parameters", "return_type", "function", "argument", "left", "right"):
            try:
                value = raw_node.child_by_field_name(name)
            except Exception:
                value = None
            if value is not None:
                fields[name] = value
        return fields
    return {
        key: value
        for key, value in vars(raw_node).items()
        if not key.startswith("_") and key not in {"children", "type"}
    } if hasattr(raw_node, "__dict__") else {}


def _is_parser_like(value: Any) -> bool:
    """Return whether parser like.

    Args:
        value: Value value.

    Returns:
        Whether the check succeeds.
    """
    if value is None or isinstance(value, (str, bytes, bytearray, int, float, bool)):
        return False
    if isinstance(value, Mapping):
        return any(key in value for key in ("type", "node_type", "kind", "body", "children"))
    return hasattr(value, "type") or hasattr(value, "_fields")


def _line(raw_node: Mapping[str, Any], *keys: str) -> int | None:
    """Process line.

    Args:
        raw_node: Raw node value.
        keys: Keys value.

    Returns:
        The computed result.
    """
    for key in keys:
        value = raw_node.get(key)
        if isinstance(value, int):
            return value
    start_point = raw_node.get("start_point")
    end_point = raw_node.get("end_point")
    if "start" in keys[0] and start_point is not None:
        return _point_line(start_point)
    if "end" in keys[0] and end_point is not None:
        return _point_line(end_point)
    return None


def _point_line(point: Any) -> int | None:
    """Process point line.

    Args:
        point: Point value.

    Returns:
        The computed result.
    """
    if point is None:
        return None
    if isinstance(point, Sequence) and point:
        return int(point[0]) + 1
    if hasattr(point, "row"):
        return int(getattr(point, "row")) + 1
    return None


def _node_text(raw_node: Any) -> str:
    """Return node text.

    Args:
        raw_node: Raw node value.

    Returns:
        The computed string.
    """
    text = getattr(raw_node, "text", b"")
    if isinstance(text, bytes):
        return text.decode("utf-8", errors="replace")
    return str(text or "")
