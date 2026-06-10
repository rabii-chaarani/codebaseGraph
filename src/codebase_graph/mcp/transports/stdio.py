from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, BinaryIO

from codebase_graph.diagnostics import log_event
from codebase_graph.mcp.protocol import McpGraphServer, rpc_error


class StdioMessageError(ValueError):
    """Signal stdio message error failures."""
    pass


def serve_stdio(
    *,
    repo_root: str | Path = ".",
    config_path: str | Path | None = None,
    db_path: str | Path | None = None,
    manifest_path: str | Path | None = None,
) -> None:
    """Serve stdio.

    Args:
        repo_root: Repo root value.
        config_path: The config path to read or write.
        db_path: The db path to read or write.
        manifest_path: The manifest path to read or write.
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
    """Read message.

    Args:
        stream: Stream value.

    Returns:
        A dictionary containing the computed payload.
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
    """Write message.

    Args:
        stream: Stream value.
        message: The message payload to process.
    """
    body = json.dumps(message, separators=(",", ":"), sort_keys=True).encode("utf-8")
    stream.write(body)
    stream.write(b"\n")
    stream.flush()


def _json_rpc_payload(data: bytes) -> dict[str, Any]:
    """Process JSON RPC payload.

    Args:
        data: Data value.

    Returns:
        A dictionary containing the computed payload.
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
