from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, BinaryIO

from codebase_graph.diagnostics import log_event
from codebase_graph.mcp.protocol import McpGraphServer, rpc_error


class StdioMessageError(ValueError):
    """Signal failures raised by the MCP server and transport surface subsystem."""
    pass


def serve_stdio(
    *,
    repo_root: str | Path = ".",
    config_path: str | Path | None = None,
    db_path: str | Path | None = None,
    manifest_path: str | Path | None = None,
) -> None:
    """Serve stdio for MCP server and transport surface.

    This starts a transport loop and blocks until the server stops.

    Args:
        repo_root: Repository root used to resolve graph state paths.
        config_path: Setup configuration path used to resolve runtime state.
        db_path: Ladybug database path, or an in-memory database marker.
        manifest_path: Manifest path used to track previously materialized file partitions.
    """
    server = McpGraphServer.from_paths(
        repo_root=repo_root,
        config_path=config_path,
        db_path=db_path,
        manifest_path=manifest_path,
    )
    while True:
        try:
            message = read_message(sys.stdin.buffer)
        except StdioMessageError as exc:
            log_event("mcp.stdio_parse_error", level="WARNING", message=str(exc))
            write_message(sys.stdout.buffer, rpc_error(None, -32700, f"Invalid JSON-RPC payload: {exc}"))
            continue
        if message is None:
            return
        response = server.handle_json_rpc(message)
        if response is not None:
            write_message(sys.stdout.buffer, response)


def read_message(stream: BinaryIO) -> dict[str, Any] | None:
    """Read message for MCP server and transport surface.

    Args:
        stream: Binary stream used for newline-delimited JSON-RPC messages.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.

    Raises:
        StdioMessageError: Raised when validation or runtime preconditions fail.
    """
    line = stream.readline()
    if not line:
        return None
    if line.lower().startswith(b"content-length:"):
        try:
            length = int(line.split(b":", 1)[1].strip())
        except ValueError as exc:
            raise StdioMessageError("Content-Length must be an integer") from exc
        if length < 0:
            raise StdioMessageError("Content-Length must be non-negative")
        while True:
            header = stream.readline()
            if header in {b"\r\n", b"\n", b""}:
                break
        body = stream.read(length)
        if len(body) != length:
            raise StdioMessageError("Body ended before Content-Length bytes were read")
        return _json_rpc_payload(body)
    return _json_rpc_payload(line)


def write_message(stream: BinaryIO, message: dict[str, Any]) -> None:
    """Write message for MCP server and transport surface.

    This writes to disk and should leave complete files on success.

    Args:
        stream: Binary stream used for newline-delimited JSON-RPC messages.
        message: JSON-RPC request or notification body.
    """
    body = json.dumps(message, separators=(",", ":"), sort_keys=True).encode("utf-8")
    stream.write(body)
    stream.write(b"\n")
    stream.flush()


def _json_rpc_payload(data: bytes) -> dict[str, Any]:
    """Manage RPC payload within MCP server and transport surface.

    Args:
        data: Raw bytes received from a transport.

    Returns:
        Structured mapping that follows the MCP server and transport surface response contract.

    Raises:
        StdioMessageError: Raised when validation or runtime preconditions fail.
    """
    try:
        payload = json.loads(data.decode("utf-8"))
    except UnicodeDecodeError as exc:
        raise StdioMessageError(f"Body must be UTF-8: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise StdioMessageError(str(exc)) from exc
    if not isinstance(payload, dict):
        raise StdioMessageError("JSON-RPC payload must be an object")
    return payload
