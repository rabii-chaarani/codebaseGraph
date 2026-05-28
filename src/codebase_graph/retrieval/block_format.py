from __future__ import annotations

import json
import re
import shlex
from typing import Any, Mapping


SIMPLE_VALUE_RE = re.compile(r"^[A-Za-z0-9_./:\-\[\]]+$")
SPAN_RE = re.compile(r"^L(?P<start>\d+)-L(?P<end>\d+)$")
ONTOLOGY_TERMS = {"Class", "Method", "Scope", "Contains", "outgoing", "path", "span", "id", "label", "rank_score"}


def serialize_search_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph-search JSON into a readable ontology-preserving block format."""
    lines = [
        " | ".join(
            [
                f"q {_format_value(str(payload.get('query', '')))}",
                f"budget {payload.get('budget', '')}",
                f"limit {payload.get('limit', '')}",
                f"profile {_format_value(str(payload.get('profile', '')))}",
            ]
        )
    ]
    current_path: str | None = None
    previous_line_was_file = False
    for result in payload.get("results", []):
        result_path = str(result.get("path", ""))
        if result_path != current_path:
            if len(lines) > 1 and not previous_line_was_file:
                lines.append("")
            lines.append(f"file path {_format_value(result_path)}")
            current_path = result_path
            previous_line_was_file = True
        else:
            previous_line_was_file = False

        result_span = _span(result.get("span", {}))
        result_parts = [
            f"- {result.get('type', '')}",
            f"label={_format_value(str(result.get('label', '')))}",
            f"span={_format_span(result_span)}",
        ]
        if "rank_score" in result:
            result_parts.append(f"rank_score={result['rank_score']}")
        if "id" in result:
            result_parts.append(f"id={_format_value(str(result['id']))}")
        summary = _meaningful_summary(result)
        if summary:
            result_parts.append(f"summary={_format_value(summary)}")
        lines.append(" ".join(result_parts))

        for context in result.get("context", []):
            context_path = str(context.get("path", ""))
            context_span = _span(context.get("span", {}))
            span_text = "L=same" if context_span == result_span else _format_span(context_span)
            context_parts = [
                f"  {context.get('direction', '')}",
                str(context.get("relation", "")),
                str(context.get("type", "")),
                f"label={_format_value(str(context.get('label', '')))}",
            ]
            if context_path and context_path != current_path:
                context_parts.append(f"path={_format_value(context_path)}")
            context_parts.append(f"span={span_text}")
            context_summary = _meaningful_summary(context)
            if context_summary:
                context_parts.append(f"summary={_format_value(context_summary)}")
            lines.append(" ".join(context_parts))
        previous_line_was_file = False
    return "\n".join(lines) + "\n"


def serialize_agent_search_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph-search JSON into a more aggressive display-only agent block."""
    lines = [f"q {_format_value(str(payload.get('query', '')))}"]
    current_path: str | None = None
    result_keys = {_record_key(result) for result in payload.get("results", [])}
    for result in payload.get("results", []):
        result_path = str(result.get("path", ""))
        if result_path != current_path:
            if len(lines) > 1:
                lines.append("")
            lines.append(f"file path {_format_value(result_path)}")
            current_path = result_path

        result_span = _span(result.get("span", {}))
        result_parts = [
            f"- {result.get('type', '')}",
            _format_value(str(result.get("label", ""))),
            _format_span(result_span),
        ]
        if "rank_score" in result:
            result_parts.append(f"rank_score={float(result['rank_score']):.2f}")
        summary = _meaningful_summary(result)
        if summary:
            result_parts.append(f"summary={_format_value(summary)}")
        lines.append(" ".join(result_parts))

        for context in result.get("context", []):
            if _omit_agent_context(context, parent_span=result_span, result_keys=result_keys):
                continue
            context_path = str(context.get("path", ""))
            context_span = _span(context.get("span", {}))
            context_parts = [
                f"  {context.get('direction', '')}",
                str(context.get("relation", "")),
                str(context.get("type", "")),
                _format_value(str(context.get("label", ""))),
                _format_span(context_span),
            ]
            if context_path and context_path != current_path:
                context_parts.append(f"path={_format_value(context_path)}")
            context_summary = _meaningful_summary(context)
            if context_summary:
                context_parts.append(f"summary={_format_value(context_summary)}")
            lines.append(" ".join(context_parts))
    return "\n".join(lines) + "\n"


def canonicalize_search_payload(payload: Mapping[str, Any]) -> dict[str, Any]:
    records: list[dict[str, Any]] = []
    for result in payload.get("results", []):
        result_record = {
            "type": result.get("type", ""),
            "label": result.get("label", ""),
            "path": result.get("path", ""),
            "span": _span(result.get("span", {})),
            "id": result.get("id", ""),
            "rank_score": result.get("rank_score"),
            "context": [],
        }
        result_summary = _meaningful_summary(result)
        if result_summary:
            result_record["summary"] = result_summary
        for context in result.get("context", []):
            context_record = {
                "direction": context.get("direction", ""),
                "relation": context.get("relation", ""),
                "type": context.get("type", ""),
                "label": context.get("label", ""),
                "path": context.get("path", ""),
                "span": _span(context.get("span", {})),
            }
            context_summary = _meaningful_summary(context)
            if context_summary:
                context_record["summary"] = context_summary
            result_record["context"].append(context_record)
        records.append(result_record)
    return {"results": records}


