from __future__ import annotations

import argparse
import json
import os
from collections.abc import Sequence
from pathlib import Path

from codebase_graph.db import create_ladybug_database
from codebase_graph.ingest import GraphMaterializer
from codebase_graph.mcp.runtime import runtime_config
from codebase_graph.mcp.tools import handle_tool_call
from codebase_graph.ontology import CONTEXT_PROFILES, QUERY_HELPERS, schema_payload
from codebase_graph.reasoning import architecture_query_catalog
from codebase_graph.retrieval import SearchRequest, SearchService, serialize_graph_block
from codebase_graph.setup import SetupError, SetupOptions, run_setup
from codebase_graph.setup.clients import supported_client_ids
from codebase_graph.setup.installer import McpInstallOptions, install_mcp_clients, supported_install_client_ids


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="codebase-graph")
    subparsers = parser.add_subparsers(dest="command", required=True)

    materialize_parser = subparsers.add_parser("materialize", help="Materialize the code graph")
    materialize_parser.add_argument("--source-root", default=".", help="Repository or source root to scan")
    materialize_parser.add_argument("--db", default=None, help="LadybugDB path; defaults under .codebaseGraph")
    materialize_parser.add_argument("--manifest", default=None, help="Manifest path; defaults under .codebaseGraph")
    materialize_parser.add_argument("--mode", choices=("full", "changed"), default="changed")
    materialize_parser.add_argument("--no-fts", action="store_true", help="Skip FTS index creation")
    _add_json_output_arguments(materialize_parser)

    search_parser = subparsers.add_parser("search", help="Search the code graph with compact context")
    _add_search_arguments(search_parser)

    context_parser = subparsers.add_parser("context", help="Return compact context for a search query")
    _add_search_arguments(context_parser)

    graph_health_parser = subparsers.add_parser("graph-health", help="Check configured graph paths")
    _add_runtime_arguments(graph_health_parser)
    _add_json_output_arguments(graph_health_parser)

    graph_search_parser = subparsers.add_parser("graph-search", help="Search the code graph with compact context")
    graph_search_parser.add_argument("query", help="Search query")
    _add_compact_context_arguments(graph_search_parser)
    _add_runtime_arguments(graph_search_parser)
    _add_graph_compatibility_arguments(graph_search_parser)

    graph_context_parser = subparsers.add_parser("graph-context", help="Return compact graph context")
    graph_context_parser.add_argument("query", nargs="?", help="Search query")
    graph_context_parser.add_argument("--node-id", default=None, help="Explicit graph node id")
    graph_context_parser.add_argument("--node-type", default=None, help="Explicit graph node type")
    _add_compact_context_arguments(graph_context_parser)
    _add_runtime_arguments(graph_context_parser)
    _add_graph_compatibility_arguments(graph_context_parser)

    graph_schema_parser = subparsers.add_parser("graph-schema", help="Return ontology schema, indexes, profiles, and helpers")
    _add_json_output_arguments(graph_schema_parser)
    graph_query_helpers_parser = subparsers.add_parser("graph-query-helpers", help="Return named read-only graph query helpers")
    _add_json_output_arguments(graph_query_helpers_parser)

    graph_architecture_parser = subparsers.add_parser(
        "graph-architecture-queries",
        help="Return the architecture-discovery query catalog",
    )
    graph_architecture_parser.add_argument("--group", default=None, help="Optional architecture query group")
    _add_json_output_arguments(graph_architecture_parser)

    graph_query_parser = subparsers.add_parser("graph-query", help="Execute a restricted read-only graph query")
    graph_query_parser.add_argument("statement", help="Read-only graph query statement")
    graph_query_parser.add_argument("--parameters", default="{}", help="JSON object with query parameters")
    graph_query_parser.add_argument("--limit", type=int, default=100, help="Maximum rows to return")
    _add_runtime_arguments(graph_query_parser)
    _add_json_output_arguments(graph_query_parser)

    setup_parser = subparsers.add_parser("setup", help="Bootstrap codebaseGraph state for a repository")
    setup_parser.add_argument("--repo-root", default=".", help="Repository root to configure")
    setup_parser.add_argument("--mcp-client", choices=supported_client_ids(), default="codex")
    setup_parser.add_argument("--mcp-config-path", default=None, help="Override MCP client config path")
    setup_parser.add_argument("--skip-mcp-config", action="store_true", help="Do not write MCP client config")
    setup_parser.add_argument("--dry-run", action="store_true", help="Return the MCP config patch without writing it")
    setup_parser.add_argument(
        "--instructions-target",
        choices=("auto", "agents", "claude", "skip"),
        default="auto",
        help="Instruction file to update",
    )
    setup_parser.add_argument("--mode", choices=("full", "changed"), default="changed", help="Materialization mode")
    setup_parser.add_argument("--json", action="store_true", help="Emit JSON output")
    _add_json_output_arguments(setup_parser)

    mcp_parser = subparsers.add_parser("mcp", help="Run or inspect the MCP server")
    mcp_subparsers = mcp_parser.add_subparsers(dest="mcp_command", required=True)
    install_parser = mcp_subparsers.add_parser("install", help="Install the MCP server in a supported client")
    install_parser.add_argument("--client", choices=supported_install_client_ids(include_all=True), default="codex")
    install_parser.add_argument("--scope", choices=("local", "user", "project"), default="local")
    install_parser.add_argument("--name", default=None, help="MCP server name; defaults to codebase_graph-<repo>")
    install_parser.add_argument("--config-path", default=None, help="Path to .codebaseGraph/config.json")
    install_parser.add_argument("--client-config-path", default=None, help="Override the target MCP client config path")
    install_parser.add_argument("--repo-root", default=".", help="Repository root used to find .codebaseGraph/config.json")
    install_parser.add_argument("--dry-run", action="store_true", help="Show the install action without writing or invoking CLIs")
    install_parser.add_argument("--verify", action="store_true", help="Run direct MCP smoke checks after installation")
    install_parser.add_argument("--json", action="store_true", help="Emit JSON output")
    _add_json_output_arguments(install_parser)

    serve_parser = mcp_subparsers.add_parser("serve", help="Serve graph tools over MCP stdio")
    serve_parser.add_argument("--repo-root", default=".", help="Repository root containing .codebaseGraph/config.json")
    serve_parser.add_argument("--config", default=None, help="Path to .codebaseGraph/config.json")
    serve_parser.add_argument("--db", default=None, help="Override LadyBugDB path")
    serve_parser.add_argument("--manifest", default=None, help="Override manifest path")
    http_parser = mcp_subparsers.add_parser("http", help="Serve graph tools over Streamable HTTP")
    http_parser.add_argument("--repo-root", default=".", help="Repository root containing .codebaseGraph/config.json")
    http_parser.add_argument("--config", default=None, help="Path to .codebaseGraph/config.json")
    http_parser.add_argument("--db", default=None, help="Override LadyBugDB path")
    http_parser.add_argument("--manifest", default=None, help="Override manifest path")
    http_parser.add_argument("--host", default="127.0.0.1", help="HTTP bind host; default keeps the server local")
    http_parser.add_argument("--port", type=int, default=8765, help="HTTP bind port")
    http_parser.add_argument("--path", default="/mcp", help="MCP HTTP endpoint path")
    http_parser.add_argument(
        "--allow-remote",
        action="store_true",
        help="Allow binding MCP HTTP to a non-local host; requires an auth token",
    )
    http_parser.add_argument(
        "--auth-token",
        default=None,
        help="Bearer token required for HTTP requests; prefer --auth-token-env to avoid shell history exposure",
    )
    http_parser.add_argument("--auth-token-env", default=None, help="Environment variable containing the HTTP bearer token")

    args = parser.parse_args(argv)
    if args.command == "materialize":
        materializer = GraphMaterializer(
            Path(args.source_root),
            db_path=args.db,
            manifest_path=args.manifest,
            include_fts=not args.no_fts,
        )
        try:
            result = materializer.materialize(mode=args.mode)
        finally:
            materializer.close()
        _print_json(_result_payload(result), args)
        return 0
    if args.command in {"search", "context"}:
        request = SearchRequest(
            query=args.query,
            limit=args.limit,
            profile=args.profile,
            budget=args.budget,
            max_depth=args.max_depth,
            context_limit=args.context_limit,
            detail=args.detail,
        )
        try:
            request.validate()
        except ValueError as exc:
            parser.error(str(exc))
        materializer = GraphMaterializer(
            Path(args.source_root),
            db_path=args.db,
            manifest_path=args.manifest,
            include_fts=True,
        )
        if args.no_refresh:
            with create_ladybug_database(materializer.db_path, include_fts=True, read_only=True) as store:
                payload = SearchService(store).search(request)
        else:
            try:
                materializer.materialize(mode="changed")
                payload = SearchService(materializer.store).search(request)
            finally:
                materializer.close()
        _print_payload(payload.as_dict(detail=args.detail), args)
        return 0
    if args.command == "graph-health":
        return _print_tool_payload(parser, "graph_health", {}, args)
    if args.command == "graph-search":
        return _print_tool_payload(parser, "graph_search", _search_arguments_payload(args), args)
    if args.command == "graph-context":
        if not args.query and not (args.node_id and args.node_type):
            parser.error("graph-context requires a query or both --node-id and --node-type")
        if (args.node_id and not args.node_type) or (args.node_type and not args.node_id):
            parser.error("graph-context explicit lookup requires both --node-id and --node-type")
        payload = _search_arguments_payload(args)
        if args.node_id and args.node_type:
            payload["node_id"] = args.node_id
            payload["node_type"] = args.node_type
        return _print_tool_payload(parser, "graph_context", payload, args)
    if args.command == "graph-schema":
        _print_json(schema_payload(), args)
        return 0
    if args.command == "graph-query-helpers":
        _print_json({"query_helpers": [helper.as_dict() for helper in QUERY_HELPERS]}, args)
        return 0
    if args.command == "graph-architecture-queries":
        try:
            payload = architecture_query_catalog(group=args.group)
        except ValueError as exc:
            parser.error(str(exc))
        _print_json(payload, args)
        return 0
    if args.command == "graph-query":
        try:
            parameters = json.loads(args.parameters)
        except json.JSONDecodeError as exc:
            parser.error(f"graph-query --parameters must be a JSON object: {exc}")
        if not isinstance(parameters, dict):
            parser.error("graph-query --parameters must be a JSON object")
        return _print_tool_payload(
            parser,
            "graph_query",
            {"statement": args.statement, "parameters": parameters, "limit": args.limit},
            args,
        )
    if args.command == "setup":
        try:
            result = run_setup(
                SetupOptions(
                    repo_root=args.repo_root,
                    mcp_client=args.mcp_client,
                    mcp_config_path=args.mcp_config_path,
                    skip_mcp_config=args.skip_mcp_config,
                    dry_run=args.dry_run,
                    instructions_target=args.instructions_target,
                    mode=args.mode,
                )
            )
        except SetupError as exc:
            parser.error(str(exc))
        _print_json(result.as_dict(), args)
        return 0
    if args.command == "mcp" and args.mcp_command == "install":
        setup_config_path = (
            Path(args.config_path).expanduser().resolve()
            if args.config_path is not None
            else Path(args.repo_root).expanduser().resolve() / ".codebaseGraph" / "config.json"
        )
        try:
            results = install_mcp_clients(
                McpInstallOptions(
                    client=args.client,
                    scope=args.scope,
                    setup_config_path=setup_config_path,
                    server_name=args.name,
                    client_config_path=args.client_config_path,
                    dry_run=args.dry_run,
                    verify=args.verify,
                )
            )
        except (OSError, ValueError) as exc:
            parser.error(str(exc))
        payload: dict[str, object]
        if args.client == "all":
            payload = {"results": [result.as_dict() for result in results]}
        else:
            payload = results[0].as_dict()
        if args.json:
            _print_json(payload, args)
        else:
            _print_mcp_install_results(results)
        return 1 if any(result.action == "failed" for result in results) else 0
    if args.command == "mcp" and args.mcp_command == "serve":
        from codebase_graph.mcp.server import serve_stdio

        serve_stdio(repo_root=args.repo_root, config_path=args.config, db_path=args.db, manifest_path=args.manifest)
        return 0
    if args.command == "mcp" and args.mcp_command == "http":
        from codebase_graph.mcp.server import serve_http

        auth_token = _http_auth_token(args, parser)
        serve_http(
            repo_root=args.repo_root,
            config_path=args.config,
            db_path=args.db,
            manifest_path=args.manifest,
            host=args.host,
            port=args.port,
            endpoint_path=args.path,
            allow_remote=args.allow_remote,
            auth_token=auth_token,
        )
        return 0
    parser.error(f"Unknown command: {args.command}")
    return 2


