from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode
from codebase_graph.extract.graph_builder import (
    GraphBuilder,
    GraphBuildResult,
    ParseBundle,
    _capture_name,
    _capture_node,
    _capture_node_type,
    _label_for,
)

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


class NativeGraphBuilderUnavailable(RuntimeError):
    """Raised when strict native graph building is requested but unavailable."""


def build_file_graph(
    bundle: ParseBundle,
    *,
    strict: bool = True,
) -> GraphBuildResult:
    """Build a graph through the native capture-bundle prototype."""
    if not bundle.captures:
        raise NativeGraphBuilderUnavailable("native graph builder only supports captures")

    command = _native_command(strict=strict)
    if command is None:
        raise NativeGraphBuilderUnavailable("native graph builder command is unavailable")

    payload = _encode_bundle(bundle)
    try:
        completed = subprocess.run(
            command,
            input=payload,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        stderr = getattr(exc, "stderr", "")
        message = f"native graph builder failed: {stderr or exc}"
        raise NativeGraphBuilderUnavailable(message) from exc

    graph = _decode_graph(completed.stdout)
    if bundle.content_hash:
        for node in graph.nodes_by_type("File"):
            node.metadata["content_hash"] = bundle.content_hash
    return GraphBuildResult(
        nodes=graph.as_dict()["nodes"],
        edges=graph.as_dict()["edges"],
        diagnostics=[],
        unresolved=[],
        graph=graph,
    )


def _native_command(*, strict: bool) -> list[str] | None:
    configured = os.environ.get("CODEBASE_GRAPH_COMPAT_GRAPH_BUILDER")
    if configured:
        return [configured]

    binary = _built_binary_path()
    if binary.exists():
        return [binary.as_posix()]

    if strict and RUST_MANIFEST.exists() and shutil.which("cargo"):
        return [
            "cargo",
            "run",
            "--quiet",
            "--manifest-path",
            RUST_MANIFEST.as_posix(),
            "--bin",
            NATIVE_BINARY_NAME,
        ]
    return None


def _built_binary_path() -> Path:
    suffix = ".exe" if sys.platform.startswith("win") else ""
    return REPO_ROOT / "rust" / "target" / "debug" / f"{NATIVE_BINARY_NAME}{suffix}"


def _encode_bundle(bundle: ParseBundle) -> str:
    lines = [
        "\t".join(("META", "path", _hex(bundle.path))),
        "\t".join(("META", "language", _hex(bundle.language))),
        "\t".join(("META", "source_root", _hex(bundle.source_root))),
        "\t".join(("META", "repository_label", _hex(bundle.repository_label))),
    ]
    normalizer = GraphBuilder(repository_label=bundle.repository_label, source_root=bundle.source_root)
    for capture in bundle.captures:
        node = normalizer._normalize(
            {
                "type": _capture_node_type(capture),
                "capture_name": _capture_name(capture),
                "node": _capture_node(capture),
            }
        )
        field_names = ",".join(sorted(node.fields.keys()))
        lines.append(
            "\t".join(
                (
                    "CAP",
                    _hex(node.capture_name),
                    _hex(node.node_type),
                    _hex(_label_for(node)),
                    _hex(node.text),
                    _int_field(node.line_start),
                    _int_field(node.line_end),
                    _int_field(node.byte_start),
                    _int_field(node.byte_end),
                    _hex(field_names),
                )
            )
        )
    return "\n".join(lines) + "\n"


def _decode_graph(output: str) -> CodeGraph:
    graph = CodeGraph()
    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        record_type = parts[0]
        if record_type == "META":
            graph.metadata[_unhex(parts[1])] = json.loads(_unhex(parts[2]))
        elif record_type == "NODE":
            graph.add_node(
                GraphNode(
                    id=_unhex(parts[1]),
                    table=_unhex(parts[2]),
                    label=_unhex(parts[3]),
                    kind=_unhex(parts[4]),
                    language=_unhex(parts[5]),
                    path=_unhex(parts[6]),
                    qualified_name=_unhex(parts[7]),
                    scope_id=_unhex(parts[8]),
                    line_start=_int(parts[9]),
                    line_end=_int(parts[10]),
                    byte_start=_int(parts[11]),
                    byte_end=_int(parts[12]),
                    tree_sitter_node_type=_unhex(parts[13]),
                    capture_name=_unhex(parts[14]),
                    summary=_unhex(parts[15]),
                    metadata=_json(_unhex(parts[16])),
                )
            )
        elif record_type == "EDGE":
            graph.add_edge(
                GraphEdge(
                    id=_unhex(parts[1]),
                    type=_unhex(parts[2]),
                    source_id=_unhex(parts[3]),
                    target_id=_unhex(parts[4]),
                    kind=_unhex(parts[5]),
                    confidence=float(parts[6]),
                    line_start=_int(parts[7]),
                    line_end=_int(parts[8]),
                    byte_start=_int(parts[9]),
                    byte_end=_int(parts[10]),
                    metadata=_json(_unhex(parts[11])),
                )
            )
        else:
            raise ValueError(f"Unknown native graph builder record: {record_type}")
    return graph


def _hex(value: str) -> str:
    return value.encode("utf-8").hex()


def _unhex(value: str) -> str:
    return bytes.fromhex(value).decode("utf-8")


def _int_field(value: int | None) -> str:
    return "" if value is None else str(value)


def _int(value: str) -> int | None:
    return int(value) if value else None


def _json(value: str) -> dict[str, Any]:
    payload = json.loads(value)
    if not isinstance(payload, dict):
        raise ValueError("Native graph builder metadata must be a JSON object")
    return payload