def parse_search_block(text: str) -> dict[str, Any]:
    records: list[dict[str, Any]] = []
    current_path = ""
    current_result: dict[str, Any] | None = None
    for raw_line in text.splitlines():
        if not raw_line.strip() or raw_line.startswith("q "):
            continue
        if raw_line.startswith("file path "):
            current_path = _parse_value(raw_line[len("file path ") :])
            current_result = None
            continue
        if raw_line.startswith("- "):
            tokens = shlex.split(raw_line)
            fields = _keyed_fields(tokens[2:])
            current_result = {
                "type": tokens[1],
                "label": fields.get("label", ""),
                "path": current_path,
                "span": _parse_span(fields.get("span", "")),
                "id": fields.get("id", ""),
                "rank_score": _parse_number(fields.get("rank_score")),
                "context": [],
            }
            if fields.get("summary"):
                current_result["summary"] = fields["summary"]
            records.append(current_result)
            continue
        if raw_line.startswith("  "):
            if current_result is None:
                raise ValueError(f"Context line has no parent result: {raw_line}")
            tokens = shlex.split(raw_line.strip())
            fields = _keyed_fields(tokens[3:])
            span = current_result["span"] if fields.get("span") == "L=same" else _parse_span(fields.get("span", ""))
            context_record = {
                "direction": tokens[0],
                "relation": tokens[1],
                "type": tokens[2],
                "label": fields.get("label", ""),
                "path": fields.get("path", current_path),
                "span": span,
            }
            if fields.get("summary"):
                context_record["summary"] = fields["summary"]
            current_result["context"].append(context_record)
            continue
        raise ValueError(f"Unknown block line: {raw_line}")
    return {"results": records}


def intentional_summary_omissions(payload: Mapping[str, Any]) -> list[str]:
    omissions: list[str] = []
    for result_index, result in enumerate(payload.get("results", [])):
        if _is_boilerplate_summary(result):
            omissions.append(f"results[{result_index}].summary")
        for context_index, context in enumerate(result.get("context", [])):
            if _is_boilerplate_summary(context):
                omissions.append(f"results[{result_index}].context[{context_index}].summary")
    return omissions


def _keyed_fields(tokens: list[str]) -> dict[str, str]:
    fields: dict[str, str] = {}
    for token in tokens:
        if "=" not in token:
            continue
        key, value = token.split("=", 1)
        fields[key] = value
    return fields


def _format_value(value: str) -> str:
    if value and SIMPLE_VALUE_RE.match(value):
        return value
    return json.dumps(value, ensure_ascii=True)


def _parse_value(value: str) -> str:
    if value.startswith('"'):
        return str(json.loads(value))
    return value


def _span(value: Any) -> dict[str, int]:
    if not isinstance(value, Mapping):
        return {}
    span: dict[str, int] = {}
    if value.get("line_start") is not None:
        span["line_start"] = int(value["line_start"])
    if value.get("line_end") is not None:
        span["line_end"] = int(value["line_end"])
    return span


def _format_span(span: Mapping[str, int]) -> str:
    start = span.get("line_start")
    end = span.get("line_end")
    if start is None or end is None:
        return "L?"
    return f"L{start}-L{end}"


def _parse_span(value: str) -> dict[str, int]:
    match = SPAN_RE.match(value)
    if not match:
        return {}
    return {"line_start": int(match.group("start")), "line_end": int(match.group("end"))}


def _parse_number(value: str | None) -> int | float | None:
    if value is None:
        return None
    try:
        as_float = float(value)
    except ValueError:
        return None
    return int(as_float) if as_float.is_integer() else as_float


def _meaningful_summary(record: Mapping[str, Any]) -> str:
    summary = str(record.get("summary", ""))
    return "" if _is_boilerplate_summary(record) else summary


def _is_boilerplate_summary(record: Mapping[str, Any]) -> bool:
    summary = str(record.get("summary", ""))
    label = str(record.get("label", ""))
    node_type = str(record.get("type", ""))
    if not summary or summary == label:
        return bool(summary)
    if node_type == "Scope" and label.endswith(" scope"):
        scoped_label = label[: -len(" scope")]
        return summary == f"Scope for {scoped_label}"
    return False


def _omit_agent_context(
    context: Mapping[str, Any],
    *,
    parent_span: Mapping[str, int],
    result_keys: set[tuple[str, str, str, tuple[tuple[str, int], ...]]],
) -> bool:
    context_span = _span(context.get("span", {}))
    if _is_boilerplate_summary(context) and context_span == dict(parent_span):
        return True
    if _record_key(context) in result_keys:
        return True
    return context.get("type") == "TypeAnnotation"


def _record_key(record: Mapping[str, Any]) -> tuple[str, str, str, tuple[tuple[str, int], ...]]:
    return (
        str(record.get("type", "")),
        str(record.get("label", "")),
        str(record.get("path", "")),
        tuple(sorted(_span(record.get("span", {})).items())),
    )


__all__ = [
    "ONTOLOGY_TERMS",
    "canonicalize_search_payload",
    "intentional_summary_omissions",
    "parse_search_block",
    "serialize_agent_search_block",
    "serialize_search_block",
]
