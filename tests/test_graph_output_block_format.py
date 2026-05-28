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
    block = serialize_search_block(payload)

    assert parse_search_block(block) == canonicalize_search_payload(payload)


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


def _load_benchmark_script() -> Any:
    spec = importlib.util.spec_from_file_location("compare_graph_output_tokens", SCRIPT_PATH)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module
