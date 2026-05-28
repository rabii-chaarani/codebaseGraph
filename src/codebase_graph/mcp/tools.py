from __future__ import annotations

import json
import re
from typing import Any

from codebase_graph.db import LadybugCodeGraphStore
from codebase_graph.diagnostics import log_event
from codebase_graph.ontology import QUERY_HELPERS, schema_payload
from codebase_graph.reasoning import CompactContextBuilder, architecture_query_catalog
from codebase_graph.retrieval import DETAIL_LEVELS, SearchRequest, SearchService, serialize_graph_block

from .graph_commands import MAX_GRAPH_QUERY_LIMIT, graph_tool_specs
from .runtime import GraphRuntimeConfig, open_graph_store

READ_ONLY_DENY_RE = re.compile(
    r"\b("
    r"ALTER|ATTACH|CALL|COPY|CREATE|DELETE|DETACH|DROP|EXPORT|IMPORT|INSERT|INSTALL|LOAD|MERGE|REMOVE|RENAME|SET|"
    r"TRUNCATE|UPDATE|USE"
    r")\b",
    re.IGNORECASE,
)


class UnknownToolError(ValueError):
    pass


def handle_tool_call(name: str, arguments: dict[str, Any], *, runtime: GraphRuntimeConfig | None) -> dict[str, Any]:
    if name == "graph_health":
        return _health(runtime)
    if name == "graph_schema":
        return schema_payload()
    if name == "graph_query_helpers":
        return {"query_helpers": [helper.as_dict() for helper in QUERY_HELPERS]}
    if name == "graph_architecture_queries":
        return architecture_query_catalog(group=_optional_str(arguments.get("group")))
    if name == "graph_search":
        with open_graph_store(_require_runtime(runtime, name)) as store:
            request = _search_request(arguments)
            return SearchService(store).search(request).as_dict(detail=request.detail)
    if name == "graph_context":
        with open_graph_store(_require_runtime(runtime, name)) as store:
            return _context_payload(store, arguments)
    if name == "graph_query":
        with open_graph_store(_require_runtime(runtime, name)) as store:
            return _query_payload(store, arguments)
    raise UnknownToolError(f"Unknown codebaseGraph MCP tool: {name}")


def call_tool_result(name: str, arguments: dict[str, Any], *, runtime: GraphRuntimeConfig) -> dict[str, Any]:
    try:
        payload = handle_tool_call(name, arguments, runtime=runtime)
        return tool_result(name, payload, arguments)
    except UnknownToolError:
        raise
    except Exception as exc:
        return tool_error_result(name, exc)


def _require_runtime(runtime: GraphRuntimeConfig | None, tool_name: str) -> GraphRuntimeConfig:
    if runtime is None:
        raise ValueError(f"{tool_name} requires a graph runtime")
    return runtime


def tool_result(name: str, payload: dict[str, Any], arguments: dict[str, Any] | None = None) -> dict[str, Any]:
    text = json.dumps(payload, separators=(",", ":"), sort_keys=True)
    if name in {"graph_search", "graph_context"} and _output_format(arguments or {}) == "block":
        text = serialize_graph_block(payload)
    return {
        "content": [{"type": "text", "text": text}],
        "structuredContent": payload,
        "isError": False,
    }


def tool_error_result(name: str, exc: Exception) -> dict[str, Any]:
    log_event(
        "mcp.tool_error",
        level="WARNING",
        tool=name,
        error_type=exc.__class__.__name__,
        message=str(exc),
    )
    payload = {
        "error": {
            "tool": name,
            "type": exc.__class__.__name__,
            "message": str(exc),
        }
    }
    return {
        "content": [{"type": "text", "text": f"{name} failed: {exc}"}],
        "structuredContent": payload,
        "isError": True,
    }


def tool_specs() -> list[dict[str, Any]]:
    return graph_tool_specs()


def _health(runtime: GraphRuntimeConfig) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "ok": False,
        "repo_root": runtime.repo_root.as_posix(),
        "database_path": runtime.db_path.as_posix(),
        "manifest_path": runtime.manifest_path.as_posix() if runtime.manifest_path else None,
        "database_exists": runtime.db_path.exists(),
        "manifest_exists": runtime.manifest_path.exists() if runtime.manifest_path else None,
    }
    if not runtime.db_path.exists():
        return payload
    try:
        with open_graph_store(runtime) as store:
            rows = store.execute("MATCH (n) RETURN count(n) AS total_nodes LIMIT 1").get_n(1)
    except Exception as exc:
        payload["graph_readable"] = False
        payload["error"] = {"type": exc.__class__.__name__, "message": str(exc)}
        log_event(
            "mcp.graph_health_failed",
            level="WARNING",
            database_path=runtime.db_path.as_posix(),
            error_type=exc.__class__.__name__,
            message=str(exc),
        )
        return payload
    payload["ok"] = True
    payload["graph_readable"] = True
    payload["total_nodes"] = _json_safe(rows[0][0]) if rows and rows[0] else 0
    return payload


