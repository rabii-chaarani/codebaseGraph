#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import statistics
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
SRC_ROOT = REPO_ROOT / "src"
if SRC_ROOT.as_posix() not in sys.path:
    sys.path.insert(0, SRC_ROOT.as_posix())

from codebase_graph.mcp.runtime import runtime_config  # noqa: E402
from codebase_graph.mcp.tools import handle_tool_call  # noqa: E402
from codebase_graph.retrieval.block_format import (  # noqa: E402
    canonicalize_search_payload,
    intentional_summary_omissions,
    parse_search_block,
    serialize_search_block,
)


DEFAULT_FIXTURE = REPO_ROOT / "tests" / "fixtures" / "search_service_graph_search.json"
DEFAULT_OUTPUT = REPO_ROOT / "docs" / "graph_output_token_comparison.md"


@dataclass(frozen=True, slots=True)
class Tokenizer:
    encoding: Any
    encoding_name: str
    model_name: str | None
    fallback_note: str = ""


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    tokenizer = resolve_tokenizer(model=args.model, encoding_name=args.encoding)
    samples = _load_samples(args)
    rows = [_compare_sample(sample, tokenizer) for sample in samples]
    aggregate = _aggregate(rows)
    _write_report(args.output, rows, aggregate, tokenizer)
    _print_summary(rows, aggregate, tokenizer, args.output)
    return 0


def resolve_tokenizer(*, model: str | None = None, encoding_name: str | None = None) -> Tokenizer:
    try:
        import tiktoken
    except ImportError as exc:
        raise RuntimeError(
            "tiktoken is required for graph output token benchmarking. Install it in the active environment "
            "or run this script where tiktoken is available."
        ) from exc

    if encoding_name:
        encoding = tiktoken.get_encoding(encoding_name)
        return Tokenizer(encoding=encoding, encoding_name=encoding.name, model_name=model)
    if model:
        try:
            encoding = tiktoken.encoding_for_model(model)
            return Tokenizer(encoding=encoding, encoding_name=encoding.name, model_name=model)
        except KeyError:
            encoding = tiktoken.get_encoding("o200k_base")
            return Tokenizer(
                encoding=encoding,
                encoding_name=encoding.name,
                model_name=model,
                fallback_note=f"model-specific encoding unavailable for {model}; defaulted to o200k_base",
            )
    encoding = tiktoken.get_encoding("o200k_base")
    return Tokenizer(encoding=encoding, encoding_name=encoding.name, model_name=None)


def count_tokens(text: str, encoding: Any) -> int:
    return len(encoding.encode(text))


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Compare graph-search JSON output with readable block output.")
    parser.add_argument("--queries", action="append", default=[], help="Graph-search query to run; repeat as needed")
    parser.add_argument("--fixture", action="append", type=Path, default=[], help="Path to a graph-search JSON fixture")
    parser.add_argument("--model", default=None, help="Model name used to resolve a tiktoken encoding")
    parser.add_argument("--encoding", default=None, help="Explicit tiktoken encoding name")
    parser.add_argument("--limit", type=int, default=3, help="Graph-search result limit for live queries")
    parser.add_argument("--profile", default="brief", help="Graph-search context profile for live queries")
    parser.add_argument("--budget", type=int, default=600, help="Graph-search context budget for live queries")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT, help="Markdown report path")
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT, help="Repository root for live graph-search queries")
    parser.add_argument("--config", type=Path, default=None, help="Optional codebaseGraph setup config path")
    parser.add_argument("--db", type=Path, default=None, help="Optional codebaseGraph database path")
    parser.add_argument("--manifest", type=Path, default=None, help="Optional codebaseGraph manifest path")
    parser.add_argument("--context-limit", type=int, default=2, help="Context items per result for live queries")
    parser.add_argument("--detail", choices=("standard", "slim"), default="slim", help="Raw graph-search detail level")
    return parser


def _load_samples(args: argparse.Namespace) -> list[dict[str, Any]]:
    samples: list[dict[str, Any]] = []
    fixture_paths = args.fixture or ([] if args.queries else [DEFAULT_FIXTURE])
    for fixture_path in fixture_paths:
        payload = json.loads(fixture_path.read_text(encoding="utf-8"))
        if isinstance(payload, list):
            samples.extend(_fixture_sample(item, fixture_path) for item in payload)
        else:
            samples.append(_fixture_sample(payload, fixture_path))
    if args.queries:
        runtime = runtime_config(
            repo_root=args.repo_root,
            config_path=args.config,
            db_path=args.db,
            manifest_path=args.manifest,
        )
        for query in args.queries:
            payload = handle_tool_call(
                "graph_search",
                {
                    "query": query,
                    "limit": args.limit,
                    "profile": args.profile,
                    "budget": args.budget,
                    "context_limit": args.context_limit,
                    "detail": args.detail,
                },
                runtime=runtime,
            )
            samples.append({"name": query, "payload": payload, "source": "live graph-search"})
    if not samples:
        raise ValueError("No samples found. Provide --queries or --fixture.")
    return samples


