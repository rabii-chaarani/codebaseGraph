from __future__ import annotations

import argparse
import json
import os
from collections.abc import Sequence
from pathlib import Path

from codebase_graph.db import create_ladybug_database
from codebase_graph.ingest import GraphMaterializer
from codebase_graph.mcp.graph_commands import (
    add_compact_context_arguments,
    add_json_output_arguments,
    graph_command_names,
    graph_command_spec,
    graph_command_specs,
)
from codebase_graph.mcp.runtime import runtime_config
from codebase_graph.mcp.tools import handle_tool_call
from codebase_graph.retrieval import SearchRequest, SearchService, serialize_graph_block
from codebase_graph.setup import SetupError, SetupOptions, run_setup
from codebase_graph.setup.clients import supported_client_ids
from codebase_graph.setup.installer import McpInstallOptions, install_mcp_clients, supported_install_client_ids


def main(argv: Sequence[str] | None = None) -> int:
    """Run the command-line entrypoint.

    Args:
        argv: Optional command-line arguments. Defaults to process arguments when omitted.

    Returns:
        The computed integer.
    """
    parser = _build_parser()
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
        _print_json(result.as_dict(), args)
        return 0
    if args.command in {"search", "context"}:
        return _run_legacy_search_command(parser, args)
    if args.command in graph_command_names():
        return _run_graph_command(parser, args)
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


def _build_parser() -> argparse.ArgumentParser:
    """Build parser.

    Returns:
        The computed result.
    """
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

    for spec in graph_command_specs():
        graph_parser = subparsers.add_parser(spec.command_name, help=spec.help)
        spec.add_arguments(graph_parser)

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
    return parser


def _add_search_arguments(parser: argparse.ArgumentParser) -> None:
    """Add search arguments.

    Args:
        parser: The parser used by the operation.
    """
    parser.add_argument("query", help="Search query")
    parser.add_argument("--source-root", default=".", help="Repository or source root to search")
    parser.add_argument("--db", default=None, help="LadybugDB path; defaults under .codebaseGraph")
    parser.add_argument("--manifest", default=None, help="Manifest path; defaults under .codebaseGraph")
    add_compact_context_arguments(parser)
    parser.add_argument("--no-refresh", action="store_true", help="Query the existing graph without changed materialization")
    parser.add_argument("--json", action="store_true", help="Emit compact JSON output")


def _runtime(args: argparse.Namespace) -> object:
    """Process runtime.

    Args:
        args: Parsed command-line arguments.

    Returns:
        The computed result.
    """
    return runtime_config(
        repo_root=args.repo_root,
        config_path=args.config,
        db_path=args.db,
        manifest_path=args.manifest,
    )


def _run_legacy_search_command(parser: argparse.ArgumentParser, args: argparse.Namespace) -> int:
    """Run legacy search command.

    Args:
        parser: The parser used by the operation.
        args: Parsed command-line arguments.

    Returns:
        The computed integer.
    """
    try:
        request = _search_request_from_args(args)
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


def _search_request_from_args(args: argparse.Namespace) -> SearchRequest:
    """Search request from args.

    Args:
        args: Parsed command-line arguments.

    Returns:
        The computed result.
    """
    request = SearchRequest(
        query=args.query,
        limit=args.limit,
        profile=args.profile,
        budget=args.budget,
        max_depth=args.max_depth,
        context_limit=args.context_limit,
        detail=args.detail,
    )
    request.validate()
    return request


def _run_graph_command(parser: argparse.ArgumentParser, args: argparse.Namespace) -> int:
    """Run graph command.

    Args:
        parser: The parser used by the operation.
        args: Parsed command-line arguments.

    Returns:
        The computed integer.
    """
    spec = graph_command_spec(args.command)
    try:
        arguments = spec.payload_from_args(args)
        runtime = _runtime(args) if spec.requires_runtime else None
        payload = handle_tool_call(spec.tool_name, arguments, runtime=runtime)
    except (OSError, ValueError) as exc:
        parser.error(str(exc))
    _print_payload(payload, args)
    return 0


def _add_json_output_arguments(parser: argparse.ArgumentParser) -> None:
    """Add JSON output arguments.

    Args:
        parser: The parser used by the operation.
    """
    add_json_output_arguments(parser)


def _print_json(payload: object, args: argparse.Namespace) -> None:
    """Print JSON.

    Args:
        payload: Payload to process.
        args: Parsed command-line arguments.
    """
    print(_json_dumps(payload, pretty=getattr(args, "pretty", False)))


def _print_payload(payload: dict[str, object], args: argparse.Namespace) -> None:
    """Print payload.

    Args:
        payload: Payload to process.
        args: Parsed command-line arguments.
    """
    if getattr(args, "json", False):
        _print_json(payload, args)
        return
    if getattr(args, "format", "json") == "block":
        print(serialize_graph_block(payload), end="")
        return
    _print_json(payload, args)


def _json_dumps(payload: object, *, pretty: bool) -> str:
    """Process JSON dumps.

    Args:
        payload: Payload to process.
        pretty: Pretty value.

    Returns:
        The computed string.
    """
    if pretty:
        return json.dumps(payload, indent=2, sort_keys=True)
    return json.dumps(payload, separators=(",", ":"), sort_keys=True)


def _print_mcp_install_results(results: Sequence[object]) -> None:
    """Print MCP install results.

    Args:
        results: Results value.
    """
    for result in results:
        action = getattr(result, "action")
        client = getattr(result, "client")
        method = getattr(result, "method") or "none"
        server_name = getattr(result, "server_name")
        target = getattr(result, "path") or " ".join(getattr(result, "command") or [])
        suffix = f" -> {target}" if target else ""
        print(f"{client}: {action} {server_name} via {method}{suffix}")


def _http_auth_token(args: argparse.Namespace, parser: argparse.ArgumentParser) -> str | None:
    """Process HTTP auth token.

    Args:
        args: Parsed command-line arguments.
        parser: The parser used by the operation.

    Returns:
        The computed result.
    """
    if args.auth_token and args.auth_token_env:
        parser.error("mcp http accepts either --auth-token or --auth-token-env, not both")
    if args.auth_token_env:
        value = os.environ.get(args.auth_token_env)
        if not value:
            parser.error(f"Environment variable {args.auth_token_env!r} must contain the HTTP bearer token")
        return value
    return args.auth_token


__all__ = ["main"]
