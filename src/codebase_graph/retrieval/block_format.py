from __future__ import annotations

import json
import re
import shlex
from typing import Any, Mapping


SIMPLE_VALUE_RE = re.compile(r"^[A-Za-z0-9_./:\-\[\]]+$")
SPAN_RE = re.compile(r"^L(?P<start>\d+)-L(?P<end>\d+)$")
ONTOLOGY_TERMS = {"Class", "Method", "Scope", "Contains", "outgoing", "path", "span", "id", "label", "rank_score"}


def serialize_parseable_search_block(payload: Mapping[str, Any]) -> str:
    """Serialize parseable search block for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.

    Returns:
        Formatted text returned to the caller.
    """
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
            _append_context_extras(context_parts, context)
            lines.append(" ".join(context_parts))
        previous_line_was_file = False
    return "\n".join(lines) + "\n"


def serialize_agent_search_block(payload: Mapping[str, Any]) -> str:
    """Serialize agent search block for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.

    Returns:
        Formatted text returned to the caller.
    """
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
        if "id" in result:
            result_parts.append(f"id={_format_value(str(result['id']))}")
        summary = _meaningful_summary(result)
        if summary:
            result_parts.append(f"summary={_format_value(summary)}")
        lines.append(" ".join(result_parts))

        for context in result.get("context", []):
            if _omit_agent_context(context, parent_span=result_span, result_keys=result_keys):
                continue
            context_path = str(context.get("path", ""))
            context_span = _span(context.get("span", {}))
            context_parts = _agent_context_parts(context, context_span)
            if context_path and context_path != current_path:
                context_parts.append(f"path={_format_value(context_path)}")
            context_summary = _meaningful_summary(context)
            if context_summary:
                context_parts.append(f"summary={_format_value(context_summary)}")
            _append_context_extras(context_parts, context, include_chain=False)
            lines.append(" ".join(context_parts))
    return "\n".join(lines) + "\n"


def serialize_context_block(payload: Mapping[str, Any]) -> str:
    """Serialize context block for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.

    Returns:
        Formatted text returned to the caller.
    """
    header = [
        f"context {payload.get('node_type', '')}",
        f"id={_format_value(str(payload.get('node_id', '')))}",
        f"profile={_format_value(str(payload.get('profile', '')))}",
    ]
    lines = [" ".join(header)]
    current_path: str | None = None
    for context in payload.get("context", []):
        context_path = str(context.get("path", ""))
        if context_path != current_path:
            if len(lines) > 1:
                lines.append("")
            lines.append(f"file path {_format_value(context_path)}")
            current_path = context_path
        context_parts = _agent_context_parts(context, _span(context.get("span", {})))
        context_summary = _meaningful_summary(context)
        if context_summary:
            context_parts.append(f"summary={_format_value(context_summary)}")
        _append_context_extras(context_parts, context, include_chain=False)
        lines.append(" ".join(context_parts))
    return "\n".join(lines) + "\n"


def serialize_graph_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph block for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.

    Returns:
        Formatted text returned to the caller.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    if "error" in payload:
        return serialize_error_block(payload)
    if "results" in payload:
        return serialize_agent_search_block(payload)
    if "context" in payload and "node_id" in payload and "node_type" in payload:
        return serialize_context_block(payload)
    if {"statement", "row_count", "rows"} <= set(payload):
        return serialize_query_block(payload)
    if "ok" in payload and "database_path" in payload:
        return serialize_health_block(payload)
    if "node_types" in payload and "relation_types" in payload:
        return serialize_schema_block(payload)
    if "query_helpers" in payload:
        return serialize_query_helpers_block(payload)
    if "workflow" in payload and "groups" in payload:
        return serialize_architecture_queries_block(payload)
    return serialize_mapping_block("graph", payload)


