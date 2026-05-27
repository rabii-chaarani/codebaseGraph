from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, BinaryIO

from codebase_graph.mcp.protocol import McpGraphServer, rpc_error


class StdioMessageError(ValueError):
    pass


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
        try:
            message = read_message(sys.stdin.buffer)
        except StdioMessageError as exc:
            write_message(sys.stdout.buffer, rpc_error(None, -32700, f"Invalid JSON-RPC payload: {exc}"))
            continue
        if message is None:
            return
        response = server.handle_json_rpc(message)
        if response is not None:
            write_message(sys.stdout.buffer, response)


def read_message(stream: BinaryIO) -> dict[str, Any] | None:
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
    body = json.dumps(message, separators=(",", ":"), sort_keys=True).encode("utf-8")
    stream.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii"))
    stream.write(body)
    stream.flush()


def _json_rpc_payload(data: bytes) -> dict[str, Any]:
    try:
        payload = json.loads(data.decode("utf-8"))
    except UnicodeDecodeError as exc:
        raise StdioMessageError(f"Body must be UTF-8: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise StdioMessageError(str(exc)) from exc
    if not isinstance(payload, dict):
        raise StdioMessageError("JSON-RPC payload must be an object")
    return payload
