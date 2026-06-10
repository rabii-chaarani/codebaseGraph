from __future__ import annotations

from .protocol import LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS, McpGraphServer, negotiate_protocol_version
from .runtime import GraphRuntimeConfig
from .tools import handle_tool_call
from .transports.http import build_http_server, serve_http
from .transports.stdio import serve_stdio


def main() -> int:
    """Run the command-line entrypoint.

    Returns:
        The computed integer.
    """
    import argparse

    parser = argparse.ArgumentParser(prog="codebase-graph-mcp")
    parser.add_argument("--repo-root", default=".", help="Repository root containing .codebaseGraph/config.json")
    parser.add_argument("--config", default=None, help="Path to .codebaseGraph/config.json")
    parser.add_argument("--db", default=None, help="Override LadyBugDB path")
    parser.add_argument("--manifest", default=None, help="Override manifest path")
    args = parser.parse_args()
    serve_stdio(repo_root=args.repo_root, config_path=args.config, db_path=args.db, manifest_path=args.manifest)
    return 0


__all__ = [
    "LATEST_PROTOCOL_VERSION",
    "SUPPORTED_PROTOCOL_VERSIONS",
    "GraphRuntimeConfig",
    "McpGraphServer",
    "build_http_server",
    "handle_tool_call",
    "negotiate_protocol_version",
    "serve_http",
    "serve_stdio",
]