def serialize_health_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph-health payload for agent-facing block output."""
    lines = [
        " ".join(
            [
                "health",
                f"ok={_format_scalar(payload.get('ok'))}",
                f"database_exists={_format_scalar(payload.get('database_exists'))}",
                f"manifest_exists={_format_scalar(payload.get('manifest_exists'))}",
            ]
        )
    ]
    if "graph_readable" in payload:
        lines[0] += f" graph_readable={_format_scalar(payload.get('graph_readable'))}"
    if "total_nodes" in payload:
        lines[0] += f" total_nodes={_format_scalar(payload.get('total_nodes'))}"
    for key in ("repo_root", "database_path", "manifest_path"):
        if payload.get(key) is not None:
            lines.append(f"{key} {_format_value(str(payload[key]))}")
    if isinstance(payload.get("error"), Mapping):
        error = payload["error"]
        lines.append(f"error type={_format_value(str(error.get('type', '')))} message={_format_value(str(error.get('message', '')))}")
    return "\n".join(lines) + "\n"


def serialize_schema_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph-schema payload for agent-facing block output."""
    node_types = list(payload.get("node_types", [])) if isinstance(payload.get("node_types"), list) else []
    relation_types = list(payload.get("relation_types", [])) if isinstance(payload.get("relation_types"), list) else []
    parser_mappings = list(payload.get("parser_node_mappings", [])) if isinstance(payload.get("parser_node_mappings"), list) else []
    search_indexes = list(payload.get("search_indexes", [])) if isinstance(payload.get("search_indexes"), list) else []
    context_profiles = payload.get("context_profiles", {})
    query_helpers = list(payload.get("query_helpers", [])) if isinstance(payload.get("query_helpers"), list) else []
    lines = [
        " ".join(
            [
                "schema",
                _format_value(str(payload.get("ontology", ""))),
                f"version={_format_value(str(payload.get('version', '')))}",
                f"nodes={len(node_types)}",
                f"relations={len(relation_types)}",
                f"parser_mappings={len(parser_mappings)}",
                f"indexes={len(search_indexes)}",
                f"profiles={len(context_profiles) if isinstance(context_profiles, Mapping) else 0}",
                f"helpers={len(query_helpers)}",
            ]
        )
    ]
    if node_types:
        lines.append("node_types " + _format_csv(_record_names(node_types)))
    if relation_types:
        lines.append("relation_types " + _format_csv(_record_names(relation_types)))
    for index in search_indexes:
        if isinstance(index, Mapping):
            lines.append(
                "index "
                + " ".join(
                    [
                        _format_value(str(index.get("name", ""))),
                        f"node_types={_format_csv(index.get('node_types', []))}",
                        f"fields={_format_csv(index.get('fields', []))}",
                    ]
                )
            )
    if isinstance(context_profiles, Mapping):
        for name, profile in context_profiles.items():
            relation_text = ""
            if isinstance(profile, Mapping) and profile.get("relations"):
                relation_text = f" relations={_format_csv(profile.get('relations', []))}"
            lines.append(f"profile {_format_value(str(name))}{relation_text}")
    return "\n".join(lines) + "\n"


def serialize_query_helpers_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph-query-helpers payload for agent-facing block output."""
    helpers = list(payload.get("query_helpers", [])) if isinstance(payload.get("query_helpers"), list) else []
    lines = [f"query_helpers count={len(helpers)}"]
    for helper in helpers:
        if not isinstance(helper, Mapping):
            continue
        lines.extend(_query_spec_lines(helper))
    return "\n".join(lines) + "\n"


def serialize_architecture_queries_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph-architecture-queries payload for agent-facing block output."""
    groups = list(payload.get("groups", [])) if isinstance(payload.get("groups"), list) else []
    lines = [
        " ".join(
            [
                "architecture_queries",
                f"workflow={_format_value(str(payload.get('workflow', '')))}",
                f"execution_tool={_format_value(str(payload.get('execution_tool', '')))}",
                f"groups={len(groups)}",
            ]
        )
    ]
    if payload.get("recommended_order"):
        lines.append(f"recommended_order {_format_csv(payload.get('recommended_order', []))}")
    for group in groups:
        if not isinstance(group, Mapping):
            continue
        lines.append(
            f"group {_format_value(str(group.get('name', '')))} goal={_format_value(str(group.get('goal', '')))}"
        )
        queries = group.get("queries", [])
        if not isinstance(queries, list):
            continue
        for query in queries:
            if isinstance(query, Mapping):
                lines.extend(_query_spec_lines(query, indent="  "))
    return "\n".join(lines) + "\n"