def _fixture_sample(payload: dict[str, Any], fixture_path: Path) -> dict[str, Any]:
    if "payload" in payload and isinstance(payload["payload"], dict):
        name = str(payload.get("name") or payload["payload"].get("query") or fixture_path.stem)
        return {"name": name, "payload": payload["payload"], "source": fixture_path.as_posix()}
    return {"name": str(payload.get("query") or fixture_path.stem), "payload": payload, "source": fixture_path.as_posix()}


def _compare_sample(sample: dict[str, Any], tokenizer: Tokenizer) -> dict[str, Any]:
    payload = sample["payload"]
    raw_text = _raw_json(payload)
    block_text = serialize_search_block(payload)
    raw_canonical = canonicalize_search_payload(payload)
    block_canonical = parse_search_block(block_text)
    if raw_canonical != block_canonical:
        raise AssertionError(
            f"Block output is not semantically equivalent for {sample['name']}:\n"
            f"raw={json.dumps(raw_canonical, sort_keys=True)}\n"
            f"block={json.dumps(block_canonical, sort_keys=True)}"
        )
    raw_tokens = count_tokens(raw_text, tokenizer.encoding)
    block_tokens = count_tokens(block_text, tokenizer.encoding)
    raw_chars = len(raw_text)
    block_chars = len(block_text)
    token_delta = raw_tokens - block_tokens
    char_delta = raw_chars - block_chars
    result_count = len(payload.get("results", []))
    context_edges = sum(len(result.get("context", [])) for result in payload.get("results", []))
    return {
        "query": sample["name"],
        "source": sample["source"],
        "raw_chars": raw_chars,
        "block_chars": block_chars,
        "raw_tokens": raw_tokens,
        "block_tokens": block_tokens,
        "token_delta": token_delta,
        "token_reduction_pct": _pct(token_delta, raw_tokens),
        "char_reduction_pct": _pct(char_delta, raw_chars),
        "results": result_count,
        "context_edges": context_edges,
        "tokenizer": tokenizer.encoding_name,
        "model": tokenizer.model_name or "",
        "intentional_omissions": intentional_summary_omissions(payload),
    }


def _aggregate(rows: list[dict[str, Any]]) -> dict[str, Any]:
    raw_tokens = [row["raw_tokens"] for row in rows]
    block_tokens = [row["block_tokens"] for row in rows]
    reductions = [row["token_reduction_pct"] for row in rows]
    total_raw = sum(raw_tokens)
    total_block = sum(block_tokens)
    sorted_by_reduction = sorted(rows, key=lambda row: row["token_reduction_pct"])
    aggregate = {
        "sample_count": len(rows),
        "total_raw_tokens": total_raw,
        "total_block_tokens": total_block,
        "total_token_delta": total_raw - total_block,
        "overall_token_reduction_pct": _pct(total_raw - total_block, total_raw),
        "mean_token_reduction_pct": statistics.fmean(reductions) if reductions else 0.0,
        "median_token_reduction_pct": statistics.median(reductions) if reductions else 0.0,
        "min_reduction_case": sorted_by_reduction[0]["query"] if sorted_by_reduction else "",
        "min_reduction_pct": sorted_by_reduction[0]["token_reduction_pct"] if sorted_by_reduction else 0.0,
        "max_reduction_case": sorted_by_reduction[-1]["query"] if sorted_by_reduction else "",
        "max_reduction_pct": sorted_by_reduction[-1]["token_reduction_pct"] if sorted_by_reduction else 0.0,
        "p90_raw_tokens": None,
        "p90_block_tokens": None,
    }
    if len(rows) >= 10:
        aggregate["p90_raw_tokens"] = _p90(raw_tokens)
        aggregate["p90_block_tokens"] = _p90(block_tokens)
    return aggregate


