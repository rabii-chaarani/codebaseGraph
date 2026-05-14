from __future__ import annotations

import json

import pytest

import codebase_graph.graph_core as graph_core
from codebase_graph.question_query_registry import (
    PHASE_ARCHITECTURE_UNDERSTANDING,
    PHASE_BREAKING_CHANGE_PREPARATION,
    get_engineering_question_query,
    list_engineering_question_queries,
    main,
)

class _StubCore:
    def cypher(self, query, parameters=None):
        return {"query": query, "parameters": parameters or {}}

def test_registry_lists_queries_and_filters_by_phase() -> None:
    all_queries = list_engineering_question_queries()
    assert len(all_queries) >= 3
    architecture_queries = list_engineering_question_queries(phase=PHASE_ARCHITECTURE_UNDERSTANDING)
    assert architecture_queries
    assert all(query.phase == PHASE_ARCHITECTURE_UNDERSTANDING for query in architecture_queries)

def test_get_query_by_id_and_run() -> None:
    query = get_engineering_question_query("se.breaking.consumers_of_contract.v1")
    assert query.phase == PHASE_BREAKING_CHANGE_PREPARATION
    response = query.run(_StubCore(), contract_id="contract:example")
    assert response["parameters"]["contract_id"] == "contract:example"

def test_required_params_are_validated() -> None:
    query = get_engineering_question_query("se.change.tests_for_artifact.v1")
    with pytest.raises(ValueError):
        query.run(_StubCore(), path="src/package", symbol="")

def test_cli_lists_queries_by_phase(capsys) -> None:
    exit_code = main(["list", "--phase", PHASE_ARCHITECTURE_UNDERSTANDING])
    assert exit_code == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["count"] >= 1
    assert all(item["phase"] == PHASE_ARCHITECTURE_UNDERSTANDING for item in payload["items"])

def test_cli_runs_query_with_params(monkeypatch, capsys) -> None:
    class _FakeGraphCore:
        def __init__(self, **kwargs):
            self.kwargs = kwargs

        def ensure_current(self):
            return {"ok": True}

        def cypher(self, query, parameters=None):
            return {"query": query, "parameters": parameters or {}}

    monkeypatch.setattr(graph_core, "CodebaseGraph", _FakeGraphCore)
    exit_code = main(["run", "se.breaking.consumers_of_contract.v1", "--params-json", '{"contract_id": "api:contract"}'])
    assert exit_code == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["question"]["id"] == "se.breaking.consumers_of_contract.v1"
    assert payload["result"]["parameters"] == {"contract_id": "api:contract"}
