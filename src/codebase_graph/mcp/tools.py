from __future__ import annotations

import json
import re
import subprocess
from typing import Any

from codebase_graph.db import LadybugCodeGraphStore
from codebase_graph.diagnostics import log_event
from codebase_graph.native_binary import resolve_native_product_binary
from codebase_graph.ontology import QUERY_HELPERS, schema_payload
from codebase_graph.reasoning import CompactContextBuilder, DEFAULT_CONTEXT_LIMIT, architecture_query_catalog
from codebase_graph.retrieval import DETAIL_LEVELS, SearchRequest, SearchService, serialize_graph_block

from .graph_commands import MAX_GRAPH_QUERY_LIMIT, graph_tool_specs
from .runtime import GraphRuntimeConfig

READ_ONLY_DENY_RE = re.compile(
    r"\b("
    r"ALTER|ATTACH|CALL|COPY|CREATE|DELETE|DETACH|DROP|EXPORT|IMPORT|INSERT|INSTALL|LOAD|MERGE|REMOVE|RENAME|SET|"
    r"TRUNCATE|UPDATE|USE"
    r")\b",
    re.IGNORECASE,
)

RUST_OWNED_TOOLS = {"graph_health", "graph_search", "graph_context", "graph_query"}


class UnknownToolError(ValueError):
    """Signal failures raised by the MCP server and transport surface subsystem."""
    pass


def handle_tool_call(name: str, arguments: dict[str, Any], *, runtime: GraphRuntimeConfig | None) -> dict[str, Any]:
    """Route a named MCP tool call to the matching graph operation.

    Args:
        name: Name used by the MCP server and transport surface workflow.
        arguments: Tool or command arguments supplied by the caller.
        runtime: Resolved runtime paths and graph database settings.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.

    Raises:
        UnknownToolError: Raised when validation or runtime preconditions fail.
    """
    if name in RUST_OWNED_TOOLS:
        resolved_runtime = _require_runtime(runtime, name)
        if name == "graph_query":
            _validate_native_graph_query_arguments(arguments)
        return _require_native_tool_payload(name, arguments, resolved_runtime)
    if name == "graph_schema":
        profiles = runtime.context_profiles if runtime is not None else None
        return schema_payload(context_profiles=profiles)
    if name == "graph_query_helpers":
        return {"query_helpers": [helper.as_dict() for helper in QUERY_HELPERS]}
    if name == "graph_architecture_queries":
        return architecture_query_catalog(group=_optional_str(arguments.get("group")))
    raise UnknownToolError(f"Unknown codebaseGraph MCP tool: {name}")


def call_tool_result(name: str, arguments: dict[str, Any], *, runtime: GraphRuntimeConfig) -> dict[str, Any]:
    """Dispatch tool result for MCP server and transport surface.

    Args:
        name: Name used by the MCP server and transport surface workflow.
        arguments: Tool or command arguments supplied by the caller.
        runtime: Resolved runtime paths and graph database settings.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.

    Raises:
        Exception: Raised when validation or runtime preconditions fail.
    """
    try:
        payload = handle_tool_call(name, arguments, runtime=runtime)
        return tool_result(name, payload, arguments)
    except UnknownToolError:
        raise
    except Exception as exc:
        return tool_error_result(name, exc, arguments)