def _write_report(path: Path, rows: list[dict[str, Any]], aggregate: dict[str, Any], tokenizer: Tokenizer) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    table_rows = "\n".join(
        "| {query} | {results} | {context_edges} | {raw_tokens:,} | {block_tokens:,} | {token_delta:,} | "
        "{token_reduction_pct:.1f}% |".format(**row)
        for row in rows
    )
    omission_lines = sorted({omission for row in rows for omission in row["intentional_omissions"]})
    omissions = "\n".join(f"- `{item}`" for item in omission_lines) or "- None"
    fallback = f"\n- {tokenizer.fallback_note}" if tokenizer.fallback_note else ""
    p90 = (
        f"- p90 raw/block tokens: {aggregate['p90_raw_tokens']:,} / {aggregate['p90_block_tokens']:,}\n"
        if aggregate["p90_raw_tokens"] is not None
        else "- p90 raw/block tokens: not reported because fewer than 10 samples were compared\n"
    )
    path.write_text(
        "\n".join(
            [
                "# Graph Search Output Token Comparison",
                "",
                "## Method",
                "- Raw format: compact JSON emitted by the current graph-search payload serializer, counted from the exact serialized JSON text with sorted keys and compact separators.",
                "- Ontology-preserving block format: grouped `file path` blocks with readable `Class`, `Method`, `Scope`, relation, `label`, `span`, `id`, and `rank_score` terms left literal.",
                f"- Tokenizer/model: encoding `{tokenizer.encoding_name}`"
                + (f" resolved from model `{tokenizer.model_name}`." if tokenizer.model_name else ".")
                + fallback,
                "- Count method: payload-only tokens using `len(encoding.encode(text))`; chat-message wrapper tokens were not included.",
                "",
                "## Results",
                "| Query | Results | Context edges | Raw tokens | Block tokens | Saved tokens | Reduction % |",
                "|---|---:|---:|---:|---:|---:|---:|",
                table_rows,
                "",
                "## Aggregate Summary",
                f"- Samples: {aggregate['sample_count']}",
                f"- Total raw tokens: {aggregate['total_raw_tokens']:,}",
                f"- Total block tokens: {aggregate['total_block_tokens']:,}",
                f"- Total saved tokens: {aggregate['total_token_delta']:,}",
                f"- Overall reduction: {aggregate['overall_token_reduction_pct']:.1f}%",
                f"- Mean reduction: {aggregate['mean_token_reduction_pct']:.1f}%",
                f"- Median reduction: {aggregate['median_token_reduction_pct']:.1f}%",
                f"- Min reduction: {aggregate['min_reduction_case']} ({aggregate['min_reduction_pct']:.1f}%)",
                f"- Max reduction: {aggregate['max_reduction_case']} ({aggregate['max_reduction_pct']:.1f}%)",
                p90.rstrip(),
                "",
                "## Ontology Preservation",
                "The validator normalizes raw JSON and block output into canonical result records preserving `type`, `label`, `path`, `span`, `id`, `rank_score`, and ordered context records with `direction`, `relation`, `type`, `label`, `path`, `span`, and non-boilerplate `summary`.",
                "",
                "Intentional omissions:",
                omissions,
                "",
                "Known limitations: the block parser validates the supported graph-search fixture shape and live graph-search output shape; it is not a general-purpose parser for hand-written variants.",
                "",
                "## Recommendation",
                "Use the ontology-preserving block format by default for agent-facing graph-search output when consumers need readable context. Keep JSON available for machine APIs and tests that require strict structured payloads.",
                "",
            ]
        ),
        encoding="utf-8",
    )


def _print_summary(rows: list[dict[str, Any]], aggregate: dict[str, Any], tokenizer: Tokenizer, output_path: Path) -> None:
    print(f"Compared {len(rows)} graph-search outputs using {tokenizer.encoding_name}.")
    print(f"Raw:   {aggregate['total_raw_tokens']:,} tokens")
    print(f"Block: {aggregate['total_block_tokens']:,} tokens")
    print(f"Saved: {aggregate['total_token_delta']:,} tokens")
    print(f"Reduction: {aggregate['overall_token_reduction_pct']:.1f}%")
    print(f"Report written to {output_path.as_posix()}")


def _raw_json(payload: dict[str, Any]) -> str:
    return json.dumps(payload, separators=(",", ":"), sort_keys=True)


def _pct(delta: int | float, original: int | float) -> float:
    return (float(delta) / float(original) * 100.0) if original else 0.0


def _p90(values: list[int]) -> int:
    ordered = sorted(values)
    index = int(0.9 * (len(ordered) - 1))
    return ordered[index]


if __name__ == "__main__":
    raise SystemExit(main())