def serialize_query_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph-query payload for agent-facing block output."""
    rows = list(payload.get("rows", [])) if isinstance(payload.get("rows"), list) else []
    columns = _query_columns(str(payload.get("statement", "")))
    lines = [
        " ".join(
            [
                "query",
                f"rows={payload.get('row_count', len(rows))}",
                f"truncated={_format_scalar(payload.get('truncated', False))}",
            ]
        ),
        f"statement {_format_value(str(payload.get('statement', '')))}",
    ]
    if columns:
        lines.append("columns " + _format_csv(columns))
    for index, row in enumerate(rows, start=1):
        values = row if isinstance(row, list) else [row]
        if columns and len(columns) == len(values):
            row_text = " ".join(f"{column}={_format_scalar(value)}" for column, value in zip(columns, values, strict=True))
        else:
            row_text = " ".join(_format_scalar(value) for value in values)
        lines.append(f"row {index} {row_text}".rstrip())
    return "\n".join(lines) + "\n"


def serialize_error_block(payload: Mapping[str, Any]) -> str:
    """Serialize graph tool errors for agent-facing block output."""
    error = payload.get("error")
    if not isinstance(error, Mapping):
        return "error\n"
    return (
        "error "
        f"tool={_format_value(str(error.get('tool', '')))} "
        f"type={_format_value(str(error.get('type', '')))} "
        f"message={_format_value(str(error.get('message', '')))}\n"
    )


def serialize_mapping_block(name: str, payload: Mapping[str, Any]) -> str:
    """Serialize an arbitrary graph payload as stable key/value block output."""
    parts = [name]
    for key, value in payload.items():
        parts.append(f"{key}={_format_scalar(value)}")
    return " ".join(parts) + "\n"


def serialize_search_block(payload: Mapping[str, Any]) -> str:
    """Serialize search block for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.

    Returns:
        Formatted text returned to the caller.
    """
    return serialize_parseable_search_block(payload)


def canonicalize_search_payload(payload: Mapping[str, Any]) -> dict[str, Any]:
    """Canonicalize search payload for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.

    Returns:
        Structured mapping that follows the search, ranking, and block-format retrieval
        response contract.
    """
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
            evidence_path = _compact_evidence_path(context)
            if evidence_path:
                context_record["evidence_path"] = evidence_path
            snippet = _compact_snippet(context)
            if snippet:
                context_record["snippet"] = snippet
            result_record["context"].append(context_record)
        records.append(result_record)
    return {"results": records}


def parse_search_block(text: str) -> dict[str, Any]:
    """Parse the compact search block format back into structured result records.

    Args:
        text: Text being parsed, formatted, or written.

    Returns:
        Structured mapping that follows the search, ranking, and block-format retrieval
        response contract.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
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
            # L=same is a compact marker for context rows that share the result span.
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
            if fields.get("chain"):
                context_record["evidence_path"] = {"chain": fields["chain"]}
            if fields.get("snippet"):
                context_record["snippet"] = {
                    "path": fields.get("snippet_path", fields.get("path", current_path)),
                    "span": _parse_span(fields.get("snippet_span", "")),
                    "text": fields["snippet"].replace("\\n", "\n"),
                }
                if fields.get("redactions"):
                    context_record["snippet"]["redactions"] = [
                        item for item in fields["redactions"].split(",") if item
                    ]
            current_result["context"].append(context_record)
            continue
        raise ValueError(f"Unknown block line: {raw_line}")
    return {"results": records}


def intentional_summary_omissions(payload: Mapping[str, Any]) -> list[str]:
    """Report summary omissions for search, ranking, and block-format retrieval.

    Args:
        payload: Structured payload being normalized or serialized.

    Returns:
        Ordered results returned to the search, ranking, and block-format retrieval caller.
    """
    omissions: list[str] = []
    for result_index, result in enumerate(payload.get("results", [])):
        if _is_boilerplate_summary(result):
            omissions.append(f"results[{result_index}].summary")
        for context_index, context in enumerate(result.get("context", [])):
            if _is_boilerplate_summary(context):
                omissions.append(f"results[{result_index}].context[{context_index}].summary")
    return omissions


def _keyed_fields(tokens: list[str]) -> dict[str, str]:
    """Manage fields within search, ranking, and block-format retrieval.

    Args:
        tokens: Tokens used by the search, ranking, and block-format retrieval
        workflow.

    Returns:
        Structured mapping that follows the search, ranking, and block-format retrieval
        response contract.
    """
    fields: dict[str, str] = {}
    for token in tokens:
        if "=" not in token:
            continue
        key, value = token.split("=", 1)
        fields[key] = value
    return fields


