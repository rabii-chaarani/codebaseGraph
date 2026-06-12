from __future__ import annotations

import pytest

from codebase_graph.ontology import (
    SemanticConstructMapping,
    decide_ontology_extension,
    evolve_cross_language_ontology,
    map_semantic_construct,
    validate_canonical_mapping,
)


def test_map_semantic_construct_reuses_language_neutral_nodes() -> None:
    mapping = map_semantic_construct("rust", "trait")

    assert mapping.canonical_node_type == "Class"
    assert mapping.kind == "trait"


def test_validate_canonical_mapping_rejects_unknown_targets() -> None:
    mapping = SemanticConstructMapping("rust", "unknown", "Trait", "trait")

    with pytest.raises(ValueError, match="Unknown ontology node type"):
        validate_canonical_mapping(mapping)


def test_decide_ontology_extension_prefers_reuse_when_construct_is_mapped() -> None:
    decision = decide_ontology_extension("macro", mapped_node_type="Symbol")

    assert decision.action == "reuse existing node"
    assert not decision.migration_required


def test_evolve_cross_language_ontology_records_decisions_for_known_and_unknown_constructs() -> None:
    decisions = evolve_cross_language_ontology((("cpp", "namespace_definition"), ("zig", "comptime"),))

    assert decisions[0].action == "reuse existing node"
    assert decisions[0].construct == "namespace_definition"
    assert decisions[1].action == "defer schema change"
    assert decisions[1].construct == "comptime"
