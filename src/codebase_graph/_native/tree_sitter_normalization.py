from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


class NativeTreeSitterNormalizationUnavailable(RuntimeError):
    """Raised when strict native tree-sitter normalization is requested but unavailable."""


def normalize_profiled_syntax(payload: str, *, strict: bool = False) -> str | None:
    """Run native profiled syntax normalization for an encoded tree payload."""
    if not strict and os.environ.get("CODEBASE_GRAPH_NATIVE") != "1":
        return None

    command = _native_command(strict=strict)
    if command is None:
        if strict:
            raise NativeTreeSitterNormalizationUnavailable("native tree-sitter normalization command is unavailable")
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
            message = f"native tree-sitter normalization failed: {stderr or exc}"
            raise NativeTreeSitterNormalizationUnavailable(message) from exc
        return None
    return completed.stdout


def _native_command(*, strict: bool) -> list[str] | None:
    configured = os.environ.get("CODEBASE_GRAPH_NATIVE_TREE_SITTER_NORMALIZER")
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
