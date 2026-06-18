from __future__ import annotations

import subprocess
import sys
from collections.abc import Sequence

from codebase_graph.native_binary import resolve_native_product_binary

from .protocol import LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS, McpGraphServer, negotiate_protocol_version
from .runtime import GraphRuntimeConfig
from .tools import handle_tool_call


def main(argv: Sequence[str] | None = None) -> int:
    argv_list = list(argv) if argv is not None else sys.argv[1:]
    return _run_native_mcp_serve(argv_list)


def _run_native_mcp_serve(argv: Sequence[str]) -> int:
    command_args = ["mcp", *argv] if any(arg in {"-h", "--help"} for arg in argv) else ["mcp", "serve", *argv]
    native_binary = _native_product_binary()
    if native_binary is None:
        raise SystemExit(
            "Rust native MCP binary is required. Build or install `codebase-graph`, "
            "or set CODEBASE_GRAPH_NATIVE_CLI to its absolute path."
        )
    status = subprocess.call([native_binary, *command_args])
    if status:
        raise SystemExit(status)
    return status


def _native_product_binary() -> str | None:
    return resolve_native_product_binary(skip_current_script=True)


__all__ = [
    "LATEST_PROTOCOL_VERSION",
    "SUPPORTED_PROTOCOL_VERSIONS",
    "GraphRuntimeConfig",
    "McpGraphServer",
    "handle_tool_call",
    "negotiate_protocol_version",
]