def _require_runtime(runtime: GraphRuntimeConfig | None, tool_name: str) -> GraphRuntimeConfig:
    """Require runtime for MCP server and transport surface.

    This executes the selected workflow and returns a process status code or result object.

    Args:
        runtime: Resolved runtime paths and graph database settings.
        tool_name: Name used to select or label tool data.

    Returns:
        GraphRuntimeConfig instance populated with data from the MCP server and transport
        surface workflow.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    if runtime is None:
        raise ValueError(f"{tool_name} requires a graph runtime")
    return runtime


def tool_result(name: str, payload: dict[str, Any], arguments: dict[str, Any] | None = None) -> dict[str, Any]:
    """Build result for MCP server and transport surface.

    Args:
        name: Name used by the MCP server and transport surface workflow.
        payload: Structured payload being normalized or serialized.
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.
    """
    arguments = arguments or {}
    text = (
        serialize_graph_block(payload)
        if _output_format(arguments) == "block"
        else json.dumps(payload, separators=(",", ":"), sort_keys=True)
    )
    include_structured_content = _include_structured_content(arguments)
    result: dict[str, Any] = {
        "content": [{"type": "text", "text": text}],
        "isError": False,
    }
    if include_structured_content:
        result["structuredContent"] = payload
    return result


def tool_error_result(name: str, exc: Exception, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
    """Build error result for MCP server and transport surface.

    Args:
        name: Name used by the MCP server and transport surface workflow.
        exc: Exception being converted into an error response.
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.
    """
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
    arguments = arguments or {}
    text = (
        serialize_graph_block(payload)
        if _output_format(arguments) == "block"
        else json.dumps(payload, separators=(",", ":"), sort_keys=True)
    )
    result: dict[str, Any] = {
        "content": [{"type": "text", "text": text}],
        "isError": True,
    }
    if _include_structured_content(arguments):
        result["structuredContent"] = payload
    return result


def tool_specs() -> list[dict[str, Any]]:
    """Build specs for MCP server and transport surface.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.
    """
    return graph_tool_specs()


def _require_native_tool_payload(name: str, arguments: dict[str, Any], runtime: GraphRuntimeConfig) -> dict[str, Any]:
    """Return the Rust-owned MCP payload or fail without a Python DB fallback."""
    native_binary = _native_product_binary()
    if native_binary is None:
        raise RuntimeError(
            "Rust native CLI binary is required for codebaseGraph MCP DB tools; "
            "build or install `codebase-graph`, or set CODEBASE_GRAPH_NATIVE_CLI."
        )
    command = [native_binary, *_native_tool_command(name, arguments), *_native_runtime_args(runtime)]
    completed = subprocess.run(command, capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        details = (completed.stderr or completed.stdout).strip()
        suffix = f": {details}" if details else ""
        raise RuntimeError(f"Rust native MCP tool {name} failed with exit code {completed.returncode}{suffix}")
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"Rust native MCP tool {name} returned invalid JSON") from exc
    if not isinstance(payload, dict):
        raise RuntimeError(f"Rust native MCP tool {name} returned a non-object payload")
    return payload


def _native_tool_command(name: str, arguments: dict[str, Any]) -> list[str]:
    if name == "graph_health":
        return ["graph-health", "--json"]
    if name == "graph_search":
        command = ["graph-search", str(arguments.get("query") or ""), "--json"]
        _extend_native_search_args(command, arguments)
        return command
    if name == "graph_context":
        query = arguments.get("query")
        command = ["graph-context"]
        if query:
            command.append(str(query))
        node_id = arguments.get("node_id")
        node_type = arguments.get("node_type")
        if node_id is not None:
            command.extend(["--node-id", str(node_id)])
        if node_type is not None:
            command.extend(["--node-type", str(node_type)])
        command.append("--json")
        _extend_native_search_args(command, arguments)
        return command
    if name == "graph_query":
        statement = str(arguments.get("statement") or arguments.get("query") or "")
        command = [
            "graph-query",
            statement,
            "--parameters",
            json.dumps(arguments.get("parameters") or {}),
            "--limit",
            str(arguments.get("limit") or MAX_GRAPH_QUERY_LIMIT),
            "--json",
        ]
        return command
    raise UnknownToolError(f"Unknown codebaseGraph MCP tool: {name}")


def _validate_native_graph_query_arguments(arguments: dict[str, Any]) -> None:
    statement = str(arguments.get("statement") or arguments.get("query") or "").strip()
    if not statement:
        raise ValueError("graph_query requires a non-empty statement")
    _validate_read_only_statement(statement)
    parameters = arguments.get("parameters") or {}
    if not isinstance(parameters, dict):
        raise ValueError("graph_query parameters must be a JSON object")
    _graph_query_limit(arguments)


def _extend_native_search_args(command: list[str], arguments: dict[str, Any]) -> None:
    for key, option in (
        ("limit", "--limit"),
        ("profile", "--profile"),
        ("budget", "--budget"),
        ("context_limit", "--context-limit"),
        ("max_depth", "--max-depth"),
        ("detail", "--detail"),
        ("snippet_context_lines", "--snippet-context-lines"),
    ):
        value = arguments.get(key)
        if value is not None:
            command.extend([option, str(value)])
    if arguments.get("include_snippets"):
        command.append("--include-snippets")
    if arguments.get("include_semantic") is False:
        command.append("--no-semantic")
    if arguments.get("include_confidence") is False:
        command.append("--no-confidence")
    if arguments.get("include_evidence"):
        command.append("--include-evidence")


def _native_runtime_args(runtime: GraphRuntimeConfig) -> list[str]:
    command = [
        "--repo-root",
        runtime.repo_root.as_posix(),
        "--db",
        runtime.db_path.as_posix(),
    ]
    if runtime.manifest_path is not None:
        command.extend(["--manifest", runtime.manifest_path.as_posix()])
    return command


def _native_product_binary() -> str | None:
    return resolve_native_product_binary(skip_current_script=False)


def _search_request(arguments: dict[str, Any], *, profile_catalog: dict[str, Any] | None = None) -> SearchRequest:
    """Search request for MCP server and transport surface.

    Args:
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        SearchRequest instance populated with data from the MCP server and transport surface
        workflow.
    """
    request = SearchRequest(
        query=str(arguments.get("query", "")),
        limit=int(arguments.get("limit", 3)),
        profile=str(arguments.get("profile", "brief")),
        budget=int(arguments.get("budget", 600)),
        max_depth=_optional_int(arguments.get("max_depth")),
        context_limit=int(arguments.get("context_limit", 3)),
        detail=_detail(arguments),
        include_snippets=_bool(arguments.get("include_snippets", False)),
        snippet_context_lines=int(arguments.get("snippet_context_lines", 0)),
        include_semantic=_bool(arguments.get("include_semantic", True)),
        include_confidence=_bool(arguments.get("include_confidence", True)),
        include_evidence=_optional_bool(arguments.get("include_evidence")),
    )
    request.validate(profile_catalog)
    return request


def _context_payload(store: LadybugCodeGraphStore, arguments: dict[str, Any]) -> dict[str, Any]:
    """Manage payload within MCP server and transport surface.

    Args:
        store: Graph store used for persistence or read-only queries.
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.
    """
    node_id = str(arguments.get("node_id") or "")
    node_type = str(arguments.get("node_type") or "")
    if node_id and node_type:
        profile = str(arguments.get("profile", "brief"))
        detail = _detail(arguments)
        include_semantic = _bool(arguments.get("include_semantic", True))
        include_confidence = _bool(arguments.get("include_confidence", True))
        include_evidence = _optional_bool(arguments.get("include_evidence"))
        context = CompactContextBuilder(
            store,
            repo_root=arguments.get("_repo_root"),
            profile_catalog=arguments.get("_profile_catalog"),
        ).build(
            node_id,
            node_type,
            profile=profile,
            limit=_explicit_context_limit(arguments),
            budget=int(arguments.get("budget", 600)),
            max_depth=_optional_int(arguments.get("max_depth")),
            include_snippets=_bool(arguments.get("include_snippets", False)),
            snippet_context_lines=int(arguments.get("snippet_context_lines", 0)),
        )
        return {
            "node_id": node_id,
            "node_type": node_type,
            "profile": profile,
            "context": [
                node.as_dict(
                    detail=detail,
                    include_semantic=include_semantic,
                    include_confidence=include_confidence,
                    include_evidence=include_evidence,
                )
                for node in context
            ],
        }
    request = _search_request(arguments, profile_catalog=arguments.get("_profile_catalog"))
    return SearchService(
        store,
        repo_root=arguments.get("_repo_root"),
        profile_catalog=arguments.get("_profile_catalog"),
    ).search(request).as_dict(detail=request.detail)


def _query_payload(store: LadybugCodeGraphStore, arguments: dict[str, Any]) -> dict[str, Any]:
    """Build payload for MCP server and transport surface.

    Args:
        store: Graph store used for persistence or read-only queries.
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    statement = str(arguments.get("statement") or arguments.get("query") or "").strip()
    if not statement:
        raise ValueError("graph_query requires a non-empty statement")
    # Tool callers can supply arbitrary text, so the read-only gate runs before
    # parameters are inspected or the statement reaches Ladybug.
    _validate_read_only_statement(statement)
    parameters = arguments.get("parameters") or {}
    if not isinstance(parameters, dict):
        raise ValueError("graph_query parameters must be a JSON object")
    limit = _graph_query_limit(arguments)
    result = store.execute(statement, parameters)
    try:
        # Fetch one extra row to report truncation without materializing an
        # unbounded query response into memory.
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
    """Reject Cypher statements that could mutate the graph or inspect database internals.

    Args:
        statement: Statement used by the MCP server and transport surface workflow.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    stripped = statement.strip().rstrip(";")
    if ";" in stripped:
        raise ValueError("graph_query accepts one read-only statement at a time")
    match = READ_ONLY_DENY_RE.search(stripped)
    if match is not None:
        raise ValueError(f"graph_query is read-only; blocked keyword: {match.group(1).upper()}")


def _graph_query_limit(arguments: dict[str, Any]) -> int:
    """Return query limit for MCP server and transport surface.

    Args:
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        Integer count, status code, or index used by the caller.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    limit = int(arguments.get("limit", 100))
    if limit <= 0:
        raise ValueError("graph_query limit must be greater than zero")
    if limit > MAX_GRAPH_QUERY_LIMIT:
        raise ValueError(f"graph_query limit must be {MAX_GRAPH_QUERY_LIMIT} or less")
    return limit


