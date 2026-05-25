from __future__ import annotations

import argparse
import json
from collections.abc import Sequence
from pathlib import Path

from codebase_graph.db import create_ladybug_database
from codebase_graph.ingest import GraphMaterializer
from codebase_graph.ontology import CONTEXT_PROFILES
from codebase_graph.retrieval import SearchRequest, SearchService
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

    search_parser = subparsers.add_parser("search", help="Search the code graph with compact context")
    _add_search_arguments(search_parser)

    context_parser = subparsers.add_parser("context", help="Return compact context for a search query")
    _add_search_arguments(context_parser)

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

    mcp_parser = subparsers.add_parser("mcp", help="Run or inspect the MCP server")
    mcp_subparsers = mcp_parser.add_subparsers(dest="mcp_command", required=True)
    install_parser = mcp_subparsers.add_parser("install", help="Install the MCP server in a supported client")
    install_parser.add_argument("--client", choices=supported_install_client_ids(include_all=True), default="codex")
    install_parser.add_argument("--scope", choices=("local", "user", "project"), default="local")
    install_parser.add_argument("--name", default=None, help="MCP server name; defaults to codebase_graph-<repo>")
    install_parser.add_argument("--config-path", default=None, help="Path to .codebaseGraph/config.json")
    install_parser.add_argument("--repo-root", default=".", help="Repository root used to find .codebaseGraph/config.json")
    install_parser.add_argument("--dry-run", action="store_true", help="Show the install action without writing or invoking CLIs")
    install_parser.add_argument("--verify", action="store_true", help="Run direct MCP smoke checks after installation")
    install_parser.add_argument("--json", action="store_true", help="Emit JSON output")

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
        print(json.dumps(_result_payload(result), indent=2, sort_keys=True))
        return 0
    if args.command in {"search", "context"}:
        request = SearchRequest(
            query=args.query,
            limit=args.limit,
            profile=args.profile,
            budget=args.budget,
            max_depth=args.max_depth,
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
        print(json.dumps(payload.as_dict(), indent=2, sort_keys=True))
        return 0
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
        print(json.dumps(result.as_dict(), indent=2, sort_keys=True))
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
            print(json.dumps(payload, indent=2, sort_keys=True))
        else:
            _print_mcp_install_results(results)
        return 1 if any(result.action == "failed" for result in results) else 0
    if args.command == "mcp" and args.mcp_command == "serve":
        from codebase_graph.mcp.server import serve_stdio

        serve_stdio(repo_root=args.repo_root, config_path=args.config, db_path=args.db, manifest_path=args.manifest)
        return 0
    if args.command == "mcp" and args.mcp_command == "http":
        from codebase_graph.mcp.server import serve_http

        serve_http(
            repo_root=args.repo_root,
            config_path=args.config,
            db_path=args.db,
            manifest_path=args.manifest,
            host=args.host,
            port=args.port,
            endpoint_path=args.path,
        )
        return 0
    parser.error(f"Unknown command: {args.command}")
    return 2


def _add_search_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("query", help="Search query")
    parser.add_argument("--source-root", default=".", help="Repository or source root to search")
    parser.add_argument("--db", default=None, help="LadybugDB path; defaults under .codebaseGraph")
    parser.add_argument("--manifest", default=None, help="Manifest path; defaults under .codebaseGraph")
    parser.add_argument("--limit", type=int, default=3, help="Maximum search hits to return")
    parser.add_argument("--profile", choices=sorted(CONTEXT_PROFILES), default="brief", help="Context profile")
    parser.add_argument("--budget", type=int, default=600, help="Approximate per-hit context character budget")
    parser.add_argument("--max-depth", type=int, default=None, help="Override the context profile depth")
    parser.add_argument("--no-refresh", action="store_true", help="Query the existing graph without changed materialization")
    parser.add_argument("--json", action="store_true", help="Emit compact JSON output")


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


__all__ = ["main"]