def _format_value(value: str) -> str:
    """Format value for search, ranking, and block-format retrieval.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Formatted text returned to the caller.
    """
    if value and SIMPLE_VALUE_RE.match(value):
        return value
    return json.dumps(value, ensure_ascii=True)


def _format_scalar(value: Any) -> str:
    """Format scalar or structured values for agent-facing graph blocks."""
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, str):
        return _format_value(value)
    return _format_value(json.dumps(value, separators=(",", ":"), sort_keys=True))


def _format_csv(values: Any) -> str:
    """Format a compact comma-separated value list."""
    if not isinstance(values, list | tuple | set):
        return _format_value(str(values))
    return _format_value(",".join(str(value) for value in values))


def _record_names(records: list[Any]) -> list[str]:
    """Return names from record dictionaries."""
    names: list[str] = []
    for record in records:
        if isinstance(record, Mapping) and record.get("name"):
            names.append(str(record["name"]))
    return names


def _query_spec_lines(spec: Mapping[str, Any], *, indent: str = "- ") -> list[str]:
    """Serialize a named query/helper spec."""
    lines = [
        (
            f"{indent}{_format_value(str(spec.get('name', '')))} "
            f"description={_format_value(str(spec.get('description', '')))}"
        )
    ]
    if spec.get("parameters"):
        lines.append(f"{indent}  parameters={_format_csv(spec.get('parameters', []))}")
    if spec.get("returns"):
        lines.append(f"{indent}  returns={_format_csv(spec.get('returns', []))}")
    if spec.get("query"):
        lines.append(f"{indent}  statement={_format_value(str(spec.get('query', '')))}")
    elif spec.get("statement"):
        lines.append(f"{indent}  statement={_format_value(str(spec.get('statement', '')))}")
    return lines


def _query_columns(statement: str) -> list[str]:
    """Best-effort column labels from a read-only graph query statement."""
    match = re.search(r"\bRETURN\b(?P<returns>.*?)(?:\bORDER\s+BY\b|\bLIMIT\b|$)", statement, flags=re.IGNORECASE | re.DOTALL)
    if match is None:
        return []
    columns: list[str] = []
    for expression in _split_return_expressions(match.group("returns")):
        alias_match = re.search(r"\bAS\s+([A-Za-z_][A-Za-z0-9_]*)\s*$", expression, flags=re.IGNORECASE)
        if alias_match is not None:
            columns.append(alias_match.group(1))
            continue
        label = expression.rsplit(".", 1)[-1].strip()
        if re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", label):
            columns.append(label)
    return columns


def _split_return_expressions(text: str) -> list[str]:
    """Split a RETURN expression list on top-level commas."""
    expressions: list[str] = []
    current: list[str] = []
    depth = 0
    quote = ""
    for character in text:
        if quote:
            current.append(character)
            if character == quote:
                quote = ""
            continue
        if character in {"'", '"'}:
            quote = character
            current.append(character)
            continue
        if character in "([{":
            depth += 1
        elif character in ")]}" and depth:
            depth -= 1
        if character == "," and depth == 0:
            expressions.append("".join(current).strip())
            current = []
            continue
        current.append(character)
    if current:
        expressions.append("".join(current).strip())
    return [expression for expression in expressions if expression]


def _agent_context_parts(context: Mapping[str, Any], span: Mapping[str, int]) -> list[str]:
    """Return context row fields for agent-facing block output."""
    evidence_path = _compact_evidence_path(context)
    if evidence_path:
        return [f"  {str(evidence_path['chain'])}", _format_span(span)]
    return [
        f"  {context.get('direction', '')}",
        str(context.get("relation", "")),
        str(context.get("type", "")),
        _format_value(str(context.get("label", ""))),
        _format_span(span),
    ]


def _append_context_extras(parts: list[str], context: Mapping[str, Any], *, include_chain: bool = True) -> None:
    """Append additive context details to a block-format context row."""
    evidence_path = _compact_evidence_path(context)
    if include_chain and evidence_path:
        parts.append(f"chain={_format_value(str(evidence_path['chain']))}")
    snippet = _compact_snippet(context)
    if snippet:
        parts.append(f"snippet_path={_format_value(str(snippet.get('path', '')))}")
        parts.append(f"snippet_span={_format_span(_span(snippet.get('span', {})))}")
        parts.append(f"snippet={_format_value(str(snippet.get('text', '')))}")
        redactions = snippet.get("redactions", [])
        if redactions:
            parts.append(f"redactions={_format_value(','.join(str(item) for item in redactions))}")


