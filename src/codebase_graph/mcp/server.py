from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path
from typing import Any

from codebase_graph.db import LadybugCodeGraphStore, create_ladybug_database
from codebase_graph.ontology import QUERY_HELPERS, schema_payload
from codebase_graph.reasoning import CompactContextBuilder
from codebase_graph.retrieval import SearchRequest, SearchService
from codebase_graph.setup.state import derive_setup_paths, load_setup_config

READ_ONLY_DENY_RE = re.compile(
    r"\b(CREATE|DELETE|SET|MERGE|DROP|COPY|INSERT|LOAD|INSTALL|DETACH|REMOVE|ALTER|RENAME)\b",
    re.IGNORECASE,
)


@dataclass(frozen=True, slots=True)
class GraphRuntimeConfig:
    repo_root: Path
    db_path: Path
    manifest_path: Path | None = None


class McpGraphServer:
    def __init__(self, runtime: GraphRuntimeConfig) -> None:
        self.runtime = runtime

    @classmethod
    def from_paths(
        cls,
        *,
        repo_root: str | Path = ".",
        config_path: str | Path | None = None,
        db_path: str | Path | None = None,
        manifest_path: str | Path | None = None,
    ) -> McpGraphServer:
        runtime = _runtime_config(
            repo_root=repo_root,
            config_path=config_path,
            db_path=db_path,
            manifest_path=manifest_path,
        )
        return cls(runtime)

    def handle_json_rpc(self, message: dict[str, Any]) -> dict[str, Any] | None:
        method = str(message.get("method", ""))
        request_id = message.get("id")
        if method.startswith("notifications/"):
            return None
        try:
            if method == "initialize":
                result = {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "codebaseGraph", "version": _package_version()},
                }
            elif method == "ping":
                result = {}
            elif method == "tools/list":
                result = {"tools": _tool_specs()}
            elif method == "tools/call":
                params = dict(message.get("params") or {})
                payload = handle_tool_call(
                    str(params.get("name", "")),
                    dict(params.get("arguments") or {}),
                    runtime=self.runtime,
                )
                result = _tool_result(payload)
            else:
                return _rpc_error(request_id, -32601, f"Unsupported MCP method: {method}")
        except Exception as exc:
            return _rpc_error(request_id, -32000, str(exc))
        return {"jsonrpc": "2.0", "id": request_id, "result": result}


def handle_tool_call(name: str, arguments: dict[str, Any], *, runtime: GraphRuntimeConfig) -> dict[str, Any]:
    if name == "graph_health":
        return _health(runtime)
    if name == "graph_schema":
        return schema_payload()
    if name == "graph_query_helpers":
        return {"query_helpers": [helper.as_dict() for helper in QUERY_HELPERS]}
    if name == "graph_search":
        with _store(runtime) as store:
            request = _search_request(arguments)
            return SearchService(store).search(request).as_dict()
    if name == "graph_context":
        with _store(runtime) as store:
            return _context_payload(store, arguments)
    if name == "graph_query":
        with _store(runtime) as store:
            return _query_payload(store, arguments)
    raise ValueError(f"Unknown codebaseGraph MCP tool: {name}")


def serve_stdio(
    *,
    repo_root: str | Path = ".",
    config_path: str | Path | None = None,
    db_path: str | Path | None = None,
    manifest_path: str | Path | None = None,
) -> None:
    server = McpGraphServer.from_paths(
        repo_root=repo_root,
        config_path=config_path,
        db_path=db_path,
        manifest_path=manifest_path,
    )
    while True:
        message = _read_message(sys.stdin.buffer)
        if message is None:
            return
        response = server.handle_json_rpc(message)
        if response is not None:
            _write_message(sys.stdout.buffer, response)


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(prog="codebase-graph-mcp")
    parser.add_argument("--repo-root", default=".", help="Repository root containing .codebaseGraph/config.json")
    parser.add_argument("--config", default=None, help="Path to .codebaseGraph/config.json")
    parser.add_argument("--db", default=None, help="Override LadyBugDB path")
    parser.add_argument("--manifest", default=None, help="Override manifest path")
    args = parser.parse_args()
    serve_stdio(repo_root=args.repo_root, config_path=args.config, db_path=args.db, manifest_path=args.manifest)
    return 0


def _runtime_config(
    *,
    repo_root: str | Path,
    config_path: str | Path | None,
    db_path: str | Path | None,
    manifest_path: str | Path | None,
) -> GraphRuntimeConfig:
    root = Path(repo_root).expanduser().resolve()
    config = Path(config_path).expanduser().resolve() if config_path else derive_setup_paths(root).config_path
    payload: dict[str, Any] = {}
    if config.exists():
        payload = load_setup_config(config)
    elif db_path is None:
        raise FileNotFoundError(f"codebaseGraph setup config is missing: {config}")
    resolved_db = Path(db_path or payload["database_path"]).expanduser().resolve()
    resolved_manifest = Path(manifest_path or payload.get("manifest_path", "")).expanduser().resolve() if (manifest_path or payload.get("manifest_path")) else None
    if not resolved_db.exists():
        raise FileNotFoundError(f"codebaseGraph database is missing: {resolved_db}")
    return GraphRuntimeConfig(repo_root=root, db_path=resolved_db, manifest_path=resolved_manifest)


def _store(runtime: GraphRuntimeConfig) -> LadybugCodeGraphStore:
    return create_ladybug_database(runtime.db_path, include_fts=True)


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


def _tool_result(payload: dict[str, Any]) -> dict[str, Any]:
    return {
        "content": [{"type": "text", "text": json.dumps(payload, indent=2, sort_keys=True)}],
        "structuredContent": payload,
        "isError": False,
    }


def _tool_specs() -> list[dict[str, Any]]:
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


def _rpc_error(request_id: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}}


def _read_message(stream: Any) -> dict[str, Any] | None:
    line = stream.readline()
    if not line:
        return None
    if line.lower().startswith(b"content-length:"):
        length = int(line.split(b":", 1)[1].strip())
        while True:
            header = stream.readline()
            if header in {b"\r\n", b"\n", b""}:
                break
        body = stream.read(length)
        return json.loads(body.decode("utf-8"))
    return json.loads(line.decode("utf-8"))


def _write_message(stream: Any, message: dict[str, Any]) -> None:
    body = json.dumps(message, separators=(",", ":"), sort_keys=True).encode("utf-8")
    stream.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii"))
    stream.write(body)
    stream.flush()


def _package_version() -> str:
    try:
        return version("codebase-graph")
    except PackageNotFoundError:
        return "0.1.0"
