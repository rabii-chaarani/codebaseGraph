from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

from codebase_graph.retrieval.block_format import (
    ONTOLOGY_TERMS,
    canonicalize_search_payload,
    parse_search_block,
    serialize_agent_search_block,
    serialize_context_block,
    serialize_graph_block,
    serialize_parseable_search_block,
    serialize_search_block,
)


FIXTURE_PATH = Path(__file__).parent / "fixtures" / "search_service_graph_search.json"
SCRIPT_PATH = Path(__file__).parents[1] / "scripts" / "compare_graph_output_tokens.py"


class _WhitespaceEncoding:
    def encode(self, text: str) -> list[str]:
        return text.split()


def test_token_counting_uses_encoded_text_length() -> None:
    module = _load_benchmark_script()

    assert module.count_tokens("Class SearchService Method", _WhitespaceEncoding()) == 3


def test_raw_vs_block_comparison_preserves_search_service_fixture() -> None:
    payload = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
    block = serialize_parseable_search_block(payload)

    assert parse_search_block(block) == canonicalize_search_payload(payload)
    assert serialize_search_block(payload) == block


def test_l_same_is_only_emitted_for_matching_context_spans() -> None:
    payload = {
        "query": "SearchService",
        "profile": "brief",
        "limit": 1,
        "budget": 600,
        "results": [
            {
                "id": "Class:1",
                "type": "Class",
                "label": "SearchService",
                "path": "src/codebase_graph/retrieval/search.py",
                "span": {"line_start": 10, "line_end": 20},
                "rank_score": 1.0,
                "context": [
                    {
                        "direction": "outgoing",
                        "relation": "Contains",
                        "type": "Scope",
                        "label": "SearchService scope",
                        "path": "src/codebase_graph/retrieval/search.py",
                        "span": {"line_start": 10, "line_end": 20},
                        "summary": "Scope for SearchService",
                    },
                    {
                        "direction": "outgoing",
                        "relation": "Contains",
                        "type": "Method",
                        "label": "__init__",
                        "path": "src/codebase_graph/retrieval/search.py",
                        "span": {"line_start": 11, "line_end": 12},
                    },
                ],
            }
        ],
    }

    block = serialize_search_block(payload)

    assert block.count("span=L=same") == 1
    assert "Method label=__init__ span=L11-L12" in block


def test_non_boilerplate_context_summaries_are_preserved() -> None:
    payload = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
    block = serialize_search_block(payload)

    assert 'summary="Stores the graph backend for later search calls."' in block
    assert parse_search_block(block) == canonicalize_search_payload(payload)


def test_parseable_block_preserves_evidence_chain_and_snippet_fields() -> None:
    payload = {
        "query": "A",
        "profile": "callgraph",
        "limit": 1,
        "budget": 600,
        "results": [
            {
                "id": "Function:A",
                "type": "Function",
                "label": "A",
                "path": "sample.py",
                "span": {"line_start": 1, "line_end": 3},
                "rank_score": 1.0,
                "context": [
                    {
                        "direction": "outgoing",
                        "relation": "Calls",
                        "type": "Function",
                        "label": "B",
                        "path": "sample.py",
                        "span": {"line_start": 5, "line_end": 8},
                        "evidence_path": {
                            "chain": "Function A Calls Function B",
                            "edges": [
                                {
                                    "relation": "Calls",
                                    "direction": "outgoing",
                                    "source_node_id": "Function:A",
                                    "target_node_id": "Function:B",
                                    "edge_id": "Calls:A:B",
                                }
                            ],
                        },
                        "snippet": {
                            "path": "sample.py",
                            "span": {"line_start": 5, "line_end": 5},
                            "text": "def b():\n",
                            "redactions": ["token"],
                        },
                    }
                ],
            }
        ],
    }

    block = serialize_search_block(payload)

    assert 'chain="Function A Calls Function B"' in block
    assert 'snippet="def b():\\n"' in block
    assert parse_search_block(block) == canonicalize_search_payload(payload)


def test_parseable_block_round_trips_semantic_annotations() -> None:
    payload = _semantic_payload()

    block = serialize_search_block(payload)

    assert "semantic=semantic_resolution" in block
    assert "confidence=0.91" in block
    assert "provider=symbol_table" in block
    assert 'evidence="edge:semantic,evidence:helper"' in block
    assert parse_search_block(block) == canonicalize_search_payload(payload)


def test_parseable_block_round_trips_multiple_semantic_annotation_groups() -> None:
    payload = _semantic_payload()
    payload["results"][0]["context"][0]["semantic_annotations"].append(
        {
            "relation_kind": "semantic_call_target",
            "confidence": 0.8,
            "provider": "receiver",
            "evidence_ids": ["edge:call"],
            "diagnostics": ["receiver match"],
        }
    )

    block = serialize_search_block(payload)

    assert 'semantic="semantic_resolution,semantic_call_target"' in block
    assert 'evidence="edge:semantic,evidence:helper;edge:call"' in block
    assert parse_search_block(block) == canonicalize_search_payload(payload)


