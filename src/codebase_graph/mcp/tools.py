from __future__ import annotations

import json
import re
from typing import Any

from codebase_graph.db import LadybugCodeGraphStore
from codebase_graph.ontology import QUERY_HELPERS, schema_payload
from codebase_graph.reasoning import CompactContextBuilder, architecture_query_catalog
from codebase_graph.retrieval import SearchRequest, SearchService

from .runtime import GraphRuntimeConfig, open_graph_store

READ_ONLY_DENY_RE = re.compile(
    r"\b(CREATE|DELETE|SET|MERGE|DROP|COPY|INSERT|LOAD|INSTALL|DETACH|REMOVE|ALTER|RENAME)\b",
    re.IGNORECASE,
)


class UnknownToolError(ValueError):
    pass


def handle_tool_call(name: str, arguments: dict[str, Any], *, runtime: GraphRuntimeConfig) -> dict[str, Any]:
    if name == "graph_health":
        return _health(runtime)
    if name == "graph_schema":
        return schema_payload()
    if name == "graph_query_helpers":
        return {"query_helpers": [helper.as_dict() for helper in QUERY_HELPERS]}
    if name == "graph_architecture_queries":
        return architecture_query_catalog(group=_optional_str(arguments.get("group")))
    if name == "graph_search":
        with open_graph_store(runtime) as store:
            request = _search_request(arguments)
            return SearchService(store).search(request).as_dict()
    if name == "graph_context":
        with open_graph_store(runtime) as store:
            return _context_payload(store, arguments)
    if name == "graph_query":
        with open_graph_store(runtime) as store:
            return _query_payload(store, arguments)
    raise UnknownToolError(f"Unknown codebaseGraph MCP tool: {name}")


def call_tool_result(name: str, arguments: dict[str, Any], *, runtime: GraphRuntimeConfig) -> dict[str, Any]:
    try:
        payload = handle_tool_call(name, arguments, runtime=runtime)
    except UnknownToolError:
        raise
    except Exception as exc:
        return tool_error_result(name, exc)
    return tool_result(payload)


def tool_result(payload: dict[str, Any]) -> dict[str, Any]:
    return {
        "content": [{"type": "text", "text": json.dumps(payload, indent=2, sort_keys=True)}],
        "structuredContent": payload,
        "isError": False,
    }


def tool_error_result(name: str, exc: Exception) -> dict[str, Any]:
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
    return [
        {
            "name": "graph_health",
            "description": "Check the configured codebaseGraph database path and manifest path.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
        },
        {
            "name": "graph_search",
            "description": "Search code, documentation, paths, and dependencies with compact graph context.",
            "inputSchema": _search_schema(required=("query",)),
        },
        {
            "name": "graph_context",
            "description": "Return compact context for a search query or explicit node_id/node_type pair.",
            "inputSchema": _search_schema(required=()),
        },
        {
            "name": "graph_schema",
            "description": "Return ontology schema, search indexes, context profiles, and query helper metadata.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
        },
        {
            "name": "graph_query_helpers",
            "description": "Return named read-only query helpers for common graph exploration tasks.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
        },
        {
            "name": "graph_architecture_queries",
            "description": "Return the grouped architecture-discovery Cypher catalog for coding-agent first-step orientation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "group": {
                        "type": "string",
                        "description": "Optional architecture query group to return.",
                    },
                },
                "additionalProperties": False,
            },
        },
        {
            "name": "graph_query",
            "description": "Execute a restricted read-only graph query against the configured database.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "statement": {"type": "string"},
                    "parameters": {"type": "object"},
                    "limit": {"type": "integer", "minimum": 1},
                },
                "required": ["statement"],
                "additionalProperties": False,
            },
        },
    ]


def _health(runtime: GraphRuntimeConfig) -> dict[str, Any]:
    return {
        "ok": runtime.db_path.exists(),
        "repo_root": runtime.repo_root.as_posix(),
        "database_path": runtime.db_path.as_posix(),
        "manifest_path": runtime.manifest_path.as_posix() if runtime.manifest_path else None,
    }


def _search_request(arguments: dict[str, Any]) -> SearchRequest:
    request = SearchRequest(
        query=str(arguments.get("query", "")),
        limit=int(arguments.get("limit", 3)),
        profile=str(arguments.get("profile", "brief")),
        budget=int(arguments.get("budget", 600)),
        max_depth=_optional_int(arguments.get("max_depth")),
    )
    request.validate()
    return request


def _context_payload(store: LadybugCodeGraphStore, arguments: dict[str, Any]) -> dict[str, Any]:
    node_id = str(arguments.get("node_id") or "")
    node_type = str(arguments.get("node_type") or "")
    if node_id and node_type:
        profile = str(arguments.get("profile", "brief"))
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
            "context": [node.as_dict() for node in context],
        }
    return SearchService(store).search(_search_request(arguments)).as_dict()


def _query_payload(store: LadybugCodeGraphStore, arguments: dict[str, Any]) -> dict[str, Any]:
    statement = str(arguments.get("statement") or arguments.get("query") or "").strip()
    if not statement:
        raise ValueError("graph_query requires a non-empty statement")
    _validate_read_only_statement(statement)
    parameters = arguments.get("parameters") or {}
    if not isinstance(parameters, dict):
        raise ValueError("graph_query parameters must be a JSON object")
    limit = int(arguments.get("limit", 100))
    rows = store.execute(statement, parameters).get_all()
    return {
        "statement": statement,
        "row_count": len(rows),
        "rows": [_row_values(row) for row in rows[:limit]],
        "truncated": len(rows) > limit,
    }


def _validate_read_only_statement(statement: str) -> None:
    stripped = statement.strip().rstrip(";")
    if ";" in stripped:
        raise ValueError("graph_query accepts one read-only statement at a time")
    match = READ_ONLY_DENY_RE.search(stripped)
    if match is not None:
        raise ValueError(f"graph_query is read-only; blocked keyword: {match.group(1).upper()}")


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


def _search_schema(*, required: tuple[str, ...]) -> dict[str, Any]:
    return {
        "type": "object",
        "properties": {
            "query": {"type": "string"},
            "limit": {"type": "integer", "minimum": 1},
            "profile": {"type": "string"},
            "budget": {"type": "integer", "minimum": 0},
            "max_depth": {"type": "integer", "minimum": 0},
            "node_id": {"type": "string"},
            "node_type": {"type": "string"},
        },
        "required": list(required),
        "additionalProperties": False,
    }


def _optional_int(value: Any) -> int | None:
    if value is None or value == "":
        return None
    return int(value)


def _optional_str(value: Any) -> str | None:
    if value is None or value == "":
        return None
    return str(value)