def _search_request(arguments: dict[str, Any]) -> SearchRequest:
    request = SearchRequest(
        query=str(arguments.get("query", "")),
        limit=int(arguments.get("limit", 3)),
        profile=str(arguments.get("profile", "brief")),
        budget=int(arguments.get("budget", 600)),
        max_depth=_optional_int(arguments.get("max_depth")),
        context_limit=int(arguments.get("context_limit", 3)),
        detail=_detail(arguments),
    )
    request.validate()
    return request


def _context_payload(store: LadybugCodeGraphStore, arguments: dict[str, Any]) -> dict[str, Any]:
    node_id = str(arguments.get("node_id") or "")
    node_type = str(arguments.get("node_type") or "")
    if node_id and node_type:
        profile = str(arguments.get("profile", "brief"))
        detail = _detail(arguments)
        context = CompactContextBuilder(store).build(
            node_id,
            node_type,
            profile=profile,
            limit=int(arguments.get("limit", 3)),
            budget=int(arguments.get("budget", 600)),
            max_depth=_optional_int(arguments.get("max_depth")),
        )
        return {
            "node_id": node_id,
            "node_type": node_type,
            "profile": profile,
            "context": [node.as_dict(detail=detail) for node in context],
        }
    request = _search_request(arguments)
    return SearchService(store).search(request).as_dict(detail=request.detail)


def _query_payload(store: LadybugCodeGraphStore, arguments: dict[str, Any]) -> dict[str, Any]:
    statement = str(arguments.get("statement") or arguments.get("query") or "").strip()
    if not statement:
        raise ValueError("graph_query requires a non-empty statement")
    _validate_read_only_statement(statement)
    parameters = arguments.get("parameters") or {}
    if not isinstance(parameters, dict):
        raise ValueError("graph_query parameters must be a JSON object")
    limit = _graph_query_limit(arguments)
    result = store.execute(statement, parameters)
    try:
        rows = result.get_n(limit + 1)
    finally:
        close = getattr(result, "close", None)
        if callable(close):
            close()
    visible_rows = rows[:limit]
    return {
        "statement": statement,
        "row_count": len(visible_rows),
        "rows": [_row_values(row) for row in visible_rows],
        "truncated": len(rows) > limit,
    }


def _validate_read_only_statement(statement: str) -> None:
    stripped = statement.strip().rstrip(";")
    if ";" in stripped:
        raise ValueError("graph_query accepts one read-only statement at a time")
    match = READ_ONLY_DENY_RE.search(stripped)
    if match is not None:
        raise ValueError(f"graph_query is read-only; blocked keyword: {match.group(1).upper()}")


def _graph_query_limit(arguments: dict[str, Any]) -> int:
    limit = int(arguments.get("limit", 100))
    if limit <= 0:
        raise ValueError("graph_query limit must be greater than zero")
    if limit > MAX_GRAPH_QUERY_LIMIT:
        raise ValueError(f"graph_query limit must be {MAX_GRAPH_QUERY_LIMIT} or less")
    return limit


def _row_values(row: Any) -> list[Any]:
    try:
        return [_json_safe(value) for value in row]
    except TypeError:
        return [_json_safe(row)]


def _json_safe(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, (list, tuple)):
        return [_json_safe(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _json_safe(item) for key, item in value.items()}
    return str(value)


def _optional_int(value: Any) -> int | None:
    if value is None or value == "":
        return None
    return int(value)


def _optional_str(value: Any) -> str | None:
    if value is None or value == "":
        return None
    return str(value)


def _detail(arguments: dict[str, Any]) -> str:
    detail = str(arguments.get("detail", "standard"))
    if detail not in DETAIL_LEVELS:
        valid = ", ".join(sorted(DETAIL_LEVELS))
        raise ValueError(f"Unknown detail level: {detail}. Valid levels: {valid}")
    return detail


def _output_format(arguments: dict[str, Any]) -> str:
    output_format = str(arguments.get("output_format", "json"))
    if output_format not in {"json", "block"}:
        raise ValueError(f"Unknown output format: {output_format}. Valid formats: block, json")
    return output_format