def test_agent_and_context_blocks_render_semantic_annotations_without_changing_row_shape() -> None:
    payload = _semantic_payload()

    agent_block = serialize_agent_search_block(payload)
    context_block = serialize_context_block(
        {
            "node_id": payload["results"][0]["id"],
            "node_type": payload["results"][0]["type"],
            "profile": payload["profile"],
            "context": payload["results"][0]["context"],
        }
    )

    assert "Function helper() ResolvesTo Function helper L5-L8 semantic=semantic_resolution" in agent_block
    assert "Function helper() ResolvesTo Function helper L5-L8 semantic=semantic_resolution" in context_block
    assert "confidence=0.91" in agent_block
    assert "provider=symbol_table" in context_block


def test_agent_block_keeps_semantic_type_annotation_context() -> None:
    payload = _semantic_payload()
    payload["results"][0]["context"] = [
        {
            "direction": "outgoing",
            "relation": "References",
            "type": "TypeAnnotation",
            "label": "User",
            "path": "sample.py",
            "span": {"line_start": 2, "line_end": 2},
            "semantic_annotations": [
                {
                    "relation_kind": "semantic_type_reference",
                    "confidence": 0.82,
                }
            ],
        }
    ]

    block = serialize_agent_search_block(payload)

    assert "TypeAnnotation User L2-L2 semantic=semantic_type_reference confidence=0.82" in block


def test_agent_block_uses_evidence_chain_as_primary_context_text() -> None:
    payload = _evidence_chain_payload()

    block = serialize_agent_search_block(payload)

    assert "Function A Calls Function B L5-L8" in block
    assert 'chain="Function A Calls Function B"' not in block
    assert "outgoing Calls Function B L5-L8" not in block


def test_context_block_uses_evidence_chain_as_primary_context_text() -> None:
    payload = _evidence_chain_payload()["results"][0]

    block = serialize_context_block(
        {
            "node_id": payload["id"],
            "node_type": payload["type"],
            "profile": "callgraph",
            "context": payload["context"],
        }
    )

    assert "Function A Calls Function B L5-L8" in block
    assert 'chain="Function A Calls Function B"' not in block
    assert "outgoing Calls Function B L5-L8" not in block


def test_graph_block_serializes_non_search_graph_payloads() -> None:
    assert serialize_graph_block(
        {
            "ok": True,
            "repo_root": ".",
            "database_path": ".codebaseGraph/graph.ldb",
            "manifest_path": ".codebaseGraph/manifest.json",
            "database_exists": True,
            "manifest_exists": True,
            "graph_readable": True,
            "total_nodes": 42,
        }
    ).startswith("health ok=true database_exists=true manifest_exists=true graph_readable=true total_nodes=42\n")

    schema_block = serialize_graph_block(
        {
            "ontology": "code_ontology_v1",
            "version": "1.0.0",
            "node_types": [{"name": "Class"}],
            "relation_types": [{"name": "Defines"}],
            "parser_node_mappings": [{"name": "python"}],
            "search_indexes": [{"name": "idx_code", "node_types": ["Class"], "fields": ["label"]}],
            "context_profiles": {"brief": {"relations": ["Defines"]}},
            "query_helpers": [{"name": "overview"}],
        }
    )
    assert schema_block.startswith("schema code_ontology_v1 version=1.0.0 nodes=1 relations=1")
    assert "node_types Class" in schema_block
    assert "index idx_code node_types=Class fields=label" in schema_block

    helpers_block = serialize_graph_block(
        {
            "query_helpers": [
                {
                    "name": "repository_overview",
                    "description": "Count graph nodes.",
                    "query": "MATCH (n) RETURN count(n) AS total",
                    "returns": ["total"],
                }
            ]
        }
    )
    assert "query_helpers count=1" in helpers_block
    assert "- repository_overview description=\"Count graph nodes.\"" in helpers_block
    assert "returns=total" in helpers_block

    architecture_block = serialize_graph_block(
        {
            "workflow": "coding_task_architecture_discovery",
            "execution_tool": "graph_query",
            "recommended_order": ["overview"],
            "groups": [
                {
                    "name": "overview",
                    "goal": "Check coverage.",
                    "queries": [
                        {
                            "name": "graph_coverage",
                            "description": "Count nodes.",
                            "statement": "MATCH (n) RETURN count(n) AS total_nodes",
                            "returns": ["total_nodes"],
                        }
                    ],
                }
            ],
        }
    )
    assert architecture_block.startswith("architecture_queries workflow=coding_task_architecture_discovery")
    assert "group overview goal=\"Check coverage.\"" in architecture_block
    assert "graph_coverage description=\"Count nodes.\"" in architecture_block

    query_block = serialize_graph_block(
        {
            "statement": "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1",
            "row_count": 1,
            "rows": [[42]],
            "truncated": False,
        }
    )
    assert "query rows=1 truncated=false" in query_block
    assert "columns total_nodes" in query_block
    assert "row 1 total_nodes=42" in query_block

    error_block = serialize_graph_block(
        {"error": {"tool": "graph_query", "type": "ValueError", "message": "read-only"}}
    )
    assert error_block == "error tool=graph_query type=ValueError message=read-only\n"


