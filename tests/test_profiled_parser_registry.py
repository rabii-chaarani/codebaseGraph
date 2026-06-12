from __future__ import annotations

from pathlib import Path

from codebase_graph.ingest import ParserRegistry, resolve_language_profile
from codebase_graph.ingest import default_parser_registry
from codebase_graph.ingest.tree_sitter_adapter import TreeSitterProfiledParser
from codebase_graph.ingest.tree_sitter_parser import assemble_profiled_parser_registry


def test_parser_registry_registers_language_profile() -> None:
    profile = resolve_language_profile("rust")
    assert profile is not None
    registry = ParserRegistry()

    registry.register_language_profile(profile)

    parser = registry.parser_for_language("rust")
    assert isinstance(parser, TreeSitterProfiledParser)
    assert registry.language_for_path(type("P", (), {"suffix": ".rs"})()) == "rust"


def test_assemble_profiled_parser_registry_skips_unavailable_grammars(monkeypatch) -> None:
    monkeypatch.setattr("codebase_graph.ingest.tree_sitter_parser.importlib.util.find_spec", lambda _name: None)

    registry = assemble_profiled_parser_registry()

    assert registry.language_for_path(type("P", (), {"suffix": ".py"})()) == "python"
    assert registry.language_for_path(type("P", (), {"suffix": ".rs"})()) is None


def test_default_parser_registry_includes_supported_languages() -> None:
    registry = default_parser_registry()

    assert registry.language_for_path(Path("main.rs")) == "rust"
    assert registry.language_for_path(Path("main.go")) == "go"
    assert registry.language_for_path(Path("lib.c")) == "c"
    assert registry.language_for_path(Path("lib.cpp")) == "cpp"
    assert registry.language_for_path(Path("lib.hpp")) == "cpp"
    assert registry.language_for_path(Path("solver.f90")) == "fortran"
