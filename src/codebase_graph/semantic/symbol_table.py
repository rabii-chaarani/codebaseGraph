from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass

from codebase_graph.core import CodeGraph, GraphNode


DECLARATION_TABLES = {
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
    "TypeAlias",
}


@dataclass(frozen=True, slots=True)
class SymbolRecord:
    """Declaration index row for a graph node that introduces a symbol."""

    symbol_id: str
    name: str
    qualified_name: str
    node_id: str
    table: str
    language: str
    scope_id: str
    visibility: str = ""


@dataclass(frozen=True, slots=True)
class ScopeRecord:
    """Lexical or package scope used during local reference resolution."""

    scope_id: str
    owner_node_id: str
    parent_scope_id: str
    language: str
    qualified_name: str


@dataclass(frozen=True, slots=True)
class ImportBinding:
    """Import, include, use, or module binding that connects a scope to a namespace."""

    import_node_id: str
    imported_name: str
    alias: str
    source_scope_id: str
    target_hint: str


@dataclass(frozen=True, slots=True)
class ProjectSymbolTable:
    """Project-level symbol, scope, and import indexes."""

    symbols: tuple[SymbolRecord, ...]
    scopes: tuple[ScopeRecord, ...]
    imports: tuple[ImportBinding, ...]
    by_name: dict[str, tuple[SymbolRecord, ...]]
    by_node_id: dict[str, SymbolRecord]


def build_project_symbol_table(graphs: CodeGraph | Iterable[CodeGraph]) -> ProjectSymbolTable:
    """Build declaration, scope, import, and export indexes from syntax graphs."""
    graph_list = _graph_tuple(graphs)
    symbols = index_symbol_exports(collect_symbol_declarations(graph_list), graph_list)
    scopes = build_scope_index(graph_list)
    imports = collect_import_bindings(graph_list)
    by_name: dict[str, list[SymbolRecord]] = {}
    for symbol in symbols:
        for key in _symbol_keys(symbol.name, symbol.qualified_name):
            by_name.setdefault(key, []).append(symbol)
    return ProjectSymbolTable(
        symbols=symbols,
        scopes=scopes,
        imports=imports,
        by_name={key: tuple(value) for key, value in by_name.items()},
        by_node_id={symbol.node_id: symbol for symbol in symbols},
    )


def collect_symbol_declarations(graphs: CodeGraph | Iterable[CodeGraph]) -> tuple[SymbolRecord, ...]:
    """Collect declarations from graph nodes."""
    records: list[SymbolRecord] = []
    for graph in _graph_tuple(graphs):
        for node in graph.nodes.values():
            if node.table not in DECLARATION_TABLES:
                continue
            name = node.label.strip()
            if not name:
                continue
            records.append(
                SymbolRecord(
                    symbol_id=f"{node.table}:{node.id}",
                    name=name,
                    qualified_name=node.qualified_name or name,
                    node_id=node.id,
                    table=node.table,
                    language=node.language,
                    scope_id=node.scope_id,
                    visibility=_visibility(node),
                )
            )
    return tuple(sorted(records, key=lambda item: (item.qualified_name, item.table, item.node_id)))


def build_scope_index(graphs: CodeGraph | Iterable[CodeGraph]) -> tuple[ScopeRecord, ...]:
    """Build parent-child lexical and package scope relationships."""
    records: list[ScopeRecord] = []
    for graph in _graph_tuple(graphs):
        for node in graph.nodes_by_type("Scope"):
            owner = graph.nodes.get(node.scope_id)
            records.append(
                ScopeRecord(
                    scope_id=node.id,
                    owner_node_id=node.scope_id,
                    parent_scope_id=owner.scope_id if owner is not None else "",
                    language=node.language,
                    qualified_name=node.qualified_name,
                )
            )
    return tuple(sorted(records, key=lambda item: item.scope_id))


def collect_import_bindings(graphs: CodeGraph | Iterable[CodeGraph]) -> tuple[ImportBinding, ...]:
    """Collect imports, includes, use declarations, and aliases."""
    bindings: list[ImportBinding] = []
    for graph in _graph_tuple(graphs):
        for node in graph.nodes_by_type("ImportDeclaration"):
            imported = str(node.metadata.get("imported_name") or node.label or "").strip()
            if not imported:
                continue
            alias = _alias_for(imported)
            bindings.append(
                ImportBinding(
                    import_node_id=node.id,
                    imported_name=imported,
                    alias=alias,
                    source_scope_id=node.scope_id,
                    target_hint=imported,
                )
            )
    return tuple(sorted(bindings, key=lambda item: (item.imported_name, item.import_node_id)))


def index_symbol_exports(
    symbols: Iterable[SymbolRecord],
    graphs: CodeGraph | Iterable[CodeGraph],
) -> tuple[SymbolRecord, ...]:
    """Mark public, exported, and package-visible symbols according to graph evidence."""
    exported_targets = {
        edge.target_id
        for graph in _graph_tuple(graphs)
        for edge in graph.edges_by_type("Exports")
    }
    indexed: list[SymbolRecord] = []
    for symbol in symbols:
        visibility = "exported" if symbol.node_id in exported_targets else symbol.visibility
        indexed.append(
            SymbolRecord(
                symbol_id=symbol.symbol_id,
                name=symbol.name,
                qualified_name=symbol.qualified_name,
                node_id=symbol.node_id,
                table=symbol.table,
                language=symbol.language,
                scope_id=symbol.scope_id,
                visibility=visibility,
            )
        )
    return tuple(indexed)


def candidate_symbol_keys(label: str) -> tuple[str, ...]:
    """Return language-neutral lookup keys for a reference label."""
    text = label.strip()
    if not text:
        return ()
    parts = {text}
    for delimiter in (".", "::", "->"):
        if delimiter in text:
            parts.add(text.rsplit(delimiter, 1)[-1])
    if "/" in text:
        parts.add(text.rsplit("/", 1)[-1])
    return tuple(sorted(_normalize_symbol_key(part) for part in parts if _normalize_symbol_key(part)))


def _graph_tuple(graphs: CodeGraph | Iterable[CodeGraph]) -> tuple[CodeGraph, ...]:
    if isinstance(graphs, CodeGraph):
        return (graphs,)
    return tuple(graphs)


def _symbol_keys(name: str, qualified_name: str) -> tuple[str, ...]:
    keys = set(candidate_symbol_keys(name))
    keys.update(candidate_symbol_keys(qualified_name))
    return tuple(sorted(keys))


def _normalize_symbol_key(value: str) -> str:
    return value.strip().lower().replace("_", "")


def _visibility(node: GraphNode) -> str:
    if node.table == "Dependency":
        return "external"
    if node.label.startswith("_"):
        return "private"
    if node.label[:1].isupper() or node.table in {"Module", "Class", "Function", "Method", "TypeAlias"}:
        return "public"
    return "local"


def _alias_for(imported: str) -> str:
    text = imported.strip()
    if " as " in text:
        return text.rsplit(" as ", 1)[-1].strip()
    for delimiter in (".", "::", "/"):
        if delimiter in text:
            return text.rsplit(delimiter, 1)[-1].strip()
    return text
