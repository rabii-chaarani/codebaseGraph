from __future__ import annotations

import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


class NativeBulkStagingUnavailable(RuntimeError):
    """Raised when strict native bulk staging is requested but unavailable."""


@dataclass(frozen=True)
class NativeBulkStagingResult:
    copy_statements: list[str]
    node_rows: int
    edge_rows: int
    connector_rows: int


def write_bulk_staging(payload: str, *, strict: bool = False) -> NativeBulkStagingResult | None:
    """Run the native bulk staging writer for an already-normalized payload."""
    if not strict and os.environ.get("CODEBASE_GRAPH_NATIVE") != "1":
        return None

    command = _native_command(strict=strict)
    if command is None:
        if strict:
            raise NativeBulkStagingUnavailable("native bulk staging command is unavailable")
        return None

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
        if strict:
            stderr = getattr(exc, "stderr", "")
            raise NativeBulkStagingUnavailable(f"native bulk staging failed: {stderr or exc}") from exc
        return None

    return _decode_result(completed.stdout)


def _native_command(*, strict: bool) -> list[str] | None:
    configured = os.environ.get("CODEBASE_GRAPH_NATIVE_BULK_STAGING")
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


def _decode_result(output: str) -> NativeBulkStagingResult:
    copy_statements: list[str] = []
    node_rows: int | None = None
    edge_rows: int | None = None
    connector_rows: int | None = None

    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        match parts[0]:
            case "RESULT" if len(parts) == 4:
                node_rows = int(parts[1])
                edge_rows = int(parts[2])
                connector_rows = int(parts[3])
            case "COPY" if len(parts) == 2:
                copy_statements.append(bytes.fromhex(parts[1]).decode("utf-8"))
            case other:
                raise ValueError(f"Unknown native bulk staging record: {other}")

    if node_rows is None or edge_rows is None or connector_rows is None:
        raise ValueError("Native bulk staging did not report row counts")

    return NativeBulkStagingResult(
        copy_statements=copy_statements,
        node_rows=node_rows,
        edge_rows=edge_rows,
        connector_rows=connector_rows,
    )