def _explicit_context_limit(arguments: dict[str, Any]) -> int:
    """Return row limit for explicit-node graph_context calls."""
    context_limit = arguments.get("context_limit")
    if context_limit is not None and (
        "limit" not in arguments or int(context_limit) != DEFAULT_CONTEXT_LIMIT
    ):
        return int(context_limit)
    return int(arguments.get("limit", context_limit or DEFAULT_CONTEXT_LIMIT))


def _row_values(row: Any) -> list[Any]:
    """Build values for MCP server and transport surface.

    Args:
        row: Database row returned by Ladybug.

    Returns:
        Ordered results returned to the MCP server and transport surface caller.
    """
    try:
        return [_json_safe(value) for value in row]
    except TypeError:
        return [_json_safe(row)]


def _json_safe(value: Any) -> Any:
    """Manage safe within MCP server and transport surface.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Any instance populated with data from the MCP server and transport surface workflow.
    """
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, (list, tuple)):
        return [_json_safe(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _json_safe(item) for key, item in value.items()}
    return str(value)


def _optional_int(value: Any) -> int | None:
    """Manage int within MCP server and transport surface.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        int | None instance populated with data from the MCP server and transport surface
        workflow.
    """
    if value is None or value == "":
        return None
    return int(value)


def _optional_str(value: Any) -> str | None:
    """Manage str within MCP server and transport surface.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        str | None instance populated with data from the MCP server and transport surface
        workflow.
    """
    if value is None or value == "":
        return None
    return str(value)


def _bool(value: Any) -> bool:
    """Coerce tool and CLI boolean values."""
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.strip().lower() in {"1", "true", "yes", "on"}
    return bool(value)


def _optional_bool(value: Any) -> bool | None:
    """Coerce optional tool and CLI boolean values."""
    if value is None or value == "":
        return None
    return _bool(value)


def _detail(arguments: dict[str, Any]) -> str:
    """Manage MCP server and transport state.

    Args:
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        Formatted text returned to the caller.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    detail = str(arguments.get("detail", "standard"))
    if detail not in DETAIL_LEVELS:
        valid = ", ".join(sorted(DETAIL_LEVELS))
        raise ValueError(f"Unknown detail level: {detail}. Valid levels: {valid}")
    return detail


def _output_format(arguments: dict[str, Any]) -> str:
    """Manage format within MCP server and transport surface.

    Args:
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        Formatted text returned to the caller.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    output_format = str(arguments.get("output_format", "block"))
    if output_format not in {"json", "block"}:
        raise ValueError(f"Unknown output format: {output_format}. Valid formats: block, json")
    return output_format


def _include_structured_content(arguments: dict[str, Any]) -> bool:
    """Manage structured content within MCP server and transport surface.

    Args:
        arguments: Tool or command arguments supplied by the caller.

    Returns:
        True when the requested condition is satisfied; otherwise False.
    """
    value = arguments.get("include_structured_content", False)
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.strip().lower() in {"1", "true", "yes"}
    return bool(value)
