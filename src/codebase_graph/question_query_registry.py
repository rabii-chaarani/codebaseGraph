from __future__ import annotations

import argparse
import json
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any, Mapping

PHASE_ARCHITECTURE_UNDERSTANDING = "architecture_understanding"
PHASE_CHANGE_PREPARATION = "change_preparation"
PHASE_BREAKING_CHANGE_PREPARATION = "breaking_change_preparation"

@dataclass(frozen=True, slots=True)
class EngineeringQuestionQuery:
    id: str
    question: str
    intent: str
    phase: str
    query: str
    required_params: tuple[str, ...] = ()
    result_shape: tuple[str, ...] = ()
    tags: tuple[str, ...] = ()

    def validate_params(self, params: Mapping[str, Any]) -> None:
        missing = [name for name in self.required_params if not params.get(name)]
        if missing:
            raise ValueError(f"Missing required params for {self.id}: {', '.join(missing)}")

    def run(self, core: Any, **params: Any) -> dict[str, Any]:
        self.validate_params(params)
        return core.cypher(self.query, parameters=dict(params))

_QUERIES: tuple[EngineeringQuestionQuery, ...] = (
    EngineeringQuestionQuery(
        id="se.architecture.entrypoints.v1",
        question="What are the main CLI or package entry points?",
        intent="Map runtime ingress points before architecture reasoning.",
        phase=PHASE_ARCHITECTURE_UNDERSTANDING,
        query="MATCH (n:EntryPoint) RETURN n.id, n.label, n.kind, n.path, n.qualified_name LIMIT 50",
        result_shape=("id", "label", "kind", "path", "qualified_name"),
        tags=("architecture", "entrypoints", "runtime"),
    ),
    EngineeringQuestionQuery(
        id="se.change.tests_for_artifact.v1",
        question="What test artifacts exist for this path or symbol?",
        intent="Identify existing tests for target path or symbol before edits.",
        phase=PHASE_CHANGE_PREPARATION,
        query="MATCH (n:Test) RETURN n.id, n.label, n.kind, n.path, n.qualified_name LIMIT 50",
        required_params=("path", "symbol"),
        result_shape=("id", "label", "kind", "path", "qualified_name"),
        tags=("change", "tests", "coverage"),
    ),
    EngineeringQuestionQuery(
        id="se.breaking.consumers_of_contract.v1",
        question="Who are the consumers of the old behavior?",
        intent="Find direct consumers of a contract before introducing breaking changes.",
        phase=PHASE_BREAKING_CHANGE_PREPARATION,
        query="MATCH (n:Call) RETURN n.id, n.label, n.kind, n.path, n.qualified_name LIMIT 50",
        required_params=("contract_id",),
        result_shape=("id", "label", "kind", "path", "qualified_name"),
        tags=("breaking-change", "consumers", "contract"),
    ),
)

def list_engineering_question_queries(phase: str | None = None) -> list[EngineeringQuestionQuery]:
    return [query for query in _QUERIES if phase is None or query.phase == phase]

def get_engineering_question_query(query_id: str) -> EngineeringQuestionQuery:
    for query in _QUERIES:
        if query.id == query_id:
            return query
    raise KeyError(query_id)

def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="List or run versioned engineering question queries.")
    parser.add_argument("--source-root", default=".")
    parser.add_argument("--state-dir", default=None)
    parser.add_argument("--db-path", default=None)
    parser.add_argument("--staging-dir", default=None)
    subparsers = parser.add_subparsers(dest="command", required=True)
    list_parser = subparsers.add_parser("list")
    list_parser.add_argument("--phase")
    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("query_id")
    run_parser.add_argument("--params-json", default="{}")
    run_parser.add_argument("--no-refresh", action="store_true")
    args = parser.parse_args(argv)
    if args.command == "list":
        queries = list_engineering_question_queries(phase=args.phase)
        payload = {"count": len(queries), "items": [_query_as_dict(query) for query in queries]}
    elif args.command == "run":
        from .graph_core import CodebaseGraph

        params = json.loads(args.params_json)
        if not isinstance(params, dict):
            raise ValueError("--params-json must decode to an object")
        core = CodebaseGraph(
            source_root=args.source_root,
            state_dir=args.state_dir,
            database_path=args.db_path,
            staging_dir=args.staging_dir,
        )
        if not args.no_refresh:
            core.ensure_current()
        question = get_engineering_question_query(args.query_id)
        payload = {"question": _query_as_dict(question), "result": question.run(core, **params)}
    else:
        parser.error(f"unsupported command: {args.command}")
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0

def _query_as_dict(query: EngineeringQuestionQuery) -> dict[str, Any]:
    return {
        "id": query.id,
        "question": query.question,
        "intent": query.intent,
        "phase": query.phase,
        "required_params": list(query.required_params),
        "result_shape": list(query.result_shape),
        "tags": list(query.tags),
    }

if __name__ == "__main__":
    raise SystemExit(main())
