from __future__ import annotations

import re

import pytest

from codebase_graph.reasoning import (
    ARCHITECTURE_QUERY_GROUPS,
    ARCHITECTURE_QUERY_ORDER,
    architecture_query_catalog,
)


def test_architecture_query_catalog_serializes_in_stable_workflow_order() -> None:
    payload = architecture_query_catalog()

    assert payload["workflow"] == "coding_task_architecture_discovery"
    assert payload["execution_tool"] == "graph_query"
    assert payload["recommended_order"] == list(ARCHITECTURE_QUERY_ORDER)
    assert [group["name"] for group in payload["groups"]] == list(ARCHITECTURE_QUERY_ORDER)


def test_architecture_query_catalog_groups_expected_queries() -> None:
    assert set(ARCHITECTURE_QUERY_GROUPS) == set(ARCHITECTURE_QUERY_ORDER)
    assert _query_names("overview") == {
        "graph_coverage",
        "source_unit_inventory",
        "package_directory_shape",
    }
    assert _query_names("public_surface") == {
        "public_surface_candidates",
        "entrypoint_runtime_surface",
    }
    assert _query_names("dependency_topology") == {
        "external_dependency_map",
        "module_import_coupling",
    }
    assert _query_names("execution_flow") == {
        "high_fan_in_definitions",
        "high_fan_out_callers",
        "callable_neighborhood",
    }
    assert _query_names("runtime_data_security") == {
        "data_query_touchpoints",
        "secret_configuration_touchpoints",
    }
    assert _query_names("documentation_context") == {
        "documentation_to_code_links",
        "evidence_for_symbol",
    }
    assert _query_names("graph_quality_gaps") == {"unresolved_reference_risk"}


def test_architecture_query_names_are_unique_and_count_matches_catalog_contract() -> None:
    names = [
        query.name
        for group_name in ARCHITECTURE_QUERY_ORDER
        for query in ARCHITECTURE_QUERY_GROUPS[group_name].queries
    ]

    assert len(names) == 15
    assert len(names) == len(set(names))


def test_architecture_query_catalog_filters_by_group() -> None:
    payload = architecture_query_catalog("execution_flow")

    assert payload["recommended_order"] == list(ARCHITECTURE_QUERY_ORDER)
    assert [group["name"] for group in payload["groups"]] == ["execution_flow"]


def test_architecture_query_catalog_rejects_unknown_group() -> None:
    with pytest.raises(ValueError, match="Valid groups: overview"):
        architecture_query_catalog("missing")


def test_architecture_queries_are_read_only_and_use_edge_node_traversal() -> None:
    forbidden = re.compile(
        r"\b(CREATE|MERGE|DELETE|SET|DROP|LOAD|COPY|INSERT|ALTER|REMOVE|RENAME|DETACH|INSTALL)\b",
        re.IGNORECASE,
    )
    direct_relation = re.compile(r"-\[:(?!FROM_|TO_)([A-Za-z][A-Za-z0-9_]*)\]->")

    for group_name in ARCHITECTURE_QUERY_ORDER:
        for query in ARCHITECTURE_QUERY_GROUPS[group_name].queries:
            assert query.statement.lstrip().upper().startswith("MATCH "), query.name
            assert ";" not in query.statement, query.name
            assert not forbidden.search(query.statement), query.name
            assert not direct_relation.search(query.statement), query.name


def _query_names(group_name: str) -> set[str]:
    return {query.name for query in ARCHITECTURE_QUERY_GROUPS[group_name].queries}
