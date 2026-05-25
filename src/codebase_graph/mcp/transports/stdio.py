from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, BinaryIO

from codebase_graph.mcp.protocol import McpGraphServer


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
        message = read_message(sys.stdin.buffer)
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
        length = int(line.split(b":", 1)[1].strip())
        while True:
            header = stream.readline()
            if header in {b"\r\n", b"\n", b""}:
                break
        body = stream.read(length)
        return json.loads(body.decode("utf-8"))
    return json.loads(line.decode("utf-8"))


def write_message(stream: BinaryIO, message: dict[str, Any]) -> None:
    body = json.dumps(message, separators=(",", ":"), sort_keys=True).encode("utf-8")
    stream.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii"))
    stream.write(body)
    stream.flush()