def _add_search_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("query", help="Search query")
    parser.add_argument("--source-root", default=".", help="Repository or source root to search")
    parser.add_argument("--db", default=None, help="LadybugDB path; defaults under .codebaseGraph")
    parser.add_argument("--manifest", default=None, help="Manifest path; defaults under .codebaseGraph")
    _add_compact_context_arguments(parser)
    parser.add_argument("--no-refresh", action="store_true", help="Query the existing graph without changed materialization")
    parser.add_argument("--json", action="store_true", help="Emit compact JSON output")


def _add_compact_context_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--limit", type=int, default=3, help="Maximum search hits to return")
    parser.add_argument("--profile", choices=sorted(CONTEXT_PROFILES), default="brief", help="Context profile")
    parser.add_argument("--budget", type=int, default=600, help="Approximate per-hit context character budget")
    parser.add_argument("--max-depth", type=int, default=None, help="Override the context profile depth")
    parser.add_argument("--context-limit", type=int, default=3, help="Maximum context items per search hit")
    parser.add_argument("--detail", choices=("standard", "slim"), default="standard", help="Output detail level")
    parser.add_argument("--format", choices=("json", "block"), default="json", help="Output format")
    _add_json_output_arguments(parser)


def _add_runtime_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--repo-root", default=".", help="Repository root containing .codebaseGraph/config.json")
    parser.add_argument("--config", default=None, help="Path to .codebaseGraph/config.json")
    parser.add_argument("--db", default=None, help="Override LadyBugDB path")
    parser.add_argument("--manifest", default=None, help="Override manifest path")


