from __future__ import annotations

ONTOLOGY_NAME = "codebase_graph_v1"

NODE_TABLES = (
    "Project",
    "Repository",
    "File",
    "DocumentationSource",
    "PythonModule",
    "PythonClass",
    "PythonFunction",
    "PythonMethod",
    "Import",
    "Call",
    "Dependency",
    "EntryPoint",
    "Test",
    "Risk",
    "Verification",
)

EDGE_NODE_TABLES = (
    "Contains",
    "Imports",
    "Calls",
    "DependsOn",
    "Defines",
    "CoveredBy",
    "RoutesTo",
    "Describes",
    "Produces",
)

TABLE_COLUMNS: dict[str, tuple[str, ...]] = {
    table: (
        "id",
        "label",
        "kind",
        "path",
        "qualified_name",
        "module_name",
        "line_start",
        "line_end",
        "summary",
        "metadata",
    )
    for table in NODE_TABLES
}

VECTOR_INDEXES: tuple[tuple[str, str, str], ...] = ()
FTS_INDEXES: tuple[tuple[str, str, tuple[str, ...]], ...] = (
    ("File", "idx_file_text", ("label", "path", "summary")),
    ("PythonClass", "idx_python_class_text", ("label", "qualified_name", "summary")),
    ("PythonFunction", "idx_python_function_text", ("label", "qualified_name", "summary")),
    ("PythonMethod", "idx_python_method_text", ("label", "qualified_name", "summary")),
    ("DocumentationSource", "idx_documentation_source_text", ("label", "path", "summary")),
)

def schema_payload() -> dict[str, object]:
    return {
        "ontology": ONTOLOGY_NAME,
        "node_tables": list(NODE_TABLES),
        "edge_tables": list(EDGE_NODE_TABLES),
        "table_columns": {name: list(columns) for name, columns in TABLE_COLUMNS.items()},
        "vector_indexes": [
            {"table": table, "index": index, "column": column}
            for table, index, column in VECTOR_INDEXES
        ],
        "fts_indexes": [
            {"table": table, "index": index, "columns": list(columns)}
            for table, index, columns in FTS_INDEXES
        ],
        "examples": [
            "MATCH (n:PythonClass) RETURN n.id, n.label, n.qualified_name LIMIT 5",
            "MATCH (n:File) RETURN n.path, n.summary LIMIT 10",
            "MATCH (n:EntryPoint) RETURN n.label, n.kind, n.path LIMIT 5",
        ],
    }