def _compact_evidence_path(record: Mapping[str, Any]) -> dict[str, str]:
    """Return the parseable subset of evidence-path output."""
    evidence_path = record.get("evidence_path")
    if not isinstance(evidence_path, Mapping):
        return {}
    chain = str(evidence_path.get("chain", ""))
    return {"chain": chain} if chain else {}


def _compact_snippet(record: Mapping[str, Any]) -> dict[str, Any]:
    """Return the parseable subset of optional source-snippet output."""
    snippet = record.get("snippet")
    if not isinstance(snippet, Mapping):
        return {}
    text = str(snippet.get("text", ""))
    if not text:
        return {}
    return {
        "path": str(snippet.get("path", "")),
        "span": _span(snippet.get("span", {})),
        "text": text,
        "redactions": list(snippet.get("redactions", [])) if isinstance(snippet.get("redactions"), list) else [],
    }


def _parse_value(value: str) -> str:
    """Parse value for search, ranking, and block-format retrieval.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Formatted text returned to the caller.
    """
    if value.startswith('"'):
        return str(json.loads(value))
    return value


def _span(value: Any) -> dict[str, int]:
    """Manage search, ranking, and block-format retrieval within search, ranking, and block-format retrieval.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Structured mapping that follows the search, ranking, and block-format retrieval
        response contract.
    """
    if not isinstance(value, Mapping):
        return {}
    span: dict[str, int] = {}
    if value.get("line_start") is not None:
        span["line_start"] = int(value["line_start"])
    if value.get("line_end") is not None:
        span["line_end"] = int(value["line_end"])
    return span


def _format_span(span: Mapping[str, int]) -> str:
    """Format span for search, ranking, and block-format retrieval.

    Args:
        span: Line-span mapping from graph output.

    Returns:
        Formatted text returned to the caller.
    """
    start = span.get("line_start")
    end = span.get("line_end")
    if start is None or end is None:
        return "L?"
    return f"L{start}-L{end}"


def _parse_span(value: str) -> dict[str, int]:
    """Parse span for search, ranking, and block-format retrieval.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Structured mapping that follows the search, ranking, and block-format retrieval
        response contract.
    """
    match = SPAN_RE.match(value)
    if not match:
        return {}
    return {"line_start": int(match.group("start")), "line_end": int(match.group("end"))}


def _parse_number(value: str | None) -> int | float | None:
    """Parse number for search, ranking, and block-format retrieval.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        int | float | None instance populated with data from the search, ranking, and block-
        format retrieval workflow.
    """
    if value is None:
        return None
    try:
        as_float = float(value)
    except ValueError:
        return None
    return int(as_float) if as_float.is_integer() else as_float


def _meaningful_summary(record: Mapping[str, Any]) -> str:
    """Manage summary within search, ranking, and block-format retrieval.

    Args:
        record: Serialized search or context record.

    Returns:
        Formatted text returned to the caller.
    """
    summary = str(record.get("summary", ""))
    return "" if _is_boilerplate_summary(record) else summary


def _is_boilerplate_summary(record: Mapping[str, Any]) -> bool:
    """Return whether boilerplate summary for search, ranking, and block-format retrieval.

    Args:
        record: Serialized search or context record.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
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
    """Decide whether to omit agent context for search, ranking, and block-format retrieval.

    Args:
        context: Context record attached to a search result.
        parent_span: Source span inherited from the parent search result.
        result_keys: Fields already emitted for the parent result.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    context_span = _span(context.get("span", {}))
    if _is_boilerplate_summary(context) and context_span == dict(parent_span):
        return True
    if _record_key(context) in result_keys:
        return True
    return context.get("type") == "TypeAnnotation"


def _record_key(record: Mapping[str, Any]) -> tuple[str, str, str, tuple[tuple[str, int], ...]]:
    """Build key for search, ranking, and block-format retrieval.

    Args:
        record: Serialized search or context record.

    Returns:
        Tuple of stable results returned to the search, ranking, and block-format retrieval
        caller.
    """
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
    "serialize_architecture_queries_block",
    "serialize_context_block",
    "serialize_error_block",
    "serialize_agent_search_block",
    "serialize_graph_block",
    "serialize_health_block",
    "serialize_query_block",
    "serialize_query_helpers_block",
    "serialize_schema_block",
    "serialize_parseable_search_block",
    "serialize_search_block",
]