def _add_graph_compatibility_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--no-refresh", action="store_true", help="Accepted for search/context command parity")
    parser.add_argument("--json", action="store_true", help="Accepted for search/context command parity; same as --format json")


def _runtime(args: argparse.Namespace) -> object:
    return runtime_config(
        repo_root=args.repo_root,
        config_path=args.config,
        db_path=args.db,
        manifest_path=args.manifest,
    )


def _search_arguments_payload(args: argparse.Namespace) -> dict[str, object]:
    payload: dict[str, object] = {
        "limit": args.limit,
        "profile": args.profile,
        "budget": args.budget,
        "context_limit": args.context_limit,
        "detail": args.detail,
    }
    if args.query:
        payload["query"] = args.query
    if args.max_depth is not None:
        payload["max_depth"] = args.max_depth
    return payload


def _print_tool_payload(
    parser: argparse.ArgumentParser,
    tool_name: str,
    arguments: dict[str, object],
    args: argparse.Namespace,
) -> int:
    try:
        payload = handle_tool_call(tool_name, arguments, runtime=_runtime(args))
    except (OSError, ValueError) as exc:
        parser.error(str(exc))
    _print_payload(payload, args)
    return 0


def _add_json_output_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--pretty", action="store_true", help="Emit indented JSON output")