def test_block_format_keeps_ontology_terms_literal() -> None:
    payload = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
    block = serialize_search_block(payload)

    for term in ONTOLOGY_TERMS:
        assert term in block
    assert "rank_score=" in block
    assert "label=" in block
    assert "span=" in block
    assert "path " in block


def test_agent_block_reduces_display_only_boilerplate() -> None:
    payload = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
    block = serialize_agent_search_block(payload)

    assert "q SearchService\n" in block
    assert "budget" not in block
    assert "limit" not in block
    assert "profile" not in block
    assert "id=Class:943d6556d328f1c7ca67" in block
    assert "id=Method:3c775c9656a4d6b85843" in block
    assert "rank_score=1.35" in block
    assert "rank_score=1.351608" not in block
    assert "SearchService scope" not in block
    assert "search scope" not in block
    assert "outgoing Contains Method __init__" not in block
    assert "TypeAnnotation" not in block
    assert (
        'outgoing Contains InstanceAttribute self.store L119-L119 '
        'summary="Stores the graph backend for later search calls."'
    ) in block


def test_context_block_serializes_explicit_node_context() -> None:
    block = serialize_context_block(
        {
            "node_id": "Class:943d6556d328f1c7ca67",
            "node_type": "Class",
            "profile": "definitions",
            "context": [
                {
                    "direction": "outgoing",
                    "relation": "Contains",
                    "type": "Method",
                    "label": "search",
                    "path": "src/codebase_graph/retrieval/search.py",
                    "span": {"line_start": 123, "line_end": 149},
                }
            ],
        }
    )

    assert block.startswith("context Class id=Class:943d6556d328f1c7ca67 profile=definitions")
    assert "file path src/codebase_graph/retrieval/search.py" in block
    assert "outgoing Contains Method search L123-L149" in block


def _load_benchmark_script() -> Any:
    spec = importlib.util.spec_from_file_location("compare_graph_output_tokens", SCRIPT_PATH)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _evidence_chain_payload() -> dict[str, Any]:
    return {
        "query": "A",
        "profile": "callgraph",
        "limit": 1,
        "budget": 600,
        "results": [
            {
                "id": "Function:A",
                "type": "Function",
                "label": "A",
                "path": "sample.py",
                "span": {"line_start": 1, "line_end": 3},
                "rank_score": 1.0,
                "context": [
                    {
                        "direction": "outgoing",
                        "relation": "Calls",
                        "type": "Function",
                        "label": "B",
                        "path": "sample.py",
                        "span": {"line_start": 5, "line_end": 8},
                        "evidence_path": {
                            "chain": "Function A Calls Function B",
                            "edges": [
                                {
                                    "relation": "Calls",
                                    "direction": "outgoing",
                                    "source_node_id": "Function:A",
                                    "target_node_id": "Function:B",
                                    "edge_id": "Calls:A:B",
                                }
                            ],
                        },
                        "snippet": {
                            "path": "sample.py",
                            "span": {"line_start": 5, "line_end": 5},
                            "text": "def b():\n",
                            "redactions": ["token"],
                        },
                    }
                ],
            }
        ],
    }


def _semantic_payload() -> dict[str, Any]:
    return {
        "query": "helper",
        "profile": "callgraph",
        "limit": 1,
        "budget": 600,
        "results": [
            {
                "id": "CallExpression:helper",
                "type": "CallExpression",
                "label": "helper()",
                "path": "sample.py",
                "span": {"line_start": 1, "line_end": 1},
                "rank_score": 1.0,
                "context": [
                    {
                        "direction": "outgoing",
                        "relation": "ResolvesTo",
                        "type": "Function",
                        "label": "helper",
                        "path": "sample.py",
                        "span": {"line_start": 5, "line_end": 8},
                        "evidence_path": {
                            "chain": "Function helper() ResolvesTo Function helper",
                            "edges": [
                                {
                                    "relation": "ResolvesTo",
                                    "direction": "outgoing",
                                    "source_node_id": "CallExpression:helper",
                                    "target_node_id": "Function:helper",
                                    "edge_id": "edge:semantic",
                                    "kind": "semantic_resolution",
                                    "confidence": 0.91,
                                    "metadata": {
                                        "resolution_source": "symbol_table",
                                        "evidence_id": "evidence:helper",
                                    },
                                }
                            ],
                        },
                        "semantic_annotations": [
                            {
                                "relation_kind": "semantic_resolution",
                                "confidence": 0.91,
                                "provider": "symbol_table",
                                "evidence_ids": ["edge:semantic", "evidence:helper"],
                                "metadata": {
                                    "resolution_source": "symbol_table",
                                    "evidence_id": "evidence:helper",
                                },
                            }
                        ],
                    }
                ],
            }
        ],
    }
