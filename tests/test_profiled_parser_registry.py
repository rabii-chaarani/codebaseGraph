from __future__ import annotations

from codebase_graph.ingest import ParserRegistry, resolve_language_profile
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