def _print_json(payload: object, args: argparse.Namespace) -> None:
    print(_json_dumps(payload, pretty=getattr(args, "pretty", False)))


def _print_payload(payload: dict[str, object], args: argparse.Namespace) -> None:
    if getattr(args, "json", False):
        _print_json(payload, args)
        return
    if getattr(args, "format", "json") == "block":
        print(serialize_graph_block(payload), end="")
        return
    _print_json(payload, args)


def _json_dumps(payload: object, *, pretty: bool) -> str:
    if pretty:
        return json.dumps(payload, indent=2, sort_keys=True)
    return json.dumps(payload, separators=(",", ":"), sort_keys=True)


def _result_payload(result: object) -> dict[str, object]:
    return {
        "mode": getattr(result, "mode"),
        "scanned": getattr(result, "scanned"),
        "rebuilt": getattr(result, "rebuilt"),
        "skipped": getattr(result, "skipped"),
        "deleted": getattr(result, "deleted"),
        "diagnostics": list(getattr(result, "diagnostics")),
        "manifest_path": getattr(result, "manifest_path"),
        "rebuilt_paths": list(getattr(result, "rebuilt_paths")),
        "skipped_paths": list(getattr(result, "skipped_paths")),
        "deleted_paths": list(getattr(result, "deleted_paths")),
        "graph_summary": dict(getattr(result, "graph_summary")),
    }


def _print_mcp_install_results(results: Sequence[object]) -> None:
    for result in results:
        action = getattr(result, "action")
        client = getattr(result, "client")
        method = getattr(result, "method") or "none"
        server_name = getattr(result, "server_name")
        target = getattr(result, "path") or " ".join(getattr(result, "command") or [])
        suffix = f" -> {target}" if target else ""
        print(f"{client}: {action} {server_name} via {method}{suffix}")


def _http_auth_token(args: argparse.Namespace, parser: argparse.ArgumentParser) -> str | None:
    if args.auth_token and args.auth_token_env:
        parser.error("mcp http accepts either --auth-token or --auth-token-env, not both")
    if args.auth_token_env:
        value = os.environ.get(args.auth_token_env)
        if not value:
            parser.error(f"Environment variable {args.auth_token_env!r} must contain the HTTP bearer token")
        return value
    return args.auth_token


__all__ = ["main"]
