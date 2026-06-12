from __future__ import annotations

from codebase_graph.core import CodeGraph
from codebase_graph.extract import GraphBuilder
from codebase_graph.ingest import parse_profiled_source, resolve_language_profile


SAMPLES = {
    "rust": (
        "src/lib.rs",
        "use std::fmt;\n"
        "struct User { id: i32 }\n"
        "enum Kind { A }\n"
        "trait Named { fn name(&self) -> String; }\n"
        "impl User { fn new(id: i32) -> Self { println!(\"{}\", id); User { id } } }\n"
        "fn helper() { User::new(1); }\n",
        {
            "Class": {"User", "Kind", "Named"},
            "Function": {"helper"},
            "Method": {"new", "name"},
            "ImportDeclaration": {"std::fmt"},
            "CallExpression": {"User::new"},
        },
    ),
    "go": (
        "main.go",
        "package main\n"
        "import \"fmt\"\n"
        "type User struct { ID int }\n"
        "type Named interface { Name() string }\n"
        "func (u User) Name() string { fmt.Println(u.ID); return \"\" }\n"
        "func helper() { fmt.Println(1) }\n",
        {
            "Class": {"User", "Named"},
            "Function": {"helper"},
            "Method": {"Name"},
            "ImportDeclaration": {"fmt"},
            "CallExpression": {"fmt.Println"},
        },
    ),
    "c": (
        "lib.c",
        "#include <stdio.h>\n"
        "#define SIZE 1\n"
        "struct User { int id; };\n"
        "enum Kind { A };\n"
        "void helper(void) { printf(\"%d\", SIZE); }\n",
        {
            "Class": {"User", "Kind"},
            "Function": {"helper"},
            "ImportDeclaration": {"stdio.h"},
            "CallExpression": {"printf"},
            "Symbol": {"SIZE"},
        },
    ),
    "cpp": (
        "lib.cpp",
        "#include <string>\n"
        "namespace app {\n"
        "class User { public: std::string name(); };\n"
        "std::string User::name() { return std::string(); }\n"
        "void helper() { User u; u.name(); }\n"
        "}\n",
        {
            "Module": {"app"},
            "Class": {"User"},
            "Function": {"helper"},
            "Method": {"name"},
            "ImportDeclaration": {"string"},
            "CallExpression": {"u.name"},
        },
    ),
    "fortran": (
        "solver.f90",
        "module math_mod\n"
        "use iso_fortran_env\n"
        "contains\n"
        "subroutine greet()\n"
        "call print_hello()\n"
        "end subroutine greet\n"
        "function add(a,b) result(c)\n"
        "integer :: a,b,c\n"
        "c = a+b\n"
        "end function add\n"
        "end module math_mod\n",
        {
            "Module": {"math_mod"},
            "Function": {"greet", "add"},
            "ImportDeclaration": {"iso_fortran_env"},
            "CallExpression": {"print_hello"},
        },
    ),
}


def test_supported_language_fixtures_emit_core_graph_semantics() -> None:
    for language, (path, source_text, expected) in SAMPLES.items():
        graph = _graph_for(language, path, source_text)

        for table, labels in expected.items():
            assert labels <= _labels(graph, table), f"{language} missing {table} labels"


def _graph_for(language: str, path: str, source_text: str) -> CodeGraph:
    profile = resolve_language_profile(language)
    assert profile is not None
    bundle = parse_profiled_source(
        source_text,
        profile,
        relative_path=path,
        source_root=".",
        repository_label="repo",
        content_hash="hash",
    )
    return GraphBuilder().build_file_graph(bundle).graph


def _labels(graph: CodeGraph, table: str) -> set[str]:
    return {node.label for node in graph.nodes_by_type(table)}
