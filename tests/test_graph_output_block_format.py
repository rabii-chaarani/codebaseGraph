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


def test_agent_block_uses_evidence_chain_as_primary_context_text() -> None:
    payload = _evidence_chain_payload()

    block = serialize_agent_search_block(payload)

    assert 'chain="Function A Calls Function B" L5-L8' in block
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

    assert 'chain="Function A Calls Function B" L5-L8' in block
    assert "outgoing Calls Function B L5-L8" not in block


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
